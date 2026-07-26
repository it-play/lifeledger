import type {
  AdvanceRequest,
  CharacterDraft,
  CharacterStartRequest,
  GameSnapshot,
} from '../../api/contracts.js';
import type {
  AdvanceRetryPolicy,
  CharacterStartRetryPolicy,
  GameCommandRetryPolicyDeps,
} from './types.js';

export function createCharacterStartRetryPolicy(
  deps: GameCommandRetryPolicyDeps,
): CharacterStartRetryPolicy {
  const pending = new Map<string, CharacterStartRequest>();

  return {
    select(snapshot, draft) {
      const key = characterKey(draft);
      return pending.get(key) ?? characterRequestOf(snapshot, draft, deps.createCommandId());
    },
    retain(request) {
      pending.set(characterKey(request.character), request);
    },
    clear(request) {
      const key = characterKey(request.character);
      if (pending.get(key)?.commandId === request.commandId) pending.delete(key);
    },
  };
}

export function createAdvanceRetryPolicy(deps: GameCommandRetryPolicyDeps): AdvanceRetryPolicy {
  const pending = new Map<string, AdvanceRequest>();

  return {
    select(snapshot, days) {
      const key = advanceKey(snapshot.runRevision, days);
      return pending.get(key) ?? advanceRequestOf(snapshot, days, deps.createCommandId());
    },
    retain(request) {
      pending.set(advanceKey(request.expectedRunRevision, request.days), request);
    },
    clear(request) {
      const key = advanceKey(request.expectedRunRevision, request.days);
      if (pending.get(key)?.commandId === request.commandId) pending.delete(key);
    },
  };
}

function characterRequestOf(
  snapshot: GameSnapshot,
  character: CharacterDraft,
  commandId: string,
): CharacterStartRequest {
  return {
    commandId,
    expectedRunRevision: snapshot.runRevision,
    expectedStateRevision: snapshot.stateRevision,
    expectedGameDay: snapshot.gameDay,
    character,
  };
}

function advanceRequestOf(snapshot: GameSnapshot, days: number, commandId: string): AdvanceRequest {
  return {
    commandId,
    expectedRunRevision: snapshot.runRevision,
    expectedStateRevision: snapshot.stateRevision,
    expectedGameDay: snapshot.gameDay,
    days,
  };
}

function characterKey(draft: CharacterDraft): string {
  return JSON.stringify([
    draft.name,
    draft.age,
    draft.gender,
    draft.military,
    draft.region,
    draft.background,
    draft.education,
    draft.careerYears,
    draft.certifications,
    draft.startingCashKrw,
    draft.studentLoanKrw,
    draft.creditLoanKrw,
    draft.health,
    draft.dependents,
  ]);
}

function advanceKey(runRevision: number, days: number): string {
  return `${runRevision}:${days}`;
}
