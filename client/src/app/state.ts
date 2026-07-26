import type { GameSnapshot, Me } from '../api/contracts.js';
import type { SseStatus } from '../lib/sse/index.js';

/**
 * Login status. `unknown` means the server has not been asked yet, and that distinction
 * is what keeps the login screen from flashing while the check is in flight.
 */
export type AuthStatus = 'unknown' | 'anonymous' | 'authenticated';

/** Global app state. Subscribed to by path string, so the shape stays shallow and stable. */
export interface AppState {
  readonly auth: {
    readonly status: AuthStatus;
    readonly user: Me | undefined;
    /** Why the login round trip failed, as reported by the server. */
    readonly error: string | undefined;
  };
  readonly connection: {
    readonly status: SseStatus;
    readonly lastError: string | undefined;
  };
  readonly game: {
    readonly snapshot: GameSnapshot | undefined;
    readonly advancing: boolean;
    readonly ordering: boolean;
  };
}

export const initialState: AppState = {
  auth: { status: 'unknown', user: undefined, error: undefined },
  connection: { status: 'idle', lastError: undefined },
  game: { snapshot: undefined, advancing: false, ordering: false },
};

/** Common subscription paths as constants, so a typo cannot silently kill a subscription. */
export const paths = {
  authStatus: 'auth.status',
  authUser: 'auth.user',
  authError: 'auth.error',
  connectionStatus: 'connection.status',
  gameSnapshot: 'game.snapshot',
  gameCareer: 'game.snapshot.career',
  gameAdvancing: 'game.advancing',
  gameOrdering: 'game.ordering',
} as const;
