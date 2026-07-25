# 화면과 라우팅 (`lib/view`, `lib/router`, `lib/dom`)

## 화면 계약

```ts
export interface View {
  mount(host: HTMLElement, ctx: ViewContext): void | Promise<void>;
  unmount(): void;
}
export type ViewFactory = () => View;
```

`ViewContext` 가 화면이 외부에 접근하는 **유일한 통로**다. 여기 없는 건 화면이 쓸 수 없다
(전역을 읽지 않는다 — 테스트에서 갈아끼울 수 있어야 한다).

| 필드 | 용도 |
|------|------|
| `params` | 라우트 파라미터 (`/game/:id` → `{ id }`) |
| `query` | `URLSearchParams` |
| `bag` | 구독·리스너 정리 소유자. `createHooks(ctx.bag)` 로 넘긴다 |
| `navigate(to)` | 화면 전환 |

의존성(store·api)은 팩토리를 만들 때 주입한다.

```ts
export function createMyView(deps: { store: Store<AppState>; api: GameApi }): ViewFactory {
  return (): View => ({ mount(host, ctx) { /* … */ }, unmount() {} });
}
```

## ViewHost

한 번에 화면 하나만 띄우고, 전환할 때 이전 화면의 `unmount()` 와 `bag.dispose()` 를 **강제**한다.
프레임워크 없이 누수를 막는 핵심이다.

```ts
const viewHost = createViewHost(document.getElementById('app'));
await viewHost.render(myViewFactory, { params: {}, query, navigate });
```

비동기 `mount` 중에 다른 화면으로 전환되면, 늦게 끝난 화면은 세대 검사로 버려진다
(화면이 겹쳐 그려지는 사고를 막는다).

## 라우터

```ts
const router = createRouter<ViewFactory>({
  routes: [
    { pattern: '/', handler: dashboard },
    { pattern: '/new', handler: characterCreate },
    { pattern: '/jobs/:id', handler: jobDetail },
  ],
  fallback: notFound,
  onNavigate: (factory, match) =>
    viewHost.render(factory, {
      params: match.params,
      query: match.query,
      navigate: (to) => router.navigate(to),
    }),
});

router.start();
```

라우터는 **무엇을 그릴지 모른다** — `onNavigate` 가 정한다. 그래서 화면 렌더 방식을 바꿀 때
라우터를 건드릴 필요가 없다.

### 앱 내부 링크

`data-link` 가 붙은 `<a>` 는 라우터가 가로채 전체 새로고침 없이 전환한다.

```ts
el('a', { href: '/new', dataset: { link: '' } }, '새 캐릭터로 다시 시작');
```

수식 키(⌘/Ctrl/Shift/Alt) 클릭과 외부 origin 은 가로채지 않는다 — 새 탭 열기가 정상 동작한다.

## DOM 헬퍼

가상 DOM 이 없다. **만들 때는 `el()`, 바꿀 때는 바꿀 노드만.**

```ts
import { bindText, el, on } from '../lib/dom/index.js';

const cash = el('strong');
const root = el(
  'section',
  { class: 'dashboard' },
  el('h1', {}, 'LifeLedger'),
  el('dl', {}, el('dt', {}, '현금'), el('dd', {}, cash)),
);
host.replaceChildren(root);
```

`el(tag, attributes, ...children)` 의 attributes:

| 키 | 의미 |
|----|------|
| `class` `id` `type` `name` `value` `href` | 해당 속성으로 설정 |
| `disabled: true` | 빈 값 속성으로 설정 |
| `dataset: { unit: 'day' }` | `data-unit="day"` |
| `attrs: { 'aria-label': '진행' }` | 그 밖의 임의 속성 |

`bindText(node)` 는 갱신 함수를 돌려주고, 값이 같으면 DOM 을 건드리지 않는다.
훅을 쓸 수 있는 화면에서는 `h.bindText(node, compute)` 가 더 낫다 (신호 변화를 자동으로 따라간다).

`on(element, type, handler)` 는 해제 함수를 돌려준다 — 반드시 `bag` 에 넣는다.
훅이 있으면 `h.useEventListener` 를 쓰는 게 안전하다 (등록을 잊을 수 없다).
