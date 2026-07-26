import { describe, expect, it } from '@jest/globals';
import type { GameSnapshot, PortfolioOrderDraft } from '../../api/contracts.js';
import { createOrderRetryPolicy } from './index.js';

const DRAFT: PortfolioOrderDraft = { accountId: '1', side: 'buy', quantity: 3 };

describe('주문 재시도 요청 선택', () => {
  describe('맥락: 응답 유실 뒤 같은 게임일의 SSE 스냅샷이 먼저 도착한 경우', () => {
    it('given 결과를 모르는 주문, when 같은 내용을 다시 제출하면, then 기존 주문 요청을 그대로 사용한다', () => {
      const policy = createOrderRetryPolicy({ createOrderId: givenOrderIds() });
      const first = policy.select(givenSnapshot(41), DRAFT);
      policy.retain(first);

      const selected = policy.select(givenSnapshot(42), DRAFT);

      expect(selected).toBe(first);
    });
  });

  describe('맥락: 결과를 모르는 주문과 다른 내용을 제출한 경우', () => {
    it('given 보류 중인 매수 주문, when 매도 주문을 제출하면, then 새 주문 요청을 사용한다', () => {
      const policy = createOrderRetryPolicy({ createOrderId: givenOrderIds() });
      const first = policy.select(givenSnapshot(41), DRAFT);
      policy.retain(first);

      const selected = policy.select(givenSnapshot(42), {
        accountId: '1',
        side: 'sell',
        quantity: 3,
      });

      expect(selected).not.toBe(first);
    });
  });

  describe('맥락: 이전 주문의 결과를 확인한 경우', () => {
    it('given 확인이 끝난 주문, when 같은 내용을 다시 제출하면, then 새 주문 요청을 사용한다', () => {
      const policy = createOrderRetryPolicy({ createOrderId: givenOrderIds() });
      const first = policy.select(givenSnapshot(41), DRAFT);
      policy.retain(first);
      policy.clear(first);

      const selected = policy.select(givenSnapshot(42), DRAFT);

      expect(selected).not.toBe(first);
    });
  });

  describe('맥락: 다른 주문의 결과를 먼저 확인한 경우', () => {
    it('given 결과를 모르는 매수 주문, when 매도 주문이 끝나면, then 매수 재시도 요청을 보존한다', () => {
      const policy = createOrderRetryPolicy({ createOrderId: givenOrderIds() });
      const first = policy.select(givenSnapshot(41), DRAFT);
      policy.retain(first);
      const other = policy.select(givenSnapshot(42), {
        accountId: '1',
        side: 'sell',
        quantity: 3,
      });
      policy.clear(other);

      const selected = policy.select(givenSnapshot(43), DRAFT);

      expect(selected).toBe(first);
    });
  });
});

function givenOrderIds(): () => string {
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
    },
  };
}
