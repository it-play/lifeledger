import type { Disposable } from '../core/types.js';

export type RouteParams = Readonly<Record<string, string>>;

export interface RouteMatch {
  readonly pattern: string;
  readonly params: RouteParams;
  readonly query: URLSearchParams;
}

export interface RouteDefinition<H> {
  /** `'/game/:id'` 처럼 `:name` 세그먼트를 쓴다. */
  readonly pattern: string;
  readonly handler: H;
}

export interface NavigateOptions {
  readonly replace?: boolean;
}

/**
 * History API 라우터. 화면 전환의 유일한 진입점이며,
 * 어떤 화면을 어떻게 그릴지는 모른다 (그건 RouterOptions.onNavigate 가 정한다).
 */
export interface Router extends Disposable {
  start(): void;
  navigate(to: string, options?: NavigateOptions): void;
  readonly current: RouteMatch | undefined;
}

export interface RouterOptions<H> {
  readonly routes: readonly RouteDefinition<H>[];
  /** 매칭 실패 시 사용할 핸들러. */
  readonly fallback: H;
  readonly onNavigate: (handler: H, match: RouteMatch) => void | Promise<void>;
}
