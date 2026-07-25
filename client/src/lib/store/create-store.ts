import type { Unsubscribe } from '../core/types.js';
import { diffPaths, pathsIntersect, setAtPath } from './paths.js';
import type { StatePath, Store } from './types.js';

interface Watcher {
  readonly paths: readonly StatePath[];
  readonly listener: () => void;
}

export interface CreateStoreOptions {
  /**
   * Whether to coalesce notifications into a microtask. Defaults to true, collapsing
   * bursts (such as fast-forwarded days) into a single render.
   */
  readonly batch?: boolean;
}

/**
 * A store subscribed to by path.
 *
 * Stands in for a framework's fine-grained reactivity: it computes which paths changed
 * and wakes only the subscribers concerned, instead of re-rendering everything.
 */
export function createStore<S extends object>(
  initialState: S,
  options: CreateStoreOptions = {},
): Store<S> {
  const batch = options.batch ?? true;
  let state = initialState;

  const watchers = new Set<Watcher>();
  const allWatchers = new Set<(state: S, changed: readonly StatePath[]) => void>();

  let pendingPaths: Set<StatePath> | undefined;

  function flush(): void {
    const changed = pendingPaths;
    pendingPaths = undefined;
    if (changed === undefined || changed.size === 0) return;
    const changedList = [...changed];

    for (const watcher of [...watchers]) {
      const hit = watcher.paths.some((watched) =>
        changedList.some((path) => pathsIntersect(watched, path)),
      );
      if (hit) watcher.listener();
    }
    for (const listener of [...allWatchers]) listener(state, changedList);
  }

  function notify(changed: readonly StatePath[]): void {
    if (changed.length === 0) return;
    if (!batch) {
      pendingPaths = new Set(changed);
      flush();
      return;
    }
    if (pendingPaths === undefined) {
      pendingPaths = new Set(changed);
      queueMicrotask(flush);
      return;
    }
    for (const path of changed) pendingPaths.add(path);
  }

  function commit(next: S): void {
    if (next === state) return;
    const prev = state;
    state = next;
    notify(diffPaths(prev, next));
  }

  return {
    getState: () => state,

    select(selector) {
      return selector(state);
    },

    watch(paths, listener): Unsubscribe {
      const watcher: Watcher = {
        paths: typeof paths === 'string' ? [paths] : [...paths],
        listener,
      };
      watchers.add(watcher);
      return () => {
        watchers.delete(watcher);
      };
    },

    watchAll(listener): Unsubscribe {
      allWatchers.add(listener);
      return () => {
        allWatchers.delete(listener);
      };
    },

    update(producer) {
      commit(producer(state));
    },

    set(path, value) {
      commit(setAtPath(state, path, value));
    },
  };
}
