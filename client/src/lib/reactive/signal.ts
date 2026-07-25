import type {
  EffectCleanup,
  EffectHandle,
  Signal,
  SignalOptions,
  WritableSignal,
} from './types.js';

/**
 * The fine-grained reactivity core. Where React hooks lean on a render cycle, this
 * remembers the context that read a value.
 *
 * With no framework in the project, this file fills that role. It stays deliberately
 * small: no scheduler, no components.
 */

interface Subscriber {
  /** What to re-run when a dependency changes. */
  readonly execute: () => void;
  /** Back-references to the signal subscriber sets holding this one, kept for removal. */
  readonly dependencies: Set<Set<Subscriber>>;
}

/** The active tracking context, set only while a computed or effect runs. */
let activeSubscriber: Subscriber | undefined;

/** Batch nesting depth; 0 means run immediately. */
let batchDepth = 0;
const pending = new Set<Subscriber>();

function link(subscribers: Set<Subscriber>): void {
  const subscriber = activeSubscriber;
  if (subscriber === undefined) return;
  subscribers.add(subscriber);
  subscriber.dependencies.add(subscribers);
}

function unlink(subscriber: Subscriber): void {
  for (const subscribers of subscriber.dependencies) subscribers.delete(subscriber);
  subscriber.dependencies.clear();
}

/** Runs work inside a tracking context, collecting dependencies afresh each time. */
function runTracked(subscriber: Subscriber, work: () => void): void {
  unlink(subscriber);
  const previous = activeSubscriber;
  activeSubscriber = subscriber;
  try {
    work();
  } finally {
    activeSubscriber = previous;
  }
}

function schedule(subscribers: Iterable<Subscriber>): void {
  if (batchDepth > 0) {
    for (const subscriber of subscribers) pending.add(subscriber);
    return;
  }
  // Iterate a copy: the subscriber set can change while notifying
  for (const subscriber of [...subscribers]) subscriber.execute();
}

/** Groups changes so subscribers wake once. */
export function batch<T>(work: () => T): T {
  batchDepth += 1;
  try {
    return work();
  } finally {
    batchDepth -= 1;
    if (batchDepth === 0) {
      const queued = [...pending];
      pending.clear();
      for (const subscriber of queued) subscriber.execute();
    }
  }
}

/** Reads without tracking, for values an effect must not depend on. */
export function untracked<T>(work: () => T): T {
  const previous = activeSubscriber;
  activeSubscriber = undefined;
  try {
    return work();
  } finally {
    activeSubscriber = previous;
  }
}

export function createSignal<T>(initial: T, options: SignalOptions<T> = {}): WritableSignal<T> {
  const equals = options.equals ?? Object.is;
  const subscribers = new Set<Subscriber>();
  const listeners = new Set<(value: T) => void>();
  let value = initial;

  const notify = (): void => {
    schedule(subscribers);
    for (const listener of [...listeners]) listener(value);
  };

  return {
    get() {
      link(subscribers);
      return value;
    },
    peek: () => value,
    set(next) {
      if (equals(value, next)) return;
      value = next;
      notify();
    },
    update(producer) {
      const next = producer(value);
      if (equals(value, next)) return;
      value = next;
      notify();
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}

/**
 * A derived value: recomputed when a dependency changes, but notifying its own
 * subscribers only when the result differs. Aggregates such as net worth therefore do
 * not shake the screen on every intermediate change.
 */
export function createComputed<T>(compute: () => T, options: SignalOptions<T> = {}): Signal<T> {
  const equals = options.equals ?? Object.is;
  const output = createSignal<T>(undefined as T, { equals });
  const subscriber: Subscriber = {
    execute: () => runTracked(subscriber, () => output.set(compute())),
    dependencies: new Set(),
  };
  subscriber.execute();

  return {
    get: () => output.get(),
    peek: () => output.peek(),
    subscribe: (listener) => output.subscribe(listener),
  };
}

/**
 * A side effect. Runs once, remembers the signals it read, and re-runs when they change.
 * A returned cleanup runs before the next execution and on dispose, as in React.
 */
export function createEffect(effect: () => EffectCleanup): EffectHandle {
  let cleanup: (() => void) | undefined;
  let disposed = false;

  const runCleanup = (): void => {
    const current = cleanup;
    cleanup = undefined;
    if (current === undefined) return;
    try {
      current();
    } catch {
      // A failed cleanup must not block the next run
    }
  };

  const subscriber: Subscriber = {
    execute: () => {
      if (disposed) return;
      runCleanup();
      runTracked(subscriber, () => {
        const result = effect();
        cleanup = typeof result === 'function' ? result : undefined;
      });
    },
    dependencies: new Set(),
  };

  subscriber.execute();

  return {
    run: () => subscriber.execute(),
    dispose() {
      if (disposed) return;
      disposed = true;
      runCleanup();
      unlink(subscriber);
    },
  };
}
