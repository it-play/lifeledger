import type {
  GameSnapshot,
  PortfolioOrderDraft,
  PortfolioOrderRequest,
} from '../../api/contracts.js';
import type { OrderRetryPolicy, OrderRetryPolicyDeps } from './types.js';

export function createOrderRetryPolicy(deps: OrderRetryPolicyDeps): OrderRetryPolicy {
  const pending = new Map<string, PortfolioOrderRequest>();

  return {
    select(snapshot, draft) {
      return (
        pending.get(draftKey(snapshot.runRevision, draft)) ??
        requestOf(snapshot, draft, deps.createOrderId())
      );
    },
    retain(request) {
      pending.set(requestKey(request), request);
    },
    clear(request) {
      const key = requestKey(request);
      if (pending.get(key)?.orderId === request.orderId) pending.delete(key);
    },
  };
}

function requestOf(
  snapshot: GameSnapshot,
  draft: PortfolioOrderDraft,
  orderId: string,
): PortfolioOrderRequest {
  return {
    orderId,
    expectedRunRevision: snapshot.runRevision,
    expectedStateRevision: snapshot.stateRevision,
    expectedGameDay: snapshot.gameDay,
    accountId: draft.accountId,
    side: draft.side,
    symbol: 'LLX',
    quantity: draft.quantity,
  };
}

function requestKey(request: PortfolioOrderRequest): string {
  return intentKey(request.expectedRunRevision, request.accountId, request.side, request.quantity);
}

function draftKey(runRevision: number, draft: PortfolioOrderDraft): string {
  return intentKey(runRevision, draft.accountId, draft.side, draft.quantity);
}

function intentKey(
  runRevision: number,
  accountId: string,
  side: PortfolioOrderDraft['side'],
  quantity: number,
): string {
  return `${runRevision}:${accountId}:${side}:${quantity}`;
}
