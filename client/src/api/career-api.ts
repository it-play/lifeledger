import { type HttpClient, HttpError, type ResponseDecoder } from '../lib/http/index.js';
import {
  type CareerActivitiesResponse,
  CareerActivitiesResponseSchema,
  type CareerActivityResponse,
  CareerActivityResponseSchema,
  type CareerActivityStartRequest,
  CareerActivityStartRequestSchema,
  type CareerApplicationRequest,
  CareerApplicationRequestSchema,
  type CareerApplicationResponse,
  CareerApplicationResponseSchema,
  type CareerApplicationsResponse,
  CareerApplicationsResponseSchema,
  type CareerArtifactKind,
  CareerArtifactKindSchema,
  type CareerArtifactPublishRequest,
  CareerArtifactPublishRequestSchema,
  type CareerArtifactResponse,
  CareerArtifactResponseSchema,
  type CareerArtifactsResponse,
  CareerArtifactsResponseSchema,
  type CareerCursorRequest,
  CareerCursorRequestSchema,
  type CareerEmploymentResponse,
  CareerEmploymentResponseSchema,
  type CareerFailureCode,
  CareerFailureSchema,
  type CareerFocusRequest,
  CareerFocusRequestSchema,
  type CareerFocusResponse,
  CareerFocusResponseSchema,
  type CareerIndustry,
  CareerIndustrySchema,
  type CareerInterviewConfirmationRequest,
  CareerInterviewConfirmationRequestSchema,
  type CareerInvitationResponse,
  CareerInvitationResponseSchema,
  type CareerJobsResponse,
  CareerJobsResponseSchema,
  type CareerOfferResponse,
  CareerOfferResponseSchema,
  type CareerPlatform,
  CareerPlatformSchema,
  type CareerSpecsResponse,
  CareerSpecsResponseSchema,
  PostingKeySchema,
  ResourceIdSchema,
} from './contracts.js';
import { asDecoder } from './zod-adapters.js';

export interface CareerPageQuery {
  readonly before?: string;
  readonly limit?: number;
}

export interface CareerArtifactPageQuery extends CareerPageQuery {
  readonly kind?: CareerArtifactKind;
}

export interface CareerJobsPageQuery extends CareerPageQuery {
  readonly platform?: CareerPlatform;
  readonly industry?: CareerIndustry;
}

export interface CareerApi {
  getSpecs(query?: CareerPageQuery, signal?: AbortSignal): Promise<CareerSpecsResponse>;
  getActivities(query?: CareerPageQuery, signal?: AbortSignal): Promise<CareerActivitiesResponse>;
  getArtifacts(
    query?: CareerArtifactPageQuery,
    signal?: AbortSignal,
  ): Promise<CareerArtifactsResponse>;
  getJobs(query?: CareerJobsPageQuery, signal?: AbortSignal): Promise<CareerJobsResponse>;
  getApplications(
    query?: CareerPageQuery,
    signal?: AbortSignal,
  ): Promise<CareerApplicationsResponse>;
  getEmployment(signal?: AbortSignal): Promise<CareerEmploymentResponse>;
  focus(request: CareerFocusRequest): Promise<CareerFocusResponse>;
  startActivity(request: CareerActivityStartRequest): Promise<CareerActivityResponse>;
  cancelActivity(activityId: string, request: CareerCursorRequest): Promise<CareerActivityResponse>;
  publishArtifact(request: CareerArtifactPublishRequest): Promise<CareerArtifactResponse>;
  apply(request: CareerApplicationRequest): Promise<CareerApplicationResponse>;
  confirmInterview(
    applicationId: string,
    request: CareerInterviewConfirmationRequest,
  ): Promise<CareerApplicationResponse>;
  withdrawApplication(
    applicationId: string,
    request: CareerCursorRequest,
  ): Promise<CareerApplicationResponse>;
  acceptInvitation(
    invitationId: string,
    request: CareerCursorRequest,
  ): Promise<CareerInvitationResponse>;
  declineInvitation(
    invitationId: string,
    request: CareerCursorRequest,
  ): Promise<CareerInvitationResponse>;
  acceptOffer(offerId: string, request: CareerCursorRequest): Promise<CareerOfferResponse>;
  declineOffer(offerId: string, request: CareerCursorRequest): Promise<CareerOfferResponse>;
}

export interface CareerApiDeps {
  readonly http: HttpClient;
}

/** A validated career rejection, independent of the HTTP status used by the server. */
export class CareerCommandError extends Error {
  constructor(
    readonly code: CareerFailureCode,
    message: string,
  ) {
    super(message);
    this.name = 'CareerCommandError';
  }
}

export function createCareerApi(deps: CareerApiDeps): CareerApi {
  const specsDecoder = asDecoder(CareerSpecsResponseSchema);
  const activitiesDecoder = asDecoder(CareerActivitiesResponseSchema);
  const artifactsDecoder = asDecoder(CareerArtifactsResponseSchema);
  const jobsDecoder = asDecoder(CareerJobsResponseSchema);
  const applicationsDecoder = asDecoder(CareerApplicationsResponseSchema);
  const employmentDecoder = asDecoder(CareerEmploymentResponseSchema);

  return {
    getSpecs(query, signal) {
      return deps.http.get(
        careerPagePath('/api/career/specs', query),
        specsDecoder,
        signal === undefined ? undefined : { signal },
      );
    },

    getActivities(query, signal) {
      return deps.http.get(
        careerPagePath('/api/career/activities', query),
        activitiesDecoder,
        signal === undefined ? undefined : { signal },
      );
    },

    getArtifacts(query, signal) {
      const params = careerPageParams(query);
      if (query?.kind !== undefined) params.set('kind', CareerArtifactKindSchema.parse(query.kind));
      const suffix = params.size === 0 ? '' : `?${params.toString()}`;
      return deps.http.get(
        `/api/career/artifacts${suffix}`,
        artifactsDecoder,
        signal === undefined ? undefined : { signal },
      );
    },

    getJobs(query, signal) {
      const params = careerJobsPageParams(query);
      const suffix = params.size === 0 ? '' : `?${params.toString()}`;
      return deps.http.get(
        `/api/career/jobs${suffix}`,
        jobsDecoder,
        signal === undefined ? undefined : { signal },
      );
    },

    getApplications(query, signal) {
      return deps.http.get(
        careerPagePath('/api/career/applications', query),
        applicationsDecoder,
        signal === undefined ? undefined : { signal },
      );
    },

    getEmployment(signal) {
      return deps.http.get(
        '/api/career/employment',
        employmentDecoder,
        signal === undefined ? undefined : { signal },
      );
    },

    async focus(request) {
      const body = CareerFocusRequestSchema.parse(request);
      const decoder = asDecoder(
        CareerFocusResponseSchema.superRefine((response, context) => {
          if (response.result.focusedJobFamilyKey !== body.focusedJobFamilyKey) {
            context.addIssue({
              code: 'custom',
              path: ['result', 'focusedJobFamilyKey'],
              message: 'focus result does not match the submitted command',
            });
          }
        }),
      );
      return postCareer(deps.http, '/api/career/focus', body, decoder);
    },

    async startActivity(request) {
      const body = CareerActivityStartRequestSchema.parse(request);
      const decoder = asDecoder(
        CareerActivityResponseSchema.superRefine((response, context) => {
          if (response.result.status !== 'active') {
            context.addIssue({
              code: 'custom',
              path: ['result', 'status'],
              message: 'started activity must be active',
            });
          }
        }),
      );
      return postCareer(deps.http, '/api/career/activities', body, decoder);
    },

    async cancelActivity(activityId, request) {
      const id = ResourceIdSchema.parse(activityId);
      const body = CareerCursorRequestSchema.parse(request);
      const decoder = asDecoder(
        CareerActivityResponseSchema.superRefine((response, context) => {
          if (response.result.activityId !== id || response.result.status !== 'cancelled') {
            context.addIssue({
              code: 'custom',
              path: ['result'],
              message: 'cancel result does not match the path activity',
            });
          }
        }),
      );
      return postCareer(deps.http, `/api/career/activities/${id}/cancel`, body, decoder);
    },

    async publishArtifact(request) {
      const body = CareerArtifactPublishRequestSchema.parse(request);
      const decoder = asDecoder(
        CareerArtifactResponseSchema.superRefine((response, context) => {
          if (response.result.kind !== body.kind) {
            context.addIssue({
              code: 'custom',
              path: ['result', 'kind'],
              message: 'artifact result does not match the submitted kind',
            });
          }
        }),
      );
      return postCareer(deps.http, '/api/career/artifacts', body, decoder);
    },

    async apply(request) {
      const body = CareerApplicationRequestSchema.parse(request);
      const decoder = asDecoder(
        CareerApplicationResponseSchema.superRefine((response, context) => {
          if (response.result.status !== 'submitted') {
            context.addIssue({
              code: 'custom',
              path: ['result', 'status'],
              message: 'new application must be submitted',
            });
          }
        }),
      );
      return postCareer(deps.http, '/api/career/applications', body, decoder);
    },

    async confirmInterview(applicationId, request) {
      const id = ResourceIdSchema.parse(applicationId);
      const body = CareerInterviewConfirmationRequestSchema.parse(request);
      return postCareer(
        deps.http,
        `/api/career/applications/${id}/interview-confirmation`,
        body,
        careerApplicationActionDecoder(id),
      );
    },

    async withdrawApplication(applicationId, request) {
      const id = ResourceIdSchema.parse(applicationId);
      const body = CareerCursorRequestSchema.parse(request);
      return postCareer(
        deps.http,
        `/api/career/applications/${id}/withdraw`,
        body,
        careerApplicationActionDecoder(id),
      );
    },

    async acceptInvitation(invitationId, request) {
      const id = ResourceIdSchema.parse(invitationId);
      const body = CareerCursorRequestSchema.parse(request);
      return postCareer(
        deps.http,
        `/api/career/invitations/${id}/accept`,
        body,
        careerInvitationActionDecoder(id),
      );
    },

    async declineInvitation(invitationId, request) {
      const id = ResourceIdSchema.parse(invitationId);
      const body = CareerCursorRequestSchema.parse(request);
      return postCareer(
        deps.http,
        `/api/career/invitations/${id}/decline`,
        body,
        careerInvitationActionDecoder(id),
      );
    },

    async acceptOffer(offerId, request) {
      const id = ResourceIdSchema.parse(offerId);
      const body = CareerCursorRequestSchema.parse(request);
      return postCareer(
        deps.http,
        `/api/career/offers/${id}/accept`,
        body,
        careerOfferActionDecoder(id),
      );
    },

    async declineOffer(offerId, request) {
      const id = ResourceIdSchema.parse(offerId);
      const body = CareerCursorRequestSchema.parse(request);
      return postCareer(
        deps.http,
        `/api/career/offers/${id}/decline`,
        body,
        careerOfferActionDecoder(id),
      );
    },
  };
}

function careerPagePath(path: string, query: CareerPageQuery | undefined): string {
  const params = careerPageParams(query);
  return params.size === 0 ? path : `${path}?${params.toString()}`;
}

function careerPageParams(query: CareerPageQuery | undefined): URLSearchParams {
  const params = new URLSearchParams();
  if (query?.before !== undefined) params.set('before', ResourceIdSchema.parse(query.before));
  if (query?.limit !== undefined) {
    if (!Number.isInteger(query.limit) || query.limit < 1 || query.limit > 200) {
      throw new RangeError('career page limit must be between 1 and 200');
    }
    params.set('limit', String(query.limit));
  }
  return params;
}

function careerJobsPageParams(query: CareerJobsPageQuery | undefined): URLSearchParams {
  const params = new URLSearchParams();
  if (query?.before !== undefined) params.set('before', PostingKeySchema.parse(query.before));
  if (query?.limit !== undefined) {
    if (!Number.isInteger(query.limit) || query.limit < 1 || query.limit > 200) {
      throw new RangeError('career page limit must be between 1 and 200');
    }
    params.set('limit', String(query.limit));
  }
  if (query?.platform !== undefined)
    params.set('platform', CareerPlatformSchema.parse(query.platform));
  if (query?.industry !== undefined)
    params.set('industry', CareerIndustrySchema.parse(query.industry));
  return params;
}

function careerApplicationActionDecoder(
  applicationId: string,
): ResponseDecoder<CareerApplicationResponse> {
  return asDecoder(
    CareerApplicationResponseSchema.superRefine((response, context) => {
      if (response.result.applicationId !== applicationId) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'applicationId'],
          message: 'application result does not match its path',
        });
      }
    }),
  );
}

function careerInvitationActionDecoder(
  invitationId: string,
): ResponseDecoder<CareerInvitationResponse> {
  return asDecoder(
    CareerInvitationResponseSchema.superRefine((response, context) => {
      if (response.result.invitationId !== invitationId) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'invitationId'],
          message: 'invitation result does not match its path',
        });
      }
    }),
  );
}

function careerOfferActionDecoder(offerId: string): ResponseDecoder<CareerOfferResponse> {
  return asDecoder(
    CareerOfferResponseSchema.superRefine((response, context) => {
      if (response.result.offerId !== offerId) {
        context.addIssue({
          code: 'custom',
          path: ['result', 'offerId'],
          message: 'offer result does not match its path',
        });
      }
    }),
  );
}

async function postCareer<TRequest, TResponse>(
  http: HttpClient,
  path: string,
  body: TRequest,
  decoder: ResponseDecoder<TResponse>,
): Promise<TResponse> {
  try {
    return await http.post(path, body, decoder);
  } catch (error) {
    const domain = toCareerCommandError(error);
    if (domain !== undefined) throw domain;
    throw error;
  }
}

function toCareerCommandError(error: unknown): CareerCommandError | undefined {
  if (!(error instanceof HttpError)) return undefined;
  const parsed = CareerFailureSchema.safeParse(error.body);
  if (!parsed.success) return undefined;
  return new CareerCommandError(parsed.data.code, parsed.data.message);
}
