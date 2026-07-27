import type {
  LoanExecutionDraft,
  LoanExecutionRequest,
  LoanPrepaymentDraft,
  LoanQuoteDraft,
  LoanQuoteRequest,
} from '../../api/contracts.js';
import { LoanCommandError } from '../../api/loan-api.js';
import type {
  LoanCommandCursorSource,
  LoanExecutionRetryPolicy,
  LoanPrepaymentCommand,
  LoanPrepaymentRetryPolicy,
  LoanQuoteRetryPolicy,
  LoanRetryPolicyDeps,
} from './types.js';

/** Keeps a quote command only while its server outcome is unknown. */
export function createLoanQuoteRetryPolicy(deps: LoanRetryPolicyDeps): LoanQuoteRetryPolicy {
  const pending = new Map<string, LoanQuoteRequest>();
  return {
    select(snapshot, draft) {
      const key = quoteKey(snapshot.runRevision, draft);
      return pending.get(key) ?? { ...cursorOf(snapshot, deps), ...draft };
    },
    complete(request) {
      clear(pending, quoteRequestKey(request), request.commandId);
    },
    fail(request, error) {
      const key = quoteRequestKey(request);
      if (error instanceof LoanCommandError) clear(pending, key, request.commandId);
      else pending.set(key, request);
    },
  };
}

/** Keeps a loan execution command only while its server outcome is unknown. */
export function createLoanExecutionRetryPolicy(
  deps: LoanRetryPolicyDeps,
): LoanExecutionRetryPolicy {
  const pending = new Map<string, LoanExecutionRequest>();
  return {
    select(snapshot, draft) {
      const key = executionKey(snapshot.runRevision, draft);
      return pending.get(key) ?? { ...cursorOf(snapshot, deps), ...draft };
    },
    pending(runRevision, draft) {
      return pending.get(executionKey(runRevision, draft));
    },
    complete(request) {
      clear(pending, executionRequestKey(request), request.commandId);
    },
    fail(request, error) {
      const key = executionRequestKey(request);
      if (error instanceof LoanCommandError) clear(pending, key, request.commandId);
      else pending.set(key, request);
    },
  };
}

/** Keeps the path and body of a prepayment while its server outcome is unknown. */
export function createLoanPrepaymentRetryPolicy(
  deps: LoanRetryPolicyDeps,
): LoanPrepaymentRetryPolicy {
  const pending = new Map<string, LoanPrepaymentCommand>();
  return {
    select(snapshot, draft) {
      const key = prepaymentKey(snapshot.runRevision, draft);
      const existing = pending.get(key);
      if (existing !== undefined) return existing;
      const command = {
        loanId: draft.loanId,
        request: { ...cursorOf(snapshot, deps), principalKrw: draft.principalKrw },
      };
      pending.set(key, command);
      return command;
    },
    pending(runRevision, draft) {
      return pending.get(prepaymentKey(runRevision, draft));
    },
    pendingForRun(runRevision) {
      for (const command of pending.values()) {
        if (command.request.expectedRunRevision === runRevision) return command;
      }
      return undefined;
    },
    complete(command) {
      clearPrepaymentCommand(pending, prepaymentCommandKey(command), command.request.commandId);
    },
    fail(command, error) {
      const key = prepaymentCommandKey(command);
      if (error instanceof LoanCommandError) {
        clearPrepaymentCommand(pending, key, command.request.commandId);
      } else {
        pending.set(key, command);
      }
    },
  };
}

type LoanCommandFields = Pick<
  LoanQuoteRequest,
  'commandId' | 'expectedRunRevision' | 'expectedStateRevision' | 'expectedGameDay'
>;

function cursorOf(snapshot: LoanCommandCursorSource, deps: LoanRetryPolicyDeps): LoanCommandFields {
  return {
    commandId: deps.createCommandId(),
    expectedRunRevision: snapshot.runRevision,
    expectedStateRevision: snapshot.stateRevision,
    expectedGameDay: snapshot.gameDay,
  };
}

function quoteKey(runRevision: number, draft: LoanQuoteDraft): string {
  return JSON.stringify([runRevision, draft.productVersionId, draft.principalKrw]);
}

function quoteRequestKey(request: LoanQuoteRequest): string {
  return quoteKey(request.expectedRunRevision, request);
}

function executionKey(runRevision: number, draft: LoanExecutionDraft): string {
  return JSON.stringify([runRevision, draft.quoteId]);
}

function executionRequestKey(request: LoanExecutionRequest): string {
  return executionKey(request.expectedRunRevision, request);
}

function prepaymentKey(runRevision: number, draft: LoanPrepaymentDraft): string {
  return JSON.stringify([runRevision, draft.loanId, draft.principalKrw]);
}

function prepaymentCommandKey(command: LoanPrepaymentCommand): string {
  return prepaymentKey(command.request.expectedRunRevision, {
    loanId: command.loanId,
    principalKrw: command.request.principalKrw,
  });
}

function clear<T extends { readonly commandId: string }>(
  pending: Map<string, T>,
  key: string,
  commandId: string,
): void {
  if (pending.get(key)?.commandId === commandId) pending.delete(key);
}

function clearPrepaymentCommand(
  pending: Map<string, LoanPrepaymentCommand>,
  key: string,
  commandId: string,
): void {
  if (pending.get(key)?.request.commandId === commandId) pending.delete(key);
}
