import type { AuthApi } from '../../api/auth-api.js';
import {
  type BondOrderRequest,
  BondOrderRequestSchema,
  type BondOrderResult,
  type BondPositionSummary,
  type BondProductCatalog,
  type CashContractSummary,
  type CashProduct,
  type CashProductCatalog,
  type CmaAccountCloseDraft,
  CmaAccountCloseDraftSchema,
  type CmaAccountOpenDraft,
  CmaAccountOpenDraftSchema,
  type CmaAccountSummary,
  type DepositCloseDraft,
  DepositCloseDraftSchema,
  type DepositOpenDraft,
  DepositOpenDraftSchema,
  type DepositProtectionSummary,
  type FinanceCommandRequest,
  type FinanceTransferDraft,
  FinanceTransferDraftSchema,
  type FinancialAccount,
  type FinancialIncomeAssessment,
  type FinancialIncomeYear,
  GAME_SPEEDS,
  type GameCommandCursor,
  type GameSnapshot,
  type GameSpeed,
  type GoldAccountOpenRequest,
  GoldAccountOpenRequestSchema,
  type GoldAccountSummary,
  type GoldOrderRequest,
  GoldOrderRequestSchema,
  type GoldOrderResult,
  type GoldProductCatalog,
  type GoldWithdrawalRequest,
  GoldWithdrawalRequestSchema,
  type GoldWithdrawalResult,
  type IsaAccountCloseDraft,
  IsaAccountCloseDraftSchema,
  type IsaAccountSummary,
  type LedgerPage,
  type LedgerTransaction,
  type LlxDistributionEntitlement,
  MarketHistoryDaysSchema,
  type MarketHistoryPoint,
  type OfflineProgress,
  type PendingSettlementSummary,
  type PensionAccountSummary,
  type PensionStartDraft,
  PensionStartDraftSchema,
  type PensionWithdrawalDraft,
  PensionWithdrawalDraftSchema,
  type PhysicalGoldHolding,
  type PortfolioExecution,
  type PortfolioOrderDraft,
  PortfolioOrderDraftSchema,
  type RunFinalization,
  STEP_DAYS,
  type StepUnit,
  type TaxAccountOpenDraft,
  TaxAccountOpenDraftSchema,
} from '../../api/contracts.js';
import {
  FinanceCommandError,
  type GameApi,
  GameCommandError,
  OfflineProgressError,
  PortfolioOrderError,
} from '../../api/game-api.js';
import { asFormValidator } from '../../api/zod-adapters.js';
import { type CloseChartPoint, createCloseChart } from '../../lib/chart/index.js';
import { bindText, el } from '../../lib/dom/index.js';
import { type FieldSpec, renderForm } from '../../lib/form/index.js';
import { type AsyncState, createHooks } from '../../lib/hooks/index.js';
import type { Store } from '../../lib/store/index.js';
import type { ToastQueue } from '../../lib/toast/index.js';
import type { View, ViewFactory } from '../../lib/view/index.js';
import { createFinanceAssetRetryPolicy } from '../asset-retry/index.js';
import {
  createCmaAccountCloseRetryPolicy,
  createCmaAccountOpenRetryPolicy,
  createDepositCloseRetryPolicy,
  createDepositOpenRetryPolicy,
} from '../cash-product-retry/index.js';
import { createFinanceTransferRetryPolicy } from '../finance-retry/index.js';
import {
  CONNECTION_LABEL,
  formatBasisPoints,
  formatGameDate,
  formatReturnPpm,
  formatWon,
  LEDGER_SOURCE_LABEL,
  MARKET_REGIME_LABEL,
} from '../format.js';
import { type AdvanceRetryPolicy, createAdvanceRetryPolicy } from '../game-command-retry/index.js';
import type { GameStateWriter } from '../game-state/index.js';
import { createOrderRetryPolicy } from '../order-retry/index.js';
import { type AppState, paths } from '../state.js';
import {
  createIsaAccountCloseRetryPolicy,
  createPensionStartRetryPolicy,
  createPensionWithdrawalRetryPolicy,
  createTaxAccountOpenRetryPolicy,
} from '../tax-account-retry/index.js';
import { createEquitySearchPanel } from './equity-search.js';

export interface DashboardDeps {
  readonly store: Store<AppState>;
  readonly snapshots: GameStateWriter;
  readonly api: GameApi;
  readonly auth: AuthApi;
  readonly toasts: ToastQueue;
  readonly createOrderId: () => string;
}

const FINANCE_COMMAND_FIELDS = {
  commandId: true,
  expectedRunRevision: true,
  expectedStateRevision: true,
  expectedGameDay: true,
} as const satisfies Readonly<Record<keyof FinanceCommandRequest, true>>;

type BondOrderDraft = Omit<BondOrderRequest, keyof FinanceCommandRequest>;
type GoldAccountOpenDraft = Omit<GoldAccountOpenRequest, keyof FinanceCommandRequest>;
type GoldOrderDraft = Omit<GoldOrderRequest, keyof FinanceCommandRequest>;
type GoldWithdrawalDraft = Omit<GoldWithdrawalRequest, keyof FinanceCommandRequest>;

const BondOrderDraftSchema = BondOrderRequestSchema.omit(FINANCE_COMMAND_FIELDS);
const GoldAccountOpenDraftSchema = GoldAccountOpenRequestSchema.omit(FINANCE_COMMAND_FIELDS);
const GoldOrderDraftSchema = GoldOrderRequestSchema.omit(FINANCE_COMMAND_FIELDS);
const GoldWithdrawalDraftSchema = GoldWithdrawalRequestSchema.omit(FINANCE_COMMAND_FIELDS);
const GoldWithdrawalDraftFormValidator = asFormValidator(GoldWithdrawalDraftSchema);

const ASSET_ACCOUNT_OPTION_CAPACITY = 32;
const BOND_SERIES_OPTION_CAPACITY = 160;

const BOND_ORDER_FIELDS: readonly FieldSpec[] = [
  {
    name: 'accountId',
    label: '거래 계좌',
    kind: 'select',
    options: fixedSelectOptions(ASSET_ACCOUNT_OPTION_CAPACITY, '계좌를 선택하세요'),
  },
  {
    name: 'seriesId',
    label: '국채 시리즈',
    kind: 'select',
    options: fixedSelectOptions(BOND_SERIES_OPTION_CAPACITY, '시리즈를 선택하세요'),
  },
  {
    name: 'side',
    label: '주문 방향',
    kind: 'select',
    options: [
      { value: 'buy', label: '매수' },
      { value: 'sell', label: '매도' },
    ],
  },
  { name: 'bondUnits', label: '수량', kind: 'number', help: '1 ~ 100,000단위' },
];

const GOLD_ACCOUNT_OPEN_FIELDS: readonly FieldSpec[] = [
  {
    name: 'type',
    label: '계좌 종류',
    kind: 'select',
    options: [{ value: 'krxGold', label: 'KRX 금현물' }],
  },
  {
    name: 'productVersionId',
    label: '금 상품',
    kind: 'select',
    options: fixedSelectOptions(1, '상품을 선택하세요'),
  },
];

const GOLD_ORDER_FIELDS: readonly FieldSpec[] = [
  {
    name: 'accountId',
    label: '금 계좌',
    kind: 'select',
    options: fixedSelectOptions(ASSET_ACCOUNT_OPTION_CAPACITY, '금 계좌를 선택하세요'),
  },
  {
    name: 'side',
    label: '주문 방향',
    kind: 'select',
    options: [
      { value: 'buy', label: '매수' },
      { value: 'sell', label: '매도' },
    ],
  },
  { name: 'quantityGram', label: '수량', kind: 'number', help: '1g 이상 정수' },
];

const GOLD_WITHDRAWAL_FIELDS: readonly FieldSpec[] = [
  {
    name: 'accountId',
    label: '금 계좌',
    kind: 'select',
    options: fixedSelectOptions(ASSET_ACCOUNT_OPTION_CAPACITY, '금 계좌를 선택하세요'),
  },
  {
    name: 'barSizeGram',
    label: '실물 bar 규격',
    kind: 'select',
    options: [
      { value: '100', label: '100g' },
      { value: '1000', label: '1kg' },
    ],
  },
  { name: 'barCount', label: 'bar 개수', kind: 'number', help: '1개 이상 정수' },
];

const ORDER_FIELDS: readonly FieldSpec[] = [
  {
    name: 'accountId',
    label: '거래 계좌',
    kind: 'select',
    options: fixedSelectOptions(ASSET_ACCOUNT_OPTION_CAPACITY, '계좌를 선택하세요'),
  },
  {
    name: 'side',
    label: '주문 방향',
    kind: 'select',
    options: [
      { value: 'buy', label: '매수' },
      { value: 'sell', label: '매도' },
    ],
  },
  { name: 'quantity', label: '수량', kind: 'number', help: '1 ~ 1,000,000주' },
];

const TRANSFER_FIELDS: readonly FieldSpec[] = [
  {
    name: 'accountId',
    label: '계좌 ID',
    kind: 'text',
    help: '일반계좌·CMA·ISA·연금계좌 ID를 입력할 수 있습니다.',
  },
  {
    name: 'direction',
    label: '이체 방향',
    kind: 'select',
    options: [
      { value: 'walletToAccount', label: '지갑 → 계좌' },
      { value: 'accountToWallet', label: '계좌 → 지갑' },
    ],
  },
  { name: 'amountKrw', label: '이체 금액', kind: 'number', help: '1원 이상' },
];

const CMA_OPEN_FIELDS: readonly FieldSpec[] = [
  {
    name: 'type',
    label: '계좌 종류',
    kind: 'select',
    options: [{ value: 'cma', label: 'CMA' }],
  },
  { name: 'productVersionId', label: 'CMA 상품 ID', kind: 'text' },
];

const CMA_CLOSE_FIELDS: readonly FieldSpec[] = [
  { name: 'accountId', label: '종료할 CMA 계좌 ID', kind: 'text' },
];

const DEPOSIT_OPEN_FIELDS: readonly FieldSpec[] = [
  {
    name: 'kind',
    label: '상품 종류',
    kind: 'select',
    options: [
      { value: 'termDeposit', label: '정기예금' },
      { value: 'installmentSavings', label: '정기적금' },
    ],
  },
  { name: 'productVersionId', label: '상품 ID', kind: 'text' },
  { name: 'settlementAccountId', label: '정산 계좌 ID', kind: 'text' },
  {
    name: 'amountKrw',
    label: '가입 금액',
    kind: 'number',
    help: '예금은 원금, 적금은 회당 납입액입니다.',
  },
];

const DEPOSIT_CLOSE_FIELDS: readonly FieldSpec[] = [
  { name: 'contractId', label: '중도해지할 계약 ID', kind: 'text' },
];

const TAX_ACCOUNT_OPEN_FIELDS: readonly FieldSpec[] = [
  {
    name: 'type',
    label: '절세계좌 종류',
    kind: 'select',
    options: [
      { value: 'isaGeneral', label: '일반형 ISA' },
      { value: 'isaLowIncome', label: '서민형 ISA' },
      { value: 'pensionSavings', label: '연금저축' },
      { value: 'irp', label: 'IRP' },
    ],
  },
];

const ISA_CLOSE_FIELDS: readonly FieldSpec[] = [
  { name: 'accountId', label: '해지할 ISA 계좌 ID', kind: 'text' },
];

const PENSION_START_FIELDS: readonly FieldSpec[] = [
  { name: 'accountId', label: '개시할 연금계좌 ID', kind: 'text' },
  { name: 'paymentYears', label: '지급 기간', kind: 'number', help: '5 ~ 100년' },
  { name: 'lifetime', label: '종신 지급', kind: 'checkbox' },
];

const PENSION_WITHDRAWAL_FIELDS: readonly FieldSpec[] = [
  { name: 'accountId', label: '인출할 연금계좌 ID', kind: 'text' },
  { name: 'amountKrw', label: 'gross 인출 금액', kind: 'number', help: '1원 이상' },
  {
    name: 'type',
    label: '인출 유형',
    kind: 'select',
    options: [
      { value: 'pension', label: '연금 수령' },
      { value: 'nonPension', label: '연금외 수령' },
      { value: 'unavoidable', label: '부득이한 사유' },
    ],
  },
  {
    name: 'reason',
    label: 'IRP 중도인출 사유',
    kind: 'select',
    help: '해당 사유가 없으면 “없음”을 선택합니다.',
    options: [
      { value: '', label: '없음' },
      { value: 'homePurchase', label: '주택 구입' },
      { value: 'housingDeposit', label: '주거 임차보증금' },
      { value: 'medicalCare', label: '의료비' },
      { value: 'disaster', label: '재난' },
      { value: 'bankruptcy', label: '파산' },
      { value: 'rehabilitation', label: '개인회생' },
      { value: 'securedLoanRepayment', label: '담보대출 상환' },
    ],
  },
];

const MAX_HISTORY_DAYS = 3660;

const HISTORY_PERIODS = [
  { days: 30, label: '최근 30일' },
  { days: 90, label: '최근 90일' },
  { days: 365, label: '최근 1년' },
  { days: MAX_HISTORY_DAYS, label: '최대 10년' },
] as const;

const DEFAULT_HISTORY_DAYS = 365;
const LEDGER_PAGE_SIZE = 50;
const ACCOUNT_TABLE_CAPACITY = 32;
const CASH_PRODUCT_TABLE_CAPACITY = 100;
const CMA_ACCOUNT_TABLE_CAPACITY = 32;
const CASH_CONTRACT_TABLE_CAPACITY = 100;
const DEPOSIT_PROTECTION_TABLE_CAPACITY = 16;
const ISA_ACCOUNT_TABLE_CAPACITY = 1;
const PENSION_ACCOUNT_TABLE_CAPACITY = 2;

interface OrderSubmitterDeps {
  readonly store: Store<AppState>;
  readonly snapshots: GameStateWriter;
  readonly api: GameApi;
  readonly toasts: ToastQueue;
  readonly createOrderId: () => string;
}

interface FinanceTransferSubmitterDeps {
  readonly store: Store<AppState>;
  readonly snapshots: GameStateWriter;
  readonly api: GameApi;
  readonly toasts: ToastQueue;
  readonly createCommandId: () => string;
}

interface CashProductSubmitterDeps {
  readonly store: Store<AppState>;
  readonly snapshots: GameStateWriter;
  readonly api: GameApi;
  readonly toasts: ToastQueue;
  readonly createCommandId: () => string;
  readonly refreshLatestLedger: () => void;
}

interface TaxAccountSubmitterDeps {
  readonly store: Store<AppState>;
  readonly snapshots: GameStateWriter;
  readonly api: GameApi;
  readonly toasts: ToastQueue;
  readonly createCommandId: () => string;
  readonly refreshLatestLedger: () => void;
}

interface AssetSubmitterDeps {
  readonly store: Store<AppState>;
  readonly snapshots: GameStateWriter;
  readonly api: GameApi;
  readonly toasts: ToastQueue;
  readonly createCommandId: () => string;
  readonly refreshLatestLedger: () => void;
}

interface FixedTable<T> {
  setItems(items: readonly T[]): void;
}

interface FixedRow<T> {
  readonly element: HTMLTableRowElement;
  setItem(item: T | undefined): void;
}

interface HistoryTable {
  setPoints(points: readonly MarketHistoryPoint[]): void;
}

interface HistoryRow {
  readonly element: HTMLTableRowElement;
  setPoint(point: MarketHistoryPoint | undefined): void;
}

interface AccountTable {
  setAccounts(accounts: readonly FinancialAccount[]): void;
}

interface AccountRow {
  readonly element: HTMLTableRowElement;
  setAccount(account: FinancialAccount | undefined): void;
}

interface LedgerTable {
  setTransactions(transactions: readonly LedgerTransaction[]): void;
}

interface LedgerRow {
  readonly element: HTMLTableRowElement;
  setTransaction(transaction: LedgerTransaction | undefined): void;
}

/**
 * The dashboard, which is the reference for this project's render convention:
 *  - build the DOM once in `mount`
 *  - update through hooks (bindText, bindAttribute) that touch only the changed node
 *  - register every subscription and listener with ctx.bag, released on unmount
 */
export function createDashboardView(deps: DashboardDeps): ViewFactory {
  const advanceRetries = createAdvanceRetryPolicy({ createCommandId: deps.createOrderId });

  return (): View => {
    let root: HTMLElement | undefined;

    return {
      mount(host, ctx) {
        const { store, snapshots, api, auth, toasts, createOrderId } = deps;
        const h = createHooks(ctx.bag);
        const equitySearchPanel = createEquitySearchPanel({ api, bag: ctx.bag });

        const snapshot = h.useStoreValue(store, paths.gameSnapshot, (s) => s.game.snapshot);
        const advancing = h.useStoreValue(store, paths.gameAdvancing, (s) => s.game.advancing);
        const ordering = h.useStoreValue(store, paths.gameOrdering, (s) => s.game.ordering);
        const connection = h.useStoreValue(
          store,
          paths.connectionStatus,
          (s) => s.connection.status,
        );
        const characterName = h.useStoreValue(
          store,
          paths.gameSnapshot,
          (s) => s.game.snapshot?.characterName,
        );
        const authStatus = h.useStoreValue(store, paths.authStatus, (s) => s.auth.status);
        const account = h.useStoreValue(store, paths.authUser, (s) => s.auth.user);
        const historyDays = h.useSignal(DEFAULT_HISTORY_DAYS);
        const historyRequest = h.useAsync((signal) =>
          api.getMarketHistory(historyDays.peek(), signal),
        );
        const ledgerBefore = h.useSignal<string | undefined>(undefined);
        const ledgerRequest = h.useAsync((signal) =>
          api.getFinanceLedger(ledgerBefore.peek(), LEDGER_PAGE_SIZE, signal),
        );
        const cashProductRequest = h.useAsync(() => api.listCashProducts());
        const bondCatalogRequest = h.useAsync((signal) => api.listBonds(signal));
        const goldProductRequest = h.useAsync((signal) => api.listGoldProducts(signal));
        const finalizationRequest = h.useAsync((signal) => {
          const runRevision = snapshot.peek()?.runRevision;
          if (runRevision === undefined) {
            return Promise.reject(new Error('run revision is unavailable'));
          }
          return api.getRunFinalization(runRevision, signal);
        });
        const offlineProgress = h.useSignal<OfflineProgress | undefined>(undefined);
        const offlineProgressRequest = h.useAsync(async (signal) => {
          const status = await api.getOfflineProgress(signal);
          offlineProgress.set(status);
          return status;
        });
        const offlineProgressDesired = h.useSignal<boolean | undefined>(undefined);
        const offlineProgressUpdate = h.useAsync(async (signal) => {
          const current = offlineProgress.peek();
          const enabled = offlineProgressDesired.peek();
          if (current === undefined || enabled === undefined) {
            throw new Error('offline progress status is unavailable');
          }
          const updated = await api.setOfflineProgress(
            { expectedRevision: current.revision, enabled },
            signal,
          );
          offlineProgress.set(updated);
          return updated;
        });
        const refreshLatestLedger = h.useDebounced(() => {
          ledgerBefore.set(undefined);
          ledgerRequest.run();
        }, 100);

        const accountText = h.useComputed(() => {
          const user = account.get();
          if (user === undefined) return '';
          return user.displayName ?? user.email ?? '';
        });
        const dateText = h.useComputed(() => {
          const current = snapshot.get();
          return current === undefined ? '—' : formatGameDate(current.startDate, current.gameDay);
        });
        const dayText = h.useComputed(() => `${snapshot.get()?.gameDay ?? '—'}`);
        const cashText = h.useComputed(() => moneyText(snapshot.get()?.cashKrw));
        const netWorthText = h.useComputed(() => moneyText(snapshot.get()?.netWorthKrw));
        const portfolioValueText = h.useComputed(() =>
          moneyText(snapshot.get()?.portfolio.marketValueKrw),
        );
        const llxPositions = h.useComputed(() => snapshot.get()?.portfolio.positions ?? []);
        const positionQuantityText = h.useComputed(() => {
          const quantity = llxPositions
            .get()
            .reduce((total, position) => total + position.quantity, 0);
          return `${quantity}주`;
        });
        const positionAverageText = h.useComputed(() => {
          const positions = llxPositions.get();
          if (positions.length === 0) return '—';
          return positions
            .map((position) => `#${position.accountId} ${formatWon(position.averagePriceKrw)}`)
            .join(' / ');
        });
        const positionValueText = h.useComputed(() =>
          moneyText(snapshot.get()?.portfolio.marketValueKrw),
        );
        const marketIndexText = h.useComputed(() => {
          const index = snapshot.get()?.market.index;
          return index === undefined ? '—' : `${index.name} (${index.symbol})`;
        });
        const marketCloseText = h.useComputed(() => {
          return moneyText(snapshot.get()?.market.index.closeKrw);
        });
        const llxCloseText = h.useComputed(() => {
          return moneyText(snapshot.get()?.market.m2Factors?.llxCloseKrw);
        });
        const goldCloseText = h.useComputed(() => {
          return moneyText(snapshot.get()?.market.m2Factors?.goldCloseKrwPerGram);
        });
        const cpiIndexText = h.useComputed(() => {
          const cpiIndex = snapshot.get()?.market.m2Factors?.cpiIndex;
          return cpiIndex === undefined ? '—' : cpiIndex.toLocaleString('ko-KR');
        });
        const marketReturnText = h.useComputed(() => {
          const returnPpm = snapshot.get()?.market.index.dailyReturnPpm;
          return returnPpm === undefined ? '—' : formatReturnPpm(returnPpm);
        });
        const marketRegimeText = h.useComputed(() => {
          const regime = snapshot.get()?.market.regime;
          return regime === undefined ? '—' : MARKET_REGIME_LABEL[regime];
        });
        const marketOpenText = h.useComputed(() => {
          const open = snapshot.get()?.market.open;
          return open === undefined ? '—' : open ? '개장' : '휴장';
        });
        const policyRateText = h.useComputed(() =>
          rateText(snapshot.get()?.market.rates?.policyRateBp),
        );
        const treasury3mText = h.useComputed(() =>
          rateText(snapshot.get()?.market.rates?.treasury3mBp),
        );
        const treasury1yText = h.useComputed(() =>
          rateText(snapshot.get()?.market.rates?.treasury1yBp),
        );
        const treasury3yText = h.useComputed(() =>
          rateText(snapshot.get()?.market.rates?.treasury3yBp),
        );
        const treasury10yText = h.useComputed(() =>
          rateText(snapshot.get()?.market.rates?.treasury10yBp),
        );
        const statusText = h.useComputed(() => {
          const status = connection.get();
          return CONNECTION_LABEL[status] ?? status;
        });
        const autoSpeed = h.useComputed(() => snapshot.get()?.autoSpeed ?? null);
        const autoStatusText = h.useComputed(() => {
          if (snapshot.get() === undefined) return '—';
          const speed = autoSpeed.get();
          return speed === null ? '정지' : `x${speed}`;
        });
        const finalizationStatusText = h.useComputed(() =>
          finalizationStatusLabel(finalizationRequest.state.get()),
        );
        const finalizationLinesText = h.useComputed(() => {
          const state = finalizationRequest.state.get();
          if (state.status !== 'success' || state.value.status !== 'completed') return '—';
          return state.value.lines
            .map(
              (line) =>
                `${line.lineNo}. ${line.componentKey}: gross ${formatWon(line.grossKrw)}, cost ${formatWon(line.costKrw)}, tax ${formatWon(line.taxKrw)}, net ${formatWon(line.netKrw)}`,
            )
            .join('\n');
        });
        const offlineProgressStatusText = h.useComputed(() => {
          return offlineProgressStatusTextOf(
            offlineProgress.get(),
            offlineProgressRequest.state.get(),
          );
        });
        const offlineProgressButtonText = h.useComputed(() =>
          offlineProgress.get()?.enabled === true ? '오프라인 진행 끄기' : '오프라인 진행 켜기',
        );
        const offlineProgressBlocked = h.useComputed(() => {
          const status = offlineProgress.get();
          return (
            status === undefined ||
            !status.available ||
            offlineProgressUpdate.state.get().status === 'loading'
          );
        });
        const gameReady = h.useComputed(() => {
          const name = snapshot.get()?.characterName;
          return name !== undefined && name !== null;
        });
        const militaryStatusText = h.useComputed(() => {
          const status = snapshot.get()?.career.militaryStatus;
          if (status === undefined) return '—';
          return {
            unserved: '미필',
            serving: '복무 중',
            completed: '복무 완료',
            exempt: '면제',
          }[status];
        });
        const militaryServiceText = h.useComputed(() => {
          const service = snapshot.get()?.career.activeMilitaryService;
          if (service === undefined) return '—';
          if (service === null) return '진행 중인 복무 없음';
          return `${service.displayName} · ${service.creditedServiceDays}/${service.totalServiceDays}일 · 종료 ${service.endGameDay}일차`;
        });
        const militarySavingsText = h.useComputed(() => {
          const contracts = snapshot.get()?.career.activeMilitarySavings;
          if (contracts === undefined) return '—';
          if (contracts.length === 0) return '활성 계약 없음';
          return contracts
            .map(
              (contract) =>
                `#${contract.id} ${contract.institutionKey} ${formatWon(contract.principalKrw)}`,
            )
            .join(' / ');
        });
        const lifeSummaryText = h.useComputed(() => {
          const life = snapshot.get()?.life;
          if (life === undefined) return '—';
          const insurance = insuranceSnapshotSummary(life);
          if (life.rateStatus === 'rateUnavailable')
            return `이 월드에서는 생활비 비율을 사용할 수 없음 · ${insurance}`;
          const month = life.currentMonth;
          if (month === null)
            return `월 청구 준비 중 · 필수 미납 ${formatWon(life.totalEssentialArrearKrw)} · ${insurance}`;
          return `${month.yearMonth.year}-${String(month.yearMonth.month).padStart(2, '0')} ${formatWon(month.totalGrossKrw)} · 필수 미납 ${formatWon(life.totalEssentialArrearKrw)} · ${insurance}`;
        });
        const defaultFinanceAccountId = h.useComputed(() => {
          const accounts = snapshot.get()?.finance.accounts ?? [];
          return (
            accounts.find((item) => item.status === 'open' && item.isDefault)?.id ??
            accounts.find((item) => item.status === 'open')?.id
          );
        });
        const defaultTradeAccountId = h.useComputed(() => {
          const current = snapshot.get();
          const accounts = current?.finance.accounts ?? [];
          const supportsM2dTaxAccounts =
            current !== undefined && current.finance.productBundle !== null;
          return (
            accounts.find(
              (item) =>
                item.status === 'open' &&
                item.isDefault &&
                accountTypeAllowsLlx(item.type, supportsM2dTaxAccounts),
            )?.id ??
            accounts.find(
              (item) =>
                item.status === 'open' && accountTypeAllowsLlx(item.type, supportsM2dTaxAccounts),
            )?.id
          );
        });
        const defaultDepositSettlementAccountId = h.useComputed(() => {
          const accounts = snapshot.get()?.finance.accounts ?? [];
          return (
            accounts.find(
              (item) =>
                item.status === 'open' && item.isDefault && item.type === 'taxableBrokerage',
            )?.id ??
            accounts.find((item) => item.status === 'open' && item.type === 'taxableBrokerage')?.id
          );
        });
        const defaultBondAccountId = h.useComputed(() => {
          const accounts = snapshot.get()?.finance.accounts ?? [];
          return (
            accounts.find(
              (item) =>
                item.status === 'open' && item.isDefault && accountTypeAllowsBond(item.type),
            )?.id ??
            accounts.find((item) => item.status === 'open' && accountTypeAllowsBond(item.type))?.id
          );
        });
        const defaultGoldAccountId = h.useComputed(() => {
          const current = snapshot.get()?.finance;
          return (
            current?.goldAccounts[0]?.accountId ??
            current?.accounts.find((item) => item.status === 'open' && item.type === 'krxGold')?.id
          );
        });
        const policySetText = h.useComputed(() => {
          const policySet = snapshot.get()?.finance.policySet;
          return policySet === undefined ? '—' : `${policySet.key} (${policySet.basisDate})`;
        });
        const historyCursor = h.useComputed(() => {
          const current = snapshot.get();
          if (current === undefined) return '';
          return `${current.market.world}:${current.runRevision}:${current.gameDay}`;
        });
        const historyStatusText = h.useComputed(() => {
          const state = historyRequest.state.get();
          if (state.status === 'idle') return '조회할 기간을 선택하세요.';
          if (state.status === 'loading') return '시장 히스토리를 불러오는 중입니다.';
          if (state.status === 'error') return '시장 히스토리를 불러오지 못했습니다.';
          return `${state.value.points.length}개 일봉, ${state.value.throughGameDay}일차까지`;
        });
        const ledgerStatusText = h.useComputed(() => ledgerStatusTextOf(ledgerRequest.state.get()));
        const cashProductStatusText = h.useComputed(() =>
          cashProductStatusTextOf(cashProductRequest.state.get()),
        );
        const bondCatalogStatusText = h.useComputed(() =>
          bondCatalogStatusTextOf(bondCatalogRequest.state.get()),
        );
        const goldProductStatusText = h.useComputed(() =>
          goldProductStatusTextOf(goldProductRequest.state.get()),
        );
        const financeMutationBlocked = h.useComputed(
          () => !gameReady.get() || advancing.get() || ordering.get() || autoSpeed.get() !== null,
        );
        const m2dReady = h.useComputed(() => {
          const current = snapshot.get();
          return (
            current !== undefined &&
            current.finance.productBundle !== null &&
            current.market.m2Factors !== null
          );
        });
        const marketAssetMutationBlocked = h.useComputed(
          () =>
            financeMutationBlocked.get() || !m2dReady.get() || snapshot.get()?.market.open !== true,
        );
        const currentTaxYearText = h.useComputed(
          () => snapshot.get()?.finance.currentTaxYear.taxYear.toString() ?? '—',
        );
        const grossFinancialIncomeText = h.useComputed(() =>
          moneyText(snapshot.get()?.finance.currentTaxYear.grossFinancialIncomeKrw),
        );
        const withheldIncomeTaxText = h.useComputed(() =>
          moneyText(snapshot.get()?.finance.currentTaxYear.withheldIncomeTaxKrw),
        );
        const withheldLocalIncomeTaxText = h.useComputed(() =>
          moneyText(snapshot.get()?.finance.currentTaxYear.withheldLocalIncomeTaxKrw),
        );
        const ledgerRunCursor = h.useComputed(() => {
          const current = snapshot.get();
          return current === undefined ? '' : `${current.runRevision}:${current.stateRevision}`;
        });
        const bondCatalogCursor = h.useComputed(() => {
          const current = snapshot.get();
          return current === undefined
            ? ''
            : `${current.market.world}:${current.runRevision}:${current.gameDay}`;
        });
        const goldProductCursor = h.useComputed(() => {
          const current = snapshot.get();
          return current === undefined ? '' : `${current.market.world}:${current.runRevision}`;
        });

        const orderForm = renderForm<PortfolioOrderDraft>(
          {
            idPrefix: 'order',
            fields: ORDER_FIELDS,
            validator: asFormValidator(PortfolioOrderDraftSchema),
            submitLabel: '주문 제출',
          },
          {
            initial: { accountId: '', side: 'buy', quantity: 1 },
            onSubmit: createOrderSubmitter({
              store,
              snapshots,
              api,
              toasts,
              createOrderId,
            }),
          },
        );
        ctx.bag.add(orderForm);

        const transferForm = renderForm<FinanceTransferDraft>(
          {
            idPrefix: 'transfer',
            fields: TRANSFER_FIELDS,
            validator: asFormValidator(FinanceTransferDraftSchema),
            submitLabel: '이체 실행',
          },
          {
            initial: { accountId: '', direction: 'walletToAccount', amountKrw: 1 },
            onSubmit: createFinanceTransferSubmitter({
              store,
              snapshots,
              api,
              toasts,
              createCommandId: createOrderId,
            }),
          },
        );
        ctx.bag.add(transferForm);

        const cashProductSubmitterDeps: CashProductSubmitterDeps = {
          store,
          snapshots,
          api,
          toasts,
          createCommandId: createOrderId,
          refreshLatestLedger,
        };
        const cmaOpenForm = renderForm<CmaAccountOpenDraft>(
          {
            idPrefix: 'cma-open',
            fields: CMA_OPEN_FIELDS,
            validator: asFormValidator(CmaAccountOpenDraftSchema),
            submitLabel: 'CMA 개설',
          },
          {
            initial: { type: 'cma', productVersionId: '' },
            onSubmit: createCmaAccountOpenSubmitter(cashProductSubmitterDeps),
          },
        );
        ctx.bag.add(cmaOpenForm);

        const cmaCloseForm = renderForm<CmaAccountCloseDraft>(
          {
            idPrefix: 'cma-close',
            fields: CMA_CLOSE_FIELDS,
            validator: asFormValidator(CmaAccountCloseDraftSchema),
            submitLabel: 'CMA 종료',
          },
          {
            initial: { accountId: '' },
            onSubmit: createCmaAccountCloseSubmitter(cashProductSubmitterDeps),
          },
        );
        ctx.bag.add(cmaCloseForm);

        const depositOpenForm = renderForm<DepositOpenDraft>(
          {
            idPrefix: 'deposit-open',
            fields: DEPOSIT_OPEN_FIELDS,
            validator: asFormValidator(DepositOpenDraftSchema),
            submitLabel: '예금·적금 가입',
          },
          {
            initial: {
              kind: 'termDeposit',
              productVersionId: '',
              settlementAccountId: '',
              amountKrw: 100_000,
            },
            onSubmit: createDepositOpenSubmitter(cashProductSubmitterDeps),
          },
        );
        ctx.bag.add(depositOpenForm);

        const depositCloseForm = renderForm<DepositCloseDraft>(
          {
            idPrefix: 'deposit-close',
            fields: DEPOSIT_CLOSE_FIELDS,
            validator: asFormValidator(DepositCloseDraftSchema),
            submitLabel: '예금·적금 중도해지',
          },
          {
            initial: { contractId: '' },
            onSubmit: createDepositCloseSubmitter(cashProductSubmitterDeps),
          },
        );
        ctx.bag.add(depositCloseForm);
        const taxAccountSubmitterDeps: TaxAccountSubmitterDeps = {
          store,
          snapshots,
          api,
          toasts,
          createCommandId: createOrderId,
          refreshLatestLedger,
        };
        const taxAccountOpenForm = renderForm<TaxAccountOpenDraft>(
          {
            idPrefix: 'tax-account-open',
            fields: TAX_ACCOUNT_OPEN_FIELDS,
            validator: asFormValidator(TaxAccountOpenDraftSchema),
            submitLabel: '절세계좌 개설',
          },
          {
            initial: { type: 'isaGeneral' },
            onSubmit: createTaxAccountOpenSubmitter(taxAccountSubmitterDeps),
          },
        );
        ctx.bag.add(taxAccountOpenForm);

        const isaCloseForm = renderForm<IsaAccountCloseDraft>(
          {
            idPrefix: 'isa-close',
            fields: ISA_CLOSE_FIELDS,
            validator: asFormValidator(IsaAccountCloseDraftSchema),
            submitLabel: 'ISA 해지',
          },
          {
            initial: { accountId: '' },
            onSubmit: createIsaAccountCloseSubmitter(taxAccountSubmitterDeps),
          },
        );
        ctx.bag.add(isaCloseForm);

        const pensionStartForm = renderForm<PensionStartDraft>(
          {
            idPrefix: 'pension-start',
            fields: PENSION_START_FIELDS,
            validator: asFormValidator(PensionStartDraftSchema),
            submitLabel: '연금 개시',
          },
          {
            initial: { accountId: '', paymentYears: 10, lifetime: false },
            onSubmit: createPensionStartSubmitter(taxAccountSubmitterDeps),
          },
        );
        ctx.bag.add(pensionStartForm);

        const pensionWithdrawalForm = renderForm<PensionWithdrawalDraft>(
          {
            idPrefix: 'pension-withdrawal',
            fields: PENSION_WITHDRAWAL_FIELDS,
            validator: asFormValidator(PensionWithdrawalDraftSchema),
            submitLabel: '연금계좌 인출',
          },
          {
            initial: { accountId: '', amountKrw: 1, type: 'nonPension', reason: '' },
            onSubmit: createPensionWithdrawalSubmitter(taxAccountSubmitterDeps),
          },
        );
        ctx.bag.add(pensionWithdrawalForm);

        const assetSubmitterDeps: AssetSubmitterDeps = {
          store,
          snapshots,
          api,
          toasts,
          createCommandId: createOrderId,
          refreshLatestLedger,
        };
        const bondOrderForm = renderForm<BondOrderDraft>(
          {
            idPrefix: 'bond-order',
            fields: BOND_ORDER_FIELDS,
            validator: asFormValidator(BondOrderDraftSchema),
            submitLabel: '국채 주문',
          },
          {
            initial: { accountId: '', seriesId: '', side: 'buy', bondUnits: 1 },
            onSubmit: createBondOrderSubmitter(assetSubmitterDeps),
          },
        );
        ctx.bag.add(bondOrderForm);

        const goldAccountOpenForm = renderForm<GoldAccountOpenDraft>(
          {
            idPrefix: 'gold-account-open',
            fields: GOLD_ACCOUNT_OPEN_FIELDS,
            validator: asFormValidator(GoldAccountOpenDraftSchema),
            submitLabel: '금 계좌 개설',
          },
          {
            initial: { type: 'krxGold', productVersionId: '' },
            onSubmit: createGoldAccountOpenSubmitter(assetSubmitterDeps),
          },
        );
        ctx.bag.add(goldAccountOpenForm);

        const goldOrderForm = renderForm<GoldOrderDraft>(
          {
            idPrefix: 'gold-order',
            fields: GOLD_ORDER_FIELDS,
            validator: asFormValidator(GoldOrderDraftSchema),
            submitLabel: '금 주문',
          },
          {
            initial: { accountId: '', side: 'buy', quantityGram: 1 },
            onSubmit: createGoldOrderSubmitter(assetSubmitterDeps),
          },
        );
        ctx.bag.add(goldOrderForm);

        const goldWithdrawalForm = renderForm<GoldWithdrawalDraft>(
          {
            idPrefix: 'gold-withdrawal',
            fields: GOLD_WITHDRAWAL_FIELDS,
            validator: {
              validate(raw) {
                const barSizeGram = raw.barSizeGram;
                return GoldWithdrawalDraftFormValidator.validate({
                  ...raw,
                  barSizeGram: typeof barSizeGram === 'string' ? Number(barSizeGram) : barSizeGram,
                });
              },
            },
            submitLabel: '실물 금 인출',
          },
          {
            initial: { accountId: '', barSizeGram: 100, barCount: 1 },
            onSubmit: createGoldWithdrawalSubmitter(assetSubmitterDeps),
          },
        );
        ctx.bag.add(goldWithdrawalForm);
        let cmaProductDefaulted = false;
        let depositProductDefaulted = false;
        let settlementAccountDefaulted = false;
        let cmaCloseDefaulted = false;
        let depositCloseDefaulted = false;
        let isaCloseDefaulted = false;
        let pensionStartDefaulted = false;
        let pensionWithdrawalDefaulted = false;

        const dateValue = el('strong');
        const dayValue = el('span');
        const cashValue = el('strong');
        const netWorthValue = el('strong');
        const portfolioValue = el('strong');
        const positionQuantityValue = el('span');
        const positionAverageValue = el('span');
        const positionValue = el('strong');
        const llxPositionsValue = el('pre');
        const marketIndexValue = el('span');
        const marketCloseValue = el('strong');
        const llxCloseValue = el('strong');
        const goldCloseValue = el('strong');
        const cpiIndexValue = el('span');
        const marketReturnValue = el('span');
        const marketRegimeValue = el('span');
        const marketOpenValue = el('span');
        const policyRateValue = el('span');
        const treasury3mValue = el('span');
        const treasury1yValue = el('span');
        const treasury3yValue = el('span');
        const treasury10yValue = el('span');
        const statusValue = el('span', { class: 'status' });
        const autoStatusValue = el('span', { class: 'auto-status' });
        const militaryStatusValue = el('span');
        const militaryServiceValue = el('span');
        const militarySavingsValue = el('span');
        const lifeSummaryValue = el('span');
        const accountValue = el('span', { class: 'account' });
        const logoutButton = el('button', { type: 'button', class: 'logout' }, '로그아웃');
        const deleteAccountButton = el(
          'button',
          { type: 'button' },
          '계정과 모든 데이터 영구 삭제',
        );
        const policySetValue = el('span');
        const accountBody = el('tbody');
        const accountTable = createAccountTable(accountBody, ACCOUNT_TABLE_CAPACITY);
        const orderFieldset = el(
          'fieldset',
          {},
          el('legend', {}, 'LLX 매수·매도'),
          orderForm.element,
        );
        const transferFieldset = el(
          'fieldset',
          {},
          el('legend', {}, '지갑·계좌 이체'),
          el(
            'p',
            {},
            'ISA 납입·원금 인출은 이 폼을 사용합니다. 연금저축·IRP 인출은 아래 전용 폼을 사용하세요.',
          ),
          transferForm.element,
        );
        const cashProductStatus = el('p', {
          id: 'cash-product-status',
          attrs: { role: 'status', 'aria-live': 'polite' },
        });
        const cashProductRefreshButton = el('button', { type: 'button' }, '상품 목록 새로고침');
        const cashProductBody = el('tbody');
        const cashProductTable = createFixedTable(
          cashProductBody,
          CASH_PRODUCT_TABLE_CAPACITY,
          createCashProductRow,
        );
        const cmaAccountBody = el('tbody');
        const cmaAccountTable = createFixedTable(
          cmaAccountBody,
          CMA_ACCOUNT_TABLE_CAPACITY,
          createCmaAccountRow,
        );
        const cashContractBody = el('tbody');
        const cashContractTable = createFixedTable(
          cashContractBody,
          CASH_CONTRACT_TABLE_CAPACITY,
          createCashContractRow,
        );
        const depositProtectionBody = el('tbody');
        const depositProtectionTable = createFixedTable(
          depositProtectionBody,
          DEPOSIT_PROTECTION_TABLE_CAPACITY,
          createDepositProtectionRow,
        );
        const cmaOpenFieldset = el(
          'fieldset',
          {},
          el('legend', {}, 'CMA 계좌 개설'),
          cmaOpenForm.element,
        );
        const cmaCloseFieldset = el(
          'fieldset',
          {},
          el('legend', {}, 'CMA 계좌 종료'),
          cmaCloseForm.element,
        );
        const depositOpenFieldset = el(
          'fieldset',
          {},
          el('legend', {}, '예금·적금 가입'),
          depositOpenForm.element,
        );
        const depositCloseFieldset = el(
          'fieldset',
          {},
          el('legend', {}, '예금·적금 중도해지'),
          depositCloseForm.element,
        );
        const taxAccountOpenFieldset = el(
          'fieldset',
          {},
          el('legend', {}, '절세계좌 개설'),
          taxAccountOpenForm.element,
        );
        const isaCloseFieldset = el(
          'fieldset',
          {},
          el('legend', {}, 'ISA 전체 해지'),
          isaCloseForm.element,
        );
        const pensionStartFieldset = el(
          'fieldset',
          {},
          el('legend', {}, '연금 수령 개시'),
          pensionStartForm.element,
        );
        const pensionWithdrawalFieldset = el(
          'fieldset',
          {},
          el('legend', {}, '연금계좌 인출'),
          pensionWithdrawalForm.element,
        );
        const isaAccountBody = el('tbody');
        const isaAccountTable = createFixedTable(
          isaAccountBody,
          ISA_ACCOUNT_TABLE_CAPACITY,
          createIsaAccountRow,
        );
        const pensionAccountBody = el('tbody');
        const pensionAccountTable = createFixedTable(
          pensionAccountBody,
          PENSION_ACCOUNT_TABLE_CAPACITY,
          createPensionAccountRow,
        );
        const bondCatalogStatus = el('p', {
          id: 'bond-catalog-status',
          attrs: { role: 'status', 'aria-live': 'polite' },
        });
        const bondCatalogRefreshButton = el('button', { type: 'button' }, '국채 목록 새로고침');
        const bondProductCatalogValue = el('pre');
        const bondSeriesCatalogValue = el('pre');
        const bondPositionValue = el('pre');
        const bondOrderFieldset = el(
          'fieldset',
          {},
          el('legend', {}, '국채 매수·매도'),
          bondOrderForm.element,
        );
        const goldProductStatus = el('p', {
          id: 'gold-product-status',
          attrs: { role: 'status', 'aria-live': 'polite' },
        });
        const goldProductRefreshButton = el('button', { type: 'button' }, '금 상품 새로고침');
        const goldProductCatalogValue = el('pre');
        const goldAccountValue = el('pre');
        const physicalGoldValue = el('pre');
        const goldAccountOpenFieldset = el(
          'fieldset',
          {},
          el('legend', {}, 'KRX 금현물 계좌 개설'),
          goldAccountOpenForm.element,
        );
        const goldOrderFieldset = el(
          'fieldset',
          {},
          el('legend', {}, '금 매수·매도'),
          goldOrderForm.element,
        );
        const goldWithdrawalFieldset = el(
          'fieldset',
          {},
          el('legend', {}, '100g·1kg 실물 인출'),
          goldWithdrawalForm.element,
        );
        const productBundleValue = el('pre');
        const llxEntitlementValue = el('pre');
        const pendingSettlementValue = el('pre');
        const currentTaxAssessmentValue = el('pre');
        const latestTaxAssessmentValue = el('pre');
        const currentTaxYearValue = el('span');
        const grossFinancialIncomeValue = el('span');
        const withheldIncomeTaxValue = el('span');
        const withheldLocalIncomeTaxValue = el('span');
        const ledgerStatus = el('p', {
          id: 'finance-ledger-status',
          attrs: { role: 'status', 'aria-live': 'polite' },
        });
        const ledgerLatestButton = el('button', { type: 'button' }, '최신 원장');
        const ledgerOlderButton = el('button', { type: 'button' }, '이전 거래');
        const ledgerBody = el('tbody');
        const ledgerTable = createLedgerTable(ledgerBody, LEDGER_PAGE_SIZE);
        const finalizationStatus = el('p', {
          attrs: { role: 'status', 'aria-live': 'polite' },
        });
        const finalizationLines = el('pre');
        const offlineProgressStatus = el('p', {
          attrs: { role: 'status', 'aria-live': 'polite' },
        });
        const offlineProgressButton = el('button', { type: 'button' });

        const stepButtons = (['day', 'week', 'month'] as const).map((unit) =>
          el('button', { type: 'button', dataset: { unit } }, stepLabel(unit)),
        );
        const speedControls = GAME_SPEEDS.map((speed) => ({
          speed,
          button: el(
            'button',
            {
              type: 'button',
              dataset: { speed: speed.toString() },
              attrs: { 'aria-pressed': 'false' },
            },
            `x${speed}`,
          ),
        }));
        const pauseButton = el('button', { type: 'button' }, '일시정지');
        const historyPeriod = el('select', {
          id: 'market-history-period',
          attrs: { 'aria-describedby': 'market-history-status' },
        });
        for (const period of HISTORY_PERIODS) {
          const option = el('option', { value: period.days.toString() }, period.label);
          option.selected = period.days === DEFAULT_HISTORY_DAYS;
          historyPeriod.appendChild(option);
        }
        const historyStatus = el('p', {
          id: 'market-history-status',
          attrs: { role: 'status', 'aria-live': 'polite' },
        });
        const chartHost = el('div', { attrs: { 'aria-hidden': 'true' } });
        const historyBody = el('tbody');
        const historyTable = createHistoryTable(historyBody, MAX_HISTORY_DAYS);

        root = el(
          'section',
          { class: 'dashboard' },
          el('h1', {}, 'LifeLedger'),
          el('p', { class: 'account-line' }, accountValue, ' ', logoutButton),
          el(
            'section',
            {},
            el('h2', {}, '개발 플레이테스트 안내'),
            el(
              'p',
              {},
              '캐릭터와 자산은 모두 허구이며 투자·법률·보험 모델은 단순화된 게임 규칙으로 조언이 아닙니다.',
            ),
            el(
              'p',
              {},
              '사용 분석은 수집하지 않습니다. 피드백 본문은 최대 90일 보관하며 개별 삭제나 동의 철회로 더 일찍 삭제할 수 있습니다.',
            ),
            el(
              'p',
              {},
              '알려진 문제·장애·삭제 문의: ',
              el(
                'a',
                {
                  href: 'https://github.com/it-play/lifeledger/blob/main/plan-docs/m5-playtest-release.md',
                  attrs: { target: '_blank', rel: 'noreferrer' },
                },
                '알려진 문제와 고지',
              ),
              ' · ',
              el(
                'a',
                {
                  href: 'https://github.com/it-play/lifeledger/issues',
                  attrs: { target: '_blank', rel: 'noreferrer' },
                },
                'GitHub Issues 문의',
              ),
              ' — 이메일, 세션 토큰, 실제 금융정보나 피드백 본문은 적지 마세요.',
            ),
            el(
              'p',
              {},
              '계정 삭제는 모든 게임 기록·동의·피드백을 되돌릴 수 없게 삭제합니다. ',
              deleteAccountButton,
            ),
          ),
          el('p', { class: 'connection' }, '스트림: ', statusValue),
          el('p', {}, el('a', { href: '/career', dataset: { link: '' } }, '커리어 관리')),
          el('p', {}, el('a', { href: '/corporation', dataset: { link: '' } }, '법인 경영')),
          el('p', {}, el('a', { href: '/life', dataset: { link: '' } }, '생활비 관리')),
          el('p', {}, el('a', { href: '/loans', dataset: { link: '' } }, '신용과 대출')),
          el('p', {}, el('a', { href: '/housing', dataset: { link: '' } }, '주거 시장')),
          el('p', {}, el('a', { href: '/welfare', dataset: { link: '' } }, '복지 프로그램')),
          el('p', {}, el('a', { href: '/rankings', dataset: { link: '' } }, '다른 삶의 순위 장부')),
          el(
            'p',
            {},
            el(
              'a',
              { href: '/playtest-feedback', dataset: { link: '' } },
              '개발 플레이테스트 피드백',
            ),
          ),
          el(
            'p',
            {},
            el('a', { href: '/recovery', dataset: { link: '' } }, '채무 청산과 신용 회복'),
          ),
          el(
            'p',
            {},
            el('a', { href: '/events-insurance', dataset: { link: '' } }, '생애 사건과 보험'),
          ),
          el(
            'dl',
            { class: 'summary' },
            el('dt', {}, '게임 날짜'),
            el('dd', {}, dateValue, ' (', dayValue, '일차)'),
            el('dt', {}, '현금'),
            el('dd', {}, cashValue),
            el('dt', {}, '순자산'),
            el('dd', {}, netWorthValue),
            el('dt', {}, '포트폴리오 평가액'),
            el('dd', {}, portfolioValue),
            el('dt', {}, '병역 상태'),
            el('dd', {}, militaryStatusValue),
            el('dt', {}, '복무 진행'),
            el('dd', {}, militaryServiceValue),
            el('dt', {}, '활성 장병적금'),
            el('dd', {}, militarySavingsValue),
            el('dt', {}, '생활비'),
            el('dd', {}, lifeSummaryValue),
            el('dt', {}, 'LLX 보유수량'),
            el('dd', {}, positionQuantityValue),
            el('dt', {}, 'LLX 계좌별 평균단가'),
            el('dd', {}, positionAverageValue),
            el('dt', {}, 'LLX 평가금액'),
            el('dd', {}, positionValue),
            el('dt', {}, '시장 지수'),
            el('dd', {}, marketIndexValue),
            el('dt', {}, '벤치마크 종가'),
            el('dd', {}, marketCloseValue),
            el('dt', {}, 'LLX 거래 종가'),
            el('dd', {}, llxCloseValue),
            el('dt', {}, '금 종가/g'),
            el('dd', {}, goldCloseValue),
            el('dt', {}, 'CPI 지수'),
            el('dd', {}, cpiIndexValue),
            el('dt', {}, '벤치마크 일수익률'),
            el('dd', {}, marketReturnValue),
            el('dt', {}, '시장 국면'),
            el('dd', {}, marketRegimeValue),
            el('dt', {}, '장 상태'),
            el('dd', {}, marketOpenValue),
            el('dt', {}, '기준금리'),
            el('dd', {}, policyRateValue),
            el('dt', {}, '국고채 3개월'),
            el('dd', {}, treasury3mValue),
            el('dt', {}, '국고채 1년'),
            el('dd', {}, treasury1yValue),
            el('dt', {}, '국고채 3년'),
            el('dd', {}, treasury3yValue),
            el('dt', {}, '국고채 10년'),
            el('dd', {}, treasury10yValue),
          ),
          el('div', { class: 'controls' }, ...stepButtons),
          el(
            'div',
            { class: 'auto-controls' },
            el('span', {}, '자동 진행 '),
            ...speedControls.map((control) => control.button),
            pauseButton,
          ),
          el('p', {}, '자동 진행 상태: ', autoStatusValue),
          el('section', {}, el('h2', {}, '시즌 결산'), finalizationStatus, finalizationLines),
          el(
            'section',
            {},
            el('h2', {}, '오프라인 진행'),
            el(
              'p',
              {},
              '명시적으로 켠 현재 실행만 서버에서 진행합니다. 접속 중에는 온라인 진행이 우선합니다.',
            ),
            offlineProgressStatus,
            offlineProgressButton,
          ),
          el('section', {}, el('h2', {}, 'LLX 계좌별 보유'), llxPositionsValue),
          equitySearchPanel,
          orderFieldset,
          el(
            'section',
            {},
            el('h2', {}, '국채'),
            bondCatalogStatus,
            bondCatalogRefreshButton,
            el('h3', {}, '게시 상품'),
            bondProductCatalogValue,
            el('h3', {}, '거래 가능 시리즈'),
            bondSeriesCatalogValue,
            bondOrderFieldset,
            el('h3', {}, '보유 국채'),
            bondPositionValue,
          ),
          el(
            'section',
            {},
            el('h2', {}, 'KRX 금현물'),
            goldProductStatus,
            goldProductRefreshButton,
            el('h3', {}, '게시 상품·실물 규격'),
            goldProductCatalogValue,
            goldAccountOpenFieldset,
            goldOrderFieldset,
            goldWithdrawalFieldset,
            el('h3', {}, '금 계좌'),
            goldAccountValue,
            el('h3', {}, '실물 금 보유'),
            physicalGoldValue,
          ),
          el(
            'section',
            {},
            el('h2', {}, 'M2-D 상품 묶음·분배 권리'),
            el('h3', {}, '런 고정 상품 묶음'),
            productBundleValue,
            el('h3', {}, '미지급 LLX 분배 권리'),
            llxEntitlementValue,
            el('h3', {}, '대기 정산'),
            pendingSettlementValue,
          ),
          el(
            'section',
            {},
            el('h2', {}, '금융계좌'),
            el('p', {}, '적용 제도: ', policySetValue),
            el(
              'table',
              {},
              el('caption', {}, '현재 런 금융계좌'),
              el(
                'thead',
                {},
                el(
                  'tr',
                  {},
                  historyHeader('계좌 ID'),
                  historyHeader('종류'),
                  historyHeader('상태'),
                  historyHeader('계좌 현금'),
                  historyHeader('기본 계좌'),
                ),
              ),
              accountBody,
            ),
            transferFieldset,
          ),
          el(
            'section',
            {},
            el('h2', {}, '절세계좌'),
            el(
              'p',
              {},
              'ISA·연금저축·IRP는 M2-D 시가손익과 세원층을 반영합니다. 계좌별 허용 한도와 IRP 위험자산 한도를 확인하세요.',
            ),
            taxAccountOpenFieldset,
            isaCloseFieldset,
            el('h3', {}, 'ISA 현황'),
            el(
              'table',
              {},
              el('caption', {}, '현재 런 ISA 계좌'),
              el(
                'thead',
                {},
                el(
                  'tr',
                  {},
                  historyHeader('계좌 ID'),
                  historyHeader('유형'),
                  historyHeader('가입·의무기간'),
                  historyHeader('납입·원금 인출·여력'),
                  historyHeader('세금 손익'),
                  historyHeader('예상 종료세금'),
                ),
              ),
              isaAccountBody,
            ),
            pensionStartFieldset,
            pensionWithdrawalFieldset,
            el('h3', {}, '연금저축·IRP 현황'),
            el(
              'table',
              {},
              el('caption', {}, '현재 런 연금계좌'),
              el(
                'thead',
                {},
                el(
                  'tr',
                  {},
                  historyHeader('계좌 ID·유형'),
                  historyHeader('가입·개시 가능일·상태'),
                  historyHeader('세원층'),
                  historyHeader('올해 납입·공제대상·예상공제'),
                  historyHeader('연금 한도·인출'),
                  historyHeader('위험자산'),
                ),
              ),
              pensionAccountBody,
            ),
          ),
          el(
            'section',
            {},
            el('h2', {}, '현금상품'),
            cashProductStatus,
            cashProductRefreshButton,
            el(
              'table',
              {},
              el('caption', {}, '게시된 CMA·예금·적금 상품'),
              el(
                'thead',
                {},
                el(
                  'tr',
                  {},
                  historyHeader('상품 ID'),
                  historyHeader('상품 key'),
                  historyHeader('종류'),
                  historyHeader('기관'),
                  historyHeader('금리 조건'),
                  historyHeader('금액·기간'),
                ),
              ),
              cashProductBody,
            ),
            cmaOpenFieldset,
            cmaCloseFieldset,
            el('h3', {}, 'CMA 계좌 현황'),
            el(
              'table',
              {},
              el('caption', {}, '현재 런 CMA 계좌'),
              el(
                'thead',
                {},
                el(
                  'tr',
                  {},
                  historyHeader('계좌 ID'),
                  historyHeader('상품 ID'),
                  historyHeader('당일 연이율'),
                  historyHeader('이자 최소 잔액'),
                  historyHeader('이자 remainder'),
                ),
              ),
              cmaAccountBody,
            ),
            depositOpenFieldset,
            depositCloseFieldset,
            el('h3', {}, '예금·적금 계약 현황'),
            el(
              'table',
              {},
              el('caption', {}, '현재 런 예금·적금 계약'),
              el(
                'thead',
                {},
                el(
                  'tr',
                  {},
                  historyHeader('계약 ID'),
                  historyHeader('상품 ID'),
                  historyHeader('정산 계좌'),
                  historyHeader('종류·상태'),
                  historyHeader('고정 연이율'),
                  historyHeader('현재 원금'),
                  historyHeader('회당 금액·회차'),
                  historyHeader('가입·만기 게임일'),
                  historyHeader('예상 만기 금액'),
                ),
              ),
              cashContractBody,
            ),
            el('h3', {}, '예금자보호 요약'),
            el(
              'table',
              {},
              el('caption', {}, '기관별 예금자보호 금액'),
              el(
                'thead',
                {},
                el(
                  'tr',
                  {},
                  historyHeader('기관 ID'),
                  historyHeader('보호 대상'),
                  historyHeader('보호 금액'),
                  historyHeader('비보호 금액'),
                ),
              ),
              depositProtectionBody,
            ),
            el('h3', {}, '현재 연도 금융소득'),
            el(
              'dl',
              {},
              el('dt', {}, '과세 연도'),
              el('dd', {}, currentTaxYearValue),
              el('dt', {}, '총 금융소득'),
              el('dd', {}, grossFinancialIncomeValue),
              el('dt', {}, '소득세 원천징수'),
              el('dd', {}, withheldIncomeTaxValue),
              el('dt', {}, '지방소득세 원천징수'),
              el('dd', {}, withheldLocalIncomeTaxValue),
            ),
            el('h4', {}, '현재 연도 상태·원천·세액 산정'),
            currentTaxAssessmentValue,
            el('h4', {}, '최근 확정 세액 산정'),
            latestTaxAssessmentValue,
          ),
          el(
            'section',
            {},
            el('h2', {}, '복식 원장'),
            ledgerStatus,
            el('div', {}, ledgerLatestButton, ledgerOlderButton),
            el(
              'table',
              {},
              el('caption', {}, '현재 런 원장 거래'),
              el(
                'thead',
                {},
                el(
                  'tr',
                  {},
                  historyHeader('거래 ID'),
                  historyHeader('게임일'),
                  historyHeader('설명'),
                  historyHeader('출처'),
                  historyHeader('분개'),
                ),
              ),
              ledgerBody,
            ),
          ),
          el(
            'section',
            {},
            el('h2', {}, 'LLX 시장 히스토리'),
            el('label', { attrs: { for: 'market-history-period' } }, '조회 기간'),
            historyPeriod,
            historyStatus,
            chartHost,
            el(
              'table',
              {},
              el('caption', {}, 'LLX 종가 히스토리 (차트와 동일한 데이터)'),
              el(
                'thead',
                {},
                el(
                  'tr',
                  {},
                  historyHeader('날짜'),
                  historyHeader('게임일'),
                  historyHeader('장 상태'),
                  historyHeader('종가'),
                  historyHeader('일수익률'),
                  historyHeader('시장 국면'),
                ),
              ),
              historyBody,
            ),
          ),
          el('p', {}, el('a', { href: '/new', dataset: { link: '' } }, '새 캐릭터로 다시 시작')),
        );
        host.replaceChildren(root);

        const closeChart = createCloseChart({ host: chartHost, bag: ctx.bag });

        h.bindText(dateValue, () => dateText.get());
        h.bindText(dayValue, () => dayText.get());
        h.bindText(cashValue, () => cashText.get());
        h.bindText(netWorthValue, () => netWorthText.get());
        h.bindText(portfolioValue, () => portfolioValueText.get());
        h.bindText(positionQuantityValue, () => positionQuantityText.get());
        h.bindText(positionAverageValue, () => positionAverageText.get());
        h.bindText(positionValue, () => positionValueText.get());
        h.bindText(llxPositionsValue, () => llxPositionsText(llxPositions.get()));
        h.bindText(marketIndexValue, () => marketIndexText.get());
        h.bindText(marketCloseValue, () => marketCloseText.get());
        h.bindText(llxCloseValue, () => llxCloseText.get());
        h.bindText(goldCloseValue, () => goldCloseText.get());
        h.bindText(cpiIndexValue, () => cpiIndexText.get());
        h.bindText(marketReturnValue, () => marketReturnText.get());
        h.bindText(marketRegimeValue, () => marketRegimeText.get());
        h.bindText(marketOpenValue, () => marketOpenText.get());
        h.bindText(policyRateValue, () => policyRateText.get());
        h.bindText(treasury3mValue, () => treasury3mText.get());
        h.bindText(treasury1yValue, () => treasury1yText.get());
        h.bindText(treasury3yValue, () => treasury3yText.get());
        h.bindText(treasury10yValue, () => treasury10yText.get());
        h.bindText(statusValue, () => statusText.get());
        h.bindText(autoStatusValue, () => autoStatusText.get());
        h.bindText(finalizationStatus, () => finalizationStatusText.get());
        h.bindText(finalizationLines, () => finalizationLinesText.get());
        h.bindText(offlineProgressStatus, () => offlineProgressStatusText.get());
        h.bindText(offlineProgressButton, () => offlineProgressButtonText.get());
        h.bindAttribute(offlineProgressButton, 'disabled', () => offlineProgressBlocked.get());
        h.bindText(militaryStatusValue, () => militaryStatusText.get());
        h.bindText(militaryServiceValue, () => militaryServiceText.get());
        h.bindText(militarySavingsValue, () => militarySavingsText.get());
        h.bindText(lifeSummaryValue, () => lifeSummaryText.get());
        h.bindText(accountValue, () => accountText.get());
        h.bindText(policySetValue, () => policySetText.get());
        h.bindText(historyStatus, () => historyStatusText.get());
        h.bindText(ledgerStatus, () => ledgerStatusText.get());
        h.bindText(cashProductStatus, () => cashProductStatusText.get());
        h.bindText(bondCatalogStatus, () => bondCatalogStatusText.get());
        h.bindText(goldProductStatus, () => goldProductStatusText.get());
        h.bindText(bondProductCatalogValue, () => {
          const state = bondCatalogRequest.state.get();
          return bondProductCatalogText(state.status === 'success' ? state.value : undefined);
        });
        h.bindText(bondSeriesCatalogValue, () => {
          const state = bondCatalogRequest.state.get();
          return bondSeriesCatalogText(state.status === 'success' ? state.value : undefined);
        });
        h.bindText(goldProductCatalogValue, () => {
          const state = goldProductRequest.state.get();
          return goldProductCatalogText(state.status === 'success' ? state.value : undefined);
        });
        h.bindText(bondPositionValue, () =>
          bondPositionsText(snapshot.get()?.finance.bondPositions ?? []),
        );
        h.bindText(goldAccountValue, () =>
          goldAccountsText(snapshot.get()?.finance.goldAccounts ?? []),
        );
        h.bindText(physicalGoldValue, () =>
          physicalGoldHoldingsText(snapshot.get()?.finance.physicalGoldHoldings ?? []),
        );
        h.bindText(productBundleValue, () =>
          productBundleText(snapshot.get()?.finance.productBundle),
        );
        h.bindText(llxEntitlementValue, () =>
          llxEntitlementsText(snapshot.get()?.finance.llxDistributionEntitlements ?? []),
        );
        h.bindText(pendingSettlementValue, () =>
          pendingSettlementsText(snapshot.get()?.finance.pendingSettlements ?? []),
        );
        h.bindText(currentTaxAssessmentValue, () =>
          financialIncomeYearText(snapshot.get()?.finance.currentTaxYear),
        );
        h.bindText(latestTaxAssessmentValue, () =>
          financialIncomeAssessmentText(snapshot.get()?.finance.latestFinancialIncomeAssessment),
        );
        h.bindText(currentTaxYearValue, () => currentTaxYearText.get());
        h.bindText(grossFinancialIncomeValue, () => grossFinancialIncomeText.get());
        h.bindText(withheldIncomeTaxValue, () => withheldIncomeTaxText.get());
        h.bindText(withheldLocalIncomeTaxValue, () => withheldLocalIncomeTaxText.get());
        h.bindAttribute(
          orderFieldset,
          'disabled',
          () =>
            !gameReady.get() ||
            defaultTradeAccountId.get() === undefined ||
            snapshot.get()?.market.open !== true ||
            advancing.get() ||
            ordering.get() ||
            autoSpeed.get() !== null,
        );
        h.bindAttribute(
          transferFieldset,
          'disabled',
          () =>
            !gameReady.get() ||
            defaultFinanceAccountId.get() === undefined ||
            advancing.get() ||
            ordering.get() ||
            autoSpeed.get() !== null,
        );
        h.bindAttribute(
          cmaOpenFieldset,
          'disabled',
          () => financeMutationBlocked.get() || cashProductRequest.state.get().status !== 'success',
        );
        h.bindAttribute(cmaCloseFieldset, 'disabled', () => financeMutationBlocked.get());
        h.bindAttribute(
          depositOpenFieldset,
          'disabled',
          () => financeMutationBlocked.get() || cashProductRequest.state.get().status !== 'success',
        );
        h.bindAttribute(depositCloseFieldset, 'disabled', () => financeMutationBlocked.get());
        h.bindAttribute(taxAccountOpenFieldset, 'disabled', () => financeMutationBlocked.get());
        h.bindAttribute(isaCloseFieldset, 'disabled', () => financeMutationBlocked.get());
        h.bindAttribute(pensionStartFieldset, 'disabled', () => financeMutationBlocked.get());
        h.bindAttribute(pensionWithdrawalFieldset, 'disabled', () => financeMutationBlocked.get());
        h.bindAttribute(bondOrderFieldset, 'disabled', () => {
          const state = bondCatalogRequest.state.get();
          const current = snapshot.get();
          const productIds = current?.finance.productBundle?.bondProductVersionIds;
          return (
            marketAssetMutationBlocked.get() ||
            defaultBondAccountId.get() === undefined ||
            state.status !== 'success' ||
            current === undefined ||
            state.value.marketVersion !== current.market.world ||
            productIds === undefined ||
            !state.value.series.some((series) => productIds.includes(series.productVersionId))
          );
        });
        h.bindAttribute(goldAccountOpenFieldset, 'disabled', () => {
          const state = goldProductRequest.state.get();
          const current = snapshot.get();
          const productId = current?.finance.productBundle?.goldProductVersionId;
          return (
            financeMutationBlocked.get() ||
            !m2dReady.get() ||
            defaultGoldAccountId.get() !== undefined ||
            state.status !== 'success' ||
            current === undefined ||
            state.value.marketVersion !== current.market.world ||
            productId === undefined ||
            !state.value.products.some((product) => product.id === productId)
          );
        });
        h.bindAttribute(goldOrderFieldset, 'disabled', () => {
          return marketAssetMutationBlocked.get() || defaultGoldAccountId.get() === undefined;
        });
        h.bindAttribute(
          goldWithdrawalFieldset,
          'disabled',
          () =>
            financeMutationBlocked.get() ||
            !m2dReady.get() ||
            defaultGoldAccountId.get() === undefined,
        );
        h.bindAttribute(
          cashProductRefreshButton,
          'disabled',
          () => cashProductRequest.state.get().status === 'loading',
        );
        h.bindAttribute(
          bondCatalogRefreshButton,
          'disabled',
          () =>
            !gameReady.get() ||
            autoSpeed.get() !== null ||
            bondCatalogRequest.state.get().status === 'loading',
        );
        h.bindAttribute(
          goldProductRefreshButton,
          'disabled',
          () =>
            !gameReady.get() ||
            autoSpeed.get() !== null ||
            goldProductRequest.state.get().status === 'loading',
        );
        h.bindAttribute(
          historyPeriod,
          'disabled',
          () => !gameReady.get() || historyRequest.state.get().status === 'loading',
        );

        for (const button of stepButtons) {
          h.bindAttribute(
            button,
            'disabled',
            () => !gameReady.get() || advancing.get() || ordering.get() || autoSpeed.get() !== null,
          );
          h.useEventListener(button, 'click', () => {
            const unit = button.dataset.unit as StepUnit | undefined;
            if (unit === undefined) return;
            void advance(store, snapshots, api, toasts, advanceRetries, STEP_DAYS[unit]);
          });
        }

        for (const control of speedControls) {
          h.bindAttribute(
            control.button,
            'disabled',
            () =>
              !gameReady.get() || advancing.get() || ordering.get() || connection.get() !== 'open',
          );
          h.bindAttribute(control.button, 'aria-pressed', () =>
            autoSpeed.get() === control.speed ? 'true' : 'false',
          );
          h.useEventListener(control.button, 'click', () => {
            void setClock(store, snapshots, api, toasts, control.speed);
          });
        }
        h.bindAttribute(
          pauseButton,
          'disabled',
          () => advancing.get() || ordering.get() || autoSpeed.get() === null,
        );
        h.bindAttribute(
          ledgerLatestButton,
          'disabled',
          () => !gameReady.get() || ledgerRequest.state.get().status === 'loading',
        );
        h.bindAttribute(ledgerOlderButton, 'disabled', () => {
          const state = ledgerRequest.state.get();
          return state.status !== 'success' || state.value.nextBefore === null;
        });
        h.useEventListener(pauseButton, 'click', () => {
          void setClock(store, snapshots, api, toasts, null);
        });
        h.useEventListener(historyPeriod, 'change', () => {
          const result = MarketHistoryDaysSchema.safeParse(Number(historyPeriod.value));
          if (result.success) historyDays.set(result.data);
        });
        h.useEventListener(ledgerLatestButton, 'click', () => {
          ledgerBefore.set(undefined);
          ledgerRequest.run();
        });
        h.useEventListener(ledgerOlderButton, 'click', () => {
          const state = ledgerRequest.state.peek();
          if (state.status !== 'success' || state.value.nextBefore === null) return;
          ledgerBefore.set(state.value.nextBefore);
          ledgerRequest.run();
        });
        h.useEventListener(cashProductRefreshButton, 'click', () => cashProductRequest.run());
        h.useEventListener(bondCatalogRefreshButton, 'click', () => {
          if (autoSpeed.get() === null) bondCatalogRequest.run();
        });
        h.useEventListener(goldProductRefreshButton, 'click', () => {
          if (autoSpeed.get() === null) goldProductRequest.run();
        });
        h.useEventListener(logoutButton, 'click', () => {
          void logout(auth, toasts);
        });
        h.useEventListener(deleteAccountButton, 'click', () => {
          void deleteAccount(auth, toasts, deleteAccountButton);
        });

        h.useEffect(() => {
          const state = historyRequest.state.get();
          if (state.status === 'success') {
            historyTable.setPoints(state.value.points);
            closeChart.setPoints(state.value.points.map(toCloseChartPoint));
            return;
          }
          historyTable.setPoints([]);
          closeChart.setPoints([]);
        });
        h.useEffect(() => {
          accountTable.setAccounts(snapshot.get()?.finance.accounts ?? []);
        });
        h.useEffect(() => {
          const current = snapshot.get();
          const accounts = current?.finance.accounts ?? [];
          const supportsM2dTaxAccounts =
            current !== undefined && current.finance.productBundle !== null;
          const llxAccounts = accounts
            .filter(
              (item) =>
                item.status === 'open' && accountTypeAllowsLlx(item.type, supportsM2dTaxAccounts),
            )
            .map((item) => ({
              value: item.id,
              label: `#${item.id} ${accountTypeLabel(item.type)} · 현금 ${formatWon(item.cashKrw)}`,
            }));
          const bondAccounts = accounts
            .filter((item) => item.status === 'open' && accountTypeAllowsBond(item.type))
            .map((item) => ({
              value: item.id,
              label: `#${item.id} ${accountTypeLabel(item.type)} · 현금 ${formatWon(item.cashKrw)}`,
            }));
          const goldAccounts = accounts
            .filter((item) => item.status === 'open' && item.type === 'krxGold')
            .map((item) => ({
              value: item.id,
              label: `#${item.id} KRX 금현물 · 현금 ${formatWon(item.cashKrw)}`,
            }));
          updateFixedSelectOptions(
            orderForm.element,
            'accountId',
            llxAccounts,
            defaultTradeAccountId.get(),
          );
          updateFixedSelectOptions(
            bondOrderForm.element,
            'accountId',
            bondAccounts,
            defaultBondAccountId.get(),
          );
          updateFixedSelectOptions(
            goldOrderForm.element,
            'accountId',
            goldAccounts,
            defaultGoldAccountId.get(),
          );
          updateFixedSelectOptions(
            goldWithdrawalForm.element,
            'accountId',
            goldAccounts,
            defaultGoldAccountId.get(),
          );
        });
        h.useEffect(() => {
          const current = snapshot.get()?.finance;
          cmaAccountTable.setItems(current?.cmaAccounts ?? []);
          cashContractTable.setItems(current?.cashContracts ?? []);
          depositProtectionTable.setItems(current?.depositProtection ?? []);
          isaAccountTable.setItems(current?.isaAccounts ?? []);
          pensionAccountTable.setItems(current?.pensionAccounts ?? []);
        });
        h.useEffect(() => {
          const state = cashProductRequest.state.get();
          cashProductTable.setItems(state.status === 'success' ? state.value.products : []);
        });
        h.useEffect(() => {
          const state = bondCatalogRequest.state.get();
          const current = snapshot.get();
          const productIds = current?.finance.productBundle?.bondProductVersionIds;
          const series =
            state.status === 'success' &&
            current !== undefined &&
            state.value.marketVersion === current.market.world &&
            productIds !== undefined
              ? state.value.series
                  .filter((item) => productIds.includes(item.productVersionId))
                  .map((item) => ({
                    value: item.id,
                    label: `#${item.id} · 만기 ${item.maturityDate} · ${formatWon(item.dirtyPriceKrw)}`,
                  }))
              : [];
          updateFixedSelectOptions(bondOrderForm.element, 'seriesId', series, series[0]?.value);
        });
        h.useEffect(() => {
          const state = goldProductRequest.state.get();
          const current = snapshot.get();
          const productId = current?.finance.productBundle?.goldProductVersionId;
          const products =
            state.status === 'success' &&
            current !== undefined &&
            state.value.marketVersion === current.market.world &&
            productId !== undefined
              ? state.value.products
                  .filter((item) => item.id === productId)
                  .map((item) => ({ value: item.id, label: item.displayName }))
              : [];
          updateFixedSelectOptions(
            goldAccountOpenForm.element,
            'productVersionId',
            products,
            productId,
          );
        });
        h.useEffect(() => {
          const state = cashProductRequest.state.get();
          if (state.status !== 'success' || cmaProductDefaulted) return;
          const cmaProduct = state.value.products.find(
            (product) => product.kind === 'cmaRp' || product.kind === 'cmaIssuedNote',
          );
          if (cmaProduct !== undefined) {
            cmaOpenForm.setValues({ type: 'cma', productVersionId: cmaProduct.id });
            cmaProductDefaulted = true;
          }
        });
        h.useEffect(() => {
          const state = cashProductRequest.state.get();
          if (state.status !== 'success' || depositProductDefaulted) return;
          const depositProduct = state.value.products.find(
            (product) => product.kind === 'termDeposit' || product.kind === 'installmentSavings',
          );
          if (depositProduct !== undefined) {
            depositOpenForm.setValues({
              kind: depositProduct.kind,
              productVersionId: depositProduct.id,
            });
            depositProductDefaulted = true;
          }
        });
        h.useEffect(() => {
          const accountId = defaultFinanceAccountId.get();
          if (accountId !== undefined) transferForm.setValues({ accountId });
        });
        h.useEffect(() => {
          const accountId = defaultDepositSettlementAccountId.get();
          if (accountId !== undefined && !settlementAccountDefaulted) {
            depositOpenForm.setValues({ settlementAccountId: accountId });
            settlementAccountDefaulted = true;
          }
        });
        h.useEffect(() => {
          const cmaAccount = snapshot.get()?.finance.cmaAccounts[0];
          if (cmaAccount !== undefined && !cmaCloseDefaulted) {
            cmaCloseForm.setValues({ accountId: cmaAccount.accountId });
            cmaCloseDefaulted = true;
          }
        });
        h.useEffect(() => {
          const isaAccount = snapshot.get()?.finance.isaAccounts[0];
          if (isaAccount !== undefined && !isaCloseDefaulted) {
            isaCloseForm.setValues({ accountId: isaAccount.accountId });
            isaCloseDefaulted = true;
          }
        });
        h.useEffect(() => {
          const pensionAccount = snapshot
            .get()
            ?.finance.pensionAccounts.find((item) => !item.pensionStarted);
          if (pensionAccount !== undefined && !pensionStartDefaulted) {
            pensionStartForm.setValues({ accountId: pensionAccount.accountId });
            pensionStartDefaulted = true;
          }
        });
        h.useEffect(() => {
          const pensionAccount = snapshot.get()?.finance.pensionAccounts[0];
          if (pensionAccount !== undefined && !pensionWithdrawalDefaulted) {
            pensionWithdrawalForm.setValues({ accountId: pensionAccount.accountId });
            pensionWithdrawalDefaulted = true;
          }
        });
        h.useEffect(() => {
          const contract = snapshot
            .get()
            ?.finance.cashContracts.find((item) => item.status === 'active');
          if (contract !== undefined && !depositCloseDefaulted) {
            depositCloseForm.setValues({ contractId: contract.contractId });
            depositCloseDefaulted = true;
          }
        });
        h.useEffect(() => {
          const state = ledgerRequest.state.get();
          ledgerTable.setTransactions(state.status === 'success' ? state.value.transactions : []);
        });
        let requestedFinalizationRun: number | undefined;
        let refreshedFinalizationTarget: string | undefined;
        h.useEffect(() => {
          const current = snapshot.get();
          const state = finalizationRequest.state.get();
          if (!hasCharacter(current)) return;
          if (requestedFinalizationRun !== current.runRevision) {
            requestedFinalizationRun = current.runRevision;
            refreshedFinalizationTarget = undefined;
            finalizationRequest.run();
            return;
          }
          const refreshKey = pendingFinalizationRefreshKey(current, state);
          if (!isNewFinalizationTarget(refreshKey, refreshedFinalizationTarget)) return;
          refreshedFinalizationTarget = refreshKey;
          finalizationRequest.run();
        });
        let requestedOfflineProgressRun: number | undefined;
        h.useEffect(() => {
          const current = snapshot.get();
          if (!hasCharacter(current)) return;
          if (requestedOfflineProgressRun === current.runRevision) return;
          requestedOfflineProgressRun = current.runRevision;
          offlineProgress.set(undefined);
          offlineProgressDesired.set(undefined);
          offlineProgressRequest.run();
        });
        h.useWatch(offlineProgressUpdate.state, (state) => {
          reportOfflineProgressUpdate(state, toasts, () => offlineProgressRequest.run());
        });
        h.useEventListener(offlineProgressButton, 'click', () => {
          const current = offlineProgress.peek();
          if (current === undefined || !current.available) return;
          offlineProgressDesired.set(!current.enabled);
          offlineProgressUpdate.run();
        });
        h.useEffect(() => {
          const ready = gameReady.get();
          const cursor = historyCursor.get();
          historyDays.get();
          if (ready && cursor !== '') historyRequest.run();
        });
        h.useEffect(() => {
          const ready = gameReady.get();
          const state = cashProductRequest.state.get();
          if (ready && state.status === 'idle') cashProductRequest.run();
        });
        h.useEffect(() => {
          const ready = gameReady.get();
          const cursor = bondCatalogCursor.get();
          const speed = autoSpeed.get();
          if (ready && cursor !== '' && speed === null) bondCatalogRequest.run();
        });
        h.useEffect(() => {
          const ready = gameReady.get();
          const cursor = goldProductCursor.get();
          const speed = autoSpeed.get();
          if (ready && cursor !== '' && speed === null) goldProductRequest.run();
        });
        h.useEffect(() => {
          const ready = gameReady.get();
          const cursor = ledgerRunCursor.get();
          const speed = autoSpeed.get();
          if (!ready || cursor === '' || speed !== null) return;
          refreshLatestLedger();
        });

        // No snapshot yet (undefined) differs from no character (null); navigating before
        // login and the first fetch settle would flash the creation screen
        h.useEffect(() => {
          const name = characterName.get();
          const status = authStatus.get();
          if (name === null && status === 'authenticated') ctx.navigate('/new');
        });
      },

      unmount() {
        root?.remove();
        root = undefined;
      },
    };
  };
}

function finalizationStatusLabel(state: AsyncState<RunFinalization>): string {
  if (state.status === 'idle' || state.status === 'loading') {
    return '결산 정보를 확인하는 중입니다.';
  }
  if (state.status === 'error') {
    return '현재 실행은 랭킹 결산 대상이 아니거나 결산 정보를 조회할 수 없습니다.';
  }
  return finalizationLabel(state.value);
}

function offlineProgressStatusTextOf(
  status: OfflineProgress | undefined,
  request: AsyncState<OfflineProgress>,
): string {
  if (status === undefined) {
    return request.status === 'error'
      ? '오프라인 진행 상태를 불러오지 못했습니다.'
      : '오프라인 진행 상태를 확인하는 중입니다.';
  }
  if (!status.available || status.policy === null) {
    return '현재 실행에는 오프라인 진행 정책이 고정되어 있지 않습니다. 새 샌드박스 실행부터 사용할 수 있습니다.';
  }
  return offlineProgressDetailsText(status, status.policy);
}

function offlineProgressDetailsText(
  status: OfflineProgress,
  policy: NonNullable<OfflineProgress['policy']>,
): string {
  return [
    status.enabled ? '사용 중' : '사용 안 함',
    offlineSystemStatusText(status),
    status.online ? '온라인' : '오프라인',
    `대기 ${status.pendingDays.toLocaleString('ko-KR')}일`,
    `처리 ${BigInt(status.processedDays).toLocaleString('ko-KR')}일`,
    `취소 ${BigInt(status.cancelledPendingDays).toLocaleString('ko-KR')}일`,
    `누적 창 ${status.windowAccruedDays.toLocaleString('ko-KR')}/${policy.absenceWindowCapDays.toLocaleString('ko-KR')}일`,
    `${policy.cadenceSeconds.toLocaleString('ko-KR')}초당 1일`,
    offlineLeaseText(status),
  ].join(' · ');
}

function offlineSystemStatusText(status: OfflineProgress): string {
  return status.status === 'pausedBySystem'
    ? `시스템 일시정지 (${status.lastErrorCode ?? '원인 미상'})`
    : '정상';
}

function offlineLeaseText(status: OfflineProgress): string {
  if (status.lease === null) return 'lease 없음';
  const holder = status.lease.holderKind === 'worker' ? '워커' : '온라인';
  return `${holder} lease #${status.lease.generation}`;
}

function reportOfflineProgressUpdate(
  state: AsyncState<OfflineProgress>,
  toasts: ToastQueue,
  refresh: () => void,
): void {
  if (state.status === 'success') {
    const message = state.value.enabled
      ? '현재 실행의 오프라인 진행을 켰습니다.'
      : '현재 실행의 오프라인 진행을 끄고 대기 작업을 취소했습니다.';
    toasts.show(message, { tone: 'success' });
    return;
  }
  if (state.status !== 'error') return;
  const error = state.error;
  const message =
    error instanceof OfflineProgressError
      ? error.message
      : '오프라인 진행 설정을 변경하지 못했습니다.';
  toasts.show(message, { tone: 'error' });
  if (error instanceof OfflineProgressError && error.code === 'revisionConflict') refresh();
}

function finalizationLabel(finalization: RunFinalization): string {
  if (finalization.status === 'pending') {
    return `목표 게임일 ${finalization.targetGameDay.toLocaleString('ko-KR')}일에 결산합니다.`;
  }
  if (finalization.status === 'failed') return `결산 실패: ${finalization.failureCode}`;
  return `세후 순자산 ${formatWon(finalization.afterTaxNetWorthKrw)} · 도산 ${finalization.insolvencyDays.toLocaleString('ko-KR')}일 · 플레이어 명령 ${finalization.playerCommandCount.toLocaleString('ko-KR')}회`;
}

function pendingFinalizationRefreshKey(
  snapshot: GameSnapshot,
  state: AsyncState<RunFinalization>,
): string | undefined {
  if (state.status !== 'success') return undefined;
  const finalization = state.value;
  if (finalization.status !== 'pending') return undefined;
  if (finalization.runRevision !== snapshot.runRevision) return undefined;
  if (snapshot.gameDay < finalization.targetGameDay) return undefined;
  return `${String(snapshot.runRevision)}:${String(finalization.targetGameDay)}`;
}

function hasCharacter(snapshot: GameSnapshot | undefined): snapshot is GameSnapshot {
  return snapshot !== undefined && snapshot.characterName !== null;
}

function isNewFinalizationTarget(
  candidate: string | undefined,
  refreshed: string | undefined,
): candidate is string {
  return candidate !== undefined && candidate !== refreshed;
}

const moneyText = (amount: number | undefined): string =>
  amount === undefined ? '—' : formatWon(amount);

function ledgerStatusTextOf(state: AsyncState<LedgerPage>): string {
  if (state.status === 'idle') return '원장을 불러오지 않았습니다.';
  if (state.status === 'loading') return '원장을 불러오는 중입니다.';
  if (state.status === 'error') return '원장을 불러오지 못했습니다.';
  if (state.value.transactions.length === 0) return '표시할 원장 거래가 없습니다.';
  return `${state.value.transactions.length}개 거래를 최신순으로 표시합니다.`;
}

function cashProductStatusTextOf(state: AsyncState<CashProductCatalog>): string {
  if (state.status === 'idle') return '현금상품 목록을 불러오지 않았습니다.';
  if (state.status === 'loading') return '현금상품 목록을 불러오는 중입니다.';
  if (state.status === 'error') return '현금상품 목록을 불러오지 못했습니다.';
  if (state.value.products.length === 0) return '가입할 수 있는 현금상품이 없습니다.';
  return `${state.value.products.length}개 현금상품을 표시합니다.`;
}

function bondCatalogStatusTextOf(state: AsyncState<BondProductCatalog>): string {
  if (state.status === 'idle') return '국채 목록을 불러오지 않았습니다.';
  if (state.status === 'loading') return '국채 목록을 불러오는 중입니다.';
  if (state.status === 'error') return '국채 목록을 불러오지 못했습니다.';
  if (state.value.products.length === 0) return '이 월드에는 게시된 국채 상품이 없습니다.';
  return `${state.value.products.length}개 상품, ${state.value.series.length}개 시리즈를 표시합니다.`;
}

function goldProductStatusTextOf(state: AsyncState<GoldProductCatalog>): string {
  if (state.status === 'idle') return '금 상품을 불러오지 않았습니다.';
  if (state.status === 'loading') return '금 상품을 불러오는 중입니다.';
  if (state.status === 'error') return '금 상품을 불러오지 못했습니다.';
  if (state.value.products.length === 0) return '이 월드에는 게시된 금 상품이 없습니다.';
  return `${state.value.products.length}개 금 상품을 표시합니다.`;
}

function bondProductCatalogText(catalog: BondProductCatalog | undefined): string {
  if (catalog === undefined) return '—';
  if (catalog.products.length === 0) return `${catalog.marketVersion} · 게시 상품 없음`;
  return [
    `시장 버전 ${catalog.marketVersion}`,
    ...catalog.products.map(
      (product) =>
        `#${product.id} ${product.displayName} [${product.key}] (${product.termYears}년) · 액면 ${formatWon(product.faceValueKrw)} · 주문 ${product.maxOrderUnits} / 보유 ${product.maxPositionUnits}단위 · 수수료 매수 ${product.buyFeePpm}ppm / 매도 ${product.sellFeePpm}ppm`,
    ),
  ].join('\n');
}

function bondSeriesCatalogText(catalog: BondProductCatalog | undefined): string {
  if (catalog === undefined) return '—';
  if (catalog.series.length === 0) return '거래 가능한 시리즈가 없습니다.';
  return catalog.series
    .map(
      (series) =>
        `#${series.id} · 상품 #${series.productVersionId} · ${series.issuedDate} 발행 / ${series.maturityDate} 만기 · 다음 이표 ${series.nextCouponDate} · 발행수익률 ${formatBasisPoints(series.issueYieldBp)} / 표면 ${formatBasisPoints(series.couponRateBp)} / 현재수익률 ${formatBasisPoints(series.currentYieldBp)} · dirty ${formatWon(series.dirtyPriceKrw)}`,
    )
    .join('\n');
}

function goldProductCatalogText(catalog: GoldProductCatalog | undefined): string {
  if (catalog === undefined) return '—';
  if (catalog.products.length === 0) return `${catalog.marketVersion} · 게시 상품 없음`;
  return [
    `시장 버전 ${catalog.marketVersion}`,
    ...catalog.products.map((product) => {
      const bars = product.withdrawalBars
        .map((bar) => `${bar.barSizeGram}g 수수료 ${formatWon(bar.feeKrw)}`)
        .join(' / ');
      return `#${product.id} ${product.displayName} [${product.key}] · 단위 ${product.unit} · 매수 수수료/세금 ${product.buyFeePpm}/${product.buyTaxPpm}ppm · 매도 수수료/세금 ${product.sellFeePpm}/${product.sellTaxPpm}ppm · ${bars}`;
    }),
  ].join('\n');
}

function productBundleText(bundle: GameSnapshot['finance']['productBundle'] | undefined): string {
  if (bundle === undefined) return '—';
  if (bundle === null) return '이 월드는 M2-D 상품 묶음을 사용하지 않습니다.';
  const index = bundle.indexProduct;
  return [
    `LLX #${index.id} ${index.displayName} [${index.key}] · 운용보수 ${index.annualManagementFeePpm}ppm · 분배율 ${index.annualDistributionRatePpm}ppm · Actual/${index.dayCountDenominator}`,
    `LLX 수수료 매수 ${index.buyFeePpm}ppm / 매도 ${index.sellFeePpm}ppm / 매도세 ${index.sellTaxPpm}ppm`,
    `국채 상품: 3년 #${bundle.bondProductVersionIds[0]} / 10년 #${bundle.bondProductVersionIds[1]}`,
    `금 상품: #${bundle.goldProductVersionId}`,
  ].join('\n');
}

function llxPositionsText(positions: GameSnapshot['portfolio']['positions']): string {
  if (positions.length === 0) return '보유한 LLX가 없습니다.';
  return positions
    .map(
      (position) =>
        `계좌 #${position.accountId} · ${position.symbol} ${position.quantity}주 · 원가 ${formatWon(position.costBasisKrw)} · 평균단가 ${formatWon(position.averagePriceKrw)} · 현재가 ${formatWon(position.currentPriceKrw)} · 시가 ${formatWon(position.marketValueKrw)}`,
    )
    .join('\n');
}

function llxEntitlementsText(items: readonly LlxDistributionEntitlement[]): string {
  if (items.length === 0) return '미지급 LLX 분배 권리가 없습니다.';
  return items
    .map(
      (item) =>
        `#${item.id} · 계좌 #${item.accountId} · 기준일 ${item.recordDate} → 지급일 ${item.paymentDate} · ${item.quantity}주 · ${formatWon(item.grossAmountKrw)} · 상태 ${llxEntitlementStatusLabel(item.status)}`,
    )
    .join('\n');
}

function pendingSettlementsText(items: readonly PendingSettlementSummary[]): string {
  if (items.length === 0) return '대기 중인 정산이 없습니다.';
  return items
    .map(
      (item) =>
        `#${item.id} · ${pendingSettlementKindLabel(item.kind)} · ${item.dueGameDay}일차 예정`,
    )
    .join('\n');
}

function bondPositionsText(positions: readonly BondPositionSummary[]): string {
  if (positions.length === 0) return '보유 국채가 없습니다.';
  return positions
    .map(
      (position) =>
        `계좌 #${position.accountId} · 시리즈 #${position.seriesId} · ${position.bondUnits}단위 · dirty ${formatWon(position.dirtyPriceKrw)} · 원가 ${formatWon(position.totalCostBasisKrw)} · 시가 ${formatWon(position.marketValueKrw)} · 미실현 ${formatWon(position.unrealizedGainLossKrw)}`,
    )
    .join('\n');
}

function goldAccountsText(accounts: readonly GoldAccountSummary[]): string {
  if (accounts.length === 0) return '개설된 KRX 금현물 계좌가 없습니다.';
  return accounts
    .map(
      (account) =>
        `계좌 #${account.accountId} · 상품 #${account.productVersionId} · ${account.quantityGram}g · 총원가 ${formatWon(account.totalCostBasisKrw)} · 평균원가 ${account.averageCostKrwPerGram === null ? '—' : formatWon(account.averageCostKrwPerGram)} · 종가/g ${formatWon(account.closeKrwPerGram)} · 시가 ${formatWon(account.marketValueKrw)} · 미실현 ${formatWon(account.unrealizedGainLossKrw)}`,
    )
    .join('\n');
}

function physicalGoldHoldingsText(holdings: readonly PhysicalGoldHolding[]): string {
  if (holdings.length === 0) return '보유 실물 금이 없습니다.';
  return holdings
    .map(
      (holding) =>
        `${holding.barSizeGram}g bar ${holding.barCount}개 · 총 ${holding.totalQuantityGram}g · 종가/g ${formatWon(holding.closeKrwPerGram)} · 시가 ${formatWon(holding.marketValueKrw)}`,
    )
    .join('\n');
}

function financialIncomeYearText(year: FinancialIncomeYear | undefined): string {
  if (year === undefined) return '—';
  const sources =
    year.sources.length === 0
      ? '원천 없음'
      : year.sources
          .map(
            (source) =>
              `${financialIncomeSourceLabel(source.source)} ${formatWon(source.grossFinancialIncomeKrw)} (소득세 ${formatWon(source.withheldIncomeTaxKrw)}, 지방세 ${formatWon(source.withheldLocalIncomeTaxKrw)})`,
          )
          .join('\n');
  return [
    `${year.taxYear}년 · ${financialIncomeStatusLabel(year.status)}`,
    sources,
    ...financialIncomeAssessmentLines(year),
  ].join('\n');
}

function financialIncomeAssessmentText(
  assessment: FinancialIncomeAssessment | null | undefined,
): string {
  if (assessment === undefined) return '—';
  if (assessment === null) return '확정된 과거 세액 산정이 없습니다.';
  return [
    `${assessment.taxYear}년 · ${financialIncomeStatusLabel(assessment.status)}`,
    `금융소득 ${formatWon(assessment.grossFinancialIncomeKrw)} · 소득세 원천징수 ${formatWon(assessment.withheldIncomeTaxKrw)} · 지방소득세 원천징수 ${formatWon(assessment.withheldLocalIncomeTaxKrw)}`,
    ...financialIncomeAssessmentLines(assessment),
  ].join('\n');
}

function financialIncomeAssessmentLines(
  assessment: FinancialIncomeYear | FinancialIncomeAssessment,
): readonly string[] {
  if (assessment.status === 'notApplicable' || assessment.status === 'open') {
    return ['세액 미확정'];
  }
  return [
    `비교 A 소득세/지방세 ${formatWon(assessment.comparisonAIncomeTaxKrw)} / ${formatWon(assessment.comparisonALocalIncomeTaxKrw)}`,
    `비교 B 소득세/지방세 ${formatWon(assessment.comparisonBIncomeTaxKrw)} / ${formatWon(assessment.comparisonBLocalIncomeTaxKrw)}`,
    `확정 소득세/지방세 ${formatWon(assessment.assessedIncomeTaxKrw)} / ${formatWon(assessment.assessedLocalIncomeTaxKrw)}`,
    `추가 납부 ${formatWon(assessment.additionalTaxKrw)} · 환급 ${formatWon(assessment.refundKrw)}`,
    `신고기한 ${assessment.filingDueDate ?? '해당 없음'} · 신고 게임일 ${assessment.filedGameDay ?? '미신고'}`,
  ];
}

function financialIncomeStatusLabel(status: FinancialIncomeYear['status']): string {
  const labels: Record<FinancialIncomeYear['status'], string> = {
    notApplicable: '비적용',
    open: '집계 중',
    finalizedNoFiling: '확정·신고 없음',
    filingPending: '신고 대기',
    filed: '신고 완료',
  };
  return labels[status];
}

function financialIncomeSourceLabel(
  source: FinancialIncomeYear['sources'][number]['source'],
): string {
  const labels: Record<FinancialIncomeYear['sources'][number]['source'], string> = {
    cmaInterest: 'CMA 이자',
    depositInterest: '예금 이자',
    bondCoupon: '국채 이표',
    llxDistribution: 'LLX 분배',
    isaEarlyClose: 'ISA 조기해지',
  };
  return labels[source];
}

function llxEntitlementStatusLabel(status: LlxDistributionEntitlement['status']): string {
  const labels: Record<LlxDistributionEntitlement['status'], string> = {
    pending: '지급 대기',
  };
  return labels[status];
}

function pendingSettlementKindLabel(kind: PendingSettlementSummary['kind']): string {
  const labels: Record<PendingSettlementSummary['kind'], string> = {
    cmaInterest: 'CMA 이자',
    depositMaturity: '예금 만기',
    savingsInstallment: '적금 납입',
    savingsMaturity: '적금 만기',
    bondCoupon: '국채 이표',
    bondMaturity: '국채 만기',
    llxDistribution: 'LLX 분배',
    financialIncomeFiling: '금융소득 신고',
    employmentPayroll: '급여 지급',
    employmentReconciliation: '연말정산 조정',
    militaryPay: '군 급여 지급',
    militarySavingsInstallment: '장병적금 납입',
    militarySavingsMaturity: '장병적금 만기',
    militarySavingsGovernmentMatch: '장병적금 정부지원금',
    loanInstallment: '대출 정기 상환',
    leaseRent: '월세 납부',
    livingCostMonth: '월 생활비',
    propertyTaxPayment: '부동산 세금 납부',
    welfareBenefitPayment: '복지 급여 지급',
    insurancePremium: '보험료 납부',
  };
  return labels[kind];
}

const rateText = (basisPoints: number | undefined): string =>
  basisPoints === undefined ? '이 월드에는 금리 팩터 없음' : formatBasisPoints(basisPoints);

const stepLabel = (unit: StepUnit): string =>
  unit === 'day' ? '1일 진행' : unit === 'week' ? '1주 진행' : '1개월 진행';

function fixedSelectOptions(
  capacity: number,
  placeholder: string,
): readonly { readonly value: string; readonly label: string }[] {
  return [
    { value: '', label: placeholder },
    ...Array.from({ length: capacity }, (_, index) => ({
      value: `__slot-${index}`,
      label: '—',
    })),
  ];
}

function updateFixedSelectOptions(
  form: HTMLFormElement,
  fieldName: string,
  choices: readonly { readonly value: string; readonly label: string }[],
  preferredValue: string | undefined,
): void {
  const field = form.elements.namedItem(fieldName);
  if (!(field instanceof HTMLSelectElement)) {
    throw new Error(`fixed select field is missing: ${fieldName}`);
  }
  const capacity = field.options.length - 1;
  if (choices.length > capacity) {
    throw new Error(`fixed select capacity exceeded: ${fieldName}`);
  }

  const previousValue = field.value;
  for (let index = 0; index < capacity; index += 1) {
    const option = field.options.item(index + 1);
    if (option === null) continue;
    const choice = choices[index];
    option.value = choice?.value ?? `__slot-${index}`;
    option.textContent = choice?.label ?? '—';
    option.hidden = choice === undefined;
    option.disabled = choice === undefined;
  }

  const hasPrevious = choices.some((choice) => choice.value === previousValue);
  const hasPreferred = choices.some((choice) => choice.value === preferredValue);
  field.value = hasPrevious
    ? previousValue
    : hasPreferred
      ? (preferredValue ?? '')
      : (choices[0]?.value ?? '');
}

function createOrderSubmitter(
  deps: OrderSubmitterDeps,
): (draft: PortfolioOrderDraft) => Promise<void> {
  const retries = createOrderRetryPolicy({ createOrderId: deps.createOrderId });

  return async (draft) => {
    const state = deps.store.getState();
    const current = state.game.snapshot;
    if (!canSubmitOrder(state, current)) {
      throw new Error('현재 상태에서는 주문할 수 없습니다.');
    }

    const request = retries.select(current, draft);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.placePortfolioOrder(request);
      retries.clear(request);
      deps.snapshots.apply(response.snapshot);
      deps.toasts.show(executionMessage(response.execution), { tone: 'success' });
    } catch (error) {
      if (error instanceof PortfolioOrderError) retries.clear(request);
      else retries.retain(request);
      throw orderDisplayError(error);
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };
}

function createFinanceTransferSubmitter(
  deps: FinanceTransferSubmitterDeps,
): (draft: FinanceTransferDraft) => Promise<void> {
  const retries = createFinanceTransferRetryPolicy({ createCommandId: deps.createCommandId });

  return async (draft) => {
    const state = deps.store.getState();
    const current = state.game.snapshot;
    if (!canSubmitFinanceCommand(state, current)) {
      throw new Error('현재 상태에서는 이체할 수 없습니다.');
    }

    const request = retries.select(current, draft);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.transferFinance(request);
      retries.clear(request);
      deps.snapshots.apply(response.snapshot);
      deps.toasts.show(transferMessage(response.transfer.replayed), { tone: 'success' });
    } catch (error) {
      if (error instanceof FinanceCommandError) retries.clear(request);
      else retries.retain(request);
      throw financeDisplayError(error);
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };
}

function createCmaAccountOpenSubmitter(
  deps: CashProductSubmitterDeps,
): (draft: CmaAccountOpenDraft) => Promise<void> {
  const retries = createCmaAccountOpenRetryPolicy({ createCommandId: deps.createCommandId });

  return async (draft) => {
    const state = deps.store.getState();
    const current = state.game.snapshot;
    if (!canSubmitFinanceCommand(state, current)) {
      throw new Error('현재 상태에서는 CMA 계좌를 개설할 수 없습니다.');
    }

    const request = retries.select(current, draft);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.openCmaAccount(request);
      retries.complete(request);
      deps.snapshots.apply(response.snapshot);
      deps.refreshLatestLedger();
      deps.toasts.show(
        response.account.replayed
          ? '이미 처리된 CMA 개설 결과를 확인했습니다.'
          : `CMA 계좌 #${response.account.accountId}을 개설했습니다.`,
        { tone: 'success' },
      );
    } catch (error) {
      retries.fail(request, error);
      throw cashProductDisplayError(error, 'CMA 개설');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };
}

function createCmaAccountCloseSubmitter(
  deps: CashProductSubmitterDeps,
): (draft: CmaAccountCloseDraft) => Promise<void> {
  const retries = createCmaAccountCloseRetryPolicy({ createCommandId: deps.createCommandId });

  return async (draft) => {
    const state = deps.store.getState();
    const current = state.game.snapshot;
    if (!canSubmitFinanceCommand(state, current)) {
      throw new Error('현재 상태에서는 CMA 계좌를 종료할 수 없습니다.');
    }

    const command = retries.select(current, draft);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.closeCmaAccount(command.accountId, command.request);
      retries.complete(command);
      deps.snapshots.apply(response.snapshot);
      deps.refreshLatestLedger();
      deps.toasts.show(
        response.accountClose.replayed
          ? '이미 처리된 CMA 종료 결과를 확인했습니다.'
          : `CMA 계좌 #${response.accountClose.accountId}을 종료했습니다.`,
        { tone: 'success' },
      );
    } catch (error) {
      retries.fail(command, error);
      throw cashProductDisplayError(error, 'CMA 종료');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };
}

function createDepositOpenSubmitter(
  deps: CashProductSubmitterDeps,
): (draft: DepositOpenDraft) => Promise<void> {
  const retries = createDepositOpenRetryPolicy({ createCommandId: deps.createCommandId });

  return async (draft) => {
    const state = deps.store.getState();
    const current = state.game.snapshot;
    if (!canSubmitFinanceCommand(state, current)) {
      throw new Error('현재 상태에서는 예금·적금에 가입할 수 없습니다.');
    }

    const request = retries.select(current, draft);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.openDeposit(request);
      retries.complete(request);
      deps.snapshots.apply(response.snapshot);
      deps.refreshLatestLedger();
      deps.toasts.show(
        response.deposit.replayed
          ? '이미 처리된 예금·적금 가입 결과를 확인했습니다.'
          : `${cashContractKindLabel(response.deposit.kind)} 계약 #${response.deposit.contractId}에 가입했습니다.`,
        { tone: 'success' },
      );
    } catch (error) {
      retries.fail(request, error);
      throw cashProductDisplayError(error, '예금·적금 가입');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };
}

function createDepositCloseSubmitter(
  deps: CashProductSubmitterDeps,
): (draft: DepositCloseDraft) => Promise<void> {
  const retries = createDepositCloseRetryPolicy({ createCommandId: deps.createCommandId });

  return async (draft) => {
    const state = deps.store.getState();
    const current = state.game.snapshot;
    if (!canSubmitFinanceCommand(state, current)) {
      throw new Error('현재 상태에서는 예금·적금을 중도해지할 수 없습니다.');
    }

    const command = retries.select(current, draft);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.closeDeposit(command.contractId, command.request);
      retries.complete(command);
      deps.snapshots.apply(response.snapshot);
      deps.refreshLatestLedger();
      deps.toasts.show(
        response.depositClose.replayed
          ? '이미 처리된 중도해지 결과를 확인했습니다.'
          : `계약 #${response.depositClose.contractId} 중도해지 지급액: ${formatWon(response.depositClose.netPayoutKrw)}`,
        { tone: 'success' },
      );
    } catch (error) {
      retries.fail(command, error);
      throw cashProductDisplayError(error, '예금·적금 중도해지');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };
}

function createTaxAccountOpenSubmitter(
  deps: TaxAccountSubmitterDeps,
): (draft: TaxAccountOpenDraft) => Promise<void> {
  const retries = createTaxAccountOpenRetryPolicy({ createCommandId: deps.createCommandId });

  return async (draft) => {
    const state = deps.store.getState();
    const current = state.game.snapshot;
    if (!canSubmitFinanceCommand(state, current)) {
      throw new Error('현재 상태에서는 절세계좌를 개설할 수 없습니다.');
    }

    const request = retries.select(current, draft);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.openTaxAccount(request);
      retries.complete(request);
      deps.snapshots.apply(response.snapshot);
      deps.refreshLatestLedger();
      deps.toasts.show(
        response.account.replayed
          ? '이미 처리된 절세계좌 개설 결과를 확인했습니다.'
          : `${accountTypeLabel(response.account.type)} #${response.account.accountId}을 개설했습니다.`,
        { tone: 'success' },
      );
    } catch (error) {
      retries.fail(request, error);
      throw taxAccountDisplayError(error, '절세계좌 개설');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };
}

function createIsaAccountCloseSubmitter(
  deps: TaxAccountSubmitterDeps,
): (draft: IsaAccountCloseDraft) => Promise<void> {
  const retries = createIsaAccountCloseRetryPolicy({ createCommandId: deps.createCommandId });

  return async (draft) => {
    const state = deps.store.getState();
    const current = state.game.snapshot;
    if (!canSubmitFinanceCommand(state, current)) {
      throw new Error('현재 상태에서는 ISA를 해지할 수 없습니다.');
    }

    const command = retries.select(current, draft);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.closeIsaAccount(command.accountId, command.request);
      retries.complete(command);
      deps.snapshots.apply(response.snapshot);
      deps.refreshLatestLedger();
      deps.toasts.show(
        response.isaClose.replayed
          ? '이미 처리된 ISA 해지 결과를 확인했습니다.'
          : `ISA #${response.isaClose.accountId} 해지 지급액: ${formatWon(response.isaClose.netPayoutKrw)}`,
        { tone: 'success' },
      );
    } catch (error) {
      retries.fail(command, error);
      throw taxAccountDisplayError(error, 'ISA 해지');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };
}

function createPensionStartSubmitter(
  deps: TaxAccountSubmitterDeps,
): (draft: PensionStartDraft) => Promise<void> {
  const retries = createPensionStartRetryPolicy({ createCommandId: deps.createCommandId });

  return async (draft) => {
    const state = deps.store.getState();
    const current = state.game.snapshot;
    if (!canSubmitFinanceCommand(state, current)) {
      throw new Error('현재 상태에서는 연금 수령을 개시할 수 없습니다.');
    }

    const command = retries.select(current, draft);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.startPension(command.accountId, command.request);
      retries.complete(command);
      deps.snapshots.apply(response.snapshot);
      deps.refreshLatestLedger();
      deps.toasts.show(
        response.pensionStart.replayed
          ? '이미 처리된 연금 개시 결과를 확인했습니다.'
          : `연금 #${response.pensionStart.accountId} 수령을 ${response.pensionStart.startTaxYear}년에 개시했습니다.`,
        { tone: 'success' },
      );
    } catch (error) {
      retries.fail(command, error);
      throw taxAccountDisplayError(error, '연금 개시');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };
}

function createPensionWithdrawalSubmitter(
  deps: TaxAccountSubmitterDeps,
): (draft: PensionWithdrawalDraft) => Promise<void> {
  const retries = createPensionWithdrawalRetryPolicy({ createCommandId: deps.createCommandId });

  return async (draft) => {
    const state = deps.store.getState();
    const current = state.game.snapshot;
    if (!canSubmitFinanceCommand(state, current)) {
      throw new Error('현재 상태에서는 연금계좌에서 인출할 수 없습니다.');
    }

    const command = retries.select(current, draft);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.withdrawPension(command.accountId, command.request);
      retries.complete(command);
      deps.snapshots.apply(response.snapshot);
      deps.refreshLatestLedger();
      deps.toasts.show(
        response.pensionWithdrawal.replayed
          ? '이미 처리된 연금 인출 결과를 확인했습니다.'
          : `연금계좌 #${response.pensionWithdrawal.accountId} 세후 지급액: ${formatWon(response.pensionWithdrawal.netPayoutKrw)}`,
        { tone: 'success' },
      );
    } catch (error) {
      retries.fail(command, error);
      throw taxAccountDisplayError(error, '연금계좌 인출');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };
}

function createBondOrderSubmitter(
  deps: AssetSubmitterDeps,
): (draft: BondOrderDraft) => Promise<void> {
  const retries = createFinanceAssetRetryPolicy<BondOrderDraft, BondOrderRequest>({
    createCommandId: deps.createCommandId,
    draftKey: (runRevision, draft) =>
      JSON.stringify([runRevision, draft.accountId, draft.seriesId, draft.side, draft.bondUnits]),
    requestKey: (request) =>
      JSON.stringify([
        request.expectedRunRevision,
        request.accountId,
        request.seriesId,
        request.side,
        request.bondUnits,
      ]),
    requestOf: (snapshot, draft, commandId) => ({
      ...financeCommandOf(snapshot, commandId),
      ...draft,
    }),
  });

  return async (draft) => {
    const state = deps.store.getState();
    const current = state.game.snapshot;
    if (!canSubmitMarketAssetCommand(state, current)) {
      throw new Error('현재 상태에서는 국채를 주문할 수 없습니다.');
    }

    const request = retries.select(current, draft);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.placeBondOrder(request);
      retries.complete(request);
      deps.snapshots.apply(response.snapshot);
      deps.refreshLatestLedger();
      deps.toasts.show(bondOrderMessage(response.bondOrder), { tone: 'success' });
    } catch (error) {
      retries.fail(request, error);
      throw assetDisplayError(error, '국채 주문');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };
}

function createGoldAccountOpenSubmitter(
  deps: AssetSubmitterDeps,
): (draft: GoldAccountOpenDraft) => Promise<void> {
  const retries = createFinanceAssetRetryPolicy<GoldAccountOpenDraft, GoldAccountOpenRequest>({
    createCommandId: deps.createCommandId,
    draftKey: (runRevision, draft) =>
      JSON.stringify([runRevision, draft.type, draft.productVersionId]),
    requestKey: (request) =>
      JSON.stringify([request.expectedRunRevision, request.type, request.productVersionId]),
    requestOf: (snapshot, draft, commandId) => ({
      ...financeCommandOf(snapshot, commandId),
      ...draft,
    }),
  });

  return async (draft) => {
    const state = deps.store.getState();
    const current = state.game.snapshot;
    if (!canSubmitFinanceCommand(state, current) || current.finance.productBundle === null) {
      throw new Error('현재 상태에서는 KRX 금현물 계좌를 개설할 수 없습니다.');
    }

    const request = retries.select(current, draft);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.openGoldAccount(request);
      retries.complete(request);
      deps.snapshots.apply(response.snapshot);
      deps.refreshLatestLedger();
      deps.toasts.show(
        response.account.replayed
          ? '이미 처리된 금 계좌 개설 결과를 확인했습니다.'
          : `KRX 금현물 계좌 #${response.account.accountId}을 개설했습니다.`,
        { tone: 'success' },
      );
    } catch (error) {
      retries.fail(request, error);
      throw assetDisplayError(error, '금 계좌 개설');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };
}

function createGoldOrderSubmitter(
  deps: AssetSubmitterDeps,
): (draft: GoldOrderDraft) => Promise<void> {
  const retries = createFinanceAssetRetryPolicy<GoldOrderDraft, GoldOrderRequest>({
    createCommandId: deps.createCommandId,
    draftKey: (runRevision, draft) =>
      JSON.stringify([runRevision, draft.accountId, draft.side, draft.quantityGram]),
    requestKey: (request) =>
      JSON.stringify([
        request.expectedRunRevision,
        request.accountId,
        request.side,
        request.quantityGram,
      ]),
    requestOf: (snapshot, draft, commandId) => ({
      ...financeCommandOf(snapshot, commandId),
      ...draft,
    }),
  });

  return async (draft) => {
    const state = deps.store.getState();
    const current = state.game.snapshot;
    if (!canSubmitMarketAssetCommand(state, current)) {
      throw new Error('현재 상태에서는 금을 주문할 수 없습니다.');
    }

    const request = retries.select(current, draft);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.placeGoldOrder(request);
      retries.complete(request);
      deps.snapshots.apply(response.snapshot);
      deps.refreshLatestLedger();
      deps.toasts.show(goldOrderMessage(response.goldOrder), { tone: 'success' });
    } catch (error) {
      retries.fail(request, error);
      throw assetDisplayError(error, '금 주문');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };
}

function createGoldWithdrawalSubmitter(
  deps: AssetSubmitterDeps,
): (draft: GoldWithdrawalDraft) => Promise<void> {
  const retries = createFinanceAssetRetryPolicy<GoldWithdrawalDraft, GoldWithdrawalRequest>({
    createCommandId: deps.createCommandId,
    draftKey: (runRevision, draft) =>
      JSON.stringify([runRevision, draft.accountId, draft.barSizeGram, draft.barCount]),
    requestKey: (request) =>
      JSON.stringify([
        request.expectedRunRevision,
        request.accountId,
        request.barSizeGram,
        request.barCount,
      ]),
    requestOf: (snapshot, draft, commandId) => ({
      ...financeCommandOf(snapshot, commandId),
      ...draft,
    }),
  });

  return async (draft) => {
    const state = deps.store.getState();
    const current = state.game.snapshot;
    if (!canSubmitM2dFinanceCommand(state, current)) {
      throw new Error('현재 상태에서는 실물 금을 인출할 수 없습니다.');
    }

    const request = retries.select(current, draft);
    deps.store.set(paths.gameOrdering, true);
    try {
      const response = await deps.api.withdrawGold(request);
      retries.complete(request);
      deps.snapshots.apply(response.snapshot);
      deps.refreshLatestLedger();
      deps.toasts.show(goldWithdrawalMessage(response.goldWithdrawal), { tone: 'success' });
    } catch (error) {
      retries.fail(request, error);
      throw assetDisplayError(error, '실물 금 인출');
    } finally {
      deps.store.set(paths.gameOrdering, false);
    }
  };
}

function financeCommandOf(snapshot: GameCommandCursor, commandId: string): FinanceCommandRequest {
  return {
    commandId,
    expectedRunRevision: snapshot.runRevision,
    expectedStateRevision: snapshot.stateRevision,
    expectedGameDay: snapshot.gameDay,
  };
}

function cashProductDisplayError(error: unknown, action: string): Error {
  if (error instanceof FinanceCommandError) return error;
  return new Error(`${action} 결과를 확인하지 못했습니다. 같은 내용으로 다시 제출해 주세요.`, {
    cause: error,
  });
}

function taxAccountDisplayError(error: unknown, action: string): Error {
  if (error instanceof FinanceCommandError) return error;
  return new Error(`${action} 결과를 확인하지 못했습니다. 같은 내용으로 다시 제출해 주세요.`, {
    cause: error,
  });
}

function assetDisplayError(error: unknown, action: string): Error {
  if (error instanceof FinanceCommandError) return error;
  return new Error(`${action} 결과를 확인하지 못했습니다. 같은 내용으로 다시 제출해 주세요.`, {
    cause: error,
  });
}

function financeDisplayError(error: unknown): Error {
  if (error instanceof FinanceCommandError) return error;
  return new Error('이체 결과를 확인하지 못했습니다. 같은 내용으로 다시 제출해 주세요.', {
    cause: error,
  });
}

function transferMessage(replayed: boolean): string {
  return replayed ? '이미 처리된 이체 결과를 확인했습니다.' : '이체를 완료했습니다.';
}

function orderDisplayError(error: unknown): Error {
  if (error instanceof PortfolioOrderError) return error;
  return new Error('주문 결과를 확인하지 못했습니다. 같은 내용으로 다시 제출해 주세요.', {
    cause: error,
  });
}

function canSubmitOrder(
  state: AppState,
  snapshot: GameSnapshot | undefined,
): snapshot is GameSnapshot {
  return (
    snapshot?.characterName !== undefined &&
    snapshot.characterName !== null &&
    snapshot.market.open &&
    snapshot.autoSpeed === null &&
    !state.game.advancing &&
    !state.game.ordering
  );
}

function canSubmitFinanceCommand(
  state: AppState,
  snapshot: GameSnapshot | undefined,
): snapshot is GameSnapshot {
  return (
    snapshot?.characterName !== undefined &&
    snapshot.characterName !== null &&
    snapshot.autoSpeed === null &&
    !state.game.advancing &&
    !state.game.ordering
  );
}

function canSubmitMarketAssetCommand(
  state: AppState,
  snapshot: GameSnapshot | undefined,
): snapshot is GameSnapshot {
  return (
    canSubmitFinanceCommand(state, snapshot) &&
    snapshot.market.open &&
    snapshot.market.m2Factors !== null &&
    snapshot.finance.productBundle !== null
  );
}

function canSubmitM2dFinanceCommand(
  state: AppState,
  snapshot: GameSnapshot | undefined,
): snapshot is GameSnapshot {
  return canSubmitFinanceCommand(state, snapshot) && snapshot.finance.productBundle !== null;
}

function executionMessage(execution: PortfolioExecution): string {
  if (execution.replayed) return '이미 처리된 주문 결과를 확인했습니다.';
  const side = execution.side === 'buy' ? '매수' : '매도';
  return `${execution.quantity}주 ${side} 체결: ${formatWon(execution.grossAmountKrw)}`;
}

function bondOrderMessage(order: BondOrderResult): string {
  if (order.replayed) return '이미 처리된 국채 주문 결과를 확인했습니다.';
  const side = order.side === 'buy' ? '매수' : '매도';
  const realized =
    order.side === 'sell' ? ` · 실현손익 ${formatWon(order.realizedGainLossKrw)}` : '';
  return `${order.bondUnits}단위 ${side} 체결: ${formatWon(order.grossAmountKrw)}${realized}`;
}

function goldOrderMessage(order: GoldOrderResult): string {
  if (order.replayed) return '이미 처리된 금 주문 결과를 확인했습니다.';
  const side = order.side === 'buy' ? '매수' : '매도';
  const realized =
    order.side === 'sell' ? ` · 실현손익 ${formatWon(order.realizedGainLossKrw)}` : '';
  return `${order.quantityGram}g ${side} 체결: ${formatWon(order.grossAmountKrw)}${realized}`;
}

function goldWithdrawalMessage(withdrawal: GoldWithdrawalResult): string {
  if (withdrawal.replayed) return '이미 처리된 실물 금 인출 결과를 확인했습니다.';
  return `${withdrawal.barSizeGram}g bar ${withdrawal.barCount}개 인출 · 현금 청구 ${formatWon(withdrawal.cashChargedKrw)}`;
}

function accountTypeAllowsLlx(
  accountType: FinancialAccount['type'],
  supportsM2dTaxAccounts: boolean,
): boolean {
  return (
    accountType === 'taxableBrokerage' ||
    (supportsM2dTaxAccounts && accountTypeAllowsBond(accountType))
  );
}

function accountTypeAllowsBond(accountType: FinancialAccount['type']): boolean {
  return (
    accountType === 'taxableBrokerage' ||
    accountType === 'isaGeneral' ||
    accountType === 'isaLowIncome' ||
    accountType === 'pensionSavings' ||
    accountType === 'irp'
  );
}

function accountTypeLabel(accountType: FinancialAccount['type']): string {
  const labels: Record<FinancialAccount['type'], string> = {
    taxableBrokerage: '일반 투자계좌',
    cma: 'CMA',
    isaGeneral: '일반형 ISA',
    isaLowIncome: '서민형 ISA',
    pensionSavings: '연금저축',
    irp: 'IRP',
    krxGold: 'KRX 금현물',
  };
  return labels[accountType];
}

function accountStatusLabel(status: FinancialAccount['status']): string {
  return status === 'open' ? '사용 중' : status === 'matured' ? '만기' : '닫힘';
}

function createFixedTable<T>(
  body: HTMLTableSectionElement,
  capacity: number,
  createRow: () => FixedRow<T>,
): FixedTable<T> {
  const rows: FixedRow<T>[] = [];
  for (let index = 0; index < capacity; index += 1) {
    const row = createRow();
    rows.push(row);
    body.appendChild(row.element);
  }

  return {
    setItems(items) {
      for (let index = 0; index < rows.length; index += 1) {
        rows[index]?.setItem(items[index]);
      }
    },
  };
}

function createCashProductRow(): FixedRow<CashProduct> {
  const id = el('td');
  const key = el('td');
  const kind = el('td');
  const institution = el('td');
  const rate = el('td');
  const terms = el('td');
  const updateId = bindText(id);
  const updateKey = bindText(key);
  const updateKind = bindText(kind);
  const updateInstitution = bindText(institution);
  const updateRate = bindText(rate);
  const updateTerms = bindText(terms);
  const element = el('tr', {}, id, key, kind, institution, rate, terms);
  element.hidden = true;

  return {
    element,
    setItem(product) {
      setRowHidden(element, product === undefined);
      if (product === undefined) return;
      updateId(product.id);
      updateKey(product.key);
      updateKind(cashProductKindLabel(product.kind));
      updateInstitution(
        `${product.institution.displayName} (#${product.institution.id}, ${product.institution.key})`,
      );
      updateRate(
        `국고채 3개월 ${spreadText(product.spreadBp)} · 일수 ${product.dayCountDenominator}`,
      );
      updateTerms(cashProductTermsText(product));
    },
  };
}

function createCmaAccountRow(): FixedRow<CmaAccountSummary> {
  const accountId = el('td');
  const productVersionId = el('td');
  const annualRate = el('td');
  const minimumBalance = el('td');
  const remainder = el('td');
  const updateAccountId = bindText(accountId);
  const updateProductVersionId = bindText(productVersionId);
  const updateAnnualRate = bindText(annualRate);
  const updateMinimumBalance = bindText(minimumBalance);
  const updateRemainder = bindText(remainder);
  const element = el('tr', {}, accountId, productVersionId, annualRate, minimumBalance, remainder);
  element.hidden = true;

  return {
    element,
    setItem(account) {
      setRowHidden(element, account === undefined);
      if (account === undefined) return;
      updateAccountId(account.accountId);
      updateProductVersionId(account.productVersionId);
      updateAnnualRate(
        account.annualRateBp === null ? '금리 팩터 없음' : formatBasisPoints(account.annualRateBp),
      );
      updateMinimumBalance(formatWon(account.minimumInterestBalanceKrw));
      updateRemainder(account.interestRemainder.toLocaleString('ko-KR'));
    },
  };
}

function createCashContractRow(): FixedRow<CashContractSummary> {
  const contractId = el('td');
  const productVersionId = el('td');
  const settlementAccountId = el('td');
  const kindAndStatus = el('td');
  const annualRate = el('td');
  const principal = el('td');
  const installment = el('td');
  const period = el('td');
  const expected = el('td');
  const updateContractId = bindText(contractId);
  const updateProductVersionId = bindText(productVersionId);
  const updateSettlementAccountId = bindText(settlementAccountId);
  const updateKindAndStatus = bindText(kindAndStatus);
  const updateAnnualRate = bindText(annualRate);
  const updatePrincipal = bindText(principal);
  const updateInstallment = bindText(installment);
  const updatePeriod = bindText(period);
  const updateExpected = bindText(expected);
  const element = el(
    'tr',
    {},
    contractId,
    productVersionId,
    settlementAccountId,
    kindAndStatus,
    annualRate,
    principal,
    installment,
    period,
    expected,
  );
  element.hidden = true;

  return {
    element,
    setItem(contract) {
      setRowHidden(element, contract === undefined);
      if (contract === undefined) return;
      updateContractId(contract.contractId);
      updateProductVersionId(contract.productVersionId);
      updateSettlementAccountId(contract.settlementAccountId);
      updateKindAndStatus(
        `${cashContractKindLabel(contract.kind)} · ${cashContractStatusLabel(contract.status)}`,
      );
      updateAnnualRate(formatBasisPoints(contract.annualRateBp));
      updatePrincipal(formatWon(contract.currentPrincipalKrw));
      updateInstallment(cashContractInstallmentText(contract));
      updatePeriod(`${contract.openedGameDay}일차 → ${contract.maturityGameDay}일차`);
      updateExpected(cashContractExpectedText(contract));
    },
  };
}

function createDepositProtectionRow(): FixedRow<DepositProtectionSummary> {
  const institutionId = el('td');
  const eligible = el('td');
  const protectedAmount = el('td');
  const unprotected = el('td');
  const updateInstitutionId = bindText(institutionId);
  const updateEligible = bindText(eligible);
  const updateProtected = bindText(protectedAmount);
  const updateUnprotected = bindText(unprotected);
  const element = el('tr', {}, institutionId, eligible, protectedAmount, unprotected);
  element.hidden = true;

  return {
    element,
    setItem(summary) {
      setRowHidden(element, summary === undefined);
      if (summary === undefined) return;
      updateInstitutionId(summary.institutionId);
      updateEligible(formatWon(summary.eligibleAmountKrw));
      updateProtected(formatWon(summary.protectedAmountKrw));
      updateUnprotected(formatWon(summary.unprotectedAmountKrw));
    },
  };
}

function createIsaAccountRow(): FixedRow<IsaAccountSummary> {
  const accountId = el('td');
  const type = el('td');
  const period = el('td');
  const contribution = el('td');
  const taxResult = el('td');
  const expectedTax = el('td');
  const updateAccountId = bindText(accountId);
  const updateType = bindText(type);
  const updatePeriod = bindText(period);
  const updateContribution = bindText(contribution);
  const updateTaxResult = bindText(taxResult);
  const updateExpectedTax = bindText(expectedTax);
  const element = el('tr', {}, accountId, type, period, contribution, taxResult, expectedTax);
  element.hidden = true;

  return {
    element,
    setItem(account) {
      setRowHidden(element, account === undefined);
      if (account === undefined) return;
      updateAccountId(account.accountId);
      updateType(accountTypeLabel(account.type));
      updatePeriod(
        `${account.openedGameDay}일차 가입 · ${account.minimumTermGameDay}일차 의무기간`,
      );
      updateContribution(
        `납입 ${formatWon(account.totalContributionKrw)} · 원금 인출 ${formatWon(account.principalWithdrawalKrw)} · 남은 납입 여력 ${formatWon(account.contributionCapacityKrw)}`,
      );
      updateTaxResult(
        `과세이익 ${formatWon(account.taxProfitKrw)} · 공제손실 ${formatWon(account.deductibleLossKrw)}`,
      );
      updateExpectedTax(
        `소득세 ${formatWon(account.expectedCloseIncomeTaxKrw)} · 지방소득세 ${formatWon(account.expectedCloseLocalIncomeTaxKrw)}`,
      );
    },
  };
}

function createPensionAccountRow(): FixedRow<PensionAccountSummary> {
  const identity = el('td');
  const period = el('td');
  const layers = el('td');
  const contribution = el('td');
  const limit = el('td');
  const risk = el('td');
  const updateIdentity = bindText(identity);
  const updatePeriod = bindText(period);
  const updateLayers = bindText(layers);
  const updateContribution = bindText(contribution);
  const updateLimit = bindText(limit);
  const updateRisk = bindText(risk);
  const element = el('tr', {}, identity, period, layers, contribution, limit, risk);
  element.hidden = true;

  return {
    element,
    setItem(account) {
      setRowHidden(element, account === undefined);
      if (account === undefined) return;
      updateIdentity(`#${account.accountId} · ${accountTypeLabel(account.type)}`);
      updatePeriod(
        `${account.openedGameDay}일차 가입 · ${account.eligiblePensionStartGameDay}일차 개시 가능 · ${account.pensionStarted ? '개시됨' : '적립 중'}`,
      );
      updateLayers(
        `공제 전 납입 ${formatWon(account.taxLayers.taxExcludedContributionKrw)} · 이연퇴직소득 ${formatWon(account.taxLayers.deferredRetirementIncomeKrw)} · 공제받은 납입 ${formatWon(account.taxLayers.creditedContributionKrw)} · 운용수익 ${formatWon(account.taxLayers.earningsKrw)}`,
      );
      updateContribution(
        `납입 ${formatWon(account.currentYearContributionKrw)} · 공제대상 ${formatWon(account.currentYearCreditEligibleKrw)} · 예상공제 ${formatWon(account.expectedCreditKrw)}`,
      );
      updateLimit(
        `한도 ${account.currentYearPensionLimitKrw === null ? '해당 없음' : formatWon(account.currentYearPensionLimitKrw)} · 인출 ${formatWon(account.currentYearPensionWithdrawnKrw)}`,
      );
      updateRisk(
        `위험자산 ${formatWon(account.riskAssetValueKrw)} / 총 ${formatWon(account.totalValueKrw)} · ${formatReturnPpm(account.riskAssetRatioPpm)}`,
      );
    },
  };
}

function setRowHidden(row: HTMLTableRowElement, hidden: boolean): void {
  if (row.hidden !== hidden) row.hidden = hidden;
}

function insuranceSnapshotSummary(life: GameSnapshot['life']): string {
  return life.insuranceCapability === 'unavailable'
    ? '보험 미지원'
    : `활성 보험 ${life.activeInsuranceContracts.length}건 · 청구 대기 ${life.pendingInsuranceClaims.length}건`;
}

function cashProductKindLabel(kind: CashProduct['kind']): string {
  const labels: Record<CashProduct['kind'], string> = {
    cmaRp: 'RP형 CMA',
    cmaIssuedNote: '발행어음형 CMA',
    termDeposit: '정기예금',
    installmentSavings: '정기적금',
  };
  return labels[kind];
}

function cashContractKindLabel(kind: CashContractSummary['kind']): string {
  return kind === 'termDeposit' ? '정기예금' : '정기적금';
}

function cashContractStatusLabel(status: CashContractSummary['status']): string {
  const labels: Record<CashContractSummary['status'], string> = {
    active: '유지 중',
    matured: '만기',
    closedEarly: '중도해지',
    cancelled: '취소',
  };
  return labels[status];
}

function spreadText(spreadBp: number): string {
  const formatted = formatBasisPoints(spreadBp);
  return spreadBp > 0 ? `+${formatted}` : formatted;
}

function cashProductTermsText(product: CashProduct): string {
  if (product.kind === 'cmaRp' || product.kind === 'cmaIssuedNote') {
    const minimum = product.minimumInterestBalanceKrw;
    return `일 이자 최소 ${minimum === undefined ? '—' : formatWon(minimum)} · 예금자보호 비대상`;
  }

  const minimum = product.minimumContributionKrw;
  const maximum = product.maximumContributionKrw;
  const contribution = `${minimum === undefined ? '—' : formatWon(minimum)} ~ ${maximum === undefined ? '—' : formatWon(maximum)}`;
  const period =
    product.kind === 'termDeposit'
      ? `${product.termDays ?? '—'}일`
      : `${product.termMonths ?? '—'}개월 · ${product.installmentCount ?? '—'}회`;
  const earlyRate = product.earlyTerminationRateBp;
  return `${contribution} · ${period} · 중도해지 ${earlyRate === undefined ? '—' : formatBasisPoints(earlyRate)} · ${product.protectionEligible ? '예금자보호 대상' : '예금자보호 비대상'}`;
}

function cashContractInstallmentText(contract: CashContractSummary): string {
  if (contract.installmentAmountKrw === null) return '해당 없음';
  return `${formatWon(contract.installmentAmountKrw)} · 납입 ${contract.paidInstallmentCount}회 · 미납 ${contract.missedInstallmentCount}회`;
}

function cashContractExpectedText(contract: CashContractSummary): string {
  if (
    contract.expectedGrossInterestKrw === null ||
    contract.expectedIncomeTaxKrw === null ||
    contract.expectedLocalIncomeTaxKrw === null ||
    contract.expectedNetPayoutKrw === null
  ) {
    return '종료된 계약';
  }
  return `gross 이자 ${formatWon(contract.expectedGrossInterestKrw)} · 소득세 ${formatWon(contract.expectedIncomeTaxKrw)} · 지방소득세 ${formatWon(contract.expectedLocalIncomeTaxKrw)} · 세후 ${formatWon(contract.expectedNetPayoutKrw)}`;
}

function createAccountTable(body: HTMLTableSectionElement, capacity: number): AccountTable {
  const rows: AccountRow[] = [];
  for (let index = 0; index < capacity; index += 1) {
    const row = createAccountRow();
    rows.push(row);
    body.appendChild(row.element);
  }

  return {
    setAccounts(accounts) {
      for (let index = 0; index < rows.length; index += 1) {
        rows[index]?.setAccount(accounts[index]);
      }
    },
  };
}

function createAccountRow(): AccountRow {
  const id = el('td');
  const type = el('td');
  const status = el('td');
  const cash = el('td');
  const isDefault = el('td');
  const updateId = bindText(id);
  const updateType = bindText(type);
  const updateStatus = bindText(status);
  const updateCash = bindText(cash);
  const updateDefault = bindText(isDefault);
  const element = el('tr', {}, id, type, status, cash, isDefault);
  element.hidden = true;

  return {
    element,
    setAccount(account) {
      const hidden = account === undefined;
      if (element.hidden !== hidden) element.hidden = hidden;
      if (account === undefined) return;
      updateId(account.id);
      updateType(accountTypeLabel(account.type));
      updateStatus(accountStatusLabel(account.status));
      updateCash(formatWon(account.cashKrw));
      updateDefault(account.isDefault ? '예' : '아니요');
    },
  };
}

function createLedgerTable(body: HTMLTableSectionElement, capacity: number): LedgerTable {
  const rows: LedgerRow[] = [];
  for (let index = 0; index < capacity; index += 1) {
    const row = createLedgerRow();
    rows.push(row);
    body.appendChild(row.element);
  }

  return {
    setTransactions(transactions) {
      for (let index = 0; index < rows.length; index += 1) {
        rows[index]?.setTransaction(transactions[index]);
      }
    },
  };
}

function createLedgerRow(): LedgerRow {
  const id = el('td');
  const gameDay = el('td');
  const description = el('td');
  const sourceKind = el('td');
  const postings = el('td');
  const updateId = bindText(id);
  const updateGameDay = bindText(gameDay);
  const updateDescription = bindText(description);
  const updateSourceKind = bindText(sourceKind);
  const updatePostings = bindText(postings);
  const element = el('tr', {}, id, gameDay, description, sourceKind, postings);
  element.hidden = true;

  return {
    element,
    setTransaction(transaction) {
      const hidden = transaction === undefined;
      if (element.hidden !== hidden) element.hidden = hidden;
      if (transaction === undefined) return;
      updateId(transaction.id);
      updateGameDay(transaction.gameDay.toString());
      updateDescription(transaction.description);
      updateSourceKind(LEDGER_SOURCE_LABEL[transaction.sourceKind]);
      updatePostings(
        transaction.postings
          .map((posting) => {
            const account = posting.accountId === null ? '' : ` #${posting.accountId}`;
            return `${ledgerAccountLabel(posting.accountCode)}${account} ${formatWon(posting.amountKrw)}`;
          })
          .join(' / '),
      );
    },
  };
}

function ledgerAccountLabel(
  accountCode: LedgerTransaction['postings'][number]['accountCode'],
): string {
  const labels: Record<LedgerTransaction['postings'][number]['accountCode'], string> = {
    wallet: '지갑',
    accountCash: '계좌 현금',
    productPrincipal: '상품 원금',
    debtPrincipal: '부채 원금',
    openingEquity: '기초 자본',
    withholdingTaxLiability: '원천징수세',
    interestIncome: '이자수익',
    feeExpense: '수수료',
    distributionIncome: '분배금수익',
    realizedGainLoss: '실현손익',
    taxSettlement: '세금 정산',
    careerDevelopmentExpense: '커리어 개발비',
    salaryIncome: '급여수익',
    employeeNationalPensionExpense: '근로자 국민연금',
    employeeHealthInsuranceExpense: '근로자 건강보험',
    employeeLongTermCareExpense: '근로자 장기요양보험',
    employeeEmploymentInsuranceExpense: '근로자 고용보험',
    employmentIncomeTaxWithholding: '근로소득세 원천징수',
    employmentLocalIncomeTaxWithholding: '근로 지방소득세 원천징수',
    otherIncomeReward: '기타소득 채용보상',
    otherIncomeTaxWithholding: '기타소득세 원천징수',
    otherLocalIncomeTaxWithholding: '기타 지방소득세 원천징수',
    pensionTaxExcludedContribution: '연금 세액공제 미확정 납입',
    pensionCreditedContribution: '연금 세액공제 확정 납입',
    militaryPayIncome: '군 급여수익',
    militarySavingsPrincipal: '장병적금 원금',
    militarySavingsBankInterest: '장병적금 은행이자',
    militarySavingsGovernmentMatchIncome: '장병적금 정부지원수익',
    livingCostExpense: '생활비',
    essentialArrearLiability: '필수 생활비 미납',
    loanPrincipalLiability: '대출 원금 의무',
    loanInterestExpense: '대출 이자비용',
    loanInterestLiability: '미납 대출 이자',
    loanFeeExpense: '대출 비용',
    taxObligationLiability: '미납 세금 의무',
    leaseDepositAsset: '임대차 보증금 자산',
    movingExpense: '이사 비용',
    leaseRentExpense: '월세 비용',
    leaseArrearLiability: '월세 연체 의무',
    propertyAsset: '주택 자산',
    acquisitionIncidentalExpense: '주택 취득 부대비용',
    propertyDispositionExpense: '주택 처분 비용',
    propertyTaxExpense: '부동산 세금 비용',
    welfareBenefitIncome: '복지 급여수익',
    lifeEventExpense: '생애 사건 비용',
    insurancePremiumExpense: '보험료 비용',
    insuranceClaimRecovery: '보험금 회수',
  };
  return labels[accountCode];
}

function historyHeader(label: string): HTMLTableCellElement {
  return el('th', { attrs: { scope: 'col' } }, label);
}

function createHistoryTable(body: HTMLTableSectionElement, capacity: number): HistoryTable {
  const rows: HistoryRow[] = [];
  for (let index = 0; index < capacity; index += 1) {
    const row = createHistoryRow();
    rows.push(row);
    body.appendChild(row.element);
  }
  let visibleCount = 0;

  return {
    setPoints(points) {
      const updateCount = Math.max(visibleCount, points.length);
      for (let index = 0; index < updateCount; index += 1) {
        rows[index]?.setPoint(points[index]);
      }
      visibleCount = points.length;
    },
  };
}

function createHistoryRow(): HistoryRow {
  const date = el('td');
  const gameDay = el('td');
  const open = el('td');
  const close = el('td');
  const dailyReturn = el('td');
  const regime = el('td');
  const updateDate = bindText(date);
  const updateGameDay = bindText(gameDay);
  const updateOpen = bindText(open);
  const updateClose = bindText(close);
  const updateDailyReturn = bindText(dailyReturn);
  const updateRegime = bindText(regime);
  const element = el('tr', {}, date, gameDay, open, close, dailyReturn, regime);
  element.hidden = true;

  return {
    element,
    setPoint(point) {
      const hidden = point === undefined;
      if (element.hidden !== hidden) element.hidden = hidden;
      if (point === undefined) return;
      updateDate(point.date);
      updateGameDay(point.gameDay.toString());
      updateOpen(point.open ? '개장' : '휴장');
      updateClose(formatWon(point.llxCloseKrw ?? point.closeKrw));
      updateDailyReturn(formatReturnPpm(point.llxDailyReturnPpm ?? point.dailyReturnPpm));
      updateRegime(MARKET_REGIME_LABEL[point.regime]);
    },
  };
}

function toCloseChartPoint(point: MarketHistoryPoint): CloseChartPoint {
  return {
    timestampSeconds: Date.parse(`${point.date}T00:00:00Z`) / 1000,
    value: point.llxCloseKrw ?? point.closeKrw,
  };
}

async function logout(auth: AuthApi, toasts: ToastQueue): Promise<void> {
  try {
    await auth.logout();
  } catch {
    toasts.show('로그아웃하지 못했습니다. 다시 시도해 주세요.', { tone: 'error' });
    return;
  }
  // A full reload is the reliable way to pick up the cleared session cookie
  globalThis.location.assign('/');
}

async function deleteAccount(
  auth: AuthApi,
  toasts: ToastQueue,
  button: HTMLButtonElement,
): Promise<void> {
  if (
    !globalThis.confirm('계정과 모든 게임 기록·동의·피드백이 영구 삭제됩니다. 계속하시겠습니까?')
  ) {
    return;
  }
  if (!globalThis.confirm('삭제 후에는 복구할 수 없습니다. 정말 계정을 삭제하시겠습니까?')) {
    return;
  }

  button.disabled = true;
  try {
    await auth.deleteAccount();
  } catch {
    button.disabled = false;
    toasts.show('계정을 삭제하지 못했습니다. 다시 시도해 주세요.', { tone: 'error' });
    return;
  }
  globalThis.location.assign('/');
}

async function advance(
  store: Store<AppState>,
  snapshots: GameStateWriter,
  api: GameApi,
  toasts: ToastQueue,
  retries: AdvanceRetryPolicy,
  days: number,
): Promise<void> {
  const game = store.getState().game;
  const snapshot = game.snapshot;
  const characterName = snapshot?.characterName;
  if (
    snapshot === undefined ||
    game.advancing ||
    game.ordering ||
    characterName === undefined ||
    characterName === null ||
    game.snapshot?.autoSpeed !== null
  ) {
    return;
  }
  const request = retries.select(snapshot, days);
  store.set(paths.gameAdvancing, true);
  try {
    const response = await api.advance(request);
    retries.clear(request);
    snapshots.applyIfAhead(response.snapshot);
  } catch (error) {
    if (error instanceof GameCommandError) {
      if (error.code === 'progressBusy') retries.retain(request);
      else retries.clear(request);
      toasts.show(error.message, { tone: 'error' });
    } else {
      retries.retain(request);
      toasts.show('진행 결과를 확인하지 못했습니다. 같은 기간으로 다시 시도해 주세요.', {
        tone: 'error',
      });
    }
  } finally {
    store.set(paths.gameAdvancing, false);
  }
}

async function setClock(
  store: Store<AppState>,
  snapshots: GameStateWriter,
  api: GameApi,
  toasts: ToastQueue,
  speed: GameSpeed | null,
): Promise<void> {
  const state = store.getState();
  const characterName = state.game.snapshot?.characterName;
  if (
    state.game.advancing ||
    state.game.ordering ||
    characterName === undefined ||
    characterName === null
  ) {
    return;
  }
  if (speed !== null && state.connection.status !== 'open') return;
  store.set(paths.gameAdvancing, true);
  try {
    const snapshot = await api.setClock(speed);
    // Clock changes keep the same game day. While connected, apply them only from the
    // ordered SSE stream so a delayed HTTP response cannot undo a newer pause or speed.
    if (speed === null && store.getState().connection.status !== 'open') {
      snapshots.apply(snapshot);
    }
  } catch {
    toasts.show('자동 진행 상태를 바꾸지 못했습니다. 다시 시도해 주세요.', { tone: 'error' });
  } finally {
    store.set(paths.gameAdvancing, false);
  }
}
