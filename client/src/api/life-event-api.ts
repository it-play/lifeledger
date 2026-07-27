import { type HttpClient, HttpError, type ResponseDecoder } from '../lib/http/index.js';
import {
  type LifeEventChoiceRequest,
  LifeEventChoiceRequestSchema,
  type LifeEventChoiceResponse,
  LifeEventChoiceResponseSchema,
  type LifeEventsQuery,
  LifeEventsQuerySchema,
  type LifeEventsResponse,
  LifeEventsResponseSchema,
  type LifeFailureCode,
  LifeFailureSchema,
  ResourceIdSchema,
} from './contracts.js';
import { asDecoder } from './zod-adapters.js';

export interface LifeEventApi {
  list(query?: LifeEventsQuery, signal?: AbortSignal): Promise<LifeEventsResponse>;
  choose(
    eventId: string,
    request: LifeEventChoiceRequest,
    signal?: AbortSignal,
  ): Promise<LifeEventChoiceResponse>;
}

export interface LifeEventApiDeps {
  readonly http: HttpClient;
}

/** A validated life-event query rejection, independent of its transport status. */
export class LifeEventQueryError extends Error {
  constructor(
    readonly code: LifeFailureCode,
    message: string,
  ) {
    super(message);
    this.name = 'LifeEventQueryError';
  }
}

/** A validated life-event command rejection whose outcome is known. */
export class LifeEventCommandError extends Error {
  constructor(
    readonly code: LifeFailureCode,
    message: string,
  ) {
    super(message);
    this.name = 'LifeEventCommandError';
  }
}

export function createLifeEventApi(deps: LifeEventApiDeps): LifeEventApi {
  const listDecoder = asDecoder(LifeEventsResponseSchema);
  return {
    list(query, signal) {
      const parsed = LifeEventsQuerySchema.parse(query ?? {});
      const params = new URLSearchParams();
      if (parsed.cursor !== undefined) params.set('cursor', parsed.cursor);
      const suffix = params.size === 0 ? '' : `?${params.toString()}`;
      return requestLifeEventQuery(() =>
        deps.http.get(
          `/api/life/events${suffix}`,
          listDecoder,
          signal === undefined ? undefined : { signal },
        ),
      );
    },

    choose(eventId, request, signal) {
      const pathEventId = ResourceIdSchema.parse(eventId);
      const body = LifeEventChoiceRequestSchema.parse(request);
      return requestLifeEventCommand(() =>
        deps.http.post(
          `/api/life/events/${pathEventId}/choices`,
          body,
          lifeEventChoiceDecoder(pathEventId, body),
          signal === undefined ? undefined : { signal },
        ),
      );
    },
  };
}

function lifeEventChoiceDecoder(
  eventId: string,
  request: LifeEventChoiceRequest,
): ResponseDecoder<LifeEventChoiceResponse> {
  return asDecoder(
    LifeEventChoiceResponseSchema.superRefine((response, context) => {
      const { result, snapshot } = response;
      if (
        result.eventId !== eventId ||
        result.choiceId !== request.choiceId ||
        result.resolvedGameDay !== request.expectedGameDay
      ) {
        context.addIssue({
          code: 'custom',
          path: ['result'],
          message: 'life event result does not match the submitted event, choice, and day',
        });
      }
      if (
        snapshot.runRevision < request.expectedRunRevision ||
        (snapshot.runRevision === request.expectedRunRevision &&
          (snapshot.gameDay < request.expectedGameDay ||
            BigInt(snapshot.stateRevision) < BigInt(request.expectedStateRevision) + 1n))
      ) {
        context.addIssue({
          code: 'custom',
          path: ['snapshot'],
          message: 'life event response does not advance from the submitted cursor',
        });
      }
      if (
        !response.replayed &&
        (snapshot.runRevision !== request.expectedRunRevision ||
          snapshot.gameDay !== request.expectedGameDay ||
          BigInt(snapshot.stateRevision) !== BigInt(request.expectedStateRevision) + 1n)
      ) {
        context.addIssue({
          code: 'custom',
          path: ['snapshot', 'stateRevision'],
          message: 'a new life event choice must advance state exactly once',
        });
      }
      if (snapshot.life.pendingEvents.some((event) => event.id === eventId)) {
        context.addIssue({
          code: 'custom',
          path: ['snapshot', 'life', 'pendingEvents'],
          message: 'a resolved life event cannot remain pending in the committed snapshot',
        });
      }
    }),
  );
}

async function requestLifeEventQuery<T>(request: () => Promise<T>): Promise<T> {
  try {
    return await request();
  } catch (error) {
    const domain = toLifeEventQueryError(error);
    if (domain !== undefined) throw domain;
    throw error;
  }
}

async function requestLifeEventCommand<T>(request: () => Promise<T>): Promise<T> {
  try {
    return await request();
  } catch (error) {
    const domain = toLifeEventCommandError(error);
    if (domain !== undefined) throw domain;
    throw error;
  }
}

function toLifeEventQueryError(error: unknown): LifeEventQueryError | undefined {
  if (!(error instanceof HttpError) || error.status >= 500) return undefined;
  const parsed = LifeFailureSchema.safeParse(error.body);
  if (!parsed.success) return undefined;
  return new LifeEventQueryError(parsed.data.code, parsed.data.message);
}

function toLifeEventCommandError(error: unknown): LifeEventCommandError | undefined {
  if (!(error instanceof HttpError) || error.status >= 500) return undefined;
  const parsed = LifeFailureSchema.safeParse(error.body);
  if (!parsed.success) return undefined;
  return new LifeEventCommandError(parsed.data.code, parsed.data.message);
}
