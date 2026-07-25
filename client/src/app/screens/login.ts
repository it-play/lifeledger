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
  /** 브라우저 이동. 테스트에서 갈아끼울 수 있게 주입받는다. */
  readonly redirect?: (url: string) => void;
}

/**
 * 로그인 화면 (§4.5).
 *
 * 제공자 목록은 서버가 준다 — 자격증명이 없는 제공자는 목록에 없으므로
 * "버튼은 있는데 누르면 실패" 가 생기지 않는다.
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
        const buttonList = el('div', { class: 'login-providers' });

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

        // 목록을 받아오기 전까지는 버튼 자리를 비워 둔다
        const providers = h.useAsync<readonly AuthProvider[]>(() => auth.listProviders());
        h.useWatch(providers.state, (result) => {
          if (result.status !== 'success') return;
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
  // 화면이 사라질 때 버튼도 함께 사라지므로 별도 해제가 필요 없다
  button.addEventListener('click', () => redirect(`/api/auth/${provider.id}/start`));

  return button;
}
