import type {
  FinanceCommandRequest,
  GameSnapshot,
  PensionStartDraft,
  PensionStartRequest,
  PensionWithdrawalDraft,
  PensionWithdrawalRequest,
  TaxAccountOpenDraft,
  TaxAccountOpenRequest,
} from '../../api/contracts.js';
import { FinanceCommandError } from '../../api/game-api.js';
import type {
  IsaAccountCloseCommand,
  IsaAccountCloseRetryPolicy,
  PensionStartCommand,
  PensionStartRetryPolicy,
  PensionWithdrawalCommand,
  PensionWithdrawalRetryPolicy,
  TaxAccountOpenRetryPolicy,
  TaxAccountRetryPolicyDeps,
} from './types.js';

export function createTaxAccountOpenRetryPolicy(
  deps: TaxAccountRetryPolicyDeps,
): TaxAccountOpenRetryPolicy {
  const pending = new Map<string, TaxAccountOpenRequest>();

  return {
    select(snapshot, draft) {
      const key = openKey(snapshot.runRevision, draft);
      return pending.get(key) ?? openRequestOf(snapshot, draft, deps.createCommandId());
    },
    complete(request) {
      clearRequest(pending, openRequestKey(request), request.commandId);
    },
    fail(request, error) {
      const key = openRequestKey(request);
      if (error instanceof FinanceCommandError) clearRequest(pending, key, request.commandId);
      else pending.set(key, request);
    },
  };
}

export function createIsaAccountCloseRetryPolicy(
  deps: TaxAccountRetryPolicyDeps,
): IsaAccountCloseRetryPolicy {
  const pending = new Map<string, IsaAccountCloseCommand>();

  return {
    select(snapshot, draft) {
      const key = accountKey(snapshot.runRevision, draft.accountId);
      return (
        pending.get(key) ?? {
          accountId: draft.accountId,
          request: commandRequestOf(snapshot, deps.createCommandId()),
        }
      );
    },
    complete(command) {
      clearCommand(pending, closeCommandKey(command), command.request.commandId);
    },
    fail(command, error) {
      failCommand(pending, closeCommandKey(command), command, error);
    },
  };
}

export function createPensionStartRetryPolicy(
  deps: TaxAccountRetryPolicyDeps,
): PensionStartRetryPolicy {
  const pending = new Map<string, PensionStartCommand>();

  return {
    select(snapshot, draft) {
      const key = pensionStartKey(snapshot.runRevision, draft);
      return (
        pending.get(key) ?? {
          accountId: draft.accountId,
          request: pensionStartRequestOf(snapshot, draft, deps.createCommandId()),
        }
      );
    },
    complete(command) {
      clearCommand(pending, pensionStartCommandKey(command), command.request.commandId);
    },
    fail(command, error) {
      failCommand(pending, pensionStartCommandKey(command), command, error);
    },
  };
}

export function createPensionWithdrawalRetryPolicy(
  deps: TaxAccountRetryPolicyDeps,
): PensionWithdrawalRetryPolicy {
  const pending = new Map<string, PensionWithdrawalCommand>();

  return {
    select(snapshot, draft) {
      const key = pensionWithdrawalKey(snapshot.runRevision, draft);
      return (
        pending.get(key) ?? {
          accountId: draft.accountId,
          request: pensionWithdrawalRequestOf(snapshot, draft, deps.createCommandId()),
        }
      );
    },
    complete(command) {
      clearCommand(pending, pensionWithdrawalCommandKey(command), command.request.commandId);
    },
    fail(command, error) {
      failCommand(pending, pensionWithdrawalCommandKey(command), command, error);
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

function openRequestOf(
  snapshot: GameSnapshot,
  draft: TaxAccountOpenDraft,
  commandId: string,
): TaxAccountOpenRequest {
  return { ...commandRequestOf(snapshot, commandId), type: draft.type };
}

function pensionStartRequestOf(
  snapshot: GameSnapshot,
  draft: PensionStartDraft,
  commandId: string,
): PensionStartRequest {
  return {
    ...commandRequestOf(snapshot, commandId),
    paymentYears: draft.paymentYears,
    lifetime: draft.lifetime,
  };
}

function pensionWithdrawalRequestOf(
  snapshot: GameSnapshot,
  draft: PensionWithdrawalDraft,
  commandId: string,
): PensionWithdrawalRequest {
  return {
    ...commandRequestOf(snapshot, commandId),
    amountKrw: draft.amountKrw,
    type: draft.type,
    reason: draft.reason,
  };
}

function openKey(runRevision: number, draft: TaxAccountOpenDraft): string {
  return JSON.stringify([runRevision, draft.type]);
}

function openRequestKey(request: TaxAccountOpenRequest): string {
  return openKey(request.expectedRunRevision, request);
}

function accountKey(runRevision: number, accountId: string): string {
  return JSON.stringify([runRevision, accountId]);
}

function closeCommandKey(command: IsaAccountCloseCommand): string {
  return accountKey(command.request.expectedRunRevision, command.accountId);
}

function pensionStartKey(runRevision: number, draft: PensionStartDraft): string {
  return JSON.stringify([runRevision, draft.accountId, draft.paymentYears, draft.lifetime]);
}

function pensionStartCommandKey(command: PensionStartCommand): string {
  return JSON.stringify([
    command.request.expectedRunRevision,
    command.accountId,
    command.request.paymentYears,
    command.request.lifetime,
  ]);
}

function pensionWithdrawalKey(runRevision: number, draft: PensionWithdrawalDraft): string {
  return JSON.stringify([runRevision, draft.accountId, draft.amountKrw, draft.type, draft.reason]);
}

function pensionWithdrawalCommandKey(command: PensionWithdrawalCommand): string {
  return JSON.stringify([
    command.request.expectedRunRevision,
    command.accountId,
    command.request.amountKrw,
    command.request.type,
    command.request.reason,
  ]);
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

function failCommand<T extends { readonly request: { readonly commandId: string } }>(
  pending: Map<string, T>,
  key: string,
  command: T,
  error: unknown,
): void {
  if (error instanceof FinanceCommandError) clearCommand(pending, key, command.request.commandId);
  else pending.set(key, command);
}
