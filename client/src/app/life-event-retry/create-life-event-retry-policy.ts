import { LifeEventCommandError } from '../../api/life-event-api.js';
import type {
  LifeEventChoiceRetryPolicy,
  LifeEventCommandCursorSource,
  LifeEventRetryPolicyDeps,
  PendingLifeEventChoiceCommand,
} from './types.js';

/** Keeps the original event path and body until the endpoint outcome is known. */
export function createLifeEventChoiceRetryPolicy(
  deps: LifeEventRetryPolicyDeps,
): LifeEventChoiceRetryPolicy {
  const pending = new Map<string, PendingLifeEventChoiceCommand>();
  return {
    select(snapshot, eventId, choiceId) {
      const key = eventKey(snapshot.runRevision, eventId);
      return pending.get(key) ?? commandOf(snapshot, eventId, choiceId, deps.createCommandId());
    },
    pending(runRevision, eventId) {
      return pending.get(eventKey(runRevision, eventId));
    },
    complete(command) {
      clear(pending, command);
    },
    fail(command, error) {
      if (error instanceof LifeEventCommandError) clear(pending, command);
      else pending.set(commandKey(command), command);
    },
  };
}

function commandOf(
  snapshot: LifeEventCommandCursorSource,
  eventId: string,
  choiceId: string,
  commandId: string,
): PendingLifeEventChoiceCommand {
  return {
    eventId,
    request: {
      commandId,
      expectedRunRevision: snapshot.runRevision,
      expectedStateRevision: snapshot.stateRevision,
      expectedGameDay: snapshot.gameDay,
      choiceId,
    },
  };
}

function eventKey(runRevision: number, eventId: string): string {
  return `${runRevision}:${eventId}`;
}

function commandKey(command: PendingLifeEventChoiceCommand): string {
  return eventKey(command.request.expectedRunRevision, command.eventId);
}

function clear(
  pending: Map<string, PendingLifeEventChoiceCommand>,
  command: PendingLifeEventChoiceCommand,
): void {
  const key = commandKey(command);
  if (pending.get(key)?.request.commandId === command.request.commandId) pending.delete(key);
}
