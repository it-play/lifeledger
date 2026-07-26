import type {
  FinanceTransferDraft,
  FinanceTransferRequest,
  GameSnapshot,
} from '../../api/contracts.js';
import type { FinanceTransferRetryPolicy, FinanceTransferRetryPolicyDeps } from './types.js';

export function createFinanceTransferRetryPolicy(
  deps: FinanceTransferRetryPolicyDeps,
): FinanceTransferRetryPolicy {
  const pending = new Map<string, FinanceTransferRequest>();

  return {
    select(snapshot, draft) {
      return (
        pending.get(draftKey(snapshot.runRevision, draft)) ??
        requestOf(snapshot, draft, deps.createCommandId())
      );
    },
    retain(request) {
      pending.set(requestKey(request), request);
    },
    clear(request) {
      const key = requestKey(request);
      if (pending.get(key)?.commandId === request.commandId) pending.delete(key);
    },
  };
}

function requestOf(
  snapshot: GameSnapshot,
  draft: FinanceTransferDraft,
  commandId: string,
): FinanceTransferRequest {
  return {
    commandId,
    expectedRunRevision: snapshot.runRevision,
    expectedStateRevision: snapshot.stateRevision,
    expectedGameDay: snapshot.gameDay,
    accountId: draft.accountId,
    direction: draft.direction,
    amountKrw: draft.amountKrw,
  };
}

function requestKey(request: FinanceTransferRequest): string {
  return intentKey(
    request.expectedRunRevision,
    request.accountId,
    request.direction,
    request.amountKrw,
  );
}

function draftKey(runRevision: number, draft: FinanceTransferDraft): string {
  return intentKey(runRevision, draft.accountId, draft.direction, draft.amountKrw);
}

function intentKey(
  runRevision: number,
  accountId: string,
  direction: FinanceTransferDraft['direction'],
  amountKrw: number,
): string {
  return `${runRevision}:${accountId}:${direction}:${amountKrw}`;
}
