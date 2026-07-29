import type { EquityMarket, EquitySearchItem } from '../../api/contracts.js';
import type { GameApi } from '../../api/game-api.js';
import type { DisposableBag } from '../../lib/core/index.js';
import { el } from '../../lib/dom/index.js';
import { createHooks } from '../../lib/hooks/index.js';

export interface EquitySearchPanelDeps {
  readonly api: GameApi;
  readonly bag: DisposableBag;
}

interface ResultSlot {
  readonly root: HTMLLIElement;
  readonly title: HTMLElement;
  readonly metadata: HTMLElement;
}

const RESULT_CAPACITY = 20;
const MARKET_LABEL: Readonly<Record<EquityMarket, string>> = {
  kospi: 'KOSPI',
  kosdaq: 'KOSDAQ',
  konex: 'KONEX',
  other: '기타 시장',
};

export function createEquitySearchPanel(deps: EquitySearchPanelDeps): HTMLElement {
  const h = createHooks(deps.bag);
  const query = h.useSignal('');
  const market = h.useSignal<EquityMarket | ''>('');
  const request = h.useAsync((signal) => {
    const selectedMarket = market.peek();
    return deps.api.searchEquities(
      query.peek(),
      selectedMarket === '' ? undefined : selectedMarket,
      RESULT_CAPACITY,
      signal,
    );
  });
  const runSearch = h.useDebounced(() => request.run(), 300);

  const input = el('input', {
    id: 'equity-search-query',
    type: 'search',
    attrs: {
      autocomplete: 'off',
      maxlength: '80',
      placeholder: '종목명, 종목코드 또는 ISIN',
    },
  });
  const marketSelect = el('select', { id: 'equity-search-market' });
  marketSelect.append(
    el('option', { value: '' }, '전체 시장'),
    el('option', { value: 'kospi' }, 'KOSPI'),
    el('option', { value: 'kosdaq' }, 'KOSDAQ'),
    el('option', { value: 'konex' }, 'KONEX'),
    el('option', { value: 'other' }, '기타'),
  );
  const status = el('p', {
    class: 'equity-search__status',
    attrs: { role: 'status', 'aria-live': 'polite' },
  });
  const notice = el(
    'p',
    { class: 'equity-search__notice' },
    '실제 종목 식별자를 검색합니다. 게임 가격은 실제 시세가 아닌 시뮬레이션 값입니다.',
  );
  const resultList = el('ol', { class: 'equity-search__results' });
  const slots = Array.from({ length: RESULT_CAPACITY }, () => createResultSlot());
  resultList.append(...slots.map((slot) => slot.root));

  h.useEventListener(input, 'input', () => {
    const nextQuery = input.value.trim().replace(/\s+/g, ' ');
    query.set(nextQuery);
    if (nextQuery.length === 0) {
      runSearch.cancel();
      request.cancel();
      return;
    }
    runSearch();
  });
  h.useEventListener(marketSelect, 'change', () => {
    const selected = marketSelect.value;
    market.set(isEquityMarket(selected) ? selected : '');
    if (query.peek().length > 0) runSearch();
  });

  h.useEffect(() => {
    const currentQuery = query.get();
    const state = request.state.get();
    if (currentQuery.length === 0) {
      status.textContent = '검색어를 입력하면 현재 발행된 국내 상장 종목 카탈로그에서 찾습니다.';
      updateSlots(slots, []);
      return;
    }
    if (state.status === 'idle' || state.status === 'loading') {
      status.textContent = '종목을 찾는 중입니다…';
      return;
    }
    if (state.status === 'error') {
      status.textContent = '종목 카탈로그를 조회하지 못했습니다. 잠시 뒤 다시 시도해 주세요.';
      updateSlots(slots, []);
      return;
    }
    if (state.value.availability === 'notSynced') {
      status.textContent = '아직 발행된 종목 카탈로그가 없습니다. 시장데이터 동기화가 필요합니다.';
      notice.textContent = state.value.simulationNotice;
      updateSlots(slots, []);
      return;
    }
    status.textContent =
      state.value.items.length === 0
        ? '일치하는 종목이 없습니다.'
        : `${state.value.items.length}개 종목을 찾았습니다. 기준일 ${state.value.sourceAsOf ?? '-'}`;
    notice.textContent = state.value.simulationNotice;
    updateSlots(slots, state.value.items);
  });

  return el(
    'section',
    { class: 'equity-search' },
    el('h2', {}, '국내 상장 종목 찾기'),
    notice,
    el(
      'div',
      { class: 'equity-search__controls' },
      el('label', { attrs: { for: 'equity-search-query' } }, '검색어'),
      input,
      el('label', { attrs: { for: 'equity-search-market' } }, '시장'),
      marketSelect,
    ),
    status,
    resultList,
  );
}

function createResultSlot(): ResultSlot {
  const title = el('strong');
  const metadata = el('span', { class: 'equity-search__metadata' });
  const root = el(
    'li',
    { class: 'equity-search__result' },
    title,
    metadata,
    el('button', { type: 'button', disabled: true }, '거래 준비 중'),
  );
  root.hidden = true;
  return { root, title, metadata };
}

function updateSlots(slots: readonly ResultSlot[], items: readonly EquitySearchItem[]): void {
  for (const [index, slot] of slots.entries()) {
    const item = items[index];
    if (item === undefined) {
      slot.root.hidden = true;
      continue;
    }
    slot.title.textContent = `${item.displayName} (${item.shortCode})`;
    slot.metadata.textContent = `${MARKET_LABEL[item.market]} · ${item.corporationName} · ${item.isin}`;
    slot.root.hidden = false;
  }
}

function isEquityMarket(value: string): value is EquityMarket {
  return value === 'kospi' || value === 'kosdaq' || value === 'konex' || value === 'other';
}
