import type {
  GameSnapshot,
  InsolvencyAction,
  InsolvencyCaseActionRequest,
  InsolvencyCasePrepareRequest,
} from '../../api/contracts.js';

export type InsolvencyCommandCursorSource = Pick<
  GameSnapshot,
  'runRevision' | 'stateRevision' | 'gameDay'
>;

export interface PendingInsolvencyPrepareCommand {
  readonly kind: 'prepare';
  readonly request: InsolvencyCasePrepareRequest;
}

export interface PendingInsolvencyActionCommand {
  readonly kind: 'action';
  readonly caseId: string;
  readonly request: InsolvencyCaseActionRequest;
}

export type PendingInsolvencyCommand =
  | PendingInsolvencyPrepareCommand
  | PendingInsolvencyActionCommand;

export interface InsolvencyRetryPolicyDeps {
  readonly createCommandId: () => string;
}

export interface InsolvencyRetryPolicy {
  prepare(snapshot: InsolvencyCommandCursorSource): PendingInsolvencyPrepareCommand;
  act(
    snapshot: InsolvencyCommandCursorSource,
    caseId: string,
    action: InsolvencyAction,
  ): PendingInsolvencyActionCommand;
  complete(command: PendingInsolvencyCommand): void;
  fail(command: PendingInsolvencyCommand, error: unknown): void;
}
