import { describe, expect, it } from '@jest/globals';
import { CareerCommandError } from '../../api/career-api.js';
import type { GameSnapshot } from '../../api/contracts.js';
import {
  createCareerActivityCancelRetryPolicy,
  createCareerActivityStartRetryPolicy,
  createCareerArtifactRetryPolicy,
  createMilitarySavingsCloseRetryPolicy,
  createMilitarySavingsEnrollmentRetryPolicy,
  createMilitaryServiceStartRetryPolicy,
} from './index.js';

describe('커리어 명령 재시도 보존', () => {
  describe('맥락: 산출물 응답을 알 수 없는 전송 실패인 경우', () => {
    it('given 자유문구와 evidence 순서, when 다시 선택하면, then UUID와 원문과 순서를 그대로 쓴다', () => {
      const policy = createCareerArtifactRetryPolicy({ createCommandId: givenCommandIds() });
      const draft = {
        kind: 'resume' as const,
        headline: '서버가 저장했을 수 있는 이력서',
        summary: '첫 줄\n둘째 줄',
        evidenceIds: ['9', '3', '7'],
      };
      const first = policy.select(givenSnapshot(11), draft);
      policy.fail(first, new Error('connection reset'));

      const retried = policy.select(givenSnapshot(12), draft);

      expect(retried).toBe(first);
      expect(retried.commandId).toBe(first.commandId);
      expect(retried.expectedStateRevision).toBe(11);
      expect(retried.summary).toBe('첫 줄\n둘째 줄');
      expect(retried.evidenceIds).toEqual(['9', '3', '7']);
    });
  });

  describe('맥락: 서버가 결정론적으로 활동 시작을 거절한 경우', () => {
    it('given activityLimit, when 다시 선택하면, then 새 cursor와 UUID를 만든다', () => {
      const policy = createCareerActivityStartRetryPolicy({ createCommandId: givenCommandIds() });
      const draft = { activityCatalogEntryId: '5', priority: 2 };
      const first = policy.select(givenSnapshot(11), draft);
      policy.fail(first, new CareerCommandError('activityLimit', '활동 슬롯이 가득 찼습니다'));

      const next = policy.select(givenSnapshot(12), draft);

      expect(next.commandId).not.toBe(first.commandId);
      expect(next.expectedStateRevision).toBe(12);
    });
  });

  describe('맥락: 취소 명령의 path를 보존해야 하는 경우', () => {
    it('given 전송 실패, when 재시도하면, then activity ID와 body를 함께 보존한다', () => {
      const policy = createCareerActivityCancelRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(11), { activityId: '41' });
      policy.fail(first, new TypeError('network'));

      const retried = policy.select(givenSnapshot(20), { activityId: '41' });

      expect(retried).toBe(first);
      expect(retried.activityId).toBe('41');
      expect(retried.request.expectedStateRevision).toBe(11);
    });
  });

  describe('맥락: 복무 시작 응답을 알 수 없는 전송 실패인 경우', () => {
    it('given option과 최초 cursor, when 다시 선택하면, then 같은 UUID와 cursor를 보존한다', () => {
      const policy = createMilitaryServiceStartRetryPolicy({ createCommandId: givenCommandIds() });
      const draft = { militaryOptionVersionId: '7' };
      const first = policy.select(givenSnapshot(11), draft);
      policy.fail(first, new TypeError('network'));

      const retried = policy.select(givenSnapshot(20), draft);

      expect(retried).toBe(first);
      expect(retried.commandId).toBe(first.commandId);
      expect(retried.expectedStateRevision).toBe(11);
    });
  });

  describe('맥락: 장병적금 가입을 서버가 결정론적으로 거절한 경우', () => {
    it('given 기관 한도 거절, when 다시 선택하면, then 새 UUID와 최신 cursor를 만든다', () => {
      const policy = createMilitarySavingsEnrollmentRetryPolicy({
        createCommandId: givenCommandIds(),
      });
      const draft = {
        productVersionId: '9',
        monthlyContributionKrw: 250_000,
        debitDayOfMonth: 25,
      };
      const first = policy.select(givenSnapshot(11), draft);
      policy.fail(first, new CareerCommandError('limitExceeded', '가입 한도를 넘었습니다.'));

      const next = policy.select(givenSnapshot(12), draft);

      expect(next.commandId).not.toBe(first.commandId);
      expect(next.expectedStateRevision).toBe(12);
    });
  });

  describe('맥락: 장병적금 중도해지 path를 보존해야 하는 경우', () => {
    it('given 전송 실패, when 재시도하면, then contract ID와 body를 함께 보존한다', () => {
      const policy = createMilitarySavingsCloseRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(11), '41');
      policy.fail(first, new Error('connection reset'));

      const retried = policy.select(givenSnapshot(20), '41');

      expect(retried).toBe(first);
      expect(retried.contractId).toBe('41');
      expect(retried.request.expectedStateRevision).toBe(11);
    });
  });
});

function givenCommandIds(): () => string {
  let next = 1;
  return () => `00000000-0000-0000-0000-${String(next++).padStart(12, '0')}`;
}

function givenSnapshot(stateRevision: number): GameSnapshot {
  return {
    runRevision: 3,
    stateRevision,
    gameDay: 8,
    startDate: '2026-01-01',
    cashKrw: 10_000_000,
    debtKrw: 0,
    netWorthKrw: 10_000_000,
    characterName: '테스터',
    autoSpeed: null,
    market: {
      world: 'm1-2026-v3',
      date: '2026-01-09',
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
