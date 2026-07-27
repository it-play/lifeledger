import { CareerCommandError } from '../../api/career-api.js';
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
  GameSnapshot,
  MilitarySavingsEnrollmentDraft,
  MilitarySavingsEnrollmentRequest,
  MilitaryServiceStartDraft,
  MilitaryServiceStartRequest,
} from '../../api/contracts.js';
import type {
  CareerActivityCancelRetryPolicy,
  CareerActivityStartRetryPolicy,
  CareerApplicationRetryPolicy,
  CareerArtifactRetryPolicy,
  CareerCancelCommand,
  CareerCancelDraft,
  CareerFocusRetryPolicy,
  CareerInterviewCommand,
  CareerInterviewRetryPolicy,
  CareerPathAction,
  CareerPathCommand,
  CareerPathRetryPolicy,
  CareerRetryPolicyDeps,
  MilitarySavingsCloseCommand,
  MilitarySavingsCloseRetryPolicy,
  MilitarySavingsEnrollmentRetryPolicy,
  MilitaryServiceStartRetryPolicy,
} from './types.js';

export function createCareerFocusRetryPolicy(deps: CareerRetryPolicyDeps): CareerFocusRetryPolicy {
  const pending = new Map<string, CareerFocusRequest>();
  return {
    select(snapshot, draft) {
      const key = focusKey(snapshot.runRevision, draft);
      return pending.get(key) ?? { ...cursorOf(snapshot, deps), ...draft };
    },
    complete(request) {
      clearRequest(pending, focusRequestKey(request), request.commandId);
    },
    fail(request, error) {
      retainTransportFailure(pending, focusRequestKey(request), request, error);
    },
  };
}

export function createCareerActivityStartRetryPolicy(
  deps: CareerRetryPolicyDeps,
): CareerActivityStartRetryPolicy {
  const pending = new Map<string, CareerActivityStartRequest>();
  return {
    select(snapshot, draft) {
      const key = activityStartKey(snapshot.runRevision, draft);
      return pending.get(key) ?? { ...cursorOf(snapshot, deps), ...draft };
    },
    complete(request) {
      clearRequest(pending, activityStartRequestKey(request), request.commandId);
    },
    fail(request, error) {
      retainTransportFailure(pending, activityStartRequestKey(request), request, error);
    },
  };
}

export function createCareerActivityCancelRetryPolicy(
  deps: CareerRetryPolicyDeps,
): CareerActivityCancelRetryPolicy {
  const pending = new Map<string, CareerCancelCommand>();
  return {
    select(snapshot, draft) {
      const key = activityCancelKey(snapshot.runRevision, draft);
      return (
        pending.get(key) ?? {
          activityId: draft.activityId,
          request: cursorOf(snapshot, deps),
        }
      );
    },
    complete(command) {
      const key = activityCancelCommandKey(command);
      if (pending.get(key)?.request.commandId === command.request.commandId) pending.delete(key);
    },
    fail(command, error) {
      const key = activityCancelCommandKey(command);
      if (error instanceof CareerCommandError) {
        if (pending.get(key)?.request.commandId === command.request.commandId) pending.delete(key);
      } else {
        pending.set(key, command);
      }
    },
  };
}

export function createCareerArtifactRetryPolicy(
  deps: CareerRetryPolicyDeps,
): CareerArtifactRetryPolicy {
  const pending = new Map<string, CareerArtifactPublishRequest>();
  return {
    select(snapshot, draft) {
      const key = artifactKey(snapshot.runRevision, draft);
      return pending.get(key) ?? artifactRequestOf(snapshot, draft, deps);
    },
    complete(request) {
      clearRequest(pending, artifactRequestKey(request), request.commandId);
    },
    fail(request, error) {
      retainTransportFailure(pending, artifactRequestKey(request), request, error);
    },
  };
}

export function createCareerApplicationRetryPolicy(
  deps: CareerRetryPolicyDeps,
): CareerApplicationRetryPolicy {
  const pending = new Map<string, CareerApplicationRequest>();
  return {
    select(snapshot, draft) {
      const key = applicationKey(snapshot.runRevision, draft);
      return pending.get(key) ?? { ...cursorOf(snapshot, deps), ...draft };
    },
    complete(request) {
      clearRequest(pending, applicationRequestKey(request), request.commandId);
    },
    fail(request, error) {
      retainTransportFailure(pending, applicationRequestKey(request), request, error);
    },
  };
}

export function createCareerInterviewRetryPolicy(
  deps: CareerRetryPolicyDeps,
): CareerInterviewRetryPolicy {
  const pending = new Map<string, CareerInterviewCommand>();
  return {
    select(snapshot, draft) {
      const key = interviewKey(snapshot.runRevision, draft.applicationId, draft.decision);
      return (
        pending.get(key) ?? {
          applicationId: draft.applicationId,
          request: { ...cursorOf(snapshot, deps), decision: draft.decision },
        }
      );
    },
    complete(command) {
      clearPathCommand(pending, interviewCommandKey(command), command.request.commandId);
    },
    fail(command, error) {
      retainPathTransportFailure(pending, interviewCommandKey(command), command, error);
    },
  };
}

export function createCareerPathRetryPolicy(deps: CareerRetryPolicyDeps): CareerPathRetryPolicy {
  const pending = new Map<string, CareerPathCommand>();
  return {
    select(snapshot, action, resourceId) {
      const key = pathKey(snapshot.runRevision, action, resourceId);
      return pending.get(key) ?? { action, resourceId, request: cursorOf(snapshot, deps) };
    },
    complete(command) {
      clearPathCommand(pending, pathCommandKey(command), command.request.commandId);
    },
    fail(command, error) {
      retainPathTransportFailure(pending, pathCommandKey(command), command, error);
    },
  };
}

export function createMilitaryServiceStartRetryPolicy(
  deps: CareerRetryPolicyDeps,
): MilitaryServiceStartRetryPolicy {
  const pending = new Map<string, MilitaryServiceStartRequest>();
  return {
    select(snapshot, draft) {
      const key = militaryServiceStartKey(snapshot.runRevision, draft);
      return pending.get(key) ?? { ...cursorOf(snapshot, deps), ...draft };
    },
    complete(request) {
      clearRequest(pending, militaryServiceStartRequestKey(request), request.commandId);
    },
    fail(request, error) {
      retainTransportFailure(pending, militaryServiceStartRequestKey(request), request, error);
    },
  };
}

export function createMilitarySavingsEnrollmentRetryPolicy(
  deps: CareerRetryPolicyDeps,
): MilitarySavingsEnrollmentRetryPolicy {
  const pending = new Map<string, MilitarySavingsEnrollmentRequest>();
  return {
    select(snapshot, draft) {
      const key = militarySavingsEnrollmentKey(snapshot.runRevision, draft);
      return pending.get(key) ?? { ...cursorOf(snapshot, deps), ...draft };
    },
    complete(request) {
      clearRequest(pending, militarySavingsEnrollmentRequestKey(request), request.commandId);
    },
    fail(request, error) {
      retainTransportFailure(pending, militarySavingsEnrollmentRequestKey(request), request, error);
    },
  };
}

export function createMilitarySavingsCloseRetryPolicy(
  deps: CareerRetryPolicyDeps,
): MilitarySavingsCloseRetryPolicy {
  const pending = new Map<string, MilitarySavingsCloseCommand>();
  return {
    select(snapshot, contractId) {
      const key = militarySavingsCloseKey(snapshot.runRevision, contractId);
      return pending.get(key) ?? { contractId, request: cursorOf(snapshot, deps) };
    },
    complete(command) {
      clearPathCommand(pending, militarySavingsCloseCommandKey(command), command.request.commandId);
    },
    fail(command, error) {
      retainPathTransportFailure(pending, militarySavingsCloseCommandKey(command), command, error);
    },
  };
}

function cursorOf(snapshot: GameSnapshot, deps: CareerRetryPolicyDeps): CareerCursorRequest {
  return {
    commandId: deps.createCommandId(),
    expectedRunRevision: snapshot.runRevision,
    expectedStateRevision: snapshot.stateRevision,
    expectedGameDay: snapshot.gameDay,
  };
}

function artifactRequestOf(
  snapshot: GameSnapshot,
  draft: CareerArtifactDraft,
  deps: CareerRetryPolicyDeps,
): CareerArtifactPublishRequest {
  return { ...cursorOf(snapshot, deps), ...draft };
}

function focusKey(runRevision: number, draft: CareerFocusDraft): string {
  return JSON.stringify([runRevision, draft.focusedJobFamilyKey]);
}

function focusRequestKey(request: CareerFocusRequest): string {
  return focusKey(request.expectedRunRevision, request);
}

function activityStartKey(runRevision: number, draft: CareerActivityStartDraft): string {
  return JSON.stringify([runRevision, draft.activityCatalogEntryId, draft.priority]);
}

function activityStartRequestKey(request: CareerActivityStartRequest): string {
  return activityStartKey(request.expectedRunRevision, request);
}

function activityCancelKey(runRevision: number, draft: CareerCancelDraft): string {
  return JSON.stringify([runRevision, draft.activityId]);
}

function activityCancelCommandKey(command: CareerCancelCommand): string {
  return activityCancelKey(command.request.expectedRunRevision, command);
}

function artifactKey(runRevision: number, draft: CareerArtifactDraft): string {
  return JSON.stringify([runRevision, draft]);
}

function artifactRequestKey(request: CareerArtifactPublishRequest): string {
  const {
    commandId: _commandId,
    expectedRunRevision,
    expectedStateRevision: _state,
    expectedGameDay: _day,
    ...draft
  } = request;
  return artifactKey(expectedRunRevision, draft);
}

function applicationKey(runRevision: number, draft: CareerApplicationDraft): string {
  return JSON.stringify([runRevision, draft]);
}

function applicationRequestKey(request: CareerApplicationRequest): string {
  const {
    commandId: _commandId,
    expectedStateRevision: _state,
    expectedGameDay: _day,
    expectedRunRevision,
    ...draft
  } = request;
  return applicationKey(expectedRunRevision, draft);
}

function interviewKey(
  runRevision: number,
  applicationId: string,
  decision: 'confirm' | 'decline',
): string {
  return JSON.stringify([runRevision, applicationId, decision]);
}

function interviewCommandKey(command: CareerInterviewCommand): string {
  return interviewKey(
    command.request.expectedRunRevision,
    command.applicationId,
    command.request.decision,
  );
}

function pathKey(runRevision: number, action: CareerPathAction, resourceId: string): string {
  return JSON.stringify([runRevision, action, resourceId]);
}

function pathCommandKey(command: CareerPathCommand): string {
  return pathKey(command.request.expectedRunRevision, command.action, command.resourceId);
}

function militaryServiceStartKey(runRevision: number, draft: MilitaryServiceStartDraft): string {
  return JSON.stringify([runRevision, draft.militaryOptionVersionId]);
}

function militaryServiceStartRequestKey(request: MilitaryServiceStartRequest): string {
  return militaryServiceStartKey(request.expectedRunRevision, request);
}

function militarySavingsEnrollmentKey(
  runRevision: number,
  draft: MilitarySavingsEnrollmentDraft,
): string {
  return JSON.stringify([
    runRevision,
    draft.productVersionId,
    draft.monthlyContributionKrw,
    draft.debitDayOfMonth,
  ]);
}

function militarySavingsEnrollmentRequestKey(request: MilitarySavingsEnrollmentRequest): string {
  return militarySavingsEnrollmentKey(request.expectedRunRevision, request);
}

function militarySavingsCloseKey(runRevision: number, contractId: string): string {
  return JSON.stringify([runRevision, contractId]);
}

function militarySavingsCloseCommandKey(command: MilitarySavingsCloseCommand): string {
  return militarySavingsCloseKey(command.request.expectedRunRevision, command.contractId);
}

function clearRequest<T extends { readonly commandId: string }>(
  pending: Map<string, T>,
  key: string,
  commandId: string,
): void {
  if (pending.get(key)?.commandId === commandId) pending.delete(key);
}

function retainTransportFailure<T extends { readonly commandId: string }>(
  pending: Map<string, T>,
  key: string,
  request: T,
  error: unknown,
): void {
  if (error instanceof CareerCommandError) clearRequest(pending, key, request.commandId);
  else pending.set(key, request);
}

function clearPathCommand<T extends { readonly request: { readonly commandId: string } }>(
  pending: Map<string, T>,
  key: string,
  commandId: string,
): void {
  if (pending.get(key)?.request.commandId === commandId) pending.delete(key);
}

function retainPathTransportFailure<T extends { readonly request: { readonly commandId: string } }>(
  pending: Map<string, T>,
  key: string,
  command: T,
  error: unknown,
): void {
  if (error instanceof CareerCommandError)
    clearPathCommand(pending, key, command.request.commandId);
  else pending.set(key, command);
}
