import { describe, expect, it, jest } from '@jest/globals';
import { createManualClock, type ManualClock } from '../core/clock.js';
import { createDisposableBag, type DisposableBag } from '../core/disposable.js';
import { createStore } from '../store/create-store.js';
import { createHooks } from './create-hooks.js';
import type { Hooks } from './types.js';

/**
 * 대상: 생명주기에 묶인 훅 (서비스 로직)
 * 구조: Data(훅이 만든 자원) — Context(시간·상태가 어떻게 흐르는가) — Interaction(언제 실행되고 언제 해제되는가)
 *
 * DOM·window 에 붙는 훅(useEventListener·useMediaQuery 등)은 테스트 정책상 대상이 아니다.
 */

interface Harness {
  readonly hooks: Hooks;
  readonly clock: ManualClock;
  readonly bag: DisposableBag;
}

function givenHooks(): Harness {
  const clock = createManualClock();
  const bag = createDisposableBag();
  return { hooks: createHooks(bag, { clock }), clock, bag };
}

describe('useSignal / useComputed', () => {
  describe('맥락: 화면 지역 상태를 다루는 경우', () => {
    it('given 지역 신호, when 값을 바꾸면, then 파생값도 따라온다', () => {
      const { hooks } = givenHooks();
      const amount = hooks.useSignal(1000);
      const doubled = hooks.useComputed(() => amount.get() * 2);

      amount.set(2500);

      expect(doubled.get()).toBe(5000);
    });
  });
});

describe('useEffect', () => {
  describe('맥락: bag 이 정리되는 경우', () => {
    it('given effect 를 등록한 뒤, when bag 을 dispose 하면, then 더 이상 실행되지 않는다', () => {
      const { hooks, bag } = givenHooks();
      const value = hooks.useSignal(0);
      const run = jest.fn();
      hooks.useEffect(() => {
        value.get();
        run();
      });
      run.mockClear();

      bag.dispose();
      value.set(1);

      expect(run).not.toHaveBeenCalled();
    });
  });
});

describe('useWatch', () => {
  describe('맥락: 값 변화에만 반응해야 하는 경우', () => {
    it('given 감시 중인 신호, when 처음 등록하면, then 콜백은 호출되지 않는다', () => {
      const { hooks } = givenHooks();
      const visible = hooks.useSignal(true);
      const onChange = jest.fn();

      hooks.useWatch(visible, onChange);

      expect(onChange).not.toHaveBeenCalled();
    });

    it('given 감시 중인 신호, when 값이 바뀌면, then 새 값과 이전 값을 받는다', () => {
      const { hooks } = givenHooks();
      const visible = hooks.useSignal(true);
      const onChange = jest.fn();
      hooks.useWatch(visible, onChange);

      visible.set(false);

      expect(onChange).toHaveBeenCalledWith(false, true);
    });
  });
});

describe('useStoreValue', () => {
  describe('맥락: 스토어의 한 경로만 화면에 끌어오는 경우', () => {
    it('given 경로 신호, when 그 경로가 바뀌면, then 신호가 갱신된다', async () => {
      const { hooks } = givenHooks();
      const store = createStore({ game: { day: 0 }, ui: { busy: false } });
      const day = hooks.useStoreValue(store, 'game.day', (s) => s.game.day);

      store.set('game.day', 7);
      await Promise.resolve();

      expect(day.get()).toBe(7);
    });

    it('given 경로 신호, when 무관한 경로가 바뀌면, then 값이 그대로다', async () => {
      const { hooks } = givenHooks();
      const store = createStore({ game: { day: 0 }, ui: { busy: false } });
      const day = hooks.useStoreValue(store, 'game.day', (s) => s.game.day);
      const listener = jest.fn();
      day.subscribe(listener);

      store.set('ui.busy', true);
      await Promise.resolve();

      expect(listener).not.toHaveBeenCalled();
    });
  });
});

describe('useInterval / useTimeout', () => {
  describe('맥락: 주기·지연 실행이 필요한 경우', () => {
    it('given 100ms 주기, when 350ms 흐르면, then 세 번 실행된다', () => {
      const { hooks, clock } = givenHooks();
      const tick = jest.fn();
      hooks.useInterval(tick, 100);

      clock.advance(350);

      expect(tick).toHaveBeenCalledTimes(3);
    });

    it('given 주기 실행 중, when bag 을 dispose 하면, then 더 이상 실행되지 않는다', () => {
      const { hooks, clock, bag } = givenHooks();
      const tick = jest.fn();
      hooks.useInterval(tick, 100);

      clock.advance(100);
      bag.dispose();
      clock.advance(500);

      expect(tick).toHaveBeenCalledTimes(1);
    });

    it('given 지연 실행, when 시간이 되면, then 한 번 실행된다', () => {
      const { hooks, clock } = givenHooks();
      const run = jest.fn();
      hooks.useTimeout(run, 50);

      clock.advance(50);

      expect(run).toHaveBeenCalledTimes(1);
    });
  });
});

describe('useDebounced', () => {
  describe('맥락: 검색 입력처럼 연달아 호출되는 경우', () => {
    it('given 연속 3회 호출, when 대기시간이 지나면, then 마지막 인자로 한 번만 실행된다', () => {
      const { hooks, clock } = givenHooks();
      const search = jest.fn();
      const debounced = hooks.useDebounced(search, 200);

      debounced('원');
      debounced('원티');
      debounced('원티드');
      clock.advance(200);

      expect(search).toHaveBeenCalledTimes(1);
      expect(search).toHaveBeenCalledWith('원티드');
    });

    it('given 대기 중, when cancel 하면, then 실행되지 않는다', () => {
      const { hooks, clock } = givenHooks();
      const search = jest.fn();
      const debounced = hooks.useDebounced(search, 200);

      debounced('x');
      debounced.cancel();
      clock.advance(500);

      expect(search).not.toHaveBeenCalled();
    });
  });
});

describe('useThrottled', () => {
  describe('맥락: 배속 진행처럼 알림이 쏟아지는 경우', () => {
    it('given 첫 호출, when 즉시 호출되면, then 바로 통과한다', () => {
      const { hooks } = givenHooks();
      const render = jest.fn();
      const throttled = hooks.useThrottled(render, 100);

      throttled(1);

      expect(render).toHaveBeenCalledTimes(1);
    });

    it('given 간격 안의 연속 호출, when 간격이 끝나면, then 마지막 것만 한 번 더 실행된다', () => {
      const { hooks, clock } = givenHooks();
      const render = jest.fn();
      const throttled = hooks.useThrottled(render, 100);

      throttled(1);
      throttled(2);
      throttled(3);
      clock.advance(100);

      expect(render).toHaveBeenCalledTimes(2);
      expect(render).toHaveBeenLastCalledWith(3);
    });
  });
});

describe('useAsync', () => {
  describe('맥락: 서버 호출의 진행 상태를 화면에 보여주는 경우', () => {
    it('given 성공하는 작업, when 실행하면, then loading 을 거쳐 success 가 된다', async () => {
      const { hooks } = givenHooks();
      const handle = hooks.useAsync(async () => 'ok');

      handle.run();
      expect(handle.state.get()).toEqual({ status: 'loading' });
      await Promise.resolve();

      expect(handle.state.get()).toEqual({ status: 'success', value: 'ok' });
    });

    it('given 실패하는 작업, when 실행하면, then error 상태가 된다', async () => {
      const { hooks } = givenHooks();
      const failure = new Error('boom');
      const handle = hooks.useAsync(() => Promise.reject(failure));

      handle.run();
      await Promise.resolve();
      await Promise.resolve();

      expect(handle.state.get()).toEqual({ status: 'error', error: failure });
    });

    it('given 진행 중인 작업, when cancel 하면, then 결과가 상태에 반영되지 않는다', async () => {
      const { hooks } = givenHooks();
      const handle = hooks.useAsync(async () => 'late');

      handle.run();
      handle.cancel();
      await Promise.resolve();
      await Promise.resolve();

      expect(handle.state.get()).toEqual({ status: 'loading' });
    });

    it('given 작업에 넘긴 signal, when 재실행하면, then 이전 요청이 취소된다', () => {
      const { hooks } = givenHooks();
      const signals: AbortSignal[] = [];
      const handle = hooks.useAsync(async (signal) => {
        signals.push(signal);
        return 1;
      });

      handle.run();
      handle.run();

      expect(signals[0]?.aborted).toBe(true);
      expect(signals[1]?.aborted).toBe(false);
    });
  });
});
