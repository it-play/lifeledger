use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sqlx::{MySql, MySqlPool, Transaction};
use time::{Date, Duration, Month};

use super::employment_income::{
    EmploymentIncomeAmounts, EmploymentIncomeEventSource, EmploymentIncomeEventWrite,
    record_employment_income_event_in_tx,
};
use super::life::read_tax_dependent_count_in_tx;
use super::mysql::write_ledger_transaction;
use super::types::{
    CareerPageQuery, CareerPayrollPageState, CareerPayrollState, CareerRewardPaymentState,
};
use crate::career::{
    DualContributionRatePolicy, EmployerContributionRate, EmployerSizeBand,
    EmploymentInsurancePolicy, EmploymentWithholdingRow, HealthInsurancePolicy,
    IndustrialAccidentPolicy, Industry, IndustryContributionRate, LocalIncomeWithholdingPolicy,
    LongTermCarePolicy, NationalPensionPolicy, OtherIncomeRewardPolicy, PayrollBreakdown,
    PayrollCalculationInput, PayrollPeriodInput, PayrollPolicy, PayrollRules,
};
use crate::finance::{
    FinanceRules, LedgerAccountCode, LedgerPosting, LedgerSource, LedgerSourceKind,
    LedgerTransactionDraft, ResourceId, RunId, RunPolicyContext, ScheduledSettlement,
    SettlementKind, SettlementSourceKind,
};

const PAYROLL_SETTLEMENT_KIND: &str = "employmentPayroll";
const PAYROLL_SETTLEMENT_SOURCE_KIND: &str = "employmentContract";
const PAYROLL_PAYLOAD_VERSION: u8 = 1;
const MAX_PAGE_LIMIT: u32 = 200;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PayrollSettlementPayload {
    version: u8,
    employment_contract_id: ResourceId,
    period_no: u64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReconciliationSettlementPayload {
    version: u8,
    tax_year: u16,
    assessment_id: ResourceId,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct EmploymentSettlementRow {
    id: u64,
    due_game_day: u32,
    kind: String,
    payload_json: String,
    source_kind: String,
    source_id: String,
    occurrence: u64,
    status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct EmploymentContractRow {
    id: u64,
    employment_policy_set_id: u64,
    finance_policy_set_id: u64,
    status: String,
    annual_salary_krw: i64,
    payday_day_of_month: u8,
    start_game_day: u32,
    payroll_baseline_period_no: u64,
    first_pay_reward_krw: i64,
    employer_size_band: String,
    industry_key: String,
    world_start_date: Date,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct NationalPensionPolicyRow {
    id: u64,
    monthly_income_rounding_unit_krw: i64,
    minimum_monthly_income_krw: i64,
    maximum_monthly_income_krw: i64,
    employee_rate_ppm: i64,
    employer_rate_ppm: i64,
    employee_rounding_unit_krw: i64,
    employer_rounding_unit_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct HealthInsurancePolicyRow {
    id: u64,
    monthly_remuneration_rounding_unit_krw: i64,
    employee_rate_ppm: i64,
    employer_rate_ppm: i64,
    employee_rounding_unit_krw: i64,
    employer_rounding_unit_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LongTermCarePolicyRow {
    id: u64,
    health_premium_rate_numerator: i64,
    health_premium_rate_denominator: i64,
    employee_rounding_unit_krw: i64,
    employer_rounding_unit_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct EmploymentInsurancePolicyRow {
    id: u64,
    employee_rate_ppm: i64,
    employee_rounding_unit_krw: i64,
    employer_rounding_unit_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct EmploymentInsuranceEmployerRateRow {
    employer_size_band: String,
    employer_rate_ppm: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct IndustrialAccidentPolicyRow {
    id: u64,
    industry_key: String,
    employer_rate_ppm: i64,
    employer_rounding_unit_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WithholdingVersionRow {
    id: u64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WithholdingPolicyRow {
    id: u64,
    lower_bound_krw: i64,
    upper_bound_exclusive_krw: Option<i64>,
    family_count: u8,
    child_count: u8,
    income_tax_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LocalIncomeWithholdingPolicyRow {
    id: u64,
    income_tax_rate_ppm: i64,
    rounding_unit_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct RewardPolicyRow {
    id: u64,
    income_tax_rate_ppm: i64,
    local_income_tax_rate_ppm: i64,
    income_tax_rounding_unit_krw: i64,
    local_income_tax_rounding_unit_krw: i64,
}

#[derive(Debug, Clone)]
struct LoadedPayrollPolicy {
    policy: PayrollPolicy,
    national_pension_policy_id: u64,
    health_insurance_policy_id: u64,
    long_term_care_policy_id: u64,
    employment_insurance_policy_id: u64,
    industrial_accident_policy_id: u64,
    withholding_version_id: u64,
    withholding_rows: Vec<WithholdingPolicyRow>,
    local_income_withholding_policy_id: u64,
    reward_policy_id: u64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PayrollApiRow {
    id: u64,
    employment_contract_id: u64,
    period_no: u64,
    salary_month_ordinal: u8,
    period_start_date: Date,
    period_end_exclusive_date: Date,
    payday_game_day: u32,
    gross_pay_krw: i64,
    national_pension_employee_krw: i64,
    national_pension_employer_krw: i64,
    health_insurance_employee_krw: i64,
    health_insurance_employer_krw: i64,
    long_term_care_employee_krw: i64,
    long_term_care_employer_krw: i64,
    employment_insurance_employee_krw: i64,
    employment_insurance_employer_krw: i64,
    industrial_accident_employer_krw: i64,
    withheld_income_tax_krw: i64,
    withheld_local_income_tax_krw: i64,
    net_salary_pay_krw: i64,
    reward_payment_id: Option<u64>,
    gross_reward_krw: Option<i64>,
    reward_withheld_income_tax_krw: Option<i64>,
    reward_withheld_local_income_tax_krw: Option<i64>,
    net_reward_krw: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct EmploymentIncomeYearLockRow {
    employment_policy_set_id: u64,
    status: String,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct MilitaryEmploymentPayrollInput {
    pub(super) service_id: u64,
    pub(super) employment_policy_set_id: u64,
    pub(super) payday: Date,
    pub(super) gross_pay_krw: i64,
    pub(super) dependents: u8,
}

pub(super) async fn calculate_military_employment_payroll_in_tx(
    tx: &mut Transaction<'_, MySql>,
    payroll_rules: &dyn PayrollRules,
    input: MilitaryEmploymentPayrollInput,
) -> Result<PayrollBreakdown> {
    let policy = load_payroll_policy(
        tx,
        input.employment_policy_set_id,
        input.payday,
        Industry::PublicSocial,
    )
    .await?;
    payroll_rules
        .validate_policy(&policy.policy)
        .context("military employment payroll policy is invalid")?;
    let payday_month_start = input
        .payday
        .replace_day(1)
        .context("military payroll payday month is invalid")?;
    let previous_month_day = payday_month_start
        .previous_day()
        .context("military payroll previous month is invalid")?;
    let synthetic_start = previous_month_day
        .replace_day(1)
        .context("military payroll period start is invalid")?;
    let annual_salary_krw = input
        .gross_pay_krw
        .checked_mul(12)
        .context("military payroll annualized gross overflowed")?;
    let breakdown = payroll_rules
        .calculate_payroll(PayrollCalculationInput {
            period: PayrollPeriodInput {
                contract_id: input.service_id,
                period_no: 1,
                contract_start_date: synthetic_start,
                annual_salary_krw,
                payday_day_of_month: input.payday.day(),
            },
            dependents: input.dependents,
            employer_size_band: EmployerSizeBand::Government,
            industry: Industry::PublicSocial,
            wanted_reward_gross_krw: None,
            policy: &policy.policy,
        })
        .context("military employment payroll calculation failed")?;
    ensure!(
        breakdown.period.payday == input.payday
            && breakdown.period.gross_pay_krw == input.gross_pay_krw,
        "military employment payroll changed the pinned gross or payday"
    );
    Ok(breakdown)
}

#[derive(Debug, Clone, Copy)]
pub(super) struct EmploymentPayrollSettlementContext {
    pub(super) save_id: u64,
    pub(super) run_revision: u32,
    pub(super) finance_policy_set_id: u64,
    pub(super) game_day: u32,
    pub(super) payday: Date,
    pub(super) settlement_id: u64,
}

#[derive(Debug, Clone, Copy)]
struct LedgerWriteContext {
    save_id: u64,
    run_revision: u32,
    policy_set_id: u64,
    game_day: u32,
}

struct PayrollRecordWrite<'a> {
    save_id: u64,
    run_revision: u32,
    employment_policy_set_id: u64,
    settlement_id: u64,
    ledger_transaction_id: Option<u64>,
    policy: &'a LoadedPayrollPolicy,
    withholding_row_id: u64,
    tax_year: u16,
    payday_game_day: u32,
    breakdown: &'a PayrollBreakdown,
}

#[derive(Debug, Clone, Copy)]
struct RewardPaymentWrite {
    save_id: u64,
    run_revision: u32,
    contract_id: u64,
    employment_policy_set_id: u64,
    payroll_record_id: u64,
    reward_policy_id: u64,
    ledger_transaction_id: u64,
    payment_date: Date,
    payment_game_day: u32,
    reward: crate::career::OtherIncomeRewardBreakdown,
}

pub(super) async fn read_career_payroll(
    pool: &MySqlPool,
    user_id: u64,
    query: CareerPageQuery,
) -> Result<CareerPayrollPageState> {
    validate_page_query(&query)?;
    let mut tx = pool.begin().await?;
    let scope: Option<(u64, u32)> = sqlx::query_as(
        "SELECT save.id, save.run_revision
         FROM save
         INNER JOIN career_run
           ON career_run.save_id = save.id
          AND career_run.run_revision = save.run_revision
         WHERE save.user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (save_id, run_revision) = scope.context("career payroll requires an active career run")?;
    let fetch_limit = query
        .limit
        .checked_add(1)
        .context("career payroll page limit overflowed")?;
    let rows: Vec<PayrollApiRow> = sqlx::query_as(
        "SELECT payroll.id, payroll.employment_contract_id, payroll.period_no,
                payroll.salary_month_ordinal, payroll.period_start_date,
                payroll.period_end_exclusive_date, payroll.payday_game_day,
                payroll.gross_pay_krw, payroll.national_pension_employee_krw,
                payroll.national_pension_employer_krw,
                payroll.health_insurance_employee_krw,
                payroll.health_insurance_employer_krw,
                payroll.long_term_care_employee_krw,
                payroll.long_term_care_employer_krw,
                payroll.employment_insurance_employee_krw,
                payroll.employment_insurance_employer_krw,
                payroll.industrial_accident_employer_krw,
                payroll.withheld_income_tax_krw,
                payroll.withheld_local_income_tax_krw,
                payroll.net_salary_pay_krw,
                reward.id AS reward_payment_id, reward.gross_reward_krw,
                reward.withheld_income_tax_krw AS reward_withheld_income_tax_krw,
                reward.withheld_local_income_tax_krw
                    AS reward_withheld_local_income_tax_krw,
                reward.net_reward_krw
         FROM payroll_record AS payroll
         LEFT JOIN career_reward_payment AS reward
           ON reward.save_id = payroll.save_id
          AND reward.run_revision = payroll.run_revision
          AND reward.payroll_record_id = payroll.id
         WHERE payroll.save_id = ? AND payroll.run_revision = ?
           AND (? IS NULL OR payroll.id < ?)
         ORDER BY payroll.id DESC LIMIT ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(query.before)
    .bind(query.before)
    .bind(fetch_limit)
    .fetch_all(&mut *tx)
    .await?;
    let has_more = rows.len() > query.limit as usize;
    let items = rows
        .into_iter()
        .take(query.limit as usize)
        .map(payroll_api_state)
        .collect::<Result<Vec<_>>>()?;
    let next_before = has_more.then(|| items.last().map(|item| item.id)).flatten();
    tx.commit().await?;
    Ok(CareerPayrollPageState { items, next_before })
}

pub(super) async fn read_latest_payroll_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<Option<CareerPayrollState>> {
    let row: Option<PayrollApiRow> = sqlx::query_as(
        "SELECT payroll.id, payroll.employment_contract_id, payroll.period_no,
                payroll.salary_month_ordinal, payroll.period_start_date,
                payroll.period_end_exclusive_date, payroll.payday_game_day,
                payroll.gross_pay_krw, payroll.national_pension_employee_krw,
                payroll.national_pension_employer_krw,
                payroll.health_insurance_employee_krw,
                payroll.health_insurance_employer_krw,
                payroll.long_term_care_employee_krw,
                payroll.long_term_care_employer_krw,
                payroll.employment_insurance_employee_krw,
                payroll.employment_insurance_employer_krw,
                payroll.industrial_accident_employer_krw,
                payroll.withheld_income_tax_krw,
                payroll.withheld_local_income_tax_krw,
                payroll.net_salary_pay_krw,
                reward.id AS reward_payment_id, reward.gross_reward_krw,
                reward.withheld_income_tax_krw AS reward_withheld_income_tax_krw,
                reward.withheld_local_income_tax_krw
                    AS reward_withheld_local_income_tax_krw,
                reward.net_reward_krw
         FROM payroll_record AS payroll
         LEFT JOIN career_reward_payment AS reward
           ON reward.save_id = payroll.save_id
          AND reward.run_revision = payroll.run_revision
          AND reward.payroll_record_id = payroll.id
         WHERE payroll.save_id = ? AND payroll.run_revision = ?
         ORDER BY payroll.payday_game_day DESC, payroll.id DESC LIMIT 1",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(payroll_api_state).transpose()
}

pub(super) fn validate_employment_settlement_envelope(
    settlement: &ScheduledSettlement,
) -> Result<()> {
    match settlement.kind {
        SettlementKind::EmploymentPayroll => {
            ensure!(
                settlement.source.kind == SettlementSourceKind::EmploymentContract,
                "employment payroll settlement has the wrong source kind"
            );
            let payload: PayrollSettlementPayload =
                serde_json::from_value(settlement.payload.clone())
                    .context("stored employment payroll payload is invalid")?;
            ensure!(
                payload.version == PAYROLL_PAYLOAD_VERSION
                    && payload.period_no > 0
                    && settlement.source.source_id == payload.employment_contract_id.to_string()
                    && settlement.source.occurrence == payload.period_no,
                "employment payroll settlement identity is invalid"
            );
        }
        SettlementKind::EmploymentReconciliation => {
            ensure!(
                settlement.source.kind == SettlementSourceKind::YearEndTaxAssessment,
                "employment reconciliation has the wrong source kind"
            );
            let payload: ReconciliationSettlementPayload =
                serde_json::from_value(settlement.payload.clone())
                    .context("stored employment reconciliation payload is invalid")?;
            ensure!(
                payload.version == PAYROLL_PAYLOAD_VERSION
                    && (1..=9999).contains(&payload.tax_year)
                    && settlement.source.source_id == payload.assessment_id.to_string()
                    && settlement.source.occurrence == 1,
                "employment reconciliation settlement identity is invalid"
            );
        }
        _ => bail!("settlement is not an employment settlement"),
    }
    Ok(())
}

pub(super) async fn schedule_initial_employment_payroll_in_tx(
    tx: &mut Transaction<'_, MySql>,
    payroll_rules: &dyn PayrollRules,
    save_id: u64,
    run_revision: u32,
    contract_id: u64,
) -> Result<u64> {
    let contract = read_contract(tx, save_id, run_revision, contract_id, true).await?;
    ensure!(
        contract.status == "pendingStart" || contract.status == "active",
        "initial payroll cannot be scheduled for an ended contract"
    );
    let period = payroll_rules
        .schedule_period(period_input(
            &contract,
            contract.payroll_baseline_period_no,
        )?)
        .context("initial payroll period is invalid")?;
    ensure_employment_policy_available(
        tx,
        contract.employment_policy_set_id,
        contract.finance_policy_set_id,
        period.payday,
    )
    .await?;
    insert_payroll_schedule(tx, save_id, run_revision, contract.id, &period).await
}

pub(super) async fn ensure_february_payroll_before_reconciliation_in_tx(
    tx: &mut Transaction<'_, MySql>,
    payroll_rules: &dyn PayrollRules,
    save_id: u64,
    run_revision: u32,
    contract_id: u64,
    reconciliation_year: i32,
) -> Result<u32> {
    let contract = read_contract(tx, save_id, run_revision, contract_id, true).await?;
    let last_period_no: Option<u64> = sqlx::query_scalar(
        "SELECT MAX(occurrence) FROM scheduled_settlement
         WHERE save_id = ? AND run_revision = ?
           AND BINARY source_kind = BINARY ? AND BINARY source_id = BINARY ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(PAYROLL_SETTLEMENT_SOURCE_KIND)
    .bind(contract_id.to_string())
    .fetch_one(&mut **tx)
    .await?;
    let mut period_no =
        last_period_no.unwrap_or_else(|| contract.payroll_baseline_period_no.saturating_sub(1));
    loop {
        period_no = period_no
            .checked_add(1)
            .context("employment payroll period overflowed before reconciliation")?;
        let period = payroll_rules
            .schedule_period(period_input(&contract, period_no)?)
            .context("employment reconciliation payroll period is invalid")?;
        ensure!(
            period.payday.year() < reconciliation_year
                || (period.payday.year() == reconciliation_year
                    && period.payday.month() <= Month::February),
            "employment payroll schedule skipped the reconciliation month"
        );
        insert_payroll_schedule(tx, save_id, run_revision, contract.id, &period).await?;
        if period.payday.year() == reconciliation_year && period.payday.month() == Month::February {
            return game_day_for_date(contract.world_start_date, period.payday);
        }
    }
}

pub(super) async fn settle_employment_payroll_by_id_in_tx(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    payroll_rules: &dyn PayrollRules,
    context: EmploymentPayrollSettlementContext,
) -> Result<()> {
    let EmploymentPayrollSettlementContext {
        save_id,
        run_revision,
        finance_policy_set_id,
        game_day,
        payday,
        settlement_id,
    } = context;
    let unlocked_settlement =
        read_settlement(tx, save_id, run_revision, settlement_id, false).await?;
    let payload = decode_payroll_settlement(&unlocked_settlement)?;
    let contract = read_contract(
        tx,
        save_id,
        run_revision,
        payload.employment_contract_id.get(),
        true,
    )
    .await?;
    let locked_settlement = read_settlement(tx, save_id, run_revision, settlement_id, true).await?;
    ensure!(
        locked_settlement.id == unlocked_settlement.id
            && locked_settlement.due_game_day == unlocked_settlement.due_game_day
            && locked_settlement.kind == unlocked_settlement.kind
            && locked_settlement.payload_json == unlocked_settlement.payload_json
            && locked_settlement.source_kind == unlocked_settlement.source_kind
            && locked_settlement.source_id == unlocked_settlement.source_id
            && locked_settlement.occurrence == unlocked_settlement.occurrence
            && locked_settlement.status == unlocked_settlement.status,
        "employment payroll settlement changed before its lock"
    );
    ensure!(
        locked_settlement.status == "pending"
            && locked_settlement.due_game_day == game_day
            && contract.status == "active"
            && contract.finance_policy_set_id == finance_policy_set_id
            && payload.period_no >= contract.payroll_baseline_period_no,
        "employment payroll is not due for the active contract"
    );
    let period_input = period_input(&contract, payload.period_no)?;
    let expected_period = payroll_rules
        .schedule_period(period_input)
        .context("stored employment contract has invalid payroll terms")?;
    ensure!(
        expected_period.payday == payday
            && game_day_for_date(contract.world_start_date, expected_period.payday)? == game_day,
        "employment payroll schedule disagrees with the game date"
    );
    ensure_employment_policy_available(
        tx,
        contract.employment_policy_set_id,
        finance_policy_set_id,
        payday,
    )
    .await?;
    let loaded_policy = load_payroll_policy(
        tx,
        contract.employment_policy_set_id,
        payday,
        enum_from_db(&contract.industry_key)?,
    )
    .await?;
    payroll_rules
        .validate_policy(&loaded_policy.policy)
        .context("published employment payroll policy is invalid")?;
    let reward_gross = (payload.period_no == 1 && contract.first_pay_reward_krw > 0)
        .then_some(contract.first_pay_reward_krw);
    let dependents = read_tax_dependent_count_in_tx(tx, save_id, run_revision, game_day).await?;
    let breakdown = payroll_rules
        .calculate_payroll(PayrollCalculationInput {
            period: period_input,
            dependents,
            employer_size_band: enum_from_db(&contract.employer_size_band)?,
            industry: enum_from_db(&contract.industry_key)?,
            wanted_reward_gross_krw: reward_gross,
            policy: &loaded_policy.policy,
        })
        .context("employment payroll calculation failed")?;
    ensure!(
        breakdown.period == expected_period,
        "payroll calculation changed the scheduled period"
    );
    let tax_year = payroll_tax_year(&breakdown)?;
    let year_row: Option<EmploymentIncomeYearLockRow> = sqlx::query_as(
        "SELECT employment_policy_set_id, status
         FROM employment_income_year
         WHERE save_id = ? AND run_revision = ? AND tax_year = ? FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(tax_year)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(row) = &year_row {
        ensure!(
            row.status == "open"
                && row.employment_policy_set_id == contract.employment_policy_set_id,
            "employment payroll targets a closed or mismatched tax year"
        );
    }
    let salary_ledger_id = create_salary_ledger(
        tx,
        finance_rules,
        LedgerWriteContext {
            save_id,
            run_revision,
            policy_set_id: finance_policy_set_id,
            game_day,
        },
        settlement_id,
        &breakdown,
    )
    .await?;
    let withholding_row_id = selected_withholding_row_id(&loaded_policy, &breakdown)?;
    let payroll_record_id = insert_payroll_record(
        tx,
        PayrollRecordWrite {
            save_id,
            run_revision,
            employment_policy_set_id: contract.employment_policy_set_id,
            settlement_id,
            ledger_transaction_id: salary_ledger_id,
            policy: &loaded_policy,
            withholding_row_id,
            tax_year,
            payday_game_day: game_day,
            breakdown: &breakdown,
        },
    )
    .await?;
    if let Some(reward) = breakdown.wanted_reward {
        let reward_ledger_id = create_reward_ledger(
            tx,
            finance_rules,
            LedgerWriteContext {
                save_id,
                run_revision,
                policy_set_id: finance_policy_set_id,
                game_day,
            },
            contract.id,
            reward,
        )
        .await?;
        insert_reward_payment(
            tx,
            RewardPaymentWrite {
                save_id,
                run_revision,
                contract_id: contract.id,
                employment_policy_set_id: contract.employment_policy_set_id,
                payroll_record_id,
                reward_policy_id: loaded_policy.reward_policy_id,
                ledger_transaction_id: reward_ledger_id,
                payment_date: payday,
                payment_game_day: game_day,
                reward,
            },
        )
        .await?;
    }
    let insurance = breakdown.insurance;
    record_employment_income_event_in_tx(
        tx,
        EmploymentIncomeEventWrite {
            save_id,
            run_revision,
            employment_policy_set_id: contract.employment_policy_set_id,
            source: EmploymentIncomeEventSource::EmploymentPayroll {
                payroll_record_id,
                period_no: payload.period_no,
            },
            scheduled_settlement_id: settlement_id,
            ledger_transaction_id: salary_ledger_id,
            paid_game_day: game_day,
            paid_date: payday,
            amounts: EmploymentIncomeAmounts {
                gross_employment_income_krw: breakdown.employment_income_accrual_krw,
                employee_national_pension_krw: insurance.national_pension.employee_amount_krw,
                employee_health_insurance_krw: insurance.health_insurance.employee_amount_krw,
                employee_long_term_care_krw: insurance.long_term_care.employee_amount_krw,
                employee_employment_insurance_krw: insurance
                    .employment_insurance
                    .employee_amount_krw,
                employee_insurance_total_krw: breakdown.employee_insurance_total_krw,
                withheld_income_tax_krw: breakdown.withheld_income_tax_krw,
                withheld_local_income_tax_krw: breakdown.withheld_local_income_tax_krw,
                net_pay_krw: breakdown.net_salary_pay_krw,
            },
        },
    )
    .await?;
    credit_wallet(
        tx,
        save_id,
        run_revision,
        finance_policy_set_id,
        breakdown.total_wallet_credit_krw,
    )
    .await?;
    let next_period_no = payload
        .period_no
        .checked_add(1)
        .context("employment payroll period overflowed")?;
    let next_period = payroll_rules
        .schedule_period(PayrollPeriodInput {
            period_no: next_period_no,
            ..period_input
        })
        .context("next employment payroll period is invalid")?;
    insert_payroll_schedule(tx, save_id, run_revision, contract.id, &next_period).await?;
    transition_payroll_settlement(
        tx,
        save_id,
        run_revision,
        settlement_id,
        salary_ledger_id,
        breakdown.period.gross_pay_krw,
    )
    .await
}

fn validate_page_query(query: &CareerPageQuery) -> Result<()> {
    ensure!(
        (1..=MAX_PAGE_LIMIT).contains(&query.limit),
        "career payroll page limit must be between 1 and {MAX_PAGE_LIMIT}"
    );
    ensure!(
        query.before != Some(0),
        "career payroll page cursor must be positive"
    );
    Ok(())
}

fn payroll_api_state(row: PayrollApiRow) -> Result<CareerPayrollState> {
    let reward = match (
        row.reward_payment_id,
        row.gross_reward_krw,
        row.reward_withheld_income_tax_krw,
        row.reward_withheld_local_income_tax_krw,
        row.net_reward_krw,
    ) {
        (None, None, None, None, None) => None,
        (Some(payment_id), Some(gross), Some(income_tax), Some(local_tax), Some(net)) => {
            Some(CareerRewardPaymentState {
                payment_id: resource_id(payment_id, "career reward payment")?,
                gross_reward_krw: gross,
                withheld_income_tax_krw: income_tax,
                withheld_local_income_tax_krw: local_tax,
                net_reward_krw: net,
            })
        }
        _ => bail!("stored career reward payment is incomplete"),
    };
    Ok(CareerPayrollState {
        id: resource_id(row.id, "payroll record")?,
        contract_id: resource_id(row.employment_contract_id, "employment contract")?,
        period_no: row.period_no,
        salary_month_ordinal: row.salary_month_ordinal,
        period_start_date: row.period_start_date.to_string(),
        period_end_exclusive_date: row.period_end_exclusive_date.to_string(),
        paid_game_day: row.payday_game_day,
        gross_pay_krw: row.gross_pay_krw,
        employee_national_pension_krw: row.national_pension_employee_krw,
        employer_national_pension_krw: row.national_pension_employer_krw,
        employee_health_insurance_krw: row.health_insurance_employee_krw,
        employer_health_insurance_krw: row.health_insurance_employer_krw,
        employee_long_term_care_krw: row.long_term_care_employee_krw,
        employer_long_term_care_krw: row.long_term_care_employer_krw,
        employee_employment_insurance_krw: row.employment_insurance_employee_krw,
        employer_employment_insurance_krw: row.employment_insurance_employer_krw,
        employer_industrial_accident_krw: row.industrial_accident_employer_krw,
        withheld_income_tax_krw: row.withheld_income_tax_krw,
        withheld_local_income_tax_krw: row.withheld_local_income_tax_krw,
        net_pay_krw: row.net_salary_pay_krw,
        reward,
    })
}

async fn read_settlement(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    settlement_id: u64,
    for_update: bool,
) -> Result<EmploymentSettlementRow> {
    let query = if for_update {
        "SELECT id, due_game_day, kind, CAST(payload AS CHAR) AS payload_json,
                source_kind, source_id, occurrence, status
         FROM scheduled_settlement
         WHERE save_id = ? AND run_revision = ? AND id = ? FOR UPDATE"
    } else {
        "SELECT id, due_game_day, kind, CAST(payload AS CHAR) AS payload_json,
                source_kind, source_id, occurrence, status
         FROM scheduled_settlement
         WHERE save_id = ? AND run_revision = ? AND id = ?"
    };
    sqlx::query_as(query)
        .bind(save_id)
        .bind(run_revision)
        .bind(settlement_id)
        .fetch_optional(&mut **tx)
        .await?
        .context("employment payroll settlement is missing")
}

fn decode_payroll_settlement(row: &EmploymentSettlementRow) -> Result<PayrollSettlementPayload> {
    ensure!(
        row.id > 0
            && row.kind == PAYROLL_SETTLEMENT_KIND
            && row.source_kind == PAYROLL_SETTLEMENT_SOURCE_KIND,
        "stored employment payroll settlement kind is invalid"
    );
    let payload: PayrollSettlementPayload = serde_json::from_str(&row.payload_json)
        .context("stored employment payroll payload is invalid")?;
    ensure!(
        payload.version == PAYROLL_PAYLOAD_VERSION
            && payload.period_no > 0
            && row.source_id == payload.employment_contract_id.to_string()
            && row.occurrence == payload.period_no,
        "stored employment payroll settlement identity is invalid"
    );
    Ok(payload)
}

async fn read_contract(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    contract_id: u64,
    for_update: bool,
) -> Result<EmploymentContractRow> {
    let query = if for_update {
        sqlx::query_as(
            "SELECT contract.id, contract.employment_policy_set_id,
                save.policy_set_id AS finance_policy_set_id, contract.status,
                contract.annual_salary_krw, contract.payday_day_of_month,
                contract.start_game_day, contract.payroll_baseline_period_no,
                contract.first_pay_reward_krw,
                contract.employer_size_band, industry.industry_key,
                world.start_date AS world_start_date
         FROM employment_contract AS contract
         INNER JOIN save
           ON save.id = contract.save_id AND save.run_revision = contract.run_revision
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         INNER JOIN career_industry AS industry
           ON industry.career_catalog_bundle_id = contract.career_catalog_bundle_id
          AND industry.id = contract.career_industry_id
         WHERE contract.save_id = ? AND contract.run_revision = ? AND contract.id = ?
         FOR UPDATE",
        )
    } else {
        sqlx::query_as(
            "SELECT contract.id, contract.employment_policy_set_id,
                save.policy_set_id AS finance_policy_set_id, contract.status,
                contract.annual_salary_krw, contract.payday_day_of_month,
                contract.start_game_day, contract.payroll_baseline_period_no,
                contract.first_pay_reward_krw,
                contract.employer_size_band, industry.industry_key,
                world.start_date AS world_start_date
         FROM employment_contract AS contract
         INNER JOIN save
           ON save.id = contract.save_id AND save.run_revision = contract.run_revision
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         INNER JOIN career_industry AS industry
           ON industry.career_catalog_bundle_id = contract.career_catalog_bundle_id
          AND industry.id = contract.career_industry_id
         WHERE contract.save_id = ? AND contract.run_revision = ? AND contract.id = ?",
        )
    };
    query
        .bind(save_id)
        .bind(run_revision)
        .bind(contract_id)
        .fetch_optional(&mut **tx)
        .await?
        .context("employment payroll contract is missing")
}

fn period_input(contract: &EmploymentContractRow, period_no: u64) -> Result<PayrollPeriodInput> {
    let contract_start_date = contract
        .world_start_date
        .checked_add(Duration::days(i64::from(contract.start_game_day)))
        .context("employment contract start date overflowed")?;
    Ok(PayrollPeriodInput {
        contract_id: contract.id,
        period_no,
        contract_start_date,
        annual_salary_krw: contract.annual_salary_krw,
        payday_day_of_month: contract.payday_day_of_month,
    })
}

fn game_day_for_date(world_start_date: Date, date: Date) -> Result<u32> {
    u32::try_from((date - world_start_date).whole_days())
        .context("employment payroll date is outside the game-day range")
}

async fn ensure_employment_policy_available(
    tx: &mut Transaction<'_, MySql>,
    employment_policy_set_id: u64,
    finance_policy_set_id: u64,
    payday: Date,
) -> Result<()> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM employment_policy_set AS employment
         INNER JOIN employment_finance_compatibility AS compatibility
           ON compatibility.employment_policy_set_id = employment.id
          AND compatibility.policy_set_id = ?
         WHERE employment.id = ? AND employment.published_at IS NOT NULL
           AND ? >= employment.coverage_start
           AND ? < employment.coverage_end_exclusive",
    )
    .bind(finance_policy_set_id)
    .bind(employment_policy_set_id)
    .bind(payday)
    .bind(payday)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        count == 1,
        "employment policy is unavailable for the payday"
    );
    Ok(())
}

async fn load_payroll_policy(
    tx: &mut Transaction<'_, MySql>,
    employment_policy_set_id: u64,
    payday: Date,
    industry: Industry,
) -> Result<LoadedPayrollPolicy> {
    let pension = exactly_one(
        sqlx::query_as::<_, NationalPensionPolicyRow>(
            "SELECT id, monthly_income_rounding_unit_krw, minimum_monthly_income_krw,
                    maximum_monthly_income_krw,
                    CAST(employee_rate_ppm AS SIGNED) AS employee_rate_ppm,
                    CAST(employer_rate_ppm AS SIGNED) AS employer_rate_ppm,
                    employee_rounding_unit_krw, employer_rounding_unit_krw
             FROM national_pension_policy
             WHERE employment_policy_set_id = ? AND ? >= effective_from
               AND (effective_to_exclusive IS NULL OR ? < effective_to_exclusive)",
        )
        .bind(employment_policy_set_id)
        .bind(payday)
        .bind(payday)
        .fetch_all(&mut **tx)
        .await?,
        "national pension policy",
    )?;
    let health = exactly_one(
        sqlx::query_as::<_, HealthInsurancePolicyRow>(
            "SELECT id, monthly_remuneration_rounding_unit_krw,
                    CAST(employee_rate_ppm AS SIGNED) AS employee_rate_ppm,
                    CAST(employer_rate_ppm AS SIGNED) AS employer_rate_ppm,
                    employee_rounding_unit_krw, employer_rounding_unit_krw
             FROM health_insurance_policy
             WHERE employment_policy_set_id = ? AND ? >= effective_from
               AND (effective_to_exclusive IS NULL OR ? < effective_to_exclusive)",
        )
        .bind(employment_policy_set_id)
        .bind(payday)
        .bind(payday)
        .fetch_all(&mut **tx)
        .await?,
        "health insurance policy",
    )?;
    let care = exactly_one(
        sqlx::query_as::<_, LongTermCarePolicyRow>(
            "SELECT id,
                    CAST(health_premium_rate_numerator AS SIGNED)
                        AS health_premium_rate_numerator,
                    CAST(health_premium_rate_denominator AS SIGNED)
                        AS health_premium_rate_denominator,
                    employee_rounding_unit_krw, employer_rounding_unit_krw
             FROM long_term_care_policy
             WHERE employment_policy_set_id = ? AND ? >= effective_from
               AND (effective_to_exclusive IS NULL OR ? < effective_to_exclusive)",
        )
        .bind(employment_policy_set_id)
        .bind(payday)
        .bind(payday)
        .fetch_all(&mut **tx)
        .await?,
        "long-term care policy",
    )?;
    let employment = exactly_one(
        sqlx::query_as::<_, EmploymentInsurancePolicyRow>(
            "SELECT id, CAST(employee_rate_ppm AS SIGNED) AS employee_rate_ppm,
                    employee_rounding_unit_krw, employer_rounding_unit_krw
             FROM employment_insurance_policy
             WHERE employment_policy_set_id = ? AND ? >= effective_from
               AND (effective_to_exclusive IS NULL OR ? < effective_to_exclusive)",
        )
        .bind(employment_policy_set_id)
        .bind(payday)
        .bind(payday)
        .fetch_all(&mut **tx)
        .await?,
        "employment insurance policy",
    )?;
    let employment_rates: Vec<EmploymentInsuranceEmployerRateRow> = sqlx::query_as(
        "SELECT employer_size_band,
                CAST(employer_rate_ppm AS SIGNED) AS employer_rate_ppm
         FROM employment_insurance_employer_rate
         WHERE employment_policy_set_id = ? AND employment_insurance_policy_id = ?
         ORDER BY employer_size_band",
    )
    .bind(employment_policy_set_id)
    .bind(employment.id)
    .fetch_all(&mut **tx)
    .await?;
    let employment_rates = employment_rates
        .into_iter()
        .map(|row| {
            Ok(EmployerContributionRate {
                employer_size_band: enum_from_db(&row.employer_size_band)?,
                rate_ppm: row.employer_rate_ppm,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let industrial_rows: Vec<IndustrialAccidentPolicyRow> = sqlx::query_as(
        "SELECT id, industry_key,
                CAST(employer_rate_ppm AS SIGNED) AS employer_rate_ppm,
                employer_rounding_unit_krw
         FROM industrial_accident_policy
         WHERE employment_policy_set_id = ? AND ? >= effective_from
           AND (effective_to_exclusive IS NULL OR ? < effective_to_exclusive)
         ORDER BY industry_key",
    )
    .bind(employment_policy_set_id)
    .bind(payday)
    .bind(payday)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        !industrial_rows.is_empty(),
        "industrial accident policy is unavailable"
    );
    let industrial_rounding_unit = industrial_rows[0].employer_rounding_unit_krw;
    ensure!(
        industrial_rows
            .iter()
            .all(|row| row.employer_rounding_unit_krw == industrial_rounding_unit),
        "industrial accident policy rounding metadata disagrees"
    );
    let mut industrial_accident_policy_id = None;
    let industrial_rates = industrial_rows
        .iter()
        .map(|row| {
            let row_industry: Industry = enum_from_db(&row.industry_key)?;
            if row_industry == industry {
                industrial_accident_policy_id = Some(row.id);
            }
            Ok(IndustryContributionRate {
                industry: row_industry,
                rate_ppm: row.employer_rate_ppm,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let withholding_version = exactly_one(
        sqlx::query_as::<_, WithholdingVersionRow>(
            "SELECT id FROM employment_withholding_table_version
             WHERE employment_policy_set_id = ? AND ? >= effective_from
               AND (effective_to_exclusive IS NULL OR ? < effective_to_exclusive)",
        )
        .bind(employment_policy_set_id)
        .bind(payday)
        .bind(payday)
        .fetch_all(&mut **tx)
        .await?,
        "employment withholding table version",
    )?;
    let withholding_rows: Vec<WithholdingPolicyRow> = sqlx::query_as(
        "SELECT id, lower_bound_krw, upper_bound_exclusive_krw,
                family_count, child_count, income_tax_krw
         FROM employment_withholding_table_row
         WHERE employment_policy_set_id = ?
           AND employment_withholding_table_version_id = ?
         ORDER BY family_count, child_count, lower_bound_krw",
    )
    .bind(employment_policy_set_id)
    .bind(withholding_version.id)
    .fetch_all(&mut **tx)
    .await?;
    let local = exactly_one(
        sqlx::query_as::<_, LocalIncomeWithholdingPolicyRow>(
            "SELECT id, CAST(income_tax_rate_ppm AS SIGNED) AS income_tax_rate_ppm,
                    rounding_unit_krw
             FROM local_income_withholding_policy
             WHERE employment_policy_set_id = ? AND ? >= effective_from
               AND (effective_to_exclusive IS NULL OR ? < effective_to_exclusive)",
        )
        .bind(employment_policy_set_id)
        .bind(payday)
        .bind(payday)
        .fetch_all(&mut **tx)
        .await?,
        "local income withholding policy",
    )?;
    let reward = exactly_one(
        sqlx::query_as::<_, RewardPolicyRow>(
            "SELECT id, CAST(income_tax_rate_ppm AS SIGNED) AS income_tax_rate_ppm,
                    CAST(local_income_tax_rate_ppm AS SIGNED)
                        AS local_income_tax_rate_ppm,
                    income_tax_rounding_unit_krw,
                    local_income_tax_rounding_unit_krw
             FROM other_income_reward_policy
             WHERE employment_policy_set_id = ? AND ? >= effective_from
               AND (effective_to_exclusive IS NULL OR ? < effective_to_exclusive)",
        )
        .bind(employment_policy_set_id)
        .bind(payday)
        .bind(payday)
        .fetch_all(&mut **tx)
        .await?,
        "other-income reward policy",
    )?;
    let policy = PayrollPolicy {
        national_pension: NationalPensionPolicy {
            monthly_income_rounding_unit_krw: pension.monthly_income_rounding_unit_krw,
            minimum_monthly_income_krw: pension.minimum_monthly_income_krw,
            maximum_monthly_income_krw: pension.maximum_monthly_income_krw,
            contribution: DualContributionRatePolicy {
                employee_rate_ppm: pension.employee_rate_ppm,
                employer_rate_ppm: pension.employer_rate_ppm,
                employee_rounding_unit_krw: pension.employee_rounding_unit_krw,
                employer_rounding_unit_krw: pension.employer_rounding_unit_krw,
            },
        },
        health_insurance: HealthInsurancePolicy {
            monthly_remuneration_rounding_unit_krw: health.monthly_remuneration_rounding_unit_krw,
            contribution: DualContributionRatePolicy {
                employee_rate_ppm: health.employee_rate_ppm,
                employer_rate_ppm: health.employer_rate_ppm,
                employee_rounding_unit_krw: health.employee_rounding_unit_krw,
                employer_rounding_unit_krw: health.employer_rounding_unit_krw,
            },
        },
        long_term_care: LongTermCarePolicy {
            health_premium_rate_numerator: care.health_premium_rate_numerator,
            health_premium_rate_denominator: care.health_premium_rate_denominator,
            employee_rounding_unit_krw: care.employee_rounding_unit_krw,
            employer_rounding_unit_krw: care.employer_rounding_unit_krw,
        },
        employment_insurance: EmploymentInsurancePolicy {
            employee_rate_ppm: employment.employee_rate_ppm,
            employer_rates: employment_rates,
            employee_rounding_unit_krw: employment.employee_rounding_unit_krw,
            employer_rounding_unit_krw: employment.employer_rounding_unit_krw,
        },
        industrial_accident: IndustrialAccidentPolicy {
            employer_rates: industrial_rates,
            employer_rounding_unit_krw: industrial_rounding_unit,
        },
        employment_withholding_table: withholding_rows
            .iter()
            .map(|row| EmploymentWithholdingRow {
                lower_bound_krw: row.lower_bound_krw,
                upper_bound_exclusive_krw: row.upper_bound_exclusive_krw,
                family_count: row.family_count,
                child_count: row.child_count,
                income_tax_krw: row.income_tax_krw,
            })
            .collect(),
        local_income_withholding: LocalIncomeWithholdingPolicy {
            income_tax_rate_ppm: local.income_tax_rate_ppm,
            rounding_unit_krw: local.rounding_unit_krw,
        },
        wanted_reward: Some(OtherIncomeRewardPolicy {
            income_tax_rate_ppm: reward.income_tax_rate_ppm,
            local_income_tax_rate_ppm: reward.local_income_tax_rate_ppm,
            income_tax_rounding_unit_krw: reward.income_tax_rounding_unit_krw,
            local_income_tax_rounding_unit_krw: reward.local_income_tax_rounding_unit_krw,
        }),
    };
    Ok(LoadedPayrollPolicy {
        policy,
        national_pension_policy_id: pension.id,
        health_insurance_policy_id: health.id,
        long_term_care_policy_id: care.id,
        employment_insurance_policy_id: employment.id,
        industrial_accident_policy_id: industrial_accident_policy_id
            .context("industrial accident policy is missing the contract industry")?,
        withholding_version_id: withholding_version.id,
        withholding_rows,
        local_income_withholding_policy_id: local.id,
        reward_policy_id: reward.id,
    })
}

fn exactly_one<T>(mut rows: Vec<T>, label: &str) -> Result<T> {
    ensure!(
        rows.len() == 1,
        "{label} must have exactly one effective row"
    );
    Ok(rows.remove(0))
}

fn selected_withholding_row_id(
    policy: &LoadedPayrollPolicy,
    breakdown: &PayrollBreakdown,
) -> Result<u64> {
    policy
        .withholding_rows
        .iter()
        .find(|row| {
            row.family_count == breakdown.withholding.family_count
                && row.child_count == breakdown.withholding.child_count
                && row.lower_bound_krw == breakdown.withholding.row_lower_bound_krw
                && row.upper_bound_exclusive_krw
                    == breakdown.withholding.row_upper_bound_exclusive_krw
                && row.income_tax_krw == breakdown.withholding.income_tax_krw
        })
        .map(|row| row.id)
        .context("payroll calculation did not preserve the withholding row identity")
}

fn payroll_tax_year(breakdown: &PayrollBreakdown) -> Result<u16> {
    u16::try_from(breakdown.period.payday.year())
        .context("payroll tax year is outside the supported range")
}

async fn create_salary_ledger(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    context: LedgerWriteContext,
    settlement_id: u64,
    breakdown: &PayrollBreakdown,
) -> Result<Option<u64>> {
    if breakdown.period.gross_pay_krw == 0 {
        return Ok(None);
    }
    let mut postings = Vec::with_capacity(8);
    push_posting(
        &mut postings,
        LedgerAccountCode::Wallet,
        breakdown.net_salary_pay_krw,
    );
    push_posting(
        &mut postings,
        LedgerAccountCode::SalaryIncome,
        breakdown
            .period
            .gross_pay_krw
            .checked_neg()
            .context("gross salary ledger amount overflowed")?,
    );
    push_posting(
        &mut postings,
        LedgerAccountCode::EmployeeNationalPensionExpense,
        breakdown.insurance.national_pension.employee_amount_krw,
    );
    push_posting(
        &mut postings,
        LedgerAccountCode::EmployeeHealthInsuranceExpense,
        breakdown.insurance.health_insurance.employee_amount_krw,
    );
    push_posting(
        &mut postings,
        LedgerAccountCode::EmployeeLongTermCareExpense,
        breakdown.insurance.long_term_care.employee_amount_krw,
    );
    push_posting(
        &mut postings,
        LedgerAccountCode::EmployeeEmploymentInsuranceExpense,
        breakdown.insurance.employment_insurance.employee_amount_krw,
    );
    push_posting(
        &mut postings,
        LedgerAccountCode::EmploymentIncomeTaxWithholding,
        breakdown.withheld_income_tax_krw,
    );
    push_posting(
        &mut postings,
        LedgerAccountCode::EmploymentLocalIncomeTaxWithholding,
        breakdown.withheld_local_income_tax_krw,
    );
    let ledger = finance_rules
        .create_ledger_transaction(LedgerTransactionDraft {
            policy: policy_context(context.save_id, context.run_revision, context.policy_set_id)?,
            source: LedgerSource {
                kind: LedgerSourceKind::EmploymentPayroll,
                source_id: settlement_id.to_string(),
            },
            game_day: context.game_day,
            description: "급여 지급".to_owned(),
            postings,
        })
        .context("employment payroll ledger is invalid")?;
    write_ledger_transaction(tx, &ledger).await.map(Some)
}

async fn create_reward_ledger(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    context: LedgerWriteContext,
    contract_id: u64,
    reward: crate::career::OtherIncomeRewardBreakdown,
) -> Result<u64> {
    let mut postings = Vec::with_capacity(4);
    push_posting(
        &mut postings,
        LedgerAccountCode::Wallet,
        reward.net_reward_krw,
    );
    push_posting(
        &mut postings,
        LedgerAccountCode::OtherIncomeReward,
        reward
            .gross_reward_krw
            .checked_neg()
            .context("career reward ledger amount overflowed")?,
    );
    push_posting(
        &mut postings,
        LedgerAccountCode::OtherIncomeTaxWithholding,
        reward.withheld_income_tax_krw,
    );
    push_posting(
        &mut postings,
        LedgerAccountCode::OtherLocalIncomeTaxWithholding,
        reward.withheld_local_income_tax_krw,
    );
    let ledger = finance_rules
        .create_ledger_transaction(LedgerTransactionDraft {
            policy: policy_context(context.save_id, context.run_revision, context.policy_set_id)?,
            source: LedgerSource {
                kind: LedgerSourceKind::CareerRewardPayment,
                source_id: contract_id.to_string(),
            },
            game_day: context.game_day,
            description: "원티드 채용 보상 지급".to_owned(),
            postings,
        })
        .context("career reward ledger is invalid")?;
    write_ledger_transaction(tx, &ledger).await
}

fn push_posting(postings: &mut Vec<LedgerPosting>, account_code: LedgerAccountCode, amount: i64) {
    if amount != 0 {
        postings.push(LedgerPosting {
            account_code,
            financial_account_id: None,
            amount_krw: amount,
        });
    }
}

fn policy_context(save_id: u64, run_revision: u32, policy_set_id: u64) -> Result<RunPolicyContext> {
    Ok(RunPolicyContext {
        run: RunId {
            save_id: resource_id(save_id, "save")?,
            run_revision,
        },
        policy_set_id: resource_id(policy_set_id, "finance policy set")?,
    })
}

async fn insert_payroll_record(
    tx: &mut Transaction<'_, MySql>,
    write: PayrollRecordWrite<'_>,
) -> Result<u64> {
    let PayrollRecordWrite {
        save_id,
        run_revision,
        employment_policy_set_id,
        settlement_id,
        ledger_transaction_id,
        policy,
        withholding_row_id,
        tax_year,
        payday_game_day,
        breakdown,
    } = write;
    let period = breakdown.period;
    let insurance = breakdown.insurance;
    let withholding = breakdown.withholding;
    let employment_assessed = period.gross_pay_krw > 0;
    let result = sqlx::query(
        "INSERT INTO payroll_record
             (save_id, run_revision, employment_contract_id, employment_policy_set_id,
              scheduled_settlement_id, ledger_transaction_id,
              national_pension_policy_id, health_insurance_policy_id,
              long_term_care_policy_id, employment_insurance_policy_id,
              industrial_accident_policy_id, employment_withholding_table_version_id,
              employment_withholding_table_row_id, local_income_withholding_policy_id,
              period_no, salary_month_ordinal, tax_year, period_start_date,
              period_end_exclusive_date, payday, payday_game_day, calendar_days,
              covered_days, base_monthly_salary_krw, gross_pay_krw,
              national_pension_assessed, national_pension_employee_basis_krw,
              national_pension_employer_basis_krw, national_pension_employee_rate_ppm,
              national_pension_employer_rate_ppm,
              national_pension_employee_rounding_unit_krw,
              national_pension_employer_rounding_unit_krw,
              national_pension_employee_krw, national_pension_employer_krw,
              health_insurance_assessed, health_insurance_employee_basis_krw,
              health_insurance_employer_basis_krw, health_insurance_employee_rate_ppm,
              health_insurance_employer_rate_ppm,
              health_insurance_employee_rounding_unit_krw,
              health_insurance_employer_rounding_unit_krw,
              health_insurance_employee_krw, health_insurance_employer_krw,
              long_term_care_assessed, long_term_care_employee_health_basis_krw,
              long_term_care_employer_health_basis_krw, long_term_care_rate_numerator,
              long_term_care_rate_denominator, long_term_care_employee_rounding_unit_krw,
              long_term_care_employer_rounding_unit_krw, long_term_care_employee_krw,
              long_term_care_employer_krw, employment_insurance_assessed,
              employment_insurance_employee_basis_krw,
              employment_insurance_employer_basis_krw,
              employment_insurance_employee_rate_ppm,
              employment_insurance_employer_rate_ppm,
              employment_insurance_employee_rounding_unit_krw,
              employment_insurance_employer_rounding_unit_krw,
              employment_insurance_employee_krw, employment_insurance_employer_krw,
              industrial_accident_assessed, industrial_accident_basis_krw,
              industrial_accident_employer_rate_ppm,
              industrial_accident_employer_rounding_unit_krw,
              industrial_accident_employer_krw, employee_insurance_total_krw,
              employer_insurance_total_krw, withholding_family_count,
              withholding_child_count, withholding_lower_bound_krw,
              withholding_upper_bound_exclusive_krw, withheld_income_tax_krw,
              local_income_tax_basis_krw, local_income_tax_rate_ppm,
              local_income_tax_rounding_unit_krw, withheld_local_income_tax_krw,
              net_salary_pay_krw)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                 ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                 ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                 ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                 ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                 ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                 ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                 ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(period.contract_id)
    .bind(employment_policy_set_id)
    .bind(settlement_id)
    .bind(ledger_transaction_id)
    .bind(policy.national_pension_policy_id)
    .bind(policy.health_insurance_policy_id)
    .bind(policy.long_term_care_policy_id)
    .bind(policy.employment_insurance_policy_id)
    .bind(policy.industrial_accident_policy_id)
    .bind(policy.withholding_version_id)
    .bind(withholding_row_id)
    .bind(policy.local_income_withholding_policy_id)
    .bind(period.period_no)
    .bind(period.salary_month_ordinal)
    .bind(tax_year)
    .bind(period.period_start_date)
    .bind(period.period_end_exclusive_date)
    .bind(period.payday)
    .bind(payday_game_day)
    .bind(period.calendar_days)
    .bind(period.covered_days)
    .bind(period.base_monthly_salary_krw)
    .bind(period.gross_pay_krw)
    .bind(insurance.national_pension.assessed)
    .bind(insurance.national_pension.employee_basis_krw)
    .bind(insurance.national_pension.employer_basis_krw)
    .bind(insurance.national_pension.employee_rate_ppm)
    .bind(insurance.national_pension.employer_rate_ppm)
    .bind(insurance.national_pension.employee_rounding_unit_krw)
    .bind(insurance.national_pension.employer_rounding_unit_krw)
    .bind(insurance.national_pension.employee_amount_krw)
    .bind(insurance.national_pension.employer_amount_krw)
    .bind(insurance.health_insurance.assessed)
    .bind(insurance.health_insurance.employee_basis_krw)
    .bind(insurance.health_insurance.employer_basis_krw)
    .bind(insurance.health_insurance.employee_rate_ppm)
    .bind(insurance.health_insurance.employer_rate_ppm)
    .bind(insurance.health_insurance.employee_rounding_unit_krw)
    .bind(insurance.health_insurance.employer_rounding_unit_krw)
    .bind(insurance.health_insurance.employee_amount_krw)
    .bind(insurance.health_insurance.employer_amount_krw)
    .bind(insurance.long_term_care.assessed)
    .bind(insurance.long_term_care.employee_health_premium_basis_krw)
    .bind(insurance.long_term_care.employer_health_premium_basis_krw)
    .bind(insurance.long_term_care.rate_numerator)
    .bind(insurance.long_term_care.rate_denominator)
    .bind(insurance.long_term_care.employee_rounding_unit_krw)
    .bind(insurance.long_term_care.employer_rounding_unit_krw)
    .bind(insurance.long_term_care.employee_amount_krw)
    .bind(insurance.long_term_care.employer_amount_krw)
    .bind(employment_assessed)
    .bind(insurance.employment_insurance.employee_basis_krw)
    .bind(insurance.employment_insurance.employer_basis_krw)
    .bind(insurance.employment_insurance.employee_rate_ppm)
    .bind(insurance.employment_insurance.employer_rate_ppm)
    .bind(insurance.employment_insurance.employee_rounding_unit_krw)
    .bind(insurance.employment_insurance.employer_rounding_unit_krw)
    .bind(insurance.employment_insurance.employee_amount_krw)
    .bind(insurance.employment_insurance.employer_amount_krw)
    .bind(employment_assessed)
    .bind(insurance.industrial_accident.basis_krw)
    .bind(insurance.industrial_accident.rate_ppm)
    .bind(insurance.industrial_accident.rounding_unit_krw)
    .bind(insurance.industrial_accident.employer_amount_krw)
    .bind(breakdown.employee_insurance_total_krw)
    .bind(breakdown.employer_insurance_total_krw)
    .bind(withholding.family_count)
    .bind(withholding.child_count)
    .bind(withholding.row_lower_bound_krw)
    .bind(withholding.row_upper_bound_exclusive_krw)
    .bind(breakdown.withheld_income_tax_krw)
    .bind(withholding.local_income_tax_basis_krw)
    .bind(withholding.local_income_tax_rate_ppm)
    .bind(withholding.local_income_tax_rounding_unit_krw)
    .bind(breakdown.withheld_local_income_tax_krw)
    .bind(breakdown.net_salary_pay_krw)
    .execute(&mut **tx)
    .await
    .context("failed to insert employment payroll record")?;
    let id = result.last_insert_id();
    ensure!(id != 0, "payroll record insert returned no ID");
    Ok(id)
}

async fn insert_reward_payment(
    tx: &mut Transaction<'_, MySql>,
    write: RewardPaymentWrite,
) -> Result<u64> {
    let RewardPaymentWrite {
        save_id,
        run_revision,
        contract_id,
        employment_policy_set_id,
        payroll_record_id,
        reward_policy_id,
        ledger_transaction_id,
        payment_date,
        payment_game_day,
        reward,
    } = write;
    let result = sqlx::query(
        "INSERT INTO career_reward_payment
             (save_id, run_revision, employment_contract_id, employment_policy_set_id,
              payroll_record_id, other_income_reward_policy_id, ledger_transaction_id,
              payment_date, payment_game_day, gross_reward_krw, income_tax_rate_ppm,
              local_income_tax_rate_ppm, income_tax_rounding_unit_krw,
              local_income_tax_rounding_unit_krw, withheld_income_tax_krw,
              withheld_local_income_tax_krw, net_reward_krw)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract_id)
    .bind(employment_policy_set_id)
    .bind(payroll_record_id)
    .bind(reward_policy_id)
    .bind(ledger_transaction_id)
    .bind(payment_date)
    .bind(payment_game_day)
    .bind(reward.gross_reward_krw)
    .bind(reward.income_tax_rate_ppm)
    .bind(reward.local_income_tax_rate_ppm)
    .bind(reward.income_tax_rounding_unit_krw)
    .bind(reward.local_income_tax_rounding_unit_krw)
    .bind(reward.withheld_income_tax_krw)
    .bind(reward.withheld_local_income_tax_krw)
    .bind(reward.net_reward_krw)
    .execute(&mut **tx)
    .await
    .context("failed to insert career reward payment")?;
    let id = result.last_insert_id();
    ensure!(id != 0, "career reward payment insert returned no ID");
    Ok(id)
}

async fn credit_wallet(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    policy_set_id: u64,
    amount_krw: i64,
) -> Result<()> {
    ensure!(amount_krw >= 0, "payroll wallet credit cannot be negative");
    if amount_krw == 0 {
        return Ok(());
    }
    let cash_krw: i64 = sqlx::query_scalar(
        "SELECT cash_krw FROM save
         WHERE id = ? AND run_revision = ? AND policy_set_id = ? FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(policy_set_id)
    .fetch_one(&mut **tx)
    .await?;
    let next_cash = cash_krw
        .checked_add(amount_krw)
        .context("payroll wallet cash overflowed")?;
    let update = sqlx::query(
        "UPDATE save SET cash_krw = ?
         WHERE id = ? AND run_revision = ? AND policy_set_id = ? AND cash_krw = ?",
    )
    .bind(next_cash)
    .bind(save_id)
    .bind(run_revision)
    .bind(policy_set_id)
    .bind(cash_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(update.rows_affected() == 1, "payroll wallet lost its lock");
    Ok(())
}

async fn insert_payroll_schedule(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    contract_id: u64,
    period: &crate::career::PayrollPeriod,
) -> Result<u64> {
    ensure!(
        period.contract_id == contract_id && period.period_no > 0,
        "payroll schedule belongs to another contract"
    );
    let due_game_day: i64 = sqlx::query_scalar(
        "SELECT DATEDIFF(?, world.start_date)
         FROM save INNER JOIN market_world AS world ON world.id = save.market_world_id
         WHERE save.id = ? AND save.run_revision = ?",
    )
    .bind(period.payday)
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await?
    .context("payroll schedule run is missing")?;
    let due_game_day = u32::try_from(due_game_day)
        .context("payroll schedule date is outside the game-day range")?;
    let payload = serde_json::json!({
        "version": PAYROLL_PAYLOAD_VERSION,
        "employmentContractId": resource_id(contract_id, "employment contract")?,
        "periodNo": period.period_no,
    });
    let payload_json = serde_json::to_string(&payload)?;
    let existing: Option<EmploymentSettlementRow> = sqlx::query_as(
        "SELECT id, due_game_day, kind, CAST(payload AS CHAR) AS payload_json,
                source_kind, source_id, occurrence, status
         FROM scheduled_settlement
         WHERE save_id = ? AND run_revision = ?
           AND BINARY source_kind = BINARY ? AND BINARY source_id = BINARY ?
           AND occurrence = ? FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(PAYROLL_SETTLEMENT_SOURCE_KIND)
    .bind(contract_id.to_string())
    .bind(period.period_no)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(existing) = existing {
        ensure!(
            existing.due_game_day == due_game_day
                && existing.kind == PAYROLL_SETTLEMENT_KIND
                && existing.source_kind == PAYROLL_SETTLEMENT_SOURCE_KIND
                && existing.source_id == contract_id.to_string()
                && existing.occurrence == period.period_no
                && decode_payroll_settlement(&existing)?.period_no == period.period_no,
            "existing employment payroll schedule conflicts with the canonical period"
        );
        return Ok(existing.id);
    }
    let result = sqlx::query(
        "INSERT INTO scheduled_settlement
             (save_id, run_revision, due_game_day, kind, payload,
              source_kind, source_id, occurrence, status)
         VALUES (?, ?, ?, ?, CAST(? AS JSON), ?, ?, ?, 'pending')",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(due_game_day)
    .bind(PAYROLL_SETTLEMENT_KIND)
    .bind(payload_json)
    .bind(PAYROLL_SETTLEMENT_SOURCE_KIND)
    .bind(contract_id.to_string())
    .bind(period.period_no)
    .execute(&mut **tx)
    .await
    .context("failed to schedule employment payroll")?;
    let id = result.last_insert_id();
    ensure!(id != 0, "employment payroll schedule insert returned no ID");
    Ok(id)
}

async fn transition_payroll_settlement(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    settlement_id: u64,
    ledger_transaction_id: Option<u64>,
    gross_pay_krw: i64,
) -> Result<()> {
    let update = if gross_pay_krw == 0 {
        ensure!(
            ledger_transaction_id.is_none(),
            "zero payroll unexpectedly created a salary ledger"
        );
        sqlx::query(
            "UPDATE scheduled_settlement
             SET status = 'settled', outcome = 'noMovement', outcome_reason = 'zeroGrossPay'
             WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'pending'",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(settlement_id)
        .execute(&mut **tx)
        .await?
    } else {
        sqlx::query(
            "UPDATE scheduled_settlement
             SET status = 'settled', outcome = 'applied',
                 settled_ledger_transaction_id = ?
             WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'pending'",
        )
        .bind(ledger_transaction_id.context("paid payroll has no salary ledger")?)
        .bind(save_id)
        .bind(run_revision)
        .bind(settlement_id)
        .execute(&mut **tx)
        .await?
    };
    ensure!(
        update.rows_affected() == 1,
        "employment payroll settlement transition lost its lock"
    );
    Ok(())
}

fn enum_from_db<T: DeserializeOwned>(raw: &str) -> Result<T> {
    serde_json::from_value(Value::String(raw.to_owned()))
        .with_context(|| format!("unknown stored employment enum value: {raw}"))
}

fn resource_id(value: u64, label: &str) -> Result<ResourceId> {
    ensure!(value != 0, "stored {label} ID is zero");
    Ok(ResourceId::from_u64(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finance::{RunId, SettlementSource, SettlementStatus};

    fn given_payroll_settlement(payload: Value) -> ScheduledSettlement {
        ScheduledSettlement {
            id: ResourceId::from_u64(11),
            run: RunId {
                save_id: ResourceId::from_u64(7),
                run_revision: 3,
            },
            due_game_day: 40,
            kind: SettlementKind::EmploymentPayroll,
            source: SettlementSource {
                kind: SettlementSourceKind::EmploymentContract,
                source_id: "13".to_owned(),
                occurrence: 2,
            },
            status: SettlementStatus::Pending,
            payload,
        }
    }

    mod context_급여_settlement_payload를_해석하는_경우 {
        use super::*;

        #[test]
        fn given_exact_payload_when_검증하면_then_source_identity와_함께_허용한다() {
            let settlement = given_payroll_settlement(serde_json::json!({
                "version": 1,
                "employmentContractId": "13",
                "periodNo": 2
            }));

            let result = validate_employment_settlement_envelope(&settlement);

            assert!(result.is_ok());
        }

        #[test]
        fn given_unknown_field_when_검증하면_then_payload를_거절한다() {
            let settlement = given_payroll_settlement(serde_json::json!({
                "version": 1,
                "employmentContractId": "13",
                "periodNo": 2,
                "grossPayKrw": 1
            }));

            let result = validate_employment_settlement_envelope(&settlement);

            assert!(result.is_err());
        }

        #[test]
        fn given_unknown_version_when_검증하면_then_payload를_거절한다() {
            let settlement = given_payroll_settlement(serde_json::json!({
                "version": 2,
                "employmentContractId": "13",
                "periodNo": 2
            }));

            let result = validate_employment_settlement_envelope(&settlement);

            assert!(result.is_err());
        }

        #[test]
        fn given_source_id가_payload와_다를때_when_검증하면_then_identity를_거절한다() {
            let mut settlement = given_payroll_settlement(serde_json::json!({
                "version": 1,
                "employmentContractId": "13",
                "periodNo": 2
            }));
            settlement.source.source_id = "14".to_owned();

            let result = validate_employment_settlement_envelope(&settlement);

            assert!(result.is_err());
        }

        #[test]
        fn given_occurrence가_period와_다를때_when_검증하면_then_identity를_거절한다() {
            let mut settlement = given_payroll_settlement(serde_json::json!({
                "version": 1,
                "employmentContractId": "13",
                "periodNo": 2
            }));
            settlement.source.occurrence = 3;

            let result = validate_employment_settlement_envelope(&settlement);

            assert!(result.is_err());
        }
    }
}
