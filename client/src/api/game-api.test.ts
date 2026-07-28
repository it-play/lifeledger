import { describe, expect, it } from '@jest/globals';
import type { HttpClient } from '../lib/http/index.js';
import { HttpError } from '../lib/http/index.js';
import type { SseClient } from '../lib/sse/index.js';
import type {
  AdvanceRequest,
  BondOrderRequest,
  CmaAccountCloseRequest,
  CmaAccountOpenRequest,
  DepositCloseRequest,
  DepositOpenRequest,
  FinanceTransferRequest,
  FinancialIncomeYear,
  GameSnapshot,
  GoldAccountOpenRequest,
  GoldOrderRequest,
  GoldWithdrawalRequest,
  LedgerPage,
  PensionStartRequest,
  PensionWithdrawalRequest,
  PortfolioOrderRequest,
  TaxAccountOpenRequest,
} from './contracts.js';
import {
  createGameApi,
  FinanceCommandError,
  GameCommandError,
  PortfolioOrderError,
} from './game-api.js';

const givenOrder = (): PortfolioOrderRequest => ({
  orderId: '4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2',
  accountId: '1',
  expectedRunRevision: 3,
  expectedStateRevision: 42,
  expectedGameDay: 17,
  side: 'buy',
  symbol: 'LLX',
  quantity: 10,
});

const givenStream = (): SseClient => ({
  status: 'idle',
  lastEventId: '',
  on: () => () => {},
  onAny: () => () => {},
  onStatusChange: () => () => {},
  connect: () => {},
  close: () => {},
  dispose: () => {},
});

const givenTransfer = (): FinanceTransferRequest => ({
  commandId: '4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2',
  expectedRunRevision: 3,
  expectedStateRevision: 42,
  expectedGameDay: 17,
  accountId: '1',
  direction: 'walletToAccount',
  amountKrw: 100_000,
});

const givenFinanceCommand = (): CmaAccountCloseRequest & DepositCloseRequest => ({
  commandId: '4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2',
  expectedRunRevision: 3,
  expectedStateRevision: 42,
  expectedGameDay: 17,
});

const givenBondOrder = (): BondOrderRequest => ({
  ...givenFinanceCommand(),
  accountId: '1',
  seriesId: '21',
  side: 'buy',
  bondUnits: 10,
});

const givenGoldAccountOpen = (): GoldAccountOpenRequest => ({
  ...givenFinanceCommand(),
  type: 'krxGold',
  productVersionId: '31',
});

const givenGoldOrder = (): GoldOrderRequest => ({
  ...givenFinanceCommand(),
  accountId: '6',
  side: 'buy',
  quantityGram: 10,
});

const givenGoldWithdrawal = (): GoldWithdrawalRequest => ({
  ...givenFinanceCommand(),
  accountId: '6',
  barSizeGram: 100,
  barCount: 2,
});

const givenLegacyFinancialIncomeYear = (taxYear = 2026): FinancialIncomeYear => ({
  taxYear,
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
});

const givenCmaAccountOpen = (): CmaAccountOpenRequest => ({
  ...givenFinanceCommand(),
  type: 'cma',
  productVersionId: '11',
});

const givenDepositOpen = (): DepositOpenRequest => ({
  ...givenFinanceCommand(),
  kind: 'termDeposit',
  productVersionId: '12',
  settlementAccountId: '1',
  amountKrw: 1_000_000,
});

const givenTaxAccountOpen = (): TaxAccountOpenRequest => ({
  ...givenFinanceCommand(),
  type: 'isaGeneral',
});

const givenPensionStart = (): PensionStartRequest => ({
  ...givenFinanceCommand(),
  paymentYears: 10,
  lifetime: false,
});

const givenPensionWithdrawal = (): PensionWithdrawalRequest => ({
  ...givenFinanceCommand(),
  amountKrw: 100_000,
  type: 'nonPension',
  reason: null,
});

const givenAdvance = (): AdvanceRequest => ({
  commandId: '4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2',
  expectedRunRevision: 3,
  expectedStateRevision: 42,
  expectedGameDay: 17,
  days: 7,
});

describe('수동 진행 명령 검증', () => {
  describe('맥락: 서버가 확정적인 명령 충돌을 반환한 경우', () => {
    it('given 멱등 충돌 본문, when 진행하면, then 타입이 있는 도메인 오류를 던진다', async () => {
      const http: HttpClient = {
        get<T>() {
          return Promise.reject(new Error('unexpected GET')) as Promise<T>;
        },
        post<T>() {
          return Promise.reject(
            new HttpError(409, '/api/advance', {
              code: 'idempotencyConflict',
              message: '같은 명령 ID가 다른 요청에 사용되었습니다',
            }),
          ) as Promise<T>;
        },
        put<T>() {
          return Promise.reject(new Error('unexpected PUT')) as Promise<T>;
        },
      };
      const api = createGameApi({ http, stream: givenStream() });

      const result = api.advance(givenAdvance());

      await expect(result).rejects.toEqual(
        new GameCommandError('idempotencyConflict', '같은 명령 ID가 다른 요청에 사용되었습니다'),
      );
    });
  });

  describe('맥락: 완료 cursor가 제출한 일수와 맞지 않는 경우', () => {
    it('given 7일 요청과 6일 결과, when 응답을 읽으면, then 계약 위반으로 거절한다', async () => {
      const request = givenAdvance();
      const http = givenRespondingHttp({
        advance: {
          commandId: request.commandId,
          requestedDays: request.days,
          initialCursor: { runRevision: 3, stateRevision: 42, gameDay: 17 },
          committedCursor: { runRevision: 3, stateRevision: 48, gameDay: 23 },
          replayed: false,
        },
        snapshot: givenSnapshot(),
      });
      const api = createGameApi({ http, stream: givenStream() });

      const result = api.advance(request);

      await expect(result).rejects.toBeDefined();
    });
  });
});

describe('포트폴리오 주문 오류 변환', () => {
  describe('맥락: 서버가 알려진 주문 실패 코드를 반환한 경우', () => {
    it('given 상태 코드와 실패 본문, when 주문하면, then 타입이 있는 도메인 오류를 던진다', async () => {
      const http: HttpClient = {
        get<T>() {
          return Promise.reject(new Error('unexpected GET')) as Promise<T>;
        },
        post<T>() {
          return Promise.reject(
            new HttpError(409, '/api/portfolio/orders', {
              code: 'insufficientAccountCash',
              message: '주문에 필요한 계좌 현금이 부족합니다',
            }),
          ) as Promise<T>;
        },
        put<T>() {
          return Promise.reject(new Error('unexpected PUT')) as Promise<T>;
        },
      };
      const api = createGameApi({ http, stream: givenStream() });

      const result = api.placePortfolioOrder(givenOrder());

      await expect(result).rejects.toEqual(
        new PortfolioOrderError('insufficientAccountCash', '주문에 필요한 계좌 현금이 부족합니다'),
      );
    });
  });
});

describe('금융 명령 오류 변환', () => {
  describe('맥락: 서버가 알려진 금융 실패 코드를 반환한 경우', () => {
    it('given 잔액 부족 본문, when 이체하면, then 타입이 있는 도메인 오류를 던진다', async () => {
      const http: HttpClient = {
        get<T>() {
          return Promise.reject(new Error('unexpected GET')) as Promise<T>;
        },
        post<T>() {
          return Promise.reject(
            new HttpError(409, '/api/finance/transfers', {
              code: 'insufficientWalletCash',
              message: '지갑 잔액이 부족합니다',
            }),
          ) as Promise<T>;
        },
        put<T>() {
          return Promise.reject(new Error('unexpected PUT')) as Promise<T>;
        },
      };
      const api = createGameApi({ http, stream: givenStream() });

      const result = api.transferFinance(givenTransfer());

      await expect(result).rejects.toEqual(
        new FinanceCommandError('insufficientWalletCash', '지갑 잔액이 부족합니다'),
      );
    });
  });

  describe('맥락: 이체 응답이 제출한 명령과 다른 경우', () => {
    it('given 다른 command ID의 정상 모양 응답, when 이체하면, then 계약 위반으로 거절한다', async () => {
      const request = givenTransfer();
      const http = givenRespondingHttp({
        transfer: {
          commandId: '00000000-0000-0000-0000-000000000999',
          accountId: request.accountId,
          direction: request.direction,
          amountKrw: request.amountKrw,
          replayed: false,
        },
        snapshot: givenSnapshot(),
      });
      const api = createGameApi({ http, stream: givenStream() });

      const result = api.transferFinance(request);

      await expect(result).rejects.toBeDefined();
    });
  });
});

describe('현금상품 카탈로그 조회', () => {
  describe('맥락: 게시된 CMA 상품이 있는 경우', () => {
    it('given 용도별 금액 필드가 맞는 상품, when 조회하면, then 고정 경로의 검증된 목록을 반환한다', async () => {
      const catalog = {
        products: [
          {
            id: '11',
            key: 'cma-rp-2026-v1',
            kind: 'cmaRp',
            displayName: 'RP형 CMA',
            institution: { id: '1', key: 'life-securities', displayName: '라이프증권' },
            protectionEligible: false,
            rateReference: 'treasury3mBp',
            spreadBp: 20,
            minimumInterestBalanceKrw: 10_000,
            dayCountDenominator: 365,
          },
        ],
      };
      const capture = givenHttpCapture();
      const api = createGameApi({
        http: givenCapturingHttp(catalog, capture),
        stream: givenStream(),
      });

      const result = await api.listCashProducts();

      expect({ result, capture }).toEqual({
        result: catalog,
        capture: { method: 'GET', path: '/api/finance/cash-products', body: null },
      });
    });
  });
});

describe('CMA 계좌 명령 상관관계', () => {
  describe('맥락: 개설 결과가 요청 상품과 일치하는 경우', () => {
    it('given CMA 상품과 공통 cursor, when 개설하면, then 고정 경로와 검증한 body를 사용한다', async () => {
      const request = givenCmaAccountOpen();
      const capture = givenHttpCapture();
      const http = givenCapturingHttp(
        {
          account: {
            commandId: request.commandId,
            accountId: '2',
            productVersionId: request.productVersionId,
            replayed: false,
          },
          snapshot: givenSnapshot(),
        },
        capture,
      );
      const api = createGameApi({ http, stream: givenStream() });

      await api.openCmaAccount(request);

      expect(capture).toEqual({
        method: 'POST',
        path: '/api/finance/accounts',
        body: request,
      });
    });
  });

  describe('맥락: 개설 결과가 다른 상품을 가리키는 경우', () => {
    it('given 제출하지 않은 product ID, when 응답을 읽으면, then 계약 위반으로 거절한다', async () => {
      const request = givenCmaAccountOpen();
      const api = createGameApi({
        http: givenRespondingHttp({
          account: {
            commandId: request.commandId,
            accountId: '2',
            productVersionId: '99',
            replayed: false,
          },
          snapshot: givenSnapshot(),
        }),
        stream: givenStream(),
      });

      const result = api.openCmaAccount(request);

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 종료 결과가 경로와 다른 계좌를 가리키는 경우', () => {
    it('given account ID가 다른 정상 모양 결과, when 응답을 읽으면, then 계약 위반으로 거절한다', async () => {
      const request = givenFinanceCommand();
      const api = createGameApi({
        http: givenRespondingHttp({
          accountClose: { commandId: request.commandId, accountId: '3', replayed: false },
          snapshot: givenSnapshot(),
        }),
        stream: givenStream(),
      });

      const result = api.closeCmaAccount('2', request);

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: CMA 현금이 남아 서버가 종료를 거절한 경우', () => {
    it('given accountNotEmpty 실패, when 종료하면, then 금융 도메인 오류로 변환한다', async () => {
      const http: HttpClient = {
        get<T>() {
          return Promise.reject(new Error('unexpected GET')) as Promise<T>;
        },
        post<T>() {
          return Promise.reject(
            new HttpError(409, '/api/finance/accounts/2/close', {
              code: 'accountNotEmpty',
              message: 'CMA 현금을 먼저 이체하세요',
            }),
          ) as Promise<T>;
        },
        put<T>() {
          return Promise.reject(new Error('unexpected PUT')) as Promise<T>;
        },
      };
      const api = createGameApi({ http, stream: givenStream() });

      const result = api.closeCmaAccount('2', givenFinanceCommand());

      await expect(result).rejects.toEqual(
        new FinanceCommandError('accountNotEmpty', 'CMA 현금을 먼저 이체하세요'),
      );
    });
  });
});

describe('절세계좌 개설·해지 상관관계', () => {
  describe('맥락: ISA 개설 결과가 요청 유형과 일치하는 경우', () => {
    it('given 일반형 ISA와 공통 cursor, when 개설하면, then 금융계좌 고정 경로와 검증한 body를 사용한다', async () => {
      const request = givenTaxAccountOpen();
      const capture = givenHttpCapture();
      const api = createGameApi({
        http: givenCapturingHttp(
          {
            account: {
              commandId: request.commandId,
              accountId: '2',
              type: request.type,
              replayed: false,
            },
            snapshot: givenSnapshot(),
          },
          capture,
        ),
        stream: givenStream(),
      });

      await api.openTaxAccount(request);

      expect(capture).toEqual({
        method: 'POST',
        path: '/api/finance/accounts',
        body: request,
      });
    });
  });

  describe('맥락: 개설 결과가 다른 절세계좌 유형을 가리키는 경우', () => {
    it('given 일반형 ISA 요청과 IRP 결과, when 응답을 읽으면, then 계약 위반으로 거절한다', async () => {
      const request = givenTaxAccountOpen();
      const api = createGameApi({
        http: givenRespondingHttp({
          account: {
            commandId: request.commandId,
            accountId: '2',
            type: 'irp',
            replayed: false,
          },
          snapshot: givenSnapshot(),
        }),
        stream: givenStream(),
      });

      const result = api.openTaxAccount(request);

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: ISA 해지 결과가 경로 계좌와 일치하는 경우', () => {
    it('given 세금 정산 영수증, when 해지하면, then ISA 전용 경로를 사용한다', async () => {
      const request = givenFinanceCommand();
      const capture = givenHttpCapture();
      const api = createGameApi({
        http: givenCapturingHttp(
          {
            isaClose: {
              commandId: request.commandId,
              accountId: '2',
              grossTaxProfitKrw: 50_000,
              deductibleLossKrw: 0,
              incomeTaxKrw: 7_000,
              localIncomeTaxKrw: 700,
              netPayoutKrw: 1_042_300,
              replayed: false,
            },
            snapshot: givenSnapshot(),
          },
          capture,
        ),
        stream: givenStream(),
      });

      await api.closeIsaAccount('2', request);

      expect(capture).toEqual({
        method: 'POST',
        path: '/api/finance/isa/2/close',
        body: request,
      });
    });
  });

  describe('맥락: ISA 해지 결과가 다른 계좌를 가리키는 경우', () => {
    it('given 경로와 다른 account ID, when 응답을 읽으면, then 계약 위반으로 거절한다', async () => {
      const request = givenFinanceCommand();
      const api = createGameApi({
        http: givenRespondingHttp({
          isaClose: {
            commandId: request.commandId,
            accountId: '9',
            grossTaxProfitKrw: 0,
            deductibleLossKrw: 0,
            incomeTaxKrw: 0,
            localIncomeTaxKrw: 0,
            netPayoutKrw: 0,
            replayed: false,
          },
          snapshot: givenSnapshot(),
        }),
        stream: givenStream(),
      });

      const result = api.closeIsaAccount('2', request);

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 같은 ISA가 이미 있어 개설이 거절된 경우', () => {
    it('given accountAlreadyExists 본문, when 개설하면, then 금융 도메인 오류로 변환한다', async () => {
      const http: HttpClient = {
        get<T>() {
          return Promise.reject(new Error('unexpected GET')) as Promise<T>;
        },
        post<T>() {
          return Promise.reject(
            new HttpError(409, '/api/finance/accounts', {
              code: 'accountAlreadyExists',
              message: '같은 종류의 계좌가 이미 있습니다',
            }),
          ) as Promise<T>;
        },
        put<T>() {
          return Promise.reject(new Error('unexpected PUT')) as Promise<T>;
        },
      };
      const api = createGameApi({ http, stream: givenStream() });

      const result = api.openTaxAccount(givenTaxAccountOpen());

      await expect(result).rejects.toEqual(
        new FinanceCommandError('accountAlreadyExists', '같은 종류의 계좌가 이미 있습니다'),
      );
    });
  });
});

describe('연금 개시·인출 상관관계', () => {
  describe('맥락: 개시 결과가 계좌와 지급조건에 일치하는 경우', () => {
    it('given 10년 확정 지급, when 개시하면, then 연금 전용 경로와 검증한 body를 사용한다', async () => {
      const request = givenPensionStart();
      const capture = givenHttpCapture();
      const api = createGameApi({
        http: givenCapturingHttp(
          {
            pensionStart: {
              commandId: request.commandId,
              accountId: '3',
              startTaxYear: 2061,
              paymentYears: request.paymentYears,
              lifetime: request.lifetime,
              replayed: false,
            },
            snapshot: givenSnapshot(),
          },
          capture,
        ),
        stream: givenStream(),
      });

      await api.startPension('3', request);

      expect(capture).toEqual({
        method: 'POST',
        path: '/api/finance/pensions/3/start',
        body: request,
      });
    });
  });

  describe('맥락: 개시 결과의 지급조건이 요청과 다른 경우', () => {
    it('given 10년 확정 요청과 종신 결과, when 응답을 읽으면, then 계약 위반으로 거절한다', async () => {
      const request = givenPensionStart();
      const api = createGameApi({
        http: givenRespondingHttp({
          pensionStart: {
            commandId: request.commandId,
            accountId: '3',
            startTaxYear: 2061,
            paymentYears: request.paymentYears,
            lifetime: true,
            replayed: false,
          },
          snapshot: givenSnapshot(),
        }),
        stream: givenStream(),
      });

      const result = api.startPension('3', request);

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 인출 결과가 계좌와 gross 금액에 일치하는 경우', () => {
    it('given required null 사유와 연금외 인출, when 실행하면, then null을 보존한 body를 전송한다', async () => {
      const request = givenPensionWithdrawal();
      const capture = givenHttpCapture();
      const api = createGameApi({
        http: givenCapturingHttp(
          {
            pensionWithdrawal: {
              commandId: request.commandId,
              accountId: '3',
              grossAmountKrw: request.amountKrw,
              pensionAmountKrw: 0,
              nonPensionAmountKrw: request.amountKrw,
              taxFreeAmountKrw: request.amountKrw,
              taxKrw: 0,
              netPayoutKrw: request.amountKrw,
              replayed: false,
            },
            snapshot: givenSnapshot(),
          },
          capture,
        ),
        stream: givenStream(),
      });

      await api.withdrawPension('3', request);

      expect(capture).toEqual({
        method: 'POST',
        path: '/api/finance/pensions/3/withdrawals',
        body: request,
      });
      expect(capture.body).toHaveProperty('reason', null);
    });
  });

  describe('맥락: 인출 결과 gross가 제출 금액과 다른 경우', () => {
    it('given 10만원 요청과 9만원 결과, when 응답을 읽으면, then 계약 위반으로 거절한다', async () => {
      const request = givenPensionWithdrawal();
      const api = createGameApi({
        http: givenRespondingHttp({
          pensionWithdrawal: {
            commandId: request.commandId,
            accountId: '3',
            grossAmountKrw: 90_000,
            pensionAmountKrw: 0,
            nonPensionAmountKrw: 90_000,
            taxFreeAmountKrw: 90_000,
            taxKrw: 0,
            netPayoutKrw: 90_000,
            replayed: false,
          },
          snapshot: givenSnapshot(),
        }),
        stream: givenStream(),
      });

      const result = api.withdrawPension('3', request);

      await expect(result).rejects.toBeDefined();
    });
  });
});

describe('예금·적금 명령 상관관계', () => {
  describe('맥락: 가입 결과의 금액이 제출한 금액과 다른 경우', () => {
    it('given 정상 모양의 다른 가입 결과, when 응답을 읽으면, then 계약 위반으로 거절한다', async () => {
      const request = givenDepositOpen();
      const http = givenRespondingHttp({
        deposit: {
          commandId: request.commandId,
          contractId: '21',
          kind: request.kind,
          productVersionId: request.productVersionId,
          settlementAccountId: request.settlementAccountId,
          amountKrw: request.amountKrw + 1,
          replayed: false,
        },
        snapshot: givenSnapshot(),
      });
      const api = createGameApi({ http, stream: givenStream() });

      const result = api.openDeposit(request);

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 중도해지 경로 ID가 canonical decimal이 아닌 경우', () => {
    it('given 선행 0이 있는 계약 ID, when 종료하면, then 요청 전에 거절한다', async () => {
      const api = createGameApi({ http: givenRespondingHttp({}), stream: givenStream() });

      const result = api.closeDeposit('021', givenFinanceCommand());

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 중도해지 결과가 경로 계약과 일치하는 경우', () => {
    it('given 세후 지급 결과, when 종료하면, then canonical 계약 경로를 사용한다', async () => {
      const request = givenFinanceCommand();
      const capture = givenHttpCapture();
      const http = givenCapturingHttp(
        {
          depositClose: {
            commandId: request.commandId,
            contractId: '21',
            grossInterestKrw: 10_000,
            incomeTaxKrw: 1_400,
            localIncomeTaxKrw: 140,
            netPayoutKrw: 1_008_460,
            replayed: false,
          },
          snapshot: givenSnapshot(),
        },
        capture,
      );
      const api = createGameApi({ http, stream: givenStream() });

      await api.closeDeposit('21', request);

      expect(capture).toEqual({
        method: 'POST',
        path: '/api/finance/deposits/21/close',
        body: request,
      });
    });
  });

  describe('맥락: 중도해지 결과가 경로와 다른 계약을 가리키는 경우', () => {
    it('given contract ID가 다른 정상 모양 결과, when 응답을 읽으면, then 계약 위반으로 거절한다', async () => {
      const request = givenFinanceCommand();
      const api = createGameApi({
        http: givenRespondingHttp({
          depositClose: {
            commandId: request.commandId,
            contractId: '22',
            grossInterestKrw: 10_000,
            incomeTaxKrw: 1_400,
            localIncomeTaxKrw: 140,
            netPayoutKrw: 1_008_460,
            replayed: false,
          },
          snapshot: givenSnapshot(),
        }),
        stream: givenStream(),
      });

      const result = api.closeDeposit('21', request);

      await expect(result).rejects.toBeDefined();
    });
  });
});

describe('국채 카탈로그와 주문 API', () => {
  describe('맥락: 과거 월드에 게시된 국채가 없는 경우', () => {
    it('given 빈 상품과 시리즈, when 조회하면, then 국채 고정 경로를 사용한다', async () => {
      const catalog = { marketVersion: 'm1-2026-v3', products: [], series: [] };
      const capture = givenHttpCapture();
      const api = createGameApi({
        http: givenCapturingHttp(catalog, capture),
        stream: givenStream(),
      });

      const result = await api.listBonds();

      expect({ result, capture }).toEqual({
        result: catalog,
        capture: { method: 'GET', path: '/api/finance/bonds', body: null },
      });
    });
  });

  describe('맥락: 이전 카탈로그 조회를 취소할 수 있어야 하는 경우', () => {
    it('given AbortSignal, when 국채를 조회하면, then 같은 신호를 HTTP GET에 전달한다', async () => {
      const catalog = { marketVersion: 'm1-2026-v3', products: [], series: [] };
      const capture = givenHttpCapture();
      const signal = new AbortController().signal;
      const api = createGameApi({
        http: givenCapturingHttp(catalog, capture),
        stream: givenStream(),
      });

      await api.listBonds(signal);

      expect(capture.signal).toBe(signal);
    });
  });

  describe('맥락: 체결 영수증이 제출한 국채 주문과 일치하는 경우', () => {
    it('given 매수 주문과 같은 영수증, when 주문하면, then 국채 주문 고정 경로를 사용한다', async () => {
      const request = givenBondOrder();
      const capture = givenHttpCapture();
      const api = createGameApi({
        http: givenCapturingHttp(
          {
            bondOrder: {
              commandId: request.commandId,
              executionId: 'c3a9aa29-fdfa-41b0-a054-332415c1976a',
              accountId: request.accountId,
              seriesId: request.seriesId,
              side: request.side,
              bondUnits: request.bondUnits,
              dirtyPriceKrw: 99_000,
              grossAmountKrw: 990_000,
              feeKrw: 1_000,
              taxKrw: 0,
              removedCostBasisKrw: 0,
              realizedGainLossKrw: 0,
              replayed: false,
            },
            snapshot: givenSnapshot(),
          },
          capture,
        ),
        stream: givenStream(),
      });

      await api.placeBondOrder(request);

      expect(capture).toEqual({
        method: 'POST',
        path: '/api/finance/bonds/orders',
        body: request,
      });
    });
  });

  describe('맥락: 체결 영수증이 제출하지 않은 시리즈를 가리키는 경우', () => {
    it('given 다른 series ID의 정상 모양 영수증, when 응답을 읽으면, then 계약 위반으로 거절한다', async () => {
      const request = givenBondOrder();
      const api = createGameApi({
        http: givenRespondingHttp({
          bondOrder: {
            commandId: request.commandId,
            executionId: 'c3a9aa29-fdfa-41b0-a054-332415c1976a',
            accountId: request.accountId,
            seriesId: '22',
            side: request.side,
            bondUnits: request.bondUnits,
            dirtyPriceKrw: 99_000,
            grossAmountKrw: 990_000,
            feeKrw: 1_000,
            taxKrw: 0,
            removedCostBasisKrw: 0,
            realizedGainLossKrw: 0,
            replayed: false,
          },
          snapshot: givenSnapshot(),
        }),
        stream: givenStream(),
      });

      const result = api.placeBondOrder(request);

      await expect(result).rejects.toBeDefined();
    });
  });
});

describe('금 계좌·주문·실물 인출 API', () => {
  describe('맥락: 과거 월드에 게시된 금 상품이 없는 경우', () => {
    it('given 빈 상품 목록, when 조회하면, then 금 상품 고정 경로를 사용한다', async () => {
      const catalog = { marketVersion: 'm1-2026-v3', products: [] };
      const capture = givenHttpCapture();
      const api = createGameApi({
        http: givenCapturingHttp(catalog, capture),
        stream: givenStream(),
      });

      const result = await api.listGoldProducts();

      expect({ result, capture }).toEqual({
        result: catalog,
        capture: { method: 'GET', path: '/api/finance/gold-products', body: null },
      });
    });
  });

  describe('맥락: 이전 상품 조회를 취소할 수 있어야 하는 경우', () => {
    it('given AbortSignal, when 금 상품을 조회하면, then 같은 신호를 HTTP GET에 전달한다', async () => {
      const catalog = { marketVersion: 'm1-2026-v3', products: [] };
      const capture = givenHttpCapture();
      const signal = new AbortController().signal;
      const api = createGameApi({
        http: givenCapturingHttp(catalog, capture),
        stream: givenStream(),
      });

      await api.listGoldProducts(signal);

      expect(capture.signal).toBe(signal);
    });
  });

  describe('맥락: 금 계좌 개설 결과가 요청 상품과 일치하는 경우', () => {
    it('given krxGold variant, when 개설하면, then 공통 계좌 경로에 strict body를 보낸다', async () => {
      const request = givenGoldAccountOpen();
      const capture = givenHttpCapture();
      const api = createGameApi({
        http: givenCapturingHttp(
          {
            account: {
              commandId: request.commandId,
              accountId: '6',
              type: request.type,
              productVersionId: request.productVersionId,
              replayed: false,
            },
            snapshot: givenSnapshot(),
          },
          capture,
        ),
        stream: givenStream(),
      });

      await api.openGoldAccount(request);

      expect(capture).toEqual({
        method: 'POST',
        path: '/api/finance/accounts',
        body: request,
      });
    });
  });

  describe('맥락: 금 시장이 닫혀 주문이 거절된 경우', () => {
    it('given marketClosed 실패, when 주문하면, then 금융 도메인 오류로 변환한다', async () => {
      const http: HttpClient = {
        get<T>() {
          return Promise.reject(new Error('unexpected GET')) as Promise<T>;
        },
        post<T>() {
          return Promise.reject(
            new HttpError(409, '/api/finance/gold/orders', {
              code: 'marketClosed',
              message: '금 시장이 닫혀 있습니다',
            }),
          ) as Promise<T>;
        },
        put<T>() {
          return Promise.reject(new Error('unexpected PUT')) as Promise<T>;
        },
      };
      const api = createGameApi({ http, stream: givenStream() });

      const result = api.placeGoldOrder(givenGoldOrder());

      await expect(result).rejects.toEqual(
        new FinanceCommandError('marketClosed', '금 시장이 닫혀 있습니다'),
      );
    });
  });

  describe('맥락: 실물 인출 영수증이 제출한 규격과 개수에 일치하는 경우', () => {
    it('given 100g bar 두 개, when 인출하면, then 금 인출 고정 경로를 사용한다', async () => {
      const request = givenGoldWithdrawal();
      const capture = givenHttpCapture();
      const api = createGameApi({
        http: givenCapturingHttp(
          {
            goldWithdrawal: {
              commandId: request.commandId,
              withdrawalId: 'c3a9aa29-fdfa-41b0-a054-332415c1976a',
              accountId: request.accountId,
              barSizeGram: request.barSizeGram,
              barCount: request.barCount,
              quantityGram: 200,
              removedCostBasisKrw: 20_000_000,
              vatKrw: 2_000_000,
              feeKrw: 10_000,
              cashChargedKrw: 2_010_000,
              replayed: false,
            },
            snapshot: givenSnapshot(),
          },
          capture,
        ),
        stream: givenStream(),
      });

      await api.withdrawGold(request);

      expect(capture).toEqual({
        method: 'POST',
        path: '/api/finance/gold/withdrawals',
        body: request,
      });
    });
  });

  describe('맥락: 실물 인출 영수증의 bar 개수가 요청과 다른 경우', () => {
    it('given 한 개 bar 영수증과 두 개 요청, when 응답을 읽으면, then 계약 위반으로 거절한다', async () => {
      const request = givenGoldWithdrawal();
      const api = createGameApi({
        http: givenRespondingHttp({
          goldWithdrawal: {
            commandId: request.commandId,
            withdrawalId: 'c3a9aa29-fdfa-41b0-a054-332415c1976a',
            accountId: request.accountId,
            barSizeGram: request.barSizeGram,
            barCount: 1,
            quantityGram: 100,
            removedCostBasisKrw: 10_000_000,
            vatKrw: 1_000_000,
            feeKrw: 10_000,
            cashChargedKrw: 1_010_000,
            replayed: false,
          },
          snapshot: givenSnapshot(),
        }),
        stream: givenStream(),
      });

      const result = api.withdrawGold(request);

      await expect(result).rejects.toBeDefined();
    });
  });
});

describe('금융소득 연도 조회 상관관계', () => {
  describe('맥락: 아직 지급된 금융소득이 없는 경우', () => {
    it('given 요청 연도와 0 누계, when 조회하면, then zero-filled 결과를 허용한다', async () => {
      const summary = givenLegacyFinancialIncomeYear();
      const capture = givenHttpCapture();
      const api = createGameApi({
        http: givenCapturingHttp(summary, capture),
        stream: givenStream(),
      });

      const result = await api.getFinanceTaxYear(2026);

      expect({ result, capture }).toEqual({
        result: summary,
        capture: { method: 'GET', path: '/api/finance/tax-years/2026', body: null },
      });
    });
  });

  describe('맥락: 응답 연도가 요청 연도와 다른 경우', () => {
    it('given 2025년 요청과 2026년 결과, when 조회하면, then 계약 위반으로 거절한다', async () => {
      const api = createGameApi({
        http: givenRespondingHttp(givenLegacyFinancialIncomeYear()),
        stream: givenStream(),
      });

      const result = api.getFinanceTaxYear(2025);

      await expect(result).rejects.toBeDefined();
    });
  });
});

describe('원장 페이지 응답 검증', () => {
  describe('맥락: 서버가 요청한 limit보다 많은 거래를 반환한 경우', () => {
    it('given limit 1과 두 거래, when 조회하면, then 계약 위반으로 거절한다', async () => {
      const page: LedgerPage = {
        transactions: [givenLedgerTransaction('3'), givenLedgerTransaction('2')],
        nextBefore: '2',
      };
      const api = createGameApi({ http: givenRespondingHttp(page), stream: givenStream() });

      const result = api.getFinanceLedger(undefined, 1);

      await expect(result).rejects.toBeDefined();
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

interface HttpCapture {
  method: 'GET' | 'POST' | 'PUT' | null;
  path: string | null;
  body: unknown;
  signal?: AbortSignal;
}

function givenHttpCapture(): HttpCapture {
  return { method: null, path: null, body: null };
}

function givenCapturingHttp(response: unknown, capture: HttpCapture): HttpClient {
  return {
    async get(path, decoder, options) {
      capture.method = 'GET';
      capture.path = path;
      if (options?.signal !== undefined) capture.signal = options.signal;
      return decoder.parse(response);
    },
    async post(path, body, decoder) {
      capture.method = 'POST';
      capture.path = path;
      capture.body = body;
      return decoder.parse(response);
    },
    async put(path, body, decoder) {
      capture.method = 'PUT';
      capture.path = path;
      capture.body = body;
      return decoder.parse(response);
    },
  };
}

function givenLedgerTransaction(id: string): LedgerPage['transactions'][number] {
  return {
    id,
    gameDay: 1,
    description: '이체',
    sourceKind: 'transfer',
    postings: [
      { accountCode: 'wallet', accountId: null, amountKrw: -1 },
      { accountCode: 'accountCash', accountId: '1', amountKrw: 1 },
    ],
  };
}

function givenSnapshot(): GameSnapshot {
  return {
    runRevision: 3,
    stateRevision: 43,
    gameDay: 17,
    startDate: '2026-01-01',
    cashKrw: 9_900_000,
    debtKrw: 0,
    netWorthKrw: 10_000_000,
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
      accounts: [
        {
          id: '1',
          type: 'taxableBrokerage',
          status: 'open',
          cashKrw: 100_000,
          isDefault: true,
        },
      ],
      cmaAccounts: [],
      cashContracts: [],
      depositProtection: [],
      currentTaxYear: givenLegacyFinancialIncomeYear(),
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
    career: givenEmptyCareerSnapshot(),
    life: {
      rateStatus: 'rateUnavailable',
      household: null,
      residence: null,
      tenantLeaseDepositKrw: 0,
      activeLease: null,
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
      creditBand: null,
      creditReasons: ['modelUnavailable'],
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

function givenEmptyCareerSnapshot(): GameSnapshot['career'] {
  return {
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
  };
}
