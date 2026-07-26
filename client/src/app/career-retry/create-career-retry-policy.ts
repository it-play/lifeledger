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
