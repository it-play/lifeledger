import type { DisposableBag, Unsubscribe } from '../core/index.js';

/** One generic time-series value. Domain-specific history is adapted before it gets here. */
export interface CloseChartPoint {
  readonly timestampSeconds: number;
  readonly value: number;
}

export interface CloseChart {
  setPoints(points: readonly CloseChartPoint[]): void;
}

export interface CloseChartDeps {
  readonly host: HTMLElement;
  readonly bag: DisposableBag;
  /** Injectable because observation owns a browser resource that must be released. */
  readonly observeResize?: (element: HTMLElement, onResize: () => void) => Unsubscribe;
}
