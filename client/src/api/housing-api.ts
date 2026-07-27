import type { z } from 'zod';

import { type HttpClient, HttpError, type ResponseDecoder } from '../lib/http/index.js';
import {
  type HousingCurrentLeaseResponse,
  HousingCurrentLeaseResponseSchema,
  type HousingLeaseArrearPaymentRequest,
  HousingLeaseArrearPaymentRequestSchema,
  type HousingLeaseArrearPaymentResponse,
  HousingLeaseArrearPaymentResponseSchema,
  type HousingLeaseDepositLoanQuoteRequest,
  HousingLeaseDepositLoanQuoteRequestSchema,
  type HousingLeaseDepositLoanQuoteResponse,
  HousingLeaseDepositLoanQuoteResponseSchema,
  type HousingLeaseRequest,
  HousingLeaseRequestSchema,
  type HousingLeaseResponse,
  HousingLeaseResponseSchema,
  type HousingListingsQuery,
  HousingListingsQuerySchema,
  type HousingListingsResponse,
  HousingListingsResponseSchema,
  type HousingMortgageQuoteRequest,
  HousingMortgageQuoteRequestSchema,
  type HousingMortgageQuoteResponse,
  HousingMortgageQuoteResponseSchema,
  type HousingPropertyHistoryQuery,
  HousingPropertyHistoryQuerySchema,
  type HousingPropertyHoldingsResponse,
  HousingPropertyHoldingsResponseSchema,
  type HousingPropertySaleOrderCancellationResponse,
  HousingPropertySaleOrderCancellationResponseSchema,
  type HousingPropertySaleOrderCancelRequest,
  HousingPropertySaleOrderCancelRequestSchema,
  type HousingPropertySaleOrderCreateRequest,
  HousingPropertySaleOrderCreateRequestSchema,
  type HousingPropertySaleOrderListingResponse,
  HousingPropertySaleOrderListingResponseSchema,
  type HousingPropertySaleOrderRepriceRequest,
  HousingPropertySaleOrderRepriceRequestSchema,
  type HousingPropertySaleOrdersResponse,
  HousingPropertySaleOrdersResponseSchema,
  type HousingPropertyTaxEventsResponse,
  HousingPropertyTaxEventsResponseSchema,
  type HousingPurchaseRequest,
  HousingPurchaseRequestSchema,
  type HousingPurchaseResponse,
  HousingPurchaseResponseSchema,
  type LifeFailureCode,
  LifeFailureSchema,
  ResourceIdSchema,
} from './contracts.js';
import { asDecoder } from './zod-adapters.js';

export interface HousingApi {
  listListings(
    query?: HousingListingsQuery,
    signal?: AbortSignal,
  ): Promise<HousingListingsResponse>;
  getCurrentLease(signal?: AbortSignal): Promise<HousingCurrentLeaseResponse>;
  getHoldings(signal?: AbortSignal): Promise<HousingPropertyHoldingsResponse>;
  listPropertySales(
    query?: HousingPropertyHistoryQuery,
    signal?: AbortSignal,
  ): Promise<HousingPropertySaleOrdersResponse>;
  listPropertyTaxEvents(
    holdingId: string,
    query?: HousingPropertyHistoryQuery,
    signal?: AbortSignal,
  ): Promise<HousingPropertyTaxEventsResponse>;
  quoteLeaseDepositLoan(
    request: HousingLeaseDepositLoanQuoteRequest,
    signal?: AbortSignal,
  ): Promise<HousingLeaseDepositLoanQuoteResponse>;
  quoteMortgage(
    request: HousingMortgageQuoteRequest,
    signal?: AbortSignal,
  ): Promise<HousingMortgageQuoteResponse>;
  startLease(request: HousingLeaseRequest, signal?: AbortSignal): Promise<HousingLeaseResponse>;
  purchase(request: HousingPurchaseRequest, signal?: AbortSignal): Promise<HousingPurchaseResponse>;
  createPropertySaleOrder(
    request: HousingPropertySaleOrderCreateRequest,
    signal?: AbortSignal,
  ): Promise<HousingPropertySaleOrderListingResponse>;
  repricePropertySaleOrder(
    orderId: string,
    request: HousingPropertySaleOrderRepriceRequest,
    signal?: AbortSignal,
  ): Promise<HousingPropertySaleOrderListingResponse>;
  cancelPropertySaleOrder(
    orderId: string,
    request: HousingPropertySaleOrderCancelRequest,
    signal?: AbortSignal,
  ): Promise<HousingPropertySaleOrderCancellationResponse>;
  payLeaseArrear(
    arrearId: string,
    request: HousingLeaseArrearPaymentRequest,
    signal?: AbortSignal,
  ): Promise<HousingLeaseArrearPaymentResponse>;
}

export interface HousingApiDeps {
  readonly http: HttpClient;
}

/** A validated housing-query rejection, independent of its transport status. */
export class HousingQueryError extends Error {
  constructor(
    readonly code: LifeFailureCode,
    message: string,
  ) {
    super(message);
    this.name = 'HousingQueryError';
  }
}

/** A validated lease-command rejection whose outcome is known. */
export class HousingCommandError extends Error {
  constructor(
    readonly code: LifeFailureCode,
    message: string,
  ) {
    super(message);
    this.name = 'HousingCommandError';
  }
}

/** Reads server-authoritative housing indexes and bounded monthly listings. */
export function createHousingApi(deps: HousingApiDeps): HousingApi {
  return {
    listListings(query, signal) {
      const parsedQuery = HousingListingsQuerySchema.parse(query ?? {});
      const params = new URLSearchParams();
      if (parsedQuery.region !== undefined) params.set('region', parsedQuery.region);
      const suffix = params.size === 0 ? '' : `?${params.toString()}`;
      return requestHousingQuery(() =>
        deps.http.get(
          `/api/housing/listings${suffix}`,
          housingListingsDecoder(parsedQuery),
          signal === undefined ? undefined : { signal },
        ),
      );
    },
    getCurrentLease(signal) {
      return requestHousingQuery(() =>
        deps.http.get(
          '/api/housing/leases/current',
          asDecoder(HousingCurrentLeaseResponseSchema),
          signal === undefined ? undefined : { signal },
        ),
      );
    },
    getHoldings(signal) {
      return requestHousingQuery(() =>
        deps.http.get(
          '/api/housing/holdings',
          asDecoder(HousingPropertyHoldingsResponseSchema),
          signal === undefined ? undefined : { signal },
        ),
      );
    },
    listPropertySales(query, signal) {
      const parsedQuery = HousingPropertyHistoryQuerySchema.parse(query ?? {});
      return requestHousingQuery(() =>
        deps.http.get(
          `/api/housing/sales${propertyHistorySuffix(parsedQuery)}`,
          asDecoder(HousingPropertySaleOrdersResponseSchema),
          signal === undefined ? undefined : { signal },
        ),
      );
    },
    listPropertyTaxEvents(holdingId, query, signal) {
      const id = ResourceIdSchema.parse(holdingId);
      const parsedQuery = HousingPropertyHistoryQuerySchema.parse(query ?? {});
      return requestHousingQuery(() =>
        deps.http.get(
          `/api/housing/holdings/${id}/tax-events${propertyHistorySuffix(parsedQuery)}`,
          asDecoder(
            HousingPropertyTaxEventsResponseSchema.superRefine((response, context) => {
              if (response.holdingId !== id) {
                context.addIssue({
                  code: 'custom',
                  path: ['holdingId'],
                  message: 'property tax history does not match the requested holding',
                });
              }
            }),
          ),
          signal === undefined ? undefined : { signal },
        ),
      );
    },
    quoteLeaseDepositLoan(request, signal) {
      const body = HousingLeaseDepositLoanQuoteRequestSchema.parse(request);
      return requestHousingCommand(() =>
        deps.http.post(
          '/api/housing/lease-deposit-loan-quotes',
          body,
          housingLeaseDepositLoanQuoteDecoder(body),
          signal === undefined ? undefined : { signal },
        ),
      );
    },
    quoteMortgage(request, signal) {
      const body = HousingMortgageQuoteRequestSchema.parse(request);
      return requestHousingCommand(() =>
        deps.http.post(
          '/api/housing/mortgage-quotes',
          body,
          housingMortgageQuoteDecoder(body),
          signal === undefined ? undefined : { signal },
        ),
      );
    },
    startLease(request, signal) {
      const body = HousingLeaseRequestSchema.parse(request);
      return requestHousingCommand(() =>
        deps.http.post(
          '/api/housing/leases',
          body,
          housingLeaseDecoder(body),
          signal === undefined ? undefined : { signal },
        ),
      );
    },
    purchase(request, signal) {
      const body = HousingPurchaseRequestSchema.parse(request);
      return requestHousingCommand(() =>
        deps.http.post(
          '/api/housing/purchases',
          body,
          housingPurchaseDecoder(body),
          signal === undefined ? undefined : { signal },
        ),
      );
    },
    createPropertySaleOrder(request, signal) {
      const body = HousingPropertySaleOrderCreateRequestSchema.parse(request);
      return requestHousingCommand(() =>
        deps.http.post(
          '/api/housing/sales',
          body,
          propertySaleOrderListingDecoder(body, body.holdingId),
          signal === undefined ? undefined : { signal },
        ),
      );
    },
    repricePropertySaleOrder(orderId, request, signal) {
      const id = ResourceIdSchema.parse(orderId);
      const body = HousingPropertySaleOrderRepriceRequestSchema.parse(request);
      return requestHousingCommand(() =>
        deps.http.post(
          `/api/housing/sales/${id}/reprice`,
          body,
          propertySaleOrderListingDecoder(body, undefined, id),
          signal === undefined ? undefined : { signal },
        ),
      );
    },
    cancelPropertySaleOrder(orderId, request, signal) {
      const id = ResourceIdSchema.parse(orderId);
      const body = HousingPropertySaleOrderCancelRequestSchema.parse(request);
      return requestHousingCommand(() =>
        deps.http.post(
          `/api/housing/sales/${id}/cancel`,
          body,
          propertySaleOrderCancellationDecoder(body, id),
          signal === undefined ? undefined : { signal },
        ),
      );
    },
    payLeaseArrear(arrearId, request, signal) {
      const id = ResourceIdSchema.parse(arrearId);
      const body = HousingLeaseArrearPaymentRequestSchema.parse(request);
      return requestHousingCommand(() =>
        deps.http.post(
          `/api/housing/lease-arrears/${id}/payments`,
          body,
          housingLeaseArrearPaymentDecoder(id, body),
          signal === undefined ? undefined : { signal },
        ),
      );
    },
  };
}

type PropertySaleListingRequest =
  | HousingPropertySaleOrderCreateRequest
  | HousingPropertySaleOrderRepriceRequest;

function propertyHistorySuffix(query: HousingPropertyHistoryQuery): string {
  const params = new URLSearchParams();
  if (query.before !== undefined) params.set('before', query.before);
  if (query.limit !== undefined) params.set('limit', String(query.limit));
  return params.size === 0 ? '' : `?${params.toString()}`;
}

function propertySaleOrderListingDecoder(
  request: PropertySaleListingRequest,
  expectedHoldingId?: string,
  expectedOrderId?: string,
): ResponseDecoder<HousingPropertySaleOrderListingResponse> {
  return asDecoder(
    HousingPropertySaleOrderListingResponseSchema.superRefine((response, context) => {
      if (
        (expectedHoldingId !== undefined && response.result.holdingId !== expectedHoldingId) ||
        (expectedOrderId !== undefined && response.result.orderId !== expectedOrderId)
      ) {
        context.addIssue({
          code: 'custom',
          path: ['result'],
          message: 'property sale order result does not match the requested resource',
        });
      }
      refinePropertySaleCommandCursor(response, request, context);
      if (!response.replayed && response.result.candidateGameDay <= request.expectedGameDay) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'candidateGameDay'],
          message: 'a new property sale candidate must follow the listing game day',
        });
      }
    }),
  );
}

function propertySaleOrderCancellationDecoder(
  request: HousingPropertySaleOrderCancelRequest,
  expectedOrderId: string,
): ResponseDecoder<HousingPropertySaleOrderCancellationResponse> {
  return asDecoder(
    HousingPropertySaleOrderCancellationResponseSchema.superRefine((response, context) => {
      if (response.result.orderId !== expectedOrderId) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'orderId'],
          message: 'property sale cancellation does not match the requested order',
        });
      }
      refinePropertySaleCommandCursor(response, request, context);
      if (!response.replayed && response.result.cancelledGameDay !== request.expectedGameDay) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'cancelledGameDay'],
          message: 'a new property sale cancellation must use the command game day',
        });
      }
    }),
  );
}

function refinePropertySaleCommandCursor(
  response: HousingPropertySaleOrderListingResponse | HousingPropertySaleOrderCancellationResponse,
  request: PropertySaleListingRequest | HousingPropertySaleOrderCancelRequest,
  context: z.RefinementCtx,
): void {
  const { snapshot } = response;
  if (
    snapshot.runRevision !== request.expectedRunRevision ||
    snapshot.gameDay < request.expectedGameDay ||
    BigInt(snapshot.stateRevision) < BigInt(request.expectedStateRevision) + 1n
  ) {
    context.addIssue({
      code: 'custom',
      path: ['snapshot'],
      message: 'property sale response does not advance from the requested cursor',
    });
  }
  if (
    !response.replayed &&
    (snapshot.gameDay !== request.expectedGameDay ||
      BigInt(snapshot.stateRevision) !== BigInt(request.expectedStateRevision) + 1n)
  ) {
    context.addIssue({
      code: 'custom',
      path: ['snapshot', 'stateRevision'],
      message: 'a new property sale command must advance state exactly once',
    });
  }
}

function housingLeaseDecoder(request: HousingLeaseRequest): ResponseDecoder<HousingLeaseResponse> {
  return asDecoder(
    HousingLeaseResponseSchema.superRefine((response, context) => {
      const { result, snapshot } = response;
      if (result.listingId !== request.listingId || result.offerKind !== request.offerKind) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'listingId'],
          message: 'lease result does not match the requested listing offer',
        });
      }
      const requestedLoanQuoteId = 'loanQuoteId' in request ? request.loanQuoteId : undefined;
      const executedLoanQuoteId = result.depositLoanExecution?.quoteId;
      if (requestedLoanQuoteId !== executedLoanQuoteId) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'depositLoanExecution'],
          message: 'lease deposit-loan execution does not match the financed request',
        });
      }
      if (
        result.effectiveFromGameDay !== request.expectedGameDay ||
        snapshot.runRevision !== request.expectedRunRevision ||
        snapshot.gameDay < request.expectedGameDay ||
        BigInt(snapshot.stateRevision) < BigInt(request.expectedStateRevision) + 1n
      ) {
        context.addIssue({
          code: 'custom',
          path: ['snapshot'],
          message: 'lease response does not advance from the requested cursor',
        });
      }
      if (
        !response.replayed &&
        (snapshot.gameDay !== request.expectedGameDay ||
          BigInt(snapshot.stateRevision) !== BigInt(request.expectedStateRevision) + 1n)
      ) {
        context.addIssue({
          code: 'custom',
          path: ['snapshot', 'stateRevision'],
          message: 'a new lease command must advance state exactly once without advancing time',
        });
      }
    }),
  );
}

function housingLeaseDepositLoanQuoteDecoder(
  request: HousingLeaseDepositLoanQuoteRequest,
): ResponseDecoder<HousingLeaseDepositLoanQuoteResponse> {
  return asDecoder(
    HousingLeaseDepositLoanQuoteResponseSchema.superRefine((response, context) => {
      const { result, snapshot } = response;
      if (
        result.listingId !== request.listingId ||
        result.offerKind !== request.offerKind ||
        result.productVersionId !== request.productVersionId ||
        result.requestedPrincipalKrw !== request.principalKrw ||
        result.createdGameDay !== request.expectedGameDay
      ) {
        context.addIssue({
          code: 'custom',
          path: ['result'],
          message: 'lease-deposit loan quote does not match its request',
        });
      }
      if (
        snapshot.runRevision !== request.expectedRunRevision ||
        snapshot.gameDay < request.expectedGameDay ||
        BigInt(snapshot.stateRevision) < BigInt(request.expectedStateRevision)
      ) {
        context.addIssue({
          code: 'custom',
          path: ['snapshot'],
          message: 'lease-deposit loan quote snapshot precedes its requested cursor',
        });
      }
      if (
        !response.replayed &&
        (snapshot.gameDay !== request.expectedGameDay ||
          snapshot.stateRevision !== request.expectedStateRevision)
      ) {
        context.addIssue({
          code: 'custom',
          path: ['snapshot', 'stateRevision'],
          message: 'a new lease-deposit loan quote cannot advance game state',
        });
      }
    }),
  );
}

function housingMortgageQuoteDecoder(
  request: HousingMortgageQuoteRequest,
): ResponseDecoder<HousingMortgageQuoteResponse> {
  return asDecoder(
    HousingMortgageQuoteResponseSchema.superRefine((response, context) => {
      const { result, snapshot } = response;
      if (
        result.listingId !== request.listingId ||
        result.productVersionId !== request.productVersionId ||
        result.requestedPrincipalKrw !== request.principalKrw ||
        result.createdGameDay !== request.expectedGameDay
      ) {
        context.addIssue({
          code: 'custom',
          path: ['result'],
          message: 'mortgage quote does not match its listing, product, principal, or game day',
        });
      }
      if (
        snapshot.runRevision !== request.expectedRunRevision ||
        snapshot.gameDay < request.expectedGameDay ||
        BigInt(snapshot.stateRevision) < BigInt(request.expectedStateRevision)
      ) {
        context.addIssue({
          code: 'custom',
          path: ['snapshot'],
          message: 'mortgage quote snapshot precedes its requested cursor',
        });
      }
      if (
        !response.replayed &&
        (snapshot.gameDay !== request.expectedGameDay ||
          snapshot.stateRevision !== request.expectedStateRevision)
      ) {
        context.addIssue({
          code: 'custom',
          path: ['snapshot', 'stateRevision'],
          message: 'a new mortgage quote cannot advance game state',
        });
      }
    }),
  );
}

function housingPurchaseDecoder(
  request: HousingPurchaseRequest,
): ResponseDecoder<HousingPurchaseResponse> {
  return asDecoder(
    HousingPurchaseResponseSchema.superRefine((response, context) => {
      const { result, snapshot } = response;
      if (
        result.listingId !== request.listingId ||
        (result.mortgageExecution?.quoteId ?? null) !== request.mortgageQuoteId ||
        result.effectiveFromGameDay !== request.expectedGameDay
      ) {
        context.addIssue({
          code: 'custom',
          path: ['result'],
          message: 'property purchase result does not match its listing, quote, or game day',
        });
      }
      if (
        snapshot.runRevision !== request.expectedRunRevision ||
        snapshot.gameDay < request.expectedGameDay ||
        BigInt(snapshot.stateRevision) < BigInt(request.expectedStateRevision) + 1n
      ) {
        context.addIssue({
          code: 'custom',
          path: ['snapshot'],
          message: 'property purchase response does not advance from the requested cursor',
        });
      }
      if (
        !response.replayed &&
        (snapshot.gameDay !== request.expectedGameDay ||
          BigInt(snapshot.stateRevision) !== BigInt(request.expectedStateRevision) + 1n)
      ) {
        context.addIssue({
          code: 'custom',
          path: ['snapshot', 'stateRevision'],
          message: 'a new property purchase must advance state exactly once without advancing time',
        });
      }
    }),
  );
}

function housingLeaseArrearPaymentDecoder(
  arrearId: string,
  request: HousingLeaseArrearPaymentRequest,
): ResponseDecoder<HousingLeaseArrearPaymentResponse> {
  return asDecoder(
    HousingLeaseArrearPaymentResponseSchema.superRefine((response, context) => {
      if (response.result.arrearId !== arrearId) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'arrearId'],
          message: 'lease-arrear payment result does not match the path arrear',
        });
      }
      if (response.result.paidKrw !== request.amountKrw) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'paidKrw'],
          message: 'lease-arrear payment result does not match the submitted amount',
        });
      }
      const { snapshot } = response;
      if (
        snapshot.runRevision !== request.expectedRunRevision ||
        snapshot.gameDay < request.expectedGameDay ||
        BigInt(snapshot.stateRevision) < BigInt(request.expectedStateRevision) + 1n
      ) {
        context.addIssue({
          code: 'custom',
          path: ['snapshot'],
          message: 'lease-arrear payment response does not advance from the requested cursor',
        });
      }
      if (
        !response.replayed &&
        (snapshot.gameDay !== request.expectedGameDay ||
          BigInt(snapshot.stateRevision) !== BigInt(request.expectedStateRevision) + 1n)
      ) {
        context.addIssue({
          code: 'custom',
          path: ['snapshot', 'stateRevision'],
          message:
            'a new lease-arrear payment must advance state exactly once without advancing time',
        });
      }
    }),
  );
}

function housingListingsDecoder(
  query: HousingListingsQuery,
): ResponseDecoder<HousingListingsResponse> {
  return asDecoder(
    HousingListingsResponseSchema.superRefine((response, context) => {
      const expectedRegion = query.region ?? response.residenceRegionKey;
      if (response.selectedRegionKey !== expectedRegion) {
        context.addIssue({
          code: 'custom',
          path: ['selectedRegionKey'],
          message: 'housing response does not match the requested or residence region',
        });
      }
    }),
  );
}

async function requestHousingQuery<T>(request: () => Promise<T>): Promise<T> {
  try {
    return await request();
  } catch (error) {
    const failure = toHousingFailure(error);
    const domain =
      failure === undefined ? undefined : new HousingQueryError(failure.code, failure.message);
    if (domain !== undefined) throw domain;
    throw error;
  }
}

async function requestHousingCommand<T>(request: () => Promise<T>): Promise<T> {
  try {
    return await request();
  } catch (error) {
    const failure = toHousingFailure(error);
    const domain =
      failure === undefined ? undefined : new HousingCommandError(failure.code, failure.message);
    if (domain !== undefined) throw domain;
    throw error;
  }
}

function toHousingFailure(error: unknown): { code: LifeFailureCode; message: string } | undefined {
  if (!(error instanceof HttpError) || error.status >= 500) return undefined;
  const parsed = LifeFailureSchema.safeParse(error.body);
  if (!parsed.success) return undefined;
  return parsed.data;
}
