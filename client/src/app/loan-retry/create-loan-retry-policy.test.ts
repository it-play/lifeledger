import { describe, expect, it } from '@jest/globals';
import { LoanCommandError } from '../../api/loan-api.js';
import { HttpError, ResponseShapeError } from '../../lib/http/index.js';
import {
  createLoanExecutionRetryPolicy,
  createLoanPrepaymentRetryPolicy,
  createLoanQuoteRetryPolicy,
} from './create-loan-retry-policy.js';
import type { LoanCommandCursorSource } from './types.js';

const QUOTE = { productVersionId: '21', principalKrw: 10_000_000 } as const;
const EXECUTION = { quoteId: '30' } as const;
const PREPAYMENT = { loanId: '40', principalKrw: 1_000_000 } as const;

describe('대출 견적 명령 재시도 판단', () => {
  describe('맥락: 견적 응답을 확인하지 못한 경우', () => {
    it('given 전송 오류, when 같은 상품과 원금으로 다시 선택하면, then 원래 명령과 cursor를 재사용한다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000001');
      const policy = createLoanQuoteRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(3, 12, 120), QUOTE);
      policy.fail(original, new Error('connection lost'));

      const retried = policy.select(givenCursor(3, 13, 121), QUOTE);

      expect(retried).toEqual(original);
      expect(ids.count()).toBe(1);
    });
  });

  describe('맥락: 서버가 견적 명령을 거절한 경우', () => {
    it('given 도메인 오류, when 같은 입력을 다시 선택하면, then 최신 cursor로 새 명령을 만든다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000001',
        '00000000-0000-4000-8000-000000000002',
      );
      const policy = createLoanQuoteRetryPolicy({ createCommandId: ids.next });
      const rejected = policy.select(givenCursor(3, 12, 120), QUOTE);
      policy.fail(rejected, new LoanCommandError('invalidCommand', '원금 범위를 벗어났습니다'));

      const next = policy.select(givenCursor(3, 13, 121), QUOTE);

      expect(next.commandId).toBe('00000000-0000-4000-8000-000000000002');
      expect(next.expectedStateRevision).toBe(13);
      expect(next.expectedGameDay).toBe(121);
    });
  });

  describe('맥락: 견적 결과를 확인한 경우', () => {
    it('given 완료된 명령, when 같은 입력을 다시 선택하면, then 새 명령을 만든다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000001',
        '00000000-0000-4000-8000-000000000002',
      );
      const policy = createLoanQuoteRetryPolicy({ createCommandId: ids.next });
      const completed = policy.select(givenCursor(3, 12, 120), QUOTE);
      policy.complete(completed);

      const next = policy.select(givenCursor(3, 13, 121), QUOTE);

      expect(next.commandId).toBe('00000000-0000-4000-8000-000000000002');
    });
  });

  describe('맥락: 견적 입력이 달라진 경우', () => {
    it('given 보류 중인 견적, when 원금을 바꾸면, then 별도 명령을 만든다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000001',
        '00000000-0000-4000-8000-000000000002',
      );
      const policy = createLoanQuoteRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(3, 12, 120), QUOTE);
      policy.fail(original, new Error('connection lost'));

      const other = policy.select(givenCursor(3, 12, 120), {
        ...QUOTE,
        principalKrw: QUOTE.principalKrw + 1,
      });

      expect(other.commandId).toBe('00000000-0000-4000-8000-000000000002');
    });
  });
});

describe('대출 실행 명령 재시도 판단', () => {
  describe('맥락: 실행 응답을 전송 중 잃은 경우', () => {
    it('given 전송 오류, when 같은 견적을 다시 실행하면, then 원래 UUID와 cursor를 재사용한다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000011');
      const policy = createLoanExecutionRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(3, 12, 120), EXECUTION);
      policy.fail(original, new Error('connection lost'));

      const retried = policy.select(givenCursor(3, 13, 120), EXECUTION);

      expect(retried).toEqual(original);
      expect(ids.count()).toBe(1);
    });

    it('given 결과 불명 실행, when 원래 run과 견적을 조회하면, then 날짜가 지나도 보류 명령을 돌려준다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000011');
      const policy = createLoanExecutionRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(3, 12, 120), EXECUTION);
      policy.fail(original, new Error('connection lost'));

      const pending = policy.pending(3, EXECUTION);

      expect(pending).toEqual(original);
    });
  });

  describe('맥락: 실행 성공 응답이 계약과 다른 경우', () => {
    it('given response shape 오류, when 같은 견적을 다시 실행하면, then 원래 UUID와 cursor를 재사용한다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000011');
      const policy = createLoanExecutionRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(3, 12, 120), EXECUTION);
      policy.fail(original, new ResponseShapeError('/api/loans', new Error('quote ID mismatch')));

      const retried = policy.select(givenCursor(3, 13, 120), EXECUTION);

      expect(retried).toEqual(original);
    });
  });

  describe('맥락: 서버 내부 오류로 실행 결과를 확인하지 못한 경우', () => {
    it('given 5xx 오류, when 같은 견적을 다시 실행하면, then 원래 UUID와 cursor를 재사용한다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000011');
      const policy = createLoanExecutionRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(3, 12, 120), EXECUTION);
      policy.fail(original, new HttpError(500, '/api/loans', undefined));

      const retried = policy.select(givenCursor(3, 13, 120), EXECUTION);

      expect(retried).toEqual(original);
    });
  });

  describe('맥락: 서버가 실행 명령을 확정적으로 거절한 경우', () => {
    it('given contractConflict, when 같은 견적을 다시 실행하면, then 최신 cursor로 새 UUID를 만든다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000011',
        '00000000-0000-4000-8000-000000000012',
      );
      const policy = createLoanExecutionRetryPolicy({ createCommandId: ids.next });
      const rejected = policy.select(givenCursor(3, 12, 120), EXECUTION);
      policy.fail(rejected, new LoanCommandError('contractConflict', '견적을 실행할 수 없습니다'));

      const next = policy.select(givenCursor(3, 13, 120), EXECUTION);

      expect(next.commandId).toBe('00000000-0000-4000-8000-000000000012');
      expect(next.expectedStateRevision).toBe(13);
    });
  });

  describe('맥락: 실행 결과를 확인한 경우', () => {
    it('given 완료된 실행, when 같은 견적을 다시 실행하면, then 새 UUID를 만든다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000011',
        '00000000-0000-4000-8000-000000000012',
      );
      const policy = createLoanExecutionRetryPolicy({ createCommandId: ids.next });
      const completed = policy.select(givenCursor(3, 12, 120), EXECUTION);
      policy.complete(completed);

      const next = policy.select(givenCursor(3, 13, 120), EXECUTION);

      expect(next.commandId).toBe('00000000-0000-4000-8000-000000000012');
    });
  });

  describe('맥락: 다른 견적을 실행하는 경우', () => {
    it('given 결과 불명 실행, when 다른 quote ID를 선택하면, then 별도 UUID를 만든다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000011',
        '00000000-0000-4000-8000-000000000012',
      );
      const policy = createLoanExecutionRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(3, 12, 120), EXECUTION);
      policy.fail(original, new Error('connection lost'));

      const other = policy.select(givenCursor(3, 12, 120), { quoteId: '31' });

      expect(other.commandId).toBe('00000000-0000-4000-8000-000000000012');
    });
  });

  describe('맥락: 견적과 실행 명령을 연달아 만드는 경우', () => {
    it('given 같은 UUID 생성기, when 견적 뒤 실행을 선택하면, then 서로 다른 UUID를 사용한다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000001',
        '00000000-0000-4000-8000-000000000011',
      );
      const quotePolicy = createLoanQuoteRetryPolicy({ createCommandId: ids.next });
      const executionPolicy = createLoanExecutionRetryPolicy({ createCommandId: ids.next });

      const quote = quotePolicy.select(givenCursor(3, 12, 120), QUOTE);
      const execution = executionPolicy.select(givenCursor(3, 12, 120), EXECUTION);

      expect(execution.commandId).not.toBe(quote.commandId);
    });
  });
});

describe('대출 조기상환 명령 재시도 판단', () => {
  describe('맥락: 전송 중 화면이 다시 mount된 경우', () => {
    it('given 선택한 조기상환, when 같은 run의 pending을 조회하면, then path와 UUID와 cursor를 보존한다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000021');
      const policy = createLoanPrepaymentRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(3, 12, 120), PREPAYMENT);

      const pending = policy.pendingForRun(3);

      expect(pending).toEqual(original);
      expect(pending?.loanId).toBe('40');
      expect(ids.count()).toBe(1);
    });
  });

  describe('맥락: SSE가 먼저 도착하고 응답은 유실된 경우', () => {
    it('given 바뀐 최신 cursor, when 같은 원금의 결과를 확인하면, then 원래 명령을 재사용한다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000021');
      const policy = createLoanPrepaymentRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(3, 12, 120), PREPAYMENT);
      policy.fail(original, new Error('connection lost'));

      const retried = policy.select(givenCursor(3, 13, 121), PREPAYMENT);

      expect(retried).toEqual(original);
    });

    it('given 완납으로 목록에서 사라진 계약, when draft 없이 run pending을 조회하면, then 원래 path를 돌려준다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000021');
      const policy = createLoanPrepaymentRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(3, 12, 120), PREPAYMENT);
      policy.fail(original, new ResponseShapeError('/api/loans/40/prepayments', new Error('lost')));

      const pending = policy.pendingForRun(3);

      expect(pending).toEqual(original);
    });
  });

  describe('맥락: 서버 내부 오류로 결과를 확인하지 못한 경우', () => {
    it('given 5xx 오류, when 같은 조기상환을 조회하면, then 원래 UUID를 보존한다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000021');
      const policy = createLoanPrepaymentRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(3, 12, 120), PREPAYMENT);
      policy.fail(original, new HttpError(500, '/api/loans/40/prepayments', undefined));

      const pending = policy.pending(3, PREPAYMENT);

      expect(pending).toEqual(original);
    });
  });

  describe('맥락: 서버가 조기상환을 확정적으로 거절한 경우', () => {
    it('given contractConflict, when 같은 입력을 다시 선택하면, then 최신 cursor로 새 UUID를 만든다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000021',
        '00000000-0000-4000-8000-000000000022',
      );
      const policy = createLoanPrepaymentRetryPolicy({ createCommandId: ids.next });
      const rejected = policy.select(givenCursor(3, 12, 120), PREPAYMENT);
      policy.fail(rejected, new LoanCommandError('contractConflict', '조기상환할 수 없습니다'));

      const next = policy.select(givenCursor(3, 13, 121), PREPAYMENT);

      expect(next.request.commandId).toBe('00000000-0000-4000-8000-000000000022');
      expect(next.request.expectedStateRevision).toBe(13);
      expect(next.request.expectedGameDay).toBe(121);
    });
  });

  describe('맥락: 조기상환 결과를 확인한 경우', () => {
    it('given 완료된 명령, when 같은 run pending을 조회하면, then 보류 명령을 제거한다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000021');
      const policy = createLoanPrepaymentRetryPolicy({ createCommandId: ids.next });
      const completed = policy.select(givenCursor(3, 12, 120), PREPAYMENT);
      policy.complete(completed);

      const pending = policy.pendingForRun(3);

      expect(pending).toBeUndefined();
    });
  });

  describe('맥락: 다른 run이나 계약이나 원금인 경우', () => {
    it('given 결과 불명 명령, when key 일부가 다르면, then 별도 UUID를 만든다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000021',
        '00000000-0000-4000-8000-000000000022',
        '00000000-0000-4000-8000-000000000023',
        '00000000-0000-4000-8000-000000000024',
      );
      const policy = createLoanPrepaymentRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(3, 12, 120), PREPAYMENT);

      const otherRun = policy.select(givenCursor(4, 1, 0), PREPAYMENT);
      const otherLoan = policy.select(givenCursor(3, 12, 120), { ...PREPAYMENT, loanId: '41' });
      const otherPrincipal = policy.select(givenCursor(3, 12, 120), {
        ...PREPAYMENT,
        principalKrw: PREPAYMENT.principalKrw + 1,
      });

      expect([
        original.request.commandId,
        otherRun.request.commandId,
        otherLoan.request.commandId,
        otherPrincipal.request.commandId,
      ]).toEqual([
        '00000000-0000-4000-8000-000000000021',
        '00000000-0000-4000-8000-000000000022',
        '00000000-0000-4000-8000-000000000023',
        '00000000-0000-4000-8000-000000000024',
      ]);
    });
  });
});

function givenCursor(
  runRevision: number,
  stateRevision: number,
  gameDay: number,
): LoanCommandCursorSource {
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
