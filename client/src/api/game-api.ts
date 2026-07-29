import type { Unsubscribe } from '../lib/core/index.js';
import { type HttpClient, HttpError } from '../lib/http/index.js';
import type { SseClient, SseMessage } from '../lib/sse/index.js';
import {
  type AdvanceRequest,
  AdvanceRequestSchema,
  type AdvanceResponse,
  AdvanceResponseSchema,
  type BondOrderRequest,
  BondOrderRequestSchema,
  type BondOrderResponse,
  BondOrderResponseSchema,
  type BondProductCatalog,
  BondProductCatalogSchema,
  type CashProductCatalog,
  CashProductCatalogSchema,
  type CharacterStartRequest,
  CharacterStartRequestSchema,
  type CharacterStartResponse,
  CharacterStartResponseSchema,
  type CmaAccountCloseRequest,
  CmaAccountCloseRequestSchema,
  type CmaAccountCloseResponse,
  CmaAccountCloseResponseSchema,
  type CmaAccountOpenRequest,
  CmaAccountOpenRequestSchema,
  type CmaAccountOpenResponse,
  CmaAccountOpenResponseSchema,
  type DepositCloseRequest,
  DepositCloseRequestSchema,
  type DepositCloseResponse,
  DepositCloseResponseSchema,
  type DepositOpenRequest,
  DepositOpenRequestSchema,
  type DepositOpenResponse,
  DepositOpenResponseSchema,
  type EquityMarket,
  EquityMarketSchema,
  EquitySearchLimitSchema,
  type EquitySearchResult,
  EquitySearchResultSchema,
  EquitySearchTextSchema,
  type FinanceAccountsResponse,
  FinanceAccountsResponseSchema,
  type FinanceFailureCode,
  FinanceFailureSchema,
  type FinanceTransferRequest,
  FinanceTransferRequestSchema,
  type FinanceTransferResponse,
  FinanceTransferResponseSchema,
  type FinancialIncomeYear,
  FinancialIncomeYearSchema,
  type GameCommandFailureCode,
  GameCommandFailureSchema,
  type GameSnapshot,
  GameSnapshotSchema,
  type GameSpeed,
  type GoldAccountOpenRequest,
  GoldAccountOpenRequestSchema,
  type GoldAccountOpenResponse,
  GoldAccountOpenResponseSchema,
  type GoldOrderRequest,
  GoldOrderRequestSchema,
  type GoldOrderResponse,
  GoldOrderResponseSchema,
  type GoldProductCatalog,
  GoldProductCatalogSchema,
  type GoldWithdrawalRequest,
  GoldWithdrawalRequestSchema,
  type GoldWithdrawalResponse,
  GoldWithdrawalResponseSchema,
  type Health,
  HealthSchema,
  type IsaAccountCloseRequest,
  IsaAccountCloseRequestSchema,
  type IsaAccountCloseResponse,
  IsaAccountCloseResponseSchema,
  type LeagueRankingPage,
  LeagueRankingPageSchema,
  type LedgerPage,
  LedgerPageSchema,
  type MarketHistory,
  MarketHistoryDaysSchema,
  MarketHistorySchema,
  type OfflineProgress,
  type OfflineProgressFailureCode,
  OfflineProgressFailureSchema,
  OfflineProgressSchema,
  type OfflineProgressUpdateRequest,
  OfflineProgressUpdateRequestSchema,
  type PensionStartRequest,
  PensionStartRequestSchema,
  type PensionStartResponse,
  PensionStartResponseSchema,
  type PensionWithdrawalRequest,
  PensionWithdrawalRequestSchema,
  type PensionWithdrawalResponse,
  PensionWithdrawalResponseSchema,
  type PointBudgetEvaluation,
  PointBudgetEvaluationSchema,
  type PointBudgetPreviewRequest,
  PointBudgetPreviewRequestSchema,
  type PortfolioOrderFailureCode,
  PortfolioOrderFailureSchema,
  type PortfolioOrderRequest,
  PortfolioOrderRequestSchema,
  type PortfolioOrderResponse,
  PortfolioOrderResponseSchema,
  type Preset,
  PresetListSchema,
  type PublicSaveDetail,
  PublicSaveDetailSchema,
  type PublicSaveRankingPage,
  PublicSaveRankingPageSchema,
  type PublicSaveRankingQuery,
  PublicSaveRankingQuerySchema,
  ResourceIdSchema,
  type RunFinalization,
  RunFinalizationSchema,
  type RunOptions,
  RunOptionsSchema,
  type RunRequestFailureCode,
  RunRequestFailureSchema,
  type RunStartRequest,
  RunStartRequestSchema,
  type RunStartResponse,
  RunStartResponseSchema,
  type SeasonLeagues,
  SeasonLeaguesSchema,
  type TaxAccountOpenRequest,
  TaxAccountOpenRequestSchema,
  type TaxAccountOpenResponse,
  TaxAccountOpenResponseSchema,
  TaxYearSchema,
  ValidationFailureSchema,
} from './contracts.js';
import { asDecoder } from './zod-adapters.js';

/**
 * The domain API. Screens use this interface rather than HttpClient or SseClient
 * directly, so changing transport leaves them untouched.
 */
/**
 * Raised when the server rejects a contradictory combination (§3.5). Carries per-field
 * messages ready for the form, so no screen needs to know an HTTP status code.
 */
export class CharacterRejectedError extends Error {
  constructor(readonly fieldErrors: Readonly<Record<string, string>>) {
    super('시작 조건이 서로 맞지 않습니다');
    this.name = 'CharacterRejectedError';
  }
}

/** A validated durable game-command rejection. */
export class GameCommandError extends Error {
  constructor(
    readonly code: GameCommandFailureCode,
    message: string,
  ) {
    super(message);
    this.name = 'GameCommandError';
  }
}

/** A validated run-catalog or point-preview rejection. */
export class RunRequestError extends Error {
  constructor(readonly code: RunRequestFailureCode) {
    super(
      code === 'versionNotFound'
        ? '선택한 포인트 예산을 사용할 수 없습니다'
        : '실행 설정이 올바르지 않습니다',
    );
    this.name = 'RunRequestError';
  }
}

/** A validated opt-in rejection for the current run's offline policy. */
export class OfflineProgressError extends Error {
  constructor(
    readonly code: OfflineProgressFailureCode,
    message: string,
  ) {
    super(message);
    this.name = 'OfflineProgressError';
  }
}

/** A validated portfolio rejection, independent of the HTTP status chosen by the server. */
export class PortfolioOrderError extends Error {
  constructor(
    readonly code: PortfolioOrderFailureCode,
    message: string,
  ) {
    super(message);
    this.name = 'PortfolioOrderError';
  }
}

/** A validated finance-command rejection, independent of its transport status. */
export class FinanceCommandError extends Error {
  constructor(
    readonly code: FinanceFailureCode,
    message: string,
  ) {
    super(message);
    this.name = 'FinanceCommandError';
  }
}

export interface GameApi {
  health(): Promise<Health>;
  /** Starting presets (§3.3). */
  listPresets(): Promise<readonly Preset[]>;
  /**
   * Creates a character and starts the game.
   * A failed combination check throws {@link CharacterRejectedError}.
   */
  createCharacter(request: CharacterStartRequest): Promise<CharacterStartResponse>;
  /** Lists immutable run-start catalogs and the currently published season. */
  listRunOptions(signal?: AbortSignal): Promise<RunOptions>;
  /** Lists one public season and its immutable league definitions. */
  listSeasonLeagues(seasonId: string, signal?: AbortSignal): Promise<SeasonLeagues>;
  /** Lists completed finalizations in one public league. */
  listLeagueRankings(
    leagueId: string,
    cursor?: string,
    limit?: number,
    signal?: AbortSignal,
  ): Promise<LeagueRankingPage>;
  /** Lists live public saves, including runs that have not reached finalization. */
  listPublicSaveRankings(
    query: PublicSaveRankingQuery,
    signal?: AbortSignal,
  ): Promise<PublicSaveRankingPage>;
  /** Reads one public save's allowlisted aggregate detail. */
  getPublicSaveDetail(saveUid: string, signal?: AbortSignal): Promise<PublicSaveDetail>;
  /** Reads the authenticated account's immutable ranked-run finalization. */
  getRunFinalization(runRevision: number, signal?: AbortSignal): Promise<RunFinalization>;
  /** Reads the current run's pinned offline-progress policy and live worker state. */
  getOfflineProgress(signal?: AbortSignal): Promise<OfflineProgress>;
  /** Changes opt-in with an exact revision guard. */
  setOfflineProgress(
    request: OfflineProgressUpdateRequest,
    signal?: AbortSignal,
  ): Promise<OfflineProgress>;
  /** Evaluates one canonical point selection on the server. */
  previewPointBudget(
    request: PointBudgetPreviewRequest,
    signal?: AbortSignal,
  ): Promise<PointBudgetEvaluation>;
  /** Starts a new version-pinned run. */
  createRun(request: RunStartRequest): Promise<RunStartResponse>;
  getSnapshot(): Promise<GameSnapshot>;
  /** Advances the game day. The result also arrives over SSE. */
  advance(request: AdvanceRequest): Promise<AdvanceResponse>;
  /** Starts, changes, or pauses the server-owned automatic clock. */
  setClock(speed: GameSpeed | null): Promise<GameSnapshot>;
  /** Places one all-or-nothing LLX order tied to the snapshot the player saw. */
  placePortfolioOrder(request: PortfolioOrderRequest): Promise<PortfolioOrderResponse>;
  /** Lists the authenticated save's current-run accounts and pinned policy set. */
  listFinanceAccounts(): Promise<FinanceAccountsResponse>;
  /** Lists published CMA and deposit product versions. */
  listCashProducts(): Promise<CashProductCatalog>;
  /** Lists the current world's published bond products and live series. */
  listBonds(signal?: AbortSignal): Promise<BondProductCatalog>;
  /** Places one all-or-nothing government-bond order. */
  placeBondOrder(request: BondOrderRequest): Promise<BondOrderResponse>;
  /** Lists the current world's published KRX gold product. */
  listGoldProducts(signal?: AbortSignal): Promise<GoldProductCatalog>;
  /** Opens the current run's single KRX gold account. */
  openGoldAccount(request: GoldAccountOpenRequest): Promise<GoldAccountOpenResponse>;
  /** Places one all-or-nothing KRX gold order in whole grams. */
  placeGoldOrder(request: GoldOrderRequest): Promise<GoldOrderResponse>;
  /** Converts account gold into 100g or 1000g physical bars. */
  withdrawGold(request: GoldWithdrawalRequest): Promise<GoldWithdrawalResponse>;
  /** Opens a CMA account from one immutable product version. */
  openCmaAccount(request: CmaAccountOpenRequest): Promise<CmaAccountOpenResponse>;
  /** Closes an empty, non-default CMA account. */
  closeCmaAccount(
    accountId: string,
    request: CmaAccountCloseRequest,
  ): Promise<CmaAccountCloseResponse>;
  /** Opens one current-run ISA, pension-savings, or IRP account. */
  openTaxAccount(request: TaxAccountOpenRequest): Promise<TaxAccountOpenResponse>;
  /** Closes an empty current-run ISA and settles its tax into the wallet. */
  closeIsaAccount(
    accountId: string,
    request: IsaAccountCloseRequest,
  ): Promise<IsaAccountCloseResponse>;
  /** Starts pension receipt under a fixed payment-period election. */
  startPension(accountId: string, request: PensionStartRequest): Promise<PensionStartResponse>;
  /** Withdraws a gross amount through the pension-specific tax path. */
  withdrawPension(
    accountId: string,
    request: PensionWithdrawalRequest,
  ): Promise<PensionWithdrawalResponse>;
  /** Opens a term deposit or installment savings contract. */
  openDeposit(request: DepositOpenRequest): Promise<DepositOpenResponse>;
  /** Closes an active cash contract at its early-termination rate. */
  closeDeposit(contractId: string, request: DepositCloseRequest): Promise<DepositCloseResponse>;
  /** Reads one calendar year's financial-income and withholding totals. */
  getFinanceTaxYear(year: number): Promise<FinancialIncomeYear>;
  /** Moves integer KRW between the settlement wallet and one current-run account. */
  transferFinance(request: FinanceTransferRequest): Promise<FinanceTransferResponse>;
  /** Reads the current run's append-only ledger newest first. */
  getFinanceLedger(before?: string, limit?: number, signal?: AbortSignal): Promise<LedgerPage>;
  /** Reads LLX history no later than the authenticated save's current game day. */
  getMarketHistory(days: number, signal?: AbortSignal): Promise<MarketHistory>;
  /** Searches the last atomically published KRX catalog without calling a provider live. */
  searchEquities(
    query: string,
    market?: EquityMarket,
    limit?: number,
    signal?: AbortSignal,
  ): Promise<EquitySearchResult>;
  /** Subscribes to ticks, validating each payload against the contract first. */
  onTick(handler: (snapshot: GameSnapshot) => void): Unsubscribe;
  connectStream(): void;
  disconnectStream(): void;
}

export interface GameApiDeps {
  readonly http: HttpClient;
  readonly stream: SseClient;
  /** What to do with a payload that breaks the contract. Logged and dropped by default. */
  readonly onInvalidTick?: (error: unknown, raw: SseMessage) => void;
}

const snapshotDecoder = asDecoder(GameSnapshotSchema);
const healthDecoder = asDecoder(HealthSchema);
const presetListDecoder = asDecoder(PresetListSchema);
const marketHistoryDecoder = asDecoder(MarketHistorySchema);
const equitySearchResultDecoder = asDecoder(EquitySearchResultSchema);
const financeAccountsDecoder = asDecoder(FinanceAccountsResponseSchema);
const cashProductCatalogDecoder = asDecoder(CashProductCatalogSchema);
const bondProductCatalogDecoder = asDecoder(BondProductCatalogSchema);
const goldProductCatalogDecoder = asDecoder(GoldProductCatalogSchema);
const runOptionsDecoder = asDecoder(RunOptionsSchema);
const seasonLeaguesDecoder = asDecoder(SeasonLeaguesSchema);
const leagueRankingPageDecoder = asDecoder(LeagueRankingPageSchema);
const publicSaveRankingPageDecoder = asDecoder(PublicSaveRankingPageSchema);
const publicSaveDetailDecoder = asDecoder(PublicSaveDetailSchema);
const runFinalizationDecoder = asDecoder(RunFinalizationSchema);
const offlineProgressDecoder = asDecoder(OfflineProgressSchema);

function publicSaveRankingSearch(query: PublicSaveRankingQuery): string {
  const entries: readonly (readonly [string, number | string | undefined])[] = [
    ['page', query.page],
    ['limit', query.limit],
    ['status', query.status],
    ['gameDayFrom', query.gameDayFrom],
    ['gameDayTo', query.gameDayTo],
    ['ageFrom', query.ageFrom],
    ['ageTo', query.ageTo],
  ];
  const params = new URLSearchParams();
  for (const [key, value] of entries) {
    if (value !== undefined) params.set(key, String(value));
  }
  return params.size === 0 ? '' : `?${params.toString()}`;
}

/** Turns a 422 body into a field-to-message map, or gives up if the shape is unfamiliar. */
function toFieldErrors(error: unknown): Record<string, string> | undefined {
  if (!(error instanceof HttpError) || error.status !== 422) return undefined;
  const parsed = ValidationFailureSchema.safeParse(error.body);
  if (!parsed.success) return undefined;
  const fieldErrors: Record<string, string> = {};
  for (const item of parsed.data.errors) {
    if (fieldErrors[item.field] === undefined) fieldErrors[item.field] = item.message;
  }
  return fieldErrors;
}

function toGameCommandError(error: unknown): GameCommandError | undefined {
  if (!(error instanceof HttpError)) return undefined;
  const parsed = GameCommandFailureSchema.safeParse(error.body);
  if (!parsed.success) return undefined;
  return new GameCommandError(parsed.data.code, parsed.data.message);
}

function toRunRequestError(error: unknown): RunRequestError | undefined {
  if (!(error instanceof HttpError)) return undefined;
  const parsed = RunRequestFailureSchema.safeParse(error.body);
  return parsed.success ? new RunRequestError(parsed.data.code) : undefined;
}

function toOfflineProgressError(error: unknown): OfflineProgressError | undefined {
  if (!(error instanceof HttpError)) return undefined;
  const parsed = OfflineProgressFailureSchema.safeParse(error.body);
  return parsed.success
    ? new OfflineProgressError(parsed.data.code, parsed.data.message)
    : undefined;
}

function toPortfolioOrderError(error: unknown): PortfolioOrderError | undefined {
  if (!(error instanceof HttpError)) return undefined;
  const parsed = PortfolioOrderFailureSchema.safeParse(error.body);
  if (!parsed.success) return undefined;
  return new PortfolioOrderError(parsed.data.code, parsed.data.message);
}

function toFinanceCommandError(error: unknown): FinanceCommandError | undefined {
  if (!(error instanceof HttpError)) return undefined;
  const parsed = FinanceFailureSchema.safeParse(error.body);
  if (!parsed.success) return undefined;
  return new FinanceCommandError(parsed.data.code, parsed.data.message);
}

export function createGameApi(deps: GameApiDeps): GameApi {
  const { http, stream } = deps;

  return {
    health: () => http.get('/api/health', healthDecoder),
    listPresets: () => http.get('/api/presets', presetListDecoder),

    async createCharacter(request) {
      const body = CharacterStartRequestSchema.parse(request);
      const decoder = asDecoder(
        CharacterStartResponseSchema.superRefine((response, context) => {
          const committed = response.start.committedCursor;
          if (
            response.start.commandId !== body.commandId ||
            committed.runRevision !== body.expectedRunRevision + 1 ||
            committed.stateRevision !== 0 ||
            committed.gameDay !== 0
          ) {
            context.addIssue({
              code: 'custom',
              path: ['start'],
              message: 'start result does not match the submitted command',
            });
          }
        }),
      );
      try {
        return await http.post('/api/characters', body, decoder);
      } catch (error) {
        const fieldErrors = toFieldErrors(error);
        if (fieldErrors !== undefined) throw new CharacterRejectedError(fieldErrors);
        const domainError = toGameCommandError(error);
        if (domainError !== undefined) throw domainError;
        throw error;
      }
    },

    listRunOptions: (signal) =>
      http.get(
        '/api/run-options',
        runOptionsDecoder,
        signal === undefined ? undefined : { signal },
      ),

    listSeasonLeagues: (seasonId, signal) => {
      const id = ResourceIdSchema.parse(seasonId);
      return http.get(
        `/api/seasons/${id}/leagues`,
        seasonLeaguesDecoder,
        signal === undefined ? undefined : { signal },
      );
    },

    listLeagueRankings: (leagueId, cursor, limit, signal) => {
      const id = ResourceIdSchema.parse(leagueId);
      const query = new URLSearchParams();
      if (cursor !== undefined) query.set('cursor', cursor);
      if (limit !== undefined) query.set('limit', String(limit));
      const suffix = query.size === 0 ? '' : `?${query.toString()}`;
      return http.get(
        `/api/leagues/${id}/rankings${suffix}`,
        leagueRankingPageDecoder,
        signal === undefined ? undefined : { signal },
      );
    },

    listPublicSaveRankings: (request, signal) => {
      const query = PublicSaveRankingQuerySchema.parse(request);
      return http.get(
        `/api/rankings/saves${publicSaveRankingSearch(query)}`,
        publicSaveRankingPageDecoder,
        signal === undefined ? undefined : { signal },
      );
    },

    getPublicSaveDetail: (saveUid, signal) => {
      const uid = PublicSaveDetailSchema.shape.saveUid.parse(saveUid);
      return http.get(
        `/api/rankings/saves/${uid}`,
        publicSaveDetailDecoder,
        signal === undefined ? undefined : { signal },
      );
    },

    getRunFinalization: (runRevision, signal) => {
      if (!Number.isSafeInteger(runRevision) || runRevision < 0) {
        return Promise.reject(new Error('run revision is invalid'));
      }
      return http.get(
        `/api/runs/${String(runRevision)}/finalization`,
        runFinalizationDecoder,
        signal === undefined ? undefined : { signal },
      );
    },

    getOfflineProgress: (signal) =>
      http.get(
        '/api/offline-progress/status',
        offlineProgressDecoder,
        signal === undefined ? undefined : { signal },
      ),

    async setOfflineProgress(request, signal) {
      const body = OfflineProgressUpdateRequestSchema.parse(request);
      try {
        return await http.put(
          '/api/offline-progress',
          body,
          offlineProgressDecoder,
          signal === undefined ? undefined : { signal },
        );
      } catch (error) {
        const domainError = toOfflineProgressError(error);
        if (domainError !== undefined) throw domainError;
        throw error;
      }
    },

    async previewPointBudget(request, signal) {
      const body = PointBudgetPreviewRequestSchema.parse(request);
      const decoder = asDecoder(
        PointBudgetEvaluationSchema.superRefine((response, context) => {
          if (response.pointBudgetVersionId !== body.pointBudgetVersionId) {
            context.addIssue({
              code: 'custom',
              path: ['pointBudgetVersionId'],
              message: 'point preview does not match the submitted budget version',
            });
          }
        }),
      );
      try {
        return await http.post(
          '/api/runs/point-preview',
          body,
          decoder,
          signal === undefined ? undefined : { signal },
        );
      } catch (error) {
        const domainError = toRunRequestError(error);
        if (domainError !== undefined) throw domainError;
        throw error;
      }
    },

    async createRun(request) {
      const body = RunStartRequestSchema.parse(request);
      const decoder = asDecoder(
        RunStartResponseSchema.superRefine((response, context) => {
          const committed = response.start.committedCursor;
          if (
            response.mode !== body.mode ||
            response.start.commandId !== body.commandId ||
            committed.runRevision !== body.expectedRunRevision + 1 ||
            committed.stateRevision !== 0 ||
            committed.gameDay !== 0 ||
            response.snapshot.runRevision !== committed.runRevision ||
            response.snapshot.stateRevision !== committed.stateRevision ||
            response.snapshot.gameDay !== committed.gameDay
          ) {
            context.addIssue({
              code: 'custom',
              path: ['start'],
              message: 'run start result does not match the submitted command',
            });
          }
        }),
      );
      try {
        return await http.post('/api/runs', body, decoder);
      } catch (error) {
        const fieldErrors = toFieldErrors(error);
        if (fieldErrors !== undefined) throw new CharacterRejectedError(fieldErrors);
        const domainError = toGameCommandError(error);
        if (domainError !== undefined) throw domainError;
        throw error;
      }
    },

    getSnapshot: () => http.get('/api/state', snapshotDecoder),
    async advance(request) {
      const body = AdvanceRequestSchema.parse(request);
      const decoder = asDecoder(
        AdvanceResponseSchema.superRefine((response, context) => {
          const result = response.advance;
          if (
            result.commandId !== body.commandId ||
            result.requestedDays !== body.days ||
            result.committedDays + result.truncatedDays !== body.days ||
            result.initialCursor.runRevision !== body.expectedRunRevision ||
            result.initialCursor.stateRevision !== body.expectedStateRevision ||
            result.initialCursor.gameDay !== body.expectedGameDay ||
            result.committedCursor.runRevision !== body.expectedRunRevision ||
            result.committedCursor.stateRevision !==
              body.expectedStateRevision + result.committedDays ||
            result.committedCursor.gameDay !== body.expectedGameDay + result.committedDays
          ) {
            context.addIssue({
              code: 'custom',
              path: ['advance'],
              message: 'advance result does not match the submitted command',
            });
          }
        }),
      );
      try {
        return await http.post('/api/advance', body, decoder);
      } catch (error) {
        const domainError = toGameCommandError(error);
        if (domainError === undefined) throw error;
        throw domainError;
      }
    },
    setClock: (speed) => http.post('/api/clock', { speed }, snapshotDecoder),

    async placePortfolioOrder(request) {
      const body = PortfolioOrderRequestSchema.parse(request);
      const decoder = asDecoder(
        PortfolioOrderResponseSchema.superRefine((response, context) => {
          const execution = response.execution;
          if (
            execution.orderId !== body.orderId ||
            execution.accountId !== body.accountId ||
            execution.side !== body.side ||
            execution.symbol !== body.symbol ||
            execution.quantity !== body.quantity
          ) {
            context.addIssue({
              code: 'custom',
              path: ['execution'],
              message: 'execution does not match the submitted order',
            });
          }
        }),
      );
      try {
        return await http.post('/api/portfolio/orders', body, decoder);
      } catch (error) {
        const domainError = toPortfolioOrderError(error);
        if (domainError === undefined) throw error;
        throw domainError;
      }
    },

    listFinanceAccounts: () => http.get('/api/finance/accounts', financeAccountsDecoder),
    listCashProducts: () => http.get('/api/finance/cash-products', cashProductCatalogDecoder),

    listBonds: (signal) =>
      http.get(
        '/api/finance/bonds',
        bondProductCatalogDecoder,
        signal === undefined ? undefined : { signal },
      ),

    async placeBondOrder(request) {
      const body = BondOrderRequestSchema.parse(request);
      const decoder = asDecoder(
        BondOrderResponseSchema.superRefine((response, context) => {
          const order = response.bondOrder;
          if (
            order.commandId !== body.commandId ||
            order.accountId !== body.accountId ||
            order.seriesId !== body.seriesId ||
            order.side !== body.side ||
            order.bondUnits !== body.bondUnits
          ) {
            context.addIssue({
              code: 'custom',
              path: ['bondOrder'],
              message: 'bond execution does not match the submitted order',
            });
          }
        }),
      );
      try {
        return await http.post('/api/finance/bonds/orders', body, decoder);
      } catch (error) {
        const domainError = toFinanceCommandError(error);
        if (domainError === undefined) throw error;
        throw domainError;
      }
    },

    listGoldProducts: (signal) =>
      http.get(
        '/api/finance/gold-products',
        goldProductCatalogDecoder,
        signal === undefined ? undefined : { signal },
      ),

    async openGoldAccount(request) {
      const body = GoldAccountOpenRequestSchema.parse(request);
      const decoder = asDecoder(
        GoldAccountOpenResponseSchema.superRefine((response, context) => {
          const account = response.account;
          if (
            account.commandId !== body.commandId ||
            account.type !== body.type ||
            account.productVersionId !== body.productVersionId
          ) {
            context.addIssue({
              code: 'custom',
              path: ['account'],
              message: 'opened gold account does not match the submitted command',
            });
          }
        }),
      );
      try {
        return await http.post('/api/finance/accounts', body, decoder);
      } catch (error) {
        const domainError = toFinanceCommandError(error);
        if (domainError === undefined) throw error;
        throw domainError;
      }
    },

    async placeGoldOrder(request) {
      const body = GoldOrderRequestSchema.parse(request);
      const decoder = asDecoder(
        GoldOrderResponseSchema.superRefine((response, context) => {
          const order = response.goldOrder;
          if (
            order.commandId !== body.commandId ||
            order.accountId !== body.accountId ||
            order.side !== body.side ||
            order.quantityGram !== body.quantityGram
          ) {
            context.addIssue({
              code: 'custom',
              path: ['goldOrder'],
              message: 'gold execution does not match the submitted order',
            });
          }
        }),
      );
      try {
        return await http.post('/api/finance/gold/orders', body, decoder);
      } catch (error) {
        const domainError = toFinanceCommandError(error);
        if (domainError === undefined) throw error;
        throw domainError;
      }
    },

    async withdrawGold(request) {
      const body = GoldWithdrawalRequestSchema.parse(request);
      const decoder = asDecoder(
        GoldWithdrawalResponseSchema.superRefine((response, context) => {
          const withdrawal = response.goldWithdrawal;
          if (
            withdrawal.commandId !== body.commandId ||
            withdrawal.accountId !== body.accountId ||
            withdrawal.barSizeGram !== body.barSizeGram ||
            withdrawal.barCount !== body.barCount
          ) {
            context.addIssue({
              code: 'custom',
              path: ['goldWithdrawal'],
              message: 'gold withdrawal does not match the submitted command',
            });
          }
        }),
      );
      try {
        return await http.post('/api/finance/gold/withdrawals', body, decoder);
      } catch (error) {
        const domainError = toFinanceCommandError(error);
        if (domainError === undefined) throw error;
        throw domainError;
      }
    },

    async openCmaAccount(request) {
      const body = CmaAccountOpenRequestSchema.parse(request);
      const decoder = asDecoder(
        CmaAccountOpenResponseSchema.superRefine((response, context) => {
          if (
            response.account.commandId !== body.commandId ||
            response.account.productVersionId !== body.productVersionId
          ) {
            context.addIssue({
              code: 'custom',
              path: ['account'],
              message: 'opened CMA account does not match the submitted command',
            });
          }
        }),
      );
      try {
        return await http.post('/api/finance/accounts', body, decoder);
      } catch (error) {
        const domainError = toFinanceCommandError(error);
        if (domainError === undefined) throw error;
        throw domainError;
      }
    },

    async closeCmaAccount(accountId, request) {
      const validAccountId = ResourceIdSchema.parse(accountId);
      const body = CmaAccountCloseRequestSchema.parse(request);
      const decoder = asDecoder(
        CmaAccountCloseResponseSchema.superRefine((response, context) => {
          if (
            response.accountClose.commandId !== body.commandId ||
            response.accountClose.accountId !== validAccountId
          ) {
            context.addIssue({
              code: 'custom',
              path: ['accountClose'],
              message: 'closed CMA account does not match the submitted command',
            });
          }
        }),
      );
      try {
        return await http.post(`/api/finance/accounts/${validAccountId}/close`, body, decoder);
      } catch (error) {
        const domainError = toFinanceCommandError(error);
        if (domainError === undefined) throw error;
        throw domainError;
      }
    },

    async openTaxAccount(request) {
      const body = TaxAccountOpenRequestSchema.parse(request);
      const decoder = asDecoder(
        TaxAccountOpenResponseSchema.superRefine((response, context) => {
          if (
            response.account.commandId !== body.commandId ||
            response.account.type !== body.type
          ) {
            context.addIssue({
              code: 'custom',
              path: ['account'],
              message: 'opened tax account does not match the submitted command',
            });
          }
        }),
      );
      try {
        return await http.post('/api/finance/accounts', body, decoder);
      } catch (error) {
        const domainError = toFinanceCommandError(error);
        if (domainError === undefined) throw error;
        throw domainError;
      }
    },

    async closeIsaAccount(accountId, request) {
      const validAccountId = ResourceIdSchema.parse(accountId);
      const body = IsaAccountCloseRequestSchema.parse(request);
      const decoder = asDecoder(
        IsaAccountCloseResponseSchema.superRefine((response, context) => {
          if (
            response.isaClose.commandId !== body.commandId ||
            response.isaClose.accountId !== validAccountId
          ) {
            context.addIssue({
              code: 'custom',
              path: ['isaClose'],
              message: 'closed ISA account does not match the submitted command',
            });
          }
        }),
      );
      try {
        return await http.post(`/api/finance/isa/${validAccountId}/close`, body, decoder);
      } catch (error) {
        const domainError = toFinanceCommandError(error);
        if (domainError === undefined) throw error;
        throw domainError;
      }
    },

    async startPension(accountId, request) {
      const validAccountId = ResourceIdSchema.parse(accountId);
      const body = PensionStartRequestSchema.parse(request);
      const decoder = asDecoder(
        PensionStartResponseSchema.superRefine((response, context) => {
          const result = response.pensionStart;
          if (
            result.commandId !== body.commandId ||
            result.accountId !== validAccountId ||
            result.paymentYears !== body.paymentYears ||
            result.lifetime !== body.lifetime
          ) {
            context.addIssue({
              code: 'custom',
              path: ['pensionStart'],
              message: 'pension start does not match the submitted command',
            });
          }
        }),
      );
      try {
        return await http.post(`/api/finance/pensions/${validAccountId}/start`, body, decoder);
      } catch (error) {
        const domainError = toFinanceCommandError(error);
        if (domainError === undefined) throw error;
        throw domainError;
      }
    },

    async withdrawPension(accountId, request) {
      const validAccountId = ResourceIdSchema.parse(accountId);
      const body = PensionWithdrawalRequestSchema.parse(request);
      const decoder = asDecoder(
        PensionWithdrawalResponseSchema.superRefine((response, context) => {
          const result = response.pensionWithdrawal;
          if (
            result.commandId !== body.commandId ||
            result.accountId !== validAccountId ||
            result.grossAmountKrw !== body.amountKrw
          ) {
            context.addIssue({
              code: 'custom',
              path: ['pensionWithdrawal'],
              message: 'pension withdrawal does not match the submitted command',
            });
          }
        }),
      );
      try {
        return await http.post(
          `/api/finance/pensions/${validAccountId}/withdrawals`,
          body,
          decoder,
        );
      } catch (error) {
        const domainError = toFinanceCommandError(error);
        if (domainError === undefined) throw error;
        throw domainError;
      }
    },

    async openDeposit(request) {
      const body = DepositOpenRequestSchema.parse(request);
      const decoder = asDecoder(
        DepositOpenResponseSchema.superRefine((response, context) => {
          const deposit = response.deposit;
          if (
            deposit.commandId !== body.commandId ||
            deposit.kind !== body.kind ||
            deposit.productVersionId !== body.productVersionId ||
            deposit.settlementAccountId !== body.settlementAccountId ||
            deposit.amountKrw !== body.amountKrw
          ) {
            context.addIssue({
              code: 'custom',
              path: ['deposit'],
              message: 'opened cash contract does not match the submitted command',
            });
          }
        }),
      );
      try {
        return await http.post('/api/finance/deposits', body, decoder);
      } catch (error) {
        const domainError = toFinanceCommandError(error);
        if (domainError === undefined) throw error;
        throw domainError;
      }
    },

    async closeDeposit(contractId, request) {
      const validContractId = ResourceIdSchema.parse(contractId);
      const body = DepositCloseRequestSchema.parse(request);
      const decoder = asDecoder(
        DepositCloseResponseSchema.superRefine((response, context) => {
          if (
            response.depositClose.commandId !== body.commandId ||
            response.depositClose.contractId !== validContractId
          ) {
            context.addIssue({
              code: 'custom',
              path: ['depositClose'],
              message: 'closed cash contract does not match the submitted command',
            });
          }
        }),
      );
      try {
        return await http.post(`/api/finance/deposits/${validContractId}/close`, body, decoder);
      } catch (error) {
        const domainError = toFinanceCommandError(error);
        if (domainError === undefined) throw error;
        throw domainError;
      }
    },

    getFinanceTaxYear(year) {
      const validYear = TaxYearSchema.parse(year);
      const decoder = asDecoder(
        FinancialIncomeYearSchema.refine((summary) => summary.taxYear === validYear, {
          path: ['taxYear'],
          message: 'tax-year result does not match the requested year',
        }),
      );
      return http.get(`/api/finance/tax-years/${validYear}`, decoder);
    },

    async transferFinance(request) {
      const body = FinanceTransferRequestSchema.parse(request);
      const decoder = asDecoder(
        FinanceTransferResponseSchema.superRefine((response, context) => {
          const transfer = response.transfer;
          if (
            transfer.commandId !== body.commandId ||
            transfer.accountId !== body.accountId ||
            transfer.direction !== body.direction ||
            transfer.amountKrw !== body.amountKrw
          ) {
            context.addIssue({
              code: 'custom',
              path: ['transfer'],
              message: 'transfer does not match the submitted command',
            });
          }
        }),
      );
      try {
        return await http.post('/api/finance/transfers', body, decoder);
      } catch (error) {
        const domainError = toFinanceCommandError(error);
        if (domainError === undefined) throw error;
        throw domainError;
      }
    },

    getFinanceLedger(before, limit = 50, signal) {
      const validLimit = Math.trunc(limit);
      if (validLimit !== limit || validLimit < 1 || validLimit > 200) {
        return Promise.reject(new RangeError('원장 조회 개수는 1 이상 200 이하여야 합니다'));
      }
      const query = new URLSearchParams({ limit: String(validLimit) });
      if (before !== undefined) query.set('before', ResourceIdSchema.parse(before));
      const decoder = asDecoder(
        LedgerPageSchema.refine((page) => page.transactions.length <= validLimit, {
          path: ['transactions'],
          message: 'ledger page exceeds the requested limit',
        }),
      );
      return http.get(
        `/api/finance/ledger?${query.toString()}`,
        decoder,
        signal === undefined ? undefined : { signal },
      );
    },

    getMarketHistory(days, signal) {
      const validDays = MarketHistoryDaysSchema.parse(days);
      const path = `/api/markets/LLX/history?days=${validDays}`;
      return http.get(path, marketHistoryDecoder, signal === undefined ? undefined : { signal });
    },

    searchEquities(query, market, limit = 20, signal) {
      const validQuery = EquitySearchTextSchema.parse(query);
      const validLimit = EquitySearchLimitSchema.parse(limit);
      const parameters = new URLSearchParams({
        q: validQuery,
        limit: String(validLimit),
      });
      if (market !== undefined) parameters.set('market', EquityMarketSchema.parse(market));
      return http.get(
        `/api/equities?${parameters.toString()}`,
        equitySearchResultDecoder,
        signal === undefined ? undefined : { signal },
      );
    },

    onTick(handler) {
      return stream.on('tick', (message) => {
        try {
          handler(snapshotDecoder.parse(JSON.parse(message.data) as unknown));
        } catch (error) {
          deps.onInvalidTick?.(error, message);
        }
      });
    },

    connectStream: () => stream.connect(),
    disconnectStream: () => stream.close(),
  };
}
