import type { InsolvencyAction } from '../../api/contracts.js';
import { InsolvencyCommandError } from '../../api/insolvency-api.js';
import type {
  InsolvencyCommandCursorSource,
  InsolvencyRetryPolicy,
  InsolvencyRetryPolicyDeps,
  PendingInsolvencyActionCommand,
  PendingInsolvencyCommand,
  PendingInsolvencyPrepareCommand,
} from './types.js';

/** Keeps an insolvency command's original path and body until its outcome is known. */
export function createInsolvencyRetryPolicy(
  deps: InsolvencyRetryPolicyDeps,
): InsolvencyRetryPolicy {
  const pending = new Map<string, PendingInsolvencyCommand>();
  return {
    prepare(snapshot) {
      const key = prepareKey(snapshot.runRevision);
      const existing = pending.get(key);
      return existing?.kind === 'prepare'
        ? existing
        : prepareCommand(snapshot, deps.createCommandId());
    },
    act(snapshot, caseId, action) {
      const key = actionKey(snapshot.runRevision, caseId, action);
      const existing = pending.get(key);
      return existing?.kind === 'action'
        ? existing
        : actionCommand(snapshot, caseId, action, deps.createCommandId());
    },
    complete(command) {
      clear(pending, command);
    },
    fail(command, error) {
      if (error instanceof InsolvencyCommandError) clear(pending, command);
      else pending.set(commandKey(command), command);
    },
  };
}

function prepareCommand(
  snapshot: InsolvencyCommandCursorSource,
  commandId: string,
): PendingInsolvencyPrepareCommand {
  return {
    kind: 'prepare',
    request: {
      ...cursorOf(snapshot, commandId),
      procedureKind: 'cashOnlyLiquidation',
    },
  };
}

function actionCommand(
  snapshot: InsolvencyCommandCursorSource,
  caseId: string,
  action: InsolvencyAction,
  commandId: string,
): PendingInsolvencyActionCommand {
  return {
    kind: 'action',
    caseId,
    request: { ...cursorOf(snapshot, commandId), action },
  };
}

function cursorOf(snapshot: InsolvencyCommandCursorSource, commandId: string) {
  return {
    commandId,
    expectedRunRevision: snapshot.runRevision,
    expectedStateRevision: snapshot.stateRevision,
    expectedGameDay: snapshot.gameDay,
  };
}

function prepareKey(runRevision: number): string {
  return `prepare:${runRevision}`;
}

function actionKey(runRevision: number, caseId: string, action: InsolvencyAction): string {
  return `action:${runRevision}:${caseId}:${action}`;
}

function commandKey(command: PendingInsolvencyCommand): string {
  return command.kind === 'prepare'
    ? prepareKey(command.request.expectedRunRevision)
    : actionKey(command.request.expectedRunRevision, command.caseId, command.request.action);
}

function clear(
  pending: Map<string, PendingInsolvencyCommand>,
  command: PendingInsolvencyCommand,
): void {
  const key = commandKey(command);
  if (pending.get(key)?.request.commandId === command.request.commandId) pending.delete(key);
}
