import type { Unsubscribe } from '../core/types.js';

/** `'portfolio.cash'` 처럼 점으로 구분한 상태 경로. */
export type StatePath = string;

/** 읽기 전용 관점. 화면은 이 인터페이스만 받는 것이 원칙이다. */
export interface ReadableStore<S> {
  getState(): S;
  select<T>(selector: (state: S) => T): T;
  /**
   * 지정한 경로들이 바뀔 때만 알림을 받는다.
   * 상위 경로를 구독하면 하위 변경도, 하위 경로를 구독하면 상위 교체도 알림 대상이다.
   */
  watch(paths: StatePath | readonly StatePath[], listener: () => void): Unsubscribe;
  /** 어떤 변경이든 받는다. 디버깅·영속화 용도. */
  watchAll(listener: (state: S, changed: readonly StatePath[]) => void): Unsubscribe;
}

export interface WritableStore<S> extends ReadableStore<S> {
  /** 새 상태를 반환하는 순수 함수를 넘긴다. 같은 참조를 반환하면 알림이 없다. */
  update(producer: (state: S) => S): void;
  /** 경로 하나만 바꾼다. 중간 경로는 얕은 복사로 갱신된다. */
  set(path: StatePath, value: unknown): void;
}

export type Store<S> = WritableStore<S>;
