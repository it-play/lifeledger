import { z } from 'zod';

/**
 * The server contract. Hand-written for now; once the server emits OpenAPI this file is
 * replaced by generated code, keeping the type names below.
 */

export const HealthSchema = z.object({
  status: z.literal('ok'),
  version: z.string(),
});

export const GameSpeedSchema = z.union([z.literal(1), z.literal(2), z.literal(4), z.literal(8)]);

export const ResourceIdSchema = z
  .string()
  .regex(/^[1-9][0-9]*$/, 'resource ID must be a canonical positive decimal')
  .refine((value) => BigInt(value) <= 18_446_744_073_709_551_615n, 'resource ID exceeds u64');

export const CanonicalUuidSchema = z
  .string()
  .regex(
    /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/,
    'command ID must be a canonical UUID',
  );

export const MarketRegimeSchema = z.enum(['expansion', 'slowdown', 'recession', 'recovery']);

export const MarketRatesSchema = z.object({
  policyRateBp: z.number().int().min(0).max(800),
  treasury3mBp: z.number().int().min(0).max(1500),
  treasury1yBp: z.number().int().min(0).max(1500),
  treasury3yBp: z.number().int().min(0).max(1500),
  treasury10yBp: z.number().int().min(0).max(1500),
});

export const M2MarketFactorsSchema = z
  .object({
    cpiIndex: z.number().int().safe().positive(),
    llxCloseKrw: z.number().int().safe().positive(),
    goldCloseKrwPerGram: z.number().int().safe().positive(),
  })
  .strict();

export const MarketSnapshotSchema = z.object({
  world: z.string(),
  date: z.string(),
  open: z.boolean(),
  regime: MarketRegimeSchema,
  index: z.object({
    symbol: z.string(),
    name: z.string(),
    closeKrw: z.number().int().positive(),
    dailyReturnPpm: z.number().int(),
  }),
  rates: MarketRatesSchema.nullable(),
  m2Factors: M2MarketFactorsSchema.nullable(),
});

export const PortfolioPositionSchema = z.object({
  accountId: ResourceIdSchema,
  symbol: z.literal('LLX'),
  quantity: z.number().int().min(1).max(1_000_000),
  costBasisKrw: z.number().int().nonnegative(),
  averagePriceKrw: z.number().int().nonnegative(),
  currentPriceKrw: z.number().int().positive(),
  marketValueKrw: z.number().int().nonnegative(),
});

export const PortfolioSnapshotSchema = z.object({
  positions: z.array(PortfolioPositionSchema),
  marketValueKrw: z.number().int().nonnegative(),
});

export const FinancialAccountTypeSchema = z.enum([
  'taxableBrokerage',
  'cma',
  'isaGeneral',
  'isaLowIncome',
  'pensionSavings',
  'irp',
  'krxGold',
]);

export const FinancialAccountStatusSchema = z.enum(['open', 'matured', 'closed']);

export const FinancialAccountSchema = z.object({
  id: ResourceIdSchema,
  type: FinancialAccountTypeSchema,
  status: FinancialAccountStatusSchema,
  cashKrw: z.number().int().nonnegative(),
  isDefault: z.boolean(),
});

export const PolicySetSummarySchema = z.object({
  key: z.string().min(1),
  basisDate: z.iso.date(),
});

export const SettlementKindSchema = z.enum([
  'cmaInterest',
  'depositMaturity',
  'savingsInstallment',
  'savingsMaturity',
  'bondCoupon',
  'bondMaturity',
  'llxDistribution',
  'financialIncomeFiling',
  'employmentPayroll',
  'employmentReconciliation',
  'militaryPay',
  'militarySavingsInstallment',
  'militarySavingsMaturity',
  'militarySavingsGovernmentMatch',
  'loanInstallment',
  'leaseRent',
  'livingCostMonth',
  'propertyTaxPayment',
  'welfareBenefitPayment',
  'insurancePremium',
]);

export const PendingSettlementSummarySchema = z.object({
  id: ResourceIdSchema,
  dueGameDay: z.number().int().nonnegative(),
  kind: SettlementKindSchema,
});

const NonnegativeKrwSchema = z.number().int().safe().nonnegative();
const PositiveKrwSchema = z.number().int().safe().positive();

export const CashProductKindSchema = z.enum([
  'cmaRp',
  'cmaIssuedNote',
  'termDeposit',
  'installmentSavings',
]);

export const DepositKindSchema = z.enum(['termDeposit', 'installmentSavings']);

export const CashRateReferenceSchema = z.literal('treasury3mBp');

export const FinancialInstitutionSummarySchema = z.object({
  id: ResourceIdSchema,
  key: z.string().min(1).max(64),
  displayName: z.string().min(1).max(100),
});

export const CashProductSchema = z
  .object({
    id: ResourceIdSchema,
    key: z.string().min(1).max(64),
    kind: CashProductKindSchema,
    displayName: z.string().min(1).max(100),
    institution: FinancialInstitutionSummarySchema,
    protectionEligible: z.boolean(),
    rateReference: CashRateReferenceSchema,
    spreadBp: z.number().int().min(-10_000).max(10_000),
    minimumInterestBalanceKrw: PositiveKrwSchema.optional(),
    minimumContributionKrw: PositiveKrwSchema.optional(),
    maximumContributionKrw: PositiveKrwSchema.optional(),
    termDays: z.number().int().positive().max(65_535).optional(),
    termMonths: z.number().int().positive().max(65_535).optional(),
    installmentCount: z.number().int().positive().max(65_535).optional(),
    earlyTerminationRateBp: z.number().int().min(0).max(10_000).optional(),
    dayCountDenominator: z.number().int().positive().max(65_535),
  })
  .superRefine((product, context) => {
    const isCma = product.kind === 'cmaRp' || product.kind === 'cmaIssuedNote';
    const isTermDeposit = product.kind === 'termDeposit';
    const hasContributionRange =
      product.minimumContributionKrw !== undefined && product.maximumContributionKrw !== undefined;

    if (
      isCma !== (product.minimumInterestBalanceKrw !== undefined) ||
      isCma === hasContributionRange ||
      (isCma &&
        (product.protectionEligible ||
          product.termDays !== undefined ||
          product.termMonths !== undefined ||
          product.installmentCount !== undefined ||
          product.earlyTerminationRateBp !== undefined)) ||
      (isTermDeposit &&
        (!product.protectionEligible ||
          product.termDays === undefined ||
          product.termMonths !== undefined ||
          product.installmentCount !== undefined ||
          product.earlyTerminationRateBp === undefined)) ||
      (product.kind === 'installmentSavings' &&
        (!product.protectionEligible ||
          product.termDays !== undefined ||
          product.termMonths === undefined ||
          product.installmentCount === undefined ||
          product.earlyTerminationRateBp === undefined))
    ) {
      context.addIssue({
        code: 'custom',
        message: 'cash product fields do not match its kind',
      });
    }

    if (
      product.minimumContributionKrw !== undefined &&
      product.maximumContributionKrw !== undefined &&
      product.minimumContributionKrw > product.maximumContributionKrw
    ) {
      context.addIssue({
        code: 'custom',
        path: ['maximumContributionKrw'],
        message: 'maximum contribution must not be below the minimum',
      });
    }
  });

export const CashProductCatalogSchema = z.object({
  products: z.array(CashProductSchema).max(100),
});

export const CmaAccountSummarySchema = z.object({
  accountId: ResourceIdSchema,
  productVersionId: ResourceIdSchema,
  annualRateBp: z.number().int().min(0).max(20_000).nullable(),
  minimumInterestBalanceKrw: PositiveKrwSchema,
  interestRemainder: NonnegativeKrwSchema,
});

export const CashContractStatusSchema = z.enum(['active', 'matured', 'closedEarly', 'cancelled']);

export const CashContractSummarySchema = z
  .object({
    contractId: ResourceIdSchema,
    productVersionId: ResourceIdSchema,
    settlementAccountId: ResourceIdSchema,
    kind: DepositKindSchema,
    status: CashContractStatusSchema,
    annualRateBp: z.number().int().min(0).max(10_000),
    currentPrincipalKrw: NonnegativeKrwSchema,
    installmentAmountKrw: PositiveKrwSchema.nullable(),
    paidInstallmentCount: z.number().int().nonnegative().max(65_535),
    missedInstallmentCount: z.number().int().nonnegative().max(65_535),
    openedGameDay: z.number().int().nonnegative(),
    maturityGameDay: z.number().int().positive(),
    expectedGrossInterestKrw: NonnegativeKrwSchema.nullable(),
    expectedIncomeTaxKrw: NonnegativeKrwSchema.nullable(),
    expectedLocalIncomeTaxKrw: NonnegativeKrwSchema.nullable(),
    expectedNetPayoutKrw: NonnegativeKrwSchema.nullable(),
  })
  .superRefine((contract, context) => {
    const expectedAmounts = [
      contract.expectedGrossInterestKrw,
      contract.expectedIncomeTaxKrw,
      contract.expectedLocalIncomeTaxKrw,
      contract.expectedNetPayoutKrw,
    ];
    const hasExpectedMaturity = expectedAmounts.every((amount) => amount !== null);
    const hasNoExpectedMaturity = expectedAmounts.every((amount) => amount === null);

    if (contract.openedGameDay >= contract.maturityGameDay) {
      context.addIssue({
        code: 'custom',
        path: ['maturityGameDay'],
        message: 'cash contract maturity must follow its opening day',
      });
    }
    if (
      (contract.kind === 'termDeposit') !== (contract.installmentAmountKrw === null) ||
      (contract.kind === 'termDeposit' &&
        (contract.paidInstallmentCount !== 0 || contract.missedInstallmentCount !== 0))
    ) {
      context.addIssue({
        code: 'custom',
        path: ['installmentAmountKrw'],
        message: 'cash contract installment fields do not match its kind',
      });
    }

    if (
      (contract.status === 'active' && !hasExpectedMaturity) ||
      (contract.status !== 'active' &&
        (contract.currentPrincipalKrw !== 0 || !hasNoExpectedMaturity))
    ) {
      context.addIssue({
        code: 'custom',
        path: ['expectedNetPayoutKrw'],
        message: 'cash contract expected amounts do not match its status',
      });
    }

    if (
      contract.status === 'active' &&
      contract.expectedGrossInterestKrw !== null &&
      contract.expectedIncomeTaxKrw !== null &&
      contract.expectedLocalIncomeTaxKrw !== null &&
      contract.expectedNetPayoutKrw !== null &&
      BigInt(contract.expectedNetPayoutKrw) !==
        BigInt(contract.currentPrincipalKrw) +
          BigInt(contract.expectedGrossInterestKrw) -
          BigInt(contract.expectedIncomeTaxKrw) -
          BigInt(contract.expectedLocalIncomeTaxKrw)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['expectedNetPayoutKrw'],
        message: 'expected net payout must reconcile with principal, interest, and tax',
      });
    }
  });

export const DepositProtectionSummarySchema = z
  .object({
    institutionId: ResourceIdSchema,
    eligibleAmountKrw: NonnegativeKrwSchema,
    protectedAmountKrw: NonnegativeKrwSchema,
    unprotectedAmountKrw: NonnegativeKrwSchema,
  })
  .superRefine((summary, context) => {
    if (
      BigInt(summary.eligibleAmountKrw) !==
      BigInt(summary.protectedAmountKrw) + BigInt(summary.unprotectedAmountKrw)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['eligibleAmountKrw'],
        message: 'eligible amount must equal protected plus unprotected amount',
      });
    }
  });

export const TaxYearSchema = z.number().int().min(1).max(9_999);

export const FinancialIncomeSourceSchema = z.enum([
  'cmaInterest',
  'depositInterest',
  'bondCoupon',
  'llxDistribution',
  'isaEarlyClose',
]);

export const FinancialIncomeSourceTotalSchema = z
  .object({
    source: FinancialIncomeSourceSchema,
    grossFinancialIncomeKrw: NonnegativeKrwSchema,
    withheldIncomeTaxKrw: NonnegativeKrwSchema,
    withheldLocalIncomeTaxKrw: NonnegativeKrwSchema,
  })
  .strict();

const FinancialIncomeYearBaseShape = {
  taxYear: TaxYearSchema,
  sources: z.array(FinancialIncomeSourceTotalSchema).max(5),
  grossFinancialIncomeKrw: NonnegativeKrwSchema,
  withheldIncomeTaxKrw: NonnegativeKrwSchema,
  withheldLocalIncomeTaxKrw: NonnegativeKrwSchema,
};

const FinancialIncomeAssessmentBaseShape = {
  taxYear: TaxYearSchema,
  grossFinancialIncomeKrw: NonnegativeKrwSchema,
  withheldIncomeTaxKrw: NonnegativeKrwSchema,
  withheldLocalIncomeTaxKrw: NonnegativeKrwSchema,
};

const NullFinancialIncomeAssessmentShape = {
  comparisonAIncomeTaxKrw: z.null(),
  comparisonALocalIncomeTaxKrw: z.null(),
  comparisonBIncomeTaxKrw: z.null(),
  comparisonBLocalIncomeTaxKrw: z.null(),
  assessedIncomeTaxKrw: z.null(),
  assessedLocalIncomeTaxKrw: z.null(),
  additionalTaxKrw: z.null(),
  refundKrw: z.null(),
  filingDueDate: z.null(),
  filedGameDay: z.null(),
};

const CalculatedFinancialIncomeAssessmentShape = {
  comparisonAIncomeTaxKrw: NonnegativeKrwSchema,
  comparisonALocalIncomeTaxKrw: NonnegativeKrwSchema,
  comparisonBIncomeTaxKrw: NonnegativeKrwSchema,
  comparisonBLocalIncomeTaxKrw: NonnegativeKrwSchema,
  assessedIncomeTaxKrw: NonnegativeKrwSchema,
  assessedLocalIncomeTaxKrw: NonnegativeKrwSchema,
};

interface FinancialIncomeAssessmentAmounts {
  readonly status: string;
  readonly withheldIncomeTaxKrw: number;
  readonly withheldLocalIncomeTaxKrw: number;
  readonly comparisonAIncomeTaxKrw: number | null;
  readonly comparisonALocalIncomeTaxKrw: number | null;
  readonly comparisonBIncomeTaxKrw: number | null;
  readonly comparisonBLocalIncomeTaxKrw: number | null;
  readonly assessedIncomeTaxKrw: number | null;
  readonly assessedLocalIncomeTaxKrw: number | null;
  readonly additionalTaxKrw: number | null;
  readonly refundKrw: number | null;
}

function refineFinancialIncomeAssessment(
  assessment: FinancialIncomeAssessmentAmounts,
  context: z.RefinementCtx,
): void {
  const {
    comparisonAIncomeTaxKrw,
    comparisonALocalIncomeTaxKrw,
    comparisonBIncomeTaxKrw,
    comparisonBLocalIncomeTaxKrw,
    assessedIncomeTaxKrw,
    assessedLocalIncomeTaxKrw,
    additionalTaxKrw,
    refundKrw,
  } = assessment;
  if (
    comparisonAIncomeTaxKrw === null ||
    comparisonALocalIncomeTaxKrw === null ||
    comparisonBIncomeTaxKrw === null ||
    comparisonBLocalIncomeTaxKrw === null ||
    assessedIncomeTaxKrw === null ||
    assessedLocalIncomeTaxKrw === null ||
    additionalTaxKrw === null ||
    refundKrw === null
  ) {
    return;
  }

  if (additionalTaxKrw > 0 && refundKrw > 0) {
    context.addIssue({
      code: 'custom',
      path: ['additionalTaxKrw'],
      message: 'additional tax and refund are mutually exclusive',
    });
  }

  const assessedTotal = BigInt(assessedIncomeTaxKrw) + BigInt(assessedLocalIncomeTaxKrw);
  const withheldTotal =
    BigInt(assessment.withheldIncomeTaxKrw) + BigInt(assessment.withheldLocalIncomeTaxKrw);
  if (assessedTotal - withheldTotal !== BigInt(additionalTaxKrw) - BigInt(refundKrw)) {
    context.addIssue({
      code: 'custom',
      path: ['additionalTaxKrw'],
      message: 'assessment, withholding, additional tax, and refund must reconcile',
    });
  }

  if (
    assessment.status === 'finalizedNoFiling' &&
    (comparisonAIncomeTaxKrw !== assessment.withheldIncomeTaxKrw ||
      comparisonALocalIncomeTaxKrw !== assessment.withheldLocalIncomeTaxKrw ||
      comparisonBIncomeTaxKrw !== assessment.withheldIncomeTaxKrw ||
      comparisonBLocalIncomeTaxKrw !== assessment.withheldLocalIncomeTaxKrw ||
      assessedIncomeTaxKrw !== assessment.withheldIncomeTaxKrw ||
      assessedLocalIncomeTaxKrw !== assessment.withheldLocalIncomeTaxKrw)
  ) {
    context.addIssue({
      code: 'custom',
      path: ['assessedIncomeTaxKrw'],
      message: 'a no-filing assessment must preserve withholding as its final tax',
    });
  }
}

const NotApplicableFinancialIncomeYearSchema = z
  .object({
    ...FinancialIncomeYearBaseShape,
    status: z.literal('notApplicable'),
    ...NullFinancialIncomeAssessmentShape,
  })
  .strict();

const OpenFinancialIncomeYearSchema = z
  .object({
    ...FinancialIncomeYearBaseShape,
    status: z.literal('open'),
    ...NullFinancialIncomeAssessmentShape,
  })
  .strict();

const FinalizedNoFilingFinancialIncomeYearSchema = z
  .object({
    ...FinancialIncomeYearBaseShape,
    status: z.literal('finalizedNoFiling'),
    ...CalculatedFinancialIncomeAssessmentShape,
    additionalTaxKrw: z.literal(0),
    refundKrw: z.literal(0),
    filingDueDate: z.null(),
    filedGameDay: z.null(),
  })
  .strict();

const FilingPendingFinancialIncomeYearSchema = z
  .object({
    ...FinancialIncomeYearBaseShape,
    status: z.literal('filingPending'),
    ...CalculatedFinancialIncomeAssessmentShape,
    additionalTaxKrw: NonnegativeKrwSchema,
    refundKrw: NonnegativeKrwSchema,
    filingDueDate: z.iso.date(),
    filedGameDay: z.null(),
  })
  .strict();

const FiledFinancialIncomeYearSchema = z
  .object({
    ...FinancialIncomeYearBaseShape,
    status: z.literal('filed'),
    ...CalculatedFinancialIncomeAssessmentShape,
    additionalTaxKrw: NonnegativeKrwSchema,
    refundKrw: NonnegativeKrwSchema,
    filingDueDate: z.iso.date(),
    filedGameDay: z.number().int().safe().nonnegative(),
  })
  .strict();

export const FinancialIncomeYearSchema = z
  .discriminatedUnion('status', [
    NotApplicableFinancialIncomeYearSchema,
    OpenFinancialIncomeYearSchema,
    FinalizedNoFilingFinancialIncomeYearSchema,
    FilingPendingFinancialIncomeYearSchema,
    FiledFinancialIncomeYearSchema,
  ])
  .superRefine((year, context) => {
    const sources = new Set<string>();
    let grossFinancialIncomeKrw = 0n;
    let withheldIncomeTaxKrw = 0n;
    let withheldLocalIncomeTaxKrw = 0n;
    for (const [index, source] of year.sources.entries()) {
      if (sources.has(source.source)) {
        context.addIssue({
          code: 'custom',
          path: ['sources', index, 'source'],
          message: 'financial-income sources must be unique',
        });
      }
      sources.add(source.source);
      grossFinancialIncomeKrw += BigInt(source.grossFinancialIncomeKrw);
      withheldIncomeTaxKrw += BigInt(source.withheldIncomeTaxKrw);
      withheldLocalIncomeTaxKrw += BigInt(source.withheldLocalIncomeTaxKrw);
    }

    if (
      year.status !== 'notApplicable' &&
      (grossFinancialIncomeKrw !== BigInt(year.grossFinancialIncomeKrw) ||
        withheldIncomeTaxKrw !== BigInt(year.withheldIncomeTaxKrw) ||
        withheldLocalIncomeTaxKrw !== BigInt(year.withheldLocalIncomeTaxKrw))
    ) {
      context.addIssue({
        code: 'custom',
        path: ['sources'],
        message: 'financial-income source rows must reconcile with yearly totals',
      });
    }
    refineFinancialIncomeAssessment(year, context);
  });

const FinalizedNoFilingFinancialIncomeAssessmentSchema = z
  .object({
    ...FinancialIncomeAssessmentBaseShape,
    status: z.literal('finalizedNoFiling'),
    ...CalculatedFinancialIncomeAssessmentShape,
    additionalTaxKrw: z.literal(0),
    refundKrw: z.literal(0),
    filingDueDate: z.null(),
    filedGameDay: z.null(),
  })
  .strict();

const FilingPendingFinancialIncomeAssessmentSchema = z
  .object({
    ...FinancialIncomeAssessmentBaseShape,
    status: z.literal('filingPending'),
    ...CalculatedFinancialIncomeAssessmentShape,
    additionalTaxKrw: NonnegativeKrwSchema,
    refundKrw: NonnegativeKrwSchema,
    filingDueDate: z.iso.date(),
    filedGameDay: z.null(),
  })
  .strict();

const FiledFinancialIncomeAssessmentSchema = z
  .object({
    ...FinancialIncomeAssessmentBaseShape,
    status: z.literal('filed'),
    ...CalculatedFinancialIncomeAssessmentShape,
    additionalTaxKrw: NonnegativeKrwSchema,
    refundKrw: NonnegativeKrwSchema,
    filingDueDate: z.iso.date(),
    filedGameDay: z.number().int().safe().nonnegative(),
  })
  .strict();

export const FinancialIncomeAssessmentSchema = z
  .discriminatedUnion('status', [
    FinalizedNoFilingFinancialIncomeAssessmentSchema,
    FilingPendingFinancialIncomeAssessmentSchema,
    FiledFinancialIncomeAssessmentSchema,
  ])
  .superRefine(refineFinancialIncomeAssessment);

export const IsaAccountTypeSchema = z.enum(['isaGeneral', 'isaLowIncome']);

export const IsaAccountSummarySchema = z
  .object({
    accountId: ResourceIdSchema,
    type: IsaAccountTypeSchema,
    openedGameDay: z.number().int().nonnegative(),
    minimumTermGameDay: z.number().int().nonnegative(),
    totalContributionKrw: NonnegativeKrwSchema,
    principalWithdrawalKrw: NonnegativeKrwSchema,
    contributionCapacityKrw: NonnegativeKrwSchema,
    taxProfitKrw: NonnegativeKrwSchema,
    deductibleLossKrw: NonnegativeKrwSchema,
    expectedCloseIncomeTaxKrw: NonnegativeKrwSchema,
    expectedCloseLocalIncomeTaxKrw: NonnegativeKrwSchema,
  })
  .strict()
  .superRefine((account, context) => {
    if (account.minimumTermGameDay <= account.openedGameDay) {
      context.addIssue({
        code: 'custom',
        path: ['minimumTermGameDay'],
        message: 'ISA minimum term must follow its opening day',
      });
    }
    if (account.principalWithdrawalKrw > account.totalContributionKrw) {
      context.addIssue({
        code: 'custom',
        path: ['principalWithdrawalKrw'],
        message: 'ISA principal withdrawal must not exceed total contribution',
      });
    }
  });

export const PensionAccountTypeSchema = z.enum(['pensionSavings', 'irp']);

export const PensionTaxLayersSchema = z
  .object({
    taxExcludedContributionKrw: NonnegativeKrwSchema,
    deferredRetirementIncomeKrw: NonnegativeKrwSchema,
    creditedContributionKrw: NonnegativeKrwSchema,
    earningsKrw: NonnegativeKrwSchema,
  })
  .strict();

export const PensionAccountSummarySchema = z
  .object({
    accountId: ResourceIdSchema,
    type: PensionAccountTypeSchema,
    openedGameDay: z.number().int().nonnegative(),
    eligiblePensionStartGameDay: z.number().int().nonnegative(),
    pensionStarted: z.boolean(),
    taxLayers: PensionTaxLayersSchema,
    currentYearContributionKrw: NonnegativeKrwSchema,
    currentYearCreditEligibleKrw: NonnegativeKrwSchema,
    expectedCreditKrw: NonnegativeKrwSchema,
    currentYearPensionLimitKrw: NonnegativeKrwSchema.nullable(),
    currentYearPensionWithdrawnKrw: NonnegativeKrwSchema,
    riskAssetValueKrw: NonnegativeKrwSchema,
    totalValueKrw: NonnegativeKrwSchema,
    riskAssetRatioPpm: z.number().int().min(0).max(1_000_000),
  })
  .strict()
  .superRefine((account, context) => {
    if (account.eligiblePensionStartGameDay <= account.openedGameDay) {
      context.addIssue({
        code: 'custom',
        path: ['eligiblePensionStartGameDay'],
        message: 'pension eligibility day must follow its opening day',
      });
    }
    if (account.currentYearCreditEligibleKrw > account.currentYearContributionKrw) {
      context.addIssue({
        code: 'custom',
        path: ['currentYearCreditEligibleKrw'],
        message: 'credit-eligible contribution must not exceed current-year contribution',
      });
    }
    if (account.riskAssetValueKrw > account.totalValueKrw) {
      context.addIssue({
        code: 'custom',
        path: ['riskAssetValueKrw'],
        message: 'risk-asset value must not exceed total pension value',
      });
    }
    const layerTotal = Object.values(account.taxLayers).reduce(
      (total, amount) => total + BigInt(amount),
      0n,
    );
    if (layerTotal !== BigInt(account.totalValueKrw)) {
      context.addIssue({
        code: 'custom',
        path: ['taxLayers'],
        message: 'pension tax layers must reconcile with total value',
      });
    }
  });

const PositiveU32Schema = z.number().int().safe().positive().max(4_294_967_295);
const NonnegativeRatePpmSchema = z.number().int().safe().min(0).max(1_000_000);

export const IndexProductSummarySchema = z
  .object({
    id: ResourceIdSchema,
    key: z.string().min(1).max(64),
    displayName: z.string().min(1).max(100),
    annualManagementFeePpm: NonnegativeRatePpmSchema,
    annualDistributionRatePpm: NonnegativeRatePpmSchema,
    dayCountDenominator: z.number().int().safe().positive(),
    buyFeePpm: NonnegativeRatePpmSchema,
    sellFeePpm: NonnegativeRatePpmSchema,
    sellTaxPpm: NonnegativeRatePpmSchema,
  })
  .strict();

export const FinanceProductBundleSchema = z
  .object({
    indexProduct: IndexProductSummarySchema,
    bondProductVersionIds: z.tuple([ResourceIdSchema, ResourceIdSchema]),
    goldProductVersionId: ResourceIdSchema,
  })
  .strict()
  .superRefine((bundle, context) => {
    if (bundle.bondProductVersionIds[0] === bundle.bondProductVersionIds[1]) {
      context.addIssue({
        code: 'custom',
        path: ['bondProductVersionIds', 1],
        message: 'three-year and ten-year bond product IDs must differ',
      });
    }
  });

export const LlxDistributionEntitlementSchema = z
  .object({
    id: ResourceIdSchema,
    accountId: ResourceIdSchema,
    recordDate: z.iso.date(),
    paymentDate: z.iso.date(),
    quantity: z.number().int().safe().positive().max(1_000_000),
    grossAmountKrw: NonnegativeKrwSchema,
    status: z.literal('pending'),
  })
  .strict()
  .superRefine((entitlement, context) => {
    if (entitlement.paymentDate < entitlement.recordDate) {
      context.addIssue({
        code: 'custom',
        path: ['paymentDate'],
        message: 'distribution payment date must not precede its record date',
      });
    }
  });

export const BondPositionSummarySchema = z
  .object({
    accountId: ResourceIdSchema,
    seriesId: ResourceIdSchema,
    bondUnits: PositiveU32Schema,
    totalCostBasisKrw: NonnegativeKrwSchema,
    dirtyPriceKrw: PositiveKrwSchema,
    marketValueKrw: NonnegativeKrwSchema,
    unrealizedGainLossKrw: z.number().int().safe(),
  })
  .strict()
  .superRefine((position, context) => {
    const marketValueKrw = BigInt(position.dirtyPriceKrw) * BigInt(position.bondUnits);
    if (marketValueKrw !== BigInt(position.marketValueKrw)) {
      context.addIssue({
        code: 'custom',
        path: ['marketValueKrw'],
        message: 'bond market value must equal dirty price times units',
      });
    }
    if (
      BigInt(position.unrealizedGainLossKrw) !==
      BigInt(position.marketValueKrw) - BigInt(position.totalCostBasisKrw)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['unrealizedGainLossKrw'],
        message: 'bond unrealized result must reconcile with market value and cost basis',
      });
    }
  });

export const GoldAccountSummarySchema = z
  .object({
    accountId: ResourceIdSchema,
    productVersionId: ResourceIdSchema,
    quantityGram: z.number().int().safe().nonnegative().max(4_294_967_295),
    totalCostBasisKrw: NonnegativeKrwSchema,
    averageCostKrwPerGram: PositiveKrwSchema.nullable(),
    closeKrwPerGram: PositiveKrwSchema,
    marketValueKrw: NonnegativeKrwSchema,
    unrealizedGainLossKrw: z.number().int().safe(),
  })
  .strict()
  .superRefine((account, context) => {
    const marketValueKrw = BigInt(account.closeKrwPerGram) * BigInt(account.quantityGram);
    if (marketValueKrw !== BigInt(account.marketValueKrw)) {
      context.addIssue({
        code: 'custom',
        path: ['marketValueKrw'],
        message: 'gold market value must equal close price times quantity',
      });
    }
    if (
      BigInt(account.unrealizedGainLossKrw) !==
      BigInt(account.marketValueKrw) - BigInt(account.totalCostBasisKrw)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['unrealizedGainLossKrw'],
        message: 'gold unrealized result must reconcile with market value and cost basis',
      });
    }

    if (
      (account.quantityGram === 0 &&
        (account.totalCostBasisKrw !== 0 || account.averageCostKrwPerGram !== null)) ||
      (account.quantityGram > 0 &&
        (account.totalCostBasisKrw === 0 ||
          account.averageCostKrwPerGram === null ||
          BigInt(account.averageCostKrwPerGram) !==
            BigInt(account.totalCostBasisKrw) / BigInt(account.quantityGram)))
    ) {
      context.addIssue({
        code: 'custom',
        path: ['averageCostKrwPerGram'],
        message: 'gold average cost must match quantity and total cost basis',
      });
    }
  });

export const GoldBarSizeGramSchema = z.union([z.literal(100), z.literal(1_000)]);

export const PhysicalGoldHoldingSchema = z
  .object({
    barSizeGram: GoldBarSizeGramSchema,
    barCount: PositiveU32Schema,
    totalQuantityGram: PositiveU32Schema,
    closeKrwPerGram: PositiveKrwSchema,
    marketValueKrw: NonnegativeKrwSchema,
  })
  .strict()
  .superRefine((holding, context) => {
    const totalQuantityGram = BigInt(holding.barSizeGram) * BigInt(holding.barCount);
    if (totalQuantityGram !== BigInt(holding.totalQuantityGram)) {
      context.addIssue({
        code: 'custom',
        path: ['totalQuantityGram'],
        message: 'physical-gold quantity must equal bar size times count',
      });
    }
    if (
      BigInt(holding.marketValueKrw) !==
      BigInt(holding.closeKrwPerGram) * BigInt(holding.totalQuantityGram)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['marketValueKrw'],
        message: 'physical-gold market value must equal close price times quantity',
      });
    }
  });

const FinanceSnapshotBaseSchema = z
  .object({
    policySet: PolicySetSummarySchema,
    accounts: z.array(FinancialAccountSchema).max(32),
    cmaAccounts: z.array(CmaAccountSummarySchema).max(32),
    cashContracts: z.array(CashContractSummarySchema).max(100),
    depositProtection: z.array(DepositProtectionSummarySchema).max(16),
    currentTaxYear: FinancialIncomeYearSchema,
    isaAccounts: z.array(IsaAccountSummarySchema).max(1),
    pensionAccounts: z.array(PensionAccountSummarySchema).max(2),
    productBundle: FinanceProductBundleSchema.nullable(),
    llxDistributionEntitlements: z.array(LlxDistributionEntitlementSchema).max(8),
    bondPositions: z.array(BondPositionSummarySchema).max(640),
    goldAccounts: z.array(GoldAccountSummarySchema).max(1),
    physicalGoldHoldings: z.array(PhysicalGoldHoldingSchema).max(2),
    latestFinancialIncomeAssessment: FinancialIncomeAssessmentSchema.nullable(),
    pendingSettlements: z.array(PendingSettlementSummarySchema).max(20),
  })
  .strict();

type FinanceSnapshotValue = z.infer<typeof FinanceSnapshotBaseSchema>;

function refineFinanceSnapshotVersion(
  snapshot: FinanceSnapshotValue,
  context: z.RefinementCtx,
): void {
  const isM2d = snapshot.productBundle !== null;
  if (
    (isM2d && snapshot.currentTaxYear.status !== 'open') ||
    (!isM2d && snapshot.currentTaxYear.status !== 'notApplicable')
  ) {
    context.addIssue({
      code: 'custom',
      path: ['currentTaxYear', 'status'],
      message: 'current tax-year status must match the pinned product bundle',
    });
  }

  if (
    !isM2d &&
    (snapshot.llxDistributionEntitlements.length > 0 ||
      snapshot.bondPositions.length > 0 ||
      snapshot.goldAccounts.length > 0 ||
      snapshot.physicalGoldHoldings.length > 0 ||
      snapshot.latestFinancialIncomeAssessment !== null)
  ) {
    context.addIssue({
      code: 'custom',
      path: ['productBundle'],
      message: 'legacy runs cannot contain M2-D assets or assessments',
    });
  }

  if (
    snapshot.latestFinancialIncomeAssessment !== null &&
    snapshot.latestFinancialIncomeAssessment.taxYear >= snapshot.currentTaxYear.taxYear
  ) {
    context.addIssue({
      code: 'custom',
      path: ['latestFinancialIncomeAssessment', 'taxYear'],
      message: 'latest assessment must precede the current tax year',
    });
  }
}

function refineFinanceSnapshotAccountReferences(
  snapshot: FinanceSnapshotValue,
  context: z.RefinementCtx,
): void {
  const accountTypes = new Map(snapshot.accounts.map((account) => [account.id, account.type]));
  for (const [index, entitlement] of snapshot.llxDistributionEntitlements.entries()) {
    if (!accountTypes.has(entitlement.accountId)) {
      context.addIssue({
        code: 'custom',
        path: ['llxDistributionEntitlements', index, 'accountId'],
        message: 'distribution entitlement must reference a snapshot account',
      });
    }
  }

  const bondPositionKeys = new Set<string>();
  for (const [index, position] of snapshot.bondPositions.entries()) {
    if (!accountTypes.has(position.accountId)) {
      context.addIssue({
        code: 'custom',
        path: ['bondPositions', index, 'accountId'],
        message: 'bond position must reference a snapshot account',
      });
    }
    const key = `${position.accountId}:${position.seriesId}`;
    if (bondPositionKeys.has(key)) {
      context.addIssue({
        code: 'custom',
        path: ['bondPositions', index],
        message: 'bond positions must be unique by account and series',
      });
    }
    bondPositionKeys.add(key);
  }

  for (const [index, account] of snapshot.goldAccounts.entries()) {
    if (accountTypes.get(account.accountId) !== 'krxGold') {
      context.addIssue({
        code: 'custom',
        path: ['goldAccounts', index, 'accountId'],
        message: 'gold summary must reference a KRX gold account',
      });
    }
    if (
      snapshot.productBundle !== null &&
      account.productVersionId !== snapshot.productBundle.goldProductVersionId
    ) {
      context.addIssue({
        code: 'custom',
        path: ['goldAccounts', index, 'productVersionId'],
        message: 'gold account must use the pinned gold product',
      });
    }
  }
}

function refinePhysicalGoldHoldingKeys(
  snapshot: FinanceSnapshotValue,
  context: z.RefinementCtx,
): void {
  const physicalBarSizes = new Set<number>();
  for (const [index, holding] of snapshot.physicalGoldHoldings.entries()) {
    if (physicalBarSizes.has(holding.barSizeGram)) {
      context.addIssue({
        code: 'custom',
        path: ['physicalGoldHoldings', index, 'barSizeGram'],
        message: 'physical-gold holdings must be unique by bar size',
      });
    }
    physicalBarSizes.add(holding.barSizeGram);
  }
}

export const FinanceSnapshotSchema = FinanceSnapshotBaseSchema.superRefine((snapshot, context) => {
  refineFinanceSnapshotVersion(snapshot, context);
  refineFinanceSnapshotAccountReferences(snapshot, context);
  refinePhysicalGoldHoldingKeys(snapshot, context);
});

// -- Career foundation (M3-A) ------------------------------------------

export const SpecDimensionSchema = z.enum([
  'education',
  'certification',
  'language',
  'training',
  'experience',
  'project',
]);

export const EvidenceKindSchema = SpecDimensionSchema;
export const LifeStatusSchema = z.enum([
  'unemployed',
  'employed',
  'activeDuty',
  'socialService',
  'specialService',
  'officerOrNco',
]);
export const CareerActivityStatusSchema = z.enum(['planned', 'active', 'completed', 'cancelled']);
export const CareerArtifactKindSchema = z.enum(['portfolio', 'resume', 'linkedinProfile']);
export const CareerIndustrySchema = z.enum([
  'itSoftware',
  'financeInsurance',
  'manufacturing',
  'constructionEngineering',
  'retailService',
  'publicSocial',
]);

export const CareerScoresSchema = z
  .object({
    education: z.number().int().min(0).max(10_000),
    certification: z.number().int().min(0).max(10_000),
    language: z.number().int().min(0).max(10_000),
    training: z.number().int().min(0).max(10_000),
    experience: z.number().int().min(0).max(10_000),
    project: z.number().int().min(0).max(10_000),
  })
  .strict();

export const CareerActivitySummarySchema = z
  .object({
    id: ResourceIdSchema,
    catalogEntryId: ResourceIdSchema,
    activityKey: z.string().min(1).max(96),
    displayName: z.string().min(1).max(120),
    status: CareerActivityStatusSchema,
    priority: z.number().int().min(1).max(3).nullable(),
    startedGameDay: z.number().int().nonnegative().nullable(),
    accumulatedEffortUnits: z.number().int().safe().nonnegative(),
    requiredEffortUnits: z.number().int().safe().positive(),
    elapsedCalendarDays: z.number().int().nonnegative(),
    minimumCalendarDays: z.number().int().positive(),
    dailyEffortCapUnits: z.number().int().safe().positive(),
    completedGameDay: z.number().int().nonnegative().nullable(),
  })
  .strict();

export const CareerArtifactSummarySchema = z
  .object({
    id: ResourceIdSchema,
    kind: CareerArtifactKindSchema,
    versionNo: z.number().int().positive(),
    completenessBp: z.number().int().min(0).max(10_000),
    createdGameDay: z.number().int().nonnegative(),
  })
  .strict();

export const CareerScheduledActionKindSchema = z.enum([
  'employmentStart',
  'militaryServiceStart',
  'militaryServiceCompletion',
  'documentReview',
  'confirmationExpiry',
  'interviewDecision',
  'offerExpiry',
  'invitationGeneration',
]);

export const CareerScheduledSettlementKindSchema = z.enum([
  'employmentPayroll',
  'employmentReconciliation',
  'militaryPay',
  'militarySavingsInstallment',
  'militarySavingsMaturity',
  'militarySavingsGovernmentMatch',
]);

export const CareerPendingScheduleItemSchema = z.discriminatedUnion('sourceKind', [
  z
    .object({
      sourceKind: z.literal('careerAction'),
      id: ResourceIdSchema,
      dueGameDay: z.number().int().safe().nonnegative(),
      kind: CareerScheduledActionKindSchema,
    })
    .strict(),
  z
    .object({
      sourceKind: z.literal('settlement'),
      id: ResourceIdSchema,
      dueGameDay: z.number().int().safe().nonnegative(),
      kind: CareerScheduledSettlementKindSchema,
    })
    .strict(),
]);

export const CareerSnapshotSchema = z
  .object({
    focusedJobFamilyKey: z.string().min(1).max(64),
    possessedScores: CareerScoresSchema,
    activeActivities: z.array(CareerActivitySummarySchema).max(3),
    latestArtifacts: z.array(CareerArtifactSummarySchema).max(3),
    openApplications: z
      .array(
        z
          .object({
            id: ResourceIdSchema,
            postingKey: z.string().regex(/^[0-9a-f]{64}$/),
            platform: z.enum(['sarangbang', 'jobkorea', 'saramin', 'wanted', 'linkedin', 'work24']),
            industry: CareerIndustrySchema,
            employerName: z.string().min(1).max(120),
            jobFamilyKey: z.string().min(1).max(64),
            status: z.enum([
              'submitted',
              'interviewAwaitingConfirmation',
              'interviewConfirmed',
              'offered',
            ]),
            confirmationDeadlineExclusiveGameDay: z.number().int().nonnegative().nullable(),
            interviewGameDay: z.number().int().nonnegative().nullable(),
            offer: z
              .object({
                id: ResourceIdSchema,
                status: z.enum(['offered']),
                annualSalaryKrw: z.number().int().safe().positive(),
                paydayDayOfMonth: z.number().int().min(1).max(31),
                startGameDay: z.number().int().nonnegative(),
                expiresExclusiveGameDay: z.number().int().nonnegative(),
                wantedRewardKrw: z.number().int().safe().nonnegative(),
              })
              .strict()
              .nullable(),
          })
          .strict(),
      )
      .max(10),
    openInvitations: z
      .array(
        z
          .object({
            id: ResourceIdSchema,
            postingKey: z.string().regex(/^[0-9a-f]{64}$/),
            platform: z.enum(['sarangbang', 'jobkorea', 'saramin', 'wanted', 'linkedin', 'work24']),
            industry: CareerIndustrySchema,
            jobFamilyKey: z.string().min(1).max(64),
            employerName: z.string().min(1).max(120),
            artifactVersionId: ResourceIdSchema,
            createdGameDay: z.number().int().nonnegative(),
            expiresExclusiveGameDay: z.number().int().nonnegative(),
          })
          .strict(),
      )
      .max(5),
    employment: z
      .object({
        id: ResourceIdSchema,
        status: z.enum(['pendingStart', 'active', 'ended']),
        jobFamilyKey: z.string().min(1).max(64),
        employerName: z.string().min(1).max(120),
        region: z.string().min(1).max(64),
        annualSalaryKrw: z.number().int().safe().positive(),
        paydayDayOfMonth: z.number().int().min(1).max(31),
        startGameDay: z.number().int().nonnegative(),
        endGameDay: z.number().int().nonnegative().nullable(),
        creditedExperienceDays: z.number().int().safe().nonnegative(),
      })
      .strict()
      .nullable(),
    latestPayroll: z.lazy(() => CareerPayrollItemSchema).nullable(),
    currentEmploymentTaxYear: z.lazy(() => OpenCareerTaxYearStateSchema),
    latestEmploymentTaxAssessment: z.lazy(() => DefinitiveCareerTaxYearStateSchema).nullable(),
    militaryStatus: z.lazy(() => CareerMilitaryStatusSchema),
    activeMilitaryService: z.lazy(() => ActiveMilitaryServiceSummarySchema).nullable(),
    activeMilitarySavings: z.array(z.lazy(() => ActiveMilitarySavingsSummarySchema)).max(2),
    pendingCareerSchedule: z.array(CareerPendingScheduleItemSchema).max(20),
  })
  .strict()
  .superRefine((snapshot, context) => {
    const priorities = new Set<number>();
    for (const [index, activity] of snapshot.activeActivities.entries()) {
      if (activity.status !== 'active' || activity.priority === null) {
        context.addIssue({
          code: 'custom',
          path: ['activeActivities', index],
          message: 'snapshot activities must be active and prioritized',
        });
      } else if (priorities.has(activity.priority)) {
        context.addIssue({
          code: 'custom',
          path: ['activeActivities', index, 'priority'],
          message: 'active activity priorities must be unique',
        });
      }
      if (activity.priority !== null) priorities.add(activity.priority);
    }
    const artifactKinds = new Set(snapshot.latestArtifacts.map((artifact) => artifact.kind));
    if (artifactKinds.size !== snapshot.latestArtifacts.length) {
      context.addIssue({
        code: 'custom',
        path: ['latestArtifacts'],
        message: 'latest artifact summaries must be unique by kind',
      });
    }
    if (
      snapshot.latestEmploymentTaxAssessment !== null &&
      snapshot.latestEmploymentTaxAssessment.taxYear >= snapshot.currentEmploymentTaxYear.taxYear
    ) {
      context.addIssue({
        code: 'custom',
        path: ['latestEmploymentTaxAssessment', 'taxYear'],
        message: 'latest employment-tax assessment must precede the current open tax year',
      });
    }
    if ((snapshot.militaryStatus === 'serving') !== (snapshot.activeMilitaryService !== null)) {
      context.addIssue({
        code: 'custom',
        path: ['activeMilitaryService'],
        message: 'serving status and active military service must appear together',
      });
    }
    if (snapshot.militaryStatus !== 'serving' && snapshot.activeMilitarySavings.length > 0) {
      context.addIssue({
        code: 'custom',
        path: ['activeMilitarySavings'],
        message: 'active military savings require a serving military status',
      });
    }
    refinePendingCareerSchedule(snapshot.pendingCareerSchedule, context);
  });

export const LivingCostCategorySchema = z.enum([
  'housing',
  'food',
  'transport',
  'communication',
  'utilities',
  'healthcare',
  'education',
  'dependentCare',
  'discretionary',
]);

export const LifeBudgetSelectionSchema = z
  .object({
    category: LivingCostCategorySchema,
    bandId: ResourceIdSchema,
  })
  .strict();

const LifeBudgetSelectionsSchema = z
  .array(LifeBudgetSelectionSchema)
  .length(LivingCostCategorySchema.options.length)
  .superRefine((selections, context) => {
    const categories = new Set(selections.map((selection) => selection.category));
    if (categories.size !== LivingCostCategorySchema.options.length) {
      context.addIssue({
        code: 'custom',
        message: 'budget selections must contain every living-cost category exactly once',
      });
    }
  });

function hasCanonicalLifeCategoryOrder(categories: readonly string[]): boolean {
  return categories.every(
    (category, index) => category === LivingCostCategorySchema.options[index],
  );
}

export const LifeRateStatusSchema = z.enum(['active', 'rateUnavailable']);
export const ResidenceTenureKindSchema = z.enum(['rentFree', 'owner', 'jeonse', 'monthlyRent']);
export const HousingRegionKeySchema = z.enum(['capitalArea', 'metropolitan', 'smallCity', 'rural']);
export const HousingPropertyTypeSchema = z.enum(['apartment', 'multiFamily', 'detached']);
export const HousingOfferKindSchema = z.enum(['sale', 'jeonse', 'monthlyRent']);

const HousingListingIdSchema = ResourceIdSchema.refine(
  (value) => BigInt(value) <= 9_223_372_036_854_775_807n,
  'housing listing ID exceeds signed 63-bit range',
);

export const HousingLeaseCapabilitySchema = z.enum([
  'unavailable',
  'cashJeonse',
  'cashJeonseAndMonthlyRent',
]);
export const HousingLeaseRenewalRuleSchema = z.enum(['openEnded', 'fixedTermAutoRenew']);
export const HousingLeaseTerminationReviewRuleSchema = z.literal('oldestActiveArrearAge');
export const HousingRentChargeRuleSchema = z.literal('nextMonthStartFull');
export const HousingArrearRepaymentRuleSchema = z.literal('manualOnly');

export const HousingLeaseLifecycleTermsSchema = z
  .object({
    termMonths: z.number().int().safe().positive(),
    renewalNoticeLeadDays: z.number().int().safe().positive(),
    monthlyRentTerminationReview: z
      .object({
        rule: HousingLeaseTerminationReviewRuleSchema,
        afterGameDays: z.number().int().safe().positive(),
      })
      .strict()
      .nullable(),
  })
  .strict();

export const HousingLeaseCurrentTermSchema = z
  .object({
    termNo: z.number().int().safe().positive(),
    effectiveFromGameDay: z.number().int().safe().nonnegative(),
    effectiveToGameDay: z.number().int().safe().nonnegative(),
  })
  .strict()
  .superRefine((term, context) => {
    if (term.effectiveToGameDay <= term.effectiveFromGameDay) {
      context.addIssue({
        code: 'custom',
        path: ['effectiveToGameDay'],
        message: 'a lease term must end strictly after it begins',
      });
    }
  });

export const HousingLeaseRenewalNoticeSchema = z
  .object({
    termNo: z.number().int().safe().positive(),
    publishedGameDay: z.number().int().safe().nonnegative(),
    renewsOnGameDay: z.number().int().safe().nonnegative(),
  })
  .strict()
  .superRefine((notice, context) => {
    if (notice.renewsOnGameDay <= notice.publishedGameDay) {
      context.addIssue({
        code: 'custom',
        path: ['renewsOnGameDay'],
        message: 'a renewal notice must be published before the renewal day',
      });
    }
  });

export const HousingLeaseTerminationReviewSchema = z
  .object({
    status: z.literal('underReview'),
    openedGameDay: z.number().int().safe().nonnegative(),
    triggerArrearId: ResourceIdSchema,
    activeLeaseArrearKrw: PositiveKrwSchema,
  })
  .strict();

const HousingActiveLeaseFields = {
  id: ResourceIdSchema,
  listingId: HousingListingIdSchema,
  role: z.literal('tenant'),
  regionKey: HousingRegionKeySchema,
  propertyType: HousingPropertyTypeSchema,
  exclusiveAreaSquareMeters: z.number().int().safe().positive(),
  depositKrw: PositiveKrwSchema,
  effectiveFromGameDay: z.number().int().safe().nonnegative(),
  effectiveToGameDay: z.null(),
  renewalRule: HousingLeaseRenewalRuleSchema,
  currentTerm: HousingLeaseCurrentTermSchema.nullable(),
  renewalNotice: HousingLeaseRenewalNoticeSchema.nullable(),
  terminationReview: HousingLeaseTerminationReviewSchema.nullable(),
} as const;

const HousingActiveJeonseLeaseSchema = z
  .object({
    ...HousingActiveLeaseFields,
    depositLoanId: ResourceIdSchema.nullable(),
    offerKind: z.literal('jeonse'),
    monthlyRentKrw: z.null(),
    nextRentDueGameDay: z.null(),
  })
  .strict();

const HousingActiveMonthlyRentLeaseSchema = z
  .object({
    ...HousingActiveLeaseFields,
    depositLoanId: z.null(),
    offerKind: z.literal('monthlyRent'),
    monthlyRentKrw: PositiveKrwSchema,
    nextRentDueGameDay: z.number().int().safe().nonnegative(),
  })
  .strict()
  .superRefine((lease, context) => {
    if (lease.nextRentDueGameDay <= lease.effectiveFromGameDay) {
      context.addIssue({
        code: 'custom',
        path: ['nextRentDueGameDay'],
        message: 'the first monthly-rent charge must be strictly after move-in',
      });
    }
  });

export const HousingActiveLeaseSchema = z
  .discriminatedUnion('offerKind', [
    HousingActiveJeonseLeaseSchema,
    HousingActiveMonthlyRentLeaseSchema,
  ])
  .superRefine((lease, context) => {
    if (lease.renewalRule === 'openEnded') {
      if (
        lease.currentTerm !== null ||
        lease.renewalNotice !== null ||
        lease.terminationReview !== null
      ) {
        context.addIssue({
          code: 'custom',
          path: ['renewalRule'],
          message: 'an open-ended lease cannot expose fixed-term lifecycle state',
        });
      }
      return;
    }

    const term = lease.currentTerm;
    if (term === null) {
      context.addIssue({
        code: 'custom',
        path: ['currentTerm'],
        message: 'a fixed-term auto-renewing lease requires its current term',
      });
      return;
    }
    if (
      term.effectiveFromGameDay < lease.effectiveFromGameDay ||
      (term.termNo === 1 && term.effectiveFromGameDay !== lease.effectiveFromGameDay)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['currentTerm', 'effectiveFromGameDay'],
        message: 'the current term must follow the lease start',
      });
    }

    const notice = lease.renewalNotice;
    if (
      notice !== null &&
      (notice.termNo !== term.termNo ||
        notice.publishedGameDay < term.effectiveFromGameDay ||
        notice.renewsOnGameDay !== term.effectiveToGameDay)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['renewalNotice'],
        message: 'a renewal notice must describe the current lease term',
      });
    }

    const review = lease.terminationReview;
    if (
      review !== null &&
      (lease.offerKind !== 'monthlyRent' || review.openedGameDay < lease.effectiveFromGameDay)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['terminationReview'],
        message: 'only an active monthly-rent lease can be under termination review',
      });
    }
  });

export const YearMonthSchema = z
  .object({
    year: TaxYearSchema,
    month: z.number().int().min(1).max(12),
  })
  .strict();

export const HousingLeaseArrearSchema = z
  .object({
    id: ResourceIdSchema,
    leaseId: ResourceIdSchema,
    rentChargeId: ResourceIdSchema,
    dueYearMonth: YearMonthSchema,
    originalKrw: PositiveKrwSchema,
    paidKrw: NonnegativeKrwSchema,
    remainingKrw: PositiveKrwSchema,
    createdGameDay: z.number().int().safe().nonnegative(),
  })
  .strict()
  .superRefine((arrear, context) => {
    if (BigInt(arrear.paidKrw) + BigInt(arrear.remainingKrw) !== BigInt(arrear.originalKrw)) {
      context.addIssue({
        code: 'custom',
        path: ['remainingKrw'],
        message: 'lease arrear amounts must reconcile with the original charge',
      });
    }
  });

function refineLeaseArrearWindow(
  arrears: readonly z.infer<typeof HousingLeaseArrearSchema>[],
  hasMore: boolean,
  totalLeaseArrearKrw: number,
  context: z.RefinementCtx,
  windowPath: 'activeArrears' | 'activeLeaseArrears',
): void {
  const windowTotal = arrears.reduce((total, arrear) => total + BigInt(arrear.remainingKrw), 0n);
  const completeTotal = BigInt(totalLeaseArrearKrw);
  const invalidCompleteWindow = !hasMore && windowTotal !== completeTotal;
  const invalidPartialWindow = hasMore && (arrears.length !== 20 || windowTotal >= completeTotal);
  if (invalidCompleteWindow || invalidPartialWindow) {
    context.addIssue({
      code: 'custom',
      path: ['totalLeaseArrearKrw'],
      message: 'lease arrear window must reconcile with the complete total',
    });
  }

  const ids = new Set<string>();
  const chargeIds = new Set<string>();
  const ordered = arrears.every((arrear, index) => {
    const previous = arrears[index - 1];
    ids.add(arrear.id);
    chargeIds.add(arrear.rentChargeId);
    return previous === undefined || compareLeaseArrearAge(previous, arrear) < 0;
  });
  if (ids.size !== arrears.length || chargeIds.size !== arrears.length || !ordered) {
    context.addIssue({
      code: 'custom',
      path: [windowPath],
      message: 'active lease arrears must be unique and ordered oldest first',
    });
  }
}

function compareLeaseArrearAge(
  left: z.infer<typeof HousingLeaseArrearSchema>,
  right: z.infer<typeof HousingLeaseArrearSchema>,
): number {
  const leftMonth = left.dueYearMonth.year * 12 + left.dueYearMonth.month;
  const rightMonth = right.dueYearMonth.year * 12 + right.dueYearMonth.month;
  if (leftMonth !== rightMonth) return leftMonth - rightMonth;
  if (left.createdGameDay !== right.createdGameDay) {
    return left.createdGameDay - right.createdGameDay;
  }
  const leftId = BigInt(left.id);
  const rightId = BigInt(right.id);
  return leftId < rightId ? -1 : leftId > rightId ? 1 : 0;
}

export const LifeHouseholdSchema = z
  .object({
    id: ResourceIdSchema,
    memberCount: z.number().int().safe().positive(),
    dependentCount: z.number().int().safe().nonnegative(),
    taxDependentEligibleCount: z.number().int().safe().nonnegative(),
  })
  .strict()
  .superRefine((household, context) => {
    if (household.dependentCount >= household.memberCount) {
      context.addIssue({
        code: 'custom',
        path: ['dependentCount'],
        message: 'dependent count must exclude the player',
      });
    }
    if (household.taxDependentEligibleCount >= household.memberCount) {
      context.addIssue({
        code: 'custom',
        path: ['taxDependentEligibleCount'],
        message: 'tax-dependent count must exclude the player',
      });
    }
  });

export const LifeResidenceSchema = z
  .object({
    id: ResourceIdSchema,
    regionKey: z.string().min(1).max(64),
    tenureKind: ResidenceTenureKindSchema,
    propertyHoldingId: ResourceIdSchema.nullable(),
    effectiveFromGameDay: z.number().int().safe().nonnegative(),
  })
  .strict()
  .superRefine((residence, context) => {
    if ((residence.tenureKind === 'owner') !== (residence.propertyHoldingId !== null)) {
      context.addIssue({
        code: 'custom',
        path: ['propertyHoldingId'],
        message: 'only an owner residence must identify its property holding',
      });
    }
  });

export const HousingPurchaseCapabilitySchema = z.enum(['unavailable', 'ownerOccupiedSingleHome']);
export const HousingPropertyHoldingStatusSchema = z.literal('active');
export const HousingPropertyHoldingPurposeSchema = z.literal('ownerOccupied');

export const HousingPropertyHoldingSchema = z
  .object({
    id: ResourceIdSchema,
    listingId: HousingListingIdSchema,
    status: HousingPropertyHoldingStatusSchema,
    purpose: HousingPropertyHoldingPurposeSchema,
    regionKey: HousingRegionKeySchema,
    propertyType: HousingPropertyTypeSchema,
    exclusiveAreaSquareMeters: z.number().int().safe().positive(),
    acquiredGameDay: z.number().int().safe().nonnegative(),
    acquisitionPriceKrw: PositiveKrwSchema,
    acquisitionIncidentalCostKrw: PositiveKrwSchema,
    bookValueKrw: PositiveKrwSchema,
    mortgageLoanId: ResourceIdSchema.nullable(),
  })
  .strict()
  .superRefine((holding, context) => {
    if (holding.bookValueKrw !== holding.acquisitionPriceKrw) {
      context.addIssue({
        code: 'custom',
        path: ['bookValueKrw'],
        message: 'C3 property book value must equal its immutable acquisition price',
      });
    }
  });

export const LifeBudgetBandSchema = z
  .object({
    id: ResourceIdSchema,
    bandKey: z.string().min(1).max(64),
    displayName: z.string().min(1).max(120),
    factorPpm: z.number().int().safe().positive(),
  })
  .strict();

export const LivingCostMonthItemSchema = z
  .object({
    category: LivingCostCategorySchema,
    essential: z.boolean(),
    bandId: ResourceIdSchema,
    baseMonthlyKrw: NonnegativeKrwSchema,
    baseCpiIndex: z.number().int().safe().positive(),
    regionFactorPpm: z.number().int().safe().positive(),
    householdFactorPpm: z.number().int().safe().positive(),
    budgetFactorPpm: z.number().int().safe().positive(),
    tenureReplacementFactorPpm: z.number().int().safe().min(0).max(1_000_000),
    grossKrw: NonnegativeKrwSchema,
    paidKrw: NonnegativeKrwSchema,
    arrearKrw: NonnegativeKrwSchema,
  })
  .strict()
  .superRefine((item, context) => {
    if (BigInt(item.paidKrw) + BigInt(item.arrearKrw) > BigInt(item.grossKrw)) {
      context.addIssue({
        code: 'custom',
        path: ['paidKrw'],
        message: 'paid and arrear amounts cannot exceed the gross cost',
      });
    }
    if (!item.essential && item.arrearKrw !== 0) {
      context.addIssue({
        code: 'custom',
        path: ['arrearKrw'],
        message: 'optional costs cannot create arrears',
      });
    }
    if (
      item.category === 'housing' &&
      (BigInt(item.baseMonthlyKrw) * BigInt(item.tenureReplacementFactorPpm)) % 1_000_000n !== 0n
    ) {
      context.addIssue({
        code: 'custom',
        path: ['tenureReplacementFactorPpm'],
        message: 'housing replacement must produce an exact KRW base amount',
      });
    }
  });

export const LivingCostMonthSchema = z
  .object({
    id: ResourceIdSchema,
    profileId: ResourceIdSchema,
    profileKey: z.string().min(1).max(96),
    currentCpiIndex: z.number().int().safe().positive(),
    prorationScale: z.literal(377_580),
    prorationUnits: z.number().int().safe().positive(),
    prorationDays: z.number().int().safe().min(1).max(31),
    daysInMonth: z.number().int().safe().min(28).max(31),
    yearMonth: YearMonthSchema,
    activationGameDay: z.number().int().safe().nonnegative(),
    settlementGameDay: z.number().int().safe().nonnegative(),
    settled: z.boolean(),
    totalGrossKrw: NonnegativeKrwSchema,
    totalPaidKrw: NonnegativeKrwSchema,
    totalArrearKrw: NonnegativeKrwSchema,
    items: z.array(LivingCostMonthItemSchema).length(LivingCostCategorySchema.options.length),
  })
  .strict()
  .superRefine((month, context) => {
    const unitsPerDay = month.prorationScale / month.daysInMonth;
    if (
      !Number.isInteger(unitsPerDay) ||
      month.prorationDays > month.daysInMonth ||
      month.prorationUnits !== month.prorationDays * unitsPerDay
    ) {
      context.addIssue({
        code: 'custom',
        path: ['prorationUnits'],
        message: 'proration units must match the canonical calendar-day scale',
      });
    }
    const itemCategories = month.items.map((item) => item.category);
    const categories = new Set(itemCategories);
    if (
      categories.size !== LivingCostCategorySchema.options.length ||
      !hasCanonicalLifeCategoryOrder(itemCategories)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['items'],
        message: 'a living-cost month must contain every category in canonical order',
      });
    }
    for (const [index, item] of month.items.entries()) {
      const outcome = BigInt(item.paidKrw) + BigInt(item.arrearKrw);
      if (!month.settled && outcome !== 0n) {
        context.addIssue({
          code: 'custom',
          path: ['items', index, 'paidKrw'],
          message: 'an unsettled month cannot contain payment outcomes',
        });
      }
      if (month.settled && item.essential && outcome !== BigInt(item.grossKrw)) {
        context.addIssue({
          code: 'custom',
          path: ['items', index, 'paidKrw'],
          message: 'a settled essential cost must be fully split into paid and arrear amounts',
        });
      }
    }
    const totals = month.items.reduce(
      (sum, item) => ({
        gross: sum.gross + BigInt(item.grossKrw),
        paid: sum.paid + BigInt(item.paidKrw),
        arrear: sum.arrear + BigInt(item.arrearKrw),
      }),
      { gross: 0n, paid: 0n, arrear: 0n },
    );
    if (totals.gross !== BigInt(month.totalGrossKrw)) {
      context.addIssue({
        code: 'custom',
        path: ['totalGrossKrw'],
        message: 'gross total mismatch',
      });
    }
    if (totals.paid !== BigInt(month.totalPaidKrw)) {
      context.addIssue({ code: 'custom', path: ['totalPaidKrw'], message: 'paid total mismatch' });
    }
    if (totals.arrear !== BigInt(month.totalArrearKrw)) {
      context.addIssue({
        code: 'custom',
        path: ['totalArrearKrw'],
        message: 'arrear total mismatch',
      });
    }
  });

export const EssentialArrearSchema = z
  .object({
    id: ResourceIdSchema,
    dueYearMonth: YearMonthSchema,
    category: LivingCostCategorySchema,
    originalKrw: PositiveKrwSchema,
    remainingKrw: PositiveKrwSchema,
  })
  .strict()
  .superRefine((arrear, context) => {
    if (arrear.remainingKrw > arrear.originalKrw) {
      context.addIssue({
        code: 'custom',
        path: ['remainingKrw'],
        message: 'remaining arrear cannot exceed its original amount',
      });
    }
  });

export const CreditBandSchema = z.enum(['prime', 'standard', 'limited', 'distressed', 'insolvent']);

export const CreditReasonSchema = z.enum([
  'modelUnavailable',
  'activeDefault',
  'activeDelinquency',
  'cleanHistory',
]);

export const LoanProductKindSchema = z.enum([
  'studentLoan',
  'unsecuredLoan',
  'leaseDepositLoan',
  'mortgage',
  'legacyDebt',
]);

export const LoanRateStatusSchema = z.enum(['available', 'rateUnavailable']);

export const LoanLenderSectorSchema = z.enum(['bank', 'nonBank']);
export const LoanRateTypeSchema = z.enum(['fixed', 'variable']);
export const LoanRateReferenceSchema = z.enum(['treasury3m']);
export const LoanRateResetRuleSchema = z.enum(['none', 'monthlyDay1']);
export const LoanDayCountRuleSchema = z.enum(['actual365']);
export const LoanRepaymentMethodSchema = z.enum(['equalPrincipal', 'levelPayment', 'bullet']);
export const LoanPaymentCalendarSchema = z.enum(['monthEnd']);
export const LoanPrepaymentEffectSchema = z.enum(['reduceTerm', 'recalculatePayment']);
export const LoanProductProvenanceSchema = z.enum(['gameBalance']);

export const LoanProductSchema = z
  .object({
    id: ResourceIdSchema,
    key: z.string().min(1).max(96),
    displayName: z.string().min(1).max(80),
    kind: LoanProductKindSchema,
    lenderSector: LoanLenderSectorSchema,
    rateStatus: LoanRateStatusSchema,
    rateType: LoanRateTypeSchema,
    currentAnnualRateBp: z.number().int().nonnegative().nullable(),
    referenceRateKey: LoanRateReferenceSchema.nullable(),
    spreadBp: z.number().int().min(-10_000).max(10_000).nullable(),
    minimumAnnualRateBp: z.number().int().nonnegative(),
    maximumAnnualRateBp: z.number().int().nonnegative(),
    rateResetRule: LoanRateResetRuleSchema,
    dayCountRule: LoanDayCountRuleSchema,
    repaymentMethod: LoanRepaymentMethodSchema,
    termMonths: z.number().int().min(1).max(65_535),
    paymentCalendar: LoanPaymentCalendarSchema,
    graceMonths: z.number().int().nonnegative().max(65_535),
    minimumPrincipalKrw: PositiveKrwSchema,
    maximumPrincipalKrw: PositiveKrwSchema,
    prepaymentFeePpm: z.number().int().nonnegative().max(1_000_000),
    prepaymentEffect: LoanPrepaymentEffectSchema,
    startingEligible: z.boolean(),
    quoteEligible: z.boolean(),
    executionEligible: z.boolean(),
    prepaymentAllowed: z.boolean(),
    dsrIncluded: z.boolean(),
    provenance: LoanProductProvenanceSchema,
  })
  .strict()
  .superRefine((product, context) => {
    const rateMatchesAvailability =
      (product.rateStatus === 'available' && product.currentAnnualRateBp !== null) ||
      (product.rateStatus === 'rateUnavailable' && product.currentAnnualRateBp === null);
    if (!rateMatchesAvailability) {
      context.addIssue({
        code: 'custom',
        path: ['currentAnnualRateBp'],
        message: 'loan product rate must match its availability',
      });
    }
    if (product.minimumAnnualRateBp > product.maximumAnnualRateBp) {
      context.addIssue({
        code: 'custom',
        path: ['maximumAnnualRateBp'],
        message: 'loan product rate bounds are reversed',
      });
    }
    if (
      product.currentAnnualRateBp !== null &&
      (product.currentAnnualRateBp < product.minimumAnnualRateBp ||
        product.currentAnnualRateBp > product.maximumAnnualRateBp)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['currentAnnualRateBp'],
        message: 'current loan product rate is outside its published bounds',
      });
    }
    const fixedShape =
      product.rateType === 'fixed' &&
      product.referenceRateKey === null &&
      product.spreadBp === null &&
      product.rateResetRule === 'none' &&
      product.rateStatus === 'available' &&
      product.currentAnnualRateBp === product.minimumAnnualRateBp &&
      product.currentAnnualRateBp === product.maximumAnnualRateBp;
    const variableShape =
      product.rateType === 'variable' &&
      product.referenceRateKey === 'treasury3m' &&
      product.spreadBp !== null &&
      product.rateResetRule === 'monthlyDay1';
    if (!fixedShape && !variableShape) {
      context.addIssue({
        code: 'custom',
        path: ['rateType'],
        message: 'loan product rate terms do not match their type',
      });
    }
    if (product.minimumPrincipalKrw > product.maximumPrincipalKrw) {
      context.addIssue({
        code: 'custom',
        path: ['maximumPrincipalKrw'],
        message: 'loan product principal bounds are reversed',
      });
    }
  });

export const HousingMortgageProductSchema = LoanProductSchema.refine(
  (
    product,
  ): product is z.infer<typeof LoanProductSchema> & {
    readonly kind: 'mortgage';
    readonly lenderSector: 'bank';
    readonly startingEligible: false;
    readonly quoteEligible: true;
    readonly executionEligible: true;
    readonly prepaymentAllowed: true;
    readonly dsrIncluded: true;
  } =>
    product.kind === 'mortgage' &&
    product.lenderSector === 'bank' &&
    !product.startingEligible &&
    product.quoteEligible &&
    product.executionEligible &&
    product.prepaymentAllowed &&
    product.dsrIncluded,
  'a mortgage must be a bank housing-purchase product unavailable as starting debt',
);

export const LoanProductCatalogSchema = z
  .object({
    creditModelVersionId: ResourceIdSchema.nullable(),
    products: z.array(LoanProductSchema).max(16),
  })
  .strict()
  .superRefine((catalog, context) => {
    if ((catalog.creditModelVersionId === null) !== (catalog.products.length === 0)) {
      context.addIssue({
        code: 'custom',
        path: ['creditModelVersionId'],
        message: 'credit model availability must match the public loan catalog',
      });
    }
    const ids = new Set(catalog.products.map((product) => product.id));
    const keys = new Set(catalog.products.map((product) => product.key));
    if (ids.size !== catalog.products.length || keys.size !== catalog.products.length) {
      context.addIssue({
        code: 'custom',
        path: ['products'],
        message: 'loan product IDs and keys must be unique',
      });
    }
    for (const kind of ['studentLoan', 'unsecuredLoan'] as const) {
      const count = catalog.products.filter(
        (product) => product.kind === kind && product.startingEligible,
      ).length;
      if (catalog.creditModelVersionId !== null && count !== 1) {
        context.addIssue({
          code: 'custom',
          path: ['products'],
          message: `loan catalog must have one starting ${kind} product`,
        });
      }
    }
    if (
      catalog.products.some(
        (product) =>
          !['studentLoan', 'unsecuredLoan', 'leaseDepositLoan', 'mortgage'].includes(product.kind),
      )
    ) {
      context.addIssue({
        code: 'custom',
        path: ['products'],
        message: 'the public loan catalog contains an unsupported product kind',
      });
    }
    if (
      catalog.products.some(
        (product) =>
          product.kind === 'leaseDepositLoan' &&
          (product.startingEligible || !product.quoteEligible || !product.executionEligible),
      )
    ) {
      context.addIssue({
        code: 'custom',
        path: ['products'],
        message: 'a lease-deposit loan must be housing-executable and unavailable as starting debt',
      });
    }
    if (catalog.products.filter((product) => product.kind === 'leaseDepositLoan').length > 1) {
      context.addIssue({
        code: 'custom',
        path: ['products'],
        message: 'the public catalog can expose at most one lease-deposit loan product',
      });
    }
    const mortgages = catalog.products.filter((product) => product.kind === 'mortgage');
    if (
      mortgages.length > 1 ||
      mortgages.some((product) => !HousingMortgageProductSchema.safeParse(product).success)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['products'],
        message: 'the public catalog can expose at most one housing-purchase mortgage product',
      });
    }
  });

export const LoanContractStatusSchema = z.enum([
  'pending',
  'active',
  'delinquent',
  'defaulted',
  'paidOff',
  'restructured',
  'discharged',
  'chargedOff',
  'cancelled',
]);

export const LoanSummarySchema = z
  .object({
    id: ResourceIdSchema,
    productVersionId: ResourceIdSchema,
    productKind: LoanProductKindSchema,
    displayName: z.string().min(1).max(80),
    rateStatus: LoanRateStatusSchema,
    currentAnnualRateBp: z.number().int().nonnegative().nullable(),
    status: LoanContractStatusSchema,
    remainingPrincipalKrw: NonnegativeKrwSchema,
    overdueKrw: NonnegativeKrwSchema,
    readOnly: z.boolean(),
  })
  .strict()
  .superRefine((loan, context) => {
    const invalidRate =
      (loan.rateStatus === 'available' && loan.currentAnnualRateBp === null) ||
      (loan.rateStatus === 'rateUnavailable' && loan.currentAnnualRateBp !== null);
    if (invalidRate) {
      context.addIssue({
        code: 'custom',
        path: ['currentAnnualRateBp'],
        message: 'loan rate value must match its availability',
      });
    }
    if (
      loan.productKind === 'legacyDebt' &&
      (!loan.readOnly || loan.rateStatus !== 'rateUnavailable')
    ) {
      context.addIssue({
        code: 'custom',
        path: ['readOnly'],
        message: 'legacy debt must remain read-only with an unavailable rate',
      });
    }
  });

export const NextLoanInstallmentSchema = z
  .object({
    loanId: ResourceIdSchema,
    installmentNo: z.number().int().min(1).max(65_535),
    dueGameDay: z.number().int().nonnegative(),
    feeKrw: NonnegativeKrwSchema,
    interestKrw: NonnegativeKrwSchema,
    principalKrw: NonnegativeKrwSchema,
    remainingDueKrw: NonnegativeKrwSchema,
  })
  .strict()
  .superRefine((installment, context) => {
    const scheduled =
      BigInt(installment.feeKrw) +
      BigInt(installment.interestKrw) +
      BigInt(installment.principalKrw);
    if (BigInt(installment.remainingDueKrw) > scheduled) {
      context.addIssue({
        code: 'custom',
        path: ['remainingDueKrw'],
        message: 'remaining installment amount cannot exceed its scheduled amount',
      });
    }
  });

const LifeSummaryFields = {
  rateStatus: LifeRateStatusSchema,
  currentMonth: LivingCostMonthSchema.nullable(),
  activeArrears: z.array(EssentialArrearSchema).max(20),
  hasMoreActiveArrears: z.boolean(),
  totalEssentialArrearKrw: NonnegativeKrwSchema,
} as const;

function refineLifeArrears(
  life: {
    readonly activeArrears: readonly {
      readonly id: string;
      readonly dueYearMonth: { readonly year: number; readonly month: number };
      readonly category: string;
      readonly remainingKrw: number;
    }[];
    readonly hasMoreActiveArrears: boolean;
    readonly totalEssentialArrearKrw: number;
  },
  context: z.RefinementCtx,
): void {
  const total = life.activeArrears.reduce((sum, arrear) => sum + BigInt(arrear.remainingKrw), 0n);
  const completeTotal = BigInt(life.totalEssentialArrearKrw);
  const invalidCompleteWindow = !life.hasMoreActiveArrears && total !== completeTotal;
  const invalidPartialWindow =
    life.hasMoreActiveArrears && (life.activeArrears.length !== 20 || total >= completeTotal);
  if (invalidCompleteWindow || invalidPartialWindow) {
    context.addIssue({
      code: 'custom',
      path: ['totalEssentialArrearKrw'],
      message: 'active arrear window must reconcile with the complete total',
    });
  }
  const ids = new Set(life.activeArrears.map((arrear) => arrear.id));
  const ordered = life.activeArrears.every((arrear, index, arrears) => {
    const previous = arrears[index - 1];
    return previous === undefined || compareArrearPriority(previous, arrear) < 0;
  });
  if (ids.size !== life.activeArrears.length || !ordered) {
    context.addIssue({
      code: 'custom',
      path: ['activeArrears'],
      message: 'active arrears must be unique and use canonical payment priority',
    });
  }
}

function compareArrearPriority(
  left: {
    readonly id: string;
    readonly dueYearMonth: { readonly year: number; readonly month: number };
    readonly category: string;
  },
  right: {
    readonly id: string;
    readonly dueYearMonth: { readonly year: number; readonly month: number };
    readonly category: string;
  },
): number {
  const leftMonth = left.dueYearMonth.year * 12 + left.dueYearMonth.month;
  const rightMonth = right.dueYearMonth.year * 12 + right.dueYearMonth.month;
  if (leftMonth !== rightMonth) return leftMonth - rightMonth;
  const categoryOrder =
    LivingCostCategorySchema.options.indexOf(
      left.category as (typeof LivingCostCategorySchema.options)[number],
    ) -
    LivingCostCategorySchema.options.indexOf(
      right.category as (typeof LivingCostCategorySchema.options)[number],
    );
  if (categoryOrder !== 0) return categoryOrder;
  const leftId = BigInt(left.id);
  const rightId = BigInt(right.id);
  return leftId < rightId ? -1 : leftId > rightId ? 1 : 0;
}

interface LifeCreditAndLoans {
  readonly creditBand: z.infer<typeof CreditBandSchema> | null;
  readonly creditReasons: readonly z.infer<typeof CreditReasonSchema>[];
  readonly activeLoans: readonly z.infer<typeof LoanSummarySchema>[];
  readonly nextLoanInstallment: z.infer<typeof NextLoanInstallmentSchema> | null;
  readonly totalLoanBalanceKrw: number;
}

function refineLifeCreditAndLoans(life: LifeCreditAndLoans, context: z.RefinementCtx): void {
  refineCreditReasons(life, context);
  refineActiveLoanSummary(life, context);
}

export const CreditResponseSchema = z
  .object({
    creditBand: CreditBandSchema.nullable(),
    creditReasons: z.array(CreditReasonSchema).max(8),
    activeLoans: z.array(LoanSummarySchema).max(8),
    nextLoanInstallment: NextLoanInstallmentSchema.nullable(),
    totalLoanBalanceKrw: NonnegativeKrwSchema,
  })
  .strict()
  .superRefine(refineLifeCreditAndLoans);

function refineCreditReasons(life: LifeCreditAndLoans, context: z.RefinementCtx): void {
  const reasonOrder = CreditReasonSchema.options;
  const uniqueReasons = new Set(life.creditReasons);
  const reasonsOrdered = life.creditReasons.every((reason, index, reasons) => {
    const previous = reasons[index - 1];
    return previous === undefined || reasonOrder.indexOf(previous) < reasonOrder.indexOf(reason);
  });
  if (uniqueReasons.size !== life.creditReasons.length || !reasonsOrdered) {
    context.addIssue({
      code: 'custom',
      path: ['creditReasons'],
      message: 'credit reasons must be unique and use canonical priority',
    });
  }

  const modelUnavailable = uniqueReasons.has('modelUnavailable');
  if ((life.creditBand === null) !== modelUnavailable) {
    context.addIssue({
      code: 'custom',
      path: ['creditBand'],
      message: 'credit band availability must match its reason',
    });
  }
  const hasDefault = life.activeLoans.some((loan) => loan.status === 'defaulted');
  const hasDelinquency = life.activeLoans.some((loan) => loan.status === 'delinquent');
  const cleanHistory = uniqueReasons.has('cleanHistory');
  const shouldHaveCleanHistory = !modelUnavailable && !hasDefault && !hasDelinquency;
  if (
    uniqueReasons.has('activeDefault') !== hasDefault ||
    uniqueReasons.has('activeDelinquency') !== hasDelinquency ||
    cleanHistory !== shouldHaveCleanHistory
  ) {
    context.addIssue({
      code: 'custom',
      path: ['creditReasons'],
      message: 'credit reasons must reconcile with active loan state',
    });
  }
}

function refineActiveLoanSummary(life: LifeCreditAndLoans, context: z.RefinementCtx): void {
  const activeIds = new Set<string>();
  let previousId: bigint | undefined;
  for (const [index, loan] of life.activeLoans.entries()) {
    const id = BigInt(loan.id);
    const activeStatus = ['active', 'delinquent', 'defaulted', 'restructured'].includes(
      loan.status,
    );
    if (activeIds.has(loan.id) || (previousId !== undefined && id <= previousId) || !activeStatus) {
      context.addIssue({
        code: 'custom',
        path: ['activeLoans', index],
        message: 'active loans must be unique, ordered, and non-terminal',
      });
    }
    activeIds.add(loan.id);
    previousId = id;
  }
  if (life.activeLoans.length > 0 && life.totalLoanBalanceKrw === 0) {
    context.addIssue({
      code: 'custom',
      path: ['totalLoanBalanceKrw'],
      message: 'an active loan requires a positive aggregate balance',
    });
  }
  if (life.nextLoanInstallment !== null && !activeIds.has(life.nextLoanInstallment.loanId)) {
    context.addIssue({
      code: 'custom',
      path: ['nextLoanInstallment', 'loanId'],
      message: 'next installment must reference an active loan summary',
    });
  }
}

interface LifePropertyHoldings {
  readonly residence: z.infer<typeof LifeResidenceSchema> | null;
  readonly activePropertyHoldings: readonly z.infer<typeof HousingPropertyHoldingSchema>[];
  readonly hasMoreActivePropertyHoldings: boolean;
  readonly totalPropertyBookValueKrw: number;
  readonly activeLoans: readonly z.infer<typeof LoanSummarySchema>[];
}

function refineLifePropertyHoldings(life: LifePropertyHoldings, context: z.RefinementCtx): void {
  const holdingIds = refinePropertyHoldingWindow(life, context);
  refinePropertyResidence(life, holdingIds, context);
  refinePropertyLoans(life, context);
}

function refinePropertyHoldingWindow(
  life: LifePropertyHoldings,
  context: z.RefinementCtx,
): ReadonlySet<string> {
  const holdingIds = new Set<string>();
  const listingIds = new Set<string>();
  let previousId: bigint | undefined;
  for (const [index, holding] of life.activePropertyHoldings.entries()) {
    const id = BigInt(holding.id);
    if (
      holdingIds.has(holding.id) ||
      listingIds.has(holding.listingId) ||
      (previousId !== undefined && id <= previousId)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['activePropertyHoldings', index],
        message: 'active property holdings must be unique and ordered by ID',
      });
    }
    holdingIds.add(holding.id);
    listingIds.add(holding.listingId);
    previousId = id;
  }

  const visibleBookValue = life.activePropertyHoldings.reduce(
    (sum, holding) => sum + BigInt(holding.bookValueKrw),
    0n,
  );
  const completeBookValue = BigInt(life.totalPropertyBookValueKrw);
  const invalidCompleteWindow =
    !life.hasMoreActivePropertyHoldings && visibleBookValue !== completeBookValue;
  const invalidPartialWindow =
    life.hasMoreActivePropertyHoldings &&
    (life.activePropertyHoldings.length !== 4 || visibleBookValue >= completeBookValue);
  if (invalidCompleteWindow || invalidPartialWindow) {
    context.addIssue({
      code: 'custom',
      path: ['totalPropertyBookValueKrw'],
      message: 'active property holding window must reconcile with its complete book value',
    });
  }
  return holdingIds;
}

function refinePropertyResidence(
  life: LifePropertyHoldings,
  holdingIds: ReadonlySet<string>,
  context: z.RefinementCtx,
): void {
  const ownerHoldingId = life.residence?.propertyHoldingId ?? null;
  if (
    (ownerHoldingId === null && life.activePropertyHoldings.length !== 0) ||
    (ownerHoldingId !== null && !holdingIds.has(ownerHoldingId))
  ) {
    context.addIssue({
      code: 'custom',
      path: ['residence', 'propertyHoldingId'],
      message: 'the owner residence must identify the active owner-occupied holding',
    });
  }
}

function refinePropertyLoans(life: LifePropertyHoldings, context: z.RefinementCtx): void {
  const activeLoansById = new Map(life.activeLoans.map((loan) => [loan.id, loan]));
  const linkedMortgageLoanIds = new Set<string>();
  for (const [index, holding] of life.activePropertyHoldings.entries()) {
    if (holding.mortgageLoanId === null) continue;
    const loan = activeLoansById.get(holding.mortgageLoanId);
    if (loan?.productKind !== 'mortgage' || linkedMortgageLoanIds.has(holding.mortgageLoanId)) {
      context.addIssue({
        code: 'custom',
        path: ['activePropertyHoldings', index, 'mortgageLoanId'],
        message: 'a property lien must identify one active mortgage loan',
      });
    }
    linkedMortgageLoanIds.add(holding.mortgageLoanId);
  }
  if (
    life.activeLoans.some(
      (loan) => loan.productKind === 'mortgage' && !linkedMortgageLoanIds.has(loan.id),
    )
  ) {
    context.addIssue({
      code: 'custom',
      path: ['activeLoans'],
      message: 'every active mortgage must be linked from one active property holding',
    });
  }
}

function refineSnapshotPropertyDates(
  snapshot: {
    readonly gameDay: number;
    readonly life: {
      readonly activePropertyHoldings: readonly z.infer<typeof HousingPropertyHoldingSchema>[];
    };
  },
  context: z.RefinementCtx,
): void {
  if (
    snapshot.life.activePropertyHoldings.some(
      (holding) => holding.acquiredGameDay > snapshot.gameDay,
    )
  ) {
    context.addIssue({
      code: 'custom',
      path: ['life', 'activePropertyHoldings'],
      message: 'an active property holding cannot be acquired after the snapshot game day',
    });
  }
}

function refineSnapshotWelfareDates(
  snapshot: {
    readonly gameDay: number;
    readonly life: {
      readonly activeWelfareApplications: readonly z.infer<typeof ActiveWelfareApplicationSchema>[];
    };
  },
  context: z.RefinementCtx,
): void {
  for (const [index, application] of snapshot.life.activeWelfareApplications.entries()) {
    if (
      application.applicationGameDay > snapshot.gameDay ||
      application.approvalGameDay > snapshot.gameDay ||
      application.nextPayment === null ||
      application.nextPayment.dueGameDay <= snapshot.gameDay
    ) {
      context.addIssue({
        code: 'custom',
        path: ['life', 'activeWelfareApplications', index],
        message: 'active welfare dates must surround the committed snapshot day',
      });
    }
  }
}

function refineSnapshotLifeEventDates(
  snapshot: {
    readonly gameDay: number;
    readonly life: {
      readonly pendingEvents: readonly z.infer<typeof PendingLifeEventSchema>[];
    };
  },
  context: z.RefinementCtx,
): void {
  for (const [index, event] of snapshot.life.pendingEvents.entries()) {
    if (event.offeredGameDay > snapshot.gameDay || event.expiresGameDay <= snapshot.gameDay) {
      context.addIssue({
        code: 'custom',
        path: ['life', 'pendingEvents', index],
        message: 'pending life event dates must contain the committed snapshot day',
      });
    }
  }
}

function refineSnapshotInsuranceDates(
  snapshot: {
    readonly gameDay: number;
    readonly life: {
      readonly activeInsuranceContracts: readonly z.infer<typeof InsuranceContractSchema>[];
      readonly pendingInsuranceClaims: readonly z.infer<typeof PendingInsuranceClaimSchema>[];
      readonly pendingEvents: readonly z.infer<typeof PendingLifeEventSchema>[];
    };
  },
  context: z.RefinementCtx,
): void {
  const pendingEventIds = new Set(snapshot.life.pendingEvents.map((event) => event.id));
  for (const [index, contract] of snapshot.life.activeInsuranceContracts.entries()) {
    if (
      contract.status !== 'active' ||
      contract.coverageStartGameDay > snapshot.gameDay ||
      contract.coverageEndExclusive <= snapshot.gameDay ||
      (contract.nextPremiumDueGameDay !== null &&
        contract.nextPremiumDueGameDay <= snapshot.gameDay)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['life', 'activeInsuranceContracts', index],
        message: 'active insurance contract dates must contain the committed snapshot day',
      });
    }
  }
  for (const [index, claim] of snapshot.life.pendingInsuranceClaims.entries()) {
    if (
      claim.offeredGameDay > snapshot.gameDay ||
      (claim.status === 'candidate' && !pendingEventIds.has(claim.eventId)) ||
      (claim.status === 'ready' && claim.filingDeadlineGameDay <= snapshot.gameDay)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['life', 'pendingInsuranceClaims', index],
        message: 'pending insurance claim must match its event and committed filing window',
      });
    }
  }
}

const WelfareProgramKeySchema = z
  .string()
  .regex(/^[a-z][a-zA-Z0-9]{0,63}$/, 'welfare program key must be canonical');
const WelfareConditionCodeSchema = z
  .string()
  .regex(/^[a-z][a-zA-Z0-9]{0,63}$/, 'welfare condition code must be canonical');
const WelfareFactFingerprintSchema = z
  .string()
  .regex(/^[0-9a-f]{64}$/, 'welfare fact fingerprint must be lowercase SHA-256 hex');

export const WelfareEvaluationStatusSchema = z.enum(['eligible', 'ineligible', 'indeterminate']);
export const WelfareConditionOutcomeSchema = z.enum(['passed', 'failed', 'unknown']);
export const WelfareApplicationStatusSchema = z.enum([
  'applied',
  'approved',
  'rejected',
  'active',
  'exhausted',
  'terminated',
]);
export const WelfarePaymentStatusSchema = z.enum(['pending', 'paid', 'cancelled']);

export const WelfareConditionResultSchema = z
  .object({
    code: WelfareConditionCodeSchema,
    label: z.string().min(1).max(120),
    outcome: WelfareConditionOutcomeSchema,
  })
  .strict();

export const WelfarePaymentSchema = z
  .object({
    id: ResourceIdSchema,
    paymentNo: z.number().int().safe().positive().max(65_535),
    amountKrw: PositiveKrwSchema,
    dueGameDay: z.number().int().safe().nonnegative(),
    status: WelfarePaymentStatusSchema,
  })
  .strict();

export const WelfareApplicationSummarySchema = z
  .object({
    id: ResourceIdSchema,
    status: WelfareApplicationStatusSchema,
    applicationGameDay: z.number().int().safe().nonnegative(),
    approvalGameDay: z.number().int().safe().nonnegative().nullable(),
    paidKrw: NonnegativeKrwSchema,
  })
  .strict();

export const WelfareProgramSchema = z
  .object({
    id: ResourceIdSchema,
    programKey: WelfareProgramKeySchema,
    displayName: z.string().min(1).max(120),
    benefitKrw: PositiveKrwSchema,
    paymentDelayGameDays: z.number().int().safe().positive().max(365),
    evaluationStatus: WelfareEvaluationStatusSchema,
    factFingerprint: WelfareFactFingerprintSchema,
    conditions: z.array(WelfareConditionResultSchema).min(1).max(32),
    applicationAvailable: z.boolean(),
    latestApplication: WelfareApplicationSummarySchema.nullable(),
    nextPayment: WelfarePaymentSchema.nullable(),
  })
  .strict()
  .superRefine((program, context) => {
    if (!hasUniqueWelfareConditionCodes(program.conditions)) {
      context.addIssue({
        code: 'custom',
        path: ['conditions'],
        message: 'welfare condition codes must be unique',
      });
    }
    if (
      program.applicationAvailable &&
      (program.evaluationStatus !== 'eligible' || program.latestApplication !== null)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['applicationAvailable'],
        message: 'welfare application availability must match server evaluation and history',
      });
    }
    if (program.latestApplication === null && program.nextPayment !== null) {
      context.addIssue({
        code: 'custom',
        path: ['nextPayment'],
        message: 'a welfare payment must belong to the latest application',
      });
    }
    if (
      program.latestApplication?.status === 'active' &&
      (program.nextPayment === null || program.nextPayment.status !== 'pending')
    ) {
      context.addIssue({
        code: 'custom',
        path: ['nextPayment'],
        message: 'an active welfare application requires its pending payment',
      });
    }
    if (
      program.latestApplication?.status === 'exhausted' &&
      (program.nextPayment !== null || program.latestApplication.paidKrw !== program.benefitKrw)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['latestApplication'],
        message: 'an exhausted welfare application must reconcile with its paid benefit',
      });
    }
    if (
      program.nextPayment !== null &&
      (program.nextPayment.paymentNo !== 1 ||
        program.nextPayment.amountKrw !== program.benefitKrw ||
        program.latestApplication === null ||
        program.nextPayment.dueGameDay !==
          program.latestApplication.applicationGameDay + program.paymentDelayGameDays)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['nextPayment'],
        message: 'the welfare payment must match the published benefit schedule',
      });
    }
  });

export const WelfareProgramsResponseSchema = z
  .object({
    componentVersionId: ResourceIdSchema,
    gameDay: z.number().int().safe().nonnegative(),
    programs: z.array(WelfareProgramSchema).max(16),
  })
  .strict()
  .superRefine((response, context) => {
    let previous: z.infer<typeof WelfareProgramSchema> | undefined;
    const ids = new Set<string>();
    const keys = new Set<string>();
    for (const [index, program] of response.programs.entries()) {
      const canonical =
        previous === undefined ||
        previous.programKey < program.programKey ||
        (previous.programKey === program.programKey && BigInt(previous.id) < BigInt(program.id));
      if (!canonical || ids.has(program.id) || keys.has(program.programKey)) {
        context.addIssue({
          code: 'custom',
          path: ['programs', index],
          message: 'welfare programs must be unique and use canonical key and ID order',
        });
      }
      ids.add(program.id);
      keys.add(program.programKey);
      previous = program;
    }
  });

export const ActiveWelfareApplicationSchema = z
  .object({
    applicationId: ResourceIdSchema,
    programVersionId: ResourceIdSchema,
    programKey: WelfareProgramKeySchema,
    displayName: z.string().min(1).max(120),
    status: z.literal('active'),
    applicationGameDay: z.number().int().safe().nonnegative(),
    approvalGameDay: z.number().int().safe().nonnegative(),
    benefitKrw: PositiveKrwSchema,
    paidKrw: NonnegativeKrwSchema,
    nextPayment: WelfarePaymentSchema.nullable(),
  })
  .strict()
  .superRefine((application, context) => {
    if (application.approvalGameDay !== application.applicationGameDay) {
      context.addIssue({
        code: 'custom',
        path: ['approvalGameDay'],
        message: 'the D1 welfare fixture approves on its application day',
      });
    }
    if (
      application.paidKrw > application.benefitKrw ||
      application.nextPayment === null ||
      application.nextPayment.status !== 'pending' ||
      application.nextPayment.paymentNo !== 1 ||
      application.nextPayment.amountKrw + application.paidKrw !== application.benefitKrw ||
      application.nextPayment.dueGameDay !== application.applicationGameDay + 1
    ) {
      context.addIssue({
        code: 'custom',
        path: ['nextPayment'],
        message: 'active welfare summary must reconcile with its one-time D+1 benefit',
      });
    }
  });

function hasUniqueWelfareConditionCodes(
  conditions: readonly z.infer<typeof WelfareConditionResultSchema>[],
): boolean {
  return new Set(conditions.map((condition) => condition.code)).size === conditions.length;
}

function refineActiveWelfareApplications(
  applications: readonly z.infer<typeof ActiveWelfareApplicationSchema>[],
  context: z.RefinementCtx,
): void {
  let previousId: bigint | undefined;
  for (const [index, application] of applications.entries()) {
    const id = BigInt(application.applicationId);
    if (previousId !== undefined && id <= previousId) {
      context.addIssue({
        code: 'custom',
        path: ['activeWelfareApplications', index],
        message: 'active welfare applications must be ordered by application ID',
      });
    }
    previousId = id;
  }
}

const LifeEventKeySchema = z
  .string()
  .regex(/^[a-z][a-zA-Z0-9]{0,63}$/, 'life event key must be canonical');

const InsuranceProductKeySchema = z
  .string()
  .regex(/^[a-z][a-zA-Z0-9]{0,63}$/, 'insurance product key must be canonical');

export const InsuranceCapabilitySchema = z.enum(['contractsAndClaims', 'unavailable']);
export const InsuranceEligibilityStatusSchema = z.enum(['eligible', 'ineligible', 'indeterminate']);
export const InsuranceEligibilityReasonSchema = z.enum([
  'ageOutsideRange',
  'dependentRequired',
  'residenceRequired',
  'militaryServing',
  'authorityUnavailable',
]);
export const InsuranceContractStatusSchema = z.enum(['active', 'lapsed', 'expired', 'cancelled']);

export const InsuranceProductSchema = z
  .object({
    id: ResourceIdSchema,
    productKey: InsuranceProductKeySchema,
    displayName: z.string().min(1).max(80),
    eligibilityStatus: InsuranceEligibilityStatusSchema,
    reasons: z.array(InsuranceEligibilityReasonSchema).max(8),
    coveredEventKey: LifeEventKeySchema,
    coveredEventDisplayName: z.string().min(1).max(80),
    premiumKrw: PositiveKrwSchema,
    premiumIntervalGameDays: z.number().int().safe().positive().max(65_535),
    termGameDays: z.number().int().safe().positive().max(65_535),
    waitingPeriodGameDays: z.number().int().safe().nonnegative().max(65_535),
    deductibleKrw: NonnegativeKrwSchema,
    occurrenceLimitKrw: PositiveKrwSchema,
    termLimitKrw: PositiveKrwSchema,
    claimWindowGameDays: z.number().int().safe().positive().max(65_535),
  })
  .strict()
  .superRefine((product, context) => {
    if (!hasCanonicalInsuranceReasons(product.reasons)) {
      context.addIssue({
        code: 'custom',
        path: ['reasons'],
        message: 'insurance eligibility reasons must be unique and use canonical order',
      });
    }
    if (
      (product.eligibilityStatus === 'eligible') !== (product.reasons.length === 0) ||
      (product.eligibilityStatus === 'ineligible' &&
        product.reasons.includes('authorityUnavailable')) ||
      (product.eligibilityStatus === 'indeterminate' &&
        !product.reasons.includes('authorityUnavailable')) ||
      product.waitingPeriodGameDays >= product.termGameDays ||
      product.premiumIntervalGameDays >= product.termGameDays ||
      product.occurrenceLimitKrw > product.termLimitKrw
    ) {
      context.addIssue({
        code: 'custom',
        message: 'insurance product terms do not match its published eligibility and limits',
      });
    }
  });

export const InsuranceContractSchema = z
  .object({
    id: ResourceIdSchema,
    productVersionId: ResourceIdSchema,
    productKey: InsuranceProductKeySchema,
    displayName: z.string().min(1).max(80),
    status: InsuranceContractStatusSchema,
    coverageStartGameDay: z.number().int().safe().nonnegative(),
    waitingEndsGameDay: z.number().int().safe().nonnegative(),
    coverageEndExclusive: z.number().int().safe().positive(),
    nextPremiumDueGameDay: z.number().int().safe().positive().nullable(),
    premiumKrw: PositiveKrwSchema,
    paidBenefitKrw: NonnegativeKrwSchema,
    reservedBenefitKrw: NonnegativeKrwSchema,
    remainingBenefitKrw: NonnegativeKrwSchema,
  })
  .strict()
  .superRefine((contract, context) => {
    const requiresWaitingInsideCoverage =
      contract.status === 'active' || contract.status === 'expired';
    if (
      contract.coverageStartGameDay >= contract.coverageEndExclusive ||
      contract.waitingEndsGameDay < contract.coverageStartGameDay ||
      (requiresWaitingInsideCoverage &&
        contract.waitingEndsGameDay >= contract.coverageEndExclusive) ||
      (contract.nextPremiumDueGameDay !== null &&
        (contract.nextPremiumDueGameDay <= contract.coverageStartGameDay ||
          contract.nextPremiumDueGameDay >= contract.coverageEndExclusive)) ||
      (contract.status !== 'active' && contract.nextPremiumDueGameDay !== null)
    ) {
      context.addIssue({
        code: 'custom',
        message: 'insurance contract dates do not match its status and coverage period',
      });
    }
  });

export const InsuranceClaimContractAllocationSchema = z
  .object({
    contractId: ResourceIdSchema,
    deductibleKrw: NonnegativeKrwSchema,
    payoutKrw: PositiveKrwSchema,
  })
  .strict();

const InsuranceClaimIdentityFields = {
  id: ResourceIdSchema,
  eventId: ResourceIdSchema,
  eventKey: LifeEventKeySchema,
  eventDisplayName: z.string().min(1).max(80),
  offeredGameDay: z.number().int().safe().nonnegative(),
} as const;

const InsuranceReadyClaimFields = {
  grossCostKrw: PositiveKrwSchema,
  payoutKrw: PositiveKrwSchema,
  filingDeadlineGameDay: z.number().int().safe().positive(),
  contractAllocations: z.array(InsuranceClaimContractAllocationSchema).min(1).max(8),
} as const;

const CandidateInsuranceClaimSchema = z
  .object({
    ...InsuranceClaimIdentityFields,
    status: z.literal('candidate'),
    grossCostKrw: z.null(),
    payoutKrw: z.null(),
    filingDeadlineGameDay: z.null(),
  })
  .strict();

const ReadyInsuranceClaimSchema = z
  .object({
    ...InsuranceClaimIdentityFields,
    status: z.literal('ready'),
    ...InsuranceReadyClaimFields,
  })
  .strict();

export const PendingInsuranceClaimSchema = z
  .discriminatedUnion('status', [CandidateInsuranceClaimSchema, ReadyInsuranceClaimSchema])
  .superRefine((claim, context) => {
    if (claim.status === 'ready') refineAllocatedInsuranceClaim(claim, context);
  });

const NotApplicableInsuranceClaimSchema = z
  .object({
    ...InsuranceClaimIdentityFields,
    status: z.literal('notApplicable'),
    resolvedGameDay: z.number().int().safe().nonnegative(),
    grossCostKrw: z.null(),
    payoutKrw: z.null(),
    filingDeadlineGameDay: z.null(),
  })
  .strict();

const NotCoveredInsuranceClaimSchema = z
  .object({
    ...InsuranceClaimIdentityFields,
    status: z.literal('notCovered'),
    resolvedGameDay: z.number().int().safe().nonnegative(),
    grossCostKrw: PositiveKrwSchema,
    payoutKrw: z.literal(0),
    filingDeadlineGameDay: z.null(),
  })
  .strict();

const PaidInsuranceClaimSchema = z
  .object({
    ...InsuranceClaimIdentityFields,
    status: z.literal('paid'),
    resolvedGameDay: z.number().int().safe().nonnegative(),
    paidGameDay: z.number().int().safe().nonnegative(),
    ...InsuranceReadyClaimFields,
  })
  .strict();

const ExpiredInsuranceClaimSchema = z
  .object({
    ...InsuranceClaimIdentityFields,
    status: z.literal('expired'),
    resolvedGameDay: z.number().int().safe().nonnegative(),
    ...InsuranceReadyClaimFields,
  })
  .strict();

export const InsuranceClaimHistoryItemSchema = z
  .discriminatedUnion('status', [
    NotApplicableInsuranceClaimSchema,
    NotCoveredInsuranceClaimSchema,
    PaidInsuranceClaimSchema,
    ExpiredInsuranceClaimSchema,
  ])
  .superRefine((claim, context) => {
    if (claim.resolvedGameDay < claim.offeredGameDay) {
      context.addIssue({
        code: 'custom',
        path: ['resolvedGameDay'],
        message: 'insurance claim resolution cannot precede its event offer',
      });
    }
    if (claim.status === 'paid' || claim.status === 'expired') {
      refineAllocatedInsuranceClaim(claim, context);
      if (claim.filingDeadlineGameDay <= claim.resolvedGameDay) {
        context.addIssue({
          code: 'custom',
          path: ['filingDeadlineGameDay'],
          message: 'insurance claim filing deadline must follow event resolution',
        });
      }
    }
    if (
      claim.status === 'paid' &&
      (claim.paidGameDay < claim.resolvedGameDay ||
        claim.paidGameDay >= claim.filingDeadlineGameDay)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['paidGameDay'],
        message: 'insurance claim payment must occur inside its filing window',
      });
    }
  });

export const InsuranceContractsQuerySchema = z
  .object({ cursor: z.string().min(1).max(512).optional() })
  .strict();

export const InsuranceContractsResponseSchema = z
  .object({
    insuranceCapability: InsuranceCapabilitySchema,
    products: z.array(InsuranceProductSchema).max(16),
    contracts: z.array(InsuranceContractSchema).max(20),
    pendingClaims: z.array(PendingInsuranceClaimSchema).max(8),
    history: z.array(InsuranceClaimHistoryItemSchema).max(20),
    nextCursor: z.string().min(1).max(512).nullable(),
  })
  .strict()
  .superRefine((response, context) => {
    if (
      response.insuranceCapability === 'unavailable' &&
      (response.products.length !== 0 ||
        response.contracts.length !== 0 ||
        response.pendingClaims.length !== 0 ||
        response.history.length !== 0 ||
        response.nextCursor !== null)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['insuranceCapability'],
        message: 'unavailable insurance must expose an empty compatibility response',
      });
    }
    refineInsuranceContracts(response.contracts, context, 'contracts');
    refinePendingInsuranceClaims(response.pendingClaims, context, 'pendingClaims');
    refineInsuranceClaimHistory(response.history, context);
    refineInsuranceProducts(response.products, context);
  });

function hasCanonicalInsuranceReasons(
  reasons: readonly z.infer<typeof InsuranceEligibilityReasonSchema>[],
): boolean {
  const rank: Record<z.infer<typeof InsuranceEligibilityReasonSchema>, number> = {
    ageOutsideRange: 1,
    dependentRequired: 2,
    residenceRequired: 3,
    militaryServing: 4,
    authorityUnavailable: 5,
  };
  return reasons.every((reason, index) => {
    const previous = reasons[index - 1];
    return previous === undefined || rank[previous] < rank[reason];
  });
}

function refineInsuranceProducts(
  products: readonly z.infer<typeof InsuranceProductSchema>[],
  context: z.RefinementCtx,
): void {
  let previousId: bigint | undefined;
  for (const [index, product] of products.entries()) {
    const id = BigInt(product.id);
    if (previousId !== undefined && id <= previousId) {
      context.addIssue({
        code: 'custom',
        path: ['products', index],
        message: 'insurance products must use ascending unique IDs',
      });
    }
    previousId = id;
  }
}

function refineAllocatedInsuranceClaim(
  claim: {
    readonly grossCostKrw: number;
    readonly payoutKrw: number;
    readonly filingDeadlineGameDay: number;
    readonly offeredGameDay: number;
    readonly contractAllocations: readonly z.infer<typeof InsuranceClaimContractAllocationSchema>[];
  },
  context: z.RefinementCtx,
): void {
  let previousContractId: bigint | undefined;
  let payout = 0n;
  for (const [index, allocation] of claim.contractAllocations.entries()) {
    const contractId = BigInt(allocation.contractId);
    if (previousContractId !== undefined && contractId <= previousContractId) {
      context.addIssue({
        code: 'custom',
        path: ['contractAllocations', index, 'contractId'],
        message: 'insurance claim allocations must use ascending unique contract IDs',
      });
    }
    previousContractId = contractId;
    payout += BigInt(allocation.payoutKrw);
  }
  if (
    payout !== BigInt(claim.payoutKrw) ||
    claim.payoutKrw > claim.grossCostKrw ||
    claim.filingDeadlineGameDay <= claim.offeredGameDay
  ) {
    context.addIssue({
      code: 'custom',
      message: 'insurance claim allocation must reconcile with payout, loss, and filing window',
    });
  }
}

function refineInsuranceContracts(
  contracts: readonly z.infer<typeof InsuranceContractSchema>[],
  context: z.RefinementCtx,
  path: string,
): void {
  let previous: z.infer<typeof InsuranceContractSchema> | undefined;
  const ids = new Set<string>();
  for (const [index, contract] of contracts.entries()) {
    const ordered =
      previous === undefined ||
      previous.coverageStartGameDay > contract.coverageStartGameDay ||
      (previous.coverageStartGameDay === contract.coverageStartGameDay &&
        BigInt(previous.id) > BigInt(contract.id));
    if (!ordered || ids.has(contract.id)) {
      context.addIssue({
        code: 'custom',
        path: [path, index],
        message: 'insurance contracts must be unique and use reverse start-day and ID order',
      });
    }
    ids.add(contract.id);
    previous = contract;
  }
}

function refineActiveInsuranceContracts(
  contracts: readonly z.infer<typeof InsuranceContractSchema>[],
  context: z.RefinementCtx,
): void {
  let previousId: bigint | undefined;
  for (const [index, contract] of contracts.entries()) {
    const id = BigInt(contract.id);
    if (contract.status !== 'active' || (previousId !== undefined && id <= previousId)) {
      context.addIssue({
        code: 'custom',
        path: ['activeInsuranceContracts', index],
        message: 'active insurance contracts must use ascending unique IDs',
      });
    }
    previousId = id;
  }
}

function refinePendingInsuranceClaims(
  claims: readonly z.infer<typeof PendingInsuranceClaimSchema>[],
  context: z.RefinementCtx,
  path: string,
): void {
  let previousId: bigint | undefined;
  for (const [index, claim] of claims.entries()) {
    const id = BigInt(claim.id);
    if (previousId !== undefined && id <= previousId) {
      context.addIssue({
        code: 'custom',
        path: [path, index],
        message: 'pending insurance claims must use ascending unique IDs',
      });
    }
    previousId = id;
  }
}

function refineInsuranceClaimHistory(
  claims: readonly z.infer<typeof InsuranceClaimHistoryItemSchema>[],
  context: z.RefinementCtx,
): void {
  let previous: z.infer<typeof InsuranceClaimHistoryItemSchema> | undefined;
  const ids = new Set<string>();
  for (const [index, claim] of claims.entries()) {
    const ordered =
      previous === undefined ||
      previous.resolvedGameDay > claim.resolvedGameDay ||
      (previous.resolvedGameDay === claim.resolvedGameDay &&
        BigInt(previous.id) > BigInt(claim.id));
    if (!ordered || ids.has(claim.id)) {
      context.addIssue({
        code: 'custom',
        path: ['history', index],
        message: 'insurance claim history must use reverse resolution-day and ID order',
      });
    }
    ids.add(claim.id);
    previous = claim;
  }
}

function refineInsuranceSnapshotCapability(
  life: {
    readonly insuranceCapability: z.infer<typeof InsuranceCapabilitySchema>;
    readonly activeInsuranceContracts: readonly z.infer<typeof InsuranceContractSchema>[];
    readonly pendingInsuranceClaims: readonly z.infer<typeof PendingInsuranceClaimSchema>[];
  },
  context: z.RefinementCtx,
): void {
  if (
    life.insuranceCapability === 'unavailable' &&
    (life.activeInsuranceContracts.length !== 0 || life.pendingInsuranceClaims.length !== 0)
  ) {
    context.addIssue({
      code: 'custom',
      path: ['insuranceCapability'],
      message: 'unavailable insurance must expose empty snapshot summaries',
    });
  }
}

export const LifeEventCapabilitySchema = z.enum(['deterministicChoices', 'unavailable']);
export const LifeEventDecisionKindSchema = z.enum(['accepted', 'declined']);
export const LifeEventResolutionKindSchema = z.enum(['accepted', 'declined', 'expired']);

export const LifeEventEffectSummarySchema = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('noEffect') }).strict(),
  z.object({ kind: z.literal('walletExpense'), amountKrw: PositiveKrwSchema }).strict(),
]);

export const LifeEventChoiceSchema = z
  .object({
    id: ResourceIdSchema,
    displayName: z.string().min(1).max(120),
    decisionKind: LifeEventDecisionKindSchema,
    effectSummary: LifeEventEffectSummarySchema,
  })
  .strict();

export const PendingLifeEventSchema = z
  .object({
    id: ResourceIdSchema,
    eventKey: LifeEventKeySchema,
    displayName: z.string().min(1).max(80),
    offeredGameDay: z.number().int().safe().nonnegative(),
    expiresGameDay: z.number().int().safe().positive(),
    defaultChoiceId: ResourceIdSchema,
    choices: z.array(LifeEventChoiceSchema).min(2).max(8),
  })
  .strict()
  .superRefine((event, context) => {
    const choiceIds = new Set(event.choices.map((choice) => choice.id));
    const defaultChoice = event.choices.find((choice) => choice.id === event.defaultChoiceId);
    if (
      choiceIds.size !== event.choices.length ||
      defaultChoice === undefined ||
      defaultChoice.effectSummary.kind !== 'noEffect'
    ) {
      context.addIssue({
        code: 'custom',
        path: ['choices'],
        message: 'life event choices must be unique and include a no-effect default choice',
      });
    }
    if (event.expiresGameDay <= event.offeredGameDay) {
      context.addIssue({
        code: 'custom',
        path: ['expiresGameDay'],
        message: 'life event expiry must follow its offer day',
      });
    }
  });

export const LifeEventHistoryItemSchema = z
  .object({
    id: ResourceIdSchema,
    eventKey: LifeEventKeySchema,
    displayName: z.string().min(1).max(80),
    offeredGameDay: z.number().int().safe().nonnegative(),
    resolvedGameDay: z.number().int().safe().nonnegative(),
    resolutionKind: LifeEventResolutionKindSchema,
    choice: LifeEventChoiceSchema,
  })
  .strict()
  .superRefine((event, context) => {
    if (event.resolvedGameDay < event.offeredGameDay) {
      context.addIssue({
        code: 'custom',
        path: ['resolvedGameDay'],
        message: 'life event resolution cannot precede its offer',
      });
    }
    if (event.resolutionKind !== 'expired' && event.resolutionKind !== event.choice.decisionKind) {
      context.addIssue({
        code: 'custom',
        path: ['choice', 'decisionKind'],
        message: 'explicit life event resolution must match its chosen decision',
      });
    }
    if (event.resolutionKind === 'expired' && event.choice.effectSummary.kind !== 'noEffect') {
      context.addIssue({
        code: 'custom',
        path: ['choice', 'effectSummary'],
        message: 'an expired life event must use its no-effect default choice',
      });
    }
  });

export const LifeEventsResponseSchema = z
  .object({
    lifeEventCapability: LifeEventCapabilitySchema,
    insuranceCapability: InsuranceCapabilitySchema,
    pendingEvents: z.array(PendingLifeEventSchema).max(8),
    history: z.array(LifeEventHistoryItemSchema).max(20),
    nextCursor: z.string().min(1).max(512).nullable(),
  })
  .strict()
  .superRefine((response, context) => {
    if (
      response.lifeEventCapability === 'unavailable' &&
      (response.pendingEvents.length !== 0 ||
        response.history.length !== 0 ||
        response.nextCursor !== null)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['lifeEventCapability'],
        message: 'unavailable life events must expose an empty compatibility response',
      });
    }
    refinePendingLifeEvents(response.pendingEvents, context, 'pendingEvents');
    let previous: z.infer<typeof LifeEventHistoryItemSchema> | undefined;
    const ids = new Set<string>();
    for (const [index, event] of response.history.entries()) {
      const ordered =
        previous === undefined ||
        previous.resolvedGameDay > event.resolvedGameDay ||
        (previous.resolvedGameDay === event.resolvedGameDay &&
          BigInt(previous.id) > BigInt(event.id));
      if (!ordered || ids.has(event.id)) {
        context.addIssue({
          code: 'custom',
          path: ['history', index],
          message: 'life event history must be unique and use canonical reverse order',
        });
      }
      ids.add(event.id);
      previous = event;
    }
  });

export const LifeEventsQuerySchema = z
  .object({ cursor: z.string().min(1).max(512).optional() })
  .strict();

function refinePendingLifeEvents(
  events: readonly z.infer<typeof PendingLifeEventSchema>[],
  context: z.RefinementCtx,
  path: string,
): void {
  let previousId: bigint | undefined;
  for (const [index, event] of events.entries()) {
    const id = BigInt(event.id);
    if (previousId !== undefined && id <= previousId) {
      context.addIssue({
        code: 'custom',
        path: [path, index],
        message: 'pending life events must be ordered by event ID',
      });
    }
    previousId = id;
  }
}

export const LifeSnapshotSchema = z
  .object({
    ...LifeSummaryFields,
    household: LifeHouseholdSchema.nullable(),
    residence: LifeResidenceSchema.nullable(),
    tenantLeaseDepositKrw: NonnegativeKrwSchema,
    activeLease: HousingActiveLeaseSchema.nullable(),
    activeLeaseArrears: z.array(HousingLeaseArrearSchema).max(20),
    hasMoreActiveLeaseArrears: z.boolean(),
    totalLeaseArrearKrw: NonnegativeKrwSchema,
    activePropertyHoldings: z.array(HousingPropertyHoldingSchema).max(4),
    hasMoreActivePropertyHoldings: z.boolean(),
    totalPropertyBookValueKrw: NonnegativeKrwSchema,
    creditBand: CreditBandSchema.nullable(),
    creditReasons: z.array(CreditReasonSchema).max(8),
    activeLoans: z.array(LoanSummarySchema).max(8),
    nextLoanInstallment: NextLoanInstallmentSchema.nullable(),
    totalLoanBalanceKrw: NonnegativeKrwSchema,
    activeWelfareApplications: z.array(ActiveWelfareApplicationSchema).max(8),
    insuranceCapability: InsuranceCapabilitySchema,
    activeInsuranceContracts: z.array(InsuranceContractSchema).max(8),
    pendingInsuranceClaims: z.array(PendingInsuranceClaimSchema).max(8),
    pendingEvents: z.array(PendingLifeEventSchema).max(8),
  })
  .strict()
  .superRefine((life, context) => {
    refineLifeArrears(life, context);
    refineLeaseArrearWindow(
      life.activeLeaseArrears,
      life.hasMoreActiveLeaseArrears,
      life.totalLeaseArrearKrw,
      context,
      'activeLeaseArrears',
    );
    refineLifeCreditAndLoans(life, context);
    refineLifePropertyHoldings(life, context);
    refineActiveWelfareApplications(life.activeWelfareApplications, context);
    refineActiveInsuranceContracts(life.activeInsuranceContracts, context);
    refinePendingInsuranceClaims(life.pendingInsuranceClaims, context, 'pendingInsuranceClaims');
    refinePendingLifeEvents(life.pendingEvents, context, 'pendingEvents');
    refineInsuranceSnapshotCapability(life, context);
    if (
      (life.activeLease === null && life.tenantLeaseDepositKrw !== 0) ||
      (life.activeLease !== null && life.tenantLeaseDepositKrw !== life.activeLease.depositKrw)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['tenantLeaseDepositKrw'],
        message: 'tenant lease deposit must reconcile with the active lease',
      });
    }
    if (
      life.activeLease !== null &&
      (life.residence === null ||
        life.residence.tenureKind !== life.activeLease.offerKind ||
        life.residence.regionKey !== life.activeLease.regionKey ||
        life.residence.effectiveFromGameDay !== life.activeLease.effectiveFromGameDay)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['activeLease'],
        message: 'the active tenant lease must match the current tenant residence',
      });
    }
    if (
      life.rateStatus === 'rateUnavailable' &&
      (life.currentMonth !== null ||
        life.activeArrears.length !== 0 ||
        life.hasMoreActiveArrears ||
        life.totalEssentialArrearKrw !== 0)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['rateStatus'],
        message: 'unavailable rates cannot expose living-cost charges or arrears',
      });
    }
    if (life.rateStatus === 'active' && (life.household === null || life.residence === null)) {
      context.addIssue({
        code: 'custom',
        path: ['household'],
        message: 'active rates require a household and residence',
      });
    }
  });

export const LifeBudgetResponseSchema = z
  .object({
    ...LifeSummaryFields,
    household: LifeHouseholdSchema,
    residence: LifeResidenceSchema,
    allowedBands: z.array(LifeBudgetBandSchema).max(16),
    selections: z.array(LifeBudgetSelectionSchema).max(LivingCostCategorySchema.options.length),
  })
  .strict()
  .superRefine((budget, context) => {
    refineLifeArrears(budget, context);
    if (budget.rateStatus === 'rateUnavailable') {
      if (
        budget.allowedBands.length !== 0 ||
        budget.selections.length !== 0 ||
        budget.currentMonth !== null ||
        budget.activeArrears.length !== 0 ||
        budget.hasMoreActiveArrears ||
        budget.totalEssentialArrearKrw !== 0
      ) {
        context.addIssue({
          code: 'custom',
          path: ['rateStatus'],
          message: 'unavailable rates must expose an empty compatibility budget',
        });
      }
      return;
    }
    if (
      budget.allowedBands.length === 0 ||
      budget.selections.length !== LivingCostCategorySchema.options.length ||
      new Set(budget.selections.map((selection) => selection.category)).size !==
        LivingCostCategorySchema.options.length ||
      !hasCanonicalLifeCategoryOrder(budget.selections.map((selection) => selection.category))
    ) {
      context.addIssue({
        code: 'custom',
        path: ['selections'],
        message: 'active rates require every budget category and at least one band',
      });
    }
    const allowedBandIds = new Set(budget.allowedBands.map((band) => band.id));
    if (allowedBandIds.size !== budget.allowedBands.length) {
      context.addIssue({
        code: 'custom',
        path: ['allowedBands'],
        message: 'allowed budget bands must be unique',
      });
    }
    for (const [index, selection] of budget.selections.entries()) {
      if (!allowedBandIds.has(selection.bandId)) {
        context.addIssue({
          code: 'custom',
          path: ['selections', index, 'bandId'],
          message: 'selected budget band must be allowed by the pinned catalog',
        });
      }
    }
  });

export const GameSnapshotSchema = z
  .object({
    runRevision: z.number().int().nonnegative(),
    stateRevision: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
    gameDay: z.number().int().nonnegative(),
    startDate: z.string(),
    cashKrw: z.number().int(),
    debtKrw: z.number().int(),
    netWorthKrw: z.number().int(),
    characterName: z.string().nullable(),
    autoSpeed: GameSpeedSchema.nullable(),
    market: MarketSnapshotSchema,
    portfolio: PortfolioSnapshotSchema,
    finance: FinanceSnapshotSchema,
    career: CareerSnapshotSchema,
    life: LifeSnapshotSchema,
  })
  .superRefine((snapshot, context) => {
    if ((snapshot.market.m2Factors !== null) !== (snapshot.finance.productBundle !== null)) {
      context.addIssue({
        code: 'custom',
        path: ['market', 'm2Factors'],
        message: 'market factors and the finance product bundle must share a world version',
      });
    }

    const accountCashKrw = snapshot.finance.accounts.reduce(
      (sum, account) => sum + BigInt(account.cashKrw),
      0n,
    );
    const cashPrincipalKrw = snapshot.finance.cashContracts.reduce(
      (sum, contract) => sum + BigInt(contract.currentPrincipalKrw),
      0n,
    );
    const bondMarketValueKrw = snapshot.finance.bondPositions.reduce(
      (sum, position) => sum + BigInt(position.marketValueKrw),
      0n,
    );
    const goldMarketValueKrw = snapshot.finance.goldAccounts.reduce(
      (sum, account) => sum + BigInt(account.marketValueKrw),
      0n,
    );
    const physicalGoldMarketValueKrw = snapshot.finance.physicalGoldHoldings.reduce(
      (sum, holding) => sum + BigInt(holding.marketValueKrw),
      0n,
    );
    const expectedNetWorthKrw =
      BigInt(snapshot.cashKrw) +
      accountCashKrw +
      cashPrincipalKrw +
      BigInt(snapshot.portfolio.marketValueKrw) +
      bondMarketValueKrw +
      goldMarketValueKrw +
      physicalGoldMarketValueKrw -
      BigInt(snapshot.debtKrw) +
      BigInt(snapshot.life.tenantLeaseDepositKrw) +
      BigInt(snapshot.life.totalPropertyBookValueKrw);
    if (expectedNetWorthKrw !== BigInt(snapshot.netWorthKrw)) {
      context.addIssue({
        code: 'custom',
        path: ['netWorthKrw'],
        message: 'net worth must reconcile with all cash, assets, and debt',
      });
    }
    if (
      snapshot.life.activeLease !== null &&
      snapshot.life.activeLease.effectiveFromGameDay > snapshot.gameDay
    ) {
      context.addIssue({
        code: 'custom',
        path: ['life', 'activeLease', 'effectiveFromGameDay'],
        message: 'an active lease cannot begin after the snapshot game day',
      });
    }
    refineSnapshotPropertyDates(snapshot, context);
    refineSnapshotWelfareDates(snapshot, context);
    refineSnapshotLifeEventDates(snapshot, context);
    refineSnapshotInsuranceDates(snapshot, context);
    if (
      snapshot.life.activeLease?.offerKind === 'monthlyRent' &&
      snapshot.life.activeLease.nextRentDueGameDay <= snapshot.gameDay
    ) {
      context.addIssue({
        code: 'custom',
        path: ['life', 'activeLease', 'nextRentDueGameDay'],
        message: 'the next monthly-rent charge must follow the snapshot game day',
      });
    }
    const lease = snapshot.life.activeLease;
    const term = lease?.currentTerm;
    if (
      term !== undefined &&
      term !== null &&
      (term.effectiveFromGameDay > snapshot.gameDay || term.effectiveToGameDay <= snapshot.gameDay)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['life', 'activeLease', 'currentTerm'],
        message: 'the current lease term must contain the snapshot game day',
      });
    }
    const notice = lease?.renewalNotice;
    if (
      notice !== undefined &&
      notice !== null &&
      (notice.publishedGameDay > snapshot.gameDay || notice.renewsOnGameDay <= snapshot.gameDay)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['life', 'activeLease', 'renewalNotice'],
        message: 'a renewal notice is visible only from publication until renewal',
      });
    }
    const review = lease?.terminationReview;
    if (
      review !== undefined &&
      review !== null &&
      (review.openedGameDay > snapshot.gameDay ||
        review.activeLeaseArrearKrw > snapshot.life.totalLeaseArrearKrw)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['life', 'activeLease', 'terminationReview'],
        message: 'termination review must be open and reconcile with snapshot lease arrears',
      });
    }
  });

export const GameCommandCursorSchema = z.object({
  runRevision: z.number().int().nonnegative(),
  stateRevision: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
  gameDay: z.number().int().nonnegative(),
});

const LifeCommandCursorFields = {
  commandId: CanonicalUuidSchema,
  expectedRunRevision: z.number().int().nonnegative(),
  expectedStateRevision: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
  expectedGameDay: z.number().int().nonnegative(),
} as const;

export const WelfareApplicationRequestSchema = z
  .object({
    ...LifeCommandCursorFields,
    programVersionId: ResourceIdSchema,
  })
  .strict();

export const LifeEventChoiceRequestSchema = z
  .object({
    ...LifeCommandCursorFields,
    choiceId: ResourceIdSchema,
  })
  .strict();

export const LifeEventChoiceResultSchema = z
  .object({
    eventId: ResourceIdSchema,
    choiceId: ResourceIdSchema,
    resolutionKind: LifeEventDecisionKindSchema,
    resolvedGameDay: z.number().int().safe().nonnegative(),
    walletDeltaKrw: z.number().int().safe().max(0).min(-Number.MAX_SAFE_INTEGER),
  })
  .strict();

export const LifeEventChoiceResponseSchema = z
  .object({
    result: LifeEventChoiceResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict();

export const InsuranceEnrollmentRequestSchema = z
  .object({
    ...LifeCommandCursorFields,
    productVersionId: ResourceIdSchema,
  })
  .strict();

export const InsuranceCancellationRequestSchema = z.object(LifeCommandCursorFields).strict();

export const InsuranceClaimRequestSchema = z
  .object({
    ...LifeCommandCursorFields,
    claimId: ResourceIdSchema,
  })
  .strict();

export const InsuranceEnrollmentResultSchema = z
  .object({
    contractId: ResourceIdSchema,
    productVersionId: ResourceIdSchema,
    status: z.literal('active'),
    coverageStartGameDay: z.number().int().safe().nonnegative(),
    waitingEndsGameDay: z.number().int().safe().nonnegative(),
    coverageEndExclusive: z.number().int().safe().positive(),
    nextPremiumDueGameDay: z.number().int().safe().positive(),
    premiumKrw: PositiveKrwSchema,
  })
  .strict()
  .superRefine((result, context) => {
    if (
      result.waitingEndsGameDay < result.coverageStartGameDay ||
      result.nextPremiumDueGameDay <= result.coverageStartGameDay ||
      result.nextPremiumDueGameDay >= result.coverageEndExclusive ||
      result.waitingEndsGameDay >= result.coverageEndExclusive
    ) {
      context.addIssue({
        code: 'custom',
        message: 'insurance enrollment result dates must form a valid coverage period',
      });
    }
  });

export const InsuranceCancellationResultSchema = z
  .object({
    contractId: ResourceIdSchema,
    status: z.literal('cancelled'),
    coverageEndExclusive: z.number().int().safe().positive(),
  })
  .strict();

export const InsuranceClaimResultSchema = z
  .object({
    claimId: ResourceIdSchema,
    eventId: ResourceIdSchema,
    payoutKrw: PositiveKrwSchema,
    paidGameDay: z.number().int().safe().nonnegative(),
  })
  .strict();

export const InsuranceEnrollmentResponseSchema = z
  .object({
    result: InsuranceEnrollmentResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict();

export const InsuranceCancellationResponseSchema = z
  .object({
    result: InsuranceCancellationResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict();

export const InsuranceClaimResponseSchema = z
  .object({
    result: InsuranceClaimResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict();

export const WelfareApplicationResultSchema = z
  .object({
    applicationId: ResourceIdSchema,
    programVersionId: ResourceIdSchema,
    status: z.literal('active'),
    applicationGameDay: z.number().int().safe().nonnegative(),
    approvalGameDay: z.number().int().safe().nonnegative(),
    eligibilityAtApplication: z.array(WelfareConditionResultSchema).min(1).max(32),
    payment: WelfarePaymentSchema,
  })
  .strict()
  .superRefine((result, context) => {
    if (!hasUniqueWelfareConditionCodes(result.eligibilityAtApplication)) {
      context.addIssue({
        code: 'custom',
        path: ['eligibilityAtApplication'],
        message: 'application evidence condition codes must be unique',
      });
    }
    if (
      result.approvalGameDay !== result.applicationGameDay ||
      result.payment.paymentNo !== 1 ||
      result.payment.dueGameDay !== result.applicationGameDay + 1 ||
      result.payment.status !== 'pending'
    ) {
      context.addIssue({
        code: 'custom',
        path: ['payment'],
        message: 'the D1 welfare application must approve one pending D+1 payment',
      });
    }
  });

export const WelfareApplicationResponseSchema = z
  .object({
    result: WelfareApplicationResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict();

export const LifeBudgetUpdateDraftSchema = z
  .object({ selections: LifeBudgetSelectionsSchema })
  .strict();

export const LifeBudgetUpdateRequestSchema = z
  .object({ ...LifeCommandCursorFields, selections: LifeBudgetSelectionsSchema })
  .strict();

export const EssentialArrearPaymentDraftSchema = z
  .object({ amountKrw: PositiveKrwSchema })
  .strict();

export const EssentialArrearPaymentRequestSchema = z
  .object({ ...LifeCommandCursorFields, amountKrw: PositiveKrwSchema })
  .strict();

export const LifeFailureCodeSchema = z.enum([
  'invalidCommand',
  'characterRequired',
  'insufficientWalletCash',
  'rateUnavailable',
  'creditRestricted',
  'incomeUnavailable',
  'affordabilityLimit',
  'collateralLimit',
  'debtServiceLimit',
  'contractConflict',
  'idempotencyConflict',
  'settlementConflict',
  'loanNotFound',
  'housingResourceNotFound',
  'welfareResourceNotFound',
  'eventNotFound',
  'eventExpired',
  'insuranceResourceNotFound',
  'claimNotCovered',
  'ineligible',
  'valuationUnavailable',
  'policyUnsupported',
  'busy',
]);

export const InsuranceFailureCodeSchema = z.enum([
  'invalidCommand',
  'characterRequired',
  'rateUnavailable',
  'ineligible',
  'insufficientWalletCash',
  'contractConflict',
  'claimNotCovered',
  'insuranceResourceNotFound',
  'idempotencyConflict',
  'busy',
]);

export const InsuranceFailureSchema = z
  .object({ code: InsuranceFailureCodeSchema, message: z.string().min(1) })
  .strict();

export const LifeFailureSchema = z
  .object({ code: LifeFailureCodeSchema, message: z.string().min(1) })
  .strict();

export const LifeBudgetUpdateResultSchema = z
  .object({
    appliedGameDay: z.number().int().safe().nonnegative(),
    selections: LifeBudgetSelectionsSchema,
  })
  .strict()
  .superRefine((result, context) => {
    if (!hasCanonicalLifeCategoryOrder(result.selections.map((selection) => selection.category))) {
      context.addIssue({
        code: 'custom',
        path: ['selections'],
        message: 'budget result selections must use canonical category order',
      });
    }
  });

export const LifeBudgetUpdateResponseSchema = z
  .object({
    result: LifeBudgetUpdateResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict();

export const EssentialArrearPaymentResultSchema = z
  .object({
    arrearId: ResourceIdSchema,
    paidKrw: PositiveKrwSchema,
    remainingKrw: NonnegativeKrwSchema,
  })
  .strict();

export const EssentialArrearPaymentResponseSchema = z
  .object({
    result: EssentialArrearPaymentResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict();

const LoanCommandFields = {
  commandId: CanonicalUuidSchema,
  expectedRunRevision: z.number().int().nonnegative(),
  expectedStateRevision: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
  expectedGameDay: z.number().int().nonnegative(),
} as const;

export const LoanQuoteDraftSchema = z
  .object({
    productVersionId: ResourceIdSchema,
    principalKrw: PositiveKrwSchema,
  })
  .strict();

export const LoanQuoteRequestSchema = z
  .object({
    ...LoanCommandFields,
    productVersionId: ResourceIdSchema,
    principalKrw: PositiveKrwSchema,
  })
  .strict();

export const LoanQuoteDecisionCodeSchema = z.enum([
  'eligible',
  'debtServiceLimit',
  'incomeUnavailable',
  'creditRestricted',
  'valuationUnavailable',
]);

export const LoanQuoteDecisionReasonSchema = z.enum([
  'activeDefault',
  'activeDelinquency',
  'activeRestructuring',
  'creditBandRestricted',
  'activeLoanLimit',
  'incomeUnavailable',
  'debtServiceLimit',
  'eligible',
]);

export const VerifiedIncomeSourceSchema = z.literal('activeEmploymentContract');

export const LoanQuoteDsrSchema = z
  .object({
    numeratorKrw: NonnegativeKrwSchema,
    denominatorKrw: PositiveKrwSchema,
    ratioPpm: z.number().int().safe().nonnegative(),
    limitPpm: z.number().int().safe().nonnegative(),
  })
  .strict()
  .superRefine((dsr, context) => {
    const expectedRatio = (BigInt(dsr.numeratorKrw) * 1_000_000n) / BigInt(dsr.denominatorKrw);
    if (expectedRatio !== BigInt(dsr.ratioPpm)) {
      context.addIssue({
        code: 'custom',
        path: ['ratioPpm'],
        message: 'DSR ratio must be the floored parts-per-million ratio',
      });
    }
  });

export const LoanQuoteFirstInstallmentSchema = z
  .object({
    dueGameDay: z.number().int().safe().nonnegative(),
    feeKrw: NonnegativeKrwSchema,
    principalKrw: NonnegativeKrwSchema,
    interestKrw: NonnegativeKrwSchema,
    totalKrw: NonnegativeKrwSchema,
  })
  .strict()
  .superRefine((installment, context) => {
    const expectedTotal =
      BigInt(installment.feeKrw) +
      BigInt(installment.principalKrw) +
      BigInt(installment.interestKrw);
    if (expectedTotal !== BigInt(installment.totalKrw)) {
      context.addIssue({
        code: 'custom',
        path: ['totalKrw'],
        message: 'first installment total must reconcile with fee, principal, and interest',
      });
    }
  });

export const LoanQuotedTermsSchema = z
  .object({
    annualRateBp: z.number().int().safe().nonnegative().max(20_000),
    repaymentMethod: LoanRepaymentMethodSchema,
    termMonths: z.number().int().min(1).max(65_535),
    firstInstallment: LoanQuoteFirstInstallmentSchema,
  })
  .strict();

const LoanQuoteResultBaseSchema = z
  .object({
    quoteId: ResourceIdSchema,
    productVersionId: ResourceIdSchema,
    requestedPrincipalKrw: PositiveKrwSchema,
    createdGameDay: z.number().int().safe().nonnegative(),
    expiresGameDay: z.number().int().safe().nonnegative(),
    decisionCode: LoanQuoteDecisionCodeSchema,
    decisionReasons: z.array(LoanQuoteDecisionReasonSchema).min(1).max(8),
    verifiedAnnualIncomeKrw: PositiveKrwSchema.nullable(),
    verifiedIncomeSource: VerifiedIncomeSourceSchema.nullable(),
    existingLoanBalanceKrw: NonnegativeKrwSchema,
    postExecutionBalanceKrw: PositiveKrwSchema,
    dsrApplied: z.boolean(),
    dsr: LoanQuoteDsrSchema.nullable(),
    stressRateBp: z.number().int().safe().nonnegative().max(20_000),
    quotedTerms: LoanQuotedTermsSchema,
  })
  .strict();

type LoanQuoteResultValue = z.infer<typeof LoanQuoteResultBaseSchema>;

export const LoanQuoteResultSchema = LoanQuoteResultBaseSchema.superRefine((quote, context) => {
  refineLoanQuoteAmounts(quote, context);
  refineLoanQuoteReasons(quote, context);
  refineLoanQuoteDsr(quote, context);
  refineLoanQuoteSchedule(quote, context);
});

function refineLoanQuoteAmounts(quote: LoanQuoteResultValue, context: z.RefinementCtx): void {
  if (quote.expiresGameDay !== quote.createdGameDay) {
    context.addIssue({
      code: 'custom',
      path: ['expiresGameDay'],
      message: 'loan quote must expire on its creation game day',
    });
  }

  if (
    BigInt(quote.existingLoanBalanceKrw) + BigInt(quote.requestedPrincipalKrw) !==
    BigInt(quote.postExecutionBalanceKrw)
  ) {
    context.addIssue({
      code: 'custom',
      path: ['postExecutionBalanceKrw'],
      message: 'post-execution balance must include the requested principal',
    });
  }

  if ((quote.verifiedAnnualIncomeKrw === null) !== (quote.verifiedIncomeSource === null)) {
    context.addIssue({
      code: 'custom',
      path: ['verifiedIncomeSource'],
      message: 'verified income amount and source must be present together',
    });
  }
}

function refineLoanQuoteReasons(quote: LoanQuoteResultValue, context: z.RefinementCtx): void {
  const reasonOrder = LoanQuoteDecisionReasonSchema.options;
  const uniqueReasons = new Set(quote.decisionReasons);
  const reasonsOrdered = quote.decisionReasons.every((reason, index, reasons) => {
    const previous = reasons[index - 1];
    return previous === undefined || reasonOrder.indexOf(previous) < reasonOrder.indexOf(reason);
  });
  if (uniqueReasons.size !== quote.decisionReasons.length || !reasonsOrdered) {
    context.addIssue({
      code: 'custom',
      path: ['decisionReasons'],
      message: 'loan quote reasons must be unique and use canonical priority',
    });
  }

  if (!loanQuoteDecisionMatchesReasons(quote)) {
    context.addIssue({
      code: 'custom',
      path: ['decisionReasons'],
      message: 'loan quote reasons must match the decision',
    });
  }
}

function loanQuoteDecisionMatchesReasons(quote: LoanQuoteResultValue): boolean {
  const onlyReason = quote.decisionReasons.length === 1 ? quote.decisionReasons[0] : undefined;
  switch (quote.decisionCode) {
    case 'eligible':
    case 'incomeUnavailable':
    case 'debtServiceLimit':
      return onlyReason === quote.decisionCode;
    case 'creditRestricted': {
      const restrictedReasons = new Set([
        'activeDefault',
        'activeDelinquency',
        'activeRestructuring',
        'creditBandRestricted',
        'activeLoanLimit',
      ]);
      return quote.decisionReasons.every((reason) => restrictedReasons.has(reason));
    }
    case 'valuationUnavailable':
      return false;
  }
}

function refineLoanQuoteDsr(quote: LoanQuoteResultValue, context: z.RefinementCtx): void {
  if (quote.dsr !== null) {
    if (!quote.dsrApplied || quote.verifiedAnnualIncomeKrw !== quote.dsr.denominatorKrw) {
      context.addIssue({
        code: 'custom',
        path: ['dsr'],
        message: 'DSR evidence must use the verified annual income denominator',
      });
    }
    if ((quote.decisionCode === 'debtServiceLimit') !== quote.dsr.ratioPpm > quote.dsr.limitPpm) {
      context.addIssue({
        code: 'custom',
        path: ['decisionCode'],
        message: 'DSR decision must match its ratio and limit',
      });
    }
  }

  if (quote.dsrApplied && quote.verifiedAnnualIncomeKrw !== null && quote.dsr === null) {
    context.addIssue({
      code: 'custom',
      path: ['dsr'],
      message: 'an applied DSR with verified income requires complete evidence',
    });
  }

  const incomeUnavailable =
    quote.dsrApplied && quote.dsr === null && quote.verifiedAnnualIncomeKrw === null;
  if ((quote.decisionCode === 'incomeUnavailable') !== incomeUnavailable) {
    context.addIssue({
      code: 'custom',
      path: ['decisionCode'],
      message: 'income-unavailable decision must match the DSR and income evidence',
    });
  }
  if (quote.decisionCode === 'debtServiceLimit' && quote.dsr === null) {
    context.addIssue({
      code: 'custom',
      path: ['dsr'],
      message: 'debt-service-limit decision requires complete DSR evidence',
    });
  }
  if (quote.decisionCode === 'creditRestricted' && (quote.dsrApplied || quote.dsr !== null)) {
    context.addIssue({
      code: 'custom',
      path: ['dsrApplied'],
      message: 'credit restrictions must short-circuit before DSR evaluation',
    });
  }
}

function refineLoanQuoteSchedule(quote: LoanQuoteResultValue, context: z.RefinementCtx): void {
  if (quote.quotedTerms.firstInstallment.dueGameDay <= quote.createdGameDay) {
    context.addIssue({
      code: 'custom',
      path: ['quotedTerms', 'firstInstallment', 'dueGameDay'],
      message: 'first installment must follow quote creation',
    });
  }
}

export const LoanQuoteResponseSchema = z
  .object({
    result: LoanQuoteResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict();

export const LoanExecutionDraftSchema = z.object({ quoteId: ResourceIdSchema }).strict();

export const LoanExecutionRequestSchema = z
  .object({
    ...LoanCommandFields,
    quoteId: ResourceIdSchema,
  })
  .strict();

const LoanExecutionResultBaseSchema = z
  .object({
    loanId: ResourceIdSchema,
    quoteId: ResourceIdSchema,
    productVersionId: ResourceIdSchema,
    principalKrw: PositiveKrwSchema,
    activatedGameDay: z.number().int().safe().nonnegative(),
    maturityGameDay: z.number().int().safe().positive(),
    annualRateBp: z.number().int().safe().nonnegative().max(20_000),
    repaymentMethod: LoanRepaymentMethodSchema,
    termMonths: z.number().int().min(1).max(65_535),
    firstInstallment: LoanQuoteFirstInstallmentSchema,
  })
  .strict();

export const LoanExecutionResultSchema = LoanExecutionResultBaseSchema.superRefine(
  (execution, context) => {
    if (execution.maturityGameDay <= execution.activatedGameDay) {
      context.addIssue({
        code: 'custom',
        path: ['maturityGameDay'],
        message: 'loan maturity must follow activation',
      });
    }
    if (
      execution.firstInstallment.dueGameDay <= execution.activatedGameDay ||
      execution.firstInstallment.dueGameDay > execution.maturityGameDay
    ) {
      context.addIssue({
        code: 'custom',
        path: ['firstInstallment', 'dueGameDay'],
        message: 'first installment must fall after activation and no later than maturity',
      });
    }
  },
);

export const LoanExecutionResponseSchema = z
  .object({
    result: LoanExecutionResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict();

export const LoanPrepaymentDraftSchema = z
  .object({
    loanId: ResourceIdSchema,
    principalKrw: PositiveKrwSchema,
  })
  .strict();

export const LoanPrepaymentRequestSchema = z
  .object({
    ...LoanCommandFields,
    principalKrw: PositiveKrwSchema,
  })
  .strict();

export const LoanPrepaymentStatusSchema = z.enum(['active', 'paidOff']);

export const LoanPrepaymentNextInstallmentSchema = z
  .object({
    installmentNo: z.number().int().min(1).max(65_535),
    dueGameDay: z.number().int().safe().nonnegative(),
    feeKrw: NonnegativeKrwSchema,
    principalKrw: NonnegativeKrwSchema,
    interestKrw: NonnegativeKrwSchema,
    totalKrw: NonnegativeKrwSchema,
  })
  .strict()
  .superRefine((installment, context) => {
    const expectedTotal =
      BigInt(installment.feeKrw) +
      BigInt(installment.principalKrw) +
      BigInt(installment.interestKrw);
    if (expectedTotal !== BigInt(installment.totalKrw)) {
      context.addIssue({
        code: 'custom',
        path: ['totalKrw'],
        message: 'prepayment next installment total must reconcile',
      });
    }
  });

const LoanPrepaymentResultBaseSchema = z
  .object({
    loanId: ResourceIdSchema,
    paymentId: ResourceIdSchema,
    principalKrw: PositiveKrwSchema,
    feeKrw: NonnegativeKrwSchema,
    totalDebitedKrw: PositiveKrwSchema,
    appliedGameDay: z.number().int().safe().nonnegative(),
    remainingPrincipalKrw: NonnegativeKrwSchema,
    status: LoanPrepaymentStatusSchema,
    prepaymentEffect: LoanPrepaymentEffectSchema,
    remainingInstallments: z.number().int().safe().nonnegative().max(65_535),
    nextInstallment: LoanPrepaymentNextInstallmentSchema.nullable(),
    finalInstallmentDueGameDay: z.number().int().safe().nonnegative().nullable(),
  })
  .strict();

export const LoanPrepaymentResultSchema = LoanPrepaymentResultBaseSchema.superRefine(
  (result, context) => {
    if (BigInt(result.principalKrw) + BigInt(result.feeKrw) !== BigInt(result.totalDebitedKrw)) {
      context.addIssue({
        code: 'custom',
        path: ['totalDebitedKrw'],
        message: 'prepayment debit must equal principal plus fee',
      });
    }

    const paidOff = result.status === 'paidOff';
    const hasNoRemainingSchedule =
      result.remainingPrincipalKrw === 0 &&
      result.remainingInstallments === 0 &&
      result.nextInstallment === null &&
      result.finalInstallmentDueGameDay === null;
    if (paidOff !== hasNoRemainingSchedule) {
      context.addIssue({
        code: 'custom',
        path: ['status'],
        message: 'prepayment status must match the remaining balance and schedule',
      });
    }

    if (!paidOff) {
      if (
        result.remainingPrincipalKrw === 0 ||
        result.remainingInstallments === 0 ||
        result.nextInstallment === null ||
        result.finalInstallmentDueGameDay === null
      ) {
        context.addIssue({
          code: 'custom',
          path: ['remainingInstallments'],
          message: 'an active loan requires a remaining schedule',
        });
      } else if (
        result.nextInstallment.dueGameDay <= result.appliedGameDay ||
        result.nextInstallment.dueGameDay > result.finalInstallmentDueGameDay
      ) {
        context.addIssue({
          code: 'custom',
          path: ['nextInstallment', 'dueGameDay'],
          message: 'next installment must follow prepayment and not exceed the final due day',
        });
      }
    }
  },
);

export const LoanPrepaymentResponseSchema = z
  .object({
    result: LoanPrepaymentResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict();

const LoanDetailBaseSchema = z
  .object({
    id: ResourceIdSchema,
    leaseContractId: ResourceIdSchema.nullable(),
    propertyHoldingId: ResourceIdSchema.nullable(),
    productVersionId: ResourceIdSchema,
    productKind: LoanProductKindSchema,
    displayName: z.string().min(1).max(80),
    rateStatus: LoanRateStatusSchema,
    currentAnnualRateBp: z.number().int().safe().nonnegative().max(20_000).nullable(),
    status: LoanContractStatusSchema,
    readOnly: z.boolean(),
    originalPrincipalKrw: PositiveKrwSchema,
    remainingPrincipalKrw: NonnegativeKrwSchema,
    accruedInterestKrw: NonnegativeKrwSchema,
    accruedFeeKrw: NonnegativeKrwSchema,
    overdueKrw: NonnegativeKrwSchema,
    repaymentMethod: LoanRepaymentMethodSchema,
    termMonths: z.number().int().min(1).max(65_535).nullable(),
    totalInstallments: z.number().int().min(1).max(65_535).nullable(),
    activatedGameDay: z.number().int().safe().nonnegative(),
    maturityGameDay: z.number().int().safe().nonnegative().nullable(),
    finalInstallmentDueGameDay: z.number().int().safe().nonnegative().nullable(),
    nextInstallmentNo: z.number().int().min(1).max(65_535).nullable(),
    oldestUnpaidDueGameDay: z.number().int().safe().nonnegative().nullable(),
    prepaymentAllowed: z.boolean(),
    prepaymentFeePpm: z.number().int().safe().nonnegative().max(1_000_000).nullable(),
    prepaymentEffect: LoanPrepaymentEffectSchema.nullable(),
    dsrIncluded: z.boolean(),
  })
  .strict();

type LoanDetailValue = z.infer<typeof LoanDetailBaseSchema>;

export const LoanDetailSchema = LoanDetailBaseSchema.superRefine((loan, context) => {
  refineLoanDetailAmounts(loan, context);
  refineLoanDetailTerms(loan, context);
  refineLoanDetailSchedule(loan, context);
  if ((loan.productKind === 'leaseDepositLoan') !== (loan.leaseContractId !== null)) {
    context.addIssue({
      code: 'custom',
      path: ['leaseContractId'],
      message: 'only a lease-deposit loan must identify its tenant lease contract',
    });
  }
  if ((loan.productKind === 'mortgage') !== (loan.propertyHoldingId !== null)) {
    context.addIssue({
      code: 'custom',
      path: ['propertyHoldingId'],
      message: 'only a mortgage must identify its property holding',
    });
  }
  if (loan.leaseContractId !== null && loan.propertyHoldingId !== null) {
    context.addIssue({
      code: 'custom',
      path: ['propertyHoldingId'],
      message: 'a loan cannot link both a lease and a property holding',
    });
  }
});

function refineLoanDetailAmounts(loan: LoanDetailValue, context: z.RefinementCtx): void {
  const rateMatchesAvailability =
    (loan.rateStatus === 'available' && loan.currentAnnualRateBp !== null) ||
    (loan.rateStatus === 'rateUnavailable' && loan.currentAnnualRateBp === null);
  if (!rateMatchesAvailability) {
    context.addIssue({
      code: 'custom',
      path: ['currentAnnualRateBp'],
      message: 'loan detail rate must match its availability',
    });
  }

  if (loan.remainingPrincipalKrw > loan.originalPrincipalKrw) {
    context.addIssue({
      code: 'custom',
      path: ['remainingPrincipalKrw'],
      message: 'remaining principal cannot exceed original principal',
    });
  }

  const hasPrepaymentTerms = loan.prepaymentFeePpm !== null && loan.prepaymentEffect !== null;
  if (loan.prepaymentAllowed !== hasPrepaymentTerms) {
    context.addIssue({
      code: 'custom',
      path: ['prepaymentAllowed'],
      message: 'prepayment capability must match its public terms',
    });
  }
}

function refineLoanDetailTerms(loan: LoanDetailValue, context: z.RefinementCtx): void {
  const scheduledTerms = [loan.termMonths, loan.totalInstallments, loan.maturityGameDay];
  const hasCompleteScheduledTerms = scheduledTerms.every((value) => value !== null);
  if (loan.productKind === 'legacyDebt') {
    const invalidLegacy =
      !loan.readOnly ||
      loan.rateStatus !== 'rateUnavailable' ||
      hasCompleteScheduledTerms ||
      scheduledTerms.some((value) => value !== null) ||
      loan.finalInstallmentDueGameDay !== null ||
      loan.nextInstallmentNo !== null ||
      loan.prepaymentAllowed ||
      loan.prepaymentFeePpm !== null ||
      loan.prepaymentEffect !== null;
    if (invalidLegacy) {
      context.addIssue({
        code: 'custom',
        path: ['productKind'],
        message: 'legacy loan detail must remain read-only without schedule or prepayment terms',
      });
    }
  } else if (!hasCompleteScheduledTerms) {
    context.addIssue({
      code: 'custom',
      path: ['termMonths'],
      message: 'a scheduled loan requires complete immutable terms',
    });
  }
}

function refineLoanDetailSchedule(loan: LoanDetailValue, context: z.RefinementCtx): void {
  if (loan.maturityGameDay !== null && loan.maturityGameDay <= loan.activatedGameDay) {
    context.addIssue({
      code: 'custom',
      path: ['maturityGameDay'],
      message: 'loan maturity must follow activation',
    });
  }
  if (
    loan.finalInstallmentDueGameDay !== null &&
    (loan.finalInstallmentDueGameDay <= loan.activatedGameDay ||
      (loan.maturityGameDay !== null && loan.finalInstallmentDueGameDay > loan.maturityGameDay))
  ) {
    context.addIssue({
      code: 'custom',
      path: ['finalInstallmentDueGameDay'],
      message: 'final installment must fall after activation and no later than maturity',
    });
  }
  if (
    loan.nextInstallmentNo !== null &&
    loan.totalInstallments !== null &&
    loan.nextInstallmentNo > loan.totalInstallments
  ) {
    context.addIssue({
      code: 'custom',
      path: ['nextInstallmentNo'],
      message: 'next installment cannot exceed the contract installment count',
    });
  }
}

export const LoanInstallmentStatusSchema = z.enum([
  'pending',
  'due',
  'partiallyPaid',
  'paid',
  'cancelled',
]);

const LoanInstallmentHistoryItemBaseSchema = z
  .object({
    id: ResourceIdSchema,
    installmentNo: z.number().int().min(1).max(65_535),
    dueGameDay: z.number().int().safe().nonnegative(),
    interestPeriodStartGameDay: z.number().int().safe().nonnegative(),
    elapsedDays: z.number().int().min(1).max(65_535),
    annualRateBp: z.number().int().safe().nonnegative().max(20_000),
    openingPrincipalKrw: PositiveKrwSchema,
    scheduledFeeKrw: NonnegativeKrwSchema,
    scheduledInterestKrw: NonnegativeKrwSchema,
    scheduledPrincipalKrw: NonnegativeKrwSchema,
    paidFeeKrw: NonnegativeKrwSchema,
    paidInterestKrw: NonnegativeKrwSchema,
    paidPrincipalKrw: NonnegativeKrwSchema,
    remainingDueKrw: NonnegativeKrwSchema,
    status: LoanInstallmentStatusSchema,
    scheduleRevision: z.number().int().safe().positive().max(4_294_967_295),
  })
  .strict();

export const LoanInstallmentHistoryItemSchema = LoanInstallmentHistoryItemBaseSchema.superRefine(
  (installment, context) => {
    if (
      installment.dueGameDay - installment.interestPeriodStartGameDay + 1 !==
      installment.elapsedDays
    ) {
      context.addIssue({
        code: 'custom',
        path: ['elapsedDays'],
        message: 'installment elapsed days must match its inclusive interest period',
      });
    }

    const scheduled =
      BigInt(installment.scheduledFeeKrw) +
      BigInt(installment.scheduledInterestKrw) +
      BigInt(installment.scheduledPrincipalKrw);
    const paid =
      BigInt(installment.paidFeeKrw) +
      BigInt(installment.paidInterestKrw) +
      BigInt(installment.paidPrincipalKrw);
    const componentOverpaid =
      installment.paidFeeKrw > installment.scheduledFeeKrw ||
      installment.paidInterestKrw > installment.scheduledInterestKrw ||
      installment.paidPrincipalKrw > installment.scheduledPrincipalKrw;
    if (componentOverpaid) {
      context.addIssue({
        code: 'custom',
        path: ['remainingDueKrw'],
        message: 'installment paid amounts cannot exceed scheduled amounts',
      });
    }

    const cancelled = installment.status === 'cancelled';
    const expectedRemaining = cancelled ? 0n : scheduled - paid;
    if (expectedRemaining !== BigInt(installment.remainingDueKrw)) {
      context.addIssue({
        code: 'custom',
        path: ['remainingDueKrw'],
        message: 'installment remaining amount must reconcile with its payment state',
      });
    }
    if (
      (installment.status === 'paid' && paid !== scheduled) ||
      (installment.status === 'partiallyPaid' && (paid === 0n || paid >= scheduled)) ||
      (['pending', 'due'].includes(installment.status) && paid !== 0n) ||
      (cancelled && paid !== 0n)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['status'],
        message: 'installment status must match its paid amounts',
      });
    }
  },
);

export const LoanPaymentKindSchema = z.enum([
  'scheduledInstallment',
  'manualPrepayment',
  'leaseMovePayoff',
  'propertySalePayoff',
]);
export const LoanPaymentAllocationKindSchema = z.enum([
  'overdueFee',
  'overdueInterest',
  'overduePrincipal',
  'currentFee',
  'currentInterest',
  'currentPrincipal',
  'prepaymentFee',
  'prepaymentPrincipal',
]);

export const LoanPaymentAllocationSchema = z
  .object({
    kind: LoanPaymentAllocationKindSchema,
    amountKrw: PositiveKrwSchema,
  })
  .strict();

const LoanPaymentHistoryItemBaseSchema = z
  .object({
    id: ResourceIdSchema,
    paymentNo: z.number().int().safe().positive().max(4_294_967_295),
    kind: LoanPaymentKindSchema,
    gameDay: z.number().int().safe().nonnegative(),
    amountKrw: PositiveKrwSchema,
    allocations: z.array(LoanPaymentAllocationSchema).min(1).max(8),
  })
  .strict();

export const LoanPaymentHistoryItemSchema = LoanPaymentHistoryItemBaseSchema.superRefine(
  (payment, context) => {
    const order = LoanPaymentAllocationKindSchema.options;
    const uniqueKinds = new Set(payment.allocations.map((allocation) => allocation.kind));
    const canonicallyOrdered = payment.allocations.every((allocation, index, allocations) => {
      const previous = allocations[index - 1];
      return (
        previous === undefined || order.indexOf(previous.kind) < order.indexOf(allocation.kind)
      );
    });
    if (uniqueKinds.size !== payment.allocations.length || !canonicallyOrdered) {
      context.addIssue({
        code: 'custom',
        path: ['allocations'],
        message: 'payment allocations must be unique and use canonical priority',
      });
    }
    const allocated = payment.allocations.reduce(
      (sum, allocation) => sum + BigInt(allocation.amountKrw),
      0n,
    );
    if (allocated !== BigInt(payment.amountKrw)) {
      context.addIssue({
        code: 'custom',
        path: ['amountKrw'],
        message: 'payment amount must equal its allocation total',
      });
    }
    if (
      payment.kind === 'leaseMovePayoff' &&
      (payment.allocations.length !== 1 || payment.allocations[0]?.kind !== 'prepaymentPrincipal')
    ) {
      context.addIssue({
        code: 'custom',
        path: ['allocations'],
        message: 'a lease-move payoff must allocate only prepayment principal',
      });
    }
  },
);

const LoanInstallmentCursorPattern = /^v1\.l([1-9][0-9]*)\.i(0|[1-9][0-9]*)\.p(0|[1-9][0-9]*)$/;

export const LoanInstallmentCursorSchema = z
  .string()
  .max(160)
  .regex(
    LoanInstallmentCursorPattern,
    'loan installment cursor must use the canonical dual-window format',
  )
  .superRefine((cursor, context) => {
    const match = LoanInstallmentCursorPattern.exec(cursor);
    const loanId = match?.[1];
    const installmentBefore = match?.[2];
    const paymentBefore = match?.[3];
    if (loanId === undefined || installmentBefore === undefined || paymentBefore === undefined) {
      return;
    }
    if (
      BigInt(loanId) > 18_446_744_073_709_551_615n ||
      BigInt(installmentBefore) > 65_535n ||
      BigInt(paymentBefore) > 4_294_967_295n
    ) {
      context.addIssue({
        code: 'custom',
        message: 'loan installment cursor component is out of range',
      });
    }
  });

export const LoanInstallmentHistoryQuerySchema = z
  .object({
    before: LoanInstallmentCursorSchema.optional(),
    limit: z.number().int().min(1).max(50).optional(),
  })
  .strict();

const LoanInstallmentHistoryResponseBaseSchema = z
  .object({
    loanId: ResourceIdSchema,
    installments: z.array(LoanInstallmentHistoryItemSchema).max(50),
    payments: z.array(LoanPaymentHistoryItemSchema).max(50),
    hasMoreInstallments: z.boolean(),
    hasMorePayments: z.boolean(),
    nextBefore: LoanInstallmentCursorSchema.nullable(),
  })
  .strict();

export const LoanInstallmentHistoryResponseSchema =
  LoanInstallmentHistoryResponseBaseSchema.superRefine((page, context) => {
    refineDescendingLoanHistory(page.installments, 'installmentNo', 'installments', context);
    refineDescendingLoanHistory(page.payments, 'paymentNo', 'payments', context);

    const lastInstallment = page.installments.at(-1)?.installmentNo;
    const lastPayment = page.payments.at(-1)?.paymentNo;
    if (
      (page.hasMoreInstallments && lastInstallment === undefined) ||
      (page.hasMorePayments && lastPayment === undefined)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['nextBefore'],
        message: 'a continuing history window requires a last item',
      });
      return;
    }

    const hasMore = page.hasMoreInstallments || page.hasMorePayments;
    const expectedNext = hasMore
      ? `v1.l${page.loanId}.i${page.hasMoreInstallments ? lastInstallment : 0}.p${page.hasMorePayments ? lastPayment : 0}`
      : null;
    if (page.nextBefore !== expectedNext) {
      context.addIssue({
        code: 'custom',
        path: ['nextBefore'],
        message: 'next cursor must match both dual-window continuation points',
      });
    }
  });

function refineDescendingLoanHistory<T extends Record<K, number>, K extends string>(
  items: readonly T[],
  key: K,
  path: string,
  context: z.RefinementCtx,
): void {
  const ordered = items.every((item, index) => {
    const previous = items[index - 1];
    return previous === undefined || previous[key] > item[key];
  });
  if (!ordered) {
    context.addIssue({
      code: 'custom',
      path: [path],
      message: 'loan history items must be unique and strictly descending',
    });
  }
}

export const HousingRateStatusSchema = z.enum(['active', 'rateUnavailable']);

const HousingSaleOfferSchema = z
  .object({
    kind: z.literal('sale'),
    priceKrw: PositiveKrwSchema,
  })
  .strict();

const HousingJeonseOfferSchema = z
  .object({
    kind: z.literal('jeonse'),
    depositKrw: PositiveKrwSchema,
  })
  .strict();

const HousingMonthlyRentOfferSchema = z
  .object({
    kind: z.literal('monthlyRent'),
    depositKrw: PositiveKrwSchema,
    monthlyRentKrw: PositiveKrwSchema,
  })
  .strict();

export const HousingOfferSchema = z.discriminatedUnion('kind', [
  HousingSaleOfferSchema,
  HousingJeonseOfferSchema,
  HousingMonthlyRentOfferSchema,
]);

const HousingOffersSchema = z
  .array(HousingOfferSchema)
  .min(1)
  .max(3)
  .superRefine((offers, context) => {
    const canonical = offers.every((offer, index) => {
      const previous = offers[index - 1];
      return (
        previous === undefined ||
        HousingOfferKindSchema.options.indexOf(previous.kind) <
          HousingOfferKindSchema.options.indexOf(offer.kind)
      );
    });
    if (!canonical) {
      context.addIssue({
        code: 'custom',
        message: 'housing offers must be unique and use canonical kind order',
      });
    }
  });

export const HousingRegionSchema = z
  .object({
    regionKey: HousingRegionKeySchema,
    displayName: z.string().min(1).max(120),
  })
  .strict();

export const HousingListingSchema = z
  .object({
    id: HousingListingIdSchema,
    regionKey: HousingRegionKeySchema,
    propertyType: HousingPropertyTypeSchema,
    exclusiveAreaSquareMeters: z.number().int().safe().positive(),
    availableFromGameDay: z.number().int().safe().nonnegative(),
    availableToGameDay: z.number().int().safe().nonnegative(),
    offers: HousingOffersSchema,
  })
  .strict()
  .superRefine((listing, context) => {
    if (listing.availableFromGameDay > listing.availableToGameDay) {
      context.addIssue({
        code: 'custom',
        path: ['availableToGameDay'],
        message: 'housing listing availability must not end before it begins',
      });
    }
  });

export const HousingListingsQuerySchema = z
  .object({ region: HousingRegionKeySchema.optional() })
  .strict();

const HousingListingsResponseBaseSchema = z
  .object({
    rateStatus: HousingRateStatusSchema,
    modelVersionId: ResourceIdSchema,
    gameDay: z.number().int().safe().nonnegative(),
    yearMonth: YearMonthSchema,
    residenceRegionKey: HousingRegionKeySchema,
    selectedRegionKey: HousingRegionKeySchema,
    regions: z.array(HousingRegionSchema).min(1).max(4),
    priceIndexPpm: z.number().int().safe().positive().nullable(),
    rentIndexPpm: z.number().int().safe().positive().nullable(),
    listings: z.array(HousingListingSchema).max(24),
  })
  .strict();

export const HousingListingsResponseSchema = HousingListingsResponseBaseSchema.superRefine(
  (response, context) => {
    const regionKeys = response.regions.map((region) => region.regionKey);
    const canonicalRegions = regionKeys.every((regionKey, index) => {
      const previous = regionKeys[index - 1];
      return (
        previous === undefined ||
        HousingRegionKeySchema.options.indexOf(previous) <
          HousingRegionKeySchema.options.indexOf(regionKey)
      );
    });
    if (!canonicalRegions) {
      context.addIssue({
        code: 'custom',
        path: ['regions'],
        message: 'housing regions must be unique and use canonical region order',
      });
    }
    if (!regionKeys.includes(response.residenceRegionKey)) {
      context.addIssue({
        code: 'custom',
        path: ['residenceRegionKey'],
        message: 'the residence region must be present in the public region catalog',
      });
    }
    if (!regionKeys.includes(response.selectedRegionKey)) {
      context.addIssue({
        code: 'custom',
        path: ['selectedRegionKey'],
        message: 'the selected region must be present in the public region catalog',
      });
    }

    const active = response.rateStatus === 'active';
    if (
      active !== (response.priceIndexPpm !== null) ||
      active !== (response.rentIndexPpm !== null) ||
      (!active && response.listings.length > 0)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['rateStatus'],
        message: 'housing availability must match indexes and public listings',
      });
    }

    const listingIds = new Set<string>();
    for (const [index, listing] of response.listings.entries()) {
      if (listingIds.has(listing.id)) {
        context.addIssue({
          code: 'custom',
          path: ['listings', index, 'id'],
          message: 'housing listing IDs must be unique',
        });
      }
      listingIds.add(listing.id);
      if (listing.regionKey !== response.selectedRegionKey) {
        context.addIssue({
          code: 'custom',
          path: ['listings', index, 'regionKey'],
          message: 'housing listings must belong to the selected region',
        });
      }
      if (
        listing.availableFromGameDay > response.gameDay ||
        listing.availableToGameDay < response.gameDay
      ) {
        context.addIssue({
          code: 'custom',
          path: ['listings', index],
          message: 'housing listings must be available on the response game day',
        });
      }
    }
  },
);

export const HousingMovingCostSchema = z
  .object({
    regionKey: HousingRegionKeySchema,
    movingCostKrw: PositiveKrwSchema,
  })
  .strict();

export const HousingMonthlyRentTermsSchema = z
  .object({
    rentChargeRule: HousingRentChargeRuleSchema,
    arrearRepaymentRule: HousingArrearRepaymentRuleSchema,
  })
  .strict();

const HousingLeaseReadFields = {
  movingCosts: z.array(HousingMovingCostSchema).length(HousingRegionKeySchema.options.length),
  tenantLeaseDepositKrw: NonnegativeKrwSchema,
  activeLease: HousingActiveLeaseSchema.nullable(),
  activeArrears: z.array(HousingLeaseArrearSchema).max(20),
  hasMoreActiveArrears: z.boolean(),
  totalLeaseArrearKrw: NonnegativeKrwSchema,
} as const;

const HousingCashJeonseCapabilitySchema = z
  .object({
    leaseCapability: z.literal('cashJeonse'),
    renewalRule: HousingLeaseRenewalRuleSchema,
    leaseLifecycleTerms: HousingLeaseLifecycleTermsSchema.nullable(),
    ...HousingLeaseReadFields,
    monthlyRentTerms: z.null(),
  })
  .strict();

const HousingCashJeonseAndMonthlyRentCapabilitySchema = z
  .object({
    leaseCapability: z.literal('cashJeonseAndMonthlyRent'),
    renewalRule: HousingLeaseRenewalRuleSchema,
    leaseLifecycleTerms: HousingLeaseLifecycleTermsSchema.nullable(),
    ...HousingLeaseReadFields,
    monthlyRentTerms: HousingMonthlyRentTermsSchema,
  })
  .strict();

const HousingUnavailableLeaseCapabilitySchema = z
  .object({
    leaseCapability: z.literal('unavailable'),
    renewalRule: z.null(),
    leaseLifecycleTerms: z.null(),
    movingCosts: z.array(HousingMovingCostSchema).length(0),
    tenantLeaseDepositKrw: z.literal(0),
    activeLease: z.null(),
    monthlyRentTerms: z.null(),
    activeArrears: z.array(HousingLeaseArrearSchema).length(0),
    hasMoreActiveArrears: z.literal(false),
    totalLeaseArrearKrw: z.literal(0),
  })
  .strict();

type HousingAvailableCurrentLeaseResponse =
  | z.infer<typeof HousingCashJeonseCapabilitySchema>
  | z.infer<typeof HousingCashJeonseAndMonthlyRentCapabilitySchema>;

function refineHousingLeaseLifecycleTerms(
  response: HousingAvailableCurrentLeaseResponse,
  context: z.RefinementCtx,
): void {
  const lifecycleTerms = response.leaseLifecycleTerms;
  const expectedRenewalRule = lifecycleTerms === null ? 'openEnded' : 'fixedTermAutoRenew';
  if (response.renewalRule !== expectedRenewalRule) {
    context.addIssue({
      code: 'custom',
      path: ['renewalRule'],
      message: 'the renewal rule must match the model lifecycle terms',
    });
  }

  const reviewTerms = lifecycleTerms?.monthlyRentTerminationReview;
  if (
    response.leaseCapability === 'cashJeonse' &&
    reviewTerms !== null &&
    reviewTerms !== undefined
  ) {
    context.addIssue({
      code: 'custom',
      path: ['leaseLifecycleTerms', 'monthlyRentTerminationReview'],
      message: 'cash-jeonse lifecycle terms cannot expose monthly-rent termination review',
    });
  }
  if (
    response.leaseCapability === 'cashJeonseAndMonthlyRent' &&
    lifecycleTerms !== null &&
    reviewTerms === null
  ) {
    context.addIssue({
      code: 'custom',
      path: ['leaseLifecycleTerms', 'monthlyRentTerminationReview'],
      message: 'fixed-term monthly-rent capability requires termination-review terms',
    });
  }
}

function refineHousingActiveLeaseLifecycle(
  response: HousingAvailableCurrentLeaseResponse,
  context: z.RefinementCtx,
): void {
  const lease = response.activeLease;
  if (lease === null) return;
  if (lease.renewalRule !== response.renewalRule) {
    context.addIssue({
      code: 'custom',
      path: ['activeLease', 'renewalRule'],
      message: 'the active lease renewal rule must match the current model',
    });
  }

  const review = lease.terminationReview;
  if (review === null) return;
  const reviewTerms = response.leaseLifecycleTerms?.monthlyRentTerminationReview;
  if (
    reviewTerms === null ||
    reviewTerms === undefined ||
    review.activeLeaseArrearKrw > response.totalLeaseArrearKrw
  ) {
    context.addIssue({
      code: 'custom',
      path: ['activeLease', 'terminationReview'],
      message: 'termination review must match model terms and aggregate lease arrears',
    });
  }
}

export const HousingCurrentLeaseResponseSchema = z
  .discriminatedUnion('leaseCapability', [
    HousingCashJeonseCapabilitySchema,
    HousingCashJeonseAndMonthlyRentCapabilitySchema,
    HousingUnavailableLeaseCapabilitySchema,
  ])
  .superRefine((response, context) => {
    refineLeaseArrearWindow(
      response.activeArrears,
      response.hasMoreActiveArrears,
      response.totalLeaseArrearKrw,
      context,
      'activeArrears',
    );
    if (response.leaseCapability === 'unavailable') return;
    refineHousingLeaseLifecycleTerms(response, context);
    refineHousingActiveLeaseLifecycle(response, context);

    const movingCostRegions = response.movingCosts.map((cost) => cost.regionKey);
    if (
      !movingCostRegions.every(
        (regionKey, index) => regionKey === HousingRegionKeySchema.options[index],
      )
    ) {
      context.addIssue({
        code: 'custom',
        path: ['movingCosts'],
        message: 'moving costs must contain every region in canonical order',
      });
    }
    if (
      (response.activeLease === null && response.tenantLeaseDepositKrw !== 0) ||
      (response.activeLease !== null &&
        response.tenantLeaseDepositKrw !== response.activeLease.depositKrw)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['tenantLeaseDepositKrw'],
        message: 'current lease deposit must reconcile with the active lease',
      });
    }
    if (
      response.leaseCapability === 'cashJeonse' &&
      (response.activeLease?.offerKind === 'monthlyRent' ||
        response.activeArrears.length !== 0 ||
        response.hasMoreActiveArrears ||
        response.totalLeaseArrearKrw !== 0)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['leaseCapability'],
        message: 'cash-jeonse capability cannot expose monthly-rent state',
      });
    }
  });

export const HousingLeaseDepositLoanQuoteDraftSchema = z
  .object({
    listingId: HousingListingIdSchema,
    offerKind: z.literal('jeonse'),
    productVersionId: ResourceIdSchema,
    principalKrw: PositiveKrwSchema,
  })
  .strict();

export const HousingLeaseDepositLoanQuoteRequestSchema = z
  .object({
    ...LifeCommandCursorFields,
    listingId: HousingListingIdSchema,
    offerKind: z.literal('jeonse'),
    productVersionId: ResourceIdSchema,
    principalKrw: PositiveKrwSchema,
  })
  .strict();

export const HousingLeaseDepositLoanDecisionCodeSchema = z.enum([
  'creditRestricted',
  'collateralLimit',
  'incomeUnavailable',
  'affordabilityLimit',
  'eligible',
]);

export const HousingLeaseDepositLoanDecisionReasonSchema = z.enum([
  'activeDefault',
  'activeDelinquency',
  'activeRestructuring',
  'creditBandRestricted',
  'activeLoanLimit',
  'collateralLimit',
  'incomeUnavailable',
  'affordabilityLimit',
  'eligible',
]);

export const HousingLeaseDepositLoanAffordabilitySchema = z
  .object({
    numeratorKrw: NonnegativeKrwSchema,
    denominatorKrw: PositiveKrwSchema,
    ratioPpm: z.number().int().safe().nonnegative(),
    limitPpm: z.number().int().safe().positive(),
  })
  .strict()
  .superRefine((affordability, context) => {
    const expectedRatio =
      (BigInt(affordability.numeratorKrw) * 1_000_000n) / BigInt(affordability.denominatorKrw);
    if (expectedRatio !== BigInt(affordability.ratioPpm)) {
      context.addIssue({
        code: 'custom',
        path: ['ratioPpm'],
        message: 'affordability ratio must be the floored parts-per-million ratio',
      });
    }
  });

const HousingLeaseDepositLoanQuoteResultBaseSchema = z
  .object({
    quoteId: ResourceIdSchema,
    listingId: HousingListingIdSchema,
    offerKind: z.literal('jeonse'),
    productVersionId: ResourceIdSchema,
    requestedPrincipalKrw: PositiveKrwSchema,
    depositKrw: PositiveKrwSchema,
    fundingLimitPpm: z.number().int().safe().positive().max(1_000_000),
    maximumFundingKrw: PositiveKrwSchema,
    createdGameDay: z.number().int().safe().nonnegative(),
    expiresGameDay: z.number().int().safe().nonnegative(),
    decisionCode: HousingLeaseDepositLoanDecisionCodeSchema,
    decisionReasons: z.array(HousingLeaseDepositLoanDecisionReasonSchema).min(1).max(8),
    verifiedAnnualIncomeKrw: PositiveKrwSchema.nullable(),
    verifiedIncomeSource: VerifiedIncomeSourceSchema.nullable(),
    existingLoanBalanceKrw: NonnegativeKrwSchema,
    postExecutionBalanceKrw: PositiveKrwSchema,
    regulatoryDsrApplied: z.literal(false),
    affordability: HousingLeaseDepositLoanAffordabilitySchema.nullable(),
    quotedTerms: LoanQuotedTermsSchema,
    replacedLoanId: ResourceIdSchema.nullable(),
    replacedLoanPrincipalKrw: NonnegativeKrwSchema,
  })
  .strict();

type HousingLeaseDepositLoanQuoteResultValue = z.infer<
  typeof HousingLeaseDepositLoanQuoteResultBaseSchema
>;

export const HousingLeaseDepositLoanQuoteResultSchema =
  HousingLeaseDepositLoanQuoteResultBaseSchema.superRefine((quote, context) => {
    if (quote.expiresGameDay !== quote.createdGameDay) {
      context.addIssue({
        code: 'custom',
        path: ['expiresGameDay'],
        message: 'a lease-deposit loan quote must expire on its creation game day',
      });
    }

    const collateralLimit = (BigInt(quote.depositKrw) * BigInt(quote.fundingLimitPpm)) / 1_000_000n;
    if (BigInt(quote.maximumFundingKrw) > collateralLimit) {
      context.addIssue({
        code: 'custom',
        path: ['maximumFundingKrw'],
        message: 'maximum lease-deposit funding cannot exceed the published deposit limit',
      });
    }

    const hasReplacedLoan = quote.replacedLoanId !== null;
    if (hasReplacedLoan !== quote.replacedLoanPrincipalKrw > 0) {
      context.addIssue({
        code: 'custom',
        path: ['replacedLoanPrincipalKrw'],
        message: 'a replacement loan ID and positive principal must be present together',
      });
    }
    if (quote.replacedLoanPrincipalKrw > quote.existingLoanBalanceKrw) {
      context.addIssue({
        code: 'custom',
        path: ['replacedLoanPrincipalKrw'],
        message: 'replacement principal cannot exceed the existing loan balance',
      });
    }
    const expectedPostExecutionBalance =
      BigInt(quote.existingLoanBalanceKrw) -
      BigInt(quote.replacedLoanPrincipalKrw) +
      BigInt(quote.requestedPrincipalKrw);
    if (
      expectedPostExecutionBalance <= 0n ||
      expectedPostExecutionBalance !== BigInt(quote.postExecutionBalanceKrw)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['postExecutionBalanceKrw'],
        message: 'post-execution balance must include replacement and requested principal',
      });
    }

    if ((quote.verifiedAnnualIncomeKrw === null) !== (quote.verifiedIncomeSource === null)) {
      context.addIssue({
        code: 'custom',
        path: ['verifiedIncomeSource'],
        message: 'verified income amount and source must be present together',
      });
    }

    refineHousingLeaseDepositLoanDecision(quote, context);

    if (quote.quotedTerms.firstInstallment.dueGameDay <= quote.createdGameDay) {
      context.addIssue({
        code: 'custom',
        path: ['quotedTerms', 'firstInstallment', 'dueGameDay'],
        message: 'first installment must follow lease-deposit loan quote creation',
      });
    }
  });

function refineHousingLeaseDepositLoanDecision(
  quote: HousingLeaseDepositLoanQuoteResultValue,
  context: z.RefinementCtx,
): void {
  const reasonOrder = HousingLeaseDepositLoanDecisionReasonSchema.options;
  const uniqueReasons = new Set(quote.decisionReasons);
  const canonicalReasons = quote.decisionReasons.every((reason, index, reasons) => {
    const previous = reasons[index - 1];
    return previous === undefined || reasonOrder.indexOf(previous) < reasonOrder.indexOf(reason);
  });
  if (uniqueReasons.size !== quote.decisionReasons.length || !canonicalReasons) {
    context.addIssue({
      code: 'custom',
      path: ['decisionReasons'],
      message: 'lease-deposit loan reasons must be unique and use canonical priority',
    });
  }

  const onlyReason = quote.decisionReasons.length === 1 ? quote.decisionReasons[0] : undefined;
  const creditReasons = new Set([
    'activeDefault',
    'activeDelinquency',
    'activeRestructuring',
    'creditBandRestricted',
    'activeLoanLimit',
  ]);
  const reasonsMatch =
    quote.decisionCode === 'creditRestricted'
      ? quote.decisionReasons.every((reason) => creditReasons.has(reason))
      : onlyReason === quote.decisionCode;
  if (!reasonsMatch) {
    context.addIssue({
      code: 'custom',
      path: ['decisionReasons'],
      message: 'lease-deposit loan reasons must match the decision',
    });
  }

  const collateralExceeded = quote.requestedPrincipalKrw > quote.maximumFundingKrw;
  const collateralDecisionMatches =
    quote.decisionCode === 'creditRestricted' ||
    (quote.decisionCode === 'collateralLimit') === collateralExceeded;
  if (!collateralDecisionMatches) {
    context.addIssue({
      code: 'custom',
      path: ['decisionCode'],
      message: 'collateral-limit decision must match the maximum funding amount',
    });
  }

  const incomeUnavailable = quote.verifiedAnnualIncomeKrw === null;
  const incomeDecisionMatches =
    quote.decisionCode === 'creditRestricted' ||
    quote.decisionCode === 'collateralLimit' ||
    (quote.decisionCode === 'incomeUnavailable') === incomeUnavailable;
  if (!incomeDecisionMatches) {
    context.addIssue({
      code: 'custom',
      path: ['decisionCode'],
      message: 'income-unavailable decision must match verified income evidence',
    });
  }

  refineHousingLeaseDepositLoanAffordability(quote, context);
}

function refineHousingLeaseDepositLoanAffordability(
  quote: HousingLeaseDepositLoanQuoteResultValue,
  context: z.RefinementCtx,
): void {
  const affordability = quote.affordability;
  if (affordability !== null) {
    if (quote.verifiedAnnualIncomeKrw !== affordability.denominatorKrw) {
      context.addIssue({
        code: 'custom',
        path: ['affordability', 'denominatorKrw'],
        message: 'affordability must use verified annual income as its denominator',
      });
    }
    const exceeds = affordability.ratioPpm > affordability.limitPpm;
    if ((quote.decisionCode === 'affordabilityLimit') !== exceeds) {
      context.addIssue({
        code: 'custom',
        path: ['decisionCode'],
        message: 'affordability decision must match its ratio and limit',
      });
    }
  }

  const affordabilityRequired =
    quote.decisionCode === 'eligible' || quote.decisionCode === 'affordabilityLimit';
  if (affordabilityRequired !== (affordability !== null)) {
    context.addIssue({
      code: 'custom',
      path: ['affordability'],
      message: 'an evaluated affordability decision requires complete evidence',
    });
  }
}

export const HousingLeaseDepositLoanQuoteResponseSchema = z
  .object({
    result: HousingLeaseDepositLoanQuoteResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict();

const HousingCashLeaseDraftSchema = z
  .object({
    listingId: HousingListingIdSchema,
    offerKind: z.enum(['jeonse', 'monthlyRent']),
  })
  .strict();

const HousingFinancedJeonseLeaseDraftSchema = z
  .object({
    listingId: HousingListingIdSchema,
    offerKind: z.literal('jeonse'),
    loanQuoteId: ResourceIdSchema,
  })
  .strict();

export const HousingLeaseDraftSchema = z.union([
  HousingCashLeaseDraftSchema,
  HousingFinancedJeonseLeaseDraftSchema,
]);

const HousingCashLeaseRequestSchema = z
  .object({
    ...LifeCommandCursorFields,
    listingId: HousingListingIdSchema,
    offerKind: z.enum(['jeonse', 'monthlyRent']),
  })
  .strict();

const HousingFinancedJeonseLeaseRequestSchema = z
  .object({
    ...LifeCommandCursorFields,
    listingId: HousingListingIdSchema,
    offerKind: z.literal('jeonse'),
    loanQuoteId: ResourceIdSchema,
  })
  .strict();

export const HousingLeaseRequestSchema = z.union([
  HousingCashLeaseRequestSchema,
  HousingFinancedJeonseLeaseRequestSchema,
]);

export const HousingDepositLoanExecutionSchema = z
  .object({
    loanId: ResourceIdSchema,
    quoteId: ResourceIdSchema,
    productVersionId: ResourceIdSchema,
    principalKrw: PositiveKrwSchema,
    annualRateBp: z.number().int().safe().nonnegative().max(20_000),
    maturityGameDay: z.number().int().safe().positive(),
    firstInstallment: LoanQuoteFirstInstallmentSchema,
  })
  .strict();

export const HousingRepaidDepositLoanSchema = z
  .object({
    loanId: ResourceIdSchema,
    paymentId: ResourceIdSchema,
    principalKrw: PositiveKrwSchema,
  })
  .strict();

type HousingEndedLeaseResultValue = {
  readonly leaseId: string;
  readonly returnedDepositKrw: number;
  readonly endedLeaseId: string | null;
  readonly repaidDepositLoan: z.infer<typeof HousingRepaidDepositLoanSchema> | null;
};

export const HousingLeaseResultSchema = z
  .object({
    leaseId: ResourceIdSchema,
    residenceId: ResourceIdSchema,
    listingId: HousingListingIdSchema,
    offerKind: z.enum(['jeonse', 'monthlyRent']),
    regionKey: HousingRegionKeySchema,
    propertyType: HousingPropertyTypeSchema,
    exclusiveAreaSquareMeters: z.number().int().safe().positive(),
    depositKrw: PositiveKrwSchema,
    monthlyRentKrw: PositiveKrwSchema.nullable(),
    returnedDepositKrw: NonnegativeKrwSchema,
    movingCostKrw: PositiveKrwSchema,
    walletDeltaKrw: z.number().int().safe(),
    effectiveFromGameDay: z.number().int().safe().nonnegative(),
    endedLeaseId: ResourceIdSchema.nullable(),
    renewalRule: HousingLeaseRenewalRuleSchema,
    depositLoanExecution: HousingDepositLoanExecutionSchema.nullable(),
    repaidDepositLoan: HousingRepaidDepositLoanSchema.nullable(),
  })
  .strict()
  .superRefine((result, context) => {
    if (
      (result.offerKind === 'jeonse' && result.monthlyRentKrw !== null) ||
      (result.offerKind === 'monthlyRent' && result.monthlyRentKrw === null)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['monthlyRentKrw'],
        message: 'lease result rent must match its offer kind',
      });
    }
    const newLoanPrincipal = BigInt(result.depositLoanExecution?.principalKrw ?? 0);
    const repaidLoanPrincipal = BigInt(result.repaidDepositLoan?.principalKrw ?? 0);
    const expectedWalletDelta =
      BigInt(result.returnedDepositKrw) -
      repaidLoanPrincipal +
      newLoanPrincipal -
      BigInt(result.depositKrw) -
      BigInt(result.movingCostKrw);
    if (expectedWalletDelta !== BigInt(result.walletDeltaKrw)) {
      context.addIssue({
        code: 'custom',
        path: ['walletDeltaKrw'],
        message: 'wallet delta must reconcile returned deposit, new deposit, and moving cost',
      });
    }
    refineHousingEndedLease(result, context);
    if (result.depositLoanExecution !== null) {
      if (
        result.offerKind !== 'jeonse' ||
        result.depositLoanExecution.maturityGameDay <= result.effectiveFromGameDay ||
        result.depositLoanExecution.firstInstallment.dueGameDay <= result.effectiveFromGameDay ||
        result.depositLoanExecution.firstInstallment.dueGameDay >
          result.depositLoanExecution.maturityGameDay
      ) {
        context.addIssue({
          code: 'custom',
          path: ['depositLoanExecution'],
          message: 'deposit-loan execution must belong to jeonse and have a valid schedule',
        });
      }
    }
  });

function refineHousingEndedLease(
  result: HousingEndedLeaseResultValue,
  context: z.RefinementCtx,
): void {
  if (
    (result.endedLeaseId === null && result.returnedDepositKrw !== 0) ||
    (result.endedLeaseId !== null && result.returnedDepositKrw === 0) ||
    result.endedLeaseId === result.leaseId
  ) {
    context.addIssue({
      code: 'custom',
      path: ['endedLeaseId'],
      message: 'returned deposit must identify a distinct ended lease',
    });
  }
  if (result.repaidDepositLoan !== null && result.endedLeaseId === null) {
    context.addIssue({
      code: 'custom',
      path: ['repaidDepositLoan'],
      message: 'a repaid deposit loan must belong to the ended lease',
    });
  }
}

export const HousingLeaseResponseSchema = z
  .object({
    result: HousingLeaseResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict()
  .superRefine((response, context) => {
    const { result, snapshot } = response;
    const activeLease = snapshot.life.activeLease;
    const resultLeaseIsCurrent = activeLease?.id === result.leaseId;

    if (result.effectiveFromGameDay > snapshot.gameDay) {
      context.addIssue({
        code: 'custom',
        path: ['snapshot', 'gameDay'],
        message: 'the response snapshot cannot precede the lease move',
      });
    }
    if (
      !resultLeaseIsCurrent &&
      (!response.replayed || snapshot.gameDay <= result.effectiveFromGameDay)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['snapshot', 'life', 'activeLease'],
        message: 'a newly completed move must be the current lease and residence',
      });
      return;
    }
    if (activeLease === null || !resultLeaseIsCurrent) return;

    const activeLeaseMatchesResult =
      activeLease.listingId === result.listingId &&
      activeLease.offerKind === result.offerKind &&
      activeLease.regionKey === result.regionKey &&
      activeLease.propertyType === result.propertyType &&
      activeLease.exclusiveAreaSquareMeters === result.exclusiveAreaSquareMeters &&
      activeLease.depositKrw === result.depositKrw &&
      activeLease.monthlyRentKrw === result.monthlyRentKrw &&
      activeLease.effectiveFromGameDay === result.effectiveFromGameDay &&
      activeLease.renewalRule === result.renewalRule &&
      activeLease.depositLoanId === (result.depositLoanExecution?.loanId ?? null);
    const residence = snapshot.life.residence;
    const residenceMatchesResult =
      residence !== null &&
      residence.id === result.residenceId &&
      residence.regionKey === result.regionKey &&
      residence.tenureKind === result.offerKind &&
      residence.effectiveFromGameDay === result.effectiveFromGameDay;
    if (!activeLeaseMatchesResult || !residenceMatchesResult) {
      context.addIssue({
        code: 'custom',
        path: ['snapshot', 'life'],
        message: 'lease result must correlate with the current lease and residence',
      });
    }
  });

export const HousingPropertyHoldingsResponseSchema = z
  .object({
    purchaseCapability: HousingPurchaseCapabilitySchema,
    maximumActiveHoldings: z.number().int().safe().min(0).max(1),
    holdings: z.array(HousingPropertyHoldingSchema).max(4),
    totalPropertyBookValueKrw: NonnegativeKrwSchema,
  })
  .strict()
  .superRefine((response, context) => {
    const unavailable = response.purchaseCapability === 'unavailable';
    if (
      unavailable !== (response.maximumActiveHoldings === 0) ||
      (unavailable && (response.holdings.length !== 0 || response.totalPropertyBookValueKrw !== 0))
    ) {
      context.addIssue({
        code: 'custom',
        path: ['purchaseCapability'],
        message: 'purchase capability must match its active-holding projection',
      });
    }
    if (response.holdings.length > response.maximumActiveHoldings) {
      context.addIssue({
        code: 'custom',
        path: ['holdings'],
        message: 'active property holdings exceed the published household limit',
      });
    }
    const ids = new Set(response.holdings.map((holding) => holding.id));
    const listings = new Set(response.holdings.map((holding) => holding.listingId));
    const mortgageLoans = response.holdings
      .map((holding) => holding.mortgageLoanId)
      .filter((loanId): loanId is string => loanId !== null);
    const ordered = response.holdings.every((holding, index, holdings) => {
      const previous = holdings[index - 1];
      return previous === undefined || BigInt(previous.id) < BigInt(holding.id);
    });
    if (
      ids.size !== response.holdings.length ||
      listings.size !== response.holdings.length ||
      new Set(mortgageLoans).size !== mortgageLoans.length ||
      !ordered
    ) {
      context.addIssue({
        code: 'custom',
        path: ['holdings'],
        message: 'property holdings and liens must be unique and use canonical ID order',
      });
    }
    const total = response.holdings.reduce(
      (sum, holding) => sum + BigInt(holding.bookValueKrw),
      0n,
    );
    if (total !== BigInt(response.totalPropertyBookValueKrw)) {
      context.addIssue({
        code: 'custom',
        path: ['totalPropertyBookValueKrw'],
        message: 'property holding book values must reconcile with their public total',
      });
    }
  });

export const HousingMortgageQuoteDraftSchema = z
  .object({
    listingId: HousingListingIdSchema,
    productVersionId: ResourceIdSchema,
    principalKrw: PositiveKrwSchema,
  })
  .strict();

export const HousingMortgageQuoteRequestSchema = z
  .object({
    ...LifeCommandCursorFields,
    listingId: HousingListingIdSchema,
    productVersionId: ResourceIdSchema,
    principalKrw: PositiveKrwSchema,
  })
  .strict();

export const HousingMortgageQuoteDecisionCodeSchema = z.enum([
  'creditRestricted',
  'purchaseRestricted',
  'collateralLimit',
  'incomeUnavailable',
  'debtServiceLimit',
  'insufficientOwnFunds',
  'eligible',
]);

export const HousingMortgageQuoteDecisionReasonSchema = z.enum([
  'activeDefault',
  'activeDelinquency',
  'activeRestructuring',
  'creditBandRestricted',
  'activeLoanLimit',
  'activeHolding',
  'residenceChangedToday',
  'leaseExitRestricted',
  'collateralLimit',
  'incomeUnavailable',
  'debtServiceLimit',
  'insufficientOwnFunds',
  'eligible',
]);

export const HousingMortgageLtvRegionClassSchema = z.enum([
  'regulatedCapitalProxy',
  'nonRegulatedProxy',
]);

export const HousingMortgageStressTreatmentSchema = z.literal('fullTermFixed');

export const HousingMortgageLtvSchema = z
  .object({
    numeratorKrw: PositiveKrwSchema,
    denominatorKrw: PositiveKrwSchema,
    ratioPpm: z.number().int().safe().nonnegative(),
    limitPpm: z.number().int().safe().positive().max(1_000_000),
  })
  .strict()
  .superRefine((ltv, context) => {
    const ratio = (BigInt(ltv.numeratorKrw) * 1_000_000n) / BigInt(ltv.denominatorKrw);
    if (ratio !== BigInt(ltv.ratioPpm)) {
      context.addIssue({
        code: 'custom',
        path: ['ratioPpm'],
        message: 'mortgage LTV must be the floored parts-per-million ratio',
      });
    }
  });

const HousingMortgageQuoteResultBaseSchema = z
  .object({
    quoteId: ResourceIdSchema,
    listingId: HousingListingIdSchema,
    productVersionId: ResourceIdSchema,
    requestedPrincipalKrw: PositiveKrwSchema,
    purchasePriceKrw: PositiveKrwSchema,
    recognizedCollateralValueKrw: PositiveKrwSchema,
    ltvRegionClass: HousingMortgageLtvRegionClassSchema,
    ltvLimitPpm: z.number().int().safe().positive().max(1_000_000),
    maximumMortgageKrw: PositiveKrwSchema,
    ltv: HousingMortgageLtvSchema,
    createdGameDay: z.number().int().safe().nonnegative(),
    expiresGameDay: z.number().int().safe().nonnegative(),
    decisionCode: HousingMortgageQuoteDecisionCodeSchema,
    decisionReasons: z.array(HousingMortgageQuoteDecisionReasonSchema).min(1).max(8),
    verifiedAnnualIncomeKrw: PositiveKrwSchema.nullable(),
    verifiedIncomeSource: VerifiedIncomeSourceSchema.nullable(),
    existingLoanBalanceKrw: NonnegativeKrwSchema,
    postExecutionBalanceKrw: PositiveKrwSchema,
    dsrApplied: z.boolean(),
    dsr: LoanQuoteDsrSchema.nullable(),
    stressRateBp: z.literal(0),
    stressTreatment: HousingMortgageStressTreatmentSchema,
    acquisitionIncidentalCostKrw: PositiveKrwSchema,
    movingCostKrw: PositiveKrwSchema,
    returnedDepositKrw: NonnegativeKrwSchema,
    replacedLoanId: ResourceIdSchema.nullable(),
    replacedLoanPrincipalKrw: NonnegativeKrwSchema,
    availableBuyerCashKrw: NonnegativeKrwSchema,
    requiredBuyerCashKrw: NonnegativeKrwSchema,
    quotedTerms: LoanQuotedTermsSchema,
  })
  .strict();

type HousingMortgageQuoteResultValue = z.infer<typeof HousingMortgageQuoteResultBaseSchema>;

export const HousingMortgageQuoteResultSchema = HousingMortgageQuoteResultBaseSchema.superRefine(
  (quote, context) => {
    refineHousingMortgageQuoteAmounts(quote, context);
    refineHousingMortgageQuoteDecision(quote, context);
    refineHousingMortgageQuoteDsr(quote, context);
  },
);

function refineHousingMortgageQuoteAmounts(
  quote: HousingMortgageQuoteResultValue,
  context: z.RefinementCtx,
): void {
  if (quote.expiresGameDay !== quote.createdGameDay) {
    context.addIssue({
      code: 'custom',
      path: ['expiresGameDay'],
      message: 'a mortgage quote must expire on its creation game day',
    });
  }
  if (
    quote.recognizedCollateralValueKrw !== quote.purchasePriceKrw ||
    quote.ltv.numeratorKrw !== quote.requestedPrincipalKrw ||
    quote.ltv.denominatorKrw !== quote.recognizedCollateralValueKrw ||
    quote.ltv.limitPpm !== quote.ltvLimitPpm
  ) {
    context.addIssue({
      code: 'custom',
      path: ['ltv'],
      message: 'mortgage LTV evidence must use the exact sale price and requested principal',
    });
  }
  const ltvMaximum =
    (BigInt(quote.recognizedCollateralValueKrw) * BigInt(quote.ltvLimitPpm)) / 1_000_000n;
  if (BigInt(quote.maximumMortgageKrw) > ltvMaximum) {
    context.addIssue({
      code: 'custom',
      path: ['maximumMortgageKrw'],
      message: 'maximum mortgage cannot exceed the published LTV limit',
    });
  }
  const hasReplacedLoan = quote.replacedLoanId !== null;
  if (
    hasReplacedLoan !== quote.replacedLoanPrincipalKrw > 0 ||
    quote.replacedLoanPrincipalKrw > quote.existingLoanBalanceKrw ||
    quote.replacedLoanPrincipalKrw > quote.returnedDepositKrw
  ) {
    context.addIssue({
      code: 'custom',
      path: ['replacedLoanPrincipalKrw'],
      message: 'replaced lease loan evidence must identify a funded active principal',
    });
  }
  const postExecutionBalance =
    BigInt(quote.existingLoanBalanceKrw) -
    BigInt(quote.replacedLoanPrincipalKrw) +
    BigInt(quote.requestedPrincipalKrw);
  if (postExecutionBalance !== BigInt(quote.postExecutionBalanceKrw)) {
    context.addIssue({
      code: 'custom',
      path: ['postExecutionBalanceKrw'],
      message: 'post-execution balance must include replacement and mortgage principal',
    });
  }
  const unclampedRequiredBuyerCash =
    BigInt(quote.purchasePriceKrw) +
    BigInt(quote.acquisitionIncidentalCostKrw) +
    BigInt(quote.movingCostKrw) -
    BigInt(quote.requestedPrincipalKrw);
  const requiredBuyerCash = unclampedRequiredBuyerCash > 0n ? unclampedRequiredBuyerCash : 0n;
  if (requiredBuyerCash !== BigInt(quote.requiredBuyerCashKrw)) {
    context.addIssue({
      code: 'custom',
      path: ['requiredBuyerCashKrw'],
      message: 'required buyer cash must reconcile purchase funding without client estimates',
    });
  }
  if ((quote.verifiedAnnualIncomeKrw === null) !== (quote.verifiedIncomeSource === null)) {
    context.addIssue({
      code: 'custom',
      path: ['verifiedIncomeSource'],
      message: 'verified income amount and source must be present together',
    });
  }
  if (quote.quotedTerms.firstInstallment.dueGameDay <= quote.createdGameDay) {
    context.addIssue({
      code: 'custom',
      path: ['quotedTerms', 'firstInstallment', 'dueGameDay'],
      message: 'first mortgage installment must follow quote creation',
    });
  }
}

function refineHousingMortgageQuoteDecision(
  quote: HousingMortgageQuoteResultValue,
  context: z.RefinementCtx,
): void {
  const order = HousingMortgageQuoteDecisionReasonSchema.options;
  const reasons = new Set(quote.decisionReasons);
  const canonical = quote.decisionReasons.every((reason, index, items) => {
    const previous = items[index - 1];
    return previous === undefined || order.indexOf(previous) < order.indexOf(reason);
  });
  if (reasons.size !== quote.decisionReasons.length || !canonical) {
    context.addIssue({
      code: 'custom',
      path: ['decisionReasons'],
      message: 'mortgage quote reasons must be unique and use canonical priority',
    });
  }
  const onlyReason = quote.decisionReasons.length === 1 ? quote.decisionReasons[0] : undefined;
  const creditReasons = new Set([
    'activeDefault',
    'activeDelinquency',
    'activeRestructuring',
    'creditBandRestricted',
    'activeLoanLimit',
  ]);
  const purchaseReasons = new Set([
    'activeHolding',
    'residenceChangedToday',
    'leaseExitRestricted',
  ]);
  const reasonsMatch =
    quote.decisionCode === 'creditRestricted'
      ? quote.decisionReasons.every((reason) => creditReasons.has(reason))
      : quote.decisionCode === 'purchaseRestricted'
        ? quote.decisionReasons.every((reason) => purchaseReasons.has(reason))
        : onlyReason === quote.decisionCode;
  if (!reasonsMatch) {
    context.addIssue({
      code: 'custom',
      path: ['decisionReasons'],
      message: 'mortgage quote reasons must match its priority decision',
    });
  }

  const beforeCollateral =
    quote.decisionCode === 'creditRestricted' || quote.decisionCode === 'purchaseRestricted';
  if (
    !beforeCollateral &&
    (quote.decisionCode === 'collateralLimit') !==
      quote.requestedPrincipalKrw > quote.maximumMortgageKrw
  ) {
    context.addIssue({
      code: 'custom',
      path: ['decisionCode'],
      message: 'collateral decision must match the maximum mortgage evidence',
    });
  }
  const beforeOwnFunds = [
    'creditRestricted',
    'purchaseRestricted',
    'collateralLimit',
    'incomeUnavailable',
    'debtServiceLimit',
  ].includes(quote.decisionCode);
  if (
    !beforeOwnFunds &&
    (quote.decisionCode === 'insufficientOwnFunds') !==
      quote.availableBuyerCashKrw < quote.requiredBuyerCashKrw
  ) {
    context.addIssue({
      code: 'custom',
      path: ['decisionCode'],
      message: 'own-funds decision must match the server-authoritative cash evidence',
    });
  }
}

function refineHousingMortgageQuoteDsr(
  quote: HousingMortgageQuoteResultValue,
  context: z.RefinementCtx,
): void {
  refineHousingMortgageDsrShape(quote, context);
  refineHousingMortgageDsrDecision(quote, context);
  refinePostDsrHousingMortgageDecision(quote, context);
}

function refineHousingMortgageDsrShape(
  quote: HousingMortgageQuoteResultValue,
  context: z.RefinementCtx,
): void {
  if (!quote.dsrApplied && quote.dsr !== null) {
    context.addIssue({
      code: 'custom',
      path: ['dsr'],
      message: 'a mortgage outside the borrower DSR gate cannot expose DSR evidence',
    });
  }
  if (
    quote.dsr !== null &&
    (!quote.dsrApplied || quote.verifiedAnnualIncomeKrw !== quote.dsr.denominatorKrw)
  ) {
    context.addIssue({
      code: 'custom',
      path: ['dsr'],
      message: 'mortgage DSR must use the verified annual income denominator',
    });
  }
}

function refineHousingMortgageDsrDecision(
  quote: HousingMortgageQuoteResultValue,
  context: z.RefinementCtx,
): void {
  if (quote.decisionCode === 'incomeUnavailable') {
    if (!quote.dsrApplied || quote.verifiedAnnualIncomeKrw !== null || quote.dsr !== null) {
      context.addIssue({
        code: 'custom',
        path: ['decisionCode'],
        message: 'income-unavailable mortgage decisions require an applied DSR gate',
      });
    }
  }
  if (quote.decisionCode === 'debtServiceLimit') {
    if (quote.dsr === null || quote.dsr.ratioPpm <= quote.dsr.limitPpm) {
      context.addIssue({
        code: 'custom',
        path: ['decisionCode'],
        message: 'debt-service-limit decisions require complete over-limit DSR evidence',
      });
    }
  }
}

function refinePostDsrHousingMortgageDecision(
  quote: HousingMortgageQuoteResultValue,
  context: z.RefinementCtx,
): void {
  const passedIncomePriority =
    quote.decisionCode === 'debtServiceLimit' ||
    quote.decisionCode === 'insufficientOwnFunds' ||
    quote.decisionCode === 'eligible';
  if (
    passedIncomePriority &&
    quote.dsrApplied &&
    (quote.verifiedAnnualIncomeKrw === null || quote.dsr === null)
  ) {
    context.addIssue({
      code: 'custom',
      path: ['dsr'],
      message: 'a post-income mortgage decision requires complete DSR evidence when applied',
    });
  }
  if (
    passedIncomePriority &&
    quote.dsr !== null &&
    (quote.decisionCode === 'debtServiceLimit') !== quote.dsr.ratioPpm > quote.dsr.limitPpm
  ) {
    context.addIssue({
      code: 'custom',
      path: ['decisionCode'],
      message: 'post-income mortgage decision must match its DSR ratio and limit',
    });
  }
}

export const HousingMortgageQuoteResponseSchema = z
  .object({
    result: HousingMortgageQuoteResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict()
  .superRefine((response, context) => {
    if (response.replayed) return;
    const availableBuyerCash =
      BigInt(response.snapshot.cashKrw) +
      BigInt(response.result.returnedDepositKrw) -
      BigInt(response.result.replacedLoanPrincipalKrw);
    if (availableBuyerCash !== BigInt(response.result.availableBuyerCashKrw)) {
      context.addIssue({
        code: 'custom',
        path: ['result', 'availableBuyerCashKrw'],
        message: 'new mortgage quote cash must reconcile with its unchanged snapshot wallet',
      });
    }
  });

export const HousingPurchaseDraftSchema = z
  .object({
    listingId: HousingListingIdSchema,
    mortgageQuoteId: ResourceIdSchema.nullable(),
  })
  .strict();

export const HousingPurchaseRequestSchema = z
  .object({
    ...LifeCommandCursorFields,
    listingId: HousingListingIdSchema,
    mortgageQuoteId: ResourceIdSchema.nullable(),
  })
  .strict();

export const HousingMortgageExecutionSchema = z
  .object({
    loanId: ResourceIdSchema,
    quoteId: ResourceIdSchema,
    productVersionId: ResourceIdSchema,
    propertyHoldingId: ResourceIdSchema,
    principalKrw: PositiveKrwSchema,
    activatedGameDay: z.number().int().safe().nonnegative(),
    maturityGameDay: z.number().int().safe().positive(),
    annualRateBp: z.number().int().safe().nonnegative().max(20_000),
    repaymentMethod: LoanRepaymentMethodSchema,
    termMonths: z.number().int().min(1).max(65_535),
    firstInstallment: LoanQuoteFirstInstallmentSchema,
  })
  .strict()
  .superRefine((execution, context) => {
    if (
      execution.maturityGameDay <= execution.activatedGameDay ||
      execution.firstInstallment.dueGameDay <= execution.activatedGameDay ||
      execution.firstInstallment.dueGameDay > execution.maturityGameDay
    ) {
      context.addIssue({
        code: 'custom',
        path: ['firstInstallment'],
        message: 'mortgage execution must expose a valid post-activation schedule',
      });
    }
  });

export const HousingPurchaseResultSchema = z
  .object({
    holding: HousingPropertyHoldingSchema,
    residenceId: ResourceIdSchema,
    listingId: HousingListingIdSchema,
    purchasePriceKrw: PositiveKrwSchema,
    acquisitionIncidentalCostKrw: PositiveKrwSchema,
    movingCostKrw: PositiveKrwSchema,
    returnedDepositKrw: NonnegativeKrwSchema,
    walletDeltaKrw: z.number().int().safe(),
    effectiveFromGameDay: z.number().int().safe().nonnegative(),
    endedLeaseId: ResourceIdSchema.nullable(),
    repaidDepositLoan: HousingRepaidDepositLoanSchema.nullable(),
    mortgageExecution: HousingMortgageExecutionSchema.nullable(),
  })
  .strict()
  .superRefine((result, context) => {
    if (
      result.holding.listingId !== result.listingId ||
      result.holding.acquiredGameDay !== result.effectiveFromGameDay ||
      result.holding.acquisitionPriceKrw !== result.purchasePriceKrw ||
      result.holding.acquisitionIncidentalCostKrw !== result.acquisitionIncidentalCostKrw
    ) {
      context.addIssue({
        code: 'custom',
        path: ['holding'],
        message: 'purchase result must preserve immutable acquisition evidence in its holding',
      });
    }
    const execution = result.mortgageExecution;
    if (
      (execution === null) !== (result.holding.mortgageLoanId === null) ||
      (execution !== null &&
        (execution.loanId !== result.holding.mortgageLoanId ||
          execution.propertyHoldingId !== result.holding.id ||
          execution.activatedGameDay !== result.effectiveFromGameDay))
    ) {
      context.addIssue({
        code: 'custom',
        path: ['mortgageExecution'],
        message: 'mortgage execution and the property lien must identify each other',
      });
    }
    if (
      (result.endedLeaseId === null && result.returnedDepositKrw !== 0) ||
      (result.endedLeaseId !== null && result.returnedDepositKrw === 0) ||
      (result.repaidDepositLoan !== null && result.endedLeaseId === null)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['endedLeaseId'],
        message: 'returned deposit and lease-loan payoff must identify the ended lease',
      });
    }
    const walletDelta =
      BigInt(result.returnedDepositKrw) -
      BigInt(result.repaidDepositLoan?.principalKrw ?? 0) +
      BigInt(execution?.principalKrw ?? 0) -
      BigInt(result.purchasePriceKrw) -
      BigInt(result.acquisitionIncidentalCostKrw) -
      BigInt(result.movingCostKrw);
    if (walletDelta !== BigInt(result.walletDeltaKrw)) {
      context.addIssue({
        code: 'custom',
        path: ['walletDeltaKrw'],
        message: 'purchase wallet delta must reconcile direct funding and lease payoff',
      });
    }
  });

export const HousingPurchaseResponseSchema = z
  .object({
    result: HousingPurchaseResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict()
  .superRefine((response, context) => {
    const { result, snapshot } = response;
    if (result.effectiveFromGameDay > snapshot.gameDay) {
      context.addIssue({
        code: 'custom',
        path: ['snapshot', 'gameDay'],
        message: 'purchase response snapshot cannot precede its effective game day',
      });
    }
    if (response.replayed) return;

    const holding = snapshot.life.activePropertyHoldings.find(
      (candidate) => candidate.id === result.holding.id,
    );
    const residence = snapshot.life.residence;
    if (
      holding === undefined ||
      !sameHousingPropertyHolding(holding, result.holding) ||
      residence === null ||
      residence.id !== result.residenceId ||
      residence.tenureKind !== 'owner' ||
      residence.propertyHoldingId !== result.holding.id ||
      residence.regionKey !== result.holding.regionKey ||
      residence.effectiveFromGameDay !== result.effectiveFromGameDay ||
      snapshot.life.activeLease !== null ||
      snapshot.life.tenantLeaseDepositKrw !== 0
    ) {
      context.addIssue({
        code: 'custom',
        path: ['snapshot', 'life'],
        message: 'purchase response must correlate its holding, owner residence, and lease exit',
      });
    }
  });

function sameHousingPropertyHolding(
  left: z.infer<typeof HousingPropertyHoldingSchema>,
  right: z.infer<typeof HousingPropertyHoldingSchema>,
): boolean {
  return (
    left.id === right.id &&
    left.listingId === right.listingId &&
    left.status === right.status &&
    left.purpose === right.purpose &&
    left.regionKey === right.regionKey &&
    left.propertyType === right.propertyType &&
    left.exclusiveAreaSquareMeters === right.exclusiveAreaSquareMeters &&
    left.acquiredGameDay === right.acquiredGameDay &&
    left.acquisitionPriceKrw === right.acquisitionPriceKrw &&
    left.acquisitionIncidentalCostKrw === right.acquisitionIncidentalCostKrw &&
    left.bookValueKrw === right.bookValueKrw &&
    left.mortgageLoanId === right.mortgageLoanId
  );
}

export const HousingPropertyHistoryQuerySchema = z
  .object({
    before: ResourceIdSchema.optional(),
    limit: z.number().int().safe().min(1).max(20).optional(),
  })
  .strict();

export const HousingPropertySaleOrderStatusSchema = z.enum([
  'active',
  'filled',
  'cancelled',
  'rejected',
]);
export const HousingPropertySaleOrderRevisionKindSchema = z.enum(['listing', 'cancellation']);
export const HousingPropertySaleOrderRejectionReasonSchema = z.enum([
  'mortgageNotPayable',
  'insufficientProceeds',
  'policyUnsupported',
]);

export const HousingPropertySaleOrderCreateDraftSchema = z
  .object({ holdingId: ResourceIdSchema, askingPriceKrw: PositiveKrwSchema })
  .strict();

export const HousingPropertySaleOrderCreateRequestSchema = z
  .object({
    ...LifeCommandCursorFields,
    holdingId: ResourceIdSchema,
    askingPriceKrw: PositiveKrwSchema,
  })
  .strict();

export const HousingPropertySaleOrderRepriceDraftSchema = z
  .object({ orderId: ResourceIdSchema, askingPriceKrw: PositiveKrwSchema })
  .strict();

export const HousingPropertySaleOrderRepriceRequestSchema = z
  .object({ ...LifeCommandCursorFields, askingPriceKrw: PositiveKrwSchema })
  .strict();

export const HousingPropertySaleOrderCancelDraftSchema = z
  .object({ orderId: ResourceIdSchema })
  .strict();

export const HousingPropertySaleOrderCancelRequestSchema = z
  .object({ ...LifeCommandCursorFields })
  .strict();

const HousingPropertySaleOrderListingResultBaseSchema = z
  .object({
    orderId: ResourceIdSchema,
    holdingId: ResourceIdSchema,
    revisionNo: z.number().int().safe().positive(),
    askingPriceKrw: PositiveKrwSchema,
    referenceValueKrw: PositiveKrwSchema,
    askingToReferencePpm: z.number().int().safe().min(800_000).max(1_200_000),
    candidateGameDay: z.number().int().safe().nonnegative(),
    status: z.literal('active'),
  })
  .strict();

export const HousingPropertySaleOrderListingResultSchema =
  HousingPropertySaleOrderListingResultBaseSchema.superRefine((result, context) => {
    const ratio = (BigInt(result.askingPriceKrw) * 1_000_000n) / BigInt(result.referenceValueKrw);
    if (ratio !== BigInt(result.askingToReferencePpm)) {
      context.addIssue({
        code: 'custom',
        path: ['askingToReferencePpm'],
        message: 'property sale asking/reference ratio must use exact integer flooring',
      });
    }
  });

export const HousingPropertySaleOrderCancellationResultSchema = z
  .object({
    orderId: ResourceIdSchema,
    holdingId: ResourceIdSchema,
    revisionNo: z.number().int().safe().positive(),
    cancelledGameDay: z.number().int().safe().nonnegative(),
    status: z.literal('cancelled'),
  })
  .strict();

export const HousingPropertySaleOrderListingResponseSchema = z
  .object({
    result: HousingPropertySaleOrderListingResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict();

export const HousingPropertySaleOrderCancellationResponseSchema = z
  .object({
    result: HousingPropertySaleOrderCancellationResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict();

export const HousingPropertySaleExecutionSchema = z
  .object({
    filledGameDay: z.number().int().safe().nonnegative(),
    grossSalePriceKrw: PositiveKrwSchema,
    transactionCostKrw: PositiveKrwSchema,
    mortgagePrincipalKrw: NonnegativeKrwSchema,
    mortgageFeeKrw: NonnegativeKrwSchema,
    capitalGainsTaxKrw: NonnegativeKrwSchema,
    walletProceedsKrw: NonnegativeKrwSchema,
    realizedGainLossKrw: z.number().int().safe(),
  })
  .strict()
  .superRefine((execution, context) => {
    const wallet =
      BigInt(execution.grossSalePriceKrw) -
      BigInt(execution.transactionCostKrw) -
      BigInt(execution.mortgagePrincipalKrw) -
      BigInt(execution.mortgageFeeKrw) -
      BigInt(execution.capitalGainsTaxKrw);
    if (wallet !== BigInt(execution.walletProceedsKrw)) {
      context.addIssue({
        code: 'custom',
        path: ['walletProceedsKrw'],
        message: 'property sale proceeds must reconcile the server waterfall',
      });
    }
  });

const HousingPropertySaleOrderSummaryBaseSchema = z
  .object({
    orderId: ResourceIdSchema,
    holdingId: ResourceIdSchema,
    revisionNo: z.number().int().safe().positive(),
    revisionKind: HousingPropertySaleOrderRevisionKindSchema,
    askingPriceKrw: PositiveKrwSchema.nullable(),
    referenceValueKrw: PositiveKrwSchema.nullable(),
    askingToReferencePpm: z.number().int().safe().min(800_000).max(1_200_000).nullable(),
    candidateGameDay: z.number().int().safe().nonnegative().nullable(),
    status: HousingPropertySaleOrderStatusSchema,
    cancelledGameDay: z.number().int().safe().nonnegative().nullable(),
    rejectionReason: HousingPropertySaleOrderRejectionReasonSchema.nullable(),
    execution: HousingPropertySaleExecutionSchema.nullable(),
  })
  .strict();

type HousingPropertySaleOrderSummaryValue = z.infer<
  typeof HousingPropertySaleOrderSummaryBaseSchema
>;

export const HousingPropertySaleOrderSummarySchema =
  HousingPropertySaleOrderSummaryBaseSchema.superRefine((order, context) =>
    refineHousingPropertySaleOrder(order, context),
  );

function refineHousingPropertySaleOrder(
  order: HousingPropertySaleOrderSummaryValue,
  context: z.RefinementCtx,
): void {
  const listingValues = [
    order.askingPriceKrw,
    order.referenceValueKrw,
    order.askingToReferencePpm,
    order.candidateGameDay,
  ];
  const allListingValues = listingValues.every((value) => value !== null);
  const noListingValues = listingValues.every((value) => value === null);
  if (
    (order.revisionKind === 'listing' && !allListingValues) ||
    (order.revisionKind === 'cancellation' && !noListingValues)
  ) {
    context.addIssue({
      code: 'custom',
      path: ['revisionKind'],
      message: 'property sale revision kind must match its nullable listing values',
    });
    return;
  }
  if (
    order.askingPriceKrw !== null &&
    order.referenceValueKrw !== null &&
    order.askingToReferencePpm !== null &&
    (BigInt(order.askingPriceKrw) * 1_000_000n) / BigInt(order.referenceValueKrw) !==
      BigInt(order.askingToReferencePpm)
  ) {
    context.addIssue({
      code: 'custom',
      path: ['askingToReferencePpm'],
      message: 'property sale summary ratio is inconsistent',
    });
  }
  const validStatusShape =
    (order.status === 'active' &&
      order.revisionKind === 'listing' &&
      order.cancelledGameDay === null &&
      order.rejectionReason === null &&
      order.execution === null) ||
    (order.status === 'filled' &&
      order.revisionKind === 'listing' &&
      order.cancelledGameDay === null &&
      order.rejectionReason === null &&
      order.execution !== null) ||
    (order.status === 'cancelled' &&
      order.revisionKind === 'cancellation' &&
      order.cancelledGameDay !== null &&
      order.rejectionReason === null &&
      order.execution === null) ||
    (order.status === 'rejected' &&
      order.revisionKind === 'listing' &&
      order.cancelledGameDay === null &&
      order.rejectionReason !== null &&
      order.execution === null);
  if (!validStatusShape) {
    context.addIssue({
      code: 'custom',
      path: ['status'],
      message: 'property sale order status has inconsistent terminal evidence',
    });
  }
  if (
    order.execution !== null &&
    (order.candidateGameDay !== order.execution.filledGameDay ||
      order.askingPriceKrw !== order.execution.grossSalePriceKrw)
  ) {
    context.addIssue({
      code: 'custom',
      path: ['execution'],
      message: 'property sale execution must match the accepted revision',
    });
  }
}

export const HousingPropertySaleOrdersResponseSchema = z
  .object({
    items: z.array(HousingPropertySaleOrderSummarySchema).max(20),
    nextBefore: ResourceIdSchema.nullable(),
  })
  .strict()
  .superRefine((page, context) => {
    const canonical = page.items.every((item, index, items) => {
      const previous = items[index - 1];
      return previous === undefined || BigInt(previous.orderId) > BigInt(item.orderId);
    });
    const last = page.items.at(-1);
    if (!canonical || (page.nextBefore !== null && last?.orderId !== page.nextBefore)) {
      context.addIssue({
        code: 'custom',
        path: ['items'],
        message: 'property sale page must use descending unique IDs and an oldest-item cursor',
      });
    }
  });

export const HousingPropertyTaxEventKindSchema = z.enum([
  'acquisition',
  'annualHolding',
  'capitalGains',
]);
export const HousingPropertyTaxEventStatusSchema = z.enum([
  'scheduled',
  'partiallyPaid',
  'paid',
  'noPaymentRequired',
]);
export const HousingPropertyTaxPaymentStatusSchema = z.enum(['pending', 'applied', 'cancelled']);

export const HousingPropertyTaxComponentSchema = z
  .object({
    componentKey: z.string().min(1).max(64),
    componentOrder: z.number().int().safe().nonnegative().max(255),
    taxBaseKrw: NonnegativeKrwSchema,
    deductionKrw: NonnegativeKrwSchema,
    taxableAmountKrw: NonnegativeKrwSchema,
    ratePpm: z.number().int().safe().nonnegative().max(1_000_000),
    progressiveDeductionKrw: NonnegativeKrwSchema,
    amountKrw: NonnegativeKrwSchema,
  })
  .strict();

export const HousingPropertyTaxPaymentSchema = z
  .object({
    paymentNo: z.number().int().safe().positive().max(255),
    dueGameDay: z.number().int().safe().nonnegative(),
    paidGameDay: z.number().int().safe().nonnegative().nullable(),
    status: HousingPropertyTaxPaymentStatusSchema,
    amountKrw: NonnegativeKrwSchema,
    walletPaidKrw: NonnegativeKrwSchema,
    taxObligationKrw: NonnegativeKrwSchema,
  })
  .strict()
  .superRefine((payment, context) => {
    const funded = BigInt(payment.walletPaidKrw) + BigInt(payment.taxObligationKrw);
    const statusShapeIsValid =
      ((payment.status === 'pending' || payment.status === 'cancelled') &&
        payment.paidGameDay === null &&
        funded === 0n) ||
      (payment.status === 'applied' &&
        payment.paidGameDay !== null &&
        funded === BigInt(payment.amountKrw));
    if (!statusShapeIsValid) {
      context.addIssue({
        code: 'custom',
        path: ['status'],
        message: 'property tax payment status and funding evidence are inconsistent',
      });
    }
  });

const HousingPropertyTaxEventBaseSchema = z
  .object({
    id: ResourceIdSchema,
    holdingId: ResourceIdSchema,
    policySetId: ResourceIdSchema,
    policyKey: z.string().min(1).max(120),
    ruleId: ResourceIdSchema,
    ruleKey: z.string().min(1).max(120),
    legalBasisDate: z.iso.date(),
    kind: HousingPropertyTaxEventKindSchema,
    status: HousingPropertyTaxEventStatusSchema,
    assessedGameDay: z.number().int().safe().nonnegative(),
    taxableGameDay: z.number().int().safe().nonnegative(),
    paidGameDay: z.number().int().safe().nonnegative().nullable(),
    householdHomeCount: z.number().int().safe().positive().max(255),
    grossAmountKrw: NonnegativeKrwSchema,
    valuationGameDay: z.number().int().safe().nonnegative().nullable(),
    valuationPriceIndexPpm: z.number().int().safe().positive().nullable(),
    officialValueKrw: NonnegativeKrwSchema.nullable(),
    taxBaseKrw: NonnegativeKrwSchema,
    deductionKrw: NonnegativeKrwSchema,
    taxableAmountKrw: NonnegativeKrwSchema,
    totalTaxKrw: NonnegativeKrwSchema,
    components: z.array(HousingPropertyTaxComponentSchema).max(16),
    payments: z.array(HousingPropertyTaxPaymentSchema).max(2),
    exclusionCodes: z.array(z.string().min(1).max(64)).max(16),
  })
  .strict();

type HousingPropertyTaxEventValue = z.infer<typeof HousingPropertyTaxEventBaseSchema>;

export const HousingPropertyTaxEventSchema = HousingPropertyTaxEventBaseSchema.superRefine(
  (event, context) => refineHousingPropertyTaxEvent(event, context),
);

function refineHousingPropertyTaxEvent(
  event: HousingPropertyTaxEventValue,
  context: z.RefinementCtx,
): void {
  const hasMarketValuation =
    event.valuationGameDay !== null && event.valuationPriceIndexPpm !== null;
  const annualValuation = hasMarketValuation && event.officialValueKrw !== null;
  const transactionValuation = hasMarketValuation && event.officialValueKrw === null;
  if (
    (event.kind === 'annualHolding' && !annualValuation) ||
    (event.kind !== 'annualHolding' && !transactionValuation)
  ) {
    context.addIssue({
      code: 'custom',
      path: ['valuationGameDay'],
      message: 'property tax valuation evidence must match the tax kind',
    });
  }
  const componentsCanonical = event.components.every((component, index, items) => {
    const previous = items[index - 1];
    return previous === undefined || previous.componentOrder < component.componentOrder;
  });
  const paymentsCanonical = event.payments.every((payment, index, items) => {
    const previous = items[index - 1];
    return previous === undefined || previous.paymentNo < payment.paymentNo;
  });
  const componentTotal = event.components.reduce(
    (sum, component) => sum + BigInt(component.amountKrw),
    0n,
  );
  const paymentTotal = event.payments.reduce((sum, payment) => sum + BigInt(payment.amountKrw), 0n);
  if (
    !componentsCanonical ||
    !paymentsCanonical ||
    componentTotal !== BigInt(event.totalTaxKrw) ||
    paymentTotal !== BigInt(event.totalTaxKrw) ||
    (event.status === 'noPaymentRequired') !== (event.totalTaxKrw === 0)
  ) {
    context.addIssue({
      code: 'custom',
      path: ['totalTaxKrw'],
      message: 'property tax components, payments, and total must reconcile in canonical order',
    });
  }
}

export const HousingPropertyTaxEventsResponseSchema = z
  .object({
    holdingId: ResourceIdSchema,
    items: z.array(HousingPropertyTaxEventSchema).max(20),
    nextBefore: ResourceIdSchema.nullable(),
  })
  .strict()
  .superRefine((page, context) => {
    const canonical = page.items.every((item, index, items) => {
      const previous = items[index - 1];
      return (
        item.holdingId === page.holdingId &&
        (previous === undefined || BigInt(previous.id) > BigInt(item.id))
      );
    });
    const last = page.items.at(-1);
    if (!canonical || (page.nextBefore !== null && last?.id !== page.nextBefore)) {
      context.addIssue({
        code: 'custom',
        path: ['items'],
        message: 'property tax page must keep one holding and an oldest-item cursor',
      });
    }
  });

export const HousingLeaseArrearPaymentDraftSchema = z
  .object({
    arrearId: ResourceIdSchema,
    amountKrw: PositiveKrwSchema,
  })
  .strict();

export const HousingLeaseArrearPaymentRequestSchema = z
  .object({
    ...LifeCommandCursorFields,
    amountKrw: PositiveKrwSchema,
  })
  .strict();

export const HousingLeaseArrearPaymentResultSchema = z
  .object({
    arrearId: ResourceIdSchema,
    paymentId: ResourceIdSchema,
    paidKrw: PositiveKrwSchema,
    remainingKrw: NonnegativeKrwSchema,
  })
  .strict();

export const HousingLeaseArrearPaymentResponseSchema = z
  .object({
    result: HousingLeaseArrearPaymentResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict();

export const AdvanceRequestSchema = z.object({
  commandId: CanonicalUuidSchema,
  expectedRunRevision: z.number().int().nonnegative(),
  expectedStateRevision: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
  expectedGameDay: z.number().int().nonnegative(),
  days: z.number().int().min(1).max(30),
});

export const AdvanceResultSchema = z.object({
  commandId: CanonicalUuidSchema,
  requestedDays: z.number().int().min(1).max(30),
  initialCursor: GameCommandCursorSchema,
  committedCursor: GameCommandCursorSchema,
  replayed: z.boolean(),
});

export const AdvanceResponseSchema = z.object({
  advance: AdvanceResultSchema,
  snapshot: GameSnapshotSchema,
});

export const GameCommandFailureCodeSchema = z.enum([
  'invalidCommand',
  'characterRequired',
  'idempotencyConflict',
  'busy',
]);

export const GameCommandFailureSchema = z.object({
  code: GameCommandFailureCodeSchema,
  message: z.string().min(1),
});

export const CareerFailureCodeSchema = z.enum([
  'invalidCommand',
  'characterRequired',
  'policyUnavailable',
  'catalogUnavailable',
  'notEligible',
  'activityLimit',
  'artifactRequired',
  'postingClosed',
  'applicationLimit',
  'alreadyApplied',
  'interviewExpired',
  'offerExpired',
  'alreadyEmployed',
  'militaryStateConflict',
  'insufficientWalletCash',
  'limitExceeded',
  'idempotencyConflict',
  'settlementConflict',
  'busy',
]);

export const CareerFailureSchema = z
  .object({
    code: CareerFailureCodeSchema,
    message: z.string().min(1),
  })
  .strict();

export const CareerEvidenceSchema = z
  .object({
    id: ResourceIdSchema,
    evidenceKey: z.string().min(1).max(96),
    catalogEntryId: ResourceIdSchema,
    catalogEntryKey: z.string().min(1).max(96),
    displayName: z.string().min(1).max(120),
    kind: EvidenceKindSchema,
    acquiredGameDay: z.number().int().nonnegative(),
    expiresOnGameDay: z.number().int().nonnegative().nullable(),
    periodStartDate: z.iso.date().nullable(),
    periodEndExclusiveDate: z.iso.date().nullable(),
    creditedExperienceDays: z.number().int().safe().nonnegative().nullable(),
  })
  .strict()
  .superRefine((evidence, context) => {
    if ((evidence.periodStartDate === null) !== (evidence.periodEndExclusiveDate === null)) {
      context.addIssue({
        code: 'custom',
        path: ['periodStartDate'],
        message: 'evidence period dates must appear together',
      });
    }
    if ((evidence.kind === 'experience') !== (evidence.creditedExperienceDays !== null)) {
      context.addIssue({
        code: 'custom',
        path: ['creditedExperienceDays'],
        message: 'credited experience days belong only to experience evidence',
      });
    }
  });

export const CareerSpecsResponseSchema = z
  .object({
    focusedJobFamilyKey: z.string().min(1).max(64),
    possessedScores: CareerScoresSchema,
    items: z.array(CareerEvidenceSchema).max(200),
    nextBefore: ResourceIdSchema.nullable(),
  })
  .strict()
  .superRefine((page, context) => refineCareerPage(page.items, page.nextBefore, context));

export const CareerActivityCatalogEntrySchema = z
  .object({
    id: ResourceIdSchema,
    activityKey: z.string().min(1).max(96),
    displayName: z.string().min(1).max(120),
    outputKind: EvidenceKindSchema,
    minimumCalendarDays: z.number().int().positive(),
    requiredEffortUnits: z.number().int().safe().positive(),
    dailyEffortCapUnits: z.number().int().safe().positive(),
    allowedLifeStatuses: z.array(LifeStatusSchema).min(1).max(6),
    costKrw: z.number().int().safe().nonnegative(),
  })
  .strict();

export const CareerActivityHistoryItemSchema = CareerActivitySummarySchema.extend({
  cancelledGameDay: z.number().int().nonnegative().nullable(),
}).strict();

export const CareerActivitiesResponseSchema = z
  .object({
    catalog: z.array(CareerActivityCatalogEntrySchema).max(200),
    active: z.array(CareerActivitySummarySchema).max(3),
    items: z.array(CareerActivityHistoryItemSchema).max(200),
    nextBefore: ResourceIdSchema.nullable(),
  })
  .strict()
  .superRefine((page, context) => refineCareerPage(page.items, page.nextBefore, context));

const ArtifactHeadlineSchema = z
  .string()
  .trim()
  .refine((value) => Array.from(value).length >= 1 && Array.from(value).length <= 120)
  .refine((value) => Array.from(value).every(isArtifactHeadlineScalar));
const ArtifactSummarySchema = z
  .string()
  .trim()
  .refine((value) => Array.from(value).length <= 2_000)
  .refine((value) => Array.from(value).every(isArtifactSummaryScalar));

function isArtifactHeadlineScalar(value: string): boolean {
  const scalar = value.codePointAt(0);
  return scalar !== undefined && scalar > 0x1f && ![0x85, 0x2028, 0x2029].includes(scalar);
}

function isArtifactSummaryScalar(value: string): boolean {
  const scalar = value.codePointAt(0);
  return scalar !== undefined && (scalar > 0x1f || scalar === 0x09 || scalar === 0x0a);
}

const CareerArtifactCommonSchema = z
  .object({
    id: ResourceIdSchema,
    versionNo: z.number().int().positive(),
    headline: ArtifactHeadlineSchema,
    summary: ArtifactSummarySchema,
    completenessBp: z.number().int().min(0).max(10_000),
    createdGameDay: z.number().int().nonnegative(),
  })
  .strict();

const PortfolioEvidenceIdsSchema = z.array(ResourceIdSchema).max(12);
const ResumeEvidenceIdsSchema = z.array(ResourceIdSchema).max(40);
const LinkedinEvidenceIdsSchema = z.array(ResourceIdSchema).max(30);

export const PortfolioArtifactSchema = CareerArtifactCommonSchema.extend({
  kind: z.literal('portfolio'),
  evidenceIds: PortfolioEvidenceIdsSchema,
}).strict();
export const ResumeArtifactSchema = CareerArtifactCommonSchema.extend({
  kind: z.literal('resume'),
  evidenceIds: ResumeEvidenceIdsSchema,
}).strict();
export const LinkedinArtifactSchema = CareerArtifactCommonSchema.extend({
  kind: z.literal('linkedinProfile'),
  evidenceIds: LinkedinEvidenceIdsSchema,
  openToWork: z.boolean(),
  industries: z.array(CareerIndustrySchema).max(3),
}).strict();
export const CareerArtifactSchema = z.discriminatedUnion('kind', [
  PortfolioArtifactSchema,
  ResumeArtifactSchema,
  LinkedinArtifactSchema,
]);

export const CareerArtifactsResponseSchema = z
  .object({
    items: z.array(CareerArtifactSchema).max(200),
    nextBefore: ResourceIdSchema.nullable(),
  })
  .strict()
  .superRefine((page, context) => refineCareerPage(page.items, page.nextBefore, context));

function refineCareerPage(
  items: readonly { readonly id: string }[],
  nextBefore: string | null,
  context: z.RefinementCtx,
): void {
  for (let index = 1; index < items.length; index += 1) {
    const previous = items[index - 1];
    const current = items[index];
    if (
      previous !== undefined &&
      current !== undefined &&
      BigInt(previous.id) <= BigInt(current.id)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['items', index, 'id'],
        message: 'career page items must be ordered by descending ID',
      });
    }
  }
  const oldest = items.at(-1)?.id ?? null;
  if (nextBefore !== null && nextBefore !== oldest) {
    context.addIssue({
      code: 'custom',
      path: ['nextBefore'],
      message: 'non-null career page cursor must equal the oldest returned ID',
    });
  }
}

const CareerCommandCursorFields = {
  commandId: CanonicalUuidSchema,
  expectedRunRevision: z.number().int().nonnegative(),
  expectedStateRevision: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
  expectedGameDay: z.number().int().nonnegative(),
} as const;

export const CareerFocusDraftSchema = z
  .object({ focusedJobFamilyKey: z.string().min(1).max(64) })
  .strict();
export const CareerFocusRequestSchema = z
  .object({ ...CareerCommandCursorFields, focusedJobFamilyKey: z.string().min(1).max(64) })
  .strict();
export const CareerActivityStartDraftSchema = z
  .object({
    activityCatalogEntryId: ResourceIdSchema,
    priority: z.number().int().min(1).max(3),
  })
  .strict();
export const CareerActivityStartRequestSchema = z
  .object({
    ...CareerCommandCursorFields,
    activityCatalogEntryId: ResourceIdSchema,
    priority: z.number().int().min(1).max(3),
  })
  .strict();
export const CareerCursorRequestSchema = z.object(CareerCommandCursorFields).strict();

const ArtifactDraftCommonFields = {
  headline: ArtifactHeadlineSchema,
  summary: ArtifactSummarySchema,
} as const;
export const PortfolioArtifactDraftSchema = z
  .object({
    ...ArtifactDraftCommonFields,
    kind: z.literal('portfolio'),
    evidenceIds: PortfolioEvidenceIdsSchema,
  })
  .strict();
export const ResumeArtifactDraftSchema = z
  .object({
    ...ArtifactDraftCommonFields,
    kind: z.literal('resume'),
    evidenceIds: ResumeEvidenceIdsSchema,
  })
  .strict();
export const LinkedinArtifactDraftSchema = z
  .object({
    ...ArtifactDraftCommonFields,
    kind: z.literal('linkedinProfile'),
    evidenceIds: LinkedinEvidenceIdsSchema,
    openToWork: z.boolean(),
    industries: z.array(CareerIndustrySchema).max(3),
  })
  .strict();
export const CareerArtifactDraftSchema = z
  .discriminatedUnion('kind', [
    PortfolioArtifactDraftSchema,
    ResumeArtifactDraftSchema,
    LinkedinArtifactDraftSchema,
  ])
  .superRefine((artifact, context) => {
    const evidence = new Set(artifact.evidenceIds);
    if (evidence.size !== artifact.evidenceIds.length) {
      context.addIssue({
        code: 'custom',
        path: ['evidenceIds'],
        message: 'evidence IDs must be unique',
      });
    }
    if (
      'industries' in artifact &&
      new Set(artifact.industries).size !== artifact.industries.length
    ) {
      context.addIssue({
        code: 'custom',
        path: ['industries'],
        message: 'industries must be unique',
      });
    }
  });

export const CareerArtifactPublishRequestSchema = z
  .discriminatedUnion('kind', [
    z
      .object({
        ...CareerCommandCursorFields,
        ...ArtifactDraftCommonFields,
        kind: z.literal('portfolio'),
        evidenceIds: PortfolioEvidenceIdsSchema,
      })
      .strict(),
    z
      .object({
        ...CareerCommandCursorFields,
        ...ArtifactDraftCommonFields,
        kind: z.literal('resume'),
        evidenceIds: ResumeEvidenceIdsSchema,
      })
      .strict(),
    z
      .object({
        ...CareerCommandCursorFields,
        ...ArtifactDraftCommonFields,
        kind: z.literal('linkedinProfile'),
        evidenceIds: LinkedinEvidenceIdsSchema,
        openToWork: z.boolean(),
        industries: z.array(CareerIndustrySchema).max(3),
      })
      .strict(),
  ])
  .superRefine((artifact, context) => {
    if (new Set(artifact.evidenceIds).size !== artifact.evidenceIds.length) {
      context.addIssue({
        code: 'custom',
        path: ['evidenceIds'],
        message: 'evidence IDs must be unique',
      });
    }
    if (
      'industries' in artifact &&
      new Set(artifact.industries).size !== artifact.industries.length
    ) {
      context.addIssue({
        code: 'custom',
        path: ['industries'],
        message: 'industries must be unique',
      });
    }
  });

export const CareerFocusResultSchema = z
  .object({ focusedJobFamilyKey: z.string().min(1).max(64) })
  .strict();
export const CareerActivityResultSchema = z
  .object({ activityId: ResourceIdSchema, status: CareerActivityStatusSchema })
  .strict();
export const CareerArtifactResultSchema = z
  .object({
    artifactVersionId: ResourceIdSchema,
    kind: CareerArtifactKindSchema,
    versionNo: z.number().int().positive(),
  })
  .strict();
export const CareerFocusResponseSchema = z
  .object({ result: CareerFocusResultSchema, replayed: z.boolean(), snapshot: GameSnapshotSchema })
  .strict();
export const CareerActivityResponseSchema = z
  .object({
    result: CareerActivityResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict();
export const CareerArtifactResponseSchema = z
  .object({
    result: CareerArtifactResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict();

// -- Recruitment and employment (M3-B) ---------------------------------

export const EducationSchema = z.enum([
  'highSchool',
  'associate',
  'bachelor',
  'master',
  'doctorate',
]);
export const RegionSchema = z.enum(['capitalArea', 'metropolitan', 'smallCity', 'rural']);
export const CareerPlatformSchema = z.enum([
  'sarangbang',
  'jobkorea',
  'saramin',
  'wanted',
  'linkedin',
  'work24',
]);
export const PostingKeySchema = z
  .string()
  .regex(/^[0-9a-f]{64}$/, 'posting key must be lowercase SHA-256 hex');
export const CareerCompetitionBandSchema = z.enum(['low', 'medium', 'high']);
export const CareerMilitaryRequirementSchema = z.enum(['any', 'completedOrExempt']);
export const CareerEmploymentTypeSchema = z.literal('regular');
export const CareerApplicationStatusSchema = z.enum([
  'submitted',
  'documentRejected',
  'interviewAwaitingConfirmation',
  'interviewConfirmed',
  'interviewRejected',
  'offered',
  'accepted',
  'declined',
  'expired',
  'withdrawn',
  'closed',
]);
export const CareerInvitationStatusSchema = z.enum([
  'open',
  'accepted',
  'declined',
  'expired',
  'closed',
]);
export const EmploymentStatusSchema = z.enum(['pendingStart', 'active', 'ended']);

export const CareerJobSchema = z
  .object({
    postingKey: PostingKeySchema,
    postedGameDay: z.number().int().nonnegative(),
    closesExclusiveGameDay: z.number().int().nonnegative(),
    platform: CareerPlatformSchema,
    industry: CareerIndustrySchema,
    jobFamilyKey: z.string().min(1).max(64),
    employerName: z.string().min(1).max(120),
    region: RegionSchema,
    employmentType: CareerEmploymentTypeSchema,
    requiredScores: CareerScoresSchema,
    possessedScores: CareerScoresSchema,
    minimumAnnualSalaryKrw: z.number().int().safe().nonnegative(),
    maximumAnnualSalaryKrw: z.number().int().safe().nonnegative(),
    salaryStepKrw: z.number().int().safe().positive(),
    competitionBand: CareerCompetitionBandSchema,
    militaryRequirement: CareerMilitaryRequirementSchema,
    minimumEducation: EducationSchema.nullable(),
    requiredCertificationName: z.string().min(1).max(120).nullable(),
    minimumExperienceDays: z.number().int().safe().nonnegative(),
    requiredArtifacts: z.array(CareerArtifactKindSchema).max(3),
  })
  .strict()
  .superRefine((job, context) => {
    if (job.closesExclusiveGameDay <= job.postedGameDay) {
      context.addIssue({
        code: 'custom',
        path: ['closesExclusiveGameDay'],
        message: 'posting close day must be after posting day',
      });
    }
    if (job.minimumAnnualSalaryKrw > job.maximumAnnualSalaryKrw) {
      context.addIssue({
        code: 'custom',
        path: ['maximumAnnualSalaryKrw'],
        message: 'posting salary range is inverted',
      });
    }
    if (
      job.minimumAnnualSalaryKrw % job.salaryStepKrw !== 0 ||
      job.maximumAnnualSalaryKrw % job.salaryStepKrw !== 0
    ) {
      context.addIssue({
        code: 'custom',
        path: ['salaryStepKrw'],
        message: 'posting salary bounds must align with the salary step',
      });
    }
    const kinds = new Set(job.requiredArtifacts);
    if (kinds.size !== job.requiredArtifacts.length) {
      context.addIssue({
        code: 'custom',
        path: ['requiredArtifacts'],
        message: 'artifact requirements must be unique by kind',
      });
    }
  });

export const CareerJobsResponseSchema = z
  .object({ items: z.array(CareerJobSchema).max(200), nextBefore: PostingKeySchema.nullable() })
  .strict()
  .superRefine((page, context) => refinePostingPage(page.items, page.nextBefore, context));

export const CareerApplicationSchema = z
  .object({
    id: ResourceIdSchema,
    postingKey: PostingKeySchema,
    platform: CareerPlatformSchema,
    industry: CareerIndustrySchema,
    employerName: z.string().min(1).max(120),
    jobFamilyKey: z.string().min(1).max(64),
    source: z.enum(['direct', 'invitation']),
    status: CareerApplicationStatusSchema,
    submittedGameDay: z.number().int().nonnegative(),
    visibleScores: CareerScoresSchema,
    possessedScores: CareerScoresSchema,
    documentScoreBp: z.number().int().min(0).max(10_000).nullable(),
    documentDecisionGameDay: z.number().int().nonnegative().nullable(),
    interviewGameDay: z.number().int().nonnegative().nullable(),
    confirmationDeadlineExclusiveGameDay: z.number().int().nonnegative().nullable(),
    interviewScoreBp: z.number().int().min(0).max(10_000).nullable(),
    offer: z
      .object({
        id: ResourceIdSchema,
        status: z.enum(['offered', 'accepted', 'declined', 'expired', 'closed']),
        annualSalaryKrw: z.number().int().safe().positive(),
        paydayDayOfMonth: z.number().int().min(1).max(31),
        startGameDay: z.number().int().nonnegative(),
        expiresExclusiveGameDay: z.number().int().nonnegative(),
        wantedRewardKrw: z.number().int().safe().nonnegative(),
      })
      .strict()
      .nullable(),
  })
  .strict();

export const CareerInvitationSchema = z
  .object({
    id: ResourceIdSchema,
    postingKey: PostingKeySchema,
    platform: CareerPlatformSchema,
    industry: CareerIndustrySchema,
    employerName: z.string().min(1).max(120),
    jobFamilyKey: z.string().min(1).max(64),
    artifactVersionId: ResourceIdSchema,
    createdGameDay: z.number().int().nonnegative(),
    expiresExclusiveGameDay: z.number().int().nonnegative(),
  })
  .strict();

export const CareerApplicationsResponseSchema = z
  .object({
    items: z.array(CareerApplicationSchema).max(200),
    nextBefore: ResourceIdSchema.nullable(),
    openInvitations: z.array(CareerInvitationSchema).max(5),
  })
  .strict()
  .superRefine((page, context) => refineCareerPage(page.items, page.nextBefore, context));

export const CareerEmploymentContractSchema = z
  .object({
    id: ResourceIdSchema,
    status: EmploymentStatusSchema,
    jobFamilyKey: z.string().min(1).max(64),
    employerName: z.string().min(1).max(120),
    region: z.string().min(1).max(64),
    annualSalaryKrw: z.number().int().safe().positive(),
    paydayDayOfMonth: z.number().int().min(1).max(31),
    startGameDay: z.number().int().nonnegative(),
    endGameDay: z.number().int().nonnegative().nullable(),
    creditedExperienceDays: z.number().int().safe().nonnegative(),
  })
  .strict();

export const CareerEmploymentResponseSchema = z
  .object({ contract: CareerEmploymentContractSchema.nullable() })
  .strict();

// -- Employment payroll (M3-C) ----------------------------------------

export const CareerPayrollRewardSchema = z
  .object({
    paymentId: ResourceIdSchema,
    grossRewardKrw: NonnegativeKrwSchema,
    withheldIncomeTaxKrw: NonnegativeKrwSchema,
    withheldLocalIncomeTaxKrw: NonnegativeKrwSchema,
    netRewardKrw: NonnegativeKrwSchema,
  })
  .strict()
  .superRefine((reward, context) => {
    if (
      reward.netRewardKrw !==
      reward.grossRewardKrw - reward.withheldIncomeTaxKrw - reward.withheldLocalIncomeTaxKrw
    ) {
      context.addIssue({
        code: 'custom',
        path: ['netRewardKrw'],
        message: 'reward net amount must equal gross less withholding',
      });
    }
  });

export const CareerPayrollItemSchema = z
  .object({
    id: ResourceIdSchema,
    contractId: ResourceIdSchema,
    periodNo: z.number().int().safe().positive(),
    salaryMonthOrdinal: z.number().int().min(1).max(12),
    periodStartDate: z.iso.date(),
    periodEndExclusiveDate: z.iso.date(),
    paidGameDay: z.number().int().nonnegative(),
    grossPayKrw: NonnegativeKrwSchema,
    employeeNationalPensionKrw: NonnegativeKrwSchema,
    employerNationalPensionKrw: NonnegativeKrwSchema,
    employeeHealthInsuranceKrw: NonnegativeKrwSchema,
    employerHealthInsuranceKrw: NonnegativeKrwSchema,
    employeeLongTermCareKrw: NonnegativeKrwSchema,
    employerLongTermCareKrw: NonnegativeKrwSchema,
    employeeEmploymentInsuranceKrw: NonnegativeKrwSchema,
    employerEmploymentInsuranceKrw: NonnegativeKrwSchema,
    employerIndustrialAccidentKrw: NonnegativeKrwSchema,
    withheldIncomeTaxKrw: NonnegativeKrwSchema,
    withheldLocalIncomeTaxKrw: NonnegativeKrwSchema,
    netPayKrw: NonnegativeKrwSchema,
    reward: CareerPayrollRewardSchema.optional(),
  })
  .strict()
  .superRefine((payroll, context) => {
    if (payroll.periodStartDate >= payroll.periodEndExclusiveDate) {
      context.addIssue({
        code: 'custom',
        path: ['periodEndExclusiveDate'],
        message: 'payroll period end must follow its start',
      });
    }
    const expectedNetPay =
      payroll.grossPayKrw -
      payroll.employeeNationalPensionKrw -
      payroll.employeeHealthInsuranceKrw -
      payroll.employeeLongTermCareKrw -
      payroll.employeeEmploymentInsuranceKrw -
      payroll.withheldIncomeTaxKrw -
      payroll.withheldLocalIncomeTaxKrw;
    if (payroll.netPayKrw !== expectedNetPay) {
      context.addIssue({
        code: 'custom',
        path: ['netPayKrw'],
        message: 'payroll net amount must equal gross less employee deductions',
      });
    }
  });

export const CareerPayrollResponseSchema = z
  .object({
    items: z.array(CareerPayrollItemSchema).max(200),
    nextBefore: ResourceIdSchema.nullable(),
  })
  .strict()
  .superRefine((page, context) => refineCareerPage(page.items, page.nextBefore, context));

// -- Employment annual tax (M3-C) ------------------------------------

export const CareerTaxYearStatusSchema = z.enum(['open', 'provisional', 'definitive']);
export const CareerTaxYearSourceSchema = z.enum(['employmentOnly', 'combined', 'legacyProfile']);

const CareerTaxYearBaseShape = {
  taxYear: TaxYearSchema,
  grossEmploymentIncomeKrw: NonnegativeKrwSchema,
};

const M3CareerTaxYearAccrualShape = {
  employeeInsuranceDeductionKrw: NonnegativeKrwSchema,
  withheldIncomeTaxKrw: NonnegativeKrwSchema,
  withheldLocalIncomeTaxKrw: NonnegativeKrwSchema,
};

const NullCareerTaxCalculationShape = {
  earnedIncomeDeductionKrw: z.null(),
  personalDeductionKrw: z.null(),
  taxableIncomeKrw: z.null(),
  calculatedIncomeTaxKrw: z.null(),
  earnedIncomeTaxCreditKrw: z.null(),
  pensionCreditEligibleContributionKrw: z.null(),
  actualPensionIncomeTaxCreditKrw: z.null(),
  actualPensionLocalIncomeTaxEffectKrw: z.null(),
  assessedIncomeTaxKrw: z.null(),
  assessedLocalIncomeTaxKrw: z.null(),
  additionalTaxKrw: z.null(),
  refundKrw: z.null(),
  reconciliationGameDay: z.null(),
};

const FinalizedCareerTaxCalculationShape = {
  earnedIncomeDeductionKrw: NonnegativeKrwSchema,
  personalDeductionKrw: NonnegativeKrwSchema,
  taxableIncomeKrw: NonnegativeKrwSchema,
  calculatedIncomeTaxKrw: NonnegativeKrwSchema,
  earnedIncomeTaxCreditKrw: NonnegativeKrwSchema,
  pensionCreditEligibleContributionKrw: NonnegativeKrwSchema,
  actualPensionIncomeTaxCreditKrw: NonnegativeKrwSchema,
  actualPensionLocalIncomeTaxEffectKrw: NonnegativeKrwSchema,
  assessedIncomeTaxKrw: NonnegativeKrwSchema,
  assessedLocalIncomeTaxKrw: NonnegativeKrwSchema,
  additionalTaxKrw: NonnegativeKrwSchema,
  refundKrw: NonnegativeKrwSchema,
  reconciliationGameDay: z.number().int().safe().nonnegative(),
};

const OpenCareerTaxYearStateSchema = z
  .object({
    ...CareerTaxYearBaseShape,
    ...M3CareerTaxYearAccrualShape,
    status: z.literal('open'),
    source: z.literal('employmentOnly'),
    ...NullCareerTaxCalculationShape,
  })
  .strict();

const ProvisionalCareerTaxYearStateSchema = z
  .object({
    ...CareerTaxYearBaseShape,
    ...M3CareerTaxYearAccrualShape,
    status: z.literal('provisional'),
    source: z.literal('employmentOnly'),
    ...FinalizedCareerTaxCalculationShape,
  })
  .strict();

const M3DefinitiveCareerTaxYearStateSchema = z
  .object({
    ...CareerTaxYearBaseShape,
    ...M3CareerTaxYearAccrualShape,
    status: z.literal('definitive'),
    source: z.enum(['employmentOnly', 'combined']),
    ...FinalizedCareerTaxCalculationShape,
  })
  .strict();

const LegacyDefinitiveCareerTaxYearStateSchema = z
  .object({
    ...CareerTaxYearBaseShape,
    status: z.literal('definitive'),
    source: z.literal('legacyProfile'),
    employeeInsuranceDeductionKrw: z.null(),
    earnedIncomeDeductionKrw: z.null(),
    personalDeductionKrw: z.null(),
    taxableIncomeKrw: NonnegativeKrwSchema,
    calculatedIncomeTaxKrw: z.null(),
    earnedIncomeTaxCreditKrw: z.null(),
    pensionCreditEligibleContributionKrw: z.null(),
    actualPensionIncomeTaxCreditKrw: z.null(),
    actualPensionLocalIncomeTaxEffectKrw: z.null(),
    withheldIncomeTaxKrw: z.null(),
    withheldLocalIncomeTaxKrw: z.null(),
    assessedIncomeTaxKrw: z.null(),
    assessedLocalIncomeTaxKrw: z.null(),
    additionalTaxKrw: z.null(),
    refundKrw: z.null(),
    reconciliationGameDay: z.null(),
  })
  .strict();

const DefinitiveCareerTaxYearStateSchema = z.union([
  M3DefinitiveCareerTaxYearStateSchema,
  LegacyDefinitiveCareerTaxYearStateSchema,
]);

export const CareerTaxYearStateSchema = z
  .union([
    OpenCareerTaxYearStateSchema,
    ProvisionalCareerTaxYearStateSchema,
    M3DefinitiveCareerTaxYearStateSchema,
    LegacyDefinitiveCareerTaxYearStateSchema,
  ])
  .superRefine((year, context) => {
    if (year.status === 'open' || year.source === 'legacyProfile') return;

    if (year.additionalTaxKrw > 0 && year.refundKrw > 0) {
      context.addIssue({
        code: 'custom',
        path: ['additionalTaxKrw'],
        message: 'additional tax and refund are mutually exclusive',
      });
    }

    const assessedTaxKrw =
      BigInt(year.assessedIncomeTaxKrw) + BigInt(year.assessedLocalIncomeTaxKrw);
    const prepaidTaxKrw =
      BigInt(year.withheldIncomeTaxKrw) + BigInt(year.withheldLocalIncomeTaxKrw);
    if (assessedTaxKrw - prepaidTaxKrw !== BigInt(year.additionalTaxKrw) - BigInt(year.refundKrw)) {
      context.addIssue({
        code: 'custom',
        path: ['additionalTaxKrw'],
        message: 'assessed tax, withholding, additional tax, and refund must reconcile',
      });
    }
  });

const CareerApplicationDraftFields = {
  postingKey: PostingKeySchema,
  resumeVersionId: ResourceIdSchema.optional(),
  portfolioVersionId: ResourceIdSchema.optional(),
  linkedinProfileVersionId: ResourceIdSchema.optional(),
} as const;

export const CareerApplicationDraftSchema = z
  .object(CareerApplicationDraftFields)
  .strict()
  .superRefine((request, context) => {
    const provided = [
      request.resumeVersionId,
      request.portfolioVersionId,
      request.linkedinProfileVersionId,
    ].filter((value): value is string => value !== undefined);
    if (provided.length === 0) {
      context.addIssue({
        code: 'custom',
        path: ['resumeVersionId'],
        message: 'an application needs an artifact version',
      });
    }
    if (new Set(provided).size !== provided.length) {
      context.addIssue({
        code: 'custom',
        path: ['resumeVersionId'],
        message: 'artifact versions must be distinct',
      });
    }
  });
export const CareerApplicationRequestSchema = z
  .object({ ...CareerCommandCursorFields, ...CareerApplicationDraftFields })
  .strict()
  .superRefine((request, context) => {
    const provided = [
      request.resumeVersionId,
      request.portfolioVersionId,
      request.linkedinProfileVersionId,
    ].filter((value): value is string => value !== undefined);
    if (provided.length === 0) {
      context.addIssue({
        code: 'custom',
        path: ['resumeVersionId'],
        message: 'an application needs an artifact version',
      });
    }
    if (new Set(provided).size !== provided.length) {
      context.addIssue({
        code: 'custom',
        path: ['resumeVersionId'],
        message: 'artifact versions must be distinct',
      });
    }
  });
export const CareerInterviewDecisionSchema = z.enum(['confirm', 'decline']);
export const CareerInterviewConfirmationRequestSchema = z
  .object({ ...CareerCommandCursorFields, decision: CareerInterviewDecisionSchema })
  .strict();

export const CareerApplicationResultSchema = z
  .object({ applicationId: ResourceIdSchema, status: CareerApplicationStatusSchema })
  .strict();
export const CareerInvitationResultSchema = z
  .object({
    invitationId: ResourceIdSchema,
    status: CareerInvitationStatusSchema,
    applicationId: ResourceIdSchema.nullable(),
  })
  .strict();
export const CareerOfferResultSchema = z
  .object({
    offerId: ResourceIdSchema,
    status: CareerApplicationStatusSchema,
    employmentContractId: ResourceIdSchema.nullable(),
  })
  .strict();
export const CareerApplicationResponseSchema = z
  .object({
    result: CareerApplicationResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict();
export const CareerInvitationResponseSchema = z
  .object({
    result: CareerInvitationResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict();
export const CareerOfferResponseSchema = z
  .object({ result: CareerOfferResultSchema, replayed: z.boolean(), snapshot: GameSnapshotSchema })
  .strict();

// -- Military service and military savings (M3-D) ----------------------

export const CareerMilitaryStatusSchema = z.enum(['unserved', 'serving', 'completed', 'exempt']);
export const MilitaryServiceTypeSchema = z.enum([
  'activeDuty',
  'socialService',
  'industrialTechnical',
  'professionalResearch',
  'commissionedOfficer',
  'nonCommissionedOfficer',
]);
export const MilitaryServiceStatusSchema = z.enum(['pendingStart', 'serving', 'completed']);
export const MilitaryServiceSourceKindSchema = z.enum(['userCommand', 'legacyBridge']);
export const MilitaryCompensationKindSchema = z.enum(['militaryPay', 'employmentPayroll']);
export const MilitarySavingsContractStatusSchema = z.enum(['active', 'matured', 'closed']);
export const MilitarySavingsInstallmentStatusSchema = z.enum(['scheduled', 'paid', 'missed']);

export const MilitaryOptionIneligibilityReasonSchema = z.enum([
  'militarySubjectRequired',
  'militaryStateConflict',
  'minimumEducation',
  'minimumCertificationCount',
  'minimumExperienceDays',
  'policyUnavailable',
]);
export const MilitarySavingsIneligibilityReasonSchema = z.enum([
  'militaryStateConflict',
  'serviceTypeNotEligible',
  'minimumRemainingService',
  'activeContractLimit',
  'institutionLimit',
  'joinWindowClosed',
  'policyUnavailable',
]);

export const MilitaryHardRequirementsSchema = z
  .object({
    minimumEducation: EducationSchema.nullable(),
    requiredCertificationCount: z.number().int().safe().nonnegative(),
    minimumExperienceDays: z.number().int().safe().nonnegative(),
  })
  .strict();

export const MilitaryPayStageSchema = z
  .object({
    startServiceMonth: z.number().int().nonnegative().max(120),
    endExclusiveServiceMonth: z.number().int().positive().max(120),
    grossMonthlyPayKrw: PositiveKrwSchema,
  })
  .strict()
  .superRefine((stage, context) => {
    if (stage.endExclusiveServiceMonth <= stage.startServiceMonth) {
      context.addIssue({
        code: 'custom',
        path: ['endExclusiveServiceMonth'],
        message: 'military pay stage must have a positive month range',
      });
    }
  });

export const MilitaryExperienceCreditSchema = z
  .object({
    jobFamilyKey: z.string().min(1).max(64),
    dailyCreditPpm: z.number().int().min(1).max(1_000_000),
  })
  .strict();

export const MilitaryOptionSchema = z
  .object({
    id: ResourceIdSchema,
    optionKey: z.string().min(1).max(96),
    serviceType: MilitaryServiceTypeSchema,
    displayName: z.string().min(1).max(120),
    eligible: z.boolean(),
    ineligibilityReasons: z.array(MilitaryOptionIneligibilityReasonSchema).max(6),
    serviceDurationMonths: z.number().int().positive().max(120),
    hardRequirements: MilitaryHardRequirementsSchema,
    compensationKind: MilitaryCompensationKindSchema,
    paySchedule: z.literal('monthly'),
    payStages: z.array(MilitaryPayStageSchema).min(1).max(12),
    effortLifeStatus: LifeStatusSchema,
    dailyEffortCapacityUnits: z.number().int().safe().nonnegative(),
    grantsCareerExperience: z.boolean(),
    experienceCredits: z.array(MilitaryExperienceCreditSchema).max(8),
  })
  .strict()
  .superRefine((option, context) => {
    if (option.eligible !== (option.ineligibilityReasons.length === 0)) {
      context.addIssue({
        code: 'custom',
        path: ['eligible'],
        message: 'military option eligibility must match its reasons',
      });
    }
    if (new Set(option.ineligibilityReasons).size !== option.ineligibilityReasons.length) {
      context.addIssue({
        code: 'custom',
        path: ['ineligibilityReasons'],
        message: 'military option ineligibility reasons must be unique',
      });
    }
    if (option.grantsCareerExperience !== option.experienceCredits.length > 0) {
      context.addIssue({
        code: 'custom',
        path: ['experienceCredits'],
        message: 'military career flag must match its experience credits',
      });
    }
    if (
      new Set(option.experienceCredits.map((credit) => credit.jobFamilyKey)).size !==
      option.experienceCredits.length
    ) {
      context.addIssue({
        code: 'custom',
        path: ['experienceCredits'],
        message: 'military experience credits must be unique by job family',
      });
    }

    let expectedStart = 0;
    for (const [index, stage] of option.payStages.entries()) {
      if (stage.startServiceMonth !== expectedStart) {
        context.addIssue({
          code: 'custom',
          path: ['payStages', index, 'startServiceMonth'],
          message: 'military pay stages must be contiguous from month zero',
        });
      }
      expectedStart = stage.endExclusiveServiceMonth;
    }
    if (expectedStart !== option.serviceDurationMonths) {
      context.addIssue({
        code: 'custom',
        path: ['payStages'],
        message: 'military pay stages must cover the complete service term',
      });
    }
  });

export const MilitaryOptionsResponseSchema = z
  .object({ items: z.array(MilitaryOptionSchema).max(6) })
  .strict()
  .superRefine((response, context) => {
    if (new Set(response.items.map((option) => option.id)).size !== response.items.length) {
      context.addIssue({
        code: 'custom',
        path: ['items'],
        message: 'military option IDs must be unique',
      });
    }
    if (
      new Set(response.items.map((option) => option.serviceType)).size !== response.items.length
    ) {
      context.addIssue({
        code: 'custom',
        path: ['items'],
        message: 'military options must be unique by service type',
      });
    }
  });

const MilitaryServiceSummaryShape = {
  id: ResourceIdSchema,
  optionVersionId: ResourceIdSchema,
  serviceType: MilitaryServiceTypeSchema,
  displayName: z.string().min(1).max(120),
  startGameDay: z.number().int().safe().nonnegative(),
  endGameDay: z.number().int().safe().positive(),
  creditedServiceDays: z.number().int().safe().nonnegative(),
  totalServiceDays: z.number().int().safe().positive(),
  effortLifeStatus: LifeStatusSchema,
  grantsCareerExperience: z.boolean(),
  nextPayGameDay: z.number().int().safe().nonnegative().nullable(),
} as const;

export const ActiveMilitaryServiceSummarySchema = z
  .object({
    ...MilitaryServiceSummaryShape,
    status: z.enum(['pendingStart', 'serving']),
  })
  .strict()
  .superRefine((service, context) => refineMilitaryServiceSummary(service, context));

export const MilitaryServiceHistorySchema = z
  .object({
    ...MilitaryServiceSummaryShape,
    status: MilitaryServiceStatusSchema,
    sourceKind: MilitaryServiceSourceKindSchema,
    startDate: z.iso.date(),
    endExclusiveDate: z.iso.date(),
    completedGameDay: z.number().int().safe().nonnegative().nullable(),
  })
  .strict()
  .superRefine((service, context) => {
    refineMilitaryServiceSummary(service, context);
    if (service.startDate >= service.endExclusiveDate) {
      context.addIssue({
        code: 'custom',
        path: ['endExclusiveDate'],
        message: 'military service end date must follow its start date',
      });
    }
    if ((service.status === 'completed') !== (service.completedGameDay !== null)) {
      context.addIssue({
        code: 'custom',
        path: ['completedGameDay'],
        message: 'completed military service requires its completion day',
      });
    }
  });

export const MilitaryServiceResponseSchema = z
  .object({
    militaryStatus: CareerMilitaryStatusSchema,
    service: MilitaryServiceHistorySchema.nullable(),
  })
  .strict()
  .superRefine((response, context) => {
    const serviceStatus = response.service?.status;
    const matches =
      (response.militaryStatus === 'unserved' && response.service === null) ||
      (response.militaryStatus === 'exempt' && response.service === null) ||
      (response.militaryStatus === 'serving' &&
        (serviceStatus === 'pendingStart' || serviceStatus === 'serving')) ||
      (response.militaryStatus === 'completed' &&
        (response.service === null || serviceStatus === 'completed'));
    if (!matches) {
      context.addIssue({
        code: 'custom',
        path: ['service'],
        message: 'military status and service history do not match',
      });
    }
  });

export const MilitarySavingsInterestTierSchema = z
  .object({
    minimumTermMonths: z.number().int().positive().max(120),
    maximumTermMonthsInclusive: z.number().int().positive().max(120),
    annualInterestRatePpm: z.number().int().nonnegative().max(1_000_000),
  })
  .strict()
  .superRefine((tier, context) => {
    if (tier.minimumTermMonths > tier.maximumTermMonthsInclusive) {
      context.addIssue({
        code: 'custom',
        path: ['maximumTermMonthsInclusive'],
        message: 'military savings interest tier is inverted',
      });
    }
  });

export const MilitarySavingsProductSchema = z
  .object({
    id: ResourceIdSchema,
    productKey: z.string().min(1).max(96),
    institutionKey: z.string().min(1).max(64),
    institutionDisplayName: z.string().min(1).max(100),
    eligible: z.boolean(),
    ineligibilityReasons: z.array(MilitarySavingsIneligibilityReasonSchema).max(7),
    eligibleServiceTypes: z.array(MilitaryServiceTypeSchema).min(1).max(6),
    joinStartDate: z.iso.date(),
    joinEndDate: z.iso.date(),
    minimumRemainingServiceMonths: z.number().int().positive().max(600),
    maximumActiveContracts: z.number().int().positive().max(2),
    maximumContractsPerInstitution: z.number().int().positive().max(2),
    minimumMonthlyContributionKrw: PositiveKrwSchema,
    maximumInstitutionMonthlyContributionKrw: PositiveKrwSchema,
    maximumTotalMonthlyContributionKrw: PositiveKrwSchema,
    limitSettingUnitKrw: PositiveKrwSchema,
    installmentUnitKrw: PositiveKrwSchema,
    interestTiers: z.array(MilitarySavingsInterestTierSchema).min(1).max(12),
    dayCountConvention: z.literal('actual365'),
    interestRounding: z.literal('floorToKrw'),
    earlyCloseAnnualInterestRatePpm: z.number().int().nonnegative().max(1_000_000),
    governmentMatchingRatePpm: z.number().int().nonnegative().max(1_000_000),
    governmentMatchPaymentDayOfMonth: z.number().int().min(1).max(31),
    maturityTaxExempt: z.boolean(),
  })
  .strict()
  .superRefine((product, context) => {
    if (product.eligible !== (product.ineligibilityReasons.length === 0)) {
      context.addIssue({
        code: 'custom',
        path: ['eligible'],
        message: 'military savings eligibility must match its reasons',
      });
    }
    if (new Set(product.ineligibilityReasons).size !== product.ineligibilityReasons.length) {
      context.addIssue({
        code: 'custom',
        path: ['ineligibilityReasons'],
        message: 'military savings ineligibility reasons must be unique',
      });
    }
    if (new Set(product.eligibleServiceTypes).size !== product.eligibleServiceTypes.length) {
      context.addIssue({
        code: 'custom',
        path: ['eligibleServiceTypes'],
        message: 'eligible military service types must be unique',
      });
    }
    if (
      product.joinStartDate > product.joinEndDate ||
      product.minimumMonthlyContributionKrw > product.maximumInstitutionMonthlyContributionKrw ||
      product.maximumInstitutionMonthlyContributionKrw > product.maximumTotalMonthlyContributionKrw
    ) {
      context.addIssue({
        code: 'custom',
        path: ['maximumInstitutionMonthlyContributionKrw'],
        message: 'military savings dates or contribution limits are inverted',
      });
    }

    let expectedMinimumTerm = 1;
    for (const [index, tier] of product.interestTiers.entries()) {
      if (tier.minimumTermMonths !== expectedMinimumTerm) {
        context.addIssue({
          code: 'custom',
          path: ['interestTiers', index, 'minimumTermMonths'],
          message: 'military savings interest tiers must be contiguous from month one',
        });
      }
      expectedMinimumTerm = tier.maximumTermMonthsInclusive + 1;
    }
  });

export const MilitarySavingsProductsResponseSchema = z
  .object({ items: z.array(MilitarySavingsProductSchema).max(20) })
  .strict();

export const ActiveMilitarySavingsSummarySchema = z
  .object({
    id: ResourceIdSchema,
    productVersionId: ResourceIdSchema,
    institutionKey: z.string().min(1).max(64),
    status: z.literal('active'),
    monthlyContributionKrw: PositiveKrwSchema,
    debitDayOfMonth: z.number().int().min(1).max(31),
    principalKrw: NonnegativeKrwSchema,
    paidInstallmentCount: z.number().int().safe().nonnegative(),
    missedInstallmentCount: z.number().int().safe().nonnegative(),
    nextInstallmentGameDay: z.number().int().safe().nonnegative().nullable(),
    maturityGameDay: z.number().int().safe().positive(),
  })
  .strict()
  .superRefine((contract, context) => {
    if (
      contract.nextInstallmentGameDay !== null &&
      contract.nextInstallmentGameDay >= contract.maturityGameDay
    ) {
      context.addIssue({
        code: 'custom',
        path: ['nextInstallmentGameDay'],
        message: 'next military savings installment must precede maturity',
      });
    }
  });

export const MilitarySavingsInstallmentSchema = z
  .object({
    id: ResourceIdSchema,
    installmentNo: z.number().int().positive().max(120),
    dueGameDay: z.number().int().safe().nonnegative(),
    status: MilitarySavingsInstallmentStatusSchema,
    paidGameDay: z.number().int().safe().nonnegative().nullable(),
    principalKrw: NonnegativeKrwSchema,
    governmentMatchingPolicyVersionId: ResourceIdSchema.nullable(),
    governmentMatchingRatePpm: z.number().int().nonnegative().max(1_000_000).nullable(),
  })
  .strict()
  .superRefine((installment, context) => {
    const paid = installment.status === 'paid';
    const settled = installment.status !== 'scheduled';
    if (
      settled !== (installment.paidGameDay !== null) ||
      (installment.paidGameDay !== null && installment.paidGameDay !== installment.dueGameDay) ||
      paid !== installment.principalKrw > 0 ||
      paid !== (installment.governmentMatchingPolicyVersionId !== null) ||
      paid !== (installment.governmentMatchingRatePpm !== null)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['status'],
        message: 'military savings settlement fields must match its status',
      });
    }
  });

export const MilitarySavingsMaturityProjectionSchema = z
  .object({
    assumption: z.literal('allScheduledInstallmentsPaid'),
    principalKrw: NonnegativeKrwSchema,
    grossBankInterestKrw: NonnegativeKrwSchema,
    governmentMatchKrw: NonnegativeKrwSchema,
    bankPayoutKrw: NonnegativeKrwSchema,
    totalBenefitKrw: NonnegativeKrwSchema,
  })
  .strict()
  .superRefine((projection, context) => {
    if (
      BigInt(projection.bankPayoutKrw) !==
        BigInt(projection.principalKrw) + BigInt(projection.grossBankInterestKrw) ||
      BigInt(projection.totalBenefitKrw) !==
        BigInt(projection.bankPayoutKrw) + BigInt(projection.governmentMatchKrw)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['totalBenefitKrw'],
        message: 'military savings projection must reconcile',
      });
    }
  });

export const MilitarySavingsHistoryItemSchema = z
  .object({
    id: ResourceIdSchema,
    serviceId: ResourceIdSchema,
    productVersionId: ResourceIdSchema,
    productKey: z.string().min(1).max(96),
    institutionKey: z.string().min(1).max(64),
    institutionDisplayName: z.string().min(1).max(100),
    status: MilitarySavingsContractStatusSchema,
    monthlyContributionKrw: PositiveKrwSchema,
    debitDayOfMonth: z.number().int().min(1).max(31),
    principalKrw: NonnegativeKrwSchema,
    paidInstallmentCount: z.number().int().safe().nonnegative(),
    missedInstallmentCount: z.number().int().safe().nonnegative(),
    nextInstallmentGameDay: z.number().int().safe().nonnegative().nullable(),
    maturityGameDay: z.number().int().safe().positive(),
    openedGameDay: z.number().int().safe().nonnegative(),
    firstInstallmentGameDay: z.number().int().safe().nonnegative(),
    contractTermMonths: z.number().int().positive().max(120),
    annualInterestRatePpm: z.number().int().nonnegative().max(1_000_000),
    closedGameDay: z.number().int().safe().nonnegative().nullable(),
    closureReason: z.enum(['maturity', 'earlyClose']).nullable(),
    settledPrincipalKrw: NonnegativeKrwSchema,
    grossBankInterestKrw: NonnegativeKrwSchema,
    governmentMatchKrw: NonnegativeKrwSchema,
    bankPayoutKrw: NonnegativeKrwSchema,
    governmentMatchPaidGameDay: z.number().int().safe().nonnegative().nullable(),
    projectedMaturity: MilitarySavingsMaturityProjectionSchema.nullable(),
    installments: z.array(MilitarySavingsInstallmentSchema).max(120),
  })
  .strict()
  .superRefine((contract, context) => {
    if (
      contract.openedGameDay >= contract.firstInstallmentGameDay ||
      contract.firstInstallmentGameDay >= contract.maturityGameDay ||
      (contract.nextInstallmentGameDay !== null &&
        contract.nextInstallmentGameDay >= contract.maturityGameDay)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['firstInstallmentGameDay'],
        message: 'military savings contract days are out of order',
      });
    }
  })
  .superRefine((contract, context) => {
    const active = contract.status === 'active';
    const matured = contract.status === 'matured';
    if (
      active !== (contract.projectedMaturity !== null) ||
      active === (contract.closedGameDay !== null) ||
      active === (contract.closureReason !== null) ||
      (!active && contract.nextInstallmentGameDay !== null) ||
      (matured && contract.closureReason !== 'maturity') ||
      (contract.status === 'closed' && contract.closureReason !== 'earlyClose')
    ) {
      context.addIssue({
        code: 'custom',
        path: ['status'],
        message: 'military savings lifecycle fields do not match its status',
      });
    }
  })
  .superRefine((contract, context) => {
    if (
      BigInt(contract.bankPayoutKrw) !==
      BigInt(contract.settledPrincipalKrw) + BigInt(contract.grossBankInterestKrw)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['bankPayoutKrw'],
        message: 'military savings bank payout must reconcile',
      });
    }
    if (
      contract.status === 'closed' &&
      (contract.governmentMatchKrw !== 0 || contract.governmentMatchPaidGameDay !== null)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['governmentMatchKrw'],
        message: 'early-closed military savings cannot retain government matching',
      });
    }
  })
  .superRefine((contract, context) => {
    for (let index = 0; index < contract.installments.length; index += 1) {
      const installment = contract.installments[index];
      if (installment !== undefined && installment.installmentNo !== index + 1) {
        context.addIssue({
          code: 'custom',
          path: ['installments', index, 'installmentNo'],
          message: 'military savings installments must be ordered without gaps',
        });
      }
    }
    const paidCount = contract.installments.filter((item) => item.status === 'paid').length;
    const missedCount = contract.installments.filter((item) => item.status === 'missed').length;
    if (
      paidCount !== contract.paidInstallmentCount ||
      missedCount !== contract.missedInstallmentCount
    ) {
      context.addIssue({
        code: 'custom',
        path: ['paidInstallmentCount'],
        message: 'military savings installment counts must match history',
      });
    }
  });

export const MilitarySavingsHistoryResponseSchema = z
  .object({
    items: z.array(MilitarySavingsHistoryItemSchema).max(200),
    nextBefore: ResourceIdSchema.nullable(),
  })
  .strict()
  .superRefine((page, context) => refineCareerPage(page.items, page.nextBefore, context));

export const MilitaryServiceStartDraftSchema = z
  .object({ militaryOptionVersionId: ResourceIdSchema })
  .strict();
export const MilitaryServiceStartRequestSchema = z
  .object({ ...CareerCommandCursorFields, militaryOptionVersionId: ResourceIdSchema })
  .strict();
export const MilitarySavingsEnrollmentDraftSchema = z
  .object({
    productVersionId: ResourceIdSchema,
    monthlyContributionKrw: PositiveKrwSchema,
    debitDayOfMonth: z.number().int().min(1).max(31),
  })
  .strict();
export const MilitarySavingsEnrollmentRequestSchema = z
  .object({
    ...CareerCommandCursorFields,
    productVersionId: ResourceIdSchema,
    monthlyContributionKrw: PositiveKrwSchema,
    debitDayOfMonth: z.number().int().min(1).max(31),
  })
  .strict();

export const MilitaryServiceResultSchema = z
  .object({
    militaryServiceId: ResourceIdSchema,
    status: z.enum(['pendingStart', 'serving']),
  })
  .strict();
export const MilitarySavingsResultSchema = z
  .object({
    militarySavingsContractId: ResourceIdSchema,
    status: MilitarySavingsContractStatusSchema,
  })
  .strict();
export const MilitaryServiceCommandResponseSchema = z
  .object({
    result: MilitaryServiceResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict();
export const MilitarySavingsCommandResponseSchema = z
  .object({
    result: MilitarySavingsResultSchema,
    replayed: z.boolean(),
    snapshot: GameSnapshotSchema,
  })
  .strict();

function refineMilitaryServiceSummary(
  service: {
    readonly startGameDay: number;
    readonly endGameDay: number;
    readonly creditedServiceDays: number;
    readonly totalServiceDays: number;
    readonly nextPayGameDay: number | null;
  },
  context: z.RefinementCtx,
): void {
  if (
    service.endGameDay <= service.startGameDay ||
    service.totalServiceDays !== service.endGameDay - service.startGameDay ||
    service.creditedServiceDays > service.totalServiceDays
  ) {
    context.addIssue({
      code: 'custom',
      path: ['totalServiceDays'],
      message: 'military service progress must match its term',
    });
  }
  if (
    service.nextPayGameDay !== null &&
    (service.nextPayGameDay < service.startGameDay || service.nextPayGameDay >= service.endGameDay)
  ) {
    context.addIssue({
      code: 'custom',
      path: ['nextPayGameDay'],
      message: 'next military pay day must fall inside the service term',
    });
  }
}

function comparePendingCareerSchedule(
  left: z.infer<typeof CareerPendingScheduleItemSchema>,
  right: z.infer<typeof CareerPendingScheduleItemSchema>,
): number {
  const leftKey = pendingCareerScheduleSortKey(left);
  const rightKey = pendingCareerScheduleSortKey(right);
  return (
    compareNumber(leftKey.dueGameDay, rightKey.dueGameDay) ||
    compareNumber(leftKey.sourceRank, rightKey.sourceRank) ||
    compareNumber(leftKey.phaseRank, rightKey.phaseRank) ||
    compareBigInt(leftKey.id, rightKey.id)
  );
}

function pendingCareerScheduleSortKey(item: z.infer<typeof CareerPendingScheduleItemSchema>) {
  return {
    dueGameDay: item.dueGameDay,
    sourceRank: item.sourceKind === 'careerAction' ? 0 : 1,
    phaseRank: item.sourceKind === 'careerAction' ? careerActionPhaseRank(item.kind) : 0,
    id: BigInt(item.id),
  };
}

function careerActionPhaseRank(kind: z.infer<typeof CareerScheduledActionKindSchema>): number {
  switch (kind) {
    case 'employmentStart':
    case 'militaryServiceStart':
    case 'militaryServiceCompletion':
      return 10;
    case 'documentReview':
      return 20;
    case 'confirmationExpiry':
      return 30;
    case 'interviewDecision':
      return 40;
    case 'offerExpiry':
      return 50;
    case 'invitationGeneration':
      return 60;
  }
}

function compareNumber(left: number, right: number): number {
  return left === right ? 0 : left < right ? -1 : 1;
}

function compareBigInt(left: bigint, right: bigint): number {
  return left === right ? 0 : left < right ? -1 : 1;
}

function refinePendingCareerSchedule(
  items: readonly z.infer<typeof CareerPendingScheduleItemSchema>[],
  context: z.RefinementCtx,
): void {
  for (let index = 1; index < items.length; index += 1) {
    const previous = items[index - 1];
    const current = items[index];
    if (
      previous !== undefined &&
      current !== undefined &&
      comparePendingCareerSchedule(previous, current) > 0
    ) {
      context.addIssue({
        code: 'custom',
        path: ['pendingCareerSchedule', index],
        message: 'pending career schedule must follow execution order',
      });
    }
  }
}

function refinePostingPage(
  items: readonly { readonly postingKey: string }[],
  nextBefore: string | null,
  context: z.RefinementCtx,
): void {
  for (let index = 1; index < items.length; index += 1) {
    const previous = items[index - 1];
    const current = items[index];
    if (
      previous !== undefined &&
      current !== undefined &&
      previous.postingKey <= current.postingKey
    ) {
      context.addIssue({
        code: 'custom',
        path: ['items', index, 'postingKey'],
        message: 'job page must be ordered by descending posting key',
      });
    }
  }
  const oldest = items.at(-1)?.postingKey ?? null;
  if (nextBefore !== null && nextBefore !== oldest) {
    context.addIssue({
      code: 'custom',
      path: ['nextBefore'],
      message: 'job cursor must equal the oldest returned posting key',
    });
  }
}

export const ClockRequestSchema = z.object({
  speed: GameSpeedSchema.nullable(),
});

// -- Portfolio orders and market history (M1-B) -------------------------

export const PortfolioOrderSideSchema = z.enum(['buy', 'sell']);

export const PortfolioOrderDraftSchema = z.object({
  accountId: ResourceIdSchema,
  side: PortfolioOrderSideSchema,
  quantity: z
    .number()
    .int('수량은 정수여야 합니다')
    .min(1, '수량은 1주 이상이어야 합니다')
    .max(1_000_000, '한 주문은 1,000,000주를 넘을 수 없습니다'),
});

export const PortfolioOrderRequestSchema = PortfolioOrderDraftSchema.extend({
  orderId: CanonicalUuidSchema,
  expectedRunRevision: z.number().int().nonnegative(),
  expectedStateRevision: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
  expectedGameDay: z.number().int().nonnegative(),
  symbol: z.literal('LLX'),
});

export const PortfolioExecutionSchema = z
  .object({
    orderId: CanonicalUuidSchema,
    accountId: ResourceIdSchema,
    symbol: z.literal('LLX'),
    side: PortfolioOrderSideSchema,
    quantity: z.number().int().min(1).max(1_000_000),
    priceKrw: PositiveKrwSchema,
    grossAmountKrw: NonnegativeKrwSchema,
    feeKrw: NonnegativeKrwSchema,
    taxKrw: NonnegativeKrwSchema,
    removedCostBasisKrw: NonnegativeKrwSchema,
    realizedGainLossKrw: z.number().int().safe(),
    replayed: z.boolean(),
  })
  .strict()
  .superRefine((execution, context) => {
    const grossAmountKrw = BigInt(execution.priceKrw) * BigInt(execution.quantity);
    if (grossAmountKrw !== BigInt(execution.grossAmountKrw)) {
      context.addIssue({
        code: 'custom',
        path: ['grossAmountKrw'],
        message: 'execution gross must equal price times quantity',
      });
    }

    const expectedRealizedGainLossKrw =
      execution.side === 'buy'
        ? 0n
        : BigInt(execution.grossAmountKrw) -
          BigInt(execution.removedCostBasisKrw) -
          BigInt(execution.feeKrw) -
          BigInt(execution.taxKrw);
    if (
      (execution.side === 'buy' && execution.removedCostBasisKrw !== 0) ||
      expectedRealizedGainLossKrw !== BigInt(execution.realizedGainLossKrw)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['realizedGainLossKrw'],
        message: 'execution cost basis and realized result do not reconcile',
      });
    }
  });

export const PortfolioOrderResponseSchema = z.object({
  execution: PortfolioExecutionSchema,
  snapshot: GameSnapshotSchema,
});

export const PortfolioOrderFailureCodeSchema = z.enum([
  'invalidOrder',
  'characterRequired',
  'accountNotFound',
  'accountClosed',
  'accountTypeNotAllowed',
  'marketClosed',
  'insufficientAccountCash',
  'insufficientQuantity',
  'positionLimit',
  'idempotencyConflict',
  'busy',
]);

export const PortfolioOrderFailureSchema = z.object({
  code: PortfolioOrderFailureCodeSchema,
  message: z.string().min(1),
});

export const MarketHistoryDaysSchema = z.number().int().min(1).max(3660);

export const MarketHistoryPointSchema = z
  .object({
    gameDay: z.number().int().nonnegative(),
    date: z.iso.date(),
    open: z.boolean(),
    closeKrw: z.number().int().positive(),
    dailyReturnPpm: z.number().int(),
    llxCloseKrw: PositiveKrwSchema.nullable(),
    llxDailyReturnPpm: z.number().int().safe().nullable(),
    regime: MarketRegimeSchema,
    rates: MarketRatesSchema.nullable(),
  })
  .strict()
  .superRefine((point, context) => {
    if ((point.llxCloseKrw === null) !== (point.llxDailyReturnPpm === null)) {
      context.addIssue({
        code: 'custom',
        path: ['llxCloseKrw'],
        message: 'LLX history fields must be both null or both populated',
      });
    }
  });

export const MarketHistorySchema = z
  .object({
    world: z.string(),
    symbol: z.literal('LLX'),
    throughGameDay: z.number().int().nonnegative(),
    points: z.array(MarketHistoryPointSchema).max(3660),
  })
  .superRefine((history, context) => {
    let previousGameDay = -1;
    for (const [index, point] of history.points.entries()) {
      if (point.gameDay <= previousGameDay) {
        context.addIssue({
          code: 'custom',
          path: ['points', index, 'gameDay'],
          message: 'history points must be in ascending game-day order',
        });
      }
      if (point.gameDay > history.throughGameDay) {
        context.addIssue({
          code: 'custom',
          path: ['points', index, 'gameDay'],
          message: 'history point exceeds throughGameDay',
        });
      }
      previousGameDay = point.gameDay;
    }
  });

// -- Finance accounts, transfers, and ledger (M2-A) ----------------------

export const FinanceAccountsResponseSchema = z.object({
  policySet: PolicySetSummarySchema,
  accounts: z.array(FinancialAccountSchema).max(32),
});

export const FinanceCommandRequestSchema = z
  .object({
    commandId: CanonicalUuidSchema,
    expectedRunRevision: z.number().int().nonnegative(),
    expectedStateRevision: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
    expectedGameDay: z.number().int().nonnegative(),
  })
  .strict();

export const TransferDirectionSchema = z.enum(['walletToAccount', 'accountToWallet']);

export const FinanceTransferDraftSchema = z.object({
  accountId: ResourceIdSchema,
  direction: TransferDirectionSchema,
  amountKrw: z.number().int().positive('이체 금액은 1원 이상이어야 합니다'),
});

export const FinanceTransferRequestSchema = FinanceTransferDraftSchema.extend({
  commandId: CanonicalUuidSchema,
  expectedRunRevision: z.number().int().nonnegative(),
  expectedStateRevision: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
  expectedGameDay: z.number().int().nonnegative(),
});

export const FinanceTransferResultSchema = z.object({
  commandId: CanonicalUuidSchema,
  accountId: ResourceIdSchema,
  direction: TransferDirectionSchema,
  amountKrw: z.number().int().positive(),
  replayed: z.boolean(),
});

export const FinanceTransferResponseSchema = z.object({
  transfer: FinanceTransferResultSchema,
  snapshot: GameSnapshotSchema,
});

// -- Cash products (M2-B) ------------------------------------------------

export const CmaAccountOpenDraftSchema = z
  .object({
    type: z.literal('cma'),
    productVersionId: ResourceIdSchema,
  })
  .strict();

export const CmaAccountCloseDraftSchema = z.object({ accountId: ResourceIdSchema }).strict();

export const DepositOpenDraftSchema = z
  .object({
    kind: DepositKindSchema,
    productVersionId: ResourceIdSchema,
    settlementAccountId: ResourceIdSchema,
    amountKrw: PositiveKrwSchema,
  })
  .strict();

export const DepositCloseDraftSchema = z.object({ contractId: ResourceIdSchema }).strict();

export const CmaAccountOpenRequestSchema = FinanceCommandRequestSchema.extend({
  type: z.literal('cma'),
  productVersionId: ResourceIdSchema,
}).strict();

export const CmaAccountOpenResultSchema = z.object({
  commandId: CanonicalUuidSchema,
  accountId: ResourceIdSchema,
  productVersionId: ResourceIdSchema,
  replayed: z.boolean(),
});

export const CmaAccountOpenResponseSchema = z.object({
  account: CmaAccountOpenResultSchema,
  snapshot: GameSnapshotSchema,
});

export const CmaAccountCloseRequestSchema = FinanceCommandRequestSchema;

export const CmaAccountCloseResultSchema = z.object({
  commandId: CanonicalUuidSchema,
  accountId: ResourceIdSchema,
  replayed: z.boolean(),
});

export const CmaAccountCloseResponseSchema = z.object({
  accountClose: CmaAccountCloseResultSchema,
  snapshot: GameSnapshotSchema,
});

export const DepositOpenRequestSchema = FinanceCommandRequestSchema.extend({
  kind: DepositKindSchema,
  productVersionId: ResourceIdSchema,
  settlementAccountId: ResourceIdSchema,
  amountKrw: PositiveKrwSchema,
}).strict();

export const DepositOpenResultSchema = z.object({
  commandId: CanonicalUuidSchema,
  contractId: ResourceIdSchema,
  kind: DepositKindSchema,
  productVersionId: ResourceIdSchema,
  settlementAccountId: ResourceIdSchema,
  amountKrw: PositiveKrwSchema,
  replayed: z.boolean(),
});

export const DepositOpenResponseSchema = z.object({
  deposit: DepositOpenResultSchema,
  snapshot: GameSnapshotSchema,
});

export const DepositCloseRequestSchema = FinanceCommandRequestSchema;

export const DepositCloseResultSchema = z.object({
  commandId: CanonicalUuidSchema,
  contractId: ResourceIdSchema,
  grossInterestKrw: NonnegativeKrwSchema,
  incomeTaxKrw: NonnegativeKrwSchema,
  localIncomeTaxKrw: NonnegativeKrwSchema,
  netPayoutKrw: NonnegativeKrwSchema,
  replayed: z.boolean(),
});

export const DepositCloseResponseSchema = z.object({
  depositClose: DepositCloseResultSchema,
  snapshot: GameSnapshotSchema,
});

// -- Tax-advantaged accounts (M2-C) --------------------------------------

export const TaxAccountOpenTypeSchema = z.enum([
  'isaGeneral',
  'isaLowIncome',
  'pensionSavings',
  'irp',
]);

export const TaxAccountOpenDraftSchema = z
  .object({
    type: TaxAccountOpenTypeSchema,
  })
  .strict();

export const IsaAccountCloseDraftSchema = z.object({ accountId: ResourceIdSchema }).strict();

export const PensionStartDraftSchema = z
  .object({
    accountId: ResourceIdSchema,
    paymentYears: z.number().int().min(5).max(100),
    lifetime: z.boolean(),
  })
  .strict();

export const PensionWithdrawalRequestKindSchema = z.enum(['pension', 'unavoidable', 'nonPension']);

export const IrpWithdrawalReasonSchema = z.enum([
  'homePurchase',
  'housingDeposit',
  'medicalCare',
  'disaster',
  'bankruptcy',
  'rehabilitation',
  'securedLoanRepayment',
]);

export const PensionWithdrawalDraftSchema = z
  .object({
    accountId: ResourceIdSchema,
    amountKrw: PositiveKrwSchema,
    type: PensionWithdrawalRequestKindSchema,
    reason: z.preprocess(
      (value) => (value === '' ? null : value),
      IrpWithdrawalReasonSchema.nullable(),
    ),
  })
  .strict();

export const TaxAccountOpenRequestSchema = FinanceCommandRequestSchema.extend({
  type: TaxAccountOpenTypeSchema,
}).strict();

export const TaxAccountOpenResultSchema = z
  .object({
    commandId: CanonicalUuidSchema,
    accountId: ResourceIdSchema,
    type: TaxAccountOpenTypeSchema,
    replayed: z.boolean(),
  })
  .strict();

export const TaxAccountOpenResponseSchema = z
  .object({
    account: TaxAccountOpenResultSchema,
    snapshot: GameSnapshotSchema,
  })
  .strict();

export const IsaAccountCloseRequestSchema = FinanceCommandRequestSchema;

export const IsaAccountCloseResultSchema = z
  .object({
    commandId: CanonicalUuidSchema,
    accountId: ResourceIdSchema,
    grossTaxProfitKrw: NonnegativeKrwSchema,
    deductibleLossKrw: NonnegativeKrwSchema,
    incomeTaxKrw: NonnegativeKrwSchema,
    localIncomeTaxKrw: NonnegativeKrwSchema,
    netPayoutKrw: NonnegativeKrwSchema,
    replayed: z.boolean(),
  })
  .strict();

export const IsaAccountCloseResponseSchema = z
  .object({
    isaClose: IsaAccountCloseResultSchema,
    snapshot: GameSnapshotSchema,
  })
  .strict();

export const PensionStartRequestSchema = FinanceCommandRequestSchema.extend({
  paymentYears: z.number().int().min(5).max(100),
  lifetime: z.boolean(),
}).strict();

export const PensionStartResultSchema = z
  .object({
    commandId: CanonicalUuidSchema,
    accountId: ResourceIdSchema,
    startTaxYear: TaxYearSchema,
    paymentYears: z.number().int().min(5).max(100),
    lifetime: z.boolean(),
    replayed: z.boolean(),
  })
  .strict();

export const PensionStartResponseSchema = z
  .object({
    pensionStart: PensionStartResultSchema,
    snapshot: GameSnapshotSchema,
  })
  .strict();

export const PensionWithdrawalRequestSchema = FinanceCommandRequestSchema.extend({
  amountKrw: PositiveKrwSchema,
  type: PensionWithdrawalRequestKindSchema,
  reason: IrpWithdrawalReasonSchema.nullable(),
}).strict();

export const PensionWithdrawalResultSchema = z
  .object({
    commandId: CanonicalUuidSchema,
    accountId: ResourceIdSchema,
    grossAmountKrw: PositiveKrwSchema,
    pensionAmountKrw: NonnegativeKrwSchema,
    nonPensionAmountKrw: NonnegativeKrwSchema,
    taxFreeAmountKrw: NonnegativeKrwSchema,
    taxKrw: NonnegativeKrwSchema,
    netPayoutKrw: NonnegativeKrwSchema,
    replayed: z.boolean(),
  })
  .strict()
  .superRefine((withdrawal, context) => {
    if (
      BigInt(withdrawal.grossAmountKrw) !==
      BigInt(withdrawal.pensionAmountKrw) + BigInt(withdrawal.nonPensionAmountKrw)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['grossAmountKrw'],
        message: 'pension and non-pension portions must equal the gross withdrawal',
      });
    }
    if (
      BigInt(withdrawal.netPayoutKrw) !==
      BigInt(withdrawal.grossAmountKrw) - BigInt(withdrawal.taxKrw)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['netPayoutKrw'],
        message: 'net pension payout must equal gross withdrawal minus tax',
      });
    }
    if (withdrawal.taxFreeAmountKrw > withdrawal.grossAmountKrw) {
      context.addIssue({
        code: 'custom',
        path: ['taxFreeAmountKrw'],
        message: 'tax-free amount must not exceed the gross withdrawal',
      });
    }
  });

export const PensionWithdrawalResponseSchema = z
  .object({
    pensionWithdrawal: PensionWithdrawalResultSchema,
    snapshot: GameSnapshotSchema,
  })
  .strict();

// -- Market-valued assets and annual tax (M2-D) --------------------------

export const BondTermYearsSchema = z.union([z.literal(3), z.literal(10)]);
export const BondOrderSideSchema = z.enum(['buy', 'sell']);

export const BondProductSchema = z
  .object({
    id: ResourceIdSchema,
    key: z.string().min(1).max(64),
    displayName: z.string().min(1).max(100),
    termYears: BondTermYearsSchema,
    faceValueKrw: PositiveKrwSchema,
    maxOrderUnits: PositiveU32Schema.max(100_000),
    maxPositionUnits: PositiveU32Schema,
    buyFeePpm: NonnegativeRatePpmSchema,
    sellFeePpm: NonnegativeRatePpmSchema,
  })
  .strict()
  .superRefine((product, context) => {
    if (product.maxOrderUnits > product.maxPositionUnits) {
      context.addIssue({
        code: 'custom',
        path: ['maxOrderUnits'],
        message: 'maximum bond order must not exceed the position limit',
      });
    }
  });

export const BondSeriesSchema = z
  .object({
    id: ResourceIdSchema,
    productVersionId: ResourceIdSchema,
    issuedDate: z.iso.date(),
    maturityDate: z.iso.date(),
    couponRateBp: z.number().int().safe().nonnegative(),
    issueYieldBp: z.number().int().safe().nonnegative(),
    nextCouponDate: z.iso.date(),
    dirtyPriceKrw: PositiveKrwSchema,
    currentYieldBp: z.number().int().safe().nonnegative(),
  })
  .strict()
  .superRefine((series, context) => {
    if (series.maturityDate <= series.issuedDate) {
      context.addIssue({
        code: 'custom',
        path: ['maturityDate'],
        message: 'bond maturity must follow its issue date',
      });
    }
    if (series.nextCouponDate <= series.issuedDate || series.nextCouponDate > series.maturityDate) {
      context.addIssue({
        code: 'custom',
        path: ['nextCouponDate'],
        message: 'next coupon date must fall after issuance and no later than maturity',
      });
    }
  });

export const BondProductCatalogSchema = z
  .object({
    marketVersion: z.string().min(1),
    products: z.array(BondProductSchema).max(2),
    series: z.array(BondSeriesSchema).max(160),
  })
  .strict()
  .superRefine((catalog, context) => {
    const productIds = new Set<string>();
    const productTerms = new Set<number>();
    for (const [index, product] of catalog.products.entries()) {
      if (productIds.has(product.id) || productTerms.has(product.termYears)) {
        context.addIssue({
          code: 'custom',
          path: ['products', index],
          message: 'bond products must be unique by ID and term',
        });
      }
      productIds.add(product.id);
      productTerms.add(product.termYears);
    }

    const seriesIds = new Set<string>();
    for (const [index, series] of catalog.series.entries()) {
      if (!productIds.has(series.productVersionId)) {
        context.addIssue({
          code: 'custom',
          path: ['series', index, 'productVersionId'],
          message: 'bond series must reference a catalog product',
        });
      }
      if (seriesIds.has(series.id)) {
        context.addIssue({
          code: 'custom',
          path: ['series', index, 'id'],
          message: 'bond series IDs must be unique',
        });
      }
      seriesIds.add(series.id);
    }
  });

export const BondOrderRequestSchema = FinanceCommandRequestSchema.extend({
  accountId: ResourceIdSchema,
  seriesId: ResourceIdSchema,
  side: BondOrderSideSchema,
  bondUnits: z.number().int().safe().min(1).max(100_000),
}).strict();

export const BondOrderResultSchema = z
  .object({
    commandId: CanonicalUuidSchema,
    executionId: CanonicalUuidSchema,
    accountId: ResourceIdSchema,
    seriesId: ResourceIdSchema,
    side: BondOrderSideSchema,
    bondUnits: z.number().int().safe().min(1).max(100_000),
    dirtyPriceKrw: PositiveKrwSchema,
    grossAmountKrw: NonnegativeKrwSchema,
    feeKrw: NonnegativeKrwSchema,
    taxKrw: NonnegativeKrwSchema,
    removedCostBasisKrw: NonnegativeKrwSchema,
    realizedGainLossKrw: z.number().int().safe(),
    replayed: z.boolean(),
  })
  .strict()
  .superRefine((order, context) => {
    if (BigInt(order.grossAmountKrw) !== BigInt(order.dirtyPriceKrw) * BigInt(order.bondUnits)) {
      context.addIssue({
        code: 'custom',
        path: ['grossAmountKrw'],
        message: 'bond gross must equal dirty price times units',
      });
    }
    const realizedGainLossKrw =
      order.side === 'buy'
        ? 0n
        : BigInt(order.grossAmountKrw) -
          BigInt(order.removedCostBasisKrw) -
          BigInt(order.feeKrw) -
          BigInt(order.taxKrw);
    if (
      (order.side === 'buy' && order.removedCostBasisKrw !== 0) ||
      BigInt(order.realizedGainLossKrw) !== realizedGainLossKrw
    ) {
      context.addIssue({
        code: 'custom',
        path: ['realizedGainLossKrw'],
        message: 'bond cost basis and realized result do not reconcile',
      });
    }
  });

export const BondOrderResponseSchema = z
  .object({
    bondOrder: BondOrderResultSchema,
    snapshot: GameSnapshotSchema,
  })
  .strict();

export const GoldWithdrawalBarSchema = z
  .object({
    barSizeGram: GoldBarSizeGramSchema,
    feeKrw: NonnegativeKrwSchema,
  })
  .strict();

export const GoldProductSchema = z
  .object({
    id: ResourceIdSchema,
    key: z.string().min(1).max(64),
    displayName: z.string().min(1).max(100),
    unit: z.literal('gram'),
    buyFeePpm: NonnegativeRatePpmSchema,
    sellFeePpm: NonnegativeRatePpmSchema,
    buyTaxPpm: NonnegativeRatePpmSchema,
    sellTaxPpm: NonnegativeRatePpmSchema,
    withdrawalBars: z.array(GoldWithdrawalBarSchema).length(2),
  })
  .strict()
  .superRefine((product, context) => {
    const barSizes = new Set(product.withdrawalBars.map((bar) => bar.barSizeGram));
    if (!barSizes.has(100) || !barSizes.has(1_000)) {
      context.addIssue({
        code: 'custom',
        path: ['withdrawalBars'],
        message: 'gold product must publish one 100g and one 1000g withdrawal bar',
      });
    }
  });

export const GoldProductCatalogSchema = z
  .object({
    marketVersion: z.string().min(1),
    products: z.array(GoldProductSchema).max(1),
  })
  .strict();

export const GoldAccountOpenRequestSchema = FinanceCommandRequestSchema.extend({
  type: z.literal('krxGold'),
  productVersionId: ResourceIdSchema,
}).strict();

export const GoldAccountOpenResultSchema = z
  .object({
    commandId: CanonicalUuidSchema,
    accountId: ResourceIdSchema,
    type: z.literal('krxGold'),
    productVersionId: ResourceIdSchema,
    replayed: z.boolean(),
  })
  .strict();

export const GoldAccountOpenResponseSchema = z
  .object({
    account: GoldAccountOpenResultSchema,
    snapshot: GameSnapshotSchema,
  })
  .strict();

export const GoldOrderSideSchema = z.enum(['buy', 'sell']);

export const GoldOrderRequestSchema = FinanceCommandRequestSchema.extend({
  accountId: ResourceIdSchema,
  side: GoldOrderSideSchema,
  quantityGram: PositiveU32Schema,
}).strict();

export const GoldOrderResultSchema = z
  .object({
    commandId: CanonicalUuidSchema,
    executionId: CanonicalUuidSchema,
    accountId: ResourceIdSchema,
    side: GoldOrderSideSchema,
    quantityGram: PositiveU32Schema,
    priceKrwPerGram: PositiveKrwSchema,
    grossAmountKrw: NonnegativeKrwSchema,
    feeKrw: NonnegativeKrwSchema,
    taxKrw: NonnegativeKrwSchema,
    removedCostBasisKrw: NonnegativeKrwSchema,
    realizedGainLossKrw: z.number().int().safe(),
    replayed: z.boolean(),
  })
  .strict()
  .superRefine((order, context) => {
    if (
      BigInt(order.grossAmountKrw) !==
      BigInt(order.priceKrwPerGram) * BigInt(order.quantityGram)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['grossAmountKrw'],
        message: 'gold gross must equal gram price times quantity',
      });
    }
    const realizedGainLossKrw =
      order.side === 'buy'
        ? 0n
        : BigInt(order.grossAmountKrw) -
          BigInt(order.removedCostBasisKrw) -
          BigInt(order.feeKrw) -
          BigInt(order.taxKrw);
    if (
      (order.side === 'buy' && order.removedCostBasisKrw !== 0) ||
      BigInt(order.realizedGainLossKrw) !== realizedGainLossKrw
    ) {
      context.addIssue({
        code: 'custom',
        path: ['realizedGainLossKrw'],
        message: 'gold cost basis and realized result do not reconcile',
      });
    }
  });

export const GoldOrderResponseSchema = z
  .object({
    goldOrder: GoldOrderResultSchema,
    snapshot: GameSnapshotSchema,
  })
  .strict();

export const GoldWithdrawalRequestSchema = FinanceCommandRequestSchema.extend({
  accountId: ResourceIdSchema,
  barSizeGram: GoldBarSizeGramSchema,
  barCount: PositiveU32Schema,
}).strict();

export const GoldWithdrawalResultSchema = z
  .object({
    commandId: CanonicalUuidSchema,
    withdrawalId: CanonicalUuidSchema,
    accountId: ResourceIdSchema,
    barSizeGram: GoldBarSizeGramSchema,
    barCount: PositiveU32Schema,
    quantityGram: PositiveU32Schema,
    removedCostBasisKrw: NonnegativeKrwSchema,
    vatKrw: NonnegativeKrwSchema,
    feeKrw: NonnegativeKrwSchema,
    cashChargedKrw: NonnegativeKrwSchema,
    replayed: z.boolean(),
  })
  .strict()
  .superRefine((withdrawal, context) => {
    if (
      BigInt(withdrawal.quantityGram) !==
      BigInt(withdrawal.barSizeGram) * BigInt(withdrawal.barCount)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['quantityGram'],
        message: 'withdrawal quantity must equal bar size times count',
      });
    }
    if (
      BigInt(withdrawal.cashChargedKrw) !==
      BigInt(withdrawal.vatKrw) + BigInt(withdrawal.feeKrw)
    ) {
      context.addIssue({
        code: 'custom',
        path: ['cashChargedKrw'],
        message: 'gold withdrawal charge must equal VAT plus fee',
      });
    }
  });

export const GoldWithdrawalResponseSchema = z
  .object({
    goldWithdrawal: GoldWithdrawalResultSchema,
    snapshot: GameSnapshotSchema,
  })
  .strict();

export const FinanceFailureCodeSchema = z.enum([
  'invalidCommand',
  'characterRequired',
  'accountNotFound',
  'accountClosed',
  'accountTypeNotAllowed',
  'insufficientWalletCash',
  'insufficientAccountCash',
  'policyNotEligible',
  'limitExceeded',
  'productNotFound',
  'contractNotFound',
  'contractClosed',
  'accountNotEmpty',
  'accountAlreadyExists',
  'rateUnavailable',
  'marketClosed',
  'insufficientQuantity',
  'positionLimit',
  'settlementConflict',
  'idempotencyConflict',
  'busy',
]);

export const FinanceFailureSchema = z.object({
  code: FinanceFailureCodeSchema,
  message: z.string().min(1),
});

export const LedgerSourceKindSchema = z.enum([
  'm2OpeningBalance',
  'transfer',
  'trade',
  'cashProductEnrollment',
  'cashProductClose',
  'interestAccrual',
  'scheduledSettlement',
  'isaClose',
  'pensionWithdrawal',
  'specActivity',
  'employmentPayroll',
  'careerRewardPayment',
  'pensionCreditAllocation',
  'militaryPay',
  'militarySavingsInstallment',
  'militarySavingsMaturity',
  'militarySavingsGovernmentMatch',
  'militarySavingsEarlyClose',
  'livingCostMonth',
  'essentialArrearPayment',
  'loanOrigination',
  'loanInstallment',
  'loanPrepayment',
  'debtAuthorityBridge',
  'leaseMove',
  'leaseRent',
  'leaseArrearPayment',
  'propertyPurchase',
  'propertySale',
  'propertyTaxPayment',
  'welfareBenefitPayment',
  'lifeEventChoice',
  'insurancePremiumPayment',
  'insuranceClaimPayment',
  'correction',
]);

export const LedgerAccountCodeSchema = z.enum([
  'wallet',
  'accountCash',
  'productPrincipal',
  'debtPrincipal',
  'openingEquity',
  'withholdingTaxLiability',
  'interestIncome',
  'feeExpense',
  'distributionIncome',
  'realizedGainLoss',
  'taxSettlement',
  'careerDevelopmentExpense',
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
  'pensionTaxExcludedContribution',
  'pensionCreditedContribution',
  'militaryPayIncome',
  'militarySavingsPrincipal',
  'militarySavingsBankInterest',
  'militarySavingsGovernmentMatchIncome',
  'livingCostExpense',
  'essentialArrearLiability',
  'loanPrincipalLiability',
  'loanInterestExpense',
  'loanInterestLiability',
  'loanFeeExpense',
  'taxObligationLiability',
  'leaseDepositAsset',
  'movingExpense',
  'leaseRentExpense',
  'leaseArrearLiability',
  'propertyAsset',
  'acquisitionIncidentalExpense',
  'propertyDispositionExpense',
  'propertyTaxExpense',
  'welfareBenefitIncome',
  'lifeEventExpense',
  'insurancePremiumExpense',
  'insuranceClaimRecovery',
]);

export const LedgerPostingSchema = z
  .object({
    accountCode: LedgerAccountCodeSchema,
    accountId: ResourceIdSchema.nullable(),
    amountKrw: z
      .number()
      .int()
      .safe()
      .refine((amount) => amount !== 0, 'posting must be nonzero'),
  })
  .superRefine((posting, context) => {
    const accountRequired =
      posting.accountCode === 'accountCash' ||
      posting.accountCode === 'productPrincipal' ||
      posting.accountCode === 'pensionTaxExcludedContribution' ||
      posting.accountCode === 'pensionCreditedContribution';
    if (accountRequired !== (posting.accountId !== null)) {
      context.addIssue({
        code: 'custom',
        path: ['accountId'],
        message: accountRequired
          ? 'account posting requires an account ID'
          : 'non-account posting forbids an account ID',
      });
    }
  });

export const LedgerTransactionSchema = z
  .object({
    id: ResourceIdSchema,
    gameDay: z.number().int().nonnegative(),
    description: z.string().min(1),
    sourceKind: LedgerSourceKindSchema,
    postings: z.array(LedgerPostingSchema).min(2),
  })
  .superRefine((transaction, context) => {
    const balance = transaction.postings.reduce(
      (sum, posting) => sum + BigInt(posting.amountKrw),
      0n,
    );
    if (balance !== 0n) {
      context.addIssue({
        code: 'custom',
        path: ['postings'],
        message: 'ledger postings must balance to zero',
      });
    }
  });

export const LedgerPageSchema = z
  .object({
    transactions: z.array(LedgerTransactionSchema).max(200),
    nextBefore: ResourceIdSchema.nullable(),
  })
  .superRefine((page, context) => {
    let previousId: bigint | undefined;
    for (const [index, transaction] of page.transactions.entries()) {
      const id = BigInt(transaction.id);
      if (previousId !== undefined && id >= previousId) {
        context.addIssue({
          code: 'custom',
          path: ['transactions', index, 'id'],
          message: 'ledger transactions must be in descending ID order',
        });
      }
      previousId = id;
    }

    if (page.nextBefore !== null) {
      const last = page.transactions.at(-1);
      if (last === undefined || last.id !== page.nextBefore) {
        context.addIssue({
          code: 'custom',
          path: ['nextBefore'],
          message: 'nextBefore must match the oldest transaction in the page',
        });
      }
    }
  });

// -- Character creation (§3) ---------------------------------------------

export const GenderSchema = z.enum(['male', 'female', 'other']);
export const MilitaryStatusSchema = z.enum([
  'notServed',
  'serving',
  'completed',
  'exempted',
  'alternative',
]);
export const FamilyBackgroundSchema = z.enum(['supportive', 'independent', 'dependent']);
export const HealthLevelSchema = z.enum(['good', 'normal', 'poor']);

/**
 * Form input validation, covering the shape of each field only. Contradictory
 * combinations (§3.5) are the server's sole authority and are not reimplemented here.
 */
export const CharacterDraftSchema = z.object({
  name: z.string().trim().min(1, '이름을 입력하세요').max(20, '이름이 너무 깁니다'),
  age: z.number().int().min(19, '19세 이상이어야 합니다').max(50, '50세 이하여야 합니다'),
  gender: GenderSchema,
  military: MilitaryStatusSchema,
  region: RegionSchema,
  background: FamilyBackgroundSchema,
  education: EducationSchema,
  careerYears: z.number().int().min(0).max(30, '경력은 30년을 넘을 수 없습니다'),
  certifications: z.number().int().min(0).max(50),
  startingCashKrw: z.number().int().min(0, '시작 자금은 0원 이상이어야 합니다'),
  studentLoanKrw: z.number().int().min(0),
  creditLoanKrw: z.number().int().min(0),
  health: HealthLevelSchema,
  dependents: z.number().int().min(0).max(6, '부양가족은 6명을 넘을 수 없습니다'),
});

export const CharacterStartProfileSchema = CharacterDraftSchema.omit({
  studentLoanKrw: true,
  creditLoanKrw: true,
}).strict();

export const CharacterStartingLoanSchema = z
  .object({
    kind: z.enum(['studentLoan', 'unsecuredLoan']),
    productVersionId: ResourceIdSchema,
    principalKrw: PositiveKrwSchema,
  })
  .strict();

export const CharacterStartingLoansSchema = z
  .array(CharacterStartingLoanSchema)
  .max(2)
  .superRefine((loans, context) => {
    const order = { studentLoan: 0, unsecuredLoan: 1 } as const;
    const kinds = new Set(loans.map((loan) => loan.kind));
    const canonical = loans.every((loan, index) => {
      const previous = loans[index - 1];
      return previous === undefined || order[previous.kind] < order[loan.kind];
    });
    if (kinds.size !== loans.length || !canonical) {
      context.addIssue({
        code: 'custom',
        message: 'starting loans must be unique and use canonical kind order',
      });
    }
  });

const CharacterStartCommandFields = {
  commandId: CanonicalUuidSchema,
  expectedRunRevision: z.number().int().nonnegative(),
  expectedStateRevision: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
  expectedGameDay: z.number().int().nonnegative(),
} as const;

export const CharacterStartV1RequestSchema = z
  .object({
    ...CharacterStartCommandFields,
    character: CharacterDraftSchema.strict(),
  })
  .strict();

export const CharacterStartV2DraftSchema = z
  .object({
    character: CharacterStartProfileSchema,
    startingLoans: CharacterStartingLoansSchema,
  })
  .strict();

export const CharacterStartV2RequestSchema = CharacterStartV2DraftSchema.extend({
  ...CharacterStartCommandFields,
}).strict();

export const CharacterStartRequestSchema = z.union([
  CharacterStartV2RequestSchema,
  CharacterStartV1RequestSchema,
]);

export const CharacterStartResultSchema = z.object({
  commandId: CanonicalUuidSchema,
  committedCursor: GameCommandCursorSchema,
  replayed: z.boolean(),
});

export const CharacterStartResponseSchema = z.object({
  start: CharacterStartResultSchema,
  snapshot: GameSnapshotSchema,
});

export const PresetSchema = z.object({
  id: z.string(),
  label: z.string(),
  summary: z.string(),
  age: z.number().int(),
  military: MilitaryStatusSchema,
  education: EducationSchema,
  region: RegionSchema,
  background: FamilyBackgroundSchema,
  careerYears: z.number().int(),
  certifications: z.number().int(),
  startingCashKrw: z.number().int(),
  studentLoanKrw: z.number().int(),
  creditLoanKrw: z.number().int(),
  health: HealthLevelSchema,
  dependents: z.number().int(),
});

export const PresetListSchema = z.array(PresetSchema);

/** The 422 body for a failed combination check; `field` matches the form field name. */
export const ValidationFailureSchema = z.object({
  errors: z.array(z.object({ field: z.string(), message: z.string() })),
});

// -- Login (§4.5) --------------------------------------------------------

export const ProviderKindSchema = z.enum(['datagsm', 'google']);

/**
 * A login provider the server enabled. One without credentials never reaches this list,
 * so the client simply draws what it receives.
 */
export const AuthProviderSchema = z.object({
  id: ProviderKindSchema,
  label: z.string(),
});

export const AuthProviderListSchema = z.array(AuthProviderSchema);

export const MeSchema = z.object({
  provider: ProviderKindSchema,
  email: z.string().nullable(),
  displayName: z.string().nullable(),
});

export type Health = z.infer<typeof HealthSchema>;
export type GameSpeed = z.infer<typeof GameSpeedSchema>;
export type ResourceId = z.infer<typeof ResourceIdSchema>;
export type MarketRegime = z.infer<typeof MarketRegimeSchema>;
export type MarketRates = z.infer<typeof MarketRatesSchema>;
export type MarketSnapshot = z.infer<typeof MarketSnapshotSchema>;
export type PortfolioPosition = z.infer<typeof PortfolioPositionSchema>;
export type PortfolioSnapshot = z.infer<typeof PortfolioSnapshotSchema>;
export type FinancialAccountType = z.infer<typeof FinancialAccountTypeSchema>;
export type FinancialAccountStatus = z.infer<typeof FinancialAccountStatusSchema>;
export type FinancialAccount = z.infer<typeof FinancialAccountSchema>;
export type PolicySetSummary = z.infer<typeof PolicySetSummarySchema>;
export type SettlementKind = z.infer<typeof SettlementKindSchema>;
export type PendingSettlementSummary = z.infer<typeof PendingSettlementSummarySchema>;
export type CashProductKind = z.infer<typeof CashProductKindSchema>;
export type DepositKind = z.infer<typeof DepositKindSchema>;
export type CashRateReference = z.infer<typeof CashRateReferenceSchema>;
export type FinancialInstitutionSummary = z.infer<typeof FinancialInstitutionSummarySchema>;
export type CashProduct = z.infer<typeof CashProductSchema>;
export type CashProductCatalog = z.infer<typeof CashProductCatalogSchema>;
export type CmaAccountSummary = z.infer<typeof CmaAccountSummarySchema>;
export type CashContractStatus = z.infer<typeof CashContractStatusSchema>;
export type CashContractSummary = z.infer<typeof CashContractSummarySchema>;
export type DepositProtectionSummary = z.infer<typeof DepositProtectionSummarySchema>;
export type TaxYear = z.infer<typeof TaxYearSchema>;
export type FinancialIncomeSource = z.infer<typeof FinancialIncomeSourceSchema>;
export type FinancialIncomeSourceTotal = z.infer<typeof FinancialIncomeSourceTotalSchema>;
export type FinancialIncomeYear = z.infer<typeof FinancialIncomeYearSchema>;
export type FinancialIncomeAssessment = z.infer<typeof FinancialIncomeAssessmentSchema>;
export type IsaAccountType = z.infer<typeof IsaAccountTypeSchema>;
export type IsaAccountSummary = z.infer<typeof IsaAccountSummarySchema>;
export type PensionAccountType = z.infer<typeof PensionAccountTypeSchema>;
export type PensionTaxLayers = z.infer<typeof PensionTaxLayersSchema>;
export type PensionAccountSummary = z.infer<typeof PensionAccountSummarySchema>;
export type IndexProductSummary = z.infer<typeof IndexProductSummarySchema>;
export type FinanceProductBundle = z.infer<typeof FinanceProductBundleSchema>;
export type LlxDistributionEntitlement = z.infer<typeof LlxDistributionEntitlementSchema>;
export type BondPositionSummary = z.infer<typeof BondPositionSummarySchema>;
export type GoldAccountSummary = z.infer<typeof GoldAccountSummarySchema>;
export type GoldBarSizeGram = z.infer<typeof GoldBarSizeGramSchema>;
export type PhysicalGoldHolding = z.infer<typeof PhysicalGoldHoldingSchema>;
export type FinanceSnapshot = z.infer<typeof FinanceSnapshotSchema>;
export type SpecDimension = z.infer<typeof SpecDimensionSchema>;
export type EvidenceKind = z.infer<typeof EvidenceKindSchema>;
export type LifeStatus = z.infer<typeof LifeStatusSchema>;
export type CareerActivityStatus = z.infer<typeof CareerActivityStatusSchema>;
export type CareerArtifactKind = z.infer<typeof CareerArtifactKindSchema>;
export type CareerIndustry = z.infer<typeof CareerIndustrySchema>;
export type CareerScores = z.infer<typeof CareerScoresSchema>;
export type CareerActivitySummary = z.infer<typeof CareerActivitySummarySchema>;
export type CareerArtifactSummary = z.infer<typeof CareerArtifactSummarySchema>;
export type CareerScheduledActionKind = z.infer<typeof CareerScheduledActionKindSchema>;
export type CareerScheduledSettlementKind = z.infer<typeof CareerScheduledSettlementKindSchema>;
export type CareerPendingScheduleItem = z.infer<typeof CareerPendingScheduleItemSchema>;
export type CareerSnapshot = z.infer<typeof CareerSnapshotSchema>;
export type LivingCostCategory = z.infer<typeof LivingCostCategorySchema>;
export type LifeBudgetSelection = z.infer<typeof LifeBudgetSelectionSchema>;
export type LifeRateStatus = z.infer<typeof LifeRateStatusSchema>;
export type ResidenceTenureKind = z.infer<typeof ResidenceTenureKindSchema>;
export type YearMonth = z.infer<typeof YearMonthSchema>;
export type LifeHousehold = z.infer<typeof LifeHouseholdSchema>;
export type LifeResidence = z.infer<typeof LifeResidenceSchema>;
export type HousingPurchaseCapability = z.infer<typeof HousingPurchaseCapabilitySchema>;
export type HousingPropertyHoldingStatus = z.infer<typeof HousingPropertyHoldingStatusSchema>;
export type HousingPropertyHoldingPurpose = z.infer<typeof HousingPropertyHoldingPurposeSchema>;
export type HousingPropertyHolding = z.infer<typeof HousingPropertyHoldingSchema>;
export type LifeBudgetBand = z.infer<typeof LifeBudgetBandSchema>;
export type LivingCostMonthItem = z.infer<typeof LivingCostMonthItemSchema>;
export type LivingCostMonth = z.infer<typeof LivingCostMonthSchema>;
export type EssentialArrear = z.infer<typeof EssentialArrearSchema>;
export type CreditBand = z.infer<typeof CreditBandSchema>;
export type CreditReason = z.infer<typeof CreditReasonSchema>;
export type LoanProductKind = z.infer<typeof LoanProductKindSchema>;
export type LoanRateStatus = z.infer<typeof LoanRateStatusSchema>;
export type LoanLenderSector = z.infer<typeof LoanLenderSectorSchema>;
export type LoanRateType = z.infer<typeof LoanRateTypeSchema>;
export type LoanRateReference = z.infer<typeof LoanRateReferenceSchema>;
export type LoanRateResetRule = z.infer<typeof LoanRateResetRuleSchema>;
export type LoanDayCountRule = z.infer<typeof LoanDayCountRuleSchema>;
export type LoanRepaymentMethod = z.infer<typeof LoanRepaymentMethodSchema>;
export type LoanPaymentCalendar = z.infer<typeof LoanPaymentCalendarSchema>;
export type LoanPrepaymentEffect = z.infer<typeof LoanPrepaymentEffectSchema>;
export type LoanProductProvenance = z.infer<typeof LoanProductProvenanceSchema>;
export type LoanProduct = z.infer<typeof LoanProductSchema>;
export type HousingMortgageProduct = z.infer<typeof HousingMortgageProductSchema>;
export type LoanProductCatalog = z.infer<typeof LoanProductCatalogSchema>;
export type LoanContractStatus = z.infer<typeof LoanContractStatusSchema>;
export type LoanSummary = z.infer<typeof LoanSummarySchema>;
export type NextLoanInstallment = z.infer<typeof NextLoanInstallmentSchema>;
export type CreditResponse = z.infer<typeof CreditResponseSchema>;
export type WelfareEvaluationStatus = z.infer<typeof WelfareEvaluationStatusSchema>;
export type WelfareConditionOutcome = z.infer<typeof WelfareConditionOutcomeSchema>;
export type WelfareApplicationStatus = z.infer<typeof WelfareApplicationStatusSchema>;
export type WelfarePaymentStatus = z.infer<typeof WelfarePaymentStatusSchema>;
export type WelfareConditionResult = z.infer<typeof WelfareConditionResultSchema>;
export type WelfarePayment = z.infer<typeof WelfarePaymentSchema>;
export type WelfareApplicationSummary = z.infer<typeof WelfareApplicationSummarySchema>;
export type WelfareProgram = z.infer<typeof WelfareProgramSchema>;
export type WelfareProgramsResponse = z.infer<typeof WelfareProgramsResponseSchema>;
export type ActiveWelfareApplication = z.infer<typeof ActiveWelfareApplicationSchema>;
export type InsuranceCapability = z.infer<typeof InsuranceCapabilitySchema>;
export type InsuranceEligibilityStatus = z.infer<typeof InsuranceEligibilityStatusSchema>;
export type InsuranceEligibilityReason = z.infer<typeof InsuranceEligibilityReasonSchema>;
export type InsuranceContractStatus = z.infer<typeof InsuranceContractStatusSchema>;
export type InsuranceProduct = z.infer<typeof InsuranceProductSchema>;
export type InsuranceContract = z.infer<typeof InsuranceContractSchema>;
export type InsuranceClaimContractAllocation = z.infer<
  typeof InsuranceClaimContractAllocationSchema
>;
export type PendingInsuranceClaim = z.infer<typeof PendingInsuranceClaimSchema>;
export type InsuranceClaimHistoryItem = z.infer<typeof InsuranceClaimHistoryItemSchema>;
export type InsuranceContractsQuery = z.infer<typeof InsuranceContractsQuerySchema>;
export type InsuranceContractsResponse = z.infer<typeof InsuranceContractsResponseSchema>;
export type LifeEventCapability = z.infer<typeof LifeEventCapabilitySchema>;
export type LifeEventDecisionKind = z.infer<typeof LifeEventDecisionKindSchema>;
export type LifeEventResolutionKind = z.infer<typeof LifeEventResolutionKindSchema>;
export type LifeEventEffectSummary = z.infer<typeof LifeEventEffectSummarySchema>;
export type LifeEventChoice = z.infer<typeof LifeEventChoiceSchema>;
export type PendingLifeEvent = z.infer<typeof PendingLifeEventSchema>;
export type LifeEventHistoryItem = z.infer<typeof LifeEventHistoryItemSchema>;
export type LifeEventsResponse = z.infer<typeof LifeEventsResponseSchema>;
export type LifeEventsQuery = z.infer<typeof LifeEventsQuerySchema>;
export type LifeSnapshot = z.infer<typeof LifeSnapshotSchema>;
export type LifeBudgetResponse = z.infer<typeof LifeBudgetResponseSchema>;
export type GameSnapshot = z.infer<typeof GameSnapshotSchema>;
export type GameCommandCursor = z.infer<typeof GameCommandCursorSchema>;
export type LifeBudgetUpdateDraft = z.infer<typeof LifeBudgetUpdateDraftSchema>;
export type LifeBudgetUpdateRequest = z.infer<typeof LifeBudgetUpdateRequestSchema>;
export type EssentialArrearPaymentDraft = z.infer<typeof EssentialArrearPaymentDraftSchema>;
export type EssentialArrearPaymentRequest = z.infer<typeof EssentialArrearPaymentRequestSchema>;
export type WelfareApplicationRequest = z.infer<typeof WelfareApplicationRequestSchema>;
export type WelfareApplicationResult = z.infer<typeof WelfareApplicationResultSchema>;
export type WelfareApplicationResponse = z.infer<typeof WelfareApplicationResponseSchema>;
export type LifeEventChoiceRequest = z.infer<typeof LifeEventChoiceRequestSchema>;
export type LifeEventChoiceResult = z.infer<typeof LifeEventChoiceResultSchema>;
export type LifeEventChoiceResponse = z.infer<typeof LifeEventChoiceResponseSchema>;
export type InsuranceEnrollmentRequest = z.infer<typeof InsuranceEnrollmentRequestSchema>;
export type InsuranceCancellationRequest = z.infer<typeof InsuranceCancellationRequestSchema>;
export type InsuranceClaimRequest = z.infer<typeof InsuranceClaimRequestSchema>;
export type InsuranceEnrollmentResult = z.infer<typeof InsuranceEnrollmentResultSchema>;
export type InsuranceCancellationResult = z.infer<typeof InsuranceCancellationResultSchema>;
export type InsuranceClaimResult = z.infer<typeof InsuranceClaimResultSchema>;
export type InsuranceEnrollmentResponse = z.infer<typeof InsuranceEnrollmentResponseSchema>;
export type InsuranceCancellationResponse = z.infer<typeof InsuranceCancellationResponseSchema>;
export type InsuranceClaimResponse = z.infer<typeof InsuranceClaimResponseSchema>;
export type InsuranceFailureCode = z.infer<typeof InsuranceFailureCodeSchema>;
export type InsuranceFailure = z.infer<typeof InsuranceFailureSchema>;
export type LifeFailureCode = z.infer<typeof LifeFailureCodeSchema>;
export type LifeFailure = z.infer<typeof LifeFailureSchema>;
export type LifeBudgetUpdateResult = z.infer<typeof LifeBudgetUpdateResultSchema>;
export type LifeBudgetUpdateResponse = z.infer<typeof LifeBudgetUpdateResponseSchema>;
export type EssentialArrearPaymentResult = z.infer<typeof EssentialArrearPaymentResultSchema>;
export type EssentialArrearPaymentResponse = z.infer<typeof EssentialArrearPaymentResponseSchema>;
export type LoanQuoteDraft = z.infer<typeof LoanQuoteDraftSchema>;
export type LoanQuoteRequest = z.infer<typeof LoanQuoteRequestSchema>;
export type LoanQuoteDecisionCode = z.infer<typeof LoanQuoteDecisionCodeSchema>;
export type LoanQuoteDecisionReason = z.infer<typeof LoanQuoteDecisionReasonSchema>;
export type VerifiedIncomeSource = z.infer<typeof VerifiedIncomeSourceSchema>;
export type LoanQuoteDsr = z.infer<typeof LoanQuoteDsrSchema>;
export type LoanQuoteFirstInstallment = z.infer<typeof LoanQuoteFirstInstallmentSchema>;
export type LoanQuotedTerms = z.infer<typeof LoanQuotedTermsSchema>;
export type LoanQuoteResult = z.infer<typeof LoanQuoteResultSchema>;
export type LoanQuoteResponse = z.infer<typeof LoanQuoteResponseSchema>;
export type LoanExecutionDraft = z.infer<typeof LoanExecutionDraftSchema>;
export type LoanExecutionRequest = z.infer<typeof LoanExecutionRequestSchema>;
export type LoanExecutionResult = z.infer<typeof LoanExecutionResultSchema>;
export type LoanExecutionResponse = z.infer<typeof LoanExecutionResponseSchema>;
export type LoanPrepaymentDraft = z.infer<typeof LoanPrepaymentDraftSchema>;
export type LoanPrepaymentRequest = z.infer<typeof LoanPrepaymentRequestSchema>;
export type LoanPrepaymentStatus = z.infer<typeof LoanPrepaymentStatusSchema>;
export type LoanPrepaymentNextInstallment = z.infer<typeof LoanPrepaymentNextInstallmentSchema>;
export type LoanPrepaymentResult = z.infer<typeof LoanPrepaymentResultSchema>;
export type LoanPrepaymentResponse = z.infer<typeof LoanPrepaymentResponseSchema>;
export type LoanDetail = z.infer<typeof LoanDetailSchema>;
export type LoanInstallmentStatus = z.infer<typeof LoanInstallmentStatusSchema>;
export type LoanInstallmentHistoryItem = z.infer<typeof LoanInstallmentHistoryItemSchema>;
export type LoanPaymentKind = z.infer<typeof LoanPaymentKindSchema>;
export type LoanPaymentAllocationKind = z.infer<typeof LoanPaymentAllocationKindSchema>;
export type LoanPaymentAllocation = z.infer<typeof LoanPaymentAllocationSchema>;
export type LoanPaymentHistoryItem = z.infer<typeof LoanPaymentHistoryItemSchema>;
export type LoanInstallmentCursor = z.infer<typeof LoanInstallmentCursorSchema>;
export type LoanInstallmentHistoryQuery = z.infer<typeof LoanInstallmentHistoryQuerySchema>;
export type LoanInstallmentHistoryResponse = z.infer<typeof LoanInstallmentHistoryResponseSchema>;
export type HousingRegionKey = z.infer<typeof HousingRegionKeySchema>;
export type HousingRateStatus = z.infer<typeof HousingRateStatusSchema>;
export type HousingPropertyType = z.infer<typeof HousingPropertyTypeSchema>;
export type HousingOfferKind = z.infer<typeof HousingOfferKindSchema>;
export type HousingLeaseCapability = z.infer<typeof HousingLeaseCapabilitySchema>;
export type HousingLeaseRenewalRule = z.infer<typeof HousingLeaseRenewalRuleSchema>;
export type HousingLeaseTerminationReviewRule = z.infer<
  typeof HousingLeaseTerminationReviewRuleSchema
>;
export type HousingRentChargeRule = z.infer<typeof HousingRentChargeRuleSchema>;
export type HousingArrearRepaymentRule = z.infer<typeof HousingArrearRepaymentRuleSchema>;
export type HousingLeaseLifecycleTerms = z.infer<typeof HousingLeaseLifecycleTermsSchema>;
export type HousingLeaseCurrentTerm = z.infer<typeof HousingLeaseCurrentTermSchema>;
export type HousingLeaseRenewalNotice = z.infer<typeof HousingLeaseRenewalNoticeSchema>;
export type HousingLeaseTerminationReview = z.infer<typeof HousingLeaseTerminationReviewSchema>;
export type HousingActiveLease = z.infer<typeof HousingActiveLeaseSchema>;
export type HousingLeaseArrear = z.infer<typeof HousingLeaseArrearSchema>;
export type HousingOffer = z.infer<typeof HousingOfferSchema>;
export type HousingRegion = z.infer<typeof HousingRegionSchema>;
export type HousingListing = z.infer<typeof HousingListingSchema>;
export type HousingListingsQuery = z.infer<typeof HousingListingsQuerySchema>;
export type HousingListingsResponse = z.infer<typeof HousingListingsResponseSchema>;
export type HousingMovingCost = z.infer<typeof HousingMovingCostSchema>;
export type HousingMonthlyRentTerms = z.infer<typeof HousingMonthlyRentTermsSchema>;
export type HousingCurrentLeaseResponse = z.infer<typeof HousingCurrentLeaseResponseSchema>;
export type HousingLeaseDepositLoanQuoteDraft = z.infer<
  typeof HousingLeaseDepositLoanQuoteDraftSchema
>;
export type HousingLeaseDepositLoanQuoteRequest = z.infer<
  typeof HousingLeaseDepositLoanQuoteRequestSchema
>;
export type HousingLeaseDepositLoanDecisionCode = z.infer<
  typeof HousingLeaseDepositLoanDecisionCodeSchema
>;
export type HousingLeaseDepositLoanDecisionReason = z.infer<
  typeof HousingLeaseDepositLoanDecisionReasonSchema
>;
export type HousingLeaseDepositLoanAffordability = z.infer<
  typeof HousingLeaseDepositLoanAffordabilitySchema
>;
export type HousingLeaseDepositLoanQuoteResult = z.infer<
  typeof HousingLeaseDepositLoanQuoteResultSchema
>;
export type HousingLeaseDepositLoanQuoteResponse = z.infer<
  typeof HousingLeaseDepositLoanQuoteResponseSchema
>;
export type HousingLeaseDraft = z.infer<typeof HousingLeaseDraftSchema>;
export type HousingLeaseRequest = z.infer<typeof HousingLeaseRequestSchema>;
export type HousingDepositLoanExecution = z.infer<typeof HousingDepositLoanExecutionSchema>;
export type HousingRepaidDepositLoan = z.infer<typeof HousingRepaidDepositLoanSchema>;
export type HousingLeaseResult = z.infer<typeof HousingLeaseResultSchema>;
export type HousingLeaseResponse = z.infer<typeof HousingLeaseResponseSchema>;
export type HousingPropertyHoldingsResponse = z.infer<typeof HousingPropertyHoldingsResponseSchema>;
export type HousingMortgageQuoteDraft = z.infer<typeof HousingMortgageQuoteDraftSchema>;
export type HousingMortgageQuoteRequest = z.infer<typeof HousingMortgageQuoteRequestSchema>;
export type HousingMortgageQuoteDecisionCode = z.infer<
  typeof HousingMortgageQuoteDecisionCodeSchema
>;
export type HousingMortgageQuoteDecisionReason = z.infer<
  typeof HousingMortgageQuoteDecisionReasonSchema
>;
export type HousingMortgageLtvRegionClass = z.infer<typeof HousingMortgageLtvRegionClassSchema>;
export type HousingMortgageStressTreatment = z.infer<typeof HousingMortgageStressTreatmentSchema>;
export type HousingMortgageLtv = z.infer<typeof HousingMortgageLtvSchema>;
export type HousingMortgageQuoteResult = z.infer<typeof HousingMortgageQuoteResultSchema>;
export type HousingMortgageQuoteResponse = z.infer<typeof HousingMortgageQuoteResponseSchema>;
export type HousingPurchaseDraft = z.infer<typeof HousingPurchaseDraftSchema>;
export type HousingPurchaseRequest = z.infer<typeof HousingPurchaseRequestSchema>;
export type HousingMortgageExecution = z.infer<typeof HousingMortgageExecutionSchema>;
export type HousingPurchaseResult = z.infer<typeof HousingPurchaseResultSchema>;
export type HousingPurchaseResponse = z.infer<typeof HousingPurchaseResponseSchema>;
export type HousingPropertyHistoryQuery = z.infer<typeof HousingPropertyHistoryQuerySchema>;
export type HousingPropertySaleOrderStatus = z.infer<typeof HousingPropertySaleOrderStatusSchema>;
export type HousingPropertySaleOrderRevisionKind = z.infer<
  typeof HousingPropertySaleOrderRevisionKindSchema
>;
export type HousingPropertySaleOrderRejectionReason = z.infer<
  typeof HousingPropertySaleOrderRejectionReasonSchema
>;
export type HousingPropertySaleOrderCreateDraft = z.infer<
  typeof HousingPropertySaleOrderCreateDraftSchema
>;
export type HousingPropertySaleOrderCreateRequest = z.infer<
  typeof HousingPropertySaleOrderCreateRequestSchema
>;
export type HousingPropertySaleOrderRepriceDraft = z.infer<
  typeof HousingPropertySaleOrderRepriceDraftSchema
>;
export type HousingPropertySaleOrderRepriceRequest = z.infer<
  typeof HousingPropertySaleOrderRepriceRequestSchema
>;
export type HousingPropertySaleOrderCancelDraft = z.infer<
  typeof HousingPropertySaleOrderCancelDraftSchema
>;
export type HousingPropertySaleOrderCancelRequest = z.infer<
  typeof HousingPropertySaleOrderCancelRequestSchema
>;
export type HousingPropertySaleOrderListingResult = z.infer<
  typeof HousingPropertySaleOrderListingResultSchema
>;
export type HousingPropertySaleOrderCancellationResult = z.infer<
  typeof HousingPropertySaleOrderCancellationResultSchema
>;
export type HousingPropertySaleOrderListingResponse = z.infer<
  typeof HousingPropertySaleOrderListingResponseSchema
>;
export type HousingPropertySaleOrderCancellationResponse = z.infer<
  typeof HousingPropertySaleOrderCancellationResponseSchema
>;
export type HousingPropertySaleExecution = z.infer<typeof HousingPropertySaleExecutionSchema>;
export type HousingPropertySaleOrderSummary = z.infer<typeof HousingPropertySaleOrderSummarySchema>;
export type HousingPropertySaleOrdersResponse = z.infer<
  typeof HousingPropertySaleOrdersResponseSchema
>;
export type HousingPropertyTaxEventKind = z.infer<typeof HousingPropertyTaxEventKindSchema>;
export type HousingPropertyTaxEventStatus = z.infer<typeof HousingPropertyTaxEventStatusSchema>;
export type HousingPropertyTaxPaymentStatus = z.infer<typeof HousingPropertyTaxPaymentStatusSchema>;
export type HousingPropertyTaxComponent = z.infer<typeof HousingPropertyTaxComponentSchema>;
export type HousingPropertyTaxPayment = z.infer<typeof HousingPropertyTaxPaymentSchema>;
export type HousingPropertyTaxEvent = z.infer<typeof HousingPropertyTaxEventSchema>;
export type HousingPropertyTaxEventsResponse = z.infer<
  typeof HousingPropertyTaxEventsResponseSchema
>;
export type HousingLeaseArrearPaymentDraft = z.infer<typeof HousingLeaseArrearPaymentDraftSchema>;
export type HousingLeaseArrearPaymentRequest = z.infer<
  typeof HousingLeaseArrearPaymentRequestSchema
>;
export type HousingLeaseArrearPaymentResult = z.infer<typeof HousingLeaseArrearPaymentResultSchema>;
export type HousingLeaseArrearPaymentResponse = z.infer<
  typeof HousingLeaseArrearPaymentResponseSchema
>;
export type AdvanceRequest = z.infer<typeof AdvanceRequestSchema>;
export type AdvanceResult = z.infer<typeof AdvanceResultSchema>;
export type AdvanceResponse = z.infer<typeof AdvanceResponseSchema>;
export type GameCommandFailureCode = z.infer<typeof GameCommandFailureCodeSchema>;
export type GameCommandFailure = z.infer<typeof GameCommandFailureSchema>;
export type CareerFailureCode = z.infer<typeof CareerFailureCodeSchema>;
export type CareerFailure = z.infer<typeof CareerFailureSchema>;
export type CareerEvidence = z.infer<typeof CareerEvidenceSchema>;
export type CareerSpecsResponse = z.infer<typeof CareerSpecsResponseSchema>;
export type CareerActivityCatalogEntry = z.infer<typeof CareerActivityCatalogEntrySchema>;
export type CareerActivityHistoryItem = z.infer<typeof CareerActivityHistoryItemSchema>;
export type CareerActivitiesResponse = z.infer<typeof CareerActivitiesResponseSchema>;
export type CareerArtifact = z.infer<typeof CareerArtifactSchema>;
export type CareerArtifactsResponse = z.infer<typeof CareerArtifactsResponseSchema>;
export type CareerFocusDraft = z.infer<typeof CareerFocusDraftSchema>;
export type CareerFocusRequest = z.infer<typeof CareerFocusRequestSchema>;
export type CareerActivityStartDraft = z.infer<typeof CareerActivityStartDraftSchema>;
export type CareerActivityStartRequest = z.infer<typeof CareerActivityStartRequestSchema>;
export type CareerCursorRequest = z.infer<typeof CareerCursorRequestSchema>;
export type CareerArtifactDraft = z.infer<typeof CareerArtifactDraftSchema>;
export type CareerArtifactPublishRequest = z.infer<typeof CareerArtifactPublishRequestSchema>;
export type CareerFocusResult = z.infer<typeof CareerFocusResultSchema>;
export type CareerActivityResult = z.infer<typeof CareerActivityResultSchema>;
export type CareerArtifactResult = z.infer<typeof CareerArtifactResultSchema>;
export type CareerFocusResponse = z.infer<typeof CareerFocusResponseSchema>;
export type CareerActivityResponse = z.infer<typeof CareerActivityResponseSchema>;
export type CareerArtifactResponse = z.infer<typeof CareerArtifactResponseSchema>;
export type CareerPlatform = z.infer<typeof CareerPlatformSchema>;
export type PostingKey = z.infer<typeof PostingKeySchema>;
export type CareerCompetitionBand = z.infer<typeof CareerCompetitionBandSchema>;
export type CareerMilitaryRequirement = z.infer<typeof CareerMilitaryRequirementSchema>;
export type CareerEmploymentType = z.infer<typeof CareerEmploymentTypeSchema>;
export type CareerApplicationStatus = z.infer<typeof CareerApplicationStatusSchema>;
export type CareerInvitationStatus = z.infer<typeof CareerInvitationStatusSchema>;
export type EmploymentStatus = z.infer<typeof EmploymentStatusSchema>;
export type CareerJob = z.infer<typeof CareerJobSchema>;
export type CareerJobsResponse = z.infer<typeof CareerJobsResponseSchema>;
export type CareerApplication = z.infer<typeof CareerApplicationSchema>;
export type CareerInvitation = z.infer<typeof CareerInvitationSchema>;
export type CareerApplicationsResponse = z.infer<typeof CareerApplicationsResponseSchema>;
export type CareerEmploymentContract = z.infer<typeof CareerEmploymentContractSchema>;
export type CareerEmploymentResponse = z.infer<typeof CareerEmploymentResponseSchema>;
export type CareerPayrollReward = z.infer<typeof CareerPayrollRewardSchema>;
export type CareerPayrollItem = z.infer<typeof CareerPayrollItemSchema>;
export type CareerPayrollResponse = z.infer<typeof CareerPayrollResponseSchema>;
export type CareerTaxYearStatus = z.infer<typeof CareerTaxYearStatusSchema>;
export type CareerTaxYearSource = z.infer<typeof CareerTaxYearSourceSchema>;
export type CareerTaxYearState = z.infer<typeof CareerTaxYearStateSchema>;
export type CareerApplicationDraft = z.infer<typeof CareerApplicationDraftSchema>;
export type CareerApplicationRequest = z.infer<typeof CareerApplicationRequestSchema>;
export type CareerInterviewConfirmationRequest = z.infer<
  typeof CareerInterviewConfirmationRequestSchema
>;
export type CareerApplicationResult = z.infer<typeof CareerApplicationResultSchema>;
export type CareerInvitationResult = z.infer<typeof CareerInvitationResultSchema>;
export type CareerOfferResult = z.infer<typeof CareerOfferResultSchema>;
export type CareerApplicationResponse = z.infer<typeof CareerApplicationResponseSchema>;
export type CareerInvitationResponse = z.infer<typeof CareerInvitationResponseSchema>;
export type CareerOfferResponse = z.infer<typeof CareerOfferResponseSchema>;
export type CareerMilitaryStatus = z.infer<typeof CareerMilitaryStatusSchema>;
export type MilitaryServiceType = z.infer<typeof MilitaryServiceTypeSchema>;
export type MilitaryServiceStatus = z.infer<typeof MilitaryServiceStatusSchema>;
export type MilitaryServiceSourceKind = z.infer<typeof MilitaryServiceSourceKindSchema>;
export type MilitaryOption = z.infer<typeof MilitaryOptionSchema>;
export type MilitaryOptionsResponse = z.infer<typeof MilitaryOptionsResponseSchema>;
export type ActiveMilitaryServiceSummary = z.infer<typeof ActiveMilitaryServiceSummarySchema>;
export type MilitaryServiceHistory = z.infer<typeof MilitaryServiceHistorySchema>;
export type MilitaryServiceResponse = z.infer<typeof MilitaryServiceResponseSchema>;
export type MilitarySavingsProduct = z.infer<typeof MilitarySavingsProductSchema>;
export type MilitarySavingsProductsResponse = z.infer<typeof MilitarySavingsProductsResponseSchema>;
export type ActiveMilitarySavingsSummary = z.infer<typeof ActiveMilitarySavingsSummarySchema>;
export type MilitarySavingsInstallment = z.infer<typeof MilitarySavingsInstallmentSchema>;
export type MilitarySavingsMaturityProjection = z.infer<
  typeof MilitarySavingsMaturityProjectionSchema
>;
export type MilitarySavingsHistoryItem = z.infer<typeof MilitarySavingsHistoryItemSchema>;
export type MilitarySavingsHistoryResponse = z.infer<typeof MilitarySavingsHistoryResponseSchema>;
export type MilitaryServiceStartDraft = z.infer<typeof MilitaryServiceStartDraftSchema>;
export type MilitaryServiceStartRequest = z.infer<typeof MilitaryServiceStartRequestSchema>;
export type MilitarySavingsEnrollmentDraft = z.infer<typeof MilitarySavingsEnrollmentDraftSchema>;
export type MilitarySavingsEnrollmentRequest = z.infer<
  typeof MilitarySavingsEnrollmentRequestSchema
>;
export type MilitaryServiceResult = z.infer<typeof MilitaryServiceResultSchema>;
export type MilitarySavingsResult = z.infer<typeof MilitarySavingsResultSchema>;
export type MilitaryServiceCommandResponse = z.infer<typeof MilitaryServiceCommandResponseSchema>;
export type MilitarySavingsCommandResponse = z.infer<typeof MilitarySavingsCommandResponseSchema>;
export type ClockRequest = z.infer<typeof ClockRequestSchema>;
export type PortfolioOrderSide = z.infer<typeof PortfolioOrderSideSchema>;
export type PortfolioOrderDraft = z.infer<typeof PortfolioOrderDraftSchema>;
export type PortfolioOrderRequest = z.infer<typeof PortfolioOrderRequestSchema>;
export type PortfolioExecution = z.infer<typeof PortfolioExecutionSchema>;
export type PortfolioOrderResponse = z.infer<typeof PortfolioOrderResponseSchema>;
export type PortfolioOrderFailureCode = z.infer<typeof PortfolioOrderFailureCodeSchema>;
export type PortfolioOrderFailure = z.infer<typeof PortfolioOrderFailureSchema>;
export type MarketHistoryPoint = z.infer<typeof MarketHistoryPointSchema>;
export type MarketHistory = z.infer<typeof MarketHistorySchema>;
export type FinanceAccountsResponse = z.infer<typeof FinanceAccountsResponseSchema>;
export type FinanceCommandRequest = z.infer<typeof FinanceCommandRequestSchema>;
export type TransferDirection = z.infer<typeof TransferDirectionSchema>;
export type FinanceTransferDraft = z.infer<typeof FinanceTransferDraftSchema>;
export type FinanceTransferRequest = z.infer<typeof FinanceTransferRequestSchema>;
export type FinanceTransferResult = z.infer<typeof FinanceTransferResultSchema>;
export type FinanceTransferResponse = z.infer<typeof FinanceTransferResponseSchema>;
export type CmaAccountOpenDraft = z.infer<typeof CmaAccountOpenDraftSchema>;
export type CmaAccountCloseDraft = z.infer<typeof CmaAccountCloseDraftSchema>;
export type DepositOpenDraft = z.infer<typeof DepositOpenDraftSchema>;
export type DepositCloseDraft = z.infer<typeof DepositCloseDraftSchema>;
export type CmaAccountOpenRequest = z.infer<typeof CmaAccountOpenRequestSchema>;
export type CmaAccountOpenResult = z.infer<typeof CmaAccountOpenResultSchema>;
export type CmaAccountOpenResponse = z.infer<typeof CmaAccountOpenResponseSchema>;
export type CmaAccountCloseRequest = z.infer<typeof CmaAccountCloseRequestSchema>;
export type CmaAccountCloseResult = z.infer<typeof CmaAccountCloseResultSchema>;
export type CmaAccountCloseResponse = z.infer<typeof CmaAccountCloseResponseSchema>;
export type DepositOpenRequest = z.infer<typeof DepositOpenRequestSchema>;
export type DepositOpenResult = z.infer<typeof DepositOpenResultSchema>;
export type DepositOpenResponse = z.infer<typeof DepositOpenResponseSchema>;
export type DepositCloseRequest = z.infer<typeof DepositCloseRequestSchema>;
export type DepositCloseResult = z.infer<typeof DepositCloseResultSchema>;
export type DepositCloseResponse = z.infer<typeof DepositCloseResponseSchema>;
export type TaxAccountOpenType = z.infer<typeof TaxAccountOpenTypeSchema>;
export type TaxAccountOpenDraft = z.infer<typeof TaxAccountOpenDraftSchema>;
export type IsaAccountCloseDraft = z.infer<typeof IsaAccountCloseDraftSchema>;
export type PensionStartDraft = z.infer<typeof PensionStartDraftSchema>;
export type PensionWithdrawalRequestKind = z.infer<typeof PensionWithdrawalRequestKindSchema>;
export type IrpWithdrawalReason = z.infer<typeof IrpWithdrawalReasonSchema>;
export type PensionWithdrawalDraft = z.infer<typeof PensionWithdrawalDraftSchema>;
export type TaxAccountOpenRequest = z.infer<typeof TaxAccountOpenRequestSchema>;
export type TaxAccountOpenResult = z.infer<typeof TaxAccountOpenResultSchema>;
export type TaxAccountOpenResponse = z.infer<typeof TaxAccountOpenResponseSchema>;
export type IsaAccountCloseRequest = z.infer<typeof IsaAccountCloseRequestSchema>;
export type IsaAccountCloseResult = z.infer<typeof IsaAccountCloseResultSchema>;
export type IsaAccountCloseResponse = z.infer<typeof IsaAccountCloseResponseSchema>;
export type PensionStartRequest = z.infer<typeof PensionStartRequestSchema>;
export type PensionStartResult = z.infer<typeof PensionStartResultSchema>;
export type PensionStartResponse = z.infer<typeof PensionStartResponseSchema>;
export type PensionWithdrawalRequest = z.infer<typeof PensionWithdrawalRequestSchema>;
export type PensionWithdrawalResult = z.infer<typeof PensionWithdrawalResultSchema>;
export type PensionWithdrawalResponse = z.infer<typeof PensionWithdrawalResponseSchema>;
export type BondTermYears = z.infer<typeof BondTermYearsSchema>;
export type BondOrderSide = z.infer<typeof BondOrderSideSchema>;
export type BondProduct = z.infer<typeof BondProductSchema>;
export type BondSeries = z.infer<typeof BondSeriesSchema>;
export type BondProductCatalog = z.infer<typeof BondProductCatalogSchema>;
export type BondOrderRequest = z.infer<typeof BondOrderRequestSchema>;
export type BondOrderResult = z.infer<typeof BondOrderResultSchema>;
export type BondOrderResponse = z.infer<typeof BondOrderResponseSchema>;
export type GoldWithdrawalBar = z.infer<typeof GoldWithdrawalBarSchema>;
export type GoldProduct = z.infer<typeof GoldProductSchema>;
export type GoldProductCatalog = z.infer<typeof GoldProductCatalogSchema>;
export type GoldAccountOpenRequest = z.infer<typeof GoldAccountOpenRequestSchema>;
export type GoldAccountOpenResult = z.infer<typeof GoldAccountOpenResultSchema>;
export type GoldAccountOpenResponse = z.infer<typeof GoldAccountOpenResponseSchema>;
export type GoldOrderSide = z.infer<typeof GoldOrderSideSchema>;
export type GoldOrderRequest = z.infer<typeof GoldOrderRequestSchema>;
export type GoldOrderResult = z.infer<typeof GoldOrderResultSchema>;
export type GoldOrderResponse = z.infer<typeof GoldOrderResponseSchema>;
export type GoldWithdrawalRequest = z.infer<typeof GoldWithdrawalRequestSchema>;
export type GoldWithdrawalResult = z.infer<typeof GoldWithdrawalResultSchema>;
export type GoldWithdrawalResponse = z.infer<typeof GoldWithdrawalResponseSchema>;
export type FinanceFailureCode = z.infer<typeof FinanceFailureCodeSchema>;
export type FinanceFailure = z.infer<typeof FinanceFailureSchema>;
export type LedgerSourceKind = z.infer<typeof LedgerSourceKindSchema>;
export type LedgerAccountCode = z.infer<typeof LedgerAccountCodeSchema>;
export type LedgerPosting = z.infer<typeof LedgerPostingSchema>;
export type LedgerTransaction = z.infer<typeof LedgerTransactionSchema>;
export type LedgerPage = z.infer<typeof LedgerPageSchema>;
export type CharacterDraft = z.infer<typeof CharacterDraftSchema>;
export type CharacterStartProfile = z.infer<typeof CharacterStartProfileSchema>;
export type CharacterStartingLoan = z.infer<typeof CharacterStartingLoanSchema>;
export type CharacterStartV2Draft = z.infer<typeof CharacterStartV2DraftSchema>;
export type CharacterStartV1Request = z.infer<typeof CharacterStartV1RequestSchema>;
export type CharacterStartV2Request = z.infer<typeof CharacterStartV2RequestSchema>;
export type CharacterStartRequest = z.infer<typeof CharacterStartRequestSchema>;
export type CharacterStartResult = z.infer<typeof CharacterStartResultSchema>;
export type CharacterStartResponse = z.infer<typeof CharacterStartResponseSchema>;
export type Preset = z.infer<typeof PresetSchema>;
export type ValidationFailure = z.infer<typeof ValidationFailureSchema>;
export type ProviderKind = z.infer<typeof ProviderKindSchema>;
export type AuthProvider = z.infer<typeof AuthProviderSchema>;
export type Me = z.infer<typeof MeSchema>;

/** Step units. UI vocabulary rather than server contract, so they live here. */
export const STEP_DAYS = { day: 1, week: 7, month: 30 } as const;
export type StepUnit = keyof typeof STEP_DAYS;

/** Server-supported automatic speeds (§4.2). */
export const GAME_SPEEDS = [1, 2, 4, 8] as const satisfies readonly GameSpeed[];
