//! Transaction-scoped persistence helpers for M2-D annual financial-income tax (§8.6).

use anyhow::{Context, Result, ensure};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{MySql, Transaction};
use time::{Date, Month};

use super::mysql::write_ledger_transaction;
use crate::finance::{
    AnnualAssessmentDraft, AnnualAssessmentFinalizeInput, AnnualFilingDatePolicy,
    AnnualFinancialIncomeTaxPolicy, FilingCashPlan, FilingCashPlanInput, FilingMovement,
    FinanceRules, FinancialIncomeAccrual, FinancialIncomeAssessmentStatus, FinancialIncomeSource,
    FinancialIncomeSourceRate, FinancialIncomeSourceYear, LedgerAccountCode, LedgerPosting,
    LedgerSource, LedgerSourceKind, LedgerTransaction, LedgerTransactionDraft,
    ProgressiveTaxBracket, ResourceId, RunId, RunPolicyContext, TaxCredits,
    finalize_annual_assessment, plan_filing_cash,
};

const ANNUAL_TAX_DOMAIN: &str = "tax";
const ANNUAL_TAX_RULE_KEY: &str = "annualFinancialIncomeAssessment";
const CORPORATION_TAX_DOMAIN: &str = "corporation";
const CORPORATION_DIVIDEND_RULE_KEY: &str = "residentDividendWithholding";
const FILING_SETTLEMENT_KIND: &str = "financialIncomeFiling";
const FILING_SETTLEMENT_SOURCE_KIND: &str = "taxYear";
const FILING_SETTLEMENT_OCCURRENCE: u32 = 1;
const FILING_PAYLOAD_SCHEMA_VERSION: u8 = 1;
const ZERO_TAX_DUE_REASON: &str = "zeroTaxDue";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AnnualTaxRunContext {
    pub save_id: u64,
    pub run_revision: u32,
    pub policy_set_id: u64,
    pub game_day: u32,
    pub market_date: Date,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnnualTaxSourceState {
    pub source: FinancialIncomeSource,
    pub gross_financial_income_krw: i64,
    pub withheld_income_tax_krw: i64,
    pub withheld_local_income_tax_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnnualTaxCalculatedState {
    pub comparison_a_income_tax_krw: i64,
    pub comparison_a_local_income_tax_krw: i64,
    pub comparison_b_income_tax_krw: i64,
    pub comparison_b_local_income_tax_krw: i64,
    pub assessed_income_tax_krw: i64,
    pub assessed_local_income_tax_krw: i64,
    pub additional_tax_krw: i64,
    pub refund_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnualTaxAssessmentState {
    NotApplicable,
    Open,
    FinalizedNoFiling {
        calculated: AnnualTaxCalculatedState,
    },
    FilingPending {
        calculated: AnnualTaxCalculatedState,
        filing_due_date: Date,
    },
    Filed {
        calculated: AnnualTaxCalculatedState,
        filing_due_date: Date,
        filed_game_day: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnualTaxYearState {
    pub tax_year: u16,
    pub sources: Vec<AnnualTaxSourceState>,
    pub gross_financial_income_krw: i64,
    pub withheld_income_tax_krw: i64,
    pub withheld_local_income_tax_krw: i64,
    pub assessment: AnnualTaxAssessmentState,
}

impl AnnualTaxYearState {
    pub fn empty_not_applicable(tax_year: u16) -> Self {
        Self {
            tax_year,
            sources: Vec::new(),
            gross_financial_income_krw: 0,
            withheld_income_tax_krw: 0,
            withheld_local_income_tax_krw: 0,
            assessment: AnnualTaxAssessmentState::NotApplicable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AnnualIncomeAccrualMode {
    AggregateOnly,
    SourceTracked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AnnualTaxFilingPlan {
    pub settlement_id: u64,
    pub tax_year: u16,
    pub execution_date: Date,
    pub expected_wallet_cash_krw: i64,
    pub expected_aggregate_debt_krw: i64,
    pub cash_plan: FilingCashPlan,
    context: AnnualTaxRunContext,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AnnualTaxRuntimePolicy {
    pub policy: AnnualFinancialIncomeTaxPolicy,
    pub other_comprehensive_income_krw: i64,
    pub credits: TaxCredits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum PersistedComparisonFormula {
    IndependentMaxOfFormulaAAndB,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum PersistedCashShortageTreatment {
    InterestFreeAggregateDebt,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PersistedAnnualTaxPolicy {
    comprehensive_threshold_krw: i64,
    general_income_tax_rate_ppm: i64,
    general_local_income_tax_rate_ppm: i64,
    non_financial_comprehensive_income_krw: i64,
    income_tax_credit_krw: i64,
    local_income_tax_credit_krw: i64,
    comparison_formula: PersistedComparisonFormula,
    cash_shortage_treatment: PersistedCashShortageTreatment,
    income_tax_brackets: Vec<ProgressiveTaxBracket>,
    local_income_tax_brackets: Vec<ProgressiveTaxBracket>,
    source_rates: Vec<FinancialIncomeSourceRate>,
    filing_date: AnnualFilingDatePolicy,
}

impl PersistedAnnualTaxPolicy {
    fn into_runtime(self) -> Result<AnnualTaxRuntimePolicy> {
        let Self {
            comprehensive_threshold_krw,
            general_income_tax_rate_ppm,
            general_local_income_tax_rate_ppm,
            non_financial_comprehensive_income_krw,
            income_tax_credit_krw,
            local_income_tax_credit_krw,
            comparison_formula,
            cash_shortage_treatment,
            income_tax_brackets,
            local_income_tax_brackets,
            source_rates,
            filing_date,
        } = self;
        ensure!(
            comparison_formula == PersistedComparisonFormula::IndependentMaxOfFormulaAAndB,
            "annual-tax comparison formula is unsupported"
        );
        ensure!(
            cash_shortage_treatment == PersistedCashShortageTreatment::InterestFreeAggregateDebt,
            "annual-tax cash-shortage treatment is unsupported"
        );
        ensure!(
            non_financial_comprehensive_income_krw == 0
                && income_tax_credit_krw == 0
                && local_income_tax_credit_krw == 0,
            "M2 annual-tax non-financial income and credits must be zero"
        );

        let policy = AnnualFinancialIncomeTaxPolicy {
            comprehensive_threshold_krw,
            general_income_tax_rate_ppm,
            general_local_income_tax_rate_ppm,
            income_tax_brackets,
            local_income_tax_brackets,
            source_rates,
            filing_date,
        };
        policy
            .validate()
            .context("stored annual-tax policy violates pure policy invariants")?;
        Ok(AnnualTaxRuntimePolicy {
            policy,
            other_comprehensive_income_krw: non_financial_comprehensive_income_krw,
            credits: TaxCredits {
                income_tax_credit_krw,
                local_income_tax_credit_krw,
            },
        })
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct AnnualTaxPolicyRuleRow {
    parameters_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PersistedCorporationDividendWithholdingPolicy {
    income_tax_rate_ppm: i64,
    local_income_tax_on_income_tax_ppm: i64,
    rounding: String,
    schema_version: u8,
    supported_recipient: String,
}

pub(super) async fn read_annual_tax_policy(
    tx: &mut Transaction<'_, MySql>,
    policy_set_id: u64,
    effective_on: Date,
) -> Result<Option<AnnualTaxRuntimePolicy>> {
    let rows: Vec<AnnualTaxPolicyRuleRow> = sqlx::query_as(
        "SELECT CAST(parameters AS CHAR) AS parameters_json
         FROM policy_rule
         WHERE policy_set_id = ? AND BINARY domain = BINARY ?
           AND BINARY rule_key = BINARY ?
           AND effective_from <= ?
           AND (effective_to IS NULL OR effective_to >= ?)
         ORDER BY effective_from DESC
         LIMIT 2",
    )
    .bind(policy_set_id)
    .bind(ANNUAL_TAX_DOMAIN)
    .bind(ANNUAL_TAX_RULE_KEY)
    .bind(effective_on)
    .bind(effective_on)
    .fetch_all(&mut **tx)
    .await
    .context("failed to read the pinned annual-tax policy")?;
    ensure!(
        rows.len() <= 1,
        "pinned annual-tax policy has overlapping effective rows"
    );
    let mut runtime = rows
        .into_iter()
        .next()
        .map(|row| parse_annual_tax_policy(&row.parameters_json))
        .transpose()?;
    let Some(runtime) = runtime.as_mut() else {
        return Ok(None);
    };
    let dividend_rows: Vec<AnnualTaxPolicyRuleRow> = sqlx::query_as(
        "SELECT CAST(parameters AS CHAR) AS parameters_json
         FROM policy_rule
         WHERE policy_set_id = ? AND BINARY domain = BINARY ?
           AND BINARY rule_key = BINARY ?
           AND effective_from <= ?
           AND (effective_to IS NULL OR effective_to >= ?)
         ORDER BY effective_from DESC
         LIMIT 2",
    )
    .bind(policy_set_id)
    .bind(CORPORATION_TAX_DOMAIN)
    .bind(CORPORATION_DIVIDEND_RULE_KEY)
    .bind(effective_on)
    .bind(effective_on)
    .fetch_all(&mut **tx)
    .await
    .context("failed to read the pinned corporation-dividend withholding policy")?;
    ensure!(
        dividend_rows.len() <= 1,
        "pinned corporation-dividend policy has overlapping effective rows"
    );
    if let Some(row) = dividend_rows.into_iter().next() {
        let policy = serde_json::from_str::<PersistedCorporationDividendWithholdingPolicy>(
            &row.parameters_json,
        )
        .context("stored corporation-dividend policy is not the strict 5-field schema")?;
        ensure!(
            policy.schema_version == 1
                && policy.rounding == "floorEachTax"
                && policy.supported_recipient == "residentIndividual",
            "stored corporation-dividend policy variant is unsupported"
        );
        let local_income_tax_rate_ppm = i64::try_from(
            i128::from(policy.income_tax_rate_ppm)
                .checked_mul(i128::from(policy.local_income_tax_on_income_tax_ppm))
                .context("corporation-dividend withholding rate overflowed")?
                / 1_000_000,
        )?;
        runtime.policy.source_rates.push(FinancialIncomeSourceRate {
            source: FinancialIncomeSource::CorporationDividend,
            income_tax_rate_ppm: policy.income_tax_rate_ppm,
            local_income_tax_rate_ppm,
        });
        runtime
            .policy
            .validate()
            .context("combined annual-tax and corporation-dividend policy is invalid")?;
    }
    Ok(Some(runtime.clone()))
}

fn parse_annual_tax_policy(parameters_json: &str) -> Result<AnnualTaxRuntimePolicy> {
    serde_json::from_str::<PersistedAnnualTaxPolicy>(parameters_json)
        .context("stored annual-tax policy is not the strict 12-field schema")?
        .into_runtime()
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct AnnualTaxAssessmentReadRow {
    policy_set_id: u64,
    year_end_tax_assessment_id: Option<u64>,
    status: String,
    gross_financial_income_krw: i64,
    other_comprehensive_income_krw: i64,
    employment_taxable_income_krw: i64,
    employment_deductions_krw: i64,
    employment_final_prepaid_income_tax_krw: i64,
    employment_final_prepaid_local_income_tax_krw: i64,
    withheld_income_tax_krw: i64,
    withheld_local_income_tax_krw: i64,
    income_tax_formula_a_krw: i64,
    income_tax_formula_b_krw: i64,
    local_income_tax_formula_a_krw: i64,
    local_income_tax_formula_b_krw: i64,
    income_tax_credit_krw: i64,
    local_income_tax_credit_krw: i64,
    final_income_tax_krw: i64,
    final_local_income_tax_krw: i64,
    additional_tax_krw: i64,
    refund_krw: i64,
    finalized_on: Option<Date>,
    filing_date: Option<Date>,
    filed_on: Option<Date>,
}

/// Reads one current-run tax year and validates its aggregate, sources, and assessment together.
pub(super) async fn read_annual_tax_year(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
) -> Result<AnnualTaxYearState> {
    ensure!(tax_year > 0, "annual-tax read year must be positive");
    ensure_annual_tax_read_context(tx, context).await?;

    let effective_on = Date::from_calendar_date(i32::from(tax_year), Month::December, 31)
        .context("annual-tax read year is outside the supported date range")?;
    let runtime = read_annual_tax_policy(tx, context.policy_set_id, effective_on).await?;
    let aggregate = read_financial_income_aggregate(tx, context, tax_year)
        .await?
        .unwrap_or(FinancialIncomeAggregateRow {
            gross_financial_income_krw: 0,
            withheld_income_tax_krw: 0,
            withheld_local_income_tax_krw: 0,
        });
    let source_rows = read_financial_income_sources(tx, context, tax_year).await?;
    let assessment = read_annual_tax_assessment_row(tx, context, tax_year).await?;

    let Some(runtime) = runtime else {
        ensure!(
            source_rows.is_empty() && assessment.is_none(),
            "non-M2-D tax year contains source or assessment rows"
        );
        return Ok(AnnualTaxYearState {
            tax_year,
            sources: Vec::new(),
            gross_financial_income_krw: aggregate.gross_financial_income_krw,
            withheld_income_tax_krw: aggregate.withheld_income_tax_krw,
            withheld_local_income_tax_krw: aggregate.withheld_local_income_tax_krw,
            assessment: AnnualTaxAssessmentState::NotApplicable,
        });
    };

    let source_years = synthesize_source_years(&aggregate, source_rows)?;
    let sources = canonical_source_states(&source_years);
    let assessment = match assessment {
        Some(row) => {
            let status = parse_db_str::<FinancialIncomeAssessmentStatus>(&row.status)?;
            let world_start_date = if status == FinancialIncomeAssessmentStatus::Filed {
                Some(read_world_start_date(tx, context).await?)
            } else {
                None
            };
            assessment_state_from_row(
                context,
                tax_year,
                &aggregate,
                &source_years,
                &runtime,
                &row,
                status,
                world_start_date,
            )?
        }
        None => {
            ensure!(
                aggregate.gross_financial_income_krw == 0
                    && aggregate.withheld_income_tax_krw == 0
                    && aggregate.withheld_local_income_tax_krw == 0,
                "M2-D tax year has totals but no assessment"
            );
            AnnualTaxAssessmentState::Open
        }
    };

    Ok(AnnualTaxYearState {
        tax_year,
        sources,
        gross_financial_income_krw: aggregate.gross_financial_income_krw,
        withheld_income_tax_krw: aggregate.withheld_income_tax_krw,
        withheld_local_income_tax_krw: aggregate.withheld_local_income_tax_krw,
        assessment,
    })
}

/// Reads the newest finalized assessment for the bounded snapshot, if one exists.
pub(super) async fn read_latest_annual_tax_assessment(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
) -> Result<Option<AnnualTaxYearState>> {
    ensure_annual_tax_read_context(tx, context).await?;
    let tax_year: Option<u16> = sqlx::query_scalar(
        "SELECT assessment.tax_year
         FROM financial_income_assessment AS assessment
         WHERE assessment.save_id = ? AND assessment.run_revision = ?
           AND assessment.status IN ('finalizedNoFiling', 'filingPending', 'filed')
         ORDER BY assessment.tax_year DESC
         LIMIT 1",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read the latest annual-tax assessment year")?;
    let Some(tax_year) = tax_year else {
        return Ok(None);
    };
    let state = read_annual_tax_year(tx, context, tax_year).await?;
    ensure!(
        matches!(
            state.assessment,
            AnnualTaxAssessmentState::FinalizedNoFiling { .. }
                | AnnualTaxAssessmentState::FilingPending { .. }
                | AnnualTaxAssessmentState::Filed { .. }
        ),
        "latest annual-tax assessment query returned a non-finalized state"
    );
    Ok(Some(state))
}

async fn ensure_annual_tax_read_context(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
) -> Result<()> {
    let save_id: Option<u64> = sqlx::query_scalar(
        "SELECT id
         FROM save
         WHERE id = ? AND run_revision = ? AND policy_set_id = ?",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(context.policy_set_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to validate the annual-tax read context")?;
    ensure!(
        save_id == Some(context.save_id),
        "annual-tax read context no longer matches the current save"
    );
    Ok(())
}

async fn read_financial_income_aggregate(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
) -> Result<Option<FinancialIncomeAggregateRow>> {
    sqlx::query_as(
        "SELECT gross_financial_income_krw, withheld_income_tax_krw,
                withheld_local_income_tax_krw
         FROM financial_income_year
         WHERE save_id = ? AND run_revision = ? AND tax_year = ?",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(tax_year)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read the financial-income aggregate year")
}

async fn read_financial_income_sources(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
) -> Result<Vec<FinancialIncomeSourceYearRow>> {
    sqlx::query_as(
        "SELECT source, gross_income_krw, withheld_income_tax_krw,
                withheld_local_income_tax_krw
         FROM financial_income_source_year
         WHERE save_id = ? AND run_revision = ? AND tax_year = ?
         ORDER BY source",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(tax_year)
    .fetch_all(&mut **tx)
    .await
    .context("failed to read source-level financial-income totals")
}

async fn read_annual_tax_assessment_row(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
) -> Result<Option<AnnualTaxAssessmentReadRow>> {
    sqlx::query_as(
        "SELECT policy_set_id, year_end_tax_assessment_id, status,
                gross_financial_income_krw, other_comprehensive_income_krw,
                employment_taxable_income_krw, employment_deductions_krw,
                employment_final_prepaid_income_tax_krw,
                employment_final_prepaid_local_income_tax_krw,
                withheld_income_tax_krw,
                withheld_local_income_tax_krw, income_tax_formula_a_krw,
                income_tax_formula_b_krw, local_income_tax_formula_a_krw,
                local_income_tax_formula_b_krw, income_tax_credit_krw,
                local_income_tax_credit_krw, final_income_tax_krw,
                final_local_income_tax_krw, additional_tax_krw, refund_krw,
                finalized_on, filing_date, filed_on
         FROM financial_income_assessment
         WHERE save_id = ? AND run_revision = ? AND tax_year = ?",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(tax_year)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read the annual-tax assessment")
}

fn canonical_source_states(
    source_years: &[FinancialIncomeSourceYear],
) -> Vec<AnnualTaxSourceState> {
    FinancialIncomeSource::ALL
        .into_iter()
        .map(|source| {
            let year = source_years
                .iter()
                .find(|year| year.source == source)
                .copied()
                .unwrap_or_else(|| FinancialIncomeSourceYear::zero(source));
            AnnualTaxSourceState {
                source,
                gross_financial_income_krw: year.gross_income_krw,
                withheld_income_tax_krw: year.withheld_income_tax_krw,
                withheld_local_income_tax_krw: year.withheld_local_income_tax_krw,
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn assessment_state_from_row(
    context: AnnualTaxRunContext,
    tax_year: u16,
    aggregate: &FinancialIncomeAggregateRow,
    source_years: &[FinancialIncomeSourceYear],
    runtime: &AnnualTaxRuntimePolicy,
    row: &AnnualTaxAssessmentReadRow,
    status: FinancialIncomeAssessmentStatus,
    world_start_date: Option<Date>,
) -> Result<AnnualTaxAssessmentState> {
    ensure!(
        row.policy_set_id == context.policy_set_id,
        "annual-tax assessment policy pin drifted"
    );
    if status == FinancialIncomeAssessmentStatus::Open {
        ensure_open_assessment_read_row(row)?;
        return Ok(AnnualTaxAssessmentState::Open);
    }

    if row.year_end_tax_assessment_id.is_some() {
        validate_combined_assessment(tax_year, aggregate, source_years, runtime, row, status)?;
        return finalized_assessment_state(row, status, world_start_date);
    }

    ensure!(
        row.other_comprehensive_income_krw == runtime.other_comprehensive_income_krw
            && row.income_tax_credit_krw == runtime.credits.income_tax_credit_krw
            && row.local_income_tax_credit_krw == runtime.credits.local_income_tax_credit_krw,
        "annual-tax assessment M2 defaults drifted from its policy"
    );
    let finalization_date = row
        .finalized_on
        .context("finalized annual-tax assessment has no finalization date")?;
    let expected_draft_status = if status == FinancialIncomeAssessmentStatus::Filed {
        FinancialIncomeAssessmentStatus::FilingPending
    } else {
        status
    };
    let calculated = finalize_annual_assessment(
        &runtime.policy,
        &AnnualAssessmentFinalizeInput {
            tax_year: i32::from(tax_year),
            finalization_date,
            current_status: FinancialIncomeAssessmentStatus::Open,
            source_years: source_years.to_vec(),
            other_comprehensive_income_krw: runtime.other_comprehensive_income_krw,
            credits: runtime.credits,
        },
    )
    .context("failed to revalidate the stored annual-tax assessment")?;
    let stored = AnnualAssessmentDraft {
        tax_year: i32::from(tax_year),
        finalization_date,
        filing_date: row.filing_date,
        status: expected_draft_status,
        gross_financial_income_krw: row.gross_financial_income_krw,
        other_comprehensive_income_krw: row.other_comprehensive_income_krw,
        withheld_income_tax_krw: row.withheld_income_tax_krw,
        withheld_local_income_tax_krw: row.withheld_local_income_tax_krw,
        income_tax_formula_a_krw: row.income_tax_formula_a_krw,
        income_tax_formula_b_krw: row.income_tax_formula_b_krw,
        local_income_tax_formula_a_krw: row.local_income_tax_formula_a_krw,
        local_income_tax_formula_b_krw: row.local_income_tax_formula_b_krw,
        income_tax_credit_krw: row.income_tax_credit_krw,
        local_income_tax_credit_krw: row.local_income_tax_credit_krw,
        final_income_tax_krw: row.final_income_tax_krw,
        final_local_income_tax_krw: row.final_local_income_tax_krw,
        additional_tax_krw: row.additional_tax_krw,
        refund_krw: row.refund_krw,
    };
    ensure!(
        calculated == stored
            && row.gross_financial_income_krw == aggregate.gross_financial_income_krw
            && row.withheld_income_tax_krw == aggregate.withheld_income_tax_krw
            && row.withheld_local_income_tax_krw == aggregate.withheld_local_income_tax_krw,
        "stored annual-tax assessment disagrees with its policy or source totals"
    );

    finalized_assessment_state(row, status, world_start_date)
}

fn finalized_assessment_state(
    row: &AnnualTaxAssessmentReadRow,
    status: FinancialIncomeAssessmentStatus,
    world_start_date: Option<Date>,
) -> Result<AnnualTaxAssessmentState> {
    let calculated = AnnualTaxCalculatedState {
        comparison_a_income_tax_krw: row.income_tax_formula_a_krw,
        comparison_a_local_income_tax_krw: row.local_income_tax_formula_a_krw,
        comparison_b_income_tax_krw: row.income_tax_formula_b_krw,
        comparison_b_local_income_tax_krw: row.local_income_tax_formula_b_krw,
        assessed_income_tax_krw: row.final_income_tax_krw,
        assessed_local_income_tax_krw: row.final_local_income_tax_krw,
        additional_tax_krw: row.additional_tax_krw,
        refund_krw: row.refund_krw,
    };
    match status {
        FinancialIncomeAssessmentStatus::Open => unreachable!("open was handled above"),
        FinancialIncomeAssessmentStatus::FinalizedNoFiling => {
            ensure!(
                row.filing_date.is_none() && row.filed_on.is_none(),
                "non-filing annual-tax assessment contains filing dates"
            );
            Ok(AnnualTaxAssessmentState::FinalizedNoFiling { calculated })
        }
        FinancialIncomeAssessmentStatus::FilingPending => {
            let filing_due_date = row
                .filing_date
                .context("filing-pending annual-tax assessment has no due date")?;
            ensure!(
                row.filed_on.is_none(),
                "filing-pending annual-tax assessment is already filed"
            );
            Ok(AnnualTaxAssessmentState::FilingPending {
                calculated,
                filing_due_date,
            })
        }
        FinancialIncomeAssessmentStatus::Filed => {
            let filing_due_date = row
                .filing_date
                .context("filed annual-tax assessment has no due date")?;
            ensure!(
                row.filed_on == Some(filing_due_date),
                "filed annual-tax assessment date disagrees with its due date"
            );
            let filed_game_day = game_day_for_date(
                world_start_date.context("filed annual-tax assessment has no world start date")?,
                filing_due_date,
            )?;
            Ok(AnnualTaxAssessmentState::Filed {
                calculated,
                filing_due_date,
                filed_game_day,
            })
        }
    }
}

fn validate_combined_assessment(
    tax_year: u16,
    aggregate: &FinancialIncomeAggregateRow,
    source_years: &[FinancialIncomeSourceYear],
    runtime: &AnnualTaxRuntimePolicy,
    row: &AnnualTaxAssessmentReadRow,
    status: FinancialIncomeAssessmentStatus,
) -> Result<()> {
    ensure!(
        status == FinancialIncomeAssessmentStatus::FilingPending
            || status == FinancialIncomeAssessmentStatus::Filed,
        "combined annual-tax assessment is not a filing assessment"
    );
    ensure!(
        row.other_comprehensive_income_krw == row.employment_taxable_income_krw
            && row.employment_taxable_income_krw >= 0
            && row.employment_deductions_krw >= 0
            && row.employment_final_prepaid_income_tax_krw >= 0
            && row.employment_final_prepaid_local_income_tax_krw >= 0,
        "combined annual-tax employment handoff is invalid"
    );
    let finalization_date = row
        .finalized_on
        .context("combined annual-tax assessment has no finalization date")?;
    let calculated = finalize_annual_assessment(
        &runtime.policy,
        &AnnualAssessmentFinalizeInput {
            tax_year: i32::from(tax_year),
            finalization_date,
            current_status: FinancialIncomeAssessmentStatus::Open,
            source_years: source_years.to_vec(),
            other_comprehensive_income_krw: row.employment_taxable_income_krw,
            credits: TaxCredits {
                income_tax_credit_krw: row.income_tax_credit_krw,
                local_income_tax_credit_krw: row.local_income_tax_credit_krw,
            },
        },
    )
    .context("failed to revalidate the combined annual-tax comparison formulas")?;
    let prepaid = row
        .withheld_income_tax_krw
        .checked_add(row.withheld_local_income_tax_krw)
        .and_then(|value| value.checked_add(row.employment_final_prepaid_income_tax_krw))
        .and_then(|value| value.checked_add(row.employment_final_prepaid_local_income_tax_krw))
        .context("combined annual-tax prepayments overflowed")?;
    let assessed = row
        .final_income_tax_krw
        .checked_add(row.final_local_income_tax_krw)
        .context("combined annual-tax assessment overflowed")?;
    let difference = assessed
        .checked_sub(prepaid)
        .context("combined annual-tax reconciliation overflowed")?;
    let (expected_additional_tax_krw, expected_refund_krw) = if difference >= 0 {
        (difference, 0)
    } else {
        (
            0,
            difference
                .checked_neg()
                .context("combined annual-tax refund overflowed")?,
        )
    };
    ensure!(
        row.gross_financial_income_krw == aggregate.gross_financial_income_krw
            && row.withheld_income_tax_krw == aggregate.withheld_income_tax_krw
            && row.withheld_local_income_tax_krw == aggregate.withheld_local_income_tax_krw
            && row.income_tax_formula_a_krw == calculated.income_tax_formula_a_krw
            && row.income_tax_formula_b_krw == calculated.income_tax_formula_b_krw
            && row.local_income_tax_formula_a_krw == calculated.local_income_tax_formula_a_krw
            && row.local_income_tax_formula_b_krw == calculated.local_income_tax_formula_b_krw
            && row.final_income_tax_krw == calculated.final_income_tax_krw
            && row.final_local_income_tax_krw == calculated.final_local_income_tax_krw
            && row.additional_tax_krw == expected_additional_tax_krw
            && row.refund_krw == expected_refund_krw,
        "stored combined annual-tax assessment disagrees with its source totals"
    );
    Ok(())
}

fn ensure_open_assessment_read_row(row: &AnnualTaxAssessmentReadRow) -> Result<()> {
    ensure!(
        row.gross_financial_income_krw == 0
            && row.year_end_tax_assessment_id.is_none()
            && row.other_comprehensive_income_krw == 0
            && row.employment_taxable_income_krw == 0
            && row.employment_deductions_krw == 0
            && row.employment_final_prepaid_income_tax_krw == 0
            && row.employment_final_prepaid_local_income_tax_krw == 0
            && row.withheld_income_tax_krw == 0
            && row.withheld_local_income_tax_krw == 0
            && row.income_tax_formula_a_krw == 0
            && row.income_tax_formula_b_krw == 0
            && row.local_income_tax_formula_a_krw == 0
            && row.local_income_tax_formula_b_krw == 0
            && row.income_tax_credit_krw == 0
            && row.local_income_tax_credit_krw == 0
            && row.final_income_tax_krw == 0
            && row.final_local_income_tax_krw == 0
            && row.additional_tax_krw == 0
            && row.refund_krw == 0
            && row.finalized_on.is_none()
            && row.filing_date.is_none()
            && row.filed_on.is_none(),
        "open annual-tax assessment contains finalized values"
    );
    Ok(())
}

/// Adds one source payment and its compatibility aggregate inside the caller's transaction.
pub(super) async fn accrue_financial_income_source(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    accrual: FinancialIncomeAccrual,
) -> Result<AnnualIncomeAccrualMode> {
    let mut validated = FinancialIncomeSourceYear::zero(accrual.source);
    validated
        .accrue(accrual)
        .context("financial-income source accrual is invalid")?;
    let tax_year = tax_year(context.market_date)?;
    let annual_policy =
        read_annual_tax_policy(tx, context.policy_set_id, context.market_date).await?;

    apply_aggregate_accrual(tx, context, tax_year, accrual).await?;
    let Some(_) = annual_policy else {
        return Ok(AnnualIncomeAccrualMode::AggregateOnly);
    };

    apply_source_accrual(tx, context, tax_year, accrual).await?;
    ensure_open_assessment_after_source(tx, context, tax_year).await?;
    Ok(AnnualIncomeAccrualMode::SourceTracked)
}

async fn apply_aggregate_accrual(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
    accrual: FinancialIncomeAccrual,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO financial_income_year
             (save_id, run_revision, tax_year, gross_financial_income_krw,
              withheld_income_tax_krw, withheld_local_income_tax_krw)
         VALUES (?, ?, ?, ?, ?, ?)
         ON DUPLICATE KEY UPDATE
             gross_financial_income_krw = gross_financial_income_krw
                 + VALUES(gross_financial_income_krw),
             withheld_income_tax_krw = withheld_income_tax_krw
                 + VALUES(withheld_income_tax_krw),
             withheld_local_income_tax_krw = withheld_local_income_tax_krw
                 + VALUES(withheld_local_income_tax_krw)",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(tax_year)
    .bind(accrual.gross_income_krw)
    .bind(accrual.withheld_income_tax_krw)
    .bind(accrual.withheld_local_income_tax_krw)
    .execute(&mut **tx)
    .await
    .context("failed to update the financial-income compatibility aggregate")?;
    Ok(())
}

async fn apply_source_accrual(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
    accrual: FinancialIncomeAccrual,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO financial_income_source_year
             (save_id, run_revision, tax_year, source, gross_income_krw,
              withheld_income_tax_krw, withheld_local_income_tax_krw)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON DUPLICATE KEY UPDATE
             gross_income_krw = gross_income_krw + VALUES(gross_income_krw),
             withheld_income_tax_krw = withheld_income_tax_krw
                 + VALUES(withheld_income_tax_krw),
             withheld_local_income_tax_krw = withheld_local_income_tax_krw
                 + VALUES(withheld_local_income_tax_krw)",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(tax_year)
    .bind(to_db_str(&accrual.source)?)
    .bind(accrual.gross_income_krw)
    .bind(accrual.withheld_income_tax_krw)
    .bind(accrual.withheld_local_income_tax_krw)
    .execute(&mut **tx)
    .await
    .context("failed to update the source-level financial-income total")?;
    Ok(())
}

/// Ensures the current year is open and freezes the prior year only on January 1.
pub(super) async fn prepare_annual_tax_year_boundary(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
) -> Result<()> {
    let current_policy =
        read_annual_tax_policy(tx, context.policy_set_id, context.market_date).await?;
    if current_policy.is_none() {
        return Ok(());
    }

    let current_tax_year = tax_year(context.market_date)?;
    ensure_current_open_year(tx, context, current_tax_year).await
}

pub(super) async fn finalize_previous_tax_year(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
) -> Result<()> {
    let existing_schedule = lock_filing_schedule_for_tax_year(tx, context, tax_year).await?;
    let Some(snapshot) = lock_tax_year_snapshot(tx, context, tax_year).await? else {
        ensure!(
            existing_schedule.is_none(),
            "annual-tax filing schedule has no source tax year"
        );
        return Ok(());
    };
    let assessment = ensure_assessment_row(tx, context, tax_year).await?;
    let status = parse_db_str::<FinancialIncomeAssessmentStatus>(&assessment.status)?;
    if !should_finalize_previous(context.market_date, status) {
        verify_existing_schedule(
            tx,
            context,
            tax_year,
            status,
            assessment.filing_date,
            existing_schedule.as_ref(),
        )
        .await?;
        return Ok(());
    }
    ensure!(
        existing_schedule.is_none(),
        "open annual-tax assessment already has a filing schedule"
    );

    let effective_on = Date::from_calendar_date(i32::from(tax_year), Month::December, 31)
        .context("previous annual-tax policy date is invalid")?;
    let runtime = read_annual_tax_policy(tx, context.policy_set_id, effective_on)
        .await?
        .context("previous tax year has no pinned annual-tax policy")?;
    let draft = finalize_annual_assessment(
        &runtime.policy,
        &AnnualAssessmentFinalizeInput {
            tax_year: i32::from(tax_year),
            finalization_date: context.market_date,
            current_status: FinancialIncomeAssessmentStatus::Open,
            source_years: snapshot.source_years,
            other_comprehensive_income_krw: runtime.other_comprehensive_income_krw,
            credits: runtime.credits,
        },
    )
    .context("failed to finalize the previous financial-income tax year")?;
    persist_assessment_draft(tx, context, &draft).await?;

    if draft.status == FinancialIncomeAssessmentStatus::FilingPending {
        let filing_date = draft
            .filing_date
            .context("filing-pending assessment has no filing date")?;
        let world_start_date = read_world_start_date(tx, context).await?;
        let due_game_day = game_day_for_date(world_start_date, filing_date)?;
        insert_filing_schedule(tx, context, tax_year, due_game_day).await?;
    }
    Ok(())
}

/// Plans an M2 assessment while allowing M3-C to inject only its published handoff.
pub(super) async fn plan_previous_tax_year_with_employment(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
    other_comprehensive_income_krw: i64,
    credits: TaxCredits,
) -> Result<Option<AnnualAssessmentDraft>> {
    let Some(snapshot) = lock_tax_year_snapshot(tx, context, tax_year).await? else {
        return Ok(None);
    };
    let assessment = ensure_assessment_row(tx, context, tax_year).await?;
    ensure!(
        parse_db_str::<FinancialIncomeAssessmentStatus>(&assessment.status)?
            == FinancialIncomeAssessmentStatus::Open,
        "annual-tax coordinator found a non-open financial assessment"
    );
    let effective_on = Date::from_calendar_date(i32::from(tax_year), Month::December, 31)
        .context("previous annual-tax policy date is invalid")?;
    let runtime = read_annual_tax_policy(tx, context.policy_set_id, effective_on)
        .await?
        .context("previous tax year has no pinned annual-tax policy")?;
    let draft = finalize_annual_assessment(
        &runtime.policy,
        &AnnualAssessmentFinalizeInput {
            tax_year: i32::from(tax_year),
            finalization_date: context.market_date,
            current_status: FinancialIncomeAssessmentStatus::Open,
            source_years: snapshot.source_years,
            other_comprehensive_income_krw,
            credits,
        },
    )
    .context("failed to plan the coordinated financial-income assessment")?;
    Ok(Some(draft))
}

async fn ensure_current_open_year(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
) -> Result<()> {
    if lock_tax_year_snapshot(tx, context, tax_year)
        .await?
        .is_none()
    {
        sqlx::query(
            "INSERT INTO financial_income_year (save_id, run_revision, tax_year)
             VALUES (?, ?, ?)",
        )
        .bind(context.save_id)
        .bind(context.run_revision)
        .bind(tax_year)
        .execute(&mut **tx)
        .await
        .context("failed to open the current financial-income aggregate year")?;
        lock_tax_year_snapshot(tx, context, tax_year)
            .await?
            .context("current financial-income year disappeared after insertion")?;
    }
    let assessment = ensure_assessment_row(tx, context, tax_year).await?;
    ensure!(
        parse_db_str::<FinancialIncomeAssessmentStatus>(&assessment.status)?
            == FinancialIncomeAssessmentStatus::Open,
        "current annual-tax assessment is not open"
    );
    Ok(())
}

async fn ensure_open_assessment_after_source(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
) -> Result<()> {
    let assessment = ensure_assessment_row(tx, context, tax_year).await?;
    ensure!(
        parse_db_str::<FinancialIncomeAssessmentStatus>(&assessment.status)?
            == FinancialIncomeAssessmentStatus::Open,
        "source income cannot accrue after annual-tax finalization"
    );
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct FinancialIncomeAggregateRow {
    gross_financial_income_krw: i64,
    withheld_income_tax_krw: i64,
    withheld_local_income_tax_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct FinancialIncomeSourceYearRow {
    source: String,
    gross_income_krw: i64,
    withheld_income_tax_krw: i64,
    withheld_local_income_tax_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LockedTaxYearSnapshot {
    pub source_years: Vec<FinancialIncomeSourceYear>,
}

pub(super) async fn lock_tax_year_snapshot(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
) -> Result<Option<LockedTaxYearSnapshot>> {
    let aggregate: Option<FinancialIncomeAggregateRow> = sqlx::query_as(
        "SELECT gross_financial_income_krw, withheld_income_tax_krw,
                withheld_local_income_tax_krw
         FROM financial_income_year
         WHERE save_id = ? AND run_revision = ? AND tax_year = ?
         FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(tax_year)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock the financial-income aggregate year")?;
    let Some(aggregate) = aggregate else {
        return Ok(None);
    };
    let rows: Vec<FinancialIncomeSourceYearRow> = sqlx::query_as(
        "SELECT source, gross_income_krw, withheld_income_tax_krw,
                withheld_local_income_tax_krw
         FROM financial_income_source_year
         WHERE save_id = ? AND run_revision = ? AND tax_year = ?
         ORDER BY source
         FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(tax_year)
    .fetch_all(&mut **tx)
    .await
    .context("failed to lock source-level financial-income totals")?;
    Ok(Some(LockedTaxYearSnapshot {
        source_years: synthesize_source_years(&aggregate, rows)?,
    }))
}

fn synthesize_source_years(
    aggregate: &FinancialIncomeAggregateRow,
    rows: Vec<FinancialIncomeSourceYearRow>,
) -> Result<Vec<FinancialIncomeSourceYear>> {
    let mut seen = [false; FinancialIncomeSource::ALL.len()];
    let mut gross_income_krw = 0_i128;
    let mut withheld_income_tax_krw = 0_i128;
    let mut withheld_local_income_tax_krw = 0_i128;
    let mut source_years = Vec::with_capacity(rows.len());
    for row in rows {
        let source = parse_db_str::<FinancialIncomeSource>(&row.source)?;
        let index = source_index(source);
        ensure!(
            !seen[index],
            "source-level financial-income row is duplicated"
        );
        seen[index] = true;
        let mut source_year = FinancialIncomeSourceYear::zero(source);
        source_year
            .accrue(FinancialIncomeAccrual {
                source,
                gross_income_krw: row.gross_income_krw,
                withheld_income_tax_krw: row.withheld_income_tax_krw,
                withheld_local_income_tax_krw: row.withheld_local_income_tax_krw,
            })
            .context("stored source-level financial-income row is invalid")?;
        gross_income_krw = gross_income_krw
            .checked_add(i128::from(row.gross_income_krw))
            .context("source-level gross income overflowed")?;
        withheld_income_tax_krw = withheld_income_tax_krw
            .checked_add(i128::from(row.withheld_income_tax_krw))
            .context("source-level withheld income tax overflowed")?;
        withheld_local_income_tax_krw = withheld_local_income_tax_krw
            .checked_add(i128::from(row.withheld_local_income_tax_krw))
            .context("source-level withheld local tax overflowed")?;
        source_years.push(source_year);
    }
    ensure!(
        gross_income_krw == i128::from(aggregate.gross_financial_income_krw)
            && withheld_income_tax_krw == i128::from(aggregate.withheld_income_tax_krw)
            && withheld_local_income_tax_krw == i128::from(aggregate.withheld_local_income_tax_krw),
        "source-level financial income disagrees with its compatibility aggregate"
    );
    source_years.sort_by_key(|year| source_index(year.source));
    Ok(source_years)
}

const fn source_index(source: FinancialIncomeSource) -> usize {
    match source {
        FinancialIncomeSource::CmaInterest => 0,
        FinancialIncomeSource::DepositInterest => 1,
        FinancialIncomeSource::BondCoupon => 2,
        FinancialIncomeSource::LlxDistribution => 3,
        FinancialIncomeSource::IsaEarlyClose => 4,
        FinancialIncomeSource::CorporationDividend => 5,
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct AssessmentBoundaryRow {
    status: String,
    filing_date: Option<Date>,
}

async fn ensure_assessment_row(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
) -> Result<AssessmentBoundaryRow> {
    if let Some(row) = read_assessment_boundary_row(tx, context, tax_year).await? {
        return Ok(row);
    }
    sqlx::query(
        "INSERT INTO financial_income_assessment
             (save_id, run_revision, tax_year, policy_set_id, status)
         VALUES (?, ?, ?, ?, 'open')",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(tax_year)
    .bind(context.policy_set_id)
    .execute(&mut **tx)
    .await
    .context("failed to create an open annual-tax assessment")?;
    read_assessment_boundary_row(tx, context, tax_year)
        .await?
        .context("annual-tax assessment disappeared after insertion")
}

async fn read_assessment_boundary_row(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
) -> Result<Option<AssessmentBoundaryRow>> {
    sqlx::query_as(
        "SELECT status, filing_date
         FROM financial_income_assessment
         WHERE save_id = ? AND run_revision = ? AND tax_year = ?
         FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(tax_year)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock the annual-tax assessment")
}

fn should_finalize_previous(market_date: Date, status: FinancialIncomeAssessmentStatus) -> bool {
    is_january_first(market_date) && status == FinancialIncomeAssessmentStatus::Open
}

pub(super) async fn persist_assessment_draft(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    draft: &AnnualAssessmentDraft,
) -> Result<()> {
    let update = sqlx::query(
        "UPDATE financial_income_assessment
         SET status = ?,
             gross_financial_income_krw = ?, other_comprehensive_income_krw = ?,
             withheld_income_tax_krw = ?, withheld_local_income_tax_krw = ?,
             income_tax_formula_a_krw = ?, income_tax_formula_b_krw = ?,
             local_income_tax_formula_a_krw = ?, local_income_tax_formula_b_krw = ?,
             income_tax_credit_krw = ?, local_income_tax_credit_krw = ?,
             final_income_tax_krw = ?, final_local_income_tax_krw = ?,
             additional_tax_krw = ?, refund_krw = ?,
             finalized_on = ?, filing_date = ?, filed_on = NULL
         WHERE save_id = ? AND run_revision = ? AND tax_year = ? AND status = 'open'",
    )
    .bind(to_db_str(&draft.status)?)
    .bind(draft.gross_financial_income_krw)
    .bind(draft.other_comprehensive_income_krw)
    .bind(draft.withheld_income_tax_krw)
    .bind(draft.withheld_local_income_tax_krw)
    .bind(draft.income_tax_formula_a_krw)
    .bind(draft.income_tax_formula_b_krw)
    .bind(draft.local_income_tax_formula_a_krw)
    .bind(draft.local_income_tax_formula_b_krw)
    .bind(draft.income_tax_credit_krw)
    .bind(draft.local_income_tax_credit_krw)
    .bind(draft.final_income_tax_krw)
    .bind(draft.final_local_income_tax_krw)
    .bind(draft.additional_tax_krw)
    .bind(draft.refund_krw)
    .bind(draft.finalization_date)
    .bind(draft.filing_date)
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(draft.tax_year)
    .execute(&mut **tx)
    .await
    .context("failed to persist the finalized annual-tax assessment")?;
    ensure!(
        update.rows_affected() == 1,
        "annual-tax finalization lost its open assessment"
    );
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AnnualEmploymentAssessmentLink {
    pub year_end_tax_assessment_id: u64,
    pub employment_taxable_income_krw: i64,
    pub employment_deductions_krw: i64,
    pub employment_final_prepaid_income_tax_krw: i64,
    pub employment_final_prepaid_local_income_tax_krw: i64,
}

pub(super) async fn persist_assessment_draft_with_employment(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    draft: &AnnualAssessmentDraft,
    employment: AnnualEmploymentAssessmentLink,
) -> Result<()> {
    let update = sqlx::query(
        "UPDATE financial_income_assessment
         SET status = ?, year_end_tax_assessment_id = ?,
             gross_financial_income_krw = ?, other_comprehensive_income_krw = ?,
             employment_taxable_income_krw = ?, employment_deductions_krw = ?,
             employment_final_prepaid_income_tax_krw = ?,
             employment_final_prepaid_local_income_tax_krw = ?,
             withheld_income_tax_krw = ?, withheld_local_income_tax_krw = ?,
             income_tax_formula_a_krw = ?, income_tax_formula_b_krw = ?,
             local_income_tax_formula_a_krw = ?, local_income_tax_formula_b_krw = ?,
             income_tax_credit_krw = ?, local_income_tax_credit_krw = ?,
             final_income_tax_krw = ?, final_local_income_tax_krw = ?,
             additional_tax_krw = ?, refund_krw = ?,
             finalized_on = ?, filing_date = ?, filed_on = NULL
         WHERE save_id = ? AND run_revision = ? AND tax_year = ? AND status = 'open'",
    )
    .bind(to_db_str(&draft.status)?)
    .bind(employment.year_end_tax_assessment_id)
    .bind(draft.gross_financial_income_krw)
    .bind(draft.other_comprehensive_income_krw)
    .bind(employment.employment_taxable_income_krw)
    .bind(employment.employment_deductions_krw)
    .bind(employment.employment_final_prepaid_income_tax_krw)
    .bind(employment.employment_final_prepaid_local_income_tax_krw)
    .bind(draft.withheld_income_tax_krw)
    .bind(draft.withheld_local_income_tax_krw)
    .bind(draft.income_tax_formula_a_krw)
    .bind(draft.income_tax_formula_b_krw)
    .bind(draft.local_income_tax_formula_a_krw)
    .bind(draft.local_income_tax_formula_b_krw)
    .bind(draft.income_tax_credit_krw)
    .bind(draft.local_income_tax_credit_krw)
    .bind(draft.final_income_tax_krw)
    .bind(draft.final_local_income_tax_krw)
    .bind(draft.additional_tax_krw)
    .bind(draft.refund_krw)
    .bind(draft.finalization_date)
    .bind(draft.filing_date)
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(draft.tax_year)
    .execute(&mut **tx)
    .await
    .context("failed to persist the coordinated annual-tax assessment")?;
    ensure!(
        update.rows_affected() == 1,
        "coordinated annual-tax finalization lost its open assessment"
    );
    Ok(())
}

pub(super) async fn schedule_annual_tax_filing_if_needed(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    draft: &AnnualAssessmentDraft,
) -> Result<()> {
    if draft.status != FinancialIncomeAssessmentStatus::FilingPending {
        return Ok(());
    }
    let filing_date = draft
        .filing_date
        .context("filing-pending assessment has no filing date")?;
    let due_game_day = game_day_for_date(read_world_start_date(tx, context).await?, filing_date)?;
    insert_filing_schedule(tx, context, u16::try_from(draft.tax_year)?, due_game_day).await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FinancialIncomeFilingPayload {
    schema_version: u8,
    tax_year: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct FilingScheduleRow {
    id: u64,
    due_game_day: u32,
    kind: String,
    payload_json: String,
    source_kind: String,
    source_id: String,
    occurrence: u64,
    status: String,
}

async fn lock_filing_schedule_for_tax_year(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
) -> Result<Option<FilingScheduleRow>> {
    sqlx::query_as(
        "SELECT id, due_game_day, kind, CAST(payload AS CHAR) AS payload_json,
                source_kind, source_id, occurrence, status
         FROM scheduled_settlement
         WHERE save_id = ? AND run_revision = ?
           AND BINARY source_kind = BINARY ? AND BINARY source_id = BINARY ?
           AND occurrence = ?
         FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(FILING_SETTLEMENT_SOURCE_KIND)
    .bind(tax_year.to_string())
    .bind(FILING_SETTLEMENT_OCCURRENCE)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock the annual-tax filing schedule")
}

pub(super) async fn insert_filing_schedule(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
    due_game_day: u32,
) -> Result<u64> {
    let payload = serde_json::to_string(&FinancialIncomeFilingPayload {
        schema_version: FILING_PAYLOAD_SCHEMA_VERSION,
        tax_year,
    })?;
    let result = sqlx::query(
        "INSERT INTO scheduled_settlement
             (save_id, run_revision, due_game_day, kind, payload,
              source_kind, source_id, occurrence, status)
         VALUES (?, ?, ?, ?, CAST(? AS JSON), ?, ?, ?, 'pending')",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(due_game_day)
    .bind(FILING_SETTLEMENT_KIND)
    .bind(payload)
    .bind(FILING_SETTLEMENT_SOURCE_KIND)
    .bind(tax_year.to_string())
    .bind(FILING_SETTLEMENT_OCCURRENCE)
    .execute(&mut **tx)
    .await
    .context("failed to schedule annual-tax filing")?;
    let settlement_id = result.last_insert_id();
    ensure!(
        settlement_id != 0,
        "annual-tax filing insert returned no ID"
    );
    Ok(settlement_id)
}

async fn verify_existing_schedule(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
    status: FinancialIncomeAssessmentStatus,
    filing_date: Option<Date>,
    schedule: Option<&FilingScheduleRow>,
) -> Result<()> {
    match status {
        FinancialIncomeAssessmentStatus::Open
        | FinancialIncomeAssessmentStatus::FinalizedNoFiling => ensure!(
            schedule.is_none(),
            "non-filing annual-tax assessment has a filing schedule"
        ),
        FinancialIncomeAssessmentStatus::FilingPending | FinancialIncomeAssessmentStatus::Filed => {
            let schedule = schedule.context("filing annual-tax assessment has no schedule")?;
            let payload = decode_filing_schedule(schedule)?;
            ensure!(
                payload.tax_year == tax_year,
                "filing schedule tax year drifted"
            );
            let filing_date = filing_date.context("filing assessment has no filing date")?;
            let world_start_date = read_world_start_date(tx, context).await?;
            ensure!(
                schedule.due_game_day == game_day_for_date(world_start_date, filing_date)?,
                "filing schedule game day disagrees with its assessment date"
            );
            let expected_status = if status == FinancialIncomeAssessmentStatus::FilingPending {
                "pending"
            } else {
                "settled"
            };
            ensure!(
                schedule.status == expected_status,
                "filing schedule status disagrees with its assessment"
            );
        }
    }
    Ok(())
}

fn decode_filing_schedule(row: &FilingScheduleRow) -> Result<FinancialIncomeFilingPayload> {
    ensure!(
        row.id > 0
            && row.kind == FILING_SETTLEMENT_KIND
            && row.source_kind == FILING_SETTLEMENT_SOURCE_KIND
            && row.occurrence == u64::from(FILING_SETTLEMENT_OCCURRENCE),
        "stored annual-tax filing settlement identity is invalid"
    );
    let payload: FinancialIncomeFilingPayload = serde_json::from_str(&row.payload_json)
        .context("stored annual-tax filing payload is invalid")?;
    ensure!(
        payload.schema_version == FILING_PAYLOAD_SCHEMA_VERSION
            && row.source_id == payload.tax_year.to_string(),
        "stored annual-tax filing payload disagrees with its source"
    );
    Ok(payload)
}

pub(super) async fn read_world_start_date(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
) -> Result<Date> {
    let row: Option<(Date,)> = sqlx::query_as(
        "SELECT world.start_date
         FROM save
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         WHERE save.id = ? AND save.run_revision = ? AND save.policy_set_id = ?",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(context.policy_set_id)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(|(start_date,)| start_date)
        .context("annual-tax run context no longer matches the current save")
}

pub(super) fn game_day_for_date(world_start_date: Date, date: Date) -> Result<u32> {
    u32::try_from((date - world_start_date).whole_days())
        .context("annual-tax filing date is before the world start or outside game-day range")
}

/// Locks and plans one due filing against the shadow wallet and aggregate-debt balances.
pub(super) async fn plan_annual_tax_filing(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    settlement_id: u64,
    wallet_cash_krw: i64,
    aggregate_debt_krw: i64,
) -> Result<AnnualTaxFilingPlan> {
    let schedule: Option<FilingScheduleRow> = sqlx::query_as(
        "SELECT id, due_game_day, kind, CAST(payload AS CHAR) AS payload_json,
                source_kind, source_id, occurrence, status
         FROM scheduled_settlement
         WHERE save_id = ? AND run_revision = ? AND id = ?
         FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(settlement_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock a due annual-tax filing")?;
    let schedule = schedule.context("annual-tax filing settlement is missing")?;
    let payload = decode_filing_schedule(&schedule)?;
    ensure!(
        schedule.status == "pending" && schedule.due_game_day == context.game_day,
        "annual-tax filing is not pending on this game day"
    );
    lock_tax_year_snapshot(tx, context, payload.tax_year)
        .await?
        .context("annual-tax filing has no source tax year")?;
    let assessment: Option<FilingAssessmentRow> = sqlx::query_as(
        "SELECT status, filing_date, additional_tax_krw, refund_krw
         FROM financial_income_assessment
         WHERE save_id = ? AND run_revision = ? AND tax_year = ?
         FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(payload.tax_year)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock the filing-pending annual-tax assessment")?;
    let assessment = assessment.context("annual-tax filing has no assessment")?;
    let cash_plan = plan_filing_from_row(
        &assessment,
        context.market_date,
        wallet_cash_krw,
        aggregate_debt_krw,
    )?;
    Ok(AnnualTaxFilingPlan {
        settlement_id,
        tax_year: payload.tax_year,
        execution_date: context.market_date,
        expected_wallet_cash_krw: wallet_cash_krw,
        expected_aggregate_debt_krw: aggregate_debt_krw,
        cash_plan,
        context,
    })
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct FilingAssessmentRow {
    status: String,
    filing_date: Option<Date>,
    additional_tax_krw: i64,
    refund_krw: i64,
}

fn plan_filing_from_row(
    assessment: &FilingAssessmentRow,
    execution_date: Date,
    wallet_cash_krw: i64,
    aggregate_debt_krw: i64,
) -> Result<FilingCashPlan> {
    plan_filing_cash(&FilingCashPlanInput {
        current_status: parse_db_str(&assessment.status)?,
        scheduled_filing_date: assessment
            .filing_date
            .context("filing-pending assessment has no scheduled date")?,
        execution_date,
        additional_tax_krw: assessment.additional_tax_krw,
        refund_krw: assessment.refund_krw,
        wallet_cash_krw,
        aggregate_debt_krw,
    })
    .context("stored annual-tax filing state is invalid")
}

/// Applies a previously locked filing plan without committing the caller's transaction.
pub(super) async fn apply_annual_tax_filing(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn FinanceRules,
    context: AnnualTaxRunContext,
    plan: &AnnualTaxFilingPlan,
) -> Result<Option<u64>> {
    ensure!(
        context == plan.context && context.market_date == plan.execution_date,
        "annual-tax filing plan belongs to another run or execution date"
    );

    let assessment_update = sqlx::query(
        "UPDATE financial_income_assessment
         SET status = 'filed', filed_on = ?
         WHERE save_id = ? AND run_revision = ? AND tax_year = ?
           AND status = 'filingPending' AND filing_date = ?",
    )
    .bind(plan.execution_date)
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(plan.tax_year)
    .bind(plan.execution_date)
    .execute(&mut **tx)
    .await
    .context("failed to mark the annual-tax assessment filed")?;
    ensure!(
        assessment_update.rows_affected() == 1,
        "annual-tax filing assessment lost its pending state"
    );

    let debt_increase_krw = match plan.cash_plan.movement {
        FilingMovement::AdditionalTax {
            aggregate_debt_increase_krw,
            ..
        } => aggregate_debt_increase_krw,
        FilingMovement::Refund { .. } | FilingMovement::NoMovement { .. } => 0,
    };
    let tax_obligation_id =
        prepare_financial_income_tax_obligation(tx, context, plan.tax_year, debt_increase_krw)
            .await?;
    let ledger = create_filing_ledger(rules, context, plan)?;
    let ledger_transaction_id = match ledger.as_ref() {
        Some(ledger) => {
            Some(write_annual_tax_ledger_transaction(tx, ledger, tax_obligation_id).await?)
        }
        None => None,
    };
    if let Some(tax_obligation_id) = tax_obligation_id {
        activate_financial_income_tax_obligation(
            tx,
            context,
            plan.tax_year,
            tax_obligation_id,
            ledger_transaction_id.context("tax obligation has no authority ledger")?,
        )
        .await?;
    }

    if plan.cash_plan.wallet_cash_krw != plan.expected_wallet_cash_krw
        || plan.cash_plan.aggregate_debt_krw != plan.expected_aggregate_debt_krw
    {
        let update = sqlx::query(
            "UPDATE save
             SET cash_krw = ?, debt_krw = ?
             WHERE id = ? AND run_revision = ? AND policy_set_id = ?
               AND cash_krw = ? AND debt_krw = ?",
        )
        .bind(plan.cash_plan.wallet_cash_krw)
        .bind(plan.cash_plan.aggregate_debt_krw)
        .bind(context.save_id)
        .bind(context.run_revision)
        .bind(context.policy_set_id)
        .bind(plan.expected_wallet_cash_krw)
        .bind(plan.expected_aggregate_debt_krw)
        .execute(&mut **tx)
        .await
        .context("failed to apply annual-tax filing cash balances")?;
        ensure!(
            update.rows_affected() == 1,
            "annual-tax filing shadow balances no longer match the save"
        );
    }

    let settlement_update = match plan.cash_plan.movement {
        FilingMovement::NoMovement { .. } => {
            ensure!(
                ledger_transaction_id.is_none(),
                "zero-tax filing unexpectedly created a ledger"
            );
            sqlx::query(
                "UPDATE scheduled_settlement
                 SET status = 'settled', outcome = 'noMovement', outcome_reason = ?
                 WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'pending'",
            )
            .bind(ZERO_TAX_DUE_REASON)
            .bind(context.save_id)
            .bind(context.run_revision)
            .bind(plan.settlement_id)
            .execute(&mut **tx)
            .await?
        }
        FilingMovement::Refund { .. } | FilingMovement::AdditionalTax { .. } => {
            sqlx::query(
                "UPDATE scheduled_settlement
                 SET status = 'settled', outcome = 'applied',
                     settled_ledger_transaction_id = ?
                 WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'pending'",
            )
            .bind(ledger_transaction_id.context("cash-moving annual-tax filing has no ledger")?)
            .bind(context.save_id)
            .bind(context.run_revision)
            .bind(plan.settlement_id)
            .execute(&mut **tx)
            .await?
        }
    };
    ensure!(
        settlement_update.rows_affected() == 1,
        "annual-tax filing settlement lost its pending state"
    );
    super::loans::validate_debt_projection_in_tx(tx, context.save_id, context.run_revision).await?;
    Ok(ledger_transaction_id)
}

async fn prepare_financial_income_tax_obligation(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
    amount_krw: i64,
) -> Result<Option<u64>> {
    if amount_krw == 0 {
        return Ok(None);
    }
    ensure!(amount_krw > 0, "annual-tax obligation must be positive");
    let source_id = tax_year.to_string();
    let insert = sqlx::query(
        "INSERT INTO tax_obligation
             (save_id, run_revision, household_id, policy_set_id,
              source_kind, source_id, due_game_day, original_amount_krw,
              paid_amount_krw, outstanding_amount_krw, status,
              authority_ledger_transaction_id)
         SELECT save.id, save.run_revision, household.id, save.policy_set_id,
                'financialIncomeAssessment', ?, ?, ?, 0, ?, 'prepared', NULL
         FROM save
         INNER JOIN household
           ON household.save_id = save.id
          AND household.run_revision = save.run_revision
         WHERE save.id = ? AND save.run_revision = ? AND save.policy_set_id = ?
           AND NOT EXISTS (
               SELECT 1 FROM tax_obligation AS existing
               WHERE existing.save_id = save.id
                 AND existing.run_revision = save.run_revision
                 AND existing.source_kind = 'financialIncomeAssessment'
                 AND BINARY existing.source_id = BINARY ?
           )",
    )
    .bind(&source_id)
    .bind(context.game_day)
    .bind(amount_krw)
    .bind(amount_krw)
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(context.policy_set_id)
    .bind(&source_id)
    .execute(&mut **tx)
    .await
    .context("failed to prepare the annual-tax obligation")?;
    ensure!(
        insert.rows_affected() == 1,
        "annual-tax obligation source is missing or already authoritative"
    );
    let obligation_id = insert.last_insert_id();
    ensure!(
        obligation_id != 0,
        "annual-tax obligation insert returned no ID"
    );
    Ok(Some(obligation_id))
}

async fn activate_financial_income_tax_obligation(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
    obligation_id: u64,
    ledger_transaction_id: u64,
) -> Result<()> {
    let update = sqlx::query(
        "UPDATE tax_obligation
         SET status = 'outstanding', authority_ledger_transaction_id = ?
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND policy_set_id = ? AND source_kind = 'financialIncomeAssessment'
           AND BINARY source_id = BINARY ? AND due_game_day = ?
           AND status = 'prepared' AND authority_ledger_transaction_id IS NULL",
    )
    .bind(ledger_transaction_id)
    .bind(obligation_id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(context.policy_set_id)
    .bind(tax_year.to_string())
    .bind(context.game_day)
    .execute(&mut **tx)
    .await
    .context("failed to activate the annual-tax obligation")?;
    ensure!(
        update.rows_affected() == 1,
        "annual-tax obligation lost its prepared state"
    );
    Ok(())
}

async fn write_annual_tax_ledger_transaction(
    tx: &mut Transaction<'_, MySql>,
    ledger: &LedgerTransaction,
    tax_obligation_id: Option<u64>,
) -> Result<u64> {
    let liability_count = ledger
        .postings()
        .iter()
        .filter(|posting| posting.account_code == LedgerAccountCode::TaxObligationLiability)
        .count();
    ensure!(
        liability_count == usize::from(tax_obligation_id.is_some()),
        "annual-tax ledger obligation reference is incomplete"
    );
    let Some(tax_obligation_id) = tax_obligation_id else {
        return write_ledger_transaction(tx, ledger).await;
    };

    let policy = ledger.policy();
    let insert = sqlx::query(
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
    .await
    .context("failed to write the annual-tax authority ledger")?;
    let ledger_transaction_id = insert.last_insert_id();
    ensure!(
        ledger_transaction_id != 0,
        "annual-tax authority ledger insert returned no ID"
    );
    for (index, posting) in ledger.postings().iter().enumerate() {
        let posting_order = u16::try_from(index + 1).context("too many annual-tax postings")?;
        let posting_obligation_id = (posting.account_code
            == LedgerAccountCode::TaxObligationLiability)
            .then_some(tax_obligation_id);
        sqlx::query(
            "INSERT INTO ledger_posting
                 (save_id, run_revision, ledger_transaction_id, posting_order,
                  account_code, financial_account_id, tax_obligation_id, amount_krw)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(policy.run.save_id.get())
        .bind(policy.run.run_revision)
        .bind(ledger_transaction_id)
        .bind(posting_order)
        .bind(to_db_str(&posting.account_code)?)
        .bind(posting.financial_account_id.map(ResourceId::get))
        .bind(posting_obligation_id)
        .bind(posting.amount_krw)
        .execute(&mut **tx)
        .await
        .context("failed to write an annual-tax authority posting")?;
    }
    Ok(ledger_transaction_id)
}

fn create_filing_ledger(
    rules: &dyn FinanceRules,
    context: AnnualTaxRunContext,
    plan: &AnnualTaxFilingPlan,
) -> Result<Option<LedgerTransaction>> {
    let postings = match plan.cash_plan.movement {
        FilingMovement::Refund { wallet_credit_krw } => vec![
            LedgerPosting {
                account_code: LedgerAccountCode::Wallet,
                financial_account_id: None,
                amount_krw: wallet_credit_krw,
            },
            LedgerPosting {
                account_code: LedgerAccountCode::TaxSettlement,
                financial_account_id: None,
                amount_krw: wallet_credit_krw
                    .checked_neg()
                    .context("annual-tax refund ledger overflowed")?,
            },
        ],
        FilingMovement::AdditionalTax {
            wallet_debit_krw,
            aggregate_debt_increase_krw,
        } => {
            let total_tax_krw = wallet_debit_krw
                .checked_add(aggregate_debt_increase_krw)
                .context("annual-tax payment ledger overflowed")?;
            let mut postings = Vec::with_capacity(3);
            if wallet_debit_krw > 0 {
                postings.push(LedgerPosting {
                    account_code: LedgerAccountCode::Wallet,
                    financial_account_id: None,
                    amount_krw: wallet_debit_krw
                        .checked_neg()
                        .context("annual-tax wallet debit overflowed")?,
                });
            }
            if aggregate_debt_increase_krw > 0 {
                postings.push(LedgerPosting {
                    account_code: LedgerAccountCode::TaxObligationLiability,
                    financial_account_id: None,
                    amount_krw: aggregate_debt_increase_krw
                        .checked_neg()
                        .context("annual-tax debt posting overflowed")?,
                });
            }
            postings.push(LedgerPosting {
                account_code: LedgerAccountCode::TaxSettlement,
                financial_account_id: None,
                amount_krw: total_tax_krw,
            });
            postings
        }
        FilingMovement::NoMovement { .. } => return Ok(None),
    };
    let ledger = rules.create_ledger_transaction(LedgerTransactionDraft {
        policy: RunPolicyContext {
            run: RunId {
                save_id: ResourceId::from_u64(context.save_id),
                run_revision: context.run_revision,
            },
            policy_set_id: ResourceId::from_u64(context.policy_set_id),
        },
        source: LedgerSource {
            kind: LedgerSourceKind::ScheduledSettlement,
            source_id: plan.settlement_id.to_string(),
        },
        game_day: context.game_day,
        description: "금융소득 확정신고 정산".to_owned(),
        postings,
    })?;
    Ok(Some(ledger))
}

fn is_january_first(date: Date) -> bool {
    date.month() == Month::January && date.day() == 1
}

fn tax_year(date: Date) -> Result<u16> {
    let year = u16::try_from(date.year()).context("market date has an invalid tax year")?;
    ensure!(year > 0, "market date has an invalid tax year");
    Ok(year)
}

fn to_db_str<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value)? {
        Value::String(value) => Ok(value),
        _ => anyhow::bail!("database enum did not serialize as a string"),
    }
}

fn parse_db_str<T: DeserializeOwned>(raw: &str) -> Result<T> {
    serde_json::from_value(Value::String(raw.to_owned())).context("database enum value is invalid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finance::FilingNoMovementReason;
    use serde_json::json;

    fn given_date(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("유효한 테스트 날짜여야 한다")
    }

    fn given_policy_json() -> Value {
        json!({
            "comprehensiveThresholdKrw": 20_000_000,
            "generalIncomeTaxRatePpm": 140_000,
            "generalLocalIncomeTaxRatePpm": 14_000,
            "nonFinancialComprehensiveIncomeKrw": 0,
            "incomeTaxCreditKrw": 0,
            "localIncomeTaxCreditKrw": 0,
            "comparisonFormula": "independentMaxOfFormulaAAndB",
            "cashShortageTreatment": "interestFreeAggregateDebt",
            "incomeTaxBrackets": [
                { "lowerBoundKrw": 0, "upperBoundKrw": null, "ratePpm": 60_000 }
            ],
            "localIncomeTaxBrackets": [
                { "lowerBoundKrw": 0, "upperBoundKrw": null, "ratePpm": 6_000 }
            ],
            "sourceRates": [
                { "source": "cmaInterest", "incomeTaxRatePpm": 140_000, "localIncomeTaxRatePpm": 14_000 },
                { "source": "depositInterest", "incomeTaxRatePpm": 140_000, "localIncomeTaxRatePpm": 14_000 },
                { "source": "bondCoupon", "incomeTaxRatePpm": 140_000, "localIncomeTaxRatePpm": 14_000 },
                { "source": "llxDistribution", "incomeTaxRatePpm": 140_000, "localIncomeTaxRatePpm": 14_000 },
                { "source": "isaEarlyClose", "incomeTaxRatePpm": 140_000, "localIncomeTaxRatePpm": 14_000 }
            ],
            "filingDate": { "month": 5, "day": 31 }
        })
    }

    fn given_aggregate(
        gross_financial_income_krw: i64,
        withheld_income_tax_krw: i64,
        withheld_local_income_tax_krw: i64,
    ) -> FinancialIncomeAggregateRow {
        FinancialIncomeAggregateRow {
            gross_financial_income_krw,
            withheld_income_tax_krw,
            withheld_local_income_tax_krw,
        }
    }

    fn given_source_row(
        source: &str,
        gross_income_krw: i64,
        withheld_income_tax_krw: i64,
        withheld_local_income_tax_krw: i64,
    ) -> FinancialIncomeSourceYearRow {
        FinancialIncomeSourceYearRow {
            source: source.to_owned(),
            gross_income_krw,
            withheld_income_tax_krw,
            withheld_local_income_tax_krw,
        }
    }

    fn given_read_context(market_date: Date) -> AnnualTaxRunContext {
        AnnualTaxRunContext {
            save_id: 1,
            run_revision: 1,
            policy_set_id: 2,
            game_day: 0,
            market_date,
        }
    }

    fn given_assessment_read_row(
        draft: &AnnualAssessmentDraft,
        status: FinancialIncomeAssessmentStatus,
        filed_on: Option<Date>,
    ) -> AnnualTaxAssessmentReadRow {
        AnnualTaxAssessmentReadRow {
            policy_set_id: 2,
            year_end_tax_assessment_id: None,
            status: to_db_str(&status).expect("상태를 DB 문자열로 바꿔야 한다"),
            gross_financial_income_krw: draft.gross_financial_income_krw,
            other_comprehensive_income_krw: draft.other_comprehensive_income_krw,
            employment_taxable_income_krw: 0,
            employment_deductions_krw: 0,
            employment_final_prepaid_income_tax_krw: 0,
            employment_final_prepaid_local_income_tax_krw: 0,
            withheld_income_tax_krw: draft.withheld_income_tax_krw,
            withheld_local_income_tax_krw: draft.withheld_local_income_tax_krw,
            income_tax_formula_a_krw: draft.income_tax_formula_a_krw,
            income_tax_formula_b_krw: draft.income_tax_formula_b_krw,
            local_income_tax_formula_a_krw: draft.local_income_tax_formula_a_krw,
            local_income_tax_formula_b_krw: draft.local_income_tax_formula_b_krw,
            income_tax_credit_krw: draft.income_tax_credit_krw,
            local_income_tax_credit_krw: draft.local_income_tax_credit_krw,
            final_income_tax_krw: draft.final_income_tax_krw,
            final_local_income_tax_krw: draft.final_local_income_tax_krw,
            additional_tax_krw: draft.additional_tax_krw,
            refund_krw: draft.refund_krw,
            finalized_on: Some(draft.finalization_date),
            filing_date: draft.filing_date,
            filed_on,
        }
    }

    mod context_persisted_annual_tax_policy_is_parsed {
        use super::*;

        #[test]
        fn given_the_exact_twelve_fields_when_parsed_then_the_pure_policy_is_returned() {
            let raw = given_policy_json().to_string();

            let result = parse_annual_tax_policy(&raw).expect("정책을 strict parse해야 한다");

            assert_eq!(result.policy.comprehensive_threshold_krw, 20_000_000);
            assert_eq!(result.other_comprehensive_income_krw, 0);
            assert_eq!(result.credits, TaxCredits::default());
        }

        #[test]
        fn given_an_unknown_field_when_parsed_then_it_is_rejected() {
            let mut value = given_policy_json();
            value["unknown"] = json!(true);

            let result = parse_annual_tax_policy(&value.to_string());

            assert!(result.is_err());
        }

        #[test]
        fn given_a_different_comparison_literal_when_parsed_then_it_is_rejected() {
            let mut value = given_policy_json();
            value["comparisonFormula"] = json!("formulaAOnly");

            let result = parse_annual_tax_policy(&value.to_string());

            assert!(result.is_err());
        }

        #[test]
        fn given_nonzero_m2_defaults_when_parsed_then_they_are_rejected() {
            let mut value = given_policy_json();
            value["incomeTaxCreditKrw"] = json!(1);

            let result = parse_annual_tax_policy(&value.to_string());

            assert!(result.is_err());
        }

        #[test]
        fn given_an_invalid_bracket_when_parsed_then_it_is_rejected_by_the_pure_policy() {
            let mut value = given_policy_json();
            value["incomeTaxBrackets"][0]["ratePpm"] = json!(0);

            let result = parse_annual_tax_policy(&value.to_string());

            assert!(result.is_err());
        }
    }

    mod context_source_rows_are_synthesized {
        use super::*;

        #[test]
        fn given_rows_equal_the_aggregate_when_synthesized_then_sources_are_canonical_order() {
            let aggregate = given_aggregate(3_000, 420, 42);
            let rows = vec![
                given_source_row("depositInterest", 2_000, 280, 28),
                given_source_row("cmaInterest", 1_000, 140, 14),
            ];

            let result = synthesize_source_years(&aggregate, rows)
                .expect("일치하는 source 누계를 합성해야 한다");
            let states = canonical_source_states(&result);

            assert_eq!(result.len(), 2);
            assert_eq!(result[0].source, FinancialIncomeSource::CmaInterest);
            assert_eq!(result[1].source, FinancialIncomeSource::DepositInterest);
            assert_eq!(states.len(), 6);
            assert_eq!(states[0].gross_financial_income_krw, 1_000);
            assert_eq!(states[4].source, FinancialIncomeSource::IsaEarlyClose);
            assert_eq!(states[4].gross_financial_income_krw, 0);
            assert_eq!(states[5].source, FinancialIncomeSource::CorporationDividend);
            assert_eq!(states[5].gross_financial_income_krw, 0);
        }

        #[test]
        fn given_rows_disagree_with_the_aggregate_when_synthesized_then_they_are_rejected() {
            let aggregate = given_aggregate(3_001, 420, 42);
            let rows = vec![given_source_row("cmaInterest", 3_000, 420, 42)];

            let result = synthesize_source_years(&aggregate, rows);

            assert!(result.is_err());
        }

        #[test]
        fn given_an_unknown_source_when_synthesized_then_it_is_rejected() {
            let aggregate = given_aggregate(1_000, 140, 14);
            let rows = vec![given_source_row("unknown", 1_000, 140, 14)];

            let result = synthesize_source_years(&aggregate, rows);

            assert!(result.is_err());
        }

        #[test]
        fn given_a_duplicate_source_when_synthesized_then_it_is_rejected() {
            let aggregate = given_aggregate(2_000, 280, 28);
            let rows = vec![
                given_source_row("cmaInterest", 1_000, 140, 14),
                given_source_row("cmaInterest", 1_000, 140, 14),
            ];

            let result = synthesize_source_years(&aggregate, rows);

            assert!(result.is_err());
        }
    }

    mod context_assessment_rows_are_read {
        use super::*;

        fn given_finalized_draft(
            gross_income_krw: i64,
        ) -> (
            AnnualTaxRuntimePolicy,
            FinancialIncomeAggregateRow,
            Vec<FinancialIncomeSourceYear>,
            AnnualAssessmentDraft,
        ) {
            let runtime = parse_annual_tax_policy(&given_policy_json().to_string())
                .expect("테스트 정책을 읽어야 한다");
            let source_years = vec![FinancialIncomeSourceYear {
                source: FinancialIncomeSource::CmaInterest,
                gross_income_krw,
                withheld_income_tax_krw: gross_income_krw * 14 / 100,
                withheld_local_income_tax_krw: gross_income_krw * 14 / 1_000,
            }];
            let draft = finalize_annual_assessment(
                &runtime.policy,
                &AnnualAssessmentFinalizeInput {
                    tax_year: 2026,
                    finalization_date: given_date(2027, Month::January, 1),
                    current_status: FinancialIncomeAssessmentStatus::Open,
                    source_years: source_years.clone(),
                    other_comprehensive_income_krw: 0,
                    credits: TaxCredits::default(),
                },
            )
            .expect("연간 세액 초안을 계산해야 한다");
            let aggregate = given_aggregate(
                draft.gross_financial_income_krw,
                draft.withheld_income_tax_krw,
                draft.withheld_local_income_tax_krw,
            );
            (runtime, aggregate, source_years, draft)
        }

        #[test]
        fn given_a_no_filing_row_when_read_then_all_calculated_values_are_non_nullable() {
            let (runtime, aggregate, source_years, draft) = given_finalized_draft(20_000_000);
            let row = given_assessment_read_row(
                &draft,
                FinancialIncomeAssessmentStatus::FinalizedNoFiling,
                None,
            );

            let result = assessment_state_from_row(
                given_read_context(given_date(2027, Month::January, 1)),
                2026,
                &aggregate,
                &source_years,
                &runtime,
                &row,
                FinancialIncomeAssessmentStatus::FinalizedNoFiling,
                None,
            )
            .expect("신고 없는 확정 상태를 읽어야 한다");

            let AnnualTaxAssessmentState::FinalizedNoFiling { calculated } = result else {
                panic!("신고 없는 확정 상태여야 한다");
            };
            assert_eq!(calculated.additional_tax_krw, 0);
            assert_eq!(calculated.refund_krw, 0);
            assert_eq!(
                calculated.assessed_income_tax_krw,
                aggregate.withheld_income_tax_krw
            );
        }

        #[test]
        fn given_a_filed_row_when_read_then_the_filed_date_becomes_a_checked_game_day() {
            let (runtime, aggregate, source_years, draft) = given_finalized_draft(20_000_001);
            let filing_date = draft.filing_date.expect("신고일이 있어야 한다");
            let row = given_assessment_read_row(
                &draft,
                FinancialIncomeAssessmentStatus::Filed,
                Some(filing_date),
            );

            let result = assessment_state_from_row(
                given_read_context(filing_date),
                2026,
                &aggregate,
                &source_years,
                &runtime,
                &row,
                FinancialIncomeAssessmentStatus::Filed,
                Some(given_date(2026, Month::January, 1)),
            )
            .expect("신고 완료 상태를 읽어야 한다");

            let AnnualTaxAssessmentState::Filed {
                filing_due_date,
                filed_game_day,
                ..
            } = result
            else {
                panic!("신고 완료 상태여야 한다");
            };
            assert_eq!(filing_due_date, filing_date);
            assert_eq!(filed_game_day, 515);
        }

        #[test]
        fn given_a_finalized_row_that_disagrees_with_sources_when_read_then_it_is_rejected() {
            let (runtime, aggregate, source_years, draft) = given_finalized_draft(20_000_001);
            let mut row = given_assessment_read_row(
                &draft,
                FinancialIncomeAssessmentStatus::FilingPending,
                None,
            );
            row.final_income_tax_krw += 1;

            let result = assessment_state_from_row(
                given_read_context(given_date(2027, Month::January, 1)),
                2026,
                &aggregate,
                &source_years,
                &runtime,
                &row,
                FinancialIncomeAssessmentStatus::FilingPending,
                None,
            );

            assert!(result.is_err());
        }

        #[test]
        fn given_an_open_row_with_finalized_values_when_read_then_it_is_rejected() {
            let (_, _, _, draft) = given_finalized_draft(20_000_000);
            let row =
                given_assessment_read_row(&draft, FinancialIncomeAssessmentStatus::Open, None);

            let result = ensure_open_assessment_read_row(&row);

            assert!(result.is_err());
        }
    }

    mod context_assessment_status_is_planned {
        use super::*;

        #[test]
        fn given_open_on_january_first_when_planned_then_previous_year_is_finalized() {
            let result = should_finalize_previous(
                given_date(2027, Month::January, 1),
                FinancialIncomeAssessmentStatus::Open,
            );

            assert!(result);
        }

        #[test]
        fn given_already_finalized_on_january_first_when_planned_then_it_is_not_recomputed() {
            let result = should_finalize_previous(
                given_date(2027, Month::January, 1),
                FinancialIncomeAssessmentStatus::FilingPending,
            );

            assert!(!result);
        }

        #[test]
        fn given_open_on_another_date_when_planned_then_it_is_not_finalized() {
            let result = should_finalize_previous(
                given_date(2027, Month::January, 2),
                FinancialIncomeAssessmentStatus::Open,
            );

            assert!(!result);
        }

        #[test]
        fn given_pending_debt_shortage_when_filing_is_planned_then_the_pure_plan_is_preserved() {
            let row = FilingAssessmentRow {
                status: "filingPending".to_owned(),
                filing_date: Some(given_date(2027, Month::May, 31)),
                additional_tax_krw: 1_200,
                refund_krw: 0,
            };

            let result = plan_filing_from_row(&row, given_date(2027, Month::May, 31), 1_000, 200)
                .expect("신고 현금 계획을 생성해야 한다");

            assert_eq!(result.wallet_cash_krw, 0);
            assert_eq!(result.aggregate_debt_krw, 400);
            assert_eq!(result.next_status, FinancialIncomeAssessmentStatus::Filed);
        }

        #[test]
        fn given_finalized_without_filing_when_filing_is_planned_then_it_is_rejected() {
            let row = FilingAssessmentRow {
                status: "finalizedNoFiling".to_owned(),
                filing_date: Some(given_date(2027, Month::May, 31)),
                additional_tax_krw: 0,
                refund_krw: 0,
            };

            let result = plan_filing_from_row(&row, given_date(2027, Month::May, 31), 1_000, 200);

            assert!(result.is_err());
        }

        #[test]
        fn given_zero_tax_pending_when_filing_is_planned_then_no_movement_is_preserved() {
            let row = FilingAssessmentRow {
                status: "filingPending".to_owned(),
                filing_date: Some(given_date(2027, Month::May, 31)),
                additional_tax_krw: 0,
                refund_krw: 0,
            };

            let result = plan_filing_from_row(&row, given_date(2027, Month::May, 31), 1_000, 200)
                .expect("0원 신고 계획을 생성해야 한다");

            assert_eq!(
                result.movement,
                FilingMovement::NoMovement {
                    reason: FilingNoMovementReason::ZeroTaxDue,
                }
            );
        }
    }

    mod context_filing_payload_is_parsed {
        use super::*;

        #[test]
        fn given_an_unknown_payload_field_when_decoded_then_it_is_rejected() {
            let row = FilingScheduleRow {
                id: 1,
                due_game_day: 515,
                kind: FILING_SETTLEMENT_KIND.to_owned(),
                payload_json: json!({
                    "schemaVersion": 1,
                    "taxYear": 2026,
                    "unknown": true
                })
                .to_string(),
                source_kind: FILING_SETTLEMENT_SOURCE_KIND.to_owned(),
                source_id: "2026".to_owned(),
                occurrence: 1,
                status: "pending".to_owned(),
            };

            let result = decode_filing_schedule(&row);

            assert!(result.is_err());
        }
    }

    mod context_세금_부족분_ledger를_만드는_경우 {
        use super::*;

        #[test]
        fn given_현금부족분_when_ledger를만들면_then_세금의무계정에기록한다() {
            let context = given_read_context(given_date(2027, Month::May, 31));
            let plan = AnnualTaxFilingPlan {
                settlement_id: 17,
                tax_year: 2026,
                execution_date: context.market_date,
                expected_wallet_cash_krw: 1_000,
                expected_aggregate_debt_krw: 200,
                cash_plan: FilingCashPlan {
                    next_status: FinancialIncomeAssessmentStatus::Filed,
                    wallet_cash_krw: 0,
                    aggregate_debt_krw: 400,
                    movement: FilingMovement::AdditionalTax {
                        wallet_debit_krw: 1_000,
                        aggregate_debt_increase_krw: 200,
                    },
                },
                context,
            };

            let ledger = create_filing_ledger(
                crate::finance::create_finance_rules().as_ref(),
                context,
                &plan,
            )
            .expect("정산 ledger를 만들 수 있어야 한다")
            .expect("부족분 정산에는 ledger가 있어야 한다");

            assert_eq!(
                ledger
                    .postings()
                    .iter()
                    .map(|posting| posting.account_code)
                    .collect::<Vec<_>>(),
                vec![
                    LedgerAccountCode::Wallet,
                    LedgerAccountCode::TaxObligationLiability,
                    LedgerAccountCode::TaxSettlement,
                ]
            );
        }
    }
}
