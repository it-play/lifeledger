import { describe, expect, it } from '@jest/globals';
import { WelfareCommandError } from '../../api/welfare-api.js';
import { createWelfareApplicationRetryPolicy } from './create-welfare-retry-policy.js';
import type { WelfareCommandCursorSource } from './types.js';

describe('복지 신청 재시도 판단', () => {
  describe('맥락: 신청 응답을 확인하지 못한 경우', () => {
    it('given 전송 오류, when 같은 프로그램을 다시 선택하면, then 원래 명령과 cursor를 재사용한다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000001');
      const policy = createWelfareApplicationRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(3, 42, 17), '71');
      policy.fail(original, new Error('connection lost'));

      const retried = policy.select(givenCursor(3, 43, 18), '71');

      expect(retried).toEqual(original);
      expect(policy.pending(3, '71')).toEqual(original);
      expect(ids.count()).toBe(1);
    });
  });

  describe('맥락: 서버가 신청을 확정적으로 거절한 경우', () => {
    it('given ineligible 오류, when 같은 프로그램을 다시 선택하면, then 최신 cursor로 새 명령을 만든다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000001',
        '00000000-0000-4000-8000-000000000002',
      );
      const policy = createWelfareApplicationRetryPolicy({ createCommandId: ids.next });
      const rejected = policy.select(givenCursor(3, 42, 17), '71');
      policy.fail(rejected, new WelfareCommandError('ineligible', '신청할 수 없습니다'));

      const next = policy.select(givenCursor(3, 43, 18), '71');

      expect(next.commandId).toBe('00000000-0000-4000-8000-000000000002');
      expect(next.expectedStateRevision).toBe(43);
      expect(next.expectedGameDay).toBe(18);
    });
  });

  describe('맥락: 신청 결과를 확인한 경우', () => {
    it('given 완료된 명령, when 같은 프로그램을 다시 선택하면, then 새 명령을 만든다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000001',
        '00000000-0000-4000-8000-000000000002',
      );
      const policy = createWelfareApplicationRetryPolicy({ createCommandId: ids.next });
      const completed = policy.select(givenCursor(3, 42, 17), '71');
      policy.complete(completed);

      const next = policy.select(givenCursor(3, 43, 18), '71');

      expect(next.commandId).toBe('00000000-0000-4000-8000-000000000002');
      expect(policy.pending(3, '71')).toBeUndefined();
    });
  });
});

function givenCursor(
  runRevision: number,
  stateRevision: number,
  gameDay: number,
): WelfareCommandCursorSource {
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
