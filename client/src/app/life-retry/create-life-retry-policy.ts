import type { LifeBudgetUpdateDraft, LifeBudgetUpdateRequest } from '../../api/contracts.js';
import { LifeCommandError } from '../../api/life-api.js';
import type {
  EssentialArrearPaymentCommand,
  EssentialArrearPaymentRetryPolicy,
  LifeBudgetRetryPolicy,
  LifeCommandCursorSource,
  LifeRetryPolicyDeps,
} from './types.js';

export function createLifeBudgetRetryPolicy(deps: LifeRetryPolicyDeps): LifeBudgetRetryPolicy {
  const pending = new Map<string, LifeBudgetUpdateRequest>();
  return {
    select(snapshot, draft) {
      const key = budgetKey(snapshot.runRevision, draft);
      return pending.get(key) ?? { ...cursorOf(snapshot, deps), selections: draft.selections };
    },
    complete(request) {
      clear(pending, budgetRequestKey(request), request.commandId);
    },
    fail(request, error) {
      retainTransportFailure(pending, budgetRequestKey(request), request, error);
    },
  };
}

export function createEssentialArrearPaymentRetryPolicy(
  deps: LifeRetryPolicyDeps,
): EssentialArrearPaymentRetryPolicy {
  const pending = new Map<string, EssentialArrearPaymentCommand>();
  return {
    select(snapshot, arrearId, draft) {
      const key = arrearPaymentKey(snapshot.runRevision, arrearId, draft.amountKrw);
      return (
        pending.get(key) ?? {
          arrearId,
          request: { ...cursorOf(snapshot, deps), amountKrw: draft.amountKrw },
        }
      );
    },
    complete(command) {
      clearArrearCommand(pending, arrearPaymentCommandKey(command), command.request.commandId);
    },
    fail(command, error) {
      const key = arrearPaymentCommandKey(command);
      if (error instanceof LifeCommandError) {
        clearArrearCommand(pending, key, command.request.commandId);
      } else {
        pending.set(key, command);
      }
    },
  };
}

function cursorOf(
  snapshot: LifeCommandCursorSource,
  deps: LifeRetryPolicyDeps,
): Pick<
  LifeBudgetUpdateRequest,
  'commandId' | 'expectedRunRevision' | 'expectedStateRevision' | 'expectedGameDay'
> {
  return {
    commandId: deps.createCommandId(),
    expectedRunRevision: snapshot.runRevision,
    expectedStateRevision: snapshot.stateRevision,
    expectedGameDay: snapshot.gameDay,
  };
}

function budgetKey(runRevision: number, draft: LifeBudgetUpdateDraft): string {
  const selections = [...draft.selections]
    .sort((left, right) => left.category.localeCompare(right.category))
    .map((selection) => [selection.category, selection.bandId]);
  return JSON.stringify([runRevision, selections]);
}

function budgetRequestKey(request: LifeBudgetUpdateRequest): string {
  return budgetKey(request.expectedRunRevision, request);
}

function arrearPaymentKey(runRevision: number, arrearId: string, amountKrw: number): string {
  return JSON.stringify([runRevision, arrearId, amountKrw]);
}

function arrearPaymentCommandKey(command: EssentialArrearPaymentCommand): string {
  return arrearPaymentKey(
    command.request.expectedRunRevision,
    command.arrearId,
    command.request.amountKrw,
  );
}

function clear<T extends { readonly commandId: string }>(
  pending: Map<string, T>,
  key: string,
  commandId: string,
): void {
  if (pending.get(key)?.commandId === commandId) pending.delete(key);
}

function clearArrearCommand(
  pending: Map<string, EssentialArrearPaymentCommand>,
  key: string,
  commandId: string,
): void {
  if (pending.get(key)?.request.commandId === commandId) pending.delete(key);
}

function retainTransportFailure(
  pending: Map<string, LifeBudgetUpdateRequest>,
  key: string,
  request: LifeBudgetUpdateRequest,
  error: unknown,
): void {
  if (error instanceof LifeCommandError) clear(pending, key, request.commandId);
  else pending.set(key, request);
}
