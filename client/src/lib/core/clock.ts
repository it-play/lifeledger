import type { Clock } from './types.js';

/** 브라우저 기본 구현. */
export function createSystemClock(): Clock {
  return {
    now: () => Date.now(),
    setTimeout: (handler, delayMs) => {
      const id = globalThis.setTimeout(handler, delayMs);
      return () => globalThis.clearTimeout(id);
    },
  };
}

export interface ManualClock extends Clock {
  /** 등록된 타이머 중 만료된 것을 순서대로 실행한다. */
  advance(ms: number): void;
  readonly pendingCount: number;
}

/** 테스트용. 시간을 직접 밀어 재연결 스케줄을 결정론적으로 검증한다. */
export function createManualClock(startMs = 0): ManualClock {
  let current = startMs;
  let seq = 0;
  const timers = new Map<number, { at: number; handler: () => void }>();

  return {
    now: () => current,
    setTimeout(handler, delayMs) {
      const id = seq++;
      timers.set(id, { at: current + delayMs, handler });
      return () => {
        timers.delete(id);
      };
    },
    advance(ms) {
      const target = current + ms;
      for (;;) {
        const due = [...timers.entries()]
          .filter(([, t]) => t.at <= target)
          .sort((a, b) => a[1].at - b[1].at);
        const next = due[0];
        if (next === undefined) break;
        const [id, timer] = next;
        timers.delete(id);
        current = timer.at;
        timer.handler();
      }
      current = target;
    },
    get pendingCount() {
      return timers.size;
    },
  };
}
