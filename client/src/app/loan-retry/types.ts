import type {
  GameSnapshot,
  LoanExecutionDraft,
  LoanExecutionRequest,
  LoanPrepaymentDraft,
  LoanPrepaymentRequest,
  LoanQuoteDraft,
  LoanQuoteRequest,
} from '../../api/contracts.js';

export type LoanCommandCursorSource = Pick<
  GameSnapshot,
  'runRevision' | 'stateRevision' | 'gameDay'
>;

export interface LoanRetryPolicyDeps {
  readonly createCommandId: () => string;
}

export interface LoanQuoteRetryPolicy {
  select(snapshot: LoanCommandCursorSource, draft: LoanQuoteDraft): LoanQuoteRequest;
  complete(request: LoanQuoteRequest): void;
  fail(request: LoanQuoteRequest, error: unknown): void;
}

export interface LoanExecutionRetryPolicy {
  select(snapshot: LoanCommandCursorSource, draft: LoanExecutionDraft): LoanExecutionRequest;
  pending(runRevision: number, draft: LoanExecutionDraft): LoanExecutionRequest | undefined;
  complete(request: LoanExecutionRequest): void;
  fail(request: LoanExecutionRequest, error: unknown): void;
}

export interface LoanPrepaymentCommand {
  readonly loanId: string;
  readonly request: LoanPrepaymentRequest;
}

export interface LoanPrepaymentRetryPolicy {
  select(snapshot: LoanCommandCursorSource, draft: LoanPrepaymentDraft): LoanPrepaymentCommand;
  pending(runRevision: number, draft: LoanPrepaymentDraft): LoanPrepaymentCommand | undefined;
  pendingForRun(runRevision: number): LoanPrepaymentCommand | undefined;
  complete(command: LoanPrepaymentCommand): void;
  fail(command: LoanPrepaymentCommand, error: unknown): void;
}
