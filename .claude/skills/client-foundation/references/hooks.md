# 훅 레퍼런스 (`lib/hooks`)

`const h = createHooks(bag, { clock? })` 로 만든다. 모든 훅은 만든 자원을 `bag` 에 등록하므로
`bag.dispose()` 한 번으로 전부 해제된다. **호출 순서 규칙 없음** — 조건문·루프 안에서도 쓸 수 있다.

## 상태와 파생

| 훅 | React 대응 | 설명 |
|----|-----------|------|
| `useSignal(initial, options?)` | `useState` | 지역 상태. `get()` / `peek()` / `set()` / `update()` |
| `useComputed(compute, options?)` | `useMemo` | 의존성 배열 없음. 읽은 신호를 자동 추적 |
| `useEffect(effect)` | `useEffect` | 정리 함수를 반환할 수 있다 |
| `useWatch(source, onChange)` | — | 첫 실행은 건너뛰고 **변화만**. `(next, previous)` 를 받는다 |
| `useStoreValue(store, path, selector)` | `useSyncExternalStore` | 스토어의 한 경로만 신호로 |

```ts
const amount = h.useSignal(1_000_000);
const doubled = h.useComputed(() => amount.get() * 2);

h.useEffect(() => {
  const id = subscribeSomething(amount.get());
  return () => unsubscribe(id);      // 다음 실행 전과 dispose 시 호출
});

h.useWatch(doubled, (next, prev) => console.log(prev, '→', next));
```

`options.equals` 로 같음 판단을 바꿀 수 있다 (객체를 값으로 비교할 때).

```ts
const point = h.useSignal({ x: 1 }, { equals: (a, b) => a.x === b.x });
```

## 시간

| 훅 | 설명 |
|----|------|
| `useInterval(handler, ms)` | 주기 실행. 반환값을 호출하면 중단 |
| `useTimeout(handler, ms)` | 지연 실행 |
| `useDebounced(handler, waitMs)` | 마지막 호출만 살린다. `.cancel()` 있음 |
| `useThrottled(handler, intervalMs)` | 첫 호출은 즉시, 이후 간격당 한 번 (마지막 인자로 트레일링 실행) |

```ts
const search = h.useDebounced((query: string) => api.searchJobs(query), 200);
input.addEventListener('input', () => search(input.value));   // 리스너는 useEventListener 권장
```

시간 의존 훅은 `createHooks(bag, { clock })` 의 `clock` 을 쓴다. 테스트에서 `createManualClock()`
을 주입하고 `clock.advance(ms)` 로 밀면 결정론적으로 검증된다.

## 이벤트

| 훅 | 설명 |
|----|------|
| `useEventListener(element, type, handler)` | 요소 이벤트. 해제 자동 등록 |
| `useWindowListener(type, handler)` | window 이벤트 |

```ts
h.useEventListener(button, 'click', () => void advance(1));
h.useWindowListener('resize', () => layout());
```

## 비동기

`useAsync(task)` — `task(signal)` 로 `AbortSignal` 을 받는다. `run()` 을 다시 부르면 이전 요청이
자동 취소되므로 경쟁 상태가 생기지 않는다.

```ts
const presets = h.useAsync((signal) => api.listPresets({ signal }));

h.useEffect(() => {
  const state = presets.state.get();
  if (state.status === 'loading') showSpinner();
  if (state.status === 'success') render(state.value);
  if (state.status === 'error') showError(state.error);
});

presets.run();
```

상태는 `{ status: 'idle' } | { status: 'loading' } | { status: 'success', value } | { status: 'error', error }`.

## 환경

| 훅 | 설명 |
|----|------|
| `useVisibility()` | 탭이 보이는지. 숨을 때 스트림·타이머를 멈추는 데 쓴다 |
| `useMediaQuery(query)` | 미디어 쿼리 일치 여부 |
| `useLocalStorage(key, initial)` | localStorage 에 붙은 신호. 저장 실패는 무시한다 |

```ts
const visible = h.useVisibility();
h.useWatch(visible, (v) => (v ? api.connectStream() : api.disconnectStream()));

const compact = h.useMediaQuery('(max-width: 640px)');
const speed = h.useLocalStorage('ui.speed', 1);
```

## DOM 바인딩

| 훅 | 설명 |
|----|------|
| `bindText(node, compute)` | 텍스트를 신호에 묶는다. 값이 같으면 DOM 을 건드리지 않는다 |
| `bindAttribute(element, name, compute)` | 속성을 묶는다. `false`/`undefined` 면 속성 제거, `true` 면 빈 값으로 설정 |

```ts
h.bindText(cashNode, () => formatWon(cash.get()));
h.bindAttribute(button, 'disabled', () => busy.get());       // boolean 속성
h.bindAttribute(link, 'href', () => `/jobs/${id.get()}`);    // 값 속성
```

## 안 만든 것과 그 이유

- `useRef` — 그냥 지역 변수를 쓴다 (렌더가 없어 참조가 유지된다)
- `useCallback` — 함수가 매 렌더 새로 생기지 않으므로 불필요
- `useContext` — 의존성은 `createHooks` 호출부에서 인자로 주입한다
- `useReducer` — `store.update(producer)` 가 그 역할을 한다
