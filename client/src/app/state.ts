import type { GameSnapshot } from '../api/contracts.js';
import type { SseStatus } from '../lib/sse/index.js';

/** 앱 전역 상태. 경로 문자열로 구독하므로 구조를 얕고 안정적으로 유지한다. */
export interface AppState {
  readonly connection: {
    readonly status: SseStatus;
    readonly lastError: string | undefined;
  };
  readonly game: {
    readonly snapshot: GameSnapshot | undefined;
    readonly advancing: boolean;
  };
}

export const initialState: AppState = {
  connection: { status: 'idle', lastError: undefined },
  game: { snapshot: undefined, advancing: false },
};

/** 자주 쓰는 구독 경로를 상수로 둔다 — 오타로 구독이 조용히 죽는 것을 막는다. */
export const paths = {
  connectionStatus: 'connection.status',
  gameSnapshot: 'game.snapshot',
  gameAdvancing: 'game.advancing',
} as const;
