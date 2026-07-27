import { describe, expect, it } from '@jest/globals';
import { InsuranceCommandError } from '../../api/insurance-api.js';
import { createInsuranceRetryPolicy } from './create-insurance-retry-policy.js';
import type { InsuranceCommandCursorSource } from './types.js';

describe('보험 명령 재시도 판단', () => {
  describe('맥락: 가입 응답을 확인하지 못한 경우', () => {
    it('given 전송 오류, when 최신 snapshot에서 다시 가입하면, then 최초 body를 그대로 재사용한다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000001');
      const policy = createInsuranceRetryPolicy({ createCommandId: ids.next });
      const original = policy.enroll(givenCursor(3, 42, 17), '71');
      policy.fail(original, new Error('connection lost'));

      const retried = policy.enroll(givenCursor(3, 45, 20), '71');

      expect(retried).toEqual(original);
      expect(policy.pendingEnrollment(3, '71')).toEqual(original);
      expect(ids.count()).toBe(1);
    });
  });

  describe('맥락: 취소 응답을 확인하지 못한 경우', () => {
    it('given 전송 오류, when 다시 취소하면, then 최초 path contract ID와 body를 그대로 재사용한다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000001');
      const policy = createInsuranceRetryPolicy({ createCommandId: ids.next });
      const original = policy.cancel(givenCursor(3, 42, 17), '91');
      policy.fail(original, new Error('connection lost'));

      const retried = policy.cancel(givenCursor(3, 46, 21), '91');

      expect(retried).toEqual(original);
      expect(retried.contractId).toBe('91');
      expect(ids.count()).toBe(1);
    });
  });

  describe('맥락: 청구 응답을 확인하지 못한 경우', () => {
    it('given 전송 오류, when 다시 청구하면, then 최초 claim ID와 body를 그대로 재사용한다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000001');
      const policy = createInsuranceRetryPolicy({ createCommandId: ids.next });
      const original = policy.claim(givenCursor(3, 42, 17), '101');
      policy.fail(original, new Error('connection lost'));

      const retried = policy.claim(givenCursor(3, 47, 22), '101');

      expect(retried).toEqual(original);
      expect(retried.request.claimId).toBe('101');
      expect(ids.count()).toBe(1);
    });
  });

  describe('맥락: 서버가 명령을 확정적으로 거절한 경우', () => {
    it('given contractConflict, when 다시 취소하면, then 최신 cursor로 새 명령을 만든다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000001',
        '00000000-0000-4000-8000-000000000002',
      );
      const policy = createInsuranceRetryPolicy({ createCommandId: ids.next });
      const rejected = policy.cancel(givenCursor(3, 42, 17), '91');
      policy.fail(rejected, new InsuranceCommandError('contractConflict', '상태가 바뀌었습니다'));

      const next = policy.cancel(givenCursor(3, 43, 18), '91');

      expect(next.request.commandId).toBe('00000000-0000-4000-8000-000000000002');
      expect(next.request.expectedStateRevision).toBe(43);
    });
  });

  describe('맥락: 명령 결과를 확인한 경우', () => {
    it('given 완료된 claim, when 다시 청구하면, then 새 command ID를 발급한다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000001',
        '00000000-0000-4000-8000-000000000002',
      );
      const policy = createInsuranceRetryPolicy({ createCommandId: ids.next });
      const completed = policy.claim(givenCursor(3, 42, 17), '101');
      policy.complete(completed);

      const next = policy.claim(givenCursor(3, 43, 18), '101');

      expect(next.request.commandId).toBe('00000000-0000-4000-8000-000000000002');
      expect(policy.pendingClaim(3, '101')).toBeUndefined();
    });
  });
});

function givenCursor(
  runRevision: number,
  stateRevision: number,
  gameDay: number,
): InsuranceCommandCursorSource {
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
