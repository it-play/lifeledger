import type { FinanceCommandRequest, GameCommandCursor } from '../../api/contracts.js';

/** Preserves an indeterminate M2-D asset command at its original cursor. */
export interface FinanceAssetRetryPolicy<Draft, Request extends FinanceCommandRequest> {
  select(snapshot: GameCommandCursor, draft: Draft): Request;
  complete(request: Request): void;
  fail(request: Request, error: unknown): void;
}

export interface FinanceAssetRetryPolicyDeps<Draft, Request extends FinanceCommandRequest> {
  readonly createCommandId: () => string;
  readonly draftKey: (runRevision: number, draft: Draft) => string;
  readonly requestKey: (request: Request) => string;
  readonly requestOf: (snapshot: GameCommandCursor, draft: Draft, commandId: string) => Request;
}
