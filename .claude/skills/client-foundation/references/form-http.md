# 폼과 HTTP (`lib/form`, `lib/http`, `api/`)

## 폼 렌더러

캐릭터 생성·이력서·공고 검색이 전부 폼이라 **하나를 재사용**한다. 화면은 DOM 을 조립하지 않고
`FieldSpec` 목록과 검증기만 준다.

```ts
import { renderForm, type FieldSpec } from '../lib/form/index.js';
import { asFormValidator } from '../api/zod-adapters.js';

const FIELDS: readonly FieldSpec[] = [
  { name: 'name', label: '이름', kind: 'text' },
  { name: 'age', label: '나이', kind: 'number', help: '19 ~ 50' },
  {
    name: 'military',
    label: '병역',
    kind: 'select',
    options: [
      { value: 'completed', label: '필' },
      { value: 'exempted', label: '면제' },
    ],
  },
];

const form = renderForm(
  { fields: FIELDS, validator: asFormValidator(CharacterDraftSchema), submitLabel: '시작' },
  {
    initial: { name: '', age: 25, military: 'completed' },
    onSubmit: async (draft) => { await api.createCharacter(draft); },
  },
);

ctx.bag.add(form);                 // 정리 등록
container.appendChild(form.element);
```

`kind` 는 `text` · `number` · `select` · `checkbox`. `number` 는 빈 값을 `undefined` 로 읽어
검증기가 "필수" 오류를 낼 수 있게 한다.

### 핸들 API

| 메서드 | 용도 |
|--------|------|
| `element` | 폼 DOM. 화면이 원하는 위치에 붙인다 |
| `setErrors(map)` | 필드 → 메시지. **서버가 준 오류를 표시할 때** |
| `setValues(map)` | 값 일부를 바꿔 넣는다. 프리셋 적용에 쓴다 |
| `reset()` | 초기화 + 오류 지우기 |
| `dispose()` | 리스너 해제 (`bag.add(form)` 하면 자동) |

제출 중에는 버튼이 자동으로 비활성화되고, `onSubmit` 이 던진 오류는 폼 전체 오류로 표시된다.

### 검증은 어디서 하나

- **클라이언트** = 필드 형태만 (타입·범위·필수). zod 스키마 하나로 끝낸다.
- **서버** = 조합 규칙의 **유일한 권위** (§3.5 나이↔병역, 학업+복무+경력 ≤ 나이 …).

서버가 422 로 준 필드 오류를 그대로 `setErrors` 에 넘긴다. 같은 규칙을 클라이언트에서 다시
구현하지 않는다 — 두 곳에 살면 반드시 어긋난다.

```ts
try {
  await api.createCharacter(draft);
} catch (error) {
  if (error instanceof CharacterRejectedError) {
    form.setErrors(error.fieldErrors);   // 서버 판단을 그대로 표시
    return;
  }
  throw error;
}
```

## HTTP 클라이언트

응답 검증을 **강제**한다. 디코더 없이 호출할 방법이 없다.

```ts
import { createHttpClient } from '../lib/http/index.js';
import { asDecoder } from '../api/zod-adapters.js';

const http = createHttpClient({ logger, credentials: 'same-origin' });
const snapshotDecoder = asDecoder(GameSnapshotSchema);

const snapshot = await http.get('/api/state', snapshotDecoder);
const next = await http.post('/api/advance', { days: 7 }, snapshotDecoder);
```

오류 두 종류를 구분해서 던진다.

| 오류 | 언제 |
|------|------|
| `HttpError(status, path, body)` | 서버가 4xx/5xx 를 줬다. `body` 는 파싱된 JSON |
| `ResponseShapeError(path, cause)` | 상태는 정상인데 형태가 계약과 다르다 (서버·클라 버전 불일치) |

`ResponseShapeError` 가 나면 화면 버그가 아니라 **계약 불일치**다. 조용히 넘기지 말 것.

## 도메인 API 계층 (`api/game-api.ts`)

화면은 `HttpClient` · `SseClient` 를 직접 만지지 않는다. 도메인 API 인터페이스만 본다.

```ts
export interface GameApi {
  listPresets(): Promise<readonly Preset[]>;
  createCharacter(draft: CharacterDraft): Promise<GameSnapshot>;
  getSnapshot(): Promise<GameSnapshot>;
  advance(days: number): Promise<GameSnapshot>;
  onTick(handler: (snapshot: GameSnapshot) => void): Unsubscribe;
  connectStream(): void;
  disconnectStream(): void;
}
```

여기서 HTTP 상태 코드를 도메인 오류로 바꾼다 (`422` → `CharacterRejectedError`). 화면이
상태 코드를 알 필요가 없게 만드는 것이 이 계층의 목적이다.

## zod 접착은 한 파일에만

`lib/` 은 zod 를 모른다. `api/zod-adapters.ts` 의 두 함수가 유일한 연결점이다.

```ts
asDecoder(schema)        // → lib/http 의 ResponseDecoder
asFormValidator(schema)  // → lib/form 의 FormValidator
```

나중에 서버 OpenAPI 코드젠으로 갈아탈 때 이 파일만 바꾸면 된다.
