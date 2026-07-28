import { describe, expect, it } from '@jest/globals';
import { type HttpClient, HttpError } from '../lib/http/index.js';
import type {
  GameSnapshot,
  WelfareApplicationRequest,
  WelfareApplicationResponse,
  WelfareProgramsResponse,
} from './contracts.js';
import { createWelfareApi, WelfareCommandError, WelfareQueryError } from './welfare-api.js';

const APPLICATION_REQUEST: WelfareApplicationRequest = {
  commandId: '4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2',
  expectedRunRevision: 3,
  expectedStateRevision: 42,
  expectedGameDay: 17,
  programVersionId: '71',
};

interface RecordedRequest {
  readonly method: 'GET' | 'POST';
  readonly path: string;
  readonly body?: unknown;
}

describe('복지 프로그램 조회 protocol', () => {
  describe('맥락: 현재 실행에 고정된 프로그램을 조회하는 경우', () => {
    it('given strict 프로그램 projection, when 조회하면, then exact path와 서버 판정을 사용한다', async () => {
      const requests: RecordedRequest[] = [];
      const api = createWelfareApi({
        http: givenRecordingHttp(givenProgramsResponse(), requests),
      });

      const response = await api.listPrograms();

      expect(requests).toEqual([{ method: 'GET', path: '/api/welfare/programs' }]);
      expect(response.programs[0]?.evaluationStatus).toBe('eligible');
      expect(response.programs[0]?.benefitKrw).toBe(333_000);
    });
  });

  describe('맥락: 서버가 내부 판정 자료를 응답에 섞은 경우', () => {
    it('given 허용되지 않은 rawFacts 필드, when 조회하면, then strict 계약 위반으로 거절한다', async () => {
      const response = givenProgramsResponse();
      const api = createWelfareApi({
        http: givenRespondingHttp({
          ...response,
          programs: [{ ...response.programs[0], rawFacts: { cashKrw: 0 } }],
        }),
      });

      const result = api.listPrograms();

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: catalog 상한을 넘긴 projection이 온 경우', () => {
    it('given 17개 프로그램, when 조회하면, then silent truncation 없이 계약 위반으로 거절한다', async () => {
      const response = givenProgramsResponse();
      const program = response.programs[0];
      if (program === undefined) throw new Error('프로그램 fixture가 없습니다.');
      const api = createWelfareApi({
        http: givenRespondingHttp({
          ...response,
          programs: Array.from({ length: 17 }, (_, index) => ({
            ...program,
            id: String(71 + index),
            programKey: `fixtureGrant${String(index + 1).padStart(2, '0')}`,
          })),
        }),
      });

      const result = api.listPrograms();

      await expect(result).rejects.toBeDefined();
    });

    it('given 33개 public condition, when 조회하면, then 일부 조건만 표시하지 않고 거절한다', async () => {
      const response = givenProgramsResponse();
      const program = response.programs[0];
      if (program === undefined) throw new Error('프로그램 fixture가 없습니다.');
      const api = createWelfareApi({
        http: givenRespondingHttp({
          ...response,
          programs: [
            {
              ...program,
              conditions: Array.from({ length: 33 }, (_, index) => ({
                code: `condition${index}`,
                label: `${index + 1}번 조건`,
                outcome: 'passed',
              })),
            },
          ],
        }),
      });

      const result = api.listPrograms();

      await expect(result).rejects.toBeDefined();
    });
  });
});

describe('복지 신청 protocol', () => {
  describe('맥락: 신청과 즉시 승인이 한 번 커밋된 경우', () => {
    it('given 공통 cursor와 program version, when 신청하면, then exact body와 active projection을 허용한다', async () => {
      const requests: RecordedRequest[] = [];
      const api = createWelfareApi({
        http: givenRecordingHttp(givenApplicationResponse(), requests),
      });

      const response = await api.apply(APPLICATION_REQUEST);

      expect(requests).toEqual([
        {
          method: 'POST',
          path: '/api/welfare/applications',
          body: APPLICATION_REQUEST,
        },
      ]);
      expect(response.result.payment).toMatchObject({ amountKrw: 333_000, dueGameDay: 18 });
      expect(response.snapshot.life.activeWelfareApplications[0]?.applicationId).toBe('81');
    });
  });

  describe('맥락: 결과가 요청과 다른 프로그램을 가리키는 경우', () => {
    it('given 다른 program version result, when 응답을 읽으면, then 상관관계 위반으로 거절한다', async () => {
      const response = givenApplicationResponse();
      const api = createWelfareApi({
        http: givenRespondingHttp({
          ...response,
          result: { ...response.result, programVersionId: '72' },
        }),
      });

      const result = api.apply(APPLICATION_REQUEST);

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 새 신청이 snapshot에 투영되지 않은 경우', () => {
    it('given receipt만 있고 active summary가 없음, when 응답을 읽으면, then 커밋 모순을 거절한다', async () => {
      const response = givenApplicationResponse();
      const api = createWelfareApi({
        http: givenRespondingHttp({
          ...response,
          snapshot: {
            ...response.snapshot,
            life: { ...response.snapshot.life, activeWelfareApplications: [] },
          },
        }),
      });

      const result = api.apply(APPLICATION_REQUEST);

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: active application projection 상한을 넘긴 경우', () => {
    it('given 9개 active application, when 응답을 읽으면, then snapshot 계약 위반으로 거절한다', async () => {
      const response = givenApplicationResponse();
      const api = createWelfareApi({
        http: givenRespondingHttp({
          ...response,
          snapshot: {
            ...response.snapshot,
            life: {
              ...response.snapshot.life,
              activeWelfareApplications: Array.from({ length: 9 }, (_, index) =>
                givenActiveWelfareApplication(index),
              ),
            },
          },
        }),
      });

      const result = api.apply(APPLICATION_REQUEST);

      await expect(result).rejects.toBeDefined();
    });
  });

  describe('맥락: 기존 신청 결과를 더 최신 snapshot과 replay한 경우', () => {
    it('given 지급 완료 뒤 저장된 result, when 재시도하면, then active summary 없이 replay를 허용한다', async () => {
      const response = givenApplicationResponse();
      const api = createWelfareApi({
        http: givenRespondingHttp({
          ...response,
          replayed: true,
          snapshot: {
            ...response.snapshot,
            stateRevision: 50,
            gameDay: 19,
            life: { ...response.snapshot.life, activeWelfareApplications: [] },
          },
        }),
      });

      const result = await api.apply(APPLICATION_REQUEST);

      expect(result.replayed).toBe(true);
      expect(result.snapshot.stateRevision).toBe(50);
    });
  });
});

describe('복지 API 실패 분류', () => {
  describe('맥락: 서버가 확정적인 자격 거절을 반환한 경우', () => {
    it('given strict ineligible 409, when 신청하면, then WelfareCommandError로 변환한다', async () => {
      const failure = new HttpError(409, '/api/welfare/applications', {
        code: 'ineligible',
        message: '현재 조건으로 신청할 수 없습니다',
      });
      const api = createWelfareApi({ http: givenRejectingHttp(failure) });

      const result = api.apply(APPLICATION_REQUEST);

      await expect(result).rejects.toEqual(
        expect.objectContaining({ code: 'ineligible', name: 'WelfareCommandError' }),
      );
      await expect(result).rejects.toBeInstanceOf(WelfareCommandError);
    });
  });

  describe('맥락: 프로그램 조회 자원이 현재 실행에 없는 경우', () => {
    it('given strict welfareResourceNotFound 404, when 조회하면, then WelfareQueryError로 변환한다', async () => {
      const failure = new HttpError(404, '/api/welfare/programs', {
        code: 'welfareResourceNotFound',
        message: '복지 프로그램을 찾을 수 없습니다',
      });
      const api = createWelfareApi({ http: givenRejectingHttp(failure) });

      const result = api.listPrograms();

      await expect(result).rejects.toBeInstanceOf(WelfareQueryError);
    });
  });

  describe('맥락: 서버 오류로 신청 결과를 알 수 없는 경우', () => {
    it('given 503 failure, when 신청하면, then 원래 오류를 보존한다', async () => {
      const failure = new HttpError(503, '/api/welfare/applications', {
        code: 'busy',
        message: '잠시 후 다시 시도해 주세요',
      });
      const api = createWelfareApi({ http: givenRejectingHttp(failure) });

      const result = api.apply(APPLICATION_REQUEST);

      await expect(result).rejects.toBe(failure);
    });
  });
});

function givenProgramsResponse(): WelfareProgramsResponse {
  return {
    componentVersionId: '61',
    gameDay: 17,
    programs: [
      {
        id: '71',
        programKey: 'fictionalRestartGrant',
        displayName: '라이프 새출발 지원금',
        benefitKrw: 333_000,
        paymentDelayGameDays: 1,
        evaluationStatus: 'eligible',
        factFingerprint: '0'.repeat(64),
        conditions: [
          { code: 'cashThreshold', label: '지갑 현금 기준', outcome: 'passed' },
          { code: 'noDuplicateClaim', label: '중복 신청 없음', outcome: 'passed' },
        ],
        applicationAvailable: true,
        latestApplication: null,
        nextPayment: null,
      },
    ],
  };
}

function givenApplicationResponse(): WelfareApplicationResponse {
  const payment = {
    id: '91',
    paymentNo: 1,
    amountKrw: 333_000,
    dueGameDay: 18,
    status: 'pending' as const,
  };
  const snapshot = givenSnapshot([givenActiveWelfareApplication(0)]);
  return {
    result: {
      applicationId: '81',
      programVersionId: '71',
      status: 'active',
      applicationGameDay: 17,
      approvalGameDay: 17,
      eligibilityAtApplication: [
        { code: 'cashThreshold', label: '지갑 현금 기준', outcome: 'passed' },
        { code: 'noDuplicateClaim', label: '중복 신청 없음', outcome: 'passed' },
      ],
      payment,
    },
    replayed: false,
    snapshot: { ...snapshot, stateRevision: 43 },
  };
}

function givenActiveWelfareApplication(
  index: number,
): GameSnapshot['life']['activeWelfareApplications'][number] {
  return {
    applicationId: String(81 + index),
    programVersionId: String(71 + index),
    programKey: index === 0 ? 'fictionalRestartGrant' : `fixtureGrant${index}`,
    displayName: `${index + 1}번 지원금`,
    status: 'active',
    applicationGameDay: 17,
    approvalGameDay: 17,
    benefitKrw: 333_000,
    paidKrw: 0,
    nextPayment: {
      id: String(91 + index),
      paymentNo: 1,
      amountKrw: 333_000,
      dueGameDay: 18,
      status: 'pending',
    },
  };
}

function givenSnapshot(
  activeWelfareApplications: GameSnapshot['life']['activeWelfareApplications'] = [],
): GameSnapshot {
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
      activeWelfareApplications,
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
