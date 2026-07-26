//! Run-scoped financial accounts, balanced ledger rules, and due settlements (§4–§5).

mod cash_products;
mod m2d;
mod rules;
mod settlement;
mod tax_accounts;
mod types;

pub use cash_products::{
    CASH_SETTLEMENT_PAYLOAD_VERSION, CashInterestTreatment, CashNoMovementReason,
    CashPayloadVersion, CashProductError, CashProductMutation, CashProductPolicy,
    CashProductTaxTreatment, CashSettlementExecution, CashSettlementExecutorRegistry,
    CashSettlementFollowUpDraft, CashSettlementKind, CashSettlementOutcome, CashSettlementPayload,
    CashSettlementPlanner, CashSettlementSource, CashSettlementSourceKind, CashSettlementTask,
    CmaDailyAccrual, CmaDailyAccrualInput, CmaDailyTerms, CmaInterestPayloadV1,
    DailyCashSettlementPlan, DailyCashSettlementPlanInput, DayCountBasis, DepositMaturityPayloadV1,
    DepositProtectionPolicy, DepositProtectionSummary, FinancialIncomeDelta,
    InstallmentSavingsContract, InstallmentSavingsPayout, InstallmentSavingsSchedule,
    InstallmentSavingsScheduleInput, InterestPayout, InterestTaxPolicy, PlannedCashSettlement,
    ProtectedDepositAmount, SavingsInstallmentCollection, SavingsInstallmentDue,
    SavingsInstallmentInterest, SavingsInstallmentPayloadV1, SavingsInstallmentPrincipal,
    SavingsMaturityPayloadV1, SettlementLedgerContext, TaxAdvantagedInterestDelta,
    TermDepositContract, WithholdingTax, accrue_cma_daily, aggregate_deposit_protection,
    calculate_cash_interest_treatment, calculate_interest_withholding,
    calculate_simple_interest_krw, cash_product_tax_treatment, collect_savings_installment,
    create_cash_settlement_planner, create_installment_savings_schedule,
    create_interest_payout_ledger, create_interest_payout_ledger_for_account,
    create_product_principal_funding_ledger, settle_installment_savings_early_close,
    settle_installment_savings_early_close_for_account, settle_installment_savings_maturity,
    settle_installment_savings_maturity_for_account, settle_term_deposit_early_close,
    settle_term_deposit_early_close_for_account, settle_term_deposit_maturity,
    settle_term_deposit_maturity_for_account,
};
pub use m2d::*;
pub use rules::create_finance_rules;
pub use settlement::create_settlement_rules;
#[cfg(test)]
pub use tax_accounts::create_tax_account_rules;
pub use tax_accounts::{
    GeneralFinancialIncomePolicy, IrpInvestmentKind, IrpRiskOrderDecision, IrpRiskOrderInput,
    IrpRiskOrderRejection, IrpWithdrawalReason, IsaAccountKind, IsaCloseTaxInput,
    IsaCloseTaxResult, IsaContributionRoom, IsaContributionRoomInput, IsaEligibility,
    IsaEnrollmentInput, IsaIneligibilityReason, IsaPolicy, IsaPriorIncomeComposition,
    IsaPriorTaxYearIncome, IsaTaxTreatment, PensionCreditIncome, PensionCreditInput,
    PensionCreditResult, PensionPolicy, PensionReceiptEligibility, PensionReceiptEligibilityInput,
    PensionReceiptIneligibilityReason, PensionReceiptLimit, PensionReceiptLimitInput,
    PensionTaxLayers, PensionTaxRate, PensionTaxSource, PensionWithdrawalPlan,
    PensionWithdrawalPlanInput, PensionWithdrawalPortion, PensionWithdrawalRequestKind,
    PensionWithdrawalTaxLine, PensionWithdrawalTreatment, TaxAccountError, TaxAccountPolicy,
    TaxAccountRules, anniversary_game_day, completed_calendar_years,
    create_tax_account_rules_with_policy, current_age_years,
};
pub use types::{
    CashProductCatalog, CashProductContractState, CashProductContractStatus, CashProductKind,
    CashProductVersion, CashRateReference, CloseCashProductCommand, CloseCashProductReceipt,
    CloseCmaAccountCommand, CloseCmaAccountReceipt, CmaAccountContractState, CommandCursor,
    CommandId, DailyInterestAccrual, DailyInterestInput, DepositProtectionState, FinanceCommand,
    FinanceCommandKind, FinanceFailureCode, FinanceRuleError, FinanceRules, FinancialAccount,
    FinancialAccountStatus, FinancialAccountType, FinancialIncomeYear, FinancialInstitution,
    LedgerAccountCode, LedgerPage, LedgerPosting, LedgerPostingRecord, LedgerRecord, LedgerSource,
    LedgerSourceKind, LedgerTransaction, LedgerTransactionDraft, OpenCashProductCommand,
    OpenCashProductReceipt, OpenCmaAccountCommand, OpenCmaAccountReceipt, PolicySet,
    PolicySetAssignment, PostingAccountRequirement, ResourceId, RunId, RunPolicyContext,
    SavingsInstallmentState, SavingsInstallmentStatus, ScheduledSettlement, SettlementKind,
    SettlementRules, SettlementSource, SettlementSourceKind, SettlementStatus, TransferCommand,
    TransferDirection, TransferInput, TransferMutation, TransferReceipt,
};
