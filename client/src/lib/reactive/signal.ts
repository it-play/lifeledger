import type {
  EffectCleanup,
  EffectHandle,
  Signal,
  SignalOptions,
  WritableSignal,
} from './types.js';

/**
 * 세밀한 반응성 코어. React 의 훅이 렌더 사이클에 의존해 하던 일을,
 * 여기서는 "값을 읽은 문맥을 기억한다" 는 방식으로 대신한다.
 *
 * 프레임워크를 쓰지 않기로 했으므로 이 파일이 그 자리를 메운다.
 * 규모는 의도적으로 작게 유지한다 — 스케줄러도, 컴포넌트도 없다.
 */

interface Subscriber {
  /** 의존성이 바뀌었을 때 다시 실행할 작업. */
  readonly execute: () => void;
  /** 이 구독자가 등록된 신호들의 구독자 집합 (해제를 위해 역참조를 들고 있는다). */
  readonly dependencies: Set<Set<Subscriber>>;
}

/** 현재 추적 중인 문맥. computed·effect 실행 중에만 설정된다. */
let activeSubscriber: Subscriber | undefined;

/** batch 중첩 깊이. 0 이면 즉시 실행. */
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

/** 추적 문맥을 세우고 작업을 실행한다. 실행마다 의존성을 새로 모은다. */
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
  // 실행 중 구독자 집합이 바뀔 수 있으므로 복사해서 돈다
  for (const subscriber of [...subscribers]) subscriber.execute();
}

/** 여러 변경을 묶어 구독자를 한 번만 깨운다. */
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

/** 추적 없이 읽는다. effect 안에서 의존성으로 잡히면 안 되는 값에 쓴다. */
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
 * 파생 값. 의존성이 바뀌면 다시 계산하고, 결과가 달라졌을 때만 자기 구독자에게 알린다.
 * (그래서 `순자산` 처럼 여러 값을 합치는 계산이 중간값 변화로 화면을 흔들지 않는다.)
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
 * 부수효과. 처음 한 번 실행하며 읽은 신호를 의존성으로 기억하고, 바뀌면 다시 실행한다.
 * 정리 함수를 반환하면 다음 실행 전과 dispose 시에 호출된다 (React useEffect 와 같은 계약).
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
      // 정리 실패가 다음 실행을 막지 않게 한다
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
