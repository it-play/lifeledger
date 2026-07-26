import uPlot from 'uplot';
import type { Unsubscribe } from '../core/index.js';
import type { CloseChart, CloseChartDeps, CloseChartPoint } from './types.js';

const CHART_HEIGHT = 320;
const FALLBACK_WIDTH = 640;

/** Creates a one-series uPlot chart and ties all browser resources to the caller's bag. */
export function createCloseChart(deps: CloseChartDeps): CloseChart {
  const chart = new uPlot(
    {
      width: measuredWidth(deps.host),
      height: CHART_HEIGHT,
      series: [{ label: '날짜' }, { label: '종가', stroke: getComputedStyle(deps.host).color }],
    },
    [[], []],
    deps.host,
  );
  deps.bag.add({ dispose: () => chart.destroy() });

  const resize = (): void => {
    const width = measuredWidth(deps.host);
    if (width !== chart.width) chart.setSize({ width, height: CHART_HEIGHT });
  };
  const stopObserving = (deps.observeResize ?? observeElementResize)(deps.host, resize);
  deps.bag.add(stopObserving);

  return {
    setPoints(points) {
      chart.setData(toAlignedData(points));
    },
  };
}

function measuredWidth(host: HTMLElement): number {
  const width = host.clientWidth || host.parentElement?.clientWidth || FALLBACK_WIDTH;
  return Math.max(1, Math.floor(width));
}

function toAlignedData(points: readonly CloseChartPoint[]): uPlot.AlignedData {
  return [points.map((point) => point.timestampSeconds), points.map((point) => point.value)];
}

function observeElementResize(element: HTMLElement, onResize: () => void): Unsubscribe {
  if (typeof ResizeObserver === 'function') {
    const observer = new ResizeObserver(onResize);
    observer.observe(element);
    return () => observer.disconnect();
  }

  globalThis.addEventListener('resize', onResize);
  return () => globalThis.removeEventListener('resize', onResize);
}
