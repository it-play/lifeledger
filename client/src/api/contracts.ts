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
      BigInt(snapshot.debtKrw);
    if (expectedNetWorthKrw !== BigInt(snapshot.netWorthKrw)) {
      context.addIssue({
        code: 'custom',
        path: ['netWorthKrw'],
        message: 'net worth must reconcile with all cash, assets, and debt',
      });
    }
  });

export const GameCommandCursorSchema = z.object({
  runRevision: z.number().int().nonnegative(),
  stateRevision: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
  gameDay: z.number().int().nonnegative(),
});

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
    region: z.string().min(1).max(64),
    employmentType: z.string().min(1).max(64),
    requiredScores: CareerScoresSchema,
    possessedScores: CareerScoresSchema,
    minimumAnnualSalaryKrw: z.number().int().safe().nonnegative(),
    maximumAnnualSalaryKrw: z.number().int().safe().nonnegative(),
    salaryStepKrw: z.number().int().safe().positive(),
    competitionBand: CareerCompetitionBandSchema,
    militaryRequirement: CareerMilitaryRequirementSchema,
    minimumEducation: z
      .enum(['highSchool', 'associate', 'bachelor', 'master', 'doctorate'])
      .nullable(),
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
      posting.accountCode === 'accountCash' || posting.accountCode === 'productPrincipal';
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
export const EducationSchema = z.enum([
  'highSchool',
  'associate',
  'bachelor',
  'master',
  'doctorate',
]);
export const RegionSchema = z.enum(['capitalArea', 'metropolitan', 'smallCity', 'rural']);
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

export const CharacterStartRequestSchema = z.object({
  commandId: CanonicalUuidSchema,
  expectedRunRevision: z.number().int().nonnegative(),
  expectedStateRevision: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
  expectedGameDay: z.number().int().nonnegative(),
  character: CharacterDraftSchema,
});

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
export type CareerSnapshot = z.infer<typeof CareerSnapshotSchema>;
export type GameSnapshot = z.infer<typeof GameSnapshotSchema>;
export type GameCommandCursor = z.infer<typeof GameCommandCursorSchema>;
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
