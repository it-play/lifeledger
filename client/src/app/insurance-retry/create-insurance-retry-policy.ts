import { InsuranceCommandError } from '../../api/insurance-api.js';
import type {
  InsuranceCommandCursorSource,
  InsuranceRetryPolicy,
  InsuranceRetryPolicyDeps,
  PendingInsuranceCancellationCommand,
  PendingInsuranceClaimCommand,
  PendingInsuranceCommand,
  PendingInsuranceEnrollmentCommand,
} from './types.js';

/** Keeps each insurance command's original path and body until its outcome is known. */
export function createInsuranceRetryPolicy(deps: InsuranceRetryPolicyDeps): InsuranceRetryPolicy {
  const pending = new Map<string, PendingInsuranceCommand>();
  return {
    enroll(snapshot, productVersionId) {
      const key = enrollmentKey(snapshot.runRevision, productVersionId);
      const existing = pending.get(key);
      return existing?.kind === 'enrollment'
        ? existing
        : enrollmentCommand(snapshot, productVersionId, deps.createCommandId());
    },
    cancel(snapshot, contractId) {
      const key = cancellationKey(snapshot.runRevision, contractId);
      const existing = pending.get(key);
      return existing?.kind === 'cancellation'
        ? existing
        : cancellationCommand(snapshot, contractId, deps.createCommandId());
    },
    claim(snapshot, claimId) {
      const key = claimKey(snapshot.runRevision, claimId);
      const existing = pending.get(key);
      return existing?.kind === 'claim'
        ? existing
        : claimCommand(snapshot, claimId, deps.createCommandId());
    },
    pendingEnrollment(runRevision, productVersionId) {
      const command = pending.get(enrollmentKey(runRevision, productVersionId));
      return command?.kind === 'enrollment' ? command : undefined;
    },
    pendingCancellation(runRevision, contractId) {
      const command = pending.get(cancellationKey(runRevision, contractId));
      return command?.kind === 'cancellation' ? command : undefined;
    },
    pendingClaim(runRevision, claimId) {
      const command = pending.get(claimKey(runRevision, claimId));
      return command?.kind === 'claim' ? command : undefined;
    },
    complete(command) {
      clear(pending, command);
    },
    fail(command, error) {
      if (error instanceof InsuranceCommandError) clear(pending, command);
      else pending.set(commandKey(command), command);
    },
  };
}

function enrollmentCommand(
  snapshot: InsuranceCommandCursorSource,
  productVersionId: string,
  commandId: string,
): PendingInsuranceEnrollmentCommand {
  return {
    kind: 'enrollment',
    request: {
      ...cursorOf(snapshot, commandId),
      productVersionId,
    },
  };
}

function cancellationCommand(
  snapshot: InsuranceCommandCursorSource,
  contractId: string,
  commandId: string,
): PendingInsuranceCancellationCommand {
  return {
    kind: 'cancellation',
    contractId,
    request: cursorOf(snapshot, commandId),
  };
}

function claimCommand(
  snapshot: InsuranceCommandCursorSource,
  claimId: string,
  commandId: string,
): PendingInsuranceClaimCommand {
  return {
    kind: 'claim',
    request: {
      ...cursorOf(snapshot, commandId),
      claimId,
    },
  };
}

function cursorOf(
  snapshot: InsuranceCommandCursorSource,
  commandId: string,
): {
  readonly commandId: string;
  readonly expectedRunRevision: number;
  readonly expectedStateRevision: number;
  readonly expectedGameDay: number;
} {
  return {
    commandId,
    expectedRunRevision: snapshot.runRevision,
    expectedStateRevision: snapshot.stateRevision,
    expectedGameDay: snapshot.gameDay,
  };
}

function enrollmentKey(runRevision: number, productVersionId: string): string {
  return `enrollment:${runRevision}:${productVersionId}`;
}

function cancellationKey(runRevision: number, contractId: string): string {
  return `cancellation:${runRevision}:${contractId}`;
}

function claimKey(runRevision: number, claimId: string): string {
  return `claim:${runRevision}:${claimId}`;
}

function commandKey(command: PendingInsuranceCommand): string {
  switch (command.kind) {
    case 'enrollment':
      return enrollmentKey(command.request.expectedRunRevision, command.request.productVersionId);
    case 'cancellation':
      return cancellationKey(command.request.expectedRunRevision, command.contractId);
    case 'claim':
      return claimKey(command.request.expectedRunRevision, command.request.claimId);
  }
}

function clear(
  pending: Map<string, PendingInsuranceCommand>,
  command: PendingInsuranceCommand,
): void {
  const key = commandKey(command);
  if (pending.get(key)?.request.commandId === command.request.commandId) pending.delete(key);
}
