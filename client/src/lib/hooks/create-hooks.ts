import { createSystemClock } from '../core/clock.js';
import type { DisposableBag } from '../core/disposable.js';
import type { Clock, Unsubscribe } from '../core/types.js';
import { createComputed, createEffect, createSignal, untracked } from '../reactive/signal.js';
import type { Signal, WritableSignal } from '../reactive/types.js';
import type { ReadableStore, StatePath } from '../store/types.js';
import type { AsyncHandle, AsyncState, DebouncedFn, Hooks, HooksOptions } from './types.js';

/**
 * Binds a set of hooks to a lifecycle bag. A screen calls
 * `const h = createHooks(ctx.bag)` once in `mount` and uses hooks only below that.
 */
export function createHooks(bag: DisposableBag, options: HooksOptions = {}): Hooks {
  const clock: Clock = options.clock ?? createSystemClock();

  const useSignal: Hooks['useSignal'] = (initial, signalOptions) =>
    createSignal(initial, signalOptions ?? {});

  const useComputed: Hooks['useComputed'] = (compute, signalOptions) => {
    const signal = createComputed(compute, signalOptions ?? {});
    return signal;
  };

  const useEffect: Hooks['useEffect'] = (effect) => {
    bag.add(createEffect(effect));
  };

  const useWatch: Hooks['useWatch'] = (source, onChange) => {
    let previous = source.peek();
    let first = true;
    bag.add(
      createEffect(() => {
        const value = source.get();
        if (first) {
          first = false;
          return;
        }
        const last = previous;
        previous = value;
        untracked(() => onChange(value, last));
      }),
    );
  };

  function useStoreValue<S, T>(
    store: ReadableStore<S>,
    path: StatePath,
    selector: (state: S) => T,
  ): Signal<T> {
    const signal = createSignal(selector(store.getState()));
    bag.add(store.watch(path, () => signal.set(selector(store.getState()))));
    return signal;
  }

  const useInterval: Hooks['useInterval'] = (handler, intervalMs) => {
    // Clock offers one-shot timers only, so re-arm to build an interval
    let cancel: (() => void) | undefined;
    let stopped = false;
    const tick = (): void => {
      if (stopped) return;
      handler();
      if (!stopped) cancel = clock.setTimeout(tick, intervalMs);
    };
    cancel = clock.setTimeout(tick, intervalMs);
    const stop: Unsubscribe = () => {
      stopped = true;
      cancel?.();
      cancel = undefined;
    };
    bag.add(stop);
    return stop;
  };

  const useTimeout: Hooks['useTimeout'] = (handler, delayMs) => {
    const cancel = clock.setTimeout(handler, delayMs);
    bag.add(cancel);
    return cancel;
  };

  function useDebounced<A extends readonly unknown[]>(
    handler: (...args: A) => void,
    waitMs: number,
  ): DebouncedFn<A> {
    let cancelPending: (() => void) | undefined;
    const debounced = ((...args: A) => {
      cancelPending?.();
      cancelPending = clock.setTimeout(() => {
        cancelPending = undefined;
        handler(...args);
      }, waitMs);
    }) as DebouncedFn<A>;
    debounced.cancel = () => {
      cancelPending?.();
      cancelPending = undefined;
    };
    bag.add(debounced.cancel);
    return debounced;
  }

  function useThrottled<A extends readonly unknown[]>(
    handler: (...args: A) => void,
    intervalMs: number,
  ): DebouncedFn<A> {
    let lastRun = Number.NEGATIVE_INFINITY;
    let cancelTrailing: (() => void) | undefined;

    const throttled = ((...args: A) => {
      const now = clock.now();
      const elapsed = now - lastRun;
      if (elapsed >= intervalMs) {
        lastRun = now;
        handler(...args);
        return;
      }
      // Calls arriving within the window collapse into one trailing run
      cancelTrailing?.();
      cancelTrailing = clock.setTimeout(() => {
        cancelTrailing = undefined;
        lastRun = clock.now();
        handler(...args);
      }, intervalMs - elapsed);
    }) as DebouncedFn<A>;

    throttled.cancel = () => {
      cancelTrailing?.();
      cancelTrailing = undefined;
    };
    bag.add(throttled.cancel);
    return throttled;
  }

  const useEventListener: Hooks['useEventListener'] = (target, type, handler) => {
    const listener = handler as EventListener;
    target.addEventListener(type, listener);
    const off: Unsubscribe = () => target.removeEventListener(type, listener);
    bag.add(off);
    return off;
  };

  const useWindowListener: Hooks['useWindowListener'] = (type, handler) => {
    const listener = handler as EventListener;
    addEventListener(type, listener);
    const off: Unsubscribe = () => removeEventListener(type, listener);
    bag.add(off);
    return off;
  };

  function useAsync<T>(task: (signal: AbortSignal) => Promise<T>): AsyncHandle<T> {
    const state = createSignal<AsyncState<T>>({ status: 'idle' });
    let controller: AbortController | undefined;

    const cancel = (): void => {
      controller?.abort();
      controller = undefined;
    };

    const run = (): void => {
      cancel();
      const ac = new AbortController();
      controller = ac;
      state.set({ status: 'loading' });
      void task(ac.signal)
        .then((value) => {
          if (ac.signal.aborted) return;
          state.set({ status: 'success', value });
        })
        .catch((error: unknown) => {
          if (ac.signal.aborted) return;
          state.set({ status: 'error', error });
        });
    };

    bag.add(cancel);
    return { state, run, cancel };
  }

  const useVisibility: Hooks['useVisibility'] = () => {
    const visible = createSignal(document.visibilityState === 'visible');
    const listener = (): void => visible.set(document.visibilityState === 'visible');
    document.addEventListener('visibilitychange', listener);
    bag.add(() => document.removeEventListener('visibilitychange', listener));
    return visible;
  };

  const useMediaQuery: Hooks['useMediaQuery'] = (query) => {
    const list = matchMedia(query);
    const matches = createSignal(list.matches);
    const listener = (event: MediaQueryListEvent): void => matches.set(event.matches);
    list.addEventListener('change', listener);
    bag.add(() => list.removeEventListener('change', listener));
    return matches;
  };

  function useLocalStorage<T>(key: string, initial: T): WritableSignal<T> {
    const signal = createSignal<T>(readStored(key) ?? initial);
    bag.add(
      createEffect(() => {
        const value = signal.get();
        try {
          localStorage.setItem(key, JSON.stringify(value));
        } catch {
          // A failed write (quota, private mode) must not break the feature
        }
      }),
    );
    return signal;
  }

  const bindText: Hooks['bindText'] = (node, compute) => {
    let last: string | undefined;
    bag.add(
      createEffect(() => {
        const text = compute();
        if (text === last) return;
        last = text;
        node.textContent = text;
      }),
    );
  };

  const bindAttribute: Hooks['bindAttribute'] = (element, name, compute) => {
    bag.add(
      createEffect(() => {
        const value = compute();
        if (value === false || value === undefined) element.removeAttribute(name);
        else element.setAttribute(name, value === true ? '' : value);
      }),
    );
  };

  return {
    useSignal,
    useComputed,
    useEffect,
    useWatch,
    useStoreValue,
    useInterval,
    useTimeout,
    useDebounced,
    useThrottled,
    useEventListener,
    useWindowListener,
    useAsync,
    useVisibility,
    useMediaQuery,
    useLocalStorage,
    bindText,
    bindAttribute,
  };
}

function readStored<T>(key: string): T | undefined {
  try {
    const raw = localStorage.getItem(key);
    return raw === null ? undefined : (JSON.parse(raw) as T);
  } catch {
    return undefined;
  }
}
