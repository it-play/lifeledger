use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{AssertSqlSafe, MySql, MySqlPool, Transaction};
use time::{Date, Duration};

use super::mysql::{
    CommandIdentitySpec, CommandIdentityState, inspect_command_identity, read_state,
    write_command_identity,
};
use super::types::{
    BusinessContractState, BusinessContractStatusState, BusinessLoanProductState,
    BusinessMarketingBandState, BusinessMonthState, BusinessMonthlyPlanState,
    BusinessOperationAction, BusinessOperationReceipt, BusinessOperationResultState,
    BusinessOperationsAvailabilityState, BusinessOperationsState, BusinessPositionState,
    BusinessPositionStatusState, BusinessWorkingCapitalLoanState,
    BusinessWorkingCapitalLoanStatusState, CorporationReadResult, LifeFailureCode, LifeStoreResult,
    ManageBusinessOperationsCommand,
};
use crate::finance::{
    CommandCursor, FinanceRules, LedgerAccountCode, LedgerPosting, LedgerSource, LedgerSourceKind,
    LedgerTransaction, LedgerTransactionDraft, ResourceId, RunId, RunPolicyContext,
};
use crate::life::{
    BusinessContractMonthInput, BusinessContractMonthOutcome, BusinessEmployeeMonthInput,
    BusinessMonthInput, BusinessMonthPlan, BusinessOperationsRules,
};

const MAX_TRANSACTION_ATTEMPTS: usize = 3;

#[derive(Debug, Clone, sqlx::FromRow)]
struct OperationScopeRow {
    save_id: u64,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
    wallet_cash_krw: i64,
    policy_set_id: u64,
    corporation_id: u64,
    corporation_status: String,
    corporation_cash_krw: i64,
    contributed_capital_krw: i64,
    retained_earnings_krw: i64,
    operating_payable_krw: i64,
    corporate_tax_payable_krw: i64,
    distributable_profit_krw: i64,
    business_catalog_version_id: Option<u64>,
    business_catalog_sha256: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct BusinessProfileRow {
    id: u64,
    business_catalog_version_id: u64,
    business_catalog_sha256: String,
    effective_year: u16,
    effective_month: u8,
    control_revision: u64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct RoleSeedRow {
    id: u64,
    maximum_positions: u16,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ContractTemplateSeedRow {
    id: u64,
    required_capacity_units: u16,
    revenue_krw: i64,
    variable_cost_ppm: u32,
    failure_penalty_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ContractRow {
    id: u64,
    template_key: String,
    display_name: String,
    status: String,
    service_year: u16,
    service_month: u8,
    required_capacity_units: u16,
    revenue_krw: i64,
    variable_cost_ppm: u32,
    failure_penalty_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PositionRow {
    id: u64,
    role_key: String,
    display_name: String,
    status: String,
    capacity_units: u16,
    monthly_gross_wage_krw: i64,
    employer_cost_rate_ppm: u32,
    effective_year: Option<u16>,
    effective_month: Option<u8>,
    hired_command_id: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct MarketingRow {
    id: u64,
    band_key: String,
    display_name: String,
    band_order: u16,
    monthly_cost_krw: i64,
    offer_slots: u16,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LoanProductRow {
    id: u64,
    product_key: String,
    display_name: String,
    minimum_principal_krw: i64,
    maximum_principal_krw: i64,
    principal_step_krw: i64,
    monthly_interest_rate_ppm: u32,
    term_months: u16,
    personal_guarantee: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WorkingCapitalLoanRow {
    id: u64,
    loan_product_id: u64,
    product_key: String,
    display_name: String,
    status: String,
    original_principal_krw: i64,
    outstanding_principal_krw: i64,
    monthly_interest_rate_ppm: u32,
    term_months: u16,
    originated_year: u16,
    originated_month: u8,
    maturity_year: u16,
    maturity_month: u8,
    personal_guarantee: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PlanRow {
    id: u64,
    effective_year: u16,
    effective_month: u8,
    plan_revision: u64,
    marketing_band_id: u64,
    marketing_band_key: String,
    cash_buffer_krw: i64,
    contract_priority_json: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct BusinessMonthRow {
    id: u64,
    operating_year: u16,
    operating_month: u8,
    total_capacity_units: u32,
    used_capacity_units: u32,
    contract_revenue_krw: i64,
    contract_variable_cost_krw: i64,
    marketing_cost_krw: i64,
    employee_gross_wage_krw: i64,
    employee_employer_cost_krw: i64,
    failed_contract_penalty_krw: i64,
    loan_interest_cost_krw: i64,
    completed_contract_count: u16,
    failed_contract_count: u16,
    active_employee_count: u16,
    applied_game_day: u32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ReceiptRow {
    command_kind: String,
    payload_sha256: String,
    result_json: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct MonthlyPlanRow {
    id: u64,
    marketing_cost_krw: i64,
    offer_slots: u16,
    cash_buffer_krw: i64,
    contract_priority_json: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct MonthlyEmployeeRow {
    id: u64,
    status: String,
    capacity_units: u16,
    monthly_gross_wage_krw: i64,
    employer_cost_rate_ppm: u32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct MonthlyContractRow {
    id: u64,
    status: String,
    required_capacity_units: u16,
    revenue_krw: i64,
    variable_cost_ppm: u32,
    failure_penalty_krw: i64,
}

pub(super) struct PreparedBusinessMonth {
    profile_id: u64,
    catalog_id: u64,
    monthly_plan_id: Option<u64>,
    offer_slots: u16,
    industry_template_key: String,
    world_seed: u64,
    operating_year: u16,
    operating_month: u8,
    hired_position_ids: Vec<u64>,
    accepted_contract_ids: Vec<u64>,
    pub loan_interest_cost_krw: i64,
    pub plan: BusinessMonthPlan,
}

pub(super) async fn initialize_business_profile_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    corporation_id: u64,
    industry_template_key: &str,
    current_date: Date,
) -> Result<()> {
    let manifest: Option<(u64, String, u64, u64)> = sqlx::query_as(
        "SELECT manifest.business_catalog_version_id, manifest.business_catalog_sha256,
                manifest.career_catalog_bundle_id, world.seed
         FROM run_manifest AS manifest
         INNER JOIN save ON save.id = manifest.save_id
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         INNER JOIN business_catalog_version AS catalog
            ON catalog.id = manifest.business_catalog_version_id
           AND BINARY catalog.canonical_sha256 = BINARY manifest.business_catalog_sha256
           AND catalog.status = 'sealed'
         WHERE manifest.save_id = ? AND manifest.run_revision = ?
         FOR SHARE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((catalog_id, catalog_sha256, career_catalog_bundle_id, world_seed)) = manifest else {
        return Ok(());
    };
    let next_month = next_month(current_date)?;
    let effective_year = u16::try_from(next_month.year())
        .context("business profile effective year is out of range")?;
    let effective_month = u8::from(next_month.month());
    let inserted = sqlx::query(
        "INSERT INTO corporation_business_profile
             (save_id, run_revision, corporation_id, business_catalog_version_id,
              business_catalog_sha256, effective_year, effective_month, control_revision)
         VALUES (?, ?, ?, ?, ?, ?, ?, 1)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(corporation_id)
    .bind(catalog_id)
    .bind(&catalog_sha256)
    .bind(effective_year)
    .bind(effective_month)
    .execute(&mut **tx)
    .await?;
    let profile_id = inserted.last_insert_id();

    let roles: Vec<RoleSeedRow> = sqlx::query_as(
        "SELECT id, maximum_positions
         FROM business_role_template
         WHERE business_catalog_version_id = ?
           AND industry_template_key = ?
           AND career_catalog_bundle_id = ?
         ORDER BY role_order, id",
    )
    .bind(catalog_id)
    .bind(industry_template_key)
    .bind(career_catalog_bundle_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        !roles.is_empty(),
        "business catalog has no compatible roles"
    );
    for role in roles {
        for position_no in 1..=role.maximum_positions {
            sqlx::query(
                "INSERT INTO corporation_staff_position
                     (save_id, run_revision, corporation_id, business_profile_id,
                      business_catalog_version_id, role_template_id, position_no, status)
                 VALUES (?, ?, ?, ?, ?, ?, ?, 'vacant')",
            )
            .bind(save_id)
            .bind(run_revision)
            .bind(corporation_id)
            .bind(profile_id)
            .bind(catalog_id)
            .bind(role.id)
            .bind(position_no)
            .execute(&mut **tx)
            .await?;
        }
    }

    materialize_contract_offers(
        tx,
        OfferMaterialization {
            save_id,
            run_revision,
            corporation_id,
            profile_id,
            catalog_id,
            industry_template_key,
            world_seed,
            offered_year: u16::try_from(current_date.year())
                .context("business offer year is out of range")?,
            offered_month: u8::from(current_date.month()),
            service_year: effective_year,
            service_month: effective_month,
            offer_slots: 1,
        },
    )
    .await
}

struct OfferMaterialization<'a> {
    save_id: u64,
    run_revision: u32,
    corporation_id: u64,
    profile_id: u64,
    catalog_id: u64,
    industry_template_key: &'a str,
    world_seed: u64,
    offered_year: u16,
    offered_month: u8,
    service_year: u16,
    service_month: u8,
    offer_slots: u16,
}

async fn materialize_contract_offers(
    tx: &mut Transaction<'_, MySql>,
    context: OfferMaterialization<'_>,
) -> Result<()> {
    let templates: Vec<ContractTemplateSeedRow> = sqlx::query_as(
        "SELECT id, required_capacity_units, revenue_krw,
                variable_cost_ppm, failure_penalty_krw
         FROM business_contract_template
         WHERE business_catalog_version_id = ? AND industry_template_key = ?
         ORDER BY template_order, id",
    )
    .bind(context.catalog_id)
    .bind(context.industry_template_key)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        !templates.is_empty(),
        "business catalog has no compatible contracts"
    );
    for occurrence_no in 1..=context.offer_slots {
        let template_index = usize::from(occurrence_no - 1) % templates.len();
        let template = templates
            .get(template_index)
            .context("business contract template index is invalid")?;
        let entropy_word = offer_entropy_word(
            context.world_seed,
            context.corporation_id,
            context.service_year,
            context.service_month,
            template.id,
            occurrence_no,
        );
        sqlx::query(
            "INSERT INTO corporation_customer_contract
                 (save_id, run_revision, corporation_id, business_profile_id,
                  business_catalog_version_id, contract_template_id, occurrence_no, status,
                  offered_year, offered_month, service_year, service_month, offer_entropy_word,
                  required_capacity_units, revenue_krw, variable_cost_ppm, failure_penalty_krw)
             VALUES (?, ?, ?, ?, ?, ?, ?, 'offered', ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(context.save_id)
        .bind(context.run_revision)
        .bind(context.corporation_id)
        .bind(context.profile_id)
        .bind(context.catalog_id)
        .bind(template.id)
        .bind(occurrence_no)
        .bind(context.offered_year)
        .bind(context.offered_month)
        .bind(context.service_year)
        .bind(context.service_month)
        .bind(entropy_word)
        .bind(template.required_capacity_units)
        .bind(template.revenue_krw)
        .bind(template.variable_cost_ppm)
        .bind(template.failure_penalty_krw)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn prepare_business_month_in_tx(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn BusinessOperationsRules,
    save_id: u64,
    run_revision: u32,
    corporation_id: u64,
    industry_template_key: &str,
    world_seed: u64,
    operating_year: u16,
    operating_month: u8,
    owner_capacity_units: u16,
) -> Result<Option<PreparedBusinessMonth>> {
    let profile: Option<BusinessProfileRow> = sqlx::query_as(
        "SELECT id, business_catalog_version_id, business_catalog_sha256,
                effective_year, effective_month, control_revision
         FROM corporation_business_profile
         WHERE save_id = ? AND run_revision = ? AND corporation_id = ?
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(corporation_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(profile) = profile else {
        return Ok(None);
    };
    ensure!(
        (profile.effective_year, profile.effective_month) == (operating_year, operating_month),
        "business profile is not aligned with the operating month"
    );
    let loan_interest_rows: Vec<(i64, u32)> = sqlx::query_as(
        "SELECT outstanding_principal_krw, monthly_interest_rate_ppm
         FROM corporation_working_capital_loan
         WHERE save_id = ? AND run_revision = ? AND corporation_id = ?
           AND status IN ('active', 'matured')
         ORDER BY id FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(corporation_id)
    .fetch_all(&mut **tx)
    .await?;
    let loan_interest_cost_krw = loan_interest_rows.into_iter().try_fold(
        0_i64,
        |total, (outstanding_principal_krw, monthly_interest_rate_ppm)| {
            let interest = i128::from(outstanding_principal_krw)
                .checked_mul(i128::from(monthly_interest_rate_ppm))
                .context("working capital interest overflowed")?
                / 1_000_000;
            let interest = i64::try_from(interest)?;
            total
                .checked_add(interest)
                .context("working capital interest total overflowed")
        },
    )?;
    let selected_plan: Option<MonthlyPlanRow> = sqlx::query_as(
        "SELECT plan.id, marketing.monthly_cost_krw, marketing.offer_slots,
                plan.cash_buffer_krw,
                CAST(plan.contract_priority_json AS CHAR) AS contract_priority_json
         FROM corporation_monthly_plan AS plan
         INNER JOIN business_marketing_band AS marketing
            ON marketing.business_catalog_version_id = plan.business_catalog_version_id
           AND marketing.id = plan.marketing_band_id
         WHERE plan.save_id = ? AND plan.run_revision = ? AND plan.corporation_id = ?
           AND plan.effective_year = ? AND plan.effective_month = ?
         ORDER BY plan.plan_revision DESC, plan.id DESC LIMIT 1
         FOR SHARE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(corporation_id)
    .bind(operating_year)
    .bind(operating_month)
    .fetch_optional(&mut **tx)
    .await?;
    let (monthly_plan_id, marketing_cost_krw, offer_slots, cash_buffer_krw, priority_ids) =
        if let Some(plan) = selected_plan {
            let raw_ids: Vec<String> = serde_json::from_str(&plan.contract_priority_json)?;
            let priority_ids = raw_ids
                .into_iter()
                .map(|raw| {
                    raw.parse::<u64>()
                        .context("business plan contract ID is invalid")
                })
                .collect::<Result<Vec<_>>>()?;
            (
                Some(plan.id),
                plan.marketing_cost_krw,
                plan.offer_slots,
                plan.cash_buffer_krw,
                priority_ids,
            )
        } else {
            let default_marketing: (i64, u16) = sqlx::query_as(
                "SELECT monthly_cost_krw, offer_slots
                 FROM business_marketing_band
                 WHERE business_catalog_version_id = ? AND band_key = 'off'
                 FOR SHARE",
            )
            .bind(profile.business_catalog_version_id)
            .fetch_one(&mut **tx)
            .await?;
            (
                None,
                default_marketing.0,
                default_marketing.1,
                0,
                Vec::new(),
            )
        };
    let employee_rows: Vec<MonthlyEmployeeRow> = sqlx::query_as(
        "SELECT position.id, position.status, role.capacity_units,
                role.monthly_gross_wage_krw, role.employer_cost_rate_ppm
         FROM corporation_staff_position AS position
         INNER JOIN business_role_template AS role
            ON role.business_catalog_version_id = position.business_catalog_version_id
           AND role.id = position.role_template_id
         WHERE position.save_id = ? AND position.run_revision = ?
           AND position.corporation_id = ? AND position.status IN ('hired', 'active')
           AND (position.effective_year < ?
                OR (position.effective_year = ? AND position.effective_month <= ?))
         ORDER BY position.id
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(corporation_id)
    .bind(operating_year)
    .bind(operating_year)
    .bind(operating_month)
    .fetch_all(&mut **tx)
    .await?;
    let contract_rows: Vec<MonthlyContractRow> = sqlx::query_as(
        "SELECT id, status, required_capacity_units, revenue_krw,
                variable_cost_ppm, failure_penalty_krw
         FROM corporation_customer_contract
         WHERE save_id = ? AND run_revision = ? AND corporation_id = ?
           AND service_year = ? AND service_month = ?
           AND status IN ('accepted', 'active')
         ORDER BY id
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(corporation_id)
    .bind(operating_year)
    .bind(operating_month)
    .fetch_all(&mut **tx)
    .await?;

    let contract_ids = contract_rows
        .iter()
        .map(|row| row.id)
        .collect::<BTreeSet<_>>();
    let mut priority_by_id = BTreeMap::new();
    let mut next_rank = 1_u16;
    for id in priority_ids {
        if contract_ids.contains(&id) {
            ensure!(
                priority_by_id.insert(id, next_rank).is_none(),
                "business plan priority is duplicated"
            );
            next_rank = next_rank
                .checked_add(1)
                .context("business contract priority overflowed")?;
        }
    }
    for id in &contract_ids {
        if !priority_by_id.contains_key(id) {
            priority_by_id.insert(*id, next_rank);
            next_rank = next_rank
                .checked_add(1)
                .context("business contract priority overflowed")?;
        }
    }
    let contracts = contract_rows
        .iter()
        .map(|row| {
            Ok(BusinessContractMonthInput {
                contract_id: ResourceId::from_u64(row.id),
                priority_rank: *priority_by_id
                    .get(&row.id)
                    .context("business contract priority is missing")?,
                required_capacity_units: row.required_capacity_units,
                revenue_krw: row.revenue_krw,
                variable_cost_ppm: row.variable_cost_ppm,
                failure_penalty_krw: row.failure_penalty_krw,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let employees = employee_rows
        .iter()
        .map(|row| BusinessEmployeeMonthInput {
            position_id: ResourceId::from_u64(row.id),
            capacity_units: row.capacity_units,
            gross_wage_krw: row.monthly_gross_wage_krw,
            employer_cost_rate_ppm: row.employer_cost_rate_ppm,
        })
        .collect::<Vec<_>>();
    let plan = rules
        .plan_month(BusinessMonthInput {
            owner_capacity_units,
            marketing_cost_krw,
            cash_buffer_krw,
            contracts: &contracts,
            employees: &employees,
        })
        .context("business operating month calculation failed")?;
    Ok(Some(PreparedBusinessMonth {
        profile_id: profile.id,
        catalog_id: profile.business_catalog_version_id,
        monthly_plan_id,
        offer_slots,
        industry_template_key: industry_template_key.to_owned(),
        world_seed,
        operating_year,
        operating_month,
        hired_position_ids: employee_rows
            .iter()
            .filter(|row| row.status == "hired")
            .map(|row| row.id)
            .collect(),
        accepted_contract_ids: contract_rows
            .iter()
            .filter(|row| row.status == "accepted")
            .map(|row| row.id)
            .collect(),
        loan_interest_cost_krw,
        plan,
    }))
}

pub(super) async fn apply_business_month_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    corporation_id: u64,
    operating_month_id: u64,
    target_game_day: u32,
    prepared: &PreparedBusinessMonth,
) -> Result<()> {
    let plan = &prepared.plan;
    sqlx::query(
        "INSERT INTO corporation_business_month
             (save_id, run_revision, corporation_id, business_profile_id,
              business_catalog_version_id, corporation_operating_month_id, monthly_plan_id,
              operating_year, operating_month, owner_capacity_units,
              employee_capacity_units, total_capacity_units, used_capacity_units,
              marketing_cost_krw, employee_gross_wage_krw, employee_employer_cost_krw,
              contract_revenue_krw, contract_variable_cost_krw, failed_contract_penalty_krw,
              loan_interest_cost_krw,
              receivable_opening_krw, receivable_created_krw, receivable_collected_krw,
              receivable_closing_krw, completed_contract_count, failed_contract_count,
              active_employee_count, cash_buffer_krw, applied_game_day)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                 0, ?, ?, 0, ?, ?, ?, ?, ?)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(corporation_id)
    .bind(prepared.profile_id)
    .bind(prepared.catalog_id)
    .bind(operating_month_id)
    .bind(prepared.monthly_plan_id)
    .bind(prepared.operating_year)
    .bind(prepared.operating_month)
    .bind(plan.owner_capacity_units)
    .bind(plan.employee_capacity_units)
    .bind(plan.total_capacity_units)
    .bind(plan.used_capacity_units)
    .bind(plan.marketing_cost_krw)
    .bind(plan.employee_gross_wage_krw)
    .bind(plan.employee_employer_cost_krw)
    .bind(plan.contract_revenue_krw)
    .bind(plan.contract_variable_cost_krw)
    .bind(plan.failed_contract_penalty_krw)
    .bind(prepared.loan_interest_cost_krw)
    .bind(plan.receivable_created_krw)
    .bind(plan.receivable_collected_krw)
    .bind(plan.completed_contract_count)
    .bind(plan.failed_contract_count)
    .bind(plan.active_employee_count)
    .bind(plan.cash_buffer_krw)
    .bind(target_game_day)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        "UPDATE corporation_working_capital_loan
         SET status = 'matured'
         WHERE save_id = ? AND run_revision = ? AND corporation_id = ? AND status = 'active'
           AND (maturity_year < ? OR (maturity_year = ? AND maturity_month <= ?))",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(corporation_id)
    .bind(prepared.operating_year)
    .bind(prepared.operating_year)
    .bind(prepared.operating_month)
    .execute(&mut **tx)
    .await?;

    for position_id in &prepared.hired_position_ids {
        sqlx::query(
            "INSERT INTO corporation_staff_transition
                 (save_id, run_revision, corporation_id, position_id, transition_no,
                  from_status, to_status, command_id, effective_year, effective_month,
                  transition_game_day)
             VALUES (?, ?, ?, ?, 2, 'hired', 'active', NULL, ?, ?, ?)",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(corporation_id)
        .bind(position_id)
        .bind(prepared.operating_year)
        .bind(prepared.operating_month)
        .bind(target_game_day)
        .execute(&mut **tx)
        .await?;
        let updated = sqlx::query(
            "UPDATE corporation_staff_position SET status = 'active'
             WHERE id = ? AND save_id = ? AND run_revision = ?
               AND corporation_id = ? AND status = 'hired'",
        )
        .bind(position_id)
        .bind(save_id)
        .bind(run_revision)
        .bind(corporation_id)
        .execute(&mut **tx)
        .await?;
        ensure!(
            updated.rows_affected() == 1,
            "business position activation failed"
        );
    }
    for contract_id in &prepared.accepted_contract_ids {
        sqlx::query(
            "INSERT INTO corporation_contract_transition
                 (save_id, run_revision, corporation_id, contract_id, transition_no,
                  from_status, to_status, command_id, transition_game_day)
             VALUES (?, ?, ?, ?, 2, 'accepted', 'active', NULL, ?)",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(corporation_id)
        .bind(contract_id)
        .bind(target_game_day)
        .execute(&mut **tx)
        .await?;
        let updated = sqlx::query(
            "UPDATE corporation_customer_contract SET status = 'active'
             WHERE id = ? AND save_id = ? AND run_revision = ?
               AND corporation_id = ? AND status = 'accepted'",
        )
        .bind(contract_id)
        .bind(save_id)
        .bind(run_revision)
        .bind(corporation_id)
        .execute(&mut **tx)
        .await?;
        ensure!(
            updated.rows_affected() == 1,
            "business contract activation failed"
        );
    }
    for contract in &plan.contract_plans {
        let terminal_status = match contract.outcome {
            BusinessContractMonthOutcome::Completed => "completed",
            BusinessContractMonthOutcome::Failed => "failed",
        };
        sqlx::query(
            "INSERT INTO corporation_contract_transition
                 (save_id, run_revision, corporation_id, contract_id, transition_no,
                  from_status, to_status, command_id, transition_game_day)
             VALUES (?, ?, ?, ?, 3, 'active', ?, NULL, ?)",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(corporation_id)
        .bind(contract.contract_id.get())
        .bind(terminal_status)
        .bind(target_game_day)
        .execute(&mut **tx)
        .await?;
        let updated = sqlx::query(
            "UPDATE corporation_customer_contract
             SET status = ?, terminal_game_day = ?
             WHERE id = ? AND save_id = ? AND run_revision = ?
               AND corporation_id = ? AND status = 'active'",
        )
        .bind(terminal_status)
        .bind(target_game_day)
        .bind(contract.contract_id.get())
        .bind(save_id)
        .bind(run_revision)
        .bind(corporation_id)
        .execute(&mut **tx)
        .await?;
        ensure!(
            updated.rows_affected() == 1,
            "business contract completion failed"
        );
    }
    let (next_year, next_month) =
        next_year_month(prepared.operating_year, prepared.operating_month)?;
    let updated = sqlx::query(
        "UPDATE corporation_business_profile
         SET effective_year = ?, effective_month = ?
         WHERE id = ? AND effective_year = ? AND effective_month = ?",
    )
    .bind(next_year)
    .bind(next_month)
    .bind(prepared.profile_id)
    .bind(prepared.operating_year)
    .bind(prepared.operating_month)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "business profile month advance failed"
    );
    materialize_contract_offers(
        tx,
        OfferMaterialization {
            save_id,
            run_revision,
            corporation_id,
            profile_id: prepared.profile_id,
            catalog_id: prepared.catalog_id,
            industry_template_key: &prepared.industry_template_key,
            world_seed: prepared.world_seed,
            offered_year: prepared.operating_year,
            offered_month: prepared.operating_month,
            service_year: next_year,
            service_month: next_month,
            offer_slots: prepared.offer_slots,
        },
    )
    .await
}

fn next_year_month(year: u16, month: u8) -> Result<(u16, u8)> {
    ensure!((1..=12).contains(&month), "business month is invalid");
    if month == 12 {
        Ok((year.checked_add(1).context("business year overflowed")?, 1))
    } else {
        Ok((year, month + 1))
    }
}

fn offer_entropy_word(
    world_seed: u64,
    corporation_id: u64,
    year: u16,
    month: u8,
    template_id: u64,
    occurrence_no: u16,
) -> u64 {
    let canonical = format!(
        "lifeledger.corporation.offer.v1\0{corporation_id}\0{year}-{month:02}\0{template_id}\0{occurrence_no}\0offer"
    );
    let mut hasher = Sha256::new();
    hasher.update(world_seed.to_be_bytes());
    hasher.update(canonical.as_bytes());
    let digest = hasher.finalize();
    u64::from_be_bytes(digest[..8].try_into().expect("SHA-256 has eight bytes"))
}

pub(super) async fn read_corporation_operations(
    pool: &MySqlPool,
    user_id: u64,
    corporation_id: ResourceId,
) -> Result<CorporationReadResult<BusinessOperationsState>> {
    let mut tx = pool.begin().await?;
    let Some(scope) = read_scope(&mut tx, user_id, corporation_id.get(), false).await? else {
        tx.commit().await?;
        return Ok(CorporationReadResult::Rejected(
            LifeFailureCode::CorporationResourceNotFound,
        ));
    };
    let state = read_operations_state(&mut tx, &scope).await?;
    tx.commit().await?;
    Ok(CorporationReadResult::Found(state))
}

pub(super) async fn manage_corporation_operations(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    user_id: u64,
    command: &ManageBusinessOperationsCommand,
) -> Result<LifeStoreResult<BusinessOperationReceipt>> {
    for _ in 0..MAX_TRANSACTION_ATTEMPTS {
        match manage_once(pool, finance_rules, user_id, command).await {
            Ok(result) => return Ok(result),
            Err(error) if super::housing::is_retryable_database_error(&error) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy))
}

async fn manage_once(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    user_id: u64,
    command: &ManageBusinessOperationsCommand,
) -> Result<LifeStoreResult<BusinessOperationReceipt>> {
    let command_kind = action_kind(&command.action);
    let fingerprint = operation_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(scope) = read_scope(&mut tx, user_id, command.corporation_id.get(), true).await?
    else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CorporationResourceNotFound,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind,
        payload_sha256: &fingerprint,
        cursor: command.cursor,
    };
    match inspect_command_identity(&mut tx, scope.save_id, &identity).await? {
        CommandIdentityState::Matching => {
            return finish_replay(tx, &scope, command, command_kind, &fingerprint).await;
        }
        CommandIdentityState::Conflict => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(
                LifeFailureCode::IdempotencyConflict,
            ));
        }
        CommandIdentityState::Missing => {}
    }
    if !operation_status_allowed(&scope.corporation_status, &command.action)
        || !has_current_cursor(&scope, command.cursor)
    {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CorporationStateConflict,
        ));
    }
    let Some(profile) = read_profile(&mut tx, &scope, true).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CorporationStateConflict,
        ));
    };
    if profile.control_revision != command.expected_revision {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CorporationStateConflict,
        ));
    }
    write_command_identity(&mut tx, scope.save_id, &identity).await?;
    let Some(result) =
        apply_operation_action(&mut tx, finance_rules, &scope, &profile, command).await?
    else {
        tx.rollback().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CorporationStateConflict,
        ));
    };
    if matches!(&command.action, BusinessOperationAction::Dissolve) {
        insert_dissolution_transition(&mut tx, &scope, command).await?;
    }
    let revision = profile
        .control_revision
        .checked_add(1)
        .context("business operations revision overflowed")?;
    let updated_profile = sqlx::query(
        "UPDATE corporation_business_profile SET control_revision = ?
         WHERE id = ? AND control_revision = ?",
    )
    .bind(revision)
    .bind(profile.id)
    .bind(profile.control_revision)
    .execute(&mut *tx)
    .await?;
    ensure!(
        updated_profile.rows_affected() == 1,
        "business profile changed during command"
    );
    let next_state_revision = scope
        .state_revision
        .checked_add(1)
        .context("business operation state revision overflowed")?;
    let updated_save = sqlx::query(
        "UPDATE save SET state_revision = ?
         WHERE id = ? AND run_revision = ? AND state_revision = ? AND game_day = ?",
    )
    .bind(next_state_revision)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.state_revision)
    .bind(scope.game_day)
    .execute(&mut *tx)
    .await?;
    ensure!(
        updated_save.rows_affected() == 1,
        "business operation cursor changed"
    );
    let receipt = BusinessOperationReceipt {
        command_id: command.command_id.clone(),
        result,
        revision,
        replayed: false,
    };
    sqlx::query(
        "INSERT INTO corporation_operation_command_receipt
             (save_id, command_id, run_revision, corporation_id,
              command_kind, payload_sha256, result_json)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(command.command_id.as_str())
    .bind(scope.run_revision)
    .bind(scope.corporation_id)
    .bind(command_kind)
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

async fn apply_operation_action(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    scope: &OperationScopeRow,
    profile: &BusinessProfileRow,
    command: &ManageBusinessOperationsCommand,
) -> Result<Option<BusinessOperationResultState>> {
    let result = match &command.action {
        BusinessOperationAction::AcceptContract { contract_id } => {
            let Some(contract) = accept_contract(tx, scope, profile, command, *contract_id).await?
            else {
                return Ok(None);
            };
            BusinessOperationResultState::AcceptContract { contract }
        }
        BusinessOperationAction::CancelContract { contract_id } => {
            let Some(contract) = cancel_contract(tx, scope, command, *contract_id).await? else {
                return Ok(None);
            };
            BusinessOperationResultState::CancelContract { contract }
        }
        BusinessOperationAction::HirePosition { position_id } => {
            let Some(position) = hire_position(tx, scope, profile, command, *position_id).await?
            else {
                return Ok(None);
            };
            BusinessOperationResultState::HirePosition { position }
        }
        BusinessOperationAction::TerminatePosition { position_id } => {
            let Some(position) =
                terminate_position(tx, scope, profile, command, *position_id).await?
            else {
                return Ok(None);
            };
            BusinessOperationResultState::TerminatePosition { position }
        }
        BusinessOperationAction::SetMonthlyPlan {
            marketing_band_id,
            cash_buffer_krw,
            contract_priority_ids,
        } => {
            let Some(plan) = set_monthly_plan(
                tx,
                scope,
                profile,
                command,
                *marketing_band_id,
                *cash_buffer_krw,
                contract_priority_ids,
            )
            .await?
            else {
                return Ok(None);
            };
            BusinessOperationResultState::SetMonthlyPlan { plan }
        }
        BusinessOperationAction::CapitalContribution { amount_krw } => {
            let Some(result) =
                capital_contribution(tx, finance_rules, scope, command, *amount_krw).await?
            else {
                return Ok(None);
            };
            result
        }
        BusinessOperationAction::DrawWorkingCapitalLoan {
            loan_product_id,
            principal_krw,
        } => {
            let Some(loan) = draw_working_capital_loan(
                tx,
                scope,
                profile,
                command,
                *loan_product_id,
                *principal_krw,
            )
            .await?
            else {
                return Ok(None);
            };
            BusinessOperationResultState::DrawWorkingCapitalLoan { loan }
        }
        BusinessOperationAction::RepayWorkingCapitalLoan {
            loan_id,
            principal_krw,
        } => {
            let Some(loan) =
                repay_working_capital_loan(tx, scope, command, *loan_id, *principal_krw).await?
            else {
                return Ok(None);
            };
            BusinessOperationResultState::RepayWorkingCapitalLoan { loan }
        }
        BusinessOperationAction::Dissolve => {
            let Some(result) = dissolve_corporation(tx, finance_rules, scope, command).await?
            else {
                return Ok(None);
            };
            result
        }
    };
    Ok(Some(result))
}

async fn capital_contribution(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    scope: &OperationScopeRow,
    command: &ManageBusinessOperationsCommand,
    amount_krw: i64,
) -> Result<Option<BusinessOperationResultState>> {
    if amount_krw <= 0 || scope.wallet_cash_krw < amount_krw {
        return Ok(None);
    }
    let wallet_after = scope
        .wallet_cash_krw
        .checked_sub(amount_krw)
        .context("capital contribution wallet underflowed")?;
    let cash_after = checked_money_add(scope.corporation_cash_krw, amount_krw)?;
    let contributed_after = checked_money_add(scope.contributed_capital_krw, amount_krw)?;
    let inserted = sqlx::query(
        "INSERT INTO corporation_capital_contribution
             (save_id, run_revision, corporation_id, command_id, amount_krw,
              wallet_before_krw, wallet_after_krw, cash_before_krw, cash_after_krw,
              contributed_capital_before_krw, contributed_capital_after_krw,
              applied_game_day, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'preparing')",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_id)
    .bind(command.command_id.as_str())
    .bind(amount_krw)
    .bind(scope.wallet_cash_krw)
    .bind(wallet_after)
    .bind(scope.corporation_cash_krw)
    .bind(cash_after)
    .bind(scope.contributed_capital_krw)
    .bind(contributed_after)
    .bind(scope.game_day)
    .execute(&mut **tx)
    .await?;
    let contribution_id = inserted.last_insert_id();
    let personal = finance_rules
        .create_ledger_transaction(LedgerTransactionDraft {
            policy: finance_policy(scope),
            source: LedgerSource {
                kind: LedgerSourceKind::CorporationCapitalContribution,
                source_id: contribution_id.to_string(),
            },
            game_day: scope.game_day,
            description: "법인 추가 출자".to_owned(),
            postings: vec![
                LedgerPosting {
                    account_code: LedgerAccountCode::CorporationInvestmentAsset,
                    financial_account_id: None,
                    amount_krw,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::Wallet,
                    financial_account_id: None,
                    amount_krw: amount_krw
                        .checked_neg()
                        .context("capital contribution negation overflowed")?,
                },
            ],
        })
        .context("capital contribution personal ledger is invalid")?;
    let personal_ledger_id =
        write_personal_finance_ledger(tx, &personal, scope.corporation_id).await?;
    let corporation_ledger_id = write_corporation_finance_ledger(
        tx,
        scope,
        "capitalContribution",
        command.command_id.as_str(),
        "capitalContribution",
        contribution_id,
        &[
            ("corporationCash", amount_krw),
            (
                "contributedCapital",
                amount_krw
                    .checked_neg()
                    .context("capital contribution ledger overflowed")?,
            ),
        ],
    )
    .await?;
    let updated_save = sqlx::query(
        "UPDATE save SET cash_krw = ? WHERE id = ? AND run_revision = ? AND cash_krw = ?",
    )
    .bind(wallet_after)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.wallet_cash_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated_save.rows_affected() == 1,
        "capital contribution wallet changed"
    );
    let updated_corporation = sqlx::query(
        "UPDATE corporation SET cash_krw = ?, contributed_capital_krw = ?
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND cash_krw = ? AND contributed_capital_krw = ?",
    )
    .bind(cash_after)
    .bind(contributed_after)
    .bind(scope.corporation_id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_cash_krw)
    .bind(scope.contributed_capital_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated_corporation.rows_affected() == 1,
        "capital contribution corporation changed"
    );
    apply_capital_contribution(
        tx,
        contribution_id,
        corporation_ledger_id,
        personal_ledger_id,
    )
    .await?;
    Ok(Some(BusinessOperationResultState::CapitalContribution {
        contribution_id: ResourceId::from_u64(contribution_id),
        amount_krw,
        corporation_cash_after_krw: cash_after,
        contributed_capital_after_krw: contributed_after,
        wallet_cash_after_krw: wallet_after,
        corporation_ledger_transaction_id: ResourceId::from_u64(corporation_ledger_id),
        personal_ledger_transaction_id: ResourceId::from_u64(personal_ledger_id),
    }))
}

async fn draw_working_capital_loan(
    tx: &mut Transaction<'_, MySql>,
    scope: &OperationScopeRow,
    profile: &BusinessProfileRow,
    command: &ManageBusinessOperationsCommand,
    loan_product_id: ResourceId,
    principal_krw: i64,
) -> Result<Option<BusinessWorkingCapitalLoanState>> {
    let product: Option<LoanProductRow> = sqlx::query_as(
        "SELECT id, product_key, display_name, minimum_principal_krw,
                maximum_principal_krw, principal_step_krw, monthly_interest_rate_ppm,
                term_months, personal_guarantee
         FROM business_loan_product
         WHERE business_catalog_version_id = ? AND id = ? FOR SHARE",
    )
    .bind(profile.business_catalog_version_id)
    .bind(loan_product_id.get())
    .fetch_optional(&mut **tx)
    .await?;
    let Some(product) = product else {
        return Ok(None);
    };
    if principal_krw < product.minimum_principal_krw
        || principal_krw > product.maximum_principal_krw
        || principal_krw % product.principal_step_krw != 0
        || product.personal_guarantee
    {
        return Ok(None);
    }
    let existing: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM corporation_working_capital_loan
         WHERE save_id = ? AND run_revision = ? AND corporation_id = ?
           AND status IN ('active', 'matured')",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_id)
    .fetch_one(&mut **tx)
    .await?;
    if existing != 0 {
        return Ok(None);
    }
    let cash_after = checked_money_add(scope.corporation_cash_krw, principal_krw)?;
    let (maturity_year, maturity_month) = add_months(
        profile.effective_year,
        profile.effective_month,
        product.term_months,
    )?;
    let inserted = sqlx::query(
        "INSERT INTO corporation_working_capital_loan
             (save_id, run_revision, corporation_id, business_profile_id,
              business_catalog_version_id, loan_product_id, command_id,
              original_principal_krw, outstanding_principal_krw,
              monthly_interest_rate_ppm, term_months,
              originated_year, originated_month, maturity_year, maturity_month,
              personal_guarantee, cash_before_krw, cash_after_krw,
              originated_game_day, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'preparing')",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_id)
    .bind(profile.id)
    .bind(profile.business_catalog_version_id)
    .bind(product.id)
    .bind(command.command_id.as_str())
    .bind(principal_krw)
    .bind(principal_krw)
    .bind(product.monthly_interest_rate_ppm)
    .bind(product.term_months)
    .bind(profile.effective_year)
    .bind(profile.effective_month)
    .bind(maturity_year)
    .bind(maturity_month)
    .bind(product.personal_guarantee)
    .bind(scope.corporation_cash_krw)
    .bind(cash_after)
    .bind(scope.game_day)
    .execute(&mut **tx)
    .await?;
    let loan_id = inserted.last_insert_id();
    let ledger_id = write_corporation_finance_ledger(
        tx,
        scope,
        "workingCapitalLoanDraw",
        command.command_id.as_str(),
        "workingCapitalLoan",
        loan_id,
        &[
            ("corporationCash", principal_krw),
            (
                "workingCapitalLoanLiability",
                principal_krw
                    .checked_neg()
                    .context("loan draw ledger overflowed")?,
            ),
        ],
    )
    .await?;
    update_corporation_cash(tx, scope, cash_after).await?;
    let applied = sqlx::query(
        "UPDATE corporation_working_capital_loan
         SET status = 'active', corporation_ledger_transaction_id = ?, applied_at = CURRENT_TIMESTAMP(3)
         WHERE id = ? AND status = 'preparing' AND corporation_ledger_transaction_id IS NULL",
    )
    .bind(ledger_id)
    .bind(loan_id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        applied.rows_affected() == 1,
        "working capital loan apply failed"
    );
    read_working_capital_loan(tx, scope, loan_id, false).await
}

async fn repay_working_capital_loan(
    tx: &mut Transaction<'_, MySql>,
    scope: &OperationScopeRow,
    command: &ManageBusinessOperationsCommand,
    loan_id: ResourceId,
    principal_krw: i64,
) -> Result<Option<BusinessWorkingCapitalLoanState>> {
    let Some(loan) = read_working_capital_loan_row(tx, scope, loan_id.get(), true).await? else {
        return Ok(None);
    };
    if !matches!(loan.status.as_str(), "active" | "matured")
        || principal_krw <= 0
        || principal_krw > loan.outstanding_principal_krw
        || principal_krw > scope.corporation_cash_krw
    {
        return Ok(None);
    }
    let outstanding_after = loan
        .outstanding_principal_krw
        .checked_sub(principal_krw)
        .context("loan repayment underflowed")?;
    let cash_after = scope
        .corporation_cash_krw
        .checked_sub(principal_krw)
        .context("loan repayment cash underflowed")?;
    let inserted = sqlx::query(
        "INSERT INTO corporation_working_capital_loan_repayment
             (save_id, run_revision, corporation_id, loan_id, command_id,
              principal_krw, outstanding_before_krw, outstanding_after_krw,
              cash_before_krw, cash_after_krw, applied_game_day, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'preparing')",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_id)
    .bind(loan.id)
    .bind(command.command_id.as_str())
    .bind(principal_krw)
    .bind(loan.outstanding_principal_krw)
    .bind(outstanding_after)
    .bind(scope.corporation_cash_krw)
    .bind(cash_after)
    .bind(scope.game_day)
    .execute(&mut **tx)
    .await?;
    let repayment_id = inserted.last_insert_id();
    let ledger_id = write_corporation_finance_ledger(
        tx,
        scope,
        "workingCapitalLoanRepayment",
        command.command_id.as_str(),
        "workingCapitalLoanRepayment",
        repayment_id,
        &[
            ("workingCapitalLoanLiability", principal_krw),
            (
                "corporationCash",
                principal_krw
                    .checked_neg()
                    .context("loan repayment ledger overflowed")?,
            ),
        ],
    )
    .await?;
    update_corporation_cash(tx, scope, cash_after).await?;
    let status = if outstanding_after == 0 {
        "repaid"
    } else {
        loan.status.as_str()
    };
    let updated_loan = sqlx::query(
        "UPDATE corporation_working_capital_loan
         SET outstanding_principal_krw = ?, status = ?
         WHERE id = ? AND outstanding_principal_krw = ? AND status = ?",
    )
    .bind(outstanding_after)
    .bind(status)
    .bind(loan.id)
    .bind(loan.outstanding_principal_krw)
    .bind(&loan.status)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated_loan.rows_affected() == 1,
        "working capital loan changed"
    );
    let applied = sqlx::query(
        "UPDATE corporation_working_capital_loan_repayment
         SET status = 'applied', corporation_ledger_transaction_id = ?, applied_at = CURRENT_TIMESTAMP(3)
         WHERE id = ? AND status = 'preparing' AND corporation_ledger_transaction_id IS NULL",
    )
    .bind(ledger_id)
    .bind(repayment_id)
    .execute(&mut **tx)
    .await?;
    ensure!(applied.rows_affected() == 1, "loan repayment apply failed");
    read_working_capital_loan(tx, scope, loan.id, false).await
}

async fn dissolve_corporation(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    scope: &OperationScopeRow,
    command: &ManageBusinessOperationsCommand,
) -> Result<Option<BusinessOperationResultState>> {
    if scope.operating_payable_krw != 0 || scope.corporate_tax_payable_krw != 0 {
        return Ok(None);
    }
    let blockers: i64 = sqlx::query_scalar(
        "SELECT
            (SELECT COUNT(*) FROM corporation_customer_contract
             WHERE save_id = ? AND run_revision = ? AND corporation_id = ?
               AND status IN ('offered', 'accepted', 'active'))
          + (SELECT COUNT(*) FROM corporation_staff_position
             WHERE save_id = ? AND run_revision = ? AND corporation_id = ?
               AND status IN ('hired', 'active'))
          + (SELECT COUNT(*) FROM corporation_working_capital_loan
             WHERE save_id = ? AND run_revision = ? AND corporation_id = ?
               AND outstanding_principal_krw > 0)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_id)
    .fetch_one(&mut **tx)
    .await?;
    if blockers != 0 {
        return Ok(None);
    }
    let distribution_krw = scope.corporation_cash_krw;
    let capital_basis_krw = scope.contributed_capital_krw;
    let realized_gain_loss_krw = capital_basis_krw
        .checked_sub(distribution_krw)
        .context("corporation liquidation gain/loss overflowed")?;
    if capital_basis_krw
        .checked_add(scope.retained_earnings_krw)
        .context("corporation liquidation equity overflowed")?
        != distribution_krw
    {
        return Ok(None);
    }
    let wallet_after = checked_money_add(scope.wallet_cash_krw, distribution_krw)?;
    let inserted = sqlx::query(
        "INSERT INTO corporation_dissolution
             (save_id, run_revision, corporation_id, command_id,
              distribution_krw, capital_basis_krw, realized_gain_loss_krw,
              wallet_before_krw, wallet_after_krw, cash_before_krw,
              contributed_capital_before_krw, retained_earnings_before_krw,
              distributable_profit_before_krw, applied_game_day, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'preparing')",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_id)
    .bind(command.command_id.as_str())
    .bind(distribution_krw)
    .bind(capital_basis_krw)
    .bind(realized_gain_loss_krw)
    .bind(scope.wallet_cash_krw)
    .bind(wallet_after)
    .bind(distribution_krw)
    .bind(capital_basis_krw)
    .bind(scope.retained_earnings_krw)
    .bind(scope.distributable_profit_krw)
    .bind(scope.game_day)
    .execute(&mut **tx)
    .await?;
    let dissolution_id = inserted.last_insert_id();
    let mut personal_postings = Vec::with_capacity(3);
    push_ledger_posting(
        &mut personal_postings,
        LedgerAccountCode::Wallet,
        distribution_krw,
    );
    push_ledger_posting(
        &mut personal_postings,
        LedgerAccountCode::CorporationInvestmentAsset,
        capital_basis_krw
            .checked_neg()
            .context("corporation liquidation basis overflowed")?,
    );
    push_ledger_posting(
        &mut personal_postings,
        LedgerAccountCode::RealizedGainLoss,
        realized_gain_loss_krw,
    );
    let personal = finance_rules
        .create_ledger_transaction(LedgerTransactionDraft {
            policy: finance_policy(scope),
            source: LedgerSource {
                kind: LedgerSourceKind::CorporationLiquidation,
                source_id: dissolution_id.to_string(),
            },
            game_day: scope.game_day,
            description: "법인 해산 잔여재산 분배".to_owned(),
            postings: personal_postings,
        })
        .context("corporation liquidation personal ledger is invalid")?;
    let personal_ledger_id =
        write_personal_finance_ledger(tx, &personal, scope.corporation_id).await?;
    let mut corporation_postings = Vec::with_capacity(3);
    push_corporation_posting(
        &mut corporation_postings,
        "corporationCash",
        -distribution_krw,
    );
    push_corporation_posting(
        &mut corporation_postings,
        "contributedCapital",
        capital_basis_krw,
    );
    push_corporation_posting(
        &mut corporation_postings,
        "retainedEarnings",
        scope.retained_earnings_krw,
    );
    let corporation_ledger_id = write_corporation_finance_ledger(
        tx,
        scope,
        "liquidation",
        command.command_id.as_str(),
        "dissolution",
        dissolution_id,
        &corporation_postings,
    )
    .await?;
    let updated_save = sqlx::query(
        "UPDATE save SET cash_krw = ? WHERE id = ? AND run_revision = ? AND cash_krw = ?",
    )
    .bind(wallet_after)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.wallet_cash_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated_save.rows_affected() == 1,
        "corporation liquidation wallet changed"
    );
    let dissolved = sqlx::query(
        "UPDATE corporation
         SET status = 'dissolved', cash_krw = 0, contributed_capital_krw = 0,
             retained_earnings_krw = 0, distributable_profit_krw = 0
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = ?
           AND cash_krw = ? AND contributed_capital_krw = ? AND retained_earnings_krw = ?",
    )
    .bind(scope.corporation_id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(&scope.corporation_status)
    .bind(distribution_krw)
    .bind(capital_basis_krw)
    .bind(scope.retained_earnings_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        dissolved.rows_affected() == 1,
        "corporation dissolution failed"
    );
    let applied = sqlx::query(
        "UPDATE corporation_dissolution
         SET status = 'applied', corporation_ledger_transaction_id = ?,
             personal_ledger_transaction_id = ?, applied_at = CURRENT_TIMESTAMP(3)
         WHERE id = ? AND status = 'preparing'",
    )
    .bind(corporation_ledger_id)
    .bind(personal_ledger_id)
    .bind(dissolution_id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        applied.rows_affected() == 1,
        "corporation dissolution apply failed"
    );
    Ok(Some(BusinessOperationResultState::Dissolve {
        dissolution_id: ResourceId::from_u64(dissolution_id),
        distribution_krw,
        capital_basis_krw,
        realized_gain_loss_krw,
        wallet_cash_after_krw: wallet_after,
        corporation_ledger_transaction_id: ResourceId::from_u64(corporation_ledger_id),
        personal_ledger_transaction_id: ResourceId::from_u64(personal_ledger_id),
    }))
}

fn finance_policy(scope: &OperationScopeRow) -> RunPolicyContext {
    RunPolicyContext {
        run: RunId {
            save_id: ResourceId::from_u64(scope.save_id),
            run_revision: scope.run_revision,
        },
        policy_set_id: ResourceId::from_u64(scope.policy_set_id),
    }
}

async fn insert_dissolution_transition(
    tx: &mut Transaction<'_, MySql>,
    scope: &OperationScopeRow,
    command: &ManageBusinessOperationsCommand,
) -> Result<()> {
    let transition_no: u16 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(MAX(transition_no), 0) + 1 AS UNSIGNED)
         FROM corporation_transition WHERE save_id = ? AND run_revision = ? AND corporation_id = ?",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_id)
    .fetch_one(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO corporation_transition
             (save_id, run_revision, corporation_id, transition_no, from_status, to_status,
              command_id, transition_game_day, transition_reason)
         VALUES (?, ?, ?, ?, ?, 'dissolved', ?, ?, 'playerDissolved')",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_id)
    .bind(transition_no)
    .bind(&scope.corporation_status)
    .bind(command.command_id.as_str())
    .bind(scope.game_day)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn push_ledger_posting(
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

fn push_corporation_posting(
    postings: &mut Vec<(&'static str, i64)>,
    account_code: &'static str,
    amount_krw: i64,
) {
    if amount_krw != 0 {
        postings.push((account_code, amount_krw));
    }
}

async fn write_personal_finance_ledger(
    tx: &mut Transaction<'_, MySql>,
    ledger: &LedgerTransaction,
    corporation_id: u64,
) -> Result<u64> {
    let policy = ledger.policy();
    let inserted = sqlx::query(
        "INSERT INTO ledger_transaction
             (save_id, run_revision, game_day, policy_set_id, source_kind, source_id, description)
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
        sqlx::query(
            "INSERT INTO ledger_posting
                 (save_id, run_revision, ledger_transaction_id, posting_order,
                  account_code, financial_account_id, corporation_id, amount_krw)
             VALUES (?, ?, ?, ?, ?, NULL, ?, ?)",
        )
        .bind(policy.run.save_id.get())
        .bind(policy.run.run_revision)
        .bind(ledger_id)
        .bind(u16::try_from(index + 1)?)
        .bind(to_db_str(&posting.account_code)?)
        .bind(corporation_id)
        .bind(posting.amount_krw)
        .execute(&mut **tx)
        .await?;
    }
    Ok(ledger_id)
}

async fn write_corporation_finance_ledger(
    tx: &mut Transaction<'_, MySql>,
    scope: &OperationScopeRow,
    transaction_kind: &str,
    correlation_id: &str,
    authority_kind: &str,
    authority_id: u64,
    postings: &[(&str, i64)],
) -> Result<u64> {
    let sql = match authority_kind {
        "capitalContribution" => {
            "INSERT INTO corporation_ledger_transaction
                 (save_id, run_revision, corporation_id, game_day, transaction_kind,
                  correlation_id, corporation_capital_contribution_id, description)
             VALUES (?, ?, ?, ?, ?, ?, ?, '법인 추가 출자')"
        }
        "workingCapitalLoan" => {
            "INSERT INTO corporation_ledger_transaction
                 (save_id, run_revision, corporation_id, game_day, transaction_kind,
                  correlation_id, working_capital_loan_id, description)
             VALUES (?, ?, ?, ?, ?, ?, ?, '법인 운전자금 대출 실행')"
        }
        "workingCapitalLoanRepayment" => {
            "INSERT INTO corporation_ledger_transaction
                 (save_id, run_revision, corporation_id, game_day, transaction_kind,
                  correlation_id, working_capital_loan_repayment_id, description)
             VALUES (?, ?, ?, ?, ?, ?, ?, '법인 운전자금 대출 상환')"
        }
        "dissolution" => {
            "INSERT INTO corporation_ledger_transaction
                 (save_id, run_revision, corporation_id, game_day, transaction_kind,
                  correlation_id, corporation_dissolution_id, description)
             VALUES (?, ?, ?, ?, ?, ?, ?, '법인 해산 정산')"
        }
        _ => bail!("unsupported corporation finance authority"),
    };
    let inserted = sqlx::query(sql)
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(scope.corporation_id)
        .bind(scope.game_day)
        .bind(transaction_kind)
        .bind(correlation_id)
        .bind(authority_id)
        .execute(&mut **tx)
        .await?;
    let ledger_id = inserted.last_insert_id();
    for (index, (account_code, amount_krw)) in postings.iter().enumerate() {
        ensure!(*amount_krw != 0, "zero corporation finance posting");
        sqlx::query(
            "INSERT INTO corporation_ledger_posting
                 (save_id, run_revision, corporation_id,
                  corporation_ledger_transaction_id, posting_order, account_code, amount_krw)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(scope.corporation_id)
        .bind(ledger_id)
        .bind(u16::try_from(index + 1)?)
        .bind(*account_code)
        .bind(*amount_krw)
        .execute(&mut **tx)
        .await?;
    }
    Ok(ledger_id)
}

async fn apply_capital_contribution(
    tx: &mut Transaction<'_, MySql>,
    contribution_id: u64,
    corporation_ledger_id: u64,
    personal_ledger_id: u64,
) -> Result<()> {
    let applied = sqlx::query(
        "UPDATE corporation_capital_contribution
         SET status = 'applied', corporation_ledger_transaction_id = ?,
             personal_ledger_transaction_id = ?, applied_at = CURRENT_TIMESTAMP(3)
         WHERE id = ? AND status = 'preparing'
           AND corporation_ledger_transaction_id IS NULL
           AND personal_ledger_transaction_id IS NULL",
    )
    .bind(corporation_ledger_id)
    .bind(personal_ledger_id)
    .bind(contribution_id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        applied.rows_affected() == 1,
        "capital contribution apply failed"
    );
    Ok(())
}

async fn update_corporation_cash(
    tx: &mut Transaction<'_, MySql>,
    scope: &OperationScopeRow,
    cash_after_krw: i64,
) -> Result<()> {
    let updated = sqlx::query(
        "UPDATE corporation SET cash_krw = ?
         WHERE id = ? AND save_id = ? AND run_revision = ? AND cash_krw = ?",
    )
    .bind(cash_after_krw)
    .bind(scope.corporation_id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_cash_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(updated.rows_affected() == 1, "corporation cash changed");
    Ok(())
}

async fn read_working_capital_loan_row(
    tx: &mut Transaction<'_, MySql>,
    scope: &OperationScopeRow,
    loan_id: u64,
    lock: bool,
) -> Result<Option<WorkingCapitalLoanRow>> {
    let query = format!(
        "SELECT loan.id, loan.loan_product_id, product.product_key, product.display_name,
                loan.status, loan.original_principal_krw, loan.outstanding_principal_krw,
                loan.monthly_interest_rate_ppm, loan.term_months,
                loan.originated_year, loan.originated_month,
                loan.maturity_year, loan.maturity_month, loan.personal_guarantee
         FROM corporation_working_capital_loan AS loan
         INNER JOIN business_loan_product AS product
            ON product.business_catalog_version_id = loan.business_catalog_version_id
           AND product.id = loan.loan_product_id
         WHERE loan.save_id = ? AND loan.run_revision = ? AND loan.corporation_id = ?
           AND loan.id = ?{}",
        if lock { " FOR UPDATE" } else { "" }
    );
    sqlx::query_as::<_, WorkingCapitalLoanRow>(AssertSqlSafe(query.as_str()))
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(scope.corporation_id)
        .bind(loan_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Into::into)
}

async fn read_working_capital_loan(
    tx: &mut Transaction<'_, MySql>,
    scope: &OperationScopeRow,
    loan_id: u64,
    lock: bool,
) -> Result<Option<BusinessWorkingCapitalLoanState>> {
    read_working_capital_loan_row(tx, scope, loan_id, lock)
        .await?
        .map(working_capital_loan_state)
        .transpose()
}

fn working_capital_loan_state(
    row: WorkingCapitalLoanRow,
) -> Result<BusinessWorkingCapitalLoanState> {
    let status = match row.status.as_str() {
        "active" => BusinessWorkingCapitalLoanStatusState::Active,
        "matured" => BusinessWorkingCapitalLoanStatusState::Matured,
        "repaid" => BusinessWorkingCapitalLoanStatusState::Repaid,
        _ => bail!("unknown working capital loan status"),
    };
    Ok(BusinessWorkingCapitalLoanState {
        id: ResourceId::from_u64(row.id),
        product_id: ResourceId::from_u64(row.loan_product_id),
        product_key: row.product_key,
        display_name: row.display_name,
        status,
        original_principal_krw: row.original_principal_krw,
        outstanding_principal_krw: row.outstanding_principal_krw,
        monthly_interest_rate_ppm: row.monthly_interest_rate_ppm,
        term_months: row.term_months,
        originated_year: row.originated_year,
        originated_month: row.originated_month,
        maturity_year: row.maturity_year,
        maturity_month: row.maturity_month,
        personal_guarantee: row.personal_guarantee,
    })
}

fn checked_money_add(left: i64, right: i64) -> Result<i64> {
    let result = i128::from(left)
        .checked_add(i128::from(right))
        .context("money addition overflowed")?;
    ensure!(
        (0..=9_007_199_254_740_991_i128).contains(&result),
        "money result is outside public bounds"
    );
    i64::try_from(result).map_err(Into::into)
}

fn add_months(year: u16, month: u8, months: u16) -> Result<(u16, u8)> {
    ensure!((1..=12).contains(&month), "invalid source month");
    let absolute = u32::from(year)
        .checked_mul(12)
        .and_then(|value| value.checked_add(u32::from(month - 1)))
        .and_then(|value| value.checked_add(u32::from(months)))
        .context("working capital maturity overflowed")?;
    let maturity_year = u16::try_from(absolute / 12)?;
    let maturity_month = u8::try_from((absolute % 12) + 1)?;
    ensure!(maturity_year > 0, "invalid maturity year");
    Ok((maturity_year, maturity_month))
}

fn operation_status_allowed(status: &str, action: &BusinessOperationAction) -> bool {
    match action {
        BusinessOperationAction::RepayWorkingCapitalLoan { .. } => {
            matches!(status, "active" | "insolvent")
        }
        BusinessOperationAction::Dissolve => {
            matches!(status, "active" | "dormant" | "insolvent")
        }
        _ => status == "active",
    }
}

fn to_db_str<T: Serialize>(value: &T) -> Result<String> {
    let json = serde_json::to_string(value)?;
    Ok(json.trim_matches('"').to_owned())
}

async fn accept_contract(
    tx: &mut Transaction<'_, MySql>,
    scope: &OperationScopeRow,
    profile: &BusinessProfileRow,
    command: &ManageBusinessOperationsCommand,
    contract_id: ResourceId,
) -> Result<Option<BusinessContractState>> {
    let Some(row) = read_contract(tx, scope, contract_id.get(), true).await? else {
        return Ok(None);
    };
    if row.status != "offered"
        || row.service_year != profile.effective_year
        || row.service_month != profile.effective_month
    {
        return Ok(None);
    }
    insert_contract_transition(tx, scope, row.id, "offered", "accepted", command, 1).await?;
    let updated = sqlx::query(
        "UPDATE corporation_customer_contract
         SET status = 'accepted', accepted_command_id = ?
         WHERE id = ? AND status = 'offered'",
    )
    .bind(command.command_id.as_str())
    .bind(row.id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "business contract acceptance failed"
    );
    let updated = read_contract(tx, scope, row.id, false)
        .await?
        .context("accepted business contract disappeared")?;
    contract_state(updated).map(Some)
}

async fn cancel_contract(
    tx: &mut Transaction<'_, MySql>,
    scope: &OperationScopeRow,
    command: &ManageBusinessOperationsCommand,
    contract_id: ResourceId,
) -> Result<Option<BusinessContractState>> {
    let Some(row) = read_contract(tx, scope, contract_id.get(), true).await? else {
        return Ok(None);
    };
    if !matches!(row.status.as_str(), "offered" | "accepted") {
        return Ok(None);
    }
    let transition_no = if row.status == "offered" { 1 } else { 2 };
    insert_contract_transition(
        tx,
        scope,
        row.id,
        &row.status,
        "cancelled",
        command,
        transition_no,
    )
    .await?;
    let updated = sqlx::query(
        "UPDATE corporation_customer_contract
         SET status = 'cancelled', terminal_command_id = ?, terminal_game_day = ?
         WHERE id = ? AND status = ?",
    )
    .bind(command.command_id.as_str())
    .bind(scope.game_day)
    .bind(row.id)
    .bind(&row.status)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "business contract cancellation failed"
    );
    let updated = read_contract(tx, scope, row.id, false)
        .await?
        .context("cancelled business contract disappeared")?;
    contract_state(updated).map(Some)
}

async fn hire_position(
    tx: &mut Transaction<'_, MySql>,
    scope: &OperationScopeRow,
    profile: &BusinessProfileRow,
    command: &ManageBusinessOperationsCommand,
    position_id: ResourceId,
) -> Result<Option<BusinessPositionState>> {
    let Some(row) = read_position(tx, scope, position_id.get(), true).await? else {
        return Ok(None);
    };
    if row.status != "vacant" {
        return Ok(None);
    }
    insert_staff_transition(tx, scope, row.id, ("vacant", "hired"), command, profile, 1).await?;
    let updated = sqlx::query(
        "UPDATE corporation_staff_position
         SET status = 'hired', effective_year = ?, effective_month = ?, hired_command_id = ?
         WHERE id = ? AND status = 'vacant'",
    )
    .bind(profile.effective_year)
    .bind(profile.effective_month)
    .bind(command.command_id.as_str())
    .bind(row.id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "business position hire failed"
    );
    let updated = read_position(tx, scope, row.id, false)
        .await?
        .context("hired business position disappeared")?;
    position_state(updated).map(Some)
}

async fn terminate_position(
    tx: &mut Transaction<'_, MySql>,
    scope: &OperationScopeRow,
    profile: &BusinessProfileRow,
    command: &ManageBusinessOperationsCommand,
    position_id: ResourceId,
) -> Result<Option<BusinessPositionState>> {
    let Some(row) = read_position(tx, scope, position_id.get(), true).await? else {
        return Ok(None);
    };
    if !matches!(row.status.as_str(), "hired" | "active") || row.hired_command_id.is_none() {
        return Ok(None);
    }
    let transition_no = if row.status == "hired" { 2 } else { 3 };
    insert_staff_transition(
        tx,
        scope,
        row.id,
        (&row.status, "terminated"),
        command,
        profile,
        transition_no,
    )
    .await?;
    let updated = sqlx::query(
        "UPDATE corporation_staff_position
         SET status = 'terminated', ended_command_id = ?
         WHERE id = ? AND status = ?",
    )
    .bind(command.command_id.as_str())
    .bind(row.id)
    .bind(&row.status)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "business position termination failed"
    );
    let updated = read_position(tx, scope, row.id, false)
        .await?
        .context("terminated business position disappeared")?;
    position_state(updated).map(Some)
}

async fn set_monthly_plan(
    tx: &mut Transaction<'_, MySql>,
    scope: &OperationScopeRow,
    profile: &BusinessProfileRow,
    command: &ManageBusinessOperationsCommand,
    marketing_band_id: ResourceId,
    cash_buffer_krw: i64,
    contract_priority_ids: &[ResourceId],
) -> Result<Option<BusinessMonthlyPlanState>> {
    if !(0..=9_007_199_254_740_991).contains(&cash_buffer_krw) {
        return Ok(None);
    }
    let marketing: Option<MarketingRow> = sqlx::query_as(
        "SELECT id, band_key, display_name, band_order, monthly_cost_krw, offer_slots
         FROM business_marketing_band
         WHERE business_catalog_version_id = ? AND id = ? FOR SHARE",
    )
    .bind(profile.business_catalog_version_id)
    .bind(marketing_band_id.get())
    .fetch_optional(&mut **tx)
    .await?;
    if marketing.is_none() {
        return Ok(None);
    }
    let mut ids = BTreeSet::new();
    for id in contract_priority_ids {
        if !ids.insert(id.get()) {
            return Ok(None);
        }
        let eligible: Option<u64> = sqlx::query_scalar(
            "SELECT id FROM corporation_customer_contract
             WHERE save_id = ? AND run_revision = ? AND corporation_id = ? AND id = ?
               AND service_year = ? AND service_month = ?
               AND status IN ('accepted', 'active') FOR SHARE",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(scope.corporation_id)
        .bind(id.get())
        .bind(profile.effective_year)
        .bind(profile.effective_month)
        .fetch_optional(&mut **tx)
        .await?;
        if eligible.is_none() {
            return Ok(None);
        }
    }
    let plan_revision: u64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(MAX(plan_revision), 0) + 1 AS UNSIGNED)
         FROM corporation_monthly_plan
         WHERE save_id = ? AND run_revision = ? AND corporation_id = ?
           AND effective_year = ? AND effective_month = ?",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_id)
    .bind(profile.effective_year)
    .bind(profile.effective_month)
    .fetch_one(&mut **tx)
    .await?;
    let priorities = contract_priority_ids
        .iter()
        .map(|id| id.get().to_string())
        .collect::<Vec<_>>();
    let inserted = sqlx::query(
        "INSERT INTO corporation_monthly_plan
             (save_id, run_revision, corporation_id, business_profile_id,
              business_catalog_version_id, marketing_band_id,
              effective_year, effective_month, plan_revision, cash_buffer_krw,
              contract_priority_json, command_id, created_game_day)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_id)
    .bind(profile.id)
    .bind(profile.business_catalog_version_id)
    .bind(marketing_band_id.get())
    .bind(profile.effective_year)
    .bind(profile.effective_month)
    .bind(plan_revision)
    .bind(cash_buffer_krw)
    .bind(serde_json::to_string(&priorities)?)
    .bind(command.command_id.as_str())
    .bind(scope.game_day)
    .execute(&mut **tx)
    .await?;
    let row = read_plan_by_id(tx, scope, inserted.last_insert_id())
        .await?
        .context("created business monthly plan disappeared")?;
    plan_state(row).map(Some)
}

async fn insert_contract_transition(
    tx: &mut Transaction<'_, MySql>,
    scope: &OperationScopeRow,
    contract_id: u64,
    from_status: &str,
    to_status: &str,
    command: &ManageBusinessOperationsCommand,
    transition_no: u16,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO corporation_contract_transition
             (save_id, run_revision, corporation_id, contract_id, transition_no,
              from_status, to_status, command_id, transition_game_day)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_id)
    .bind(contract_id)
    .bind(transition_no)
    .bind(from_status)
    .bind(to_status)
    .bind(command.command_id.as_str())
    .bind(scope.game_day)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_staff_transition(
    tx: &mut Transaction<'_, MySql>,
    scope: &OperationScopeRow,
    position_id: u64,
    status_transition: (&str, &str),
    command: &ManageBusinessOperationsCommand,
    profile: &BusinessProfileRow,
    transition_no: u16,
) -> Result<()> {
    let (from_status, to_status) = status_transition;
    sqlx::query(
        "INSERT INTO corporation_staff_transition
             (save_id, run_revision, corporation_id, position_id, transition_no,
              from_status, to_status, command_id, effective_year, effective_month,
              transition_game_day)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_id)
    .bind(position_id)
    .bind(transition_no)
    .bind(from_status)
    .bind(to_status)
    .bind(command.command_id.as_str())
    .bind(profile.effective_year)
    .bind(profile.effective_month)
    .bind(scope.game_day)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn finish_replay(
    mut tx: Transaction<'_, MySql>,
    scope: &OperationScopeRow,
    command: &ManageBusinessOperationsCommand,
    command_kind: &str,
    fingerprint: &str,
) -> Result<LifeStoreResult<BusinessOperationReceipt>> {
    let row: Option<ReceiptRow> = sqlx::query_as(
        "SELECT command_kind, payload_sha256, CAST(result_json AS CHAR) AS result_json
         FROM corporation_operation_command_receipt
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
        row.command_kind == command_kind && row.payload_sha256 == fingerprint,
        "business operation receipt disagrees with command identity"
    );
    let mut receipt: BusinessOperationReceipt = serde_json::from_str(&row.result_json)?;
    receipt.replayed = true;
    let save = read_state(&mut tx, scope.save_id).await?;
    tx.commit().await?;
    Ok(LifeStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

async fn read_operations_state(
    tx: &mut Transaction<'_, MySql>,
    scope: &OperationScopeRow,
) -> Result<BusinessOperationsState> {
    let profile = read_profile(tx, scope, false).await?;
    let Some(profile) = profile else {
        return Ok(BusinessOperationsState {
            availability: BusinessOperationsAvailabilityState::Unavailable,
            corporation_id: ResourceId::from_u64(scope.corporation_id),
            catalog_version_id: None,
            catalog_sha256: None,
            revision: 0,
            next_operating_year: None,
            next_operating_month: None,
            marketing_bands: Vec::new(),
            loan_products: Vec::new(),
            working_capital_loans: Vec::new(),
            contracts: Vec::new(),
            positions: Vec::new(),
            plan: None,
            latest_month: None,
        });
    };
    ensure!(
        Some(profile.business_catalog_version_id) == scope.business_catalog_version_id
            && Some(profile.business_catalog_sha256.as_str())
                == scope.business_catalog_sha256.as_deref(),
        "business profile disagrees with run manifest"
    );
    let marketing_rows: Vec<MarketingRow> = sqlx::query_as(
        "SELECT id, band_key, display_name, band_order, monthly_cost_krw, offer_slots
         FROM business_marketing_band WHERE business_catalog_version_id = ?
         ORDER BY band_order, id",
    )
    .bind(profile.business_catalog_version_id)
    .fetch_all(&mut **tx)
    .await?;
    let loan_rows: Vec<LoanProductRow> = sqlx::query_as(
        "SELECT id, product_key, display_name, minimum_principal_krw,
                maximum_principal_krw, principal_step_krw, monthly_interest_rate_ppm,
                term_months, personal_guarantee
         FROM business_loan_product WHERE business_catalog_version_id = ? ORDER BY id",
    )
    .bind(profile.business_catalog_version_id)
    .fetch_all(&mut **tx)
    .await?;
    let working_capital_loan_rows: Vec<WorkingCapitalLoanRow> = sqlx::query_as(
        "SELECT loan.id, loan.loan_product_id, product.product_key, product.display_name,
                loan.status, loan.original_principal_krw, loan.outstanding_principal_krw,
                loan.monthly_interest_rate_ppm, loan.term_months,
                loan.originated_year, loan.originated_month,
                loan.maturity_year, loan.maturity_month, loan.personal_guarantee
         FROM corporation_working_capital_loan AS loan
         INNER JOIN business_loan_product AS product
            ON product.business_catalog_version_id = loan.business_catalog_version_id
           AND product.id = loan.loan_product_id
         WHERE loan.save_id = ? AND loan.run_revision = ? AND loan.corporation_id = ?
           AND loan.status IN ('active', 'matured', 'repaid')
         ORDER BY loan.id DESC LIMIT 8",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_id)
    .fetch_all(&mut **tx)
    .await?;
    let contract_rows: Vec<ContractRow> = sqlx::query_as(
        "SELECT contract.id, template.template_key, template.display_name, contract.status,
                contract.service_year, contract.service_month,
                contract.required_capacity_units, contract.revenue_krw,
                contract.variable_cost_ppm, contract.failure_penalty_krw
         FROM corporation_customer_contract AS contract
         INNER JOIN business_contract_template AS template
            ON template.business_catalog_version_id = contract.business_catalog_version_id
           AND template.id = contract.contract_template_id
         WHERE contract.save_id = ? AND contract.run_revision = ?
           AND contract.corporation_id = ?
         ORDER BY contract.service_year, contract.service_month, contract.id
         LIMIT 50",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_id)
    .fetch_all(&mut **tx)
    .await?;
    let position_rows: Vec<PositionRow> = sqlx::query_as(
        "SELECT position.id, role.role_key, role.display_name, position.status,
                role.capacity_units, role.monthly_gross_wage_krw,
                role.employer_cost_rate_ppm, position.effective_year,
                position.effective_month, position.hired_command_id
         FROM corporation_staff_position AS position
         INNER JOIN business_role_template AS role
            ON role.business_catalog_version_id = position.business_catalog_version_id
           AND role.id = position.role_template_id
         WHERE position.save_id = ? AND position.run_revision = ?
           AND position.corporation_id = ?
         ORDER BY role.role_order, position.position_no, position.id",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_id)
    .fetch_all(&mut **tx)
    .await?;
    let plan = read_latest_plan(tx, scope, &profile)
        .await?
        .map(plan_state)
        .transpose()?;
    let latest_month: Option<BusinessMonthRow> = sqlx::query_as(
        "SELECT id, operating_year, operating_month, total_capacity_units,
                used_capacity_units, contract_revenue_krw, contract_variable_cost_krw,
                marketing_cost_krw, employee_gross_wage_krw,
                employee_employer_cost_krw, failed_contract_penalty_krw,
                loan_interest_cost_krw,
                completed_contract_count, failed_contract_count,
                active_employee_count, applied_game_day
         FROM corporation_business_month
         WHERE save_id = ? AND run_revision = ? AND corporation_id = ?
         ORDER BY operating_year DESC, operating_month DESC, id DESC LIMIT 1",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(BusinessOperationsState {
        availability: BusinessOperationsAvailabilityState::Active,
        corporation_id: ResourceId::from_u64(scope.corporation_id),
        catalog_version_id: Some(ResourceId::from_u64(profile.business_catalog_version_id)),
        catalog_sha256: Some(profile.business_catalog_sha256),
        revision: profile.control_revision,
        next_operating_year: Some(profile.effective_year),
        next_operating_month: Some(profile.effective_month),
        marketing_bands: marketing_rows.into_iter().map(marketing_state).collect(),
        loan_products: loan_rows.into_iter().map(loan_product_state).collect(),
        working_capital_loans: working_capital_loan_rows
            .into_iter()
            .map(working_capital_loan_state)
            .collect::<Result<Vec<_>>>()?,
        contracts: contract_rows
            .into_iter()
            .map(contract_state)
            .collect::<Result<Vec<_>>>()?,
        positions: position_rows
            .into_iter()
            .map(position_state)
            .collect::<Result<Vec<_>>>()?,
        plan,
        latest_month: latest_month.map(business_month_state).transpose()?,
    })
}

async fn read_scope(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
    corporation_id: u64,
    lock: bool,
) -> Result<Option<OperationScopeRow>> {
    let query = if lock {
        sqlx::query_as::<_, OperationScopeRow>(
            "SELECT save.id AS save_id, save.run_revision, save.state_revision, save.game_day,
                save.cash_krw AS wallet_cash_krw, bundle.policy_set_id,
                corporation.id AS corporation_id, corporation.status AS corporation_status,
                corporation.cash_krw AS corporation_cash_krw,
                corporation.contributed_capital_krw, corporation.retained_earnings_krw,
                corporation.operating_payable_krw, corporation.corporate_tax_payable_krw,
                corporation.distributable_profit_krw,
                manifest.business_catalog_version_id, manifest.business_catalog_sha256
         FROM save
         INNER JOIN corporation ON corporation.save_id = save.id
            AND corporation.run_revision = save.run_revision AND corporation.id = ?
         INNER JOIN run_manifest AS manifest ON manifest.save_id = save.id
            AND manifest.run_revision = save.run_revision
         INNER JOIN run_rule_bundle AS bundle ON bundle.save_id = save.id
            AND bundle.run_revision = save.run_revision
         WHERE save.user_id = ? FOR UPDATE",
        )
    } else {
        sqlx::query_as::<_, OperationScopeRow>(
            "SELECT save.id AS save_id, save.run_revision, save.state_revision, save.game_day,
                save.cash_krw AS wallet_cash_krw, bundle.policy_set_id,
                corporation.id AS corporation_id, corporation.status AS corporation_status,
                corporation.cash_krw AS corporation_cash_krw,
                corporation.contributed_capital_krw, corporation.retained_earnings_krw,
                corporation.operating_payable_krw, corporation.corporate_tax_payable_krw,
                corporation.distributable_profit_krw,
                manifest.business_catalog_version_id, manifest.business_catalog_sha256
         FROM save
         INNER JOIN corporation ON corporation.save_id = save.id
            AND corporation.run_revision = save.run_revision AND corporation.id = ?
         INNER JOIN run_manifest AS manifest ON manifest.save_id = save.id
            AND manifest.run_revision = save.run_revision
         INNER JOIN run_rule_bundle AS bundle ON bundle.save_id = save.id
            AND bundle.run_revision = save.run_revision
         WHERE save.user_id = ?",
        )
    };
    query
        .bind(corporation_id)
        .bind(user_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Into::into)
}

async fn read_profile(
    tx: &mut Transaction<'_, MySql>,
    scope: &OperationScopeRow,
    lock: bool,
) -> Result<Option<BusinessProfileRow>> {
    let query = if lock {
        sqlx::query_as::<_, BusinessProfileRow>(
            "SELECT id, business_catalog_version_id, business_catalog_sha256,
                effective_year, effective_month, control_revision
         FROM corporation_business_profile
         WHERE save_id = ? AND run_revision = ? AND corporation_id = ? FOR UPDATE",
        )
    } else {
        sqlx::query_as::<_, BusinessProfileRow>(
            "SELECT id, business_catalog_version_id, business_catalog_sha256,
                effective_year, effective_month, control_revision
         FROM corporation_business_profile
         WHERE save_id = ? AND run_revision = ? AND corporation_id = ?",
        )
    };
    query
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(scope.corporation_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Into::into)
}

async fn read_contract(
    tx: &mut Transaction<'_, MySql>,
    scope: &OperationScopeRow,
    contract_id: u64,
    lock: bool,
) -> Result<Option<ContractRow>> {
    let query = if lock {
        sqlx::query_as::<_, ContractRow>(
            "SELECT contract.id, template.template_key, template.display_name, contract.status,
                contract.service_year, contract.service_month,
                contract.required_capacity_units, contract.revenue_krw,
                contract.variable_cost_ppm, contract.failure_penalty_krw
         FROM corporation_customer_contract AS contract
         INNER JOIN business_contract_template AS template
            ON template.business_catalog_version_id = contract.business_catalog_version_id
           AND template.id = contract.contract_template_id
         WHERE contract.save_id = ? AND contract.run_revision = ?
           AND contract.corporation_id = ? AND contract.id = ? FOR UPDATE",
        )
    } else {
        sqlx::query_as::<_, ContractRow>(
            "SELECT contract.id, template.template_key, template.display_name, contract.status,
                contract.service_year, contract.service_month,
                contract.required_capacity_units, contract.revenue_krw,
                contract.variable_cost_ppm, contract.failure_penalty_krw
         FROM corporation_customer_contract AS contract
         INNER JOIN business_contract_template AS template
            ON template.business_catalog_version_id = contract.business_catalog_version_id
           AND template.id = contract.contract_template_id
         WHERE contract.save_id = ? AND contract.run_revision = ?
           AND contract.corporation_id = ? AND contract.id = ?",
        )
    };
    query
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(scope.corporation_id)
        .bind(contract_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Into::into)
}

async fn read_position(
    tx: &mut Transaction<'_, MySql>,
    scope: &OperationScopeRow,
    position_id: u64,
    lock: bool,
) -> Result<Option<PositionRow>> {
    let query = if lock {
        sqlx::query_as::<_, PositionRow>(
            "SELECT position.id, role.role_key, role.display_name, position.status,
                role.capacity_units, role.monthly_gross_wage_krw,
                role.employer_cost_rate_ppm, position.effective_year,
                position.effective_month, position.hired_command_id
         FROM corporation_staff_position AS position
         INNER JOIN business_role_template AS role
            ON role.business_catalog_version_id = position.business_catalog_version_id
           AND role.id = position.role_template_id
         WHERE position.save_id = ? AND position.run_revision = ?
           AND position.corporation_id = ? AND position.id = ? FOR UPDATE",
        )
    } else {
        sqlx::query_as::<_, PositionRow>(
            "SELECT position.id, role.role_key, role.display_name, position.status,
                role.capacity_units, role.monthly_gross_wage_krw,
                role.employer_cost_rate_ppm, position.effective_year,
                position.effective_month, position.hired_command_id
         FROM corporation_staff_position AS position
         INNER JOIN business_role_template AS role
            ON role.business_catalog_version_id = position.business_catalog_version_id
           AND role.id = position.role_template_id
         WHERE position.save_id = ? AND position.run_revision = ?
           AND position.corporation_id = ? AND position.id = ?",
        )
    };
    query
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(scope.corporation_id)
        .bind(position_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Into::into)
}

async fn read_latest_plan(
    tx: &mut Transaction<'_, MySql>,
    scope: &OperationScopeRow,
    profile: &BusinessProfileRow,
) -> Result<Option<PlanRow>> {
    sqlx::query_as(
        "SELECT plan.id, plan.effective_year, plan.effective_month, plan.plan_revision,
                plan.marketing_band_id, marketing.band_key AS marketing_band_key,
                plan.cash_buffer_krw,
                CAST(plan.contract_priority_json AS CHAR) AS contract_priority_json
         FROM corporation_monthly_plan AS plan
         INNER JOIN business_marketing_band AS marketing
            ON marketing.business_catalog_version_id = plan.business_catalog_version_id
           AND marketing.id = plan.marketing_band_id
         WHERE plan.save_id = ? AND plan.run_revision = ? AND plan.corporation_id = ?
           AND plan.effective_year = ? AND plan.effective_month = ?
         ORDER BY plan.plan_revision DESC, plan.id DESC LIMIT 1",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_id)
    .bind(profile.effective_year)
    .bind(profile.effective_month)
    .fetch_optional(&mut **tx)
    .await
    .map_err(Into::into)
}

async fn read_plan_by_id(
    tx: &mut Transaction<'_, MySql>,
    scope: &OperationScopeRow,
    plan_id: u64,
) -> Result<Option<PlanRow>> {
    sqlx::query_as(
        "SELECT plan.id, plan.effective_year, plan.effective_month, plan.plan_revision,
                plan.marketing_band_id, marketing.band_key AS marketing_band_key,
                plan.cash_buffer_krw,
                CAST(plan.contract_priority_json AS CHAR) AS contract_priority_json
         FROM corporation_monthly_plan AS plan
         INNER JOIN business_marketing_band AS marketing
            ON marketing.business_catalog_version_id = plan.business_catalog_version_id
           AND marketing.id = plan.marketing_band_id
         WHERE plan.save_id = ? AND plan.run_revision = ?
           AND plan.corporation_id = ? AND plan.id = ?",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.corporation_id)
    .bind(plan_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(Into::into)
}

fn next_month(current_date: Date) -> Result<Date> {
    current_date
        .replace_day(1)
        .context("business current month is invalid")?
        .checked_add(Duration::days(32))
        .context("business next month overflowed")?
        .replace_day(1)
        .context("business next month is invalid")
}

fn has_current_cursor(scope: &OperationScopeRow, cursor: CommandCursor) -> bool {
    cursor.expected_run_revision == scope.run_revision
        && cursor.expected_state_revision == scope.state_revision
        && cursor.expected_game_day == scope.game_day
}

fn action_kind(action: &BusinessOperationAction) -> &'static str {
    match action {
        BusinessOperationAction::AcceptContract { .. } => "acceptContract",
        BusinessOperationAction::CancelContract { .. } => "cancelContract",
        BusinessOperationAction::HirePosition { .. } => "hirePosition",
        BusinessOperationAction::TerminatePosition { .. } => "terminatePosition",
        BusinessOperationAction::SetMonthlyPlan { .. } => "setMonthlyPlan",
        BusinessOperationAction::CapitalContribution { .. } => "capitalContribution",
        BusinessOperationAction::DrawWorkingCapitalLoan { .. } => "drawWorkingCapitalLoan",
        BusinessOperationAction::RepayWorkingCapitalLoan { .. } => "repayWorkingCapitalLoan",
        BusinessOperationAction::Dissolve => "dissolveCorporation",
    }
}

fn operation_fingerprint(command: &ManageBusinessOperationsCommand) -> String {
    let action = match &command.action {
        BusinessOperationAction::AcceptContract { contract_id }
        | BusinessOperationAction::CancelContract { contract_id } => {
            format!("contractId={}", contract_id.get())
        }
        BusinessOperationAction::HirePosition { position_id }
        | BusinessOperationAction::TerminatePosition { position_id } => {
            format!("positionId={}", position_id.get())
        }
        BusinessOperationAction::SetMonthlyPlan {
            marketing_band_id,
            cash_buffer_krw,
            contract_priority_ids,
        } => format!(
            "marketingBandId={}\ncashBufferKrw={}\ncontractPriorityIds={}",
            marketing_band_id.get(),
            cash_buffer_krw,
            contract_priority_ids
                .iter()
                .map(|id| id.get().to_string())
                .collect::<Vec<_>>()
                .join(",")
        ),
        BusinessOperationAction::CapitalContribution { amount_krw } => {
            format!("amountKrw={amount_krw}")
        }
        BusinessOperationAction::DrawWorkingCapitalLoan {
            loan_product_id,
            principal_krw,
        } => format!(
            "loanProductId={}\nprincipalKrw={principal_krw}",
            loan_product_id.get()
        ),
        BusinessOperationAction::RepayWorkingCapitalLoan {
            loan_id,
            principal_krw,
        } => format!("loanId={}\nprincipalKrw={principal_krw}", loan_id.get()),
        BusinessOperationAction::Dissolve => "dissolve=true".to_owned(),
    };
    let canonical = format!(
        "lifeledger.corporation.operation.v1\nkind={}\ncorporationId={}\nexpectedRevision={}\nrunRevision={}\nstateRevision={}\ngameDay={}\n{}",
        action_kind(&command.action),
        command.corporation_id.get(),
        command.expected_revision,
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        action
    );
    format!("{:x}", Sha256::digest(canonical.as_bytes()))
}

fn marketing_state(row: MarketingRow) -> BusinessMarketingBandState {
    BusinessMarketingBandState {
        id: ResourceId::from_u64(row.id),
        band_key: row.band_key,
        display_name: row.display_name,
        band_order: row.band_order,
        monthly_cost_krw: row.monthly_cost_krw,
        offer_slots: row.offer_slots,
    }
}

fn loan_product_state(row: LoanProductRow) -> BusinessLoanProductState {
    BusinessLoanProductState {
        id: ResourceId::from_u64(row.id),
        product_key: row.product_key,
        display_name: row.display_name,
        minimum_principal_krw: row.minimum_principal_krw,
        maximum_principal_krw: row.maximum_principal_krw,
        principal_step_krw: row.principal_step_krw,
        monthly_interest_rate_ppm: row.monthly_interest_rate_ppm,
        term_months: row.term_months,
        personal_guarantee: row.personal_guarantee,
    }
}

fn contract_state(row: ContractRow) -> Result<BusinessContractState> {
    Ok(BusinessContractState {
        id: ResourceId::from_u64(row.id),
        template_key: row.template_key,
        display_name: row.display_name,
        status: parse_contract_status(&row.status)?,
        service_year: row.service_year,
        service_month: row.service_month,
        required_capacity_units: row.required_capacity_units,
        revenue_krw: row.revenue_krw,
        variable_cost_ppm: row.variable_cost_ppm,
        failure_penalty_krw: row.failure_penalty_krw,
    })
}

fn position_state(row: PositionRow) -> Result<BusinessPositionState> {
    Ok(BusinessPositionState {
        id: ResourceId::from_u64(row.id),
        role_key: row.role_key,
        display_name: row.display_name,
        status: parse_position_status(&row.status)?,
        capacity_units: row.capacity_units,
        monthly_gross_wage_krw: row.monthly_gross_wage_krw,
        employer_cost_rate_ppm: row.employer_cost_rate_ppm,
        effective_year: row.effective_year,
        effective_month: row.effective_month,
    })
}

fn plan_state(row: PlanRow) -> Result<BusinessMonthlyPlanState> {
    let raw_ids: Vec<String> = serde_json::from_str(&row.contract_priority_json)?;
    let contract_priority_ids = raw_ids
        .into_iter()
        .map(|raw| {
            raw.parse::<u64>()
                .context("business plan contract ID is invalid")
                .map(ResourceId::from_u64)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(BusinessMonthlyPlanState {
        id: ResourceId::from_u64(row.id),
        effective_year: row.effective_year,
        effective_month: row.effective_month,
        plan_revision: row.plan_revision,
        marketing_band_id: ResourceId::from_u64(row.marketing_band_id),
        marketing_band_key: row.marketing_band_key,
        cash_buffer_krw: row.cash_buffer_krw,
        contract_priority_ids,
    })
}

fn business_month_state(row: BusinessMonthRow) -> Result<BusinessMonthState> {
    Ok(BusinessMonthState {
        id: ResourceId::from_u64(row.id),
        operating_year: row.operating_year,
        operating_month: row.operating_month,
        total_capacity_units: row.total_capacity_units,
        used_capacity_units: row.used_capacity_units,
        contract_revenue_krw: row.contract_revenue_krw,
        contract_variable_cost_krw: row.contract_variable_cost_krw,
        marketing_cost_krw: row.marketing_cost_krw,
        employee_cost_krw: row
            .employee_gross_wage_krw
            .checked_add(row.employee_employer_cost_krw)
            .context("business month employee cost overflowed")?,
        failed_contract_penalty_krw: row.failed_contract_penalty_krw,
        loan_interest_cost_krw: row.loan_interest_cost_krw,
        completed_contract_count: row.completed_contract_count,
        failed_contract_count: row.failed_contract_count,
        active_employee_count: row.active_employee_count,
        applied_game_day: row.applied_game_day,
    })
}

fn parse_contract_status(raw: &str) -> Result<BusinessContractStatusState> {
    match raw {
        "offered" => Ok(BusinessContractStatusState::Offered),
        "accepted" => Ok(BusinessContractStatusState::Accepted),
        "active" => Ok(BusinessContractStatusState::Active),
        "completed" => Ok(BusinessContractStatusState::Completed),
        "failed" => Ok(BusinessContractStatusState::Failed),
        "cancelled" => Ok(BusinessContractStatusState::Cancelled),
        _ => bail!("stored business contract status is invalid"),
    }
}

fn parse_position_status(raw: &str) -> Result<BusinessPositionStatusState> {
    match raw {
        "vacant" => Ok(BusinessPositionStatusState::Vacant),
        "hired" => Ok(BusinessPositionStatusState::Hired),
        "active" => Ok(BusinessPositionStatusState::Active),
        "resigned" => Ok(BusinessPositionStatusState::Resigned),
        "terminated" => Ok(BusinessPositionStatusState::Terminated),
        _ => bail!("stored business position status is invalid"),
    }
}
