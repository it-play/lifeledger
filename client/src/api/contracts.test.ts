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
  CareerSnapshotSchema,
  CareerSpecsResponseSchema,
  CashContractSummarySchema,
  type CashProduct,
  CashProductCatalogSchema,
  FinanceFailureCodeSchema,
  FinanceSnapshotSchema,
  type FinancialIncomeYear,
  FinancialIncomeYearSchema,
  GoldProductCatalogSchema,
  GoldWithdrawalResultSchema,
  IsaAccountSummarySchema,
  type LedgerPage,
  LedgerPageSchema,
  LedgerSourceKindSchema,
  type MarketHistory,
  MarketHistorySchema,
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
            region: '서울',
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
