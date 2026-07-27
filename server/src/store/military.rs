use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use sqlx::{MySql, MySqlPool, QueryBuilder, Transaction};
use time::{Date, Duration};

use super::career::{
    increment_state_revision, lock_save_for_user, replay_or_conflict, validate_current,
};
use super::employment::{
    MilitaryEmploymentPayrollInput, calculate_military_employment_payroll_in_tx,
};
use super::employment_income::{
    EmploymentIncomeAmounts, EmploymentIncomeEventSource, EmploymentIncomeEventWrite,
    record_employment_income_event_in_tx,
};
use super::life::read_tax_dependent_count_in_tx;
use super::mysql::{
    CommandIdentitySpec, GameCommandReceiptWrite, read_state, write_command_identity,
    write_game_command_receipt, write_ledger_transaction,
};
use super::types::{
    ActiveMilitarySavingsState, ActiveMilitaryServiceState, CareerMilitaryStatus,
    CareerPendingScheduleItemState, CareerStoreResult, CloseMilitarySavingsCommand,
    MilitaryExperienceCreditState, MilitaryHardRequirementsState,
    MilitaryOptionIneligibilityReason, MilitaryOptionState, MilitaryOptionsState,
    MilitaryPayStageState, MilitarySavingsCommandReceipt, MilitarySavingsContractStatus,
    MilitarySavingsDayCountConvention, MilitarySavingsHistoryItemState,
    MilitarySavingsIneligibilityReason, MilitarySavingsInstallmentState,
    MilitarySavingsInterestRounding, MilitarySavingsInterestTierState,
    MilitarySavingsMaturityProjectionState, MilitarySavingsPageState, MilitarySavingsProductState,
    MilitarySavingsProductsState, MilitarySavingsProjectionAssumption,
    MilitaryServiceCommandReceipt, MilitaryServiceHistoryState, MilitaryServiceState,
    MilitaryServiceType, OpenMilitarySavingsCommand, StartMilitaryServiceCommand,
};
use crate::career::{
    ActiveMilitarySavingsContract, CareerFailureCode, MilitaryEligibilityInput, MilitaryError,
    MilitaryExperiencePolicy, MilitaryOptionPolicy, MilitaryPayScheduleInput,
    MilitaryPayStageInput, MilitaryPayStagePolicy, MilitarySavingsEarlyCloseInput,
    MilitarySavingsEnrollmentInput, MilitarySavingsInstallmentInput,
    MilitarySavingsInstallmentStatus, MilitarySavingsMaturityInput, MilitarySavingsPolicy,
    MilitarySavingsProductPolicy, MilitaryServiceDayInput, MilitaryServicePlan,
    MilitaryServiceStartInput, MilitaryServiceStatus, MilitaryServiceTransitionInput,
    MilitaryStatus, PaidMilitarySavingsInstallment, create_military_rules,
};
use crate::character::Education;
use crate::finance::{
    FinanceRules, LedgerAccountCode, LedgerPosting, LedgerSource, LedgerSourceKind,
    LedgerTransactionDraft, ResourceId, RunId, RunPolicyContext, ScheduledSettlement,
    SettlementKind, SettlementSourceKind,
};

const MILITARY_PAGE_LIMIT: u32 = 200;
const SNAPSHOT_SCHEDULE_LIMIT: u32 = 20;
const COMMAND_KIND_START_SERVICE: &str = "startMilitaryService";
const COMMAND_KIND_OPEN_SAVINGS: &str = "openMilitarySavings";
const COMMAND_KIND_CLOSE_SAVINGS: &str = "closeMilitarySavings";
const MILITARY_PAYLOAD_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy)]
pub(super) struct MilitarySettlementContext {
    pub(super) save_id: u64,
    pub(super) run_revision: u32,
    pub(super) finance_policy_set_id: u64,
    pub(super) game_day: u32,
    pub(super) market_date: Date,
    pub(super) settlement_id: u64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MilitaryPayPayload {
    version: u8,
    military_service_id: ResourceId,
    period_no: u64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MilitarySavingsInstallmentPayload {
    version: u8,
    contract_id: ResourceId,
    installment_no: u64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MilitarySavingsMaturityPayload {
    version: u8,
    contract_id: ResourceId,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MilitarySavingsGovernmentMatchPayload {
    version: u8,
    contract_id: ResourceId,
    installment_no: u64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct MilitarySettlementRow {
    id: u64,
    due_game_day: u32,
    kind: String,
    payload_json: String,
    source_kind: String,
    source_id: String,
    occurrence: u64,
    status: String,
}

#[derive(sqlx::FromRow)]
struct MilitaryReadScopeRow {
    save_id: u64,
    run_revision: u32,
    game_day: u32,
    market_date: Date,
    career_catalog_bundle_id: u64,
    employment_policy_set_id: u64,
    military_status: String,
    education: String,
    certifications: u32,
    experience_days: u32,
}

#[derive(sqlx::FromRow)]
struct MilitaryOptionRow {
    id: u64,
    option_key: String,
    service_type: String,
    display_name: String,
    effort_life_status: String,
    compensation_kind: String,
    pay_schedule: String,
    grants_career_experience: bool,
    minimum_education: Option<String>,
    required_certification_count: u32,
    minimum_experience_days: u32,
    policy_id: Option<u64>,
    service_duration_months: Option<u16>,
    availability_status: Option<String>,
    effort_units: u64,
}

#[derive(sqlx::FromRow)]
struct MilitaryPayStageRow {
    military_option_version_id: u64,
    start_service_month: u16,
    end_service_month_exclusive: u16,
    monthly_gross_pay_krw: i64,
}

#[derive(sqlx::FromRow)]
struct MilitaryExperienceRow {
    military_option_version_id: u64,
    job_family_key: String,
    experience_credit_ppm: i64,
}

#[derive(sqlx::FromRow)]
struct MilitaryServiceRow {
    id: u64,
    military_option_version_id: u64,
    service_type: String,
    display_name: String,
    status: String,
    source_kind: String,
    start_game_day: u32,
    end_game_day: u32,
    start_date: Date,
    end_exclusive_date: Date,
    credited_service_days: u32,
    completed_game_day: Option<u32>,
    effort_life_status: String,
    grants_career_experience: bool,
    next_pay_game_day: Option<u32>,
}

#[derive(sqlx::FromRow)]
struct ActiveSavingsRow {
    id: u64,
    product_version_id: u64,
    institution_key: String,
    status: String,
    monthly_contribution_krw: i64,
    debit_day_of_month: u8,
    principal_krw: i64,
    paid_installment_count: u16,
    missed_installment_count: u16,
    next_installment_game_day: Option<u32>,
    maturity_game_day: u32,
}

#[derive(sqlx::FromRow)]
struct PendingScheduleRow {
    source_kind: String,
    id: u64,
    due_game_day: u32,
    kind: String,
}

#[derive(sqlx::FromRow)]
struct SavingsProductRow {
    id: u64,
    product_key: String,
    institution_key: String,
    institution_display_name: String,
    available_from: Date,
    available_to_exclusive: Option<Date>,
    day_count_denominator: u16,
    interest_rounding_kind: String,
    interest_rounding_unit_krw: i64,
    early_termination_rate_bp: u16,
    policy_id: Option<u64>,
    policy_effective_from: Option<Date>,
    policy_effective_to_exclusive: Option<Date>,
    join_through: Option<Date>,
    minimum_remaining_service_months: Option<u16>,
    max_contracts_per_service: Option<u8>,
    max_contracts_per_institution: Option<u8>,
    institution_monthly_limit_krw: Option<i64>,
    person_monthly_limit_krw: Option<i64>,
    limit_setting_unit_krw: Option<i64>,
    minimum_installment_krw: Option<i64>,
    installment_unit_krw: Option<i64>,
    government_match_rate_ppm: Option<u32>,
    government_match_next_month_day: Option<u8>,
    tax_exempt: Option<bool>,
    active_service_type: Option<String>,
    service_end_exclusive_date: Option<Date>,
    active_contract_count: i64,
    institution_contract_count: i64,
}

#[derive(sqlx::FromRow)]
struct SavingsRateRow {
    military_savings_product_id: u64,
    minimum_term_months: u16,
    maximum_term_months_exclusive: u16,
    fixed_rate_bp: u16,
}

#[derive(sqlx::FromRow)]
struct SavingsHistoryRow {
    id: u64,
    service_id: u64,
    product_version_id: u64,
    product_key: String,
    institution_key: String,
    institution_display_name: String,
    status: String,
    monthly_contribution_krw: i64,
    debit_day_of_month: u8,
    principal_krw: i64,
    paid_installment_count: u16,
    missed_installment_count: u16,
    next_installment_game_day: Option<u32>,
    maturity_game_day: u32,
    opened_game_day: u32,
    first_installment_game_day: u32,
    term_months: u16,
    fixed_rate_bp: u16,
    closed_game_day: Option<u32>,
    closure_kind: Option<String>,
    bank_interest_krw: i64,
    government_match_received_krw: i64,
    government_match_paid_game_day: Option<u32>,
    maturity_date: Date,
    day_count_denominator: u16,
    interest_rounding_unit_krw: i64,
    government_match_next_month_day: u8,
    government_match_rate_ppm: u32,
}

#[derive(sqlx::FromRow)]
struct SavingsInstallmentRow {
    id: u64,
    military_savings_contract_id: u64,
    installment_no: u16,
    due_game_day: u32,
    status: String,
    paid_game_day: Option<u32>,
    paid_date: Option<Date>,
    paid_principal_krw: i64,
    matching_policy_id: Option<u64>,
    matching_rate_ppm: Option<u32>,
}

#[derive(sqlx::FromRow)]
struct CommandOptionPolicyRow {
    option_version_id: u64,
    option_policy_id: u64,
    service_type: String,
    service_duration_months: u16,
    pay_schedule_kind: String,
    payday_day_of_month: u8,
    partial_month_pay_kind: String,
    minimum_education: Option<String>,
    required_certification_count: u32,
    minimum_experience_days: u32,
    effort_life_status: String,
    effort_units: u64,
}

#[derive(sqlx::FromRow)]
struct SavingsEnrollmentRow {
    military_service_id: u64,
    service_type: String,
    service_end_game_day: u32,
    service_end_exclusive_date: Date,
    savings_policy_id: u64,
    minimum_remaining_service_months: u16,
    max_contracts_per_service: u8,
    max_contracts_per_institution: u8,
    institution_monthly_limit_krw: i64,
    person_monthly_limit_krw: i64,
    limit_setting_unit_krw: i64,
    minimum_installment_krw: i64,
    installment_unit_krw: i64,
    government_match_rate_ppm: u32,
    government_match_next_month_day: u8,
    product_version_id: u64,
    military_savings_institution_id: u64,
    institution_contract_count: i64,
    institution_key: String,
    day_count_denominator: u16,
    interest_rounding_unit_krw: i64,
    early_termination_rate_bp: u16,
}

#[derive(sqlx::FromRow)]
struct ActiveContractPolicyRow {
    institution_key: String,
    monthly_contribution_krw: i64,
}

#[derive(sqlx::FromRow)]
struct SavingsCloseRow {
    id: u64,
    maturity_game_day: u32,
    maturity_date: Date,
    early_termination_rate_bp: u16,
    day_count_denominator: u16,
    interest_rounding_unit_krw: i64,
}

#[derive(sqlx::FromRow)]
struct MilitaryPayServiceRow {
    id: u64,
    save_id: u64,
    run_revision: u32,
    status: String,
    employment_policy_set_id: u64,
    military_option_version_id: u64,
    military_option_policy_id: u64,
    service_type: String,
    service_duration_months: u16,
    start_game_day: u32,
    end_game_day: u32,
    start_date: Date,
    end_exclusive_date: Date,
    credited_service_days: u32,
    last_credited_game_day: Option<u32>,
    pay_schedule_kind: String,
    payday_day_of_month: u8,
    partial_month_pay_kind: String,
    minimum_education: Option<String>,
    required_certification_count: u32,
    minimum_experience_days: u32,
    effort_life_status: String,
    effort_units: u64,
    compensation_calculation_kind: String,
    social_insurance_kind: String,
}

#[derive(sqlx::FromRow)]
struct MilitaryInstallmentSettlementRow {
    contract_id: u64,
    contract_status: String,
    employment_policy_set_id: u64,
    monthly_contribution_krw: i64,
    installment_id: u64,
    installment_no: u16,
    due_game_day: u32,
    installment_status: String,
    wallet_cash_krw: i64,
}

#[derive(sqlx::FromRow)]
struct MilitaryMaturitySettlementRow {
    contract_id: u64,
    contract_status: String,
    term_months: u16,
    maturity_game_day: u32,
    maturity_date: Date,
    fixed_rate_bp: u16,
    day_count_denominator: u16,
    interest_rounding_unit_krw: i64,
    government_match_next_month_day: u8,
    service_status: String,
}

#[derive(sqlx::FromRow)]
struct MilitaryGovernmentMatchSettlementRow {
    installment_id: u64,
    contract_id: u64,
    installment_no: u16,
    installment_status: String,
    government_match_krw: i64,
    government_match_settlement_id: Option<u64>,
    contract_status: String,
}

pub(super) fn validate_military_settlement_envelope(
    settlement: &ScheduledSettlement,
) -> Result<()> {
    match settlement.kind {
        SettlementKind::MilitaryPay => {
            ensure!(
                settlement.source.kind == SettlementSourceKind::MilitaryService,
                "military pay settlement has the wrong source kind"
            );
            let payload: MilitaryPayPayload = serde_json::from_value(settlement.payload.clone())
                .context("stored military pay payload is invalid")?;
            ensure!(
                payload.version == MILITARY_PAYLOAD_VERSION
                    && payload.period_no > 0
                    && settlement.source.source_id == payload.military_service_id.to_string()
                    && settlement.source.occurrence == payload.period_no,
                "military pay settlement identity is invalid"
            );
        }
        SettlementKind::MilitarySavingsInstallment => {
            ensure!(
                settlement.source.kind == SettlementSourceKind::MilitarySavingsContract,
                "military savings installment has the wrong source kind"
            );
            let payload: MilitarySavingsInstallmentPayload =
                serde_json::from_value(settlement.payload.clone())
                    .context("stored military savings installment payload is invalid")?;
            ensure!(
                payload.version == MILITARY_PAYLOAD_VERSION
                    && payload.installment_no > 0
                    && settlement.source.source_id == payload.contract_id.to_string()
                    && settlement.source.occurrence == payload.installment_no,
                "military savings installment identity is invalid"
            );
        }
        SettlementKind::MilitarySavingsMaturity => {
            ensure!(
                settlement.source.kind == SettlementSourceKind::MilitarySavingsContract,
                "military savings maturity has the wrong source kind"
            );
            let payload: MilitarySavingsMaturityPayload =
                serde_json::from_value(settlement.payload.clone())
                    .context("stored military savings maturity payload is invalid")?;
            ensure!(
                payload.version == MILITARY_PAYLOAD_VERSION
                    && settlement.source.source_id == payload.contract_id.to_string()
                    && settlement.source.occurrence > 1,
                "military savings maturity identity is invalid"
            );
        }
        SettlementKind::MilitarySavingsGovernmentMatch => {
            ensure!(
                settlement.source.kind == SettlementSourceKind::MilitarySavingsInstallment,
                "military savings government match has the wrong source kind"
            );
            let payload: MilitarySavingsGovernmentMatchPayload =
                serde_json::from_value(settlement.payload.clone())
                    .context("stored military savings government match payload is invalid")?;
            ensure!(
                payload.version == MILITARY_PAYLOAD_VERSION
                    && payload.installment_no > 0
                    && settlement.source.occurrence == 1,
                "military savings government match identity is invalid"
            );
        }
        _ => bail!("settlement is not a military settlement"),
    }
    Ok(())
}

pub(super) async fn initialize_legacy_military_service_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<()> {
    let character_military: String = sqlx::query_scalar(
        "SELECT `character`.military
         FROM save INNER JOIN `character` ON `character`.save_id = save.id
         WHERE save.id = ? AND save.run_revision = ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await?;
    let service_type = match character_military.as_str() {
        "serving" => "activeDuty",
        "alternative" => "socialService",
        _ => return Ok(()),
    };
    let scope = read_scope_for_save(tx, save_id, run_revision).await?;
    if scope.military_status == "completed" {
        return Ok(());
    }
    ensure!(
        scope.military_status == "serving",
        "legacy serving character did not initialize a serving career run"
    );
    let existing_services: Vec<(u64, String)> = sqlx::query_as(
        "SELECT id, source_kind FROM military_service
         WHERE save_id = ? AND run_revision = ?
         ORDER BY id LIMIT 2 FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        existing_services.len() <= 1,
        "legacy run has multiple military services"
    );
    if let Some((service_id, source_kind)) = existing_services.first() {
        ensure!(
            source_kind == "legacyBridge",
            "legacy run has a non-bridge military service"
        );
        let service =
            load_military_service_for_update(tx, save_id, run_revision, *service_id).await?;
        ensure!(
            service.service_type == service_type
                && matches!(service.status.as_str(), "pendingStart" | "serving"),
            "legacy military service disagrees with the character bridge"
        );
        let option = load_pinned_service_option(tx, &service).await?;
        let service_plan = military_service_plan(&service)?;
        repair_legacy_military_pay_schedule_in_tx(
            tx,
            save_id,
            run_revision,
            *service_id,
            scope.game_day,
            &service_plan,
            &option,
        )
        .await?;
        return Ok(());
    }
    let option_ids: Vec<u64> = sqlx::query_scalar(
        "SELECT id FROM military_option_version
         WHERE career_catalog_bundle_id = ? AND service_type = ?
         ORDER BY id LIMIT 2",
    )
    .bind(scope.career_catalog_bundle_id)
    .bind(service_type)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        option_ids.len() == 1,
        "legacy military service type has no unique option"
    );
    let (option_row, option) = load_command_option_policy(tx, &scope, option_ids[0])
        .await?
        .context("legacy military option has no effective policy")?;
    let plan = create_military_rules().plan_service_start(MilitaryServiceStartInput {
        current_status: MilitaryStatus::Unserved,
        current_game_day: scope.game_day,
        current_date: scope.market_date,
        eligibility: MilitaryEligibilityInput {
            military_subject: true,
            education: enum_from_db(&scope.education)?,
            certification_count: scope.certifications,
            experience_days: scope.experience_days,
        },
        option: &option,
    })?;
    ensure!(
        enum_to_db(&plan.service_type)? == service_type,
        "legacy military option resolved to another service type"
    );
    let insert = sqlx::query(
        "INSERT INTO military_service
             (save_id, run_revision, career_catalog_bundle_id, employment_policy_set_id,
              military_option_version_id, military_option_policy_id, service_type,
              status, source_kind, start_command_id, start_game_day, end_game_day,
              start_date, end_exclusive_date, credited_service_days,
              last_credited_game_day, completed_game_day)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'pendingStart', 'legacyBridge', NULL,
                 ?, ?, ?, ?, 0, NULL, NULL)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(scope.career_catalog_bundle_id)
    .bind(scope.employment_policy_set_id)
    .bind(option_row.option_version_id)
    .bind(option_row.option_policy_id)
    .bind(service_type)
    .bind(plan.start_game_day)
    .bind(plan.end_game_day)
    .bind(plan.start_date)
    .bind(plan.end_exclusive_date)
    .execute(&mut **tx)
    .await?;
    let service_id = insert.last_insert_id();
    insert_service_progress_in_tx(
        tx,
        save_id,
        run_revision,
        scope.career_catalog_bundle_id,
        service_id,
        option_row.option_version_id,
    )
    .await?;
    insert_service_actions_in_tx(
        tx,
        save_id,
        run_revision,
        scope.career_catalog_bundle_id,
        service_id,
        plan.start_game_day,
        plan.end_game_day,
    )
    .await?;
    repair_legacy_military_pay_schedule_in_tx(
        tx,
        save_id,
        run_revision,
        service_id,
        scope.game_day,
        &plan,
        &option,
    )
    .await?;
    Ok(())
}

pub(super) async fn read_military_options(
    pool: &MySqlPool,
    user_id: u64,
) -> Result<MilitaryOptionsState> {
    let mut tx = pool.begin().await?;
    let scope = read_scope_for_user(&mut tx, user_id).await?;
    let state = read_options_in_tx(&mut tx, &scope).await?;
    tx.commit().await?;
    Ok(state)
}

pub(super) async fn read_military_service(
    pool: &MySqlPool,
    user_id: u64,
) -> Result<MilitaryServiceState> {
    let mut tx = pool.begin().await?;
    let scope = read_scope_for_user(&mut tx, user_id).await?;
    let service = read_service_history_in_tx(&mut tx, scope.save_id, scope.run_revision).await?;
    tx.commit().await?;
    Ok(MilitaryServiceState {
        military_status: enum_from_db(&scope.military_status)?,
        service,
    })
}

pub(super) async fn read_military_savings_products(
    pool: &MySqlPool,
    user_id: u64,
) -> Result<MilitarySavingsProductsState> {
    let mut tx = pool.begin().await?;
    let scope = read_scope_for_user(&mut tx, user_id).await?;
    let state = read_savings_products_in_tx(&mut tx, &scope).await?;
    tx.commit().await?;
    Ok(state)
}

pub(super) async fn read_military_savings(
    pool: &MySqlPool,
    user_id: u64,
    before: Option<u64>,
    limit: u32,
) -> Result<MilitarySavingsPageState> {
    ensure!(
        (1..=MILITARY_PAGE_LIMIT).contains(&limit),
        "military savings page limit is invalid"
    );
    ensure!(
        before != Some(0),
        "military savings cursor must be positive"
    );
    let mut tx = pool.begin().await?;
    let scope = read_scope_for_user(&mut tx, user_id).await?;
    let state = read_savings_history_in_tx(&mut tx, &scope, before, limit).await?;
    tx.commit().await?;
    Ok(state)
}

pub(super) async fn start_military_service_command(
    pool: &MySqlPool,
    user_id: u64,
    command: &StartMilitaryServiceCommand,
) -> Result<CareerStoreResult<MilitaryServiceCommandReceipt>> {
    let fingerprint = start_service_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(current) = lock_save_for_user(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(
            CareerFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_START_SERVICE,
        payload_sha256: &fingerprint,
        cursor: command.cursor,
    };
    if let Some(result) = replay_or_conflict::<MilitaryServiceCommandReceipt>(
        &mut tx,
        &current,
        &identity,
        &fingerprint,
    )
    .await?
    {
        return finish_service_replay(tx, current.id, result).await;
    }
    if let Some(failure) = validate_current(&current, command.cursor) {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(failure));
    }
    let scope = read_scope_for_user(&mut tx, user_id).await?;
    let Some((option_row, option)) =
        load_command_option_policy(&mut tx, &scope, command.military_option_version_id.get())
            .await?
    else {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(
            CareerFailureCode::PolicyUnavailable,
        ));
    };
    let rules = create_military_rules();
    let plan = match rules.plan_service_start(MilitaryServiceStartInput {
        current_status: enum_from_db(&scope.military_status)?,
        current_game_day: scope.game_day,
        current_date: scope.market_date,
        eligibility: MilitaryEligibilityInput {
            military_subject: scope.military_status == "unserved",
            education: enum_from_db(&scope.education)?,
            certification_count: scope.certifications,
            experience_days: scope.experience_days,
        },
        option: &option,
    }) {
        Ok(plan) => plan,
        Err(error) => {
            tx.commit().await?;
            return Ok(CareerStoreResult::Rejected(military_failure(error)));
        }
    };

    write_command_identity(&mut tx, current.id, &identity).await?;
    let insert = sqlx::query(
        "INSERT INTO military_service
             (save_id, run_revision, career_catalog_bundle_id, employment_policy_set_id,
              military_option_version_id, military_option_policy_id, service_type,
              status, source_kind, start_command_id, start_game_day, end_game_day,
              start_date, end_exclusive_date, credited_service_days,
              last_credited_game_day, completed_game_day)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'pendingStart', 'userCommand', ?, ?, ?, ?, ?, 0, NULL, NULL)",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(scope.career_catalog_bundle_id)
    .bind(scope.employment_policy_set_id)
    .bind(option_row.option_version_id)
    .bind(option_row.option_policy_id)
    .bind(enum_to_db(&plan.service_type)?)
    .bind(command.command_id.as_str())
    .bind(plan.start_game_day)
    .bind(plan.end_game_day)
    .bind(plan.start_date)
    .bind(plan.end_exclusive_date)
    .execute(&mut *tx)
    .await?;
    let service_id = insert.last_insert_id();
    insert_service_progress_in_tx(
        &mut tx,
        current.id,
        current.run_revision,
        scope.career_catalog_bundle_id,
        service_id,
        option_row.option_version_id,
    )
    .await?;
    insert_service_actions_in_tx(
        &mut tx,
        current.id,
        current.run_revision,
        scope.career_catalog_bundle_id,
        service_id,
        plan.start_game_day,
        plan.end_game_day,
    )
    .await?;
    insert_military_pay_schedule_in_tx(
        &mut tx,
        current.id,
        current.run_revision,
        service_id,
        &plan,
        &option,
    )
    .await?;
    let status_update = sqlx::query(
        "UPDATE career_run SET military_status = 'serving'
         WHERE save_id = ? AND run_revision = ? AND military_status = 'unserved'",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .execute(&mut *tx)
    .await?;
    ensure!(
        status_update.rows_affected() == 1,
        "military status was not reserved for the pending service"
    );
    let committed = increment_state_revision(&mut tx, &current, current.cash_krw).await?;
    let receipt = MilitaryServiceCommandReceipt {
        command_id: command.command_id.clone(),
        military_service_id: ResourceId::from_u64(service_id),
        status: plan.service_status,
        replayed: false,
    };
    write_game_command_receipt(
        &mut tx,
        GameCommandReceiptWrite {
            save_id: current.id,
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_START_SERVICE,
            payload_sha256: &fingerprint,
            market_world_id: current.market_world_id,
            committed_cursor: committed,
            result: &receipt,
            ledger_transaction_id: None,
        },
    )
    .await?;
    let save = read_state(&mut tx, current.id).await?;
    tx.commit().await?;
    Ok(CareerStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

pub(super) async fn open_military_savings_command(
    pool: &MySqlPool,
    user_id: u64,
    command: &OpenMilitarySavingsCommand,
) -> Result<CareerStoreResult<MilitarySavingsCommandReceipt>> {
    let fingerprint = open_savings_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(current) = lock_save_for_user(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(
            CareerFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_OPEN_SAVINGS,
        payload_sha256: &fingerprint,
        cursor: command.cursor,
    };
    if let Some(result) = replay_or_conflict::<MilitarySavingsCommandReceipt>(
        &mut tx,
        &current,
        &identity,
        &fingerprint,
    )
    .await?
    {
        return finish_savings_replay(tx, current.id, result).await;
    }
    if let Some(failure) = validate_current(&current, command.cursor) {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(failure));
    }
    let scope = read_scope_for_user(&mut tx, user_id).await?;
    let Some((enrollment, policy, product, active_contracts)) =
        load_savings_enrollment_policy(&mut tx, &scope, command.product_version_id.get()).await?
    else {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(
            CareerFailureCode::PolicyUnavailable,
        ));
    };
    let plan =
        match create_military_rules().plan_savings_enrollment(MilitarySavingsEnrollmentInput {
            external_status: enum_from_db(&scope.military_status)?,
            service_type: enum_from_db(&enrollment.service_type)?,
            current_date: scope.market_date,
            current_game_day: scope.game_day,
            service_end_exclusive_date: enrollment.service_end_exclusive_date,
            service_end_game_day: enrollment.service_end_game_day,
            institution_key: &enrollment.institution_key,
            monthly_contribution_krw: command.monthly_contribution_krw,
            debit_day_of_month: command.debit_day_of_month,
            active_contracts: &active_contracts,
            service_institution_contract_count: u32::try_from(
                enrollment.institution_contract_count,
            )
            .context("military savings institution contract count exceeds u32")?,
            policy: &policy,
            product: &product,
        }) {
            Ok(plan) => plan,
            Err(error) => {
                tx.commit().await?;
                return Ok(CareerStoreResult::Rejected(military_failure(error)));
            }
        };
    let first_installment_game_day = plan
        .installments
        .first()
        .map(|installment| installment.due_game_day)
        .context("military savings plan has no installments")?;

    write_command_identity(&mut tx, current.id, &identity).await?;
    let insert = sqlx::query(
        "INSERT INTO military_savings_contract
             (save_id, run_revision, military_service_id, career_catalog_bundle_id,
              employment_policy_set_id, military_savings_policy_id,
              military_savings_product_id, military_savings_institution_id,
              status, opened_command_id, close_command_id, opened_game_day,
              monthly_contribution_krw, debit_day_of_month, term_months, fixed_rate_bp,
              first_installment_game_day, maturity_game_day, principal_krw,
              paid_installment_count, missed_installment_count, bank_interest_krw,
              government_match_entitlement_krw, government_match_received_krw,
              maturity_ledger_transaction_id, closed_game_day, closure_kind)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'active', ?, NULL, ?, ?, ?, ?, ?, ?, ?,
                 0, 0, 0, 0, 0, 0, NULL, NULL, NULL)",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(enrollment.military_service_id)
    .bind(scope.career_catalog_bundle_id)
    .bind(scope.employment_policy_set_id)
    .bind(enrollment.savings_policy_id)
    .bind(enrollment.product_version_id)
    .bind(enrollment.military_savings_institution_id)
    .bind(command.command_id.as_str())
    .bind(scope.game_day)
    .bind(plan.monthly_contribution_krw)
    .bind(plan.debit_day_of_month)
    .bind(plan.contract_term_months)
    .bind(
        u16::try_from(plan.annual_interest_rate_ppm / 100)
            .context("military savings annual rate cannot be represented as basis points")?,
    )
    .bind(first_installment_game_day)
    .bind(plan.maturity_game_day)
    .execute(&mut *tx)
    .await?;
    let contract_id = insert.last_insert_id();
    insert_savings_schedule_in_tx(
        &mut tx,
        current.id,
        current.run_revision,
        contract_id,
        &plan,
    )
    .await?;
    let committed = increment_state_revision(&mut tx, &current, current.cash_krw).await?;
    let receipt = MilitarySavingsCommandReceipt {
        command_id: command.command_id.clone(),
        military_savings_contract_id: ResourceId::from_u64(contract_id),
        status: MilitarySavingsContractStatus::Active,
        replayed: false,
    };
    write_game_command_receipt(
        &mut tx,
        GameCommandReceiptWrite {
            save_id: current.id,
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_OPEN_SAVINGS,
            payload_sha256: &fingerprint,
            market_world_id: current.market_world_id,
            committed_cursor: committed,
            result: &receipt,
            ledger_transaction_id: None,
        },
    )
    .await?;
    let save = read_state(&mut tx, current.id).await?;
    tx.commit().await?;
    Ok(CareerStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

pub(super) async fn close_military_savings_command(
    pool: &MySqlPool,
    finance_rules: Arc<dyn FinanceRules>,
    user_id: u64,
    command: &CloseMilitarySavingsCommand,
) -> Result<CareerStoreResult<MilitarySavingsCommandReceipt>> {
    let fingerprint = close_savings_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(current) = lock_save_for_user(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(
            CareerFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_CLOSE_SAVINGS,
        payload_sha256: &fingerprint,
        cursor: command.cursor,
    };
    if let Some(result) = replay_or_conflict::<MilitarySavingsCommandReceipt>(
        &mut tx,
        &current,
        &identity,
        &fingerprint,
    )
    .await?
    {
        return finish_savings_replay(tx, current.id, result).await;
    }
    if let Some(failure) = validate_current(&current, command.cursor) {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(failure));
    }

    let contract: Option<SavingsCloseRow> = sqlx::query_as(
        "SELECT contract.id, contract.maturity_game_day,
                DATE_ADD(world.start_date, INTERVAL contract.maturity_game_day DAY)
                    AS maturity_date,
                product.early_termination_rate_bp,
                product.day_count_denominator, product.interest_rounding_unit_krw
         FROM military_savings_contract AS contract
         INNER JOIN save ON save.id = contract.save_id
                        AND save.run_revision = contract.run_revision
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         INNER JOIN military_savings_product_version AS product
           ON product.career_catalog_bundle_id = contract.career_catalog_bundle_id
          AND product.id = contract.military_savings_product_id
         WHERE contract.save_id = ? AND contract.run_revision = ?
           AND contract.id = ? AND contract.status = 'active'
         FOR UPDATE",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(command.contract_id.get())
    .fetch_optional(&mut *tx)
    .await?;
    let Some(contract) = contract else {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(
            CareerFailureCode::InvalidCommand,
        ));
    };
    if current.game_day >= contract.maturity_game_day {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(
            CareerFailureCode::InvalidCommand,
        ));
    }

    let paid_installments = sqlx::query_as::<_, (u16, Date, i64, Option<u32>)>(
        "SELECT installment.installment_no,
                DATE_ADD(world.start_date, INTERVAL installment.paid_game_day DAY),
                installment.paid_principal_krw, installment.matching_rate_ppm
         FROM military_savings_installment AS installment
         INNER JOIN save ON save.id = installment.save_id
                        AND save.run_revision = installment.run_revision
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         WHERE installment.save_id = ? AND installment.run_revision = ?
           AND installment.military_savings_contract_id = ?
           AND installment.status = 'paid'
         ORDER BY installment.installment_no",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(contract.id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(
        |(installment_no, paid_date, principal_krw, matching_rate_ppm)| {
            Ok(PaidMilitarySavingsInstallment {
                installment_no: u32::from(installment_no),
                paid_date,
                principal_krw,
                government_matching_rate_ppm: i64::from(
                    matching_rate_ppm.context("paid military installment has no matching rate")?,
                ),
            })
        },
    )
    .collect::<Result<Vec<_>>>()?;
    let close_date = contract
        .maturity_date
        .checked_sub(Duration::days(i64::from(
            contract.maturity_game_day - current.game_day,
        )))
        .context("military savings close date overflowed")?;
    let plan =
        match create_military_rules().plan_savings_early_close(MilitarySavingsEarlyCloseInput {
            close_date,
            maturity_date: contract.maturity_date,
            early_close_annual_interest_rate_ppm: i64::from(contract.early_termination_rate_bp)
                * 100,
            day_count_denominator: contract.day_count_denominator,
            interest_rounding_unit_krw: contract.interest_rounding_unit_krw,
            paid_installments: &paid_installments,
        }) {
            Ok(plan) => plan,
            Err(error) => {
                tx.commit().await?;
                return Ok(CareerStoreResult::Rejected(military_failure(error)));
            }
        };

    write_command_identity(&mut tx, current.id, &identity).await?;
    let ledger_transaction_id = if plan.wallet_credit_krw == 0 {
        None
    } else {
        let mut postings = vec![LedgerPosting {
            account_code: LedgerAccountCode::Wallet,
            financial_account_id: None,
            amount_krw: plan.wallet_credit_krw,
        }];
        if plan.principal_krw > 0 {
            postings.push(LedgerPosting {
                account_code: LedgerAccountCode::MilitarySavingsPrincipal,
                financial_account_id: None,
                amount_krw: -plan.principal_krw,
            });
        }
        if plan.gross_bank_interest_krw > 0 {
            postings.push(LedgerPosting {
                account_code: LedgerAccountCode::MilitarySavingsBankInterest,
                financial_account_id: None,
                amount_krw: -plan.gross_bank_interest_krw,
            });
        }
        let ledger = finance_rules.create_military_savings_ledger_transaction(
            LedgerTransactionDraft {
                policy: RunPolicyContext {
                    run: RunId {
                        save_id: ResourceId::from_u64(current.id),
                        run_revision: current.run_revision,
                    },
                    policy_set_id: ResourceId::from_u64(current.policy_set_id),
                },
                source: LedgerSource {
                    kind: LedgerSourceKind::MilitarySavingsEarlyClose,
                    source_id: contract.id.to_string(),
                },
                game_day: current.game_day,
                description: "장병내일준비적금 중도해지".to_owned(),
                postings,
            },
            ResourceId::from_u64(contract.id),
        )?;
        Some(write_ledger_transaction(&mut tx, &ledger).await?)
    };

    sqlx::query(
        "UPDATE scheduled_settlement
         SET status = 'cancelled', cancellation_reason = 'militarySavingsEarlyClose'
         WHERE save_id = ? AND run_revision = ? AND status = 'pending'
           AND source_kind = 'militarySavingsContract' AND source_id = ?",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(contract.id.to_string())
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE scheduled_settlement AS settlement
         INNER JOIN military_savings_installment AS installment
           ON installment.save_id = settlement.save_id
          AND installment.run_revision = settlement.run_revision
          AND settlement.source_kind = 'militarySavingsInstallment'
          AND settlement.source_id = CAST(installment.id AS CHAR)
         SET settlement.status = 'cancelled',
             settlement.cancellation_reason = 'militarySavingsEarlyClose'
         WHERE settlement.save_id = ? AND settlement.run_revision = ?
           AND settlement.status = 'pending'
           AND installment.military_savings_contract_id = ?",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(contract.id)
    .execute(&mut *tx)
    .await?;
    let contract_update = sqlx::query(
        "UPDATE military_savings_contract
         SET status = 'closed', close_command_id = ?, bank_interest_krw = ?,
             government_match_entitlement_krw = 0,
             government_match_received_krw = 0,
             maturity_ledger_transaction_id = ?, closed_game_day = ?,
             closure_kind = 'earlyClose'
         WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'active'",
    )
    .bind(command.command_id.as_str())
    .bind(plan.gross_bank_interest_krw)
    .bind(ledger_transaction_id)
    .bind(current.game_day)
    .bind(current.id)
    .bind(current.run_revision)
    .bind(contract.id)
    .execute(&mut *tx)
    .await?;
    ensure!(
        contract_update.rows_affected() == 1,
        "military savings contract changed while closing"
    );
    let next_cash = current
        .cash_krw
        .checked_add(plan.wallet_credit_krw)
        .context("military savings close wallet overflowed")?;
    let committed = increment_state_revision(&mut tx, &current, next_cash).await?;
    let receipt = MilitarySavingsCommandReceipt {
        command_id: command.command_id.clone(),
        military_savings_contract_id: ResourceId::from_u64(contract.id),
        status: MilitarySavingsContractStatus::Closed,
        replayed: false,
    };
    write_game_command_receipt(
        &mut tx,
        GameCommandReceiptWrite {
            save_id: current.id,
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_CLOSE_SAVINGS,
            payload_sha256: &fingerprint,
            market_world_id: current.market_world_id,
            committed_cursor: committed,
            result: &receipt,
            ledger_transaction_id,
        },
    )
    .await?;
    let save = read_state(&mut tx, current.id).await?;
    tx.commit().await?;
    Ok(CareerStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

pub(super) async fn settle_military_by_id_in_tx(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    context: MilitarySettlementContext,
) -> Result<()> {
    let settlement = read_military_settlement(tx, context).await?;
    ensure!(
        settlement.status == "pending" && settlement.due_game_day == context.game_day,
        "military settlement is not due"
    );
    match enum_from_db::<SettlementKind>(&settlement.kind)? {
        SettlementKind::MilitaryPay => {
            settle_military_pay(tx, finance_rules, context, &settlement).await
        }
        SettlementKind::MilitarySavingsInstallment => {
            settle_military_savings_installment(tx, finance_rules, context, &settlement).await
        }
        SettlementKind::MilitarySavingsMaturity => {
            settle_military_savings_maturity(tx, finance_rules, context, &settlement).await
        }
        SettlementKind::MilitarySavingsGovernmentMatch => {
            settle_military_savings_government_match(tx, finance_rules, context, &settlement).await
        }
        _ => bail!("settlement is not a military settlement"),
    }
}

pub(super) async fn advance_military_lifecycle_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    target_game_day: u32,
) -> Result<()> {
    let start_actions: Vec<(u64, u64)> = sqlx::query_as(
        "SELECT id, military_service_id
         FROM career_scheduled_action
         WHERE save_id = ? AND run_revision = ? AND status = 'pending'
           AND action_kind = 'militaryServiceStart' AND due_game_day = ?
         ORDER BY phase_rank, id FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(target_game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        start_actions.len() <= 1,
        "multiple military services start today"
    );
    for (action_id, service_id) in start_actions {
        let service =
            load_military_service_for_update(tx, save_id, run_revision, service_id).await?;
        let transition =
            create_military_rules().transition_service(MilitaryServiceTransitionInput {
                external_status: MilitaryStatus::Serving,
                service_status: enum_from_db(&service.status)?,
                current_game_day: target_game_day,
                start_game_day: service.start_game_day,
                end_game_day: service.end_game_day,
            })?;
        ensure!(
            transition.service_status == MilitaryServiceStatus::Serving,
            "military start action did not enter service"
        );
        let update = sqlx::query(
            "UPDATE military_service SET status = 'serving'
             WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'pendingStart'",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(service_id)
        .execute(&mut **tx)
        .await?;
        ensure!(
            update.rows_affected() == 1,
            "military start transition was lost"
        );
        complete_military_action(tx, save_id, run_revision, action_id, target_game_day).await?;
    }

    let serving_ids: Vec<u64> = sqlx::query_scalar(
        "SELECT id FROM military_service
         WHERE save_id = ? AND run_revision = ? AND status = 'serving'
         ORDER BY id FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        serving_ids.len() <= 1,
        "multiple military services are active"
    );
    if let Some(service_id) = serving_ids.first().copied() {
        let service =
            load_military_service_for_update(tx, save_id, run_revision, service_id).await?;
        if target_game_day < service.end_game_day {
            let option = load_pinned_service_option(tx, &service).await?;
            let service_plan = military_service_plan(&service)?;
            let effect = create_military_rules().plan_service_day(MilitaryServiceDayInput {
                current_game_day: target_game_day,
                service: service_plan,
                option: &option,
            })?;
            ensure!(
                effect.credited_service_days == 1
                    && effect.effort_life_status == option.effort_life_status
                    && effect.available_effort_units == option.daily_effort_capacity_units,
                "military daily effect disagrees with its pinned option"
            );
            let next_credited_days = service
                .credited_service_days
                .checked_add(effect.credited_service_days)
                .context("military service credit overflowed")?;
            let service_update = sqlx::query(
                "UPDATE military_service
                 SET credited_service_days = ?, last_credited_game_day = ?
                 WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'serving'",
            )
            .bind(next_credited_days)
            .bind(target_game_day)
            .bind(save_id)
            .bind(run_revision)
            .bind(service_id)
            .execute(&mut **tx)
            .await?;
            ensure!(
                service_update.rows_affected() == 1,
                "military daily credit was lost"
            );
            let progress_update = sqlx::query(
                "UPDATE military_service_progress
                 SET credited_experience_day_ppm
                        = credited_experience_day_ppm + experience_credit_ppm,
                     last_credited_game_day = ?
                 WHERE save_id = ? AND run_revision = ?
                   AND military_service_id = ? AND status = 'active'",
            )
            .bind(target_game_day)
            .bind(save_id)
            .bind(run_revision)
            .bind(service_id)
            .execute(&mut **tx)
            .await?;
            ensure!(
                progress_update.rows_affected()
                    == u64::try_from(effect.experience.len())
                        .context("military experience mapping count overflowed")?,
                "military experience progress disagrees with its pinned option"
            );
        }
    }

    let completion_actions: Vec<(u64, u64)> = sqlx::query_as(
        "SELECT id, military_service_id
         FROM career_scheduled_action
         WHERE save_id = ? AND run_revision = ? AND status = 'pending'
           AND action_kind = 'militaryServiceCompletion' AND due_game_day = ?
         ORDER BY phase_rank, id FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(target_game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        completion_actions.len() <= 1,
        "multiple military services complete today"
    );
    for (action_id, service_id) in completion_actions {
        let service =
            load_military_service_for_update(tx, save_id, run_revision, service_id).await?;
        let transition =
            create_military_rules().transition_service(MilitaryServiceTransitionInput {
                external_status: MilitaryStatus::Serving,
                service_status: enum_from_db(&service.status)?,
                current_game_day: target_game_day,
                start_game_day: service.start_game_day,
                end_game_day: service.end_game_day,
            })?;
        ensure!(
            transition.external_status == MilitaryStatus::Completed
                && transition.service_status == MilitaryServiceStatus::Completed
                && service.credited_service_days == service.end_game_day - service.start_game_day
                && service.last_credited_game_day == service.end_game_day.checked_sub(1),
            "military service cannot complete before all service days are credited"
        );
        let service_update = sqlx::query(
            "UPDATE military_service
             SET status = 'completed', completed_game_day = ?
             WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'serving'",
        )
        .bind(target_game_day)
        .bind(save_id)
        .bind(run_revision)
        .bind(service_id)
        .execute(&mut **tx)
        .await?;
        ensure!(
            service_update.rows_affected() == 1,
            "military completion transition was lost"
        );
        let run_update = sqlx::query(
            "UPDATE career_run SET military_status = 'completed'
             WHERE save_id = ? AND run_revision = ? AND military_status = 'serving'",
        )
        .bind(save_id)
        .bind(run_revision)
        .execute(&mut **tx)
        .await?;
        ensure!(
            run_update.rows_affected() == 1,
            "career military status was not completed"
        );
        sqlx::query(
            "UPDATE military_service_progress SET status = 'finalized'
             WHERE save_id = ? AND run_revision = ?
               AND military_service_id = ? AND status = 'active'",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(service_id)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT INTO spec_evidence
                 (save_id, run_revision, career_catalog_bundle_id, evidence_key,
                  spec_catalog_entry_id, kind, acquired_game_day, expires_on_game_day,
                  period_start_date, period_end_exclusive_date,
                  credited_experience_days, source_kind, source_activity_id,
                  source_employment_contract_id, source_military_service_id)
             SELECT service.save_id, service.run_revision, service.career_catalog_bundle_id,
                    CONCAT('militaryService:', service.id, ':', mapping.career_job_family_id),
                    mapping.spec_catalog_entry_id, 'experience', service.end_game_day, NULL,
                    service.start_date, service.end_exclusive_date,
                    FLOOR(progress.credited_experience_day_ppm / 1000000),
                    'militaryService', NULL, NULL, service.id
             FROM military_service AS service
             INNER JOIN military_option_experience_evidence_mapping AS mapping
               ON mapping.career_catalog_bundle_id = service.career_catalog_bundle_id
              AND mapping.military_option_version_id = service.military_option_version_id
             INNER JOIN military_service_progress AS progress
               ON progress.save_id = service.save_id
              AND progress.run_revision = service.run_revision
              AND progress.military_service_id = service.id
              AND progress.career_job_family_id = mapping.career_job_family_id
              AND progress.status = 'finalized'
             WHERE service.save_id = ? AND service.run_revision = ? AND service.id = ?
               AND service.status = 'completed'
             ORDER BY mapping.career_job_family_id",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(service_id)
        .execute(&mut **tx)
        .await?;
        complete_military_action(tx, save_id, run_revision, action_id, target_game_day).await?;
    }
    Ok(())
}

fn military_service_plan(service: &MilitaryPayServiceRow) -> Result<MilitaryServicePlan> {
    Ok(MilitaryServicePlan {
        option_version_id: service.military_option_version_id,
        service_type: enum_from_db(&service.service_type)?,
        external_status: if service.status == "completed" {
            MilitaryStatus::Completed
        } else {
            MilitaryStatus::Serving
        },
        service_status: enum_from_db(&service.status)?,
        start_game_day: service.start_game_day,
        end_game_day: service.end_game_day,
        start_date: service.start_date,
        end_exclusive_date: service.end_exclusive_date,
    })
}

async fn complete_military_action(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    action_id: u64,
    target_game_day: u32,
) -> Result<()> {
    let update = sqlx::query(
        "UPDATE career_scheduled_action
         SET status = 'completed', completed_game_day = ?
         WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'pending'",
    )
    .bind(target_game_day)
    .bind(save_id)
    .bind(run_revision)
    .bind(action_id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "military lifecycle action changed"
    );
    Ok(())
}

async fn read_military_settlement(
    tx: &mut Transaction<'_, MySql>,
    context: MilitarySettlementContext,
) -> Result<MilitarySettlementRow> {
    sqlx::query_as(
        "SELECT id, due_game_day, kind, CAST(payload AS CHAR) AS payload_json,
                source_kind, source_id, occurrence, status
         FROM scheduled_settlement
         WHERE save_id = ? AND run_revision = ? AND id = ? FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(context.settlement_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("military settlement is missing")
}

async fn settle_military_pay(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    context: MilitarySettlementContext,
    settlement: &MilitarySettlementRow,
) -> Result<()> {
    let payload: MilitaryPayPayload = serde_json::from_str(&settlement.payload_json)
        .context("stored military pay payload is invalid")?;
    ensure!(
        payload.version == MILITARY_PAYLOAD_VERSION
            && payload.period_no > 0
            && settlement.source_kind == "militaryService"
            && settlement.source_id == payload.military_service_id.to_string()
            && settlement.occurrence == payload.period_no,
        "military pay settlement identity is invalid"
    );
    let service = load_military_service_for_update(
        tx,
        context.save_id,
        context.run_revision,
        payload.military_service_id.get(),
    )
    .await?;
    ensure!(
        matches!(service.status.as_str(), "serving" | "completed")
            && context.game_day >= service.start_game_day
            && context.game_day < service.end_game_day,
        "military pay is outside the service period"
    );
    let option = load_pinned_service_option(tx, &service).await?;
    let pay_stage = create_military_rules().select_pay_stage(MilitaryPayStageInput {
        service_date: context.market_date,
        service_start_date: service.start_date,
        service_end_exclusive_date: service.end_exclusive_date,
        option: &option,
    })?;
    let amounts = match (
        service.compensation_calculation_kind.as_str(),
        service.social_insurance_kind.as_str(),
    ) {
        ("militaryStage", "notAssessed") => EmploymentIncomeAmounts {
            gross_employment_income_krw: pay_stage.gross_monthly_pay_krw,
            employee_national_pension_krw: 0,
            employee_health_insurance_krw: 0,
            employee_long_term_care_krw: 0,
            employee_employment_insurance_krw: 0,
            employee_insurance_total_krw: 0,
            withheld_income_tax_krw: 0,
            withheld_local_income_tax_krw: 0,
            net_pay_krw: pay_stage.gross_monthly_pay_krw,
        },
        ("employmentPayrollMinimum" | "basePayOnly", "employmentPayroll") => {
            let dependents = read_tax_dependent_count_in_tx(
                tx,
                context.save_id,
                context.run_revision,
                context.game_day,
            )
            .await?;
            let breakdown = calculate_military_employment_payroll_in_tx(
                tx,
                crate::career::create_payroll_rules().as_ref(),
                MilitaryEmploymentPayrollInput {
                    service_id: service.id,
                    employment_policy_set_id: service.employment_policy_set_id,
                    payday: context.market_date,
                    gross_pay_krw: pay_stage.gross_monthly_pay_krw,
                    dependents,
                },
            )
            .await?;
            EmploymentIncomeAmounts {
                gross_employment_income_krw: breakdown.employment_income_accrual_krw,
                employee_national_pension_krw: breakdown
                    .insurance
                    .national_pension
                    .employee_amount_krw,
                employee_health_insurance_krw: breakdown
                    .insurance
                    .health_insurance
                    .employee_amount_krw,
                employee_long_term_care_krw: breakdown.insurance.long_term_care.employee_amount_krw,
                employee_employment_insurance_krw: breakdown
                    .insurance
                    .employment_insurance
                    .employee_amount_krw,
                employee_insurance_total_krw: breakdown.employee_insurance_total_krw,
                withheld_income_tax_krw: breakdown.withheld_income_tax_krw,
                withheld_local_income_tax_krw: breakdown.withheld_local_income_tax_krw,
                net_pay_krw: breakdown.net_salary_pay_krw,
            }
        }
        _ => bail!("military compensation policy combination is unsupported"),
    };
    let mut postings = Vec::with_capacity(8);
    push_military_pay_posting(
        &mut postings,
        LedgerAccountCode::Wallet,
        amounts.net_pay_krw,
    );
    push_military_pay_posting(
        &mut postings,
        LedgerAccountCode::MilitaryPayIncome,
        amounts
            .gross_employment_income_krw
            .checked_neg()
            .context("military pay ledger overflowed")?,
    );
    push_military_pay_posting(
        &mut postings,
        LedgerAccountCode::EmployeeNationalPensionExpense,
        amounts.employee_national_pension_krw,
    );
    push_military_pay_posting(
        &mut postings,
        LedgerAccountCode::EmployeeHealthInsuranceExpense,
        amounts.employee_health_insurance_krw,
    );
    push_military_pay_posting(
        &mut postings,
        LedgerAccountCode::EmployeeLongTermCareExpense,
        amounts.employee_long_term_care_krw,
    );
    push_military_pay_posting(
        &mut postings,
        LedgerAccountCode::EmployeeEmploymentInsuranceExpense,
        amounts.employee_employment_insurance_krw,
    );
    push_military_pay_posting(
        &mut postings,
        LedgerAccountCode::EmploymentIncomeTaxWithholding,
        amounts.withheld_income_tax_krw,
    );
    push_military_pay_posting(
        &mut postings,
        LedgerAccountCode::EmploymentLocalIncomeTaxWithholding,
        amounts.withheld_local_income_tax_krw,
    );
    let ledger = finance_rules.create_ledger_transaction(LedgerTransactionDraft {
        policy: military_ledger_policy(context),
        source: LedgerSource {
            kind: LedgerSourceKind::MilitaryPay,
            source_id: settlement.id.to_string(),
        },
        game_day: context.game_day,
        description: "군 복무 급여 지급".to_owned(),
        postings,
    })?;
    let ledger_id = write_ledger_transaction(tx, &ledger).await?;
    record_employment_income_event_in_tx(
        tx,
        EmploymentIncomeEventWrite {
            save_id: context.save_id,
            run_revision: context.run_revision,
            employment_policy_set_id: service.employment_policy_set_id,
            source: EmploymentIncomeEventSource::MilitaryPay {
                military_service_id: service.id,
                period_no: payload.period_no,
            },
            scheduled_settlement_id: settlement.id,
            ledger_transaction_id: Some(ledger_id),
            paid_game_day: context.game_day,
            paid_date: context.market_date,
            amounts,
        },
    )
    .await?;
    adjust_military_wallet(tx, context, amounts.net_pay_krw).await?;
    settle_military_settlement(tx, context, Some(ledger_id), None).await
}

fn push_military_pay_posting(
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

async fn load_military_service_for_update(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    service_id: u64,
) -> Result<MilitaryPayServiceRow> {
    sqlx::query_as(
        "SELECT service.id, service.save_id, service.run_revision,
                service.status, service.employment_policy_set_id,
                service.military_option_version_id, service.military_option_policy_id,
                service.service_type, option_policy.service_duration_months,
                service.start_game_day, service.end_game_day, service.start_date,
                service.end_exclusive_date, service.credited_service_days,
                service.last_credited_game_day, option_policy.pay_schedule_kind,
                option_policy.payday_day_of_month, option_policy.partial_month_pay_kind,
                eligibility.minimum_education, eligibility.required_certification_count,
                eligibility.minimum_experience_days, option_row.effort_life_status,
                capacity.effort_units, option_policy.compensation_calculation_kind,
                option_policy.social_insurance_kind
         FROM military_service AS service
         INNER JOIN military_option_policy AS option_policy
           ON option_policy.id = service.military_option_policy_id
          AND option_policy.employment_policy_set_id = service.employment_policy_set_id
          AND option_policy.career_catalog_bundle_id = service.career_catalog_bundle_id
          AND option_policy.military_option_version_id = service.military_option_version_id
         INNER JOIN military_option_version AS option_row
           ON option_row.career_catalog_bundle_id = service.career_catalog_bundle_id
          AND option_row.id = service.military_option_version_id
         INNER JOIN military_option_eligibility_rule AS eligibility
           ON eligibility.career_catalog_bundle_id = service.career_catalog_bundle_id
          AND eligibility.military_option_version_id = service.military_option_version_id
         INNER JOIN career_effort_capacity AS capacity
          ON capacity.career_catalog_bundle_id = service.career_catalog_bundle_id
          AND BINARY capacity.life_status = BINARY option_row.effort_life_status
         WHERE service.save_id = ? AND service.run_revision = ? AND service.id = ?
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(service_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("military service is missing")
}

async fn load_pinned_service_option(
    tx: &mut Transaction<'_, MySql>,
    service: &MilitaryPayServiceRow,
) -> Result<MilitaryOptionPolicy> {
    let pay_stages = sqlx::query_as::<_, MilitaryPayStageRow>(
        "SELECT policy.military_option_version_id,
                stage.start_service_month, stage.end_service_month_exclusive,
                stage.monthly_gross_pay_krw
         FROM military_pay_stage AS stage
         INNER JOIN military_option_policy AS policy
           ON policy.id = stage.military_option_policy_id
         WHERE stage.military_option_policy_id = ? ORDER BY stage.stage_order",
    )
    .bind(service.military_option_policy_id)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|stage| MilitaryPayStagePolicy {
        start_service_month: stage.start_service_month,
        end_exclusive_service_month: stage.end_service_month_exclusive,
        gross_monthly_pay_krw: stage.monthly_gross_pay_krw,
    })
    .collect();
    let experience = sqlx::query_as::<_, MilitaryExperienceRow>(
        "SELECT mapping.military_option_version_id, family.job_family_key,
                mapping.experience_credit_ppm
         FROM military_option_job_family AS mapping
         INNER JOIN career_job_family AS family
           ON family.career_catalog_bundle_id = mapping.career_catalog_bundle_id
          AND family.id = mapping.career_job_family_id
         INNER JOIN military_service AS service
           ON service.career_catalog_bundle_id = mapping.career_catalog_bundle_id
          AND service.military_option_version_id = mapping.military_option_version_id
         WHERE service.id = ? AND service.save_id = ? AND service.run_revision = ?
         ORDER BY family.job_family_key",
    )
    .bind(service.id)
    .bind(service.save_id)
    .bind(service.run_revision)
    .fetch_all(&mut **tx)
    .await?;
    Ok(MilitaryOptionPolicy {
        option_version_id: service.military_option_version_id,
        service_type: enum_from_db(&service.service_type)?,
        service_duration_months: service.service_duration_months,
        pay_schedule_kind: enum_from_db(&service.pay_schedule_kind)?,
        payday_day_of_month: service.payday_day_of_month,
        partial_month_pay_kind: enum_from_db(&service.partial_month_pay_kind)?,
        hard_requirements: MilitaryHardRequirementsState {
            minimum_education: service
                .minimum_education
                .as_deref()
                .map(enum_from_db)
                .transpose()?,
            minimum_certification_count: service.required_certification_count,
            minimum_experience_days: service.minimum_experience_days,
        },
        pay_stages,
        effort_life_status: enum_from_db(&service.effort_life_status)?,
        daily_effort_capacity_units: service.effort_units,
        experience: experience
            .into_iter()
            .map(|credit| MilitaryExperiencePolicy {
                job_family_key: credit.job_family_key,
                daily_credit_ppm: credit.experience_credit_ppm,
            })
            .collect(),
    })
}

async fn settle_military_savings_installment(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    context: MilitarySettlementContext,
    settlement: &MilitarySettlementRow,
) -> Result<()> {
    let payload: MilitarySavingsInstallmentPayload = serde_json::from_str(&settlement.payload_json)
        .context("stored military savings installment payload is invalid")?;
    ensure!(
        payload.version == MILITARY_PAYLOAD_VERSION
            && payload.installment_no > 0
            && settlement.source_kind == "militarySavingsContract"
            && settlement.source_id == payload.contract_id.to_string()
            && settlement.occurrence == payload.installment_no,
        "military savings installment identity is invalid"
    );
    let row: MilitaryInstallmentSettlementRow = sqlx::query_as(
        "SELECT contract.id AS contract_id, contract.status AS contract_status,
                contract.employment_policy_set_id, contract.monthly_contribution_krw,
                installment.id AS installment_id, installment.installment_no,
                installment.due_game_day, installment.status AS installment_status,
                save.cash_krw AS wallet_cash_krw
         FROM military_savings_contract AS contract
         INNER JOIN military_savings_installment AS installment
           ON installment.save_id = contract.save_id
          AND installment.run_revision = contract.run_revision
          AND installment.military_savings_contract_id = contract.id
          AND installment.installment_no = ?
         INNER JOIN save ON save.id = contract.save_id
                        AND save.run_revision = contract.run_revision
         WHERE contract.save_id = ? AND contract.run_revision = ? AND contract.id = ?
         FOR UPDATE",
    )
    .bind(payload.installment_no)
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(payload.contract_id.get())
    .fetch_optional(&mut **tx)
    .await?
    .context("military savings installment is missing")?;
    ensure!(
        row.contract_status == "active"
            && row.installment_status == "scheduled"
            && row.due_game_day == context.game_day
            && row.installment_no as u64 == payload.installment_no,
        "military savings installment is not collectible"
    );
    let plan =
        create_military_rules().settle_savings_installment(MilitarySavingsInstallmentInput {
            installment_no: u32::from(row.installment_no),
            contribution_krw: row.monthly_contribution_krw,
            wallet_cash_krw: row.wallet_cash_krw,
        })?;
    let (ledger_id, no_movement_reason) = match plan.status {
        MilitarySavingsInstallmentStatus::Paid => {
            let policies: Vec<(u64, u32)> = sqlx::query_as(
                "SELECT id, government_match_rate_ppm
                 FROM military_savings_policy
                 WHERE employment_policy_set_id = ? AND ? >= effective_from
                   AND (effective_to_exclusive IS NULL OR ? < effective_to_exclusive)
                 ORDER BY effective_from DESC, id DESC LIMIT 2",
            )
            .bind(row.employment_policy_set_id)
            .bind(context.market_date)
            .bind(context.market_date)
            .fetch_all(&mut **tx)
            .await?;
            ensure!(
                policies.len() == 1,
                "military savings match policy is ambiguous"
            );
            let (matching_policy_id, matching_rate_ppm) = policies[0];
            let government_match_krw = i64::try_from(
                i128::from(plan.principal_delta_krw)
                    .checked_mul(i128::from(matching_rate_ppm))
                    .context("military savings government match overflowed")?
                    / 1_000_000,
            )
            .context("military savings government match exceeds i64")?;
            let ledger = finance_rules.create_military_savings_ledger_transaction(
                LedgerTransactionDraft {
                    policy: military_ledger_policy(context),
                    source: LedgerSource {
                        kind: LedgerSourceKind::MilitarySavingsInstallment,
                        source_id: settlement.id.to_string(),
                    },
                    game_day: context.game_day,
                    description: "장병내일준비적금 납입".to_owned(),
                    postings: vec![
                        LedgerPosting {
                            account_code: LedgerAccountCode::Wallet,
                            financial_account_id: None,
                            amount_krw: plan.wallet_cash_delta_krw,
                        },
                        LedgerPosting {
                            account_code: LedgerAccountCode::MilitarySavingsPrincipal,
                            financial_account_id: None,
                            amount_krw: plan.principal_delta_krw,
                        },
                    ],
                },
                ResourceId::from_u64(row.contract_id),
            )?;
            let ledger_id = write_ledger_transaction(tx, &ledger).await?;
            let update = sqlx::query(
                "UPDATE military_savings_installment
                 SET status = 'paid', paid_principal_krw = ?, paid_game_day = ?,
                     no_movement_reason = NULL, matching_policy_id = ?,
                     matching_rate_ppm = ?, government_match_krw = ?,
                     ledger_transaction_id = ?
                 WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'scheduled'",
            )
            .bind(plan.principal_delta_krw)
            .bind(context.game_day)
            .bind(matching_policy_id)
            .bind(matching_rate_ppm)
            .bind(government_match_krw)
            .bind(ledger_id)
            .bind(context.save_id)
            .bind(context.run_revision)
            .bind(row.installment_id)
            .execute(&mut **tx)
            .await?;
            ensure!(update.rows_affected() == 1, "military installment changed");
            (Some(ledger_id), None)
        }
        MilitarySavingsInstallmentStatus::Missed => {
            let update = sqlx::query(
                "UPDATE military_savings_installment
                 SET status = 'missed', paid_principal_krw = 0, paid_game_day = ?,
                     no_movement_reason = 'insufficientWalletCash'
                 WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'scheduled'",
            )
            .bind(context.game_day)
            .bind(context.save_id)
            .bind(context.run_revision)
            .bind(row.installment_id)
            .execute(&mut **tx)
            .await?;
            ensure!(update.rows_affected() == 1, "military installment changed");
            (None, Some("insufficientWalletCash"))
        }
    };
    refresh_military_savings_contract_aggregates(tx, context, row.contract_id).await?;
    adjust_military_wallet(tx, context, plan.wallet_cash_delta_krw).await?;
    settle_military_settlement(tx, context, ledger_id, no_movement_reason).await
}

async fn settle_military_savings_maturity(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    context: MilitarySettlementContext,
    settlement: &MilitarySettlementRow,
) -> Result<()> {
    let payload: MilitarySavingsMaturityPayload = serde_json::from_str(&settlement.payload_json)
        .context("stored military savings maturity payload is invalid")?;
    let row: MilitaryMaturitySettlementRow = sqlx::query_as(
        "SELECT contract.id AS contract_id, contract.status AS contract_status,
                contract.term_months, contract.maturity_game_day,
                DATE_ADD(world.start_date, INTERVAL contract.maturity_game_day DAY)
                    AS maturity_date,
                contract.fixed_rate_bp, product.day_count_denominator,
                product.interest_rounding_unit_krw,
                savings_policy.government_match_next_month_day,
                service.status AS service_status
         FROM military_savings_contract AS contract
         INNER JOIN military_service AS service
           ON service.save_id = contract.save_id
          AND service.run_revision = contract.run_revision
          AND service.id = contract.military_service_id
         INNER JOIN military_savings_product_version AS product
           ON product.career_catalog_bundle_id = contract.career_catalog_bundle_id
          AND product.id = contract.military_savings_product_id
         INNER JOIN military_savings_policy AS savings_policy
           ON savings_policy.employment_policy_set_id = contract.employment_policy_set_id
          AND savings_policy.id = contract.military_savings_policy_id
         INNER JOIN save ON save.id = contract.save_id
                        AND save.run_revision = contract.run_revision
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         WHERE contract.save_id = ? AND contract.run_revision = ? AND contract.id = ?
         FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(payload.contract_id.get())
    .fetch_optional(&mut **tx)
    .await?
    .context("military savings maturity contract is missing")?;
    ensure!(
        payload.version == MILITARY_PAYLOAD_VERSION
            && settlement.source_kind == "militarySavingsContract"
            && settlement.source_id == payload.contract_id.to_string()
            && settlement.occurrence == u64::from(row.term_months) + 1
            && row.contract_status == "active"
            && row.service_status == "completed"
            && row.maturity_game_day == context.game_day
            && row.maturity_date == context.market_date,
        "military savings maturity identity is invalid"
    );
    let paid_rows: Vec<(u64, u16, Date, i64, Option<u32>)> = sqlx::query_as(
        "SELECT installment.id, installment.installment_no,
                DATE_ADD(world.start_date, INTERVAL installment.paid_game_day DAY),
                installment.paid_principal_krw, installment.matching_rate_ppm
         FROM military_savings_installment AS installment
         INNER JOIN save ON save.id = installment.save_id
                        AND save.run_revision = installment.run_revision
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         WHERE installment.save_id = ? AND installment.run_revision = ?
           AND installment.military_savings_contract_id = ?
           AND installment.status = 'paid'
         ORDER BY installment.installment_no FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(row.contract_id)
    .fetch_all(&mut **tx)
    .await?;
    let paid = paid_rows
        .iter()
        .map(
            |(_, installment_no, paid_date, principal_krw, matching_rate)| {
                Ok(PaidMilitarySavingsInstallment {
                    installment_no: u32::from(*installment_no),
                    paid_date: *paid_date,
                    principal_krw: *principal_krw,
                    government_matching_rate_ppm: i64::from(
                        matching_rate.context("paid military installment has no matching rate")?,
                    ),
                })
            },
        )
        .collect::<Result<Vec<_>>>()?;
    let plan = create_military_rules().plan_savings_maturity(MilitarySavingsMaturityInput {
        maturity_date: row.maturity_date,
        service_completion_confirmed: true,
        annual_interest_rate_ppm: i64::from(row.fixed_rate_bp) * 100,
        day_count_denominator: row.day_count_denominator,
        interest_rounding_unit_krw: row.interest_rounding_unit_krw,
        government_match_payment_day_of_month: row.government_match_next_month_day,
        paid_installments: &paid,
    })?;
    let ledger_id = if plan.wallet_credit_krw == 0 {
        None
    } else {
        let mut postings = vec![LedgerPosting {
            account_code: LedgerAccountCode::Wallet,
            financial_account_id: None,
            amount_krw: plan.wallet_credit_krw,
        }];
        if plan.principal_krw > 0 {
            postings.push(LedgerPosting {
                account_code: LedgerAccountCode::MilitarySavingsPrincipal,
                financial_account_id: None,
                amount_krw: -plan.principal_krw,
            });
        }
        if plan.gross_bank_interest_krw > 0 {
            postings.push(LedgerPosting {
                account_code: LedgerAccountCode::MilitarySavingsBankInterest,
                financial_account_id: None,
                amount_krw: -plan.gross_bank_interest_krw,
            });
        }
        let ledger = finance_rules.create_military_savings_ledger_transaction(
            LedgerTransactionDraft {
                policy: military_ledger_policy(context),
                source: LedgerSource {
                    kind: LedgerSourceKind::MilitarySavingsMaturity,
                    source_id: settlement.id.to_string(),
                },
                game_day: context.game_day,
                description: "장병내일준비적금 만기해지".to_owned(),
                postings,
            },
            ResourceId::from_u64(row.contract_id),
        )?;
        Some(write_ledger_transaction(tx, &ledger).await?)
    };
    let update = sqlx::query(
        "UPDATE military_savings_contract
         SET status = 'matured', bank_interest_krw = ?,
             maturity_ledger_transaction_id = ?, closed_game_day = ?,
             closure_kind = 'maturity'
         WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'active'",
    )
    .bind(plan.gross_bank_interest_krw)
    .bind(ledger_id)
    .bind(context.game_day)
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(row.contract_id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "military savings maturity changed"
    );
    let due_days = (plan.government_match.due_date - context.market_date).whole_days();
    ensure!(
        due_days > 0,
        "military savings government match is not future-dated"
    );
    let government_match_game_day = context
        .game_day
        .checked_add(u32::try_from(due_days).context("government match date is too far")?)
        .context("government match game day overflowed")?;
    for line in &plan.government_match.installments {
        let (installment_id, _, _, _, _) = paid_rows
            .iter()
            .find(|(_, installment_no, _, _, _)| u32::from(*installment_no) == line.installment_no)
            .context("maturity plan references an unknown military installment")?;
        let payload_json = serde_json::json!({
            "version": MILITARY_PAYLOAD_VERSION,
            "contractId": row.contract_id.to_string(),
            "installmentNo": line.installment_no,
        });
        let scheduled = sqlx::query(
            "INSERT INTO scheduled_settlement
                 (save_id, run_revision, due_game_day, kind, payload,
                  source_kind, source_id, occurrence, status)
             VALUES (?, ?, ?, 'militarySavingsGovernmentMatch', CAST(? AS JSON),
                     'militarySavingsInstallment', ?, 1, 'pending')",
        )
        .bind(context.save_id)
        .bind(context.run_revision)
        .bind(government_match_game_day)
        .bind(payload_json.to_string())
        .bind(installment_id.to_string())
        .execute(&mut **tx)
        .await?;
        let installment_update = sqlx::query(
            "UPDATE military_savings_installment
             SET government_match_settlement_id = ?
             WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'paid'
               AND government_match_settlement_id IS NULL",
        )
        .bind(scheduled.last_insert_id())
        .bind(context.save_id)
        .bind(context.run_revision)
        .bind(*installment_id)
        .execute(&mut **tx)
        .await?;
        ensure!(
            installment_update.rows_affected() == 1,
            "military savings match schedule changed"
        );
    }
    adjust_military_wallet(tx, context, plan.wallet_credit_krw).await?;
    settle_military_settlement(
        tx,
        context,
        ledger_id,
        (plan.wallet_credit_krw == 0).then_some("zeroPayout"),
    )
    .await
}

async fn settle_military_savings_government_match(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    context: MilitarySettlementContext,
    settlement: &MilitarySettlementRow,
) -> Result<()> {
    let payload: MilitarySavingsGovernmentMatchPayload =
        serde_json::from_str(&settlement.payload_json)
            .context("stored military savings government match payload is invalid")?;
    let installment_id = settlement
        .source_id
        .parse::<u64>()
        .context("military savings match source ID is invalid")?;
    let row: MilitaryGovernmentMatchSettlementRow = sqlx::query_as(
        "SELECT installment.id AS installment_id,
                installment.military_savings_contract_id AS contract_id,
                installment.installment_no, installment.status AS installment_status,
                installment.government_match_krw,
                installment.government_match_settlement_id,
                contract.status AS contract_status
         FROM military_savings_installment AS installment
         INNER JOIN military_savings_contract AS contract
           ON contract.save_id = installment.save_id
          AND contract.run_revision = installment.run_revision
          AND contract.id = installment.military_savings_contract_id
         WHERE installment.save_id = ? AND installment.run_revision = ?
           AND installment.id = ? FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(installment_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("military savings match installment is missing")?;
    ensure!(
        payload.version == MILITARY_PAYLOAD_VERSION
            && payload.contract_id.get() == row.contract_id
            && payload.installment_no == u64::from(row.installment_no)
            && settlement.source_kind == "militarySavingsInstallment"
            && settlement.occurrence == 1
            && row.installment_id == installment_id
            && row.installment_status == "paid"
            && row.contract_status == "matured"
            && row.government_match_settlement_id == Some(settlement.id)
            && row.government_match_krw > 0,
        "military savings government match identity is invalid"
    );
    let ledger = finance_rules.create_military_savings_ledger_transaction(
        LedgerTransactionDraft {
            policy: military_ledger_policy(context),
            source: LedgerSource {
                kind: LedgerSourceKind::MilitarySavingsGovernmentMatch,
                source_id: settlement.id.to_string(),
            },
            game_day: context.game_day,
            description: "장병내일준비적금 정부매칭 지급".to_owned(),
            postings: vec![
                LedgerPosting {
                    account_code: LedgerAccountCode::Wallet,
                    financial_account_id: None,
                    amount_krw: row.government_match_krw,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::MilitarySavingsGovernmentMatchIncome,
                    financial_account_id: None,
                    amount_krw: -row.government_match_krw,
                },
            ],
        },
        ResourceId::from_u64(row.contract_id),
    )?;
    let ledger_id = write_ledger_transaction(tx, &ledger).await?;
    let update = sqlx::query(
        "UPDATE military_savings_installment
         SET government_match_ledger_transaction_id = ?,
             government_match_paid_game_day = ?
         WHERE save_id = ? AND run_revision = ? AND id = ?
           AND government_match_ledger_transaction_id IS NULL",
    )
    .bind(ledger_id)
    .bind(context.game_day)
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(row.installment_id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "military savings match changed"
    );
    refresh_military_savings_contract_aggregates(tx, context, row.contract_id).await?;
    adjust_military_wallet(tx, context, row.government_match_krw).await?;
    settle_military_settlement(tx, context, Some(ledger_id), None).await
}

async fn refresh_military_savings_contract_aggregates(
    tx: &mut Transaction<'_, MySql>,
    context: MilitarySettlementContext,
    contract_id: u64,
) -> Result<()> {
    let update = sqlx::query(
        "UPDATE military_savings_contract AS contract
         SET contract.principal_krw = (
                 SELECT COALESCE(SUM(installment.paid_principal_krw), 0)
                 FROM military_savings_installment AS installment
                 WHERE installment.save_id = contract.save_id
                   AND installment.run_revision = contract.run_revision
                   AND installment.military_savings_contract_id = contract.id),
             contract.paid_installment_count = (
                 SELECT COUNT(*) FROM military_savings_installment AS installment
                 WHERE installment.save_id = contract.save_id
                   AND installment.run_revision = contract.run_revision
                   AND installment.military_savings_contract_id = contract.id
                   AND installment.status = 'paid'),
             contract.missed_installment_count = (
                 SELECT COUNT(*) FROM military_savings_installment AS installment
                 WHERE installment.save_id = contract.save_id
                   AND installment.run_revision = contract.run_revision
                   AND installment.military_savings_contract_id = contract.id
                   AND installment.status = 'missed'),
             contract.government_match_entitlement_krw = (
                 SELECT COALESCE(SUM(installment.government_match_krw), 0)
                 FROM military_savings_installment AS installment
                 WHERE installment.save_id = contract.save_id
                   AND installment.run_revision = contract.run_revision
                   AND installment.military_savings_contract_id = contract.id
                   AND installment.status = 'paid'),
             contract.government_match_received_krw = (
                 SELECT COALESCE(SUM(installment.government_match_krw), 0)
                 FROM military_savings_installment AS installment
                 WHERE installment.save_id = contract.save_id
                   AND installment.run_revision = contract.run_revision
                   AND installment.military_savings_contract_id = contract.id
                   AND installment.government_match_ledger_transaction_id IS NOT NULL)
         WHERE contract.save_id = ? AND contract.run_revision = ? AND contract.id = ?",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(contract_id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "military savings contract is missing"
    );
    Ok(())
}

async fn adjust_military_wallet(
    tx: &mut Transaction<'_, MySql>,
    context: MilitarySettlementContext,
    delta_krw: i64,
) -> Result<()> {
    if delta_krw == 0 {
        return Ok(());
    }
    let cash: i64 = sqlx::query_scalar(
        "SELECT cash_krw FROM save
         WHERE id = ? AND run_revision = ? AND policy_set_id = ? FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(context.finance_policy_set_id)
    .fetch_one(&mut **tx)
    .await?;
    let next_cash = cash
        .checked_add(delta_krw)
        .context("military settlement wallet overflowed")?;
    ensure!(next_cash >= 0, "military settlement wallet became negative");
    let update = sqlx::query(
        "UPDATE save SET cash_krw = ?
         WHERE id = ? AND run_revision = ? AND policy_set_id = ? AND cash_krw = ?",
    )
    .bind(next_cash)
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(context.finance_policy_set_id)
    .bind(cash)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "military settlement wallet changed"
    );
    Ok(())
}

async fn settle_military_settlement(
    tx: &mut Transaction<'_, MySql>,
    context: MilitarySettlementContext,
    ledger_id: Option<u64>,
    no_movement_reason: Option<&str>,
) -> Result<()> {
    let update = match (ledger_id, no_movement_reason) {
        (Some(ledger_id), None) => {
            sqlx::query(
                "UPDATE scheduled_settlement
             SET status = 'settled', outcome = 'applied',
                 settled_ledger_transaction_id = ?
             WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'pending'",
            )
            .bind(ledger_id)
            .bind(context.save_id)
            .bind(context.run_revision)
            .bind(context.settlement_id)
            .execute(&mut **tx)
            .await?
        }
        (None, Some(reason)) => {
            sqlx::query(
                "UPDATE scheduled_settlement
             SET status = 'settled', outcome = 'noMovement', outcome_reason = ?
             WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'pending'",
            )
            .bind(reason)
            .bind(context.save_id)
            .bind(context.run_revision)
            .bind(context.settlement_id)
            .execute(&mut **tx)
            .await?
        }
        _ => bail!("military settlement has an invalid outcome shape"),
    };
    ensure!(update.rows_affected() == 1, "military settlement changed");
    Ok(())
}

fn military_ledger_policy(context: MilitarySettlementContext) -> RunPolicyContext {
    RunPolicyContext {
        run: RunId {
            save_id: ResourceId::from_u64(context.save_id),
            run_revision: context.run_revision,
        },
        policy_set_id: ResourceId::from_u64(context.finance_policy_set_id),
    }
}

pub(super) async fn read_military_snapshot_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<(
    CareerMilitaryStatus,
    Option<ActiveMilitaryServiceState>,
    Vec<ActiveMilitarySavingsState>,
    Vec<CareerPendingScheduleItemState>,
)> {
    let military_status: String = sqlx::query_scalar(
        "SELECT military_status FROM career_run WHERE save_id = ? AND run_revision = ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await?;
    let service = read_service_row_in_tx(tx, save_id, run_revision).await?;
    let active_service = service
        .filter(|row| row.status == "pendingStart" || row.status == "serving")
        .map(active_service_from_row)
        .transpose()?;
    let active_savings = read_active_savings_in_tx(tx, save_id, run_revision).await?;
    let schedule = read_pending_schedule_in_tx(tx, save_id, run_revision).await?;
    Ok((
        enum_from_db(&military_status)?,
        active_service,
        active_savings,
        schedule,
    ))
}

async fn read_scope_for_user(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
) -> Result<MilitaryReadScopeRow> {
    sqlx::query_as(
        "SELECT save.id AS save_id, save.run_revision, save.game_day,
                DATE_ADD(world.start_date, INTERVAL save.game_day DAY) AS market_date,
                career_run.career_catalog_bundle_id, career_run.employment_policy_set_id,
                career_run.military_status, `character`.education,
                CAST((
                    SELECT COUNT(*) FROM spec_evidence AS certification
                    WHERE certification.save_id = save.id
                      AND certification.run_revision = save.run_revision
                      AND certification.kind = 'certification'
                      AND (certification.expires_on_game_day IS NULL
                           OR certification.expires_on_game_day >= save.game_day)
                ) AS UNSIGNED) AS certifications,
                CAST(COALESCE((
                    SELECT SUM(evidence.credited_experience_days)
                    FROM spec_evidence AS evidence
                    WHERE evidence.save_id = save.id
                      AND evidence.run_revision = save.run_revision
                      AND evidence.kind = 'experience'
                ), 0) AS UNSIGNED) AS experience_days
         FROM save
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         INNER JOIN career_run
           ON career_run.save_id = save.id AND career_run.run_revision = save.run_revision
         INNER JOIN `character` ON `character`.save_id = save.id
         WHERE save.user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("military state requires an active character")
}

async fn read_scope_for_save(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<MilitaryReadScopeRow> {
    sqlx::query_as(
        "SELECT save.id AS save_id, save.run_revision, save.game_day,
                DATE_ADD(world.start_date, INTERVAL save.game_day DAY) AS market_date,
                career_run.career_catalog_bundle_id, career_run.employment_policy_set_id,
                career_run.military_status, `character`.education,
                CAST((
                    SELECT COUNT(*) FROM spec_evidence AS certification
                    WHERE certification.save_id = save.id
                      AND certification.run_revision = save.run_revision
                      AND certification.kind = 'certification'
                      AND (certification.expires_on_game_day IS NULL
                           OR certification.expires_on_game_day >= save.game_day)
                ) AS UNSIGNED) AS certifications,
                CAST(COALESCE((
                    SELECT SUM(evidence.credited_experience_days)
                    FROM spec_evidence AS evidence
                    WHERE evidence.save_id = save.id
                      AND evidence.run_revision = save.run_revision
                      AND evidence.kind = 'experience'
                ), 0) AS UNSIGNED) AS experience_days
         FROM save
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         INNER JOIN career_run
           ON career_run.save_id = save.id AND career_run.run_revision = save.run_revision
         INNER JOIN `character` ON `character`.save_id = save.id
         WHERE save.id = ? AND save.run_revision = ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await?
    .context("military bridge requires an active character")
}

async fn read_options_in_tx(
    tx: &mut Transaction<'_, MySql>,
    scope: &MilitaryReadScopeRow,
) -> Result<MilitaryOptionsState> {
    let rows: Vec<MilitaryOptionRow> = sqlx::query_as(
        "SELECT option_row.id, option_row.option_key, option_row.service_type,
                option_row.display_name, option_row.effort_life_status,
                option_row.compensation_kind, option_row.pay_schedule,
                option_row.grants_career_experience, eligibility.minimum_education,
                eligibility.required_certification_count, eligibility.minimum_experience_days,
                option_policy.id AS policy_id, option_policy.service_duration_months,
                option_policy.availability_status, capacity.effort_units
         FROM military_option_version AS option_row
         INNER JOIN military_option_eligibility_rule AS eligibility
           ON eligibility.career_catalog_bundle_id = option_row.career_catalog_bundle_id
          AND eligibility.military_option_version_id = option_row.id
         INNER JOIN career_effort_capacity AS capacity
           ON capacity.career_catalog_bundle_id = option_row.career_catalog_bundle_id
          AND BINARY capacity.life_status = BINARY option_row.effort_life_status
         LEFT JOIN military_option_policy AS option_policy
           ON option_policy.employment_policy_set_id = ?
          AND option_policy.career_catalog_bundle_id = option_row.career_catalog_bundle_id
          AND option_policy.military_option_version_id = option_row.id
          AND ? >= option_policy.effective_from
          AND (option_policy.effective_to_exclusive IS NULL
               OR ? < option_policy.effective_to_exclusive)
         WHERE option_row.career_catalog_bundle_id = ?
         ORDER BY option_row.id",
    )
    .bind(scope.employment_policy_set_id)
    .bind(scope.market_date)
    .bind(scope.market_date)
    .bind(scope.career_catalog_bundle_id)
    .fetch_all(&mut **tx)
    .await?;
    let stage_rows: Vec<MilitaryPayStageRow> = sqlx::query_as(
        "SELECT policy.military_option_version_id,
                stage.start_service_month, stage.end_service_month_exclusive,
                stage.monthly_gross_pay_krw
         FROM military_pay_stage AS stage
         INNER JOIN military_option_policy AS policy ON policy.id = stage.military_option_policy_id
         WHERE policy.employment_policy_set_id = ?
           AND policy.career_catalog_bundle_id = ?
           AND ? >= policy.effective_from
           AND (policy.effective_to_exclusive IS NULL OR ? < policy.effective_to_exclusive)
         ORDER BY policy.military_option_version_id, stage.stage_order",
    )
    .bind(scope.employment_policy_set_id)
    .bind(scope.career_catalog_bundle_id)
    .bind(scope.market_date)
    .bind(scope.market_date)
    .fetch_all(&mut **tx)
    .await?;
    let experience_rows: Vec<MilitaryExperienceRow> = sqlx::query_as(
        "SELECT mapping.military_option_version_id, family.job_family_key,
                mapping.experience_credit_ppm
         FROM military_option_job_family AS mapping
         INNER JOIN career_job_family AS family
           ON family.career_catalog_bundle_id = mapping.career_catalog_bundle_id
          AND family.id = mapping.career_job_family_id
         WHERE mapping.career_catalog_bundle_id = ?
         ORDER BY mapping.military_option_version_id, family.job_family_key",
    )
    .bind(scope.career_catalog_bundle_id)
    .fetch_all(&mut **tx)
    .await?;

    let current_education: Education = enum_from_db(&scope.education)?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let service_duration_months = row
            .service_duration_months
            .context("published military option has no effective policy")?;
        let availability_status = row
            .availability_status
            .as_deref()
            .context("published military option has no availability status")?;
        let minimum_education = row
            .minimum_education
            .as_deref()
            .map(enum_from_db)
            .transpose()?;
        let mut reasons = Vec::new();
        if scope.military_status != "unserved" {
            reasons.push(MilitaryOptionIneligibilityReason::MilitaryStateConflict);
        }
        if minimum_education.is_some_and(|minimum| current_education < minimum) {
            reasons.push(MilitaryOptionIneligibilityReason::MinimumEducation);
        }
        if scope.certifications < row.required_certification_count {
            reasons.push(MilitaryOptionIneligibilityReason::MinimumCertificationCount);
        }
        if scope.experience_days < row.minimum_experience_days {
            reasons.push(MilitaryOptionIneligibilityReason::MinimumExperienceDays);
        }
        if availability_status != "available" {
            reasons.push(MilitaryOptionIneligibilityReason::PolicyUnavailable);
        }
        let pay_stages = stage_rows
            .iter()
            .filter(|stage| stage.military_option_version_id == row.id)
            .map(|stage| MilitaryPayStageState {
                start_service_month: stage.start_service_month,
                end_exclusive_service_month: stage.end_service_month_exclusive,
                gross_monthly_pay_krw: stage.monthly_gross_pay_krw,
            })
            .collect::<Vec<_>>();
        let experience_credits = experience_rows
            .iter()
            .filter(|credit| credit.military_option_version_id == row.id)
            .map(|credit| MilitaryExperienceCreditState {
                job_family_key: credit.job_family_key.clone(),
                daily_credit_ppm: credit.experience_credit_ppm,
            })
            .collect::<Vec<_>>();
        if row.policy_id.is_some() && pay_stages.is_empty() {
            reasons.push(MilitaryOptionIneligibilityReason::PolicyUnavailable);
        }
        items.push(MilitaryOptionState {
            id: ResourceId::from_u64(row.id),
            option_key: row.option_key,
            service_type: enum_from_db(&row.service_type)?,
            display_name: row.display_name,
            eligible: reasons.is_empty(),
            ineligibility_reasons: reasons,
            service_duration_months,
            hard_requirements: MilitaryHardRequirementsState {
                minimum_education,
                minimum_certification_count: row.required_certification_count,
                minimum_experience_days: row.minimum_experience_days,
            },
            compensation_kind: enum_from_db(&row.compensation_kind)?,
            pay_schedule: enum_from_db(&row.pay_schedule)?,
            pay_stages,
            effort_life_status: enum_from_db(&row.effort_life_status)?,
            daily_effort_capacity_units: row.effort_units,
            grants_career_experience: row.grants_career_experience,
            experience_credits,
        });
    }
    Ok(MilitaryOptionsState { items })
}

async fn read_service_row_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<Option<MilitaryServiceRow>> {
    sqlx::query_as(
        "SELECT service.id, service.military_option_version_id, service.service_type,
                option_row.display_name, service.status, service.source_kind,
                service.start_game_day, service.end_game_day, service.start_date,
                service.end_exclusive_date, service.credited_service_days,
                service.completed_game_day, option_row.effort_life_status,
                option_row.grants_career_experience,
                (SELECT MIN(settlement.due_game_day)
                 FROM scheduled_settlement AS settlement
                 WHERE settlement.save_id = service.save_id
                   AND settlement.run_revision = service.run_revision
                   AND settlement.kind = 'militaryPay'
                   AND settlement.source_kind = 'militaryService'
                   AND BINARY settlement.source_id = BINARY CAST(service.id AS CHAR)
                   AND settlement.status = 'pending') AS next_pay_game_day
         FROM military_service AS service
         INNER JOIN military_option_version AS option_row
           ON option_row.career_catalog_bundle_id = service.career_catalog_bundle_id
          AND option_row.id = service.military_option_version_id
         WHERE service.save_id = ? AND service.run_revision = ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read military service")
}

async fn read_service_history_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<Option<MilitaryServiceHistoryState>> {
    read_service_row_in_tx(tx, save_id, run_revision)
        .await?
        .map(service_history_from_row)
        .transpose()
}

fn active_service_from_row(row: MilitaryServiceRow) -> Result<ActiveMilitaryServiceState> {
    let total_service_days = row
        .end_game_day
        .checked_sub(row.start_game_day)
        .context("military service period is inverted")?;
    Ok(ActiveMilitaryServiceState {
        id: ResourceId::from_u64(row.id),
        option_version_id: ResourceId::from_u64(row.military_option_version_id),
        service_type: enum_from_db(&row.service_type)?,
        display_name: row.display_name,
        status: enum_from_db(&row.status)?,
        start_game_day: row.start_game_day,
        end_game_day: row.end_game_day,
        credited_service_days: row.credited_service_days,
        total_service_days,
        effort_life_status: enum_from_db(&row.effort_life_status)?,
        grants_career_experience: row.grants_career_experience,
        next_pay_game_day: row.next_pay_game_day,
    })
}

fn service_history_from_row(row: MilitaryServiceRow) -> Result<MilitaryServiceHistoryState> {
    let total_service_days = row
        .end_game_day
        .checked_sub(row.start_game_day)
        .context("military service period is inverted")?;
    Ok(MilitaryServiceHistoryState {
        id: ResourceId::from_u64(row.id),
        option_version_id: ResourceId::from_u64(row.military_option_version_id),
        service_type: enum_from_db(&row.service_type)?,
        display_name: row.display_name,
        status: enum_from_db(&row.status)?,
        source_kind: enum_from_db(&row.source_kind)?,
        start_game_day: row.start_game_day,
        end_game_day: row.end_game_day,
        credited_service_days: row.credited_service_days,
        total_service_days,
        effort_life_status: enum_from_db(&row.effort_life_status)?,
        grants_career_experience: row.grants_career_experience,
        next_pay_game_day: row.next_pay_game_day,
        start_date: row.start_date.to_string(),
        end_exclusive_date: row.end_exclusive_date.to_string(),
        completed_game_day: row.completed_game_day,
    })
}

async fn read_active_savings_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<Vec<ActiveMilitarySavingsState>> {
    let rows: Vec<ActiveSavingsRow> = sqlx::query_as(
        "SELECT contract.id, contract.military_savings_product_id AS product_version_id,
                institution.institution_key, contract.status,
                contract.monthly_contribution_krw, contract.debit_day_of_month,
                contract.principal_krw, contract.paid_installment_count,
                contract.missed_installment_count, contract.maturity_game_day,
                (SELECT MIN(installment.due_game_day)
                 FROM military_savings_installment AS installment
                 WHERE installment.military_savings_contract_id = contract.id
                   AND installment.status = 'scheduled') AS next_installment_game_day
         FROM military_savings_contract AS contract
         INNER JOIN military_savings_institution_catalog AS catalog
           ON catalog.career_catalog_bundle_id = contract.career_catalog_bundle_id
          AND catalog.id = contract.military_savings_institution_id
         INNER JOIN financial_institution AS institution
           ON institution.id = catalog.financial_institution_id
         WHERE contract.save_id = ? AND contract.run_revision = ?
           AND contract.status = 'active'
         ORDER BY contract.id",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(ActiveMilitarySavingsState {
                id: ResourceId::from_u64(row.id),
                product_version_id: ResourceId::from_u64(row.product_version_id),
                institution_key: row.institution_key,
                status: enum_from_db(&row.status)?,
                monthly_contribution_krw: row.monthly_contribution_krw,
                debit_day_of_month: row.debit_day_of_month,
                principal_krw: row.principal_krw,
                paid_installment_count: row.paid_installment_count,
                missed_installment_count: row.missed_installment_count,
                next_installment_game_day: row.next_installment_game_day,
                maturity_game_day: row.maturity_game_day,
            })
        })
        .collect()
}

async fn read_pending_schedule_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<Vec<CareerPendingScheduleItemState>> {
    let rows: Vec<PendingScheduleRow> = sqlx::query_as(
        "SELECT schedule.source_kind, schedule.id, schedule.due_game_day, schedule.kind
         FROM (
             SELECT 'careerAction' AS source_kind, action.id, action.due_game_day,
                    action.action_kind AS kind, 0 AS source_rank, action.phase_rank
             FROM career_scheduled_action AS action
             WHERE action.save_id = ? AND action.run_revision = ? AND action.status = 'pending'
               AND action.action_kind IN (
                   'employmentStart', 'militaryServiceStart', 'militaryServiceCompletion',
                   'documentReview', 'confirmationExpiry', 'interviewDecision',
                   'offerExpiry', 'invitationGeneration'
               )
             UNION ALL
             SELECT 'settlement', settlement.id, settlement.due_game_day,
                    settlement.kind, 1, 0
             FROM scheduled_settlement AS settlement
             WHERE settlement.save_id = ? AND settlement.run_revision = ?
               AND settlement.status = 'pending'
               AND settlement.kind IN (
                   'employmentPayroll', 'employmentReconciliation', 'militaryPay',
                   'militarySavingsInstallment', 'militarySavingsMaturity',
                   'militarySavingsGovernmentMatch'
               )
         ) AS schedule
         ORDER BY schedule.due_game_day, schedule.source_rank, schedule.phase_rank, schedule.id
         LIMIT ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(save_id)
    .bind(run_revision)
    .bind(SNAPSHOT_SCHEDULE_LIMIT)
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter()
        .map(|row| match row.source_kind.as_str() {
            "careerAction" => Ok(CareerPendingScheduleItemState::CareerAction {
                id: ResourceId::from_u64(row.id),
                due_game_day: row.due_game_day,
                kind: enum_from_db(&row.kind)?,
            }),
            "settlement" => Ok(CareerPendingScheduleItemState::Settlement {
                id: ResourceId::from_u64(row.id),
                due_game_day: row.due_game_day,
                kind: enum_from_db(&row.kind)?,
            }),
            _ => bail!("stored career schedule source is invalid"),
        })
        .collect()
}

async fn read_savings_products_in_tx(
    tx: &mut Transaction<'_, MySql>,
    scope: &MilitaryReadScopeRow,
) -> Result<MilitarySavingsProductsState> {
    let rows: Vec<SavingsProductRow> = sqlx::query_as(
        "SELECT product.id, product.product_key, institution.institution_key,
                institution.display_name AS institution_display_name,
                product.available_from, product.available_to_exclusive,
                product.day_count_denominator, product.interest_rounding_kind,
                product.interest_rounding_unit_krw, product.early_termination_rate_bp,
                savings_policy.id AS policy_id,
                savings_policy.effective_from AS policy_effective_from,
                savings_policy.effective_to_exclusive AS policy_effective_to_exclusive,
                savings_policy.join_through, savings_policy.minimum_remaining_service_months,
                savings_policy.max_contracts_per_service,
                savings_policy.max_contracts_per_institution,
                savings_policy.institution_monthly_limit_krw,
                savings_policy.person_monthly_limit_krw,
                savings_policy.limit_setting_unit_krw,
                savings_policy.minimum_installment_krw,
                savings_policy.installment_unit_krw,
                savings_policy.government_match_rate_ppm,
                savings_policy.government_match_next_month_day,
                savings_policy.tax_exempt,
                service.service_type AS active_service_type,
                service.end_exclusive_date AS service_end_exclusive_date,
                (SELECT COUNT(*) FROM military_savings_contract AS active_contract
                 WHERE active_contract.save_id = ? AND active_contract.run_revision = ?
                   AND active_contract.status = 'active') AS active_contract_count,
                (SELECT COUNT(*) FROM military_savings_contract AS institution_contract
                 WHERE institution_contract.military_service_id = service.id
                   AND institution_contract.military_savings_institution_id = catalog.id)
                    AS institution_contract_count
         FROM military_savings_product_version AS product
         INNER JOIN military_savings_institution_catalog AS catalog
           ON catalog.career_catalog_bundle_id = product.career_catalog_bundle_id
          AND catalog.id = product.military_savings_institution_id
         INNER JOIN financial_institution AS institution
           ON institution.id = catalog.financial_institution_id
         LEFT JOIN military_savings_policy AS savings_policy
           ON savings_policy.employment_policy_set_id = ?
          AND ? >= savings_policy.effective_from
          AND (savings_policy.effective_to_exclusive IS NULL
               OR ? < savings_policy.effective_to_exclusive)
         LEFT JOIN military_service AS service
           ON service.save_id = ? AND service.run_revision = ? AND service.status = 'serving'
         WHERE product.career_catalog_bundle_id = ?
         ORDER BY product.id",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.employment_policy_set_id)
    .bind(scope.market_date)
    .bind(scope.market_date)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.career_catalog_bundle_id)
    .fetch_all(&mut **tx)
    .await?;
    let rates: Vec<SavingsRateRow> = sqlx::query_as(
        "SELECT military_savings_product_id, minimum_term_months,
                maximum_term_months_exclusive, fixed_rate_bp
         FROM military_savings_product_rate_band
         WHERE career_catalog_bundle_id = ?
         ORDER BY military_savings_product_id, band_order",
    )
    .bind(scope.career_catalog_bundle_id)
    .fetch_all(&mut **tx)
    .await?;
    let eligible_services: Vec<(u64, String)> = sqlx::query_as(
        "SELECT eligible.military_savings_policy_id, eligible.service_type
         FROM military_savings_policy_eligible_service AS eligible
         INNER JOIN military_savings_policy AS policy
           ON policy.id = eligible.military_savings_policy_id
         WHERE policy.employment_policy_set_id = ?
           AND ? >= policy.effective_from
           AND (policy.effective_to_exclusive IS NULL OR ? < policy.effective_to_exclusive)
         ORDER BY eligible.military_savings_policy_id, eligible.service_type",
    )
    .bind(scope.employment_policy_set_id)
    .bind(scope.market_date)
    .bind(scope.market_date)
    .fetch_all(&mut **tx)
    .await?;

    let rules = create_military_rules();
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let policy_id = row
            .policy_id
            .context("published military savings product has no effective policy")?;
        let minimum_remaining_service_months = row
            .minimum_remaining_service_months
            .context("military savings policy has no remaining-service minimum")?;
        let maximum_active_contracts = row
            .max_contracts_per_service
            .context("military savings policy has no active-contract limit")?;
        let maximum_contracts_per_institution = row
            .max_contracts_per_institution
            .context("military savings policy has no institution limit")?;
        let maximum_institution_monthly_contribution_krw = row
            .institution_monthly_limit_krw
            .context("military savings policy has no institution monthly limit")?;
        let maximum_total_monthly_contribution_krw = row
            .person_monthly_limit_krw
            .context("military savings policy has no total monthly limit")?;
        let limit_setting_unit_krw = row
            .limit_setting_unit_krw
            .context("military savings policy has no limit-setting unit")?;
        let minimum_monthly_contribution_krw = row
            .minimum_installment_krw
            .context("military savings policy has no minimum installment")?;
        let installment_unit_krw = row
            .installment_unit_krw
            .context("military savings policy has no installment unit")?;
        let government_matching_rate_ppm = i64::from(
            row.government_match_rate_ppm
                .context("military savings policy has no matching rate")?,
        );
        let government_match_payment_day_of_month = row
            .government_match_next_month_day
            .context("military savings policy has no matching payment day")?;
        let maturity_tax_exempt = row
            .tax_exempt
            .context("military savings policy has no tax treatment")?;
        let mut reasons = Vec::new();
        if scope.military_status != "serving" || row.active_service_type.is_none() {
            reasons.push(MilitarySavingsIneligibilityReason::MilitaryStateConflict);
        }
        let eligible_types = eligible_services
            .iter()
            .filter(|(id, _)| *id == policy_id)
            .map(|(_, value)| enum_from_db(value))
            .collect::<Result<Vec<MilitaryServiceType>>>()?;
        if let Some(service_type) = row.active_service_type.as_deref() {
            let service_type = enum_from_db(service_type)?;
            if !eligible_types.contains(&service_type) {
                reasons.push(MilitarySavingsIneligibilityReason::ServiceTypeNotEligible);
            }
        }
        if let Some(service_end_exclusive_date) = row.service_end_exclusive_date
            && !rules.minimum_remaining_service_met(
                scope.market_date,
                service_end_exclusive_date,
                minimum_remaining_service_months,
            )?
        {
            reasons.push(MilitarySavingsIneligibilityReason::MinimumRemainingService);
        }
        if row.active_contract_count >= i64::from(maximum_active_contracts) {
            reasons.push(MilitarySavingsIneligibilityReason::ActiveContractLimit);
        }
        if row.institution_contract_count >= i64::from(maximum_contracts_per_institution) {
            reasons.push(MilitarySavingsIneligibilityReason::InstitutionLimit);
        }
        let join_start = max_date(row.available_from, row.policy_effective_from);
        let join_through = row
            .join_through
            .context("military savings policy has no join-through date")?;
        let (join_end, join_ineligibility) = classify_savings_join_window(
            scope.market_date,
            join_start,
            row.available_to_exclusive
                .map(|date| {
                    date.previous_day()
                        .context("military savings product window is empty")
                })
                .transpose()?,
            row.policy_effective_to_exclusive
                .map(|date| {
                    date.previous_day()
                        .context("military savings policy window is empty")
                })
                .transpose()?,
            join_through,
        )?;
        if let Some(reason) = join_ineligibility {
            reasons.push(reason);
        }
        if row.day_count_denominator != 365
            || row.interest_rounding_kind != "floor"
            || row.interest_rounding_unit_krw != 1
        {
            reasons.push(MilitarySavingsIneligibilityReason::PolicyUnavailable);
        }
        let interest_tiers = rates
            .iter()
            .filter(|rate| rate.military_savings_product_id == row.id)
            .map(|rate| MilitarySavingsInterestTierState {
                minimum_term_months: rate.minimum_term_months,
                maximum_term_months_inclusive: rate.maximum_term_months_exclusive - 1,
                annual_interest_rate_ppm: i64::from(rate.fixed_rate_bp) * 100,
            })
            .collect::<Vec<_>>();
        if interest_tiers.is_empty() {
            reasons.push(MilitarySavingsIneligibilityReason::PolicyUnavailable);
        }
        items.push(MilitarySavingsProductState {
            id: ResourceId::from_u64(row.id),
            product_key: row.product_key,
            institution_key: row.institution_key,
            institution_display_name: row.institution_display_name,
            eligible: reasons.is_empty(),
            ineligibility_reasons: reasons,
            eligible_service_types: eligible_types,
            join_start_date: join_start.to_string(),
            join_end_date: join_end.to_string(),
            minimum_remaining_service_months,
            maximum_active_contracts,
            maximum_contracts_per_institution,
            minimum_monthly_contribution_krw,
            maximum_institution_monthly_contribution_krw,
            maximum_total_monthly_contribution_krw,
            limit_setting_unit_krw,
            installment_unit_krw,
            interest_tiers,
            day_count_convention: MilitarySavingsDayCountConvention::Actual365,
            interest_rounding: MilitarySavingsInterestRounding::FloorToKrw,
            early_close_annual_interest_rate_ppm: i64::from(row.early_termination_rate_bp) * 100,
            government_matching_rate_ppm,
            government_match_payment_day_of_month,
            maturity_tax_exempt,
        });
    }
    Ok(MilitarySavingsProductsState { items })
}

async fn read_savings_history_in_tx(
    tx: &mut Transaction<'_, MySql>,
    scope: &MilitaryReadScopeRow,
    before: Option<u64>,
    limit: u32,
) -> Result<MilitarySavingsPageState> {
    let mut rows: Vec<SavingsHistoryRow> = sqlx::query_as(
        "SELECT contract.id, contract.military_service_id AS service_id,
                contract.military_savings_product_id AS product_version_id,
                product.product_key, institution.institution_key,
                institution.display_name AS institution_display_name,
                contract.status, contract.monthly_contribution_krw,
                contract.debit_day_of_month, contract.principal_krw,
                contract.paid_installment_count, contract.missed_installment_count,
                CASE WHEN contract.status = 'active' THEN
                    (SELECT MIN(next_installment.due_game_day)
                     FROM military_savings_installment AS next_installment
                     WHERE next_installment.military_savings_contract_id = contract.id
                       AND next_installment.status = 'scheduled')
                ELSE NULL END AS next_installment_game_day,
                contract.maturity_game_day, contract.opened_game_day,
                contract.first_installment_game_day, contract.term_months,
                contract.fixed_rate_bp, contract.closed_game_day, contract.closure_kind,
                contract.bank_interest_krw, contract.government_match_received_krw,
                (SELECT MAX(match_installment.government_match_paid_game_day)
                 FROM military_savings_installment AS match_installment
                 WHERE match_installment.military_savings_contract_id = contract.id)
                    AS government_match_paid_game_day,
                DATE_ADD(world.start_date, INTERVAL contract.maturity_game_day DAY)
                    AS maturity_date,
                product.day_count_denominator, product.interest_rounding_unit_krw,
                savings_policy.government_match_next_month_day,
                savings_policy.government_match_rate_ppm
         FROM military_savings_contract AS contract
         INNER JOIN save ON save.id = contract.save_id AND save.run_revision = contract.run_revision
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         INNER JOIN military_savings_product_version AS product
           ON product.career_catalog_bundle_id = contract.career_catalog_bundle_id
          AND product.id = contract.military_savings_product_id
         INNER JOIN military_savings_institution_catalog AS catalog
           ON catalog.career_catalog_bundle_id = contract.career_catalog_bundle_id
          AND catalog.id = contract.military_savings_institution_id
         INNER JOIN financial_institution AS institution
           ON institution.id = catalog.financial_institution_id
         INNER JOIN military_savings_policy AS savings_policy
           ON savings_policy.id = contract.military_savings_policy_id
         WHERE contract.save_id = ? AND contract.run_revision = ?
           AND (? IS NULL OR contract.id < ?)
         ORDER BY contract.id DESC
         LIMIT ?",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(before)
    .bind(before)
    .bind(
        limit
            .checked_add(1)
            .context("military savings page limit overflowed")?,
    )
    .fetch_all(&mut **tx)
    .await?;
    let has_more = rows.len() > usize::try_from(limit)?;
    rows.truncate(usize::try_from(limit)?);
    let contract_ids = rows.iter().map(|row| row.id).collect::<Vec<_>>();
    let installments: Vec<SavingsInstallmentRow> = if contract_ids.is_empty() {
        Vec::new()
    } else {
        let mut query = QueryBuilder::<MySql>::new(
            "SELECT installment.id, installment.military_savings_contract_id,
                    installment.installment_no, installment.due_game_day, installment.status,
                    installment.paid_game_day,
                    CASE WHEN installment.paid_game_day IS NULL THEN NULL
                         ELSE DATE_ADD(world.start_date, INTERVAL installment.paid_game_day DAY)
                    END AS paid_date,
                    installment.paid_principal_krw, installment.matching_policy_id,
                    installment.matching_rate_ppm
             FROM military_savings_installment AS installment
             INNER JOIN save ON save.id = installment.save_id
                            AND save.run_revision = installment.run_revision
             INNER JOIN market_world AS world ON world.id = save.market_world_id
             WHERE installment.save_id = ",
        );
        query
            .push_bind(scope.save_id)
            .push(" AND installment.run_revision = ")
            .push_bind(scope.run_revision)
            .push(" AND installment.military_savings_contract_id IN (");
        let mut ids = query.separated(", ");
        for contract_id in &contract_ids {
            ids.push_bind(contract_id);
        }
        ids.push_unseparated(")");
        query
            .push(" ORDER BY installment.military_savings_contract_id, installment.installment_no");
        query
            .build_query_as::<SavingsInstallmentRow>()
            .fetch_all(&mut **tx)
            .await?
    };

    let rules = create_military_rules();
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let paid = installments
            .iter()
            .filter(|item| item.military_savings_contract_id == row.id && item.status == "paid")
            .map(|item| {
                Ok(PaidMilitarySavingsInstallment {
                    installment_no: u32::from(item.installment_no),
                    paid_date: item
                        .paid_date
                        .context("paid military installment has no date")?,
                    principal_krw: item.paid_principal_krw,
                    government_matching_rate_ppm: i64::from(
                        item.matching_rate_ppm
                            .context("paid military installment has no matching rate")?,
                    ),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let installment_states = installments
            .iter()
            .filter(|item| item.military_savings_contract_id == row.id)
            .map(|item| {
                Ok(MilitarySavingsInstallmentState {
                    id: ResourceId::from_u64(item.id),
                    installment_no: item.installment_no,
                    due_game_day: item.due_game_day,
                    status: enum_from_db(&item.status)?,
                    paid_game_day: item.paid_game_day,
                    principal_krw: item.paid_principal_krw,
                    government_matching_policy_version_id: item
                        .matching_policy_id
                        .map(ResourceId::from_u64),
                    government_matching_rate_ppm: item.matching_rate_ppm.map(i64::from),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let status: MilitarySavingsContractStatus = enum_from_db(&row.status)?;
        let projected_maturity = if status == MilitarySavingsContractStatus::Active {
            let projected = paid
                .iter()
                .cloned()
                .chain(
                    installments
                        .iter()
                        .filter(|item| {
                            item.military_savings_contract_id == row.id
                                && item.status == "scheduled"
                        })
                        .map(|item| PaidMilitarySavingsInstallment {
                            installment_no: u32::from(item.installment_no),
                            paid_date: scope.market_date
                                + Duration::days(i64::from(
                                    item.due_game_day.saturating_sub(scope.game_day),
                                )),
                            principal_krw: row.monthly_contribution_krw,
                            government_matching_rate_ppm: i64::from(row.government_match_rate_ppm),
                        }),
                )
                .collect::<Vec<_>>();
            let plan = rules.plan_savings_maturity(MilitarySavingsMaturityInput {
                maturity_date: row.maturity_date,
                service_completion_confirmed: true,
                annual_interest_rate_ppm: i64::from(row.fixed_rate_bp) * 100,
                day_count_denominator: row.day_count_denominator,
                interest_rounding_unit_krw: row.interest_rounding_unit_krw,
                government_match_payment_day_of_month: row.government_match_next_month_day,
                paid_installments: &projected,
            })?;
            Some(MilitarySavingsMaturityProjectionState {
                assumption: MilitarySavingsProjectionAssumption::AllScheduledInstallmentsPaid,
                principal_krw: plan.principal_krw,
                gross_bank_interest_krw: plan.gross_bank_interest_krw,
                government_match_krw: plan.government_match.amount_krw,
                bank_payout_krw: plan.wallet_credit_krw,
                total_benefit_krw: plan
                    .wallet_credit_krw
                    .checked_add(plan.government_match.amount_krw)
                    .context("military savings projection overflowed")?,
            })
        } else {
            None
        };
        let settled_principal_krw = if status == MilitarySavingsContractStatus::Active {
            0
        } else {
            row.principal_krw
        };
        items.push(MilitarySavingsHistoryItemState {
            id: ResourceId::from_u64(row.id),
            service_id: ResourceId::from_u64(row.service_id),
            product_version_id: ResourceId::from_u64(row.product_version_id),
            product_key: row.product_key,
            institution_key: row.institution_key,
            institution_display_name: row.institution_display_name,
            status,
            monthly_contribution_krw: row.monthly_contribution_krw,
            debit_day_of_month: row.debit_day_of_month,
            principal_krw: row.principal_krw,
            paid_installment_count: row.paid_installment_count,
            missed_installment_count: row.missed_installment_count,
            next_installment_game_day: row.next_installment_game_day,
            maturity_game_day: row.maturity_game_day,
            opened_game_day: row.opened_game_day,
            first_installment_game_day: row.first_installment_game_day,
            contract_term_months: row.term_months,
            annual_interest_rate_ppm: i64::from(row.fixed_rate_bp) * 100,
            closed_game_day: row.closed_game_day,
            closure_reason: row.closure_kind.as_deref().map(enum_from_db).transpose()?,
            settled_principal_krw,
            gross_bank_interest_krw: row.bank_interest_krw,
            government_match_krw: row.government_match_received_krw,
            bank_payout_krw: settled_principal_krw
                .checked_add(row.bank_interest_krw)
                .context("military savings payout overflowed")?,
            government_match_paid_game_day: row.government_match_paid_game_day,
            projected_maturity,
            installments: installment_states,
        });
    }
    let next_before = has_more.then(|| items.last().map(|item| item.id)).flatten();
    Ok(MilitarySavingsPageState { items, next_before })
}

fn max_date(left: Date, right: Option<Date>) -> Date {
    right.map_or(left, |right| left.max(right))
}

fn min_date(first: Option<Date>, second: Option<Date>, third: Option<Date>) -> Option<Date> {
    [first, second, third].into_iter().flatten().min()
}

fn classify_savings_join_window(
    current_date: Date,
    join_start: Date,
    product_join_end: Option<Date>,
    policy_effective_end: Option<Date>,
    policy_join_through: Date,
) -> Result<(Date, Option<MilitarySavingsIneligibilityReason>)> {
    let join_end = min_date(
        product_join_end,
        policy_effective_end,
        Some(policy_join_through),
    )
    .context("military savings join window has no end date")?;
    let reason = if current_date > policy_join_through {
        Some(MilitarySavingsIneligibilityReason::PolicyUnavailable)
    } else if current_date < join_start || current_date > join_end {
        Some(MilitarySavingsIneligibilityReason::JoinWindowClosed)
    } else {
        None
    };
    Ok((join_end, reason))
}

async fn load_command_option_policy(
    tx: &mut Transaction<'_, MySql>,
    scope: &MilitaryReadScopeRow,
    option_version_id: u64,
) -> Result<Option<(CommandOptionPolicyRow, MilitaryOptionPolicy)>> {
    let row: Option<CommandOptionPolicyRow> = sqlx::query_as(
        "SELECT option_row.id AS option_version_id, option_policy.id AS option_policy_id,
                option_policy.service_type, option_policy.service_duration_months,
                option_policy.pay_schedule_kind, option_policy.payday_day_of_month,
                option_policy.partial_month_pay_kind, eligibility.minimum_education,
                eligibility.required_certification_count, eligibility.minimum_experience_days,
                option_row.effort_life_status, capacity.effort_units
         FROM military_option_version AS option_row
         INNER JOIN military_option_eligibility_rule AS eligibility
           ON eligibility.career_catalog_bundle_id = option_row.career_catalog_bundle_id
          AND eligibility.military_option_version_id = option_row.id
         INNER JOIN military_option_policy AS option_policy
           ON option_policy.career_catalog_bundle_id = option_row.career_catalog_bundle_id
          AND option_policy.military_option_version_id = option_row.id
          AND option_policy.employment_policy_set_id = ?
          AND option_policy.availability_status = 'available'
          AND ? >= option_policy.effective_from
          AND (option_policy.effective_to_exclusive IS NULL
               OR ? < option_policy.effective_to_exclusive)
         INNER JOIN career_effort_capacity AS capacity
           ON capacity.career_catalog_bundle_id = option_row.career_catalog_bundle_id
          AND BINARY capacity.life_status = BINARY option_row.effort_life_status
         WHERE option_row.career_catalog_bundle_id = ? AND option_row.id = ?",
    )
    .bind(scope.employment_policy_set_id)
    .bind(
        scope
            .market_date
            .next_day()
            .context("military start date overflowed")?,
    )
    .bind(
        scope
            .market_date
            .next_day()
            .context("military start date overflowed")?,
    )
    .bind(scope.career_catalog_bundle_id)
    .bind(option_version_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let pay_stages: Vec<MilitaryPayStagePolicy> = sqlx::query_as::<_, MilitaryPayStageRow>(
        "SELECT policy.military_option_version_id,
                stage.start_service_month, stage.end_service_month_exclusive,
                stage.monthly_gross_pay_krw
         FROM military_pay_stage AS stage
         INNER JOIN military_option_policy AS policy ON policy.id = stage.military_option_policy_id
         WHERE stage.military_option_policy_id = ?
         ORDER BY stage.stage_order",
    )
    .bind(row.option_policy_id)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|stage| MilitaryPayStagePolicy {
        start_service_month: stage.start_service_month,
        end_exclusive_service_month: stage.end_service_month_exclusive,
        gross_monthly_pay_krw: stage.monthly_gross_pay_krw,
    })
    .collect();
    let experience: Vec<MilitaryExperiencePolicy> = sqlx::query_as::<_, MilitaryExperienceRow>(
        "SELECT mapping.military_option_version_id, family.job_family_key,
                    mapping.experience_credit_ppm
             FROM military_option_job_family AS mapping
             INNER JOIN career_job_family AS family
               ON family.career_catalog_bundle_id = mapping.career_catalog_bundle_id
              AND family.id = mapping.career_job_family_id
             WHERE mapping.career_catalog_bundle_id = ?
               AND mapping.military_option_version_id = ?
             ORDER BY family.job_family_key",
    )
    .bind(scope.career_catalog_bundle_id)
    .bind(option_version_id)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|credit| MilitaryExperiencePolicy {
        job_family_key: credit.job_family_key,
        daily_credit_ppm: credit.experience_credit_ppm,
    })
    .collect();
    let option = MilitaryOptionPolicy {
        option_version_id: row.option_version_id,
        service_type: enum_from_db(&row.service_type)?,
        service_duration_months: row.service_duration_months,
        pay_schedule_kind: enum_from_db(&row.pay_schedule_kind)?,
        payday_day_of_month: row.payday_day_of_month,
        partial_month_pay_kind: enum_from_db(&row.partial_month_pay_kind)?,
        hard_requirements: MilitaryHardRequirementsState {
            minimum_education: row
                .minimum_education
                .as_deref()
                .map(enum_from_db)
                .transpose()?,
            minimum_certification_count: row.required_certification_count,
            minimum_experience_days: row.minimum_experience_days,
        },
        pay_stages,
        effort_life_status: enum_from_db(&row.effort_life_status)?,
        daily_effort_capacity_units: row.effort_units,
        experience,
    };
    Ok(Some((row, option)))
}

async fn load_savings_enrollment_policy(
    tx: &mut Transaction<'_, MySql>,
    scope: &MilitaryReadScopeRow,
    product_version_id: u64,
) -> Result<
    Option<(
        SavingsEnrollmentRow,
        MilitarySavingsPolicy,
        MilitarySavingsProductPolicy,
        Vec<ActiveMilitarySavingsContract>,
    )>,
> {
    let row: Option<SavingsEnrollmentRow> = sqlx::query_as(
        "SELECT service.id AS military_service_id, service.service_type,
                service.end_game_day AS service_end_game_day,
                service.end_exclusive_date AS service_end_exclusive_date,
                savings_policy.id AS savings_policy_id,
                savings_policy.minimum_remaining_service_months,
                savings_policy.max_contracts_per_service,
                savings_policy.max_contracts_per_institution,
                savings_policy.institution_monthly_limit_krw,
                savings_policy.person_monthly_limit_krw,
                savings_policy.limit_setting_unit_krw,
                savings_policy.minimum_installment_krw,
                savings_policy.installment_unit_krw,
                savings_policy.government_match_rate_ppm,
                savings_policy.government_match_next_month_day,
                product.id AS product_version_id,
                catalog.id AS military_savings_institution_id,
                (SELECT COUNT(*) FROM military_savings_contract AS institution_contract
                 WHERE institution_contract.military_service_id = service.id
                   AND institution_contract.military_savings_institution_id = catalog.id)
                    AS institution_contract_count,
                institution.institution_key, product.day_count_denominator,
                product.interest_rounding_unit_krw,
                product.early_termination_rate_bp
         FROM military_service AS service
         INNER JOIN military_savings_policy AS savings_policy
           ON savings_policy.employment_policy_set_id = service.employment_policy_set_id
          AND ? >= savings_policy.effective_from
          AND (savings_policy.effective_to_exclusive IS NULL
               OR ? < savings_policy.effective_to_exclusive)
          AND ? <= savings_policy.join_through
         INNER JOIN military_savings_product_version AS product
           ON product.career_catalog_bundle_id = service.career_catalog_bundle_id
          AND product.id = ?
          AND ? >= product.available_from
          AND (product.available_to_exclusive IS NULL OR ? < product.available_to_exclusive)
         INNER JOIN military_savings_institution_catalog AS catalog
           ON catalog.career_catalog_bundle_id = product.career_catalog_bundle_id
          AND catalog.id = product.military_savings_institution_id
         INNER JOIN financial_institution AS institution
           ON institution.id = catalog.financial_institution_id
         WHERE service.save_id = ? AND service.run_revision = ?
           AND service.status = 'serving'",
    )
    .bind(scope.market_date)
    .bind(scope.market_date)
    .bind(scope.market_date)
    .bind(product_version_id)
    .bind(scope.market_date)
    .bind(scope.market_date)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let eligible_service_types: Vec<MilitaryServiceType> = sqlx::query_scalar::<_, String>(
        "SELECT service_type
         FROM military_savings_policy_eligible_service
         WHERE military_savings_policy_id = ?
         ORDER BY service_type",
    )
    .bind(row.savings_policy_id)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|value| enum_from_db(&value))
    .collect::<Result<Vec<_>>>()?;
    let interest_tiers = sqlx::query_as::<_, SavingsRateRow>(
        "SELECT military_savings_product_id, minimum_term_months,
                maximum_term_months_exclusive, fixed_rate_bp
         FROM military_savings_product_rate_band
         WHERE military_savings_product_id = ?
         ORDER BY band_order",
    )
    .bind(row.product_version_id)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|rate| crate::career::MilitarySavingsInterestTier {
        minimum_term_months: rate.minimum_term_months,
        maximum_term_months_inclusive: rate.maximum_term_months_exclusive - 1,
        annual_interest_rate_ppm: i64::from(rate.fixed_rate_bp) * 100,
    })
    .collect::<Vec<_>>();
    let active_contracts = sqlx::query_as::<_, ActiveContractPolicyRow>(
        "SELECT institution.institution_key, contract.monthly_contribution_krw
         FROM military_savings_contract AS contract
         INNER JOIN military_savings_institution_catalog AS catalog
           ON catalog.career_catalog_bundle_id = contract.career_catalog_bundle_id
          AND catalog.id = contract.military_savings_institution_id
         INNER JOIN financial_institution AS institution
           ON institution.id = catalog.financial_institution_id
         WHERE contract.save_id = ? AND contract.run_revision = ?
           AND contract.status = 'active'
         ORDER BY contract.id
         FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|contract| ActiveMilitarySavingsContract {
        institution_key: contract.institution_key,
        monthly_contribution_krw: contract.monthly_contribution_krw,
    })
    .collect::<Vec<_>>();
    let policy = MilitarySavingsPolicy {
        eligible_service_types,
        minimum_remaining_service_months: row.minimum_remaining_service_months,
        maximum_active_contracts: row.max_contracts_per_service,
        maximum_contracts_per_institution: row.max_contracts_per_institution,
        institution_monthly_limit_krw: row.institution_monthly_limit_krw,
        total_monthly_limit_krw: row.person_monthly_limit_krw,
        limit_setting_unit_krw: row.limit_setting_unit_krw,
        minimum_installment_krw: row.minimum_installment_krw,
        installment_unit_krw: row.installment_unit_krw,
        government_matching_rate_ppm: i64::from(row.government_match_rate_ppm),
        government_match_payment_day_of_month: row.government_match_next_month_day,
    };
    let product = MilitarySavingsProductPolicy {
        product_version_id: row.product_version_id,
        institution_key: row.institution_key.clone(),
        interest_tiers,
        day_count_denominator: row.day_count_denominator,
        interest_rounding_unit_krw: row.interest_rounding_unit_krw,
        early_close_annual_interest_rate_ppm: i64::from(row.early_termination_rate_bp) * 100,
    };
    Ok(Some((row, policy, product, active_contracts)))
}

async fn insert_savings_schedule_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    contract_id: u64,
    plan: &crate::career::MilitarySavingsEnrollmentPlan,
) -> Result<()> {
    for installment in &plan.installments {
        let payload = serde_json::json!({
            "version": 1,
            "contractId": contract_id.to_string(),
            "installmentNo": installment.installment_no,
        });
        let settlement = sqlx::query(
            "INSERT INTO scheduled_settlement
                 (save_id, run_revision, due_game_day, kind, payload,
                  source_kind, source_id, occurrence, status)
             VALUES (?, ?, ?, 'militarySavingsInstallment', CAST(? AS JSON),
                     'militarySavingsContract', ?, ?, 'pending')",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(installment.due_game_day)
        .bind(payload.to_string())
        .bind(contract_id.to_string())
        .bind(installment.installment_no)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT INTO military_savings_installment
                 (save_id, run_revision, military_savings_contract_id, installment_no,
                  due_game_day, scheduled_settlement_id, status, planned_principal_krw,
                  paid_principal_krw, paid_game_day, no_movement_reason,
                  matching_policy_id, matching_rate_ppm, government_match_krw,
                  ledger_transaction_id, government_match_settlement_id,
                  government_match_ledger_transaction_id, government_match_paid_game_day)
             VALUES (?, ?, ?, ?, ?, ?, 'scheduled', ?, 0, NULL, NULL,
                     NULL, NULL, 0, NULL, NULL, NULL, NULL)",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(contract_id)
        .bind(installment.installment_no)
        .bind(installment.due_game_day)
        .bind(settlement.last_insert_id())
        .bind(plan.monthly_contribution_krw)
        .execute(&mut **tx)
        .await?;
    }
    let payload = serde_json::json!({
        "version": 1,
        "contractId": contract_id.to_string(),
    });
    sqlx::query(
        "INSERT INTO scheduled_settlement
             (save_id, run_revision, due_game_day, kind, payload,
              source_kind, source_id, occurrence, status)
         VALUES (?, ?, ?, 'militarySavingsMaturity', CAST(? AS JSON),
                 'militarySavingsContract', ?, ?, 'pending')",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(plan.maturity_game_day)
    .bind(payload.to_string())
    .bind(contract_id.to_string())
    .bind(u64::from(plan.contract_term_months) + 1)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_service_progress_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    bundle_id: u64,
    service_id: u64,
    option_version_id: u64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO military_service_progress
             (save_id, run_revision, career_catalog_bundle_id, military_service_id,
              career_job_family_id, military_option_version_id, experience_credit_ppm,
              credited_experience_day_ppm, last_credited_game_day, status)
         SELECT ?, ?, mapping.career_catalog_bundle_id, ?, mapping.career_job_family_id,
                mapping.military_option_version_id, mapping.experience_credit_ppm,
                0, NULL, 'active'
         FROM military_option_job_family AS mapping
         WHERE mapping.career_catalog_bundle_id = ?
           AND mapping.military_option_version_id = ?
         ORDER BY mapping.career_job_family_id",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(service_id)
    .bind(bundle_id)
    .bind(option_version_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_service_actions_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    bundle_id: u64,
    service_id: u64,
    start_game_day: u32,
    end_game_day: u32,
) -> Result<()> {
    for (kind, occurrence, due_game_day) in [
        ("militaryServiceStart", 1_u64, start_game_day),
        ("militaryServiceCompletion", 2_u64, end_game_day),
    ] {
        sqlx::query(
            "INSERT INTO career_scheduled_action
                 (save_id, run_revision, career_catalog_bundle_id, recruitment_ruleset_id,
                  action_kind, payload_version, phase_rank, due_game_day, status,
                  source_kind, source_id, occurrence, employment_contract_id,
                  job_application_id, military_service_id, platform_catalog_id,
                  platform_key, invitation_generation_game_day)
             VALUES (?, ?, ?, NULL, ?, 1, 10, ?, 'pending',
                     'militaryService', ?, ?, NULL, NULL, ?, NULL, NULL, NULL)",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(bundle_id)
        .bind(kind)
        .bind(due_game_day)
        .bind(service_id)
        .bind(occurrence)
        .bind(service_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn insert_military_pay_schedule_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    service_id: u64,
    service: &crate::career::MilitaryServicePlan,
    option: &MilitaryOptionPolicy,
) -> Result<()> {
    let schedule = create_military_rules().plan_pay_schedule(MilitaryPayScheduleInput {
        service_start_game_day: service.start_game_day,
        service_start_date: service.start_date,
        service_end_exclusive_date: service.end_exclusive_date,
        option,
    })?;
    for period in schedule {
        let payload = serde_json::json!({
            "version": 1,
            "militaryServiceId": service_id.to_string(),
            "periodNo": period.payroll_period,
        });
        sqlx::query(
            "INSERT INTO scheduled_settlement
                 (save_id, run_revision, due_game_day, kind, payload,
                  source_kind, source_id, occurrence, status)
             VALUES (?, ?, ?, 'militaryPay', CAST(? AS JSON),
                     'militaryService', ?, ?, 'pending')",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(period.pay_game_day)
        .bind(payload.to_string())
        .bind(service_id.to_string())
        .bind(period.payroll_period)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn repair_legacy_military_pay_schedule_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    service_id: u64,
    current_game_day: u32,
    service: &crate::career::MilitaryServicePlan,
    option: &MilitaryOptionPolicy,
) -> Result<()> {
    let schedule = create_military_rules().plan_pay_schedule(MilitaryPayScheduleInput {
        service_start_game_day: service.start_game_day,
        service_start_date: service.start_date,
        service_end_exclusive_date: service.end_exclusive_date,
        option,
    })?;
    for period in schedule
        .into_iter()
        .filter(|period| period.pay_game_day > current_game_day)
    {
        let existing: Option<MilitarySettlementRow> = sqlx::query_as(
            "SELECT id, due_game_day, kind, CAST(payload AS CHAR) AS payload_json,
                    source_kind, source_id, occurrence, status
             FROM scheduled_settlement
             WHERE save_id = ? AND run_revision = ?
               AND source_kind = 'militaryService' AND source_id = ? AND occurrence = ?
             FOR UPDATE",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(service_id.to_string())
        .bind(period.payroll_period)
        .fetch_optional(&mut **tx)
        .await?;
        if let Some(existing) = existing {
            let payload: MilitaryPayPayload = serde_json::from_str(&existing.payload_json)
                .context("stored legacy military pay payload is invalid")?;
            ensure!(
                existing.due_game_day == period.pay_game_day
                    && existing.kind == "militaryPay"
                    && existing.source_kind == "militaryService"
                    && existing.source_id == service_id.to_string()
                    && existing.occurrence == u64::from(period.payroll_period)
                    && existing.status == "pending"
                    && payload.version == MILITARY_PAYLOAD_VERSION
                    && payload.military_service_id == ResourceId::from_u64(service_id)
                    && payload.period_no == u64::from(period.payroll_period),
                "legacy military pay schedule disagrees with its pinned option"
            );
            continue;
        }
        let payload = serde_json::json!({
            "version": MILITARY_PAYLOAD_VERSION,
            "militaryServiceId": service_id.to_string(),
            "periodNo": period.payroll_period,
        });
        sqlx::query(
            "INSERT INTO scheduled_settlement
                 (save_id, run_revision, due_game_day, kind, payload,
                  source_kind, source_id, occurrence, status)
             VALUES (?, ?, ?, 'militaryPay', CAST(? AS JSON),
                     'militaryService', ?, ?, 'pending')",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(period.pay_game_day)
        .bind(payload.to_string())
        .bind(service_id.to_string())
        .bind(period.payroll_period)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn finish_service_replay(
    mut tx: Transaction<'_, MySql>,
    save_id: u64,
    result: Result<MilitaryServiceCommandReceipt, CareerFailureCode>,
) -> Result<CareerStoreResult<MilitaryServiceCommandReceipt>> {
    match result {
        Err(failure) => {
            tx.commit().await?;
            Ok(CareerStoreResult::Rejected(failure))
        }
        Ok(mut receipt) => {
            ensure!(
                !receipt.replayed,
                "stored military service receipt is replayed"
            );
            receipt.replayed = true;
            let save = read_state(&mut tx, save_id).await?;
            tx.commit().await?;
            Ok(CareerStoreResult::Applied {
                receipt,
                save: Box::new(save),
            })
        }
    }
}

async fn finish_savings_replay(
    mut tx: Transaction<'_, MySql>,
    save_id: u64,
    result: Result<MilitarySavingsCommandReceipt, CareerFailureCode>,
) -> Result<CareerStoreResult<MilitarySavingsCommandReceipt>> {
    match result {
        Err(failure) => {
            tx.commit().await?;
            Ok(CareerStoreResult::Rejected(failure))
        }
        Ok(mut receipt) => {
            ensure!(
                !receipt.replayed,
                "stored military savings receipt is replayed"
            );
            receipt.replayed = true;
            let save = read_state(&mut tx, save_id).await?;
            tx.commit().await?;
            Ok(CareerStoreResult::Applied {
                receipt,
                save: Box::new(save),
            })
        }
    }
}

fn start_service_fingerprint(command: &StartMilitaryServiceCommand) -> String {
    command_fingerprint(
        "lifeledger.career.military-service-start.v1",
        command.cursor,
        &[(
            "militaryOptionVersionId",
            command.military_option_version_id.to_string(),
        )],
    )
}

fn open_savings_fingerprint(command: &OpenMilitarySavingsCommand) -> String {
    command_fingerprint(
        "lifeledger.career.military-savings-open.v1",
        command.cursor,
        &[
            ("productVersionId", command.product_version_id.to_string()),
            (
                "monthlyContributionKrw",
                command.monthly_contribution_krw.to_string(),
            ),
            ("debitDayOfMonth", command.debit_day_of_month.to_string()),
        ],
    )
}

fn close_savings_fingerprint(command: &CloseMilitarySavingsCommand) -> String {
    command_fingerprint(
        "lifeledger.career.military-savings-close.v1",
        command.cursor,
        &[("contractId", command.contract_id.to_string())],
    )
}

fn command_fingerprint(
    version: &str,
    cursor: crate::finance::CommandCursor,
    fields: &[(&str, String)],
) -> String {
    let mut canonical = String::new();
    for (name, value) in [
        ("version", version.to_owned()),
        (
            "expectedRunRevision",
            cursor.expected_run_revision.to_string(),
        ),
        (
            "expectedStateRevision",
            cursor.expected_state_revision.to_string(),
        ),
        ("expectedGameDay", cursor.expected_game_day.to_string()),
    ]
    .into_iter()
    .chain(fields.iter().map(|(name, value)| (*name, value.clone())))
    {
        canonical.push_str(name);
        canonical.push('=');
        canonical.push_str(&value.len().to_string());
        canonical.push(':');
        canonical.push_str(&value);
        canonical.push('\n');
    }
    Sha256::digest(canonical.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn military_failure(error: MilitaryError) -> CareerFailureCode {
    match error {
        MilitaryError::MilitaryStateConflict => CareerFailureCode::MilitaryStateConflict,
        MilitaryError::NotEligible | MilitaryError::InsufficientRemainingService => {
            CareerFailureCode::NotEligible
        }
        MilitaryError::ContractLimitExceeded
        | MilitaryError::InstitutionLimitExceeded
        | MilitaryError::TotalLimitExceeded
        | MilitaryError::ArithmeticOverflow => CareerFailureCode::LimitExceeded,
        _ => CareerFailureCode::InvalidCommand,
    }
}

fn enum_to_db<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_value(value)?
        .as_str()
        .map(str::to_owned)
        .context("military enum did not serialize as a string")
}

fn enum_from_db<T: DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_value(serde_json::Value::String(value.to_owned()))
        .with_context(|| format!("stored enum value is invalid: {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    fn given_date(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("유효한 날짜여야 한다")
    }

    mod context_장병적금_상품의_가입창을_판정하는_경우 {
        use super::*;

        #[test]
        fn given_policy_join_through_다음날_when_판정하면_then_policy_unavailable이다() {
            let current_date = given_date(2027, Month::January, 1);

            let (_, reason) = classify_savings_join_window(
                current_date,
                given_date(2026, Month::January, 1),
                None,
                None,
                given_date(2026, Month::December, 31),
            )
            .expect("가입창을 판정해야 한다");

            assert_eq!(
                reason,
                Some(MilitarySavingsIneligibilityReason::PolicyUnavailable)
            );
        }

        #[test]
        fn given_가입시작일_전_when_판정하면_then_join_window_closed이다() {
            let current_date = given_date(2025, Month::December, 31);

            let (_, reason) = classify_savings_join_window(
                current_date,
                given_date(2026, Month::January, 1),
                None,
                None,
                given_date(2026, Month::December, 31),
            )
            .expect("가입창을 판정해야 한다");

            assert_eq!(
                reason,
                Some(MilitarySavingsIneligibilityReason::JoinWindowClosed)
            );
        }

        #[test]
        fn given_product_가입종료후_policy_cutoff전_when_판정하면_then_join_window_closed이다() {
            let current_date = given_date(2026, Month::July, 1);

            let (join_end, reason) = classify_savings_join_window(
                current_date,
                given_date(2026, Month::January, 1),
                Some(given_date(2026, Month::June, 30)),
                None,
                given_date(2026, Month::December, 31),
            )
            .expect("가입창을 판정해야 한다");

            assert_eq!(join_end, given_date(2026, Month::June, 30));
            assert_eq!(
                reason,
                Some(MilitarySavingsIneligibilityReason::JoinWindowClosed)
            );
        }

        #[test]
        fn given_가입창_안의날짜_when_판정하면_then_가입창사유가없다() {
            let current_date = given_date(2026, Month::June, 30);

            let (join_end, reason) = classify_savings_join_window(
                current_date,
                given_date(2026, Month::January, 1),
                Some(given_date(2026, Month::June, 30)),
                None,
                given_date(2026, Month::December, 31),
            )
            .expect("가입창을 판정해야 한다");

            assert_eq!(join_end, given_date(2026, Month::June, 30));
            assert_eq!(reason, None);
        }
    }
}
