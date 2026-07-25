import type { Logger } from '../core/types.js';

/** The minimal runtime validation contract; a zod schema satisfies it. */
export interface ResponseDecoder<T> {
  /** Throws when validation fails. */
  parse(input: unknown): T;
}

export interface RequestOptions {
  readonly signal?: AbortSignal;
  readonly headers?: Readonly<Record<string, string>>;
}

/**
 * The HTTP boundary. Screens reach it through the domain API rather than directly.
 * Validation is mandatory here, so a changed server contract fails at the boundary
 * instead of inside a screen.
 */
export interface HttpClient {
  get<T>(path: string, decoder: ResponseDecoder<T>, options?: RequestOptions): Promise<T>;
  post<T>(
    path: string,
    body: unknown,
    decoder: ResponseDecoder<T>,
    options?: RequestOptions,
  ): Promise<T>;
}

export interface HttpClientOptions {
  readonly baseUrl?: string;
  readonly defaultHeaders?: Readonly<Record<string, string>>;
  readonly credentials?: RequestCredentials;
  readonly logger?: Logger;
  readonly fetchImpl?: typeof fetch;
}

/** The server answered 4xx or 5xx. */
export class HttpError extends Error {
  constructor(
    readonly status: number,
    readonly path: string,
    readonly body: unknown,
  ) {
    super(`HTTP ${status} ${path}`);
    this.name = 'HttpError';
  }
}

/** The response did not match the contract. A version mismatch must not pass silently. */
export class ResponseShapeError extends Error {
  constructor(
    readonly path: string,
    override readonly cause: unknown,
  ) {
    super(`response does not match the contract: ${path}`);
    this.name = 'ResponseShapeError';
  }
}
