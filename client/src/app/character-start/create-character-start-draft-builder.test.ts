import { describe, expect, it } from '@jest/globals';
import type { CharacterDraft, LoanProduct, LoanProductCatalog } from '../../api/contracts.js';
import { createCharacterStartDraftBuilder } from './index.js';

describe('캐릭터 시작 대출 상품 해석', () => {
  describe('맥락: 두 종류의 시작 부채가 있는 경우', () => {
    it('given 종류별 유일한 상품, when 요청 초안을 만들면, then 상품 ID를 canonical 순서로 붙인다', () => {
      const builder = createCharacterStartDraftBuilder();

      const result = builder.build(
        { ...givenCharacter(), studentLoanKrw: 12_000_000, creditLoanKrw: 3_000_000 },
        givenCatalog(),
      );

      expect(result).toEqual({
        ok: true,
        value: {
          character: {
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
            health: 'normal',
            dependents: 0,
          },
          startingLoans: [
            { kind: 'studentLoan', productVersionId: '20', principalKrw: 12_000_000 },
            { kind: 'unsecuredLoan', productVersionId: '21', principalKrw: 3_000_000 },
          ],
        },
      });
    });
  });

  describe('맥락: 시작 부채가 없는 경우', () => {
    it('given 상품 조회 실패, when 요청 초안을 만들면, then 빈 대출 목록으로 시작을 허용한다', () => {
      const builder = createCharacterStartDraftBuilder();

      const result = builder.build(givenCharacter(), undefined);

      expect(result.ok && result.value.startingLoans).toEqual([]);
    });
  });

  describe('맥락: 시작 상품을 확정할 수 없는 경우', () => {
    it('given 양수 학자금과 빈 catalog, when 요청 초안을 만들면, then 해당 입력 오류를 반환한다', () => {
      const builder = createCharacterStartDraftBuilder();

      const result = builder.build(
        { ...givenCharacter(), studentLoanKrw: 1_000_000 },
        { creditModelVersionId: null, products: [] },
      );

      expect(result.ok ? undefined : result.errors.studentLoanKrw).toBeDefined();
    });
  });

  describe('맥락: 상품 원금 범위를 벗어난 경우', () => {
    it('given 최대액 초과 신용대출, when 요청 초안을 만들면, then 신용 부채 입력을 거절한다', () => {
      const builder = createCharacterStartDraftBuilder();

      const result = builder.build(
        { ...givenCharacter(), creditLoanKrw: 200_000_001 },
        givenCatalog(),
      );

      expect(result.ok ? undefined : result.errors.creditLoanKrw).toBeDefined();
    });
  });
});

function givenCharacter(): CharacterDraft {
  return {
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
}

function givenCatalog(): LoanProductCatalog {
  return {
    creditModelVersionId: '5',
    products: [
      givenProduct({
        id: '20',
        key: 'student',
        kind: 'studentLoan',
        rateType: 'fixed',
        currentAnnualRateBp: 170,
        referenceRateKey: null,
        spreadBp: null,
        minimumAnnualRateBp: 170,
        maximumAnnualRateBp: 170,
        rateResetRule: 'none',
        repaymentMethod: 'equalPrincipal',
        termMonths: 120,
        maximumPrincipalKrw: 50_000_000,
        prepaymentFeePpm: 0,
        prepaymentEffect: 'reduceTerm',
        quoteEligible: false,
        executionEligible: false,
      }),
      givenProduct({ id: '21', key: 'unsecured', kind: 'unsecuredLoan' }),
    ],
  };
}

function givenProduct(overrides: Partial<LoanProduct>): LoanProduct {
  return {
    id: '21',
    key: 'unsecured',
    displayName: '개발 대출',
    kind: 'unsecuredLoan',
    lenderSector: 'bank',
    rateStatus: 'available',
    rateType: 'variable',
    currentAnnualRateBp: 655,
    referenceRateKey: 'treasury3m',
    spreadBp: 400,
    minimumAnnualRateBp: 300,
    maximumAnnualRateBp: 1_500,
    rateResetRule: 'monthlyDay1',
    dayCountRule: 'actual365',
    repaymentMethod: 'levelPayment',
    termMonths: 60,
    paymentCalendar: 'monthEnd',
    graceMonths: 0,
    minimumPrincipalKrw: 1,
    maximumPrincipalKrw: 200_000_000,
    prepaymentFeePpm: 10_000,
    prepaymentEffect: 'recalculatePayment',
    startingEligible: true,
    quoteEligible: true,
    executionEligible: true,
    prepaymentAllowed: true,
    dsrIncluded: true,
    provenance: 'gameBalance',
    ...overrides,
  };
}
