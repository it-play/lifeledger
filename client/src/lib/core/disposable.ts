import type { Disposable, Unsubscribe } from './types.js';

/**
 * Collects teardown functions and releases them together, so a view or client never
 * leaks its own subscriptions.
 */
export interface DisposableBag extends Disposable {
  add(cleanup: Unsubscribe | Disposable): void;
  readonly size: number;
  readonly disposed: boolean;
}

export function createDisposableBag(): DisposableBag {
  const cleanups = new Set<Unsubscribe>();
  let disposed = false;

  return {
    add(cleanup) {
      const fn = typeof cleanup === 'function' ? cleanup : () => cleanup.dispose();
      // A resource registered after disposal is released immediately
      if (disposed) {
        fn();
        return;
      }
      cleanups.add(fn);
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      // Reverse registration order, in case resources depend on each other
      for (const fn of [...cleanups].reverse()) {
        try {
          fn();
        } catch {
          // One failure must not strand the rest
        }
      }
      cleanups.clear();
    },
    get size() {
      return cleanups.size;
    },
    get disposed() {
      return disposed;
    },
  };
}
