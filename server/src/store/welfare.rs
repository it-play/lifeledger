//! M4-D welfare catalog, evaluation, application, and payment persistence.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};
use sha2::{Digest, Sha256};
use sqlx::{MySql, MySqlPool, Transaction};

use super::m2d_assets::{
    M2dWelfarePolicyValues, M2dWelfareValuationUnknown, read_m2d_welfare_policy_values_in_tx,
};
use super::mysql::{
    CommandIdentitySpec, CommandIdentityState, GameCommandReceiptWrite, inspect_command_identity,
    read_state, write_command_identity, write_game_command_receipt, write_ledger_transaction,
};
use super::types::{
    ActiveWelfareApplicationState, ApplyWelfareProgramCommand, GameCommandCursor, LifeFailureCode,
    LifeStoreResult, WelfareApplicationReceipt, WelfareApplicationStatusState,
    WelfareApplicationSummaryState, WelfareConditionOutcomeState, WelfareConditionResultState,
    WelfareEvaluationStatusState, WelfarePaymentState, WelfarePaymentStatusState,
    WelfareProgramState, WelfareProgramsState,
};
use crate::finance::{
    FinanceRules, LedgerAccountCode, LedgerPosting, LedgerSource, LedgerSourceKind,
    LedgerTransactionDraft, ResourceId, RunId, RunPolicyContext, ScheduledSettlement,
    SettlementKind, SettlementSourceKind,
};
use crate::life::{
    WelfareAuthorityRevision, WelfareBenefitDefinition, WelfareCollectionEvidence,
    WelfareCollectionEvidenceValue, WelfareEligibilityExpression, WelfareEnumValue,
    WelfareEvaluation, WelfareEvaluationInput, WelfareEvaluationStatus, WelfareEvidenceValue,
    WelfareExpression, WelfareFactEvidence, WelfareFactSource, WelfareFingerprintInput,
    WelfarePeriodPin, WelfareProgramCondition, WelfareProgramConstant, WelfareProgramDefinition,
    WelfareProgramPurpose, WelfareRankedAvailability, WelfareResolvedWindow, WelfareRules,
    WelfareTruth, WelfareUnknownReason, WelfareValue, WelfareValueType, WelfareWindowBound,
    WelfareWindowDays, WelfareWindowSpec,
};

const COMMAND_KIND_APPLY_WELFARE: &str = "applyWelfareProgram";
const MAX_WELFARE_PROGRAMS: usize = 16;
const MAX_ACTIVE_WELFARE_APPLICATIONS: usize = 8;

#[derive(Debug, Clone, sqlx::FromRow)]
struct WelfareScopeRow {
    save_id: u64,
    market_world_id: u64,
    market_world_product_bundle_id: Option<u64>,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
    life_catalog_set_id: u64,
    welfare_component_version_id: u64,
    availability: String,
    has_character: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WelfareProgramRow {
    id: u64,
    schema_version: u16,
    program_key: String,
    display_name: String,
    purpose: String,
    ranked_availability: String,
    application_kind: String,
    application_period_kind: String,
    application_start_game_day: Option<u32>,
    application_end_game_day: Option<u32>,
    duplicate_group_key: String,
    duplicate_scope: String,
    maximum_approved_per_group: u8,
    reassessment_basis: String,
    ast_node_count: u16,
    ast_max_depth: u8,
    eligibility_ast_json: String,
    benefit_formula_json: String,
    payment_schedule_json: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WelfareConditionRow {
    id: u64,
    condition_order: u8,
    condition_code: String,
    public_label: String,
    node_count: u16,
    max_depth: u8,
    expression_ast_json: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WelfareConstantRow {
    constant_order: u8,
    constant_key: String,
    value_type: String,
    unit: String,
    enum_schema_key: Option<String>,
    boolean_value: Option<bool>,
    integer_value: Option<i64>,
    string_value: Option<String>,
    date_value: Option<time::Date>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WelfareTriggerRow {
    trigger_order: u8,
    source_kind: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WelfareFactDefinitionRow {
    fact_order: u8,
    fact_key: String,
    value_type: String,
    unit: String,
    enum_schema_key: Option<String>,
    window_kind: String,
    minimum_window_days: Option<u16>,
    maximum_window_days: Option<u16>,
    collection_bound: Option<u8>,
    source_schema_version: u16,
    source_kind: String,
}

#[derive(Debug)]
struct ParsedWelfareExpression {
    expression: WelfareExpression,
    node_count: u16,
    max_depth: u8,
}

#[derive(Debug)]
struct ParsedWelfareEligibility {
    expression: WelfareEligibilityExpression,
    operator_count: u16,
    expanded_depth: u8,
}

#[derive(Debug, Clone)]
struct LoadedWelfareProgram {
    definition: WelfareProgramDefinition,
    display_name: String,
    duplicate_group_key: String,
    benefit_krw: i64,
    payment_delay_game_days: u16,
    conditions: Vec<WelfareConditionRow>,
}

#[derive(Debug, Clone)]
struct PlannedWelfareEvaluation {
    facts: Vec<WelfareFactEvidence>,
    collections: Vec<WelfareCollectionEvidence>,
    canonical_json: String,
    evaluation: WelfareEvaluation,
    evaluation_game_day: u32,
    authority_state_revision: u64,
    prior_close_state_revision: u64,
    previous_closed_start_game_day: u32,
}

#[derive(Debug, Default)]
struct WelfareEvidenceRequirements {
    facts: BTreeSet<(String, WelfareResolvedWindow)>,
    collections: BTreeSet<(String, WelfareResolvedWindow)>,
}

#[derive(Debug, Clone, Copy)]
enum WelfarePlanningMode {
    InitialCurrentDay,
    CurrentDayCommand,
    TargetDay { evaluation_game_day: u32 },
}

#[derive(Debug)]
struct WelfareEvaluationAnchor {
    facts: BTreeMap<(String, WelfareResolvedWindow), WelfareFactEvidence>,
    collections: BTreeMap<(String, WelfareResolvedWindow), WelfareCollectionEvidence>,
    period_pin: WelfarePeriodPin,
    prior_close_state_revision: u64,
}

#[derive(Debug, Clone)]
enum BoundedMoneyValues {
    Known(Vec<i64>),
    Unknown(WelfareUnknownReason),
}

#[derive(Debug)]
struct CurrentWelfareAuthorities {
    age_years: i64,
    member_count: i64,
    dependent_count: i64,
    residence_count: i64,
    residence_region: Option<String>,
    employment_status: String,
    military_status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WelfareEvaluationRow {
    id: u64,
    status: String,
    fact_fingerprint: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WelfareEvaluationConditionRow {
    condition_code: String,
    public_label: String,
    outcome: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WelfareApplicationRow {
    id: u64,
    status: String,
    application_game_day: u32,
    approval_game_day: Option<u32>,
    paid_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WelfarePaymentRow {
    id: u64,
    payment_no: u8,
    amount_krw: i64,
    due_game_day: u32,
    status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ActiveWelfareApplicationRow {
    application_id: u64,
    program_version_id: u64,
    program_key: String,
    display_name: String,
    status: String,
    application_game_day: u32,
    approval_game_day: u32,
    benefit_krw: i64,
    paid_krw: i64,
    payment_id: Option<u64>,
    payment_no: Option<u8>,
    payment_amount_krw: Option<i64>,
    payment_due_game_day: Option<u32>,
    payment_status: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct StoredWelfareReceiptRow {
    command_kind: String,
    payload_sha256: String,
    result_json: String,
    ledger_transaction_id: Option<u64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WelfareSettlementApplicationRow {
    id: u64,
    status: String,
    benefit_amount_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WelfareSettlementPaymentRow {
    id: u64,
    application_id: u64,
    payment_no: u8,
    due_game_day: u32,
    amount_krw: i64,
    status: String,
    scheduled_settlement_id: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
struct WelfareApplicationTransitionInsert<'a> {
    save_id: u64,
    run_revision: u32,
    application_id: u64,
    transition_no: u8,
    from_status: Option<&'a str>,
    to_status: &'a str,
    game_day: u32,
    reason: &'a str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WelfareSettlementPayload {
    version: u8,
    welfare_payment_id: ResourceId,
    application_id: ResourceId,
    payment_no: u16,
}

pub(super) async fn read_welfare_programs(
    pool: &MySqlPool,
    rules: &dyn WelfareRules,
    user_id: u64,
) -> Result<Option<WelfareProgramsState>> {
    let mut tx = pool.begin().await?;
    let Some(scope) = read_welfare_scope_for_user(&mut tx, user_id, false).await? else {
        tx.commit().await?;
        return Ok(None);
    };
    let programs = if scope.availability == "active" {
        let loaded = load_welfare_programs(&mut tx, rules, &scope).await?;
        let mut states = Vec::with_capacity(loaded.len());
        for program in loaded {
            states.push(read_welfare_program_state(&mut tx, &scope, &program).await?);
        }
        states
    } else {
        Vec::new()
    };
    tx.commit().await?;
    Ok(Some(WelfareProgramsState {
        component_version_id: ResourceId::from_u64(scope.welfare_component_version_id),
        game_day: scope.game_day,
        programs,
    }))
}

pub(super) async fn apply_welfare_program(
    pool: &MySqlPool,
    welfare_rules: &dyn WelfareRules,
    user_id: u64,
    command: &ApplyWelfareProgramCommand,
) -> Result<LifeStoreResult<WelfareApplicationReceipt>> {
    let fingerprint = apply_welfare_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(scope) = read_welfare_scope_for_user(&mut tx, user_id, true).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::WelfareResourceNotFound,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_APPLY_WELFARE,
        payload_sha256: &fingerprint,
        cursor: command.cursor,
    };
    match inspect_command_identity(&mut tx, scope.save_id, &identity).await? {
        CommandIdentityState::Conflict => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(
                LifeFailureCode::IdempotencyConflict,
            ));
        }
        CommandIdentityState::Matching => {
            let row =
                read_stored_welfare_receipt(&mut tx, scope.save_id, command.command_id.as_str())
                    .await?;
            ensure!(
                row.command_kind == COMMAND_KIND_APPLY_WELFARE
                    && row.payload_sha256 == fingerprint
                    && row.ledger_transaction_id.is_none(),
                "stored welfare receipt disagrees with its command"
            );
            let mut receipt: WelfareApplicationReceipt = serde_json::from_str(&row.result_json)
                .context("stored welfare receipt is invalid")?;
            ensure!(
                !receipt.replayed
                    && receipt.command_id == command.command_id
                    && receipt.program_version_id == command.program_version_id,
                "stored welfare result disagrees with its command"
            );
            receipt.replayed = true;
            let save = read_state(&mut tx, scope.save_id).await?;
            tx.commit().await?;
            return Ok(LifeStoreResult::Applied {
                receipt,
                save: Box::new(save),
            });
        }
        CommandIdentityState::Missing => {}
    }
    if !scope.has_character {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    }
    if !has_current_cursor(&scope, command.cursor) {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy));
    }
    if scope.availability != "active" {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::WelfareResourceNotFound,
        ));
    }
    let Some(program) = load_welfare_programs(&mut tx, welfare_rules, &scope)
        .await?
        .into_iter()
        .find(|program| program.definition.program_version_id == command.program_version_id)
    else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::WelfareResourceNotFound,
        ));
    };
    let duplicate_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM welfare_application
             WHERE save_id = ? AND run_revision = ?
               AND BINARY duplicate_group_claim_key = BINARY ?
         )",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(&program.duplicate_group_key)
    .fetch_one(&mut *tx)
    .await?;
    if duplicate_exists {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::Ineligible));
    }
    let planned = plan_welfare_evaluation(
        &mut tx,
        welfare_rules,
        &scope,
        &program,
        WelfarePlanningMode::CurrentDayCommand,
    )
    .await?;
    match planned.evaluation.status {
        WelfareEvaluationStatus::Eligible => {}
        WelfareEvaluationStatus::Ineligible => {
            tx.rollback().await?;
            return Ok(LifeStoreResult::Rejected(LifeFailureCode::Ineligible));
        }
        WelfareEvaluationStatus::Indeterminate | WelfareEvaluationStatus::NotEvaluated => {
            tx.rollback().await?;
            return Ok(LifeStoreResult::Rejected(
                LifeFailureCode::ValuationUnavailable,
            ));
        }
    }

    write_command_identity(&mut tx, scope.save_id, &identity).await?;
    let evaluation_id = persist_welfare_evaluation(&mut tx, &scope, &program, &planned).await?;
    let application_id = insert_welfare_application(
        &mut tx,
        &scope,
        &program,
        evaluation_id,
        &planned.evaluation.fact_fingerprint,
        command,
    )
    .await?;
    let payment =
        insert_welfare_payment_and_settlement(&mut tx, &scope, &program, application_id).await?;

    let committed_state_revision = scope
        .state_revision
        .checked_add(1)
        .context("welfare application state revision overflowed")?;
    let update = sqlx::query(
        "UPDATE save SET state_revision = ?
         WHERE id = ? AND run_revision = ? AND state_revision = ? AND game_day = ?",
    )
    .bind(committed_state_revision)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.state_revision)
    .bind(scope.game_day)
    .execute(&mut *tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "welfare command lost its cursor"
    );

    let receipt = WelfareApplicationReceipt {
        command_id: command.command_id.clone(),
        application_id: ResourceId::from_u64(application_id),
        program_version_id: command.program_version_id,
        status: WelfareApplicationStatusState::Active,
        application_game_day: scope.game_day,
        approval_game_day: scope.game_day,
        eligibility_at_application: public_conditions(&program, &planned.evaluation)?,
        payment,
        replayed: false,
    };
    write_game_command_receipt(
        &mut tx,
        GameCommandReceiptWrite {
            save_id: scope.save_id,
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_APPLY_WELFARE,
            payload_sha256: &fingerprint,
            market_world_id: scope.market_world_id,
            committed_cursor: GameCommandCursor {
                run_revision: scope.run_revision,
                state_revision: committed_state_revision,
                game_day: scope.game_day,
            },
            result: &receipt,
            ledger_transaction_id: None,
        },
    )
    .await?;
    let save = read_state(&mut tx, scope.save_id).await?;
    tx.commit().await?;
    Ok(LifeStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

pub(super) async fn ensure_welfare_evaluations_in_tx(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn WelfareRules,
    save_id: u64,
) -> Result<()> {
    let Some(scope) = read_welfare_scope_for_save(tx, save_id, false).await? else {
        return Ok(());
    };
    if !scope.has_character || scope.availability != "active" {
        return Ok(());
    }
    for program in load_welfare_programs(tx, rules, &scope).await? {
        let planned = plan_welfare_evaluation(
            tx,
            rules,
            &scope,
            &program,
            WelfarePlanningMode::InitialCurrentDay,
        )
        .await?;
        persist_welfare_evaluation(tx, &scope, &program, &planned).await?;
    }
    Ok(())
}

pub(super) async fn ensure_welfare_evaluations_for_target_day_in_tx(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn WelfareRules,
    save_id: u64,
    evaluation_game_day: u32,
) -> Result<()> {
    let Some(scope) = read_welfare_scope_for_save(tx, save_id, false).await? else {
        return Ok(());
    };
    ensure!(
        evaluation_game_day
            == scope
                .game_day
                .checked_add(1)
                .context("welfare target day overflowed")?,
        "welfare target planner day is not the next locked day"
    );
    if !scope.has_character || scope.availability != "active" {
        return Ok(());
    }
    for program in load_welfare_programs(tx, rules, &scope).await? {
        let planned = plan_welfare_evaluation(
            tx,
            rules,
            &scope,
            &program,
            WelfarePlanningMode::TargetDay {
                evaluation_game_day,
            },
        )
        .await?;
        persist_welfare_evaluation(tx, &scope, &program, &planned).await?;
    }
    Ok(())
}

pub(super) async fn read_active_welfare_applications_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<Vec<ActiveWelfareApplicationState>> {
    let rows: Vec<ActiveWelfareApplicationRow> = sqlx::query_as(
        "SELECT application.id AS application_id,
                application.program_version_id, program.program_key, program.display_name,
                application.status, application.application_game_day,
                application.approval_game_day, application.benefit_amount_krw AS benefit_krw,
                application.paid_krw, payment.id AS payment_id,
                payment.payment_no, payment.amount_krw AS payment_amount_krw,
                payment.due_game_day AS payment_due_game_day,
                payment.status AS payment_status
         FROM welfare_application AS application
         INNER JOIN welfare_program_version AS program
           ON program.id = application.program_version_id
         LEFT JOIN welfare_payment AS payment
           ON payment.save_id = application.save_id
          AND payment.run_revision = application.run_revision
          AND payment.application_id = application.id
          AND payment.status = 'pending'
         WHERE application.save_id = ? AND application.run_revision = ?
           AND application.status = 'active'
         ORDER BY application.id
         LIMIT 9",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() <= MAX_ACTIVE_WELFARE_APPLICATIONS,
        "active welfare applications exceeded the snapshot bound"
    );
    rows.into_iter().map(active_application_state).collect()
}

pub(super) fn validate_welfare_settlement_envelope(settlement: &ScheduledSettlement) -> Result<()> {
    ensure!(
        settlement.kind == SettlementKind::WelfareBenefitPayment
            && settlement.source.kind == SettlementSourceKind::WelfarePayment,
        "welfare settlement kind and source disagree"
    );
    let payload: WelfareSettlementPayload = serde_json::from_value(settlement.payload.clone())
        .context("welfare settlement payload is invalid")?;
    ensure!(
        payload.version == 1
            && payload.payment_no == 1
            && settlement.source.source_id == payload.welfare_payment_id.to_string()
            && settlement.source.occurrence == u64::from(payload.payment_no),
        "welfare settlement envelope disagrees with its payload"
    );
    Ok(())
}

pub(super) async fn settle_welfare_benefit_by_id_in_tx(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    save_id: u64,
    run_revision: u32,
    policy_set_id: u64,
    game_day: u32,
    settlement_id: u64,
) -> Result<()> {
    let envelope: (String,) = sqlx::query_as(
        "SELECT CAST(payload AS CHAR) FROM scheduled_settlement
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'pending'",
    )
    .bind(settlement_id)
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await?;
    let payload: WelfareSettlementPayload =
        serde_json::from_str(&envelope.0).context("welfare settlement payload is invalid")?;
    ensure!(payload.version == 1 && payload.payment_no == 1);

    let application: WelfareSettlementApplicationRow = sqlx::query_as(
        "SELECT id, status, benefit_amount_krw
         FROM welfare_application
         WHERE id = ? AND save_id = ? AND run_revision = ? FOR UPDATE",
    )
    .bind(payload.application_id.get())
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await?;
    let payment: WelfareSettlementPaymentRow = sqlx::query_as(
        "SELECT id, application_id, payment_no, due_game_day, amount_krw,
                status, scheduled_settlement_id
         FROM welfare_payment
         WHERE id = ? AND save_id = ? AND run_revision = ? FOR UPDATE",
    )
    .bind(payload.welfare_payment_id.get())
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await?;
    let settlement: (u32, String, String, String, u64) = sqlx::query_as(
        "SELECT due_game_day, kind, source_kind, source_id, occurrence
         FROM scheduled_settlement
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'pending'
         FOR UPDATE",
    )
    .bind(settlement_id)
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        application.status == "active"
            && payment.status == "pending"
            && payment.application_id == application.id
            && payment.payment_no == 1
            && payment.amount_krw == application.benefit_amount_krw
            && payment.due_game_day == game_day
            && payment.scheduled_settlement_id == Some(settlement_id)
            && settlement.0 == game_day
            && settlement.1 == "welfareBenefitPayment"
            && settlement.2 == "welfarePayment"
            && settlement.3 == payment.id.to_string()
            && settlement.4 == u64::from(payment.payment_no),
        "welfare payment authority disagrees with its settlement"
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
                kind: LedgerSourceKind::WelfareBenefitPayment,
                source_id: payment.id.to_string(),
            },
            game_day,
            description: "복지 급여 지급".to_owned(),
            postings: vec![
                LedgerPosting {
                    account_code: LedgerAccountCode::WelfareBenefitIncome,
                    financial_account_id: None,
                    amount_krw: payment
                        .amount_krw
                        .checked_neg()
                        .context("welfare benefit posting overflowed")?,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::Wallet,
                    financial_account_id: None,
                    amount_krw: payment.amount_krw,
                },
            ],
        })
        .context("welfare benefit ledger is invalid")?;
    let ledger_id = write_ledger_transaction(tx, &ledger).await?;
    let cash_update = sqlx::query(
        "UPDATE save SET cash_krw = cash_krw + ?
         WHERE id = ? AND run_revision = ? AND policy_set_id = ?",
    )
    .bind(payment.amount_krw)
    .bind(save_id)
    .bind(run_revision)
    .bind(policy_set_id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        cash_update.rows_affected() == 1,
        "welfare payment lost its save"
    );
    let payment_update = sqlx::query(
        "UPDATE welfare_payment
         SET status = 'paid', ledger_transaction_id = ?, paid_game_day = ?
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'pending'",
    )
    .bind(ledger_id)
    .bind(game_day)
    .bind(payment.id)
    .bind(save_id)
    .bind(run_revision)
    .execute(&mut **tx)
    .await?;
    ensure!(
        payment_update.rows_affected() == 1,
        "welfare payment lost its pending state"
    );
    let application_update = sqlx::query(
        "UPDATE welfare_application
         SET status = 'exhausted', paid_krw = benefit_amount_krw,
             terminal_game_day = ?, terminal_reason = 'benefitPaid'
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'active'",
    )
    .bind(game_day)
    .bind(application.id)
    .bind(save_id)
    .bind(run_revision)
    .execute(&mut **tx)
    .await?;
    ensure!(
        application_update.rows_affected() == 1,
        "welfare application lost its active state"
    );
    insert_application_transition(
        tx,
        WelfareApplicationTransitionInsert {
            save_id,
            run_revision,
            application_id: application.id,
            transition_no: 4,
            from_status: Some("active"),
            to_status: "exhausted",
            game_day,
            reason: "benefitPaid",
        },
    )
    .await?;
    let settlement_update = sqlx::query(
        "UPDATE scheduled_settlement
         SET status = 'settled', outcome = 'applied', settled_ledger_transaction_id = ?
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'pending'",
    )
    .bind(ledger_id)
    .bind(settlement_id)
    .bind(save_id)
    .bind(run_revision)
    .execute(&mut **tx)
    .await?;
    ensure!(
        settlement_update.rows_affected() == 1,
        "welfare settlement lost its pending state"
    );
    Ok(())
}

pub(super) async fn close_welfare_for_new_run_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
) -> Result<()> {
    let application_ids: Vec<u64> = sqlx::query_scalar(
        "SELECT id FROM welfare_application
         WHERE save_id = ? AND run_revision = ? AND status = 'active'
         ORDER BY id FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    for application_id in application_ids {
        let payment_update = sqlx::query(
            "UPDATE welfare_payment
             SET status = 'cancelled', cancelled_game_day = ?, cancellation_reason = 'newRun'
             WHERE save_id = ? AND run_revision = ? AND application_id = ?
               AND status = 'pending'",
        )
        .bind(game_day)
        .bind(save_id)
        .bind(run_revision)
        .bind(application_id)
        .execute(&mut **tx)
        .await?;
        ensure!(
            payment_update.rows_affected() == 1,
            "active welfare application has no pending payment"
        );
        let application_update = sqlx::query(
            "UPDATE welfare_application
             SET status = 'terminated', terminal_game_day = ?, terminal_reason = 'newRun'
             WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'active'",
        )
        .bind(game_day)
        .bind(application_id)
        .bind(save_id)
        .bind(run_revision)
        .execute(&mut **tx)
        .await?;
        ensure!(
            application_update.rows_affected() == 1,
            "active welfare application was not terminated"
        );
        insert_application_transition(
            tx,
            WelfareApplicationTransitionInsert {
                save_id,
                run_revision,
                application_id,
                transition_no: 4,
                from_status: Some("active"),
                to_status: "terminated",
                game_day,
                reason: "newRun",
            },
        )
        .await?;
    }
    Ok(())
}

async fn read_welfare_scope_for_user(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
    lock: bool,
) -> Result<Option<WelfareScopeRow>> {
    let row = if lock {
        sqlx::query_as(
            "SELECT save.id AS save_id, save.market_world_id,
                save.market_world_product_bundle_id, save.run_revision,
                save.state_revision, save.game_day, bundle.life_catalog_set_id,
                catalog.welfare_component_version_id, component.availability,
                EXISTS(SELECT 1 FROM `character` WHERE `character`.save_id = save.id)
                    AS has_character
         FROM save
         INNER JOIN run_rule_bundle AS bundle
           ON bundle.save_id = save.id AND bundle.run_revision = save.run_revision
         INNER JOIN life_catalog_set AS catalog ON catalog.id = bundle.life_catalog_set_id
         INNER JOIN life_component_version AS component
           ON component.id = catalog.welfare_component_version_id
         WHERE save.user_id = ? FOR UPDATE",
        )
        .bind(user_id)
        .fetch_optional(&mut **tx)
        .await?
    } else {
        sqlx::query_as(
            "SELECT save.id AS save_id, save.market_world_id,
                save.market_world_product_bundle_id, save.run_revision,
                save.state_revision, save.game_day, bundle.life_catalog_set_id,
                catalog.welfare_component_version_id, component.availability,
                EXISTS(SELECT 1 FROM `character` WHERE `character`.save_id = save.id)
                    AS has_character
         FROM save
         INNER JOIN run_rule_bundle AS bundle
           ON bundle.save_id = save.id AND bundle.run_revision = save.run_revision
         INNER JOIN life_catalog_set AS catalog ON catalog.id = bundle.life_catalog_set_id
         INNER JOIN life_component_version AS component
           ON component.id = catalog.welfare_component_version_id
         WHERE save.user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(&mut **tx)
        .await?
    };
    Ok(row)
}

async fn read_welfare_scope_for_save(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    lock: bool,
) -> Result<Option<WelfareScopeRow>> {
    let row = if lock {
        sqlx::query_as(
            "SELECT save.id AS save_id, save.market_world_id,
                save.market_world_product_bundle_id, save.run_revision,
                save.state_revision, save.game_day, bundle.life_catalog_set_id,
                catalog.welfare_component_version_id, component.availability,
                EXISTS(SELECT 1 FROM `character` WHERE `character`.save_id = save.id)
                    AS has_character
         FROM save
         INNER JOIN run_rule_bundle AS bundle
           ON bundle.save_id = save.id AND bundle.run_revision = save.run_revision
         INNER JOIN life_catalog_set AS catalog ON catalog.id = bundle.life_catalog_set_id
         INNER JOIN life_component_version AS component
           ON component.id = catalog.welfare_component_version_id
         WHERE save.id = ? FOR UPDATE",
        )
        .bind(save_id)
        .fetch_optional(&mut **tx)
        .await?
    } else {
        sqlx::query_as(
            "SELECT save.id AS save_id, save.market_world_id,
                save.market_world_product_bundle_id, save.run_revision,
                save.state_revision, save.game_day, bundle.life_catalog_set_id,
                catalog.welfare_component_version_id, component.availability,
                EXISTS(SELECT 1 FROM `character` WHERE `character`.save_id = save.id)
                    AS has_character
         FROM save
         INNER JOIN run_rule_bundle AS bundle
           ON bundle.save_id = save.id AND bundle.run_revision = save.run_revision
         INNER JOIN life_catalog_set AS catalog ON catalog.id = bundle.life_catalog_set_id
         INNER JOIN life_component_version AS component
           ON component.id = catalog.welfare_component_version_id
         WHERE save.id = ?",
        )
        .bind(save_id)
        .fetch_optional(&mut **tx)
        .await?
    };
    Ok(row)
}

async fn load_welfare_programs(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn WelfareRules,
    scope: &WelfareScopeRow,
) -> Result<Vec<LoadedWelfareProgram>> {
    validate_welfare_fact_registry(tx, rules, scope.welfare_component_version_id).await?;
    let rows: Vec<WelfareProgramRow> = sqlx::query_as(
        "SELECT program.id, program.schema_version, program.program_key,
                program.display_name, program.purpose, program.ranked_availability,
                program.application_kind, program.application_period_kind,
                program.application_start_game_day, program.application_end_game_day,
                program.duplicate_group_key, program.duplicate_scope,
                program.maximum_approved_per_group, program.reassessment_basis,
                program.ast_node_count, program.ast_max_depth,
                CAST(program.eligibility_ast AS CHAR) AS eligibility_ast_json,
                CAST(program.benefit_formula AS CHAR) AS benefit_formula_json,
                CAST(program.payment_schedule AS CHAR) AS payment_schedule_json
         FROM welfare_program_version AS program
         WHERE program.life_component_version_id = ?
         ORDER BY program.program_key, program.id
         LIMIT 17",
    )
    .bind(scope.welfare_component_version_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() <= MAX_WELFARE_PROGRAMS,
        "welfare program catalog exceeded its bound"
    );
    let mut loaded = Vec::with_capacity(rows.len());
    for row in rows {
        let conditions: Vec<WelfareConditionRow> = sqlx::query_as(
            "SELECT id, condition_order, condition_code, public_label,
                    node_count, max_depth,
                    CAST(expression_ast AS CHAR) AS expression_ast_json
             FROM welfare_program_condition
             WHERE program_version_id = ? ORDER BY condition_order",
        )
        .bind(row.id)
        .fetch_all(&mut **tx)
        .await?;
        let constant_rows: Vec<WelfareConstantRow> = sqlx::query_as(
            "SELECT constant_order, constant_key, value_type, unit, enum_schema_key,
                    boolean_value, integer_value, string_value, date_value
             FROM welfare_program_constant
             WHERE program_version_id = ? ORDER BY constant_order",
        )
        .bind(row.id)
        .fetch_all(&mut **tx)
        .await?;
        let trigger_rows: Vec<WelfareTriggerRow> = sqlx::query_as(
            "SELECT trigger_order, source_kind
             FROM welfare_reassessment_trigger
             WHERE program_version_id = ? ORDER BY trigger_order",
        )
        .bind(row.id)
        .fetch_all(&mut **tx)
        .await?;

        let constants = parse_welfare_constants(rules, constant_rows)?;
        let parsed_conditions = parse_welfare_conditions(rules, &conditions)?;
        let condition_depths = parsed_conditions
            .iter()
            .zip(&conditions)
            .map(|(condition, stored)| (condition.code.clone(), stored.max_depth))
            .collect::<BTreeMap<_, _>>();
        let eligibility = parse_welfare_eligibility(&row.eligibility_ast_json, &condition_depths)?;
        let (amount_constant_key, benefit_krw) =
            parse_welfare_benefit(&row.benefit_formula_json, &constants)?;
        let payment_delay_game_days = parse_welfare_payment_schedule(&row.payment_schedule_json)?;
        let reassessment_triggers = parse_welfare_triggers(trigger_rows)?;
        let condition_node_count = conditions.iter().try_fold(0_u16, |total, condition| {
            total.checked_add(condition.node_count)
        });
        ensure!(
            row.application_kind == "manual"
                && row.application_period_kind == "always"
                && row.application_start_game_day.is_none()
                && row.application_end_game_day.is_none()
                && row.duplicate_scope == "run"
                && row.maximum_approved_per_group == 1
                && row.reassessment_basis == "eligibilityAtApplication"
                && condition_node_count
                    .and_then(|count| count.checked_add(eligibility.operator_count))
                    == Some(row.ast_node_count)
                && eligibility.expanded_depth == row.ast_max_depth,
            "sealed welfare program metadata is invalid"
        );
        let definition = WelfareProgramDefinition {
            schema_version: row.schema_version,
            program_version_id: ResourceId::from_u64(row.id),
            program_key: row.program_key.clone(),
            purpose: parse_welfare_program_purpose(&row.purpose)?,
            ranked_availability: parse_welfare_ranked_availability(&row.ranked_availability)?,
            duplicate_group_key: row.duplicate_group_key.clone(),
            constants,
            conditions: parsed_conditions,
            eligibility_root: eligibility.expression,
            benefit: WelfareBenefitDefinition {
                amount_constant_key,
                payment_delay_days: payment_delay_game_days,
            },
            reassessment_triggers,
        };
        rules
            .validate_program(&definition)
            .context("stored welfare program is invalid")?;
        loaded.push(LoadedWelfareProgram {
            definition,
            display_name: row.display_name,
            duplicate_group_key: row.duplicate_group_key,
            benefit_krw,
            payment_delay_game_days,
            conditions,
        });
    }
    Ok(loaded)
}

async fn validate_welfare_fact_registry(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn WelfareRules,
    component_version_id: u64,
) -> Result<()> {
    let rows: Vec<WelfareFactDefinitionRow> = sqlx::query_as(
        "SELECT fact_order, fact_key, value_type, unit, enum_schema_key,
                window_kind, minimum_window_days, maximum_window_days,
                collection_bound, source_schema_version, source_kind
         FROM welfare_fact_definition
         WHERE life_component_version_id = ? ORDER BY fact_order",
    )
    .bind(component_version_id)
    .fetch_all(&mut **tx)
    .await?;
    let registry = rules.fact_registry();
    ensure!(
        rows.len() == registry.facts.len(),
        "stored welfare fact registry cardinality drifted"
    );
    for (index, (stored, expected)) in rows.iter().zip(&registry.facts).enumerate() {
        let expected_order = u8::try_from(index + 1).context("welfare fact order overflowed")?;
        let (value_type, unit, enum_schema_key) = stored_value_type(&expected.value_type);
        let window_matches = match &expected.window {
            crate::life::WelfareWindowConstraint::CurrentDay => {
                stored.window_kind == "currentGameDay"
                    && stored.minimum_window_days.is_none()
                    && stored.maximum_window_days.is_none()
            }
            crate::life::WelfareWindowConstraint::PreviousClosedDays { minimum, maximum } => {
                stored.window_kind == "previousClosedDays"
                    && stored.minimum_window_days == Some(*minimum)
                    && stored.maximum_window_days == Some(*maximum)
            }
            crate::life::WelfareWindowConstraint::PriorClose => {
                stored.window_kind == "priorClose"
                    && stored.minimum_window_days.is_none()
                    && stored.maximum_window_days.is_none()
            }
        };
        ensure!(
            stored.fact_order == expected_order
                && stored.fact_key == expected.path
                && stored.value_type == value_type
                && stored.unit == unit
                && stored.enum_schema_key.as_deref() == enum_schema_key.as_deref()
                && window_matches
                && stored.collection_bound
                    == expected_fact_collection_bound(expected.path.as_str())
                && stored.source_schema_version == registry.schema_version
                && parse_welfare_fact_source(&stored.source_kind)? == expected.source,
            "stored welfare fact registry disagrees with the rule engine"
        );
    }
    Ok(())
}

fn expected_fact_collection_bound(path: &str) -> Option<u8> {
    match path {
        "household.memberCount"
        | "household.dependentCount"
        | "income.periodTotal"
        | "asset.policyValuation"
        | "debt.policyBalance" => Some(32),
        _ => None,
    }
}

fn stored_value_type(value_type: &WelfareValueType) -> (String, String, Option<String>) {
    match value_type {
        WelfareValueType::Boolean => ("boolean".to_owned(), "boolean".to_owned(), None),
        WelfareValueType::Integer => ("integer".to_owned(), "integer".to_owned(), None),
        WelfareValueType::MoneyKrw => ("moneyKrw".to_owned(), "krw".to_owned(), None),
        WelfareValueType::Count => ("count".to_owned(), "count".to_owned(), None),
        WelfareValueType::AgeYears => ("ageYears".to_owned(), "years".to_owned(), None),
        WelfareValueType::Date => ("date".to_owned(), "date".to_owned(), None),
        WelfareValueType::String => ("string".to_owned(), "string".to_owned(), None),
        WelfareValueType::Enum(schema_key) => (
            "enum".to_owned(),
            "enum".to_owned(),
            Some(schema_key.clone()),
        ),
    }
}

fn parse_welfare_constants(
    rules: &dyn WelfareRules,
    rows: Vec<WelfareConstantRow>,
) -> Result<Vec<WelfareProgramConstant>> {
    let mut keys = BTreeSet::new();
    rows.into_iter()
        .enumerate()
        .map(|(index, row)| {
            ensure!(
                row.constant_order
                    == u8::try_from(index + 1).context("welfare constant order overflowed")?
                    && keys.insert(row.constant_key.clone()),
                "stored welfare constant order or key is invalid"
            );
            let value = match row.value_type.as_str() {
                "boolean" => {
                    ensure!(
                        row.unit == "boolean"
                            && row.enum_schema_key.is_none()
                            && row.integer_value.is_none()
                            && row.string_value.is_none()
                            && row.date_value.is_none()
                    );
                    WelfareValue::Boolean(
                        row.boolean_value
                            .context("welfare boolean constant is missing")?,
                    )
                }
                "integer" => {
                    ensure!(row.unit == "integer" && row.enum_schema_key.is_none());
                    WelfareValue::Integer(required_integer_constant(&row)?)
                }
                "moneyKrw" => {
                    ensure!(row.unit == "krw" && row.enum_schema_key.is_none());
                    WelfareValue::MoneyKrw(required_integer_constant(&row)?)
                }
                "count" => {
                    ensure!(row.unit == "count" && row.enum_schema_key.is_none());
                    WelfareValue::Count(required_integer_constant(&row)?)
                }
                "ageYears" => {
                    ensure!(row.unit == "years" && row.enum_schema_key.is_none());
                    WelfareValue::AgeYears(required_integer_constant(&row)?)
                }
                "string" => {
                    ensure!(
                        row.unit == "string"
                            && row.enum_schema_key.is_none()
                            && row.boolean_value.is_none()
                            && row.integer_value.is_none()
                            && row.date_value.is_none()
                    );
                    WelfareValue::String(
                        row.string_value
                            .clone()
                            .context("welfare string constant is missing")?,
                    )
                }
                "date" => {
                    ensure!(
                        row.unit == "date"
                            && row.enum_schema_key.is_none()
                            && row.boolean_value.is_none()
                            && row.integer_value.is_none()
                            && row.string_value.is_none()
                    );
                    WelfareValue::Date(row.date_value.context("welfare date constant is missing")?)
                }
                "enum" => {
                    ensure!(
                        row.unit == "enum"
                            && row.boolean_value.is_none()
                            && row.integer_value.is_none()
                            && row.date_value.is_none()
                    );
                    let value = row
                        .string_value
                        .clone()
                        .context("welfare enum constant is missing")?;
                    let schema_key = row
                        .enum_schema_key
                        .clone()
                        .context("welfare enum constant schema is missing")?;
                    let schema = rules
                        .fact_registry()
                        .enums
                        .iter()
                        .find(|schema| schema.schema_key == schema_key)
                        .context("welfare enum constant schema is unknown")?;
                    ensure!(
                        schema.values.iter().any(|candidate| candidate == &value),
                        "welfare enum constant value is invalid"
                    );
                    WelfareValue::Enum(WelfareEnumValue { schema_key, value })
                }
                _ => bail!("stored welfare constant type is invalid"),
            };
            Ok(WelfareProgramConstant {
                key: row.constant_key,
                value,
            })
        })
        .collect()
}

fn required_integer_constant(row: &WelfareConstantRow) -> Result<i64> {
    ensure!(row.boolean_value.is_none() && row.string_value.is_none() && row.date_value.is_none());
    row.integer_value
        .context("welfare numeric constant is missing")
}

fn parse_welfare_conditions(
    rules: &dyn WelfareRules,
    rows: &[WelfareConditionRow],
) -> Result<Vec<WelfareProgramCondition>> {
    let mut codes = BTreeSet::new();
    rows.iter()
        .enumerate()
        .map(|(index, row)| {
            ensure!(
                row.condition_order
                    == u8::try_from(index + 1).context("welfare condition order overflowed")?
                    && codes.insert(row.condition_code.clone()),
                "stored welfare condition order or code is invalid"
            );
            let value: JsonValue = serde_json::from_str(&row.expression_ast_json)
                .context("stored welfare condition AST is invalid JSON")?;
            let parsed = parse_welfare_expression(rules, value)?;
            ensure!(
                parsed.node_count == row.node_count && parsed.max_depth == row.max_depth,
                "stored welfare condition AST metrics drifted"
            );
            Ok(WelfareProgramCondition {
                code: row.condition_code.clone(),
                expression: parsed.expression,
            })
        })
        .collect()
}

fn parse_welfare_expression(
    rules: &dyn WelfareRules,
    value: JsonValue,
) -> Result<ParsedWelfareExpression> {
    let mut object = into_json_object(value, "welfare expression")?;
    let kind = take_json_string(&mut object, "kind")?;
    match kind.as_str() {
        "all" | "any" => {
            let values = take_json_array(&mut object, "children")?;
            finish_json_object(&object, "welfare logical expression")?;
            let children = values
                .into_iter()
                .map(|child| parse_welfare_expression(rules, child))
                .collect::<Result<Vec<_>>>()?;
            let (node_count, max_depth) = expression_parent_metrics(&children)?;
            let expressions = children.into_iter().map(|child| child.expression).collect();
            let expression = if kind == "all" {
                WelfareExpression::All {
                    children: expressions,
                }
            } else {
                WelfareExpression::Any {
                    children: expressions,
                }
            };
            Ok(ParsedWelfareExpression {
                expression,
                node_count,
                max_depth,
            })
        }
        "not" => {
            let child = parse_welfare_expression(rules, take_json_value(&mut object, "child")?)?;
            finish_json_object(&object, "welfare not expression")?;
            Ok(ParsedWelfareExpression {
                node_count: child
                    .node_count
                    .checked_add(1)
                    .context("welfare AST node count overflowed")?,
                max_depth: child
                    .max_depth
                    .checked_add(1)
                    .context("welfare AST depth overflowed")?,
                expression: WelfareExpression::Not {
                    child: Box::new(child.expression),
                },
            })
        }
        "eq" | "lt" | "lte" | "gt" | "gte" => {
            let left = parse_welfare_expression(rules, take_json_value(&mut object, "left")?)?;
            let right = parse_welfare_expression(rules, take_json_value(&mut object, "right")?)?;
            finish_json_object(&object, "welfare comparison expression")?;
            let node_count = left
                .node_count
                .checked_add(right.node_count)
                .and_then(|count| count.checked_add(1))
                .context("welfare AST node count overflowed")?;
            let max_depth = left
                .max_depth
                .max(right.max_depth)
                .checked_add(1)
                .context("welfare AST depth overflowed")?;
            let left = Box::new(left.expression);
            let right = Box::new(right.expression);
            let expression = match kind.as_str() {
                "eq" => WelfareExpression::Eq { left, right },
                "lt" => WelfareExpression::Lt { left, right },
                "lte" => WelfareExpression::Lte { left, right },
                "gt" => WelfareExpression::Gt { left, right },
                "gte" => WelfareExpression::Gte { left, right },
                _ => unreachable!(),
            };
            Ok(ParsedWelfareExpression {
                expression,
                node_count,
                max_depth,
            })
        }
        "in" => {
            let value = parse_welfare_expression(rules, take_json_value(&mut object, "value")?)?;
            let literal_values = take_json_array(&mut object, "literals")?;
            finish_json_object(&object, "welfare in expression")?;
            let parsed_literals = literal_values
                .into_iter()
                .map(|literal| parse_welfare_expression(rules, literal))
                .collect::<Result<Vec<_>>>()?;
            let mut literals = Vec::with_capacity(parsed_literals.len());
            let mut node_count = value
                .node_count
                .checked_add(1)
                .context("welfare AST node count overflowed")?;
            let mut max_child_depth = value.max_depth;
            for literal in parsed_literals {
                node_count = node_count
                    .checked_add(literal.node_count)
                    .context("welfare AST node count overflowed")?;
                max_child_depth = max_child_depth.max(literal.max_depth);
                let WelfareExpression::Literal { value } = literal.expression else {
                    bail!("welfare in-list item is not a literal");
                };
                literals.push(value);
            }
            Ok(ParsedWelfareExpression {
                expression: WelfareExpression::In {
                    value: Box::new(value.expression),
                    literals,
                },
                node_count,
                max_depth: max_child_depth
                    .checked_add(1)
                    .context("welfare AST depth overflowed")?,
            })
        }
        "between" => {
            let value = parse_welfare_expression(rules, take_json_value(&mut object, "value")?)?;
            let lower = parse_welfare_expression(rules, take_json_value(&mut object, "lower")?)?;
            let upper = parse_welfare_expression(rules, take_json_value(&mut object, "upper")?)?;
            finish_json_object(&object, "welfare between expression")?;
            let node_count = value
                .node_count
                .checked_add(lower.node_count)
                .and_then(|count| count.checked_add(upper.node_count))
                .and_then(|count| count.checked_add(1))
                .context("welfare AST node count overflowed")?;
            let max_depth = value
                .max_depth
                .max(lower.max_depth)
                .max(upper.max_depth)
                .checked_add(1)
                .context("welfare AST depth overflowed")?;
            Ok(ParsedWelfareExpression {
                expression: WelfareExpression::Between {
                    value: Box::new(value.expression),
                    lower: Box::new(lower.expression),
                    upper: Box::new(upper.expression),
                },
                node_count,
                max_depth,
            })
        }
        "fact" => {
            let path = take_json_string(&mut object, "path")?;
            let unit = take_json_string(&mut object, "unit")?;
            let window = parse_welfare_window(take_json_value(&mut object, "window")?)?;
            finish_json_object(&object, "welfare fact expression")?;
            let definition = rules
                .fact_registry()
                .facts
                .iter()
                .find(|definition| definition.path == path)
                .context("welfare AST references an unknown fact")?;
            let (_, expected_unit, _) = stored_value_type(&definition.value_type);
            ensure!(unit == expected_unit, "welfare fact unit is invalid");
            Ok(ParsedWelfareExpression {
                expression: WelfareExpression::Fact { path, window },
                node_count: 1,
                max_depth: 1,
            })
        }
        "constant" => {
            let key = take_json_string(&mut object, "key")?;
            finish_json_object(&object, "welfare constant expression")?;
            Ok(ParsedWelfareExpression {
                expression: WelfareExpression::Constant { key },
                node_count: 1,
                max_depth: 1,
            })
        }
        "literal" => {
            let literal = parse_welfare_literal(&mut object)?;
            finish_json_object(&object, "welfare literal expression")?;
            Ok(ParsedWelfareExpression {
                expression: WelfareExpression::Literal { value: literal },
                node_count: 1,
                max_depth: 1,
            })
        }
        "sum" | "count" | "exists" => {
            let collection = take_json_string(&mut object, "collection")?;
            let unit = take_json_string(&mut object, "unit")?;
            let window = parse_welfare_window(take_json_value(&mut object, "window")?)?;
            finish_json_object(&object, "welfare collection expression")?;
            let definition = rules
                .fact_registry()
                .collections
                .iter()
                .find(|definition| definition.key == collection)
                .context("welfare AST references an unknown collection")?;
            let (_, expected_unit, _) = stored_value_type(&definition.item_type);
            ensure!(unit == expected_unit, "welfare collection unit is invalid");
            let expression = match kind.as_str() {
                "sum" => WelfareExpression::Sum { collection, window },
                "count" => WelfareExpression::Count { collection, window },
                "exists" => WelfareExpression::Exists { collection, window },
                _ => unreachable!(),
            };
            Ok(ParsedWelfareExpression {
                expression,
                node_count: 1,
                max_depth: 1,
            })
        }
        _ => bail!("stored welfare expression kind is invalid"),
    }
}

fn expression_parent_metrics(children: &[ParsedWelfareExpression]) -> Result<(u16, u8)> {
    let node_count = children
        .iter()
        .try_fold(1_u16, |total, child| total.checked_add(child.node_count));
    let max_child_depth = children
        .iter()
        .map(|child| child.max_depth)
        .max()
        .unwrap_or(0);
    Ok((
        node_count.context("welfare AST node count overflowed")?,
        max_child_depth
            .checked_add(1)
            .context("welfare AST depth overflowed")?,
    ))
}

fn parse_welfare_window(value: JsonValue) -> Result<WelfareWindowSpec> {
    let mut object = into_json_object(value, "welfare window")?;
    let kind = take_json_string(&mut object, "kind")?;
    let window = match kind.as_str() {
        "currentGameDay" => WelfareWindowSpec::CurrentDay,
        "priorClose" => WelfareWindowSpec::PriorClose,
        "previousClosedDays" => {
            let days = take_json_value(&mut object, "days")?;
            let days = if let Some(days) = days.as_u64() {
                WelfareWindowDays::Literal {
                    days: u16::try_from(days).context("welfare window days overflowed")?,
                }
            } else {
                let mut days = into_json_object(days, "welfare window days")?;
                let kind = take_json_string(&mut days, "kind")?;
                let parsed = match kind.as_str() {
                    "constant" => WelfareWindowDays::Constant {
                        key: take_json_string(&mut days, "key")?,
                    },
                    "literal" => WelfareWindowDays::Literal {
                        days: u16::try_from(take_json_u64(&mut days, "value")?)
                            .context("welfare window days overflowed")?,
                    },
                    _ => bail!("welfare window-day expression is invalid"),
                };
                finish_json_object(&days, "welfare window days")?;
                parsed
            };
            WelfareWindowSpec::PreviousClosedDays { days }
        }
        _ => bail!("stored welfare window kind is invalid"),
    };
    finish_json_object(&object, "welfare window")?;
    Ok(window)
}

fn parse_welfare_literal(object: &mut JsonMap<String, JsonValue>) -> Result<WelfareValue> {
    let value_type = take_json_string(object, "valueType")?;
    let unit = take_json_string(object, "unit")?;
    let value = take_json_value(object, "value")?;
    match value_type.as_str() {
        "boolean" => {
            ensure!(unit == "boolean");
            Ok(WelfareValue::Boolean(
                value
                    .as_bool()
                    .context("welfare boolean literal is invalid")?,
            ))
        }
        "integer" => {
            ensure!(unit == "integer");
            Ok(WelfareValue::Integer(json_i64(&value)?))
        }
        "moneyKrw" => {
            ensure!(unit == "krw");
            Ok(WelfareValue::MoneyKrw(json_i64(&value)?))
        }
        "count" => {
            ensure!(unit == "count");
            Ok(WelfareValue::Count(json_i64(&value)?))
        }
        "ageYears" => {
            ensure!(unit == "years");
            Ok(WelfareValue::AgeYears(json_i64(&value)?))
        }
        "date" => {
            ensure!(unit == "date");
            Ok(WelfareValue::Date(parse_storage_date(
                value.as_str().context("welfare date literal is invalid")?,
            )?))
        }
        "string" => {
            ensure!(unit == "string");
            Ok(WelfareValue::String(
                value
                    .as_str()
                    .context("welfare string literal is invalid")?
                    .to_owned(),
            ))
        }
        "enum" => {
            ensure!(unit == "enum");
            Ok(WelfareValue::Enum(WelfareEnumValue {
                schema_key: take_json_string(object, "schemaKey")?,
                value: value
                    .as_str()
                    .context("welfare enum literal is invalid")?
                    .to_owned(),
            }))
        }
        _ => bail!("stored welfare literal type is invalid"),
    }
}

fn parse_storage_date(raw: &str) -> Result<time::Date> {
    let mut parts = raw.split('-');
    let year: i32 = parts
        .next()
        .context("welfare date year is missing")?
        .parse()
        .context("welfare date year is invalid")?;
    let month: u8 = parts
        .next()
        .context("welfare date month is missing")?
        .parse()
        .context("welfare date month is invalid")?;
    let day: u8 = parts
        .next()
        .context("welfare date day is missing")?
        .parse()
        .context("welfare date day is invalid")?;
    ensure!(parts.next().is_none(), "welfare date has extra components");
    time::Date::from_calendar_date(
        year,
        time::Month::try_from(month).context("welfare date month is out of range")?,
        day,
    )
    .context("welfare date is out of range")
}

fn parse_welfare_eligibility(
    raw: &str,
    condition_depths: &BTreeMap<String, u8>,
) -> Result<ParsedWelfareEligibility> {
    let value = serde_json::from_str(raw).context("stored welfare eligibility AST is invalid")?;
    parse_welfare_eligibility_value(value, condition_depths, true)
}

fn parse_welfare_eligibility_value(
    value: JsonValue,
    condition_depths: &BTreeMap<String, u8>,
    root: bool,
) -> Result<ParsedWelfareEligibility> {
    let mut object = into_json_object(value, "welfare eligibility")?;
    if root {
        ensure!(
            take_json_u64(&mut object, "version")? == 1,
            "welfare eligibility version is invalid"
        );
    }
    let kind = take_json_string(&mut object, "kind")?;
    match kind.as_str() {
        "all" | "any" => {
            let children = if object.contains_key("conditionCodes") {
                ensure!(
                    !object.contains_key("children"),
                    "welfare eligibility has two child representations"
                );
                take_json_array(&mut object, "conditionCodes")?
                    .into_iter()
                    .map(|code| {
                        let code = code
                            .as_str()
                            .context("welfare eligibility condition code is invalid")?
                            .to_owned();
                        let depth = *condition_depths
                            .get(&code)
                            .context("welfare eligibility references an unknown condition")?;
                        Ok(ParsedWelfareEligibility {
                            expression: WelfareEligibilityExpression::Condition { code },
                            operator_count: 0,
                            expanded_depth: depth,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?
            } else {
                take_json_array(&mut object, "children")?
                    .into_iter()
                    .map(|child| parse_welfare_eligibility_value(child, condition_depths, false))
                    .collect::<Result<Vec<_>>>()?
            };
            finish_json_object(&object, "welfare eligibility logical expression")?;
            let operator_count = children.iter().try_fold(1_u16, |total, child| {
                total.checked_add(child.operator_count)
            });
            let expanded_depth = children
                .iter()
                .map(|child| child.expanded_depth)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .context("welfare eligibility depth overflowed")?;
            let expressions = children.into_iter().map(|child| child.expression).collect();
            let expression = if kind == "all" {
                WelfareEligibilityExpression::All {
                    children: expressions,
                }
            } else {
                WelfareEligibilityExpression::Any {
                    children: expressions,
                }
            };
            Ok(ParsedWelfareEligibility {
                expression,
                operator_count: operator_count
                    .context("welfare eligibility node count overflowed")?,
                expanded_depth,
            })
        }
        "not" => {
            let child = parse_welfare_eligibility_value(
                take_json_value(&mut object, "child")?,
                condition_depths,
                false,
            )?;
            finish_json_object(&object, "welfare eligibility not expression")?;
            Ok(ParsedWelfareEligibility {
                expression: WelfareEligibilityExpression::Not {
                    child: Box::new(child.expression),
                },
                operator_count: child
                    .operator_count
                    .checked_add(1)
                    .context("welfare eligibility node count overflowed")?,
                expanded_depth: child
                    .expanded_depth
                    .checked_add(1)
                    .context("welfare eligibility depth overflowed")?,
            })
        }
        "condition" => {
            let code = if object.contains_key("code") {
                take_json_string(&mut object, "code")?
            } else {
                take_json_string(&mut object, "conditionCode")?
            };
            finish_json_object(&object, "welfare eligibility condition")?;
            let expanded_depth = *condition_depths
                .get(&code)
                .context("welfare eligibility references an unknown condition")?;
            Ok(ParsedWelfareEligibility {
                expression: WelfareEligibilityExpression::Condition { code },
                operator_count: 0,
                expanded_depth,
            })
        }
        _ => bail!("stored welfare eligibility kind is invalid"),
    }
}

fn parse_welfare_benefit(raw: &str, constants: &[WelfareProgramConstant]) -> Result<(String, i64)> {
    let value: JsonValue =
        serde_json::from_str(raw).context("stored welfare benefit is invalid")?;
    let mut object = into_json_object(value, "welfare benefit")?;
    ensure!(
        take_json_u64(&mut object, "version")? == 1
            && take_json_string(&mut object, "kind")? == "fixed",
        "welfare benefit header is invalid"
    );
    let mut amount = into_json_object(take_json_value(&mut object, "amount")?, "welfare amount")?;
    ensure!(
        take_json_string(&mut amount, "kind")? == "constant"
            && take_json_string(&mut amount, "unit")? == "krw",
        "welfare benefit amount is invalid"
    );
    let key = take_json_string(&mut amount, "key")?;
    finish_json_object(&amount, "welfare amount")?;
    finish_json_object(&object, "welfare benefit")?;
    let benefit_krw = constants
        .iter()
        .find(|constant| constant.key == key)
        .and_then(|constant| match constant.value {
            WelfareValue::MoneyKrw(value) => Some(value),
            _ => None,
        })
        .context("welfare benefit constant is missing or has the wrong type")?;
    ensure!(benefit_krw > 0, "welfare benefit must be positive");
    Ok((key, benefit_krw))
}

fn parse_welfare_payment_schedule(raw: &str) -> Result<u16> {
    let value: JsonValue =
        serde_json::from_str(raw).context("stored welfare payment schedule is invalid")?;
    let mut object = into_json_object(value, "welfare payment schedule")?;
    ensure!(
        take_json_u64(&mut object, "version")? == 1
            && take_json_string(&mut object, "kind")? == "once",
        "welfare payment schedule header is invalid"
    );
    let days = u16::try_from(take_json_u64(&mut object, "delayGameDays")?)
        .context("welfare payment delay overflowed")?;
    finish_json_object(&object, "welfare payment schedule")?;
    Ok(days)
}

fn parse_welfare_triggers(rows: Vec<WelfareTriggerRow>) -> Result<Vec<WelfareFactSource>> {
    rows.into_iter()
        .enumerate()
        .map(|(index, row)| {
            ensure!(
                row.trigger_order
                    == u8::try_from(index + 1).context("welfare trigger order overflowed")?,
                "stored welfare trigger order is invalid"
            );
            parse_welfare_fact_source(&row.source_kind)
        })
        .collect()
}

fn parse_welfare_fact_source(raw: &str) -> Result<WelfareFactSource> {
    match raw {
        "gameDay" => Ok(WelfareFactSource::GameDay),
        "household" => Ok(WelfareFactSource::Household),
        "residence" => Ok(WelfareFactSource::Residence),
        "employment" => Ok(WelfareFactSource::Employment),
        "military" => Ok(WelfareFactSource::Military),
        "income" => Ok(WelfareFactSource::Income),
        "asset" => Ok(WelfareFactSource::Asset),
        "debt" => Ok(WelfareFactSource::Debt),
        _ => bail!("stored welfare trigger source is invalid"),
    }
}

fn parse_welfare_program_purpose(raw: &str) -> Result<WelfareProgramPurpose> {
    match raw {
        "gameBalance" => Ok(WelfareProgramPurpose::GameBalance),
        "realPolicyReference" => Ok(WelfareProgramPurpose::RealPolicyReference),
        _ => bail!("stored welfare purpose is invalid"),
    }
}

fn parse_welfare_ranked_availability(raw: &str) -> Result<WelfareRankedAvailability> {
    match raw {
        "unrankedOnly" => Ok(WelfareRankedAvailability::UnrankedOnly),
        "rankedAndUnranked" => Ok(WelfareRankedAvailability::RankedAndUnranked),
        _ => bail!("stored welfare ranked availability is invalid"),
    }
}

fn into_json_object(value: JsonValue, context: &str) -> Result<JsonMap<String, JsonValue>> {
    match value {
        JsonValue::Object(object) => Ok(object),
        _ => bail!("{context} must be an object"),
    }
}

fn take_json_value(object: &mut JsonMap<String, JsonValue>, key: &str) -> Result<JsonValue> {
    object
        .remove(key)
        .with_context(|| format!("welfare JSON field `{key}` is missing"))
}

fn take_json_string(object: &mut JsonMap<String, JsonValue>, key: &str) -> Result<String> {
    take_json_value(object, key)?
        .as_str()
        .map(str::to_owned)
        .with_context(|| format!("welfare JSON field `{key}` must be a string"))
}

fn take_json_u64(object: &mut JsonMap<String, JsonValue>, key: &str) -> Result<u64> {
    take_json_value(object, key)?
        .as_u64()
        .with_context(|| format!("welfare JSON field `{key}` must be an unsigned integer"))
}

fn take_json_array(object: &mut JsonMap<String, JsonValue>, key: &str) -> Result<Vec<JsonValue>> {
    match take_json_value(object, key)? {
        JsonValue::Array(values) => Ok(values),
        _ => bail!("welfare JSON field `{key}` must be an array"),
    }
}

fn finish_json_object(object: &JsonMap<String, JsonValue>, context: &str) -> Result<()> {
    ensure!(object.is_empty(), "{context} has unknown fields");
    Ok(())
}

fn json_i64(value: &JsonValue) -> Result<i64> {
    value
        .as_i64()
        .context("welfare JSON value must be an integer")
}

async fn plan_welfare_evaluation(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn WelfareRules,
    scope: &WelfareScopeRow,
    program: &LoadedWelfareProgram,
    mode: WelfarePlanningMode,
) -> Result<PlannedWelfareEvaluation> {
    let required = required_welfare_evidence(&program.definition)?;
    let (evaluation_game_day, prior_close_game_day, anchor) = match mode {
        WelfarePlanningMode::InitialCurrentDay => (scope.game_day, scope.game_day, None),
        WelfarePlanningMode::TargetDay {
            evaluation_game_day,
        } => (evaluation_game_day, scope.game_day, None),
        WelfarePlanningMode::CurrentDayCommand => {
            let anchor = read_welfare_evaluation_anchor(
                tx,
                rules,
                scope,
                program.definition.program_version_id,
            )
            .await?;
            let prior_close_game_day = anchor
                .period_pin
                .window_bounds
                .iter()
                .find(|bound| bound.window == WelfareResolvedWindow::PriorClose)
                .map(|bound| bound.start_game_day)
                .unwrap_or(scope.game_day);
            (scope.game_day, prior_close_game_day, Some(anchor))
        }
    };
    let period_pin = match anchor.as_ref() {
        Some(anchor) => anchor.period_pin.clone(),
        None => build_welfare_period_pin(
            evaluation_game_day,
            prior_close_game_day,
            &required.facts,
            &required.collections,
            &program.definition.reassessment_triggers,
        )?,
    };
    ensure!(
        period_pin.evaluation_game_day == evaluation_game_day,
        "welfare period anchor day drifted"
    );
    let previous_start = period_pin
        .window_bounds
        .iter()
        .filter_map(|bound| match bound.window {
            WelfareResolvedWindow::PreviousClosedDays { .. } => Some(bound.start_game_day),
            _ => None,
        })
        .min()
        .unwrap_or(evaluation_game_day);
    let (facts, collections) = gather_welfare_evidence(
        tx,
        rules,
        scope,
        evaluation_game_day,
        prior_close_game_day,
        &required,
        anchor.as_ref(),
    )
    .await?;
    let input = WelfareEvaluationInput {
        facts: &facts,
        collections: &collections,
        period_pin: &period_pin,
    };
    let evaluation = rules
        .evaluate_program(&program.definition, &input)
        .context("welfare program evaluation failed")?;
    let fingerprint_input = WelfareFingerprintInput {
        schema_version: program.definition.schema_version,
        program_version_id: program.definition.program_version_id,
        facts: &facts,
        collections: &collections,
        period_pin: &period_pin,
    };
    let canonical_json = rules
        .canonical_fingerprint_json(&fingerprint_input)
        .context("welfare fingerprint serialization failed")?;
    let db_fingerprint = hex_sha256(&canonical_json);
    ensure!(
        db_fingerprint == evaluation.fact_fingerprint,
        "welfare fingerprint drifted from canonical input"
    );
    Ok(PlannedWelfareEvaluation {
        facts,
        collections,
        canonical_json,
        evaluation,
        evaluation_game_day,
        authority_state_revision: scope.state_revision,
        prior_close_state_revision: anchor.as_ref().map_or(scope.state_revision, |anchor| {
            anchor.prior_close_state_revision
        }),
        previous_closed_start_game_day: previous_start,
    })
}

fn required_welfare_evidence(
    program: &WelfareProgramDefinition,
) -> Result<WelfareEvidenceRequirements> {
    let constants = program
        .constants
        .iter()
        .map(|constant| (constant.key.clone(), constant.value.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut facts = BTreeSet::new();
    let mut collections = BTreeSet::new();
    for condition in &program.conditions {
        collect_welfare_evidence_requirements(
            &condition.expression,
            &constants,
            &mut facts,
            &mut collections,
        )?;
    }
    ensure!(
        facts.len() + collections.len() <= 32,
        "welfare program requires too much public evidence"
    );
    Ok(WelfareEvidenceRequirements { facts, collections })
}

fn collect_welfare_evidence_requirements(
    expression: &WelfareExpression,
    constants: &BTreeMap<String, WelfareValue>,
    facts: &mut BTreeSet<(String, WelfareResolvedWindow)>,
    collections: &mut BTreeSet<(String, WelfareResolvedWindow)>,
) -> Result<()> {
    match expression {
        WelfareExpression::All { children } | WelfareExpression::Any { children } => {
            for child in children {
                collect_welfare_evidence_requirements(child, constants, facts, collections)?;
            }
        }
        WelfareExpression::Not { child } => {
            collect_welfare_evidence_requirements(child, constants, facts, collections)?;
        }
        WelfareExpression::Eq { left, right }
        | WelfareExpression::Lt { left, right }
        | WelfareExpression::Lte { left, right }
        | WelfareExpression::Gt { left, right }
        | WelfareExpression::Gte { left, right } => {
            collect_welfare_evidence_requirements(left, constants, facts, collections)?;
            collect_welfare_evidence_requirements(right, constants, facts, collections)?;
        }
        WelfareExpression::In { value, .. } => {
            collect_welfare_evidence_requirements(value, constants, facts, collections)?;
        }
        WelfareExpression::Between {
            value,
            lower,
            upper,
        } => {
            collect_welfare_evidence_requirements(value, constants, facts, collections)?;
            collect_welfare_evidence_requirements(lower, constants, facts, collections)?;
            collect_welfare_evidence_requirements(upper, constants, facts, collections)?;
        }
        WelfareExpression::Sum { collection, window }
        | WelfareExpression::Count { collection, window }
        | WelfareExpression::Exists { collection, window } => {
            collections.insert((
                collection.clone(),
                resolve_welfare_window(window, constants)?,
            ));
        }
        WelfareExpression::Fact { path, window } => {
            facts.insert((path.clone(), resolve_welfare_window(window, constants)?));
        }
        WelfareExpression::Constant { .. } | WelfareExpression::Literal { .. } => {}
    }
    Ok(())
}

fn resolve_welfare_window(
    window: &WelfareWindowSpec,
    constants: &BTreeMap<String, WelfareValue>,
) -> Result<WelfareResolvedWindow> {
    match window {
        WelfareWindowSpec::CurrentDay => Ok(WelfareResolvedWindow::CurrentDay),
        WelfareWindowSpec::PriorClose => Ok(WelfareResolvedWindow::PriorClose),
        WelfareWindowSpec::PreviousClosedDays { days } => {
            let days = match days {
                WelfareWindowDays::Literal { days } => *days,
                WelfareWindowDays::Constant { key } => match constants.get(key) {
                    Some(WelfareValue::Count(days)) => u16::try_from(*days)
                        .context("welfare window-day constant is out of range")?,
                    _ => bail!("welfare window-day constant has the wrong type"),
                },
            };
            Ok(WelfareResolvedWindow::PreviousClosedDays { days })
        }
    }
}

fn build_welfare_period_pin(
    evaluation_game_day: u32,
    prior_close_game_day: u32,
    facts: &BTreeSet<(String, WelfareResolvedWindow)>,
    collections: &BTreeSet<(String, WelfareResolvedWindow)>,
    triggers: &[WelfareFactSource],
) -> Result<WelfarePeriodPin> {
    let windows = facts
        .iter()
        .map(|(_, window)| window.clone())
        .chain(collections.iter().map(|(_, window)| window.clone()))
        .collect::<BTreeSet<_>>();
    let mut window_bounds = Vec::with_capacity(windows.len());
    for window in windows {
        let (start_game_day, end_game_day) = match window {
            WelfareResolvedWindow::CurrentDay => (evaluation_game_day, evaluation_game_day),
            WelfareResolvedWindow::PreviousClosedDays { days } => (
                evaluation_game_day.saturating_sub(u32::from(days)),
                evaluation_game_day,
            ),
            WelfareResolvedWindow::PriorClose => (prior_close_game_day, prior_close_game_day),
        };
        window_bounds.push(WelfareWindowBound {
            window,
            start_game_day,
            end_game_day,
        });
    }
    Ok(WelfarePeriodPin {
        evaluation_game_day,
        window_bounds,
        // Schema revisions stay stable when an unrelated save command advances the cursor.
        // The canonical fact values and resolved bounds carry the actual authority state.
        authority_revisions: triggers
            .iter()
            .map(|source| authority(welfare_fact_source_name(*source), 1))
            .collect(),
    })
}

const fn welfare_fact_source_name(source: WelfareFactSource) -> &'static str {
    match source {
        WelfareFactSource::GameDay => "gameDay",
        WelfareFactSource::Household => "household",
        WelfareFactSource::Residence => "residence",
        WelfareFactSource::Employment => "employment",
        WelfareFactSource::Military => "military",
        WelfareFactSource::Income => "income",
        WelfareFactSource::Asset => "asset",
        WelfareFactSource::Debt => "debt",
    }
}

async fn gather_welfare_evidence(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn WelfareRules,
    scope: &WelfareScopeRow,
    evaluation_game_day: u32,
    prior_close_game_day: u32,
    required: &WelfareEvidenceRequirements,
    anchor: Option<&WelfareEvaluationAnchor>,
) -> Result<(Vec<WelfareFactEvidence>, Vec<WelfareCollectionEvidence>)> {
    let required_facts = &required.facts;
    let required_collections = &required.collections;
    let needs_current = required_facts
        .iter()
        .any(|(_, window)| *window == WelfareResolvedWindow::CurrentDay);
    let current = if needs_current {
        Some(read_current_welfare_authorities(tx, scope, evaluation_game_day).await?)
    } else {
        None
    };
    let previous_windows = required_facts
        .iter()
        .map(|(_, window)| window)
        .chain(required_collections.iter().map(|(_, window)| window))
        .filter(|window| matches!(window, WelfareResolvedWindow::PreviousClosedDays { .. }))
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut incomes = BTreeMap::new();
    if anchor.is_none() {
        for window in previous_windows {
            let WelfareResolvedWindow::PreviousClosedDays { days } = window else {
                unreachable!();
            };
            incomes.insert(
                WelfareResolvedWindow::PreviousClosedDays { days },
                read_bounded_income_values(tx, scope, evaluation_game_day, days).await?,
            );
        }
    }
    let needs_asset = anchor.is_none()
        && (required_facts.contains(&(
            "asset.policyValuation".to_owned(),
            WelfareResolvedWindow::PriorClose,
        )) || required_collections.contains(&(
            "asset.positions".to_owned(),
            WelfareResolvedWindow::PriorClose,
        )));
    let assets = if needs_asset {
        Some(read_bounded_asset_values(tx, scope, prior_close_game_day).await?)
    } else {
        None
    };
    let needs_debt = anchor.is_none()
        && (required_facts.contains(&(
            "debt.policyBalance".to_owned(),
            WelfareResolvedWindow::PriorClose,
        )) || required_collections.contains(&(
            "debt.positions".to_owned(),
            WelfareResolvedWindow::PriorClose,
        )));
    let (debt_balance, debt_values) = if needs_debt {
        let debt_balance: i64 =
            sqlx::query_scalar("SELECT debt_krw FROM save WHERE id = ? AND run_revision = ?")
                .bind(scope.save_id)
                .bind(scope.run_revision)
                .fetch_one(&mut **tx)
                .await?;
        (
            Some(debt_balance),
            Some(read_bounded_debt_values(tx, scope, debt_balance).await?),
        )
    } else {
        (None, None)
    };

    let mut facts = Vec::with_capacity(required_facts.len());
    for (key, window) in required_facts {
        if let Some(anchor) = anchor
            && *window != WelfareResolvedWindow::CurrentDay
        {
            facts.push(
                anchor
                    .facts
                    .get(&(key.clone(), window.clone()))
                    .cloned()
                    .context("welfare period anchor is missing a required fact")?,
            );
            continue;
        }
        let definition = rules
            .fact_registry()
            .facts
            .iter()
            .find(|definition| definition.path == *key)
            .context("required welfare fact is not registered")?;
        let value = match (key.as_str(), window) {
            ("character.age", WelfareResolvedWindow::CurrentDay) => known(WelfareValue::AgeYears(
                required_current(&current)?.age_years,
            )),
            ("household.memberCount", WelfareResolvedWindow::CurrentDay) => {
                bounded_count(required_current(&current)?.member_count)
            }
            ("household.dependentCount", WelfareResolvedWindow::CurrentDay) => {
                bounded_count(required_current(&current)?.dependent_count)
            }
            ("residence.exists", WelfareResolvedWindow::CurrentDay) => {
                let count = required_current(&current)?.residence_count;
                ensure!(count <= 1, "multiple welfare residences are active");
                known(WelfareValue::Boolean(count == 1))
            }
            ("residence.region", WelfareResolvedWindow::CurrentDay) => required_current(&current)?
                .residence_region
                .as_deref()
                .map(|region| known(welfare_enum("region", region)))
                .unwrap_or_else(|| unknown(WelfareUnknownReason::AuthorityMissing)),
            ("career.employmentStatus", WelfareResolvedWindow::CurrentDay) => known(welfare_enum(
                "welfareEmployment",
                &required_current(&current)?.employment_status,
            )),
            ("military.status", WelfareResolvedWindow::CurrentDay) => known(welfare_enum(
                "military",
                &required_current(&current)?.military_status,
            )),
            ("income.periodTotal", WelfareResolvedWindow::PreviousClosedDays { .. }) => {
                aggregate_money_evidence(required_bounded_values(&incomes, window)?)
            }
            ("asset.policyValuation", WelfareResolvedWindow::PriorClose) => {
                aggregate_money_evidence(
                    assets
                        .as_ref()
                        .context("welfare asset authority is missing")?,
                )
            }
            ("debt.policyBalance", WelfareResolvedWindow::PriorClose) => {
                let debt_krw = debt_balance.context("welfare debt authority is missing")?;
                ensure!(debt_krw >= 0, "welfare aggregate debt cannot be negative");
                known(WelfareValue::MoneyKrw(debt_krw))
            }
            _ => bail!("welfare fact authority adapter is missing"),
        };
        facts.push(fact(
            key,
            definition.value_type.clone(),
            window.clone(),
            value,
        ));
    }

    let mut collections = Vec::with_capacity(required_collections.len());
    for (key, window) in required_collections {
        if let Some(anchor) = anchor {
            collections.push(
                anchor
                    .collections
                    .get(&(key.clone(), window.clone()))
                    .cloned()
                    .context("welfare period anchor is missing a required collection")?,
            );
            continue;
        }
        let definition = rules
            .fact_registry()
            .collections
            .iter()
            .find(|definition| definition.key == *key)
            .context("required welfare collection is not registered")?;
        let values = match (key.as_str(), window) {
            ("income.entries", WelfareResolvedWindow::PreviousClosedDays { .. }) => {
                collection_money_evidence(required_bounded_values(&incomes, window)?)
            }
            ("asset.positions", WelfareResolvedWindow::PriorClose) => collection_money_evidence(
                assets
                    .as_ref()
                    .context("welfare asset authority is missing")?,
            ),
            ("debt.positions", WelfareResolvedWindow::PriorClose) => collection_money_evidence(
                debt_values
                    .as_ref()
                    .context("welfare debt authority is missing")?,
            ),
            _ => bail!("welfare collection authority adapter is missing"),
        };
        collections.push(WelfareCollectionEvidence {
            key: key.clone(),
            item_type: definition.item_type.clone(),
            window: window.clone(),
            value: values,
        });
    }
    Ok((facts, collections))
}

fn required_current(
    current: &Option<CurrentWelfareAuthorities>,
) -> Result<&CurrentWelfareAuthorities> {
    current
        .as_ref()
        .context("current welfare authority is missing")
}

fn required_bounded_values<'a>(
    values: &'a BTreeMap<WelfareResolvedWindow, BoundedMoneyValues>,
    window: &WelfareResolvedWindow,
) -> Result<&'a BoundedMoneyValues> {
    values
        .get(window)
        .context("bounded welfare authority window is missing")
}

fn known(value: WelfareValue) -> WelfareEvidenceValue {
    WelfareEvidenceValue::Known(value)
}

const fn unknown(reason: WelfareUnknownReason) -> WelfareEvidenceValue {
    WelfareEvidenceValue::Unknown(reason)
}

fn bounded_count(value: i64) -> WelfareEvidenceValue {
    if value > 32 {
        unknown(WelfareUnknownReason::CollectionLimitExceeded)
    } else {
        known(WelfareValue::Count(value))
    }
}

fn aggregate_money_evidence(values: &BoundedMoneyValues) -> WelfareEvidenceValue {
    match values {
        BoundedMoneyValues::Unknown(reason) => unknown(*reason),
        BoundedMoneyValues::Known(values) => values
            .iter()
            .try_fold(0_i64, |total, value| total.checked_add(*value))
            .map_or_else(
                || unknown(WelfareUnknownReason::ArithmeticOverflow),
                |total| known(WelfareValue::MoneyKrw(total)),
            ),
    }
}

fn collection_money_evidence(values: &BoundedMoneyValues) -> WelfareCollectionEvidenceValue {
    match values {
        BoundedMoneyValues::Unknown(reason) => WelfareCollectionEvidenceValue::Unknown(*reason),
        BoundedMoneyValues::Known(values) => WelfareCollectionEvidenceValue::Known(
            values.iter().copied().map(WelfareValue::MoneyKrw).collect(),
        ),
    }
}

async fn read_current_welfare_authorities(
    tx: &mut Transaction<'_, MySql>,
    scope: &WelfareScopeRow,
    evaluation_game_day: u32,
) -> Result<CurrentWelfareAuthorities> {
    let row: (i64, i64, i64, i64, Option<String>, String, String) = sqlx::query_as(
        "SELECT TIMESTAMPDIFF(YEAR, career.birth_date, market.market_date),
                (SELECT COUNT(*) FROM household_member AS member
                 WHERE member.save_id = save.id AND member.run_revision = save.run_revision
                   AND member.joined_game_day <= ?
                   AND (member.left_game_day IS NULL OR member.left_game_day > ?)),
                (SELECT COUNT(*) FROM household_member AS member
                 WHERE member.save_id = save.id AND member.run_revision = save.run_revision
                   AND member.member_role <> 'player' AND member.joined_game_day <= ?
                   AND (member.left_game_day IS NULL OR member.left_game_day > ?)),
                (SELECT COUNT(*) FROM residence
                 WHERE residence.save_id = save.id
                   AND residence.run_revision = save.run_revision
                   AND residence.effective_from_game_day <= ?
                   AND (residence.effective_to_game_day IS NULL
                        OR residence.effective_to_game_day > ?)),
                (SELECT residence.region_key FROM residence
                 WHERE residence.save_id = save.id
                   AND residence.run_revision = save.run_revision
                   AND residence.effective_from_game_day <= ?
                   AND (residence.effective_to_game_day IS NULL
                        OR residence.effective_to_game_day > ?)
                 ORDER BY residence.id LIMIT 1),
                COALESCE((SELECT CASE
                                      WHEN contract.start_game_day > ? THEN 'pendingStart'
                                      WHEN contract.end_game_day IS NOT NULL
                                           AND contract.end_game_day <= ? THEN 'ended'
                                      ELSE 'active'
                                  END
                          FROM employment_contract AS contract
                          WHERE contract.save_id = save.id
                            AND contract.run_revision = save.run_revision
                          ORDER BY contract.id DESC LIMIT 1), 'none'),
                CASE
                    WHEN career.military_status = 'exempt' THEN 'exempt'
                    ELSE COALESCE((
                        SELECT CASE
                                   WHEN ? < service.start_game_day THEN 'unserved'
                                   WHEN ? < service.end_game_day THEN 'serving'
                                   ELSE 'completed'
                               END
                        FROM military_service AS service
                        WHERE service.save_id = save.id
                          AND service.run_revision = save.run_revision
                        ORDER BY service.id DESC LIMIT 1
                    ), career.military_status)
                END
         FROM save
         INNER JOIN career_run AS career
           ON career.save_id = save.id AND career.run_revision = save.run_revision
         INNER JOIN market_daily AS market
           ON market.world_id = save.market_world_id AND market.game_day = ?
         WHERE save.id = ? AND save.run_revision = ?",
    )
    .bind(evaluation_game_day)
    .bind(evaluation_game_day)
    .bind(evaluation_game_day)
    .bind(evaluation_game_day)
    .bind(evaluation_game_day)
    .bind(evaluation_game_day)
    .bind(evaluation_game_day)
    .bind(evaluation_game_day)
    .bind(evaluation_game_day)
    .bind(evaluation_game_day)
    .bind(evaluation_game_day)
    .bind(evaluation_game_day)
    .bind(evaluation_game_day)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        row.1 >= 0 && row.2 >= 0 && row.3 >= 0,
        "welfare authority count is negative"
    );
    Ok(CurrentWelfareAuthorities {
        age_years: row.0,
        member_count: row.1,
        dependent_count: row.2,
        residence_count: row.3,
        residence_region: row.4,
        employment_status: row.5,
        military_status: row.6,
    })
}

async fn read_bounded_income_values(
    tx: &mut Transaction<'_, MySql>,
    scope: &WelfareScopeRow,
    evaluation_game_day: u32,
    days: u16,
) -> Result<BoundedMoneyValues> {
    let start_game_day = evaluation_game_day.saturating_sub(u32::from(days));
    let values: Vec<i64> = sqlx::query_scalar(
        "SELECT gross_employment_income_krw
         FROM employment_income_event
         WHERE save_id = ? AND run_revision = ?
           AND paid_game_day >= ? AND paid_game_day < ?
         ORDER BY paid_game_day, id LIMIT 33",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(start_game_day)
    .bind(evaluation_game_day)
    .fetch_all(&mut **tx)
    .await?;
    Ok(if values.len() > 32 {
        BoundedMoneyValues::Unknown(WelfareUnknownReason::CollectionLimitExceeded)
    } else {
        BoundedMoneyValues::Known(values)
    })
}

async fn read_bounded_debt_values(
    tx: &mut Transaction<'_, MySql>,
    scope: &WelfareScopeRow,
    expected_debt_krw: i64,
) -> Result<BoundedMoneyValues> {
    let rows: Vec<(i64, i64, i64)> = sqlx::query_as(
        "SELECT remaining_principal_krw, accrued_interest_krw, accrued_fee_krw
         FROM loan_contract
         WHERE save_id = ? AND run_revision = ?
           AND status IN ('active', 'delinquent', 'defaulted', 'restructured')
         ORDER BY id LIMIT 33",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_all(&mut **tx)
    .await?;
    if rows.len() > 32 {
        return Ok(BoundedMoneyValues::Unknown(
            WelfareUnknownReason::CollectionLimitExceeded,
        ));
    }
    let mut values = Vec::with_capacity(rows.len());
    for (principal_krw, interest_krw, fee_krw) in rows {
        ensure!(
            principal_krw >= 0 && interest_krw >= 0 && fee_krw >= 0,
            "loan debt position contains a negative component"
        );
        let total_krw = i128::from(principal_krw) + i128::from(interest_krw) + i128::from(fee_krw);
        let Ok(total_krw) = i64::try_from(total_krw) else {
            return Ok(BoundedMoneyValues::Unknown(
                WelfareUnknownReason::ArithmeticOverflow,
            ));
        };
        values.push(total_krw);
    }
    values.extend(
        sqlx::query_scalar::<_, i64>(
            "SELECT outstanding_amount_krw FROM essential_arrear
             WHERE save_id = ? AND run_revision = ? AND status = 'active'
             ORDER BY id LIMIT 33",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .fetch_all(&mut **tx)
        .await?,
    );
    if values.len() > 32 {
        return Ok(BoundedMoneyValues::Unknown(
            WelfareUnknownReason::CollectionLimitExceeded,
        ));
    }
    values.extend(
        sqlx::query_scalar::<_, i64>(
            "SELECT remaining_krw FROM lease_arrear
             WHERE save_id = ? AND run_revision = ? AND status = 'active'
             ORDER BY id LIMIT 33",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .fetch_all(&mut **tx)
        .await?,
    );
    if values.len() > 32 {
        return Ok(BoundedMoneyValues::Unknown(
            WelfareUnknownReason::CollectionLimitExceeded,
        ));
    }
    values.extend(
        sqlx::query_scalar::<_, i64>(
            "SELECT outstanding_amount_krw FROM tax_obligation
             WHERE save_id = ? AND run_revision = ? AND status = 'outstanding'
             ORDER BY id LIMIT 33",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .fetch_all(&mut **tx)
        .await?,
    );
    if values.len() > 32 {
        return Ok(BoundedMoneyValues::Unknown(
            WelfareUnknownReason::CollectionLimitExceeded,
        ));
    }
    ensure!(
        values.iter().all(|value| *value >= 0),
        "welfare debt position is negative"
    );
    let Some(total_krw) = checked_money_total(&values) else {
        return Ok(BoundedMoneyValues::Unknown(
            WelfareUnknownReason::ArithmeticOverflow,
        ));
    };
    ensure!(
        expected_debt_krw >= 0 && total_krw == expected_debt_krw,
        "welfare debt positions disagree with the save projection"
    );
    Ok(BoundedMoneyValues::Known(values))
}

fn checked_money_total(values: &[i64]) -> Option<i64> {
    let total = values
        .iter()
        .try_fold(0_i128, |total, value| total.checked_add(i128::from(*value)))?;
    i64::try_from(total).ok()
}

async fn read_welfare_evaluation_anchor(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn WelfareRules,
    scope: &WelfareScopeRow,
    program_version_id: ResourceId,
) -> Result<WelfareEvaluationAnchor> {
    let row: (String, u32, u64) = sqlx::query_as(
        "SELECT CAST(canonical_input_json AS CHAR),
                previous_closed_start_game_day, prior_close_state_revision
         FROM welfare_period_pin
         WHERE save_id = ? AND run_revision = ? AND program_version_id = ?
           AND evaluation_game_day = ?
         ORDER BY authority_state_revision DESC, id DESC LIMIT 1 FOR SHARE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(program_version_id.get())
    .bind(scope.game_day)
    .fetch_optional(&mut **tx)
    .await?
    .context("current welfare period anchor is missing")?;
    parse_welfare_evaluation_anchor(
        rules,
        program_version_id,
        scope.game_day,
        &row.0,
        row.1,
        row.2,
    )
}

fn parse_welfare_evaluation_anchor(
    rules: &dyn WelfareRules,
    program_version_id: ResourceId,
    evaluation_game_day: u32,
    raw: &str,
    previous_closed_start_game_day: u32,
    prior_close_state_revision: u64,
) -> Result<WelfareEvaluationAnchor> {
    let value: JsonValue =
        serde_json::from_str(raw).context("stored welfare period anchor is invalid")?;
    let mut object = into_json_object(value, "welfare period anchor")?;
    ensure!(
        take_json_u64(&mut object, "schemaVersion")? == 1
            && take_json_string(&mut object, "programVersionId")? == program_version_id.to_string(),
        "welfare period anchor header drifted"
    );
    let period_pin = parse_canonical_welfare_period(take_json_value(&mut object, "period")?)?;
    ensure!(
        period_pin.evaluation_game_day == evaluation_game_day,
        "welfare period anchor day drifted"
    );
    let evidence = take_json_array(&mut object, "facts")?;
    finish_json_object(&object, "welfare period anchor")?;
    let mut facts = BTreeMap::new();
    let mut collections = BTreeMap::new();
    for evidence in evidence {
        let mut evidence = into_json_object(evidence, "welfare canonical evidence")?;
        let key = take_json_string(&mut evidence, "key")?;
        let kind = take_json_string(&mut evidence, "kind")?;
        let value_type = take_json_string(&mut evidence, "valueType")?;
        let unit = take_json_string(&mut evidence, "unit")?;
        let window = parse_canonical_welfare_window(&take_json_string(&mut evidence, "window")?)?;
        let value = take_json_value(&mut evidence, "value")?;
        finish_json_object(&evidence, "welfare canonical evidence")?;
        match kind.as_str() {
            "fact" => {
                let definition = rules
                    .fact_registry()
                    .facts
                    .iter()
                    .find(|definition| definition.path == key)
                    .context("welfare anchor contains an unknown fact")?;
                ensure_canonical_type(&definition.value_type, &value_type, &unit)?;
                let value = parse_canonical_fact_value(&definition.value_type, value)?;
                ensure!(
                    facts
                        .insert(
                            (key.clone(), window.clone()),
                            WelfareFactEvidence {
                                key,
                                value_type: definition.value_type.clone(),
                                window,
                                value,
                            },
                        )
                        .is_none(),
                    "welfare anchor contains duplicate facts"
                );
            }
            "collection" => {
                let definition = rules
                    .fact_registry()
                    .collections
                    .iter()
                    .find(|definition| definition.key == key)
                    .context("welfare anchor contains an unknown collection")?;
                ensure_canonical_type(&definition.item_type, &value_type, &unit)?;
                let value = parse_canonical_collection_value(&definition.item_type, value)?;
                ensure!(
                    collections
                        .insert(
                            (key.clone(), window.clone()),
                            WelfareCollectionEvidence {
                                key,
                                item_type: definition.item_type.clone(),
                                window,
                                value,
                            },
                        )
                        .is_none(),
                    "welfare anchor contains duplicate collections"
                );
            }
            _ => bail!("welfare anchor evidence kind is invalid"),
        }
    }
    let canonical_previous_start = period_pin
        .window_bounds
        .iter()
        .filter_map(|bound| match bound.window {
            WelfareResolvedWindow::PreviousClosedDays { .. } => Some(bound.start_game_day),
            _ => None,
        })
        .min()
        .unwrap_or(evaluation_game_day);
    ensure!(
        canonical_previous_start == previous_closed_start_game_day,
        "welfare anchor previous-closed boundary drifted"
    );
    Ok(WelfareEvaluationAnchor {
        facts,
        collections,
        period_pin,
        prior_close_state_revision,
    })
}

fn parse_canonical_welfare_period(value: JsonValue) -> Result<WelfarePeriodPin> {
    let mut object = into_json_object(value, "welfare canonical period")?;
    let evaluation_game_day = u32::try_from(take_json_u64(&mut object, "evaluationGameDay")?)
        .context("welfare evaluation day overflowed")?;
    let window_bounds = take_json_array(&mut object, "windowBounds")?
        .into_iter()
        .map(|bound| {
            let mut bound = into_json_object(bound, "welfare canonical window bound")?;
            let window = parse_canonical_welfare_window(&take_json_string(&mut bound, "window")?)?;
            let start_game_day = u32::try_from(take_json_u64(&mut bound, "startGameDay")?)
                .context("welfare window start overflowed")?;
            let end_game_day = u32::try_from(take_json_u64(&mut bound, "endGameDay")?)
                .context("welfare window end overflowed")?;
            finish_json_object(&bound, "welfare canonical window bound")?;
            Ok(WelfareWindowBound {
                window,
                start_game_day,
                end_game_day,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let authority_revisions = take_json_array(&mut object, "authorityRevisions")?
        .into_iter()
        .map(|revision| {
            let mut revision = into_json_object(revision, "welfare canonical authority revision")?;
            let authority = take_json_string(&mut revision, "authority")?;
            let revision_value = take_json_string(&mut revision, "revision")?;
            finish_json_object(&revision, "welfare canonical authority revision")?;
            Ok(WelfareAuthorityRevision {
                authority,
                revision: revision_value,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    finish_json_object(&object, "welfare canonical period")?;
    Ok(WelfarePeriodPin {
        evaluation_game_day,
        window_bounds,
        authority_revisions,
    })
}

fn parse_canonical_welfare_window(raw: &str) -> Result<WelfareResolvedWindow> {
    match raw {
        "currentDay" => Ok(WelfareResolvedWindow::CurrentDay),
        "priorClose" => Ok(WelfareResolvedWindow::PriorClose),
        _ => {
            let Some(days) = raw.strip_prefix("previousClosedDays:") else {
                bail!("welfare canonical window is invalid");
            };
            Ok(WelfareResolvedWindow::PreviousClosedDays {
                days: days
                    .parse()
                    .context("welfare canonical window days are invalid")?,
            })
        }
    }
}

fn ensure_canonical_type(expected: &WelfareValueType, value_type: &str, unit: &str) -> Result<()> {
    let (expected_type, expected_unit) = canonical_value_type(expected);
    ensure!(
        value_type == expected_type && unit == expected_unit,
        "welfare canonical evidence type drifted"
    );
    Ok(())
}

fn canonical_value_type(value_type: &WelfareValueType) -> (String, String) {
    match value_type {
        WelfareValueType::Boolean => ("boolean".to_owned(), "boolean".to_owned()),
        WelfareValueType::Integer => ("integer".to_owned(), "integer".to_owned()),
        WelfareValueType::MoneyKrw => ("moneyKrw".to_owned(), "krw".to_owned()),
        WelfareValueType::Count => ("count".to_owned(), "count".to_owned()),
        WelfareValueType::AgeYears => ("ageYears".to_owned(), "years".to_owned()),
        WelfareValueType::Date => ("date".to_owned(), "date".to_owned()),
        WelfareValueType::String => ("string".to_owned(), "string".to_owned()),
        WelfareValueType::Enum(schema_key) => ("enum".to_owned(), schema_key.clone()),
    }
}

fn parse_canonical_fact_value(
    value_type: &WelfareValueType,
    value: JsonValue,
) -> Result<WelfareEvidenceValue> {
    let mut object = into_json_object(value, "welfare canonical fact value")?;
    let state = take_json_string(&mut object, "state")?;
    let value = match state.as_str() {
        "known" => WelfareEvidenceValue::Known(parse_canonical_value(
            value_type,
            take_json_value(&mut object, "value")?,
        )?),
        "unknown" => WelfareEvidenceValue::Unknown(parse_unknown_reason(&take_json_string(
            &mut object,
            "reason",
        )?)?),
        _ => bail!("welfare canonical evidence state is invalid"),
    };
    finish_json_object(&object, "welfare canonical fact value")?;
    Ok(value)
}

fn parse_canonical_collection_value(
    item_type: &WelfareValueType,
    value: JsonValue,
) -> Result<WelfareCollectionEvidenceValue> {
    let mut object = into_json_object(value, "welfare canonical collection value")?;
    let state = take_json_string(&mut object, "state")?;
    let value = match state.as_str() {
        "known" => {
            WelfareCollectionEvidenceValue::Known(match take_json_value(&mut object, "value")? {
                JsonValue::Array(values) => values
                    .into_iter()
                    .map(|value| parse_canonical_value(item_type, value))
                    .collect::<Result<Vec<_>>>()?,
                _ => bail!("welfare canonical collection value must be an array"),
            })
        }
        "unknown" => WelfareCollectionEvidenceValue::Unknown(parse_unknown_reason(
            &take_json_string(&mut object, "reason")?,
        )?),
        _ => bail!("welfare canonical collection state is invalid"),
    };
    finish_json_object(&object, "welfare canonical collection value")?;
    Ok(value)
}

fn parse_canonical_value(value_type: &WelfareValueType, value: JsonValue) -> Result<WelfareValue> {
    match value_type {
        WelfareValueType::Boolean => Ok(WelfareValue::Boolean(
            value
                .as_bool()
                .context("welfare canonical boolean is invalid")?,
        )),
        WelfareValueType::Integer => Ok(WelfareValue::Integer(json_i64(&value)?)),
        WelfareValueType::MoneyKrw => Ok(WelfareValue::MoneyKrw(json_i64(&value)?)),
        WelfareValueType::Count => Ok(WelfareValue::Count(json_i64(&value)?)),
        WelfareValueType::AgeYears => Ok(WelfareValue::AgeYears(json_i64(&value)?)),
        WelfareValueType::Date => Ok(WelfareValue::Date(parse_storage_date(
            value
                .as_str()
                .context("welfare canonical date is invalid")?,
        )?)),
        WelfareValueType::String => Ok(WelfareValue::String(
            value
                .as_str()
                .context("welfare canonical string is invalid")?
                .to_owned(),
        )),
        WelfareValueType::Enum(schema_key) => Ok(WelfareValue::Enum(WelfareEnumValue {
            schema_key: schema_key.clone(),
            value: value
                .as_str()
                .context("welfare canonical enum is invalid")?
                .to_owned(),
        })),
    }
}

fn parse_unknown_reason(raw: &str) -> Result<WelfareUnknownReason> {
    match raw {
        "authorityMissing" => Ok(WelfareUnknownReason::AuthorityMissing),
        "valuationUnavailable" => Ok(WelfareUnknownReason::ValuationUnavailable),
        "collectionLimitExceeded" => Ok(WelfareUnknownReason::CollectionLimitExceeded),
        "windowIncomplete" => Ok(WelfareUnknownReason::WindowIncomplete),
        "arithmeticOverflow" => Ok(WelfareUnknownReason::ArithmeticOverflow),
        _ => bail!("welfare canonical unknown reason is invalid"),
    }
}

async fn read_bounded_asset_values(
    tx: &mut Transaction<'_, MySql>,
    scope: &WelfareScopeRow,
    valuation_game_day: u32,
) -> Result<BoundedMoneyValues> {
    let (wallet_cash_krw, property_book_value_krw): (i64, i64) = sqlx::query_as(
        "SELECT cash_krw, property_book_value_krw
         FROM save WHERE id = ? AND run_revision = ?",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_one(&mut **tx)
    .await?;
    let mut components = vec![wallet_cash_krw];
    components.extend(
        sqlx::query_scalar::<_, i64>(
            "SELECT cash_krw FROM financial_account
             WHERE save_id = ? AND run_revision = ? AND status = 'open'
             ORDER BY id LIMIT 33",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .fetch_all(&mut **tx)
        .await?,
    );
    if components.len() > 32 {
        return Ok(BoundedMoneyValues::Unknown(
            WelfareUnknownReason::CollectionLimitExceeded,
        ));
    }

    let cash_contracts: Vec<(u64, String, Option<i64>)> = sqlx::query_as(
        "SELECT id, contract_kind, principal_krw
         FROM cash_product_contract
         WHERE save_id = ? AND run_revision = ? AND status = 'active'
         ORDER BY id LIMIT 33",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_all(&mut **tx)
    .await?;
    if components.len() + cash_contracts.len() > 32 {
        return Ok(BoundedMoneyValues::Unknown(
            WelfareUnknownReason::CollectionLimitExceeded,
        ));
    }
    let installment_rows: Vec<(u64, i64)> = sqlx::query_as(
        "SELECT contract.id, installment.amount_krw
         FROM cash_product_contract AS contract
         INNER JOIN savings_installment AS installment
           ON installment.contract_id = contract.id AND installment.status = 'paid'
         WHERE contract.save_id = ? AND contract.run_revision = ?
           AND contract.status = 'active'
           AND contract.contract_kind = 'installmentSavings'
         ORDER BY contract.id, installment.installment_no",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_all(&mut **tx)
    .await?;
    let mut installment_principals = cash_contracts
        .iter()
        .filter(|(_, kind, _)| kind == "installmentSavings")
        .map(|(id, _, _)| (*id, 0_i128))
        .collect::<BTreeMap<_, _>>();
    for (contract_id, amount_krw) in installment_rows {
        ensure!(amount_krw > 0, "paid savings installment is not positive");
        let principal = installment_principals
            .get_mut(&contract_id)
            .context("paid installment has no active savings contract")?;
        let Some(next) = principal.checked_add(i128::from(amount_krw)) else {
            return Ok(BoundedMoneyValues::Unknown(
                WelfareUnknownReason::ArithmeticOverflow,
            ));
        };
        *principal = next;
    }
    for (contract_id, kind, principal_krw) in cash_contracts {
        let value = match kind.as_str() {
            "termDeposit" => principal_krw.context("term deposit principal is missing")?,
            "installmentSavings" => {
                ensure!(
                    principal_krw.is_none(),
                    "installment savings has a stored lump-sum principal"
                );
                let principal = installment_principals
                    .remove(&contract_id)
                    .context("installment savings principal accumulator is missing")?;
                let Ok(principal) = i64::try_from(principal) else {
                    return Ok(BoundedMoneyValues::Unknown(
                        WelfareUnknownReason::ArithmeticOverflow,
                    ));
                };
                principal
            }
            _ => bail!("active cash product kind is invalid"),
        };
        components.push(value);
    }
    ensure!(
        installment_principals.is_empty(),
        "unused installment principal accumulator remains"
    );

    components.extend(
        sqlx::query_scalar::<_, i64>(
            "SELECT principal_krw FROM military_savings_contract
             WHERE save_id = ? AND run_revision = ? AND status = 'active'
             ORDER BY id LIMIT 33",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .fetch_all(&mut **tx)
        .await?,
    );
    if components.len() > 32 {
        return Ok(BoundedMoneyValues::Unknown(
            WelfareUnknownReason::CollectionLimitExceeded,
        ));
    }
    let lease_deposits: Vec<i64> = sqlx::query_scalar(
        "SELECT deposit_krw FROM lease_contract
         WHERE save_id = ? AND run_revision = ? AND role = 'tenant'
           AND effective_from_game_day <= ?
           AND (effective_to_game_day IS NULL OR effective_to_game_day > ?)
         ORDER BY id LIMIT 2",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(valuation_game_day)
    .bind(valuation_game_day)
    .fetch_all(&mut **tx)
    .await?;
    if lease_deposits.len() > 1 {
        bail!("multiple active tenant leases entered welfare valuation");
    }
    components.extend(lease_deposits);
    if components.len() > 32 {
        return Ok(BoundedMoneyValues::Unknown(
            WelfareUnknownReason::CollectionLimitExceeded,
        ));
    }

    let llx_positions: Vec<(u32, Option<i64>)> = sqlx::query_as(
        "SELECT position.quantity, market.llx_close_krw
         FROM asset_position AS position
         INNER JOIN financial_account AS account
           ON account.id = position.account_id
          AND account.save_id = position.save_id
          AND account.run_revision = ?
          AND account.status = 'open'
         LEFT JOIN market_daily AS market
           ON market.world_id = ? AND market.game_day = ?
         WHERE position.save_id = ? AND position.symbol = 'LLX'
         ORDER BY position.account_id LIMIT 33",
    )
    .bind(scope.run_revision)
    .bind(scope.market_world_id)
    .bind(valuation_game_day)
    .bind(scope.save_id)
    .fetch_all(&mut **tx)
    .await?;
    for (quantity, close_krw) in llx_positions {
        let Some(close_krw) = close_krw else {
            return Ok(BoundedMoneyValues::Unknown(
                WelfareUnknownReason::ValuationUnavailable,
            ));
        };
        let Some(market_value_krw) = i128::from(quantity)
            .checked_mul(i128::from(close_krw))
            .and_then(|value| i64::try_from(value).ok())
        else {
            return Ok(BoundedMoneyValues::Unknown(
                WelfareUnknownReason::ArithmeticOverflow,
            ));
        };
        components.push(market_value_krw);
    }
    if components.len() > 32 {
        return Ok(BoundedMoneyValues::Unknown(
            WelfareUnknownReason::CollectionLimitExceeded,
        ));
    }

    let m2d_values = read_m2d_welfare_policy_values_in_tx(
        tx,
        scope.save_id,
        scope.market_world_id,
        scope.market_world_product_bundle_id,
        scope.run_revision,
        valuation_game_day,
    )
    .await?;
    match m2d_values {
        M2dWelfarePolicyValues::Known(values) => components.extend(values),
        M2dWelfarePolicyValues::Unknown(reason) => {
            return Ok(BoundedMoneyValues::Unknown(match reason {
                M2dWelfareValuationUnknown::AuthorityMissing => {
                    WelfareUnknownReason::AuthorityMissing
                }
                M2dWelfareValuationUnknown::ValuationUnavailable => {
                    WelfareUnknownReason::ValuationUnavailable
                }
                M2dWelfareValuationUnknown::ArithmeticOverflow => {
                    WelfareUnknownReason::ArithmeticOverflow
                }
                M2dWelfareValuationUnknown::CollectionLimitExceeded => {
                    WelfareUnknownReason::CollectionLimitExceeded
                }
            }));
        }
    }
    if components.len() > 32 {
        return Ok(BoundedMoneyValues::Unknown(
            WelfareUnknownReason::CollectionLimitExceeded,
        ));
    }

    let property_values: Vec<i64> = sqlx::query_scalar(
        "SELECT book_value_krw FROM property_holding
         WHERE save_id = ? AND run_revision = ? AND status = 'active'
         ORDER BY id LIMIT 33",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_all(&mut **tx)
    .await?;
    if components.len() + property_values.len() > 32 {
        return Ok(BoundedMoneyValues::Unknown(
            WelfareUnknownReason::CollectionLimitExceeded,
        ));
    }
    let Some(projected_property_book_value_krw) = checked_money_total(&property_values) else {
        return Ok(BoundedMoneyValues::Unknown(
            WelfareUnknownReason::ArithmeticOverflow,
        ));
    };
    ensure!(
        projected_property_book_value_krw == property_book_value_krw,
        "welfare property positions disagree with the save projection"
    );
    components.extend(property_values);
    ensure!(
        components.iter().all(|value| *value >= 0),
        "welfare gross asset component is negative"
    );
    if checked_money_total(&components).is_none() {
        return Ok(BoundedMoneyValues::Unknown(
            WelfareUnknownReason::ArithmeticOverflow,
        ));
    }
    Ok(BoundedMoneyValues::Known(components))
}

async fn persist_welfare_evaluation(
    tx: &mut Transaction<'_, MySql>,
    scope: &WelfareScopeRow,
    program: &LoadedWelfareProgram,
    planned: &PlannedWelfareEvaluation,
) -> Result<u64> {
    let existing: Option<(u64, i64)> = sqlx::query_as(
        "SELECT evaluation.id, COUNT(evidence.evaluation_id)
         FROM welfare_evaluation AS evaluation
         LEFT JOIN welfare_evaluation_condition AS evidence
           ON evidence.evaluation_id = evaluation.id
         WHERE evaluation.save_id = ? AND evaluation.run_revision = ?
           AND evaluation.program_version_id = ?
           AND BINARY evaluation.fact_fingerprint = BINARY ?
         GROUP BY evaluation.id",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(program.definition.program_version_id.get())
    .bind(&planned.evaluation.fact_fingerprint)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((evaluation_id, evidence_count)) = existing {
        ensure!(
            evidence_count
                == i64::try_from(program.conditions.len())
                    .context("welfare condition count overflowed")?,
            "stored welfare evaluation evidence is incomplete"
        );
        return Ok(evaluation_id);
    }

    let fact_count = u8::try_from(planned.facts.len() + planned.collections.len())
        .context("too much welfare evidence")?;
    let pin = sqlx::query(
        "INSERT INTO welfare_period_pin
             (save_id, run_revision, life_catalog_set_id, welfare_component_version_id,
              program_version_id, evaluation_game_day, authority_state_revision,
              previous_closed_start_game_day, previous_closed_end_game_day,
              prior_close_state_revision, fact_count, canonical_input_json)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.life_catalog_set_id)
    .bind(scope.welfare_component_version_id)
    .bind(program.definition.program_version_id.get())
    .bind(planned.evaluation_game_day)
    .bind(planned.authority_state_revision)
    .bind(planned.previous_closed_start_game_day)
    .bind(planned.evaluation_game_day)
    .bind(planned.prior_close_state_revision)
    .bind(fact_count)
    .bind(&planned.canonical_json)
    .execute(&mut **tx)
    .await?;
    let pin_id = pin.last_insert_id();
    ensure!(pin_id > 0, "welfare period pin has no identity");
    let status = evaluation_status_name(planned.evaluation.status)?;
    let condition_count =
        u8::try_from(planned.evaluation.conditions.len()).context("too many welfare conditions")?;
    let inserted_evaluation = sqlx::query(
        "INSERT INTO welfare_evaluation
             (save_id, run_revision, program_version_id, period_pin_id,
              fact_fingerprint, evaluation_game_day, authority_state_revision,
              status, condition_count)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(program.definition.program_version_id.get())
    .bind(pin_id)
    .bind(&planned.evaluation.fact_fingerprint)
    .bind(planned.evaluation_game_day)
    .bind(planned.authority_state_revision)
    .bind(status)
    .bind(condition_count)
    .execute(&mut **tx)
    .await?;
    let evaluation_id = inserted_evaluation.last_insert_id();
    ensure!(evaluation_id > 0, "welfare evaluation has no identity");
    for (catalog, result) in program
        .conditions
        .iter()
        .zip(&planned.evaluation.conditions)
    {
        ensure!(
            catalog.condition_code == result.code,
            "welfare condition order drifted"
        );
        let (outcome, unknown_reason) = truth_names(result.result);
        sqlx::query(
            "INSERT INTO welfare_evaluation_condition
                 (save_id, run_revision, evaluation_id, program_version_id,
                  program_condition_id, condition_order, outcome, unknown_reason)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(evaluation_id)
        .bind(program.definition.program_version_id.get())
        .bind(catalog.id)
        .bind(catalog.condition_order)
        .bind(outcome)
        .bind(unknown_reason)
        .execute(&mut **tx)
        .await?;
    }
    Ok(evaluation_id)
}

async fn read_welfare_program_state(
    tx: &mut Transaction<'_, MySql>,
    scope: &WelfareScopeRow,
    program: &LoadedWelfareProgram,
) -> Result<WelfareProgramState> {
    let evaluation: WelfareEvaluationRow = sqlx::query_as(
        "SELECT id, status, fact_fingerprint FROM welfare_evaluation
         WHERE save_id = ? AND run_revision = ? AND program_version_id = ?
           AND evaluation_game_day = ?
         ORDER BY authority_state_revision DESC, id DESC LIMIT 1",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(program.definition.program_version_id.get())
    .bind(scope.game_day)
    .fetch_one(&mut **tx)
    .await
    .context("current welfare planner evaluation is missing")?;
    let condition_rows: Vec<WelfareEvaluationConditionRow> = sqlx::query_as(
        "SELECT catalog_condition.condition_code, catalog_condition.public_label, evidence.outcome
         FROM welfare_evaluation_condition AS evidence
         INNER JOIN welfare_program_condition AS catalog_condition
           ON catalog_condition.id = evidence.program_condition_id
         WHERE evidence.evaluation_id = ? ORDER BY evidence.condition_order",
    )
    .bind(evaluation.id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        condition_rows.len() == program.conditions.len(),
        "welfare evaluation evidence is incomplete"
    );
    let latest: Option<WelfareApplicationRow> = sqlx::query_as(
        "SELECT id, status, application_game_day, approval_game_day, paid_krw
         FROM welfare_application
         WHERE save_id = ? AND run_revision = ? AND program_version_id = ?
         ORDER BY id DESC LIMIT 1",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(program.definition.program_version_id.get())
    .fetch_optional(&mut **tx)
    .await?;
    let duplicate_group_claimed: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM welfare_application
             WHERE save_id = ? AND run_revision = ?
               AND BINARY duplicate_group_claim_key = BINARY ?
         )",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(&program.duplicate_group_key)
    .fetch_one(&mut **tx)
    .await?;
    let next_payment = match latest.as_ref() {
        Some(application) => read_pending_payment(tx, scope, application.id).await?,
        None => None,
    };
    let evaluation_status = parse_evaluation_status(&evaluation.status)?;
    Ok(WelfareProgramState {
        id: program.definition.program_version_id,
        program_key: program.definition.program_key.clone(),
        display_name: program.display_name.clone(),
        benefit_krw: program.benefit_krw,
        payment_delay_game_days: program.payment_delay_game_days,
        evaluation_status,
        fact_fingerprint: evaluation.fact_fingerprint,
        conditions: condition_rows
            .into_iter()
            .map(condition_state)
            .collect::<Result<_>>()?,
        application_available: evaluation_status == WelfareEvaluationStatusState::Eligible
            && !duplicate_group_claimed,
        latest_application: latest.map(application_summary).transpose()?,
        next_payment,
    })
}

async fn read_pending_payment(
    tx: &mut Transaction<'_, MySql>,
    scope: &WelfareScopeRow,
    application_id: u64,
) -> Result<Option<WelfarePaymentState>> {
    let row: Option<WelfarePaymentRow> = sqlx::query_as(
        "SELECT id, payment_no, amount_krw, due_game_day, status
         FROM welfare_payment
         WHERE save_id = ? AND run_revision = ? AND application_id = ?
           AND status = 'pending' ORDER BY payment_no LIMIT 1",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(application_id)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(payment_state).transpose()
}

async fn insert_welfare_application(
    tx: &mut Transaction<'_, MySql>,
    scope: &WelfareScopeRow,
    program: &LoadedWelfareProgram,
    evaluation_id: u64,
    fingerprint: &str,
    command: &ApplyWelfareProgramCommand,
) -> Result<u64> {
    let insert = sqlx::query(
        "INSERT INTO welfare_application
             (save_id, run_revision, life_catalog_set_id, welfare_component_version_id,
              program_version_id, eligibility_evaluation_id,
              eligibility_fact_fingerprint, eligibility_basis, command_id,
              duplicate_group_key, benefit_amount_krw, payment_delay_game_days,
              status, application_game_day)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'eligibilityAtApplication', ?, ?, ?, ?, 'applied', ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.life_catalog_set_id)
    .bind(scope.welfare_component_version_id)
    .bind(program.definition.program_version_id.get())
    .bind(evaluation_id)
    .bind(fingerprint)
    .bind(command.command_id.as_str())
    .bind(&program.duplicate_group_key)
    .bind(program.benefit_krw)
    .bind(program.payment_delay_game_days)
    .bind(scope.game_day)
    .execute(&mut **tx)
    .await?;
    let application_id = insert.last_insert_id();
    ensure!(application_id > 0, "welfare application has no identity");
    insert_application_transition(
        tx,
        WelfareApplicationTransitionInsert {
            save_id: scope.save_id,
            run_revision: scope.run_revision,
            application_id,
            transition_no: 1,
            from_status: None,
            to_status: "applied",
            game_day: scope.game_day,
            reason: "playerApplication",
        },
    )
    .await?;
    let approval = sqlx::query(
        "UPDATE welfare_application
         SET status = 'approved', approval_game_day = application_game_day,
             duplicate_group_claim_key = duplicate_group_key
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'applied'",
    )
    .bind(application_id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .execute(&mut **tx)
    .await?;
    ensure!(
        approval.rows_affected() == 1,
        "welfare application was not approved"
    );
    insert_application_transition(
        tx,
        WelfareApplicationTransitionInsert {
            save_id: scope.save_id,
            run_revision: scope.run_revision,
            application_id,
            transition_no: 2,
            from_status: Some("applied"),
            to_status: "approved",
            game_day: scope.game_day,
            reason: "eligibilityApproved",
        },
    )
    .await?;
    let activation = sqlx::query(
        "UPDATE welfare_application SET status = 'active'
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'approved'",
    )
    .bind(application_id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .execute(&mut **tx)
    .await?;
    ensure!(
        activation.rows_affected() == 1,
        "welfare application was not activated"
    );
    insert_application_transition(
        tx,
        WelfareApplicationTransitionInsert {
            save_id: scope.save_id,
            run_revision: scope.run_revision,
            application_id,
            transition_no: 3,
            from_status: Some("approved"),
            to_status: "active",
            game_day: scope.game_day,
            reason: "paymentScheduled",
        },
    )
    .await?;
    Ok(application_id)
}

async fn insert_welfare_payment_and_settlement(
    tx: &mut Transaction<'_, MySql>,
    scope: &WelfareScopeRow,
    program: &LoadedWelfareProgram,
    application_id: u64,
) -> Result<WelfarePaymentState> {
    let due_game_day = scope
        .game_day
        .checked_add(u32::from(program.payment_delay_game_days))
        .context("welfare payment day overflowed")?;
    let insert = sqlx::query(
        "INSERT INTO welfare_payment
             (save_id, run_revision, application_id, payment_no,
              due_game_day, amount_krw, status)
         VALUES (?, ?, ?, 1, ?, ?, 'pending')",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(application_id)
    .bind(due_game_day)
    .bind(program.benefit_krw)
    .execute(&mut **tx)
    .await?;
    let payment_id = insert.last_insert_id();
    ensure!(payment_id > 0, "welfare payment has no identity");
    let payload = serde_json::to_string(&WelfareSettlementPayload {
        version: 1,
        welfare_payment_id: ResourceId::from_u64(payment_id),
        application_id: ResourceId::from_u64(application_id),
        payment_no: 1,
    })?;
    let settlement = sqlx::query(
        "INSERT INTO scheduled_settlement
             (save_id, run_revision, due_game_day, kind, payload,
              source_kind, source_id, occurrence, status)
         VALUES (?, ?, ?, 'welfareBenefitPayment', ?, 'welfarePayment', ?, 1, 'pending')",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(due_game_day)
    .bind(payload)
    .bind(payment_id.to_string())
    .execute(&mut **tx)
    .await?;
    let settlement_id = settlement.last_insert_id();
    let payment_link = sqlx::query(
        "UPDATE welfare_payment SET scheduled_settlement_id = ?
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND status = 'pending' AND scheduled_settlement_id IS NULL",
    )
    .bind(settlement_id)
    .bind(payment_id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .execute(&mut **tx)
    .await?;
    ensure!(
        payment_link.rows_affected() == 1,
        "welfare payment was not linked to its settlement"
    );
    Ok(WelfarePaymentState {
        id: ResourceId::from_u64(payment_id),
        payment_no: 1,
        amount_krw: program.benefit_krw,
        due_game_day,
        status: WelfarePaymentStatusState::Pending,
    })
}

async fn insert_application_transition(
    tx: &mut Transaction<'_, MySql>,
    transition: WelfareApplicationTransitionInsert<'_>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO welfare_application_transition
             (save_id, run_revision, application_id, transition_no,
              from_status, to_status, transition_game_day, transition_reason)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(transition.save_id)
    .bind(transition.run_revision)
    .bind(transition.application_id)
    .bind(transition.transition_no)
    .bind(transition.from_status)
    .bind(transition.to_status)
    .bind(transition.game_day)
    .bind(transition.reason)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn read_stored_welfare_receipt(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    command_id: &str,
) -> Result<StoredWelfareReceiptRow> {
    sqlx::query_as(
        "SELECT command_kind, payload_sha256, CAST(result AS CHAR) AS result_json,
                ledger_transaction_id
         FROM command_receipt WHERE save_id = ? AND command_id = ? FOR SHARE",
    )
    .bind(save_id)
    .bind(command_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("welfare command identity has no final receipt")
}

fn fact(
    key: &str,
    value_type: WelfareValueType,
    window: WelfareResolvedWindow,
    value: WelfareEvidenceValue,
) -> WelfareFactEvidence {
    WelfareFactEvidence {
        key: key.to_owned(),
        value_type,
        window,
        value,
    }
}

fn welfare_enum(schema_key: &str, value: &str) -> WelfareValue {
    WelfareValue::Enum(crate::life::WelfareEnumValue {
        schema_key: schema_key.to_owned(),
        value: value.to_owned(),
    })
}

fn authority(authority: &str, revision: impl ToString) -> WelfareAuthorityRevision {
    WelfareAuthorityRevision {
        authority: authority.to_owned(),
        revision: revision.to_string(),
    }
}

fn truth_names(truth: WelfareTruth) -> (&'static str, Option<&'static str>) {
    match truth {
        WelfareTruth::True => ("passed", None),
        WelfareTruth::False => ("failed", None),
        WelfareTruth::Unknown(reason) => ("unknown", Some(unknown_reason_name(reason))),
    }
}

const fn unknown_reason_name(reason: WelfareUnknownReason) -> &'static str {
    match reason {
        WelfareUnknownReason::AuthorityMissing => "authorityMissing",
        WelfareUnknownReason::ValuationUnavailable => "valuationUnavailable",
        WelfareUnknownReason::CollectionLimitExceeded => "collectionLimitExceeded",
        WelfareUnknownReason::WindowIncomplete => "windowIncomplete",
        WelfareUnknownReason::ArithmeticOverflow => "arithmeticOverflow",
    }
}

fn evaluation_status_name(status: WelfareEvaluationStatus) -> Result<&'static str> {
    match status {
        WelfareEvaluationStatus::Eligible => Ok("eligible"),
        WelfareEvaluationStatus::Ineligible => Ok("ineligible"),
        WelfareEvaluationStatus::Indeterminate => Ok("indeterminate"),
        WelfareEvaluationStatus::NotEvaluated => {
            bail!("not-evaluated welfare result cannot be persisted")
        }
    }
}

fn parse_evaluation_status(raw: &str) -> Result<WelfareEvaluationStatusState> {
    match raw {
        "eligible" => Ok(WelfareEvaluationStatusState::Eligible),
        "ineligible" => Ok(WelfareEvaluationStatusState::Ineligible),
        "indeterminate" => Ok(WelfareEvaluationStatusState::Indeterminate),
        _ => bail!("invalid welfare evaluation status"),
    }
}

fn condition_state(row: WelfareEvaluationConditionRow) -> Result<WelfareConditionResultState> {
    let outcome = match row.outcome.as_str() {
        "passed" => WelfareConditionOutcomeState::Passed,
        "failed" => WelfareConditionOutcomeState::Failed,
        "unknown" => WelfareConditionOutcomeState::Unknown,
        _ => bail!("invalid welfare condition outcome"),
    };
    Ok(WelfareConditionResultState {
        code: row.condition_code,
        label: row.public_label,
        outcome,
    })
}

fn public_conditions(
    program: &LoadedWelfareProgram,
    evaluation: &WelfareEvaluation,
) -> Result<Vec<WelfareConditionResultState>> {
    program
        .conditions
        .iter()
        .zip(&evaluation.conditions)
        .map(|(catalog, result)| {
            ensure!(
                catalog.condition_code == result.code,
                "welfare condition order drifted"
            );
            Ok(WelfareConditionResultState {
                code: result.code.clone(),
                label: catalog.public_label.clone(),
                outcome: match result.result {
                    WelfareTruth::True => WelfareConditionOutcomeState::Passed,
                    WelfareTruth::False => WelfareConditionOutcomeState::Failed,
                    WelfareTruth::Unknown(_) => WelfareConditionOutcomeState::Unknown,
                },
            })
        })
        .collect()
}

fn application_summary(row: WelfareApplicationRow) -> Result<WelfareApplicationSummaryState> {
    Ok(WelfareApplicationSummaryState {
        id: ResourceId::from_u64(row.id),
        status: parse_application_status(&row.status)?,
        application_game_day: row.application_game_day,
        approval_game_day: row.approval_game_day,
        paid_krw: row.paid_krw,
    })
}

fn parse_application_status(raw: &str) -> Result<WelfareApplicationStatusState> {
    match raw {
        "applied" => Ok(WelfareApplicationStatusState::Applied),
        "approved" => Ok(WelfareApplicationStatusState::Approved),
        "rejected" => Ok(WelfareApplicationStatusState::Rejected),
        "active" => Ok(WelfareApplicationStatusState::Active),
        "exhausted" => Ok(WelfareApplicationStatusState::Exhausted),
        "terminated" => Ok(WelfareApplicationStatusState::Terminated),
        _ => bail!("invalid welfare application status"),
    }
}

fn payment_state(row: WelfarePaymentRow) -> Result<WelfarePaymentState> {
    let status = match row.status.as_str() {
        "pending" => WelfarePaymentStatusState::Pending,
        "paid" => WelfarePaymentStatusState::Paid,
        "cancelled" => WelfarePaymentStatusState::Cancelled,
        _ => bail!("invalid welfare payment status"),
    };
    Ok(WelfarePaymentState {
        id: ResourceId::from_u64(row.id),
        payment_no: u16::from(row.payment_no),
        amount_krw: row.amount_krw,
        due_game_day: row.due_game_day,
        status,
    })
}

fn active_application_state(
    row: ActiveWelfareApplicationRow,
) -> Result<ActiveWelfareApplicationState> {
    ensure!(
        row.status == "active",
        "non-active welfare application entered the active snapshot"
    );
    let next_payment = match (
        row.payment_id,
        row.payment_no,
        row.payment_amount_krw,
        row.payment_due_game_day,
        row.payment_status,
    ) {
        (Some(id), Some(payment_no), Some(amount_krw), Some(due_game_day), Some(status)) => {
            Some(payment_state(WelfarePaymentRow {
                id,
                payment_no,
                amount_krw,
                due_game_day,
                status,
            })?)
        }
        (None, None, None, None, None) => None,
        _ => bail!("active welfare payment projection is partial"),
    };
    ensure!(
        next_payment.is_some(),
        "active welfare application has no pending payment"
    );
    Ok(ActiveWelfareApplicationState {
        application_id: ResourceId::from_u64(row.application_id),
        program_version_id: ResourceId::from_u64(row.program_version_id),
        program_key: row.program_key,
        display_name: row.display_name,
        status: WelfareApplicationStatusState::Active,
        application_game_day: row.application_game_day,
        approval_game_day: row.approval_game_day,
        benefit_krw: row.benefit_krw,
        paid_krw: row.paid_krw,
        next_payment,
    })
}

fn has_current_cursor(scope: &WelfareScopeRow, cursor: crate::finance::CommandCursor) -> bool {
    scope.run_revision == cursor.expected_run_revision
        && scope.state_revision == cursor.expected_state_revision
        && scope.game_day == cursor.expected_game_day
}

fn apply_welfare_fingerprint(command: &ApplyWelfareProgramCommand) -> String {
    hex_sha256(&format!(
        "lifeledger.life.applyWelfareProgram.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\nprogramVersionId={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.program_version_id,
    ))
}

fn hex_sha256(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finance::{CommandCursor, CommandId};

    mod context_welfare_command_identity {
        use super::*;

        #[test]
        fn given_same_cursor_and_program_when_fingerprinted_then_result_is_stable() {
            let command = ApplyWelfareProgramCommand {
                command_id: CommandId::parse("6ec2a078-72ca-4265-b0de-269c3ab64bc7")
                    .expect("명령 ID를 만들 수 있어야 한다"),
                cursor: CommandCursor {
                    expected_run_revision: 4,
                    expected_state_revision: 9,
                    expected_game_day: 12,
                },
                program_version_id: ResourceId::from_u64(7),
            };

            let fingerprint = apply_welfare_fingerprint(&command);

            assert_eq!(fingerprint.len(), 64);
            assert!(
                fingerprint
                    .chars()
                    .all(|character| character.is_ascii_hexdigit())
            );
        }
    }

    mod context_welfare_settlement_payload {
        use super::*;
        use crate::finance::{RunId, SettlementSource, SettlementStatus};

        #[test]
        fn given_extra_payload_field_when_validated_then_it_is_rejected() {
            let settlement = ScheduledSettlement {
                id: ResourceId::from_u64(1),
                run: RunId {
                    save_id: ResourceId::from_u64(2),
                    run_revision: 3,
                },
                due_game_day: 4,
                kind: SettlementKind::WelfareBenefitPayment,
                source: SettlementSource {
                    kind: SettlementSourceKind::WelfarePayment,
                    source_id: "5".to_owned(),
                    occurrence: 1,
                },
                status: SettlementStatus::Pending,
                payload: serde_json::json!({
                    "version": 1,
                    "welfarePaymentId": "5",
                    "applicationId": "6",
                    "paymentNo": 1,
                    "amountKrw": 333000,
                }),
            };

            let result = validate_welfare_settlement_envelope(&settlement);

            assert!(result.is_err());
        }
    }

    mod checked_money_total_rule {
        use super::*;

        mod context_합계가_i64_범위인_경우 {
            use super::*;

            #[test]
            fn given_여러_position_when_합산하면_then_정수_합계를_반환한다() {
                let values = [100, 200, 300];

                let result = checked_money_total(&values);

                assert_eq!(result, Some(600));
            }
        }

        mod context_합계가_i64_범위를_넘는_경우 {
            use super::*;

            #[test]
            fn given_큰_position들_when_합산하면_then_overflow를_반환한다() {
                let values = [i64::MAX, 1];

                let result = checked_money_total(&values);

                assert_eq!(result, None);
            }
        }
    }
}
