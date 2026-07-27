import type {
  CareerActivityStartDraft,
  CareerActivityStartRequest,
  CareerApplicationDraft,
  CareerApplicationRequest,
  CareerArtifactDraft,
  CareerArtifactPublishRequest,
  CareerCursorRequest,
  CareerFocusDraft,
  CareerFocusRequest,
  CareerInterviewConfirmationRequest,
  GameSnapshot,
  MilitarySavingsEnrollmentDraft,
  MilitarySavingsEnrollmentRequest,
  MilitaryServiceStartDraft,
  MilitaryServiceStartRequest,
} from '../../api/contracts.js';

export interface CareerRetryPolicyDeps {
  readonly createCommandId: () => string;
}

export interface CareerCancelDraft {
  readonly activityId: string;
}

export interface CareerCancelCommand {
  readonly activityId: string;
  readonly request: CareerCursorRequest;
}

export interface CareerFocusRetryPolicy {
  select(snapshot: GameSnapshot, draft: CareerFocusDraft): CareerFocusRequest;
  complete(request: CareerFocusRequest): void;
  fail(request: CareerFocusRequest, error: unknown): void;
}

export interface CareerActivityStartRetryPolicy {
  select(snapshot: GameSnapshot, draft: CareerActivityStartDraft): CareerActivityStartRequest;
  complete(request: CareerActivityStartRequest): void;
  fail(request: CareerActivityStartRequest, error: unknown): void;
}

export interface CareerActivityCancelRetryPolicy {
  select(snapshot: GameSnapshot, draft: CareerCancelDraft): CareerCancelCommand;
  complete(command: CareerCancelCommand): void;
  fail(command: CareerCancelCommand, error: unknown): void;
}

export interface CareerArtifactRetryPolicy {
  select(snapshot: GameSnapshot, draft: CareerArtifactDraft): CareerArtifactPublishRequest;
  complete(request: CareerArtifactPublishRequest): void;
  fail(request: CareerArtifactPublishRequest, error: unknown): void;
}

export interface CareerApplicationRetryPolicy {
  select(snapshot: GameSnapshot, draft: CareerApplicationDraft): CareerApplicationRequest;
  complete(request: CareerApplicationRequest): void;
  fail(request: CareerApplicationRequest, error: unknown): void;
}

export interface CareerInterviewCommand {
  readonly applicationId: string;
  readonly request: CareerInterviewConfirmationRequest;
}

export interface CareerInterviewRetryPolicy {
  select(
    snapshot: GameSnapshot,
    draft: { readonly applicationId: string; readonly decision: 'confirm' | 'decline' },
  ): CareerInterviewCommand;
  complete(command: CareerInterviewCommand): void;
  fail(command: CareerInterviewCommand, error: unknown): void;
}

export type CareerPathAction =
  | 'withdrawApplication'
  | 'acceptInvitation'
  | 'declineInvitation'
  | 'acceptOffer'
  | 'declineOffer';

export interface CareerPathCommand {
  readonly action: CareerPathAction;
  readonly resourceId: string;
  readonly request: CareerCursorRequest;
}

export interface CareerPathRetryPolicy {
  select(snapshot: GameSnapshot, action: CareerPathAction, resourceId: string): CareerPathCommand;
  complete(command: CareerPathCommand): void;
  fail(command: CareerPathCommand, error: unknown): void;
}

export interface MilitaryServiceStartRetryPolicy {
  select(snapshot: GameSnapshot, draft: MilitaryServiceStartDraft): MilitaryServiceStartRequest;
  complete(request: MilitaryServiceStartRequest): void;
  fail(request: MilitaryServiceStartRequest, error: unknown): void;
}

export interface MilitarySavingsEnrollmentRetryPolicy {
  select(
    snapshot: GameSnapshot,
    draft: MilitarySavingsEnrollmentDraft,
  ): MilitarySavingsEnrollmentRequest;
  complete(request: MilitarySavingsEnrollmentRequest): void;
  fail(request: MilitarySavingsEnrollmentRequest, error: unknown): void;
}

export interface MilitarySavingsCloseCommand {
  readonly contractId: string;
  readonly request: CareerCursorRequest;
}

export interface MilitarySavingsCloseRetryPolicy {
  select(snapshot: GameSnapshot, contractId: string): MilitarySavingsCloseCommand;
  complete(command: MilitarySavingsCloseCommand): void;
  fail(command: MilitarySavingsCloseCommand, error: unknown): void;
}
