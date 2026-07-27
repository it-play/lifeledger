import { type HttpClient, HttpError, type ResponseDecoder } from '../lib/http/index.js';
import {
  type CreditResponse,
  CreditResponseSchema,
  type LifeFailureCode,
  LifeFailureSchema,
  type LoanDetail,
  LoanDetailSchema,
  type LoanExecutionRequest,
  LoanExecutionRequestSchema,
  type LoanExecutionResponse,
  LoanExecutionResponseSchema,
  type LoanInstallmentHistoryQuery,
  LoanInstallmentHistoryQuerySchema,
  type LoanInstallmentHistoryResponse,
  LoanInstallmentHistoryResponseSchema,
  type LoanPrepaymentRequest,
  LoanPrepaymentRequestSchema,
  type LoanPrepaymentResponse,
  LoanPrepaymentResponseSchema,
  type LoanProductCatalog,
  LoanProductCatalogSchema,
  type LoanQuoteRequest,
  LoanQuoteRequestSchema,
  type LoanQuoteResponse,
  LoanQuoteResponseSchema,
  ResourceIdSchema,
} from './contracts.js';
import { asDecoder } from './zod-adapters.js';

export interface LoanApi {
  getCredit(signal?: AbortSignal): Promise<CreditResponse>;
  listProducts(signal?: AbortSignal): Promise<LoanProductCatalog>;
  getDetail(loanId: string, signal?: AbortSignal): Promise<LoanDetail>;
  getInstallmentHistory(
    loanId: string,
    query?: LoanInstallmentHistoryQuery,
    signal?: AbortSignal,
  ): Promise<LoanInstallmentHistoryResponse>;
  quote(request: LoanQuoteRequest): Promise<LoanQuoteResponse>;
  execute(request: LoanExecutionRequest): Promise<LoanExecutionResponse>;
  prepay(loanId: string, request: LoanPrepaymentRequest): Promise<LoanPrepaymentResponse>;
}

export interface LoanApiDeps {
  readonly http: HttpClient;
}

/** A validated loan-command rejection, independent of its transport status. */
export class LoanCommandError extends Error {
  constructor(
    readonly code: LifeFailureCode,
    message: string,
  ) {
    super(message);
    this.name = 'LoanCommandError';
  }
}

/** Reads server-authoritative loan terms and credit projections. */
export function createLoanApi(deps: LoanApiDeps): LoanApi {
  const creditDecoder = asDecoder(CreditResponseSchema);
  const productDecoder = asDecoder(LoanProductCatalogSchema);

  return {
    getCredit(signal) {
      return deps.http.get(
        '/api/credit',
        creditDecoder,
        signal === undefined ? undefined : { signal },
      );
    },

    listProducts(signal) {
      return deps.http.get(
        '/api/loans/products',
        productDecoder,
        signal === undefined ? undefined : { signal },
      );
    },

    getDetail(loanId, signal) {
      const pathLoanId = ResourceIdSchema.parse(loanId);
      return requestLoanCommand(() =>
        deps.http.get(
          `/api/loans/${pathLoanId}`,
          loanDetailDecoder(pathLoanId),
          signal === undefined ? undefined : { signal },
        ),
      );
    },

    getInstallmentHistory(loanId, query, signal) {
      const pathLoanId = ResourceIdSchema.parse(loanId);
      const parsedQuery = LoanInstallmentHistoryQuerySchema.refine(
        (value) => value.before === undefined || value.before.startsWith(`v1.l${pathLoanId}.`),
        {
          path: ['before'],
          message: 'loan history cursor does not match the path loan',
        },
      ).parse(query ?? {});
      const limit = parsedQuery.limit ?? 50;
      const params = new URLSearchParams();
      if (parsedQuery.before !== undefined) params.set('before', parsedQuery.before);
      if (parsedQuery.limit !== undefined) params.set('limit', String(parsedQuery.limit));
      const suffix = params.size === 0 ? '' : `?${params.toString()}`;
      return requestLoanCommand(() =>
        deps.http.get(
          `/api/loans/${pathLoanId}/installments${suffix}`,
          loanInstallmentHistoryDecoder(pathLoanId, limit),
          signal === undefined ? undefined : { signal },
        ),
      );
    },

    quote(request) {
      const body = LoanQuoteRequestSchema.parse(request);
      return requestLoanCommand(() =>
        deps.http.post('/api/loans/quotes', body, loanQuoteDecoder(body)),
      );
    },

    execute(request) {
      const body = LoanExecutionRequestSchema.parse(request);
      return requestLoanCommand(() =>
        deps.http.post('/api/loans', body, loanExecutionDecoder(body)),
      );
    },

    prepay(loanId, request) {
      const pathLoanId = ResourceIdSchema.parse(loanId);
      const body = LoanPrepaymentRequestSchema.parse(request);
      return requestLoanCommand(() =>
        deps.http.post(
          `/api/loans/${pathLoanId}/prepayments`,
          body,
          loanPrepaymentDecoder(pathLoanId, body),
        ),
      );
    },
  };
}

function loanQuoteDecoder(request: LoanQuoteRequest): ResponseDecoder<LoanQuoteResponse> {
  return asDecoder(
    LoanQuoteResponseSchema.superRefine((response, context) => {
      if (response.result.productVersionId !== request.productVersionId) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'productVersionId'],
          message: 'loan quote result does not match the requested product',
        });
      }
      if (response.result.requestedPrincipalKrw !== request.principalKrw) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'requestedPrincipalKrw'],
          message: 'loan quote result does not match the requested principal',
        });
      }
      if (response.result.createdGameDay !== request.expectedGameDay) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'createdGameDay'],
          message: 'loan quote result does not match the requested game day',
        });
      }
    }),
  );
}

function loanExecutionDecoder(
  request: LoanExecutionRequest,
): ResponseDecoder<LoanExecutionResponse> {
  return asDecoder(
    LoanExecutionResponseSchema.superRefine((response, context) => {
      if (response.result.quoteId !== request.quoteId) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'quoteId'],
          message: 'loan execution result does not match the requested quote',
        });
      }
      if (response.result.activatedGameDay !== request.expectedGameDay) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'activatedGameDay'],
          message: 'loan execution result does not match the requested game day',
        });
      }
    }),
  );
}

function loanPrepaymentDecoder(
  loanId: string,
  request: LoanPrepaymentRequest,
): ResponseDecoder<LoanPrepaymentResponse> {
  return asDecoder(
    LoanPrepaymentResponseSchema.superRefine((response, context) => {
      if (response.result.loanId !== loanId) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'loanId'],
          message: 'loan prepayment result does not match the path loan',
        });
      }
      if (response.result.principalKrw !== request.principalKrw) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'principalKrw'],
          message: 'loan prepayment result does not match the requested principal',
        });
      }
      if (response.result.appliedGameDay !== request.expectedGameDay) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'appliedGameDay'],
          message: 'loan prepayment result does not match the requested game day',
        });
      }
    }),
  );
}

function loanDetailDecoder(loanId: string): ResponseDecoder<LoanDetail> {
  return asDecoder(
    LoanDetailSchema.superRefine((detail, context) => {
      if (detail.id !== loanId) {
        context.addIssue({
          code: 'custom',
          path: ['id'],
          message: 'loan detail does not match the path loan',
        });
      }
    }),
  );
}

function loanInstallmentHistoryDecoder(
  loanId: string,
  limit: number,
): ResponseDecoder<LoanInstallmentHistoryResponse> {
  return asDecoder(
    LoanInstallmentHistoryResponseSchema.superRefine((page, context) => {
      if (page.loanId !== loanId) {
        context.addIssue({
          code: 'custom',
          path: ['loanId'],
          message: 'loan history does not match the path loan',
        });
      }
      if (page.installments.length > limit) {
        context.addIssue({
          code: 'custom',
          path: ['installments'],
          message: 'loan installment window exceeds the requested limit',
        });
      }
      if (page.payments.length > limit) {
        context.addIssue({
          code: 'custom',
          path: ['payments'],
          message: 'loan payment window exceeds the requested limit',
        });
      }
    }),
  );
}

async function requestLoanCommand<T>(request: () => Promise<T>): Promise<T> {
  try {
    return await request();
  } catch (error) {
    const domain = toLoanCommandError(error);
    if (domain !== undefined) throw domain;
    throw error;
  }
}

function toLoanCommandError(error: unknown): LoanCommandError | undefined {
  if (!(error instanceof HttpError)) return undefined;
  if (error.status >= 500) return undefined;
  const parsed = LifeFailureSchema.safeParse(error.body);
  if (!parsed.success) return undefined;
  return new LoanCommandError(parsed.data.code, parsed.data.message);
}
