import type { z } from 'zod';
import { type HttpClient, HttpError, type ResponseDecoder } from '../lib/http/index.js';
import {
  type CorporationCreateRequest,
  CorporationCreateRequestSchema,
  type CorporationCreateResponse,
  CorporationCreateResponseSchema,
  type CorporationDividendResponse,
  CorporationDividendResponseSchema,
  type CorporationOperatingMonthPageResponse,
  CorporationOperatingMonthPageResponseSchema,
  type CorporationPayoutRequest,
  CorporationPayoutRequestSchema,
  type CorporationSettingsRequest,
  CorporationSettingsRequestSchema,
  type CorporationSettingsResponse,
  CorporationSettingsResponseSchema,
  type CorporationSummary,
  CorporationSummarySchema,
  type CorporationTemplatesResponse,
  CorporationTemplatesResponseSchema,
  type LifeFailureCode,
  LifeFailureSchema,
  ResourceIdSchema,
} from './contracts.js';
import { asDecoder } from './zod-adapters.js';

export interface CorporationApi {
  templates(signal?: AbortSignal): Promise<CorporationTemplatesResponse>;
  detail(corporationId: string, signal?: AbortSignal): Promise<CorporationSummary>;
  create(
    request: CorporationCreateRequest,
    signal?: AbortSignal,
  ): Promise<CorporationCreateResponse>;
  updateSettings(
    corporationId: string,
    request: CorporationSettingsRequest,
    signal?: AbortSignal,
  ): Promise<CorporationSettingsResponse>;
  payDividend(
    corporationId: string,
    request: CorporationPayoutRequest,
    signal?: AbortSignal,
  ): Promise<CorporationDividendResponse>;
  months(
    corporationId: string,
    cursor?: string,
    signal?: AbortSignal,
  ): Promise<CorporationOperatingMonthPageResponse>;
}

export interface CorporationApiDeps {
  readonly http: HttpClient;
}

export class CorporationQueryError extends Error {
  constructor(
    readonly code: LifeFailureCode,
    message: string,
  ) {
    super(message);
    this.name = 'CorporationQueryError';
  }
}

export class CorporationCommandError extends Error {
  constructor(
    readonly code: LifeFailureCode,
    message: string,
  ) {
    super(message);
    this.name = 'CorporationCommandError';
  }
}

export function createCorporationApi(deps: CorporationApiDeps): CorporationApi {
  return {
    templates(signal) {
      return requestQuery(() =>
        deps.http.get(
          '/api/corporations/templates',
          asDecoder(CorporationTemplatesResponseSchema),
          requestOptions(signal),
        ),
      );
    },

    detail(corporationId, signal) {
      const id = ResourceIdSchema.parse(corporationId);
      return requestQuery(() =>
        deps.http.get(
          `/api/corporations/${id}`,
          asDecoder(
            CorporationSummarySchema.refine((summary) => summary.id === id, {
              path: ['id'],
              message: 'corporation detail does not match the requested corporation',
            }),
          ),
          requestOptions(signal),
        ),
      );
    },

    create(request, signal) {
      const body = CorporationCreateRequestSchema.parse(request);
      return requestCommand(() =>
        deps.http.post(
          '/api/corporations',
          body,
          commandDecoder(CorporationCreateResponseSchema, body),
          requestOptions(signal),
        ),
      );
    },

    updateSettings(corporationId, request, signal) {
      const id = ResourceIdSchema.parse(corporationId);
      const body = CorporationSettingsRequestSchema.parse(request);
      return requestCommand(() =>
        deps.http.put(
          `/api/corporations/${id}/settings`,
          body,
          commandDecoder(CorporationSettingsResponseSchema, body, id),
          requestOptions(signal),
        ),
      );
    },

    payDividend(corporationId, request, signal) {
      const id = ResourceIdSchema.parse(corporationId);
      const body = CorporationPayoutRequestSchema.parse(request);
      return requestCommand(() =>
        deps.http.post(
          `/api/corporations/${id}/payouts`,
          body,
          commandDecoder(CorporationDividendResponseSchema, body, id),
          requestOptions(signal),
        ),
      );
    },

    months(corporationId, cursor, signal) {
      const id = ResourceIdSchema.parse(corporationId);
      const path =
        cursor === undefined
          ? `/api/corporations/${id}/months`
          : `/api/corporations/${id}/months?${new URLSearchParams({ cursor }).toString()}`;
      return requestQuery(() =>
        deps.http.get(
          path,
          asDecoder(CorporationOperatingMonthPageResponseSchema),
          requestOptions(signal),
        ),
      );
    },
  };
}

function commandDecoder<
  T extends { result: unknown; replayed: boolean; snapshot: CorporationCommandSnapshot },
>(
  schema: z.ZodType<T>,
  request: CorporationCommandCursor,
  corporationId?: string,
): ResponseDecoder<T> {
  return asDecoder(
    schema.superRefine((response, context) => {
      refineCommandCursor(response, request, context);
      const committed = response.snapshot.life.corporation.current;
      if (
        !response.replayed &&
        (committed === null || committed.id !== corporationIdOrResult(response, corporationId))
      ) {
        context.addIssue({
          code: 'custom',
          path: ['snapshot', 'life', 'corporation', 'current'],
          message: 'a new corporation command must appear in the committed snapshot',
        });
      }
    }),
  );
}

interface CorporationCommandCursor {
  readonly expectedRunRevision: number;
  readonly expectedStateRevision: number;
  readonly expectedGameDay: number;
}

interface CorporationCommandSnapshot {
  readonly runRevision: number;
  readonly stateRevision: number;
  readonly gameDay: number;
  readonly life: {
    readonly corporation: {
      readonly current: { readonly id: string } | null;
    };
  };
}

function corporationIdOrResult(
  response: { readonly result: unknown },
  corporationId: string | undefined,
): string | undefined {
  if (corporationId !== undefined) return corporationId;
  const parsed = CorporationSummarySchema.safeParse(response.result);
  return parsed.success ? parsed.data.id : undefined;
}

function refineCommandCursor(
  response: { readonly replayed: boolean; readonly snapshot: CorporationCommandSnapshot },
  request: CorporationCommandCursor,
  context: z.RefinementCtx,
): void {
  const { snapshot } = response;
  if (
    snapshot.runRevision !== request.expectedRunRevision ||
    snapshot.gameDay < request.expectedGameDay ||
    snapshot.stateRevision < request.expectedStateRevision + 1 ||
    (!response.replayed &&
      (snapshot.gameDay !== request.expectedGameDay ||
        snapshot.stateRevision !== request.expectedStateRevision + 1))
  ) {
    context.addIssue({
      code: 'custom',
      path: ['snapshot'],
      message: 'corporation response does not advance from the submitted cursor',
    });
  }
}

function requestOptions(signal: AbortSignal | undefined): { signal: AbortSignal } | undefined {
  return signal === undefined ? undefined : { signal };
}

async function requestQuery<T>(request: () => Promise<T>): Promise<T> {
  try {
    return await request();
  } catch (error) {
    const domain = domainError(error, 'query');
    if (domain !== undefined) throw domain;
    throw error;
  }
}

async function requestCommand<T>(request: () => Promise<T>): Promise<T> {
  try {
    return await request();
  } catch (error) {
    const domain = domainError(error, 'command');
    if (domain !== undefined) throw domain;
    throw error;
  }
}

function domainError(
  error: unknown,
  kind: 'query' | 'command',
): CorporationQueryError | CorporationCommandError | undefined {
  if (!(error instanceof HttpError) || error.status >= 500) return undefined;
  const parsed = LifeFailureSchema.safeParse(error.body);
  if (!parsed.success) return undefined;
  return kind === 'query'
    ? new CorporationQueryError(parsed.data.code, parsed.data.message)
    : new CorporationCommandError(parsed.data.code, parsed.data.message);
}
