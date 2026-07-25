import type { Unsubscribe } from '../core/types.js';
import { diffPaths, pathsIntersect, setAtPath } from './paths.js';
import type { StatePath, Store } from './types.js';

interface Watcher {
  readonly paths: readonly StatePath[];
  readonly listener: () => void;
}

export interface CreateStoreOptions {
  /**
   * 알림을 마이크로태스크로 모을지 여부. 기본 true.
   * 배속 진행처럼 한 틱에 여러 번 갱신될 때 렌더를 1회로 합친다.
   */
  readonly batch?: boolean;
}

/**
 * 경로 단위 구독 스토어.
 *
 * 프레임워크의 세밀한 반응성을 대신하는 최소 장치다. 전체 리렌더를 피하기 위해
 * "무엇이 바뀌었는지"를 경로로 계산해 관련 구독자만 깨운다.
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
