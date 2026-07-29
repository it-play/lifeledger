import type {
  PublicSaveDetail,
  PublicSaveRankingItem,
  PublicSaveRankingPage,
  PublicSaveRankingQuery,
} from '../../api/contracts.js';
import type { GameApi } from '../../api/game-api.js';
import { el } from '../../lib/dom/index.js';
import { type AsyncState, createHooks } from '../../lib/hooks/index.js';
import type { View, ViewFactory } from '../../lib/view/index.js';

const PAGE_SIZE = 20;

type RankingMode = 'overall' | 'gameDay' | 'age' | 'completed';

interface RangeOption {
  readonly label: string;
  readonly from: number;
  readonly to?: number;
}

const GAME_DAY_RANGES: readonly RangeOption[] = [
  { label: '첫 1년 · 0~364일', from: 0, to: 364 },
  { label: '1~4년 · 365~1,824일', from: 365, to: 1_824 },
  { label: '5~9년 · 1,825~3,649일', from: 1_825, to: 3_649 },
  { label: '10~19년 · 3,650~7,299일', from: 3_650, to: 7_299 },
  { label: '20년 이상 · 7,300일+', from: 7_300 },
];

const AGE_RANGES: readonly RangeOption[] = [
  { label: '20대 이하', from: 0, to: 29 },
  { label: '30대', from: 30, to: 39 },
  { label: '40대', from: 40, to: 49 },
  { label: '50대', from: 50, to: 59 },
  { label: '60대 이상', from: 60 },
];

const MODE_LABEL: Readonly<Record<RankingMode, string>> = {
  overall: '전체',
  gameDay: '게임일 구간',
  age: '연령 구간',
  completed: '완주',
};

export interface RankingsDeps {
  readonly api: Pick<GameApi, 'listPublicSaveRankings' | 'getPublicSaveDetail'>;
}

interface RankingRow {
  readonly element: HTMLTableRowElement;
  setItem(
    item: PublicSaveRankingItem | undefined,
    metric: PublicSaveRankingPage['rankingMetric'],
  ): void;
}

export function createRankingsView(deps: RankingsDeps): ViewFactory {
  return (): View => ({
    mount(host, ctx) {
      const h = createHooks(ctx.bag);
      const mode = h.useSignal<RankingMode>('overall');
      const page = h.useSignal(0);
      const gameDayRangeIndex = h.useSignal(0);
      const ageRangeIndex = h.useSignal(0);
      const selectedSaveUid = h.useSignal<string | undefined>(undefined);
      const rankingRequest = h.useAsync((signal) =>
        deps.api.listPublicSaveRankings(
          buildQuery(mode.peek(), page.peek(), gameDayRangeIndex.peek(), ageRangeIndex.peek()),
          signal,
        ),
      );
      const detailRequest = h.useAsync((signal) => {
        const saveUid = selectedSaveUid.peek();
        if (saveUid === undefined) return Promise.reject(new Error('save UID is unavailable'));
        return deps.api.getPublicSaveDetail(saveUid, signal);
      });

      const status = el('p', {
        class: 'rankings__status',
        attrs: { role: 'status', 'aria-live': 'polite' },
      });
      const metric = el('p', { class: 'rankings__metric' });
      const pageStatus = el('span');
      const previousButton = el('button', { type: 'button' }, '이전');
      const nextButton = el('button', { type: 'button' }, '다음');
      const gameDayRange = createRangeSelect(GAME_DAY_RANGES, '게임일 구간');
      const ageRange = createRangeSelect(AGE_RANGES, '연령 구간');
      const gameDayControl = el(
        'label',
        { class: 'rankings__range' },
        el('span', {}, '게임일 구간'),
        gameDayRange,
      );
      const ageControl = el(
        'label',
        { class: 'rankings__range' },
        el('span', {}, '연령 구간'),
        ageRange,
      );
      const body = el('tbody');
      const detailDialog = createDetailDialog();
      const rows = Array.from({ length: PAGE_SIZE }, () =>
        createRankingRow(h, (item) => {
          selectedSaveUid.set(item.saveUid);
          detailDialog.setLoading(item.characterName);
          if (!detailDialog.element.open) detailDialog.element.showModal();
          detailRequest.run();
        }),
      );
      body.append(...rows.map((row) => row.element));

      const tabs = (Object.keys(MODE_LABEL) as readonly RankingMode[]).map((value) => {
        const button = el(
          'button',
          {
            type: 'button',
            class: 'rankings__tab',
            attrs: { role: 'tab', 'aria-selected': 'false' },
          },
          MODE_LABEL[value],
        );
        h.bindAttribute(button, 'aria-selected', () => String(mode.get() === value));
        h.useEventListener(button, 'click', () => {
          if (mode.peek() === value) return;
          mode.set(value);
          page.set(0);
          rankingRequest.run();
        });
        return button;
      });

      h.bindAttribute(gameDayControl, 'hidden', () => mode.get() !== 'gameDay');
      h.bindAttribute(ageControl, 'hidden', () => mode.get() !== 'age');
      h.useEventListener(gameDayRange, 'change', () => {
        gameDayRangeIndex.set(gameDayRange.selectedIndex);
        page.set(0);
        rankingRequest.run();
      });
      h.useEventListener(ageRange, 'change', () => {
        ageRangeIndex.set(ageRange.selectedIndex);
        page.set(0);
        rankingRequest.run();
      });
      h.useEventListener(previousButton, 'click', () => {
        if (page.peek() === 0) return;
        page.update((value) => value - 1);
        rankingRequest.run();
      });
      h.useEventListener(nextButton, 'click', () => {
        page.update((value) => value + 1);
        rankingRequest.run();
      });
      h.bindAttribute(previousButton, 'disabled', () => {
        return page.get() === 0 || rankingRequest.state.get().status === 'loading';
      });
      h.bindAttribute(nextButton, 'disabled', () => {
        const state = rankingRequest.state.get();
        if (state.status !== 'success') return true;
        return (state.value.page + 1) * state.value.limit >= state.value.total;
      });
      h.bindText(pageStatus, () => {
        const state = rankingRequest.state.get();
        if (state.status !== 'success' || state.value.total === 0) return '0 / 0';
        return `${state.value.page + 1} / ${Math.ceil(state.value.total / state.value.limit)}`;
      });
      h.bindText(status, () => rankingStatus(rankingRequest.state.get()));
      h.bindText(metric, () => rankingMetricText(rankingRequest.state.get()));

      h.useEffect(() => {
        const state = rankingRequest.state.get();
        const items = state.status === 'success' ? state.value.items : [];
        const rankingMetric =
          state.status === 'success' ? state.value.rankingMetric : 'currentNetWorth';
        rows.forEach((row, index) => {
          row.setItem(items[index], rankingMetric);
        });
      });
      h.useEffect(() => {
        const state = detailRequest.state.get();
        if (state.status === 'success') detailDialog.setDetail(state.value);
        else if (state.status === 'error') detailDialog.setError();
      });
      h.useEventListener(detailDialog.closeButton, 'click', () => detailDialog.element.close());
      h.useEventListener(detailDialog.element, 'click', (event) => {
        if (event.target === detailDialog.element) detailDialog.element.close();
      });

      host.replaceChildren(
        el(
          'main',
          { class: 'rankings' },
          el(
            'header',
            { class: 'rankings__header' },
            el('p', { class: 'rankings__eyebrow' }, 'PUBLIC SAVE LEDGER'),
            el('h1', {}, '다른 삶의 장부'),
            el(
              'p',
              { class: 'rankings__intro' },
              '완주 여부와 상관없이 지금까지 쌓인 세이브를 펼쳐 봅니다. 전체와 구간 순위는 현재 순자산, 완주는 세후 청산값을 사용합니다.',
            ),
            el(
              'a',
              { href: '/', dataset: { link: '' }, class: 'rankings__back' },
              '내 장부로 돌아가기',
            ),
          ),
          el('div', { class: 'rankings__tabs', attrs: { role: 'tablist' } }, ...tabs),
          el('div', { class: 'rankings__filters' }, gameDayControl, ageControl, metric),
          status,
          el(
            'div',
            { class: 'rankings__table-wrap' },
            el(
              'table',
              { class: 'rankings__table' },
              el(
                'thead',
                {},
                el(
                  'tr',
                  {},
                  el('th', { attrs: { scope: 'col' } }, '순위'),
                  el('th', { attrs: { scope: 'col' } }, '주인공'),
                  el('th', { attrs: { scope: 'col' } }, '상태'),
                  el('th', { attrs: { scope: 'col' } }, '현재 시점'),
                  el('th', { attrs: { scope: 'col' } }, '비교 금액'),
                ),
              ),
              body,
            ),
          ),
          el(
            'nav',
            { class: 'rankings__pagination', attrs: { 'aria-label': '순위 페이지' } },
            previousButton,
            pageStatus,
            nextButton,
          ),
          detailDialog.element,
        ),
      );

      rankingRequest.run();
    },
    unmount() {},
  });
}

function buildQuery(
  mode: RankingMode,
  page: number,
  gameDayRangeIndex: number,
  ageRangeIndex: number,
): PublicSaveRankingQuery {
  const base = { page, limit: PAGE_SIZE };
  if (mode === 'completed') return { ...base, status: 'completed' };
  if (mode === 'gameDay') {
    const range = GAME_DAY_RANGES[gameDayRangeIndex] ?? GAME_DAY_RANGES[0];
    if (range === undefined) return base;
    return { ...base, gameDayFrom: range.from, gameDayTo: range.to };
  }
  if (mode === 'age') {
    const range = AGE_RANGES[ageRangeIndex] ?? AGE_RANGES[0];
    if (range === undefined) return base;
    return { ...base, ageFrom: range.from, ageTo: range.to };
  }
  return base;
}

function createRangeSelect(ranges: readonly RangeOption[], label: string): HTMLSelectElement {
  const select = el('select', { attrs: { 'aria-label': label } });
  for (const range of ranges)
    select.appendChild(el('option', { value: String(range.from) }, range.label));
  return select;
}

function createRankingRow(
  h: ReturnType<typeof createHooks>,
  onOpen: (item: PublicSaveRankingItem) => void,
): RankingRow {
  let current: PublicSaveRankingItem | undefined;
  const rank = el('td', { class: 'rankings__rank' });
  const nameButton = el('button', { type: 'button', class: 'rankings__name' });
  const status = el('td');
  const progress = el('td', { class: 'rankings__number' });
  const value = el('td', { class: 'rankings__money' });
  const element = el('tr', {}, rank, el('td', {}, nameButton), status, progress, value);
  h.useEventListener(nameButton, 'click', () => {
    if (current !== undefined) onOpen(current);
  });

  return {
    element,
    setItem(item, metric) {
      current = item;
      element.hidden = item === undefined;
      nameButton.disabled = item === undefined;
      if (item === undefined) return;
      rank.textContent = String(item.rank).padStart(2, '0');
      nameButton.textContent = item.characterName;
      status.textContent = progressStatusLabel(item.progressStatus);
      progress.textContent = `${item.ageYears}세 · ${item.gameDay.toLocaleString('ko-KR')}일`;
      const amount = metric === 'afterTaxNetWorth' ? item.afterTaxNetWorthKrw : item.netWorthKrw;
      value.textContent = amount === null ? '—' : formatWon(amount);
    },
  };
}

interface DetailDialog {
  readonly element: HTMLDialogElement;
  readonly closeButton: HTMLButtonElement;
  setLoading(characterName: string): void;
  setDetail(detail: PublicSaveDetail): void;
  setError(): void;
}

function createDetailDialog(): DetailDialog {
  const title = el('h2');
  const subtitle = el('p', { class: 'rankings-modal__subtitle' });
  const status = el('p', { class: 'rankings-modal__status', attrs: { role: 'status' } });
  const closeButton = el(
    'button',
    { type: 'button', class: 'rankings-modal__close', attrs: { autofocus: '' } },
    '닫기',
  );
  const fields = {
    progress: el('dd'),
    profile: el('dd'),
    career: el('dd'),
    household: el('dd'),
    residence: el('dd'),
    netWorth: el('dd'),
    liquidCash: el('dd'),
    savings: el('dd'),
    investments: el('dd'),
    property: el('dd'),
    debt: el('dd'),
    finalization: el('dd'),
  };
  const list = el(
    'dl',
    { class: 'rankings-modal__ledger' },
    ...detailRows([
      ['진행', fields.progress],
      ['출발 조건', fields.profile],
      ['직업', fields.career],
      ['가구', fields.household],
      ['주거·법인', fields.residence],
      ['현재 순자산', fields.netWorth],
      ['현금', fields.liquidCash],
      ['예금·보증금', fields.savings],
      ['투자자산', fields.investments],
      ['부동산', fields.property],
      ['부채', fields.debt],
      ['완주 결산', fields.finalization],
    ]),
  );
  const element = el(
    'dialog',
    { class: 'rankings-modal', attrs: { 'aria-labelledby': 'ranking-detail-title' } },
    el('div', { class: 'rankings-modal__head' }, el('div', {}, title, subtitle), closeButton),
    status,
    list,
    el(
      'p',
      { class: 'rankings-modal__note' },
      '개별 계좌·종목·대출·계약과 계정 정보는 공개하지 않습니다.',
    ),
  );
  title.id = 'ranking-detail-title';

  return {
    element,
    closeButton,
    setLoading(characterName) {
      title.textContent = characterName;
      subtitle.textContent = '세이브 공개 장부를 읽는 중';
      status.textContent = '최신 상태를 불러오고 있습니다.';
      list.hidden = true;
    },
    setDetail(detail) {
      title.textContent = detail.characterName;
      subtitle.textContent = `SAVE ${detail.saveUid.slice(0, 8).toUpperCase()}`;
      status.textContent = '';
      list.hidden = false;
      fields.progress.textContent = `${progressStatusLabel(detail.progressStatus)} · ${detail.ageYears}세 · ${detail.gameDay.toLocaleString('ko-KR')}일차`;
      fields.profile.textContent = `${educationLabel(detail.education)} · ${regionLabel(detail.region)}`;
      fields.career.textContent = careerText(detail);
      fields.household.textContent =
        detail.householdMemberCount === null
          ? '가구 정보 없음'
          : `${detail.householdMemberCount.toLocaleString('ko-KR')}명`;
      fields.residence.textContent = residenceText(detail);
      fields.netWorth.textContent = formatWon(detail.netWorthKrw);
      fields.liquidCash.textContent = `${formatWon(detail.liquidCashKrw)} · 지갑 ${formatWon(detail.walletCashKrw)}`;
      fields.savings.textContent = `예금 ${formatWon(detail.cashProductPrincipalKrw)} · 임차보증금 ${formatWon(detail.leaseDepositKrw)}`;
      fields.investments.textContent = formatWon(detail.investmentValueKrw);
      fields.property.textContent = `${formatWon(detail.propertyValueKrw)} · ${detail.activePropertyCount.toLocaleString('ko-KR')}채`;
      fields.debt.textContent = formatWon(detail.debtKrw);
      fields.finalization.textContent =
        detail.afterTaxNetWorthKrw === null
          ? '아직 완주 결산 없음'
          : formatWon(detail.afterTaxNetWorthKrw);
    },
    setError() {
      subtitle.textContent = '세이브 공개 장부';
      status.textContent = '상세 정보를 불러오지 못했습니다. 잠시 뒤 다시 시도해 주세요.';
      list.hidden = true;
    },
  };
}

function detailRows(entries: readonly (readonly [string, HTMLElement])[]): HTMLElement[] {
  return entries.flatMap(([label, value]) => [el('dt', {}, label), value]);
}

function rankingStatus(state: AsyncState<PublicSaveRankingPage>): string {
  if (state.status === 'loading' || state.status === 'idle')
    return '세이브 장부를 모으는 중입니다.';
  if (state.status === 'error') return '순위를 불러오지 못했습니다. 잠시 뒤 다시 시도해 주세요.';
  if (state.value.total === 0) return '이 조건에 해당하는 공개 세이브가 없습니다.';
  return `${state.value.total.toLocaleString('ko-KR')}개 세이브 중 ${state.value.items.length.toLocaleString('ko-KR')}개를 표시합니다.`;
}

function rankingMetricText(state: AsyncState<PublicSaveRankingPage>): string {
  return state.status === 'success' && state.value.rankingMetric === 'afterTaxNetWorth'
    ? '정렬 기준 · 세후 청산 순자산'
    : '정렬 기준 · 현재 순자산';
}

function progressStatusLabel(status: PublicSaveRankingItem['progressStatus']): string {
  if (status === 'completed') return '완주';
  if (status === 'finalizationFailed') return '결산 확인 중';
  return '진행 중';
}

function careerText(detail: PublicSaveDetail): string {
  if (detail.employerName === null || detail.jobFamilyKey === null) return '현재 직업 없음';
  const salary =
    detail.annualSalaryKrw === null ? '' : ` · 연 ${formatWon(detail.annualSalaryKrw)}`;
  return `${detail.employerName} · ${detail.jobFamilyKey}${salary}`;
}

function residenceText(detail: PublicSaveDetail): string {
  const residence =
    detail.residenceTenure === null ? '주거 정보 없음' : tenureLabel(detail.residenceTenure);
  return detail.corporationName === null
    ? residence
    : `${residence} · ${detail.corporationName} 운영`;
}

function educationLabel(value: string): string {
  return (
    {
      highSchool: '고졸',
      associate: '전문학사',
      bachelor: '학사',
      master: '석사',
      doctorate: '박사',
    }[value] ?? value
  );
}

function regionLabel(value: string): string {
  return (
    {
      capitalArea: '수도권',
      metropolitan: '광역시',
      smallCity: '중소도시',
      rural: '군 지역',
    }[value] ?? value
  );
}

function tenureLabel(value: string): string {
  return (
    {
      rentFree: '무상 거주',
      owner: '자가',
      jeonse: '전세',
      monthlyRent: '월세',
    }[value] ?? value
  );
}

function formatWon(value: number): string {
  return `${value.toLocaleString('ko-KR')}원`;
}
