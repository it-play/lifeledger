import { describe, expect, it } from '@jest/globals';
import { type HttpClient, HttpError } from '../lib/http/index.js';
import type {
  GameSnapshot,
  InsuranceCancellationRequest,
  InsuranceCancellationResponse,
  InsuranceClaimRequest,
  InsuranceClaimResponse,
  InsuranceContractsQuery,
  InsuranceContractsResponse,
  InsuranceEnrollmentRequest,
  InsuranceEnrollmentResponse,
} from './contracts.js';
import { createInsuranceApi, InsuranceCommandError, InsuranceQueryError } from './insurance-api.js';

const ENROLLMENT_REQUEST: InsuranceEnrollmentRequest = {
  commandId: '00000000-0000-4000-8000-000000000001',
  expectedRunRevision: 3,
  expectedStateRevision: 42,
  expectedGameDay: 17,
  productVersionId: '71',
};

const CANCELLATION_REQUEST: InsuranceCancellationRequest = {
  commandId: '00000000-0000-4000-8000-000000000002',
  expectedRunRevision: 3,
  expectedStateRevision: 42,
  expectedGameDay: 17,
};

const CLAIM_REQUEST: InsuranceClaimRequest = {
  commandId: '00000000-0000-4000-8000-000000000003',
  expectedRunRevision: 3,
  expectedStateRevision: 42,
  expectedGameDay: 17,
  claimId: '101',
};

interface RecordedRequest {
  readonly method: 'GET' | 'POST';
  readonly path: string;
  readonly body?: unknown;
}

describe('보험 계약 조회 protocol', () => {
  describe('맥락: current run의 보험 첫 페이지를 조회하는 경우', () => {
    it('given cursor 없음, when 조회하면, then exact path와 6개 응답 필드를 사용한다', async () => {
      const requests: RecordedRequest[] = [];
      const api = createInsuranceApi({
        http: givenRecordingHttp(givenContractsResponse(), requests),
      });

      const response = await api.list();

      expect(requests).toEqual([{ method: 'GET', path: '/api/insurance/contracts' }]);
      expect(Object.keys(response)).toHaveLength(6);
      expect(response.products[0]?.eligibilityStatus).toBe('eligible');
    });

    it('given opaque cursor, when 조회하면, then 유일 query를 URL encoding한다', async () => {
      const requests: RecordedRequest[] = [];
      const api = createInsuranceApi({
        http: givenRecordingHttp(givenContractsResponse(), requests),
      });

      await api.list({ cursor: 'contract+claim/=' });

      expect(requests).toEqual([
        {
          method: 'GET',
          path: '/api/insurance/contracts?cursor=contract%2Bclaim%2F%3D',
        },
      ]);
    });
  });

  describe('맥락: 공개 query 밖의 필드가 있는 경우', () => {
    it('given unknown limit, when 조회하면, then HTTP 전에 거절한다', () => {
      const requests: RecordedRequest[] = [];
      const api = createInsuranceApi({
        http: givenRecordingHttp(givenContractsResponse(), requests),
      });
      const query = { limit: 20 } as unknown as InsuranceContractsQuery;

      const whenRead = () => api.list(query);

      expect(whenRead).toThrow();
      expect(requests).toEqual([]);
    });
  });
});

describe('보험 가입 protocol', () => {
  describe('맥락: 첫 보험료와 계약이 한 번 커밋된 경우', () => {
    it('given cursor와 product version, when 가입하면, then exact body와 active snapshot을 확인한다', async () => {
      const requests: RecordedRequest[] = [];
      const api = createInsuranceApi({
        http: givenRecordingHttp(givenEnrollmentResponse(), requests),
      });

      const response = await api.enroll(ENROLLMENT_REQUEST);

      expect(requests).toEqual([
        { method: 'POST', path: '/api/insurance/contracts', body: ENROLLMENT_REQUEST },
      ]);
      expect(response.snapshot.life.activeInsuranceContracts[0]?.id).toBe('91');
    });
  });

  describe('맥락: 가입 결과가 다른 상품을 가리키는 경우', () => {
    it('given mismatched productVersionId, when 응답을 읽으면, then 명령 상관관계 위반을 거절한다', async () => {
      const response = givenEnrollmentResponse();
      const api = createInsuranceApi({
        http: givenRespondingHttp({
          ...response,
          result: { ...response.result, productVersionId: '72' },
        }),
      });

      const result = api.enroll(ENROLLMENT_REQUEST);

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 가입 응답을 잃은 뒤 새 run에서 재시도한 경우', () => {
    it('given 저장된 원래 result와 최신 run snapshot, when replay를 읽으면, then revision reset을 허용한다', async () => {
      const response = givenEnrollmentResponse();
      const api = createInsuranceApi({
        http: givenRespondingHttp({
          ...response,
          replayed: true,
          snapshot: givenSnapshot({
            runRevision: 4,
            stateRevision: 0,
            gameDay: 0,
            marketDate: '2026-01-01',
          }),
        }),
      });

      const result = api.enroll(ENROLLMENT_REQUEST);

      await expect(result).resolves.toMatchObject({ replayed: true, snapshot: { runRevision: 4 } });
    });
  });
});

describe('보험 취소 protocol', () => {
  describe('맥락: active 계약을 중도 취소한 경우', () => {
    it('given path contract ID와 cursor, when 취소하면, then 최초 path·body와 active 제거를 확인한다', async () => {
      const requests: RecordedRequest[] = [];
      const api = createInsuranceApi({
        http: givenRecordingHttp(givenCancellationResponse(), requests),
      });

      const response = await api.cancel('91', CANCELLATION_REQUEST);

      expect(requests).toEqual([
        {
          method: 'POST',
          path: '/api/insurance/contracts/91/cancellations',
          body: CANCELLATION_REQUEST,
        },
      ]);
      expect(response.result).toMatchObject({ status: 'cancelled', coverageEndExclusive: 18 });
    });
  });
});

describe('보험금 청구 protocol', () => {
  describe('맥락: ready claim이 지급된 경우', () => {
    it('given cursor와 claim ID, when 청구하면, then exact body와 pending 제거를 확인한다', async () => {
      const requests: RecordedRequest[] = [];
      const api = createInsuranceApi({
        http: givenRecordingHttp(givenClaimResponse(), requests),
      });

      const response = await api.fileClaim(CLAIM_REQUEST);

      expect(requests).toEqual([
        { method: 'POST', path: '/api/insurance/claims', body: CLAIM_REQUEST },
      ]);
      expect(response.result).toMatchObject({ claimId: '101', payoutKrw: 100_000 });
      expect(response.snapshot.life.pendingInsuranceClaims).toEqual([]);
    });
  });

  describe('맥락: 응답 result가 다른 claim을 가리키는 경우', () => {
    it('given mismatched claimId, when 응답을 읽으면, then body와 result 상관관계 위반을 거절한다', async () => {
      const response = givenClaimResponse();
      const api = createInsuranceApi({
        http: givenRespondingHttp({
          ...response,
          result: { ...response.result, claimId: '102' },
        }),
      });

      const result = api.fileClaim(CLAIM_REQUEST);

      await expect(result).rejects.toBeDefined();
    });
  });
});

describe('보험 API 실패 분류', () => {
  describe('맥락: 현재 실행의 계약이 아닌 경우', () => {
    it('given strict insuranceResourceNotFound 404, when 취소하면, then 확정 command 오류로 변환한다', async () => {
      const failure = new HttpError(404, '/api/insurance/contracts/91/cancellations', {
        code: 'insuranceResourceNotFound',
        message: '보험 계약을 찾을 수 없습니다',
      });
      const api = createInsuranceApi({ http: givenRejectingHttp(failure) });

      const result = api.cancel('91', CANCELLATION_REQUEST);

      await expect(result).rejects.toBeInstanceOf(InsuranceCommandError);
      await expect(result).rejects.toMatchObject({ code: 'insuranceResourceNotFound' });
    });
  });

  describe('맥락: cursor가 올바르지 않은 경우', () => {
    it('given invalidCommand 400, when 조회하면, then query 오류로 변환한다', async () => {
      const failure = new HttpError(400, '/api/insurance/contracts?cursor=bad', {
        code: 'invalidCommand',
        message: 'cursor가 올바르지 않습니다',
      });
      const api = createInsuranceApi({ http: givenRejectingHttp(failure) });

      const result = api.list({ cursor: 'bad' });

      await expect(result).rejects.toBeInstanceOf(InsuranceQueryError);
    });
  });

  describe('맥락: 서버 오류로 가입 결과를 알 수 없는 경우', () => {
    it('given 503, when 가입하면, then 재시도 정책이 식별하도록 원래 오류를 보존한다', async () => {
      const failure = new HttpError(503, '/api/insurance/contracts', {
        code: 'busy',
        message: '잠시 후 다시 시도해 주세요',
      });
      const api = createInsuranceApi({ http: givenRejectingHttp(failure) });

      const result = api.enroll(ENROLLMENT_REQUEST);

      await expect(result).rejects.toBe(failure);
    });
  });
});

function givenContractsResponse(): InsuranceContractsResponse {
  return {
    insuranceCapability: 'contractsAndClaims',
    products: [givenProduct()],
    contracts: [givenActiveContract()],
    pendingClaims: [givenReadyClaim()],
    history: [],
    nextCursor: null,
  };
}

function givenProduct(): InsuranceContractsResponse['products'][number] {
  return {
    id: '71',
    productKey: 'fictionalFamilyCareCover',
    displayName: '가족 돌봄 비용 보장',
    eligibilityStatus: 'eligible',
    reasons: [],
    coveredEventKey: 'fictionalDependentCareRequest',
    coveredEventDisplayName: '가족 돌봄 요청의 즉시 지갑 지출',
    premiumKrw: 10_000,
    premiumIntervalGameDays: 30,
    termGameDays: 360,
    waitingPeriodGameDays: 7,
    deductibleKrw: 20_000,
    occurrenceLimitKrw: 100_000,
    termLimitKrw: 200_000,
    claimWindowGameDays: 7,
  };
}

function givenActiveContract(): GameSnapshot['life']['activeInsuranceContracts'][number] {
  return {
    id: '91',
    productVersionId: '71',
    productKey: 'fictionalFamilyCareCover',
    displayName: '가족 돌봄 비용 보장',
    status: 'active',
    coverageStartGameDay: 17,
    waitingEndsGameDay: 24,
    coverageEndExclusive: 377,
    nextPremiumDueGameDay: 47,
    premiumKrw: 10_000,
    paidBenefitKrw: 0,
    reservedBenefitKrw: 0,
    remainingBenefitKrw: 200_000,
  };
}

function givenReadyClaim(): GameSnapshot['life']['pendingInsuranceClaims'][number] {
  return {
    id: '101',
    eventId: '81',
    eventKey: 'fictionalDependentCareRequest',
    eventDisplayName: '가족 돌봄 요청',
    offeredGameDay: 10,
    status: 'ready',
    grossCostKrw: 120_000,
    payoutKrw: 100_000,
    filingDeadlineGameDay: 24,
    contractAllocations: [{ contractId: '91', deductibleKrw: 20_000, payoutKrw: 100_000 }],
  };
}

function givenEnrollmentResponse(): InsuranceEnrollmentResponse {
  const contract = givenActiveContract();
  return {
    result: {
      contractId: contract.id,
      productVersionId: contract.productVersionId,
      status: 'active',
      coverageStartGameDay: contract.coverageStartGameDay,
      waitingEndsGameDay: contract.waitingEndsGameDay,
      coverageEndExclusive: contract.coverageEndExclusive,
      nextPremiumDueGameDay: contract.nextPremiumDueGameDay ?? 47,
      premiumKrw: contract.premiumKrw,
    },
    replayed: false,
    snapshot: givenSnapshot({ activeInsuranceContracts: [contract], cashKrw: 9_990_000 }),
  };
}

function givenCancellationResponse(): InsuranceCancellationResponse {
  return {
    result: { contractId: '91', status: 'cancelled', coverageEndExclusive: 18 },
    replayed: false,
    snapshot: givenSnapshot(),
  };
}

function givenClaimResponse(): InsuranceClaimResponse {
  return {
    result: { claimId: '101', eventId: '81', payoutKrw: 100_000, paidGameDay: 17 },
    replayed: false,
    snapshot: givenSnapshot({
      activeInsuranceContracts: [givenActiveContract()],
      cashKrw: 10_100_000,
    }),
  };
}

interface SnapshotOptions {
  readonly activeInsuranceContracts?: GameSnapshot['life']['activeInsuranceContracts'];
  readonly pendingInsuranceClaims?: GameSnapshot['life']['pendingInsuranceClaims'];
  readonly cashKrw?: number;
  readonly runRevision?: number;
  readonly stateRevision?: number;
  readonly gameDay?: number;
  readonly marketDate?: string;
}

function givenSnapshot(options: SnapshotOptions = {}): GameSnapshot {
  const cashKrw = options.cashKrw ?? 10_000_000;
  return {
    runRevision: options.runRevision ?? 3,
    stateRevision: options.stateRevision ?? 43,
    gameDay: options.gameDay ?? 17,
    startDate: '2026-01-01',
    cashKrw,
    debtKrw: 0,
    netWorthKrw: cashKrw,
    characterName: '테스터',
    autoSpeed: null,
    market: {
      world: 'm1-2026-v3',
      date: options.marketDate ?? '2026-01-18',
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
      insuranceCapability: 'contractsAndClaims',
      activeInsuranceContracts: options.activeInsuranceContracts ?? [],
      pendingInsuranceClaims: options.pendingInsuranceClaims ?? [],
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

function givenRespondingHttp(response: unknown): HttpClient {
  return givenRecordingHttp(response, []);
}

function givenRecordingHttp(response: unknown, requests: RecordedRequest[]): HttpClient {
  return {
    async get(path, decoder) {
      requests.push({ method: 'GET', path });
      return decoder.parse(response);
    },
    async post(path, body, decoder) {
      requests.push({ method: 'POST', path, body });
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
