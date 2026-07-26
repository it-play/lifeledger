import type { GameSnapshot } from '../../api/contracts.js';
import { paths } from '../state.js';
import type { GameStateWriter, GameStateWriterDeps } from './types.js';

/** Creates the single gate for HTTP and SSE snapshot updates. */
export function createGameStateWriter(deps: GameStateWriterDeps): GameStateWriter {
  const apply = (snapshot: GameSnapshot, requireAhead: boolean): boolean => {
    const current = deps.store.getState().game.snapshot;
    if (current !== undefined) {
      const position = comparePosition(snapshot, current);
      if (position < 0 || (requireAhead && position === 0)) return false;
    }
    deps.store.set(paths.gameSnapshot, snapshot);
    return true;
  };

  return {
    apply: (snapshot) => apply(snapshot, false),
    applyIfAhead: (snapshot) => apply(snapshot, true),
  };
}

function comparePosition(left: GameSnapshot, right: GameSnapshot): number {
  if (left.runRevision !== right.runRevision) return left.runRevision - right.runRevision;
  return left.stateRevision - right.stateRevision;
}
