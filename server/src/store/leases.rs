//! Current-run tenant leases, monthly-rent settlement, and atomic moves (§5.6).

use std::str::FromStr;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{MySql, MySqlPool, Transaction};
use time::{Date, Month};

use super::housing::{is_retryable_database_error, prepare_current_housing_catalogs};
use super::loans::{
    LeaseDepositLoanExecutionPreparation, LeaseMovePayoffPreparation,
    apply_lease_move_payoff_in_tx, calculate_debt_projection_in_tx,
    mark_lease_move_payoff_applied_in_tx, originate_lease_deposit_loan_in_tx,
    prepare_lease_deposit_loan_execution_in_tx, prepare_lease_move_payoff_in_tx,
    validate_debt_projection_in_tx,
};
use super::mysql::{
    CommandIdentitySpec, CommandIdentityState, GameCommandReceiptWrite, inspect_command_identity,
    read_state, write_command_identity, write_game_command_receipt,
};
use super::types::{
    ActiveHousingLeaseState, ActiveLeaseTermState, DepositLoanExecutionReceipt, GameCommandCursor,
    HousingLeaseCurrentState, HousingLeaseMoveReceipt, HousingMovingCostState,
    LeaseArrearPaymentReceipt, LeaseArrearState, LeaseLifecycleTermsState, LeaseRenewalNoticeState,
    LeaseTerminationReviewState, LeaseTerminationReviewStatusState, LifeFailureCode,
    LifeStoreResult, MonthlyRentTerminationReviewTermsState, MonthlyRentTermsState,
    PayLeaseArrearCommand, StartHousingLeaseCommand,
};
use crate::finance::{
    FinanceRules, LedgerAccountCode, LedgerPosting, LedgerSource, LedgerSourceKind,
    LedgerTransaction, LedgerTransactionDraft, ResourceId, RunId, RunPolicyContext,
};
use crate::life::{
    HousingLeaseArrearRepaymentRule, HousingLeaseCapability, HousingLeaseOfferKind,
    HousingLeaseRenewalRule, HousingLeaseRole, HousingLeaseTerminationReviewRule,
    HousingRentChargeRule, LeaseArrearPaymentInput, LeaseError, LeaseMoveFundingInput,
    LeaseMoveFundingPlan, LeaseMoveLivingCostAction, LeaseMovePostingLease, LeaseMovePostingLoan,
    LeaseRentPostingOwner, LeaseTermPlan, LeaseTermPlanInput, LeaseTerminationReviewDecision,
    LeaseTerminationReviewInput, LifeRegionKey, MonthlyRentSettlementInput, PropertyType,
    RealEstateRules, YearMonth, create_lease_rules,
};

const COMMAND_KIND_START_LEASE: &str = "startLease";
const COMMAND_KIND_PAY_LEASE_ARREAR: &str = "payLeaseArrear";
const LEASE_RENT_PAYLOAD_VERSION: u8 = 1;
const MAX_ACTIVE_LEASE_ARREARS: usize = 20;
const MAX_TRANSACTION_ATTEMPTS: usize = 3;

#[derive(Debug, sqlx::FromRow)]
struct LeaseReadScopeRow {
    save_id: u64,
    run_revision: u32,
    game_day: u32,
    real_estate_model_version_id: u64,
    model_availability: String,
    model_sealed: bool,
    has_character: bool,
    household_id: Option<u64>,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct LockedLeaseSaveRow {
    pub id: u64,
    pub market_world_id: u64,
    pub policy_set_id: u64,
    pub run_revision: u32,
    pub state_revision: u64,
    pub game_day: u32,
    pub cash_krw: i64,
    pub debt_krw: i64,
    pub has_character: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct LeaseModelScopeRow {
    real_estate_model_version_id: u64,
    model_availability: String,
    model_sealed: bool,
    market_date: Date,
}

#[derive(Debug, sqlx::FromRow)]
struct LeaseProfileRow {
    offer_kind: String,
    renewal_rule: String,
    rent_charge_rule: Option<String>,
    arrear_repayment_rule: Option<String>,
    term_months: Option<u16>,
    renewal_notice_lead_days: Option<u16>,
    termination_review_rule: Option<String>,
    termination_review_after_days: Option<u16>,
}

#[derive(Debug, sqlx::FromRow)]
struct MovingCostRow {
    region_key: String,
    region_order: u8,
    moving_cost_krw: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(super) struct ResidenceProjectionRow {
    pub id: u64,
    pub household_id: u64,
    pub region_key: String,
    pub tenure_type: String,
    pub effective_from_game_day: u32,
    pub effective_to_game_day: Option<u32>,
    pub lease_contract_id: Option<u64>,
    pub property_holding_id: Option<u64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(super) struct LeaseProjectionRow {
    pub id: u64,
    household_id: u64,
    real_estate_model_version_id: u64,
    property_listing_id: u64,
    role: String,
    region_key: String,
    property_type: String,
    exclusive_area_square_meters: u16,
    offer_kind: String,
    pub deposit_krw: i64,
    monthly_rent_krw: Option<i64>,
    renewal_rule: String,
    rent_charge_rule: Option<String>,
    arrear_repayment_rule: Option<String>,
    term_months: Option<u16>,
    renewal_notice_lead_days: Option<u16>,
    termination_review_rule: Option<String>,
    termination_review_after_days: Option<u16>,
    effective_from_game_day: u32,
    effective_to_game_day: Option<u32>,
    next_rent_due_game_day: Option<u32>,
    deposit_loan_id: Option<u64>,
    listing_market_world_id: u64,
    listing_model_version_id: u64,
    listing_region_key: String,
    listing_property_type: String,
    listing_exclusive_area_square_meters: u16,
    listing_deposit_krw: Option<i64>,
    listing_monthly_rent_krw: Option<i64>,
    save_market_world_id: u64,
}

#[derive(Debug, sqlx::FromRow)]
struct ActiveLeaseTermRow {
    id: u64,
    term_no: u32,
    effective_from_game_day: u32,
    effective_to_game_day: u32,
}

#[derive(Debug, sqlx::FromRow)]
struct LeaseTermActionRow {
    action_kind: String,
    phase_rank: u16,
    due_game_day: u32,
    source_kind: String,
    source_id: u64,
    occurrence: u32,
    status: String,
    applied_game_day: Option<u32>,
}

#[derive(Debug, sqlx::FromRow)]
struct LeaseLifecycleActionEnvelopeRow {
    id: u64,
    lease_contract_id: u64,
    lease_contract_term_id: Option<u64>,
    lease_arrear_id: Option<u64>,
    action_kind: String,
    payload_version: u8,
    phase_rank: u16,
    due_game_day: u32,
    source_kind: String,
    source_id: u64,
    occurrence: u32,
    status: String,
    term_contract_id: Option<u64>,
    term_no: Option<u32>,
    term_effective_from_game_day: Option<u32>,
    term_effective_to_game_day: Option<u32>,
    term_status: Option<String>,
    arrear_contract_id: Option<u64>,
    arrear_created_game_day: Option<u32>,
    arrear_status: Option<String>,
    contract_renewal_rule: String,
    contract_term_months: Option<u16>,
    contract_renewal_notice_lead_days: Option<u16>,
    contract_termination_review_rule: Option<String>,
    contract_termination_review_after_days: Option<u16>,
    contract_household_id: u64,
    contract_effective_from_game_day: u32,
    contract_effective_to_game_day: Option<u32>,
}

#[derive(Debug, sqlx::FromRow)]
struct OpenLeaseTerminationReviewRow {
    opened_game_day: u32,
    trigger_lease_arrear_id: u64,
    active_lease_arrear_krw: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct LockedListingOfferRow {
    id: u64,
    market_world_id: u64,
    real_estate_model_version_id: u64,
    year_month: Date,
    region_key: String,
    property_type: String,
    exclusive_area_square_meters: u16,
    available_from_game_day: u32,
    available_to_game_day: u32,
    offer_kind: String,
    price_krw: Option<i64>,
    deposit_krw: Option<i64>,
    monthly_rent_krw: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
struct StoredLeaseReceiptRow {
    command_kind: String,
    payload_sha256: String,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
    result_json: String,
    ledger_transaction_id: Option<u64>,
}

#[derive(Debug, sqlx::FromRow)]
struct LeaseRentSettlementEnvelopeRow {
    due_game_day: u32,
    payload_json: String,
    source_id: String,
    occurrence: u32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LeaseRentSettlementPayload {
    version: u8,
    lease_contract_id: String,
    rent_charge_id: String,
    charge_no: u32,
}

#[derive(Debug, sqlx::FromRow)]
struct LockedRentChargeRow {
    id: u64,
    lease_contract_id: u64,
    charge_no: u32,
    due_year_month: Date,
    due_game_day: u32,
    amount_krw: i64,
    paid_krw: Option<i64>,
    arrear_krw: Option<i64>,
    status: String,
}

#[derive(Debug, sqlx::FromRow)]
struct LockedMonthlyRentContractRow {
    id: u64,
    household_id: u64,
    monthly_rent_krw: Option<i64>,
    rent_charge_rule: Option<String>,
    arrear_repayment_rule: Option<String>,
    renewal_rule: String,
    termination_review_rule: Option<String>,
    termination_review_after_days: Option<u16>,
    effective_to_game_day: Option<u32>,
}

pub(super) struct LeaseRentSettlementContext<'a> {
    pub finance_rules: &'a dyn FinanceRules,
    pub save_id: u64,
    pub run_revision: u32,
    pub policy_set_id: u64,
    pub game_day: u32,
    pub market_date: Date,
    pub settlement_id: u64,
}

struct LeaseRentChargeDraft {
    save_id: u64,
    run_revision: u32,
    lease_contract_id: u64,
    charge_no: u32,
    due_year_month: Date,
    due_game_day: u32,
    amount_krw: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct ActiveLeaseArrearRow {
    id: u64,
    lease_contract_id: u64,
    lease_rent_charge_id: u64,
    due_year_month: Date,
    original_krw: i64,
    paid_krw: i64,
    remaining_krw: i64,
    created_game_day: u32,
}

#[derive(Debug, sqlx::FromRow)]
struct LockedLeaseArrearRow {
    id: u64,
    household_id: u64,
    lease_contract_id: u64,
    lease_rent_charge_id: u64,
    original_krw: i64,
    paid_krw: i64,
    remaining_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeasePostingReference {
    None,
    Contract(u64),
    LoanContract(u64),
    RentCharge(u64),
    Arrear(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LeaseMoveLedgerOwners {
    ended_lease_id: Option<u64>,
    started_lease_id: u64,
    repaid_loan_id: Option<u64>,
    originated_loan_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LeaseArrearWindow {
    pub items: Vec<LeaseArrearState>,
    pub has_more: bool,
    pub total_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct LivingCostPinRow {
    id: u64,
    household_id: u64,
    residence_id: u64,
    year_month: Date,
    region_key: String,
    tenure_type: String,
    household_fingerprint_sha256: String,
    proration_scale: u32,
    proration_units: u32,
    days_in_month: u8,
    status: String,
}

struct LeaseCatalogState {
    capability: HousingLeaseCapability,
    renewal_rule: Option<HousingLeaseRenewalRule>,
    lease_lifecycle_terms: Option<LeaseLifecycleTermsState>,
    monthly_rent_terms: Option<MonthlyRentTermsState>,
    moving_costs: Vec<HousingMovingCostState>,
}

pub(super) async fn read_current_housing_lease(
    pool: &MySqlPool,
    user_id: u64,
) -> Result<Option<HousingLeaseCurrentState>> {
    let mut tx = pool.begin().await?;
    let scope: Option<LeaseReadScopeRow> = sqlx::query_as(
        r#"
        SELECT save.id AS save_id,
               save.run_revision,
               save.game_day,
               bundle.real_estate_model_version_id,
               model.availability AS model_availability,
               (model.sealed_at IS NOT NULL) AS model_sealed,
               EXISTS(
                   SELECT 1 FROM `character`
                   WHERE `character`.save_id = save.id
               ) AS has_character,
               household.id AS household_id
        FROM save
        INNER JOIN run_rule_bundle AS bundle
            ON bundle.save_id = save.id
           AND bundle.run_revision = save.run_revision
           AND bundle.market_world_id = save.market_world_id
        INNER JOIN real_estate_model_version AS model
            ON model.id = bundle.real_estate_model_version_id
        LEFT JOIN household
            ON household.save_id = save.id
           AND household.run_revision = save.run_revision
        WHERE save.user_id = ?
        "#,
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(scope) = scope else {
        tx.commit().await?;
        return Ok(None);
    };
    if !scope.has_character {
        tx.commit().await?;
        return Ok(None);
    }
    ensure!(
        scope.household_id.is_some(),
        "current housing lease scope has no household"
    );
    let catalog = read_lease_catalog_in_tx(
        &mut tx,
        scope.real_estate_model_version_id,
        &scope.model_availability,
        scope.model_sealed,
    )
    .await?;
    let (tenant_lease_deposit_krw, active_lease) = read_active_housing_lease_snapshot_in_tx(
        &mut tx,
        scope.save_id,
        scope.run_revision,
        scope.game_day,
    )
    .await?;
    let arrears =
        read_active_lease_arrears_in_tx(&mut tx, scope.save_id, scope.run_revision).await?;
    validate_debt_projection_in_tx(&mut tx, scope.save_id, scope.run_revision).await?;
    if catalog.capability == HousingLeaseCapability::Unavailable {
        ensure!(
            tenant_lease_deposit_krw == 0 && active_lease.is_none(),
            "unavailable lease model has an active tenant lease"
        );
    }
    let state = HousingLeaseCurrentState {
        lease_capability: catalog.capability,
        renewal_rule: catalog.renewal_rule,
        lease_lifecycle_terms: catalog.lease_lifecycle_terms,
        moving_costs: catalog.moving_costs,
        tenant_lease_deposit_krw,
        active_lease,
        monthly_rent_terms: catalog.monthly_rent_terms,
        active_arrears: arrears.items,
        has_more_active_arrears: arrears.has_more,
        total_lease_arrear_krw: arrears.total_krw,
    };
    tx.commit().await?;
    Ok(Some(state))
}

pub(super) async fn start_housing_lease_command(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    real_estate_rules: &dyn RealEstateRules,
    user_id: u64,
    command: &StartHousingLeaseCommand,
) -> Result<LifeStoreResult<HousingLeaseMoveReceipt>> {
    if let Err(error) = prepare_current_housing_catalogs(pool, real_estate_rules, user_id).await {
        if is_retryable_database_error(&error) {
            return Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy));
        }
        return Err(error);
    }

    let fingerprint = start_lease_fingerprint(command);
    for _ in 0..MAX_TRANSACTION_ATTEMPTS {
        match start_housing_lease_once(pool, finance_rules, user_id, command, &fingerprint).await {
            Ok(result) => return Ok(result),
            Err(error) if is_retryable_database_error(&error) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy))
}

pub(super) async fn pay_lease_arrear_command(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    user_id: u64,
    command: &PayLeaseArrearCommand,
) -> Result<LifeStoreResult<LeaseArrearPaymentReceipt>> {
    let fingerprint = pay_lease_arrear_fingerprint(command);
    for _ in 0..MAX_TRANSACTION_ATTEMPTS {
        match pay_lease_arrear_once(pool, finance_rules, user_id, command, &fingerprint).await {
            Ok(result) => return Ok(result),
            Err(error) if is_retryable_database_error(&error) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy))
}

pub(super) async fn close_lease_lifecycle_for_new_run_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
) -> Result<()> {
    sqlx::query(
        "UPDATE lease_rent_charge
         SET status = 'cancelled'
         WHERE save_id = ? AND run_revision = ? AND status = 'pending'",
    )
    .bind(save_id)
    .bind(run_revision)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE lease_lifecycle_action
         SET status = 'cancelled', cancelled_game_day = ?,
             cancellation_reason = 'newRun'
         WHERE save_id = ? AND run_revision = ? AND status = 'pending'",
    )
    .bind(game_day)
    .bind(save_id)
    .bind(run_revision)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE lease_termination_review
         SET status = 'resolved', resolved_game_day = ?,
             resolution_reason = 'newRun'
         WHERE save_id = ? AND run_revision = ? AND status = 'open'",
    )
    .bind(game_day)
    .bind(save_id)
    .bind(run_revision)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE lease_contract_term
         SET status = 'terminated', closed_game_day = ?,
             termination_reason = 'newRun'
         WHERE save_id = ? AND run_revision = ? AND status = 'active'",
    )
    .bind(game_day)
    .bind(save_id)
    .bind(run_revision)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn pay_lease_arrear_once(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    user_id: u64,
    command: &PayLeaseArrearCommand,
    fingerprint: &str,
) -> Result<LifeStoreResult<LeaseArrearPaymentReceipt>> {
    let mut tx = pool.begin().await?;
    let Some(current) = lock_lease_save(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_PAY_LEASE_ARREAR,
        payload_sha256: fingerprint,
        cursor: command.cursor,
    };
    match inspect_command_identity(&mut tx, current.id, &identity).await? {
        CommandIdentityState::Conflict => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(
                LifeFailureCode::IdempotencyConflict,
            ));
        }
        CommandIdentityState::Matching => {
            let row =
                read_stored_lease_receipt(&mut tx, current.id, command.command_id.as_str()).await?;
            if row.run_revision != current.run_revision {
                tx.commit().await?;
                return Ok(LifeStoreResult::Rejected(
                    LifeFailureCode::IdempotencyConflict,
                ));
            }
            let mut receipt: LeaseArrearPaymentReceipt = serde_json::from_str(&row.result_json)
                .context("stored lease-arrear payment receipt is invalid")?;
            let expected_state_revision = command
                .cursor
                .expected_state_revision
                .checked_add(1)
                .context("stored lease-arrear state revision overflowed")?;
            ensure!(
                row.command_kind == COMMAND_KIND_PAY_LEASE_ARREAR
                    && row.payload_sha256 == fingerprint
                    && row.run_revision == command.cursor.expected_run_revision
                    && row.state_revision == expected_state_revision
                    && row.game_day == command.cursor.expected_game_day
                    && row.ledger_transaction_id.is_some()
                    && receipt.command_id == command.command_id
                    && receipt.arrear_id == command.arrear_id
                    && receipt.paid_krw == command.amount_krw
                    && !receipt.replayed,
                "stored lease-arrear payment receipt disagrees with its command"
            );
            let payment_exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(
                     SELECT 1 FROM lease_arrear_payment
                     WHERE id = ? AND save_id = ? AND run_revision = ?
                       AND lease_arrear_id = ? AND command_id = ?
                       AND amount_krw = ? AND status = 'applied'
                       AND ledger_transaction_id = ?
                 )",
            )
            .bind(receipt.payment_id.get())
            .bind(current.id)
            .bind(current.run_revision)
            .bind(receipt.arrear_id.get())
            .bind(command.command_id.as_str())
            .bind(receipt.paid_krw)
            .bind(row.ledger_transaction_id)
            .fetch_one(&mut *tx)
            .await?;
            ensure!(
                payment_exists,
                "stored lease-arrear payment lost its current-run resource"
            );
            receipt.replayed = true;
            let save = read_state(&mut tx, current.id).await?;
            tx.commit().await?;
            return Ok(LifeStoreResult::Applied {
                receipt,
                save: Box::new(save),
            });
        }
        CommandIdentityState::Missing => {}
    }
    if !current.has_character {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    }
    if !has_cursor(&current, command.cursor) {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy));
    }
    if command.amount_krw <= 0 {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::InvalidCommand));
    }
    let household_id = lock_current_household(&mut tx, current.id, current.run_revision).await?;
    validate_debt_projection_in_tx(&mut tx, current.id, current.run_revision).await?;
    let arrear_contract_id: Option<u64> = sqlx::query_scalar(
        "SELECT lease_contract_id
         FROM lease_arrear
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'active'",
    )
    .bind(command.arrear_id.get())
    .bind(current.id)
    .bind(current.run_revision)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(arrear_contract_id) = arrear_contract_id else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
    };
    let arrear_contract: LockedMonthlyRentContractRow = sqlx::query_as(
        "SELECT id, household_id, monthly_rent_krw, rent_charge_rule,
                arrear_repayment_rule, renewal_rule,
                termination_review_rule, termination_review_after_days,
                effective_to_game_day
         FROM lease_contract
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND role = 'tenant' AND offer_kind = 'monthlyRent'
         FOR UPDATE",
    )
    .bind(arrear_contract_id)
    .bind(current.id)
    .bind(current.run_revision)
    .fetch_one(&mut *tx)
    .await
    .context("lease-arrear contract is missing")?;
    let arrear: Option<LockedLeaseArrearRow> = sqlx::query_as(
        "SELECT id, household_id, lease_contract_id, lease_rent_charge_id,
                original_krw, paid_krw, remaining_krw
         FROM lease_arrear
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND status = 'active'
         FOR UPDATE",
    )
    .bind(command.arrear_id.get())
    .bind(current.id)
    .bind(current.run_revision)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(arrear) = arrear else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
    };
    ensure!(
        arrear.household_id == household_id
            && arrear.lease_contract_id == arrear_contract.id
            && arrear.lease_rent_charge_id > 0
            && arrear.original_krw > 0
            && arrear.paid_krw >= 0
            && arrear.remaining_krw > 0
            && arrear.paid_krw.checked_add(arrear.remaining_krw) == Some(arrear.original_krw),
        "active lease arrear has an invalid owner or balance"
    );
    if command.amount_krw > arrear.remaining_krw {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
    }
    if command.amount_krw > current.cash_krw {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::InsufficientWalletCash,
        ));
    }
    let plan = create_lease_rules()
        .plan_lease_arrear_payment(LeaseArrearPaymentInput {
            wallet_cash_krw: current.cash_krw,
            outstanding_krw: arrear.remaining_krw,
            amount_krw: command.amount_krw,
        })
        .context("lease-arrear payment planning failed")?;
    write_command_identity(&mut tx, current.id, &identity).await?;
    let payment_no_raw: u64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(MAX(payment_no), 0) + 1 AS UNSIGNED)
         FROM lease_arrear_payment WHERE lease_arrear_id = ?",
    )
    .bind(arrear.id)
    .fetch_one(&mut *tx)
    .await?;
    let payment_no =
        u32::try_from(payment_no_raw).context("lease-arrear payment count is out of range")?;
    let payment_id = sqlx::query(
        "INSERT INTO lease_arrear_payment
             (save_id, run_revision, lease_arrear_id, command_id,
              payment_no, amount_krw, game_day, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'prepared')",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(arrear.id)
    .bind(command.command_id.as_str())
    .bind(payment_no)
    .bind(plan.paid_krw)
    .bind(current.game_day)
    .execute(&mut *tx)
    .await?
    .last_insert_id();
    ensure!(
        payment_id > 0,
        "lease-arrear payment has no durable identity"
    );
    let ledger = finance_rules.create_ledger_transaction(LedgerTransactionDraft {
        policy: RunPolicyContext {
            run: RunId {
                save_id: ResourceId::from_u64(current.id),
                run_revision: current.run_revision,
            },
            policy_set_id: ResourceId::from_u64(current.policy_set_id),
        },
        source: LedgerSource {
            kind: LedgerSourceKind::LeaseArrearPayment,
            source_id: payment_id.to_string(),
        },
        game_day: current.game_day,
        description: "월세 연체 상환".to_owned(),
        postings: plan
            .postings
            .iter()
            .map(|posting| LedgerPosting {
                account_code: posting.account_code,
                financial_account_id: None,
                amount_krw: posting.amount_krw,
            })
            .collect(),
    })?;
    let references = plan
        .postings
        .iter()
        .map(|posting| match posting.owner {
            LeaseRentPostingOwner::None => LeasePostingReference::None,
            LeaseRentPostingOwner::Arrear => LeasePostingReference::Arrear(arrear.id),
            LeaseRentPostingOwner::RentCharge => {
                LeasePostingReference::RentCharge(arrear.lease_rent_charge_id)
            }
        })
        .collect::<Vec<_>>();
    let ledger_transaction_id =
        write_lease_ledger_transaction(&mut tx, &ledger, &references).await?;
    let payment_updated = sqlx::query(
        "UPDATE lease_arrear_payment
         SET status = 'applied', ledger_transaction_id = ?
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'prepared'",
    )
    .bind(ledger_transaction_id)
    .bind(payment_id)
    .bind(current.id)
    .bind(current.run_revision)
    .execute(&mut *tx)
    .await?;
    ensure!(
        payment_updated.rows_affected() == 1,
        "lease-arrear payment was not applied"
    );
    let arrear_updated = sqlx::query(
        "UPDATE lease_arrear
         SET paid_krw = paid_krw + ?,
             status = IF(? = 0, 'paid', 'active'),
             closed_game_day = IF(? = 0, ?, NULL)
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'active'
           AND remaining_krw = ?",
    )
    .bind(plan.paid_krw)
    .bind(plan.remaining_krw)
    .bind(plan.remaining_krw)
    .bind(current.game_day)
    .bind(arrear.id)
    .bind(current.id)
    .bind(current.run_revision)
    .bind(arrear.remaining_krw)
    .execute(&mut *tx)
    .await?;
    ensure!(
        arrear_updated.rows_affected() == 1,
        "lease arrear changed during payment"
    );
    reconcile_lease_termination_review_after_payment(
        &mut tx,
        current.id,
        current.run_revision,
        current.game_day,
        &arrear_contract,
        arrear.id,
        plan.remaining_krw == 0,
    )
    .await?;
    let projection =
        calculate_debt_projection_in_tx(&mut tx, current.id, current.run_revision).await?;
    let committed_state_revision = current
        .state_revision
        .checked_add(1)
        .context("lease-arrear state revision overflowed")?;
    update_save_after_lease_arrear_payment(
        &mut tx,
        &current,
        committed_state_revision,
        plan.wallet_after_krw,
        projection.total_krw,
    )
    .await?;
    validate_debt_projection_in_tx(&mut tx, current.id, current.run_revision).await?;
    let receipt = LeaseArrearPaymentReceipt {
        command_id: command.command_id.clone(),
        arrear_id: command.arrear_id,
        payment_id: ResourceId::from_u64(payment_id),
        paid_krw: plan.paid_krw,
        remaining_krw: plan.remaining_krw,
        replayed: false,
    };
    write_game_command_receipt(
        &mut tx,
        GameCommandReceiptWrite {
            save_id: current.id,
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_PAY_LEASE_ARREAR,
            payload_sha256: fingerprint,
            market_world_id: current.market_world_id,
            committed_cursor: GameCommandCursor {
                run_revision: current.run_revision,
                state_revision: committed_state_revision,
                game_day: current.game_day,
            },
            result: &receipt,
            ledger_transaction_id: Some(ledger_transaction_id),
        },
    )
    .await?;
    let save = read_state(&mut tx, current.id).await?;
    tx.commit().await?;
    Ok(LifeStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

async fn start_housing_lease_once(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    user_id: u64,
    command: &StartHousingLeaseCommand,
    fingerprint: &str,
) -> Result<LifeStoreResult<HousingLeaseMoveReceipt>> {
    let mut tx = pool.begin().await?;
    let Some(current) = lock_lease_save(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_START_LEASE,
        payload_sha256: fingerprint,
        cursor: command.cursor,
    };
    match inspect_command_identity(&mut tx, current.id, &identity).await? {
        CommandIdentityState::Conflict => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(
                LifeFailureCode::IdempotencyConflict,
            ));
        }
        CommandIdentityState::Matching => {
            let row =
                read_stored_lease_receipt(&mut tx, current.id, command.command_id.as_str()).await?;
            if row.run_revision != current.run_revision {
                tx.commit().await?;
                return Ok(LifeStoreResult::Rejected(
                    LifeFailureCode::IdempotencyConflict,
                ));
            }
            let mut receipt: HousingLeaseMoveReceipt = serde_json::from_str(&row.result_json)
                .context("stored housing lease receipt is invalid")?;
            validate_replayed_receipt(&mut tx, &current, command, fingerprint, &row, &receipt)
                .await?;
            receipt.replayed = true;
            let save = read_state(&mut tx, current.id).await?;
            tx.commit().await?;
            return Ok(LifeStoreResult::Applied {
                receipt,
                save: Box::new(save),
            });
        }
        CommandIdentityState::Missing => {}
    }
    if !current.has_character {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    }
    if !has_cursor(&current, command.cursor) {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
    }

    let household_id = lock_current_household(&mut tx, current.id, current.run_revision).await?;
    if tenant_lease_boundary_conflict_in_tx(
        &mut tx,
        current.id,
        current.run_revision,
        household_id,
        current.game_day,
    )
    .await?
    {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
    }
    let model = read_current_model_scope(&mut tx, &current).await?;
    let catalog = read_lease_catalog_in_tx(
        &mut tx,
        model.real_estate_model_version_id,
        &model.model_availability,
        model.model_sealed,
    )
    .await?;
    let offer_supported = matches!(
        (catalog.capability, command.offer_kind),
        (
            HousingLeaseCapability::CashJeonse,
            HousingLeaseOfferKind::Jeonse
        ) | (
            HousingLeaseCapability::CashJeonseAndMonthlyRent,
            HousingLeaseOfferKind::Jeonse | HousingLeaseOfferKind::MonthlyRent,
        )
    );
    if !offer_supported {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::RateUnavailable));
    }
    let renewal_rule = catalog
        .renewal_rule
        .context("lease-capable catalog has no renewal rule")?;

    let residence = lock_current_residence(
        &mut tx,
        current.id,
        current.run_revision,
        household_id,
        current.game_day,
    )
    .await?;
    if residence.effective_from_game_day >= current.game_day {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
    }
    let existing_lease = lock_existing_lease(
        &mut tx,
        &current,
        household_id,
        &residence,
        model.real_estate_model_version_id,
    )
    .await?;
    if existing_lease
        .as_ref()
        .is_some_and(|lease| lease.property_listing_id == command.listing_id.get())
    {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
    }

    let prepared_payoff = match prepare_lease_move_payoff_in_tx(
        &mut tx,
        current.id,
        current.run_revision,
        household_id,
        existing_lease.as_ref().map(|lease| lease.id),
        existing_lease.as_ref().map_or(0, |lease| lease.deposit_krw),
    )
    .await?
    {
        LeaseMovePayoffPreparation::None => None,
        LeaseMovePayoffPreparation::Prepared(prepared) => Some(prepared),
        LeaseMovePayoffPreparation::Rejected(code) => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(code));
        }
    };
    let prepared_execution = match command.loan_quote_id {
        Some(quote_id) => {
            if command.offer_kind != HousingLeaseOfferKind::Jeonse {
                tx.commit().await?;
                return Ok(LifeStoreResult::Rejected(LifeFailureCode::InvalidCommand));
            }
            match prepare_lease_deposit_loan_execution_in_tx(
                &mut tx,
                user_id,
                current.id,
                current.run_revision,
                command.listing_id,
                quote_id,
            )
            .await?
            {
                LeaseDepositLoanExecutionPreparation::Prepared(prepared) => Some(prepared),
                LeaseDepositLoanExecutionPreparation::Rejected(code) => {
                    tx.commit().await?;
                    return Ok(LifeStoreResult::Rejected(code));
                }
            }
        }
        None => None,
    };
    if let Some(execution) = &prepared_execution {
        let payoff_loan_id = prepared_payoff.as_ref().map(|payoff| payoff.loan_id());
        if execution.replaced_loan_id() != payoff_loan_id {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
        }
    }

    let listing_rows = lock_listing(&mut tx, command.listing_id).await?;
    let Some(listing) = select_lease_listing(&listing_rows, &current, &model, command)? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
    };
    let region_key = LifeRegionKey::from_str(&listing.region_key)
        .context("lease listing has an unknown region")?;
    let moving_cost_krw = catalog
        .moving_costs
        .iter()
        .find(|cost| cost.region_key == region_key)
        .map(|cost| cost.moving_cost_krw)
        .context("lease listing region has no moving cost")?;
    let new_deposit_krw = listing
        .deposit_krw
        .filter(|amount| *amount > 0)
        .context("lease listing has an invalid deposit")?;
    let existing_deposit_krw = existing_lease.as_ref().map_or(0, |lease| lease.deposit_krw);
    let repaid_loan_principal_krw = prepared_payoff
        .as_ref()
        .map_or(0, |payoff| payoff.principal_krw());
    let new_loan_principal_krw = prepared_execution
        .as_ref()
        .map_or(0, |execution| execution.principal_krw());
    let plan = match create_lease_rules().plan_lease_move_funding(LeaseMoveFundingInput {
        wallet_cash_krw: current.cash_krw,
        existing_deposit_krw,
        repaid_loan_principal_krw,
        new_deposit_krw,
        new_loan_principal_krw,
        moving_cost_krw,
    }) {
        Ok(plan) => plan,
        Err(LeaseError::InsufficientWalletCash) => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(
                LifeFailureCode::InsufficientWalletCash,
            ));
        }
        Err(error) => return Err(error).context("lease move funding planning failed"),
    };
    ensure!(
        plan.living_cost_action == LeaseMoveLivingCostAction::PreserveCurrentMonth,
        "cash-jeonse planner attempted to recalculate the current living-cost month"
    );
    let living_cost_before =
        read_current_living_cost_pin(&mut tx, current.id, current.run_revision, model.market_date)
            .await?;

    write_command_identity(&mut tx, current.id, &identity).await?;
    let repaid_deposit_loan = match prepared_payoff {
        Some(prepared) => Some(
            apply_lease_move_payoff_in_tx(
                &mut tx,
                current.id,
                current.run_revision,
                current.game_day,
                command.command_id.as_str(),
                *prepared,
            )
            .await?,
        ),
        None => None,
    };
    let ended_lease_id = existing_lease.as_ref().map(|lease| lease.id);
    if let Some(lease) = &existing_lease {
        cancel_future_lease_rent_charge(&mut tx, &current, lease).await?;
        close_existing_lease_lifecycle(&mut tx, &current, lease).await?;
        close_existing_lease(&mut tx, &current, lease).await?;
    }
    close_existing_residence(&mut tx, &current, &residence).await?;
    let lease_id = insert_tenant_lease(
        &mut tx,
        &current,
        household_id,
        listing,
        command,
        new_deposit_krw,
        &catalog,
    )
    .await?;
    let deposit_loan_execution: Option<DepositLoanExecutionReceipt> = match prepared_execution {
        Some(prepared) => Some(
            originate_lease_deposit_loan_in_tx(
                &mut tx,
                command.command_id.as_str(),
                lease_id,
                *prepared,
            )
            .await?,
        ),
        None => None,
    };
    if let Some(lifecycle_terms) = catalog.lease_lifecycle_terms {
        insert_initial_lease_lifecycle(
            &mut tx,
            &current,
            lease_id,
            model.market_date,
            lifecycle_terms,
        )
        .await?;
    }
    let residence_id = insert_lease_residence(
        &mut tx,
        &current,
        household_id,
        lease_id,
        &listing.region_key,
        command.offer_kind,
    )
    .await?;
    if command.offer_kind == HousingLeaseOfferKind::MonthlyRent {
        let monthly_rent_krw = listing
            .monthly_rent_krw
            .filter(|amount| *amount > 0)
            .context("monthly-rent listing has no positive rent")?;
        let due = create_lease_rules()
            .next_monthly_rent_charge(current.game_day, model.market_date)
            .context("first monthly-rent charge date is invalid")?;
        insert_lease_rent_charge_and_settlement(
            &mut tx,
            LeaseRentChargeDraft {
                save_id: current.id,
                run_revision: current.run_revision,
                lease_contract_id: lease_id,
                charge_no: 1,
                due_year_month: year_month_date(due.due_year_month)?,
                due_game_day: due.due_game_day,
                amount_krw: monthly_rent_krw,
            },
        )
        .await?;
    }
    let ledger_transaction_id = write_lease_move_ledger(
        &mut tx,
        finance_rules,
        &current,
        command.command_id.as_str(),
        &plan,
        LeaseMoveLedgerOwners {
            ended_lease_id,
            started_lease_id: lease_id,
            repaid_loan_id: repaid_deposit_loan
                .as_ref()
                .map(|payoff| payoff.loan_id.get()),
            originated_loan_id: deposit_loan_execution
                .as_ref()
                .map(|execution| execution.loan_id.get()),
        },
    )
    .await?;
    if let Some(payoff) = &repaid_deposit_loan {
        mark_lease_move_payoff_applied_in_tx(
            &mut tx,
            current.id,
            current.run_revision,
            payoff,
            ledger_transaction_id,
        )
        .await?;
    }
    let committed_state_revision = current
        .state_revision
        .checked_add(1)
        .context("housing lease state revision overflowed")?;
    update_save_after_lease_move(
        &mut tx,
        &current,
        committed_state_revision,
        plan.wallet_after_krw,
        current
            .debt_krw
            .checked_add(plan.debt_delta_krw)
            .context("housing lease debt projection overflowed")?,
    )
    .await?;
    validate_debt_projection_in_tx(&mut tx, current.id, current.run_revision).await?;

    let living_cost_after =
        read_current_living_cost_pin(&mut tx, current.id, current.run_revision, model.market_date)
            .await?;
    ensure!(
        living_cost_before == living_cost_after,
        "housing lease move changed the current living-cost month"
    );
    let (tenant_lease_deposit_krw, active_lease) = read_active_housing_lease_snapshot_in_tx(
        &mut tx,
        current.id,
        current.run_revision,
        current.game_day,
    )
    .await?;
    ensure!(
        tenant_lease_deposit_krw == plan.tenant_lease_deposit_krw
            && active_lease.as_ref().map(|lease| lease.id.get()) == Some(lease_id)
            && active_lease
                .as_ref()
                .and_then(|lease| lease.deposit_loan_id)
                == deposit_loan_execution
                    .as_ref()
                    .map(|execution| execution.loan_id),
        "housing lease move disagrees with the active lease projection"
    );

    let receipt = HousingLeaseMoveReceipt {
        command_id: command.command_id.clone(),
        lease_id: ResourceId::from_u64(lease_id),
        residence_id: ResourceId::from_u64(residence_id),
        listing_id: command.listing_id,
        offer_kind: command.offer_kind,
        region_key,
        property_type: PropertyType::from_str(&listing.property_type)
            .context("lease listing has an unknown property type")?,
        exclusive_area_square_meters: listing.exclusive_area_square_meters,
        deposit_krw: plan.deposit_krw,
        monthly_rent_krw: listing.monthly_rent_krw,
        returned_deposit_krw: plan.returned_deposit_krw,
        moving_cost_krw: plan.moving_cost_krw,
        wallet_delta_krw: plan.wallet_delta_krw,
        effective_from_game_day: current.game_day,
        ended_lease_id: ended_lease_id.map(ResourceId::from_u64),
        renewal_rule,
        deposit_loan_execution,
        repaid_deposit_loan,
        replayed: false,
    };
    write_game_command_receipt(
        &mut tx,
        GameCommandReceiptWrite {
            save_id: current.id,
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_START_LEASE,
            payload_sha256: fingerprint,
            market_world_id: current.market_world_id,
            committed_cursor: GameCommandCursor {
                run_revision: current.run_revision,
                state_revision: committed_state_revision,
                game_day: current.game_day,
            },
            result: &receipt,
            ledger_transaction_id: Some(ledger_transaction_id),
        },
    )
    .await?;
    let save = read_state(&mut tx, current.id).await?;
    ensure!(
        save.cash_krw == plan.wallet_after_krw
            && save.debt_krw
                == current
                    .debt_krw
                    .checked_add(plan.debt_delta_krw)
                    .context("housing lease saved debt projection overflowed")?
            && save.state_revision == committed_state_revision,
        "housing lease receipt disagrees with the saved projection"
    );
    tx.commit().await?;
    Ok(LifeStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

pub(super) async fn read_active_housing_lease_snapshot_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
) -> Result<(i64, Option<ActiveHousingLeaseState>)> {
    let residences: Vec<ResidenceProjectionRow> = sqlx::query_as(
        "SELECT id, household_id, region_key, tenure_type, effective_from_game_day,
                effective_to_game_day, lease_contract_id, property_holding_id
         FROM residence
         WHERE save_id = ? AND run_revision = ?
           AND effective_from_game_day <= ?
           AND (effective_to_game_day IS NULL OR effective_to_game_day > ?)
         ORDER BY id",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(game_day)
    .bind(game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        residences.len() == 1,
        "lease projection requires exactly one current residence"
    );
    let residence = &residences[0];
    let leases: Vec<LeaseProjectionRow> = sqlx::query_as(
        r#"
        SELECT lease.id,
               lease.household_id,
               lease.real_estate_model_version_id,
               lease.property_listing_id,
               lease.role,
               lease.region_key,
               lease.property_type,
               lease.exclusive_area_square_meters,
               lease.offer_kind,
               lease.deposit_krw,
               lease.monthly_rent_krw,
               lease.renewal_rule,
               lease.rent_charge_rule,
               lease.arrear_repayment_rule,
               lease.term_months,
               lease.renewal_notice_lead_days,
               lease.termination_review_rule,
               lease.termination_review_after_days,
               lease.effective_from_game_day,
               lease.effective_to_game_day,
               (
                   SELECT MIN(charge.due_game_day)
                   FROM lease_rent_charge AS charge
                   WHERE charge.save_id = lease.save_id
                     AND charge.run_revision = lease.run_revision
                     AND charge.lease_contract_id = lease.id
                     AND charge.status = 'pending'
               ) AS next_rent_due_game_day,
               (
                   SELECT loan.id
                   FROM loan_contract AS loan
                   WHERE loan.save_id = lease.save_id
                     AND loan.run_revision = lease.run_revision
                     AND loan.lease_contract_id = lease.id
               ) AS deposit_loan_id,
               listing.market_world_id AS listing_market_world_id,
               listing.real_estate_model_version_id AS listing_model_version_id,
               listing.region_key AS listing_region_key,
               listing.property_type AS listing_property_type,
               listing.exclusive_area_square_meters
                   AS listing_exclusive_area_square_meters,
               offer.deposit_krw AS listing_deposit_krw,
               offer.monthly_rent_krw AS listing_monthly_rent_krw,
               save.market_world_id AS save_market_world_id
        FROM lease_contract AS lease
        INNER JOIN save
            ON save.id = lease.save_id
           AND save.run_revision = lease.run_revision
        INNER JOIN property_listing AS listing
            ON listing.id = lease.property_listing_id
        INNER JOIN property_listing_offer AS offer
            ON offer.property_listing_id = lease.property_listing_id
           AND BINARY offer.offer_kind = BINARY lease.offer_kind
        WHERE lease.save_id = ? AND lease.run_revision = ?
          AND lease.role = 'tenant'
          AND lease.effective_from_game_day <= ?
          AND (lease.effective_to_game_day IS NULL OR lease.effective_to_game_day > ?)
        ORDER BY lease.id
        "#,
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(game_day)
    .bind(game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(leases.len() <= 1, "run has multiple current tenant leases");
    let mut active_lease = leases
        .into_iter()
        .next()
        .map(|lease| map_active_lease(lease, residence))
        .transpose()?;
    if let Some(lease) = &mut active_lease {
        read_active_lease_lifecycle_in_tx(tx, save_id, run_revision, game_day, lease).await?;
    }
    match &active_lease {
        Some(lease) => ensure!(
            residence.lease_contract_id == Some(lease.id.get())
                && residence.property_holding_id.is_none()
                && residence.tenure_type
                    == match lease.offer_kind {
                        HousingLeaseOfferKind::Jeonse => "jeonse",
                        HousingLeaseOfferKind::MonthlyRent => "monthlyRent",
                    }
                && residence.region_key == lease.region_key.as_str()
                && residence.effective_from_game_day == lease.effective_from_game_day
                && residence.effective_to_game_day.is_none(),
            "current residence disagrees with its tenant lease"
        ),
        None => ensure!(
            residence.lease_contract_id.is_none()
                && !matches!(residence.tenure_type.as_str(), "jeonse" | "monthlyRent"),
            "current residence refers to a missing tenant lease"
        ),
    }
    let tenant_lease_deposit_krw = active_lease.as_ref().map_or(0, |lease| lease.deposit_krw);
    let ledger_deposit_krw = read_lease_deposit_ledger_balance(tx, save_id, run_revision).await?;
    ensure!(
        ledger_deposit_krw == tenant_lease_deposit_krw,
        "tenant lease deposit disagrees with its ledger projection"
    );
    Ok((tenant_lease_deposit_krw, active_lease))
}

pub(super) async fn read_active_lease_arrears_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<LeaseArrearWindow> {
    let mut rows: Vec<ActiveLeaseArrearRow> = sqlx::query_as(
        "SELECT arrear.id, arrear.lease_contract_id, arrear.lease_rent_charge_id,
                arrear.due_year_month, arrear.original_krw,
                arrear.paid_krw, arrear.remaining_krw,
                arrear.created_game_day
         FROM lease_arrear AS arrear
         INNER JOIN lease_rent_charge AS charge
           ON charge.save_id = arrear.save_id
          AND charge.run_revision = arrear.run_revision
          AND charge.id = arrear.lease_rent_charge_id
         WHERE arrear.save_id = ? AND arrear.run_revision = ?
           AND arrear.status = 'active'
         ORDER BY charge.due_year_month, arrear.id
         LIMIT 21",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    let has_more = rows.len() > MAX_ACTIVE_LEASE_ARREARS;
    rows.truncate(MAX_ACTIVE_LEASE_ARREARS);
    let items = rows
        .into_iter()
        .map(|row| {
            ensure!(
                row.id > 0
                    && row.lease_contract_id > 0
                    && row.lease_rent_charge_id > 0
                    && row.original_krw > 0
                    && row.paid_krw >= 0
                    && row.remaining_krw > 0
                    && row.paid_krw.checked_add(row.remaining_krw) == Some(row.original_krw),
                "active lease arrear has an invalid balance"
            );
            Ok(LeaseArrearState {
                id: ResourceId::from_u64(row.id),
                lease_id: ResourceId::from_u64(row.lease_contract_id),
                rent_charge_id: ResourceId::from_u64(row.lease_rent_charge_id),
                due_year_month: to_year_month(row.due_year_month)?,
                original_krw: row.original_krw,
                paid_krw: row.paid_krw,
                remaining_krw: row.remaining_krw,
                created_game_day: row.created_game_day,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let total_krw: i64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(remaining_krw), 0) AS SIGNED)
         FROM lease_arrear
         WHERE save_id = ? AND run_revision = ? AND status = 'active'",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await?;
    let window_total = items.iter().try_fold(0_i64, |total, arrear| {
        total
            .checked_add(arrear.remaining_krw)
            .context("lease-arrear window total overflowed")
    })?;
    ensure!(
        (!has_more && window_total == total_krw) || (has_more && total_krw > window_total),
        "lease-arrear window disagrees with its total"
    );
    Ok(LeaseArrearWindow {
        items,
        has_more,
        total_krw,
    })
}

async fn read_lease_catalog_in_tx(
    tx: &mut Transaction<'_, MySql>,
    model_version_id: u64,
    model_availability: &str,
    model_sealed: bool,
) -> Result<LeaseCatalogState> {
    let profiles: Vec<LeaseProfileRow> = sqlx::query_as(
        "SELECT offer_kind, renewal_rule, rent_charge_rule, arrear_repayment_rule,
                term_months, renewal_notice_lead_days,
                termination_review_rule, termination_review_after_days
         FROM real_estate_lease_profile
         WHERE real_estate_model_version_id = ?
         ORDER BY offer_kind",
    )
    .bind(model_version_id)
    .fetch_all(&mut **tx)
    .await?;
    if profiles.is_empty() {
        return Ok(LeaseCatalogState {
            capability: HousingLeaseCapability::Unavailable,
            renewal_rule: None,
            lease_lifecycle_terms: None,
            monthly_rent_terms: None,
            moving_costs: Vec::new(),
        });
    }
    ensure!(
        model_availability == "active" && model_sealed && (1..=2).contains(&profiles.len()),
        "lease-capable model has an invalid sealed profile set"
    );
    let jeonse = profiles
        .iter()
        .find(|profile| profile.offer_kind == "jeonse")
        .context("lease-capable model has no cash-jeonse profile")?;
    ensure!(
        jeonse.rent_charge_rule.is_none() && jeonse.arrear_repayment_rule.is_none(),
        "cash-jeonse profile has monthly-rent terms"
    );
    let (renewal_rule, lease_lifecycle_terms) = match jeonse.renewal_rule.as_str() {
        "openEnded" => {
            ensure!(
                jeonse.term_months.is_none()
                    && jeonse.renewal_notice_lead_days.is_none()
                    && jeonse.termination_review_rule.is_none()
                    && jeonse.termination_review_after_days.is_none(),
                "open-ended cash-jeonse profile has lifecycle terms"
            );
            (HousingLeaseRenewalRule::OpenEnded, None)
        }
        "fixedTermAutoRenew" => {
            ensure!(
                jeonse.term_months == Some(12)
                    && jeonse.renewal_notice_lead_days == Some(30)
                    && jeonse.termination_review_rule.is_none()
                    && jeonse.termination_review_after_days.is_none(),
                "fixed-term cash-jeonse profile has unsupported lifecycle terms"
            );
            (
                HousingLeaseRenewalRule::FixedTermAutoRenew,
                Some(LeaseLifecycleTermsState {
                    term_months: 12,
                    renewal_notice_lead_days: 30,
                    monthly_rent_termination_review: Some(MonthlyRentTerminationReviewTermsState {
                        rule: HousingLeaseTerminationReviewRule::OldestActiveArrearAge,
                        after_game_days: 60,
                    }),
                }),
            )
        }
        _ => bail!("cash-jeonse profile has an unknown renewal rule"),
    };
    let monthly_rent_terms = profiles
        .iter()
        .find(|profile| profile.offer_kind == "monthlyRent")
        .map(|profile| {
            ensure!(
                profile.renewal_rule == jeonse.renewal_rule
                    && profile.rent_charge_rule.as_deref() == Some("nextMonthStartFull")
                    && profile.arrear_repayment_rule.as_deref() == Some("manualOnly"),
                "monthly-rent profile has unsupported terms"
            );
            match renewal_rule {
                HousingLeaseRenewalRule::OpenEnded => ensure!(
                    profile.term_months.is_none()
                        && profile.renewal_notice_lead_days.is_none()
                        && profile.termination_review_rule.is_none()
                        && profile.termination_review_after_days.is_none(),
                    "open-ended monthly-rent profile has lifecycle terms"
                ),
                HousingLeaseRenewalRule::FixedTermAutoRenew => ensure!(
                    profile.term_months == Some(12)
                        && profile.renewal_notice_lead_days == Some(30)
                        && profile.termination_review_rule.as_deref()
                            == Some("oldestActiveArrearAge")
                        && profile.termination_review_after_days == Some(60),
                    "fixed-term monthly-rent profile has unsupported lifecycle terms"
                ),
            }
            Ok(MonthlyRentTermsState {
                rent_charge_rule: HousingRentChargeRule::NextMonthStartFull,
                arrear_repayment_rule: HousingLeaseArrearRepaymentRule::ManualOnly,
            })
        })
        .transpose()?;
    ensure!(
        profiles.len() == usize::from(monthly_rent_terms.is_some()) + 1,
        "lease catalog contains an unknown offer profile"
    );
    ensure!(
        renewal_rule == HousingLeaseRenewalRule::OpenEnded || monthly_rent_terms.is_some(),
        "fixed-term lease catalog is missing its monthly-rent profile"
    );
    let rows: Vec<MovingCostRow> = sqlx::query_as(
        "SELECT region.region_key, region.region_order, cost.moving_cost_krw
         FROM life_region AS region
         LEFT JOIN real_estate_region_moving_cost AS cost
           ON cost.real_estate_model_version_id = ?
          AND BINARY cost.region_key = BINARY region.region_key
         ORDER BY region.region_order, region.region_key",
    )
    .bind(model_version_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() == LifeRegionKey::ALL.len(),
        "lease moving-cost catalog is not bounded to four regions"
    );
    let moving_costs = rows
        .into_iter()
        .zip(LifeRegionKey::ALL)
        .map(|(row, expected)| {
            let region_key = LifeRegionKey::from_str(&row.region_key)
                .context("lease moving-cost catalog has an unknown region")?;
            ensure!(
                region_key == expected
                    && row.region_order == expected.order()
                    && row.moving_cost_krw.is_some_and(|amount| amount > 0),
                "lease moving-cost catalog is not canonical"
            );
            Ok(HousingMovingCostState {
                region_key,
                moving_cost_krw: row
                    .moving_cost_krw
                    .context("lease moving cost is missing")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(LeaseCatalogState {
        capability: if monthly_rent_terms.is_some() {
            HousingLeaseCapability::CashJeonseAndMonthlyRent
        } else {
            HousingLeaseCapability::CashJeonse
        },
        renewal_rule: Some(renewal_rule),
        lease_lifecycle_terms,
        monthly_rent_terms,
        moving_costs,
    })
}

async fn lock_lease_save(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
) -> Result<Option<LockedLeaseSaveRow>> {
    sqlx::query_as(
        "SELECT save.id, save.market_world_id, save.policy_set_id,
                save.run_revision, save.state_revision, save.game_day,
                save.cash_krw, save.debt_krw,
                EXISTS(SELECT 1 FROM `character` WHERE `character`.save_id = save.id)
                    AS has_character
         FROM save
         WHERE save.user_id = ?
         FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock the housing lease save")
}

pub(super) async fn lock_current_household(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<u64> {
    let rows: Vec<(u64,)> = sqlx::query_as(
        "SELECT id FROM household
         WHERE save_id = ? AND run_revision = ?
         ORDER BY id FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() == 1,
        "housing lease command requires exactly one current household"
    );
    Ok(rows[0].0)
}

pub(super) async fn tenant_lease_boundary_conflict_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    household_id: u64,
    game_day: u32,
) -> Result<bool> {
    let residences: Vec<(String, Option<u64>)> = sqlx::query_as(
        "SELECT tenure_type, property_holding_id
         FROM residence
         WHERE save_id = ? AND run_revision = ? AND household_id = ?
           AND effective_from_game_day <= ?
           AND (effective_to_game_day IS NULL OR effective_to_game_day > ?)
         ORDER BY id",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(household_id)
    .bind(game_day)
    .bind(game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        residences.len() == 1,
        "tenant lease boundary requires exactly one current residence"
    );
    let active_holding_ids: Vec<(u64,)> = sqlx::query_as(
        "SELECT id FROM property_holding
         WHERE save_id = ? AND run_revision = ? AND household_id = ?
           AND status = 'active'
         ORDER BY id",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(household_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        active_holding_ids.len() <= 1,
        "tenant lease boundary found multiple active property holdings"
    );
    let (tenure_type, property_holding_id) = &residences[0];
    Ok(tenant_lease_boundary_conflict(
        tenure_type,
        *property_holding_id,
        active_holding_ids.len(),
    ))
}

fn tenant_lease_boundary_conflict(
    tenure_type: &str,
    property_holding_id: Option<u64>,
    active_holding_count: usize,
) -> bool {
    tenure_type == "owner" || property_holding_id.is_some() || active_holding_count > 0
}

async fn read_current_model_scope(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedLeaseSaveRow,
) -> Result<LeaseModelScopeRow> {
    sqlx::query_as(
        "SELECT bundle.real_estate_model_version_id,
                model.availability AS model_availability,
                (model.sealed_at IS NOT NULL) AS model_sealed,
                market_daily.market_date
         FROM run_rule_bundle AS bundle
         INNER JOIN real_estate_model_version AS model
           ON model.id = bundle.real_estate_model_version_id
         INNER JOIN market_daily
           ON market_daily.world_id = bundle.market_world_id
          AND market_daily.game_day = ?
         WHERE bundle.save_id = ? AND bundle.run_revision = ?
           AND bundle.market_world_id = ?",
    )
    .bind(current.game_day)
    .bind(current.id)
    .bind(current.run_revision)
    .bind(current.market_world_id)
    .fetch_one(&mut **tx)
    .await
    .context("housing lease command has no current model scope")
}

async fn lock_current_residence(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    household_id: u64,
    game_day: u32,
) -> Result<ResidenceProjectionRow> {
    let mut rows: Vec<ResidenceProjectionRow> = sqlx::query_as(
        "SELECT id, household_id, region_key, tenure_type, effective_from_game_day,
                effective_to_game_day, lease_contract_id, property_holding_id
         FROM residence
         WHERE save_id = ? AND run_revision = ? AND household_id = ?
           AND effective_from_game_day <= ?
           AND (effective_to_game_day IS NULL OR effective_to_game_day > ?)
         ORDER BY id FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(household_id)
    .bind(game_day)
    .bind(game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() == 1,
        "housing lease command requires exactly one current residence"
    );
    rows.pop().context("current residence disappeared")
}

pub(super) async fn lock_existing_lease(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedLeaseSaveRow,
    household_id: u64,
    residence: &ResidenceProjectionRow,
    model_version_id: u64,
) -> Result<Option<LeaseProjectionRow>> {
    ensure!(
        residence.household_id == household_id
            && residence.effective_to_game_day.is_none()
            && matches!(
                residence.tenure_type.as_str(),
                "rentFree" | "jeonse" | "monthlyRent"
            ),
        "current residence cannot be replaced by a tenant lease"
    );
    let rows: Vec<LeaseProjectionRow> = sqlx::query_as(
        r#"
        SELECT lease.id,
               lease.household_id,
               lease.real_estate_model_version_id,
               lease.property_listing_id,
               lease.role,
               lease.region_key,
               lease.property_type,
               lease.exclusive_area_square_meters,
               lease.offer_kind,
               lease.deposit_krw,
               lease.monthly_rent_krw,
               lease.renewal_rule,
               lease.rent_charge_rule,
               lease.arrear_repayment_rule,
               lease.term_months,
               lease.renewal_notice_lead_days,
               lease.termination_review_rule,
               lease.termination_review_after_days,
               lease.effective_from_game_day,
               lease.effective_to_game_day,
               (
                   SELECT MIN(charge.due_game_day)
                   FROM lease_rent_charge AS charge
                   WHERE charge.save_id = lease.save_id
                     AND charge.run_revision = lease.run_revision
                     AND charge.lease_contract_id = lease.id
                     AND charge.status = 'pending'
               ) AS next_rent_due_game_day,
               (
                   SELECT loan.id
                   FROM loan_contract AS loan
                   WHERE loan.save_id = lease.save_id
                     AND loan.run_revision = lease.run_revision
                     AND loan.lease_contract_id = lease.id
               ) AS deposit_loan_id,
               listing.market_world_id AS listing_market_world_id,
               listing.real_estate_model_version_id AS listing_model_version_id,
               listing.region_key AS listing_region_key,
               listing.property_type AS listing_property_type,
               listing.exclusive_area_square_meters
                   AS listing_exclusive_area_square_meters,
               offer.deposit_krw AS listing_deposit_krw,
               offer.monthly_rent_krw AS listing_monthly_rent_krw,
               save.market_world_id AS save_market_world_id
        FROM lease_contract AS lease
        INNER JOIN save
          ON save.id = lease.save_id AND save.run_revision = lease.run_revision
        INNER JOIN property_listing AS listing
          ON listing.id = lease.property_listing_id
        INNER JOIN property_listing_offer AS offer
          ON offer.property_listing_id = lease.property_listing_id
         AND BINARY offer.offer_kind = BINARY lease.offer_kind
        WHERE lease.save_id = ? AND lease.run_revision = ? AND lease.household_id = ?
          AND lease.role = 'tenant'
          AND lease.effective_from_game_day <= ?
          AND (lease.effective_to_game_day IS NULL OR lease.effective_to_game_day > ?)
        ORDER BY lease.id FOR UPDATE
        "#,
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(household_id)
    .bind(current.game_day)
    .bind(current.game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() <= 1,
        "household has multiple current tenant leases"
    );
    let lease = rows.into_iter().next();
    if let Some(lease) = &lease {
        validate_lease_contract_lifecycle_terms(lease)?;
    }
    match (
        &residence.tenure_type[..],
        residence.lease_contract_id,
        &lease,
    ) {
        ("rentFree", None, None) => Ok(None),
        ("jeonse", Some(residence_lease_id), Some(lease)) => {
            ensure!(
                lease.id == residence_lease_id
                    && lease.household_id == household_id
                    && lease.real_estate_model_version_id == model_version_id
                    && lease.role == "tenant"
                    && lease.offer_kind == "jeonse"
                    && lease.deposit_krw > 0
                    && lease.monthly_rent_krw.is_none()
                    && lease.rent_charge_rule.is_none()
                    && lease.arrear_repayment_rule.is_none()
                    && lease.effective_from_game_day == residence.effective_from_game_day
                    && lease.effective_to_game_day.is_none()
                    && lease.region_key == residence.region_key,
                "current residence and tenant lease disagree"
            );
            validate_lease_listing_projection(lease)?;
            Ok(Some(lease.clone()))
        }
        ("monthlyRent", Some(residence_lease_id), Some(lease)) => {
            ensure!(
                lease.id == residence_lease_id
                    && lease.household_id == household_id
                    && lease.real_estate_model_version_id == model_version_id
                    && lease.role == "tenant"
                    && lease.offer_kind == "monthlyRent"
                    && lease.deposit_krw > 0
                    && lease.monthly_rent_krw.is_some_and(|amount| amount > 0)
                    && lease.rent_charge_rule.as_deref() == Some("nextMonthStartFull")
                    && lease.arrear_repayment_rule.as_deref() == Some("manualOnly")
                    && lease.effective_from_game_day == residence.effective_from_game_day
                    && lease.effective_to_game_day.is_none()
                    && lease.region_key == residence.region_key
                    && lease.next_rent_due_game_day.is_some(),
                "current residence and monthly-rent lease disagree"
            );
            validate_lease_listing_projection(lease)?;
            Ok(Some(lease.clone()))
        }
        _ => bail!("current residence has an invalid tenant lease connection"),
    }
}

async fn lock_listing(
    tx: &mut Transaction<'_, MySql>,
    listing_id: ResourceId,
) -> Result<Vec<LockedListingOfferRow>> {
    sqlx::query_as(
        "SELECT listing.id, listing.market_world_id,
                listing.real_estate_model_version_id, listing.`year_month`,
                listing.region_key, listing.property_type,
                listing.exclusive_area_square_meters,
                listing.available_from_game_day, listing.available_to_game_day,
                offer.offer_kind, offer.price_krw, offer.deposit_krw,
                offer.monthly_rent_krw
         FROM property_listing AS listing
         INNER JOIN property_listing_offer AS offer
           ON offer.property_listing_id = listing.id
         WHERE listing.id = ?
         ORDER BY offer.offer_order
         FOR UPDATE",
    )
    .bind(listing_id.get())
    .fetch_all(&mut **tx)
    .await
    .context("failed to lock the selected housing listing")
}

fn select_lease_listing<'a>(
    rows: &'a [LockedListingOfferRow],
    current: &LockedLeaseSaveRow,
    model: &LeaseModelScopeRow,
    command: &StartHousingLeaseCommand,
) -> Result<Option<&'a LockedListingOfferRow>> {
    if rows.is_empty() {
        return Ok(None);
    }
    let first = &rows[0];
    ensure!(
        rows.iter().all(|row| {
            row.id == first.id
                && row.market_world_id == first.market_world_id
                && row.real_estate_model_version_id == first.real_estate_model_version_id
                && row.year_month == first.year_month
                && row.region_key == first.region_key
                && row.property_type == first.property_type
                && row.exclusive_area_square_meters == first.exclusive_area_square_meters
                && row.available_from_game_day == first.available_from_game_day
                && row.available_to_game_day == first.available_to_game_day
        }),
        "housing listing offers disagree with their parent"
    );
    let current_month =
        Date::from_calendar_date(model.market_date.year(), model.market_date.month(), 1)
            .context("housing lease market month is invalid")?;
    if first.id != command.listing_id.get()
        || first.market_world_id != current.market_world_id
        || first.real_estate_model_version_id != model.real_estate_model_version_id
        || first.year_month != current_month
        || !(first.available_from_game_day..=first.available_to_game_day)
            .contains(&current.game_day)
    {
        return Ok(None);
    }
    let offer_kind = match command.offer_kind {
        HousingLeaseOfferKind::Jeonse => "jeonse",
        HousingLeaseOfferKind::MonthlyRent => "monthlyRent",
    };
    let Some(offer) = rows.iter().find(|row| row.offer_kind == offer_kind) else {
        return Ok(None);
    };
    let valid_shape = offer.price_krw.is_none()
        && offer.deposit_krw.is_some_and(|amount| amount > 0)
        && match command.offer_kind {
            HousingLeaseOfferKind::Jeonse => offer.monthly_rent_krw.is_none(),
            HousingLeaseOfferKind::MonthlyRent => {
                offer.monthly_rent_krw.is_some_and(|amount| amount > 0)
            }
        };
    if !valid_shape {
        bail!("housing listing has invalid lease terms");
    }
    Ok(Some(offer))
}

pub(super) fn validate_lease_rent_settlement_envelope(
    settlement: &crate::finance::ScheduledSettlement,
) -> Result<()> {
    ensure!(
        settlement.kind == crate::finance::SettlementKind::LeaseRent
            && settlement.source.kind == crate::finance::SettlementSourceKind::LeaseContract,
        "settlement is not monthly rent"
    );
    let payload: LeaseRentSettlementPayload = serde_json::from_value(settlement.payload.clone())
        .context("stored monthly-rent settlement payload is invalid")?;
    let lease_contract_id = payload
        .lease_contract_id
        .parse::<u64>()
        .context("monthly-rent settlement lease id is invalid")?;
    let rent_charge_id = payload
        .rent_charge_id
        .parse::<u64>()
        .context("monthly-rent settlement charge id is invalid")?;
    ensure!(
        payload.version == LEASE_RENT_PAYLOAD_VERSION
            && lease_contract_id > 0
            && rent_charge_id > 0
            && payload.charge_no > 0
            && settlement.source.source_id == payload.lease_contract_id
            && settlement.source.occurrence == u64::from(payload.charge_no),
        "stored monthly-rent settlement identity is invalid"
    );
    Ok(())
}

pub(super) async fn validate_due_lease_lifecycle_actions_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    target_game_day: u32,
) -> Result<()> {
    let rows: Vec<LeaseLifecycleActionEnvelopeRow> = sqlx::query_as(
        "SELECT action.id, action.lease_contract_id,
                action.lease_contract_term_id, action.lease_arrear_id,
                action.action_kind, action.payload_version, action.phase_rank,
                action.due_game_day, action.source_kind, action.source_id,
                action.occurrence, action.status,
                term.lease_contract_id AS term_contract_id,
                term.term_no,
                term.effective_from_game_day AS term_effective_from_game_day,
                term.effective_to_game_day AS term_effective_to_game_day,
                term.status AS term_status,
                arrear.lease_contract_id AS arrear_contract_id,
                arrear.created_game_day AS arrear_created_game_day,
                arrear.status AS arrear_status,
                contract.renewal_rule AS contract_renewal_rule,
                contract.term_months AS contract_term_months,
                contract.renewal_notice_lead_days
                    AS contract_renewal_notice_lead_days,
                contract.termination_review_rule
                    AS contract_termination_review_rule,
                contract.termination_review_after_days
                    AS contract_termination_review_after_days,
                contract.household_id AS contract_household_id,
                contract.effective_from_game_day
                    AS contract_effective_from_game_day,
                contract.effective_to_game_day AS contract_effective_to_game_day
         FROM lease_lifecycle_action AS action
         INNER JOIN lease_contract AS contract
           ON contract.id = action.lease_contract_id
          AND contract.save_id = action.save_id
          AND contract.run_revision = action.run_revision
         LEFT JOIN lease_contract_term AS term
           ON term.id = action.lease_contract_term_id
          AND term.save_id = action.save_id
          AND term.run_revision = action.run_revision
         LEFT JOIN lease_arrear AS arrear
           ON arrear.id = action.lease_arrear_id
          AND arrear.save_id = action.save_id
          AND arrear.run_revision = action.run_revision
         WHERE action.save_id = ? AND action.run_revision = ?
           AND action.status = 'pending' AND action.due_game_day <= ?
         ORDER BY action.due_game_day, action.phase_rank, action.id",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(target_game_day)
    .fetch_all(&mut **tx)
    .await?;
    for row in &rows {
        validate_lease_lifecycle_action_envelope(row, target_game_day)?;
    }
    ensure!(
        rows.windows(2).all(|pair| {
            (pair[0].due_game_day, pair[0].phase_rank, pair[0].id)
                < (pair[1].due_game_day, pair[1].phase_rank, pair[1].id)
        }),
        "due lease lifecycle actions are not in canonical order"
    );
    Ok(())
}

fn validate_lease_lifecycle_action_envelope(
    row: &LeaseLifecycleActionEnvelopeRow,
    target_game_day: u32,
) -> Result<()> {
    ensure!(
        row.id > 0
            && row.lease_contract_id > 0
            && row.payload_version == 1
            && row.due_game_day == target_game_day
            && row.status == "pending"
            && row.contract_renewal_rule == "fixedTermAutoRenew"
            && row.contract_term_months == Some(12)
            && row.contract_renewal_notice_lead_days == Some(30)
            && row.contract_household_id > 0
            && row.contract_effective_from_game_day <= target_game_day
            && row.contract_effective_to_game_day.is_none(),
        "due lease lifecycle action has invalid common terms"
    );
    match row.action_kind.as_str() {
        "renewalNotice" => ensure!(
            row.phase_rank == 500
                && row.lease_contract_term_id.is_some()
                && row.lease_arrear_id.is_none()
                && row.source_kind == "leaseTerm"
                && row.source_id == row.lease_contract_term_id.unwrap_or_default()
                && row.term_contract_id == Some(row.lease_contract_id)
                && row.term_no == Some(row.occurrence)
                && row.term_effective_from_game_day.is_some()
                && row.term_effective_to_game_day.is_some()
                && row.term_status.as_deref() == Some("active")
                && row
                    .term_effective_to_game_day
                    .and_then(|day| day.checked_sub(30))
                    == Some(row.due_game_day)
                && row.arrear_contract_id.is_none()
                && row.arrear_created_game_day.is_none()
                && row.arrear_status.is_none(),
            "renewal-notice action has an invalid typed payload"
        ),
        "termRenewal" => ensure!(
            row.phase_rank == 600
                && row.lease_contract_term_id.is_some()
                && row.lease_arrear_id.is_none()
                && row.source_kind == "leaseTerm"
                && row.source_id == row.lease_contract_term_id.unwrap_or_default()
                && row.term_contract_id == Some(row.lease_contract_id)
                && row.term_no == Some(row.occurrence)
                && row.term_effective_from_game_day.is_some()
                && row.term_effective_to_game_day == Some(row.due_game_day)
                && row.term_status.as_deref() == Some("active")
                && row.arrear_contract_id.is_none()
                && row.arrear_created_game_day.is_none()
                && row.arrear_status.is_none(),
            "term-renewal action has an invalid typed payload"
        ),
        "terminationReview" => ensure!(
            row.phase_rank == 700
                && row.lease_contract_term_id.is_none()
                && row.lease_arrear_id.is_some()
                && row.source_kind == "leaseArrear"
                && row.source_id == row.lease_arrear_id.unwrap_or_default()
                && row.occurrence == 1
                && row.term_contract_id.is_none()
                && row.term_no.is_none()
                && row.term_effective_from_game_day.is_none()
                && row.term_effective_to_game_day.is_none()
                && row.term_status.is_none()
                && row.arrear_contract_id == Some(row.lease_contract_id)
                && row.arrear_status.as_deref() == Some("active")
                && row.contract_termination_review_rule.as_deref() == Some("oldestActiveArrearAge")
                && row.contract_termination_review_after_days == Some(60)
                && row.due_game_day
                    == row
                        .arrear_created_game_day
                        .unwrap_or_default()
                        .checked_add(60)
                        .unwrap_or_default(),
            "termination-review action has an invalid typed payload"
        ),
        _ => bail!("due lease lifecycle action has an unknown kind"),
    }
    Ok(())
}

pub(super) async fn prelock_due_lease_contracts_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    target_game_day: u32,
) -> Result<()> {
    let has_due_lease_work: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM scheduled_settlement
             WHERE save_id = ? AND run_revision = ?
               AND kind = 'leaseRent' AND source_kind = 'leaseContract'
               AND status = 'pending' AND due_game_day <= ?
             UNION ALL
             SELECT 1 FROM lease_lifecycle_action
             WHERE save_id = ? AND run_revision = ?
               AND status = 'pending' AND due_game_day <= ?
         )",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(target_game_day)
    .bind(save_id)
    .bind(run_revision)
    .bind(target_game_day)
    .fetch_one(&mut **tx)
    .await?;
    if !has_due_lease_work {
        return Ok(());
    }
    let households: Vec<(u64,)> = sqlx::query_as(
        "SELECT id FROM household
         WHERE save_id = ? AND run_revision = ?
         ORDER BY id FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        households.len() == 1,
        "due monthly rent has no unique household"
    );
    let residences: Vec<(u64,)> = sqlx::query_as(
        "SELECT id FROM residence
         WHERE save_id = ? AND run_revision = ? AND effective_to_game_day IS NULL
         ORDER BY id FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        residences.len() == 1,
        "due monthly rent has no unique current residence"
    );
    let ids: Vec<(u64,)> = sqlx::query_as(
        "SELECT lease.id
         FROM lease_contract AS lease
         WHERE lease.save_id = ? AND lease.run_revision = ?
           AND (
             EXISTS (
               SELECT 1
               FROM scheduled_settlement AS settlement
               WHERE settlement.save_id = lease.save_id
                 AND settlement.run_revision = lease.run_revision
                 AND settlement.kind = 'leaseRent'
                 AND settlement.source_kind = 'leaseContract'
                 AND settlement.source_id = CAST(lease.id AS CHAR)
                 AND settlement.status = 'pending'
                 AND settlement.due_game_day <= ?
             )
             OR EXISTS (
               SELECT 1
               FROM lease_lifecycle_action AS action
               WHERE action.save_id = lease.save_id
                 AND action.run_revision = lease.run_revision
                 AND action.lease_contract_id = lease.id
                 AND action.status = 'pending'
                 AND action.due_game_day <= ?
             )
           )
         ORDER BY lease.id",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(target_game_day)
    .bind(target_game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        !ids.is_empty() && ids.windows(2).all(|pair| pair[0].0 < pair[1].0),
        "due monthly-rent lease locks are not canonical"
    );
    for (lease_id,) in ids {
        let locked_id: Option<u64> = sqlx::query_scalar(
            "SELECT id FROM lease_contract
             WHERE save_id = ? AND run_revision = ? AND id = ?
             FOR UPDATE",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(lease_id)
        .fetch_optional(&mut **tx)
        .await?;
        ensure!(
            locked_id == Some(lease_id),
            "due monthly-rent lease disappeared before its canonical lock"
        );
    }
    Ok(())
}

pub(super) async fn advance_lease_lifecycle_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    target_game_day: u32,
) -> Result<()> {
    let rows: Vec<LeaseLifecycleActionEnvelopeRow> = sqlx::query_as(
        "SELECT action.id, action.lease_contract_id,
                action.lease_contract_term_id, action.lease_arrear_id,
                action.action_kind, action.payload_version, action.phase_rank,
                action.due_game_day, action.source_kind, action.source_id,
                action.occurrence, action.status,
                term.lease_contract_id AS term_contract_id,
                term.term_no,
                term.effective_from_game_day AS term_effective_from_game_day,
                term.effective_to_game_day AS term_effective_to_game_day,
                term.status AS term_status,
                arrear.lease_contract_id AS arrear_contract_id,
                arrear.created_game_day AS arrear_created_game_day,
                arrear.status AS arrear_status,
                contract.renewal_rule AS contract_renewal_rule,
                contract.term_months AS contract_term_months,
                contract.renewal_notice_lead_days
                    AS contract_renewal_notice_lead_days,
                contract.termination_review_rule
                    AS contract_termination_review_rule,
                contract.termination_review_after_days
                    AS contract_termination_review_after_days,
                contract.household_id AS contract_household_id,
                contract.effective_from_game_day
                    AS contract_effective_from_game_day,
                contract.effective_to_game_day AS contract_effective_to_game_day
         FROM lease_lifecycle_action AS action
         INNER JOIN lease_contract AS contract
           ON contract.id = action.lease_contract_id
          AND contract.save_id = action.save_id
          AND contract.run_revision = action.run_revision
         LEFT JOIN lease_contract_term AS term
           ON term.id = action.lease_contract_term_id
          AND term.save_id = action.save_id
          AND term.run_revision = action.run_revision
         LEFT JOIN lease_arrear AS arrear
           ON arrear.id = action.lease_arrear_id
          AND arrear.save_id = action.save_id
          AND arrear.run_revision = action.run_revision
         WHERE action.save_id = ? AND action.run_revision = ?
           AND action.status = 'pending' AND action.due_game_day <= ?
         ORDER BY action.due_game_day, action.phase_rank, action.id
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(target_game_day)
    .fetch_all(&mut **tx)
    .await?;
    for row in rows {
        validate_lease_lifecycle_action_envelope(&row, target_game_day)?;
        match row.action_kind.as_str() {
            "renewalNotice" => {
                apply_lease_lifecycle_action(tx, save_id, run_revision, row.id, target_game_day)
                    .await?;
            }
            "termRenewal" => {
                let term_id = row
                    .lease_contract_term_id
                    .context("term-renewal action has no term")?;
                let term_no = row
                    .term_no
                    .context("term-renewal action has no term number")?;
                let notice_applied: bool = sqlx::query_scalar(
                    "SELECT EXISTS(
                         SELECT 1 FROM lease_lifecycle_action
                         WHERE save_id = ? AND run_revision = ?
                           AND lease_contract_id = ? AND lease_contract_term_id = ?
                           AND action_kind = 'renewalNotice' AND status = 'applied'
                           AND applied_game_day = due_game_day
                     )",
                )
                .bind(save_id)
                .bind(run_revision)
                .bind(row.lease_contract_id)
                .bind(term_id)
                .fetch_one(&mut **tx)
                .await?;
                ensure!(
                    notice_applied,
                    "lease term reached renewal without its notice"
                );
                let renewed = sqlx::query(
                    "UPDATE lease_contract_term
                     SET status = 'renewed', closed_game_day = ?
                     WHERE id = ? AND save_id = ? AND run_revision = ?
                       AND lease_contract_id = ? AND status = 'active'
                       AND effective_to_game_day = ?",
                )
                .bind(target_game_day)
                .bind(term_id)
                .bind(save_id)
                .bind(run_revision)
                .bind(row.lease_contract_id)
                .bind(target_game_day)
                .execute(&mut **tx)
                .await?;
                ensure!(
                    renewed.rows_affected() == 1,
                    "active lease term was not renewed"
                );
                let anchor_date: Date = sqlx::query_scalar(
                    "SELECT DATE_ADD(world.start_date,
                                     INTERVAL contract.effective_from_game_day DAY)
                     FROM lease_contract AS contract
                     INNER JOIN save ON save.id = contract.save_id
                     INNER JOIN market_world AS world ON world.id = save.market_world_id
                     WHERE contract.id = ? AND contract.save_id = ?
                       AND contract.run_revision = ?",
                )
                .bind(row.lease_contract_id)
                .bind(save_id)
                .bind(run_revision)
                .fetch_one(&mut **tx)
                .await?;
                let next_term_no = term_no
                    .checked_add(1)
                    .context("lease term number overflowed")?;
                let plan = create_lease_rules()
                    .plan_lease_term(LeaseTermPlanInput {
                        anchor_game_day: row.contract_effective_from_game_day,
                        anchor_date,
                        term_no: next_term_no,
                        term_months: row
                            .contract_term_months
                            .context("fixed-term contract has no duration")?,
                        renewal_notice_lead_days: row
                            .contract_renewal_notice_lead_days
                            .context("fixed-term contract has no notice lead")?,
                    })
                    .context("renewed lease term planning failed")?;
                ensure!(
                    plan.effective_from_game_day == target_game_day
                        && plan.renewal_game_day == plan.effective_to_game_day,
                    "renewed lease term is not contiguous"
                );
                insert_lease_term_and_actions(
                    tx,
                    save_id,
                    run_revision,
                    row.lease_contract_id,
                    plan,
                )
                .await?;
                apply_lease_lifecycle_action(tx, save_id, run_revision, row.id, target_game_day)
                    .await?;
            }
            "terminationReview" => {
                let arrear_id = row
                    .lease_arrear_id
                    .context("termination-review action has no arrear")?;
                let oldest_active_arrear_id: Option<u64> = sqlx::query_scalar(
                    "SELECT id FROM lease_arrear
                     WHERE save_id = ? AND run_revision = ? AND lease_contract_id = ?
                       AND status = 'active'
                     ORDER BY created_game_day, id
                     LIMIT 1",
                )
                .bind(save_id)
                .bind(run_revision)
                .bind(row.lease_contract_id)
                .fetch_optional(&mut **tx)
                .await?;
                ensure!(
                    oldest_active_arrear_id == Some(arrear_id),
                    "termination-review action no longer targets the oldest arrear"
                );
                let decision = create_lease_rules()
                    .decide_lease_termination_review(LeaseTerminationReviewInput {
                        current_game_day: target_game_day,
                        review_after_days: row
                            .contract_termination_review_after_days
                            .context("termination-review action has no threshold")?,
                        oldest_active_arrear_created_game_day: row.arrear_created_game_day,
                        review_is_open: false,
                    })
                    .context("lease termination-review opening failed")?;
                ensure!(
                    decision == LeaseTerminationReviewDecision::Open,
                    "due termination-review action did not open a review"
                );
                let review_no_raw: u64 = sqlx::query_scalar(
                    "SELECT CAST(COALESCE(MAX(review_no), 0) + 1 AS UNSIGNED)
                     FROM lease_termination_review WHERE lease_contract_id = ?",
                )
                .bind(row.lease_contract_id)
                .fetch_one(&mut **tx)
                .await?;
                let review_no = u32::try_from(review_no_raw)
                    .context("lease termination-review count is out of range")?;
                let review_id = sqlx::query(
                    "INSERT INTO lease_termination_review
                         (save_id, run_revision, household_id, lease_contract_id,
                          review_no, trigger_lease_lifecycle_action_id,
                          trigger_lease_arrear_id, status, opened_game_day,
                          resolved_game_day, resolution_reason)
                     VALUES (?, ?, ?, ?, ?, ?, ?, 'open', ?, NULL, NULL)",
                )
                .bind(save_id)
                .bind(run_revision)
                .bind(row.contract_household_id)
                .bind(row.lease_contract_id)
                .bind(review_no)
                .bind(row.id)
                .bind(arrear_id)
                .bind(target_game_day)
                .execute(&mut **tx)
                .await?
                .last_insert_id();
                ensure!(
                    review_id > 0,
                    "lease termination review has no durable identity"
                );
                apply_lease_lifecycle_action(tx, save_id, run_revision, row.id, target_game_day)
                    .await?;
            }
            _ => bail!("due lease lifecycle action has an unknown kind"),
        }
    }
    let overdue_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM lease_lifecycle_action
         WHERE save_id = ? AND run_revision = ? AND status = 'pending'
           AND due_game_day <= ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(target_game_day)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        overdue_count == 0,
        "daily lease lifecycle left an overdue action"
    );
    Ok(())
}

async fn apply_lease_lifecycle_action(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    action_id: u64,
    game_day: u32,
) -> Result<()> {
    let applied = sqlx::query(
        "UPDATE lease_lifecycle_action
         SET status = 'applied', applied_game_day = ?
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND status = 'pending' AND due_game_day = ?",
    )
    .bind(game_day)
    .bind(action_id)
    .bind(save_id)
    .bind(run_revision)
    .bind(game_day)
    .execute(&mut **tx)
    .await?;
    ensure!(
        applied.rows_affected() == 1,
        "lease lifecycle action was not applied"
    );
    Ok(())
}

pub(super) async fn settle_due_lease_rent_in_tx(
    tx: &mut Transaction<'_, MySql>,
    context: LeaseRentSettlementContext<'_>,
) -> Result<()> {
    let LeaseRentSettlementContext {
        finance_rules,
        save_id,
        run_revision,
        policy_set_id,
        game_day,
        market_date,
        settlement_id,
    } = context;
    let initial_projection = validate_debt_projection_in_tx(tx, save_id, run_revision).await?;
    let envelope: LeaseRentSettlementEnvelopeRow = sqlx::query_as(
        "SELECT due_game_day, CAST(payload AS CHAR CHARACTER SET utf8mb4) AS payload_json,
                source_id, occurrence
         FROM scheduled_settlement
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND kind = 'leaseRent' AND source_kind = 'leaseContract'
           AND status = 'pending'
         FOR UPDATE",
    )
    .bind(settlement_id)
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await
    .context("due monthly-rent settlement is missing")?;
    ensure!(
        envelope.due_game_day == game_day,
        "monthly-rent settlement is not due on this game day"
    );
    let payload: LeaseRentSettlementPayload = serde_json::from_str(&envelope.payload_json)
        .context("stored monthly-rent settlement payload is invalid")?;
    let contract_id = payload
        .lease_contract_id
        .parse::<u64>()
        .context("monthly-rent settlement lease id is invalid")?;
    let charge_id = payload
        .rent_charge_id
        .parse::<u64>()
        .context("monthly-rent settlement charge id is invalid")?;
    ensure!(
        payload.version == LEASE_RENT_PAYLOAD_VERSION
            && contract_id > 0
            && charge_id > 0
            && payload.charge_no > 0
            && envelope.source_id == payload.lease_contract_id
            && envelope.occurrence == payload.charge_no,
        "monthly-rent settlement identity changed"
    );
    let contract: LockedMonthlyRentContractRow = sqlx::query_as(
        "SELECT id, household_id, monthly_rent_krw, rent_charge_rule,
                arrear_repayment_rule, renewal_rule,
                termination_review_rule, termination_review_after_days,
                effective_to_game_day
         FROM lease_contract
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND role = 'tenant' AND offer_kind = 'monthlyRent'
         FOR UPDATE",
    )
    .bind(contract_id)
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await
    .context("monthly-rent settlement contract is missing")?;
    let monthly_rent_krw = contract
        .monthly_rent_krw
        .filter(|amount| *amount > 0)
        .context("monthly-rent contract has no positive rent")?;
    ensure!(
        contract.id == contract_id
            && contract.rent_charge_rule.as_deref() == Some("nextMonthStartFull")
            && contract.arrear_repayment_rule.as_deref() == Some("manualOnly")
            && match contract.renewal_rule.as_str() {
                "openEnded" => {
                    contract.termination_review_rule.is_none()
                        && contract.termination_review_after_days.is_none()
                }
                "fixedTermAutoRenew" => {
                    contract.termination_review_rule.as_deref() == Some("oldestActiveArrearAge")
                        && contract.termination_review_after_days == Some(60)
                }
                _ => false,
            }
            && contract.effective_to_game_day.is_none(),
        "monthly-rent contract is not active on its charge day"
    );
    let charge: LockedRentChargeRow = sqlx::query_as(
        "SELECT id, lease_contract_id, charge_no, due_year_month, due_game_day,
                amount_krw, paid_krw, arrear_krw, status
         FROM lease_rent_charge
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND lease_contract_id = ? AND charge_no = ?
         FOR UPDATE",
    )
    .bind(charge_id)
    .bind(save_id)
    .bind(run_revision)
    .bind(contract_id)
    .bind(payload.charge_no)
    .fetch_one(&mut **tx)
    .await
    .context("monthly-rent charge is missing")?;
    ensure!(
        charge.id == charge_id
            && charge.lease_contract_id == contract_id
            && charge.charge_no == payload.charge_no
            && charge.due_game_day == game_day
            && charge.due_year_month == month_start(market_date)?
            && charge.amount_krw == monthly_rent_krw
            && charge.paid_krw.is_none()
            && charge.arrear_krw.is_none()
            && charge.status == "pending",
        "monthly-rent charge disagrees with its settlement"
    );
    let wallet_cash_krw: i64 = sqlx::query_scalar(
        "SELECT cash_krw FROM save WHERE id = ? AND run_revision = ? FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await?;
    let plan = create_lease_rules()
        .plan_monthly_rent_settlement(MonthlyRentSettlementInput {
            wallet_cash_krw,
            monthly_rent_krw,
        })
        .context("monthly-rent allocation failed")?;
    let arrear_id = if plan.arrear_krw > 0 {
        let inserted = sqlx::query(
            "INSERT INTO lease_arrear
                 (save_id, run_revision, household_id, lease_contract_id,
                  lease_rent_charge_id, due_year_month, original_krw, paid_krw,
                  status, created_game_day, closed_game_day)
             VALUES (?, ?, ?, ?, ?, ?, ?, 0, 'active', ?, NULL)",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(contract.household_id)
        .bind(contract_id)
        .bind(charge_id)
        .bind(charge.due_year_month)
        .bind(plan.arrear_krw)
        .bind(game_day)
        .execute(&mut **tx)
        .await?
        .last_insert_id();
        ensure!(inserted > 0, "monthly-rent arrear has no durable identity");
        Some(inserted)
    } else {
        None
    };
    if arrear_id.is_some() {
        schedule_next_lease_termination_review_action(
            tx,
            save_id,
            run_revision,
            game_day,
            &contract,
        )
        .await?;
    }
    let ledger = finance_rules.create_ledger_transaction(LedgerTransactionDraft {
        policy: RunPolicyContext {
            run: RunId {
                save_id: ResourceId::from_u64(save_id),
                run_revision,
            },
            policy_set_id: ResourceId::from_u64(policy_set_id),
        },
        source: LedgerSource {
            kind: LedgerSourceKind::LeaseRent,
            source_id: charge_id.to_string(),
        },
        game_day,
        description: "월세 청구".to_owned(),
        postings: plan
            .postings
            .iter()
            .map(|posting| LedgerPosting {
                account_code: posting.account_code,
                financial_account_id: None,
                amount_krw: posting.amount_krw,
            })
            .collect(),
    })?;
    let references = plan
        .postings
        .iter()
        .map(|posting| match posting.owner {
            LeaseRentPostingOwner::None => Ok(LeasePostingReference::None),
            LeaseRentPostingOwner::RentCharge => Ok(LeasePostingReference::RentCharge(charge_id)),
            LeaseRentPostingOwner::Arrear => Ok(LeasePostingReference::Arrear(
                arrear_id.context("lease-rent arrear posting has no arrear")?,
            )),
        })
        .collect::<Result<Vec<_>>>()?;
    let ledger_transaction_id = write_lease_ledger_transaction(tx, &ledger, &references).await?;
    let charge_updated = sqlx::query(
        "UPDATE lease_rent_charge
         SET paid_krw = ?, arrear_krw = ?, status = 'settled', ledger_transaction_id = ?
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND status = 'pending' AND paid_krw IS NULL AND arrear_krw IS NULL",
    )
    .bind(plan.paid_krw)
    .bind(plan.arrear_krw)
    .bind(ledger_transaction_id)
    .bind(charge_id)
    .bind(save_id)
    .bind(run_revision)
    .execute(&mut **tx)
    .await?;
    ensure!(
        charge_updated.rows_affected() == 1,
        "monthly-rent charge changed during settlement"
    );
    let settlement_updated = sqlx::query(
        "UPDATE scheduled_settlement
         SET status = 'settled', outcome = 'applied',
             settled_ledger_transaction_id = ?
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'pending'",
    )
    .bind(ledger_transaction_id)
    .bind(settlement_id)
    .bind(save_id)
    .bind(run_revision)
    .execute(&mut **tx)
    .await?;
    ensure!(
        settlement_updated.rows_affected() == 1,
        "monthly-rent settlement changed during execution"
    );
    let next_due = create_lease_rules()
        .next_monthly_rent_charge(game_day, market_date)
        .context("next monthly-rent charge date is invalid")?;
    let next_charge_no = charge
        .charge_no
        .checked_add(1)
        .context("monthly-rent charge number overflowed")?;
    insert_lease_rent_charge_and_settlement(
        tx,
        LeaseRentChargeDraft {
            save_id,
            run_revision,
            lease_contract_id: contract_id,
            charge_no: next_charge_no,
            due_year_month: year_month_date(next_due.due_year_month)?,
            due_game_day: next_due.due_game_day,
            amount_krw: monthly_rent_krw,
        },
    )
    .await?;
    let projection = calculate_debt_projection_in_tx(tx, save_id, run_revision).await?;
    let save_updated = sqlx::query(
        "UPDATE save SET cash_krw = ?, debt_krw = ?
         WHERE id = ? AND run_revision = ? AND game_day + 1 = ?
           AND cash_krw = ? AND debt_krw = ?",
    )
    .bind(plan.wallet_after_krw)
    .bind(projection.total_krw)
    .bind(save_id)
    .bind(run_revision)
    .bind(game_day)
    .bind(wallet_cash_krw)
    .bind(initial_projection.total_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        save_updated.rows_affected() == 1,
        "save changed during monthly-rent settlement"
    );
    validate_debt_projection_in_tx(tx, save_id, run_revision).await?;
    Ok(())
}

async fn schedule_next_lease_termination_review_action(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
    contract: &LockedMonthlyRentContractRow,
) -> Result<()> {
    if contract.renewal_rule == "openEnded" {
        ensure!(
            contract.termination_review_rule.is_none()
                && contract.termination_review_after_days.is_none(),
            "open-ended monthly-rent contract has review terms"
        );
        return Ok(());
    }
    ensure!(
        contract.renewal_rule == "fixedTermAutoRenew"
            && contract.termination_review_rule.as_deref() == Some("oldestActiveArrearAge")
            && contract.termination_review_after_days == Some(60)
            && contract.effective_to_game_day.is_none(),
        "monthly-rent contract has unsupported review terms"
    );
    let has_open_review: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM lease_termination_review
             WHERE save_id = ? AND run_revision = ? AND lease_contract_id = ?
               AND status = 'open'
         )",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract.id)
    .fetch_one(&mut **tx)
    .await?;
    let has_pending_action: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM lease_lifecycle_action
             WHERE save_id = ? AND run_revision = ? AND lease_contract_id = ?
               AND action_kind = 'terminationReview' AND status = 'pending'
         )",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract.id)
    .fetch_one(&mut **tx)
    .await?;
    if has_open_review || has_pending_action {
        return Ok(());
    }
    let oldest: Option<(u64, u32)> = sqlx::query_as(
        "SELECT id, created_game_day
         FROM lease_arrear
         WHERE save_id = ? AND run_revision = ? AND lease_contract_id = ?
           AND status = 'active'
         ORDER BY created_game_day, id
         LIMIT 1",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract.id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((arrear_id, created_game_day)) = oldest else {
        return Ok(());
    };
    let review_after_days = contract
        .termination_review_after_days
        .context("fixed-term monthly-rent contract has no review threshold")?;
    let canonical_due_game_day = created_game_day
        .checked_add(u32::from(review_after_days))
        .context("lease termination-review due day overflowed")?;
    let decision = create_lease_rules()
        .decide_lease_termination_review(LeaseTerminationReviewInput {
            current_game_day: game_day,
            review_after_days,
            oldest_active_arrear_created_game_day: Some(created_game_day),
            review_is_open: false,
        })
        .context("lease termination-review scheduling failed")?;
    let due_game_day = match decision {
        LeaseTerminationReviewDecision::Schedule { due_game_day } => {
            ensure!(
                due_game_day == canonical_due_game_day,
                "lease termination-review planner changed its canonical due day"
            );
            due_game_day
        }
        LeaseTerminationReviewDecision::Open => {
            ensure!(
                canonical_due_game_day == game_day,
                "lease termination-review scheduling found an overdue action gap"
            );
            canonical_due_game_day
        }
        LeaseTerminationReviewDecision::NoAction
        | LeaseTerminationReviewDecision::KeepOpen
        | LeaseTerminationReviewDecision::Resolve => {
            bail!("lease termination-review planner returned an invalid scheduling decision")
        }
    };
    let action_id = sqlx::query(
        "INSERT INTO lease_lifecycle_action
             (save_id, run_revision, lease_contract_id,
              lease_contract_term_id, lease_arrear_id,
              action_kind, payload_version, phase_rank, due_game_day,
              source_kind, source_id, occurrence, status,
              applied_game_day, cancelled_game_day, cancellation_reason)
         VALUES (?, ?, ?, NULL, ?, 'terminationReview', 1, 700, ?,
                 'leaseArrear', ?, 1, 'pending', NULL, NULL, NULL)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract.id)
    .bind(arrear_id)
    .bind(due_game_day)
    .bind(arrear_id)
    .execute(&mut **tx)
    .await?
    .last_insert_id();
    ensure!(
        action_id > 0,
        "lease termination-review action has no durable identity"
    );
    Ok(())
}

async fn reconcile_lease_termination_review_after_payment(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
    contract: &LockedMonthlyRentContractRow,
    paid_arrear_id: u64,
    paid_in_full: bool,
) -> Result<()> {
    if !paid_in_full || contract.renewal_rule == "openEnded" {
        return Ok(());
    }
    ensure!(
        contract.renewal_rule == "fixedTermAutoRenew"
            && contract.termination_review_rule.as_deref() == Some("oldestActiveArrearAge")
            && contract.termination_review_after_days == Some(60),
        "lease-arrear contract has unsupported review terms"
    );
    sqlx::query(
        "UPDATE lease_lifecycle_action
         SET status = 'cancelled', cancelled_game_day = ?,
             cancellation_reason = 'arrearPaid'
         WHERE save_id = ? AND run_revision = ? AND lease_contract_id = ?
           AND lease_arrear_id = ? AND action_kind = 'terminationReview'
           AND status = 'pending'",
    )
    .bind(game_day)
    .bind(save_id)
    .bind(run_revision)
    .bind(contract.id)
    .bind(paid_arrear_id)
    .execute(&mut **tx)
    .await?;
    let review_is_open: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM lease_termination_review
             WHERE save_id = ? AND run_revision = ? AND lease_contract_id = ?
               AND status = 'open'
         )",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract.id)
    .fetch_one(&mut **tx)
    .await?;
    let oldest_created_game_day: Option<u32> = sqlx::query_scalar(
        "SELECT created_game_day
         FROM lease_arrear
         WHERE save_id = ? AND run_revision = ? AND lease_contract_id = ?
           AND status = 'active'
         ORDER BY created_game_day, id
         LIMIT 1",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(contract.id)
    .fetch_optional(&mut **tx)
    .await?;
    let decision = create_lease_rules()
        .decide_lease_termination_review(LeaseTerminationReviewInput {
            current_game_day: game_day,
            review_after_days: contract
                .termination_review_after_days
                .context("fixed-term monthly-rent contract has no review threshold")?,
            oldest_active_arrear_created_game_day: oldest_created_game_day,
            review_is_open,
        })
        .context("lease termination-review reconciliation failed")?;
    match decision {
        LeaseTerminationReviewDecision::Resolve => {
            let resolved = sqlx::query(
                "UPDATE lease_termination_review
                 SET status = 'resolved', resolved_game_day = ?,
                     resolution_reason = 'arrearsCleared'
                 WHERE save_id = ? AND run_revision = ? AND lease_contract_id = ?
                   AND status = 'open'",
            )
            .bind(game_day)
            .bind(save_id)
            .bind(run_revision)
            .bind(contract.id)
            .execute(&mut **tx)
            .await?;
            ensure!(
                resolved.rows_affected() == 1,
                "open lease termination review was not resolved"
            );
        }
        LeaseTerminationReviewDecision::KeepOpen | LeaseTerminationReviewDecision::NoAction => {}
        LeaseTerminationReviewDecision::Schedule { .. } => {
            if contract.effective_to_game_day.is_none() {
                schedule_next_lease_termination_review_action(
                    tx,
                    save_id,
                    run_revision,
                    game_day,
                    contract,
                )
                .await?;
            }
        }
        LeaseTerminationReviewDecision::Open => {
            bail!("lease termination review became overdue during an arrear payment")
        }
    }
    Ok(())
}

async fn insert_lease_rent_charge_and_settlement(
    tx: &mut Transaction<'_, MySql>,
    draft: LeaseRentChargeDraft,
) -> Result<u64> {
    let LeaseRentChargeDraft {
        save_id,
        run_revision,
        lease_contract_id,
        charge_no,
        due_year_month,
        due_game_day,
        amount_krw,
    } = draft;
    ensure!(
        lease_contract_id > 0 && charge_no > 0 && due_year_month.day() == 1 && amount_krw > 0,
        "monthly-rent charge draft is invalid"
    );
    let inserted = sqlx::query(
        "INSERT INTO lease_rent_charge
             (save_id, run_revision, lease_contract_id, charge_no,
              due_year_month, due_game_day, amount_krw, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'pending')",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(lease_contract_id)
    .bind(charge_no)
    .bind(due_year_month)
    .bind(due_game_day)
    .bind(amount_krw)
    .execute(&mut **tx)
    .await?;
    let rent_charge_id = inserted.last_insert_id();
    ensure!(
        rent_charge_id > 0,
        "monthly-rent charge has no durable identity"
    );
    let payload = serde_json::to_string(&LeaseRentSettlementPayload {
        version: LEASE_RENT_PAYLOAD_VERSION,
        lease_contract_id: lease_contract_id.to_string(),
        rent_charge_id: rent_charge_id.to_string(),
        charge_no,
    })?;
    sqlx::query(
        "INSERT INTO scheduled_settlement
             (save_id, run_revision, due_game_day, kind, payload,
              source_kind, source_id, occurrence, status)
         VALUES (?, ?, ?, 'leaseRent', ?, 'leaseContract', ?, ?, 'pending')",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(due_game_day)
    .bind(payload)
    .bind(lease_contract_id.to_string())
    .bind(charge_no)
    .execute(&mut **tx)
    .await?;
    Ok(rent_charge_id)
}

pub(super) async fn cancel_future_lease_rent_charge(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedLeaseSaveRow,
    lease: &LeaseProjectionRow,
) -> Result<()> {
    let charges: Vec<(u64, u32, u32)> = sqlx::query_as(
        "SELECT id, charge_no, due_game_day
         FROM lease_rent_charge
         WHERE save_id = ? AND run_revision = ? AND lease_contract_id = ?
           AND status = 'pending'
         ORDER BY charge_no
         FOR UPDATE",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(lease.id)
    .fetch_all(&mut **tx)
    .await?;
    if lease.offer_kind == "jeonse" {
        ensure!(charges.is_empty(), "cash-jeonse lease has a rent charge");
        return Ok(());
    }
    ensure!(
        charges.len() == 1 && charges[0].2 > current.game_day,
        "active monthly-rent lease has no unique future charge"
    );
    let (charge_id, charge_no, _) = charges[0];
    let charge = sqlx::query(
        "UPDATE lease_rent_charge
         SET status = 'cancelled'
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND lease_contract_id = ? AND charge_no = ? AND status = 'pending'",
    )
    .bind(charge_id)
    .bind(current.id)
    .bind(current.run_revision)
    .bind(lease.id)
    .bind(charge_no)
    .execute(&mut **tx)
    .await?;
    ensure!(
        charge.rows_affected() == 1,
        "future monthly-rent charge was not cancelled"
    );
    let settlement = sqlx::query(
        "UPDATE scheduled_settlement
         SET status = 'cancelled', cancellation_reason = 'leaseEnded'
         WHERE save_id = ? AND run_revision = ?
           AND kind = 'leaseRent' AND source_kind = 'leaseContract'
           AND source_id = ? AND occurrence = ? AND status = 'pending'",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(lease.id.to_string())
    .bind(charge_no)
    .execute(&mut **tx)
    .await?;
    ensure!(
        settlement.rows_affected() == 1,
        "future monthly-rent settlement was not cancelled"
    );
    Ok(())
}

async fn insert_initial_lease_lifecycle(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedLeaseSaveRow,
    lease_contract_id: u64,
    market_date: Date,
    terms: LeaseLifecycleTermsState,
) -> Result<()> {
    let plan = create_lease_rules()
        .plan_lease_term(LeaseTermPlanInput {
            anchor_game_day: current.game_day,
            anchor_date: market_date,
            term_no: 1,
            term_months: terms.term_months,
            renewal_notice_lead_days: terms.renewal_notice_lead_days,
        })
        .context("initial lease term planning failed")?;
    ensure!(
        plan.term_no == 1
            && plan.effective_from_game_day == current.game_day
            && plan.renewal_game_day == plan.effective_to_game_day,
        "initial lease term plan is not anchored to move-in"
    );
    insert_lease_term_and_actions(
        tx,
        current.id,
        current.run_revision,
        lease_contract_id,
        plan,
    )
    .await?;
    Ok(())
}

async fn insert_lease_term_and_actions(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    lease_contract_id: u64,
    plan: LeaseTermPlan,
) -> Result<u64> {
    let term_id = sqlx::query(
        "INSERT INTO lease_contract_term
             (save_id, run_revision, lease_contract_id, term_no,
              effective_from_game_day, effective_to_game_day, status,
              closed_game_day, termination_reason)
         VALUES (?, ?, ?, ?, ?, ?, 'active', NULL, NULL)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(lease_contract_id)
    .bind(plan.term_no)
    .bind(plan.effective_from_game_day)
    .bind(plan.effective_to_game_day)
    .execute(&mut **tx)
    .await?
    .last_insert_id();
    ensure!(term_id > 0, "lease term has no durable identity");
    for (action_kind, phase_rank, due_game_day) in [
        ("renewalNotice", 500_u16, plan.renewal_notice_game_day),
        ("termRenewal", 600_u16, plan.effective_to_game_day),
    ] {
        let action_id = sqlx::query(
            "INSERT INTO lease_lifecycle_action
                 (save_id, run_revision, lease_contract_id,
                  lease_contract_term_id, lease_arrear_id,
                  action_kind, payload_version, phase_rank, due_game_day,
                  source_kind, source_id, occurrence, status,
                  applied_game_day, cancelled_game_day, cancellation_reason)
             VALUES (?, ?, ?, ?, NULL, ?, 1, ?, ?, 'leaseTerm', ?, ?,
                     'pending', NULL, NULL, NULL)",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(lease_contract_id)
        .bind(term_id)
        .bind(action_kind)
        .bind(phase_rank)
        .bind(due_game_day)
        .bind(term_id)
        .bind(plan.term_no)
        .execute(&mut **tx)
        .await?
        .last_insert_id();
        ensure!(
            action_id > 0,
            "lease lifecycle action has no durable identity"
        );
    }
    Ok(term_id)
}

pub(super) async fn close_existing_lease_lifecycle(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedLeaseSaveRow,
    lease: &LeaseProjectionRow,
) -> Result<()> {
    if validate_lease_contract_lifecycle_terms(lease)? == HousingLeaseRenewalRule::OpenEnded {
        return Ok(());
    }
    sqlx::query(
        "UPDATE lease_lifecycle_action
         SET status = 'cancelled', cancelled_game_day = ?,
             cancellation_reason = 'leaseEnded'
         WHERE save_id = ? AND run_revision = ? AND lease_contract_id = ?
           AND status = 'pending'",
    )
    .bind(current.game_day)
    .bind(current.id)
    .bind(current.run_revision)
    .bind(lease.id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE lease_termination_review
         SET status = 'resolved', resolved_game_day = ?,
             resolution_reason = 'leaseEnded'
         WHERE save_id = ? AND run_revision = ? AND lease_contract_id = ?
           AND status = 'open'",
    )
    .bind(current.game_day)
    .bind(current.id)
    .bind(current.run_revision)
    .bind(lease.id)
    .execute(&mut **tx)
    .await?;
    let term_updated = sqlx::query(
        "UPDATE lease_contract_term
         SET status = 'terminated', closed_game_day = ?,
             termination_reason = 'leaseEnded'
         WHERE save_id = ? AND run_revision = ? AND lease_contract_id = ?
           AND status = 'active'",
    )
    .bind(current.game_day)
    .bind(current.id)
    .bind(current.run_revision)
    .bind(lease.id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        term_updated.rows_affected() == 1,
        "fixed-term tenant lease has no active term to close"
    );
    Ok(())
}

pub(super) async fn close_existing_lease(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedLeaseSaveRow,
    lease: &LeaseProjectionRow,
) -> Result<()> {
    let updated = sqlx::query(
        "UPDATE lease_contract SET effective_to_game_day = ?
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND effective_from_game_day < ? AND effective_to_game_day IS NULL",
    )
    .bind(current.game_day)
    .bind(lease.id)
    .bind(current.id)
    .bind(current.run_revision)
    .bind(current.game_day)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "current tenant lease changed during move"
    );
    Ok(())
}

pub(super) async fn close_existing_residence(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedLeaseSaveRow,
    residence: &ResidenceProjectionRow,
) -> Result<()> {
    let updated = sqlx::query(
        "UPDATE residence SET effective_to_game_day = ?
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND effective_from_game_day < ? AND effective_to_game_day IS NULL",
    )
    .bind(current.game_day)
    .bind(residence.id)
    .bind(current.id)
    .bind(current.run_revision)
    .bind(current.game_day)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "current residence changed during move"
    );
    Ok(())
}

async fn insert_tenant_lease(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedLeaseSaveRow,
    household_id: u64,
    listing: &LockedListingOfferRow,
    command: &StartHousingLeaseCommand,
    deposit_krw: i64,
    catalog: &LeaseCatalogState,
) -> Result<u64> {
    let renewal_rule = catalog
        .renewal_rule
        .context("lease-capable catalog has no renewal rule")?;
    let (term_months, renewal_notice_lead_days) =
        catalog.lease_lifecycle_terms.map_or((None, None), |terms| {
            (
                Some(terms.term_months),
                Some(terms.renewal_notice_lead_days),
            )
        });
    let termination_review_terms = if command.offer_kind == HousingLeaseOfferKind::MonthlyRent {
        catalog
            .lease_lifecycle_terms
            .and_then(|terms| terms.monthly_rent_termination_review)
    } else {
        None
    };
    let inserted = sqlx::query(
        "INSERT INTO lease_contract
             (save_id, run_revision, household_id, real_estate_model_version_id,
              property_listing_id, command_id, role, region_key, property_type,
              exclusive_area_square_meters, offer_kind, deposit_krw,
              monthly_rent_krw, renewal_rule, rent_charge_rule,
              arrear_repayment_rule, term_months, renewal_notice_lead_days,
              termination_review_rule, termination_review_after_days,
              effective_from_game_day, effective_to_game_day)
         VALUES (?, ?, ?, ?, ?, ?, 'tenant', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(household_id)
    .bind(listing.real_estate_model_version_id)
    .bind(listing.id)
    .bind(command.command_id.as_str())
    .bind(&listing.region_key)
    .bind(&listing.property_type)
    .bind(listing.exclusive_area_square_meters)
    .bind(match command.offer_kind {
        HousingLeaseOfferKind::Jeonse => "jeonse",
        HousingLeaseOfferKind::MonthlyRent => "monthlyRent",
    })
    .bind(deposit_krw)
    .bind(listing.monthly_rent_krw)
    .bind(match renewal_rule {
        HousingLeaseRenewalRule::OpenEnded => "openEnded",
        HousingLeaseRenewalRule::FixedTermAutoRenew => "fixedTermAutoRenew",
    })
    .bind(match command.offer_kind {
        HousingLeaseOfferKind::Jeonse => None,
        HousingLeaseOfferKind::MonthlyRent => Some("nextMonthStartFull"),
    })
    .bind(match command.offer_kind {
        HousingLeaseOfferKind::Jeonse => None,
        HousingLeaseOfferKind::MonthlyRent => Some("manualOnly"),
    })
    .bind(term_months)
    .bind(renewal_notice_lead_days)
    .bind(termination_review_terms.map(|terms| match terms.rule {
        HousingLeaseTerminationReviewRule::OldestActiveArrearAge => "oldestActiveArrearAge",
    }))
    .bind(termination_review_terms.map(|terms| terms.after_game_days))
    .bind(current.game_day)
    .execute(&mut **tx)
    .await?;
    let lease_id = inserted.last_insert_id();
    ensure!(lease_id > 0, "tenant lease has no durable identity");
    Ok(lease_id)
}

async fn insert_lease_residence(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedLeaseSaveRow,
    household_id: u64,
    lease_id: u64,
    region_key: &str,
    offer_kind: HousingLeaseOfferKind,
) -> Result<u64> {
    let inserted = sqlx::query(
        "INSERT INTO residence
             (save_id, run_revision, household_id, region_key, tenure_type,
              lease_contract_id, effective_from_game_day, effective_to_game_day)
         VALUES (?, ?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(household_id)
    .bind(region_key)
    .bind(match offer_kind {
        HousingLeaseOfferKind::Jeonse => "jeonse",
        HousingLeaseOfferKind::MonthlyRent => "monthlyRent",
    })
    .bind(lease_id)
    .bind(current.game_day)
    .execute(&mut **tx)
    .await?;
    let residence_id = inserted.last_insert_id();
    ensure!(residence_id > 0, "lease residence has no durable identity");
    Ok(residence_id)
}

async fn write_lease_move_ledger(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    current: &LockedLeaseSaveRow,
    command_id: &str,
    plan: &LeaseMoveFundingPlan,
    owners: LeaseMoveLedgerOwners,
) -> Result<u64> {
    let ledger = finance_rules.create_ledger_transaction(LedgerTransactionDraft {
        policy: RunPolicyContext {
            run: RunId {
                save_id: ResourceId::from_u64(current.id),
                run_revision: current.run_revision,
            },
            policy_set_id: ResourceId::from_u64(current.policy_set_id),
        },
        source: LedgerSource {
            kind: LedgerSourceKind::LeaseMove,
            source_id: command_id.to_owned(),
        },
        game_day: current.game_day,
        description: "임대차 이동".to_owned(),
        postings: plan
            .postings
            .iter()
            .map(|posting| LedgerPosting {
                account_code: posting.account_code,
                financial_account_id: None,
                amount_krw: posting.amount_krw,
            })
            .collect(),
    })?;
    write_lease_ledger_transaction(
        tx,
        &ledger,
        &plan
            .postings
            .iter()
            .map(
                |posting| match (posting.lease_contract, posting.loan_contract) {
                    (Some(LeaseMovePostingLease::Ended), None) => owners
                        .ended_lease_id
                        .map(LeasePostingReference::Contract)
                        .context("lease-move ended posting has no ended lease"),
                    (Some(LeaseMovePostingLease::Started), None) => {
                        Ok(LeasePostingReference::Contract(owners.started_lease_id))
                    }
                    (None, Some(LeaseMovePostingLoan::Repaid)) => owners
                        .repaid_loan_id
                        .map(LeasePostingReference::LoanContract)
                        .context("lease-move payoff posting has no repaid loan"),
                    (None, Some(LeaseMovePostingLoan::Originated)) => owners
                        .originated_loan_id
                        .map(LeasePostingReference::LoanContract)
                        .context("lease-move origination posting has no originated loan"),
                    (None, None) => Ok(LeasePostingReference::None),
                    (Some(_), Some(_)) => bail!("lease-move posting has two owners"),
                },
            )
            .collect::<Result<Vec<_>>>()?,
    )
    .await
}

async fn write_lease_ledger_transaction(
    tx: &mut Transaction<'_, MySql>,
    ledger: &LedgerTransaction,
    references: &[LeasePostingReference],
) -> Result<u64> {
    ensure!(
        ledger.postings().len() == references.len(),
        "lease ledger posting ownership is incomplete"
    );
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
    let transaction_id = inserted.last_insert_id();
    for (index, (posting, reference)) in ledger.postings().iter().zip(references.iter()).enumerate()
    {
        let (lease_contract_id, loan_contract_id, lease_rent_charge_id, lease_arrear_id) =
            match reference {
                LeasePostingReference::None => (None, None, None, None),
                LeasePostingReference::Contract(id) => (Some(*id), None, None, None),
                LeasePostingReference::LoanContract(id) => (None, Some(*id), None, None),
                LeasePostingReference::RentCharge(id) => (None, None, Some(*id), None),
                LeasePostingReference::Arrear(id) => (None, None, None, Some(*id)),
            };
        let valid_reference = match posting.account_code {
            LedgerAccountCode::LeaseDepositAsset => {
                matches!(reference, LeasePostingReference::Contract(_))
            }
            LedgerAccountCode::LeaseRentExpense => {
                matches!(reference, LeasePostingReference::RentCharge(_))
            }
            LedgerAccountCode::LeaseArrearLiability => {
                matches!(reference, LeasePostingReference::Arrear(_))
            }
            LedgerAccountCode::LoanPrincipalLiability => {
                matches!(reference, LeasePostingReference::LoanContract(_))
            }
            _ => matches!(reference, LeasePostingReference::None),
        };
        ensure!(
            valid_reference,
            "lease ledger posting has invalid ownership"
        );
        let posting_order = u16::try_from(index + 1).context("too many lease ledger postings")?;
        sqlx::query(
            "INSERT INTO ledger_posting
                 (save_id, run_revision, ledger_transaction_id, posting_order,
                  account_code, financial_account_id, lease_contract_id,
                  loan_contract_id, lease_rent_charge_id, lease_arrear_id, amount_krw)
             VALUES (?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?)",
        )
        .bind(policy.run.save_id.get())
        .bind(policy.run.run_revision)
        .bind(transaction_id)
        .bind(posting_order)
        .bind(to_db_str(&posting.account_code)?)
        .bind(lease_contract_id)
        .bind(loan_contract_id)
        .bind(lease_rent_charge_id)
        .bind(lease_arrear_id)
        .bind(posting.amount_krw)
        .execute(&mut **tx)
        .await?;
    }
    Ok(transaction_id)
}

async fn update_save_after_lease_move(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedLeaseSaveRow,
    committed_state_revision: u64,
    cash_krw: i64,
    debt_krw: i64,
) -> Result<()> {
    let updated = sqlx::query(
        "UPDATE save SET state_revision = ?, cash_krw = ?, debt_krw = ?
         WHERE id = ? AND market_world_id = ? AND policy_set_id = ?
           AND run_revision = ? AND state_revision = ? AND game_day = ?
           AND cash_krw = ? AND debt_krw = ?",
    )
    .bind(committed_state_revision)
    .bind(cash_krw)
    .bind(debt_krw)
    .bind(current.id)
    .bind(current.market_world_id)
    .bind(current.policy_set_id)
    .bind(current.run_revision)
    .bind(current.state_revision)
    .bind(current.game_day)
    .bind(current.cash_krw)
    .bind(current.debt_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "housing lease command cursor changed"
    );
    Ok(())
}

async fn update_save_after_lease_arrear_payment(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedLeaseSaveRow,
    committed_state_revision: u64,
    cash_krw: i64,
    debt_krw: i64,
) -> Result<()> {
    let updated = sqlx::query(
        "UPDATE save SET state_revision = ?, cash_krw = ?, debt_krw = ?
         WHERE id = ? AND market_world_id = ? AND policy_set_id = ?
           AND run_revision = ? AND state_revision = ? AND game_day = ?
           AND cash_krw = ? AND debt_krw = ?",
    )
    .bind(committed_state_revision)
    .bind(cash_krw)
    .bind(debt_krw)
    .bind(current.id)
    .bind(current.market_world_id)
    .bind(current.policy_set_id)
    .bind(current.run_revision)
    .bind(current.state_revision)
    .bind(current.game_day)
    .bind(current.cash_krw)
    .bind(current.debt_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "lease-arrear payment cursor changed"
    );
    Ok(())
}

async fn read_current_living_cost_pin(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    market_date: Date,
) -> Result<Option<LivingCostPinRow>> {
    let year_month = Date::from_calendar_date(market_date.year(), market_date.month(), 1)
        .context("living-cost pin month is invalid")?;
    sqlx::query_as(
        "SELECT id, household_id, residence_id, `year_month`, region_key, tenure_type,
                household_fingerprint_sha256, proration_scale, proration_units,
                days_in_month, status
         FROM living_cost_month
         WHERE save_id = ? AND run_revision = ? AND `year_month` = ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(year_month)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to pin the current living-cost month")
}

async fn read_lease_deposit_ledger_balance(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<i64> {
    let value: String = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(amount_krw), 0) AS CHAR)
         FROM ledger_posting
         WHERE save_id = ? AND run_revision = ?
           AND account_code = 'leaseDepositAsset'",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await?;
    value
        .parse::<i64>()
        .context("lease deposit ledger balance is out of range")
}

fn map_active_lease(
    row: LeaseProjectionRow,
    residence: &ResidenceProjectionRow,
) -> Result<ActiveHousingLeaseState> {
    validate_lease_listing_projection(&row)?;
    let renewal_rule = validate_lease_contract_lifecycle_terms(&row)?;
    ensure!(
        row.household_id == residence.household_id
            && row.role == "tenant"
            && row.deposit_krw > 0
            && row.effective_to_game_day.is_none(),
        "active tenant lease has invalid common terms"
    );
    let (offer_kind, monthly_rent_krw, next_rent_due_game_day) = match row.offer_kind.as_str() {
        "jeonse" => {
            ensure!(
                row.monthly_rent_krw.is_none()
                    && row.rent_charge_rule.is_none()
                    && row.arrear_repayment_rule.is_none()
                    && row.next_rent_due_game_day.is_none(),
                "active cash-jeonse lease has monthly-rent terms"
            );
            (HousingLeaseOfferKind::Jeonse, None, None)
        }
        "monthlyRent" => {
            ensure!(
                row.monthly_rent_krw.is_some_and(|amount| amount > 0)
                    && row.rent_charge_rule.as_deref() == Some("nextMonthStartFull")
                    && row.arrear_repayment_rule.as_deref() == Some("manualOnly")
                    && row.next_rent_due_game_day.is_some(),
                "active monthly-rent lease has invalid terms"
            );
            (
                HousingLeaseOfferKind::MonthlyRent,
                row.monthly_rent_krw,
                row.next_rent_due_game_day,
            )
        }
        _ => bail!("active tenant lease has an unknown offer kind"),
    };
    Ok(ActiveHousingLeaseState {
        id: ResourceId::from_u64(row.id),
        listing_id: ResourceId::from_u64(row.property_listing_id),
        role: HousingLeaseRole::Tenant,
        offer_kind,
        region_key: LifeRegionKey::from_str(&row.region_key)
            .context("active tenant lease has an unknown region")?,
        property_type: PropertyType::from_str(&row.property_type)
            .context("active tenant lease has an unknown property type")?,
        exclusive_area_square_meters: row.exclusive_area_square_meters,
        deposit_krw: row.deposit_krw,
        monthly_rent_krw,
        next_rent_due_game_day,
        effective_from_game_day: row.effective_from_game_day,
        effective_to_game_day: row.effective_to_game_day,
        renewal_rule,
        current_term: None,
        renewal_notice: None,
        termination_review: None,
        deposit_loan_id: row.deposit_loan_id.map(ResourceId::from_u64),
    })
}

fn validate_lease_contract_lifecycle_terms(
    row: &LeaseProjectionRow,
) -> Result<HousingLeaseRenewalRule> {
    match row.renewal_rule.as_str() {
        "openEnded" => {
            ensure!(
                row.term_months.is_none()
                    && row.renewal_notice_lead_days.is_none()
                    && row.termination_review_rule.is_none()
                    && row.termination_review_after_days.is_none(),
                "open-ended tenant lease has lifecycle terms"
            );
            Ok(HousingLeaseRenewalRule::OpenEnded)
        }
        "fixedTermAutoRenew" => {
            ensure!(
                row.term_months == Some(12) && row.renewal_notice_lead_days == Some(30),
                "fixed-term tenant lease has unsupported term settings"
            );
            match row.offer_kind.as_str() {
                "jeonse" => ensure!(
                    row.termination_review_rule.is_none()
                        && row.termination_review_after_days.is_none(),
                    "fixed-term cash-jeonse lease has termination-review settings"
                ),
                "monthlyRent" => ensure!(
                    row.termination_review_rule.as_deref() == Some("oldestActiveArrearAge")
                        && row.termination_review_after_days == Some(60),
                    "fixed-term monthly-rent lease has unsupported review settings"
                ),
                _ => bail!("fixed-term tenant lease has an unknown offer kind"),
            }
            Ok(HousingLeaseRenewalRule::FixedTermAutoRenew)
        }
        _ => bail!("tenant lease has an unknown renewal rule"),
    }
}

async fn read_active_lease_lifecycle_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
    lease: &mut ActiveHousingLeaseState,
) -> Result<()> {
    if lease.renewal_rule == HousingLeaseRenewalRule::OpenEnded {
        let lifecycle_count: i64 = sqlx::query_scalar(
            "SELECT
                 (SELECT COUNT(*) FROM lease_contract_term
                  WHERE save_id = ? AND run_revision = ? AND lease_contract_id = ?)
               + (SELECT COUNT(*) FROM lease_lifecycle_action
                  WHERE save_id = ? AND run_revision = ? AND lease_contract_id = ?)
               + (SELECT COUNT(*) FROM lease_termination_review
                  WHERE save_id = ? AND run_revision = ? AND lease_contract_id = ?)",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(lease.id.get())
        .bind(save_id)
        .bind(run_revision)
        .bind(lease.id.get())
        .bind(save_id)
        .bind(run_revision)
        .bind(lease.id.get())
        .fetch_one(&mut **tx)
        .await?;
        ensure!(
            lifecycle_count == 0,
            "open-ended tenant lease has lifecycle history"
        );
        return Ok(());
    }

    let terms: Vec<ActiveLeaseTermRow> = sqlx::query_as(
        "SELECT id, term_no, effective_from_game_day, effective_to_game_day
         FROM lease_contract_term
         WHERE save_id = ? AND run_revision = ? AND lease_contract_id = ?
           AND status = 'active'
         ORDER BY term_no, id",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(lease.id.get())
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        terms.len() == 1,
        "fixed-term tenant lease has no unique active term"
    );
    let term = &terms[0];
    ensure!(
        term.id > 0
            && term.term_no > 0
            && term.effective_from_game_day <= game_day
            && game_day < term.effective_to_game_day,
        "active lease term does not contain the current game day"
    );
    let actions: Vec<LeaseTermActionRow> = sqlx::query_as(
        "SELECT action_kind, phase_rank, due_game_day, source_kind,
                source_id, occurrence, status, applied_game_day
         FROM lease_lifecycle_action
         WHERE save_id = ? AND run_revision = ? AND lease_contract_id = ?
           AND lease_contract_term_id = ?
           AND action_kind IN ('renewalNotice', 'termRenewal')
         ORDER BY phase_rank, id",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(lease.id.get())
    .bind(term.id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        actions.len() == 2,
        "active lease term has an incomplete lifecycle schedule"
    );
    let notice = actions
        .iter()
        .find(|action| action.action_kind == "renewalNotice")
        .context("active lease term has no renewal notice")?;
    let renewal = actions
        .iter()
        .find(|action| action.action_kind == "termRenewal")
        .context("active lease term has no renewal action")?;
    let notice_game_day = term
        .effective_to_game_day
        .checked_sub(30)
        .context("active lease term is shorter than its notice lead")?;
    ensure!(
        notice.phase_rank == 500
            && notice.due_game_day == notice_game_day
            && notice.source_kind == "leaseTerm"
            && notice.source_id == term.id
            && notice.occurrence == term.term_no
            && renewal.phase_rank == 600
            && renewal.due_game_day == term.effective_to_game_day
            && renewal.source_kind == "leaseTerm"
            && renewal.source_id == term.id
            && renewal.occurrence == term.term_no
            && renewal.status == "pending"
            && renewal.applied_game_day.is_none(),
        "active lease term lifecycle schedule is not canonical"
    );
    let renewal_notice = match notice.status.as_str() {
        "pending" => {
            ensure!(
                game_day < notice_game_day && notice.applied_game_day.is_none(),
                "due renewal notice was not applied"
            );
            None
        }
        "applied" => {
            ensure!(
                game_day >= notice_game_day && notice.applied_game_day == Some(notice_game_day),
                "applied renewal notice has an invalid publication day"
            );
            Some(LeaseRenewalNoticeState {
                term_no: term.term_no,
                published_game_day: notice_game_day,
                renews_on_game_day: term.effective_to_game_day,
            })
        }
        _ => bail!("active lease term has a cancelled renewal notice"),
    };
    lease.current_term = Some(ActiveLeaseTermState {
        term_no: term.term_no,
        effective_from_game_day: term.effective_from_game_day,
        effective_to_game_day: term.effective_to_game_day,
    });
    lease.renewal_notice = renewal_notice;

    let reviews: Vec<OpenLeaseTerminationReviewRow> = sqlx::query_as(
        "SELECT review.opened_game_day, review.trigger_lease_arrear_id,
                CAST((
                    SELECT COALESCE(SUM(active_arrear.remaining_krw), 0)
                    FROM lease_arrear AS active_arrear
                    WHERE active_arrear.save_id = review.save_id
                      AND active_arrear.run_revision = review.run_revision
                      AND active_arrear.lease_contract_id = review.lease_contract_id
                      AND active_arrear.status = 'active'
                ) AS SIGNED) AS active_lease_arrear_krw
         FROM lease_termination_review AS review
         INNER JOIN lease_lifecycle_action AS trigger_action
           ON trigger_action.id = review.trigger_lease_lifecycle_action_id
          AND trigger_action.save_id = review.save_id
          AND trigger_action.run_revision = review.run_revision
          AND trigger_action.lease_contract_id = review.lease_contract_id
          AND trigger_action.lease_arrear_id = review.trigger_lease_arrear_id
          AND trigger_action.action_kind = 'terminationReview'
          AND trigger_action.status = 'applied'
         WHERE review.save_id = ? AND review.run_revision = ?
           AND review.lease_contract_id = ? AND review.status = 'open'
         ORDER BY review.id",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(lease.id.get())
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        reviews.len() <= 1,
        "tenant lease has multiple open termination reviews"
    );
    match (lease.offer_kind, reviews.into_iter().next()) {
        (HousingLeaseOfferKind::Jeonse, None) => {}
        (HousingLeaseOfferKind::Jeonse, Some(_)) => {
            bail!("cash-jeonse lease has a termination review")
        }
        (HousingLeaseOfferKind::MonthlyRent, Some(review)) => {
            ensure!(
                review.trigger_lease_arrear_id > 0
                    && review.opened_game_day <= game_day
                    && review.active_lease_arrear_krw > 0,
                "open lease termination review has invalid arrears"
            );
            lease.termination_review = Some(LeaseTerminationReviewState {
                status: LeaseTerminationReviewStatusState::UnderReview,
                opened_game_day: review.opened_game_day,
                trigger_arrear_id: ResourceId::from_u64(review.trigger_lease_arrear_id),
                active_lease_arrear_krw: review.active_lease_arrear_krw,
            });
        }
        (HousingLeaseOfferKind::MonthlyRent, None) => {}
    }
    Ok(())
}

fn validate_lease_listing_projection(row: &LeaseProjectionRow) -> Result<()> {
    ensure!(
        row.real_estate_model_version_id == row.listing_model_version_id
            && row.listing_market_world_id == row.save_market_world_id
            && row.region_key == row.listing_region_key
            && row.property_type == row.listing_property_type
            && row.exclusive_area_square_meters == row.listing_exclusive_area_square_meters
            && row.deposit_krw == row.listing_deposit_krw.unwrap_or_default()
            && row.monthly_rent_krw == row.listing_monthly_rent_krw,
        "tenant lease has drifted from its immutable listing"
    );
    Ok(())
}

async fn read_stored_lease_receipt(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    command_id: &str,
) -> Result<StoredLeaseReceiptRow> {
    sqlx::query_as(
        "SELECT command_kind, payload_sha256, run_revision, state_revision, game_day,
                CAST(result AS CHAR) AS result_json, ledger_transaction_id
         FROM command_receipt
         WHERE save_id = ? AND command_id = ?
         FOR SHARE",
    )
    .bind(save_id)
    .bind(command_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("housing lease command identity has no final receipt")
}

async fn validate_replayed_receipt(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedLeaseSaveRow,
    command: &StartHousingLeaseCommand,
    fingerprint: &str,
    row: &StoredLeaseReceiptRow,
    receipt: &HousingLeaseMoveReceipt,
) -> Result<()> {
    let expected_state_revision = command
        .cursor
        .expected_state_revision
        .checked_add(1)
        .context("stored housing lease state revision overflowed")?;
    ensure!(
        row.command_kind == COMMAND_KIND_START_LEASE
            && row.payload_sha256 == fingerprint
            && row.run_revision == command.cursor.expected_run_revision
            && row.state_revision == expected_state_revision
            && row.game_day == command.cursor.expected_game_day
            && row.ledger_transaction_id.is_some()
            && receipt.command_id == command.command_id
            && receipt.listing_id == command.listing_id
            && receipt.offer_kind == command.offer_kind
            && receipt
                .deposit_loan_execution
                .as_ref()
                .map(|execution| execution.quote_id)
                == command.loan_quote_id
            && receipt.effective_from_game_day == command.cursor.expected_game_day
            && receipt.deposit_krw > 0
            && match receipt.offer_kind {
                HousingLeaseOfferKind::Jeonse => receipt.monthly_rent_krw.is_none(),
                HousingLeaseOfferKind::MonthlyRent => {
                    receipt.monthly_rent_krw.is_some_and(|amount| amount > 0)
                }
            }
            && receipt.moving_cost_krw > 0
            && !receipt.replayed,
        "stored housing lease receipt disagrees with its command"
    );
    let valid: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1
             FROM lease_contract AS lease
             INNER JOIN residence
               ON residence.save_id = lease.save_id
              AND residence.run_revision = lease.run_revision
              AND residence.lease_contract_id = lease.id
             WHERE lease.save_id = ? AND lease.run_revision = ?
               AND lease.id = ? AND lease.property_listing_id = ?
               AND lease.command_id = ? AND lease.renewal_rule = ?
               AND residence.id = ?
         )",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(receipt.lease_id.get())
    .bind(receipt.listing_id.get())
    .bind(command.command_id.as_str())
    .bind(match receipt.renewal_rule {
        HousingLeaseRenewalRule::OpenEnded => "openEnded",
        HousingLeaseRenewalRule::FixedTermAutoRenew => "fixedTermAutoRenew",
    })
    .bind(receipt.residence_id.get())
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        valid,
        "stored housing lease receipt lost its current-run resources"
    );
    let rent_charge_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM lease_rent_charge
         WHERE save_id = ? AND run_revision = ? AND lease_contract_id = ?",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(receipt.lease_id.get())
    .fetch_one(&mut **tx)
    .await?;
    match receipt.offer_kind {
        HousingLeaseOfferKind::Jeonse => ensure!(
            rent_charge_count == 0,
            "stored cash-jeonse receipt has a rent charge"
        ),
        HousingLeaseOfferKind::MonthlyRent => ensure!(
            rent_charge_count > 0,
            "stored monthly-rent receipt lost its charge history"
        ),
    }
    if let Some(ended_lease_id) = receipt.ended_lease_id {
        let owned: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM lease_contract
                 WHERE save_id = ? AND run_revision = ? AND id = ?
             )",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(ended_lease_id.get())
        .fetch_one(&mut **tx)
        .await?;
        ensure!(
            owned,
            "stored housing lease receipt exposes another run's lease"
        );
    }
    let ledger_transaction_id = row
        .ledger_transaction_id
        .context("stored housing lease receipt has no ledger")?;
    let ledger_valid: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM ledger_transaction
             WHERE id = ? AND save_id = ? AND run_revision = ?
               AND source_kind = 'leaseMove' AND BINARY source_id = BINARY ?
               AND game_day = ?
         )",
    )
    .bind(ledger_transaction_id)
    .bind(current.id)
    .bind(current.run_revision)
    .bind(command.command_id.as_str())
    .bind(command.cursor.expected_game_day)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(ledger_valid, "stored housing lease receipt lost its ledger");

    let executed_loan_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM loan_contract
         WHERE save_id = ? AND run_revision = ?
           AND origin_kind = 'leaseDepositExecution'
           AND BINARY origin_command_id = BINARY ?",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(command.command_id.as_str())
    .fetch_one(&mut **tx)
    .await?;
    match &receipt.deposit_loan_execution {
        Some(execution) => {
            let execution_valid: bool = sqlx::query_scalar(
                "SELECT EXISTS(
                     SELECT 1 FROM loan_contract
                     WHERE id = ? AND save_id = ? AND run_revision = ?
                       AND household_id = (SELECT household_id FROM lease_contract WHERE id = ?)
                       AND loan_product_version_id = ? AND loan_quote_id = ?
                       AND lease_contract_id = ?
                       AND origin_kind = 'leaseDepositExecution'
                       AND BINARY origin_command_id = BINARY ?
                       AND product_kind = 'leaseDepositLoan'
                       AND original_principal_krw = ?
                       AND fixed_annual_rate_bp = ? AND maturity_game_day = ?
                 )",
            )
            .bind(execution.loan_id.get())
            .bind(current.id)
            .bind(current.run_revision)
            .bind(receipt.lease_id.get())
            .bind(execution.product_version_id.get())
            .bind(execution.quote_id.get())
            .bind(receipt.lease_id.get())
            .bind(command.command_id.as_str())
            .bind(execution.principal_krw)
            .bind(
                u16::try_from(execution.annual_rate_bp)
                    .context("stored lease-deposit execution rate is invalid")?,
            )
            .bind(execution.maturity_game_day)
            .fetch_one(&mut **tx)
            .await?;
            ensure!(
                executed_loan_count == 1 && execution_valid,
                "stored housing lease receipt lost its deposit loan"
            );
        }
        None => ensure!(
            executed_loan_count == 0,
            "cash housing lease receipt has an executed deposit loan"
        ),
    }

    let payoff_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM loan_payment
         WHERE save_id = ? AND run_revision = ?
           AND payment_kind = 'leaseMovePayoff' AND BINARY command_id = BINARY ?",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(command.command_id.as_str())
    .fetch_one(&mut **tx)
    .await?;
    match &receipt.repaid_deposit_loan {
        Some(payoff) => {
            let ended_lease_id = receipt
                .ended_lease_id
                .context("stored lease payoff has no ended lease")?;
            let payoff_valid: bool = sqlx::query_scalar(
                "SELECT EXISTS(
                     SELECT 1
                     FROM loan_payment AS payment
                     INNER JOIN loan_payment_allocation AS allocation
                       ON allocation.loan_payment_id = payment.id
                      AND allocation.loan_contract_id = payment.loan_contract_id
                     INNER JOIN loan_contract AS contract
                       ON contract.id = payment.loan_contract_id
                      AND contract.save_id = payment.save_id
                      AND contract.run_revision = payment.run_revision
                     WHERE payment.id = ? AND payment.save_id = ?
                       AND payment.run_revision = ? AND payment.loan_contract_id = ?
                       AND payment.payment_kind = 'leaseMovePayoff'
                       AND payment.amount_krw = ? AND payment.game_day = ?
                       AND BINARY payment.command_id = BINARY ?
                       AND payment.status = 'applied' AND payment.ledger_transaction_id = ?
                       AND allocation.allocation_order = 1
                       AND allocation.allocation_kind = 'prepaymentPrincipal'
                       AND allocation.amount_krw = payment.amount_krw
                       AND contract.product_kind = 'leaseDepositLoan'
                       AND contract.lease_contract_id = ? AND contract.status = 'paidOff'
                       AND contract.remaining_principal_krw = 0
                 )",
            )
            .bind(payoff.payment_id.get())
            .bind(current.id)
            .bind(current.run_revision)
            .bind(payoff.loan_id.get())
            .bind(payoff.principal_krw)
            .bind(command.cursor.expected_game_day)
            .bind(command.command_id.as_str())
            .bind(ledger_transaction_id)
            .bind(ended_lease_id.get())
            .fetch_one(&mut **tx)
            .await?;
            ensure!(
                payoff_count == 1 && payoff_valid,
                "stored housing lease receipt lost its deposit-loan payoff"
            );
        }
        None => ensure!(
            payoff_count == 0,
            "stored housing lease receipt omitted its deposit-loan payoff"
        ),
    }
    Ok(())
}

fn has_cursor(current: &LockedLeaseSaveRow, cursor: crate::finance::CommandCursor) -> bool {
    cursor.expected_run_revision == current.run_revision
        && cursor.expected_state_revision == current.state_revision
        && cursor.expected_game_day == current.game_day
}

fn start_lease_fingerprint(command: &StartHousingLeaseCommand) -> String {
    let offer_kind = match command.offer_kind {
        HousingLeaseOfferKind::Jeonse => "jeonse",
        HousingLeaseOfferKind::MonthlyRent => "monthlyRent",
    };
    let canonical = match command.loan_quote_id {
        Some(quote_id) => format!(
            concat!(
                "lifeledger.life.startLease.v2\n",
                "expectedRunRevision={}\n",
                "expectedStateRevision={}\n",
                "expectedGameDay={}\n",
                "listingId={}\n",
                "offerKind={}\n",
                "loanQuoteId={}"
            ),
            command.cursor.expected_run_revision,
            command.cursor.expected_state_revision,
            command.cursor.expected_game_day,
            command.listing_id.get(),
            offer_kind,
            quote_id.get(),
        ),
        None => format!(
            concat!(
                "lifeledger.life.startLease.v1\n",
                "expectedRunRevision={}\n",
                "expectedStateRevision={}\n",
                "expectedGameDay={}\n",
                "listingId={}\n",
                "offerKind={}"
            ),
            command.cursor.expected_run_revision,
            command.cursor.expected_state_revision,
            command.cursor.expected_game_day,
            command.listing_id.get(),
            offer_kind,
        ),
    };
    let digest = Sha256::digest(canonical.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn pay_lease_arrear_fingerprint(command: &PayLeaseArrearCommand) -> String {
    let canonical = format!(
        concat!(
            "lifeledger.life.payLeaseArrear.v1\n",
            "expectedRunRevision={}\n",
            "expectedStateRevision={}\n",
            "expectedGameDay={}\n",
            "leaseArrearId={}\n",
            "amountKrw={}"
        ),
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.arrear_id.get(),
        command.amount_krw,
    );
    let digest = Sha256::digest(canonical.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn month_start(date: Date) -> Result<Date> {
    date.replace_day(1)
        .context("monthly-rent market date has no month start")
}

fn year_month_date(year_month: YearMonth) -> Result<Date> {
    ensure!(year_month.is_valid(), "monthly-rent year-month is invalid");
    Date::from_calendar_date(
        year_month.year,
        Month::try_from(year_month.month).context("monthly-rent month is invalid")?,
        1,
    )
    .context("monthly-rent year-month is out of range")
}

fn to_year_month(date: Date) -> Result<YearMonth> {
    let year_month = YearMonth {
        year: date.year(),
        month: u8::from(date.month()),
    };
    ensure!(
        year_month.is_valid(),
        "stored monthly-rent month is invalid"
    );
    Ok(year_month)
}

fn to_db_str<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value)? {
        serde_json::Value::String(value) => Ok(value),
        _ => bail!("database enum did not serialize as a string"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finance::{
        CommandCursor, CommandId, RunId, ScheduledSettlement, SettlementKind, SettlementSource,
        SettlementSourceKind, SettlementStatus,
    };

    fn given_command() -> StartHousingLeaseCommand {
        StartHousingLeaseCommand {
            command_id: CommandId::parse("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2")
                .expect("표준 UUID여야 한다"),
            cursor: CommandCursor {
                expected_run_revision: 3,
                expected_state_revision: 7,
                expected_game_day: 31,
            },
            listing_id: ResourceId::from_u64(123),
            offer_kind: HousingLeaseOfferKind::Jeonse,
            loan_quote_id: None,
        }
    }

    fn given_lease_arrear_payment_command() -> PayLeaseArrearCommand {
        PayLeaseArrearCommand {
            command_id: CommandId::parse("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2")
                .expect("표준 UUID여야 한다"),
            cursor: CommandCursor {
                expected_run_revision: 3,
                expected_state_revision: 7,
                expected_game_day: 31,
            },
            arrear_id: ResourceId::from_u64(19),
            amount_krw: 10_000,
        }
    }

    fn given_lease_rent_settlement() -> ScheduledSettlement {
        ScheduledSettlement {
            id: ResourceId::from_u64(41),
            run: RunId {
                save_id: ResourceId::from_u64(3),
                run_revision: 2,
            },
            due_game_day: 62,
            kind: SettlementKind::LeaseRent,
            source: SettlementSource {
                kind: SettlementSourceKind::LeaseContract,
                source_id: "17".to_owned(),
                occurrence: 4,
            },
            status: SettlementStatus::Pending,
            payload: serde_json::json!({
                "version": 1,
                "leaseContractId": "17",
                "rentChargeId": "29",
                "chargeNo": 4,
            }),
        }
    }

    mod context_전세_시작_명령의_fingerprint를_만들_때 {
        use super::*;

        #[test]
        fn given_canonical_decimal_listing_when_fingerprint하면_then_안정된_hash를_만든다() {
            let command = given_command();

            let fingerprint = start_lease_fingerprint(&command);

            assert_eq!(
                fingerprint,
                "1bd60f37e3a6b3998bb6ebbd47e1742fb566be62e0417782be71ca26e414f635"
            );
        }

        #[test]
        fn given_다른_listing_when_fingerprint하면_then_hash가_달라진다() {
            let command = given_command();
            let mut other = given_command();
            other.listing_id = ResourceId::from_u64(124);

            let fingerprint = start_lease_fingerprint(&command);
            let other_fingerprint = start_lease_fingerprint(&other);

            assert_ne!(fingerprint, other_fingerprint);
        }

        #[test]
        fn given_같은_listing의_월세_when_fingerprint하면_then_전세와_hash가_다르다() {
            let jeonse = given_command();
            let mut monthly_rent = given_command();
            monthly_rent.offer_kind = HousingLeaseOfferKind::MonthlyRent;

            let jeonse_fingerprint = start_lease_fingerprint(&jeonse);
            let monthly_rent_fingerprint = start_lease_fingerprint(&monthly_rent);

            assert_ne!(jeonse_fingerprint, monthly_rent_fingerprint);
        }
    }

    mod context_월세_연체_상환_명령의_fingerprint를_만들_때 {
        use super::*;

        #[test]
        fn given_path와_amount와_cursor_when_fingerprint하면_then_안정된_hash를_만든다() {
            let command = given_lease_arrear_payment_command();

            let fingerprint = pay_lease_arrear_fingerprint(&command);

            assert_eq!(
                fingerprint,
                "ee7b56f941c9f9379f01231c239434fbb0ea9b711050943e9ace61d032689fbf"
            );
        }

        #[test]
        fn given_다른_amount_when_fingerprint하면_then_hash가_달라진다() {
            let command = given_lease_arrear_payment_command();
            let mut changed = given_lease_arrear_payment_command();
            changed.amount_krw += 1;

            let fingerprint = pay_lease_arrear_fingerprint(&command);
            let changed_fingerprint = pay_lease_arrear_fingerprint(&changed);

            assert_ne!(fingerprint, changed_fingerprint);
        }
    }

    mod context_세입자_이사_경계를_판단할_때 {
        use super::*;

        #[test]
        fn given_owner_residence_when_판단하면_then_계약충돌로_차단한다() {
            let tenure_type = "owner";
            let property_holding_id = Some(41);
            let active_holding_count = 1;

            let result = tenant_lease_boundary_conflict(
                tenure_type,
                property_holding_id,
                active_holding_count,
            );

            assert!(result);
        }

        #[test]
        fn given_tenant_residence와_활성보유_when_판단하면_then_계약충돌로_차단한다() {
            let tenure_type = "jeonse";
            let property_holding_id = None;
            let active_holding_count = 1;

            let result = tenant_lease_boundary_conflict(
                tenure_type,
                property_holding_id,
                active_holding_count,
            );

            assert!(result);
        }
    }

    mod context_월세_정산_envelope를_검증할_때 {
        use super::*;

        #[test]
        fn given_exact_payload_when_검증하면_then_수락한다() {
            let settlement = given_lease_rent_settlement();

            let result = validate_lease_rent_settlement_envelope(&settlement);

            assert!(result.is_ok());
        }

        #[test]
        fn given_알수없는_payload_field_when_검증하면_then_거절한다() {
            let mut settlement = given_lease_rent_settlement();
            settlement.payload["unexpected"] = serde_json::json!(true);

            let result = validate_lease_rent_settlement_envelope(&settlement);

            assert!(result.is_err());
        }

        #[test]
        fn given_다른_occurrence_when_검증하면_then_거절한다() {
            let mut settlement = given_lease_rent_settlement();
            settlement.source.occurrence = 5;

            let result = validate_lease_rent_settlement_envelope(&settlement);

            assert!(result.is_err());
        }
    }
}
