import type { Clock, Unsubscribe } from '../core/types.js';
import type { EffectCleanup, Signal, SignalOptions, WritableSignal } from '../reactive/types.js';
import type { ReadableStore, StatePath } from '../store/types.js';

/** 비동기 작업의 상태. React Query 류가 주는 것 중 이 프로젝트에 필요한 최소치. */
export type AsyncState<T> =
  | { readonly status: 'idle' }
  | { readonly status: 'loading' }
  | { readonly status: 'success'; readonly value: T }
  | { readonly status: 'error'; readonly error: unknown };

export interface AsyncHandle<T> {
  readonly state: Signal<AsyncState<T>>;
  /** 실행한다. 이미 실행 중이면 이전 요청을 취소한다. */
  run(): void;
  /** 진행 중인 요청만 취소한다 (상태는 그대로). */
  cancel(): void;
}

export interface DebouncedFn<A extends readonly unknown[]> {
  (...args: A): void;
  /** 대기 중인 호출을 취소한다. */
  cancel(): void;
}

export interface HooksOptions {
  readonly clock?: Clock;
}

export interface StoreHookDeps<S> {
  readonly store: ReadableStore<S>;
}

/**
 * 생명주기에 묶인 훅 모음.
 *
 * React 훅과 목적은 같지만 호출 규칙이 다르다 — 렌더가 없으므로 **호출 순서 규칙이 없고**,
 * 조건문·루프 안에서 불러도 된다. 대신 모든 훅은 자신이 만든 자원을 이 훅 집합을 만들 때
 * 넘긴 `DisposableBag` 에 등록하며, bag 이 정리되면 전부 해제된다.
 */
export interface Hooks {
  /** 지역 상태. React useState 대응. */
  useSignal<T>(initial: T, options?: SignalOptions<T>): WritableSignal<T>;

  /** 파생 값. React useMemo 대응 (의존성 배열 없이 자동 추적). */
  useComputed<T>(compute: () => T, options?: SignalOptions<T>): Signal<T>;

  /** 부수효과. React useEffect 대응. 정리 함수를 반환할 수 있다. */
  useEffect(effect: () => EffectCleanup): void;

  /** 값이 바뀔 때만 콜백. 첫 실행은 건너뛴다 (React 에는 없지만 자주 필요하다). */
  useWatch<T>(source: Signal<T>, onChange: (value: T, previous: T) => void): void;

  /** 스토어의 한 경로를 신호로 노출한다. React useSyncExternalStore 대응. */
  useStoreValue<S, T>(
    store: ReadableStore<S>,
    path: StatePath,
    selector: (state: S) => T,
  ): Signal<T>;

  /** 주기 실행. 정리는 bag 이 맡는다. */
  useInterval(handler: () => void, intervalMs: number): Unsubscribe;

  /** 지연 실행. */
  useTimeout(handler: () => void, delayMs: number): Unsubscribe;

  /** 마지막 호출만 살린다 (검색 입력 등). */
  useDebounced<A extends readonly unknown[]>(
    handler: (...args: A) => void,
    waitMs: number,
  ): DebouncedFn<A>;

  /** 일정 간격에 한 번만 통과시킨다 (배속 진행 중 렌더 억제 등). */
  useThrottled<A extends readonly unknown[]>(
    handler: (...args: A) => void,
    intervalMs: number,
  ): DebouncedFn<A>;

  /** DOM 이벤트 구독. 해제는 bag 이 맡는다. */
  useEventListener<K extends keyof HTMLElementEventMap>(
    target: HTMLElement,
    type: K,
    handler: (event: HTMLElementEventMap[K]) => void,
  ): Unsubscribe;

  /** window 이벤트 구독. */
  useWindowListener<K extends keyof WindowEventMap>(
    type: K,
    handler: (event: WindowEventMap[K]) => void,
  ): Unsubscribe;

  /** 비동기 호출을 상태 기계로 감싼다. AbortSignal 을 넘겨준다. */
  useAsync<T>(task: (signal: AbortSignal) => Promise<T>): AsyncHandle<T>;

  /** 문서 가시성. 탭이 숨으면 스트림·타이머를 멈추는 데 쓴다. */
  useVisibility(): Signal<boolean>;

  /** 미디어 쿼리 일치 여부. */
  useMediaQuery(query: string): Signal<boolean>;

  /** localStorage 에 붙은 신호. 값이 바뀌면 저장하고, 읽기 실패 시 기본값을 쓴다. */
  useLocalStorage<T>(key: string, initial: T): WritableSignal<T>;

  /** 텍스트 노드를 신호에 묶는다 — 이 프로젝트에서 가장 자주 쓰는 렌더 패턴. */
  bindText(node: Node, compute: () => string): void;

  /** 요소 속성을 신호에 묶는다 (`disabled` 같은 boolean 속성 포함). */
  bindAttribute(element: HTMLElement, name: string, compute: () => string | boolean): void;
}
