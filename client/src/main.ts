import { type AuthApi, createAuthApi } from './api/auth-api.js';
import { createCareerApi } from './api/career-api.js';
import { createCorporationApi } from './api/corporation-api.js';
import { createGameApi } from './api/game-api.js';
import { createHousingApi } from './api/housing-api.js';
import { createInsolvencyApi } from './api/insolvency-api.js';
import { createInsuranceApi } from './api/insurance-api.js';
import { createLifeApi } from './api/life-api.js';
import { createLifeEventApi } from './api/life-event-api.js';
import { createLoanApi } from './api/loan-api.js';
import { createPlaytestApi } from './api/playtest-api.js';
import { createWelfareApi } from './api/welfare-api.js';
import { createGameStateWriter, type GameStateWriter } from './app/game-state/index.js';
import { createCareerView } from './app/screens/career.js';
import { createCharacterCreateView } from './app/screens/character-create.js';
import { createCorporationView } from './app/screens/corporation.js';
import { createDashboardView } from './app/screens/dashboard.js';
import { createEventsInsuranceView } from './app/screens/events-insurance.js';
import { createHousingView } from './app/screens/housing.js';
import { createLifeView } from './app/screens/life.js';
import { createLoansView } from './app/screens/loans.js';
import { createLoginView } from './app/screens/login.js';
import { createNotFoundView } from './app/screens/not-found.js';
import { createPlaytestFeedbackView } from './app/screens/playtest-feedback.js';
import { createRecoveryView } from './app/screens/recovery.js';
import { createWelfareView } from './app/screens/welfare.js';
import { type AppState, initialState, paths } from './app/state.js';
import { createConsoleLogger, createDisposableBag, type Logger } from './lib/core/index.js';
import { createHooks } from './lib/hooks/index.js';
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
  const snapshots = createGameStateWriter({ store });
  const appBag = createDisposableBag();

  // Lives outside #app so a screen swap cannot take a message off the screen with it
  const toasts = createToastQueue();
  document.body.appendChild(createToastHost(toasts).element);

  const http = createHttpClient({ logger, credentials: 'same-origin' });
  const stream = createSseClient({ url: '/api/stream', logger, credentials: 'same-origin' });
  const auth = createAuthApi({ http });
  const careerApi = createCareerApi({ http });
  const corporationApi = createCorporationApi({ http });
  const housingApi = createHousingApi({ http });
  const insolvencyApi = createInsolvencyApi({ http });
  const insuranceApi = createInsuranceApi({ http });
  const lifeEventApi = createLifeEventApi({ http });
  const lifeApi = createLifeApi({ http });
  const loanApi = createLoanApi({ http });
  const playtestApi = createPlaytestApi({ http });
  const welfareApi = createWelfareApi({ http });
  const api = createGameApi({
    http,
    stream,
    onInvalidTick: (error) => logger.log('error', '틱 payload 가 계약과 다릅니다', { error }),
  });
  appBag.add(stream);

  // Stream status and ticks reach screens only through the store
  appBag.add(
    stream.onStatusChange((status) => {
      store.set(paths.connectionStatus, status);
      // Reconnects are routine and stay silent; only giving up is worth interrupting for
      if (status === 'closed' && document.visibilityState === 'visible') {
        toasts.show('서버와 연결이 끊겼습니다. 새로고침해 주세요.', {
          tone: 'error',
          durationMs: 0,
        });
      }
    }),
  );
  appBag.add(api.onTick((snapshot) => snapshots.apply(snapshot)));

  // Visibility belongs to the authenticated app, not one screen: automatic progress must
  // stop even when the player hides a character or error route.
  const appHooks = createHooks(appBag);
  const visible = appHooks.useVisibility();
  let gameStreamEnabled = false;
  const syncGameStream = (): void => {
    if (!gameStreamEnabled || store.getState().auth.status !== 'authenticated') return;
    if (visible.peek()) api.connectStream();
    else api.disconnectStream();
  };
  appHooks.useWatch(visible, syncGameStream);

  const viewHost = createViewHost(mountPoint);
  const router = createRouter<ViewFactory>({
    routes: [
      {
        pattern: '/',
        handler: createDashboardView({
          store,
          snapshots,
          api,
          auth,
          toasts,
          createOrderId: () => globalThis.crypto.randomUUID(),
        }),
      },
      {
        pattern: '/corporation',
        handler: createCorporationView({
          store,
          snapshots,
          api: corporationApi,
          toasts,
          createCommandId: () => globalThis.crypto.randomUUID(),
        }),
      },
      {
        pattern: '/career',
        handler: createCareerView({
          store,
          snapshots,
          api: careerApi,
          toasts,
          createCommandId: () => globalThis.crypto.randomUUID(),
        }),
      },
      {
        pattern: '/housing',
        handler: createHousingView({
          store,
          snapshots,
          api: housingApi,
          loanApi,
          createCommandId: () => globalThis.crypto.randomUUID(),
        }),
      },
      {
        pattern: '/events-insurance',
        handler: createEventsInsuranceView({
          store,
          snapshots,
          eventApi: lifeEventApi,
          insuranceApi,
          toasts,
          createCommandId: () => globalThis.crypto.randomUUID(),
        }),
      },
      {
        pattern: '/life',
        handler: createLifeView({
          store,
          snapshots,
          api: lifeApi,
          toasts,
          createCommandId: () => globalThis.crypto.randomUUID(),
        }),
      },
      {
        pattern: '/loans',
        handler: createLoansView({
          store,
          snapshots,
          api: loanApi,
          createCommandId: () => globalThis.crypto.randomUUID(),
        }),
      },
      {
        pattern: '/welfare',
        handler: createWelfareView({
          store,
          snapshots,
          api: welfareApi,
          toasts,
          createCommandId: () => globalThis.crypto.randomUUID(),
        }),
      },
      {
        pattern: '/playtest-feedback',
        handler: createPlaytestFeedbackView({ store, api: playtestApi }),
      },
      {
        pattern: '/recovery',
        handler: createRecoveryView({
          store,
          snapshots,
          api: insolvencyApi,
          createCommandId: () => globalThis.crypto.randomUUID(),
        }),
      },
      {
        pattern: '/new',
        handler: createCharacterCreateView({
          store,
          snapshots,
          api,
          loanApi,
          toasts,
          createCommandId: () => globalThis.crypto.randomUUID(),
        }),
      },
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
  appBag.add(() => viewHost.clear());
  appBag.add(router);

  const disposeOnPageHide = (): void => appBag.dispose();
  globalThis.addEventListener('pagehide', disposeOnPageHide, { once: true });
  appBag.add(() => globalThis.removeEventListener('pagehide', disposeOnPageHide));

  // The server reports login failure by query parameter (§4.5); do not leave it in the URL
  takeLoginError(store);

  router.start();

  // Check the session first: fetching game data while signed out only yields 401s
  void resume(store, snapshots, auth, api, router.navigate, logger, toasts, () => {
    gameStreamEnabled = true;
    syncGameStream();
  });
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
  snapshots: GameStateWriter,
  auth: AuthApi,
  api: GameApi,
  navigate: (to: string) => void,
  logger: Logger,
  toasts: ToastQueue,
  enableGameStream: () => void,
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

  if (globalThis.location.pathname === LOGIN_PATH) navigate('/');

  try {
    snapshots.apply(await api.getSnapshot());
  } catch (error) {
    logger.log('warn', '초기 상태를 불러오지 못했습니다', { error });
    toasts.show('게임 상태를 불러오지 못했습니다.', { tone: 'error' });
  } finally {
    // Fetch first, then subscribe. The stream's initial snapshot is ordered after this
    // response and can safely catch up anything that changed during the request.
    enableGameStream();
  }
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', bootstrap, { once: true });
} else {
  bootstrap();
}
