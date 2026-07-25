import type { Unsubscribe } from '../lib/core/index.js';
import { type HttpClient, HttpError } from '../lib/http/index.js';
import type { SseClient, SseMessage } from '../lib/sse/index.js';
import {
  type CharacterDraft,
  type GameSnapshot,
  GameSnapshotSchema,
  type Health,
  HealthSchema,
  type Preset,
  PresetListSchema,
  ValidationFailureSchema,
} from './contracts.js';
import { asDecoder } from './zod-adapters.js';

/**
 * 도메인 API. 화면은 HttpClient·SseClient 를 직접 만지지 않고 이 인터페이스만 쓴다.
 * 전송 수단(REST/SSE)이 바뀌어도 화면은 영향받지 않는다.
 */
/**
 * 서버가 조합 모순(§3.5)을 거부했을 때. 필드별 메시지를 그대로 폼에 꽂을 수 있다.
 * 화면이 HTTP 상태 코드를 알 필요가 없게 여기서 도메인 오류로 바꿔준다.
 */
export class CharacterRejectedError extends Error {
  constructor(readonly fieldErrors: Readonly<Record<string, string>>) {
    super('시작 조건이 서로 맞지 않습니다');
    this.name = 'CharacterRejectedError';
  }
}

export interface GameApi {
  health(): Promise<Health>;
  /** 시작 프리셋 목록 (§3.3). */
  listPresets(): Promise<readonly Preset[]>;
  /**
   * 캐릭터를 만들고 게임을 시작한다.
   * 조합 검증 실패는 {@link CharacterRejectedError} 로 던진다.
   */
  createCharacter(draft: CharacterDraft): Promise<GameSnapshot>;
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
const presetListDecoder = asDecoder(PresetListSchema);

/** 서버의 422 본문을 필드 → 메시지 맵으로 바꾼다. 형태가 다르면 그대로 다시 던진다. */
function toFieldErrors(error: unknown): Record<string, string> | undefined {
  if (!(error instanceof HttpError) || error.status !== 422) return undefined;
  const parsed = ValidationFailureSchema.safeParse(error.body);
  if (!parsed.success) return undefined;
  const fieldErrors: Record<string, string> = {};
  for (const item of parsed.data.errors) {
    if (fieldErrors[item.field] === undefined) fieldErrors[item.field] = item.message;
  }
  return fieldErrors;
}

export function createGameApi(deps: GameApiDeps): GameApi {
  const { http, stream } = deps;

  return {
    health: () => http.get('/api/health', healthDecoder),
    listPresets: () => http.get('/api/presets', presetListDecoder),

    async createCharacter(draft) {
      try {
        return await http.post('/api/characters', draft, snapshotDecoder);
      } catch (error) {
        const fieldErrors = toFieldErrors(error);
        if (fieldErrors === undefined) throw error;
        throw new CharacterRejectedError(fieldErrors);
      }
    },

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
