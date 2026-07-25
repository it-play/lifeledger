import type { Disposable, Unsubscribe } from '../core/types.js';

/**
 * A read-only reactive value. Calling `get()` inside a tracking context (computed or
 * effect) subscribes that context automatically, which is why dependencies are never
 * listed by hand.
 */
export interface Signal<T> {
  get(): T;
  /** Reads without tracking, so no dependency is recorded. */
  peek(): T;
  subscribe(listener: (value: T) => void): Unsubscribe;
}

export interface WritableSignal<T> extends Signal<T> {
  set(value: T): void;
  update(producer: (previous: T) => T): void;
}

/**
 * Cleanup an effect may return. Runs before the next execution and on dispose.
 *
 * biome-ignore lint/suspicious/noConfusingVoidType: deliberately mirrors the React
 * useEffect contract - return a cleanup function, or nothing.
 */
export type EffectCleanup = void | (() => void);

export interface EffectHandle extends Disposable {
  /** Re-runs immediately, regardless of dependency changes. */
  run(): void;
}

export interface SignalOptions<T> {
  /** Suppresses notification when values compare equal. Defaults to Object.is. */
  readonly equals?: (previous: T, next: T) => boolean;
}
