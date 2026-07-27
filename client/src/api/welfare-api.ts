import { type HttpClient, HttpError, type ResponseDecoder } from '../lib/http/index.js';
import {
  type LifeFailureCode,
  LifeFailureSchema,
  type WelfareApplicationRequest,
  WelfareApplicationRequestSchema,
  type WelfareApplicationResponse,
  WelfareApplicationResponseSchema,
  type WelfareProgramsResponse,
  WelfareProgramsResponseSchema,
} from './contracts.js';
import { asDecoder } from './zod-adapters.js';

export interface WelfareApi {
  listPrograms(signal?: AbortSignal): Promise<WelfareProgramsResponse>;
  apply(
    request: WelfareApplicationRequest,
    signal?: AbortSignal,
  ): Promise<WelfareApplicationResponse>;
}

export interface WelfareApiDeps {
  readonly http: HttpClient;
}

/** A validated welfare-query rejection, independent of its transport status. */
export class WelfareQueryError extends Error {
  constructor(
    readonly code: LifeFailureCode,
    message: string,
  ) {
    super(message);
    this.name = 'WelfareQueryError';
  }
}

/** A validated welfare-command rejection whose outcome is known. */
export class WelfareCommandError extends Error {
  constructor(
    readonly code: LifeFailureCode,
    message: string,
  ) {
    super(message);
    this.name = 'WelfareCommandError';
  }
}

export function createWelfareApi(deps: WelfareApiDeps): WelfareApi {
  const programsDecoder = asDecoder(WelfareProgramsResponseSchema);

  return {
    listPrograms(signal) {
      return requestWelfareQuery(() =>
        deps.http.get(
          '/api/welfare/programs',
          programsDecoder,
          signal === undefined ? undefined : { signal },
        ),
      );
    },

    apply(request, signal) {
      const body = WelfareApplicationRequestSchema.parse(request);
      return requestWelfareCommand(() =>
        deps.http.post(
          '/api/welfare/applications',
          body,
          welfareApplicationDecoder(body),
          signal === undefined ? undefined : { signal },
        ),
      );
    },
  };
}

function welfareApplicationDecoder(
  request: WelfareApplicationRequest,
): ResponseDecoder<WelfareApplicationResponse> {
  return asDecoder(
    WelfareApplicationResponseSchema.superRefine((response, context) => {
      const { result, snapshot } = response;
      if (
        result.programVersionId !== request.programVersionId ||
        result.applicationGameDay !== request.expectedGameDay ||
        result.approvalGameDay !== request.expectedGameDay
      ) {
        context.addIssue({
          code: 'custom',
          path: ['result'],
          message: 'welfare application result does not match the submitted program and day',
        });
      }
      if (
        snapshot.runRevision !== request.expectedRunRevision ||
        snapshot.gameDay < request.expectedGameDay ||
        BigInt(snapshot.stateRevision) < BigInt(request.expectedStateRevision) + 1n
      ) {
        context.addIssue({
          code: 'custom',
          path: ['snapshot'],
          message: 'welfare application response does not advance from the submitted cursor',
        });
      }
      if (
        !response.replayed &&
        (snapshot.gameDay !== request.expectedGameDay ||
          BigInt(snapshot.stateRevision) !== BigInt(request.expectedStateRevision) + 1n)
      ) {
        context.addIssue({
          code: 'custom',
          path: ['snapshot', 'stateRevision'],
          message: 'a new welfare application must advance state exactly once',
        });
      }
      const active = snapshot.life.activeWelfareApplications.find(
        (application) => application.applicationId === result.applicationId,
      );
      if (
        !response.replayed &&
        (active === undefined ||
          active.programVersionId !== result.programVersionId ||
          active.status !== result.status ||
          active.applicationGameDay !== result.applicationGameDay ||
          active.approvalGameDay !== result.approvalGameDay ||
          active.benefitKrw !== result.payment.amountKrw ||
          active.paidKrw !== 0 ||
          active.nextPayment?.id !== result.payment.id ||
          active.nextPayment.paymentNo !== result.payment.paymentNo ||
          active.nextPayment.amountKrw !== result.payment.amountKrw ||
          active.nextPayment.dueGameDay !== result.payment.dueGameDay ||
          active.nextPayment.status !== result.payment.status)
      ) {
        context.addIssue({
          code: 'custom',
          path: ['snapshot', 'life', 'activeWelfareApplications'],
          message: 'a new welfare result must appear in the committed active summary',
        });
      }
    }),
  );
}

async function requestWelfareQuery<T>(request: () => Promise<T>): Promise<T> {
  try {
    return await request();
  } catch (error) {
    const domain = toWelfareQueryError(error);
    if (domain !== undefined) throw domain;
    throw error;
  }
}

async function requestWelfareCommand<T>(request: () => Promise<T>): Promise<T> {
  try {
    return await request();
  } catch (error) {
    const domain = toWelfareCommandError(error);
    if (domain !== undefined) throw domain;
    throw error;
  }
}

function toWelfareQueryError(error: unknown): WelfareQueryError | undefined {
  if (!(error instanceof HttpError) || error.status >= 500) return undefined;
  const parsed = LifeFailureSchema.safeParse(error.body);
  if (!parsed.success) return undefined;
  return new WelfareQueryError(parsed.data.code, parsed.data.message);
}

function toWelfareCommandError(error: unknown): WelfareCommandError | undefined {
  if (!(error instanceof HttpError) || error.status >= 500) return undefined;
  const parsed = LifeFailureSchema.safeParse(error.body);
  if (!parsed.success) return undefined;
  return new WelfareCommandError(parsed.data.code, parsed.data.message);
}
