import type { Clock, Disposable, Logger, Unsubscribe } from '../core/types.js';

/** One event, as the spec defines it. `type` is 'message' without an `event:` field. */
export interface SseMessage {
  readonly type: string;
  readonly data: string;
  readonly lastEventId: string;
}

/**
 * An incremental parser: feed it chunks in order and it returns completed events.
 * Pure logic, so it can be tested without a network.
 */
export interface EventStreamParser {
  /** Feeds a chunk and returns the events it completed, in order. */
  push(chunk: string): readonly SseMessage[];
  /** Called at end of stream; per spec, incomplete data is discarded. */
  end(): void;
  /** Sent as `Last-Event-ID` on reconnect. Survives dispatch. */
  readonly lastEventId: string;
  /** Reconnect delay in ms from a server `retry:` field, if one arrived. */
  readonly serverRetryMs: number | undefined;
}

export type SseStatus = 'idle' | 'connecting' | 'open' | 'reconnecting' | 'closed';

/** Why a connection ended; the input to the retry decision. */
export type DisconnectReason =
  | { readonly kind: 'network'; readonly error: unknown }
  | { readonly kind: 'http'; readonly status: number }
  | { readonly kind: 'bad-content-type'; readonly contentType: string | null }
  | { readonly kind: 'stream-ended' }
  | { readonly kind: 'closed-by-caller' };

/** Whether to reconnect. The default lives in policy.ts and can be injected over. */
export interface RetryDecider {
  shouldRetry(reason: DisconnectReason): boolean;
}

/** Reconnect delay. `attempt` starts at 1. */
export interface BackoffPolicy {
  delayMs(attempt: number, serverRetryMs: number | undefined): number;
}

export interface SseClientOptions {
  readonly url: string;
  /** Default headers. Accept and Last-Event-ID are managed by the client. */
  readonly headers?: Readonly<Record<string, string>>;
  readonly credentials?: RequestCredentials;
  readonly backoff?: BackoffPolicy;
  readonly retryDecider?: RetryDecider;
  readonly clock?: Clock;
  readonly logger?: Logger;
  /** Seam for substituting fetch in tests. */
  readonly fetchImpl?: typeof fetch;
}

/**
 * Manages one SSE connection.
 *
 * EventSource is not used: it cannot set custom headers, cannot control the reconnect
 * policy, and makes state transitions hard to observe.
 */
export interface SseClient extends Disposable {
  readonly status: SseStatus;
  readonly lastEventId: string;
  /** Subscribes by event type, named by the server's `event:` value. */
  on(type: string, handler: (message: SseMessage) => void): Unsubscribe;
  /** Subscribes to every event. */
  onAny(handler: (message: SseMessage) => void): Unsubscribe;
  onStatusChange(handler: (status: SseStatus) => void): Unsubscribe;
  /** Opens the connection; a no-op when already connecting. */
  connect(): void;
  /** Closes the connection without reconnecting. */
  close(): void;
}
