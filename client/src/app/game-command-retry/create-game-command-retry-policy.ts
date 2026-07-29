import type {
  AdvanceRequest,
  CharacterStartV2Draft,
  CharacterStartV2Request,
  GameSnapshot,
  RunStartDraft,
  RunStartRequest,
} from '../../api/contracts.js';
import type {
  AdvanceRetryPolicy,
  CharacterStartRetryPolicy,
  GameCommandRetryPolicyDeps,
  RunStartRetryPolicy,
} from './types.js';

export function createCharacterStartRetryPolicy(
  deps: GameCommandRetryPolicyDeps,
): CharacterStartRetryPolicy {
  const pending = new Map<string, CharacterStartV2Request>();

  return {
    select(snapshot, draft) {
      const key = characterKey(draft);
      return pending.get(key) ?? characterRequestOf(snapshot, draft, deps.createCommandId());
    },
    retain(request) {
      pending.set(characterKey(request), request);
    },
    clear(request) {
      const key = characterKey(request);
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

export function createRunStartRetryPolicy(deps: GameCommandRetryPolicyDeps): RunStartRetryPolicy {
  const pending = new Map<string, RunStartRequest>();

  return {
    select(snapshot, draft) {
      const key = runStartKey(draft);
      return pending.get(key) ?? runStartRequestOf(snapshot, draft, deps.createCommandId());
    },
    retain(request) {
      pending.set(runStartKey(request), request);
    },
    clear(request) {
      const key = runStartKey(request);
      if (pending.get(key)?.commandId === request.commandId) pending.delete(key);
    },
  };
}

function characterRequestOf(
  snapshot: GameSnapshot,
  draft: CharacterStartV2Draft,
  commandId: string,
): CharacterStartV2Request {
  return {
    commandId,
    expectedRunRevision: snapshot.runRevision,
    expectedStateRevision: snapshot.stateRevision,
    expectedGameDay: snapshot.gameDay,
    ...draft,
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

function runStartRequestOf(
  snapshot: GameSnapshot,
  draft: RunStartDraft,
  commandId: string,
): RunStartRequest {
  return {
    commandId,
    expectedRunRevision: snapshot.runRevision,
    expectedStateRevision: snapshot.stateRevision,
    expectedGameDay: snapshot.gameDay,
    ...draft,
  };
}

function characterKey(draft: CharacterStartV2Draft): string {
  const { character } = draft;
  return JSON.stringify([
    character.name,
    character.age,
    character.gender,
    character.military,
    character.region,
    character.background,
    character.education,
    character.careerYears,
    character.certifications,
    character.startingCashKrw,
    character.health,
    character.dependents,
    draft.startingLoans.map((loan) => [loan.kind, loan.productVersionId, loan.principalKrw]),
  ]);
}

function advanceKey(runRevision: number, days: number): string {
  return `${runRevision}:${days}`;
}

function runStartKey(draft: RunStartDraft): string {
  switch (draft.mode) {
    case 'rankedPreset':
      return JSON.stringify([draft.mode, draft.characterPresetVersionId]);
    case 'rankedCustom':
      return JSON.stringify([
        draft.mode,
        draft.pointBudgetVersionId,
        draft.selections.map((selection) => [selection.optionId, selection.quantity]),
      ]);
    case 'sandbox':
      return JSON.stringify([draft.mode, characterKey(draft)]);
  }
}
