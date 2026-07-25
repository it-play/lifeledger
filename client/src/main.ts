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
import { createViewHost, type ViewFactory } from './lib/view/index.js';

/** 로그인하지 않아도 볼 수 있는 유일한 경로. */
const LOGIN_PATH = '/login';

/**
 * 부트스트랩. 의존성을 여기서 한 번 조립하고 아래로 주입한다.
 * 화면·라이브러리는 전역을 읽지 않는다 (테스트에서 갈아끼울 수 있어야 한다).
 */
function bootstrap(): void {
  const mountPoint = document.getElementById('app');
  if (mountPoint === null) throw new Error('#app 요소가 없습니다');

  const logger = createConsoleLogger({ minLevel: 'debug', scope: 'app' });
  const store = createStore<AppState>(initialState);

  const http = createHttpClient({ logger, credentials: 'same-origin' });
  const stream = createSseClient({ url: '/api/stream', logger, credentials: 'same-origin' });
  const auth = createAuthApi({ http });
  const api = createGameApi({
    http,
    stream,
    onInvalidTick: (error) => logger.log('error', '틱 payload 가 계약과 다릅니다', { error }),
  });

  // 스트림 상태와 틱을 스토어로만 흘린다 — 화면은 스토어만 본다
  stream.onStatusChange((status) => store.set(paths.connectionStatus, status));
  api.onTick((snapshot) => store.set(paths.gameSnapshot, snapshot));

  const viewHost = createViewHost(mountPoint);
  const router = createRouter<ViewFactory>({
    routes: [
      { pattern: '/', handler: createDashboardView({ store, api, auth }) },
      { pattern: '/new', handler: createCharacterCreateView({ store, api }) },
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

  // 서버가 로그인 실패를 쿼리로 알려준다 (§4.5). 주소창에 남겨두지 않는다
  takeLoginError(store);

  router.start();

  // 세션을 먼저 확인한다. 그 전에는 게임 데이터를 부르지 않는다 —
  // 미인증이면 전부 401 이라 콘솔만 시끄러워진다
  void resume(store, auth, api, router.navigate, logger);
}

/** `?login_error=` 를 스토어로 옮기고 주소창에서 지운다. */
function takeLoginError(store: Store<AppState>): void {
  const url = new URL(globalThis.location.href);
  const reason = url.searchParams.get('login_error');
  if (reason === null) return;

  store.set(paths.authError, reason);
  url.searchParams.delete('login_error');
  globalThis.history.replaceState(null, '', `${url.pathname}${url.search}${url.hash}`);
}

type GameApi = ReturnType<typeof createGameApi>;

/** 세션이 있으면 게임을 잇고, 없으면 로그인 화면으로 보낸다. */
async function resume(
  store: Store<AppState>,
  auth: AuthApi,
  api: GameApi,
  navigate: (to: string) => void,
  logger: Logger,
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
  }
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', bootstrap, { once: true });
} else {
  bootstrap();
}
