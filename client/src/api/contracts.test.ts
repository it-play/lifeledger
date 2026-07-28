import { describe, expect, it } from '@jest/globals';
import {
  BondOrderRequestSchema,
  BondOrderResultSchema,
  CareerActivitiesResponseSchema,
  CareerApplicationRequestSchema,
  CareerArtifactDraftSchema,
  CareerArtifactPublishRequestSchema,
  CareerArtifactsResponseSchema,
  CareerFailureCodeSchema,
  CareerJobsResponseSchema,
  CareerPayrollResponseSchema,
  CareerSnapshotSchema,
  CareerSpecsResponseSchema,
  CareerTaxYearStateSchema,
  CashContractSummarySchema,
  type CashProduct,
  CashProductCatalogSchema,
  CharacterStartRequestSchema,
  CreditResponseSchema,
  EssentialArrearPaymentRequestSchema,
  FinanceFailureCodeSchema,
  FinanceSnapshotSchema,
  type FinancialIncomeYear,
  FinancialIncomeYearSchema,
  GoldProductCatalogSchema,
  GoldWithdrawalResultSchema,
  HousingCurrentLeaseResponseSchema,
  HousingLeaseArrearPaymentRequestSchema,
  HousingLeaseArrearPaymentResultSchema,
  HousingLeaseDepositLoanQuoteRequestSchema,
  HousingLeaseDepositLoanQuoteResultSchema,
  HousingLeaseRequestSchema,
  HousingLeaseResultSchema,
  HousingListingsQuerySchema,
  HousingListingsResponseSchema,
  HousingMortgageProductSchema,
  HousingMortgageQuoteRequestSchema,
  HousingMortgageQuoteResultSchema,
  HousingPropertyHoldingsResponseSchema,
  HousingPurchaseRequestSchema,
  HousingPurchaseResultSchema,
  IsaAccountSummarySchema,
  LedgerAccountCodeSchema,
  type LedgerPage,
  LedgerPageSchema,
  LedgerSourceKindSchema,
  LifeBudgetResponseSchema,
  LifeBudgetUpdateRequestSchema,
  LifeFailureCodeSchema,
  LifeSnapshotSchema,
  LivingCostMonthSchema,
  LoanDetailSchema,
  LoanExecutionRequestSchema,
  LoanExecutionResultSchema,
  LoanInstallmentHistoryQuerySchema,
  LoanInstallmentHistoryResponseSchema,
  LoanPrepaymentRequestSchema,
  LoanPrepaymentResultSchema,
  LoanProductCatalogSchema,
  LoanQuoteRequestSchema,
  LoanQuoteResultSchema,
  type MarketHistory,
  MarketHistorySchema,
  MilitaryOptionsResponseSchema,
  MilitarySavingsHistoryResponseSchema,
  MilitarySavingsProductsResponseSchema,
  MilitaryServiceResponseSchema,
  MilitaryServiceStartRequestSchema,
  PensionAccountSummarySchema,
  PensionWithdrawalRequestSchema,
  PensionWithdrawalResultSchema,
  PortfolioExecutionSchema,
  PortfolioOrderRequestSchema,
  SettlementKindSchema,
} from './contracts.js';

const givenCmaProduct = (): CashProduct => ({
  id: '11',
  key: 'cma-rp-2026-v1',
  kind: 'cmaRp',
  displayName: 'RP형 CMA',
  institution: { id: '1', key: 'life-securities', displayName: '라이프증권' },
  protectionEligible: false,
  rateReference: 'treasury3mBp',
  spreadBp: 20,
  minimumInterestBalanceKrw: 10_000,
  dayCountDenominator: 365,
});

const givenTermDepositProduct = (): CashProduct => ({
  id: '12',
  key: 'term-deposit-365d-2026-v1',
  kind: 'termDeposit',
  displayName: '1년 정기예금',
  institution: { id: '2', key: 'life-bank', displayName: '라이프은행' },
  protectionEligible: true,
  rateReference: 'treasury3mBp',
  spreadBp: 80,
  minimumContributionKrw: 100_000,
  maximumContributionKrw: 50_000_000,
  termDays: 365,
  earlyTerminationRateBp: 50,
  dayCountDenominator: 365,
});

const givenLegacyFinancialIncomeYear = (taxYear = 2026): FinancialIncomeYear => ({
  taxYear,
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
});

const givenOpenFinancialIncomeYear = (): FinancialIncomeYear => ({
  taxYear: 2026,
  status: 'open',
  sources: [
    {
      source: 'bondCoupon',
      grossFinancialIncomeKrw: 10_000,
      withheldIncomeTaxKrw: 1_400,
      withheldLocalIncomeTaxKrw: 140,
    },
  ],
  grossFinancialIncomeKrw: 10_000,
  withheldIncomeTaxKrw: 1_400,
  withheldLocalIncomeTaxKrw: 140,
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
});

const givenLedgerPage = (): LedgerPage => ({
  transactions: [
    {
      id: '3',
      gameDay: 4,
      description: '지갑에서 금융계좌로 이체',
      sourceKind: 'transfer',
      postings: [
        { accountCode: 'wallet', accountId: null, amountKrw: -100_000 },
        { accountCode: 'accountCash', accountId: '1', amountKrw: 100_000 },
      ],
    },
    {
      id: '2',
      gameDay: 0,
      description: '새 런 기초 잔액',
      sourceKind: 'm2OpeningBalance',
      postings: [
        { accountCode: 'wallet', accountId: null, amountKrw: 1_000_000 },
        { accountCode: 'openingEquity', accountId: null, amountKrw: -1_000_000 },
      ],
    },
  ],
  nextBefore: '2',
});

const givenHistory = (): MarketHistory => ({
  world: 'm1-2026-v2',
  symbol: 'LLX',
  throughGameDay: 2,
  points: [
    {
      gameDay: 1,
      date: '2026-01-02',
      open: true,
      closeKrw: 100_100,
      dailyReturnPpm: 1_000,
      regime: 'expansion',
      rates: null,
      llxCloseKrw: null,
      llxDailyReturnPpm: null,
    },
    {
      gameDay: 2,
      date: '2026-01-03',
      open: false,
      closeKrw: 100_100,
      dailyReturnPpm: 0,
      regime: 'expansion',
      rates: null,
      llxCloseKrw: null,
      llxDailyReturnPpm: null,
    },
  ],
});

const givenLoanProductCatalog = () => ({
  creditModelVersionId: '5',
  products: [
    {
      id: '20',
      key: 'dev-student-fixed-equal-principal-2026-v1',
      displayName: '개발 학자금 고정금리 대출',
      kind: 'studentLoan',
      lenderSector: 'bank',
      rateStatus: 'available',
      rateType: 'fixed',
      currentAnnualRateBp: 170,
      referenceRateKey: null,
      spreadBp: null,
      minimumAnnualRateBp: 170,
      maximumAnnualRateBp: 170,
      rateResetRule: 'none',
      dayCountRule: 'actual365',
      repaymentMethod: 'equalPrincipal',
      termMonths: 120,
      paymentCalendar: 'monthEnd',
      graceMonths: 0,
      minimumPrincipalKrw: 1,
      maximumPrincipalKrw: 50_000_000,
      prepaymentFeePpm: 0,
      prepaymentEffect: 'reduceTerm',
      startingEligible: true,
      quoteEligible: false,
      executionEligible: false,
      prepaymentAllowed: true,
      dsrIncluded: true,
      provenance: 'gameBalance',
    },
    {
      id: '21',
      key: 'dev-unsecured-variable-level-payment-2026-v1',
      displayName: '개발 변동금리 신용대출',
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
    },
  ],
});

const givenLeaseDepositLoanProduct = () => ({
  id: '22',
  key: 'dev-lease-deposit-fixed-bullet-2026-v1',
  displayName: '개발 전세자금 고정금리 대출',
  kind: 'leaseDepositLoan',
  lenderSector: 'bank',
  rateStatus: 'available',
  rateType: 'fixed',
  currentAnnualRateBp: 400,
  referenceRateKey: null,
  spreadBp: null,
  minimumAnnualRateBp: 400,
  maximumAnnualRateBp: 400,
  rateResetRule: 'none',
  dayCountRule: 'actual365',
  repaymentMethod: 'bullet',
  termMonths: 24,
  paymentCalendar: 'monthEnd',
  graceMonths: 0,
  minimumPrincipalKrw: 1,
  maximumPrincipalKrw: 400_000_000,
  prepaymentFeePpm: 0,
  prepaymentEffect: 'reduceTerm',
  startingEligible: false,
  quoteEligible: true,
  executionEligible: true,
  prepaymentAllowed: true,
  dsrIncluded: false,
  provenance: 'gameBalance',
});

const givenMortgageProduct = () => ({
  id: '23',
  key: 'dev-mortgage-fixed-level-payment-2026-v1',
  displayName: '개발 주택담보 고정금리 대출',
  kind: 'mortgage',
  lenderSector: 'bank',
  rateStatus: 'available',
  rateType: 'fixed',
  currentAnnualRateBp: 400,
  referenceRateKey: null,
  spreadBp: null,
  minimumAnnualRateBp: 400,
  maximumAnnualRateBp: 400,
  rateResetRule: 'none',
  dayCountRule: 'actual365',
  repaymentMethod: 'levelPayment',
  termMonths: 360,
  paymentCalendar: 'monthEnd',
  graceMonths: 0,
  minimumPrincipalKrw: 1,
  maximumPrincipalKrw: 600_000_000,
  prepaymentFeePpm: 10_000,
  prepaymentEffect: 'recalculatePayment',
  startingEligible: false,
  quoteEligible: true,
  executionEligible: true,
  prepaymentAllowed: true,
  dsrIncluded: true,
  provenance: 'gameBalance',
});

const givenLeaseDepositLoanQuoteResult = () => ({
  quoteId: '31',
  listingId: '7002',
  offerKind: 'jeonse',
  productVersionId: '22',
  requestedPrincipalKrw: 80_000_000,
  depositKrw: 100_000_000,
  fundingLimitPpm: 800_000,
  maximumFundingKrw: 80_000_000,
  createdGameDay: 120,
  expiresGameDay: 120,
  decisionCode: 'eligible',
  decisionReasons: ['eligible'],
  verifiedAnnualIncomeKrw: 60_000_000,
  verifiedIncomeSource: 'activeEmploymentContract',
  existingLoanBalanceKrw: 0,
  postExecutionBalanceKrw: 80_000_000,
  regulatoryDsrApplied: false,
  affordability: {
    numeratorKrw: 18_000_000,
    denominatorKrw: 60_000_000,
    ratioPpm: 300_000,
    limitPpm: 400_000,
  },
  quotedTerms: {
    annualRateBp: 400,
    repaymentMethod: 'bullet',
    termMonths: 24,
    firstInstallment: {
      dueGameDay: 151,
      feeKrw: 0,
      principalKrw: 0,
      interestKrw: 271_232,
      totalKrw: 271_232,
    },
  },
  replacedLoanId: null,
  replacedLoanPrincipalKrw: 0,
});

const givenLoanQuoteResult = () => ({
  quoteId: '30',
  productVersionId: '21',
  requestedPrincipalKrw: 10_000_000,
  createdGameDay: 120,
  expiresGameDay: 120,
  decisionCode: 'eligible',
  decisionReasons: ['eligible'],
  verifiedAnnualIncomeKrw: 50_000_000,
  verifiedIncomeSource: 'activeEmploymentContract',
  existingLoanBalanceKrw: 1_000_000,
  postExecutionBalanceKrw: 11_000_000,
  dsrApplied: true,
  dsr: {
    numeratorKrw: 20_000_000,
    denominatorKrw: 50_000_000,
    ratioPpm: 400_000,
    limitPpm: 500_000,
  },
  stressRateBp: 300,
  quotedTerms: {
    annualRateBp: 655,
    repaymentMethod: 'levelPayment',
    termMonths: 60,
    firstInstallment: {
      dueGameDay: 151,
      feeKrw: 0,
      principalKrw: 143_941,
      interestKrw: 55_630,
      totalKrw: 199_571,
    },
  },
});

const givenLoanExecutionResult = () => ({
  loanId: '40',
  quoteId: '30',
  productVersionId: '21',
  principalKrw: 10_000_000,
  activatedGameDay: 120,
  maturityGameDay: 1_946,
  annualRateBp: 655,
  repaymentMethod: 'levelPayment',
  termMonths: 60,
  firstInstallment: {
    dueGameDay: 151,
    feeKrw: 0,
    principalKrw: 143_941,
    interestKrw: 55_630,
    totalKrw: 199_571,
  },
});

const givenLoanPrepaymentResult = () => ({
  loanId: '40',
  paymentId: '50',
  principalKrw: 1_000_000,
  feeKrw: 10_000,
  totalDebitedKrw: 1_010_000,
  appliedGameDay: 120,
  remainingPrincipalKrw: 9_000_000,
  status: 'active',
  prepaymentEffect: 'recalculatePayment',
  remainingInstallments: 60,
  nextInstallment: {
    installmentNo: 1,
    dueGameDay: 151,
    feeKrw: 0,
    principalKrw: 129_546,
    interestKrw: 50_067,
    totalKrw: 179_613,
  },
  finalInstallmentDueGameDay: 1_946,
});

const givenLoanDetail = () => ({
  id: '40',
  leaseContractId: null,
  propertyHoldingId: null,
  productVersionId: '21',
  productKind: 'unsecuredLoan',
  displayName: '개발 변동금리 신용대출',
  rateStatus: 'available',
  currentAnnualRateBp: 655,
  status: 'active',
  readOnly: false,
  originalPrincipalKrw: 10_000_000,
  remainingPrincipalKrw: 9_000_000,
  accruedInterestKrw: 0,
  accruedFeeKrw: 0,
  overdueKrw: 0,
  repaymentMethod: 'levelPayment',
  termMonths: 60,
  totalInstallments: 60,
  activatedGameDay: 120,
  maturityGameDay: 1_946,
  finalInstallmentDueGameDay: 1_946,
  nextInstallmentNo: 1,
  oldestUnpaidDueGameDay: null,
  prepaymentAllowed: true,
  prepaymentFeePpm: 10_000,
  prepaymentEffect: 'recalculatePayment',
  dsrIncluded: true,
});

const givenLoanInstallment = (installmentNo: number, id: string) => ({
  id,
  installmentNo,
  dueGameDay: 151,
  interestPeriodStartGameDay: 121,
  elapsedDays: 31,
  annualRateBp: 655,
  openingPrincipalKrw: 9_000_000,
  scheduledFeeKrw: 0,
  scheduledInterestKrw: 50_067,
  scheduledPrincipalKrw: 129_546,
  paidFeeKrw: 0,
  paidInterestKrw: 0,
  paidPrincipalKrw: 0,
  remainingDueKrw: 179_613,
  status: 'pending',
  scheduleRevision: 2,
});

const givenLoanPayment = (paymentNo: number, id: string) => ({
  id,
  paymentNo,
  kind: 'manualPrepayment',
  gameDay: 120,
  amountKrw: 1_010_000,
  allocations: [
    { kind: 'prepaymentFee', amountKrw: 10_000 },
    { kind: 'prepaymentPrincipal', amountKrw: 1_000_000 },
  ],
});

const givenLoanInstallmentHistory = () => ({
  loanId: '40',
  installments: [givenLoanInstallment(60, '160'), givenLoanInstallment(59, '159')],
  payments: [givenLoanPayment(2, '52'), givenLoanPayment(1, '51')],
  hasMoreInstallments: true,
  hasMorePayments: false,
  nextBefore: 'v1.l40.i59.p0',
});

const givenHousingListingsResponse = () => ({
  rateStatus: 'active',
  modelVersionId: '31',
  gameDay: 120,
  yearMonth: { year: 2026, month: 5 },
  residenceRegionKey: 'capitalArea',
  selectedRegionKey: 'metropolitan',
  regions: [
    { regionKey: 'capitalArea', displayName: '수도권' },
    { regionKey: 'metropolitan', displayName: '광역시' },
    { regionKey: 'smallCity', displayName: '중소도시' },
    { regionKey: 'rural', displayName: '농촌' },
  ],
  priceIndexPpm: 1_021_000,
  rentIndexPpm: 1_011_000,
  listings: [
    {
      id: '7001',
      regionKey: 'metropolitan',
      propertyType: 'apartment',
      exclusiveAreaSquareMeters: 84,
      availableFromGameDay: 100,
      availableToGameDay: 130,
      offers: [
        { kind: 'sale', priceKrw: 420_000_000 },
        { kind: 'jeonse', depositKrw: 252_000_000 },
        { kind: 'monthlyRent', depositKrw: 42_000_000, monthlyRentKrw: 1_100_000 },
      ],
    },
  ],
});

const givenCharacterStartV2 = () => ({
  commandId: '00000000-0000-0000-0000-000000000001',
  expectedRunRevision: 0,
  expectedStateRevision: 0,
  expectedGameDay: 0,
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
});

describe('캐릭터 시작 v2 요청 계약', () => {
  describe('맥락: 상품 ID가 포함된 시작 대출인 경우', () => {
    it('given canonical 두 종류 대출, when 검증하면, then strict v2 요청을 허용한다', () => {
      const request = givenCharacterStartV2();

      const result = CharacterStartRequestSchema.safeParse(request);

      expect(result.success).toBe(true);
    });
  });

  describe('맥락: 시작 대출이 없는 경우', () => {
    it('given 빈 startingLoans, when 검증하면, then v2 요청을 허용한다', () => {
      const request = { ...givenCharacterStartV2(), startingLoans: [] };

      const result = CharacterStartRequestSchema.safeParse(request);

      expect(result.success).toBe(true);
    });
  });

  describe('맥락: v1 금액과 v2 목록을 섞은 경우', () => {
    it('given character 내부 legacy 금액, when 검증하면, then mixed shape를 거절한다', () => {
      const request = givenCharacterStartV2();

      const result = CharacterStartRequestSchema.safeParse({
        ...request,
        character: { ...request.character, studentLoanKrw: 12_000_000 },
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 시작 대출 순서가 바뀐 경우', () => {
    it('given 신용대출 다음 학자금, when 검증하면, then canonical order 위반을 거절한다', () => {
      const request = givenCharacterStartV2();

      const result = CharacterStartRequestSchema.safeParse({
        ...request,
        startingLoans: [...request.startingLoans].reverse(),
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 시작 원금이 0원인 경우', () => {
    it('given 0원 학자금, when 검증하면, then 항목 자체를 거절한다', () => {
      const request = givenCharacterStartV2();
      const first = request.startingLoans[0];

      const result = CharacterStartRequestSchema.safeParse({
        ...request,
        startingLoans: [{ ...first, principalKrw: 0 }],
      });

      expect(result.success).toBe(false);
    });
  });
});

describe('시장 히스토리 응답 계약', () => {
  describe('맥락: 포인트가 게임일 오름차순인 경우', () => {
    it('given 현재 게임일까지의 포인트, when 검증하면, then 응답을 허용한다', () => {
      const history = givenHistory();

      const result = MarketHistorySchema.safeParse(history);

      expect(result.success).toBe(true);
    });
  });

  describe('맥락: 금리 팩터가 있는 v3 포인트인 경우', () => {
    it('given 완전한 정책금리와 기간구조, when 검증하면, then 응답을 허용한다', () => {
      const history = givenHistory();
      const first = history.points[0];
      if (first === undefined) throw new Error('테스트 히스토리에는 첫 포인트가 있어야 한다');
      first.rates = {
        policyRateBp: 250,
        treasury3mBp: 255,
        treasury1yBp: 265,
        treasury3yBp: 280,
        treasury10yBp: 310,
      };

      const result = MarketHistorySchema.safeParse(history);

      expect(result.success).toBe(true);
    });
  });

  describe('맥락: 포인트 순서가 뒤집힌 경우', () => {
    it('given 내림차순 포인트, when 검증하면, then 응답을 거절한다', () => {
      const history = givenHistory();
      history.points.reverse();

      const result = MarketHistorySchema.safeParse(history);

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 저장 커서보다 미래 포인트가 포함된 경우', () => {
    it('given 상한 뒤의 포인트, when 검증하면, then 응답을 거절한다', () => {
      const history = givenHistory();
      history.throughGameDay = 1;

      const result = MarketHistorySchema.safeParse(history);

      expect(result.success).toBe(false);
    });
  });
});

describe('포트폴리오 주문 요청 계약', () => {
  describe('맥락: 주문 ID가 canonical UUID가 아닌 경우', () => {
    it('given 대문자 UUID, when 검증하면, then 요청을 거절한다', () => {
      const request = {
        orderId: '4F521F4C-9DD8-4D20-8E1F-15CB13CBE0F2',
        expectedRunRevision: 3,
        expectedStateRevision: 42,
        expectedGameDay: 17,
        accountId: '1',
        side: 'buy',
        symbol: 'LLX',
        quantity: 10,
      };

      const result = PortfolioOrderRequestSchema.safeParse(request);

      expect(result.success).toBe(false);
    });
  });
});

describe('현금상품 카탈로그 계약', () => {
  describe('맥락: CMA와 정기예금의 금액 필드가 용도별로 분리된 경우', () => {
    it('given 이자 최소 잔액과 가입 금액 범위, when 검증하면, then 상품 목록을 허용한다', () => {
      const catalog = { products: [givenCmaProduct(), givenTermDepositProduct()] };

      const result = CashProductCatalogSchema.safeParse(catalog);

      expect(result.success).toBe(true);
    });
  });

  describe('맥락: CMA가 예금 가입 금액 필드를 함께 사용한 경우', () => {
    it('given 서로 다른 의미의 금액 필드 혼용, when 검증하면, then 상품 목록을 거절한다', () => {
      const product = {
        ...givenCmaProduct(),
        minimumContributionKrw: 10_000,
        maximumContributionKrw: 1_000_000,
      };

      const result = CashProductCatalogSchema.safeParse({ products: [product] });

      expect(result.success).toBe(false);
    });
  });
});

describe('현금계약 스냅샷 계약', () => {
  describe('맥락: 활성 계약의 예상 만기액이 원금과 이자 및 세금에 맞는 경우', () => {
    it('given 활성 정기예금 요약, when 검증하면, then 계약을 허용한다', () => {
      const contract = {
        contractId: '21',
        productVersionId: '12',
        settlementAccountId: '1',
        kind: 'termDeposit',
        status: 'active',
        annualRateBp: 330,
        currentPrincipalKrw: 1_000_000,
        installmentAmountKrw: null,
        paidInstallmentCount: 0,
        missedInstallmentCount: 0,
        openedGameDay: 1,
        maturityGameDay: 366,
        expectedGrossInterestKrw: 33_000,
        expectedIncomeTaxKrw: 4_620,
        expectedLocalIncomeTaxKrw: 462,
        expectedNetPayoutKrw: 1_027_918,
      };

      const result = CashContractSummarySchema.safeParse(contract);

      expect(result.success).toBe(true);
    });
  });

  describe('맥락: 종료 계약에 예상 만기액이 남은 경우', () => {
    it('given 종료 상태와 non-null 예상액, when 검증하면, then 계약을 거절한다', () => {
      const contract = {
        contractId: '21',
        productVersionId: '12',
        settlementAccountId: '1',
        kind: 'termDeposit',
        status: 'closedEarly',
        annualRateBp: 330,
        currentPrincipalKrw: 0,
        installmentAmountKrw: null,
        paidInstallmentCount: 0,
        missedInstallmentCount: 0,
        openedGameDay: 1,
        maturityGameDay: 366,
        expectedGrossInterestKrw: 0,
        expectedIncomeTaxKrw: null,
        expectedLocalIncomeTaxKrw: null,
        expectedNetPayoutKrw: null,
      };

      const result = CashContractSummarySchema.safeParse(contract);

      expect(result.success).toBe(false);
    });
  });
});

describe('금융소득 연도 tagged union', () => {
  describe('맥락: v4 현재 연도의 원천별 누계가 합계와 일치하는 경우', () => {
    it('given open 상태와 한 원천, when 검증하면, then null 평가 필드를 포함해 허용한다', () => {
      const year = givenOpenFinancialIncomeYear();

      const result = FinancialIncomeYearSchema.safeParse(year);

      expect(result.success).toBe(true);
    });
  });

  describe('맥락: tagged variant의 필드 일부가 빠진 경우', () => {
    it('given filedGameDay가 없는 open 객체, when 검증하면, then 부분 variant를 거절한다', () => {
      const year: Record<string, unknown> = { ...givenOpenFinancialIncomeYear() };
      delete year.filedGameDay;

      const result = FinancialIncomeYearSchema.safeParse(year);

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 연도 객체에 계약 밖 필드가 포함된 경우', () => {
    it('given legacyTotal 필드, when 검증하면, then strict 계약으로 거절한다', () => {
      const year = { ...givenOpenFinancialIncomeYear(), legacyTotal: 10_000 };

      const result = FinancialIncomeYearSchema.safeParse(year);

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 추가 납부와 환급이 동시에 양수인 경우', () => {
    it('given 서로 상쇄되는 두 금액, when 검증하면, then 배타성 위반으로 거절한다', () => {
      const year = {
        taxYear: 2025,
        status: 'filed',
        sources: [],
        grossFinancialIncomeKrw: 0,
        withheldIncomeTaxKrw: 0,
        withheldLocalIncomeTaxKrw: 0,
        comparisonAIncomeTaxKrw: 0,
        comparisonALocalIncomeTaxKrw: 0,
        comparisonBIncomeTaxKrw: 0,
        comparisonBLocalIncomeTaxKrw: 0,
        assessedIncomeTaxKrw: 0,
        assessedLocalIncomeTaxKrw: 0,
        additionalTaxKrw: 100,
        refundKrw: 100,
        filingDueDate: '2026-05-31',
        filedGameDay: 500,
      };

      const result = FinancialIncomeYearSchema.safeParse(year);

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 원천별 누계와 연도 합계가 다른 경우', () => {
    it('given 1원 작은 gross 합계, when 검증하면, then 교차 합계 위반으로 거절한다', () => {
      const year = { ...givenOpenFinancialIncomeYear(), grossFinancialIncomeKrw: 9_999 };

      const result = FinancialIncomeYearSchema.safeParse(year);

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 원천별 행을 도입하기 전 레거시 런에 누계가 있는 경우', () => {
    it('given notApplicable 상태와 레거시 합계, when 검증하면, then 비소급 응답을 허용한다', () => {
      const year = {
        ...givenLegacyFinancialIncomeYear(),
        grossFinancialIncomeKrw: 10_000,
        withheldIncomeTaxKrw: 1_400,
        withheldLocalIncomeTaxKrw: 140,
      };

      const result = FinancialIncomeYearSchema.safeParse(year);

      expect(result.success).toBe(true);
    });
  });
});

describe('금융 스냅샷 배열 상한', () => {
  describe('맥락: 현재 런의 계좌가 32개를 넘은 경우', () => {
    it('given 33개 금융계좌, when 검증하면, then 스냅샷을 거절한다', () => {
      const accounts = Array.from({ length: 33 }, (_, index) => ({
        id: String(index + 1),
        type: 'taxableBrokerage',
        status: 'open',
        cashKrw: 0,
        isDefault: index === 0,
      }));
      const snapshot = {
        policySet: { key: 'kr-individual-2026-v1', basisDate: '2026-01-01' },
        accounts,
        cmaAccounts: [],
        cashContracts: [],
        depositProtection: [],
        currentTaxYear: givenLegacyFinancialIncomeYear(),
        isaAccounts: [],
        pensionAccounts: [],
        productBundle: null,
        llxDistributionEntitlements: [],
        bondPositions: [],
        goldAccounts: [],
        physicalGoldHoldings: [],
        latestFinancialIncomeAssessment: null,
        pendingSettlements: [],
      };

      const result = FinanceSnapshotSchema.safeParse(snapshot);

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: ISA와 연금 계좌가 bounded 상한을 넘은 경우', () => {
    it('given ISA 2개와 연금 3개, when 검증하면, then 스냅샷을 거절한다', () => {
      const isa = {
        accountId: '2',
        type: 'isaGeneral',
        openedGameDay: 1,
        minimumTermGameDay: 1096,
        totalContributionKrw: 0,
        principalWithdrawalKrw: 0,
        contributionCapacityKrw: 20_000_000,
        taxProfitKrw: 0,
        deductibleLossKrw: 0,
        expectedCloseIncomeTaxKrw: 0,
        expectedCloseLocalIncomeTaxKrw: 0,
      } as const;
      const pension = {
        accountId: '3',
        type: 'pensionSavings',
        openedGameDay: 1,
        eligiblePensionStartGameDay: 1827,
        pensionStarted: false,
        taxLayers: {
          taxExcludedContributionKrw: 0,
          deferredRetirementIncomeKrw: 0,
          creditedContributionKrw: 0,
          earningsKrw: 0,
        },
        currentYearContributionKrw: 0,
        currentYearCreditEligibleKrw: 0,
        expectedCreditKrw: 0,
        currentYearPensionLimitKrw: null,
        currentYearPensionWithdrawnKrw: 0,
        riskAssetValueKrw: 0,
        totalValueKrw: 0,
        riskAssetRatioPpm: 0,
      } as const;
      const snapshot = {
        policySet: { key: 'kr-individual-2026-v1', basisDate: '2026-01-01' },
        accounts: [],
        cmaAccounts: [],
        cashContracts: [],
        depositProtection: [],
        currentTaxYear: givenLegacyFinancialIncomeYear(),
        isaAccounts: [isa, { ...isa, accountId: '4' }],
        pensionAccounts: [
          pension,
          { ...pension, accountId: '5', type: 'irp' },
          { ...pension, accountId: '6' },
        ],
        productBundle: null,
        llxDistributionEntitlements: [],
        bondPositions: [],
        goldAccounts: [],
        physicalGoldHoldings: [],
        latestFinancialIncomeAssessment: null,
        pendingSettlements: [],
      };

      const result = FinanceSnapshotSchema.safeParse(snapshot);

      expect(result.success).toBe(false);
    });
  });
});

describe('M2-D 금융 스냅샷 상한', () => {
  describe('맥락: pending LLX 분배 권리가 8개를 넘은 경우', () => {
    it('given 9개 pending 권리, when 검증하면, then bounded 스냅샷을 거절한다', () => {
      const entitlements = Array.from({ length: 9 }, (_, index) => ({
        id: String(index + 10),
        accountId: '1',
        recordDate: '2026-03-31',
        paymentDate: '2026-04-15',
        quantity: 1,
        grossAmountKrw: 100,
        status: 'pending',
      }));
      const snapshot = {
        policySet: { key: 'kr-individual-2026-v1', basisDate: '2026-01-01' },
        accounts: [
          {
            id: '1',
            type: 'taxableBrokerage',
            status: 'open',
            cashKrw: 0,
            isDefault: true,
          },
        ],
        cmaAccounts: [],
        cashContracts: [],
        depositProtection: [],
        currentTaxYear: givenOpenFinancialIncomeYear(),
        isaAccounts: [],
        pensionAccounts: [],
        productBundle: {
          indexProduct: {
            id: '2',
            key: 'llx-2026-v1',
            displayName: '라이프 한국 종합지수',
            annualManagementFeePpm: 1_000,
            annualDistributionRatePpm: 20_000,
            dayCountDenominator: 365,
            buyFeePpm: 100,
            sellFeePpm: 100,
            sellTaxPpm: 2_000,
          },
          bondProductVersionIds: ['3', '4'],
          goldProductVersionId: '5',
        },
        llxDistributionEntitlements: entitlements,
        bondPositions: [],
        goldAccounts: [],
        physicalGoldHoldings: [],
        latestFinancialIncomeAssessment: null,
        pendingSettlements: [],
      };

      const result = FinanceSnapshotSchema.safeParse(snapshot);

      expect(result.success).toBe(false);
    });
  });
});

describe('절세계좌 스냅샷 계약', () => {
  describe('맥락: ISA 의무기간이 가입일과 같은 경우', () => {
    it('given 0일 의무기간, when 검증하면, then ISA 요약을 거절한다', () => {
      const account = {
        accountId: '2',
        type: 'isaGeneral',
        openedGameDay: 10,
        minimumTermGameDay: 10,
        totalContributionKrw: 0,
        principalWithdrawalKrw: 0,
        contributionCapacityKrw: 20_000_000,
        taxProfitKrw: 0,
        deductibleLossKrw: 0,
        expectedCloseIncomeTaxKrw: 0,
        expectedCloseLocalIncomeTaxKrw: 0,
      };

      const result = IsaAccountSummarySchema.safeParse(account);

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: ISA 요약에 계약에 없는 필드가 포함된 경우', () => {
    it('given unknown 필드가 있는 요약, when 검증하면, then strict 계약으로 거절한다', () => {
      const account = {
        accountId: '2',
        type: 'isaGeneral',
        openedGameDay: 10,
        minimumTermGameDay: 1105,
        totalContributionKrw: 0,
        principalWithdrawalKrw: 0,
        contributionCapacityKrw: 20_000_000,
        taxProfitKrw: 0,
        deductibleLossKrw: 0,
        expectedCloseIncomeTaxKrw: 0,
        expectedCloseLocalIncomeTaxKrw: 0,
        clientComputedTaxKrw: 0,
      };

      const result = IsaAccountSummarySchema.safeParse(account);

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 연금 공제대상액이 해당 연도 납입액을 넘는 경우', () => {
    it('given 납입 100만원과 공제대상 100만1원, when 검증하면, then 연금 요약을 거절한다', () => {
      const account = {
        accountId: '3',
        type: 'pensionSavings',
        openedGameDay: 1,
        eligiblePensionStartGameDay: 1827,
        pensionStarted: false,
        taxLayers: {
          taxExcludedContributionKrw: 1_000_000,
          deferredRetirementIncomeKrw: 0,
          creditedContributionKrw: 0,
          earningsKrw: 0,
        },
        currentYearContributionKrw: 1_000_000,
        currentYearCreditEligibleKrw: 1_000_001,
        expectedCreditKrw: 165_000,
        currentYearPensionLimitKrw: null,
        currentYearPensionWithdrawnKrw: 0,
        riskAssetValueKrw: 0,
        totalValueKrw: 1_000_000,
        riskAssetRatioPpm: 0,
      };

      const result = PensionAccountSummarySchema.safeParse(account);

      expect(result.success).toBe(false);
    });
  });
});

describe('연금 인출 요청·응답 계약', () => {
  describe('맥락: 사유가 명시적인 null인 경우', () => {
    it('given 일반 연금외 인출과 null 사유, when 검증하면, then 요청을 허용한다', () => {
      const request = {
        commandId: '4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2',
        expectedRunRevision: 3,
        expectedStateRevision: 42,
        expectedGameDay: 17,
        amountKrw: 100_000,
        type: 'nonPension',
        reason: null,
      };

      const result = PensionWithdrawalRequestSchema.safeParse(request);

      expect(result.success).toBe(true);
    });
  });

  describe('맥락: 필수 nullable 사유 필드가 빠진 경우', () => {
    it('given reason 없는 인출, when 검증하면, then 요청을 거절한다', () => {
      const request = {
        commandId: '4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2',
        expectedRunRevision: 3,
        expectedStateRevision: 42,
        expectedGameDay: 17,
        amountKrw: 100_000,
        type: 'nonPension',
      };

      const result = PensionWithdrawalRequestSchema.safeParse(request);

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: gross 분할과 세후 지급액이 일치하지 않는 경우', () => {
    it('given 누락된 연금외 부분, when 검증하면, then 인출 영수증을 거절한다', () => {
      const receipt = {
        commandId: '4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2',
        accountId: '3',
        grossAmountKrw: 100_000,
        pensionAmountKrw: 60_000,
        nonPensionAmountKrw: 30_000,
        taxFreeAmountKrw: 10_000,
        taxKrw: 5_000,
        netPayoutKrw: 95_000,
        replayed: false,
      };

      const result = PensionWithdrawalResultSchema.safeParse(receipt);

      expect(result.success).toBe(false);
    });
  });
});

describe('LLX 체결 응답 계약', () => {
  describe('맥락: 매도에서 수수료와 제거 원가가 함께 기록된 경우', () => {
    it('given 손실 체결, when 검증하면, then signed 실현손익과 비용을 보존한다', () => {
      const execution = {
        orderId: '4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2',
        accountId: '3',
        symbol: 'LLX',
        side: 'sell',
        quantity: 10,
        priceKrw: 90_000,
        grossAmountKrw: 900_000,
        feeKrw: 1_000,
        taxKrw: 0,
        removedCostBasisKrw: 1_000_000,
        realizedGainLossKrw: -101_000,
        replayed: false,
      };

      const result = PortfolioExecutionSchema.safeParse(execution);

      expect(result.success).toBe(true);
    });
  });
});

describe('국채 주문 프로토콜', () => {
  describe('맥락: 요청에 서버가 정의하지 않은 필드가 있는 경우', () => {
    it('given quotePrice 필드, when 검증하면, then strict 주문을 거절한다', () => {
      const request = {
        commandId: '4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2',
        expectedRunRevision: 3,
        expectedStateRevision: 42,
        expectedGameDay: 17,
        accountId: '1',
        seriesId: '21',
        side: 'buy',
        bondUnits: 10,
        quotePrice: 99_000,
      };

      const result = BondOrderRequestSchema.safeParse(request);

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 매도 영수증의 실현손익이 원가와 비용에 맞지 않는 경우', () => {
    it('given 1원 큰 실현손익, when 검증하면, then 영수증을 거절한다', () => {
      const order = {
        commandId: '4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2',
        executionId: 'c3a9aa29-fdfa-41b0-a054-332415c1976a',
        accountId: '1',
        seriesId: '21',
        side: 'sell',
        bondUnits: 10,
        dirtyPriceKrw: 99_000,
        grossAmountKrw: 990_000,
        feeKrw: 1_000,
        taxKrw: 0,
        removedCostBasisKrw: 900_000,
        realizedGainLossKrw: 89_001,
        replayed: false,
      };

      const result = BondOrderResultSchema.safeParse(order);

      expect(result.success).toBe(false);
    });
  });
});

describe('금 상품과 실물 인출 프로토콜', () => {
  describe('맥락: 출고 bar 규격 두 항목이 같은 경우', () => {
    it('given 100g 규격 중복, when 검증하면, then 상품 카탈로그를 거절한다', () => {
      const catalog = {
        marketVersion: 'm1-2026-v4',
        products: [
          {
            id: '5',
            key: 'krx-gold-2026-v1',
            displayName: 'KRX 금시장',
            unit: 'gram',
            buyFeePpm: 1_000,
            sellFeePpm: 1_000,
            buyTaxPpm: 0,
            sellTaxPpm: 0,
            withdrawalBars: [
              { barSizeGram: 100, feeKrw: 10_000 },
              { barSizeGram: 100, feeKrw: 20_000 },
            ],
          },
        ],
      };

      const result = GoldProductCatalogSchema.safeParse(catalog);

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 실물 인출 현금 청구액이 VAT와 수수료 합과 다른 경우', () => {
    it('given 1원 작은 청구액, when 검증하면, then 인출 영수증을 거절한다', () => {
      const withdrawal = {
        commandId: '4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2',
        withdrawalId: 'c3a9aa29-fdfa-41b0-a054-332415c1976a',
        accountId: '6',
        barSizeGram: 100,
        barCount: 2,
        quantityGram: 200,
        removedCostBasisKrw: 20_000_000,
        vatKrw: 2_000_000,
        feeKrw: 10_000,
        cashChargedKrw: 2_009_999,
        replayed: false,
      };

      const result = GoldWithdrawalResultSchema.safeParse(withdrawal);

      expect(result.success).toBe(false);
    });
  });
});

describe('M2-D 공통 enum 계약', () => {
  describe('맥락: 금융소득 신고 settlement 이름이 변경된 경우', () => {
    it('given 새 이름과 과거 이름, when 검증하면, then 새 이름만 허용한다', () => {
      const results = ['financialIncomeFiling', 'taxFiling'].map(
        (kind) => SettlementKindSchema.safeParse(kind).success,
      );

      expect(results).toEqual([true, false]);
    });
  });

  describe('맥락: 자산 주문 공통 실패가 발생한 경우', () => {
    it('given 신규 실패 코드 세 종류, when 검증하면, then 모두 금융 실패로 허용한다', () => {
      const results = ['marketClosed', 'insufficientQuantity', 'positionLimit'].map(
        (code) => FinanceFailureCodeSchema.safeParse(code).success,
      );

      expect(results).toEqual([true, true, true]);
    });
  });
});

describe('복식 원장 응답 계약', () => {
  describe('맥락: 현금상품 가입과 해지가 원장 출처인 경우', () => {
    it('given 신규 source kind 두 종류, when 검증하면, then 원장 출처로 허용한다', () => {
      const sources = ['cashProductEnrollment', 'cashProductClose'];

      const results = sources.map((source) => LedgerSourceKindSchema.safeParse(source).success);

      expect(results).toEqual([true, true]);
    });
  });

  describe('맥락: ISA 해지와 연금 인출이 원장 출처인 경우', () => {
    it('given M2-C source kind 두 종류, when 검증하면, then 원장 출처로 허용한다', () => {
      const sources = ['isaClose', 'pensionWithdrawal'];

      const results = sources.map((source) => LedgerSourceKindSchema.safeParse(source).success);

      expect(results).toEqual([true, true]);
    });
  });

  describe('맥락: 급여와 채용보상이 별도 원장 출처인 경우', () => {
    it('given M3-C 원장 출처와 계정 코드, when 검증하면, then 모두 허용한다', () => {
      const sources = ['employmentPayroll', 'careerRewardPayment', 'pensionCreditAllocation'];
      const accounts = [
        'salaryIncome',
        'employeeNationalPensionExpense',
        'employeeHealthInsuranceExpense',
        'employeeLongTermCareExpense',
        'employeeEmploymentInsuranceExpense',
        'employmentIncomeTaxWithholding',
        'employmentLocalIncomeTaxWithholding',
        'otherIncomeReward',
        'otherIncomeTaxWithholding',
        'otherLocalIncomeTaxWithholding',
      ];

      const sourceResults = sources.map(
        (source) => LedgerSourceKindSchema.safeParse(source).success,
      );
      const accountResults = accounts.map(
        (account) => LedgerAccountCodeSchema.safeParse(account).success,
      );

      expect(sourceResults.every(Boolean) && accountResults.every(Boolean)).toBe(true);
    });
  });

  describe('맥락: 군 급여와 장병적금 이동을 원장에서 분리하는 경우', () => {
    it('given 군 관련 source kind 다섯 종류, when 검증하면, then 모두 원장 출처로 허용한다', () => {
      const sources = [
        'militaryPay',
        'militarySavingsInstallment',
        'militarySavingsMaturity',
        'militarySavingsGovernmentMatch',
        'militarySavingsEarlyClose',
      ];

      const results = sources.map((source) => LedgerSourceKindSchema.safeParse(source).success);

      expect(results).toEqual([true, true, true, true, true]);
    });

    it('given 원금·은행이자·정부지원 계정 코드, when 검증하면, then 각 금액을 별도 계정으로 허용한다', () => {
      const accounts = [
        'militaryPayIncome',
        'militarySavingsPrincipal',
        'militarySavingsBankInterest',
        'militarySavingsGovernmentMatchIncome',
      ];

      const results = accounts.map((account) => LedgerAccountCodeSchema.safeParse(account).success);

      expect(results).toEqual([true, true, true, true]);
    });
  });

  describe('맥락: 같은 종류의 절세계좌가 이미 있는 경우', () => {
    it('given accountAlreadyExists, when 검증하면, then 금융 실패 코드로 허용한다', () => {
      const result = FinanceFailureCodeSchema.safeParse('accountAlreadyExists');

      expect(result.success).toBe(true);
    });
  });

  describe('맥락: 거래와 분개가 최신순으로 균형을 이룬 경우', () => {
    it('given 계좌 참조와 다음 커서가 맞는 페이지, when 검증하면, then 응답을 허용한다', () => {
      const page = givenLedgerPage();

      const result = LedgerPageSchema.safeParse(page);

      expect(result.success).toBe(true);
    });
  });

  describe('맥락: 한 거래의 분개 합계가 0이 아닌 경우', () => {
    it('given 불균형 posting, when 검증하면, then 응답을 거절한다', () => {
      const page = givenLedgerPage();
      const posting = page.transactions[0]?.postings[1];
      if (posting === undefined) throw new Error('테스트 분개가 있어야 한다');
      posting.amountKrw = 99_999;

      const result = LedgerPageSchema.safeParse(page);

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 계좌 원장 코드에 계좌 ID가 없는 경우', () => {
    it('given accountCash와 null 계좌, when 검증하면, then 응답을 거절한다', () => {
      const page = givenLedgerPage();
      const posting = page.transactions[0]?.postings[1];
      if (posting === undefined) throw new Error('테스트 분개가 있어야 한다');
      posting.accountId = null;

      const result = LedgerPageSchema.safeParse(page);

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 거래 ID가 최신순이 아니거나 다음 커서가 다른 경우', () => {
    it('given 뒤집힌 거래와 잘못된 nextBefore, when 검증하면, then 응답을 거절한다', () => {
      const page = givenLedgerPage();
      page.transactions.reverse();
      page.nextBefore = '3';

      const result = LedgerPageSchema.safeParse(page);

      expect(result.success).toBe(false);
    });
  });
});

describe('커리어 M3-A protocol 계약', () => {
  describe('맥락: 산출물 종류와 전용 필드가 모순되는 경우', () => {
    it('given resume와 openToWork, when 검증하면, then strict tagged union으로 거절한다', () => {
      const result = CareerArtifactDraftSchema.safeParse({
        kind: 'resume',
        headline: '개발자',
        summary: '',
        evidenceIds: [],
        openToWork: true,
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: headline Unicode scalar 경계인 경우', () => {
    it('given 120개 emoji, when 검증하면, then UTF-16 길이와 무관하게 허용한다', () => {
      const result = CareerArtifactDraftSchema.safeParse({
        kind: 'portfolio',
        headline: '😀'.repeat(120),
        summary: '',
        evidenceIds: [],
      });

      expect(result.success).toBe(true);
    });

    it('given 121개 Unicode scalar, when 검증하면, then 상한으로 거절한다', () => {
      const result = CareerArtifactDraftSchema.safeParse({
        kind: 'portfolio',
        headline: '가'.repeat(121),
        summary: '',
        evidenceIds: [],
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: summary control 문자 경계인 경우', () => {
    it('given LF와 tab, when 검증하면, then plain text 예외로 허용한다', () => {
      const result = CareerArtifactDraftSchema.safeParse({
        kind: 'resume',
        headline: '이력서',
        summary: '첫 줄\n\t둘째 줄',
        evidenceIds: [],
      });

      expect(result.success).toBe(true);
    });

    it('given NUL, when 검증하면, then control 문자로 거절한다', () => {
      const result = CareerArtifactDraftSchema.safeParse({
        kind: 'resume',
        headline: '이력서',
        summary: '금지\u0000문자',
        evidenceIds: [],
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 산출물 참조 배열에 중복이 있는 경우', () => {
    it('given 같은 evidence ID 두 번, when 명령을 검증하면, then 거절한다', () => {
      const result = CareerArtifactPublishRequestSchema.safeParse({
        commandId: '00000000-0000-0000-0000-000000000001',
        expectedRunRevision: 1,
        expectedStateRevision: 2,
        expectedGameDay: 3,
        kind: 'linkedinProfile',
        headline: '프로필',
        summary: '',
        evidenceIds: ['1', '1'],
        openToWork: true,
        industries: ['itSoftware'],
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 산출물 종류별 evidence 상한을 검증하는 경우', () => {
    it('given portfolio evidence 13개, when 초안과 게시 요청을 검증하면, then 모두 거절한다', () => {
      const evidenceIds = Array.from({ length: 13 }, (_, index) => String(index + 1));
      const draft = CareerArtifactDraftSchema.safeParse({
        kind: 'portfolio',
        headline: '포트폴리오',
        summary: '',
        evidenceIds,
      });
      const request = CareerArtifactPublishRequestSchema.safeParse({
        commandId: '00000000-0000-0000-0000-000000000001',
        expectedRunRevision: 1,
        expectedStateRevision: 2,
        expectedGameDay: 3,
        kind: 'portfolio',
        headline: '포트폴리오',
        summary: '',
        evidenceIds,
      });

      expect([draft.success, request.success]).toEqual([false, false]);
    });

    it('given LinkedIn evidence 31개, when 게시 요청과 응답을 검증하면, then 모두 거절한다', () => {
      const evidenceIds = Array.from({ length: 31 }, (_, index) => String(index + 1));
      const request = CareerArtifactPublishRequestSchema.safeParse({
        commandId: '00000000-0000-0000-0000-000000000001',
        expectedRunRevision: 1,
        expectedStateRevision: 2,
        expectedGameDay: 3,
        kind: 'linkedinProfile',
        headline: '프로필',
        summary: '',
        evidenceIds,
        openToWork: true,
        industries: [],
      });
      const response = CareerArtifactsResponseSchema.safeParse({
        items: [
          {
            id: '1',
            kind: 'linkedinProfile',
            versionNo: 1,
            headline: '프로필',
            summary: '',
            evidenceIds,
            openToWork: true,
            industries: [],
            completenessBp: 0,
            createdGameDay: 0,
          },
        ],
        nextBefore: null,
      });

      expect([request.success, response.success]).toEqual([false, false]);
    });

    it('given resume evidence 40개, when 초안과 게시 요청을 검증하면, then 모두 허용한다', () => {
      const evidenceIds = Array.from({ length: 40 }, (_, index) => String(index + 1));
      const draft = CareerArtifactDraftSchema.safeParse({
        kind: 'resume',
        headline: '이력서',
        summary: '',
        evidenceIds,
      });
      const request = CareerArtifactPublishRequestSchema.safeParse({
        commandId: '00000000-0000-0000-0000-000000000001',
        expectedRunRevision: 1,
        expectedStateRevision: 2,
        expectedGameDay: 3,
        kind: 'resume',
        headline: '이력서',
        summary: '',
        evidenceIds,
      });

      expect([draft.success, request.success]).toEqual([true, true]);
    });
  });

  describe('맥락: bounded snapshot 활동 순서가 모순되는 경우', () => {
    it('given 중복 priority, when 검증하면, then snapshot을 거절한다', () => {
      const activity = {
        id: '1',
        catalogEntryId: '1',
        activityKey: 'project-basic',
        displayName: '기초 프로젝트',
        status: 'active' as const,
        priority: 1,
        startedGameDay: 0,
        accumulatedEffortUnits: 1,
        requiredEffortUnits: 10,
        elapsedCalendarDays: 1,
        minimumCalendarDays: 1,
        dailyEffortCapUnits: 5,
        completedGameDay: null,
      };
      const result = CareerSnapshotSchema.safeParse({
        focusedJobFamilyKey: 'softwareEngineering',
        possessedScores: {
          education: 0,
          certification: 0,
          language: 0,
          training: 0,
          experience: 0,
          project: 0,
        },
        activeActivities: [activity, { ...activity, id: '2' }],
        latestArtifacts: [],
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: career page cursor와 정렬이 일치하는 경우', () => {
    it('given 내림차순 evidence와 마지막 ID cursor, when 검증하면, then 허용한다', () => {
      const evidence = (id: string) => ({
        id,
        evidenceKey: `bridge:${id}`,
        catalogEntryId: id,
        catalogEntryKey: `entry-${id}`,
        displayName: `증거 ${id}`,
        kind: 'certification' as const,
        acquiredGameDay: 0,
        expiresOnGameDay: null,
        periodStartDate: null,
        periodEndExclusiveDate: null,
        creditedExperienceDays: null,
      });
      const result = CareerSpecsResponseSchema.safeParse({
        focusedJobFamilyKey: 'softwareEngineering',
        possessedScores: {
          education: 0,
          certification: 0,
          language: 0,
          training: 0,
          experience: 0,
          project: 0,
        },
        items: [evidence('3'), evidence('2')],
        nextBefore: '2',
      });

      expect(result.success).toBe(true);
    });

    it('given 오래된 ID가 먼저인 artifact page, when 검증하면, then 거절한다', () => {
      const result = CareerArtifactsResponseSchema.safeParse({
        items: [
          {
            id: '1',
            kind: 'resume',
            versionNo: 1,
            headline: '첫 이력서',
            summary: '',
            evidenceIds: [],
            completenessBp: 0,
            createdGameDay: 0,
          },
          {
            id: '2',
            kind: 'resume',
            versionNo: 2,
            headline: '둘째 이력서',
            summary: '',
            evidenceIds: [],
            completenessBp: 0,
            createdGameDay: 1,
          },
        ],
        nextBefore: '2',
      });

      expect(result.success).toBe(false);
    });

    it('given 마지막 non-empty page와 null cursor, when 검증하면, then 허용한다', () => {
      const result = CareerArtifactsResponseSchema.safeParse({
        items: [
          {
            id: '1',
            kind: 'portfolio',
            versionNo: 1,
            headline: '마지막 포트폴리오',
            summary: '',
            evidenceIds: [],
            completenessBp: 0,
            createdGameDay: 0,
          },
        ],
        nextBefore: null,
      });

      expect(result.success).toBe(true);
    });

    it('given 인정 일수가 없는 경력 evidence, when 검증하면, then 거절한다', () => {
      const evidence = {
        id: '1',
        evidenceKey: 'employment:1',
        catalogEntryId: '1',
        catalogEntryKey: 'experience-software',
        displayName: '소프트웨어 경력',
        kind: 'experience',
        acquiredGameDay: 30,
        expiresOnGameDay: null,
        periodStartDate: '2026-01-01',
        periodEndExclusiveDate: '2026-02-01',
        creditedExperienceDays: null,
      };

      const result = CareerSpecsResponseSchema.safeParse({
        focusedJobFamilyKey: 'softwareEngineering',
        possessedScores: {
          education: 0,
          certification: 0,
          language: 0,
          training: 0,
          experience: 0,
          project: 0,
        },
        items: [evidence],
        nextBefore: null,
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 고정 실패 코드와 빈 활동 페이지인 경우', () => {
    it('given activityLimit과 빈 페이지, when 검증하면, then 계약을 허용한다', () => {
      const failure = CareerFailureCodeSchema.safeParse('activityLimit');
      const page = CareerActivitiesResponseSchema.safeParse({
        catalog: [],
        active: [],
        items: [],
        nextBefore: null,
      });

      expect(failure.success).toBe(true);
      expect(page.success).toBe(true);
    });
  });
});

describe('커리어 M3-B protocol 계약', () => {
  describe('맥락: 플랫폼별 공고 페이지가 deterministic posting key 순서인 경우', () => {
    it('given 요구 조건과 내 점수를 갖춘 공고, when 검증하면, then 허용한다', () => {
      const scores = {
        education: 0,
        certification: 0,
        language: 0,
        training: 0,
        experience: 0,
        project: 0,
      };
      const result = CareerJobsResponseSchema.safeParse({
        items: [
          {
            postingKey: 'f'.repeat(64),
            postedGameDay: 4,
            closesExclusiveGameDay: 18,
            platform: 'wanted',
            industry: 'itSoftware',
            jobFamilyKey: 'softwareEngineering',
            employerName: '라이프테크',
            region: 'capitalArea',
            employmentType: 'regular',
            requiredScores: scores,
            possessedScores: scores,
            minimumAnnualSalaryKrw: 36_000_000,
            maximumAnnualSalaryKrw: 48_000_000,
            salaryStepKrw: 1_000_000,
            competitionBand: 'high',
            militaryRequirement: 'any',
            minimumEducation: 'bachelor',
            requiredCertificationName: null,
            minimumExperienceDays: 0,
            requiredArtifacts: ['resume', 'portfolio'],
          },
        ],
        nextBefore: 'f'.repeat(64),
      });

      expect(result.success).toBe(true);
    });
  });

  describe('맥락: 직접 지원 artifact version이 없는 경우', () => {
    it('given posting key만 있는 명령, when 검증하면, then strict boundary에서 거절한다', () => {
      const result = CareerApplicationRequestSchema.safeParse({
        commandId: '00000000-0000-0000-0000-000000000001',
        expectedRunRevision: 1,
        expectedStateRevision: 2,
        expectedGameDay: 3,
        postingKey: 'a'.repeat(64),
      });

      expect(result.success).toBe(false);
    });
  });
});

function givenPayrollPage() {
  return {
    items: [
      {
        id: '9',
        contractId: '3',
        periodNo: 1,
        salaryMonthOrdinal: 1,
        periodStartDate: '2026-01-23',
        periodEndExclusiveDate: '2026-02-01',
        paidGameDay: 55,
        grossPayKrw: 1_125_000,
        employeeNationalPensionKrw: 0,
        employerNationalPensionKrw: 0,
        employeeHealthInsuranceKrw: 0,
        employerHealthInsuranceKrw: 0,
        employeeLongTermCareKrw: 0,
        employerLongTermCareKrw: 0,
        employeeEmploymentInsuranceKrw: 10_120,
        employerEmploymentInsuranceKrw: 12_930,
        employerIndustrialAccidentKrw: 7_870,
        withheldIncomeTaxKrw: 24_000,
        withheldLocalIncomeTaxKrw: 2_400,
        netPayKrw: 1_088_480,
        reward: {
          paymentId: '4',
          grossRewardKrw: 500_000,
          withheldIncomeTaxKrw: 100_000,
          withheldLocalIncomeTaxKrw: 10_000,
          netRewardKrw: 390_000,
        },
      },
    ],
    nextBefore: null,
  };
}

describe('커리어 M3-C 급여 protocol 계약', () => {
  describe('맥락: 근로자·사용자 부담과 플랫폼 보상이 분리된 경우', () => {
    it('given 균형이 맞는 급여 명세, when 검증하면, then 허용한다', () => {
      const response = givenPayrollPage();

      const result = CareerPayrollResponseSchema.safeParse(response);

      expect(result.success).toBe(true);
    });
  });

  describe('맥락: 급여 순입금이 공제 합계와 다른 경우', () => {
    it('given 틀린 순입금, when 검증하면, then 거절한다', () => {
      const response = givenPayrollPage();
      const payroll = response.items[0];
      if (payroll === undefined) throw new Error('테스트 급여 페이지에는 첫 항목이 있어야 한다');
      payroll.netPayKrw += 1;

      const result = CareerPayrollResponseSchema.safeParse(response);

      expect(result.success).toBe(false);
    });
  });
});

function givenOpenEmploymentTaxYear() {
  return {
    taxYear: 2026,
    status: 'open' as const,
    source: 'employmentOnly' as const,
    grossEmploymentIncomeKrw: 3_000_000,
    employeeInsuranceDeductionKrw: 291_520,
    earnedIncomeDeductionKrw: null,
    personalDeductionKrw: null,
    taxableIncomeKrw: null,
    calculatedIncomeTaxKrw: null,
    earnedIncomeTaxCreditKrw: null,
    pensionCreditEligibleContributionKrw: null,
    actualPensionIncomeTaxCreditKrw: null,
    actualPensionLocalIncomeTaxEffectKrw: null,
    withheldIncomeTaxKrw: 84_850,
    withheldLocalIncomeTaxKrw: 8_480,
    assessedIncomeTaxKrw: null,
    assessedLocalIncomeTaxKrw: null,
    additionalTaxKrw: null,
    refundKrw: null,
    reconciliationGameDay: null,
  };
}

function givenDefinitiveEmploymentTaxYear() {
  return {
    taxYear: 2026,
    status: 'definitive' as const,
    source: 'employmentOnly' as const,
    grossEmploymentIncomeKrw: 36_000_000,
    employeeInsuranceDeductionKrw: 3_000_000,
    earnedIncomeDeductionKrw: 10_000_000,
    personalDeductionKrw: 1_500_000,
    taxableIncomeKrw: 21_500_000,
    calculatedIncomeTaxKrw: 1_500_000,
    earnedIncomeTaxCreditKrw: 700_000,
    pensionCreditEligibleContributionKrw: 4_000_000,
    actualPensionIncomeTaxCreditKrw: 600_000,
    actualPensionLocalIncomeTaxEffectKrw: 60_000,
    withheldIncomeTaxKrw: 900_000,
    withheldLocalIncomeTaxKrw: 90_000,
    assessedIncomeTaxKrw: 200_000,
    assessedLocalIncomeTaxKrw: 20_000,
    additionalTaxKrw: 0,
    refundKrw: 770_000,
    reconciliationGameDay: 420,
  };
}

describe('커리어 M3-C 연말정산 protocol 계약', () => {
  describe('맥락: 아직 닫히지 않은 귀속연도인 경우', () => {
    it('given 누계와 모든 확정 필드 null, when 검증하면, then open 응답을 허용한다', () => {
      const response = givenOpenEmploymentTaxYear();

      const result = CareerTaxYearStateSchema.safeParse(response);

      expect(result.success).toBe(true);
    });

    it('given nullable 확정 필드가 누락됨, when 검증하면, then exact 응답을 거절한다', () => {
      const response: Record<string, unknown> = { ...givenOpenEmploymentTaxYear() };
      delete response.actualPensionIncomeTaxCreditKrw;

      const result = CareerTaxYearStateSchema.safeParse(response);

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 귀속연도 정산이 확정된 경우', () => {
    it('given 원천세보다 확정세액이 적은 정산, when 검증하면, then 환급 응답을 허용한다', () => {
      const response = givenDefinitiveEmploymentTaxYear();

      const result = CareerTaxYearStateSchema.safeParse(response);

      expect(result.success).toBe(true);
    });

    it('given 환급액이 세액 차이와 다름, when 검증하면, then 모순된 정산을 거절한다', () => {
      const response = { ...givenDefinitiveEmploymentTaxYear(), refundKrw: 769_999 };

      const result = CareerTaxYearStateSchema.safeParse(response);

      expect(result.success).toBe(false);
    });

    it('given 기존 시작 프로필의 보존 가능한 총급여와 과세표준, when 검증하면, then 나머지 null 상세를 허용한다', () => {
      const response = {
        taxYear: 2025,
        status: 'definitive',
        source: 'legacyProfile',
        grossEmploymentIncomeKrw: 42_000_000,
        employeeInsuranceDeductionKrw: null,
        earnedIncomeDeductionKrw: null,
        personalDeductionKrw: null,
        taxableIncomeKrw: 20_000_000,
        calculatedIncomeTaxKrw: null,
        earnedIncomeTaxCreditKrw: null,
        pensionCreditEligibleContributionKrw: null,
        actualPensionIncomeTaxCreditKrw: null,
        actualPensionLocalIncomeTaxEffectKrw: null,
        withheldIncomeTaxKrw: null,
        withheldLocalIncomeTaxKrw: null,
        assessedIncomeTaxKrw: null,
        assessedLocalIncomeTaxKrw: null,
        additionalTaxKrw: null,
        refundKrw: null,
        reconciliationGameDay: null,
      };

      const result = CareerTaxYearStateSchema.safeParse(response);

      expect(result.success).toBe(true);
    });
  });
});

describe('병역 M3-D protocol 계약', () => {
  describe('맥락: 군 급여와 장병적금 정산이 예약된 경우', () => {
    it('given M3-D settlement kind 네 종류, when 검증하면, then 모두 허용한다', () => {
      const kinds = [
        'militaryPay',
        'militarySavingsInstallment',
        'militarySavingsMaturity',
        'militarySavingsGovernmentMatch',
      ];

      const results = kinds.map((kind) => SettlementKindSchema.safeParse(kind).success);

      expect(results).toEqual([true, true, true, true]);
    });
  });

  describe('맥락: bounded snapshot에 현재 복무와 활성 적금이 함께 있는 경우', () => {
    it('given serving 상태와 active 요약, when 검증하면, then exact snapshot을 허용한다', () => {
      const snapshot = givenMilitaryCareerSnapshot();

      const result = CareerSnapshotSchema.safeParse(snapshot);

      expect(result.success).toBe(true);
    });

    it('given unserved 상태에 active 적금이 남음, when 검증하면, then 모순을 거절한다', () => {
      const snapshot = {
        ...givenMilitaryCareerSnapshot(),
        militaryStatus: 'unserved',
        activeMilitaryService: null,
      };

      const result = CareerSnapshotSchema.safeParse(snapshot);

      expect(result.success).toBe(false);
    });

    it('given 같은 날 settlement가 action보다 앞선 일정, when 검증하면, then 실행 순서 위반을 거절한다', () => {
      const base = givenMilitaryCareerSnapshot();
      const snapshot = {
        ...base,
        pendingCareerSchedule: [...base.pendingCareerSchedule].reverse(),
      };

      const result = CareerSnapshotSchema.safeParse(snapshot);

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 서버가 option 자격과 보수 구간을 완전하게 보낸 경우', () => {
    it('given 18개월을 빈틈없이 덮는 현역 보수표, when 검증하면, then option을 허용한다', () => {
      const response = { items: [givenMilitaryOption()] };

      const result = MilitaryOptionsResponseSchema.safeParse(response);

      expect(result.success).toBe(true);
    });

    it('given option에 계약 밖 필드가 추가됨, when 검증하면, then unknown field를 거절한다', () => {
      const option = { ...givenMilitaryOption(), clientEstimateKrw: 750_000 };

      const result = MilitaryOptionsResponseSchema.safeParse({ items: [option] });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 외부 병역 상태와 복무 이력이 함께 조회된 경우', () => {
    it('given serving 상태와 진행 중 service, when 검증하면, then 이력을 허용한다', () => {
      const response = givenMilitaryServiceResponse();

      const result = MilitaryServiceResponseSchema.safeParse(response);

      expect(result.success).toBe(true);
    });

    it('given M3-D 이전 군필 상태와 소급 복무 이력 없음, when 검증하면, then bridge 응답을 허용한다', () => {
      const response = { militaryStatus: 'completed', service: null };

      const result = MilitaryServiceResponseSchema.safeParse(response);

      expect(result.success).toBe(true);
    });

    it('given completed 상태와 아직 serving인 service, when 검증하면, then 모순을 거절한다', () => {
      const response = { ...givenMilitaryServiceResponse(), militaryStatus: 'completed' };

      const result = MilitaryServiceResponseSchema.safeParse(response);

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 장병적금 상품과 서버 계산 만기 예상액을 받은 경우', () => {
    it('given actual365 상품과 합계가 맞는 예상액, when 검증하면, then 상품과 계약을 허용한다', () => {
      const products = MilitarySavingsProductsResponseSchema.safeParse({
        items: [givenMilitarySavingsProduct()],
      });
      const history = MilitarySavingsHistoryResponseSchema.safeParse({
        items: [givenActiveMilitarySavingsHistory()],
        nextBefore: '11',
      });

      expect([products.success, history.success]).toEqual([true, true]);
    });

    it('given 잔액 부족으로 확정된 missed 회차, when 검증하면, then due day를 정산일로 허용한다', () => {
      const contract = givenActiveMilitarySavingsHistory();
      const response = {
        ...contract,
        missedInstallmentCount: 1,
        installments: [
          ...contract.installments,
          {
            id: '22',
            installmentNo: 2,
            dueGameDay: 85,
            status: 'missed' as const,
            paidGameDay: 85,
            principalKrw: 0,
            governmentMatchingPolicyVersionId: null,
            governmentMatchingRatePpm: null,
          },
        ],
      };

      const result = MilitarySavingsHistoryResponseSchema.safeParse({
        items: [response],
        nextBefore: null,
      });

      expect(result.success).toBe(true);
    });

    it('given 예상 총혜택이 원금·이자·정부지원 합계와 다름, when 검증하면, then 서버 계산 모순을 거절한다', () => {
      const contract = givenActiveMilitarySavingsHistory();
      const projectedMaturity = contract.projectedMaturity;
      if (projectedMaturity === null) throw new Error('활성 계약에는 예상 만기액이 있어야 한다');

      const result = MilitarySavingsHistoryResponseSchema.safeParse({
        items: [
          {
            ...contract,
            projectedMaturity: { ...projectedMaturity, totalBenefitKrw: 9_099_999 },
          },
        ],
        nextBefore: null,
      });

      expect(result.success).toBe(false);
    });

    it('given 중도해지 계약에 다음 납입일이 남음, when 검증하면, then 종료 상태 모순을 거절한다', () => {
      const contract = givenActiveMilitarySavingsHistory();
      const response = {
        ...contract,
        status: 'closed' as const,
        closedGameDay: 60,
        closureReason: 'earlyClose' as const,
        settledPrincipalKrw: 250_000,
        bankPayoutKrw: 250_000,
        projectedMaturity: null,
      };

      const result = MilitarySavingsHistoryResponseSchema.safeParse({
        items: [response],
        nextBefore: null,
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 복무 시작 command가 exact cursor를 사용한 경우', () => {
    it('given canonical UUID와 option ID 외의 필드, when 검증하면, then 추가 필드를 거절한다', () => {
      const request = {
        commandId: '00000000-0000-0000-0000-000000000001',
        expectedRunRevision: 1,
        expectedStateRevision: 2,
        expectedGameDay: 3,
        militaryOptionVersionId: '7',
        selectedByClient: true,
      };

      const result = MilitaryServiceStartRequestSchema.safeParse(request);

      expect(result.success).toBe(false);
    });
  });
});

describe('생활비 M4-A protocol 계약', () => {
  describe('맥락: 아홉 생활비 항목의 예산을 한 번에 변경하는 경우', () => {
    it('given 한 category가 중복된 선택, when 검증하면, then 전체 요청을 거절한다', () => {
      const selections = givenLifeBudgetSelections();
      selections[8] = { category: 'housing', bandId: '11' };
      const request = {
        commandId: '00000000-0000-0000-0000-000000000001',
        expectedRunRevision: 1,
        expectedStateRevision: 2,
        expectedGameDay: 3,
        selections,
      };

      const result = LifeBudgetUpdateRequestSchema.safeParse(request);

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 현재 월의 고정 산정 근거를 조회하는 경우', () => {
    it('given CPI와 항목별 계수 및 허용 band, when 검증하면, then strict 응답을 허용한다', () => {
      const response = givenActiveLifeBudgetResponse();

      const result = LifeBudgetResponseSchema.safeParse(response);

      expect(result.success).toBe(true);
    });

    it('given category 순서가 뒤섞인 서버 선택, when 검증하면, then 응답을 거절한다', () => {
      const response = givenActiveLifeBudgetResponse();
      const first = response.selections[0];
      const second = response.selections[1];
      if (first !== undefined && second !== undefined) {
        response.selections[0] = second;
        response.selections[1] = first;
      }

      const result = LifeBudgetResponseSchema.safeParse(response);

      expect(result.success).toBe(false);
    });

    it('given CPI가 없는 호환 런, when 검증하면, then 빈 예산 조회를 허용한다', () => {
      const response = {
        rateStatus: 'rateUnavailable',
        household: {
          id: '1',
          memberCount: 1,
          dependentCount: 0,
          taxDependentEligibleCount: 0,
        },
        residence: {
          id: '2',
          regionKey: 'capitalArea',
          tenureKind: 'rentFree',
          propertyHoldingId: null,
          effectiveFromGameDay: 0,
        },
        allowedBands: [],
        selections: [],
        currentMonth: null,
        activeArrears: [],
        hasMoreActiveArrears: false,
        totalEssentialArrearKrw: 0,
      };

      const result = LifeBudgetResponseSchema.safeParse(response);

      expect(result.success).toBe(true);
    });

    it('given 선택 항목에 미납액이 생긴 월, when 검증하면, then 월 계약을 거절한다', () => {
      const month = givenLivingCostMonth();
      const discretionary = month.items[8];
      if (discretionary !== undefined) {
        month.items[8] = { ...discretionary, grossKrw: 1, arrearKrw: 1 };
        month.totalGrossKrw = 1;
        month.totalArrearKrw = 1;
      }

      const result = LivingCostMonthSchema.safeParse(month);

      expect(result.success).toBe(false);
    });

    it('given 아직 정산 전인데 납부 결과가 있는 월, when 검증하면, then 월 계약을 거절한다', () => {
      const month = givenLivingCostMonth();
      const housing = month.items[0];
      if (housing !== undefined) {
        month.items[0] = { ...housing, grossKrw: 1, paidKrw: 1 };
        month.totalGrossKrw = 1;
        month.totalPaidKrw = 1;
      }

      const result = LivingCostMonthSchema.safeParse(month);

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 필수 생활비 미납액을 상환하는 경우', () => {
    it('given 상환 우선순위가 뒤집힌 연체 window, when 검증하면, then 응답을 거절한다', () => {
      const response = {
        ...givenActiveLifeBudgetResponse(),
        activeArrears: [
          {
            id: '2',
            dueYearMonth: { year: 2026, month: 2 },
            category: 'housing',
            originalKrw: 1,
            remainingKrw: 1,
          },
          {
            id: '1',
            dueYearMonth: { year: 2026, month: 1 },
            category: 'housing',
            originalKrw: 1,
            remainingKrw: 1,
          },
        ],
        totalEssentialArrearKrw: 2,
      };

      const result = LifeBudgetResponseSchema.safeParse(response);

      expect(result.success).toBe(false);
    });

    it('given 알려지지 않은 필드가 있는 지급 요청, when 검증하면, then 요청을 거절한다', () => {
      const request = {
        commandId: '00000000-0000-0000-0000-000000000001',
        expectedRunRevision: 1,
        expectedStateRevision: 2,
        expectedGameDay: 3,
        amountKrw: 10_000,
        remainingKrw: 20_000,
      };

      const result = EssentialArrearPaymentRequestSchema.safeParse(request);

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 생활비 정산과 원장 출처가 공개되는 경우', () => {
    it('given M4-A enum 이름, when 검증하면, then 고정 protocol만 허용한다', () => {
      const results = [
        SettlementKindSchema.safeParse('livingCostMonth').success,
        LedgerSourceKindSchema.safeParse('livingCostMonth').success,
        LedgerSourceKindSchema.safeParse('essentialArrearPayment').success,
        LedgerAccountCodeSchema.safeParse('livingCostExpense').success,
        LedgerAccountCodeSchema.safeParse('essentialArrearLiability').success,
      ];

      expect(results).toEqual([true, true, true, true, true]);
    });
  });

  describe('맥락: 대출 정산과 권위 부채 원장이 공개되는 경우', () => {
    it('given M4-B enum 이름, when 검증하면, then 고정 protocol만 허용한다', () => {
      const results = [
        SettlementKindSchema.safeParse('loanInstallment').success,
        LedgerSourceKindSchema.safeParse('loanOrigination').success,
        LedgerSourceKindSchema.safeParse('loanInstallment').success,
        LedgerSourceKindSchema.safeParse('loanPrepayment').success,
        LedgerSourceKindSchema.safeParse('debtAuthorityBridge').success,
        LedgerAccountCodeSchema.safeParse('loanPrincipalLiability').success,
        LedgerAccountCodeSchema.safeParse('loanInterestExpense').success,
        LedgerAccountCodeSchema.safeParse('loanInterestLiability').success,
        LedgerAccountCodeSchema.safeParse('loanFeeExpense').success,
        LedgerAccountCodeSchema.safeParse('taxObligationLiability').success,
      ];

      expect(results.every(Boolean)).toBe(true);
    });

    it('given 고정·변동 시작 상품, when catalog를 검증하면, then exact typed 조건을 허용한다', () => {
      const catalog = givenLoanProductCatalog();

      const result = LoanProductCatalogSchema.safeParse(catalog);

      expect(result.success).toBe(true);
    });

    it('given housing 전용 전세자금대출 한 건, when catalog를 검증하면, then 시작 부채가 아닌 전용 실행 상품을 허용한다', () => {
      const catalog = givenLoanProductCatalog();

      const result = LoanProductCatalogSchema.safeParse({
        ...catalog,
        products: [...catalog.products, givenLeaseDepositLoanProduct()],
      });

      expect(result.success).toBe(true);
    });

    it('given housing purchase 전용 주담대 한 건, when catalog를 검증하면, then generic 시작 부채가 아닌 상품을 허용한다', () => {
      const catalog = givenLoanProductCatalog();

      const result = LoanProductCatalogSchema.safeParse({
        ...catalog,
        products: [...catalog.products, givenMortgageProduct()],
      });

      expect(result.success).toBe(true);
      expect(HousingMortgageProductSchema.safeParse(givenMortgageProduct()).success).toBe(true);
    });

    it('given starting debt로 노출된 주담대, when catalog를 검증하면, then housing 전용 channel 모순을 거절한다', () => {
      const catalog = givenLoanProductCatalog();

      const result = LoanProductCatalogSchema.safeParse({
        ...catalog,
        products: [...catalog.products, { ...givenMortgageProduct(), startingEligible: true }],
      });

      expect(result.success).toBe(false);
    });

    it('given 내부 execution channel을 공개한 상품, when catalog를 검증하면, then public shape가 거절한다', () => {
      const catalog = givenLoanProductCatalog();

      const result = LoanProductCatalogSchema.safeParse({
        ...catalog,
        products: [
          ...catalog.products,
          { ...givenLeaseDepositLoanProduct(), executionChannel: 'leaseMove' },
        ],
      });

      expect(result.success).toBe(false);
    });

    it('given 전세자금대출 상품 두 건, when catalog를 검증하면, then pinned 전용 상품 cardinality를 거절한다', () => {
      const catalog = givenLoanProductCatalog();
      const product = givenLeaseDepositLoanProduct();

      const result = LoanProductCatalogSchema.safeParse({
        ...catalog,
        products: [
          ...catalog.products,
          product,
          { ...product, id: '23', key: 'another-lease-deposit-loan' },
        ],
      });

      expect(result.success).toBe(false);
    });

    it('given 시작 부채로 표시된 전세자금대출, when catalog를 검증하면, then channel 혼합을 거절한다', () => {
      const catalog = givenLoanProductCatalog();

      const result = LoanProductCatalogSchema.safeParse({
        ...catalog,
        products: [
          ...catalog.products,
          { ...givenLeaseDepositLoanProduct(), startingEligible: true },
        ],
      });

      expect(result.success).toBe(false);
    });

    it('given 종류별 시작 상품이 중복된 catalog, when 검증하면, then 임의 선택을 거절한다', () => {
      const catalog = givenLoanProductCatalog();
      const duplicate = {
        ...catalog.products[0],
        id: '22',
        key: 'another-student-loan',
      };

      const result = LoanProductCatalogSchema.safeParse({
        ...catalog,
        products: [...catalog.products, duplicate],
      });

      expect(result.success).toBe(false);
    });

    it('given 이용 불가 금리와 값이 함께 온 상품, when 검증하면, then 모순을 거절한다', () => {
      const catalog = givenLoanProductCatalog();
      const product = catalog.products[1];

      const result = LoanProductCatalogSchema.safeParse({
        ...catalog,
        products: [
          catalog.products[0],
          { ...product, rateStatus: 'rateUnavailable', currentAnnualRateBp: 655 },
        ],
      });

      expect(result.success).toBe(false);
    });

    it('given cursor와 상품·원금만 있는 견적 요청, when 검증하면, then strict 명령을 허용한다', () => {
      const request = {
        commandId: '00000000-0000-4000-8000-000000000001',
        expectedRunRevision: 3,
        expectedStateRevision: 12,
        expectedGameDay: 120,
        productVersionId: '21',
        principalKrw: 10_000_000,
      };

      const result = LoanQuoteRequestSchema.safeParse(request);

      expect(result.success).toBe(true);
    });

    it('given 클라이언트 계산 금리를 덧붙인 견적 요청, when 검증하면, then unknown 필드를 거절한다', () => {
      const request = {
        commandId: '00000000-0000-4000-8000-000000000001',
        expectedRunRevision: 3,
        expectedStateRevision: 12,
        expectedGameDay: 120,
        productVersionId: '21',
        principalKrw: 10_000_000,
        annualRateBp: 655,
      };

      const result = LoanQuoteRequestSchema.safeParse(request);

      expect(result.success).toBe(false);
    });

    it('given cursor와 quote ID만 있는 실행 요청, when 검증하면, then strict 명령을 허용한다', () => {
      const request = {
        commandId: '00000000-0000-4000-8000-000000000002',
        expectedRunRevision: 3,
        expectedStateRevision: 12,
        expectedGameDay: 120,
        quoteId: '30',
      };

      const result = LoanExecutionRequestSchema.safeParse(request);

      expect(result.success).toBe(true);
    });

    it('given 실행 요청에 상품과 원금을 다시 넣으면, when 검증하면, then unknown 필드를 거절한다', () => {
      const request = {
        commandId: '00000000-0000-4000-8000-000000000002',
        expectedRunRevision: 3,
        expectedStateRevision: 12,
        expectedGameDay: 120,
        quoteId: '30',
        productVersionId: '21',
        principalKrw: 10_000_000,
      };

      const result = LoanExecutionRequestSchema.safeParse(request);

      expect(result.success).toBe(false);
    });

    it('given 실행 계약과 첫 납입이 일관되면, when 검증하면, then strict 결과를 허용한다', () => {
      const execution = givenLoanExecutionResult();

      const result = LoanExecutionResultSchema.safeParse(execution);

      expect(result.success).toBe(true);
    });

    it('given 첫 납입일이 만기 뒤면, when 실행 결과를 검증하면, then 일관성 위반을 거절한다', () => {
      const execution = givenLoanExecutionResult();

      const result = LoanExecutionResultSchema.safeParse({
        ...execution,
        firstInstallment: {
          ...execution.firstInstallment,
          dueGameDay: execution.maturityGameDay + 1,
        },
      });

      expect(result.success).toBe(false);
    });

    it('given cursor와 줄일 원금만 있는 조기상환 요청, when 검증하면, then strict 명령을 허용한다', () => {
      const request = {
        commandId: '00000000-0000-4000-8000-000000000003',
        expectedRunRevision: 3,
        expectedStateRevision: 12,
        expectedGameDay: 120,
        principalKrw: 1_000_000,
      };

      const result = LoanPrepaymentRequestSchema.safeParse(request);

      expect(result.success).toBe(true);
    });

    it('given path ID와 계산값을 덧붙인 조기상환 요청, when 검증하면, then unknown 필드를 거절한다', () => {
      const request = {
        commandId: '00000000-0000-4000-8000-000000000003',
        expectedRunRevision: 3,
        expectedStateRevision: 12,
        expectedGameDay: 120,
        loanId: '40',
        principalKrw: 1_000_000,
        feeKrw: 10_000,
      };

      const result = LoanPrepaymentRequestSchema.safeParse(request);

      expect(result.success).toBe(false);
    });

    it('given 잔액과 남은 일정이 일관된 조기상환, when 검증하면, then strict 결과를 허용한다', () => {
      const prepayment = givenLoanPrepaymentResult();

      const result = LoanPrepaymentResultSchema.safeParse(prepayment);

      expect(result.success).toBe(true);
    });

    it('given 전액상환으로 일정이 사라진 결과, when 검증하면, then paidOff 결과를 허용한다', () => {
      const prepayment = givenLoanPrepaymentResult();

      const result = LoanPrepaymentResultSchema.safeParse({
        ...prepayment,
        remainingPrincipalKrw: 0,
        status: 'paidOff',
        remainingInstallments: 0,
        nextInstallment: null,
        finalInstallmentDueGameDay: null,
      });

      expect(result.success).toBe(true);
    });

    it('given 원금과 수수료 합이 다른 총 차감액, when 검증하면, then 금액 모순을 거절한다', () => {
      const prepayment = givenLoanPrepaymentResult();

      const result = LoanPrepaymentResultSchema.safeParse({
        ...prepayment,
        totalDebitedKrw: prepayment.totalDebitedKrw + 1,
      });

      expect(result.success).toBe(false);
    });

    it('given 다음 회차 구성과 합계가 다른 조기상환, when 검증하면, then 일정 모순을 거절한다', () => {
      const prepayment = givenLoanPrepaymentResult();

      const result = LoanPrepaymentResultSchema.safeParse({
        ...prepayment,
        nextInstallment: {
          ...prepayment.nextInstallment,
          totalKrw: prepayment.nextInstallment.totalKrw + 1,
        },
      });

      expect(result.success).toBe(false);
    });

    it('given 완납 상태에 남은 일정이 있는 조기상환, when 검증하면, then 상태 모순을 거절한다', () => {
      const prepayment = givenLoanPrepaymentResult();

      const result = LoanPrepaymentResultSchema.safeParse({
        ...prepayment,
        remainingPrincipalKrw: 0,
        status: 'paidOff',
      });

      expect(result.success).toBe(false);
    });

    it('given 현재 run의 일반 대출 상세, when 검증하면, then strict 계약 정보를 허용한다', () => {
      const detail = givenLoanDetail();

      const result = LoanDetailSchema.safeParse(detail);

      expect(result.success).toBe(true);
    });

    it('given 임대차에 연결된 전세자금대출 상세, when 검증하면, then lease contract ID를 허용한다', () => {
      const detail = givenLoanDetail();

      const result = LoanDetailSchema.safeParse({
        ...detail,
        leaseContractId: '8002',
        propertyHoldingId: null,
        productVersionId: '22',
        productKind: 'leaseDepositLoan',
        displayName: '개발 전세자금 고정금리 대출',
        currentAnnualRateBp: 400,
        repaymentMethod: 'bullet',
        termMonths: 24,
        totalInstallments: 24,
        prepaymentFeePpm: 0,
        prepaymentEffect: 'reduceTerm',
        dsrIncluded: false,
      });

      expect(result.success).toBe(true);
    });

    it('given 임대차 ID 없는 전세자금대출 상세, when 검증하면, then 연결 불변식을 거절한다', () => {
      const detail = givenLoanDetail();

      const result = LoanDetailSchema.safeParse({
        ...detail,
        productKind: 'leaseDepositLoan',
      });

      expect(result.success).toBe(false);
    });

    it('given 보유주택에 연결된 주택담보대출 상세, when 검증하면, then property holding ID를 허용한다', () => {
      const detail = givenLoanDetail();

      const result = LoanDetailSchema.safeParse({
        ...detail,
        propertyHoldingId: '9401',
        productVersionId: '23',
        productKind: 'mortgage',
        displayName: '개발 주택담보 고정금리 대출',
        currentAnnualRateBp: 400,
        termMonths: 360,
        totalInstallments: 360,
      });

      expect(result.success).toBe(true);
    });

    it('given 보유주택 ID 없는 주택담보대출 상세, when 검증하면, then lien 연결 불변식을 거절한다', () => {
      const detail = givenLoanDetail();

      const result = LoanDetailSchema.safeParse({
        ...detail,
        productKind: 'mortgage',
      });

      expect(result.success).toBe(false);
    });

    it('given 이용 불가 금리에 값이 있는 상세, when 검증하면, then rate 모순을 거절한다', () => {
      const detail = givenLoanDetail();

      const result = LoanDetailSchema.safeParse({
        ...detail,
        rateStatus: 'rateUnavailable',
      });

      expect(result.success).toBe(false);
    });

    it('given schedule과 조기상환 terms가 없는 legacy 상세, when 검증하면, then 조회 전용 이력을 허용한다', () => {
      const detail = givenLoanDetail();

      const result = LoanDetailSchema.safeParse({
        ...detail,
        productKind: 'legacyDebt',
        displayName: '이전 버전 합산 부채',
        rateStatus: 'rateUnavailable',
        currentAnnualRateBp: null,
        readOnly: true,
        repaymentMethod: 'bullet',
        termMonths: null,
        totalInstallments: null,
        maturityGameDay: null,
        finalInstallmentDueGameDay: null,
        nextInstallmentNo: null,
        prepaymentAllowed: false,
        prepaymentFeePpm: null,
        prepaymentEffect: null,
        dsrIncluded: false,
      });

      expect(result.success).toBe(true);
    });

    it('given unknown query와 범위 밖 limit과 비정규 cursor, when 검증하면, then history 조회를 거절한다', () => {
      const unknown = LoanInstallmentHistoryQuerySchema.safeParse({ limit: 50, offset: 10 });
      const outOfRange = LoanInstallmentHistoryQuerySchema.safeParse({ limit: 51 });
      const malformed = LoanInstallmentHistoryQuerySchema.safeParse({
        before: 'v1.l040.i0.p0',
      });
      const outOfRangeCursor = LoanInstallmentHistoryQuerySchema.safeParse({
        before: 'v1.l40.i65536.p0',
      });

      const result = [
        unknown.success,
        outOfRange.success,
        malformed.success,
        outOfRangeCursor.success,
      ];

      expect(result).toEqual([false, false, false, false]);
    });

    it('given 두 window의 내림차순 기록과 cursor, when 검증하면, then strict 이력을 허용한다', () => {
      const history = givenLoanInstallmentHistory();

      const result = LoanInstallmentHistoryResponseSchema.safeParse(history);

      expect(result.success).toBe(true);
    });

    it('given 이사 transaction의 전세대출 payoff, when 납부 이력을 검증하면, then 조기상환 원금 allocation을 허용한다', () => {
      const history = givenLoanInstallmentHistory();

      const result = LoanInstallmentHistoryResponseSchema.safeParse({
        ...history,
        payments: [
          {
            ...givenLoanPayment(2, '52'),
            kind: 'leaseMovePayoff',
            amountKrw: 1_000_000,
            allocations: [{ kind: 'prepaymentPrincipal', amountKrw: 1_000_000 }],
          },
          givenLoanPayment(1, '51'),
        ],
      });

      expect(result.success).toBe(true);
    });

    it('given 오름차순 installment, when 검증하면, then 안정된 history 순서를 거절한다', () => {
      const history = givenLoanInstallmentHistory();

      const result = LoanInstallmentHistoryResponseSchema.safeParse({
        ...history,
        installments: [...history.installments].reverse(),
      });

      expect(result.success).toBe(false);
    });

    it('given payment보다 allocation 합이 큰 이력, when 검증하면, then 금액 모순을 거절한다', () => {
      const history = givenLoanInstallmentHistory();
      const payment = givenLoanPayment(2, '52');
      const fee = { kind: 'prepaymentFee', amountKrw: 10_000 } as const;
      const principal = { kind: 'prepaymentPrincipal', amountKrw: 1_000_001 } as const;

      const result = LoanInstallmentHistoryResponseSchema.safeParse({
        ...history,
        payments: [
          {
            ...payment,
            allocations: [fee, principal],
          },
          givenLoanPayment(1, '51'),
        ],
      });

      expect(result.success).toBe(false);
    });

    it('given 끝난 payment window를 계속 가리키는 cursor, when 검증하면, then sentinel 모순을 거절한다', () => {
      const history = givenLoanInstallmentHistory();

      const wrongSentinel = LoanInstallmentHistoryResponseSchema.safeParse({
        ...history,
        nextBefore: 'v1.l40.i59.p1',
      });
      const wrongLoan = LoanInstallmentHistoryResponseSchema.safeParse({
        ...history,
        nextBefore: 'v1.l41.i59.p0',
      });

      const result = [wrongSentinel.success, wrongLoan.success];

      expect(result).toEqual([false, false]);
    });

    it('given 최신 재심사 실패 코드, when 검증하면, then 고정 loan failure protocol을 허용한다', () => {
      const codes = [
        'creditRestricted',
        'incomeUnavailable',
        'debtServiceLimit',
        'contractConflict',
        'loanNotFound',
      ];

      const results = codes.map((code) => LifeFailureCodeSchema.safeParse(code).success);

      expect(results).toEqual([true, true, true, true, true]);
    });

    it('given 소득과 DSR 근거가 일치하는 eligible 견적, when 검증하면, then exact 결과를 허용한다', () => {
      const quote = givenLoanQuoteResult();

      const result = LoanQuoteResultSchema.safeParse(quote);

      expect(result.success).toBe(true);
    });

    it('given DSR 적용과 인정소득은 있지만 근거가 없는 견적, when 검증하면, then 불완전한 심사를 거절한다', () => {
      const quote = givenLoanQuoteResult();

      const result = LoanQuoteResultSchema.safeParse({ ...quote, dsr: null });

      expect(result.success).toBe(false);
    });

    it('given 우선순서가 뒤집힌 신용 제한 사유, when 검증하면, then canonical reason 위반을 거절한다', () => {
      const quote = givenLoanQuoteResult();

      const result = LoanQuoteResultSchema.safeParse({
        ...quote,
        decisionCode: 'creditRestricted',
        decisionReasons: ['creditBandRestricted', 'activeDefault'],
        verifiedAnnualIncomeKrw: null,
        verifiedIncomeSource: null,
        dsrApplied: false,
        dsr: null,
      });

      expect(result.success).toBe(false);
    });

    it('given 요청 원금이 빠진 실행 후 잔액, when 검증하면, then 잔액 불일치를 거절한다', () => {
      const quote = givenLoanQuoteResult();

      const result = LoanQuoteResultSchema.safeParse({
        ...quote,
        postExecutionBalanceKrw: quote.postExecutionBalanceKrw - 1,
      });

      expect(result.success).toBe(false);
    });

    it('given 원 단위 분수와 다른 DSR ppm, when 검증하면, then ratio 불일치를 거절한다', () => {
      const quote = givenLoanQuoteResult();

      const result = LoanQuoteResultSchema.safeParse({
        ...quote,
        dsr: { ...quote.dsr, ratioPpm: 400_001 },
      });

      expect(result.success).toBe(false);
    });

    it('given 변동금리 전체 기간 예상액을 덧붙인 견적, when 검증하면, then 고정되지 않은 미래 합계를 거절한다', () => {
      const quote = givenLoanQuoteResult();

      const result = LoanQuoteResultSchema.safeParse({
        ...quote,
        quotedTerms: { ...quote.quotedTerms, totalInterestKrw: 1_000_000 },
      });

      expect(result.success).toBe(false);
    });

    it('given active credit과 다음 납입, when credit 응답을 검증하면, then 요약 projection을 허용한다', () => {
      const response = {
        creditBand: 'standard',
        creditReasons: ['cleanHistory'],
        activeLoans: [
          {
            id: '10',
            productVersionId: '20',
            productKind: 'studentLoan',
            displayName: '개발 학자금 고정금리 대출',
            rateStatus: 'available',
            currentAnnualRateBp: 170,
            status: 'active',
            remainingPrincipalKrw: 1_000_000,
            overdueKrw: 0,
            readOnly: false,
          },
        ],
        nextLoanInstallment: {
          loanId: '10',
          installmentNo: 1,
          dueGameDay: 30,
          feeKrw: 0,
          interestKrw: 1_397,
          principalKrw: 8_333,
          remainingDueKrw: 9_730,
        },
        totalLoanBalanceKrw: 1_000_000,
      };

      const result = CreditResponseSchema.safeParse(response);

      expect(result.success).toBe(true);
    });

    it('given adverse 계약이 없는 active credit과 빈 사유, when 검증하면, then cleanHistory 누락을 거절한다', () => {
      const response = {
        creditBand: 'standard',
        creditReasons: [],
        activeLoans: [],
        nextLoanInstallment: null,
        totalLoanBalanceKrw: 0,
      };

      const result = CreditResponseSchema.safeParse(response);

      expect(result.success).toBe(false);
    });

    it('given active credit과 다음 납입, when 검증하면, then bounded 요약을 허용한다', () => {
      const result = LifeSnapshotSchema.safeParse({
        rateStatus: 'active',
        household: {
          id: '1',
          memberCount: 1,
          dependentCount: 0,
          taxDependentEligibleCount: 0,
        },
        residence: {
          id: '2',
          regionKey: 'capitalArea',
          tenureKind: 'rentFree',
          propertyHoldingId: null,
          effectiveFromGameDay: 0,
        },
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
        creditBand: 'standard',
        creditReasons: ['cleanHistory'],
        activeLoans: [
          {
            id: '10',
            productVersionId: '20',
            productKind: 'studentLoan',
            displayName: '개발 학자금 고정금리 대출',
            rateStatus: 'available',
            currentAnnualRateBp: 170,
            status: 'active',
            remainingPrincipalKrw: 1_000_000,
            overdueKrw: 0,
            readOnly: false,
          },
        ],
        nextLoanInstallment: {
          loanId: '10',
          installmentNo: 1,
          dueGameDay: 30,
          feeKrw: 0,
          interestKrw: 1_397,
          principalKrw: 8_333,
          remainingDueKrw: 9_730,
        },
        totalLoanBalanceKrw: 1_000_000,
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
      });

      expect(result.success).toBe(true);
    });

    it('given mutable legacy debt 요약, when 검증하면, then compatibility 모순을 거절한다', () => {
      const result = LifeSnapshotSchema.safeParse({
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
        activeLoans: [
          {
            id: '10',
            productVersionId: '20',
            productKind: 'legacyDebt',
            displayName: '이전 버전 합산 부채',
            rateStatus: 'rateUnavailable',
            currentAnnualRateBp: null,
            status: 'active',
            remainingPrincipalKrw: 1_000_000,
            overdueKrw: 0,
            readOnly: false,
          },
        ],
        nextLoanInstallment: null,
        totalLoanBalanceKrw: 1_000_000,
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
      });

      expect(result.success).toBe(false);
    });
  });
});

describe('보험 스냅샷 protocol 계약', () => {
  describe('맥락: 활성 보험계약 여러 건을 요약하는 경우', () => {
    it('given 오름차순 계약 ID, when 검증하면, then 서버 canonical 순서를 허용한다', () => {
      const life = givenJeonseLifeSnapshot();

      const result = LifeSnapshotSchema.safeParse({
        ...life,
        insuranceCapability: 'contractsAndClaims',
        activeInsuranceContracts: [
          givenActiveInsuranceContract('91'),
          givenActiveInsuranceContract('92'),
        ],
      });

      expect(result.success).toBe(true);
    });

    it('given 내림차순 계약 ID, when 검증하면, then snapshot 순서 위반을 거절한다', () => {
      const life = givenJeonseLifeSnapshot();

      const result = LifeSnapshotSchema.safeParse({
        ...life,
        insuranceCapability: 'contractsAndClaims',
        activeInsuranceContracts: [
          givenActiveInsuranceContract('92'),
          givenActiveInsuranceContract('91'),
        ],
      });

      expect(result.success).toBe(false);
    });
  });
});

describe('주거 M4-C1 매물 조회 protocol 계약', () => {
  describe('맥락: 공개된 지역의 현재 월 매물을 조회하는 경우', () => {
    it('given canonical 지역·offer와 서버 산정 금액, when 검증하면, then strict active 응답을 허용한다', () => {
      const response = givenHousingListingsResponse();

      const result = HousingListingsResponseSchema.safeParse(response);

      expect(result.success).toBe(true);
    });

    it('given 지원 지역 하나만 있는 query, when 검증하면, then strict 선택 지역을 허용한다', () => {
      const query = { region: 'smallCity' };

      const result = HousingListingsQuerySchema.safeParse(query);

      expect(result.success).toBe(true);
    });
  });

  describe('맥락: query 또는 응답에 공개 계약 밖 값이 있는 경우', () => {
    it('given unknown query와 지원하지 않는 region, when 검증하면, then 요청을 거절한다', () => {
      const unknown = HousingListingsQuerySchema.safeParse({ region: 'rural', limit: 24 });
      const invalidRegion = HousingListingsQuerySchema.safeParse({ region: 'overseas' });

      const result = [unknown.success, invalidRegion.success];

      expect(result).toEqual([false, false]);
    });

    it('given profile 원시 입력을 덧붙인 응답, when 검증하면, then unknown 필드를 거절한다', () => {
      const response = givenHousingListingsResponse();

      const result = HousingListingsResponseSchema.safeParse({
        ...response,
        worldSeed: 42,
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: compatibility model의 지수를 사용할 수 없는 경우', () => {
    it('given null 지수와 빈 매물, when 검증하면, then strict disabled 응답을 허용한다', () => {
      const response = givenHousingListingsResponse();

      const result = HousingListingsResponseSchema.safeParse({
        ...response,
        rateStatus: 'rateUnavailable',
        priceIndexPpm: null,
        rentIndexPpm: null,
        listings: [],
      });

      expect(result.success).toBe(true);
    });

    it('given disabled 상태에 매물이 남은 응답, when 검증하면, then availability 모순을 거절한다', () => {
      const response = givenHousingListingsResponse();

      const result = HousingListingsResponseSchema.safeParse({
        ...response,
        rateStatus: 'rateUnavailable',
        priceIndexPpm: null,
        rentIndexPpm: null,
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 공개 배열의 canonical 순서와 상한이 깨진 경우', () => {
    it('given 뒤집힌 지역 순서, when 검증하면, then canonical region 위반을 거절한다', () => {
      const response = givenHousingListingsResponse();

      const result = HousingListingsResponseSchema.safeParse({
        ...response,
        regions: [...response.regions].reverse(),
      });

      expect(result.success).toBe(false);
    });

    it('given 뒤집힌 offer 순서, when 검증하면, then canonical offer 위반을 거절한다', () => {
      const response = givenHousingListingsResponse();
      const listing = response.listings[0];

      const result = HousingListingsResponseSchema.safeParse({
        ...response,
        listings:
          listing === undefined ? [] : [{ ...listing, offers: [...listing.offers].reverse() }],
      });

      expect(result.success).toBe(false);
    });

    it('given 지역 5개·매물 25개·offer 4개, when 검증하면, then 모든 공개 배열 상한을 거절한다', () => {
      const response = givenHousingListingsResponse();
      const listing = response.listings[0];
      const listings = Array.from({ length: 25 }, (_, index) => ({
        ...listing,
        id: String(8_000 + index),
      }));

      const tooManyRegions = HousingListingsResponseSchema.safeParse({
        ...response,
        regions: [...response.regions, { regionKey: 'rural', displayName: '농촌 중복' }],
      });
      const tooManyListings = HousingListingsResponseSchema.safeParse({ ...response, listings });
      const tooManyOffers = HousingListingsResponseSchema.safeParse({
        ...response,
        listings:
          listing === undefined
            ? []
            : [
                {
                  ...listing,
                  offers: [...listing.offers, { kind: 'sale', priceKrw: 1 }],
                },
              ],
      });

      expect([tooManyRegions.success, tooManyListings.success, tooManyOffers.success]).toEqual([
        false,
        false,
        false,
      ]);
    });
  });

  describe('맥락: 선택 지역의 현재 유효 매물이 아닌 경우', () => {
    it('given 다른 지역 매물, when 검증하면, then response correlation 위반을 거절한다', () => {
      const response = givenHousingListingsResponse();
      const listing = response.listings[0];

      const result = HousingListingsResponseSchema.safeParse({
        ...response,
        listings: listing === undefined ? [] : [{ ...listing, regionKey: 'rural' }],
      });

      expect(result.success).toBe(false);
    });

    it('given 현재 game day 전에 끝난 매물, when 검증하면, then 유효기간 위반을 거절한다', () => {
      const response = givenHousingListingsResponse();
      const listing = response.listings[0];

      const result = HousingListingsResponseSchema.safeParse({
        ...response,
        listings:
          listing === undefined ? [] : [{ ...listing, availableToGameDay: response.gameDay - 1 }],
      });

      expect(result.success).toBe(false);
    });
  });
});

describe('주거 M4-C2 임대차 protocol 계약', () => {
  describe('맥락: 현금 전세 capability가 활성화된 경우', () => {
    it('given canonical 이사비와 현재 계약, when 조회 응답을 검증하면, then 허용한다', () => {
      const response = givenCurrentLeaseResponse();

      const result = HousingCurrentLeaseResponseSchema.safeParse(response);

      expect(result.success).toBe(true);
    });

    it('given lifecycle null 필드를 생략한 legacy 응답, when 검증하면, then strict shape 위반을 거절한다', () => {
      const response = givenCurrentLeaseResponse();

      const result = HousingCurrentLeaseResponseSchema.safeParse({
        ...response,
        leaseLifecycleTerms: undefined,
      });

      expect(result.success).toBe(false);
    });

    it('given 뒤집힌 이사비 지역 순서, when 조회 응답을 검증하면, then 거절한다', () => {
      const response = givenCurrentLeaseResponse();

      const result = HousingCurrentLeaseResponseSchema.safeParse({
        ...response,
        movingCosts: [...response.movingCosts].reverse(),
      });

      expect(result.success).toBe(false);
    });

    it('given 현재 계약과 다른 보증금 자산, when 조회 응답을 검증하면, then 거절한다', () => {
      const response = givenCurrentLeaseResponse();

      const result = HousingCurrentLeaseResponseSchema.safeParse({
        ...response,
        tenantLeaseDepositKrw: response.tenantLeaseDepositKrw + 1,
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: v4 고정기간 자동갱신 월세 계약을 조회하는 경우', () => {
    it('given 현재 term과 게시된 갱신 안내와 종료 검토, when 응답을 검증하면, then 허용한다', () => {
      const response = givenFixedTermMonthlyCurrentLeaseResponse();

      const result = HousingCurrentLeaseResponseSchema.safeParse(response);

      expect(result.success).toBe(true);
    });

    it('given current term 없는 고정기간 계약, when 응답을 검증하면, then 상관관계 위반을 거절한다', () => {
      const response = givenFixedTermMonthlyCurrentLeaseResponse();

      const result = HousingCurrentLeaseResponseSchema.safeParse({
        ...response,
        activeLease: { ...response.activeLease, currentTerm: null },
      });

      expect(result.success).toBe(false);
    });

    it('given 다른 term을 가리키는 갱신 안내, when 응답을 검증하면, then 상관관계 위반을 거절한다', () => {
      const response = givenFixedTermMonthlyCurrentLeaseResponse();
      const notice = response.activeLease.renewalNotice;

      const result = HousingCurrentLeaseResponseSchema.safeParse({
        ...response,
        activeLease: {
          ...response.activeLease,
          renewalNotice: notice === null ? null : { ...notice, termNo: notice.termNo + 1 },
        },
      });

      expect(result.success).toBe(false);
    });

    it('given 전체 연체보다 큰 활성 계약 종료 검토 금액, when 응답을 검증하면, then 모순을 거절한다', () => {
      const response = givenFixedTermMonthlyCurrentLeaseResponse();
      const review = response.activeLease.terminationReview;

      const result = HousingCurrentLeaseResponseSchema.safeParse({
        ...response,
        activeLease: {
          ...response.activeLease,
          terminationReview:
            review === null
              ? null
              : { ...review, activeLeaseArrearKrw: response.totalLeaseArrearKrw + 1 },
        },
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 기존 model에 lease capability가 없는 경우', () => {
    it('given exact null·empty compatibility 응답, when 검증하면, then 허용한다', () => {
      const response = {
        leaseCapability: 'unavailable',
        renewalRule: null,
        leaseLifecycleTerms: null,
        movingCosts: [],
        tenantLeaseDepositKrw: 0,
        activeLease: null,
        monthlyRentTerms: null,
        activeArrears: [],
        hasMoreActiveArrears: false,
        totalLeaseArrearKrw: 0,
      };

      const result = HousingCurrentLeaseResponseSchema.safeParse(response);

      expect(result.success).toBe(true);
    });

    it('given unavailable 상태의 숨은 이사비, when 검증하면, then 거절한다', () => {
      const response = {
        leaseCapability: 'unavailable',
        renewalRule: null,
        leaseLifecycleTerms: null,
        movingCosts: [{ regionKey: 'rural', movingCostKrw: 300_000 }],
        tenantLeaseDepositKrw: 0,
        activeLease: null,
        monthlyRentTerms: null,
        activeArrears: [],
        hasMoreActiveArrears: false,
        totalLeaseArrearKrw: 0,
      };

      const result = HousingCurrentLeaseResponseSchema.safeParse(response);

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 전세 listing으로 원자적 이사를 명령하는 경우', () => {
    it('given 공통 cursor와 jeonse listing ID, when 요청을 검증하면, then strict body를 허용한다', () => {
      const request = givenLeaseRequest();

      const result = HousingLeaseRequestSchema.safeParse(request);

      expect(result.success).toBe(true);
    });

    it('given monthlyRent listing과 공통 cursor, when 요청을 검증하면, then strict body를 허용한다', () => {
      const request = givenLeaseRequest();

      const monthlyRent = HousingLeaseRequestSchema.safeParse({
        ...request,
        offerKind: 'monthlyRent',
      });

      expect(monthlyRent.success).toBe(true);
    });

    it('given eligible quote ID가 있는 financed 전세, when 요청을 검증하면, then strict v2 branch를 허용한다', () => {
      const request = givenLeaseRequest();

      const result = HousingLeaseRequestSchema.safeParse({
        ...request,
        loanQuoteId: '31',
      });

      expect(result.success).toBe(true);
    });

    it('given 월세에 전세대출 quote ID를 섞은 body, when 요청을 검증하면, then strict union이 거절한다', () => {
      const request = givenLeaseRequest();

      const result = HousingLeaseRequestSchema.safeParse({
        ...request,
        offerKind: 'monthlyRent',
        loanQuoteId: '31',
      });

      expect(result.success).toBe(false);
    });

    it('given client 산정 이사비, when 요청을 검증하면, then 공개 body 밖 필드를 거절한다', () => {
      const request = givenLeaseRequest();

      const clientCost = HousingLeaseRequestSchema.safeParse({
        ...request,
        movingCostKrw: 600_000,
      });

      expect(clientCost.success).toBe(false);
    });

    it('given 반환 보증금·새 보증금·이사비, when result를 검증하면, then 지갑 변화를 허용한다', () => {
      const response = givenLeaseResult();

      const result = HousingLeaseResultSchema.safeParse(response);

      expect(result.success).toBe(true);
    });

    it('given 종료 계약 없는 전세대출 payoff, when result를 검증하면, then 연결 불변식을 거절한다', () => {
      const response = givenLeaseResult();

      const result = HousingLeaseResultSchema.safeParse({
        ...response,
        returnedDepositKrw: 0,
        walletDeltaKrw: -280_450_000,
        endedLeaseId: null,
        repaidDepositLoan: {
          loanId: '40',
          paymentId: '51',
          principalKrw: 100_000_000,
        },
      });

      expect(result.success).toBe(false);
    });

    it('given v4 자동갱신 규칙 result, when 검증하면, then 기존 move shape 그대로 허용한다', () => {
      const response = givenLeaseResult();

      const result = HousingLeaseResultSchema.safeParse({
        ...response,
        renewalRule: 'fixedTermAutoRenew',
      });

      expect(result.success).toBe(true);
    });

    it('given 합계와 다른 wallet delta, when result를 검증하면, then 회계 모순을 거절한다', () => {
      const response = givenLeaseResult();

      const result = HousingLeaseResultSchema.safeParse({
        ...response,
        walletDeltaKrw: response.walletDeltaKrw + 1,
      });

      expect(result.success).toBe(false);
    });

    it('given listing·상품·원금만 있는 전세대출 견적, when 요청을 검증하면, then strict 전용 body를 허용한다', () => {
      const request = givenLeaseRequest();

      const result = HousingLeaseDepositLoanQuoteRequestSchema.safeParse({
        ...request,
        productVersionId: '22',
        principalKrw: 80_000_000,
      });

      expect(result.success).toBe(true);
    });

    it('given client가 보증금 한도를 보낸 전세대출 견적, when 요청을 검증하면, then 권위 밖 필드를 거절한다', () => {
      const request = givenLeaseRequest();

      const result = HousingLeaseDepositLoanQuoteRequestSchema.safeParse({
        ...request,
        productVersionId: '22',
        principalKrw: 80_000_000,
        maximumFundingKrw: 80_000_000,
      });

      expect(result.success).toBe(false);
    });

    it('given 서버 한도와 개발 상환여력 근거, when 견적 result를 검증하면, then eligible 전용 shape를 허용한다', () => {
      const quote = givenLeaseDepositLoanQuoteResult();

      const result = HousingLeaseDepositLoanQuoteResultSchema.safeParse(quote);

      expect(result.success).toBe(true);
    });

    it('given 기존 전세대출을 전액 대체하는 견적, when result를 검증하면, then 실행 후 잔액을 허용한다', () => {
      const quote = givenLeaseDepositLoanQuoteResult();

      const result = HousingLeaseDepositLoanQuoteResultSchema.safeParse({
        ...quote,
        existingLoanBalanceKrw: 100_000_000,
        replacedLoanId: '40',
        replacedLoanPrincipalKrw: 100_000_000,
        postExecutionBalanceKrw: 80_000_000,
      });

      expect(result.success).toBe(true);
    });

    it('given 기존 잔액보다 큰 대체 원금, when 견적 result를 검증하면, then 잔액 모순을 거절한다', () => {
      const quote = givenLeaseDepositLoanQuoteResult();

      const result = HousingLeaseDepositLoanQuoteResultSchema.safeParse({
        ...quote,
        existingLoanBalanceKrw: 90_000_000,
        replacedLoanId: '40',
        replacedLoanPrincipalKrw: 100_000_000,
        postExecutionBalanceKrw: 70_000_000,
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: snapshot에 활성 전세 계약을 공개하는 경우', () => {
    it('given 일치하는 residence와 보증금 자산, when life snapshot을 검증하면, then 허용한다', () => {
      const life = givenJeonseLifeSnapshot();

      const result = LifeSnapshotSchema.safeParse(life);

      expect(result.success).toBe(true);
    });

    it('given 전세자금대출이 연결된 전세 계약, when 현재 계약을 검증하면, then loan ID를 허용한다', () => {
      const response = givenCurrentLeaseResponse();

      const result = HousingCurrentLeaseResponseSchema.safeParse({
        ...response,
        activeLease: { ...response.activeLease, depositLoanId: '40' },
      });

      expect(result.success).toBe(true);
    });

    it('given 대출 직접 지급과 기존 대출 payoff, when result를 검증하면, then financed wallet delta를 허용한다', () => {
      const response = givenLeaseResult();

      const result = HousingLeaseResultSchema.safeParse({
        ...response,
        walletDeltaKrw: 51_550_000,
        depositLoanExecution: {
          loanId: '41',
          quoteId: '31',
          productVersionId: '22',
          principalKrw: 80_000_000,
          annualRateBp: 400,
          maturityGameDay: 850,
          firstInstallment: {
            dueGameDay: 151,
            feeKrw: 0,
            principalKrw: 0,
            interestKrw: 271_232,
            totalKrw: 271_232,
          },
        },
        repaidDepositLoan: {
          loanId: '40',
          paymentId: '51',
          principalKrw: 100_000_000,
        },
      });

      expect(result.success).toBe(true);
    });

    it('given 다른 지역의 residence, when life snapshot을 검증하면, then 상관관계 위반을 거절한다', () => {
      const life = givenJeonseLifeSnapshot();

      const result = LifeSnapshotSchema.safeParse({
        ...life,
        residence: { ...life.residence, regionKey: 'rural' },
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 월세 capability와 활성 계약을 공개하는 경우', () => {
    it('given 월세 terms와 canonical 연체 window, when 현재 계약을 검증하면, then 허용한다', () => {
      const response = givenMonthlyCurrentLeaseResponse();

      const result = HousingCurrentLeaseResponseSchema.safeParse(response);

      expect(result.success).toBe(true);
    });

    it('given 월세인데 next due가 null, when 활성 계약을 검증하면, then tagged shape 위반을 거절한다', () => {
      const response = givenMonthlyCurrentLeaseResponse();

      const result = HousingCurrentLeaseResponseSchema.safeParse({
        ...response,
        activeLease: { ...response.activeLease, nextRentDueGameDay: null },
      });

      expect(result.success).toBe(false);
    });

    it('given 전세자금대출이 연결된 월세 계약, when 활성 계약을 검증하면, then tagged shape 위반을 거절한다', () => {
      const response = givenMonthlyCurrentLeaseResponse();

      const result = HousingCurrentLeaseResponseSchema.safeParse({
        ...response,
        activeLease: { ...response.activeLease, depositLoanId: '40' },
      });

      expect(result.success).toBe(false);
    });

    it('given 오래된 순서가 뒤집힌 연체, when 현재 계약을 검증하면, then canonical window를 거절한다', () => {
      const response = givenMonthlyCurrentLeaseResponse();
      const older = givenLeaseArrear();
      const newer = {
        ...givenLeaseArrear(),
        id: '8102',
        rentChargeId: '8052',
        dueYearMonth: { year: 2026, month: 3 },
        createdGameDay: 60,
      };

      const result = HousingCurrentLeaseResponseSchema.safeParse({
        ...response,
        activeArrears: [newer, older],
        totalLeaseArrearKrw: newer.remainingKrw + older.remainingKrw,
      });

      expect(result.success).toBe(false);
    });

    it('given 일부 window인데 20건보다 적은 연체, when 현재 계약을 검증하면, then bounded 표기를 거절한다', () => {
      const response = givenMonthlyCurrentLeaseResponse();

      const result = HousingCurrentLeaseResponseSchema.safeParse({
        ...response,
        hasMoreActiveArrears: true,
        totalLeaseArrearKrw: response.totalLeaseArrearKrw + 1,
      });

      expect(result.success).toBe(false);
    });

    it('given 오래된 20건과 더 큰 total, when 일부 window를 검증하면, then 허용한다', () => {
      const response = givenMonthlyCurrentLeaseResponse();
      const activeArrears = Array.from({ length: 20 }, (_, index) => ({
        id: String(8_101 + index),
        leaseId: '8002',
        rentChargeId: String(8_051 + index),
        dueYearMonth: {
          year: 2025 + Math.floor(index / 12),
          month: (index % 12) + 1,
        },
        originalKrw: 1,
        paidKrw: 0,
        remainingKrw: 1,
        createdGameDay: index + 1,
      }));

      const result = HousingCurrentLeaseResponseSchema.safeParse({
        ...response,
        activeArrears,
        hasMoreActiveArrears: true,
        totalLeaseArrearKrw: 21,
      });

      expect(result.success).toBe(true);
    });
  });

  describe('맥락: snapshot에 활성 월세 계약과 체납을 공개하는 경우', () => {
    it('given monthlyRent residence와 같은 연체 window, when life snapshot을 검증하면, then 허용한다', () => {
      const life = givenMonthlyLifeSnapshot();

      const result = LifeSnapshotSchema.safeParse(life);

      expect(result.success).toBe(true);
    });

    it('given 월세 lease에 전세 residence, when life snapshot을 검증하면, then tenure 상관관계를 거절한다', () => {
      const life = givenMonthlyLifeSnapshot();

      const result = LifeSnapshotSchema.safeParse({
        ...life,
        residence: { ...life.residence, tenureKind: 'jeonse' },
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 월세 연체를 수동 상환하는 경우', () => {
    it('given 공통 cursor와 양의 금액, when 요청과 receipt를 검증하면, then payment ID를 포함해 허용한다', () => {
      const request = HousingLeaseArrearPaymentRequestSchema.safeParse({
        commandId: '00000000-0000-4000-8000-000000000002',
        expectedRunRevision: 3,
        expectedStateRevision: 44,
        expectedGameDay: 62,
        amountKrw: 200_000,
      });
      const result = HousingLeaseArrearPaymentResultSchema.safeParse({
        arrearId: '8101',
        paymentId: '8201',
        paidKrw: 200_000,
        remainingKrw: 300_000,
      });

      expect([request.success, result.success]).toEqual([true, true]);
    });
  });
});

describe('주거 M4-C3 매수와 주택담보대출 protocol 계약', () => {
  describe('맥락: 주택담보대출 전용 견적을 요청하는 경우', () => {
    it('given 공통 cursor와 매물·상품·원금, when 요청을 검증하면, then 최소 strict body를 허용한다', () => {
      const result = HousingMortgageQuoteRequestSchema.safeParse(givenMortgageQuoteRequest());

      expect(result.success).toBe(true);
    });

    it('given client가 LTV를 보낸 견적, when 요청을 검증하면, then 서버 권위 밖 필드를 거절한다', () => {
      const result = HousingMortgageQuoteRequestSchema.safeParse({
        ...givenMortgageQuoteRequest(),
        ltvLimitPpm: 700_000,
      });

      expect(result.success).toBe(false);
    });

    it('given exact 매매가의 LTV·DSR·자기자금 근거, when result를 검증하면, then eligible 견적을 허용한다', () => {
      const result = HousingMortgageQuoteResultSchema.safeParse(givenMortgageQuoteResult());

      expect(result.success).toBe(true);
    });

    it('given 새 policy가 반환한 다른 양의 부대비용, when 자기자금 근거가 일치하면, then client 재계산 없이 허용한다', () => {
      const quote = givenMortgageQuoteResult();
      const result = HousingMortgageQuoteResultSchema.safeParse({
        ...quote,
        acquisitionIncidentalCostKrw: quote.acquisitionIncidentalCostKrw + 1,
        requiredBuyerCashKrw: quote.requiredBuyerCashKrw + 1,
      });

      expect(result.success).toBe(true);
    });

    it('given 올림한 LTV ratio, when result를 검증하면, then 정수 내림 근거 위반을 거절한다', () => {
      const quote = givenMortgageQuoteResult();
      const result = HousingMortgageQuoteResultSchema.safeParse({
        ...quote,
        ltv: { ...quote.ltv, ratioPpm: quote.ltv.ratioPpm + 1 },
      });

      expect(result.success).toBe(false);
    });

    it('given 총 매수비용보다 큰 product-valid 원금, when 담보한도 초과 result를 검증하면, then 필요 자기자금을 0원으로 허용한다', () => {
      const quote = givenMortgageQuoteResult();
      const requestedPrincipalKrw = 600_000_000;
      const result = HousingMortgageQuoteResultSchema.safeParse({
        ...quote,
        requestedPrincipalKrw,
        ltv: {
          ...quote.ltv,
          numeratorKrw: requestedPrincipalKrw,
          ratioPpm: 1_200_000,
        },
        postExecutionBalanceKrw: requestedPrincipalKrw,
        requiredBuyerCashKrw: 0,
        decisionCode: 'collateralLimit',
        decisionReasons: ['collateralLimit'],
      });

      expect(result.success).toBe(true);
    });
  });

  describe('맥락: 매수 capability와 active holding을 조회하는 경우', () => {
    it('given compatibility run의 unavailable projection, when 검증하면, then 빈 보유주택을 허용한다', () => {
      const result = HousingPropertyHoldingsResponseSchema.safeParse({
        purchaseCapability: 'unavailable',
        maximumActiveHoldings: 0,
        holdings: [],
        totalPropertyBookValueKrw: 0,
      });

      expect(result.success).toBe(true);
    });

    it('given 단일 owner-occupied holding과 mortgage lien, when 검증하면, then 취득가 총액을 허용한다', () => {
      const holding = givenPropertyHolding();
      const result = HousingPropertyHoldingsResponseSchema.safeParse({
        purchaseCapability: 'ownerOccupiedSingleHome',
        maximumActiveHoldings: 1,
        holdings: [holding],
        totalPropertyBookValueKrw: holding.bookValueKrw,
      });

      expect(result.success).toBe(true);
    });

    it('given 취득가와 다른 장부가, when 검증하면, then 숨은 평가이익을 거절한다', () => {
      const holding = givenPropertyHolding();
      const result = HousingPropertyHoldingsResponseSchema.safeParse({
        purchaseCapability: 'ownerOccupiedSingleHome',
        maximumActiveHoldings: 1,
        holdings: [{ ...holding, bookValueKrw: holding.bookValueKrw + 1 }],
        totalPropertyBookValueKrw: holding.bookValueKrw + 1,
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: 현금 또는 주담대로 주택을 매수하는 경우', () => {
    it('given nullable mortgage quote ID, when 두 요청을 검증하면, then cash와 financed tagged body를 허용한다', () => {
      const cash = HousingPurchaseRequestSchema.safeParse({
        ...givenPurchaseRequest(),
        mortgageQuoteId: null,
      });
      const financed = HousingPurchaseRequestSchema.safeParse({
        ...givenPurchaseRequest(),
        mortgageQuoteId: '9301',
      });

      expect([cash.success, financed.success]).toEqual([true, true]);
    });

    it('given direct mortgage funding과 기존 전세대출 payoff, when result를 검증하면, then wallet 분개를 허용한다', () => {
      const result = HousingPurchaseResultSchema.safeParse(givenPurchaseResult());

      expect(result.success).toBe(true);
    });

    it('given mortgage 원금을 지갑에 한 번 더 더한 delta, when result를 검증하면, then 이중 계상을 거절한다', () => {
      const purchase = givenPurchaseResult();
      const result = HousingPurchaseResultSchema.safeParse({
        ...purchase,
        walletDeltaKrw: purchase.walletDeltaKrw + (purchase.mortgageExecution?.principalKrw ?? 0),
      });

      expect(result.success).toBe(false);
    });
  });

  describe('맥락: snapshot에 owner residence와 mortgage를 투영하는 경우', () => {
    it('given holding·residence·loan이 서로 연결된 snapshot, when 검증하면, then 장부가와 부채를 허용한다', () => {
      const result = LifeSnapshotSchema.safeParse(givenOwnerLifeSnapshot());

      expect(result.success).toBe(true);
    });

    it('given 다른 holding을 가리키는 mortgage loan, when 검증하면, then lien 상관관계를 거절한다', () => {
      const life = givenOwnerLifeSnapshot();
      const result = LifeSnapshotSchema.safeParse({
        ...life,
        activePropertyHoldings: [{ ...life.activePropertyHoldings[0], mortgageLoanId: '9999' }],
      });

      expect(result.success).toBe(false);
    });
  });
});

function givenActiveLease() {
  return {
    id: '8001',
    listingId: '7001',
    depositLoanId: null,
    role: 'tenant',
    offerKind: 'jeonse',
    regionKey: 'metropolitan',
    propertyType: 'apartment',
    exclusiveAreaSquareMeters: 84,
    depositKrw: 252_000_000,
    monthlyRentKrw: null,
    nextRentDueGameDay: null,
    effectiveFromGameDay: 120,
    effectiveToGameDay: null,
    renewalRule: 'openEnded',
    currentTerm: null,
    renewalNotice: null,
    terminationReview: null,
  };
}

function givenCurrentLeaseResponse() {
  return {
    leaseCapability: 'cashJeonse',
    renewalRule: 'openEnded',
    leaseLifecycleTerms: null,
    movingCosts: [
      { regionKey: 'capitalArea', movingCostKrw: 800_000 },
      { regionKey: 'metropolitan', movingCostKrw: 600_000 },
      { regionKey: 'smallCity', movingCostKrw: 450_000 },
      { regionKey: 'rural', movingCostKrw: 300_000 },
    ],
    tenantLeaseDepositKrw: 252_000_000,
    activeLease: givenActiveLease(),
    monthlyRentTerms: null,
    activeArrears: [],
    hasMoreActiveArrears: false,
    totalLeaseArrearKrw: 0,
  };
}

function givenLeaseRequest() {
  return {
    commandId: '00000000-0000-4000-8000-000000000001',
    expectedRunRevision: 2,
    expectedStateRevision: 9,
    expectedGameDay: 120,
    listingId: '7002',
    offerKind: 'jeonse',
  };
}

function givenMortgageQuoteRequest() {
  return {
    commandId: '00000000-0000-4000-8000-000000000031',
    expectedRunRevision: 4,
    expectedStateRevision: 12,
    expectedGameDay: 121,
    listingId: '7003',
    productVersionId: '23',
    principalKrw: 300_000_000,
  };
}

function givenPurchaseRequest() {
  return {
    commandId: '00000000-0000-4000-8000-000000000032',
    expectedRunRevision: 4,
    expectedStateRevision: 12,
    expectedGameDay: 121,
    listingId: '7003',
  };
}

function givenMortgageQuoteResult() {
  return {
    quoteId: '9301',
    listingId: '7003',
    productVersionId: '23',
    requestedPrincipalKrw: 300_000_000,
    purchasePriceKrw: 500_000_000,
    recognizedCollateralValueKrw: 500_000_000,
    ltvRegionClass: 'nonRegulatedProxy',
    ltvLimitPpm: 700_000,
    maximumMortgageKrw: 350_000_000,
    ltv: {
      numeratorKrw: 300_000_000,
      denominatorKrw: 500_000_000,
      ratioPpm: 600_000,
      limitPpm: 700_000,
    },
    createdGameDay: 121,
    expiresGameDay: 121,
    decisionCode: 'eligible',
    decisionReasons: ['eligible'],
    verifiedAnnualIncomeKrw: 100_000_000,
    verifiedIncomeSource: 'activeEmploymentContract',
    existingLoanBalanceKrw: 100_000_000,
    postExecutionBalanceKrw: 300_000_000,
    dsrApplied: true,
    dsr: {
      numeratorKrw: 30_000_000,
      denominatorKrw: 100_000_000,
      ratioPpm: 300_000,
      limitPpm: 400_000,
    },
    stressRateBp: 0,
    stressTreatment: 'fullTermFixed',
    acquisitionIncidentalCostKrw: 5_000_000,
    movingCostKrw: 450_000,
    returnedDepositKrw: 252_000_000,
    replacedLoanId: '40',
    replacedLoanPrincipalKrw: 100_000_000,
    availableBuyerCashKrw: 352_000_000,
    requiredBuyerCashKrw: 205_450_000,
    quotedTerms: {
      annualRateBp: 400,
      repaymentMethod: 'levelPayment',
      termMonths: 360,
      firstInstallment: {
        dueGameDay: 151,
        feeKrw: 0,
        principalKrw: 550_000,
        interestKrw: 1_000_000,
        totalKrw: 1_550_000,
      },
    },
  };
}

function givenPropertyHolding() {
  return {
    id: '9401',
    listingId: '7003',
    status: 'active',
    purpose: 'ownerOccupied',
    regionKey: 'smallCity',
    propertyType: 'apartment',
    exclusiveAreaSquareMeters: 84,
    acquiredGameDay: 121,
    acquisitionPriceKrw: 500_000_000,
    acquisitionIncidentalCostKrw: 5_000_000,
    bookValueKrw: 500_000_000,
    mortgageLoanId: '41',
  };
}

function givenPurchaseResult() {
  return {
    holding: givenPropertyHolding(),
    residenceId: '9501',
    listingId: '7003',
    purchasePriceKrw: 500_000_000,
    acquisitionIncidentalCostKrw: 5_000_000,
    movingCostKrw: 450_000,
    returnedDepositKrw: 252_000_000,
    walletDeltaKrw: -53_450_000,
    effectiveFromGameDay: 121,
    endedLeaseId: '8001',
    repaidDepositLoan: {
      loanId: '40',
      paymentId: '51',
      principalKrw: 100_000_000,
    },
    mortgageExecution: {
      loanId: '41',
      quoteId: '9301',
      productVersionId: '23',
      propertyHoldingId: '9401',
      principalKrw: 300_000_000,
      activatedGameDay: 121,
      maturityGameDay: 11_079,
      annualRateBp: 400,
      repaymentMethod: 'levelPayment',
      termMonths: 360,
      firstInstallment: {
        dueGameDay: 151,
        feeKrw: 0,
        principalKrw: 550_000,
        interestKrw: 1_000_000,
        totalKrw: 1_550_000,
      },
    },
  };
}

function givenOwnerLifeSnapshot() {
  const holding = givenPropertyHolding();
  return {
    rateStatus: 'active',
    household: {
      id: '1',
      memberCount: 1,
      dependentCount: 0,
      taxDependentEligibleCount: 0,
    },
    residence: {
      id: '9501',
      regionKey: 'smallCity',
      tenureKind: 'owner',
      propertyHoldingId: holding.id,
      effectiveFromGameDay: 121,
    },
    tenantLeaseDepositKrw: 0,
    activeLease: null,
    activeLeaseArrears: [],
    hasMoreActiveLeaseArrears: false,
    totalLeaseArrearKrw: 0,
    activePropertyHoldings: [holding],
    hasMoreActivePropertyHoldings: false,
    totalPropertyBookValueKrw: holding.bookValueKrw,
    currentMonth: null,
    activeArrears: [],
    hasMoreActiveArrears: false,
    totalEssentialArrearKrw: 0,
    creditBand: 'standard',
    creditReasons: ['cleanHistory'],
    activeLoans: [
      {
        id: '41',
        productVersionId: '23',
        productKind: 'mortgage',
        displayName: '개발 주택담보 고정금리 대출',
        rateStatus: 'available',
        currentAnnualRateBp: 400,
        status: 'active',
        remainingPrincipalKrw: 300_000_000,
        overdueKrw: 0,
        readOnly: false,
      },
    ],
    nextLoanInstallment: null,
    totalLoanBalanceKrw: 300_000_000,
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
  };
}

function givenActiveInsuranceContract(id: string) {
  return {
    id,
    productVersionId: '71',
    productKey: 'fictionalFamilyCareCover',
    displayName: '가족 돌봄 비용 보장',
    status: 'active',
    coverageStartGameDay: 0,
    waitingEndsGameDay: 7,
    coverageEndExclusive: 360,
    nextPremiumDueGameDay: 30,
    premiumKrw: 10_000,
    paidBenefitKrw: 0,
    reservedBenefitKrw: 0,
    remainingBenefitKrw: 200_000,
  };
}

function givenLeaseResult() {
  return {
    leaseId: '8002',
    residenceId: '9002',
    listingId: '7002',
    offerKind: 'jeonse',
    regionKey: 'smallCity',
    propertyType: 'multiFamily',
    exclusiveAreaSquareMeters: 59,
    depositKrw: 180_000_000,
    monthlyRentKrw: null,
    returnedDepositKrw: 252_000_000,
    movingCostKrw: 450_000,
    walletDeltaKrw: 71_550_000,
    effectiveFromGameDay: 121,
    endedLeaseId: '8001',
    renewalRule: 'openEnded',
    depositLoanExecution: null,
    repaidDepositLoan: null,
  };
}

function givenJeonseLifeSnapshot() {
  return {
    rateStatus: 'active',
    household: {
      id: '1',
      memberCount: 1,
      dependentCount: 0,
      taxDependentEligibleCount: 0,
    },
    residence: {
      id: '9001',
      regionKey: 'metropolitan',
      tenureKind: 'jeonse',
      propertyHoldingId: null,
      effectiveFromGameDay: 120,
    },
    tenantLeaseDepositKrw: 252_000_000,
    activeLease: givenActiveLease(),
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
    creditBand: 'standard',
    creditReasons: ['cleanHistory'],
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
  };
}

function givenMonthlyActiveLease() {
  return {
    id: '8002',
    listingId: '7002',
    depositLoanId: null,
    role: 'tenant',
    offerKind: 'monthlyRent',
    regionKey: 'smallCity',
    propertyType: 'multiFamily',
    exclusiveAreaSquareMeters: 59,
    depositKrw: 20_000_000,
    monthlyRentKrw: 650_000,
    nextRentDueGameDay: 90,
    effectiveFromGameDay: 45,
    effectiveToGameDay: null,
    renewalRule: 'openEnded',
    currentTerm: null,
    renewalNotice: null,
    terminationReview: null,
  };
}

function givenLeaseArrear() {
  return {
    id: '8101',
    leaseId: '8002',
    rentChargeId: '8051',
    dueYearMonth: { year: 2026, month: 2 },
    originalKrw: 650_000,
    paidKrw: 150_000,
    remainingKrw: 500_000,
    createdGameDay: 59,
  };
}

function givenMonthlyCurrentLeaseResponse() {
  return {
    leaseCapability: 'cashJeonseAndMonthlyRent',
    renewalRule: 'openEnded',
    leaseLifecycleTerms: null,
    movingCosts: [
      { regionKey: 'capitalArea', movingCostKrw: 800_000 },
      { regionKey: 'metropolitan', movingCostKrw: 600_000 },
      { regionKey: 'smallCity', movingCostKrw: 450_000 },
      { regionKey: 'rural', movingCostKrw: 300_000 },
    ],
    tenantLeaseDepositKrw: 20_000_000,
    activeLease: givenMonthlyActiveLease(),
    monthlyRentTerms: {
      rentChargeRule: 'nextMonthStartFull',
      arrearRepaymentRule: 'manualOnly',
    },
    activeArrears: [givenLeaseArrear()],
    hasMoreActiveArrears: false,
    totalLeaseArrearKrw: 500_000,
  };
}

function givenFixedTermMonthlyCurrentLeaseResponse() {
  const response = givenMonthlyCurrentLeaseResponse();
  return {
    ...response,
    renewalRule: 'fixedTermAutoRenew',
    leaseLifecycleTerms: {
      termMonths: 12,
      renewalNoticeLeadDays: 30,
      monthlyRentTerminationReview: {
        rule: 'oldestActiveArrearAge',
        afterGameDays: 60,
      },
    },
    activeLease: {
      ...response.activeLease,
      renewalRule: 'fixedTermAutoRenew',
      currentTerm: {
        termNo: 1,
        effectiveFromGameDay: 45,
        effectiveToGameDay: 410,
      },
      renewalNotice: {
        termNo: 1,
        publishedGameDay: 380,
        renewsOnGameDay: 410,
      },
      terminationReview: {
        status: 'underReview',
        openedGameDay: 105,
        triggerArrearId: '8101',
        activeLeaseArrearKrw: 500_000,
      },
    },
  };
}

function givenMonthlyLifeSnapshot() {
  return {
    ...givenJeonseLifeSnapshot(),
    residence: {
      id: '9002',
      regionKey: 'smallCity',
      tenureKind: 'monthlyRent',
      propertyHoldingId: null,
      effectiveFromGameDay: 45,
    },
    tenantLeaseDepositKrw: 20_000_000,
    activeLease: givenMonthlyActiveLease(),
    activeLeaseArrears: [givenLeaseArrear()],
    totalLeaseArrearKrw: 500_000,
  };
}

function givenLifeBudgetSelections() {
  return [
    'housing',
    'food',
    'transport',
    'communication',
    'utilities',
    'healthcare',
    'education',
    'dependentCare',
    'discretionary',
  ].map((category) => ({ category, bandId: '11' }));
}

function givenActiveLifeBudgetResponse() {
  return {
    rateStatus: 'active',
    household: {
      id: '1',
      memberCount: 2,
      dependentCount: 1,
      taxDependentEligibleCount: 1,
    },
    residence: {
      id: '2',
      regionKey: 'capitalArea',
      tenureKind: 'rentFree',
      propertyHoldingId: null,
      effectiveFromGameDay: 0,
    },
    allowedBands: [
      { id: '10', bandKey: 'frugal', displayName: '절약', factorPpm: 850_000 },
      { id: '11', bandKey: 'standard', displayName: '표준', factorPpm: 1_000_000 },
      { id: '12', bandKey: 'generous', displayName: '여유', factorPpm: 1_250_000 },
    ],
    selections: givenLifeBudgetSelections(),
    currentMonth: givenLivingCostMonth(),
    activeArrears: [],
    hasMoreActiveArrears: false,
    totalEssentialArrearKrw: 0,
  };
}

function givenLivingCostMonth() {
  const baseAmounts = [
    450_000, 350_000, 120_000, 60_000, 100_000, 70_000, 50_000, 120_000, 180_000,
  ];
  const essential = [true, true, true, true, true, true, false, true, false];
  return {
    id: '20',
    profileId: '5',
    profileKey: 'dev-unranked-m4-life-2026-v1',
    currentCpiIndex: 1_000_000,
    prorationScale: 377_580,
    prorationUnits: 377_580,
    prorationDays: 31,
    daysInMonth: 31,
    yearMonth: { year: 2026, month: 1 },
    activationGameDay: 0,
    settlementGameDay: 30,
    settled: false,
    totalGrossKrw: 0,
    totalPaidKrw: 0,
    totalArrearKrw: 0,
    items: givenLifeBudgetSelections().map((selection, index) => ({
      ...selection,
      essential: essential[index],
      baseMonthlyKrw: baseAmounts[index],
      baseCpiIndex: 1_000_000,
      regionFactorPpm: 1_000_000,
      householdFactorPpm: 1_000_000,
      budgetFactorPpm: 1_000_000,
      tenureReplacementFactorPpm: selection.category === 'housing' ? 0 : 1_000_000,
      grossKrw: 0,
      paidKrw: 0,
      arrearKrw: 0,
    })),
  };
}

function givenMilitaryOption() {
  return {
    id: '7',
    optionKey: 'active-duty-v1',
    serviceType: 'activeDuty' as const,
    displayName: '현역',
    eligible: true,
    ineligibilityReasons: [],
    serviceDurationMonths: 18,
    hardRequirements: {
      minimumEducation: null,
      requiredCertificationCount: 0,
      minimumExperienceDays: 0,
    },
    compensationKind: 'militaryPay' as const,
    paySchedule: 'monthly' as const,
    payStages: [
      { startServiceMonth: 0, endExclusiveServiceMonth: 2, grossMonthlyPayKrw: 750_000 },
      { startServiceMonth: 2, endExclusiveServiceMonth: 8, grossMonthlyPayKrw: 900_000 },
      { startServiceMonth: 8, endExclusiveServiceMonth: 14, grossMonthlyPayKrw: 1_200_000 },
      { startServiceMonth: 14, endExclusiveServiceMonth: 18, grossMonthlyPayKrw: 1_500_000 },
    ],
    effortLifeStatus: 'activeDuty' as const,
    dailyEffortCapacityUnits: 1,
    grantsCareerExperience: false,
    experienceCredits: [],
  };
}

function givenMilitaryServiceResponse() {
  return {
    militaryStatus: 'serving' as const,
    service: {
      id: '3',
      optionVersionId: '7',
      serviceType: 'activeDuty' as const,
      displayName: '현역',
      status: 'serving' as const,
      sourceKind: 'userCommand' as const,
      startGameDay: 1,
      endGameDay: 547,
      startDate: '2026-01-02',
      endExclusiveDate: '2027-07-02',
      creditedServiceDays: 10,
      totalServiceDays: 546,
      effortLifeStatus: 'activeDuty' as const,
      grantsCareerExperience: false,
      nextPayGameDay: 31,
      completedGameDay: null,
    },
  };
}

function givenMilitaryCareerSnapshot() {
  return {
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
    currentEmploymentTaxYear: givenOpenEmploymentTaxYear(),
    latestEmploymentTaxAssessment: null,
    militaryStatus: 'serving' as const,
    activeMilitaryService: {
      id: '3',
      optionVersionId: '7',
      serviceType: 'activeDuty' as const,
      displayName: '현역',
      status: 'serving' as const,
      startGameDay: 1,
      endGameDay: 547,
      creditedServiceDays: 10,
      totalServiceDays: 546,
      effortLifeStatus: 'activeDuty' as const,
      grantsCareerExperience: false,
      nextPayGameDay: 31,
    },
    activeMilitarySavings: [
      {
        id: '11',
        productVersionId: '9',
        institutionKey: 'life-bank-a',
        status: 'active' as const,
        monthlyContributionKrw: 250_000,
        debitDayOfMonth: 25,
        principalKrw: 250_000,
        paidInstallmentCount: 1,
        missedInstallmentCount: 0,
        nextInstallmentGameDay: 85,
        maturityGameDay: 547,
      },
    ],
    pendingCareerSchedule: [
      {
        sourceKind: 'careerAction' as const,
        id: '20',
        dueGameDay: 31,
        kind: 'documentReview' as const,
      },
      {
        sourceKind: 'settlement' as const,
        id: '21',
        dueGameDay: 31,
        kind: 'militaryPay' as const,
      },
    ],
  };
}

function givenMilitarySavingsProduct() {
  return {
    id: '9',
    productKey: 'life-bank-a-soldier-savings-v1',
    institutionKey: 'life-bank-a',
    institutionDisplayName: '라이프은행 A',
    eligible: true,
    ineligibilityReasons: [],
    eligibleServiceTypes: ['activeDuty', 'socialService'],
    joinStartDate: '2026-01-01',
    joinEndDate: '2026-12-31',
    minimumRemainingServiceMonths: 1,
    maximumActiveContracts: 2,
    maximumContractsPerInstitution: 1,
    minimumMonthlyContributionKrw: 1_000,
    maximumInstitutionMonthlyContributionKrw: 300_000,
    maximumTotalMonthlyContributionKrw: 550_000,
    limitSettingUnitKrw: 50_000,
    installmentUnitKrw: 1,
    interestTiers: [
      { minimumTermMonths: 1, maximumTermMonthsInclusive: 11, annualInterestRatePpm: 40_000 },
      { minimumTermMonths: 12, maximumTermMonthsInclusive: 14, annualInterestRatePpm: 45_000 },
      { minimumTermMonths: 15, maximumTermMonthsInclusive: 24, annualInterestRatePpm: 50_000 },
    ],
    dayCountConvention: 'actual365' as const,
    interestRounding: 'floorToKrw' as const,
    earlyCloseAnnualInterestRatePpm: 0,
    governmentMatchingRatePpm: 1_000_000,
    governmentMatchPaymentDayOfMonth: 25,
    maturityTaxExempt: true,
  };
}

function givenActiveMilitarySavingsHistory() {
  return {
    id: '11',
    serviceId: '3',
    productVersionId: '9',
    productKey: 'life-bank-a-soldier-savings-v1',
    institutionKey: 'life-bank-a',
    institutionDisplayName: '라이프은행 A',
    status: 'active' as const,
    monthlyContributionKrw: 250_000,
    debitDayOfMonth: 25,
    principalKrw: 250_000,
    paidInstallmentCount: 1,
    missedInstallmentCount: 0,
    nextInstallmentGameDay: 85,
    maturityGameDay: 366,
    openedGameDay: 10,
    firstInstallmentGameDay: 55,
    contractTermMonths: 18,
    annualInterestRatePpm: 50_000,
    closedGameDay: null,
    closureReason: null,
    settledPrincipalKrw: 0,
    grossBankInterestKrw: 0,
    governmentMatchKrw: 0,
    bankPayoutKrw: 0,
    governmentMatchPaidGameDay: null,
    projectedMaturity: {
      assumption: 'allScheduledInstallmentsPaid' as const,
      principalKrw: 4_500_000,
      grossBankInterestKrw: 100_000,
      governmentMatchKrw: 4_500_000,
      bankPayoutKrw: 4_600_000,
      totalBenefitKrw: 9_100_000,
    },
    installments: [
      {
        id: '21',
        installmentNo: 1,
        dueGameDay: 55,
        status: 'paid' as const,
        paidGameDay: 55,
        principalKrw: 250_000,
        governmentMatchingPolicyVersionId: '4',
        governmentMatchingRatePpm: 1_000_000,
      },
    ],
  };
}
