import { describe, expect, it } from '@jest/globals';
import { createManualClock, type ManualClock } from '../core/clock.js';
import { createToastQueue } from './create-toast-queue.js';
import type { ToastQueue } from './types.js';

function givenQueue(overrides: { limit?: number; defaultDurationMs?: number } = {}): {
  queue: ToastQueue;
  clock: ManualClock;
} {
  const clock = createManualClock();
  const queue = createToastQueue({ clock, defaultDurationMs: 1000, ...overrides });

  return { queue, clock };
}

const textsOf = (queue: ToastQueue): string[] => queue.items.peek().map((toast) => toast.text);

describe('토스트 큐', () => {
  describe('맥락: 메시지를 띄우는 경우', () => {
    it('given 메시지, when 띄우면, then 목록에 남는다', () => {
      const { queue } = givenQueue();

      queue.show('저장했습니다');

      expect(textsOf(queue)).toEqual(['저장했습니다']);
    });

    it('given 여러 메시지, when 띄우면, then 오래된 것이 앞에 온다', () => {
      const { queue } = givenQueue();

      queue.show('첫째');
      queue.show('둘째');

      expect(textsOf(queue)).toEqual(['첫째', '둘째']);
    });

    it('given 기본값, when 종류를 지정하지 않으면, then info 로 띄운다', () => {
      const { queue } = givenQueue();

      queue.show('안내');

      expect(queue.items.peek()[0]?.tone).toBe('info');
    });
  });

  describe('맥락: 시간이 지나 스스로 사라지는 경우', () => {
    it('given 표시 시간이 지난 토스트, when 시간이 흐르면, then 사라진다', () => {
      const { queue, clock } = givenQueue();
      queue.show('사라질 메시지');

      clock.advance(1000);

      expect(textsOf(queue)).toEqual([]);
    });

    it('given 표시 시간 전, when 시간이 조금만 흐르면, then 아직 남아 있다', () => {
      const { queue, clock } = givenQueue();
      queue.show('아직 있음');

      clock.advance(999);

      expect(textsOf(queue)).toEqual(['아직 있음']);
    });

    it('given 표시 시간이 0 인 토스트, when 오래 기다려도, then 사라지지 않는다', () => {
      const { queue, clock } = givenQueue();
      queue.show('직접 닫아야 함', { durationMs: 0 });

      clock.advance(60_000);

      expect(textsOf(queue)).toEqual(['직접 닫아야 함']);
    });

    it('given 표시 시간이 다른 두 토스트, when 짧은 쪽만 지나면, then 그것만 사라진다', () => {
      const { queue, clock } = givenQueue();
      queue.show('짧음', { durationMs: 500 });
      queue.show('김', { durationMs: 5000 });

      clock.advance(500);

      expect(textsOf(queue)).toEqual(['김']);
    });
  });

  describe('맥락: 한 번에 보여줄 수 있는 개수를 넘는 경우', () => {
    it('given 상한이 2 인 큐, when 셋을 띄우면, then 가장 오래된 것이 밀려난다', () => {
      const { queue } = givenQueue({ limit: 2 });

      queue.show('첫째');
      queue.show('둘째');
      queue.show('셋째');

      expect(textsOf(queue)).toEqual(['둘째', '셋째']);
    });

    it('given 밀려난 토스트, when 그 표시 시간이 지나면, then 남은 토스트는 그대로다', () => {
      const { queue, clock } = givenQueue({ limit: 1 });
      queue.show('밀려날 것', { durationMs: 500 });
      queue.show('남을 것', { durationMs: 5000 });

      clock.advance(500);

      expect(textsOf(queue)).toEqual(['남을 것']);
    });
  });

  describe('맥락: 직접 닫는 경우', () => {
    it('given 띄운 토스트, when 그 id 로 닫으면, then 그것만 사라진다', () => {
      const { queue } = givenQueue();
      const id = queue.show('닫을 것');
      queue.show('남을 것');

      queue.dismiss(id);

      expect(textsOf(queue)).toEqual(['남을 것']);
    });

    it('given 이미 닫힌 토스트, when 다시 닫아도, then 아무 일도 없다', () => {
      const { queue } = givenQueue();
      const id = queue.show('한 번만');
      queue.dismiss(id);

      queue.dismiss(id);

      expect(textsOf(queue)).toEqual([]);
    });

    it('given 여러 토스트, when 전부 비우면, then 목록이 빈다', () => {
      const { queue } = givenQueue();
      queue.show('첫째');
      queue.show('둘째');

      queue.clear();

      expect(textsOf(queue)).toEqual([]);
    });
  });

  describe('맥락: 화면이 정리된 뒤에 늦은 호출이 오는 경우', () => {
    it('given 정리된 큐, when 뒤늦게 띄우면, then 목록은 비어 있다', () => {
      const { queue } = givenQueue();
      queue.dispose();

      queue.show('늦은 메시지');

      expect(textsOf(queue)).toEqual([]);
    });

    it('given 대기 중인 토스트, when 큐를 정리하면, then 타이머가 남지 않는다', () => {
      const { queue, clock } = givenQueue();
      queue.show('정리될 것');

      queue.dispose();
      clock.advance(1000);

      expect(textsOf(queue)).toEqual([]);
    });
  });

  describe('맥락: 구독자가 변화를 지켜보는 경우', () => {
    it('given 구독자, when 토스트를 띄우면, then 새 목록을 받는다', () => {
      const { queue } = givenQueue();
      const seen: number[] = [];
      queue.items.subscribe((items) => seen.push(items.length));

      queue.show('알림');

      expect(seen).toEqual([1]);
    });
  });
});
