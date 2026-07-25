import type { Disposable, Unsubscribe } from '../core/types.js';

/**
 * 읽기 전용 반응 값. `get()` 을 어떤 추적 문맥(computed·effect) 안에서 호출하면
 * 그 문맥이 이 값의 구독자로 자동 등록된다 — 의존성을 손으로 나열하지 않는 이유다.
 */
export interface Signal<T> {
  get(): T;
  /** 추적 없이 현재 값만 본다 (의존성으로 등록되지 않는다). */
  peek(): T;
  subscribe(listener: (value: T) => void): Unsubscribe;
}

export interface WritableSignal<T> extends Signal<T> {
  set(value: T): void;
  update(producer: (previous: T) => T): void;
}

/**
 * effect 가 돌려줄 수 있는 정리 함수. 다음 실행 전과 dispose 시 호출된다.
 *
 * biome-ignore lint/suspicious/noConfusingVoidType: React useEffect 와 같은 계약을 의도적으로 유지한다
 * (정리 함수를 반환하거나 아무것도 반환하지 않는다).
 */
export type EffectCleanup = void | (() => void);

export interface EffectHandle extends Disposable {
  /** 의존성 변화와 무관하게 즉시 다시 실행한다. */
  run(): void;
}

export interface SignalOptions<T> {
  /** 같은 값으로 판단되면 알리지 않는다. 기본은 Object.is. */
  readonly equals?: (previous: T, next: T) => boolean;
}
