import { createGameApi } from './api/game-api.js';
import { createDashboardView } from './app/screens/dashboard.js';
import { createNotFoundView } from './app/screens/not-found.js';
import { type AppState, initialState, paths } from './app/state.js';
import { createConsoleLogger } from './lib/core/index.js';
import { createHttpClient } from './lib/http/index.js';
import { createRouter } from './lib/router/index.js';
import { createSseClient } from './lib/sse/index.js';
import { createStore } from './lib/store/index.js';
import { createViewHost, type ViewFactory } from './lib/view/index.js';

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
  const api = createGameApi({
    http,
    stream,
    onInvalidTick: (error) => logger.log('error', '틱 payload 가 계약과 다릅니다', { error }),
  });

  // 스트림 상태와 틱을 스토어로만 흘린다 — 화면은 스토어만 본다
  stream.onStatusChange((status) => store.set(paths.connectionStatus, status));
  api.onTick((snapshot) => store.set(paths.gameSnapshot, snapshot));

  const viewHost = createViewHost(mountPoint);
  const dashboard = createDashboardView({ store, api });
  const notFound = createNotFoundView();

  const router = createRouter<ViewFactory>({
    routes: [{ pattern: '/', handler: dashboard }],
    fallback: notFound,
    onNavigate: (factory, match) =>
      viewHost.render(factory, {
        params: match.params,
        query: match.query,
        navigate: (to) => router.navigate(to),
      }),
  });

  router.start();
  api.connectStream();

  void api
    .getSnapshot()
    .then((snapshot) => store.set(paths.gameSnapshot, snapshot))
    .catch((error: unknown) => logger.log('warn', '초기 상태를 불러오지 못했습니다', { error }));
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', bootstrap, { once: true });
} else {
  bootstrap();
}
