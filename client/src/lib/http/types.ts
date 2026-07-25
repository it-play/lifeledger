import type { Logger } from '../core/types.js';

/** 응답을 런타임에 검증하는 최소 계약. zod 스키마가 이 형태를 만족한다. */
export interface ResponseDecoder<T> {
  /** 검증 실패 시 throw 한다. */
  parse(input: unknown): T;
}

export interface RequestOptions {
  readonly signal?: AbortSignal;
  readonly headers?: Readonly<Record<string, string>>;
}

/**
 * HTTP 경계. 화면은 이 인터페이스를 직접 쓰지 않고 도메인 API 를 통해 접근한다.
 * 여기서 응답 검증을 강제해, 서버 계약이 바뀌면 화면이 아니라 경계에서 터지게 한다.
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

/** 서버가 4xx/5xx 를 준 경우. */
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

/** 응답이 계약과 다른 경우. 서버·클라이언트 버전 불일치를 조용히 넘기지 않는다. */
export class ResponseShapeError extends Error {
  constructor(
    readonly path: string,
    override readonly cause: unknown,
  ) {
    super(`응답 형식이 계약과 다릅니다: ${path}`);
    this.name = 'ResponseShapeError';
  }
}
