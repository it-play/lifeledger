//! Transaction-scoped persistence helpers for M3-C annual employment tax (§12–§14).

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{MySql, MySqlPool, Transaction};
use time::{Date, Month};

use super::annual_tax::{
    AnnualEmploymentAssessmentLink, AnnualTaxRunContext, finalize_previous_tax_year,
    persist_assessment_draft_with_employment, plan_previous_tax_year_with_employment,
    schedule_annual_tax_filing_if_needed,
};
use super::life::read_tax_dependent_count_in_tx;
use super::mysql::write_ledger_transaction;
use super::types::{
    CareerEmploymentTaxYearSource, CareerEmploymentTaxYearState, CareerEmploymentTaxYearStatus,
};
use crate::career::{
    AnnualLocalIncomeTaxPolicy, CombinedEmploymentTaxPlanningInput, EarnedIncomeDeductionBracket,
    EarnedIncomeTaxCreditPolicy, EmployeeStatutoryInsuranceAmounts, EmploymentAnnualTaxPolicy,
    EmploymentIncomeAuthority, EmploymentOnlyTaxPlanningInput, EmploymentTaxAssessmentPlan,
    EmploymentTaxAssessmentSource, EmploymentTaxAssessmentStatus, PensionContributionAccountKind,
    PensionContributionCreditPolicy, PensionContributionCreditRate, PensionContributionEvent,
    PensionContributionSourceEvent, PensionOpeningTaxExcludedBalance, PensionWithdrawalEvent,
    ProgressiveEmploymentTaxBracket,
};
use crate::finance::{
    AnnualAssessmentDraft, FinanceRules, FinancialIncomeAssessmentStatus, LedgerAccountCode,
    LedgerPosting, LedgerSource, LedgerSourceKind, LedgerTransaction, LedgerTransactionDraft,
    PensionPolicy, ResourceId, RunId, RunPolicyContext, TaxCredits,
};

const RECONCILIATION_KIND: &str = "employmentReconciliation";
const RECONCILIATION_SOURCE_KIND: &str = "yearEndTaxAssessment";
const RECONCILIATION_OCCURRENCE: u64 = 1;
const RECONCILIATION_PAYLOAD_VERSION: u8 = 1;
const EMPLOYMENT_PAYROLL_KIND: &str = "employmentPayroll";
const EMPLOYMENT_CONTRACT_SOURCE_KIND: &str = "employmentContract";
const MILITARY_PAY_KIND: &str = "militaryPay";
const MILITARY_SERVICE_SOURCE_KIND: &str = "militaryService";
const PENSION_POLICY_DOMAIN: &str = "pension";
const PENSION_POLICY_RULE: &str = "contributionAndWithdrawal";

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
struct EmploymentTaxScopeRow {
    save_id: u64,
    run_revision: u32,
    market_date: Date,
    world_start_date: Date,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct IncomeYearReadRow {
    status: String,
    income_event_count: u64,
    last_income_event_id: Option<u64>,
    gross_employment_income_krw: i64,
    employee_insurance_total_krw: i64,
    withheld_income_tax_krw: i64,
    withheld_local_income_tax_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct AssessmentReadRow {
    tax_year: u16,
    assessment_kind: String,
    assessment_status: String,
    gross_employment_income_krw: i64,
    employment_income_deduction_krw: i64,
    basic_personal_deduction_krw: i64,
    insurance_income_deduction_krw: i64,
    taxable_employment_income_krw: i64,
    calculated_income_tax_krw: i64,
    employment_income_tax_credit_krw: i64,
    pension_credit_eligible_krw: i64,
    actual_pension_income_tax_credit_krw: i64,
    actual_pension_local_tax_effect_krw: i64,
    final_income_tax_krw: i64,
    final_local_income_tax_krw: i64,
    withheld_income_tax_krw: i64,
    withheld_local_income_tax_krw: i64,
    additional_tax_krw: i64,
    refund_krw: i64,
    reconciliation_income_gross_krw: i64,
    reconciliation_income_insurance_krw: i64,
    reconciliation_income_withheld_income_tax_krw: i64,
    reconciliation_income_withheld_local_income_tax_krw: i64,
    reconciliation_income_event_count: u64,
    reconciliation_last_income_event_id: Option<u64>,
    reconciliation_assessment_gross_krw: i64,
    reconciliation_assessment_insurance_krw: i64,
    reconciliation_prepaid_income_tax_krw: i64,
    reconciliation_prepaid_local_income_tax_krw: i64,
    reconciliation_final_income_tax_krw: i64,
    reconciliation_final_local_income_tax_krw: i64,
    reconciliation_additional_tax_krw: i64,
    reconciliation_refund_krw: i64,
    reconciliation_game_day: Option<u32>,
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
struct LegacyTaxProfileRow {
    prior_year_employment_income_krw: i64,
    prior_year_total_salary_krw: i64,
}

pub(super) async fn read_career_employment_tax_year(
    pool: &MySqlPool,
    user_id: u64,
    tax_year: u16,
) -> Result<CareerEmploymentTaxYearState> {
    ensure!(tax_year > 0, "employment tax year must be positive");
    let mut tx = pool.begin().await?;
    let scope: EmploymentTaxScopeRow = sqlx::query_as(
        "SELECT save.id AS save_id, save.run_revision,
                COALESCE(daily.market_date,
                    DATE_ADD(world.start_date, INTERVAL save.game_day DAY)) AS market_date,
                world.start_date AS world_start_date
         FROM save
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         LEFT JOIN market_daily AS daily
           ON daily.world_id = save.market_world_id AND daily.game_day = save.game_day
         WHERE save.user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?
    .context("career tax year requires an active save")?;
    let state = read_tax_year_state_in_tx(
        &mut tx,
        scope.save_id,
        scope.run_revision,
        tax_year,
        scope.market_date.year(),
        scope.world_start_date.year(),
    )
    .await?;
    tx.commit().await?;
    Ok(state)
}

pub(super) async fn read_employment_tax_snapshot_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    current_tax_year: u16,
    world_start_year: i32,
) -> Result<(
    CareerEmploymentTaxYearState,
    Option<CareerEmploymentTaxYearState>,
)> {
    let current = read_tax_year_state_in_tx(
        tx,
        save_id,
        run_revision,
        current_tax_year,
        i32::from(current_tax_year),
        world_start_year,
    )
    .await?;
    let latest_m3_year: Option<u16> = sqlx::query_scalar(
        "SELECT tax_year
         FROM year_end_tax_assessment
         WHERE save_id = ? AND run_revision = ? AND assessment_status = 'definitive'
         ORDER BY tax_year DESC LIMIT 1",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await?;
    let latest = if let Some(tax_year) = latest_m3_year {
        Some(
            read_tax_year_state_in_tx(
                tx,
                save_id,
                run_revision,
                tax_year,
                i32::from(current_tax_year),
                world_start_year,
            )
            .await?,
        )
    } else if world_start_year > 1 {
        let legacy_year = u16::try_from(world_start_year - 1)
            .context("legacy employment tax year is out of range")?;
        read_legacy_tax_year(tx, save_id, run_revision, legacy_year, world_start_year).await?
    } else {
        None
    };
    Ok((current, latest))
}

async fn read_tax_year_state_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    tax_year: u16,
    current_year: i32,
    world_start_year: i32,
) -> Result<CareerEmploymentTaxYearState> {
    if let Some(row) = read_assessment(tx, save_id, run_revision, tax_year).await? {
        return assessment_state(row);
    }
    if let Some(row) = read_income_year(tx, save_id, run_revision, tax_year).await? {
        income_event_authority(row.income_event_count, row.last_income_event_id)?;
        ensure!(
            row.status == "open",
            "finalized income year has no tax assessment"
        );
        return Ok(CareerEmploymentTaxYearState {
            tax_year,
            gross_employment_income_krw: row.gross_employment_income_krw,
            employee_insurance_deduction_krw: Some(row.employee_insurance_total_krw),
            withheld_income_tax_krw: Some(row.withheld_income_tax_krw),
            withheld_local_income_tax_krw: Some(row.withheld_local_income_tax_krw),
            ..CareerEmploymentTaxYearState::open(tax_year)
        });
    }
    if let Some(legacy) =
        read_legacy_tax_year(tx, save_id, run_revision, tax_year, world_start_year).await?
    {
        return Ok(legacy);
    }
    ensure!(
        i32::from(tax_year) >= current_year,
        "requested employment tax year has no authoritative record"
    );
    Ok(CareerEmploymentTaxYearState::open(tax_year))
}

async fn read_income_year(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    tax_year: u16,
) -> Result<Option<IncomeYearReadRow>> {
    sqlx::query_as(
        "SELECT status, income_event_count, last_income_event_id,
                gross_employment_income_krw, employee_insurance_total_krw,
                withheld_income_tax_krw, withheld_local_income_tax_krw
         FROM employment_income_year
         WHERE save_id = ? AND run_revision = ? AND tax_year = ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(tax_year)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read the employment income year")
}

async fn read_assessment(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    tax_year: u16,
) -> Result<Option<AssessmentReadRow>> {
    sqlx::query_as(
        "SELECT assessment.tax_year, assessment.assessment_kind,
                assessment.assessment_status, assessment.gross_employment_income_krw,
                assessment.employment_income_deduction_krw,
                assessment.basic_personal_deduction_krw,
                assessment.insurance_income_deduction_krw,
                assessment.taxable_employment_income_krw,
                assessment.calculated_income_tax_krw,
                assessment.employment_income_tax_credit_krw,
                assessment.pension_credit_eligible_krw,
                assessment.actual_pension_income_tax_credit_krw,
                assessment.actual_pension_local_tax_effect_krw,
                assessment.final_income_tax_krw, assessment.final_local_income_tax_krw,
                income_year.withheld_income_tax_krw,
                income_year.withheld_local_income_tax_krw,
                assessment.additional_tax_krw, assessment.refund_krw,
                income_year.gross_employment_income_krw
                    AS reconciliation_income_gross_krw,
                income_year.employee_insurance_total_krw
                    AS reconciliation_income_insurance_krw,
                income_year.withheld_income_tax_krw
                    AS reconciliation_income_withheld_income_tax_krw,
                income_year.withheld_local_income_tax_krw
                    AS reconciliation_income_withheld_local_income_tax_krw,
                income_year.income_event_count AS reconciliation_income_event_count,
                income_year.last_income_event_id AS reconciliation_last_income_event_id,
                employment_only.gross_employment_income_krw
                    AS reconciliation_assessment_gross_krw,
                employment_only.insurance_income_deduction_krw
                    AS reconciliation_assessment_insurance_krw,
                employment_only.prepaid_income_tax_krw
                    AS reconciliation_prepaid_income_tax_krw,
                employment_only.prepaid_local_income_tax_krw
                    AS reconciliation_prepaid_local_income_tax_krw,
                employment_only.final_income_tax_krw
                    AS reconciliation_final_income_tax_krw,
                employment_only.final_local_income_tax_krw
                    AS reconciliation_final_local_income_tax_krw,
                employment_only.additional_tax_krw
                    AS reconciliation_additional_tax_krw,
                employment_only.refund_krw AS reconciliation_refund_krw,
                settlement.due_game_day AS reconciliation_game_day
         FROM year_end_tax_assessment AS assessment
         INNER JOIN year_end_tax_assessment AS employment_only
           ON employment_only.save_id = assessment.save_id
          AND employment_only.run_revision = assessment.run_revision
          AND employment_only.tax_year = assessment.tax_year
          AND employment_only.assessment_kind = 'employmentOnly'
         INNER JOIN employment_income_year AS income_year
           ON income_year.save_id = assessment.save_id
          AND income_year.run_revision = assessment.run_revision
          AND income_year.tax_year = assessment.tax_year
         LEFT JOIN scheduled_settlement AS settlement
           ON settlement.save_id = employment_only.save_id
          AND settlement.run_revision = employment_only.run_revision
          AND BINARY settlement.source_kind = BINARY 'yearEndTaxAssessment'
          AND BINARY settlement.source_id = BINARY CAST(employment_only.id AS CHAR)
          AND settlement.occurrence = 1
         WHERE assessment.save_id = ? AND assessment.run_revision = ?
           AND assessment.tax_year = ?
         ORDER BY assessment.assessment_status = 'definitive' DESC,
                  assessment.assessment_kind = 'combined' DESC
         LIMIT 1",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(tax_year)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read the employment tax assessment")
}

fn assessment_state(row: AssessmentReadRow) -> Result<CareerEmploymentTaxYearState> {
    let has_income_event_authority = income_event_authority(
        row.reconciliation_income_event_count,
        row.reconciliation_last_income_event_id,
    )?;
    let status = match row.assessment_status.as_str() {
        "provisional" => CareerEmploymentTaxYearStatus::Provisional,
        "definitive" => CareerEmploymentTaxYearStatus::Definitive,
        _ => bail!("stored employment tax assessment status is invalid"),
    };
    let source = match row.assessment_kind.as_str() {
        "employmentOnly" => CareerEmploymentTaxYearSource::EmploymentOnly,
        "combined" => CareerEmploymentTaxYearSource::Combined,
        _ => bail!("stored employment tax assessment kind is invalid"),
    };
    if row.reconciliation_game_day.is_some() {
        ensure!(
            has_income_event_authority,
            "employment reconciliation has no income-event authority"
        );
    } else if status == CareerEmploymentTaxYearStatus::Definitive {
        ensure!(
            assessment_allows_omitted_reconciliation(&row),
            "non-zero definitive employment tax assessment has no reconciliation schedule"
        );
    }
    Ok(CareerEmploymentTaxYearState {
        tax_year: row.tax_year,
        status,
        source,
        gross_employment_income_krw: row.gross_employment_income_krw,
        employee_insurance_deduction_krw: Some(row.insurance_income_deduction_krw),
        earned_income_deduction_krw: Some(row.employment_income_deduction_krw),
        personal_deduction_krw: Some(row.basic_personal_deduction_krw),
        taxable_income_krw: Some(row.taxable_employment_income_krw),
        calculated_income_tax_krw: Some(row.calculated_income_tax_krw),
        earned_income_tax_credit_krw: Some(row.employment_income_tax_credit_krw),
        pension_credit_eligible_contribution_krw: Some(row.pension_credit_eligible_krw),
        actual_pension_income_tax_credit_krw: Some(row.actual_pension_income_tax_credit_krw),
        actual_pension_local_income_tax_effect_krw: Some(row.actual_pension_local_tax_effect_krw),
        withheld_income_tax_krw: Some(row.withheld_income_tax_krw),
        withheld_local_income_tax_krw: Some(row.withheld_local_income_tax_krw),
        assessed_income_tax_krw: Some(row.final_income_tax_krw),
        assessed_local_income_tax_krw: Some(row.final_local_income_tax_krw),
        additional_tax_krw: Some(row.additional_tax_krw),
        refund_krw: Some(row.refund_krw),
        reconciliation_game_day: row.reconciliation_game_day,
    })
}

fn assessment_allows_omitted_reconciliation(row: &AssessmentReadRow) -> bool {
    row.reconciliation_income_event_count == 0
        && row.reconciliation_last_income_event_id.is_none()
        && row.reconciliation_income_gross_krw == 0
        && row.reconciliation_income_insurance_krw == 0
        && row.reconciliation_income_withheld_income_tax_krw == 0
        && row.reconciliation_income_withheld_local_income_tax_krw == 0
        && row.reconciliation_assessment_gross_krw == 0
        && row.reconciliation_assessment_insurance_krw == 0
        && row.actual_pension_income_tax_credit_krw == 0
        && row.actual_pension_local_tax_effect_krw == 0
        && row.reconciliation_prepaid_income_tax_krw == 0
        && row.reconciliation_prepaid_local_income_tax_krw == 0
        && row.reconciliation_final_income_tax_krw == 0
        && row.reconciliation_final_local_income_tax_krw == 0
        && row.reconciliation_additional_tax_krw == 0
        && row.reconciliation_refund_krw == 0
}

async fn read_legacy_tax_year(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    tax_year: u16,
    world_start_year: i32,
) -> Result<Option<CareerEmploymentTaxYearState>> {
    if i32::from(tax_year) != world_start_year - 1 {
        return Ok(None);
    }
    let row: Option<LegacyTaxProfileRow> = sqlx::query_as(
        "SELECT prior_year_employment_income_krw, prior_year_total_salary_krw
         FROM run_tax_profile WHERE save_id = ? AND run_revision = ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|row| CareerEmploymentTaxYearState {
        tax_year,
        status: CareerEmploymentTaxYearStatus::Definitive,
        source: CareerEmploymentTaxYearSource::LegacyProfile,
        gross_employment_income_krw: row.prior_year_total_salary_krw,
        employee_insurance_deduction_krw: None,
        earned_income_deduction_krw: None,
        personal_deduction_krw: None,
        taxable_income_krw: Some(row.prior_year_employment_income_krw),
        calculated_income_tax_krw: None,
        earned_income_tax_credit_krw: None,
        pension_credit_eligible_contribution_krw: None,
        actual_pension_income_tax_credit_krw: None,
        actual_pension_local_income_tax_effect_krw: None,
        withheld_income_tax_krw: None,
        withheld_local_income_tax_krw: None,
        assessed_income_tax_krw: None,
        assessed_local_income_tax_krw: None,
        additional_tax_krw: None,
        refund_krw: None,
        reconciliation_game_day: None,
    }))
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct IncomeYearClosingRow {
    employment_policy_set_id: u64,
    status: String,
    income_event_count: u64,
    last_income_event_id: Option<u64>,
    gross_employment_income_krw: i64,
    employee_national_pension_krw: i64,
    employee_health_insurance_krw: i64,
    employee_long_term_care_krw: i64,
    employee_employment_insurance_krw: i64,
    withheld_income_tax_krw: i64,
    withheld_local_income_tax_krw: i64,
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
struct AnnualPolicyRow {
    id: u64,
    policy_tax_year: u16,
    ranked_eligible: bool,
    february_reconciliation_day_of_month: u8,
    basic_personal_deduction_krw: i64,
    taxable_income_rounding_unit_krw: i64,
    calculated_tax_rounding_unit_krw: i64,
    income_tax_credit_low_tax_boundary_krw: i64,
    income_tax_credit_low_rate_ppm: i64,
    income_tax_credit_high_base_krw: i64,
    income_tax_credit_high_rate_ppm: i64,
    credit_cap_salary_boundary_one_krw: i64,
    credit_cap_salary_boundary_two_krw: i64,
    credit_cap_one_krw: i64,
    credit_cap_two_base_krw: i64,
    credit_cap_two_reduction_rate_ppm: i64,
    credit_cap_two_floor_krw: i64,
    credit_cap_three_base_krw: i64,
    credit_cap_three_reduction_rate_ppm: i64,
    credit_cap_three_floor_krw: i64,
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
struct EarnedDeductionRow {
    lower_bound_krw: i64,
    upper_bound_exclusive_krw: Option<i64>,
    base_deduction_krw: i64,
    marginal_rate_ppm: i64,
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
struct BasicTaxBracketRow {
    lower_bound_krw: i64,
    upper_bound_exclusive_krw: Option<i64>,
    rate_ppm: i64,
    quick_deduction_krw: i64,
}

#[derive(Debug, Clone)]
struct LoadedAnnualPolicy {
    id: u64,
    february_reconciliation_day_of_month: u8,
    policy: EmploymentAnnualTaxPolicy,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PensionEventRow {
    id: u64,
    financial_account_id: u64,
    account_type: String,
    event_kind: String,
    game_day: u32,
    movement_amount_krw: i64,
    payload_json: String,
    ledger_transaction_id: Option<u64>,
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
struct PensionBalanceRow {
    financial_account_id: u64,
    tax_excluded_contribution_krw: i64,
}

#[derive(Debug, Clone)]
struct PensionSources {
    opening: Vec<PensionOpeningTaxExcludedBalance>,
    events: Vec<PensionContributionSourceEvent>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ContributionPayload {
    version: u8,
    amount_krw: i64,
    allocations: Vec<ContributionPayloadAllocation>,
    tax_layers_after: PensionLayersPayload,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ContributionPayloadAllocation {
    account_id: String,
    total_contribution_krw: i64,
    credit_eligible_krw: i64,
    expected_credit_rate_ppm: i64,
    expected_credit_krw: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PensionLayersPayload {
    tax_excluded_contribution_krw: i64,
    deferred_retirement_income_krw: i64,
    credited_contribution_krw: i64,
    earnings_krw: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WithdrawalPayload {
    version: u8,
    request_kind: String,
    reason: Option<String>,
    gross_amount_krw: i64,
    pension_amount_krw: i64,
    non_pension_amount_krw: i64,
    tax_free_amount_krw: i64,
    tax_krw: i64,
    net_payout_krw: i64,
    remaining_layers: PensionLayersPayload,
    portions: Vec<WithdrawalPortionPayload>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WithdrawalPortionPayload {
    treatment: String,
    gross_amount_krw: i64,
    tax_free_amount_krw: i64,
    tax_krw: i64,
    net_amount_krw: i64,
    tax_lines: Vec<WithdrawalTaxLinePayload>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WithdrawalTaxLinePayload {
    source: String,
    gross_amount_krw: i64,
    tax_rate: serde_json::Value,
    tax_krw: i64,
    net_amount_krw: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReconciliationPayload {
    version: u8,
    tax_year: u16,
    assessment_id: ResourceId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReconciliationAnchor {
    EmploymentPayroll {
        settlement_id: u64,
        due_game_day: u32,
    },
    MilitaryPay {
        settlement_id: u64,
        due_game_day: u32,
    },
    PolicyFallback {
        due_game_day: u32,
    },
}

impl ReconciliationAnchor {
    fn due_game_day(self) -> u32 {
        match self {
            Self::EmploymentPayroll { due_game_day, .. }
            | Self::MilitaryPay { due_game_day, .. }
            | Self::PolicyFallback { due_game_day } => due_game_day,
        }
    }

    fn settlement_id(self) -> Option<u64> {
        match self {
            Self::EmploymentPayroll { settlement_id, .. }
            | Self::MilitaryPay { settlement_id, .. } => Some(settlement_id),
            Self::PolicyFallback { .. } => None,
        }
    }
}

fn choose_reconciliation_anchor(
    private_payroll: Option<ReconciliationAnchor>,
    military_pay: Option<ReconciliationAnchor>,
    policy_game_day: u32,
) -> ReconciliationAnchor {
    private_payroll
        .or(military_pay)
        .unwrap_or(ReconciliationAnchor::PolicyFallback {
            due_game_day: policy_game_day,
        })
}

fn income_event_authority(
    income_event_count: u64,
    last_income_event_id: Option<u64>,
) -> Result<bool> {
    match (income_event_count, last_income_event_id) {
        (0, None) => Ok(false),
        (count, Some(_)) if count > 0 => Ok(true),
        _ => bail!("employment income-event authority is inconsistent"),
    }
}

fn february_reconciliation_date(reconciliation_year: i32, requested_day: u8) -> Result<Date> {
    ensure!(
        (1..=31).contains(&requested_day),
        "employment annual policy has an invalid February reconciliation day"
    );
    let day = requested_day.min(Month::February.length(reconciliation_year));
    Date::from_calendar_date(reconciliation_year, Month::February, day)
        .context("employment reconciliation date is invalid")
}

fn game_day_from_annual_boundary(context: AnnualTaxRunContext, date: Date) -> Result<u32> {
    ensure!(
        context.market_date.month() == Month::January && context.market_date.day() == 1,
        "employment reconciliation anchor requires the annual boundary"
    );
    let day_offset = u32::try_from((date - context.market_date).whole_days())
        .context("employment reconciliation date precedes the annual boundary")?;
    context
        .game_day
        .checked_add(day_offset)
        .context("employment reconciliation game day overflowed")
}

fn policy_reconciliation_game_day(
    context: AnnualTaxRunContext,
    tax_year: u16,
    requested_day: u8,
) -> Result<u32> {
    let date = february_reconciliation_date(i32::from(tax_year) + 1, requested_day)?;
    game_day_from_annual_boundary(context, date)
}

fn february_game_day_range(context: AnnualTaxRunContext, tax_year: u16) -> Result<(u32, u32)> {
    let reconciliation_year = i32::from(tax_year) + 1;
    let start = Date::from_calendar_date(reconciliation_year, Month::February, 1)
        .context("employment reconciliation February start is invalid")?;
    let end = Date::from_calendar_date(reconciliation_year, Month::March, 1)
        .context("employment reconciliation March start is invalid")?;
    Ok((
        game_day_from_annual_boundary(context, start)?,
        game_day_from_annual_boundary(context, end)?,
    ))
}

pub(super) async fn lock_annual_employment_contract_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    market_date: Date,
) -> Result<()> {
    if market_date.month() != Month::January || market_date.day() != 1 {
        return Ok(());
    }
    sqlx::query(
        "SELECT contract.id
         FROM employment_contract AS contract
         WHERE contract.save_id = ? AND contract.run_revision = ?
         ORDER BY contract.id
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await
    .context("failed to pre-lock employment contracts for annual closing")?;
    Ok(())
}

pub(super) async fn prepare_employment_annual_tax_boundary(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    context: AnnualTaxRunContext,
) -> Result<()> {
    if context.market_date.month() != Month::January || context.market_date.day() != 1 {
        return Ok(());
    }
    let current_year = u16::try_from(context.market_date.year())
        .context("annual closing current year is out of range")?;
    let Some(tax_year) = current_year.checked_sub(1) else {
        return Ok(());
    };
    let Some(mut income_year) = lock_income_year(tx, context, tax_year).await? else {
        finalize_previous_tax_year(tx, context, tax_year).await?;
        return Ok(());
    };
    income_event_authority(
        income_year.income_event_count,
        income_year.last_income_event_id,
    )?;

    let coordinator_key =
        annual_coordinator_key(context, tax_year, income_year.employment_policy_set_id);
    if verify_coordinator_replay(tx, context, tax_year, &coordinator_key).await? {
        return Ok(());
    }
    ensure!(
        income_year.status == "open",
        "finalized employment income year has no assessment"
    );
    let finalize = sqlx::query(
        "UPDATE employment_income_year SET status = 'finalized', finalized_on = ?
         WHERE save_id = ? AND run_revision = ? AND tax_year = ? AND status = 'open'",
    )
    .bind(context.market_date)
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(tax_year)
    .execute(&mut **tx)
    .await
    .context("failed to finalize the employment income year")?;
    ensure!(
        finalize.rows_affected() == 1,
        "employment income finalization lost its lock"
    );
    income_year.status = "finalized".to_owned();

    let loaded_policy = load_annual_policy(
        tx,
        income_year.employment_policy_set_id,
        context.policy_set_id,
        tax_year,
    )
    .await?;
    let employment_rules = crate::career::create_employment_tax_rules();
    employment_rules
        .validate_policy(&loaded_policy.policy)
        .context("stored annual employment policy violates pure invariants")?;
    let pension_sources = read_pension_sources(tx, context, tax_year).await?;
    let personal_count = read_personal_deduction_count(tx, context).await?;

    let standalone_financial = plan_previous_tax_year_with_employment(
        tx,
        context,
        tax_year,
        0,
        TaxCredits {
            income_tax_credit_krw: 0,
            local_income_tax_credit_krw: 0,
        },
    )
    .await?;
    let requires_combined = standalone_financial
        .as_ref()
        .is_some_and(|draft| draft.status == FinancialIncomeAssessmentStatus::FilingPending);

    let employment_only = employment_rules
        .plan_employment_only(EmploymentOnlyTaxPlanningInput {
            authority: EmploymentIncomeAuthority::M3Payroll,
            tax_year,
            gross_employment_income_krw: income_year.gross_employment_income_krw,
            employee_statutory_insurance: EmployeeStatutoryInsuranceAmounts {
                national_pension_krw: income_year.employee_national_pension_krw,
                health_insurance_krw: income_year.employee_health_insurance_krw,
                long_term_care_krw: income_year.employee_long_term_care_krw,
                employment_insurance_krw: income_year.employee_employment_insurance_krw,
            },
            personal_deduction_person_count: personal_count,
            withheld_income_tax_krw: income_year.withheld_income_tax_krw,
            withheld_local_income_tax_krw: income_year.withheld_local_income_tax_krw,
            requires_combined_assessment: requires_combined,
            pension_opening_tax_excluded_balances: &pension_sources.opening,
            pension_source_events: &pension_sources.events,
            policy: &loaded_policy.policy,
        })
        .context("failed to plan the employment-only annual assessment")?;
    let employment_only_id = insert_assessment(
        tx,
        context,
        income_year.employment_policy_set_id,
        loaded_policy.id,
        &coordinator_key,
        &employment_only,
    )
    .await?;

    let (definitive_id, definitive, financial_draft) = if requires_combined {
        let income_credit = employment_only.calculation.earned_income_tax_credit_krw;
        let local_credit =
            linked_local_credit_for_employment(&employment_only, &loaded_policy.policy)?;
        let mut draft = plan_previous_tax_year_with_employment(
            tx,
            context,
            tax_year,
            employment_only.calculation.taxable_income_krw,
            TaxCredits {
                income_tax_credit_krw: income_credit,
                local_income_tax_credit_krw: local_credit,
            },
        )
        .await?
        .context("combined annual assessment has no financial-income year")?;
        ensure!(
            draft.status == FinancialIncomeAssessmentStatus::FilingPending,
            "combined assessment lost its financial comprehensive target"
        );
        let combined = employment_rules
            .plan_combined(CombinedEmploymentTaxPlanningInput {
                authority: EmploymentIncomeAuthority::M3Payroll,
                handoff: employment_only.combined_handoff,
                comprehensive_income_krw: draft
                    .gross_financial_income_krw
                    .checked_add(employment_only.calculation.taxable_income_krw)
                    .context("combined comprehensive income overflowed")?,
                calculated_combined_income_tax_krw: draft
                    .income_tax_formula_a_krw
                    .max(draft.income_tax_formula_b_krw),
                income_tax_before_pension_credit_krw: draft.final_income_tax_krw,
                local_income_tax_before_pension_effect_krw: draft.final_local_income_tax_krw,
                total_prepaid_income_tax_krw: employment_only
                    .calculation
                    .assessed_income_tax_krw
                    .checked_add(draft.withheld_income_tax_krw)
                    .context("combined prepaid income tax overflowed")?,
                total_prepaid_local_income_tax_krw: employment_only
                    .calculation
                    .assessed_local_income_tax_krw
                    .checked_add(draft.withheld_local_income_tax_krw)
                    .context("combined prepaid local tax overflowed")?,
                pension_opening_tax_excluded_balances: &pension_sources.opening,
                pension_source_events: &pension_sources.events,
                policy: &loaded_policy.policy,
            })
            .context("failed to plan the combined annual assessment")?;
        apply_combined_result_to_financial_draft(&mut draft, &combined)?;
        let combined_id = insert_assessment(
            tx,
            context,
            income_year.employment_policy_set_id,
            loaded_policy.id,
            &coordinator_key,
            &combined,
        )
        .await?;
        (combined_id, combined, Some(draft))
    } else {
        (
            employment_only_id,
            employment_only.clone(),
            standalone_financial,
        )
    };

    if let Some(draft) = financial_draft.as_ref() {
        if requires_combined {
            persist_assessment_draft_with_employment(
                tx,
                context,
                draft,
                AnnualEmploymentAssessmentLink {
                    year_end_tax_assessment_id: definitive_id,
                    employment_taxable_income_krw: employment_only.calculation.taxable_income_krw,
                    employment_deductions_krw: employment_only
                        .calculation
                        .earned_income_deduction_krw
                        .checked_add(employment_only.calculation.personal_deduction_krw)
                        .and_then(|value| {
                            value.checked_add(
                                employment_only.calculation.employee_insurance_deduction_krw,
                            )
                        })
                        .context("employment deductions overflowed")?,
                    employment_final_prepaid_income_tax_krw: employment_only
                        .calculation
                        .assessed_income_tax_krw,
                    employment_final_prepaid_local_income_tax_krw: employment_only
                        .calculation
                        .assessed_local_income_tax_krw,
                },
            )
            .await?;
        } else {
            super::annual_tax::persist_assessment_draft(tx, context, draft).await?;
        }
        schedule_annual_tax_filing_if_needed(tx, context, draft).await?;
    }
    persist_pension_allocations(tx, finance_rules, context, definitive_id, &definitive).await?;
    schedule_reconciliation(
        tx,
        context,
        tax_year,
        employment_only_id,
        &income_year,
        &employment_only,
        loaded_policy.february_reconciliation_day_of_month,
    )
    .await?;
    Ok(())
}

async fn lock_income_year(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
) -> Result<Option<IncomeYearClosingRow>> {
    sqlx::query_as(
        "SELECT employment_policy_set_id, status, income_event_count, last_income_event_id,
                gross_employment_income_krw,
                employee_national_pension_krw, employee_health_insurance_krw,
                employee_long_term_care_krw, employee_employment_insurance_krw,
                withheld_income_tax_krw, withheld_local_income_tax_krw
         FROM employment_income_year
         WHERE save_id = ? AND run_revision = ? AND tax_year = ? FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(tax_year)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock the employment income year")
}

fn annual_coordinator_key(
    context: AnnualTaxRunContext,
    tax_year: u16,
    employment_policy_set_id: u64,
) -> String {
    let canonical = format!(
        "lifeledger.year-end.v1:{}:{}:{}:{}:{}",
        context.save_id,
        context.run_revision,
        tax_year,
        context.policy_set_id,
        employment_policy_set_id
    );
    format!("{:x}", Sha256::digest(canonical.as_bytes()))
}

async fn verify_coordinator_replay(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
    coordinator_key: &str,
) -> Result<bool> {
    #[derive(sqlx::FromRow)]
    struct ReplayAssessmentRow {
        id: u64,
        employment_annual_tax_policy_id: u64,
        assessment_kind: String,
        assessment_status: String,
        coordinator_key: String,
        gross_employment_income_krw: i64,
        insurance_income_deduction_krw: i64,
        actual_pension_income_tax_credit_krw: i64,
        actual_pension_local_tax_effect_krw: i64,
        final_income_tax_krw: i64,
        final_local_income_tax_krw: i64,
        prepaid_income_tax_krw: i64,
        prepaid_local_income_tax_krw: i64,
        additional_tax_krw: i64,
        refund_krw: i64,
    }
    let rows: Vec<ReplayAssessmentRow> = sqlx::query_as(
        "SELECT id, employment_annual_tax_policy_id, assessment_kind,
                assessment_status, coordinator_key,
                gross_employment_income_krw, insurance_income_deduction_krw,
                actual_pension_income_tax_credit_krw,
                actual_pension_local_tax_effect_krw,
                final_income_tax_krw, final_local_income_tax_krw,
                prepaid_income_tax_krw, prepaid_local_income_tax_krw,
                additional_tax_krw, refund_krw
         FROM year_end_tax_assessment
         WHERE save_id = ? AND run_revision = ? AND tax_year = ?
         ORDER BY assessment_kind FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(tax_year)
    .fetch_all(&mut **tx)
    .await?;
    if rows.is_empty() {
        return Ok(false);
    }
    ensure!(
        rows.iter()
            .all(|row| row.coordinator_key == coordinator_key),
        "year-end coordinator identity conflicts with persisted assessments"
    );
    let employment_only = rows
        .iter()
        .find(|row| row.assessment_kind == "employmentOnly")
        .context("year-end coordinator replay has no employment-only assessment")?;
    let definitive = rows
        .iter()
        .find(|row| row.assessment_status == "definitive")
        .context("year-end coordinator replay is incomplete")?;
    let income_year: IncomeYearClosingRow = sqlx::query_as(
        "SELECT employment_policy_set_id, status, income_event_count, last_income_event_id,
                gross_employment_income_krw,
                employee_national_pension_krw, employee_health_insurance_krw,
                employee_long_term_care_krw, employee_employment_insurance_krw,
                withheld_income_tax_krw, withheld_local_income_tax_krw
         FROM employment_income_year
         WHERE save_id = ? AND run_revision = ? AND tax_year = ? FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(tax_year)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        income_year.status == "finalized",
        "replayed employment income year is not finalized"
    );
    let has_income_event_authority = income_event_authority(
        income_year.income_event_count,
        income_year.last_income_event_id,
    )?;
    let allocation_sums: (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(income_tax_credit_krw), 0),
                COALESCE(SUM(local_income_tax_effect_krw), 0)
         FROM pension_credit_allocation
         WHERE save_id = ? AND run_revision = ? AND tax_year = ?
           AND year_end_tax_assessment_id = ?",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(tax_year)
    .bind(definitive.id)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        allocation_sums.0 == definitive.actual_pension_income_tax_credit_krw
            && allocation_sums.1 == definitive.actual_pension_local_tax_effect_krw,
        "replayed pension allocations do not reconcile"
    );
    if definitive.assessment_kind == "combined" {
        let linked_id: Option<u64> = sqlx::query_scalar(
            "SELECT year_end_tax_assessment_id FROM financial_income_assessment
             WHERE save_id = ? AND run_revision = ? AND tax_year = ?",
        )
        .bind(context.save_id)
        .bind(context.run_revision)
        .bind(tax_year)
        .fetch_optional(&mut **tx)
        .await?
        .flatten();
        ensure!(
            linked_id == Some(definitive.id),
            "combined replay lost its M2 assessment link"
        );
    }
    let reconciliation: Option<(u64, u32)> = sqlx::query_as(
        "SELECT id, due_game_day FROM scheduled_settlement
         WHERE save_id = ? AND run_revision = ?
           AND BINARY source_kind = BINARY 'yearEndTaxAssessment'
           AND BINARY source_id = BINARY ? AND occurrence = 1",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(employment_only.id.to_string())
    .fetch_optional(&mut **tx)
    .await?;
    let Some((reconciliation_id, due_game_day)) = reconciliation else {
        ensure!(
            !has_income_event_authority
                && income_year_allows_omitted_reconciliation(&income_year)
                && employment_only.gross_employment_income_krw == 0
                && employment_only.insurance_income_deduction_krw == 0
                && employment_only.actual_pension_income_tax_credit_krw == 0
                && employment_only.actual_pension_local_tax_effect_krw == 0
                && employment_only.final_income_tax_krw == 0
                && employment_only.final_local_income_tax_krw == 0
                && employment_only.prepaid_income_tax_krw == 0
                && employment_only.prepaid_local_income_tax_krw == 0
                && employment_only.additional_tax_krw == 0
                && employment_only.refund_krw == 0,
            "non-zero replayed assessment has no reconciliation schedule"
        );
        return Ok(true);
    };
    ensure!(
        has_income_event_authority,
        "reconciliation replay has no income-event authority"
    );
    let private_payroll = lock_pending_february_anchor(
        tx,
        context,
        tax_year,
        EMPLOYMENT_PAYROLL_KIND,
        EMPLOYMENT_CONTRACT_SOURCE_KIND,
    )
    .await?
    .map(
        |(settlement_id, due_game_day)| ReconciliationAnchor::EmploymentPayroll {
            settlement_id,
            due_game_day,
        },
    );
    let military_pay = if private_payroll.is_none() {
        lock_pending_february_anchor(
            tx,
            context,
            tax_year,
            MILITARY_PAY_KIND,
            MILITARY_SERVICE_SOURCE_KIND,
        )
        .await?
        .map(
            |(settlement_id, due_game_day)| ReconciliationAnchor::MilitaryPay {
                settlement_id,
                due_game_day,
            },
        )
    } else {
        None
    };
    let reconciliation_day =
        load_pinned_reconciliation_day(tx, employment_only.employment_annual_tax_policy_id).await?;
    let policy_game_day = policy_reconciliation_game_day(context, tax_year, reconciliation_day)?;
    let expected_anchor =
        choose_reconciliation_anchor(private_payroll, military_pay, policy_game_day);
    ensure!(
        expected_anchor.due_game_day() == due_game_day,
        "reconciliation replay uses a non-canonical February anchor"
    );
    if let Some(anchor_settlement_id) = expected_anchor.settlement_id() {
        ensure!(
            anchor_settlement_id < reconciliation_id,
            "reconciliation replay lost its preceding February pay settlement"
        );
    }
    Ok(true)
}

async fn load_annual_policy(
    tx: &mut Transaction<'_, MySql>,
    employment_policy_set_id: u64,
    finance_policy_set_id: u64,
    tax_year: u16,
) -> Result<LoadedAnnualPolicy> {
    let compatible: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM employment_finance_compatibility
             WHERE employment_policy_set_id = ? AND policy_set_id = ?
         )",
    )
    .bind(employment_policy_set_id)
    .bind(finance_policy_set_id)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        compatible,
        "employment and finance policy sets are incompatible"
    );
    let row: AnnualPolicyRow = sqlx::query_as(
        "SELECT annual_policy.id, annual_policy.tax_year AS policy_tax_year,
                policy_set.ranked_eligible,
                annual_policy.february_reconciliation_day_of_month,
                annual_policy.basic_personal_deduction_krw,
                annual_policy.taxable_income_rounding_unit_krw,
                annual_policy.calculated_tax_rounding_unit_krw,
                annual_policy.income_tax_credit_low_tax_boundary_krw,
                CAST(annual_policy.income_tax_credit_low_rate_ppm AS SIGNED)
                    AS income_tax_credit_low_rate_ppm,
                annual_policy.income_tax_credit_high_base_krw,
                CAST(annual_policy.income_tax_credit_high_rate_ppm AS SIGNED)
                    AS income_tax_credit_high_rate_ppm,
                annual_policy.credit_cap_salary_boundary_one_krw,
                annual_policy.credit_cap_salary_boundary_two_krw,
                annual_policy.credit_cap_one_krw,
                annual_policy.credit_cap_two_base_krw,
                CAST(annual_policy.credit_cap_two_reduction_rate_ppm AS SIGNED)
                    AS credit_cap_two_reduction_rate_ppm,
                annual_policy.credit_cap_two_floor_krw,
                annual_policy.credit_cap_three_base_krw,
                CAST(annual_policy.credit_cap_three_reduction_rate_ppm AS SIGNED)
                    AS credit_cap_three_reduction_rate_ppm,
                annual_policy.credit_cap_three_floor_krw
         FROM employment_annual_tax_policy AS annual_policy
         INNER JOIN employment_policy_set AS policy_set
             ON policy_set.id = annual_policy.employment_policy_set_id
            AND policy_set.published_at IS NOT NULL
         WHERE annual_policy.employment_policy_set_id = ?
           AND annual_policy.tax_year <= ?
         ORDER BY annual_policy.tax_year DESC
         LIMIT 1 FOR SHARE",
    )
    .bind(employment_policy_set_id)
    .bind(tax_year)
    .fetch_optional(&mut **tx)
    .await?
    .context("pinned employment policy has no annual tax policy for the year")?;
    ensure!(
        !row.ranked_eligible || row.policy_tax_year == tax_year,
        "ranked employment policy has no exact annual tax policy for the year"
    );
    ensure!(
        (1..=31).contains(&row.february_reconciliation_day_of_month),
        "employment annual policy has an invalid February reconciliation day"
    );
    let deduction_rows: Vec<EarnedDeductionRow> = sqlx::query_as(
        "SELECT lower_bound_krw, upper_bound_exclusive_krw,
                base_deduction_krw,
                CAST(marginal_rate_ppm AS SIGNED) AS marginal_rate_ppm
         FROM employment_income_deduction_bracket
         WHERE employment_policy_set_id = ? AND employment_annual_tax_policy_id = ?
         ORDER BY bracket_order FOR SHARE",
    )
    .bind(employment_policy_set_id)
    .bind(row.id)
    .fetch_all(&mut **tx)
    .await?;
    let tax_rows: Vec<BasicTaxBracketRow> = sqlx::query_as(
        "SELECT lower_bound_krw, upper_bound_exclusive_krw,
                CAST(rate_ppm AS SIGNED) AS rate_ppm, quick_deduction_krw
         FROM employment_income_tax_bracket
         WHERE employment_policy_set_id = ? AND employment_annual_tax_policy_id = ?
         ORDER BY bracket_order FOR SHARE",
    )
    .bind(employment_policy_set_id)
    .bind(row.id)
    .fetch_all(&mut **tx)
    .await?;
    let pension_json_rows: Vec<(String,)> = sqlx::query_as(
        "SELECT CAST(parameters AS CHAR)
         FROM policy_rule
         WHERE policy_set_id = ? AND BINARY domain = BINARY ?
           AND BINARY rule_key = BINARY ?
           AND effective_from <= ? AND (effective_to IS NULL OR effective_to >= ?)
         ORDER BY effective_from DESC LIMIT 2",
    )
    .bind(finance_policy_set_id)
    .bind(PENSION_POLICY_DOMAIN)
    .bind(PENSION_POLICY_RULE)
    .bind(Date::from_calendar_date(
        i32::from(tax_year),
        Month::December,
        31,
    )?)
    .bind(Date::from_calendar_date(
        i32::from(tax_year),
        Month::December,
        31,
    )?)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        pension_json_rows.len() == 1,
        "pinned pension credit policy is missing or overlaps"
    );
    let pension_policy: PensionPolicy = serde_json::from_str(&pension_json_rows[0].0)
        .context("stored pension policy is not its strict schema")?;
    ensure!(
        pension_policy.high_local_income_tax_credit_rate_ppm
            == pension_policy.high_income_tax_credit_rate_ppm / 10
            && pension_policy.standard_local_income_tax_credit_rate_ppm
                == pension_policy.standard_income_tax_credit_rate_ppm / 10,
        "pension and annual local-income credit rates are incompatible"
    );
    let policy = EmploymentAnnualTaxPolicy {
        tax_year,
        earned_income_deduction_brackets: deduction_rows
            .into_iter()
            .map(|row| EarnedIncomeDeductionBracket {
                lower_bound_krw: row.lower_bound_krw,
                upper_bound_exclusive_krw: row.upper_bound_exclusive_krw,
                base_deduction_krw: row.base_deduction_krw,
                marginal_rate_ppm: row.marginal_rate_ppm,
            })
            .collect(),
        basic_personal_deduction_per_person_krw: row.basic_personal_deduction_krw,
        taxable_income_rounding_unit_krw: row.taxable_income_rounding_unit_krw,
        basic_tax_brackets: tax_rows
            .into_iter()
            .map(|row| ProgressiveEmploymentTaxBracket {
                lower_bound_krw: row.lower_bound_krw,
                upper_bound_exclusive_krw: row.upper_bound_exclusive_krw,
                rate_ppm: row.rate_ppm,
                quick_deduction_krw: row.quick_deduction_krw,
            })
            .collect(),
        calculated_tax_rounding_unit_krw: row.calculated_tax_rounding_unit_krw,
        earned_income_tax_credit: EarnedIncomeTaxCreditPolicy {
            low_tax_boundary_krw: row.income_tax_credit_low_tax_boundary_krw,
            low_tax_rate_ppm: row.income_tax_credit_low_rate_ppm,
            high_tax_base_credit_krw: row.income_tax_credit_high_base_krw,
            high_tax_marginal_rate_ppm: row.income_tax_credit_high_rate_ppm,
            salary_boundary_one_krw: row.credit_cap_salary_boundary_one_krw,
            salary_boundary_two_krw: row.credit_cap_salary_boundary_two_krw,
            cap_one_krw: row.credit_cap_one_krw,
            cap_two_base_krw: row.credit_cap_two_base_krw,
            cap_two_reduction_rate_ppm: row.credit_cap_two_reduction_rate_ppm,
            cap_two_floor_krw: row.credit_cap_two_floor_krw,
            cap_three_base_krw: row.credit_cap_three_base_krw,
            cap_three_reduction_rate_ppm: row.credit_cap_three_reduction_rate_ppm,
            cap_three_floor_krw: row.credit_cap_three_floor_krw,
        },
        pension_credit: PensionContributionCreditPolicy {
            pension_savings_limit_krw: pension_policy.pension_savings_credit_limit_krw,
            pension_savings_and_irp_limit_krw: pension_policy.combined_credit_limit_krw,
            salary_rate_boundary_krw: pension_policy.salary_high_credit_boundary_krw,
            comprehensive_income_rate_boundary_krw: pension_policy
                .comprehensive_income_high_credit_boundary_krw,
            lower_income_rate: PensionContributionCreditRate {
                income_tax_rate_ppm: pension_policy.high_income_tax_credit_rate_ppm,
                income_tax_rounding_unit_krw: 1,
            },
            higher_income_rate: PensionContributionCreditRate {
                income_tax_rate_ppm: pension_policy.standard_income_tax_credit_rate_ppm,
                income_tax_rounding_unit_krw: 1,
            },
        },
        local_income_tax: AnnualLocalIncomeTaxPolicy {
            linked_income_tax_rate_ppm: 100_000,
            rounding_unit_krw: 1,
        },
    };
    Ok(LoadedAnnualPolicy {
        id: row.id,
        february_reconciliation_day_of_month: row.february_reconciliation_day_of_month,
        policy,
    })
}

async fn read_personal_deduction_count(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
) -> Result<u8> {
    let dependents =
        read_tax_dependent_count_in_tx(tx, context.save_id, context.run_revision, context.game_day)
            .await?;
    let count = dependents
        .checked_add(1)
        .context("personal deduction count overflowed")?;
    ensure!(
        (1..=7).contains(&count),
        "personal deduction count exceeds policy support"
    );
    Ok(count)
}

fn linked_local_credit_for_employment(
    plan: &EmploymentTaxAssessmentPlan,
    policy: &EmploymentAnnualTaxPolicy,
) -> Result<i64> {
    let calculated_local = linked_local_tax(
        plan.calculation.calculated_income_tax_krw,
        policy.local_income_tax,
    )?;
    let after_earned_before_pension = plan
        .calculation
        .assessed_local_income_tax_krw
        .checked_add(plan.calculation.actual_pension_local_income_tax_effect_krw)
        .context("employment local-income tax credit overflowed")?;
    calculated_local
        .checked_sub(after_earned_before_pension)
        .context("employment local-income credit underflowed")
}

fn linked_local_tax(income_tax_krw: i64, policy: AnnualLocalIncomeTaxPolicy) -> Result<i64> {
    ensure!(income_tax_krw >= 0, "linked local tax input is negative");
    let raw = i128::from(income_tax_krw)
        .checked_mul(i128::from(policy.linked_income_tax_rate_ppm))
        .and_then(|value| value.checked_div(1_000_000))
        .context("linked local tax overflowed")?;
    let unit = i128::from(policy.rounding_unit_krw);
    i64::try_from(raw / unit * unit).context("linked local tax is out of range")
}

fn apply_combined_result_to_financial_draft(
    draft: &mut AnnualAssessmentDraft,
    combined: &EmploymentTaxAssessmentPlan,
) -> Result<()> {
    draft.income_tax_credit_krw = draft
        .income_tax_credit_krw
        .checked_add(combined.calculation.actual_pension_income_tax_credit_krw)
        .context("combined income-tax credit overflowed")?;
    draft.local_income_tax_credit_krw = draft
        .local_income_tax_credit_krw
        .checked_add(
            combined
                .calculation
                .actual_pension_local_income_tax_effect_krw,
        )
        .context("combined local-income credit overflowed")?;
    draft.final_income_tax_krw = combined.calculation.assessed_income_tax_krw;
    draft.final_local_income_tax_krw = combined.calculation.assessed_local_income_tax_krw;
    draft.additional_tax_krw = combined.reconciliation.additional_tax_krw;
    draft.refund_krw = combined.reconciliation.refund_krw;
    Ok(())
}

async fn read_pension_sources(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
) -> Result<PensionSources> {
    let balances: Vec<PensionBalanceRow> = sqlx::query_as(
        "SELECT balance.financial_account_id, balance.tax_excluded_contribution_krw
         FROM pension_tax_balance AS balance
         INNER JOIN financial_account AS account
           ON account.save_id = balance.save_id
          AND account.run_revision = balance.run_revision
          AND account.id = balance.financial_account_id
         WHERE balance.save_id = ? AND balance.run_revision = ?
           AND account.account_type IN ('pensionSavings', 'irp')
         ORDER BY balance.financial_account_id FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .fetch_all(&mut **tx)
    .await?;
    let rows: Vec<PensionEventRow> = sqlx::query_as(
        "SELECT event.id, event.financial_account_id, account.account_type,
                event.event_kind, event.game_day, event.movement_amount_krw,
                CAST(event.payload AS CHAR) AS payload_json,
                event.ledger_transaction_id
         FROM tax_account_event AS event
         INNER JOIN financial_account AS account
           ON account.save_id = event.save_id
          AND account.run_revision = event.run_revision
          AND account.id = event.financial_account_id
         WHERE event.save_id = ? AND event.run_revision = ? AND event.tax_year = ?
           AND event.event_kind IN ('pensionContribution', 'pensionWithdrawal')
         ORDER BY event.game_day, event.ledger_transaction_id,
                  event.financial_account_id, event.id
         FOR SHARE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(tax_year)
    .fetch_all(&mut **tx)
    .await?;
    let mut contribution_totals = HashMap::<u64, i64>::new();
    let mut withdrawal_totals = HashMap::<u64, i64>::new();
    let mut events = Vec::with_capacity(rows.len());
    let mut source_ids = HashSet::new();
    let mut ledger_ids = HashSet::new();
    for row in rows {
        ensure!(
            source_ids.insert(row.id),
            "duplicate pension source event ID"
        );
        let source = parse_pension_event(&row, tax_year)?;
        match source {
            PensionContributionSourceEvent::Contribution(event) => {
                ensure!(
                    ledger_ids.insert(event.ledger_transaction_id),
                    "duplicate pension contribution ledger identity"
                );
                add_amount(&mut contribution_totals, event.account_id, event.amount_krw)?;
            }
            PensionContributionSourceEvent::Withdrawal(event) => {
                ensure!(
                    ledger_ids.insert(event.ledger_transaction_id),
                    "duplicate pension withdrawal ledger identity"
                );
                add_amount(
                    &mut withdrawal_totals,
                    event.account_id,
                    event.tax_excluded_withdrawn_krw,
                )?;
            }
        }
        events.push(source);
    }
    let mut opening = Vec::with_capacity(balances.len());
    let mut known_accounts = HashSet::new();
    for balance in balances {
        known_accounts.insert(balance.financial_account_id);
        let amount_krw = balance
            .tax_excluded_contribution_krw
            .checked_sub(
                contribution_totals
                    .get(&balance.financial_account_id)
                    .copied()
                    .unwrap_or(0),
            )
            .and_then(|value| {
                value.checked_add(
                    withdrawal_totals
                        .get(&balance.financial_account_id)
                        .copied()
                        .unwrap_or(0),
                )
            })
            .context("pension opening tax-excluded balance overflowed")?;
        ensure!(
            amount_krw >= 0,
            "pension event history exceeds its opening layer"
        );
        opening.push(PensionOpeningTaxExcludedBalance {
            account_id: balance.financial_account_id,
            amount_krw,
        });
    }
    ensure!(
        contribution_totals
            .keys()
            .chain(withdrawal_totals.keys())
            .all(|account_id| known_accounts.contains(account_id)),
        "pension event has no tax-layer balance"
    );
    Ok(PensionSources { opening, events })
}

fn add_amount(amounts: &mut HashMap<u64, i64>, account_id: u64, amount_krw: i64) -> Result<()> {
    ensure!(amount_krw >= 0, "pension source movement is negative");
    let next = amounts
        .get(&account_id)
        .copied()
        .unwrap_or(0)
        .checked_add(amount_krw)
        .context("pension source total overflowed")?;
    amounts.insert(account_id, next);
    Ok(())
}

fn parse_pension_event(
    row: &PensionEventRow,
    tax_year: u16,
) -> Result<PensionContributionSourceEvent> {
    let ledger_transaction_id = row
        .ledger_transaction_id
        .context("pension source event has no ledger transaction")?;
    let account_kind = match row.account_type.as_str() {
        "pensionSavings" => PensionContributionAccountKind::PensionSavings,
        "irp" => PensionContributionAccountKind::Irp,
        _ => bail!("pension source event has an invalid account type"),
    };
    match row.event_kind.as_str() {
        "pensionContribution" => {
            let payload: ContributionPayload = serde_json::from_str(&row.payload_json)
                .context("stored pension contribution payload is not the strict schema")?;
            validate_contribution_payload(&payload, row)?;
            Ok(PensionContributionSourceEvent::Contribution(
                PensionContributionEvent {
                    contribution_source_id: row.id,
                    account_id: row.financial_account_id,
                    account_kind,
                    tax_year,
                    contribution_game_day: row.game_day,
                    ledger_transaction_id,
                    amount_krw: row.movement_amount_krw,
                },
            ))
        }
        "pensionWithdrawal" => {
            let payload: WithdrawalPayload = serde_json::from_str(&row.payload_json)
                .context("stored pension withdrawal payload is not the strict schema")?;
            let tax_excluded_withdrawn_krw = validate_withdrawal_payload(&payload, row)?;
            Ok(PensionContributionSourceEvent::Withdrawal(
                PensionWithdrawalEvent {
                    account_id: row.financial_account_id,
                    tax_year,
                    withdrawal_game_day: row.game_day,
                    ledger_transaction_id,
                    tax_excluded_withdrawn_krw,
                },
            ))
        }
        _ => bail!("pension source query returned an unexpected event kind"),
    }
}

fn validate_contribution_payload(
    payload: &ContributionPayload,
    row: &PensionEventRow,
) -> Result<()> {
    ensure!(
        payload.version == 1 && payload.amount_krw == row.movement_amount_krw,
        "pension contribution payload disagrees with its envelope"
    );
    validate_layers(&payload.tax_layers_after)?;
    let mut previous_account_id = 0_u64;
    let mut target_present = false;
    for allocation in &payload.allocations {
        let account_id = allocation
            .account_id
            .parse::<u64>()
            .context("pension contribution allocation account ID is invalid")?;
        ensure!(
            account_id > previous_account_id
                && allocation.total_contribution_krw >= 0
                && allocation.credit_eligible_krw >= 0
                && allocation.expected_credit_rate_ppm >= 0
                && allocation.expected_credit_krw >= 0,
            "pension contribution allocation is not canonical"
        );
        previous_account_id = account_id;
        target_present |= account_id == row.financial_account_id;
    }
    ensure!(
        target_present,
        "pension contribution payload lost its target account"
    );
    Ok(())
}

fn validate_withdrawal_payload(payload: &WithdrawalPayload, row: &PensionEventRow) -> Result<i64> {
    ensure!(
        payload.version == 1
            && payload.gross_amount_krw == row.movement_amount_krw
            && !payload.request_kind.is_empty()
            && payload.pension_amount_krw >= 0
            && payload.non_pension_amount_krw >= 0
            && payload.tax_free_amount_krw >= 0
            && payload.tax_krw >= 0
            && payload.net_payout_krw >= 0
            && payload.net_payout_krw.checked_add(payload.tax_krw)
                == Some(payload.gross_amount_krw),
        "pension withdrawal payload disagrees with its envelope"
    );
    if payload.request_kind != "irpEarly" {
        ensure!(
            payload.reason.is_none(),
            "pension withdrawal reason is unexpected"
        );
    }
    validate_layers(&payload.remaining_layers)?;
    let mut gross_total = 0_i64;
    let mut tax_excluded_total = 0_i64;
    for portion in &payload.portions {
        ensure!(
            !portion.treatment.is_empty()
                && portion.gross_amount_krw >= 0
                && portion.tax_free_amount_krw >= 0
                && portion.tax_krw >= 0
                && portion.net_amount_krw >= 0
                && portion.net_amount_krw.checked_add(portion.tax_krw)
                    == Some(portion.gross_amount_krw),
            "pension withdrawal portion is invalid"
        );
        gross_total = gross_total
            .checked_add(portion.gross_amount_krw)
            .context("pension withdrawal portions overflowed")?;
        for line in &portion.tax_lines {
            ensure!(
                line.gross_amount_krw >= 0
                    && line.tax_krw >= 0
                    && line.net_amount_krw >= 0
                    && line.tax_rate.is_object()
                    && line.net_amount_krw.checked_add(line.tax_krw) == Some(line.gross_amount_krw),
                "pension withdrawal tax line is invalid"
            );
            if line.source == "taxExcludedContribution" {
                tax_excluded_total = tax_excluded_total
                    .checked_add(line.gross_amount_krw)
                    .context("pension tax-excluded withdrawal overflowed")?;
            }
        }
    }
    ensure!(
        gross_total == payload.gross_amount_krw,
        "pension withdrawal portions do not sum"
    );
    Ok(tax_excluded_total)
}

fn validate_layers(layers: &PensionLayersPayload) -> Result<()> {
    ensure!(
        layers.tax_excluded_contribution_krw >= 0
            && layers.deferred_retirement_income_krw >= 0
            && layers.credited_contribution_krw >= 0
            && layers.earnings_krw >= 0,
        "pension payload contains a negative tax layer"
    );
    Ok(())
}

async fn insert_assessment(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    employment_policy_set_id: u64,
    annual_policy_id: u64,
    coordinator_key: &str,
    plan: &EmploymentTaxAssessmentPlan,
) -> Result<u64> {
    let calculation = plan.calculation;
    let kind = match calculation.source {
        EmploymentTaxAssessmentSource::EmploymentOnly => "employmentOnly",
        EmploymentTaxAssessmentSource::Combined => "combined",
        EmploymentTaxAssessmentSource::LegacyProfile => {
            bail!("legacy tax profiles must not be persisted as M3 assessments")
        }
    };
    let status = match calculation.status {
        EmploymentTaxAssessmentStatus::Provisional => "provisional",
        EmploymentTaxAssessmentStatus::Definitive => "definitive",
    };
    let adjusted_income = calculation
        .gross_employment_income_krw
        .checked_sub(calculation.earned_income_deduction_krw)
        .context("adjusted employment income underflowed")?
        .max(0);
    let actual_pension_credit = calculation
        .actual_pension_income_tax_credit_krw
        .checked_add(calculation.actual_pension_local_income_tax_effect_krw)
        .context("actual pension credit overflowed")?;
    let income_adjustment = calculation
        .assessed_income_tax_krw
        .checked_sub(plan.reconciliation.prepaid_income_tax_krw)
        .context("employment income-tax adjustment overflowed")?;
    let local_adjustment = calculation
        .assessed_local_income_tax_krw
        .checked_sub(plan.reconciliation.prepaid_local_income_tax_krw)
        .context("employment local-tax adjustment overflowed")?;
    let result = sqlx::query(
        "INSERT INTO year_end_tax_assessment
             (save_id, run_revision, tax_year, employment_policy_set_id,
              employment_annual_tax_policy_id, assessment_kind, assessment_status,
              coordinator_key, uses_financial_income_assessment,
              gross_employment_income_krw, employment_income_deduction_krw,
              adjusted_employment_income_krw, basic_personal_deduction_krw,
              insurance_income_deduction_krw, taxable_employment_income_krw,
              calculated_income_tax_krw, employment_income_tax_credit_krw,
              other_nonrefundable_tax_credit_krw, pension_credit_eligible_krw,
              actual_pension_income_tax_credit_krw,
              actual_pension_local_tax_effect_krw, actual_pension_credit_krw,
              final_income_tax_krw, final_local_income_tax_krw,
              prepaid_income_tax_krw, prepaid_local_income_tax_krw,
              income_tax_adjustment_krw, local_income_tax_adjustment_krw,
              additional_tax_krw, refund_krw, assessed_on)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?, ?,
                 ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(calculation.tax_year)
    .bind(employment_policy_set_id)
    .bind(annual_policy_id)
    .bind(kind)
    .bind(status)
    .bind(coordinator_key)
    .bind(kind == "combined")
    .bind(calculation.gross_employment_income_krw)
    .bind(calculation.earned_income_deduction_krw)
    .bind(adjusted_income)
    .bind(calculation.personal_deduction_krw)
    .bind(calculation.employee_insurance_deduction_krw)
    .bind(calculation.taxable_income_krw)
    .bind(calculation.calculated_income_tax_krw)
    .bind(calculation.earned_income_tax_credit_krw)
    .bind(calculation.pension_credit_eligible_contribution_krw)
    .bind(calculation.actual_pension_income_tax_credit_krw)
    .bind(calculation.actual_pension_local_income_tax_effect_krw)
    .bind(actual_pension_credit)
    .bind(calculation.assessed_income_tax_krw)
    .bind(calculation.assessed_local_income_tax_krw)
    .bind(plan.reconciliation.prepaid_income_tax_krw)
    .bind(plan.reconciliation.prepaid_local_income_tax_krw)
    .bind(income_adjustment)
    .bind(local_adjustment)
    .bind(plan.reconciliation.additional_tax_krw)
    .bind(plan.reconciliation.refund_krw)
    .bind(context.market_date)
    .execute(&mut **tx)
    .await
    .context("failed to persist the immutable employment tax assessment")?;
    let id = result.last_insert_id();
    ensure!(id != 0, "employment tax assessment insert returned no ID");
    Ok(id)
}

async fn lock_pending_february_anchor(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
    kind: &str,
    source_kind: &str,
) -> Result<Option<(u64, u32)>> {
    let (february_start_game_day, march_start_game_day) =
        february_game_day_range(context, tax_year)?;
    sqlx::query_as(
        "SELECT id, due_game_day
         FROM scheduled_settlement
         WHERE save_id = ? AND run_revision = ?
           AND BINARY kind = BINARY ? AND BINARY source_kind = BINARY ?
           AND status = 'pending'
           AND due_game_day >= ? AND due_game_day < ?
         ORDER BY due_game_day, id
         LIMIT 1 FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(kind)
    .bind(source_kind)
    .bind(february_start_game_day)
    .bind(march_start_game_day)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock a February reconciliation pay anchor")
}

async fn load_pinned_reconciliation_day(
    tx: &mut Transaction<'_, MySql>,
    annual_policy_id: u64,
) -> Result<u8> {
    let day: u8 = sqlx::query_scalar(
        "SELECT february_reconciliation_day_of_month
         FROM employment_annual_tax_policy
         WHERE id = ? FOR SHARE",
    )
    .bind(annual_policy_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("employment assessment lost its pinned annual policy")?;
    ensure!(
        (1..=31).contains(&day),
        "employment annual policy has an invalid February reconciliation day"
    );
    Ok(day)
}

async fn ensure_reconciliation_anchor(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
    policy_day_of_month: u8,
) -> Result<ReconciliationAnchor> {
    let existing_private = lock_pending_february_anchor(
        tx,
        context,
        tax_year,
        EMPLOYMENT_PAYROLL_KIND,
        EMPLOYMENT_CONTRACT_SOURCE_KIND,
    )
    .await?
    .map(
        |(settlement_id, due_game_day)| ReconciliationAnchor::EmploymentPayroll {
            settlement_id,
            due_game_day,
        },
    );
    if let Some(anchor) = existing_private {
        return Ok(anchor);
    }

    let continuing_contract_id: Option<u64> = sqlx::query_scalar(
        "SELECT id
         FROM employment_contract
         WHERE save_id = ? AND run_revision = ? AND status IN ('pendingStart', 'active')
         ORDER BY id
         LIMIT 1 FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(contract_id) = continuing_contract_id {
        let ensured_due_game_day =
            super::employment::ensure_february_payroll_before_reconciliation_in_tx(
                tx,
                crate::career::create_payroll_rules().as_ref(),
                context.save_id,
                context.run_revision,
                contract_id,
                i32::from(tax_year) + 1,
            )
            .await?;
        let (settlement_id, due_game_day) = lock_pending_february_anchor(
            tx,
            context,
            tax_year,
            EMPLOYMENT_PAYROLL_KIND,
            EMPLOYMENT_CONTRACT_SOURCE_KIND,
        )
        .await?
        .context("ensured February employment payroll anchor is missing")?;
        ensure!(
            due_game_day == ensured_due_game_day,
            "ensured February employment payroll anchor changed"
        );
        return Ok(ReconciliationAnchor::EmploymentPayroll {
            settlement_id,
            due_game_day,
        });
    }

    let military_pay = lock_pending_february_anchor(
        tx,
        context,
        tax_year,
        MILITARY_PAY_KIND,
        MILITARY_SERVICE_SOURCE_KIND,
    )
    .await?
    .map(
        |(settlement_id, due_game_day)| ReconciliationAnchor::MilitaryPay {
            settlement_id,
            due_game_day,
        },
    );
    let policy_game_day = policy_reconciliation_game_day(context, tax_year, policy_day_of_month)?;
    Ok(choose_reconciliation_anchor(
        None,
        military_pay,
        policy_game_day,
    ))
}

async fn schedule_reconciliation(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
    assessment_id: u64,
    income_year: &IncomeYearClosingRow,
    assessment: &EmploymentTaxAssessmentPlan,
    policy_day_of_month: u8,
) -> Result<()> {
    let has_income_event_authority = income_event_authority(
        income_year.income_event_count,
        income_year.last_income_event_id,
    )?;
    if !has_income_event_authority {
        ensure!(
            income_year_allows_omitted_reconciliation(income_year)
                && assessment_plan_allows_omitted_reconciliation(assessment),
            "employment annual assessment has non-zero values without income-event authority"
        );
        return Ok(());
    }
    let anchor = ensure_reconciliation_anchor(tx, context, tax_year, policy_day_of_month).await?;
    let due_game_day = anchor.due_game_day();
    let payload = serde_json::to_string(&ReconciliationPayload {
        version: RECONCILIATION_PAYLOAD_VERSION,
        tax_year,
        assessment_id: ResourceId::from_u64(assessment_id),
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
    .bind(RECONCILIATION_KIND)
    .bind(payload)
    .bind(RECONCILIATION_SOURCE_KIND)
    .bind(assessment_id.to_string())
    .bind(RECONCILIATION_OCCURRENCE)
    .execute(&mut **tx)
    .await
    .context("failed to schedule employment tax reconciliation")?;
    let reconciliation_id = result.last_insert_id();
    ensure!(
        reconciliation_id != 0,
        "employment reconciliation insert returned no ID"
    );
    if let Some(anchor_settlement_id) = anchor.settlement_id() {
        ensure!(
            anchor_settlement_id < reconciliation_id,
            "employment reconciliation was inserted before its February pay anchor"
        );
    }
    Ok(())
}

fn income_year_allows_omitted_reconciliation(income_year: &IncomeYearClosingRow) -> bool {
    income_year.income_event_count == 0
        && income_year.last_income_event_id.is_none()
        && income_year.gross_employment_income_krw == 0
        && income_year.employee_national_pension_krw == 0
        && income_year.employee_health_insurance_krw == 0
        && income_year.employee_long_term_care_krw == 0
        && income_year.employee_employment_insurance_krw == 0
        && income_year.withheld_income_tax_krw == 0
        && income_year.withheld_local_income_tax_krw == 0
}

fn assessment_plan_allows_omitted_reconciliation(assessment: &EmploymentTaxAssessmentPlan) -> bool {
    assessment.calculation.gross_employment_income_krw == 0
        && assessment.calculation.employee_insurance_deduction_krw == 0
        && assessment.calculation.actual_pension_income_tax_credit_krw == 0
        && assessment
            .calculation
            .actual_pension_local_income_tax_effect_krw
            == 0
        && assessment.calculation.assessed_income_tax_krw == 0
        && assessment.calculation.assessed_local_income_tax_krw == 0
        && assessment.reconciliation.prepaid_income_tax_krw == 0
        && assessment.reconciliation.prepaid_local_income_tax_krw == 0
        && assessment.reconciliation.additional_tax_krw == 0
        && assessment.reconciliation.refund_krw == 0
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
struct LockedPensionLayersRow {
    tax_excluded_contribution_krw: i64,
    deferred_retirement_income_krw: i64,
    credited_contribution_krw: i64,
    earnings_krw: i64,
}

async fn persist_pension_allocations(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    context: AnnualTaxRunContext,
    assessment_id: u64,
    plan: &EmploymentTaxAssessmentPlan,
) -> Result<()> {
    let allocation = plan
        .pension_allocation
        .as_ref()
        .context("definitive assessment has no pension allocation plan")?;
    let movements = allocation.tax_layer_movements().collect::<Vec<_>>();
    let income_credit_sum = movements.iter().try_fold(0_i64, |total, movement| {
        total
            .checked_add(movement.income_tax_credit_krw)
            .context("persisted pension income-tax credits overflowed")
    })?;
    let local_credit_sum = movements.iter().try_fold(0_i64, |total, movement| {
        total
            .checked_add(movement.local_income_tax_effect_krw)
            .context("persisted pension local-tax effects overflowed")
    })?;
    ensure!(
        income_credit_sum == plan.calculation.actual_pension_income_tax_credit_krw
            && local_credit_sum == plan.calculation.actual_pension_local_income_tax_effect_krw,
        "pension allocation credits do not reconcile to the definitive assessment"
    );
    for movement in movements {
        let before: LockedPensionLayersRow = sqlx::query_as(
            "SELECT tax_excluded_contribution_krw, deferred_retirement_income_krw,
                    credited_contribution_krw, earnings_krw
             FROM pension_tax_balance
             WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
             FOR UPDATE",
        )
        .bind(context.save_id)
        .bind(context.run_revision)
        .bind(movement.account_id)
        .fetch_optional(&mut **tx)
        .await?
        .context("pension credit allocation has no tax-layer balance")?;
        ensure!(
            before.tax_excluded_contribution_krw >= movement.credited_contribution_krw,
            "pension credit allocation exceeds the tax-excluded layer"
        );
        let after_tax_excluded = before
            .tax_excluded_contribution_krw
            .checked_sub(movement.credited_contribution_krw)
            .context("pension tax-excluded layer underflowed")?;
        let after_credited = before
            .credited_contribution_krw
            .checked_add(movement.credited_contribution_krw)
            .context("pension credited layer overflowed")?;
        let account_id = ResourceId::from_u64(movement.account_id);
        let ledger = finance_rules
            .create_ledger_transaction(LedgerTransactionDraft {
                policy: policy_context(context)?,
                source: LedgerSource {
                    kind: LedgerSourceKind::PensionCreditAllocation,
                    source_id: movement.contribution_source_id.to_string(),
                },
                game_day: context.game_day,
                description: "연금계좌 세액공제 확정".to_owned(),
                postings: vec![
                    LedgerPosting {
                        account_code: LedgerAccountCode::PensionTaxExcludedContribution,
                        financial_account_id: Some(account_id),
                        amount_krw: movement
                            .credited_contribution_krw
                            .checked_neg()
                            .context("pension layer movement cannot be represented")?,
                    },
                    LedgerPosting {
                        account_code: LedgerAccountCode::PensionCreditedContribution,
                        financial_account_id: Some(account_id),
                        amount_krw: movement.credited_contribution_krw,
                    },
                ],
            })
            .context("pension credit allocation ledger is invalid")?;
        let ledger_id = write_ledger_transaction(tx, &ledger).await?;
        let insert = sqlx::query(
            "INSERT INTO pension_credit_allocation
                 (save_id, run_revision, tax_year, year_end_tax_assessment_id,
                  contribution_source_id, financial_account_id, ledger_transaction_id,
                  allocated_contribution_krw, income_tax_credit_krw,
                  local_income_tax_effect_krw, total_credit_krw)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(context.save_id)
        .bind(context.run_revision)
        .bind(plan.calculation.tax_year)
        .bind(assessment_id)
        .bind(movement.contribution_source_id)
        .bind(movement.account_id)
        .bind(ledger_id)
        .bind(movement.credited_contribution_krw)
        .bind(movement.income_tax_credit_krw)
        .bind(movement.local_income_tax_effect_krw)
        .bind(movement.total_credit_krw)
        .execute(&mut **tx)
        .await
        .context("failed to persist the pension credit allocation")?;
        ensure!(
            insert.rows_affected() == 1,
            "pension credit allocation was not inserted"
        );
        let update = sqlx::query(
            "UPDATE pension_tax_balance
             SET tax_excluded_contribution_krw = ?, credited_contribution_krw = ?
             WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
               AND tax_excluded_contribution_krw = ?
               AND deferred_retirement_income_krw = ?
               AND credited_contribution_krw = ? AND earnings_krw = ?",
        )
        .bind(after_tax_excluded)
        .bind(after_credited)
        .bind(context.save_id)
        .bind(context.run_revision)
        .bind(movement.account_id)
        .bind(before.tax_excluded_contribution_krw)
        .bind(before.deferred_retirement_income_krw)
        .bind(before.credited_contribution_krw)
        .bind(before.earnings_krw)
        .execute(&mut **tx)
        .await?;
        ensure!(
            update.rows_affected() == 1,
            "pension credit allocation lost its layer lock"
        );
        insert_pension_credit_value_event(
            tx,
            context,
            movement.account_id,
            movement.contribution_source_id,
            plan.calculation.tax_year,
            before,
            after_tax_excluded,
            after_credited,
        )
        .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_pension_credit_value_event(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    account_id: u64,
    contribution_source_id: u64,
    tax_year: u16,
    before: LockedPensionLayersRow,
    after_tax_excluded_krw: i64,
    after_credited_krw: i64,
) -> Result<()> {
    let position_market_value_krw: i64 = sqlx::query_scalar(
        "SELECT COALESCE(position_market_value_krw, 0)
         FROM pension_valuation_state
         WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(account_id)
    .fetch_optional(&mut **tx)
    .await?
    .unwrap_or(0);
    let account_total_before = sum_layers(
        before.tax_excluded_contribution_krw,
        before.deferred_retirement_income_krw,
        before.credited_contribution_krw,
        before.earnings_krw,
    )?;
    let account_total_after = sum_layers(
        after_tax_excluded_krw,
        before.deferred_retirement_income_krw,
        after_credited_krw,
        before.earnings_krw,
    )?;
    ensure!(
        account_total_before == account_total_after,
        "pension reclassification changed value"
    );
    sqlx::query(
        "INSERT INTO tax_account_value_event
             (save_id, run_revision, financial_account_id, event_game_day, cause,
              source_kind, source_id, occurrence,
              position_market_value_before_krw, position_market_value_after_krw,
              account_total_before_krw, account_total_after_krw, value_change_krw,
              before_tax_excluded_krw, before_deferred_retirement_krw,
              before_credited_contribution_krw, before_earnings_krw,
              after_tax_excluded_krw, after_deferred_retirement_krw,
              after_credited_contribution_krw, after_earnings_krw)
         VALUES (?, ?, ?, ?, 'pensionCreditFinalized', 'pensionCreditAllocation', ?, ?,
                 ?, ?, ?, ?, 0, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(account_id)
    .bind(context.game_day)
    .bind(contribution_source_id.to_string())
    .bind(tax_year)
    .bind(position_market_value_krw)
    .bind(position_market_value_krw)
    .bind(account_total_before)
    .bind(account_total_after)
    .bind(before.tax_excluded_contribution_krw)
    .bind(before.deferred_retirement_income_krw)
    .bind(before.credited_contribution_krw)
    .bind(before.earnings_krw)
    .bind(after_tax_excluded_krw)
    .bind(before.deferred_retirement_income_krw)
    .bind(after_credited_krw)
    .bind(before.earnings_krw)
    .execute(&mut **tx)
    .await
    .context("failed to persist the pension credit value event")?;
    Ok(())
}

fn sum_layers(a: i64, b: i64, c: i64, d: i64) -> Result<i64> {
    a.checked_add(b)
        .and_then(|value| value.checked_add(c))
        .and_then(|value| value.checked_add(d))
        .context("pension tax layers overflowed")
}

fn policy_context(context: AnnualTaxRunContext) -> Result<RunPolicyContext> {
    Ok(RunPolicyContext {
        run: RunId {
            save_id: ResourceId::from_u64(context.save_id),
            run_revision: context.run_revision,
        },
        policy_set_id: ResourceId::from_u64(context.policy_set_id),
    })
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ReconciliationScheduleRow {
    id: u64,
    due_game_day: u32,
    kind: String,
    payload_json: String,
    source_kind: String,
    source_id: String,
    occurrence: u64,
    status: String,
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
struct ReconciliationAssessmentRow {
    tax_year: u16,
    additional_tax_krw: i64,
    refund_krw: i64,
}

pub(super) async fn settle_employment_reconciliation_by_id_in_tx(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    context: AnnualTaxRunContext,
    settlement_id: u64,
) -> Result<()> {
    let schedule: ReconciliationScheduleRow = sqlx::query_as(
        "SELECT id, due_game_day, kind, CAST(payload AS CHAR) AS payload_json,
                source_kind, source_id, occurrence, status
         FROM scheduled_settlement
         WHERE save_id = ? AND run_revision = ? AND id = ? FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(settlement_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("employment reconciliation settlement is missing")?;
    let payload: ReconciliationPayload = serde_json::from_str(&schedule.payload_json)
        .context("stored employment reconciliation payload is invalid")?;
    ensure!(
        schedule.id == settlement_id
            && schedule.due_game_day == context.game_day
            && schedule.kind == RECONCILIATION_KIND
            && schedule.source_kind == RECONCILIATION_SOURCE_KIND
            && schedule.source_id == payload.assessment_id.to_string()
            && schedule.occurrence == RECONCILIATION_OCCURRENCE
            && schedule.status == "pending"
            && payload.version == RECONCILIATION_PAYLOAD_VERSION,
        "employment reconciliation settlement identity is invalid"
    );
    let assessment: ReconciliationAssessmentRow = sqlx::query_as(
        "SELECT tax_year, additional_tax_krw, refund_krw
         FROM year_end_tax_assessment
         WHERE save_id = ? AND run_revision = ? AND id = ?
           AND assessment_kind = 'employmentOnly'
           AND assessment_status = 'definitive' FOR SHARE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(payload.assessment_id.get())
    .fetch_optional(&mut **tx)
    .await?
    .context("employment reconciliation assessment is missing")?;
    ensure!(
        assessment.tax_year == payload.tax_year
            && (assessment.additional_tax_krw == 0 || assessment.refund_krw == 0),
        "employment reconciliation assessment disagrees with its payload"
    );
    let (cash_before, debt_before): (i64, i64) = sqlx::query_as(
        "SELECT cash_krw, debt_krw FROM save
         WHERE id = ? AND run_revision = ? AND policy_set_id = ? FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(context.policy_set_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("employment reconciliation run disappeared")?;
    ensure!(
        cash_before >= 0 && debt_before >= 0,
        "employment reconciliation balances are invalid"
    );

    let tax_obligation_amount_krw = if assessment.additional_tax_krw > 0 {
        assessment
            .additional_tax_krw
            .checked_sub(cash_before.min(assessment.additional_tax_krw))
            .context("employment additional tax underflowed")?
    } else {
        0
    };
    let (cash_after, debt_after, postings) = if assessment.refund_krw > 0 {
        (
            cash_before
                .checked_add(assessment.refund_krw)
                .context("employment tax refund overflowed wallet cash")?,
            debt_before,
            vec![
                LedgerPosting {
                    account_code: LedgerAccountCode::Wallet,
                    financial_account_id: None,
                    amount_krw: assessment.refund_krw,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::TaxSettlement,
                    financial_account_id: None,
                    amount_krw: assessment
                        .refund_krw
                        .checked_neg()
                        .context("employment tax refund posting overflowed")?,
                },
            ],
        )
    } else if assessment.additional_tax_krw > 0 {
        let wallet_debit = cash_before.min(assessment.additional_tax_krw);
        let mut postings = Vec::with_capacity(3);
        if wallet_debit > 0 {
            postings.push(LedgerPosting {
                account_code: LedgerAccountCode::Wallet,
                financial_account_id: None,
                amount_krw: wallet_debit
                    .checked_neg()
                    .context("employment tax wallet posting overflowed")?,
            });
        }
        if tax_obligation_amount_krw > 0 {
            postings.push(LedgerPosting {
                account_code: LedgerAccountCode::TaxObligationLiability,
                financial_account_id: None,
                amount_krw: tax_obligation_amount_krw
                    .checked_neg()
                    .context("employment tax debt posting overflowed")?,
            });
        }
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::TaxSettlement,
            financial_account_id: None,
            amount_krw: assessment.additional_tax_krw,
        });
        (
            cash_before
                .checked_sub(wallet_debit)
                .context("employment tax wallet underflowed")?,
            debt_before
                .checked_add(tax_obligation_amount_krw)
                .context("employment tax debt overflowed")?,
            postings,
        )
    } else {
        (cash_before, debt_before, Vec::new())
    };
    let tax_obligation_id = prepare_employment_tax_obligation(
        tx,
        context,
        payload.assessment_id.get(),
        schedule.due_game_day,
        tax_obligation_amount_krw,
    )
    .await?;
    let ledger_id = if postings.is_empty() {
        None
    } else {
        let ledger = finance_rules
            .create_ledger_transaction(LedgerTransactionDraft {
                policy: policy_context(context)?,
                source: LedgerSource {
                    kind: LedgerSourceKind::ScheduledSettlement,
                    source_id: settlement_id.to_string(),
                },
                game_day: context.game_day,
                description: "근로소득 연말정산".to_owned(),
                postings,
            })
            .context("employment reconciliation ledger is invalid")?;
        Some(write_employment_tax_ledger_transaction(tx, &ledger, tax_obligation_id).await?)
    };
    if let Some(tax_obligation_id) = tax_obligation_id {
        activate_employment_tax_obligation(
            tx,
            context,
            payload.assessment_id.get(),
            schedule.due_game_day,
            tax_obligation_id,
            ledger_id.context("employment tax obligation has no authority ledger")?,
        )
        .await?;
    }
    if cash_before != cash_after || debt_before != debt_after {
        let update = sqlx::query(
            "UPDATE save SET cash_krw = ?, debt_krw = ?
             WHERE id = ? AND run_revision = ? AND policy_set_id = ?
               AND cash_krw = ? AND debt_krw = ?",
        )
        .bind(cash_after)
        .bind(debt_after)
        .bind(context.save_id)
        .bind(context.run_revision)
        .bind(context.policy_set_id)
        .bind(cash_before)
        .bind(debt_before)
        .execute(&mut **tx)
        .await?;
        ensure!(
            update.rows_affected() == 1,
            "employment reconciliation lost save balances"
        );
    }
    let update = if let Some(ledger_id) = ledger_id {
        sqlx::query(
            "UPDATE scheduled_settlement
             SET status = 'settled', outcome = 'applied', settled_ledger_transaction_id = ?
             WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'pending'",
        )
        .bind(ledger_id)
        .bind(context.save_id)
        .bind(context.run_revision)
        .bind(settlement_id)
        .execute(&mut **tx)
        .await?
    } else {
        sqlx::query(
            "UPDATE scheduled_settlement
             SET status = 'settled', outcome = 'noMovement', outcome_reason = 'zeroTaxDue'
             WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'pending'",
        )
        .bind(context.save_id)
        .bind(context.run_revision)
        .bind(settlement_id)
        .execute(&mut **tx)
        .await?
    };
    ensure!(
        update.rows_affected() == 1,
        "employment reconciliation lost settlement state"
    );
    super::loans::validate_debt_projection_in_tx(tx, context.save_id, context.run_revision).await?;
    Ok(())
}

async fn prepare_employment_tax_obligation(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    assessment_id: u64,
    due_game_day: u32,
    amount_krw: i64,
) -> Result<Option<u64>> {
    if amount_krw == 0 {
        return Ok(None);
    }
    ensure!(
        amount_krw > 0 && due_game_day == context.game_day,
        "employment tax obligation is invalid"
    );
    let source_id = assessment_id.to_string();
    let insert = sqlx::query(
        "INSERT INTO tax_obligation
             (save_id, run_revision, household_id, policy_set_id,
              source_kind, source_id, due_game_day, original_amount_krw,
              paid_amount_krw, outstanding_amount_krw, status,
              authority_ledger_transaction_id)
         SELECT save.id, save.run_revision, household.id, save.policy_set_id,
                'yearEndTaxAssessment', ?, ?, ?, 0, ?, 'prepared', NULL
         FROM save
         INNER JOIN household
           ON household.save_id = save.id
          AND household.run_revision = save.run_revision
         WHERE save.id = ? AND save.run_revision = ? AND save.policy_set_id = ?
           AND NOT EXISTS (
               SELECT 1 FROM tax_obligation AS existing
               WHERE existing.save_id = save.id
                 AND existing.run_revision = save.run_revision
                 AND existing.source_kind = 'yearEndTaxAssessment'
                 AND BINARY existing.source_id = BINARY ?
           )",
    )
    .bind(&source_id)
    .bind(due_game_day)
    .bind(amount_krw)
    .bind(amount_krw)
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(context.policy_set_id)
    .bind(&source_id)
    .execute(&mut **tx)
    .await
    .context("failed to prepare the employment tax obligation")?;
    ensure!(
        insert.rows_affected() == 1,
        "employment tax obligation source is missing or already authoritative"
    );
    let obligation_id = insert.last_insert_id();
    ensure!(
        obligation_id != 0,
        "employment tax obligation insert returned no ID"
    );
    Ok(Some(obligation_id))
}

async fn activate_employment_tax_obligation(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    assessment_id: u64,
    due_game_day: u32,
    obligation_id: u64,
    ledger_transaction_id: u64,
) -> Result<()> {
    let update = sqlx::query(
        "UPDATE tax_obligation
         SET status = 'outstanding', authority_ledger_transaction_id = ?
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND policy_set_id = ? AND source_kind = 'yearEndTaxAssessment'
           AND BINARY source_id = BINARY ? AND due_game_day = ?
           AND status = 'prepared' AND authority_ledger_transaction_id IS NULL",
    )
    .bind(ledger_transaction_id)
    .bind(obligation_id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(context.policy_set_id)
    .bind(assessment_id.to_string())
    .bind(due_game_day)
    .execute(&mut **tx)
    .await
    .context("failed to activate the employment tax obligation")?;
    ensure!(
        update.rows_affected() == 1,
        "employment tax obligation lost its prepared state"
    );
    Ok(())
}

async fn write_employment_tax_ledger_transaction(
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
        "employment tax ledger obligation reference is incomplete"
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
    .bind(ledger_enum_db(&ledger.source().kind)?)
    .bind(&ledger.source().source_id)
    .bind(ledger.description())
    .execute(&mut **tx)
    .await
    .context("failed to write the employment tax authority ledger")?;
    let ledger_transaction_id = insert.last_insert_id();
    ensure!(
        ledger_transaction_id != 0,
        "employment tax authority ledger insert returned no ID"
    );
    for (index, posting) in ledger.postings().iter().enumerate() {
        let posting_order = u16::try_from(index + 1).context("too many employment tax postings")?;
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
        .bind(ledger_enum_db(&posting.account_code)?)
        .bind(posting.financial_account_id.map(ResourceId::get))
        .bind(posting_obligation_id)
        .bind(posting.amount_krw)
        .execute(&mut **tx)
        .await
        .context("failed to write an employment tax authority posting")?;
    }
    Ok(ledger_transaction_id)
}

fn ledger_enum_db<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value)? {
        serde_json::Value::String(text) => Ok(text),
        other => bail!("ledger value is not storable as a string: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn given_annual_context() -> AnnualTaxRunContext {
        AnnualTaxRunContext {
            save_id: 7,
            run_revision: 3,
            policy_set_id: 11,
            game_day: 365,
            market_date: Date::from_calendar_date(2027, Month::January, 1)
                .expect("유효한 날짜여야 한다"),
        }
    }

    mod context_연간_코디네이터_식별자를_만드는_경우 {
        use super::*;

        #[test]
        fn given_같은_런과_정책_when_해시하면_then_동일한_소문자_식별자를_만든다() {
            let context = given_annual_context();

            let first = annual_coordinator_key(context, 2026, 13);
            let second = annual_coordinator_key(context, 2026, 13);

            assert_eq!(first, second);
            assert_eq!(first.len(), 64);
            assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
        }

        #[test]
        fn given_다른_귀속연도_when_해시하면_then_식별자가_달라진다() {
            let context = given_annual_context();

            let first = annual_coordinator_key(context, 2026, 13);
            let second = annual_coordinator_key(context, 2025, 13);

            assert_ne!(first, second);
        }
    }

    mod context_근로소득_이벤트_권위를_판정하는_경우 {
        use super::*;

        #[test]
        fn given_이벤트가있는누계_when_판정하면_then_근로소득권위가있다() {
            let income_event_count = 1;
            let last_income_event_id = Some(17);

            let has_authority = income_event_authority(income_event_count, last_income_event_id)
                .expect("일관된 이벤트 권위여야 한다");

            assert!(has_authority);
        }

        #[test]
        fn given_이벤트가없는누계_when_판정하면_then_근로소득권위가없다() {
            let income_event_count = 0;
            let last_income_event_id = None;

            let has_authority = income_event_authority(income_event_count, last_income_event_id)
                .expect("일관된 빈 이벤트 권위여야 한다");

            assert!(!has_authority);
        }

        #[test]
        fn given_마지막이벤트가없는양수누계_when_판정하면_then_거절한다() {
            let income_event_count = 1;
            let last_income_event_id = None;

            let result = income_event_authority(income_event_count, last_income_event_id);

            assert!(result.is_err());
        }
    }

    mod context_2월_정산_anchor를_고르는_경우 {
        use super::*;

        #[test]
        fn given_민간급여와군급여_when_고르면_then_민간급여를_우선한다() {
            let private_payroll = ReconciliationAnchor::EmploymentPayroll {
                settlement_id: 10,
                due_game_day: 400,
            };
            let military_pay = ReconciliationAnchor::MilitaryPay {
                settlement_id: 11,
                due_game_day: 390,
            };

            let anchor =
                choose_reconciliation_anchor(Some(private_payroll), Some(military_pay), 420);

            assert_eq!(anchor, private_payroll);
        }

        #[test]
        fn given_군급여만_when_고르면_then_군급여를_사용한다() {
            let military_pay = ReconciliationAnchor::MilitaryPay {
                settlement_id: 11,
                due_game_day: 390,
            };

            let anchor = choose_reconciliation_anchor(None, Some(military_pay), 420);

            assert_eq!(anchor, military_pay);
        }

        #[test]
        fn given_급여anchor가없을때_when_고르면_then_정책일을_사용한다() {
            let policy_game_day = 420;

            let anchor = choose_reconciliation_anchor(None, None, policy_game_day);

            assert_eq!(
                anchor,
                ReconciliationAnchor::PolicyFallback {
                    due_game_day: policy_game_day,
                }
            );
        }
    }

    mod context_정책의_2월_정산일을_계산하는_경우 {
        use super::*;

        #[test]
        fn given_평년의31일_when_계산하면_then_2월말일로_당긴다() {
            let requested_day = 31;

            let date = february_reconciliation_date(2027, requested_day)
                .expect("정책 정산일을 계산할 수 있어야 한다");

            assert_eq!(
                date,
                Date::from_calendar_date(2027, Month::February, 28).expect("유효한 날짜여야 한다")
            );
        }

        #[test]
        fn given_윤년의31일_when_계산하면_then_2월29일로_당긴다() {
            let requested_day = 31;

            let date = february_reconciliation_date(2028, requested_day)
                .expect("정책 정산일을 계산할 수 있어야 한다");

            assert_eq!(
                date,
                Date::from_calendar_date(2028, Month::February, 29).expect("유효한 날짜여야 한다")
            );
        }
    }

    mod context_연금_기여_payload를_해석하는_경우 {
        use super::*;

        #[test]
        fn given_알수없는_필드_when_역직렬화하면_then_거절한다() {
            let payload = json!({
                "version": 1,
                "amountKrw": 100000,
                "allocations": [],
                "taxLayersAfter": {
                    "taxExcludedContributionKrw": 100000,
                    "deferredRetirementIncomeKrw": 0,
                    "creditedContributionKrw": 0,
                    "earningsKrw": 0
                },
                "unexpected": true
            });

            let result = serde_json::from_value::<ContributionPayload>(payload);

            assert!(result.is_err());
        }
    }

    mod context_연금_인출_payload를_검증하는_경우 {
        use super::*;

        #[test]
        fn given_세액공제되지않은_원금_when_검증하면_then_소비액만_복원한다() {
            let payload: WithdrawalPayload = serde_json::from_value(json!({
                "version": 1,
                "requestKind": "pensionSavingsEarly",
                "reason": null,
                "grossAmountKrw": 100000,
                "pensionAmountKrw": 0,
                "nonPensionAmountKrw": 100000,
                "taxFreeAmountKrw": 100000,
                "taxKrw": 0,
                "netPayoutKrw": 100000,
                "remainingLayers": {
                    "taxExcludedContributionKrw": 0,
                    "deferredRetirementIncomeKrw": 0,
                    "creditedContributionKrw": 0,
                    "earningsKrw": 0
                },
                "portions": [{
                    "treatment": "nonPension",
                    "grossAmountKrw": 100000,
                    "taxFreeAmountKrw": 100000,
                    "taxKrw": 0,
                    "netAmountKrw": 100000,
                    "taxLines": [{
                        "source": "taxExcludedContribution",
                        "grossAmountKrw": 100000,
                        "taxRate": {"type": "exempt"},
                        "taxKrw": 0,
                        "netAmountKrw": 100000
                    }]
                }]
            }))
            .expect("정확한 payload여야 한다");
            let row = PensionEventRow {
                id: 1,
                financial_account_id: 2,
                account_type: "pensionSavings".to_owned(),
                event_kind: "pensionWithdrawal".to_owned(),
                game_day: 10,
                movement_amount_krw: 100_000,
                payload_json: String::new(),
                ledger_transaction_id: Some(3),
            };

            let amount = validate_withdrawal_payload(&payload, &row)
                .expect("원금 소비액을 복원할 수 있어야 한다");

            assert_eq!(amount, 100_000);
        }
    }
}
