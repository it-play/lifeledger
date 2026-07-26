import type {
  GameSnapshot,
  IsaAccountCloseDraft,
  IsaAccountCloseRequest,
  PensionStartDraft,
  PensionStartRequest,
  PensionWithdrawalDraft,
  PensionWithdrawalRequest,
  TaxAccountOpenDraft,
  TaxAccountOpenRequest,
} from '../../api/contracts.js';

export interface TaxAccountRetryPolicyDeps {
  readonly createCommandId: () => string;
}

export interface IsaAccountCloseCommand {
  readonly accountId: string;
  readonly request: IsaAccountCloseRequest;
}

export interface PensionStartCommand {
  readonly accountId: string;
  readonly request: PensionStartRequest;
}

export interface PensionWithdrawalCommand {
  readonly accountId: string;
  readonly request: PensionWithdrawalRequest;
}

export interface TaxAccountOpenRetryPolicy {
  select(snapshot: GameSnapshot, draft: TaxAccountOpenDraft): TaxAccountOpenRequest;
  complete(request: TaxAccountOpenRequest): void;
  fail(request: TaxAccountOpenRequest, error: unknown): void;
}

export interface IsaAccountCloseRetryPolicy {
  select(snapshot: GameSnapshot, draft: IsaAccountCloseDraft): IsaAccountCloseCommand;
  complete(command: IsaAccountCloseCommand): void;
  fail(command: IsaAccountCloseCommand, error: unknown): void;
}

export interface PensionStartRetryPolicy {
  select(snapshot: GameSnapshot, draft: PensionStartDraft): PensionStartCommand;
  complete(command: PensionStartCommand): void;
  fail(command: PensionStartCommand, error: unknown): void;
}

export interface PensionWithdrawalRetryPolicy {
  select(snapshot: GameSnapshot, draft: PensionWithdrawalDraft): PensionWithdrawalCommand;
  complete(command: PensionWithdrawalCommand): void;
  fail(command: PensionWithdrawalCommand, error: unknown): void;
}
