import type { AuthApi } from '../../api/auth-api.js';
import { STEP_DAYS, type StepUnit } from '../../api/contracts.js';
import type { GameApi } from '../../api/game-api.js';
import { el } from '../../lib/dom/index.js';
import { createHooks } from '../../lib/hooks/index.js';
import type { Store } from '../../lib/store/index.js';
import type { ToastQueue } from '../../lib/toast/index.js';
import type { View, ViewFactory } from '../../lib/view/index.js';
import { CONNECTION_LABEL, formatGameDate, formatWon } from '../format.js';
import { type AppState, paths } from '../state.js';

export interface DashboardDeps {
  readonly store: Store<AppState>;
  readonly api: GameApi;
  readonly auth: AuthApi;
  readonly toasts: ToastQueue;
}

/**
 * The dashboard, which is the reference for this project's render convention:
 *  - build the DOM once in `mount`
 *  - update through hooks (bindText, bindAttribute) that touch only the changed node
 *  - register every subscription and listener with ctx.bag, released on unmount
 */
export function createDashboardView(deps: DashboardDeps): ViewFactory {
  return (): View => {
    let root: HTMLElement | undefined;

    return {
      mount(host, ctx) {
        const { store, api, auth, toasts } = deps;
        const h = createHooks(ctx.bag);

        // Pull in only the paths this screen needs; other changes do not wake it
        const snapshot = h.useStoreValue(store, paths.gameSnapshot, (s) => s.game.snapshot);
        const advancing = h.useStoreValue(store, paths.gameAdvancing, (s) => s.game.advancing);
        const connection = h.useStoreValue(
          store,
          paths.connectionStatus,
          (s) => s.connection.status,
        );

        // Without a character, route to creation
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
            void advance(store, api, toasts, STEP_DAYS[unit]);
          });
        }

        h.useEventListener(logoutButton, 'click', () => {
          void logout(auth, toasts);
        });

        // No snapshot yet (undefined) differs from no character (null); navigating before
        // login and the first fetch settle would flash the creation screen
        h.useWatch(characterName, (name) => {
          if (name === null && authStatus.get() === 'authenticated') ctx.navigate('/new');
        });

        // Drop the stream on a hidden tab and reattach on return, sparing battery and connections
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

async function logout(auth: AuthApi, toasts: ToastQueue): Promise<void> {
  try {
    await auth.logout();
  } catch {
    toasts.show('로그아웃하지 못했습니다. 다시 시도해 주세요.', { tone: 'error' });
    return;
  }
  // A full reload is the reliable way to pick up the cleared session cookie
  globalThis.location.assign('/');
}

async function advance(
  store: Store<AppState>,
  api: GameApi,
  toasts: ToastQueue,
  days: number,
): Promise<void> {
  store.set(paths.gameAdvancing, true);
  try {
    store.set(paths.gameSnapshot, await api.advance(days));
  } catch {
    toasts.show('게임일을 진행하지 못했습니다. 다시 시도해 주세요.', { tone: 'error' });
  } finally {
    store.set(paths.gameAdvancing, false);
  }
}
