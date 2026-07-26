import type { FinanceCommandRequest } from '../../api/contracts.js';
import { FinanceCommandError } from '../../api/game-api.js';
import type { FinanceAssetRetryPolicy, FinanceAssetRetryPolicyDeps } from './types.js';

export function createFinanceAssetRetryPolicy<Draft, Request extends FinanceCommandRequest>(
  deps: FinanceAssetRetryPolicyDeps<Draft, Request>,
): FinanceAssetRetryPolicy<Draft, Request> {
  const pending = new Map<string, Request>();

  return {
    select(snapshot, draft) {
      const key = deps.draftKey(snapshot.runRevision, draft);
      return pending.get(key) ?? deps.requestOf(snapshot, draft, deps.createCommandId());
    },
    complete(request) {
      clearMatchingRequest(pending, deps.requestKey(request), request.commandId);
    },
    fail(request, error) {
      const key = deps.requestKey(request);
      if (error instanceof FinanceCommandError) {
        clearMatchingRequest(pending, key, request.commandId);
        return;
      }
      pending.set(key, request);
    },
  };
}

function clearMatchingRequest<Request extends FinanceCommandRequest>(
  pending: Map<string, Request>,
  key: string,
  commandId: string,
): void {
  if (pending.get(key)?.commandId === commandId) pending.delete(key);
}
