import type {
  CmaAccountCloseDraft,
  CmaAccountCloseRequest,
  CmaAccountOpenDraft,
  CmaAccountOpenRequest,
  DepositCloseDraft,
  DepositCloseRequest,
  DepositOpenDraft,
  DepositOpenRequest,
  GameSnapshot,
} from '../../api/contracts.js';

export interface CashProductRetryPolicyDeps {
  readonly createCommandId: () => string;
}

export interface CmaAccountCloseCommand {
  readonly accountId: string;
  readonly request: CmaAccountCloseRequest;
}

export interface DepositCloseCommand {
  readonly contractId: string;
  readonly request: DepositCloseRequest;
}

/** Preserves an indeterminate CMA-open command at its original cursor. */
export interface CmaAccountOpenRetryPolicy {
  select(snapshot: GameSnapshot, draft: CmaAccountOpenDraft): CmaAccountOpenRequest;
  complete(request: CmaAccountOpenRequest): void;
  fail(request: CmaAccountOpenRequest, error: unknown): void;
}

/** Preserves both the path ID and body of an indeterminate CMA-close command. */
export interface CmaAccountCloseRetryPolicy {
  select(snapshot: GameSnapshot, draft: CmaAccountCloseDraft): CmaAccountCloseCommand;
  complete(command: CmaAccountCloseCommand): void;
  fail(command: CmaAccountCloseCommand, error: unknown): void;
}

/** Preserves an indeterminate deposit-open payload at its original cursor. */
export interface DepositOpenRetryPolicy {
  select(snapshot: GameSnapshot, draft: DepositOpenDraft): DepositOpenRequest;
  complete(request: DepositOpenRequest): void;
  fail(request: DepositOpenRequest, error: unknown): void;
}

/** Preserves both the path ID and body of an indeterminate deposit-close command. */
export interface DepositCloseRetryPolicy {
  select(snapshot: GameSnapshot, draft: DepositCloseDraft): DepositCloseCommand;
  complete(command: DepositCloseCommand): void;
  fail(command: DepositCloseCommand, error: unknown): void;
}
