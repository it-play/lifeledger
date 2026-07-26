import type { GameSnapshot } from '../../api/contracts.js';
import type { Store } from '../../lib/store/index.js';
import type { AppState } from '../state.js';

/** Applies snapshots in forward-only run-revision then state-revision order. */
export interface GameStateWriter {
  /** Returns false when the snapshot is older than the state already shown. */
  apply(snapshot: GameSnapshot): boolean;
  /** Applies an HTTP catch-up only when it is ahead of the current run position. */
  applyIfAhead(snapshot: GameSnapshot): boolean;
}

export interface GameStateWriterDeps {
  readonly store: Store<AppState>;
}
