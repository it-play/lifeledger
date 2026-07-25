import type { NavigateOptions, RouteMatch, Router, RouterOptions } from './types.js';

interface CompiledRoute<H> {
  readonly pattern: string;
  readonly regex: RegExp;
  readonly keys: readonly string[];
  readonly handler: H;
}

/** `'/game/:id'` → `/^\/game\/([^/]+)$/` + `['id']` */
function compile<H>(pattern: string, handler: H): CompiledRoute<H> {
  const keys: string[] = [];
  const source = pattern
    .split('/')
    .map((segment) => {
      if (!segment.startsWith(':')) return escapeRegex(segment);
      keys.push(segment.slice(1));
      return '([^/]+)';
    })
    .join('/');
  return { pattern, regex: new RegExp(`^${source}/?$`), keys, handler };
}

const escapeRegex = (s: string): string => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

export function createRouter<H>(options: RouterOptions<H>): Router {
  const compiled = options.routes.map((route) => compile(route.pattern, route.handler));
  let current: RouteMatch | undefined;
  let started = false;

  function resolve(pathname: string): { handler: H; match: RouteMatch } {
    for (const route of compiled) {
      const result = route.regex.exec(pathname);
      if (result === null) continue;
      const params: Record<string, string> = {};
      route.keys.forEach((key, index) => {
        const value = result[index + 1];
        if (value !== undefined) params[key] = decodeURIComponent(value);
      });
      return {
        handler: route.handler,
        match: { pattern: route.pattern, params, query: new URLSearchParams(location.search) },
      };
    }
    return {
      handler: options.fallback,
      match: { pattern: '*', params: {}, query: new URLSearchParams(location.search) },
    };
  }

  async function dispatch(): Promise<void> {
    const { handler, match } = resolve(location.pathname);
    current = match;
    await options.onNavigate(handler, match);
  }

  function onPopState(): void {
    void dispatch();
  }

  /** 앱 내부 링크(`<a data-link href="/x">`)를 라우터가 가로챈다. */
  function onClick(event: MouseEvent): void {
    if (event.defaultPrevented || event.button !== 0) return;
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    const target = event.target;
    if (!(target instanceof Element)) return;
    const anchor = target.closest('a[data-link]');
    if (!(anchor instanceof HTMLAnchorElement)) return;
    if (anchor.origin !== location.origin) return;
    event.preventDefault();
    navigate(anchor.pathname + anchor.search);
  }

  function navigate(to: string, navigateOptions: NavigateOptions = {}): void {
    if (navigateOptions.replace === true) history.replaceState(null, '', to);
    else history.pushState(null, '', to);
    void dispatch();
  }

  return {
    start() {
      if (started) return;
      started = true;
      addEventListener('popstate', onPopState);
      addEventListener('click', onClick);
      void dispatch();
    },
    navigate,
    get current() {
      return current;
    },
    dispose() {
      removeEventListener('popstate', onPopState);
      removeEventListener('click', onClick);
      started = false;
      current = undefined;
    },
  };
}
