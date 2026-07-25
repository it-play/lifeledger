---
name: client-foundation
description: LifeLedger 클라이언트의 자체 기반 레이어(reactive·hooks·store·sse·http·router·view·form·dom) 사용법. 프레임워크 없이 화면을 만들거나 고칠 때, 훅·시그널·스토어 구독·폼·SSE 를 쓰는 방법이 필요할 때 읽는다.
---

## 언제 이 스킬을 쓰나

`client/` 에서 화면을 추가·수정하거나, 상태·스트림·폼을 다루는 코드를 쓸 때.
이 프로젝트는 **UI 프레임워크를 쓰지 않고** `client/src/lib/` 의 자체 레이어를 쓴다.
그 사용법이 여기 있다.

## 레이어 지도

| 모듈 | 역할 | 상세 |
|------|------|------|
| `lib/reactive` | 시그널·파생값·부수효과 (의존성 자동 추적) | `.agents/skills/client-foundation/references/reactive-store.md` |
| `lib/hooks` | 생명주기에 묶인 훅 17종 (React 훅 대응) | `.agents/skills/client-foundation/references/hooks.md` |
| `lib/store` | 서버 상태 보관 + 경로 단위 구독 | `.agents/skills/client-foundation/references/reactive-store.md` |
| `lib/view` + `lib/router` | 화면 생명주기와 라우팅 | `.agents/skills/client-foundation/references/view-router.md` |
| `lib/form` | 스키마 기반 폼 렌더러 | `.agents/skills/client-foundation/references/form-http.md` |
| `lib/http` | REST + zod 응답 검증 | `.agents/skills/client-foundation/references/form-http.md` |
| `lib/sse` | 직접 구현한 SSE 클라이언트 | `.agents/skills/client-foundation/references/sse.md` |
| `lib/dom` | `el()` · `bindText()` 같은 최소 헬퍼 | 아래 예시 참고 |

## 절대 규칙

1. **배럴만 import.** `from '../../lib/hooks/index.js'` ○ / `from '../../lib/hooks/create-hooks.js'` ✗
2. **DOM 은 mount 에서 한 번만 만든다.** 갱신은 바뀐 노드만 건드린다. 서브트리를 다시 만들지 않는다.
3. **모든 구독은 `ctx.bag` 소유.** 훅을 쓰면 자동으로 등록된다. 직접 `addEventListener` 하면 해제도 직접 등록해야 한다.
4. **금액은 정수 원.** 부동소수점으로 돈을 계산하지 않는다.
5. **서버 응답은 zod 로 검증한 뒤 쓴다.** 검증은 `api/` 경계에서 끝낸다.

## 화면 하나의 표준 형태

이 골격을 복사해서 시작한다.

```ts
import { el } from '../../lib/dom/index.js';
import { createHooks } from '../../lib/hooks/index.js';
import type { View, ViewFactory } from '../../lib/view/index.js';
import { paths, type AppState } from '../state.js';

export function createMyView(deps: MyDeps): ViewFactory {
  return (): View => ({
    mount(host, ctx) {
      const h = createHooks(ctx.bag);          // 1. 훅을 bag 에 묶는다

      const cash = h.useStoreValue(deps.store, paths.gameSnapshot, (s) => s.game.snapshot?.cashKrw);
      const cashText = h.useComputed(() => formatWon(cash.get() ?? 0));   // 2. 파생값

      const cashNode = el('strong');
      host.replaceChildren(el('section', {}, '현금 ', cashNode));          // 3. DOM 한 번

      h.bindText(cashNode, () => cashText.get());                         // 4. 노드 하나만 갱신
    },
    unmount() {},                                                          // 5. 정리는 bag 이 한다
  });
}
```

`createHooks(ctx.bag)` 이후로는 **호출 순서 규칙이 없다** — 조건문·루프 안에서 훅을 불러도 된다
(렌더 사이클이 없으므로). React 훅과 다른 점이니 헷갈리지 말 것.

## 자주 쓰는 조합

**서버 상태 → 화면**: `useStoreValue` → `useComputed` → `bindText`
**버튼 상태**: `h.bindAttribute(button, 'disabled', () => busy.get())`
**입력 지연**: `const search = h.useDebounced((q: string) => run(q), 200)`
**서버 호출 상태**: `const req = h.useAsync((signal) => api.listPresets(signal))` → `req.state` 는
`idle | loading | success | error`
**탭 숨김 대응**: `h.useWatch(h.useVisibility(), (v) => (v ? api.connectStream() : api.disconnectStream()))`

## 함정

- `useComputed` 안에서 신호를 **읽어야** 의존성이 걸린다. 조건 분기 때문에 읽지 않은 신호는 추적되지 않는다.
- 추적을 피하려면 `signal.peek()` 또는 `untracked()` 를 쓴다.
- `store.set()` 은 마이크로태스크로 모아 통지한다. 테스트에서는 `await Promise.resolve()` 후 검증한다.
- `store.watch(path, fn)` 의 경로는 문자열이다. 오타가 나면 조용히 안 불린다 — `app/state.ts` 의
  `paths` 상수를 쓴다.

## 테스트

`AGENTS.md` 의 정책을 따른다 — **핵심·서비스 로직만, jest, BDD/DCI 구조**.
이 기반 레이어에서 테스트 대상인 것: `reactive`, `store`, `hooks`(시간 의존 훅은 `createManualClock` 주입),
`sse/parser`, `sse/policy`. 화면·DOM·라우팅은 테스트하지 않는다.

```ts
const clock = createManualClock();
const bag = createDisposableBag();
const h = createHooks(bag, { clock });   // 시간을 직접 밀어 검증한다
clock.advance(200);
```
