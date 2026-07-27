import { describe, expect, it } from '@jest/globals';
import { HousingCommandError } from '../../api/housing-api.js';
import {
  createHousingLeaseArrearPaymentRetryPolicy,
  createHousingLeaseDepositLoanQuoteRetryPolicy,
  createHousingLeaseRetryPolicy,
  createHousingMortgageQuoteRetryPolicy,
  createHousingPropertySaleOrderCancelRetryPolicy,
  createHousingPropertySaleOrderCreateRetryPolicy,
  createHousingPropertySaleOrderRepriceRetryPolicy,
  createHousingPurchaseRetryPolicy,
} from './create-housing-retry-policy.js';
import type { HousingCommandCursorSource } from './types.js';

describe('전세자금대출 견적 명령 재시도 판단', () => {
  describe('맥락: 전송 뒤 서버 결과를 확인하지 못한 경우', () => {
    it('given outcome-unknown 오류, when 다른 견적을 선택하면, then 최초 path용 body를 그대로 쓴다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000001');
      const policy = createHousingLeaseDepositLoanQuoteRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(2, 10, 30), {
        listingId: '701',
        offerKind: 'jeonse',
        productVersionId: '31',
        principalKrw: 80_000_000,
      });
      policy.fail(original, new Error('connection lost'));

      const retried = policy.select(givenCursor(2, 11, 31), {
        listingId: '702',
        offerKind: 'jeonse',
        productVersionId: '32',
        principalKrw: 1,
      });

      expect(retried).toEqual(original);
      expect(policy.pending(2)).toEqual(original);
    });
  });

  describe('맥락: 서버가 견적을 명시적으로 거절한 경우', () => {
    it('given domain 오류, when 다시 선택하면, then 최신 cursor와 새 command ID를 쓴다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000001',
        '00000000-0000-4000-8000-000000000002',
      );
      const policy = createHousingLeaseDepositLoanQuoteRetryPolicy({ createCommandId: ids.next });
      const rejected = policy.select(givenCursor(2, 10, 30), {
        listingId: '701',
        offerKind: 'jeonse',
        productVersionId: '31',
        principalKrw: 80_000_000,
      });
      policy.fail(rejected, new HousingCommandError('contractConflict', '매물 충돌'));

      const next = policy.select(givenCursor(2, 11, 30), {
        listingId: '701',
        offerKind: 'jeonse',
        productVersionId: '31',
        principalKrw: 80_000_000,
      });

      expect(next).toMatchObject({
        commandId: '00000000-0000-4000-8000-000000000002',
        expectedStateRevision: 11,
        expectedGameDay: 30,
      });
    });
  });
});

describe('주택담보대출 견적 명령 재시도 판단', () => {
  describe('맥락: 전용 견적 endpoint의 결과를 확인하지 못한 경우', () => {
    it('given outcome-unknown 오류, when 다른 매물로 견적을 고르면, then 최초 body와 상품·원금을 그대로 쓴다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000011');
      const policy = createHousingMortgageQuoteRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(4, 30, 90), {
        listingId: '901',
        productVersionId: '41',
        principalKrw: 300_000_000,
      });
      policy.fail(original, new Error('connection lost'));

      const retried = policy.select(givenCursor(4, 31, 91), {
        listingId: '902',
        productVersionId: '42',
        principalKrw: 1,
      });

      expect(retried).toEqual(original);
      expect(policy.pending(4)).toEqual(original);
    });
  });
});

describe('보유주택 매수 명령 재시도 판단', () => {
  describe('맥락: 대출 매수 결과를 확인하지 못한 경우', () => {
    it('given outcome-unknown 오류, when 현금 매수로 바꾸면, then 최초 listing과 mortgage quote ID를 보존한다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000012');
      const policy = createHousingPurchaseRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(4, 30, 90), {
        listingId: '901',
        mortgageQuoteId: '991',
      });
      policy.fail(original, new Error('connection lost'));

      const retried = policy.select(givenCursor(4, 31, 91), {
        listingId: '902',
        mortgageQuoteId: null,
      });

      expect(retried).toEqual(original);
      expect(retried.mortgageQuoteId).toBe('991');
    });
  });

  describe('맥락: 현금 매수 결과를 확인한 경우', () => {
    it('given 완료된 명령, when pending을 조회하면, then 보존된 body가 없다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000013');
      const policy = createHousingPurchaseRetryPolicy({ createCommandId: ids.next });
      const completed = policy.select(givenCursor(4, 30, 90), {
        listingId: '901',
        mortgageQuoteId: null,
      });
      policy.fail(completed, new Error('connection lost'));
      policy.complete(completed);

      const pending = policy.pending(4);

      expect(pending).toBeUndefined();
    });
  });
});

describe('보유주택 매도 주문 생성 재시도 판단', () => {
  describe('맥락: 주문 생성 뒤 서버 결과를 확인하지 못한 경우', () => {
    it('given outcome-unknown 오류, when 다른 주택과 주문가를 선택하면, then 최초 body를 그대로 쓴다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000021');
      const policy = createHousingPropertySaleOrderCreateRetryPolicy({
        createCommandId: ids.next,
      });
      const original = policy.select(givenCursor(5, 40, 120), {
        holdingId: '1001',
        askingPriceKrw: 420_000_000,
      });
      policy.fail(original, new Error('connection lost'));

      const retried = policy.select(givenCursor(5, 41, 121), {
        holdingId: '1002',
        askingPriceKrw: 1,
      });

      expect(retried).toEqual(original);
      expect(policy.pending(5)).toEqual(original);
    });
  });
});

describe('보유주택 매도 주문가 변경 재시도 판단', () => {
  describe('맥락: 주문가 변경 뒤 서버 결과를 확인하지 못한 경우', () => {
    it('given outcome-unknown 오류, when 다른 주문을 변경하면, then 최초 path와 body를 그대로 쓴다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000022');
      const policy = createHousingPropertySaleOrderRepriceRetryPolicy({
        createCommandId: ids.next,
      });
      const original = policy.select(givenCursor(5, 40, 120), {
        orderId: '2001',
        askingPriceKrw: 410_000_000,
      });
      policy.fail(original, new Error('connection lost'));

      const retried = policy.select(givenCursor(5, 41, 121), {
        orderId: '2002',
        askingPriceKrw: 1,
      });

      expect(retried).toEqual(original);
      expect(retried.orderId).toBe('2001');
      expect(policy.pending(5)).toEqual(original);
    });
  });
});

describe('보유주택 매도 주문 취소 재시도 판단', () => {
  describe('맥락: 서버가 주문 취소를 명시적으로 거절한 경우', () => {
    it('given domain 오류, when 다른 주문을 취소하면, then 최신 path와 cursor를 쓴다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000023',
        '00000000-0000-4000-8000-000000000024',
      );
      const policy = createHousingPropertySaleOrderCancelRetryPolicy({
        createCommandId: ids.next,
      });
      const rejected = policy.select(givenCursor(5, 40, 120), { orderId: '2001' });
      policy.fail(rejected, new HousingCommandError('contractConflict', '주문 충돌'));

      const next = policy.select(givenCursor(5, 41, 121), { orderId: '2002' });

      expect(next).toMatchObject({
        orderId: '2002',
        request: {
          commandId: '00000000-0000-4000-8000-000000000024',
          expectedStateRevision: 41,
          expectedGameDay: 121,
        },
      });
    });
  });
});

describe('임대차 이사 명령 재시도 판단', () => {
  describe('맥락: 전송 뒤 서버 결과를 확인하지 못한 경우', () => {
    it('given outcome-unknown 오류, when 다시 선택하면, then 최초 body를 그대로 사용한다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000001',
        '00000000-0000-4000-8000-000000000002',
      );
      const policy = createHousingLeaseRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(2, 10, 30), {
        listingId: '701',
        offerKind: 'jeonse',
      });
      policy.fail(original, new Error('connection lost'));

      const retried = policy.select(givenCursor(2, 11, 31), {
        listingId: '702',
        offerKind: 'jeonse',
      });

      expect(retried).toEqual(original);
    });

    it('given 월세 이사의 outcome-unknown 오류, when 다시 선택하면, then 최초 offer kind를 유지한다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000001');
      const policy = createHousingLeaseRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(2, 10, 30), {
        listingId: '701',
        offerKind: 'monthlyRent',
      });
      policy.fail(original, new Error('connection lost'));

      const retried = policy.select(givenCursor(2, 11, 31), {
        listingId: '702',
        offerKind: 'jeonse',
      });

      expect(retried).toEqual(original);
      expect(retried.offerKind).toBe('monthlyRent');
    });

    it('given financed 전세 이사의 outcome-unknown 오류, when 다시 선택하면, then 최초 quote ID까지 유지한다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000001');
      const policy = createHousingLeaseRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(2, 10, 30), {
        listingId: '701',
        offerKind: 'jeonse',
        loanQuoteId: '901',
      });
      policy.fail(original, new Error('connection lost'));

      const retried = policy.select(givenCursor(2, 11, 31), {
        listingId: '702',
        offerKind: 'jeonse',
      });

      expect(retried).toEqual(original);
      expect('loanQuoteId' in retried ? retried.loanQuoteId : undefined).toBe('901');
    });

    it('given outcome-unknown 오류, when 화면을 다시 열면, then run의 pending body를 복구한다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000001');
      const policy = createHousingLeaseRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(2, 10, 30), {
        listingId: '701',
        offerKind: 'jeonse',
      });
      policy.fail(original, new Error('connection lost'));

      const pending = policy.pending(2);

      expect(pending).toEqual(original);
    });
  });

  describe('맥락: 서버가 명령을 명시적으로 거절한 경우', () => {
    it('given domain 오류, when 다시 선택하면, then 최신 cursor와 새 command ID를 쓴다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000001',
        '00000000-0000-4000-8000-000000000002',
      );
      const policy = createHousingLeaseRetryPolicy({ createCommandId: ids.next });
      const rejected = policy.select(givenCursor(2, 10, 30), {
        listingId: '701',
        offerKind: 'jeonse',
      });
      policy.fail(rejected, new HousingCommandError('contractConflict', '매물 충돌'));

      const next = policy.select(givenCursor(2, 11, 31), {
        listingId: '701',
        offerKind: 'jeonse',
      });

      expect(next).toMatchObject({
        commandId: '00000000-0000-4000-8000-000000000002',
        expectedStateRevision: 11,
        expectedGameDay: 31,
      });
    });
  });

  describe('맥락: 명령 결과를 확인한 경우', () => {
    it('given 완료된 명령, when pending을 조회하면, then 복구할 body가 없다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000001');
      const policy = createHousingLeaseRetryPolicy({ createCommandId: ids.next });
      const completed = policy.select(givenCursor(2, 10, 30), {
        listingId: '701',
        offerKind: 'jeonse',
      });
      policy.fail(completed, new Error('connection lost'));
      policy.complete(completed);

      const pending = policy.pending(2);

      expect(pending).toBeUndefined();
    });
  });
});

describe('월세 연체 상환 명령 재시도 판단', () => {
  describe('맥락: 전송 뒤 서버 결과를 확인하지 못한 경우', () => {
    it('given outcome-unknown 오류, when 다른 연체로 다시 선택하면, then 최초 path와 body를 그대로 쓴다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000001');
      const policy = createHousingLeaseArrearPaymentRetryPolicy({ createCommandId: ids.next });
      const original = policy.select(givenCursor(3, 20, 45), {
        arrearId: '801',
        amountKrw: 120_000,
      });
      policy.fail(original, new Error('connection lost'));

      const retried = policy.select(givenCursor(3, 21, 46), {
        arrearId: '802',
        amountKrw: 1,
      });

      expect(retried).toEqual(original);
      expect(policy.pending(3)).toEqual(original);
    });
  });

  describe('맥락: 서버가 상환을 명시적으로 거절한 경우', () => {
    it('given domain 오류, when 다시 선택하면, then 최신 cursor와 새 command ID를 쓴다', () => {
      const ids = givenCommandIds(
        '00000000-0000-4000-8000-000000000001',
        '00000000-0000-4000-8000-000000000002',
      );
      const policy = createHousingLeaseArrearPaymentRetryPolicy({ createCommandId: ids.next });
      const rejected = policy.select(givenCursor(3, 20, 45), {
        arrearId: '801',
        amountKrw: 120_000,
      });
      policy.fail(rejected, new HousingCommandError('contractConflict', '연체 충돌'));

      const next = policy.select(givenCursor(3, 21, 45), {
        arrearId: '801',
        amountKrw: 120_000,
      });

      expect(next).toMatchObject({
        arrearId: '801',
        request: {
          commandId: '00000000-0000-4000-8000-000000000002',
          expectedStateRevision: 21,
          expectedGameDay: 45,
          amountKrw: 120_000,
        },
      });
    });
  });

  describe('맥락: 상환 결과를 확인한 경우', () => {
    it('given 완료된 상환, when pending을 조회하면, then 복구할 path와 body가 없다', () => {
      const ids = givenCommandIds('00000000-0000-4000-8000-000000000001');
      const policy = createHousingLeaseArrearPaymentRetryPolicy({ createCommandId: ids.next });
      const completed = policy.select(givenCursor(3, 20, 45), {
        arrearId: '801',
        amountKrw: 120_000,
      });
      policy.fail(completed, new Error('connection lost'));
      policy.complete(completed);

      const pending = policy.pending(3);

      expect(pending).toBeUndefined();
    });
  });
});

function givenCursor(
  runRevision: number,
  stateRevision: number,
  gameDay: number,
): HousingCommandCursorSource {
  return { runRevision, stateRevision, gameDay };
}

function givenCommandIds(...ids: readonly string[]): {
  readonly next: () => string;
} {
  let index = 0;
  return {
    next() {
      const id = ids[index];
      if (id === undefined) throw new Error('명령 ID fixture가 부족합니다.');
      index += 1;
      return id;
    },
  };
}
