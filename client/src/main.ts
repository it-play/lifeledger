import { type AuthApi, createAuthApi } from './api/auth-api.js';
import { createGameApi } from './api/game-api.js';
import { createCharacterCreateView } from './app/screens/character-create.js';
import { createDashboardView } from './app/screens/dashboard.js';
import { createLoginView } from './app/screens/login.js';
import { createNotFoundView } from './app/screens/not-found.js';
import { type AppState, initialState, paths } from './app/state.js';
import { createConsoleLogger, type Logger } from './lib/core/index.js';
import { createHttpClient } from './lib/http/index.js';
import { createRouter } from './lib/router/index.js';
import { createSseClient } from './lib/sse/index.js';
import { createStore, type Store } from './lib/store/index.js';
import { createToastHost, createToastQueue, type ToastQueue } from './lib/toast/index.js';
import { createViewHost, type ViewFactory } from './lib/view/index.js';

/** The only route reachable without signing in. */
const LOGIN_PATH = '/login';

/**
 * Bootstrap. Dependencies are assembled once here and injected downward, so no screen or
 * library reads a global and everything stays substitutable in tests.
 */
function bootstrap(): void {
  const mountPoint = document.getElementById('app');
  if (mountPoint === null) throw new Error('#app 요소가 없습니다');

  const logger = createConsoleLogger({ minLevel: 'debug', scope: 'app' });
  const store = createStore<AppState>(initialState);

  // Lives outside #app so a screen swap cannot take a message off the screen with it
  const toasts = createToastQueue();
  document.body.appendChild(createToastHost(toasts).element);

  const http = createHttpClient({ logger, credentials: 'same-origin' });
  const stream = createSseClient({ url: '/api/stream', logger, credentials: 'same-origin' });
  const auth = createAuthApi({ http });
  const api = createGameApi({
    http,
    stream,
    onInvalidTick: (error) => logger.log('error', '틱 payload 가 계약과 다릅니다', { error }),
  });

  // Stream status and ticks reach screens only through the store
  stream.onStatusChange((status) => {
    store.set(paths.connectionStatus, status);
    // Reconnects are routine and stay silent; only giving up is worth interrupting for
    if (status === 'closed') {
      toasts.show('서버와 연결이 끊겼습니다. 새로고침해 주세요.', { tone: 'error', durationMs: 0 });
    }
  });
  api.onTick((snapshot) => store.set(paths.gameSnapshot, snapshot));

  const viewHost = createViewHost(mountPoint);
  const router = createRouter<ViewFactory>({
    routes: [
      { pattern: '/', handler: createDashboardView({ store, api, auth, toasts }) },
      { pattern: '/new', handler: createCharacterCreateView({ store, api, toasts }) },
      { pattern: LOGIN_PATH, handler: createLoginView({ store, auth }) },
    ],
    fallback: createNotFoundView(),
    onNavigate: (factory, match) =>
      viewHost.render(factory, {
        params: match.params,
        query: match.query,
        navigate: (to) => router.navigate(to),
      }),
  });

  // The server reports login failure by query parameter (§4.5); do not leave it in the URL
  takeLoginError(store);

  router.start();

  // Check the session first: fetching game data while signed out only yields 401s
  void resume(store, auth, api, router.navigate, logger, toasts);
}

/** Moves `?login_error=` into the store and strips it from the URL. */
function takeLoginError(store: Store<AppState>): void {
  const url = new URL(globalThis.location.href);
  const reason = url.searchParams.get('login_error');
  if (reason === null) return;

  store.set(paths.authError, reason);
  url.searchParams.delete('login_error');
  globalThis.history.replaceState(null, '', `${url.pathname}${url.search}${url.hash}`);
}

type GameApi = ReturnType<typeof createGameApi>;

/** Resumes the game when a session exists, otherwise routes to login. */
async function resume(
  store: Store<AppState>,
  auth: AuthApi,
  api: GameApi,
  navigate: (to: string) => void,
  logger: Logger,
  toasts: ToastQueue,
): Promise<void> {
  let user: Awaited<ReturnType<AuthApi['me']>>;
  try {
    user = await auth.me();
  } catch (error) {
    logger.log('error', '세션을 확인하지 못했습니다', { error });
    store.set(paths.authStatus, 'anonymous');
    navigate(LOGIN_PATH);
    return;
  }

  if (user === undefined) {
    store.set(paths.authStatus, 'anonymous');
    navigate(LOGIN_PATH);
    return;
  }

  store.set(paths.authUser, user);
  store.set(paths.authStatus, 'authenticated');

  api.connectStream();
  try {
    store.set(paths.gameSnapshot, await api.getSnapshot());
  } catch (error) {
    logger.log('warn', '초기 상태를 불러오지 못했습니다', { error });
    toasts.show('게임 상태를 불러오지 못했습니다.', { tone: 'error' });
  }
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', bootstrap, { once: true });
} else {
  bootstrap();
}
