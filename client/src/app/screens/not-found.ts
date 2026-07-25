import { el } from '../../lib/dom/index.js';
import type { View, ViewFactory } from '../../lib/view/index.js';

/**
 * Not found, written as a ledger entry: the columns a Korean ledger actually uses
 * (적요 / 금액), with the figure in the red reserved for a deficit.
 */
export function createNotFoundView(): ViewFactory {
  return (): View => {
    let root: HTMLElement | undefined;

    return {
      mount(host) {
        root = el(
          'section',
          { class: 'not-found' },
          el('p', { class: 'not-found__mark' }, 'LifeLedger'),
          el(
            'div',
            { class: 'not-found__ledger' },
            el(
              'div',
              { class: 'not-found__row not-found__row--head' },
              el('span', {}, '적요'),
              el('span', {}, '금액'),
            ),
            el(
              'div',
              { class: 'not-found__row not-found__row--entry' },
              el('span', { class: 'not-found__label' }, '기록되지 않은 페이지'),
              el('span', { class: 'not-found__amount' }, '404'),
            ),
          ),
          el(
            'p',
            { class: 'not-found__note' },
            '이 주소는 장부에 없습니다. 주소를 다시 확인하거나 대시보드에서 이어가세요.',
          ),
          el(
            'a',
            { class: 'not-found__back', href: '/', dataset: { link: '' } },
            '대시보드로 가기',
          ),
        );
        host.replaceChildren(root);
      },

      unmount() {
        root?.remove();
        root = undefined;
      },
    };
  };
}
