import type {
  AdvanceRequest,
  CharacterStartV2Draft,
  CharacterStartV2Request,
  GameSnapshot,
} from '../../api/contracts.js';

/** Keeps an unknown character-start result tied to its original cursor and UUID. */
export interface CharacterStartRetryPolicy {
  select(snapshot: GameSnapshot, draft: CharacterStartV2Draft): CharacterStartV2Request;
  retain(request: CharacterStartV2Request): void;
  clear(request: CharacterStartV2Request): void;
}

/** Keeps a partially committed manual advance tied to its original cursor and UUID. */
export interface AdvanceRetryPolicy {
  select(snapshot: GameSnapshot, days: number): AdvanceRequest;
  retain(request: AdvanceRequest): void;
  clear(request: AdvanceRequest): void;
}

export interface GameCommandRetryPolicyDeps {
  readonly createCommandId: () => string;
}
