import type { Disposable } from '../core/types.js';

export type RouteParams = Readonly<Record<string, string>>;

export interface RouteMatch {
  readonly pattern: string;
  readonly params: RouteParams;
  readonly query: URLSearchParams;
}

export interface RouteDefinition<H> {
  /** Uses `:name` segments, as in `'/game/:id'`. */
  readonly pattern: string;
  readonly handler: H;
}

export interface NavigateOptions {
  readonly replace?: boolean;
}

/**
 * A History API router: the single entry point for screen changes, and deliberately
 * ignorant of which screen to draw - that is RouterOptions.onNavigate's decision.
 */
export interface Router extends Disposable {
  start(): void;
  navigate(to: string, options?: NavigateOptions): void;
  readonly current: RouteMatch | undefined;
}

export interface RouterOptions<H> {
  readonly routes: readonly RouteDefinition<H>[];
  /** Handler used when nothing matches. */
  readonly fallback: H;
  readonly onNavigate: (handler: H, match: RouteMatch) => void | Promise<void>;
}
