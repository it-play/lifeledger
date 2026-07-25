import { createSystemClock } from '../core/clock.js';
import { createNullLogger } from '../core/logger.js';
import { type CancelTimer, err, ok, type Result } from '../core/types.js';
import { createEventStreamParser } from './parser.js';
import { createDefaultRetryDecider, createExponentialBackoff } from './policy.js';
import type {
  DisconnectReason,
  SseClient,
  SseClientOptions,
  SseMessage,
  SseStatus,
} from './types.js';

const CONTENT_TYPE = 'text/event-stream';

/**
 * fetch 스트리밍 기반 SSE 클라이언트.
 *
 * EventSource 를 쓰지 않는 이유:
 *  - 요청 헤더를 붙일 수 없다 (인증 토큰을 쿠키 외 방법으로 못 보냄)
 *  - 재연결 지연을 제어할 수 없다 (백오프·지터 불가)
 *  - 상태 전이(connecting/open/reconnecting)를 세밀하게 관찰하기 어렵다
 *  - POST 로 열 수 없다
 *
 * 대신 스펙의 파싱·재연결 의미론은 그대로 지킨다 (parser.ts).
 */
export function createSseClient(options: SseClientOptions): SseClient {
  const clock = options.clock ?? createSystemClock();
  const logger = (options.logger ?? createNullLogger()).child('sse');
  const backoff = options.backoff ?? createExponentialBackoff();
  const retryDecider = options.retryDecider ?? createDefaultRetryDecider();
  const fetchImpl = options.fetchImpl ?? globalThis.fetch.bind(globalThis);

  const parser = createEventStreamParser();
  const typedHandlers = new Map<string, Set<(m: SseMessage) => void>>();
  const anyHandlers = new Set<(m: SseMessage) => void>();
  const statusHandlers = new Set<(s: SseStatus) => void>();

  let status: SseStatus = 'idle';
  let controller: AbortController | undefined;
  let cancelRetryTimer: CancelTimer | undefined;
  let attempt = 0;
  let disposed = false;

  function setStatus(next: SseStatus): void {
    if (status === next) return;
    status = next;
    for (const handler of [...statusHandlers]) handler(next);
  }

  function emit(message: SseMessage): void {
    for (const handler of [...anyHandlers]) handler(message);
    const handlers = typedHandlers.get(message.type);
    if (handlers === undefined) return;
    for (const handler of [...handlers]) handler(message);
  }

  function scheduleReconnect(reason: DisconnectReason): void {
    if (disposed) return;
    if (!retryDecider.shouldRetry(reason)) {
      logger.log('info', '재연결하지 않고 종료', { reason: reason.kind });
      setStatus('closed');
      return;
    }
    attempt += 1;
    const delay = backoff.delayMs(attempt, parser.serverRetryMs);
    logger.log('debug', '재연결 예약', { attempt, delay, reason: reason.kind });
    setStatus('reconnecting');
    cancelRetryTimer = clock.setTimeout(() => {
      cancelRetryTimer = undefined;
      void openConnection();
    }, delay);
  }

  function buildHeaders(): HeadersInit {
    const headers: Record<string, string> = {
      ...(options.headers ?? {}),
      Accept: CONTENT_TYPE,
      'Cache-Control': 'no-store',
    };
    // 스펙: 마지막 이벤트 id 가 있으면 재연결 시 서버에 알린다 → 서버가 이어서 보낼 수 있다
    if (parser.lastEventId !== '') headers['Last-Event-ID'] = parser.lastEventId;
    return headers;
  }

  /** 연결 수립 단계. 스트림을 얻거나, 실패 이유를 돌려준다. */
  async function requestStream(
    signal: AbortSignal,
  ): Promise<Result<ReadableStream<Uint8Array>, DisconnectReason>> {
    let response: Response;
    try {
      response = await fetchImpl(options.url, {
        method: 'GET',
        headers: buildHeaders(),
        signal,
        ...(options.credentials === undefined ? {} : { credentials: options.credentials }),
      });
    } catch (error) {
      return err({ kind: 'network', error });
    }

    if (!response.ok) {
      logger.log('warn', 'SSE 응답 상태 이상', { status: response.status });
      return err({ kind: 'http', status: response.status });
    }

    const contentType = response.headers.get('content-type');
    if (contentType === null || !contentType.includes(CONTENT_TYPE)) {
      logger.log('error', 'SSE Content-Type 불일치', { contentType });
      return err({ kind: 'bad-content-type', contentType });
    }

    if (response.body === null) return err({ kind: 'stream-ended' });
    return ok(response.body);
  }

  /** 스트림 소비 단계. 끊긴 이유를 돌려준다. */
  async function pumpStream(body: ReadableStream<Uint8Array>): Promise<DisconnectReason> {
    const reader = body.getReader();
    // stream: true — 멀티바이트 문자가 청크 경계에 걸려도 깨지지 않게 디코더가 상태를 유지한다
    const decoder = new TextDecoder('utf-8');
    try {
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        if (value === undefined) continue;
        for (const message of parser.push(decoder.decode(value, { stream: true }))) emit(message);
      }
      // 서버가 스트림을 정상 종료했다 — SSE 에서는 다시 붙는 것이 기본 동작이다
      return { kind: 'stream-ended' };
    } catch (error) {
      return { kind: 'network', error };
    } finally {
      parser.end();
      reader.releaseLock();
    }
  }

  async function openConnection(): Promise<void> {
    if (disposed) return;
    const ac = new AbortController();
    controller = ac;
    setStatus(attempt === 0 ? 'connecting' : 'reconnecting');

    const stream = await requestStream(ac.signal);
    if (ac.signal.aborted) return; // 호출자가 닫은 것
    if (!stream.ok) {
      scheduleReconnect(stream.error);
      return;
    }

    attempt = 0; // 성공적으로 열렸으므로 백오프를 초기화한다
    setStatus('open');
    logger.log('info', 'SSE 연결됨', { url: options.url });

    const reason = await pumpStream(stream.value);
    if (ac.signal.aborted) return;
    scheduleReconnect(reason);
  }

  function stop(): void {
    cancelRetryTimer?.();
    cancelRetryTimer = undefined;
    controller?.abort();
    controller = undefined;
  }

  return {
    get status() {
      return status;
    },
    get lastEventId() {
      return parser.lastEventId;
    },
    on(type, handler) {
      const set = typedHandlers.get(type) ?? new Set();
      set.add(handler);
      typedHandlers.set(type, set);
      return () => {
        set.delete(handler);
        if (set.size === 0) typedHandlers.delete(type);
      };
    },
    onAny(handler) {
      anyHandlers.add(handler);
      return () => {
        anyHandlers.delete(handler);
      };
    },
    onStatusChange(handler) {
      statusHandlers.add(handler);
      return () => {
        statusHandlers.delete(handler);
      };
    },
    connect() {
      if (disposed) return;
      if (status === 'connecting' || status === 'open' || status === 'reconnecting') return;
      attempt = 0;
      void openConnection();
    },
    close() {
      if (disposed) return;
      stop();
      setStatus('closed');
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      stop();
      setStatus('closed');
      typedHandlers.clear();
      anyHandlers.clear();
      statusHandlers.clear();
    },
  };
}
