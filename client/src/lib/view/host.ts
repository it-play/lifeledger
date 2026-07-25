import { createDisposableBag } from '../core/disposable.js';
import type { View, ViewContext, ViewFactory, ViewHost } from './types.js';

/**
 * 한 번에 화면 하나. 전환할 때 이전 화면의 unmount 와 구독 정리를 강제한다.
 * 이 규칙이 프레임워크 없이도 누수를 막는 핵심이다.
 */
export function createViewHost(host: HTMLElement): ViewHost {
  let active: { view: View; bag: ReturnType<typeof createDisposableBag> } | undefined;
  /** 비동기 mount 중에 다시 전환될 수 있으므로 세대를 센다. */
  let generation = 0;

  function teardown(): void {
    if (active === undefined) return;
    const { view, bag } = active;
    active = undefined;
    try {
      view.unmount();
    } finally {
      bag.dispose();
    }
    host.replaceChildren();
  }

  return {
    async render(factory: ViewFactory, ctx) {
      const myGeneration = ++generation;
      teardown();

      const bag = createDisposableBag();
      const view = factory();
      const fullContext: ViewContext = { ...ctx, bag };
      active = { view, bag };

      await view.mount(host, fullContext);

      // mount 를 기다리는 동안 다른 화면으로 전환됐다면 이 화면은 버린다
      if (myGeneration !== generation) {
        try {
          view.unmount();
        } finally {
          bag.dispose();
        }
      }
    },
    clear: teardown,
  };
}
