import { describe, expect, it } from '@jest/globals';
import type {
  BondOrderRequest,
  FinanceCommandRequest,
  GameCommandCursor,
} from '../../api/contracts.js';
import { FinanceCommandError } from '../../api/game-api.js';
import { createFinanceAssetRetryPolicy, type FinanceAssetRetryPolicy } from './index.js';

type BondOrderDraft = Omit<BondOrderRequest, keyof FinanceCommandRequest>;

const DRAFT: BondOrderDraft = {
  accountId: '1',
  seriesId: '21',
  side: 'buy',
  bondUnits: 10,
};

describe('M2-D 자산 명령 재시도', () => {
  describe('맥락: 전송 결과를 알 수 없는 경우', () => {
    it('given 결과가 불명확한 명령, when 같은 의도를 다시 제출하면, then 원래 UUID와 cursor를 재사용한다', () => {
      const policy = givenPolicy();
      const first = whenSelect(policy, givenCursor(41), DRAFT);
      policy.fail(first, new Error('response lost'));

      const selected = whenSelect(policy, givenCursor(42), DRAFT);

      expect(selected).toBe(first);
    });
  });

  describe('맥락: 서버가 확정적인 도메인 오류를 반환한 경우', () => {
    it('given 보류 후 실패가 확정된 명령, when 같은 의도를 다시 제출하면, then 새 명령을 만든다', () => {
      const policy = givenPolicy();
      const first = whenSelect(policy, givenCursor(41), DRAFT);
      policy.fail(first, new Error('response lost'));
      const retried = whenSelect(policy, givenCursor(42), DRAFT);
      policy.fail(retried, new FinanceCommandError('busy', '다른 명령을 처리 중입니다'));

      const selected = whenSelect(policy, givenCursor(43), DRAFT);

      expect(selected).not.toBe(first);
    });
  });

  describe('맥락: 명령 성공을 확인한 경우', () => {
    it('given 완료된 명령, when 같은 의도를 다시 제출하면, then 새 명령을 만든다', () => {
      const policy = givenPolicy();
      const first = whenSelect(policy, givenCursor(41), DRAFT);
      policy.fail(first, new Error('response lost'));
      policy.complete(first);

      const selected = whenSelect(policy, givenCursor(42), DRAFT);

      expect(selected).not.toBe(first);
    });
  });

  describe('맥락: 보류된 명령과 다른 주문 의도를 제출한 경우', () => {
    it('given 보류된 매수, when 매도와 원래 매수를 차례로 선택하면, then 의도별 보류 상태를 분리한다', () => {
      const policy = givenPolicy();
      const first = whenSelect(policy, givenCursor(41), DRAFT);
      policy.fail(first, new Error('response lost'));

      const different = whenSelect(policy, givenCursor(42), { ...DRAFT, side: 'sell' });
      const original = whenSelect(policy, givenCursor(43), DRAFT);

      expect({
        differentReusedFirst: different === first,
        originalReusedFirst: original === first,
      }).toEqual({ differentReusedFirst: false, originalReusedFirst: true });
    });
  });
});

function givenPolicy(): FinanceAssetRetryPolicy<BondOrderDraft, BondOrderRequest> {
  return createFinanceAssetRetryPolicy({
    createCommandId: givenCommandIds(),
    draftKey: (runRevision, draft) =>
      JSON.stringify([runRevision, draft.accountId, draft.seriesId, draft.side, draft.bondUnits]),
    requestKey: (request) =>
      JSON.stringify([
        request.expectedRunRevision,
        request.accountId,
        request.seriesId,
        request.side,
        request.bondUnits,
      ]),
    requestOf: (snapshot, draft, commandId) => ({
      commandId,
      expectedRunRevision: snapshot.runRevision,
      expectedStateRevision: snapshot.stateRevision,
      expectedGameDay: snapshot.gameDay,
      ...draft,
    }),
  });
}

function givenCommandIds(): () => string {
  let next = 1;
  return () => `00000000-0000-0000-0000-${String(next++).padStart(12, '0')}`;
}

function givenCursor(stateRevision: number): GameCommandCursor {
  return { runRevision: 3, stateRevision, gameDay: 8 };
}

function whenSelect(
  policy: FinanceAssetRetryPolicy<BondOrderDraft, BondOrderRequest>,
  cursor: GameCommandCursor,
  draft: BondOrderDraft,
): BondOrderRequest {
  return policy.select(cursor, draft);
}
