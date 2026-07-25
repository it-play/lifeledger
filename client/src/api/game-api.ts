import type { Unsubscribe } from '../lib/core/index.js';
import type { HttpClient } from '../lib/http/index.js';
import type { SseClient, SseMessage } from '../lib/sse/index.js';
import { type GameSnapshot, GameSnapshotSchema, type Health, HealthSchema } from './contracts.js';
import { asDecoder } from './zod-adapters.js';

/**
 * 도메인 API. 화면은 HttpClient·SseClient 를 직접 만지지 않고 이 인터페이스만 쓴다.
 * 전송 수단(REST/SSE)이 바뀌어도 화면은 영향받지 않는다.
 */
export interface GameApi {
  health(): Promise<Health>;
  getSnapshot(): Promise<GameSnapshot>;
  /** 게임일을 days 만큼 전진시킨다. 전진 결과는 SSE 로도 흘러온다. */
  advance(days: number): Promise<GameSnapshot>;
  /** 틱 스트림 구독. 서버가 보낸 payload 를 계약으로 검증한 뒤 넘긴다. */
  onTick(handler: (snapshot: GameSnapshot) => void): Unsubscribe;
  connectStream(): void;
  disconnectStream(): void;
}

export interface GameApiDeps {
  readonly http: HttpClient;
  readonly stream: SseClient;
  /** 계약 위반 payload 를 어떻게 다룰지. 기본은 무시하고 로깅만. */
  readonly onInvalidTick?: (error: unknown, raw: SseMessage) => void;
}

const snapshotDecoder = asDecoder(GameSnapshotSchema);
const healthDecoder = asDecoder(HealthSchema);

export function createGameApi(deps: GameApiDeps): GameApi {
  const { http, stream } = deps;

  return {
    health: () => http.get('/api/health', healthDecoder),
    getSnapshot: () => http.get('/api/state', snapshotDecoder),
    advance: (days) => http.post('/api/advance', { days }, snapshotDecoder),

    onTick(handler) {
      return stream.on('tick', (message) => {
        try {
          handler(snapshotDecoder.parse(JSON.parse(message.data) as unknown));
        } catch (error) {
          deps.onInvalidTick?.(error, message);
        }
      });
    },

    connectStream: () => stream.connect(),
    disconnectStream: () => stream.close(),
  };
}
