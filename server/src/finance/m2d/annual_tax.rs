//! M2-D annual financial-income assessment rules (§8.6).

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};
use time::{Date, Month};

const RATE_SCALE_PPM: i128 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnualTaxError {
    InvalidThreshold,
    InvalidRate,
    InvalidBracketRange,
    UnsortedBrackets,
    OverlappingBrackets,
    NonContiguousBrackets,
    EndlessBracketNotLast,
    MissingEndlessBracket,
    DuplicateSourceRate,
    MissingSourceRate,
    DuplicateSourceYear,
    InvalidDate,
    SourceMismatch,
    InvalidMoney,
    ArithmeticOverflow,
    InvalidStateTransition,
    InvalidFinalizationDate,
    InvalidFilingDate,
    InvalidSettlementAmounts,
}

impl Display for AnnualTaxError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidThreshold => "annual financial-income threshold must be positive",
            Self::InvalidRate => "annual financial-income tax rate is invalid",
            Self::InvalidBracketRange => "progressive tax bracket range is invalid",
            Self::UnsortedBrackets => "progressive tax brackets are not sorted",
            Self::OverlappingBrackets => "progressive tax brackets overlap",
            Self::NonContiguousBrackets => "progressive tax brackets are not contiguous",
            Self::EndlessBracketNotLast => "only the last progressive tax bracket may be endless",
            Self::MissingEndlessBracket => "the last progressive tax bracket must be endless",
            Self::DuplicateSourceRate => "annual financial-income source rate is duplicated",
            Self::MissingSourceRate => "annual financial-income source rate is missing",
            Self::DuplicateSourceYear => "annual financial-income source total is duplicated",
            Self::InvalidDate => "annual financial-income policy date is invalid",
            Self::SourceMismatch => "financial-income accrual source does not match its total",
            Self::InvalidMoney => "annual financial-income money must be non-negative",
            Self::ArithmeticOverflow => "annual financial-income arithmetic overflowed",
            Self::InvalidStateTransition => {
                "annual financial-income assessment state transition is invalid"
            }
            Self::InvalidFinalizationDate => {
                "annual financial-income assessment must finalize on the next January 1"
            }
            Self::InvalidFilingDate => {
                "annual financial-income filing must execute on its scheduled date"
            }
            Self::InvalidSettlementAmounts => {
                "annual financial-income settlement amounts are inconsistent"
            }
        };
        formatter.write_str(message)
    }
}

impl Error for AnnualTaxError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum FinancialIncomeSource {
    CmaInterest,
    DepositInterest,
    BondCoupon,
    LlxDistribution,
    IsaEarlyClose,
}

impl FinancialIncomeSource {
    pub const ALL: [Self; 5] = [
        Self::CmaInterest,
        Self::DepositInterest,
        Self::BondCoupon,
        Self::LlxDistribution,
        Self::IsaEarlyClose,
    ];

    fn index(self) -> usize {
        match self {
            Self::CmaInterest => 0,
            Self::DepositInterest => 1,
            Self::BondCoupon => 2,
            Self::LlxDistribution => 3,
            Self::IsaEarlyClose => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressiveTaxBracket {
    pub lower_bound_krw: i64,
    pub upper_bound_krw: Option<i64>,
    pub rate_ppm: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FinancialIncomeSourceRate {
    pub source: FinancialIncomeSource,
    pub income_tax_rate_ppm: i64,
    pub local_income_tax_rate_ppm: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnnualFilingDatePolicy {
    pub month: u8,
    pub day: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnnualFinancialIncomeTaxPolicy {
    pub comprehensive_threshold_krw: i64,
    pub general_income_tax_rate_ppm: i64,
    pub general_local_income_tax_rate_ppm: i64,
    pub income_tax_brackets: Vec<ProgressiveTaxBracket>,
    pub local_income_tax_brackets: Vec<ProgressiveTaxBracket>,
    pub source_rates: Vec<FinancialIncomeSourceRate>,
    pub filing_date: AnnualFilingDatePolicy,
}

impl AnnualFinancialIncomeTaxPolicy {
    /// Validates the complete policy before it is used for an assessment.
    pub fn validate(&self) -> Result<(), AnnualTaxError> {
        if self.comprehensive_threshold_krw <= 0 {
            return Err(AnnualTaxError::InvalidThreshold);
        }
        validate_rate(self.general_income_tax_rate_ppm)?;
        validate_rate(self.general_local_income_tax_rate_ppm)?;
        validate_brackets(&self.income_tax_brackets)?;
        validate_brackets(&self.local_income_tax_brackets)?;
        validate_source_rates(&self.source_rates)?;
        validate_annual_date(self.filing_date)?;
        Ok(())
    }

    fn source_rate(
        &self,
        source: FinancialIncomeSource,
    ) -> Result<FinancialIncomeSourceRate, AnnualTaxError> {
        self.source_rates
            .iter()
            .find(|rate| rate.source == source)
            .copied()
            .ok_or(AnnualTaxError::MissingSourceRate)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FinancialIncomeAccrual {
    pub source: FinancialIncomeSource,
    pub gross_income_krw: i64,
    pub withheld_income_tax_krw: i64,
    pub withheld_local_income_tax_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FinancialIncomeSourceYear {
    pub source: FinancialIncomeSource,
    pub gross_income_krw: i64,
    pub withheld_income_tax_krw: i64,
    pub withheld_local_income_tax_krw: i64,
}

impl FinancialIncomeSourceYear {
    pub fn zero(source: FinancialIncomeSource) -> Self {
        Self {
            source,
            gross_income_krw: 0,
            withheld_income_tax_krw: 0,
            withheld_local_income_tax_krw: 0,
        }
    }

    /// Adds one payment to this source's annual totals without partially mutating on failure.
    pub fn accrue(&mut self, accrual: FinancialIncomeAccrual) -> Result<(), AnnualTaxError> {
        if self.source != accrual.source {
            return Err(AnnualTaxError::SourceMismatch);
        }
        validate_money_values(&[
            self.gross_income_krw,
            self.withheld_income_tax_krw,
            self.withheld_local_income_tax_krw,
            accrual.gross_income_krw,
            accrual.withheld_income_tax_krw,
            accrual.withheld_local_income_tax_krw,
        ])?;

        let gross_income_krw = checked_add_money(self.gross_income_krw, accrual.gross_income_krw)?;
        let withheld_income_tax_krw = checked_add_money(
            self.withheld_income_tax_krw,
            accrual.withheld_income_tax_krw,
        )?;
        let withheld_local_income_tax_krw = checked_add_money(
            self.withheld_local_income_tax_krw,
            accrual.withheld_local_income_tax_krw,
        )?;

        self.gross_income_krw = gross_income_krw;
        self.withheld_income_tax_krw = withheld_income_tax_krw;
        self.withheld_local_income_tax_krw = withheld_local_income_tax_krw;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaxCredits {
    pub income_tax_credit_krw: i64,
    pub local_income_tax_credit_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FinancialIncomeAssessmentStatus {
    Open,
    FinalizedNoFiling,
    FilingPending,
    Filed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnualAssessmentFinalizeInput {
    pub tax_year: i32,
    pub finalization_date: Date,
    pub current_status: FinancialIncomeAssessmentStatus,
    pub source_years: Vec<FinancialIncomeSourceYear>,
    pub other_comprehensive_income_krw: i64,
    pub credits: TaxCredits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnnualAssessmentDraft {
    pub tax_year: i32,
    pub finalization_date: Date,
    pub filing_date: Option<Date>,
    pub status: FinancialIncomeAssessmentStatus,
    pub gross_financial_income_krw: i64,
    pub other_comprehensive_income_krw: i64,
    pub withheld_income_tax_krw: i64,
    pub withheld_local_income_tax_krw: i64,
    pub income_tax_formula_a_krw: i64,
    pub income_tax_formula_b_krw: i64,
    pub local_income_tax_formula_a_krw: i64,
    pub local_income_tax_formula_b_krw: i64,
    pub income_tax_credit_krw: i64,
    pub local_income_tax_credit_krw: i64,
    pub final_income_tax_krw: i64,
    pub final_local_income_tax_krw: i64,
    pub additional_tax_krw: i64,
    pub refund_krw: i64,
}

/// Calculates progressive basic tax, flooring each bracket slice independently.
pub fn calculate_basic_tax(
    taxable_income_krw: i64,
    brackets: &[ProgressiveTaxBracket],
) -> Result<i64, AnnualTaxError> {
    if taxable_income_krw < 0 {
        return Err(AnnualTaxError::InvalidMoney);
    }
    validate_brackets(brackets)?;

    let mut total_tax = 0_i128;
    for bracket in brackets {
        if taxable_income_krw <= bracket.lower_bound_krw {
            break;
        }
        let slice_upper = bracket
            .upper_bound_krw
            .map_or(taxable_income_krw, |upper| upper.min(taxable_income_krw));
        let slice_krw = slice_upper
            .checked_sub(bracket.lower_bound_krw)
            .ok_or(AnnualTaxError::ArithmeticOverflow)?;
        if slice_krw <= 0 {
            continue;
        }
        let slice_tax = i128::from(slice_krw)
            .checked_mul(i128::from(bracket.rate_ppm))
            .ok_or(AnnualTaxError::ArithmeticOverflow)?
            .checked_div(RATE_SCALE_PPM)
            .ok_or(AnnualTaxError::ArithmeticOverflow)?;
        total_tax = total_tax
            .checked_add(slice_tax)
            .ok_or(AnnualTaxError::ArithmeticOverflow)?;
    }
    checked_i128_to_i64(total_tax)
}

/// Finalizes one open tax year into either a closed record or a filing draft.
pub fn finalize_annual_assessment(
    policy: &AnnualFinancialIncomeTaxPolicy,
    input: &AnnualAssessmentFinalizeInput,
) -> Result<AnnualAssessmentDraft, AnnualTaxError> {
    policy.validate()?;
    if input.current_status != FinancialIncomeAssessmentStatus::Open {
        return Err(AnnualTaxError::InvalidStateTransition);
    }
    validate_finalization_date(input.tax_year, input.finalization_date)?;
    validate_money_values(&[
        input.other_comprehensive_income_krw,
        input.credits.income_tax_credit_krw,
        input.credits.local_income_tax_credit_krw,
    ])?;

    let totals = sum_source_years(&input.source_years)?;
    if totals.gross_financial_income_krw <= policy.comprehensive_threshold_krw {
        validate_assessment_transition(
            input.current_status,
            FinancialIncomeAssessmentStatus::FinalizedNoFiling,
        )?;
        return Ok(AnnualAssessmentDraft {
            tax_year: input.tax_year,
            finalization_date: input.finalization_date,
            filing_date: None,
            status: FinancialIncomeAssessmentStatus::FinalizedNoFiling,
            gross_financial_income_krw: totals.gross_financial_income_krw,
            other_comprehensive_income_krw: input.other_comprehensive_income_krw,
            withheld_income_tax_krw: totals.withheld_income_tax_krw,
            withheld_local_income_tax_krw: totals.withheld_local_income_tax_krw,
            income_tax_formula_a_krw: totals.withheld_income_tax_krw,
            income_tax_formula_b_krw: totals.withheld_income_tax_krw,
            local_income_tax_formula_a_krw: totals.withheld_local_income_tax_krw,
            local_income_tax_formula_b_krw: totals.withheld_local_income_tax_krw,
            income_tax_credit_krw: input.credits.income_tax_credit_krw,
            local_income_tax_credit_krw: input.credits.local_income_tax_credit_krw,
            final_income_tax_krw: totals.withheld_income_tax_krw,
            final_local_income_tax_krw: totals.withheld_local_income_tax_krw,
            additional_tax_krw: 0,
            refund_krw: 0,
        });
    }

    validate_assessment_transition(
        input.current_status,
        FinancialIncomeAssessmentStatus::FilingPending,
    )?;
    let financial_excess_krw = totals
        .gross_financial_income_krw
        .checked_sub(policy.comprehensive_threshold_krw)
        .ok_or(AnnualTaxError::ArithmeticOverflow)?;
    let formula_a_taxable_krw =
        checked_add_money(financial_excess_krw, input.other_comprehensive_income_krw)?;

    let income_tax_formula_a_krw = checked_add_money(
        calculate_basic_tax(formula_a_taxable_krw, &policy.income_tax_brackets)?,
        calculate_rate_tax(
            policy.comprehensive_threshold_krw,
            policy.general_income_tax_rate_ppm,
        )?,
    )?;
    let local_income_tax_formula_a_krw = checked_add_money(
        calculate_basic_tax(formula_a_taxable_krw, &policy.local_income_tax_brackets)?,
        calculate_rate_tax(
            policy.comprehensive_threshold_krw,
            policy.general_local_income_tax_rate_ppm,
        )?,
    )?;
    let income_tax_formula_b_krw = checked_add_money(
        calculate_source_tax(policy, &input.source_years, TaxKind::Income)?,
        calculate_basic_tax(
            input.other_comprehensive_income_krw,
            &policy.income_tax_brackets,
        )?,
    )?;
    let local_income_tax_formula_b_krw = checked_add_money(
        calculate_source_tax(policy, &input.source_years, TaxKind::LocalIncome)?,
        calculate_basic_tax(
            input.other_comprehensive_income_krw,
            &policy.local_income_tax_brackets,
        )?,
    )?;

    let final_income_tax_krw = apply_credit(
        income_tax_formula_a_krw.max(income_tax_formula_b_krw),
        input.credits.income_tax_credit_krw,
    )?;
    let final_local_income_tax_krw = apply_credit(
        local_income_tax_formula_a_krw.max(local_income_tax_formula_b_krw),
        input.credits.local_income_tax_credit_krw,
    )?;
    let settlement = calculate_settlement(
        final_income_tax_krw,
        final_local_income_tax_krw,
        totals.withheld_income_tax_krw,
        totals.withheld_local_income_tax_krw,
    )?;

    Ok(AnnualAssessmentDraft {
        tax_year: input.tax_year,
        finalization_date: input.finalization_date,
        filing_date: Some(filing_date(input.tax_year, policy.filing_date)?),
        status: FinancialIncomeAssessmentStatus::FilingPending,
        gross_financial_income_krw: totals.gross_financial_income_krw,
        other_comprehensive_income_krw: input.other_comprehensive_income_krw,
        withheld_income_tax_krw: totals.withheld_income_tax_krw,
        withheld_local_income_tax_krw: totals.withheld_local_income_tax_krw,
        income_tax_formula_a_krw,
        income_tax_formula_b_krw,
        local_income_tax_formula_a_krw,
        local_income_tax_formula_b_krw,
        income_tax_credit_krw: input.credits.income_tax_credit_krw,
        local_income_tax_credit_krw: input.credits.local_income_tax_credit_krw,
        final_income_tax_krw,
        final_local_income_tax_krw,
        additional_tax_krw: settlement.additional_tax_krw,
        refund_krw: settlement.refund_krw,
    })
}

/// Accepts only the state edges defined by the annual assessment lifecycle.
pub fn validate_assessment_transition(
    from: FinancialIncomeAssessmentStatus,
    to: FinancialIncomeAssessmentStatus,
) -> Result<(), AnnualTaxError> {
    if matches!(
        (from, to),
        (
            FinancialIncomeAssessmentStatus::Open,
            FinancialIncomeAssessmentStatus::FinalizedNoFiling
        ) | (
            FinancialIncomeAssessmentStatus::Open,
            FinancialIncomeAssessmentStatus::FilingPending
        ) | (
            FinancialIncomeAssessmentStatus::FilingPending,
            FinancialIncomeAssessmentStatus::Filed
        )
    ) {
        Ok(())
    } else {
        Err(AnnualTaxError::InvalidStateTransition)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilingCashPlanInput {
    pub current_status: FinancialIncomeAssessmentStatus,
    pub scheduled_filing_date: Date,
    pub execution_date: Date,
    pub additional_tax_krw: i64,
    pub refund_krw: i64,
    pub wallet_cash_krw: i64,
    pub aggregate_debt_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilingNoMovementReason {
    ZeroTaxDue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilingMovement {
    Refund {
        wallet_credit_krw: i64,
    },
    AdditionalTax {
        wallet_debit_krw: i64,
        aggregate_debt_increase_krw: i64,
    },
    NoMovement {
        reason: FilingNoMovementReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FilingCashPlan {
    pub next_status: FinancialIncomeAssessmentStatus,
    pub wallet_cash_krw: i64,
    pub aggregate_debt_krw: i64,
    pub movement: FilingMovement,
}

/// Plans filing cash effects; callers persist real postings only for non-empty movements.
pub fn plan_filing_cash(input: &FilingCashPlanInput) -> Result<FilingCashPlan, AnnualTaxError> {
    validate_assessment_transition(input.current_status, FinancialIncomeAssessmentStatus::Filed)?;
    if input.execution_date != input.scheduled_filing_date {
        return Err(AnnualTaxError::InvalidFilingDate);
    }
    validate_money_values(&[
        input.additional_tax_krw,
        input.refund_krw,
        input.wallet_cash_krw,
        input.aggregate_debt_krw,
    ])?;
    if input.additional_tax_krw > 0 && input.refund_krw > 0 {
        return Err(AnnualTaxError::InvalidSettlementAmounts);
    }

    let (wallet_cash_krw, aggregate_debt_krw, movement) = if input.refund_krw > 0 {
        let next_wallet = checked_add_money(input.wallet_cash_krw, input.refund_krw)?;
        (
            next_wallet,
            input.aggregate_debt_krw,
            FilingMovement::Refund {
                wallet_credit_krw: input.refund_krw,
            },
        )
    } else if input.additional_tax_krw > 0 {
        let wallet_debit_krw = input.wallet_cash_krw.min(input.additional_tax_krw);
        let aggregate_debt_increase_krw = input
            .additional_tax_krw
            .checked_sub(wallet_debit_krw)
            .ok_or(AnnualTaxError::ArithmeticOverflow)?;
        let next_wallet = input
            .wallet_cash_krw
            .checked_sub(wallet_debit_krw)
            .ok_or(AnnualTaxError::ArithmeticOverflow)?;
        let next_debt = checked_add_money(input.aggregate_debt_krw, aggregate_debt_increase_krw)?;
        (
            next_wallet,
            next_debt,
            FilingMovement::AdditionalTax {
                wallet_debit_krw,
                aggregate_debt_increase_krw,
            },
        )
    } else {
        (
            input.wallet_cash_krw,
            input.aggregate_debt_krw,
            FilingMovement::NoMovement {
                reason: FilingNoMovementReason::ZeroTaxDue,
            },
        )
    };

    Ok(FilingCashPlan {
        next_status: FinancialIncomeAssessmentStatus::Filed,
        wallet_cash_krw,
        aggregate_debt_krw,
        movement,
    })
}

#[derive(Debug, Clone, Copy)]
struct AnnualSourceTotals {
    gross_financial_income_krw: i64,
    withheld_income_tax_krw: i64,
    withheld_local_income_tax_krw: i64,
}

#[derive(Debug, Clone, Copy)]
struct SettlementAmounts {
    additional_tax_krw: i64,
    refund_krw: i64,
}

#[derive(Debug, Clone, Copy)]
enum TaxKind {
    Income,
    LocalIncome,
}

fn validate_rate(rate_ppm: i64) -> Result<(), AnnualTaxError> {
    if rate_ppm <= 0 || i128::from(rate_ppm) > RATE_SCALE_PPM {
        Err(AnnualTaxError::InvalidRate)
    } else {
        Ok(())
    }
}

fn validate_brackets(brackets: &[ProgressiveTaxBracket]) -> Result<(), AnnualTaxError> {
    if brackets.is_empty() {
        return Err(AnnualTaxError::MissingEndlessBracket);
    }
    if brackets
        .windows(2)
        .any(|pair| pair[1].lower_bound_krw < pair[0].lower_bound_krw)
    {
        return Err(AnnualTaxError::UnsortedBrackets);
    }

    let last_index = brackets
        .len()
        .checked_sub(1)
        .ok_or(AnnualTaxError::MissingEndlessBracket)?;
    for (index, bracket) in brackets.iter().enumerate() {
        validate_rate(bracket.rate_ppm)?;
        if bracket.lower_bound_krw < 0
            || bracket
                .upper_bound_krw
                .is_some_and(|upper| upper <= bracket.lower_bound_krw)
        {
            return Err(AnnualTaxError::InvalidBracketRange);
        }
        if bracket.upper_bound_krw.is_none() && index != last_index {
            return Err(AnnualTaxError::EndlessBracketNotLast);
        }
    }
    if brackets[last_index].upper_bound_krw.is_some() {
        return Err(AnnualTaxError::MissingEndlessBracket);
    }
    if brackets[0].lower_bound_krw != 0 {
        return Err(AnnualTaxError::NonContiguousBrackets);
    }

    for pair in brackets.windows(2) {
        let previous_upper = pair[0]
            .upper_bound_krw
            .ok_or(AnnualTaxError::EndlessBracketNotLast)?;
        if pair[1].lower_bound_krw < previous_upper {
            return Err(AnnualTaxError::OverlappingBrackets);
        }
        if pair[1].lower_bound_krw > previous_upper {
            return Err(AnnualTaxError::NonContiguousBrackets);
        }
    }
    Ok(())
}

fn validate_source_rates(rates: &[FinancialIncomeSourceRate]) -> Result<(), AnnualTaxError> {
    let mut seen = [false; FinancialIncomeSource::ALL.len()];
    for rate in rates {
        validate_rate(rate.income_tax_rate_ppm)?;
        validate_rate(rate.local_income_tax_rate_ppm)?;
        let index = rate.source.index();
        if seen[index] {
            return Err(AnnualTaxError::DuplicateSourceRate);
        }
        seen[index] = true;
    }
    if seen.into_iter().any(|present| !present) {
        return Err(AnnualTaxError::MissingSourceRate);
    }
    Ok(())
}

fn validate_annual_date(policy: AnnualFilingDatePolicy) -> Result<(), AnnualTaxError> {
    let month = Month::try_from(policy.month).map_err(|_| AnnualTaxError::InvalidDate)?;
    Date::from_calendar_date(2001, month, policy.day)
        .map(|_| ())
        .map_err(|_| AnnualTaxError::InvalidDate)
}

fn validate_finalization_date(tax_year: i32, date: Date) -> Result<(), AnnualTaxError> {
    let next_year = tax_year
        .checked_add(1)
        .ok_or(AnnualTaxError::InvalidFinalizationDate)?;
    let expected = Date::from_calendar_date(next_year, Month::January, 1)
        .map_err(|_| AnnualTaxError::InvalidFinalizationDate)?;
    if date == expected {
        Ok(())
    } else {
        Err(AnnualTaxError::InvalidFinalizationDate)
    }
}

fn filing_date(tax_year: i32, policy: AnnualFilingDatePolicy) -> Result<Date, AnnualTaxError> {
    let year = tax_year.checked_add(1).ok_or(AnnualTaxError::InvalidDate)?;
    let month = Month::try_from(policy.month).map_err(|_| AnnualTaxError::InvalidDate)?;
    Date::from_calendar_date(year, month, policy.day).map_err(|_| AnnualTaxError::InvalidDate)
}

fn validate_money_values(values: &[i64]) -> Result<(), AnnualTaxError> {
    if values.iter().any(|value| *value < 0) {
        Err(AnnualTaxError::InvalidMoney)
    } else {
        Ok(())
    }
}

fn checked_add_money(left: i64, right: i64) -> Result<i64, AnnualTaxError> {
    checked_i128_to_i64(
        i128::from(left)
            .checked_add(i128::from(right))
            .ok_or(AnnualTaxError::ArithmeticOverflow)?,
    )
}

fn checked_i128_to_i64(value: i128) -> Result<i64, AnnualTaxError> {
    i64::try_from(value).map_err(|_| AnnualTaxError::ArithmeticOverflow)
}

fn calculate_rate_tax(amount_krw: i64, rate_ppm: i64) -> Result<i64, AnnualTaxError> {
    if amount_krw < 0 {
        return Err(AnnualTaxError::InvalidMoney);
    }
    validate_rate(rate_ppm)?;
    checked_i128_to_i64(
        i128::from(amount_krw)
            .checked_mul(i128::from(rate_ppm))
            .ok_or(AnnualTaxError::ArithmeticOverflow)?
            .checked_div(RATE_SCALE_PPM)
            .ok_or(AnnualTaxError::ArithmeticOverflow)?,
    )
}

fn sum_source_years(
    source_years: &[FinancialIncomeSourceYear],
) -> Result<AnnualSourceTotals, AnnualTaxError> {
    let mut seen = [false; FinancialIncomeSource::ALL.len()];
    let mut gross_financial_income_krw = 0_i128;
    let mut withheld_income_tax_krw = 0_i128;
    let mut withheld_local_income_tax_krw = 0_i128;

    for source_year in source_years {
        validate_money_values(&[
            source_year.gross_income_krw,
            source_year.withheld_income_tax_krw,
            source_year.withheld_local_income_tax_krw,
        ])?;
        let index = source_year.source.index();
        if seen[index] {
            return Err(AnnualTaxError::DuplicateSourceYear);
        }
        seen[index] = true;
        gross_financial_income_krw = gross_financial_income_krw
            .checked_add(i128::from(source_year.gross_income_krw))
            .ok_or(AnnualTaxError::ArithmeticOverflow)?;
        withheld_income_tax_krw = withheld_income_tax_krw
            .checked_add(i128::from(source_year.withheld_income_tax_krw))
            .ok_or(AnnualTaxError::ArithmeticOverflow)?;
        withheld_local_income_tax_krw = withheld_local_income_tax_krw
            .checked_add(i128::from(source_year.withheld_local_income_tax_krw))
            .ok_or(AnnualTaxError::ArithmeticOverflow)?;
    }

    Ok(AnnualSourceTotals {
        gross_financial_income_krw: checked_i128_to_i64(gross_financial_income_krw)?,
        withheld_income_tax_krw: checked_i128_to_i64(withheld_income_tax_krw)?,
        withheld_local_income_tax_krw: checked_i128_to_i64(withheld_local_income_tax_krw)?,
    })
}

fn calculate_source_tax(
    policy: &AnnualFinancialIncomeTaxPolicy,
    source_years: &[FinancialIncomeSourceYear],
    tax_kind: TaxKind,
) -> Result<i64, AnnualTaxError> {
    let mut total = 0_i128;
    for source_year in source_years {
        let source_rate = policy.source_rate(source_year.source)?;
        let rate_ppm = match tax_kind {
            TaxKind::Income => source_rate.income_tax_rate_ppm,
            TaxKind::LocalIncome => source_rate.local_income_tax_rate_ppm,
        };
        let source_tax = calculate_rate_tax(source_year.gross_income_krw, rate_ppm)?;
        total = total
            .checked_add(i128::from(source_tax))
            .ok_or(AnnualTaxError::ArithmeticOverflow)?;
    }
    checked_i128_to_i64(total)
}

fn apply_credit(selected_tax_krw: i64, credit_krw: i64) -> Result<i64, AnnualTaxError> {
    validate_money_values(&[selected_tax_krw, credit_krw])?;
    if credit_krw >= selected_tax_krw {
        Ok(0)
    } else {
        selected_tax_krw
            .checked_sub(credit_krw)
            .ok_or(AnnualTaxError::ArithmeticOverflow)
    }
}

fn calculate_settlement(
    final_income_tax_krw: i64,
    final_local_income_tax_krw: i64,
    withheld_income_tax_krw: i64,
    withheld_local_income_tax_krw: i64,
) -> Result<SettlementAmounts, AnnualTaxError> {
    let final_tax = i128::from(final_income_tax_krw)
        .checked_add(i128::from(final_local_income_tax_krw))
        .ok_or(AnnualTaxError::ArithmeticOverflow)?;
    let withheld_tax = i128::from(withheld_income_tax_krw)
        .checked_add(i128::from(withheld_local_income_tax_krw))
        .ok_or(AnnualTaxError::ArithmeticOverflow)?;
    let difference = final_tax
        .checked_sub(withheld_tax)
        .ok_or(AnnualTaxError::ArithmeticOverflow)?;

    if difference > 0 {
        Ok(SettlementAmounts {
            additional_tax_krw: checked_i128_to_i64(difference)?,
            refund_krw: 0,
        })
    } else if difference < 0 {
        let refund_krw = difference
            .checked_neg()
            .ok_or(AnnualTaxError::ArithmeticOverflow)?;
        Ok(SettlementAmounts {
            additional_tax_krw: 0,
            refund_krw: checked_i128_to_i64(refund_krw)?,
        })
    } else {
        Ok(SettlementAmounts {
            additional_tax_krw: 0,
            refund_krw: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::{Date, Month};

    const TAX_YEAR: i32 = 2026;

    fn given_date(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("유효한 테스트 날짜여야 한다")
    }

    fn given_income_brackets() -> Vec<ProgressiveTaxBracket> {
        vec![
            ProgressiveTaxBracket {
                lower_bound_krw: 0,
                upper_bound_krw: Some(14_000_000),
                rate_ppm: 60_000,
            },
            ProgressiveTaxBracket {
                lower_bound_krw: 14_000_000,
                upper_bound_krw: Some(50_000_000),
                rate_ppm: 150_000,
            },
            ProgressiveTaxBracket {
                lower_bound_krw: 50_000_000,
                upper_bound_krw: None,
                rate_ppm: 240_000,
            },
        ]
    }

    fn given_local_brackets() -> Vec<ProgressiveTaxBracket> {
        vec![
            ProgressiveTaxBracket {
                lower_bound_krw: 0,
                upper_bound_krw: Some(14_000_000),
                rate_ppm: 6_000,
            },
            ProgressiveTaxBracket {
                lower_bound_krw: 14_000_000,
                upper_bound_krw: Some(50_000_000),
                rate_ppm: 15_000,
            },
            ProgressiveTaxBracket {
                lower_bound_krw: 50_000_000,
                upper_bound_krw: None,
                rate_ppm: 24_000,
            },
        ]
    }

    fn given_source_rates(
        income_tax_rate_ppm: i64,
        local_income_tax_rate_ppm: i64,
    ) -> Vec<FinancialIncomeSourceRate> {
        FinancialIncomeSource::ALL
            .into_iter()
            .map(|source| FinancialIncomeSourceRate {
                source,
                income_tax_rate_ppm,
                local_income_tax_rate_ppm,
            })
            .collect()
    }

    fn given_policy() -> AnnualFinancialIncomeTaxPolicy {
        AnnualFinancialIncomeTaxPolicy {
            comprehensive_threshold_krw: 20_000_000,
            general_income_tax_rate_ppm: 140_000,
            general_local_income_tax_rate_ppm: 14_000,
            income_tax_brackets: given_income_brackets(),
            local_income_tax_brackets: given_local_brackets(),
            source_rates: given_source_rates(140_000, 14_000),
            filing_date: AnnualFilingDatePolicy { month: 5, day: 31 },
        }
    }

    fn given_source_year(
        source: FinancialIncomeSource,
        gross_income_krw: i64,
        withheld_income_tax_krw: i64,
        withheld_local_income_tax_krw: i64,
    ) -> FinancialIncomeSourceYear {
        FinancialIncomeSourceYear {
            source,
            gross_income_krw,
            withheld_income_tax_krw,
            withheld_local_income_tax_krw,
        }
    }

    fn given_finalize_input(
        gross_income_krw: i64,
        withheld_income_tax_krw: i64,
        withheld_local_income_tax_krw: i64,
    ) -> AnnualAssessmentFinalizeInput {
        AnnualAssessmentFinalizeInput {
            tax_year: TAX_YEAR,
            finalization_date: given_date(2027, Month::January, 1),
            current_status: FinancialIncomeAssessmentStatus::Open,
            source_years: vec![given_source_year(
                FinancialIncomeSource::DepositInterest,
                gross_income_krw,
                withheld_income_tax_krw,
                withheld_local_income_tax_krw,
            )],
            other_comprehensive_income_krw: 0,
            credits: TaxCredits::default(),
        }
    }

    fn when_finalizing(
        policy: &AnnualFinancialIncomeTaxPolicy,
        input: &AnnualAssessmentFinalizeInput,
    ) -> Result<AnnualAssessmentDraft, AnnualTaxError> {
        finalize_annual_assessment(policy, input)
    }

    mod context_strict_policy_is_validated {
        use super::*;

        #[test]
        fn given_complete_policy_when_validated_then_it_is_accepted() {
            let policy = given_policy();

            let result = policy.validate();

            assert_eq!(result, Ok(()));
        }

        #[test]
        fn given_zero_threshold_when_validated_then_it_is_rejected() {
            let mut policy = given_policy();
            policy.comprehensive_threshold_krw = 0;

            let result = policy.validate();

            assert_eq!(result, Err(AnnualTaxError::InvalidThreshold));
        }

        #[test]
        fn given_negative_threshold_when_validated_then_it_is_rejected() {
            let mut policy = given_policy();
            policy.comprehensive_threshold_krw = -1;

            let result = policy.validate();

            assert_eq!(result, Err(AnnualTaxError::InvalidThreshold));
        }

        #[test]
        fn given_zero_rate_when_validated_then_it_is_rejected() {
            let mut policy = given_policy();
            policy.general_income_tax_rate_ppm = 0;

            let result = policy.validate();

            assert_eq!(result, Err(AnnualTaxError::InvalidRate));
        }

        #[test]
        fn given_negative_rate_when_validated_then_it_is_rejected() {
            let mut policy = given_policy();
            policy.source_rates[0].local_income_tax_rate_ppm = -1;

            let result = policy.validate();

            assert_eq!(result, Err(AnnualTaxError::InvalidRate));
        }

        #[test]
        fn given_overlapping_brackets_when_validated_then_they_are_rejected() {
            let mut policy = given_policy();
            policy.income_tax_brackets[1].lower_bound_krw = 13_999_999;

            let result = policy.validate();

            assert_eq!(result, Err(AnnualTaxError::OverlappingBrackets));
        }

        #[test]
        fn given_unsorted_brackets_when_validated_then_they_are_rejected() {
            let mut policy = given_policy();
            policy.income_tax_brackets.swap(0, 1);

            let result = policy.validate();

            assert_eq!(result, Err(AnnualTaxError::UnsortedBrackets));
        }

        #[test]
        fn given_a_nonfinal_endless_bracket_when_validated_then_it_is_rejected() {
            let mut policy = given_policy();
            policy.income_tax_brackets[1].upper_bound_krw = None;

            let result = policy.validate();

            assert_eq!(result, Err(AnnualTaxError::EndlessBracketNotLast));
        }

        #[test]
        fn given_no_endless_bracket_when_validated_then_it_is_rejected() {
            let mut policy = given_policy();
            policy.income_tax_brackets[2].upper_bound_krw = Some(1_000_000_000);

            let result = policy.validate();

            assert_eq!(result, Err(AnnualTaxError::MissingEndlessBracket));
        }

        #[test]
        fn given_a_duplicate_source_rate_when_validated_then_it_is_rejected() {
            let mut policy = given_policy();
            policy.source_rates[1].source = FinancialIncomeSource::CmaInterest;

            let result = policy.validate();

            assert_eq!(result, Err(AnnualTaxError::DuplicateSourceRate));
        }

        #[test]
        fn given_a_missing_source_rate_when_validated_then_it_is_rejected() {
            let mut policy = given_policy();
            policy.source_rates.pop();

            let result = policy.validate();

            assert_eq!(result, Err(AnnualTaxError::MissingSourceRate));
        }

        #[test]
        fn given_an_invalid_annual_filing_date_when_validated_then_it_is_rejected() {
            let mut policy = given_policy();
            policy.filing_date = AnnualFilingDatePolicy { month: 2, day: 30 };

            let result = policy.validate();

            assert_eq!(result, Err(AnnualTaxError::InvalidDate));
        }
    }

    mod context_source_contract_is_fixed {
        use super::*;

        #[test]
        fn given_every_source_when_serialized_then_the_wire_values_are_exact() {
            let values = FinancialIncomeSource::ALL
                .map(|source| serde_json::to_value(source).expect("source를 직렬화해야 한다"));

            assert_eq!(
                values,
                [
                    serde_json::json!("cmaInterest"),
                    serde_json::json!("depositInterest"),
                    serde_json::json!("bondCoupon"),
                    serde_json::json!("llxDistribution"),
                    serde_json::json!("isaEarlyClose"),
                ]
            );
        }

        #[test]
        fn given_every_status_when_serialized_then_the_wire_values_are_exact() {
            let values = [
                FinancialIncomeAssessmentStatus::Open,
                FinancialIncomeAssessmentStatus::FinalizedNoFiling,
                FinancialIncomeAssessmentStatus::FilingPending,
                FinancialIncomeAssessmentStatus::Filed,
            ]
            .map(|status| serde_json::to_value(status).expect("status를 직렬화해야 한다"));

            assert_eq!(
                values,
                [
                    serde_json::json!("open"),
                    serde_json::json!("finalizedNoFiling"),
                    serde_json::json!("filingPending"),
                    serde_json::json!("filed"),
                ]
            );
        }
    }

    mod context_source_year_income_is_accumulated {
        use super::*;

        #[test]
        fn given_two_source_events_when_accumulated_then_each_amount_is_added() {
            let mut year = FinancialIncomeSourceYear::zero(FinancialIncomeSource::CmaInterest);
            let first = FinancialIncomeAccrual {
                source: FinancialIncomeSource::CmaInterest,
                gross_income_krw: 1_000,
                withheld_income_tax_krw: 140,
                withheld_local_income_tax_krw: 14,
            };
            let second = FinancialIncomeAccrual {
                source: FinancialIncomeSource::CmaInterest,
                gross_income_krw: 2_000,
                withheld_income_tax_krw: 280,
                withheld_local_income_tax_krw: 28,
            };

            year.accrue(first).expect("첫 누계가 통과해야 한다");
            let result = year.accrue(second);

            assert_eq!(result, Ok(()));
            assert_eq!(
                year,
                given_source_year(FinancialIncomeSource::CmaInterest, 3_000, 420, 42)
            );
        }

        #[test]
        fn given_a_different_source_when_accumulated_then_it_is_rejected() {
            let mut year = FinancialIncomeSourceYear::zero(FinancialIncomeSource::CmaInterest);
            let accrual = FinancialIncomeAccrual {
                source: FinancialIncomeSource::BondCoupon,
                gross_income_krw: 1_000,
                withheld_income_tax_krw: 140,
                withheld_local_income_tax_krw: 14,
            };

            let result = year.accrue(accrual);

            assert_eq!(result, Err(AnnualTaxError::SourceMismatch));
        }

        #[test]
        fn given_a_negative_amount_when_accumulated_then_it_is_rejected() {
            let mut year = FinancialIncomeSourceYear::zero(FinancialIncomeSource::CmaInterest);
            let accrual = FinancialIncomeAccrual {
                source: FinancialIncomeSource::CmaInterest,
                gross_income_krw: -1,
                withheld_income_tax_krw: 0,
                withheld_local_income_tax_krw: 0,
            };

            let result = year.accrue(accrual);

            assert_eq!(result, Err(AnnualTaxError::InvalidMoney));
        }

        #[test]
        fn given_a_db_range_sum_when_accumulated_then_overflow_is_rejected() {
            let mut year = given_source_year(FinancialIncomeSource::CmaInterest, i64::MAX, 0, 0);
            let accrual = FinancialIncomeAccrual {
                source: FinancialIncomeSource::CmaInterest,
                gross_income_krw: 1,
                withheld_income_tax_krw: 0,
                withheld_local_income_tax_krw: 0,
            };

            let result = year.accrue(accrual);

            assert_eq!(result, Err(AnnualTaxError::ArithmeticOverflow));
        }
    }

    mod context_basic_tax_is_calculated_by_bracket_slice {
        use super::*;

        #[test]
        fn given_fractional_won_in_each_slice_when_calculated_then_each_slice_is_floored() {
            let brackets = vec![
                ProgressiveTaxBracket {
                    lower_bound_krw: 0,
                    upper_bound_krw: Some(1),
                    rate_ppm: 500_000,
                },
                ProgressiveTaxBracket {
                    lower_bound_krw: 1,
                    upper_bound_krw: None,
                    rate_ppm: 500_000,
                },
            ];

            let result = calculate_basic_tax(2, &brackets);

            assert_eq!(result, Ok(0));
        }
    }

    mod context_financial_income_does_not_exceed_the_threshold {
        use super::*;

        #[test]
        fn given_19_999_999_when_finalized_then_no_filing_is_required() {
            let policy = given_policy();
            let input = given_finalize_input(19_999_999, 2_799_999, 279_999);

            let result =
                when_finalizing(&policy, &input).expect("기준 이하 소득은 확정되어야 한다");

            assert_eq!(
                result.status,
                FinancialIncomeAssessmentStatus::FinalizedNoFiling
            );
            assert_eq!(result.additional_tax_krw, 0);
            assert_eq!(result.refund_krw, 0);
        }

        #[test]
        fn given_20_000_000_when_finalized_then_no_filing_is_required() {
            let policy = given_policy();
            let input = given_finalize_input(20_000_000, 2_800_000, 280_000);

            let result =
                when_finalizing(&policy, &input).expect("기준과 같은 소득은 확정되어야 한다");

            assert_eq!(
                result.status,
                FinancialIncomeAssessmentStatus::FinalizedNoFiling
            );
            assert_eq!(result.income_tax_formula_a_krw, 2_800_000);
            assert_eq!(result.income_tax_formula_b_krw, 2_800_000);
            assert_eq!(result.final_income_tax_krw, 2_800_000);
            assert_eq!(result.local_income_tax_formula_a_krw, 280_000);
            assert_eq!(result.local_income_tax_formula_b_krw, 280_000);
            assert_eq!(result.final_local_income_tax_krw, 280_000);
            assert_eq!(result.filing_date, None);
        }
    }

    mod context_financial_income_exceeds_the_threshold {
        use super::*;

        #[test]
        fn given_20_000_001_when_finalized_then_next_may_filing_is_pending() {
            let policy = given_policy();
            let input = given_finalize_input(20_000_001, 2_800_000, 280_000);

            let result =
                when_finalizing(&policy, &input).expect("기준 초과 소득은 확정되어야 한다");

            assert_eq!(
                result.status,
                FinancialIncomeAssessmentStatus::FilingPending
            );
            assert_eq!(result.filing_date, Some(given_date(2027, Month::May, 31)));
        }

        #[test]
        fn given_lower_source_rates_when_compared_then_formula_a_wins_income_tax() {
            let mut policy = given_policy();
            policy.source_rates = given_source_rates(100_000, 10_000);
            let input = given_finalize_input(30_000_000, 0, 0);

            let result = when_finalizing(&policy, &input).expect("비교세액을 계산해야 한다");

            assert_eq!(result.income_tax_formula_a_krw, 3_400_000);
            assert_eq!(result.income_tax_formula_b_krw, 3_000_000);
            assert_eq!(result.final_income_tax_krw, 3_400_000);
        }

        #[test]
        fn given_higher_source_rates_when_compared_then_formula_b_wins_income_tax() {
            let mut policy = given_policy();
            policy.source_rates = given_source_rates(200_000, 20_000);
            let input = given_finalize_input(30_000_000, 0, 0);

            let result = when_finalizing(&policy, &input).expect("비교세액을 계산해야 한다");

            assert_eq!(result.income_tax_formula_a_krw, 3_400_000);
            assert_eq!(result.income_tax_formula_b_krw, 6_000_000);
            assert_eq!(result.final_income_tax_krw, 6_000_000);
        }

        #[test]
        fn given_different_source_rates_when_compared_then_each_tax_selects_independently() {
            let mut policy = given_policy();
            policy.source_rates = given_source_rates(100_000, 20_000);
            let input = given_finalize_input(30_000_000, 0, 0);

            let result = when_finalizing(&policy, &input).expect("세목별 비교세액을 계산해야 한다");

            assert_eq!(result.final_income_tax_krw, result.income_tax_formula_a_krw);
            assert_eq!(
                result.final_local_income_tax_krw,
                result.local_income_tax_formula_b_krw
            );
        }

        #[test]
        fn given_tax_credits_when_finalized_then_they_reduce_each_selected_tax_to_zero_at_most() {
            let mut policy = given_policy();
            policy.source_rates = given_source_rates(100_000, 10_000);
            let mut input = given_finalize_input(30_000_000, 0, 0);
            input.credits = TaxCredits {
                income_tax_credit_krw: 400_000,
                local_income_tax_credit_krw: 40_000,
            };

            let result = when_finalizing(&policy, &input).expect("세액공제를 적용해야 한다");

            assert_eq!(result.final_income_tax_krw, 3_000_000);
            assert_eq!(result.final_local_income_tax_krw, 300_000);
        }

        #[test]
        fn given_withholding_below_final_tax_when_finalized_then_only_additional_tax_is_positive() {
            let policy = given_policy();
            let input = given_finalize_input(30_000_000, 1_000_000, 100_000);

            let result = when_finalizing(&policy, &input).expect("추가세액을 계산해야 한다");

            assert!(result.additional_tax_krw > 0);
            assert_eq!(result.refund_krw, 0);
        }

        #[test]
        fn given_withholding_above_final_tax_when_finalized_then_only_refund_is_positive() {
            let policy = given_policy();
            let input = given_finalize_input(30_000_000, 5_000_000, 500_000);

            let result = when_finalizing(&policy, &input).expect("환급액을 계산해야 한다");

            assert_eq!(result.additional_tax_krw, 0);
            assert!(result.refund_krw > 0);
        }

        #[test]
        fn given_withholding_equal_to_final_tax_when_finalized_then_both_settlement_amounts_are_zero()
         {
            let policy = given_policy();
            let input = given_finalize_input(30_000_000, 4_200_000, 420_000);

            let result = when_finalizing(&policy, &input).expect("0원 신고세액을 계산해야 한다");

            assert_eq!(result.additional_tax_krw, 0);
            assert_eq!(result.refund_krw, 0);
        }
    }

    mod context_assessment_state_and_dates_are_validated {
        use super::*;

        #[test]
        fn given_open_assessment_when_finalized_without_filing_then_transition_is_valid() {
            let result = validate_assessment_transition(
                FinancialIncomeAssessmentStatus::Open,
                FinancialIncomeAssessmentStatus::FinalizedNoFiling,
            );

            assert_eq!(result, Ok(()));
        }

        #[test]
        fn given_open_assessment_when_filing_becomes_pending_then_transition_is_valid() {
            let result = validate_assessment_transition(
                FinancialIncomeAssessmentStatus::Open,
                FinancialIncomeAssessmentStatus::FilingPending,
            );

            assert_eq!(result, Ok(()));
        }

        #[test]
        fn given_pending_assessment_when_filed_then_transition_is_valid() {
            let result = validate_assessment_transition(
                FinancialIncomeAssessmentStatus::FilingPending,
                FinancialIncomeAssessmentStatus::Filed,
            );

            assert_eq!(result, Ok(()));
        }

        #[test]
        fn given_finalized_no_filing_when_filed_then_transition_is_rejected() {
            let result = validate_assessment_transition(
                FinancialIncomeAssessmentStatus::FinalizedNoFiling,
                FinancialIncomeAssessmentStatus::Filed,
            );

            assert_eq!(result, Err(AnnualTaxError::InvalidStateTransition));
        }

        #[test]
        fn given_a_non_january_first_date_when_finalized_then_it_is_rejected() {
            let policy = given_policy();
            let mut input = given_finalize_input(20_000_001, 0, 0);
            input.finalization_date = given_date(2026, Month::December, 31);

            let result = when_finalizing(&policy, &input);

            assert_eq!(result, Err(AnnualTaxError::InvalidFinalizationDate));
        }

        #[test]
        fn given_a_non_open_assessment_when_finalized_then_it_is_rejected() {
            let policy = given_policy();
            let mut input = given_finalize_input(20_000_001, 0, 0);
            input.current_status = FinancialIncomeAssessmentStatus::Filed;

            let result = when_finalizing(&policy, &input);

            assert_eq!(result, Err(AnnualTaxError::InvalidStateTransition));
        }
    }

    mod context_filing_cash_is_planned {
        use super::*;

        fn given_filing_input(
            additional_tax_krw: i64,
            refund_krw: i64,
            wallet_cash_krw: i64,
            aggregate_debt_krw: i64,
        ) -> FilingCashPlanInput {
            FilingCashPlanInput {
                current_status: FinancialIncomeAssessmentStatus::FilingPending,
                scheduled_filing_date: given_date(2027, Month::May, 31),
                execution_date: given_date(2027, Month::May, 31),
                additional_tax_krw,
                refund_krw,
                wallet_cash_krw,
                aggregate_debt_krw,
            }
        }

        #[test]
        fn given_a_refund_when_filed_then_it_is_credited_to_the_wallet() {
            let input = given_filing_input(0, 500, 1_000, 200);

            let result = plan_filing_cash(&input).expect("환급 계획을 만들어야 한다");

            assert_eq!(result.wallet_cash_krw, 1_500);
            assert_eq!(result.aggregate_debt_krw, 200);
            assert_eq!(
                result.movement,
                FilingMovement::Refund {
                    wallet_credit_krw: 500
                }
            );
            assert_eq!(result.next_status, FinancialIncomeAssessmentStatus::Filed);
        }

        #[test]
        fn given_wallet_covers_additional_tax_when_filed_then_only_wallet_cash_is_used() {
            let input = given_filing_input(700, 0, 1_000, 200);

            let result = plan_filing_cash(&input).expect("현금 납부 계획을 만들어야 한다");

            assert_eq!(result.wallet_cash_krw, 300);
            assert_eq!(result.aggregate_debt_krw, 200);
            assert_eq!(
                result.movement,
                FilingMovement::AdditionalTax {
                    wallet_debit_krw: 700,
                    aggregate_debt_increase_krw: 0,
                }
            );
        }

        #[test]
        fn given_wallet_is_short_when_filed_then_the_remainder_becomes_aggregate_debt() {
            let input = given_filing_input(1_200, 0, 1_000, 200);

            let result = plan_filing_cash(&input).expect("부족분 계획을 만들어야 한다");

            assert_eq!(result.wallet_cash_krw, 0);
            assert_eq!(result.aggregate_debt_krw, 400);
            assert_eq!(
                result.movement,
                FilingMovement::AdditionalTax {
                    wallet_debit_krw: 1_000,
                    aggregate_debt_increase_krw: 200,
                }
            );
        }

        #[test]
        fn given_zero_due_when_filed_then_no_movement_is_planned() {
            let input = given_filing_input(0, 0, 1_000, 200);

            let result = plan_filing_cash(&input).expect("0원 신고 계획을 만들어야 한다");

            assert_eq!(result.wallet_cash_krw, 1_000);
            assert_eq!(result.aggregate_debt_krw, 200);
            assert_eq!(
                result.movement,
                FilingMovement::NoMovement {
                    reason: FilingNoMovementReason::ZeroTaxDue
                }
            );
        }

        #[test]
        fn given_nonpending_assessment_when_filed_then_it_is_rejected() {
            let mut input = given_filing_input(0, 0, 1_000, 200);
            input.current_status = FinancialIncomeAssessmentStatus::Open;

            let result = plan_filing_cash(&input);

            assert_eq!(result, Err(AnnualTaxError::InvalidStateTransition));
        }

        #[test]
        fn given_a_different_execution_date_when_filed_then_it_is_rejected() {
            let mut input = given_filing_input(0, 0, 1_000, 200);
            input.execution_date = given_date(2027, Month::May, 30);

            let result = plan_filing_cash(&input);

            assert_eq!(result, Err(AnnualTaxError::InvalidFilingDate));
        }

        #[test]
        fn given_both_additional_tax_and_refund_when_filed_then_it_is_rejected() {
            let input = given_filing_input(1, 1, 1_000, 200);

            let result = plan_filing_cash(&input);

            assert_eq!(result, Err(AnnualTaxError::InvalidSettlementAmounts));
        }

        #[test]
        fn given_wallet_refund_exceeds_db_range_when_filed_then_overflow_is_rejected() {
            let input = given_filing_input(0, 1, i64::MAX, 0);

            let result = plan_filing_cash(&input);

            assert_eq!(result, Err(AnnualTaxError::ArithmeticOverflow));
        }

        #[test]
        fn given_debt_shortage_exceeds_db_range_when_filed_then_overflow_is_rejected() {
            let input = given_filing_input(1, 0, 0, i64::MAX);

            let result = plan_filing_cash(&input);

            assert_eq!(result, Err(AnnualTaxError::ArithmeticOverflow));
        }
    }

    mod context_annual_calculation_range_is_checked {
        use super::*;

        #[test]
        fn given_financial_and_other_income_exceed_db_range_when_finalized_then_overflow_is_rejected()
         {
            let policy = given_policy();
            let mut input = given_finalize_input(i64::MAX, 0, 0);
            input.other_comprehensive_income_krw = i64::MAX;

            let result = when_finalizing(&policy, &input);

            assert_eq!(result, Err(AnnualTaxError::ArithmeticOverflow));
        }
    }
}
