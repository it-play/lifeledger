import type { Clock } from './types.js';

/** Browser-backed default. */
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
  /** Runs every expired timer, in order. */
  advance(ms: number): void;
  readonly pendingCount: number;
}

/** For tests: advance time by hand to verify reconnect scheduling deterministically. */
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
