import type { AuthApi } from '../../api/auth-api.js';
import { STEP_DAYS, type StepUnit } from '../../api/contracts.js';
import type { GameApi } from '../../api/game-api.js';
import { el } from '../../lib/dom/index.js';
import { createHooks } from '../../lib/hooks/index.js';
import type { Store } from '../../lib/store/index.js';
import type { View, ViewFactory } from '../../lib/view/index.js';
import { CONNECTION_LABEL, formatGameDate, formatWon } from '../format.js';
import { type AppState, paths } from '../state.js';

export interface DashboardDeps {
  readonly store: Store<AppState>;
  readonly api: GameApi;
  readonly auth: AuthApi;
}

/**
 * 대시보드. 이 프로젝트의 렌더 규약을 보여주는 기준 화면이다.
 *  - mount 에서 DOM 을 한 번 만든다
 *  - 이후 갱신은 훅(bindText·bindAttribute)이 신호 변화만 보고 해당 노드만 건드린다
 *  - 모든 구독·리스너는 ctx.bag 에 등록되어 unmount 에서 일괄 해제된다
 */
export function createDashboardView(deps: DashboardDeps): ViewFactory {
  return (): View => {
    let root: HTMLElement | undefined;

    return {
      mount(host, ctx) {
        const { store, api, auth } = deps;
        const h = createHooks(ctx.bag);

        // 스토어의 특정 경로만 신호로 끌어온다 — 나머지 변경에는 반응하지 않는다
        const snapshot = h.useStoreValue(store, paths.gameSnapshot, (s) => s.game.snapshot);
        const advancing = h.useStoreValue(store, paths.gameAdvancing, (s) => s.game.advancing);
        const connection = h.useStoreValue(
          store,
          paths.connectionStatus,
          (s) => s.connection.status,
        );

        // 캐릭터를 아직 만들지 않았으면 생성 화면으로 보낸다
        const characterName = h.useStoreValue(
          store,
          paths.gameSnapshot,
          (s) => s.game.snapshot?.characterName ?? null,
        );
        const authStatus = h.useStoreValue(store, paths.authStatus, (s) => s.auth.status);
        const account = h.useStoreValue(store, paths.authUser, (s) => s.auth.user);
        const accountText = h.useComputed(() => {
          const user = account.get();
          if (user === undefined) return '';
          return user.displayName ?? user.email ?? '';
        });

        const dateText = h.useComputed(() => {
          const current = snapshot.get();
          return current === undefined ? '—' : formatGameDate(current.startDate, current.gameDay);
        });
        const dayText = h.useComputed(() => `${snapshot.get()?.gameDay ?? '—'}`);
        const cashText = h.useComputed(() => moneyText(snapshot.get()?.cashKrw));
        const netWorthText = h.useComputed(() => moneyText(snapshot.get()?.netWorthKrw));
        const statusText = h.useComputed(() => {
          const status = connection.get();
          return CONNECTION_LABEL[status] ?? status;
        });

        const dateValue = el('strong');
        const dayValue = el('span');
        const cashValue = el('strong');
        const netWorthValue = el('strong');
        const statusValue = el('span', { class: 'status' });
        const accountValue = el('span', { class: 'account' });
        const logoutButton = el('button', { type: 'button', class: 'logout' }, '로그아웃');

        const stepButtons = (['day', 'week', 'month'] as const).map((unit) =>
          el('button', { type: 'button', dataset: { unit } }, stepLabel(unit)),
        );

        root = el(
          'section',
          { class: 'dashboard' },
          el('h1', {}, 'LifeLedger'),
          el('p', { class: 'account-line' }, accountValue, ' ', logoutButton),
          el('p', { class: 'connection' }, '스트림: ', statusValue),
          el(
            'dl',
            { class: 'summary' },
            el('dt', {}, '게임 날짜'),
            el('dd', {}, dateValue, ' (', dayValue, '일차)'),
            el('dt', {}, '현금'),
            el('dd', {}, cashValue),
            el('dt', {}, '순자산'),
            el('dd', {}, netWorthValue),
          ),
          el('div', { class: 'controls' }, ...stepButtons),
          el('p', {}, el('a', { href: '/new', dataset: { link: '' } }, '새 캐릭터로 다시 시작')),
        );
        host.replaceChildren(root);

        h.bindText(dateValue, () => dateText.get());
        h.bindText(dayValue, () => dayText.get());
        h.bindText(cashValue, () => cashText.get());
        h.bindText(netWorthValue, () => netWorthText.get());
        h.bindText(statusValue, () => statusText.get());
        h.bindText(accountValue, () => accountText.get());

        for (const button of stepButtons) {
          h.bindAttribute(button, 'disabled', () => advancing.get());
          h.useEventListener(button, 'click', () => {
            const unit = button.dataset.unit as StepUnit | undefined;
            if (unit === undefined) return;
            void advance(store, api, STEP_DAYS[unit]);
          });
        }

        h.useEventListener(logoutButton, 'click', () => {
          void logout(auth);
        });

        // 스냅샷이 아직 없을 때(undefined) 와 캐릭터가 없을 때(null) 는 다르다.
        // 로그인·조회가 끝나기 전에 보내면 생성 화면이 잠깐 번쩍인다
        h.useWatch(characterName, (name) => {
          if (name === null && authStatus.get() === 'authenticated') ctx.navigate('/new');
        });

        // 탭이 숨으면 스트림을 끊고, 돌아오면 다시 붙는다 (모바일 배터리·서버 연결 절약)
        const visible = h.useVisibility();
        h.useWatch(visible, (isVisible) => {
          if (isVisible) api.connectStream();
          else api.disconnectStream();
        });
      },

      unmount() {
        root?.remove();
        root = undefined;
      },
    };
  };
}

const moneyText = (amount: number | undefined): string =>
  amount === undefined ? '—' : formatWon(amount);

const stepLabel = (unit: StepUnit): string =>
  unit === 'day' ? '1일 진행' : unit === 'week' ? '1주 진행' : '1개월 진행';

async function logout(auth: AuthApi): Promise<void> {
  try {
    await auth.logout();
  } finally {
    // 세션 쿠키가 사라진 상태를 확실히 반영하려면 새로 읽는 편이 안전하다
    globalThis.location.assign('/');
  }
}

async function advance(store: Store<AppState>, api: GameApi, days: number): Promise<void> {
  store.set(paths.gameAdvancing, true);
  try {
    const snapshot = await api.advance(days);
    store.set(paths.gameSnapshot, snapshot);
  } catch (error) {
    store.set('connection.lastError', error instanceof Error ? error.message : '진행 실패');
  } finally {
    store.set(paths.gameAdvancing, false);
  }
}
