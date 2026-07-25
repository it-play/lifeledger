import type { DisposableBag } from '../core/disposable.js';
import type { RouteParams } from '../router/types.js';

/**
 * 화면 하나의 계약. 프레임워크의 컴포넌트 생명주기를 대신한다.
 * mount 에서 만든 모든 구독은 ctx.bag 에 넣어야 하며, unmount 는 그것만 정리한다.
 */
export interface View {
  mount(host: HTMLElement, ctx: ViewContext): void | Promise<void>;
  unmount(): void;
}

export type ViewFactory = () => View;

/** 화면이 외부 세계에 접근하는 유일한 통로. 여기 없는 것은 화면이 쓸 수 없다. */
export interface ViewContext {
  readonly params: RouteParams;
  readonly query: URLSearchParams;
  readonly bag: DisposableBag;
  readonly navigate: (to: string) => void;
}

/** 화면을 하나만 띄우는 호스트. 전환 시 이전 화면을 확실히 정리한다. */
export interface ViewHost {
  render(factory: ViewFactory, ctx: Omit<ViewContext, 'bag'>): Promise<void>;
  clear(): void;
}
