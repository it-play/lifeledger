import { describe, expect, it, jest } from '@jest/globals';
import { createStore } from './create-store.js';
import { diffPaths, getAtPath, pathsIntersect, setAtPath } from './paths.js';

/**
 * 대상: 경로 구독 스토어 (핵심 로직)
 * 구조: Data(상태 트리) — Context(무엇을 구독했는가) — Interaction(무엇이 바뀌면 누가 깨어나는가)
 */

interface TestState {
  readonly game: { readonly day: number; readonly cash: number };
  readonly ui: { readonly busy: boolean };
}

const givenState = (): TestState => ({ game: { day: 0, cash: 100 }, ui: { busy: false } });

/** 스토어는 알림을 마이크로태스크로 모으므로, 검증 전에 큐를 비운다. */
const flush = (): Promise<void> => Promise.resolve();

describe('상태 경로 유틸', () => {
  describe('맥락: 경로로 값을 읽는 경우', () => {
    it('given 존재하는 경로, when 읽으면, then 값을 돌려준다', () => {
      expect(getAtPath(givenState(), 'game.cash')).toBe(100);
    });

    it('given 없는 경로, when 읽으면, then undefined 를 돌려준다', () => {
      expect(getAtPath(givenState(), 'game.missing')).toBeUndefined();
    });
  });

  describe('맥락: 경로로 값을 쓰는 경우', () => {
    it('given 하위 경로 변경, when 쓰면, then 건드리지 않은 가지는 참조가 유지된다', () => {
      const state = givenState();

      const next = setAtPath(state, 'game.day', 5);

      expect(next.game.day).toBe(5);
      expect(next.ui).toBe(state.ui); // 구조 공유
      expect(next).not.toBe(state);
    });

    it('given 같은 값, when 쓰면, then 같은 참조를 돌려준다', () => {
      const state = givenState();

      expect(setAtPath(state, 'game.day', 0)).toBe(state);
    });
  });

  describe('맥락: 두 상태를 비교하는 경우', () => {
    it('given 한 값만 바뀐 상태, when 비교하면, then 바뀐 경로만 나온다', () => {
      const state = givenState();

      expect(diffPaths(state, setAtPath(state, 'game.day', 1))).toEqual(['game.day']);
    });

    it('given 상위·하위 경로 쌍, when 교차를 판단하면, then 서로 영향을 준다고 본다', () => {
      expect(pathsIntersect('game', 'game.day')).toBe(true);
      expect(pathsIntersect('game.day', 'game')).toBe(true);
    });

    it('given 형제 경로 쌍, when 교차를 판단하면, then 무관하다고 본다', () => {
      expect(pathsIntersect('game.day', 'game.cash')).toBe(false);
      expect(pathsIntersect('ui.busy', 'game.day')).toBe(false);
    });
  });
});

describe('스토어', () => {
  describe('맥락: 서로 다른 경로를 구독한 화면이 둘 있는 경우', () => {
    it('given day 와 busy 구독자, when day 를 바꾸면, then day 구독자만 깨어난다', async () => {
      const store = createStore(givenState());
      const onDay = jest.fn();
      const onBusy = jest.fn();
      store.watch('game.day', onDay);
      store.watch('ui.busy', onBusy);

      store.set('game.day', 3);
      await flush();

      expect(onDay).toHaveBeenCalledTimes(1);
      expect(onBusy).not.toHaveBeenCalled();
    });

    it('given 상위 경로 구독자, when 하위 값을 바꾸면, then 깨어난다', async () => {
      const store = createStore(givenState());
      const onGame = jest.fn();
      store.watch('game', onGame);

      store.set('game.cash', 1);
      await flush();

      expect(onGame).toHaveBeenCalledTimes(1);
    });
  });

  describe('맥락: 한 틱에 여러 번 갱신되는 경우 (배속 진행)', () => {
    it('given 연속 3회 갱신, when 마이크로태스크가 끝나면, then 알림은 1회로 합쳐진다', async () => {
      const store = createStore(givenState());
      const onGame = jest.fn();
      store.watch('game', onGame);

      store.set('game.day', 1);
      store.set('game.day', 2);
      store.set('game.cash', 50);
      expect(onGame).not.toHaveBeenCalled(); // flush 전에는 조용하다
      await flush();

      expect(onGame).toHaveBeenCalledTimes(1);
      expect(store.getState().game).toEqual({ day: 2, cash: 50 });
    });

    it('given batch 를 끈 스토어, when 갱신하면, then 즉시 알린다', () => {
      const store = createStore(givenState(), { batch: false });
      const onDay = jest.fn();
      store.watch('game.day', onDay);

      store.set('game.day', 1);

      expect(onDay).toHaveBeenCalledTimes(1);
    });
  });

  describe('맥락: 의미 없는 갱신이나 해제된 구독이 있는 경우', () => {
    it('given 같은 값 갱신, when 커밋하면, then 아무도 깨우지 않는다', async () => {
      const store = createStore(givenState());
      const onDay = jest.fn();
      store.watch('game.day', onDay);

      store.set('game.day', 0);
      await flush();

      expect(onDay).not.toHaveBeenCalled();
    });

    it('given 구독을 해제한 뒤, when 값을 바꾸면, then 호출되지 않는다', async () => {
      const store = createStore(givenState());
      const onDay = jest.fn();
      const off = store.watch('game.day', onDay);

      off();
      store.set('game.day', 9);
      await flush();

      expect(onDay).not.toHaveBeenCalled();
    });
  });

  describe('맥락: 전체 변경을 관찰하는 경우 (디버깅·영속화)', () => {
    it('given 두 가지를 동시에 바꾸는 갱신, when watchAll 이면, then 바뀐 경로 목록을 받는다', async () => {
      const store = createStore(givenState());
      const seen: string[][] = [];
      store.watchAll((_, changed) => seen.push([...changed]));

      store.update((s) => ({ ...s, game: { ...s.game, day: 1 }, ui: { busy: true } }));
      await flush();

      expect(seen[0]?.sort()).toEqual(['game.day', 'ui.busy']);
    });
  });
});
