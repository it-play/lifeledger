import { describe, expect, it } from '@jest/globals';
import type { GameSnapshot } from '../../api/contracts.js';
import { createStore } from '../../lib/store/index.js';
import { initialState } from '../state.js';
import { createGameStateWriter } from './index.js';

const givenSnapshot = (
  gameDay: number,
  autoSpeed: GameSnapshot['autoSpeed'] = null,
  runRevision = 0,
  stateRevision = gameDay,
): GameSnapshot => ({
  runRevision,
  stateRevision,
  gameDay,
  startDate: '2026-01-01',
  cashKrw: 10_000_000,
  debtKrw: 0,
  netWorthKrw: 10_000_000,
  characterName: '테스터',
  autoSpeed,
  market: {
    world: 'm1-2026-v1',
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
    accounts: [{ id: '1', type: 'taxableBrokerage', status: 'open', cashKrw: 0, isDefault: true }],
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
  },
});

describe('게임 스냅샷 적용', () => {
  describe('맥락: 아직 표시한 스냅샷이 없는 경우', () => {
    it('given 첫 스냅샷, when 적용하면, then 그대로 저장한다', () => {
      const store = createStore(initialState);
      const writer = createGameStateWriter({ store });
      const snapshot = givenSnapshot(0);

      const applied = writer.apply(snapshot);

      expect(applied).toBe(true);
      expect(store.getState().game.snapshot).toBe(snapshot);
    });
  });

  describe('맥락: HTTP와 SSE 응답 순서가 엇갈린 경우', () => {
    it('given 더 최신 상태, when 오래된 스냅샷을 적용하면, then 버린다', () => {
      const store = createStore({
        ...initialState,
        game: { ...initialState.game, snapshot: givenSnapshot(8) },
      });
      const writer = createGameStateWriter({ store });

      const applied = writer.apply(givenSnapshot(7));

      expect(applied).toBe(false);
      expect(store.getState().game.snapshot?.gameDay).toBe(8);
    });

    it('given 같은 게임일, when 배속 상태가 바뀌면, then 새 상태를 적용한다', () => {
      const store = createStore({
        ...initialState,
        game: { ...initialState.game, snapshot: givenSnapshot(8, 4) },
      });
      const writer = createGameStateWriter({ store });
      const paused = givenSnapshot(8, null);

      const applied = writer.apply(paused);

      expect(applied).toBe(true);
      expect(store.getState().game.snapshot).toBe(paused);
    });

    it('given 같은 게임일까지 SSE로 받은 상태, when 수동 진행 응답이 늦게 오면, then 배속 상태를 보존한다', () => {
      const current = givenSnapshot(8, 4);
      const store = createStore({
        ...initialState,
        game: { ...initialState.game, snapshot: current },
      });
      const writer = createGameStateWriter({ store });

      const applied = writer.applyIfAhead(givenSnapshot(8, null));

      expect(applied).toBe(false);
      expect(store.getState().game.snapshot).toBe(current);
    });

    it('given 같은 실행의 더 최신 상태 버전, when 같은 게임일 주문 응답을 적용하면, then 반영한다', () => {
      const store = createStore({
        ...initialState,
        game: { ...initialState.game, snapshot: givenSnapshot(8, null, 3, 41) },
      });
      const writer = createGameStateWriter({ store });
      const ordered = givenSnapshot(8, null, 3, 42);

      const applied = writer.applyIfAhead(ordered);

      expect(applied).toBe(true);
      expect(store.getState().game.snapshot).toBe(ordered);
    });

    it('given 게임일은 더 크지만 상태 버전이 이전인 응답, when 적용하면, then 버린다', () => {
      const current = givenSnapshot(8, null, 3, 42);
      const store = createStore({
        ...initialState,
        game: { ...initialState.game, snapshot: current },
      });
      const writer = createGameStateWriter({ store });

      const applied = writer.apply(givenSnapshot(9, null, 3, 41));

      expect(applied).toBe(false);
      expect(store.getState().game.snapshot).toBe(current);
    });

    it('given SSE가 아직 이전 게임일, when 수동 진행 응답이 오면, then 앞선 상태를 보완한다', () => {
      const store = createStore({
        ...initialState,
        game: { ...initialState.game, snapshot: givenSnapshot(7) },
      });
      const writer = createGameStateWriter({ store });
      const response = givenSnapshot(8);

      const applied = writer.applyIfAhead(response);

      expect(applied).toBe(true);
      expect(store.getState().game.snapshot).toBe(response);
    });

    it('given 새 캐릭터 실행 세대, when 0일차 스냅샷을 적용하면, then 재시작을 반영한다', () => {
      const store = createStore({
        ...initialState,
        game: { ...initialState.game, snapshot: givenSnapshot(100, null, 3) },
      });
      const writer = createGameStateWriter({ store });
      const restarted = givenSnapshot(0, null, 4);

      const applied = writer.apply(restarted);

      expect(applied).toBe(true);
      expect(store.getState().game.snapshot).toBe(restarted);
    });

    it('given 이전 캐릭터 실행 세대, when 게임일이 더 커도, then 늦은 응답을 버린다', () => {
      const store = createStore({
        ...initialState,
        game: { ...initialState.game, snapshot: givenSnapshot(0, null, 4) },
      });
      const writer = createGameStateWriter({ store });

      const applied = writer.apply(givenSnapshot(101, null, 3));

      expect(applied).toBe(false);
      expect(store.getState().game.snapshot?.runRevision).toBe(4);
    });
  });
});
