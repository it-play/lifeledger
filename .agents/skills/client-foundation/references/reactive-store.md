# 반응성과 스토어 (`lib/reactive`, `lib/store`)

두 계층의 역할이 다르다.

- **reactive** — 값 하나의 변화를 추적하는 저수준 도구. 화면 지역 상태와 파생 계산.
- **store** — 서버에서 온 앱 전역 상태. 경로 문자열로 구독한다.

화면에서는 보통 `hooks` 를 통해 쓰고, 직접 부르는 건 라이브러리 코드나 테스트다.

## reactive

```ts
import { batch, createComputed, createEffect, createSignal, untracked } from '../lib/reactive/index.js';

const cash = createSignal(10_000_000);
const debt = createSignal(20_000_000);
const netWorth = createComputed(() => cash.get() - debt.get());

createEffect(() => {
  console.log('순자산', netWorth.get());     // 읽었으므로 의존성이 걸린다
});

cash.set(15_000_000);                         // effect 다시 실행
```

### 추적 규칙

- `get()` 은 **현재 추적 문맥에 의존성을 등록**한다. `peek()` 은 등록하지 않는다.
- 실행할 때마다 의존성을 새로 모은다. 조건 분기로 읽지 않은 신호는 그 회차의 의존성이 아니다.
- `untracked(() => ...)` 안에서 읽으면 의존성으로 잡히지 않는다.

```ts
createEffect(() => {
  const shown = visible.get();               // 추적됨
  const seed = untracked(() => worldSeed.get());  // 추적 안 됨
});
```

### computed 는 결과가 바뀔 때만 전파한다

중간값이 흔들려도 최종 결과가 같으면 구독자를 깨우지 않는다.

```ts
const netWorth = createComputed(() => cash.get() - debt.get());
batch(() => {
  cash.set(200);   // 100 → 200
  debt.set(100);   // 0 → 100
});
// netWorth 는 여전히 100 → 알림 없음
```

### batch

여러 값을 한 번에 바꿀 때 (배속 진행 정산 등) 구독자를 한 번만 깨운다.

```ts
batch(() => {
  day.set(day.peek() + 1);
  cash.set(nextCash);
  debt.set(nextDebt);
});
```

### effect 의 정리

```ts
const handle = createEffect(() => {
  const timer = setInterval(tick, 1000);
  return () => clearInterval(timer);   // 다음 실행 전 + dispose 시
});
handle.run();      // 강제 재실행
handle.dispose();  // 정리 후 구독 해제
```

## store

```ts
import { createStore } from '../lib/store/index.js';

const store = createStore<AppState>(initialState);

store.set('game.snapshot', snapshot);        // 경로 하나만 교체
store.update((s) => ({ ...s, ui: { ...s.ui, busy: true } }));   // 순수 함수로 교체

const off = store.watch('game.snapshot', () => render());
const offAll = store.watchAll((state, changed) => console.log(changed));
```

### 경로 구독의 의미

- 상위 경로 구독은 하위 변경에 반응한다 (`'game'` 구독 → `'game.day'` 변경에 반응)
- 하위 경로 구독도 상위 교체에 반응한다 (`'game.day'` 구독 → `'game'` 교체에 반응)
- 형제 경로는 서로 무관하다 (`'game.day'` ↔ `'ui.busy'`)

### 알림은 마이크로태스크로 모인다

한 틱에 여러 번 `set` 해도 구독자는 한 번만 깨어난다.

```ts
store.set('game.day', 1);
store.set('game.day', 2);
store.set('game.cash', 50);
// → 구독자 1회 호출
await Promise.resolve();   // 테스트에서는 이걸 기다린 뒤 검증
```

즉시 통지가 필요하면 `createStore(initial, { batch: false })`.

### 구조 공유

`set` 은 경로상의 객체만 얕게 복사한다. 건드리지 않은 가지는 참조가 그대로라서 변경 경로 계산이
싸다. 그래서 **상태를 얕고 안정적인 구조로 유지**해야 한다 (깊게 중첩하면 경로가 길어지고 diff 가 커진다).

### 경로 오타 방지

경로는 문자열이라 오타가 나면 구독이 조용히 죽는다. `app/state.ts` 의 `paths` 상수를 쓴다.

```ts
export const paths = {
  gameSnapshot: 'game.snapshot',
  gameAdvancing: 'game.advancing',
  connectionStatus: 'connection.status',
} as const;
```
