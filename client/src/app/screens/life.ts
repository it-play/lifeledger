import type {
  CreditBand,
  CreditReason,
  EssentialArrear,
  GameSnapshot,
  LifeBudgetBand,
  LifeBudgetResponse,
  LifeBudgetSelection,
  LivingCostCategory,
  LivingCostMonthItem,
  LoanContractStatus,
  LoanProductKind,
  LoanSummary,
  NextLoanInstallment,
  ResidenceTenureKind,
} from '../../api/contracts.js';
import { type LifeApi, LifeCommandError } from '../../api/life-api.js';
import { el } from '../../lib/dom/index.js';
import { type AsyncState, createHooks } from '../../lib/hooks/index.js';
import type { Store } from '../../lib/store/index.js';
import type { ToastQueue } from '../../lib/toast/index.js';
import type { View, ViewFactory } from '../../lib/view/index.js';
import { formatBasisPoints, formatWon } from '../format.js';
import type { GameStateWriter } from '../game-state/index.js';
import {
  createEssentialArrearPaymentRetryPolicy,
  createLifeBudgetRetryPolicy,
} from '../life-retry/index.js';
import { type AppState, paths } from '../state.js';

export interface LifeDeps {
  readonly store: Store<AppState>;
  readonly snapshots: GameStateWriter;
  readonly api: LifeApi;
  readonly toasts: ToastQueue;
  readonly createCommandId: () => string;
}

interface BudgetItemRow {
  readonly category: LivingCostCategory;
  readonly element: HTMLTableRowElement;
  readonly band: HTMLSelectElement;
  readonly bandOptions: readonly HTMLOptionElement[];
  readonly essential: HTMLTableCellElement;
  readonly base: HTMLTableCellElement;
  readonly baseCpi: HTMLTableCellElement;
  readonly regionFactor: HTMLTableCellElement;
  readonly householdFactor: HTMLTableCellElement;
  readonly budgetFactor: HTMLTableCellElement;
  readonly tenureFactor: HTMLTableCellElement;
  readonly gross: HTMLTableCellElement;
  readonly paid: HTMLTableCellElement;
  readonly arrear: HTMLTableCellElement;
}

interface FixedArrearList {
  readonly element: HTMLUListElement;
  setItems(items: readonly EssentialArrear[]): void;
}

interface FixedArrearSelect {
  readonly element: HTMLSelectElement;
  setItems(items: readonly EssentialArrear[]): void;
}

interface FixedLoanList {
  readonly element: HTMLUListElement;
  setItems(items: readonly LoanSummary[]): void;
}

const CATEGORY_ORDER = [
  'housing',
  'food',
  'transport',
  'communication',
  'utilities',
  'healthcare',
  'education',
  'dependentCare',
  'discretionary',
] as const satisfies readonly LivingCostCategory[];

const CATEGORY_LABEL: Record<LivingCostCategory, string> = {
  housing: '주거',
  food: '식비',
  transport: '교통',
  communication: '통신',
  utilities: '공과금',
  healthcare: '의료',
  education: '교육',
  dependentCare: '부양가족 돌봄',
  discretionary: '재량 소비',
};

const TENURE_LABEL: Record<ResidenceTenureKind, string> = {
  rentFree: '무상 거주',
  owner: '자가',
  jeonse: '전세',
  monthlyRent: '월세',
};

const CREDIT_BAND_LABEL: Record<CreditBand, string> = {
  prime: '우량',
  standard: '일반',
  limited: '제한',
  distressed: '위험',
  insolvent: '채무불이행',
};

const CREDIT_REASON_LABEL: Record<CreditReason, string> = {
  modelUnavailable: '이 실행에는 신용 모형이 적용되지 않음',
  activeDefault: '채무불이행 대출이 있음',
  activeDelinquency: '연체 대출이 있음',
  cleanHistory: '현재 연체·채무불이행이 없음',
};

const LOAN_PRODUCT_LABEL: Record<LoanProductKind, string> = {
  studentLoan: '학자금 대출',
  unsecuredLoan: '신용 대출',
  leaseDepositLoan: '전세자금 대출',
  mortgage: '주택담보대출',
  legacyDebt: '기존 통합 부채',
};

const LOAN_STATUS_LABEL: Record<LoanContractStatus, string> = {
  pending: '실행 대기',
  active: '상환 중',
  delinquent: '연체',
  defaulted: '채무불이행',
  paidOff: '상환 완료',
  restructured: '채무조정',
  discharged: '면책',
  chargedOff: '상각',
  cancelled: '취소',
};

const MAX_BANDS = 16;
const MAX_ACTIVE_ARREARS = 20;
const MAX_ACTIVE_LOANS = 8;

/** M4-A household, living-cost budget, and essential-arrear controls. */
export function createLifeView(deps: LifeDeps): ViewFactory {
  const budgetRetries = createLifeBudgetRetryPolicy({ createCommandId: deps.createCommandId });
  const arrearRetries = createEssentialArrearPaymentRetryPolicy({
    createCommandId: deps.createCommandId,
  });

  return (): View => ({
    mount(host, ctx) {
      const h = createHooks(ctx.bag);
      const snapshot = h.useStoreValue(
        deps.store,
        paths.gameSnapshot,
        (state) => state.game.snapshot,
      );
      const advancing = h.useStoreValue(
        deps.store,
        paths.gameAdvancing,
        (state) => state.game.advancing,
      );
      const ordering = h.useStoreValue(
        deps.store,
        paths.gameOrdering,
        (state) => state.game.ordering,
      );
      const budget = h.useSignal<LifeBudgetResponse | undefined>(undefined);
      const commandBusy = h.useSignal(false);
      const commandFeedback = h.useSignal('');
      const gameReady = h.useComputed(() => {
        const current = snapshot.get();
        return current !== undefined && current.characterName !== null;
      });
      const lifeSummary = h.useComputed(() => snapshot.get()?.life ?? budget.get());
      const creditSummary = h.useComputed(() => snapshot.get()?.life);
      const household = h.useComputed(
        () => snapshot.get()?.life.household ?? budget.get()?.household ?? null,
      );
      const residence = h.useComputed(
        () => snapshot.get()?.life.residence ?? budget.get()?.residence ?? null,
      );
      const canMutate = h.useComputed(() => {
        const current = snapshot.get();
        return (
          current !== undefined &&
          current.characterName !== null &&
          current.autoSpeed === null &&
          current.life.rateStatus === 'active' &&
          budget.get() !== undefined &&
          !advancing.get() &&
          !ordering.get() &&
          !commandBusy.get()
        );
      });

      const budgetRequest = h.useAsync((signal) => deps.api.getBudget(signal));

      const requestStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const refresh = el('button', { type: 'button' }, '생활비 다시 조회');
      const commandStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const commandResult = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });

      const householdId = el('dd');
      const householdMembers = el('dd');
      const householdDependents = el('dd');
      const householdTaxDependents = el('dd');
      const residenceId = el('dd');
      const residenceRegion = el('dd');
      const residenceTenure = el('dd');
      const residenceEffectiveDay = el('dd');

      const creditBand = el('dd');
      const creditReasons = el('dd');
      const totalLoanBalance = el('dd');
      const nextLoanInstallment = el('dd');
      const loanList = createFixedLoanList();

      const rateStatus = el('dd');
      const profile = el('dd');
      const yearMonth = el('dd');
      const currentCpi = el('dd');
      const proration = el('dd');
      const activationDay = el('dd');
      const settlementDay = el('dd');
      const settlementStatus = el('dd');
      const totalGross = el('dd');
      const totalPaid = el('dd');
      const totalArrear = el('dd');

      const budgetRows = CATEGORY_ORDER.map(createBudgetItemRow);
      const budgetBody = el('tbody', {}, ...budgetRows.map((row) => row.element));
      const budgetSubmit = el('button', { type: 'submit' }, '항목별 예산 저장');
      const budgetForm = el(
        'form',
        {},
        el(
          'table',
          {},
          el(
            'thead',
            {},
            el(
              'tr',
              {},
              el('th', { attrs: { scope: 'col' } }, '항목'),
              el('th', { attrs: { scope: 'col' } }, '구분'),
              el('th', { attrs: { scope: 'col' } }, '예산 band'),
              el('th', { attrs: { scope: 'col' } }, '1인 기준액'),
              el('th', { attrs: { scope: 'col' } }, '기준 CPI'),
              el('th', { attrs: { scope: 'col' } }, '지역 계수'),
              el('th', { attrs: { scope: 'col' } }, '가구 계수'),
              el('th', { attrs: { scope: 'col' } }, '예산 계수'),
              el('th', { attrs: { scope: 'col' } }, '거주 대체 계수'),
              el('th', { attrs: { scope: 'col' } }, '확정액'),
              el('th', { attrs: { scope: 'col' } }, '납부액'),
              el('th', { attrs: { scope: 'col' } }, '부족액'),
            ),
          ),
          budgetBody,
        ),
        budgetSubmit,
      );

      const activeArrearTotal = el('strong');
      const activeArrearWindow = el('p');
      const arrearList = createFixedArrearList();
      const arrearSelect = createFixedArrearSelect();
      const arrearAmount = el('input', {
        type: 'number',
        name: 'amountKrw',
        attrs: { min: '1', step: '1', inputmode: 'numeric' },
      });
      const partialPaymentSubmit = el('button', { type: 'submit' }, '입력 금액 상환');
      const fullPayment = el('button', { type: 'button' }, '선택 연체 전액 상환');
      const arrearPaymentForm = el(
        'form',
        {},
        el('label', {}, '상환할 연체 ', arrearSelect.element),
        el('label', {}, '상환 금액(원) ', arrearAmount),
        partialPaymentSubmit,
        fullPayment,
      );

      host.replaceChildren(
        el(
          'main',
          { class: 'life' },
          el('h1', {}, '생활'),
          el('p', {}, el('a', { href: '/', dataset: { link: '' } }, '대시보드로 돌아가기')),
          requestStatus,
          refresh,
          commandStatus,
          commandResult,
          el(
            'section',
            {},
            el('h2', {}, '가구와 거주'),
            el(
              'dl',
              {},
              el('dt', {}, '가구 ID'),
              householdId,
              el('dt', {}, '전체 가구원'),
              householdMembers,
              el('dt', {}, '부양가족'),
              householdDependents,
              el('dt', {}, '세법상 부양가족'),
              householdTaxDependents,
              el('dt', {}, '거주 ID'),
              residenceId,
              el('dt', {}, '지역'),
              residenceRegion,
              el('dt', {}, '점유 형태'),
              residenceTenure,
              el('dt', {}, '적용 시작 게임일'),
              residenceEffectiveDay,
            ),
          ),
          el(
            'section',
            {},
            el('h2', {}, '신용과 대출'),
            el(
              'dl',
              {},
              el('dt', {}, '신용 구간'),
              creditBand,
              el('dt', {}, '판정 근거'),
              creditReasons,
              el('dt', {}, '총 대출 잔액'),
              totalLoanBalance,
              el('dt', {}, '다음 납부'),
              nextLoanInstallment,
            ),
            el('h3', {}, '진행 중인 대출'),
            loanList.element,
          ),
          el(
            'section',
            {},
            el('h2', {}, '이번 달 생활비 산정'),
            el(
              'p',
              {},
              '주거 항목은 먼저 1인 기준액에 거주 대체 계수를 적용합니다. 그 유효 기준액에 현재/기준 CPI 비율, 지역·가구·예산 계수를 적용하고 게임 시작 월은 남은 날짜만 일할 계산합니다.',
            ),
            el(
              'p',
              {},
              '예산 band 변경은 이미 확정된 이번 달 금액이 아니라 다음 산정부터 적용됩니다.',
            ),
            el(
              'dl',
              {},
              el('dt', {}, '요율 상태'),
              rateStatus,
              el('dt', {}, '생활비 profile'),
              profile,
              el('dt', {}, '대상 월'),
              yearMonth,
              el('dt', {}, '현재 CPI index'),
              currentCpi,
              el('dt', {}, '일할 계산 근거'),
              proration,
              el('dt', {}, '산정 시작 게임일'),
              activationDay,
              el('dt', {}, '월말 정산 게임일'),
              settlementDay,
              el('dt', {}, '정산 상태'),
              settlementStatus,
              el('dt', {}, '총 확정액'),
              totalGross,
              el('dt', {}, '총 납부액'),
              totalPaid,
              el('dt', {}, '총 부족액'),
              totalArrear,
            ),
            budgetForm,
          ),
          el(
            'section',
            {},
            el('h2', {}, '필수 생활비 연체'),
            el('p', {}, '현재 남은 필수 생활비 연체: ', activeArrearTotal),
            activeArrearWindow,
            arrearList.element,
            arrearPaymentForm,
          ),
        ),
      );

      h.bindText(requestStatus, () =>
        budgetRequestStatus(budgetRequest.state.get(), gameReady.get()),
      );
      h.bindText(commandStatus, () =>
        lifeCommandStatus(snapshot.get(), advancing.get(), ordering.get(), budget.get()),
      );
      h.bindText(commandResult, () => commandFeedback.get());
      h.bindText(householdId, () => household.get()?.id ?? '—');
      h.bindText(householdMembers, () => countText(household.get()?.memberCount));
      h.bindText(householdDependents, () => countText(household.get()?.dependentCount));
      h.bindText(householdTaxDependents, () =>
        countText(household.get()?.taxDependentEligibleCount),
      );
      h.bindText(residenceId, () => residence.get()?.id ?? '—');
      h.bindText(residenceRegion, () => residence.get()?.regionKey ?? '—');
      h.bindText(residenceTenure, () => {
        const tenure = residence.get()?.tenureKind;
        return tenure === undefined ? '—' : TENURE_LABEL[tenure];
      });
      h.bindText(residenceEffectiveDay, () => gameDayText(residence.get()?.effectiveFromGameDay));

      h.bindText(creditBand, () => creditBandText(creditSummary.get()?.creditBand));
      h.bindText(creditReasons, () => creditReasonText(creditSummary.get()?.creditReasons));
      h.bindText(totalLoanBalance, () => moneyText(creditSummary.get()?.totalLoanBalanceKrw));
      h.bindText(nextLoanInstallment, () =>
        nextLoanInstallmentText(creditSummary.get()?.nextLoanInstallment),
      );

      h.bindText(rateStatus, () => {
        const status = lifeSummary.get()?.rateStatus;
        if (status === undefined) return '—';
        return status === 'active' ? '산정 가능' : '이 월드에서는 CPI 요율을 사용할 수 없음';
      });
      h.bindText(profile, () => {
        const month = lifeSummary.get()?.currentMonth;
        return month === null || month === undefined
          ? '—'
          : `${month.profileKey} (#${month.profileId})`;
      });
      h.bindText(yearMonth, () => yearMonthText(lifeSummary.get()?.currentMonth));
      h.bindText(currentCpi, () => integerText(lifeSummary.get()?.currentMonth?.currentCpiIndex));
      h.bindText(proration, () => prorationText(lifeSummary.get()?.currentMonth));
      h.bindText(activationDay, () =>
        gameDayText(lifeSummary.get()?.currentMonth?.activationGameDay),
      );
      h.bindText(settlementDay, () =>
        gameDayText(lifeSummary.get()?.currentMonth?.settlementGameDay),
      );
      h.bindText(settlementStatus, () => {
        const settled = lifeSummary.get()?.currentMonth?.settled;
        return settled === undefined ? '—' : settled ? '정산 완료' : '월말 정산 대기';
      });
      h.bindText(totalGross, () => moneyText(lifeSummary.get()?.currentMonth?.totalGrossKrw));
      h.bindText(totalPaid, () => moneyText(lifeSummary.get()?.currentMonth?.totalPaidKrw));
      h.bindText(totalArrear, () => moneyText(lifeSummary.get()?.currentMonth?.totalArrearKrw));
      h.bindText(activeArrearTotal, () =>
        formatWon(lifeSummary.get()?.totalEssentialArrearKrw ?? 0),
      );
      h.bindText(activeArrearWindow, () =>
        lifeSummary.get()?.hasMoreActiveArrears
          ? '우선순위가 높은 20건을 표시합니다. 전액 상환하면 다음 연체가 나타납니다.'
          : '현재 활성 연체를 모두 표시합니다.',
      );

      for (const row of budgetRows) {
        h.bindText(row.essential, () => {
          const item = currentMonthItem(lifeSummary.get()?.currentMonth?.items, row.category);
          return item === undefined ? '—' : item.essential ? '필수' : '선택';
        });
        h.bindText(row.base, () =>
          moneyText(
            currentMonthItem(lifeSummary.get()?.currentMonth?.items, row.category)?.baseMonthlyKrw,
          ),
        );
        h.bindText(row.baseCpi, () =>
          integerText(
            currentMonthItem(lifeSummary.get()?.currentMonth?.items, row.category)?.baseCpiIndex,
          ),
        );
        h.bindText(row.regionFactor, () =>
          factorText(
            currentMonthItem(lifeSummary.get()?.currentMonth?.items, row.category)?.regionFactorPpm,
          ),
        );
        h.bindText(row.householdFactor, () =>
          factorText(
            currentMonthItem(lifeSummary.get()?.currentMonth?.items, row.category)
              ?.householdFactorPpm,
          ),
        );
        h.bindText(row.budgetFactor, () =>
          factorText(
            currentMonthItem(lifeSummary.get()?.currentMonth?.items, row.category)?.budgetFactorPpm,
          ),
        );
        h.bindText(row.tenureFactor, () =>
          row.category === 'housing'
            ? factorText(
                currentMonthItem(lifeSummary.get()?.currentMonth?.items, row.category)
                  ?.tenureReplacementFactorPpm,
              )
            : '적용 없음',
        );
        h.bindText(row.gross, () =>
          moneyText(
            currentMonthItem(lifeSummary.get()?.currentMonth?.items, row.category)?.grossKrw,
          ),
        );
        h.bindText(row.paid, () =>
          moneyText(
            currentMonthItem(lifeSummary.get()?.currentMonth?.items, row.category)?.paidKrw,
          ),
        );
        h.bindText(row.arrear, () =>
          moneyText(
            currentMonthItem(lifeSummary.get()?.currentMonth?.items, row.category)?.arrearKrw,
          ),
        );
        h.bindAttribute(row.band, 'disabled', () => !canMutate.get());
      }

      h.bindAttribute(
        refresh,
        'disabled',
        () => !gameReady.get() || budgetRequest.state.get().status === 'loading',
      );
      h.bindAttribute(budgetSubmit, 'disabled', () => !canMutate.get());
      h.bindAttribute(arrearSelect.element, 'disabled', () => !canMutate.get());
      h.bindAttribute(arrearAmount, 'disabled', () => !canMutate.get());
      h.bindAttribute(
        partialPaymentSubmit,
        'disabled',
        () => !canMutate.get() || (lifeSummary.get()?.activeArrears.length ?? 0) === 0,
      );
      h.bindAttribute(
        fullPayment,
        'disabled',
        () => !canMutate.get() || (lifeSummary.get()?.activeArrears.length ?? 0) === 0,
      );

      h.useEffect(() => {
        const state = budgetRequest.state.get();
        if (state.status === 'success') budget.set(state.value);
      });
      h.useEffect(() => {
        const current = budget.get();
        for (const row of budgetRows) {
          updateBandSelect(
            row,
            current?.allowedBands ?? [],
            current?.selections.find((selection) => selection.category === row.category)?.bandId,
          );
        }
      });
      h.useEffect(() => {
        const arrears = lifeSummary.get()?.activeArrears ?? [];
        arrearList.setItems(arrears);
        arrearSelect.setItems(arrears);
        syncArrearAmountLimit(arrearSelect.element, arrearAmount, arrears);
      });
      h.useEffect(() => {
        loanList.setItems(creditSummary.get()?.activeLoans ?? []);
      });

      h.useWatch(snapshot, (next, previous) => {
        if (next === undefined || next.characterName === null) {
          budgetRequest.cancel();
          budget.set(undefined);
          return;
        }
        if (
          previous === undefined ||
          next.runRevision !== previous.runRevision ||
          next.stateRevision !== previous.stateRevision
        ) {
          budgetRequest.run();
        }
      });

      h.useEventListener(refresh, 'click', () => {
        if (gameReady.peek()) budgetRequest.run();
      });
      h.useEventListener(arrearSelect.element, 'change', () => {
        syncArrearAmountLimit(
          arrearSelect.element,
          arrearAmount,
          lifeSummary.peek()?.activeArrears ?? [],
        );
      });
      h.useEventListener(budgetForm, 'submit', (event) => {
        event.preventDefault();
        void submitBudget().catch((error: unknown) => {
          commandFeedback.set(error instanceof Error ? error.message : '예산 변경에 실패했습니다.');
        });
      });
      h.useEventListener(arrearPaymentForm, 'submit', (event) => {
        event.preventDefault();
        void submitSelectedArrear(false).catch((error: unknown) => {
          commandFeedback.set(error instanceof Error ? error.message : '연체 상환에 실패했습니다.');
        });
      });
      h.useEventListener(fullPayment, 'click', () => {
        void submitSelectedArrear(true).catch((error: unknown) => {
          commandFeedback.set(error instanceof Error ? error.message : '연체 상환에 실패했습니다.');
        });
      });

      if (gameReady.peek()) budgetRequest.run();

      async function submitBudget(): Promise<void> {
        const currentSnapshot = commandSnapshot(deps, '생활비 예산을 변경');
        const selections = budgetSelectionsOf(budget.peek(), budgetRows);
        const request = budgetRetries.select(currentSnapshot, { selections });

        commandBusy.set(true);
        commandFeedback.set('생활비 예산을 저장하는 중입니다.');
        deps.store.set(paths.gameOrdering, true);
        try {
          const response = await deps.api.updateBudget(request);
          budgetRetries.complete(request);
          applyLifeCommandSnapshot(
            deps.snapshots,
            budget,
            response.snapshot,
            response.result.selections,
          );
          commandFeedback.set(budgetUpdateSuccessText(response.replayed));
          deps.toasts.show('생활비 예산을 저장했습니다.', { tone: 'success' });
          budgetRequest.run();
        } catch (error) {
          budgetRetries.fail(request, error);
          throw lifeDisplayError(error, '생활비 예산 변경');
        } finally {
          deps.store.set(paths.gameOrdering, false);
          commandBusy.set(false);
        }
      }

      async function submitSelectedArrear(payFull: boolean): Promise<void> {
        const currentSnapshot = commandSnapshot(deps, '필수 생활비 연체를 상환');
        const payment = arrearPaymentInputOf(
          arrearSelect.element,
          arrearAmount,
          lifeSummary.peek()?.activeArrears ?? [],
          payFull,
        );
        const command = arrearRetries.select(currentSnapshot, payment.arrearId, {
          amountKrw: payment.amountKrw,
        });

        commandBusy.set(true);
        commandFeedback.set('필수 생활비 연체를 상환하는 중입니다.');
        deps.store.set(paths.gameOrdering, true);
        try {
          const response = await deps.api.payEssentialArrear(command.arrearId, command.request);
          arrearRetries.complete(command);
          applyLifeCommandSnapshot(deps.snapshots, budget, response.snapshot);
          commandFeedback.set(arrearPaymentSuccessText(response.replayed, response.result.paidKrw));
          deps.toasts.show(`${formatWon(response.result.paidKrw)}을 상환했습니다.`, {
            tone: 'success',
          });
          arrearAmount.value = '';
          budgetRequest.run();
        } catch (error) {
          arrearRetries.fail(command, error);
          throw lifeDisplayError(error, '필수 생활비 연체 상환');
        } finally {
          deps.store.set(paths.gameOrdering, false);
          commandBusy.set(false);
        }
      }
    },
    unmount() {},
  });
}

function createBudgetItemRow(category: LivingCostCategory): BudgetItemRow {
  const band = el('select', {
    name: `budget-${category}`,
    attrs: { 'aria-label': `${CATEGORY_LABEL[category]} 예산 band` },
  });
  band.appendChild(el('option', { value: '', disabled: true }, '예산 band 조회 중'));
  const bandOptions = Array.from({ length: MAX_BANDS }, () => el('option'));
  for (const option of bandOptions) {
    option.hidden = true;
    option.disabled = true;
    band.appendChild(option);
  }
  const essential = el('td');
  const base = el('td');
  const baseCpi = el('td');
  const regionFactor = el('td');
  const householdFactor = el('td');
  const budgetFactor = el('td');
  const tenureFactor = el('td');
  const gross = el('td');
  const paid = el('td');
  const arrear = el('td');
  return {
    category,
    element: el(
      'tr',
      {},
      el('th', { attrs: { scope: 'row' } }, CATEGORY_LABEL[category]),
      essential,
      el('td', {}, band),
      base,
      baseCpi,
      regionFactor,
      householdFactor,
      budgetFactor,
      tenureFactor,
      gross,
      paid,
      arrear,
    ),
    band,
    bandOptions,
    essential,
    base,
    baseCpi,
    regionFactor,
    householdFactor,
    budgetFactor,
    tenureFactor,
    gross,
    paid,
    arrear,
  };
}

function updateBandSelect(
  row: BudgetItemRow,
  bands: readonly LifeBudgetBand[],
  selectedBandId: string | undefined,
): void {
  for (const [index, option] of row.bandOptions.entries()) {
    const band = bands[index];
    option.hidden = band === undefined;
    option.disabled = band === undefined;
    option.value = band?.id ?? '';
    const label = band === undefined ? '' : `${band.displayName} (${factorText(band.factorPpm)})`;
    if (option.textContent !== label) option.textContent = label;
  }
  const next = selectedBandId ?? '';
  if (row.band.value !== next) row.band.value = next;
}

function createFixedArrearList(): FixedArrearList {
  const rows = Array.from({ length: MAX_ACTIVE_ARREARS }, () => {
    const text = el('span');
    const element = el('li', {}, text);
    element.hidden = true;
    return { element, text };
  });
  return {
    element: el('ul', {}, ...rows.map((row) => row.element)),
    setItems(items) {
      for (const [index, row] of rows.entries()) {
        const item = items[index];
        row.element.hidden = item === undefined;
        const text = item === undefined ? '' : arrearText(item);
        if (row.text.textContent !== text) row.text.textContent = text;
      }
    },
  };
}

function createFixedLoanList(): FixedLoanList {
  const rows = Array.from({ length: MAX_ACTIVE_LOANS }, () => {
    const text = el('span');
    const element = el('li', {}, text);
    element.hidden = true;
    return { element, text };
  });
  return {
    element: el('ul', {}, ...rows.map((row) => row.element)),
    setItems(items) {
      for (const [index, row] of rows.entries()) {
        const loan = items[index];
        row.element.hidden = loan === undefined;
        const text = loan === undefined ? '' : loanText(loan);
        if (row.text.textContent !== text) row.text.textContent = text;
      }
    },
  };
}

function createFixedArrearSelect(): FixedArrearSelect {
  const element = el('select', {
    name: 'arrearId',
    attrs: { 'aria-label': '상환할 필수 생활비 연체' },
  });
  element.appendChild(el('option', { value: '' }, '연체를 선택하세요'));
  const options = Array.from({ length: MAX_ACTIVE_ARREARS }, () => el('option'));
  for (const option of options) {
    option.hidden = true;
    option.disabled = true;
    element.appendChild(option);
  }
  return {
    element,
    setItems(items) {
      const previous = element.value;
      for (const [index, option] of options.entries()) {
        const item = items[index];
        option.hidden = item === undefined;
        option.disabled = item === undefined;
        option.value = item?.id ?? '';
        const text = item === undefined ? '' : arrearOptionText(item);
        if (option.textContent !== text) option.textContent = text;
      }
      const stillActive = items.some((item) => item.id === previous);
      element.value = stillActive ? previous : (items[0]?.id ?? '');
    },
  };
}

function mergeSnapshotLife(
  budgetSignal: { peek(): LifeBudgetResponse | undefined; set(value: LifeBudgetResponse): void },
  snapshot: GameSnapshot,
  selections?: readonly LifeBudgetSelection[],
): void {
  const current = budgetSignal.peek();
  const { household, residence } = snapshot.life;
  if (current === undefined || household === null || residence === null) return;
  budgetSignal.set({
    ...current,
    rateStatus: snapshot.life.rateStatus,
    currentMonth: snapshot.life.currentMonth,
    activeArrears: snapshot.life.activeArrears,
    hasMoreActiveArrears: snapshot.life.hasMoreActiveArrears,
    totalEssentialArrearKrw: snapshot.life.totalEssentialArrearKrw,
    household,
    residence,
    selections: selections === undefined ? current.selections : [...selections],
  });
}

function applyLifeCommandSnapshot(
  snapshots: GameStateWriter,
  budgetSignal: { peek(): LifeBudgetResponse | undefined; set(value: LifeBudgetResponse): void },
  snapshot: GameSnapshot,
  selections?: readonly LifeBudgetSelection[],
): void {
  if (!snapshots.apply(snapshot)) return;
  mergeSnapshotLife(budgetSignal, snapshot, selections);
}

function budgetSelectionsOf(
  budget: LifeBudgetResponse | undefined,
  rows: readonly BudgetItemRow[],
): LifeBudgetSelection[] {
  if (budget === undefined) throw new Error('생활비를 먼저 조회해 주세요.');
  return rows.map((row): LifeBudgetSelection => {
    if (row.band.value === '') {
      throw new Error(`${CATEGORY_LABEL[row.category]} 예산 band를 선택해 주세요.`);
    }
    return { category: row.category, bandId: row.band.value };
  });
}

function arrearPaymentInputOf(
  select: HTMLSelectElement,
  input: HTMLInputElement,
  arrears: readonly EssentialArrear[],
  payFull: boolean,
): { readonly arrearId: string; readonly amountKrw: number } {
  const arrear = selectedArrear(select, arrears);
  if (arrear === undefined) throw new Error('상환할 연체를 선택해 주세요.');
  const amountKrw = payFull ? arrear.remainingKrw : positiveIntegerOf(input.value);
  if (amountKrw > arrear.remainingKrw) {
    throw new Error(`상환 금액은 남은 ${formatWon(arrear.remainingKrw)} 이하여야 합니다.`);
  }
  return { arrearId: arrear.id, amountKrw };
}

function budgetUpdateSuccessText(replayed: boolean): string {
  return replayed
    ? '이미 처리된 예산 변경 결과를 확인했습니다.'
    : '항목별 생활비 예산을 저장했습니다.';
}

function arrearPaymentSuccessText(replayed: boolean, paidKrw: number): string {
  return replayed
    ? '이미 처리된 연체 상환 결과를 확인했습니다.'
    : `${formatWon(paidKrw)}을 상환했습니다.`;
}

function currentMonthItem(
  items: readonly LivingCostMonthItem[] | undefined,
  category: LivingCostCategory,
): LivingCostMonthItem | undefined {
  return items?.find((item) => item.category === category);
}

function selectedArrear(
  select: HTMLSelectElement,
  arrears: readonly EssentialArrear[],
): EssentialArrear | undefined {
  return arrears.find((arrear) => arrear.id === select.value);
}

function syncArrearAmountLimit(
  select: HTMLSelectElement,
  input: HTMLInputElement,
  arrears: readonly EssentialArrear[],
): void {
  const arrear = selectedArrear(select, arrears);
  if (arrear === undefined) input.removeAttribute('max');
  else input.max = String(arrear.remainingKrw);
}

function positiveIntegerOf(raw: string): number {
  const value = Number(raw);
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new Error('상환 금액은 1원 이상의 정수로 입력해 주세요.');
  }
  return value;
}

function commandSnapshot(deps: LifeDeps, action: string): GameSnapshot {
  const state = deps.store.getState();
  const snapshot = state.game.snapshot;
  if (snapshot === undefined || snapshot.characterName === null) {
    throw new Error(`캐릭터를 만든 뒤 ${action}할 수 있습니다.`);
  }
  if (snapshot.autoSpeed !== null) {
    throw new Error(`자동 진행을 멈춘 뒤 ${action}할 수 있습니다.`);
  }
  if (snapshot.life.rateStatus !== 'active') {
    throw new Error('이 월드에서는 CPI 생활비 요율을 사용할 수 없습니다.');
  }
  if (state.game.advancing || state.game.ordering) {
    throw new Error(`다른 게임 명령이 끝난 뒤 ${action}해 주세요.`);
  }
  return snapshot;
}

function lifeDisplayError(error: unknown, action: string): Error {
  if (error instanceof LifeCommandError) return new Error(error.message);
  return new Error(`${action} 결과를 확인하지 못했습니다. 같은 입력으로 다시 시도해 주세요.`);
}

function lifeCommandStatus(
  snapshot: GameSnapshot | undefined,
  advancing: boolean,
  ordering: boolean,
  budget: LifeBudgetResponse | undefined,
): string {
  if (snapshot === undefined) return '게임 상태를 기다리는 중입니다.';
  if (snapshot.characterName === null) return '캐릭터를 만든 뒤 생활비를 관리할 수 있습니다.';
  if (snapshot.life.rateStatus !== 'active') {
    return '이 월드는 CPI 생활비 요율을 지원하지 않아 조회만 할 수 있습니다.';
  }
  if (snapshot.autoSpeed !== null) return '생활비 명령을 보내려면 자동 진행을 멈추세요.';
  if (advancing || ordering) return '다른 게임 명령을 처리하는 중입니다.';
  if (budget === undefined) return '생활비 조회가 끝나면 예산과 연체를 관리할 수 있습니다.';
  return '생활비 예산과 필수 생활비 연체를 관리할 수 있습니다.';
}

function budgetRequestStatus(state: AsyncState<LifeBudgetResponse>, gameReady: boolean): string {
  if (!gameReady) return '캐릭터를 만든 뒤 생활비를 조회할 수 있습니다.';
  switch (state.status) {
    case 'idle':
      return '생활비 조회를 기다리는 중입니다.';
    case 'loading':
      return '생활비 산정 근거를 불러오는 중입니다.';
    case 'success':
      return '생활비 산정 근거를 불러왔습니다.';
    case 'error':
      return state.error instanceof LifeCommandError
        ? state.error.message
        : '생활비 산정 근거를 불러오지 못했습니다.';
  }
}

function arrearText(arrear: EssentialArrear): string {
  return `${yearMonthValueText(arrear.dueYearMonth.year, arrear.dueYearMonth.month)} ${CATEGORY_LABEL[arrear.category]} · 최초 ${formatWon(arrear.originalKrw)} · 남은 ${formatWon(arrear.remainingKrw)} · #${arrear.id}`;
}

function arrearOptionText(arrear: EssentialArrear): string {
  return `${yearMonthValueText(arrear.dueYearMonth.year, arrear.dueYearMonth.month)} ${CATEGORY_LABEL[arrear.category]} · ${formatWon(arrear.remainingKrw)}`;
}

function creditBandText(band: CreditBand | null | undefined): string {
  if (band === undefined) return '—';
  return band === null ? '이 실행에서는 산정하지 않음' : CREDIT_BAND_LABEL[band];
}

function creditReasonText(reasons: readonly CreditReason[] | undefined): string {
  if (reasons === undefined) return '—';
  return reasons.length === 0
    ? '판정 근거 없음'
    : reasons.map((reason) => CREDIT_REASON_LABEL[reason]).join(' · ');
}

function loanText(loan: LoanSummary): string {
  const rate =
    loan.rateStatus === 'rateUnavailable' || loan.currentAnnualRateBp === null
      ? '금리 정보 없음'
      : `연 ${formatBasisPoints(loan.currentAnnualRateBp)}`;
  const readOnly = loan.readOnly ? ' · 조회 전용' : '';
  return `#${loan.id} ${loan.displayName} · ${LOAN_PRODUCT_LABEL[loan.productKind]} · ${LOAN_STATUS_LABEL[loan.status]} · 잔여 원금 ${formatWon(loan.remainingPrincipalKrw)} · 연체 ${formatWon(loan.overdueKrw)} · ${rate}${readOnly}`;
}

function nextLoanInstallmentText(installment: NextLoanInstallment | null | undefined): string {
  if (installment === undefined) return '—';
  if (installment === null) return '예정된 납부 없음';
  return `대출 #${installment.loanId} ${installment.installmentNo}회차 · 게임 ${installment.dueGameDay.toLocaleString('ko-KR')}일 · 남은 ${formatWon(installment.remainingDueKrw)} (수수료 ${formatWon(installment.feeKrw)} / 이자 ${formatWon(installment.interestKrw)} / 원금 ${formatWon(installment.principalKrw)})`;
}

function yearMonthText(month: LifeBudgetResponse['currentMonth'] | undefined): string {
  if (month === undefined) return '—';
  return month === null
    ? '아직 확정된 생활비가 없습니다.'
    : yearMonthValueText(month.yearMonth.year, month.yearMonth.month);
}

function prorationText(month: LifeBudgetResponse['currentMonth'] | undefined): string {
  if (month === undefined || month === null) return '—';
  return `${month.prorationDays.toLocaleString('ko-KR')}/${month.daysInMonth.toLocaleString('ko-KR')}일 · ${month.prorationUnits.toLocaleString('ko-KR')}/${month.prorationScale.toLocaleString('ko-KR')} 단위`;
}

function yearMonthValueText(year: number, month: number): string {
  return `${year}년 ${month}월`;
}

function countText(value: number | undefined): string {
  return value === undefined ? '—' : `${value.toLocaleString('ko-KR')}명`;
}

function gameDayText(value: number | undefined): string {
  return value === undefined ? '—' : `${value.toLocaleString('ko-KR')}일`;
}

function integerText(value: number | undefined): string {
  return value === undefined ? '—' : value.toLocaleString('ko-KR');
}

function moneyText(value: number | undefined): string {
  return value === undefined ? '—' : formatWon(value);
}

function factorText(value: number | undefined): string {
  if (value === undefined) return '—';
  const whole = Math.floor(value / 10_000);
  const fraction = (value % 10_000).toString().padStart(4, '0').replace(/0+$/, '');
  return `${whole}${fraction === '' ? '' : `.${fraction}`}%`;
}
