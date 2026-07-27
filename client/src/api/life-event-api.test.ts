import { describe, expect, it } from '@jest/globals';
import { type HttpClient, HttpError } from '../lib/http/index.js';
import type {
  GameSnapshot,
  LifeEventChoiceRequest,
  LifeEventChoiceResponse,
  LifeEventsQuery,
  LifeEventsResponse,
} from './contracts.js';
import {
  createLifeEventApi,
  LifeEventCommandError,
  LifeEventQueryError,
} from './life-event-api.js';

const CHOICE_REQUEST: LifeEventChoiceRequest = {
  commandId: '00000000-0000-4000-8000-000000000001',
  expectedRunRevision: 3,
  expectedStateRevision: 42,
  expectedGameDay: 17,
  choiceId: '81',
};

interface RecordedRequest {
  readonly method: 'GET' | 'POST';
  readonly path: string;
  readonly body?: unknown;
}

describe('생애 사건 조회 protocol', () => {
  describe('맥락: 현재 실행의 사건 첫 페이지를 조회하는 경우', () => {
    it('given cursor 없음, when 조회하면, then exact path와 strict projection을 사용한다', async () => {
      const requests: RecordedRequest[] = [];
      const api = createLifeEventApi({
        http: givenRecordingHttp(givenEventsResponse(), requests),
      });

      const response = await api.list();

      expect(requests).toEqual([{ method: 'GET', path: '/api/life/events' }]);
      expect(response.pendingEvents[0]?.eventKey).toBe('fictionalDependentCareRequest');
    });

    it('given opaque cursor, when 조회하면, then URL encoding한 유일 query를 보낸다', async () => {
      const requests: RecordedRequest[] = [];
      const api = createLifeEventApi({
        http: givenRecordingHttp(givenEventsResponse(), requests),
      });

      await api.list({ cursor: 'v1.day+id/=' });

      expect(requests).toEqual([
        { method: 'GET', path: '/api/life/events?cursor=v1.day%2Bid%2F%3D' },
      ]);
    });
  });

  describe('맥락: 공개 query 밖의 필드가 있는 경우', () => {
    it('given unknown limit, when 조회하면, then HTTP 전에 거절한다', () => {
      const requests: RecordedRequest[] = [];
      const api = createLifeEventApi({
        http: givenRecordingHttp(givenEventsResponse(), requests),
      });
      const query = { limit: 20 } as unknown as LifeEventsQuery;

      const whenRead = () => api.list(query);

      expect(whenRead).toThrow();
      expect(requests).toEqual([]);
    });
  });

  describe('맥락: 조회 cursor가 서버에서 거절된 경우', () => {
    it('given invalidCommand 400, when 조회하면, then query domain 오류로 분류한다', async () => {
      const api = createLifeEventApi({
        http: givenRejectingHttp(
          new HttpError(400, '/api/life/events', {
            code: 'invalidCommand',
            message: 'cursor가 올바르지 않습니다',
          }),
        ),
      });

      const result = api.list({ cursor: 'invalid' });

      await expect(result).rejects.toBeInstanceOf(LifeEventQueryError);
      await expect(result).rejects.toMatchObject({ code: 'invalidCommand' });
    });
  });
});

describe('생애 사건 선택 protocol', () => {
  describe('맥락: 선택 응답을 잃은 뒤 새 run에서 재시도한 경우', () => {
    it('given 저장된 원래 result와 최신 run snapshot, when replay를 읽으면, then revision reset을 허용한다', async () => {
      const response = givenChoiceResponse();
      const api = createLifeEventApi({
        http: givenRecordingHttp(
          {
            ...response,
            replayed: true,
            snapshot: givenSnapshot({
              runRevision: 4,
              stateRevision: 0,
              gameDay: 0,
              marketDate: '2026-01-01',
            }),
          },
          [],
        ),
      });

      const result = api.choose('71', CHOICE_REQUEST);

      await expect(result).resolves.toMatchObject({ replayed: true, snapshot: { runRevision: 4 } });
    });
  });

  describe('맥락: 현재 사건에 속하지 않는 선택지를 보낸 경우', () => {
    it('given contractConflict 409, when 선택하면, then exact path와 body를 보존해 확정 오류로 분류한다', async () => {
      const requests: RecordedRequest[] = [];
      const api = createLifeEventApi({
        http: givenRecordingRejectingHttp(
          new HttpError(409, '/api/life/events/71/choices', {
            code: 'contractConflict',
            message: '선택지가 사건에 속하지 않습니다',
          }),
          requests,
        ),
      });

      const result = api.choose('71', CHOICE_REQUEST);

      await expect(result).rejects.toBeInstanceOf(LifeEventCommandError);
      await expect(result).rejects.toMatchObject({ code: 'contractConflict' });
      expect(requests).toEqual([
        {
          method: 'POST',
          path: '/api/life/events/71/choices',
          body: CHOICE_REQUEST,
        },
      ]);
    });
  });

  describe('맥락: 서버 오류로 선택 결과를 알 수 없는 경우', () => {
    it('given 503, when 선택하면, then 재시도 정책이 식별하도록 원래 오류를 보존한다', async () => {
      const failure = new HttpError(503, '/api/life/events/71/choices', {
        code: 'busy',
        message: '잠시 후 다시 시도해 주세요',
      });
      const api = createLifeEventApi({ http: givenRejectingHttp(failure) });

      const result = api.choose('71', CHOICE_REQUEST);

      await expect(result).rejects.toBe(failure);
    });
  });
});

function givenEventsResponse(): LifeEventsResponse {
  return {
    lifeEventCapability: 'deterministicChoices',
    insuranceCapability: 'unavailable',
    pendingEvents: [
      {
        id: '71',
        eventKey: 'fictionalDependentCareRequest',
        displayName: '가족 돌봄 요청',
        offeredGameDay: 17,
        expiresGameDay: 24,
        defaultChoiceId: '82',
        choices: [
          {
            id: '81',
            displayName: '지금 돕는다',
            decisionKind: 'accepted',
            effectSummary: { kind: 'walletExpense', amountKrw: 120_000 },
          },
          {
            id: '82',
            displayName: '이번에는 돕지 않는다',
            decisionKind: 'declined',
            effectSummary: { kind: 'noEffect' },
          },
        ],
      },
    ],
    history: [],
    nextCursor: null,
  };
}

function givenChoiceResponse(): LifeEventChoiceResponse {
  return {
    result: {
      eventId: '71',
      choiceId: '81',
      resolutionKind: 'accepted',
      resolvedGameDay: 17,
      walletDeltaKrw: -120_000,
    },
    replayed: false,
    snapshot: givenSnapshot(),
  };
}

interface SnapshotOptions {
  readonly runRevision?: number;
  readonly stateRevision?: number;
  readonly gameDay?: number;
  readonly marketDate?: string;
}

function givenSnapshot(options: SnapshotOptions = {}): GameSnapshot {
  return {
    runRevision: options.runRevision ?? 3,
    stateRevision: options.stateRevision ?? 43,
    gameDay: options.gameDay ?? 17,
    startDate: '2026-01-01',
    cashKrw: 10_000_000,
    debtKrw: 0,
    netWorthKrw: 10_000_000,
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
      insuranceCapability: 'unavailable',
      activeInsuranceContracts: [],
      pendingInsuranceClaims: [],
      pendingEvents: [],
    },
  };
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
  return givenRecordingRejectingHttp(error, []);
}

function givenRecordingRejectingHttp(error: unknown, requests: RecordedRequest[]): HttpClient {
  return {
    async get(path) {
      requests.push({ method: 'GET', path });
      throw error;
    },
    async post(path, body) {
      requests.push({ method: 'POST', path, body });
      throw error;
    },
    async put() {
      throw error;
    },
  };
}
