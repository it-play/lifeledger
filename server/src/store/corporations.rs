//! M4-E2a corporation establishment and separate-ledger persistence.

use anyhow::{Context, Result, bail, ensure};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{AssertSqlSafe, MySql, MySqlPool, Transaction};
use time::{Date, Duration};

use super::annual_tax::{AnnualTaxRunContext, accrue_financial_income_source};
use super::employment::{
    CorporationEmploymentPayrollInput, calculate_corporation_employment_payroll_in_tx,
};
use super::employment_income::{
    EmploymentIncomeAmounts, EmploymentIncomeEventSource, EmploymentIncomeEventWrite,
    record_employment_income_event_in_tx,
};
use super::housing::is_retryable_database_error;
use super::life::read_tax_dependent_count_in_tx;
use super::mysql::{
    CommandIdentitySpec, CommandIdentityState, inspect_command_identity, read_state,
    write_command_identity,
};
use super::types::{
    CorporationAvailabilityState, CorporationDividendReceipt, CorporationNextMonthSettingState,
    CorporationOperatingMonthPageState, CorporationOperatingMonthState,
    CorporationOperatingScaleState, CorporationOperatingSettingState, CorporationReadResult,
    CorporationReceipt, CorporationSettingsReceipt, CorporationSnapshotState,
    CorporationStatusState, CorporationSummaryState, CorporationTemplateState,
    CorporationTemplatesState, CreateCorporationCommand, LifeFailureCode, LifeStoreResult,
    PayCorporationDividendCommand, UpdateCorporationSettingsCommand,
};
use crate::career::{Industry, PayrollBreakdown, PayrollRules};
use crate::finance::{
    FinanceRules, FinancialIncomeAccrual, FinancialIncomeSource, LedgerAccountCode, LedgerPosting,
    LedgerSource, LedgerSourceKind, LedgerTransaction, LedgerTransactionDraft, ResourceId, RunId,
    RunPolicyContext,
};
use crate::life::{
    CorporationDividendInput, CorporationError, CorporationEstablishmentInput,
    CorporationEstablishmentTerms, CorporationOfficerPayrollInput, CorporationOfficerPayrollPlan,
    CorporationOperatingMonthInput, CorporationOperatingMonthPlan, CorporationOperatingScaleTerms,
    CorporationRegisteredOfficeClass, CorporationRegistrationPolicy, CorporationRules,
    CorporationTaxBracket, CorporationTaxInput, CorporationTaxPolicy,
};

const COMPONENT_KEY: &str = "dev-unranked-m4-corporation-2026-v1";
const POLICY_KEY: &str = "dev-unranked-kr-corporation-2026-v5";
const COMMAND_KIND_CREATE: &str = "createCorporation";
const COMMAND_KIND_UPDATE_SETTINGS: &str = "updateCorporationSettings";
const COMMAND_KIND_PAY_DIVIDEND: &str = "payCorporationDividend";
const MONTH_CURSOR_DOMAIN: &[u8] = b"lifeledger.corporation.month.cursor.v1";
const MONTH_CURSOR_VERSION: u8 = 1;
const MAX_TRANSACTION_ATTEMPTS: usize = 3;

#[derive(Debug, Clone, sqlx::FromRow)]
struct ScopeRow {
    save_id: u64,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
    current_date: Date,
    cash_krw: i64,
    representative_name: Option<String>,
    life_catalog_set_id: Option<u64>,
    policy_set_id: Option<u64>,
    policy_key: Option<String>,
    corporation_component_version_id: Option<u64>,
    component_version_key: Option<String>,
    component_availability: Option<String>,
    registered_office_class: Option<String>,
    minimum_capital_krw: Option<i64>,
    maximum_capital_krw: Option<i64>,
    game_administrative_fee_krw: Option<i64>,
    registration_policy_rule_id: Option<u64>,
    registration_license_tax_rate_ppm: Option<i64>,
    minimum_registration_license_tax_krw: Option<i64>,
    local_education_tax_rate_ppm: Option<i64>,
    corporation_policy_rule_count: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct TemplateRow {
    id: u64,
    template_key: String,
    display_name: String,
    template_order: u8,
    base_monthly_revenue_krw: i64,
    revenue_variation_ppm: u32,
    variable_cost_ppm: u32,
    fixed_monthly_cost_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ScaleRow {
    id: u64,
    industry_template_id: u64,
    scale_key: String,
    scale_order: u8,
    revenue_factor_ppm: u32,
    fixed_cost_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct SettingRow {
    id: u64,
    corporation_id: u64,
    operating_scale_id: u64,
    scale_key: String,
    scale_order: u8,
    revenue_factor_ppm: u32,
    fixed_cost_krw: i64,
    effective_year: u16,
    effective_month: u8,
    officer_gross_salary_krw: i64,
    created_game_day: u32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct SettingReceiptRow {
    command_kind: String,
    payload_sha256: String,
    result_json: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CorporationRow {
    id: u64,
    corporation_component_version_id: u64,
    industry_template_id: u64,
    template_key: String,
    template_display_name: String,
    name: String,
    representative_name: String,
    status: String,
    established_game_day: u32,
    capital_krw: i64,
    registration_license_tax_krw: i64,
    local_education_tax_krw: i64,
    game_administrative_fee_krw: i64,
    total_establishment_fee_krw: i64,
    cash_krw: i64,
    contributed_capital_krw: i64,
    retained_earnings_krw: i64,
    operating_payable_krw: i64,
    corporate_tax_payable_krw: i64,
    distributable_profit_krw: i64,
    personal_ledger_transaction_id: Option<u64>,
    corporation_ledger_transaction_id: Option<u64>,
    next_setting_id: Option<u64>,
    next_operating_scale_id: u64,
    next_scale_key: String,
    next_scale_order: u8,
    next_revenue_factor_ppm: u32,
    next_fixed_cost_krw: i64,
    next_effective_year: u16,
    next_effective_month: u8,
    next_officer_gross_salary_krw: i64,
    next_setting_created_game_day: Option<u32>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ReceiptRow {
    command_kind: String,
    payload_sha256: String,
    result_json: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct OperatingCorporationRow {
    id: u64,
    corporation_component_version_id: u64,
    industry_template_id: u64,
    template_key: String,
    established_date: Date,
    world_seed: u64,
    finance_policy_set_id: u64,
    employment_policy_set_id: u64,
    personal_cash_krw: i64,
    cash_krw: i64,
    operating_payable_krw: i64,
    retained_earnings_krw: i64,
    base_monthly_revenue_krw: i64,
    revenue_variation_ppm: u32,
    variable_cost_ppm: u32,
    fixed_monthly_cost_krw: i64,
    operating_scale_id: u64,
    scale_revenue_factor_ppm: u32,
    scale_fixed_cost_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct SelectedOperatingSettingRow {
    operating_setting_id: u64,
    operating_scale_id: u64,
    scale_revenue_factor_ppm: u32,
    scale_fixed_cost_krw: i64,
    officer_gross_salary_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CorporationTaxCloseRow {
    id: u64,
    policy_set_id: u64,
    retained_earnings_krw: i64,
    corporate_tax_payable_krw: i64,
    status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CorporationTaxBracketRow {
    policy_rule_id: u64,
    tax_kind: String,
    bracket_order: u8,
    maximum_tax_base_krw: Option<i64>,
    rate_ppm: u32,
    progressive_deduction_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CorporationDividendPolicyRow {
    policy_rule_id: u64,
    income_tax_rate_ppm: i64,
    local_income_tax_on_income_tax_ppm: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CorporationMonthRow {
    id: u64,
    operating_year: u16,
    operating_month: u8,
    scale_key: String,
    officer_gross_salary_krw: i64,
    revenue_krw: i64,
    operating_expense_krw: i64,
    total_payroll_cost_krw: i64,
    pre_tax_profit_krw: i64,
    payroll_status: String,
    cash_after_krw: i64,
    operating_payable_after_krw: i64,
    retained_earnings_after_krw: i64,
    applied_game_day: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CorporationMonthCursor {
    save_id: u64,
    run_revision: u32,
    corporation_id: u64,
    operating_year: u16,
    operating_month: u8,
    id: u64,
}

#[derive(Debug, Clone, Copy)]
enum EstablishmentTransition {
    Draft,
    Active,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CorporationOperatingSettlementContext {
    pub save_id: u64,
    pub run_revision: u32,
    pub target_game_day: u32,
    pub market_date: Date,
}

pub(super) async fn read_corporation_templates(
    pool: &MySqlPool,
    user_id: u64,
) -> Result<CorporationReadResult<CorporationTemplatesState>> {
    let mut tx = pool.begin().await?;
    let Some(scope) = read_scope_for_user(&mut tx, user_id, false).await? else {
        tx.commit().await?;
        return Ok(CorporationReadResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    if scope.representative_name.is_none() {
        tx.commit().await?;
        return Ok(CorporationReadResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    }
    let state = read_templates_for_scope(&mut tx, &scope).await?;
    tx.commit().await?;
    Ok(CorporationReadResult::Found(state))
}

pub(super) async fn read_corporation_detail(
    pool: &MySqlPool,
    user_id: u64,
    corporation_id: ResourceId,
) -> Result<CorporationReadResult<CorporationSummaryState>> {
    let mut tx = pool.begin().await?;
    let Some(scope) = read_scope_for_user(&mut tx, user_id, false).await? else {
        tx.commit().await?;
        return Ok(CorporationReadResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    if scope.representative_name.is_none() {
        tx.commit().await?;
        return Ok(CorporationReadResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    }
    let Some(row) = read_corporation_by_id(&mut tx, &scope, corporation_id.get(), false).await?
    else {
        tx.commit().await?;
        return Ok(CorporationReadResult::Rejected(
            LifeFailureCode::CorporationResourceNotFound,
        ));
    };
    let summary = corporation_summary(&row)?;
    tx.commit().await?;
    Ok(CorporationReadResult::Found(summary))
}

pub(super) async fn read_corporation_operating_months(
    pool: &MySqlPool,
    user_id: u64,
    corporation_id: ResourceId,
    cursor: Option<String>,
) -> Result<CorporationReadResult<CorporationOperatingMonthPageState>> {
    let mut tx = pool.begin().await?;
    let Some(scope) = read_scope_for_user(&mut tx, user_id, false).await? else {
        tx.commit().await?;
        return Ok(CorporationReadResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    if scope.representative_name.is_none() {
        tx.commit().await?;
        return Ok(CorporationReadResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    }
    if read_corporation_by_id(&mut tx, &scope, corporation_id.get(), false)
        .await?
        .is_none()
    {
        tx.commit().await?;
        return Ok(CorporationReadResult::Rejected(
            LifeFailureCode::CorporationResourceNotFound,
        ));
    }
    let after = match cursor.as_deref().map(decode_month_cursor).transpose() {
        Ok(after) => after,
        Err(_) => {
            tx.commit().await?;
            return Ok(CorporationReadResult::Rejected(
                LifeFailureCode::InvalidCommand,
            ));
        }
    };
    if after.is_some_and(|after| {
        after.save_id != scope.save_id
            || after.run_revision != scope.run_revision
            || after.corporation_id != corporation_id.get()
    }) {
        tx.commit().await?;
        return Ok(CorporationReadResult::Rejected(
            LifeFailureCode::InvalidCommand,
        ));
    }
    let (after_year, after_month, after_id) = after.map_or((0_u16, 0_u8, 0_u64), |value| {
        (value.operating_year, value.operating_month, value.id)
    });
    let mut rows: Vec<CorporationMonthRow> = sqlx::query_as(
        "SELECT operating_month.id, operating_month.operating_year,
                operating_month.operating_month, scale.scale_key,
                operating_month.officer_gross_salary_krw,
                operating_month.revenue_krw, operating_month.operating_expense_krw,
                operating_month.total_payroll_cost_krw, operating_month.pre_tax_profit_krw,
                operating_month.payroll_status, operating_month.cash_after_krw,
                operating_month.operating_payable_after_krw,
                operating_month.retained_earnings_after_krw,
                operating_month.applied_game_day
         FROM corporation_operating_month AS operating_month
         INNER JOIN corporation_operating_scale AS scale
           ON scale.id = operating_month.operating_scale_id
         WHERE operating_month.save_id = ? AND operating_month.run_revision = ?
           AND operating_month.corporation_id = ? AND operating_month.status = 'applied'
           AND (operating_month.operating_year > ?
                OR (operating_month.operating_year = ?
                    AND operating_month.operating_month > ?)
                OR (operating_month.operating_year = ?
                    AND operating_month.operating_month = ? AND operating_month.id > ?))
         ORDER BY operating_month.operating_year, operating_month.operating_month,
                  operating_month.id
         LIMIT 21",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(corporation_id.get())
    .bind(after_year)
    .bind(after_year)
    .bind(after_month)
    .bind(after_year)
    .bind(after_month)
    .bind(after_id)
    .fetch_all(&mut *tx)
    .await?;
    let has_more = rows.len() > 20;
    if has_more {
        rows.pop();
    }
    let next_cursor = if has_more {
        rows.last().map(|row| {
            encode_month_cursor(CorporationMonthCursor {
                save_id: scope.save_id,
                run_revision: scope.run_revision,
                corporation_id: corporation_id.get(),
                operating_year: row.operating_year,
                operating_month: row.operating_month,
                id: row.id,
            })
        })
    } else {
        None
    };
    let months = rows
        .into_iter()
        .map(corporation_month_state)
        .collect::<Result<Vec<_>>>()?;
    tx.commit().await?;
    Ok(CorporationReadResult::Found(
        CorporationOperatingMonthPageState {
            months,
            next_cursor,
        },
    ))
}

fn corporation_month_state(row: CorporationMonthRow) -> Result<CorporationOperatingMonthState> {
    ensure!(
        (1..=12).contains(&row.operating_month)
            && matches!(
                row.payroll_status.as_str(),
                "notConfigured" | "paid" | "unpaid"
            ),
        "corporation operating month projection is invalid"
    );
    Ok(CorporationOperatingMonthState {
        id: ResourceId::from_u64(row.id),
        operating_year: row.operating_year,
        operating_month: row.operating_month,
        scale_key: row.scale_key,
        officer_gross_salary_krw: row.officer_gross_salary_krw,
        revenue_krw: row.revenue_krw,
        operating_expense_krw: row.operating_expense_krw,
        total_payroll_cost_krw: row.total_payroll_cost_krw,
        pre_tax_profit_krw: row.pre_tax_profit_krw,
        payroll_status: row.payroll_status,
        cash_after_krw: row.cash_after_krw,
        operating_payable_after_krw: row.operating_payable_after_krw,
        retained_earnings_after_krw: row.retained_earnings_after_krw,
        applied_game_day: row.applied_game_day,
    })
}

fn encode_month_cursor(cursor: CorporationMonthCursor) -> String {
    let mut payload = Vec::with_capacity(48);
    payload.push(MONTH_CURSOR_VERSION);
    payload.extend_from_slice(&cursor.save_id.to_be_bytes());
    payload.extend_from_slice(&cursor.run_revision.to_be_bytes());
    payload.extend_from_slice(&cursor.corporation_id.to_be_bytes());
    payload.extend_from_slice(&cursor.operating_year.to_be_bytes());
    payload.push(cursor.operating_month);
    payload.extend_from_slice(&cursor.id.to_be_bytes());
    let checksum = month_cursor_checksum(&payload);
    payload.extend_from_slice(&checksum[..16]);
    URL_SAFE_NO_PAD.encode(payload)
}

fn decode_month_cursor(raw: &str) -> Result<CorporationMonthCursor> {
    ensure!(!raw.is_empty() && raw.len() <= 512 && raw.is_ascii());
    let decoded = URL_SAFE_NO_PAD.decode(raw)?;
    ensure!(decoded.len() == 48);
    let (payload, checksum) = decoded.split_at(32);
    ensure!(checksum == &month_cursor_checksum(payload)[..16]);
    let cursor = CorporationMonthCursor {
        save_id: u64::from_be_bytes(payload[1..9].try_into()?),
        run_revision: u32::from_be_bytes(payload[9..13].try_into()?),
        corporation_id: u64::from_be_bytes(payload[13..21].try_into()?),
        operating_year: u16::from_be_bytes(payload[21..23].try_into()?),
        operating_month: payload[23],
        id: u64::from_be_bytes(payload[24..32].try_into()?),
    };
    ensure!(
        payload[0] == MONTH_CURSOR_VERSION
            && cursor.save_id > 0
            && cursor.corporation_id > 0
            && cursor.operating_year > 0
            && (1..=12).contains(&cursor.operating_month)
            && cursor.id > 0
            && encode_month_cursor(cursor) == raw
    );
    Ok(cursor)
}

fn month_cursor_checksum(payload: &[u8]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(MONTH_CURSOR_DOMAIN);
    digest.update(payload);
    digest.finalize().into()
}

pub(super) async fn read_corporation_snapshot_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
) -> Result<CorporationSnapshotState> {
    let Some(scope) = read_scope_for_save(tx, save_id).await? else {
        return Ok(CorporationSnapshotState::unavailable());
    };
    if !scope_is_available(&scope) {
        return Ok(CorporationSnapshotState::unavailable());
    }
    let current = read_current_corporation(tx, &scope, false)
        .await?
        .map(|row| corporation_summary(&row))
        .transpose()?;
    Ok(CorporationSnapshotState {
        availability: CorporationAvailabilityState::Active,
        current,
    })
}

pub(super) async fn settle_corporation_tax_year_in_tx(
    tx: &mut Transaction<'_, MySql>,
    corporation_rules: &dyn CorporationRules,
    save_id: u64,
    run_revision: u32,
    target_game_day: u32,
    market_date: Date,
) -> Result<()> {
    if market_date.month() != time::Month::January || market_date.day() != 1 {
        return Ok(());
    }
    let tax_year = market_date
        .year()
        .checked_sub(1)
        .context("corporation tax year underflowed")?;
    let corporation: Option<CorporationTaxCloseRow> = sqlx::query_as(
        "SELECT id, policy_set_id, retained_earnings_krw,
                corporate_tax_payable_krw, status
         FROM corporation
         WHERE save_id = ? AND run_revision = ?
           AND status IN ('active', 'insolvent')
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(corporation) = corporation else {
        return Ok(());
    };
    let existing_status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM corporation_tax_year
         WHERE save_id = ? AND run_revision = ? AND corporation_id = ? AND tax_year = ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(corporation.id)
    .bind(tax_year)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(status) = existing_status {
        ensure!(status == "applied", "corporation tax year is incomplete");
        return Ok(());
    }
    let annual_pre_tax_profit_krw: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(pre_tax_profit_krw), 0)
         FROM corporation_operating_month
         WHERE save_id = ? AND run_revision = ? AND corporation_id = ?
           AND operating_year = ? AND status = 'applied'",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(corporation.id)
    .bind(tax_year)
    .fetch_one(&mut **tx)
    .await?;
    let bracket_rows: Vec<CorporationTaxBracketRow> = sqlx::query_as(
        "SELECT bracket.policy_rule_id, bracket.tax_kind, bracket.bracket_order,
                bracket.maximum_tax_base_krw, bracket.rate_ppm,
                bracket.progressive_deduction_krw
         FROM policy_rule AS rule
         INNER JOIN corporation_tax_policy_bracket AS bracket
           ON bracket.policy_rule_id = rule.id
         WHERE rule.policy_set_id = ? AND rule.domain = 'corporation'
           AND rule.rule_key = 'corporateIncomeTax'
           AND rule.effective_from <= ?
           AND (rule.effective_to IS NULL OR rule.effective_to >= ?)
         ORDER BY FIELD(bracket.tax_kind, 'national', 'local'), bracket.bracket_order",
    )
    .bind(corporation.policy_set_id)
    .bind(market_date)
    .bind(market_date)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        bracket_rows.len() == 8,
        "corporation tax policy is incomplete"
    );
    let policy_rule_id = bracket_rows[0].policy_rule_id;
    ensure!(
        bracket_rows
            .iter()
            .all(|row| row.policy_rule_id == policy_rule_id),
        "corporation tax brackets cross policy rules"
    );
    let national = tax_brackets(&bracket_rows, "national")?;
    let local = tax_brackets(&bracket_rows, "local")?;
    let plan = corporation_rules
        .plan_corporate_tax(CorporationTaxInput {
            annual_pre_tax_profit_krw,
            policy: CorporationTaxPolicy {
                national_brackets: &national,
                local_brackets: &local,
            },
        })
        .context("corporation tax calculation failed")?;
    let retained_earnings_after_krw = corporation
        .retained_earnings_krw
        .checked_sub(plan.total_tax_krw)
        .context("corporation retained earnings overflowed at year close")?;
    let corporate_tax_payable_after_krw = corporation
        .corporate_tax_payable_krw
        .checked_add(plan.total_tax_krw)
        .context("corporation tax payable overflowed")?;
    let distributable_profit_after_krw = retained_earnings_after_krw.max(0);
    let inserted = sqlx::query(
        "INSERT INTO corporation_tax_year
             (save_id, run_revision, corporation_id, policy_rule_id, tax_year,
              annual_pre_tax_profit_krw, tax_base_krw, corporate_income_tax_krw,
              local_corporate_income_tax_krw, total_tax_krw,
              retained_earnings_before_krw, retained_earnings_after_krw,
              corporate_tax_payable_before_krw, corporate_tax_payable_after_krw,
              distributable_profit_after_krw, ledger_transaction_id,
              applied_game_day, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, 'preparing')",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(corporation.id)
    .bind(policy_rule_id)
    .bind(tax_year)
    .bind(annual_pre_tax_profit_krw)
    .bind(plan.tax_base_krw)
    .bind(plan.corporate_income_tax_krw)
    .bind(plan.local_corporate_income_tax_krw)
    .bind(plan.total_tax_krw)
    .bind(corporation.retained_earnings_krw)
    .bind(retained_earnings_after_krw)
    .bind(corporation.corporate_tax_payable_krw)
    .bind(corporate_tax_payable_after_krw)
    .bind(distributable_profit_after_krw)
    .bind(target_game_day)
    .execute(&mut **tx)
    .await?;
    let tax_year_id = inserted.last_insert_id();
    let ledger_transaction_id = if plan.total_tax_krw > 0 {
        Some(
            write_tax_corporation_ledger(
                tx,
                save_id,
                run_revision,
                corporation.id,
                tax_year_id,
                target_game_day,
                plan.total_tax_krw,
            )
            .await?,
        )
    } else {
        None
    };
    let updated = sqlx::query(
        "UPDATE corporation
         SET retained_earnings_krw = ?, corporate_tax_payable_krw = ?,
             distributable_profit_krw = ?
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = ?
           AND retained_earnings_krw = ? AND corporate_tax_payable_krw = ?",
    )
    .bind(retained_earnings_after_krw)
    .bind(corporate_tax_payable_after_krw)
    .bind(distributable_profit_after_krw)
    .bind(corporation.id)
    .bind(save_id)
    .bind(run_revision)
    .bind(&corporation.status)
    .bind(corporation.retained_earnings_krw)
    .bind(corporation.corporate_tax_payable_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "corporation changed during year close"
    );
    let applied = sqlx::query(
        "UPDATE corporation_tax_year
         SET status = 'applied', ledger_transaction_id = ?, applied_at = CURRENT_TIMESTAMP(3)
         WHERE id = ? AND status = 'preparing' AND ledger_transaction_id IS NULL",
    )
    .bind(ledger_transaction_id)
    .bind(tax_year_id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        applied.rows_affected() == 1,
        "corporation tax year apply failed"
    );
    Ok(())
}

fn tax_brackets(
    rows: &[CorporationTaxBracketRow],
    tax_kind: &str,
) -> Result<Vec<CorporationTaxBracket>> {
    let selected = rows
        .iter()
        .filter(|row| row.tax_kind == tax_kind)
        .collect::<Vec<_>>();
    ensure!(
        selected.len() == 4,
        "corporation tax bracket kind is incomplete"
    );
    selected
        .iter()
        .enumerate()
        .map(|(index, row)| {
            ensure!(
                usize::from(row.bracket_order) == index + 1,
                "corporation tax brackets are not canonical"
            );
            Ok(CorporationTaxBracket {
                maximum_tax_base_krw: row.maximum_tax_base_krw,
                rate_ppm: i64::from(row.rate_ppm),
                progressive_deduction_krw: row.progressive_deduction_krw,
            })
        })
        .collect()
}

async fn write_tax_corporation_ledger(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    corporation_id: u64,
    tax_year_id: u64,
    game_day: u32,
    total_tax_krw: i64,
) -> Result<u64> {
    let inserted = sqlx::query(
        "INSERT INTO corporation_ledger_transaction
             (save_id, run_revision, corporation_id, game_day, transaction_kind,
              correlation_id, operating_month_id, corporation_tax_year_id,
              corporation_dividend_id, description)
         VALUES (?, ?, ?, ?, 'corporateTax', NULL, NULL, ?, NULL, '법인세 결산')",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(corporation_id)
    .bind(game_day)
    .bind(tax_year_id)
    .execute(&mut **tx)
    .await?;
    let ledger_id = inserted.last_insert_id();
    for (posting_order, account_code, amount_krw) in [
        (1_u16, "corporateTaxExpense", total_tax_krw),
        (2_u16, "corporateTaxPayable", -total_tax_krw),
    ] {
        sqlx::query(
            "INSERT INTO corporation_ledger_posting
                 (save_id, run_revision, corporation_id,
                  corporation_ledger_transaction_id, posting_order, account_code, amount_krw)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(corporation_id)
        .bind(ledger_id)
        .bind(posting_order)
        .bind(account_code)
        .bind(amount_krw)
        .execute(&mut **tx)
        .await?;
    }
    Ok(ledger_id)
}

pub(super) async fn settle_corporation_operating_month_in_tx(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    payroll_rules: &dyn PayrollRules,
    corporation_rules: &dyn CorporationRules,
    context: CorporationOperatingSettlementContext,
) -> Result<()> {
    let CorporationOperatingSettlementContext {
        save_id,
        run_revision,
        target_game_day,
        market_date,
    } = context;
    if market_date.day() != 1 {
        return Ok(());
    }
    let corporation: Option<OperatingCorporationRow> = sqlx::query_as(
        "SELECT corporation_row.id,
                corporation_row.corporation_component_version_id,
                corporation_row.industry_template_id, template.template_key,
                DATE_ADD(world.start_date, INTERVAL corporation_row.established_game_day DAY)
                    AS established_date,
                world.seed AS world_seed, bundle.policy_set_id AS finance_policy_set_id,
                bundle.employment_policy_set_id, save.cash_krw AS personal_cash_krw,
                corporation_row.cash_krw,
                corporation_row.operating_payable_krw,
                corporation_row.retained_earnings_krw,
                template.base_monthly_revenue_krw, template.revenue_variation_ppm,
                template.variable_cost_ppm, template.fixed_monthly_cost_krw,
                scale.id AS operating_scale_id,
                scale.revenue_factor_ppm AS scale_revenue_factor_ppm,
                scale.fixed_cost_krw AS scale_fixed_cost_krw
         FROM corporation AS corporation_row
         INNER JOIN save ON save.id = corporation_row.save_id
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         INNER JOIN run_rule_bundle AS bundle
           ON bundle.save_id = save.id AND bundle.run_revision = save.run_revision
         INNER JOIN corporation_industry_template AS template
           ON template.life_component_version_id
                = corporation_row.corporation_component_version_id
          AND template.id = corporation_row.industry_template_id
         INNER JOIN corporation_operating_scale AS scale
           ON scale.life_component_version_id
                = corporation_row.corporation_component_version_id
          AND scale.industry_template_id = corporation_row.industry_template_id
          AND scale.scale_key = 'standard'
         WHERE corporation_row.save_id = ? AND corporation_row.run_revision = ?
           AND corporation_row.status = 'active'
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(corporation) = corporation else {
        return Ok(());
    };
    if (
        corporation.established_date.year(),
        corporation.established_date.month(),
    ) >= (market_date.year(), market_date.month())
    {
        return Ok(());
    }
    let operating_year = market_date.year();
    let operating_month = u8::from(market_date.month());
    let existing_status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM corporation_operating_month
         WHERE save_id = ? AND run_revision = ? AND corporation_id = ?
           AND operating_year = ? AND operating_month = ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(corporation.id)
    .bind(operating_year)
    .bind(operating_month)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(status) = existing_status {
        ensure!(
            status == "applied",
            "corporation operating month is incomplete"
        );
        return Ok(());
    }

    let selected_setting: Option<SelectedOperatingSettingRow> = sqlx::query_as(
        "SELECT setting_row.id AS operating_setting_id,
                scale.id AS operating_scale_id,
                scale.revenue_factor_ppm AS scale_revenue_factor_ppm,
                scale.fixed_cost_krw AS scale_fixed_cost_krw,
                setting_row.officer_gross_salary_krw
         FROM corporation_operating_setting AS setting_row
         INNER JOIN corporation_operating_scale AS scale
           ON scale.id = setting_row.operating_scale_id
          AND scale.life_component_version_id
                = setting_row.corporation_component_version_id
          AND scale.industry_template_id = setting_row.industry_template_id
         WHERE setting_row.save_id = ? AND setting_row.run_revision = ?
           AND setting_row.corporation_id = ?
           AND (setting_row.effective_year < ?
                OR (setting_row.effective_year = ? AND setting_row.effective_month <= ?))
         ORDER BY setting_row.effective_year DESC, setting_row.effective_month DESC,
                  setting_row.id DESC
         LIMIT 1",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(corporation.id)
    .bind(operating_year)
    .bind(operating_year)
    .bind(operating_month)
    .fetch_optional(&mut **tx)
    .await?;
    let (
        operating_setting_id,
        operating_scale_id,
        scale_revenue_factor_ppm,
        scale_fixed_cost_krw,
        officer_gross_salary_krw,
    ) = selected_setting.map_or(
        (
            None,
            corporation.operating_scale_id,
            corporation.scale_revenue_factor_ppm,
            corporation.scale_fixed_cost_krw,
            0,
        ),
        |setting| {
            (
                Some(setting.operating_setting_id),
                setting.operating_scale_id,
                setting.scale_revenue_factor_ppm,
                setting.scale_fixed_cost_krw,
                setting.officer_gross_salary_krw,
            )
        },
    );
    let plan = corporation_rules
        .plan_operating_month(CorporationOperatingMonthInput {
            world_seed: corporation.world_seed,
            corporation_id: ResourceId::from_u64(corporation.id),
            operating_year,
            operating_month,
            stream: 0,
            base_monthly_revenue_krw: corporation.base_monthly_revenue_krw,
            revenue_variation_ppm: i64::from(corporation.revenue_variation_ppm),
            variable_cost_ppm: i64::from(corporation.variable_cost_ppm),
            fixed_monthly_cost_krw: corporation.fixed_monthly_cost_krw,
            scale: CorporationOperatingScaleTerms {
                revenue_factor_ppm: i64::from(scale_revenue_factor_ppm),
                fixed_cost_krw: scale_fixed_cost_krw,
            },
        })
        .context("corporation operating month calculation failed")?;
    let cash_available_krw = corporation
        .cash_krw
        .checked_add(plan.revenue_krw)
        .context("corporation operating cash overflowed")?;
    let operating_cost_cash_paid_krw = cash_available_krw.min(plan.operating_expense_krw);
    let operating_cost_payable_krw = plan
        .operating_expense_krw
        .checked_sub(operating_cost_cash_paid_krw)
        .context("corporation operating payable underflowed")?;
    let operating_cash_after_krw = cash_available_krw
        .checked_sub(operating_cost_cash_paid_krw)
        .context("corporation operating cash underflowed")?;
    let employment_industry = employment_industry(&corporation.template_key)?;
    let employment_industry_kind = employment_industry_kind(&corporation.template_key)?;
    let payroll_breakdown = if officer_gross_salary_krw > 0 {
        let dependents =
            read_tax_dependent_count_in_tx(tx, save_id, run_revision, target_game_day).await?;
        Some(
            calculate_corporation_employment_payroll_in_tx(
                tx,
                payroll_rules,
                CorporationEmploymentPayrollInput {
                    payroll_subject_id: corporation.id,
                    employment_policy_set_id: corporation.employment_policy_set_id,
                    payday: market_date,
                    gross_pay_krw: officer_gross_salary_krw,
                    dependents,
                    industry: employment_industry_kind,
                },
            )
            .await?,
        )
    } else {
        None
    };
    let payroll_plan = payroll_breakdown
        .map(|breakdown| {
            corporation_rules.plan_officer_payroll(CorporationOfficerPayrollInput {
                cash_after_operating_krw: operating_cash_after_krw,
                gross_salary_krw: breakdown.period.gross_pay_krw,
                employee_insurance_total_krw: breakdown.employee_insurance_total_krw,
                employer_insurance_total_krw: breakdown.employer_insurance_total_krw,
                withheld_income_tax_krw: breakdown.withheld_income_tax_krw,
                withheld_local_income_tax_krw: breakdown.withheld_local_income_tax_krw,
                net_salary_krw: breakdown.net_salary_pay_krw,
            })
        })
        .transpose()
        .context("corporation officer payroll plan failed")?;
    let payroll_status = match payroll_plan {
        None => "notConfigured",
        Some(payroll) if payroll.paid => "paid",
        Some(_) => "unpaid",
    };
    let insurance = payroll_breakdown.map(|breakdown| breakdown.insurance);
    let national_pension_employee_krw = insurance
        .map(|value| value.national_pension.employee_amount_krw)
        .unwrap_or(0);
    let national_pension_employer_krw = insurance
        .map(|value| value.national_pension.employer_amount_krw)
        .unwrap_or(0);
    let health_insurance_employee_krw = insurance
        .map(|value| value.health_insurance.employee_amount_krw)
        .unwrap_or(0);
    let health_insurance_employer_krw = insurance
        .map(|value| value.health_insurance.employer_amount_krw)
        .unwrap_or(0);
    let long_term_care_employee_krw = insurance
        .map(|value| value.long_term_care.employee_amount_krw)
        .unwrap_or(0);
    let long_term_care_employer_krw = insurance
        .map(|value| value.long_term_care.employer_amount_krw)
        .unwrap_or(0);
    let employment_insurance_employee_krw = insurance
        .map(|value| value.employment_insurance.employee_amount_krw)
        .unwrap_or(0);
    let employment_insurance_employer_krw = insurance
        .map(|value| value.employment_insurance.employer_amount_krw)
        .unwrap_or(0);
    let industrial_accident_employer_krw = insurance
        .map(|value| value.industrial_accident.employer_amount_krw)
        .unwrap_or(0);
    let employee_insurance_total_krw = payroll_breakdown
        .map(|value| value.employee_insurance_total_krw)
        .unwrap_or(0);
    let employer_insurance_total_krw = payroll_breakdown
        .map(|value| value.employer_insurance_total_krw)
        .unwrap_or(0);
    let withheld_income_tax_krw = payroll_breakdown
        .map(|value| value.withheld_income_tax_krw)
        .unwrap_or(0);
    let withheld_local_income_tax_krw = payroll_breakdown
        .map(|value| value.withheld_local_income_tax_krw)
        .unwrap_or(0);
    let net_salary_pay_krw = payroll_breakdown
        .map(|value| value.net_salary_pay_krw)
        .unwrap_or(0);
    let total_payroll_cost_krw = payroll_plan
        .map(|value| value.total_payroll_cost_krw)
        .unwrap_or(0);
    let withholding_liability_krw = payroll_plan
        .map(|value| value.withholding_liability_krw)
        .unwrap_or(0);
    let payroll_cash_paid_krw = payroll_plan
        .map(|value| value.corporation_cash_debit_krw)
        .unwrap_or(0);
    let payroll_payable_krw = payroll_plan
        .map(|value| value.operating_payable_increase_krw)
        .unwrap_or(0);
    let cash_after_krw = operating_cash_after_krw
        .checked_sub(payroll_cash_paid_krw)
        .context("corporation payroll cash underflowed")?;
    let operating_payable_after_krw = corporation
        .operating_payable_krw
        .checked_add(operating_cost_payable_krw)
        .and_then(|amount| amount.checked_add(payroll_payable_krw))
        .context("corporation operating payable overflowed")?;
    let pre_tax_profit_krw = plan
        .pre_payroll_profit_krw
        .checked_sub(total_payroll_cost_krw)
        .context("corporation pre-tax profit overflowed")?;
    let retained_earnings_after_krw = corporation
        .retained_earnings_krw
        .checked_add(pre_tax_profit_krw)
        .context("corporation retained earnings overflowed")?;
    ensure!(
        cash_after_krw <= crate::life::CORPORATION_MAX_PUBLIC_MONEY_KRW
            && operating_payable_after_krw <= crate::life::CORPORATION_MAX_PUBLIC_MONEY_KRW
            && retained_earnings_after_krw.abs() <= crate::life::CORPORATION_MAX_PUBLIC_MONEY_KRW,
        "corporation operating result exceeds the public money range"
    );
    let inserted = sqlx::query(
        "INSERT INTO corporation_operating_month
             (save_id, run_revision, corporation_id,
              corporation_component_version_id, industry_template_id, operating_scale_id,
              operating_setting_id, employment_policy_set_id,
              operating_year, operating_month, entropy_stream, entropy_word, shock_ppm,
              employment_industry, base_monthly_revenue_krw, revenue_variation_ppm,
              variable_cost_ppm, base_fixed_cost_krw, scale_revenue_factor_ppm,
              scale_fixed_cost_krw, officer_gross_salary_krw,
              payroll_status, national_pension_employee_krw,
              national_pension_employer_krw, health_insurance_employee_krw,
              health_insurance_employer_krw, long_term_care_employee_krw,
              long_term_care_employer_krw, employment_insurance_employee_krw,
              employment_insurance_employer_krw, industrial_accident_employer_krw,
              employee_insurance_total_krw, employer_insurance_total_krw,
              withheld_income_tax_krw, withheld_local_income_tax_krw,
              net_salary_pay_krw, total_payroll_cost_krw,
              withholding_liability_krw, payroll_cash_paid_krw, payroll_payable_krw,
              revenue_krw, variable_cost_krw, operating_expense_krw,
              pre_payroll_profit_krw, pre_tax_profit_krw,
              cash_before_krw, operating_cost_cash_paid_krw,
              operating_cost_payable_krw, operating_cash_after_krw, cash_after_krw,
              operating_payable_before_krw, operating_payable_after_krw,
              retained_earnings_before_krw, retained_earnings_after_krw,
              applied_game_day, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0,
                 ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                 ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                 ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                 ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'preparing')",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(corporation.id)
    .bind(corporation.corporation_component_version_id)
    .bind(corporation.industry_template_id)
    .bind(operating_scale_id)
    .bind(operating_setting_id)
    .bind((officer_gross_salary_krw > 0).then_some(corporation.employment_policy_set_id))
    .bind(operating_year)
    .bind(operating_month)
    .bind(plan.entropy_word)
    .bind(plan.shock_ppm)
    .bind(employment_industry)
    .bind(corporation.base_monthly_revenue_krw)
    .bind(corporation.revenue_variation_ppm)
    .bind(corporation.variable_cost_ppm)
    .bind(plan.base_fixed_cost_krw)
    .bind(scale_revenue_factor_ppm)
    .bind(plan.scale_fixed_cost_krw)
    .bind(officer_gross_salary_krw)
    .bind(payroll_status)
    .bind(national_pension_employee_krw)
    .bind(national_pension_employer_krw)
    .bind(health_insurance_employee_krw)
    .bind(health_insurance_employer_krw)
    .bind(long_term_care_employee_krw)
    .bind(long_term_care_employer_krw)
    .bind(employment_insurance_employee_krw)
    .bind(employment_insurance_employer_krw)
    .bind(industrial_accident_employer_krw)
    .bind(employee_insurance_total_krw)
    .bind(employer_insurance_total_krw)
    .bind(withheld_income_tax_krw)
    .bind(withheld_local_income_tax_krw)
    .bind(net_salary_pay_krw)
    .bind(total_payroll_cost_krw)
    .bind(withholding_liability_krw)
    .bind(payroll_cash_paid_krw)
    .bind(payroll_payable_krw)
    .bind(plan.revenue_krw)
    .bind(plan.variable_cost_krw)
    .bind(plan.operating_expense_krw)
    .bind(plan.pre_payroll_profit_krw)
    .bind(pre_tax_profit_krw)
    .bind(corporation.cash_krw)
    .bind(operating_cost_cash_paid_krw)
    .bind(operating_cost_payable_krw)
    .bind(operating_cash_after_krw)
    .bind(cash_after_krw)
    .bind(corporation.operating_payable_krw)
    .bind(operating_payable_after_krw)
    .bind(corporation.retained_earnings_krw)
    .bind(retained_earnings_after_krw)
    .bind(target_game_day)
    .execute(&mut **tx)
    .await?;
    let operating_month_id = inserted.last_insert_id();
    let revenue_ledger_transaction_id = write_monthly_revenue_ledger(
        tx,
        save_id,
        run_revision,
        corporation.id,
        operating_month_id,
        target_game_day,
        &plan,
    )
    .await?;
    let expense_ledger_transaction_id = write_monthly_expense_ledger(
        tx,
        save_id,
        run_revision,
        corporation.id,
        operating_month_id,
        target_game_day,
        &plan,
        operating_cost_cash_paid_krw,
        operating_cost_payable_krw,
    )
    .await?;
    let payroll_ledger_transaction_id = if let Some(payroll) = payroll_plan {
        Some(
            write_officer_payroll_corporation_ledger(
                tx,
                save_id,
                run_revision,
                corporation.id,
                operating_month_id,
                target_game_day,
                payroll,
            )
            .await?,
        )
    } else {
        None
    };
    let mut personal_payroll_ledger_transaction_id = None;
    let mut employment_income_event_id = None;
    if let (Some(breakdown), Some(payroll)) = (payroll_breakdown, payroll_plan)
        && payroll.paid
    {
        let personal_ledger_id = write_officer_payroll_personal_ledger(
            tx,
            finance_rules,
            save_id,
            run_revision,
            corporation.finance_policy_set_id,
            corporation.id,
            operating_month_id,
            target_game_day,
            &breakdown,
        )
        .await?;
        let insurance = breakdown.insurance;
        let income_event_id = record_employment_income_event_in_tx(
            tx,
            EmploymentIncomeEventWrite {
                save_id,
                run_revision,
                employment_policy_set_id: corporation.employment_policy_set_id,
                source: EmploymentIncomeEventSource::CorporationOfficerPayroll {
                    operating_month_id,
                },
                scheduled_settlement_id: None,
                ledger_transaction_id: Some(personal_ledger_id),
                paid_game_day: target_game_day,
                paid_date: market_date,
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
        let personal_cash_after_krw = corporation
            .personal_cash_krw
            .checked_add(payroll.personal_wallet_credit_krw)
            .context("corporation payroll personal cash overflowed")?;
        let updated_wallet = sqlx::query(
            "UPDATE save SET cash_krw = ?
             WHERE id = ? AND run_revision = ? AND game_day + 1 = ? AND cash_krw = ?",
        )
        .bind(personal_cash_after_krw)
        .bind(save_id)
        .bind(run_revision)
        .bind(target_game_day)
        .bind(corporation.personal_cash_krw)
        .execute(&mut **tx)
        .await?;
        ensure!(
            updated_wallet.rows_affected() == 1,
            "corporation payroll wallet changed"
        );
        personal_payroll_ledger_transaction_id = Some(personal_ledger_id);
        employment_income_event_id = Some(income_event_id);
    }
    let next_status = if operating_cost_payable_krw > 0 || payroll_payable_krw > 0 {
        "insolvent"
    } else {
        "active"
    };
    let updated = sqlx::query(
        "UPDATE corporation
         SET cash_krw = ?, operating_payable_krw = ?, retained_earnings_krw = ?, status = ?
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'active'
           AND cash_krw = ? AND operating_payable_krw = ? AND retained_earnings_krw = ?",
    )
    .bind(cash_after_krw)
    .bind(operating_payable_after_krw)
    .bind(retained_earnings_after_krw)
    .bind(next_status)
    .bind(corporation.id)
    .bind(save_id)
    .bind(run_revision)
    .bind(corporation.cash_krw)
    .bind(corporation.operating_payable_krw)
    .bind(corporation.retained_earnings_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "corporation operating state changed"
    );
    if operating_cost_payable_krw > 0 || payroll_payable_krw > 0 {
        sqlx::query(
            "INSERT INTO corporation_transition
                 (save_id, run_revision, corporation_id, transition_no,
                  from_status, to_status, command_id, transition_game_day, transition_reason)
             VALUES (?, ?, ?, 3, 'active', 'insolvent', NULL, ?, 'operatingCashShortfall')",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(corporation.id)
        .bind(target_game_day)
        .execute(&mut **tx)
        .await?;
    }
    let applied = sqlx::query(
        "UPDATE corporation_operating_month
         SET revenue_ledger_transaction_id = ?, expense_ledger_transaction_id = ?,
             payroll_ledger_transaction_id = ?, personal_payroll_ledger_transaction_id = ?,
             employment_income_event_id = ?,
             status = 'applied', applied_at = CURRENT_TIMESTAMP(3)
         WHERE id = ? AND status = 'preparing'",
    )
    .bind(revenue_ledger_transaction_id)
    .bind(expense_ledger_transaction_id)
    .bind(payroll_ledger_transaction_id)
    .bind(personal_payroll_ledger_transaction_id)
    .bind(employment_income_event_id)
    .bind(operating_month_id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        applied.rows_affected() == 1,
        "corporation operating month was not applied"
    );
    Ok(())
}

fn employment_industry(template_key: &str) -> Result<&'static str> {
    match template_key {
        "softwareService" => Ok("itSoftware"),
        "onlineRetail" | "contentStudio" => Ok("retailService"),
        _ => bail!("unknown corporation employment industry mapping"),
    }
}

fn employment_industry_kind(template_key: &str) -> Result<Industry> {
    match template_key {
        "softwareService" => Ok(Industry::ItSoftware),
        "onlineRetail" | "contentStudio" => Ok(Industry::RetailService),
        _ => bail!("unknown corporation employment industry mapping"),
    }
}

#[allow(clippy::too_many_arguments)]
async fn write_officer_payroll_corporation_ledger(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    corporation_id: u64,
    operating_month_id: u64,
    game_day: u32,
    payroll: CorporationOfficerPayrollPlan,
) -> Result<u64> {
    let mut postings = vec![("officerPayrollExpense", payroll.total_payroll_cost_krw)];
    if payroll.paid {
        if payroll.corporation_cash_debit_krw > 0 {
            postings.push(("corporationCash", -payroll.corporation_cash_debit_krw));
        }
        if payroll.withholding_liability_krw > 0 {
            postings.push((
                "withholdingTaxLiability",
                -payroll.withholding_liability_krw,
            ));
        }
    } else {
        postings.push(("operatingPayable", -payroll.operating_payable_increase_krw));
    }
    write_monthly_corporation_ledger(
        tx,
        save_id,
        run_revision,
        corporation_id,
        operating_month_id,
        game_day,
        "officerPayroll",
        "월 대표 급여",
        &postings,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn write_officer_payroll_personal_ledger(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    save_id: u64,
    run_revision: u32,
    policy_set_id: u64,
    corporation_id: u64,
    operating_month_id: u64,
    game_day: u32,
    breakdown: &PayrollBreakdown,
) -> Result<u64> {
    let insurance = breakdown.insurance;
    let mut postings = Vec::with_capacity(8);
    push_personal_posting(
        &mut postings,
        LedgerAccountCode::Wallet,
        breakdown.net_salary_pay_krw,
    );
    push_personal_posting(
        &mut postings,
        LedgerAccountCode::SalaryIncome,
        breakdown
            .period
            .gross_pay_krw
            .checked_neg()
            .context("corporation gross salary ledger amount overflowed")?,
    );
    push_personal_posting(
        &mut postings,
        LedgerAccountCode::EmployeeNationalPensionExpense,
        insurance.national_pension.employee_amount_krw,
    );
    push_personal_posting(
        &mut postings,
        LedgerAccountCode::EmployeeHealthInsuranceExpense,
        insurance.health_insurance.employee_amount_krw,
    );
    push_personal_posting(
        &mut postings,
        LedgerAccountCode::EmployeeLongTermCareExpense,
        insurance.long_term_care.employee_amount_krw,
    );
    push_personal_posting(
        &mut postings,
        LedgerAccountCode::EmployeeEmploymentInsuranceExpense,
        insurance.employment_insurance.employee_amount_krw,
    );
    push_personal_posting(
        &mut postings,
        LedgerAccountCode::EmploymentIncomeTaxWithholding,
        breakdown.withheld_income_tax_krw,
    );
    push_personal_posting(
        &mut postings,
        LedgerAccountCode::EmploymentLocalIncomeTaxWithholding,
        breakdown.withheld_local_income_tax_krw,
    );
    let ledger = finance_rules
        .create_ledger_transaction(LedgerTransactionDraft {
            policy: RunPolicyContext {
                run: RunId {
                    save_id: ResourceId::from_u64(save_id),
                    run_revision,
                },
                policy_set_id: ResourceId::from_u64(policy_set_id),
            },
            source: LedgerSource {
                kind: LedgerSourceKind::CorporationOfficerPayroll,
                source_id: operating_month_id.to_string(),
            },
            game_day,
            description: "법인 대표 급여 지급".to_owned(),
            postings,
        })
        .context("corporation officer personal ledger is invalid")?;
    write_personal_ledger(tx, &ledger, ResourceId::from_u64(corporation_id)).await
}

fn push_personal_posting(
    postings: &mut Vec<LedgerPosting>,
    account_code: LedgerAccountCode,
    amount_krw: i64,
) {
    if amount_krw != 0 {
        postings.push(LedgerPosting {
            account_code,
            financial_account_id: None,
            amount_krw,
        });
    }
}

async fn write_monthly_revenue_ledger(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    corporation_id: u64,
    operating_month_id: u64,
    game_day: u32,
    plan: &CorporationOperatingMonthPlan,
) -> Result<u64> {
    write_monthly_corporation_ledger(
        tx,
        save_id,
        run_revision,
        corporation_id,
        operating_month_id,
        game_day,
        "monthlyRevenue",
        "월 법인 매출",
        &[
            ("corporationCash", plan.revenue_krw),
            ("operatingRevenue", -plan.revenue_krw),
        ],
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn write_monthly_expense_ledger(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    corporation_id: u64,
    operating_month_id: u64,
    game_day: u32,
    plan: &CorporationOperatingMonthPlan,
    operating_cost_cash_paid_krw: i64,
    operating_cost_payable_krw: i64,
) -> Result<u64> {
    let fixed_cost_krw = plan
        .base_fixed_cost_krw
        .checked_add(plan.scale_fixed_cost_krw)
        .context("corporation fixed cost overflowed")?;
    let mut postings = vec![
        ("variableCostExpense", plan.variable_cost_krw),
        ("fixedCostExpense", fixed_cost_krw),
        ("corporationCash", -operating_cost_cash_paid_krw),
    ];
    if operating_cost_payable_krw > 0 {
        postings.push(("operatingPayable", -operating_cost_payable_krw));
    }
    write_monthly_corporation_ledger(
        tx,
        save_id,
        run_revision,
        corporation_id,
        operating_month_id,
        game_day,
        "monthlyExpense",
        "월 법인 영업비용",
        &postings,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn write_monthly_corporation_ledger(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    corporation_id: u64,
    operating_month_id: u64,
    game_day: u32,
    transaction_kind: &str,
    description: &str,
    postings: &[(&str, i64)],
) -> Result<u64> {
    ensure!(
        postings
            .iter()
            .try_fold(0_i64, |sum, (_, amount)| sum.checked_add(*amount))
            == Some(0),
        "corporation monthly ledger is not balanced"
    );
    let inserted = sqlx::query(
        "INSERT INTO corporation_ledger_transaction
             (save_id, run_revision, corporation_id, game_day,
              transaction_kind, correlation_id, operating_month_id, description)
         VALUES (?, ?, ?, ?, ?, NULL, ?, ?)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(corporation_id)
    .bind(game_day)
    .bind(transaction_kind)
    .bind(operating_month_id)
    .bind(description)
    .execute(&mut **tx)
    .await?;
    let ledger_transaction_id = inserted.last_insert_id();
    for (index, (account_code, amount_krw)) in postings.iter().enumerate() {
        let posting_order = u16::try_from(index + 1).context("too many corporation postings")?;
        sqlx::query(
            "INSERT INTO corporation_ledger_posting
                 (save_id, run_revision, corporation_id,
                  corporation_ledger_transaction_id, posting_order, account_code, amount_krw)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(corporation_id)
        .bind(ledger_transaction_id)
        .bind(posting_order)
        .bind(account_code)
        .bind(amount_krw)
        .execute(&mut **tx)
        .await?;
    }
    Ok(ledger_transaction_id)
}

pub(super) async fn create_corporation(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    corporation_rules: &dyn CorporationRules,
    user_id: u64,
    command: &CreateCorporationCommand,
) -> Result<LifeStoreResult<CorporationReceipt>> {
    for _ in 0..MAX_TRANSACTION_ATTEMPTS {
        match create_corporation_once(pool, finance_rules, corporation_rules, user_id, command)
            .await
        {
            Ok(result) => return Ok(result),
            Err(error) if is_retryable_database_error(&error) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy))
}

async fn create_corporation_once(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    corporation_rules: &dyn CorporationRules,
    user_id: u64,
    command: &CreateCorporationCommand,
) -> Result<LifeStoreResult<CorporationReceipt>> {
    let fingerprint = create_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(scope) = read_scope_for_user(&mut tx, user_id, true).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_CREATE,
        payload_sha256: &fingerprint,
        cursor: command.cursor,
    };
    match inspect_command_identity(&mut tx, scope.save_id, &identity).await? {
        CommandIdentityState::Matching => {
            return finish_replay(tx, &scope, command, &fingerprint).await;
        }
        CommandIdentityState::Conflict => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(
                LifeFailureCode::IdempotencyConflict,
            ));
        }
        CommandIdentityState::Missing => {}
    }
    let Some(representative_name) = scope.representative_name.as_deref() else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    if !has_current_cursor(&scope, command) {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CorporationStateConflict,
        ));
    }
    if !scope_is_available(&scope) {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::RateUnavailable));
    }
    if read_current_corporation(&mut tx, &scope, true)
        .await?
        .is_some()
        || has_non_terminal_insolvency(&mut tx, &scope).await?
    {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CorporationStateConflict,
        ));
    }
    let Some(template) = read_template_by_id(
        &mut tx,
        scope
            .corporation_component_version_id
            .context("corporation scope has no component")?,
        command.industry_template_id.get(),
    )
    .await?
    else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CorporationResourceNotFound,
        ));
    };
    let plan = match corporation_rules.plan_establishment(CorporationEstablishmentInput {
        name: &command.name,
        capital_krw: command.capital_krw,
        wallet_cash_krw: scope.cash_krw,
        policy: registration_policy(&scope)?,
        terms: establishment_terms(&scope)?,
    }) {
        Ok(plan) => plan,
        Err(CorporationError::InsufficientWalletCash) => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(
                LifeFailureCode::InsufficientWalletCash,
            ));
        }
        Err(_) => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(LifeFailureCode::InvalidCommand));
        }
    };

    write_command_identity(&mut tx, scope.save_id, &identity).await?;
    let inserted = sqlx::query(
        "INSERT INTO corporation
             (save_id, run_revision, life_catalog_set_id, policy_set_id,
              corporation_component_version_id, industry_template_id,
              registration_policy_rule_id, name, representative_name, status,
              registered_office_class, establishment_command_id, established_game_day,
              capital_krw, registration_license_tax_krw, local_education_tax_krw,
              game_administrative_fee_krw, total_establishment_fee_krw,
              cash_krw, contributed_capital_krw)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'draft', 'standardRegisteredOffice', ?, ?,
                 ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.life_catalog_set_id.context("missing life catalog")?)
    .bind(scope.policy_set_id.context("missing corporation policy")?)
    .bind(
        scope
            .corporation_component_version_id
            .context("missing corporation component")?,
    )
    .bind(template.id)
    .bind(
        scope
            .registration_policy_rule_id
            .context("missing corporation registration rule")?,
    )
    .bind(&plan.canonical_name)
    .bind(representative_name)
    .bind(command.command_id.as_str())
    .bind(scope.game_day)
    .bind(plan.capital_krw)
    .bind(plan.charges.registration_license_tax_krw)
    .bind(plan.charges.local_education_tax_krw)
    .bind(plan.charges.game_administrative_fee_krw)
    .bind(plan.charges.total_fee_krw)
    .bind(plan.corporation_cash_after_krw)
    .bind(plan.capital_krw)
    .execute(&mut *tx)
    .await?;
    let corporation_id = inserted.last_insert_id();
    insert_transition(
        &mut tx,
        &scope,
        corporation_id,
        EstablishmentTransition::Draft,
        command.command_id.as_str(),
    )
    .await?;

    let personal_ledger = finance_rules
        .create_ledger_transaction(LedgerTransactionDraft {
            policy: policy_context(&scope)?,
            source: LedgerSource {
                kind: LedgerSourceKind::CorporationEstablishment,
                source_id: command.command_id.as_str().to_owned(),
            },
            game_day: scope.game_day,
            description: format!("{} 법인 설립", plan.canonical_name),
            postings: vec![
                LedgerPosting {
                    account_code: LedgerAccountCode::CorporationInvestmentAsset,
                    financial_account_id: None,
                    amount_krw: plan.capital_krw,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::CorporationRegistrationExpense,
                    financial_account_id: None,
                    amount_krw: plan.charges.total_fee_krw,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::Wallet,
                    financial_account_id: None,
                    amount_krw: -plan.wallet_debit_krw,
                },
            ],
        })
        .context("corporation personal ledger validation failed")?;
    let personal_ledger_transaction_id = write_personal_ledger(
        &mut tx,
        &personal_ledger,
        ResourceId::from_u64(corporation_id),
    )
    .await?;
    let corporation_ledger_transaction_id = write_corporation_ledger(
        &mut tx,
        &scope,
        corporation_id,
        command.command_id.as_str(),
        &plan,
    )
    .await?;

    let next_revision = scope
        .state_revision
        .checked_add(1)
        .context("corporation state revision overflowed")?;
    let updated = sqlx::query(
        "UPDATE save SET cash_krw = ?, state_revision = ?
         WHERE id = ? AND run_revision = ? AND state_revision = ? AND game_day = ?",
    )
    .bind(plan.wallet_cash_after_krw)
    .bind(next_revision)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.state_revision)
    .bind(scope.game_day)
    .execute(&mut *tx)
    .await?;
    ensure!(updated.rows_affected() == 1, "corporation cursor changed");

    let activated = sqlx::query(
        "UPDATE corporation
         SET status = 'active', personal_ledger_transaction_id = ?,
             corporation_ledger_transaction_id = ?
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'draft'",
    )
    .bind(personal_ledger_transaction_id)
    .bind(corporation_ledger_transaction_id)
    .bind(corporation_id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .execute(&mut *tx)
    .await?;
    ensure!(
        activated.rows_affected() == 1,
        "corporation activation failed"
    );
    insert_transition(
        &mut tx,
        &scope,
        corporation_id,
        EstablishmentTransition::Active,
        command.command_id.as_str(),
    )
    .await?;

    let row = read_corporation_by_id(&mut tx, &scope, corporation_id, false)
        .await?
        .context("created corporation disappeared")?;
    let receipt = CorporationReceipt {
        command_id: command.command_id.clone(),
        corporation: corporation_summary(&row)?,
        wallet_debit_krw: plan.wallet_debit_krw,
        replayed: false,
    };
    write_receipt(
        &mut tx,
        &scope,
        command,
        &fingerprint,
        next_revision,
        &receipt,
    )
    .await?;
    let save = read_state(&mut tx, scope.save_id).await?;
    tx.commit().await?;
    Ok(LifeStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

pub(super) async fn update_corporation_settings(
    pool: &MySqlPool,
    user_id: u64,
    command: &UpdateCorporationSettingsCommand,
) -> Result<LifeStoreResult<CorporationSettingsReceipt>> {
    for _ in 0..MAX_TRANSACTION_ATTEMPTS {
        match update_corporation_settings_once(pool, user_id, command).await {
            Ok(result) => return Ok(result),
            Err(error) if is_retryable_database_error(&error) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy))
}

async fn update_corporation_settings_once(
    pool: &MySqlPool,
    user_id: u64,
    command: &UpdateCorporationSettingsCommand,
) -> Result<LifeStoreResult<CorporationSettingsReceipt>> {
    let fingerprint = settings_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(scope) = read_scope_for_user(&mut tx, user_id, true).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_UPDATE_SETTINGS,
        payload_sha256: &fingerprint,
        cursor: command.cursor,
    };
    match inspect_command_identity(&mut tx, scope.save_id, &identity).await? {
        CommandIdentityState::Matching => {
            return finish_settings_replay(tx, &scope, command, &fingerprint).await;
        }
        CommandIdentityState::Conflict => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(
                LifeFailureCode::IdempotencyConflict,
            ));
        }
        CommandIdentityState::Missing => {}
    }
    if !has_current_settings_cursor(&scope, command) {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CorporationStateConflict,
        ));
    }
    if !scope_is_available(&scope) {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::RateUnavailable));
    }
    let Some(corporation) =
        read_corporation_by_id(&mut tx, &scope, command.corporation_id.get(), true).await?
    else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CorporationResourceNotFound,
        ));
    };
    if corporation.status != "active"
        || !(0..=100_000_000).contains(&command.officer_gross_salary_krw)
    {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CorporationStateConflict,
        ));
    }
    let scale = sqlx::query_as::<_, ScaleRow>(
        "SELECT id, industry_template_id, scale_key, scale_order,
                revenue_factor_ppm, fixed_cost_krw
         FROM corporation_operating_scale
         WHERE life_component_version_id = ? AND industry_template_id = ? AND id = ?",
    )
    .bind(corporation.corporation_component_version_id)
    .bind(corporation.industry_template_id)
    .bind(command.operating_scale_id.get())
    .fetch_optional(&mut *tx)
    .await?;
    let Some(scale) = scale else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CorporationResourceNotFound,
        ));
    };
    let next_month_date = scope
        .current_date
        .replace_day(1)
        .context("corporation setting current month is invalid")?
        .checked_add(Duration::days(32))
        .context("corporation setting effective month overflowed")?
        .replace_day(1)
        .context("corporation setting next month is invalid")?;
    let effective_year = u16::try_from(next_month_date.year())
        .context("corporation setting year is outside the supported range")?;
    let effective_month = u8::from(next_month_date.month());

    write_command_identity(&mut tx, scope.save_id, &identity).await?;
    let inserted = sqlx::query(
        "INSERT INTO corporation_operating_setting
             (save_id, run_revision, corporation_id,
              corporation_component_version_id, industry_template_id, operating_scale_id,
              command_id, effective_year, effective_month,
              officer_gross_salary_krw, created_game_day)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(corporation.id)
    .bind(corporation.corporation_component_version_id)
    .bind(corporation.industry_template_id)
    .bind(scale.id)
    .bind(command.command_id.as_str())
    .bind(effective_year)
    .bind(effective_month)
    .bind(command.officer_gross_salary_krw)
    .bind(scope.game_day)
    .execute(&mut *tx)
    .await?;
    let setting_id = inserted.last_insert_id();
    let next_revision = scope
        .state_revision
        .checked_add(1)
        .context("corporation setting state revision overflowed")?;
    let updated = sqlx::query(
        "UPDATE save SET state_revision = ?
         WHERE id = ? AND run_revision = ? AND state_revision = ? AND game_day = ?",
    )
    .bind(next_revision)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.state_revision)
    .bind(scope.game_day)
    .execute(&mut *tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "corporation setting cursor changed"
    );
    let setting = setting_state(SettingRow {
        id: setting_id,
        corporation_id: corporation.id,
        operating_scale_id: scale.id,
        scale_key: scale.scale_key,
        scale_order: scale.scale_order,
        revenue_factor_ppm: scale.revenue_factor_ppm,
        fixed_cost_krw: scale.fixed_cost_krw,
        effective_year,
        effective_month,
        officer_gross_salary_krw: command.officer_gross_salary_krw,
        created_game_day: scope.game_day,
    });
    let receipt = CorporationSettingsReceipt {
        command_id: command.command_id.clone(),
        setting,
        replayed: false,
    };
    sqlx::query(
        "INSERT INTO corporation_setting_command_receipt
             (save_id, command_id, run_revision, state_revision, game_day,
              corporation_id, operating_setting_id, command_kind, payload_sha256, result)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'updateCorporationSettings', ?, ?)",
    )
    .bind(scope.save_id)
    .bind(command.command_id.as_str())
    .bind(scope.run_revision)
    .bind(next_revision)
    .bind(scope.game_day)
    .bind(corporation.id)
    .bind(setting_id)
    .bind(&fingerprint)
    .bind(serde_json::to_string(&receipt)?)
    .execute(&mut *tx)
    .await?;
    let save = read_state(&mut tx, scope.save_id).await?;
    tx.commit().await?;
    Ok(LifeStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

async fn finish_settings_replay(
    mut tx: Transaction<'_, MySql>,
    scope: &ScopeRow,
    command: &UpdateCorporationSettingsCommand,
    fingerprint: &str,
) -> Result<LifeStoreResult<CorporationSettingsReceipt>> {
    let row = sqlx::query_as::<_, SettingReceiptRow>(
        "SELECT command_kind, payload_sha256, CAST(result AS CHAR) AS result_json
         FROM corporation_setting_command_receipt
         WHERE save_id = ? AND command_id = ?",
    )
    .bind(scope.save_id)
    .bind(command.command_id.as_str())
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::IdempotencyConflict,
        ));
    };
    ensure!(
        row.command_kind == COMMAND_KIND_UPDATE_SETTINGS && row.payload_sha256 == fingerprint,
        "corporation setting replay receipt disagrees with command identity"
    );
    let mut receipt: CorporationSettingsReceipt = serde_json::from_str(&row.result_json)?;
    receipt.replayed = true;
    let save = read_state(&mut tx, scope.save_id).await?;
    tx.commit().await?;
    Ok(LifeStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

pub(super) async fn pay_corporation_dividend(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    corporation_rules: &dyn CorporationRules,
    user_id: u64,
    command: &PayCorporationDividendCommand,
) -> Result<LifeStoreResult<CorporationDividendReceipt>> {
    for _ in 0..MAX_TRANSACTION_ATTEMPTS {
        match pay_corporation_dividend_once(
            pool,
            finance_rules,
            corporation_rules,
            user_id,
            command,
        )
        .await
        {
            Ok(result) => return Ok(result),
            Err(error) if is_retryable_database_error(&error) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy))
}

async fn pay_corporation_dividend_once(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    corporation_rules: &dyn CorporationRules,
    user_id: u64,
    command: &PayCorporationDividendCommand,
) -> Result<LifeStoreResult<CorporationDividendReceipt>> {
    let fingerprint = dividend_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(scope) = read_scope_for_user(&mut tx, user_id, true).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_PAY_DIVIDEND,
        payload_sha256: &fingerprint,
        cursor: command.cursor,
    };
    match inspect_command_identity(&mut tx, scope.save_id, &identity).await? {
        CommandIdentityState::Matching => {
            return finish_dividend_replay(tx, &scope, command, &fingerprint).await;
        }
        CommandIdentityState::Conflict => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(
                LifeFailureCode::IdempotencyConflict,
            ));
        }
        CommandIdentityState::Missing => {}
    }
    if scope.representative_name.is_none() {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    }
    if !scope_is_available(&scope) {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::RateUnavailable));
    }
    if scope.run_revision != command.cursor.expected_run_revision
        || scope.state_revision != command.cursor.expected_state_revision
        || scope.game_day != command.cursor.expected_game_day
    {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CorporationStateConflict,
        ));
    }
    let Some(corporation) =
        read_corporation_by_id(&mut tx, &scope, command.corporation_id.get(), true).await?
    else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CorporationResourceNotFound,
        ));
    };
    if corporation.status != "active" {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CorporationStateConflict,
        ));
    }
    let tax_year: Option<u16> = sqlx::query_scalar(
        "SELECT tax_year FROM corporation_tax_year
         WHERE save_id = ? AND run_revision = ? AND corporation_id = ? AND status = 'applied'
         ORDER BY tax_year DESC LIMIT 1",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(corporation.id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(tax_year) = tax_year else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CorporationStateConflict,
        ));
    };
    let policy: Option<CorporationDividendPolicyRow> = sqlx::query_as(
        "SELECT rule.id AS policy_rule_id,
                CAST(JSON_UNQUOTE(JSON_EXTRACT(rule.parameters, '$.incomeTaxRatePpm')) AS SIGNED)
                    AS income_tax_rate_ppm,
                CAST(JSON_UNQUOTE(JSON_EXTRACT(
                    rule.parameters, '$.localIncomeTaxOnIncomeTaxPpm'
                )) AS SIGNED) AS local_income_tax_on_income_tax_ppm
         FROM policy_rule AS rule
         WHERE rule.policy_set_id = ? AND rule.domain = 'corporation'
           AND rule.rule_key = 'residentDividendWithholding'
           AND rule.effective_from <= ?
           AND (rule.effective_to IS NULL OR rule.effective_to >= ?)
           AND JSON_LENGTH(rule.parameters) = 5
           AND JSON_EXTRACT(rule.parameters, '$.schemaVersion') = 1
           AND JSON_UNQUOTE(JSON_EXTRACT(rule.parameters, '$.rounding')) = 'floorEachTax'
           AND JSON_UNQUOTE(JSON_EXTRACT(rule.parameters, '$.supportedRecipient'))
                = 'residentIndividual'
         LIMIT 1",
    )
    .bind(scope.policy_set_id.context("missing corporation policy")?)
    .bind(scope.current_date)
    .bind(scope.current_date)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(policy) = policy else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::PolicyUnsupported,
        ));
    };
    let plan = match corporation_rules.plan_dividend(CorporationDividendInput {
        gross_dividend_krw: command.gross_dividend_krw,
        distributable_profit_krw: corporation.distributable_profit_krw,
        corporation_cash_krw: corporation.cash_krw,
        income_tax_rate_ppm: policy.income_tax_rate_ppm,
        local_income_tax_on_income_tax_ppm: policy.local_income_tax_on_income_tax_ppm,
    }) {
        Ok(plan) => plan,
        Err(CorporationError::InsufficientDividendCapacity) => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(
                LifeFailureCode::CorporationStateConflict,
            ));
        }
        Err(_) => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(LifeFailureCode::InvalidCommand));
        }
    };
    write_command_identity(&mut tx, scope.save_id, &identity).await?;
    let retained_earnings_after_krw = corporation
        .retained_earnings_krw
        .checked_sub(plan.gross_dividend_krw)
        .context("corporation retained earnings underflowed on dividend")?;
    let inserted = sqlx::query(
        "INSERT INTO corporation_dividend
             (save_id, run_revision, corporation_id, tax_year, policy_rule_id, command_id,
              gross_dividend_krw, withheld_income_tax_krw,
              withheld_local_income_tax_krw, net_dividend_krw,
              cash_before_krw, cash_after_krw,
              retained_earnings_before_krw, retained_earnings_after_krw,
              distributable_profit_before_krw, distributable_profit_after_krw,
              corporation_ledger_transaction_id, personal_ledger_transaction_id,
              paid_game_day, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, ?, 'preparing')",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(corporation.id)
    .bind(tax_year)
    .bind(policy.policy_rule_id)
    .bind(command.command_id.as_str())
    .bind(plan.gross_dividend_krw)
    .bind(plan.withheld_income_tax_krw)
    .bind(plan.withheld_local_income_tax_krw)
    .bind(plan.net_dividend_krw)
    .bind(corporation.cash_krw)
    .bind(plan.corporation_cash_after_krw)
    .bind(corporation.retained_earnings_krw)
    .bind(retained_earnings_after_krw)
    .bind(corporation.distributable_profit_krw)
    .bind(plan.distributable_profit_after_krw)
    .bind(scope.game_day)
    .execute(&mut *tx)
    .await?;
    let dividend_id = inserted.last_insert_id();
    let corporation_ledger_transaction_id = write_dividend_corporation_ledger(
        &mut tx,
        &scope,
        corporation.id,
        dividend_id,
        command.command_id.as_str(),
        &plan,
    )
    .await?;
    let personal_ledger = finance_rules
        .create_ledger_transaction(LedgerTransactionDraft {
            policy: policy_context(&scope)?,
            source: LedgerSource {
                kind: LedgerSourceKind::CorporationDividend,
                source_id: dividend_id.to_string(),
            },
            game_day: scope.game_day,
            description: "법인 배당 지급".to_owned(),
            postings: dividend_personal_postings(&plan)?,
        })
        .context("corporation dividend personal ledger is invalid")?;
    let personal_ledger_transaction_id = write_personal_ledger(
        &mut tx,
        &personal_ledger,
        ResourceId::from_u64(corporation.id),
    )
    .await?;
    accrue_financial_income_source(
        &mut tx,
        AnnualTaxRunContext {
            save_id: scope.save_id,
            run_revision: scope.run_revision,
            policy_set_id: scope.policy_set_id.context("missing corporation policy")?,
            game_day: scope.game_day,
            market_date: scope.current_date,
        },
        FinancialIncomeAccrual {
            source: FinancialIncomeSource::CorporationDividend,
            gross_income_krw: plan.gross_dividend_krw,
            withheld_income_tax_krw: plan.withheld_income_tax_krw,
            withheld_local_income_tax_krw: plan.withheld_local_income_tax_krw,
        },
    )
    .await?;
    let next_cash_krw = scope
        .cash_krw
        .checked_add(plan.net_dividend_krw)
        .context("personal wallet overflowed on dividend")?;
    let next_revision = scope
        .state_revision
        .checked_add(1)
        .context("corporation dividend revision overflowed")?;
    let updated_save = sqlx::query(
        "UPDATE save SET cash_krw = ?, state_revision = ?
         WHERE id = ? AND run_revision = ? AND state_revision = ? AND game_day = ?",
    )
    .bind(next_cash_krw)
    .bind(next_revision)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.state_revision)
    .bind(scope.game_day)
    .execute(&mut *tx)
    .await?;
    ensure!(
        updated_save.rows_affected() == 1,
        "corporation dividend cursor changed"
    );
    let updated_corporation = sqlx::query(
        "UPDATE corporation
         SET cash_krw = ?, retained_earnings_krw = ?, distributable_profit_krw = ?
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'active'
           AND cash_krw = ? AND retained_earnings_krw = ? AND distributable_profit_krw = ?",
    )
    .bind(plan.corporation_cash_after_krw)
    .bind(retained_earnings_after_krw)
    .bind(plan.distributable_profit_after_krw)
    .bind(corporation.id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(corporation.cash_krw)
    .bind(corporation.retained_earnings_krw)
    .bind(corporation.distributable_profit_krw)
    .execute(&mut *tx)
    .await?;
    ensure!(
        updated_corporation.rows_affected() == 1,
        "corporation changed during dividend"
    );
    let applied = sqlx::query(
        "UPDATE corporation_dividend
         SET status = 'applied', corporation_ledger_transaction_id = ?,
             personal_ledger_transaction_id = ?, applied_at = CURRENT_TIMESTAMP(3)
         WHERE id = ? AND status = 'preparing'",
    )
    .bind(corporation_ledger_transaction_id)
    .bind(personal_ledger_transaction_id)
    .bind(dividend_id)
    .execute(&mut *tx)
    .await?;
    ensure!(
        applied.rows_affected() == 1,
        "corporation dividend apply failed"
    );
    let receipt = CorporationDividendReceipt {
        command_id: command.command_id.clone(),
        id: ResourceId::from_u64(dividend_id),
        corporation_id: command.corporation_id,
        tax_year,
        gross_dividend_krw: plan.gross_dividend_krw,
        withheld_income_tax_krw: plan.withheld_income_tax_krw,
        withheld_local_income_tax_krw: plan.withheld_local_income_tax_krw,
        net_dividend_krw: plan.net_dividend_krw,
        corporation_ledger_transaction_id: ResourceId::from_u64(corporation_ledger_transaction_id),
        personal_ledger_transaction_id: ResourceId::from_u64(personal_ledger_transaction_id),
        paid_game_day: scope.game_day,
        replayed: false,
    };
    sqlx::query(
        "INSERT INTO corporation_dividend_command_receipt
             (save_id, command_id, run_revision, state_revision, game_day,
              corporation_id, payload_sha256, result)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(command.command_id.as_str())
    .bind(scope.run_revision)
    .bind(next_revision)
    .bind(scope.game_day)
    .bind(corporation.id)
    .bind(&fingerprint)
    .bind(serde_json::to_string(&receipt)?)
    .execute(&mut *tx)
    .await?;
    let save = read_state(&mut tx, scope.save_id).await?;
    tx.commit().await?;
    Ok(LifeStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

fn dividend_personal_postings(
    plan: &crate::life::CorporationDividendPlan,
) -> Result<Vec<LedgerPosting>> {
    let mut postings = vec![
        LedgerPosting {
            account_code: LedgerAccountCode::Wallet,
            financial_account_id: None,
            amount_krw: plan.net_dividend_krw,
        },
        LedgerPosting {
            account_code: LedgerAccountCode::DistributionIncome,
            financial_account_id: None,
            amount_krw: -plan.gross_dividend_krw,
        },
    ];
    let withholding_krw = plan
        .withheld_income_tax_krw
        .checked_add(plan.withheld_local_income_tax_krw)
        .context("corporation dividend withholding overflowed")?;
    if withholding_krw > 0 {
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::WithholdingTaxLiability,
            financial_account_id: None,
            amount_krw: withholding_krw,
        });
    }
    Ok(postings)
}

async fn write_dividend_corporation_ledger(
    tx: &mut Transaction<'_, MySql>,
    scope: &ScopeRow,
    corporation_id: u64,
    dividend_id: u64,
    command_id: &str,
    plan: &crate::life::CorporationDividendPlan,
) -> Result<u64> {
    let inserted = sqlx::query(
        "INSERT INTO corporation_ledger_transaction
             (save_id, run_revision, corporation_id, game_day, transaction_kind,
              correlation_id, operating_month_id, corporation_tax_year_id,
              corporation_dividend_id, description)
         VALUES (?, ?, ?, ?, 'dividend', ?, NULL, NULL, ?, '법인 배당')",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(corporation_id)
    .bind(scope.game_day)
    .bind(command_id)
    .bind(dividend_id)
    .execute(&mut **tx)
    .await?;
    let ledger_id = inserted.last_insert_id();
    let withholding_krw = plan
        .withheld_income_tax_krw
        .checked_add(plan.withheld_local_income_tax_krw)
        .context("corporation dividend withholding overflowed")?;
    let mut postings = vec![
        ("dividendDistribution", plan.gross_dividend_krw),
        ("corporationCash", -plan.net_dividend_krw),
    ];
    if withholding_krw > 0 {
        postings.push(("withholdingTaxLiability", -withholding_krw));
    }
    for (index, (account_code, amount_krw)) in postings.into_iter().enumerate() {
        sqlx::query(
            "INSERT INTO corporation_ledger_posting
                 (save_id, run_revision, corporation_id,
                  corporation_ledger_transaction_id, posting_order, account_code, amount_krw)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(corporation_id)
        .bind(ledger_id)
        .bind(u16::try_from(index + 1)?)
        .bind(account_code)
        .bind(amount_krw)
        .execute(&mut **tx)
        .await?;
    }
    Ok(ledger_id)
}

async fn finish_dividend_replay(
    mut tx: Transaction<'_, MySql>,
    scope: &ScopeRow,
    command: &PayCorporationDividendCommand,
    fingerprint: &str,
) -> Result<LifeStoreResult<CorporationDividendReceipt>> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT payload_sha256, CAST(result AS CHAR) AS result_json
         FROM corporation_dividend_command_receipt
         WHERE save_id = ? AND command_id = ?",
    )
    .bind(scope.save_id)
    .bind(command.command_id.as_str())
    .fetch_optional(&mut *tx)
    .await?;
    let Some((payload_sha256, result_json)) = row else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::IdempotencyConflict,
        ));
    };
    ensure!(
        payload_sha256 == fingerprint,
        "corporation dividend receipt fingerprint drifted"
    );
    let mut receipt: CorporationDividendReceipt = serde_json::from_str(&result_json)?;
    receipt.replayed = true;
    let save = read_state(&mut tx, scope.save_id).await?;
    tx.commit().await?;
    Ok(LifeStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

fn setting_state(row: SettingRow) -> CorporationOperatingSettingState {
    CorporationOperatingSettingState {
        id: ResourceId::from_u64(row.id),
        corporation_id: ResourceId::from_u64(row.corporation_id),
        operating_scale_id: ResourceId::from_u64(row.operating_scale_id),
        scale_key: row.scale_key,
        scale_order: row.scale_order,
        revenue_factor_ppm: row.revenue_factor_ppm,
        fixed_cost_krw: row.fixed_cost_krw,
        effective_year: row.effective_year,
        effective_month: row.effective_month,
        officer_gross_salary_krw: row.officer_gross_salary_krw,
        created_game_day: row.created_game_day,
    }
}

async fn read_scope_for_user(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
    lock: bool,
) -> Result<Option<ScopeRow>> {
    read_scope(tx, "save.user_id = ?", user_id, lock).await
}

async fn read_scope_for_save(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
) -> Result<Option<ScopeRow>> {
    read_scope(tx, "save.id = ?", save_id, false).await
}

async fn read_scope(
    tx: &mut Transaction<'_, MySql>,
    predicate: &'static str,
    value: u64,
    lock: bool,
) -> Result<Option<ScopeRow>> {
    let query = format!(
        "SELECT save.id AS save_id, save.run_revision, save.state_revision,
                save.game_day,
                DATE_ADD(world.start_date, INTERVAL save.game_day DAY) AS current_date,
                save.cash_krw,
                `character`.name AS representative_name,
                bundle.life_catalog_set_id, bundle.policy_set_id,
                policy.policy_key,
                catalog.corporation_component_version_id,
                component.version_key AS component_version_key,
                component.availability AS component_availability,
                profile.registered_office_class,
                profile.minimum_capital_krw, profile.maximum_capital_krw,
                profile.game_administrative_fee_krw,
                registration_rule.id AS registration_policy_rule_id,
                CAST(JSON_UNQUOTE(JSON_EXTRACT(
                    registration_rule.parameters, '$.registrationLicenseTaxRatePpm'
                )) AS SIGNED) AS registration_license_tax_rate_ppm,
                CAST(JSON_UNQUOTE(JSON_EXTRACT(
                    registration_rule.parameters, '$.minimumRegistrationLicenseTaxKrw'
                )) AS SIGNED) AS minimum_registration_license_tax_krw,
                CAST(JSON_UNQUOTE(JSON_EXTRACT(
                    registration_rule.parameters, '$.localEducationTaxRatePpm'
                )) AS SIGNED) AS local_education_tax_rate_ppm,
                (
                    SELECT COUNT(*)
                    FROM policy_rule AS corporation_rule
                    WHERE corporation_rule.policy_set_id = bundle.policy_set_id
                      AND corporation_rule.domain = 'corporation'
                      AND corporation_rule.rule_key IN (
                          'standardRegistration',
                          'corporateIncomeTax',
                          'residentDividendWithholding'
                      )
                      AND corporation_rule.effective_from
                            <= DATE_ADD(world.start_date, INTERVAL save.game_day DAY)
                      AND (
                          corporation_rule.effective_to IS NULL
                          OR corporation_rule.effective_to
                                > DATE_ADD(world.start_date, INTERVAL save.game_day DAY)
                      )
                ) AS corporation_policy_rule_count
         FROM save
         LEFT JOIN `character` ON `character`.save_id = save.id
         LEFT JOIN run_rule_bundle AS bundle
           ON bundle.save_id = save.id AND bundle.run_revision = save.run_revision
         LEFT JOIN market_world AS world ON world.id = bundle.market_world_id
         LEFT JOIN policy_set AS policy ON policy.id = bundle.policy_set_id
         LEFT JOIN life_catalog_set AS catalog ON catalog.id = bundle.life_catalog_set_id
         LEFT JOIN life_component_version AS component
           ON component.id = catalog.corporation_component_version_id
          AND component.component_kind = 'corporation'
         LEFT JOIN corporation_component_profile AS profile
           ON profile.life_component_version_id = component.id
         LEFT JOIN policy_rule AS registration_rule
           ON registration_rule.policy_set_id = bundle.policy_set_id
          AND registration_rule.domain = 'corporation'
          AND registration_rule.rule_key = 'standardRegistration'
          AND registration_rule.effective_from <= DATE_ADD(world.start_date, INTERVAL save.game_day DAY)
          AND (registration_rule.effective_to IS NULL
               OR registration_rule.effective_to > DATE_ADD(world.start_date, INTERVAL save.game_day DAY))
         WHERE {predicate}{}",
        if lock { " FOR UPDATE" } else { "" }
    );
    Ok(sqlx::query_as::<_, ScopeRow>(AssertSqlSafe(query.as_str()))
        .bind(value)
        .fetch_optional(&mut **tx)
        .await?)
}

fn scope_is_available(scope: &ScopeRow) -> bool {
    scope.policy_key.as_deref() == Some(POLICY_KEY)
        && scope.component_version_key.as_deref() == Some(COMPONENT_KEY)
        && scope.component_availability.as_deref() == Some("active")
        && scope.life_catalog_set_id.is_some()
        && scope.policy_set_id.is_some()
        && scope.corporation_component_version_id.is_some()
        && scope.registration_policy_rule_id.is_some()
        && scope.corporation_policy_rule_count == 3
        && scope.registered_office_class.as_deref() == Some("standardRegisteredOffice")
}

async fn read_templates_for_scope(
    tx: &mut Transaction<'_, MySql>,
    scope: &ScopeRow,
) -> Result<CorporationTemplatesState> {
    if !scope_is_available(scope) {
        return Ok(CorporationTemplatesState {
            availability: CorporationAvailabilityState::Unavailable,
            component_version_id: None,
            registered_office_class: None,
            minimum_capital_krw: None,
            maximum_capital_krw: None,
            game_administrative_fee_krw: None,
            templates: Vec::new(),
        });
    }
    let component_id = scope
        .corporation_component_version_id
        .context("available corporation scope has no component")?;
    let rows = sqlx::query_as::<_, TemplateRow>(
        "SELECT id, template_key, display_name, template_order,
                base_monthly_revenue_krw, revenue_variation_ppm,
                variable_cost_ppm, fixed_monthly_cost_krw
         FROM corporation_industry_template
         WHERE life_component_version_id = ?
         ORDER BY template_order",
    )
    .bind(component_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(rows.len() == 3, "corporation template cardinality changed");
    let scales = sqlx::query_as::<_, ScaleRow>(
        "SELECT id, industry_template_id, scale_key, scale_order,
                revenue_factor_ppm, fixed_cost_krw
         FROM corporation_operating_scale
         WHERE life_component_version_id = ?
         ORDER BY industry_template_id, scale_order",
    )
    .bind(component_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        scales.len() == 9,
        "corporation operating scale cardinality changed"
    );
    let templates = rows
        .into_iter()
        .map(|row| {
            let operating_scales = scales
                .iter()
                .filter(|scale| scale.industry_template_id == row.id)
                .cloned()
                .map(scale_state)
                .collect::<Vec<_>>();
            ensure!(
                operating_scales.len() == 3,
                "corporation template has an incomplete scale catalog"
            );
            Ok(template_state(row, operating_scales))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(CorporationTemplatesState {
        availability: CorporationAvailabilityState::Active,
        component_version_id: Some(ResourceId::from_u64(component_id)),
        registered_office_class: scope.registered_office_class.clone(),
        minimum_capital_krw: scope.minimum_capital_krw,
        maximum_capital_krw: scope.maximum_capital_krw,
        game_administrative_fee_krw: scope.game_administrative_fee_krw,
        templates,
    })
}

fn template_state(
    row: TemplateRow,
    operating_scales: Vec<CorporationOperatingScaleState>,
) -> CorporationTemplateState {
    CorporationTemplateState {
        id: ResourceId::from_u64(row.id),
        template_key: row.template_key,
        display_name: row.display_name,
        template_order: row.template_order,
        base_monthly_revenue_krw: row.base_monthly_revenue_krw,
        revenue_variation_ppm: row.revenue_variation_ppm,
        variable_cost_ppm: row.variable_cost_ppm,
        fixed_monthly_cost_krw: row.fixed_monthly_cost_krw,
        operating_scales,
    }
}

fn scale_state(row: ScaleRow) -> CorporationOperatingScaleState {
    CorporationOperatingScaleState {
        id: ResourceId::from_u64(row.id),
        scale_key: row.scale_key,
        scale_order: row.scale_order,
        revenue_factor_ppm: row.revenue_factor_ppm,
        fixed_cost_krw: row.fixed_cost_krw,
    }
}

async fn read_template_by_id(
    tx: &mut Transaction<'_, MySql>,
    component_id: u64,
    template_id: u64,
) -> Result<Option<TemplateRow>> {
    Ok(sqlx::query_as::<_, TemplateRow>(
        "SELECT id, template_key, display_name, template_order,
                base_monthly_revenue_krw, revenue_variation_ppm,
                variable_cost_ppm, fixed_monthly_cost_krw
         FROM corporation_industry_template
         WHERE life_component_version_id = ? AND id = ?",
    )
    .bind(component_id)
    .bind(template_id)
    .fetch_optional(&mut **tx)
    .await?)
}

async fn read_current_corporation(
    tx: &mut Transaction<'_, MySql>,
    scope: &ScopeRow,
    lock: bool,
) -> Result<Option<CorporationRow>> {
    read_corporation(tx, scope, None, lock).await
}

async fn read_corporation_by_id(
    tx: &mut Transaction<'_, MySql>,
    scope: &ScopeRow,
    corporation_id: u64,
    lock: bool,
) -> Result<Option<CorporationRow>> {
    read_corporation(tx, scope, Some(corporation_id), lock).await
}

async fn read_corporation(
    tx: &mut Transaction<'_, MySql>,
    scope: &ScopeRow,
    corporation_id: Option<u64>,
    lock: bool,
) -> Result<Option<CorporationRow>> {
    let query = format!(
        "SELECT corporation_row.id,
                corporation_row.corporation_component_version_id,
                corporation_row.industry_template_id,
                template.template_key,
                template.display_name AS template_display_name,
                corporation_row.name, corporation_row.representative_name,
                corporation_row.status, corporation_row.established_game_day,
                corporation_row.capital_krw,
                corporation_row.registration_license_tax_krw,
                corporation_row.local_education_tax_krw,
                corporation_row.game_administrative_fee_krw,
                corporation_row.total_establishment_fee_krw,
                corporation_row.cash_krw, corporation_row.contributed_capital_krw,
                corporation_row.retained_earnings_krw,
                corporation_row.operating_payable_krw,
                corporation_row.corporate_tax_payable_krw,
                corporation_row.distributable_profit_krw,
                corporation_row.personal_ledger_transaction_id,
                corporation_row.corporation_ledger_transaction_id,
                next_setting.id AS next_setting_id,
                next_scale.id AS next_operating_scale_id,
                next_scale.scale_key AS next_scale_key,
                next_scale.scale_order AS next_scale_order,
                next_scale.revenue_factor_ppm AS next_revenue_factor_ppm,
                next_scale.fixed_cost_krw AS next_fixed_cost_krw,
                YEAR(DATE_ADD(
                    DATE_ADD(world.start_date, INTERVAL current_save.game_day DAY),
                    INTERVAL 1 MONTH
                )) AS next_effective_year,
                MONTH(DATE_ADD(
                    DATE_ADD(world.start_date, INTERVAL current_save.game_day DAY),
                    INTERVAL 1 MONTH
                )) AS next_effective_month,
                COALESCE(next_setting.officer_gross_salary_krw, 0)
                    AS next_officer_gross_salary_krw,
                next_setting.created_game_day AS next_setting_created_game_day
         FROM corporation AS corporation_row
         INNER JOIN save AS current_save
           ON current_save.id = corporation_row.save_id
          AND current_save.run_revision = corporation_row.run_revision
         INNER JOIN market_world AS world ON world.id = current_save.market_world_id
         INNER JOIN corporation_industry_template AS template
           ON template.id = corporation_row.industry_template_id
          AND template.life_component_version_id
                = corporation_row.corporation_component_version_id
         LEFT JOIN corporation_operating_setting AS next_setting
           ON next_setting.id = (
                SELECT candidate.id
                FROM corporation_operating_setting AS candidate
                WHERE candidate.save_id = corporation_row.save_id
                  AND candidate.run_revision = corporation_row.run_revision
                  AND candidate.corporation_id = corporation_row.id
                  AND (
                      candidate.effective_year < YEAR(DATE_ADD(
                          DATE_ADD(world.start_date, INTERVAL current_save.game_day DAY),
                          INTERVAL 1 MONTH
                      ))
                      OR candidate.effective_year = YEAR(DATE_ADD(
                          DATE_ADD(world.start_date, INTERVAL current_save.game_day DAY),
                          INTERVAL 1 MONTH
                      ))
                      AND candidate.effective_month <= MONTH(DATE_ADD(
                          DATE_ADD(world.start_date, INTERVAL current_save.game_day DAY),
                          INTERVAL 1 MONTH
                      ))
                  )
                ORDER BY candidate.effective_year DESC, candidate.effective_month DESC,
                         candidate.id DESC
                LIMIT 1
           )
         INNER JOIN corporation_operating_scale AS next_scale
           ON next_scale.life_component_version_id
                = corporation_row.corporation_component_version_id
          AND next_scale.industry_template_id = corporation_row.industry_template_id
          AND next_scale.id = COALESCE(
                next_setting.operating_scale_id,
                (
                    SELECT default_scale.id
                    FROM corporation_operating_scale AS default_scale
                    WHERE default_scale.life_component_version_id
                            = corporation_row.corporation_component_version_id
                      AND default_scale.industry_template_id
                            = corporation_row.industry_template_id
                      AND default_scale.scale_key = 'standard'
                )
          )
         WHERE corporation_row.save_id = ? AND corporation_row.run_revision = ?{}{}",
        if corporation_id.is_some() {
            " AND corporation_row.id = ?"
        } else {
            ""
        },
        if lock { " FOR UPDATE" } else { "" }
    );
    let mut query = sqlx::query_as::<_, CorporationRow>(AssertSqlSafe(query.as_str()))
        .bind(scope.save_id)
        .bind(scope.run_revision);
    if let Some(corporation_id) = corporation_id {
        query = query.bind(corporation_id);
    }
    Ok(query.fetch_optional(&mut **tx).await?)
}

async fn has_non_terminal_insolvency(
    tx: &mut Transaction<'_, MySql>,
    scope: &ScopeRow,
) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM insolvency_case
         WHERE save_id = ? AND run_revision = ?
           AND status IN ('prepared', 'filed', 'liquidation', 'discharged', 'rebuilding')",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_one(&mut **tx)
    .await?;
    Ok(count > 0)
}

fn registration_policy(scope: &ScopeRow) -> Result<CorporationRegistrationPolicy> {
    Ok(CorporationRegistrationPolicy {
        registered_office_class: CorporationRegisteredOfficeClass::StandardRegisteredOffice,
        registration_license_tax_rate_ppm: scope
            .registration_license_tax_rate_ppm
            .context("corporation registration rate is missing")?,
        minimum_registration_license_tax_krw: scope
            .minimum_registration_license_tax_krw
            .context("corporation minimum registration tax is missing")?,
        local_education_tax_rate_ppm: scope
            .local_education_tax_rate_ppm
            .context("corporation local education tax rate is missing")?,
    })
}

fn establishment_terms(scope: &ScopeRow) -> Result<CorporationEstablishmentTerms> {
    Ok(CorporationEstablishmentTerms {
        minimum_capital_krw: scope
            .minimum_capital_krw
            .context("corporation minimum capital is missing")?,
        maximum_capital_krw: scope
            .maximum_capital_krw
            .context("corporation maximum capital is missing")?,
        game_administrative_fee_krw: scope
            .game_administrative_fee_krw
            .context("corporation administrative fee is missing")?,
    })
}

fn policy_context(scope: &ScopeRow) -> Result<RunPolicyContext> {
    Ok(RunPolicyContext {
        run: RunId {
            save_id: ResourceId::from_u64(scope.save_id),
            run_revision: scope.run_revision,
        },
        policy_set_id: ResourceId::from_u64(
            scope
                .policy_set_id
                .context("corporation policy is missing")?,
        ),
    })
}

async fn write_personal_ledger(
    tx: &mut Transaction<'_, MySql>,
    ledger: &LedgerTransaction,
    corporation_id: ResourceId,
) -> Result<u64> {
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
    let ledger_id = inserted.last_insert_id();
    for (index, posting) in ledger.postings().iter().enumerate() {
        let posting_order = u16::try_from(index + 1).context("too many corporation postings")?;
        sqlx::query(
            "INSERT INTO ledger_posting
                 (save_id, run_revision, ledger_transaction_id, posting_order,
                  account_code, financial_account_id, corporation_id, amount_krw)
             VALUES (?, ?, ?, ?, ?, NULL, ?, ?)",
        )
        .bind(policy.run.save_id.get())
        .bind(policy.run.run_revision)
        .bind(ledger_id)
        .bind(posting_order)
        .bind(to_db_str(&posting.account_code)?)
        .bind(corporation_id.get())
        .bind(posting.amount_krw)
        .execute(&mut **tx)
        .await?;
    }
    Ok(ledger_id)
}

async fn write_corporation_ledger(
    tx: &mut Transaction<'_, MySql>,
    scope: &ScopeRow,
    corporation_id: u64,
    correlation_id: &str,
    plan: &crate::life::CorporationEstablishmentPlan,
) -> Result<u64> {
    let inserted = sqlx::query(
        "INSERT INTO corporation_ledger_transaction
             (save_id, run_revision, corporation_id, game_day,
              transaction_kind, correlation_id, description)
         VALUES (?, ?, ?, ?, 'establishment', ?, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(corporation_id)
    .bind(scope.game_day)
    .bind(correlation_id)
    .bind(format!("{} 설립 출자", plan.canonical_name))
    .execute(&mut **tx)
    .await?;
    let ledger_id = inserted.last_insert_id();
    for (order, account_code, amount_krw) in [
        (1_u16, "corporationCash", plan.capital_krw),
        (2_u16, "contributedCapital", -plan.capital_krw),
    ] {
        sqlx::query(
            "INSERT INTO corporation_ledger_posting
                 (save_id, run_revision, corporation_id,
                  corporation_ledger_transaction_id, posting_order,
                  account_code, amount_krw)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(corporation_id)
        .bind(ledger_id)
        .bind(order)
        .bind(account_code)
        .bind(amount_krw)
        .execute(&mut **tx)
        .await?;
    }
    Ok(ledger_id)
}

async fn insert_transition(
    tx: &mut Transaction<'_, MySql>,
    scope: &ScopeRow,
    corporation_id: u64,
    transition: EstablishmentTransition,
    command_id: &str,
) -> Result<()> {
    let (transition_no, from_status, to_status, reason) = match transition {
        EstablishmentTransition::Draft => (1_u16, None, "draft", "playerEstablished"),
        EstablishmentTransition::Active => (2_u16, Some("draft"), "active", "establishmentFunded"),
    };
    sqlx::query(
        "INSERT INTO corporation_transition
             (save_id, run_revision, corporation_id, transition_no,
              from_status, to_status, command_id, transition_game_day, transition_reason)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(corporation_id)
    .bind(transition_no)
    .bind(from_status)
    .bind(to_status)
    .bind(command_id)
    .bind(scope.game_day)
    .bind(reason)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn write_receipt(
    tx: &mut Transaction<'_, MySql>,
    scope: &ScopeRow,
    command: &CreateCorporationCommand,
    fingerprint: &str,
    state_revision: u64,
    receipt: &CorporationReceipt,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO corporation_command_receipt
             (save_id, command_id, run_revision, state_revision, game_day,
              corporation_id, command_kind, payload_sha256,
              personal_ledger_transaction_id, corporation_ledger_transaction_id, result)
         VALUES (?, ?, ?, ?, ?, ?, 'createCorporation', ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(command.command_id.as_str())
    .bind(scope.run_revision)
    .bind(state_revision)
    .bind(scope.game_day)
    .bind(receipt.corporation.id.get())
    .bind(fingerprint)
    .bind(receipt.corporation.personal_ledger_transaction_id.get())
    .bind(receipt.corporation.corporation_ledger_transaction_id.get())
    .bind(serde_json::to_string(receipt)?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn finish_replay(
    mut tx: Transaction<'_, MySql>,
    scope: &ScopeRow,
    command: &CreateCorporationCommand,
    fingerprint: &str,
) -> Result<LifeStoreResult<CorporationReceipt>> {
    let row = sqlx::query_as::<_, ReceiptRow>(
        "SELECT command_kind, payload_sha256, CAST(result AS CHAR) AS result_json
         FROM corporation_command_receipt
         WHERE save_id = ? AND command_id = ?",
    )
    .bind(scope.save_id)
    .bind(command.command_id.as_str())
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::IdempotencyConflict,
        ));
    };
    ensure!(
        row.command_kind == COMMAND_KIND_CREATE && row.payload_sha256 == fingerprint,
        "corporation replay receipt disagrees with command identity"
    );
    let mut receipt: CorporationReceipt = serde_json::from_str(&row.result_json)?;
    receipt.replayed = true;
    let save = read_state(&mut tx, scope.save_id).await?;
    tx.commit().await?;
    Ok(LifeStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

fn corporation_summary(row: &CorporationRow) -> Result<CorporationSummaryState> {
    Ok(CorporationSummaryState {
        id: ResourceId::from_u64(row.id),
        component_version_id: ResourceId::from_u64(row.corporation_component_version_id),
        industry_template_id: ResourceId::from_u64(row.industry_template_id),
        template_key: row.template_key.clone(),
        template_display_name: row.template_display_name.clone(),
        name: row.name.clone(),
        representative_name: row.representative_name.clone(),
        status: parse_status(&row.status)?,
        established_game_day: row.established_game_day,
        capital_krw: row.capital_krw,
        registration_license_tax_krw: row.registration_license_tax_krw,
        local_education_tax_krw: row.local_education_tax_krw,
        game_administrative_fee_krw: row.game_administrative_fee_krw,
        total_establishment_fee_krw: row.total_establishment_fee_krw,
        cash_krw: row.cash_krw,
        contributed_capital_krw: row.contributed_capital_krw,
        retained_earnings_krw: row.retained_earnings_krw,
        operating_payable_krw: row.operating_payable_krw,
        corporate_tax_payable_krw: row.corporate_tax_payable_krw,
        distributable_profit_krw: row.distributable_profit_krw,
        personal_ledger_transaction_id: ResourceId::from_u64(
            row.personal_ledger_transaction_id
                .context("active corporation has no personal ledger")?,
        ),
        corporation_ledger_transaction_id: ResourceId::from_u64(
            row.corporation_ledger_transaction_id
                .context("active corporation has no corporation ledger")?,
        ),
        next_month_setting: CorporationNextMonthSettingState {
            setting_id: row.next_setting_id.map(ResourceId::from_u64),
            operating_scale_id: ResourceId::from_u64(row.next_operating_scale_id),
            scale_key: row.next_scale_key.clone(),
            scale_order: row.next_scale_order,
            revenue_factor_ppm: row.next_revenue_factor_ppm,
            fixed_cost_krw: row.next_fixed_cost_krw,
            effective_year: row.next_effective_year,
            effective_month: row.next_effective_month,
            officer_gross_salary_krw: row.next_officer_gross_salary_krw,
            created_game_day: row.next_setting_created_game_day,
        },
    })
}

fn parse_status(raw: &str) -> Result<CorporationStatusState> {
    match raw {
        "draft" => Ok(CorporationStatusState::Draft),
        "active" => Ok(CorporationStatusState::Active),
        "dormant" => Ok(CorporationStatusState::Dormant),
        "insolvent" => Ok(CorporationStatusState::Insolvent),
        "dissolved" => Ok(CorporationStatusState::Dissolved),
        _ => bail!("unknown corporation status"),
    }
}

fn has_current_cursor(scope: &ScopeRow, command: &CreateCorporationCommand) -> bool {
    scope.run_revision == command.cursor.expected_run_revision
        && scope.state_revision == command.cursor.expected_state_revision
        && scope.game_day == command.cursor.expected_game_day
}

fn has_current_settings_cursor(
    scope: &ScopeRow,
    command: &UpdateCorporationSettingsCommand,
) -> bool {
    scope.run_revision == command.cursor.expected_run_revision
        && scope.state_revision == command.cursor.expected_state_revision
        && scope.game_day == command.cursor.expected_game_day
}

fn create_fingerprint(command: &CreateCorporationCommand) -> String {
    sha256(&format!(
        "lifeledger.life.createCorporation.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\nindustryTemplateId={}\nnameBytes={}\nname={}\ncapitalKrw={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.industry_template_id,
        command.name.len(),
        command.name,
        command.capital_krw,
    ))
}

fn settings_fingerprint(command: &UpdateCorporationSettingsCommand) -> String {
    sha256(&format!(
        "lifeledger.life.updateCorporationSettings.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\ncorporationId={}\noperatingScaleId={}\nofficerGrossSalaryKrw={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.corporation_id,
        command.operating_scale_id,
        command.officer_gross_salary_krw,
    ))
}

fn dividend_fingerprint(command: &PayCorporationDividendCommand) -> String {
    sha256(&format!(
        "lifeledger.life.payCorporationDividend.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\ncorporationId={}\nkind=dividend\ngrossDividendKrw={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.corporation_id,
        command.gross_dividend_krw,
    ))
}

fn sha256(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn to_db_str<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value)? {
        Value::String(text) => Ok(text),
        other => bail!("value is not storable as a string: {other}"),
    }
}
