import type { Unsubscribe } from '../core/types.js';

/** A dot-separated state path such as `'portfolio.cash'`. */
export type StatePath = string;

/** The read-only view. Screens receive this interface, not the writable one. */
export interface ReadableStore<S> {
  getState(): S;
  select<T>(selector: (state: S) => T): T;
  /**
   * Notifies only when the given paths change. Subscribing to a parent also catches
   * changes below it, and subscribing to a child also catches a parent being replaced.
   */
  watch(paths: StatePath | readonly StatePath[], listener: () => void): Unsubscribe;
  /** Every change, for debugging and persistence. */
  watchAll(listener: (state: S, changed: readonly StatePath[]) => void): Unsubscribe;
}

export interface WritableStore<S> extends ReadableStore<S> {
  /** Takes a pure function returning the next state. Same reference means no notification. */
  update(producer: (state: S) => S): void;
  /** Changes a single path, shallow-copying the objects along the way. */
  set(path: StatePath, value: unknown): void;
}

export type Store<S> = WritableStore<S>;
