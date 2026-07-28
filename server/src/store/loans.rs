//! M4-B loan authority, servicing, and credit-state persistence.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{MySql, Transaction};
use time::{Date, Duration, Month};

use super::employment_income::read_verified_annual_income_in_tx;
use super::mysql::write_ledger_transaction;
use super::types::{
    CreateLeaseDepositLoanQuoteCommand, CreateLoanQuoteCommand, CreditOverviewState,
    CreditReasonState, DepositLoanExecutionReceipt, ExecuteLoanCommand,
    LeaseDepositLoanAffordabilityState, LeaseDepositLoanQuoteDecisionState,
    LeaseDepositLoanQuoteReasonState, LeaseDepositLoanQuoteReceipt, LifeFailureCode,
    LoanDetailState, LoanExecutionReceipt, LoanInstallmentPageCursor, LoanInstallmentPageQuery,
    LoanInstallmentPageState, LoanInstallmentState, LoanInstallmentStatusState,
    LoanPaymentAllocationKindState, LoanPaymentAllocationState, LoanPaymentKindState,
    LoanPaymentState, LoanPrepaymentNextInstallmentState, LoanPrepaymentReceipt,
    LoanPrepaymentStatusState, LoanProductCatalogState, LoanProductState, LoanQuoteDecisionState,
    LoanQuoteDsrState, LoanQuoteFirstInstallmentState, LoanQuoteReasonState, LoanQuoteReceipt,
    LoanQuotedTermsState, LoanSummaryState, MortgageExecutionReceipt, MortgageQuoteReasonState,
    NextLoanInstallmentState, PrepayLoanCommand, RepaidDepositLoanReceipt,
    VerifiedIncomeSourceState,
};
use crate::finance::{
    FinanceRules, LedgerAccountCode, LedgerPosting, LedgerSource, LedgerSourceKind,
    LedgerTransaction, LedgerTransactionDraft, ResourceId, RunId, RunPolicyContext,
    create_finance_rules,
};
use crate::life::{
    CreditBand, CreditBandThresholds, CreditDayEvent, CreditDayInput, CreditDefaultAssessmentInput,
    CreditDelinquencyBucket, CreditEventKind, CreditModelTerms, DsrAssessmentInput, DsrLoanInput,
    DsrPaymentTreatment, DsrPolicy, HousingLeaseOfferKind, LeaseDepositAffordabilityInput,
    LeaseDepositAffordabilityNewLoanInput, LeaseDepositFundingLimitInput, LoanContractStatus,
    LoanDayCountRule, LoanEndOfDayStatusInput, LoanLenderSector, LoanPaymentCalendar,
    LoanPrepaymentEffect, LoanPrepaymentInput, LoanPrepaymentScheduleCalculation,
    LoanPrepaymentScheduleInput, LoanPrepaymentSchedulePeriod, LoanProductKind,
    LoanProductProvenance, LoanRateReference, LoanRateReset, LoanRateResetRule, LoanRateStatus,
    LoanRateType, LoanRepaymentMethod, LoanScheduleCalculation, LoanScheduleInput,
    LoanSchedulePeriod, RepaymentAllocationInput, RepaymentBucketBalance, RepaymentBucketKind,
    create_credit_rules, create_loan_rules,
};

const LOAN_SETTLEMENT_PAYLOAD_VERSION: u8 = 1;
const ACTUAL_365_DAY_COUNT: u16 = 365;
const MAX_STARTING_LOANS: usize = 2;
const MAX_ACTIVE_LOANS: usize = 8;
const MAX_LOAN_HISTORY_PAGE_SIZE: usize = 50;
const LEASE_DEPOSIT_EXECUTION_CHANNEL: &str = "leaseMove";
const LEASE_DEPOSIT_AFFORDABILITY_RULE: &str = "interestOnly";
const LEASE_DEPOSIT_REGULATORY_DSR_TREATMENT: &str = "excludedNoOwnedHome";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LoanQuoteCreation {
    Applied(Box<LoanQuoteReceipt>),
    Rejected(LifeFailureCode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LeaseDepositLoanQuoteCreation {
    Applied(Box<LeaseDepositLoanQuoteReceipt>),
    Rejected(LifeFailureCode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LoanExecutionCreation {
    Applied {
        receipt: Box<LoanExecutionReceipt>,
        ledger_transaction_id: u64,
    },
    Rejected(LifeFailureCode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LoanPrepaymentCreation {
    Applied {
        receipt: Box<LoanPrepaymentReceipt>,
        ledger_transaction_id: u64,
    },
    Rejected(LifeFailureCode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "CharacterStart v2 is wired by the M4-B3 API slice"
)]
pub(super) enum StartingLoanOrigin {
    CharacterStartV2,
    LegacyV1Mapping,
}

impl StartingLoanOrigin {
    const fn as_str(self) -> &'static str {
        match self {
            Self::CharacterStartV2 => "characterStartV2",
            Self::LegacyV1Mapping => "legacyV1Mapping",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StartingLoanSelection {
    pub product_version_id: u64,
    pub product_kind: LoanProductKind,
    pub principal_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LoanRunInitialization {
    pub credit_model_version_id: u64,
    pub contract_ids: Vec<u64>,
    pub total_principal_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LoanInstallmentSettlement {
    pub contract_id: u64,
    pub installment_no: u16,
    pub paid_krw: i64,
    pub unpaid_krw: i64,
    pub wallet_cash_krw: i64,
    pub debt_krw: i64,
    pub ledger_transaction_id: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DebtProjection {
    pub loan_krw: i64,
    pub essential_arrear_krw: i64,
    pub lease_arrear_krw: i64,
    pub tax_obligation_krw: i64,
    pub total_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CreditDayApplication {
    pub units_before: i64,
    pub units_after: i64,
    pub band: CreditBand,
    pub transitioned_contracts: u32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LoanRunScopeRow {
    save_id: u64,
    market_world_id: u64,
    run_revision: u32,
    household_id: u64,
    credit_model_version_id: u64,
    credit_model_version_key: String,
    real_estate_model_version_id: u64,
    policy_set_id: u64,
    game_day: u32,
    wallet_cash_krw: i64,
    debt_krw: i64,
    legacy_debt_krw_at_activation: i64,
    world_start_date: Date,
    treasury_3m_bp: Option<i16>,
    model_parameters_json: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct StartingProductRow {
    id: u64,
    mapping_order: u8,
    legacy_field_key: String,
    product_kind: String,
    lender_sector: String,
    rate_status: String,
    rate_type: String,
    reference_rate_key: Option<String>,
    fixed_annual_rate_bp: Option<u16>,
    spread_bp: Option<i16>,
    minimum_annual_rate_bp: Option<u16>,
    maximum_annual_rate_bp: Option<u16>,
    rate_reset_rule: String,
    day_count_rule: String,
    repayment_method: String,
    term_months: Option<u16>,
    payment_calendar: String,
    grace_months: Option<u16>,
    minimum_principal_krw: Option<i64>,
    maximum_principal_krw: Option<i64>,
    prepayment_fee_ppm: Option<u32>,
    prepayment_effect: String,
    starting_eligible: bool,
    dsr_included: bool,
    read_only: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LoanProductCatalogContextRow {
    credit_model_version_id: Option<u64>,
    market_world_id: Option<u64>,
    game_day: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LoanProductCatalogRow {
    id: u64,
    product_key: String,
    display_name: String,
    product_kind: String,
    lender_sector: String,
    rate_status: String,
    rate_type: String,
    reference_rate_key: Option<String>,
    fixed_annual_rate_bp: Option<u16>,
    spread_bp: Option<i16>,
    minimum_annual_rate_bp: Option<u16>,
    maximum_annual_rate_bp: Option<u16>,
    rate_reset_rule: String,
    day_count_rule: String,
    repayment_method: String,
    term_months: Option<u16>,
    payment_calendar: String,
    grace_months: Option<u16>,
    minimum_principal_krw: Option<i64>,
    maximum_principal_krw: Option<i64>,
    prepayment_fee_ppm: Option<u32>,
    prepayment_effect: String,
    starting_eligible: bool,
    quote_eligible: bool,
    execution_eligible: bool,
    prepayment_allowed: bool,
    dsr_included: bool,
    provenance_kind: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct QuotePolicyRow {
    rule_key: String,
    parameters_json: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredBorrowerDsrLimits {
    schema_version: u8,
    annual_income_status_required: String,
    application_balance_boundary: String,
    application_balance_threshold_krw: i64,
    bank_limit_ppm: i64,
    evaluation_horizon_months: u8,
    non_bank_limit_ppm: i64,
    ratio_scale_ppm: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredOtherLoanDsrInclusion {
    schema_version: u8,
    bullet_amortization_months: u16,
    included_product_kinds: Vec<String>,
    scheduled_loan_measure: String,
    student_loan_classification: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredUnsecuredStressDsr {
    schema_version: u8,
    application_balance_boundary: String,
    application_balance_threshold_krw: i64,
    fixed_at_least_five_years_application_ppm: i64,
    fixed_at_least_three_years_application_ppm: i64,
    other_fixed_or_variable_application_ppm: i64,
    stress_rate_bp: i64,
}

#[derive(Debug, Clone, Copy)]
struct LoadedDsrPolicy {
    general_loan_balance_gate_krw: i64,
    bank_limit_ppm: i64,
    non_bank_limit_ppm: i64,
    credit_balance_stress_gate_krw: i64,
    base_stress_rate_bp: i64,
    medium_fixed_stress_multiplier_ppm: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct QuoteLoanRow {
    id: u64,
    status: String,
    product_kind: String,
    rate_type: String,
    current_annual_rate_bp: Option<u16>,
    repayment_method: String,
    term_months: Option<u16>,
    day_count_denominator: Option<u16>,
    remaining_principal_krw: i64,
    interest_remainder_numerator: String,
    dsr_included: bool,
    read_only: bool,
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
struct QuoteInstallmentRow {
    installment_no: u16,
    due_game_day: u32,
    elapsed_days: u16,
    annual_rate_bp: u16,
}

#[derive(Debug, Clone)]
struct OwnedDsrLoan {
    loan_id: u64,
    included_in_dsr: bool,
    counts_toward_general_loan_balance: bool,
    counts_toward_credit_stress_balance: bool,
    rate_type: LoanRateType,
    fixed_rate_period_months: u16,
    payment_treatment: DsrPaymentTreatment,
    principal_krw: i64,
    initial_annual_rate_bp: i64,
    repayment_method: LoanRepaymentMethod,
    prior_interest_remainder_numerator: i128,
    periods: Vec<LoanSchedulePeriod>,
    rate_resets: Vec<LoanRateReset>,
}

#[derive(Debug, Clone, Copy)]
struct ResolvedProductTerms {
    repayment_method: LoanRepaymentMethod,
    initial_annual_rate_bp: i64,
    term_months: u16,
}

#[derive(Debug, Clone, Copy)]
struct StartingContractDraft<'a> {
    scope: &'a LoanRunScopeRow,
    product: &'a StartingProductRow,
    terms: ResolvedProductTerms,
    origin_command_id: &'a str,
    origin: StartingLoanOrigin,
    principal_krw: i64,
    maturity_game_day: u32,
}

#[derive(Debug, Clone, Copy)]
struct ExecutedContractDraft<'a> {
    scope: &'a LoanRunScopeRow,
    product: &'a LoanProductState,
    quote_id: ResourceId,
    origin_command_id: &'a str,
    principal_krw: i64,
    maturity_game_day: u32,
}

#[derive(Debug, Clone, Copy)]
struct StoredPeriod {
    start_game_day: u32,
    end_game_day: u32,
    calculation: LoanSchedulePeriod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LoanInstallmentSettlementPayload {
    version: u8,
    loan_contract_id: String,
    installment_no: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredCreditModelParameters {
    schema_version: u8,
    credit_units: StoredCreditUnits,
    bands: Vec<StoredCreditBand>,
    event_penalty: StoredEventPenalty,
    daily_change: StoredDailyChange,
    default_rule: StoredDefaultRule,
    loan_eligibility: Option<StoredLoanEligibility>,
    lease_deposit_affordability: Option<StoredLeaseDepositAffordability>,
    provenance: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredLoanEligibility {
    unsecured_loan: StoredUnsecuredLoanEligibility,
    lease_deposit_loan: Option<StoredLeaseDepositLoanEligibility>,
    mortgage: Option<StoredMortgageLoanEligibility>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredUnsecuredLoanEligibility {
    allowed_credit_bands: Vec<String>,
    disallowed_contract_statuses: Vec<String>,
    maximum_active_contracts: u8,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredLeaseDepositLoanEligibility {
    allowed_credit_bands: Vec<String>,
    disallowed_contract_statuses: Vec<String>,
    maximum_active_contracts: u8,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredMortgageLoanEligibility {
    allowed_credit_bands: Vec<String>,
    disallowed_contract_statuses: Vec<String>,
    maximum_active_contracts: u8,
    maximum_active_holdings: u8,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredLeaseDepositAffordability {
    maximum_ratio_ppm: i64,
    new_loan_treatment: String,
    replacement_loan_treatment: String,
}

#[derive(Debug, Clone)]
struct QuoteEligibility {
    allowed_credit_bands: Vec<CreditBand>,
    maximum_active_contracts: usize,
}

#[derive(Debug, Clone)]
struct LeaseDepositQuoteEligibility {
    allowed_credit_bands: Vec<CreditBand>,
    maximum_active_contracts: usize,
    maximum_affordability_ratio_ppm: i64,
}

#[derive(Debug, Clone)]
struct MortgageQuoteEligibility {
    allowed_credit_bands: Vec<CreditBand>,
    maximum_active_contracts: usize,
    maximum_active_holdings: usize,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LeaseDepositProductPolicyRow {
    execution_channel: String,
    funding_limit_ppm: Option<u32>,
    affordability_rule: Option<String>,
    affordability_limit_ppm: Option<u32>,
    regulatory_dsr_treatment: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LeaseDepositListingRow {
    id: u64,
    market_world_id: u64,
    real_estate_model_version_id: u64,
    year_month: Date,
    available_from_game_day: u32,
    available_to_game_day: u32,
    offer_kind: String,
    deposit_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LinkedDepositLoanRow {
    lease_contract_id: u64,
    lease_deposit_krw: i64,
    loan_contract_id: Option<u64>,
    product_kind: Option<String>,
    status: Option<String>,
    remaining_principal_krw: Option<i64>,
    accrued_interest_krw: Option<i64>,
    accrued_fee_krw: Option<i64>,
    overdue_krw: Option<i64>,
}

#[derive(Debug, Clone)]
struct LeaseDepositLoanApplicationAssessment {
    product: LoanProductState,
    periods: Vec<StoredPeriod>,
    schedule: LoanScheduleCalculation,
    listing_id: ResourceId,
    deposit_krw: i64,
    funding_limit_ppm: i64,
    maximum_funding_krw: i64,
    decision_code: LeaseDepositLoanQuoteDecisionState,
    decision_reasons: Vec<LeaseDepositLoanQuoteReasonState>,
    verified_annual_income_krw: Option<i64>,
    verified_income_source: Option<super::types::VerifiedIncomeSourceState>,
    existing_loan_balance_krw: i64,
    post_execution_balance_krw: i64,
    affordability: Option<LeaseDepositLoanAffordabilityState>,
    quoted_terms: LoanQuotedTermsState,
    replaced_loan_id: Option<ResourceId>,
    replaced_loan_principal_krw: i64,
}

#[derive(Debug, Clone)]
pub(super) struct MortgageLoanAssessment {
    scope: LoanRunScopeRow,
    pub product: LoanProductState,
    periods: Vec<StoredPeriod>,
    schedule: LoanScheduleCalculation,
    pub credit_reasons: Vec<MortgageQuoteReasonState>,
    pub maximum_active_holdings: usize,
    pub verified_annual_income_krw: Option<i64>,
    pub verified_income_source: Option<VerifiedIncomeSourceState>,
    pub existing_loan_balance_krw: i64,
    pub post_execution_balance_krw: i64,
    pub dsr_applied: bool,
    pub dsr: Option<LoanQuoteDsrState>,
    pub stress_rate_bp: i64,
    pub quoted_terms: LoanQuotedTermsState,
}

#[derive(Debug, Clone)]
pub(super) enum MortgageLoanAssessmentResult {
    Assessed(Box<MortgageLoanAssessment>),
    Rejected(LifeFailureCode),
}

#[derive(Debug)]
pub(super) struct PreparedLeaseDepositLoanExecution {
    scope: LoanRunScopeRow,
    quote_id: ResourceId,
    principal_krw: i64,
    assessment: Box<LeaseDepositLoanApplicationAssessment>,
}

impl PreparedLeaseDepositLoanExecution {
    pub(super) const fn principal_krw(&self) -> i64 {
        self.principal_krw
    }

    pub(super) const fn replaced_loan_id(&self) -> Option<ResourceId> {
        self.assessment.replaced_loan_id
    }
}

#[derive(Debug)]
pub(super) enum LeaseDepositLoanExecutionPreparation {
    Prepared(Box<PreparedLeaseDepositLoanExecution>),
    Rejected(LifeFailureCode),
}

#[derive(Debug)]
pub(super) struct PreparedLeaseMovePayoff {
    contract: PrepaymentContractRow,
    installments: Vec<PendingInstallmentRow>,
}

impl PreparedLeaseMovePayoff {
    pub(super) const fn loan_id(&self) -> ResourceId {
        ResourceId::from_u64(self.contract.id)
    }

    pub(super) const fn principal_krw(&self) -> i64 {
        self.contract.remaining_principal_krw
    }
}

#[derive(Debug)]
pub(super) enum LeaseMovePayoffPreparation {
    None,
    Prepared(Box<PreparedLeaseMovePayoff>),
    Rejected(LifeFailureCode),
}

#[derive(Debug)]
pub(super) struct PreparedPropertySalePayoff {
    contract: PrepaymentContractRow,
    installments: Vec<PendingInstallmentRow>,
    fee_krw: i64,
}

impl PreparedPropertySalePayoff {
    pub(super) const fn loan_id(&self) -> ResourceId {
        ResourceId::from_u64(self.contract.id)
    }

    pub(super) const fn principal_krw(&self) -> i64 {
        self.contract.remaining_principal_krw
    }

    pub(super) const fn fee_krw(&self) -> i64 {
        self.fee_krw
    }
}

#[derive(Debug)]
pub(super) enum PropertySalePayoffPreparation {
    None,
    Prepared(Box<PreparedPropertySalePayoff>),
    MortgageNotPayable,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PropertySalePayoffApplication {
    pub loan_id: ResourceId,
    pub payment_id: ResourceId,
    pub principal_krw: i64,
    pub fee_krw: i64,
}

#[derive(Debug, Clone)]
enum LeaseDepositLoanAssessmentResult {
    Assessed(Box<LeaseDepositLoanApplicationAssessment>),
    Rejected(LifeFailureCode),
}

#[derive(Debug, Clone, Copy)]
enum QuoteReplacementResolution {
    None,
    Active {
        loan_id: ResourceId,
        principal_krw: i64,
    },
    ContractConflict,
}

#[derive(Debug, Clone)]
struct LoanApplicationAssessment {
    product: LoanProductState,
    periods: Vec<StoredPeriod>,
    schedule: LoanScheduleCalculation,
    decision_code: LoanQuoteDecisionState,
    decision_reasons: Vec<LoanQuoteReasonState>,
    quoted_terms: LoanQuotedTermsState,
}

#[derive(Debug, Clone)]
enum LoanApplicationAssessmentResult {
    Assessed(Box<LoanApplicationAssessment>),
    Rejected(LifeFailureCode),
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ExecutableQuoteRow {
    purpose: String,
    loan_product_version_id: u64,
    command_id: String,
    payload_sha256: String,
    expected_state_revision: u64,
    requested_principal_krw: i64,
    created_game_day: u32,
    expires_game_day: u32,
    decision_code: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ExecutableLeaseDepositQuoteRow {
    loan_product_version_id: u64,
    command_id: String,
    payload_sha256: String,
    expected_state_revision: u64,
    requested_principal_krw: i64,
    created_game_day: u32,
    expires_game_day: u32,
    decision_code: String,
    decision_reasons_json: String,
    quoted_terms_json: String,
    property_listing_id: Option<u64>,
    lease_deposit_krw: Option<i64>,
    funding_limit_ppm: Option<u32>,
    maximum_funding_krw: Option<i64>,
    replaced_loan_contract_id: Option<u64>,
    replaced_loan_principal_krw: i64,
    regulatory_dsr_applied: Option<bool>,
    verified_annual_income_krw: Option<i64>,
    verified_income_source: Option<String>,
    existing_loan_balance_krw: i64,
    post_execution_balance_krw: i64,
    affordability_numerator_krw: Option<i64>,
    affordability_denominator_krw: Option<i64>,
    affordability_ratio_ppm: Option<i64>,
    affordability_limit_ppm: Option<u32>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredCreditUnits {
    minimum: i64,
    maximum: i64,
    initial: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredCreditBand {
    band: String,
    minimum_units: i64,
    maximum_units: i64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredEventPenalty {
    active_to_delinquent_units: i64,
    delinquent_to_defaulted_units: i64,
    legal_procedure_units: i64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredDailyChange {
    delinquent_or_defaulted_penalty_units: i64,
    clean_recovery_units: i64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredDefaultRule {
    absolute_oldest_bucket_days: u32,
    amount_and_age_minimum_krw: i64,
    amount_and_age_oldest_bucket_days: u32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct SettlementEnvelopeRow {
    due_game_day: u32,
    payload_json: String,
    source_id: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedContractRow {
    id: u64,
    household_id: u64,
    policy_set_id: u64,
    status: String,
    read_only: bool,
    remaining_principal_krw: i64,
    accrued_interest_krw: i64,
    accrued_fee_krw: i64,
    interest_remainder_numerator: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedInstallmentRow {
    id: u64,
    installment_no: u16,
    due_game_day: u32,
    scheduled_fee_krw: i64,
    scheduled_interest_krw: i64,
    scheduled_principal_krw: i64,
    interest_remainder_after: String,
    paid_fee_krw: i64,
    paid_interest_krw: i64,
    paid_principal_krw: i64,
    status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct StoredBucketRow {
    id: u64,
    loan_installment_id: u64,
    bucket_kind: String,
    due_game_day: u32,
    original_amount_krw: i64,
    paid_amount_krw: i64,
    status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct VariableContractRow {
    id: u64,
    reference_rate_key: String,
    applied_spread_bp: i16,
    minimum_annual_rate_bp: u16,
    maximum_annual_rate_bp: u16,
    current_annual_rate_bp: u16,
    day_count_denominator: u16,
    repayment_method: String,
    remaining_principal_krw: i64,
    interest_remainder_numerator: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PendingInstallmentRow {
    id: u64,
    installment_no: u16,
    due_game_day: u32,
    interest_period_start_game_day: u32,
    interest_period_end_game_day: u32,
    elapsed_days: u16,
    annual_rate_bp: u16,
    opening_principal_krw: i64,
    scheduled_fee_krw: i64,
    scheduled_interest_krw: i64,
    scheduled_principal_krw: i64,
    interest_remainder_before: String,
    interest_remainder_after: String,
    schedule_revision: u32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PrepaymentContractRow {
    id: u64,
    status: String,
    read_only: bool,
    remaining_principal_krw: i64,
    accrued_interest_krw: i64,
    accrued_fee_krw: i64,
    interest_remainder_numerator: String,
    prepayment_fee_ppm: Option<u32>,
    prepayment_effect: String,
    current_annual_rate_bp: Option<u16>,
    day_count_denominator: Option<u16>,
    repayment_method: String,
    next_installment_no: Option<u16>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CreditStateRow {
    household_id: u64,
    credit_model_version_id: u64,
    credit_units: u16,
    credit_band: String,
    save_game_day: u32,
    last_evaluated_game_day: u32,
    evaluation_revision: u64,
    model_parameters_json: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CreditContractRow {
    id: u64,
    status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LoanSummaryRow {
    id: u64,
    product_version_id: u64,
    product_kind: String,
    display_name: String,
    rate_status: String,
    current_annual_rate_bp: Option<u16>,
    status: String,
    remaining_principal_krw: i64,
    overdue_krw: i64,
    read_only: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LoanDetailRow {
    id: u64,
    product_version_id: u64,
    product_kind: String,
    display_name: String,
    rate_status: String,
    current_annual_rate_bp: Option<u16>,
    status: String,
    read_only: bool,
    original_principal_krw: i64,
    remaining_principal_krw: i64,
    accrued_interest_krw: i64,
    accrued_fee_krw: i64,
    overdue_krw: i64,
    repayment_method: String,
    term_months: Option<u16>,
    total_installments: Option<u16>,
    activated_game_day: u32,
    maturity_game_day: Option<u32>,
    final_installment_due_game_day: Option<u32>,
    next_installment_no: Option<u16>,
    oldest_unpaid_due_game_day: Option<u32>,
    product_prepayment_allowed: bool,
    prepayment_fee_ppm: Option<u32>,
    prepayment_effect: String,
    dsr_included: bool,
    lease_contract_id: Option<u64>,
    property_holding_id: Option<u64>,
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
struct OwnedLoanScopeRow {
    save_id: u64,
    run_revision: u32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LoanInstallmentHistoryRow {
    id: u64,
    installment_no: u16,
    due_game_day: u32,
    interest_period_start_game_day: u32,
    elapsed_days: u16,
    annual_rate_bp: u16,
    opening_principal_krw: i64,
    scheduled_fee_krw: i64,
    scheduled_interest_krw: i64,
    scheduled_principal_krw: i64,
    paid_fee_krw: i64,
    paid_interest_krw: i64,
    paid_principal_krw: i64,
    status: String,
    schedule_revision: u32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LoanPaymentHistoryRow {
    id: u64,
    payment_no: u32,
    payment_kind: String,
    amount_krw: i64,
    game_day: u32,
    status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LoanPaymentAllocationHistoryRow {
    loan_payment_id: u64,
    payment_no: u32,
    allocation_order: u16,
    allocation_kind: String,
    amount_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct NextInstallmentRow {
    loan_id: u64,
    installment_no: u16,
    due_game_day: u32,
    fee_krw: i64,
    interest_krw: i64,
    principal_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LoanPostingReference {
    None,
    Contract(u64),
}

pub(super) async fn initialize_legacy_starting_loans_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    origin_command_id: &str,
    student_loan_krw: i64,
    credit_loan_krw: i64,
) -> Result<LoanRunInitialization> {
    ensure!(
        student_loan_krw >= 0 && credit_loan_krw >= 0,
        "legacy starting-loan amounts cannot be negative"
    );
    let scope = read_loan_run_scope(tx, save_id, run_revision).await?;
    let products = read_legacy_start_products(tx, scope.credit_model_version_id).await?;
    ensure!(
        products.len() == MAX_STARTING_LOANS,
        "active credit model has no exact legacy start mapping"
    );
    ensure!(
        products[0].mapping_order == 1
            && products[0].legacy_field_key == "studentLoanKrw"
            && products[0].product_kind == "studentLoan"
            && products[1].mapping_order == 2
            && products[1].legacy_field_key == "creditLoanKrw"
            && products[1].product_kind == "unsecuredLoan",
        "active credit model legacy start mapping is not canonical"
    );

    let mut selections = Vec::with_capacity(MAX_STARTING_LOANS);
    for product in &products {
        let principal_krw = match product.legacy_field_key.as_str() {
            "studentLoanKrw" => student_loan_krw,
            "creditLoanKrw" => credit_loan_krw,
            _ => bail!("active credit model contains an unknown legacy start mapping"),
        };
        if principal_krw > 0 {
            selections.push(StartingLoanSelection {
                product_version_id: product.id,
                product_kind: parse_product_kind(&product.product_kind)?,
                principal_krw,
            });
        }
    }

    initialize_starting_loans_with_scope(
        tx,
        &scope,
        origin_command_id,
        StartingLoanOrigin::LegacyV1Mapping,
        &selections,
        &products,
    )
    .await
}

#[allow(
    dead_code,
    reason = "CharacterStart v2 is wired by the M4-B3 API slice"
)]
pub(super) async fn initialize_starting_loans_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    origin_command_id: &str,
    selections: &[StartingLoanSelection],
) -> Result<LoanRunInitialization> {
    let scope = read_loan_run_scope(tx, save_id, run_revision).await?;
    let products = read_legacy_start_products(tx, scope.credit_model_version_id).await?;
    initialize_starting_loans_with_scope(
        tx,
        &scope,
        origin_command_id,
        StartingLoanOrigin::CharacterStartV2,
        selections,
        &products,
    )
    .await
}

async fn initialize_starting_loans_with_scope(
    tx: &mut Transaction<'_, MySql>,
    scope: &LoanRunScopeRow,
    origin_command_id: &str,
    origin: StartingLoanOrigin,
    selections: &[StartingLoanSelection],
    products: &[StartingProductRow],
) -> Result<LoanRunInitialization> {
    ensure!(
        !origin_command_id.is_empty(),
        "starting loans require their start command identity"
    );
    ensure!(
        selections.len() <= MAX_STARTING_LOANS,
        "too many starting loans"
    );
    ensure!(
        scope.game_day == 0,
        "starting loans must initialize on game day zero"
    );
    ensure!(
        scope.wallet_cash_krw >= 0,
        "starting-loan run has a negative wallet balance"
    );
    ensure!(
        scope.world_start_date.month() == Month::January && scope.world_start_date.day() == 1,
        "M4-B loan calendars require a January 1 world start"
    );

    let model = parse_credit_model(&scope.model_parameters_json)?;
    let credit_rules = create_credit_rules();
    let starting_units = credit_rules
        .starting_units(model)
        .context("active credit model has invalid starting units")?;
    let starting_band = credit_rules
        .band(model, starting_units)
        .context("active credit model has invalid starting band")?;

    let mut selected_product_ids = BTreeSet::new();
    let mut selected_kinds = BTreeSet::new();
    let mut total_principal_krw = 0_i64;
    for selection in selections {
        ensure!(
            selection.principal_krw > 0,
            "starting-loan principal must be positive"
        );
        ensure!(
            selected_product_ids.insert(selection.product_version_id),
            "starting-loan product is duplicated"
        );
        ensure!(
            selected_kinds.insert(selection.product_kind),
            "starting-loan kind is duplicated"
        );
        total_principal_krw = total_principal_krw
            .checked_add(selection.principal_krw)
            .context("starting-loan principal total overflowed")?;
    }
    ensure!(
        total_principal_krw == scope.legacy_debt_krw_at_activation
            && total_principal_krw == scope.debt_krw,
        "starting loans disagree with the opening debt authority"
    );

    insert_initial_credit_state(tx, scope, starting_units, starting_band).await?;

    let loan_rules = create_loan_rules();
    let finance_rules = create_finance_rules();
    let mut contract_ids = Vec::with_capacity(selections.len());
    for selection in selections {
        let product = products
            .iter()
            .find(|product| product.id == selection.product_version_id)
            .context("starting-loan product is not a mapped child of the active model")?;
        ensure!(
            parse_product_kind(&product.product_kind)? == selection.product_kind,
            "starting-loan product kind disagrees with its selection"
        );
        let terms = validate_starting_product(product, selection.principal_krw, scope)?;
        let periods =
            build_month_end_periods(scope.world_start_date, scope.game_day, terms.term_months)?;
        let calculation_periods = periods
            .iter()
            .map(|period| period.calculation)
            .collect::<Vec<_>>();
        let schedule = loan_rules
            .build_schedule(LoanScheduleInput {
                principal_krw: selection.principal_krw,
                initial_annual_rate_bp: terms.initial_annual_rate_bp,
                day_count: ACTUAL_365_DAY_COUNT,
                repayment_method: terms.repayment_method,
                prior_interest_remainder_numerator: 0,
                periods: &calculation_periods,
                rate_resets: &[],
            })
            .context("starting-loan schedule is invalid")?;
        let maturity_game_day = periods
            .last()
            .map(|period| period.end_game_day)
            .context("starting-loan schedule has no maturity")?;
        let contract_id = insert_starting_contract(
            tx,
            StartingContractDraft {
                scope,
                product,
                terms,
                origin_command_id,
                origin,
                principal_krw: selection.principal_krw,
                maturity_game_day,
            },
        )
        .await?;
        insert_schedule_and_settlements(tx, scope, contract_id, &periods, &schedule.installments)
            .await?;

        let ledger = finance_rules.create_ledger_transaction(LedgerTransactionDraft {
            policy: RunPolicyContext {
                run: RunId {
                    save_id: ResourceId::from_u64(scope_id(scope)?),
                    run_revision: scope_run_revision(scope)?,
                },
                policy_set_id: ResourceId::from_u64(scope.policy_set_id),
            },
            source: LedgerSource {
                kind: LedgerSourceKind::LoanOrigination,
                source_id: contract_id.to_string(),
            },
            game_day: scope.game_day,
            description: "대출 원금 권위 전환".to_owned(),
            postings: vec![
                LedgerPosting {
                    account_code: LedgerAccountCode::DebtPrincipal,
                    financial_account_id: None,
                    amount_krw: selection.principal_krw,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::LoanPrincipalLiability,
                    financial_account_id: None,
                    amount_krw: selection
                        .principal_krw
                        .checked_neg()
                        .context("starting-loan principal cannot be negated")?,
                },
            ],
        })?;
        write_loan_ledger_transaction(
            tx,
            &ledger,
            &[
                LoanPostingReference::None,
                LoanPostingReference::Contract(contract_id),
            ],
        )
        .await?;
        contract_ids.push(contract_id);
    }

    let projection =
        calculate_debt_projection_in_tx(tx, scope_id(scope)?, scope_run_revision(scope)?).await?;
    ensure!(
        projection.total_krw == scope.debt_krw,
        "initialized loan authority disagrees with save debt projection"
    );

    Ok(LoanRunInitialization {
        credit_model_version_id: scope.credit_model_version_id,
        contract_ids,
        total_principal_krw,
    })
}

async fn read_loan_run_scope(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<LoanRunScopeRow> {
    sqlx::query_as(
        "SELECT save.id AS save_id, save.market_world_id, save.run_revision,
                household.id AS household_id,
                bundle.credit_model_version_id,
                model.version_key AS credit_model_version_key,
                bundle.real_estate_model_version_id,
                save.policy_set_id, save.game_day,
                save.cash_krw AS wallet_cash_krw, save.debt_krw,
                household.legacy_debt_krw_at_activation, world.start_date AS world_start_date,
                daily.treasury_3m_bp,
                CAST(model.parameters AS CHAR CHARACTER SET utf8mb4) AS model_parameters_json
         FROM save
         INNER JOIN run_rule_bundle AS bundle
           ON bundle.save_id = save.id AND bundle.run_revision = save.run_revision
         INNER JOIN household
           ON household.save_id = save.id AND household.run_revision = save.run_revision
         INNER JOIN credit_model_version AS model
           ON model.id = bundle.credit_model_version_id
          AND model.availability = 'active' AND model.sealed_at IS NOT NULL
         INNER JOIN credit_model_strict_manifest AS manifest
           ON manifest.credit_model_version_id = model.id
          AND manifest.canonical_sha256 = model.canonical_sha256
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         LEFT JOIN market_daily AS daily
           ON daily.world_id = world.id AND daily.game_day = save.game_day
         WHERE save.id = ? AND save.run_revision = ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await
    .context("run has no active sealed credit model")
}

async fn read_legacy_start_products(
    tx: &mut Transaction<'_, MySql>,
    credit_model_version_id: u64,
) -> Result<Vec<StartingProductRow>> {
    let rows = sqlx::query_as(
        "SELECT product.id, mapping.mapping_order, mapping.legacy_field_key,
                product.product_kind, product.lender_sector, product.rate_status,
                product.rate_type, product.reference_rate_key, product.fixed_annual_rate_bp,
                product.spread_bp, product.minimum_annual_rate_bp,
                product.maximum_annual_rate_bp, product.rate_reset_rule,
                product.day_count_rule, product.repayment_method, product.term_months,
                product.payment_calendar, product.grace_months,
                product.minimum_principal_krw, product.maximum_principal_krw,
                product.prepayment_fee_ppm, product.prepayment_effect,
                product.starting_eligible, product.dsr_included, product.read_only
         FROM loan_product_legacy_start_mapping AS mapping
         INNER JOIN loan_product_version AS product
           ON product.id = mapping.loan_product_version_id
          AND product.credit_model_version_id = mapping.credit_model_version_id
          AND product.catalog_scope = 'modelChild'
          AND product.sealed_at IS NOT NULL
         WHERE mapping.credit_model_version_id = ?
         ORDER BY mapping.mapping_order, product.id",
    )
    .bind(credit_model_version_id)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows)
}

fn validate_starting_product(
    product: &StartingProductRow,
    principal_krw: i64,
    scope: &LoanRunScopeRow,
) -> Result<ResolvedProductTerms> {
    ensure!(
        product.starting_eligible && !product.read_only && product.rate_status == "available",
        "loan product is not eligible at character start"
    );
    ensure!(
        product.day_count_rule == "actual365"
            && product.payment_calendar == "monthEnd"
            && product.grace_months == Some(0),
        "starting-loan servicing terms are unsupported"
    );
    let minimum_principal_krw = product
        .minimum_principal_krw
        .context("starting-loan minimum principal is missing")?;
    let maximum_principal_krw = product
        .maximum_principal_krw
        .context("starting-loan maximum principal is missing")?;
    ensure!(
        (minimum_principal_krw..=maximum_principal_krw).contains(&principal_krw),
        "starting-loan principal is outside product bounds"
    );
    ensure!(
        product.prepayment_fee_ppm.is_some()
            && product.prepayment_effect != "forbidden"
            && product.dsr_included,
        "starting-loan product is missing servicing terms"
    );
    let product_kind = parse_product_kind(&product.product_kind)?;
    ensure!(
        matches!(
            product_kind,
            LoanProductKind::StudentLoan | LoanProductKind::UnsecuredLoan
        ),
        "M4-B cannot initialize this loan kind"
    );
    let rate_type = parse_rate_type(&product.rate_type)?;
    let initial_annual_rate_bp = match rate_type {
        LoanRateType::Fixed => {
            ensure!(
                product.reference_rate_key.is_none()
                    && product.spread_bp.is_none()
                    && product.rate_reset_rule == "none",
                "fixed starting-loan rate terms are inconsistent"
            );
            i64::from(
                product
                    .fixed_annual_rate_bp
                    .context("fixed starting-loan rate is missing")?,
            )
        }
        LoanRateType::Variable => {
            ensure!(
                product.reference_rate_key.as_deref() == Some("treasury3m")
                    && product.rate_reset_rule == "monthlyDay1",
                "variable starting-loan rate reference is unsupported"
            );
            let reference_rate_bp = i64::from(
                scope
                    .treasury_3m_bp
                    .context("day-zero treasury rate is unavailable")?,
            );
            let spread_bp = i64::from(
                product
                    .spread_bp
                    .context("variable starting-loan spread is missing")?,
            );
            let unclamped_rate_bp = reference_rate_bp
                .checked_add(spread_bp)
                .context("variable starting-loan rate overflowed")?;
            let minimum_rate_bp = i64::from(
                product
                    .minimum_annual_rate_bp
                    .context("variable starting-loan rate floor is missing")?,
            );
            let maximum_rate_bp = i64::from(
                product
                    .maximum_annual_rate_bp
                    .context("variable starting-loan rate cap is missing")?,
            );
            unclamped_rate_bp.clamp(minimum_rate_bp, maximum_rate_bp)
        }
    };
    Ok(ResolvedProductTerms {
        repayment_method: parse_repayment_method(&product.repayment_method)?,
        initial_annual_rate_bp,
        term_months: product
            .term_months
            .context("starting-loan term is missing")?,
    })
}

async fn insert_initial_credit_state(
    tx: &mut Transaction<'_, MySql>,
    scope: &LoanRunScopeRow,
    starting_units: i64,
    starting_band: CreditBand,
) -> Result<()> {
    let save_id = scope_id(scope)?;
    let run_revision = scope_run_revision(scope)?;
    let credit_units =
        u16::try_from(starting_units).context("starting credit units are invalid")?;
    let band = credit_band_db(starting_band);
    sqlx::query(
        "INSERT INTO credit_state
             (save_id, run_revision, household_id, credit_model_version_id,
              credit_units, credit_band, last_evaluated_game_day, evaluation_revision)
         VALUES (?, ?, ?, ?, ?, ?, ?, 0)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(scope.household_id)
    .bind(scope.credit_model_version_id)
    .bind(credit_units)
    .bind(band)
    .bind(scope.game_day)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO credit_history
             (save_id, run_revision, household_id, credit_model_version_id,
              loan_contract_id, game_day, event_order, event_kind, reason_code,
              delta_units, unclamped_before_units, unclamped_after_units,
              before_units, after_units, before_band, after_band)
         VALUES (?, ?, ?, ?, NULL, ?, 1, 'initial', 'characterStart',
                 0, ?, ?, ?, ?, ?, ?)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(scope.household_id)
    .bind(scope.credit_model_version_id)
    .bind(scope.game_day)
    .bind(starting_units)
    .bind(starting_units)
    .bind(credit_units)
    .bind(credit_units)
    .bind(band)
    .bind(band)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_starting_contract(
    tx: &mut Transaction<'_, MySql>,
    draft: StartingContractDraft<'_>,
) -> Result<u64> {
    let StartingContractDraft {
        scope,
        product,
        terms,
        origin_command_id,
        origin,
        principal_krw,
        maturity_game_day,
    } = draft;
    let inserted = sqlx::query(
        "INSERT INTO loan_contract
             (save_id, run_revision, household_id, credit_model_version_id,
              loan_product_version_id, loan_quote_id, origin_kind, origin_command_id,
              product_kind, lender_sector, rate_status, rate_type, reference_rate_key,
              fixed_annual_rate_bp, applied_spread_bp, minimum_annual_rate_bp,
              maximum_annual_rate_bp, current_annual_rate_bp, rate_reset_rule,
              day_count_denominator, repayment_method, term_months, total_installments,
              payment_calendar, grace_months, prepayment_fee_ppm, prepayment_effect,
              dsr_included, read_only, status, original_principal_krw,
              remaining_principal_krw, accrued_interest_krw, accrued_fee_krw,
              interest_remainder_numerator, activated_game_day, maturity_game_day,
              next_installment_no, oldest_unpaid_due_game_day)
         VALUES (?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                 365, ?, ?, ?, ?, ?, ?, ?, ?, FALSE, 'active', ?, ?, 0, 0, 0, ?, ?, 1, NULL)",
    )
    .bind(scope_id(scope)?)
    .bind(scope_run_revision(scope)?)
    .bind(scope.household_id)
    .bind(scope.credit_model_version_id)
    .bind(product.id)
    .bind(origin.as_str())
    .bind(origin_command_id)
    .bind(&product.product_kind)
    .bind(&product.lender_sector)
    .bind(&product.rate_status)
    .bind(&product.rate_type)
    .bind(&product.reference_rate_key)
    .bind(product.fixed_annual_rate_bp)
    .bind(product.spread_bp)
    .bind(product.minimum_annual_rate_bp)
    .bind(product.maximum_annual_rate_bp)
    .bind(u16::try_from(terms.initial_annual_rate_bp).context("initial loan rate is invalid")?)
    .bind(&product.rate_reset_rule)
    .bind(&product.repayment_method)
    .bind(terms.term_months)
    .bind(terms.term_months)
    .bind(&product.payment_calendar)
    .bind(product.grace_months)
    .bind(product.prepayment_fee_ppm)
    .bind(&product.prepayment_effect)
    .bind(product.dsr_included)
    .bind(principal_krw)
    .bind(principal_krw)
    .bind(scope.game_day)
    .bind(maturity_game_day)
    .execute(&mut **tx)
    .await?;
    Ok(inserted.last_insert_id())
}

async fn insert_executed_contract(
    tx: &mut Transaction<'_, MySql>,
    draft: ExecutedContractDraft<'_>,
) -> Result<u64> {
    let ExecutedContractDraft {
        scope,
        product,
        quote_id,
        origin_command_id,
        principal_krw,
        maturity_game_day,
    } = draft;
    let annual_rate_bp = product
        .current_annual_rate_bp
        .context("executed loan product has no current rate")?;
    let fixed_annual_rate_bp = (product.rate_type == LoanRateType::Fixed)
        .then(|| u16::try_from(annual_rate_bp))
        .transpose()
        .context("executed fixed rate is out of range")?;
    let reference_rate_key = product
        .reference_rate_key
        .as_ref()
        .map(to_db_str)
        .transpose()?;
    let inserted = sqlx::query(
        "INSERT INTO loan_contract
             (save_id, run_revision, household_id, credit_model_version_id,
              loan_product_version_id, loan_quote_id, origin_kind, origin_command_id,
              product_kind, lender_sector, rate_status, rate_type, reference_rate_key,
              fixed_annual_rate_bp, applied_spread_bp, minimum_annual_rate_bp,
              maximum_annual_rate_bp, current_annual_rate_bp, rate_reset_rule,
              day_count_denominator, repayment_method, term_months, total_installments,
              payment_calendar, grace_months, prepayment_fee_ppm, prepayment_effect,
              dsr_included, read_only, status, original_principal_krw,
              remaining_principal_krw, accrued_interest_krw, accrued_fee_krw,
              interest_remainder_numerator, activated_game_day, maturity_game_day,
              next_installment_no, oldest_unpaid_due_game_day)
         VALUES (?, ?, ?, ?, ?, ?, 'quoteExecution', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                 365, ?, ?, ?, ?, ?, ?, ?, ?, FALSE, 'active', ?, ?, 0, 0, 0, ?, ?, 1, NULL)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.household_id)
    .bind(scope.credit_model_version_id)
    .bind(product.id.get())
    .bind(quote_id.get())
    .bind(origin_command_id)
    .bind(to_db_str(&product.kind)?)
    .bind(to_db_str(&product.lender_sector)?)
    .bind(to_db_str(&product.rate_status)?)
    .bind(to_db_str(&product.rate_type)?)
    .bind(reference_rate_key)
    .bind(fixed_annual_rate_bp)
    .bind(
        product
            .spread_bp
            .map(i16::try_from)
            .transpose()
            .context("executed loan spread is out of range")?,
    )
    .bind(
        u16::try_from(product.minimum_annual_rate_bp)
            .context("executed minimum loan rate is out of range")?,
    )
    .bind(
        u16::try_from(product.maximum_annual_rate_bp)
            .context("executed maximum loan rate is out of range")?,
    )
    .bind(u16::try_from(annual_rate_bp).context("executed loan rate is out of range")?)
    .bind(to_db_str(&product.rate_reset_rule)?)
    .bind(to_db_str(&product.repayment_method)?)
    .bind(product.term_months)
    .bind(product.term_months)
    .bind(to_db_str(&product.payment_calendar)?)
    .bind(product.grace_months)
    .bind(product.prepayment_fee_ppm)
    .bind(to_db_str(&product.prepayment_effect)?)
    .bind(product.dsr_included)
    .bind(principal_krw)
    .bind(principal_krw)
    .bind(scope.game_day)
    .bind(maturity_game_day)
    .execute(&mut **tx)
    .await?;
    Ok(inserted.last_insert_id())
}

#[allow(clippy::too_many_arguments)]
async fn insert_lease_deposit_executed_contract(
    tx: &mut Transaction<'_, MySql>,
    scope: &LoanRunScopeRow,
    product: &LoanProductState,
    quote_id: ResourceId,
    lease_id: u64,
    origin_command_id: &str,
    principal_krw: i64,
    maturity_game_day: u32,
) -> Result<u64> {
    let annual_rate_bp = product
        .current_annual_rate_bp
        .context("lease-deposit product has no current rate")?;
    let fixed_annual_rate_bp = (product.rate_type == LoanRateType::Fixed)
        .then(|| u16::try_from(annual_rate_bp))
        .transpose()
        .context("lease-deposit fixed rate is out of range")?;
    let reference_rate_key = product
        .reference_rate_key
        .as_ref()
        .map(to_db_str)
        .transpose()?;
    let inserted = sqlx::query(
        "INSERT INTO loan_contract
             (save_id, run_revision, household_id, credit_model_version_id,
              loan_product_version_id, loan_quote_id, lease_contract_id,
              origin_kind, origin_command_id, product_kind, lender_sector,
              rate_status, rate_type, reference_rate_key, fixed_annual_rate_bp,
              applied_spread_bp, minimum_annual_rate_bp, maximum_annual_rate_bp,
              current_annual_rate_bp, rate_reset_rule, day_count_denominator,
              repayment_method, term_months, total_installments, payment_calendar,
              grace_months, prepayment_fee_ppm, prepayment_effect, dsr_included,
              read_only, status, original_principal_krw, remaining_principal_krw,
              accrued_interest_krw, accrued_fee_krw, interest_remainder_numerator,
              activated_game_day, maturity_game_day, next_installment_no,
              oldest_unpaid_due_game_day)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'leaseDepositExecution', ?, ?, ?, ?, ?, ?, ?,
                 ?, ?, ?, ?, ?, 365, ?, ?, ?, ?, ?, ?, ?, ?, FALSE, 'active', ?, ?,
                 0, 0, 0, ?, ?, 1, NULL)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.household_id)
    .bind(scope.credit_model_version_id)
    .bind(product.id.get())
    .bind(quote_id.get())
    .bind(lease_id)
    .bind(origin_command_id)
    .bind(to_db_str(&product.kind)?)
    .bind(to_db_str(&product.lender_sector)?)
    .bind(to_db_str(&product.rate_status)?)
    .bind(to_db_str(&product.rate_type)?)
    .bind(reference_rate_key)
    .bind(fixed_annual_rate_bp)
    .bind(
        product
            .spread_bp
            .map(i16::try_from)
            .transpose()
            .context("lease-deposit spread is out of range")?,
    )
    .bind(
        u16::try_from(product.minimum_annual_rate_bp)
            .context("lease-deposit minimum rate is out of range")?,
    )
    .bind(
        u16::try_from(product.maximum_annual_rate_bp)
            .context("lease-deposit maximum rate is out of range")?,
    )
    .bind(u16::try_from(annual_rate_bp).context("lease-deposit rate is out of range")?)
    .bind(to_db_str(&product.rate_reset_rule)?)
    .bind(to_db_str(&product.repayment_method)?)
    .bind(product.term_months)
    .bind(product.term_months)
    .bind(to_db_str(&product.payment_calendar)?)
    .bind(product.grace_months)
    .bind(product.prepayment_fee_ppm)
    .bind(to_db_str(&product.prepayment_effect)?)
    .bind(product.dsr_included)
    .bind(principal_krw)
    .bind(principal_krw)
    .bind(scope.game_day)
    .bind(maturity_game_day)
    .execute(&mut **tx)
    .await?;
    Ok(inserted.last_insert_id())
}

#[allow(clippy::too_many_arguments)]
async fn insert_mortgage_executed_contract(
    tx: &mut Transaction<'_, MySql>,
    scope: &LoanRunScopeRow,
    product: &LoanProductState,
    quote_id: ResourceId,
    property_holding_id: u64,
    origin_command_id: &str,
    principal_krw: i64,
    maturity_game_day: u32,
) -> Result<u64> {
    let annual_rate_bp = product
        .current_annual_rate_bp
        .context("mortgage product has no current rate")?;
    let inserted = sqlx::query(
        "INSERT INTO loan_contract
             (save_id, run_revision, household_id, credit_model_version_id,
              loan_product_version_id, loan_quote_id, property_holding_id,
              origin_kind, origin_command_id, product_kind, lender_sector,
              rate_status, rate_type, reference_rate_key, fixed_annual_rate_bp,
              applied_spread_bp, minimum_annual_rate_bp, maximum_annual_rate_bp,
              current_annual_rate_bp, rate_reset_rule, day_count_denominator,
              repayment_method, term_months, total_installments, payment_calendar,
              grace_months, prepayment_fee_ppm, prepayment_effect, dsr_included,
              read_only, status, original_principal_krw, remaining_principal_krw,
              accrued_interest_krw, accrued_fee_krw, interest_remainder_numerator,
              activated_game_day, maturity_game_day, next_installment_no,
              oldest_unpaid_due_game_day)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'mortgagePurchaseExecution', ?, ?, ?, ?, ?, NULL, ?,
                 NULL, ?, ?, ?, ?, 365, ?, ?, ?, ?, ?, ?, ?, ?, FALSE, 'active', ?, ?,
                 0, 0, 0, ?, ?, 1, NULL)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.household_id)
    .bind(scope.credit_model_version_id)
    .bind(product.id.get())
    .bind(quote_id.get())
    .bind(property_holding_id)
    .bind(origin_command_id)
    .bind(to_db_str(&product.kind)?)
    .bind(to_db_str(&product.lender_sector)?)
    .bind(to_db_str(&product.rate_status)?)
    .bind(to_db_str(&product.rate_type)?)
    .bind(u16::try_from(annual_rate_bp).context("mortgage fixed rate is out of range")?)
    .bind(
        u16::try_from(product.minimum_annual_rate_bp)
            .context("mortgage minimum rate is out of range")?,
    )
    .bind(
        u16::try_from(product.maximum_annual_rate_bp)
            .context("mortgage maximum rate is out of range")?,
    )
    .bind(u16::try_from(annual_rate_bp).context("mortgage rate is out of range")?)
    .bind(to_db_str(&product.rate_reset_rule)?)
    .bind(to_db_str(&product.repayment_method)?)
    .bind(product.term_months)
    .bind(product.term_months)
    .bind(to_db_str(&product.payment_calendar)?)
    .bind(product.grace_months)
    .bind(product.prepayment_fee_ppm)
    .bind(to_db_str(&product.prepayment_effect)?)
    .bind(product.dsr_included)
    .bind(principal_krw)
    .bind(principal_krw)
    .bind(scope.game_day)
    .bind(maturity_game_day)
    .execute(&mut **tx)
    .await?;
    Ok(inserted.last_insert_id())
}

async fn insert_schedule_and_settlements(
    tx: &mut Transaction<'_, MySql>,
    scope: &LoanRunScopeRow,
    contract_id: u64,
    periods: &[StoredPeriod],
    installments: &[crate::life::LoanInstallmentCalculation],
) -> Result<()> {
    ensure!(
        periods.len() == installments.len(),
        "loan schedule period count changed"
    );
    let mut remainder_before = 0_i128;
    for (period, installment) in periods.iter().zip(installments) {
        ensure!(
            installment.due_game_day == period.end_game_day
                && installment.elapsed_days == period.calculation.elapsed_days,
            "loan schedule calendar changed"
        );
        sqlx::query(
            "INSERT INTO loan_installment
                 (save_id, run_revision, loan_contract_id, installment_no, due_game_day,
                  interest_period_start_game_day, interest_period_end_game_day, elapsed_days,
                  annual_rate_bp, opening_principal_krw, scheduled_fee_krw,
                  scheduled_interest_krw, scheduled_principal_krw,
                  interest_remainder_before, interest_remainder_after,
                  paid_fee_krw, paid_interest_krw, paid_principal_krw, status,
                  schedule_revision)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, 0, 0, 0, 'pending', 1)",
        )
        .bind(scope_id(scope)?)
        .bind(scope_run_revision(scope)?)
        .bind(contract_id)
        .bind(installment.sequence)
        .bind(installment.due_game_day)
        .bind(period.start_game_day)
        .bind(period.end_game_day)
        .bind(installment.elapsed_days)
        .bind(u16::try_from(installment.annual_rate_bp).context("scheduled rate is invalid")?)
        .bind(installment.opening_principal_krw)
        .bind(installment.interest_krw)
        .bind(installment.principal_krw)
        .bind(remainder_before.to_string())
        .bind(installment.carried_interest_remainder_numerator.to_string())
        .execute(&mut **tx)
        .await?;
        let payload = serde_json::to_string(&LoanInstallmentSettlementPayload {
            version: LOAN_SETTLEMENT_PAYLOAD_VERSION,
            loan_contract_id: contract_id.to_string(),
            installment_no: installment.sequence,
        })?;
        sqlx::query(
            "INSERT INTO scheduled_settlement
                 (save_id, run_revision, due_game_day, kind, payload,
                  source_kind, source_id, occurrence, status)
             VALUES (?, ?, ?, 'loanInstallment', ?, 'loanContract', ?, ?, 'pending')",
        )
        .bind(scope_id(scope)?)
        .bind(scope_run_revision(scope)?)
        .bind(installment.due_game_day)
        .bind(payload)
        .bind(contract_id.to_string())
        .bind(u32::from(installment.sequence))
        .execute(&mut **tx)
        .await?;
        remainder_before = installment.carried_interest_remainder_numerator;
    }
    Ok(())
}

fn build_month_end_periods(
    world_start_date: Date,
    activated_game_day: u32,
    term_months: u16,
) -> Result<Vec<StoredPeriod>> {
    ensure!(term_months > 0, "loan term must contain at least one month");
    let activation_date = world_start_date
        .checked_add(Duration::days(i64::from(activated_game_day)))
        .context("loan activation date overflowed")?;
    let activation_month_end = month_end(activation_date)?;
    let first_due_date = if activation_date < activation_month_end {
        activation_month_end
    } else {
        month_end(next_month_start(activation_date)?)?
    };
    let mut periods = Vec::with_capacity(usize::from(term_months));
    let mut start_game_day = activated_game_day
        .checked_add(1)
        .context("first loan interest day overflowed")?;
    let mut due_date = first_due_date;
    for _ in 0..term_months {
        let end_game_day = game_day_for_date(world_start_date, due_date)?;
        ensure!(
            end_game_day >= start_game_day,
            "loan interest period ends before it starts"
        );
        let elapsed_days = end_game_day
            .checked_sub(start_game_day)
            .and_then(|days| days.checked_add(1))
            .context("loan interest interval overflowed")?;
        periods.push(StoredPeriod {
            start_game_day,
            end_game_day,
            calculation: LoanSchedulePeriod {
                due_game_day: end_game_day,
                elapsed_days: u16::try_from(elapsed_days)
                    .context("loan interest interval is out of range")?,
            },
        });
        start_game_day = end_game_day
            .checked_add(1)
            .context("next loan interest period overflowed")?;
        due_date = month_end(next_month_start(due_date)?)?;
    }
    Ok(periods)
}

fn month_end(date: Date) -> Result<Date> {
    next_month_start(date)?
        .previous_day()
        .context("loan month end overflowed")
}

fn next_month_start(date: Date) -> Result<Date> {
    let (year, month) = if date.month() == Month::December {
        (
            date.year().checked_add(1).context("loan year overflowed")?,
            Month::January,
        )
    } else {
        (
            date.year(),
            Month::try_from(u8::from(date.month()) + 1).context("loan month is invalid")?,
        )
    };
    Date::from_calendar_date(year, month, 1).context("loan month start is invalid")
}

fn game_day_for_date(world_start_date: Date, date: Date) -> Result<u32> {
    u32::try_from((date - world_start_date).whole_days())
        .context("loan date is outside the game-day range")
}

fn scope_id(scope: &LoanRunScopeRow) -> Result<u64> {
    ensure!(scope.save_id > 0, "loan run scope has an invalid save id");
    Ok(scope.save_id)
}

fn scope_run_revision(scope: &LoanRunScopeRow) -> Result<u32> {
    Ok(scope.run_revision)
}

fn parse_product_kind(value: &str) -> Result<LoanProductKind> {
    match value {
        "studentLoan" => Ok(LoanProductKind::StudentLoan),
        "unsecuredLoan" => Ok(LoanProductKind::UnsecuredLoan),
        "leaseDepositLoan" => Ok(LoanProductKind::LeaseDepositLoan),
        "mortgage" => Ok(LoanProductKind::Mortgage),
        "legacyDebt" => Ok(LoanProductKind::LegacyDebt),
        _ => bail!("unknown loan product kind"),
    }
}

fn parse_rate_type(value: &str) -> Result<LoanRateType> {
    match value {
        "fixed" => Ok(LoanRateType::Fixed),
        "variable" => Ok(LoanRateType::Variable),
        _ => bail!("unknown active loan rate type"),
    }
}

fn parse_lender_sector(value: &str) -> Result<LoanLenderSector> {
    match value {
        "bank" => Ok(LoanLenderSector::Bank),
        "nonBank" => Ok(LoanLenderSector::NonBank),
        _ => bail!("unknown loan lender sector"),
    }
}

fn parse_rate_reference(value: &str) -> Result<LoanRateReference> {
    match value {
        "treasury3m" => Ok(LoanRateReference::Treasury3m),
        _ => bail!("unknown loan reference rate"),
    }
}

fn parse_rate_reset_rule(value: &str) -> Result<LoanRateResetRule> {
    match value {
        "none" => Ok(LoanRateResetRule::None),
        "monthlyDay1" => Ok(LoanRateResetRule::MonthlyDay1),
        _ => bail!("unknown loan rate reset rule"),
    }
}

fn parse_day_count_rule(value: &str) -> Result<LoanDayCountRule> {
    match value {
        "actual365" => Ok(LoanDayCountRule::Actual365),
        _ => bail!("unknown loan day-count rule"),
    }
}

fn parse_payment_calendar(value: &str) -> Result<LoanPaymentCalendar> {
    match value {
        "monthEnd" => Ok(LoanPaymentCalendar::MonthEnd),
        _ => bail!("unknown loan payment calendar"),
    }
}

fn parse_prepayment_effect(value: &str) -> Result<LoanPrepaymentEffect> {
    match value {
        "reduceTerm" => Ok(LoanPrepaymentEffect::ReduceTerm),
        "recalculatePayment" => Ok(LoanPrepaymentEffect::RecalculatePayment),
        _ => bail!("unknown loan prepayment effect"),
    }
}

fn parse_product_provenance(value: &str) -> Result<LoanProductProvenance> {
    match value {
        "GAME_BALANCE" => Ok(LoanProductProvenance::GameBalance),
        _ => bail!("unknown loan product provenance"),
    }
}

fn parse_rate_status(value: &str) -> Result<LoanRateStatus> {
    match value {
        "available" => Ok(LoanRateStatus::Available),
        "rateUnavailable" => Ok(LoanRateStatus::RateUnavailable),
        _ => bail!("unknown loan rate status"),
    }
}

fn parse_credit_band(value: &str) -> Result<CreditBand> {
    match value {
        "prime" => Ok(CreditBand::Prime),
        "standard" => Ok(CreditBand::Standard),
        "limited" => Ok(CreditBand::Limited),
        "distressed" => Ok(CreditBand::Distressed),
        "insolvent" => Ok(CreditBand::Insolvent),
        _ => bail!("unknown credit band"),
    }
}

fn parse_repayment_method(value: &str) -> Result<LoanRepaymentMethod> {
    match value {
        "equalPrincipal" => Ok(LoanRepaymentMethod::EqualPrincipal),
        "levelPayment" => Ok(LoanRepaymentMethod::LevelPayment),
        "bullet" => Ok(LoanRepaymentMethod::Bullet),
        _ => bail!("unknown loan repayment method"),
    }
}

fn parse_credit_model(parameters_json: &str) -> Result<CreditModelTerms> {
    let stored: StoredCreditModelParameters =
        serde_json::from_str(parameters_json).context("credit model parameters are invalid")?;
    ensure!(
        stored.provenance == "GAME_BALANCE",
        "credit model provenance is unsupported"
    );
    ensure!(
        matches!(
            (stored.schema_version, stored.loan_eligibility.as_ref()),
            (2, None) | (3..=5, Some(_))
        ),
        "credit model schema is unsupported"
    );
    let mut bands = BTreeMap::new();
    for band in stored.bands {
        ensure!(
            band.minimum_units <= band.maximum_units
                && bands
                    .insert(band.band, (band.minimum_units, band.maximum_units))
                    .is_none(),
            "credit model bands are invalid"
        );
    }
    let prime = bands
        .remove("prime")
        .context("credit model prime band is missing")?;
    let standard = bands
        .remove("standard")
        .context("credit model standard band is missing")?;
    let limited = bands
        .remove("limited")
        .context("credit model limited band is missing")?;
    let distressed = bands
        .remove("distressed")
        .context("credit model distressed band is missing")?;
    let insolvent = bands
        .remove("insolvent")
        .context("credit model insolvent band is missing")?;
    ensure!(
        bands.is_empty()
            && prime.1 == stored.credit_units.maximum
            && standard.1.checked_add(1) == Some(prime.0)
            && limited.1.checked_add(1) == Some(standard.0)
            && distressed.1.checked_add(1) == Some(limited.0)
            && insolvent.1.checked_add(1) == Some(distressed.0)
            && insolvent.0 == stored.credit_units.minimum,
        "credit model bands are not contiguous"
    );
    Ok(CreditModelTerms {
        minimum_units: stored.credit_units.minimum,
        maximum_units: stored.credit_units.maximum,
        starting_units: stored.credit_units.initial,
        band_thresholds: CreditBandThresholds {
            prime_min_units: prime.0,
            standard_min_units: standard.0,
            limited_min_units: limited.0,
            distressed_min_units: distressed.0,
            insolvent_min_units: insolvent.0,
        },
        delinquency_event_penalty_units: stored.event_penalty.active_to_delinquent_units,
        default_event_penalty_units: stored.event_penalty.delinquent_to_defaulted_units,
        legal_procedure_event_penalty_units: stored.event_penalty.legal_procedure_units,
        adverse_day_penalty_units: stored.daily_change.delinquent_or_defaulted_penalty_units,
        recovery_units: stored.daily_change.clean_recovery_units,
        default_oldest_days: stored.default_rule.absolute_oldest_bucket_days,
        amount_default_threshold_krw: stored.default_rule.amount_and_age_minimum_krw,
        amount_default_oldest_days: stored.default_rule.amount_and_age_oldest_bucket_days,
    })
}

fn credit_band_db(band: CreditBand) -> &'static str {
    match band {
        CreditBand::Prime => "prime",
        CreditBand::Standard => "standard",
        CreditBand::Limited => "limited",
        CreditBand::Distressed => "distressed",
        CreditBand::Insolvent => "insolvent",
    }
}

pub(super) async fn calculate_debt_projection_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<DebtProjection> {
    let loan_krw: i64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(
                    remaining_principal_krw + accrued_interest_krw + accrued_fee_krw
                ), 0) AS SIGNED)
         FROM loan_contract
         WHERE save_id = ? AND run_revision = ?
           AND status IN ('active', 'delinquent', 'defaulted', 'restructured')",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await?;
    let essential_arrear_krw: i64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(outstanding_amount_krw), 0) AS SIGNED)
         FROM essential_arrear
         WHERE save_id = ? AND run_revision = ? AND status = 'active'",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await?;
    let lease_arrear_krw: i64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(remaining_krw), 0) AS SIGNED)
         FROM lease_arrear
         WHERE save_id = ? AND run_revision = ? AND status = 'active'",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await?;
    let tax_obligation_krw: i64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(outstanding_amount_krw), 0) AS SIGNED)
         FROM tax_obligation
         WHERE save_id = ? AND run_revision = ? AND status = 'outstanding'",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        loan_krw >= 0
            && essential_arrear_krw >= 0
            && lease_arrear_krw >= 0
            && tax_obligation_krw >= 0,
        "debt authority contains a negative balance"
    );
    let total_krw = loan_krw
        .checked_add(essential_arrear_krw)
        .and_then(|value| value.checked_add(lease_arrear_krw))
        .and_then(|value| value.checked_add(tax_obligation_krw))
        .context("debt projection overflowed")?;
    Ok(DebtProjection {
        loan_krw,
        essential_arrear_krw,
        lease_arrear_krw,
        tax_obligation_krw,
        total_krw,
    })
}

pub(super) async fn validate_debt_projection_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<DebtProjection> {
    let projection = calculate_debt_projection_in_tx(tx, save_id, run_revision).await?;
    let stored_debt_krw: i64 = sqlx::query_scalar(
        "SELECT debt_krw FROM save WHERE id = ? AND run_revision = ? FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        stored_debt_krw == projection.total_krw,
        "save debt disagrees with its typed authority projection"
    );
    Ok(projection)
}

pub(super) async fn read_loan_product_catalog_in_tx(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
) -> Result<LoanProductCatalogState> {
    let context: LoanProductCatalogContextRow = sqlx::query_as(
        "SELECT
             CASE WHEN current_save.id IS NULL
                  THEN assignment.credit_model_version_id
                  ELSE bundle.credit_model_version_id END AS credit_model_version_id,
             CASE WHEN current_save.id IS NULL
                  THEN assignment.market_world_id
                  ELSE current_save.market_world_id END AS market_world_id,
             CASE WHEN current_save.id IS NULL THEN 0 ELSE current_save.game_day END AS game_day
         FROM run_rule_bundle_assignment AS assignment
         LEFT JOIN save AS current_save
           ON current_save.user_id = ?
          AND EXISTS (
              SELECT 1 FROM `character`
              WHERE `character`.save_id = current_save.id
          )
         LEFT JOIN run_rule_bundle AS bundle
           ON bundle.save_id = current_save.id
          AND bundle.run_revision = current_save.run_revision
         WHERE assignment.assignment_key = 'newRun'",
    )
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?;
    let credit_model_version_id = context
        .credit_model_version_id
        .context("loan catalog context has no credit model")?;
    let market_world_id = context
        .market_world_id
        .context("loan catalog context has no market world")?;
    let game_day = u32::try_from(
        context
            .game_day
            .context("loan catalog context has no game day")?,
    )
    .context("loan catalog game day is out of range")?;
    let active_model_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM credit_model_version AS model
         INNER JOIN credit_model_strict_manifest AS manifest
           ON manifest.credit_model_version_id = model.id
          AND manifest.canonical_sha256 = model.canonical_sha256
         WHERE model.id = ? AND model.availability = 'active'
           AND model.sealed_at IS NOT NULL",
    )
    .bind(credit_model_version_id)
    .fetch_one(&mut **tx)
    .await?;
    if active_model_count == 0 {
        return Ok(LoanProductCatalogState {
            credit_model_version_id: None,
            products: Vec::new(),
        });
    }
    ensure!(
        active_model_count == 1,
        "loan catalog matched more than one active credit model"
    );

    let treasury_3m_bp = sqlx::query_scalar::<_, Option<i16>>(
        "SELECT treasury_3m_bp FROM market_daily
         WHERE world_id = ? AND game_day = ?",
    )
    .bind(market_world_id)
    .bind(game_day)
    .fetch_optional(&mut **tx)
    .await?
    .flatten();
    let rows: Vec<LoanProductCatalogRow> = sqlx::query_as(
        "SELECT product.id, product.product_key, product.display_name,
                product.product_kind, product.lender_sector, product.rate_status,
                product.rate_type, product.reference_rate_key,
                product.fixed_annual_rate_bp, product.spread_bp,
                product.minimum_annual_rate_bp, product.maximum_annual_rate_bp,
                product.rate_reset_rule, product.day_count_rule,
                product.repayment_method, product.term_months,
                product.payment_calendar, product.grace_months,
                product.minimum_principal_krw, product.maximum_principal_krw,
                product.prepayment_fee_ppm, product.prepayment_effect,
                product.starting_eligible, product.quote_eligible,
                product.execution_eligible, product.prepayment_allowed,
                product.dsr_included, product.provenance_kind
         FROM loan_product_version AS product
         INNER JOIN loan_product_canonical_manifest AS manifest
           ON manifest.loan_product_version_id = product.id
          AND manifest.canonical_sha256 = product.canonical_sha256
         WHERE product.credit_model_version_id = ?
           AND product.catalog_scope = 'modelChild'
           AND product.sealed_at IS NOT NULL
         ORDER BY product.display_order, product.id
         LIMIT 17",
    )
    .bind(credit_model_version_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() <= 16,
        "loan product catalog exceeds its public bound"
    );
    let products = rows
        .into_iter()
        .map(|row| loan_product_state_from_row(row, treasury_3m_bp))
        .collect::<Result<Vec<_>>>()?;
    for kind in [LoanProductKind::StudentLoan, LoanProductKind::UnsecuredLoan] {
        ensure!(
            products
                .iter()
                .filter(|product| product.kind == kind && product.starting_eligible)
                .count()
                == 1,
            "loan catalog must publish exactly one starting product per M4-B kind"
        );
    }
    Ok(LoanProductCatalogState {
        credit_model_version_id: Some(ResourceId::from_u64(credit_model_version_id)),
        products,
    })
}

fn loan_product_state_from_row(
    row: LoanProductCatalogRow,
    treasury_3m_bp: Option<i16>,
) -> Result<LoanProductState> {
    ensure!(row.id > 0, "loan product catalog has an invalid id");
    let kind = parse_product_kind(&row.product_kind)?;
    ensure!(
        matches!(
            kind,
            LoanProductKind::StudentLoan
                | LoanProductKind::UnsecuredLoan
                | LoanProductKind::LeaseDepositLoan
                | LoanProductKind::Mortgage
        ),
        "public loan catalog contains an unsupported product kind"
    );
    let lender_sector = parse_lender_sector(&row.lender_sector)?;
    let stored_rate_status = parse_rate_status(&row.rate_status)?;
    let rate_type = parse_rate_type(&row.rate_type)?;
    let minimum_annual_rate_bp = i64::from(
        row.minimum_annual_rate_bp
            .context("loan product minimum rate is missing")?,
    );
    let maximum_annual_rate_bp = i64::from(
        row.maximum_annual_rate_bp
            .context("loan product maximum rate is missing")?,
    );
    ensure!(
        minimum_annual_rate_bp <= maximum_annual_rate_bp,
        "loan product rate bounds are reversed"
    );
    let resolved_annual_rate_bp = match rate_type {
        LoanRateType::Fixed => Some(i64::from(
            row.fixed_annual_rate_bp
                .context("fixed loan product has no fixed rate")?,
        )),
        LoanRateType::Variable => treasury_3m_bp
            .zip(row.spread_bp)
            .map(|(reference, spread)| i64::from(reference) + i64::from(spread))
            .map(|rate| rate.clamp(minimum_annual_rate_bp, maximum_annual_rate_bp)),
    };
    let current_annual_rate_bp = (stored_rate_status == LoanRateStatus::Available)
        .then_some(resolved_annual_rate_bp)
        .flatten();
    let rate_status = if current_annual_rate_bp.is_some() {
        LoanRateStatus::Available
    } else {
        LoanRateStatus::RateUnavailable
    };
    let reference_rate_key = row
        .reference_rate_key
        .as_deref()
        .map(parse_rate_reference)
        .transpose()?;
    ensure!(
        matches!(
            (rate_type, reference_rate_key, row.spread_bp),
            (LoanRateType::Fixed, None, None)
                | (
                    LoanRateType::Variable,
                    Some(LoanRateReference::Treasury3m),
                    Some(_)
                )
        ),
        "loan product rate reference shape is invalid"
    );
    let minimum_principal_krw = row
        .minimum_principal_krw
        .context("loan product minimum principal is missing")?;
    let maximum_principal_krw = row
        .maximum_principal_krw
        .context("loan product maximum principal is missing")?;
    ensure!(
        minimum_principal_krw > 0 && maximum_principal_krw >= minimum_principal_krw,
        "loan product principal bounds are invalid"
    );
    Ok(LoanProductState {
        id: ResourceId::from_u64(row.id),
        key: row.product_key,
        display_name: row.display_name,
        kind,
        lender_sector,
        rate_status,
        rate_type,
        current_annual_rate_bp,
        reference_rate_key,
        spread_bp: row.spread_bp.map(i64::from),
        minimum_annual_rate_bp,
        maximum_annual_rate_bp,
        rate_reset_rule: parse_rate_reset_rule(&row.rate_reset_rule)?,
        day_count_rule: parse_day_count_rule(&row.day_count_rule)?,
        repayment_method: parse_repayment_method(&row.repayment_method)?,
        term_months: row.term_months.context("loan product term is missing")?,
        payment_calendar: parse_payment_calendar(&row.payment_calendar)?,
        grace_months: row
            .grace_months
            .context("loan product grace period is missing")?,
        minimum_principal_krw,
        maximum_principal_krw,
        prepayment_fee_ppm: row
            .prepayment_fee_ppm
            .context("loan product prepayment fee is missing")?,
        prepayment_effect: parse_prepayment_effect(&row.prepayment_effect)?,
        starting_eligible: row.starting_eligible,
        quote_eligible: row.quote_eligible,
        execution_eligible: row.execution_eligible,
        prepayment_allowed: row.prepayment_allowed,
        dsr_included: row.dsr_included,
        provenance: parse_product_provenance(&row.provenance_kind)?,
    })
}

pub(super) async fn create_loan_quote_in_tx(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
    command: &CreateLoanQuoteCommand,
    fingerprint: &str,
) -> Result<LoanQuoteCreation> {
    if command.principal_krw <= 0 {
        return Ok(LoanQuoteCreation::Rejected(LifeFailureCode::InvalidCommand));
    }
    let save_id: Option<u64> =
        sqlx::query_scalar("SELECT id FROM save WHERE user_id = ? AND run_revision = ?")
            .bind(user_id)
            .bind(command.cursor.expected_run_revision)
            .fetch_optional(&mut **tx)
            .await?;
    let Some(save_id) = save_id else {
        return Ok(LoanQuoteCreation::Rejected(LifeFailureCode::Busy));
    };
    let scope = read_loan_run_scope(tx, save_id, command.cursor.expected_run_revision).await?;
    let Some(eligibility) = parse_quote_eligibility(&scope.model_parameters_json)? else {
        return Ok(LoanQuoteCreation::Rejected(
            LifeFailureCode::RateUnavailable,
        ));
    };
    let catalog = read_loan_product_catalog_in_tx(tx, user_id).await?;
    ensure!(
        catalog.credit_model_version_id
            == Some(ResourceId::from_u64(scope.credit_model_version_id)),
        "loan quote catalog disagrees with the run model"
    );
    let Some(product) = catalog
        .products
        .into_iter()
        .find(|product| product.id == command.product_version_id)
    else {
        return Ok(LoanQuoteCreation::Rejected(LifeFailureCode::InvalidCommand));
    };
    if product.kind != LoanProductKind::UnsecuredLoan
        || !product.quote_eligible
        || !product.execution_eligible
        || !product.dsr_included
        || product.day_count_rule != LoanDayCountRule::Actual365
        || product.payment_calendar != LoanPaymentCalendar::MonthEnd
        || product.grace_months != 0
        || !(product.minimum_principal_krw..=product.maximum_principal_krw)
            .contains(&command.principal_krw)
    {
        return Ok(LoanQuoteCreation::Rejected(LifeFailureCode::InvalidCommand));
    }
    if product.rate_status != LoanRateStatus::Available {
        return Ok(LoanQuoteCreation::Rejected(
            LifeFailureCode::RateUnavailable,
        ));
    }
    let Some(annual_rate_bp) = product.current_annual_rate_bp else {
        return Ok(LoanQuoteCreation::Rejected(
            LifeFailureCode::RateUnavailable,
        ));
    };
    let verified_income =
        read_verified_annual_income_in_tx(tx, scope.save_id, scope.run_revision, scope.game_day)
            .await?;
    let credit_band_raw: String = sqlx::query_scalar(
        "SELECT credit_band FROM credit_state
         WHERE save_id = ? AND run_revision = ? AND credit_model_version_id = ?",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.credit_model_version_id)
    .fetch_one(&mut **tx)
    .await?;
    let credit_band = parse_credit_band(&credit_band_raw)?;
    sqlx::query(
        "SELECT id FROM household
         WHERE save_id = ? AND run_revision = ? AND id = ? FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.household_id)
    .fetch_one(&mut **tx)
    .await?;
    let loans: Vec<QuoteLoanRow> = sqlx::query_as(
        "SELECT id, status, product_kind, rate_type, current_annual_rate_bp,
                repayment_method, term_months, day_count_denominator,
                remaining_principal_krw,
                CAST(interest_remainder_numerator AS CHAR) AS interest_remainder_numerator,
                dsr_included, read_only
         FROM loan_contract
         WHERE save_id = ? AND run_revision = ?
           AND status IN ('pending', 'active', 'delinquent', 'defaulted', 'restructured')
         ORDER BY id
         LIMIT 9
         FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        loans.len() <= MAX_ACTIVE_LOANS,
        "active loan contracts exceed the run invariant"
    );
    let existing_loan_balance_krw = loans.iter().try_fold(0_i64, |total, loan| {
        let kind = parse_product_kind(&loan.product_kind)?;
        if loan.dsr_included
            && matches!(
                kind,
                LoanProductKind::StudentLoan
                    | LoanProductKind::UnsecuredLoan
                    | LoanProductKind::Mortgage
            )
        {
            total
                .checked_add(loan.remaining_principal_krw)
                .context("existing general-loan balance overflowed")
        } else {
            Ok(total)
        }
    })?;
    let post_execution_balance_krw = existing_loan_balance_krw
        .checked_add(command.principal_krw)
        .context("post-execution loan balance overflowed")?;
    let periods =
        build_month_end_periods(scope.world_start_date, scope.game_day, product.term_months)?;
    let schedule_periods = periods
        .iter()
        .map(|period| period.calculation)
        .collect::<Vec<_>>();
    let schedule = create_loan_rules().build_schedule(LoanScheduleInput {
        principal_krw: command.principal_krw,
        initial_annual_rate_bp: annual_rate_bp,
        day_count: ACTUAL_365_DAY_COUNT,
        repayment_method: product.repayment_method,
        prior_interest_remainder_numerator: 0,
        periods: &schedule_periods,
        rate_resets: &[],
    })?;
    let first = schedule
        .installments
        .first()
        .context("loan quote schedule has no first installment")?;
    let quoted_terms = LoanQuotedTermsState {
        annual_rate_bp,
        repayment_method: product.repayment_method,
        term_months: product.term_months,
        first_installment: LoanQuoteFirstInstallmentState {
            due_game_day: first.due_game_day,
            fee_krw: 0,
            principal_krw: first.principal_krw,
            interest_krw: first.interest_krw,
            total_krw: first.payment_krw,
        },
    };
    let mut restricted_reasons = Vec::new();
    if super::insolvency::credit_restricted_in_tx(
        tx,
        scope.save_id,
        scope.run_revision,
        scope.game_day,
    )
    .await?
    {
        restricted_reasons.push(LoanQuoteReasonState::InsolvencyRebuilding);
    }
    if loans.iter().any(|loan| loan.status == "defaulted") {
        restricted_reasons.push(LoanQuoteReasonState::ActiveDefault);
    }
    if loans.iter().any(|loan| loan.status == "delinquent") {
        restricted_reasons.push(LoanQuoteReasonState::ActiveDelinquency);
    }
    if loans.iter().any(|loan| loan.status == "restructured") {
        restricted_reasons.push(LoanQuoteReasonState::ActiveRestructuring);
    }
    if !eligibility.allowed_credit_bands.contains(&credit_band) {
        restricted_reasons.push(LoanQuoteReasonState::CreditBandRestricted);
    }
    if loans.len() >= eligibility.maximum_active_contracts {
        restricted_reasons.push(LoanQuoteReasonState::ActiveLoanLimit);
    }
    ensure!(
        loans
            .iter()
            .all(|loan| parse_contract_status(&loan.status).is_ok()),
        "loan quote encountered an unknown contract status"
    );

    let (decision_code, decision_reasons, dsr_applied, dsr, stress_rate_bp) =
        if restricted_reasons.is_empty() {
            assess_loan_quote_dsr(
                tx,
                &scope,
                &product,
                &loans,
                &schedule_periods,
                command.principal_krw,
                verified_income.map(|income| income.annual_income_krw),
            )
            .await?
        } else {
            (
                LoanQuoteDecisionState::CreditRestricted,
                restricted_reasons,
                false,
                None,
                0,
            )
        };
    let decision_reasons_json = serde_json::to_string(&decision_reasons)
        .context("loan quote reasons could not be serialized")?;
    let quoted_terms_json =
        serde_json::to_string(&quoted_terms).context("loan quote terms could not be serialized")?;
    let inserted = sqlx::query(
        "INSERT INTO loan_quote
             (save_id, run_revision, household_id, credit_model_version_id, purpose,
              loan_product_version_id, command_id, payload_sha256,
              expected_state_revision, created_game_day, expires_game_day,
              requested_principal_krw, verified_annual_income_krw,
              verified_income_source, existing_loan_balance_krw,
              post_execution_balance_krw, dsr_numerator_krw,
              dsr_denominator_krw, dsr_ratio_ppm, dsr_limit_ppm,
              stress_rate_bp, decision_code, decision_reasons, quoted_terms)
         VALUES (?, ?, ?, ?, 'unsecured', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.household_id)
    .bind(scope.credit_model_version_id)
    .bind(command.product_version_id.get())
    .bind(command.command_id.as_str())
    .bind(fingerprint)
    .bind(command.cursor.expected_state_revision)
    .bind(scope.game_day)
    .bind(scope.game_day)
    .bind(command.principal_krw)
    .bind(verified_income.map(|income| income.annual_income_krw))
    .bind(verified_income.map(|_| "activeEmploymentContract"))
    .bind(existing_loan_balance_krw)
    .bind(post_execution_balance_krw)
    .bind(dsr.map(|value| value.numerator_krw))
    .bind(dsr.map(|value| value.denominator_krw))
    .bind(dsr.map(|value| value.ratio_ppm))
    .bind(dsr.map(|value| value.limit_ppm))
    .bind(stress_rate_bp)
    .bind(loan_quote_decision_db(decision_code))
    .bind(&decision_reasons_json)
    .bind(&quoted_terms_json)
    .execute(&mut **tx)
    .await?;
    let receipt = LoanQuoteReceipt {
        command_id: command.command_id.clone(),
        quote_id: ResourceId::from_u64(inserted.last_insert_id()),
        product_version_id: command.product_version_id,
        requested_principal_krw: command.principal_krw,
        created_game_day: scope.game_day,
        expires_game_day: scope.game_day,
        decision_code,
        decision_reasons,
        verified_annual_income_krw: verified_income.map(|income| income.annual_income_krw),
        verified_income_source: verified_income.map(|income| income.source),
        existing_loan_balance_krw,
        post_execution_balance_krw,
        dsr_applied,
        dsr,
        stress_rate_bp,
        quoted_terms,
        replayed: false,
    };
    Ok(LoanQuoteCreation::Applied(Box::new(receipt)))
}

pub(super) async fn create_lease_deposit_loan_quote_in_tx(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
    command: &CreateLeaseDepositLoanQuoteCommand,
    fingerprint: &str,
) -> Result<LeaseDepositLoanQuoteCreation> {
    if command.offer_kind != HousingLeaseOfferKind::Jeonse || command.principal_krw <= 0 {
        return Ok(LeaseDepositLoanQuoteCreation::Rejected(
            LifeFailureCode::InvalidCommand,
        ));
    }
    let save_id: Option<u64> =
        sqlx::query_scalar("SELECT id FROM save WHERE user_id = ? AND run_revision = ?")
            .bind(user_id)
            .bind(command.cursor.expected_run_revision)
            .fetch_optional(&mut **tx)
            .await?;
    let Some(save_id) = save_id else {
        return Ok(LeaseDepositLoanQuoteCreation::Rejected(
            LifeFailureCode::Busy,
        ));
    };
    let scope = read_loan_run_scope(tx, save_id, command.cursor.expected_run_revision).await?;
    let assessment = match assess_lease_deposit_loan_application_in_tx(
        tx,
        user_id,
        &scope,
        command.listing_id,
        command.product_version_id,
        command.principal_krw,
    )
    .await?
    {
        LeaseDepositLoanAssessmentResult::Assessed(assessment) => *assessment,
        LeaseDepositLoanAssessmentResult::Rejected(code) => {
            return Ok(LeaseDepositLoanQuoteCreation::Rejected(code));
        }
    };
    let decision_reasons_json = serde_json::to_string(&assessment.decision_reasons)
        .context("lease-deposit quote reasons could not be serialized")?;
    let quoted_terms_json = serde_json::to_string(&assessment.quoted_terms)
        .context("lease-deposit quote terms could not be serialized")?;
    let inserted = sqlx::query(
        "INSERT INTO loan_quote
             (save_id, run_revision, household_id, credit_model_version_id, purpose,
              loan_product_version_id, command_id, payload_sha256,
              expected_state_revision, created_game_day, expires_game_day,
              requested_principal_krw, verified_annual_income_krw,
              verified_income_source, existing_loan_balance_krw,
              post_execution_balance_krw, dsr_numerator_krw,
              dsr_denominator_krw, dsr_ratio_ppm, dsr_limit_ppm,
              stress_rate_bp, decision_code, decision_reasons, quoted_terms,
              property_listing_id, lease_deposit_krw, funding_limit_ppm,
              maximum_funding_krw, replaced_loan_contract_id,
              replaced_loan_principal_krw, regulatory_dsr_applied,
              affordability_numerator_krw, affordability_denominator_krw,
              affordability_ratio_ppm, affordability_limit_ppm)
         VALUES (?, ?, ?, ?, 'leaseDeposit', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                 NULL, NULL, NULL, NULL, 0, ?, ?, ?, ?, ?, ?, ?, ?, ?, FALSE,
                 ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.household_id)
    .bind(scope.credit_model_version_id)
    .bind(command.product_version_id.get())
    .bind(command.command_id.as_str())
    .bind(fingerprint)
    .bind(command.cursor.expected_state_revision)
    .bind(scope.game_day)
    .bind(scope.game_day)
    .bind(command.principal_krw)
    .bind(assessment.verified_annual_income_krw)
    .bind(
        assessment
            .verified_income_source
            .map(|_| "activeEmploymentContract"),
    )
    .bind(assessment.existing_loan_balance_krw)
    .bind(assessment.post_execution_balance_krw)
    .bind(lease_deposit_quote_decision_db(assessment.decision_code))
    .bind(&decision_reasons_json)
    .bind(&quoted_terms_json)
    .bind(command.listing_id.get())
    .bind(assessment.deposit_krw)
    .bind(assessment.funding_limit_ppm)
    .bind(assessment.maximum_funding_krw)
    .bind(assessment.replaced_loan_id.map(ResourceId::get))
    .bind(assessment.replaced_loan_principal_krw)
    .bind(assessment.affordability.map(|value| value.numerator_krw))
    .bind(assessment.affordability.map(|value| value.denominator_krw))
    .bind(assessment.affordability.map(|value| value.ratio_ppm))
    .bind(assessment.affordability.map(|value| value.limit_ppm))
    .execute(&mut **tx)
    .await?;
    let receipt = LeaseDepositLoanQuoteReceipt {
        command_id: command.command_id.clone(),
        quote_id: ResourceId::from_u64(inserted.last_insert_id()),
        listing_id: assessment.listing_id,
        offer_kind: command.offer_kind,
        product_version_id: command.product_version_id,
        requested_principal_krw: command.principal_krw,
        deposit_krw: assessment.deposit_krw,
        funding_limit_ppm: assessment.funding_limit_ppm,
        maximum_funding_krw: assessment.maximum_funding_krw,
        created_game_day: scope.game_day,
        expires_game_day: scope.game_day,
        decision_code: assessment.decision_code,
        decision_reasons: assessment.decision_reasons,
        verified_annual_income_krw: assessment.verified_annual_income_krw,
        verified_income_source: assessment.verified_income_source,
        existing_loan_balance_krw: assessment.existing_loan_balance_krw,
        post_execution_balance_krw: assessment.post_execution_balance_krw,
        regulatory_dsr_applied: false,
        affordability: assessment.affordability,
        quoted_terms: assessment.quoted_terms,
        replaced_loan_id: assessment.replaced_loan_id,
        replaced_loan_principal_krw: assessment.replaced_loan_principal_krw,
        replayed: false,
    };
    Ok(LeaseDepositLoanQuoteCreation::Applied(Box::new(receipt)))
}

const fn lease_deposit_quote_decision_db(
    decision: LeaseDepositLoanQuoteDecisionState,
) -> &'static str {
    match decision {
        LeaseDepositLoanQuoteDecisionState::Eligible => "eligible",
        LeaseDepositLoanQuoteDecisionState::CreditRestricted => "creditRestricted",
        LeaseDepositLoanQuoteDecisionState::CollateralLimit => "collateralLimit",
        LeaseDepositLoanQuoteDecisionState::IncomeUnavailable => "incomeUnavailable",
        LeaseDepositLoanQuoteDecisionState::AffordabilityLimit => "affordabilityLimit",
    }
}

async fn assess_lease_deposit_loan_application_in_tx(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
    scope: &LoanRunScopeRow,
    listing_id: ResourceId,
    product_version_id: ResourceId,
    principal_krw: i64,
) -> Result<LeaseDepositLoanAssessmentResult> {
    let Some(eligibility) = parse_lease_deposit_quote_eligibility(&scope.model_parameters_json)?
    else {
        return Ok(LeaseDepositLoanAssessmentResult::Rejected(
            LifeFailureCode::RateUnavailable,
        ));
    };
    let catalog = read_loan_product_catalog_in_tx(tx, user_id).await?;
    ensure!(
        catalog.credit_model_version_id
            == Some(ResourceId::from_u64(scope.credit_model_version_id)),
        "lease-deposit catalog disagrees with the run model"
    );
    let Some(product) = catalog
        .products
        .into_iter()
        .find(|product| product.id == product_version_id)
    else {
        return Ok(LeaseDepositLoanAssessmentResult::Rejected(
            LifeFailureCode::InvalidCommand,
        ));
    };
    if principal_krw <= 0
        || product.kind != LoanProductKind::LeaseDepositLoan
        || product.starting_eligible
        || !product.quote_eligible
        || !product.execution_eligible
        || !product.prepayment_allowed
        || product.dsr_included
        || product.lender_sector != LoanLenderSector::Bank
        || product.rate_type != LoanRateType::Fixed
        || product.rate_status != LoanRateStatus::Available
        || product.current_annual_rate_bp.is_none()
        || product.day_count_rule != LoanDayCountRule::Actual365
        || product.repayment_method != LoanRepaymentMethod::Bullet
        || product.payment_calendar != LoanPaymentCalendar::MonthEnd
        || product.grace_months != 0
        || product.minimum_principal_krw <= 0
        || product.maximum_principal_krw < product.minimum_principal_krw
        || product.prepayment_fee_ppm != 0
        || product.prepayment_effect != LoanPrepaymentEffect::ReduceTerm
        || !(product.minimum_principal_krw..=product.maximum_principal_krw).contains(&principal_krw)
    {
        return Ok(LeaseDepositLoanAssessmentResult::Rejected(
            LifeFailureCode::InvalidCommand,
        ));
    }
    let product_policy: LeaseDepositProductPolicyRow = sqlx::query_as(
        "SELECT execution_channel, funding_limit_ppm, affordability_rule,
                affordability_limit_ppm, regulatory_dsr_treatment
         FROM loan_product_version
         WHERE id = ? AND credit_model_version_id = ? AND product_kind = 'leaseDepositLoan'
           AND catalog_scope = 'modelChild' AND sealed_at IS NOT NULL",
    )
    .bind(product.id.get())
    .bind(scope.credit_model_version_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("lease-deposit product policy is missing")?;
    let funding_limit_ppm = i64::from(
        product_policy
            .funding_limit_ppm
            .context("lease-deposit funding limit is missing")?,
    );
    let affordability_limit_ppm = i64::from(
        product_policy
            .affordability_limit_ppm
            .context("lease-deposit affordability limit is missing")?,
    );
    ensure!(
        product_policy.execution_channel == LEASE_DEPOSIT_EXECUTION_CHANNEL
            && (1..=1_000_000).contains(&funding_limit_ppm)
            && product_policy.affordability_rule.as_deref()
                == Some(LEASE_DEPOSIT_AFFORDABILITY_RULE)
            && affordability_limit_ppm == eligibility.maximum_affordability_ratio_ppm
            && (1..=1_000_000).contains(&affordability_limit_ppm)
            && product_policy.regulatory_dsr_treatment.as_deref()
                == Some(LEASE_DEPOSIT_REGULATORY_DSR_TREATMENT),
        "lease-deposit product policy is unsupported"
    );

    sqlx::query(
        "SELECT id FROM household
         WHERE save_id = ? AND run_revision = ? AND id = ? FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.household_id)
    .fetch_one(&mut **tx)
    .await?;
    let linked_rows: Vec<LinkedDepositLoanRow> = sqlx::query_as(
        "SELECT lease.id AS lease_contract_id, lease.deposit_krw AS lease_deposit_krw,
                loan.id AS loan_contract_id, loan.product_kind, loan.status,
                loan.remaining_principal_krw, loan.accrued_interest_krw,
                loan.accrued_fee_krw,
                CASE WHEN loan.id IS NULL THEN NULL ELSE CAST(COALESCE((
                    SELECT SUM(bucket.original_amount_krw - bucket.paid_amount_krw)
                    FROM loan_obligation_bucket AS bucket
                    WHERE bucket.save_id = loan.save_id
                      AND bucket.run_revision = loan.run_revision
                      AND bucket.loan_contract_id = loan.id
                      AND bucket.status IN ('pending', 'delinquent')
                      AND bucket.paid_amount_krw < bucket.original_amount_krw
                ), 0) AS SIGNED) END AS overdue_krw
         FROM lease_contract AS lease
         LEFT JOIN loan_contract AS loan
           ON loan.save_id = lease.save_id
          AND loan.run_revision = lease.run_revision
          AND loan.lease_contract_id = lease.id
         WHERE lease.save_id = ? AND lease.run_revision = ?
           AND lease.household_id = ? AND lease.role = 'tenant'
           AND lease.effective_to_game_day IS NULL
         ORDER BY lease.id, loan.id
         FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.household_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        linked_rows.len() <= 1,
        "active lease has multiple linked deposit loans"
    );
    let (replaced_loan_id, replaced_active_loan_id, replaced_loan_principal_krw) =
        match resolve_quote_replacement(linked_rows.first())? {
            QuoteReplacementResolution::None => (None, None, 0),
            QuoteReplacementResolution::Active {
                loan_id,
                principal_krw,
            } => (Some(loan_id), Some(loan_id), principal_krw),
            QuoteReplacementResolution::ContractConflict => {
                return Ok(LeaseDepositLoanAssessmentResult::Rejected(
                    LifeFailureCode::ContractConflict,
                ));
            }
        };

    let current_date = scope
        .world_start_date
        .checked_add(Duration::days(i64::from(scope.game_day)))
        .context("lease-deposit listing date overflowed")?;
    let current_month = Date::from_calendar_date(current_date.year(), current_date.month(), 1)
        .context("lease-deposit listing month is invalid")?;
    let listing: Option<LeaseDepositListingRow> = sqlx::query_as(
        "SELECT listing.id, listing.market_world_id,
                listing.real_estate_model_version_id, listing.`year_month`,
                listing.available_from_game_day, listing.available_to_game_day,
                offer.offer_kind,
                CAST(offer.deposit_krw AS SIGNED) AS deposit_krw
         FROM run_rule_bundle AS bundle
         INNER JOIN real_estate_model_version AS model
           ON model.id = bundle.real_estate_model_version_id
          AND model.version_key IN (
              'dev-unranked-m4-real-estate-lifecycle-2026-v4',
              'dev-unranked-m4-real-estate-purchase-2026-v5'
          )
          AND model.availability = 'active' AND model.sealed_at IS NOT NULL
         INNER JOIN real_estate_model_strict_manifest AS manifest
           ON manifest.real_estate_model_version_id = model.id
          AND BINARY manifest.canonical_sha256 = BINARY model.canonical_sha256
         INNER JOIN property_listing AS listing
           ON listing.id = ?
          AND listing.market_world_id = bundle.market_world_id
          AND listing.real_estate_model_version_id = model.id
         INNER JOIN property_listing_offer AS offer
           ON offer.property_listing_id = listing.id
          AND offer.offer_kind = 'jeonse'
          AND offer.price_krw IS NULL AND offer.monthly_rent_krw IS NULL
          AND offer.deposit_krw > 0
         WHERE bundle.save_id = ? AND bundle.run_revision = ?
           AND bundle.market_world_id = ?
           AND bundle.real_estate_model_version_id = ?
         FOR UPDATE",
    )
    .bind(listing_id.get())
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.market_world_id)
    .bind(scope.real_estate_model_version_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(listing) = listing else {
        return Ok(LeaseDepositLoanAssessmentResult::Rejected(
            LifeFailureCode::ContractConflict,
        ));
    };
    if listing.id != listing_id.get()
        || listing.market_world_id != scope.market_world_id
        || listing.real_estate_model_version_id != scope.real_estate_model_version_id
        || listing.year_month != current_month
        || listing.offer_kind != "jeonse"
        || !(listing.available_from_game_day..=listing.available_to_game_day)
            .contains(&scope.game_day)
    {
        return Ok(LeaseDepositLoanAssessmentResult::Rejected(
            LifeFailureCode::ContractConflict,
        ));
    }
    let funding = create_loan_rules().calculate_lease_deposit_funding_limit(
        LeaseDepositFundingLimitInput {
            deposit_krw: listing.deposit_krw,
            funding_limit_ppm,
            product_maximum_principal_krw: product.maximum_principal_krw,
        },
    )?;

    let verified_income =
        read_verified_annual_income_in_tx(tx, scope.save_id, scope.run_revision, scope.game_day)
            .await?;
    let credit_band_raw: String = sqlx::query_scalar(
        "SELECT credit_band FROM credit_state
         WHERE save_id = ? AND run_revision = ? AND credit_model_version_id = ?",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.credit_model_version_id)
    .fetch_one(&mut **tx)
    .await?;
    let credit_band = parse_credit_band(&credit_band_raw)?;
    let loans: Vec<QuoteLoanRow> = sqlx::query_as(
        "SELECT id, status, product_kind, rate_type, current_annual_rate_bp,
                repayment_method, term_months, day_count_denominator,
                remaining_principal_krw,
                CAST(interest_remainder_numerator AS CHAR) AS interest_remainder_numerator,
                dsr_included, read_only
         FROM loan_contract
         WHERE save_id = ? AND run_revision = ?
           AND status IN ('pending', 'active', 'delinquent', 'defaulted', 'restructured')
         ORDER BY id
         LIMIT 9
         FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        loans.len() <= MAX_ACTIVE_LOANS,
        "active loan contracts exceed the run invariant"
    );
    if let Some(replaced_active_loan_id) = replaced_active_loan_id {
        ensure!(
            loans
                .iter()
                .any(|loan| loan.id == replaced_active_loan_id.get()),
            "active linked deposit loan is missing from the credit lock set"
        );
    }
    let existing_loan_balance_krw = loans.iter().try_fold(0_i64, |total, loan| {
        total
            .checked_add(loan.remaining_principal_krw)
            .context("lease-deposit existing balance overflowed")
    })?;
    let post_execution_balance_krw = existing_loan_balance_krw
        .checked_sub(replaced_loan_principal_krw)
        .context("lease-deposit replacement balance underflowed")?
        .checked_add(principal_krw)
        .context("lease-deposit post-execution balance overflowed")?;
    let periods =
        build_month_end_periods(scope.world_start_date, scope.game_day, product.term_months)?;
    let schedule_periods = periods
        .iter()
        .map(|period| period.calculation)
        .collect::<Vec<_>>();
    let schedule = create_loan_rules().build_schedule(LoanScheduleInput {
        principal_krw,
        initial_annual_rate_bp: product
            .current_annual_rate_bp
            .context("lease-deposit product lost its current rate")?,
        day_count: ACTUAL_365_DAY_COUNT,
        repayment_method: LoanRepaymentMethod::Bullet,
        prior_interest_remainder_numerator: 0,
        periods: &schedule_periods,
        rate_resets: &[],
    })?;
    let first = schedule
        .installments
        .first()
        .context("lease-deposit quote schedule has no first installment")?;
    let quoted_terms = LoanQuotedTermsState {
        annual_rate_bp: product
            .current_annual_rate_bp
            .context("lease-deposit product lost its current rate")?,
        repayment_method: LoanRepaymentMethod::Bullet,
        term_months: product.term_months,
        first_installment: LoanQuoteFirstInstallmentState {
            due_game_day: first.due_game_day,
            fee_krw: 0,
            principal_krw: first.principal_krw,
            interest_krw: first.interest_krw,
            total_krw: first.payment_krw,
        },
    };

    let mut restricted_reasons = Vec::new();
    if super::insolvency::credit_restricted_in_tx(
        tx,
        scope.save_id,
        scope.run_revision,
        scope.game_day,
    )
    .await?
    {
        restricted_reasons.push(LeaseDepositLoanQuoteReasonState::InsolvencyRebuilding);
    }
    if loans.iter().any(|loan| loan.status == "defaulted") {
        restricted_reasons.push(LeaseDepositLoanQuoteReasonState::ActiveDefault);
    }
    if loans.iter().any(|loan| loan.status == "delinquent") {
        restricted_reasons.push(LeaseDepositLoanQuoteReasonState::ActiveDelinquency);
    }
    if loans.iter().any(|loan| loan.status == "restructured") {
        restricted_reasons.push(LeaseDepositLoanQuoteReasonState::ActiveRestructuring);
    }
    if !eligibility.allowed_credit_bands.contains(&credit_band) {
        restricted_reasons.push(LeaseDepositLoanQuoteReasonState::CreditBandRestricted);
    }
    let active_count_after_replacement = loans
        .len()
        .checked_sub(usize::from(replaced_active_loan_id.is_some()));
    ensure!(
        active_count_after_replacement.is_some(),
        "lease-deposit replacement count underflowed"
    );
    if active_count_after_replacement.unwrap_or_default() >= eligibility.maximum_active_contracts {
        restricted_reasons.push(LeaseDepositLoanQuoteReasonState::ActiveLoanLimit);
    }
    ensure!(
        loans
            .iter()
            .all(|loan| parse_contract_status(&loan.status).is_ok()),
        "lease-deposit quote encountered an unknown contract status"
    );

    let (decision_code, decision_reasons, affordability) = if !restricted_reasons.is_empty() {
        (
            LeaseDepositLoanQuoteDecisionState::CreditRestricted,
            restricted_reasons,
            None,
        )
    } else if principal_krw > funding.maximum_funding_krw {
        (
            LeaseDepositLoanQuoteDecisionState::CollateralLimit,
            vec![LeaseDepositLoanQuoteReasonState::CollateralLimit],
            None,
        )
    } else if verified_income.is_none() {
        (
            LeaseDepositLoanQuoteDecisionState::IncomeUnavailable,
            vec![LeaseDepositLoanQuoteReasonState::IncomeUnavailable],
            None,
        )
    } else {
        let affordability = assess_lease_deposit_affordability_in_tx(
            tx,
            scope,
            &loans,
            &schedule_periods,
            principal_krw,
            product
                .current_annual_rate_bp
                .context("lease-deposit product lost its current rate")?,
            verified_income.map(|income| income.annual_income_krw),
            affordability_limit_ppm,
            replaced_active_loan_id,
        )
        .await?;
        if affordability.ratio_ppm <= affordability.limit_ppm {
            (
                LeaseDepositLoanQuoteDecisionState::Eligible,
                vec![LeaseDepositLoanQuoteReasonState::Eligible],
                Some(affordability),
            )
        } else {
            (
                LeaseDepositLoanQuoteDecisionState::AffordabilityLimit,
                vec![LeaseDepositLoanQuoteReasonState::AffordabilityLimit],
                Some(affordability),
            )
        }
    };
    Ok(LeaseDepositLoanAssessmentResult::Assessed(Box::new(
        LeaseDepositLoanApplicationAssessment {
            product,
            periods,
            schedule,
            listing_id,
            deposit_krw: listing.deposit_krw,
            funding_limit_ppm,
            maximum_funding_krw: funding.maximum_funding_krw,
            decision_code,
            decision_reasons,
            verified_annual_income_krw: verified_income.map(|income| income.annual_income_krw),
            verified_income_source: verified_income.map(|income| income.source),
            existing_loan_balance_krw,
            post_execution_balance_krw,
            affordability,
            quoted_terms,
            replaced_loan_id,
            replaced_loan_principal_krw,
        },
    )))
}

fn resolve_quote_replacement(
    linked: Option<&LinkedDepositLoanRow>,
) -> Result<QuoteReplacementResolution> {
    let Some(linked) = linked else {
        return Ok(QuoteReplacementResolution::None);
    };
    let Some(loan_id) = linked.loan_contract_id else {
        ensure!(
            linked.product_kind.is_none()
                && linked.status.is_none()
                && linked.remaining_principal_krw.is_none()
                && linked.accrued_interest_krw.is_none()
                && linked.accrued_fee_krw.is_none()
                && linked.overdue_krw.is_none(),
            "unlinked lease exposes partial loan data"
        );
        return Ok(QuoteReplacementResolution::None);
    };
    ensure!(
        linked.lease_contract_id > 0
            && linked.lease_deposit_krw > 0
            && linked.product_kind.as_deref() == Some("leaseDepositLoan"),
        "active lease has an invalid linked loan"
    );
    let status = linked
        .status
        .as_deref()
        .context("linked loan status is missing")?;
    let principal = linked
        .remaining_principal_krw
        .context("linked loan principal is missing")?;
    let accrued_interest = linked
        .accrued_interest_krw
        .context("linked loan accrued interest is missing")?;
    let accrued_fee = linked
        .accrued_fee_krw
        .context("linked loan accrued fee is missing")?;
    let overdue = linked
        .overdue_krw
        .context("linked loan overdue balance is missing")?;
    let loan_id = ResourceId::from_u64(loan_id);
    match status {
        "active" => {
            if principal <= 0
                || principal > linked.lease_deposit_krw
                || accrued_interest != 0
                || accrued_fee != 0
                || overdue != 0
            {
                Ok(QuoteReplacementResolution::ContractConflict)
            } else {
                Ok(QuoteReplacementResolution::Active {
                    loan_id,
                    principal_krw: principal,
                })
            }
        }
        "paidOff" => {
            ensure!(
                principal == 0 && accrued_interest == 0 && accrued_fee == 0 && overdue == 0,
                "paid-off linked deposit loan retains a balance"
            );
            Ok(QuoteReplacementResolution::None)
        }
        "delinquent" | "defaulted" | "restructured" => Ok(QuoteReplacementResolution::None),
        _ => bail!("active lease has an unsupported linked-loan status"),
    }
}

#[allow(clippy::too_many_arguments)]
async fn assess_lease_deposit_affordability_in_tx(
    tx: &mut Transaction<'_, MySql>,
    scope: &LoanRunScopeRow,
    loans: &[QuoteLoanRow],
    candidate_periods: &[LoanSchedulePeriod],
    requested_principal_krw: i64,
    requested_annual_rate_bp: i64,
    verified_annual_income_krw: Option<i64>,
    maximum_ratio_ppm: i64,
    replaced_loan_id: Option<ResourceId>,
) -> Result<LeaseDepositLoanAffordabilityState> {
    let policy = read_dsr_policy_in_tx(tx, scope).await?;
    let owned = read_affordability_loans_in_tx(tx, scope, loans, replaced_loan_id).await?;
    let candidate_id = owned
        .iter()
        .map(|loan| loan.loan_id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .context("prospective lease-deposit loan id overflowed")?;
    let inputs = owned
        .iter()
        .map(|loan| DsrLoanInput {
            loan_id: loan.loan_id,
            included_in_dsr: loan.included_in_dsr,
            counts_toward_general_loan_balance: loan.counts_toward_general_loan_balance,
            counts_toward_credit_stress_balance: loan.counts_toward_credit_stress_balance,
            rate_type: loan.rate_type,
            fixed_rate_period_months: loan.fixed_rate_period_months,
            payment_treatment: loan.payment_treatment,
            schedule: LoanScheduleInput {
                principal_krw: loan.principal_krw,
                initial_annual_rate_bp: loan.initial_annual_rate_bp,
                day_count: ACTUAL_365_DAY_COUNT,
                repayment_method: loan.repayment_method,
                prior_interest_remainder_numerator: loan.prior_interest_remainder_numerator,
                periods: &loan.periods,
                rate_resets: &loan.rate_resets,
            },
        })
        .collect::<Vec<_>>();
    let evaluation_end_game_day =
        twelve_month_horizon_game_day(scope.world_start_date, scope.game_day)?;
    let assessment =
        create_loan_rules().assess_lease_deposit_affordability(LeaseDepositAffordabilityInput {
            evaluation_game_day: scope.game_day,
            evaluation_end_game_day,
            verified_annual_income_krw,
            maximum_ratio_ppm,
            stress_policy: DsrPolicy {
                general_loan_balance_gate_krw: policy.general_loan_balance_gate_krw,
                maximum_ratio_ppm,
                credit_balance_stress_gate_krw: policy.credit_balance_stress_gate_krw,
                base_stress_rate_bp: policy.base_stress_rate_bp,
                medium_fixed_stress_multiplier_ppm: policy.medium_fixed_stress_multiplier_ppm,
            },
            existing_loans: &inputs,
            new_loan: LeaseDepositAffordabilityNewLoanInput {
                loan_id: candidate_id,
                schedule: LoanScheduleInput {
                    principal_krw: requested_principal_krw,
                    initial_annual_rate_bp: requested_annual_rate_bp,
                    day_count: ACTUAL_365_DAY_COUNT,
                    repayment_method: LoanRepaymentMethod::Bullet,
                    prior_interest_remainder_numerator: 0,
                    periods: candidate_periods,
                    rate_resets: &[],
                },
            },
            replaced_loan_id: replaced_loan_id.map(ResourceId::get),
        })?;
    ensure!(
        assessment.replaced_loan_id == replaced_loan_id.map(ResourceId::get)
            && assessment.denominator_krw == verified_annual_income_krw.unwrap_or_default()
            && assessment.maximum_ratio_ppm == maximum_ratio_ppm
            && assessment.passed == (assessment.ratio_ppm <= maximum_ratio_ppm),
        "lease-deposit affordability assessment disagrees with its inputs"
    );
    Ok(LeaseDepositLoanAffordabilityState {
        numerator_krw: assessment.numerator_krw,
        denominator_krw: assessment.denominator_krw,
        ratio_ppm: assessment.ratio_ppm,
        limit_ppm: assessment.maximum_ratio_ppm,
    })
}

async fn read_affordability_loans_in_tx(
    tx: &mut Transaction<'_, MySql>,
    scope: &LoanRunScopeRow,
    loans: &[QuoteLoanRow],
    replaced_loan_id: Option<ResourceId>,
) -> Result<Vec<OwnedDsrLoan>> {
    let mut result = Vec::new();
    for loan in loans
        .iter()
        .filter(|loan| loan.dsr_included || replaced_loan_id.is_some_and(|id| id.get() == loan.id))
    {
        let status = parse_contract_status(&loan.status)?;
        ensure!(
            matches!(
                status,
                LoanContractStatus::Pending | LoanContractStatus::Active
            ),
            "restricted loan reached lease-deposit affordability"
        );
        let product_kind = parse_product_kind(&loan.product_kind)?;
        ensure!(
            matches!(
                product_kind,
                LoanProductKind::StudentLoan
                    | LoanProductKind::UnsecuredLoan
                    | LoanProductKind::Mortgage
                    | LoanProductKind::LeaseDepositLoan
            ),
            "unsupported loan kind reached lease-deposit affordability"
        );
        if product_kind == LoanProductKind::LeaseDepositLoan {
            ensure!(
                replaced_loan_id.is_some_and(|id| id.get() == loan.id) && !loan.dsr_included,
                "non-replacement lease-deposit loan reached affordability"
            );
        } else {
            ensure!(
                loan.dsr_included,
                "general loan is excluded from affordability"
            );
        }
        ensure!(
            !loan.read_only
                && loan.remaining_principal_krw > 0
                && loan.day_count_denominator == Some(ACTUAL_365_DAY_COUNT),
            "affordability loan balance or servicing shape is invalid"
        );
        let installments: Vec<QuoteInstallmentRow> = sqlx::query_as(
            "SELECT installment_no, due_game_day, elapsed_days, annual_rate_bp
             FROM loan_installment
             WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
               AND status IN ('pending', 'due', 'partiallyPaid')
               AND due_game_day > ?
             ORDER BY installment_no
             FOR UPDATE",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(loan.id)
        .bind(scope.game_day)
        .fetch_all(&mut **tx)
        .await?;
        ensure!(
            !installments.is_empty()
                && installments.windows(2).all(|pair| {
                    pair[0].installment_no.checked_add(1) == Some(pair[1].installment_no)
                }),
            "affordability loan installments are not contiguous"
        );
        let initial_annual_rate_bp = i64::from(
            installments
                .first()
                .context("affordability loan has no first installment")?
                .annual_rate_bp,
        );
        ensure!(
            loan.current_annual_rate_bp.map(i64::from) == Some(initial_annual_rate_bp),
            "affordability loan current rate disagrees with its schedule"
        );
        let mut rate_resets = Vec::new();
        for (index, pair) in installments.windows(2).enumerate() {
            if pair[0].annual_rate_bp != pair[1].annual_rate_bp {
                rate_resets.push(LoanRateReset {
                    after_installment_sequence: u16::try_from(index + 1)
                        .context("affordability rate reset index overflowed")?,
                    next_annual_rate_bp: i64::from(pair[1].annual_rate_bp),
                });
            }
        }
        let rate_type = parse_rate_type(&loan.rate_type)?;
        let repayment_method = parse_repayment_method(&loan.repayment_method)?;
        let term_months = loan
            .term_months
            .context("affordability loan term is missing")?;
        result.push(OwnedDsrLoan {
            loan_id: loan.id,
            included_in_dsr: loan.dsr_included,
            counts_toward_general_loan_balance: matches!(
                product_kind,
                LoanProductKind::StudentLoan
                    | LoanProductKind::UnsecuredLoan
                    | LoanProductKind::Mortgage
            ),
            counts_toward_credit_stress_balance: product_kind == LoanProductKind::UnsecuredLoan,
            rate_type,
            fixed_rate_period_months: if rate_type == LoanRateType::Fixed {
                term_months
            } else {
                0
            },
            payment_treatment: if product_kind == LoanProductKind::UnsecuredLoan
                && repayment_method == LoanRepaymentMethod::Bullet
            {
                DsrPaymentTreatment::BulletCreditFiveYear
            } else {
                DsrPaymentTreatment::Scheduled
            },
            principal_krw: loan.remaining_principal_krw,
            initial_annual_rate_bp,
            repayment_method,
            prior_interest_remainder_numerator: loan
                .interest_remainder_numerator
                .parse()
                .context("affordability loan interest remainder is invalid")?,
            periods: installments
                .iter()
                .map(|installment| LoanSchedulePeriod {
                    due_game_day: installment.due_game_day,
                    elapsed_days: installment.elapsed_days,
                })
                .collect(),
            rate_resets,
        });
    }
    Ok(result)
}

async fn assess_loan_application_in_tx(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
    scope: &LoanRunScopeRow,
    product_version_id: ResourceId,
    principal_krw: i64,
) -> Result<LoanApplicationAssessmentResult> {
    if principal_krw <= 0 {
        return Ok(LoanApplicationAssessmentResult::Rejected(
            LifeFailureCode::InvalidCommand,
        ));
    }
    let Some(eligibility) = parse_quote_eligibility(&scope.model_parameters_json)? else {
        return Ok(LoanApplicationAssessmentResult::Rejected(
            LifeFailureCode::RateUnavailable,
        ));
    };
    let catalog = read_loan_product_catalog_in_tx(tx, user_id).await?;
    ensure!(
        catalog.credit_model_version_id
            == Some(ResourceId::from_u64(scope.credit_model_version_id)),
        "loan application catalog disagrees with the run model"
    );
    let Some(product) = catalog
        .products
        .into_iter()
        .find(|product| product.id == product_version_id)
    else {
        return Ok(LoanApplicationAssessmentResult::Rejected(
            LifeFailureCode::InvalidCommand,
        ));
    };
    if product.kind != LoanProductKind::UnsecuredLoan
        || !product.quote_eligible
        || !product.execution_eligible
        || !product.dsr_included
        || product.day_count_rule != LoanDayCountRule::Actual365
        || product.payment_calendar != LoanPaymentCalendar::MonthEnd
        || product.grace_months != 0
        || !(product.minimum_principal_krw..=product.maximum_principal_krw).contains(&principal_krw)
    {
        return Ok(LoanApplicationAssessmentResult::Rejected(
            LifeFailureCode::InvalidCommand,
        ));
    }
    if product.rate_status != LoanRateStatus::Available {
        return Ok(LoanApplicationAssessmentResult::Rejected(
            LifeFailureCode::RateUnavailable,
        ));
    }
    let Some(annual_rate_bp) = product.current_annual_rate_bp else {
        return Ok(LoanApplicationAssessmentResult::Rejected(
            LifeFailureCode::RateUnavailable,
        ));
    };
    let verified_income =
        read_verified_annual_income_in_tx(tx, scope.save_id, scope.run_revision, scope.game_day)
            .await?;
    let credit_band_raw: String = sqlx::query_scalar(
        "SELECT credit_band FROM credit_state
         WHERE save_id = ? AND run_revision = ? AND credit_model_version_id = ?",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.credit_model_version_id)
    .fetch_one(&mut **tx)
    .await?;
    let credit_band = parse_credit_band(&credit_band_raw)?;
    sqlx::query(
        "SELECT id FROM household
         WHERE save_id = ? AND run_revision = ? AND id = ? FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.household_id)
    .fetch_one(&mut **tx)
    .await?;
    let loans: Vec<QuoteLoanRow> = sqlx::query_as(
        "SELECT id, status, product_kind, rate_type, current_annual_rate_bp,
                repayment_method, term_months, day_count_denominator,
                remaining_principal_krw,
                CAST(interest_remainder_numerator AS CHAR) AS interest_remainder_numerator,
                dsr_included, read_only
         FROM loan_contract
         WHERE save_id = ? AND run_revision = ?
           AND status IN ('pending', 'active', 'delinquent', 'defaulted', 'restructured')
         ORDER BY id
         LIMIT 9
         FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        loans.len() <= MAX_ACTIVE_LOANS,
        "active loan contracts exceed the run invariant"
    );
    let existing_loan_balance_krw = loans.iter().try_fold(0_i64, |total, loan| {
        let kind = parse_product_kind(&loan.product_kind)?;
        if loan.dsr_included
            && matches!(
                kind,
                LoanProductKind::StudentLoan
                    | LoanProductKind::UnsecuredLoan
                    | LoanProductKind::Mortgage
            )
        {
            total
                .checked_add(loan.remaining_principal_krw)
                .context("existing general-loan balance overflowed")
        } else {
            Ok(total)
        }
    })?;
    let _post_execution_balance_krw = existing_loan_balance_krw
        .checked_add(principal_krw)
        .context("post-execution loan balance overflowed")?;
    let periods =
        build_month_end_periods(scope.world_start_date, scope.game_day, product.term_months)?;
    let schedule_periods = periods
        .iter()
        .map(|period| period.calculation)
        .collect::<Vec<_>>();
    let schedule = create_loan_rules().build_schedule(LoanScheduleInput {
        principal_krw,
        initial_annual_rate_bp: annual_rate_bp,
        day_count: ACTUAL_365_DAY_COUNT,
        repayment_method: product.repayment_method,
        prior_interest_remainder_numerator: 0,
        periods: &schedule_periods,
        rate_resets: &[],
    })?;
    let first = schedule
        .installments
        .first()
        .context("loan application schedule has no first installment")?;
    let quoted_terms = LoanQuotedTermsState {
        annual_rate_bp,
        repayment_method: product.repayment_method,
        term_months: product.term_months,
        first_installment: LoanQuoteFirstInstallmentState {
            due_game_day: first.due_game_day,
            fee_krw: 0,
            principal_krw: first.principal_krw,
            interest_krw: first.interest_krw,
            total_krw: first.payment_krw,
        },
    };
    let mut restricted_reasons = Vec::new();
    if super::insolvency::credit_restricted_in_tx(
        tx,
        scope.save_id,
        scope.run_revision,
        scope.game_day,
    )
    .await?
    {
        restricted_reasons.push(LoanQuoteReasonState::InsolvencyRebuilding);
    }
    if loans.iter().any(|loan| loan.status == "defaulted") {
        restricted_reasons.push(LoanQuoteReasonState::ActiveDefault);
    }
    if loans.iter().any(|loan| loan.status == "delinquent") {
        restricted_reasons.push(LoanQuoteReasonState::ActiveDelinquency);
    }
    if loans.iter().any(|loan| loan.status == "restructured") {
        restricted_reasons.push(LoanQuoteReasonState::ActiveRestructuring);
    }
    if !eligibility.allowed_credit_bands.contains(&credit_band) {
        restricted_reasons.push(LoanQuoteReasonState::CreditBandRestricted);
    }
    if loans.len() >= eligibility.maximum_active_contracts {
        restricted_reasons.push(LoanQuoteReasonState::ActiveLoanLimit);
    }
    ensure!(
        loans
            .iter()
            .all(|loan| parse_contract_status(&loan.status).is_ok()),
        "loan application encountered an unknown contract status"
    );
    let (decision_code, decision_reasons, _, _, _) = if restricted_reasons.is_empty() {
        assess_loan_quote_dsr(
            tx,
            scope,
            &product,
            &loans,
            &schedule_periods,
            principal_krw,
            verified_income.map(|income| income.annual_income_krw),
        )
        .await?
    } else {
        (
            LoanQuoteDecisionState::CreditRestricted,
            restricted_reasons,
            false,
            None,
            0,
        )
    };
    Ok(LoanApplicationAssessmentResult::Assessed(Box::new(
        LoanApplicationAssessment {
            product,
            periods,
            schedule,
            decision_code,
            decision_reasons,
            quoted_terms,
        },
    )))
}

pub(super) async fn prepare_lease_move_payoff_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    household_id: u64,
    lease_id: Option<u64>,
    lease_deposit_krw: i64,
) -> Result<LeaseMovePayoffPreparation> {
    let Some(lease_id) = lease_id else {
        return Ok(LeaseMovePayoffPreparation::None);
    };
    let contracts: Vec<PrepaymentContractRow> = sqlx::query_as(
        "SELECT id, status, read_only, remaining_principal_krw,
                accrued_interest_krw, accrued_fee_krw,
                CAST(interest_remainder_numerator AS CHAR)
                    AS interest_remainder_numerator,
                prepayment_fee_ppm, prepayment_effect, current_annual_rate_bp,
                day_count_denominator, repayment_method, next_installment_no
         FROM loan_contract
         WHERE save_id = ? AND run_revision = ? AND household_id = ?
           AND lease_contract_id = ? AND product_kind = 'leaseDepositLoan'
         ORDER BY id
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(household_id)
    .bind(lease_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        contracts.len() <= 1,
        "lease has multiple linked deposit loans"
    );
    let Some(contract) = contracts.into_iter().next() else {
        return Ok(LeaseMovePayoffPreparation::None);
    };
    if contract.status == "paidOff" {
        ensure!(
            contract.remaining_principal_krw == 0
                && contract.accrued_interest_krw == 0
                && contract.accrued_fee_krw == 0
                && contract.next_installment_no.is_none(),
            "paid-off linked deposit loan retains a balance"
        );
        return Ok(LeaseMovePayoffPreparation::None);
    }
    if contract.status != "active"
        || contract.read_only
        || contract.remaining_principal_krw <= 0
        || contract.remaining_principal_krw > lease_deposit_krw
        || contract.accrued_interest_krw != 0
        || contract.accrued_fee_krw != 0
        || contract.prepayment_fee_ppm != Some(0)
        || contract.prepayment_effect != "reduceTerm"
        || contract.current_annual_rate_bp.is_none()
        || contract.day_count_denominator != Some(ACTUAL_365_DAY_COUNT)
    {
        return Ok(LeaseMovePayoffPreparation::Rejected(
            LifeFailureCode::ContractConflict,
        ));
    }
    let unpaid_bucket_ids: Vec<(u64,)> = sqlx::query_as(
        "SELECT id FROM loan_obligation_bucket
         WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
           AND status IN ('pending', 'delinquent')
           AND paid_amount_krw < original_amount_krw
         ORDER BY id
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract.id)
    .fetch_all(&mut **tx)
    .await?;
    if !unpaid_bucket_ids.is_empty() {
        return Ok(LeaseMovePayoffPreparation::Rejected(
            LifeFailureCode::ContractConflict,
        ));
    }
    let installments =
        read_pending_installments_in_tx(tx, save_id, run_revision, &contract).await?;
    Ok(LeaseMovePayoffPreparation::Prepared(Box::new(
        PreparedLeaseMovePayoff {
            contract,
            installments,
        },
    )))
}

pub(super) async fn prepare_property_sale_payoff_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    property_holding_id: u64,
    loan_contract_id: Option<u64>,
) -> Result<PropertySalePayoffPreparation> {
    let Some(loan_contract_id) = loan_contract_id else {
        return Ok(PropertySalePayoffPreparation::None);
    };
    let contract: Option<PrepaymentContractRow> = sqlx::query_as(
        "SELECT id, status, read_only, remaining_principal_krw,
                accrued_interest_krw, accrued_fee_krw,
                CAST(interest_remainder_numerator AS CHAR)
                    AS interest_remainder_numerator,
                prepayment_fee_ppm, prepayment_effect, current_annual_rate_bp,
                day_count_denominator, repayment_method, next_installment_no
         FROM loan_contract
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND property_holding_id = ? AND product_kind = 'mortgage'
         FOR UPDATE",
    )
    .bind(loan_contract_id)
    .bind(save_id)
    .bind(run_revision)
    .bind(property_holding_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(contract) = contract else {
        return Ok(PropertySalePayoffPreparation::MortgageNotPayable);
    };
    let Some(fee_ppm) = contract.prepayment_fee_ppm else {
        return Ok(PropertySalePayoffPreparation::MortgageNotPayable);
    };
    if contract.status != "active"
        || contract.read_only
        || contract.remaining_principal_krw <= 0
        || contract.accrued_interest_krw != 0
        || contract.accrued_fee_krw != 0
        || contract.current_annual_rate_bp.is_none()
        || contract.day_count_denominator != Some(ACTUAL_365_DAY_COUNT)
        || !matches!(
            contract.prepayment_effect.as_str(),
            "reduceTerm" | "recalculatePayment"
        )
    {
        return Ok(PropertySalePayoffPreparation::MortgageNotPayable);
    }
    let open_buckets: Vec<(u64,)> = sqlx::query_as(
        "SELECT id FROM loan_obligation_bucket
         WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
           AND status IN ('pending', 'delinquent')
           AND paid_amount_krw < original_amount_krw
         ORDER BY id FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract.id)
    .fetch_all(&mut **tx)
    .await?;
    if !open_buckets.is_empty() {
        return Ok(PropertySalePayoffPreparation::MortgageNotPayable);
    }
    let prepayment = create_loan_rules()
        .calculate_prepayment(LoanPrepaymentInput {
            remaining_principal_krw: contract.remaining_principal_krw,
            principal_krw: contract.remaining_principal_krw,
            fee_ppm,
        })
        .context("property-sale mortgage payoff calculation failed")?;
    ensure!(
        prepayment.remaining_principal_krw == 0
            && prepayment.principal_krw == contract.remaining_principal_krw
            && prepayment.total_debited_krw
                == prepayment
                    .principal_krw
                    .checked_add(prepayment.fee_krw)
                    .context("property-sale mortgage payoff overflowed")?,
        "property-sale mortgage payoff is not a full payoff"
    );
    let installments =
        read_pending_installments_in_tx(tx, save_id, run_revision, &contract).await?;
    Ok(PropertySalePayoffPreparation::Prepared(Box::new(
        PreparedPropertySalePayoff {
            contract,
            installments,
            fee_krw: prepayment.fee_krw,
        },
    )))
}

pub(super) async fn apply_property_sale_payoff_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    property_holding_id: u64,
    execution_id: u64,
    game_day: u32,
    prepared: PreparedPropertySalePayoff,
) -> Result<PropertySalePayoffApplication> {
    let amount_krw = prepared
        .contract
        .remaining_principal_krw
        .checked_add(prepared.fee_krw)
        .context("property-sale mortgage payment overflowed")?;
    let payment_no_raw: u64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(MAX(payment_no), 0) + 1 AS UNSIGNED)
         FROM loan_payment WHERE loan_contract_id = ?",
    )
    .bind(prepared.contract.id)
    .fetch_one(&mut **tx)
    .await?;
    let payment_no =
        u32::try_from(payment_no_raw).context("property-sale payment count is out of range")?;
    let inserted = sqlx::query(
        "INSERT INTO loan_payment
             (save_id, run_revision, loan_contract_id, payment_no, payment_kind,
              amount_krw, game_day, command_id, property_sale_execution_id,
              status, ledger_transaction_id)
         VALUES (?, ?, ?, ?, 'propertySalePayoff', ?, ?, NULL, ?, 'prepared', NULL)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(prepared.contract.id)
    .bind(payment_no)
    .bind(amount_krw)
    .bind(game_day)
    .bind(execution_id)
    .execute(&mut **tx)
    .await?;
    let payment_id = inserted.last_insert_id();
    ensure!(
        payment_id > 0,
        "property-sale mortgage payment has no identity"
    );
    let mut allocation_order = 0_u16;
    if prepared.fee_krw > 0 {
        allocation_order = allocation_order
            .checked_add(1)
            .context("property-sale payment allocation order overflowed")?;
        insert_manual_loan_allocation(
            tx,
            save_id,
            run_revision,
            prepared.contract.id,
            payment_id,
            allocation_order,
            "prepaymentFee",
            prepared.fee_krw,
        )
        .await?;
    }
    allocation_order = allocation_order
        .checked_add(1)
        .context("property-sale payment allocation order overflowed")?;
    insert_manual_loan_allocation(
        tx,
        save_id,
        run_revision,
        prepared.contract.id,
        payment_id,
        allocation_order,
        "prepaymentPrincipal",
        prepared.contract.remaining_principal_krw,
    )
    .await?;
    for installment in &prepared.installments {
        cancel_installment_for_property_sale(
            tx,
            save_id,
            run_revision,
            prepared.contract.id,
            installment,
        )
        .await?;
    }
    let contract_updated = sqlx::query(
        "UPDATE loan_contract
         SET status = 'paidOff', remaining_principal_krw = 0,
             interest_remainder_numerator = '0', next_installment_no = NULL,
             oldest_unpaid_due_game_day = NULL
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND property_holding_id = ? AND product_kind = 'mortgage'
           AND status = 'active' AND read_only = FALSE
           AND remaining_principal_krw = ? AND accrued_interest_krw = 0
           AND accrued_fee_krw = 0 AND interest_remainder_numerator = ?
           AND next_installment_no = ?",
    )
    .bind(prepared.contract.id)
    .bind(save_id)
    .bind(run_revision)
    .bind(property_holding_id)
    .bind(prepared.contract.remaining_principal_krw)
    .bind(&prepared.contract.interest_remainder_numerator)
    .bind(prepared.contract.next_installment_no)
    .execute(&mut **tx)
    .await?;
    ensure!(
        contract_updated.rows_affected() == 1,
        "mortgage changed during property-sale payoff"
    );
    let lien_updated = sqlx::query(
        "UPDATE property_lien
         SET status = 'released', released_game_day = ?
         WHERE save_id = ? AND run_revision = ?
           AND property_holding_id = ? AND loan_contract_id = ?
           AND status = 'active' AND released_game_day IS NULL",
    )
    .bind(game_day)
    .bind(save_id)
    .bind(run_revision)
    .bind(property_holding_id)
    .bind(prepared.contract.id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        lien_updated.rows_affected() == 1,
        "mortgage lien changed during property-sale payoff"
    );
    Ok(PropertySalePayoffApplication {
        loan_id: ResourceId::from_u64(prepared.contract.id),
        payment_id: ResourceId::from_u64(payment_id),
        principal_krw: prepared.contract.remaining_principal_krw,
        fee_krw: prepared.fee_krw,
    })
}

pub(super) async fn mark_property_sale_payoff_applied_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    execution_id: u64,
    payoff: PropertySalePayoffApplication,
    ledger_transaction_id: u64,
) -> Result<()> {
    let applied = sqlx::query(
        "UPDATE loan_payment SET status = 'applied', ledger_transaction_id = ?
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND loan_contract_id = ? AND property_sale_execution_id = ?
           AND payment_kind = 'propertySalePayoff' AND amount_krw = ?
           AND status = 'prepared' AND ledger_transaction_id IS NULL",
    )
    .bind(ledger_transaction_id)
    .bind(payoff.payment_id.get())
    .bind(save_id)
    .bind(run_revision)
    .bind(payoff.loan_id.get())
    .bind(execution_id)
    .bind(
        payoff
            .principal_krw
            .checked_add(payoff.fee_krw)
            .context("property-sale applied payoff overflowed")?,
    )
    .execute(&mut **tx)
    .await?;
    ensure!(
        applied.rows_affected() == 1,
        "property-sale payoff changed before ledger application"
    );
    Ok(())
}

pub(super) async fn prepare_lease_deposit_loan_execution_in_tx(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
    save_id: u64,
    run_revision: u32,
    listing_id: ResourceId,
    quote_id: ResourceId,
) -> Result<LeaseDepositLoanExecutionPreparation> {
    let scope = read_loan_run_scope(tx, save_id, run_revision).await?;
    let quote: Option<ExecutableLeaseDepositQuoteRow> = sqlx::query_as(
        "SELECT loan_product_version_id, command_id, payload_sha256,
                expected_state_revision, requested_principal_krw,
                created_game_day, expires_game_day, decision_code,
                CAST(decision_reasons AS CHAR CHARACTER SET utf8mb4)
                    AS decision_reasons_json,
                CAST(quoted_terms AS CHAR CHARACTER SET utf8mb4) AS quoted_terms_json,
                property_listing_id, lease_deposit_krw, funding_limit_ppm,
                maximum_funding_krw, replaced_loan_contract_id,
                replaced_loan_principal_krw, regulatory_dsr_applied,
                verified_annual_income_krw, verified_income_source,
                existing_loan_balance_krw, post_execution_balance_krw,
                affordability_numerator_krw, affordability_denominator_krw,
                affordability_ratio_ppm, affordability_limit_ppm
         FROM loan_quote
         WHERE save_id = ? AND run_revision = ? AND household_id = ? AND id = ?
           AND purpose = 'leaseDeposit'
         FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.household_id)
    .bind(quote_id.get())
    .fetch_optional(&mut **tx)
    .await?;
    let Some(quote) = quote else {
        return Ok(LeaseDepositLoanExecutionPreparation::Rejected(
            LifeFailureCode::ContractConflict,
        ));
    };
    if quote.decision_code != "eligible"
        || quote.created_game_day != scope.game_day
        || quote.expires_game_day != scope.game_day
        || quote.property_listing_id != Some(listing_id.get())
    {
        return Ok(LeaseDepositLoanExecutionPreparation::Rejected(
            LifeFailureCode::ContractConflict,
        ));
    }
    let command_evidence_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM command_identity AS identity
         INNER JOIN command_receipt AS receipt
           ON receipt.save_id = identity.save_id
          AND BINARY receipt.command_id = BINARY identity.command_id
         WHERE identity.save_id = ? AND BINARY identity.command_id = BINARY ?
           AND BINARY identity.command_kind = BINARY 'createLeaseDepositLoanQuote'
           AND BINARY identity.payload_sha256 = BINARY ?
           AND identity.initial_run_revision = ?
           AND identity.initial_state_revision = ?
           AND identity.initial_game_day = ?
           AND BINARY receipt.command_kind = BINARY identity.command_kind
           AND BINARY receipt.payload_sha256 = BINARY identity.payload_sha256
           AND receipt.run_revision = identity.initial_run_revision
           AND receipt.state_revision = identity.initial_state_revision
           AND receipt.game_day = identity.initial_game_day
           AND receipt.ledger_transaction_id IS NULL",
    )
    .bind(scope.save_id)
    .bind(&quote.command_id)
    .bind(&quote.payload_sha256)
    .bind(scope.run_revision)
    .bind(quote.expected_state_revision)
    .bind(quote.created_game_day)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        command_evidence_count == 1,
        "lease-deposit quote disagrees with its durable command evidence"
    );
    let existing_contract: Option<u64> =
        sqlx::query_scalar("SELECT id FROM loan_contract WHERE loan_quote_id = ? FOR UPDATE")
            .bind(quote_id.get())
            .fetch_optional(&mut **tx)
            .await?;
    if existing_contract.is_some() {
        return Ok(LeaseDepositLoanExecutionPreparation::Rejected(
            LifeFailureCode::ContractConflict,
        ));
    }
    let assessment = match assess_lease_deposit_loan_application_in_tx(
        tx,
        user_id,
        &scope,
        listing_id,
        ResourceId::from_u64(quote.loan_product_version_id),
        quote.requested_principal_krw,
    )
    .await?
    {
        LeaseDepositLoanAssessmentResult::Assessed(assessment) => assessment,
        LeaseDepositLoanAssessmentResult::Rejected(code) => {
            return Ok(LeaseDepositLoanExecutionPreparation::Rejected(code));
        }
    };
    let rejection = match assessment.decision_code {
        LeaseDepositLoanQuoteDecisionState::Eligible => None,
        LeaseDepositLoanQuoteDecisionState::CreditRestricted => {
            Some(LifeFailureCode::CreditRestricted)
        }
        LeaseDepositLoanQuoteDecisionState::CollateralLimit => {
            Some(LifeFailureCode::CollateralLimit)
        }
        LeaseDepositLoanQuoteDecisionState::IncomeUnavailable => {
            Some(LifeFailureCode::IncomeUnavailable)
        }
        LeaseDepositLoanQuoteDecisionState::AffordabilityLimit => {
            Some(LifeFailureCode::AffordabilityLimit)
        }
    };
    if let Some(code) = rejection {
        return Ok(LeaseDepositLoanExecutionPreparation::Rejected(code));
    }
    let stored_reasons: Vec<LeaseDepositLoanQuoteReasonState> =
        serde_json::from_str(&quote.decision_reasons_json)
            .context("stored lease-deposit quote reasons are invalid")?;
    let stored_terms: LoanQuotedTermsState = serde_json::from_str(&quote.quoted_terms_json)
        .context("stored lease-deposit quote terms are invalid")?;
    let stored_affordability = match (
        quote.affordability_numerator_krw,
        quote.affordability_denominator_krw,
        quote.affordability_ratio_ppm,
        quote.affordability_limit_ppm,
    ) {
        (Some(numerator_krw), Some(denominator_krw), Some(ratio_ppm), Some(limit_ppm)) => {
            Some(LeaseDepositLoanAffordabilityState {
                numerator_krw,
                denominator_krw,
                ratio_ppm,
                limit_ppm: i64::from(limit_ppm),
            })
        }
        (None, None, None, None) => None,
        _ => bail!("stored lease-deposit quote has partial affordability evidence"),
    };
    let expected_income_source = assessment
        .verified_income_source
        .map(|_| "activeEmploymentContract");
    let quote_matches = quote.loan_product_version_id == assessment.product.id.get()
        && quote.property_listing_id == Some(assessment.listing_id.get())
        && quote.lease_deposit_krw == Some(assessment.deposit_krw)
        && quote.funding_limit_ppm == u32::try_from(assessment.funding_limit_ppm).ok()
        && quote.maximum_funding_krw == Some(assessment.maximum_funding_krw)
        && quote.replaced_loan_contract_id == assessment.replaced_loan_id.map(ResourceId::get)
        && quote.replaced_loan_principal_krw == assessment.replaced_loan_principal_krw
        && quote.regulatory_dsr_applied == Some(false)
        && quote.verified_annual_income_krw == assessment.verified_annual_income_krw
        && quote.verified_income_source.as_deref() == expected_income_source
        && quote.existing_loan_balance_krw == assessment.existing_loan_balance_krw
        && quote.post_execution_balance_krw == assessment.post_execution_balance_krw
        && stored_affordability == assessment.affordability
        && stored_reasons == assessment.decision_reasons
        && stored_terms == assessment.quoted_terms;
    if !quote_matches {
        return Ok(LeaseDepositLoanExecutionPreparation::Rejected(
            LifeFailureCode::ContractConflict,
        ));
    }
    Ok(LeaseDepositLoanExecutionPreparation::Prepared(Box::new(
        PreparedLeaseDepositLoanExecution {
            scope,
            quote_id,
            principal_krw: quote.requested_principal_krw,
            assessment,
        },
    )))
}

pub(super) async fn apply_lease_move_payoff_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
    command_id: &str,
    prepared: PreparedLeaseMovePayoff,
) -> Result<RepaidDepositLoanReceipt> {
    let payment_no_raw: u64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(MAX(payment_no), 0) + 1 AS UNSIGNED)
         FROM loan_payment WHERE loan_contract_id = ?",
    )
    .bind(prepared.contract.id)
    .fetch_one(&mut **tx)
    .await?;
    let payment_no =
        u32::try_from(payment_no_raw).context("lease-move payment count is out of range")?;
    let inserted = sqlx::query(
        "INSERT INTO loan_payment
             (save_id, run_revision, loan_contract_id, payment_no, payment_kind,
              amount_krw, game_day, command_id, status, ledger_transaction_id)
         VALUES (?, ?, ?, ?, 'leaseMovePayoff', ?, ?, ?, 'prepared', NULL)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(prepared.contract.id)
    .bind(payment_no)
    .bind(prepared.contract.remaining_principal_krw)
    .bind(game_day)
    .bind(command_id)
    .execute(&mut **tx)
    .await?;
    let payment_id = inserted.last_insert_id();
    insert_manual_loan_allocation(
        tx,
        save_id,
        run_revision,
        prepared.contract.id,
        payment_id,
        1,
        "prepaymentPrincipal",
        prepared.contract.remaining_principal_krw,
    )
    .await?;
    let (remaining_installments, next_installment, final_due_game_day) = apply_prepayment_schedule(
        tx,
        save_id,
        run_revision,
        &prepared.contract,
        &prepared.installments,
        None,
    )
    .await?;
    ensure!(
        remaining_installments == 0 && next_installment.is_none() && final_due_game_day.is_none(),
        "lease-move payoff retained a schedule"
    );
    update_contract_after_prepayment(
        tx,
        save_id,
        run_revision,
        &prepared.contract,
        0,
        LoanPrepaymentStatusState::PaidOff,
        None,
    )
    .await?;
    Ok(RepaidDepositLoanReceipt {
        loan_id: ResourceId::from_u64(prepared.contract.id),
        payment_id: ResourceId::from_u64(payment_id),
        principal_krw: prepared.contract.remaining_principal_krw,
    })
}

pub(super) async fn originate_lease_deposit_loan_in_tx(
    tx: &mut Transaction<'_, MySql>,
    command_id: &str,
    lease_id: u64,
    prepared: PreparedLeaseDepositLoanExecution,
) -> Result<DepositLoanExecutionReceipt> {
    let maturity_game_day = prepared
        .assessment
        .periods
        .last()
        .map(|period| period.end_game_day)
        .context("lease-deposit execution schedule has no maturity")?;
    let contract_id = insert_lease_deposit_executed_contract(
        tx,
        &prepared.scope,
        &prepared.assessment.product,
        prepared.quote_id,
        lease_id,
        command_id,
        prepared.principal_krw,
        maturity_game_day,
    )
    .await?;
    insert_schedule_and_settlements(
        tx,
        &prepared.scope,
        contract_id,
        &prepared.assessment.periods,
        &prepared.assessment.schedule.installments,
    )
    .await?;
    Ok(DepositLoanExecutionReceipt {
        loan_id: ResourceId::from_u64(contract_id),
        quote_id: prepared.quote_id,
        product_version_id: prepared.assessment.product.id,
        principal_krw: prepared.principal_krw,
        annual_rate_bp: prepared.assessment.quoted_terms.annual_rate_bp,
        maturity_game_day,
        first_installment: prepared.assessment.quoted_terms.first_installment,
    })
}

pub(super) async fn originate_mortgage_in_tx(
    tx: &mut Transaction<'_, MySql>,
    command_id: &str,
    property_holding_id: u64,
    quote_id: ResourceId,
    principal_krw: i64,
    assessment: Box<MortgageLoanAssessment>,
) -> Result<MortgageExecutionReceipt> {
    let maturity_game_day = assessment
        .periods
        .last()
        .map(|period| period.end_game_day)
        .context("mortgage execution schedule has no maturity")?;
    let contract_id = insert_mortgage_executed_contract(
        tx,
        &assessment.scope,
        &assessment.product,
        quote_id,
        property_holding_id,
        command_id,
        principal_krw,
        maturity_game_day,
    )
    .await?;
    insert_schedule_and_settlements(
        tx,
        &assessment.scope,
        contract_id,
        &assessment.periods,
        &assessment.schedule.installments,
    )
    .await?;
    Ok(MortgageExecutionReceipt {
        loan_id: ResourceId::from_u64(contract_id),
        quote_id,
        product_version_id: assessment.product.id,
        property_holding_id: ResourceId::from_u64(property_holding_id),
        principal_krw,
        activated_game_day: assessment.scope.game_day,
        annual_rate_bp: assessment.quoted_terms.annual_rate_bp,
        maturity_game_day,
        repayment_method: assessment.quoted_terms.repayment_method,
        term_months: assessment.quoted_terms.term_months,
        first_installment: assessment.quoted_terms.first_installment,
    })
}

pub(super) async fn mark_lease_move_payoff_applied_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    payoff: &RepaidDepositLoanReceipt,
    ledger_transaction_id: u64,
) -> Result<()> {
    let applied = sqlx::query(
        "UPDATE loan_payment SET status = 'applied', ledger_transaction_id = ?
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND loan_contract_id = ? AND payment_kind = 'leaseMovePayoff'
           AND amount_krw = ? AND status = 'prepared'
           AND ledger_transaction_id IS NULL",
    )
    .bind(ledger_transaction_id)
    .bind(payoff.payment_id.get())
    .bind(save_id)
    .bind(run_revision)
    .bind(payoff.loan_id.get())
    .bind(payoff.principal_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        applied.rows_affected() == 1,
        "lease-move payoff changed before application"
    );
    Ok(())
}

pub(super) async fn execute_loan_in_tx(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    user_id: u64,
    save_id: u64,
    command: &ExecuteLoanCommand,
) -> Result<LoanExecutionCreation> {
    let scope = read_loan_run_scope(tx, save_id, command.cursor.expected_run_revision).await?;
    let quote: Option<ExecutableQuoteRow> = sqlx::query_as(
        "SELECT purpose, loan_product_version_id, command_id, payload_sha256,
                expected_state_revision, requested_principal_krw,
                created_game_day, expires_game_day, decision_code
         FROM loan_quote
         WHERE save_id = ? AND run_revision = ? AND household_id = ? AND id = ?
         FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.household_id)
    .bind(command.quote_id.get())
    .fetch_optional(&mut **tx)
    .await?;
    let Some(quote) = quote else {
        return Ok(LoanExecutionCreation::Rejected(
            LifeFailureCode::ContractConflict,
        ));
    };
    if quote.decision_code != "eligible"
        || quote.purpose != "unsecured"
        || quote.created_game_day != scope.game_day
        || quote.expires_game_day != scope.game_day
    {
        return Ok(LoanExecutionCreation::Rejected(
            LifeFailureCode::ContractConflict,
        ));
    }
    let command_evidence_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM command_identity AS identity
         INNER JOIN command_receipt AS receipt
           ON receipt.save_id = identity.save_id
          AND BINARY receipt.command_id = BINARY identity.command_id
         WHERE identity.save_id = ? AND BINARY identity.command_id = BINARY ?
           AND BINARY identity.command_kind = BINARY 'createLoanQuote'
           AND BINARY identity.payload_sha256 = BINARY ?
           AND identity.initial_run_revision = ?
           AND identity.initial_state_revision = ?
           AND identity.initial_game_day = ?
           AND BINARY receipt.command_kind = BINARY identity.command_kind
           AND BINARY receipt.payload_sha256 = BINARY identity.payload_sha256
           AND receipt.run_revision = identity.initial_run_revision
           AND receipt.state_revision = identity.initial_state_revision
           AND receipt.game_day = identity.initial_game_day
           AND receipt.ledger_transaction_id IS NULL",
    )
    .bind(scope.save_id)
    .bind(&quote.command_id)
    .bind(&quote.payload_sha256)
    .bind(scope.run_revision)
    .bind(quote.expected_state_revision)
    .bind(quote.created_game_day)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        command_evidence_count == 1,
        "loan quote disagrees with its durable command evidence"
    );
    let existing_contract: Option<u64> =
        sqlx::query_scalar("SELECT id FROM loan_contract WHERE loan_quote_id = ? FOR UPDATE")
            .bind(command.quote_id.get())
            .fetch_optional(&mut **tx)
            .await?;
    if existing_contract.is_some() {
        return Ok(LoanExecutionCreation::Rejected(
            LifeFailureCode::ContractConflict,
        ));
    }
    let product_version_id = ResourceId::from_u64(quote.loan_product_version_id);
    let assessment = match assess_loan_application_in_tx(
        tx,
        user_id,
        &scope,
        product_version_id,
        quote.requested_principal_krw,
    )
    .await?
    {
        LoanApplicationAssessmentResult::Assessed(assessment) => *assessment,
        LoanApplicationAssessmentResult::Rejected(LifeFailureCode::RateUnavailable) => {
            return Ok(LoanExecutionCreation::Rejected(
                LifeFailureCode::RateUnavailable,
            ));
        }
        LoanApplicationAssessmentResult::Rejected(code) => {
            bail!("eligible loan quote became structurally invalid during execution: {code:?}")
        }
    };
    let rejection = match assessment.decision_code {
        LoanQuoteDecisionState::Eligible => None,
        LoanQuoteDecisionState::CreditRestricted => Some(LifeFailureCode::CreditRestricted),
        LoanQuoteDecisionState::IncomeUnavailable => Some(LifeFailureCode::IncomeUnavailable),
        LoanQuoteDecisionState::DebtServiceLimit => Some(LifeFailureCode::DebtServiceLimit),
        LoanQuoteDecisionState::ValuationUnavailable => {
            bail!("unsecured loan execution unexpectedly required a valuation")
        }
    };
    if let Some(code) = rejection {
        return Ok(LoanExecutionCreation::Rejected(code));
    }
    ensure!(
        assessment.decision_reasons == vec![LoanQuoteReasonState::Eligible],
        "eligible loan execution has non-eligible reasons"
    );
    let maturity_game_day = assessment
        .periods
        .last()
        .map(|period| period.end_game_day)
        .context("executed loan schedule has no maturity")?;
    let contract_id = insert_executed_contract(
        tx,
        ExecutedContractDraft {
            scope: &scope,
            product: &assessment.product,
            quote_id: command.quote_id,
            origin_command_id: command.command_id.as_str(),
            principal_krw: quote.requested_principal_krw,
            maturity_game_day,
        },
    )
    .await?;
    insert_schedule_and_settlements(
        tx,
        &scope,
        contract_id,
        &assessment.periods,
        &assessment.schedule.installments,
    )
    .await?;
    let ledger = finance_rules.create_ledger_transaction(LedgerTransactionDraft {
        policy: RunPolicyContext {
            run: RunId {
                save_id: ResourceId::from_u64(scope.save_id),
                run_revision: scope.run_revision,
            },
            policy_set_id: ResourceId::from_u64(scope.policy_set_id),
        },
        source: LedgerSource {
            kind: LedgerSourceKind::LoanOrigination,
            source_id: contract_id.to_string(),
        },
        game_day: scope.game_day,
        description: "신규 대출 실행".to_owned(),
        postings: vec![
            LedgerPosting {
                account_code: LedgerAccountCode::Wallet,
                financial_account_id: None,
                amount_krw: quote.requested_principal_krw,
            },
            LedgerPosting {
                account_code: LedgerAccountCode::LoanPrincipalLiability,
                financial_account_id: None,
                amount_krw: quote
                    .requested_principal_krw
                    .checked_neg()
                    .context("executed loan principal cannot be negated")?,
            },
        ],
    })?;
    let ledger_transaction_id = write_loan_ledger_transaction(
        tx,
        &ledger,
        &[
            LoanPostingReference::None,
            LoanPostingReference::Contract(contract_id),
        ],
    )
    .await?;
    let receipt = LoanExecutionReceipt {
        command_id: command.command_id.clone(),
        loan_id: ResourceId::from_u64(contract_id),
        quote_id: command.quote_id,
        product_version_id,
        principal_krw: quote.requested_principal_krw,
        activated_game_day: scope.game_day,
        maturity_game_day,
        annual_rate_bp: assessment.quoted_terms.annual_rate_bp,
        repayment_method: assessment.quoted_terms.repayment_method,
        term_months: assessment.quoted_terms.term_months,
        first_installment: assessment.quoted_terms.first_installment,
        replayed: false,
    };
    Ok(LoanExecutionCreation::Applied {
        receipt: Box::new(receipt),
        ledger_transaction_id,
    })
}

async fn read_pending_installments_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    contract: &PrepaymentContractRow,
) -> Result<Vec<PendingInstallmentRow>> {
    let installments: Vec<PendingInstallmentRow> = sqlx::query_as(
        "SELECT id, installment_no, due_game_day,
                interest_period_start_game_day, interest_period_end_game_day,
                elapsed_days, annual_rate_bp, opening_principal_krw,
                scheduled_fee_krw, scheduled_interest_krw, scheduled_principal_krw,
                CAST(interest_remainder_before AS CHAR) AS interest_remainder_before,
                CAST(interest_remainder_after AS CHAR) AS interest_remainder_after,
                schedule_revision
         FROM loan_installment
         WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
           AND status = 'pending'
         ORDER BY installment_no
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract.id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        !installments.is_empty()
            && installments.first().map(|row| row.installment_no) == contract.next_installment_no
            && installments
                .windows(2)
                .all(|pair| pair[1].installment_no == pair[0].installment_no.saturating_add(1)),
        "active linked deposit loan pending installments are invalid"
    );
    ensure!(
        installments.iter().all(|installment| {
            installment.scheduled_fee_krw == 0
                && installment.interest_period_end_game_day == installment.due_game_day
                && u32::from(installment.elapsed_days)
                    == installment
                        .interest_period_end_game_day
                        .saturating_sub(installment.interest_period_start_game_day)
                        .saturating_add(1)
        }),
        "active linked deposit loan schedule periods are invalid"
    );
    let scheduled_principal_krw = installments.iter().try_fold(0_i64, |total, installment| {
        total
            .checked_add(installment.scheduled_principal_krw)
            .context("linked deposit loan pending principal overflowed")
    })?;
    ensure!(
        scheduled_principal_krw == contract.remaining_principal_krw,
        "linked deposit loan schedule disagrees with its balance"
    );
    Ok(installments)
}

pub(super) async fn prepay_loan_in_tx(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    save_id: u64,
    run_revision: u32,
    wallet_cash_krw: i64,
    command: &PrepayLoanCommand,
) -> Result<LoanPrepaymentCreation> {
    let contract: Option<PrepaymentContractRow> = sqlx::query_as(
        "SELECT id, status, read_only, remaining_principal_krw,
                accrued_interest_krw, accrued_fee_krw,
                CAST(interest_remainder_numerator AS CHAR)
                    AS interest_remainder_numerator,
                prepayment_fee_ppm, prepayment_effect, current_annual_rate_bp,
                day_count_denominator, repayment_method, next_installment_no
         FROM loan_contract
         WHERE id = ? AND save_id = ? AND run_revision = ?
         FOR UPDATE",
    )
    .bind(command.loan_id.get())
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(contract) = contract else {
        return Ok(LoanPrepaymentCreation::Rejected(
            LifeFailureCode::ContractConflict,
        ));
    };
    let prepayment_effect = match contract.prepayment_effect.as_str() {
        "reduceTerm" => LoanPrepaymentEffect::ReduceTerm,
        "recalculatePayment" => LoanPrepaymentEffect::RecalculatePayment,
        _ => {
            return Ok(LoanPrepaymentCreation::Rejected(
                LifeFailureCode::ContractConflict,
            ));
        }
    };
    let (Some(fee_ppm), Some(annual_rate_bp), Some(day_count)) = (
        contract.prepayment_fee_ppm,
        contract.current_annual_rate_bp,
        contract.day_count_denominator,
    ) else {
        return Ok(LoanPrepaymentCreation::Rejected(
            LifeFailureCode::ContractConflict,
        ));
    };
    if contract.status != "active"
        || contract.read_only
        || contract.accrued_interest_krw != 0
        || contract.accrued_fee_krw != 0
    {
        return Ok(LoanPrepaymentCreation::Rejected(
            LifeFailureCode::ContractConflict,
        ));
    }
    let unpaid_bucket_ids: Vec<(u64,)> = sqlx::query_as(
        "SELECT id FROM loan_obligation_bucket
         WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
           AND status IN ('pending', 'delinquent')
         ORDER BY id
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract.id)
    .fetch_all(&mut **tx)
    .await?;
    if !unpaid_bucket_ids.is_empty() {
        return Ok(LoanPrepaymentCreation::Rejected(
            LifeFailureCode::ContractConflict,
        ));
    }
    let prepayment = match create_loan_rules().calculate_prepayment(LoanPrepaymentInput {
        remaining_principal_krw: contract.remaining_principal_krw,
        principal_krw: command.principal_krw,
        fee_ppm,
    }) {
        Ok(prepayment) => prepayment,
        Err(crate::life::LoanRuleError::InvalidPrepayment) => {
            return Ok(LoanPrepaymentCreation::Rejected(
                LifeFailureCode::ContractConflict,
            ));
        }
        Err(error) => return Err(error).context("loan prepayment calculation failed"),
    };
    if wallet_cash_krw < prepayment.total_debited_krw {
        return Ok(LoanPrepaymentCreation::Rejected(
            LifeFailureCode::InsufficientWalletCash,
        ));
    }

    let installments: Vec<PendingInstallmentRow> = sqlx::query_as(
        "SELECT id, installment_no, due_game_day,
                interest_period_start_game_day, interest_period_end_game_day,
                elapsed_days, annual_rate_bp, opening_principal_krw,
                scheduled_fee_krw, scheduled_interest_krw, scheduled_principal_krw,
                CAST(interest_remainder_before AS CHAR) AS interest_remainder_before,
                CAST(interest_remainder_after AS CHAR) AS interest_remainder_after,
                schedule_revision
         FROM loan_installment
         WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
           AND status = 'pending'
         ORDER BY installment_no
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract.id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        !installments.is_empty()
            && Some(installments[0].installment_no) == contract.next_installment_no
            && installments
                .windows(2)
                .all(|pair| { pair[1].installment_no == pair[0].installment_no.saturating_add(1) }),
        "active loan pending installments are invalid"
    );
    ensure!(
        installments.iter().all(|installment| {
            installment.scheduled_fee_krw == 0
                && installment.interest_period_end_game_day == installment.due_game_day
                && u32::from(installment.elapsed_days)
                    == installment
                        .interest_period_end_game_day
                        .saturating_sub(installment.interest_period_start_game_day)
                        .saturating_add(1)
        }),
        "active loan pending schedule periods are invalid"
    );
    let scheduled_principal_krw = installments.iter().try_fold(0_i64, |total, installment| {
        total
            .checked_add(installment.scheduled_principal_krw)
            .context("pending loan principal overflowed")
    })?;
    ensure!(
        scheduled_principal_krw == contract.remaining_principal_krw,
        "pending loan principal disagrees with the contract balance"
    );
    let prior_interest_remainder_numerator = contract
        .interest_remainder_numerator
        .parse::<i128>()
        .context("loan prepayment interest remainder is invalid")?;
    let repayment_method = parse_repayment_method(&contract.repayment_method)?;
    let schedule_periods = installments
        .iter()
        .map(|installment| LoanPrepaymentSchedulePeriod {
            installment_no: installment.installment_no,
            due_game_day: installment.due_game_day,
            elapsed_days: installment.elapsed_days,
            scheduled_principal_cap_krw: installment.scheduled_principal_krw,
        })
        .collect::<Vec<_>>();
    let recalculated_schedule = if prepayment.remaining_principal_krw == 0 {
        None
    } else {
        match create_loan_rules().rebuild_prepayment_schedule(LoanPrepaymentScheduleInput {
            principal_before_prepayment_krw: contract.remaining_principal_krw,
            principal_after_prepayment_krw: prepayment.remaining_principal_krw,
            annual_rate_bp: i64::from(annual_rate_bp),
            day_count,
            repayment_method,
            prepayment_effect,
            prior_interest_remainder_numerator,
            periods: &schedule_periods,
        }) {
            Ok(schedule) => Some(schedule),
            Err(crate::life::LoanRuleError::InvalidPrepaymentSchedule) => {
                return Ok(LoanPrepaymentCreation::Rejected(
                    LifeFailureCode::ContractConflict,
                ));
            }
            Err(error) => return Err(error).context("loan prepayment schedule is invalid"),
        }
    };

    let payment_id = insert_manual_loan_payment(
        tx,
        save_id,
        run_revision,
        contract.id,
        prepayment.total_debited_krw,
        command.cursor.expected_game_day,
        command.command_id.as_str(),
    )
    .await?;
    let mut allocation_order = 0_u16;
    if prepayment.fee_krw > 0 {
        allocation_order = allocation_order
            .checked_add(1)
            .context("loan prepayment allocation order overflowed")?;
        insert_manual_loan_allocation(
            tx,
            save_id,
            run_revision,
            contract.id,
            payment_id,
            allocation_order,
            "prepaymentFee",
            prepayment.fee_krw,
        )
        .await?;
    }
    allocation_order = allocation_order
        .checked_add(1)
        .context("loan prepayment allocation order overflowed")?;
    insert_manual_loan_allocation(
        tx,
        save_id,
        run_revision,
        contract.id,
        payment_id,
        allocation_order,
        "prepaymentPrincipal",
        prepayment.principal_krw,
    )
    .await?;
    let ledger = finance_rules.create_ledger_transaction(LedgerTransactionDraft {
        policy: RunPolicyContext {
            run: RunId {
                save_id: ResourceId::from_u64(save_id),
                run_revision,
            },
            policy_set_id: ResourceId::from_u64(
                read_contract_policy_set_id(tx, save_id, run_revision, contract.id).await?,
            ),
        },
        source: LedgerSource {
            kind: LedgerSourceKind::LoanPrepayment,
            source_id: payment_id.to_string(),
        },
        game_day: command.cursor.expected_game_day,
        description: "대출 중도상환".to_owned(),
        postings: create_prepayment_postings(
            prepayment.principal_krw,
            prepayment.fee_krw,
            prepayment.total_debited_krw,
        )?,
    })?;
    let references = ledger
        .postings()
        .iter()
        .map(|posting| {
            if matches!(
                posting.account_code,
                LedgerAccountCode::LoanPrincipalLiability | LedgerAccountCode::LoanFeeExpense
            ) {
                LoanPostingReference::Contract(contract.id)
            } else {
                LoanPostingReference::None
            }
        })
        .collect::<Vec<_>>();
    let ledger_transaction_id = write_loan_ledger_transaction(tx, &ledger, &references).await?;
    let applied = sqlx::query(
        "UPDATE loan_payment SET status = 'applied', ledger_transaction_id = ?
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND loan_contract_id = ? AND status = 'prepared'
           AND ledger_transaction_id IS NULL",
    )
    .bind(ledger_transaction_id)
    .bind(payment_id)
    .bind(save_id)
    .bind(run_revision)
    .bind(contract.id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        applied.rows_affected() == 1,
        "loan prepayment changed before application"
    );

    let (remaining_installments, next_installment, final_installment_due_game_day) =
        apply_prepayment_schedule(
            tx,
            save_id,
            run_revision,
            &contract,
            &installments,
            recalculated_schedule.as_ref(),
        )
        .await?;
    let status = if prepayment.remaining_principal_krw == 0 {
        LoanPrepaymentStatusState::PaidOff
    } else {
        LoanPrepaymentStatusState::Active
    };
    update_contract_after_prepayment(
        tx,
        save_id,
        run_revision,
        &contract,
        prepayment.remaining_principal_krw,
        status,
        next_installment
            .as_ref()
            .map(|installment| installment.installment_no),
    )
    .await?;
    let receipt = LoanPrepaymentReceipt {
        command_id: command.command_id.clone(),
        loan_id: command.loan_id,
        payment_id: ResourceId::from_u64(payment_id),
        principal_krw: prepayment.principal_krw,
        fee_krw: prepayment.fee_krw,
        total_debited_krw: prepayment.total_debited_krw,
        applied_game_day: command.cursor.expected_game_day,
        remaining_principal_krw: prepayment.remaining_principal_krw,
        status,
        prepayment_effect,
        remaining_installments,
        next_installment,
        final_installment_due_game_day,
        replayed: false,
    };
    Ok(LoanPrepaymentCreation::Applied {
        receipt: Box::new(receipt),
        ledger_transaction_id,
    })
}

async fn read_contract_policy_set_id(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    contract_id: u64,
) -> Result<u64> {
    Ok(sqlx::query_scalar(
        "SELECT save.policy_set_id
         FROM loan_contract AS contract
         INNER JOIN save ON save.id = contract.save_id
         WHERE contract.id = ? AND contract.save_id = ? AND contract.run_revision = ?",
    )
    .bind(contract_id)
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await?)
}

fn create_prepayment_postings(
    principal_krw: i64,
    fee_krw: i64,
    total_debited_krw: i64,
) -> Result<Vec<LedgerPosting>> {
    let mut postings = vec![
        LedgerPosting {
            account_code: LedgerAccountCode::Wallet,
            financial_account_id: None,
            amount_krw: total_debited_krw
                .checked_neg()
                .context("loan prepayment total cannot be negated")?,
        },
        LedgerPosting {
            account_code: LedgerAccountCode::LoanPrincipalLiability,
            financial_account_id: None,
            amount_krw: principal_krw,
        },
    ];
    if fee_krw > 0 {
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::LoanFeeExpense,
            financial_account_id: None,
            amount_krw: fee_krw,
        });
    }
    Ok(postings)
}

#[allow(clippy::too_many_arguments)]
async fn insert_manual_loan_payment(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    contract_id: u64,
    amount_krw: i64,
    game_day: u32,
    command_id: &str,
) -> Result<u64> {
    let payment_no_raw: u64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(MAX(payment_no), 0) + 1 AS UNSIGNED)
         FROM loan_payment WHERE loan_contract_id = ?",
    )
    .bind(contract_id)
    .fetch_one(&mut **tx)
    .await?;
    let payment_no = u32::try_from(payment_no_raw).context("loan payment count is out of range")?;
    let inserted = sqlx::query(
        "INSERT INTO loan_payment
             (save_id, run_revision, loan_contract_id, payment_no, payment_kind,
              amount_krw, game_day, command_id, status, ledger_transaction_id)
         VALUES (?, ?, ?, ?, 'manualPrepayment', ?, ?, ?, 'prepared', NULL)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract_id)
    .bind(payment_no)
    .bind(amount_krw)
    .bind(game_day)
    .bind(command_id)
    .execute(&mut **tx)
    .await?;
    Ok(inserted.last_insert_id())
}

#[allow(clippy::too_many_arguments)]
async fn insert_manual_loan_allocation(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    contract_id: u64,
    payment_id: u64,
    allocation_order: u16,
    allocation_kind: &str,
    amount_krw: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO loan_payment_allocation
             (save_id, run_revision, loan_contract_id, loan_payment_id,
              loan_obligation_bucket_id, allocation_order, allocation_kind, amount_krw)
         VALUES (?, ?, ?, ?, NULL, ?, ?, ?)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract_id)
    .bind(payment_id)
    .bind(allocation_order)
    .bind(allocation_kind)
    .bind(amount_krw)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn apply_prepayment_schedule(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    contract: &PrepaymentContractRow,
    stored_installments: &[PendingInstallmentRow],
    recalculated: Option<&LoanPrepaymentScheduleCalculation>,
) -> Result<(u16, Option<LoanPrepaymentNextInstallmentState>, Option<u32>)> {
    let retained_count = recalculated
        .map(|schedule| schedule.installments.len())
        .unwrap_or(0);
    ensure!(
        retained_count <= stored_installments.len(),
        "loan prepayment retained too many installments"
    );
    let cancelled_numbers = stored_installments[retained_count..]
        .iter()
        .map(|installment| installment.installment_no)
        .collect::<Vec<_>>();
    if let Some(schedule) = recalculated {
        ensure!(
            !schedule.installments.is_empty()
                && schedule.cancelled_installment_numbers == cancelled_numbers,
            "loan prepayment schedule cancellation set is invalid"
        );
        let mut remainder_before = contract
            .interest_remainder_numerator
            .parse::<i128>()
            .context("loan prepayment interest remainder is invalid")?;
        for (stored, calculated) in stored_installments.iter().zip(&schedule.installments) {
            ensure!(
                calculated.sequence == stored.installment_no
                    && calculated.due_game_day == stored.due_game_day
                    && calculated.elapsed_days == stored.elapsed_days,
                "loan prepayment changed the retained installment calendar"
            );
            let updated = sqlx::query(
                "UPDATE loan_installment
                 SET annual_rate_bp = ?, opening_principal_krw = ?,
                     scheduled_interest_krw = ?, scheduled_principal_krw = ?,
                     interest_remainder_before = ?, interest_remainder_after = ?,
                     schedule_revision = schedule_revision + 1
                 WHERE id = ? AND save_id = ? AND run_revision = ?
                   AND loan_contract_id = ? AND status = 'pending'
                   AND schedule_revision = ? AND due_game_day = ?
                   AND annual_rate_bp = ? AND opening_principal_krw = ?
                   AND scheduled_fee_krw = ? AND scheduled_interest_krw = ?
                   AND scheduled_principal_krw = ?
                   AND interest_remainder_before = ? AND interest_remainder_after = ?",
            )
            .bind(
                u16::try_from(calculated.annual_rate_bp)
                    .context("loan prepayment rate is invalid")?,
            )
            .bind(calculated.opening_principal_krw)
            .bind(calculated.interest_krw)
            .bind(calculated.principal_krw)
            .bind(remainder_before.to_string())
            .bind(calculated.carried_interest_remainder_numerator.to_string())
            .bind(stored.id)
            .bind(save_id)
            .bind(run_revision)
            .bind(contract.id)
            .bind(stored.schedule_revision)
            .bind(stored.due_game_day)
            .bind(stored.annual_rate_bp)
            .bind(stored.opening_principal_krw)
            .bind(stored.scheduled_fee_krw)
            .bind(stored.scheduled_interest_krw)
            .bind(stored.scheduled_principal_krw)
            .bind(&stored.interest_remainder_before)
            .bind(&stored.interest_remainder_after)
            .execute(&mut **tx)
            .await?;
            ensure!(
                updated.rows_affected() == 1,
                "pending loan schedule changed during prepayment"
            );
            remainder_before = calculated.carried_interest_remainder_numerator;
        }
    } else {
        ensure!(
            cancelled_numbers.len() == stored_installments.len(),
            "full loan prepayment did not cancel every installment"
        );
    }
    for installment in &stored_installments[retained_count..] {
        cancel_installment_for_prepayment(tx, save_id, run_revision, contract.id, installment)
            .await?;
    }

    let remaining_installments =
        u16::try_from(retained_count).context("remaining loan installment count is invalid")?;
    let next_installment = match (recalculated, stored_installments.first()) {
        (Some(schedule), Some(stored)) => {
            let calculated = schedule
                .installments
                .first()
                .context("loan prepayment next installment is missing")?;
            let total_krw = stored
                .scheduled_fee_krw
                .checked_add(calculated.principal_krw)
                .and_then(|amount| amount.checked_add(calculated.interest_krw))
                .context("loan prepayment next installment total overflowed")?;
            Some(LoanPrepaymentNextInstallmentState {
                installment_no: calculated.sequence,
                due_game_day: calculated.due_game_day,
                fee_krw: stored.scheduled_fee_krw,
                principal_krw: calculated.principal_krw,
                interest_krw: calculated.interest_krw,
                total_krw,
            })
        }
        (None, _) => None,
        (Some(_), None) => bail!("loan prepayment next installment is missing"),
    };
    let final_installment_due_game_day = recalculated
        .and_then(|schedule| schedule.installments.last())
        .map(|installment| installment.due_game_day);
    Ok((
        remaining_installments,
        next_installment,
        final_installment_due_game_day,
    ))
}

async fn cancel_installment_for_prepayment(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    contract_id: u64,
    installment: &PendingInstallmentRow,
) -> Result<()> {
    let cancelled = sqlx::query(
        "UPDATE loan_installment SET status = 'cancelled'
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND loan_contract_id = ? AND status = 'pending'
           AND schedule_revision = ?",
    )
    .bind(installment.id)
    .bind(save_id)
    .bind(run_revision)
    .bind(contract_id)
    .bind(installment.schedule_revision)
    .execute(&mut **tx)
    .await?;
    ensure!(
        cancelled.rows_affected() == 1,
        "pending loan installment changed during prepayment"
    );
    let settlement_cancelled = sqlx::query(
        "UPDATE scheduled_settlement
         SET status = 'cancelled', cancellation_reason = 'loanPrepayment'
         WHERE save_id = ? AND run_revision = ?
           AND source_kind = 'loanContract' AND source_id = ?
           AND kind = 'loanInstallment' AND occurrence = ? AND status = 'pending'",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract_id.to_string())
    .bind(u32::from(installment.installment_no))
    .execute(&mut **tx)
    .await?;
    ensure!(
        settlement_cancelled.rows_affected() == 1,
        "pending loan settlement changed during prepayment"
    );
    Ok(())
}

async fn cancel_installment_for_property_sale(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    contract_id: u64,
    installment: &PendingInstallmentRow,
) -> Result<()> {
    let cancelled = sqlx::query(
        "UPDATE loan_installment SET status = 'cancelled'
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND loan_contract_id = ? AND status = 'pending'
           AND schedule_revision = ?",
    )
    .bind(installment.id)
    .bind(save_id)
    .bind(run_revision)
    .bind(contract_id)
    .bind(installment.schedule_revision)
    .execute(&mut **tx)
    .await?;
    ensure!(
        cancelled.rows_affected() == 1,
        "pending loan installment changed during property sale"
    );
    let settlement_cancelled = sqlx::query(
        "UPDATE scheduled_settlement
         SET status = 'cancelled', cancellation_reason = 'propertySale'
         WHERE save_id = ? AND run_revision = ?
           AND source_kind = 'loanContract' AND source_id = ?
           AND kind = 'loanInstallment' AND occurrence = ? AND status = 'pending'",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract_id.to_string())
    .bind(u32::from(installment.installment_no))
    .execute(&mut **tx)
    .await?;
    ensure!(
        settlement_cancelled.rows_affected() == 1,
        "pending loan settlement changed during property sale"
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn update_contract_after_prepayment(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    contract: &PrepaymentContractRow,
    remaining_principal_krw: i64,
    status: LoanPrepaymentStatusState,
    next_installment_no: Option<u16>,
) -> Result<()> {
    let (status, interest_remainder_numerator) = match status {
        LoanPrepaymentStatusState::Active => {
            ensure!(
                remaining_principal_krw > 0 && next_installment_no.is_some(),
                "partial loan prepayment has no next installment"
            );
            ("active", contract.interest_remainder_numerator.as_str())
        }
        LoanPrepaymentStatusState::PaidOff => {
            ensure!(
                remaining_principal_krw == 0 && next_installment_no.is_none(),
                "full loan prepayment retained a balance or installment"
            );
            ("paidOff", "0")
        }
    };
    let updated = sqlx::query(
        "UPDATE loan_contract
         SET status = ?, remaining_principal_krw = ?,
             interest_remainder_numerator = ?, next_installment_no = ?,
             oldest_unpaid_due_game_day = NULL
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND status = 'active' AND read_only = FALSE
           AND remaining_principal_krw = ? AND accrued_interest_krw = 0
           AND accrued_fee_krw = 0 AND interest_remainder_numerator = ?
           AND next_installment_no = ?",
    )
    .bind(status)
    .bind(remaining_principal_krw)
    .bind(interest_remainder_numerator)
    .bind(next_installment_no)
    .bind(contract.id)
    .bind(save_id)
    .bind(run_revision)
    .bind(contract.remaining_principal_krw)
    .bind(&contract.interest_remainder_numerator)
    .bind(contract.next_installment_no)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "loan contract changed during prepayment"
    );
    if status == "paidOff" {
        release_property_lien_after_payoff(tx, save_id, run_revision, contract.id).await?;
    }
    Ok(())
}

async fn release_property_lien_after_payoff(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    contract_id: u64,
) -> Result<()> {
    sqlx::query(
        "UPDATE property_lien AS lien
         INNER JOIN loan_contract AS contract
           ON contract.id = lien.loan_contract_id
          AND contract.save_id = lien.save_id
          AND contract.run_revision = lien.run_revision
         INNER JOIN save
           ON save.id = lien.save_id AND save.run_revision = lien.run_revision
         SET lien.status = 'released', lien.released_game_day = save.game_day
         WHERE lien.save_id = ? AND lien.run_revision = ?
           AND lien.loan_contract_id = ? AND lien.status = 'active'
           AND contract.product_kind = 'mortgage'
           AND contract.status = 'paidOff' AND contract.remaining_principal_krw = 0",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn assess_loan_quote_dsr(
    tx: &mut Transaction<'_, MySql>,
    scope: &LoanRunScopeRow,
    product: &LoanProductState,
    loans: &[QuoteLoanRow],
    candidate_periods: &[LoanSchedulePeriod],
    requested_principal_krw: i64,
    verified_annual_income_krw: Option<i64>,
) -> Result<(
    LoanQuoteDecisionState,
    Vec<LoanQuoteReasonState>,
    bool,
    Option<LoanQuoteDsrState>,
    i64,
)> {
    let policy = read_dsr_policy_in_tx(tx, scope).await?;
    let mut owned = read_existing_dsr_loans_in_tx(tx, scope, loans).await?;
    let candidate_id = owned
        .iter()
        .map(|loan| loan.loan_id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .context("prospective loan id overflowed")?;
    owned.push(OwnedDsrLoan {
        loan_id: candidate_id,
        included_in_dsr: true,
        counts_toward_general_loan_balance: true,
        counts_toward_credit_stress_balance: true,
        rate_type: product.rate_type,
        fixed_rate_period_months: if product.rate_type == LoanRateType::Fixed {
            product.term_months
        } else {
            0
        },
        payment_treatment: if product.repayment_method == LoanRepaymentMethod::Bullet {
            DsrPaymentTreatment::BulletCreditFiveYear
        } else {
            DsrPaymentTreatment::Scheduled
        },
        principal_krw: requested_principal_krw,
        initial_annual_rate_bp: product
            .current_annual_rate_bp
            .context("quoted product lost its current rate")?,
        repayment_method: product.repayment_method,
        prior_interest_remainder_numerator: 0,
        periods: candidate_periods.to_vec(),
        rate_resets: Vec::new(),
    });
    let inputs = owned
        .iter()
        .map(|loan| DsrLoanInput {
            loan_id: loan.loan_id,
            included_in_dsr: loan.included_in_dsr,
            counts_toward_general_loan_balance: loan.counts_toward_general_loan_balance,
            counts_toward_credit_stress_balance: loan.counts_toward_credit_stress_balance,
            rate_type: loan.rate_type,
            fixed_rate_period_months: loan.fixed_rate_period_months,
            payment_treatment: loan.payment_treatment,
            schedule: LoanScheduleInput {
                principal_krw: loan.principal_krw,
                initial_annual_rate_bp: loan.initial_annual_rate_bp,
                day_count: ACTUAL_365_DAY_COUNT,
                repayment_method: loan.repayment_method,
                prior_interest_remainder_numerator: loan.prior_interest_remainder_numerator,
                periods: &loan.periods,
                rate_resets: &loan.rate_resets,
            },
        })
        .collect::<Vec<_>>();
    let evaluation_end_game_day =
        twelve_month_horizon_game_day(scope.world_start_date, scope.game_day)?;
    let maximum_ratio_ppm = match product.lender_sector {
        LoanLenderSector::Bank => policy.bank_limit_ppm,
        LoanLenderSector::NonBank => policy.non_bank_limit_ppm,
    };
    let assessment = create_loan_rules().assess_dsr(DsrAssessmentInput {
        evaluation_game_day: scope.game_day,
        evaluation_end_game_day,
        verified_annual_income_krw: verified_annual_income_krw.or(Some(1)),
        policy: DsrPolicy {
            general_loan_balance_gate_krw: policy.general_loan_balance_gate_krw,
            maximum_ratio_ppm,
            credit_balance_stress_gate_krw: policy.credit_balance_stress_gate_krw,
            base_stress_rate_bp: policy.base_stress_rate_bp,
            medium_fixed_stress_multiplier_ppm: policy.medium_fixed_stress_multiplier_ppm,
        },
        loans: &inputs,
    })?;
    let stress_rate_bp = assessment
        .loan_contributions
        .iter()
        .find(|contribution| contribution.loan_id == candidate_id)
        .map(|contribution| contribution.stress_rate_bp)
        .context("prospective DSR contribution is missing")?;
    if !assessment.gate_applied {
        return Ok((
            LoanQuoteDecisionState::Eligible,
            vec![LoanQuoteReasonState::Eligible],
            false,
            None,
            stress_rate_bp,
        ));
    }
    let Some(verified_annual_income_krw) = verified_annual_income_krw else {
        return Ok((
            LoanQuoteDecisionState::IncomeUnavailable,
            vec![LoanQuoteReasonState::IncomeUnavailable],
            true,
            None,
            stress_rate_bp,
        ));
    };
    ensure!(
        assessment.denominator_krw == Some(verified_annual_income_krw),
        "DSR denominator disagrees with verified income"
    );
    let dsr = LoanQuoteDsrState {
        numerator_krw: assessment.numerator_krw,
        denominator_krw: assessment
            .denominator_krw
            .context("applied DSR has no denominator")?,
        ratio_ppm: assessment.ratio_ppm.context("applied DSR has no ratio")?,
        limit_ppm: assessment.maximum_ratio_ppm,
    };
    if assessment.passed {
        Ok((
            LoanQuoteDecisionState::Eligible,
            vec![LoanQuoteReasonState::Eligible],
            true,
            Some(dsr),
            stress_rate_bp,
        ))
    } else {
        Ok((
            LoanQuoteDecisionState::DebtServiceLimit,
            vec![LoanQuoteReasonState::DebtServiceLimit],
            true,
            Some(dsr),
            stress_rate_bp,
        ))
    }
}

fn parse_quote_eligibility(parameters_json: &str) -> Result<Option<QuoteEligibility>> {
    let stored: StoredCreditModelParameters =
        serde_json::from_str(parameters_json).context("credit model parameters are invalid")?;
    let Some(eligibility) = stored.loan_eligibility else {
        ensure!(
            stored.schema_version == 2,
            "credit model eligibility schema is missing"
        );
        return Ok(None);
    };
    ensure!(
        matches!(stored.schema_version, 3..=5) && stored.provenance == "GAME_BALANCE",
        "credit model quote eligibility schema is unsupported"
    );
    let unsecured = eligibility.unsecured_loan;
    ensure!(
        !unsecured.allowed_credit_bands.is_empty() && unsecured.allowed_credit_bands.len() <= 5,
        "unsecured-loan allowed credit bands are invalid"
    );
    let mut seen_bands = BTreeSet::new();
    let allowed_credit_bands = unsecured
        .allowed_credit_bands
        .iter()
        .map(|band| {
            ensure!(
                seen_bands.insert(band.as_str()),
                "credit band eligibility is duplicated"
            );
            parse_credit_band(band)
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        unsecured.disallowed_contract_statuses == ["delinquent", "defaulted", "restructured"],
        "unsecured-loan restricted statuses are unsupported"
    );
    let _disallowed_contract_statuses = unsecured
        .disallowed_contract_statuses
        .iter()
        .map(|status| parse_contract_status(status))
        .collect::<Result<Vec<_>>>()?;
    let maximum_active_contracts = usize::from(unsecured.maximum_active_contracts);
    ensure!(
        (1..=MAX_ACTIVE_LOANS).contains(&maximum_active_contracts),
        "unsecured-loan active-contract limit is invalid"
    );
    Ok(Some(QuoteEligibility {
        allowed_credit_bands,
        maximum_active_contracts,
    }))
}

fn parse_lease_deposit_quote_eligibility(
    parameters_json: &str,
) -> Result<Option<LeaseDepositQuoteEligibility>> {
    let stored: StoredCreditModelParameters =
        serde_json::from_str(parameters_json).context("credit model parameters are invalid")?;
    if !matches!(stored.schema_version, 4 | 5) {
        return Ok(None);
    }
    ensure!(
        stored.provenance == "GAME_BALANCE",
        "lease-deposit credit model provenance is unsupported"
    );
    let loan_eligibility = stored
        .loan_eligibility
        .context("lease-deposit loan eligibility is missing")?;
    let eligibility = loan_eligibility
        .lease_deposit_loan
        .context("lease-deposit loan eligibility is missing")?;
    ensure!(
        !eligibility.allowed_credit_bands.is_empty() && eligibility.allowed_credit_bands.len() <= 5,
        "lease-deposit allowed credit bands are invalid"
    );
    let mut seen_bands = BTreeSet::new();
    let allowed_credit_bands = eligibility
        .allowed_credit_bands
        .iter()
        .map(|band| {
            ensure!(
                seen_bands.insert(band.as_str()),
                "lease-deposit credit band eligibility is duplicated"
            );
            parse_credit_band(band)
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        eligibility.disallowed_contract_statuses == ["delinquent", "defaulted", "restructured"],
        "lease-deposit restricted statuses are unsupported"
    );
    for status in &eligibility.disallowed_contract_statuses {
        parse_contract_status(status)?;
    }
    let maximum_active_contracts = usize::from(eligibility.maximum_active_contracts);
    ensure!(
        (1..=MAX_ACTIVE_LOANS).contains(&maximum_active_contracts),
        "lease-deposit active-contract limit is invalid"
    );
    let affordability = stored
        .lease_deposit_affordability
        .context("lease-deposit affordability rule is missing")?;
    ensure!(
        affordability.maximum_ratio_ppm > 0
            && affordability.new_loan_treatment == "interestOnly"
            && affordability.replacement_loan_treatment == "excluded",
        "lease-deposit affordability rule is unsupported"
    );
    Ok(Some(LeaseDepositQuoteEligibility {
        allowed_credit_bands,
        maximum_active_contracts,
        maximum_affordability_ratio_ppm: affordability.maximum_ratio_ppm,
    }))
}

fn parse_mortgage_quote_eligibility(
    parameters_json: &str,
) -> Result<Option<MortgageQuoteEligibility>> {
    let stored: StoredCreditModelParameters =
        serde_json::from_str(parameters_json).context("credit model parameters are invalid")?;
    if stored.schema_version != 5 {
        return Ok(None);
    }
    ensure!(
        stored.provenance == "GAME_BALANCE",
        "mortgage credit model provenance is unsupported"
    );
    let eligibility = stored
        .loan_eligibility
        .context("mortgage loan eligibility is missing")?
        .mortgage
        .context("mortgage loan eligibility is missing")?;
    ensure!(
        !eligibility.allowed_credit_bands.is_empty() && eligibility.allowed_credit_bands.len() <= 5,
        "mortgage allowed credit bands are invalid"
    );
    let mut seen_bands = BTreeSet::new();
    let allowed_credit_bands = eligibility
        .allowed_credit_bands
        .iter()
        .map(|band| {
            ensure!(
                seen_bands.insert(band.as_str()),
                "mortgage credit band eligibility is duplicated"
            );
            parse_credit_band(band)
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        eligibility.disallowed_contract_statuses == ["delinquent", "defaulted", "restructured"],
        "mortgage restricted statuses are unsupported"
    );
    for status in &eligibility.disallowed_contract_statuses {
        parse_contract_status(status)?;
    }
    let maximum_active_contracts = usize::from(eligibility.maximum_active_contracts);
    let maximum_active_holdings = usize::from(eligibility.maximum_active_holdings);
    ensure!(
        (1..=MAX_ACTIVE_LOANS).contains(&maximum_active_contracts) && maximum_active_holdings < 2,
        "mortgage active-contract or holding limit is invalid"
    );
    Ok(Some(MortgageQuoteEligibility {
        allowed_credit_bands,
        maximum_active_contracts,
        maximum_active_holdings,
    }))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn assess_mortgage_loan_in_tx(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
    save_id: u64,
    run_revision: u32,
    product_version_id: ResourceId,
    principal_krw: i64,
    replaced_loan_id: Option<ResourceId>,
    replaced_loan_principal_krw: i64,
) -> Result<MortgageLoanAssessmentResult> {
    if principal_krw <= 0 || replaced_loan_principal_krw < 0 {
        return Ok(MortgageLoanAssessmentResult::Rejected(
            LifeFailureCode::InvalidCommand,
        ));
    }
    let scope = read_loan_run_scope(tx, save_id, run_revision).await?;
    if scope.credit_model_version_key != "dev-unranked-m4c3-credit-2026-v4" {
        return Ok(MortgageLoanAssessmentResult::Rejected(
            LifeFailureCode::RateUnavailable,
        ));
    }
    let Some(eligibility) = parse_mortgage_quote_eligibility(&scope.model_parameters_json)? else {
        return Ok(MortgageLoanAssessmentResult::Rejected(
            LifeFailureCode::RateUnavailable,
        ));
    };
    let catalog = read_loan_product_catalog_in_tx(tx, user_id).await?;
    ensure!(
        catalog.credit_model_version_id
            == Some(ResourceId::from_u64(scope.credit_model_version_id)),
        "mortgage catalog disagrees with the run model"
    );
    let Some(product) = catalog
        .products
        .into_iter()
        .find(|product| product.id == product_version_id)
    else {
        return Ok(MortgageLoanAssessmentResult::Rejected(
            LifeFailureCode::InvalidCommand,
        ));
    };
    if product.kind != LoanProductKind::Mortgage
        || product.lender_sector != LoanLenderSector::Bank
        || product.rate_type != LoanRateType::Fixed
        || product.repayment_method != LoanRepaymentMethod::LevelPayment
        || product.term_months != 360
        || product.day_count_rule != LoanDayCountRule::Actual365
        || product.payment_calendar != LoanPaymentCalendar::MonthEnd
        || product.grace_months != 0
        || !product.quote_eligible
        || !product.execution_eligible
        || !product.prepayment_allowed
        || product.prepayment_fee_ppm != 10_000
        || product.prepayment_effect != LoanPrepaymentEffect::RecalculatePayment
        || !product.dsr_included
        || !(product.minimum_principal_krw..=product.maximum_principal_krw).contains(&principal_krw)
    {
        return Ok(MortgageLoanAssessmentResult::Rejected(
            LifeFailureCode::InvalidCommand,
        ));
    }
    if product.rate_status != LoanRateStatus::Available {
        return Ok(MortgageLoanAssessmentResult::Rejected(
            LifeFailureCode::RateUnavailable,
        ));
    }
    let annual_rate_bp = product
        .current_annual_rate_bp
        .context("mortgage product lost its current rate")?;
    let verified_income =
        read_verified_annual_income_in_tx(tx, scope.save_id, scope.run_revision, scope.game_day)
            .await?;
    let credit_band_raw: String = sqlx::query_scalar(
        "SELECT credit_band FROM credit_state
         WHERE save_id = ? AND run_revision = ? AND credit_model_version_id = ?",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.credit_model_version_id)
    .fetch_one(&mut **tx)
    .await?;
    let credit_band = parse_credit_band(&credit_band_raw)?;
    let loans: Vec<QuoteLoanRow> = sqlx::query_as(
        "SELECT id, status, product_kind, rate_type, current_annual_rate_bp,
                repayment_method, term_months, day_count_denominator,
                remaining_principal_krw,
                CAST(interest_remainder_numerator AS CHAR) AS interest_remainder_numerator,
                dsr_included, read_only
         FROM loan_contract
         WHERE save_id = ? AND run_revision = ?
           AND status IN ('pending', 'active', 'delinquent', 'defaulted', 'restructured')
         ORDER BY id
         LIMIT 9
         FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        loans.len() <= MAX_ACTIVE_LOANS,
        "active loan contracts exceed the run invariant"
    );
    if let Some(replaced_loan_id) = replaced_loan_id {
        ensure!(
            loans.iter().any(|loan| {
                loan.id == replaced_loan_id.get()
                    && loan.product_kind == "leaseDepositLoan"
                    && loan.remaining_principal_krw == replaced_loan_principal_krw
            }),
            "mortgage replacement loan is absent from the lock set"
        );
    } else {
        ensure!(
            replaced_loan_principal_krw == 0,
            "mortgage replacement principal has no loan"
        );
    }
    let existing_loan_balance_krw = loans.iter().try_fold(0_i64, |total, loan| {
        total
            .checked_add(loan.remaining_principal_krw)
            .context("mortgage existing balance overflowed")
    })?;
    let post_execution_balance_krw = existing_loan_balance_krw
        .checked_sub(replaced_loan_principal_krw)
        .context("mortgage replacement balance underflowed")?
        .checked_add(principal_krw)
        .context("mortgage post-execution balance overflowed")?;
    let periods =
        build_month_end_periods(scope.world_start_date, scope.game_day, product.term_months)?;
    let schedule_periods = periods
        .iter()
        .map(|period| period.calculation)
        .collect::<Vec<_>>();
    let schedule = create_loan_rules().build_schedule(LoanScheduleInput {
        principal_krw,
        initial_annual_rate_bp: annual_rate_bp,
        day_count: ACTUAL_365_DAY_COUNT,
        repayment_method: LoanRepaymentMethod::LevelPayment,
        prior_interest_remainder_numerator: 0,
        periods: &schedule_periods,
        rate_resets: &[],
    })?;
    let first = schedule
        .installments
        .first()
        .context("mortgage quote schedule has no first installment")?;
    let quoted_terms = LoanQuotedTermsState {
        annual_rate_bp,
        repayment_method: LoanRepaymentMethod::LevelPayment,
        term_months: product.term_months,
        first_installment: LoanQuoteFirstInstallmentState {
            due_game_day: first.due_game_day,
            fee_krw: 0,
            principal_krw: first.principal_krw,
            interest_krw: first.interest_krw,
            total_krw: first.payment_krw,
        },
    };
    let mut credit_reasons = Vec::new();
    if super::insolvency::credit_restricted_in_tx(
        tx,
        scope.save_id,
        scope.run_revision,
        scope.game_day,
    )
    .await?
    {
        credit_reasons.push(MortgageQuoteReasonState::InsolvencyRebuilding);
    }
    if loans.iter().any(|loan| loan.status == "defaulted") {
        credit_reasons.push(MortgageQuoteReasonState::ActiveDefault);
    }
    if loans.iter().any(|loan| loan.status == "delinquent") {
        credit_reasons.push(MortgageQuoteReasonState::ActiveDelinquency);
    }
    if loans.iter().any(|loan| loan.status == "restructured") {
        credit_reasons.push(MortgageQuoteReasonState::ActiveRestructuring);
    }
    if !eligibility.allowed_credit_bands.contains(&credit_band) {
        credit_reasons.push(MortgageQuoteReasonState::CreditBandRestricted);
    }
    let replacement_count = usize::from(replaced_loan_id.is_some());
    let active_contracts_after = loans
        .len()
        .checked_sub(replacement_count)
        .and_then(|count| count.checked_add(1))
        .context("mortgage active-contract count overflowed")?;
    if active_contracts_after > eligibility.maximum_active_contracts {
        credit_reasons.push(MortgageQuoteReasonState::ActiveLoanLimit);
    }
    let common_dsr_policy = read_dsr_policy_in_tx(tx, &scope).await?;
    let mortgage_dsr_policy = read_mortgage_dsr_policy_in_tx(tx, &scope).await?;
    validate_mortgage_dsr_policy(common_dsr_policy, mortgage_dsr_policy)?;
    let (dsr_applied, dsr, stress_rate_bp) = if let Some(evidence) =
        credit_restricted_mortgage_dsr_evidence(
            &credit_reasons,
            post_execution_balance_krw,
            mortgage_dsr_policy,
        ) {
        evidence
    } else {
        assess_mortgage_quote_dsr(
            tx,
            &scope,
            &product,
            &loans,
            &schedule_periods,
            principal_krw,
            verified_income
                .as_ref()
                .map(|income| income.annual_income_krw),
            common_dsr_policy,
            mortgage_dsr_policy,
        )
        .await?
    };
    Ok(MortgageLoanAssessmentResult::Assessed(Box::new(
        MortgageLoanAssessment {
            scope,
            product,
            periods,
            schedule,
            credit_reasons,
            maximum_active_holdings: eligibility.maximum_active_holdings,
            verified_annual_income_krw: verified_income
                .as_ref()
                .map(|income| income.annual_income_krw),
            verified_income_source: verified_income.map(|income| income.source),
            existing_loan_balance_krw,
            post_execution_balance_krw,
            dsr_applied,
            dsr,
            stress_rate_bp,
            quoted_terms,
        },
    )))
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
struct MortgageDsrPolicyRow {
    borrower_dsr_balance_threshold_krw: i64,
    bank_dsr_limit_ppm: u32,
    evaluation_horizon_months: u8,
    full_term_fixed_stress_rate_bp: u16,
}

#[allow(clippy::too_many_arguments)]
async fn assess_mortgage_quote_dsr(
    tx: &mut Transaction<'_, MySql>,
    scope: &LoanRunScopeRow,
    product: &LoanProductState,
    loans: &[QuoteLoanRow],
    candidate_periods: &[LoanSchedulePeriod],
    requested_principal_krw: i64,
    verified_annual_income_krw: Option<i64>,
    common_policy: LoadedDsrPolicy,
    mortgage_policy: MortgageDsrPolicyRow,
) -> Result<(bool, Option<LoanQuoteDsrState>, i64)> {
    let mut owned = read_existing_dsr_loans_in_tx(tx, scope, loans).await?;
    let candidate_id = owned
        .iter()
        .map(|loan| loan.loan_id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .context("prospective mortgage id overflowed")?;
    owned.push(OwnedDsrLoan {
        loan_id: candidate_id,
        included_in_dsr: true,
        counts_toward_general_loan_balance: true,
        counts_toward_credit_stress_balance: false,
        rate_type: LoanRateType::Fixed,
        fixed_rate_period_months: product.term_months,
        payment_treatment: DsrPaymentTreatment::Scheduled,
        principal_krw: requested_principal_krw,
        initial_annual_rate_bp: product
            .current_annual_rate_bp
            .context("mortgage product lost its current rate")?,
        repayment_method: LoanRepaymentMethod::LevelPayment,
        prior_interest_remainder_numerator: 0,
        periods: candidate_periods.to_vec(),
        rate_resets: Vec::new(),
    });
    let inputs = owned
        .iter()
        .map(|loan| DsrLoanInput {
            loan_id: loan.loan_id,
            included_in_dsr: loan.included_in_dsr,
            counts_toward_general_loan_balance: loan.counts_toward_general_loan_balance,
            counts_toward_credit_stress_balance: loan.counts_toward_credit_stress_balance,
            rate_type: loan.rate_type,
            fixed_rate_period_months: loan.fixed_rate_period_months,
            payment_treatment: loan.payment_treatment,
            schedule: LoanScheduleInput {
                principal_krw: loan.principal_krw,
                initial_annual_rate_bp: loan.initial_annual_rate_bp,
                day_count: ACTUAL_365_DAY_COUNT,
                repayment_method: loan.repayment_method,
                prior_interest_remainder_numerator: loan.prior_interest_remainder_numerator,
                periods: &loan.periods,
                rate_resets: &loan.rate_resets,
            },
        })
        .collect::<Vec<_>>();
    let assessment = create_loan_rules().assess_dsr(DsrAssessmentInput {
        evaluation_game_day: scope.game_day,
        evaluation_end_game_day: twelve_month_horizon_game_day(
            scope.world_start_date,
            scope.game_day,
        )?,
        verified_annual_income_krw: verified_annual_income_krw.or(Some(1)),
        policy: DsrPolicy {
            general_loan_balance_gate_krw: mortgage_policy.borrower_dsr_balance_threshold_krw,
            maximum_ratio_ppm: i64::from(mortgage_policy.bank_dsr_limit_ppm),
            credit_balance_stress_gate_krw: common_policy.credit_balance_stress_gate_krw,
            base_stress_rate_bp: common_policy.base_stress_rate_bp,
            medium_fixed_stress_multiplier_ppm: common_policy.medium_fixed_stress_multiplier_ppm,
        },
        loans: &inputs,
    })?;
    let stress_rate_bp = assessment
        .loan_contributions
        .iter()
        .find(|contribution| contribution.loan_id == candidate_id)
        .map(|contribution| contribution.stress_rate_bp)
        .context("prospective mortgage DSR contribution is missing")?;
    ensure!(
        stress_rate_bp == i64::from(mortgage_policy.full_term_fixed_stress_rate_bp),
        "full-term fixed mortgage received a stress surcharge"
    );
    if !assessment.gate_applied {
        return Ok((false, None, stress_rate_bp));
    }
    let Some(verified_annual_income_krw) = verified_annual_income_krw else {
        return Ok((true, None, stress_rate_bp));
    };
    ensure!(
        assessment.denominator_krw == Some(verified_annual_income_krw),
        "mortgage DSR denominator disagrees with verified income"
    );
    Ok((
        true,
        Some(LoanQuoteDsrState {
            numerator_krw: assessment.numerator_krw,
            denominator_krw: verified_annual_income_krw,
            ratio_ppm: assessment
                .ratio_ppm
                .context("mortgage DSR ratio is missing")?,
            limit_ppm: assessment.maximum_ratio_ppm,
        }),
        stress_rate_bp,
    ))
}

async fn read_mortgage_dsr_policy_in_tx(
    tx: &mut Transaction<'_, MySql>,
    scope: &LoanRunScopeRow,
) -> Result<MortgageDsrPolicyRow> {
    sqlx::query_as(
        "SELECT profile.borrower_dsr_balance_threshold_krw,
                profile.bank_dsr_limit_ppm, profile.evaluation_horizon_months,
                profile.full_term_fixed_stress_rate_bp
         FROM credit_model_version AS model
         INNER JOIN credit_mortgage_policy_profile AS profile
           ON profile.policy_set_id = model.credit_policy_set_id
         WHERE model.id = ?",
    )
    .bind(scope.credit_model_version_id)
    .fetch_one(&mut **tx)
    .await
    .context("mortgage DSR profile is unavailable")
}

fn validate_mortgage_dsr_policy(
    common_policy: LoadedDsrPolicy,
    mortgage_policy: MortgageDsrPolicyRow,
) -> Result<()> {
    ensure!(
        mortgage_policy.borrower_dsr_balance_threshold_krw
            == common_policy.general_loan_balance_gate_krw
            && i64::from(mortgage_policy.bank_dsr_limit_ppm) == common_policy.bank_limit_ppm
            && mortgage_policy.evaluation_horizon_months == 12
            && mortgage_policy.full_term_fixed_stress_rate_bp == 0,
        "mortgage DSR profile disagrees with the sealed credit policy"
    );
    Ok(())
}

fn credit_restricted_mortgage_dsr_evidence(
    credit_reasons: &[MortgageQuoteReasonState],
    post_execution_balance_krw: i64,
    mortgage_policy: MortgageDsrPolicyRow,
) -> Option<(bool, Option<LoanQuoteDsrState>, i64)> {
    (!credit_reasons.is_empty()).then_some((
        post_execution_balance_krw > mortgage_policy.borrower_dsr_balance_threshold_krw,
        None,
        i64::from(mortgage_policy.full_term_fixed_stress_rate_bp),
    ))
}

async fn read_dsr_policy_in_tx(
    tx: &mut Transaction<'_, MySql>,
    scope: &LoanRunScopeRow,
) -> Result<LoadedDsrPolicy> {
    let current_date = scope
        .world_start_date
        .checked_add(Duration::days(i64::from(scope.game_day)))
        .context("loan quote market date overflowed")?;
    let rows: Vec<QuotePolicyRow> = sqlx::query_as(
        "SELECT rule.rule_key,
                CAST(rule.parameters AS CHAR CHARACTER SET utf8mb4) AS parameters_json
         FROM credit_model_version AS model
         INNER JOIN policy_rule AS rule
           ON rule.policy_set_id = model.credit_policy_set_id
          AND rule.domain = 'credit'
          AND rule.rule_key IN (
              'borrowerDsrLimits',
              'otherLoanDsrInclusion',
              'unsecuredStressDsr2026H2'
          )
          AND rule.effective_from <= ?
          AND (rule.effective_to IS NULL OR rule.effective_to >= ?)
         WHERE model.id = ?
         ORDER BY rule.rule_key, rule.id",
    )
    .bind(current_date)
    .bind(current_date)
    .bind(scope.credit_model_version_id)
    .fetch_all(&mut **tx)
    .await?;
    let mut by_key = BTreeMap::new();
    for row in rows {
        ensure!(
            by_key.insert(row.rule_key, row.parameters_json).is_none(),
            "credit policy contains a duplicate effective rule"
        );
    }
    let limits_json = by_key
        .remove("borrowerDsrLimits")
        .context("borrower DSR limit rule is missing")?;
    let limits: StoredBorrowerDsrLimits =
        serde_json::from_str(&limits_json).context("borrower DSR limit rule is invalid")?;
    let inclusion_json = by_key
        .remove("otherLoanDsrInclusion")
        .context("other-loan DSR inclusion rule is missing")?;
    let inclusion: StoredOtherLoanDsrInclusion = serde_json::from_str(&inclusion_json)
        .context("other-loan DSR inclusion rule is invalid")?;
    let stress = by_key
        .remove("unsecuredStressDsr2026H2")
        .map(|parameters| {
            serde_json::from_str::<StoredUnsecuredStressDsr>(&parameters)
                .context("unsecured stress DSR rule is invalid")
        })
        .transpose()?;
    ensure!(
        by_key.is_empty(),
        "credit policy contains an unknown DSR rule"
    );
    ensure!(
        limits.schema_version == 1
            && limits.annual_income_status_required == "verified"
            && limits.application_balance_boundary == "strictlyGreaterThan"
            && limits.application_balance_threshold_krw > 0
            && limits.bank_limit_ppm > 0
            && limits.non_bank_limit_ppm > 0
            && limits.evaluation_horizon_months == 12
            && limits.ratio_scale_ppm == 1_000_000,
        "borrower DSR limit rule is unsupported"
    );
    ensure!(
        inclusion.schema_version == 1
            && inclusion.bullet_amortization_months == 60
            && inclusion.included_product_kinds == ["studentLoan", "unsecuredLoan"]
            && inclusion.scheduled_loan_measure == "nextTwelveMonthsPrincipalAndInterest"
            && inclusion.student_loan_classification == "otherHouseholdLoan",
        "other-loan DSR inclusion rule is unsupported"
    );
    if let Some(stress) = stress.as_ref() {
        ensure!(
            stress.schema_version == 1
                && stress.application_balance_boundary == "strictlyGreaterThan"
                && stress.application_balance_threshold_krw > 0
                && stress.fixed_at_least_five_years_application_ppm == 0
                && stress.fixed_at_least_three_years_application_ppm == 600_000
                && stress.other_fixed_or_variable_application_ppm == 1_000_000
                && stress.stress_rate_bp >= 0,
            "unsecured stress DSR rule is unsupported"
        );
    }
    Ok(LoadedDsrPolicy {
        general_loan_balance_gate_krw: limits.application_balance_threshold_krw,
        bank_limit_ppm: limits.bank_limit_ppm,
        non_bank_limit_ppm: limits.non_bank_limit_ppm,
        credit_balance_stress_gate_krw: stress
            .as_ref()
            .map_or(i64::MAX, |rule| rule.application_balance_threshold_krw),
        base_stress_rate_bp: stress.as_ref().map_or(0, |rule| rule.stress_rate_bp),
        medium_fixed_stress_multiplier_ppm: stress
            .map_or(0, |rule| rule.fixed_at_least_three_years_application_ppm),
    })
}

async fn read_existing_dsr_loans_in_tx(
    tx: &mut Transaction<'_, MySql>,
    scope: &LoanRunScopeRow,
    loans: &[QuoteLoanRow],
) -> Result<Vec<OwnedDsrLoan>> {
    let mut result = Vec::new();
    for loan in loans.iter().filter(|loan| loan.dsr_included) {
        ensure!(!loan.read_only, "read-only loan is included in DSR");
        let status = parse_contract_status(&loan.status)?;
        ensure!(
            matches!(
                status,
                LoanContractStatus::Pending | LoanContractStatus::Active
            ),
            "restricted loan reached DSR calculation"
        );
        let product_kind = parse_product_kind(&loan.product_kind)?;
        ensure!(
            matches!(
                product_kind,
                LoanProductKind::StudentLoan
                    | LoanProductKind::UnsecuredLoan
                    | LoanProductKind::Mortgage
            ),
            "unsupported loan kind is included in M4-B DSR"
        );
        ensure!(
            loan.remaining_principal_krw > 0
                && loan.day_count_denominator == Some(ACTUAL_365_DAY_COUNT),
            "DSR loan balance or day-count is invalid"
        );
        let installments: Vec<QuoteInstallmentRow> = sqlx::query_as(
            "SELECT installment_no, due_game_day, elapsed_days, annual_rate_bp
             FROM loan_installment
             WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
               AND status IN ('pending', 'due', 'partiallyPaid')
               AND due_game_day > ?
             ORDER BY installment_no
             FOR UPDATE",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(loan.id)
        .bind(scope.game_day)
        .fetch_all(&mut **tx)
        .await?;
        ensure!(
            !installments.is_empty(),
            "DSR loan has no future installments"
        );
        for pair in installments.windows(2) {
            ensure!(
                pair[0].installment_no.checked_add(1) == Some(pair[1].installment_no),
                "DSR loan installments are not contiguous"
            );
        }
        let initial_annual_rate_bp = i64::from(
            installments
                .first()
                .context("DSR loan has no first installment")?
                .annual_rate_bp,
        );
        ensure!(
            loan.current_annual_rate_bp.map(i64::from) == Some(initial_annual_rate_bp),
            "DSR loan current rate disagrees with its next installment"
        );
        let mut rate_resets = Vec::new();
        for (index, pair) in installments.windows(2).enumerate() {
            if pair[0].annual_rate_bp != pair[1].annual_rate_bp {
                rate_resets.push(LoanRateReset {
                    after_installment_sequence: u16::try_from(index + 1)
                        .context("DSR rate reset index overflowed")?,
                    next_annual_rate_bp: i64::from(pair[1].annual_rate_bp),
                });
            }
        }
        let rate_type = parse_rate_type(&loan.rate_type)?;
        let repayment_method = parse_repayment_method(&loan.repayment_method)?;
        let term_months = loan.term_months.context("DSR loan term is missing")?;
        let periods = installments
            .iter()
            .map(|installment| LoanSchedulePeriod {
                due_game_day: installment.due_game_day,
                elapsed_days: installment.elapsed_days,
            })
            .collect();
        result.push(OwnedDsrLoan {
            loan_id: loan.id,
            included_in_dsr: true,
            counts_toward_general_loan_balance: true,
            counts_toward_credit_stress_balance: product_kind == LoanProductKind::UnsecuredLoan,
            rate_type,
            fixed_rate_period_months: if rate_type == LoanRateType::Fixed {
                term_months
            } else {
                0
            },
            payment_treatment: if product_kind == LoanProductKind::UnsecuredLoan
                && repayment_method == LoanRepaymentMethod::Bullet
            {
                DsrPaymentTreatment::BulletCreditFiveYear
            } else {
                DsrPaymentTreatment::Scheduled
            },
            principal_krw: loan.remaining_principal_krw,
            initial_annual_rate_bp,
            repayment_method,
            prior_interest_remainder_numerator: loan
                .interest_remainder_numerator
                .parse()
                .context("DSR loan interest remainder is invalid")?,
            periods,
            rate_resets,
        });
    }
    Ok(result)
}

fn twelve_month_horizon_game_day(world_start_date: Date, game_day: u32) -> Result<u32> {
    let current_date = world_start_date
        .checked_add(Duration::days(i64::from(game_day)))
        .context("DSR evaluation date overflowed")?;
    let next_year = current_date
        .year()
        .checked_add(1)
        .context("DSR evaluation year overflowed")?;
    let day = current_date
        .day()
        .min(current_date.month().length(next_year));
    let end_date = Date::from_calendar_date(next_year, current_date.month(), day)
        .context("DSR evaluation end date is invalid")?;
    game_day_for_date(world_start_date, end_date)
}

const fn loan_quote_decision_db(decision: LoanQuoteDecisionState) -> &'static str {
    match decision {
        LoanQuoteDecisionState::Eligible => "eligible",
        LoanQuoteDecisionState::DebtServiceLimit => "debtServiceLimit",
        LoanQuoteDecisionState::IncomeUnavailable => "incomeUnavailable",
        LoanQuoteDecisionState::CreditRestricted => "creditRestricted",
        LoanQuoteDecisionState::ValuationUnavailable => "valuationUnavailable",
    }
}

pub(super) async fn read_loan_detail_in_tx(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
    loan_id: ResourceId,
) -> Result<Option<LoanDetailState>> {
    let row: Option<LoanDetailRow> = sqlx::query_as(
        "SELECT contract.id,
                contract.loan_product_version_id AS product_version_id,
                contract.product_kind, product.display_name, contract.rate_status,
                contract.current_annual_rate_bp, contract.status, contract.read_only,
                contract.original_principal_krw, contract.remaining_principal_krw,
                contract.accrued_interest_krw, contract.accrued_fee_krw,
                CAST(COALESCE((
                    SELECT SUM(bucket.original_amount_krw - bucket.paid_amount_krw)
                    FROM loan_obligation_bucket AS bucket
                    WHERE bucket.save_id = contract.save_id
                      AND bucket.run_revision = contract.run_revision
                      AND bucket.loan_contract_id = contract.id
                      AND bucket.status = 'delinquent'
                ), 0) AS SIGNED) AS overdue_krw,
                contract.repayment_method, contract.term_months,
                contract.total_installments, contract.activated_game_day,
                contract.maturity_game_day,
                (
                    SELECT MAX(installment.due_game_day)
                    FROM loan_installment AS installment
                    WHERE installment.save_id = contract.save_id
                      AND installment.run_revision = contract.run_revision
                      AND installment.loan_contract_id = contract.id
                      AND installment.status <> 'cancelled'
                ) AS final_installment_due_game_day,
                contract.next_installment_no, contract.oldest_unpaid_due_game_day,
                product.prepayment_allowed AS product_prepayment_allowed,
                contract.prepayment_fee_ppm, contract.prepayment_effect,
                contract.dsr_included, contract.lease_contract_id,
                contract.property_holding_id
         FROM save
         INNER JOIN `character` AS current_character
           ON current_character.save_id = save.id
         INNER JOIN loan_contract AS contract
           ON contract.save_id = save.id
          AND contract.run_revision = save.run_revision
         INNER JOIN loan_product_version AS product
           ON product.id = contract.loan_product_version_id
         WHERE save.user_id = ? AND contract.id = ?",
    )
    .bind(user_id)
    .bind(loan_id.get())
    .fetch_optional(&mut **tx)
    .await?;

    row.map(loan_detail_from_row).transpose()
}

fn loan_detail_from_row(row: LoanDetailRow) -> Result<LoanDetailState> {
    ensure!(
        row.id > 0 && row.product_version_id > 0 && !row.display_name.is_empty(),
        "loan detail has invalid identity metadata"
    );
    ensure!(
        row.original_principal_krw > 0
            && row.remaining_principal_krw >= 0
            && row.remaining_principal_krw <= row.original_principal_krw
            && row.accrued_interest_krw >= 0
            && row.accrued_fee_krw >= 0
            && row.overdue_krw >= 0,
        "loan detail has invalid balances"
    );
    let rate_status = parse_rate_status(&row.rate_status)?;
    ensure!(
        matches!(
            (rate_status, row.current_annual_rate_bp),
            (LoanRateStatus::Available, Some(_)) | (LoanRateStatus::RateUnavailable, None)
        ),
        "loan detail rate status disagrees with its current rate"
    );
    let prepayment_effect = match row.prepayment_effect.as_str() {
        "forbidden" => None,
        value => Some(parse_prepayment_effect(value)?),
    };
    let replicated_prepayment_capability =
        !row.read_only && row.prepayment_fee_ppm.is_some() && prepayment_effect.is_some();
    ensure!(
        row.product_prepayment_allowed == replicated_prepayment_capability,
        "loan contract prepayment capability disagrees with its immutable product"
    );
    if !row.product_prepayment_allowed {
        ensure!(
            row.prepayment_fee_ppm.is_none() && prepayment_effect.is_none(),
            "loan without prepayment capability exposes prepayment terms"
        );
    }
    let status = parse_contract_status(&row.status)?;
    if row.read_only {
        ensure!(
            row.term_months.is_none()
                && row.total_installments.is_none()
                && row.maturity_game_day.is_none()
                && row.final_installment_due_game_day.is_none()
                && row.next_installment_no.is_none()
                && row.oldest_unpaid_due_game_day.is_none()
                && !row.product_prepayment_allowed,
            "read-only loan detail has mutable servicing terms"
        );
    } else {
        ensure!(
            row.term_months.is_some()
                && row.total_installments.is_some()
                && row.maturity_game_day.is_some(),
            "serviced loan detail has incomplete terms"
        );
    }
    if matches!(
        status,
        LoanContractStatus::Pending | LoanContractStatus::Active | LoanContractStatus::Delinquent
    ) && !row.read_only
    {
        ensure!(
            row.final_installment_due_game_day.is_some() && row.next_installment_no.is_some(),
            "open serviced loan has no current schedule position"
        );
    } else {
        ensure!(
            row.next_installment_no.is_none(),
            "terminal loan retains a next installment"
        );
    }

    Ok(LoanDetailState {
        id: ResourceId::from_u64(row.id),
        product_version_id: ResourceId::from_u64(row.product_version_id),
        product_kind: parse_product_kind(&row.product_kind)?,
        display_name: row.display_name,
        rate_status,
        current_annual_rate_bp: row.current_annual_rate_bp.map(i64::from),
        status,
        read_only: row.read_only,
        original_principal_krw: row.original_principal_krw,
        remaining_principal_krw: row.remaining_principal_krw,
        accrued_interest_krw: row.accrued_interest_krw,
        accrued_fee_krw: row.accrued_fee_krw,
        overdue_krw: row.overdue_krw,
        repayment_method: parse_repayment_method(&row.repayment_method)?,
        term_months: row.term_months,
        total_installments: row.total_installments,
        activated_game_day: row.activated_game_day,
        maturity_game_day: row.maturity_game_day,
        final_installment_due_game_day: row.final_installment_due_game_day,
        next_installment_no: row.next_installment_no,
        oldest_unpaid_due_game_day: row.oldest_unpaid_due_game_day,
        prepayment_allowed: row.product_prepayment_allowed,
        prepayment_fee_ppm: row.prepayment_fee_ppm,
        prepayment_effect,
        dsr_included: row.dsr_included,
        lease_contract_id: row.lease_contract_id.map(ResourceId::from_u64),
        property_holding_id: row.property_holding_id.map(ResourceId::from_u64),
    })
}

async fn read_owned_loan_scope_in_tx(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
    loan_id: ResourceId,
) -> Result<Option<OwnedLoanScopeRow>> {
    Ok(sqlx::query_as(
        "SELECT contract.save_id, contract.run_revision
         FROM save
         INNER JOIN `character` AS current_character
           ON current_character.save_id = save.id
         INNER JOIN loan_contract AS contract
           ON contract.save_id = save.id
          AND contract.run_revision = save.run_revision
         WHERE save.user_id = ? AND contract.id = ?",
    )
    .bind(user_id)
    .bind(loan_id.get())
    .fetch_optional(&mut **tx)
    .await?)
}

pub(super) async fn read_loan_installment_page_in_tx(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
    loan_id: ResourceId,
    query: LoanInstallmentPageQuery,
) -> Result<Option<LoanInstallmentPageState>> {
    ensure!(
        (1..=MAX_LOAN_HISTORY_PAGE_SIZE).contains(&usize::from(query.limit)),
        "loan history page limit is out of range"
    );
    if let Some(before) = query.before {
        ensure!(
            before.loan_id == loan_id,
            "loan history cursor belongs to another contract"
        );
    }
    let Some(scope) = read_owned_loan_scope_in_tx(tx, user_id, loan_id).await? else {
        return Ok(None);
    };
    ensure!(scope.save_id > 0, "owned loan scope has an invalid save id");

    let fetch_limit = u32::from(query.limit) + 1;
    let installment_window = query.before.map(|before| before.installment_before);
    let installment_rows = match installment_window {
        Some(None) => Vec::new(),
        None => {
            sqlx::query_as(
                "SELECT id, installment_no, due_game_day,
                        interest_period_start_game_day, elapsed_days, annual_rate_bp,
                        opening_principal_krw, scheduled_fee_krw,
                        scheduled_interest_krw, scheduled_principal_krw,
                        paid_fee_krw, paid_interest_krw, paid_principal_krw,
                        status, schedule_revision
                 FROM loan_installment
                 WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
                 ORDER BY installment_no DESC
                 LIMIT ?",
            )
            .bind(scope.save_id)
            .bind(scope.run_revision)
            .bind(loan_id.get())
            .bind(fetch_limit)
            .fetch_all(&mut **tx)
            .await?
        }
        Some(Some(before)) => {
            sqlx::query_as(
                "SELECT id, installment_no, due_game_day,
                        interest_period_start_game_day, elapsed_days, annual_rate_bp,
                        opening_principal_krw, scheduled_fee_krw,
                        scheduled_interest_krw, scheduled_principal_krw,
                        paid_fee_krw, paid_interest_krw, paid_principal_krw,
                        status, schedule_revision
                 FROM loan_installment
                 WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
                   AND installment_no < ?
                 ORDER BY installment_no DESC
                 LIMIT ?",
            )
            .bind(scope.save_id)
            .bind(scope.run_revision)
            .bind(loan_id.get())
            .bind(before)
            .bind(fetch_limit)
            .fetch_all(&mut **tx)
            .await?
        }
    };
    let (installments, has_more_installments) =
        build_installment_window(installment_rows, usize::from(query.limit))?;

    let payment_window = query.before.map(|before| before.payment_before);
    let payment_rows = match payment_window {
        Some(None) => Vec::new(),
        None => {
            sqlx::query_as(
                "SELECT id, payment_no, payment_kind, amount_krw, game_day, status
                 FROM loan_payment
                 WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
                   AND status = 'applied'
                 ORDER BY payment_no DESC
                 LIMIT ?",
            )
            .bind(scope.save_id)
            .bind(scope.run_revision)
            .bind(loan_id.get())
            .bind(fetch_limit)
            .fetch_all(&mut **tx)
            .await?
        }
        Some(Some(before)) => {
            sqlx::query_as(
                "SELECT id, payment_no, payment_kind, amount_krw, game_day, status
                 FROM loan_payment
                 WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
                   AND status = 'applied' AND payment_no < ?
                 ORDER BY payment_no DESC
                 LIMIT ?",
            )
            .bind(scope.save_id)
            .bind(scope.run_revision)
            .bind(loan_id.get())
            .bind(before)
            .bind(fetch_limit)
            .fetch_all(&mut **tx)
            .await?
        }
    };
    let (payment_rows, has_more_payments) =
        select_payment_window(payment_rows, usize::from(query.limit))?;
    let allocation_rows =
        read_payment_allocation_window(tx, scope, loan_id, payment_rows.as_slice()).await?;
    let payments = build_payment_history(payment_rows, allocation_rows)?;

    let next_before = if has_more_installments || has_more_payments {
        Some(LoanInstallmentPageCursor {
            loan_id,
            installment_before: if has_more_installments {
                Some(
                    installments
                        .last()
                        .context("continuing installment window is empty")?
                        .installment_no,
                )
            } else {
                None
            },
            payment_before: if has_more_payments {
                Some(
                    payments
                        .last()
                        .context("continuing payment window is empty")?
                        .payment_no,
                )
            } else {
                None
            },
        })
    } else {
        None
    };

    Ok(Some(LoanInstallmentPageState {
        loan_id,
        installments,
        payments,
        has_more_installments,
        has_more_payments,
        next_before,
    }))
}

fn build_installment_window(
    rows: Vec<LoanInstallmentHistoryRow>,
    limit: usize,
) -> Result<(Vec<LoanInstallmentState>, bool)> {
    ensure!(
        limit > 0 && limit <= MAX_LOAN_HISTORY_PAGE_SIZE && rows.len() <= limit.saturating_add(1),
        "installment window exceeds its bound"
    );
    ensure!(
        rows.windows(2)
            .all(|pair| pair[0].installment_no > pair[1].installment_no),
        "installment window is not strictly descending"
    );
    let has_more = rows.len() > limit;
    let installments = rows
        .into_iter()
        .take(limit)
        .map(loan_installment_from_row)
        .collect::<Result<Vec<_>>>()?;
    Ok((installments, has_more))
}

fn loan_installment_from_row(row: LoanInstallmentHistoryRow) -> Result<LoanInstallmentState> {
    ensure!(
        row.id > 0 && row.installment_no > 0 && row.schedule_revision > 0,
        "loan installment has invalid identity metadata"
    );
    let expected_elapsed_days = row
        .due_game_day
        .checked_sub(row.interest_period_start_game_day)
        .and_then(|days| days.checked_add(1))
        .context("loan installment interest period is invalid")?;
    ensure!(
        expected_elapsed_days == u32::from(row.elapsed_days) && row.elapsed_days > 0,
        "loan installment elapsed days disagree with its period"
    );
    ensure!(
        row.opening_principal_krw > 0
            && row.scheduled_fee_krw >= 0
            && row.scheduled_interest_krw >= 0
            && row.scheduled_principal_krw >= 0
            && (0..=row.scheduled_fee_krw).contains(&row.paid_fee_krw)
            && (0..=row.scheduled_interest_krw).contains(&row.paid_interest_krw)
            && (0..=row.scheduled_principal_krw).contains(&row.paid_principal_krw),
        "loan installment has invalid amounts"
    );
    let status = parse_installment_status(&row.status)?;
    let scheduled_total_krw = row
        .scheduled_fee_krw
        .checked_add(row.scheduled_interest_krw)
        .and_then(|value| value.checked_add(row.scheduled_principal_krw))
        .context("loan installment scheduled amount overflowed")?;
    let paid_total_krw = row
        .paid_fee_krw
        .checked_add(row.paid_interest_krw)
        .and_then(|value| value.checked_add(row.paid_principal_krw))
        .context("loan installment paid amount overflowed")?;
    ensure!(
        scheduled_total_krw > 0
            && match status {
                LoanInstallmentStatusState::Pending | LoanInstallmentStatusState::Due => {
                    paid_total_krw == 0
                }
                LoanInstallmentStatusState::PartiallyPaid => {
                    paid_total_krw > 0 && paid_total_krw < scheduled_total_krw
                }
                LoanInstallmentStatusState::Paid => paid_total_krw == scheduled_total_krw,
                LoanInstallmentStatusState::Cancelled => paid_total_krw == 0,
            },
        "loan installment status disagrees with its paid amount"
    );
    let remaining_due_krw = if status == LoanInstallmentStatusState::Cancelled {
        0
    } else {
        scheduled_total_krw
            .checked_sub(paid_total_krw)
            .context("loan installment remaining amount underflowed")?
    };

    Ok(LoanInstallmentState {
        id: ResourceId::from_u64(row.id),
        installment_no: row.installment_no,
        due_game_day: row.due_game_day,
        interest_period_start_game_day: row.interest_period_start_game_day,
        elapsed_days: row.elapsed_days,
        annual_rate_bp: i64::from(row.annual_rate_bp),
        opening_principal_krw: row.opening_principal_krw,
        scheduled_fee_krw: row.scheduled_fee_krw,
        scheduled_interest_krw: row.scheduled_interest_krw,
        scheduled_principal_krw: row.scheduled_principal_krw,
        paid_fee_krw: row.paid_fee_krw,
        paid_interest_krw: row.paid_interest_krw,
        paid_principal_krw: row.paid_principal_krw,
        remaining_due_krw,
        status,
        schedule_revision: row.schedule_revision,
    })
}

fn parse_installment_status(value: &str) -> Result<LoanInstallmentStatusState> {
    match value {
        "pending" => Ok(LoanInstallmentStatusState::Pending),
        "due" => Ok(LoanInstallmentStatusState::Due),
        "partiallyPaid" => Ok(LoanInstallmentStatusState::PartiallyPaid),
        "paid" => Ok(LoanInstallmentStatusState::Paid),
        "cancelled" => Ok(LoanInstallmentStatusState::Cancelled),
        _ => bail!("unknown loan installment status"),
    }
}

fn select_payment_window(
    mut rows: Vec<LoanPaymentHistoryRow>,
    limit: usize,
) -> Result<(Vec<LoanPaymentHistoryRow>, bool)> {
    ensure!(
        limit > 0 && limit <= MAX_LOAN_HISTORY_PAGE_SIZE && rows.len() <= limit.saturating_add(1),
        "payment window exceeds its bound"
    );
    ensure!(
        rows.windows(2)
            .all(|pair| pair[0].payment_no > pair[1].payment_no),
        "payment window is not strictly descending"
    );
    ensure!(
        rows.iter().all(|row| row.status == "applied"),
        "payment window contains a non-applied payment"
    );
    let has_more = rows.len() > limit;
    rows.truncate(limit);
    Ok((rows, has_more))
}

async fn read_payment_allocation_window(
    tx: &mut Transaction<'_, MySql>,
    scope: OwnedLoanScopeRow,
    loan_id: ResourceId,
    payments: &[LoanPaymentHistoryRow],
) -> Result<Vec<LoanPaymentAllocationHistoryRow>> {
    let Some(highest_payment_no) = payments.first().map(|payment| payment.payment_no) else {
        return Ok(Vec::new());
    };
    let lowest_payment_no = payments
        .last()
        .map(|payment| payment.payment_no)
        .context("payment allocation window is empty")?;
    Ok(sqlx::query_as(
        "SELECT allocation.loan_payment_id, payment.payment_no,
                allocation.allocation_order, allocation.allocation_kind,
                allocation.amount_krw
         FROM loan_payment AS payment
         INNER JOIN loan_payment_allocation AS allocation
           ON allocation.save_id = payment.save_id
          AND allocation.run_revision = payment.run_revision
          AND allocation.loan_contract_id = payment.loan_contract_id
          AND allocation.loan_payment_id = payment.id
         WHERE payment.save_id = ? AND payment.run_revision = ?
           AND payment.loan_contract_id = ? AND payment.status = 'applied'
           AND payment.payment_no BETWEEN ? AND ?
         ORDER BY payment.payment_no DESC, allocation.allocation_order",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(loan_id.get())
    .bind(lowest_payment_no)
    .bind(highest_payment_no)
    .fetch_all(&mut **tx)
    .await?)
}

fn build_payment_history(
    rows: Vec<LoanPaymentHistoryRow>,
    allocation_rows: Vec<LoanPaymentAllocationHistoryRow>,
) -> Result<Vec<LoanPaymentState>> {
    let payment_numbers = rows
        .iter()
        .map(|row| (row.id, row.payment_no))
        .collect::<BTreeMap<_, _>>();
    ensure!(
        payment_numbers.len() == rows.len(),
        "payment window has duplicate ids"
    );
    let mut grouped_allocations = BTreeMap::<u64, Vec<LoanPaymentAllocationHistoryRow>>::new();
    let mut previous_payment_no = None;
    let mut previous_allocation_order = 0;
    for allocation in allocation_rows {
        ensure!(
            payment_numbers.get(&allocation.loan_payment_id) == Some(&allocation.payment_no),
            "payment allocation is outside its parent window"
        );
        if previous_payment_no == Some(allocation.payment_no) {
            ensure!(
                allocation.allocation_order > previous_allocation_order,
                "payment allocations are not strictly ordered"
            );
        } else {
            if let Some(previous) = previous_payment_no {
                ensure!(
                    previous > allocation.payment_no,
                    "payment allocation groups are not strictly descending"
                );
            }
            previous_payment_no = Some(allocation.payment_no);
        }
        ensure!(
            allocation.allocation_order > 0,
            "payment allocation has an invalid order"
        );
        previous_allocation_order = allocation.allocation_order;
        grouped_allocations
            .entry(allocation.loan_payment_id)
            .or_default()
            .push(allocation);
    }

    rows.into_iter()
        .map(|row| {
            ensure!(
                row.id > 0 && row.payment_no > 0 && row.amount_krw > 0,
                "loan payment has invalid identity or amount"
            );
            let raw_allocations = grouped_allocations
                .remove(&row.id)
                .context("applied loan payment has no allocations")?;
            let allocations = aggregate_payment_allocations(&raw_allocations, row.amount_krw)?;
            Ok(LoanPaymentState {
                id: ResourceId::from_u64(row.id),
                payment_no: row.payment_no,
                kind: parse_payment_kind(&row.payment_kind)?,
                game_day: row.game_day,
                amount_krw: row.amount_krw,
                allocations,
            })
        })
        .collect::<Result<Vec<_>>>()
        .and_then(|payments| {
            ensure!(
                grouped_allocations.is_empty(),
                "payment allocation window has an unknown parent"
            );
            Ok(payments)
        })
}

fn aggregate_payment_allocations(
    rows: &[LoanPaymentAllocationHistoryRow],
    payment_amount_krw: i64,
) -> Result<Vec<LoanPaymentAllocationState>> {
    let mut amounts = BTreeMap::<LoanPaymentAllocationKindState, i64>::new();
    for row in rows {
        ensure!(
            row.amount_krw > 0,
            "loan payment allocation is not positive"
        );
        let kind = parse_payment_allocation_kind(&row.allocation_kind)?;
        let amount = amounts.entry(kind).or_default();
        *amount = amount
            .checked_add(row.amount_krw)
            .context("loan payment allocation total overflowed")?;
    }
    ensure!(
        !amounts.is_empty() && amounts.len() <= 8,
        "loan payment allocation summary exceeds its bound"
    );
    let allocations = amounts
        .into_iter()
        .map(|(kind, amount_krw)| LoanPaymentAllocationState { kind, amount_krw })
        .collect::<Vec<_>>();
    let allocation_total = allocations.iter().try_fold(0_i64, |total, allocation| {
        total
            .checked_add(allocation.amount_krw)
            .context("loan payment public allocation total overflowed")
    })?;
    ensure!(
        allocation_total == payment_amount_krw,
        "loan payment amount disagrees with its allocations"
    );
    Ok(allocations)
}

fn parse_payment_kind(value: &str) -> Result<LoanPaymentKindState> {
    match value {
        "scheduledInstallment" => Ok(LoanPaymentKindState::ScheduledInstallment),
        "manualPrepayment" => Ok(LoanPaymentKindState::ManualPrepayment),
        "leaseMovePayoff" => Ok(LoanPaymentKindState::LeaseMovePayoff),
        "propertySalePayoff" => Ok(LoanPaymentKindState::PropertySalePayoff),
        _ => bail!("unknown loan payment kind"),
    }
}

fn parse_payment_allocation_kind(value: &str) -> Result<LoanPaymentAllocationKindState> {
    match value {
        "overdueFee" => Ok(LoanPaymentAllocationKindState::OverdueFee),
        "overdueInterest" => Ok(LoanPaymentAllocationKindState::OverdueInterest),
        "overduePrincipal" => Ok(LoanPaymentAllocationKindState::OverduePrincipal),
        "currentFee" => Ok(LoanPaymentAllocationKindState::CurrentFee),
        "currentInterest" => Ok(LoanPaymentAllocationKindState::CurrentInterest),
        "currentPrincipal" => Ok(LoanPaymentAllocationKindState::CurrentPrincipal),
        "prepaymentFee" => Ok(LoanPaymentAllocationKindState::PrepaymentFee),
        "prepaymentPrincipal" => Ok(LoanPaymentAllocationKindState::PrepaymentPrincipal),
        _ => bail!("unknown loan payment allocation kind"),
    }
}

pub(super) async fn read_credit_and_loan_snapshot_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<CreditOverviewState> {
    let model_state: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT model.availability, state.credit_band
         FROM run_rule_bundle AS bundle
         INNER JOIN credit_model_version AS model
           ON model.id = bundle.credit_model_version_id
         LEFT JOIN credit_state AS state
           ON state.save_id = bundle.save_id
          AND state.run_revision = bundle.run_revision
          AND state.credit_model_version_id = model.id
         WHERE bundle.save_id = ? AND bundle.run_revision = ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await?;
    let (credit_band, mut credit_reasons) = match model_state {
        Some((availability, band)) if availability == "active" => {
            let band = band
                .as_deref()
                .map(parse_credit_band)
                .transpose()?
                .context("active credit model has no credit state")?;
            (Some(band), Vec::with_capacity(2))
        }
        Some(_) | None => (None, vec![CreditReasonState::ModelUnavailable]),
    };
    if credit_band.is_some() {
        let (default_count, delinquent_count): (i64, i64) = sqlx::query_as(
            "SELECT
                 CAST(COUNT(CASE WHEN status = 'defaulted' THEN 1 END) AS SIGNED),
                 CAST(COUNT(CASE WHEN status = 'delinquent' THEN 1 END) AS SIGNED)
             FROM loan_contract
             WHERE save_id = ? AND run_revision = ? AND read_only = FALSE
               AND status IN ('delinquent', 'defaulted')",
        )
        .bind(save_id)
        .bind(run_revision)
        .fetch_one(&mut **tx)
        .await?;
        if default_count > 0 {
            credit_reasons.push(CreditReasonState::ActiveDefault);
        }
        if delinquent_count > 0 {
            credit_reasons.push(CreditReasonState::ActiveDelinquency);
        }
        if credit_reasons.is_empty() {
            credit_reasons.push(CreditReasonState::CleanHistory);
        }
    }

    let rows: Vec<LoanSummaryRow> = sqlx::query_as(
        "SELECT contract.id, contract.loan_product_version_id AS product_version_id,
                contract.product_kind, product.display_name, contract.rate_status,
                contract.current_annual_rate_bp, contract.status,
                contract.remaining_principal_krw,
                CAST(COALESCE(overdue.overdue_krw, 0) AS SIGNED) AS overdue_krw,
                contract.read_only
         FROM loan_contract AS contract
         INNER JOIN loan_product_version AS product
           ON product.id = contract.loan_product_version_id
         LEFT JOIN (
             SELECT loan_contract_id,
                    SUM(original_amount_krw - paid_amount_krw) AS overdue_krw
             FROM loan_obligation_bucket
             WHERE save_id = ? AND run_revision = ? AND status = 'delinquent'
             GROUP BY loan_contract_id
         ) AS overdue ON overdue.loan_contract_id = contract.id
         WHERE contract.save_id = ? AND contract.run_revision = ?
           AND contract.status IN ('active', 'delinquent', 'defaulted', 'restructured')
         ORDER BY contract.id
         LIMIT 9",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() <= 8,
        "active loan summary exceeds its public bound"
    );
    let mut active_loans = rows
        .into_iter()
        .map(loan_summary_from_row)
        .collect::<Result<Vec<_>>>()?;
    let next_row: Option<NextInstallmentRow> = sqlx::query_as(
        "SELECT installment.loan_contract_id AS loan_id, installment.installment_no,
                installment.due_game_day,
                installment.scheduled_fee_krw - installment.paid_fee_krw AS fee_krw,
                installment.scheduled_interest_krw
                    - installment.paid_interest_krw AS interest_krw,
                installment.scheduled_principal_krw
                    - installment.paid_principal_krw AS principal_krw
         FROM loan_installment AS installment
         INNER JOIN loan_contract AS contract
           ON contract.id = installment.loan_contract_id
          AND contract.save_id = installment.save_id
          AND contract.run_revision = installment.run_revision
         WHERE installment.save_id = ? AND installment.run_revision = ?
           AND installment.status IN ('pending', 'due', 'partiallyPaid')
           AND contract.read_only = FALSE AND contract.status IN ('active', 'delinquent')
         ORDER BY installment.due_game_day, installment.loan_contract_id,
                  installment.installment_no
         LIMIT 1",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await?;
    let next_loan_id = next_row.as_ref().map(|row| row.loan_id);
    let next_loan_installment = next_row
        .map(|row| {
            ensure!(
                row.loan_id > 0,
                "next loan installment has an invalid contract id"
            );
            ensure!(
                row.fee_krw >= 0 && row.interest_krw >= 0 && row.principal_krw >= 0,
                "next loan installment has a negative balance"
            );
            let remaining_due_krw = row
                .fee_krw
                .checked_add(row.interest_krw)
                .and_then(|value| value.checked_add(row.principal_krw))
                .context("next loan installment total overflowed")?;
            Ok(NextLoanInstallmentState {
                loan_id: ResourceId::from_u64(row.loan_id),
                installment_no: row.installment_no,
                due_game_day: row.due_game_day,
                fee_krw: row.fee_krw,
                interest_krw: row.interest_krw,
                principal_krw: row.principal_krw,
                remaining_due_krw,
            })
        })
        .transpose()?;
    if let Some(next_loan_id) = next_loan_id
        && !active_loans
            .iter()
            .any(|loan| loan.id.get() == next_loan_id)
    {
        let next_summary = read_loan_summary_by_id(tx, save_id, run_revision, next_loan_id)
            .await?
            .context("next loan installment is outside the active-loan summary")?;
        if active_loans.len() == 8 {
            active_loans.pop();
        }
        active_loans.push(loan_summary_from_row(next_summary)?);
        active_loans.sort_by_key(|loan| loan.id.get());
    }
    let total_loan_balance_krw: i64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(
                    remaining_principal_krw + accrued_interest_krw + accrued_fee_krw
                ), 0) AS SIGNED)
         FROM loan_contract
         WHERE save_id = ? AND run_revision = ?
           AND status IN ('active', 'delinquent', 'defaulted', 'restructured')",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        total_loan_balance_krw >= 0,
        "loan snapshot total balance is negative"
    );
    Ok(CreditOverviewState {
        credit_band,
        credit_reasons,
        active_loans,
        next_loan_installment,
        total_loan_balance_krw,
    })
}

async fn read_loan_summary_by_id(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    contract_id: u64,
) -> Result<Option<LoanSummaryRow>> {
    Ok(sqlx::query_as(
        "SELECT contract.id, contract.loan_product_version_id AS product_version_id,
                contract.product_kind, product.display_name, contract.rate_status,
                contract.current_annual_rate_bp, contract.status,
                contract.remaining_principal_krw,
                CAST(COALESCE((
                    SELECT SUM(bucket.original_amount_krw - bucket.paid_amount_krw)
                    FROM loan_obligation_bucket AS bucket
                    WHERE bucket.save_id = contract.save_id
                      AND bucket.run_revision = contract.run_revision
                      AND bucket.loan_contract_id = contract.id
                      AND bucket.status = 'delinquent'
                ), 0) AS SIGNED) AS overdue_krw,
                contract.read_only
         FROM loan_contract AS contract
         INNER JOIN loan_product_version AS product
           ON product.id = contract.loan_product_version_id
         WHERE contract.id = ? AND contract.save_id = ? AND contract.run_revision = ?
           AND contract.status IN ('active', 'delinquent', 'defaulted', 'restructured')",
    )
    .bind(contract_id)
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await?)
}

fn loan_summary_from_row(row: LoanSummaryRow) -> Result<LoanSummaryState> {
    ensure!(
        row.id > 0 && row.product_version_id > 0,
        "loan snapshot has an invalid id"
    );
    ensure!(
        row.remaining_principal_krw >= 0 && row.overdue_krw >= 0,
        "loan snapshot has a negative balance"
    );
    Ok(LoanSummaryState {
        id: ResourceId::from_u64(row.id),
        product_version_id: ResourceId::from_u64(row.product_version_id),
        product_kind: parse_product_kind(&row.product_kind)?,
        display_name: row.display_name,
        rate_status: parse_rate_status(&row.rate_status)?,
        current_annual_rate_bp: row.current_annual_rate_bp.map(i64::from),
        status: parse_contract_status(&row.status)?,
        remaining_principal_krw: row.remaining_principal_krw,
        overdue_krw: row.overdue_krw,
        read_only: row.read_only,
    })
}

pub(super) async fn settle_due_loan_installment_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    settlement_id: u64,
    game_day: u32,
) -> Result<LoanInstallmentSettlement> {
    let initial_projection = validate_debt_projection_in_tx(tx, save_id, run_revision).await?;
    let envelope: SettlementEnvelopeRow = sqlx::query_as(
        "SELECT due_game_day, CAST(payload AS CHAR CHARACTER SET utf8mb4) AS payload_json,
                source_id
         FROM scheduled_settlement
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND kind = 'loanInstallment' AND source_kind = 'loanContract'
           AND status = 'pending'
         FOR UPDATE",
    )
    .bind(settlement_id)
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await
    .context("due loan settlement is missing")?;
    ensure!(
        envelope.due_game_day == game_day,
        "loan installment is not due on this game day"
    );
    let payload: LoanInstallmentSettlementPayload = serde_json::from_str(&envelope.payload_json)
        .context("loan settlement payload is invalid")?;
    ensure!(
        payload.version == LOAN_SETTLEMENT_PAYLOAD_VERSION,
        "loan settlement payload version is unsupported"
    );
    let contract_id = payload
        .loan_contract_id
        .parse::<u64>()
        .context("loan settlement contract id is invalid")?;
    ensure!(
        contract_id > 0 && envelope.source_id == payload.loan_contract_id,
        "loan settlement source identity changed"
    );

    let contract: LockedContractRow = sqlx::query_as(
        "SELECT contract.id, contract.household_id, save.policy_set_id,
                contract.status, contract.read_only, contract.remaining_principal_krw,
                contract.accrued_interest_krw, contract.accrued_fee_krw,
                CAST(contract.interest_remainder_numerator AS CHAR)
                    AS interest_remainder_numerator
         FROM loan_contract AS contract
         INNER JOIN save
           ON save.id = contract.save_id AND save.run_revision = contract.run_revision
         WHERE contract.id = ? AND contract.save_id = ? AND contract.run_revision = ?
         FOR UPDATE",
    )
    .bind(contract_id)
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await
    .context("loan settlement contract is missing")?;
    ensure_mutable_contract(&contract)?;
    ensure!(
        contract.household_id > 0,
        "loan contract has an invalid household"
    );
    ensure!(
        matches!(contract.status.as_str(), "active" | "delinquent"),
        "loan contract is not serviceable"
    );
    let installment: LockedInstallmentRow = sqlx::query_as(
        "SELECT id, installment_no, due_game_day, scheduled_fee_krw,
                scheduled_interest_krw, scheduled_principal_krw,
                CAST(interest_remainder_after AS CHAR) AS interest_remainder_after,
                paid_fee_krw, paid_interest_krw, paid_principal_krw, status
         FROM loan_installment
         WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
           AND installment_no = ?
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract_id)
    .bind(payload.installment_no)
    .fetch_one(&mut **tx)
    .await
    .context("due loan installment is missing")?;
    ensure!(
        installment.due_game_day == game_day && installment.status == "pending",
        "loan installment is not pending on its due day"
    );
    ensure!(
        installment.paid_fee_krw == 0
            && installment.paid_interest_krw == 0
            && installment.paid_principal_krw == 0,
        "pending loan installment already contains a payment"
    );
    materialize_installment_buckets(tx, save_id, run_revision, contract_id, &installment).await?;

    let mut buckets: Vec<StoredBucketRow> = sqlx::query_as(
        "SELECT id, loan_installment_id, bucket_kind, due_game_day,
                original_amount_krw, paid_amount_krw, status
         FROM loan_obligation_bucket
         WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
           AND status IN ('pending', 'delinquent') AND due_game_day <= ?
         ORDER BY due_game_day, id
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract_id)
    .bind(game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        buckets
            .iter()
            .any(|bucket| bucket.loan_installment_id == installment.id),
        "due loan installment has no obligation bucket"
    );
    buckets.sort_by_key(|bucket| {
        (
            repayment_bucket_kind(bucket, game_day)
                .map(RepaymentBucketKind::order)
                .unwrap_or(u8::MAX),
            bucket.due_game_day,
            bucket.id,
        )
    });
    let mut aggregated = BTreeMap::<RepaymentBucketKind, i64>::new();
    for bucket in &buckets {
        let remaining_krw = bucket
            .original_amount_krw
            .checked_sub(bucket.paid_amount_krw)
            .context("loan obligation bucket balance overflowed")?;
        ensure!(
            remaining_krw > 0,
            "open loan obligation bucket has no balance"
        );
        let kind = repayment_bucket_kind(bucket, game_day)?;
        let total = aggregated.entry(kind).or_default();
        *total = total
            .checked_add(remaining_krw)
            .context("loan repayment bucket total overflowed")?;
    }
    let repayment_buckets = aggregated
        .iter()
        .map(|(kind, due_krw)| RepaymentBucketBalance {
            kind: *kind,
            due_krw: *due_krw,
        })
        .collect::<Vec<_>>();
    let wallet_cash_krw: i64 = sqlx::query_scalar(
        "SELECT cash_krw FROM save WHERE id = ? AND run_revision = ? FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await?;
    let allocation = create_loan_rules()
        .allocate_repayment(RepaymentAllocationInput {
            wallet_cash_krw,
            buckets: &repayment_buckets,
        })
        .context("loan installment repayment allocation failed")?;
    let paid_krw = allocation
        .wallet_cash_before_krw
        .checked_sub(allocation.wallet_cash_after_krw)
        .context("loan payment amount overflowed")?;
    let total_due_krw = repayment_buckets.iter().try_fold(0_i64, |total, bucket| {
        total
            .checked_add(bucket.due_krw)
            .context("loan installment due total overflowed")
    })?;
    let unpaid_krw = total_due_krw
        .checked_sub(paid_krw)
        .context("loan installment unpaid amount overflowed")?;

    let payment_id = if paid_krw > 0 {
        Some(
            insert_scheduled_loan_payment(
                tx,
                save_id,
                run_revision,
                contract_id,
                paid_krw,
                game_day,
            )
            .await?,
        )
    } else {
        None
    };
    let paid_by_kind = allocation
        .buckets
        .iter()
        .map(|bucket| (bucket.kind, bucket.paid_krw))
        .collect::<BTreeMap<_, _>>();
    let mut remaining_by_kind = paid_by_kind;
    let mut allocation_order = 0_u16;
    let mut paid_principal_krw = 0_i64;
    let mut paid_interest_krw = 0_i64;
    let mut paid_fee_krw = 0_i64;
    for bucket in &buckets {
        let kind = repayment_bucket_kind(bucket, game_day)?;
        let available = remaining_by_kind
            .get_mut(&kind)
            .context("loan repayment allocation omitted a bucket kind")?;
        let bucket_remaining_krw = bucket
            .original_amount_krw
            .checked_sub(bucket.paid_amount_krw)
            .context("loan bucket balance overflowed")?;
        let bucket_paid_krw = (*available).min(bucket_remaining_krw);
        *available = available
            .checked_sub(bucket_paid_krw)
            .context("loan bucket allocation overflowed")?;
        if bucket_paid_krw > 0 {
            allocation_order = allocation_order
                .checked_add(1)
                .context("too many loan payment allocations")?;
            insert_loan_payment_allocation(
                tx,
                save_id,
                run_revision,
                contract_id,
                payment_id.context("positive loan allocation has no payment")?,
                bucket.id,
                allocation_order,
                kind,
                bucket_paid_krw,
            )
            .await?;
        }
        match bucket.bucket_kind.as_str() {
            "fee" => {
                paid_fee_krw = paid_fee_krw
                    .checked_add(bucket_paid_krw)
                    .context("paid loan fees overflowed")?;
            }
            "interest" => {
                paid_interest_krw = paid_interest_krw
                    .checked_add(bucket_paid_krw)
                    .context("paid loan interest overflowed")?;
            }
            "principal" => {
                paid_principal_krw = paid_principal_krw
                    .checked_add(bucket_paid_krw)
                    .context("paid loan principal overflowed")?;
            }
            _ => bail!("unknown loan obligation bucket kind"),
        }
        apply_bucket_payment(tx, bucket, bucket_paid_krw, game_day).await?;
    }
    ensure!(
        remaining_by_kind.values().all(|amount| *amount == 0),
        "loan repayment allocation was not exhausted"
    );
    refresh_affected_installments(tx, save_id, run_revision, contract_id, &buckets).await?;

    let ledger_transaction_id = match payment_id {
        Some(payment_id) => {
            let ledger = create_loan_payment_ledger(
                save_id,
                run_revision,
                contract.policy_set_id,
                contract_id,
                payment_id,
                game_day,
                paid_principal_krw,
                paid_interest_krw,
                paid_fee_krw,
            )?;
            let references = ledger
                .postings()
                .iter()
                .map(|posting| {
                    if matches!(
                        posting.account_code,
                        LedgerAccountCode::LoanPrincipalLiability
                            | LedgerAccountCode::LoanInterestExpense
                            | LedgerAccountCode::LoanFeeExpense
                    ) {
                        LoanPostingReference::Contract(contract_id)
                    } else {
                        LoanPostingReference::None
                    }
                })
                .collect::<Vec<_>>();
            let ledger_transaction_id =
                write_loan_ledger_transaction(tx, &ledger, &references).await?;
            let updated = sqlx::query(
                "UPDATE loan_payment SET status = 'applied', ledger_transaction_id = ?
                 WHERE id = ? AND save_id = ? AND run_revision = ?
                   AND status = 'prepared' AND ledger_transaction_id IS NULL",
            )
            .bind(ledger_transaction_id)
            .bind(payment_id)
            .bind(save_id)
            .bind(run_revision)
            .execute(&mut **tx)
            .await?;
            ensure!(
                updated.rows_affected() == 1,
                "loan payment changed during settlement"
            );
            Some(ledger_transaction_id)
        }
        None => None,
    };

    refresh_contract_after_installment(
        tx,
        save_id,
        run_revision,
        &contract,
        &installment,
        paid_principal_krw,
    )
    .await?;
    let projection = calculate_debt_projection_in_tx(tx, save_id, run_revision).await?;
    if allocation.wallet_cash_after_krw != wallet_cash_krw
        || projection.total_krw != initial_projection.total_krw
    {
        let save_updated = sqlx::query(
            "UPDATE save SET cash_krw = ?, debt_krw = ?
             WHERE id = ? AND run_revision = ? AND game_day + 1 = ?
               AND cash_krw = ? AND debt_krw = ?",
        )
        .bind(allocation.wallet_cash_after_krw)
        .bind(projection.total_krw)
        .bind(save_id)
        .bind(run_revision)
        .bind(game_day)
        .bind(wallet_cash_krw)
        .bind(initial_projection.total_krw)
        .execute(&mut **tx)
        .await?;
        ensure!(
            save_updated.rows_affected() == 1,
            "save changed during loan settlement"
        );
    }
    let settlement_updated = match ledger_transaction_id {
        Some(ledger_transaction_id) => {
            sqlx::query(
                "UPDATE scheduled_settlement
                 SET status = 'settled', outcome = 'applied',
                     settled_ledger_transaction_id = ?
                 WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'pending'",
            )
            .bind(ledger_transaction_id)
            .bind(settlement_id)
            .bind(save_id)
            .bind(run_revision)
            .execute(&mut **tx)
            .await?
        }
        None => {
            sqlx::query(
                "UPDATE scheduled_settlement
                 SET status = 'settled', outcome = 'noMovement',
                     outcome_reason = 'insufficientWalletCash'
                 WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'pending'",
            )
            .bind(settlement_id)
            .bind(save_id)
            .bind(run_revision)
            .execute(&mut **tx)
            .await?
        }
    };
    ensure!(
        settlement_updated.rows_affected() == 1,
        "loan settlement changed during execution"
    );
    validate_debt_projection_in_tx(tx, save_id, run_revision).await?;
    Ok(LoanInstallmentSettlement {
        contract_id,
        installment_no: installment.installment_no,
        paid_krw,
        unpaid_krw,
        wallet_cash_krw: allocation.wallet_cash_after_krw,
        debt_krw: projection.total_krw,
        ledger_transaction_id,
    })
}

fn ensure_mutable_contract(contract: &LockedContractRow) -> Result<()> {
    ensure!(
        !contract.read_only,
        "legacy bridge loan is read-only and cannot be mutated"
    );
    Ok(())
}

async fn materialize_installment_buckets(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    contract_id: u64,
    installment: &LockedInstallmentRow,
) -> Result<()> {
    for (kind, amount_krw) in [
        ("fee", installment.scheduled_fee_krw),
        ("interest", installment.scheduled_interest_krw),
        ("principal", installment.scheduled_principal_krw),
    ] {
        if amount_krw == 0 {
            continue;
        }
        ensure!(
            amount_krw > 0,
            "scheduled loan obligation cannot be negative"
        );
        sqlx::query(
            "INSERT INTO loan_obligation_bucket
                 (save_id, run_revision, loan_contract_id, loan_installment_id,
                  bucket_kind, due_game_day, original_amount_krw,
                  paid_amount_krw, status, delinquent_since_game_day)
             VALUES (?, ?, ?, ?, ?, ?, ?, 0, 'pending', NULL)",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(contract_id)
        .bind(installment.id)
        .bind(kind)
        .bind(installment.due_game_day)
        .bind(amount_krw)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

fn repayment_bucket_kind(
    bucket: &StoredBucketRow,
    current_due_game_day: u32,
) -> Result<RepaymentBucketKind> {
    let overdue = bucket.due_game_day < current_due_game_day || bucket.status == "delinquent";
    match (overdue, bucket.bucket_kind.as_str()) {
        (true, "fee") => Ok(RepaymentBucketKind::OverdueFee),
        (true, "interest") => Ok(RepaymentBucketKind::OverdueInterest),
        (true, "principal") => Ok(RepaymentBucketKind::OverduePrincipal),
        (false, "fee") => Ok(RepaymentBucketKind::CurrentFee),
        (false, "interest") => Ok(RepaymentBucketKind::CurrentInterest),
        (false, "principal") => Ok(RepaymentBucketKind::CurrentPrincipal),
        _ => bail!("unknown loan obligation bucket kind"),
    }
}

async fn insert_scheduled_loan_payment(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    contract_id: u64,
    amount_krw: i64,
    game_day: u32,
) -> Result<u64> {
    let payment_no_raw: u64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(MAX(payment_no), 0) + 1 AS UNSIGNED)
         FROM loan_payment WHERE loan_contract_id = ?",
    )
    .bind(contract_id)
    .fetch_one(&mut **tx)
    .await?;
    let payment_no = u32::try_from(payment_no_raw).context("loan payment count is out of range")?;
    let inserted = sqlx::query(
        "INSERT INTO loan_payment
             (save_id, run_revision, loan_contract_id, payment_no, payment_kind,
              amount_krw, game_day, command_id, status, ledger_transaction_id)
         VALUES (?, ?, ?, ?, 'scheduledInstallment', ?, ?, NULL, 'prepared', NULL)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract_id)
    .bind(payment_no)
    .bind(amount_krw)
    .bind(game_day)
    .execute(&mut **tx)
    .await?;
    Ok(inserted.last_insert_id())
}

#[allow(clippy::too_many_arguments)]
async fn insert_loan_payment_allocation(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    contract_id: u64,
    payment_id: u64,
    bucket_id: u64,
    allocation_order: u16,
    kind: RepaymentBucketKind,
    amount_krw: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO loan_payment_allocation
             (save_id, run_revision, loan_contract_id, loan_payment_id,
              loan_obligation_bucket_id, allocation_order, allocation_kind, amount_krw)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract_id)
    .bind(payment_id)
    .bind(bucket_id)
    .bind(allocation_order)
    .bind(repayment_bucket_db(kind))
    .bind(amount_krw)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn repayment_bucket_db(kind: RepaymentBucketKind) -> &'static str {
    match kind {
        RepaymentBucketKind::OverdueFee => "overdueFee",
        RepaymentBucketKind::OverdueInterest => "overdueInterest",
        RepaymentBucketKind::OverduePrincipal => "overduePrincipal",
        RepaymentBucketKind::CurrentFee => "currentFee",
        RepaymentBucketKind::CurrentInterest => "currentInterest",
        RepaymentBucketKind::CurrentPrincipal => "currentPrincipal",
    }
}

async fn apply_bucket_payment(
    tx: &mut Transaction<'_, MySql>,
    bucket: &StoredBucketRow,
    paid_krw: i64,
    game_day: u32,
) -> Result<()> {
    let next_paid_krw = bucket
        .paid_amount_krw
        .checked_add(paid_krw)
        .context("loan bucket paid amount overflowed")?;
    ensure!(
        next_paid_krw <= bucket.original_amount_krw,
        "loan bucket is overpaid"
    );
    let (status, delinquent_since_game_day) = if next_paid_krw == bucket.original_amount_krw {
        ("paid", None)
    } else {
        ("delinquent", Some(game_day.min(bucket.due_game_day)))
    };
    let updated = sqlx::query(
        "UPDATE loan_obligation_bucket
         SET paid_amount_krw = ?, status = ?, delinquent_since_game_day = ?
         WHERE id = ? AND paid_amount_krw = ? AND status IN ('pending', 'delinquent')",
    )
    .bind(next_paid_krw)
    .bind(status)
    .bind(delinquent_since_game_day)
    .bind(bucket.id)
    .bind(bucket.paid_amount_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "loan obligation bucket changed"
    );
    Ok(())
}

async fn refresh_affected_installments(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    contract_id: u64,
    buckets: &[StoredBucketRow],
) -> Result<()> {
    let installment_ids = buckets
        .iter()
        .map(|bucket| bucket.loan_installment_id)
        .collect::<BTreeSet<_>>();
    for installment_id in installment_ids {
        let row: (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT installment.scheduled_fee_krw,
                    installment.scheduled_interest_krw,
                    installment.scheduled_principal_krw,
                    CAST(COALESCE(SUM(CASE WHEN bucket.bucket_kind = 'fee'
                        THEN bucket.paid_amount_krw ELSE 0 END), 0) AS SIGNED),
                    CAST(COALESCE(SUM(CASE WHEN bucket.bucket_kind = 'interest'
                        THEN bucket.paid_amount_krw ELSE 0 END), 0) AS SIGNED),
                    CAST(COALESCE(SUM(CASE WHEN bucket.bucket_kind = 'principal'
                        THEN bucket.paid_amount_krw ELSE 0 END), 0) AS SIGNED)
             FROM loan_installment AS installment
             LEFT JOIN loan_obligation_bucket AS bucket
               ON bucket.loan_installment_id = installment.id
             WHERE installment.id = ? AND installment.save_id = ?
               AND installment.run_revision = ? AND installment.loan_contract_id = ?
             GROUP BY installment.id",
        )
        .bind(installment_id)
        .bind(save_id)
        .bind(run_revision)
        .bind(contract_id)
        .fetch_one(&mut **tx)
        .await?;
        let (fee_due, interest_due, principal_due, fee_paid, interest_paid, principal_paid) = row;
        let total_due = fee_due
            .checked_add(interest_due)
            .and_then(|value| value.checked_add(principal_due))
            .context("loan installment total overflowed")?;
        let total_paid = fee_paid
            .checked_add(interest_paid)
            .and_then(|value| value.checked_add(principal_paid))
            .context("loan installment paid total overflowed")?;
        let status = if total_paid == total_due {
            "paid"
        } else if total_paid > 0 {
            "partiallyPaid"
        } else {
            "due"
        };
        let updated = sqlx::query(
            "UPDATE loan_installment
             SET paid_fee_krw = ?, paid_interest_krw = ?, paid_principal_krw = ?, status = ?
             WHERE id = ? AND save_id = ? AND run_revision = ?
               AND loan_contract_id = ? AND status IN ('pending', 'due', 'partiallyPaid')",
        )
        .bind(fee_paid)
        .bind(interest_paid)
        .bind(principal_paid)
        .bind(status)
        .bind(installment_id)
        .bind(save_id)
        .bind(run_revision)
        .bind(contract_id)
        .execute(&mut **tx)
        .await?;
        ensure!(updated.rows_affected() == 1, "loan installment changed");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn create_loan_payment_ledger(
    save_id: u64,
    run_revision: u32,
    policy_set_id: u64,
    contract_id: u64,
    payment_id: u64,
    game_day: u32,
    principal_krw: i64,
    interest_krw: i64,
    fee_krw: i64,
) -> Result<LedgerTransaction> {
    let total_krw = principal_krw
        .checked_add(interest_krw)
        .and_then(|value| value.checked_add(fee_krw))
        .context("loan ledger payment total overflowed")?;
    ensure!(total_krw > 0, "loan payment ledger cannot be empty");
    let mut postings = vec![LedgerPosting {
        account_code: LedgerAccountCode::Wallet,
        financial_account_id: None,
        amount_krw: total_krw
            .checked_neg()
            .context("loan payment cannot be negated")?,
    }];
    if principal_krw > 0 {
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::LoanPrincipalLiability,
            financial_account_id: None,
            amount_krw: principal_krw,
        });
    }
    if interest_krw > 0 {
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::LoanInterestExpense,
            financial_account_id: None,
            amount_krw: interest_krw,
        });
    }
    if fee_krw > 0 {
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::LoanFeeExpense,
            financial_account_id: None,
            amount_krw: fee_krw,
        });
    }
    let _ = contract_id;
    Ok(
        create_finance_rules().create_ledger_transaction(LedgerTransactionDraft {
            policy: RunPolicyContext {
                run: RunId {
                    save_id: ResourceId::from_u64(save_id),
                    run_revision,
                },
                policy_set_id: ResourceId::from_u64(policy_set_id),
            },
            source: LedgerSource {
                kind: LedgerSourceKind::LoanInstallment,
                source_id: payment_id.to_string(),
            },
            game_day,
            description: "대출 정기 상환".to_owned(),
            postings,
        })?,
    )
}

async fn refresh_contract_after_installment(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    contract: &LockedContractRow,
    installment: &LockedInstallmentRow,
    paid_principal_krw: i64,
) -> Result<()> {
    let remaining_principal_krw = contract
        .remaining_principal_krw
        .checked_sub(paid_principal_krw)
        .context("loan principal balance overflowed")?;
    ensure!(
        remaining_principal_krw >= 0,
        "loan principal balance became negative"
    );
    let (accrued_fee_krw, accrued_interest_krw, oldest_unpaid_due_game_day): (
        i64,
        i64,
        Option<u32>,
    ) = sqlx::query_as(
        "SELECT
             CAST(COALESCE(SUM(CASE WHEN bucket.bucket_kind = 'fee'
                 THEN bucket.original_amount_krw - bucket.paid_amount_krw ELSE 0 END), 0)
                 AS SIGNED),
             CAST(COALESCE(SUM(CASE WHEN bucket.bucket_kind = 'interest'
                 THEN bucket.original_amount_krw - bucket.paid_amount_krw ELSE 0 END), 0)
                 AS SIGNED),
             MIN(CASE WHEN bucket.status IN ('pending', 'delinquent')
                 THEN bucket.due_game_day ELSE NULL END)
         FROM loan_obligation_bucket AS bucket
         WHERE bucket.save_id = ? AND bucket.run_revision = ?
           AND bucket.loan_contract_id = ?
           AND bucket.status IN ('pending', 'delinquent')",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract.id)
    .fetch_one(&mut **tx)
    .await?;
    let next_installment_no: Option<u16> = sqlx::query_scalar(
        "SELECT MIN(installment_no) FROM loan_installment
         WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
           AND status IN ('pending', 'due', 'partiallyPaid')",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract.id)
    .fetch_one(&mut **tx)
    .await?;
    let paid_off = remaining_principal_krw == 0
        && accrued_fee_krw == 0
        && accrued_interest_krw == 0
        && next_installment_no.is_none();
    let status = if paid_off {
        "paidOff"
    } else {
        contract.status.as_str()
    };
    let next_installment_no = if paid_off {
        None
    } else {
        Some(next_installment_no.unwrap_or(installment.installment_no))
    };
    let updated = sqlx::query(
        "UPDATE loan_contract
         SET status = ?, remaining_principal_krw = ?, accrued_interest_krw = ?,
             accrued_fee_krw = ?, interest_remainder_numerator = ?,
             next_installment_no = ?, oldest_unpaid_due_game_day = ?
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND read_only = FALSE AND status = ? AND remaining_principal_krw = ?
           AND accrued_interest_krw = ? AND accrued_fee_krw = ?
           AND interest_remainder_numerator = ?",
    )
    .bind(status)
    .bind(remaining_principal_krw)
    .bind(accrued_interest_krw)
    .bind(accrued_fee_krw)
    .bind(&installment.interest_remainder_after)
    .bind(next_installment_no)
    .bind(oldest_unpaid_due_game_day)
    .bind(contract.id)
    .bind(save_id)
    .bind(run_revision)
    .bind(&contract.status)
    .bind(contract.remaining_principal_krw)
    .bind(contract.accrued_interest_krw)
    .bind(contract.accrued_fee_krw)
    .bind(&contract.interest_remainder_numerator)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "loan contract changed during settlement"
    );
    if paid_off {
        release_property_lien_after_payoff(tx, save_id, run_revision, contract.id).await?;
    }
    Ok(())
}

pub(super) async fn reset_variable_loan_rates_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
) -> Result<u32> {
    let (market_date, treasury_3m_bp): (Date, Option<i16>) = sqlx::query_as(
        "SELECT daily.market_date, daily.treasury_3m_bp
         FROM save
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         INNER JOIN market_daily AS daily
           ON daily.world_id = world.id AND daily.game_day = ?
         WHERE save.id = ? AND save.run_revision = ? AND save.game_day + 1 = ?",
    )
    .bind(game_day)
    .bind(save_id)
    .bind(run_revision)
    .bind(game_day)
    .fetch_one(&mut **tx)
    .await?;
    if game_day == 0 || market_date.day() != 1 {
        return Ok(0);
    }
    validate_debt_projection_in_tx(tx, save_id, run_revision).await?;
    let observed_reference_rate_bp = treasury_3m_bp
        .map(i64::from)
        .context("monthly loan reference rate is unavailable")?;
    let contracts: Vec<VariableContractRow> = sqlx::query_as(
        "SELECT id, reference_rate_key, applied_spread_bp,
                minimum_annual_rate_bp, maximum_annual_rate_bp,
                current_annual_rate_bp, day_count_denominator, repayment_method,
                remaining_principal_krw,
                CAST(interest_remainder_numerator AS CHAR)
                    AS interest_remainder_numerator
         FROM loan_contract
         WHERE save_id = ? AND run_revision = ? AND read_only = FALSE
           AND status IN ('active', 'delinquent')
           AND rate_type = 'variable' AND rate_reset_rule = 'monthlyDay1'
         ORDER BY id
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    let loan_rules = create_loan_rules();
    let mut reset_count = 0_u32;
    for contract in contracts {
        ensure!(
            contract.reference_rate_key == "treasury3m",
            "variable loan has an unsupported reference rate"
        );
        ensure!(
            contract.day_count_denominator == ACTUAL_365_DAY_COUNT,
            "variable loan has an unsupported day-count rule"
        );
        let installments: Vec<PendingInstallmentRow> = sqlx::query_as(
            "SELECT id, installment_no, due_game_day,
                    interest_period_start_game_day, interest_period_end_game_day,
                    elapsed_days, annual_rate_bp, opening_principal_krw,
                    scheduled_fee_krw, scheduled_interest_krw, scheduled_principal_krw,
                    CAST(interest_remainder_before AS CHAR) AS interest_remainder_before,
                    CAST(interest_remainder_after AS CHAR) AS interest_remainder_after,
                    schedule_revision
             FROM loan_installment
             WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
               AND status = 'pending' AND interest_period_start_game_day >= ?
             ORDER BY installment_no
             FOR UPDATE",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(contract.id)
        .bind(game_day)
        .fetch_all(&mut **tx)
        .await?;
        if installments.is_empty() {
            continue;
        }
        ensure!(
            installments[0].interest_period_start_game_day == game_day,
            "monthly loan reset does not start on the observation day"
        );
        ensure!(
            installments
                .windows(2)
                .all(|pair| { pair[1].installment_no == pair[0].installment_no.saturating_add(1) }),
            "variable loan pending installments are not contiguous"
        );
        ensure!(
            installments.iter().all(|installment| {
                installment.interest_period_end_game_day == installment.due_game_day
                    && u32::from(installment.elapsed_days)
                        == installment
                            .interest_period_end_game_day
                            .saturating_sub(installment.interest_period_start_game_day)
                            .saturating_add(1)
            }),
            "variable loan schedule periods are invalid"
        );
        let overdue_principal_krw: i64 = sqlx::query_scalar(
            "SELECT CAST(COALESCE(SUM(original_amount_krw - paid_amount_krw), 0)
                    AS SIGNED)
             FROM loan_obligation_bucket
             WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
               AND bucket_kind = 'principal' AND status = 'delinquent'",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(contract.id)
        .fetch_one(&mut **tx)
        .await?;
        let scheduled_principal_krw = contract
            .remaining_principal_krw
            .checked_sub(overdue_principal_krw)
            .context("variable loan scheduled principal overflowed")?;
        ensure!(
            scheduled_principal_krw > 0,
            "variable loan has no principal for its pending schedule"
        );
        let spread_bp = i64::from(contract.applied_spread_bp);
        let unclamped_rate_bp = observed_reference_rate_bp
            .checked_add(spread_bp)
            .context("variable loan reset rate overflowed")?;
        let applied_rate_bp = unclamped_rate_bp.clamp(
            i64::from(contract.minimum_annual_rate_bp),
            i64::from(contract.maximum_annual_rate_bp),
        );
        let periods = installments
            .iter()
            .map(|installment| LoanSchedulePeriod {
                due_game_day: installment.due_game_day,
                elapsed_days: installment.elapsed_days,
            })
            .collect::<Vec<_>>();
        let prior_remainder = contract
            .interest_remainder_numerator
            .parse::<i128>()
            .context("variable loan interest remainder is invalid")?;
        let repayment_method = parse_repayment_method(&contract.repayment_method)?;
        let schedule = loan_rules
            .build_schedule(LoanScheduleInput {
                principal_krw: scheduled_principal_krw,
                initial_annual_rate_bp: applied_rate_bp,
                day_count: contract.day_count_denominator,
                repayment_method,
                prior_interest_remainder_numerator: prior_remainder,
                periods: &periods,
                rate_resets: &[],
            })
            .context("variable loan reset schedule is invalid")?;
        ensure!(
            schedule.installments.len() == installments.len(),
            "variable loan reset changed installment count"
        );
        let prior_level_payment_krw = if repayment_method == LoanRepaymentMethod::LevelPayment {
            Some(
                installments[0]
                    .scheduled_interest_krw
                    .checked_add(installments[0].scheduled_principal_krw)
                    .context("prior level payment overflowed")?,
            )
        } else {
            None
        };
        let recalculated_level_payment_krw =
            if repayment_method == LoanRepaymentMethod::LevelPayment {
                Some(schedule.installments[0].payment_krw)
            } else {
                None
            };
        let reset_no_raw: u64 = sqlx::query_scalar(
            "SELECT CAST(COALESCE(MAX(reset_no), 0) + 1 AS UNSIGNED)
             FROM loan_rate_reset WHERE loan_contract_id = ?",
        )
        .bind(contract.id)
        .fetch_one(&mut **tx)
        .await?;
        let reset_no = u16::try_from(reset_no_raw).context("loan reset count is out of range")?;
        sqlx::query(
            "INSERT INTO loan_rate_reset
                 (save_id, run_revision, loan_contract_id, reset_no,
                  observation_game_day, effective_from_game_day, reference_rate_key,
                  observed_reference_rate_bp, applied_spread_bp,
                  unclamped_annual_rate_bp, applied_annual_rate_bp,
                  prior_level_payment_krw, recalculated_level_payment_krw)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(contract.id)
        .bind(reset_no)
        .bind(game_day)
        .bind(game_day)
        .bind(&contract.reference_rate_key)
        .bind(i16::try_from(observed_reference_rate_bp).context("observed loan rate is invalid")?)
        .bind(contract.applied_spread_bp)
        .bind(i32::try_from(unclamped_rate_bp).context("unclamped loan rate is invalid")?)
        .bind(u16::try_from(applied_rate_bp).context("applied loan rate is invalid")?)
        .bind(prior_level_payment_krw)
        .bind(recalculated_level_payment_krw)
        .execute(&mut **tx)
        .await?;

        let mut remainder_before = prior_remainder;
        for (stored, calculated) in installments.iter().zip(&schedule.installments) {
            ensure!(
                calculated.due_game_day == stored.due_game_day
                    && calculated.elapsed_days == stored.elapsed_days,
                "variable loan reset changed its calendar"
            );
            let updated = sqlx::query(
                "UPDATE loan_installment
                 SET annual_rate_bp = ?, opening_principal_krw = ?,
                     scheduled_interest_krw = ?, scheduled_principal_krw = ?,
                     interest_remainder_before = ?, interest_remainder_after = ?,
                     schedule_revision = schedule_revision + 1
                 WHERE id = ? AND save_id = ? AND run_revision = ?
                   AND loan_contract_id = ? AND status = 'pending'
                   AND schedule_revision = ? AND annual_rate_bp = ?
                   AND opening_principal_krw = ? AND scheduled_interest_krw = ?
                   AND scheduled_principal_krw = ?
                   AND interest_remainder_before = ? AND interest_remainder_after = ?",
            )
            .bind(u16::try_from(calculated.annual_rate_bp).context("reset rate is invalid")?)
            .bind(calculated.opening_principal_krw)
            .bind(calculated.interest_krw)
            .bind(calculated.principal_krw)
            .bind(remainder_before.to_string())
            .bind(calculated.carried_interest_remainder_numerator.to_string())
            .bind(stored.id)
            .bind(save_id)
            .bind(run_revision)
            .bind(contract.id)
            .bind(stored.schedule_revision)
            .bind(stored.annual_rate_bp)
            .bind(stored.opening_principal_krw)
            .bind(stored.scheduled_interest_krw)
            .bind(stored.scheduled_principal_krw)
            .bind(&stored.interest_remainder_before)
            .bind(&stored.interest_remainder_after)
            .execute(&mut **tx)
            .await?;
            ensure!(
                updated.rows_affected() == 1,
                "pending loan schedule changed during rate reset"
            );
            remainder_before = calculated.carried_interest_remainder_numerator;
        }
        let applied_rate_bp =
            u16::try_from(applied_rate_bp).context("applied loan rate is invalid")?;
        if applied_rate_bp != contract.current_annual_rate_bp {
            let contract_updated = sqlx::query(
                "UPDATE loan_contract SET current_annual_rate_bp = ?
                 WHERE id = ? AND save_id = ? AND run_revision = ?
                   AND read_only = FALSE AND status IN ('active', 'delinquent')
                   AND current_annual_rate_bp = ?",
            )
            .bind(applied_rate_bp)
            .bind(contract.id)
            .bind(save_id)
            .bind(run_revision)
            .bind(contract.current_annual_rate_bp)
            .execute(&mut **tx)
            .await?;
            ensure!(
                contract_updated.rows_affected() == 1,
                "loan contract changed during rate reset"
            );
        }
        reset_count = reset_count
            .checked_add(1)
            .context("loan reset count overflowed")?;
    }
    validate_debt_projection_in_tx(tx, save_id, run_revision).await?;
    Ok(reset_count)
}

pub(super) async fn apply_credit_end_of_day_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
) -> Result<CreditDayApplication> {
    validate_debt_projection_in_tx(tx, save_id, run_revision).await?;
    let state: CreditStateRow = sqlx::query_as(
        "SELECT state.household_id, state.credit_model_version_id,
                state.credit_units, state.credit_band, save.game_day AS save_game_day,
                state.last_evaluated_game_day,
                state.evaluation_revision,
                CAST(model.parameters AS CHAR CHARACTER SET utf8mb4)
                    AS model_parameters_json
         FROM credit_state AS state
         INNER JOIN credit_model_version AS model
           ON model.id = state.credit_model_version_id
          AND model.availability = 'active' AND model.sealed_at IS NOT NULL
         INNER JOIN save
           ON save.id = state.save_id AND save.run_revision = state.run_revision
         WHERE state.save_id = ? AND state.run_revision = ?
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await
    .context("run credit state is missing")?;
    ensure!(
        game_day
            == state
                .save_game_day
                .checked_add(1)
                .context("save game day overflowed during credit evaluation")?
            && game_day
                == state
                    .last_evaluated_game_day
                    .checked_add(1)
                    .context("credit evaluation day overflowed")?,
        "credit state must advance exactly one game day"
    );
    let model = parse_credit_model(&state.model_parameters_json)?;
    let credit_rules = create_credit_rules();
    let stored_band = credit_rules
        .band(model, i64::from(state.credit_units))
        .context("stored credit state has invalid units")?;
    ensure!(
        credit_band_db(stored_band) == state.credit_band,
        "stored credit band disagrees with its units"
    );
    let contracts: Vec<CreditContractRow> = sqlx::query_as(
        "SELECT id, status FROM loan_contract
         WHERE save_id = ? AND run_revision = ? AND read_only = FALSE
           AND status IN ('active', 'delinquent', 'defaulted')
         ORDER BY id
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    let mut events = Vec::new();
    let mut final_statuses = Vec::with_capacity(contracts.len());
    for contract in &contracts {
        let current_status = parse_contract_status(&contract.status)?;
        if current_status == LoanContractStatus::Defaulted {
            final_statuses.push((contract.id, current_status));
            continue;
        }
        let delinquent_rows: Vec<(u64, u32, i64)> = sqlx::query_as(
            "SELECT id, delinquent_since_game_day,
                    original_amount_krw - paid_amount_krw
             FROM loan_obligation_bucket
             WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
               AND status = 'delinquent'
             ORDER BY due_game_day, id",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(contract.id)
        .fetch_all(&mut **tx)
        .await?;
        let buckets = delinquent_rows
            .iter()
            .map(|(bucket_id, delinquent_since_game_day, outstanding_krw)| {
                let days_past_due = game_day
                    .checked_sub(*delinquent_since_game_day)
                    .context("loan delinquency begins after the evaluation day")?;
                Ok(CreditDelinquencyBucket {
                    bucket_id: *bucket_id,
                    days_past_due,
                    outstanding_krw: *outstanding_krw,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let default_assessment = credit_rules
            .assess_default(CreditDefaultAssessmentInput {
                model,
                buckets: &buckets,
            })
            .context("loan default assessment failed")?;
        let final_status = credit_rules
            .resolve_end_of_day_status(LoanEndOfDayStatusInput {
                current: current_status,
                has_unpaid_buckets: !buckets.is_empty(),
                default_triggered: default_assessment.should_default,
            })
            .context("loan end-of-day status resolution failed")?;
        if current_status == LoanContractStatus::Active
            && final_status == LoanContractStatus::Delinquent
        {
            events.push(CreditDayEvent {
                contract_id: contract.id,
                kind: CreditEventKind::EnteredDelinquency,
            });
        } else if current_status == LoanContractStatus::Delinquent
            && final_status == LoanContractStatus::Defaulted
        {
            events.push(CreditDayEvent {
                contract_id: contract.id,
                kind: CreditEventKind::EnteredDefault,
            });
        }
        apply_contract_end_of_day_status(
            tx,
            save_id,
            run_revision,
            contract.id,
            current_status,
            final_status,
        )
        .await?;
        final_statuses.push((contract.id, final_status));
    }
    let adverse_contract_count = u32::try_from(
        final_statuses
            .iter()
            .filter(|(_, status)| {
                matches!(
                    status,
                    LoanContractStatus::Delinquent | LoanContractStatus::Defaulted
                )
            })
            .count(),
    )
    .context("adverse loan count is out of range")?;
    let calculation = credit_rules
        .calculate_day(CreditDayInput {
            model,
            current_units: i64::from(state.credit_units),
            events: &events,
            adverse_contract_count,
        })
        .context("daily credit calculation failed")?;
    append_credit_history(
        tx,
        save_id,
        run_revision,
        game_day,
        &state,
        model,
        &calculation,
    )
    .await?;
    let evaluation_revision = state
        .evaluation_revision
        .checked_add(1)
        .context("credit evaluation revision overflowed")?;
    let updated = sqlx::query(
        "UPDATE credit_state
         SET credit_units = ?, credit_band = ?, last_evaluated_game_day = ?,
             evaluation_revision = ?
         WHERE save_id = ? AND run_revision = ? AND household_id = ?
           AND credit_model_version_id = ? AND credit_units = ? AND credit_band = ?
           AND last_evaluated_game_day = ? AND evaluation_revision = ?",
    )
    .bind(u16::try_from(calculation.units_after).context("credit units are invalid")?)
    .bind(credit_band_db(calculation.band))
    .bind(game_day)
    .bind(evaluation_revision)
    .bind(save_id)
    .bind(run_revision)
    .bind(state.household_id)
    .bind(state.credit_model_version_id)
    .bind(state.credit_units)
    .bind(&state.credit_band)
    .bind(state.last_evaluated_game_day)
    .bind(state.evaluation_revision)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "credit state changed during evaluation"
    );
    validate_debt_projection_in_tx(tx, save_id, run_revision).await?;
    Ok(CreditDayApplication {
        units_before: calculation.units_before,
        units_after: calculation.units_after,
        band: calculation.band,
        transitioned_contracts: u32::try_from(events.len())
            .context("credit event count is out of range")?,
    })
}

async fn apply_contract_end_of_day_status(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    contract_id: u64,
    current_status: LoanContractStatus,
    final_status: LoanContractStatus,
) -> Result<()> {
    if current_status == final_status {
        return Ok(());
    }
    if final_status == LoanContractStatus::Defaulted {
        sqlx::query(
            "UPDATE loan_installment
             SET status = 'cancelled'
             WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
               AND status = 'pending'",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(contract_id)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "UPDATE scheduled_settlement
             SET status = 'cancelled', cancellation_reason = 'loanDefaulted'
             WHERE save_id = ? AND run_revision = ?
               AND source_kind = 'loanContract' AND source_id = ?
               AND kind = 'loanInstallment' AND status = 'pending'",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(contract_id.to_string())
        .execute(&mut **tx)
        .await?;
    }
    let updated = sqlx::query(
        "UPDATE loan_contract SET status = ?, next_installment_no = ?
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND read_only = FALSE AND status = ?",
    )
    .bind(contract_status_db(final_status))
    .bind(if final_status == LoanContractStatus::Defaulted {
        None
    } else {
        read_next_installment_number(tx, save_id, run_revision, contract_id).await?
    })
    .bind(contract_id)
    .bind(save_id)
    .bind(run_revision)
    .bind(contract_status_db(current_status))
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "loan status changed during credit evaluation"
    );
    Ok(())
}

async fn read_next_installment_number(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    contract_id: u64,
) -> Result<Option<u16>> {
    Ok(sqlx::query_scalar(
        "SELECT MIN(installment_no) FROM loan_installment
         WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
           AND status IN ('pending', 'due', 'partiallyPaid')",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract_id)
    .fetch_one(&mut **tx)
    .await?)
}

async fn append_credit_history(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
    state: &CreditStateRow,
    model: CreditModelTerms,
    calculation: &crate::life::CreditDayCalculation,
) -> Result<()> {
    let credit_rules = create_credit_rules();
    let mut event_order_raw: u64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(MAX(event_order), 0) AS UNSIGNED)
         FROM credit_history WHERE household_id = ? AND game_day = ?",
    )
    .bind(state.household_id)
    .bind(game_day)
    .fetch_one(&mut **tx)
    .await?;
    let mut raw_units = calculation.units_before;
    for event in &calculation.event_applications {
        let next_raw_units = raw_units
            .checked_add(event.delta_units)
            .context("credit event raw units overflowed")?;
        event_order_raw = event_order_raw
            .checked_add(1)
            .context("credit history order overflowed")?;
        insert_credit_history_row(
            tx,
            CreditHistoryWrite {
                save_id,
                run_revision,
                household_id: state.household_id,
                credit_model_version_id: state.credit_model_version_id,
                loan_contract_id: Some(event.contract_id),
                game_day,
                event_order: u16::try_from(event_order_raw)
                    .context("credit history order is out of range")?,
                event_kind: match event.kind {
                    CreditEventKind::EnteredDelinquency => "activeToDelinquent",
                    CreditEventKind::EnteredDefault => "delinquentToDefaulted",
                    CreditEventKind::EnteredLegalProcedure => "legalProcedure",
                },
                reason_code: match event.kind {
                    CreditEventKind::EnteredDelinquency => "paymentShortfall",
                    CreditEventKind::EnteredDefault => "defaultThreshold",
                    CreditEventKind::EnteredLegalProcedure => "legalProcedure",
                },
                delta_units: event.delta_units,
                raw_before_units: raw_units,
                raw_after_units: next_raw_units,
                model,
            },
            credit_rules.as_ref(),
        )
        .await?;
        raw_units = next_raw_units;
    }
    let next_raw_units = raw_units
        .checked_add(calculation.daily_delta_units)
        .context("daily credit raw units overflowed")?;
    event_order_raw = event_order_raw
        .checked_add(1)
        .context("credit history order overflowed")?;
    insert_credit_history_row(
        tx,
        CreditHistoryWrite {
            save_id,
            run_revision,
            household_id: state.household_id,
            credit_model_version_id: state.credit_model_version_id,
            loan_contract_id: None,
            game_day,
            event_order: u16::try_from(event_order_raw)
                .context("credit history order is out of range")?,
            event_kind: if calculation.daily_delta_units < 0 {
                "dailyPenalty"
            } else {
                "cleanRecovery"
            },
            reason_code: if calculation.daily_delta_units < 0 {
                "adverseContractActive"
            } else {
                "noAdverseContract"
            },
            delta_units: calculation.daily_delta_units,
            raw_before_units: raw_units,
            raw_after_units: next_raw_units,
            model,
        },
        credit_rules.as_ref(),
    )
    .await?;
    raw_units = next_raw_units;
    if raw_units != calculation.units_after {
        let clamp_delta = calculation
            .units_after
            .checked_sub(raw_units)
            .context("credit clamp delta overflowed")?;
        event_order_raw = event_order_raw
            .checked_add(1)
            .context("credit history order overflowed")?;
        insert_credit_history_row(
            tx,
            CreditHistoryWrite {
                save_id,
                run_revision,
                household_id: state.household_id,
                credit_model_version_id: state.credit_model_version_id,
                loan_contract_id: None,
                game_day,
                event_order: u16::try_from(event_order_raw)
                    .context("credit history order is out of range")?,
                event_kind: "clamp",
                reason_code: "modelBoundary",
                delta_units: clamp_delta,
                raw_before_units: raw_units,
                raw_after_units: calculation.units_after,
                model,
            },
            credit_rules.as_ref(),
        )
        .await?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct CreditHistoryWrite<'a> {
    save_id: u64,
    run_revision: u32,
    household_id: u64,
    credit_model_version_id: u64,
    loan_contract_id: Option<u64>,
    game_day: u32,
    event_order: u16,
    event_kind: &'a str,
    reason_code: &'a str,
    delta_units: i64,
    raw_before_units: i64,
    raw_after_units: i64,
    model: CreditModelTerms,
}

async fn insert_credit_history_row(
    tx: &mut Transaction<'_, MySql>,
    write: CreditHistoryWrite<'_>,
    credit_rules: &dyn crate::life::CreditRules,
) -> Result<()> {
    let bounded_before = write
        .raw_before_units
        .clamp(write.model.minimum_units, write.model.maximum_units);
    let bounded_after = write
        .raw_after_units
        .clamp(write.model.minimum_units, write.model.maximum_units);
    let before_band = credit_rules
        .band(write.model, bounded_before)
        .context("credit history before band is invalid")?;
    let after_band = credit_rules
        .band(write.model, bounded_after)
        .context("credit history after band is invalid")?;
    sqlx::query(
        "INSERT INTO credit_history
             (save_id, run_revision, household_id, credit_model_version_id,
              loan_contract_id, game_day, event_order, event_kind, reason_code,
              delta_units, unclamped_before_units, unclamped_after_units,
              before_units, after_units, before_band, after_band)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(write.save_id)
    .bind(write.run_revision)
    .bind(write.household_id)
    .bind(write.credit_model_version_id)
    .bind(write.loan_contract_id)
    .bind(write.game_day)
    .bind(write.event_order)
    .bind(write.event_kind)
    .bind(write.reason_code)
    .bind(i16::try_from(write.delta_units).context("credit history delta is out of range")?)
    .bind(i32::try_from(write.raw_before_units).context("raw credit units are out of range")?)
    .bind(i32::try_from(write.raw_after_units).context("raw credit units are out of range")?)
    .bind(u16::try_from(bounded_before).context("bounded credit units are invalid")?)
    .bind(u16::try_from(bounded_after).context("bounded credit units are invalid")?)
    .bind(credit_band_db(before_band))
    .bind(credit_band_db(after_band))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn parse_contract_status(value: &str) -> Result<LoanContractStatus> {
    match value {
        "pending" => Ok(LoanContractStatus::Pending),
        "active" => Ok(LoanContractStatus::Active),
        "delinquent" => Ok(LoanContractStatus::Delinquent),
        "defaulted" => Ok(LoanContractStatus::Defaulted),
        "paidOff" => Ok(LoanContractStatus::PaidOff),
        "restructured" => Ok(LoanContractStatus::Restructured),
        "discharged" => Ok(LoanContractStatus::Discharged),
        "chargedOff" => Ok(LoanContractStatus::ChargedOff),
        "cancelled" => Ok(LoanContractStatus::Cancelled),
        _ => bail!("unknown loan contract status"),
    }
}

fn contract_status_db(status: LoanContractStatus) -> &'static str {
    match status {
        LoanContractStatus::Pending => "pending",
        LoanContractStatus::Active => "active",
        LoanContractStatus::Delinquent => "delinquent",
        LoanContractStatus::Defaulted => "defaulted",
        LoanContractStatus::PaidOff => "paidOff",
        LoanContractStatus::Restructured => "restructured",
        LoanContractStatus::Discharged => "discharged",
        LoanContractStatus::ChargedOff => "chargedOff",
        LoanContractStatus::Cancelled => "cancelled",
    }
}

pub(super) async fn write_loan_ledger_transaction(
    tx: &mut Transaction<'_, MySql>,
    ledger: &LedgerTransaction,
    references: &[LoanPostingReference],
) -> Result<u64> {
    ensure!(
        references.len() == ledger.postings().len(),
        "loan ledger posting references are incomplete"
    );
    if references
        .iter()
        .all(|reference| *reference == LoanPostingReference::None)
    {
        return write_ledger_transaction(tx, ledger).await;
    }

    let policy = ledger.policy();
    let inserted = sqlx::query(
        "INSERT INTO ledger_transaction
             (save_id, run_revision, game_day, policy_set_id,
              source_kind, source_id, description)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(policy.run.save_id.get())
    .bind(policy.run.run_revision)
    .bind(ledger.game_day())
    .bind(policy.policy_set_id.get())
    .bind(to_db_str(&ledger.source().kind)?)
    .bind(&ledger.source().source_id)
    .bind(ledger.description())
    .execute(&mut **tx)
    .await?;
    let ledger_transaction_id = inserted.last_insert_id();
    for (index, (posting, reference)) in ledger.postings().iter().zip(references).enumerate() {
        let posting_order = u16::try_from(index + 1).context("too many loan ledger postings")?;
        let loan_contract_id = match reference {
            LoanPostingReference::None => None,
            LoanPostingReference::Contract(contract_id) => Some(*contract_id),
        };
        sqlx::query(
            "INSERT INTO ledger_posting
                 (save_id, run_revision, ledger_transaction_id, posting_order,
                  account_code, financial_account_id, loan_contract_id,
                  tax_obligation_id, amount_krw)
             VALUES (?, ?, ?, ?, ?, ?, ?, NULL, ?)",
        )
        .bind(policy.run.save_id.get())
        .bind(policy.run.run_revision)
        .bind(ledger_transaction_id)
        .bind(posting_order)
        .bind(to_db_str(&posting.account_code)?)
        .bind(posting.financial_account_id.map(ResourceId::get))
        .bind(loan_contract_id)
        .bind(posting.amount_krw)
        .execute(&mut **tx)
        .await?;
    }
    Ok(ledger_transaction_id)
}

fn to_db_str<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value)? {
        Value::String(text) => Ok(text),
        other => bail!("value is not storable as a string: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn given_credit_model_parameters(
        schema_version: u8,
        loan_eligibility: Option<Value>,
    ) -> String {
        serde_json::json!({
            "schemaVersion": schema_version,
            "creditUnits": {"minimum": 0, "maximum": 1000, "initial": 700},
            "bands": [
                {"band": "prime", "minimumUnits": 850, "maximumUnits": 1000},
                {"band": "standard", "minimumUnits": 650, "maximumUnits": 849},
                {"band": "limited", "minimumUnits": 450, "maximumUnits": 649},
                {"band": "distressed", "minimumUnits": 1, "maximumUnits": 449},
                {"band": "insolvent", "minimumUnits": 0, "maximumUnits": 0}
            ],
            "eventPenalty": {
                "activeToDelinquentUnits": -80,
                "delinquentToDefaultedUnits": -300,
                "legalProcedureUnits": 0
            },
            "dailyChange": {
                "delinquentOrDefaultedPenaltyUnits": -5,
                "cleanRecoveryUnits": 1
            },
            "defaultRule": {
                "absoluteOldestBucketDays": 90,
                "amountAndAgeMinimumKrw": 1_000_000,
                "amountAndAgeOldestBucketDays": 30
            },
            "loanEligibility": loan_eligibility,
            "provenance": "GAME_BALANCE"
        })
        .to_string()
    }

    fn given_unsecured_eligibility() -> Value {
        serde_json::json!({
            "unsecuredLoan": {
                "allowedCreditBands": ["prime", "standard"],
                "disallowedContractStatuses": ["delinquent", "defaulted", "restructured"],
                "maximumActiveContracts": 8
            }
        })
    }

    fn given_mortgage_dsr_policy() -> MortgageDsrPolicyRow {
        MortgageDsrPolicyRow {
            borrower_dsr_balance_threshold_krw: 100_000_000,
            bank_dsr_limit_ppm: 400_000,
            evaluation_horizon_months: 12,
            full_term_fixed_stress_rate_bp: 0,
        }
    }

    mod context_신용모델의_기본규칙을_읽는_경우 {
        use super::*;

        #[test]
        fn given_전세대출이추가된_v4_when_기본규칙을읽으면_then_기존신용구간을유지한다() {
            let parameters = given_credit_model_parameters(4, Some(given_unsecured_eligibility()));

            let result = parse_credit_model(&parameters)
                .expect("v4 모델의 기본 신용 규칙을 읽을 수 있어야 한다");

            assert_eq!(result.starting_units, 700);
            assert_eq!(result.band_thresholds.prime_min_units, 850);
        }
    }

    mod context_quote_eligibility_is_parsed {
        use super::*;

        #[test]
        fn given_the_sealed_v2_model_when_parsed_then_quote_eligibility_is_absent() {
            let parameters = given_credit_model_parameters(2, None);

            let result =
                parse_quote_eligibility(&parameters).expect("v2 model은 읽을 수 있어야 한다");

            assert!(result.is_none());
        }

        #[test]
        fn given_the_v3_model_when_parsed_then_prime_and_standard_are_allowed() {
            let parameters = given_credit_model_parameters(3, Some(given_unsecured_eligibility()));

            let result = parse_quote_eligibility(&parameters)
                .expect("v3 model은 읽을 수 있어야 한다")
                .expect("v3 model에는 심사 자격이 있어야 한다");

            assert_eq!(
                result.allowed_credit_bands,
                vec![CreditBand::Prime, CreditBand::Standard]
            );
            assert_eq!(result.maximum_active_contracts, 8);
        }

        #[test]
        fn given_duplicate_allowed_bands_when_parsed_then_the_model_is_rejected() {
            let parameters = given_credit_model_parameters(
                3,
                Some(serde_json::json!({
                    "unsecuredLoan": {
                        "allowedCreditBands": ["prime", "prime"],
                        "disallowedContractStatuses": [
                            "delinquent",
                            "defaulted",
                            "restructured"
                        ],
                        "maximumActiveContracts": 8
                    }
                })),
            );

            let result = parse_quote_eligibility(&parameters);

            assert!(result.is_err());
        }
    }

    mod context_dsr_horizon_is_calculated {
        use super::*;

        #[test]
        fn given_february_29_when_advanced_twelve_months_then_february_28_is_used() {
            let world_start = Date::from_calendar_date(2028, Month::January, 1)
                .expect("월드 시작일은 유효해야 한다");
            let evaluation_date =
                Date::from_calendar_date(2028, Month::February, 29).expect("윤일은 유효해야 한다");
            let expected_end = Date::from_calendar_date(2029, Month::February, 28)
                .expect("다음 해 말일은 유효해야 한다");
            let game_day = game_day_for_date(world_start, evaluation_date)
                .expect("평가일을 game day로 바꿀 수 있어야 한다");

            let result = twelve_month_horizon_game_day(world_start, game_day)
                .expect("12개월 평가 종료일을 계산할 수 있어야 한다");

            assert_eq!(
                result,
                game_day_for_date(world_start, expected_end)
                    .expect("예상 종료일을 game day로 바꿀 수 있어야 한다")
            );
        }
    }

    mod context_신용제한이_주담대_dsr보다_우선하는_경우 {
        use super::*;

        #[test]
        fn given_활성부도와_gate초과잔액_when_dsr증거를_만들면_then_schedule없이_gate만_표시한다() {
            let credit_reasons = [MortgageQuoteReasonState::ActiveDefault];
            let post_execution_balance_krw = 100_000_001;

            let result = credit_restricted_mortgage_dsr_evidence(
                &credit_reasons,
                post_execution_balance_krw,
                given_mortgage_dsr_policy(),
            )
            .expect("신용 제한이면 DSR 이전 증거를 만들어야 한다");

            assert_eq!(result, (true, None, 0));
        }

        #[test]
        fn given_활성연체와_gate이하잔액_when_dsr증거를_만들면_then_schedule없이_미적용으로_표시한다()
         {
            let credit_reasons = [MortgageQuoteReasonState::ActiveDelinquency];
            let post_execution_balance_krw = 100_000_000;

            let result = credit_restricted_mortgage_dsr_evidence(
                &credit_reasons,
                post_execution_balance_krw,
                given_mortgage_dsr_policy(),
            )
            .expect("신용 제한이면 DSR 이전 증거를 만들어야 한다");

            assert_eq!(result, (false, None, 0));
        }
    }

    mod context_중도상환_ledger_posting을_만드는_경우 {
        use super::*;

        #[test]
        fn given_수수료0_when_posting을만들면_then_wallet과원금만기록한다() {
            let result = create_prepayment_postings(100, 0, 100)
                .expect("수수료 없는 중도상환 posting을 만들어야 한다");

            assert_eq!(
                result
                    .iter()
                    .map(|posting| (posting.account_code, posting.amount_krw))
                    .collect::<Vec<_>>(),
                vec![
                    (LedgerAccountCode::Wallet, -100),
                    (LedgerAccountCode::LoanPrincipalLiability, 100),
                ]
            );
        }

        #[test]
        fn given_수수료가있을때_when_posting을만들면_then_fee_expense를마지막에기록한다() {
            let result = create_prepayment_postings(100, 5, 105)
                .expect("수수료 있는 중도상환 posting을 만들어야 한다");

            assert_eq!(
                result
                    .iter()
                    .map(|posting| (posting.account_code, posting.amount_krw))
                    .collect::<Vec<_>>(),
                vec![
                    (LedgerAccountCode::Wallet, -105),
                    (LedgerAccountCode::LoanPrincipalLiability, 100),
                    (LedgerAccountCode::LoanFeeExpense, 5),
                ]
            );
        }
    }

    mod context_대출_상세를_공개하는_경우 {
        use super::*;

        fn given_종료된_조기상환불가_계약() -> LoanDetailRow {
            LoanDetailRow {
                id: 41,
                product_version_id: 7,
                product_kind: "unsecuredLoan".to_owned(),
                display_name: "고정 계약".to_owned(),
                rate_status: "available".to_owned(),
                current_annual_rate_bp: Some(450),
                status: "paidOff".to_owned(),
                read_only: false,
                original_principal_krw: 1_000_000,
                remaining_principal_krw: 0,
                accrued_interest_krw: 0,
                accrued_fee_krw: 0,
                overdue_krw: 0,
                repayment_method: "levelPayment".to_owned(),
                term_months: Some(12),
                total_installments: Some(12),
                activated_game_day: 10,
                maturity_game_day: Some(375),
                final_installment_due_game_day: None,
                next_installment_no: None,
                oldest_unpaid_due_game_day: None,
                product_prepayment_allowed: false,
                prepayment_fee_ppm: None,
                prepayment_effect: "forbidden".to_owned(),
                dsr_included: true,
                lease_contract_id: None,
                property_holding_id: None,
            }
        }

        #[test]
        fn given_모든회차가취소된_paid_off_when_상세를만들면_then_final과_조기상환조건은_null이다()
        {
            let row = given_종료된_조기상환불가_계약();

            let result = loan_detail_from_row(row).expect("종료 계약 상세를 만들 수 있어야 한다");

            assert_eq!(result.final_installment_due_game_day, None);
            assert!(!result.prepayment_allowed);
            assert_eq!(result.prepayment_fee_ppm, None);
            assert_eq!(result.prepayment_effect, None);
        }

        #[test]
        fn given_read_only_legacy_when_상세를만들면_then_schedule과_rate는_null이다() {
            let mut row = given_종료된_조기상환불가_계약();
            row.product_kind = "legacyDebt".to_owned();
            row.rate_status = "rateUnavailable".to_owned();
            row.current_annual_rate_bp = None;
            row.status = "active".to_owned();
            row.read_only = true;
            row.remaining_principal_krw = 1_000_000;
            row.term_months = None;
            row.total_installments = None;
            row.maturity_game_day = None;

            let result = loan_detail_from_row(row).expect("legacy 계약 상세를 만들 수 있어야 한다");

            assert_eq!(result.current_annual_rate_bp, None);
            assert_eq!(result.term_months, None);
            assert_eq!(result.final_installment_due_game_day, None);
            assert!(!result.prepayment_allowed);
        }
    }

    mod context_대출_회차를_공개하는_경우 {
        use super::*;

        fn given_회차(status: &str, paid_principal_krw: i64) -> LoanInstallmentHistoryRow {
            LoanInstallmentHistoryRow {
                id: 101,
                installment_no: 3,
                due_game_day: 30,
                interest_period_start_game_day: 1,
                elapsed_days: 30,
                annual_rate_bp: 450,
                opening_principal_krw: 1_000,
                scheduled_fee_krw: 5,
                scheduled_interest_krw: 10,
                scheduled_principal_krw: 85,
                paid_fee_krw: 0,
                paid_interest_krw: 0,
                paid_principal_krw,
                status: status.to_owned(),
                schedule_revision: 1,
            }
        }

        #[test]
        fn given_cancelled회차_when_공개값을만들면_then_remaining_due는0이다() {
            let row = given_회차("cancelled", 0);

            let result =
                loan_installment_from_row(row).expect("취소 회차 공개값을 만들 수 있어야 한다");

            assert_eq!(result.remaining_due_krw, 0);
            assert_eq!(result.status, LoanInstallmentStatusState::Cancelled);
        }

        #[test]
        fn given_pending회차에_paid금액_when_공개값을만들면_then_거절한다() {
            let row = given_회차("pending", 10);

            let result = loan_installment_from_row(row);

            assert!(result.is_err());
        }

        #[test]
        fn given_limit보다한건많은_desc회차_when_window를만들면_then_has_more와_limit을지킨다() {
            let mut newest = given_회차("pending", 0);
            newest.installment_no = 3;
            let mut middle = given_회차("pending", 0);
            middle.id = 102;
            middle.installment_no = 2;
            let mut oldest = given_회차("pending", 0);
            oldest.id = 103;
            oldest.installment_no = 1;

            let (result, has_more) = build_installment_window(vec![newest, middle, oldest], 2)
                .expect("회차 window를 만들 수 있어야 한다");

            assert!(has_more);
            assert_eq!(
                result
                    .iter()
                    .map(|installment| installment.installment_no)
                    .collect::<Vec<_>>(),
                vec![3, 2]
            );
        }
    }

    mod context_납부_allocation을_공개하는_경우 {
        use super::*;

        fn given_allocation(
            order: u16,
            kind: &str,
            amount_krw: i64,
        ) -> LoanPaymentAllocationHistoryRow {
            LoanPaymentAllocationHistoryRow {
                loan_payment_id: 51,
                payment_no: 4,
                allocation_order: order,
                allocation_kind: kind.to_owned(),
                amount_krw,
            }
        }

        #[test]
        fn given_같은kind의여러bucket_when_집계하면_then_합치고_canonical순서로공개한다() {
            let rows = vec![
                given_allocation(1, "currentPrincipal", 30),
                given_allocation(2, "overdueInterest", 20),
                given_allocation(3, "currentPrincipal", 50),
            ];

            let result = aggregate_payment_allocations(&rows, 100)
                .expect("allocation을 집계할 수 있어야 한다");

            assert_eq!(
                result,
                vec![
                    LoanPaymentAllocationState {
                        kind: LoanPaymentAllocationKindState::OverdueInterest,
                        amount_krw: 20,
                    },
                    LoanPaymentAllocationState {
                        kind: LoanPaymentAllocationKindState::CurrentPrincipal,
                        amount_krw: 80,
                    },
                ]
            );
        }

        #[test]
        fn given_payment금액과allocation합이다를때_when_집계하면_then_거절한다() {
            let rows = vec![given_allocation(1, "currentPrincipal", 99)];

            let result = aggregate_payment_allocations(&rows, 100);

            assert!(result.is_err());
        }
    }
}
