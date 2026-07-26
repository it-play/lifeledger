import type {
  GameSnapshot,
  PortfolioOrderDraft,
  PortfolioOrderRequest,
} from '../../api/contracts.js';

/** Selects and remembers an idempotent request after an indeterminate order result. */
export interface OrderRetryPolicy {
  select(snapshot: GameSnapshot, draft: PortfolioOrderDraft): PortfolioOrderRequest;
  retain(request: PortfolioOrderRequest): void;
  clear(request: PortfolioOrderRequest): void;
}

export interface OrderRetryPolicyDeps {
  readonly createOrderId: () => string;
}
