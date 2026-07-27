import type {
  GameSnapshot,
  InsuranceCancellationRequest,
  InsuranceClaimRequest,
  InsuranceEnrollmentRequest,
} from '../../api/contracts.js';

export type InsuranceCommandCursorSource = Pick<
  GameSnapshot,
  'runRevision' | 'stateRevision' | 'gameDay'
>;

export interface PendingInsuranceEnrollmentCommand {
  readonly kind: 'enrollment';
  readonly request: InsuranceEnrollmentRequest;
}

export interface PendingInsuranceCancellationCommand {
  readonly kind: 'cancellation';
  readonly contractId: string;
  readonly request: InsuranceCancellationRequest;
}

export interface PendingInsuranceClaimCommand {
  readonly kind: 'claim';
  readonly request: InsuranceClaimRequest;
}

export type PendingInsuranceCommand =
  | PendingInsuranceEnrollmentCommand
  | PendingInsuranceCancellationCommand
  | PendingInsuranceClaimCommand;

export interface InsuranceRetryPolicyDeps {
  readonly createCommandId: () => string;
}

export interface InsuranceRetryPolicy {
  enroll(
    snapshot: InsuranceCommandCursorSource,
    productVersionId: string,
  ): PendingInsuranceEnrollmentCommand;
  cancel(
    snapshot: InsuranceCommandCursorSource,
    contractId: string,
  ): PendingInsuranceCancellationCommand;
  claim(snapshot: InsuranceCommandCursorSource, claimId: string): PendingInsuranceClaimCommand;
  pendingEnrollment(
    runRevision: number,
    productVersionId: string,
  ): PendingInsuranceEnrollmentCommand | undefined;
  pendingCancellation(
    runRevision: number,
    contractId: string,
  ): PendingInsuranceCancellationCommand | undefined;
  pendingClaim(runRevision: number, claimId: string): PendingInsuranceClaimCommand | undefined;
  complete(command: PendingInsuranceCommand): void;
  fail(command: PendingInsuranceCommand, error: unknown): void;
}
