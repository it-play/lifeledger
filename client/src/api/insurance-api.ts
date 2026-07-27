import { type HttpClient, HttpError, type ResponseDecoder } from '../lib/http/index.js';
import {
  type InsuranceCancellationRequest,
  InsuranceCancellationRequestSchema,
  type InsuranceCancellationResponse,
  InsuranceCancellationResponseSchema,
  type InsuranceClaimRequest,
  InsuranceClaimRequestSchema,
  type InsuranceClaimResponse,
  InsuranceClaimResponseSchema,
  type InsuranceContractsQuery,
  InsuranceContractsQuerySchema,
  type InsuranceContractsResponse,
  InsuranceContractsResponseSchema,
  type InsuranceEnrollmentRequest,
  InsuranceEnrollmentRequestSchema,
  type InsuranceEnrollmentResponse,
  InsuranceEnrollmentResponseSchema,
  type InsuranceFailureCode,
  InsuranceFailureSchema,
  ResourceIdSchema,
} from './contracts.js';
import { asDecoder } from './zod-adapters.js';

export interface InsuranceApi {
  list(query?: InsuranceContractsQuery, signal?: AbortSignal): Promise<InsuranceContractsResponse>;
  enroll(
    request: InsuranceEnrollmentRequest,
    signal?: AbortSignal,
  ): Promise<InsuranceEnrollmentResponse>;
  cancel(
    contractId: string,
    request: InsuranceCancellationRequest,
    signal?: AbortSignal,
  ): Promise<InsuranceCancellationResponse>;
  fileClaim(request: InsuranceClaimRequest, signal?: AbortSignal): Promise<InsuranceClaimResponse>;
}

export interface InsuranceApiDeps {
  readonly http: HttpClient;
}

/** A validated insurance-query rejection, independent of its transport status. */
export class InsuranceQueryError extends Error {
  constructor(
    readonly code: InsuranceFailureCode,
    message: string,
  ) {
    super(message);
    this.name = 'InsuranceQueryError';
  }
}

/** A validated insurance-command rejection whose outcome is known. */
export class InsuranceCommandError extends Error {
  constructor(
    readonly code: InsuranceFailureCode,
    message: string,
  ) {
    super(message);
    this.name = 'InsuranceCommandError';
  }
}

export function createInsuranceApi(deps: InsuranceApiDeps): InsuranceApi {
  const listDecoder = asDecoder(InsuranceContractsResponseSchema);
  return {
    list(query, signal) {
      const parsed = InsuranceContractsQuerySchema.parse(query ?? {});
      const params = new URLSearchParams();
      if (parsed.cursor !== undefined) params.set('cursor', parsed.cursor);
      const suffix = params.size === 0 ? '' : `?${params.toString()}`;
      return requestInsuranceQuery(() =>
        deps.http.get(
          `/api/insurance/contracts${suffix}`,
          listDecoder,
          signal === undefined ? undefined : { signal },
        ),
      );
    },

    enroll(request, signal) {
      const body = InsuranceEnrollmentRequestSchema.parse(request);
      return requestInsuranceCommand(() =>
        deps.http.post(
          '/api/insurance/contracts',
          body,
          insuranceEnrollmentDecoder(body),
          signal === undefined ? undefined : { signal },
        ),
      );
    },

    cancel(contractId, request, signal) {
      const pathContractId = ResourceIdSchema.parse(contractId);
      const body = InsuranceCancellationRequestSchema.parse(request);
      return requestInsuranceCommand(() =>
        deps.http.post(
          `/api/insurance/contracts/${pathContractId}/cancellations`,
          body,
          insuranceCancellationDecoder(pathContractId, body),
          signal === undefined ? undefined : { signal },
        ),
      );
    },

    fileClaim(request, signal) {
      const body = InsuranceClaimRequestSchema.parse(request);
      return requestInsuranceCommand(() =>
        deps.http.post(
          '/api/insurance/claims',
          body,
          insuranceClaimDecoder(body),
          signal === undefined ? undefined : { signal },
        ),
      );
    },
  };
}

function insuranceEnrollmentDecoder(
  request: InsuranceEnrollmentRequest,
): ResponseDecoder<InsuranceEnrollmentResponse> {
  return asDecoder(
    InsuranceEnrollmentResponseSchema.superRefine((response, context) => {
      const { result, snapshot } = response;
      if (
        result.productVersionId !== request.productVersionId ||
        result.coverageStartGameDay !== request.expectedGameDay
      ) {
        context.addIssue({
          code: 'custom',
          path: ['result'],
          message: 'insurance enrollment result does not match the submitted product and day',
        });
      }
      for (const issue of insuranceCommandSnapshotIssues(response, request)) {
        context.addIssue({ code: 'custom', path: [...issue.path], message: issue.message });
      }
      if (!response.replayed) {
        const active = snapshot.life.activeInsuranceContracts.find(
          (contract) => contract.id === result.contractId,
        );
        if (
          active === undefined ||
          active.productVersionId !== result.productVersionId ||
          active.status !== result.status ||
          active.coverageStartGameDay !== result.coverageStartGameDay ||
          active.waitingEndsGameDay !== result.waitingEndsGameDay ||
          active.coverageEndExclusive !== result.coverageEndExclusive ||
          active.nextPremiumDueGameDay !== result.nextPremiumDueGameDay ||
          active.premiumKrw !== result.premiumKrw
        ) {
          context.addIssue({
            code: 'custom',
            path: ['snapshot', 'life', 'activeInsuranceContracts'],
            message: 'new insurance enrollment must appear in the committed active summary',
          });
        }
      }
    }),
  );
}

function insuranceCancellationDecoder(
  contractId: string,
  request: InsuranceCancellationRequest,
): ResponseDecoder<InsuranceCancellationResponse> {
  return asDecoder(
    InsuranceCancellationResponseSchema.superRefine((response, context) => {
      if (
        response.result.contractId !== contractId ||
        response.result.coverageEndExclusive !== request.expectedGameDay + 1
      ) {
        context.addIssue({
          code: 'custom',
          path: ['result'],
          message: 'insurance cancellation result does not match the submitted contract and day',
        });
      }
      for (const issue of insuranceCommandSnapshotIssues(response, request)) {
        context.addIssue({ code: 'custom', path: [...issue.path], message: issue.message });
      }
      if (
        !response.replayed &&
        response.snapshot.life.activeInsuranceContracts.some(
          (contract) => contract.id === contractId,
        )
      ) {
        context.addIssue({
          code: 'custom',
          path: ['snapshot', 'life', 'activeInsuranceContracts'],
          message: 'a cancelled insurance contract cannot remain in the active summary',
        });
      }
    }),
  );
}

function insuranceClaimDecoder(
  request: InsuranceClaimRequest,
): ResponseDecoder<InsuranceClaimResponse> {
  return asDecoder(
    InsuranceClaimResponseSchema.superRefine((response, context) => {
      if (
        response.result.claimId !== request.claimId ||
        response.result.paidGameDay !== request.expectedGameDay
      ) {
        context.addIssue({
          code: 'custom',
          path: ['result'],
          message: 'insurance claim result does not match the submitted claim and day',
        });
      }
      for (const issue of insuranceCommandSnapshotIssues(response, request)) {
        context.addIssue({ code: 'custom', path: [...issue.path], message: issue.message });
      }
      if (
        response.snapshot.life.pendingInsuranceClaims.some((claim) => claim.id === request.claimId)
      ) {
        context.addIssue({
          code: 'custom',
          path: ['snapshot', 'life', 'pendingInsuranceClaims'],
          message: 'a paid insurance claim cannot remain in the pending summary',
        });
      }
    }),
  );
}

interface InsuranceProtocolIssue {
  readonly path: readonly (string | number)[];
  readonly message: string;
}

function insuranceCommandSnapshotIssues(
  response: {
    readonly replayed: boolean;
    readonly snapshot: {
      readonly runRevision: number;
      readonly stateRevision: number;
      readonly gameDay: number;
    };
  },
  request: {
    readonly expectedRunRevision: number;
    readonly expectedStateRevision: number;
    readonly expectedGameDay: number;
  },
): readonly InsuranceProtocolIssue[] {
  const issues: InsuranceProtocolIssue[] = [];
  const { snapshot } = response;
  const sameRun = snapshot.runRevision === request.expectedRunRevision;
  if (
    snapshot.runRevision < request.expectedRunRevision ||
    (sameRun &&
      (snapshot.gameDay < request.expectedGameDay ||
        BigInt(snapshot.stateRevision) < BigInt(request.expectedStateRevision) + 1n))
  ) {
    issues.push({
      path: ['snapshot'],
      message: 'insurance command response does not advance from the submitted cursor',
    });
  }
  if (
    !response.replayed &&
    (!sameRun ||
      snapshot.gameDay !== request.expectedGameDay ||
      BigInt(snapshot.stateRevision) !== BigInt(request.expectedStateRevision) + 1n)
  ) {
    issues.push({
      path: ['snapshot', 'stateRevision'],
      message: 'a new insurance command must advance state exactly once',
    });
  }
  return issues;
}

async function requestInsuranceQuery<T>(request: () => Promise<T>): Promise<T> {
  try {
    return await request();
  } catch (error) {
    const domain = toInsuranceQueryError(error);
    if (domain !== undefined) throw domain;
    throw error;
  }
}

async function requestInsuranceCommand<T>(request: () => Promise<T>): Promise<T> {
  try {
    return await request();
  } catch (error) {
    const domain = toInsuranceCommandError(error);
    if (domain !== undefined) throw domain;
    throw error;
  }
}

function toInsuranceQueryError(error: unknown): InsuranceQueryError | undefined {
  if (!(error instanceof HttpError) || error.status >= 500) return undefined;
  const parsed = InsuranceFailureSchema.safeParse(error.body);
  if (!parsed.success) return undefined;
  return new InsuranceQueryError(parsed.data.code, parsed.data.message);
}

function toInsuranceCommandError(error: unknown): InsuranceCommandError | undefined {
  if (!(error instanceof HttpError) || error.status >= 500) return undefined;
  const parsed = InsuranceFailureSchema.safeParse(error.body);
  if (!parsed.success) return undefined;
  return new InsuranceCommandError(parsed.data.code, parsed.data.message);
}
