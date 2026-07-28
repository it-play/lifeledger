import type { z } from 'zod';
import { type HttpClient, HttpError, type ResponseDecoder } from '../lib/http/index.js';
import {
  type InsolvencyCaseActionRequest,
  InsolvencyCaseActionRequestSchema,
  type InsolvencyCaseCommandResponse,
  InsolvencyCaseCommandResponseSchema,
  type InsolvencyCaseDetailResponse,
  InsolvencyCaseDetailResponseSchema,
  type InsolvencyCasePrepareRequest,
  InsolvencyCasePrepareRequestSchema,
  type InsolvencyClaimPageResponse,
  InsolvencyClaimPageResponseSchema,
  type InsolvencyLiquidationPageResponse,
  InsolvencyLiquidationPageResponseSchema,
  type InsolvencyOverviewResponse,
  InsolvencyOverviewResponseSchema,
  type InsolvencyPageQuery,
  InsolvencyPageQuerySchema,
  type LifeFailureCode,
  LifeFailureSchema,
  ResourceIdSchema,
} from './contracts.js';
import { asDecoder } from './zod-adapters.js';

export interface InsolvencyApi {
  overview(signal?: AbortSignal): Promise<InsolvencyOverviewResponse>;
  prepare(
    request: InsolvencyCasePrepareRequest,
    signal?: AbortSignal,
  ): Promise<InsolvencyCaseCommandResponse>;
  act(
    caseId: string,
    request: InsolvencyCaseActionRequest,
    signal?: AbortSignal,
  ): Promise<InsolvencyCaseCommandResponse>;
  detail(caseId: string, signal?: AbortSignal): Promise<InsolvencyCaseDetailResponse>;
  claims(
    caseId: string,
    query?: InsolvencyPageQuery,
    signal?: AbortSignal,
  ): Promise<InsolvencyClaimPageResponse>;
  liquidations(
    caseId: string,
    query?: InsolvencyPageQuery,
    signal?: AbortSignal,
  ): Promise<InsolvencyLiquidationPageResponse>;
}

export interface InsolvencyApiDeps {
  readonly http: HttpClient;
}

/** A validated insolvency-query rejection, independent of its transport status. */
export class InsolvencyQueryError extends Error {
  constructor(
    readonly code: LifeFailureCode,
    message: string,
  ) {
    super(message);
    this.name = 'InsolvencyQueryError';
  }
}

/** A validated insolvency-command rejection whose outcome is known. */
export class InsolvencyCommandError extends Error {
  constructor(
    readonly code: LifeFailureCode,
    message: string,
  ) {
    super(message);
    this.name = 'InsolvencyCommandError';
  }
}

export function createInsolvencyApi(deps: InsolvencyApiDeps): InsolvencyApi {
  return {
    overview(signal) {
      return requestQuery(() =>
        deps.http.get(
          '/api/insolvency',
          asDecoder(InsolvencyOverviewResponseSchema),
          signal === undefined ? undefined : { signal },
        ),
      );
    },

    prepare(request, signal) {
      const body = InsolvencyCasePrepareRequestSchema.parse(request);
      return requestCommand(() =>
        deps.http.post(
          '/api/insolvency/cases',
          body,
          commandDecoder(body, 'prepared'),
          signal === undefined ? undefined : { signal },
        ),
      );
    },

    act(caseId, request, signal) {
      const id = ResourceIdSchema.parse(caseId);
      const body = InsolvencyCaseActionRequestSchema.parse(request);
      const expectedStatus = body.action === 'submit' ? 'rebuilding' : 'withdrawn';
      return requestCommand(() =>
        deps.http.post(
          `/api/insolvency/${id}/actions`,
          body,
          commandDecoder(body, expectedStatus, id),
          signal === undefined ? undefined : { signal },
        ),
      );
    },

    detail(caseId, signal) {
      const id = ResourceIdSchema.parse(caseId);
      return requestQuery(() =>
        deps.http.get(
          `/api/insolvency/${id}`,
          asDecoder(
            InsolvencyCaseDetailResponseSchema.refine((detail) => detail.summary.id === id, {
              path: ['summary', 'id'],
              message: 'insolvency detail does not match the requested case',
            }),
          ),
          signal === undefined ? undefined : { signal },
        ),
      );
    },

    claims(caseId, query, signal) {
      const id = ResourceIdSchema.parse(caseId);
      return requestQuery(() =>
        deps.http.get(
          pagePath(`/api/insolvency/${id}/claims`, query),
          asDecoder(InsolvencyClaimPageResponseSchema),
          signal === undefined ? undefined : { signal },
        ),
      );
    },

    liquidations(caseId, query, signal) {
      const id = ResourceIdSchema.parse(caseId);
      return requestQuery(() =>
        deps.http.get(
          pagePath(`/api/insolvency/${id}/liquidations`, query),
          asDecoder(InsolvencyLiquidationPageResponseSchema),
          signal === undefined ? undefined : { signal },
        ),
      );
    },
  };
}

function pagePath(path: string, query: InsolvencyPageQuery | undefined): string {
  const parsed = InsolvencyPageQuerySchema.parse(query ?? {});
  if (parsed.cursor === undefined) return path;
  const params = new URLSearchParams({ cursor: parsed.cursor });
  return `${path}?${params.toString()}`;
}

function commandDecoder(
  request: InsolvencyCasePrepareRequest | InsolvencyCaseActionRequest,
  expectedStatus: 'prepared' | 'rebuilding' | 'withdrawn',
  expectedCaseId?: string,
): ResponseDecoder<InsolvencyCaseCommandResponse> {
  return asDecoder(
    InsolvencyCaseCommandResponseSchema.superRefine((response, context) => {
      refineCommandResult(response, context, expectedStatus, expectedCaseId);
      refineCommandCursor(response, context, request);
    }),
  );
}

function refineCommandResult(
  response: InsolvencyCaseCommandResponse,
  context: z.RefinementCtx,
  expectedStatus: 'prepared' | 'rebuilding' | 'withdrawn',
  expectedCaseId: string | undefined,
): void {
  const { result, snapshot } = response;
  if (
    result.status !== expectedStatus ||
    (expectedCaseId !== undefined && result.id !== expectedCaseId)
  ) {
    context.addIssue({
      code: 'custom',
      path: ['result'],
      message: 'insolvency result does not match the submitted command',
    });
  }
  const committed = snapshot.life.insolvency.currentCase;
  const matchesCommittedState =
    expectedStatus === 'withdrawn'
      ? committed === null
      : committed !== null && committed.id === result.id;
  if (!response.replayed && !matchesCommittedState) {
    context.addIssue({
      code: 'custom',
      path: ['snapshot', 'life', 'insolvency', 'currentCase'],
      message: 'a new insolvency result must appear in the committed snapshot',
    });
  }
}

function refineCommandCursor(
  response: InsolvencyCaseCommandResponse,
  context: z.RefinementCtx,
  request: InsolvencyCasePrepareRequest | InsolvencyCaseActionRequest,
): void {
  const { snapshot } = response;
  const sameRun = snapshot.runRevision === request.expectedRunRevision;
  if (
    snapshot.runRevision < request.expectedRunRevision ||
    (sameRun &&
      (snapshot.gameDay < request.expectedGameDay ||
        BigInt(snapshot.stateRevision) < BigInt(request.expectedStateRevision) + 1n))
  ) {
    context.addIssue({
      code: 'custom',
      path: ['snapshot'],
      message: 'insolvency command response does not advance from the submitted cursor',
    });
  }
  if (
    !response.replayed &&
    (!sameRun ||
      snapshot.gameDay !== request.expectedGameDay ||
      BigInt(snapshot.stateRevision) !== BigInt(request.expectedStateRevision) + 1n)
  ) {
    context.addIssue({
      code: 'custom',
      path: ['snapshot', 'stateRevision'],
      message: 'a new insolvency command must advance state exactly once',
    });
  }
}

async function requestQuery<T>(request: () => Promise<T>): Promise<T> {
  try {
    return await request();
  } catch (error) {
    const domain = toDomainError(error, 'query');
    if (domain !== undefined) throw domain;
    throw error;
  }
}

async function requestCommand<T>(request: () => Promise<T>): Promise<T> {
  try {
    return await request();
  } catch (error) {
    const domain = toDomainError(error, 'command');
    if (domain !== undefined) throw domain;
    throw error;
  }
}

function toDomainError(
  error: unknown,
  kind: 'query' | 'command',
): InsolvencyQueryError | InsolvencyCommandError | undefined {
  if (!(error instanceof HttpError) || error.status >= 500) return undefined;
  const parsed = LifeFailureSchema.safeParse(error.body);
  if (!parsed.success) return undefined;
  return kind === 'query'
    ? new InsolvencyQueryError(parsed.data.code, parsed.data.message)
    : new InsolvencyCommandError(parsed.data.code, parsed.data.message);
}
