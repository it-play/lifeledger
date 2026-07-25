import type { BackoffPolicy, DisconnectReason, RetryDecider } from './types.js';

export interface ExponentialBackoffOptions {
  /** 첫 재시도 지연. 서버가 retry: 를 보내면 그 값이 우선한다. */
  readonly baseMs?: number;
  readonly maxMs?: number;
  readonly factor?: number;
  /** 0 이면 지터 없음, 0.5 면 ±50%. 여러 클라이언트가 동시에 몰리는 것을 막는다. */
  readonly jitterRatio?: number;
  /** 테스트용 난수 주입. */
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
      // [1-r, 1+r) 범위로 흔든다
      const spread = capped * jitterRatio;
      const jittered = capped - spread + random() * spread * 2;
      return Math.round(Math.max(0, jittered));
    },
  };
}

/**
 * 기본 재시도 판단.
 *  - 네트워크 오류·스트림 정상 종료 → 재시도 (SSE 는 끊기면 다시 붙는 것이 정상)
 *  - 5xx, 408, 425, 429 → 재시도 (일시적)
 *  - 그 밖의 4xx → 재시도 안 함 (인증 실패·잘못된 경로를 계속 두드릴 이유가 없다)
 *  - 204 → 재시도 안 함 (서버가 "더 보낼 것 없다"고 알린 것)
 *  - Content-Type 불일치 → 재시도 안 함 (설정 오류이므로 재시도해도 같다)
 *  - 호출자가 닫음 → 재시도 안 함
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
