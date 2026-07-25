import type { GameSnapshot, Me } from '../api/contracts.js';
import type { SseStatus } from '../lib/sse/index.js';

/**
 * 로그인 상태. `unknown` 은 아직 서버에 물어보기 전이라는 뜻이다.
 * 이 구분이 있어야 확인하는 동안 로그인 화면이 잠깐 번쩍이지 않는다.
 */
export type AuthStatus = 'unknown' | 'anonymous' | 'authenticated';

/** 앱 전역 상태. 경로 문자열로 구독하므로 구조를 얕고 안정적으로 유지한다. */
export interface AppState {
  readonly auth: {
    readonly status: AuthStatus;
    readonly user: Me | undefined;
    /** 로그인 왕복이 실패했을 때 서버가 쿼리로 알려준 이유. */
    readonly error: string | undefined;
  };
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
  auth: { status: 'unknown', user: undefined, error: undefined },
  connection: { status: 'idle', lastError: undefined },
  game: { snapshot: undefined, advancing: false },
};

/** 자주 쓰는 구독 경로를 상수로 둔다 — 오타로 구독이 조용히 죽는 것을 막는다. */
export const paths = {
  authStatus: 'auth.status',
  authUser: 'auth.user',
  authError: 'auth.error',
  connectionStatus: 'connection.status',
  gameSnapshot: 'game.snapshot',
  gameAdvancing: 'game.advancing',
} as const;
