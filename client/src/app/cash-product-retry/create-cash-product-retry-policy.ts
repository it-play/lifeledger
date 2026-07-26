import type {
  CmaAccountOpenDraft,
  CmaAccountOpenRequest,
  DepositOpenDraft,
  DepositOpenRequest,
  FinanceCommandRequest,
  GameSnapshot,
} from '../../api/contracts.js';
import { FinanceCommandError } from '../../api/game-api.js';
import type {
  CashProductRetryPolicyDeps,
  CmaAccountCloseCommand,
  CmaAccountCloseRetryPolicy,
  CmaAccountOpenRetryPolicy,
  DepositCloseCommand,
  DepositCloseRetryPolicy,
  DepositOpenRetryPolicy,
} from './types.js';

export function createCmaAccountOpenRetryPolicy(
  deps: CashProductRetryPolicyDeps,
): CmaAccountOpenRetryPolicy {
  const pending = new Map<string, CmaAccountOpenRequest>();

  return {
    select(snapshot, draft) {
      const key = cmaOpenKey(snapshot.runRevision, draft);
      return pending.get(key) ?? cmaOpenRequestOf(snapshot, draft, deps.createCommandId());
    },
    complete(request) {
      clearRequest(pending, cmaOpenRequestKey(request), request.commandId);
    },
    fail(request, error) {
      const key = cmaOpenRequestKey(request);
      if (error instanceof FinanceCommandError) clearRequest(pending, key, request.commandId);
      else pending.set(key, request);
    },
  };
}

export function createCmaAccountCloseRetryPolicy(
  deps: CashProductRetryPolicyDeps,
): CmaAccountCloseRetryPolicy {
  const pending = new Map<string, CmaAccountCloseCommand>();

  return {
    select(snapshot, draft) {
      const key = cmaCloseKey(snapshot.runRevision, draft.accountId);
      return (
        pending.get(key) ?? {
          accountId: draft.accountId,
          request: commandRequestOf(snapshot, deps.createCommandId()),
        }
      );
    },
    complete(command) {
      clearCommand(pending, cmaCloseCommandKey(command), command.request.commandId);
    },
    fail(command, error) {
      const key = cmaCloseCommandKey(command);
      if (error instanceof FinanceCommandError)
        clearCommand(pending, key, command.request.commandId);
      else pending.set(key, command);
    },
  };
}

export function createDepositOpenRetryPolicy(
  deps: CashProductRetryPolicyDeps,
): DepositOpenRetryPolicy {
  const pending = new Map<string, DepositOpenRequest>();

  return {
    select(snapshot, draft) {
      const key = depositOpenKey(snapshot.runRevision, draft);
      return pending.get(key) ?? depositOpenRequestOf(snapshot, draft, deps.createCommandId());
    },
    complete(request) {
      clearRequest(pending, depositOpenRequestKey(request), request.commandId);
    },
    fail(request, error) {
      const key = depositOpenRequestKey(request);
      if (error instanceof FinanceCommandError) clearRequest(pending, key, request.commandId);
      else pending.set(key, request);
    },
  };
}

export function createDepositCloseRetryPolicy(
  deps: CashProductRetryPolicyDeps,
): DepositCloseRetryPolicy {
  const pending = new Map<string, DepositCloseCommand>();

  return {
    select(snapshot, draft) {
      const key = depositCloseKey(snapshot.runRevision, draft.contractId);
      return (
        pending.get(key) ?? {
          contractId: draft.contractId,
          request: commandRequestOf(snapshot, deps.createCommandId()),
        }
      );
    },
    complete(command) {
      clearCommand(pending, depositCloseCommandKey(command), command.request.commandId);
    },
    fail(command, error) {
      const key = depositCloseCommandKey(command);
      if (error instanceof FinanceCommandError) {
        clearCommand(pending, key, command.request.commandId);
      } else {
        pending.set(key, command);
      }
    },
  };
}

function commandRequestOf(snapshot: GameSnapshot, commandId: string): FinanceCommandRequest {
  return {
    commandId,
    expectedRunRevision: snapshot.runRevision,
    expectedStateRevision: snapshot.stateRevision,
    expectedGameDay: snapshot.gameDay,
  };
}

function cmaOpenRequestOf(
  snapshot: GameSnapshot,
  draft: CmaAccountOpenDraft,
  commandId: string,
): CmaAccountOpenRequest {
  return { ...commandRequestOf(snapshot, commandId), ...draft };
}

function depositOpenRequestOf(
  snapshot: GameSnapshot,
  draft: DepositOpenDraft,
  commandId: string,
): DepositOpenRequest {
  return { ...commandRequestOf(snapshot, commandId), ...draft };
}

function cmaOpenKey(runRevision: number, draft: CmaAccountOpenDraft): string {
  return JSON.stringify([runRevision, draft.type, draft.productVersionId]);
}

function cmaOpenRequestKey(request: CmaAccountOpenRequest): string {
  return cmaOpenKey(request.expectedRunRevision, request);
}

function cmaCloseKey(runRevision: number, accountId: string): string {
  return JSON.stringify([runRevision, accountId]);
}

function cmaCloseCommandKey(command: CmaAccountCloseCommand): string {
  return cmaCloseKey(command.request.expectedRunRevision, command.accountId);
}

function depositOpenKey(runRevision: number, draft: DepositOpenDraft): string {
  return JSON.stringify([
    runRevision,
    draft.kind,
    draft.productVersionId,
    draft.settlementAccountId,
    draft.amountKrw,
  ]);
}

function depositOpenRequestKey(request: DepositOpenRequest): string {
  return depositOpenKey(request.expectedRunRevision, request);
}

function depositCloseKey(runRevision: number, contractId: string): string {
  return JSON.stringify([runRevision, contractId]);
}

function depositCloseCommandKey(command: DepositCloseCommand): string {
  return depositCloseKey(command.request.expectedRunRevision, command.contractId);
}

function clearRequest<T extends { readonly commandId: string }>(
  pending: Map<string, T>,
  key: string,
  commandId: string,
): void {
  if (pending.get(key)?.commandId === commandId) pending.delete(key);
}

function clearCommand<T extends { readonly request: { readonly commandId: string } }>(
  pending: Map<string, T>,
  key: string,
  commandId: string,
): void {
  if (pending.get(key)?.request.commandId === commandId) pending.delete(key);
}
