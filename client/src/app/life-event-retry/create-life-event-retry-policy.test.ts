import { describe, expect, it } from '@jest/globals';
import { LifeEventCommandError } from '../../api/life-event-api.js';
import { createLifeEventChoiceRetryPolicy } from './create-life-event-retry-policy.js';
import type { LifeEventCommandCursorSource } from './types.js';

describe('생애 사건 선택 재시도 판단', () => {
  describe('맥락: 선택 응답을 확인하지 못한 경우', () => {
    it('given 전송 오류, when 다른 선택지를 누르면, then 최초 path와 body를 그대로 재사용한다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000001');
      const policy = createLifeEventChoiceRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(3, 42, 17), '71', '81');
      policy.fail(original, new Error('connection lost'));

      const retried = policy.select(givenCursor(3, 43, 18), '71', '82');

      expect(retried).toEqual(original);
      expect(policy.pending(3, '71')).toEqual(original);
      expect(ids.count()).toBe(1);
    });
  });

  describe('맥락: 서버가 선택을 확정적으로 거절한 경우', () => {
    it('given eventExpired 오류, when 다시 선택하면, then 최신 cursor로 새 명령을 만든다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000001',
        '00000000-0000-4000-8000-000000000002',
      );
      const policy = createLifeEventChoiceRetryPolicy({ createCommandId: ids.next });
      const rejected = policy.select(givenCursor(3, 42, 17), '71', '81');
      policy.fail(rejected, new LifeEventCommandError('eventExpired', '기한이 지났습니다'));

      const next = policy.select(givenCursor(3, 43, 18), '71', '82');

      expect(next.request.commandId).toBe('00000000-0000-4000-8000-000000000002');
      expect(next.request.choiceId).toBe('82');
      expect(next.request.expectedStateRevision).toBe(43);
    });
  });

  describe('맥락: 선택 결과를 확인한 경우', () => {
    it('given 완료된 명령, when 다시 선택하면, then 새 명령을 만든다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000001',
        '00000000-0000-4000-8000-000000000002',
      );
      const policy = createLifeEventChoiceRetryPolicy({ createCommandId: ids.next });
      const completed = policy.select(givenCursor(3, 42, 17), '71', '81');
      policy.complete(completed);

      const next = policy.select(givenCursor(3, 43, 18), '71', '82');

      expect(next.request.commandId).toBe('00000000-0000-4000-8000-000000000002');
      expect(policy.pending(3, '71')).toBeUndefined();
    });
  });
});

function givenCursor(
  runRevision: number,
  stateRevision: number,
  gameDay: number,
): LifeEventCommandCursorSource {
  return { runRevision, stateRevision, gameDay };
}

function givenCommandIds(...ids: readonly string[]): {
  readonly next: () => string;
  readonly count: () => number;
} {
  let index = 0;
  return {
    next() {
      const id = ids[index];
      if (id === undefined) throw new Error('명령 ID fixture가 부족합니다.');
      index += 1;
      return id;
    },
    count: () => index,
  };
}
