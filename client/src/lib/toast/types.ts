import type { Clock, Disposable } from '../core/types.js';
import type { Signal } from '../reactive/types.js';

export type ToastTone = 'info' | 'success' | 'error';

export interface Toast {
  /** Stable for the toast's whole life, so a rerender can reuse its node. */
  readonly id: number;
  readonly tone: ToastTone;
  readonly text: string;
}

export interface ShowOptions {
  readonly tone?: ToastTone;
  /** Milliseconds until it leaves on its own. `0` keeps it until dismissed. */
  readonly durationMs?: number;
}

/**
 * A queue of transient messages. Holds no DOM: what is on screen is derived from
 * {@link ToastQueue.items}, so the rule can be tested without a document.
 */
export interface ToastQueue extends Disposable {
  /** Oldest first. */
  readonly items: Signal<readonly Toast[]>;
  /** Returns the id, so a long-lived toast can be dismissed later. */
  show(text: string, options?: ShowOptions): number;
  dismiss(id: number): void;
  clear(): void;
}

export interface ToastQueueOptions {
  /** Injected so expiry can be driven by hand in tests. */
  readonly clock?: Clock;
  readonly defaultDurationMs?: number;
  /** Showing one more than this drops the oldest. */
  readonly limit?: number;
}

/** The region that draws a queue. Owned by whoever created it. */
export interface ToastHost extends Disposable {
  readonly element: HTMLElement;
}

export interface ToastHostOptions {
  /** Label read out when a toast appears. */
  readonly dismissLabel?: string;
}
