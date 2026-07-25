/**
 * The minimal contracts shared across the library. Nothing here carries an
 * implementation; each module supplies its own.
 */

/** Unsubscribe. Must be safe to call twice. */
export type Unsubscribe = () => void;

/** Something holding a resource that needs releasing. */
export interface Disposable {
  dispose(): void;
}

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

/**
 * Where logs go. The library never calls `console` directly, so tests can collect logs
 * and production can point them at a remote sink.
 */
export interface Logger {
  log(level: LogLevel, message: string, context?: Readonly<Record<string, unknown>>): void;
  child(scope: string): Logger;
}

/**
 * Time, injected so that reconnect delays and timeouts can be verified
 * deterministically in tests.
 */
export interface Clock {
  now(): number;
  setTimeout(handler: () => void, delayMs: number): CancelTimer;
}

export type CancelTimer = () => void;

/** For boundaries (parsing, network) that prefer errors as values. */
export type Result<T, E = Error> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: E };

export const ok = <T>(value: T): Result<T, never> => ({ ok: true, value });
export const err = <E>(error: E): Result<never, E> => ({ ok: false, error });
