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
 * An SSE client built on streaming fetch.
 *
 * EventSource is not used because it cannot set request headers, cannot control the
 * reconnect delay (so no backoff or jitter), makes state transitions hard to observe,
 * and cannot open with POST.
 *
 * The spec's parsing and reconnection semantics are still honoured (see parser.ts).
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
      logger.log('info', 'closing without reconnect', { reason: reason.kind });
      setStatus('closed');
      return;
    }
    attempt += 1;
    const delay = backoff.delayMs(attempt, parser.serverRetryMs);
    logger.log('debug', 'reconnect scheduled', { attempt, delay, reason: reason.kind });
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
    // Per spec, a known last event id is sent on reconnect so the server can resume
    if (parser.lastEventId !== '') headers['Last-Event-ID'] = parser.lastEventId;
    return headers;
  }

  /** Establishes the connection, yielding either a stream or a reason it failed. */
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
      logger.log('warn', 'unexpected SSE response status', { status: response.status });
      return err({ kind: 'http', status: response.status });
    }

    const contentType = response.headers.get('content-type');
    if (contentType === null || !contentType.includes(CONTENT_TYPE)) {
      logger.log('error', 'unexpected SSE Content-Type', { contentType });
      return err({ kind: 'bad-content-type', contentType });
    }

    if (response.body === null) return err({ kind: 'stream-ended' });
    return ok(response.body);
  }

  /** Consumes the stream, returning why it ended. */
  async function pumpStream(body: ReadableStream<Uint8Array>): Promise<DisconnectReason> {
    const reader = body.getReader();
    // stream: true keeps decoder state, so a multi-byte character split across chunks survives
    const decoder = new TextDecoder('utf-8');
    try {
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        if (value === undefined) continue;
        for (const message of parser.push(decoder.decode(value, { stream: true }))) emit(message);
      }
      // The server closed the stream cleanly, and reconnecting is the SSE default
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
    if (ac.signal.aborted) return; // closed by the caller
    if (!stream.ok) {
      scheduleReconnect(stream.error);
      return;
    }

    attempt = 0; // opened, so reset the backoff
    setStatus('open');
    logger.log('info', 'SSE connected', { url: options.url });

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
