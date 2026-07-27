import type {
  HousingLeaseArrearPaymentDraft,
  HousingLeaseArrearPaymentRequest,
  HousingLeaseDepositLoanQuoteDraft,
  HousingLeaseDepositLoanQuoteRequest,
  HousingLeaseDraft,
  HousingLeaseRequest,
  HousingMortgageQuoteDraft,
  HousingMortgageQuoteRequest,
  HousingPropertySaleOrderCancelDraft,
  HousingPropertySaleOrderCreateDraft,
  HousingPropertySaleOrderCreateRequest,
  HousingPropertySaleOrderRepriceDraft,
  HousingPurchaseDraft,
  HousingPurchaseRequest,
} from '../../api/contracts.js';
import { HousingCommandError } from '../../api/housing-api.js';
import type {
  HousingCommandCursorSource,
  HousingLeaseArrearPaymentCommand,
  HousingLeaseArrearPaymentRetryPolicy,
  HousingLeaseDepositLoanQuoteRetryPolicy,
  HousingLeaseRetryPolicy,
  HousingLeaseRetryPolicyDeps,
  HousingMortgageQuoteRetryPolicy,
  HousingPropertySaleOrderCancelCommand,
  HousingPropertySaleOrderCancelRetryPolicy,
  HousingPropertySaleOrderCreateRetryPolicy,
  HousingPropertySaleOrderRepriceCommand,
  HousingPropertySaleOrderRepriceRetryPolicy,
  HousingPurchaseRetryPolicy,
} from './types.js';

/** Keeps the original property-sale listing body until the endpoint outcome is known. */
export function createHousingPropertySaleOrderCreateRetryPolicy(
  deps: HousingLeaseRetryPolicyDeps,
): HousingPropertySaleOrderCreateRetryPolicy {
  const pendingByRun = new Map<number, HousingPropertySaleOrderCreateRequest>();
  return {
    select(snapshot, draft) {
      return (
        pendingByRun.get(snapshot.runRevision) ?? propertySaleCreateRequestOf(snapshot, draft, deps)
      );
    },
    pending(runRevision) {
      return pendingByRun.get(runRevision);
    },
    complete(request) {
      clear(pendingByRun, request);
    },
    fail(request, error) {
      if (error instanceof HousingCommandError) clear(pendingByRun, request);
      else pendingByRun.set(request.expectedRunRevision, request);
    },
  };
}

/** Keeps the original property-sale reprice path and body until the outcome is known. */
export function createHousingPropertySaleOrderRepriceRetryPolicy(
  deps: HousingLeaseRetryPolicyDeps,
): HousingPropertySaleOrderRepriceRetryPolicy {
  const pendingByRun = new Map<number, HousingPropertySaleOrderRepriceCommand>();
  return {
    select(snapshot, draft) {
      return (
        pendingByRun.get(snapshot.runRevision) ??
        propertySaleRepriceCommandOf(snapshot, draft, deps)
      );
    },
    pending(runRevision) {
      return pendingByRun.get(runRevision);
    },
    complete(command) {
      clearPropertySaleCommand(pendingByRun, command);
    },
    fail(command, error) {
      if (error instanceof HousingCommandError) clearPropertySaleCommand(pendingByRun, command);
      else pendingByRun.set(command.request.expectedRunRevision, command);
    },
  };
}

/** Keeps the original property-sale cancellation path and body until the outcome is known. */
export function createHousingPropertySaleOrderCancelRetryPolicy(
  deps: HousingLeaseRetryPolicyDeps,
): HousingPropertySaleOrderCancelRetryPolicy {
  const pendingByRun = new Map<number, HousingPropertySaleOrderCancelCommand>();
  return {
    select(snapshot, draft) {
      return (
        pendingByRun.get(snapshot.runRevision) ?? propertySaleCancelCommandOf(snapshot, draft, deps)
      );
    },
    pending(runRevision) {
      return pendingByRun.get(runRevision);
    },
    complete(command) {
      clearPropertySaleCommand(pendingByRun, command);
    },
    fail(command, error) {
      if (error instanceof HousingCommandError) clearPropertySaleCommand(pendingByRun, command);
      else pendingByRun.set(command.request.expectedRunRevision, command);
    },
  };
}

/** Keeps the original mortgage-quote body until the dedicated endpoint outcome is known. */
export function createHousingMortgageQuoteRetryPolicy(
  deps: HousingLeaseRetryPolicyDeps,
): HousingMortgageQuoteRetryPolicy {
  const pendingByRun = new Map<number, HousingMortgageQuoteRequest>();
  return {
    select(snapshot, draft) {
      return (
        pendingByRun.get(snapshot.runRevision) ?? mortgageQuoteRequestOf(snapshot, draft, deps)
      );
    },
    pending(runRevision) {
      return pendingByRun.get(runRevision);
    },
    complete(request) {
      clear(pendingByRun, request);
    },
    fail(request, error) {
      if (error instanceof HousingCommandError) clear(pendingByRun, request);
      else pendingByRun.set(request.expectedRunRevision, request);
    },
  };
}

/** Keeps the original cash-or-mortgage purchase body until the endpoint outcome is known. */
export function createHousingPurchaseRetryPolicy(
  deps: HousingLeaseRetryPolicyDeps,
): HousingPurchaseRetryPolicy {
  const pendingByRun = new Map<number, HousingPurchaseRequest>();
  return {
    select(snapshot, draft) {
      return pendingByRun.get(snapshot.runRevision) ?? purchaseRequestOf(snapshot, draft, deps);
    },
    pending(runRevision) {
      return pendingByRun.get(runRevision);
    },
    complete(request) {
      clear(pendingByRun, request);
    },
    fail(request, error) {
      if (error instanceof HousingCommandError) clear(pendingByRun, request);
      else pendingByRun.set(request.expectedRunRevision, request);
    },
  };
}

/** Keeps the original housing loan-quote body until the server outcome is known. */
export function createHousingLeaseDepositLoanQuoteRetryPolicy(
  deps: HousingLeaseRetryPolicyDeps,
): HousingLeaseDepositLoanQuoteRetryPolicy {
  const pendingByRun = new Map<number, HousingLeaseDepositLoanQuoteRequest>();
  return {
    select(snapshot, draft) {
      return pendingByRun.get(snapshot.runRevision) ?? quoteRequestOf(snapshot, draft, deps);
    },
    pending(runRevision) {
      return pendingByRun.get(runRevision);
    },
    complete(request) {
      clear(pendingByRun, request);
    },
    fail(request, error) {
      if (error instanceof HousingCommandError) clear(pendingByRun, request);
      else pendingByRun.set(request.expectedRunRevision, request);
    },
  };
}

/** Keeps the original lease body until the server outcome is known. */
export function createHousingLeaseRetryPolicy(
  deps: HousingLeaseRetryPolicyDeps,
): HousingLeaseRetryPolicy {
  const pendingByRun = new Map<number, HousingLeaseRequest>();
  return {
    select(snapshot, draft) {
      return pendingByRun.get(snapshot.runRevision) ?? requestOf(snapshot, draft, deps);
    },
    pending(runRevision) {
      return pendingByRun.get(runRevision);
    },
    complete(request) {
      clear(pendingByRun, request);
    },
    fail(request, error) {
      if (error instanceof HousingCommandError) clear(pendingByRun, request);
      else pendingByRun.set(request.expectedRunRevision, request);
    },
  };
}

/** Keeps both the original arrear path and body until the server outcome is known. */
export function createHousingLeaseArrearPaymentRetryPolicy(
  deps: HousingLeaseRetryPolicyDeps,
): HousingLeaseArrearPaymentRetryPolicy {
  const pendingByRun = new Map<number, HousingLeaseArrearPaymentCommand>();
  return {
    select(snapshot, draft) {
      return pendingByRun.get(snapshot.runRevision) ?? paymentCommandOf(snapshot, draft, deps);
    },
    pending(runRevision) {
      return pendingByRun.get(runRevision);
    },
    complete(command) {
      clearPayment(pendingByRun, command);
    },
    fail(command, error) {
      if (error instanceof HousingCommandError) clearPayment(pendingByRun, command);
      else pendingByRun.set(command.request.expectedRunRevision, command);
    },
  };
}

function requestOf(
  snapshot: HousingCommandCursorSource,
  draft: HousingLeaseDraft,
  deps: HousingLeaseRetryPolicyDeps,
): HousingLeaseRequest {
  return {
    ...cursorOf(snapshot, deps),
    ...draft,
  };
}

function quoteRequestOf(
  snapshot: HousingCommandCursorSource,
  draft: HousingLeaseDepositLoanQuoteDraft,
  deps: HousingLeaseRetryPolicyDeps,
): HousingLeaseDepositLoanQuoteRequest {
  return {
    ...cursorOf(snapshot, deps),
    ...draft,
  };
}

function mortgageQuoteRequestOf(
  snapshot: HousingCommandCursorSource,
  draft: HousingMortgageQuoteDraft,
  deps: HousingLeaseRetryPolicyDeps,
): HousingMortgageQuoteRequest {
  return {
    ...cursorOf(snapshot, deps),
    ...draft,
  };
}

function purchaseRequestOf(
  snapshot: HousingCommandCursorSource,
  draft: HousingPurchaseDraft,
  deps: HousingLeaseRetryPolicyDeps,
): HousingPurchaseRequest {
  return {
    ...cursorOf(snapshot, deps),
    ...draft,
  };
}

function propertySaleCreateRequestOf(
  snapshot: HousingCommandCursorSource,
  draft: HousingPropertySaleOrderCreateDraft,
  deps: HousingLeaseRetryPolicyDeps,
): HousingPropertySaleOrderCreateRequest {
  return { ...cursorOf(snapshot, deps), ...draft };
}

function propertySaleRepriceCommandOf(
  snapshot: HousingCommandCursorSource,
  draft: HousingPropertySaleOrderRepriceDraft,
  deps: HousingLeaseRetryPolicyDeps,
): HousingPropertySaleOrderRepriceCommand {
  return {
    orderId: draft.orderId,
    request: {
      ...cursorOf(snapshot, deps),
      askingPriceKrw: draft.askingPriceKrw,
    },
  };
}

function propertySaleCancelCommandOf(
  snapshot: HousingCommandCursorSource,
  draft: HousingPropertySaleOrderCancelDraft,
  deps: HousingLeaseRetryPolicyDeps,
): HousingPropertySaleOrderCancelCommand {
  return {
    orderId: draft.orderId,
    request: cursorOf(snapshot, deps),
  };
}

function paymentCommandOf(
  snapshot: HousingCommandCursorSource,
  draft: HousingLeaseArrearPaymentDraft,
  deps: HousingLeaseRetryPolicyDeps,
): HousingLeaseArrearPaymentCommand {
  return {
    arrearId: draft.arrearId,
    request: {
      ...cursorOf(snapshot, deps),
      amountKrw: draft.amountKrw,
    },
  };
}

function cursorOf(
  snapshot: HousingCommandCursorSource,
  deps: HousingLeaseRetryPolicyDeps,
): Pick<
  HousingLeaseArrearPaymentRequest,
  'commandId' | 'expectedRunRevision' | 'expectedStateRevision' | 'expectedGameDay'
> {
  return {
    commandId: deps.createCommandId(),
    expectedRunRevision: snapshot.runRevision,
    expectedStateRevision: snapshot.stateRevision,
    expectedGameDay: snapshot.gameDay,
  };
}

function clear<T extends { readonly commandId: string; readonly expectedRunRevision: number }>(
  pendingByRun: Map<number, T>,
  request: T,
): void {
  if (pendingByRun.get(request.expectedRunRevision)?.commandId === request.commandId) {
    pendingByRun.delete(request.expectedRunRevision);
  }
}

function clearPayment(
  pendingByRun: Map<number, HousingLeaseArrearPaymentCommand>,
  command: HousingLeaseArrearPaymentCommand,
): void {
  if (
    pendingByRun.get(command.request.expectedRunRevision)?.request.commandId ===
    command.request.commandId
  ) {
    pendingByRun.delete(command.request.expectedRunRevision);
  }
}

function clearPropertySaleCommand<
  T extends {
    readonly request: { readonly commandId: string; readonly expectedRunRevision: number };
  },
>(pendingByRun: Map<number, T>, command: T): void {
  if (
    pendingByRun.get(command.request.expectedRunRevision)?.request.commandId ===
    command.request.commandId
  ) {
    pendingByRun.delete(command.request.expectedRunRevision);
  }
}
