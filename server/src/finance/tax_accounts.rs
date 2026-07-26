use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use time::Date;
use utoipa::ToSchema;

const RATE_SCALE_PPM: i128 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaxAccountError {
    InvalidMoney,
    InvalidRate,
    InvalidDateRange,
    InvalidPensionReceiptYear,
    MissingPensionReceiptYear,
    PensionReceiptNotEligible,
    WithdrawalExceedsBalance,
    InvalidIrpState,
    InvalidPolicy,
    ArithmeticOverflow,
}

impl Display for TaxAccountError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidMoney => "tax-account money must satisfy its non-negative constraints",
            Self::InvalidRate => "tax-account rate is invalid",
            Self::InvalidDateRange => "tax-account date range is invalid",
            Self::InvalidPensionReceiptYear => "pension receipt year must be positive",
            Self::MissingPensionReceiptYear => {
                "pension receipt year is required for deferred retirement income"
            }
            Self::PensionReceiptNotEligible => "regular pension receipt requirements are not met",
            Self::WithdrawalExceedsBalance => "pension withdrawal exceeds tax-layer balances",
            Self::InvalidIrpState => "IRP reserve values violate their invariants",
            Self::InvalidPolicy => "tax-account policy parameters are invalid",
            Self::ArithmeticOverflow => "tax-account arithmetic overflowed",
        };
        formatter.write_str(message)
    }
}

impl Error for TaxAccountError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IsaPolicy {
    pub minimum_age: u32,
    pub working_income_minimum_age: u32,
    pub comprehensive_tax_lookback_years: u32,
    pub annual_contribution_limit_krw: i64,
    pub total_contribution_limit_krw: i64,
    pub maximum_contribution_years: u32,
    pub minimum_term_years: u32,
    pub low_income_total_salary_limit_krw: i64,
    pub low_income_comprehensive_income_limit_krw: i64,
    pub general_tax_free_limit_krw: i64,
    pub low_income_tax_free_limit_krw: i64,
    pub separate_income_tax_ppm: i64,
    pub separate_local_income_tax_ppm: i64,
}

impl Default for IsaPolicy {
    fn default() -> Self {
        Self {
            minimum_age: 19,
            working_income_minimum_age: 15,
            comprehensive_tax_lookback_years: 3,
            annual_contribution_limit_krw: 20_000_000,
            total_contribution_limit_krw: 100_000_000,
            maximum_contribution_years: 5,
            minimum_term_years: 3,
            low_income_total_salary_limit_krw: 50_000_000,
            low_income_comprehensive_income_limit_krw: 38_000_000,
            general_tax_free_limit_krw: 2_000_000,
            low_income_tax_free_limit_krw: 4_000_000,
            separate_income_tax_ppm: 90_000,
            separate_local_income_tax_ppm: 9_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PensionPolicy {
    pub pension_savings_credit_limit_krw: i64,
    pub combined_credit_limit_krw: i64,
    pub salary_high_credit_boundary_krw: i64,
    pub comprehensive_income_high_credit_boundary_krw: i64,
    pub high_income_tax_credit_rate_ppm: i64,
    pub high_local_income_tax_credit_rate_ppm: i64,
    pub standard_income_tax_credit_rate_ppm: i64,
    pub standard_local_income_tax_credit_rate_ppm: i64,
    pub minimum_pension_age: u32,
    pub minimum_enrollment_years: u32,
    pub irp_risk_asset_limit_ppm: i64,
    pub under_age70_pension_tax_ppm: i64,
    pub under_age80_pension_tax_ppm: i64,
    pub age80_or_older_pension_tax_ppm: i64,
    pub lifetime_pension_tax_ppm: i64,
    pub non_pension_withdrawal_tax_ppm: i64,
    pub pension_receipt_limit_rate_ppm: i64,
    pub limited_receipt_years: u32,
    pub deferred_retirement_first10_years_ppm: i64,
    pub deferred_retirement_years11_to20_ppm: i64,
    pub deferred_retirement_after20_years_ppm: i64,
}

impl Default for PensionPolicy {
    fn default() -> Self {
        Self {
            pension_savings_credit_limit_krw: 6_000_000,
            combined_credit_limit_krw: 9_000_000,
            salary_high_credit_boundary_krw: 55_000_000,
            comprehensive_income_high_credit_boundary_krw: 45_000_000,
            high_income_tax_credit_rate_ppm: 150_000,
            high_local_income_tax_credit_rate_ppm: 15_000,
            standard_income_tax_credit_rate_ppm: 120_000,
            standard_local_income_tax_credit_rate_ppm: 12_000,
            minimum_pension_age: 55,
            minimum_enrollment_years: 5,
            irp_risk_asset_limit_ppm: 700_000,
            under_age70_pension_tax_ppm: 55_000,
            under_age80_pension_tax_ppm: 44_000,
            age80_or_older_pension_tax_ppm: 33_000,
            lifetime_pension_tax_ppm: 33_000,
            non_pension_withdrawal_tax_ppm: 165_000,
            pension_receipt_limit_rate_ppm: 1_200_000,
            limited_receipt_years: 10,
            deferred_retirement_first10_years_ppm: 700_000,
            deferred_retirement_years11_to20_ppm: 600_000,
            deferred_retirement_after20_years_ppm: 500_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeneralFinancialIncomePolicy {
    pub income_tax_ppm: i64,
    pub local_income_tax_ppm: i64,
    pub comprehensive_threshold_krw: i64,
}

impl Default for GeneralFinancialIncomePolicy {
    fn default() -> Self {
        Self {
            income_tax_ppm: 140_000,
            local_income_tax_ppm: 14_000,
            comprehensive_threshold_krw: 20_000_000,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaxAccountPolicy {
    pub isa: IsaPolicy,
    pub pension: PensionPolicy,
    pub general_financial_income: GeneralFinancialIncomePolicy,
}

impl TaxAccountPolicy {
    pub fn validate(&self) -> Result<(), TaxAccountError> {
        let isa = &self.isa;
        let pension = &self.pension;
        let general = &self.general_financial_income;
        let valid_isa = isa.minimum_age <= 150
            && isa.working_income_minimum_age <= isa.minimum_age
            && isa.comprehensive_tax_lookback_years == 3
            && isa.annual_contribution_limit_krw > 0
            && (1..=100).contains(&isa.maximum_contribution_years)
            && i128::from(isa.annual_contribution_limit_krw)
                .checked_mul(i128::from(isa.maximum_contribution_years))
                == Some(i128::from(isa.total_contribution_limit_krw))
            && (1..=100).contains(&isa.minimum_term_years)
            && isa.low_income_total_salary_limit_krw >= 0
            && isa.low_income_comprehensive_income_limit_krw >= 0
            && isa.general_tax_free_limit_krw >= 0
            && isa.low_income_tax_free_limit_krw >= isa.general_tax_free_limit_krw
            && valid_policy_rate(isa.separate_income_tax_ppm)
            && valid_policy_rate(isa.separate_local_income_tax_ppm)
            && isa
                .separate_income_tax_ppm
                .checked_add(isa.separate_local_income_tax_ppm)
                .is_some_and(valid_policy_rate);
        let valid_credit_rates = [
            pension.high_income_tax_credit_rate_ppm,
            pension.high_local_income_tax_credit_rate_ppm,
            pension.standard_income_tax_credit_rate_ppm,
            pension.standard_local_income_tax_credit_rate_ppm,
        ]
        .into_iter()
        .all(valid_policy_rate);
        let valid_pension = pension.pension_savings_credit_limit_krw > 0
            && pension.combined_credit_limit_krw >= pension.pension_savings_credit_limit_krw
            && pension.salary_high_credit_boundary_krw >= 0
            && pension.comprehensive_income_high_credit_boundary_krw >= 0
            && valid_credit_rates
            && (1..=150).contains(&pension.minimum_pension_age)
            && (1..=100).contains(&pension.minimum_enrollment_years)
            && [
                pension.irp_risk_asset_limit_ppm,
                pension.under_age70_pension_tax_ppm,
                pension.under_age80_pension_tax_ppm,
                pension.age80_or_older_pension_tax_ppm,
                pension.lifetime_pension_tax_ppm,
                pension.non_pension_withdrawal_tax_ppm,
                pension.deferred_retirement_first10_years_ppm,
                pension.deferred_retirement_years11_to20_ppm,
                pension.deferred_retirement_after20_years_ppm,
            ]
            .into_iter()
            .all(valid_policy_rate)
            && (1..=100).contains(&pension.limited_receipt_years)
            && pension.pension_receipt_limit_rate_ppm > 0
            && pension.pension_receipt_limit_rate_ppm <= 10_000_000
            && pension
                .high_income_tax_credit_rate_ppm
                .checked_add(pension.high_local_income_tax_credit_rate_ppm)
                .is_some_and(valid_policy_rate)
            && pension
                .standard_income_tax_credit_rate_ppm
                .checked_add(pension.standard_local_income_tax_credit_rate_ppm)
                .is_some_and(valid_policy_rate)
            && pension.under_age70_pension_tax_ppm >= pension.under_age80_pension_tax_ppm
            && pension.under_age80_pension_tax_ppm >= pension.age80_or_older_pension_tax_ppm
            && pension.deferred_retirement_first10_years_ppm
                >= pension.deferred_retirement_years11_to20_ppm
            && pension.deferred_retirement_years11_to20_ppm
                >= pension.deferred_retirement_after20_years_ppm;
        let valid_general = general.comprehensive_threshold_krw >= 0
            && valid_policy_rate(general.income_tax_ppm)
            && valid_policy_rate(general.local_income_tax_ppm)
            && general
                .income_tax_ppm
                .checked_add(general.local_income_tax_ppm)
                .is_some_and(valid_policy_rate);
        if valid_isa && valid_pension && valid_general {
            Ok(())
        } else {
            Err(TaxAccountError::InvalidPolicy)
        }
    }
}

fn valid_policy_rate(rate_ppm: i64) -> bool {
    rate_ppm >= 0 && i128::from(rate_ppm) <= RATE_SCALE_PPM
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsaAccountKind {
    General,
    LowIncome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsaPriorIncomeComposition {
    WageOnlyOrComprehensiveTaxExcluded,
    IncludesOtherComprehensiveIncome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IsaPriorTaxYearIncome {
    pub taxable_wage_income_krw: i64,
    pub total_salary_krw: i64,
    pub comprehensive_income_krw: i64,
    pub composition: IsaPriorIncomeComposition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IsaEnrollmentInput {
    pub requested_kind: IsaAccountKind,
    pub age_years: u32,
    pub prior_tax_year_income: Option<IsaPriorTaxYearIncome>,
    pub previous_three_tax_years_financial_income_taxed: Option<[bool; 3]>,
    pub has_open_isa: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsaIneligibilityReason {
    ExistingAccount,
    MissingTaxYearRecord,
    AgeOrWageIncome,
    FinancialIncomeComprehensiveTaxationHistory,
    LowIncomeCriteria,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsaEligibility {
    Eligible,
    Ineligible(IsaIneligibilityReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IsaContributionRoomInput {
    pub opened_on: Date,
    pub current_on: Date,
    pub cumulative_contribution_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IsaContributionRoom {
    pub completed_years: u32,
    pub carried_annual_capacity_krw: i64,
    pub lifetime_remaining_krw: i64,
    pub available_contribution_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IsaCloseTaxInput {
    pub account_kind: IsaAccountKind,
    pub opened_on: Date,
    pub closed_on: Date,
    pub isa_tax_profit_krw: i64,
    pub isa_deductible_loss_krw: i64,
    pub statutory_unavoidable_reason: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsaTaxTreatment {
    GeneralTaxation,
    IsaSeparateTaxation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IsaCloseTaxResult {
    pub treatment: IsaTaxTreatment,
    pub gross_tax_profit_krw: i64,
    pub deductible_loss_krw: i64,
    pub net_tax_profit_krw: i64,
    pub exempt_profit_krw: i64,
    pub taxable_profit_krw: i64,
    pub income_tax_krw: i64,
    pub local_income_tax_krw: i64,
    pub gross_financial_income_delta_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PensionCreditIncome {
    WageOnly { total_salary_krw: i64 },
    Other { comprehensive_income_krw: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PensionCreditInput {
    pub pension_savings_contribution_krw: i64,
    pub irp_contribution_krw: i64,
    pub income: PensionCreditIncome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PensionCreditResult {
    pub pension_savings_eligible_krw: i64,
    pub irp_eligible_krw: i64,
    pub total_eligible_krw: i64,
    pub income_tax_credit_krw: i64,
    pub local_income_tax_credit_krw: i64,
    pub expected_credit_krw: i64,
    pub expected_credit_rate_ppm: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PensionTaxLayers {
    pub tax_excluded_contribution_krw: i64,
    pub deferred_retirement_income_krw: i64,
    pub credited_contribution_krw: i64,
    pub earnings_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PensionReceiptLimitInput {
    pub pension_receipt_year: u32,
    pub tax_period_opening_value_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PensionReceiptLimit {
    Limited { annual_limit_krw: i64 },
    Unlimited,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PensionReceiptEligibilityInput {
    pub holder_age_years: u32,
    pub pension_started: bool,
    pub opened_on: Date,
    pub current_on: Date,
    pub has_deferred_retirement_income: bool,
    pub pension_receipt_year: u32,
    pub tax_period_opening_value_krw: i64,
    pub pension_withdrawn_before_request_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PensionReceiptIneligibilityReason {
    UnderMinimumAge,
    NotStarted,
    MinimumHoldingPeriod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PensionReceiptEligibility {
    Eligible {
        annual_limit_krw: Option<i64>,
        remaining_limit_krw: Option<i64>,
    },
    Ineligible(PensionReceiptIneligibilityReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum PensionWithdrawalRequestKind {
    #[serde(rename = "pension")]
    RegularPension,
    #[serde(rename = "unavoidable")]
    StatutoryUnavoidable,
    #[serde(rename = "nonPension")]
    ExplicitNonPension,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PensionWithdrawalPlanInput {
    pub layers: PensionTaxLayers,
    pub requested_amount_krw: i64,
    pub request_kind: PensionWithdrawalRequestKind,
    pub holder_age_years: u32,
    pub pension_started: bool,
    pub opened_on: Date,
    pub current_on: Date,
    pub pension_receipt_year: Option<u32>,
    pub tax_period_opening_value_krw: i64,
    pub pension_withdrawn_before_request_krw: i64,
    pub lifetime_contract: bool,
    pub deferred_retirement_non_pension_tax_rate_ppm: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PensionWithdrawalTreatment {
    Pension,
    PensionUnavoidable,
    NonPension,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PensionTaxSource {
    TaxExcludedContribution,
    DeferredRetirementIncome,
    CreditedContribution,
    Earnings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PensionTaxRate {
    Exempt,
    FixedPpm(i64),
    DeferredRetirementPension {
        non_pension_rate_ppm: i64,
        pension_factor_ppm: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PensionWithdrawalTaxLine {
    pub source: PensionTaxSource,
    pub gross_amount_krw: i64,
    pub tax_rate: PensionTaxRate,
    pub tax_krw: i64,
    pub net_amount_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PensionWithdrawalPortion {
    pub treatment: PensionWithdrawalTreatment,
    pub gross_amount_krw: i64,
    pub tax_free_amount_krw: i64,
    pub tax_krw: i64,
    pub net_amount_krw: i64,
    pub tax_lines: [PensionWithdrawalTaxLine; 4],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PensionWithdrawalPlan {
    pub gross_amount_krw: i64,
    pub pension_amount_krw: i64,
    pub non_pension_amount_krw: i64,
    pub tax_free_amount_krw: i64,
    pub tax_krw: i64,
    pub net_payout_krw: i64,
    pub portions: Vec<PensionWithdrawalPortion>,
    pub remaining_layers: PensionTaxLayers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrpInvestmentKind {
    Cash,
    Deposit,
    TreasuryBond,
    Llx,
    KrxPhysicalGold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrpRiskOrderInput {
    pub total_reserve_value_krw: i64,
    pub risk_asset_value_krw: i64,
    pub purchase_amount_krw: i64,
    pub investment_kind: IrpInvestmentKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrpRiskOrderRejection {
    InvestmentNotPermitted,
    RiskLimitExceeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrpRiskOrderDecision {
    Allowed {
        post_order_risk_asset_value_krw: i64,
        post_order_total_value_krw: i64,
        post_order_risk_ratio_ppm: i64,
    },
    Rejected {
        reason: IrpRiskOrderRejection,
        post_order_risk_asset_value_krw: i64,
        post_order_total_value_krw: i64,
        post_order_risk_ratio_ppm: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum IrpWithdrawalReason {
    HomePurchase,
    HousingDeposit,
    MedicalCare,
    Disaster,
    Bankruptcy,
    Rehabilitation,
    SecuredLoanRepayment,
}

pub trait TaxAccountRules: Send + Sync + 'static {
    fn isa_minimum_term_years(&self) -> u32;

    fn pension_minimum_enrollment_years(&self) -> u32;

    fn minimum_pension_age(&self) -> u32;

    fn isa_enrollment_eligibility(
        &self,
        input: IsaEnrollmentInput,
    ) -> Result<IsaEligibility, TaxAccountError>;

    fn isa_contribution_room(
        &self,
        input: IsaContributionRoomInput,
    ) -> Result<IsaContributionRoom, TaxAccountError>;

    fn isa_close_tax(&self, input: IsaCloseTaxInput) -> Result<IsaCloseTaxResult, TaxAccountError>;

    fn pension_credit(
        &self,
        input: PensionCreditInput,
    ) -> Result<PensionCreditResult, TaxAccountError>;

    fn pension_receipt_limit(
        &self,
        input: PensionReceiptLimitInput,
    ) -> Result<PensionReceiptLimit, TaxAccountError>;

    fn pension_receipt_eligibility(
        &self,
        input: PensionReceiptEligibilityInput,
    ) -> Result<PensionReceiptEligibility, TaxAccountError>;

    fn plan_pension_withdrawal(
        &self,
        input: PensionWithdrawalPlanInput,
    ) -> Result<PensionWithdrawalPlan, TaxAccountError>;

    fn evaluate_irp_risk_order(
        &self,
        input: IrpRiskOrderInput,
    ) -> Result<IrpRiskOrderDecision, TaxAccountError>;
}

#[derive(Debug)]
struct V1TaxAccountRules {
    policy: TaxAccountPolicy,
}

#[cfg(test)]
pub fn create_tax_account_rules() -> Arc<dyn TaxAccountRules> {
    Arc::new(V1TaxAccountRules {
        policy: TaxAccountPolicy::default(),
    })
}

pub fn create_tax_account_rules_with_policy(
    policy: TaxAccountPolicy,
) -> Result<Arc<dyn TaxAccountRules>, TaxAccountError> {
    policy.validate()?;
    Ok(Arc::new(V1TaxAccountRules { policy }))
}

pub fn completed_calendar_years(opened_on: Date, current_on: Date) -> Result<u32, TaxAccountError> {
    if current_on < opened_on {
        return Err(TaxAccountError::InvalidDateRange);
    }

    let year_difference = current_on
        .year()
        .checked_sub(opened_on.year())
        .ok_or(TaxAccountError::ArithmeticOverflow)?;
    let year_difference =
        u32::try_from(year_difference).map_err(|_| TaxAccountError::ArithmeticOverflow)?;
    let anniversary = add_years_clamped(opened_on, year_difference)?;
    if current_on >= anniversary {
        Ok(year_difference)
    } else {
        year_difference
            .checked_sub(1)
            .ok_or(TaxAccountError::ArithmeticOverflow)
    }
}

pub fn current_age_years(
    starting_age_years: u32,
    world_start_date: Date,
    current_date: Date,
) -> Result<u32, TaxAccountError> {
    starting_age_years
        .checked_add(completed_calendar_years(world_start_date, current_date)?)
        .ok_or(TaxAccountError::ArithmeticOverflow)
}

pub fn anniversary_game_day(
    world_start_date: Date,
    opened_on: Date,
    years: u32,
) -> Result<u32, TaxAccountError> {
    let anniversary = add_years_clamped(opened_on, years)?;
    u32::try_from((anniversary - world_start_date).whole_days())
        .map_err(|_| TaxAccountError::InvalidDateRange)
}

impl TaxAccountRules for V1TaxAccountRules {
    fn isa_minimum_term_years(&self) -> u32 {
        self.policy.isa.minimum_term_years
    }

    fn pension_minimum_enrollment_years(&self) -> u32 {
        self.policy.pension.minimum_enrollment_years
    }

    fn minimum_pension_age(&self) -> u32 {
        self.policy.pension.minimum_pension_age
    }

    fn isa_enrollment_eligibility(
        &self,
        input: IsaEnrollmentInput,
    ) -> Result<IsaEligibility, TaxAccountError> {
        if input.has_open_isa {
            return Ok(IsaEligibility::Ineligible(
                IsaIneligibilityReason::ExistingAccount,
            ));
        }

        let Some(financial_income_history) = input.previous_three_tax_years_financial_income_taxed
        else {
            return Ok(IsaEligibility::Ineligible(
                IsaIneligibilityReason::MissingTaxYearRecord,
            ));
        };
        if financial_income_history.contains(&true) {
            return Ok(IsaEligibility::Ineligible(
                IsaIneligibilityReason::FinancialIncomeComprehensiveTaxationHistory,
            ));
        }

        if let Some(income) = input.prior_tax_year_income {
            validate_isa_prior_income(income)?;
        }

        let age_eligible = input.age_years >= self.policy.isa.minimum_age
            || (input.age_years >= self.policy.isa.working_income_minimum_age
                && input
                    .prior_tax_year_income
                    .is_some_and(|income| income.taxable_wage_income_krw > 0));
        if !age_eligible {
            let reason = if input.age_years >= self.policy.isa.working_income_minimum_age
                && input.prior_tax_year_income.is_none()
            {
                IsaIneligibilityReason::MissingTaxYearRecord
            } else {
                IsaIneligibilityReason::AgeOrWageIncome
            };
            return Ok(IsaEligibility::Ineligible(reason));
        }

        if input.requested_kind == IsaAccountKind::LowIncome {
            let Some(income) = input.prior_tax_year_income else {
                return Ok(IsaEligibility::Ineligible(
                    IsaIneligibilityReason::MissingTaxYearRecord,
                ));
            };
            let income_composition_eligible =
                income.composition == IsaPriorIncomeComposition::WageOnlyOrComprehensiveTaxExcluded;
            let low_income_eligible = income.total_salary_krw
                <= self.policy.isa.low_income_total_salary_limit_krw
                && (income.comprehensive_income_krw
                    <= self.policy.isa.low_income_comprehensive_income_limit_krw
                    || income_composition_eligible);
            if !low_income_eligible {
                return Ok(IsaEligibility::Ineligible(
                    IsaIneligibilityReason::LowIncomeCriteria,
                ));
            }
        }

        Ok(IsaEligibility::Eligible)
    }

    fn isa_contribution_room(
        &self,
        input: IsaContributionRoomInput,
    ) -> Result<IsaContributionRoom, TaxAccountError> {
        validate_non_negative(input.cumulative_contribution_krw)?;
        let completed_years = completed_calendar_years(input.opened_on, input.current_on)?;
        let maximum_completed_years = self
            .policy
            .isa
            .maximum_contribution_years
            .checked_sub(1)
            .ok_or(TaxAccountError::InvalidPolicy)?;
        let carry_years = 1_u32
            .checked_add(completed_years.min(maximum_completed_years))
            .ok_or(TaxAccountError::ArithmeticOverflow)?;
        let carried_annual_capacity_krw = checked_i128_to_i64(
            i128::from(self.policy.isa.annual_contribution_limit_krw)
                .checked_mul(i128::from(carry_years))
                .ok_or(TaxAccountError::ArithmeticOverflow)?,
        )?;
        let lifetime_remaining_krw = self
            .policy
            .isa
            .total_contribution_limit_krw
            .checked_sub(input.cumulative_contribution_krw)
            .ok_or(TaxAccountError::ArithmeticOverflow)?
            .max(0);
        let carried_remaining_krw = carried_annual_capacity_krw
            .checked_sub(input.cumulative_contribution_krw)
            .ok_or(TaxAccountError::ArithmeticOverflow)?
            .max(0);

        Ok(IsaContributionRoom {
            completed_years,
            carried_annual_capacity_krw,
            lifetime_remaining_krw,
            available_contribution_krw: lifetime_remaining_krw.min(carried_remaining_krw),
        })
    }

    fn isa_close_tax(&self, input: IsaCloseTaxInput) -> Result<IsaCloseTaxResult, TaxAccountError> {
        validate_non_negative(input.isa_tax_profit_krw)?;
        validate_non_negative(input.isa_deductible_loss_krw)?;
        let completed_years = completed_calendar_years(input.opened_on, input.closed_on)?;
        let preferred_treatment = completed_years >= self.policy.isa.minimum_term_years
            || input.statutory_unavoidable_reason;
        let net_tax_profit_krw = input
            .isa_tax_profit_krw
            .checked_sub(input.isa_deductible_loss_krw)
            .ok_or(TaxAccountError::ArithmeticOverflow)?
            .max(0);

        if !preferred_treatment {
            let taxable_profit_krw = input.isa_tax_profit_krw;
            return Ok(IsaCloseTaxResult {
                treatment: IsaTaxTreatment::GeneralTaxation,
                gross_tax_profit_krw: input.isa_tax_profit_krw,
                deductible_loss_krw: input.isa_deductible_loss_krw,
                net_tax_profit_krw,
                exempt_profit_krw: 0,
                taxable_profit_krw,
                income_tax_krw: floor_rate(
                    taxable_profit_krw,
                    self.policy.general_financial_income.income_tax_ppm,
                )?,
                local_income_tax_krw: floor_rate(
                    taxable_profit_krw,
                    self.policy.general_financial_income.local_income_tax_ppm,
                )?,
                gross_financial_income_delta_krw: taxable_profit_krw,
            });
        }

        let exemption_limit_krw = match input.account_kind {
            IsaAccountKind::General => self.policy.isa.general_tax_free_limit_krw,
            IsaAccountKind::LowIncome => self.policy.isa.low_income_tax_free_limit_krw,
        };
        let exempt_profit_krw = net_tax_profit_krw.min(exemption_limit_krw);
        let taxable_profit_krw = net_tax_profit_krw
            .checked_sub(exempt_profit_krw)
            .ok_or(TaxAccountError::ArithmeticOverflow)?;
        Ok(IsaCloseTaxResult {
            treatment: IsaTaxTreatment::IsaSeparateTaxation,
            gross_tax_profit_krw: input.isa_tax_profit_krw,
            deductible_loss_krw: input.isa_deductible_loss_krw,
            net_tax_profit_krw,
            exempt_profit_krw,
            taxable_profit_krw,
            income_tax_krw: floor_rate(
                taxable_profit_krw,
                self.policy.isa.separate_income_tax_ppm,
            )?,
            local_income_tax_krw: floor_rate(
                taxable_profit_krw,
                self.policy.isa.separate_local_income_tax_ppm,
            )?,
            gross_financial_income_delta_krw: 0,
        })
    }

    fn pension_credit(
        &self,
        input: PensionCreditInput,
    ) -> Result<PensionCreditResult, TaxAccountError> {
        validate_non_negative(input.pension_savings_contribution_krw)?;
        validate_non_negative(input.irp_contribution_krw)?;
        let low_income_rate = match input.income {
            PensionCreditIncome::WageOnly { total_salary_krw } => {
                validate_non_negative(total_salary_krw)?;
                total_salary_krw <= self.policy.pension.salary_high_credit_boundary_krw
            }
            PensionCreditIncome::Other {
                comprehensive_income_krw,
            } => {
                validate_non_negative(comprehensive_income_krw)?;
                comprehensive_income_krw
                    <= self
                        .policy
                        .pension
                        .comprehensive_income_high_credit_boundary_krw
            }
        };

        let pension_savings_eligible_krw = input
            .pension_savings_contribution_krw
            .min(self.policy.pension.pension_savings_credit_limit_krw);
        let combined_remaining_krw = self
            .policy
            .pension
            .combined_credit_limit_krw
            .checked_sub(pension_savings_eligible_krw)
            .ok_or(TaxAccountError::ArithmeticOverflow)?;
        let irp_eligible_krw = input.irp_contribution_krw.min(combined_remaining_krw);
        let total_eligible_krw = pension_savings_eligible_krw
            .checked_add(irp_eligible_krw)
            .ok_or(TaxAccountError::ArithmeticOverflow)?;
        let (income_tax_rate_ppm, local_income_tax_rate_ppm) = if low_income_rate {
            (
                self.policy.pension.high_income_tax_credit_rate_ppm,
                self.policy.pension.high_local_income_tax_credit_rate_ppm,
            )
        } else {
            (
                self.policy.pension.standard_income_tax_credit_rate_ppm,
                self.policy
                    .pension
                    .standard_local_income_tax_credit_rate_ppm,
            )
        };
        let expected_credit_rate_ppm = income_tax_rate_ppm
            .checked_add(local_income_tax_rate_ppm)
            .ok_or(TaxAccountError::ArithmeticOverflow)?;
        let income_tax_credit_krw = floor_rate(total_eligible_krw, income_tax_rate_ppm)?;
        let local_income_tax_credit_krw =
            floor_rate(total_eligible_krw, local_income_tax_rate_ppm)?;
        let expected_credit_krw = income_tax_credit_krw
            .checked_add(local_income_tax_credit_krw)
            .ok_or(TaxAccountError::ArithmeticOverflow)?;

        Ok(PensionCreditResult {
            pension_savings_eligible_krw,
            irp_eligible_krw,
            total_eligible_krw,
            income_tax_credit_krw,
            local_income_tax_credit_krw,
            expected_credit_krw,
            expected_credit_rate_ppm,
        })
    }

    fn pension_receipt_limit(
        &self,
        input: PensionReceiptLimitInput,
    ) -> Result<PensionReceiptLimit, TaxAccountError> {
        validate_non_negative(input.tax_period_opening_value_krw)?;
        if input.pension_receipt_year == 0 {
            return Err(TaxAccountError::InvalidPensionReceiptYear);
        }
        if input.pension_receipt_year > self.policy.pension.limited_receipt_years {
            return Ok(PensionReceiptLimit::Unlimited);
        }

        let denominator = self
            .policy
            .pension
            .limited_receipt_years
            .checked_add(1)
            .ok_or(TaxAccountError::ArithmeticOverflow)?
            .checked_sub(input.pension_receipt_year)
            .ok_or(TaxAccountError::ArithmeticOverflow)?;
        let numerator = i128::from(input.tax_period_opening_value_krw)
            .checked_mul(i128::from(
                self.policy.pension.pension_receipt_limit_rate_ppm,
            ))
            .ok_or(TaxAccountError::ArithmeticOverflow)?;
        let divisor = i128::from(denominator)
            .checked_mul(RATE_SCALE_PPM)
            .ok_or(TaxAccountError::ArithmeticOverflow)?;
        let annual_limit_krw = checked_i128_to_i64(
            numerator
                .checked_div(divisor)
                .ok_or(TaxAccountError::ArithmeticOverflow)?,
        )?;
        Ok(PensionReceiptLimit::Limited { annual_limit_krw })
    }

    fn pension_receipt_eligibility(
        &self,
        input: PensionReceiptEligibilityInput,
    ) -> Result<PensionReceiptEligibility, TaxAccountError> {
        validate_non_negative(input.tax_period_opening_value_krw)?;
        validate_non_negative(input.pension_withdrawn_before_request_krw)?;
        if input.holder_age_years < self.policy.pension.minimum_pension_age {
            return Ok(PensionReceiptEligibility::Ineligible(
                PensionReceiptIneligibilityReason::UnderMinimumAge,
            ));
        }
        if !input.pension_started {
            return Ok(PensionReceiptEligibility::Ineligible(
                PensionReceiptIneligibilityReason::NotStarted,
            ));
        }
        if !input.has_deferred_retirement_income
            && completed_calendar_years(input.opened_on, input.current_on)?
                < self.policy.pension.minimum_enrollment_years
        {
            return Ok(PensionReceiptEligibility::Ineligible(
                PensionReceiptIneligibilityReason::MinimumHoldingPeriod,
            ));
        }

        match self.pension_receipt_limit(PensionReceiptLimitInput {
            pension_receipt_year: input.pension_receipt_year,
            tax_period_opening_value_krw: input.tax_period_opening_value_krw,
        })? {
            PensionReceiptLimit::Limited { annual_limit_krw } => {
                let remaining_limit_krw = annual_limit_krw
                    .checked_sub(input.pension_withdrawn_before_request_krw)
                    .ok_or(TaxAccountError::ArithmeticOverflow)?
                    .max(0);
                Ok(PensionReceiptEligibility::Eligible {
                    annual_limit_krw: Some(annual_limit_krw),
                    remaining_limit_krw: Some(remaining_limit_krw),
                })
            }
            PensionReceiptLimit::Unlimited => Ok(PensionReceiptEligibility::Eligible {
                annual_limit_krw: None,
                remaining_limit_krw: None,
            }),
        }
    }

    fn plan_pension_withdrawal(
        &self,
        input: PensionWithdrawalPlanInput,
    ) -> Result<PensionWithdrawalPlan, TaxAccountError> {
        validate_layers(input.layers)?;
        if input.requested_amount_krw <= 0 {
            return Err(TaxAccountError::InvalidMoney);
        }
        validate_rate(input.deferred_retirement_non_pension_tax_rate_ppm)?;
        validate_non_negative(input.tax_period_opening_value_krw)?;
        validate_non_negative(input.pension_withdrawn_before_request_krw)?;
        if input.requested_amount_krw > layer_total(input.layers)? {
            return Err(TaxAccountError::WithdrawalExceedsBalance);
        }

        let (pension_amount_krw, non_pension_amount_krw, pension_treatment) = match input
            .request_kind
        {
            PensionWithdrawalRequestKind::RegularPension => {
                let pension_receipt_year = input
                    .pension_receipt_year
                    .ok_or(TaxAccountError::InvalidPensionReceiptYear)?;
                let eligibility =
                    self.pension_receipt_eligibility(PensionReceiptEligibilityInput {
                        holder_age_years: input.holder_age_years,
                        pension_started: input.pension_started,
                        opened_on: input.opened_on,
                        current_on: input.current_on,
                        has_deferred_retirement_income: input.layers.deferred_retirement_income_krw
                            > 0,
                        pension_receipt_year,
                        tax_period_opening_value_krw: input.tax_period_opening_value_krw,
                        pension_withdrawn_before_request_krw: input
                            .pension_withdrawn_before_request_krw,
                    })?;
                let remaining_limit_krw = match eligibility {
                    PensionReceiptEligibility::Eligible {
                        remaining_limit_krw,
                        ..
                    } => remaining_limit_krw,
                    PensionReceiptEligibility::Ineligible(_) => {
                        return Err(TaxAccountError::PensionReceiptNotEligible);
                    }
                };
                let pension_amount_krw = remaining_limit_krw
                    .map_or(input.requested_amount_krw, |remaining| {
                        input.requested_amount_krw.min(remaining)
                    });
                let non_pension_amount_krw = input
                    .requested_amount_krw
                    .checked_sub(pension_amount_krw)
                    .ok_or(TaxAccountError::ArithmeticOverflow)?;
                (
                    pension_amount_krw,
                    non_pension_amount_krw,
                    PensionWithdrawalTreatment::Pension,
                )
            }
            PensionWithdrawalRequestKind::StatutoryUnavoidable => (
                input.requested_amount_krw,
                0,
                PensionWithdrawalTreatment::PensionUnavoidable,
            ),
            PensionWithdrawalRequestKind::ExplicitNonPension => (
                0,
                input.requested_amount_krw,
                PensionWithdrawalTreatment::Pension,
            ),
        };

        let mut remaining_layers = input.layers;
        let mut portions = Vec::with_capacity(2);
        if pension_amount_krw > 0 {
            let allocations = withdraw_from_layers(&mut remaining_layers, pension_amount_krw)?;
            portions.push(build_withdrawal_portion(
                pension_treatment,
                allocations,
                input.holder_age_years,
                input.lifetime_contract,
                input.pension_receipt_year,
                input.deferred_retirement_non_pension_tax_rate_ppm,
                &self.policy.pension,
            )?);
        }
        if non_pension_amount_krw > 0 {
            let allocations = withdraw_from_layers(&mut remaining_layers, non_pension_amount_krw)?;
            portions.push(build_withdrawal_portion(
                PensionWithdrawalTreatment::NonPension,
                allocations,
                input.holder_age_years,
                input.lifetime_contract,
                input.pension_receipt_year,
                input.deferred_retirement_non_pension_tax_rate_ppm,
                &self.policy.pension,
            )?);
        }

        let mut tax_free_amount_krw = 0_i64;
        let mut tax_krw = 0_i64;
        let mut net_payout_krw = 0_i64;
        for portion in &portions {
            tax_free_amount_krw = tax_free_amount_krw
                .checked_add(portion.tax_free_amount_krw)
                .ok_or(TaxAccountError::ArithmeticOverflow)?;
            tax_krw = tax_krw
                .checked_add(portion.tax_krw)
                .ok_or(TaxAccountError::ArithmeticOverflow)?;
            net_payout_krw = net_payout_krw
                .checked_add(portion.net_amount_krw)
                .ok_or(TaxAccountError::ArithmeticOverflow)?;
        }

        Ok(PensionWithdrawalPlan {
            gross_amount_krw: input.requested_amount_krw,
            pension_amount_krw,
            non_pension_amount_krw,
            tax_free_amount_krw,
            tax_krw,
            net_payout_krw,
            portions,
            remaining_layers,
        })
    }

    fn evaluate_irp_risk_order(
        &self,
        input: IrpRiskOrderInput,
    ) -> Result<IrpRiskOrderDecision, TaxAccountError> {
        if input.total_reserve_value_krw < 0
            || input.risk_asset_value_krw < 0
            || input.purchase_amount_krw <= 0
            || input.risk_asset_value_krw > input.total_reserve_value_krw
        {
            return Err(TaxAccountError::InvalidIrpState);
        }

        if input.investment_kind == IrpInvestmentKind::KrxPhysicalGold {
            let risk_ratio_ppm =
                ratio_ppm(input.risk_asset_value_krw, input.total_reserve_value_krw)?;
            return Ok(IrpRiskOrderDecision::Rejected {
                reason: IrpRiskOrderRejection::InvestmentNotPermitted,
                post_order_risk_asset_value_krw: input.risk_asset_value_krw,
                post_order_total_value_krw: input.total_reserve_value_krw,
                post_order_risk_ratio_ppm: risk_ratio_ppm,
            });
        }

        let is_risk_asset = input.investment_kind == IrpInvestmentKind::Llx;
        let post_order_risk_asset_value_krw = if is_risk_asset {
            input
                .risk_asset_value_krw
                .checked_add(input.purchase_amount_krw)
                .ok_or(TaxAccountError::ArithmeticOverflow)?
        } else {
            input.risk_asset_value_krw
        };
        let post_order_total_value_krw = input.total_reserve_value_krw;
        let post_order_risk_ratio_ppm =
            ratio_ppm(post_order_risk_asset_value_krw, post_order_total_value_krw)?;
        let exceeds_limit = is_risk_asset
            && i128::from(post_order_risk_asset_value_krw)
                .checked_mul(RATE_SCALE_PPM)
                .ok_or(TaxAccountError::ArithmeticOverflow)?
                > i128::from(post_order_total_value_krw)
                    .checked_mul(i128::from(self.policy.pension.irp_risk_asset_limit_ppm))
                    .ok_or(TaxAccountError::ArithmeticOverflow)?;

        if exceeds_limit {
            Ok(IrpRiskOrderDecision::Rejected {
                reason: IrpRiskOrderRejection::RiskLimitExceeded,
                post_order_risk_asset_value_krw,
                post_order_total_value_krw,
                post_order_risk_ratio_ppm,
            })
        } else {
            Ok(IrpRiskOrderDecision::Allowed {
                post_order_risk_asset_value_krw,
                post_order_total_value_krw,
                post_order_risk_ratio_ppm,
            })
        }
    }
}

fn validate_isa_prior_income(income: IsaPriorTaxYearIncome) -> Result<(), TaxAccountError> {
    validate_non_negative(income.taxable_wage_income_krw)?;
    validate_non_negative(income.total_salary_krw)?;
    validate_non_negative(income.comprehensive_income_krw)
}

fn add_years_clamped(date: Date, years: u32) -> Result<Date, TaxAccountError> {
    let years = i32::try_from(years).map_err(|_| TaxAccountError::ArithmeticOverflow)?;
    let target_year = date
        .year()
        .checked_add(years)
        .ok_or(TaxAccountError::ArithmeticOverflow)?;
    for day in (1..=date.day()).rev() {
        if let Ok(candidate) = Date::from_calendar_date(target_year, date.month(), day) {
            return Ok(candidate);
        }
    }
    Err(TaxAccountError::InvalidDateRange)
}

fn validate_non_negative(amount_krw: i64) -> Result<(), TaxAccountError> {
    if amount_krw < 0 {
        Err(TaxAccountError::InvalidMoney)
    } else {
        Ok(())
    }
}

fn validate_rate(rate_ppm: i64) -> Result<(), TaxAccountError> {
    if rate_ppm < 0 || i128::from(rate_ppm) > RATE_SCALE_PPM {
        Err(TaxAccountError::InvalidRate)
    } else {
        Ok(())
    }
}

fn checked_i128_to_i64(value: i128) -> Result<i64, TaxAccountError> {
    i64::try_from(value).map_err(|_| TaxAccountError::ArithmeticOverflow)
}

fn floor_rate(amount_krw: i64, rate_ppm: i64) -> Result<i64, TaxAccountError> {
    validate_non_negative(amount_krw)?;
    validate_rate(rate_ppm)?;
    checked_i128_to_i64(
        i128::from(amount_krw)
            .checked_mul(i128::from(rate_ppm))
            .ok_or(TaxAccountError::ArithmeticOverflow)?
            .checked_div(RATE_SCALE_PPM)
            .ok_or(TaxAccountError::ArithmeticOverflow)?,
    )
}

fn validate_layers(layers: PensionTaxLayers) -> Result<(), TaxAccountError> {
    validate_non_negative(layers.tax_excluded_contribution_krw)?;
    validate_non_negative(layers.deferred_retirement_income_krw)?;
    validate_non_negative(layers.credited_contribution_krw)?;
    validate_non_negative(layers.earnings_krw)
}

fn layer_total(layers: PensionTaxLayers) -> Result<i64, TaxAccountError> {
    checked_i128_to_i64(
        i128::from(layers.tax_excluded_contribution_krw)
            .checked_add(i128::from(layers.deferred_retirement_income_krw))
            .and_then(|value| value.checked_add(i128::from(layers.credited_contribution_krw)))
            .and_then(|value| value.checked_add(i128::from(layers.earnings_krw)))
            .ok_or(TaxAccountError::ArithmeticOverflow)?,
    )
}

fn withdraw_from_layers(
    layers: &mut PensionTaxLayers,
    amount_krw: i64,
) -> Result<[i64; 4], TaxAccountError> {
    let mut remaining = amount_krw;
    let tax_excluded = remaining.min(layers.tax_excluded_contribution_krw);
    layers.tax_excluded_contribution_krw = layers
        .tax_excluded_contribution_krw
        .checked_sub(tax_excluded)
        .ok_or(TaxAccountError::ArithmeticOverflow)?;
    remaining = remaining
        .checked_sub(tax_excluded)
        .ok_or(TaxAccountError::ArithmeticOverflow)?;

    let deferred = remaining.min(layers.deferred_retirement_income_krw);
    layers.deferred_retirement_income_krw = layers
        .deferred_retirement_income_krw
        .checked_sub(deferred)
        .ok_or(TaxAccountError::ArithmeticOverflow)?;
    remaining = remaining
        .checked_sub(deferred)
        .ok_or(TaxAccountError::ArithmeticOverflow)?;

    let credited = remaining.min(layers.credited_contribution_krw);
    layers.credited_contribution_krw = layers
        .credited_contribution_krw
        .checked_sub(credited)
        .ok_or(TaxAccountError::ArithmeticOverflow)?;
    remaining = remaining
        .checked_sub(credited)
        .ok_or(TaxAccountError::ArithmeticOverflow)?;

    let earnings = remaining.min(layers.earnings_krw);
    layers.earnings_krw = layers
        .earnings_krw
        .checked_sub(earnings)
        .ok_or(TaxAccountError::ArithmeticOverflow)?;
    remaining = remaining
        .checked_sub(earnings)
        .ok_or(TaxAccountError::ArithmeticOverflow)?;
    if remaining != 0 {
        return Err(TaxAccountError::WithdrawalExceedsBalance);
    }
    Ok([tax_excluded, deferred, credited, earnings])
}

fn build_withdrawal_portion(
    treatment: PensionWithdrawalTreatment,
    allocations: [i64; 4],
    holder_age_years: u32,
    lifetime_contract: bool,
    pension_receipt_year: Option<u32>,
    deferred_retirement_non_pension_tax_rate_ppm: i64,
    policy: &PensionPolicy,
) -> Result<PensionWithdrawalPortion, TaxAccountError> {
    let tax_rate_context = PensionTaxRateContext {
        holder_age_years,
        lifetime_contract,
        pension_receipt_year,
        deferred_retirement_non_pension_tax_rate_ppm,
        policy,
    };
    let sources = [
        PensionTaxSource::TaxExcludedContribution,
        PensionTaxSource::DeferredRetirementIncome,
        PensionTaxSource::CreditedContribution,
        PensionTaxSource::Earnings,
    ];
    let mut tax_lines = [PensionWithdrawalTaxLine {
        source: PensionTaxSource::TaxExcludedContribution,
        gross_amount_krw: 0,
        tax_rate: PensionTaxRate::Exempt,
        tax_krw: 0,
        net_amount_krw: 0,
    }; 4];
    let mut gross_amount_krw = 0_i64;
    for ((line, source), gross) in tax_lines.iter_mut().zip(sources).zip(allocations) {
        let tax_rate = pension_tax_rate(treatment, source, gross, &tax_rate_context)?;
        let line_tax = pension_tax(gross, tax_rate)?;
        let net = gross
            .checked_sub(line_tax)
            .ok_or(TaxAccountError::ArithmeticOverflow)?;
        *line = PensionWithdrawalTaxLine {
            source,
            gross_amount_krw: gross,
            tax_rate,
            tax_krw: line_tax,
            net_amount_krw: net,
        };
        gross_amount_krw = gross_amount_krw
            .checked_add(gross)
            .ok_or(TaxAccountError::ArithmeticOverflow)?;
    }

    let credited_and_earnings_krw = allocations[2]
        .checked_add(allocations[3])
        .ok_or(TaxAccountError::ArithmeticOverflow)?;
    let credited_and_earnings_tax_krw =
        pension_tax(credited_and_earnings_krw, tax_lines[2].tax_rate)?;
    let credited_tax_krw = pension_tax(allocations[2], tax_lines[2].tax_rate)?;
    let earnings_tax_krw = credited_and_earnings_tax_krw
        .checked_sub(credited_tax_krw)
        .ok_or(TaxAccountError::ArithmeticOverflow)?;
    tax_lines[2].tax_krw = credited_tax_krw;
    tax_lines[2].net_amount_krw = allocations[2]
        .checked_sub(credited_tax_krw)
        .ok_or(TaxAccountError::ArithmeticOverflow)?;
    tax_lines[3].tax_krw = earnings_tax_krw;
    tax_lines[3].net_amount_krw = allocations[3]
        .checked_sub(earnings_tax_krw)
        .ok_or(TaxAccountError::ArithmeticOverflow)?;
    let tax_krw = tax_lines
        .iter()
        .try_fold(0_i64, |total, line| total.checked_add(line.tax_krw))
        .ok_or(TaxAccountError::ArithmeticOverflow)?;
    let net_amount_krw = gross_amount_krw
        .checked_sub(tax_krw)
        .ok_or(TaxAccountError::ArithmeticOverflow)?;

    Ok(PensionWithdrawalPortion {
        treatment,
        gross_amount_krw,
        tax_free_amount_krw: allocations[0],
        tax_krw,
        net_amount_krw,
        tax_lines,
    })
}

struct PensionTaxRateContext<'a> {
    holder_age_years: u32,
    lifetime_contract: bool,
    pension_receipt_year: Option<u32>,
    deferred_retirement_non_pension_tax_rate_ppm: i64,
    policy: &'a PensionPolicy,
}

fn pension_tax_rate(
    treatment: PensionWithdrawalTreatment,
    source: PensionTaxSource,
    gross_amount_krw: i64,
    context: &PensionTaxRateContext<'_>,
) -> Result<PensionTaxRate, TaxAccountError> {
    if source == PensionTaxSource::TaxExcludedContribution {
        return Ok(PensionTaxRate::Exempt);
    }
    if source == PensionTaxSource::DeferredRetirementIncome {
        if treatment == PensionWithdrawalTreatment::NonPension {
            return Ok(PensionTaxRate::FixedPpm(
                context.deferred_retirement_non_pension_tax_rate_ppm,
            ));
        }
        if gross_amount_krw == 0 {
            return Ok(PensionTaxRate::DeferredRetirementPension {
                non_pension_rate_ppm: context.deferred_retirement_non_pension_tax_rate_ppm,
                pension_factor_ppm: context.policy.deferred_retirement_first10_years_ppm,
            });
        }
        let receipt_year = context
            .pension_receipt_year
            .ok_or(TaxAccountError::MissingPensionReceiptYear)?;
        if receipt_year == 0 {
            return Err(TaxAccountError::InvalidPensionReceiptYear);
        }
        let pension_factor_ppm = if receipt_year <= 10 {
            context.policy.deferred_retirement_first10_years_ppm
        } else if receipt_year <= 20 {
            context.policy.deferred_retirement_years11_to20_ppm
        } else {
            context.policy.deferred_retirement_after20_years_ppm
        };
        return Ok(PensionTaxRate::DeferredRetirementPension {
            non_pension_rate_ppm: context.deferred_retirement_non_pension_tax_rate_ppm,
            pension_factor_ppm,
        });
    }

    if treatment == PensionWithdrawalTreatment::NonPension {
        return Ok(PensionTaxRate::FixedPpm(
            context.policy.non_pension_withdrawal_tax_ppm,
        ));
    }
    let rate_ppm = if context.lifetime_contract {
        context.policy.lifetime_pension_tax_ppm
    } else if context.holder_age_years < 70 {
        context.policy.under_age70_pension_tax_ppm
    } else if context.holder_age_years < 80 {
        context.policy.under_age80_pension_tax_ppm
    } else {
        context.policy.age80_or_older_pension_tax_ppm
    };
    Ok(PensionTaxRate::FixedPpm(rate_ppm))
}

fn pension_tax(amount_krw: i64, rate: PensionTaxRate) -> Result<i64, TaxAccountError> {
    match rate {
        PensionTaxRate::Exempt => Ok(0),
        PensionTaxRate::FixedPpm(rate_ppm) => floor_rate(amount_krw, rate_ppm),
        PensionTaxRate::DeferredRetirementPension {
            non_pension_rate_ppm,
            pension_factor_ppm,
        } => {
            validate_rate(non_pension_rate_ppm)?;
            validate_rate(pension_factor_ppm)?;
            checked_i128_to_i64(
                i128::from(amount_krw)
                    .checked_mul(i128::from(non_pension_rate_ppm))
                    .and_then(|value| value.checked_mul(i128::from(pension_factor_ppm)))
                    .ok_or(TaxAccountError::ArithmeticOverflow)?
                    .checked_div(
                        RATE_SCALE_PPM
                            .checked_mul(RATE_SCALE_PPM)
                            .ok_or(TaxAccountError::ArithmeticOverflow)?,
                    )
                    .ok_or(TaxAccountError::ArithmeticOverflow)?,
            )
        }
    }
}

fn ratio_ppm(risk_value_krw: i64, total_value_krw: i64) -> Result<i64, TaxAccountError> {
    if total_value_krw == 0 {
        return if risk_value_krw == 0 {
            Ok(0)
        } else {
            Ok(i64::MAX)
        };
    }
    checked_i128_to_i64(
        i128::from(risk_value_krw)
            .checked_mul(RATE_SCALE_PPM)
            .ok_or(TaxAccountError::ArithmeticOverflow)?
            .checked_div(i128::from(total_value_krw))
            .ok_or(TaxAccountError::ArithmeticOverflow)?,
    )
}

#[cfg(test)]
mod tests {
    use time::Month;

    use super::*;

    fn given_date(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("유효한 테스트 날짜여야 한다")
    }

    fn given_prior_income() -> IsaPriorTaxYearIncome {
        IsaPriorTaxYearIncome {
            taxable_wage_income_krw: 0,
            total_salary_krw: 0,
            comprehensive_income_krw: 0,
            composition: IsaPriorIncomeComposition::WageOnlyOrComprehensiveTaxExcluded,
        }
    }

    fn given_rules(policy: TaxAccountPolicy) -> Arc<dyn TaxAccountRules> {
        create_tax_account_rules_with_policy(policy).expect("유효한 테스트 정책이어야 한다")
    }

    mod context_policy_parameters_are_strict_and_configurable {
        use super::*;

        #[test]
        fn given_an_unknown_isa_field_when_decoded_then_the_policy_is_rejected() {
            let mut value = serde_json::to_value(IsaPolicy::default())
                .expect("ISA 테스트 정책을 JSON으로 바꿔야 한다");
            value
                .as_object_mut()
                .expect("ISA 정책은 객체여야 한다")
                .insert("unexpected".to_owned(), serde_json::json!(1));

            let result = serde_json::from_value::<IsaPolicy>(value);

            assert!(result.is_err());
        }

        #[test]
        fn given_a_missing_pension_field_when_decoded_then_the_policy_is_rejected() {
            let mut value = serde_json::to_value(PensionPolicy::default())
                .expect("연금 테스트 정책을 JSON으로 바꿔야 한다");
            value
                .as_object_mut()
                .expect("연금 정책은 객체여야 한다")
                .remove("limitedReceiptYears");

            let result = serde_json::from_value::<PensionPolicy>(value);

            assert!(result.is_err());
        }

        #[test]
        fn given_inconsistent_isa_limits_when_rules_are_created_then_the_policy_is_rejected() {
            let mut policy = TaxAccountPolicy::default();
            policy.isa.total_contribution_limit_krw = 99_000_000;

            let result = create_tax_account_rules_with_policy(policy);

            assert!(matches!(result, Err(TaxAccountError::InvalidPolicy)));
        }

        #[test]
        fn given_a_lower_minimum_age_when_eligibility_is_checked_then_the_pinned_age_is_used() {
            let mut policy = TaxAccountPolicy::default();
            policy.isa.minimum_age = 18;
            let rules = given_rules(policy);
            let input = IsaEnrollmentInput {
                requested_kind: IsaAccountKind::General,
                age_years: 18,
                prior_tax_year_income: Some(given_prior_income()),
                previous_three_tax_years_financial_income_taxed: Some([false; 3]),
                has_open_isa: false,
            };

            let result = rules
                .isa_enrollment_eligibility(input)
                .expect("정책화된 ISA 가입 자격을 판정해야 한다");

            assert_eq!(result, IsaEligibility::Eligible);
        }

        #[test]
        fn given_smaller_isa_contribution_limits_when_room_is_calculated_then_the_pinned_limits_apply()
         {
            let mut policy = TaxAccountPolicy::default();
            policy.isa.annual_contribution_limit_krw = 10_000_000;
            policy.isa.total_contribution_limit_krw = 50_000_000;
            let rules = given_rules(policy);
            let opened_on = given_date(2026, Month::January, 1);

            let result = rules
                .isa_contribution_room(IsaContributionRoomInput {
                    opened_on,
                    current_on: opened_on,
                    cumulative_contribution_krw: 0,
                })
                .expect("정책화된 ISA 납입한도를 계산해야 한다");

            assert_eq!(result.available_contribution_krw, 10_000_000);
        }

        #[test]
        fn given_custom_isa_tax_rates_when_a_mature_account_closes_then_the_pinned_rates_apply() {
            let mut policy = TaxAccountPolicy::default();
            policy.isa.general_tax_free_limit_krw = 0;
            policy.isa.low_income_tax_free_limit_krw = 0;
            policy.isa.separate_income_tax_ppm = 10_000;
            policy.isa.separate_local_income_tax_ppm = 1_000;
            let rules = given_rules(policy);

            let result = rules
                .isa_close_tax(IsaCloseTaxInput {
                    account_kind: IsaAccountKind::General,
                    opened_on: given_date(2026, Month::January, 1),
                    closed_on: given_date(2029, Month::January, 1),
                    isa_tax_profit_krw: 1_000_000,
                    isa_deductible_loss_krw: 0,
                    statutory_unavoidable_reason: false,
                })
                .expect("정책화된 ISA 종료세를 계산해야 한다");

            assert_eq!(
                (result.income_tax_krw, result.local_income_tax_krw),
                (10_000, 1_000)
            );
        }

        #[test]
        fn given_custom_pension_credit_limits_and_rates_when_calculated_then_the_pinned_values_apply()
         {
            let mut policy = TaxAccountPolicy::default();
            policy.pension.pension_savings_credit_limit_krw = 1_000_000;
            policy.pension.combined_credit_limit_krw = 2_000_000;
            policy.pension.high_income_tax_credit_rate_ppm = 100_000;
            policy.pension.high_local_income_tax_credit_rate_ppm = 10_000;
            let rules = given_rules(policy);

            let result = rules
                .pension_credit(PensionCreditInput {
                    pension_savings_contribution_krw: 3_000_000,
                    irp_contribution_krw: 3_000_000,
                    income: PensionCreditIncome::WageOnly {
                        total_salary_krw: 0,
                    },
                })
                .expect("정책화된 연금 예상 공제를 계산해야 한다");

            assert_eq!(
                (
                    result.pension_savings_eligible_krw,
                    result.irp_eligible_krw,
                    result.expected_credit_rate_ppm,
                    result.expected_credit_krw,
                ),
                (1_000_000, 1_000_000, 110_000, 220_000)
            );
        }

        #[test]
        fn given_a_higher_irp_risk_limit_when_a_purchase_is_checked_then_the_pinned_limit_allows_it()
         {
            let mut policy = TaxAccountPolicy::default();
            policy.pension.irp_risk_asset_limit_ppm = 800_000;
            let rules = given_rules(policy);

            let result = rules
                .evaluate_irp_risk_order(IrpRiskOrderInput {
                    total_reserve_value_krw: 1_000,
                    risk_asset_value_krw: 700,
                    purchase_amount_krw: 50,
                    investment_kind: IrpInvestmentKind::Llx,
                })
                .expect("정책화된 IRP 위험한도를 판정해야 한다");

            assert!(matches!(result, IrpRiskOrderDecision::Allowed { .. }));
        }
    }

    fn given_isa_enrollment(requested_kind: IsaAccountKind) -> IsaEnrollmentInput {
        IsaEnrollmentInput {
            requested_kind,
            age_years: 19,
            prior_tax_year_income: Some(given_prior_income()),
            previous_three_tax_years_financial_income_taxed: Some([false; 3]),
            has_open_isa: false,
        }
    }

    fn given_pension_layers() -> PensionTaxLayers {
        PensionTaxLayers {
            tax_excluded_contribution_krw: 100,
            deferred_retirement_income_krw: 200,
            credited_contribution_krw: 300,
            earnings_krw: 400,
        }
    }

    fn given_pension_withdrawal(
        request_kind: PensionWithdrawalRequestKind,
        layers: PensionTaxLayers,
        amount_krw: i64,
    ) -> PensionWithdrawalPlanInput {
        PensionWithdrawalPlanInput {
            layers,
            requested_amount_krw: amount_krw,
            request_kind,
            holder_age_years: 60,
            pension_started: true,
            opened_on: given_date(2020, Month::January, 1),
            current_on: given_date(2026, Month::January, 1),
            pension_receipt_year: Some(1),
            tax_period_opening_value_krw: 100_000_000,
            pension_withdrawn_before_request_krw: 0,
            lifetime_contract: false,
            deferred_retirement_non_pension_tax_rate_ppm: 100_000,
        }
    }

    mod context_calendar_anniversaries {
        use super::*;

        #[test]
        fn given_a_february_29_start_when_checked_in_a_common_year_then_month_end_is_the_anniversary()
         {
            let opened_on = given_date(2024, Month::February, 29);

            let before = completed_calendar_years(opened_on, given_date(2025, Month::February, 27))
                .expect("기념일 전 경과연수를 계산해야 한다");
            let on_anniversary =
                completed_calendar_years(opened_on, given_date(2025, Month::February, 28))
                    .expect("월말 기념일 경과연수를 계산해야 한다");

            assert_eq!(before, 0);
            assert_eq!(on_anniversary, 1);
        }

        #[test]
        fn given_a_leap_day_opening_when_the_first_anniversary_is_converted_then_the_clamped_game_day_is_returned()
         {
            let world_start = given_date(2024, Month::January, 1);
            let opened_on = given_date(2024, Month::February, 29);
            let expected =
                u32::try_from((given_date(2025, Month::February, 28) - world_start).whole_days())
                    .expect("테스트 날짜는 양의 게임 일수여야 한다");

            let game_day = anniversary_game_day(world_start, opened_on, 1)
                .expect("기념일 게임 일수를 계산해야 한다");

            assert_eq!(game_day, expected);
        }

        #[test]
        fn given_a_starting_age_when_one_calendar_anniversary_passes_then_age_increases_once() {
            let world_start = given_date(2026, Month::January, 1);

            let before = current_age_years(18, world_start, given_date(2026, Month::December, 31))
                .expect("기념일 전 나이를 계산해야 한다");
            let after = current_age_years(18, world_start, given_date(2027, Month::January, 1))
                .expect("기념일 나이를 계산해야 한다");

            assert_eq!(before, 18);
            assert_eq!(after, 19);
        }

        #[test]
        fn given_a_current_date_before_opening_when_years_are_counted_then_the_range_is_rejected() {
            let opened_on = given_date(2026, Month::January, 2);

            let result = completed_calendar_years(opened_on, given_date(2026, Month::January, 1));

            assert_eq!(result, Err(TaxAccountError::InvalidDateRange));
        }
    }

    mod context_isa_enrollment_eligibility {
        use super::*;

        #[test]
        fn given_an_adult_without_financial_income_history_when_general_isa_is_requested_then_enrollment_is_eligible()
         {
            let input = given_isa_enrollment(IsaAccountKind::General);

            let result = create_tax_account_rules()
                .isa_enrollment_eligibility(input)
                .expect("ISA 가입 자격을 계산해야 한다");

            assert_eq!(result, IsaEligibility::Eligible);
        }

        #[test]
        fn given_a_fifteen_year_old_with_taxable_wage_income_when_general_isa_is_requested_then_enrollment_is_eligible()
         {
            let mut input = given_isa_enrollment(IsaAccountKind::General);
            input.age_years = 15;
            input.prior_tax_year_income = Some(IsaPriorTaxYearIncome {
                taxable_wage_income_krw: 1,
                ..given_prior_income()
            });

            let result = create_tax_account_rules()
                .isa_enrollment_eligibility(input)
                .expect("ISA 가입 자격을 계산해야 한다");

            assert_eq!(result, IsaEligibility::Eligible);
        }

        #[test]
        fn given_a_minor_without_a_prior_record_when_enrollment_is_checked_then_missing_history_is_reported()
         {
            let mut input = given_isa_enrollment(IsaAccountKind::General);
            input.age_years = 15;
            input.prior_tax_year_income = None;

            let result = create_tax_account_rules()
                .isa_enrollment_eligibility(input)
                .expect("ISA 가입 자격을 계산해야 한다");

            assert_eq!(
                result,
                IsaEligibility::Ineligible(IsaIneligibilityReason::MissingTaxYearRecord)
            );
        }

        #[test]
        fn given_any_financial_income_comprehensive_taxation_year_when_enrollment_is_checked_then_tax_benefit_is_ineligible()
         {
            let mut input = given_isa_enrollment(IsaAccountKind::General);
            input.previous_three_tax_years_financial_income_taxed = Some([false, true, false]);

            let result = create_tax_account_rules()
                .isa_enrollment_eligibility(input)
                .expect("ISA 가입 자격을 계산해야 한다");

            assert_eq!(
                result,
                IsaEligibility::Ineligible(
                    IsaIneligibilityReason::FinancialIncomeComprehensiveTaxationHistory
                )
            );
        }

        #[test]
        fn given_missing_financial_income_records_when_enrollment_is_checked_then_eligibility_does_not_pass()
         {
            let mut input = given_isa_enrollment(IsaAccountKind::General);
            input.previous_three_tax_years_financial_income_taxed = None;

            let result = create_tax_account_rules()
                .isa_enrollment_eligibility(input)
                .expect("ISA 가입 자격을 계산해야 한다");

            assert_eq!(
                result,
                IsaEligibility::Ineligible(IsaIneligibilityReason::MissingTaxYearRecord)
            );
        }

        #[test]
        fn given_wage_only_income_at_the_salary_boundary_when_low_income_isa_is_requested_then_enrollment_is_eligible()
         {
            let mut input = given_isa_enrollment(IsaAccountKind::LowIncome);
            input.prior_tax_year_income = Some(IsaPriorTaxYearIncome {
                taxable_wage_income_krw: 50_000_000,
                total_salary_krw: 50_000_000,
                comprehensive_income_krw: 50_000_000,
                composition: IsaPriorIncomeComposition::WageOnlyOrComprehensiveTaxExcluded,
            });

            let result = create_tax_account_rules()
                .isa_enrollment_eligibility(input)
                .expect("서민형 ISA 자격을 계산해야 한다");

            assert_eq!(result, IsaEligibility::Eligible);
        }

        #[test]
        fn given_other_income_at_both_income_boundaries_when_low_income_isa_is_requested_then_enrollment_is_eligible()
         {
            let mut input = given_isa_enrollment(IsaAccountKind::LowIncome);
            input.prior_tax_year_income = Some(IsaPriorTaxYearIncome {
                taxable_wage_income_krw: 1,
                total_salary_krw: 50_000_000,
                comprehensive_income_krw: 38_000_000,
                composition: IsaPriorIncomeComposition::IncludesOtherComprehensiveIncome,
            });

            let result = create_tax_account_rules()
                .isa_enrollment_eligibility(input)
                .expect("서민형 ISA 자격을 계산해야 한다");

            assert_eq!(result, IsaEligibility::Eligible);
        }

        #[test]
        fn given_salary_above_fifty_million_when_low_income_isa_is_requested_then_enrollment_is_ineligible()
         {
            let mut input = given_isa_enrollment(IsaAccountKind::LowIncome);
            input.prior_tax_year_income = Some(IsaPriorTaxYearIncome {
                taxable_wage_income_krw: 1,
                total_salary_krw: 50_000_001,
                comprehensive_income_krw: 1,
                composition: IsaPriorIncomeComposition::IncludesOtherComprehensiveIncome,
            });

            let result = create_tax_account_rules()
                .isa_enrollment_eligibility(input)
                .expect("서민형 ISA 자격을 계산해야 한다");

            assert_eq!(
                result,
                IsaEligibility::Ineligible(IsaIneligibilityReason::LowIncomeCriteria)
            );
        }
    }

    mod context_isa_contribution_room {
        use super::*;

        #[test]
        fn given_a_date_before_the_first_anniversary_when_room_is_calculated_then_only_the_first_annual_limit_is_available()
         {
            let input = IsaContributionRoomInput {
                opened_on: given_date(2026, Month::January, 1),
                current_on: given_date(2026, Month::December, 31),
                cumulative_contribution_krw: 0,
            };

            let result = create_tax_account_rules()
                .isa_contribution_room(input)
                .expect("ISA 납입 가능액을 계산해야 한다");

            assert_eq!(result.completed_years, 0);
            assert_eq!(result.available_contribution_krw, 20_000_000);
        }

        #[test]
        fn given_one_anniversary_and_prior_contributions_when_room_is_calculated_then_unused_capacity_carries_forward()
         {
            let input = IsaContributionRoomInput {
                opened_on: given_date(2026, Month::January, 1),
                current_on: given_date(2027, Month::January, 1),
                cumulative_contribution_krw: 5_000_000,
            };

            let result = create_tax_account_rules()
                .isa_contribution_room(input)
                .expect("ISA 이월 납입 가능액을 계산해야 한다");

            assert_eq!(result.completed_years, 1);
            assert_eq!(result.carried_annual_capacity_krw, 40_000_000);
            assert_eq!(result.available_contribution_krw, 35_000_000);
        }

        #[test]
        fn given_four_anniversaries_when_room_is_calculated_then_the_lifetime_limit_is_reached() {
            let input = IsaContributionRoomInput {
                opened_on: given_date(2026, Month::January, 1),
                current_on: given_date(2030, Month::January, 1),
                cumulative_contribution_krw: 0,
            };

            let result = create_tax_account_rules()
                .isa_contribution_room(input)
                .expect("ISA 총 납입한도를 계산해야 한다");

            assert_eq!(result.carried_annual_capacity_krw, 100_000_000);
            assert_eq!(result.available_contribution_krw, 100_000_000);
        }

        #[test]
        fn given_contributions_above_the_lifetime_limit_when_room_is_calculated_then_available_room_is_zero()
         {
            let input = IsaContributionRoomInput {
                opened_on: given_date(2026, Month::January, 1),
                current_on: given_date(2031, Month::January, 1),
                cumulative_contribution_krw: 100_000_001,
            };

            let result = create_tax_account_rules()
                .isa_contribution_room(input)
                .expect("초과 상태의 남은 납입 가능액을 계산해야 한다");

            assert_eq!(result.available_contribution_krw, 0);
        }

        #[test]
        fn given_negative_cumulative_contributions_when_room_is_calculated_then_the_input_is_rejected()
         {
            let input = IsaContributionRoomInput {
                opened_on: given_date(2026, Month::January, 1),
                current_on: given_date(2026, Month::January, 1),
                cumulative_contribution_krw: -1,
            };

            let result = create_tax_account_rules().isa_contribution_room(input);

            assert_eq!(result, Err(TaxAccountError::InvalidMoney));
        }
    }

    mod context_isa_close_tax {
        use super::*;

        fn given_close_input(account_kind: IsaAccountKind) -> IsaCloseTaxInput {
            IsaCloseTaxInput {
                account_kind,
                opened_on: given_date(2026, Month::January, 1),
                closed_on: given_date(2029, Month::January, 1),
                isa_tax_profit_krw: 10_000_000,
                isa_deductible_loss_krw: 3_000_000,
                statutory_unavoidable_reason: false,
            }
        }

        #[test]
        fn given_a_general_isa_at_the_third_anniversary_when_closed_then_losses_and_the_two_million_exemption_apply()
         {
            let input = given_close_input(IsaAccountKind::General);

            let result = create_tax_account_rules()
                .isa_close_tax(input)
                .expect("일반형 ISA 종료세를 계산해야 한다");

            assert_eq!(result.treatment, IsaTaxTreatment::IsaSeparateTaxation);
            assert_eq!(result.net_tax_profit_krw, 7_000_000);
            assert_eq!(result.exempt_profit_krw, 2_000_000);
            assert_eq!(result.taxable_profit_krw, 5_000_000);
            assert_eq!(result.income_tax_krw, 450_000);
            assert_eq!(result.local_income_tax_krw, 45_000);
            assert_eq!(result.gross_financial_income_delta_krw, 0);
        }

        #[test]
        fn given_a_low_income_isa_after_the_minimum_term_when_closed_then_the_four_million_exemption_applies()
         {
            let input = given_close_input(IsaAccountKind::LowIncome);

            let result = create_tax_account_rules()
                .isa_close_tax(input)
                .expect("서민형 ISA 종료세를 계산해야 한다");

            assert_eq!(result.exempt_profit_krw, 4_000_000);
            assert_eq!(result.taxable_profit_krw, 3_000_000);
            assert_eq!(result.income_tax_krw, 270_000);
            assert_eq!(result.local_income_tax_krw, 27_000);
        }

        #[test]
        fn given_an_ordinary_close_before_three_years_when_taxed_then_gross_profit_uses_general_rates_without_loss_netting()
         {
            let mut input = given_close_input(IsaAccountKind::General);
            input.closed_on = given_date(2028, Month::December, 31);

            let result = create_tax_account_rules()
                .isa_close_tax(input)
                .expect("ISA 조기 종료 일반세를 계산해야 한다");

            assert_eq!(result.treatment, IsaTaxTreatment::GeneralTaxation);
            assert_eq!(result.taxable_profit_krw, 10_000_000);
            assert_eq!(result.income_tax_krw, 1_400_000);
            assert_eq!(result.local_income_tax_krw, 140_000);
            assert_eq!(result.gross_financial_income_delta_krw, 10_000_000);
        }

        #[test]
        fn given_a_statutory_unavoidable_close_before_three_years_when_taxed_then_isa_treatment_is_preserved()
         {
            let mut input = given_close_input(IsaAccountKind::General);
            input.closed_on = given_date(2027, Month::January, 1);
            input.statutory_unavoidable_reason = true;

            let result = create_tax_account_rules()
                .isa_close_tax(input)
                .expect("부득이한 ISA 종료세를 계산해야 한다");

            assert_eq!(result.treatment, IsaTaxTreatment::IsaSeparateTaxation);
            assert_eq!(result.taxable_profit_krw, 5_000_000);
        }

        #[test]
        fn given_deductible_losses_above_profit_when_closed_after_the_term_then_taxable_profit_is_zero()
         {
            let mut input = given_close_input(IsaAccountKind::General);
            input.isa_deductible_loss_krw = 20_000_000;

            let result = create_tax_account_rules()
                .isa_close_tax(input)
                .expect("손실 초과 ISA 종료세를 계산해야 한다");

            assert_eq!(result.net_tax_profit_krw, 0);
            assert_eq!(result.income_tax_krw, 0);
            assert_eq!(result.local_income_tax_krw, 0);
        }
    }

    mod context_pension_credit {
        use super::*;

        #[test]
        fn given_contributions_above_both_limits_and_low_wage_income_when_calculated_then_six_and_nine_million_caps_use_sixteen_point_five_percent()
         {
            let input = PensionCreditInput {
                pension_savings_contribution_krw: 8_000_000,
                irp_contribution_krw: 5_000_000,
                income: PensionCreditIncome::WageOnly {
                    total_salary_krw: 55_000_000,
                },
            };

            let result = create_tax_account_rules()
                .pension_credit(input)
                .expect("연금 예상 공제를 계산해야 한다");

            assert_eq!(result.pension_savings_eligible_krw, 6_000_000);
            assert_eq!(result.irp_eligible_krw, 3_000_000);
            assert_eq!(result.total_eligible_krw, 9_000_000);
            assert_eq!(result.income_tax_credit_krw, 1_350_000);
            assert_eq!(result.local_income_tax_credit_krw, 135_000);
            assert_eq!(result.expected_credit_krw, 1_485_000);
        }

        #[test]
        fn given_salary_one_won_above_the_threshold_when_calculated_then_thirteen_point_two_percent_applies()
         {
            let input = PensionCreditInput {
                pension_savings_contribution_krw: 6_000_000,
                irp_contribution_krw: 3_000_000,
                income: PensionCreditIncome::WageOnly {
                    total_salary_krw: 55_000_001,
                },
            };

            let result = create_tax_account_rules()
                .pension_credit(input)
                .expect("고소득 구간 예상 공제를 계산해야 한다");

            assert_eq!(result.income_tax_credit_krw, 1_080_000);
            assert_eq!(result.local_income_tax_credit_krw, 108_000);
            assert_eq!(result.expected_credit_krw, 1_188_000);
        }

        #[test]
        fn given_comprehensive_income_at_the_threshold_when_calculated_then_the_low_rate_boundary_is_inclusive()
         {
            let input = PensionCreditInput {
                pension_savings_contribution_krw: 1_000_000,
                irp_contribution_krw: 0,
                income: PensionCreditIncome::Other {
                    comprehensive_income_krw: 45_000_000,
                },
            };

            let result = create_tax_account_rules()
                .pension_credit(input)
                .expect("종합소득 경계 공제를 계산해야 한다");

            assert_eq!(result.expected_credit_krw, 165_000);
        }

        #[test]
        fn given_a_negative_contribution_when_calculated_then_the_input_is_rejected() {
            let input = PensionCreditInput {
                pension_savings_contribution_krw: -1,
                irp_contribution_krw: 0,
                income: PensionCreditIncome::Other {
                    comprehensive_income_krw: 0,
                },
            };

            let result = create_tax_account_rules().pension_credit(input);

            assert_eq!(result, Err(TaxAccountError::InvalidMoney));
        }
    }

    mod context_pension_receipt_limit {
        use super::*;

        #[test]
        fn given_the_first_receipt_year_when_limit_is_calculated_then_the_opening_value_is_divided_by_ten_and_multiplied_by_one_point_two()
         {
            let input = PensionReceiptLimitInput {
                pension_receipt_year: 1,
                tax_period_opening_value_krw: 100_000_000,
            };

            let result = create_tax_account_rules()
                .pension_receipt_limit(input)
                .expect("첫 연금수령한도를 계산해야 한다");

            assert_eq!(
                result,
                PensionReceiptLimit::Limited {
                    annual_limit_krw: 12_000_000
                }
            );
        }

        #[test]
        fn given_a_fractional_won_result_when_limit_is_calculated_then_only_the_final_result_is_floored()
         {
            let input = PensionReceiptLimitInput {
                pension_receipt_year: 8,
                tax_period_opening_value_krw: 101,
            };

            let result = create_tax_account_rules()
                .pension_receipt_limit(input)
                .expect("원 미만을 버린 연금수령한도를 계산해야 한다");

            assert_eq!(
                result,
                PensionReceiptLimit::Limited {
                    annual_limit_krw: 40
                }
            );
        }

        #[test]
        fn given_the_eleventh_receipt_year_when_limit_is_calculated_then_the_formula_limit_is_unlimited()
         {
            let input = PensionReceiptLimitInput {
                pension_receipt_year: 11,
                tax_period_opening_value_krw: 100_000_000,
            };

            let result = create_tax_account_rules()
                .pension_receipt_limit(input)
                .expect("11년차 연금수령한도를 계산해야 한다");

            assert_eq!(result, PensionReceiptLimit::Unlimited);
        }

        #[test]
        fn given_zero_as_the_receipt_year_when_limit_is_calculated_then_the_input_is_rejected() {
            let input = PensionReceiptLimitInput {
                pension_receipt_year: 0,
                tax_period_opening_value_krw: 100,
            };

            let result = create_tax_account_rules().pension_receipt_limit(input);

            assert_eq!(result, Err(TaxAccountError::InvalidPensionReceiptYear));
        }

        #[test]
        fn given_an_opening_value_whose_limit_exceeds_i64_when_calculated_then_overflow_is_reported()
         {
            let input = PensionReceiptLimitInput {
                pension_receipt_year: 10,
                tax_period_opening_value_krw: i64::MAX,
            };

            let result = create_tax_account_rules().pension_receipt_limit(input);

            assert_eq!(result, Err(TaxAccountError::ArithmeticOverflow));
        }
    }

    mod context_pension_receipt_eligibility {
        use super::*;

        fn given_eligibility_input() -> PensionReceiptEligibilityInput {
            PensionReceiptEligibilityInput {
                holder_age_years: 55,
                pension_started: true,
                opened_on: given_date(2024, Month::February, 29),
                current_on: given_date(2029, Month::February, 28),
                has_deferred_retirement_income: false,
                pension_receipt_year: 1,
                tax_period_opening_value_krw: 100_000_000,
                pension_withdrawn_before_request_krw: 2_000_000,
            }
        }

        #[test]
        fn given_age_start_and_the_clamped_fifth_anniversary_when_checked_then_remaining_limit_is_returned()
         {
            let input = given_eligibility_input();

            let result = create_tax_account_rules()
                .pension_receipt_eligibility(input)
                .expect("연금수령 자격을 계산해야 한다");

            assert_eq!(
                result,
                PensionReceiptEligibility::Eligible {
                    annual_limit_krw: Some(12_000_000),
                    remaining_limit_krw: Some(10_000_000),
                }
            );
        }

        #[test]
        fn given_the_day_before_the_fifth_clamped_anniversary_when_checked_then_holding_period_is_not_met()
         {
            let mut input = given_eligibility_input();
            input.current_on = given_date(2029, Month::February, 27);

            let result = create_tax_account_rules()
                .pension_receipt_eligibility(input)
                .expect("연금 가입기간 자격을 계산해야 한다");

            assert_eq!(
                result,
                PensionReceiptEligibility::Ineligible(
                    PensionReceiptIneligibilityReason::MinimumHoldingPeriod
                )
            );
        }

        #[test]
        fn given_deferred_retirement_income_before_five_years_when_checked_then_holding_period_is_waived()
         {
            let mut input = given_eligibility_input();
            input.current_on = given_date(2024, Month::March, 1);
            input.has_deferred_retirement_income = true;

            let result = create_tax_account_rules()
                .pension_receipt_eligibility(input)
                .expect("이연퇴직소득 예외를 적용해야 한다");

            assert!(matches!(result, PensionReceiptEligibility::Eligible { .. }));
        }

        #[test]
        fn given_an_age_below_fifty_five_when_checked_then_pension_receipt_is_ineligible() {
            let mut input = given_eligibility_input();
            input.holder_age_years = 54;

            let result = create_tax_account_rules()
                .pension_receipt_eligibility(input)
                .expect("연금 나이 자격을 계산해야 한다");

            assert_eq!(
                result,
                PensionReceiptEligibility::Ineligible(
                    PensionReceiptIneligibilityReason::UnderMinimumAge
                )
            );
        }

        #[test]
        fn given_no_start_application_when_checked_then_pension_receipt_is_ineligible() {
            let mut input = given_eligibility_input();
            input.pension_started = false;

            let result = create_tax_account_rules()
                .pension_receipt_eligibility(input)
                .expect("연금 개시 자격을 계산해야 한다");

            assert_eq!(
                result,
                PensionReceiptEligibility::Ineligible(
                    PensionReceiptIneligibilityReason::NotStarted
                )
            );
        }
    }

    mod context_pension_withdrawal_order_and_split {
        use super::*;

        #[test]
        fn given_all_four_tax_layers_when_non_pension_is_withdrawn_then_layers_are_consumed_in_statutory_order()
         {
            let input = given_pension_withdrawal(
                PensionWithdrawalRequestKind::ExplicitNonPension,
                given_pension_layers(),
                650,
            );

            let result = create_tax_account_rules()
                .plan_pension_withdrawal(input)
                .expect("연금외 인출을 계산해야 한다");

            assert_eq!(result.pension_amount_krw, 0);
            assert_eq!(result.non_pension_amount_krw, 650);
            assert_eq!(result.tax_free_amount_krw, 100);
            assert_eq!(result.tax_krw, 77);
            assert_eq!(result.net_payout_krw, 573);
            assert_eq!(result.portions[0].tax_lines[0].gross_amount_krw, 100);
            assert_eq!(result.portions[0].tax_lines[1].gross_amount_krw, 200);
            assert_eq!(result.portions[0].tax_lines[2].gross_amount_krw, 300);
            assert_eq!(result.portions[0].tax_lines[3].gross_amount_krw, 50);
            assert_eq!(result.remaining_layers.earnings_krw, 350);
        }

        #[test]
        fn given_credited_principal_and_earnings_with_sub_won_individual_tax_when_withdrawn_then_the_shared_tax_base_is_floored_once()
         {
            let layers = PensionTaxLayers {
                tax_excluded_contribution_krw: 0,
                deferred_retirement_income_krw: 0,
                credited_contribution_krw: 4,
                earnings_krw: 4,
            };
            let input = given_pension_withdrawal(
                PensionWithdrawalRequestKind::ExplicitNonPension,
                layers,
                8,
            );

            let result = create_tax_account_rules()
                .plan_pension_withdrawal(input)
                .expect("같은 기타소득 세원 묶음을 함께 계산해야 한다");

            assert_eq!(result.tax_krw, 1);
            assert_eq!(result.portions[0].tax_lines[2].tax_krw, 0);
            assert_eq!(result.portions[0].tax_lines[3].tax_krw, 1);
        }

        #[test]
        fn given_a_regular_request_above_the_remaining_limit_when_planned_then_pension_and_non_pension_share_one_layer_order()
         {
            let layers = PensionTaxLayers {
                tax_excluded_contribution_krw: 100,
                deferred_retirement_income_krw: 0,
                credited_contribution_krw: 1_000,
                earnings_krw: 1_000,
            };
            let mut input =
                given_pension_withdrawal(PensionWithdrawalRequestKind::RegularPension, layers, 300);
            input.tax_period_opening_value_krw = 1_250;

            let result = create_tax_account_rules()
                .plan_pension_withdrawal(input)
                .expect("한도 초과 연금 인출을 분할해야 한다");

            assert_eq!(result.pension_amount_krw, 150);
            assert_eq!(result.non_pension_amount_krw, 150);
            assert_eq!(result.portions.len(), 2);
            assert_eq!(
                result.portions[0].treatment,
                PensionWithdrawalTreatment::Pension
            );
            assert_eq!(result.portions[0].tax_lines[0].gross_amount_krw, 100);
            assert_eq!(result.portions[0].tax_lines[2].gross_amount_krw, 50);
            assert_eq!(
                result.portions[1].treatment,
                PensionWithdrawalTreatment::NonPension
            );
            assert_eq!(result.portions[1].tax_lines[2].gross_amount_krw, 150);
            assert_eq!(result.tax_krw, 26);
            assert_eq!(result.remaining_layers.credited_contribution_krw, 800);
        }

        #[test]
        fn given_a_regular_request_before_pension_start_when_planned_then_it_cannot_bypass_eligibility()
         {
            let mut input = given_pension_withdrawal(
                PensionWithdrawalRequestKind::RegularPension,
                given_pension_layers(),
                100,
            );
            input.pension_started = false;

            let result = create_tax_account_rules().plan_pension_withdrawal(input);

            assert_eq!(result, Err(TaxAccountError::PensionReceiptNotEligible));
        }

        #[test]
        fn given_a_statutory_unavoidable_request_without_normal_eligibility_when_planned_then_the_whole_amount_uses_pension_tax()
         {
            let layers = PensionTaxLayers {
                tax_excluded_contribution_krw: 0,
                deferred_retirement_income_krw: 0,
                credited_contribution_krw: 1_000,
                earnings_krw: 0,
            };
            let mut input = given_pension_withdrawal(
                PensionWithdrawalRequestKind::StatutoryUnavoidable,
                layers,
                1_000,
            );
            input.holder_age_years = 54;
            input.pension_started = false;
            input.current_on = given_date(2021, Month::January, 1);
            input.pension_receipt_year = None;

            let result = create_tax_account_rules()
                .plan_pension_withdrawal(input)
                .expect("부득이한 인출에 연금세율을 적용해야 한다");

            assert_eq!(result.pension_amount_krw, 1_000);
            assert_eq!(result.non_pension_amount_krw, 0);
            assert_eq!(
                result.portions[0].treatment,
                PensionWithdrawalTreatment::PensionUnavoidable
            );
            assert_eq!(result.tax_krw, 55);
        }

        #[test]
        fn given_a_withdrawal_above_total_layers_when_planned_then_the_request_is_rejected() {
            let input = given_pension_withdrawal(
                PensionWithdrawalRequestKind::ExplicitNonPension,
                given_pension_layers(),
                1_001,
            );

            let result = create_tax_account_rules().plan_pension_withdrawal(input);

            assert_eq!(result, Err(TaxAccountError::WithdrawalExceedsBalance));
        }

        #[test]
        fn given_layer_totals_above_i64_when_planned_then_overflow_is_reported() {
            let layers = PensionTaxLayers {
                tax_excluded_contribution_krw: i64::MAX,
                deferred_retirement_income_krw: i64::MAX,
                credited_contribution_krw: 0,
                earnings_krw: 0,
            };
            let input = given_pension_withdrawal(
                PensionWithdrawalRequestKind::ExplicitNonPension,
                layers,
                1,
            );

            let result = create_tax_account_rules().plan_pension_withdrawal(input);

            assert_eq!(result, Err(TaxAccountError::ArithmeticOverflow));
        }
    }

    mod context_pension_withdrawal_rates {
        use super::*;

        fn when_unavoidable_credited_tax_is_planned(
            age_years: u32,
            lifetime_contract: bool,
        ) -> PensionWithdrawalPlan {
            let layers = PensionTaxLayers {
                tax_excluded_contribution_krw: 0,
                deferred_retirement_income_krw: 0,
                credited_contribution_krw: 1_000,
                earnings_krw: 0,
            };
            let mut input = given_pension_withdrawal(
                PensionWithdrawalRequestKind::StatutoryUnavoidable,
                layers,
                1_000,
            );
            input.holder_age_years = age_years;
            input.lifetime_contract = lifetime_contract;

            create_tax_account_rules()
                .plan_pension_withdrawal(input)
                .expect("연령별 연금세율을 계산해야 한다")
        }

        fn when_deferred_pension_tax_is_planned(receipt_year: u32) -> PensionWithdrawalPlan {
            let layers = PensionTaxLayers {
                tax_excluded_contribution_krw: 0,
                deferred_retirement_income_krw: 1_000,
                credited_contribution_krw: 0,
                earnings_krw: 0,
            };
            let mut input = given_pension_withdrawal(
                PensionWithdrawalRequestKind::StatutoryUnavoidable,
                layers,
                1_000,
            );
            input.pension_receipt_year = Some(receipt_year);

            create_tax_account_rules()
                .plan_pension_withdrawal(input)
                .expect("이연퇴직소득 연금세율을 계산해야 한다")
        }

        #[test]
        fn given_age_below_seventy_when_pension_tax_is_planned_then_five_point_five_percent_applies()
         {
            let result = when_unavoidable_credited_tax_is_planned(69, false);

            let tax = result.tax_krw;

            assert_eq!(tax, 55);
        }

        #[test]
        fn given_age_seventy_when_pension_tax_is_planned_then_four_point_four_percent_applies() {
            let result = when_unavoidable_credited_tax_is_planned(70, false);

            let tax = result.tax_krw;

            assert_eq!(tax, 44);
        }

        #[test]
        fn given_age_eighty_when_pension_tax_is_planned_then_three_point_three_percent_applies() {
            let result = when_unavoidable_credited_tax_is_planned(80, false);

            let tax = result.tax_krw;

            assert_eq!(tax, 33);
        }

        #[test]
        fn given_a_lifetime_contract_when_pension_tax_is_planned_then_three_point_three_percent_overrides_age()
         {
            let result = when_unavoidable_credited_tax_is_planned(60, true);

            let tax = result.tax_krw;

            assert_eq!(tax, 33);
        }

        #[test]
        fn given_the_tenth_receipt_year_when_deferred_tax_is_planned_then_seventy_percent_of_retirement_tax_applies()
         {
            let result = when_deferred_pension_tax_is_planned(10);

            let tax = result.tax_krw;

            assert_eq!(tax, 70);
        }

        #[test]
        fn given_the_eleventh_receipt_year_when_deferred_tax_is_planned_then_sixty_percent_of_retirement_tax_applies()
         {
            let result = when_deferred_pension_tax_is_planned(11);

            let tax = result.tax_krw;

            assert_eq!(tax, 60);
        }

        #[test]
        fn given_the_twenty_first_receipt_year_when_deferred_tax_is_planned_then_fifty_percent_of_retirement_tax_applies()
         {
            let result = when_deferred_pension_tax_is_planned(21);

            let tax = result.tax_krw;

            assert_eq!(tax, 50);
        }

        #[test]
        fn given_deferred_income_without_a_receipt_year_when_unavoidable_tax_is_planned_then_missing_year_is_reported()
         {
            let layers = PensionTaxLayers {
                tax_excluded_contribution_krw: 0,
                deferred_retirement_income_krw: 1_000,
                credited_contribution_krw: 0,
                earnings_krw: 0,
            };
            let mut input = given_pension_withdrawal(
                PensionWithdrawalRequestKind::StatutoryUnavoidable,
                layers,
                1_000,
            );
            input.pension_receipt_year = None;

            let result = create_tax_account_rules().plan_pension_withdrawal(input);

            assert_eq!(result, Err(TaxAccountError::MissingPensionReceiptYear));
        }
    }

    mod context_irp_risk_limit {
        use super::*;

        #[test]
        fn given_a_risky_purchase_reaching_exactly_seventy_percent_when_checked_then_the_order_is_allowed()
         {
            let input = IrpRiskOrderInput {
                total_reserve_value_krw: 1_000,
                risk_asset_value_krw: 600,
                purchase_amount_krw: 100,
                investment_kind: IrpInvestmentKind::Llx,
            };

            let result = create_tax_account_rules()
                .evaluate_irp_risk_order(input)
                .expect("IRP 위험자산 한도를 계산해야 한다");

            assert_eq!(
                result,
                IrpRiskOrderDecision::Allowed {
                    post_order_risk_asset_value_krw: 700,
                    post_order_total_value_krw: 1_000,
                    post_order_risk_ratio_ppm: 700_000,
                }
            );
        }

        #[test]
        fn given_a_risky_purchase_one_won_above_seventy_percent_when_checked_then_the_order_is_rejected()
         {
            let input = IrpRiskOrderInput {
                total_reserve_value_krw: 1_000,
                risk_asset_value_krw: 600,
                purchase_amount_krw: 101,
                investment_kind: IrpInvestmentKind::Llx,
            };

            let result = create_tax_account_rules()
                .evaluate_irp_risk_order(input)
                .expect("IRP 위험자산 한도를 계산해야 한다");

            assert!(matches!(
                result,
                IrpRiskOrderDecision::Rejected {
                    reason: IrpRiskOrderRejection::RiskLimitExceeded,
                    ..
                }
            ));
        }

        #[test]
        fn given_appreciation_already_above_seventy_percent_when_a_safe_asset_is_bought_then_the_order_is_allowed_without_forced_sale()
         {
            let input = IrpRiskOrderInput {
                total_reserve_value_krw: 1_000,
                risk_asset_value_krw: 800,
                purchase_amount_krw: 100,
                investment_kind: IrpInvestmentKind::TreasuryBond,
            };

            let result = create_tax_account_rules()
                .evaluate_irp_risk_order(input)
                .expect("사후 초과 상태의 안전자산 주문을 판정해야 한다");

            assert!(matches!(result, IrpRiskOrderDecision::Allowed { .. }));
        }

        #[test]
        fn given_appreciation_already_above_seventy_percent_when_more_llx_is_bought_then_the_order_is_rejected()
         {
            let input = IrpRiskOrderInput {
                total_reserve_value_krw: 1_000,
                risk_asset_value_krw: 800,
                purchase_amount_krw: 1,
                investment_kind: IrpInvestmentKind::Llx,
            };

            let result = create_tax_account_rules()
                .evaluate_irp_risk_order(input)
                .expect("사후 초과 상태의 위험자산 주문을 판정해야 한다");

            assert!(matches!(
                result,
                IrpRiskOrderDecision::Rejected {
                    reason: IrpRiskOrderRejection::RiskLimitExceeded,
                    ..
                }
            ));
        }

        #[test]
        fn given_krx_physical_gold_when_checked_then_the_investment_is_not_permitted() {
            let input = IrpRiskOrderInput {
                total_reserve_value_krw: 1_000,
                risk_asset_value_krw: 0,
                purchase_amount_krw: 1,
                investment_kind: IrpInvestmentKind::KrxPhysicalGold,
            };

            let result = create_tax_account_rules()
                .evaluate_irp_risk_order(input)
                .expect("IRP 허용 운용방법을 판정해야 한다");

            assert!(matches!(
                result,
                IrpRiskOrderDecision::Rejected {
                    reason: IrpRiskOrderRejection::InvestmentNotPermitted,
                    ..
                }
            ));
        }

        #[test]
        fn given_risk_value_above_total_reserve_when_checked_then_the_state_is_rejected() {
            let input = IrpRiskOrderInput {
                total_reserve_value_krw: 999,
                risk_asset_value_krw: 1_000,
                purchase_amount_krw: 1,
                investment_kind: IrpInvestmentKind::Llx,
            };

            let result = create_tax_account_rules().evaluate_irp_risk_order(input);

            assert_eq!(result, Err(TaxAccountError::InvalidIrpState));
        }

        #[test]
        fn given_a_risk_value_addition_above_i64_when_checked_then_overflow_is_reported() {
            let input = IrpRiskOrderInput {
                total_reserve_value_krw: i64::MAX,
                risk_asset_value_krw: i64::MAX,
                purchase_amount_krw: 1,
                investment_kind: IrpInvestmentKind::Llx,
            };

            let result = create_tax_account_rules().evaluate_irp_risk_order(input);

            assert_eq!(result, Err(TaxAccountError::ArithmeticOverflow));
        }
    }

    mod context_request_enum_protocol {
        use super::*;

        #[test]
        fn given_pension_and_irp_reason_enums_when_serialized_then_api_discriminants_are_camel_case()
         {
            let pension = PensionWithdrawalRequestKind::RegularPension;
            let reason = IrpWithdrawalReason::HomePurchase;

            let pension_json =
                serde_json::to_string(&pension).expect("연금 인출 유형을 직렬화해야 한다");
            let reason_json =
                serde_json::to_string(&reason).expect("IRP 인출 사유를 직렬화해야 한다");

            assert_eq!(pension_json, "\"pension\"");
            assert_eq!(reason_json, "\"homePurchase\"");
        }
    }
}
