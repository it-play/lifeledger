import { describe, expect, it } from '@jest/globals';
import type { FinanceTransferDraft, GameSnapshot } from '../../api/contracts.js';
import { createFinanceTransferRetryPolicy } from './index.js';

const DRAFT: FinanceTransferDraft = {
  accountId: '1',
  direction: 'walletToAccount',
  amountKrw: 1_000_000,
};

describe('금융 이체 재시도 요청 선택', () => {
  describe('맥락: 응답 유실 뒤 더 최신 스냅샷이 도착한 경우', () => {
    it('given 결과를 모르는 이체, when 같은 내용을 다시 제출하면, then 기존 명령을 그대로 사용한다', () => {
      const policy = createFinanceTransferRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(41), DRAFT);
      policy.retain(first);

      const selected = policy.select(givenSnapshot(42), DRAFT);

      expect(selected).toBe(first);
    });
  });

  describe('맥락: 이전 이체 결과를 확인한 경우', () => {
    it('given 확인이 끝난 이체, when 같은 내용을 제출하면, then 새 명령을 사용한다', () => {
      const policy = createFinanceTransferRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(41), DRAFT);
      policy.retain(first);
      policy.clear(first);

      const selected = policy.select(givenSnapshot(42), DRAFT);

      expect(selected).not.toBe(first);
    });
  });

  describe('맥락: 보류 중인 이체와 다른 내용을 제출한 경우', () => {
    it('given 지갑에서 계좌로 보내는 이체, when 반대 방향을 제출하면, then 새 명령을 사용한다', () => {
      const policy = createFinanceTransferRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(41), DRAFT);
      policy.retain(first);

      const selected = policy.select(givenSnapshot(42), {
        ...DRAFT,
        direction: 'accountToWallet',
      });

      expect(selected).not.toBe(first);
    });
  });
});

function givenCommandIds(): () => string {
  const ids = ['00000000-0000-0000-0000-000000000001', '00000000-0000-0000-0000-000000000002'];
  let index = 0;
  return () => {
    const id = ids[index];
    index += 1;
    return id ?? '00000000-0000-0000-0000-000000000003';
  };
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
      world: 'm1-2026-v1',
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
