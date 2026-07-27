import type { WelfareApplicationRequest } from '../../api/contracts.js';
import { WelfareCommandError } from '../../api/welfare-api.js';
import type {
  WelfareApplicationRetryPolicy,
  WelfareCommandCursorSource,
  WelfareRetryPolicyDeps,
} from './types.js';

/** Keeps the original welfare application body until the endpoint outcome is known. */
export function createWelfareApplicationRetryPolicy(
  deps: WelfareRetryPolicyDeps,
): WelfareApplicationRetryPolicy {
  const pending = new Map<string, WelfareApplicationRequest>();
  return {
    select(snapshot, programVersionId) {
      const key = applicationKey(snapshot.runRevision, programVersionId);
      return pending.get(key) ?? requestOf(snapshot, programVersionId, deps.createCommandId());
    },
    pending(runRevision, programVersionId) {
      return pending.get(applicationKey(runRevision, programVersionId));
    },
    complete(request) {
      clear(pending, request);
    },
    fail(request, error) {
      if (error instanceof WelfareCommandError) clear(pending, request);
      else pending.set(applicationRequestKey(request), request);
    },
  };
}

function requestOf(
  snapshot: WelfareCommandCursorSource,
  programVersionId: string,
  commandId: string,
): WelfareApplicationRequest {
  return {
    commandId,
    expectedRunRevision: snapshot.runRevision,
    expectedStateRevision: snapshot.stateRevision,
    expectedGameDay: snapshot.gameDay,
    programVersionId,
  };
}

function applicationKey(runRevision: number, programVersionId: string): string {
  return `${runRevision}:${programVersionId}`;
}

function applicationRequestKey(request: WelfareApplicationRequest): string {
  return applicationKey(request.expectedRunRevision, request.programVersionId);
}

function clear(
  pending: Map<string, WelfareApplicationRequest>,
  request: WelfareApplicationRequest,
): void {
  const key = applicationRequestKey(request);
  if (pending.get(key)?.commandId === request.commandId) pending.delete(key);
}
