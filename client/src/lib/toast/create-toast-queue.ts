import { createSystemClock } from '../core/clock.js';
import type { CancelTimer } from '../core/types.js';
import { createSignal } from '../reactive/signal.js';
import type { ShowOptions, Toast, ToastQueue, ToastQueueOptions } from './types.js';

const DEFAULT_DURATION_MS = 4000;
/** Past this the newest messages push the oldest out rather than stacking off-screen. */
const DEFAULT_LIMIT = 3;

export function createToastQueue(options: ToastQueueOptions = {}): ToastQueue {
  const clock = options.clock ?? createSystemClock();
  const defaultDurationMs = options.defaultDurationMs ?? DEFAULT_DURATION_MS;
  const limit = Math.max(1, options.limit ?? DEFAULT_LIMIT);

  const items = createSignal<readonly Toast[]>([]);
  const timers = new Map<number, CancelTimer>();
  let nextId = 1;
  let disposed = false;

  function stopTimer(id: number): void {
    timers.get(id)?.();
    timers.delete(id);
  }

  function stopAllTimers(): void {
    for (const cancel of timers.values()) cancel();
    timers.clear();
  }

  function remove(id: number): void {
    stopTimer(id);
    const current = items.peek();
    const remaining = current.filter((toast) => toast.id !== id);
    if (remaining.length !== current.length) items.set(remaining);
  }

  return {
    items,

    show(text, showOptions: ShowOptions = {}) {
      const id = nextId++;
      // A late callback must not revive the queue after teardown
      if (disposed) return id;

      const toast: Toast = { id, tone: showOptions.tone ?? 'info', text };
      const previous = items.peek();
      // Trim from the front: the oldest message is the one the reader has had longest
      const kept = [...previous, toast].slice(-limit);
      for (const dropped of previous) {
        if (!kept.includes(dropped)) stopTimer(dropped.id);
      }
      items.set(kept);

      const durationMs = showOptions.durationMs ?? defaultDurationMs;
      if (durationMs > 0)
        timers.set(
          id,
          clock.setTimeout(() => remove(id), durationMs),
        );

      return id;
    },

    dismiss: remove,

    clear() {
      stopAllTimers();
      if (items.peek().length > 0) items.set([]);
    },

    dispose() {
      disposed = true;
      stopAllTimers();
      items.set([]);
    },
  };
}
