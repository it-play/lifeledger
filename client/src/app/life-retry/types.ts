import type {
  EssentialArrearPaymentDraft,
  EssentialArrearPaymentRequest,
  GameSnapshot,
  LifeBudgetUpdateDraft,
  LifeBudgetUpdateRequest,
} from '../../api/contracts.js';

export type LifeCommandCursorSource = Pick<
  GameSnapshot,
  'runRevision' | 'stateRevision' | 'gameDay'
>;

export interface LifeRetryPolicyDeps {
  readonly createCommandId: () => string;
}

export interface LifeBudgetRetryPolicy {
  select(snapshot: LifeCommandCursorSource, draft: LifeBudgetUpdateDraft): LifeBudgetUpdateRequest;
  complete(request: LifeBudgetUpdateRequest): void;
  fail(request: LifeBudgetUpdateRequest, error: unknown): void;
}

export interface EssentialArrearPaymentCommand {
  readonly arrearId: string;
  readonly request: EssentialArrearPaymentRequest;
}

export interface EssentialArrearPaymentRetryPolicy {
  select(
    snapshot: LifeCommandCursorSource,
    arrearId: string,
    draft: EssentialArrearPaymentDraft,
  ): EssentialArrearPaymentCommand;
  complete(command: EssentialArrearPaymentCommand): void;
  fail(command: EssentialArrearPaymentCommand, error: unknown): void;
}
