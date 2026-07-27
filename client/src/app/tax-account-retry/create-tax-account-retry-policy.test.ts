import { describe, expect, it } from '@jest/globals';
import type { GameSnapshot } from '../../api/contracts.js';
import { FinanceCommandError } from '../../api/game-api.js';
import {
  createIsaAccountCloseRetryPolicy,
  createPensionStartRetryPolicy,
  createPensionWithdrawalRetryPolicy,
  createTaxAccountOpenRetryPolicy,
} from './index.js';

describe('절세계좌 개설 명령 재시도', () => {
  describe('맥락: 응답 유실로 처리 여부를 확인할 수 없는 경우', () => {
    it('given 보류된 ISA 개설, when 최신 snapshot에서 같은 유형을 제출하면, then UUID·cursor·payload를 보존한다', () => {
      const policy = createTaxAccountOpenRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(41), { type: 'isaGeneral' });
      policy.fail(first, new Error('response lost'));

      const selected = policy.select(givenSnapshot(42), { type: 'isaGeneral' });

      expect(selected).toBe(first);
    });
  });

  describe('맥락: cursor 경쟁으로 서버가 busy를 반환한 경우', () => {
    it('given 거절된 IRP 개설, when 최신 snapshot에서 다시 제출하면, then 같은 payload와 최신 cursor의 새 UUID를 사용한다', () => {
      const policy = createTaxAccountOpenRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(41), { type: 'irp' });
      policy.fail(first, givenBusy());

      const selected = policy.select(givenSnapshot(42), { type: 'irp' });

      expect(selected).toMatchObject({
        type: first.type,
        expectedStateRevision: 42,
      });
      expect(selected.commandId).not.toBe(first.commandId);
    });
  });
});

describe('ISA 해지 명령 재시도', () => {
  describe('맥락: 응답 유실로 처리 여부를 확인할 수 없는 경우', () => {
    it('given 보류된 해지, when 같은 계좌를 다시 제출하면, then path·UUID·cursor를 보존한다', () => {
      const policy = createIsaAccountCloseRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(41), { accountId: '2' });
      policy.fail(first, new Error('response lost'));

      const selected = policy.select(givenSnapshot(42), { accountId: '2' });

      expect(selected).toBe(first);
    });
  });

  describe('맥락: cursor 경쟁으로 서버가 busy를 반환한 경우', () => {
    it('given 거절된 해지, when 최신 snapshot에서 다시 제출하면, then 같은 계좌와 최신 cursor를 사용한다', () => {
      const policy = createIsaAccountCloseRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(41), { accountId: '2' });
      policy.fail(first, givenBusy());

      const selected = policy.select(givenSnapshot(42), { accountId: '2' });

      expect(selected).toMatchObject({
        accountId: first.accountId,
        request: { expectedStateRevision: 42 },
      });
    });
  });
});

describe('연금 개시 명령 재시도', () => {
  const draft = { accountId: '3', paymentYears: 10, lifetime: false } as const;

  describe('맥락: 응답 유실로 처리 여부를 확인할 수 없는 경우', () => {
    it('given 보류된 개시, when 같은 지급조건을 다시 제출하면, then path·UUID·cursor·payload를 보존한다', () => {
      const policy = createPensionStartRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(41), draft);
      policy.fail(first, new Error('response lost'));

      const selected = policy.select(givenSnapshot(42), draft);

      expect(selected).toBe(first);
    });
  });

  describe('맥락: cursor 경쟁으로 서버가 busy를 반환한 경우', () => {
    it('given 거절된 개시, when 최신 snapshot에서 다시 제출하면, then 지급조건을 유지하고 최신 cursor를 사용한다', () => {
      const policy = createPensionStartRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(41), draft);
      policy.fail(first, givenBusy());

      const selected = policy.select(givenSnapshot(42), draft);

      expect(selected.request).toMatchObject({
        paymentYears: 10,
        lifetime: false,
        expectedStateRevision: 42,
      });
    });
  });
});

describe('연금 인출 명령 재시도', () => {
  const draft = {
    accountId: '3',
    amountKrw: 1_000_000,
    type: 'nonPension',
    reason: null,
  } as const;

  describe('맥락: 응답 유실로 처리 여부를 확인할 수 없는 경우', () => {
    it('given 보류된 인출, when 같은 내용을 다시 제출하면, then required null을 포함한 전체 명령을 보존한다', () => {
      const policy = createPensionWithdrawalRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(41), draft);
      policy.fail(first, new Error('response lost'));

      const selected = policy.select(givenSnapshot(42), draft);

      expect(selected).toBe(first);
      expect(selected.request).toHaveProperty('reason', null);
    });
  });

  describe('맥락: cursor 경쟁으로 서버가 busy를 반환한 경우', () => {
    it('given 거절된 인출, when 최신 snapshot에서 다시 제출하면, then 금액·유형·사유와 최신 cursor를 사용한다', () => {
      const policy = createPensionWithdrawalRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(41), draft);
      policy.fail(first, givenBusy());

      const selected = policy.select(givenSnapshot(42), draft);

      expect(selected.request).toMatchObject({
        amountKrw: 1_000_000,
        type: 'nonPension',
        reason: null,
        expectedStateRevision: 42,
      });
    });
  });
});

function givenBusy(): FinanceCommandError {
  return new FinanceCommandError('busy', '최신 상태에서 다시 시도하세요');
}

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
      accounts: [
        { id: '1', type: 'taxableBrokerage', status: 'open', cashKrw: 0, isDefault: true },
      ],
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
