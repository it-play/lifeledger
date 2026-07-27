import type { GameSnapshot, WelfareApplicationRequest } from '../../api/contracts.js';

export type WelfareCommandCursorSource = Pick<
  GameSnapshot,
  'runRevision' | 'stateRevision' | 'gameDay'
>;

export interface WelfareRetryPolicyDeps {
  readonly createCommandId: () => string;
}

export interface WelfareApplicationRetryPolicy {
  select(snapshot: WelfareCommandCursorSource, programVersionId: string): WelfareApplicationRequest;
  pending(runRevision: number, programVersionId: string): WelfareApplicationRequest | undefined;
  complete(request: WelfareApplicationRequest): void;
  fail(request: WelfareApplicationRequest, error: unknown): void;
}
