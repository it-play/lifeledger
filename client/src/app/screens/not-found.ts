import { el } from '../../lib/dom/index.js';
import type { View, ViewFactory } from '../../lib/view/index.js';

export function createNotFoundView(): ViewFactory {
  return (): View => ({
    mount(host) {
      host.replaceChildren(
        el(
          'section',
          { class: 'not-found' },
          el('h1', {}, '없는 화면입니다'),
          el('a', { href: '/', dataset: { link: '' } }, '대시보드로'),
        ),
      );
    },
    unmount() {},
  });
}
