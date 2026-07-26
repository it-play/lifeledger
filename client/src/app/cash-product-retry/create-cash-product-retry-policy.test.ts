import { describe, expect, it } from '@jest/globals';
import type { GameSnapshot } from '../../api/contracts.js';
import { FinanceCommandError } from '../../api/game-api.js';
import {
  createCmaAccountCloseRetryPolicy,
  createCmaAccountOpenRetryPolicy,
  createDepositCloseRetryPolicy,
  createDepositOpenRetryPolicy,
} from './index.js';

describe('CMA 개설 명령 재시도', () => {
  describe('맥락: 응답 유실로 결과를 확인할 수 없는 경우', () => {
    it('given 보류된 개설, when 최신 snapshot에서 같은 상품을 다시 선택하면, then UUID·cursor·payload를 보존한다', () => {
      const policy = createCmaAccountOpenRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(41), { type: 'cma', productVersionId: '11' });
      policy.fail(first, new Error('response lost'));

      const selected = policy.select(givenSnapshot(42), { type: 'cma', productVersionId: '11' });

      expect(selected).toBe(first);
    });
  });

  describe('맥락: 서버가 확정적인 금융 오류를 반환한 경우', () => {
    it('given productNotFound, when 같은 상품을 다시 선택하면, then 최신 cursor의 새 UUID를 사용한다', () => {
      const policy = createCmaAccountOpenRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(41), { type: 'cma', productVersionId: '11' });
      policy.fail(first, givenFinanceError());

      const selected = policy.select(givenSnapshot(42), { type: 'cma', productVersionId: '11' });

      expect(selected.commandId).not.toBe(first.commandId);
    });
  });
});

describe('CMA 종료 명령 재시도', () => {
  describe('맥락: 응답 유실로 결과를 확인할 수 없는 경우', () => {
    it('given 보류된 종료, when 같은 계좌를 다시 제출하면, then path·UUID·cursor를 보존한다', () => {
      const policy = createCmaAccountCloseRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(41), { accountId: '2' });
      policy.fail(first, new Error('response lost'));

      const selected = policy.select(givenSnapshot(42), { accountId: '2' });

      expect(selected).toBe(first);
    });
  });

  describe('맥락: 서버가 확정적인 금융 오류를 반환한 경우', () => {
    it('given accountNotEmpty, when 같은 계좌를 다시 제출하면, then 최신 cursor의 새 UUID를 사용한다', () => {
      const policy = createCmaAccountCloseRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(41), { accountId: '2' });
      policy.fail(first, new FinanceCommandError('accountNotEmpty', '계좌가 비어 있지 않습니다'));

      const selected = policy.select(givenSnapshot(42), { accountId: '2' });

      expect(selected.request.commandId).not.toBe(first.request.commandId);
    });
  });
});

describe('예금·적금 가입 명령 재시도', () => {
  const draft = {
    kind: 'termDeposit' as const,
    productVersionId: '12',
    settlementAccountId: '1',
    amountKrw: 1_000_000,
  };

  describe('맥락: 응답 유실로 결과를 확인할 수 없는 경우', () => {
    it('given 보류된 가입, when 최신 snapshot에서 같은 내용을 다시 제출하면, then UUID·cursor·payload를 보존한다', () => {
      const policy = createDepositOpenRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(41), draft);
      policy.fail(first, new Error('response lost'));

      const selected = policy.select(givenSnapshot(42), draft);

      expect(selected).toBe(first);
    });
  });

  describe('맥락: 서버가 확정적인 금융 오류를 반환한 경우', () => {
    it('given productNotFound, when 같은 내용을 다시 제출하면, then 최신 cursor의 새 UUID를 사용한다', () => {
      const policy = createDepositOpenRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(41), draft);
      policy.fail(first, givenFinanceError());

      const selected = policy.select(givenSnapshot(42), draft);

      expect(selected.commandId).not.toBe(first.commandId);
    });
  });
});

describe('예금·적금 중도해지 명령 재시도', () => {
  describe('맥락: 응답 유실로 결과를 확인할 수 없는 경우', () => {
    it('given 보류된 중도해지, when 같은 계약을 다시 제출하면, then path·UUID·cursor를 보존한다', () => {
      const policy = createDepositCloseRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(41), { contractId: '21' });
      policy.fail(first, new Error('response lost'));

      const selected = policy.select(givenSnapshot(42), { contractId: '21' });

      expect(selected).toBe(first);
    });
  });

  describe('맥락: 서버가 확정적인 금융 오류를 반환한 경우', () => {
    it('given contractClosed, when 같은 계약을 다시 제출하면, then 최신 cursor의 새 UUID를 사용한다', () => {
      const policy = createDepositCloseRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(41), { contractId: '21' });
      policy.fail(first, new FinanceCommandError('contractClosed', '이미 닫힌 계약입니다'));

      const selected = policy.select(givenSnapshot(42), { contractId: '21' });

      expect(selected.request.commandId).not.toBe(first.request.commandId);
    });
  });
});

function givenFinanceError(): FinanceCommandError {
  return new FinanceCommandError('productNotFound', '상품을 찾을 수 없습니다');
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
    },
  };
}
