import { describe, expect, it } from '@jest/globals';
import type { CharacterDraft, GameSnapshot } from '../../api/contracts.js';
import { createAdvanceRetryPolicy, createCharacterStartRetryPolicy } from './index.js';

const CHARACTER: CharacterDraft = {
  name: '테스터',
  age: 25,
  gender: 'other',
  military: 'completed',
  region: 'capitalArea',
  background: 'independent',
  education: 'bachelor',
  careerYears: 1,
  certifications: 1,
  startingCashKrw: 10_000_000,
  studentLoanKrw: 0,
  creditLoanKrw: 0,
  health: 'normal',
  dependents: 0,
};

describe('캐릭터 시작 재시도 요청 선택', () => {
  describe('맥락: 응답 유실 뒤 새 런 SSE가 먼저 도착한 경우', () => {
    it('given 결과를 모르는 시작 명령, when 같은 캐릭터를 다시 제출하면, then 최초 요청을 그대로 사용한다', () => {
      const policy = createCharacterStartRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(3, 42, 17), CHARACTER);
      policy.retain(first);

      const selected = policy.select(givenSnapshot(4, 0, 0), CHARACTER);

      expect(selected).toBe(first);
    });
  });

  describe('맥락: 보류 중인 명령과 다른 캐릭터를 제출한 경우', () => {
    it('given 이전 이름의 시작 명령, when 이름을 바꿔 제출하면, then 새 UUID를 사용한다', () => {
      const policy = createCharacterStartRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(3, 42, 17), CHARACTER);
      policy.retain(first);

      const selected = policy.select(givenSnapshot(3, 42, 17), { ...CHARACTER, name: '새 이름' });

      expect(selected.commandId).not.toBe(first.commandId);
    });
  });

  describe('맥락: 시작 결과를 확인한 경우', () => {
    it('given 완료한 시작 명령, when 같은 캐릭터를 다시 제출하면, then 새 UUID를 사용한다', () => {
      const policy = createCharacterStartRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(3, 42, 17), CHARACTER);
      policy.retain(first);
      policy.clear(first);

      const selected = policy.select(givenSnapshot(4, 0, 0), CHARACTER);

      expect(selected.commandId).not.toBe(first.commandId);
    });
  });
});

describe('수동 진행 재시도 요청 선택', () => {
  describe('맥락: 다일 명령 일부가 SSE로 먼저 도착한 경우', () => {
    it('given 결과를 모르는 7일 명령, when 같은 일수를 다시 요청하면, then 최초 cursor와 UUID를 보존한다', () => {
      const policy = createAdvanceRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(3, 42, 17), 7);
      policy.retain(first);

      const selected = policy.select(givenSnapshot(3, 45, 20), 7);

      expect(selected).toBe(first);
    });
  });

  describe('맥락: 다른 일수로 진행하는 경우', () => {
    it('given 보류 중인 7일 명령, when 1일을 요청하면, then 별도 명령을 만든다', () => {
      const policy = createAdvanceRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(3, 42, 17), 7);
      policy.retain(first);

      const selected = policy.select(givenSnapshot(3, 43, 18), 1);

      expect(selected.commandId).not.toBe(first.commandId);
    });
  });

  describe('맥락: 이전 진행 결과를 확인한 경우', () => {
    it('given 완료한 7일 명령, when 다시 7일을 요청하면, then 최신 cursor의 새 명령을 만든다', () => {
      const policy = createAdvanceRetryPolicy({ createCommandId: givenCommandIds() });
      const first = policy.select(givenSnapshot(3, 42, 17), 7);
      policy.retain(first);
      policy.clear(first);

      const selected = policy.select(givenSnapshot(3, 49, 24), 7);

      expect(selected.commandId).not.toBe(first.commandId);
      expect(selected.expectedStateRevision).toBe(49);
    });
  });
});

function givenCommandIds(): () => string {
  let next = 1;
  return () => `00000000-0000-0000-0000-${String(next++).padStart(12, '0')}`;
}

function givenSnapshot(runRevision: number, stateRevision: number, gameDay: number): GameSnapshot {
  return {
    runRevision,
    stateRevision,
    gameDay,
    startDate: '2026-01-01',
    cashKrw: 10_000_000,
    debtKrw: 0,
    netWorthKrw: 10_000_000,
    characterName: runRevision === 3 ? null : '테스터',
    autoSpeed: null,
    market: {
      world: 'm1-2026-v3',
      date: '2026-01-01',
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
    },
  };
}
