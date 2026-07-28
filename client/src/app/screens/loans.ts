import type {
  CreditBand,
  CreditReason,
  CreditResponse,
  GameSnapshot,
  LoanContractStatus,
  LoanDetail,
  LoanExecutionRequest,
  LoanExecutionResponse,
  LoanInstallmentHistoryItem,
  LoanInstallmentHistoryResponse,
  LoanInstallmentStatus,
  LoanPaymentAllocationKind,
  LoanPaymentHistoryItem,
  LoanPaymentKind,
  LoanPrepaymentEffect,
  LoanPrepaymentResponse,
  LoanPrepaymentResult,
  LoanProduct,
  LoanProductCatalog,
  LoanProductKind,
  LoanQuoteDecisionCode,
  LoanQuoteDecisionReason,
  LoanQuoteResult,
  LoanRepaymentMethod,
  LoanSummary,
} from '../../api/contracts.js';
import { type LoanApi, LoanCommandError } from '../../api/loan-api.js';
import { el } from '../../lib/dom/index.js';
import { type AsyncState, createHooks } from '../../lib/hooks/index.js';
import type { Store } from '../../lib/store/index.js';
import type { ViewFactory } from '../../lib/view/index.js';
import { formatBasisPoints, formatWon } from '../format.js';
import type { GameStateWriter } from '../game-state/index.js';
import {
  createLoanExecutionRetryPolicy,
  createLoanPrepaymentRetryPolicy,
  createLoanQuoteRetryPolicy,
  type LoanExecutionRetryPolicy,
  type LoanPrepaymentCommand,
  type LoanPrepaymentRetryPolicy,
} from '../loan-retry/index.js';
import { type AppState, paths } from '../state.js';

export interface LoansViewDeps {
  readonly store: Store<AppState>;
  readonly snapshots: GameStateWriter;
  readonly api: LoanApi;
  readonly createCommandId: () => string;
}

interface CreditNodes {
  readonly status: HTMLElement;
  readonly creditBand: HTMLElement;
  readonly creditReasons: HTMLElement;
  readonly totalBalance: HTMLElement;
  readonly nextInstallment: HTMLElement;
  readonly loanRows: readonly HTMLLIElement[];
}

interface FixedProductSelect {
  readonly element: HTMLSelectElement;
  setItems(items: readonly LoanProduct[], selectedId: string): string;
}

interface FixedLoanSelect {
  readonly element: HTMLSelectElement;
  setItems(
    items: readonly LoanSummary[],
    selectedId: string,
    pending: LoanPrepaymentCommand | undefined,
  ): string;
}

interface FixedDetailLoanSelect {
  readonly element: HTMLSelectElement;
  setItems(items: readonly LoanSummary[], selectedId: string): string;
}

interface LoanDetailNodes {
  readonly status: HTMLElement;
  readonly identity: HTMLElement;
  readonly propertyHolding: HTMLAnchorElement;
  readonly contractStatus: HTMLElement;
  readonly rate: HTMLElement;
  readonly balances: HTMLElement;
  readonly terms: HTMLElement;
  readonly schedule: HTMLElement;
  readonly prepayment: HTMLElement;
  readonly dsr: HTMLElement;
}

const PRODUCT_KIND_LABEL: Record<LoanProductKind, string> = {
  studentLoan: '학자금 대출',
  unsecuredLoan: '신용대출',
  leaseDepositLoan: '임차보증금 대출',
  mortgage: '주택담보대출',
  legacyDebt: '이전 버전 합산 부채',
};

const CREDIT_BAND_LABEL: Record<CreditBand, string> = {
  prime: '우수',
  standard: '일반',
  limited: '제한',
  distressed: '위험',
  insolvent: '지급불능',
};

const CREDIT_REASON_LABEL: Record<CreditReason, string> = {
  modelUnavailable: '이 run에는 신용 model이 없습니다',
  activeDefault: '채무불이행 계약이 있습니다',
  activeDelinquency: '연체 계약이 있습니다',
  cleanHistory: '현재 연체나 채무불이행이 없습니다',
};

const LOAN_STATUS_LABEL: Record<LoanContractStatus, string> = {
  pending: '대기',
  active: '정상',
  delinquent: '연체',
  defaulted: '채무불이행',
  paidOff: '상환 완료',
  restructured: '재조정',
  discharged: '면책',
  chargedOff: '상각',
  cancelled: '취소',
};

const PREPAYMENT_EFFECT_LABEL: Record<LoanPrepaymentEffect, string> = {
  reduceTerm: '남은 만기 단축',
  recalculatePayment: '남은 납입액 재산정',
};

const INSTALLMENT_STATUS_LABEL: Record<LoanInstallmentStatus, string> = {
  pending: '예정',
  due: '납입일 도래',
  partiallyPaid: '일부 납부',
  paid: '납부 완료',
  cancelled: '취소',
  discharged: '면책',
};

const PAYMENT_KIND_LABEL: Record<LoanPaymentKind, string> = {
  scheduledInstallment: '정기 납입',
  manualPrepayment: '수동 조기상환',
  leaseMovePayoff: '임대차 이사 상환',
  propertySalePayoff: '주택 매도 상환',
  insolvencyDistribution: '도산 청산 배분',
};

const ALLOCATION_KIND_LABEL: Record<LoanPaymentAllocationKind, string> = {
  overdueFee: '연체 비용',
  overdueInterest: '연체 이자',
  overduePrincipal: '연체 원금',
  currentFee: '당일 비용',
  currentInterest: '당일 이자',
  currentPrincipal: '당일 원금',
  prepaymentFee: '조기상환 수수료',
  prepaymentPrincipal: '조기상환 원금',
};

const QUOTE_DECISION_LABEL: Record<LoanQuoteDecisionCode, string> = {
  eligible: '견적 가능',
  debtServiceLimit: 'DSR 한도 초과',
  incomeUnavailable: '인정 소득 확인 불가',
  creditRestricted: '신용 조건 제한',
  valuationUnavailable: '담보 가치 확인 불가',
};

const QUOTE_REASON_LABEL: Record<LoanQuoteDecisionReason, string> = {
  insolvencyRebuilding: '도산 후 신용 회복 기간',
  activeDefault: '채무불이행 계약',
  activeDelinquency: '연체 계약',
  activeRestructuring: '채무조정 계약',
  creditBandRestricted: '허용되지 않는 신용 구간',
  activeLoanLimit: '활성 대출 계약 수 상한',
  incomeUnavailable: '인정 소득 없음',
  debtServiceLimit: 'DSR 한도 초과',
  eligible: '심사 조건 충족',
};

/** Functional M4-B loan catalog, credit summary, quote, and execution screen. */
export function createLoansView(deps: LoansViewDeps): ViewFactory {
  const quoteRetries = createLoanQuoteRetryPolicy({ createCommandId: deps.createCommandId });
  const executionRetries = createLoanExecutionRetryPolicy({
    createCommandId: deps.createCommandId,
  });
  const prepaymentRetries = createLoanPrepaymentRetryPolicy({
    createCommandId: deps.createCommandId,
  });

  return () => ({
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
      const mountedRunRevision = deps.store.getState().game.snapshot?.runRevision;
      const mountedPrepayment =
        mountedRunRevision === undefined
          ? undefined
          : prepaymentRetries.pendingForRun(mountedRunRevision);
      const quoteBusy = h.useSignal(false);
      const executeBusy = h.useSignal(false);
      const prepaymentBusy = h.useSignal(false);
      const quoteFeedback = h.useSignal('');
      const quoteResult = h.useSignal<LoanQuoteResult | undefined>(undefined);
      const quoteRunRevision = h.useSignal<number | undefined>(undefined);
      const selectedProductId = h.useSignal('');
      const principalRaw = h.useSignal('');
      const selectedDetailLoanId = h.useSignal('');
      const historyBefore = h.useSignal<string | undefined>(undefined);
      const prepaymentFeedback = h.useSignal('');
      const prepaymentResult = h.useSignal<LoanPrepaymentResult | undefined>(undefined);
      const pendingPrepayment = h.useSignal<LoanPrepaymentCommand | undefined>(mountedPrepayment);
      const selectedLoanId = h.useSignal(mountedPrepayment?.loanId ?? '');
      const prepaymentPrincipalRaw = h.useSignal(
        mountedPrepayment === undefined ? '' : String(mountedPrepayment.request.principalKrw),
      );

      const status = el('p', {}, '대출 정보를 불러오는 중…');
      const catalogStatus = el('p', {}, '대출 상품을 불러오는 중…');
      const creditBand = el('dd', {}, '-');
      const creditReasons = el('dd', {}, '-');
      const totalBalance = el('dd', {}, formatWon(0));
      const nextInstallment = el('dd', {}, '-');
      const productRows = createRows(16);
      const loanRows = createRows(8);
      const reload = el('button', { type: 'button' }, '다시 불러오기');
      const productList = el('ul', {}, ...productRows);
      const loanList = el('ul', {}, ...loanRows);

      const quoteStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const quoteMessage = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const quoteProducts = createFixedProductSelect();
      const principal = el('input', {
        type: 'number',
        name: 'principalKrw',
        attrs: { min: '1', step: '1', inputmode: 'numeric' },
      });
      const quoteSubmit = el('button', { type: 'submit' }, '대출 견적 심사');
      const quoteForm = el(
        'form',
        {},
        el('label', {}, '견적 상품 ', quoteProducts.element),
        el('label', {}, '신청 원금(원) ', principal),
        quoteSubmit,
      );

      const resultSection = el('section', { attrs: { 'aria-live': 'polite' } });
      const resultDecision = el('dd');
      const resultReasons = el('dd');
      const resultRequested = el('dd');
      const resultValidity = el('dd');
      const resultIncome = el('dd');
      const resultBalance = el('dd');
      const resultDsr = el('dd');
      const resultStressRate = el('dd');
      const resultTerms = el('dd');
      const resultFirstInstallment = el('dd');
      const executeStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const executeSubmit = el('button', { type: 'button' }, '이 견적으로 대출 실행');
      resultSection.append(
        el('h3', {}, '견적 결과'),
        el(
          'p',
          {},
          '견적은 표시용이며 생성된 game day에만 유효합니다. 실제 실행 때 서버가 최신 신용·소득·금리·DSR을 다시 심사합니다.',
        ),
        el(
          'dl',
          {},
          el('dt', {}, '결정'),
          resultDecision,
          el('dt', {}, '공개 사유'),
          resultReasons,
          el('dt', {}, '요청 원금'),
          resultRequested,
          el('dt', {}, '유효 기간'),
          resultValidity,
          el('dt', {}, '인정 소득'),
          resultIncome,
          el('dt', {}, 'DSR 대상 대출 잔액'),
          resultBalance,
          el('dt', {}, 'DSR'),
          resultDsr,
          el('dt', {}, '심사 stress 금리'),
          resultStressRate,
          el('dt', {}, '견적 조건'),
          resultTerms,
          el('dt', {}, '첫 납입'),
          resultFirstInstallment,
        ),
        executeStatus,
        executeSubmit,
      );

      const prepaymentStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const prepaymentMessage = el('p', {
        attrs: { role: 'status', 'aria-live': 'polite' },
      });
      const prepaymentLoans = createFixedLoanSelect();
      const prepaymentPrincipal = el('input', {
        type: 'number',
        name: 'prepaymentPrincipalKrw',
        attrs: { min: '1', step: '1', inputmode: 'numeric' },
      });
      prepaymentPrincipal.value = prepaymentPrincipalRaw.peek();
      const prepaymentSubmit = el('button', { type: 'submit' }, '조기상환');
      const prepaymentForm = el(
        'form',
        {},
        el('label', {}, '상환할 계약 ', prepaymentLoans.element),
        el('label', {}, '줄일 원금(원) ', prepaymentPrincipal),
        prepaymentSubmit,
      );
      const prepaymentResultSection = el('section', { attrs: { 'aria-live': 'polite' } });
      const prepaymentPayment = el('dd');
      const prepaymentDebit = el('dd');
      const prepaymentRemaining = el('dd');
      const prepaymentSchedule = el('dd');
      prepaymentResultSection.append(
        el('h3', {}, '조기상환 결과'),
        el(
          'dl',
          {},
          el('dt', {}, '지급'),
          prepaymentPayment,
          el('dt', {}, '지갑 차감'),
          prepaymentDebit,
          el('dt', {}, '계약 상태'),
          prepaymentRemaining,
          el('dt', {}, '남은 일정'),
          prepaymentSchedule,
        ),
      );

      const detailLoans = createFixedDetailLoanSelect();
      const detailReload = el('button', { type: 'button' }, '선택 계약 다시 불러오기');
      const detailStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const detailIdentity = el('dd', {}, '-');
      const detailPropertyHolding = el(
        'a',
        { href: '/housing', dataset: { link: '' } },
        '연결 보유주택 없음',
      );
      const detailContractStatus = el('dd', {}, '-');
      const detailRate = el('dd', {}, '-');
      const detailBalances = el('dd', {}, '-');
      const detailTerms = el('dd', {}, '-');
      const detailSchedule = el('dd', {}, '-');
      const detailPrepayment = el('dd', {}, '-');
      const detailDsr = el('dd', {}, '-');
      const installmentRows = createRows(50);
      const paymentRows = createRows(50);
      const historyStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const historyLoadOlder = el('button', { type: 'button' }, '이전 상환표·납부 이력 더 보기');
      const detailSection = el(
        'section',
        {},
        el('label', {}, '상세 계약 ', detailLoans.element),
        detailReload,
        detailStatus,
        el(
          'dl',
          {},
          el('dt', {}, '계약'),
          detailIdentity,
          el('dt', {}, '연결 보유주택'),
          el('dd', {}, detailPropertyHolding),
          el('dt', {}, '상태'),
          detailContractStatus,
          el('dt', {}, '금리'),
          detailRate,
          el('dt', {}, '잔액과 미납'),
          detailBalances,
          el('dt', {}, '계약 조건'),
          detailTerms,
          el('dt', {}, '상환 일정'),
          detailSchedule,
          el('dt', {}, '조기상환 조건'),
          detailPrepayment,
          el('dt', {}, 'DSR 포함'),
          detailDsr,
        ),
        el('h3', {}, '상환표'),
        el('ul', {}, ...installmentRows),
        el('h3', {}, '납부 이력'),
        el('ul', {}, ...paymentRows),
        historyStatus,
        historyLoadOlder,
      );

      const section = el(
        'section',
        { class: 'loans' },
        el('h1', {}, '신용과 대출'),
        el(
          'p',
          {},
          '표시된 신용 구간과 심사 조건은 게임용 규칙이며 실제 금융기관의 신용평점이나 상품이 아닙니다.',
        ),
        status,
        reload,
        el(
          'dl',
          {},
          el('dt', {}, '신용 구간'),
          creditBand,
          el('dt', {}, '공개 사유'),
          creditReasons,
          el('dt', {}, '전체 대출 잔액'),
          totalBalance,
          el('dt', {}, '다음 납입'),
          nextInstallment,
        ),
        el('h2', {}, '신규 신용대출 견적'),
        quoteStatus,
        quoteMessage,
        quoteForm,
        resultSection,
        el('h2', {}, '대출 상품'),
        catalogStatus,
        productList,
        el('h2', {}, '활성 계약'),
        loanList,
        el('h2', {}, '계약 상세와 이력'),
        detailSection,
        el('h2', {}, '계약 조기상환'),
        el(
          'p',
          {},
          '줄일 원금만 입력합니다. 조기상환 수수료와 실제 지갑 차감액은 서버가 확정합니다.',
        ),
        prepaymentStatus,
        prepaymentMessage,
        prepaymentForm,
        prepaymentResultSection,
        el(
          'nav',
          {},
          el('a', { href: '/', dataset: { link: '' } }, '대시보드'),
          ' · ',
          el('a', { href: '/life', dataset: { link: '' } }, '생활비 관리'),
        ),
      );
      host.replaceChildren(section);

      const products = h.useAsync((signal) => deps.api.listProducts(signal));
      const credit = h.useAsync((signal) => deps.api.getCredit(signal));
      const detail = h.useAsync(async (signal) => {
        const loanId = selectedDetailLoanId.peek();
        if (loanId === '') throw new Error('조회할 대출 계약을 선택해 주세요.');
        return deps.api.getDetail(loanId, signal);
      });
      const installmentHistory = h.useAsync(async (signal) => {
        const loanId = selectedDetailLoanId.peek();
        if (loanId === '') throw new Error('조회할 대출 계약을 선택해 주세요.');
        const before = historyBefore.peek();
        const query = before === undefined ? { limit: 50 } : { before, limit: 50 };
        return deps.api.getInstallmentHistory(loanId, query, signal);
      });
      const eligibleProducts = h.useComputed(() => {
        const state = products.state.get();
        return state.status === 'success'
          ? state.value.products.filter(
              (product) =>
                product.kind === 'unsecuredLoan' &&
                product.quoteEligible &&
                product.executionEligible,
            )
          : [];
      });
      const selectedProduct = h.useComputed(() =>
        eligibleProducts.get().find((product) => product.id === selectedProductId.get()),
      );
      const prepayableLoans = h.useComputed(() => {
        const creditState = credit.state.get();
        const productState = products.state.get();
        if (creditState.status !== 'success' || productState.status !== 'success') return [];
        const allowedProductIds = new Set(
          productState.value.products
            .filter((product) => product.prepaymentAllowed)
            .map((product) => product.id),
        );
        return creditState.value.activeLoans.filter(
          (loan) =>
            loan.status === 'active' &&
            loan.overdueKrw === 0 &&
            !loan.readOnly &&
            loan.remainingPrincipalKrw > 0 &&
            allowedProductIds.has(loan.productVersionId),
        );
      });
      const selectedPrepaymentLoan = h.useComputed(() =>
        prepayableLoans.get().find((loan) => loan.id === selectedLoanId.get()),
      );
      const selectedPrepaymentProduct = h.useComputed(() => {
        const productState = products.state.get();
        const loan = selectedPrepaymentLoan.get();
        return productState.status === 'success' && loan !== undefined
          ? productState.value.products.find((product) => product.id === loan.productVersionId)
          : undefined;
      });
      const canQuote = h.useComputed(() => {
        const current = snapshot.get();
        const product = selectedProduct.get();
        const amount = positiveSafeIntegerOrUndefined(principalRaw.get());
        return (
          current !== undefined &&
          current.characterName !== null &&
          current.autoSpeed === null &&
          product !== undefined &&
          product.rateStatus === 'available' &&
          amount !== undefined &&
          amount >= product.minimumPrincipalKrw &&
          amount <= product.maximumPrincipalKrw &&
          !advancing.get() &&
          !ordering.get() &&
          !quoteBusy.get() &&
          !executeBusy.get() &&
          !prepaymentBusy.get()
        );
      });
      const canExecute = h.useComputed(() => {
        const current = snapshot.get();
        const result = quoteResult.get();
        const originRunRevision = quoteRunRevision.get();
        const pending = pendingExecution(executionRetries, originRunRevision, result);
        return (
          current !== undefined &&
          result !== undefined &&
          result.decisionCode === 'eligible' &&
          (pending !== undefined ||
            (originRunRevision === current.runRevision &&
              result.createdGameDay === current.gameDay &&
              result.expiresGameDay === current.gameDay)) &&
          current.characterName !== null &&
          current.autoSpeed === null &&
          !advancing.get() &&
          !ordering.get() &&
          !quoteBusy.get() &&
          !executeBusy.get() &&
          !prepaymentBusy.get()
        );
      });
      const canPrepay = h.useComputed(() => {
        const current = snapshot.get();
        const pending = pendingPrepayment.get();
        if (
          current === undefined ||
          current.characterName === null ||
          current.autoSpeed !== null ||
          advancing.get() ||
          ordering.get() ||
          quoteBusy.get() ||
          executeBusy.get() ||
          prepaymentBusy.get()
        ) {
          return false;
        }
        if (pending !== undefined) {
          return pending.request.expectedRunRevision === current.runRevision;
        }
        const loan = selectedPrepaymentLoan.get();
        const product = selectedPrepaymentProduct.get();
        const amount = positiveSafeIntegerOrUndefined(prepaymentPrincipalRaw.get());
        return (
          loan !== undefined &&
          product?.prepaymentAllowed === true &&
          amount !== undefined &&
          amount <= loan.remainingPrincipalKrw
        );
      });
      const hasOlderHistory = h.useComputed(() => {
        const state = installmentHistory.state.get();
        return state.status === 'success' && state.value.nextBefore !== null;
      });
      const cursorRevision = h.useComputed(() => {
        const current = snapshot.get();
        return current === undefined
          ? 'unavailable'
          : `${current.runRevision}:${current.stateRevision}:${current.gameDay}`;
      });
      const currentRunRevision = h.useComputed(() => snapshot.get()?.runRevision);
      const quoteDayRevision = h.useComputed(() => {
        const current = snapshot.get();
        return current === undefined ? 'unavailable' : `${current.runRevision}:${current.gameDay}`;
      });
      const runReads = (): void => {
        products.run();
        credit.run();
      };
      const runSelectedLoanReads = (resetHistory: boolean): void => {
        if (selectedDetailLoanId.peek() === '') return;
        if (resetHistory) historyBefore.set(undefined);
        detail.run();
        installmentHistory.run();
      };
      const reloadAllReads = (): void => {
        runReads();
        runSelectedLoanReads(true);
      };

      h.useEventListener(reload, 'click', reloadAllReads);
      h.useEventListener(quoteProducts.element, 'change', () => {
        selectedProductId.set(quoteProducts.element.value);
      });
      h.useEventListener(principal, 'input', () => principalRaw.set(principal.value));
      h.useEventListener(quoteForm, 'submit', (event) => {
        event.preventDefault();
        void submitQuote().catch((error: unknown) => {
          quoteFeedback.set(error instanceof Error ? error.message : '대출 견적에 실패했습니다.');
        });
      });
      h.useEventListener(executeSubmit, 'click', () => {
        void submitExecution().catch((error: unknown) => {
          quoteFeedback.set(error instanceof Error ? error.message : '대출 실행에 실패했습니다.');
        });
      });
      h.useEventListener(prepaymentLoans.element, 'change', () => {
        selectedLoanId.set(prepaymentLoans.element.value);
      });
      h.useEventListener(prepaymentPrincipal, 'input', () => {
        prepaymentPrincipalRaw.set(prepaymentPrincipal.value);
      });
      h.useEventListener(prepaymentForm, 'submit', (event) => {
        event.preventDefault();
        void submitPrepayment().catch((error: unknown) => {
          prepaymentFeedback.set(
            error instanceof Error ? error.message : '조기상환에 실패했습니다.',
          );
        });
      });
      h.useEventListener(detailLoans.element, 'change', () => {
        selectedDetailLoanId.set(detailLoans.element.value);
      });
      h.useEventListener(detailReload, 'click', () => runSelectedLoanReads(true));
      h.useEventListener(historyLoadOlder, 'click', () => {
        const state = installmentHistory.state.peek();
        if (state.status !== 'success' || state.value.nextBefore === null) return;
        historyBefore.set(state.value.nextBefore);
        installmentHistory.run();
      });
      h.useWatch(selectedDetailLoanId, () => runSelectedLoanReads(true));
      h.useWatch(currentRunRevision, (current, previous) => {
        if (current === previous) return;
        selectedDetailLoanId.set('');
        historyBefore.set(undefined);
      });
      h.useWatch(cursorRevision, () => {
        runReads();
        runSelectedLoanReads(false);
      });
      h.useWatch(quoteDayRevision, () => {
        const result = quoteResult.peek();
        const originRunRevision = quoteRunRevision.peek();
        if (
          result !== undefined &&
          originRunRevision !== undefined &&
          pendingExecution(executionRetries, originRunRevision, result) !== undefined
        ) {
          return;
        }
        quoteResult.set(undefined);
        quoteRunRevision.set(undefined);
        quoteFeedback.set('');
      });
      h.useEffect(() => {
        const current = snapshot.get();
        pendingPrepayment.set(
          current === undefined ? undefined : prepaymentRetries.pendingForRun(current.runRevision),
        );
      });
      h.useEffect(() => {
        const state = products.state.get();
        renderProducts(state, productRows, catalogStatus);
        const selected = quoteProducts.setItems(eligibleProducts.get(), selectedProductId.peek());
        selectedProductId.set(selected);
      });
      h.useEffect(() => {
        const creditState = credit.state.get();
        renderCredit(creditState, {
          status,
          creditBand,
          creditReasons,
          totalBalance,
          nextInstallment,
          loanRows,
        });
        const loans = creditState.status === 'success' ? creditState.value.activeLoans : [];
        const selected = detailLoans.setItems(loans, selectedDetailLoanId.peek());
        selectedDetailLoanId.set(selected);
      });
      h.useEffect(() =>
        renderLoanDetail(selectedDetailLoanId.get(), detail.state.get(), {
          status: detailStatus,
          identity: detailIdentity,
          propertyHolding: detailPropertyHolding,
          contractStatus: detailContractStatus,
          rate: detailRate,
          balances: detailBalances,
          terms: detailTerms,
          schedule: detailSchedule,
          prepayment: detailPrepayment,
          dsr: detailDsr,
        }),
      );
      h.useEffect(() =>
        renderLoanHistory(
          selectedDetailLoanId.get(),
          installmentHistory.state.get(),
          installmentRows,
          paymentRows,
          historyStatus,
        ),
      );
      h.useEffect(() => {
        const pending = pendingPrepayment.get();
        if (pending === undefined) return;
        selectedLoanId.set(pending.loanId);
        const raw = String(pending.request.principalKrw);
        prepaymentPrincipalRaw.set(raw);
        prepaymentPrincipal.value = raw;
      });
      h.useEffect(() => {
        const selected = prepaymentLoans.setItems(
          prepayableLoans.get(),
          selectedLoanId.peek(),
          pendingPrepayment.get(),
        );
        selectedLoanId.set(selected);
      });
      h.useEffect(() => syncPrincipalBounds(principal, selectedProduct.get()));
      h.useEffect(() => syncPrepaymentBounds(prepaymentPrincipal, selectedPrepaymentLoan.get()));
      h.bindAttribute(quoteSubmit, 'disabled', () => !canQuote.get());
      h.bindAttribute(resultSection, 'hidden', () => quoteResult.get() === undefined);
      h.bindAttribute(
        executeSubmit,
        'hidden',
        () => quoteResult.get()?.decisionCode !== 'eligible',
      );
      h.bindAttribute(executeSubmit, 'disabled', () => !canExecute.get());
      h.bindAttribute(prepaymentSubmit, 'disabled', () => !canPrepay.get());
      h.bindAttribute(
        prepaymentLoans.element,
        'disabled',
        () => pendingPrepayment.get() !== undefined || prepaymentBusy.get(),
      );
      h.bindAttribute(
        prepaymentPrincipal,
        'disabled',
        () => pendingPrepayment.get() !== undefined || prepaymentBusy.get(),
      );
      h.bindAttribute(
        prepaymentResultSection,
        'hidden',
        () => prepaymentResult.get() === undefined,
      );
      h.bindAttribute(
        detailReload,
        'disabled',
        () => selectedDetailLoanId.get() === '' || detail.state.get().status === 'loading',
      );
      h.bindAttribute(historyLoadOlder, 'disabled', () => !hasOlderHistory.get());
      h.bindAttribute(historyLoadOlder, 'hidden', () => !hasOlderHistory.get());
      h.bindText(quoteStatus, () =>
        quoteAvailabilityText(
          snapshot.get(),
          advancing.get(),
          ordering.get(),
          quoteBusy.get() || executeBusy.get() || prepaymentBusy.get(),
          products.state.get(),
          selectedProduct.get(),
        ),
      );
      h.bindText(quoteMessage, () => quoteFeedback.get());
      h.bindText(executeStatus, () =>
        executionAvailabilityText(
          snapshot.get(),
          quoteRunRevision.get(),
          quoteResult.get(),
          pendingExecution(executionRetries, quoteRunRevision.get(), quoteResult.get()) !==
            undefined,
          advancing.get(),
          ordering.get(),
          quoteBusy.get() || executeBusy.get() || prepaymentBusy.get(),
        ),
      );
      h.bindText(resultDecision, () => quoteDecisionText(quoteResult.get()));
      h.bindText(resultReasons, () => quoteReasonsText(quoteResult.get()));
      h.bindText(resultRequested, () => quoteRequestedText(quoteResult.get()));
      h.bindText(resultValidity, () => quoteValidityText(quoteResult.get()));
      h.bindText(resultIncome, () => quoteIncomeText(quoteResult.get()));
      h.bindText(resultBalance, () => quoteBalanceText(quoteResult.get()));
      h.bindText(resultDsr, () => quoteDsrText(quoteResult.get()));
      h.bindText(resultStressRate, () => quoteStressRateText(quoteResult.get()));
      h.bindText(resultTerms, () => quoteTermsText(quoteResult.get()));
      h.bindText(resultFirstInstallment, () => quoteFirstInstallmentText(quoteResult.get()));
      h.bindText(prepaymentStatus, () =>
        prepaymentAvailabilityText(
          snapshot.get(),
          pendingPrepayment.get(),
          advancing.get(),
          ordering.get(),
          quoteBusy.get() || executeBusy.get() || prepaymentBusy.get(),
          credit.state.get(),
          products.state.get(),
          selectedPrepaymentLoan.get(),
          selectedPrepaymentProduct.get(),
        ),
      );
      h.bindText(prepaymentMessage, () => prepaymentFeedback.get());
      h.bindText(prepaymentPayment, () => prepaymentPaymentText(prepaymentResult.get()));
      h.bindText(prepaymentDebit, () => prepaymentDebitText(prepaymentResult.get()));
      h.bindText(prepaymentRemaining, () => prepaymentRemainingText(prepaymentResult.get()));
      h.bindText(prepaymentSchedule, () => prepaymentScheduleText(prepaymentResult.get()));

      runReads();

      async function submitQuote(): Promise<void> {
        const current = commandSnapshot(deps, 'quote');
        const product = selectedQuoteProduct(selectedProduct.peek());
        const principalKrw = quotePrincipal(principal.value, product);
        const request = quoteRetries.select(current, {
          productVersionId: product.id,
          principalKrw,
        });

        quoteBusy.set(true);
        quoteFeedback.set('서버에서 신용과 DSR을 심사하는 중입니다.');
        deps.store.set(paths.gameOrdering, true);
        try {
          const response = await deps.api.quote(request);
          quoteRetries.complete(request);
          deps.snapshots.apply(response.snapshot);
          quoteResult.set(response.result);
          quoteRunRevision.set(request.expectedRunRevision);
          quoteFeedback.set(
            response.replayed
              ? '이전에 완료된 같은 견적 결과를 다시 불러왔습니다.'
              : '대출 견적 심사가 끝났습니다.',
          );
        } catch (error) {
          quoteRetries.fail(request, error);
          throw quoteDisplayError(error);
        } finally {
          deps.store.set(paths.gameOrdering, false);
          quoteBusy.set(false);
        }
      }

      async function submitExecution(): Promise<void> {
        const current = commandSnapshot(deps, 'execute');
        const originRunRevision = quoteRunRevision.peek();
        const displayedResult = quoteResult.peek();
        const pending = pendingExecution(executionRetries, originRunRevision, displayedResult);
        const result = executableQuote(
          current,
          originRunRevision,
          displayedResult,
          pending !== undefined,
        );
        const request = pending ?? executionRetries.select(current, { quoteId: result.quoteId });

        executeBusy.set(true);
        quoteFeedback.set('최신 신용·소득·금리·DSR로 대출 실행을 다시 심사하는 중입니다.');
        deps.store.set(paths.gameOrdering, true);
        try {
          const response = await deps.api.execute(request);
          executionRetries.complete(request);
          deps.snapshots.apply(response.snapshot);
          quoteResult.set(undefined);
          quoteRunRevision.set(undefined);
          quoteFeedback.set(executionSuccessText(response));
          runReads();
        } catch (error) {
          executionRetries.fail(request, error);
          throw executionDisplayError(error);
        } finally {
          deps.store.set(paths.gameOrdering, false);
          executeBusy.set(false);
        }
      }

      async function submitPrepayment(): Promise<void> {
        const current = commandSnapshot(deps, 'prepay');
        const pending = currentPrepaymentPending(
          prepaymentRetries,
          pendingPrepayment.peek(),
          current.runRevision,
        );
        const command = selectPrepaymentCommand(
          prepaymentRetries,
          current,
          pending,
          selectedPrepaymentLoan.peek(),
          selectedPrepaymentProduct.peek(),
          prepaymentPrincipal.value,
        );

        pendingPrepayment.set(command);
        prepaymentBusy.set(true);
        prepaymentResult.set(undefined);
        prepaymentFeedback.set(
          pending === undefined
            ? '서버에서 조기상환 금액과 남은 일정을 확정하는 중입니다.'
            : '이전에 전송한 조기상환의 결과를 같은 명령으로 다시 확인하는 중입니다.',
        );
        deps.store.set(paths.gameOrdering, true);
        try {
          const response = await deps.api.prepay(command.loanId, command.request);
          prepaymentRetries.complete(command);
          pendingPrepayment.set(undefined);
          deps.snapshots.apply(response.snapshot);
          prepaymentResult.set(response.result);
          prepaymentFeedback.set(prepaymentSuccessText(response));
          runReads();
          runSelectedLoanReads(false);
        } catch (error) {
          prepaymentRetries.fail(command, error);
          pendingPrepayment.set(
            prepaymentRetries.pendingForRun(command.request.expectedRunRevision),
          );
          throw prepaymentDisplayError(error);
        } finally {
          deps.store.set(paths.gameOrdering, false);
          prepaymentBusy.set(false);
        }
      }
    },

    unmount() {},
  });
}

function renderProducts(
  state: AsyncState<LoanProductCatalog>,
  rows: readonly HTMLLIElement[],
  status: HTMLElement,
): void {
  if (state.status === 'success') {
    status.textContent =
      state.value.products.length === 0
        ? '현재 공개된 대출 상품이 없습니다.'
        : '현재 run에 게시된 대출 상품입니다.';
    updateRows(rows, state.value.products, productText);
    return;
  }
  updateRows(rows, [], productText);
  status.textContent =
    state.status === 'error' ? '대출 상품을 불러오지 못했습니다.' : '대출 상품을 불러오는 중…';
}

function renderCredit(state: AsyncState<CreditResponse>, nodes: CreditNodes): void {
  if (state.status === 'success') {
    const credit = state.value;
    nodes.status.textContent = '서버의 현재 상태를 표시합니다.';
    nodes.creditBand.textContent =
      credit.creditBand === null ? '사용할 수 없음' : CREDIT_BAND_LABEL[credit.creditBand];
    nodes.creditReasons.textContent = credit.creditReasons
      .map((reason) => CREDIT_REASON_LABEL[reason])
      .join(' · ');
    nodes.totalBalance.textContent = formatWon(credit.totalLoanBalanceKrw);
    nodes.nextInstallment.textContent = nextInstallmentText(credit);
    updateRows(nodes.loanRows, credit.activeLoans, loanText);
    return;
  }
  if (state.status === 'error') {
    nodes.status.textContent = '신용과 대출 정보를 불러오지 못했습니다.';
    nodes.creditBand.textContent = '-';
    nodes.creditReasons.textContent = '-';
    nodes.totalBalance.textContent = formatWon(0);
    nodes.nextInstallment.textContent = '-';
    updateRows(nodes.loanRows, [], loanText);
    return;
  }
  nodes.status.textContent = '대출 정보를 불러오는 중…';
}

function renderLoanDetail(
  selectedLoanId: string,
  state: AsyncState<LoanDetail>,
  nodes: LoanDetailNodes,
): void {
  if (selectedLoanId === '') {
    nodes.status.textContent = '활성 계약을 선택하면 상세를 조회합니다.';
    updateLoanDetailNodes(nodes, undefined);
    return;
  }
  if (state.status === 'success') {
    nodes.status.textContent = '서버의 계약 조건과 현재 잔액입니다.';
    updateLoanDetailNodes(nodes, state.value);
    return;
  }
  updateLoanDetailNodes(nodes, undefined);
  nodes.status.textContent =
    state.status === 'error'
      ? loanReadErrorText(state.error, '대출 계약 상세를 불러오지 못했습니다.')
      : '대출 계약 상세를 불러오는 중…';
}

function updateLoanDetailNodes(nodes: LoanDetailNodes, detail: LoanDetail | undefined): void {
  if (detail === undefined) {
    nodes.identity.textContent = '-';
    nodes.propertyHolding.textContent = '연결 보유주택 없음';
    nodes.propertyHolding.href = '/housing';
    nodes.propertyHolding.hidden = true;
    nodes.contractStatus.textContent = '-';
    nodes.rate.textContent = '-';
    nodes.balances.textContent = '-';
    nodes.terms.textContent = '-';
    nodes.schedule.textContent = '-';
    nodes.prepayment.textContent = '-';
    nodes.dsr.textContent = '-';
    return;
  }
  const linkedLease =
    detail.leaseContractId === null ? '' : ` · 연결 임대차 #${detail.leaseContractId}`;
  nodes.identity.textContent = `${detail.displayName} · 계약 #${detail.id} · 상품 #${detail.productVersionId} · ${PRODUCT_KIND_LABEL[detail.productKind]}${linkedLease}`;
  nodes.propertyHolding.hidden = detail.propertyHoldingId === null;
  nodes.propertyHolding.href =
    detail.propertyHoldingId === null
      ? '/housing'
      : `/housing?holding=${encodeURIComponent(detail.propertyHoldingId)}`;
  nodes.propertyHolding.textContent =
    detail.propertyHoldingId === null
      ? '연결 보유주택 없음'
      : `보유주택 #${detail.propertyHoldingId} 보기`;
  nodes.contractStatus.textContent = `${LOAN_STATUS_LABEL[detail.status]}${detail.readOnly ? ' · 조회 전용' : ''}`;
  nodes.rate.textContent =
    detail.currentAnnualRateBp === null
      ? '현재 금리 확인 불가'
      : formatBasisPoints(detail.currentAnnualRateBp);
  nodes.balances.textContent = `최초 원금 ${formatWon(detail.originalPrincipalKrw)} · 남은 원금 ${formatWon(detail.remainingPrincipalKrw)} · 확정 이자 ${formatWon(detail.accruedInterestKrw)} · 확정 비용 ${formatWon(detail.accruedFeeKrw)} · 연체 ${formatWon(detail.overdueKrw)}`;
  nodes.terms.textContent = loanDetailTermsText(detail);
  nodes.schedule.textContent = loanDetailScheduleText(detail);
  nodes.prepayment.textContent = loanDetailPrepaymentText(detail);
  nodes.dsr.textContent = detail.dsrIncluded ? '포함' : '미포함';
}

function loanDetailTermsText(detail: LoanDetail): string {
  const term = detail.termMonths === null ? '기간 없음' : `${detail.termMonths}개월`;
  const installments =
    detail.totalInstallments === null ? '회차 없음' : `총 ${detail.totalInstallments}회`;
  const maturity =
    detail.maturityGameDay === null
      ? '계약 만기 없음'
      : `계약 만기 game day ${detail.maturityGameDay}`;
  return `${repaymentMethodText(detail.repaymentMethod)} · ${term} · ${installments} · 활성 game day ${detail.activatedGameDay} · ${maturity}`;
}

function loanDetailScheduleText(detail: LoanDetail): string {
  const finalDue =
    detail.finalInstallmentDueGameDay === null
      ? '남은 최종 지급일 없음'
      : `남은 최종 지급 game day ${detail.finalInstallmentDueGameDay}`;
  const next =
    detail.nextInstallmentNo === null ? '다음 회차 없음' : `다음 #${detail.nextInstallmentNo}`;
  const oldest =
    detail.oldestUnpaidDueGameDay === null
      ? '미납 지급일 없음'
      : `가장 오래된 미납 game day ${detail.oldestUnpaidDueGameDay}`;
  return `${finalDue} · ${next} · ${oldest}`;
}

function loanDetailPrepaymentText(detail: LoanDetail): string {
  if (
    !detail.prepaymentAllowed ||
    detail.prepaymentFeePpm === null ||
    detail.prepaymentEffect === null
  ) {
    return '계약 조건상 조기상환 불가';
  }
  return `허용 · 수수료율 ${detail.prepaymentFeePpm.toLocaleString('ko-KR')}ppm · ${PREPAYMENT_EFFECT_LABEL[detail.prepaymentEffect]}`;
}

function renderLoanHistory(
  selectedLoanId: string,
  state: AsyncState<LoanInstallmentHistoryResponse>,
  installmentRows: readonly HTMLLIElement[],
  paymentRows: readonly HTMLLIElement[],
  status: HTMLElement,
): void {
  if (selectedLoanId === '') {
    updateRows(installmentRows, [], loanInstallmentHistoryText);
    updateRows(paymentRows, [], loanPaymentHistoryText);
    status.textContent = '계약을 선택하면 상환표와 납부 이력을 조회합니다.';
    return;
  }
  if (state.status === 'success') {
    updateRows(installmentRows, state.value.installments, loanInstallmentHistoryText);
    updateRows(paymentRows, state.value.payments, loanPaymentHistoryText);
    status.textContent = loanHistoryStatusText(state.value);
    return;
  }
  updateRows(installmentRows, [], loanInstallmentHistoryText);
  updateRows(paymentRows, [], loanPaymentHistoryText);
  status.textContent =
    state.status === 'error'
      ? loanReadErrorText(state.error, '상환표와 납부 이력을 불러오지 못했습니다.')
      : '상환표와 납부 이력을 불러오는 중…';
}

function loanHistoryStatusText(history: LoanInstallmentHistoryResponse): string {
  const more = [
    history.hasMoreInstallments ? '이전 상환표 있음' : '상환표 끝',
    history.hasMorePayments ? '이전 납부 이력 있음' : '납부 이력 끝',
  ].join(' · ');
  return `현재 window: 상환표 ${history.installments.length}건 · 납부 ${history.payments.length}건 · ${more}`;
}

function loanInstallmentHistoryText(installment: LoanInstallmentHistoryItem): string {
  return `#${installment.installmentNo} · ${INSTALLMENT_STATUS_LABEL[installment.status]} · 지급 game day ${installment.dueGameDay} · 이자기간 ${installment.interestPeriodStartGameDay}~${installment.dueGameDay} (${installment.elapsedDays}일) · ${formatBasisPoints(installment.annualRateBp)} · 기초원금 ${formatWon(installment.openingPrincipalKrw)} · 예정 비용 ${formatWon(installment.scheduledFeeKrw)}, 이자 ${formatWon(installment.scheduledInterestKrw)}, 원금 ${formatWon(installment.scheduledPrincipalKrw)} · 납부 비용 ${formatWon(installment.paidFeeKrw)}, 이자 ${formatWon(installment.paidInterestKrw)}, 원금 ${formatWon(installment.paidPrincipalKrw)} · 남은 금액 ${formatWon(installment.remainingDueKrw)} · 일정 revision ${installment.scheduleRevision}`;
}

function loanPaymentHistoryText(payment: LoanPaymentHistoryItem): string {
  const allocations = payment.allocations
    .map(
      (allocation) =>
        `${ALLOCATION_KIND_LABEL[allocation.kind]} ${formatWon(allocation.amountKrw)}`,
    )
    .join(' · ');
  return `#${payment.paymentNo} · ${PAYMENT_KIND_LABEL[payment.kind]} · game day ${payment.gameDay} · ${formatWon(payment.amountKrw)} · ${allocations}`;
}

function loanReadErrorText(error: unknown, fallback: string): string {
  return error instanceof LoanCommandError && error.code === 'loanNotFound'
    ? '현재 run에서 대출 계약을 찾을 수 없습니다.'
    : fallback;
}

function createFixedProductSelect(): FixedProductSelect {
  const element = el('select', {
    name: 'productVersionId',
    attrs: { 'aria-label': '견적을 받을 대출 상품' },
  });
  const placeholder = el('option', { value: '' }, '견적 상품을 불러오는 중');
  const options = Array.from({ length: 16 }, () => el('option'));
  element.append(placeholder, ...options);

  return {
    element,
    setItems(items, selectedId) {
      const available = items.filter((product) => product.rateStatus === 'available');
      const nextSelected = available.some((product) => product.id === selectedId)
        ? selectedId
        : (available[0]?.id ?? '');
      placeholder.textContent =
        items.length === 0
          ? '견적 가능한 상품이 없습니다'
          : available.length === 0
            ? '현재 금리를 확인할 수 없습니다'
            : '견적 상품을 선택하세요';
      placeholder.disabled = nextSelected !== '';
      placeholder.hidden = nextSelected !== '';
      for (const [index, option] of options.entries()) {
        const product = items[index];
        option.hidden = product === undefined;
        option.disabled = product === undefined || product.rateStatus !== 'available';
        option.value = product?.id ?? '';
        option.textContent = product === undefined ? '' : quoteProductOptionText(product);
      }
      element.value = nextSelected;
      return nextSelected;
    },
  };
}

function createFixedLoanSelect(): FixedLoanSelect {
  const element = el('select', {
    name: 'loanId',
    attrs: { 'aria-label': '조기상환할 대출 계약' },
  });
  const placeholder = el('option', { value: '' }, '조기상환 가능한 계약을 불러오는 중');
  const pendingOption = el('option', { attrs: { hidden: '' } });
  const options = Array.from({ length: 8 }, () => el('option'));
  element.append(placeholder, pendingOption, ...options);

  return {
    element,
    setItems(items, selectedId, pending) {
      const nextSelected = nextPrepaymentLoanId(items, selectedId, pending);
      updatePrepaymentPlaceholder(placeholder, items.length, nextSelected, pending);
      updatePendingLoanOption(pendingOption, items, pending);
      for (const [index, option] of options.entries()) {
        updatePrepaymentLoanOption(option, items[index]);
      }
      element.value = nextSelected;
      return nextSelected;
    },
  };
}

function createFixedDetailLoanSelect(): FixedDetailLoanSelect {
  const element = el('select', {
    name: 'detailLoanId',
    attrs: { 'aria-label': '상세와 이력을 조회할 대출 계약' },
  });
  const placeholder = el('option', { value: '' }, '조회할 계약을 불러오는 중');
  const retainedOption = el('option', { attrs: { hidden: '' } });
  const options = Array.from({ length: 8 }, () => el('option'));
  element.append(placeholder, retainedOption, ...options);

  return {
    element,
    setItems(items, selectedId) {
      const nextSelected = nextDetailLoanId(items, selectedId);
      placeholder.textContent =
        items.length === 0 && selectedId === ''
          ? '현재 조회할 활성 계약이 없습니다'
          : '상세 계약을 선택하세요';
      placeholder.disabled = nextSelected !== '';
      placeholder.hidden = nextSelected !== '';
      updateRetainedDetailOption(retainedOption, items, nextSelected);
      for (const [index, option] of options.entries()) {
        updatePrepaymentLoanOption(option, items[index]);
      }
      element.value = nextSelected;
      return nextSelected;
    },
  };
}

function nextDetailLoanId(items: readonly LoanSummary[], selectedId: string): string {
  if (selectedId !== '') return selectedId;
  return items[0]?.id ?? '';
}

function updateRetainedDetailOption(
  option: HTMLOptionElement,
  items: readonly LoanSummary[],
  selectedId: string,
): void {
  const retained = selectedId !== '' && !items.some((loan) => loan.id === selectedId);
  option.hidden = !retained;
  option.value = retained ? selectedId : '';
  option.textContent = retained ? `계약 #${selectedId} · 종료/이력 조회` : '';
}

function nextPrepaymentLoanId(
  items: readonly LoanSummary[],
  selectedId: string,
  pending: LoanPrepaymentCommand | undefined,
): string {
  if (pending !== undefined) return pending.loanId;
  if (items.some((loan) => loan.id === selectedId)) return selectedId;
  return items[0]?.id ?? '';
}

function updatePrepaymentPlaceholder(
  placeholder: HTMLOptionElement,
  itemCount: number,
  selectedId: string,
  pending: LoanPrepaymentCommand | undefined,
): void {
  placeholder.textContent =
    itemCount === 0 && pending === undefined
      ? '현재 조기상환 가능한 계약이 없습니다'
      : '조기상환할 계약을 선택하세요';
  placeholder.disabled = selectedId !== '';
  placeholder.hidden = selectedId !== '';
}

function updatePendingLoanOption(
  option: HTMLOptionElement,
  items: readonly LoanSummary[],
  pending: LoanPrepaymentCommand | undefined,
): void {
  const pendingInItems = pending !== undefined && items.some((loan) => loan.id === pending.loanId);
  if (pending === undefined || pendingInItems) {
    option.hidden = true;
    option.value = '';
    option.textContent = '';
    return;
  }
  option.hidden = false;
  option.value = pending.loanId;
  option.textContent = `계약 #${pending.loanId} · 이전 요청 결과 확인 중`;
}

function updatePrepaymentLoanOption(
  option: HTMLOptionElement,
  loan: LoanSummary | undefined,
): void {
  option.hidden = loan === undefined;
  option.disabled = loan === undefined;
  option.value = loan?.id ?? '';
  option.textContent = loan === undefined ? '' : prepaymentLoanOptionText(loan);
}

function productText(product: LoanProduct): string {
  const rate =
    product.currentAnnualRateBp === null
      ? '금리 확정 불가'
      : `현재 ${formatBasisPoints(product.currentAnnualRateBp)}`;
  return `${product.displayName} · ${PRODUCT_KIND_LABEL[product.kind]} · ${rate} · ${product.termMonths}개월 · ${repaymentMethodText(product.repaymentMethod)} · ${formatWon(product.minimumPrincipalKrw)}~${formatWon(product.maximumPrincipalKrw)} · 중도상환 비용 ${product.prepaymentFeePpm.toLocaleString('ko-KR')}ppm`;
}

function quoteProductOptionText(product: LoanProduct): string {
  const rate =
    product.currentAnnualRateBp === null
      ? '금리 확인 불가'
      : formatBasisPoints(product.currentAnnualRateBp);
  return `${product.displayName} · ${rate} · ${formatWon(product.minimumPrincipalKrw)}~${formatWon(product.maximumPrincipalKrw)}`;
}

function prepaymentLoanOptionText(loan: LoanSummary): string {
  return `${loan.displayName} (#${loan.id}) · 남은 원금 ${formatWon(loan.remainingPrincipalKrw)}`;
}

function loanText(loan: LoanSummary): string {
  const rate =
    loan.currentAnnualRateBp === null
      ? '금리 확인 불가'
      : formatBasisPoints(loan.currentAnnualRateBp);
  return `${loan.displayName} · ${LOAN_STATUS_LABEL[loan.status]} · ${rate} · 남은 원금 ${formatWon(loan.remainingPrincipalKrw)} · 연체 ${formatWon(loan.overdueKrw)}${loan.readOnly ? ' · 조회 전용' : ''}`;
}

function nextInstallmentText(credit: CreditResponse): string {
  const next = credit.nextLoanInstallment;
  if (next === null) return '예정된 납입 없음';
  return `game day ${next.dueGameDay} · ${formatWon(next.remainingDueKrw)} (원금 ${formatWon(next.principalKrw)}, 이자 ${formatWon(next.interestKrw)}, 비용 ${formatWon(next.feeKrw)})`;
}

function quoteAvailabilityText(
  snapshot: GameSnapshot | undefined,
  advancing: boolean,
  ordering: boolean,
  busy: boolean,
  products: AsyncState<LoanProductCatalog>,
  product: LoanProduct | undefined,
): string {
  if (snapshot === undefined) return '게임 상태를 기다리는 중입니다.';
  if (snapshot.characterName === null) return '캐릭터를 만든 뒤 대출 견적을 받을 수 있습니다.';
  if (snapshot.autoSpeed !== null) return '대출 견적을 받으려면 자동 진행을 멈추세요.';
  if (advancing || ordering || busy) return '다른 게임 명령을 처리하는 중입니다.';
  if (products.status === 'loading' || products.status === 'idle') {
    return '견적 가능한 상품을 불러오는 중입니다.';
  }
  if (products.status === 'error') return '상품을 다시 불러온 뒤 견적을 요청해 주세요.';
  if (product === undefined) return '현재 견적 가능한 신용대출 상품이 없습니다.';
  if (product.rateStatus === 'rateUnavailable') return '현재 상품 금리를 확인할 수 없습니다.';
  return `${formatWon(product.minimumPrincipalKrw)}부터 ${formatWon(product.maximumPrincipalKrw)}까지 정수 원 단위로 입력하세요.`;
}

function executionAvailabilityText(
  snapshot: GameSnapshot | undefined,
  quoteRunRevision: number | undefined,
  result: LoanQuoteResult | undefined,
  pending: boolean,
  advancing: boolean,
  ordering: boolean,
  busy: boolean,
): string {
  if (result === undefined) return '';
  if (result.decisionCode !== 'eligible') return '적격 견적만 대출 계약으로 실행할 수 있습니다.';
  if (snapshot === undefined) return '게임 상태를 기다리는 중입니다.';
  if (snapshot.autoSpeed !== null) return '대출을 실행하려면 자동 진행을 멈추세요.';
  if (advancing || ordering || busy) return '다른 게임 명령을 처리하는 중입니다.';
  if (pending) return '이전에 전송한 대출 실행의 결과를 같은 명령으로 다시 확인합니다.';
  if (quoteRunRevision !== snapshot.runRevision) return '현재 run에서 새 견적을 받아 주세요.';
  if (result.createdGameDay !== snapshot.gameDay || result.expiresGameDay !== snapshot.gameDay) {
    return '견적이 만료되었습니다. 현재 game day에서 새 견적을 받아 주세요.';
  }
  return `${formatWon(result.requestedPrincipalKrw)} 대출을 최신 상태로 다시 심사해 실행합니다.`;
}

function prepaymentAvailabilityText(
  snapshot: GameSnapshot | undefined,
  pending: LoanPrepaymentCommand | undefined,
  advancing: boolean,
  ordering: boolean,
  busy: boolean,
  credit: AsyncState<CreditResponse>,
  products: AsyncState<LoanProductCatalog>,
  loan: LoanSummary | undefined,
  product: LoanProduct | undefined,
): string {
  const unavailable = prepaymentEnvironmentText(snapshot, advancing, ordering, busy);
  if (unavailable !== undefined) return unavailable;
  if (snapshot === undefined) return '게임 상태를 기다리는 중입니다.';
  if (pending !== undefined) {
    return pending.request.expectedRunRevision === snapshot.runRevision
      ? '이전에 전송한 조기상환의 결과를 같은 UUID와 원래 cursor로 다시 확인합니다.'
      : '현재 run의 조기상환 계약을 다시 선택해 주세요.';
  }
  const readStatus = prepaymentReadStatus(credit, products);
  if (readStatus !== undefined) return readStatus;
  if (loan === undefined || product === undefined || !product.prepaymentAllowed) {
    return '현재 조기상환 가능한 정상 계약이 없습니다.';
  }
  return `1원부터 ${formatWon(loan.remainingPrincipalKrw)}까지 입력하세요. 수수료율 ${product.prepaymentFeePpm.toLocaleString('ko-KR')}ppm · ${PREPAYMENT_EFFECT_LABEL[product.prepaymentEffect]}. 실제 수수료는 서버가 확정합니다.`;
}

function prepaymentEnvironmentText(
  snapshot: GameSnapshot | undefined,
  advancing: boolean,
  ordering: boolean,
  busy: boolean,
): string | undefined {
  if (snapshot === undefined) return '게임 상태를 기다리는 중입니다.';
  if (snapshot.characterName === null) return '캐릭터를 만든 뒤 대출을 조기상환할 수 있습니다.';
  if (snapshot.autoSpeed !== null) return '조기상환하려면 자동 진행을 멈추세요.';
  if (advancing || ordering || busy) return '다른 게임 명령을 처리하는 중입니다.';
  return undefined;
}

function prepaymentReadStatus(
  credit: AsyncState<CreditResponse>,
  products: AsyncState<LoanProductCatalog>,
): string | undefined {
  if (credit.status === 'loading' || credit.status === 'idle') {
    return '조기상환 가능한 계약을 불러오는 중입니다.';
  }
  if (credit.status === 'error') return '신용 정보를 다시 불러온 뒤 조기상환해 주세요.';
  if (products.status === 'loading' || products.status === 'idle') {
    return '조기상환 조건을 불러오는 중입니다.';
  }
  if (products.status === 'error') return '상품 정보를 다시 불러온 뒤 조기상환해 주세요.';
  return undefined;
}

function pendingExecution(
  policy: LoanExecutionRetryPolicy,
  runRevision: number | undefined,
  result: LoanQuoteResult | undefined,
): LoanExecutionRequest | undefined {
  if (runRevision === undefined || result === undefined) return undefined;
  return policy.pending(runRevision, { quoteId: result.quoteId });
}

function currentPrepaymentPending(
  policy: LoanPrepaymentRetryPolicy,
  displayed: LoanPrepaymentCommand | undefined,
  runRevision: number,
): LoanPrepaymentCommand | undefined {
  if (displayed?.request.expectedRunRevision === runRevision) return displayed;
  return policy.pendingForRun(runRevision);
}

function selectPrepaymentCommand(
  policy: LoanPrepaymentRetryPolicy,
  snapshot: GameSnapshot,
  pending: LoanPrepaymentCommand | undefined,
  loan: LoanSummary | undefined,
  product: LoanProduct | undefined,
  principalRaw: string,
): LoanPrepaymentCommand {
  if (pending !== undefined) return pending;
  const selected = selectedPrepaymentContract(loan, product);
  return policy.select(snapshot, {
    loanId: selected.id,
    principalKrw: prepaymentPrincipalAmount(principalRaw, selected),
  });
}

function commandSnapshot(
  deps: LoansViewDeps,
  action: 'quote' | 'execute' | 'prepay',
): GameSnapshot {
  const state = deps.store.getState();
  const snapshot = state.game.snapshot;
  const actionText =
    action === 'quote' ? '대출 견적을 요청' : action === 'execute' ? '대출을 실행' : '조기상환';
  if (snapshot === undefined || snapshot.characterName === null) {
    throw new Error(`캐릭터를 만든 뒤 ${actionText}할 수 있습니다.`);
  }
  if (snapshot.autoSpeed !== null) {
    throw new Error(`자동 진행을 멈춘 뒤 ${actionText}해 주세요.`);
  }
  if (state.game.advancing || state.game.ordering) {
    throw new Error(`다른 게임 명령이 끝난 뒤 ${actionText}해 주세요.`);
  }
  return snapshot;
}

function executableQuote(
  snapshot: GameSnapshot,
  quoteRunRevision: number | undefined,
  result: LoanQuoteResult | undefined,
  pending: boolean,
): LoanQuoteResult {
  if (result === undefined || result.decisionCode !== 'eligible') {
    throw new Error('실행 가능한 대출 견적을 먼저 받아 주세요.');
  }
  if (!pending && quoteRunRevision !== snapshot.runRevision) {
    throw new Error('현재 run에서 대출 견적을 다시 받아 주세요.');
  }
  if (
    !pending &&
    (result.createdGameDay !== snapshot.gameDay || result.expiresGameDay !== snapshot.gameDay)
  ) {
    throw new Error('견적이 만료되었습니다. 현재 game day에서 다시 받아 주세요.');
  }
  return result;
}

function selectedQuoteProduct(product: LoanProduct | undefined): LoanProduct {
  if (product === undefined) throw new Error('견적 가능한 신용대출 상품을 선택해 주세요.');
  if (product.rateStatus !== 'available') {
    throw new Error('현재 금리를 확인할 수 없는 상품입니다.');
  }
  return product;
}

function quotePrincipal(raw: string, product: LoanProduct): number {
  const amount = positiveSafeIntegerOrUndefined(raw);
  if (amount === undefined) throw new Error('신청 원금은 1원 이상의 정수로 입력해 주세요.');
  if (amount < product.minimumPrincipalKrw || amount > product.maximumPrincipalKrw) {
    throw new Error(
      `신청 원금은 ${formatWon(product.minimumPrincipalKrw)}부터 ${formatWon(product.maximumPrincipalKrw)}까지 입력해 주세요.`,
    );
  }
  return amount;
}

function selectedPrepaymentContract(
  loan: LoanSummary | undefined,
  product: LoanProduct | undefined,
): LoanSummary {
  if (loan === undefined || product === undefined || loan.productVersionId !== product.id) {
    throw new Error('조기상환 가능한 대출 계약을 선택해 주세요.');
  }
  if (
    loan.status !== 'active' ||
    loan.overdueKrw !== 0 ||
    loan.readOnly ||
    loan.remainingPrincipalKrw === 0 ||
    !product.prepaymentAllowed
  ) {
    throw new Error('정상 상태이며 연체가 없는 변경 가능 계약만 조기상환할 수 있습니다.');
  }
  return loan;
}

function prepaymentPrincipalAmount(raw: string, loan: LoanSummary | undefined): number {
  if (loan === undefined) throw new Error('조기상환 가능한 대출 계약을 선택해 주세요.');
  const amount = positiveSafeIntegerOrUndefined(raw);
  if (amount === undefined) throw new Error('줄일 원금은 1원 이상의 정수로 입력해 주세요.');
  if (amount > loan.remainingPrincipalKrw) {
    throw new Error(`줄일 원금은 ${formatWon(loan.remainingPrincipalKrw)} 이하여야 합니다.`);
  }
  return amount;
}

function positiveSafeIntegerOrUndefined(raw: string): number | undefined {
  const value = Number(raw);
  return Number.isSafeInteger(value) && value > 0 ? value : undefined;
}

function syncPrincipalBounds(input: HTMLInputElement, product: LoanProduct | undefined): void {
  if (product === undefined) {
    input.removeAttribute('min');
    input.removeAttribute('max');
    input.placeholder = '견적 상품을 먼저 선택하세요';
    return;
  }
  input.min = String(product.minimumPrincipalKrw);
  input.max = String(product.maximumPrincipalKrw);
  input.placeholder = `${product.minimumPrincipalKrw}~${product.maximumPrincipalKrw}`;
}

function syncPrepaymentBounds(input: HTMLInputElement, loan: LoanSummary | undefined): void {
  input.min = '1';
  if (loan === undefined) {
    input.removeAttribute('max');
    input.placeholder = '조기상환할 계약을 먼저 선택하세요';
    return;
  }
  input.max = String(loan.remainingPrincipalKrw);
  input.placeholder = `1~${loan.remainingPrincipalKrw}`;
}

function quoteDisplayError(error: unknown): Error {
  if (error instanceof LoanCommandError) return new Error(error.message);
  return new Error('견적 결과를 확인하지 못했습니다. 같은 상품과 원금으로 다시 시도해 주세요.');
}

function executionDisplayError(error: unknown): Error {
  if (error instanceof LoanCommandError) return new Error(error.message);
  return new Error('대출 실행 결과를 확인하지 못했습니다. 같은 견적 실행을 다시 시도해 주세요.');
}

function prepaymentDisplayError(error: unknown): Error {
  if (error instanceof LoanCommandError) return new Error(error.message);
  return new Error(
    '조기상환 결과를 확인하지 못했습니다. 같은 계약과 원금으로 결과를 다시 확인해 주세요.',
  );
}

function executionSuccessText(response: LoanExecutionResponse): string {
  const result = response.result;
  const outcome = response.replayed
    ? '이전에 완료된 대출 실행 결과를 다시 불러왔습니다.'
    : '대출 실행이 완료되었습니다.';
  return `${outcome} 계약 #${result.loanId} · ${formatWon(result.principalKrw)} · ${formatBasisPoints(result.annualRateBp)} · ${result.termMonths}개월 ${repaymentMethodText(result.repaymentMethod)} · 만기 game day ${result.maturityGameDay} · 첫 납입 ${formatWon(result.firstInstallment.totalKrw)}`;
}

function prepaymentSuccessText(response: LoanPrepaymentResponse): string {
  const outcome = response.replayed
    ? '이전에 완료된 조기상환 결과를 다시 불러왔습니다.'
    : '조기상환이 완료되었습니다.';
  return `${outcome} 지급 #${response.result.paymentId} · 총 ${formatWon(response.result.totalDebitedKrw)}`;
}

function prepaymentPaymentText(result: LoanPrepaymentResult | undefined): string {
  return result === undefined
    ? '-'
    : `지급 #${result.paymentId} · 계약 #${result.loanId} · game day ${result.appliedGameDay}`;
}

function prepaymentDebitText(result: LoanPrepaymentResult | undefined): string {
  return result === undefined
    ? '-'
    : `총 ${formatWon(result.totalDebitedKrw)} (원금 ${formatWon(result.principalKrw)}, 수수료 ${formatWon(result.feeKrw)})`;
}

function prepaymentRemainingText(result: LoanPrepaymentResult | undefined): string {
  return result === undefined
    ? '-'
    : `${LOAN_STATUS_LABEL[result.status]} · 남은 원금 ${formatWon(result.remainingPrincipalKrw)}`;
}

function prepaymentScheduleText(result: LoanPrepaymentResult | undefined): string {
  if (result === undefined) return '-';
  const effect = PREPAYMENT_EFFECT_LABEL[result.prepaymentEffect];
  const next = result.nextInstallment;
  if (next === null) return `${effect} · 남은 회차 없음`;
  const finalDue =
    result.finalInstallmentDueGameDay === null
      ? ''
      : ` · 마지막 지급 game day ${result.finalInstallmentDueGameDay}`;
  return `${effect} · ${result.remainingInstallments}회 · 다음 #${next.installmentNo} game day ${next.dueGameDay} ${formatWon(next.totalKrw)} (원금 ${formatWon(next.principalKrw)}, 이자 ${formatWon(next.interestKrw)}, 비용 ${formatWon(next.feeKrw)})${finalDue}`;
}

function quoteDecisionText(result: LoanQuoteResult | undefined): string {
  return result === undefined
    ? '-'
    : `${QUOTE_DECISION_LABEL[result.decisionCode]} (#${result.quoteId})`;
}

function quoteReasonsText(result: LoanQuoteResult | undefined): string {
  return result === undefined
    ? '-'
    : result.decisionReasons.map((reason) => QUOTE_REASON_LABEL[reason]).join(' · ');
}

function quoteRequestedText(result: LoanQuoteResult | undefined): string {
  return result === undefined
    ? '-'
    : `${formatWon(result.requestedPrincipalKrw)} · 상품 #${result.productVersionId}`;
}

function quoteValidityText(result: LoanQuoteResult | undefined): string {
  return result === undefined
    ? '-'
    : `game day ${result.createdGameDay} 생성 · game day ${result.expiresGameDay}까지`;
}

function quoteIncomeText(result: LoanQuoteResult | undefined): string {
  if (result === undefined) return '-';
  if (result.verifiedAnnualIncomeKrw === null) return '인정 소득 없음';
  return `${formatWon(result.verifiedAnnualIncomeKrw)} / 년 · 현재 활성 근로계약`;
}

function quoteBalanceText(result: LoanQuoteResult | undefined): string {
  return result === undefined
    ? '-'
    : `현재 ${formatWon(result.existingLoanBalanceKrw)} → 실행 가정 ${formatWon(result.postExecutionBalanceKrw)}`;
}

function quoteDsrText(result: LoanQuoteResult | undefined): string {
  if (result === undefined) return '-';
  if (!result.dsrApplied) return 'DSR gate 미적용';
  if (result.dsr === null) return 'DSR gate 적용 · 인정 소득 없음';
  return `분자 ${formatWon(result.dsr.numeratorKrw)} / 분모 ${formatWon(result.dsr.denominatorKrw)} · ${result.dsr.ratioPpm.toLocaleString('ko-KR')}ppm / 한도 ${result.dsr.limitPpm.toLocaleString('ko-KR')}ppm`;
}

function quoteStressRateText(result: LoanQuoteResult | undefined): string {
  return result === undefined ? '-' : formatBasisPoints(result.stressRateBp);
}

function quoteTermsText(result: LoanQuoteResult | undefined): string {
  if (result === undefined) return '-';
  const terms = result.quotedTerms;
  return `${formatBasisPoints(terms.annualRateBp)} · ${terms.termMonths}개월 · ${repaymentMethodText(terms.repaymentMethod)}`;
}

function quoteFirstInstallmentText(result: LoanQuoteResult | undefined): string {
  if (result === undefined) return '-';
  const first = result.quotedTerms.firstInstallment;
  return `game day ${first.dueGameDay} · 합계 ${formatWon(first.totalKrw)} (원금 ${formatWon(first.principalKrw)}, 이자 ${formatWon(first.interestKrw)}, 비용 ${formatWon(first.feeKrw)})`;
}

function repaymentMethodText(method: LoanRepaymentMethod): string {
  switch (method) {
    case 'equalPrincipal':
      return '원금균등';
    case 'levelPayment':
      return '원리금균등';
    case 'bullet':
      return '만기일시';
  }
}

function createRows(count: number): HTMLLIElement[] {
  return Array.from({ length: count }, () => {
    const row = el('li', {}) as HTMLLIElement;
    row.hidden = true;
    return row;
  });
}

function updateRows<T>(
  rows: readonly HTMLLIElement[],
  items: readonly T[],
  textOf: (item: T) => string,
): void {
  for (const [index, row] of rows.entries()) {
    const item = items[index];
    row.hidden = item === undefined;
    row.textContent = item === undefined ? '' : textOf(item);
  }
}
