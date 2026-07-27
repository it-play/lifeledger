import type {
  GameSnapshot,
  HousingActiveLease,
  HousingCurrentLeaseResponse,
  HousingLeaseArrear,
  HousingLeaseArrearPaymentResult,
  HousingLeaseDepositLoanQuoteRequest,
  HousingLeaseDepositLoanQuoteResult,
  HousingLeaseRequest,
  HousingLeaseResult,
  HousingListing,
  HousingListingsResponse,
  HousingMortgageQuoteRequest,
  HousingMortgageQuoteResult,
  HousingMovingCost,
  HousingOffer,
  HousingPropertyHolding,
  HousingPropertyHoldingsResponse,
  HousingPropertySaleOrderCancellationResult,
  HousingPropertySaleOrderCreateRequest,
  HousingPropertySaleOrderListingResult,
  HousingPropertySaleOrderSummary,
  HousingPropertyTaxEvent,
  HousingPropertyTaxEventsResponse,
  HousingPropertyType,
  HousingPurchaseRequest,
  HousingPurchaseResult,
  HousingRegion,
  HousingRegionKey,
  LoanProduct,
  LoanProductCatalog,
} from '../../api/contracts.js';
import { type HousingApi, HousingCommandError, HousingQueryError } from '../../api/housing-api.js';
import type { LoanApi } from '../../api/loan-api.js';
import { el } from '../../lib/dom/index.js';
import { type AsyncState, createHooks } from '../../lib/hooks/index.js';
import type { Store } from '../../lib/store/index.js';
import type { View, ViewFactory } from '../../lib/view/index.js';
import { formatWon } from '../format.js';
import type { GameStateWriter } from '../game-state/index.js';
import {
  createHousingLeaseArrearPaymentRetryPolicy,
  createHousingLeaseDepositLoanQuoteRetryPolicy,
  createHousingLeaseRetryPolicy,
  createHousingMortgageQuoteRetryPolicy,
  createHousingPropertySaleOrderCancelRetryPolicy,
  createHousingPropertySaleOrderCreateRetryPolicy,
  createHousingPropertySaleOrderRepriceRetryPolicy,
  createHousingPurchaseRetryPolicy,
  type HousingLeaseArrearPaymentCommand,
  type HousingPropertySaleOrderCancelCommand,
  type HousingPropertySaleOrderRepriceCommand,
} from '../housing-retry/index.js';
import { type AppState, paths } from '../state.js';

export interface HousingViewDeps {
  readonly store: Store<AppState>;
  readonly snapshots: GameStateWriter;
  readonly api: HousingApi;
  readonly loanApi: LoanApi;
  readonly createCommandId: () => string;
}

interface HousingSummaryNodes {
  readonly status: HTMLElement;
  readonly model: HTMLElement;
  readonly residenceRegion: HTMLElement;
  readonly selectedRegion: HTMLElement;
  readonly gameDay: HTMLElement;
  readonly yearMonth: HTMLElement;
  readonly priceIndex: HTMLElement;
  readonly rentIndex: HTMLElement;
}

interface LeaseSummaryNodes {
  readonly status: HTMLElement;
  readonly capability: HTMLElement;
  readonly renewalRule: HTMLElement;
  readonly lifecycleTerms: HTMLElement;
  readonly depositAsset: HTMLElement;
  readonly activeLease: HTMLElement;
  readonly currentTerm: HTMLElement;
  readonly renewalNotice: HTMLElement;
  readonly terminationReview: HTMLElement;
  readonly monthlyRentTerms: HTMLElement;
  readonly arrearTotal: HTMLElement;
  readonly arrearWindow: HTMLElement;
}

interface MovePreviewNodes {
  readonly wallet: HTMLElement;
  readonly returnedDeposit: HTMLElement;
  readonly repaidDepositLoan: HTMLElement;
  readonly available: HTMLElement;
  readonly newDeposit: HTMLElement;
  readonly monthlyRent: HTMLElement;
  readonly movingCost: HTMLElement;
  readonly required: HTMLElement;
}

interface MoveResultNodes {
  readonly section: HTMLElement;
  readonly lease: HTMLElement;
  readonly deposit: HTMLElement;
  readonly monthlyRent: HTMLElement;
  readonly returnedDeposit: HTMLElement;
  readonly movingCost: HTMLElement;
  readonly walletDelta: HTMLElement;
  readonly depositLoan: HTMLElement;
  readonly repaidDepositLoan: HTMLElement;
}

interface DepositLoanQuoteNodes {
  readonly section: HTMLElement;
  readonly decision: HTMLElement;
  readonly requested: HTMLElement;
  readonly collateral: HTMLElement;
  readonly income: HTMLElement;
  readonly affordability: HTMLElement;
  readonly terms: HTMLElement;
  readonly replacement: HTMLElement;
  readonly balances: HTMLElement;
}

interface MortgageQuoteNodes {
  readonly section: HTMLElement;
  readonly decision: HTMLElement;
  readonly purchase: HTMLElement;
  readonly collateral: HTMLElement;
  readonly ltv: HTMLElement;
  readonly income: HTMLElement;
  readonly dsr: HTMLElement;
  readonly ownFunds: HTMLElement;
  readonly terms: HTMLElement;
  readonly leaseExit: HTMLElement;
}

interface PurchaseResultNodes {
  readonly section: HTMLElement;
  readonly holding: HTMLElement;
  readonly acquisition: HTMLElement;
  readonly leaseExit: HTMLElement;
  readonly wallet: HTMLElement;
  readonly mortgage: HTMLElement;
}

interface FixedRegionSelect {
  readonly element: HTMLSelectElement;
  setRegions(regions: readonly HousingRegion[], selected: HousingRegionKey | undefined): void;
}

interface FixedListingTable {
  setListings(listings: readonly HousingListing[]): void;
}

interface FixedMovingCostList {
  readonly element: HTMLUListElement;
  setItems(items: readonly HousingMovingCost[]): void;
}

interface FixedLeaseSelect {
  readonly element: HTMLSelectElement;
  setListings(
    listings: readonly HousingListing[],
    capability: HousingCurrentLeaseResponse['leaseCapability'] | undefined,
    selectedKey: string,
    pending: HousingLeaseRequest | HousingLeaseDepositLoanQuoteRequest | undefined,
  ): string;
}

interface FixedDepositLoanProductSelect {
  readonly element: HTMLSelectElement;
  setItems(items: readonly LoanProduct[], selectedId: string): string;
}

interface FixedSaleSelect {
  readonly element: HTMLSelectElement;
  setListings(
    listings: readonly HousingListing[],
    selectedId: string,
    pending: HousingMortgageQuoteRequest | HousingPurchaseRequest | undefined,
  ): string;
}

interface FixedPropertyHoldingList {
  readonly element: HTMLUListElement;
  setItems(items: readonly HousingPropertyHolding[]): void;
}

interface FixedPropertyHoldingSelect {
  readonly element: HTMLSelectElement;
  setItems(
    items: readonly HousingPropertyHolding[],
    selectedId: string,
    pendingId?: string,
  ): string;
}

interface FixedPropertySaleOrderSelect {
  readonly element: HTMLSelectElement;
  setItems(
    items: readonly HousingPropertySaleOrderSummary[],
    selectedId: string,
    pendingId?: string,
  ): string;
}

interface FixedPropertySaleOrderList {
  readonly element: HTMLUListElement;
  setItems(items: readonly HousingPropertySaleOrderSummary[]): void;
}

interface FixedPropertyTaxEventList {
  readonly element: HTMLUListElement;
  setItems(items: readonly HousingPropertyTaxEvent[]): void;
}

interface FixedPropertyTaxHoldingSelect {
  readonly element: HTMLSelectElement;
  setItems(
    holdings: readonly HousingPropertyHolding[],
    saleOrders: readonly HousingPropertySaleOrderSummary[],
    selectedId: string,
  ): string;
}

interface FixedLeaseArrearList {
  readonly element: HTMLUListElement;
  setItems(items: readonly HousingLeaseArrear[]): void;
}

interface FixedLeaseArrearSelect {
  readonly element: HTMLSelectElement;
  setItems(items: readonly HousingLeaseArrear[], pendingId: string | undefined): string;
}

interface ListingRow {
  readonly element: HTMLTableRowElement;
  setListing(listing: HousingListing | undefined): void;
}

type TenantHousingOffer =
  | Extract<HousingOffer, { kind: 'jeonse' }>
  | Extract<HousingOffer, { kind: 'monthlyRent' }>;

type SaleHousingOffer = Extract<HousingOffer, { kind: 'sale' }>;

const PROPERTY_TYPE_LABEL: Record<HousingPropertyType, string> = {
  apartment: '아파트',
  multiFamily: '다세대 주택',
  detached: '단독 주택',
};

const REGION_LABEL: Record<HousingRegionKey, string> = {
  capitalArea: '수도권',
  metropolitan: '광역시',
  smallCity: '중소도시',
  rural: '농촌',
};

const MAX_REGIONS = 4;
const MAX_LISTINGS = 24;
const MAX_LEASE_OFFERS = MAX_LISTINGS * 2;
const MAX_LEASE_ARREARS = 20;
const MAX_LOAN_PRODUCTS = 16;
const MAX_PROPERTY_HOLDINGS = 4;
const MAX_PROPERTY_HISTORY = 20;
const MAX_PROPERTY_TAX_HOLDINGS = MAX_PROPERTY_HOLDINGS + MAX_PROPERTY_HISTORY;

class HousingFormError extends Error {}

/** M4-C housing listings, tenant leases, rent arrears, and atomic moves. */
export function createHousingView(deps: HousingViewDeps): ViewFactory {
  const leaseRetries = createHousingLeaseRetryPolicy({
    createCommandId: deps.createCommandId,
  });
  const quoteRetries = createHousingLeaseDepositLoanQuoteRetryPolicy({
    createCommandId: deps.createCommandId,
  });
  const arrearPaymentRetries = createHousingLeaseArrearPaymentRetryPolicy({
    createCommandId: deps.createCommandId,
  });
  const mortgageQuoteRetries = createHousingMortgageQuoteRetryPolicy({
    createCommandId: deps.createCommandId,
  });
  const purchaseRetries = createHousingPurchaseRetryPolicy({
    createCommandId: deps.createCommandId,
  });
  const propertySaleCreateRetries = createHousingPropertySaleOrderCreateRetryPolicy({
    createCommandId: deps.createCommandId,
  });
  const propertySaleRepriceRetries = createHousingPropertySaleOrderRepriceRetryPolicy({
    createCommandId: deps.createCommandId,
  });
  const propertySaleCancelRetries = createHousingPropertySaleOrderCancelRetryPolicy({
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
      const selectedRegion = h.useSignal<HousingRegionKey | undefined>(undefined);
      const selectedLeaseKey = h.useSignal('');
      const selectedDepositLoanProductId = h.useSignal('');
      const selectedSaleListingId = h.useSignal('');
      const selectedMortgageProductId = h.useSignal('');
      const commandBusy = h.useSignal(false);
      const commandFeedback = h.useSignal('');
      const quoteFeedback = h.useSignal('');
      const mortgageFeedback = h.useSignal('');
      const purchaseFeedback = h.useSignal('');
      const commandResult = h.useSignal<HousingLeaseResult | undefined>(undefined);
      const quoteResult = h.useSignal<HousingLeaseDepositLoanQuoteResult | undefined>(undefined);
      const mortgageQuoteResult = h.useSignal<HousingMortgageQuoteResult | undefined>(undefined);
      const purchaseResult = h.useSignal<HousingPurchaseResult | undefined>(undefined);
      const paymentResult = h.useSignal<HousingLeaseArrearPaymentResult | undefined>(undefined);
      const pendingRequest = h.useSignal<HousingLeaseRequest | undefined>(
        pendingForSnapshot(leaseRetries, deps.store.getState().game.snapshot),
      );
      const pendingPayment = h.useSignal<HousingLeaseArrearPaymentCommand | undefined>(
        pendingPaymentForSnapshot(arrearPaymentRetries, deps.store.getState().game.snapshot),
      );
      const pendingQuote = h.useSignal<HousingLeaseDepositLoanQuoteRequest | undefined>(
        pendingQuoteForSnapshot(quoteRetries, deps.store.getState().game.snapshot),
      );
      const pendingMortgageQuote = h.useSignal<HousingMortgageQuoteRequest | undefined>(
        pendingMortgageQuoteForSnapshot(mortgageQuoteRetries, deps.store.getState().game.snapshot),
      );
      const pendingPurchase = h.useSignal<HousingPurchaseRequest | undefined>(
        pendingPurchaseForSnapshot(purchaseRetries, deps.store.getState().game.snapshot),
      );
      const selectedPropertySaleHoldingId = h.useSignal('');
      const selectedPropertySaleOrderId = h.useSignal('');
      const selectedPropertyTaxHoldingId = h.useSignal('');
      const propertySaleFeedback = h.useSignal('');
      const propertyTaxFeedback = h.useSignal('');
      const propertySaleListingResult = h.useSignal<
        HousingPropertySaleOrderListingResult | undefined
      >(undefined);
      const propertySaleCancellationResult = h.useSignal<
        HousingPropertySaleOrderCancellationResult | undefined
      >(undefined);
      const pendingPropertySaleCreate = h.useSignal<
        HousingPropertySaleOrderCreateRequest | undefined
      >(
        pendingPropertySaleCreateForSnapshot(
          propertySaleCreateRetries,
          deps.store.getState().game.snapshot,
        ),
      );
      const pendingPropertySaleReprice = h.useSignal<
        HousingPropertySaleOrderRepriceCommand | undefined
      >(
        pendingPropertySaleRepriceForSnapshot(
          propertySaleRepriceRetries,
          deps.store.getState().game.snapshot,
        ),
      );
      const pendingPropertySaleCancel = h.useSignal<
        HousingPropertySaleOrderCancelCommand | undefined
      >(
        pendingPropertySaleCancelForSnapshot(
          propertySaleCancelRetries,
          deps.store.getState().game.snapshot,
        ),
      );
      const gameReady = h.useComputed(() => {
        const current = snapshot.get();
        return current !== undefined && current.characterName !== null;
      });
      const listingsRequest = h.useAsync((signal) => {
        const region = selectedRegion.peek();
        return deps.api.listListings(region === undefined ? undefined : { region }, signal);
      });
      const currentLeaseRequest = h.useAsync((signal) => deps.api.getCurrentLease(signal));
      const loanProductsRequest = h.useAsync((signal) => deps.loanApi.listProducts(signal));
      const holdingsRequest = h.useAsync((signal) => deps.api.getHoldings(signal));
      const propertySalesRequest = h.useAsync((signal) =>
        deps.api.listPropertySales(undefined, signal),
      );
      const propertyTaxEventsRequest = h.useAsync((signal) => {
        const holdingId = selectedPropertyTaxHoldingId.peek();
        if (holdingId === '')
          throw new HousingFormError('세금 이력을 조회할 보유주택을 선택하세요.');
        return deps.api.listPropertyTaxEvents(holdingId, undefined, signal);
      });
      const listings = h.useComputed(() => {
        const state = listingsRequest.state.get();
        return state.status === 'success' ? state.value.listings : [];
      });
      const currentLease = h.useComputed(() => {
        const state = currentLeaseRequest.state.get();
        return state.status === 'success' ? state.value : undefined;
      });
      const activePropertyHoldings = h.useComputed(() => {
        const state = holdingsRequest.state.get();
        return state.status === 'success' ? state.value.holdings : [];
      });
      const propertySaleOrders = h.useComputed(() => {
        const state = propertySalesRequest.state.get();
        return state.status === 'success' ? state.value.items : [];
      });
      const selectedPropertySaleHolding = h.useComputed(() =>
        activePropertyHoldings
          .get()
          .find((holding) => holding.id === selectedPropertySaleHoldingId.get()),
      );
      const selectedPropertySaleOrder = h.useComputed(() =>
        propertySaleOrders
          .get()
          .find((order) => order.orderId === selectedPropertySaleOrderId.get()),
      );
      const selectedLease = h.useComputed(() =>
        selectedLeaseOf(listings.get(), selectedLeaseKey.get()),
      );
      const selectedListing = h.useComputed(() => selectedLease.get()?.listing);
      const selectedOffer = h.useComputed(() => selectedLease.get()?.offer);
      const depositLoanProducts = h.useComputed(() => {
        const state = loanProductsRequest.state.get();
        return state.status === 'success'
          ? state.value.products.filter(isHousingDepositLoanProduct)
          : [];
      });
      const selectedDepositLoanProduct = h.useComputed(() =>
        depositLoanProducts
          .get()
          .find((product) => product.id === selectedDepositLoanProductId.get()),
      );
      const selectedSaleListing = h.useComputed(() =>
        listings.get().find((listing) => listing.id === selectedSaleListingId.get()),
      );
      const selectedSaleOffer = h.useComputed(() => saleOfferOf(selectedSaleListing.get()));
      const mortgageProducts = h.useComputed(() => {
        const state = loanProductsRequest.state.get();
        return state.status === 'success'
          ? state.value.products.filter(isHousingMortgageProduct)
          : [];
      });
      const selectedMortgageProduct = h.useComputed(() =>
        mortgageProducts.get().find((product) => product.id === selectedMortgageProductId.get()),
      );
      const selectedMovingCost = h.useComputed(() => {
        const lease = currentLease.get();
        const listing = selectedListing.get();
        return lease?.movingCosts.find((cost) => cost.regionKey === listing?.regionKey);
      });
      const canMove = h.useComputed(() => {
        const current = snapshot.get();
        const pending = pendingRequest.get();
        if (
          current === undefined ||
          current.characterName === null ||
          current.autoSpeed !== null ||
          advancing.get() ||
          ordering.get() ||
          commandBusy.get() ||
          current.life.residence?.tenureKind === 'owner' ||
          current.life.activePropertyHoldings.length > 0 ||
          pendingPayment.get() !== undefined ||
          pendingQuote.get() !== undefined ||
          pendingMortgageQuote.get() !== undefined ||
          pendingPurchase.get() !== undefined
        ) {
          return false;
        }
        if (pending !== undefined) {
          return pending.expectedRunRevision === current.runRevision && !('loanQuoteId' in pending);
        }
        const lease = currentLease.get();
        return (
          lease !== undefined &&
          lease.leaseCapability !== 'unavailable' &&
          selectedListing.get() !== undefined &&
          selectedOffer.get() !== undefined &&
          capabilitySupportsOffer(lease.leaseCapability, selectedOffer.get()?.kind) &&
          selectedMovingCost.get() !== undefined &&
          current.life.residence !== null &&
          current.gameDay > current.life.residence.effectiveFromGameDay
        );
      });
      const canPayArrear = h.useComputed(() => {
        const current = snapshot.get();
        const pending = pendingPayment.get();
        if (
          current === undefined ||
          current.characterName === null ||
          current.autoSpeed !== null ||
          advancing.get() ||
          ordering.get() ||
          commandBusy.get() ||
          pendingRequest.get() !== undefined ||
          pendingQuote.get() !== undefined ||
          pendingMortgageQuote.get() !== undefined ||
          pendingPurchase.get() !== undefined
        ) {
          return false;
        }
        if (pending !== undefined)
          return pending.request.expectedRunRevision === current.runRevision;
        const lease = currentLease.get();
        return (
          lease?.leaseCapability === 'cashJeonseAndMonthlyRent' && lease.activeArrears.length > 0
        );
      });
      const canQuoteDepositLoan = h.useComputed(() => {
        const current = snapshot.get();
        const pending = pendingQuote.get();
        if (
          current === undefined ||
          current.characterName === null ||
          current.autoSpeed !== null ||
          advancing.get() ||
          ordering.get() ||
          commandBusy.get() ||
          current.life.residence?.tenureKind === 'owner' ||
          current.life.activePropertyHoldings.length > 0 ||
          pendingRequest.get() !== undefined ||
          pendingPayment.get() !== undefined ||
          pendingMortgageQuote.get() !== undefined ||
          pendingPurchase.get() !== undefined
        ) {
          return false;
        }
        if (pending !== undefined) return pending.expectedRunRevision === current.runRevision;
        const capability = currentLease.get()?.leaseCapability;
        return (
          capability !== undefined &&
          capabilitySupportsOffer(capability, 'jeonse') &&
          selectedOffer.get()?.kind === 'jeonse' &&
          selectedDepositLoanProduct.get()?.rateStatus === 'available'
        );
      });
      const canMoveWithDepositLoan = h.useComputed(() => {
        const current = snapshot.get();
        const pending = pendingRequest.get();
        if (
          current === undefined ||
          current.characterName === null ||
          current.autoSpeed !== null ||
          advancing.get() ||
          ordering.get() ||
          commandBusy.get() ||
          current.life.residence?.tenureKind === 'owner' ||
          current.life.activePropertyHoldings.length > 0 ||
          pendingPayment.get() !== undefined ||
          pendingQuote.get() !== undefined ||
          pendingMortgageQuote.get() !== undefined ||
          pendingPurchase.get() !== undefined
        ) {
          return false;
        }
        if (pending !== undefined) {
          return pending.expectedRunRevision === current.runRevision && 'loanQuoteId' in pending;
        }
        const quote = quoteResult.get();
        const lease = currentLease.get();
        return (
          quote?.decisionCode === 'eligible' &&
          quote.expiresGameDay === current.gameDay &&
          quote.listingId === selectedListing.get()?.id &&
          quote.productVersionId === selectedDepositLoanProduct.get()?.id &&
          selectedOffer.get()?.kind === 'jeonse' &&
          selectedMovingCost.get() !== undefined &&
          current.life.residence !== null &&
          current.gameDay > current.life.residence.effectiveFromGameDay &&
          lease !== undefined &&
          capabilitySupportsOffer(lease.leaseCapability, 'jeonse')
        );
      });
      const canQuoteMortgage = h.useComputed(() => {
        const current = snapshot.get();
        const pending = pendingMortgageQuote.get();
        if (
          current === undefined ||
          current.characterName === null ||
          current.autoSpeed !== null ||
          advancing.get() ||
          ordering.get() ||
          commandBusy.get() ||
          pendingRequest.get() !== undefined ||
          pendingPayment.get() !== undefined ||
          pendingQuote.get() !== undefined ||
          pendingPurchase.get() !== undefined
        ) {
          return false;
        }
        if (pending !== undefined) return pending.expectedRunRevision === current.runRevision;
        const holdingState = holdingsRequest.state.get();
        return (
          holdingState.status === 'success' &&
          holdingState.value.purchaseCapability === 'ownerOccupiedSingleHome' &&
          holdingState.value.holdings.length === 0 &&
          selectedSaleOffer.get() !== undefined &&
          selectedMortgageProduct.get()?.rateStatus === 'available' &&
          current.life.residence !== null &&
          current.gameDay > current.life.residence.effectiveFromGameDay
        );
      });
      const canPurchaseCash = h.useComputed(() => {
        const current = snapshot.get();
        const pending = pendingPurchase.get();
        if (
          current === undefined ||
          current.characterName === null ||
          current.autoSpeed !== null ||
          advancing.get() ||
          ordering.get() ||
          commandBusy.get() ||
          pendingRequest.get() !== undefined ||
          pendingPayment.get() !== undefined ||
          pendingQuote.get() !== undefined ||
          pendingMortgageQuote.get() !== undefined
        ) {
          return false;
        }
        if (pending !== undefined) {
          return (
            pending.expectedRunRevision === current.runRevision && pending.mortgageQuoteId === null
          );
        }
        const holdingState = holdingsRequest.state.get();
        return (
          holdingState.status === 'success' &&
          holdingState.value.purchaseCapability === 'ownerOccupiedSingleHome' &&
          holdingState.value.holdings.length === 0 &&
          selectedSaleOffer.get() !== undefined &&
          current.life.residence !== null &&
          current.gameDay > current.life.residence.effectiveFromGameDay
        );
      });
      const canPurchaseWithMortgage = h.useComputed(() => {
        const current = snapshot.get();
        const pending = pendingPurchase.get();
        if (
          current === undefined ||
          current.characterName === null ||
          current.autoSpeed !== null ||
          advancing.get() ||
          ordering.get() ||
          commandBusy.get() ||
          pendingRequest.get() !== undefined ||
          pendingPayment.get() !== undefined ||
          pendingQuote.get() !== undefined ||
          pendingMortgageQuote.get() !== undefined
        ) {
          return false;
        }
        if (pending !== undefined) {
          return (
            pending.expectedRunRevision === current.runRevision && pending.mortgageQuoteId !== null
          );
        }
        const quote = mortgageQuoteResult.get();
        return (
          canPurchaseCash.get() &&
          quote?.decisionCode === 'eligible' &&
          quote.expiresGameDay === current.gameDay &&
          quote.listingId === selectedSaleListing.get()?.id &&
          quote.productVersionId === selectedMortgageProduct.get()?.id
        );
      });
      const propertySaleBaseBlocked = (): boolean => {
        const current = snapshot.get();
        return (
          current === undefined ||
          current.characterName === null ||
          current.autoSpeed !== null ||
          advancing.get() ||
          ordering.get() ||
          commandBusy.get() ||
          pendingRequest.get() !== undefined ||
          pendingPayment.get() !== undefined ||
          pendingQuote.get() !== undefined ||
          pendingMortgageQuote.get() !== undefined ||
          pendingPurchase.get() !== undefined
        );
      };
      const canCreatePropertySale = h.useComputed(() => {
        const current = snapshot.get();
        const pending = pendingPropertySaleCreate.get();
        if (
          propertySaleBaseBlocked() ||
          current === undefined ||
          pendingPropertySaleReprice.get() !== undefined ||
          pendingPropertySaleCancel.get() !== undefined
        ) {
          return false;
        }
        if (pending !== undefined) return pending.expectedRunRevision === current.runRevision;
        return selectedPropertySaleHolding.get() !== undefined;
      });
      const canRepricePropertySale = h.useComputed(() => {
        const current = snapshot.get();
        const pending = pendingPropertySaleReprice.get();
        if (
          propertySaleBaseBlocked() ||
          current === undefined ||
          pendingPropertySaleCreate.get() !== undefined ||
          pendingPropertySaleCancel.get() !== undefined
        ) {
          return false;
        }
        if (pending !== undefined) {
          return pending.request.expectedRunRevision === current.runRevision;
        }
        return selectedPropertySaleOrder.get()?.status === 'active';
      });
      const canCancelPropertySale = h.useComputed(() => {
        const current = snapshot.get();
        const pending = pendingPropertySaleCancel.get();
        if (
          propertySaleBaseBlocked() ||
          current === undefined ||
          pendingPropertySaleCreate.get() !== undefined ||
          pendingPropertySaleReprice.get() !== undefined
        ) {
          return false;
        }
        if (pending !== undefined) {
          return pending.request.expectedRunRevision === current.runRevision;
        }
        return selectedPropertySaleOrder.get()?.status === 'active';
      });

      const status = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const model = el('dd');
      const residenceRegion = el('dd');
      const selectedRegionText = el('dd');
      const gameDay = el('dd');
      const yearMonth = el('dd');
      const priceIndex = el('dd');
      const rentIndex = el('dd');
      const regionSelect = createFixedRegionSelect();
      const reload = el('button', { type: 'button' }, '다시 불러오기');
      const listingBody = el('tbody');
      const listingTable = createFixedListingTable(listingBody);
      const summaryNodes: HousingSummaryNodes = {
        status,
        model,
        residenceRegion,
        selectedRegion: selectedRegionText,
        gameDay,
        yearMonth,
        priceIndex,
        rentIndex,
      };

      const leaseStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const leaseCapability = el('dd');
      const leaseRenewalRule = el('dd');
      const leaseLifecycleTerms = el('dd');
      const leaseDepositAsset = el('dd');
      const activeLease = el('dd');
      const leaseCurrentTerm = el('dd');
      const leaseRenewalNotice = el('dd');
      const leaseTerminationReview = el('dd');
      const monthlyRentTerms = el('dd');
      const leaseArrearTotal = el('dd');
      const leaseArrearWindow = el('dd');
      const movingCosts = createFixedMovingCostList();
      const leaseArrearList = createFixedLeaseArrearList();
      const leaseSummaryNodes: LeaseSummaryNodes = {
        status: leaseStatus,
        capability: leaseCapability,
        renewalRule: leaseRenewalRule,
        lifecycleTerms: leaseLifecycleTerms,
        depositAsset: leaseDepositAsset,
        activeLease,
        currentTerm: leaseCurrentTerm,
        renewalNotice: leaseRenewalNotice,
        terminationReview: leaseTerminationReview,
        monthlyRentTerms,
        arrearTotal: leaseArrearTotal,
        arrearWindow: leaseArrearWindow,
      };

      const arrearSelect = createFixedLeaseArrearSelect();
      const arrearAmount = el('input', {
        name: 'amountKrw',
        type: 'number',
        attrs: { min: '1', step: '1', inputmode: 'numeric' },
      });
      const partialPaymentSubmit = el('button', { type: 'submit' }, '입력 금액 상환');
      const fullPayment = el('button', { type: 'button' }, '남은 금액 전액 상환');
      const arrearPaymentForm = el(
        'form',
        {},
        el('label', {}, '상환할 월세 연체 ', arrearSelect.element),
        ' ',
        el('label', {}, '상환 금액 ', arrearAmount, '원'),
        ' ',
        partialPaymentSubmit,
        ' ',
        fullPayment,
      );

      const depositLoanProductSelect = createFixedDepositLoanProductSelect();
      const depositLoanPrincipal = el('input', {
        name: 'principalKrw',
        type: 'number',
        attrs: { min: '1', step: '1', inputmode: 'numeric' },
      });
      const depositLoanQuoteSubmit = el('button', { type: 'submit' }, '전세자금대출 견적 받기');
      const financedMoveSubmit = el('button', { type: 'button' }, '견적 대출로 전세 입주');
      const depositLoanQuoteForm = el(
        'form',
        {},
        el('label', {}, '전세자금대출 상품 ', depositLoanProductSelect.element),
        ' ',
        el('label', {}, '요청 원금 ', depositLoanPrincipal, '원'),
        ' ',
        depositLoanQuoteSubmit,
      );
      const depositLoanStatus = el('p', {
        attrs: { role: 'status', 'aria-live': 'polite' },
      });
      const depositLoanQuoteNodes: DepositLoanQuoteNodes = {
        section: el('section'),
        decision: el('dd'),
        requested: el('dd'),
        collateral: el('dd'),
        income: el('dd'),
        affordability: el('dd'),
        terms: el('dd'),
        replacement: el('dd'),
        balances: el('dd'),
      };
      depositLoanQuoteNodes.section.append(
        el('h3', {}, '최근 전세자금대출 견적'),
        el(
          'dl',
          {},
          el('dt', {}, '심사 결과'),
          depositLoanQuoteNodes.decision,
          el('dt', {}, '신청 내용'),
          depositLoanQuoteNodes.requested,
          el('dt', {}, '보증금 한도'),
          depositLoanQuoteNodes.collateral,
          el('dt', {}, '검증 소득'),
          depositLoanQuoteNodes.income,
          el('dt', {}, '개발 상환여력'),
          depositLoanQuoteNodes.affordability,
          el('dt', {}, '대출 조건과 첫 납입'),
          depositLoanQuoteNodes.terms,
          el('dt', {}, '기존 전세대출 대체'),
          depositLoanQuoteNodes.replacement,
          el('dt', {}, '실행 전후 총 대출 원금'),
          depositLoanQuoteNodes.balances,
        ),
      );
      const depositLoanSection = el(
        'section',
        {},
        el('h2', {}, '전세자금대출과 전세 입주'),
        el(
          'p',
          {},
          '전세 매물과 서버가 게시한 전용 상품을 선택합니다. 대출금은 지갑을 거치지 않고 보증금에 직접 지급됩니다.',
        ),
        depositLoanStatus,
        depositLoanQuoteForm,
        depositLoanQuoteNodes.section,
        financedMoveSubmit,
      );

      const leaseSelect = createFixedLeaseSelect();
      const moveSubmit = el('button', { type: 'submit' }, '선택한 임대차로 이사');
      const moveForm = el(
        'form',
        {},
        el('label', {}, '임대차 조건 ', leaseSelect.element),
        moveSubmit,
      );
      const moveSection = el('section');
      const moveStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const commandMessage = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const previewNodes: MovePreviewNodes = {
        wallet: el('dd'),
        returnedDeposit: el('dd'),
        repaidDepositLoan: el('dd'),
        available: el('dd'),
        newDeposit: el('dd'),
        monthlyRent: el('dd'),
        movingCost: el('dd'),
        required: el('dd'),
      };
      moveSection.append(
        el('h2', {}, '현금 임대차 이사'),
        el(
          'p',
          {},
          '기존 임대차 보증금을 먼저 반환받고 새 보증금과 서버가 정한 이사비를 한 transaction에서 지급합니다. 월세의 첫 청구는 입주 다음 시장 월 1일입니다.',
        ),
        moveStatus,
        el(
          'dl',
          {},
          el('dt', {}, '현재 지갑'),
          previewNodes.wallet,
          el('dt', {}, '반환받을 기존 보증금'),
          previewNodes.returnedDeposit,
          el('dt', {}, '반환 보증금에서 먼저 상환할 전세대출'),
          previewNodes.repaidDepositLoan,
          el('dt', {}, '이사에 사용할 수 있는 금액'),
          previewNodes.available,
          el('dt', {}, '새 임대차 보증금'),
          previewNodes.newDeposit,
          el('dt', {}, '다음 달부터 적용될 월세'),
          previewNodes.monthlyRent,
          el('dt', {}, '목적지 이사비'),
          previewNodes.movingCost,
          el('dt', {}, '필요 금액'),
          previewNodes.required,
        ),
        moveForm,
      );

      const resultNodes: MoveResultNodes = {
        section: el('section'),
        lease: el('dd'),
        deposit: el('dd'),
        monthlyRent: el('dd'),
        returnedDeposit: el('dd'),
        movingCost: el('dd'),
        walletDelta: el('dd'),
        depositLoan: el('dd'),
        repaidDepositLoan: el('dd'),
      };
      resultNodes.section.append(
        el('h2', {}, '최근 이사 결과'),
        el(
          'dl',
          {},
          el('dt', {}, '새 계약'),
          resultNodes.lease,
          el('dt', {}, '새 보증금'),
          resultNodes.deposit,
          el('dt', {}, '월세'),
          resultNodes.monthlyRent,
          el('dt', {}, '반환 보증금'),
          resultNodes.returnedDeposit,
          el('dt', {}, '이사비'),
          resultNodes.movingCost,
          el('dt', {}, '지갑 변화'),
          resultNodes.walletDelta,
          el('dt', {}, '새 전세자금대출'),
          resultNodes.depositLoan,
          el('dt', {}, '상환한 기존 전세대출'),
          resultNodes.repaidDepositLoan,
        ),
      );

      const paymentResultSection = el('section');
      const paymentResultArrear = el('dd');
      const paymentResultId = el('dd');
      const paymentResultPaid = el('dd');
      const paymentResultRemaining = el('dd');
      paymentResultSection.append(
        el('h2', {}, '최근 월세 연체 상환 결과'),
        el(
          'dl',
          {},
          el('dt', {}, '연체 ID'),
          paymentResultArrear,
          el('dt', {}, '지급 ID'),
          paymentResultId,
          el('dt', {}, '상환액'),
          paymentResultPaid,
          el('dt', {}, '남은 금액'),
          paymentResultRemaining,
        ),
      );

      const holdingsStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const purchaseCapability = el('dd');
      const propertyBookValue = el('dd');
      const propertyHoldings = createFixedPropertyHoldingList();
      const saleSelect = createFixedSaleSelect();
      const mortgageProductSelect = createFixedMortgageProductSelect();
      const mortgagePrincipal = el('input', {
        name: 'mortgagePrincipalKrw',
        type: 'number',
        attrs: { min: '1', step: '1', inputmode: 'numeric' },
      });
      const mortgageQuoteSubmit = el('button', { type: 'submit' }, '주택담보대출 견적 받기');
      const mortgageQuoteForm = el(
        'form',
        {},
        el('label', {}, '매수할 매물 ', saleSelect.element),
        ' ',
        el('label', {}, '주택담보대출 상품 ', mortgageProductSelect.element),
        ' ',
        el('label', {}, '요청 원금 ', mortgagePrincipal, '원'),
        ' ',
        mortgageQuoteSubmit,
      );
      const cashPurchase = el('button', { type: 'button' }, '선택한 매물 현금 매수');
      const financedPurchase = el('button', { type: 'button' }, '견적 주담대로 매수');
      const mortgageStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const purchaseStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const mortgageQuoteNodes: MortgageQuoteNodes = {
        section: el('section'),
        decision: el('dd'),
        purchase: el('dd'),
        collateral: el('dd'),
        ltv: el('dd'),
        income: el('dd'),
        dsr: el('dd'),
        ownFunds: el('dd'),
        terms: el('dd'),
        leaseExit: el('dd'),
      };
      mortgageQuoteNodes.section.append(
        el('h3', {}, '최근 주택담보대출 견적'),
        el(
          'dl',
          {},
          el('dt', {}, '심사 결과'),
          mortgageQuoteNodes.decision,
          el('dt', {}, '매매가·부대비용·이사비'),
          mortgageQuoteNodes.purchase,
          el('dt', {}, '인정 담보가치와 최대 주담대'),
          mortgageQuoteNodes.collateral,
          el('dt', {}, 'LTV 근거'),
          mortgageQuoteNodes.ltv,
          el('dt', {}, '검증 소득'),
          mortgageQuoteNodes.income,
          el('dt', {}, 'DSR 근거'),
          mortgageQuoteNodes.dsr,
          el('dt', {}, '자기자금 근거'),
          mortgageQuoteNodes.ownFunds,
          el('dt', {}, '대출 조건과 첫 납입'),
          mortgageQuoteNodes.terms,
          el('dt', {}, '기존 임대차 종료 근거'),
          mortgageQuoteNodes.leaseExit,
        ),
      );
      const purchaseResultNodes: PurchaseResultNodes = {
        section: el('section'),
        holding: el('dd'),
        acquisition: el('dd'),
        leaseExit: el('dd'),
        wallet: el('dd'),
        mortgage: el('dd'),
      };
      purchaseResultNodes.section.append(
        el('h3', {}, '최근 주택 매수 결과'),
        el(
          'dl',
          {},
          el('dt', {}, '보유주택'),
          purchaseResultNodes.holding,
          el('dt', {}, '취득 금액'),
          purchaseResultNodes.acquisition,
          el('dt', {}, '기존 임대차 종료'),
          purchaseResultNodes.leaseExit,
          el('dt', {}, '지갑 변화'),
          purchaseResultNodes.wallet,
          el('dt', {}, '주택담보대출'),
          purchaseResultNodes.mortgage,
        ),
      );
      const purchaseSection = el(
        'section',
        {},
        el('h2', {}, '보유주택 매수'),
        el(
          'p',
          {},
          '매매가·취득 부대비용·이사비·담보한도는 서버가 확정합니다. 주담대 원금은 지갑을 거치지 않고 매도인 지급에 직접 충당됩니다.',
        ),
        holdingsStatus,
        el(
          'dl',
          {},
          el('dt', {}, '매수 기능'),
          purchaseCapability,
          el('dt', {}, '보유주택 장부가 합계'),
          propertyBookValue,
        ),
        propertyHoldings.element,
        mortgageStatus,
        mortgageQuoteForm,
        mortgageQuoteNodes.section,
        cashPurchase,
        ' ',
        financedPurchase,
        purchaseStatus,
        purchaseResultNodes.section,
      );

      const propertySaleHoldingSelect = createFixedPropertyHoldingSelect('매도할 보유주택');
      const propertySaleAskingPrice = el('input', {
        name: 'propertySaleAskingPriceKrw',
        type: 'number',
        attrs: { min: '1', step: '1', inputmode: 'numeric' },
      });
      const propertySaleCreateSubmit = el('button', { type: 'submit' }, '매도 주문 만들기');
      const propertySaleCreateForm = el(
        'form',
        {},
        el('label', {}, '보유주택 ', propertySaleHoldingSelect.element),
        ' ',
        el('label', {}, '주문가 ', propertySaleAskingPrice, '원'),
        ' ',
        propertySaleCreateSubmit,
      );
      const propertySaleOrderSelect = createFixedPropertySaleOrderSelect();
      const propertySaleReprice = el('input', {
        name: 'propertySaleRepriceKrw',
        type: 'number',
        attrs: { min: '1', step: '1', inputmode: 'numeric' },
      });
      const propertySaleRepriceSubmit = el('button', { type: 'submit' }, '주문가 변경');
      const propertySaleRepriceForm = el(
        'form',
        {},
        el('label', {}, '활성 주문 ', propertySaleOrderSelect.element),
        ' ',
        el('label', {}, '새 주문가 ', propertySaleReprice, '원'),
        ' ',
        propertySaleRepriceSubmit,
      );
      const propertySaleCancel = el('button', { type: 'button' }, '선택한 주문 취소');
      const propertySaleStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const propertySaleResultText = el('p');
      const propertySaleOrderList = createFixedPropertySaleOrderList();
      const propertyTaxHoldingSelect = createFixedPropertyTaxHoldingSelect();
      const propertyTaxReload = el('button', { type: 'button' }, '세금 이력 새로고침');
      const propertyTaxStatus = el('p', { attrs: { role: 'status', 'aria-live': 'polite' } });
      const propertyTaxEventList = createFixedPropertyTaxEventList();
      const propertySaleSection = el(
        'section',
        {},
        el('h2', {}, '보유주택 매도와 부동산 세금'),
        el('p', {}, '기준가·후보 체결일·거래비용·담보상환·양도세·순수령액은 서버가 확정합니다.'),
        propertySaleCreateForm,
        propertySaleRepriceForm,
        propertySaleCancel,
        propertySaleStatus,
        propertySaleResultText,
        el('h3', {}, '매도 주문 이력'),
        propertySaleOrderList.element,
        el('h3', {}, '취득·보유·양도세 이력'),
        el('label', {}, '보유주택 ', propertyTaxHoldingSelect.element),
        ' ',
        propertyTaxReload,
        propertyTaxStatus,
        propertyTaxEventList.element,
      );

      host.replaceChildren(
        el(
          'main',
          {},
          el('h1', {}, '주거 시장'),
          el('p', {}, el('a', { href: '/', dataset: { link: '' } }, '대시보드로 돌아가기')),
          status,
          el('p', {}, el('label', {}, '조회 지역 ', regionSelect.element), ' ', reload),
          el(
            'dl',
            {},
            el('dt', {}, '시장 model'),
            model,
            el('dt', {}, '현재 거주 지역'),
            residenceRegion,
            el('dt', {}, '조회 지역'),
            selectedRegionText,
            el('dt', {}, '현재 game day'),
            gameDay,
            el('dt', {}, '시장 월'),
            yearMonth,
            el('dt', {}, '가격 지수'),
            priceIndex,
            el('dt', {}, '임대료 지수'),
            rentIndex,
          ),
          el(
            'section',
            {},
            el('h2', {}, '현재 임대차 계약과 이사비'),
            leaseStatus,
            el(
              'dl',
              {},
              el('dt', {}, '이사 기능'),
              leaseCapability,
              el('dt', {}, '갱신 규칙'),
              leaseRenewalRule,
              el('dt', {}, '계약 기간 정책'),
              leaseLifecycleTerms,
              el('dt', {}, '임대차 보증금 자산'),
              leaseDepositAsset,
              el('dt', {}, '활성 계약'),
              activeLease,
              el('dt', {}, '현재 계약 기간'),
              leaseCurrentTerm,
              el('dt', {}, '자동갱신 안내'),
              leaseRenewalNotice,
              el('dt', {}, '계약 종료 검토'),
              leaseTerminationReview,
              el('dt', {}, '월세 규칙'),
              monthlyRentTerms,
              el('dt', {}, '월세 연체 총액'),
              leaseArrearTotal,
              el('dt', {}, '연체 조회 범위'),
              leaseArrearWindow,
            ),
            el('h3', {}, '지역별 이사비'),
            movingCosts.element,
            el('h3', {}, '활성 월세 연체'),
            leaseArrearList.element,
            arrearPaymentForm,
          ),
          moveSection,
          depositLoanSection,
          commandMessage,
          resultNodes.section,
          paymentResultSection,
          purchaseSection,
          propertySaleSection,
          el(
            'table',
            {},
            el('caption', {}, '현재 월 주거 매물'),
            el(
              'thead',
              {},
              el(
                'tr',
                {},
                el('th', { attrs: { scope: 'col' } }, '매물 ID'),
                el('th', { attrs: { scope: 'col' } }, '지역'),
                el('th', { attrs: { scope: 'col' } }, '주택 유형'),
                el('th', { attrs: { scope: 'col' } }, '전용면적'),
                el('th', { attrs: { scope: 'col' } }, '게시 기간'),
                el('th', { attrs: { scope: 'col' } }, '거래 조건'),
              ),
            ),
            listingBody,
          ),
        ),
      );

      const runReads = (): void => {
        listingsRequest.run();
        currentLeaseRequest.run();
        loanProductsRequest.run();
        holdingsRequest.run();
        propertySalesRequest.run();
      };
      const runSelectedPropertyTaxRead = (): void => {
        if (selectedPropertyTaxHoldingId.peek() !== '') propertyTaxEventsRequest.run();
      };

      h.useEffect(() => {
        renderHousing(
          listingsRequest.state.get(),
          gameReady.get(),
          summaryNodes,
          regionSelect,
          listingTable,
        );
      });
      h.useEffect(() => {
        renderCurrentLease(
          currentLeaseRequest.state.get(),
          gameReady.get(),
          leaseSummaryNodes,
          movingCosts,
        );
      });
      h.useEffect(() => {
        const pending = pendingRequest.get();
        const selected = leaseSelect.setListings(
          listings.get(),
          currentLease.get()?.leaseCapability,
          selectedLeaseKey.peek(),
          pending ?? pendingQuote.get(),
        );
        selectedLeaseKey.set(selected);
      });
      h.useEffect(() => {
        const state = loanProductsRequest.state.get();
        const items =
          state.status === 'success'
            ? state.value.products.filter(isHousingDepositLoanProduct)
            : [];
        const pending = pendingQuote.get();
        const selected = depositLoanProductSelect.setItems(
          items,
          pending?.productVersionId ?? selectedDepositLoanProductId.peek(),
        );
        selectedDepositLoanProductId.set(selected);
        if (pending !== undefined) depositLoanPrincipal.value = String(pending.principalKrw);
        syncDepositLoanPrincipalBounds(
          depositLoanPrincipal,
          items.find((product) => product.id === selected),
        );
      });
      h.useEffect(() => {
        const pending = pendingPurchase.get() ?? pendingMortgageQuote.get();
        const selected = saleSelect.setListings(
          listings.get(),
          selectedSaleListingId.peek(),
          pending,
        );
        selectedSaleListingId.set(selected);
      });
      h.useEffect(() => {
        const items = mortgageProducts.get();
        const pending = pendingMortgageQuote.get();
        const selected = mortgageProductSelect.setItems(
          items,
          pending?.productVersionId ?? selectedMortgageProductId.peek(),
        );
        selectedMortgageProductId.set(selected);
        if (pending !== undefined) mortgagePrincipal.value = String(pending.principalKrw);
        syncMortgagePrincipalBounds(
          mortgagePrincipal,
          items.find((product) => product.id === selected),
        );
      });
      h.useEffect(() =>
        renderPropertyHoldings(
          holdingsRequest.state.get(),
          holdingsStatus,
          purchaseCapability,
          propertyBookValue,
          propertyHoldings,
        ),
      );
      h.useEffect(() => {
        const items = activePropertyHoldings.get();
        const pendingHoldingId = pendingPropertySaleCreate.get()?.holdingId;
        const selectedSaleHolding = propertySaleHoldingSelect.setItems(
          items,
          selectedPropertySaleHoldingId.peek(),
          pendingHoldingId,
        );
        selectedPropertySaleHoldingId.set(selectedSaleHolding);
        if (pendingPropertySaleCreate.get() !== undefined) {
          propertySaleAskingPrice.value = String(pendingPropertySaleCreate.get()?.askingPriceKrw);
        }
        const previousTaxHolding = selectedPropertyTaxHoldingId.peek();
        const selectedTaxHolding = propertyTaxHoldingSelect.setItems(
          items,
          propertySaleOrders.get(),
          previousTaxHolding,
        );
        selectedPropertyTaxHoldingId.set(selectedTaxHolding);
        if (selectedTaxHolding !== '' && selectedTaxHolding !== previousTaxHolding) {
          propertyTaxEventsRequest.run();
        }
      });
      h.useEffect(() => {
        const items = propertySaleOrders.get();
        const pending = pendingPropertySaleReprice.get() ?? pendingPropertySaleCancel.get();
        const selected = propertySaleOrderSelect.setItems(
          items,
          selectedPropertySaleOrderId.peek(),
          pending?.orderId,
        );
        selectedPropertySaleOrderId.set(selected);
        if (pendingPropertySaleReprice.get() !== undefined) {
          propertySaleReprice.value = String(
            pendingPropertySaleReprice.get()?.request.askingPriceKrw,
          );
        }
        propertySaleOrderList.setItems(items);
        const previousTaxHolding = selectedPropertyTaxHoldingId.peek();
        const selectedTaxHolding = propertyTaxHoldingSelect.setItems(
          activePropertyHoldings.get(),
          items,
          previousTaxHolding,
        );
        selectedPropertyTaxHoldingId.set(selectedTaxHolding);
        if (selectedTaxHolding !== '' && selectedTaxHolding !== previousTaxHolding) {
          propertyTaxEventsRequest.run();
        }
      });
      h.useEffect(() => {
        const state = propertyTaxEventsRequest.state.get();
        propertyTaxEventList.setItems(state.status === 'success' ? state.value.items : []);
        propertyTaxFeedback.set(
          propertyTaxHistoryStatusText(state, selectedPropertyTaxHoldingId.get()),
        );
      });
      h.useEffect(() => {
        propertySaleResultText.textContent = propertySaleCommandResultText(
          propertySaleListingResult.get(),
          propertySaleCancellationResult.get(),
        );
      });
      h.useEffect(() => {
        const arrears = currentLease.get()?.activeArrears ?? [];
        const pending = pendingPayment.get();
        leaseArrearList.setItems(arrears);
        arrearSelect.setItems(arrears, pending?.arrearId);
        if (pending !== undefined) arrearAmount.value = String(pending.request.amountKrw);
        syncLeaseArrearAmountLimit(arrearSelect.element, arrearAmount, arrears);
      });
      h.useEffect(() => {
        renderMovePreview(
          snapshot.get(),
          currentLease.get(),
          selectedOffer.get(),
          selectedMovingCost.get(),
          pendingRequest.get(),
          previewNodes,
        );
      });
      h.useEffect(() => renderMoveResult(commandResult.get(), resultNodes));
      h.useEffect(() => renderDepositLoanQuote(quoteResult.get(), depositLoanQuoteNodes));
      h.useEffect(() => renderMortgageQuote(mortgageQuoteResult.get(), mortgageQuoteNodes));
      h.useEffect(() => renderPurchaseResult(purchaseResult.get(), purchaseResultNodes));
      h.useEffect(() =>
        renderLeaseArrearPaymentResult(paymentResult.get(), {
          section: paymentResultSection,
          arrear: paymentResultArrear,
          payment: paymentResultId,
          paid: paymentResultPaid,
          remaining: paymentResultRemaining,
        }),
      );
      h.useEffect(() => {
        const current = snapshot.get();
        pendingRequest.set(pendingForSnapshot(leaseRetries, current));
        pendingPayment.set(pendingPaymentForSnapshot(arrearPaymentRetries, current));
        pendingQuote.set(pendingQuoteForSnapshot(quoteRetries, current));
        pendingMortgageQuote.set(pendingMortgageQuoteForSnapshot(mortgageQuoteRetries, current));
        pendingPurchase.set(pendingPurchaseForSnapshot(purchaseRetries, current));
        pendingPropertySaleCreate.set(
          pendingPropertySaleCreateForSnapshot(propertySaleCreateRetries, current),
        );
        pendingPropertySaleReprice.set(
          pendingPropertySaleRepriceForSnapshot(propertySaleRepriceRetries, current),
        );
        pendingPropertySaleCancel.set(
          pendingPropertySaleCancelForSnapshot(propertySaleCancelRetries, current),
        );
      });

      h.bindText(commandMessage, () => commandFeedback.get());
      h.bindText(depositLoanStatus, () =>
        depositLoanStatusText(
          quoteFeedback.get(),
          loanProductsRequest.state.get(),
          pendingQuote.get(),
          selectedOffer.get(),
        ),
      );
      h.bindText(moveStatus, () =>
        moveAvailabilityText(
          snapshot.get(),
          currentLeaseRequest.state.get(),
          pendingRequest.get(),
          advancing.get(),
          ordering.get(),
          commandBusy.get(),
          selectedOffer.get(),
        ),
      );
      h.bindText(mortgageStatus, () =>
        mortgageStatusText(
          mortgageFeedback.get(),
          holdingsRequest.state.get(),
          pendingMortgageQuote.get(),
          selectedSaleOffer.get(),
        ),
      );
      h.bindText(purchaseStatus, () => purchaseFeedback.get());
      h.bindText(propertySaleStatus, () =>
        propertySaleStatusText(
          propertySaleFeedback.get(),
          propertySalesRequest.state.get(),
          pendingPropertySaleCreate.get(),
          pendingPropertySaleReprice.get(),
          pendingPropertySaleCancel.get(),
        ),
      );
      h.bindText(propertyTaxStatus, () => propertyTaxFeedback.get());
      h.bindAttribute(
        regionSelect.element,
        'disabled',
        () =>
          !gameReady.get() ||
          listingsRequest.state.get().status === 'loading' ||
          listingsRequest.state.get().status !== 'success',
      );
      h.bindAttribute(
        reload,
        'disabled',
        () =>
          !gameReady.get() ||
          listingsRequest.state.get().status === 'loading' ||
          currentLeaseRequest.state.get().status === 'loading',
      );
      h.bindAttribute(
        moveSection,
        'hidden',
        () =>
          currentLease.get()?.leaseCapability === 'unavailable' &&
          pendingRequest.get() === undefined,
      );
      h.bindAttribute(
        leaseSelect.element,
        'disabled',
        () =>
          pendingRequest.get() !== undefined ||
          pendingPayment.get() !== undefined ||
          pendingQuote.get() !== undefined ||
          pendingMortgageQuote.get() !== undefined ||
          pendingPurchase.get() !== undefined,
      );
      h.bindAttribute(moveSubmit, 'disabled', () => !canMove.get());
      h.bindText(moveSubmit, () =>
        pendingRequest.get() === undefined
          ? '선택한 임대차로 이사'
          : '같은 이사 명령 결과 다시 확인',
      );
      h.bindAttribute(
        depositLoanProductSelect.element,
        'disabled',
        () => pendingQuote.get() !== undefined || !canQuoteDepositLoan.get(),
      );
      h.bindAttribute(
        depositLoanPrincipal,
        'disabled',
        () => pendingQuote.get() !== undefined || !canQuoteDepositLoan.get(),
      );
      h.bindAttribute(depositLoanQuoteSubmit, 'disabled', () => !canQuoteDepositLoan.get());
      h.bindText(depositLoanQuoteSubmit, () =>
        pendingQuote.get() === undefined
          ? '전세자금대출 견적 받기'
          : '같은 전세자금대출 견적 다시 확인',
      );
      h.bindAttribute(financedMoveSubmit, 'disabled', () => !canMoveWithDepositLoan.get());
      h.bindText(financedMoveSubmit, () => {
        const pending = pendingRequest.get();
        return pending !== undefined && 'loanQuoteId' in pending
          ? '같은 대출 전세 입주 결과 다시 확인'
          : '견적 대출로 전세 입주';
      });
      h.bindAttribute(
        arrearSelect.element,
        'disabled',
        () => !canPayArrear.get() || pendingPayment.get() !== undefined,
      );
      h.bindAttribute(
        arrearAmount,
        'disabled',
        () => !canPayArrear.get() || pendingPayment.get() !== undefined,
      );
      h.bindAttribute(partialPaymentSubmit, 'disabled', () => !canPayArrear.get());
      h.bindAttribute(
        fullPayment,
        'disabled',
        () => !canPayArrear.get() || pendingPayment.get() !== undefined,
      );
      h.bindText(partialPaymentSubmit, () =>
        pendingPayment.get() === undefined ? '입력 금액 상환' : '같은 상환 명령 결과 다시 확인',
      );
      h.bindAttribute(
        saleSelect.element,
        'disabled',
        () => pendingMortgageQuote.get() !== undefined || pendingPurchase.get() !== undefined,
      );
      h.bindAttribute(
        mortgageProductSelect.element,
        'disabled',
        () => pendingMortgageQuote.get() !== undefined || !canQuoteMortgage.get(),
      );
      h.bindAttribute(
        mortgagePrincipal,
        'disabled',
        () => pendingMortgageQuote.get() !== undefined || !canQuoteMortgage.get(),
      );
      h.bindAttribute(mortgageQuoteSubmit, 'disabled', () => !canQuoteMortgage.get());
      h.bindText(mortgageQuoteSubmit, () =>
        pendingMortgageQuote.get() === undefined
          ? '주택담보대출 견적 받기'
          : '같은 주택담보대출 견적 다시 확인',
      );
      h.bindAttribute(cashPurchase, 'disabled', () => !canPurchaseCash.get());
      h.bindText(cashPurchase, () => {
        const pending = pendingPurchase.get();
        return pending?.mortgageQuoteId === null
          ? '같은 현금 매수 결과 다시 확인'
          : '선택한 매물 현금 매수';
      });
      h.bindAttribute(financedPurchase, 'disabled', () => !canPurchaseWithMortgage.get());
      h.bindText(financedPurchase, () => {
        const pending = pendingPurchase.get();
        return pending !== undefined && pending.mortgageQuoteId !== null
          ? '같은 주담대 매수 결과 다시 확인'
          : '견적 주담대로 매수';
      });
      h.bindAttribute(
        propertySaleHoldingSelect.element,
        'disabled',
        () =>
          propertySaleBaseBlocked() ||
          pendingPropertySaleCreate.get() !== undefined ||
          pendingPropertySaleReprice.get() !== undefined ||
          pendingPropertySaleCancel.get() !== undefined,
      );
      h.bindAttribute(
        propertySaleAskingPrice,
        'disabled',
        () =>
          propertySaleBaseBlocked() ||
          pendingPropertySaleCreate.get() !== undefined ||
          pendingPropertySaleReprice.get() !== undefined ||
          pendingPropertySaleCancel.get() !== undefined,
      );
      h.bindAttribute(propertySaleCreateSubmit, 'disabled', () => !canCreatePropertySale.get());
      h.bindText(propertySaleCreateSubmit, () =>
        pendingPropertySaleCreate.get() === undefined
          ? '매도 주문 만들기'
          : '같은 매도 주문 결과 다시 확인',
      );
      h.bindAttribute(
        propertySaleOrderSelect.element,
        'disabled',
        () =>
          propertySaleBaseBlocked() ||
          pendingPropertySaleCreate.get() !== undefined ||
          pendingPropertySaleReprice.get() !== undefined ||
          pendingPropertySaleCancel.get() !== undefined,
      );
      h.bindAttribute(
        propertySaleReprice,
        'disabled',
        () =>
          propertySaleBaseBlocked() ||
          pendingPropertySaleCreate.get() !== undefined ||
          pendingPropertySaleReprice.get() !== undefined ||
          pendingPropertySaleCancel.get() !== undefined,
      );
      h.bindAttribute(propertySaleRepriceSubmit, 'disabled', () => !canRepricePropertySale.get());
      h.bindText(propertySaleRepriceSubmit, () =>
        pendingPropertySaleReprice.get() === undefined
          ? '주문가 변경'
          : '같은 주문가 변경 결과 다시 확인',
      );
      h.bindAttribute(propertySaleCancel, 'disabled', () => !canCancelPropertySale.get());
      h.bindText(propertySaleCancel, () =>
        pendingPropertySaleCancel.get() === undefined
          ? '선택한 주문 취소'
          : '같은 주문 취소 결과 다시 확인',
      );
      h.bindAttribute(
        propertyTaxHoldingSelect.element,
        'disabled',
        () =>
          !gameReady.get() ||
          propertyTaxEventsRequest.state.get().status === 'loading' ||
          selectedPropertyTaxHoldingId.get() === '',
      );
      h.bindAttribute(
        propertyTaxReload,
        'disabled',
        () =>
          !gameReady.get() ||
          propertyTaxEventsRequest.state.get().status === 'loading' ||
          selectedPropertyTaxHoldingId.get() === '',
      );

      h.useEventListener(regionSelect.element, 'change', () => {
        const state = listingsRequest.state.peek();
        if (state.status !== 'success') return;
        const region = state.value.regions.find(
          (candidate) => candidate.regionKey === regionSelect.element.value,
        );
        if (region === undefined) return;
        selectedRegion.set(region.regionKey);
        selectedLeaseKey.set('');
        selectedSaleListingId.set('');
        if (pendingMortgageQuote.peek() === undefined) {
          mortgageQuoteResult.set(undefined);
          mortgageFeedback.set('');
        }
        listingsRequest.run();
      });
      h.useEventListener(leaseSelect.element, 'change', () => {
        selectedLeaseKey.set(leaseSelect.element.value);
        if (pendingQuote.peek() === undefined) {
          quoteResult.set(undefined);
          quoteFeedback.set('');
        }
      });
      h.useEventListener(depositLoanProductSelect.element, 'change', () => {
        selectedDepositLoanProductId.set(depositLoanProductSelect.element.value);
        quoteResult.set(undefined);
        quoteFeedback.set('');
        syncDepositLoanPrincipalBounds(
          depositLoanPrincipal,
          depositLoanProducts
            .peek()
            .find((product) => product.id === depositLoanProductSelect.element.value),
        );
      });
      h.useEventListener(depositLoanPrincipal, 'input', () => {
        if (pendingQuote.peek() !== undefined) return;
        quoteResult.set(undefined);
        quoteFeedback.set('');
      });
      h.useEventListener(arrearSelect.element, 'change', () => {
        syncLeaseArrearAmountLimit(
          arrearSelect.element,
          arrearAmount,
          currentLease.peek()?.activeArrears ?? [],
        );
      });
      h.useEventListener(saleSelect.element, 'change', () => {
        selectedSaleListingId.set(saleSelect.element.value);
        if (pendingMortgageQuote.peek() === undefined) {
          mortgageQuoteResult.set(undefined);
          mortgageFeedback.set('');
        }
      });
      h.useEventListener(mortgageProductSelect.element, 'change', () => {
        selectedMortgageProductId.set(mortgageProductSelect.element.value);
        mortgageQuoteResult.set(undefined);
        mortgageFeedback.set('');
        syncMortgagePrincipalBounds(
          mortgagePrincipal,
          mortgageProducts
            .peek()
            .find((product) => product.id === mortgageProductSelect.element.value),
        );
      });
      h.useEventListener(mortgagePrincipal, 'input', () => {
        if (pendingMortgageQuote.peek() !== undefined) return;
        mortgageQuoteResult.set(undefined);
        mortgageFeedback.set('');
      });
      h.useEventListener(reload, 'click', runReads);
      h.useEventListener(moveForm, 'submit', (event) => {
        event.preventDefault();
        void submitMove().catch((error: unknown) => {
          commandFeedback.set(housingCommandErrorText(error));
        });
      });
      h.useEventListener(depositLoanQuoteForm, 'submit', (event) => {
        event.preventDefault();
        void submitDepositLoanQuote().catch((error: unknown) => {
          quoteFeedback.set(housingDepositLoanErrorText(error, 'quote'));
        });
      });
      h.useEventListener(financedMoveSubmit, 'click', () => {
        void submitFinancedMove().catch((error: unknown) => {
          commandFeedback.set(housingDepositLoanErrorText(error, 'move'));
        });
      });
      h.useEventListener(arrearPaymentForm, 'submit', (event) => {
        event.preventDefault();
        void submitArrearPayment(false).catch((error: unknown) => {
          commandFeedback.set(housingArrearPaymentErrorText(error));
        });
      });
      h.useEventListener(fullPayment, 'click', () => {
        void submitArrearPayment(true).catch((error: unknown) => {
          commandFeedback.set(housingArrearPaymentErrorText(error));
        });
      });
      h.useEventListener(mortgageQuoteForm, 'submit', (event) => {
        event.preventDefault();
        void submitMortgageQuote().catch((error: unknown) => {
          mortgageFeedback.set(housingMortgageErrorText(error, 'quote'));
        });
      });
      h.useEventListener(cashPurchase, 'click', () => {
        void submitPurchase(false).catch((error: unknown) => {
          purchaseFeedback.set(housingMortgageErrorText(error, 'purchase'));
        });
      });
      h.useEventListener(financedPurchase, 'click', () => {
        void submitPurchase(true).catch((error: unknown) => {
          purchaseFeedback.set(housingMortgageErrorText(error, 'purchase'));
        });
      });
      h.useEventListener(propertySaleHoldingSelect.element, 'change', () => {
        selectedPropertySaleHoldingId.set(propertySaleHoldingSelect.element.value);
        if (pendingPropertySaleCreate.peek() === undefined) {
          propertySaleListingResult.set(undefined);
          propertySaleCancellationResult.set(undefined);
          propertySaleFeedback.set('');
        }
      });
      h.useEventListener(propertySaleOrderSelect.element, 'change', () => {
        selectedPropertySaleOrderId.set(propertySaleOrderSelect.element.value);
        if (
          pendingPropertySaleReprice.peek() === undefined &&
          pendingPropertySaleCancel.peek() === undefined
        ) {
          propertySaleListingResult.set(undefined);
          propertySaleCancellationResult.set(undefined);
          propertySaleFeedback.set('');
        }
      });
      h.useEventListener(propertySaleAskingPrice, 'input', () => {
        if (pendingPropertySaleCreate.peek() === undefined) propertySaleFeedback.set('');
      });
      h.useEventListener(propertySaleReprice, 'input', () => {
        if (pendingPropertySaleReprice.peek() === undefined) propertySaleFeedback.set('');
      });
      h.useEventListener(propertyTaxHoldingSelect.element, 'change', () => {
        selectedPropertyTaxHoldingId.set(propertyTaxHoldingSelect.element.value);
        propertyTaxEventsRequest.run();
      });
      h.useEventListener(propertyTaxReload, 'click', () => propertyTaxEventsRequest.run());
      h.useEventListener(propertySaleCreateForm, 'submit', (event) => {
        event.preventDefault();
        void submitPropertySaleCreate().catch((error: unknown) => {
          propertySaleFeedback.set(housingPropertySaleErrorText(error, 'create'));
        });
      });
      h.useEventListener(propertySaleRepriceForm, 'submit', (event) => {
        event.preventDefault();
        void submitPropertySaleReprice().catch((error: unknown) => {
          propertySaleFeedback.set(housingPropertySaleErrorText(error, 'reprice'));
        });
      });
      h.useEventListener(propertySaleCancel, 'click', () => {
        void submitPropertySaleCancel().catch((error: unknown) => {
          propertySaleFeedback.set(housingPropertySaleErrorText(error, 'cancel'));
        });
      });
      h.useWatch(snapshot, (next, previous) => {
        if (!hasCurrentCharacter(next)) {
          listingsRequest.cancel();
          currentLeaseRequest.cancel();
          loanProductsRequest.cancel();
          holdingsRequest.cancel();
          propertySalesRequest.cancel();
          propertyTaxEventsRequest.cancel();
          return;
        }
        if (runRevisionChanged(next, previous)) {
          selectedRegion.set(undefined);
          selectedLeaseKey.set('');
          selectedSaleListingId.set('');
          commandResult.set(undefined);
          quoteResult.set(undefined);
          mortgageQuoteResult.set(undefined);
          purchaseResult.set(undefined);
          paymentResult.set(undefined);
          commandFeedback.set('');
          quoteFeedback.set('');
          mortgageFeedback.set('');
          purchaseFeedback.set('');
          selectedPropertySaleHoldingId.set('');
          selectedPropertySaleOrderId.set('');
          selectedPropertyTaxHoldingId.set('');
          propertySaleListingResult.set(undefined);
          propertySaleCancellationResult.set(undefined);
          propertySaleFeedback.set('');
          propertyTaxFeedback.set('');
        }
        if (housingReadCursorChanged(next, previous)) {
          runReads();
          runSelectedPropertyTaxRead();
        }
      });

      if (gameReady.peek()) runReads();

      async function submitMove(): Promise<void> {
        const current = commandSnapshot(deps);
        const selection = selectMoveRequest(leaseRetries, current, selectedLeaseKey.peek());
        const { request } = selection;

        commandBusy.set(true);
        commandFeedback.set(moveSubmittingText(selection.retry));
        deps.store.set(paths.gameOrdering, true);
        try {
          const response = await deps.api.startLease(request);
          leaseRetries.complete(request);
          pendingRequest.set(undefined);
          deps.snapshots.apply(response.snapshot);
          commandResult.set(response.result);
          commandFeedback.set(moveSuccessText(response.replayed));
          runReads();
        } catch (error) {
          leaseRetries.fail(request, error);
          pendingRequest.set(leaseRetries.pending(request.expectedRunRevision));
          throw error;
        } finally {
          deps.store.set(paths.gameOrdering, false);
          commandBusy.set(false);
        }
      }

      async function submitDepositLoanQuote(): Promise<void> {
        const current = commandSnapshot(deps);
        const selection = selectDepositLoanQuoteRequest(
          quoteRetries,
          current,
          selectedListing.peek(),
          selectedOffer.peek(),
          selectedDepositLoanProduct.peek(),
          depositLoanPrincipal.value,
        );
        const { request } = selection;

        commandBusy.set(true);
        quoteFeedback.set(
          selection.retry
            ? '같은 전세자금대출 견적 결과를 다시 확인하는 중입니다.'
            : '전세자금대출 한도와 상환여력을 심사하는 중입니다.',
        );
        deps.store.set(paths.gameOrdering, true);
        try {
          const response = await deps.api.quoteLeaseDepositLoan(request);
          quoteRetries.complete(request);
          pendingQuote.set(undefined);
          deps.snapshots.apply(response.snapshot);
          quoteResult.set(response.result);
          quoteFeedback.set(
            response.replayed
              ? '이전에 만든 같은 전세자금대출 견적을 확인했습니다.'
              : '전세자금대출 견적이 확정되었습니다.',
          );
        } catch (error) {
          quoteRetries.fail(request, error);
          pendingQuote.set(quoteRetries.pending(request.expectedRunRevision));
          throw error;
        } finally {
          deps.store.set(paths.gameOrdering, false);
          commandBusy.set(false);
        }
      }

      async function submitFinancedMove(): Promise<void> {
        const current = commandSnapshot(deps);
        const selection = selectFinancedMoveRequest(
          leaseRetries,
          current,
          quoteResult.peek(),
          selectedListing.peek(),
          selectedOffer.peek(),
        );
        const { request } = selection;

        commandBusy.set(true);
        commandFeedback.set(moveSubmittingText(selection.retry));
        deps.store.set(paths.gameOrdering, true);
        try {
          const response = await deps.api.startLease(request);
          leaseRetries.complete(request);
          pendingRequest.set(undefined);
          deps.snapshots.apply(response.snapshot);
          commandResult.set(response.result);
          commandFeedback.set(moveSuccessText(response.replayed));
          runReads();
        } catch (error) {
          leaseRetries.fail(request, error);
          pendingRequest.set(leaseRetries.pending(request.expectedRunRevision));
          throw error;
        } finally {
          deps.store.set(paths.gameOrdering, false);
          commandBusy.set(false);
        }
      }

      async function submitMortgageQuote(): Promise<void> {
        const current = commandSnapshot(deps);
        const selection = selectMortgageQuoteRequest(
          mortgageQuoteRetries,
          current,
          selectedSaleListing.peek(),
          selectedMortgageProduct.peek(),
          mortgagePrincipal.value,
        );
        const { request } = selection;

        commandBusy.set(true);
        mortgageFeedback.set(
          selection.retry
            ? '같은 주택담보대출 견적 결과를 다시 확인하는 중입니다.'
            : '서버에서 LTV·DSR·자기자금을 심사하는 중입니다.',
        );
        deps.store.set(paths.gameOrdering, true);
        try {
          const response = await deps.api.quoteMortgage(request);
          mortgageQuoteRetries.complete(request);
          pendingMortgageQuote.set(undefined);
          deps.snapshots.apply(response.snapshot);
          mortgageQuoteResult.set(response.result);
          mortgageFeedback.set(
            response.replayed
              ? '이전에 만든 같은 주택담보대출 견적을 확인했습니다.'
              : '주택담보대출 견적이 확정되었습니다.',
          );
        } catch (error) {
          mortgageQuoteRetries.fail(request, error);
          pendingMortgageQuote.set(mortgageQuoteRetries.pending(request.expectedRunRevision));
          throw error;
        } finally {
          deps.store.set(paths.gameOrdering, false);
          commandBusy.set(false);
        }
      }

      async function submitPurchase(useMortgage: boolean): Promise<void> {
        const current = commandSnapshot(deps);
        const selection = selectPurchaseRequest(
          purchaseRetries,
          current,
          selectedSaleListing.peek(),
          mortgageQuoteForPurchase(useMortgage, mortgageQuoteResult.peek()),
          useMortgage,
        );
        const { request } = selection;

        commandBusy.set(true);
        purchaseFeedback.set(purchaseSubmittingText(selection.retry, useMortgage));
        deps.store.set(paths.gameOrdering, true);
        try {
          const response = await deps.api.purchase(request);
          purchaseRetries.complete(request);
          pendingPurchase.set(undefined);
          deps.snapshots.apply(response.snapshot);
          purchaseResult.set(response.result);
          mortgageQuoteResult.set(undefined);
          purchaseFeedback.set(purchaseSuccessText(response.replayed));
          runReads();
        } catch (error) {
          purchaseRetries.fail(request, error);
          pendingPurchase.set(purchaseRetries.pending(request.expectedRunRevision));
          throw error;
        } finally {
          deps.store.set(paths.gameOrdering, false);
          commandBusy.set(false);
        }
      }

      async function submitPropertySaleCreate(): Promise<void> {
        const current = commandSnapshot(deps);
        const selection = selectPropertySaleCreateRequest(
          propertySaleCreateRetries,
          current,
          selectedPropertySaleHoldingId.peek(),
          propertySaleAskingPrice.value,
        );
        const { request } = selection;

        commandBusy.set(true);
        propertySaleFeedback.set(
          selection.retry
            ? '같은 매도 주문 결과를 다시 확인하는 중입니다.'
            : '기준가와 후보 체결일을 확정해 매도 주문을 만드는 중입니다.',
        );
        deps.store.set(paths.gameOrdering, true);
        try {
          const response = await deps.api.createPropertySaleOrder(request);
          propertySaleCreateRetries.complete(request);
          pendingPropertySaleCreate.set(undefined);
          deps.snapshots.apply(response.snapshot);
          selectedPropertySaleOrderId.set(response.result.orderId);
          propertySaleListingResult.set(response.result);
          propertySaleCancellationResult.set(undefined);
          propertySaleFeedback.set(propertySaleListingSuccessText(response.replayed, 'create'));
          propertySaleAskingPrice.value = '';
          runReads();
        } catch (error) {
          propertySaleCreateRetries.fail(request, error);
          pendingPropertySaleCreate.set(
            propertySaleCreateRetries.pending(request.expectedRunRevision),
          );
          throw error;
        } finally {
          deps.store.set(paths.gameOrdering, false);
          commandBusy.set(false);
        }
      }

      async function submitPropertySaleReprice(): Promise<void> {
        const current = commandSnapshot(deps);
        const selection = selectPropertySaleRepriceCommand(
          propertySaleRepriceRetries,
          current,
          selectedPropertySaleOrderId.peek(),
          propertySaleReprice.value,
          propertySaleOrders.peek(),
        );
        const { command } = selection;

        commandBusy.set(true);
        propertySaleFeedback.set(
          selection.retry
            ? '같은 주문가 변경 결과를 다시 확인하는 중입니다.'
            : '새 기준가와 후보 체결일로 주문을 변경하는 중입니다.',
        );
        deps.store.set(paths.gameOrdering, true);
        try {
          const response = await deps.api.repricePropertySaleOrder(
            command.orderId,
            command.request,
          );
          propertySaleRepriceRetries.complete(command);
          pendingPropertySaleReprice.set(undefined);
          deps.snapshots.apply(response.snapshot);
          selectedPropertySaleOrderId.set(response.result.orderId);
          propertySaleListingResult.set(response.result);
          propertySaleCancellationResult.set(undefined);
          propertySaleFeedback.set(propertySaleListingSuccessText(response.replayed, 'reprice'));
          propertySaleReprice.value = '';
          runReads();
        } catch (error) {
          propertySaleRepriceRetries.fail(command, error);
          pendingPropertySaleReprice.set(
            propertySaleRepriceRetries.pending(command.request.expectedRunRevision),
          );
          throw error;
        } finally {
          deps.store.set(paths.gameOrdering, false);
          commandBusy.set(false);
        }
      }

      async function submitPropertySaleCancel(): Promise<void> {
        const current = commandSnapshot(deps);
        const selection = selectPropertySaleCancelCommand(
          propertySaleCancelRetries,
          current,
          selectedPropertySaleOrderId.peek(),
          propertySaleOrders.peek(),
        );
        const { command } = selection;

        commandBusy.set(true);
        propertySaleFeedback.set(
          selection.retry
            ? '같은 매도 주문 취소 결과를 다시 확인하는 중입니다.'
            : '매도 주문을 취소하는 중입니다.',
        );
        deps.store.set(paths.gameOrdering, true);
        try {
          const response = await deps.api.cancelPropertySaleOrder(command.orderId, command.request);
          propertySaleCancelRetries.complete(command);
          pendingPropertySaleCancel.set(undefined);
          deps.snapshots.apply(response.snapshot);
          selectedPropertySaleOrderId.set(response.result.orderId);
          propertySaleListingResult.set(undefined);
          propertySaleCancellationResult.set(response.result);
          propertySaleFeedback.set(
            response.replayed
              ? '이전에 완료된 같은 매도 주문 취소 결과를 확인했습니다.'
              : '매도 주문을 취소했습니다.',
          );
          runReads();
        } catch (error) {
          propertySaleCancelRetries.fail(command, error);
          pendingPropertySaleCancel.set(
            propertySaleCancelRetries.pending(command.request.expectedRunRevision),
          );
          throw error;
        } finally {
          deps.store.set(paths.gameOrdering, false);
          commandBusy.set(false);
        }
      }

      async function submitArrearPayment(payFull: boolean): Promise<void> {
        const current = commandSnapshot(deps);
        const selection = selectLeaseArrearPayment(
          arrearPaymentRetries,
          current,
          arrearSelect.element,
          arrearAmount,
          currentLease.peek()?.activeArrears ?? [],
          payFull,
        );
        const { command } = selection;

        commandBusy.set(true);
        commandFeedback.set(
          selection.retry
            ? '같은 월세 연체 상환 결과를 다시 확인하는 중입니다.'
            : '월세 연체를 상환하는 중입니다.',
        );
        deps.store.set(paths.gameOrdering, true);
        try {
          const response = await deps.api.payLeaseArrear(command.arrearId, command.request);
          arrearPaymentRetries.complete(command);
          pendingPayment.set(undefined);
          deps.snapshots.apply(response.snapshot);
          paymentResult.set(response.result);
          commandFeedback.set(
            response.replayed
              ? '이미 완료된 같은 월세 연체 상환 결과를 확인했습니다.'
              : `${formatWon(response.result.paidKrw)}을 상환했습니다.`,
          );
          arrearAmount.value = '';
          runReads();
        } catch (error) {
          arrearPaymentRetries.fail(command, error);
          pendingPayment.set(arrearPaymentRetries.pending(command.request.expectedRunRevision));
          throw error;
        } finally {
          deps.store.set(paths.gameOrdering, false);
          commandBusy.set(false);
        }
      }
    },
    unmount() {},
  });
}

function renderHousing(
  state: AsyncState<HousingListingsResponse>,
  gameReady: boolean,
  nodes: HousingSummaryNodes,
  regionSelect: FixedRegionSelect,
  listings: FixedListingTable,
): void {
  if (!gameReady) {
    updateSummary(nodes, undefined);
    regionSelect.setRegions([], undefined);
    listings.setListings([]);
    nodes.status.textContent = '주거 시장을 보려면 현재 캐릭터가 필요합니다.';
    return;
  }
  if (state.status === 'success') {
    updateSummary(nodes, state.value);
    regionSelect.setRegions(state.value.regions, state.value.selectedRegionKey);
    listings.setListings(state.value.listings);
    nodes.status.textContent = housingStatusText(state.value);
    return;
  }

  updateSummary(nodes, undefined);
  regionSelect.setRegions([], undefined);
  listings.setListings([]);
  nodes.status.textContent =
    state.status === 'error' ? housingQueryErrorText(state.error) : '주거 시장을 불러오는 중…';
}

function renderCurrentLease(
  state: AsyncState<HousingCurrentLeaseResponse>,
  gameReady: boolean,
  nodes: LeaseSummaryNodes,
  movingCosts: FixedMovingCostList,
): void {
  if (!gameReady) {
    clearLeaseSummary(nodes, movingCosts);
    nodes.status.textContent = '현재 임대차 계약을 보려면 캐릭터가 필요합니다.';
    return;
  }
  if (state.status !== 'success') {
    clearLeaseSummary(nodes, movingCosts);
    nodes.status.textContent =
      state.status === 'error'
        ? housingQueryErrorText(state.error)
        : '현재 임대차 계약을 불러오는 중…';
    return;
  }

  const response = state.value;
  nodes.capability.textContent = leaseCapabilityText(response.leaseCapability);
  nodes.renewalRule.textContent = leaseRenewalRuleText(response.renewalRule);
  nodes.lifecycleTerms.textContent = leaseLifecycleTermsText(response.leaseLifecycleTerms);
  nodes.depositAsset.textContent = formatWon(response.tenantLeaseDepositKrw);
  nodes.activeLease.textContent = activeLeaseText(response.activeLease);
  nodes.currentTerm.textContent = currentLeaseTermText(response.activeLease);
  nodes.renewalNotice.textContent = leaseRenewalNoticeText(response.activeLease);
  nodes.terminationReview.textContent = leaseTerminationReviewText(response.activeLease);
  nodes.monthlyRentTerms.textContent =
    response.monthlyRentTerms === null ? '-' : '다음 시장 월 1일 전액 청구 · 수동 상환만 가능';
  nodes.arrearTotal.textContent = formatWon(response.totalLeaseArrearKrw);
  nodes.arrearWindow.textContent = response.hasMoreActiveArrears
    ? '가장 오래된 20건을 표시합니다.'
    : '현재 활성 연체를 모두 표시합니다.';
  movingCosts.setItems(response.movingCosts);
  nodes.status.textContent =
    response.leaseCapability !== 'unavailable'
      ? '현재 계약과 서버가 정한 지역별 이사비입니다.'
      : '매물 조회는 가능하지만 이 run의 부동산 model에는 임대차 기능이 없습니다.';
}

function clearLeaseSummary(nodes: LeaseSummaryNodes, movingCosts: FixedMovingCostList): void {
  nodes.capability.textContent = '-';
  nodes.renewalRule.textContent = '-';
  nodes.lifecycleTerms.textContent = '-';
  nodes.depositAsset.textContent = '-';
  nodes.activeLease.textContent = '-';
  nodes.currentTerm.textContent = '-';
  nodes.renewalNotice.textContent = '-';
  nodes.terminationReview.textContent = '-';
  nodes.monthlyRentTerms.textContent = '-';
  nodes.arrearTotal.textContent = '-';
  nodes.arrearWindow.textContent = '-';
  movingCosts.setItems([]);
}

function renderMovePreview(
  snapshot: GameSnapshot | undefined,
  currentLease: HousingCurrentLeaseResponse | undefined,
  offer: TenantHousingOffer | undefined,
  movingCost: HousingMovingCost | undefined,
  pending: HousingLeaseRequest | undefined,
  nodes: MovePreviewNodes,
): void {
  const wallet = snapshot?.cashKrw;
  const returnedDeposit = currentLease?.tenantLeaseDepositKrw;
  const linkedLoanId = currentLease?.activeLease?.depositLoanId;
  const repaidDepositLoan =
    linkedLoanId === null || linkedLoanId === undefined
      ? 0
      : (snapshot?.life.activeLoans.find((loan) => loan.id === linkedLoanId)
          ?.remainingPrincipalKrw ?? 0);
  nodes.wallet.textContent = moneyOrDash(wallet);
  nodes.returnedDeposit.textContent = moneyOrDash(returnedDeposit);
  nodes.repaidDepositLoan.textContent =
    returnedDeposit === undefined ? '-' : formatWon(repaidDepositLoan);
  nodes.available.textContent =
    wallet === undefined || returnedDeposit === undefined
      ? '-'
      : formatWon(BigInt(wallet) + BigInt(returnedDeposit) - BigInt(repaidDepositLoan));
  nodes.newDeposit.textContent = moneyOrDash(offer?.depositKrw);
  nodes.monthlyRent.textContent =
    offer === undefined
      ? '-'
      : offer.kind === 'monthlyRent'
        ? formatWon(offer.monthlyRentKrw)
        : '없음';
  nodes.movingCost.textContent = moneyOrDash(movingCost?.movingCostKrw);
  nodes.required.textContent =
    offer === undefined || movingCost === undefined
      ? pending === undefined
        ? '-'
        : '원래 요청 body로 결과 확인 대기'
      : formatWon(BigInt(offer.depositKrw) + BigInt(movingCost.movingCostKrw));
}

function renderMoveResult(result: HousingLeaseResult | undefined, nodes: MoveResultNodes): void {
  nodes.section.hidden = result === undefined;
  nodes.lease.textContent =
    result === undefined
      ? ''
      : `#${result.leaseId}, 매물 #${result.listingId}, game day ${result.effectiveFromGameDay}`;
  nodes.deposit.textContent = result === undefined ? '' : formatWon(result.depositKrw);
  nodes.monthlyRent.textContent =
    result === undefined
      ? ''
      : result.monthlyRentKrw === null
        ? '없음'
        : formatWon(result.monthlyRentKrw);
  nodes.returnedDeposit.textContent =
    result === undefined ? '' : formatWon(result.returnedDepositKrw);
  nodes.movingCost.textContent = result === undefined ? '' : formatWon(result.movingCostKrw);
  nodes.walletDelta.textContent = result === undefined ? '' : formatWon(result.walletDeltaKrw);
  const executed = result?.depositLoanExecution;
  nodes.depositLoan.textContent =
    executed === undefined || executed === null
      ? '없음'
      : `계약 #${executed.loanId} · 견적 #${executed.quoteId} · ${formatWon(executed.principalKrw)} · 연 ${executed.annualRateBp.toLocaleString('ko-KR')}bp · 만기 game day ${executed.maturityGameDay}`;
  const repaid = result?.repaidDepositLoan;
  nodes.repaidDepositLoan.textContent =
    repaid === undefined || repaid === null
      ? '없음'
      : `계약 #${repaid.loanId} · 지급 #${repaid.paymentId} · ${formatWon(repaid.principalKrw)}`;
}

function renderDepositLoanQuote(
  result: HousingLeaseDepositLoanQuoteResult | undefined,
  nodes: DepositLoanQuoteNodes,
): void {
  nodes.section.hidden = result === undefined;
  if (result === undefined) {
    nodes.decision.textContent = '';
    nodes.requested.textContent = '';
    nodes.collateral.textContent = '';
    nodes.income.textContent = '';
    nodes.affordability.textContent = '';
    nodes.terms.textContent = '';
    nodes.replacement.textContent = '';
    nodes.balances.textContent = '';
    return;
  }
  nodes.decision.textContent = `${depositLoanDecisionText(result.decisionCode)} · ${result.decisionReasons.map(depositLoanDecisionReasonText).join(', ')} · 견적 #${result.quoteId} · game day ${result.expiresGameDay}까지`;
  nodes.requested.textContent = `매물 #${result.listingId} · 보증금 ${formatWon(result.depositKrw)} · 요청 ${formatWon(result.requestedPrincipalKrw)} · 상품 #${result.productVersionId}`;
  nodes.collateral.textContent = `${result.fundingLimitPpm.toLocaleString('ko-KR')}ppm · 최대 ${formatWon(result.maximumFundingKrw)}`;
  nodes.income.textContent =
    result.verifiedAnnualIncomeKrw === null
      ? '검증 가능한 연소득 없음'
      : `${formatWon(result.verifiedAnnualIncomeKrw)} · 재직 계약`;
  const affordability = result.affordability;
  nodes.affordability.textContent =
    affordability === null
      ? '심사 순서상 산정하지 않음'
      : `연간 부담 ${formatWon(affordability.numeratorKrw)} / 소득 ${formatWon(affordability.denominatorKrw)} · ${affordability.ratioPpm.toLocaleString('ko-KR')}ppm (한도 ${affordability.limitPpm.toLocaleString('ko-KR')}ppm) · 법정 DSR 적용 아님`;
  const first = result.quotedTerms.firstInstallment;
  nodes.terms.textContent = `연 ${result.quotedTerms.annualRateBp.toLocaleString('ko-KR')}bp · ${result.quotedTerms.termMonths.toLocaleString('ko-KR')}개월 · 첫 납입 game day ${first.dueGameDay}, ${formatWon(first.totalKrw)} (원금 ${formatWon(first.principalKrw)}, 이자 ${formatWon(first.interestKrw)}, 비용 ${formatWon(first.feeKrw)})`;
  nodes.replacement.textContent =
    result.replacedLoanId === null
      ? '대체할 기존 전세대출 없음'
      : `계약 #${result.replacedLoanId} · 반환 보증금에서 ${formatWon(result.replacedLoanPrincipalKrw)} 전액상환`;
  nodes.balances.textContent = `실행 전 ${formatWon(result.existingLoanBalanceKrw)} · 실행 후 ${formatWon(result.postExecutionBalanceKrw)}`;
}

function renderPropertyHoldings(
  state: AsyncState<HousingPropertyHoldingsResponse>,
  status: HTMLElement,
  capability: HTMLElement,
  bookValue: HTMLElement,
  list: FixedPropertyHoldingList,
): void {
  if (state.status === 'success') {
    status.textContent =
      state.value.holdings.length === 0
        ? '현재 보유주택이 없습니다.'
        : '현재 run의 보유주택과 연결 주택담보대출입니다.';
    capability.textContent =
      state.value.purchaseCapability === 'ownerOccupiedSingleHome'
        ? `실거주 1주택 · 최대 ${state.value.maximumActiveHoldings}건`
        : '이 run에서는 주택 매수를 사용할 수 없음';
    bookValue.textContent = formatWon(state.value.totalPropertyBookValueKrw);
    list.setItems(state.value.holdings);
    return;
  }
  status.textContent =
    state.status === 'error' ? housingQueryErrorText(state.error) : '보유주택을 불러오는 중입니다.';
  capability.textContent = '-';
  bookValue.textContent = '-';
  list.setItems([]);
}

function renderMortgageQuote(
  result: HousingMortgageQuoteResult | undefined,
  nodes: MortgageQuoteNodes,
): void {
  nodes.section.hidden = result === undefined;
  if (result === undefined) {
    nodes.decision.textContent = '';
    nodes.purchase.textContent = '';
    nodes.collateral.textContent = '';
    nodes.ltv.textContent = '';
    nodes.income.textContent = '';
    nodes.dsr.textContent = '';
    nodes.ownFunds.textContent = '';
    nodes.terms.textContent = '';
    nodes.leaseExit.textContent = '';
    return;
  }
  nodes.decision.textContent = `${mortgageDecisionText(result.decisionCode)} · ${result.decisionReasons.map(mortgageDecisionReasonText).join(', ')} · 견적 #${result.quoteId} · game day ${result.expiresGameDay}까지`;
  nodes.purchase.textContent = `매물 #${result.listingId} · 매매가 ${formatWon(result.purchasePriceKrw)} · 부대비용 ${formatWon(result.acquisitionIncidentalCostKrw)} · 이사비 ${formatWon(result.movingCostKrw)}`;
  nodes.collateral.textContent = `${formatWon(result.recognizedCollateralValueKrw)} · ${ltvRegionClassText(result.ltvRegionClass)} · 최대 ${formatWon(result.maximumMortgageKrw)}`;
  nodes.ltv.textContent = `${formatWon(result.ltv.numeratorKrw)} / ${formatWon(result.ltv.denominatorKrw)} · ${result.ltv.ratioPpm.toLocaleString('ko-KR')}ppm (한도 ${result.ltv.limitPpm.toLocaleString('ko-KR')}ppm)`;
  nodes.income.textContent =
    result.verifiedAnnualIncomeKrw === null
      ? '검증 가능한 연소득 없음'
      : `${formatWon(result.verifiedAnnualIncomeKrw)} · 재직 계약`;
  nodes.dsr.textContent = mortgageDsrText(result);
  nodes.ownFunds.textContent = `사용 가능 ${formatWon(result.availableBuyerCashKrw)} / 필요 ${formatWon(result.requiredBuyerCashKrw)}`;
  const first = result.quotedTerms.firstInstallment;
  nodes.terms.textContent = `연 ${result.quotedTerms.annualRateBp.toLocaleString('ko-KR')}bp · ${result.quotedTerms.termMonths.toLocaleString('ko-KR')}개월 · 전기간 고정 stress 0bp · 첫 납입 game day ${first.dueGameDay}, ${formatWon(first.totalKrw)} (원금 ${formatWon(first.principalKrw)}, 이자 ${formatWon(first.interestKrw)}, 비용 ${formatWon(first.feeKrw)})`;
  nodes.leaseExit.textContent =
    result.returnedDepositKrw === 0
      ? '반환 임대차 보증금 없음'
      : `보증금 ${formatWon(result.returnedDepositKrw)} 반환 · ${result.replacedLoanId === null ? '상환할 전세대출 없음' : `전세대출 #${result.replacedLoanId} ${formatWon(result.replacedLoanPrincipalKrw)} 상환`}`;
}

function renderPurchaseResult(
  result: HousingPurchaseResult | undefined,
  nodes: PurchaseResultNodes,
): void {
  nodes.section.hidden = result === undefined;
  if (result === undefined) {
    nodes.holding.textContent = '';
    nodes.acquisition.textContent = '';
    nodes.leaseExit.textContent = '';
    nodes.wallet.textContent = '';
    nodes.mortgage.textContent = '';
    return;
  }
  nodes.holding.textContent = `#${result.holding.id} · 매물 #${result.listingId} · ${REGION_LABEL[result.holding.regionKey]} ${PROPERTY_TYPE_LABEL[result.holding.propertyType]} ${result.holding.exclusiveAreaSquareMeters.toLocaleString('ko-KR')}㎡ · game day ${result.effectiveFromGameDay}부터 실거주`;
  nodes.acquisition.textContent = `매매가 ${formatWon(result.purchasePriceKrw)} · 부대비용 ${formatWon(result.acquisitionIncidentalCostKrw)} · 이사비 ${formatWon(result.movingCostKrw)}`;
  nodes.leaseExit.textContent =
    result.endedLeaseId === null
      ? '종료한 임대차 없음'
      : `임대차 #${result.endedLeaseId} · 보증금 ${formatWon(result.returnedDepositKrw)} 반환${result.repaidDepositLoan === null ? '' : ` · 전세대출 #${result.repaidDepositLoan.loanId} ${formatWon(result.repaidDepositLoan.principalKrw)} 상환`}`;
  nodes.wallet.textContent = formatWon(result.walletDeltaKrw);
  const mortgage = result.mortgageExecution;
  nodes.mortgage.textContent =
    mortgage === null
      ? '현금 매수 · lien 없음'
      : `계약 #${mortgage.loanId} · 견적 #${mortgage.quoteId} · ${formatWon(mortgage.principalKrw)} · 연 ${mortgage.annualRateBp.toLocaleString('ko-KR')}bp · 첫 납입 game day ${mortgage.firstInstallment.dueGameDay}`;
}

function updateSummary(
  nodes: HousingSummaryNodes,
  response: HousingListingsResponse | undefined,
): void {
  if (response === undefined) {
    nodes.model.textContent = '-';
    nodes.residenceRegion.textContent = '-';
    nodes.selectedRegion.textContent = '-';
    nodes.gameDay.textContent = '-';
    nodes.yearMonth.textContent = '-';
    nodes.priceIndex.textContent = '-';
    nodes.rentIndex.textContent = '-';
    return;
  }

  nodes.model.textContent = `#${response.modelVersionId}`;
  nodes.residenceRegion.textContent = housingRegionText(
    response.regions,
    response.residenceRegionKey,
  );
  nodes.selectedRegion.textContent = housingRegionText(
    response.regions,
    response.selectedRegionKey,
  );
  nodes.gameDay.textContent = String(response.gameDay);
  nodes.yearMonth.textContent = `${response.yearMonth.year}년 ${response.yearMonth.month}월`;
  nodes.priceIndex.textContent = indexText(response.priceIndexPpm);
  nodes.rentIndex.textContent = indexText(response.rentIndexPpm);
}

function housingStatusText(response: HousingListingsResponse): string {
  if (response.rateStatus === 'rateUnavailable') {
    return '이 run에는 주거 시장 지수와 매물이 적용되지 않습니다.';
  }
  return response.listings.length === 0
    ? '선택한 지역에 현재 유효한 매물이 없습니다.'
    : `서버가 산정한 현재 매물 ${response.listings.length}건입니다.`;
}

function housingQueryErrorText(error: unknown): string {
  return error instanceof HousingQueryError && error.code === 'characterRequired'
    ? '주거 정보를 보려면 현재 캐릭터와 run이 필요합니다.'
    : '주거 정보를 불러오지 못했습니다.';
}

function propertySaleStatusText(
  feedback: string,
  state: AsyncState<{ readonly items: readonly HousingPropertySaleOrderSummary[] }>,
  pendingCreate: HousingPropertySaleOrderCreateRequest | undefined,
  pendingReprice: HousingPropertySaleOrderRepriceCommand | undefined,
  pendingCancel: HousingPropertySaleOrderCancelCommand | undefined,
): string {
  if (feedback !== '') return feedback;
  if (pendingCreate !== undefined) {
    return '결과를 확인하지 못한 매도 주문 생성이 있습니다. 같은 body로 확인합니다.';
  }
  if (pendingReprice !== undefined) {
    return '결과를 확인하지 못한 주문가 변경이 있습니다. 같은 경로와 body로 확인합니다.';
  }
  if (pendingCancel !== undefined) {
    return '결과를 확인하지 못한 주문 취소가 있습니다. 같은 경로와 body로 확인합니다.';
  }
  if (state.status === 'loading' || state.status === 'idle') {
    return '매도 주문 이력을 불러오는 중입니다.';
  }
  if (state.status === 'error') return housingQueryErrorText(state.error);
  const activeCount = state.value.items.filter((order) => order.status === 'active').length;
  return state.value.items.length === 0
    ? '아직 매도 주문 이력이 없습니다.'
    : `최근 매도 주문 ${state.value.items.length}건 · 활성 ${activeCount}건`;
}

function propertyTaxHistoryStatusText(
  state: AsyncState<HousingPropertyTaxEventsResponse>,
  holdingId: string,
): string {
  if (holdingId === '') return '세금 이력을 조회할 보유주택이 없습니다.';
  if (state.status === 'loading' || state.status === 'idle') {
    return `보유주택 #${holdingId}의 세금 이력을 불러오는 중입니다.`;
  }
  if (state.status === 'error') {
    if (
      state.error instanceof HousingQueryError &&
      state.error.code === 'housingResourceNotFound'
    ) {
      return '선택한 보유주택의 세금 이력을 찾을 수 없습니다.';
    }
    return housingQueryErrorText(state.error);
  }
  return state.value.items.length === 0
    ? `보유주택 #${holdingId}의 세금 이력이 없습니다.`
    : `보유주택 #${holdingId}의 최근 세금 이력 ${state.value.items.length}건입니다.`;
}

function propertySaleCommandResultText(
  listing: HousingPropertySaleOrderListingResult | undefined,
  cancellation: HousingPropertySaleOrderCancellationResult | undefined,
): string {
  if (listing !== undefined) {
    return `주문 #${listing.orderId} 개정 ${listing.revisionNo} · 주문가 ${formatWon(listing.askingPriceKrw)} · 기준가 ${formatWon(listing.referenceValueKrw)} · 비율 ${listing.askingToReferencePpm.toLocaleString('ko-KR')}ppm · 후보 체결 game day ${listing.candidateGameDay}`;
  }
  if (cancellation !== undefined) {
    return `주문 #${cancellation.orderId} 개정 ${cancellation.revisionNo} · game day ${cancellation.cancelledGameDay} 취소`;
  }
  return '';
}

function propertySaleListingSuccessText(replayed: boolean, action: 'create' | 'reprice'): string {
  if (replayed) {
    return action === 'create'
      ? '이전에 만든 같은 매도 주문 결과를 확인했습니다.'
      : '이전에 완료된 같은 주문가 변경 결과를 확인했습니다.';
  }
  return action === 'create'
    ? '매도 주문을 만들었습니다. 후보 game day에 서버가 체결을 판정합니다.'
    : '주문가와 후보 체결일을 변경했습니다.';
}

function housingPropertySaleErrorText(
  error: unknown,
  action: 'create' | 'reprice' | 'cancel',
): string {
  if (error instanceof HousingFormError) return error.message;
  if (!(error instanceof HousingCommandError)) {
    const actionText =
      action === 'create' ? '매도 주문 생성' : action === 'reprice' ? '주문가 변경' : '주문 취소';
    return `서버 응답을 확인하지 못했습니다. 같은 ${actionText} 명령으로 다시 확인해 주세요.`;
  }
  switch (error.code) {
    case 'housingResourceNotFound':
      return '선택한 보유주택 또는 매도 주문을 찾을 수 없습니다.';
    case 'policyUnsupported':
    case 'rateUnavailable':
      return '이 run의 부동산 정책에서는 주택 매도를 사용할 수 없습니다.';
    case 'contractConflict':
      return '보유주택·담보대출·매도 주문 상태가 바뀌었습니다. 최신 이력을 확인해 주세요.';
    case 'insufficientWalletCash':
      return '거래비용·담보상환·세금을 충당할 수 없어 현재 조건으로 체결할 수 없습니다.';
    case 'busy':
      return '서버가 다른 정산을 처리 중입니다. 잠시 후 최신 상태에서 다시 시도해 주세요.';
    default:
      return error.message;
  }
}

function depositLoanStatusText(
  feedback: string,
  products: AsyncState<LoanProductCatalog>,
  pending: HousingLeaseDepositLoanQuoteRequest | undefined,
  offer: TenantHousingOffer | undefined,
): string {
  if (feedback !== '') return feedback;
  if (pending !== undefined) {
    return '결과를 확인하지 못한 전세자금대출 견적이 있습니다. 같은 body로 확인합니다.';
  }
  if (products.status === 'loading' || products.status === 'idle') {
    return '전세자금대출 상품을 불러오는 중입니다.';
  }
  if (products.status === 'error') return '전세자금대출 상품을 불러오지 못했습니다.';
  if (!products.value.products.some(isHousingDepositLoanProduct)) {
    return '이 run에는 전세자금대출 상품이 적용되지 않습니다.';
  }
  if (offer?.kind !== 'jeonse') return '현재 월의 전세 조건을 선택해 주세요.';
  return '요청 원금을 입력하면 서버가 보증금 한도와 개발 상환여력을 심사합니다.';
}

function isHousingDepositLoanProduct(product: LoanProduct): boolean {
  return (
    product.kind === 'leaseDepositLoan' &&
    product.quoteEligible &&
    product.executionEligible &&
    !product.startingEligible
  );
}

function isHousingMortgageProduct(product: LoanProduct): boolean {
  return (
    product.kind === 'mortgage' &&
    product.quoteEligible &&
    product.executionEligible &&
    !product.startingEligible
  );
}

function mortgageStatusText(
  feedback: string,
  holdings: AsyncState<HousingPropertyHoldingsResponse>,
  pending: HousingMortgageQuoteRequest | undefined,
  offer: SaleHousingOffer | undefined,
): string {
  if (feedback !== '') return feedback;
  if (pending !== undefined) {
    return '결과를 확인하지 못한 주택담보대출 견적이 있습니다. 같은 body로 확인합니다.';
  }
  if (holdings.status === 'loading' || holdings.status === 'idle') {
    return '주택 매수 기능을 확인하는 중입니다.';
  }
  if (holdings.status === 'error') return housingQueryErrorText(holdings.error);
  if (holdings.value.purchaseCapability === 'unavailable') {
    return '이 run에서는 주택 매수와 주택담보대출을 사용할 수 없습니다.';
  }
  if (holdings.value.holdings.length > 0) return '이미 active 보유주택이 있습니다.';
  if (offer === undefined) return '현재 월의 매매 조건을 선택해 주세요.';
  return '현금 매수 또는 서버의 LTV·DSR·자기자금 심사를 거친 주담대 매수를 선택할 수 있습니다.';
}

function housingMortgageErrorText(error: unknown, action: 'quote' | 'purchase'): string {
  if (error instanceof HousingFormError) return error.message;
  if (!(error instanceof HousingCommandError)) {
    return action === 'quote'
      ? '서버 응답을 확인하지 못했습니다. 같은 주택담보대출 견적으로 다시 확인해 주세요.'
      : '서버 응답을 확인하지 못했습니다. 같은 주택 매수 명령으로 다시 확인해 주세요.';
  }
  switch (error.code) {
    case 'creditRestricted':
      return '최신 신용 상태에서는 주택담보대출을 실행할 수 없습니다.';
    case 'incomeUnavailable':
      return '검증 가능한 소득이 없어 DSR 심사를 완료할 수 없습니다.';
    case 'collateralLimit':
      return '최신 담보 한도가 요청 원금보다 작습니다.';
    case 'debtServiceLimit':
      return '최신 DSR 한도를 초과했습니다.';
    case 'rateUnavailable':
      return '현재 주택담보대출 금리를 확정할 수 없습니다.';
    case 'contractConflict':
      return '매물·거주·임대차·보유주택 또는 견적 상태가 바뀌었습니다. 최신 정보로 다시 확인해 주세요.';
    case 'insufficientWalletCash':
      return '서버가 확정한 자기자금·부대비용·이사비를 지갑에서 지급할 수 없습니다.';
    case 'busy':
      return '서버가 다른 정산을 처리 중입니다. 잠시 후 다시 시도해 주세요.';
    default:
      return error.message;
  }
}

function mortgageDecisionText(decision: HousingMortgageQuoteResult['decisionCode']): string {
  switch (decision) {
    case 'creditRestricted':
      return '신용 제한';
    case 'purchaseRestricted':
      return '매수 제한';
    case 'collateralLimit':
      return '담보 한도 초과';
    case 'incomeUnavailable':
      return '검증 소득 없음';
    case 'debtServiceLimit':
      return 'DSR 한도 초과';
    case 'insufficientOwnFunds':
      return '자기자금 부족';
    case 'eligible':
      return '실행 가능';
  }
}

function mortgageDecisionReasonText(
  reason: HousingMortgageQuoteResult['decisionReasons'][number],
): string {
  switch (reason) {
    case 'activeDefault':
      return '활성 채무불이행';
    case 'activeDelinquency':
      return '활성 연체';
    case 'activeRestructuring':
      return '활성 채무조정';
    case 'creditBandRestricted':
      return '신용등급 제한';
    case 'activeLoanLimit':
      return '활성 대출 수 제한';
    case 'activeHolding':
      return '이미 보유주택 있음';
    case 'residenceChangedToday':
      return '오늘 거주 변경됨';
    case 'leaseExitRestricted':
      return '임대차 연결 대출 종료 불가';
    case 'collateralLimit':
      return 'LTV·지역·상품 한도 초과';
    case 'incomeUnavailable':
      return '검증 소득 없음';
    case 'debtServiceLimit':
      return 'DSR 한도 초과';
    case 'insufficientOwnFunds':
      return '자기자금 부족';
    case 'eligible':
      return '실행 가능';
  }
}

function ltvRegionClassText(regionClass: HousingMortgageQuoteResult['ltvRegionClass']): string {
  return regionClass === 'regulatedCapitalProxy'
    ? '게임 규제수도권 proxy'
    : '게임 비규제지역 proxy';
}

function mortgageDsrText(result: HousingMortgageQuoteResult): string {
  if (!result.dsrApplied) return '차주단위 DSR gate 미적용 · 전기간 고정 stress 적용률 0%';
  if (result.dsr === null) return 'DSR gate 적용 · 완전한 소득 근거 없음';
  return `${formatWon(result.dsr.numeratorKrw)} / ${formatWon(result.dsr.denominatorKrw)} · ${result.dsr.ratioPpm.toLocaleString('ko-KR')}ppm (한도 ${result.dsr.limitPpm.toLocaleString('ko-KR')}ppm) · 전기간 고정 stress 적용률 0%`;
}

function housingDepositLoanErrorText(error: unknown, action: 'quote' | 'move'): string {
  if (error instanceof HousingFormError) return error.message;
  if (!(error instanceof HousingCommandError)) {
    return action === 'quote'
      ? '서버 응답을 확인하지 못했습니다. 같은 전세자금대출 견적으로 다시 확인해 주세요.'
      : '서버 응답을 확인하지 못했습니다. 같은 대출 전세 입주 명령으로 다시 확인해 주세요.';
  }
  switch (error.code) {
    case 'creditRestricted':
      return '현재 신용 상태에서는 전세자금대출을 사용할 수 없습니다.';
    case 'incomeUnavailable':
      return '검증 가능한 연소득이 없어 전세자금대출을 실행할 수 없습니다.';
    case 'affordabilityLimit':
      return '개발 상환여력 한도를 초과했습니다.';
    case 'collateralLimit':
      return '요청 원금이 전세 보증금 대출 한도를 초과했습니다.';
    case 'rateUnavailable':
      return '현재 전세자금대출 금리를 확정할 수 없습니다.';
    case 'contractConflict':
      return '매물·견적·기존 계약 상태가 바뀌었습니다. 최신 정보로 다시 견적을 받아 주세요.';
    case 'insufficientWalletCash':
      return '대출을 제외한 자기자금과 이사비를 지갑에서 지급할 수 없습니다.';
    case 'busy':
      return '서버가 다른 정산을 처리 중입니다. 잠시 후 다시 시도해 주세요.';
    default:
      return error.message;
  }
}

function depositLoanDecisionReasonText(
  reason: HousingLeaseDepositLoanQuoteResult['decisionReasons'][number],
): string {
  switch (reason) {
    case 'activeDefault':
      return '활성 채무불이행';
    case 'activeDelinquency':
      return '활성 연체';
    case 'activeRestructuring':
      return '활성 채무조정';
    case 'creditBandRestricted':
      return '신용등급 제한';
    case 'activeLoanLimit':
      return '활성 대출 수 제한';
    case 'collateralLimit':
      return '보증금 한도 초과';
    case 'incomeUnavailable':
      return '검증 소득 없음';
    case 'affordabilityLimit':
      return '개발 상환여력 초과';
    case 'eligible':
      return '실행 가능';
  }
}

function depositLoanDecisionText(
  decision: HousingLeaseDepositLoanQuoteResult['decisionCode'],
): string {
  switch (decision) {
    case 'eligible':
      return '실행 가능';
    case 'creditRestricted':
      return '신용 제한';
    case 'collateralLimit':
      return '보증금 한도 초과';
    case 'incomeUnavailable':
      return '검증 소득 없음';
    case 'affordabilityLimit':
      return '개발 상환여력 초과';
  }
}

function housingCommandErrorText(error: unknown): string {
  if (error instanceof HousingFormError) return error.message;
  if (!(error instanceof HousingCommandError)) {
    return '서버 응답을 확인하지 못했습니다. 같은 이사 명령으로 다시 확인해 주세요.';
  }
  switch (error.code) {
    case 'insufficientWalletCash':
      return '반환 보증금과 지갑을 합쳐도 새 보증금과 이사비가 부족합니다.';
    case 'rateUnavailable':
      return '이 run에서는 선택한 임대차 이사를 사용할 수 없습니다.';
    case 'contractConflict':
      return '매물 또는 현재 계약 상태가 바뀌었습니다. 최신 정보를 확인해 주세요.';
    case 'busy':
      return '서버가 다른 정산을 처리 중입니다. 잠시 후 최신 상태에서 다시 시도해 주세요.';
    default:
      return error.message;
  }
}

function housingArrearPaymentErrorText(error: unknown): string {
  if (error instanceof HousingFormError) return error.message;
  if (!(error instanceof HousingCommandError)) {
    return '서버 응답을 확인하지 못했습니다. 같은 월세 연체 상환 명령으로 다시 확인해 주세요.';
  }
  switch (error.code) {
    case 'insufficientWalletCash':
      return '지갑 현금이 상환 금액보다 부족합니다.';
    case 'contractConflict':
      return '연체가 이미 상환됐거나 금액이 현재 잔액을 초과합니다. 최신 정보를 확인해 주세요.';
    case 'busy':
      return '서버가 다른 정산을 처리 중입니다. 잠시 후 최신 상태에서 다시 시도해 주세요.';
    default:
      return error.message;
  }
}

function moveAvailabilityText(
  snapshot: GameSnapshot | undefined,
  leaseState: AsyncState<HousingCurrentLeaseResponse>,
  pending: HousingLeaseRequest | undefined,
  advancing: boolean,
  ordering: boolean,
  busy: boolean,
  offer: TenantHousingOffer | undefined,
): string {
  if (snapshot === undefined || snapshot.characterName === null) return '현재 캐릭터가 필요합니다.';
  if (pending !== undefined)
    return '결과를 확인하지 못한 이사 명령이 있습니다. 같은 body로 확인합니다.';
  if (leaseState.status === 'loading' || leaseState.status === 'idle') {
    return '이사 기능을 확인하는 중입니다.';
  }
  if (leaseState.status === 'error') return housingQueryErrorText(leaseState.error);
  if (leaseState.value.leaseCapability === 'unavailable') {
    return '이 run에서는 임대차 이사를 사용할 수 없습니다.';
  }
  if (snapshot.autoSpeed !== null) return '이사하려면 자동 진행을 먼저 멈춰 주세요.';
  if (advancing || ordering || busy) return '다른 게임 명령을 처리하는 중입니다.';
  if (
    snapshot.life.residence !== null &&
    snapshot.gameDay <= snapshot.life.residence.effectiveFromGameDay
  ) {
    return '현재 거주가 시작된 다음 game day부터 다시 이사할 수 있습니다.';
  }
  if (offer === undefined) return '현재 월의 전세 또는 월세 조건을 선택해 주세요.';
  if (!capabilitySupportsOffer(leaseState.value.leaseCapability, offer.kind)) {
    return '이 run에서는 선택한 임대차 조건을 사용할 수 없습니다.';
  }
  return '표시 금액을 확인한 뒤 이사 명령을 실행할 수 있습니다.';
}

function hasCurrentCharacter(
  snapshot: GameSnapshot | undefined,
): snapshot is GameSnapshot & { readonly characterName: string } {
  return snapshot !== undefined && snapshot.characterName !== null;
}

function runRevisionChanged(next: GameSnapshot, previous: GameSnapshot | undefined): boolean {
  return previous === undefined || next.runRevision !== previous.runRevision;
}

function housingReadCursorChanged(next: GameSnapshot, previous: GameSnapshot | undefined): boolean {
  return (
    previous === undefined ||
    next.runRevision !== previous.runRevision ||
    next.stateRevision !== previous.stateRevision ||
    next.gameDay !== previous.gameDay
  );
}

function commandSnapshot(deps: HousingViewDeps): GameSnapshot {
  const current = deps.store.getState().game.snapshot;
  if (current === undefined || current.characterName === null) {
    throw new HousingFormError('현재 캐릭터가 필요합니다.');
  }
  return current;
}

function selectMoveRequest(
  retries: ReturnType<typeof createHousingLeaseRetryPolicy>,
  snapshot: GameSnapshot,
  selectedLeaseKey: string,
): { readonly request: HousingLeaseRequest; readonly retry: boolean } {
  const pending = retries.pending(snapshot.runRevision);
  const selection = pending ?? leaseSelectionFromKey(selectedLeaseKey);
  if (selection === undefined) {
    throw new HousingFormError('이사할 전세 또는 월세 조건을 선택해 주세요.');
  }
  return {
    request: retries.select(snapshot, {
      listingId: selection.listingId,
      offerKind: selection.offerKind,
    }),
    retry: pending !== undefined,
  };
}

function selectDepositLoanQuoteRequest(
  retries: ReturnType<typeof createHousingLeaseDepositLoanQuoteRetryPolicy>,
  snapshot: GameSnapshot,
  listing: HousingListing | undefined,
  offer: TenantHousingOffer | undefined,
  product: LoanProduct | undefined,
  rawPrincipalKrw: string,
): { readonly request: HousingLeaseDepositLoanQuoteRequest; readonly retry: boolean } {
  const pending = retries.pending(snapshot.runRevision);
  if (pending !== undefined) return { request: pending, retry: true };
  if (listing === undefined || offer?.kind !== 'jeonse') {
    throw new HousingFormError('전세자금대출 견적을 받을 전세 매물을 선택해 주세요.');
  }
  if (product === undefined || !isHousingDepositLoanProduct(product)) {
    throw new HousingFormError('현재 run의 전세자금대출 상품을 선택해 주세요.');
  }
  if (product.rateStatus !== 'available') {
    throw new HousingFormError('현재 금리를 확정할 수 없는 전세자금대출 상품입니다.');
  }
  const principalKrw = positiveIntegerOf(rawPrincipalKrw, '요청 원금');
  if (principalKrw < product.minimumPrincipalKrw || principalKrw > product.maximumPrincipalKrw) {
    throw new HousingFormError(
      `요청 원금은 ${formatWon(product.minimumPrincipalKrw)}부터 ${formatWon(product.maximumPrincipalKrw)}까지 입력해 주세요.`,
    );
  }
  return {
    request: retries.select(snapshot, {
      listingId: listing.id,
      offerKind: 'jeonse',
      productVersionId: product.id,
      principalKrw,
    }),
    retry: false,
  };
}

function selectFinancedMoveRequest(
  retries: ReturnType<typeof createHousingLeaseRetryPolicy>,
  snapshot: GameSnapshot,
  quote: HousingLeaseDepositLoanQuoteResult | undefined,
  listing: HousingListing | undefined,
  offer: TenantHousingOffer | undefined,
): { readonly request: HousingLeaseRequest; readonly retry: boolean } {
  const pending = retries.pending(snapshot.runRevision);
  if (pending !== undefined) {
    if (!('loanQuoteId' in pending)) {
      throw new HousingFormError('결과 확인 대기 중인 현금 임대차 명령을 먼저 확인해 주세요.');
    }
    return { request: pending, retry: true };
  }
  if (
    quote?.decisionCode !== 'eligible' ||
    quote.expiresGameDay !== snapshot.gameDay ||
    quote.listingId !== listing?.id ||
    offer?.kind !== 'jeonse'
  ) {
    throw new HousingFormError('현재 선택한 전세 매물의 실행 가능한 견적을 먼저 받아 주세요.');
  }
  return {
    request: retries.select(snapshot, {
      listingId: quote.listingId,
      offerKind: 'jeonse',
      loanQuoteId: quote.quoteId,
    }),
    retry: false,
  };
}

function selectMortgageQuoteRequest(
  retries: ReturnType<typeof createHousingMortgageQuoteRetryPolicy>,
  snapshot: GameSnapshot,
  listing: HousingListing | undefined,
  product: LoanProduct | undefined,
  rawPrincipalKrw: string,
): { readonly request: HousingMortgageQuoteRequest; readonly retry: boolean } {
  const pending = retries.pending(snapshot.runRevision);
  if (pending !== undefined) return { request: pending, retry: true };
  if (listing === undefined || saleOfferOf(listing) === undefined) {
    throw new HousingFormError('주택담보대출 견적을 받을 매매 매물을 선택해 주세요.');
  }
  if (product === undefined || !isHousingMortgageProduct(product)) {
    throw new HousingFormError('현재 run의 주택담보대출 상품을 선택해 주세요.');
  }
  if (product.rateStatus !== 'available') {
    throw new HousingFormError('현재 금리를 확정할 수 없는 주택담보대출 상품입니다.');
  }
  const principalKrw = positiveIntegerOf(rawPrincipalKrw, '요청 원금');
  if (principalKrw < product.minimumPrincipalKrw || principalKrw > product.maximumPrincipalKrw) {
    throw new HousingFormError(
      `요청 원금은 ${formatWon(product.minimumPrincipalKrw)}부터 ${formatWon(product.maximumPrincipalKrw)}까지 입력해 주세요.`,
    );
  }
  return {
    request: retries.select(snapshot, {
      listingId: listing.id,
      productVersionId: product.id,
      principalKrw,
    }),
    retry: false,
  };
}

function selectPurchaseRequest(
  retries: ReturnType<typeof createHousingPurchaseRetryPolicy>,
  snapshot: GameSnapshot,
  listing: HousingListing | undefined,
  quote: HousingMortgageQuoteResult | undefined,
  useMortgage: boolean,
): { readonly request: HousingPurchaseRequest; readonly retry: boolean } {
  const pending = retries.pending(snapshot.runRevision);
  if (pending !== undefined) return { request: pending, retry: true };
  if (listing === undefined || saleOfferOf(listing) === undefined) {
    throw new HousingFormError('매수할 매매 매물을 선택해 주세요.');
  }
  if (
    useMortgage &&
    (quote?.decisionCode !== 'eligible' ||
      quote.expiresGameDay !== snapshot.gameDay ||
      quote.listingId !== listing.id)
  ) {
    throw new HousingFormError(
      '현재 선택한 매물의 실행 가능한 주택담보대출 견적을 먼저 받아 주세요.',
    );
  }
  return {
    request: retries.select(snapshot, {
      listingId: listing.id,
      mortgageQuoteId: useMortgage ? (quote?.quoteId ?? null) : null,
    }),
    retry: false,
  };
}

function mortgageQuoteForPurchase(
  useMortgage: boolean,
  quote: HousingMortgageQuoteResult | undefined,
): HousingMortgageQuoteResult | undefined {
  return useMortgage ? quote : undefined;
}

function purchaseSubmittingText(retry: boolean, useMortgage: boolean): string {
  if (retry) return '같은 주택 매수 결과를 다시 확인하는 중입니다.';
  return useMortgage
    ? '주담대를 재심사하고 주택을 매수하는 중입니다.'
    : '서버가 최종 비용을 확정해 현금으로 주택을 매수하는 중입니다.';
}

function purchaseSuccessText(replayed: boolean): string {
  return replayed
    ? '이전에 완료된 같은 주택 매수 결과를 확인했습니다.'
    : '주택 매수와 소유자 입주가 완료되었습니다.';
}

function moveSubmittingText(retry: boolean): string {
  return retry
    ? '같은 이사 명령의 결과를 다시 확인하는 중입니다.'
    : '임대차 계약과 이사를 처리하는 중입니다.';
}

function moveSuccessText(replayed: boolean): string {
  return replayed
    ? '이전에 완료된 같은 이사 결과를 다시 불러왔습니다.'
    : '임대차 계약과 이사가 완료되었습니다.';
}

function pendingForSnapshot(
  retries: ReturnType<typeof createHousingLeaseRetryPolicy>,
  snapshot: GameSnapshot | undefined,
): HousingLeaseRequest | undefined {
  return snapshot === undefined ? undefined : retries.pending(snapshot.runRevision);
}

function pendingPaymentForSnapshot(
  retries: ReturnType<typeof createHousingLeaseArrearPaymentRetryPolicy>,
  snapshot: GameSnapshot | undefined,
): HousingLeaseArrearPaymentCommand | undefined {
  return snapshot === undefined ? undefined : retries.pending(snapshot.runRevision);
}

function pendingQuoteForSnapshot(
  retries: ReturnType<typeof createHousingLeaseDepositLoanQuoteRetryPolicy>,
  snapshot: GameSnapshot | undefined,
): HousingLeaseDepositLoanQuoteRequest | undefined {
  return snapshot === undefined ? undefined : retries.pending(snapshot.runRevision);
}

function pendingMortgageQuoteForSnapshot(
  retries: ReturnType<typeof createHousingMortgageQuoteRetryPolicy>,
  snapshot: GameSnapshot | undefined,
): HousingMortgageQuoteRequest | undefined {
  return snapshot === undefined ? undefined : retries.pending(snapshot.runRevision);
}

function pendingPurchaseForSnapshot(
  retries: ReturnType<typeof createHousingPurchaseRetryPolicy>,
  snapshot: GameSnapshot | undefined,
): HousingPurchaseRequest | undefined {
  return snapshot === undefined ? undefined : retries.pending(snapshot.runRevision);
}

function pendingPropertySaleCreateForSnapshot(
  retries: ReturnType<typeof createHousingPropertySaleOrderCreateRetryPolicy>,
  snapshot: GameSnapshot | undefined,
): HousingPropertySaleOrderCreateRequest | undefined {
  return snapshot === undefined ? undefined : retries.pending(snapshot.runRevision);
}

function pendingPropertySaleRepriceForSnapshot(
  retries: ReturnType<typeof createHousingPropertySaleOrderRepriceRetryPolicy>,
  snapshot: GameSnapshot | undefined,
): HousingPropertySaleOrderRepriceCommand | undefined {
  return snapshot === undefined ? undefined : retries.pending(snapshot.runRevision);
}

function pendingPropertySaleCancelForSnapshot(
  retries: ReturnType<typeof createHousingPropertySaleOrderCancelRetryPolicy>,
  snapshot: GameSnapshot | undefined,
): HousingPropertySaleOrderCancelCommand | undefined {
  return snapshot === undefined ? undefined : retries.pending(snapshot.runRevision);
}

function selectPropertySaleCreateRequest(
  retries: ReturnType<typeof createHousingPropertySaleOrderCreateRetryPolicy>,
  snapshot: GameSnapshot,
  holdingId: string,
  askingPriceRaw: string,
): { readonly request: HousingPropertySaleOrderCreateRequest; readonly retry: boolean } {
  const pending = retries.pending(snapshot.runRevision);
  if (pending !== undefined) return { request: pending, retry: true };
  if (holdingId === '') throw new HousingFormError('매도할 보유주택을 선택해 주세요.');
  return {
    request: retries.select(snapshot, {
      holdingId,
      askingPriceKrw: positiveIntegerOf(askingPriceRaw, '주문가'),
    }),
    retry: false,
  };
}

function selectPropertySaleRepriceCommand(
  retries: ReturnType<typeof createHousingPropertySaleOrderRepriceRetryPolicy>,
  snapshot: GameSnapshot,
  orderId: string,
  askingPriceRaw: string,
  orders: readonly HousingPropertySaleOrderSummary[],
): { readonly command: HousingPropertySaleOrderRepriceCommand; readonly retry: boolean } {
  const pending = retries.pending(snapshot.runRevision);
  if (pending !== undefined) return { command: pending, retry: true };
  const order = orders.find((candidate) => candidate.orderId === orderId);
  if (order?.status !== 'active') {
    throw new HousingFormError('주문가를 변경할 활성 매도 주문을 선택해 주세요.');
  }
  return {
    command: retries.select(snapshot, {
      orderId: order.orderId,
      askingPriceKrw: positiveIntegerOf(askingPriceRaw, '새 주문가'),
    }),
    retry: false,
  };
}

function selectPropertySaleCancelCommand(
  retries: ReturnType<typeof createHousingPropertySaleOrderCancelRetryPolicy>,
  snapshot: GameSnapshot,
  orderId: string,
  orders: readonly HousingPropertySaleOrderSummary[],
): { readonly command: HousingPropertySaleOrderCancelCommand; readonly retry: boolean } {
  const pending = retries.pending(snapshot.runRevision);
  if (pending !== undefined) return { command: pending, retry: true };
  const order = orders.find((candidate) => candidate.orderId === orderId);
  if (order?.status !== 'active') {
    throw new HousingFormError('취소할 활성 매도 주문을 선택해 주세요.');
  }
  return {
    command: retries.select(snapshot, { orderId: order.orderId }),
    retry: false,
  };
}

function selectLeaseArrearPayment(
  retries: ReturnType<typeof createHousingLeaseArrearPaymentRetryPolicy>,
  snapshot: GameSnapshot,
  select: HTMLSelectElement,
  input: HTMLInputElement,
  arrears: readonly HousingLeaseArrear[],
  payFull: boolean,
): { readonly command: HousingLeaseArrearPaymentCommand; readonly retry: boolean } {
  const pending = retries.pending(snapshot.runRevision);
  if (pending !== undefined) return { command: pending, retry: true };

  const arrear = selectedLeaseArrear(select, arrears);
  if (arrear === undefined) {
    throw new HousingFormError('상환할 월세 연체를 선택해 주세요.');
  }
  const amountKrw = payFull ? arrear.remainingKrw : positiveIntegerOf(input.value, '상환 금액');
  if (amountKrw > arrear.remainingKrw) {
    throw new HousingFormError(
      `상환 금액은 남은 ${formatWon(arrear.remainingKrw)} 이하여야 합니다.`,
    );
  }
  return {
    command: retries.select(snapshot, { arrearId: arrear.id, amountKrw }),
    retry: false,
  };
}

function renderLeaseArrearPaymentResult(
  result: HousingLeaseArrearPaymentResult | undefined,
  nodes: {
    readonly section: HTMLElement;
    readonly arrear: HTMLElement;
    readonly payment: HTMLElement;
    readonly paid: HTMLElement;
    readonly remaining: HTMLElement;
  },
): void {
  nodes.section.hidden = result === undefined;
  nodes.arrear.textContent = result?.arrearId ?? '';
  nodes.payment.textContent = result?.paymentId ?? '';
  nodes.paid.textContent = result === undefined ? '' : formatWon(result.paidKrw);
  nodes.remaining.textContent = result === undefined ? '' : formatWon(result.remainingKrw);
}

function activeLeaseText(lease: HousingActiveLease | null): string {
  if (lease === null) return '활성 tenant 임대차 계약 없음';
  const rent =
    lease.offerKind === 'monthlyRent'
      ? `, 월세 ${formatWon(lease.monthlyRentKrw)}, 다음 청구 game day ${lease.nextRentDueGameDay}`
      : '';
  const depositLoan =
    lease.depositLoanId === null ? '' : `, 연결 전세자금대출 #${lease.depositLoanId}`;
  return `#${lease.id}, 매물 #${lease.listingId}, ${lease.offerKind === 'jeonse' ? '전세' : '월세'}, ${REGION_LABEL[lease.regionKey]} ${PROPERTY_TYPE_LABEL[lease.propertyType]} ${lease.exclusiveAreaSquareMeters.toLocaleString('ko-KR')}㎡, 보증금 ${formatWon(lease.depositKrw)}${rent}${depositLoan}, game day ${lease.effectiveFromGameDay}부터`;
}

function leaseRenewalRuleText(rule: HousingCurrentLeaseResponse['renewalRule']): string {
  switch (rule) {
    case null:
      return '-';
    case 'openEnded':
      return '다음 이사까지 유지';
    case 'fixedTermAutoRenew':
      return '고정기간 · 만료 시 자동갱신';
  }
}

function leaseLifecycleTermsText(
  terms: HousingCurrentLeaseResponse['leaseLifecycleTerms'],
): string {
  if (terms === null) return '고정기간 계약 정책 미적용';
  const review = terms.monthlyRentTerminationReview;
  const reviewText =
    review === null
      ? ''
      : ` · 월세 연체 ${review.afterGameDays.toLocaleString('ko-KR')} game day부터 종료 검토`;
  return `${terms.termMonths.toLocaleString('ko-KR')}개월 고정기간 · 만료 ${terms.renewalNoticeLeadDays.toLocaleString('ko-KR')} game day 전 안내${reviewText}`;
}

function currentLeaseTermText(lease: HousingActiveLease | null): string {
  if (lease === null) return '-';
  const term = lease.currentTerm;
  if (term === null) return '고정기간 없음';
  return `${term.termNo.toLocaleString('ko-KR')}차 기간 · game day ${term.effectiveFromGameDay.toLocaleString('ko-KR')}부터 ${term.effectiveToGameDay.toLocaleString('ko-KR')} 전까지`;
}

function leaseRenewalNoticeText(lease: HousingActiveLease | null): string {
  if (lease === null) return '-';
  const notice = lease.renewalNotice;
  if (notice !== null) {
    return `${notice.termNo.toLocaleString('ko-KR')}차 기간 · game day ${notice.publishedGameDay.toLocaleString('ko-KR')} 게시 · game day ${notice.renewsOnGameDay.toLocaleString('ko-KR')} 자동갱신 예정`;
  }
  if (lease.currentTerm !== null) {
    return `안내 미게시 · game day ${lease.currentTerm.effectiveToGameDay.toLocaleString('ko-KR')} 자동갱신 예정`;
  }
  return '고정기간 자동갱신 없음';
}

function leaseTerminationReviewText(lease: HousingActiveLease | null): string {
  if (lease === null) return '-';
  const review = lease.terminationReview;
  if (review !== null) {
    return `game day ${review.openedGameDay.toLocaleString('ko-KR')}부터 종료 검토 중(자동 퇴거 아님) · 기준 연체 #${review.triggerArrearId} · 활성 계약 연체 ${formatWon(review.activeLeaseArrearKrw)}`;
  }
  if (lease.renewalRule === 'openEnded') return '계약 종료 검토 정책 미적용';
  return lease.offerKind === 'monthlyRent' ? '종료 검토 없음' : '월세 계약만 검토 대상';
}

function leaseCapabilityText(capability: HousingCurrentLeaseResponse['leaseCapability']): string {
  switch (capability) {
    case 'unavailable':
      return '이 run에서는 사용할 수 없음';
    case 'cashJeonse':
      return '현금 전세 이사 가능';
    case 'cashJeonseAndMonthlyRent':
      return '현금 전세·월세 이사 가능';
  }
}

function capabilitySupportsOffer(
  capability: HousingCurrentLeaseResponse['leaseCapability'] | undefined,
  offerKind: TenantHousingOffer['kind'] | undefined,
): boolean {
  if (capability === undefined || offerKind === undefined || capability === 'unavailable') {
    return false;
  }
  return offerKind === 'jeonse' || capability === 'cashJeonseAndMonthlyRent';
}

function selectedLeaseOf(
  listings: readonly HousingListing[],
  key: string,
): { readonly listing: HousingListing; readonly offer: TenantHousingOffer } | undefined {
  const selection = leaseSelectionFromKey(key);
  if (selection === undefined) return undefined;
  const listing = listings.find((candidate) => candidate.id === selection.listingId);
  const offer = tenantOffersOf(listing).find((candidate) => candidate.kind === selection.offerKind);
  return listing === undefined || offer === undefined ? undefined : { listing, offer };
}

function leaseSelectionFromKey(
  key: string,
): Pick<HousingLeaseRequest, 'listingId' | 'offerKind'> | undefined {
  const match = /^(0|[1-9]\d*):(jeonse|monthlyRent)$/.exec(key);
  const listingId = match?.[1];
  const offerKind = match?.[2];
  if (listingId === undefined || (offerKind !== 'jeonse' && offerKind !== 'monthlyRent')) {
    return undefined;
  }
  return { listingId, offerKind };
}

function leaseKey(listingId: string, offerKind: TenantHousingOffer['kind']): string {
  return `${listingId}:${offerKind}`;
}

function tenantOffersOf(listing: HousingListing | undefined): TenantHousingOffer[] {
  return (
    listing?.offers.filter(
      (offer): offer is TenantHousingOffer =>
        offer.kind === 'jeonse' || offer.kind === 'monthlyRent',
    ) ?? []
  );
}

function offerKindLabel(kind: TenantHousingOffer['kind']): string {
  return kind === 'jeonse' ? '전세' : '월세';
}

function leaseOptionText(listing: HousingListing, offer: TenantHousingOffer): string {
  const terms =
    offer.kind === 'jeonse'
      ? `보증금 ${formatWon(offer.depositKrw)}`
      : `보증금 ${formatWon(offer.depositKrw)}, 월 ${formatWon(offer.monthlyRentKrw)}`;
  return `#${listing.id} ${offerKindLabel(offer.kind)} · ${REGION_LABEL[listing.regionKey]} ${PROPERTY_TYPE_LABEL[listing.propertyType]} ${listing.exclusiveAreaSquareMeters.toLocaleString('ko-KR')}㎡ · ${terms}`;
}

function housingRegionText(regions: readonly HousingRegion[], regionKey: HousingRegionKey): string {
  const region = regions.find((candidate) => candidate.regionKey === regionKey);
  return region === undefined ? regionKey : `${region.displayName} (${region.regionKey})`;
}

function indexText(indexPpm: number | null): string {
  return indexPpm === null ? '사용할 수 없음' : `${indexPpm.toLocaleString('ko-KR')} ppm`;
}

function moneyOrDash(amount: number | undefined): string {
  return amount === undefined ? '-' : formatWon(amount);
}

function createFixedDepositLoanProductSelect(): FixedDepositLoanProductSelect {
  const element = el('select', {
    name: 'productVersionId',
    attrs: { 'aria-label': '전세자금대출 상품' },
  });
  const placeholder = el('option', { value: '' }, '전세자금대출 상품을 불러오는 중');
  const options = Array.from({ length: MAX_LOAN_PRODUCTS }, () => el('option'));
  element.append(placeholder, ...options);

  return {
    element,
    setItems(items, selectedId) {
      const available = items.filter((product) => product.rateStatus === 'available');
      const next = availableProductId(available, selectedId);
      updateDepositLoanProductPlaceholder(placeholder, items.length, available.length, next);
      updateDepositLoanProductOptions(options, items);
      element.value = next;
      return next;
    },
  };
}

function availableProductId(items: readonly LoanProduct[], selectedId: string): string {
  return items.some((product) => product.id === selectedId) ? selectedId : (items[0]?.id ?? '');
}

function updateDepositLoanProductPlaceholder(
  placeholder: HTMLOptionElement,
  itemCount: number,
  availableCount: number,
  selectedId: string,
): void {
  placeholder.hidden = selectedId !== '';
  placeholder.disabled = selectedId !== '';
  placeholder.textContent =
    itemCount === 0
      ? '이 run에는 전세자금대출 상품이 없습니다'
      : availableCount === 0
        ? '현재 금리를 확인할 수 없습니다'
        : '전세자금대출 상품을 선택하세요';
}

function updateDepositLoanProductOptions(
  options: readonly HTMLOptionElement[],
  items: readonly LoanProduct[],
): void {
  for (const [index, option] of options.entries()) {
    const product = items[index];
    option.hidden = product === undefined;
    option.disabled = product === undefined || product.rateStatus !== 'available';
    option.value = product?.id ?? '';
    option.textContent =
      product === undefined
        ? ''
        : `${product.displayName} · ${formatWon(product.minimumPrincipalKrw)}~${formatWon(product.maximumPrincipalKrw)} · 연 ${product.currentAnnualRateBp?.toLocaleString('ko-KR') ?? '-'}bp`;
  }
}

function syncDepositLoanPrincipalBounds(
  input: HTMLInputElement,
  product: LoanProduct | undefined,
): void {
  if (product === undefined) {
    input.removeAttribute('min');
    input.removeAttribute('max');
    input.placeholder = '';
    return;
  }
  input.min = String(product.minimumPrincipalKrw);
  input.max = String(product.maximumPrincipalKrw);
  input.placeholder = `${product.minimumPrincipalKrw}~${product.maximumPrincipalKrw}`;
}

function createFixedMortgageProductSelect(): FixedDepositLoanProductSelect {
  const element = el('select', {
    name: 'mortgageProductVersionId',
    attrs: { 'aria-label': '주택담보대출 상품' },
  });
  const placeholder = el('option', { value: '' }, '주택담보대출 상품을 불러오는 중');
  const options = Array.from({ length: MAX_LOAN_PRODUCTS }, () => el('option'));
  element.append(placeholder, ...options);
  return {
    element,
    setItems(items, selectedId) {
      const available = items.filter((product) => product.rateStatus === 'available');
      const next = availableProductId(available, selectedId);
      placeholder.hidden = next !== '';
      placeholder.disabled = next !== '';
      placeholder.textContent =
        items.length === 0
          ? '이 run에는 주택담보대출 상품이 없습니다'
          : available.length === 0
            ? '현재 주택담보대출 금리를 확인할 수 없습니다'
            : '주택담보대출 상품을 선택하세요';
      updateDepositLoanProductOptions(options, items);
      element.value = next;
      return next;
    },
  };
}

function syncMortgagePrincipalBounds(
  input: HTMLInputElement,
  product: LoanProduct | undefined,
): void {
  syncDepositLoanPrincipalBounds(input, product);
}

function createFixedSaleSelect(): FixedSaleSelect {
  const element = el('select', {
    name: 'saleListingId',
    attrs: { 'aria-label': '매수할 매매 매물' },
  });
  const placeholder = el('option', { value: '' }, '매매 매물을 불러오는 중');
  const options = Array.from({ length: MAX_LISTINGS }, () => el('option'));
  element.append(placeholder, ...options);
  return {
    element,
    setListings(listings, selectedId, pending) {
      const saleListings = listings.filter((listing) => saleOfferOf(listing) !== undefined);
      const pendingId = pending?.listingId;
      const next = saleSelectValue(saleListings, selectedId, pendingId);
      updateSalePlaceholder(placeholder, saleListings.length, pendingId, next);
      updateSaleOptions(options, saleListings);
      element.value = next;
      return next;
    },
  };
}

function saleSelectValue(
  listings: readonly HousingListing[],
  selectedId: string,
  pendingId: string | undefined,
): string {
  if (pendingId !== undefined) return pendingId;
  return listings.some((listing) => listing.id === selectedId)
    ? selectedId
    : (listings[0]?.id ?? '');
}

function updateSalePlaceholder(
  placeholder: HTMLOptionElement,
  listingCount: number,
  pendingId: string | undefined,
  selectedId: string,
): void {
  placeholder.hidden = selectedId !== '' && pendingId === undefined;
  placeholder.disabled = pendingId === undefined && listingCount > 0;
  placeholder.value = pendingId ?? '';
  if (pendingId !== undefined) {
    placeholder.textContent = `응답 확인 대기 중인 매물 #${pendingId}`;
    return;
  }
  placeholder.textContent =
    listingCount === 0 ? '현재 매수 가능한 매매 매물이 없습니다' : '매매 매물을 선택하세요';
}

function updateSaleOptions(
  options: readonly HTMLOptionElement[],
  listings: readonly HousingListing[],
): void {
  for (const [index, option] of options.entries()) {
    const listing = listings[index];
    const sale = saleOfferOf(listing);
    option.hidden = listing === undefined;
    option.disabled = listing === undefined;
    option.value = listing?.id ?? '';
    option.textContent =
      listing === undefined || sale === undefined
        ? ''
        : `#${listing.id} · ${REGION_LABEL[listing.regionKey]} ${PROPERTY_TYPE_LABEL[listing.propertyType]} ${listing.exclusiveAreaSquareMeters.toLocaleString('ko-KR')}㎡ · ${formatWon(sale.priceKrw)}`;
  }
}

function saleOfferOf(listing: HousingListing | undefined): SaleHousingOffer | undefined {
  return listing?.offers.find((offer): offer is SaleHousingOffer => offer.kind === 'sale');
}

function createFixedPropertyHoldingList(): FixedPropertyHoldingList {
  const rows = Array.from({ length: 4 }, () => {
    const text = el('span');
    const loan = el('a', { href: '/loans', dataset: { link: '' } }, '연결 대출 보기');
    const element = el('li', {}, text, ' ', loan);
    element.hidden = true;
    return { element, text, loan };
  });
  return {
    element: el('ul', {}, ...rows.map((row) => row.element)),
    setItems(items) {
      for (const [index, row] of rows.entries()) {
        const holding = items[index];
        row.element.hidden = holding === undefined;
        row.element.id = holding === undefined ? '' : `property-holding-${holding.id}`;
        row.text.textContent = holding === undefined ? '' : propertyHoldingText(holding);
        row.loan.hidden = holding?.mortgageLoanId === null || holding === undefined;
        row.loan.textContent =
          holding?.mortgageLoanId === null || holding === undefined
            ? ''
            : `주택담보대출 #${holding.mortgageLoanId} 보기`;
      }
    },
  };
}

function propertyHoldingText(holding: HousingPropertyHolding): string {
  const mortgage =
    holding.mortgageLoanId === null ? 'lien 없음' : `주택담보대출 #${holding.mortgageLoanId}`;
  return `#${holding.id} · 매물 #${holding.listingId} · ${REGION_LABEL[holding.regionKey]} ${PROPERTY_TYPE_LABEL[holding.propertyType]} ${holding.exclusiveAreaSquareMeters.toLocaleString('ko-KR')}㎡ · 취득가 ${formatWon(holding.acquisitionPriceKrw)} · 부대비용 ${formatWon(holding.acquisitionIncidentalCostKrw)} · 장부가 ${formatWon(holding.bookValueKrw)} · ${mortgage}`;
}

function createFixedPropertyHoldingSelect(label: string): FixedPropertyHoldingSelect {
  const element = el('select', { attrs: { 'aria-label': label } });
  const placeholder = el('option', { value: '' }, '보유주택이 없습니다');
  const options = Array.from({ length: MAX_PROPERTY_HOLDINGS }, () => el('option'));
  element.append(placeholder, ...options);
  return {
    element,
    setItems(items, selectedId, pendingId) {
      const next = propertyHoldingSelectValue(items, selectedId, pendingId);
      updatePropertyHoldingPlaceholder(placeholder, items.length, pendingId);
      updatePropertyHoldingOptions(options, items);
      element.value = next;
      return next;
    },
  };
}

function propertyHoldingSelectValue(
  items: readonly HousingPropertyHolding[],
  selectedId: string,
  pendingId: string | undefined,
): string {
  if (pendingId !== undefined) return pendingId;
  if (items.some((holding) => holding.id === selectedId)) return selectedId;
  return items[0]?.id ?? '';
}

function updatePropertyHoldingPlaceholder(
  placeholder: HTMLOptionElement,
  itemCount: number,
  pendingId: string | undefined,
): void {
  placeholder.hidden = pendingId === undefined && itemCount > 0;
  placeholder.disabled = pendingId === undefined && itemCount > 0;
  placeholder.value = pendingId ?? '';
  if (pendingId !== undefined) {
    placeholder.textContent = `응답 확인 대기 중인 보유주택 #${pendingId}`;
    return;
  }
  placeholder.textContent = itemCount === 0 ? '보유주택이 없습니다' : '보유주택을 선택하세요';
}

function updatePropertyHoldingOptions(
  options: readonly HTMLOptionElement[],
  items: readonly HousingPropertyHolding[],
): void {
  for (const [index, option] of options.entries()) {
    const holding = items[index];
    option.hidden = holding === undefined;
    option.disabled = holding === undefined;
    option.value = holding?.id ?? '';
    option.textContent = holding === undefined ? '' : propertyHoldingText(holding);
  }
}

function createFixedPropertySaleOrderSelect(): FixedPropertySaleOrderSelect {
  const element = el('select', {
    name: 'propertySaleOrderId',
    attrs: { 'aria-label': '변경하거나 취소할 활성 매도 주문' },
  });
  const placeholder = el('option', { value: '' }, '활성 매도 주문이 없습니다');
  const options = Array.from({ length: MAX_PROPERTY_HISTORY }, () => el('option'));
  element.append(placeholder, ...options);
  return {
    element,
    setItems(items, selectedId, pendingId) {
      const active = items.filter((order) => order.status === 'active');
      const next = propertySaleOrderSelectValue(active, selectedId, pendingId);
      updatePropertySaleOrderPlaceholder(placeholder, active.length, pendingId);
      updatePropertySaleOrderOptions(options, active);
      element.value = next;
      return next;
    },
  };
}

function propertySaleOrderSelectValue(
  active: readonly HousingPropertySaleOrderSummary[],
  selectedId: string,
  pendingId: string | undefined,
): string {
  if (pendingId !== undefined) return pendingId;
  if (active.some((order) => order.orderId === selectedId)) return selectedId;
  return active[0]?.orderId ?? '';
}

function updatePropertySaleOrderPlaceholder(
  placeholder: HTMLOptionElement,
  activeCount: number,
  pendingId: string | undefined,
): void {
  placeholder.hidden = pendingId === undefined && activeCount > 0;
  placeholder.disabled = pendingId === undefined && activeCount > 0;
  placeholder.value = pendingId ?? '';
  if (pendingId !== undefined) {
    placeholder.textContent = `응답 확인 대기 중인 주문 #${pendingId}`;
    return;
  }
  placeholder.textContent =
    activeCount === 0 ? '활성 매도 주문이 없습니다' : '활성 매도 주문을 선택하세요';
}

function updatePropertySaleOrderOptions(
  options: readonly HTMLOptionElement[],
  active: readonly HousingPropertySaleOrderSummary[],
): void {
  for (const [index, option] of options.entries()) {
    const order = active[index];
    option.hidden = order === undefined;
    option.disabled = order === undefined;
    option.value = order?.orderId ?? '';
    option.textContent = order === undefined ? '' : propertySaleOrderText(order);
  }
}

function createFixedPropertySaleOrderList(): FixedPropertySaleOrderList {
  const rows = Array.from({ length: MAX_PROPERTY_HISTORY }, () => {
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
        row.text.textContent = item === undefined ? '' : propertySaleOrderText(item);
      }
    },
  };
}

function createFixedPropertyTaxHoldingSelect(): FixedPropertyTaxHoldingSelect {
  const element = el('select', {
    name: 'propertyTaxHoldingId',
    attrs: { 'aria-label': '세금 이력을 볼 보유주택' },
  });
  const placeholder = el('option', { value: '' }, '세금 이력을 볼 주택이 없습니다');
  const options = Array.from({ length: MAX_PROPERTY_TAX_HOLDINGS }, () => el('option'));
  element.append(placeholder, ...options);
  return {
    element,
    setItems(holdings, saleOrders, selectedId) {
      const holdingIds = propertyTaxHoldingIds(holdings, saleOrders);
      const next = holdingIds.includes(selectedId) ? selectedId : (holdingIds[0] ?? '');
      updatePropertyTaxHoldingPlaceholder(placeholder, holdingIds.length);
      updatePropertyTaxHoldingOptions(options, holdingIds, holdings);
      element.value = next;
      return next;
    },
  };
}

function propertyTaxHoldingIds(
  holdings: readonly HousingPropertyHolding[],
  saleOrders: readonly HousingPropertySaleOrderSummary[],
): string[] {
  const ids = holdings.map((holding) => holding.id);
  for (const order of saleOrders) {
    if (!ids.includes(order.holdingId)) ids.push(order.holdingId);
  }
  return ids;
}

function updatePropertyTaxHoldingPlaceholder(
  placeholder: HTMLOptionElement,
  itemCount: number,
): void {
  placeholder.hidden = itemCount > 0;
  placeholder.disabled = itemCount > 0;
  placeholder.textContent =
    itemCount === 0 ? '세금 이력을 볼 주택이 없습니다' : '보유주택을 선택하세요';
}

function updatePropertyTaxHoldingOptions(
  options: readonly HTMLOptionElement[],
  holdingIds: readonly string[],
  holdings: readonly HousingPropertyHolding[],
): void {
  for (const [index, option] of options.entries()) {
    const holdingId = holdingIds[index];
    const holding = holdings.find((candidate) => candidate.id === holdingId);
    option.hidden = holdingId === undefined;
    option.disabled = holdingId === undefined;
    option.value = holdingId ?? '';
    option.textContent = propertyTaxHoldingOptionText(holdingId, holding);
  }
}

function propertyTaxHoldingOptionText(
  holdingId: string | undefined,
  holding: HousingPropertyHolding | undefined,
): string {
  if (holdingId === undefined) return '';
  return holding === undefined
    ? `매도 이력이 있는 보유주택 #${holdingId}`
    : propertyHoldingText(holding);
}

function createFixedPropertyTaxEventList(): FixedPropertyTaxEventList {
  const rows = Array.from({ length: MAX_PROPERTY_HISTORY }, () => {
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
        row.text.textContent = item === undefined ? '' : propertyTaxEventText(item);
      }
    },
  };
}

function propertySaleOrderText(order: HousingPropertySaleOrderSummary): string {
  const listing =
    order.revisionKind === 'listing' &&
    order.askingPriceKrw !== null &&
    order.referenceValueKrw !== null &&
    order.askingToReferencePpm !== null &&
    order.candidateGameDay !== null
      ? `주문가 ${formatWon(order.askingPriceKrw)} · 기준가 ${formatWon(order.referenceValueKrw)} · ${order.askingToReferencePpm.toLocaleString('ko-KR')}ppm · 후보 game day ${order.candidateGameDay}`
      : `game day ${order.cancelledGameDay ?? '-'} 취소 개정`;
  const rejection =
    order.rejectionReason === null
      ? ''
      : ` · 거절 ${propertySaleRejectionReasonText(order.rejectionReason)}`;
  const execution =
    order.execution === null
      ? ''
      : ` · 체결 ${formatWon(order.execution.grossSalePriceKrw)} · 거래비용 ${formatWon(order.execution.transactionCostKrw)} · 담보원금 ${formatWon(order.execution.mortgagePrincipalKrw)} · 담보수수료 ${formatWon(order.execution.mortgageFeeKrw)} · 양도세 ${formatWon(order.execution.capitalGainsTaxKrw)} · 순수령 ${formatWon(order.execution.walletProceedsKrw)} · 실현손익 ${formatWon(order.execution.realizedGainLossKrw)}`;
  return `주문 #${order.orderId} · 보유주택 #${order.holdingId} · 개정 ${order.revisionNo} · ${listing} · ${propertySaleOrderStatusText(order.status)}${rejection}${execution}`;
}

function propertySaleOrderStatusText(status: HousingPropertySaleOrderSummary['status']): string {
  switch (status) {
    case 'active':
      return '활성';
    case 'filled':
      return '체결';
    case 'cancelled':
      return '취소';
    case 'rejected':
      return '체결 거절';
  }
}

function propertySaleRejectionReasonText(
  reason: NonNullable<HousingPropertySaleOrderSummary['rejectionReason']>,
): string {
  switch (reason) {
    case 'mortgageNotPayable':
      return '담보대출 전액상환 불가';
    case 'insufficientProceeds':
      return '매각대금 부족';
    case 'policyUnsupported':
      return '정책 미지원';
  }
}

function propertyTaxEventText(event: HousingPropertyTaxEvent): string {
  const valuation =
    event.valuationGameDay === null ||
    event.valuationPriceIndexPpm === null ||
    event.officialValueKrw === null
      ? ''
      : ` · 평가 game day ${event.valuationGameDay} / 지수 ${event.valuationPriceIndexPpm.toLocaleString('ko-KR')}ppm / 공시가 ${formatWon(event.officialValueKrw)}`;
  const components = event.components
    .map(
      (component) =>
        `${component.componentKey}: 과세표준 ${formatWon(component.taxBaseKrw)}, 공제 ${formatWon(component.deductionKrw)}, 과세액 ${formatWon(component.taxableAmountKrw)}, 세율 ${component.ratePpm.toLocaleString('ko-KR')}ppm, 누진공제 ${formatWon(component.progressiveDeductionKrw)}, 세액 ${formatWon(component.amountKrw)}`,
    )
    .join(' / ');
  const payments = event.payments
    .map(
      (payment) =>
        `${payment.paymentNo}차 ${propertyTaxPaymentStatusText(payment.status)} · 납기 game day ${payment.dueGameDay} · ${formatWon(payment.amountKrw)} · 지갑 ${formatWon(payment.walletPaidKrw)} · 납세의무 ${formatWon(payment.taxObligationKrw)}${payment.paidGameDay === null ? '' : ` · 지급 game day ${payment.paidGameDay}`}`,
    )
    .join(' / ');
  const exclusions =
    event.exclusionCodes.length === 0 ? '제외 없음' : event.exclusionCodes.join(', ');
  return `세금 #${event.id} · ${propertyTaxKindText(event.kind)} · ${propertyTaxStatusText(event.status)} · 과세 game day ${event.taxableGameDay} / 산정 ${event.assessedGameDay} · ${event.policyKey} / ${event.ruleKey} / 법적 기준일 ${event.legalBasisDate} · 주택 수 ${event.householdHomeCount} · 총액 ${formatWon(event.totalTaxKrw)} · 과세표준 ${formatWon(event.taxBaseKrw)} / 공제 ${formatWon(event.deductionKrw)} / 과세액 ${formatWon(event.taxableAmountKrw)}${valuation} · 구성 [${components}] · 납부 [${payments}] · 제외 [${exclusions}]`;
}

function propertyTaxKindText(kind: HousingPropertyTaxEvent['kind']): string {
  switch (kind) {
    case 'acquisition':
      return '취득세';
    case 'annualHolding':
      return '연간 보유세';
    case 'capitalGains':
      return '양도소득세';
  }
}

function propertyTaxStatusText(status: HousingPropertyTaxEvent['status']): string {
  switch (status) {
    case 'scheduled':
      return '납부 예정';
    case 'partiallyPaid':
      return '일부 납부';
    case 'paid':
      return '납부 완료';
    case 'noPaymentRequired':
      return '납부 없음';
  }
}

function propertyTaxPaymentStatusText(
  status: HousingPropertyTaxEvent['payments'][number]['status'],
): string {
  switch (status) {
    case 'pending':
      return '예정';
    case 'applied':
      return '적용';
    case 'cancelled':
      return '취소';
  }
}

function createFixedRegionSelect(): FixedRegionSelect {
  const element = el('select', {
    name: 'region',
    attrs: { 'aria-label': '조회할 주거 지역' },
  });
  const placeholder = el('option', { value: '' }, '지역을 불러오는 중');
  const options = Array.from({ length: MAX_REGIONS }, () => el('option'));
  element.append(placeholder, ...options);

  return {
    element,
    setRegions(regions, selected) {
      const selectedAvailable =
        selected !== undefined && regions.some((region) => region.regionKey === selected);
      placeholder.hidden = selectedAvailable;
      placeholder.disabled = selectedAvailable;
      placeholder.textContent =
        regions.length === 0 ? '조회할 지역이 없습니다' : '지역을 선택하세요';
      for (const [index, option] of options.entries()) {
        const region = regions[index];
        option.hidden = region === undefined;
        option.disabled = region === undefined;
        option.value = region?.regionKey ?? '';
        option.textContent =
          region === undefined ? '' : `${region.displayName} (${region.regionKey})`;
      }
      element.value = selectedAvailable && selected !== undefined ? selected : '';
    },
  };
}

function createFixedListingTable(body: HTMLTableSectionElement): FixedListingTable {
  const rows = Array.from({ length: MAX_LISTINGS }, createListingRow);
  body.append(...rows.map((row) => row.element));
  return {
    setListings(listings) {
      for (const [index, row] of rows.entries()) row.setListing(listings[index]);
    },
  };
}

function createListingRow(): ListingRow {
  const id = el('td');
  const region = el('td');
  const type = el('td');
  const area = el('td');
  const availability = el('td');
  const offers = el('td');
  const element = el('tr', { attrs: { hidden: '' } }, id, region, type, area, availability, offers);

  return {
    element,
    setListing(listing) {
      element.hidden = listing === undefined;
      id.textContent = listing?.id ?? '';
      region.textContent = listing?.regionKey ?? '';
      type.textContent = listing === undefined ? '' : PROPERTY_TYPE_LABEL[listing.propertyType];
      area.textContent =
        listing === undefined
          ? ''
          : `${listing.exclusiveAreaSquareMeters.toLocaleString('ko-KR')}㎡`;
      availability.textContent =
        listing === undefined
          ? ''
          : `game day ${listing.availableFromGameDay}~${listing.availableToGameDay}`;
      offers.textContent = listing === undefined ? '' : listing.offers.map(offerText).join(' · ');
    },
  };
}

function createFixedMovingCostList(): FixedMovingCostList {
  const rows = Array.from({ length: MAX_REGIONS }, () => {
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
        row.text.textContent =
          item === undefined
            ? ''
            : `${REGION_LABEL[item.regionKey]}: ${formatWon(item.movingCostKrw)}`;
      }
    },
  };
}

function createFixedLeaseArrearList(): FixedLeaseArrearList {
  const rows = Array.from({ length: MAX_LEASE_ARREARS }, () => {
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
        row.text.textContent = item === undefined ? '' : leaseArrearText(item);
      }
    },
  };
}

function createFixedLeaseArrearSelect(): FixedLeaseArrearSelect {
  const element = el('select', {
    name: 'arrearId',
    attrs: { 'aria-label': '상환할 월세 연체' },
  });
  const placeholder = el('option', { value: '' }, '월세 연체가 없습니다');
  const options = Array.from({ length: MAX_LEASE_ARREARS }, () => el('option'));
  element.append(placeholder, ...options);
  return {
    element,
    setItems(items, pendingId) {
      const previous = element.value;
      updateLeaseArrearPlaceholder(placeholder, items.length, pendingId);
      updateLeaseArrearOptions(options, items);
      const next = selectedLeaseArrearId(items, previous, pendingId);
      element.value = next;
      return next;
    },
  };
}

function updateLeaseArrearPlaceholder(
  placeholder: HTMLOptionElement,
  itemCount: number,
  pendingId: string | undefined,
): void {
  const hasItems = itemCount > 0;
  placeholder.hidden = pendingId === undefined && hasItems;
  placeholder.disabled = pendingId === undefined && hasItems;
  placeholder.value = pendingId ?? '';
  if (pendingId !== undefined) {
    placeholder.textContent = `응답 확인 대기 중인 연체 #${pendingId}`;
    return;
  }
  placeholder.textContent = hasItems ? '월세 연체를 선택하세요' : '월세 연체가 없습니다';
}

function updateLeaseArrearOptions(
  options: readonly HTMLOptionElement[],
  items: readonly HousingLeaseArrear[],
): void {
  for (const [index, option] of options.entries()) {
    const item = items[index];
    option.hidden = item === undefined;
    option.disabled = item === undefined;
    option.value = item?.id ?? '';
    option.textContent = item === undefined ? '' : leaseArrearText(item);
  }
}

function selectedLeaseArrearId(
  items: readonly HousingLeaseArrear[],
  previous: string,
  pendingId: string | undefined,
): string {
  if (pendingId !== undefined) return pendingId;
  return items.some((item) => item.id === previous) ? previous : (items[0]?.id ?? '');
}

function leaseArrearText(arrear: HousingLeaseArrear): string {
  const month = `${arrear.dueYearMonth.year}-${String(arrear.dueYearMonth.month).padStart(2, '0')}`;
  return `#${arrear.id} · ${month} · 계약 #${arrear.leaseId} / 청구 #${arrear.rentChargeId} · 원금 ${formatWon(arrear.originalKrw)} · 지급 ${formatWon(arrear.paidKrw)} · 남음 ${formatWon(arrear.remainingKrw)} · 생성 game day ${arrear.createdGameDay}`;
}

function selectedLeaseArrear(
  select: HTMLSelectElement,
  arrears: readonly HousingLeaseArrear[],
): HousingLeaseArrear | undefined {
  return arrears.find((arrear) => arrear.id === select.value);
}

function syncLeaseArrearAmountLimit(
  select: HTMLSelectElement,
  input: HTMLInputElement,
  arrears: readonly HousingLeaseArrear[],
): void {
  const arrear = selectedLeaseArrear(select, arrears);
  if (arrear === undefined) input.removeAttribute('max');
  else input.max = String(arrear.remainingKrw);
}

function positiveIntegerOf(raw: string, label: string): number {
  const value = Number(raw);
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new HousingFormError(`${label}은 1원 이상의 정수로 입력해 주세요.`);
  }
  return value;
}

function createFixedLeaseSelect(): FixedLeaseSelect {
  const element = el('select', {
    name: 'listingId',
    attrs: { 'aria-label': '이사할 임대차 조건' },
  });
  const placeholder = el('option', { value: '' }, '임대차 조건을 불러오는 중');
  const options = Array.from({ length: MAX_LEASE_OFFERS }, () => el('option'));
  element.append(placeholder, ...options);
  return {
    element,
    setListings(listings, capability, selectedKey, pending) {
      const leaseOptions = listings.flatMap((listing) =>
        tenantOffersOf(listing).flatMap((offer) =>
          capabilitySupportsOffer(capability, offer.kind) ? [{ listing, offer }] : [],
        ),
      );
      const selectedAvailable = leaseOptions.some(
        (option) => leaseKey(option.listing.id, option.offer.kind) === selectedKey,
      );
      updateLeasePlaceholder(placeholder, leaseOptions.length, selectedAvailable, pending);
      for (const [index, option] of options.entries()) {
        updateLeaseOption(option, leaseOptions[index]);
      }
      const next = selectedLeaseKey(selectedKey, selectedAvailable, pending);
      element.value = next;
      return next;
    },
  };
}

function updateLeasePlaceholder(
  placeholder: HTMLOptionElement,
  offerCount: number,
  selectedAvailable: boolean,
  pending: HousingLeaseRequest | HousingLeaseDepositLoanQuoteRequest | undefined,
): void {
  const pendingAvailable = pending !== undefined;
  placeholder.hidden = selectedAvailable && !pendingAvailable;
  placeholder.disabled = !pendingAvailable;
  placeholder.value = pending === undefined ? '' : leaseKey(pending.listingId, pending.offerKind);
  if (pending !== undefined) {
    placeholder.textContent = `응답 확인 대기 중인 매물 #${pending.listingId} ${offerKindLabel(pending.offerKind)}`;
  } else {
    placeholder.textContent =
      offerCount === 0
        ? '현재 선택 가능한 임대차 조건이 없습니다'
        : '전세 또는 월세 조건을 선택하세요';
  }
}

function updateLeaseOption(
  option: HTMLOptionElement,
  leaseOption: { readonly listing: HousingListing; readonly offer: TenantHousingOffer } | undefined,
): void {
  const unavailable = leaseOption === undefined;
  option.hidden = unavailable;
  option.disabled = unavailable;
  option.value =
    leaseOption === undefined ? '' : leaseKey(leaseOption.listing.id, leaseOption.offer.kind);
  option.textContent =
    leaseOption === undefined ? '' : leaseOptionText(leaseOption.listing, leaseOption.offer);
}

function selectedLeaseKey(
  selectedKey: string,
  selectedAvailable: boolean,
  pending: HousingLeaseRequest | HousingLeaseDepositLoanQuoteRequest | undefined,
): string {
  if (pending !== undefined) return leaseKey(pending.listingId, pending.offerKind);
  return selectedAvailable ? selectedKey : '';
}

function offerText(offer: HousingOffer): string {
  switch (offer.kind) {
    case 'sale':
      return `매매 ${formatWon(offer.priceKrw)}`;
    case 'jeonse':
      return `전세 보증금 ${formatWon(offer.depositKrw)}`;
    case 'monthlyRent':
      return `월세 보증금 ${formatWon(offer.depositKrw)}, 월 ${formatWon(offer.monthlyRentKrw)}`;
  }
}
