import { type HttpClient, HttpError, type ResponseDecoder } from '../lib/http/index.js';
import {
  type EssentialArrearPaymentRequest,
  EssentialArrearPaymentRequestSchema,
  type EssentialArrearPaymentResponse,
  EssentialArrearPaymentResponseSchema,
  type LifeBudgetResponse,
  LifeBudgetResponseSchema,
  type LifeBudgetUpdateRequest,
  LifeBudgetUpdateRequestSchema,
  type LifeBudgetUpdateResponse,
  LifeBudgetUpdateResponseSchema,
  type LifeFailureCode,
  LifeFailureSchema,
  ResourceIdSchema,
} from './contracts.js';
import { asDecoder } from './zod-adapters.js';

export interface LifeApi {
  getBudget(signal?: AbortSignal): Promise<LifeBudgetResponse>;
  updateBudget(request: LifeBudgetUpdateRequest): Promise<LifeBudgetUpdateResponse>;
  payEssentialArrear(
    arrearId: string,
    request: EssentialArrearPaymentRequest,
  ): Promise<EssentialArrearPaymentResponse>;
}

export interface LifeApiDeps {
  readonly http: HttpClient;
}

/** A validated life-command rejection, independent of its transport status. */
export class LifeCommandError extends Error {
  constructor(
    readonly code: LifeFailureCode,
    message: string,
  ) {
    super(message);
    this.name = 'LifeCommandError';
  }
}

export function createLifeApi(deps: LifeApiDeps): LifeApi {
  const budgetDecoder = asDecoder(LifeBudgetResponseSchema);

  return {
    getBudget(signal) {
      return requestLife(() =>
        deps.http.get(
          '/api/life/budget',
          budgetDecoder,
          signal === undefined ? undefined : { signal },
        ),
      );
    },

    updateBudget(request) {
      const body = LifeBudgetUpdateRequestSchema.parse(request);
      const decoder = budgetUpdateDecoder(body);
      return requestLife(() => deps.http.put('/api/life/budget', body, decoder));
    },

    payEssentialArrear(arrearId, request) {
      const id = ResourceIdSchema.parse(arrearId);
      const body = EssentialArrearPaymentRequestSchema.parse(request);
      const decoder = arrearPaymentDecoder(id, body);
      return requestLife(() => deps.http.post(`/api/life/arrears/${id}/payments`, body, decoder));
    },
  };
}

function budgetUpdateDecoder(
  request: LifeBudgetUpdateRequest,
): ResponseDecoder<LifeBudgetUpdateResponse> {
  const expectedSelections = new Map(
    request.selections.map((selection) => [selection.category, selection.bandId]),
  );
  return asDecoder(
    LifeBudgetUpdateResponseSchema.superRefine((response, context) => {
      if (response.result.appliedGameDay !== request.expectedGameDay) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'appliedGameDay'],
          message: 'budget result does not match the original command day',
        });
      }
      const matches = response.result.selections.every(
        (selection) => expectedSelections.get(selection.category) === selection.bandId,
      );
      if (!matches) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'selections'],
          message: 'budget result does not match the submitted selections',
        });
      }
    }),
  );
}

function arrearPaymentDecoder(
  arrearId: string,
  request: EssentialArrearPaymentRequest,
): ResponseDecoder<EssentialArrearPaymentResponse> {
  return asDecoder(
    EssentialArrearPaymentResponseSchema.superRefine((response, context) => {
      if (response.result.arrearId !== arrearId) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'arrearId'],
          message: 'arrear payment result does not match the path arrear',
        });
      }
      if (response.result.paidKrw !== request.amountKrw) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'paidKrw'],
          message: 'arrear payment result does not match the submitted amount',
        });
      }
    }),
  );
}

async function requestLife<T>(request: () => Promise<T>): Promise<T> {
  try {
    return await request();
  } catch (error) {
    const domain = toLifeCommandError(error);
    if (domain !== undefined) throw domain;
    throw error;
  }
}

function toLifeCommandError(error: unknown): LifeCommandError | undefined {
  if (!(error instanceof HttpError)) return undefined;
  const parsed = LifeFailureSchema.safeParse(error.body);
  if (!parsed.success) return undefined;
  return new LifeCommandError(parsed.data.code, parsed.data.message);
}
