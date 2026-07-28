import { describe, expect, it } from '@jest/globals';
import { type HttpClient, HttpError } from '../lib/http/index.js';
import type {
  GameSnapshot,
  LoanExecutionRequest,
  LoanPrepaymentRequest,
  LoanQuoteRequest,
} from './contracts.js';
import { createLoanApi, LoanCommandError } from './loan-api.js';

const QUOTE_REQUEST: LoanQuoteRequest = {
  commandId: '4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2',
  expectedRunRevision: 3,
  expectedStateRevision: 42,
  expectedGameDay: 17,
  productVersionId: '21',
  principalKrw: 10_000_000,
};

const EXECUTION_REQUEST: LoanExecutionRequest = {
  commandId: '4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f3',
  expectedRunRevision: 3,
  expectedStateRevision: 42,
  expectedGameDay: 17,
  quoteId: '30',
};

const PREPAYMENT_REQUEST: LoanPrepaymentRequest = {
  commandId: '4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f4',
  expectedRunRevision: 3,
  expectedStateRevision: 42,
  expectedGameDay: 17,
  principalKrw: 1_000_000,
};

describe('대출 견적 응답 상관관계', () => {
  describe('맥락: 요청한 게임일과 결과 생성일이 같은 경우', () => {
    it('given 같은 상품·원금·게임일, when 응답을 읽으면, then 견적을 허용한다', async () => {
      const api = createLoanApi({ http: givenRespondingHttp(givenQuoteResponse(17)) });

      const result = await api.quote(QUOTE_REQUEST);

      expect(result.result.createdGameDay).toBe(17);
    });
  });

  describe('맥락: 요청한 게임일과 결과 생성일이 다른 경우', () => {
    it('given 같은 상품·원금과 다른 게임일, when 응답을 읽으면, then 계약 위반으로 거절한다', async () => {
      const api = createLoanApi({ http: givenRespondingHttp(givenQuoteResponse(18)) });

      const result = api.quote(QUOTE_REQUEST);

      await expect(result).rejects.toBeDefined();
    });
  });
});

describe('대출 계약 상세 조회 protocol', () => {
  describe('맥락: 현재 run 소유 계약을 조회하는 경우', () => {
    it('given exact 계약 ID, when 상세를 읽으면, then strict 계약을 허용한다', async () => {
      const gets: string[] = [];
      const api = createLoanApi({
        http: givenRecordingGetHttp(givenLoanDetailResponse(), gets),
      });

      const result = await api.getDetail('40');

      expect(result.id).toBe('40');
      expect(gets).toEqual(['/api/loans/40']);
    });
  });

  describe('맥락: 다른 계약의 상세가 온 경우', () => {
    it('given path와 다른 ID, when 응답을 읽으면, then 상관관계 위반으로 거절한다', async () => {
      const api = createLoanApi({
        http: givenRespondingHttp(givenLoanDetailResponse({ id: '41' })),
      });

      const result = api.getDetail('40');

      await expect(result).rejects.toBeDefined();
    });
  });
});

describe('대출 상환표와 납부 이력 protocol', () => {
  describe('맥락: dual-window cursor로 이전 기록을 조회하는 경우', () => {
    it('given opaque before와 limit, when 조회하면, then exact query와 strict page를 사용한다', async () => {
      const gets: string[] = [];
      const api = createLoanApi({
        http: givenRecordingGetHttp(givenLoanHistoryResponse(), gets),
      });

      const result = await api.getInstallmentHistory('40', {
        before: 'v1.l40.i2.p1',
        limit: 2,
      });

      expect(result.loanId).toBe('40');
      expect(gets).toEqual(['/api/loans/40/installments?before=v1.l40.i2.p1&limit=2']);
    });

    it('given 다른 계약 ID를 담은 cursor, when 조회하면, then HTTP 전에 query를 거절한다', () => {
      const gets: string[] = [];
      const api = createLoanApi({
        http: givenRecordingGetHttp(givenLoanHistoryResponse(), gets),
      });

      const whenRead = () =>
        api.getInstallmentHistory('40', {
          before: 'v1.l41.i2.p1',
          limit: 2,
        });

      expect(whenRead).toThrow();
      expect(gets).toEqual([]);
    });
  });

  describe('맥락: path와 다른 계약 window가 온 경우', () => {
    it('given 다른 loan ID, when 응답을 읽으면, then 상관관계 위반으로 거절한다', async () => {
      const api = createLoanApi({
        http: givenRespondingHttp(givenLoanHistoryResponse({ loanId: '41' })),
      });

      const result = api.getInstallmentHistory('40');

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 요청 limit보다 큰 window가 온 경우', () => {
    it('given 두 installment와 limit 1, when 응답을 읽으면, then bounded 계약 위반으로 거절한다', async () => {
      const page = givenLoanHistoryResponse();
      const api = createLoanApi({
        http: givenRespondingHttp({
          ...page,
          installments: [
            givenLoanHistoryInstallment(2, '161'),
            givenLoanHistoryInstallment(1, '160'),
          ],
        }),
      });

      const result = api.getInstallmentHistory('40', { limit: 1 });

      await expect(result).rejects.toBeDefined();
    });
  });
});

describe('대출 실행 응답 상관관계', () => {
  describe('맥락: 요청한 견적의 계약이 실행된 경우', () => {
    it('given 같은 quote ID의 strict 결과, when 응답을 읽으면, then 실행을 허용한다', async () => {
      const api = createLoanApi({ http: givenRespondingHttp(givenExecutionResponse('30')) });

      const result = await api.execute(EXECUTION_REQUEST);

      expect(result.result.loanId).toBe('40');
    });
  });

  describe('맥락: 다른 견적의 실행 결과가 온 경우', () => {
    it('given 다른 quote ID, when 응답을 읽으면, then 계약 위반으로 거절한다', async () => {
      const api = createLoanApi({ http: givenRespondingHttp(givenExecutionResponse('31')) });

      const result = api.execute(EXECUTION_REQUEST);

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 요청한 게임일과 계약 실행일이 다른 경우', () => {
    it('given 다른 activation game day, when 응답을 읽으면, then 계약 위반으로 거절한다', async () => {
      const api = createLoanApi({ http: givenRespondingHttp(givenExecutionResponse('30', 18)) });

      const result = api.execute(EXECUTION_REQUEST);

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 실행 응답에 계약 밖 필드가 있는 경우', () => {
    it('given strict envelope 밖의 값, when 응답을 읽으면, then 계약 위반으로 거절한다', async () => {
      const api = createLoanApi({
        http: givenRespondingHttp({ ...givenExecutionResponse('30'), annualRateBp: 655 }),
      });

      const result = api.execute(EXECUTION_REQUEST);

      await expect(result).rejects.toBeDefined();
    });
  });
});

describe('대출 조기상환 protocol', () => {
  describe('맥락: 선택한 계약의 원금을 조기상환한 경우', () => {
    it('given path 계약과 strict body, when 요청하면, then 대응하는 지급 결과를 허용한다', async () => {
      const posts: { readonly path: string; readonly body: unknown }[] = [];
      const api = createLoanApi({
        http: givenRecordingHttp(givenPrepaymentResponse(), posts),
      });

      const result = await api.prepay('40', PREPAYMENT_REQUEST);

      expect(result.result.paymentId).toBe('50');
      expect(posts).toEqual([
        {
          path: '/api/loans/40/prepayments',
          body: PREPAYMENT_REQUEST,
        },
      ]);
    });
  });

  describe('맥락: 다른 계약의 지급 결과가 온 경우', () => {
    it('given path와 다른 loan ID, when 응답을 읽으면, then 계약 위반으로 거절한다', async () => {
      const api = createLoanApi({
        http: givenRespondingHttp(givenPrepaymentResponse({ loanId: '41' })),
      });

      const result = api.prepay('40', PREPAYMENT_REQUEST);

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 요청과 다른 원금이나 게임일의 결과가 온 경우', () => {
    it('given 다른 principal, when 응답을 읽으면, then 상관관계 위반으로 거절한다', async () => {
      const api = createLoanApi({
        http: givenRespondingHttp(
          givenPrepaymentResponse({ principalKrw: 999_999, totalDebitedKrw: 1_009_999 }),
        ),
      });

      const result = api.prepay('40', PREPAYMENT_REQUEST);

      await expect(result).rejects.toBeDefined();
    });

    it('given 다른 applied game day, when 응답을 읽으면, then 상관관계 위반으로 거절한다', async () => {
      const api = createLoanApi({
        http: givenRespondingHttp(givenPrepaymentResponse({ appliedGameDay: 18 })),
      });

      const result = api.prepay('40', PREPAYMENT_REQUEST);

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 총 지갑 차감액이 원금과 수수료 합과 다른 경우', () => {
    it('given 불일치 total, when 응답을 읽으면, then BigInt 합계 검증으로 거절한다', async () => {
      const api = createLoanApi({
        http: givenRespondingHttp(givenPrepaymentResponse({ totalDebitedKrw: 1_010_001 })),
      });

      const result = api.prepay('40', PREPAYMENT_REQUEST);

      await expect(result).rejects.toBeDefined();
    });
  });
});

describe('대출 명령 오류 분류', () => {
  describe('맥락: 서버 내부 오류로 실행 결과를 확인하지 못한 경우', () => {
    it('given LifeFailure 모양의 5xx, when 실행하면, then 결과 불명 HttpError를 보존한다', async () => {
      const failure = new HttpError(500, '/api/loans', {
        code: 'contractConflict',
        message: '서버 내부 오류',
      });
      const api = createLoanApi({ http: givenRejectingHttp(failure) });

      const result = api.execute(EXECUTION_REQUEST);

      await expect(result).rejects.toBe(failure);
    });
  });

  describe('맥락: 서버가 실행을 확정적으로 거절한 경우', () => {
    it('given stable 409 failure, when 실행하면, then LoanCommandError로 변환한다', async () => {
      const failure = new HttpError(409, '/api/loans', {
        code: 'contractConflict',
        message: '견적을 실행할 수 없습니다',
      });
      const api = createLoanApi({ http: givenRejectingHttp(failure) });

      const result = api.execute(EXECUTION_REQUEST);

      await expect(result).rejects.toBeInstanceOf(LoanCommandError);
    });
  });

  describe('맥락: 현재 run에서 계약을 찾지 못한 경우', () => {
    it('given stable loanNotFound 404, when 상세를 조회하면, then 존재를 구분하지 않는 오류로 변환한다', async () => {
      const failure = new HttpError(404, '/api/loans/40', {
        code: 'loanNotFound',
        message: '대출 계약을 찾을 수 없습니다',
      });
      const api = createLoanApi({ http: givenRejectingHttp(failure) });

      const result = api.getDetail('40');

      await expect(result).rejects.toMatchObject({ code: 'loanNotFound' });
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

function givenRecordingHttp(
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

function givenQuoteResponse(createdGameDay: number): unknown {
  return {
    result: {
      quoteId: '30',
      productVersionId: '21',
      requestedPrincipalKrw: 10_000_000,
      createdGameDay,
      expiresGameDay: createdGameDay,
      decisionCode: 'eligible',
      decisionReasons: ['eligible'],
      verifiedAnnualIncomeKrw: null,
      verifiedIncomeSource: null,
      existingLoanBalanceKrw: 1_000_000,
      postExecutionBalanceKrw: 11_000_000,
      dsrApplied: false,
      dsr: null,
      stressRateBp: 0,
      quotedTerms: {
        annualRateBp: 655,
        repaymentMethod: 'levelPayment',
        termMonths: 60,
        firstInstallment: {
          dueGameDay: 30,
          feeKrw: 0,
          principalKrw: 143_941,
          interestKrw: 55_630,
          totalKrw: 199_571,
        },
      },
    },
    replayed: false,
    snapshot: givenSnapshot(),
  };
}

function givenExecutionResponse(quoteId: string, activatedGameDay = 17) {
  return {
    result: {
      loanId: '40',
      quoteId,
      productVersionId: '21',
      principalKrw: 10_000_000,
      activatedGameDay,
      maturityGameDay: 1_843,
      annualRateBp: 655,
      repaymentMethod: 'levelPayment',
      termMonths: 60,
      firstInstallment: {
        dueGameDay: 30,
        feeKrw: 0,
        principalKrw: 143_941,
        interestKrw: 55_630,
        totalKrw: 199_571,
      },
    },
    replayed: false,
    snapshot: givenSnapshot(),
  };
}

function givenPrepaymentResponse(resultOverrides: Readonly<Record<string, unknown>> = {}): unknown {
  return {
    result: {
      loanId: '40',
      paymentId: '50',
      principalKrw: 1_000_000,
      feeKrw: 10_000,
      totalDebitedKrw: 1_010_000,
      appliedGameDay: 17,
      remainingPrincipalKrw: 9_000_000,
      status: 'active',
      prepaymentEffect: 'recalculatePayment',
      remainingInstallments: 60,
      nextInstallment: {
        installmentNo: 1,
        dueGameDay: 30,
        feeKrw: 0,
        principalKrw: 129_546,
        interestKrw: 50_067,
        totalKrw: 179_613,
      },
      finalInstallmentDueGameDay: 1_843,
      ...resultOverrides,
    },
    replayed: false,
    snapshot: givenSnapshot(),
  };
}

function givenLoanDetailResponse(resultOverrides: Readonly<Record<string, unknown>> = {}): unknown {
  return {
    id: '40',
    leaseContractId: null,
    propertyHoldingId: null,
    productVersionId: '21',
    productKind: 'unsecuredLoan',
    displayName: '개발 변동금리 신용대출',
    rateStatus: 'available',
    currentAnnualRateBp: 655,
    status: 'active',
    readOnly: false,
    originalPrincipalKrw: 10_000_000,
    remainingPrincipalKrw: 9_000_000,
    accruedInterestKrw: 0,
    accruedFeeKrw: 0,
    overdueKrw: 0,
    repaymentMethod: 'levelPayment',
    termMonths: 60,
    totalInstallments: 60,
    activatedGameDay: 17,
    maturityGameDay: 1_843,
    finalInstallmentDueGameDay: 1_843,
    nextInstallmentNo: 1,
    oldestUnpaidDueGameDay: null,
    prepaymentAllowed: true,
    prepaymentFeePpm: 10_000,
    prepaymentEffect: 'recalculatePayment',
    dsrIncluded: true,
    ...resultOverrides,
  };
}

function givenLoanHistoryResponse(resultOverrides: Readonly<Record<string, unknown>> = {}) {
  return {
    loanId: '40',
    installments: [givenLoanHistoryInstallment(2, '161')],
    payments: [givenLoanHistoryPayment()],
    hasMoreInstallments: false,
    hasMorePayments: false,
    nextBefore: null,
    ...resultOverrides,
  };
}

function givenLoanHistoryInstallment(installmentNo: number, id: string) {
  return {
    id,
    installmentNo,
    dueGameDay: 30,
    interestPeriodStartGameDay: 18,
    elapsedDays: 13,
    annualRateBp: 655,
    openingPrincipalKrw: 9_000_000,
    scheduledFeeKrw: 0,
    scheduledInterestKrw: 50_067,
    scheduledPrincipalKrw: 129_546,
    paidFeeKrw: 0,
    paidInterestKrw: 0,
    paidPrincipalKrw: 0,
    remainingDueKrw: 179_613,
    status: 'pending',
    scheduleRevision: 2,
  };
}

function givenLoanHistoryPayment() {
  return {
    id: '50',
    paymentNo: 1,
    kind: 'manualPrepayment',
    gameDay: 17,
    amountKrw: 1_010_000,
    allocations: [
      { kind: 'prepaymentFee', amountKrw: 10_000 },
      { kind: 'prepaymentPrincipal', amountKrw: 1_000_000 },
    ],
  };
}

function givenSnapshot(): GameSnapshot {
  return {
    runRevision: 3,
    stateRevision: 42,
    gameDay: 17,
    startDate: '2026-01-01',
    cashKrw: 10_000_000,
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
      corporation: { availability: 'unavailable', current: null },
    },
  };
}
