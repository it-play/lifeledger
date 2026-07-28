//! M4-D3 sealed insurance catalog, contracts, premiums, and claims.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, json};
use sha2::{Digest, Sha256};
use sqlx::{MySql, MySqlPool, Transaction};

use super::mysql::{
    CommandIdentitySpec, CommandIdentityState, GameCommandReceiptWrite, inspect_command_identity,
    read_state, write_command_identity, write_game_command_receipt, write_ledger_transaction,
};
use super::types::{
    CancelInsuranceContractCommand, EnrollInsuranceContractCommand, FileInsuranceClaimCommand,
    GameCommandCursor, InsuranceCancellationReceipt, InsuranceCapabilityState,
    InsuranceClaimAllocationState, InsuranceClaimHistoryState, InsuranceClaimReceipt,
    InsuranceContractState, InsuranceContractStatusState, InsuranceEligibilityReasonState,
    InsuranceEligibilityStatusState, InsuranceEnrollmentReceipt, InsuranceProductState,
    InsuranceQueryState, InsuranceReadResult, InsuranceSnapshotState, InsuranceState,
    LifeFailureCode, LifeStoreResult, PendingInsuranceClaimState,
};
use crate::finance::{
    FinanceRules, LedgerAccountCode, LedgerPosting, LedgerSource, LedgerSourceKind,
    LedgerTransactionDraft, ResourceId, RunId, RunPolicyContext, ScheduledSettlement,
    SettlementKind, SettlementSourceKind, SettlementStatus,
};
use crate::life::{
    INSURANCE_MAX_ACTIVE_CONTRACTS, INSURANCE_MAX_CLAIM_CONTRACTS, INSURANCE_MAX_PRODUCTS,
    InsuranceCatalog, InsuranceClaimCandidateInput, InsuranceClaimContractAggregatePlan,
    InsuranceClaimContractPin, InsuranceClaimFinalizationContractInput, InsuranceClaimLedgerInput,
    InsuranceClaimPaymentInput, InsuranceClaimResolutionInput, InsuranceClaimResolutionKind,
    InsuranceClaimStatus, InsuranceContractExpiryInput, InsuranceContractPlanInput,
    InsuranceContractStatus, InsuranceEligibilityInput, InsuranceEligibilityStatus, InsuranceError,
    InsuranceLedgerAccountCode, InsuranceLedgerPlan, InsurancePremiumLedgerInput, InsuranceRules,
    InsuranceTerminationInput, InsuranceTerminationKind, LifeEventEvidenceValue,
    LifeEventFactEvidence, LifeEventFactSourceKind, LifeEventUnit, LifeEventUnknownReason,
    LifeEventValue, LifeEventValueType, LifeEventWindowKind,
    create_fictional_family_care_insurance_catalog,
};

const INSURANCE_COMPONENT_KEY: &str = "dev-unranked-m4-insurance-2026-v1";
const COMMAND_KIND_ENROLL: &str = "enrollInsurance";
const COMMAND_KIND_CANCEL: &str = "cancelInsurance";
const COMMAND_KIND_FILE_CLAIM: &str = "fileInsuranceClaim";
const MAX_TRANSACTION_ATTEMPTS: usize = 3;
const PAGE_SIZE: usize = 20;
const QUERY_BOUND: usize = PAGE_SIZE + 1;
const CURSOR_VERSION: u8 = 1;
const CURSOR_PAYLOAD_BYTES: usize = 1 + 8 + 4 + 8 + 1 + 4 + 8 + 4 + 8;
const CURSOR_CHECKSUM_BYTES: usize = 16;
const CURSOR_BYTES: usize = CURSOR_PAYLOAD_BYTES + CURSOR_CHECKSUM_BYTES;
const CURSOR_DOMAIN: &[u8] = b"lifeledger.life.insurance.cursor.v1\0";

#[derive(Debug, Clone, sqlx::FromRow)]
struct InsuranceScopeRow {
    save_id: u64,
    market_world_id: u64,
    policy_set_id: u64,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
    cash_krw: i64,
    life_catalog_set_id: u64,
    life_event_component_version_id: u64,
    insurance_component_version_id: u64,
    component_version_key: String,
    availability: String,
    component_sealed: bool,
    catalog_sealed: bool,
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
struct CatalogProductRow {
    id: u64,
    schema_version: u16,
    product_order: u8,
    product_key: String,
    display_name: String,
    purpose: String,
    ranked_availability: String,
    eligibility_ast_json: String,
    ast_node_count: u16,
    ast_max_depth: u8,
    premium_krw: i64,
    premium_cadence_game_days: u16,
    term_game_days: u16,
    waiting_game_days: u16,
    claim_window_game_days: u16,
    grace_game_days: u16,
    reinstatement_allowed: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CatalogCoverageRow {
    id: u64,
    product_version_id: u64,
    coverage_order: u8,
    coverage_kind: String,
    event_key: String,
    effect_kind: String,
    deductible_krw: i64,
    occurrence_limit_krw: i64,
    term_limit_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct FactAuthorityRow {
    age_years: Option<i64>,
    dependent_count: i64,
    residence_count: i64,
    military_status: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ContractPublicRow {
    id: u64,
    product_version_id: u64,
    product_key: String,
    display_name: String,
    status: String,
    start_game_day: u32,
    coverage_start_game_day: u32,
    waiting_ends_game_day: u32,
    coverage_end_exclusive: u32,
    premium_krw: i64,
    term_limit_krw: i64,
    paid_term_krw: i64,
    reserved_term_krw: i64,
    next_premium_due_game_day: Option<u32>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ClaimPublicRow {
    id: u64,
    event_id: u64,
    event_key: String,
    event_display_name: String,
    status: String,
    offered_game_day: u32,
    gross_cost_krw: Option<i64>,
    payout_krw: Option<i64>,
    resolved_game_day: Option<u32>,
    filing_deadline_game_day: Option<u32>,
    paid_game_day: Option<u32>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct AllocationPublicRow {
    claim_id: u64,
    contract_id: u64,
    deductible_krw: i64,
    allocated_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ContractLockRow {
    id: u64,
    status: String,
    coverage_start_game_day: u32,
    waiting_ends_game_day: u32,
    term_end_exclusive: u32,
    coverage_end_exclusive: u32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ChargeLockRow {
    id: u64,
    charge_no: u16,
    due_game_day: u32,
    amount_krw: i64,
    status: String,
    scheduled_settlement_id: Option<u64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ClaimLockRow {
    id: u64,
    life_event_instance_id: u64,
    status: String,
    payout_krw: Option<i64>,
    filing_deadline_game_day: Option<u32>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ClaimFinalizationRow {
    contract_id: u64,
    allocated_krw: i64,
    paid_term_krw: i64,
    reserved_term_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct StoredCommandReceiptRow {
    command_kind: String,
    payload_sha256: String,
    result_json: String,
    ledger_transaction_id: Option<u64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct SettlementEnvelopeRow {
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
struct OfferedEventRow {
    id: u64,
    life_catalog_set_id: u64,
    life_event_component_version_id: u64,
    life_event_definition_id: u64,
    event_key: String,
    offered_game_day: u32,
    status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ResolvedEventRow {
    id: u64,
    event_key: String,
    resolved_game_day: u32,
    effect_kind: String,
    effect_amount_krw: Option<i64>,
    status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct MatchingContractRow {
    contract_id: u64,
    product_version_id: u64,
    coverage_id: u64,
    coverage_start_game_day: u32,
    waiting_ends_game_day: u32,
    coverage_end_exclusive: u32,
    deductible_krw: i64,
    occurrence_limit_krw: i64,
    term_limit_krw: i64,
    paid_term_krw: i64,
    reserved_term_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct StoredClaimPinRow {
    id: u64,
    contract_id: u64,
    product_version_id: u64,
    coverage_id: u64,
    coverage_start_game_day: u32,
    waiting_ends_game_day: u32,
    coverage_end_exclusive: u32,
    waiting_satisfied: bool,
    deductible_krw: i64,
    occurrence_limit_krw: i64,
    term_limit_krw: i64,
    paid_term_krw_at_offer: i64,
    reserved_term_krw_at_offer: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PremiumSettlementPayload {
    version: u8,
    insurance_contract_id: ResourceId,
    premium_charge_id: ResourceId,
    charge_no: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InsuranceCursor {
    save_id: u64,
    run_revision: u32,
    component_version_id: u64,
    contracts_exhausted: bool,
    claims_exhausted: bool,
    contract_start_game_day: u32,
    contract_id: u64,
    claim_resolved_game_day: u32,
    claim_id: u64,
}

pub(super) async fn read_insurance(
    pool: &MySqlPool,
    rules: &dyn InsuranceRules,
    user_id: u64,
    query: InsuranceQueryState,
) -> Result<InsuranceReadResult> {
    let mut tx = pool.begin().await?;
    let Some(scope) = read_scope_for_user(&mut tx, user_id, false).await? else {
        tx.commit().await?;
        return Ok(InsuranceReadResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    if !scope.has_character {
        tx.commit().await?;
        return Ok(InsuranceReadResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    }
    if !component_is_active(&scope)? {
        if query.cursor.is_some() {
            tx.commit().await?;
            return Ok(InsuranceReadResult::Rejected(
                LifeFailureCode::InvalidCommand,
            ));
        }
        tx.commit().await?;
        return Ok(InsuranceReadResult::Found(InsuranceState {
            capability: InsuranceCapabilityState::Unavailable,
            products: Vec::new(),
            contracts: Vec::new(),
            pending_claims: Vec::new(),
            history: Vec::new(),
            next_cursor: None,
        }));
    }

    let catalog = load_catalog(&mut tx, &scope, rules).await?;
    let cursor = match query.cursor.as_deref() {
        Some(raw) => match decode_cursor(raw) {
            Ok(cursor) if cursor_matches_scope(cursor, &scope) => Some(cursor),
            _ => {
                tx.commit().await?;
                return Ok(InsuranceReadResult::Rejected(
                    LifeFailureCode::InvalidCommand,
                ));
            }
        },
        None => None,
    };
    if let Some(cursor) = cursor
        && !cursor_anchors_exist(&mut tx, &scope, cursor).await?
    {
        tx.commit().await?;
        return Ok(InsuranceReadResult::Rejected(
            LifeFailureCode::InvalidCommand,
        ));
    }

    let facts = collect_fact_evidence(&mut tx, &scope, &catalog, scope.game_day).await?;
    let products = read_products(&mut tx, &scope, rules, &catalog, &facts).await?;
    let (contracts, contract_more) = read_contract_page(&mut tx, &scope, cursor).await?;
    let pending_claims = read_pending_claims(&mut tx, &scope).await?;
    let (history, claim_more) = read_claim_history_page(&mut tx, &scope, cursor).await?;
    let next_cursor = next_cursor(
        &scope,
        cursor,
        &contracts,
        contract_more,
        &history,
        claim_more,
    )?;
    tx.commit().await?;
    Ok(InsuranceReadResult::Found(InsuranceState {
        capability: InsuranceCapabilityState::ContractsAndClaims,
        products,
        contracts,
        pending_claims,
        history,
        next_cursor,
    }))
}

pub(super) async fn enroll_insurance_contract(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    rules: &dyn InsuranceRules,
    user_id: u64,
    command: &EnrollInsuranceContractCommand,
) -> Result<LifeStoreResult<InsuranceEnrollmentReceipt>> {
    for _ in 0..MAX_TRANSACTION_ATTEMPTS {
        match enroll_insurance_contract_once(pool, finance_rules, rules, user_id, command).await {
            Ok(result) => return Ok(result),
            Err(error) if super::housing::is_retryable_database_error(&error) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy))
}

async fn enroll_insurance_contract_once(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    rules: &dyn InsuranceRules,
    user_id: u64,
    command: &EnrollInsuranceContractCommand,
) -> Result<LifeStoreResult<InsuranceEnrollmentReceipt>> {
    let fingerprint = enroll_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(scope) = read_scope_for_user(&mut tx, user_id, true).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_ENROLL,
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
                row.command_kind == COMMAND_KIND_ENROLL
                    && row.payload_sha256 == fingerprint
                    && row.ledger_transaction_id.is_some(),
                "stored enrollment receipt disagrees with its command"
            );
            let mut receipt: InsuranceEnrollmentReceipt = serde_json::from_str(&row.result_json)
                .context("stored enrollment receipt is invalid")?;
            ensure!(
                !receipt.replayed
                    && receipt.command_id == command.command_id
                    && receipt.product_version_id == command.product_version_id,
                "stored enrollment result disagrees with its command"
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
    if !component_is_active(&scope)? {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::RateUnavailable));
    }
    if !has_current_cursor(&scope, command.cursor) {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
    }

    let catalog = load_catalog(&mut tx, &scope, rules).await?;
    let Some(product) = catalog
        .products
        .iter()
        .find(|product| product.product_version_id == command.product_version_id)
    else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::InsuranceResourceNotFound,
        ));
    };
    let active_rows: Vec<(u64, u64)> = sqlx::query_as(
        "SELECT id, product_version_id FROM insurance_contract
         WHERE save_id = ? AND run_revision = ? AND status = 'active'
         ORDER BY id LIMIT 9 FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_all(&mut *tx)
    .await?;
    ensure!(
        active_rows.len() <= INSURANCE_MAX_ACTIVE_CONTRACTS,
        "active insurance contract count exceeded its bound"
    );
    if active_rows
        .iter()
        .any(|row| row.1 == command.product_version_id.get())
    {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
    }
    if active_rows.len() == INSURANCE_MAX_ACTIVE_CONTRACTS {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
    }
    let facts = collect_fact_evidence(&mut tx, &scope, &catalog, scope.game_day).await?;
    let evaluation = rules
        .evaluate_eligibility(InsuranceEligibilityInput {
            catalog: &catalog,
            product_version_id: product.product_version_id,
            evaluation_game_day: scope.game_day,
            facts: &facts,
        })
        .context("insurance enrollment eligibility evaluation failed")?;
    if evaluation.status != InsuranceEligibilityStatus::Eligible {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::Ineligible));
    }
    let premium_ledger = match rules.plan_premium_ledger(InsurancePremiumLedgerInput {
        wallet_cash_krw: scope.cash_krw,
        premium_krw: product.premium_krw,
    }) {
        Ok(plan) => plan,
        Err(InsuranceError::InsufficientWalletCash) => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(
                LifeFailureCode::InsufficientWalletCash,
            ));
        }
        Err(error) => return Err(error).context("insurance first premium plan failed"),
    };
    let provisional = rules
        .plan_contract(InsuranceContractPlanInput {
            contract_id: ResourceId::from_u64(1),
            product,
            start_game_day: scope.game_day,
        })
        .context("insurance contract planning failed")?;
    ensure!(
        provisional.status == InsuranceContractStatus::Active
            && provisional.product_version_id == product.product_version_id
            && provisional.premium_charges.len() == 12,
        "insurance contract plan escaped schema v1"
    );

    write_command_identity(&mut tx, scope.save_id, &identity).await?;
    let contract_insert = sqlx::query(
        "INSERT INTO insurance_contract
             (save_id, run_revision, life_catalog_set_id,
              insurance_component_version_id, product_version_id,
              enrollment_command_id, status, start_game_day,
              coverage_start_game_day, waiting_ends_game_day,
              term_end_exclusive, coverage_end_exclusive,
              paid_term_krw, reserved_term_krw)
         VALUES (?, ?, ?, ?, ?, ?, 'pending', ?, ?, ?, ?, ?, 0, 0)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.life_catalog_set_id)
    .bind(scope.insurance_component_version_id)
    .bind(product.product_version_id.get())
    .bind(command.command_id.as_str())
    .bind(scope.game_day)
    .bind(provisional.coverage_start_game_day)
    .bind(provisional.waiting_ends_game_day)
    .bind(provisional.coverage_end_exclusive)
    .bind(provisional.coverage_end_exclusive)
    .execute(&mut *tx)
    .await?;
    let contract_id = contract_insert.last_insert_id();
    let plan = rules
        .plan_contract(InsuranceContractPlanInput {
            contract_id: ResourceId::from_u64(contract_id),
            product,
            start_game_day: scope.game_day,
        })
        .context("insurance contract planning failed after identity allocation")?;
    ensure!(
        plan.coverage_start_game_day == provisional.coverage_start_game_day
            && plan.waiting_ends_game_day == provisional.waiting_ends_game_day
            && plan.coverage_end_exclusive == provisional.coverage_end_exclusive
            && plan.premium_charges == provisional.premium_charges,
        "insurance contract plan depends on allocated identity"
    );
    insert_contract_transition(
        &mut tx,
        &scope,
        contract_id,
        1,
        None,
        "pending",
        Some(command.command_id.as_str()),
        scope.game_day,
        "playerEnrollment",
    )
    .await?;
    let eligibility_json = serde_json::to_string(&json!({
        "componentVersionId": scope.insurance_component_version_id.to_string(),
        "evaluation": evaluation,
        "evaluationGameDay": scope.game_day,
        "facts": facts,
        "productVersionId": product.product_version_id.to_string(),
        "schemaVersion": 1,
    }))
    .context("insurance eligibility pin serialization failed")?;
    let pin_insert = sqlx::query(
        "INSERT INTO insurance_contract_eligibility_pin
             (save_id, run_revision, contract_id, evaluation_game_day,
              fact_count, eligibility_result, canonical_input_json)
         VALUES (?, ?, ?, ?, 4, 'eligible', ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(contract_id)
    .bind(scope.game_day)
    .bind(eligibility_json)
    .execute(&mut *tx)
    .await?;
    ensure!(pin_insert.rows_affected() == 1);

    let mut first_charge_id = None;
    let mut next_premium_due_game_day = None;
    for charge in &plan.premium_charges {
        let charge_insert = sqlx::query(
            "INSERT INTO insurance_premium_charge
                 (save_id, run_revision, contract_id, charge_no,
                  due_game_day, amount_krw, status)
             VALUES (?, ?, ?, ?, ?, ?, 'scheduled')",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(contract_id)
        .bind(charge.charge_no)
        .bind(charge.due_game_day)
        .bind(charge.amount_krw)
        .execute(&mut *tx)
        .await?;
        let charge_id = charge_insert.last_insert_id();
        if charge.charge_no == 1 {
            ensure!(charge.due_game_day == scope.game_day);
            first_charge_id = Some(charge_id);
            continue;
        }
        if charge.charge_no == 2 {
            next_premium_due_game_day = Some(charge.due_game_day);
        }
        let payload = premium_settlement_payload(contract_id, charge_id, charge.charge_no)?;
        let settlement_insert = sqlx::query(
            "INSERT INTO scheduled_settlement
                 (save_id, run_revision, due_game_day, kind, payload,
                  source_kind, source_id, occurrence, status)
             VALUES (?, ?, ?, 'insurancePremium', ?, 'insuranceContract', ?, ?, 'pending')",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(charge.due_game_day)
        .bind(payload)
        .bind(contract_id.to_string())
        .bind(u32::from(charge.charge_no))
        .execute(&mut *tx)
        .await?;
        let settlement_id = settlement_insert.last_insert_id();
        let attach = sqlx::query(
            "UPDATE insurance_premium_charge SET scheduled_settlement_id = ?
             WHERE id = ? AND save_id = ? AND run_revision = ?
               AND contract_id = ? AND status = 'scheduled'
               AND scheduled_settlement_id IS NULL",
        )
        .bind(settlement_id)
        .bind(charge_id)
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(contract_id)
        .execute(&mut *tx)
        .await?;
        ensure!(attach.rows_affected() == 1);
    }
    let first_charge_id = first_charge_id.context("insurance plan has no first premium")?;
    let ledger_transaction_id = write_insurance_ledger(
        &mut tx,
        finance_rules,
        &scope,
        &premium_ledger,
        LedgerSourceKind::InsurancePremiumPayment,
        first_charge_id,
        scope.game_day,
        "보험료 납부",
    )
    .await?;
    let paid = sqlx::query(
        "UPDATE insurance_premium_charge
         SET status = 'paid', ledger_transaction_id = ?, paid_game_day = due_game_day
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND contract_id = ? AND charge_no = 1 AND status = 'scheduled'",
    )
    .bind(ledger_transaction_id)
    .bind(first_charge_id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(contract_id)
    .execute(&mut *tx)
    .await?;
    ensure!(paid.rows_affected() == 1);
    insert_contract_transition(
        &mut tx,
        &scope,
        contract_id,
        2,
        Some("pending"),
        "active",
        Some(command.command_id.as_str()),
        scope.game_day,
        "firstPremiumPaid",
    )
    .await?;
    let activated = sqlx::query(
        "UPDATE insurance_contract SET status = 'active'
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'pending'",
    )
    .bind(contract_id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .execute(&mut *tx)
    .await?;
    ensure!(activated.rows_affected() == 1);
    let committed_cursor =
        update_save_after_command(&mut tx, &scope, premium_ledger.wallet_cash_after_krw).await?;
    let next_premium_due_game_day =
        next_premium_due_game_day.context("active insurance enrollment has no D+30 premium")?;
    ensure!(
        next_premium_due_game_day
            == scope
                .game_day
                .checked_add(u32::from(product.premium_cadence_game_days))
                .context("insurance next premium day overflowed")?
    );
    let receipt = InsuranceEnrollmentReceipt {
        command_id: command.command_id.clone(),
        contract_id: ResourceId::from_u64(contract_id),
        product_version_id: product.product_version_id,
        status: InsuranceContractStatusState::Active,
        coverage_start_game_day: plan.coverage_start_game_day,
        waiting_ends_game_day: plan.waiting_ends_game_day,
        coverage_end_exclusive: plan.coverage_end_exclusive,
        next_premium_due_game_day,
        premium_krw: product.premium_krw,
        replayed: false,
    };
    write_command_receipts(
        &mut tx,
        &scope,
        &identity,
        committed_cursor,
        &receipt,
        Some(contract_id),
        None,
        Some(ledger_transaction_id),
    )
    .await?;
    let save = read_state(&mut tx, scope.save_id).await?;
    tx.commit().await?;
    Ok(LifeStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

pub(super) async fn cancel_insurance_contract(
    pool: &MySqlPool,
    rules: &dyn InsuranceRules,
    user_id: u64,
    command: &CancelInsuranceContractCommand,
) -> Result<LifeStoreResult<InsuranceCancellationReceipt>> {
    for _ in 0..MAX_TRANSACTION_ATTEMPTS {
        match cancel_insurance_contract_once(pool, rules, user_id, command).await {
            Ok(result) => return Ok(result),
            Err(error) if super::housing::is_retryable_database_error(&error) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy))
}

async fn cancel_insurance_contract_once(
    pool: &MySqlPool,
    rules: &dyn InsuranceRules,
    user_id: u64,
    command: &CancelInsuranceContractCommand,
) -> Result<LifeStoreResult<InsuranceCancellationReceipt>> {
    let fingerprint = cancel_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(scope) = read_scope_for_user(&mut tx, user_id, true).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_CANCEL,
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
                row.command_kind == COMMAND_KIND_CANCEL
                    && row.payload_sha256 == fingerprint
                    && row.ledger_transaction_id.is_none(),
                "stored insurance cancellation receipt disagrees with its command"
            );
            let mut receipt: InsuranceCancellationReceipt = serde_json::from_str(&row.result_json)
                .context("stored insurance cancellation receipt is invalid")?;
            ensure!(
                !receipt.replayed
                    && receipt.command_id == command.command_id
                    && receipt.contract_id == command.contract_id,
                "stored insurance cancellation result disagrees with its command"
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
    if !component_is_active(&scope)? {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::RateUnavailable));
    }
    if !has_current_cursor(&scope, command.cursor) {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
    }
    load_catalog(&mut tx, &scope, rules).await?;
    let contract: Option<ContractLockRow> = sqlx::query_as(
        "SELECT id, status, coverage_start_game_day,
                waiting_ends_game_day, term_end_exclusive, coverage_end_exclusive
         FROM insurance_contract
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND insurance_component_version_id = ? FOR UPDATE",
    )
    .bind(command.contract_id.get())
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.insurance_component_version_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(contract) = contract else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::InsuranceResourceNotFound,
        ));
    };
    if contract.status != "active" {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
    }
    ensure!(
        contract.waiting_ends_game_day >= contract.coverage_start_game_day
            && contract.term_end_exclusive >= contract.coverage_end_exclusive,
        "stored insurance contract period is invalid"
    );
    let plan = match rules.terminate_contract(InsuranceTerminationInput {
        contract_id: command.contract_id,
        coverage_start_game_day: contract.coverage_start_game_day,
        current_coverage_end_exclusive: contract.coverage_end_exclusive,
        effective_game_day: scope.game_day,
        kind: InsuranceTerminationKind::Cancellation,
    }) {
        Ok(plan) => plan,
        Err(InsuranceError::InvalidContractState) => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
        }
        Err(error) => return Err(error).context("insurance cancellation planning failed"),
    };
    ensure!(
        plan.status == InsuranceContractStatus::Cancelled
            && plan.cancel_future_charges
            && plan.contract_id == command.contract_id,
        "insurance cancellation plan escaped its command"
    );
    let charges: Vec<ChargeLockRow> = sqlx::query_as(
        "SELECT id, charge_no, due_game_day, amount_krw, status, scheduled_settlement_id
         FROM insurance_premium_charge
         WHERE save_id = ? AND run_revision = ? AND contract_id = ?
           AND status = 'scheduled' ORDER BY charge_no FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(contract.id)
    .fetch_all(&mut *tx)
    .await?;
    ensure!(
        charges.len() <= 11,
        "insurance contract has too many future premiums"
    );
    write_command_identity(&mut tx, scope.save_id, &identity).await?;
    insert_contract_transition(
        &mut tx,
        &scope,
        contract.id,
        3,
        Some("active"),
        "cancelled",
        Some(command.command_id.as_str()),
        scope.game_day,
        "playerCancellation",
    )
    .await?;
    cancel_charges_and_settlements(
        &mut tx,
        &scope,
        contract.id,
        &charges,
        scope.game_day,
        "playerCancellation",
    )
    .await?;
    let update = sqlx::query(
        "UPDATE insurance_contract
         SET status = 'cancelled', coverage_end_exclusive = ?,
             terminal_game_day = ?, terminal_reason = 'playerCancellation'
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'active'",
    )
    .bind(plan.coverage_end_exclusive)
    .bind(plan.effective_game_day)
    .bind(contract.id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .execute(&mut *tx)
    .await?;
    ensure!(update.rows_affected() == 1);
    let committed_cursor = update_save_after_command(&mut tx, &scope, scope.cash_krw).await?;
    let receipt = InsuranceCancellationReceipt {
        command_id: command.command_id.clone(),
        contract_id: command.contract_id,
        status: InsuranceContractStatusState::Cancelled,
        coverage_end_exclusive: plan.coverage_end_exclusive,
        replayed: false,
    };
    write_command_receipts(
        &mut tx,
        &scope,
        &identity,
        committed_cursor,
        &receipt,
        Some(contract.id),
        None,
        None,
    )
    .await?;
    let save = read_state(&mut tx, scope.save_id).await?;
    tx.commit().await?;
    Ok(LifeStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

pub(super) async fn file_insurance_claim(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    rules: &dyn InsuranceRules,
    user_id: u64,
    command: &FileInsuranceClaimCommand,
) -> Result<LifeStoreResult<InsuranceClaimReceipt>> {
    for _ in 0..MAX_TRANSACTION_ATTEMPTS {
        match file_insurance_claim_once(pool, finance_rules, rules, user_id, command).await {
            Ok(result) => return Ok(result),
            Err(error) if super::housing::is_retryable_database_error(&error) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy))
}

async fn file_insurance_claim_once(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    rules: &dyn InsuranceRules,
    user_id: u64,
    command: &FileInsuranceClaimCommand,
) -> Result<LifeStoreResult<InsuranceClaimReceipt>> {
    let fingerprint = claim_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(scope) = read_scope_for_user(&mut tx, user_id, true).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_FILE_CLAIM,
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
                row.command_kind == COMMAND_KIND_FILE_CLAIM
                    && row.payload_sha256 == fingerprint
                    && row.ledger_transaction_id.is_some(),
                "stored insurance claim receipt disagrees with its command"
            );
            let mut receipt: InsuranceClaimReceipt = serde_json::from_str(&row.result_json)
                .context("stored insurance claim receipt is invalid")?;
            ensure!(
                !receipt.replayed
                    && receipt.command_id == command.command_id
                    && receipt.claim_id == command.claim_id,
                "stored insurance claim result disagrees with its command"
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
    if !component_is_active(&scope)? {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::RateUnavailable));
    }
    if !has_current_cursor(&scope, command.cursor) {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
    }
    load_catalog(&mut tx, &scope, rules).await?;
    let preliminary: Option<(String,)> = sqlx::query_as(
        "SELECT status FROM insurance_claim
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND insurance_component_version_id = ?",
    )
    .bind(command.claim_id.get())
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.insurance_component_version_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((preliminary_status,)) = preliminary else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::InsuranceResourceNotFound,
        ));
    };
    if preliminary_status != "ready" {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ClaimNotCovered));
    }
    let contract_ids: Vec<(u64,)> = sqlx::query_as(
        "SELECT contract_id FROM insurance_claim_allocation
         WHERE save_id = ? AND run_revision = ? AND claim_id = ?
         ORDER BY contract_id LIMIT 9",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(command.claim_id.get())
    .fetch_all(&mut *tx)
    .await?;
    ensure!(
        !contract_ids.is_empty() && contract_ids.len() <= INSURANCE_MAX_CLAIM_CONTRACTS,
        "ready insurance claim allocation cardinality is invalid"
    );
    for (contract_id,) in &contract_ids {
        let locked: Option<(u64,)> = sqlx::query_as(
            "SELECT id FROM insurance_contract
             WHERE id = ? AND save_id = ? AND run_revision = ? FOR UPDATE",
        )
        .bind(contract_id)
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .fetch_optional(&mut *tx)
        .await?;
        ensure!(locked.is_some(), "insurance claim contract disappeared");
    }
    let claim: Option<ClaimLockRow> = sqlx::query_as(
        "SELECT id, life_event_instance_id, status, payout_krw,
                filing_deadline_game_day
         FROM insurance_claim
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND insurance_component_version_id = ? FOR UPDATE",
    )
    .bind(command.claim_id.get())
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.insurance_component_version_id)
    .fetch_optional(&mut *tx)
    .await?;
    let claim = claim.context("insurance claim disappeared under the save lock")?;
    if claim.status != "ready" {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ClaimNotCovered));
    }
    let pin_rows: Vec<(u64,)> = sqlx::query_as(
        "SELECT id FROM insurance_claim_contract_pin
         WHERE save_id = ? AND run_revision = ? AND claim_id = ?
         ORDER BY contract_id FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(claim.id)
    .fetch_all(&mut *tx)
    .await?;
    ensure!(pin_rows.len() <= INSURANCE_MAX_CLAIM_CONTRACTS);
    let finalization_rows: Vec<ClaimFinalizationRow> = sqlx::query_as(
        "SELECT allocation.contract_id, allocation.allocated_krw,
                contract.paid_term_krw, contract.reserved_term_krw
         FROM insurance_claim_allocation AS allocation
         INNER JOIN insurance_contract AS contract
           ON contract.id = allocation.contract_id
          AND contract.save_id = allocation.save_id
          AND contract.run_revision = allocation.run_revision
         WHERE allocation.save_id = ? AND allocation.run_revision = ?
           AND allocation.claim_id = ?
         ORDER BY allocation.contract_id FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(claim.id)
    .fetch_all(&mut *tx)
    .await?;
    ensure!(
        finalization_rows.len() == contract_ids.len(),
        "insurance claim allocation set changed under lock"
    );
    let finalization = finalization_rows
        .iter()
        .map(|row| InsuranceClaimFinalizationContractInput {
            contract_id: ResourceId::from_u64(row.contract_id),
            allocation_krw: row.allocated_krw,
            paid_krw: row.paid_term_krw,
            reserved_krw: row.reserved_term_krw,
        })
        .collect::<Vec<_>>();
    let deadline = claim
        .filing_deadline_game_day
        .context("ready insurance claim has no filing deadline")?;
    let payment = match rules.pay_claim(InsuranceClaimPaymentInput {
        claim_id: command.claim_id,
        current_status: InsuranceClaimStatus::Ready,
        current_game_day: scope.game_day,
        filing_deadline_game_day: deadline,
        contracts: &finalization,
    }) {
        Ok(plan) => plan,
        Err(InsuranceError::ClaimExpired | InsuranceError::InvalidClaimTransition) => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(LifeFailureCode::ClaimNotCovered));
        }
        Err(error) => return Err(error).context("insurance claim payment planning failed"),
    };
    ensure!(
        payment.status == InsuranceClaimStatus::Paid
            && payment.claim_id == command.claim_id
            && Some(payment.payout_krw) == claim.payout_krw,
        "insurance claim payment plan escaped its claim"
    );
    let ledger_plan = rules
        .plan_claim_ledger(InsuranceClaimLedgerInput {
            wallet_cash_krw: scope.cash_krw,
            payout_krw: payment.payout_krw,
        })
        .context("insurance claim ledger planning failed")?;
    write_command_identity(&mut tx, scope.save_id, &identity).await?;
    insert_claim_transition(
        &mut tx,
        &scope,
        claim.id,
        3,
        Some("ready"),
        "paid",
        Some(command.command_id.as_str()),
        payment.paid_game_day,
        "playerClaim",
    )
    .await?;
    let ledger_transaction_id = write_insurance_ledger(
        &mut tx,
        finance_rules,
        &scope,
        &ledger_plan,
        LedgerSourceKind::InsuranceClaimPayment,
        claim.id,
        scope.game_day,
        "보험금 지급",
    )
    .await?;
    apply_contract_aggregates(&mut tx, &scope, &payment.contract_aggregates).await?;
    let claim_update = sqlx::query(
        "UPDATE insurance_claim
         SET status = 'paid', paid_game_day = ?, ledger_transaction_id = ?
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'ready'",
    )
    .bind(payment.paid_game_day)
    .bind(ledger_transaction_id)
    .bind(claim.id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .execute(&mut *tx)
    .await?;
    ensure!(claim_update.rows_affected() == 1);
    let committed_cursor =
        update_save_after_command(&mut tx, &scope, ledger_plan.wallet_cash_after_krw).await?;
    let receipt = InsuranceClaimReceipt {
        command_id: command.command_id.clone(),
        claim_id: command.claim_id,
        event_id: ResourceId::from_u64(claim.life_event_instance_id),
        payout_krw: payment.payout_krw,
        paid_game_day: payment.paid_game_day,
        replayed: false,
    };
    write_command_receipts(
        &mut tx,
        &scope,
        &identity,
        committed_cursor,
        &receipt,
        None,
        Some(claim.id),
        Some(ledger_transaction_id),
    )
    .await?;
    let save = read_state(&mut tx, scope.save_id).await?;
    tx.commit().await?;
    Ok(LifeStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

pub(super) async fn read_insurance_snapshot_in_tx(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn InsuranceRules,
    save_id: u64,
) -> Result<InsuranceSnapshotState> {
    let Some(scope) = read_scope_for_save(tx, save_id, false).await? else {
        return Ok(InsuranceSnapshotState::unavailable());
    };
    if !component_is_active(&scope)? {
        return Ok(InsuranceSnapshotState::unavailable());
    }
    let catalog = load_catalog(tx, &scope, rules).await?;
    let active_contracts = read_active_contracts(tx, &scope).await?;
    ensure!(
        active_contracts.len() <= INSURANCE_MAX_ACTIVE_CONTRACTS,
        "active insurance contracts exceeded the snapshot bound"
    );
    let pending_claims = read_pending_claims(tx, &scope).await?;
    ensure!(
        pending_claims.len() <= INSURANCE_MAX_CLAIM_CONTRACTS,
        "pending insurance claims exceeded the snapshot bound"
    );
    rules
        .validate_catalog(&catalog)
        .context("stored insurance catalog is invalid")?;
    Ok(InsuranceSnapshotState {
        capability: InsuranceCapabilityState::ContractsAndClaims,
        active_contracts,
        pending_claims,
    })
}

pub(super) fn validate_insurance_settlement_envelope(
    settlement: &ScheduledSettlement,
) -> Result<()> {
    ensure!(
        settlement.kind == SettlementKind::InsurancePremium
            && settlement.source.kind == SettlementSourceKind::InsuranceContract
            && settlement.status == SettlementStatus::Pending,
        "insurance settlement envelope has an invalid kind or state"
    );
    let payload: PremiumSettlementPayload = serde_json::from_value(settlement.payload.clone())
        .context("insurance settlement payload is invalid")?;
    ensure!(
        payload.version == 1
            && (2..=12).contains(&payload.charge_no)
            && settlement.source.source_id == payload.insurance_contract_id.to_string()
            && settlement.source.occurrence == u64::from(payload.charge_no),
        "insurance settlement envelope disagrees with its payload"
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn settle_insurance_premium_by_id_in_tx(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    rules: &dyn InsuranceRules,
    save_id: u64,
    run_revision: u32,
    policy_set_id: u64,
    game_day: u32,
    settlement_id: u64,
) -> Result<()> {
    let scope = read_scope_for_save(tx, save_id, false)
        .await?
        .context("insurance premium settlement lost its save")?;
    ensure!(
        component_is_active(&scope)?
            && scope.run_revision == run_revision
            && scope.policy_set_id == policy_set_id
            && scope.game_day.checked_add(1) == Some(game_day),
        "insurance premium settlement escaped its run scope"
    );
    let envelope: SettlementEnvelopeRow = sqlx::query_as(
        "SELECT id, due_game_day, kind, CAST(payload AS CHAR) AS payload_json,
                source_kind, source_id, occurrence, status
         FROM scheduled_settlement
         WHERE id = ? AND save_id = ? AND run_revision = ? FOR UPDATE",
    )
    .bind(settlement_id)
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await?;
    let payload: PremiumSettlementPayload = serde_json::from_str(&envelope.payload_json)
        .context("stored insurance premium payload is invalid")?;
    ensure!(
        envelope.id == settlement_id
            && envelope.kind == "insurancePremium"
            && envelope.source_kind == "insuranceContract"
            && envelope.source_id == payload.insurance_contract_id.to_string()
            && envelope.occurrence == u64::from(payload.charge_no)
            && envelope.status == "pending"
            && envelope.due_game_day == game_day
            && payload.version == 1
            && (2..=12).contains(&payload.charge_no),
        "stored insurance premium envelope is invalid"
    );
    let contract: ContractLockRow = sqlx::query_as(
        "SELECT id, status, coverage_start_game_day,
                waiting_ends_game_day, term_end_exclusive, coverage_end_exclusive
         FROM insurance_contract
         WHERE id = ? AND save_id = ? AND run_revision = ? FOR UPDATE",
    )
    .bind(payload.insurance_contract_id.get())
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        contract.status == "active",
        "insurance premium contract is not active"
    );
    let charges: Vec<ChargeLockRow> = sqlx::query_as(
        "SELECT id, charge_no, due_game_day, amount_krw, status, scheduled_settlement_id
         FROM insurance_premium_charge
         WHERE save_id = ? AND run_revision = ? AND contract_id = ?
           AND status = 'scheduled' ORDER BY charge_no FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract.id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        charges.len() <= 11,
        "insurance premium schedule exceeded its bound"
    );
    let charge = charges
        .iter()
        .find(|charge| charge.id == payload.premium_charge_id.get())
        .context("insurance premium charge is missing from its contract")?;
    ensure!(
        charge.charge_no == payload.charge_no
            && charge.due_game_day == game_day
            && charge.status == "scheduled"
            && charge.scheduled_settlement_id == Some(settlement_id),
        "insurance premium charge disagrees with its settlement"
    );
    let resolution = rules
        .resolve_premium(crate::life::InsurancePremiumResolutionInput {
            contract_id: payload.insurance_contract_id,
            charge_no: payload.charge_no,
            due_game_day: charge.due_game_day,
            premium_krw: charge.amount_krw,
            wallet_cash_krw: scope.cash_krw,
        })
        .context("insurance premium resolution failed")?;
    match resolution.charge_status {
        crate::life::InsurancePremiumChargeStatus::Paid => {
            ensure!(
                resolution.contract_status == InsuranceContractStatus::Active
                    && !resolution.cancel_future_charges
                    && resolution.coverage_end_exclusive.is_none(),
                "paid insurance premium resolution changed its contract"
            );
            let ledger_plan = rules
                .plan_premium_ledger(InsurancePremiumLedgerInput {
                    wallet_cash_krw: scope.cash_krw,
                    premium_krw: charge.amount_krw,
                })
                .context("insurance premium ledger planning failed")?;
            let ledger_id = write_insurance_ledger(
                tx,
                finance_rules,
                &scope,
                &ledger_plan,
                LedgerSourceKind::InsurancePremiumPayment,
                charge.id,
                game_day,
                "보험료 납부",
            )
            .await?;
            let cash_update = sqlx::query(
                "UPDATE save SET cash_krw = ?
                 WHERE id = ? AND run_revision = ? AND policy_set_id = ? AND cash_krw = ?",
            )
            .bind(ledger_plan.wallet_cash_after_krw)
            .bind(save_id)
            .bind(run_revision)
            .bind(policy_set_id)
            .bind(scope.cash_krw)
            .execute(&mut **tx)
            .await?;
            ensure!(
                cash_update.rows_affected() == 1,
                "insurance premium lost its wallet"
            );
            let charge_update = sqlx::query(
                "UPDATE insurance_premium_charge
                 SET status = 'paid', ledger_transaction_id = ?, paid_game_day = due_game_day
                 WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'scheduled'",
            )
            .bind(ledger_id)
            .bind(charge.id)
            .bind(save_id)
            .bind(run_revision)
            .execute(&mut **tx)
            .await?;
            ensure!(charge_update.rows_affected() == 1);
            let settlement_update = sqlx::query(
                "UPDATE scheduled_settlement
                 SET status = 'settled', outcome = 'applied',
                     settled_ledger_transaction_id = ?
                 WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'pending'",
            )
            .bind(ledger_id)
            .bind(settlement_id)
            .bind(save_id)
            .bind(run_revision)
            .execute(&mut **tx)
            .await?;
            ensure!(settlement_update.rows_affected() == 1);
        }
        crate::life::InsurancePremiumChargeStatus::Missed => {
            ensure!(
                resolution.contract_status == InsuranceContractStatus::Lapsed
                    && resolution.cancel_future_charges,
                "missed insurance premium did not lapse its contract"
            );
            let coverage_end_exclusive = resolution
                .coverage_end_exclusive
                .context("missed insurance premium has no coverage boundary")?;
            let charge_update = sqlx::query(
                "UPDATE insurance_premium_charge
                 SET status = 'missed', terminal_game_day = due_game_day,
                     terminal_reason = 'insufficientWalletCash'
                 WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'scheduled'",
            )
            .bind(charge.id)
            .bind(save_id)
            .bind(run_revision)
            .execute(&mut **tx)
            .await?;
            ensure!(charge_update.rows_affected() == 1);
            let settlement_update = sqlx::query(
                "UPDATE scheduled_settlement
                 SET status = 'cancelled', cancellation_reason = 'insurancePremiumMissed'
                 WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'pending'",
            )
            .bind(settlement_id)
            .bind(save_id)
            .bind(run_revision)
            .execute(&mut **tx)
            .await?;
            ensure!(settlement_update.rows_affected() == 1);
            insert_contract_transition(
                tx,
                &scope,
                contract.id,
                3,
                Some("active"),
                "lapsed",
                None,
                game_day,
                "premiumMissed",
            )
            .await?;
            let future = charges
                .iter()
                .filter(|candidate| candidate.id != charge.id)
                .cloned()
                .collect::<Vec<_>>();
            cancel_charges_and_settlements(
                tx,
                &scope,
                contract.id,
                &future,
                game_day,
                "contractLapsed",
            )
            .await?;
            let contract_update = sqlx::query(
                "UPDATE insurance_contract
                 SET status = 'lapsed', coverage_end_exclusive = ?,
                     terminal_game_day = ?, terminal_reason = 'premiumMissed'
                 WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'active'",
            )
            .bind(coverage_end_exclusive)
            .bind(game_day)
            .bind(contract.id)
            .bind(save_id)
            .bind(run_revision)
            .execute(&mut **tx)
            .await?;
            ensure!(contract_update.rows_affected() == 1);
        }
        _ => bail!("insurance premium resolution returned an unsupported charge state"),
    }
    Ok(())
}

pub(super) async fn expire_insurance_contracts_in_tx(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn InsuranceRules,
    save_id: u64,
    target_game_day: u32,
) -> Result<()> {
    let Some(scope) = read_scope_for_save(tx, save_id, false).await? else {
        return Ok(());
    };
    if !component_is_active(&scope)? {
        return Ok(());
    }
    let contracts: Vec<ContractLockRow> = sqlx::query_as(
        "SELECT id, status, coverage_start_game_day,
                waiting_ends_game_day, term_end_exclusive, coverage_end_exclusive
         FROM insurance_contract
         WHERE save_id = ? AND run_revision = ? AND status = 'active'
           AND term_end_exclusive <= ? ORDER BY id FOR UPDATE",
    )
    .bind(save_id)
    .bind(scope.run_revision)
    .bind(target_game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        contracts.len() <= INSURANCE_MAX_ACTIVE_CONTRACTS,
        "expiring insurance contract count exceeded its bound"
    );
    for contract in contracts {
        ensure!(contract.term_end_exclusive == contract.coverage_end_exclusive);
        let plan = rules
            .expire_contract(InsuranceContractExpiryInput {
                contract_id: ResourceId::from_u64(contract.id),
                current_status: InsuranceContractStatus::Active,
                coverage_end_exclusive: contract.coverage_end_exclusive,
                target_game_day,
            })
            .context("insurance contract expiry planning failed")?;
        ensure!(
            plan.status == InsuranceContractStatus::Expired
                && plan.expired_game_day == contract.term_end_exclusive,
            "insurance contract expiry plan escaped its term"
        );
        let charges: Vec<ChargeLockRow> = sqlx::query_as(
            "SELECT id, charge_no, due_game_day, amount_krw, status,
                    scheduled_settlement_id
             FROM insurance_premium_charge
             WHERE save_id = ? AND run_revision = ? AND contract_id = ?
               AND status = 'scheduled' ORDER BY charge_no FOR UPDATE",
        )
        .bind(save_id)
        .bind(scope.run_revision)
        .bind(contract.id)
        .fetch_all(&mut **tx)
        .await?;
        insert_contract_transition(
            tx,
            &scope,
            contract.id,
            3,
            Some("active"),
            "expired",
            None,
            plan.expired_game_day,
            "termEnded",
        )
        .await?;
        cancel_charges_and_settlements(
            tx,
            &scope,
            contract.id,
            &charges,
            plan.expired_game_day,
            "termEnded",
        )
        .await?;
        let update = sqlx::query(
            "UPDATE insurance_contract
             SET status = 'expired', terminal_game_day = ?, terminal_reason = 'termEnded'
             WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'active'",
        )
        .bind(plan.expired_game_day)
        .bind(contract.id)
        .bind(save_id)
        .bind(scope.run_revision)
        .execute(&mut **tx)
        .await?;
        ensure!(update.rows_affected() == 1);
    }
    Ok(())
}

pub(super) async fn expire_insurance_claims_in_tx(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn InsuranceRules,
    save_id: u64,
    target_game_day: u32,
) -> Result<()> {
    let Some(scope) = read_scope_for_save(tx, save_id, false).await? else {
        return Ok(());
    };
    if !component_is_active(&scope)? {
        return Ok(());
    }
    let claim_ids: Vec<(u64, u64, u32)> = sqlx::query_as(
        "SELECT id, life_event_instance_id, filing_deadline_game_day
         FROM insurance_claim
         WHERE save_id = ? AND run_revision = ? AND status = 'ready'
           AND filing_deadline_game_day <= ?
         ORDER BY life_event_instance_id, id LIMIT 9",
    )
    .bind(save_id)
    .bind(scope.run_revision)
    .bind(target_game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        claim_ids.len() <= INSURANCE_MAX_CLAIM_CONTRACTS,
        "expiring insurance claim count exceeded its bound"
    );
    if claim_ids.is_empty() {
        return Ok(());
    }
    let contract_ids: Vec<(u64,)> = sqlx::query_as(
        "SELECT DISTINCT allocation.contract_id
         FROM insurance_claim_allocation AS allocation
         INNER JOIN insurance_claim AS claim
           ON claim.id = allocation.claim_id AND claim.save_id = allocation.save_id
          AND claim.run_revision = allocation.run_revision
         WHERE allocation.save_id = ? AND allocation.run_revision = ?
           AND claim.status = 'ready' AND claim.filing_deadline_game_day <= ?
         ORDER BY allocation.contract_id LIMIT 9",
    )
    .bind(save_id)
    .bind(scope.run_revision)
    .bind(target_game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        !contract_ids.is_empty() && contract_ids.len() <= INSURANCE_MAX_CLAIM_CONTRACTS,
        "expiring insurance claim contract count is invalid"
    );
    for (contract_id,) in &contract_ids {
        let locked: Option<(u64,)> = sqlx::query_as(
            "SELECT id FROM insurance_contract
             WHERE id = ? AND save_id = ? AND run_revision = ? FOR UPDATE",
        )
        .bind(contract_id)
        .bind(save_id)
        .bind(scope.run_revision)
        .fetch_optional(&mut **tx)
        .await?;
        ensure!(locked.is_some());
    }
    for (claim_id, _, _) in &claim_ids {
        let locked: Option<(u64,)> = sqlx::query_as(
            "SELECT id FROM insurance_claim
             WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'ready'
             FOR UPDATE",
        )
        .bind(claim_id)
        .bind(save_id)
        .bind(scope.run_revision)
        .fetch_optional(&mut **tx)
        .await?;
        ensure!(locked.is_some());
    }
    for (claim_id, _, deadline) in claim_ids {
        ensure!(
            target_game_day == deadline,
            "ready insurance claim missed its exact expiry pipeline day"
        );
        let _: Vec<(u64,)> = sqlx::query_as(
            "SELECT id FROM insurance_claim_contract_pin
             WHERE save_id = ? AND run_revision = ? AND claim_id = ?
             ORDER BY contract_id FOR UPDATE",
        )
        .bind(save_id)
        .bind(scope.run_revision)
        .bind(claim_id)
        .fetch_all(&mut **tx)
        .await?;
        let rows: Vec<ClaimFinalizationRow> = sqlx::query_as(
            "SELECT allocation.contract_id, allocation.allocated_krw,
                    contract.paid_term_krw, contract.reserved_term_krw
             FROM insurance_claim_allocation AS allocation
             INNER JOIN insurance_contract AS contract
               ON contract.id = allocation.contract_id
              AND contract.save_id = allocation.save_id
              AND contract.run_revision = allocation.run_revision
             WHERE allocation.save_id = ? AND allocation.run_revision = ?
               AND allocation.claim_id = ?
             ORDER BY allocation.contract_id FOR UPDATE",
        )
        .bind(save_id)
        .bind(scope.run_revision)
        .bind(claim_id)
        .fetch_all(&mut **tx)
        .await?;
        let contracts = rows
            .iter()
            .map(|row| InsuranceClaimFinalizationContractInput {
                contract_id: ResourceId::from_u64(row.contract_id),
                allocation_krw: row.allocated_krw,
                paid_krw: row.paid_term_krw,
                reserved_krw: row.reserved_term_krw,
            })
            .collect::<Vec<_>>();
        let plan = rules
            .expire_claim(crate::life::InsuranceClaimExpiryInput {
                claim_id: ResourceId::from_u64(claim_id),
                current_status: InsuranceClaimStatus::Ready,
                current_game_day: target_game_day,
                filing_deadline_game_day: deadline,
                contracts: &contracts,
            })
            .context("insurance claim expiry planning failed")?;
        ensure!(
            plan.status == InsuranceClaimStatus::Expired && plan.expired_game_day == deadline,
            "insurance claim expiry plan escaped its deadline"
        );
        insert_claim_transition(
            tx,
            &scope,
            claim_id,
            3,
            Some("ready"),
            "expired",
            None,
            deadline,
            "filingDeadline",
        )
        .await?;
        apply_contract_aggregates(tx, &scope, &plan.contract_aggregates).await?;
        let update = sqlx::query(
            "UPDATE insurance_claim SET status = 'expired'
             WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'ready'",
        )
        .bind(claim_id)
        .bind(save_id)
        .bind(scope.run_revision)
        .execute(&mut **tx)
        .await?;
        ensure!(update.rows_affected() == 1);
    }
    Ok(())
}

pub(super) async fn pin_insurance_claim_for_event_in_tx(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn InsuranceRules,
    save_id: u64,
    event_instance_id: u64,
) -> Result<()> {
    let Some(scope) = read_scope_for_save(tx, save_id, false).await? else {
        return Ok(());
    };
    if !component_is_active(&scope)? {
        return Ok(());
    }
    let catalog = load_catalog(tx, &scope, rules).await?;
    let event: OfferedEventRow = sqlx::query_as(
        "SELECT instance.id, instance.life_catalog_set_id,
                instance.life_event_component_version_id,
                instance.life_event_definition_id, definition.event_key,
                instance.offered_game_day, instance.status
         FROM life_event_instance AS instance
         INNER JOIN life_event_definition AS definition
           ON definition.id = instance.life_event_definition_id
          AND definition.life_component_version_id = instance.life_event_component_version_id
         WHERE instance.id = ? AND instance.save_id = ? AND instance.run_revision = ?
           AND instance.life_event_component_version_id = ? FOR SHARE",
    )
    .bind(event_instance_id)
    .bind(save_id)
    .bind(scope.run_revision)
    .bind(scope.life_event_component_version_id)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        event.status == "offered"
            && event.life_catalog_set_id == scope.life_catalog_set_id
            && event.life_event_component_version_id == scope.life_event_component_version_id,
        "insurance claim pin escaped its offered event"
    );
    let existing: Option<(u64, String)> = sqlx::query_as(
        "SELECT id, status FROM insurance_claim
         WHERE save_id = ? AND run_revision = ? AND life_event_instance_id = ?",
    )
    .bind(save_id)
    .bind(scope.run_revision)
    .bind(event_instance_id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((_, status)) = existing {
        ensure!(
            status == "candidate",
            "offered event has a non-candidate insurance claim"
        );
        return Ok(());
    }
    let rows: Vec<MatchingContractRow> = sqlx::query_as(
        "SELECT contract.id AS contract_id, contract.product_version_id,
                coverage.id AS coverage_id, contract.coverage_start_game_day,
                contract.waiting_ends_game_day, contract.coverage_end_exclusive,
                coverage.deductible_krw, coverage.occurrence_limit_krw,
                coverage.term_limit_krw, contract.paid_term_krw,
                contract.reserved_term_krw
         FROM insurance_contract AS contract
         INNER JOIN insurance_product_coverage AS coverage
           ON coverage.life_component_version_id = contract.insurance_component_version_id
          AND coverage.product_version_id = contract.product_version_id
         WHERE contract.save_id = ? AND contract.run_revision = ?
           AND contract.insurance_component_version_id = ? AND contract.status = 'active'
           AND contract.coverage_start_game_day <= ?
           AND ? < contract.coverage_end_exclusive
           AND BINARY coverage.event_key = BINARY ?
           AND EXISTS(
               SELECT 1 FROM life_event_choice AS choice_row
               WHERE choice_row.life_event_definition_id = ?
                 AND choice_row.life_component_version_id = ?
                 AND BINARY choice_row.effect_kind = BINARY coverage.effect_kind
           )
         ORDER BY contract.id LIMIT 9",
    )
    .bind(save_id)
    .bind(scope.run_revision)
    .bind(scope.insurance_component_version_id)
    .bind(event.offered_game_day)
    .bind(event.offered_game_day)
    .bind(&event.event_key)
    .bind(event.life_event_definition_id)
    .bind(event.life_event_component_version_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() <= INSURANCE_MAX_CLAIM_CONTRACTS,
        "insurance event matched too many contracts"
    );
    let mut pins = Vec::with_capacity(rows.len());
    for row in rows {
        ensure!(
            catalog.products.iter().any(|product| {
                product.product_version_id.get() == row.product_version_id
                    && product.coverages.iter().any(|coverage| {
                        coverage.coverage_version_id.get() == row.coverage_id
                            && coverage.event_key == event.event_key
                    })
            }),
            "insurance event matched a coverage outside the loaded catalog"
        );
        let waiting_passed = rules
            .is_event_covered(crate::life::InsuranceCoverageInput {
                coverage_start_game_day: row.coverage_start_game_day,
                waiting_ends_game_day: row.waiting_ends_game_day,
                coverage_end_exclusive: row.coverage_end_exclusive,
                event_offered_game_day: event.offered_game_day,
            })
            .context("insurance event coverage evaluation failed")?;
        pins.push(InsuranceClaimContractPin {
            contract_id: ResourceId::from_u64(row.contract_id),
            product_version_id: ResourceId::from_u64(row.product_version_id),
            coverage_version_id: ResourceId::from_u64(row.coverage_id),
            coverage_start_game_day: row.coverage_start_game_day,
            waiting_ends_game_day: row.waiting_ends_game_day,
            coverage_end_exclusive: row.coverage_end_exclusive,
            waiting_passed,
            deductible_krw: row.deductible_krw,
            occurrence_limit_krw: row.occurrence_limit_krw,
            term_limit_krw: row.term_limit_krw,
            paid_krw: row.paid_term_krw,
            reserved_krw: row.reserved_term_krw,
        });
    }
    let provisional = rules
        .plan_claim_candidate(InsuranceClaimCandidateInput {
            claim_id: ResourceId::from_u64(1),
            event_instance_id: ResourceId::from_u64(event.id),
            offered_game_day: event.offered_game_day,
            matching_contracts: &pins,
        })
        .context("insurance claim candidate planning failed")?;
    ensure!(provisional.status == InsuranceClaimStatus::Candidate);
    let insert = sqlx::query(
        "INSERT INTO insurance_claim
             (save_id, run_revision, life_catalog_set_id,
              life_event_component_version_id, insurance_component_version_id,
              life_event_instance_id, status, offered_game_day,
              contract_pin_count, contract_pin_sha256)
         VALUES (?, ?, ?, ?, ?, ?, 'candidate', ?, ?, ?)",
    )
    .bind(save_id)
    .bind(scope.run_revision)
    .bind(scope.life_catalog_set_id)
    .bind(scope.life_event_component_version_id)
    .bind(scope.insurance_component_version_id)
    .bind(event.id)
    .bind(event.offered_game_day)
    .bind(u8::try_from(provisional.contract_pins.len()).context("claim pin count overflowed")?)
    .bind(&provisional.contract_set_digest)
    .execute(&mut **tx)
    .await?;
    let claim_id = insert.last_insert_id();
    let plan = rules
        .plan_claim_candidate(InsuranceClaimCandidateInput {
            claim_id: ResourceId::from_u64(claim_id),
            event_instance_id: ResourceId::from_u64(event.id),
            offered_game_day: event.offered_game_day,
            matching_contracts: &pins,
        })
        .context("insurance claim candidate planning failed after identity allocation")?;
    ensure!(
        plan.contract_set_digest == provisional.contract_set_digest
            && plan.contract_pins == provisional.contract_pins,
        "insurance claim candidate digest depends on allocated identity"
    );
    for (index, pin) in plan.contract_pins.iter().enumerate() {
        let pin_order = u8::try_from(index + 1).context("insurance pin order overflowed")?;
        let pin_insert = sqlx::query(
            "INSERT INTO insurance_claim_contract_pin
                 (save_id, run_revision, claim_id, contract_id, pin_order,
                  insurance_component_version_id, product_version_id, coverage_id,
                  coverage_start_game_day, waiting_ends_game_day,
                  coverage_end_exclusive, waiting_satisfied, deductible_krw,
                  occurrence_limit_krw, term_limit_krw,
                  paid_term_krw_at_offer, reserved_term_krw_at_offer)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(save_id)
        .bind(scope.run_revision)
        .bind(claim_id)
        .bind(pin.contract_id.get())
        .bind(pin_order)
        .bind(scope.insurance_component_version_id)
        .bind(pin.product_version_id.get())
        .bind(pin.coverage_version_id.get())
        .bind(pin.coverage_start_game_day)
        .bind(pin.waiting_ends_game_day)
        .bind(pin.coverage_end_exclusive)
        .bind(pin.waiting_passed)
        .bind(pin.deductible_krw)
        .bind(pin.occurrence_limit_krw)
        .bind(pin.term_limit_krw)
        .bind(pin.paid_krw)
        .bind(pin.reserved_krw)
        .execute(&mut **tx)
        .await?;
        ensure!(pin_insert.rows_affected() == 1);
    }
    insert_claim_transition(
        tx,
        &scope,
        claim_id,
        1,
        None,
        "candidate",
        None,
        event.offered_game_day,
        "eventOffered",
    )
    .await?;
    Ok(())
}

pub(super) async fn allocate_insurance_claim_for_event_in_tx(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn InsuranceRules,
    save_id: u64,
    event_instance_id: u64,
) -> Result<()> {
    let Some(scope) = read_scope_for_save(tx, save_id, false).await? else {
        return Ok(());
    };
    if !component_is_active(&scope)? {
        return Ok(());
    }
    let catalog = load_catalog(tx, &scope, rules).await?;
    let event: ResolvedEventRow = sqlx::query_as(
        "SELECT instance.id, definition.event_key, instance.resolved_game_day,
                choice_row.effect_kind, choice_row.effect_amount_krw, instance.status
         FROM life_event_instance AS instance
         INNER JOIN life_event_definition AS definition
           ON definition.id = instance.life_event_definition_id
          AND definition.life_component_version_id = instance.life_event_component_version_id
         INNER JOIN life_event_choice AS choice_row
           ON choice_row.id = instance.resolved_choice_id
          AND choice_row.life_event_definition_id = instance.life_event_definition_id
          AND choice_row.life_component_version_id = instance.life_event_component_version_id
         WHERE instance.id = ? AND instance.save_id = ? AND instance.run_revision = ?
           AND instance.life_event_component_version_id = ?",
    )
    .bind(event_instance_id)
    .bind(save_id)
    .bind(scope.run_revision)
    .bind(scope.life_event_component_version_id)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        event.status == "resolved",
        "insurance allocation event is not resolved"
    );
    let resolution_kind = match (event.effect_kind.as_str(), event.effect_amount_krw) {
        ("noEffect", None) => InsuranceClaimResolutionKind::NoEffect,
        ("fixedWalletExpense", Some(amount)) if amount > 0 => {
            InsuranceClaimResolutionKind::FixedWalletExpense
        }
        _ => bail!("resolved event has an unsupported insurance effect projection"),
    };
    let gross_cost_krw = match resolution_kind {
        InsuranceClaimResolutionKind::NoEffect => None,
        InsuranceClaimResolutionKind::FixedWalletExpense => event.effect_amount_krw,
    };
    let preliminary: (u64, String) = sqlx::query_as(
        "SELECT id, status FROM insurance_claim
         WHERE save_id = ? AND run_revision = ? AND life_event_instance_id = ?",
    )
    .bind(save_id)
    .bind(scope.run_revision)
    .bind(event.id)
    .fetch_one(&mut **tx)
    .await?;
    if preliminary.1 != "candidate" {
        ensure!(
            matches!(
                preliminary.1.as_str(),
                "notApplicable" | "notCovered" | "ready" | "paid" | "expired"
            ),
            "stored insurance claim status is invalid"
        );
        return Ok(());
    }
    let contract_ids: Vec<(u64,)> = sqlx::query_as(
        "SELECT contract_id FROM insurance_claim_contract_pin
         WHERE save_id = ? AND run_revision = ? AND claim_id = ?
         ORDER BY contract_id LIMIT 9",
    )
    .bind(save_id)
    .bind(scope.run_revision)
    .bind(preliminary.0)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(contract_ids.len() <= INSURANCE_MAX_CLAIM_CONTRACTS);
    for (contract_id,) in &contract_ids {
        let locked: Option<(u64,)> = sqlx::query_as(
            "SELECT id FROM insurance_contract
             WHERE id = ? AND save_id = ? AND run_revision = ? FOR UPDATE",
        )
        .bind(contract_id)
        .bind(save_id)
        .bind(scope.run_revision)
        .fetch_optional(&mut **tx)
        .await?;
        ensure!(locked.is_some());
    }
    let claim: ClaimLockRow = sqlx::query_as(
        "SELECT id, life_event_instance_id, status, payout_krw,
                filing_deadline_game_day
         FROM insurance_claim
         WHERE id = ? AND save_id = ? AND run_revision = ? FOR UPDATE",
    )
    .bind(preliminary.0)
    .bind(save_id)
    .bind(scope.run_revision)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(claim.status == "candidate" && claim.life_event_instance_id == event.id);
    let pin_rows: Vec<StoredClaimPinRow> = sqlx::query_as(
        "SELECT id, contract_id, product_version_id, coverage_id,
                coverage_start_game_day, waiting_ends_game_day,
                coverage_end_exclusive, waiting_satisfied, deductible_krw,
                occurrence_limit_krw, term_limit_krw,
                paid_term_krw_at_offer, reserved_term_krw_at_offer
         FROM insurance_claim_contract_pin
         WHERE save_id = ? AND run_revision = ? AND claim_id = ?
         ORDER BY contract_id FOR UPDATE",
    )
    .bind(save_id)
    .bind(scope.run_revision)
    .bind(claim.id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(pin_rows.len() == contract_ids.len());
    let pins = pin_rows
        .iter()
        .map(|row| InsuranceClaimContractPin {
            contract_id: ResourceId::from_u64(row.contract_id),
            product_version_id: ResourceId::from_u64(row.product_version_id),
            coverage_version_id: ResourceId::from_u64(row.coverage_id),
            coverage_start_game_day: row.coverage_start_game_day,
            waiting_ends_game_day: row.waiting_ends_game_day,
            coverage_end_exclusive: row.coverage_end_exclusive,
            waiting_passed: row.waiting_satisfied,
            deductible_krw: row.deductible_krw,
            occurrence_limit_krw: row.occurrence_limit_krw,
            term_limit_krw: row.term_limit_krw,
            paid_krw: row.paid_term_krw_at_offer,
            reserved_krw: row.reserved_term_krw_at_offer,
        })
        .collect::<Vec<_>>();
    let product = catalog
        .products
        .first()
        .context("insurance catalog has no claim window authority")?;
    ensure!(
        product
            .coverages
            .iter()
            .any(|coverage| coverage.event_key == event.event_key),
        "insurance claim event is not covered by the loaded catalog"
    );
    let plan = rules
        .resolve_claim(InsuranceClaimResolutionInput {
            claim_id: ResourceId::from_u64(claim.id),
            current_status: InsuranceClaimStatus::Candidate,
            resolved_game_day: event.resolved_game_day,
            resolution_kind,
            gross_cost_krw,
            claim_window_game_days: product.claim_window_game_days,
            contract_pins: &pins,
        })
        .context("insurance claim resolution planning failed")?;
    let to_status = claim_status_db(plan.status)?;
    ensure!(matches!(
        plan.status,
        InsuranceClaimStatus::NotApplicable
            | InsuranceClaimStatus::NotCovered
            | InsuranceClaimStatus::Ready
    ));
    for (index, allocation) in plan.allocations.iter().enumerate() {
        let pin = pin_rows
            .iter()
            .find(|pin| pin.contract_id == allocation.contract_id.get())
            .context("insurance allocation has no event-time pin")?;
        let aggregate = plan
            .contract_aggregates
            .iter()
            .find(|aggregate| aggregate.contract_id == allocation.contract_id)
            .context("insurance allocation has no contract aggregate")?;
        let insert = sqlx::query(
            "INSERT INTO insurance_claim_allocation
                 (save_id, run_revision, claim_id, claim_contract_pin_id,
                  contract_id, allocation_order, raw_indemnity_krw,
                  allocated_krw, reserved_term_before_krw,
                  reserved_term_after_krw)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(save_id)
        .bind(scope.run_revision)
        .bind(claim.id)
        .bind(pin.id)
        .bind(allocation.contract_id.get())
        .bind(u8::try_from(index + 1).context("insurance allocation order overflowed")?)
        .bind(allocation.raw_krw)
        .bind(allocation.allocation_krw)
        .bind(aggregate.reserved_before_krw)
        .bind(aggregate.reserved_after_krw)
        .execute(&mut **tx)
        .await?;
        ensure!(insert.rows_affected() == 1);
    }
    if !plan.contract_aggregates.is_empty() {
        apply_contract_aggregates(tx, &scope, &plan.contract_aggregates).await?;
    }
    insert_claim_transition(
        tx,
        &scope,
        claim.id,
        2,
        Some("candidate"),
        to_status,
        None,
        event.resolved_game_day,
        "eventResolved",
    )
    .await?;
    let update = match plan.status {
        InsuranceClaimStatus::NotApplicable => {
            sqlx::query(
                "UPDATE insurance_claim
             SET status = 'notApplicable', resolved_game_day = ?
             WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'candidate'",
            )
            .bind(event.resolved_game_day)
            .bind(claim.id)
            .bind(save_id)
            .bind(scope.run_revision)
            .execute(&mut **tx)
            .await?
        }
        InsuranceClaimStatus::NotCovered => {
            sqlx::query(
                "UPDATE insurance_claim
             SET status = 'notCovered', gross_cost_krw = ?, payout_krw = 0,
                 resolved_game_day = ?
             WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'candidate'",
            )
            .bind(plan.gross_cost_krw)
            .bind(event.resolved_game_day)
            .bind(claim.id)
            .bind(save_id)
            .bind(scope.run_revision)
            .execute(&mut **tx)
            .await?
        }
        InsuranceClaimStatus::Ready => {
            sqlx::query(
                "UPDATE insurance_claim
             SET status = 'ready', gross_cost_krw = ?, payout_krw = ?,
                 resolved_game_day = ?, filing_deadline_game_day = ?
             WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'candidate'",
            )
            .bind(plan.gross_cost_krw)
            .bind(plan.payout_krw)
            .bind(event.resolved_game_day)
            .bind(plan.filing_deadline_game_day)
            .bind(claim.id)
            .bind(save_id)
            .bind(scope.run_revision)
            .execute(&mut **tx)
            .await?
        }
        _ => bail!("insurance claim resolution returned an unsupported state"),
    };
    ensure!(update.rows_affected() == 1);
    Ok(())
}

pub(super) async fn close_insurance_for_new_run_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
) -> Result<()> {
    let contracts: Vec<ContractLockRow> = sqlx::query_as(
        "SELECT id, status, coverage_start_game_day,
                waiting_ends_game_day, term_end_exclusive, coverage_end_exclusive
         FROM insurance_contract
         WHERE save_id = ? AND run_revision = ? AND status = 'active'
         ORDER BY id FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        contracts.len() <= INSURANCE_MAX_ACTIVE_CONTRACTS,
        "new-run insurance contract count exceeded its bound"
    );
    for contract in contracts {
        let charges: Vec<ChargeLockRow> = sqlx::query_as(
            "SELECT id, charge_no, due_game_day, amount_krw, status,
                    scheduled_settlement_id
             FROM insurance_premium_charge
             WHERE save_id = ? AND run_revision = ? AND contract_id = ?
               AND status = 'scheduled' ORDER BY charge_no FOR UPDATE",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(contract.id)
        .fetch_all(&mut **tx)
        .await?;
        let scope = InsuranceScopeRow {
            save_id,
            market_world_id: 0,
            policy_set_id: 0,
            run_revision,
            state_revision: 0,
            game_day,
            cash_krw: 0,
            life_catalog_set_id: 0,
            life_event_component_version_id: 0,
            insurance_component_version_id: 0,
            component_version_key: String::new(),
            availability: String::new(),
            component_sealed: false,
            catalog_sealed: false,
            has_character: true,
        };
        insert_contract_transition(
            tx,
            &scope,
            contract.id,
            3,
            Some("active"),
            "expired",
            None,
            game_day,
            "newRun",
        )
        .await?;
        cancel_charges_and_settlements(tx, &scope, contract.id, &charges, game_day, "newRun")
            .await?;
        let coverage_end_exclusive = contract.term_end_exclusive.min(
            game_day
                .checked_add(1)
                .context("new-run insurance day overflowed")?,
        );
        let update = sqlx::query(
            "UPDATE insurance_contract
             SET status = 'expired', coverage_end_exclusive = ?,
                 terminal_game_day = ?, terminal_reason = 'newRun'
             WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'active'",
        )
        .bind(coverage_end_exclusive)
        .bind(game_day)
        .bind(contract.id)
        .bind(save_id)
        .bind(run_revision)
        .execute(&mut **tx)
        .await?;
        ensure!(update.rows_affected() == 1);
    }
    Ok(())
}

async fn read_scope_for_user(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
    lock: bool,
) -> Result<Option<InsuranceScopeRow>> {
    let sql = if lock {
        "SELECT save.id AS save_id, save.market_world_id, bundle.policy_set_id,
                save.run_revision, save.state_revision, save.game_day, save.cash_krw,
                bundle.life_catalog_set_id, catalog.life_event_component_version_id,
                catalog.insurance_component_version_id,
                component.version_key AS component_version_key,
                component.availability, component.sealed_at IS NOT NULL AS component_sealed,
                catalog.sealed_at IS NOT NULL AS catalog_sealed,
                EXISTS(SELECT 1 FROM `character` WHERE `character`.save_id = save.id)
                    AS has_character
         FROM save
         INNER JOIN run_rule_bundle AS bundle
           ON bundle.save_id = save.id AND bundle.run_revision = save.run_revision
         INNER JOIN life_catalog_set AS catalog ON catalog.id = bundle.life_catalog_set_id
         INNER JOIN life_component_version AS component
           ON component.id = catalog.insurance_component_version_id
          AND component.component_kind = 'insurance'
         WHERE save.user_id = ?
         FOR UPDATE"
    } else {
        "SELECT save.id AS save_id, save.market_world_id, bundle.policy_set_id,
                save.run_revision, save.state_revision, save.game_day, save.cash_krw,
                bundle.life_catalog_set_id, catalog.life_event_component_version_id,
                catalog.insurance_component_version_id,
                component.version_key AS component_version_key,
                component.availability, component.sealed_at IS NOT NULL AS component_sealed,
                catalog.sealed_at IS NOT NULL AS catalog_sealed,
                EXISTS(SELECT 1 FROM `character` WHERE `character`.save_id = save.id)
                    AS has_character
         FROM save
         INNER JOIN run_rule_bundle AS bundle
           ON bundle.save_id = save.id AND bundle.run_revision = save.run_revision
         INNER JOIN life_catalog_set AS catalog ON catalog.id = bundle.life_catalog_set_id
         INNER JOIN life_component_version AS component
           ON component.id = catalog.insurance_component_version_id
          AND component.component_kind = 'insurance'
         WHERE save.user_id = ?"
    };
    let row = sqlx::query_as(sql)
        .bind(user_id)
        .fetch_optional(&mut **tx)
        .await?;
    if let Some(scope) = &row {
        ensure_component_pin(scope)?;
    }
    Ok(row)
}

async fn read_scope_for_save(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    lock: bool,
) -> Result<Option<InsuranceScopeRow>> {
    let sql = if lock {
        "SELECT save.id AS save_id, save.market_world_id, bundle.policy_set_id,
                save.run_revision, save.state_revision, save.game_day, save.cash_krw,
                bundle.life_catalog_set_id, catalog.life_event_component_version_id,
                catalog.insurance_component_version_id,
                component.version_key AS component_version_key,
                component.availability, component.sealed_at IS NOT NULL AS component_sealed,
                catalog.sealed_at IS NOT NULL AS catalog_sealed,
                EXISTS(SELECT 1 FROM `character` WHERE `character`.save_id = save.id)
                    AS has_character
         FROM save
         INNER JOIN run_rule_bundle AS bundle
           ON bundle.save_id = save.id AND bundle.run_revision = save.run_revision
         INNER JOIN life_catalog_set AS catalog ON catalog.id = bundle.life_catalog_set_id
         INNER JOIN life_component_version AS component
           ON component.id = catalog.insurance_component_version_id
          AND component.component_kind = 'insurance'
         WHERE save.id = ?
         FOR UPDATE"
    } else {
        "SELECT save.id AS save_id, save.market_world_id, bundle.policy_set_id,
                save.run_revision, save.state_revision, save.game_day, save.cash_krw,
                bundle.life_catalog_set_id, catalog.life_event_component_version_id,
                catalog.insurance_component_version_id,
                component.version_key AS component_version_key,
                component.availability, component.sealed_at IS NOT NULL AS component_sealed,
                catalog.sealed_at IS NOT NULL AS catalog_sealed,
                EXISTS(SELECT 1 FROM `character` WHERE `character`.save_id = save.id)
                    AS has_character
         FROM save
         INNER JOIN run_rule_bundle AS bundle
           ON bundle.save_id = save.id AND bundle.run_revision = save.run_revision
         INNER JOIN life_catalog_set AS catalog ON catalog.id = bundle.life_catalog_set_id
         INNER JOIN life_component_version AS component
           ON component.id = catalog.insurance_component_version_id
          AND component.component_kind = 'insurance'
         WHERE save.id = ?"
    };
    let row = sqlx::query_as(sql)
        .bind(save_id)
        .fetch_optional(&mut **tx)
        .await?;
    if let Some(scope) = &row {
        ensure_component_pin(scope)?;
    }
    Ok(row)
}

fn ensure_component_pin(scope: &InsuranceScopeRow) -> Result<()> {
    ensure!(scope.catalog_sealed, "current life catalog is not sealed");
    match scope.availability.as_str() {
        "active" => {
            ensure!(
                scope.component_sealed && scope.component_version_key == INSURANCE_COMPONENT_KEY,
                "current run pins an unsupported active insurance component"
            );
        }
        "disabled" => {
            ensure!(
                scope.component_sealed && scope.component_version_key == "disabled-m4a-v1",
                "current run pins an unsupported disabled insurance component"
            );
        }
        _ => bail!("stored insurance availability is invalid"),
    }
    Ok(())
}

fn component_is_active(scope: &InsuranceScopeRow) -> Result<bool> {
    ensure_component_pin(scope)?;
    Ok(scope.availability == "active")
}

async fn load_catalog(
    tx: &mut Transaction<'_, MySql>,
    scope: &InsuranceScopeRow,
    rules: &dyn InsuranceRules,
) -> Result<InsuranceCatalog> {
    ensure!(component_is_active(scope)?);
    let manifest_valid: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM life_component_version AS component
             INNER JOIN life_component_canonical_manifest AS manifest
               ON manifest.life_component_version_id = component.id
              AND BINARY manifest.canonical_sha256 = BINARY component.canonical_sha256
              AND BINARY manifest.canonical_sha256 = BINARY SHA2(manifest.canonical_json, 256)
             INNER JOIN insurance_component_canonical_projection AS projection
               ON projection.life_component_version_id = component.id
              AND BINARY projection.canonical_json = BINARY manifest.canonical_json
             WHERE component.id = ? AND component.component_kind = 'insurance'
               AND component.version_key = ? AND component.availability = 'active'
               AND component.sealed_at IS NOT NULL
         )",
    )
    .bind(scope.insurance_component_version_id)
    .bind(INSURANCE_COMPONENT_KEY)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        manifest_valid,
        "sealed insurance component manifest is invalid"
    );

    let fact_rows: Vec<CatalogFactRow> = sqlx::query_as(
        "SELECT id, fact_order, fact_key, value_type, unit, enum_schema_key,
                window_kind, source_schema_version, source_kind
         FROM insurance_fact_definition
         WHERE life_component_version_id = ? ORDER BY fact_order LIMIT 17",
    )
    .bind(scope.insurance_component_version_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        !fact_rows.is_empty() && fact_rows.len() <= 16,
        "insurance fact registry cardinality is invalid"
    );
    let product_rows: Vec<CatalogProductRow> = sqlx::query_as(
        "SELECT id, schema_version, product_order, product_key, display_name, purpose,
                ranked_availability, CAST(eligibility_ast AS CHAR) AS eligibility_ast_json,
                ast_node_count, ast_max_depth, premium_krw, premium_cadence_game_days,
                term_game_days, waiting_game_days, claim_window_game_days,
                grace_game_days, reinstatement_allowed
         FROM insurance_product_version
         WHERE life_component_version_id = ? ORDER BY product_order LIMIT 17",
    )
    .bind(scope.insurance_component_version_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        !product_rows.is_empty() && product_rows.len() <= INSURANCE_MAX_PRODUCTS,
        "insurance product cardinality is invalid"
    );
    let coverage_rows: Vec<CatalogCoverageRow> = sqlx::query_as(
        "SELECT id, product_version_id, coverage_order, coverage_kind, event_key,
                effect_kind, deductible_krw, occurrence_limit_krw, term_limit_krw
         FROM insurance_product_coverage
         WHERE life_component_version_id = ?
         ORDER BY product_version_id, coverage_order LIMIT 129",
    )
    .bind(scope.insurance_component_version_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        coverage_rows.len() <= INSURANCE_MAX_PRODUCTS * 8,
        "insurance coverage cardinality is invalid"
    );

    let mut catalog = create_fictional_family_care_insurance_catalog();
    ensure!(
        catalog.facts.len() == fact_rows.len()
            && catalog.products.len() == product_rows.len()
            && catalog.products.len() == 1
            && coverage_rows.len() == 1,
        "stored insurance v1 graph differs from its supported fixture"
    );
    catalog.component_version_id = ResourceId::from_u64(scope.insurance_component_version_id);
    for (definition, row) in catalog.facts.iter_mut().zip(fact_rows) {
        ensure!(
            definition.fact_order == row.fact_order
                && definition.fact_key == row.fact_key
                && enum_db(&definition.value_type)? == row.value_type
                && enum_db(&definition.unit)? == row.unit
                && definition.enum_schema_key == row.enum_schema_key
                && enum_db(&definition.window_kind)? == row.window_kind
                && definition.source_schema_version == row.source_schema_version
                && enum_db(&definition.source_kind)? == row.source_kind,
            "stored insurance fact definition differs from schema v1"
        );
        definition.id = ResourceId::from_u64(row.id);
    }
    let product = catalog
        .products
        .first_mut()
        .context("supported insurance catalog has no product")?;
    let row = product_rows
        .first()
        .context("stored insurance catalog has no product")?;
    let stored_ast = super::life_events::parse_eligibility_ast(&row.eligibility_ast_json)
        .context("stored insurance eligibility AST is invalid")?;
    ensure!(
        product.schema_version == row.schema_version
            && product.product_order == row.product_order
            && product.product_key == row.product_key
            && product.display_name == row.display_name
            && enum_db(&product.purpose)? == row.purpose
            && enum_db(&product.ranked_availability)? == row.ranked_availability
            && product.eligibility_ast == stored_ast
            && product.ast_node_count == row.ast_node_count
            && product.ast_max_depth == row.ast_max_depth
            && product.premium_krw == row.premium_krw
            && product.premium_cadence_game_days == row.premium_cadence_game_days
            && product.term_game_days == row.term_game_days
            && product.waiting_game_days == row.waiting_game_days
            && product.claim_window_game_days == row.claim_window_game_days
            && product.grace_game_days == row.grace_game_days
            && product.reinstatement_allowed == row.reinstatement_allowed
            && !product.automatic_renewal,
        "stored insurance product differs from supported schema v1"
    );
    product.product_version_id = ResourceId::from_u64(row.id);
    let coverage = product
        .coverages
        .first_mut()
        .context("supported insurance product has no coverage")?;
    let coverage_row = coverage_rows
        .first()
        .context("stored insurance product has no coverage")?;
    ensure!(
        coverage_row.product_version_id == row.id
            && coverage.coverage_order == coverage_row.coverage_order
            && enum_db(&coverage.coverage_kind)? == coverage_row.coverage_kind
            && coverage.event_key == coverage_row.event_key
            && enum_db(&coverage.effect_kind)? == coverage_row.effect_kind
            && coverage.deductible_krw == coverage_row.deductible_krw
            && coverage.occurrence_limit_krw == coverage_row.occurrence_limit_krw
            && coverage.term_limit_krw == coverage_row.term_limit_krw,
        "stored insurance coverage differs from supported schema v1"
    );
    coverage.coverage_version_id = ResourceId::from_u64(coverage_row.id);
    rules
        .validate_catalog(&catalog)
        .context("stored insurance catalog is invalid")?;
    Ok(catalog)
}

fn enum_db<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value).context("insurance enum serialization failed")? {
        JsonValue::String(raw) => Ok(raw),
        _ => bail!("insurance enum did not serialize as a string"),
    }
}

async fn collect_fact_evidence(
    tx: &mut Transaction<'_, MySql>,
    scope: &InsuranceScopeRow,
    catalog: &InsuranceCatalog,
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
                 WHERE residence.save_id = save.id AND residence.run_revision = save.run_revision
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
        "insurance fact authority returned a negative count"
    );
    ensure!(
        authority.residence_count <= 1,
        "multiple residences are active for insurance eligibility"
    );

    catalog
        .facts
        .iter()
        .map(|definition| {
            ensure!(
                definition.window_kind == LifeEventWindowKind::CurrentGameDay,
                "insurance fact window is unsupported"
            );
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
                    "insurance fact authority adapter is missing for {}",
                    definition.fact_key
                ),
            };
            Ok(LifeEventFactEvidence {
                fact_key: definition.fact_key.clone(),
                value,
            })
        })
        .collect()
}

async fn read_products(
    tx: &mut Transaction<'_, MySql>,
    scope: &InsuranceScopeRow,
    rules: &dyn InsuranceRules,
    catalog: &InsuranceCatalog,
    facts: &[LifeEventFactEvidence],
) -> Result<Vec<InsuranceProductState>> {
    ensure!(catalog.products.len() <= INSURANCE_MAX_PRODUCTS);
    let mut states = Vec::with_capacity(catalog.products.len());
    for product in &catalog.products {
        ensure!(
            product.coverages.len() == 1,
            "public insurance v1 product must have one coverage"
        );
        let coverage = product
            .coverages
            .first()
            .context("insurance product has no public coverage")?;
        let event_name: Option<(String,)> = sqlx::query_as(
            "SELECT display_name FROM life_event_definition
             WHERE life_component_version_id = ? AND BINARY event_key = BINARY ?",
        )
        .bind(scope.life_event_component_version_id)
        .bind(&coverage.event_key)
        .fetch_optional(&mut **tx)
        .await?;
        let event_display_name = event_name
            .map(|row| row.0)
            .context("insurance coverage event is missing from the pinned event component")?;
        let evaluation = rules
            .evaluate_eligibility(InsuranceEligibilityInput {
                catalog,
                product_version_id: product.product_version_id,
                evaluation_game_day: scope.game_day,
                facts,
            })
            .context("insurance eligibility evaluation failed")?;
        let eligibility_status = eligibility_state(evaluation.status);
        let reasons = public_eligibility_reasons(facts, evaluation.status)?;
        states.push(InsuranceProductState {
            id: product.product_version_id,
            product_key: product.product_key.clone(),
            display_name: product.display_name.clone(),
            eligibility_status,
            reasons,
            covered_event_key: coverage.event_key.clone(),
            covered_event_display_name: event_display_name,
            premium_krw: product.premium_krw,
            premium_interval_game_days: product.premium_cadence_game_days,
            term_game_days: product.term_game_days,
            waiting_period_game_days: product.waiting_game_days,
            deductible_krw: coverage.deductible_krw,
            occurrence_limit_krw: coverage.occurrence_limit_krw,
            term_limit_krw: coverage.term_limit_krw,
            claim_window_game_days: product.claim_window_game_days,
        });
    }
    Ok(states)
}

fn eligibility_state(status: InsuranceEligibilityStatus) -> InsuranceEligibilityStatusState {
    match status {
        InsuranceEligibilityStatus::Eligible => InsuranceEligibilityStatusState::Eligible,
        InsuranceEligibilityStatus::Ineligible => InsuranceEligibilityStatusState::Ineligible,
        InsuranceEligibilityStatus::Indeterminate => InsuranceEligibilityStatusState::Indeterminate,
    }
}

fn public_eligibility_reasons(
    facts: &[LifeEventFactEvidence],
    status: InsuranceEligibilityStatus,
) -> Result<Vec<InsuranceEligibilityReasonState>> {
    if status == InsuranceEligibilityStatus::Eligible {
        return Ok(Vec::new());
    }
    if facts
        .iter()
        .any(|fact| matches!(fact.value, LifeEventEvidenceValue::Unknown(_)))
    {
        ensure!(status == InsuranceEligibilityStatus::Indeterminate);
        return Ok(vec![InsuranceEligibilityReasonState::AuthorityUnavailable]);
    }
    ensure!(status == InsuranceEligibilityStatus::Ineligible);
    let values = facts
        .iter()
        .map(|fact| (fact.fact_key.as_str(), &fact.value))
        .collect::<BTreeMap<_, _>>();
    let mut reasons = Vec::new();
    match values.get("character.age") {
        Some(LifeEventEvidenceValue::Known(LifeEventValue::AgeYears(age)))
            if !(22..=67).contains(age) =>
        {
            reasons.push(InsuranceEligibilityReasonState::AgeOutsideRange);
        }
        Some(LifeEventEvidenceValue::Known(LifeEventValue::AgeYears(_))) => {}
        _ => bail!("insurance age evidence has an invalid type"),
    }
    match values.get("household.dependentCount") {
        Some(LifeEventEvidenceValue::Known(LifeEventValue::Count(count))) if *count < 1 => {
            reasons.push(InsuranceEligibilityReasonState::DependentRequired);
        }
        Some(LifeEventEvidenceValue::Known(LifeEventValue::Count(_))) => {}
        _ => bail!("insurance dependent evidence has an invalid type"),
    }
    match values.get("residence.exists") {
        Some(LifeEventEvidenceValue::Known(LifeEventValue::Boolean(false))) => {
            reasons.push(InsuranceEligibilityReasonState::ResidenceRequired);
        }
        Some(LifeEventEvidenceValue::Known(LifeEventValue::Boolean(true))) => {}
        _ => bail!("insurance residence evidence has an invalid type"),
    }
    match values.get("military.status") {
        Some(LifeEventEvidenceValue::Known(LifeEventValue::Enum { schema_key, value }))
            if schema_key == "military" && value == "serving" =>
        {
            reasons.push(InsuranceEligibilityReasonState::MilitaryServing);
        }
        Some(LifeEventEvidenceValue::Known(LifeEventValue::Enum { schema_key, .. }))
            if schema_key == "military" => {}
        _ => bail!("insurance military evidence has an invalid type"),
    }
    ensure!(
        !reasons.is_empty(),
        "ineligible insurance facts have no public reason"
    );
    Ok(reasons)
}

async fn read_contract_page(
    tx: &mut Transaction<'_, MySql>,
    scope: &InsuranceScopeRow,
    cursor: Option<InsuranceCursor>,
) -> Result<(Vec<InsuranceContractState>, bool)> {
    if cursor.is_some_and(|value| value.contracts_exhausted) {
        return Ok((Vec::new(), false));
    }
    let rows: Vec<ContractPublicRow> = if let Some(cursor) = cursor {
        ensure!(
            cursor.contract_id != 0,
            "insurance contract cursor has no anchor"
        );
        sqlx::query_as(
            "SELECT contract.id, contract.product_version_id, product.product_key,
                    product.display_name, contract.status, contract.start_game_day,
                    contract.coverage_start_game_day, contract.waiting_ends_game_day,
                    contract.coverage_end_exclusive, product.premium_krw,
                    coverage.term_limit_krw, contract.paid_term_krw,
                    contract.reserved_term_krw,
                    (SELECT MIN(charge.due_game_day) FROM insurance_premium_charge AS charge
                     WHERE charge.save_id = contract.save_id
                       AND charge.run_revision = contract.run_revision
                       AND charge.contract_id = contract.id
                       AND charge.status = 'scheduled') AS next_premium_due_game_day
             FROM insurance_contract AS contract
             INNER JOIN insurance_product_version AS product
               ON product.id = contract.product_version_id
              AND product.life_component_version_id = contract.insurance_component_version_id
             INNER JOIN insurance_product_coverage AS coverage
               ON coverage.product_version_id = product.id
              AND coverage.life_component_version_id = product.life_component_version_id
              AND coverage.coverage_order = 1
             WHERE contract.save_id = ? AND contract.run_revision = ?
               AND contract.insurance_component_version_id = ?
               AND contract.status <> 'pending'
               AND (contract.start_game_day < ?
                    OR (contract.start_game_day = ? AND contract.id < ?))
             ORDER BY contract.start_game_day DESC, contract.id DESC LIMIT 21",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(scope.insurance_component_version_id)
        .bind(cursor.contract_start_game_day)
        .bind(cursor.contract_start_game_day)
        .bind(cursor.contract_id)
        .fetch_all(&mut **tx)
        .await?
    } else {
        sqlx::query_as(
            "SELECT contract.id, contract.product_version_id, product.product_key,
                    product.display_name, contract.status, contract.start_game_day,
                    contract.coverage_start_game_day, contract.waiting_ends_game_day,
                    contract.coverage_end_exclusive, product.premium_krw,
                    coverage.term_limit_krw, contract.paid_term_krw,
                    contract.reserved_term_krw,
                    (SELECT MIN(charge.due_game_day) FROM insurance_premium_charge AS charge
                     WHERE charge.save_id = contract.save_id
                       AND charge.run_revision = contract.run_revision
                       AND charge.contract_id = contract.id
                       AND charge.status = 'scheduled') AS next_premium_due_game_day
             FROM insurance_contract AS contract
             INNER JOIN insurance_product_version AS product
               ON product.id = contract.product_version_id
              AND product.life_component_version_id = contract.insurance_component_version_id
             INNER JOIN insurance_product_coverage AS coverage
               ON coverage.product_version_id = product.id
              AND coverage.life_component_version_id = product.life_component_version_id
              AND coverage.coverage_order = 1
             WHERE contract.save_id = ? AND contract.run_revision = ?
               AND contract.insurance_component_version_id = ?
               AND contract.status <> 'pending'
             ORDER BY contract.start_game_day DESC, contract.id DESC LIMIT 21",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(scope.insurance_component_version_id)
        .fetch_all(&mut **tx)
        .await?
    };
    ensure!(
        rows.len() <= QUERY_BOUND,
        "insurance contract query escaped its bound"
    );
    let has_more = rows.len() == QUERY_BOUND;
    let states = rows
        .into_iter()
        .take(PAGE_SIZE)
        .map(contract_state)
        .collect::<Result<Vec<_>>>()?;
    Ok((states, has_more))
}

async fn read_active_contracts(
    tx: &mut Transaction<'_, MySql>,
    scope: &InsuranceScopeRow,
) -> Result<Vec<InsuranceContractState>> {
    let rows: Vec<ContractPublicRow> = sqlx::query_as(
        "SELECT contract.id, contract.product_version_id, product.product_key,
                product.display_name, contract.status, contract.start_game_day,
                contract.coverage_start_game_day, contract.waiting_ends_game_day,
                contract.coverage_end_exclusive, product.premium_krw,
                coverage.term_limit_krw, contract.paid_term_krw,
                contract.reserved_term_krw,
                (SELECT MIN(charge.due_game_day) FROM insurance_premium_charge AS charge
                 WHERE charge.save_id = contract.save_id
                   AND charge.run_revision = contract.run_revision
                   AND charge.contract_id = contract.id
                   AND charge.status = 'scheduled') AS next_premium_due_game_day
         FROM insurance_contract AS contract
         INNER JOIN insurance_product_version AS product
           ON product.id = contract.product_version_id
          AND product.life_component_version_id = contract.insurance_component_version_id
         INNER JOIN insurance_product_coverage AS coverage
           ON coverage.product_version_id = product.id
          AND coverage.life_component_version_id = product.life_component_version_id
          AND coverage.coverage_order = 1
         WHERE contract.save_id = ? AND contract.run_revision = ?
           AND contract.insurance_component_version_id = ? AND contract.status = 'active'
         ORDER BY contract.id LIMIT 9",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.insurance_component_version_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() <= INSURANCE_MAX_ACTIVE_CONTRACTS,
        "active insurance contracts exceeded their bound"
    );
    rows.into_iter().map(contract_state).collect()
}

fn contract_state(row: ContractPublicRow) -> Result<InsuranceContractState> {
    ensure!(row.start_game_day == row.coverage_start_game_day);
    let status = match row.status.as_str() {
        "active" => InsuranceContractStatusState::Active,
        "lapsed" => InsuranceContractStatusState::Lapsed,
        "expired" => InsuranceContractStatusState::Expired,
        "cancelled" => InsuranceContractStatusState::Cancelled,
        _ => bail!("stored insurance contract status is invalid"),
    };
    if status != InsuranceContractStatusState::Active {
        ensure!(
            row.next_premium_due_game_day.is_none(),
            "terminal insurance contract has a scheduled premium"
        );
    }
    let used = row
        .paid_term_krw
        .checked_add(row.reserved_term_krw)
        .context("insurance term usage overflowed")?;
    let remaining_benefit_krw = row
        .term_limit_krw
        .checked_sub(used)
        .context("insurance term usage exceeds its limit")?;
    ensure!(remaining_benefit_krw >= 0);
    Ok(InsuranceContractState {
        id: ResourceId::from_u64(row.id),
        product_version_id: ResourceId::from_u64(row.product_version_id),
        product_key: row.product_key,
        display_name: row.display_name,
        status,
        coverage_start_game_day: row.coverage_start_game_day,
        waiting_ends_game_day: row.waiting_ends_game_day,
        coverage_end_exclusive: row.coverage_end_exclusive,
        next_premium_due_game_day: row.next_premium_due_game_day,
        premium_krw: row.premium_krw,
        paid_benefit_krw: row.paid_term_krw,
        reserved_benefit_krw: row.reserved_term_krw,
        remaining_benefit_krw,
    })
}

async fn read_pending_claims(
    tx: &mut Transaction<'_, MySql>,
    scope: &InsuranceScopeRow,
) -> Result<Vec<PendingInsuranceClaimState>> {
    let rows: Vec<ClaimPublicRow> = sqlx::query_as(
        "SELECT claim.id, claim.life_event_instance_id AS event_id,
                definition.event_key, definition.display_name AS event_display_name,
                claim.status, claim.offered_game_day, claim.gross_cost_krw,
                claim.payout_krw, claim.resolved_game_day,
                claim.filing_deadline_game_day, claim.paid_game_day
         FROM insurance_claim AS claim
         INNER JOIN life_event_instance AS instance
           ON instance.id = claim.life_event_instance_id
          AND instance.save_id = claim.save_id AND instance.run_revision = claim.run_revision
          AND instance.life_event_component_version_id = claim.life_event_component_version_id
         INNER JOIN life_event_definition AS definition
           ON definition.id = instance.life_event_definition_id
          AND definition.life_component_version_id = instance.life_event_component_version_id
         WHERE claim.save_id = ? AND claim.run_revision = ?
           AND claim.insurance_component_version_id = ?
           AND claim.status IN ('candidate', 'ready')
         ORDER BY claim.id LIMIT 9",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.insurance_component_version_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() <= INSURANCE_MAX_CLAIM_CONTRACTS,
        "pending insurance claims exceeded their public bound"
    );
    let allocations = read_ready_allocations(tx, scope).await?;
    let mut grouped = group_allocations(allocations)?;
    let mut states = Vec::with_capacity(rows.len());
    for row in rows {
        let state = match row.status.as_str() {
            "candidate" => {
                ensure!(
                    row.gross_cost_krw.is_none()
                        && row.payout_krw.is_none()
                        && row.resolved_game_day.is_none()
                        && row.filing_deadline_game_day.is_none()
                        && grouped.remove(&row.id).is_none(),
                    "candidate insurance claim has resolved projections"
                );
                PendingInsuranceClaimState::Candidate {
                    id: ResourceId::from_u64(row.id),
                    event_id: ResourceId::from_u64(row.event_id),
                    event_key: row.event_key,
                    event_display_name: row.event_display_name,
                    offered_game_day: row.offered_game_day,
                }
            }
            "ready" => {
                let gross_cost_krw = row
                    .gross_cost_krw
                    .filter(|value| *value > 0)
                    .context("ready insurance claim has no gross cost")?;
                let payout_krw = row
                    .payout_krw
                    .filter(|value| *value > 0)
                    .context("ready insurance claim has no payout")?;
                let filing_deadline_game_day = row
                    .filing_deadline_game_day
                    .context("ready insurance claim has no filing deadline")?;
                ensure!(row.resolved_game_day.is_some() && row.paid_game_day.is_none());
                let contract_allocations = grouped
                    .remove(&row.id)
                    .context("ready insurance claim has no allocations")?;
                PendingInsuranceClaimState::Ready {
                    id: ResourceId::from_u64(row.id),
                    event_id: ResourceId::from_u64(row.event_id),
                    event_key: row.event_key,
                    event_display_name: row.event_display_name,
                    offered_game_day: row.offered_game_day,
                    gross_cost_krw,
                    payout_krw,
                    filing_deadline_game_day,
                    contract_allocations,
                }
            }
            _ => bail!("stored pending insurance claim status is invalid"),
        };
        states.push(state);
    }
    ensure!(
        grouped.is_empty(),
        "insurance allocation escaped pending claim bound"
    );
    Ok(states)
}

async fn read_claim_history_page(
    tx: &mut Transaction<'_, MySql>,
    scope: &InsuranceScopeRow,
    cursor: Option<InsuranceCursor>,
) -> Result<(Vec<InsuranceClaimHistoryState>, bool)> {
    if cursor.is_some_and(|value| value.claims_exhausted) {
        return Ok((Vec::new(), false));
    }
    let rows: Vec<ClaimPublicRow> = if let Some(cursor) = cursor {
        ensure!(cursor.claim_id != 0, "insurance claim cursor has no anchor");
        sqlx::query_as(
            "SELECT claim.id, claim.life_event_instance_id AS event_id,
                    definition.event_key, definition.display_name AS event_display_name,
                    claim.status, claim.offered_game_day, claim.gross_cost_krw,
                    claim.payout_krw, claim.resolved_game_day,
                    claim.filing_deadline_game_day, claim.paid_game_day
             FROM insurance_claim AS claim
             INNER JOIN life_event_instance AS instance
               ON instance.id = claim.life_event_instance_id
              AND instance.save_id = claim.save_id AND instance.run_revision = claim.run_revision
              AND instance.life_event_component_version_id = claim.life_event_component_version_id
             INNER JOIN life_event_definition AS definition
               ON definition.id = instance.life_event_definition_id
              AND definition.life_component_version_id = instance.life_event_component_version_id
             WHERE claim.save_id = ? AND claim.run_revision = ?
               AND claim.insurance_component_version_id = ?
               AND claim.status IN ('notApplicable', 'notCovered', 'paid', 'expired')
               AND (claim.resolved_game_day < ?
                    OR (claim.resolved_game_day = ? AND claim.id < ?))
             ORDER BY claim.resolved_game_day DESC, claim.id DESC LIMIT 21",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(scope.insurance_component_version_id)
        .bind(cursor.claim_resolved_game_day)
        .bind(cursor.claim_resolved_game_day)
        .bind(cursor.claim_id)
        .fetch_all(&mut **tx)
        .await?
    } else {
        sqlx::query_as(
            "SELECT claim.id, claim.life_event_instance_id AS event_id,
                    definition.event_key, definition.display_name AS event_display_name,
                    claim.status, claim.offered_game_day, claim.gross_cost_krw,
                    claim.payout_krw, claim.resolved_game_day,
                    claim.filing_deadline_game_day, claim.paid_game_day
             FROM insurance_claim AS claim
             INNER JOIN life_event_instance AS instance
               ON instance.id = claim.life_event_instance_id
              AND instance.save_id = claim.save_id AND instance.run_revision = claim.run_revision
              AND instance.life_event_component_version_id = claim.life_event_component_version_id
             INNER JOIN life_event_definition AS definition
               ON definition.id = instance.life_event_definition_id
              AND definition.life_component_version_id = instance.life_event_component_version_id
             WHERE claim.save_id = ? AND claim.run_revision = ?
               AND claim.insurance_component_version_id = ?
               AND claim.status IN ('notApplicable', 'notCovered', 'paid', 'expired')
             ORDER BY claim.resolved_game_day DESC, claim.id DESC LIMIT 21",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(scope.insurance_component_version_id)
        .fetch_all(&mut **tx)
        .await?
    };
    ensure!(
        rows.len() <= QUERY_BOUND,
        "insurance claim history query escaped its bound"
    );
    let has_more = rows.len() == QUERY_BOUND;
    let rows = rows.into_iter().take(PAGE_SIZE).collect::<Vec<_>>();
    let allocations = if rows.is_empty() {
        Vec::new()
    } else {
        let ids = rows
            .iter()
            .map(|row| row.id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT allocation.claim_id, allocation.contract_id, pin.deductible_krw,
                    allocation.allocated_krw
             FROM insurance_claim_allocation AS allocation
             INNER JOIN insurance_claim_contract_pin AS pin
               ON pin.id = allocation.claim_contract_pin_id
              AND pin.save_id = allocation.save_id AND pin.run_revision = allocation.run_revision
              AND pin.claim_id = allocation.claim_id
             WHERE allocation.save_id = ? AND allocation.run_revision = ?
               AND allocation.claim_id IN ({ids})
             ORDER BY allocation.claim_id, allocation.allocation_order LIMIT 161"
        );
        // Every interpolated value is an unsigned identifier read from the locked page.
        sqlx::query_as::<_, AllocationPublicRow>(sql.as_str())
            .bind(scope.save_id)
            .bind(scope.run_revision)
            .fetch_all(&mut **tx)
            .await?
    };
    ensure!(allocations.len() <= PAGE_SIZE * INSURANCE_MAX_CLAIM_CONTRACTS);
    let mut grouped = group_allocations(allocations)?;
    let mut history = Vec::with_capacity(rows.len());
    for row in rows {
        let resolved_game_day = row
            .resolved_game_day
            .context("terminal insurance claim has no resolution day")?;
        let id = ResourceId::from_u64(row.id);
        let event_id = ResourceId::from_u64(row.event_id);
        let state = match row.status.as_str() {
            "notApplicable" => {
                ensure!(
                    row.gross_cost_krw.is_none()
                        && row.payout_krw.is_none()
                        && row.filing_deadline_game_day.is_none()
                        && grouped.remove(&row.id).is_none()
                );
                InsuranceClaimHistoryState::NotApplicable {
                    id,
                    event_id,
                    event_key: row.event_key,
                    event_display_name: row.event_display_name,
                    offered_game_day: row.offered_game_day,
                    resolved_game_day,
                }
            }
            "notCovered" => {
                let gross_cost_krw = row
                    .gross_cost_krw
                    .filter(|value| *value > 0)
                    .context("not-covered insurance claim has no gross cost")?;
                ensure!(row.payout_krw == Some(0) && grouped.remove(&row.id).is_none());
                InsuranceClaimHistoryState::NotCovered {
                    id,
                    event_id,
                    event_key: row.event_key,
                    event_display_name: row.event_display_name,
                    offered_game_day: row.offered_game_day,
                    resolved_game_day,
                    gross_cost_krw,
                }
            }
            "paid" => InsuranceClaimHistoryState::Paid {
                id,
                event_id,
                event_key: row.event_key,
                event_display_name: row.event_display_name,
                offered_game_day: row.offered_game_day,
                resolved_game_day,
                gross_cost_krw: row
                    .gross_cost_krw
                    .filter(|value| *value > 0)
                    .context("paid insurance claim has no gross cost")?,
                payout_krw: row
                    .payout_krw
                    .filter(|value| *value > 0)
                    .context("paid insurance claim has no payout")?,
                filing_deadline_game_day: row
                    .filing_deadline_game_day
                    .context("paid insurance claim has no filing deadline")?,
                paid_game_day: row
                    .paid_game_day
                    .context("paid insurance claim has no paid day")?,
                contract_allocations: grouped
                    .remove(&row.id)
                    .context("paid insurance claim has no allocations")?,
            },
            "expired" => InsuranceClaimHistoryState::Expired {
                id,
                event_id,
                event_key: row.event_key,
                event_display_name: row.event_display_name,
                offered_game_day: row.offered_game_day,
                resolved_game_day,
                gross_cost_krw: row
                    .gross_cost_krw
                    .filter(|value| *value > 0)
                    .context("expired insurance claim has no gross cost")?,
                payout_krw: row
                    .payout_krw
                    .filter(|value| *value > 0)
                    .context("expired insurance claim has no payout")?,
                filing_deadline_game_day: row
                    .filing_deadline_game_day
                    .context("expired insurance claim has no filing deadline")?,
                contract_allocations: grouped
                    .remove(&row.id)
                    .context("expired insurance claim has no allocations")?,
            },
            _ => bail!("stored insurance history status is invalid"),
        };
        history.push(state);
    }
    ensure!(
        grouped.is_empty(),
        "insurance allocation escaped history page"
    );
    Ok((history, has_more))
}

async fn read_ready_allocations(
    tx: &mut Transaction<'_, MySql>,
    scope: &InsuranceScopeRow,
) -> Result<Vec<AllocationPublicRow>> {
    let rows = sqlx::query_as(
        "SELECT allocation.claim_id, allocation.contract_id, pin.deductible_krw,
                allocation.allocated_krw
         FROM insurance_claim_allocation AS allocation
         INNER JOIN insurance_claim AS claim
           ON claim.id = allocation.claim_id AND claim.save_id = allocation.save_id
          AND claim.run_revision = allocation.run_revision
         INNER JOIN insurance_claim_contract_pin AS pin
           ON pin.id = allocation.claim_contract_pin_id
          AND pin.save_id = allocation.save_id AND pin.run_revision = allocation.run_revision
          AND pin.claim_id = allocation.claim_id
         WHERE allocation.save_id = ? AND allocation.run_revision = ?
           AND claim.insurance_component_version_id = ? AND claim.status = 'ready'
         ORDER BY allocation.claim_id, allocation.allocation_order LIMIT 65",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.insurance_component_version_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() <= INSURANCE_MAX_CLAIM_CONTRACTS * INSURANCE_MAX_CLAIM_CONTRACTS,
        "pending insurance allocations exceeded their bound"
    );
    Ok(rows)
}

fn group_allocations(
    rows: Vec<AllocationPublicRow>,
) -> Result<BTreeMap<u64, Vec<InsuranceClaimAllocationState>>> {
    let mut grouped = BTreeMap::<u64, Vec<InsuranceClaimAllocationState>>::new();
    for row in rows {
        ensure!(row.deductible_krw >= 0 && row.allocated_krw > 0);
        let allocations = grouped.entry(row.claim_id).or_default();
        ensure!(
            allocations.len() < INSURANCE_MAX_CLAIM_CONTRACTS,
            "insurance claim allocation count exceeded its bound"
        );
        allocations.push(InsuranceClaimAllocationState {
            contract_id: ResourceId::from_u64(row.contract_id),
            deductible_krw: row.deductible_krw,
            payout_krw: row.allocated_krw,
        });
    }
    Ok(grouped)
}

fn next_cursor(
    scope: &InsuranceScopeRow,
    prior: Option<InsuranceCursor>,
    contracts: &[InsuranceContractState],
    contract_more: bool,
    history: &[InsuranceClaimHistoryState],
    claim_more: bool,
) -> Result<Option<String>> {
    if !contract_more && !claim_more {
        return Ok(None);
    }
    let prior = prior.unwrap_or(InsuranceCursor {
        save_id: scope.save_id,
        run_revision: scope.run_revision,
        component_version_id: scope.insurance_component_version_id,
        contracts_exhausted: false,
        claims_exhausted: false,
        contract_start_game_day: 0,
        contract_id: 0,
        claim_resolved_game_day: 0,
        claim_id: 0,
    });
    let (contract_start_game_day, contract_id) = contracts.last().map_or(
        (prior.contract_start_game_day, prior.contract_id),
        |contract| (contract.coverage_start_game_day, contract.id.get()),
    );
    let (claim_resolved_game_day, claim_id) = history.last().map_or(
        (prior.claim_resolved_game_day, prior.claim_id),
        claim_history_anchor,
    );
    let cursor = InsuranceCursor {
        save_id: scope.save_id,
        run_revision: scope.run_revision,
        component_version_id: scope.insurance_component_version_id,
        contracts_exhausted: !contract_more,
        claims_exhausted: !claim_more,
        contract_start_game_day,
        contract_id,
        claim_resolved_game_day,
        claim_id,
    };
    ensure!(
        cursor.contracts_exhausted || cursor.contract_id != 0,
        "continuing insurance contract window has no anchor"
    );
    ensure!(
        cursor.claims_exhausted || cursor.claim_id != 0,
        "continuing insurance claim window has no anchor"
    );
    Ok(Some(encode_cursor(cursor)))
}

fn claim_history_anchor(state: &InsuranceClaimHistoryState) -> (u32, u64) {
    match state {
        InsuranceClaimHistoryState::NotApplicable {
            id,
            resolved_game_day,
            ..
        }
        | InsuranceClaimHistoryState::NotCovered {
            id,
            resolved_game_day,
            ..
        }
        | InsuranceClaimHistoryState::Paid {
            id,
            resolved_game_day,
            ..
        }
        | InsuranceClaimHistoryState::Expired {
            id,
            resolved_game_day,
            ..
        } => (*resolved_game_day, id.get()),
    }
}

async fn cursor_anchors_exist(
    tx: &mut Transaction<'_, MySql>,
    scope: &InsuranceScopeRow,
    cursor: InsuranceCursor,
) -> Result<bool> {
    let contract_valid = if cursor.contract_id == 0 {
        cursor.contracts_exhausted && cursor.contract_start_game_day == 0
    } else {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM insurance_contract
             WHERE id = ? AND save_id = ? AND run_revision = ?
               AND insurance_component_version_id = ? AND start_game_day = ?
               AND status <> 'pending')",
        )
        .bind(cursor.contract_id)
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(scope.insurance_component_version_id)
        .bind(cursor.contract_start_game_day)
        .fetch_one(&mut **tx)
        .await?
    };
    let claim_valid = if cursor.claim_id == 0 {
        cursor.claims_exhausted && cursor.claim_resolved_game_day == 0
    } else {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM insurance_claim
             WHERE id = ? AND save_id = ? AND run_revision = ?
               AND insurance_component_version_id = ? AND resolved_game_day = ?
               AND status IN ('notApplicable', 'notCovered', 'paid', 'expired'))",
        )
        .bind(cursor.claim_id)
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(scope.insurance_component_version_id)
        .bind(cursor.claim_resolved_game_day)
        .fetch_one(&mut **tx)
        .await?
    };
    Ok(contract_valid && claim_valid)
}

fn cursor_matches_scope(cursor: InsuranceCursor, scope: &InsuranceScopeRow) -> bool {
    cursor.save_id == scope.save_id
        && cursor.run_revision == scope.run_revision
        && cursor.component_version_id == scope.insurance_component_version_id
}

fn encode_cursor(cursor: InsuranceCursor) -> String {
    let mut payload = Vec::with_capacity(CURSOR_BYTES);
    payload.push(CURSOR_VERSION);
    payload.extend_from_slice(&cursor.save_id.to_be_bytes());
    payload.extend_from_slice(&cursor.run_revision.to_be_bytes());
    payload.extend_from_slice(&cursor.component_version_id.to_be_bytes());
    payload.push(u8::from(cursor.contracts_exhausted) | (u8::from(cursor.claims_exhausted) << 1));
    payload.extend_from_slice(&cursor.contract_start_game_day.to_be_bytes());
    payload.extend_from_slice(&cursor.contract_id.to_be_bytes());
    payload.extend_from_slice(&cursor.claim_resolved_game_day.to_be_bytes());
    payload.extend_from_slice(&cursor.claim_id.to_be_bytes());
    let checksum = cursor_checksum(&payload);
    payload.extend_from_slice(&checksum[..CURSOR_CHECKSUM_BYTES]);
    URL_SAFE_NO_PAD.encode(payload)
}

fn decode_cursor(raw: &str) -> Result<InsuranceCursor> {
    ensure!(!raw.is_empty() && raw.len() <= 512 && raw.is_ascii());
    let decoded = URL_SAFE_NO_PAD
        .decode(raw)
        .context("insurance cursor is not canonical base64url")?;
    ensure!(
        decoded.len() == CURSOR_BYTES,
        "insurance cursor has an invalid length"
    );
    let (payload, checksum) = decoded.split_at(CURSOR_PAYLOAD_BYTES);
    ensure!(
        checksum == &cursor_checksum(payload)[..CURSOR_CHECKSUM_BYTES],
        "insurance cursor checksum is invalid"
    );
    ensure!(
        payload[0] == CURSOR_VERSION,
        "insurance cursor version is unsupported"
    );
    let flags = payload[21];
    ensure!(flags & !0b11 == 0, "insurance cursor flags are invalid");
    let cursor = InsuranceCursor {
        save_id: read_u64(&payload[1..9])?,
        run_revision: read_u32(&payload[9..13])?,
        component_version_id: read_u64(&payload[13..21])?,
        contracts_exhausted: flags & 1 != 0,
        claims_exhausted: flags & 2 != 0,
        contract_start_game_day: read_u32(&payload[22..26])?,
        contract_id: read_u64(&payload[26..34])?,
        claim_resolved_game_day: read_u32(&payload[34..38])?,
        claim_id: read_u64(&payload[38..46])?,
    };
    ensure!(
        cursor.save_id != 0 && cursor.component_version_id != 0,
        "insurance cursor contains a zero scope identifier"
    );
    ensure!(
        encode_cursor(cursor) == raw,
        "insurance cursor is not canonically encoded"
    );
    Ok(cursor)
}

fn cursor_checksum(payload: &[u8]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(CURSOR_DOMAIN);
    digest.update(payload);
    digest.finalize().into()
}

fn read_u64(bytes: &[u8]) -> Result<u64> {
    let bytes: [u8; 8] = bytes
        .try_into()
        .context("insurance cursor u64 width is invalid")?;
    Ok(u64::from_be_bytes(bytes))
}

fn read_u32(bytes: &[u8]) -> Result<u32> {
    let bytes: [u8; 4] = bytes
        .try_into()
        .context("insurance cursor u32 width is invalid")?;
    Ok(u32::from_be_bytes(bytes))
}

fn has_current_cursor(scope: &InsuranceScopeRow, cursor: crate::finance::CommandCursor) -> bool {
    scope.run_revision == cursor.expected_run_revision
        && scope.state_revision == cursor.expected_state_revision
        && scope.game_day == cursor.expected_game_day
}

async fn read_stored_receipt(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    command_id: &str,
) -> Result<StoredCommandReceiptRow> {
    sqlx::query_as(
        "SELECT command_kind, payload_sha256, CAST(result AS CHAR) AS result_json,
                ledger_transaction_id
         FROM command_receipt WHERE save_id = ? AND command_id = ? FOR SHARE",
    )
    .bind(save_id)
    .bind(command_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("insurance command identity has no final receipt")
}

#[allow(clippy::too_many_arguments)]
async fn insert_contract_transition(
    tx: &mut Transaction<'_, MySql>,
    scope: &InsuranceScopeRow,
    contract_id: u64,
    transition_no: u8,
    from_status: Option<&str>,
    to_status: &str,
    command_id: Option<&str>,
    transition_game_day: u32,
    transition_reason: &str,
) -> Result<()> {
    let insert = sqlx::query(
        "INSERT INTO insurance_contract_transition
             (save_id, run_revision, contract_id, transition_no,
              from_status, to_status, command_id, transition_game_day, transition_reason)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(contract_id)
    .bind(transition_no)
    .bind(from_status)
    .bind(to_status)
    .bind(command_id)
    .bind(transition_game_day)
    .bind(transition_reason)
    .execute(&mut **tx)
    .await?;
    ensure!(
        insert.rows_affected() == 1,
        "insurance contract transition was not inserted"
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_claim_transition(
    tx: &mut Transaction<'_, MySql>,
    scope: &InsuranceScopeRow,
    claim_id: u64,
    transition_no: u8,
    from_status: Option<&str>,
    to_status: &str,
    command_id: Option<&str>,
    transition_game_day: u32,
    transition_reason: &str,
) -> Result<()> {
    let insert = sqlx::query(
        "INSERT INTO insurance_claim_transition
             (save_id, run_revision, claim_id, transition_no,
              from_status, to_status, command_id, transition_game_day, transition_reason)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(claim_id)
    .bind(transition_no)
    .bind(from_status)
    .bind(to_status)
    .bind(command_id)
    .bind(transition_game_day)
    .bind(transition_reason)
    .execute(&mut **tx)
    .await?;
    ensure!(
        insert.rows_affected() == 1,
        "insurance claim transition was not inserted"
    );
    Ok(())
}

fn premium_settlement_payload(contract_id: u64, charge_id: u64, charge_no: u16) -> Result<String> {
    ensure!((2..=12).contains(&charge_no));
    serde_json::to_string(&json!({
        "version": 1,
        "insuranceContractId": contract_id.to_string(),
        "premiumChargeId": charge_id.to_string(),
        "chargeNo": charge_no,
    }))
    .context("insurance premium settlement payload serialization failed")
}

#[allow(clippy::too_many_arguments)]
async fn write_insurance_ledger(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    scope: &InsuranceScopeRow,
    plan: &InsuranceLedgerPlan,
    source_kind: LedgerSourceKind,
    source_id: u64,
    game_day: u32,
    description: &str,
) -> Result<u64> {
    ensure!(
        plan.wallet_cash_before_krw == scope.cash_krw
            && plan
                .wallet_cash_before_krw
                .checked_add(plan.wallet_delta_krw)
                == Some(plan.wallet_cash_after_krw),
        "insurance ledger plan escaped its wallet scope"
    );
    let postings = plan
        .postings
        .iter()
        .map(|posting| {
            let account_code = match posting.account_code {
                InsuranceLedgerAccountCode::Wallet => LedgerAccountCode::Wallet,
                InsuranceLedgerAccountCode::InsurancePremiumExpense => {
                    LedgerAccountCode::InsurancePremiumExpense
                }
                InsuranceLedgerAccountCode::InsuranceClaimRecovery => {
                    LedgerAccountCode::InsuranceClaimRecovery
                }
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
                kind: source_kind,
                source_id: source_id.to_string(),
            },
            game_day,
            description: description.to_owned(),
            postings,
        })
        .context("insurance finance ledger is invalid")?;
    write_ledger_transaction(tx, &ledger).await
}

async fn update_save_after_command(
    tx: &mut Transaction<'_, MySql>,
    scope: &InsuranceScopeRow,
    wallet_cash_after_krw: i64,
) -> Result<GameCommandCursor> {
    let state_revision = scope
        .state_revision
        .checked_add(1)
        .context("insurance state revision overflowed")?;
    let update = sqlx::query(
        "UPDATE save SET cash_krw = ?, state_revision = ?
         WHERE id = ? AND run_revision = ? AND state_revision = ?
           AND game_day = ? AND cash_krw = ?",
    )
    .bind(wallet_cash_after_krw)
    .bind(state_revision)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.state_revision)
    .bind(scope.game_day)
    .bind(scope.cash_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "insurance save cursor changed under lock"
    );
    Ok(GameCommandCursor {
        run_revision: scope.run_revision,
        state_revision,
        game_day: scope.game_day,
    })
}

#[allow(clippy::too_many_arguments)]
async fn write_command_receipts<T: Serialize>(
    tx: &mut Transaction<'_, MySql>,
    scope: &InsuranceScopeRow,
    identity: &CommandIdentitySpec<'_>,
    committed_cursor: GameCommandCursor,
    receipt: &T,
    contract_id: Option<u64>,
    claim_id: Option<u64>,
    ledger_transaction_id: Option<u64>,
) -> Result<()> {
    write_game_command_receipt(
        tx,
        GameCommandReceiptWrite {
            save_id: scope.save_id,
            command_id: identity.command_id,
            command_kind: identity.command_kind,
            payload_sha256: identity.payload_sha256,
            market_world_id: scope.market_world_id,
            committed_cursor,
            result: receipt,
            ledger_transaction_id,
        },
    )
    .await?;
    let result_json =
        serde_json::to_string(receipt).context("insurance command result serialization failed")?;
    let insert = sqlx::query(
        "INSERT INTO insurance_command_receipt
             (save_id, run_revision, insurance_component_version_id,
              command_id, command_kind, payload_sha256, contract_id, claim_id,
              ledger_transaction_id, result_json, committed_state_revision,
              committed_game_day)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.insurance_component_version_id)
    .bind(identity.command_id.as_str())
    .bind(identity.command_kind)
    .bind(identity.payload_sha256)
    .bind(contract_id)
    .bind(claim_id)
    .bind(ledger_transaction_id)
    .bind(result_json)
    .bind(committed_cursor.state_revision)
    .bind(committed_cursor.game_day)
    .execute(&mut **tx)
    .await?;
    ensure!(
        insert.rows_affected() == 1,
        "insurance command receipt was not inserted"
    );
    Ok(())
}

async fn cancel_charges_and_settlements(
    tx: &mut Transaction<'_, MySql>,
    scope: &InsuranceScopeRow,
    contract_id: u64,
    charges: &[ChargeLockRow],
    terminal_game_day: u32,
    terminal_reason: &str,
) -> Result<()> {
    ensure!(matches!(
        terminal_reason,
        "contractLapsed" | "playerCancellation" | "termEnded" | "newRun"
    ));
    for charge in charges {
        ensure!(
            charge.status == "scheduled"
                && charge.charge_no > 1
                && charge.amount_krw > 0
                && charge.scheduled_settlement_id.is_some(),
            "future insurance premium charge is invalid"
        );
        let settlement_id = charge
            .scheduled_settlement_id
            .context("future insurance premium has no settlement")?;
        let update = sqlx::query(
            "UPDATE insurance_premium_charge
             SET status = 'cancelled', terminal_game_day = ?, terminal_reason = ?
             WHERE id = ? AND save_id = ? AND run_revision = ?
               AND contract_id = ? AND status = 'scheduled'",
        )
        .bind(terminal_game_day)
        .bind(terminal_reason)
        .bind(charge.id)
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(contract_id)
        .execute(&mut **tx)
        .await?;
        ensure!(update.rows_affected() == 1);
        let settlement_update = sqlx::query(
            "UPDATE scheduled_settlement
             SET status = 'cancelled', cancellation_reason = ?
             WHERE id = ? AND save_id = ? AND run_revision = ?
               AND status = 'pending' AND kind = 'insurancePremium'",
        )
        .bind(terminal_reason)
        .bind(settlement_id)
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .execute(&mut **tx)
        .await?;
        ensure!(settlement_update.rows_affected() == 1);
    }
    Ok(())
}

async fn apply_contract_aggregates(
    tx: &mut Transaction<'_, MySql>,
    scope: &InsuranceScopeRow,
    aggregates: &[InsuranceClaimContractAggregatePlan],
) -> Result<()> {
    ensure!(
        !aggregates.is_empty() && aggregates.len() <= INSURANCE_MAX_CLAIM_CONTRACTS,
        "insurance claim aggregate cardinality is invalid"
    );
    for aggregate in aggregates {
        let update = sqlx::query(
            "UPDATE insurance_contract
             SET paid_term_krw = ?, reserved_term_krw = ?
             WHERE id = ? AND save_id = ? AND run_revision = ?
               AND paid_term_krw = ? AND reserved_term_krw = ?",
        )
        .bind(aggregate.paid_after_krw)
        .bind(aggregate.reserved_after_krw)
        .bind(aggregate.contract_id.get())
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(aggregate.paid_before_krw)
        .bind(aggregate.reserved_before_krw)
        .execute(&mut **tx)
        .await?;
        ensure!(
            update.rows_affected() == 1,
            "insurance claim aggregate changed under lock"
        );
    }
    Ok(())
}

fn enroll_fingerprint(command: &EnrollInsuranceContractCommand) -> String {
    hex_sha256(
        format!(
            "lifeledger.life.enrollInsurance.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\nproductVersionId={}",
            command.cursor.expected_run_revision,
            command.cursor.expected_state_revision,
            command.cursor.expected_game_day,
            command.product_version_id,
        )
        .as_bytes(),
    )
}

fn cancel_fingerprint(command: &CancelInsuranceContractCommand) -> String {
    hex_sha256(
        format!(
            "lifeledger.life.cancelInsurance.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\ncontractId={}",
            command.cursor.expected_run_revision,
            command.cursor.expected_state_revision,
            command.cursor.expected_game_day,
            command.contract_id,
        )
        .as_bytes(),
    )
}

fn claim_fingerprint(command: &FileInsuranceClaimCommand) -> String {
    hex_sha256(
        format!(
            "lifeledger.life.fileInsuranceClaim.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\nclaimId={}",
            command.cursor.expected_run_revision,
            command.cursor.expected_state_revision,
            command.cursor.expected_game_day,
            command.claim_id,
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

fn claim_status_db(status: InsuranceClaimStatus) -> Result<&'static str> {
    match status {
        InsuranceClaimStatus::Candidate => Ok("candidate"),
        InsuranceClaimStatus::NotApplicable => Ok("notApplicable"),
        InsuranceClaimStatus::NotCovered => Ok("notCovered"),
        InsuranceClaimStatus::Ready => Ok("ready"),
        InsuranceClaimStatus::Paid => Ok("paid"),
        InsuranceClaimStatus::Expired => Ok("expired"),
    }
}
