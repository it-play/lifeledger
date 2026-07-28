import { describe, expect, it } from '@jest/globals';
import { type HttpClient, HttpError } from '../lib/http/index.js';
import type {
  GameSnapshot,
  HousingLeaseArrearPaymentRequest,
  HousingLeaseDepositLoanQuoteRequest,
  HousingLeaseRequest,
  HousingListingsQuery,
  HousingMortgageQuoteRequest,
  HousingPropertySaleOrderCancelRequest,
  HousingPropertySaleOrderCreateRequest,
  HousingPropertySaleOrderRepriceRequest,
  HousingPurchaseRequest,
} from './contracts.js';
import { createHousingApi, HousingCommandError, HousingQueryError } from './housing-api.js';

const LEASE_REQUEST: HousingLeaseRequest = {
  commandId: '00000000-0000-4000-8000-000000000001',
  expectedRunRevision: 3,
  expectedStateRevision: 42,
  expectedGameDay: 17,
  listingId: '7002',
  offerKind: 'jeonse',
};

const MONTHLY_LEASE_REQUEST: HousingLeaseRequest = {
  ...LEASE_REQUEST,
  offerKind: 'monthlyRent',
};

const FINANCED_LEASE_REQUEST: HousingLeaseRequest = {
  ...LEASE_REQUEST,
  loanQuoteId: '8301',
};

const DEPOSIT_LOAN_QUOTE_REQUEST: HousingLeaseDepositLoanQuoteRequest = {
  commandId: '00000000-0000-4000-8000-000000000003',
  expectedRunRevision: 3,
  expectedStateRevision: 43,
  expectedGameDay: 17,
  listingId: '7002',
  offerKind: 'jeonse',
  productVersionId: '22',
  principalKrw: 4_000_000,
};

const ARREAR_PAYMENT_REQUEST: HousingLeaseArrearPaymentRequest = {
  commandId: '00000000-0000-4000-8000-000000000002',
  expectedRunRevision: 3,
  expectedStateRevision: 42,
  expectedGameDay: 17,
  amountKrw: 200_000,
};

const MORTGAGE_QUOTE_REQUEST: HousingMortgageQuoteRequest = {
  commandId: '00000000-0000-4000-8000-000000000004',
  expectedRunRevision: 3,
  expectedStateRevision: 43,
  expectedGameDay: 17,
  listingId: '7002',
  productVersionId: '23',
  principalKrw: 4_000_000,
};

const PURCHASE_REQUEST: HousingPurchaseRequest = {
  commandId: '00000000-0000-4000-8000-000000000005',
  expectedRunRevision: 3,
  expectedStateRevision: 43,
  expectedGameDay: 17,
  listingId: '7002',
  mortgageQuoteId: '9301',
};

const CASH_PURCHASE_REQUEST: HousingPurchaseRequest = {
  ...PURCHASE_REQUEST,
  commandId: '00000000-0000-4000-8000-000000000006',
  mortgageQuoteId: null,
};

const PROPERTY_SALE_CREATE_REQUEST: HousingPropertySaleOrderCreateRequest = {
  commandId: '00000000-0000-4000-8000-000000000007',
  expectedRunRevision: 3,
  expectedStateRevision: 43,
  expectedGameDay: 17,
  holdingId: '9401',
  askingPriceKrw: 10_000_000,
};

const PROPERTY_SALE_REPRICE_REQUEST: HousingPropertySaleOrderRepriceRequest = {
  commandId: '00000000-0000-4000-8000-000000000008',
  expectedRunRevision: 3,
  expectedStateRevision: 43,
  expectedGameDay: 17,
  askingPriceKrw: 9_000_000,
};

const PROPERTY_SALE_CANCEL_REQUEST: HousingPropertySaleOrderCancelRequest = {
  commandId: '00000000-0000-4000-8000-000000000009',
  expectedRunRevision: 3,
  expectedStateRevision: 43,
  expectedGameDay: 17,
};

describe('주거 매물 조회 protocol', () => {
  describe('맥락: 현재 거주 지역을 기본 조회하는 경우', () => {
    it('given region 없는 query, when 조회하면, then exact path와 residence 지역 응답을 사용한다', async () => {
      const gets: string[] = [];
      const api = createHousingApi({
        http: givenRecordingGetHttp(givenHousingResponse('capitalArea'), gets),
      });

      const result = await api.listListings();

      expect(result.selectedRegionKey).toBe('capitalArea');
      expect(gets).toEqual(['/api/housing/listings']);
    });
  });

  describe('맥락: 사용자가 다른 지역을 선택한 경우', () => {
    it('given canonical region query, when 조회하면, then exact query와 선택 지역 응답을 사용한다', async () => {
      const gets: string[] = [];
      const api = createHousingApi({
        http: givenRecordingGetHttp(givenHousingResponse('metropolitan'), gets),
      });

      const result = await api.listListings({ region: 'metropolitan' });

      expect(result.selectedRegionKey).toBe('metropolitan');
      expect(gets).toEqual(['/api/housing/listings?region=metropolitan']);
    });

    it('given query와 다른 selected region, when 응답을 읽으면, then 상관관계 위반으로 거절한다', async () => {
      const api = createHousingApi({
        http: givenRespondingHttp(givenHousingResponse('capitalArea')),
      });

      const result = api.listListings({ region: 'metropolitan' });

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 공개 query 밖의 값으로 조회하는 경우', () => {
    it('given unknown limit query, when 조회하면, then HTTP 전에 strict 요청을 거절한다', () => {
      const gets: string[] = [];
      const api = createHousingApi({
        http: givenRecordingGetHttp(givenHousingResponse('capitalArea'), gets),
      });
      const query = { region: 'capitalArea', limit: 24 } as unknown as HousingListingsQuery;

      const whenRead = () => api.listListings(query);

      expect(whenRead).toThrow();
      expect(gets).toEqual([]);
    });
  });

  describe('맥락: 서버가 공개 계약 밖 필드를 보낸 경우', () => {
    it('given entropy 원시 값이 있는 응답, when 읽으면, then strict decoder가 거절한다', async () => {
      const response = { ...givenHousingResponse('capitalArea'), entropy: 1 };
      const api = createHousingApi({ http: givenRespondingHttp(response) });

      const result = api.listListings();

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 현재 캐릭터나 run이 없는 경우', () => {
    it('given stable characterRequired 409, when 조회하면, then domain 오류로 변환한다', async () => {
      const failure = new HttpError(409, '/api/housing/listings', {
        code: 'characterRequired',
        message: '캐릭터가 필요합니다',
      });
      const api = createHousingApi({ http: givenRejectingHttp(failure) });

      const result = api.listListings();

      await expect(result).rejects.toBeInstanceOf(HousingQueryError);
      await expect(result).rejects.toMatchObject({ code: 'characterRequired' });
    });
  });
});

describe('보유주택과 주택담보대출 매수 protocol', () => {
  describe('맥락: 현재 run의 property holdings를 조회하는 경우', () => {
    it('given owner-occupied capability 응답, when 조회하면, then 전용 GET path를 사용한다', async () => {
      const gets: string[] = [];
      const api = createHousingApi({
        http: givenRecordingGetHttp(givenHoldingsResponse(), gets),
      });

      const result = await api.getHoldings();

      expect(result.purchaseCapability).toBe('ownerOccupiedSingleHome');
      expect(gets).toEqual(['/api/housing/holdings']);
    });
  });

  describe('맥락: sale listing의 주담대 견적을 요청하는 경우', () => {
    it('given cursor와 listing·상품·원금, when 견적을 받으면, then 전용 path와 exact body를 보낸다', async () => {
      const posts: { path: string; body: unknown }[] = [];
      const api = createHousingApi({
        http: givenRecordingPostHttp(givenMortgageQuoteResponse(), posts),
      });

      const result = await api.quoteMortgage(MORTGAGE_QUOTE_REQUEST);

      expect(result.result.quoteId).toBe('9301');
      expect(posts).toEqual([
        { path: '/api/housing/mortgage-quotes', body: MORTGAGE_QUOTE_REQUEST },
      ]);
    });

    it('given 다른 listing의 quote result, when 응답을 읽으면, then preflight 상관관계 위반을 거절한다', async () => {
      const response = givenMortgageQuoteResponse();
      const api = createHousingApi({
        http: givenRespondingHttp({
          ...response,
          result: { ...response.result, listingId: '7003' },
        }),
      });

      const result = api.quoteMortgage(MORTGAGE_QUOTE_REQUEST);

      await expect(result).rejects.toBeDefined();
    });

    it('given client 산정 담보가치를 넣은 body, when 실행하면, then HTTP 전에 strict 요청을 거절한다', () => {
      const posts: { path: string; body: unknown }[] = [];
      const api = createHousingApi({
        http: givenRecordingPostHttp(givenMortgageQuoteResponse(), posts),
      });
      const request = {
        ...MORTGAGE_QUOTE_REQUEST,
        recognizedCollateralValueKrw: 10_000_000,
      } as unknown as HousingMortgageQuoteRequest;

      const whenQuote = () => api.quoteMortgage(request);

      expect(whenQuote).toThrow();
      expect(posts).toEqual([]);
    });
  });

  describe('맥락: eligible quote로 주택을 매수하는 경우', () => {
    it('given listing과 nullable quote ID, when 실행하면, then purchase path와 exact body를 보낸다', async () => {
      const posts: { path: string; body: unknown }[] = [];
      const api = createHousingApi({
        http: givenRecordingPostHttp(givenPurchaseResponse(), posts),
      });

      const result = await api.purchase(PURCHASE_REQUEST);

      expect(result.result.holding.id).toBe('9401');
      expect(posts).toEqual([{ path: '/api/housing/purchases', body: PURCHASE_REQUEST }]);
    });

    it('given null mortgage quote ID, when 현금 매수하면, then 같은 path에 cash body를 보낸다', async () => {
      const posts: { path: string; body: unknown }[] = [];
      const api = createHousingApi({
        http: givenRecordingPostHttp(givenCashPurchaseResponse(), posts),
      });

      const result = await api.purchase(CASH_PURCHASE_REQUEST);

      expect(result.result.mortgageExecution).toBeNull();
      expect(posts).toEqual([{ path: '/api/housing/purchases', body: CASH_PURCHASE_REQUEST }]);
    });

    it('given request와 다른 quote execution, when 응답을 읽으면, then result 상관관계 위반을 거절한다', async () => {
      const response = givenPurchaseResponse();
      const api = createHousingApi({
        http: givenRespondingHttp({
          ...response,
          result: {
            ...response.result,
            mortgageExecution: {
              ...response.result.mortgageExecution,
              quoteId: '9302',
            },
          },
        }),
      });

      const result = api.purchase(PURCHASE_REQUEST);

      await expect(result).rejects.toBeDefined();
    });

    it('given property 장부가가 빠진 net worth, when 응답을 읽으면, then snapshot 자산 모순을 거절한다', async () => {
      const response = givenPurchaseResponse();
      const api = createHousingApi({
        http: givenRespondingHttp({
          ...response,
          snapshot: {
            ...response.snapshot,
            netWorthKrw:
              response.snapshot.netWorthKrw - response.snapshot.life.totalPropertyBookValueKrw,
          },
        }),
      });

      const result = api.purchase(PURCHASE_REQUEST);

      await expect(result).rejects.toBeDefined();
    });

    it('given 주담대 상환 뒤 매수 명령 replay, when 응답을 읽으면, then 과거 receipt와 최신 무담보 snapshot을 허용한다', async () => {
      const response = givenPurchaseResponse();
      const paidOffHolding = { ...response.result.holding, mortgageLoanId: null };
      const api = createHousingApi({
        http: givenRespondingHttp({
          ...response,
          replayed: true,
          snapshot: {
            ...response.snapshot,
            stateRevision: response.snapshot.stateRevision + 1,
            cashKrw: response.snapshot.cashKrw - 4_000_000,
            debtKrw: 0,
            life: {
              ...response.snapshot.life,
              activePropertyHoldings: [paidOffHolding],
              activeLoans: [],
              totalLoanBalanceKrw: 0,
            },
          },
        }),
      });

      const result = await api.purchase(PURCHASE_REQUEST);

      expect(result.replayed).toBe(true);
      expect(result.result.holding.mortgageLoanId).toBe('8402');
      expect(result.snapshot.life.activePropertyHoldings[0]?.mortgageLoanId).toBeNull();
    });
  });
});

describe('주택 매도 주문과 이력 protocol', () => {
  describe('맥락: 보유주택을 처음 매물로 등록하는 경우', () => {
    it('given holding과 주문가를 가진 cursor, when 생성하면, then create path와 exact body를 보낸다', async () => {
      const posts: { path: string; body: unknown }[] = [];
      const api = createHousingApi({
        http: givenRecordingPostHttp(givenPropertySaleCreateResponse(), posts),
      });

      const result = await api.createPropertySaleOrder(PROPERTY_SALE_CREATE_REQUEST);

      expect(result.result.orderId).toBe('9501');
      expect(posts).toEqual([{ path: '/api/housing/sales', body: PROPERTY_SALE_CREATE_REQUEST }]);
    });

    it('given 요청과 다른 holding의 result, when 응답을 읽으면, then resource 상관관계 위반으로 거절한다', async () => {
      const response = givenPropertySaleCreateResponse();
      const api = createHousingApi({
        http: givenRespondingHttp({
          ...response,
          result: { ...response.result, holdingId: '9402' },
        }),
      });

      const result = api.createPropertySaleOrder(PROPERTY_SALE_CREATE_REQUEST);

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 활성 매도 주문의 가격을 바꾸는 경우', () => {
    it('given order ID와 새 주문가 cursor, when 변경하면, then reprice path와 exact body를 보낸다', async () => {
      const posts: { path: string; body: unknown }[] = [];
      const api = createHousingApi({
        http: givenRecordingPostHttp(givenPropertySaleRepriceResponse(), posts),
      });

      const result = await api.repricePropertySaleOrder('9501', PROPERTY_SALE_REPRICE_REQUEST);

      expect(result.result.revisionNo).toBe(2);
      expect(posts).toEqual([
        {
          path: '/api/housing/sales/9501/reprice',
          body: PROPERTY_SALE_REPRICE_REQUEST,
        },
      ]);
    });

    it('given path와 다른 order의 result, when 응답을 읽으면, then resource 상관관계 위반으로 거절한다', async () => {
      const response = givenPropertySaleRepriceResponse();
      const api = createHousingApi({
        http: givenRespondingHttp({
          ...response,
          result: { ...response.result, orderId: '9502' },
        }),
      });

      const result = api.repricePropertySaleOrder('9501', PROPERTY_SALE_REPRICE_REQUEST);

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 활성 매도 주문을 취소하는 경우', () => {
    it('given order ID와 cursor, when 취소하면, then cancel path와 exact body를 보낸다', async () => {
      const posts: { path: string; body: unknown }[] = [];
      const api = createHousingApi({
        http: givenRecordingPostHttp(givenPropertySaleCancellationResponse(), posts),
      });

      const result = await api.cancelPropertySaleOrder('9501', PROPERTY_SALE_CANCEL_REQUEST);

      expect(result.result.status).toBe('cancelled');
      expect(posts).toEqual([
        {
          path: '/api/housing/sales/9501/cancel',
          body: PROPERTY_SALE_CANCEL_REQUEST,
        },
      ]);
    });

    it('given state revision이 전진하지 않은 result, when 응답을 읽으면, then cursor 상관관계 위반으로 거절한다', async () => {
      const response = givenPropertySaleCancellationResponse();
      const api = createHousingApi({
        http: givenRespondingHttp({
          ...response,
          snapshot: { ...response.snapshot, stateRevision: 43 },
        }),
      });

      const result = api.cancelPropertySaleOrder('9501', PROPERTY_SALE_CANCEL_REQUEST);

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 매도 주문 이력을 cursor로 조회하는 경우', () => {
    it('given before와 limit, when 조회하면, then canonical query와 내림차순 page를 사용한다', async () => {
      const gets: string[] = [];
      const api = createHousingApi({
        http: givenRecordingGetHttp(givenPropertySaleOrdersResponse(), gets),
      });

      const result = await api.listPropertySales({ before: '9503', limit: 2 });

      expect(result.items.map((item) => item.orderId)).toEqual(['9502', '9501']);
      expect(gets).toEqual(['/api/housing/sales?before=9503&limit=2']);
    });

    it('given 오름차순 order와 oldest가 아닌 cursor, when 응답을 읽으면, then canonical page 위반으로 거절한다', async () => {
      const response = givenPropertySaleOrdersResponse();
      const api = createHousingApi({
        http: givenRespondingHttp({ ...response, items: [...response.items].reverse() }),
      });

      const result = api.listPropertySales();

      await expect(result).rejects.toBeDefined();
    });

    it('given 공제 합계와 다른 wallet proceeds, when 체결 이력을 읽으면, then sale waterfall 위반으로 거절한다', async () => {
      const order = givenFilledPropertySaleOrderSummary();
      const api = createHousingApi({
        http: givenRespondingHttp({
          items: [
            {
              ...order,
              execution: { ...order.execution, walletProceedsKrw: 5_900_001 },
            },
          ],
          nextBefore: null,
        }),
      });

      const result = api.listPropertySales();

      await expect(result).rejects.toBeDefined();
    });
  });
});

describe('주택 세금 이력 protocol', () => {
  describe('맥락: 이미 매각된 보유주택의 세금 이력을 조회하는 경우', () => {
    it('given disposed holding ID와 pagination, when 조회하면, then exact path로 세 종류의 이력을 읽는다', async () => {
      const gets: string[] = [];
      const api = createHousingApi({
        http: givenRecordingGetHttp(givenPropertyTaxEventsResponse(), gets),
      });

      const result = await api.listPropertyTaxEvents('9401', { before: '9800', limit: 3 });

      expect(result.items.map((item) => item.kind)).toEqual([
        'capitalGains',
        'annualHolding',
        'acquisition',
      ]);
      expect(gets).toEqual(['/api/housing/holdings/9401/tax-events?before=9800&limit=3']);
    });

    it('given path와 다른 holding의 page, when 응답을 읽으면, then holding 상관관계 위반으로 거절한다', async () => {
      const response = givenPropertyTaxEventsResponse();
      const api = createHousingApi({
        http: givenRespondingHttp({
          ...response,
          holdingId: '9402',
          items: response.items.map((item) => ({ ...item, holdingId: '9402' })),
        }),
      });

      const result = api.listPropertyTaxEvents('9401');

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 취득세 component 합계가 응답 총액과 다른 경우', () => {
    it('given 불일치 acquisition component, when 응답을 읽으면, then strict decoder가 거절한다', async () => {
      const event = givenAcquisitionTaxEvent();
      const api = createHousingApi({
        http: givenRespondingHttp({
          holdingId: '9401',
          items: [
            {
              ...event,
              components: event.components.map((component, index) =>
                index === 0 ? { ...component, amountKrw: component.amountKrw + 1 } : component,
              ),
            },
          ],
          nextBefore: null,
        }),
      });

      const result = api.listPropertyTaxEvents('9401');

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 연간 보유세 payment 합계가 응답 총액과 다른 경우', () => {
    it('given 불일치 annual-holding payment, when 응답을 읽으면, then strict decoder가 거절한다', async () => {
      const event = givenAnnualHoldingTaxEvent();
      const api = createHousingApi({
        http: givenRespondingHttp({
          holdingId: '9401',
          items: [
            {
              ...event,
              payments: event.payments.map((payment) =>
                payment.paymentNo === 2
                  ? { ...payment, amountKrw: 3_001, walletPaidKrw: 3_001 }
                  : payment,
              ),
            },
          ],
          nextBefore: null,
        }),
      });

      const result = api.listPropertyTaxEvents('9401');

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 양도세 applied payment의 자금 evidence가 부족한 경우', () => {
    it('given 불일치 capital-gains funding, when 응답을 읽으면, then strict decoder가 거절한다', async () => {
      const event = givenCapitalGainsTaxEvent();
      const api = createHousingApi({
        http: givenRespondingHttp({
          holdingId: '9401',
          items: [
            {
              ...event,
              payments: event.payments.map((payment) => ({
                ...payment,
                walletPaidKrw: payment.walletPaidKrw - 1,
              })),
            },
          ],
          nextBefore: null,
        }),
      });

      const result = api.listPropertyTaxEvents('9401');

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 새 run에서 아직 납부하지 않은 세금 회차를 취소한 경우', () => {
    it('given 지급일과 funding이 없는 cancelled payment, when 응답을 읽으면, then 취소 evidence를 허용한다', async () => {
      const event = givenAcquisitionTaxEvent();
      const cancelled = {
        ...event,
        payments: event.payments.map((payment) => ({ ...payment, status: 'cancelled' })),
      };
      const api = createHousingApi({
        http: givenRespondingHttp({
          holdingId: '9401',
          items: [cancelled],
          nextBefore: null,
        }),
      });

      const result = await api.listPropertyTaxEvents('9401');

      expect(result.items[0]?.payments[0]?.status).toBe('cancelled');
    });

    it('given funding이 생긴 cancelled payment, when 응답을 읽으면, then 상태 evidence 위반으로 거절한다', async () => {
      const event = givenAcquisitionTaxEvent();
      const api = createHousingApi({
        http: givenRespondingHttp({
          holdingId: '9401',
          items: [
            {
              ...event,
              payments: event.payments.map((payment) => ({
                ...payment,
                status: 'cancelled',
                walletPaidKrw: 1,
              })),
            },
          ],
          nextBefore: null,
        }),
      });

      const result = api.listPropertyTaxEvents('9401');

      await expect(result).rejects.toBeDefined();
    });
  });
});

describe('현재 임대차와 이사 명령 protocol', () => {
  describe('맥락: 현재 lease capability를 조회하는 경우', () => {
    it('given strict current lease 응답, when 조회하면, then 고정 path와 보증금 자산을 사용한다', async () => {
      const gets: string[] = [];
      const api = createHousingApi({
        http: givenRecordingGetHttp(givenCurrentLeaseResponse(), gets),
      });

      const result = await api.getCurrentLease();

      expect(gets).toEqual(['/api/housing/leases/current']);
      expect(result.tenantLeaseDepositKrw).toBe(2_000_000);
    });

    it('given 현재 계약과 다른 보증금 합계, when 조회하면, then strict decoder가 거절한다', async () => {
      const response = givenCurrentLeaseResponse();
      const api = createHousingApi({
        http: givenRespondingHttp({ ...response, tenantLeaseDepositKrw: 1 }),
      });

      const result = api.getCurrentLease();

      await expect(result).rejects.toBeDefined();
    });

    it('given v3 월세 terms와 연체 window, when 조회하면, then 공개된 수동 상환 규칙을 사용한다', async () => {
      const api = createHousingApi({
        http: givenRespondingHttp(givenMonthlyCurrentLeaseResponse()),
      });

      const result = await api.getCurrentLease();

      expect(result.monthlyRentTerms).toEqual({
        rentChargeRule: 'nextMonthStartFull',
        arrearRepaymentRule: 'manualOnly',
      });
      expect(result.activeArrears).toHaveLength(1);
    });

    it('given v4 자동갱신 lifecycle 응답, when 조회하면, then 현재 term과 안내를 검증해 공개한다', async () => {
      const api = createHousingApi({
        http: givenRespondingHttp(givenFixedTermCurrentLeaseResponse()),
      });

      const result = await api.getCurrentLease();

      expect(result.leaseLifecycleTerms?.termMonths).toBe(12);
      expect(result.activeLease?.currentTerm?.termNo).toBe(1);
      expect(result.activeLease?.renewalNotice?.renewsOnGameDay).toBe(366);
    });

    it('given model과 다른 active lease 갱신 규칙, when 조회하면, then strict decoder가 거절한다', async () => {
      const response = givenFixedTermCurrentLeaseResponse();
      const api = createHousingApi({
        http: givenRespondingHttp({
          ...response,
          activeLease: { ...response.activeLease, renewalRule: 'openEnded' },
        }),
      });

      const result = api.getCurrentLease();

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 현재 전세 listing의 전세자금대출을 심사하는 경우', () => {
    it('given cursor와 listing·상품·원금, when 견적을 받으면, then 전용 path와 exact body를 보낸다', async () => {
      const posts: { path: string; body: unknown }[] = [];
      const api = createHousingApi({
        http: givenRecordingPostHttp(givenDepositLoanQuoteResponse(), posts),
      });

      const result = await api.quoteLeaseDepositLoan(DEPOSIT_LOAN_QUOTE_REQUEST);

      expect(result.result.quoteId).toBe('8301');
      expect(posts).toEqual([
        {
          path: '/api/housing/lease-deposit-loan-quotes',
          body: DEPOSIT_LOAN_QUOTE_REQUEST,
        },
      ]);
    });

    it('given 다른 listing의 견적 result, when 응답을 읽으면, then 상관관계 위반으로 거절한다', async () => {
      const response = givenDepositLoanQuoteResponse();
      const api = createHousingApi({
        http: givenRespondingHttp({
          ...response,
          result: { ...response.result, listingId: '7003' },
        }),
      });

      const result = api.quoteLeaseDepositLoan(DEPOSIT_LOAN_QUOTE_REQUEST);

      await expect(result).rejects.toBeDefined();
    });

    it('given client 산정 한도를 넣은 견적 body, when 실행하면, then HTTP 전에 strict 요청을 거절한다', () => {
      const posts: { path: string; body: unknown }[] = [];
      const api = createHousingApi({
        http: givenRecordingPostHttp(givenDepositLoanQuoteResponse(), posts),
      });
      const request = {
        ...DEPOSIT_LOAN_QUOTE_REQUEST,
        maximumFundingKrw: 4_000_000,
      } as unknown as HousingLeaseDepositLoanQuoteRequest;

      const whenQuote = () => api.quoteLeaseDepositLoan(request);

      expect(whenQuote).toThrow();
      expect(posts).toEqual([]);
    });
  });

  describe('맥락: 현재 listing의 현금 전세로 이사하는 경우', () => {
    it('given command cursor와 listing, when 실행하면, then 고정 path와 strict body를 보낸다', async () => {
      const posts: { path: string; body: unknown }[] = [];
      const api = createHousingApi({
        http: givenRecordingPostHttp(givenLeaseCommandResponse(), posts),
      });

      const result = await api.startLease(LEASE_REQUEST);

      expect(result.result.listingId).toBe('7002');
      expect(posts).toEqual([{ path: '/api/housing/leases', body: LEASE_REQUEST }]);
    });

    it('given request와 다른 listing result, when 응답을 읽으면, then 상관관계 위반으로 거절한다', async () => {
      const response = givenLeaseCommandResponse();
      const api = createHousingApi({
        http: givenRespondingHttp({
          ...response,
          result: { ...response.result, listingId: '7003' },
        }),
      });

      const result = api.startLease(LEASE_REQUEST);

      await expect(result).rejects.toBeDefined();
    });

    it('given 보증금 자산을 빠뜨린 net worth, when 응답을 읽으면, then 순자산 모순으로 거절한다', async () => {
      const response = givenLeaseCommandResponse();
      const api = createHousingApi({
        http: givenRespondingHttp({
          ...response,
          snapshot: {
            ...response.snapshot,
            netWorthKrw:
              response.snapshot.netWorthKrw - response.snapshot.life.tenantLeaseDepositKrw,
          },
        }),
      });

      const result = api.startLease(LEASE_REQUEST);

      await expect(result).rejects.toBeDefined();
    });

    it('given 공개 요청 밖 이사비, when 실행하면, then HTTP 전에 strict body를 거절한다', () => {
      const posts: { path: string; body: unknown }[] = [];
      const api = createHousingApi({
        http: givenRecordingPostHttp(givenLeaseCommandResponse(), posts),
      });
      const request = {
        ...LEASE_REQUEST,
        movingCostKrw: 450_000,
      } as unknown as HousingLeaseRequest;

      const whenStart = () => api.startLease(request);

      expect(whenStart).toThrow();
      expect(posts).toEqual([]);
    });
  });

  describe('맥락: 현재 listing의 월세로 이사하는 경우', () => {
    it('given monthlyRent offer와 cursor, when 실행하면, then 같은 endpoint에 exact offer kind를 보낸다', async () => {
      const posts: { path: string; body: unknown }[] = [];
      const api = createHousingApi({
        http: givenRecordingPostHttp(givenMonthlyLeaseCommandResponse(), posts),
      });

      const result = await api.startLease(MONTHLY_LEASE_REQUEST);

      expect(result.result.monthlyRentKrw).toBe(650_000);
      expect(posts).toEqual([{ path: '/api/housing/leases', body: MONTHLY_LEASE_REQUEST }]);
    });

    it('given monthlyRent request에 전세 result, when 응답을 읽으면, then offer 상관관계 위반으로 거절한다', async () => {
      const api = createHousingApi({
        http: givenRespondingHttp(givenLeaseCommandResponse()),
      });

      const result = api.startLease(MONTHLY_LEASE_REQUEST);

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: eligible 전세자금대출 견적으로 전세 입주하는 경우', () => {
    it('given quote ID가 있는 financed body, when 실행하면, then 기존 lease path에 exact v2 body를 보낸다', async () => {
      const posts: { path: string; body: unknown }[] = [];
      const api = createHousingApi({
        http: givenRecordingPostHttp(givenFinancedLeaseCommandResponse(), posts),
      });

      const result = await api.startLease(FINANCED_LEASE_REQUEST);

      expect(result.result.depositLoanExecution?.loanId).toBe('8401');
      expect(posts).toEqual([{ path: '/api/housing/leases', body: FINANCED_LEASE_REQUEST }]);
    });

    it('given request와 다른 quote를 실행한 result, when 응답을 읽으면, then 상관관계 위반으로 거절한다', async () => {
      const response = givenFinancedLeaseCommandResponse();
      const api = createHousingApi({
        http: givenRespondingHttp({
          ...response,
          result: {
            ...response.result,
            depositLoanExecution: {
              ...response.result.depositLoanExecution,
              quoteId: '8302',
            },
          },
        }),
      });

      const result = api.startLease(FINANCED_LEASE_REQUEST);

      await expect(result).rejects.toBeDefined();
    });

    it('given monthlyRent에 quote ID를 섞은 body, when 실행하면, then HTTP 전에 strict union이 거절한다', () => {
      const posts: { path: string; body: unknown }[] = [];
      const api = createHousingApi({
        http: givenRecordingPostHttp(givenFinancedLeaseCommandResponse(), posts),
      });
      const request = {
        ...MONTHLY_LEASE_REQUEST,
        loanQuoteId: '8301',
      } as unknown as HousingLeaseRequest;

      const whenStart = () => api.startLease(request);

      expect(whenStart).toThrow();
      expect(posts).toEqual([]);
    });
  });

  describe('맥락: 월세 연체를 수동 상환하는 경우', () => {
    it('given canonical arrear ID와 amount, when 실행하면, then 원래 path와 strict body를 보낸다', async () => {
      const posts: { path: string; body: unknown }[] = [];
      const api = createHousingApi({
        http: givenRecordingPostHttp(givenLeaseArrearPaymentResponse(), posts),
      });

      const result = await api.payLeaseArrear('8101', ARREAR_PAYMENT_REQUEST);

      expect(result.result.paymentId).toBe('8201');
      expect(posts).toEqual([
        {
          path: '/api/housing/lease-arrears/8101/payments',
          body: ARREAR_PAYMENT_REQUEST,
        },
      ]);
    });

    it('given path와 다른 arrear receipt, when 응답을 읽으면, then 상관관계 위반으로 거절한다', async () => {
      const response = givenLeaseArrearPaymentResponse();
      const api = createHousingApi({
        http: givenRespondingHttp({
          ...response,
          result: { ...response.result, arrearId: '8102' },
        }),
      });

      const result = api.payLeaseArrear('8101', ARREAR_PAYMENT_REQUEST);

      await expect(result).rejects.toBeDefined();
    });

    it('given non-canonical path ID, when 실행하면, then HTTP 전에 거절한다', () => {
      const posts: { path: string; body: unknown }[] = [];
      const api = createHousingApi({
        http: givenRecordingPostHttp(givenLeaseArrearPaymentResponse(), posts),
      });

      const whenPay = () => api.payLeaseArrear('08101', ARREAR_PAYMENT_REQUEST);

      expect(whenPay).toThrow();
      expect(posts).toEqual([]);
    });

    it('given 서버 500, when 실행하면, then outcome-unknown transport 오류를 그대로 유지한다', async () => {
      const failure = new HttpError(500, '/api/housing/lease-arrears/8101/payments', {
        code: 'busy',
        message: '결과를 확인할 수 없습니다',
      });
      const api = createHousingApi({ http: givenRejectingHttp(failure) });

      const result = api.payLeaseArrear('8101', ARREAR_PAYMENT_REQUEST);

      await expect(result).rejects.toBe(failure);
      await expect(result).rejects.not.toBeInstanceOf(HousingCommandError);
    });
  });

  describe('맥락: 이사 명령을 서버가 거절한 경우', () => {
    it('given insufficientWalletCash 409, when 실행하면, then outcome-known domain 오류로 변환한다', async () => {
      const failure = new HttpError(409, '/api/housing/leases', {
        code: 'insufficientWalletCash',
        message: '현금이 부족합니다',
      });
      const api = createHousingApi({ http: givenRejectingHttp(failure) });

      const result = api.startLease(LEASE_REQUEST);

      await expect(result).rejects.toBeInstanceOf(HousingCommandError);
      await expect(result).rejects.toMatchObject({ code: 'insufficientWalletCash' });
    });
  });
});

function givenRespondingHttp(response: unknown): HttpClient {
  return {
    async get(_path, decoder) {
      return decoder.parse(response);
    },
    async post(_path, _body, decoder) {
      return decoder.parse(response);
    },
    async put(_path, _body, decoder) {
      return decoder.parse(response);
    },
  };
}

function givenRecordingGetHttp(response: unknown, gets: string[]): HttpClient {
  return {
    async get(path, decoder) {
      gets.push(path);
      return decoder.parse(response);
    },
    async post(_path, _body, decoder) {
      return decoder.parse(response);
    },
    async put(_path, _body, decoder) {
      return decoder.parse(response);
    },
  };
}

function givenRecordingPostHttp(
  response: unknown,
  posts: { path: string; body: unknown }[],
): HttpClient {
  return {
    async get(_path, decoder) {
      return decoder.parse(response);
    },
    async post(path, body, decoder) {
      posts.push({ path, body });
      return decoder.parse(response);
    },
    async put(_path, _body, decoder) {
      return decoder.parse(response);
    },
  };
}

function givenRejectingHttp(error: unknown): HttpClient {
  return {
    async get() {
      throw error;
    },
    async post() {
      throw error;
    },
    async put() {
      throw error;
    },
  };
}

function givenHousingResponse(selectedRegionKey: 'capitalArea' | 'metropolitan') {
  return {
    rateStatus: 'active',
    modelVersionId: '31',
    gameDay: 120,
    yearMonth: { year: 2026, month: 5 },
    residenceRegionKey: 'capitalArea',
    selectedRegionKey,
    regions: [
      { regionKey: 'capitalArea', displayName: '수도권' },
      { regionKey: 'metropolitan', displayName: '광역시' },
      { regionKey: 'smallCity', displayName: '중소도시' },
      { regionKey: 'rural', displayName: '농촌' },
    ],
    priceIndexPpm: 1_021_000,
    rentIndexPpm: 1_011_000,
    listings: [
      {
        id: '7001',
        regionKey: selectedRegionKey,
        propertyType: 'apartment',
        exclusiveAreaSquareMeters: 84,
        availableFromGameDay: 100,
        availableToGameDay: 130,
        offers: [{ kind: 'sale', priceKrw: 420_000_000 }],
      },
    ],
  };
}

function givenCurrentLeaseResponse() {
  return {
    leaseCapability: 'cashJeonse',
    renewalRule: 'openEnded',
    leaseLifecycleTerms: null,
    movingCosts: [
      { regionKey: 'capitalArea', movingCostKrw: 800_000 },
      { regionKey: 'metropolitan', movingCostKrw: 600_000 },
      { regionKey: 'smallCity', movingCostKrw: 450_000 },
      { regionKey: 'rural', movingCostKrw: 300_000 },
    ],
    tenantLeaseDepositKrw: 2_000_000,
    activeLease: {
      id: '8000',
      listingId: '7001',
      depositLoanId: null,
      role: 'tenant',
      offerKind: 'jeonse',
      regionKey: 'capitalArea',
      propertyType: 'apartment',
      exclusiveAreaSquareMeters: 59,
      depositKrw: 2_000_000,
      monthlyRentKrw: null,
      nextRentDueGameDay: null,
      effectiveFromGameDay: 1,
      effectiveToGameDay: null,
      renewalRule: 'openEnded',
      currentTerm: null,
      renewalNotice: null,
      terminationReview: null,
    },
    monthlyRentTerms: null,
    activeArrears: [],
    hasMoreActiveArrears: false,
    totalLeaseArrearKrw: 0,
  };
}

function givenLeaseCommandResponse() {
  return {
    result: {
      leaseId: '8001',
      residenceId: '9001',
      listingId: '7002',
      offerKind: 'jeonse',
      regionKey: 'smallCity',
      propertyType: 'multiFamily',
      exclusiveAreaSquareMeters: 59,
      depositKrw: 5_000_000,
      monthlyRentKrw: null,
      returnedDepositKrw: 2_000_000,
      movingCostKrw: 450_000,
      walletDeltaKrw: -3_450_000,
      effectiveFromGameDay: 17,
      endedLeaseId: '8000',
      renewalRule: 'openEnded',
      depositLoanExecution: null,
      repaidDepositLoan: null,
    },
    replayed: false,
    snapshot: givenLeaseSnapshot(),
  };
}

function givenDepositLoanQuoteResponse() {
  return {
    result: {
      quoteId: '8301',
      listingId: '7002',
      offerKind: 'jeonse',
      productVersionId: '22',
      requestedPrincipalKrw: 4_000_000,
      depositKrw: 5_000_000,
      fundingLimitPpm: 800_000,
      maximumFundingKrw: 4_000_000,
      createdGameDay: 17,
      expiresGameDay: 17,
      decisionCode: 'eligible',
      decisionReasons: ['eligible'],
      verifiedAnnualIncomeKrw: 60_000_000,
      verifiedIncomeSource: 'activeEmploymentContract',
      existingLoanBalanceKrw: 0,
      postExecutionBalanceKrw: 4_000_000,
      regulatoryDsrApplied: false,
      affordability: {
        numeratorKrw: 12_000_000,
        denominatorKrw: 60_000_000,
        ratioPpm: 200_000,
        limitPpm: 400_000,
      },
      quotedTerms: {
        annualRateBp: 400,
        repaymentMethod: 'bullet',
        termMonths: 24,
        firstInstallment: {
          dueGameDay: 31,
          feeKrw: 0,
          principalKrw: 0,
          interestKrw: 61_369,
          totalKrw: 61_369,
        },
      },
      replacedLoanId: null,
      replacedLoanPrincipalKrw: 0,
    },
    replayed: false,
    snapshot: givenLeaseSnapshot(),
  };
}

function givenPropertyHolding() {
  return {
    id: '9401',
    listingId: '7002',
    status: 'active',
    purpose: 'ownerOccupied',
    regionKey: 'smallCity',
    propertyType: 'multiFamily',
    exclusiveAreaSquareMeters: 59,
    acquiredGameDay: 17,
    acquisitionPriceKrw: 10_000_000,
    acquisitionIncidentalCostKrw: 100_000,
    bookValueKrw: 10_000_000,
    mortgageLoanId: '8402',
  };
}

function givenHoldingsResponse() {
  const holding = givenPropertyHolding();
  return {
    purchaseCapability: 'ownerOccupiedSingleHome',
    maximumActiveHoldings: 1,
    holdings: [holding],
    totalPropertyBookValueKrw: holding.bookValueKrw,
  };
}

function givenPropertySaleCreateResponse() {
  return givenPropertySaleListingResponse('9501', '9401', 1, 10_000_000);
}

function givenPropertySaleRepriceResponse() {
  return givenPropertySaleListingResponse('9501', '9401', 2, 9_000_000);
}

function givenPropertySaleListingResponse(
  orderId: string,
  holdingId: string,
  revisionNo: number,
  askingPriceKrw: number,
) {
  const snapshot = givenLeaseSnapshot();
  return {
    result: {
      orderId,
      holdingId,
      revisionNo,
      askingPriceKrw,
      referenceValueKrw: 10_000_000,
      askingToReferencePpm: Math.floor((askingPriceKrw * 1_000_000) / 10_000_000),
      candidateGameDay: 18,
      status: 'active',
    },
    replayed: false,
    snapshot: { ...snapshot, stateRevision: 44 },
  };
}

function givenPropertySaleCancellationResponse() {
  const snapshot = givenLeaseSnapshot();
  return {
    result: {
      orderId: '9501',
      holdingId: '9401',
      revisionNo: 2,
      cancelledGameDay: 17,
      status: 'cancelled',
    },
    replayed: false,
    snapshot: { ...snapshot, stateRevision: 44 },
  };
}

function givenPropertySaleOrdersResponse() {
  const active = givenPropertySaleListingResponse('9502', '9401', 1, 10_000_000).result;
  return {
    items: [
      {
        ...active,
        revisionKind: 'listing',
        cancelledGameDay: null,
        rejectionReason: null,
        execution: null,
      },
      givenFilledPropertySaleOrderSummary(),
    ],
    nextBefore: '9501',
  };
}

function givenFilledPropertySaleOrderSummary() {
  return {
    orderId: '9501',
    holdingId: '9401',
    revisionNo: 1,
    revisionKind: 'listing',
    askingPriceKrw: 10_000_000,
    referenceValueKrw: 10_000_000,
    askingToReferencePpm: 1_000_000,
    candidateGameDay: 20,
    status: 'filled',
    cancelledGameDay: null,
    rejectionReason: null,
    execution: {
      filledGameDay: 20,
      grossSalePriceKrw: 10_000_000,
      transactionCostKrw: 50_000,
      mortgagePrincipalKrw: 4_000_000,
      mortgageFeeKrw: 10_000,
      capitalGainsTaxKrw: 40_000,
      walletProceedsKrw: 5_900_000,
      realizedGainLossKrw: -50_000,
    },
  };
}

function givenPropertyTaxEventsResponse() {
  return {
    holdingId: '9401',
    items: [givenCapitalGainsTaxEvent(), givenAnnualHoldingTaxEvent(), givenAcquisitionTaxEvent()],
    nextBefore: '9701',
  };
}

function givenAcquisitionTaxEvent() {
  return {
    ...givenPropertyTaxEventIdentity('9701', 'acquisition-tax-2026-v1'),
    kind: 'acquisition',
    status: 'scheduled',
    assessedGameDay: 17,
    taxableGameDay: 17,
    paidGameDay: null,
    householdHomeCount: 1,
    grossAmountKrw: 10_000_000,
    valuationGameDay: 17,
    valuationPriceIndexPpm: 1_000_000,
    officialValueKrw: null,
    taxBaseKrw: 10_000_000,
    deductionKrw: 0,
    taxableAmountKrw: 10_000_000,
    totalTaxKrw: 110_000,
    components: [
      givenPropertyTaxComponent('acquisitionTax', 1, 10_000, 100_000),
      givenPropertyTaxComponent('acquisitionLocalEducationTax', 2, 1_000, 10_000),
    ],
    payments: [
      {
        paymentNo: 1,
        dueGameDay: 77,
        paidGameDay: null,
        status: 'pending',
        amountKrw: 110_000,
        walletPaidKrw: 0,
        taxObligationKrw: 0,
      },
    ],
    exclusionCodes: [],
  };
}

function givenAnnualHoldingTaxEvent() {
  return {
    ...givenPropertyTaxEventIdentity('9702', 'annual-property-tax-2026-v1'),
    kind: 'annualHolding',
    status: 'paid',
    assessedGameDay: 150,
    taxableGameDay: 150,
    paidGameDay: 272,
    householdHomeCount: 1,
    grossAmountKrw: 10_000_000,
    valuationGameDay: 149,
    valuationPriceIndexPpm: 1_000_000,
    officialValueKrw: 7_000_000,
    taxBaseKrw: 3_000_000,
    deductionKrw: 0,
    taxableAmountKrw: 3_000_000,
    totalTaxKrw: 6_000,
    components: [
      givenPropertyTaxComponent('annualPropertyTax', 1, 1_667, 5_000),
      givenPropertyTaxComponent('annualLocalEducationTax', 2, 333, 1_000),
    ],
    payments: [
      {
        paymentNo: 1,
        dueGameDay: 212,
        paidGameDay: 212,
        status: 'applied',
        amountKrw: 3_000,
        walletPaidKrw: 2_000,
        taxObligationKrw: 1_000,
      },
      {
        paymentNo: 2,
        dueGameDay: 272,
        paidGameDay: 272,
        status: 'applied',
        amountKrw: 3_000,
        walletPaidKrw: 3_000,
        taxObligationKrw: 0,
      },
    ],
    exclusionCodes: ['cityAreaPortionUnsupported'],
  };
}

function givenCapitalGainsTaxEvent() {
  return {
    ...givenPropertyTaxEventIdentity('9703', 'capital-gains-tax-2026-v1'),
    kind: 'capitalGains',
    status: 'paid',
    assessedGameDay: 500,
    taxableGameDay: 500,
    paidGameDay: 500,
    householdHomeCount: 1,
    grossAmountKrw: 10_000_000,
    valuationGameDay: 500,
    valuationPriceIndexPpm: 1_000_000,
    officialValueKrw: null,
    taxBaseKrw: 5_000_000,
    deductionKrw: 0,
    taxableAmountKrw: 5_000_000,
    totalTaxKrw: 550_000,
    components: [
      givenPropertyTaxComponent('capitalGainsTax', 1, 100_000, 500_000),
      givenPropertyTaxComponent('capitalGainsLocalIncomeTax', 2, 10_000, 50_000),
    ],
    payments: [
      {
        paymentNo: 1,
        dueGameDay: 500,
        paidGameDay: 500,
        status: 'applied',
        amountKrw: 550_000,
        walletPaidKrw: 550_000,
        taxObligationKrw: 0,
      },
    ],
    exclusionCodes: [],
  };
}

function givenPropertyTaxEventIdentity(id: string, ruleKey: string) {
  return {
    id,
    holdingId: '9401',
    policySetId: '61',
    policyKey: 'kr-individual-2026-v1',
    ruleId: '62',
    ruleKey,
    legalBasisDate: '2026-01-01',
  };
}

function givenPropertyTaxComponent(
  componentKey: string,
  componentOrder: number,
  ratePpm: number,
  amountKrw: number,
) {
  return {
    componentKey,
    componentOrder,
    taxBaseKrw: 10_000_000,
    deductionKrw: 0,
    taxableAmountKrw: 10_000_000,
    ratePpm,
    progressiveDeductionKrw: 0,
    amountKrw,
  };
}

function givenMortgageQuoteResponse() {
  return {
    result: {
      quoteId: '9301',
      listingId: '7002',
      productVersionId: '23',
      requestedPrincipalKrw: 4_000_000,
      purchasePriceKrw: 10_000_000,
      recognizedCollateralValueKrw: 10_000_000,
      ltvRegionClass: 'nonRegulatedProxy',
      ltvLimitPpm: 700_000,
      maximumMortgageKrw: 7_000_000,
      ltv: {
        numeratorKrw: 4_000_000,
        denominatorKrw: 10_000_000,
        ratioPpm: 400_000,
        limitPpm: 700_000,
      },
      createdGameDay: 17,
      expiresGameDay: 17,
      decisionCode: 'eligible',
      decisionReasons: ['eligible'],
      verifiedAnnualIncomeKrw: null,
      verifiedIncomeSource: null,
      existingLoanBalanceKrw: 0,
      postExecutionBalanceKrw: 4_000_000,
      dsrApplied: false,
      dsr: null,
      stressRateBp: 0,
      stressTreatment: 'fullTermFixed',
      acquisitionIncidentalCostKrw: 100_000,
      movingCostKrw: 450_000,
      returnedDepositKrw: 5_000_000,
      replacedLoanId: null,
      replacedLoanPrincipalKrw: 0,
      availableBuyerCashKrw: 11_550_000,
      requiredBuyerCashKrw: 6_550_000,
      quotedTerms: {
        annualRateBp: 400,
        repaymentMethod: 'levelPayment',
        termMonths: 360,
        firstInstallment: {
          dueGameDay: 31,
          feeKrw: 0,
          principalKrw: 7_000,
          interestKrw: 60_000,
          totalKrw: 67_000,
        },
      },
    },
    replayed: false,
    snapshot: givenLeaseSnapshot(),
  };
}

function givenPurchaseResponse() {
  const snapshot = givenLeaseSnapshot();
  const holding = givenPropertyHolding();
  const firstInstallment = {
    dueGameDay: 31,
    feeKrw: 0,
    principalKrw: 7_000,
    interestKrw: 60_000,
    totalKrw: 67_000,
  };
  return {
    result: {
      holding,
      residenceId: '9002',
      listingId: '7002',
      purchasePriceKrw: 10_000_000,
      acquisitionIncidentalCostKrw: 100_000,
      movingCostKrw: 450_000,
      returnedDepositKrw: 5_000_000,
      walletDeltaKrw: -1_550_000,
      effectiveFromGameDay: 17,
      endedLeaseId: '8001',
      repaidDepositLoan: null,
      mortgageExecution: {
        loanId: '8402',
        quoteId: '9301',
        productVersionId: '23',
        propertyHoldingId: holding.id,
        principalKrw: 4_000_000,
        activatedGameDay: 17,
        maturityGameDay: 10_975,
        annualRateBp: 400,
        repaymentMethod: 'levelPayment',
        termMonths: 360,
        firstInstallment,
      },
    },
    replayed: false,
    snapshot: {
      ...snapshot,
      stateRevision: 44,
      cashKrw: 5_000_000,
      debtKrw: 4_000_000,
      netWorthKrw: 11_000_000,
      life: {
        ...snapshot.life,
        residence: {
          id: '9002',
          regionKey: 'smallCity',
          tenureKind: 'owner',
          propertyHoldingId: holding.id,
          effectiveFromGameDay: 17,
        },
        tenantLeaseDepositKrw: 0,
        activeLease: null,
        activePropertyHoldings: [holding],
        totalPropertyBookValueKrw: holding.bookValueKrw,
        activeLoans: [
          {
            id: '8402',
            productVersionId: '23',
            productKind: 'mortgage',
            displayName: '개발 주택담보 고정금리 대출',
            rateStatus: 'available',
            currentAnnualRateBp: 400,
            status: 'active',
            remainingPrincipalKrw: 4_000_000,
            overdueKrw: 0,
            readOnly: false,
          },
        ],
        totalLoanBalanceKrw: 4_000_000,
      },
    },
  };
}

function givenCashPurchaseResponse() {
  const response = givenPurchaseResponse();
  const holding = { ...response.result.holding, mortgageLoanId: null };
  return {
    ...response,
    result: {
      ...response.result,
      holding,
      walletDeltaKrw: -5_550_000,
      mortgageExecution: null,
    },
    snapshot: {
      ...response.snapshot,
      cashKrw: 1_000_000,
      debtKrw: 0,
      life: {
        ...response.snapshot.life,
        activePropertyHoldings: [holding],
        activeLoans: [],
        totalLoanBalanceKrw: 0,
      },
    },
  };
}

function givenFinancedLeaseCommandResponse() {
  const response = givenLeaseCommandResponse();
  return {
    ...response,
    result: {
      ...response.result,
      walletDeltaKrw: 550_000,
      depositLoanExecution: {
        loanId: '8401',
        quoteId: '8301',
        productVersionId: '22',
        principalKrw: 4_000_000,
        annualRateBp: 400,
        maturityGameDay: 747,
        firstInstallment: {
          dueGameDay: 31,
          feeKrw: 0,
          principalKrw: 0,
          interestKrw: 61_369,
          totalKrw: 61_369,
        },
      },
    },
    snapshot: {
      ...response.snapshot,
      debtKrw: 4_000_000,
      netWorthKrw: 7_550_000,
      life: {
        ...response.snapshot.life,
        activeLease: {
          ...response.snapshot.life.activeLease,
          depositLoanId: '8401',
        },
        activeLoans: [
          {
            id: '8401',
            productVersionId: '22',
            productKind: 'leaseDepositLoan',
            displayName: '개발 전세자금 고정금리 대출',
            rateStatus: 'available',
            currentAnnualRateBp: 400,
            status: 'active',
            remainingPrincipalKrw: 4_000_000,
            overdueKrw: 0,
            readOnly: false,
          },
        ],
        totalLoanBalanceKrw: 4_000_000,
      },
    },
  };
}

function givenMonthlyCurrentLeaseResponse() {
  const response = givenCurrentLeaseResponse();
  return {
    ...response,
    leaseCapability: 'cashJeonseAndMonthlyRent',
    monthlyRentTerms: {
      rentChargeRule: 'nextMonthStartFull',
      arrearRepaymentRule: 'manualOnly',
    },
    activeArrears: [
      {
        id: '8101',
        leaseId: '7999',
        rentChargeId: '8051',
        dueYearMonth: { year: 2026, month: 1 },
        originalKrw: 650_000,
        paidKrw: 150_000,
        remainingKrw: 500_000,
        createdGameDay: 15,
      },
    ],
    totalLeaseArrearKrw: 500_000,
  };
}

function givenFixedTermCurrentLeaseResponse() {
  const response = givenMonthlyCurrentLeaseResponse();
  return {
    ...response,
    renewalRule: 'fixedTermAutoRenew',
    leaseLifecycleTerms: {
      termMonths: 12,
      renewalNoticeLeadDays: 30,
      monthlyRentTerminationReview: {
        rule: 'oldestActiveArrearAge',
        afterGameDays: 60,
      },
    },
    activeLease: {
      ...response.activeLease,
      renewalRule: 'fixedTermAutoRenew',
      currentTerm: {
        termNo: 1,
        effectiveFromGameDay: 1,
        effectiveToGameDay: 366,
      },
      renewalNotice: {
        termNo: 1,
        publishedGameDay: 336,
        renewsOnGameDay: 366,
      },
      terminationReview: null,
    },
  };
}

function givenMonthlyLeaseCommandResponse() {
  const response = givenLeaseCommandResponse();
  return {
    ...response,
    result: {
      ...response.result,
      offerKind: 'monthlyRent',
      monthlyRentKrw: 650_000,
    },
    snapshot: {
      ...response.snapshot,
      life: {
        ...response.snapshot.life,
        residence: {
          id: '9001',
          regionKey: 'smallCity',
          tenureKind: 'monthlyRent',
          propertyHoldingId: null,
          effectiveFromGameDay: 17,
        },
        activeLease: {
          id: '8001',
          listingId: '7002',
          depositLoanId: null,
          role: 'tenant',
          offerKind: 'monthlyRent',
          regionKey: 'smallCity',
          propertyType: 'multiFamily',
          exclusiveAreaSquareMeters: 59,
          depositKrw: 5_000_000,
          monthlyRentKrw: 650_000,
          nextRentDueGameDay: 31,
          effectiveFromGameDay: 17,
          effectiveToGameDay: null,
          renewalRule: 'openEnded',
          currentTerm: null,
          renewalNotice: null,
          terminationReview: null,
        },
      },
    },
  };
}

function givenLeaseArrearPaymentResponse() {
  return {
    result: {
      arrearId: '8101',
      paymentId: '8201',
      paidKrw: 200_000,
      remainingKrw: 0,
    },
    replayed: false,
    snapshot: givenLeaseSnapshot(),
  };
}

function givenLeaseSnapshot(): GameSnapshot {
  return {
    runRevision: 3,
    stateRevision: 43,
    gameDay: 17,
    startDate: '2026-01-01',
    cashKrw: 6_550_000,
    debtKrw: 0,
    netWorthKrw: 11_550_000,
    characterName: '테스터',
    autoSpeed: null,
    market: {
      world: 'm1-2026-v3',
      date: '2026-01-18',
      open: true,
      regime: 'expansion',
      index: {
        symbol: 'LLX',
        name: '라이프 한국 종합지수',
        closeKrw: 100_000,
        dailyReturnPpm: 0,
      },
      rates: null,
      m2Factors: null,
    },
    portfolio: { positions: [], marketValueKrw: 0 },
    finance: {
      policySet: { key: 'kr-individual-2026-v1', basisDate: '2026-01-01' },
      accounts: [],
      cmaAccounts: [],
      cashContracts: [],
      depositProtection: [],
      currentTaxYear: {
        taxYear: 2026,
        status: 'notApplicable',
        sources: [],
        grossFinancialIncomeKrw: 0,
        withheldIncomeTaxKrw: 0,
        withheldLocalIncomeTaxKrw: 0,
        comparisonAIncomeTaxKrw: null,
        comparisonALocalIncomeTaxKrw: null,
        comparisonBIncomeTaxKrw: null,
        comparisonBLocalIncomeTaxKrw: null,
        assessedIncomeTaxKrw: null,
        assessedLocalIncomeTaxKrw: null,
        additionalTaxKrw: null,
        refundKrw: null,
        filingDueDate: null,
        filedGameDay: null,
      },
      isaAccounts: [],
      pensionAccounts: [],
      productBundle: null,
      llxDistributionEntitlements: [],
      bondPositions: [],
      goldAccounts: [],
      physicalGoldHoldings: [],
      latestFinancialIncomeAssessment: null,
      pendingSettlements: [],
    },
    career: {
      focusedJobFamilyKey: 'softwareEngineering',
      possessedScores: {
        education: 0,
        certification: 0,
        language: 0,
        training: 0,
        experience: 0,
        project: 0,
      },
      activeActivities: [],
      latestArtifacts: [],
      openApplications: [],
      openInvitations: [],
      employment: null,
      latestPayroll: null,
      currentEmploymentTaxYear: {
        taxYear: 2026,
        status: 'open',
        source: 'employmentOnly',
        grossEmploymentIncomeKrw: 0,
        employeeInsuranceDeductionKrw: 0,
        earnedIncomeDeductionKrw: null,
        personalDeductionKrw: null,
        taxableIncomeKrw: null,
        calculatedIncomeTaxKrw: null,
        earnedIncomeTaxCreditKrw: null,
        pensionCreditEligibleContributionKrw: null,
        actualPensionIncomeTaxCreditKrw: null,
        actualPensionLocalIncomeTaxEffectKrw: null,
        withheldIncomeTaxKrw: 0,
        withheldLocalIncomeTaxKrw: 0,
        assessedIncomeTaxKrw: null,
        assessedLocalIncomeTaxKrw: null,
        additionalTaxKrw: null,
        refundKrw: null,
        reconciliationGameDay: null,
      },
      latestEmploymentTaxAssessment: null,
      militaryStatus: 'unserved',
      activeMilitaryService: null,
      activeMilitarySavings: [],
      pendingCareerSchedule: [],
    },
    life: {
      rateStatus: 'active',
      household: {
        id: '1',
        memberCount: 1,
        dependentCount: 0,
        taxDependentEligibleCount: 0,
      },
      residence: {
        id: '9001',
        regionKey: 'smallCity',
        tenureKind: 'jeonse',
        propertyHoldingId: null,
        effectiveFromGameDay: 17,
      },
      tenantLeaseDepositKrw: 5_000_000,
      activeLease: {
        id: '8001',
        listingId: '7002',
        depositLoanId: null,
        role: 'tenant',
        offerKind: 'jeonse',
        regionKey: 'smallCity',
        propertyType: 'multiFamily',
        exclusiveAreaSquareMeters: 59,
        depositKrw: 5_000_000,
        monthlyRentKrw: null,
        nextRentDueGameDay: null,
        effectiveFromGameDay: 17,
        effectiveToGameDay: null,
        renewalRule: 'openEnded',
        currentTerm: null,
        renewalNotice: null,
        terminationReview: null,
      },
      activeLeaseArrears: [],
      hasMoreActiveLeaseArrears: false,
      totalLeaseArrearKrw: 0,
      activePropertyHoldings: [],
      hasMoreActivePropertyHoldings: false,
      totalPropertyBookValueKrw: 0,
      currentMonth: null,
      activeArrears: [],
      hasMoreActiveArrears: false,
      totalEssentialArrearKrw: 0,
      creditBand: 'standard',
      creditReasons: ['cleanHistory'],
      activeLoans: [],
      nextLoanInstallment: null,
      totalLoanBalanceKrw: 0,
      activeWelfareApplications: [],
      insuranceCapability: 'unavailable',
      activeInsuranceContracts: [],
      pendingInsuranceClaims: [],
      pendingEvents: [],
      insolvency: {
        availability: 'unavailable',
        eligibility: 'unavailable',
        reasons: ['componentUnavailable'],
        currentCase: null,
      },
    },
  };
}
