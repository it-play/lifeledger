import type { GameSnapshot, LifeEventChoiceRequest } from '../../api/contracts.js';

export type LifeEventCommandCursorSource = Pick<
  GameSnapshot,
  'runRevision' | 'stateRevision' | 'gameDay'
>;

export interface PendingLifeEventChoiceCommand {
  readonly eventId: string;
  readonly request: LifeEventChoiceRequest;
}

export interface LifeEventRetryPolicyDeps {
  readonly createCommandId: () => string;
}

export interface LifeEventChoiceRetryPolicy {
  select(
    snapshot: LifeEventCommandCursorSource,
    eventId: string,
    choiceId: string,
  ): PendingLifeEventChoiceCommand;
  pending(runRevision: number, eventId: string): PendingLifeEventChoiceCommand | undefined;
  complete(command: PendingLifeEventChoiceCommand): void;
  fail(command: PendingLifeEventChoiceCommand, error: unknown): void;
}
