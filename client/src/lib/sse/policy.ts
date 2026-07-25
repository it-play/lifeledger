import type { BackoffPolicy, DisconnectReason, RetryDecider } from './types.js';

export interface ExponentialBackoffOptions {
  /** First retry delay. A server-sent `retry:` takes precedence. */
  readonly baseMs?: number;
  readonly maxMs?: number;
  readonly factor?: number;
  /** 0 disables jitter, 0.5 means +/-50%. Keeps clients from reconnecting in lockstep. */
  readonly jitterRatio?: number;
  /** Injectable randomness, for tests. */
  readonly random?: () => number;
}

export function createExponentialBackoff(options: ExponentialBackoffOptions = {}): BackoffPolicy {
  const baseMs = options.baseMs ?? 1_000;
  const maxMs = options.maxMs ?? 30_000;
  const factor = options.factor ?? 2;
  const jitterRatio = options.jitterRatio ?? 0.2;
  const random = options.random ?? Math.random;

  return {
    delayMs(attempt, serverRetryMs) {
      const start = serverRetryMs ?? baseMs;
      const raw = start * factor ** Math.max(0, attempt - 1);
      const capped = Math.min(raw, maxMs);
      if (jitterRatio === 0) return Math.round(capped);
      // Spread across [1-r, 1+r)
      const spread = capped * jitterRatio;
      const jittered = capped - spread + random() * spread * 2;
      return Math.round(Math.max(0, jittered));
    },
  };
}

/**
 * The default retry decision.
 *  - network error or clean stream end: retry, since reconnecting is normal for SSE
 *  - 5xx, 408, 425, 429: retry, as these are transient
 *  - any other 4xx: no retry; there is no point hammering a bad path or a failed auth
 *  - 204: no retry, the server said there is nothing more to send
 *  - Content-Type mismatch: no retry, a config error repeats identically
 *  - closed by the caller: no retry
 */
export function createDefaultRetryDecider(): RetryDecider {
  const transientStatuses = new Set([408, 425, 429]);
  return {
    shouldRetry(reason: DisconnectReason) {
      switch (reason.kind) {
        case 'network':
        case 'stream-ended':
          return true;
        case 'http':
          if (reason.status === 204) return false;
          if (reason.status >= 500) return true;
          return transientStatuses.has(reason.status);
        case 'bad-content-type':
        case 'closed-by-caller':
          return false;
      }
    },
  };
}
