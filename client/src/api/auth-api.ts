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
 * The login API (§4.5). Token exchange happens entirely on the server, so all the client
 * handles is who is signed in and where to go to start signing in.
 */
export interface AuthApi {
  /** Providers the server enabled; one without credentials never appears. */
  listProviders(): Promise<readonly AuthProvider[]>;
  /** The signed-in account, or undefined. A 401 is a normal answer here, not an error. */
  me(): Promise<Me | undefined>;
  logout(): Promise<void>;
  /**
   * Where to start a login. Must be navigated to with `location.assign`; fetching it
   * would never open the provider's login page.
   */
  loginUrl(provider: ProviderKind): string;
}

export interface AuthApiDeps {
  readonly http: HttpClient;
}

const providerListDecoder = asDecoder(AuthProviderListSchema);
const meDecoder = asDecoder(MeSchema);
/** A 204 has no body, so there is nothing to validate. */
const emptyDecoder = { parse: () => undefined };

export function createAuthApi(deps: AuthApiDeps): AuthApi {
  const { http } = deps;

  return {
    listProviders: () => http.get('/api/auth/providers', providerListDecoder),

    async me() {
      try {
        return await http.get('/api/auth/me', meDecoder);
      } catch (error) {
        // Being signed out is normal; anything else propagates
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
