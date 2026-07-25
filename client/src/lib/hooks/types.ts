import type { Clock, Unsubscribe } from '../core/types.js';
import type { EffectCleanup, Signal, SignalOptions, WritableSignal } from '../reactive/types.js';
import type { ReadableStore, StatePath } from '../store/types.js';

/** Async task state: the minimum this project needs from a React Query-style tool. */
export type AsyncState<T> =
  | { readonly status: 'idle' }
  | { readonly status: 'loading' }
  | { readonly status: 'success'; readonly value: T }
  | { readonly status: 'error'; readonly error: unknown };

export interface AsyncHandle<T> {
  readonly state: Signal<AsyncState<T>>;
  /** Runs the task, cancelling any request already in flight. */
  run(): void;
  /** Cancels the in-flight request, leaving state untouched. */
  cancel(): void;
}

export interface DebouncedFn<A extends readonly unknown[]> {
  (...args: A): void;
  /** Cancels a pending call. */
  cancel(): void;
}

export interface HooksOptions {
  readonly clock?: Clock;
}

export interface StoreHookDeps<S> {
  readonly store: ReadableStore<S>;
}

/**
 * Hooks bound to a lifecycle.
 *
 * Same purpose as React hooks, different rules: with no render there is no call-order
 * constraint, so they may be called conditionally or in a loop. In exchange, every hook
 * registers what it creates with the `DisposableBag` given at construction, and disposing
 * the bag releases all of it.
 */
export interface Hooks {
  /** Local state; the React useState counterpart. */
  useSignal<T>(initial: T, options?: SignalOptions<T>): WritableSignal<T>;

  /** A derived value; useMemo, but tracked automatically with no dependency array. */
  useComputed<T>(compute: () => T, options?: SignalOptions<T>): Signal<T>;

  /** A side effect; useEffect, optionally returning a cleanup. */
  useEffect(effect: () => EffectCleanup): void;

  /** Calls back only on change, skipping the first run. No React equivalent, often needed. */
  useWatch<T>(source: Signal<T>, onChange: (value: T, previous: T) => void): void;

  /** Exposes one store path as a signal; the useSyncExternalStore counterpart. */
  useStoreValue<S, T>(
    store: ReadableStore<S>,
    path: StatePath,
    selector: (state: S) => T,
  ): Signal<T>;

  /** Runs on an interval; the bag handles teardown. */
  useInterval(handler: () => void, intervalMs: number): Unsubscribe;

  /** Runs once after a delay. */
  useTimeout(handler: () => void, delayMs: number): Unsubscribe;

  /** Keeps only the last call, as a search box needs. */
  useDebounced<A extends readonly unknown[]>(
    handler: (...args: A) => void,
    waitMs: number,
  ): DebouncedFn<A>;

  /** Passes at most one call per interval, damping renders during fast-forward. */
  useThrottled<A extends readonly unknown[]>(
    handler: (...args: A) => void,
    intervalMs: number,
  ): DebouncedFn<A>;

  /** Subscribes to a DOM event; the bag handles removal. */
  useEventListener<K extends keyof HTMLElementEventMap>(
    target: HTMLElement,
    type: K,
    handler: (event: HTMLElementEventMap[K]) => void,
  ): Unsubscribe;

  /** Subscribes to a window event. */
  useWindowListener<K extends keyof WindowEventMap>(
    type: K,
    handler: (event: WindowEventMap[K]) => void,
  ): Unsubscribe;

  /** Wraps an async call in a state machine, passing it an AbortSignal. */
  useAsync<T>(task: (signal: AbortSignal) => Promise<T>): AsyncHandle<T>;

  /** Document visibility, used to pause streams and timers on a hidden tab. */
  useVisibility(): Signal<boolean>;

  /** Whether a media query matches. */
  useMediaQuery(query: string): Signal<boolean>;

  /** A signal backed by localStorage; writes on change, falls back on a failed read. */
  useLocalStorage<T>(key: string, initial: T): WritableSignal<T>;

  /** Binds a text node to a signal - the most common render pattern here. */
  bindText(node: Node, compute: () => string): void;

  /** Binds an element attribute to a signal, boolean attributes such as `disabled` included. */
  bindAttribute(element: HTMLElement, name: string, compute: () => string | boolean): void;
}
