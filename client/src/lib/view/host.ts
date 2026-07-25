import { createDisposableBag } from '../core/disposable.js';
import type { View, ViewContext, ViewFactory, ViewHost } from './types.js';

/**
 * One screen at a time. Switching forces the previous screen to unmount and release its
 * subscriptions, which is what prevents leaks without a framework.
 */
export function createViewHost(host: HTMLElement): ViewHost {
  let active: { view: View; bag: ReturnType<typeof createDisposableBag> } | undefined;
  /** Generation counter, since another switch can land during an async mount. */
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

      // Discard this screen if another was selected while its mount was pending
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
