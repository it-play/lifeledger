import type { DisposableBag } from '../core/disposable.js';
import type { RouteParams } from '../router/types.js';

/**
 * The contract for one screen, standing in for a framework's component lifecycle.
 * Every subscription made in `mount` belongs in ctx.bag, and `unmount` releases just that.
 */
export interface View {
  mount(host: HTMLElement, ctx: ViewContext): void | Promise<void>;
  unmount(): void;
}

export type ViewFactory = () => View;

/** A screen's only route to the outside world; what is absent here is unavailable to it. */
export interface ViewContext {
  readonly params: RouteParams;
  readonly query: URLSearchParams;
  readonly bag: DisposableBag;
  readonly navigate: (to: string) => void;
}

/** Hosts a single screen, tearing down the previous one on every switch. */
export interface ViewHost {
  render(factory: ViewFactory, ctx: Omit<ViewContext, 'bag'>): Promise<void>;
  clear(): void;
}
