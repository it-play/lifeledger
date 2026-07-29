import { type DeleteHttpClient, HttpError } from '../lib/http/index.js';
import {
  type PlaytestConsentRequest,
  PlaytestConsentRequestSchema,
  type PlaytestConsentUpdate,
  PlaytestConsentUpdateSchema,
  type PlaytestFailureCode,
  PlaytestFailureSchema,
  type PlaytestFeedback,
  type PlaytestFeedbackDeletion,
  PlaytestFeedbackDeletionSchema,
  PlaytestFeedbackIdSchema,
  type PlaytestFeedbackOverview,
  PlaytestFeedbackOverviewSchema,
  type PlaytestFeedbackRequest,
  PlaytestFeedbackRequestSchema,
  PlaytestFeedbackSchema,
} from './contracts.js';
import { asDecoder } from './zod-adapters.js';

export interface PlaytestApi {
  overview(signal?: AbortSignal): Promise<PlaytestFeedbackOverview>;
  setConsent(request: PlaytestConsentRequest, signal?: AbortSignal): Promise<PlaytestConsentUpdate>;
  submitFeedback(request: PlaytestFeedbackRequest, signal?: AbortSignal): Promise<PlaytestFeedback>;
  deleteFeedback(feedbackId: string, signal?: AbortSignal): Promise<PlaytestFeedbackDeletion>;
}

export interface PlaytestApiDeps {
  readonly http: DeleteHttpClient;
}

export class PlaytestRequestError extends Error {
  constructor(readonly code: PlaytestFailureCode) {
    super(PLAYTEST_FAILURE_MESSAGE[code]);
    this.name = 'PlaytestRequestError';
  }
}

const PLAYTEST_FAILURE_MESSAGE: Readonly<Record<PlaytestFailureCode, string>> = {
  invalidCommand: '피드백 요청 형식이 올바르지 않습니다.',
  policyUnavailable: '현재 피드백 고지를 사용할 수 없습니다.',
  revisionConflict: '동의 상태가 바뀌었습니다. 새로 조회한 뒤 다시 시도해 주세요.',
  consentRequired: '현재 고지에 먼저 동의해 주세요.',
  privacyConfirmationRequired: '개인정보 제외 안내를 확인해 주세요.',
  feedbackCapacityReached: '활성 피드백 20건 한도에 도달했습니다. 기존 항목을 삭제해 주세요.',
  runReferenceNotFound: '이 계정이 소유한 실행을 찾지 못했습니다.',
  feedbackNotFound: '삭제할 수 있는 본인 피드백을 찾지 못했습니다.',
};

export function createPlaytestApi(deps: PlaytestApiDeps): PlaytestApi {
  return {
    overview(signal) {
      return request(() =>
        deps.http.get(
          '/api/playtest/feedback',
          asDecoder(PlaytestFeedbackOverviewSchema),
          requestOptions(signal),
        ),
      );
    },

    setConsent(requestValue, signal) {
      const body = PlaytestConsentRequestSchema.parse(requestValue);
      return request(() =>
        deps.http.put(
          '/api/playtest/consent',
          body,
          asDecoder(
            PlaytestConsentUpdateSchema.superRefine((response, context) => {
              const expectedStatus = body.action === 'grant' ? 'granted' : 'withdrawn';
              const revisionDelta = response.consent.revision - body.expectedRevision;
              if (
                response.consent.status !== expectedStatus ||
                ![0, 1].includes(revisionDelta) ||
                (body.action === 'grant' &&
                  response.consent.policyVersionId !== body.policyVersionId) ||
                (body.action === 'grant' && response.purgedFeedbackCount !== 0)
              ) {
                context.addIssue({
                  code: 'custom',
                  message: 'playtest consent response does not match the request',
                });
              }
            }),
          ),
          requestOptions(signal),
        ),
      );
    },

    submitFeedback(requestValue, signal) {
      const body = PlaytestFeedbackRequestSchema.parse(requestValue);
      return request(() =>
        deps.http.post(
          '/api/playtest/feedback',
          body,
          asDecoder(
            PlaytestFeedbackSchema.superRefine((response, context) => {
              if (
                response.category !== body.category ||
                response.severity !== body.severity ||
                response.message !== body.message ||
                response.runRevision !== (body.runRevision ?? null)
              ) {
                context.addIssue({
                  code: 'custom',
                  message: 'playtest feedback response does not match the submission',
                });
              }
            }),
          ),
          requestOptions(signal),
        ),
      );
    },

    deleteFeedback(feedbackId, signal) {
      const id = parseFeedbackId(feedbackId);
      return request(() =>
        deps.http.delete(
          `/api/playtest/feedback/${id}`,
          asDecoder(
            PlaytestFeedbackDeletionSchema.refine((response) => response.id === id, {
              path: ['id'],
              message: 'playtest feedback deletion does not match the requested feedback',
            }),
          ),
          requestOptions(signal),
        ),
      );
    },
  };
}

function parseFeedbackId(value: string): string {
  return PlaytestFeedbackIdSchema.parse(value);
}

function requestOptions(signal: AbortSignal | undefined): { signal: AbortSignal } | undefined {
  return signal === undefined ? undefined : { signal };
}

async function request<T>(operation: () => Promise<T>): Promise<T> {
  try {
    return await operation();
  } catch (error) {
    if (error instanceof HttpError && error.status < 500) {
      const failure = PlaytestFailureSchema.safeParse(error.body);
      if (failure.success) throw new PlaytestRequestError(failure.data.code);
    }
    throw error;
  }
}
