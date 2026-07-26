import type {
  FinanceTransferDraft,
  FinanceTransferRequest,
  GameSnapshot,
} from '../../api/contracts.js';

/** Selects and remembers an idempotent transfer after an indeterminate response. */
export interface FinanceTransferRetryPolicy {
  select(snapshot: GameSnapshot, draft: FinanceTransferDraft): FinanceTransferRequest;
  retain(request: FinanceTransferRequest): void;
  clear(request: FinanceTransferRequest): void;
}

export interface FinanceTransferRetryPolicyDeps {
  readonly createCommandId: () => string;
}
