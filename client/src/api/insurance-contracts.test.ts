import { describe, expect, it } from '@jest/globals';
import {
  InsuranceCancellationRequestSchema,
  InsuranceClaimRequestSchema,
  InsuranceContractsResponseSchema,
  InsuranceEnrollmentRequestSchema,
  InsuranceProductSchema,
  LedgerAccountCodeSchema,
  LedgerSourceKindSchema,
  PendingInsuranceClaimSchema,
  SettlementKindSchema,
} from './contracts.js';

describe('보험 공개 계약', () => {
  describe('맥락: 계약과 청구를 지원하는 실행을 조회한 경우', () => {
    it('given exact 6개 필드, when 검증하면, then product·contract·claim을 strict union으로 받는다', () => {
      const response = InsuranceContractsResponseSchema.parse(givenInsuranceResponse());

      expect(Object.keys(response)).toEqual([
        'insuranceCapability',
        'products',
        'contracts',
        'pendingClaims',
        'history',
        'nextCursor',
      ]);
      expect(response.pendingClaims.map((claim) => claim.status)).toEqual(['candidate', 'ready']);
      expect(response.history.map((claim) => claim.status)).toEqual([
        'paid',
        'expired',
        'notCovered',
        'notApplicable',
      ]);
    });

    it('given 내부 fact AST 필드, when 검증하면, then 공개 계약 밖의 필드를 거절한다', () => {
      const response = givenInsuranceResponse();

      const result = InsuranceContractsResponseSchema.safeParse({
        ...response,
        products: [{ ...response.products[0], rawFacts: { age: 30 } }],
      });

      expect(result.success).toBe(false);
    });

    it('given 가입일에 해지해 대기기간보다 먼저 끝난 계약, when 검증하면, then 종료 이력을 허용한다', () => {
      const response = givenInsuranceResponse();
      const contract = givenContract();

      const result = InsuranceContractsResponseSchema.safeParse({
        ...response,
        contracts: [
          {
            ...contract,
            status: 'cancelled',
            coverageEndExclusive: 1,
            nextPremiumDueGameDay: null,
          },
        ],
      });

      expect(result.success).toBe(true);
    });
  });

  describe('맥락: 보험 기능이 없는 기존 실행인 경우', () => {
    it('given unavailable인데 product가 존재함, when 검증하면, then 빈 호환 응답 위반을 거절한다', () => {
      const response = givenInsuranceResponse();

      const result = InsuranceContractsResponseSchema.safeParse({
        ...response,
        insuranceCapability: 'unavailable',
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 자격 fact를 확정할 수 없는 경우', () => {
    it('given indeterminate와 authorityUnavailable, when 검증하면, then 불자격으로 바꾸지 않고 공개한다', () => {
      const product = givenInsuranceResponse().products[0];
      if (product === undefined) throw new Error('보험 상품 fixture가 없습니다.');

      const parsed = InsuranceProductSchema.parse({
        ...product,
        eligibilityStatus: 'indeterminate',
        reasons: ['authorityUnavailable'],
      });

      expect(parsed.eligibilityStatus).toBe('indeterminate');
    });

    it('given indeterminate인데 authority reason 없음, when 검증하면, then strict 자격 상관관계를 거절한다', () => {
      const product = givenInsuranceResponse().products[0];
      if (product === undefined) throw new Error('보험 상품 fixture가 없습니다.');

      const result = InsuranceProductSchema.safeParse({
        ...product,
        eligibilityStatus: 'indeterminate',
        reasons: ['dependentRequired'],
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: ready claim의 계약별 배분을 공개한 경우', () => {
    it('given claim payout과 합계가 다른 allocation, when 검증하면, then 배분 모순을 거절한다', () => {
      const ready = givenReadyClaim();

      const result = PendingInsuranceClaimSchema.safeParse({
        ...ready,
        contractAllocations: [{ contractId: '91', deductibleKrw: 20_000, payoutKrw: 90_000 }],
      });

      expect(result.success).toBe(false);
    });

    it('given 내림차순 contract ID, when 검증하면, then canonical allocation 순서 위반을 거절한다', () => {
      const ready = givenReadyClaim();

      const result = PendingInsuranceClaimSchema.safeParse({
        ...ready,
        payoutKrw: 100_000,
        contractAllocations: [
          { contractId: '92', deductibleKrw: 20_000, payoutKrw: 50_000 },
          { contractId: '91', deductibleKrw: 20_000, payoutKrw: 50_000 },
        ],
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 가입·취소·청구 body를 만든 경우', () => {
    it('given 가입 body에 보험료를 넣음, when 검증하면, then 서버 권한 필드를 거절한다', () => {
      const result = InsuranceEnrollmentRequestSchema.safeParse({
        ...givenCursor(),
        productVersionId: '71',
        premiumKrw: 1,
      });

      expect(result.success).toBe(false);
    });

    it('given 취소 body에 path contract ID를 중복함, when 검증하면, then 공통 cursor만 허용한다', () => {
      const result = InsuranceCancellationRequestSchema.safeParse({
        ...givenCursor(),
        contractId: '91',
      });

      expect(result.success).toBe(false);
    });

    it('given 청구 body에 payout을 넣음, when 검증하면, then claim ID 외 판정값을 거절한다', () => {
      const result = InsuranceClaimRequestSchema.safeParse({
        ...givenCursor(),
        claimId: '102',
        payoutKrw: 100_000,
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 보험료와 보험금을 원장에 공개하는 경우', () => {
    it('given D3 settlement·source·account enum, when 검증하면, then exact protocol 이름을 허용한다', () => {
      const results = [
        SettlementKindSchema.safeParse('insurancePremium').success,
        LedgerSourceKindSchema.safeParse('insurancePremiumPayment').success,
        LedgerSourceKindSchema.safeParse('insuranceClaimPayment').success,
        LedgerAccountCodeSchema.safeParse('insurancePremiumExpense').success,
        LedgerAccountCodeSchema.safeParse('insuranceClaimRecovery').success,
      ];

      expect(results).toEqual([true, true, true, true, true]);
    });
  });
});

function givenInsuranceResponse(): Record<string, unknown> & {
  readonly products: readonly Record<string, unknown>[];
} {
  return {
    insuranceCapability: 'contractsAndClaims',
    products: [
      {
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
      },
    ],
    contracts: [givenContract()],
    pendingClaims: [givenCandidateClaim(), givenReadyClaim()],
    history: [
      { ...givenReadyClaim(), id: '120', status: 'paid', resolvedGameDay: 31, paidGameDay: 32 },
      { ...givenReadyClaim(), id: '119', status: 'expired', resolvedGameDay: 31 },
      {
        ...givenCandidateClaim(),
        id: '118',
        offeredGameDay: 20,
        status: 'notCovered',
        resolvedGameDay: 30,
        grossCostKrw: 120_000,
        payoutKrw: 0,
      },
      {
        ...givenCandidateClaim(),
        id: '117',
        offeredGameDay: 20,
        status: 'notApplicable',
        resolvedGameDay: 29,
      },
    ],
    nextCursor: 'opaque.contract+claim/=',
  };
}

function givenContract(): Record<string, unknown> {
  return {
    id: '91',
    productVersionId: '71',
    productKey: 'fictionalFamilyCareCover',
    displayName: '가족 돌봄 비용 보장',
    status: 'active',
    coverageStartGameDay: 0,
    waitingEndsGameDay: 7,
    coverageEndExclusive: 360,
    nextPremiumDueGameDay: 60,
    premiumKrw: 10_000,
    paidBenefitKrw: 0,
    reservedBenefitKrw: 100_000,
    remainingBenefitKrw: 100_000,
  };
}

function givenCandidateClaim(): Record<string, unknown> {
  return {
    id: '101',
    eventId: '81',
    eventKey: 'fictionalDependentCareRequest',
    eventDisplayName: '가족 돌봄 요청',
    offeredGameDay: 31,
    status: 'candidate',
    grossCostKrw: null,
    payoutKrw: null,
    filingDeadlineGameDay: null,
  };
}

function givenReadyClaim(): Record<string, unknown> {
  return {
    id: '102',
    eventId: '82',
    eventKey: 'fictionalDependentCareRequest',
    eventDisplayName: '가족 돌봄 요청',
    offeredGameDay: 31,
    status: 'ready',
    grossCostKrw: 120_000,
    payoutKrw: 100_000,
    filingDeadlineGameDay: 39,
    contractAllocations: [{ contractId: '91', deductibleKrw: 20_000, payoutKrw: 100_000 }],
  };
}

function givenCursor(): Record<string, unknown> {
  return {
    commandId: '00000000-0000-4000-8000-000000000001',
    expectedRunRevision: 3,
    expectedStateRevision: 42,
    expectedGameDay: 17,
  };
}
