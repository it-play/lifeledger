import { el, on } from '../dom/index.js';
import { createEffect } from '../reactive/signal.js';
import type { Toast, ToastHost, ToastHostOptions, ToastQueue } from './types.js';

/**
 * Draws a queue into a live region.
 *
 * Nodes are keyed by toast id and reused, so a message that is still on screen keeps its
 * element - otherwise a new toast would restart every neighbour's enter animation.
 */
export function createToastHost(queue: ToastQueue, options: ToastHostOptions = {}): ToastHost {
  const dismissLabel = options.dismissLabel ?? '닫기';
  const root = el('div', { class: 'toasts', attrs: { role: 'region', 'aria-live': 'polite' } });
  const nodes = new Map<number, HTMLElement>();

  function build(toast: Toast): HTMLElement {
    const close = el(
      'button',
      { type: 'button', class: 'toast__close', attrs: { 'aria-label': dismissLabel } },
      '×',
    );
    on(close, 'click', () => queue.dismiss(toast.id));

    return el(
      'div',
      { class: `toast toast--${toast.tone}`, dataset: { toastId: String(toast.id) } },
      el('span', { class: 'toast__text' }, toast.text),
      close,
    );
  }

  const effect = createEffect(() => {
    const items = queue.items.get();
    const live = new Set(items.map((toast) => toast.id));

    for (const [id, node] of nodes) {
      if (live.has(id)) continue;
      node.remove();
      nodes.delete(id);
    }

    for (const toast of items) {
      let node = nodes.get(toast.id);
      if (node === undefined) {
        node = build(toast);
        nodes.set(toast.id, node);
      }
      // Re-appending an existing child moves it, which is how order is kept
      root.appendChild(node);
    }
  });

  return {
    element: root,
    dispose() {
      effect.dispose();
      nodes.clear();
      root.remove();
    },
  };
}
