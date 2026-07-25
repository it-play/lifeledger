import { describe, expect, it, jest } from '@jest/globals';
import { batch, createComputed, createEffect, createSignal, untracked } from './signal.js';

/**
 * 대상: 반응성 코어 (핵심 로직)
 * 구조: Data(신호 값) — Context(누가 그 값을 읽었는가) — Interaction(값이 바뀌면 무엇이 다시 도는가)
 */

describe('signal', () => {
  describe('맥락: 값을 읽고 쓰는 경우', () => {
    it('given 초기값, when 읽으면, then 그 값이 나온다', () => {
      const count = createSignal(1);

      expect(count.get()).toBe(1);
    });

    it('given 구독자, when 값을 바꾸면, then 새 값으로 호출된다', () => {
      const count = createSignal(1);
      const listener = jest.fn();
      count.subscribe(listener);

      count.set(2);

      expect(listener).toHaveBeenCalledWith(2);
    });

    it('given 같은 값, when 다시 쓰면, then 구독자를 깨우지 않는다', () => {
      const count = createSignal(1);
      const listener = jest.fn();
      count.subscribe(listener);

      count.set(1);

      expect(listener).not.toHaveBeenCalled();
    });

    it('given equals 를 지정한 신호, when 동등한 값을 쓰면, then 알리지 않는다', () => {
      const point = createSignal({ x: 1 }, { equals: (a, b) => a.x === b.x });
      const listener = jest.fn();
      point.subscribe(listener);

      point.set({ x: 1 });

      expect(listener).not.toHaveBeenCalled();
    });
  });
});

describe('computed', () => {
  describe('맥락: 여러 신호에서 값을 파생하는 경우', () => {
    it('given 두 신호의 합, when 하나가 바뀌면, then 다시 계산된다', () => {
      const cash = createSignal(100);
      const stocks = createSignal(50);
      const netWorth = createComputed(() => cash.get() + stocks.get());

      cash.set(200);

      expect(netWorth.get()).toBe(250);
    });

    it('given 계산 결과가 같아지는 변경, when 의존성이 바뀌면, then 구독자를 깨우지 않는다', () => {
      const cash = createSignal(100);
      const debt = createSignal(0);
      const netWorth = createComputed(() => cash.get() - debt.get());
      const listener = jest.fn();
      netWorth.subscribe(listener);

      // 100 - 0 → 200 - 100 : 결과는 그대로 100
      batch(() => {
        cash.set(200);
        debt.set(100);
      });

      expect(netWorth.get()).toBe(100);
      expect(listener).not.toHaveBeenCalled();
    });

    it('given computed 를 읽는 computed, when 밑단이 바뀌면, then 위까지 전파된다', () => {
      const base = createSignal(2);
      const doubled = createComputed(() => base.get() * 2);
      const quadrupled = createComputed(() => doubled.get() * 2);

      base.set(3);

      expect(quadrupled.get()).toBe(12);
    });
  });
});

describe('effect', () => {
  describe('맥락: 읽은 신호를 자동으로 의존성으로 잡는 경우', () => {
    it('given effect, when 생성되면, then 즉시 한 번 실행된다', () => {
      const count = createSignal(0);
      const run = jest.fn();

      createEffect(() => {
        count.get();
        run();
      });

      expect(run).toHaveBeenCalledTimes(1);
    });

    it('given 읽은 신호가 바뀌면, when 알림이 오면, then 다시 실행된다', () => {
      const count = createSignal(0);
      const seen: number[] = [];
      createEffect(() => {
        seen.push(count.get());
      });

      count.set(1);
      count.set(2);

      expect(seen).toEqual([0, 1, 2]);
    });

    it('given untracked 로 읽은 신호, when 그 값이 바뀌면, then 다시 실행되지 않는다', () => {
      const tracked = createSignal(0);
      const hidden = createSignal(0);
      const run = jest.fn();
      createEffect(() => {
        tracked.get();
        untracked(() => hidden.get());
        run();
      });

      hidden.set(1);

      expect(run).toHaveBeenCalledTimes(1);
    });
  });

  describe('맥락: 정리 함수를 반환하는 경우', () => {
    it('given 정리 함수, when 다시 실행되면, then 이전 정리가 먼저 호출된다', () => {
      const count = createSignal(0);
      const cleanup = jest.fn();
      createEffect(() => {
        count.get();
        return cleanup;
      });

      count.set(1);

      expect(cleanup).toHaveBeenCalledTimes(1);
    });

    it('given dispose 한 effect, when 신호가 바뀌면, then 실행되지 않고 정리가 끝나 있다', () => {
      const count = createSignal(0);
      const cleanup = jest.fn();
      const run = jest.fn();
      const handle = createEffect(() => {
        count.get();
        run();
        return cleanup;
      });

      handle.dispose();
      count.set(1);

      expect(cleanup).toHaveBeenCalledTimes(1);
      expect(run).toHaveBeenCalledTimes(1);
    });
  });
});

describe('batch', () => {
  describe('맥락: 한 번에 여러 값을 바꾸는 경우 (배속 진행 정산)', () => {
    it('given 묶인 변경 3건, when batch 가 끝나면, then effect 는 한 번만 다시 돈다', () => {
      const day = createSignal(0);
      const cash = createSignal(0);
      const run = jest.fn();
      createEffect(() => {
        day.get();
        cash.get();
        run();
      });
      run.mockClear();

      batch(() => {
        day.set(1);
        cash.set(10);
        day.set(2);
      });

      expect(run).toHaveBeenCalledTimes(1);
    });
  });
});
