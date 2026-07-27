//! M4-D2 sealed life-event catalog, planning, resolution, and bounded reads.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail, ensure};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map as JsonMap, Value as JsonValue};
use sha2::{Digest, Sha256};
use sqlx::{MySql, MySqlPool, Transaction};
use time::Date;

use super::insurance::{
    allocate_insurance_claim_for_event_in_tx, pin_insurance_claim_for_event_in_tx,
};
use super::mysql::{
    CommandIdentitySpec, CommandIdentityState, GameCommandReceiptWrite, inspect_command_identity,
    read_state, write_command_identity, write_game_command_receipt, write_ledger_transaction,
};
use super::types::{
    GameCommandCursor, InsuranceCapabilityState, LifeEventCapabilityState, LifeEventChoiceReceipt,
    LifeEventChoiceState, LifeEventDecisionKindState, LifeEventEffectSummaryState,
    LifeEventHistoryItemState, LifeEventResolutionKindState, LifeEventsQueryState,
    LifeEventsReadResult, LifeEventsState, LifeFailureCode, LifeStoreResult, PendingLifeEventState,
    ResolveLifeEventCommand,
};
use crate::finance::{
    FinanceRules, LedgerAccountCode, LedgerPosting, LedgerSource, LedgerSourceKind,
    LedgerTransactionDraft, ResourceId, RunId, RunPolicyContext,
};
use crate::life::{
    InsuranceRules, LIFE_EVENT_MAX_CHOICES, LIFE_EVENT_MAX_DEFINITIONS, LIFE_EVENT_MAX_PENDING,
    LIFE_EVENT_SCHEMA_VERSION, LifeEventCatalog, LifeEventChoiceDefinition,
    LifeEventChoiceResolutionInput, LifeEventDecisionKind, LifeEventDefinition, LifeEventEffect,
    LifeEventEffectAccountCode, LifeEventEffectAst, LifeEventEffectKind, LifeEventEvidenceValue,
    LifeEventExpiryResolutionInput, LifeEventExpression, LifeEventFactDefinition,
    LifeEventFactEvidence, LifeEventFactReference, LifeEventFactSourceKind,
    LifeEventLedgerAccountCode, LifeEventLiteralValue, LifeEventMonthPlanInput,
    LifeEventOccurrence, LifeEventOperand, LifeEventResolutionKind, LifeEventRules, LifeEventUnit,
    LifeEventUnknownReason, LifeEventValue, LifeEventValueType, LifeEventWindowKind, YearMonth,
};

const COMMAND_KIND_RESOLVE_LIFE_EVENT: &str = "resolveLifeEvent";
const HISTORY_PAGE_SIZE: usize = 20;
const HISTORY_QUERY_BOUND: usize = HISTORY_PAGE_SIZE + 1;
const MAX_CATALOG_FACTS: usize = 16;
const MAX_CATALOG_CHOICES: usize = LIFE_EVENT_MAX_DEFINITIONS * LIFE_EVENT_MAX_CHOICES;
const MAX_OCCURRENCE_HISTORY: usize = LIFE_EVENT_MAX_DEFINITIONS * 255;
const LIFE_EVENT_CURSOR_VERSION: u8 = 1;
const LIFE_EVENT_CURSOR_PAYLOAD_BYTES: usize = 1 + 8 + 4 + 8 + 4 + 8;
const LIFE_EVENT_CURSOR_CHECKSUM_BYTES: usize = 16;
const LIFE_EVENT_CURSOR_BYTES: usize =
    LIFE_EVENT_CURSOR_PAYLOAD_BYTES + LIFE_EVENT_CURSOR_CHECKSUM_BYTES;
const LIFE_EVENT_CURSOR_DOMAIN: &[u8] = b"lifeledger.life.events.cursor.v1\0";

#[derive(Debug, Clone, sqlx::FromRow)]
struct LifeEventScopeRow {
    save_id: u64,
    market_world_id: u64,
    world_seed: u64,
    policy_set_id: u64,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
    cash_krw: i64,
    life_catalog_set_id: u64,
    life_event_component_version_id: u64,
    component_version_key: String,
    availability: String,
    component_sealed: bool,
    catalog_sealed: bool,
    insurance_version_key: String,
    insurance_availability: String,
    insurance_sealed: bool,
    has_character: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CatalogFactRow {
    id: u64,
    fact_order: u8,
    fact_key: String,
    value_type: String,
    unit: String,
    enum_schema_key: Option<String>,
    window_kind: String,
    source_schema_version: u16,
    source_kind: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CatalogDefinitionRow {
    id: u64,
    schema_version: u16,
    entropy_stream_version: u16,
    event_order: u8,
    event_key: String,
    display_name: String,
    purpose: String,
    ranked_availability: String,
    eligibility_ast_json: String,
    ast_node_count: u16,
    ast_max_depth: u8,
    hazard_ppm: u32,
    cooldown_game_days: u16,
    maximum_occurrences: u16,
    priority: u16,
    exclusive_group_key: Option<String>,
    offer_duration_game_days: u16,
    default_choice_key: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CatalogChoiceRow {
    id: u64,
    life_event_definition_id: u64,
    choice_order: u8,
    choice_key: String,
    display_name: String,
    decision_kind: String,
    effect_kind: String,
    effect_amount_krw: Option<i64>,
    effect_account_code: Option<String>,
    effect_ast_json: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PendingHeaderRow {
    id: u64,
    event_key: String,
    display_name: String,
    offered_game_day: u32,
    expires_game_day: u32,
    default_choice_id: u64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PublicChoiceRow {
    life_event_instance_id: u64,
    id: u64,
    choice_order: u8,
    display_name: String,
    decision_kind: String,
    effect_kind: String,
    effect_amount_krw: Option<i64>,
    effect_account_code: Option<String>,
    effect_ast_json: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct HistoryRow {
    id: u64,
    event_key: String,
    display_name: String,
    offered_game_day: u32,
    resolved_game_day: u32,
    resolution_kind: String,
    choice_id: u64,
    choice_display_name: String,
    choice_decision_kind: String,
    choice_effect_kind: String,
    choice_effect_amount_krw: Option<i64>,
    choice_effect_account_code: Option<String>,
    choice_effect_ast_json: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct EventInstanceRow {
    id: u64,
    life_event_definition_id: u64,
    offered_game_day: u32,
    expires_game_day: u32,
    status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct StoredLifeEventReceiptRow {
    command_kind: String,
    payload_sha256: String,
    result_json: String,
    ledger_transaction_id: Option<u64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct FactAuthorityRow {
    age_years: Option<i64>,
    dependent_count: i64,
    residence_count: i64,
    military_status: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PriorOccurrenceRow {
    life_event_definition_id: u64,
    occurrence_no: u16,
    offered_game_day: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HistoryCursor {
    save_id: u64,
    run_revision: u32,
    component_version_id: u64,
    resolved_game_day: u32,
    event_instance_id: u64,
}

pub(super) async fn read_life_events(
    pool: &MySqlPool,
    rules: &dyn LifeEventRules,
    user_id: u64,
    query: LifeEventsQueryState,
) -> Result<LifeEventsReadResult> {
    let mut tx = pool.begin().await?;
    let Some(scope) = read_scope_for_user(&mut tx, user_id, false).await? else {
        tx.commit().await?;
        return Ok(LifeEventsReadResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    if !scope.has_character {
        tx.commit().await?;
        return Ok(LifeEventsReadResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    }
    if !component_is_active(&scope)? {
        if query.cursor.is_some() {
            tx.commit().await?;
            return Ok(LifeEventsReadResult::Rejected(
                LifeFailureCode::InvalidCommand,
            ));
        }
        tx.commit().await?;
        return Ok(LifeEventsReadResult::Found(LifeEventsState {
            capability: LifeEventCapabilityState::Unavailable,
            insurance_capability: InsuranceCapabilityState::Unavailable,
            pending_events: Vec::new(),
            history: Vec::new(),
            next_cursor: None,
        }));
    }

    let catalog = load_catalog(&mut tx, &scope).await?;
    rules
        .validate_catalog(&catalog)
        .context("stored life-event catalog is invalid")?;
    let cursor = match query.cursor.as_deref() {
        Some(raw) => match decode_history_cursor(raw) {
            Ok(cursor) if cursor_matches_scope(cursor, &scope) => Some(cursor),
            _ => {
                tx.commit().await?;
                return Ok(LifeEventsReadResult::Rejected(
                    LifeFailureCode::InvalidCommand,
                ));
            }
        },
        None => None,
    };
    if let Some(cursor) = cursor
        && !history_cursor_anchor_exists(&mut tx, &scope, cursor).await?
    {
        tx.commit().await?;
        return Ok(LifeEventsReadResult::Rejected(
            LifeFailureCode::InvalidCommand,
        ));
    }

    let pending_events = read_pending_for_scope(&mut tx, &scope).await?;
    let (history, next_cursor) = read_history_page(&mut tx, &scope, cursor).await?;
    tx.commit().await?;
    Ok(LifeEventsReadResult::Found(LifeEventsState {
        capability: LifeEventCapabilityState::DeterministicChoices,
        insurance_capability: insurance_capability(&scope)?,
        pending_events,
        history,
        next_cursor,
    }))
}

pub(super) async fn resolve_life_event(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    event_rules: &dyn LifeEventRules,
    insurance_rules: &dyn InsuranceRules,
    user_id: u64,
    command: &ResolveLifeEventCommand,
) -> Result<LifeStoreResult<LifeEventChoiceReceipt>> {
    let fingerprint = resolve_command_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(scope) = read_scope_for_user(&mut tx, user_id, true).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::EventNotFound));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_RESOLVE_LIFE_EVENT,
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
                read_stored_receipt(&mut tx, scope.save_id, command.command_id.as_str()).await?;
            ensure!(
                row.command_kind == COMMAND_KIND_RESOLVE_LIFE_EVENT
                    && row.payload_sha256 == fingerprint,
                "stored life-event receipt disagrees with its command"
            );
            let mut receipt: LifeEventChoiceReceipt = serde_json::from_str(&row.result_json)
                .context("stored life-event receipt is invalid")?;
            ensure!(
                !receipt.replayed
                    && receipt.command_id == command.command_id
                    && receipt.event_id == command.event_id
                    && receipt.choice_id == command.choice_id
                    && (receipt.wallet_delta_krw == 0) == row.ledger_transaction_id.is_none(),
                "stored life-event result disagrees with its command"
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
    if !component_is_active(&scope)? {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::EventNotFound));
    }
    let Some(instance) = read_owned_instance_for_update(&mut tx, &scope, command.event_id).await?
    else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::EventNotFound));
    };
    if instance.status != "offered" || scope.game_day >= instance.expires_game_day {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::EventExpired));
    }

    let catalog = load_catalog(&mut tx, &scope).await?;
    event_rules
        .validate_catalog(&catalog)
        .context("stored life-event catalog is invalid")?;
    let plan = match event_rules.resolve_choice(LifeEventChoiceResolutionInput {
        catalog: &catalog,
        event_definition_id: ResourceId::from_u64(instance.life_event_definition_id),
        event_instance_id: command.event_id,
        offered_game_day: instance.offered_game_day,
        expires_game_day: instance.expires_game_day,
        choice_id: command.choice_id,
        current_game_day: scope.game_day,
        wallet_cash_krw: scope.cash_krw,
    }) {
        Ok(plan) => plan,
        Err(crate::life::LifeEventError::ChoiceNotFound) => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
        }
        Err(crate::life::LifeEventError::EventExpired) => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(LifeFailureCode::EventExpired));
        }
        Err(crate::life::LifeEventError::InsufficientWalletCash) => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(
                LifeFailureCode::InsufficientWalletCash,
            ));
        }
        Err(error) => return Err(error).context("life-event choice could not be planned"),
    };
    ensure!(
        plan.event_instance_id == command.event_id
            && plan.choice_id == command.choice_id
            && plan.resolved_game_day == scope.game_day
            && plan.effect.wallet_cash_before_krw == scope.cash_krw
            && plan
                .effect
                .wallet_cash_before_krw
                .checked_add(plan.effect.wallet_delta_krw)
                == Some(plan.effect.wallet_cash_after_krw)
            && matches!(
                plan.resolution_kind,
                LifeEventResolutionKind::Accepted | LifeEventResolutionKind::Declined
            ),
        "life-event resolution plan escaped its command scope"
    );

    write_command_identity(&mut tx, scope.save_id, &identity).await?;
    let decision_status = resolution_kind_db(plan.resolution_kind)?;
    insert_resolution_transition(
        &mut tx,
        &scope,
        instance.id,
        2,
        "offered",
        decision_status,
        plan.choice_id.get(),
        Some(command.command_id.as_str()),
        plan.resolved_game_day,
        "playerChoice",
    )
    .await?;

    let ledger_transaction_id = if plan.effect.postings.is_empty() {
        ensure!(
            plan.effect.wallet_delta_krw == 0
                && plan.effect.wallet_cash_after_krw == scope.cash_krw,
            "no-effect life-event choice changed the wallet"
        );
        None
    } else {
        let postings = plan
            .effect
            .postings
            .iter()
            .map(|posting| {
                let account_code = match posting.account_code {
                    LifeEventLedgerAccountCode::LifeEventExpense => {
                        LedgerAccountCode::LifeEventExpense
                    }
                    LifeEventLedgerAccountCode::Wallet => LedgerAccountCode::Wallet,
                };
                LedgerPosting {
                    account_code,
                    financial_account_id: None,
                    amount_krw: posting.amount_krw,
                }
            })
            .collect();
        let ledger = finance_rules
            .create_ledger_transaction(LedgerTransactionDraft {
                policy: RunPolicyContext {
                    run: RunId {
                        save_id: ResourceId::from_u64(scope.save_id),
                        run_revision: scope.run_revision,
                    },
                    policy_set_id: ResourceId::from_u64(scope.policy_set_id),
                },
                source: LedgerSource {
                    kind: LedgerSourceKind::LifeEventChoice,
                    source_id: instance.id.to_string(),
                },
                game_day: scope.game_day,
                description: "생애 사건 선택".to_owned(),
                postings,
            })
            .context("life-event ledger is invalid")?;
        Some(write_ledger_transaction(&mut tx, &ledger).await?)
    };
    let transition_reason = if ledger_transaction_id.is_some() {
        "effectApplied"
    } else {
        "noEffectResolved"
    };
    insert_resolution_transition(
        &mut tx,
        &scope,
        instance.id,
        3,
        decision_status,
        "resolved",
        plan.choice_id.get(),
        Some(command.command_id.as_str()),
        plan.resolved_game_day,
        transition_reason,
    )
    .await?;
    project_resolved_instance(
        &mut tx,
        &scope,
        instance.id,
        decision_status,
        plan.choice_id.get(),
        Some(command.command_id.as_str()),
        plan.resolved_game_day,
        ledger_transaction_id,
    )
    .await?;
    allocate_insurance_claim_for_event_in_tx(&mut tx, insurance_rules, scope.save_id, instance.id)
        .await?;

    let committed_state_revision = scope
        .state_revision
        .checked_add(1)
        .context("life-event state revision overflowed")?;
    let save_update = sqlx::query(
        "UPDATE save SET cash_krw = ?, state_revision = ?
         WHERE id = ? AND run_revision = ? AND state_revision = ?
           AND game_day = ? AND cash_krw = ?",
    )
    .bind(plan.effect.wallet_cash_after_krw)
    .bind(committed_state_revision)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.state_revision)
    .bind(scope.game_day)
    .bind(scope.cash_krw)
    .execute(&mut *tx)
    .await?;
    ensure!(
        save_update.rows_affected() == 1,
        "life-event command lost its cursor"
    );

    let receipt = LifeEventChoiceReceipt {
        command_id: command.command_id.clone(),
        event_id: command.event_id,
        choice_id: command.choice_id,
        resolution_kind: decision_kind_state(plan.resolution_kind)?,
        resolved_game_day: plan.resolved_game_day,
        wallet_delta_krw: plan.effect.wallet_delta_krw,
        replayed: false,
    };
    write_game_command_receipt(
        &mut tx,
        GameCommandReceiptWrite {
            save_id: scope.save_id,
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_RESOLVE_LIFE_EVENT,
            payload_sha256: &fingerprint,
            market_world_id: scope.market_world_id,
            committed_cursor: GameCommandCursor {
                run_revision: scope.run_revision,
                state_revision: committed_state_revision,
                game_day: scope.game_day,
            },
            result: &receipt,
            ledger_transaction_id,
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

pub(super) async fn read_pending_life_events_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
) -> Result<Vec<PendingLifeEventState>> {
    let Some(scope) = read_scope_for_save(tx, save_id, false).await? else {
        bail!("life-event snapshot lost its save scope");
    };
    if !component_is_active(&scope)? {
        return Ok(Vec::new());
    }
    read_pending_for_scope(tx, &scope).await
}

pub(super) async fn ensure_life_event_month_plan_in_tx(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn LifeEventRules,
    insurance_rules: &dyn InsuranceRules,
    save_id: u64,
    target_game_day: u32,
) -> Result<()> {
    let Some(scope) = read_scope_for_save(tx, save_id, true).await? else {
        bail!("life-event planner lost its save scope");
    };
    ensure_target_day(&scope, target_game_day)?;
    if !scope.has_character || !component_is_active(&scope)? {
        return Ok(());
    }
    let target_date: Option<Date> = sqlx::query_scalar(
        "SELECT market_date FROM market_daily
         WHERE world_id = ? AND game_day = ?",
    )
    .bind(scope.market_world_id)
    .bind(target_game_day)
    .fetch_optional(&mut **tx)
    .await?;
    let target_date = target_date.context("life-event target market day is missing")?;
    if target_date.day() != 1 {
        return Ok(());
    }
    let year_month = YearMonth {
        year: target_date.year(),
        month: u8::from(target_date.month()),
    };
    ensure!(year_month.is_valid(), "life-event target month is invalid");
    let year_month_db = format!("{:04}-{:02}", year_month.year, year_month.month);
    let existing: Option<(u64, String, u32)> = sqlx::query_as(
        "SELECT id, status, target_game_day
         FROM life_event_month_plan
         WHERE save_id = ? AND run_revision = ?
           AND life_event_component_version_id = ? AND BINARY `year_month` = BINARY ?
         FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.life_event_component_version_id)
    .bind(&year_month_db)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((_id, status, stored_target_day)) = existing {
        ensure!(
            status == "completed" && stored_target_day == target_game_day,
            "stored life-event month plan is incomplete or disagrees with its period"
        );
        return Ok(());
    }

    let catalog = load_catalog(tx, &scope).await?;
    rules
        .validate_catalog(&catalog)
        .context("stored life-event catalog is invalid")?;
    let facts = collect_fact_evidence(tx, &scope, &catalog, target_game_day).await?;
    let fact_fingerprint = fact_fingerprint(&catalog, target_game_day, &facts)?;
    let occurrence_rows: Vec<PriorOccurrenceRow> = sqlx::query_as(
        "SELECT life_event_definition_id, occurrence_no, offered_game_day
         FROM life_event_instance
         WHERE save_id = ? AND run_revision = ?
           AND life_event_component_version_id = ?
         ORDER BY life_event_definition_id, occurrence_no
         LIMIT 8161",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.life_event_component_version_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        occurrence_rows.len() <= MAX_OCCURRENCE_HISTORY,
        "life-event occurrence history exceeded its bound"
    );
    let occurrences = occurrence_rows
        .into_iter()
        .map(|row| LifeEventOccurrence {
            event_definition_id: ResourceId::from_u64(row.life_event_definition_id),
            occurrence_no: row.occurrence_no,
            offered_game_day: row.offered_game_day,
        })
        .collect::<Vec<_>>();
    let pending_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM life_event_instance
         WHERE save_id = ? AND run_revision = ?
           AND life_event_component_version_id = ? AND status = 'offered'",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.life_event_component_version_id)
    .fetch_one(&mut **tx)
    .await?;
    let pending_count = usize::try_from(pending_count)
        .context("life-event pending count is negative or too large")?;
    ensure!(
        pending_count <= LIFE_EVENT_MAX_PENDING,
        "life-event pending count exceeded its invariant"
    );
    let plan = rules
        .plan_month(LifeEventMonthPlanInput {
            catalog: &catalog,
            world_seed: scope.world_seed,
            save_id: ResourceId::from_u64(scope.save_id),
            run_revision: scope.run_revision,
            year_month,
            target_game_day,
            authority_state_revision: scope.state_revision,
            eligibility_fact_fingerprint: &fact_fingerprint,
            facts: &facts,
            prior_occurrences: &occurrences,
            existing_pending_count: pending_count,
        })
        .context("life-event month could not be planned")?;
    ensure!(
        plan.save_id == ResourceId::from_u64(scope.save_id)
            && plan.run_revision == scope.run_revision
            && plan.component_version_id
                == ResourceId::from_u64(scope.life_event_component_version_id)
            && plan.year_month == year_month
            && plan.target_game_day == target_game_day
            && plan.authority_state_revision == scope.state_revision
            && plan.candidates.len() == catalog.definitions.len()
            && plan.offers.len() <= LIFE_EVENT_MAX_PENDING,
        "life-event month plan escaped its authority scope"
    );

    let definition_count =
        u8::try_from(plan.candidates.len()).context("life-event definition count is too large")?;
    let offered_count =
        u8::try_from(plan.offers.len()).context("life-event offer count is too large")?;
    let plan_insert = sqlx::query(
        "INSERT INTO life_event_month_plan
             (save_id, run_revision, life_catalog_set_id,
              life_event_component_version_id, `year_month`, target_game_day,
              authority_state_revision, fact_registry_schema_version,
              entropy_stream_version, definition_count, offered_count, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'planning')",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.life_catalog_set_id)
    .bind(scope.life_event_component_version_id)
    .bind(&year_month_db)
    .bind(target_game_day)
    .bind(scope.state_revision)
    .bind(plan.fact_registry_schema_version)
    .bind(plan.entropy_stream_version)
    .bind(definition_count)
    .bind(offered_count)
    .execute(&mut **tx)
    .await?;
    ensure!(
        plan_insert.rows_affected() == 1,
        "life-event month plan was not inserted"
    );
    let month_plan_id = plan_insert.last_insert_id();

    let mut candidate_ids = BTreeMap::new();
    for candidate in &plan.candidates {
        let result = enum_to_db(&candidate.result)?;
        let unknown_reason = candidate
            .unknown_reason
            .as_ref()
            .map(enum_to_db)
            .transpose()?;
        let insert = sqlx::query(
            "INSERT INTO life_event_candidate
                 (save_id, run_revision, month_plan_id,
                  life_event_component_version_id, life_event_definition_id,
                  candidate_order, occurrence_no, eligibility_fact_fingerprint,
                  candidate_result, unknown_reason, roll_ppm)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(month_plan_id)
        .bind(scope.life_event_component_version_id)
        .bind(candidate.event_definition_id.get())
        .bind(candidate.candidate_order)
        .bind(candidate.occurrence_no)
        .bind(&candidate.eligibility_fact_fingerprint)
        .bind(result)
        .bind(unknown_reason)
        .bind(candidate.roll_ppm)
        .execute(&mut **tx)
        .await?;
        ensure!(
            insert.rows_affected() == 1,
            "life-event candidate was not inserted"
        );
        ensure!(
            candidate_ids
                .insert(candidate.event_definition_id, insert.last_insert_id())
                .is_none(),
            "life-event candidate definition was duplicated"
        );
    }
    for offer in &plan.offers {
        let candidate_id = *candidate_ids
            .get(&offer.event_definition_id)
            .context("life-event offer has no candidate")?;
        let insert = sqlx::query(
            "INSERT INTO life_event_instance
                 (save_id, run_revision, life_catalog_set_id,
                  life_event_component_version_id, life_event_definition_id,
                  month_plan_id, candidate_id, occurrence_no,
                  offered_game_day, expires_game_day, status)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'offered')",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(scope.life_catalog_set_id)
        .bind(scope.life_event_component_version_id)
        .bind(offer.event_definition_id.get())
        .bind(month_plan_id)
        .bind(candidate_id)
        .bind(offer.occurrence_no)
        .bind(offer.offered_game_day)
        .bind(offer.expires_game_day)
        .execute(&mut **tx)
        .await?;
        ensure!(
            insert.rows_affected() == 1,
            "life-event instance was not inserted"
        );
        let instance_id = insert.last_insert_id();
        let transition = sqlx::query(
            "INSERT INTO life_event_transition
                 (save_id, run_revision, life_event_instance_id, transition_no,
                  from_status, to_status, choice_id, command_id,
                  transition_game_day, transition_reason)
             VALUES (?, ?, ?, 1, NULL, 'offered', NULL, NULL, ?, 'monthlyPlanner')",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(instance_id)
        .bind(offer.offered_game_day)
        .execute(&mut **tx)
        .await?;
        ensure!(
            transition.rows_affected() == 1,
            "life-event offer transition was not inserted"
        );
        pin_insurance_claim_for_event_in_tx(tx, insurance_rules, scope.save_id, instance_id)
            .await?;
    }
    let complete = sqlx::query(
        "UPDATE life_event_month_plan SET status = 'completed'
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'planning'",
    )
    .bind(month_plan_id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .execute(&mut **tx)
    .await?;
    ensure!(
        complete.rows_affected() == 1,
        "life-event month plan was not completed"
    );
    Ok(())
}

pub(super) async fn expire_life_events_in_tx(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn LifeEventRules,
    insurance_rules: &dyn InsuranceRules,
    save_id: u64,
    target_game_day: u32,
) -> Result<()> {
    let Some(scope) = read_scope_for_save(tx, save_id, true).await? else {
        bail!("life-event expiry lost its save scope");
    };
    ensure_target_day(&scope, target_game_day)?;
    if !scope.has_character || !component_is_active(&scope)? {
        return Ok(());
    }
    let catalog = load_catalog(tx, &scope).await?;
    rules
        .validate_catalog(&catalog)
        .context("stored life-event catalog is invalid")?;
    let due: Vec<EventInstanceRow> = sqlx::query_as(
        "SELECT id, life_event_definition_id, offered_game_day, expires_game_day, status
         FROM life_event_instance
         WHERE save_id = ? AND run_revision = ?
           AND life_event_component_version_id = ?
           AND status = 'offered' AND expires_game_day = ?
         ORDER BY id
         LIMIT 9
         FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.life_event_component_version_id)
    .bind(target_game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        due.len() <= LIFE_EVENT_MAX_PENDING,
        "life-event expiry set exceeded the pending bound"
    );
    for instance in due {
        ensure!(
            instance.status == "offered",
            "life-event expiry lock changed status"
        );
        let plan = rules
            .resolve_expired(LifeEventExpiryResolutionInput {
                catalog: &catalog,
                event_definition_id: ResourceId::from_u64(instance.life_event_definition_id),
                event_instance_id: ResourceId::from_u64(instance.id),
                offered_game_day: instance.offered_game_day,
                expires_game_day: instance.expires_game_day,
                current_game_day: target_game_day,
                wallet_cash_krw: scope.cash_krw,
            })
            .context("life-event expiry could not be planned")?;
        ensure!(
            plan.event_instance_id == ResourceId::from_u64(instance.id)
                && plan.resolution_kind == LifeEventResolutionKind::Expired
                && plan.resolved_game_day == target_game_day
                && plan.effect.wallet_delta_krw == 0
                && plan.effect.postings.is_empty(),
            "life-event default expiry was not a no-effect resolution"
        );
        insert_resolution_transition(
            tx,
            &scope,
            instance.id,
            2,
            "offered",
            "expired",
            plan.choice_id.get(),
            None,
            target_game_day,
            "offerExpired",
        )
        .await?;
        insert_resolution_transition(
            tx,
            &scope,
            instance.id,
            3,
            "expired",
            "resolved",
            plan.choice_id.get(),
            None,
            target_game_day,
            "noEffectResolved",
        )
        .await?;
        project_resolved_instance(
            tx,
            &scope,
            instance.id,
            "expired",
            plan.choice_id.get(),
            None,
            target_game_day,
            None,
        )
        .await?;
        allocate_insurance_claim_for_event_in_tx(tx, insurance_rules, scope.save_id, instance.id)
            .await?;
    }
    Ok(())
}

async fn read_scope_for_user(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
    lock: bool,
) -> Result<Option<LifeEventScopeRow>> {
    let row = if lock {
        sqlx::query_as(
            "SELECT save.id AS save_id, save.market_world_id, world.seed AS world_seed,
                    bundle.policy_set_id, save.run_revision, save.state_revision,
                    save.game_day, save.cash_krw, bundle.life_catalog_set_id,
                    catalog.life_event_component_version_id,
                    component.version_key AS component_version_key,
                    component.availability,
                    component.sealed_at IS NOT NULL AS component_sealed,
                    catalog.sealed_at IS NOT NULL AS catalog_sealed,
                    insurance.version_key AS insurance_version_key,
                    insurance.availability AS insurance_availability,
                    insurance.sealed_at IS NOT NULL AS insurance_sealed,
                    EXISTS(SELECT 1 FROM `character`
                           WHERE `character`.save_id = save.id) AS has_character
             FROM save
             INNER JOIN market_world AS world ON world.id = save.market_world_id
             INNER JOIN run_rule_bundle AS bundle
               ON bundle.save_id = save.id AND bundle.run_revision = save.run_revision
             INNER JOIN life_catalog_set AS catalog ON catalog.id = bundle.life_catalog_set_id
             INNER JOIN life_component_version AS component
               ON component.id = catalog.life_event_component_version_id
              AND component.component_kind = 'lifeEvent'
             INNER JOIN life_component_version AS insurance
               ON insurance.id = catalog.insurance_component_version_id
              AND insurance.component_kind = 'insurance'
             WHERE save.user_id = ?
             FOR UPDATE",
        )
        .bind(user_id)
        .fetch_optional(&mut **tx)
        .await?
    } else {
        sqlx::query_as(
            "SELECT save.id AS save_id, save.market_world_id, world.seed AS world_seed,
                    bundle.policy_set_id, save.run_revision, save.state_revision,
                    save.game_day, save.cash_krw, bundle.life_catalog_set_id,
                    catalog.life_event_component_version_id,
                    component.version_key AS component_version_key,
                    component.availability,
                    component.sealed_at IS NOT NULL AS component_sealed,
                    catalog.sealed_at IS NOT NULL AS catalog_sealed,
                    insurance.version_key AS insurance_version_key,
                    insurance.availability AS insurance_availability,
                    insurance.sealed_at IS NOT NULL AS insurance_sealed,
                    EXISTS(SELECT 1 FROM `character`
                           WHERE `character`.save_id = save.id) AS has_character
             FROM save
             INNER JOIN market_world AS world ON world.id = save.market_world_id
             INNER JOIN run_rule_bundle AS bundle
               ON bundle.save_id = save.id AND bundle.run_revision = save.run_revision
             INNER JOIN life_catalog_set AS catalog ON catalog.id = bundle.life_catalog_set_id
             INNER JOIN life_component_version AS component
               ON component.id = catalog.life_event_component_version_id
              AND component.component_kind = 'lifeEvent'
             INNER JOIN life_component_version AS insurance
               ON insurance.id = catalog.insurance_component_version_id
              AND insurance.component_kind = 'insurance'
             WHERE save.user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(&mut **tx)
        .await?
    };
    if let Some(scope) = &row {
        ensure_insurance_pin(scope)?;
    }
    Ok(row)
}

async fn read_scope_for_save(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    lock: bool,
) -> Result<Option<LifeEventScopeRow>> {
    let row = if lock {
        sqlx::query_as(
            "SELECT save.id AS save_id, save.market_world_id, world.seed AS world_seed,
                    bundle.policy_set_id, save.run_revision, save.state_revision,
                    save.game_day, save.cash_krw, bundle.life_catalog_set_id,
                    catalog.life_event_component_version_id,
                    component.version_key AS component_version_key,
                    component.availability,
                    component.sealed_at IS NOT NULL AS component_sealed,
                    catalog.sealed_at IS NOT NULL AS catalog_sealed,
                    insurance.version_key AS insurance_version_key,
                    insurance.availability AS insurance_availability,
                    insurance.sealed_at IS NOT NULL AS insurance_sealed,
                    EXISTS(SELECT 1 FROM `character`
                           WHERE `character`.save_id = save.id) AS has_character
             FROM save
             INNER JOIN market_world AS world ON world.id = save.market_world_id
             INNER JOIN run_rule_bundle AS bundle
               ON bundle.save_id = save.id AND bundle.run_revision = save.run_revision
             INNER JOIN life_catalog_set AS catalog ON catalog.id = bundle.life_catalog_set_id
             INNER JOIN life_component_version AS component
               ON component.id = catalog.life_event_component_version_id
              AND component.component_kind = 'lifeEvent'
             INNER JOIN life_component_version AS insurance
               ON insurance.id = catalog.insurance_component_version_id
              AND insurance.component_kind = 'insurance'
             WHERE save.id = ?
             FOR UPDATE",
        )
        .bind(save_id)
        .fetch_optional(&mut **tx)
        .await?
    } else {
        sqlx::query_as(
            "SELECT save.id AS save_id, save.market_world_id, world.seed AS world_seed,
                    bundle.policy_set_id, save.run_revision, save.state_revision,
                    save.game_day, save.cash_krw, bundle.life_catalog_set_id,
                    catalog.life_event_component_version_id,
                    component.version_key AS component_version_key,
                    component.availability,
                    component.sealed_at IS NOT NULL AS component_sealed,
                    catalog.sealed_at IS NOT NULL AS catalog_sealed,
                    insurance.version_key AS insurance_version_key,
                    insurance.availability AS insurance_availability,
                    insurance.sealed_at IS NOT NULL AS insurance_sealed,
                    EXISTS(SELECT 1 FROM `character`
                           WHERE `character`.save_id = save.id) AS has_character
             FROM save
             INNER JOIN market_world AS world ON world.id = save.market_world_id
             INNER JOIN run_rule_bundle AS bundle
               ON bundle.save_id = save.id AND bundle.run_revision = save.run_revision
             INNER JOIN life_catalog_set AS catalog ON catalog.id = bundle.life_catalog_set_id
             INNER JOIN life_component_version AS component
               ON component.id = catalog.life_event_component_version_id
              AND component.component_kind = 'lifeEvent'
             INNER JOIN life_component_version AS insurance
               ON insurance.id = catalog.insurance_component_version_id
              AND insurance.component_kind = 'insurance'
             WHERE save.id = ?",
        )
        .bind(save_id)
        .fetch_optional(&mut **tx)
        .await?
    };
    if let Some(scope) = &row {
        ensure_insurance_pin(scope)?;
    }
    Ok(row)
}

fn ensure_insurance_pin(scope: &LifeEventScopeRow) -> Result<()> {
    match scope.insurance_availability.as_str() {
        "disabled" => {
            ensure!(
                scope.insurance_sealed,
                "current run does not pin a sealed disabled insurance component"
            );
        }
        "active" => {
            ensure!(
                scope.insurance_sealed
                    && scope.insurance_version_key == "dev-unranked-m4-insurance-2026-v1",
                "current run pins an unsupported active insurance component"
            );
        }
        _ => bail!("stored insurance availability is invalid"),
    }
    Ok(())
}

fn insurance_capability(scope: &LifeEventScopeRow) -> Result<InsuranceCapabilityState> {
    ensure_insurance_pin(scope)?;
    match scope.insurance_availability.as_str() {
        "active" => Ok(InsuranceCapabilityState::ContractsAndClaims),
        "disabled" => Ok(InsuranceCapabilityState::Unavailable),
        _ => bail!("stored insurance availability is invalid"),
    }
}

fn component_is_active(scope: &LifeEventScopeRow) -> Result<bool> {
    match scope.availability.as_str() {
        "active" => {
            ensure!(
                scope.component_sealed && scope.catalog_sealed,
                "active life-event component or catalog is not sealed"
            );
            Ok(true)
        }
        "disabled" => Ok(false),
        _ => bail!("stored life-event availability is invalid"),
    }
}

fn has_current_cursor(scope: &LifeEventScopeRow, cursor: crate::finance::CommandCursor) -> bool {
    scope.run_revision == cursor.expected_run_revision
        && scope.state_revision == cursor.expected_state_revision
        && scope.game_day == cursor.expected_game_day
}

fn ensure_target_day(scope: &LifeEventScopeRow, target_game_day: u32) -> Result<()> {
    let next_game_day = scope
        .game_day
        .checked_add(1)
        .context("life-event target day overflowed")?;
    ensure!(
        target_game_day == scope.game_day || target_game_day == next_game_day,
        "life-event target day is outside the locked player-day transaction"
    );
    Ok(())
}

async fn read_pending_for_scope(
    tx: &mut Transaction<'_, MySql>,
    scope: &LifeEventScopeRow,
) -> Result<Vec<PendingLifeEventState>> {
    let headers: Vec<PendingHeaderRow> = sqlx::query_as(
        "SELECT instance.id, definition.event_key, definition.display_name,
                instance.offered_game_day, instance.expires_game_day,
                default_choice.id AS default_choice_id
         FROM life_event_instance AS instance
         INNER JOIN life_event_definition AS definition
           ON definition.id = instance.life_event_definition_id
          AND definition.life_component_version_id = instance.life_event_component_version_id
         INNER JOIN life_event_choice AS default_choice
           ON default_choice.life_component_version_id = instance.life_event_component_version_id
          AND default_choice.life_event_definition_id = definition.id
          AND BINARY default_choice.choice_key = BINARY definition.default_choice_key
         WHERE instance.save_id = ? AND instance.run_revision = ?
           AND instance.life_event_component_version_id = ? AND instance.status = 'offered'
         ORDER BY instance.id
         LIMIT 9",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.life_event_component_version_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        headers.len() <= LIFE_EVENT_MAX_PENDING,
        "pending life events exceeded their public bound"
    );
    if headers.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<PublicChoiceRow> = sqlx::query_as(
        "SELECT instance.id AS life_event_instance_id, choice_row.id,
                choice_row.choice_order, choice_row.display_name,
                choice_row.decision_kind, choice_row.effect_kind,
                choice_row.effect_amount_krw, choice_row.effect_account_code,
                CAST(choice_row.effect_ast AS CHAR) AS effect_ast_json
         FROM life_event_instance AS instance
         INNER JOIN life_event_choice AS choice_row
           ON choice_row.life_component_version_id = instance.life_event_component_version_id
          AND choice_row.life_event_definition_id = instance.life_event_definition_id
         WHERE instance.save_id = ? AND instance.run_revision = ?
           AND instance.life_event_component_version_id = ? AND instance.status = 'offered'
         ORDER BY instance.id, choice_row.choice_order
         LIMIT 65",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.life_event_component_version_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() <= LIFE_EVENT_MAX_PENDING * LIFE_EVENT_MAX_CHOICES,
        "pending life-event choices exceeded their public bound"
    );
    let mut grouped = BTreeMap::<u64, Vec<(u8, LifeEventChoiceState)>>::new();
    for row in rows {
        let instance_id = row.life_event_instance_id;
        let choice_order = row.choice_order;
        grouped
            .entry(instance_id)
            .or_default()
            .push((choice_order, public_choice(row)?));
    }
    let mut states = Vec::with_capacity(headers.len());
    for header in headers {
        let ordered = grouped
            .remove(&header.id)
            .context("pending life event has no choices")?;
        ensure!(
            (2..=LIFE_EVENT_MAX_CHOICES).contains(&ordered.len()),
            "pending life event has invalid choice cardinality"
        );
        let mut choices = Vec::with_capacity(ordered.len());
        for (index, (choice_order, choice)) in ordered.into_iter().enumerate() {
            ensure!(
                choice_order
                    == u8::try_from(index + 1).context("life-event choice order overflowed")?,
                "pending life-event choices are not contiguous"
            );
            choices.push(choice);
        }
        ensure!(
            choices.iter().any(|choice| {
                choice.id == ResourceId::from_u64(header.default_choice_id)
                    && choice.effect_summary == LifeEventEffectSummaryState::NoEffect
            }),
            "pending life-event default choice is invalid"
        );
        states.push(PendingLifeEventState {
            id: ResourceId::from_u64(header.id),
            event_key: header.event_key,
            display_name: header.display_name,
            offered_game_day: header.offered_game_day,
            expires_game_day: header.expires_game_day,
            default_choice_id: ResourceId::from_u64(header.default_choice_id),
            choices,
        });
    }
    ensure!(
        grouped.is_empty(),
        "pending life-event choice escaped its header bound"
    );
    Ok(states)
}

fn public_choice(row: PublicChoiceRow) -> Result<LifeEventChoiceState> {
    Ok(LifeEventChoiceState {
        id: ResourceId::from_u64(row.id),
        display_name: row.display_name,
        decision_kind: match parse_db_enum::<LifeEventDecisionKind>(&row.decision_kind)? {
            LifeEventDecisionKind::Accepted => LifeEventDecisionKindState::Accepted,
            LifeEventDecisionKind::Declined => LifeEventDecisionKindState::Declined,
        },
        effect_summary: public_effect_summary(
            &row.effect_kind,
            row.effect_amount_krw,
            row.effect_account_code.as_deref(),
            &row.effect_ast_json,
        )?,
    })
}

fn public_effect_summary(
    effect_kind: &str,
    effect_amount_krw: Option<i64>,
    effect_account_code: Option<&str>,
    effect_ast_json: &str,
) -> Result<LifeEventEffectSummaryState> {
    let ast: LifeEventEffectAst =
        serde_json::from_str(effect_ast_json).context("stored life-event effect AST is invalid")?;
    ensure!(
        ast.version == LIFE_EVENT_SCHEMA_VERSION,
        "stored life-event effect schema version is unsupported"
    );
    match (
        parse_db_enum::<LifeEventEffectKind>(effect_kind)?,
        effect_amount_krw,
        effect_account_code,
        ast.effect,
    ) {
        (LifeEventEffectKind::NoEffect, None, None, LifeEventEffect::NoEffect) => {
            Ok(LifeEventEffectSummaryState::NoEffect)
        }
        (
            LifeEventEffectKind::FixedWalletExpense,
            Some(projected_amount),
            Some(raw_account),
            LifeEventEffect::FixedWalletExpense {
                amount_krw,
                account_code: LifeEventEffectAccountCode::LifeEventExpense,
            },
        ) if projected_amount == amount_krw
            && amount_krw > 0
            && parse_db_enum::<LifeEventEffectAccountCode>(raw_account)?
                == LifeEventEffectAccountCode::LifeEventExpense =>
        {
            Ok(LifeEventEffectSummaryState::WalletExpense { amount_krw })
        }
        _ => bail!("stored life-event effect projection is invalid"),
    }
}

async fn read_history_page(
    tx: &mut Transaction<'_, MySql>,
    scope: &LifeEventScopeRow,
    cursor: Option<HistoryCursor>,
) -> Result<(Vec<LifeEventHistoryItemState>, Option<String>)> {
    let mut rows: Vec<HistoryRow> = if let Some(cursor) = cursor {
        sqlx::query_as(
            "SELECT instance.id, definition.event_key, definition.display_name,
                    instance.offered_game_day, instance.resolved_game_day,
                    instance.resolution_kind, choice_row.id AS choice_id,
                    choice_row.display_name AS choice_display_name,
                    choice_row.decision_kind AS choice_decision_kind,
                    choice_row.effect_kind AS choice_effect_kind,
                    choice_row.effect_amount_krw AS choice_effect_amount_krw,
                    choice_row.effect_account_code AS choice_effect_account_code,
                    CAST(choice_row.effect_ast AS CHAR) AS choice_effect_ast_json
             FROM life_event_instance AS instance
             INNER JOIN life_event_definition AS definition
               ON definition.id = instance.life_event_definition_id
              AND definition.life_component_version_id = instance.life_event_component_version_id
             INNER JOIN life_event_choice AS choice_row
               ON choice_row.id = instance.resolved_choice_id
              AND choice_row.life_event_definition_id = instance.life_event_definition_id
              AND choice_row.life_component_version_id = instance.life_event_component_version_id
             WHERE instance.save_id = ? AND instance.run_revision = ?
               AND instance.life_event_component_version_id = ?
               AND instance.status = 'resolved'
               AND (instance.resolved_game_day < ?
                    OR (instance.resolved_game_day = ? AND instance.id < ?))
             ORDER BY instance.resolved_game_day DESC, instance.id DESC
             LIMIT 21",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(scope.life_event_component_version_id)
        .bind(cursor.resolved_game_day)
        .bind(cursor.resolved_game_day)
        .bind(cursor.event_instance_id)
        .fetch_all(&mut **tx)
        .await?
    } else {
        sqlx::query_as(
            "SELECT instance.id, definition.event_key, definition.display_name,
                    instance.offered_game_day, instance.resolved_game_day,
                    instance.resolution_kind, choice_row.id AS choice_id,
                    choice_row.display_name AS choice_display_name,
                    choice_row.decision_kind AS choice_decision_kind,
                    choice_row.effect_kind AS choice_effect_kind,
                    choice_row.effect_amount_krw AS choice_effect_amount_krw,
                    choice_row.effect_account_code AS choice_effect_account_code,
                    CAST(choice_row.effect_ast AS CHAR) AS choice_effect_ast_json
             FROM life_event_instance AS instance
             INNER JOIN life_event_definition AS definition
               ON definition.id = instance.life_event_definition_id
              AND definition.life_component_version_id = instance.life_event_component_version_id
             INNER JOIN life_event_choice AS choice_row
               ON choice_row.id = instance.resolved_choice_id
              AND choice_row.life_event_definition_id = instance.life_event_definition_id
              AND choice_row.life_component_version_id = instance.life_event_component_version_id
             WHERE instance.save_id = ? AND instance.run_revision = ?
               AND instance.life_event_component_version_id = ?
               AND instance.status = 'resolved'
             ORDER BY instance.resolved_game_day DESC, instance.id DESC
             LIMIT 21",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(scope.life_event_component_version_id)
        .fetch_all(&mut **tx)
        .await?
    };
    ensure!(
        rows.len() <= HISTORY_QUERY_BOUND,
        "life-event history query escaped its bound"
    );
    let has_more = rows.len() == HISTORY_QUERY_BOUND;
    if has_more {
        rows.truncate(HISTORY_PAGE_SIZE);
    }
    let next_cursor = if has_more {
        let last = rows
            .last()
            .context("life-event history page unexpectedly has no cursor anchor")?;
        Some(encode_history_cursor(HistoryCursor {
            save_id: scope.save_id,
            run_revision: scope.run_revision,
            component_version_id: scope.life_event_component_version_id,
            resolved_game_day: last.resolved_game_day,
            event_instance_id: last.id,
        }))
    } else {
        None
    };
    let history = rows
        .into_iter()
        .map(|row| {
            let resolution_kind =
                match parse_db_enum::<LifeEventResolutionKind>(&row.resolution_kind)? {
                    LifeEventResolutionKind::Accepted => LifeEventResolutionKindState::Accepted,
                    LifeEventResolutionKind::Declined => LifeEventResolutionKindState::Declined,
                    LifeEventResolutionKind::Expired => LifeEventResolutionKindState::Expired,
                };
            let choice = public_choice(PublicChoiceRow {
                life_event_instance_id: row.id,
                id: row.choice_id,
                choice_order: 1,
                display_name: row.choice_display_name,
                decision_kind: row.choice_decision_kind,
                effect_kind: row.choice_effect_kind,
                effect_amount_krw: row.choice_effect_amount_krw,
                effect_account_code: row.choice_effect_account_code,
                effect_ast_json: row.choice_effect_ast_json,
            })?;
            ensure!(
                matches!(
                    (resolution_kind, choice.decision_kind),
                    (
                        LifeEventResolutionKindState::Accepted,
                        LifeEventDecisionKindState::Accepted
                    ) | (
                        LifeEventResolutionKindState::Declined,
                        LifeEventDecisionKindState::Declined
                    ) | (LifeEventResolutionKindState::Expired, _)
                ),
                "life-event history decision projection is inconsistent"
            );
            Ok(LifeEventHistoryItemState {
                id: ResourceId::from_u64(row.id),
                event_key: row.event_key,
                display_name: row.display_name,
                offered_game_day: row.offered_game_day,
                resolved_game_day: row.resolved_game_day,
                resolution_kind,
                choice,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((history, next_cursor))
}

async fn history_cursor_anchor_exists(
    tx: &mut Transaction<'_, MySql>,
    scope: &LifeEventScopeRow,
    cursor: HistoryCursor,
) -> Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM life_event_instance
             WHERE id = ? AND save_id = ? AND run_revision = ?
               AND life_event_component_version_id = ? AND status = 'resolved'
               AND resolved_game_day = ?
         )",
    )
    .bind(cursor.event_instance_id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.life_event_component_version_id)
    .bind(cursor.resolved_game_day)
    .fetch_one(&mut **tx)
    .await
    .context("failed to validate a life-event history cursor")
}

fn encode_history_cursor(cursor: HistoryCursor) -> String {
    let mut payload = Vec::with_capacity(LIFE_EVENT_CURSOR_BYTES);
    payload.push(LIFE_EVENT_CURSOR_VERSION);
    payload.extend_from_slice(&cursor.save_id.to_be_bytes());
    payload.extend_from_slice(&cursor.run_revision.to_be_bytes());
    payload.extend_from_slice(&cursor.component_version_id.to_be_bytes());
    payload.extend_from_slice(&cursor.resolved_game_day.to_be_bytes());
    payload.extend_from_slice(&cursor.event_instance_id.to_be_bytes());
    let checksum = cursor_checksum(&payload);
    payload.extend_from_slice(&checksum[..LIFE_EVENT_CURSOR_CHECKSUM_BYTES]);
    URL_SAFE_NO_PAD.encode(payload)
}

fn decode_history_cursor(raw: &str) -> Result<HistoryCursor> {
    ensure!(!raw.is_empty() && raw.len() <= 512 && raw.is_ascii());
    let decoded = URL_SAFE_NO_PAD
        .decode(raw)
        .context("life-event history cursor is not canonical base64url")?;
    ensure!(
        decoded.len() == LIFE_EVENT_CURSOR_BYTES,
        "life-event history cursor has an invalid length"
    );
    let (payload, checksum) = decoded.split_at(LIFE_EVENT_CURSOR_PAYLOAD_BYTES);
    ensure!(
        checksum == &cursor_checksum(payload)[..LIFE_EVENT_CURSOR_CHECKSUM_BYTES],
        "life-event history cursor checksum is invalid"
    );
    ensure!(
        payload[0] == LIFE_EVENT_CURSOR_VERSION,
        "life-event history cursor version is unsupported"
    );
    let save_id = read_u64(&payload[1..9])?;
    let run_revision = read_u32(&payload[9..13])?;
    let component_version_id = read_u64(&payload[13..21])?;
    let resolved_game_day = read_u32(&payload[21..25])?;
    let event_instance_id = read_u64(&payload[25..33])?;
    ensure!(
        save_id != 0 && component_version_id != 0 && event_instance_id != 0,
        "life-event history cursor contains a zero identifier"
    );
    ensure!(
        encode_history_cursor(HistoryCursor {
            save_id,
            run_revision,
            component_version_id,
            resolved_game_day,
            event_instance_id,
        }) == raw,
        "life-event history cursor is not canonically encoded"
    );
    Ok(HistoryCursor {
        save_id,
        run_revision,
        component_version_id,
        resolved_game_day,
        event_instance_id,
    })
}

fn cursor_checksum(payload: &[u8]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(LIFE_EVENT_CURSOR_DOMAIN);
    digest.update(payload);
    digest.finalize().into()
}

fn read_u64(bytes: &[u8]) -> Result<u64> {
    let bytes: [u8; 8] = bytes
        .try_into()
        .context("life-event cursor u64 field has an invalid width")?;
    Ok(u64::from_be_bytes(bytes))
}

fn read_u32(bytes: &[u8]) -> Result<u32> {
    let bytes: [u8; 4] = bytes
        .try_into()
        .context("life-event cursor u32 field has an invalid width")?;
    Ok(u32::from_be_bytes(bytes))
}

fn cursor_matches_scope(cursor: HistoryCursor, scope: &LifeEventScopeRow) -> bool {
    cursor.save_id == scope.save_id
        && cursor.run_revision == scope.run_revision
        && cursor.component_version_id == scope.life_event_component_version_id
}

async fn load_catalog(
    tx: &mut Transaction<'_, MySql>,
    scope: &LifeEventScopeRow,
) -> Result<LifeEventCatalog> {
    ensure!(component_is_active(scope)?);
    let manifest_valid: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1
             FROM life_component_version AS component
             INNER JOIN life_component_canonical_manifest AS manifest
               ON manifest.life_component_version_id = component.id
              AND BINARY manifest.canonical_sha256 = BINARY component.canonical_sha256
              AND BINARY manifest.canonical_sha256 = BINARY SHA2(manifest.canonical_json, 256)
             WHERE component.id = ? AND component.component_kind = 'lifeEvent'
               AND component.availability = 'active' AND component.sealed_at IS NOT NULL
         )",
    )
    .bind(scope.life_event_component_version_id)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        manifest_valid,
        "sealed life-event component manifest is invalid"
    );

    let fact_rows: Vec<CatalogFactRow> = sqlx::query_as(
        "SELECT id, fact_order, fact_key, value_type, unit, enum_schema_key,
                window_kind, source_schema_version, source_kind
         FROM life_event_fact_definition
         WHERE life_component_version_id = ?
         ORDER BY fact_order
         LIMIT 17",
    )
    .bind(scope.life_event_component_version_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        !fact_rows.is_empty() && fact_rows.len() <= MAX_CATALOG_FACTS,
        "life-event fact registry cardinality is invalid"
    );
    let fact_registry_schema_version = fact_rows[0].source_schema_version;
    let mut fact_ids = BTreeSet::new();
    let mut fact_keys = BTreeSet::new();
    let facts = fact_rows
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            ensure!(
                row.fact_order
                    == u8::try_from(index + 1).context("life-event fact order overflowed")?
                    && row.source_schema_version == fact_registry_schema_version
                    && fact_ids.insert(row.id)
                    && fact_keys.insert(row.fact_key.clone()),
                "life-event fact registry order, version, or identity is invalid"
            );
            Ok(LifeEventFactDefinition {
                id: ResourceId::from_u64(row.id),
                fact_order: row.fact_order,
                fact_key: row.fact_key,
                value_type: parse_db_enum(&row.value_type)?,
                unit: parse_db_enum(&row.unit)?,
                enum_schema_key: row.enum_schema_key,
                window_kind: parse_db_enum(&row.window_kind)?,
                source_schema_version: row.source_schema_version,
                source_kind: parse_db_enum(&row.source_kind)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let definition_rows: Vec<CatalogDefinitionRow> = sqlx::query_as(
        "SELECT id, schema_version, entropy_stream_version, event_order,
                event_key, display_name, purpose, ranked_availability,
                CAST(eligibility_ast AS CHAR) AS eligibility_ast_json,
                ast_node_count, ast_max_depth, hazard_ppm, cooldown_game_days,
                maximum_occurrences, priority, exclusive_group_key,
                offer_duration_game_days, default_choice_key
         FROM life_event_definition
         WHERE life_component_version_id = ?
         ORDER BY event_key, id
         LIMIT 33",
    )
    .bind(scope.life_event_component_version_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        !definition_rows.is_empty() && definition_rows.len() <= LIFE_EVENT_MAX_DEFINITIONS,
        "life-event definition cardinality is invalid"
    );
    let choice_rows: Vec<CatalogChoiceRow> = sqlx::query_as(
        "SELECT choice_row.id, choice_row.life_event_definition_id,
                choice_row.choice_order, choice_row.choice_key,
                choice_row.display_name, choice_row.decision_kind,
                choice_row.effect_kind, choice_row.effect_amount_krw,
                choice_row.effect_account_code,
                CAST(choice_row.effect_ast AS CHAR) AS effect_ast_json
         FROM life_event_choice AS choice_row
         INNER JOIN life_event_definition AS definition
           ON definition.id = choice_row.life_event_definition_id
          AND definition.life_component_version_id = choice_row.life_component_version_id
         WHERE choice_row.life_component_version_id = ?
         ORDER BY definition.event_key, choice_row.choice_order, choice_row.id
         LIMIT 257",
    )
    .bind(scope.life_event_component_version_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        choice_rows.len() <= MAX_CATALOG_CHOICES,
        "life-event catalog choices exceeded their bound"
    );
    let mut choices_by_definition = BTreeMap::<u64, Vec<LifeEventChoiceDefinition>>::new();
    let mut choice_ids = BTreeSet::new();
    for row in choice_rows {
        ensure!(
            choice_ids.insert(row.id),
            "life-event catalog choice ID is duplicated"
        );
        let effect_ast: LifeEventEffectAst = serde_json::from_str(&row.effect_ast_json)
            .context("stored life-event effect AST is invalid JSON")?;
        choices_by_definition
            .entry(row.life_event_definition_id)
            .or_default()
            .push(LifeEventChoiceDefinition {
                id: ResourceId::from_u64(row.id),
                choice_order: row.choice_order,
                choice_key: row.choice_key,
                display_name: row.display_name,
                decision_kind: parse_db_enum(&row.decision_kind)?,
                effect_kind: parse_db_enum(&row.effect_kind)?,
                effect_amount_krw: row.effect_amount_krw,
                effect_account_code: row
                    .effect_account_code
                    .as_deref()
                    .map(parse_db_enum)
                    .transpose()?,
                effect_ast,
            });
    }
    let mut definition_ids = BTreeSet::new();
    let mut definition_keys = BTreeSet::new();
    let definitions = definition_rows
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            ensure!(
                row.event_order
                    == u8::try_from(index + 1).context("life-event definition order overflowed")?
                    && definition_ids.insert(row.id)
                    && definition_keys.insert(row.event_key.clone()),
                "life-event definition order or identity is invalid"
            );
            let choices = choices_by_definition
                .remove(&row.id)
                .context("life-event definition has no choices")?;
            ensure!(
                (2..=LIFE_EVENT_MAX_CHOICES).contains(&choices.len()),
                "life-event choice cardinality is invalid"
            );
            Ok(LifeEventDefinition {
                id: ResourceId::from_u64(row.id),
                schema_version: row.schema_version,
                entropy_stream_version: row.entropy_stream_version,
                event_order: row.event_order,
                event_key: row.event_key,
                display_name: row.display_name,
                purpose: parse_db_enum(&row.purpose)?,
                ranked_availability: parse_db_enum(&row.ranked_availability)?,
                eligibility_ast: parse_eligibility_ast(&row.eligibility_ast_json)?,
                ast_node_count: row.ast_node_count,
                ast_max_depth: row.ast_max_depth,
                hazard_ppm: row.hazard_ppm,
                cooldown_game_days: row.cooldown_game_days,
                maximum_occurrences: row.maximum_occurrences,
                priority: row.priority,
                exclusive_group_key: row.exclusive_group_key,
                offer_duration_game_days: row.offer_duration_game_days,
                default_choice_key: row.default_choice_key,
                choices,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        choices_by_definition.is_empty(),
        "life-event catalog choice has no definition"
    );
    Ok(LifeEventCatalog {
        component_version_id: ResourceId::from_u64(scope.life_event_component_version_id),
        component_version_key: scope.component_version_key.clone(),
        fact_registry_schema_version,
        facts,
        definitions,
    })
}

pub(super) fn parse_eligibility_ast(raw: &str) -> Result<crate::life::LifeEventEligibilityAst> {
    let value: JsonValue =
        serde_json::from_str(raw).context("stored life-event eligibility AST is invalid JSON")?;
    let mut object = into_json_object(value, "life-event eligibility AST")?;
    let version = take_json_u16(&mut object, "version")?;
    let root = parse_expression(JsonValue::Object(object))?;
    Ok(crate::life::LifeEventEligibilityAst { version, root })
}

fn parse_expression(value: JsonValue) -> Result<LifeEventExpression> {
    let mut object = into_json_object(value, "life-event expression")?;
    let kind = take_json_string(&mut object, "kind")?;
    match kind.as_str() {
        "all" | "any" => {
            let children = take_json_array(&mut object, "children")?
                .into_iter()
                .map(parse_expression)
                .collect::<Result<Vec<_>>>()?;
            finish_json_object(&object, "life-event logical expression")?;
            if kind == "all" {
                Ok(LifeEventExpression::All { children })
            } else {
                Ok(LifeEventExpression::Any { children })
            }
        }
        "not" => {
            let child = Box::new(parse_expression(take_json_value(&mut object, "child")?)?);
            finish_json_object(&object, "life-event not expression")?;
            Ok(LifeEventExpression::Not { child })
        }
        "eq" | "gte" => {
            let left = Box::new(parse_operand(take_json_value(&mut object, "left")?)?);
            let right = Box::new(parse_operand(take_json_value(&mut object, "right")?)?);
            finish_json_object(&object, "life-event comparison expression")?;
            if kind == "eq" {
                Ok(LifeEventExpression::Eq { left, right })
            } else {
                Ok(LifeEventExpression::Gte { left, right })
            }
        }
        "between" => {
            let value = Box::new(parse_operand(take_json_value(&mut object, "value")?)?);
            let lower = Box::new(parse_operand(take_json_value(&mut object, "lower")?)?);
            let upper = Box::new(parse_operand(take_json_value(&mut object, "upper")?)?);
            finish_json_object(&object, "life-event between expression")?;
            Ok(LifeEventExpression::Between {
                value,
                lower,
                upper,
            })
        }
        "fact" => {
            let reference = parse_fact_reference(&mut object)?;
            finish_json_object(&object, "life-event fact expression")?;
            Ok(LifeEventExpression::Fact { reference })
        }
        _ => bail!("stored life-event expression kind is unsupported"),
    }
}

fn parse_operand(value: JsonValue) -> Result<LifeEventOperand> {
    let mut object = into_json_object(value, "life-event operand")?;
    let kind = take_json_string(&mut object, "kind")?;
    match kind.as_str() {
        "fact" => {
            let reference = parse_fact_reference(&mut object)?;
            finish_json_object(&object, "life-event fact operand")?;
            Ok(LifeEventOperand::Fact { reference })
        }
        "literal" => {
            let value_type = take_json_string(&mut object, "valueType")?;
            let unit = parse_db_enum::<LifeEventUnit>(&take_json_string(&mut object, "unit")?)?;
            let value = match value_type.as_str() {
                "boolean" => LifeEventLiteralValue::Boolean(take_json_bool(&mut object, "value")?),
                "count" => LifeEventLiteralValue::Count(take_json_i64(&mut object, "value")?),
                "ageYears" => LifeEventLiteralValue::AgeYears(take_json_i64(&mut object, "value")?),
                "enum" => LifeEventLiteralValue::Enum {
                    schema_key: take_json_string(&mut object, "schemaKey")?,
                    value: take_json_string(&mut object, "value")?,
                },
                _ => bail!("stored life-event literal type is unsupported"),
            };
            finish_json_object(&object, "life-event literal operand")?;
            Ok(LifeEventOperand::Literal { unit, value })
        }
        _ => bail!("stored life-event operand kind is unsupported"),
    }
}

fn parse_fact_reference(object: &mut JsonMap<String, JsonValue>) -> Result<LifeEventFactReference> {
    let path = take_json_string(object, "path")?;
    let unit = parse_db_enum(&take_json_string(object, "unit")?)?;
    let mut window =
        into_json_object(take_json_value(object, "window")?, "life-event fact window")?;
    let window_kind = take_json_string(&mut window, "kind")?;
    finish_json_object(&window, "life-event fact window")?;
    Ok(LifeEventFactReference {
        path,
        unit,
        window: parse_db_enum(&window_kind)?,
    })
}

fn into_json_object(value: JsonValue, label: &str) -> Result<JsonMap<String, JsonValue>> {
    value
        .as_object()
        .cloned()
        .with_context(|| format!("stored {label} is not an object"))
}

fn take_json_value(object: &mut JsonMap<String, JsonValue>, key: &str) -> Result<JsonValue> {
    object
        .remove(key)
        .with_context(|| format!("stored life-event JSON is missing {key}"))
}

fn take_json_string(object: &mut JsonMap<String, JsonValue>, key: &str) -> Result<String> {
    take_json_value(object, key)?
        .as_str()
        .map(str::to_owned)
        .with_context(|| format!("stored life-event JSON field {key} is not a string"))
}

fn take_json_array(object: &mut JsonMap<String, JsonValue>, key: &str) -> Result<Vec<JsonValue>> {
    take_json_value(object, key)?
        .as_array()
        .cloned()
        .with_context(|| format!("stored life-event JSON field {key} is not an array"))
}

fn take_json_bool(object: &mut JsonMap<String, JsonValue>, key: &str) -> Result<bool> {
    take_json_value(object, key)?
        .as_bool()
        .with_context(|| format!("stored life-event JSON field {key} is not a boolean"))
}

fn take_json_i64(object: &mut JsonMap<String, JsonValue>, key: &str) -> Result<i64> {
    take_json_value(object, key)?
        .as_i64()
        .with_context(|| format!("stored life-event JSON field {key} is not an integer"))
}

fn take_json_u16(object: &mut JsonMap<String, JsonValue>, key: &str) -> Result<u16> {
    let raw = take_json_value(object, key)?
        .as_u64()
        .with_context(|| format!("stored life-event JSON field {key} is not unsigned"))?;
    u16::try_from(raw).with_context(|| format!("stored life-event JSON field {key} is too large"))
}

fn finish_json_object(object: &JsonMap<String, JsonValue>, label: &str) -> Result<()> {
    ensure!(object.is_empty(), "stored {label} has unknown fields");
    Ok(())
}

fn parse_db_enum<T: DeserializeOwned>(raw: &str) -> Result<T> {
    serde_json::from_value(JsonValue::String(raw.to_owned()))
        .with_context(|| format!("stored life-event enum value {raw} is invalid"))
}

fn enum_to_db<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value).context("life-event enum serialization failed")? {
        JsonValue::String(raw) => Ok(raw),
        _ => bail!("life-event enum did not serialize as a string"),
    }
}

async fn collect_fact_evidence(
    tx: &mut Transaction<'_, MySql>,
    scope: &LifeEventScopeRow,
    catalog: &LifeEventCatalog,
    target_game_day: u32,
) -> Result<Vec<LifeEventFactEvidence>> {
    let authority: FactAuthorityRow = sqlx::query_as(
        "SELECT TIMESTAMPDIFF(YEAR, career.birth_date, market.market_date) AS age_years,
                (SELECT COUNT(*) FROM household_member AS member
                 WHERE member.save_id = save.id AND member.run_revision = save.run_revision
                   AND member.member_role <> 'player' AND member.joined_game_day <= ?
                   AND (member.left_game_day IS NULL OR member.left_game_day > ?))
                    AS dependent_count,
                (SELECT COUNT(*) FROM residence
                 WHERE residence.save_id = save.id
                   AND residence.run_revision = save.run_revision
                   AND residence.effective_from_game_day <= ?
                   AND (residence.effective_to_game_day IS NULL
                        OR residence.effective_to_game_day > ?)) AS residence_count,
                CASE
                    WHEN career.military_status IS NULL THEN NULL
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
                END AS military_status
         FROM save
         LEFT JOIN career_run AS career
           ON career.save_id = save.id AND career.run_revision = save.run_revision
         LEFT JOIN market_daily AS market
           ON market.world_id = save.market_world_id AND market.game_day = ?
         WHERE save.id = ? AND save.run_revision = ?",
    )
    .bind(target_game_day)
    .bind(target_game_day)
    .bind(target_game_day)
    .bind(target_game_day)
    .bind(target_game_day)
    .bind(target_game_day)
    .bind(target_game_day)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        authority.dependent_count >= 0 && authority.residence_count >= 0,
        "life-event fact authority returned a negative count"
    );
    ensure!(
        authority.residence_count <= 1,
        "multiple residences are active for life-event eligibility"
    );

    catalog
        .facts
        .iter()
        .map(|definition| {
            let value = match (
                definition.fact_key.as_str(),
                definition.source_kind,
                definition.value_type,
                definition.unit,
            ) {
                (
                    "character.age",
                    LifeEventFactSourceKind::GameDay,
                    LifeEventValueType::AgeYears,
                    LifeEventUnit::Years,
                ) => match authority.age_years {
                    Some(age) if age >= 0 => {
                        LifeEventEvidenceValue::Known(LifeEventValue::AgeYears(age))
                    }
                    Some(_) => {
                        LifeEventEvidenceValue::Unknown(LifeEventUnknownReason::ArithmeticOverflow)
                    }
                    None => {
                        LifeEventEvidenceValue::Unknown(LifeEventUnknownReason::AuthorityMissing)
                    }
                },
                (
                    "household.dependentCount",
                    LifeEventFactSourceKind::Household,
                    LifeEventValueType::Count,
                    LifeEventUnit::Count,
                ) => {
                    if authority.dependent_count > 32 {
                        LifeEventEvidenceValue::Unknown(
                            LifeEventUnknownReason::CollectionLimitExceeded,
                        )
                    } else {
                        LifeEventEvidenceValue::Known(LifeEventValue::Count(
                            authority.dependent_count,
                        ))
                    }
                }
                (
                    "residence.exists",
                    LifeEventFactSourceKind::Residence,
                    LifeEventValueType::Boolean,
                    LifeEventUnit::Boolean,
                ) => LifeEventEvidenceValue::Known(LifeEventValue::Boolean(
                    authority.residence_count == 1,
                )),
                (
                    "military.status",
                    LifeEventFactSourceKind::Military,
                    LifeEventValueType::Enum,
                    LifeEventUnit::Enum,
                ) if definition.enum_schema_key.as_deref() == Some("military") => {
                    match authority.military_status.as_deref() {
                        Some(status) => LifeEventEvidenceValue::Known(LifeEventValue::Enum {
                            schema_key: "military".to_owned(),
                            value: status.to_owned(),
                        }),
                        None => LifeEventEvidenceValue::Unknown(
                            LifeEventUnknownReason::AuthorityMissing,
                        ),
                    }
                }
                _ => bail!(
                    "life-event fact authority adapter is missing for {}",
                    definition.fact_key
                ),
            };
            ensure!(
                definition.window_kind == LifeEventWindowKind::CurrentGameDay,
                "life-event fact window is unsupported by the authority adapter"
            );
            Ok(LifeEventFactEvidence {
                fact_key: definition.fact_key.clone(),
                value,
            })
        })
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FactFingerprintInput<'a> {
    schema_version: u16,
    component_version_id: ResourceId,
    target_game_day: u32,
    facts: Vec<FactFingerprintEntry<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FactFingerprintEntry<'a> {
    fact_key: &'a str,
    value_type: LifeEventValueType,
    unit: LifeEventUnit,
    window: LifeEventWindowKind,
    value: &'a LifeEventEvidenceValue,
}

fn fact_fingerprint(
    catalog: &LifeEventCatalog,
    target_game_day: u32,
    evidence: &[LifeEventFactEvidence],
) -> Result<String> {
    ensure!(
        catalog.facts.len() == evidence.len(),
        "life-event fact fingerprint evidence is incomplete"
    );
    let evidence_by_key = evidence
        .iter()
        .map(|fact| (fact.fact_key.as_str(), &fact.value))
        .collect::<BTreeMap<_, _>>();
    ensure!(
        evidence_by_key.len() == evidence.len(),
        "life-event fact fingerprint evidence is duplicated"
    );
    let mut definitions = catalog.facts.iter().collect::<Vec<_>>();
    definitions.sort_by(|left, right| left.fact_key.cmp(&right.fact_key));
    let facts = definitions
        .into_iter()
        .map(|definition| {
            let value = evidence_by_key
                .get(definition.fact_key.as_str())
                .copied()
                .context("life-event fact fingerprint evidence is missing")?;
            Ok(FactFingerprintEntry {
                fact_key: &definition.fact_key,
                value_type: definition.value_type,
                unit: definition.unit,
                window: definition.window_kind,
                value,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let canonical = serde_json::to_vec(&FactFingerprintInput {
        schema_version: catalog.fact_registry_schema_version,
        component_version_id: catalog.component_version_id,
        target_game_day,
        facts,
    })
    .context("life-event fact fingerprint serialization failed")?;
    Ok(hex_sha256(&canonical))
}

async fn read_owned_instance_for_update(
    tx: &mut Transaction<'_, MySql>,
    scope: &LifeEventScopeRow,
    event_id: ResourceId,
) -> Result<Option<EventInstanceRow>> {
    sqlx::query_as(
        "SELECT id, life_event_definition_id, offered_game_day, expires_game_day, status
         FROM life_event_instance
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND life_event_component_version_id = ?
         FOR UPDATE",
    )
    .bind(event_id.get())
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.life_event_component_version_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock a life-event instance")
}

async fn read_stored_receipt(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    command_id: &str,
) -> Result<StoredLifeEventReceiptRow> {
    sqlx::query_as(
        "SELECT command_kind, payload_sha256, CAST(result AS CHAR) AS result_json,
                ledger_transaction_id
         FROM command_receipt
         WHERE save_id = ? AND command_id = ?
         FOR SHARE",
    )
    .bind(save_id)
    .bind(command_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("life-event command identity has no final receipt")
}

#[allow(clippy::too_many_arguments)]
async fn insert_resolution_transition(
    tx: &mut Transaction<'_, MySql>,
    scope: &LifeEventScopeRow,
    instance_id: u64,
    transition_no: u8,
    from_status: &str,
    to_status: &str,
    choice_id: u64,
    command_id: Option<&str>,
    transition_game_day: u32,
    transition_reason: &str,
) -> Result<()> {
    let insert = sqlx::query(
        "INSERT INTO life_event_transition
             (save_id, run_revision, life_event_instance_id, transition_no,
              from_status, to_status, choice_id, command_id,
              transition_game_day, transition_reason)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(instance_id)
    .bind(transition_no)
    .bind(from_status)
    .bind(to_status)
    .bind(choice_id)
    .bind(command_id)
    .bind(transition_game_day)
    .bind(transition_reason)
    .execute(&mut **tx)
    .await?;
    ensure!(
        insert.rows_affected() == 1,
        "life-event resolution transition was not inserted"
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn project_resolved_instance(
    tx: &mut Transaction<'_, MySql>,
    scope: &LifeEventScopeRow,
    instance_id: u64,
    resolution_kind: &str,
    choice_id: u64,
    command_id: Option<&str>,
    resolved_game_day: u32,
    ledger_transaction_id: Option<u64>,
) -> Result<()> {
    let update = sqlx::query(
        "UPDATE life_event_instance
         SET status = 'resolved', resolution_kind = ?, resolved_choice_id = ?,
             resolution_command_id = ?, resolution_sequence = 1,
             resolved_game_day = ?, ledger_transaction_id = ?
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND life_event_component_version_id = ? AND status = 'offered'",
    )
    .bind(resolution_kind)
    .bind(choice_id)
    .bind(command_id)
    .bind(resolved_game_day)
    .bind(ledger_transaction_id)
    .bind(instance_id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.life_event_component_version_id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "life-event instance projection was not resolved"
    );
    Ok(())
}

fn resolution_kind_db(kind: LifeEventResolutionKind) -> Result<&'static str> {
    match kind {
        LifeEventResolutionKind::Accepted => Ok("accepted"),
        LifeEventResolutionKind::Declined => Ok("declined"),
        LifeEventResolutionKind::Expired => {
            bail!("explicit life-event choice cannot resolve as expired")
        }
    }
}

fn decision_kind_state(kind: LifeEventResolutionKind) -> Result<LifeEventDecisionKindState> {
    match kind {
        LifeEventResolutionKind::Accepted => Ok(LifeEventDecisionKindState::Accepted),
        LifeEventResolutionKind::Declined => Ok(LifeEventDecisionKindState::Declined),
        LifeEventResolutionKind::Expired => {
            bail!("explicit life-event receipt cannot resolve as expired")
        }
    }
}

fn resolve_command_fingerprint(command: &ResolveLifeEventCommand) -> String {
    hex_sha256(
        format!(
            "lifeledger.life.resolveEvent.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\neventId={}\nchoiceId={}",
            command.cursor.expected_run_revision,
            command.cursor.expected_state_revision,
            command.cursor.expected_game_day,
            command.event_id,
            command.choice_id,
        )
        .as_bytes(),
    )
}

fn hex_sha256(value: &[u8]) -> String {
    Sha256::digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finance::{CommandCursor, CommandId};

    mod context_기록_cursor를_왕복하는_경우 {
        use super::*;

        #[test]
        fn given_범위가_묶인_cursor_when_인코딩후_디코딩하면_then_범위_필드가_보존된다() {
            let cursor = HistoryCursor {
                save_id: 41,
                run_revision: 3,
                component_version_id: 97,
                resolved_game_day: 212,
                event_instance_id: 1_003,
            };

            let encoded = encode_history_cursor(cursor);
            let decoded = decode_history_cursor(&encoded)
                .expect("cursor를 canonical하게 되읽을 수 있어야 한다");

            assert_eq!(decoded, cursor);
        }

        #[test]
        fn given_변조된_cursor_when_디코딩하면_then_checksum이_거절한다() {
            let cursor = HistoryCursor {
                save_id: 41,
                run_revision: 3,
                component_version_id: 97,
                resolved_game_day: 212,
                event_instance_id: 1_003,
            };
            let encoded = encode_history_cursor(cursor);
            let mut bytes = encoded.into_bytes();
            let last_index = bytes
                .len()
                .checked_sub(1)
                .expect("cursor가 비어 있지 않아야 한다");
            bytes[last_index] = if bytes[last_index] == b'A' {
                b'B'
            } else {
                b'A'
            };
            let tampered = String::from_utf8(bytes).expect("cursor는 ASCII여야 한다");

            let decoded = decode_history_cursor(&tampered);

            assert!(decoded.is_err());
        }
    }

    mod context_봉인_catalog_ast를_읽는_경우 {
        use super::*;

        fn given_eligibility_ast(extra_root_field: &str) -> String {
            format!(
                r#"{{
                    "version":1,
                    "kind":"all",
                    "children":[
                        {{
                            "kind":"gte",
                            "left":{{
                                "kind":"fact",
                                "path":"household.dependentCount",
                                "unit":"count",
                                "window":{{"kind":"currentGameDay"}}
                            }},
                            "right":{{
                                "kind":"literal",
                                "valueType":"count",
                                "unit":"count",
                                "value":1
                            }}
                        }}
                    ]
                    {extra_root_field}
                }}"#
            )
        }

        #[test]
        fn given_schema_v1_ast_when_파싱하면_then_typed_operand가_복원된다() {
            let raw = given_eligibility_ast("");

            let parsed = parse_eligibility_ast(&raw).expect("schema v1 AST를 읽을 수 있어야 한다");

            assert!(matches!(
                parsed.root,
                LifeEventExpression::All { ref children }
                    if matches!(children.as_slice(), [LifeEventExpression::Gte { .. }])
            ));
        }

        #[test]
        fn given_알수없는_ast_필드_when_파싱하면_then_loader가_닫힌_채_실패한다() {
            let raw = given_eligibility_ast(",\"unexpected\":true");

            let parsed = parse_eligibility_ast(&raw);

            assert!(parsed.is_err());
        }
    }

    mod context_effect_projection을_공개하는_경우 {
        use super::*;

        #[test]
        fn given_일치하는_고정비용_when_요약하면_then_공개_금액만_노출한다() {
            let ast = r#"{"version":1,"kind":"fixedWalletExpense","amountKrw":120000,"accountCode":"lifeEventExpense"}"#;

            let summary = public_effect_summary(
                "fixedWalletExpense",
                Some(120_000),
                Some("lifeEventExpense"),
                ast,
            )
            .expect("일치하는 effect projection을 읽을 수 있어야 한다");

            assert_eq!(
                summary,
                LifeEventEffectSummaryState::WalletExpense {
                    amount_krw: 120_000
                }
            );
        }

        #[test]
        fn given_어긋난_effect_금액_when_요약하면_then_projection을_거절한다() {
            let ast = r#"{"version":1,"kind":"fixedWalletExpense","amountKrw":120000,"accountCode":"lifeEventExpense"}"#;

            let summary = public_effect_summary(
                "fixedWalletExpense",
                Some(119_999),
                Some("lifeEventExpense"),
                ast,
            );

            assert!(summary.is_err());
        }
    }

    mod context_명령_동일성을_판단하는_경우 {
        use super::*;

        fn given_command(choice_id: u64) -> ResolveLifeEventCommand {
            ResolveLifeEventCommand {
                command_id: CommandId::parse("6ec2a078-72ca-4265-b0de-269c3ab64bc7")
                    .expect("명령 ID를 만들 수 있어야 한다"),
                cursor: CommandCursor {
                    expected_run_revision: 4,
                    expected_state_revision: 17,
                    expected_game_day: 33,
                },
                event_id: ResourceId::from_u64(91),
                choice_id: ResourceId::from_u64(choice_id),
            }
        }

        #[test]
        fn given_같은_cursor_사건_선택_when_fingerprint하면_then_digest가_안정적이다() {
            let command = given_command(92);

            let first = resolve_command_fingerprint(&command);
            let second = resolve_command_fingerprint(&command);

            assert_eq!(first, second);
            assert_eq!(first.len(), 64);
        }

        #[test]
        fn given_다른_선택_when_fingerprint하면_then_digest가_달라진다() {
            let first = resolve_command_fingerprint(&given_command(92));

            let second = resolve_command_fingerprint(&given_command(93));

            assert_ne!(first, second);
        }
    }

    mod context_사실_fingerprint를_만드는_경우 {
        use super::*;

        fn given_catalog() -> LifeEventCatalog {
            LifeEventCatalog {
                component_version_id: ResourceId::from_u64(7),
                component_version_key: "test-life-event-v1".to_owned(),
                fact_registry_schema_version: 1,
                facts: vec![
                    LifeEventFactDefinition {
                        id: ResourceId::from_u64(1),
                        fact_order: 1,
                        fact_key: "residence.exists".to_owned(),
                        value_type: LifeEventValueType::Boolean,
                        unit: LifeEventUnit::Boolean,
                        enum_schema_key: None,
                        window_kind: LifeEventWindowKind::CurrentGameDay,
                        source_schema_version: 1,
                        source_kind: LifeEventFactSourceKind::Residence,
                    },
                    LifeEventFactDefinition {
                        id: ResourceId::from_u64(2),
                        fact_order: 2,
                        fact_key: "household.dependentCount".to_owned(),
                        value_type: LifeEventValueType::Count,
                        unit: LifeEventUnit::Count,
                        enum_schema_key: None,
                        window_kind: LifeEventWindowKind::CurrentGameDay,
                        source_schema_version: 1,
                        source_kind: LifeEventFactSourceKind::Household,
                    },
                ],
                definitions: Vec::new(),
            }
        }

        fn given_facts() -> Vec<LifeEventFactEvidence> {
            vec![
                LifeEventFactEvidence {
                    fact_key: "residence.exists".to_owned(),
                    value: LifeEventEvidenceValue::Known(LifeEventValue::Boolean(true)),
                },
                LifeEventFactEvidence {
                    fact_key: "household.dependentCount".to_owned(),
                    value: LifeEventEvidenceValue::Known(LifeEventValue::Count(1)),
                },
            ]
        }

        #[test]
        fn given_같은_사실의_다른_순서_when_fingerprint하면_then_digest가_안정적이다() {
            let catalog = given_catalog();
            let facts = given_facts();
            let mut reversed = facts.clone();
            reversed.reverse();

            let first = fact_fingerprint(&catalog, 31, &facts)
                .expect("사실 fingerprint를 만들 수 있어야 한다");
            let second = fact_fingerprint(&catalog, 31, &reversed)
                .expect("조회 순서와 무관하게 fingerprint를 만들 수 있어야 한다");

            assert_eq!(first, second);
            assert_eq!(first.len(), 64);
        }
    }
}
