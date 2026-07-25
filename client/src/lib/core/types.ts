/**
 * 라이브러리 전역에서 공유하는 최소 계약.
 * 여기 있는 것들은 구현을 갖지 않는다 — 구현체는 각 모듈이 제공한다.
 */

/** 구독 해제 함수. 두 번 호출해도 안전해야 한다. */
export type Unsubscribe = () => void;

/** 정리해야 할 자원을 가진 객체. */
export interface Disposable {
  dispose(): void;
}

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

/**
 * 로깅 대상. 라이브러리는 console 을 직접 부르지 않고 이 인터페이스만 쓴다.
 * 테스트에서 로그를 수집하거나, 운영에서 원격 수집기로 바꿔 끼울 수 있어야 한다.
 */
export interface Logger {
  log(level: LogLevel, message: string, context?: Readonly<Record<string, unknown>>): void;
  child(scope: string): Logger;
}

/**
 * 시간 의존성. 재연결 지연·타임아웃처럼 시간에 의존하는 로직을
 * 테스트에서 결정론적으로 검증하기 위해 주입 가능하게 둔다.
 */
export interface Clock {
  now(): number;
  setTimeout(handler: () => void, delayMs: number): CancelTimer;
}

export type CancelTimer = () => void;

/** 예외를 값으로 다루고 싶은 경계(파싱·네트워크)에서 사용한다. */
export type Result<T, E = Error> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: E };

export const ok = <T>(value: T): Result<T, never> => ({ ok: true, value });
export const err = <E>(error: E): Result<never, E> => ({ ok: false, error });
