use std::error::Error;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use time::Date;

use crate::finance::{LedgerAccountCode, ResourceId};

pub const LIVING_COST_FACTOR_SCALE_PPM: i64 = 1_000_000;
pub const LIVING_COST_PRORATION_SCALE: i64 = 377_580;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LivingCostCategory {
    Housing,
    Food,
    Transport,
    Communication,
    Utilities,
    Healthcare,
    Education,
    DependentCare,
    Discretionary,
}

impl LivingCostCategory {
    pub const ALL: [Self; 9] = [
        Self::Housing,
        Self::Food,
        Self::Transport,
        Self::Communication,
        Self::Utilities,
        Self::Healthcare,
        Self::Education,
        Self::DependentCare,
        Self::Discretionary,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Housing => "housing",
            Self::Food => "food",
            Self::Transport => "transport",
            Self::Communication => "communication",
            Self::Utilities => "utilities",
            Self::Healthcare => "healthcare",
            Self::Education => "education",
            Self::DependentCare => "dependentCare",
            Self::Discretionary => "discretionary",
        }
    }

    pub const fn order(self) -> u8 {
        match self {
            Self::Housing => 0,
            Self::Food => 1,
            Self::Transport => 2,
            Self::Communication => 3,
            Self::Utilities => 4,
            Self::Healthcare => 5,
            Self::Education => 6,
            Self::DependentCare => 7,
            Self::Discretionary => 8,
        }
    }
}

impl FromStr for LivingCostCategory {
    type Err = LivingCostError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "housing" => Ok(Self::Housing),
            "food" => Ok(Self::Food),
            "transport" => Ok(Self::Transport),
            "communication" => Ok(Self::Communication),
            "utilities" => Ok(Self::Utilities),
            "healthcare" => Ok(Self::Healthcare),
            "education" => Ok(Self::Education),
            "dependentCare" => Ok(Self::DependentCare),
            "discretionary" => Ok(Self::Discretionary),
            _ => Err(LivingCostError::UnknownCategory),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct YearMonth {
    pub year: i32,
    pub month: u8,
}

impl YearMonth {
    pub const fn is_valid(self) -> bool {
        self.year >= 1 && self.year <= 9_999 && self.month >= 1 && self.month <= 12
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LivingCostProration {
    pub remaining_calendar_days: u8,
    pub days_in_month: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LivingCostCategoryCalculationInput {
    pub category: LivingCostCategory,
    pub essential: bool,
    pub base_monthly_krw: i64,
    pub base_cpi_index: i64,
    pub current_cpi_index: i64,
    pub region_factor_ppm: i64,
    pub household_factor_ppm: i64,
    pub budget_factor_ppm: i64,
    pub prior_remainder_numerator: i128,
    pub proration: Option<LivingCostProration>,
}

#[derive(Debug, Clone, Copy)]
pub struct LivingCostMonthCalculationInput<'a> {
    pub categories: &'a [LivingCostCategoryCalculationInput],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LivingCostCategoryCalculation {
    pub category: LivingCostCategory,
    pub essential: bool,
    pub gross_krw: i64,
    pub remainder_numerator: i128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LivingCostMonthCalculation {
    pub categories: Vec<LivingCostCategoryCalculation>,
    pub total_gross_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CurrentLivingCostCharge {
    pub category: LivingCostCategory,
    pub essential: bool,
    pub gross_krw: i64,
}

impl From<LivingCostCategoryCalculation> for CurrentLivingCostCharge {
    fn from(value: LivingCostCategoryCalculation) -> Self {
        Self {
            category: value.category,
            essential: value.essential,
            gross_krw: value.gross_krw,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EssentialArrearBalance {
    pub arrear_id: u64,
    pub due_year_month: YearMonth,
    pub category: LivingCostCategory,
    pub remaining_krw: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct LivingCostAllocationInput<'a> {
    pub due_year_month: YearMonth,
    pub wallet_cash_krw: i64,
    pub current_charges: &'a [CurrentLivingCostCharge],
    pub existing_arrears: &'a [EssentialArrearBalance],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CurrentLivingCostAllocation {
    pub category: LivingCostCategory,
    pub essential: bool,
    pub gross_krw: i64,
    pub paid_krw: i64,
    pub unpaid_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EssentialArrearPayment {
    pub arrear_id: u64,
    pub due_year_month: YearMonth,
    pub category: LivingCostCategory,
    pub balance_before_krw: i64,
    pub paid_krw: i64,
    pub balance_after_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EssentialArrearDraft {
    pub due_year_month: YearMonth,
    pub category: LivingCostCategory,
    pub amount_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LivingCostAllocation {
    pub wallet_cash_before_krw: i64,
    pub wallet_cash_after_krw: i64,
    pub current_allocations: Vec<CurrentLivingCostAllocation>,
    pub existing_arrear_payments: Vec<EssentialArrearPayment>,
    pub created_arrears: Vec<EssentialArrearDraft>,
}

pub trait LivingCostRules: Send + Sync + 'static {
    fn parse_category(&self, value: &str) -> Result<LivingCostCategory, LivingCostError>;

    fn calculate_category(
        &self,
        input: LivingCostCategoryCalculationInput,
    ) -> Result<LivingCostCategoryCalculation, LivingCostError>;

    fn calculate_month(
        &self,
        input: LivingCostMonthCalculationInput<'_>,
    ) -> Result<LivingCostMonthCalculation, LivingCostError>;

    fn allocate_month(
        &self,
        input: LivingCostAllocationInput<'_>,
    ) -> Result<LivingCostAllocation, LivingCostError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LivingCostError {
    UnknownCategory,
    DuplicateCategory(LivingCostCategory),
    MissingCategory(LivingCostCategory),
    InvalidBaseMonthlyAmount(LivingCostCategory),
    InvalidBaseCpiIndex(LivingCostCategory),
    InvalidCurrentCpiIndex(LivingCostCategory),
    InvalidRegionFactor(LivingCostCategory),
    InvalidHouseholdFactor(LivingCostCategory),
    InvalidBudgetFactor(LivingCostCategory),
    RequiredCategoryHasZeroBudget(LivingCostCategory),
    InvalidRemainder(LivingCostCategory),
    InvalidProration(LivingCostCategory),
    InvalidYearMonth,
    InvalidWalletCash,
    InvalidCurrentCharge(LivingCostCategory),
    InvalidArrear,
    DuplicateArrearId(u64),
    ArithmeticOverflow,
}

impl Display for LivingCostError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "living cost error: {self:?}")
    }
}

impl Error for LivingCostError {}

pub const LOAN_RATE_SCALE_BP: i64 = 10_000;
pub const LOAN_RATIO_SCALE_PPM: i64 = 1_000_000;
pub const BULLET_DSR_PRINCIPAL_YEARS: i64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoanProductKind {
    StudentLoan,
    UnsecuredLoan,
    LeaseDepositLoan,
    Mortgage,
    LegacyDebt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoanRateStatus {
    Available,
    RateUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoanRepaymentMethod {
    EqualPrincipal,
    LevelPayment,
    Bullet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoanRateType {
    Fixed,
    Variable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoanLenderSector {
    Bank,
    NonBank,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoanRateReference {
    Treasury3m,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoanRateResetRule {
    None,
    MonthlyDay1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoanDayCountRule {
    Actual365,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoanPaymentCalendar {
    MonthEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoanPrepaymentEffect {
    ReduceTerm,
    RecalculatePayment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoanProductProvenance {
    GameBalance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoanInterestInput {
    pub principal_krw: i64,
    pub annual_rate_bp: i64,
    pub elapsed_days: u16,
    pub day_count: u16,
    pub prior_remainder_numerator: i128,
    pub discard_remainder: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoanInterestCalculation {
    pub interest_krw: i64,
    pub carried_remainder_numerator: i128,
    pub discarded_remainder_numerator: i128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoanSchedulePeriod {
    pub due_game_day: u32,
    pub elapsed_days: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
/// A reset whose rate starts with the interval after the named installment.
pub struct LoanRateReset {
    pub after_installment_sequence: u16,
    pub next_annual_rate_bp: i64,
}

#[derive(Debug, Clone, Copy)]
/// Schedule terms after product rate floors and caps have been applied.
pub struct LoanScheduleInput<'a> {
    pub principal_krw: i64,
    pub initial_annual_rate_bp: i64,
    pub day_count: u16,
    pub repayment_method: LoanRepaymentMethod,
    pub prior_interest_remainder_numerator: i128,
    pub periods: &'a [LoanSchedulePeriod],
    pub rate_resets: &'a [LoanRateReset],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoanInstallmentCalculation {
    pub sequence: u16,
    pub due_game_day: u32,
    pub elapsed_days: u16,
    pub annual_rate_bp: i64,
    pub opening_principal_krw: i64,
    pub payment_krw: i64,
    pub principal_krw: i64,
    pub interest_krw: i64,
    pub remaining_principal_krw: i64,
    pub carried_interest_remainder_numerator: i128,
    pub discarded_interest_remainder_numerator: i128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoanScheduleCalculation {
    pub installments: Vec<LoanInstallmentCalculation>,
    pub total_principal_krw: i64,
    pub total_interest_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoanPrepaymentInput {
    pub remaining_principal_krw: i64,
    pub principal_krw: i64,
    pub fee_ppm: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoanPrepaymentCalculation {
    pub principal_krw: i64,
    pub fee_krw: i64,
    pub total_debited_krw: i64,
    pub remaining_principal_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoanPrepaymentSchedulePeriod {
    pub installment_no: u16,
    pub due_game_day: u32,
    pub elapsed_days: u16,
    pub scheduled_principal_cap_krw: i64,
}

#[derive(Debug, Clone, Copy)]
/// Pending schedule terms after a principal prepayment has been applied.
pub struct LoanPrepaymentScheduleInput<'a> {
    pub principal_before_prepayment_krw: i64,
    pub principal_after_prepayment_krw: i64,
    pub annual_rate_bp: i64,
    pub day_count: u16,
    pub repayment_method: LoanRepaymentMethod,
    pub prepayment_effect: LoanPrepaymentEffect,
    pub prior_interest_remainder_numerator: i128,
    pub periods: &'a [LoanPrepaymentSchedulePeriod],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoanPrepaymentScheduleCalculation {
    pub installments: Vec<LoanInstallmentCalculation>,
    pub cancelled_installment_numbers: Vec<u16>,
    pub total_principal_krw: i64,
    pub total_interest_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RepaymentBucketKind {
    OverdueFee,
    OverdueInterest,
    OverduePrincipal,
    CurrentFee,
    CurrentInterest,
    CurrentPrincipal,
}

impl RepaymentBucketKind {
    pub const ALL: [Self; 6] = [
        Self::OverdueFee,
        Self::OverdueInterest,
        Self::OverduePrincipal,
        Self::CurrentFee,
        Self::CurrentInterest,
        Self::CurrentPrincipal,
    ];

    pub const fn order(self) -> u8 {
        match self {
            Self::OverdueFee => 0,
            Self::OverdueInterest => 1,
            Self::OverduePrincipal => 2,
            Self::CurrentFee => 3,
            Self::CurrentInterest => 4,
            Self::CurrentPrincipal => 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepaymentBucketBalance {
    pub kind: RepaymentBucketKind,
    pub due_krw: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct RepaymentAllocationInput<'a> {
    pub wallet_cash_krw: i64,
    pub buckets: &'a [RepaymentBucketBalance],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepaymentBucketAllocation {
    pub kind: RepaymentBucketKind,
    pub due_krw: i64,
    pub paid_krw: i64,
    pub unpaid_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepaymentAllocation {
    pub wallet_cash_before_krw: i64,
    pub wallet_cash_after_krw: i64,
    pub buckets: Vec<RepaymentBucketAllocation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DsrPaymentTreatment {
    Scheduled,
    BulletCreditFiveYear,
}

#[derive(Debug, Clone, Copy)]
pub struct DsrLoanInput<'a> {
    pub loan_id: u64,
    pub included_in_dsr: bool,
    pub counts_toward_general_loan_balance: bool,
    pub counts_toward_credit_stress_balance: bool,
    pub rate_type: LoanRateType,
    pub fixed_rate_period_months: u16,
    pub payment_treatment: DsrPaymentTreatment,
    pub schedule: LoanScheduleInput<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DsrPolicy {
    pub general_loan_balance_gate_krw: i64,
    pub maximum_ratio_ppm: i64,
    pub credit_balance_stress_gate_krw: i64,
    pub base_stress_rate_bp: i64,
    pub medium_fixed_stress_multiplier_ppm: i64,
}

#[derive(Debug, Clone, Copy)]
/// DSR inputs whose end day is exactly twelve calendar months after the evaluation day.
pub struct DsrAssessmentInput<'a> {
    pub evaluation_game_day: u32,
    pub evaluation_end_game_day: u32,
    pub verified_annual_income_krw: Option<i64>,
    pub policy: DsrPolicy,
    pub loans: &'a [DsrLoanInput<'a>],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DsrLoanContribution {
    pub loan_id: u64,
    pub stress_rate_bp: i64,
    pub debt_service_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DsrAssessment {
    pub gate_applied: bool,
    pub stress_gate_applied: bool,
    pub general_loan_balance_krw: i64,
    pub credit_loan_balance_krw: i64,
    pub numerator_krw: i64,
    pub denominator_krw: Option<i64>,
    pub ratio_ppm: Option<i64>,
    pub maximum_ratio_ppm: i64,
    pub passed: bool,
    pub loan_contributions: Vec<DsrLoanContribution>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
/// LTV inputs with collateral value supplied by the M4-C valuation provider.
pub struct LtvAssessmentInput {
    pub existing_senior_balance_krw: i64,
    pub new_principal_krw: i64,
    pub included_fees_krw: i64,
    pub recognized_collateral_value_krw: Option<i64>,
    pub maximum_ratio_ppm: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LtvAssessment {
    pub numerator_krw: i64,
    pub denominator_krw: i64,
    pub ratio_ppm: i64,
    pub maximum_ratio_ppm: i64,
    pub passed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LeaseDepositFundingLimitInput {
    pub deposit_krw: i64,
    pub funding_limit_ppm: i64,
    pub product_maximum_principal_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LeaseDepositFundingLimit {
    pub deposit_based_limit_krw: i64,
    pub maximum_funding_krw: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct LeaseDepositAffordabilityNewLoanInput<'a> {
    pub loan_id: u64,
    pub schedule: LoanScheduleInput<'a>,
}

#[derive(Debug, Clone, Copy)]
/// Affordability is always applied and excludes the loan replaced by the same lease move.
pub struct LeaseDepositAffordabilityInput<'a> {
    pub evaluation_game_day: u32,
    pub evaluation_end_game_day: u32,
    pub verified_annual_income_krw: Option<i64>,
    pub maximum_ratio_ppm: i64,
    pub stress_policy: DsrPolicy,
    pub existing_loans: &'a [DsrLoanInput<'a>],
    pub new_loan: LeaseDepositAffordabilityNewLoanInput<'a>,
    pub replaced_loan_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LeaseDepositAffordabilityAssessment {
    pub numerator_krw: i64,
    pub denominator_krw: i64,
    pub ratio_ppm: i64,
    pub maximum_ratio_ppm: i64,
    pub passed: bool,
    pub new_loan_interest_krw: i64,
    pub existing_loan_contributions: Vec<DsrLoanContribution>,
    pub replaced_loan_id: Option<u64>,
}

pub trait LoanRules: Send + Sync + 'static {
    fn calculate_interest(
        &self,
        input: LoanInterestInput,
    ) -> Result<LoanInterestCalculation, LoanRuleError>;

    fn build_schedule(
        &self,
        input: LoanScheduleInput<'_>,
    ) -> Result<LoanScheduleCalculation, LoanRuleError>;

    fn calculate_prepayment(
        &self,
        input: LoanPrepaymentInput,
    ) -> Result<LoanPrepaymentCalculation, LoanRuleError>;

    fn rebuild_prepayment_schedule(
        &self,
        input: LoanPrepaymentScheduleInput<'_>,
    ) -> Result<LoanPrepaymentScheduleCalculation, LoanRuleError>;

    fn allocate_repayment(
        &self,
        input: RepaymentAllocationInput<'_>,
    ) -> Result<RepaymentAllocation, LoanRuleError>;

    fn assess_dsr(&self, input: DsrAssessmentInput<'_>) -> Result<DsrAssessment, LoanRuleError>;

    fn assess_ltv(&self, input: LtvAssessmentInput) -> Result<LtvAssessment, LoanRuleError>;

    fn calculate_lease_deposit_funding_limit(
        &self,
        input: LeaseDepositFundingLimitInput,
    ) -> Result<LeaseDepositFundingLimit, LoanRuleError>;

    fn assess_lease_deposit_affordability(
        &self,
        input: LeaseDepositAffordabilityInput<'_>,
    ) -> Result<LeaseDepositAffordabilityAssessment, LoanRuleError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoanRuleError {
    InvalidPrincipal,
    InvalidAnnualRate,
    InvalidDayCount,
    InvalidElapsedDays,
    InvalidInterestRemainder,
    EmptySchedule,
    InvalidSchedulePeriod,
    InvalidPrepayment,
    InvalidPrepaymentSchedule,
    DuplicateRateReset(u16),
    InvalidRateReset(u16),
    ScheduleDoesNotAmortize,
    InvalidWalletCash,
    InvalidRepaymentBucket(RepaymentBucketKind),
    DuplicateRepaymentBucket(RepaymentBucketKind),
    InvalidDsrPolicy,
    InvalidDsrLoan(u64),
    DuplicateLoanId(u64),
    InvalidEvaluationPeriod,
    IncomeUnavailable,
    ValuationUnavailable,
    InvalidLtvInput,
    InvalidLeaseDepositFundingLimit,
    InvalidLeaseDepositAffordability,
    ReplacementLoanNotFound(u64),
    ArithmeticOverflow,
}

impl Display for LoanRuleError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "loan rule error: {self:?}")
    }
}

impl Error for LoanRuleError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CreditBand {
    Prime,
    Standard,
    Limited,
    Distressed,
    Insolvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreditBandThresholds {
    pub prime_min_units: i64,
    pub standard_min_units: i64,
    pub limited_min_units: i64,
    pub distressed_min_units: i64,
    pub insolvent_min_units: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreditModelTerms {
    pub minimum_units: i64,
    pub maximum_units: i64,
    pub starting_units: i64,
    pub band_thresholds: CreditBandThresholds,
    pub delinquency_event_penalty_units: i64,
    pub default_event_penalty_units: i64,
    pub legal_procedure_event_penalty_units: i64,
    pub adverse_day_penalty_units: i64,
    pub recovery_units: i64,
    pub default_oldest_days: u32,
    pub amount_default_threshold_krw: i64,
    pub amount_default_oldest_days: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CreditEventKind {
    EnteredDelinquency,
    EnteredDefault,
    EnteredLegalProcedure,
}

impl CreditEventKind {
    pub const fn order(self) -> u8 {
        match self {
            Self::EnteredDelinquency => 0,
            Self::EnteredDefault => 1,
            Self::EnteredLegalProcedure => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreditDayEvent {
    pub contract_id: u64,
    pub kind: CreditEventKind,
}

#[derive(Debug, Clone, Copy)]
pub struct CreditDayInput<'a> {
    pub model: CreditModelTerms,
    pub current_units: i64,
    pub events: &'a [CreditDayEvent],
    pub adverse_contract_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreditEventApplication {
    pub contract_id: u64,
    pub kind: CreditEventKind,
    pub delta_units: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreditDayCalculation {
    pub units_before: i64,
    pub event_applications: Vec<CreditEventApplication>,
    pub event_delta_units: i64,
    pub daily_delta_units: i64,
    pub units_after: i64,
    pub band: CreditBand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreditDelinquencyBucket {
    pub bucket_id: u64,
    pub days_past_due: u32,
    pub outstanding_krw: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct CreditDefaultAssessmentInput<'a> {
    pub model: CreditModelTerms,
    pub buckets: &'a [CreditDelinquencyBucket],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreditDefaultAssessment {
    pub total_outstanding_krw: i64,
    pub oldest_days_past_due: u32,
    pub should_default: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoanContractStatus {
    Pending,
    Active,
    Delinquent,
    Defaulted,
    PaidOff,
    Restructured,
    Discharged,
    ChargedOff,
    Cancelled,
}

impl LoanContractStatus {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::PaidOff | Self::Discharged | Self::ChargedOff | Self::Cancelled
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoanContractTransitionInput {
    pub from: LoanContractStatus,
    pub to: LoanContractStatus,
    pub money_moved: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoanEndOfDayStatusInput {
    pub current: LoanContractStatus,
    pub has_unpaid_buckets: bool,
    pub default_triggered: bool,
}

pub trait CreditRules: Send + Sync + 'static {
    fn starting_units(&self, model: CreditModelTerms) -> Result<i64, CreditRuleError>;

    fn band(&self, model: CreditModelTerms, units: i64) -> Result<CreditBand, CreditRuleError>;

    fn calculate_day(
        &self,
        input: CreditDayInput<'_>,
    ) -> Result<CreditDayCalculation, CreditRuleError>;

    fn assess_default(
        &self,
        input: CreditDefaultAssessmentInput<'_>,
    ) -> Result<CreditDefaultAssessment, CreditRuleError>;

    fn is_transition_allowed(
        &self,
        input: LoanContractTransitionInput,
    ) -> Result<bool, CreditRuleError>;

    fn resolve_end_of_day_status(
        &self,
        input: LoanEndOfDayStatusInput,
    ) -> Result<LoanContractStatus, CreditRuleError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreditRuleError {
    InvalidModel,
    UnitsOutOfRange,
    InvalidContractId,
    DuplicateEvent(u64, CreditEventKind),
    InvalidDelinquencyBucket,
    DuplicateDelinquencyBucket(u64),
    InvalidStatusResolution,
    ArithmeticOverflow,
}

impl Display for CreditRuleError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "credit rule error: {self:?}")
    }
}

impl Error for CreditRuleError {}

pub const REAL_ESTATE_INDEX_SCALE_PPM: i64 = 1_000_000;
pub const REAL_ESTATE_MAX_EXCLUSIVE_AREA_SQUARE_METERS: u16 = 10_000;
pub const REAL_ESTATE_MAX_INDEX_PPM: i64 = 9_007_199_254_740_991;
pub const REAL_ESTATE_MAX_LISTINGS_PER_REGION: u8 = 24;
pub const REAL_ESTATE_MAX_PUBLIC_LISTING_ID: u64 = i64::MAX as u64;
pub const REAL_ESTATE_MAX_PUBLIC_MONEY_KRW: i64 = 9_007_199_254_740_991;
pub const REAL_ESTATE_MAX_VARIATION_PPM: i64 = 10_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifeRegionKey {
    CapitalArea,
    Metropolitan,
    SmallCity,
    Rural,
}

impl LifeRegionKey {
    pub const ALL: [Self; 4] = [
        Self::CapitalArea,
        Self::Metropolitan,
        Self::SmallCity,
        Self::Rural,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CapitalArea => "capitalArea",
            Self::Metropolitan => "metropolitan",
            Self::SmallCity => "smallCity",
            Self::Rural => "rural",
        }
    }

    pub const fn order(self) -> u8 {
        match self {
            Self::CapitalArea => 1,
            Self::Metropolitan => 2,
            Self::SmallCity => 3,
            Self::Rural => 4,
        }
    }
}

impl FromStr for LifeRegionKey {
    type Err = RealEstateError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "capitalArea" => Ok(Self::CapitalArea),
            "metropolitan" => Ok(Self::Metropolitan),
            "smallCity" => Ok(Self::SmallCity),
            "rural" => Ok(Self::Rural),
            _ => Err(RealEstateError::UnknownRegion),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertyType {
    Apartment,
    MultiFamily,
    Detached,
}

impl PropertyType {
    pub const ALL: [Self; 3] = [Self::Apartment, Self::MultiFamily, Self::Detached];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Apartment => "apartment",
            Self::MultiFamily => "multiFamily",
            Self::Detached => "detached",
        }
    }

    pub const fn order(self) -> u8 {
        match self {
            Self::Apartment => 1,
            Self::MultiFamily => 2,
            Self::Detached => 3,
        }
    }
}

impl FromStr for PropertyType {
    type Err = RealEstateError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "apartment" => Ok(Self::Apartment),
            "multiFamily" => Ok(Self::MultiFamily),
            "detached" => Ok(Self::Detached),
            _ => Err(RealEstateError::UnknownPropertyType),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertyOfferKind {
    Sale,
    Jeonse,
    MonthlyRent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertyListingAvailabilityRule {
    MarketMonthInclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertyOfferRotationRule {
    SaleJeonseMonthlyRent,
}

impl PropertyListingAvailabilityRule {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MarketMonthInclusive => "marketMonthInclusive",
        }
    }
}

impl FromStr for PropertyListingAvailabilityRule {
    type Err = RealEstateError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "marketMonthInclusive" => Ok(Self::MarketMonthInclusive),
            _ => Err(RealEstateError::UnknownAvailabilityRule),
        }
    }
}

impl PropertyOfferRotationRule {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SaleJeonseMonthlyRent => "saleJeonseMonthlyRent",
        }
    }
}

impl FromStr for PropertyOfferRotationRule {
    type Err = RealEstateError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "saleJeonseMonthlyRent" => Ok(Self::SaleJeonseMonthlyRent),
            _ => Err(RealEstateError::UnknownOfferRotationRule),
        }
    }
}

impl PropertyOfferKind {
    pub const ALL: [Self; 3] = [Self::Sale, Self::Jeonse, Self::MonthlyRent];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sale => "sale",
            Self::Jeonse => "jeonse",
            Self::MonthlyRent => "monthlyRent",
        }
    }

    pub const fn order(self) -> u8 {
        match self {
            Self::Sale => 1,
            Self::Jeonse => 2,
            Self::MonthlyRent => 3,
        }
    }
}

impl FromStr for PropertyOfferKind {
    type Err = RealEstateError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "sale" => Ok(Self::Sale),
            "jeonse" => Ok(Self::Jeonse),
            "monthlyRent" => Ok(Self::MonthlyRent),
            _ => Err(RealEstateError::UnknownOfferKind),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RealEstateRegionProfile {
    pub region_key: LifeRegionKey,
    pub monthly_listing_slot_count: u8,
    pub minimum_exclusive_area_square_meters: u16,
    pub maximum_exclusive_area_square_meters: u16,
    pub base_price_per_square_meter_krw: i64,
    pub price_daily_drift_ppm: i64,
    pub price_daily_shock_amplitude_ppm: i64,
    pub rent_daily_drift_ppm: i64,
    pub rent_daily_shock_amplitude_ppm: i64,
    pub minimum_index_ppm: i64,
    pub maximum_index_ppm: i64,
    pub minimum_price_variation_ppm: i64,
    pub maximum_price_variation_ppm: i64,
    pub jeonse_ratio_ppm: i64,
    pub annual_gross_rent_yield_ppm: i64,
    pub monthly_deposit_ratio_ppm: i64,
    pub availability_rule: PropertyListingAvailabilityRule,
    pub offer_rotation_rule: PropertyOfferRotationRule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RealEstateIndexState {
    pub index_ppm: i64,
    pub remainder_numerator: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RealEstateDaily {
    pub region_key: LifeRegionKey,
    pub game_day: u32,
    pub price: RealEstateIndexState,
    pub rent: RealEstateIndexState,
}

#[derive(Debug, Clone, Copy)]
pub struct RealEstateDayZeroInput {
    pub profile: RealEstateRegionProfile,
}

#[derive(Debug, Clone, Copy)]
pub struct RealEstateNextDayInput {
    pub world_seed: u64,
    pub model_version_id: ResourceId,
    pub profile: RealEstateRegionProfile,
    pub previous: RealEstateDaily,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertyListingEntropyKey {
    pub world_seed: u64,
    pub model_version_id: ResourceId,
    pub year_month: YearMonth,
    pub region_key: LifeRegionKey,
    pub slot: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct PropertyListingGenerationInput<'a> {
    pub world_seed: u64,
    pub model_version_id: ResourceId,
    pub year_month: YearMonth,
    pub profile: RealEstateRegionProfile,
    pub allowed_property_types: &'a [PropertyType],
    pub available_from_game_day: u32,
    pub available_to_game_day: u32,
    pub month_start_daily: RealEstateDaily,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PropertyListingOffer {
    Sale {
        price_krw: i64,
    },
    Jeonse {
        deposit_krw: i64,
    },
    MonthlyRent {
        deposit_krw: i64,
        monthly_rent_krw: i64,
    },
}

impl PropertyListingOffer {
    pub const fn kind(&self) -> PropertyOfferKind {
        match self {
            Self::Sale { .. } => PropertyOfferKind::Sale,
            Self::Jeonse { .. } => PropertyOfferKind::Jeonse,
            Self::MonthlyRent { .. } => PropertyOfferKind::MonthlyRent,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertyListing {
    pub id: ResourceId,
    pub year_month: YearMonth,
    pub region_key: LifeRegionKey,
    pub slot: u8,
    pub property_type: PropertyType,
    pub exclusive_area_square_meters: u16,
    pub price_variation_ppm: i64,
    pub available_from_game_day: u32,
    pub available_to_game_day: u32,
    pub offers: Vec<PropertyListingOffer>,
}

pub trait RealEstateRules: Send + Sync + 'static {
    fn day_zero(&self, input: RealEstateDayZeroInput) -> Result<RealEstateDaily, RealEstateError>;

    fn next_day(&self, input: RealEstateNextDayInput) -> Result<RealEstateDaily, RealEstateError>;

    fn stable_listing_id(
        &self,
        key: PropertyListingEntropyKey,
    ) -> Result<ResourceId, RealEstateError>;

    fn generate_monthly_listings(
        &self,
        input: PropertyListingGenerationInput<'_>,
    ) -> Result<Vec<PropertyListing>, RealEstateError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealEstateError {
    UnknownRegion,
    UnknownPropertyType,
    UnknownOfferKind,
    UnknownAvailabilityRule,
    UnknownOfferRotationRule,
    InvalidProfile,
    InvalidPreviousDay,
    InvalidIndex,
    InvalidRemainder,
    InvalidYearMonth,
    InvalidListingWindow,
    InvalidPropertyTypes,
    InvalidListingSlot,
    InvalidOffer,
    ListingIdCollision(ResourceId),
    EntropyExhausted,
    ArithmeticOverflow,
}

impl Display for RealEstateError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "real-estate error: {self:?}")
    }
}

impl Error for RealEstateError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcquisitionIncidentalCostInput {
    pub purchase_price_krw: i64,
    pub cost_ppm: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MortgageFundingLimitInput {
    pub recognized_collateral_value_krw: i64,
    pub ltv_limit_ppm: i64,
    pub regional_price_cap_krw: Option<i64>,
    pub product_maximum_principal_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MortgageRegionalPriceCapPolicy {
    pub lower_price_threshold_krw: i64,
    pub upper_price_threshold_krw: i64,
    pub lower_band_cap_krw: i64,
    pub middle_band_cap_krw: i64,
    pub upper_band_cap_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MortgageRegionalPriceCapInput {
    pub recognized_collateral_value_krw: i64,
    pub policy: Option<MortgageRegionalPriceCapPolicy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MortgageFundingLimit {
    pub ltv_based_limit_krw: i64,
    pub regional_price_cap_krw: Option<i64>,
    pub maximum_mortgage_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertyPurchaseFundingInput {
    pub wallet_cash_krw: i64,
    pub returned_deposit_krw: i64,
    pub repaid_loan_principal_krw: i64,
    pub purchase_price_krw: i64,
    pub acquisition_incidental_cost_krw: i64,
    pub moving_cost_krw: i64,
    pub new_mortgage_principal_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertyPurchaseFundingPlan {
    pub wallet_cash_before_krw: i64,
    pub wallet_cash_after_krw: i64,
    pub available_buyer_cash_krw: i64,
    pub required_buyer_cash_krw: i64,
    pub returned_deposit_krw: i64,
    pub repaid_loan_principal_krw: i64,
    pub purchase_price_krw: i64,
    pub acquisition_incidental_cost_krw: i64,
    pub moving_cost_krw: i64,
    pub new_mortgage_principal_krw: i64,
    pub wallet_delta_krw: i64,
    pub debt_delta_krw: i64,
    pub property_book_value_delta_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertySaleLiquidityProfile {
    pub minimum_asking_ratio_ppm: i64,
    pub fast_band_maximum_asking_ratio_ppm: i64,
    pub normal_band_maximum_asking_ratio_ppm: i64,
    pub maximum_asking_ratio_ppm: i64,
    pub fast_band_minimum_delay_days: u16,
    pub fast_band_maximum_delay_days: u16,
    pub normal_band_minimum_delay_days: u16,
    pub normal_band_maximum_delay_days: u16,
    pub slow_band_minimum_delay_days: u16,
    pub slow_band_maximum_delay_days: u16,
    pub disposition_cost_rate_ppm: i64,
    pub minimum_disposition_cost_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PropertySaleReferenceValueInput {
    pub acquisition_price_krw: i64,
    pub acquisition_price_index_ppm: i64,
    pub current_price_index_ppm: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertySaleLiquidityBand {
    Fast,
    Normal,
    Slow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PropertySaleCandidateInput {
    pub world_seed: u64,
    pub listing_id: ResourceId,
    pub order_revision: u32,
    pub current_game_day: u32,
    pub reference_value_krw: i64,
    pub asking_price_krw: i64,
    pub liquidity: PropertySaleLiquidityProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertySaleCandidatePlan {
    pub liquidity_band: PropertySaleLiquidityBand,
    pub delay_days: u16,
    pub candidate_game_day: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PropertySalePeriodInput {
    pub acquired_on: Date,
    pub owner_occupied_from: Date,
    pub as_of: Date,
    pub minimum_holding_years: u16,
    pub minimum_residence_years: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertySalePeriod {
    pub completed_holding_years: u16,
    pub completed_residence_years: u16,
    pub minimum_holding_years: u16,
    pub minimum_residence_years: u16,
    pub is_eligible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PropertyDispositionCostInput {
    pub gross_sale_price_krw: i64,
    pub disposition_cost_rate_ppm: i64,
    pub minimum_disposition_cost_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PropertySaleProceedsInput {
    pub gross_sale_price_krw: i64,
    pub property_book_value_krw: i64,
    pub disposition_cost_krw: i64,
    pub mortgage_principal_payoff_krw: i64,
    pub mortgage_prepayment_fee_krw: i64,
    pub national_capital_gains_tax_krw: i64,
    pub local_capital_gains_tax_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CapitalGainsTaxScope {
    National,
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PropertySaleLedgerPosting {
    pub account_code: LedgerAccountCode,
    pub capital_gains_tax_scope: Option<CapitalGainsTaxScope>,
    pub amount_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertySaleProceedsPlan {
    pub gross_sale_price_krw: i64,
    pub property_book_value_krw: i64,
    pub disposition_cost_krw: i64,
    pub mortgage_principal_payoff_krw: i64,
    pub mortgage_prepayment_fee_krw: i64,
    pub national_capital_gains_tax_krw: i64,
    pub local_capital_gains_tax_krw: i64,
    pub total_capital_gains_tax_krw: i64,
    pub wallet_proceeds_krw: i64,
    pub postings: Vec<PropertySaleLedgerPosting>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertyTaxRoundingRule {
    HalfUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertyAcquisitionTaxPolicy {
    pub supported_home_count: u8,
    pub lower_price_maximum_krw: i64,
    pub middle_price_maximum_krw: i64,
    pub lower_rate_ppm: i64,
    pub upper_rate_ppm: i64,
    pub middle_rate_price_divisor_krw: i64,
    pub middle_rate_offset_ppm: i64,
    pub middle_rate_rounding: PropertyTaxRoundingRule,
    pub local_education_rate_ratio_ppm: i64,
    pub payment_due_days: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnnualPropertyTaxFairMarketRatioBand {
    pub official_value_upper_bound_krw: Option<i64>,
    pub fair_market_value_ratio_ppm: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnnualPropertyTaxRateSchedule {
    Special,
    Standard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnnualPropertyTaxRateBracket {
    pub rate_schedule: AnnualPropertyTaxRateSchedule,
    pub tax_base_upper_bound_krw: Option<i64>,
    pub rate_ppm: i64,
    pub progressive_deduction_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnnualPropertyTaxOwnershipCutoffRule {
    PriorDayClosingOwner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnnualPropertyTaxPaymentSplitRule {
    FloorHalfThenRemainder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnnualPropertyTaxPolicy {
    pub supported_home_count: u8,
    pub assessment_month: u8,
    pub assessment_day: u8,
    pub ownership_cutoff_rule: AnnualPropertyTaxOwnershipCutoffRule,
    pub official_value_ratio_ppm: i64,
    pub fair_market_ratio_bands: Vec<AnnualPropertyTaxFairMarketRatioBand>,
    pub special_rate_official_value_maximum_krw: i64,
    pub rate_brackets: Vec<AnnualPropertyTaxRateBracket>,
    pub local_education_rate_ratio_ppm: i64,
    pub first_payment_month: u8,
    pub first_payment_day: u8,
    pub second_payment_month: u8,
    pub second_payment_day: u8,
    pub payment_split_rule: AnnualPropertyTaxPaymentSplitRule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapitalGainsTaxRateBracket {
    pub tax_scope: CapitalGainsTaxScope,
    pub taxable_amount_upper_bound_krw: Option<i64>,
    pub rate_ppm: i64,
    pub progressive_deduction_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CapitalGainsTaxPaymentRule {
    WithheldAtSale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertyCapitalGainsTaxPolicy {
    pub supported_home_count: u8,
    pub high_value_threshold_krw: i64,
    pub basic_deduction_krw: i64,
    pub minimum_holding_years: u16,
    pub minimum_residence_years: u16,
    pub holding_deduction_start_years: u16,
    pub holding_deduction_start_rate_ppm: i64,
    pub holding_deduction_per_year_ppm: i64,
    pub holding_deduction_maximum_ppm: i64,
    pub residence_deduction_start_years: u16,
    pub residence_deduction_start_rate_ppm: i64,
    pub residence_deduction_per_year_ppm: i64,
    pub residence_deduction_maximum_ppm: i64,
    pub local_income_tax_ratio_ppm: i64,
    pub rate_brackets: Vec<CapitalGainsTaxRateBracket>,
    pub payment_rule: CapitalGainsTaxPaymentRule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertyTaxPolicy {
    pub acquisition: PropertyAcquisitionTaxPolicy,
    pub annual: AnnualPropertyTaxPolicy,
    pub capital_gains: PropertyCapitalGainsTaxPolicy,
}

#[derive(Debug, Clone, Copy)]
pub struct PropertyAcquisitionTaxInput<'a> {
    pub purchase_price_krw: i64,
    pub household_home_count: u8,
    pub policy: &'a PropertyAcquisitionTaxPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertyAcquisitionTaxCalculation {
    pub tax_base_krw: i64,
    pub acquisition_tax_rate_ppm: i64,
    pub acquisition_tax_krw: i64,
    pub local_education_rate_ratio_ppm: i64,
    pub local_education_tax_krw: i64,
    pub total_tax_krw: i64,
    pub payment_due_days: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct AnnualPropertyTaxInput<'a> {
    pub reference_value_krw: i64,
    pub household_home_count: u8,
    pub policy: &'a AnnualPropertyTaxPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnnualPropertyTaxCalculation {
    pub reference_value_krw: i64,
    pub official_value_krw: i64,
    pub fair_market_value_ratio_ppm: i64,
    pub tax_base_krw: i64,
    pub rate_schedule: AnnualPropertyTaxRateSchedule,
    pub property_tax_rate_ppm: i64,
    pub progressive_deduction_krw: i64,
    pub property_tax_krw: i64,
    pub local_education_rate_ratio_ppm: i64,
    pub local_education_tax_krw: i64,
    pub total_tax_krw: i64,
    pub first_payment_krw: i64,
    pub second_payment_krw: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct OneHomeCapitalGainsTaxInput<'a> {
    pub sale_price_krw: i64,
    pub acquisition_price_krw: i64,
    pub acquisition_incidental_cost_krw: i64,
    pub acquisition_taxes_krw: i64,
    pub disposition_cost_krw: i64,
    pub acquired_on: Date,
    pub owner_occupied_from: Date,
    pub sold_on: Date,
    pub household_home_count: u8,
    pub policy: &'a PropertyCapitalGainsTaxPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CapitalGainsTaxTreatment {
    OneHomeExempt,
    HighValueHome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapitalGainsTaxComponentCalculation {
    pub tax_scope: CapitalGainsTaxScope,
    pub rate_ppm: i64,
    pub progressive_deduction_krw: i64,
    pub tax_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OneHomeCapitalGainsTaxCalculation {
    pub treatment: CapitalGainsTaxTreatment,
    pub completed_holding_years: u16,
    pub completed_residence_years: u16,
    pub gross_gain_krw: i64,
    pub high_value_gain_krw: i64,
    pub holding_deduction_rate_ppm: i64,
    pub residence_deduction_rate_ppm: i64,
    pub long_term_deduction_rate_ppm: i64,
    pub long_term_deduction_krw: i64,
    pub basic_deduction_krw: i64,
    pub taxable_amount_krw: i64,
    pub national: CapitalGainsTaxComponentCalculation,
    pub local: CapitalGainsTaxComponentCalculation,
    pub total_tax_krw: i64,
}

pub trait PropertyRules: Send + Sync + 'static {
    fn calculate_acquisition_incidental_cost(
        &self,
        input: AcquisitionIncidentalCostInput,
    ) -> Result<i64, PropertyError>;

    fn calculate_mortgage_funding_limit(
        &self,
        input: MortgageFundingLimitInput,
    ) -> Result<MortgageFundingLimit, PropertyError>;

    fn select_mortgage_regional_price_cap(
        &self,
        input: MortgageRegionalPriceCapInput,
    ) -> Result<Option<i64>, PropertyError>;

    fn plan_purchase_funding(
        &self,
        input: PropertyPurchaseFundingInput,
    ) -> Result<PropertyPurchaseFundingPlan, PropertyError>;

    fn calculate_sale_reference_value(
        &self,
        input: PropertySaleReferenceValueInput,
    ) -> Result<i64, PropertyError>;

    fn plan_sale_candidate(
        &self,
        input: PropertySaleCandidateInput,
    ) -> Result<PropertySaleCandidatePlan, PropertyError>;

    fn calculate_sale_period(
        &self,
        input: PropertySalePeriodInput,
    ) -> Result<PropertySalePeriod, PropertyError>;

    fn calculate_disposition_cost(
        &self,
        input: PropertyDispositionCostInput,
    ) -> Result<i64, PropertyError>;

    fn plan_sale_proceeds(
        &self,
        input: PropertySaleProceedsInput,
    ) -> Result<PropertySaleProceedsPlan, PropertyError>;
}

pub trait PropertyTaxRules: Send + Sync + 'static {
    fn validate_policy(&self, policy: &PropertyTaxPolicy) -> Result<(), PropertyTaxError>;

    fn calculate_acquisition_tax(
        &self,
        input: PropertyAcquisitionTaxInput<'_>,
    ) -> Result<PropertyAcquisitionTaxCalculation, PropertyTaxError>;

    fn calculate_annual_property_tax(
        &self,
        input: AnnualPropertyTaxInput<'_>,
    ) -> Result<AnnualPropertyTaxCalculation, PropertyTaxError>;

    fn calculate_one_home_capital_gains_tax(
        &self,
        input: OneHomeCapitalGainsTaxInput<'_>,
    ) -> Result<OneHomeCapitalGainsTaxCalculation, PropertyTaxError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyError {
    InvalidAcquisitionCost,
    InvalidMortgageFundingLimit,
    InvalidMortgageRegionalPriceCap,
    InvalidPurchaseFunding,
    InvalidSaleReferenceValue,
    InvalidSaleLiquidityProfile,
    InvalidSaleCandidate,
    AskingPriceOutOfRange,
    InvalidSalePeriod,
    InvalidDispositionCost,
    InvalidSaleProceeds,
    InsufficientWalletCash,
    InsufficientSaleProceeds,
    EntropyExhausted,
    ArithmeticOverflow,
}

impl Display for PropertyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "property error: {self:?}")
    }
}

impl Error for PropertyError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyTaxError {
    InvalidPolicy,
    InvalidAcquisitionTaxInput,
    InvalidAnnualPropertyTaxInput,
    InvalidCapitalGainsTaxInput,
    PolicyUnsupported,
    ArithmeticOverflow,
}

impl Display for PropertyTaxError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "property tax error: {self:?}")
    }
}

impl Error for PropertyTaxError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HousingLeaseCapability {
    CashJeonse,
    CashJeonseAndMonthlyRent,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HousingLeaseOfferKind {
    Jeonse,
    MonthlyRent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HousingLeaseRenewalRule {
    OpenEnded,
    FixedTermAutoRenew,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HousingLeaseTerminationReviewRule {
    OldestActiveArrearAge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HousingRentChargeRule {
    NextMonthStartFull,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HousingLeaseArrearRepaymentRule {
    ManualOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HousingLeaseRole {
    Tenant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CashJeonseMoveInput {
    pub wallet_cash_krw: i64,
    pub existing_deposit_krw: i64,
    pub new_deposit_krw: i64,
    pub moving_cost_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseMovePostingLease {
    Ended,
    Started,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseMoveLedgerPosting {
    pub account_code: LedgerAccountCode,
    pub lease_contract: Option<LeaseMovePostingLease>,
    pub amount_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseMoveLivingCostAction {
    PreserveCurrentMonth,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CashJeonseMovePlan {
    pub returned_deposit_krw: i64,
    pub deposit_krw: i64,
    pub moving_cost_krw: i64,
    pub wallet_delta_krw: i64,
    pub wallet_after_krw: i64,
    pub tenant_lease_deposit_krw: i64,
    pub lease_deposit_asset_delta_krw: i64,
    pub net_worth_delta_krw: i64,
    pub living_cost_action: LeaseMoveLivingCostAction,
    pub postings: Vec<LeaseMoveLedgerPosting>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseMoveFundingInput {
    pub wallet_cash_krw: i64,
    pub existing_deposit_krw: i64,
    pub repaid_loan_principal_krw: i64,
    pub new_deposit_krw: i64,
    pub new_loan_principal_krw: i64,
    pub moving_cost_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseMovePostingLoan {
    Repaid,
    Originated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseMoveFundingLedgerPosting {
    pub account_code: LedgerAccountCode,
    pub lease_contract: Option<LeaseMovePostingLease>,
    pub loan_contract: Option<LeaseMovePostingLoan>,
    pub amount_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseMoveFundingPlan {
    pub returned_deposit_krw: i64,
    pub repaid_loan_principal_krw: i64,
    pub deposit_krw: i64,
    pub new_loan_principal_krw: i64,
    pub moving_cost_krw: i64,
    pub wallet_delta_krw: i64,
    pub wallet_after_krw: i64,
    pub debt_delta_krw: i64,
    pub tenant_lease_deposit_krw: i64,
    pub lease_deposit_asset_delta_krw: i64,
    pub net_worth_delta_krw: i64,
    pub living_cost_action: LeaseMoveLivingCostAction,
    pub postings: Vec<LeaseMoveFundingLedgerPosting>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseRentPostingOwner {
    None,
    RentCharge,
    Arrear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseRentLedgerPosting {
    pub account_code: LedgerAccountCode,
    pub owner: LeaseRentPostingOwner,
    pub amount_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonthlyRentSettlementInput {
    pub wallet_cash_krw: i64,
    pub monthly_rent_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthlyRentSettlementPlan {
    pub paid_krw: i64,
    pub arrear_krw: i64,
    pub wallet_after_krw: i64,
    pub postings: Vec<LeaseRentLedgerPosting>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseArrearPaymentInput {
    pub wallet_cash_krw: i64,
    pub outstanding_krw: i64,
    pub amount_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseArrearPaymentPlan {
    pub paid_krw: i64,
    pub remaining_krw: i64,
    pub wallet_after_krw: i64,
    pub postings: Vec<LeaseRentLedgerPosting>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonthlyRentChargeDue {
    pub due_game_day: u32,
    pub due_year_month: YearMonth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseTermPlanInput {
    /// The first day of the original contract, never a prior term's clamped boundary.
    pub anchor_game_day: u32,
    pub anchor_date: Date,
    pub term_no: u32,
    pub term_months: u16,
    pub renewal_notice_lead_days: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseTermPlan {
    pub term_no: u32,
    pub effective_from_game_day: u32,
    pub effective_to_game_day: u32,
    pub renewal_notice_game_day: u32,
    pub renewal_game_day: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseTerminationReviewInput {
    pub current_game_day: u32,
    pub review_after_days: u16,
    pub oldest_active_arrear_created_game_day: Option<u32>,
    pub review_is_open: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseTerminationReviewDecision {
    NoAction,
    Schedule { due_game_day: u32 },
    Open,
    KeepOpen,
    Resolve,
}

pub trait LeaseRules: Send + Sync + 'static {
    fn plan_cash_jeonse_move(
        &self,
        input: CashJeonseMoveInput,
    ) -> Result<CashJeonseMovePlan, LeaseError>;

    fn plan_lease_move_funding(
        &self,
        input: LeaseMoveFundingInput,
    ) -> Result<LeaseMoveFundingPlan, LeaseError>;

    fn plan_monthly_rent_settlement(
        &self,
        input: MonthlyRentSettlementInput,
    ) -> Result<MonthlyRentSettlementPlan, LeaseError>;

    fn plan_lease_arrear_payment(
        &self,
        input: LeaseArrearPaymentInput,
    ) -> Result<LeaseArrearPaymentPlan, LeaseError>;

    fn next_monthly_rent_charge(
        &self,
        current_game_day: u32,
        market_date: Date,
    ) -> Result<MonthlyRentChargeDue, LeaseError>;

    fn plan_lease_term(&self, input: LeaseTermPlanInput) -> Result<LeaseTermPlan, LeaseError>;

    fn decide_lease_termination_review(
        &self,
        input: LeaseTerminationReviewInput,
    ) -> Result<LeaseTerminationReviewDecision, LeaseError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseError {
    InvalidWalletCash,
    InvalidExistingDeposit,
    InvalidNewDeposit,
    InvalidMovingCost,
    InvalidRepaidLoanPrincipal,
    InvalidNewLoanPrincipal,
    InvalidMonthlyRent,
    InvalidArrearBalance,
    InvalidArrearPayment,
    ArrearPaymentExceedsOutstanding,
    InvalidTermNumber,
    InvalidTermMonths,
    InvalidRenewalNoticeLeadDays,
    InvalidTerminationReviewAfterDays,
    InvalidArrearGameDay,
    InsufficientWalletCash,
    ArithmeticOverflow,
}

impl Display for LeaseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "lease error: {self:?}")
    }
}

impl Error for LeaseError {}

pub const WELFARE_SCHEMA_VERSION: u16 = 1;
pub const WELFARE_MAX_AST_DEPTH: usize = 12;
pub const WELFARE_MAX_PROGRAM_NODES: usize = 128;
pub const WELFARE_MAX_LOGICAL_CHILDREN: usize = 16;
pub const WELFARE_MAX_IN_LITERALS: usize = 32;
pub const WELFARE_MAX_CONSTANTS: usize = 64;
pub const WELFARE_MAX_PUBLIC_FACTS: usize = 32;
pub const WELFARE_MAX_CONDITIONS: usize = 32;
pub const WELFARE_MAX_PREVIOUS_CLOSED_DAYS: u16 = 366;
pub const WELFARE_MAX_COLLECTION_ROWS: usize = 32;
pub const WELFARE_MAX_STRING_SCALARS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", content = "schemaKey", rename_all = "camelCase")]
pub enum WelfareValueType {
    Boolean,
    Integer,
    MoneyKrw,
    Count,
    AgeYears,
    Date,
    String,
    Enum(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WelfareEnumValue {
    pub schema_key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum WelfareValue {
    Boolean(bool),
    Integer(i64),
    MoneyKrw(i64),
    Count(i64),
    AgeYears(i64),
    Date(Date),
    String(String),
    Enum(WelfareEnumValue),
}

impl WelfareValue {
    pub fn value_type(&self) -> WelfareValueType {
        match self {
            Self::Boolean(_) => WelfareValueType::Boolean,
            Self::Integer(_) => WelfareValueType::Integer,
            Self::MoneyKrw(_) => WelfareValueType::MoneyKrw,
            Self::Count(_) => WelfareValueType::Count,
            Self::AgeYears(_) => WelfareValueType::AgeYears,
            Self::Date(_) => WelfareValueType::Date,
            Self::String(_) => WelfareValueType::String,
            Self::Enum(value) => WelfareValueType::Enum(value.schema_key.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WelfareResolvedWindow {
    CurrentDay,
    PreviousClosedDays { days: u16 },
    PriorClose,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WelfareWindowDays {
    Literal { days: u16 },
    Constant { key: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WelfareWindowSpec {
    CurrentDay,
    PreviousClosedDays { days: WelfareWindowDays },
    PriorClose,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WelfareWindowConstraint {
    CurrentDay,
    PreviousClosedDays { minimum: u16, maximum: u16 },
    PriorClose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WelfareFactSource {
    GameDay,
    Household,
    Residence,
    Employment,
    Military,
    Income,
    Asset,
    Debt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WelfareFactDefinition {
    pub path: String,
    pub value_type: WelfareValueType,
    pub window: WelfareWindowConstraint,
    pub source: WelfareFactSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WelfareCollectionDefinition {
    pub key: String,
    pub item_type: WelfareValueType,
    pub window: WelfareWindowConstraint,
    pub source: WelfareFactSource,
    pub maximum_rows: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WelfareEnumDefinition {
    pub schema_key: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WelfareFactRegistry {
    pub schema_version: u16,
    pub facts: Vec<WelfareFactDefinition>,
    pub collections: Vec<WelfareCollectionDefinition>,
    pub enums: Vec<WelfareEnumDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operator", rename_all = "camelCase")]
pub enum WelfareExpression {
    All {
        children: Vec<WelfareExpression>,
    },
    Any {
        children: Vec<WelfareExpression>,
    },
    Not {
        child: Box<WelfareExpression>,
    },
    Eq {
        left: Box<WelfareExpression>,
        right: Box<WelfareExpression>,
    },
    In {
        value: Box<WelfareExpression>,
        literals: Vec<WelfareValue>,
    },
    Lt {
        left: Box<WelfareExpression>,
        right: Box<WelfareExpression>,
    },
    Lte {
        left: Box<WelfareExpression>,
        right: Box<WelfareExpression>,
    },
    Gt {
        left: Box<WelfareExpression>,
        right: Box<WelfareExpression>,
    },
    Gte {
        left: Box<WelfareExpression>,
        right: Box<WelfareExpression>,
    },
    Between {
        value: Box<WelfareExpression>,
        lower: Box<WelfareExpression>,
        upper: Box<WelfareExpression>,
    },
    Sum {
        collection: String,
        window: WelfareWindowSpec,
    },
    Count {
        collection: String,
        window: WelfareWindowSpec,
    },
    Exists {
        collection: String,
        window: WelfareWindowSpec,
    },
    Fact {
        path: String,
        window: WelfareWindowSpec,
    },
    Constant {
        key: String,
    },
    Literal {
        value: WelfareValue,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operator", rename_all = "camelCase")]
pub enum WelfareEligibilityExpression {
    All {
        children: Vec<WelfareEligibilityExpression>,
    },
    Any {
        children: Vec<WelfareEligibilityExpression>,
    },
    Not {
        child: Box<WelfareEligibilityExpression>,
    },
    Condition {
        code: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WelfareProgramConstant {
    pub key: String,
    pub value: WelfareValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WelfareProgramCondition {
    pub code: String,
    pub expression: WelfareExpression,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WelfareProgramPurpose {
    GameBalance,
    RealPolicyReference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WelfareRankedAvailability {
    UnrankedOnly,
    RankedAndUnranked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WelfareBenefitDefinition {
    pub amount_constant_key: String,
    pub payment_delay_days: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WelfareProgramDefinition {
    pub schema_version: u16,
    pub program_version_id: ResourceId,
    pub program_key: String,
    pub purpose: WelfareProgramPurpose,
    pub ranked_availability: WelfareRankedAvailability,
    pub duplicate_group_key: String,
    pub constants: Vec<WelfareProgramConstant>,
    pub conditions: Vec<WelfareProgramCondition>,
    pub eligibility_root: WelfareEligibilityExpression,
    pub benefit: WelfareBenefitDefinition,
    pub reassessment_triggers: Vec<WelfareFactSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WelfareUnknownReason {
    AuthorityMissing,
    ValuationUnavailable,
    CollectionLimitExceeded,
    WindowIncomplete,
    ArithmeticOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "reason", rename_all = "camelCase")]
pub enum WelfareTruth {
    True,
    False,
    Unknown(WelfareUnknownReason),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "value", rename_all = "camelCase")]
pub enum WelfareEvidenceValue {
    Known(WelfareValue),
    Unknown(WelfareUnknownReason),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WelfareFactEvidence {
    pub key: String,
    pub value_type: WelfareValueType,
    pub window: WelfareResolvedWindow,
    pub value: WelfareEvidenceValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "values", rename_all = "camelCase")]
pub enum WelfareCollectionEvidenceValue {
    Known(Vec<WelfareValue>),
    Unknown(WelfareUnknownReason),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WelfareCollectionEvidence {
    pub key: String,
    pub item_type: WelfareValueType,
    pub window: WelfareResolvedWindow,
    pub value: WelfareCollectionEvidenceValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WelfareWindowBound {
    pub window: WelfareResolvedWindow,
    pub start_game_day: u32,
    pub end_game_day: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WelfareAuthorityRevision {
    pub authority: String,
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WelfarePeriodPin {
    pub evaluation_game_day: u32,
    pub window_bounds: Vec<WelfareWindowBound>,
    pub authority_revisions: Vec<WelfareAuthorityRevision>,
}

#[derive(Debug, Clone, Copy)]
pub struct WelfareEvaluationInput<'a> {
    pub facts: &'a [WelfareFactEvidence],
    pub collections: &'a [WelfareCollectionEvidence],
    pub period_pin: &'a WelfarePeriodPin,
}

#[derive(Debug, Clone, Copy)]
pub struct WelfareFingerprintInput<'a> {
    pub schema_version: u16,
    pub program_version_id: ResourceId,
    pub facts: &'a [WelfareFactEvidence],
    pub collections: &'a [WelfareCollectionEvidence],
    pub period_pin: &'a WelfarePeriodPin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WelfareConditionResult {
    pub code: String,
    pub result: WelfareTruth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WelfareEvaluationStatus {
    NotEvaluated,
    Eligible,
    Ineligible,
    Indeterminate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WelfareEvaluation {
    pub status: WelfareEvaluationStatus,
    pub fact_fingerprint: String,
    pub conditions: Vec<WelfareConditionResult>,
}

pub trait WelfareRules: Send + Sync + 'static {
    fn fact_registry(&self) -> &WelfareFactRegistry;

    fn validate_program(&self, program: &WelfareProgramDefinition) -> Result<(), WelfareError>;

    fn evaluate_program(
        &self,
        program: &WelfareProgramDefinition,
        input: &WelfareEvaluationInput<'_>,
    ) -> Result<WelfareEvaluation, WelfareError>;

    fn fingerprint(&self, input: &WelfareFingerprintInput<'_>) -> Result<String, WelfareError>;

    fn canonical_fingerprint_json(
        &self,
        input: &WelfareFingerprintInput<'_>,
    ) -> Result<String, WelfareError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WelfareError {
    UnsupportedSchemaVersion,
    InvalidCanonicalKey,
    DuplicateConstant,
    DuplicateCondition,
    DuplicateFact,
    DuplicateCollection,
    DuplicateEnum,
    DuplicatePeriodBound,
    DuplicateAuthorityRevision,
    DuplicateReassessmentTrigger,
    TooManyConstants,
    TooManyConditions,
    TooManyPublicFacts,
    AstTooDeep,
    ProgramTooLarge,
    InvalidLogicalArity,
    InvalidInArity,
    InvalidStringLiteral,
    InvalidWindow,
    InvalidCollectionBound,
    UnknownFact,
    UnknownCollection,
    UnknownConstant,
    UnknownCondition,
    UnreachableCondition,
    UnusedConstant,
    MissingReassessmentTrigger,
    TypeMismatch,
    UnitMismatch,
    UnorderedType,
    InvalidBetweenBounds,
    InvalidLiteral,
    InvalidEligibilityRoot,
    InvalidBenefit,
    InvalidEvidence,
    InvalidPeriodPin,
    CanonicalSerialization,
}

impl Display for WelfareError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "welfare error: {self:?}")
    }
}

impl Error for WelfareError {}

pub const LIFE_EVENT_SCHEMA_VERSION: u16 = 1;
pub const LIFE_EVENT_FACT_REGISTRY_SCHEMA_VERSION: u16 = 1;
pub const LIFE_EVENT_ENTROPY_STREAM_VERSION: u16 = 1;
pub const LIFE_EVENT_PROBABILITY_SCALE_PPM: u32 = 1_000_000;
pub const LIFE_EVENT_MAX_DEFINITIONS: usize = 32;
pub const LIFE_EVENT_MIN_CHOICES: usize = 2;
pub const LIFE_EVENT_MAX_CHOICES: usize = 8;
pub const LIFE_EVENT_MAX_AST_DEPTH: usize = 12;
pub const LIFE_EVENT_MAX_AST_NODES: usize = 128;
pub const LIFE_EVENT_MAX_LOGICAL_CHILDREN: usize = 32;
pub const LIFE_EVENT_MAX_PENDING: usize = 8;
pub const LIFE_EVENT_MAX_EFFECT_KRW: i64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifeEventValueType {
    Boolean,
    Count,
    AgeYears,
    Enum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifeEventUnit {
    Boolean,
    Count,
    Years,
    Enum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifeEventWindowKind {
    CurrentGameDay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifeEventFactSourceKind {
    GameDay,
    Household,
    Residence,
    Military,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeEventFactDefinition {
    pub id: ResourceId,
    pub fact_order: u8,
    pub fact_key: String,
    pub value_type: LifeEventValueType,
    pub unit: LifeEventUnit,
    pub enum_schema_key: Option<String>,
    pub window_kind: LifeEventWindowKind,
    pub source_schema_version: u16,
    pub source_kind: LifeEventFactSourceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifeEventPurpose {
    GameBalance,
    RealPolicyReference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifeEventRankedAvailability {
    UnrankedOnly,
    RankedAndUnranked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifeEventDecisionKind {
    Accepted,
    Declined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifeEventEffectKind {
    NoEffect,
    FixedWalletExpense,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifeEventEffectAccountCode {
    LifeEventExpense,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum LifeEventEffect {
    NoEffect,
    FixedWalletExpense {
        amount_krw: i64,
        account_code: LifeEventEffectAccountCode,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LifeEventEffectAst {
    pub version: u16,
    #[serde(flatten)]
    pub effect: LifeEventEffect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeEventFactReference {
    pub path: String,
    pub unit: LifeEventUnit,
    pub window: LifeEventWindowKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "valueType",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LifeEventLiteralValue {
    Boolean(bool),
    Count(i64),
    AgeYears(i64),
    Enum { schema_key: String, value: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum LifeEventOperand {
    Fact {
        reference: LifeEventFactReference,
    },
    Literal {
        unit: LifeEventUnit,
        value: LifeEventLiteralValue,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum LifeEventExpression {
    All {
        children: Vec<LifeEventExpression>,
    },
    Any {
        children: Vec<LifeEventExpression>,
    },
    Not {
        child: Box<LifeEventExpression>,
    },
    Eq {
        left: Box<LifeEventOperand>,
        right: Box<LifeEventOperand>,
    },
    Gte {
        left: Box<LifeEventOperand>,
        right: Box<LifeEventOperand>,
    },
    Between {
        value: Box<LifeEventOperand>,
        lower: Box<LifeEventOperand>,
        upper: Box<LifeEventOperand>,
    },
    Fact {
        reference: LifeEventFactReference,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeEventEligibilityAst {
    pub version: u16,
    pub root: LifeEventExpression,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeEventChoiceDefinition {
    pub id: ResourceId,
    pub choice_order: u8,
    pub choice_key: String,
    pub display_name: String,
    pub decision_kind: LifeEventDecisionKind,
    pub effect_kind: LifeEventEffectKind,
    pub effect_amount_krw: Option<i64>,
    pub effect_account_code: Option<LifeEventEffectAccountCode>,
    pub effect_ast: LifeEventEffectAst,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeEventDefinition {
    pub id: ResourceId,
    pub schema_version: u16,
    pub entropy_stream_version: u16,
    pub event_order: u8,
    pub event_key: String,
    pub display_name: String,
    pub purpose: LifeEventPurpose,
    pub ranked_availability: LifeEventRankedAvailability,
    pub eligibility_ast: LifeEventEligibilityAst,
    pub ast_node_count: u16,
    pub ast_max_depth: u8,
    pub hazard_ppm: u32,
    pub cooldown_game_days: u16,
    pub maximum_occurrences: u16,
    pub priority: u16,
    pub exclusive_group_key: Option<String>,
    pub offer_duration_game_days: u16,
    pub default_choice_key: String,
    pub choices: Vec<LifeEventChoiceDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeEventCatalog {
    pub component_version_id: ResourceId,
    pub component_version_key: String,
    pub fact_registry_schema_version: u16,
    pub facts: Vec<LifeEventFactDefinition>,
    pub definitions: Vec<LifeEventDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "valueType",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LifeEventValue {
    Boolean(bool),
    Count(i64),
    AgeYears(i64),
    Enum { schema_key: String, value: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifeEventUnknownReason {
    AuthorityMissing,
    CollectionLimitExceeded,
    ArithmeticOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "value", rename_all = "camelCase")]
pub enum LifeEventEvidenceValue {
    Known(LifeEventValue),
    Unknown(LifeEventUnknownReason),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeEventFactEvidence {
    pub fact_key: String,
    pub value: LifeEventEvidenceValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "reason", rename_all = "camelCase")]
pub enum LifeEventTruth {
    True,
    False,
    Unknown(LifeEventUnknownReason),
}

#[derive(Debug, Clone, Copy)]
pub struct LifeEventEligibilityInput<'a> {
    pub catalog: &'a LifeEventCatalog,
    pub event_definition_id: ResourceId,
    pub facts: &'a [LifeEventFactEvidence],
}

#[derive(Debug, Clone, Copy)]
pub struct LifeEventEntropyInput<'a> {
    pub world_seed: u64,
    pub save_id: ResourceId,
    pub run_revision: u32,
    pub year_month: YearMonth,
    pub event_key: &'a str,
    pub occurrence_no: u16,
}

pub trait LifeEventEntropy: Send + Sync + 'static {
    fn digest(&self, world_seed: u64, canonical_message: &[u8])
    -> Result<[u8; 32], LifeEventError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeEventOccurrence {
    pub event_definition_id: ResourceId,
    pub occurrence_no: u16,
    pub offered_game_day: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct LifeEventMonthPlanInput<'a> {
    pub catalog: &'a LifeEventCatalog,
    pub world_seed: u64,
    pub save_id: ResourceId,
    pub run_revision: u32,
    pub year_month: YearMonth,
    pub target_game_day: u32,
    pub authority_state_revision: u64,
    pub eligibility_fact_fingerprint: &'a str,
    pub facts: &'a [LifeEventFactEvidence],
    pub prior_occurrences: &'a [LifeEventOccurrence],
    pub existing_pending_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifeEventCandidateResult {
    Ineligible,
    Indeterminate,
    NotSelected,
    Suppressed,
    Offered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeEventCandidatePlan {
    pub candidate_order: u8,
    pub event_definition_id: ResourceId,
    pub event_key: String,
    pub occurrence_no: u16,
    pub eligibility_fact_fingerprint: String,
    pub result: LifeEventCandidateResult,
    pub unknown_reason: Option<LifeEventUnknownReason>,
    pub roll_ppm: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeEventOfferPlan {
    pub event_definition_id: ResourceId,
    pub event_key: String,
    pub occurrence_no: u16,
    pub offered_game_day: u32,
    pub expires_game_day: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeEventMonthPlan {
    pub save_id: ResourceId,
    pub run_revision: u32,
    pub component_version_id: ResourceId,
    pub year_month: YearMonth,
    pub target_game_day: u32,
    pub authority_state_revision: u64,
    pub fact_registry_schema_version: u16,
    pub entropy_stream_version: u16,
    pub candidates: Vec<LifeEventCandidatePlan>,
    pub offers: Vec<LifeEventOfferPlan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifeEventLedgerAccountCode {
    LifeEventExpense,
    Wallet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeEventLedgerPosting {
    pub account_code: LifeEventLedgerAccountCode,
    pub amount_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeEventEffectPlan {
    pub wallet_cash_before_krw: i64,
    pub wallet_cash_after_krw: i64,
    pub wallet_delta_krw: i64,
    pub postings: Vec<LifeEventLedgerPosting>,
}

#[derive(Debug, Clone, Copy)]
pub struct LifeEventEffectPlanInput<'a> {
    pub effect: &'a LifeEventEffect,
    pub wallet_cash_krw: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct LifeEventChoiceResolutionInput<'a> {
    pub catalog: &'a LifeEventCatalog,
    pub event_definition_id: ResourceId,
    pub event_instance_id: ResourceId,
    pub offered_game_day: u32,
    pub expires_game_day: u32,
    pub choice_id: ResourceId,
    pub current_game_day: u32,
    pub wallet_cash_krw: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct LifeEventExpiryResolutionInput<'a> {
    pub catalog: &'a LifeEventCatalog,
    pub event_definition_id: ResourceId,
    pub event_instance_id: ResourceId,
    pub offered_game_day: u32,
    pub expires_game_day: u32,
    pub current_game_day: u32,
    pub wallet_cash_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifeEventResolutionKind {
    Accepted,
    Declined,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeEventResolutionPlan {
    pub event_instance_id: ResourceId,
    pub choice_id: ResourceId,
    pub resolution_kind: LifeEventResolutionKind,
    pub resolved_game_day: u32,
    pub effect: LifeEventEffectPlan,
}

pub trait LifeEventRules: Send + Sync + 'static {
    fn validate_catalog(&self, catalog: &LifeEventCatalog) -> Result<(), LifeEventError>;

    fn evaluate_eligibility(
        &self,
        input: LifeEventEligibilityInput<'_>,
    ) -> Result<LifeEventTruth, LifeEventError>;

    fn eligibility_digest(
        &self,
        input: LifeEventEntropyInput<'_>,
    ) -> Result<[u8; 32], LifeEventError>;

    fn eligibility_roll_ppm(&self, input: LifeEventEntropyInput<'_>)
    -> Result<u32, LifeEventError>;

    fn plan_month(
        &self,
        input: LifeEventMonthPlanInput<'_>,
    ) -> Result<LifeEventMonthPlan, LifeEventError>;

    fn plan_effect(
        &self,
        input: LifeEventEffectPlanInput<'_>,
    ) -> Result<LifeEventEffectPlan, LifeEventError>;

    fn resolve_choice(
        &self,
        input: LifeEventChoiceResolutionInput<'_>,
    ) -> Result<LifeEventResolutionPlan, LifeEventError>;

    fn resolve_expired(
        &self,
        input: LifeEventExpiryResolutionInput<'_>,
    ) -> Result<LifeEventResolutionPlan, LifeEventError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifeEventError {
    UnsupportedSchemaVersion,
    UnsupportedEntropyStreamVersion,
    InvalidComponentVersionKey,
    InvalidCanonicalKey,
    InvalidDisplayName,
    InvalidFactRegistry,
    DuplicateFact,
    DuplicateDefinition,
    DuplicateChoice,
    InvalidDefinitionOrder,
    InvalidChoiceOrder,
    InvalidProbability,
    InvalidDefinitionLimits,
    InvalidLogicalArity,
    AstTooDeep,
    AstTooLarge,
    AstProjectionMismatch,
    UnknownFact,
    UnknownEnum,
    InvalidLiteral,
    TypeMismatch,
    UnitMismatch,
    UnorderedType,
    InvalidBetweenBounds,
    InvalidEligibilityRoot,
    InvalidDefaultChoice,
    InvalidEffect,
    InvalidEvidence,
    InvalidYearMonth,
    InvalidFactFingerprint,
    InvalidOccurrenceHistory,
    PendingLimitExceeded,
    EntropyUnavailable,
    ArithmeticOverflow,
    InvalidWalletCash,
    InsufficientWalletCash,
    EventNotFound,
    ChoiceNotFound,
    EventExpired,
    EventNotExpired,
    InvalidOfferPeriod,
    UnbalancedLedgerPlan,
}

impl Display for LifeEventError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "life-event error: {self:?}")
    }
}

impl Error for LifeEventError {}

pub const INSURANCE_SCHEMA_VERSION: u16 = 1;
pub const INSURANCE_FACT_REGISTRY_SCHEMA_VERSION: u16 = 1;
pub const INSURANCE_MAX_PRODUCTS: usize = 16;
pub const INSURANCE_MAX_COVERAGES_PER_PRODUCT: usize = 8;
pub const INSURANCE_MAX_ACTIVE_CONTRACTS: usize = 8;
pub const INSURANCE_MAX_CLAIM_CONTRACTS: usize = 8;
pub const INSURANCE_MAX_AST_DEPTH: usize = LIFE_EVENT_MAX_AST_DEPTH;
pub const INSURANCE_MAX_AST_NODES: usize = LIFE_EVENT_MAX_AST_NODES;
pub const INSURANCE_MAX_LOGICAL_CHILDREN: usize = LIFE_EVENT_MAX_LOGICAL_CHILDREN;
pub const INSURANCE_MAX_MONEY_KRW: i64 = LIFE_EVENT_MAX_EFFECT_KRW;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceFactRegistry {
    pub schema_version: u16,
    pub facts: Vec<LifeEventFactDefinition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsurancePurpose {
    GameBalance,
    RealPolicyReference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsuranceRankedAvailability {
    UnrankedOnly,
    RankedAndUnranked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsuranceCoverageKind {
    FixedIndemnity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceCoverageDefinition {
    pub coverage_version_id: ResourceId,
    pub coverage_order: u8,
    pub coverage_kind: InsuranceCoverageKind,
    pub event_key: String,
    pub effect_kind: LifeEventEffectKind,
    pub deductible_krw: i64,
    pub occurrence_limit_krw: i64,
    pub term_limit_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceProductDefinition {
    pub product_version_id: ResourceId,
    pub schema_version: u16,
    pub product_order: u8,
    pub product_key: String,
    pub display_name: String,
    pub purpose: InsurancePurpose,
    pub ranked_availability: InsuranceRankedAvailability,
    pub eligibility_ast: LifeEventEligibilityAst,
    pub ast_node_count: u16,
    pub ast_max_depth: u8,
    pub premium_krw: i64,
    pub premium_cadence_game_days: u16,
    pub term_game_days: u16,
    pub waiting_game_days: u16,
    pub claim_window_game_days: u16,
    pub grace_game_days: u16,
    pub reinstatement_allowed: bool,
    pub automatic_renewal: bool,
    pub coverages: Vec<InsuranceCoverageDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceCatalog {
    pub component_version_id: ResourceId,
    pub component_version_key: String,
    pub schema_version: u16,
    pub fact_registry_schema_version: u16,
    pub facts: Vec<LifeEventFactDefinition>,
    pub products: Vec<InsuranceProductDefinition>,
}

#[derive(Debug, Clone, Copy)]
pub struct InsuranceEligibilityInput<'a> {
    pub catalog: &'a InsuranceCatalog,
    pub product_version_id: ResourceId,
    pub evaluation_game_day: u32,
    pub facts: &'a [LifeEventFactEvidence],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsuranceEligibilityStatus {
    Eligible,
    Ineligible,
    Indeterminate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum InsuranceEligibilityReason {
    EligibilityExpressionFalse,
    FactUnknown {
        fact_key: String,
        reason: LifeEventUnknownReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceEligibilityEvaluation {
    pub status: InsuranceEligibilityStatus,
    pub reasons: Vec<InsuranceEligibilityReason>,
    pub fact_fingerprint: String,
}

pub trait InsuranceClaimPinHasher: Send + Sync + 'static {
    fn digest(&self, canonical_bytes: &[u8]) -> Result<[u8; 32], InsuranceError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsuranceContractStatus {
    Pending,
    Active,
    Lapsed,
    Expired,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsurancePremiumChargeStatus {
    Scheduled,
    Paid,
    Missed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsurancePremiumChargePlan {
    pub charge_no: u16,
    pub due_game_day: u32,
    pub amount_krw: i64,
    pub status: InsurancePremiumChargeStatus,
}

#[derive(Debug, Clone, Copy)]
pub struct InsuranceContractPlanInput<'a> {
    pub contract_id: ResourceId,
    pub product: &'a InsuranceProductDefinition,
    pub start_game_day: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceContractPlan {
    pub contract_id: ResourceId,
    pub product_version_id: ResourceId,
    pub status: InsuranceContractStatus,
    pub coverage_start_game_day: u32,
    pub waiting_ends_game_day: u32,
    pub coverage_end_exclusive: u32,
    pub premium_charges: Vec<InsurancePremiumChargePlan>,
}

#[derive(Debug, Clone, Copy)]
pub struct InsurancePremiumResolutionInput {
    pub contract_id: ResourceId,
    pub charge_no: u16,
    pub due_game_day: u32,
    pub premium_krw: i64,
    pub wallet_cash_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsurancePremiumResolution {
    pub contract_id: ResourceId,
    pub charge_no: u16,
    pub charge_status: InsurancePremiumChargeStatus,
    pub contract_status: InsuranceContractStatus,
    pub paid_krw: i64,
    pub wallet_cash_before_krw: i64,
    pub wallet_cash_after_krw: i64,
    pub coverage_end_exclusive: Option<u32>,
    pub cancel_future_charges: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct InsuranceCoverageInput {
    pub coverage_start_game_day: u32,
    pub waiting_ends_game_day: u32,
    pub coverage_end_exclusive: u32,
    pub event_offered_game_day: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsuranceTerminationKind {
    Lapse,
    Cancellation,
}

#[derive(Debug, Clone, Copy)]
pub struct InsuranceTerminationInput {
    pub contract_id: ResourceId,
    pub coverage_start_game_day: u32,
    pub current_coverage_end_exclusive: u32,
    pub effective_game_day: u32,
    pub kind: InsuranceTerminationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceTerminationPlan {
    pub contract_id: ResourceId,
    pub status: InsuranceContractStatus,
    pub kind: InsuranceTerminationKind,
    pub effective_game_day: u32,
    pub coverage_end_exclusive: u32,
    pub cancel_future_charges: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct InsuranceContractExpiryInput {
    pub contract_id: ResourceId,
    pub current_status: InsuranceContractStatus,
    pub coverage_end_exclusive: u32,
    pub target_game_day: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceContractExpiryPlan {
    pub contract_id: ResourceId,
    pub status: InsuranceContractStatus,
    pub expired_game_day: u32,
    pub coverage_end_exclusive: u32,
    pub cancel_future_charges: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceClaimContractPin {
    pub contract_id: ResourceId,
    pub product_version_id: ResourceId,
    pub coverage_version_id: ResourceId,
    pub coverage_start_game_day: u32,
    pub waiting_ends_game_day: u32,
    pub coverage_end_exclusive: u32,
    pub waiting_passed: bool,
    pub deductible_krw: i64,
    pub occurrence_limit_krw: i64,
    pub term_limit_krw: i64,
    pub paid_krw: i64,
    pub reserved_krw: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct InsuranceClaimCandidateInput<'a> {
    pub claim_id: ResourceId,
    pub event_instance_id: ResourceId,
    pub offered_game_day: u32,
    pub matching_contracts: &'a [InsuranceClaimContractPin],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceClaimCandidatePlan {
    pub claim_id: ResourceId,
    pub event_instance_id: ResourceId,
    pub offered_game_day: u32,
    pub status: InsuranceClaimStatus,
    pub contract_set_digest: String,
    pub contract_pins: Vec<InsuranceClaimContractPin>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsuranceClaimStatus {
    Candidate,
    NotApplicable,
    NotCovered,
    Ready,
    Paid,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsuranceClaimResolutionKind {
    NoEffect,
    FixedWalletExpense,
}

#[derive(Debug, Clone, Copy)]
pub struct InsuranceClaimResolutionInput<'a> {
    pub claim_id: ResourceId,
    pub current_status: InsuranceClaimStatus,
    pub resolved_game_day: u32,
    pub resolution_kind: InsuranceClaimResolutionKind,
    pub gross_cost_krw: Option<i64>,
    pub claim_window_game_days: u16,
    pub contract_pins: &'a [InsuranceClaimContractPin],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceClaimAllocation {
    pub contract_id: ResourceId,
    pub deductible_krw: i64,
    pub raw_krw: i64,
    pub allocation_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceClaimContractAggregatePlan {
    pub contract_id: ResourceId,
    pub paid_before_krw: i64,
    pub paid_after_krw: i64,
    pub reserved_before_krw: i64,
    pub reserved_after_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceClaimResolutionPlan {
    pub claim_id: ResourceId,
    pub status: InsuranceClaimStatus,
    pub resolved_game_day: u32,
    pub gross_cost_krw: Option<i64>,
    pub payout_krw: i64,
    pub filing_deadline_game_day: Option<u32>,
    pub allocations: Vec<InsuranceClaimAllocation>,
    pub contract_aggregates: Vec<InsuranceClaimContractAggregatePlan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InsuranceClaimFinalizationContractInput {
    pub contract_id: ResourceId,
    pub allocation_krw: i64,
    pub paid_krw: i64,
    pub reserved_krw: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct InsuranceClaimPaymentInput<'a> {
    pub claim_id: ResourceId,
    pub current_status: InsuranceClaimStatus,
    pub current_game_day: u32,
    pub filing_deadline_game_day: u32,
    pub contracts: &'a [InsuranceClaimFinalizationContractInput],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceClaimPaymentPlan {
    pub claim_id: ResourceId,
    pub status: InsuranceClaimStatus,
    pub paid_game_day: u32,
    pub payout_krw: i64,
    pub contract_aggregates: Vec<InsuranceClaimContractAggregatePlan>,
}

#[derive(Debug, Clone, Copy)]
pub struct InsuranceClaimExpiryInput<'a> {
    pub claim_id: ResourceId,
    pub current_status: InsuranceClaimStatus,
    pub current_game_day: u32,
    pub filing_deadline_game_day: u32,
    pub contracts: &'a [InsuranceClaimFinalizationContractInput],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceClaimExpiryPlan {
    pub claim_id: ResourceId,
    pub status: InsuranceClaimStatus,
    pub expired_game_day: u32,
    pub released_reservation_krw: i64,
    pub contract_aggregates: Vec<InsuranceClaimContractAggregatePlan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsuranceLedgerAccountCode {
    Wallet,
    InsurancePremiumExpense,
    InsuranceClaimRecovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceLedgerPosting {
    pub account_code: InsuranceLedgerAccountCode,
    pub amount_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceLedgerPlan {
    pub wallet_cash_before_krw: i64,
    pub wallet_cash_after_krw: i64,
    pub wallet_delta_krw: i64,
    pub postings: Vec<InsuranceLedgerPosting>,
}

#[derive(Debug, Clone, Copy)]
pub struct InsurancePremiumLedgerInput {
    pub wallet_cash_krw: i64,
    pub premium_krw: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct InsuranceClaimLedgerInput {
    pub wallet_cash_krw: i64,
    pub payout_krw: i64,
}

pub trait InsuranceRules: Send + Sync + 'static {
    fn fact_registry(&self) -> &InsuranceFactRegistry;

    fn validate_catalog(&self, catalog: &InsuranceCatalog) -> Result<(), InsuranceError>;

    fn evaluate_eligibility(
        &self,
        input: InsuranceEligibilityInput<'_>,
    ) -> Result<InsuranceEligibilityEvaluation, InsuranceError>;

    fn plan_contract(
        &self,
        input: InsuranceContractPlanInput<'_>,
    ) -> Result<InsuranceContractPlan, InsuranceError>;

    fn resolve_premium(
        &self,
        input: InsurancePremiumResolutionInput,
    ) -> Result<InsurancePremiumResolution, InsuranceError>;

    fn is_event_covered(&self, input: InsuranceCoverageInput) -> Result<bool, InsuranceError>;

    fn terminate_contract(
        &self,
        input: InsuranceTerminationInput,
    ) -> Result<InsuranceTerminationPlan, InsuranceError>;

    fn expire_contract(
        &self,
        input: InsuranceContractExpiryInput,
    ) -> Result<InsuranceContractExpiryPlan, InsuranceError>;

    fn plan_claim_candidate(
        &self,
        input: InsuranceClaimCandidateInput<'_>,
    ) -> Result<InsuranceClaimCandidatePlan, InsuranceError>;

    fn resolve_claim(
        &self,
        input: InsuranceClaimResolutionInput<'_>,
    ) -> Result<InsuranceClaimResolutionPlan, InsuranceError>;

    fn pay_claim(
        &self,
        input: InsuranceClaimPaymentInput<'_>,
    ) -> Result<InsuranceClaimPaymentPlan, InsuranceError>;

    fn expire_claim(
        &self,
        input: InsuranceClaimExpiryInput<'_>,
    ) -> Result<InsuranceClaimExpiryPlan, InsuranceError>;

    fn plan_premium_ledger(
        &self,
        input: InsurancePremiumLedgerInput,
    ) -> Result<InsuranceLedgerPlan, InsuranceError>;

    fn plan_claim_ledger(
        &self,
        input: InsuranceClaimLedgerInput,
    ) -> Result<InsuranceLedgerPlan, InsuranceError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsuranceError {
    UnsupportedSchemaVersion,
    InvalidCatalog,
    InvalidComponentVersionKey,
    InvalidFactRegistry,
    DuplicateFact,
    DuplicateProduct,
    DuplicateCoverage,
    InvalidProductOrder,
    InvalidCoverageOrder,
    InvalidCanonicalKey,
    InvalidDisplayName,
    InvalidProductTerms,
    InvalidCoverage,
    InvalidLogicalArity,
    AstTooDeep,
    AstTooLarge,
    AstProjectionMismatch,
    UnknownFact,
    UnknownEnum,
    InvalidLiteral,
    TypeMismatch,
    UnitMismatch,
    UnorderedType,
    InvalidBetweenBounds,
    InvalidEligibilityRoot,
    InvalidEvidence,
    EligibilityIndeterminate,
    ProductNotFound,
    ContractLimitExceeded,
    InvalidContractState,
    ContractNotExpired,
    InvalidPremiumCharge,
    InvalidCoverageWindow,
    InsufficientWalletCash,
    ClaimContractLimitExceeded,
    DuplicateContract,
    InvalidClaimPin,
    InvalidClaimTransition,
    ClaimExpired,
    ClaimNotExpired,
    InvalidClaimAmount,
    InvalidTermUsage,
    ArithmeticOverflow,
    HashUnavailable,
    UnbalancedLedgerPlan,
    Conflict,
}

impl Display for InsuranceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "insurance error: {self:?}")
    }
}

impl Error for InsuranceError {}
