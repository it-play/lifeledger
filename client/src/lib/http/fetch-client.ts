import { createNullLogger } from '../core/logger.js';
import type { HttpClient, HttpClientOptions, RequestOptions, ResponseDecoder } from './types.js';
import { HttpError, ResponseShapeError } from './types.js';

export function createHttpClient(options: HttpClientOptions = {}): HttpClient {
  const baseUrl = options.baseUrl ?? '';
  const logger = (options.logger ?? createNullLogger()).child('http');
  const fetchImpl = options.fetchImpl ?? globalThis.fetch.bind(globalThis);

  async function send<T>(
    method: 'GET' | 'POST',
    path: string,
    decoder: ResponseDecoder<T>,
    body: unknown,
    requestOptions: RequestOptions | undefined,
  ): Promise<T> {
    const url = `${baseUrl}${path}`;
    const headers: Record<string, string> = {
      Accept: 'application/json',
      ...(options.defaultHeaders ?? {}),
      ...(requestOptions?.headers ?? {}),
    };
    if (body !== undefined) headers['Content-Type'] = 'application/json';

    const response = await fetchImpl(url, {
      method,
      headers,
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      ...(requestOptions?.signal === undefined ? {} : { signal: requestOptions.signal }),
      ...(options.credentials === undefined ? {} : { credentials: options.credentials }),
    });

    const raw = await readBody(response);

    if (!response.ok) {
      logger.log('warn', '요청 실패', { path, status: response.status });
      throw new HttpError(response.status, path, raw);
    }

    try {
      return decoder.parse(raw);
    } catch (cause) {
      logger.log('error', '응답 형식 불일치', { path });
      throw new ResponseShapeError(path, cause);
    }
  }

  return {
    get: (path, decoder, requestOptions) => send('GET', path, decoder, undefined, requestOptions),
    post: (path, body, decoder, requestOptions) =>
      send('POST', path, decoder, body, requestOptions),
  };
}

async function readBody(response: Response): Promise<unknown> {
  const text = await response.text();
  if (text === '') return undefined;
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return text;
  }
}
