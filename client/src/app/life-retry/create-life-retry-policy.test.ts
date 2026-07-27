import { describe, expect, it } from '@jest/globals';
import type { LifeBudgetUpdateDraft } from '../../api/contracts.js';
import { LifeCommandError } from '../../api/life-api.js';
import {
  createEssentialArrearPaymentRetryPolicy,
  createLifeBudgetRetryPolicy,
} from './create-life-retry-policy.js';
import type { LifeCommandCursorSource } from './types.js';

const BUDGET: LifeBudgetUpdateDraft = {
  selections: [
    { category: 'housing', bandId: '1' },
    { category: 'food', bandId: '1' },
    { category: 'transport', bandId: '1' },
    { category: 'communication', bandId: '1' },
    { category: 'utilities', bandId: '1' },
    { category: 'healthcare', bandId: '1' },
    { category: 'education', bandId: '1' },
    { category: 'dependentCare', bandId: '1' },
    { category: 'discretionary', bandId: '1' },
  ],
};

describe('생활비 명령 재시도 판단', () => {
  describe('맥락: 예산 변경 응답을 확인하지 못한 경우', () => {
    it('given 전송 오류, when 같은 입력을 다시 선택하면, then 원래 명령과 cursor를 재사용한다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000001');
      const policy = createLifeBudgetRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(1, 3, 5), BUDGET);
      policy.fail(original, new Error('connection lost'));

      const retried = policy.select(givenCursor(1, 4, 6), BUDGET);

      expect(retried).toEqual(original);
      expect(ids.count()).toBe(1);
    });
  });

  describe('맥락: 예산 변경이 서버에서 거절된 경우', () => {
    it('given 도메인 오류, when 같은 입력을 다시 선택하면, then 최신 cursor로 새 명령을 만든다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000001',
        '00000000-0000-4000-8000-000000000002',
      );
      const policy = createLifeBudgetRetryPolicy({ createCommandId: ids.next });
      const rejected = policy.select(givenCursor(1, 3, 5), BUDGET);
      policy.fail(rejected, new LifeCommandError('settlementConflict', '충돌'));

      const next = policy.select(givenCursor(1, 4, 6), BUDGET);

      expect(next.commandId).toBe('00000000-0000-4000-8000-000000000002');
      expect(next.expectedStateRevision).toBe(4);
      expect(next.expectedGameDay).toBe(6);
    });
  });

  describe('맥락: 필수 생활비 연체 상환 응답을 확인하지 못한 경우', () => {
    it('given 전송 오류, when 같은 연체와 금액을 다시 선택하면, then 원래 명령을 재사용한다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000001');
      const policy = createEssentialArrearPaymentRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(2, 10, 30), '7', { amountKrw: 50_000 });
      policy.fail(original, new Error('connection lost'));

      const retried = policy.select(givenCursor(2, 11, 31), '7', { amountKrw: 50_000 });

      expect(retried).toEqual(original);
      expect(ids.count()).toBe(1);
    });
  });

  describe('맥락: 필수 생활비 연체 상환 결과를 확인한 경우', () => {
    it('given 완료된 명령, when 같은 금액을 다시 선택하면, then 새 명령을 만든다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000001',
        '00000000-0000-4000-8000-000000000002',
      );
      const policy = createEssentialArrearPaymentRetryPolicy({ createCommandId: ids.next });
      const completed = policy.select(givenCursor(2, 10, 30), '7', { amountKrw: 50_000 });
      policy.complete(completed);

      const next = policy.select(givenCursor(2, 11, 31), '7', { amountKrw: 50_000 });

      expect(next.request.commandId).toBe('00000000-0000-4000-8000-000000000002');
      expect(next.request.expectedStateRevision).toBe(11);
    });
  });
});

function givenCursor(
  runRevision: number,
  stateRevision: number,
  gameDay: number,
): LifeCommandCursorSource {
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
