import type { Clock, Disposable, Logger, Unsubscribe } from '../core/types.js';

/** 스펙상 하나의 이벤트. `type` 은 `event:` 필드가 없으면 'message'. */
export interface SseMessage {
  readonly type: string;
  readonly data: string;
  readonly lastEventId: string;
}

/**
 * 증분 파서. 네트워크에서 온 문자열 조각을 순서대로 넣으면 완성된 이벤트를 돌려준다.
 * 순수 로직이라 네트워크 없이 단독 테스트할 수 있다.
 */
export interface EventStreamParser {
  /** 청크를 넣고, 이번 청크로 완성된 이벤트들을 순서대로 받는다. */
  push(chunk: string): readonly SseMessage[];
  /** 스트림이 끝났을 때. 스펙에 따라 미완성 데이터는 버린다. */
  end(): void;
  /** 재연결 시 `Last-Event-ID` 로 보낼 값. dispatch 시에도 초기화되지 않는다. */
  readonly lastEventId: string;
  /** 서버가 `retry:` 로 알려준 재연결 지연(ms). 없으면 undefined. */
  readonly serverRetryMs: number | undefined;
}

export type SseStatus = 'idle' | 'connecting' | 'open' | 'reconnecting' | 'closed';

/** 연결이 끊긴 이유. 재시도 여부 판단의 입력이 된다. */
export type DisconnectReason =
  | { readonly kind: 'network'; readonly error: unknown }
  | { readonly kind: 'http'; readonly status: number }
  | { readonly kind: 'bad-content-type'; readonly contentType: string | null }
  | { readonly kind: 'stream-ended' }
  | { readonly kind: 'closed-by-caller' };

/** 재연결 여부 정책. 기본 구현은 정책 파일에 있고, 주입해 바꿀 수 있다. */
export interface RetryDecider {
  shouldRetry(reason: DisconnectReason): boolean;
}

/** 재연결 지연 계산. attempt 는 1부터 시작한다. */
export interface BackoffPolicy {
  delayMs(attempt: number, serverRetryMs: number | undefined): number;
}

export interface SseClientOptions {
  readonly url: string;
  /** 기본 헤더. Accept 와 Last-Event-ID 는 클라이언트가 관리한다. */
  readonly headers?: Readonly<Record<string, string>>;
  readonly credentials?: RequestCredentials;
  readonly backoff?: BackoffPolicy;
  readonly retryDecider?: RetryDecider;
  readonly clock?: Clock;
  readonly logger?: Logger;
  /** 테스트에서 가짜 fetch 를 넣기 위한 구멍. */
  readonly fetchImpl?: typeof fetch;
}

/**
 * SSE 연결 하나를 관리한다.
 * EventSource 를 쓰지 않는 이유: 커스텀 헤더를 붙일 수 없고, 재연결 정책을
 * 제어할 수 없고, 상태 전이를 관찰하기 어렵다.
 */
export interface SseClient extends Disposable {
  readonly status: SseStatus;
  readonly lastEventId: string;
  /** 이벤트 타입별 구독. 타입 이름은 서버의 `event:` 값. */
  on(type: string, handler: (message: SseMessage) => void): Unsubscribe;
  /** 모든 이벤트 구독. */
  onAny(handler: (message: SseMessage) => void): Unsubscribe;
  onStatusChange(handler: (status: SseStatus) => void): Unsubscribe;
  /** 연결 시작. 이미 연결 중이면 무시. */
  connect(): void;
  /** 연결 종료. 재연결하지 않는다. */
  close(): void;
}
