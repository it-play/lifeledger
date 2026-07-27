import type {
  GameSnapshot,
  HousingLeaseArrearPaymentDraft,
  HousingLeaseArrearPaymentRequest,
  HousingLeaseDepositLoanQuoteDraft,
  HousingLeaseDepositLoanQuoteRequest,
  HousingLeaseDraft,
  HousingLeaseRequest,
  HousingMortgageQuoteDraft,
  HousingMortgageQuoteRequest,
  HousingPropertySaleOrderCancelDraft,
  HousingPropertySaleOrderCancelRequest,
  HousingPropertySaleOrderCreateDraft,
  HousingPropertySaleOrderCreateRequest,
  HousingPropertySaleOrderRepriceDraft,
  HousingPropertySaleOrderRepriceRequest,
  HousingPurchaseDraft,
  HousingPurchaseRequest,
} from '../../api/contracts.js';

export type HousingCommandCursorSource = Pick<
  GameSnapshot,
  'runRevision' | 'stateRevision' | 'gameDay'
>;

export interface HousingLeaseRetryPolicyDeps {
  readonly createCommandId: () => string;
}

export interface HousingLeaseRetryPolicy {
  select(snapshot: HousingCommandCursorSource, draft: HousingLeaseDraft): HousingLeaseRequest;
  pending(runRevision: number): HousingLeaseRequest | undefined;
  complete(request: HousingLeaseRequest): void;
  fail(request: HousingLeaseRequest, error: unknown): void;
}

export interface HousingLeaseDepositLoanQuoteRetryPolicy {
  select(
    snapshot: HousingCommandCursorSource,
    draft: HousingLeaseDepositLoanQuoteDraft,
  ): HousingLeaseDepositLoanQuoteRequest;
  pending(runRevision: number): HousingLeaseDepositLoanQuoteRequest | undefined;
  complete(request: HousingLeaseDepositLoanQuoteRequest): void;
  fail(request: HousingLeaseDepositLoanQuoteRequest, error: unknown): void;
}

export interface HousingMortgageQuoteRetryPolicy {
  select(
    snapshot: HousingCommandCursorSource,
    draft: HousingMortgageQuoteDraft,
  ): HousingMortgageQuoteRequest;
  pending(runRevision: number): HousingMortgageQuoteRequest | undefined;
  complete(request: HousingMortgageQuoteRequest): void;
  fail(request: HousingMortgageQuoteRequest, error: unknown): void;
}

export interface HousingPurchaseRetryPolicy {
  select(snapshot: HousingCommandCursorSource, draft: HousingPurchaseDraft): HousingPurchaseRequest;
  pending(runRevision: number): HousingPurchaseRequest | undefined;
  complete(request: HousingPurchaseRequest): void;
  fail(request: HousingPurchaseRequest, error: unknown): void;
}

export interface HousingPropertySaleOrderCreateRetryPolicy {
  select(
    snapshot: HousingCommandCursorSource,
    draft: HousingPropertySaleOrderCreateDraft,
  ): HousingPropertySaleOrderCreateRequest;
  pending(runRevision: number): HousingPropertySaleOrderCreateRequest | undefined;
  complete(request: HousingPropertySaleOrderCreateRequest): void;
  fail(request: HousingPropertySaleOrderCreateRequest, error: unknown): void;
}

export interface HousingPropertySaleOrderRepriceCommand {
  readonly orderId: string;
  readonly request: HousingPropertySaleOrderRepriceRequest;
}

export interface HousingPropertySaleOrderRepriceRetryPolicy {
  select(
    snapshot: HousingCommandCursorSource,
    draft: HousingPropertySaleOrderRepriceDraft,
  ): HousingPropertySaleOrderRepriceCommand;
  pending(runRevision: number): HousingPropertySaleOrderRepriceCommand | undefined;
  complete(command: HousingPropertySaleOrderRepriceCommand): void;
  fail(command: HousingPropertySaleOrderRepriceCommand, error: unknown): void;
}

export interface HousingPropertySaleOrderCancelCommand {
  readonly orderId: string;
  readonly request: HousingPropertySaleOrderCancelRequest;
}

export interface HousingPropertySaleOrderCancelRetryPolicy {
  select(
    snapshot: HousingCommandCursorSource,
    draft: HousingPropertySaleOrderCancelDraft,
  ): HousingPropertySaleOrderCancelCommand;
  pending(runRevision: number): HousingPropertySaleOrderCancelCommand | undefined;
  complete(command: HousingPropertySaleOrderCancelCommand): void;
  fail(command: HousingPropertySaleOrderCancelCommand, error: unknown): void;
}

export interface HousingLeaseArrearPaymentCommand {
  readonly arrearId: string;
  readonly request: HousingLeaseArrearPaymentRequest;
}

export interface HousingLeaseArrearPaymentRetryPolicy {
  select(
    snapshot: HousingCommandCursorSource,
    draft: HousingLeaseArrearPaymentDraft,
  ): HousingLeaseArrearPaymentCommand;
  pending(runRevision: number): HousingLeaseArrearPaymentCommand | undefined;
  complete(command: HousingLeaseArrearPaymentCommand): void;
  fail(command: HousingLeaseArrearPaymentCommand, error: unknown): void;
}
