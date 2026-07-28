//! M4-E2a corporation establishment and separate-ledger persistence.

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{AssertSqlSafe, MySql, MySqlPool, Transaction};
use time::Date;

use super::housing::is_retryable_database_error;
use super::mysql::{
    CommandIdentitySpec, CommandIdentityState, inspect_command_identity, read_state,
    write_command_identity,
};
use super::types::{
    CorporationAvailabilityState, CorporationReadResult, CorporationReceipt,
    CorporationSnapshotState, CorporationStatusState, CorporationSummaryState,
    CorporationTemplateState, CorporationTemplatesState, CreateCorporationCommand, LifeFailureCode,
    LifeStoreResult,
};
use crate::finance::{
    FinanceRules, LedgerAccountCode, LedgerPosting, LedgerSource, LedgerSourceKind,
    LedgerTransaction, LedgerTransactionDraft, ResourceId, RunId, RunPolicyContext,
};
use crate::life::{
    CorporationError, CorporationEstablishmentInput, CorporationEstablishmentTerms,
    CorporationOperatingMonthInput, CorporationOperatingMonthPlan, CorporationOperatingScaleTerms,
    CorporationRegisteredOfficeClass, CorporationRegistrationPolicy, CorporationRules,
};

const COMPONENT_KEY: &str = "dev-unranked-m4-corporation-2026-v1";
const POLICY_KEY: &str = "dev-unranked-kr-corporation-2026-v5";
const COMMAND_KIND_CREATE: &str = "createCorporation";
const MAX_TRANSACTION_ATTEMPTS: usize = 3;

#[derive(Debug, Clone, sqlx::FromRow)]
struct ScopeRow {
    save_id: u64,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
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
    distributable_profit_krw: i64,
    personal_ledger_transaction_id: Option<u64>,
    corporation_ledger_transaction_id: Option<u64>,
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

#[derive(Debug, Clone, Copy)]
enum EstablishmentTransition {
    Draft,
    Active,
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

pub(super) async fn settle_corporation_operating_month_in_tx(
    tx: &mut Transaction<'_, MySql>,
    corporation_rules: &dyn CorporationRules,
    save_id: u64,
    run_revision: u32,
    target_game_day: u32,
    market_date: Date,
) -> Result<()> {
    if market_date.day() != 1 {
        return Ok(());
    }
    let corporation: Option<OperatingCorporationRow> = sqlx::query_as(
        "SELECT corporation_row.id,
                corporation_row.corporation_component_version_id,
                corporation_row.industry_template_id, template.template_key,
                DATE_ADD(world.start_date, INTERVAL corporation_row.established_game_day DAY)
                    AS established_date,
                world.seed AS world_seed, corporation_row.cash_krw,
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
                revenue_factor_ppm: i64::from(corporation.scale_revenue_factor_ppm),
                fixed_cost_krw: corporation.scale_fixed_cost_krw,
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
    let cash_after_krw = cash_available_krw
        .checked_sub(operating_cost_cash_paid_krw)
        .context("corporation operating cash underflowed")?;
    let operating_payable_after_krw = corporation
        .operating_payable_krw
        .checked_add(operating_cost_payable_krw)
        .context("corporation operating payable overflowed")?;
    let retained_earnings_after_krw = corporation
        .retained_earnings_krw
        .checked_add(plan.pre_payroll_profit_krw)
        .context("corporation retained earnings overflowed")?;
    ensure!(
        cash_after_krw <= crate::life::CORPORATION_MAX_PUBLIC_MONEY_KRW
            && operating_payable_after_krw <= crate::life::CORPORATION_MAX_PUBLIC_MONEY_KRW
            && retained_earnings_after_krw.abs() <= crate::life::CORPORATION_MAX_PUBLIC_MONEY_KRW,
        "corporation operating result exceeds the public money range"
    );
    let employment_industry = employment_industry(&corporation.template_key)?;

    let inserted = sqlx::query(
        "INSERT INTO corporation_operating_month
             (save_id, run_revision, corporation_id,
              corporation_component_version_id, industry_template_id, operating_scale_id,
              operating_year, operating_month, entropy_stream, entropy_word, shock_ppm,
              employment_industry, base_monthly_revenue_krw, revenue_variation_ppm,
              variable_cost_ppm, base_fixed_cost_krw, scale_revenue_factor_ppm,
              scale_fixed_cost_krw, officer_gross_salary_krw,
              revenue_krw, variable_cost_krw, operating_expense_krw,
              pre_payroll_profit_krw, cash_before_krw, operating_cost_cash_paid_krw,
              operating_cost_payable_krw, cash_after_krw,
              operating_payable_before_krw, operating_payable_after_krw,
              retained_earnings_before_krw, retained_earnings_after_krw,
              applied_game_day, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0,
                 ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'preparing')",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(corporation.id)
    .bind(corporation.corporation_component_version_id)
    .bind(corporation.industry_template_id)
    .bind(corporation.operating_scale_id)
    .bind(operating_year)
    .bind(operating_month)
    .bind(plan.entropy_word)
    .bind(plan.shock_ppm)
    .bind(employment_industry)
    .bind(corporation.base_monthly_revenue_krw)
    .bind(corporation.revenue_variation_ppm)
    .bind(corporation.variable_cost_ppm)
    .bind(plan.base_fixed_cost_krw)
    .bind(corporation.scale_revenue_factor_ppm)
    .bind(plan.scale_fixed_cost_krw)
    .bind(plan.revenue_krw)
    .bind(plan.variable_cost_krw)
    .bind(plan.operating_expense_krw)
    .bind(plan.pre_payroll_profit_krw)
    .bind(corporation.cash_krw)
    .bind(operating_cost_cash_paid_krw)
    .bind(operating_cost_payable_krw)
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
    let next_status = if operating_cost_payable_krw > 0 {
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
    if operating_cost_payable_krw > 0 {
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
             status = 'applied', applied_at = CURRENT_TIMESTAMP(3)
         WHERE id = ? AND status = 'preparing'",
    )
    .bind(revenue_ledger_transaction_id)
    .bind(expense_ledger_transaction_id)
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
                save.game_day, save.cash_krw,
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
    Ok(CorporationTemplatesState {
        availability: CorporationAvailabilityState::Active,
        component_version_id: Some(ResourceId::from_u64(component_id)),
        registered_office_class: scope.registered_office_class.clone(),
        minimum_capital_krw: scope.minimum_capital_krw,
        maximum_capital_krw: scope.maximum_capital_krw,
        game_administrative_fee_krw: scope.game_administrative_fee_krw,
        templates: rows.into_iter().map(template_state).collect(),
    })
}

fn template_state(row: TemplateRow) -> CorporationTemplateState {
    CorporationTemplateState {
        id: ResourceId::from_u64(row.id),
        template_key: row.template_key,
        display_name: row.display_name,
        template_order: row.template_order,
        base_monthly_revenue_krw: row.base_monthly_revenue_krw,
        revenue_variation_ppm: row.revenue_variation_ppm,
        variable_cost_ppm: row.variable_cost_ppm,
        fixed_monthly_cost_krw: row.fixed_monthly_cost_krw,
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
                corporation_row.distributable_profit_krw,
                corporation_row.personal_ledger_transaction_id,
                corporation_row.corporation_ledger_transaction_id
         FROM corporation AS corporation_row
         INNER JOIN corporation_industry_template AS template
           ON template.id = corporation_row.industry_template_id
          AND template.life_component_version_id
                = corporation_row.corporation_component_version_id
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
        distributable_profit_krw: row.distributable_profit_krw,
        personal_ledger_transaction_id: ResourceId::from_u64(
            row.personal_ledger_transaction_id
                .context("active corporation has no personal ledger")?,
        ),
        corporation_ledger_transaction_id: ResourceId::from_u64(
            row.corporation_ledger_transaction_id
                .context("active corporation has no corporation ledger")?,
        ),
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

fn sha256(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn to_db_str<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value)? {
        Value::String(text) => Ok(text),
        other => bail!("value is not storable as a string: {other}"),
    }
}
