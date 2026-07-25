import { type HttpClient, HttpError } from '../lib/http/index.js';
import {
  type AuthProvider,
  AuthProviderListSchema,
  type Me,
  MeSchema,
  type ProviderKind,
} from './contracts.js';
import { asDecoder } from './zod-adapters.js';

/**
 * 로그인 API (§4.5). 토큰 교환은 전부 서버가 하므로 클라이언트가 다루는 것은
 * "누구로 로그인했는가" 와 "어디로 보내면 로그인이 시작되는가" 뿐이다.
 */
export interface AuthApi {
  /** 서버가 켜 둔 제공자. 자격증명이 없는 제공자는 목록에 없다. */
  listProviders(): Promise<readonly AuthProvider[]>;
  /** 로그인한 계정. 미인증이면 undefined — 401 은 오류가 아니라 정상 응답이다. */
  me(): Promise<Me | undefined>;
  logout(): Promise<void>;
  /**
   * 로그인을 시작할 주소. `location.assign` 으로 이동해야 한다 —
   * fetch 로 부르면 제공자의 로그인 페이지가 열리지 않는다.
   */
  loginUrl(provider: ProviderKind): string;
}

export interface AuthApiDeps {
  readonly http: HttpClient;
}

const providerListDecoder = asDecoder(AuthProviderListSchema);
const meDecoder = asDecoder(MeSchema);
/** 204 응답은 본문이 없다. 검증할 것이 없으므로 그대로 통과시킨다. */
const emptyDecoder = { parse: () => undefined };

export function createAuthApi(deps: AuthApiDeps): AuthApi {
  const { http } = deps;

  return {
    listProviders: () => http.get('/api/auth/providers', providerListDecoder),

    async me() {
      try {
        return await http.get('/api/auth/me', meDecoder);
      } catch (error) {
        // 로그인하지 않은 상태는 정상이다. 그 외 오류는 그대로 올린다
        if (error instanceof HttpError && error.status === 401) return undefined;
        throw error;
      }
    },

    async logout() {
      await http.post('/api/auth/logout', undefined, emptyDecoder);
    },

    loginUrl: (provider) => `/api/auth/${provider}/start`,
  };
}
