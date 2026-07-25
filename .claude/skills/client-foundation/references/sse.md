# SSE 클라이언트 (`lib/sse`)

`EventSource` 를 쓰지 않고 fetch 스트리밍 위에 직접 구현했다. 이유는 넷이다.

- 요청 헤더를 붙일 수 없다 (쿠키 외 인증 수단을 못 씀)
- 재연결 지연을 제어할 수 없다 (백오프·지터 불가)
- 상태 전이를 세밀하게 관찰하기 어렵다
- POST 로 열 수 없다

파싱·재연결 의미론은 WHATWG HTML "event stream" 알고리즘을 그대로 지킨다.

## 사용법

```ts
import { createSseClient } from '../lib/sse/index.js';

const stream = createSseClient({
  url: '/api/stream',
  logger,
  credentials: 'same-origin',
});

stream.on('tick', (message) => {                 // 서버의 `event: tick`
  const snapshot = JSON.parse(message.data);
});
stream.onStatusChange((status) => store.set('connection.status', status));

stream.connect();
// ...
stream.close();      // 재연결 안 함
stream.dispose();    // 핸들러까지 정리
```

상태는 `idle → connecting → open → reconnecting → closed`.
`stream.lastEventId` 로 마지막으로 받은 `id` 를 볼 수 있다 (재연결 시 `Last-Event-ID` 로 자동 전송).

화면에서는 보통 이걸 직접 만지지 않고 `api/game-api.ts` 의 `onTick()` 을 쓴다 — payload 를 zod 로
검증해서 넘겨주기 때문이다.

## 정책 교체

재연결 판단과 지연은 인터페이스로 분리돼 있다.

```ts
import { createExponentialBackoff, createDefaultRetryDecider } from '../lib/sse/index.js';

createSseClient({
  url: '/api/stream',
  backoff: createExponentialBackoff({ baseMs: 500, maxMs: 10_000, jitterRatio: 0.3 }),
  retryDecider: { shouldRetry: (reason) => reason.kind !== 'http' },
});
```

기본 판단:

| 끊긴 이유 | 재시도 |
|-----------|:---:|
| 네트워크 오류, 스트림 정상 종료 | ○ |
| 5xx, 408, 425, 429 | ○ |
| 그 밖의 4xx, 204, Content-Type 불일치, 호출자 종료 | ✗ |

서버가 `retry:` 를 보내면 그 값이 백오프의 기준이 된다 (지수 증가는 그 위에서).

## 파서를 고칠 때 주의할 것

`sse/parser.ts` 는 순수 증분 파서다. 스펙에서 틀리기 쉬운 지점들:

- 줄 구분자는 **CRLF · LF · CR 세 가지 모두**
- 맨 앞 BOM **하나만** 제거
- `:` 로 시작하는 줄은 주석 (서버 keep-alive 가 이걸 쓴다)
- 콜론 없는 줄은 필드명만 있고 값은 빈 문자열
- 값 앞 스페이스는 **하나만** 제거 (두 칸이면 한 칸은 값에 남는다)
- `data` 여러 줄은 LF 로 이어지고, dispatch 때 **마지막 LF 하나만** 제거
- `data` 가 비어 있으면 **이벤트를 발생시키지 않는다**
- NUL 이 든 `id` 는 무시 (기존 값 유지)
- `lastEventId` 는 dispatch 후에도 **유지된다** (event/data 버퍼만 초기화)
- EOF 시 미완성 데이터는 폐기
- **청크가 `\r` 로 끝나는 경우** — 줄은 이미 끝났으니 바로 처리하고, 다음 청크 첫 LF 하나만
  건너뛴다. 줄 전체를 보류하면 이벤트가 한 청크 늦게 나가거나 다음 LF 가 빈 줄로 오인된다.

이 규칙들은 전부 `parser.test.ts` 에 케이스가 있다. 파서를 고치면 그 테스트가 먼저 깨져야 한다.

## 서버 쪽 계약

`server/src/routes.rs` 의 `/api/stream` 이 보내는 형태:

```
event: tick
id: 0                ← 게임일. 재연결 시 Last-Event-ID 로 돌아온다
data: {"gameDay":0,"startDate":"2026-01-01",...}
retry: 1000          ← 첫 이벤트에만

:                    ← keep-alive 주석 (15초 간격)
```
