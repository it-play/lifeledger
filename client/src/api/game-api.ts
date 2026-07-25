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
 * The domain API. Screens use this interface rather than HttpClient or SseClient
 * directly, so changing transport leaves them untouched.
 */
/**
 * Raised when the server rejects a contradictory combination (§3.5). Carries per-field
 * messages ready for the form, so no screen needs to know an HTTP status code.
 */
export class CharacterRejectedError extends Error {
  constructor(readonly fieldErrors: Readonly<Record<string, string>>) {
    super('시작 조건이 서로 맞지 않습니다');
    this.name = 'CharacterRejectedError';
  }
}

export interface GameApi {
  health(): Promise<Health>;
  /** Starting presets (§3.3). */
  listPresets(): Promise<readonly Preset[]>;
  /**
   * Creates a character and starts the game.
   * A failed combination check throws {@link CharacterRejectedError}.
   */
  createCharacter(draft: CharacterDraft): Promise<GameSnapshot>;
  getSnapshot(): Promise<GameSnapshot>;
  /** Advances the game day. The result also arrives over SSE. */
  advance(days: number): Promise<GameSnapshot>;
  /** Subscribes to ticks, validating each payload against the contract first. */
  onTick(handler: (snapshot: GameSnapshot) => void): Unsubscribe;
  connectStream(): void;
  disconnectStream(): void;
}

export interface GameApiDeps {
  readonly http: HttpClient;
  readonly stream: SseClient;
  /** What to do with a payload that breaks the contract. Logged and dropped by default. */
  readonly onInvalidTick?: (error: unknown, raw: SseMessage) => void;
}

const snapshotDecoder = asDecoder(GameSnapshotSchema);
const healthDecoder = asDecoder(HealthSchema);
const presetListDecoder = asDecoder(PresetListSchema);

/** Turns a 422 body into a field-to-message map, or gives up if the shape is unfamiliar. */
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
