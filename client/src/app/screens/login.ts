import type { AuthApi } from '../../api/auth-api.js';
import type { AuthProvider } from '../../api/contracts.js';
import { el } from '../../lib/dom/index.js';
import { createHooks } from '../../lib/hooks/index.js';
import type { Store } from '../../lib/store/index.js';
import type { View, ViewFactory } from '../../lib/view/index.js';
import { LOGIN_ERROR_LABEL } from '../format.js';
import { type AppState, paths } from '../state.js';

export interface LoginDeps {
  readonly store: Store<AppState>;
  readonly auth: AuthApi;
  /** Browser navigation, injected so tests can substitute it. */
  readonly redirect?: (url: string) => void;
}

/**
 * The login screen (§4.5).
 *
 * The provider list comes from the server, which omits any provider lacking credentials,
 * so no button can exist that fails when pressed.
 */
export function createLoginView(deps: LoginDeps): ViewFactory {
  return (): View => {
    let root: HTMLElement | undefined;

    return {
      mount(host, ctx) {
        const { store, auth } = deps;
        const redirect = deps.redirect ?? ((url: string) => globalThis.location.assign(url));
        const h = createHooks(ctx.bag);

        const error = h.useStoreValue(store, paths.authError, (s) => s.auth.error);
        const errorText = h.useComputed(() => {
          const reason = error.get();
          return reason === undefined ? '' : (LOGIN_ERROR_LABEL[reason] ?? '로그인하지 못했습니다');
        });

        const errorLine = el('p', { class: 'login-error' });
        const buttonList = el('div', { class: 'login-providers' }, '로그인 수단 불러오는 중…');

        root = el(
          'section',
          { class: 'login' },
          el('h1', {}, 'LifeLedger'),
          el('p', { class: 'login-lead' }, '계정으로 로그인하면 세이브가 이어집니다.'),
          errorLine,
          buttonList,
        );
        host.replaceChildren(root);

        h.bindText(errorLine, () => errorText.get());
        h.bindAttribute(errorLine, 'hidden', () => errorText.get() === '');

        const providers = h.useAsync<readonly AuthProvider[]>(() => auth.listProviders());
        h.useEffect(() => {
          const result = providers.state.get();
          if (result.status === 'error') {
            buttonList.replaceChildren('로그인 수단을 불러오지 못했습니다. 새로고침해 주세요.');
            return;
          }
          if (result.status !== 'success') return;
          if (result.value.length === 0) {
            buttonList.replaceChildren('현재 사용할 수 있는 로그인 수단이 없습니다.');
            return;
          }
          buttonList.replaceChildren(
            ...result.value.map((provider) => providerButton(provider, redirect)),
          );
        });
        providers.run();
      },

      unmount() {
        root?.remove();
        root = undefined;
      },
    };
  };
}

function providerButton(provider: AuthProvider, redirect: (url: string) => void): HTMLElement {
  const button = el(
    'button',
    { type: 'button', class: `login-button login-button--${provider.id}` },
    `${provider.label} 계정으로 로그인`,
  );
  // The button dies with the screen, so the listener needs no separate removal
  button.addEventListener('click', () => redirect(`/api/auth/${provider.id}/start`));

  return button;
}
