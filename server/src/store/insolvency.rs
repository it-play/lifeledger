//! M4-E1 cash-only insolvency persistence and bounded reads.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{AssertSqlSafe, MySql, MySqlPool, Transaction};
use time::Date;

use super::loans::{LoanPostingReference, write_loan_ledger_transaction};
use super::mysql::{
    CommandIdentitySpec, CommandIdentityState, inspect_command_identity, read_state,
    write_command_identity, write_ledger_transaction,
};
use super::types::{
    ActOnInsolvencyCaseCommand, InsolvencyActionState, InsolvencyAvailabilityState,
    InsolvencyCaseDetailState, InsolvencyCaseReceipt, InsolvencyCaseSummaryState,
    InsolvencyClaimPageState, InsolvencyClaimState, InsolvencyLiquidationPageState,
    InsolvencyLiquidationState, InsolvencyReadResult, InsolvencySnapshotState,
    InsolvencyTransitionState, InsolvencyWalletAssetState, LifeFailureCode, LifeStoreResult,
    PrepareInsolvencyCaseCommand,
};
use crate::finance::{
    FinanceRules, LedgerAccountCode, LedgerPosting, LedgerSource, LedgerSourceKind,
    LedgerTransactionDraft, ResourceId, RunId, RunPolicyContext,
};
use crate::life::{
    InsolvencyCaseStatus, InsolvencyCompositionFact, InsolvencyCompositionInput,
    InsolvencyDistributionClaimInput, InsolvencyEligibilityInput, InsolvencyEligibilityStatus,
    InsolvencyLoanPosition, InsolvencyProcedureKind, InsolvencyRules, LoanContractStatus,
    LoanProductKind, RepaymentBucketBalance, RepaymentBucketKind,
};

const COMPONENT_KEY: &str = "dev-unranked-m4-insolvency-2026-v1";
const POLICY_KEY: &str = "dev-unranked-kr-individual-insolvency-2026-v4";
const COMMAND_KIND_PREPARE: &str = "prepareInsolvencyCase";
const COMMAND_KIND_ACTION: &str = "actOnInsolvencyCase";
const MAX_TRANSACTION_ATTEMPTS: usize = 3;
const PAGE_SIZE: usize = 20;
const CURSOR_DOMAIN: &[u8] = b"lifeledger.life.insolvency-page.v1";
const CURSOR_CHECKSUM_BYTES: usize = 8;
const CURSOR_PAYLOAD_BYTES: usize = 29;

#[derive(Debug, Clone, sqlx::FromRow)]
struct ScopeRow {
    save_id: u64,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
    cash_krw: i64,
    has_character: bool,
    policy_set_id: u64,
    policy_key: String,
    life_catalog_set_id: u64,
    insolvency_component_version_id: Option<u64>,
    component_version_key: Option<String>,
    component_availability: Option<String>,
    minimum_simulation_date: Option<Date>,
    simulation_date: Date,
    policy_rule_id: Option<u64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LoanRow {
    contract_id: u64,
    product_kind: String,
    status: String,
    read_only: bool,
    remaining_principal_krw: i64,
    accrued_interest_krw: i64,
    accrued_fee_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CaseRow {
    id: u64,
    life_catalog_set_id: u64,
    policy_set_id: u64,
    insolvency_component_version_id: u64,
    procedure_kind: String,
    status: String,
    composition_sha256: String,
    prepared_game_day: u32,
    submitted_game_day: Option<u32>,
    credit_restriction_end_exclusive: Option<u32>,
    wallet_cash_krw: i64,
    automatic_protected_krw: i64,
    additional_protected_krw: i64,
    liquidatable_krw: i64,
    total_claim_krw: i64,
    claim_count: u8,
    distributed_krw: i64,
    discharged_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ClaimRow {
    id: u64,
    loan_contract_id: u64,
    principal_krw: i64,
    interest_krw: i64,
    fee_krw: i64,
    allowed_krw: i64,
    distributed_krw: i64,
    discharged_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct BucketRow {
    id: u64,
    loan_installment_id: u64,
    bucket_kind: String,
    original_amount_krw: i64,
    paid_amount_krw: i64,
    status: String,
}

#[derive(Debug)]
struct ClaimRuntime {
    claim: ClaimRow,
    buckets: Vec<BucketRow>,
    aggregated: Vec<RepaymentBucketBalance>,
}

#[derive(Debug, Clone, Copy)]
struct PageCursor {
    kind: u8,
    save_id: u64,
    run_revision: u32,
    case_id: u64,
    after_id: u64,
}

pub(super) async fn read_insolvency_overview(
    pool: &MySqlPool,
    rules: &dyn InsolvencyRules,
    user_id: u64,
) -> Result<InsolvencyReadResult<InsolvencySnapshotState>> {
    let mut tx = pool.begin().await?;
    let Some(scope) = read_scope_for_user(&mut tx, user_id, false).await? else {
        tx.commit().await?;
        return Ok(InsolvencyReadResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    if !scope.has_character {
        tx.commit().await?;
        return Ok(InsolvencyReadResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    }
    let snapshot = read_snapshot_for_scope(&mut tx, rules, &scope).await?;
    tx.commit().await?;
    Ok(InsolvencyReadResult::Found(snapshot))
}

pub(super) async fn read_insolvency_snapshot_in_tx(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn InsolvencyRules,
    save_id: u64,
) -> Result<InsolvencySnapshotState> {
    let Some(scope) = read_scope_for_save(tx, save_id).await? else {
        return Ok(InsolvencySnapshotState::unavailable());
    };
    if !scope.has_character {
        return Ok(InsolvencySnapshotState::unavailable());
    }
    read_snapshot_for_scope(tx, rules, &scope).await
}

pub(super) async fn prepare_insolvency_case(
    pool: &MySqlPool,
    rules: &dyn InsolvencyRules,
    user_id: u64,
    command: &PrepareInsolvencyCaseCommand,
) -> Result<LifeStoreResult<InsolvencyCaseReceipt>> {
    for _ in 0..MAX_TRANSACTION_ATTEMPTS {
        match prepare_insolvency_case_once(pool, rules, user_id, command).await {
            Ok(result) => return Ok(result),
            Err(error) if super::housing::is_retryable_database_error(&error) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy))
}

async fn prepare_insolvency_case_once(
    pool: &MySqlPool,
    rules: &dyn InsolvencyRules,
    user_id: u64,
    command: &PrepareInsolvencyCaseCommand,
) -> Result<LifeStoreResult<InsolvencyCaseReceipt>> {
    let fingerprint = prepare_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(scope) = read_scope_for_user(&mut tx, user_id, true).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_PREPARE,
        payload_sha256: &fingerprint,
        cursor: command.cursor,
    };
    if let Some(replay) = inspect_replay(&mut tx, &scope, &identity).await? {
        return finish_replay(tx, scope.save_id, replay).await;
    }
    if !scope.has_character {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    }
    if command.procedure_kind != InsolvencyProcedureKind::CashOnlyLiquidation {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::InvalidCommand));
    }
    if !has_current_cursor(&scope, command.cursor) {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::InsolvencyStateConflict,
        ));
    }

    let loans = read_loan_rows(&mut tx, &scope, true).await?;
    let (assessment, positions, unsupported_assets, unsupported_obligations, has_lien) =
        assess_scope(&mut tx, rules, &scope, &loans, true).await?;
    match assessment.status {
        InsolvencyEligibilityStatus::Eligible => {}
        InsolvencyEligibilityStatus::CompositionUnsupported => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(
                LifeFailureCode::InsolvencyCompositionUnsupported,
            ));
        }
        InsolvencyEligibilityStatus::Unavailable => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(LifeFailureCode::RateUnavailable));
        }
        InsolvencyEligibilityStatus::Ineligible => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(LifeFailureCode::Ineligible));
        }
    }
    let protection = rules
        .calculate_cash_protection(crate::life::InsolvencyCashProtectionInput {
            wallet_cash_krw: scope.cash_krw,
            policy: rules.policy_terms(),
        })
        .context("insolvency cash protection failed")?;
    let composition_sha256 = composition_hash(
        rules,
        scope.cash_krw,
        &positions,
        unsupported_assets,
        unsupported_obligations,
        has_lien,
    )?;
    let policy_rule_id = scope
        .policy_rule_id
        .context("active insolvency policy has no rule")?;
    let component_version_id = scope
        .insolvency_component_version_id
        .context("active insolvency catalog has no component")?;

    write_command_identity(&mut tx, scope.save_id, &identity).await?;
    let inserted = sqlx::query(
        "INSERT INTO insolvency_case
             (save_id, run_revision, life_catalog_set_id, policy_set_id,
              insolvency_component_version_id, procedure_kind, status,
              prepared_command_id, composition_sha256,
              automatic_cash_protection_rule_id, additional_exemption_rule_id,
              prepared_game_day, wallet_cash_krw, automatic_protected_krw,
              additional_protected_krw, liquidatable_krw, total_claim_krw, claim_count)
         VALUES (?, ?, ?, ?, ?, 'cashOnlyLiquidation', 'prepared', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.life_catalog_set_id)
    .bind(scope.policy_set_id)
    .bind(component_version_id)
    .bind(command.command_id.as_str())
    .bind(&composition_sha256)
    .bind(policy_rule_id)
    .bind(policy_rule_id)
    .bind(scope.game_day)
    .bind(scope.cash_krw)
    .bind(protection.automatic_protected_krw)
    .bind(protection.additional_protected_krw)
    .bind(protection.liquidatable_krw)
    .bind(assessment.total_supported_claim_krw)
    .bind(assessment.supported_claim_count)
    .execute(&mut *tx)
    .await?;
    let case_id = inserted.last_insert_id();
    insert_transition(
        &mut tx,
        &scope,
        case_id,
        1,
        None,
        "prepared",
        Some(command.command_id.as_str()),
        scope.game_day,
        "playerPrepared",
    )
    .await?;
    sqlx::query(
        "INSERT INTO insolvency_asset
             (save_id, run_revision, case_id, life_catalog_set_id, policy_set_id,
              insolvency_component_version_id, asset_kind, authority_key,
              original_amount_krw, automatic_protected_krw,
              additional_protected_krw, liquidatable_krw)
         VALUES (?, ?, ?, ?, ?, ?, 'wallet', 'save.walletCashKrw', ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(case_id)
    .bind(scope.life_catalog_set_id)
    .bind(scope.policy_set_id)
    .bind(component_version_id)
    .bind(scope.cash_krw)
    .bind(protection.automatic_protected_krw)
    .bind(protection.additional_protected_krw)
    .bind(protection.liquidatable_krw)
    .execute(&mut *tx)
    .await?;
    for loan in &positions {
        let allowed = loan
            .remaining_principal_krw
            .checked_add(loan.accrued_interest_krw)
            .and_then(|value| value.checked_add(loan.accrued_fee_krw))
            .context("insolvency claim overflowed")?;
        if allowed == 0 {
            continue;
        }
        sqlx::query(
            "INSERT INTO insolvency_claim
                 (save_id, run_revision, case_id, life_catalog_set_id, policy_set_id,
                  insolvency_component_version_id, loan_contract_id, claim_class,
                  principal_krw, interest_krw, fee_krw, allowed_krw)
             VALUES (?, ?, ?, ?, ?, ?, ?, 'generalUnsecured', ?, ?, ?, ?)",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(case_id)
        .bind(scope.life_catalog_set_id)
        .bind(scope.policy_set_id)
        .bind(component_version_id)
        .bind(loan.contract_id.get())
        .bind(loan.remaining_principal_krw)
        .bind(loan.accrued_interest_krw)
        .bind(loan.accrued_fee_krw)
        .bind(allowed)
        .execute(&mut *tx)
        .await?;
    }
    let changed = advance_state_revision(&mut tx, &scope).await?;
    let case = read_case_by_id(&mut tx, &scope, case_id, false)
        .await?
        .context("prepared insolvency case disappeared")?;
    let receipt = InsolvencyCaseReceipt {
        command_id: command.command_id.clone(),
        case: case_summary(&case)?,
        replayed: false,
    };
    write_receipt(
        &mut tx,
        &scope,
        component_version_id,
        command.command_id.as_str(),
        "prepareCase",
        &fingerprint,
        case_id,
        changed,
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

pub(super) async fn act_on_insolvency_case(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    rules: &dyn InsolvencyRules,
    user_id: u64,
    command: &ActOnInsolvencyCaseCommand,
) -> Result<LifeStoreResult<InsolvencyCaseReceipt>> {
    for _ in 0..MAX_TRANSACTION_ATTEMPTS {
        match act_on_insolvency_case_once(pool, finance_rules, rules, user_id, command).await {
            Ok(result) => return Ok(result),
            Err(error) if is_composition_changed(&error) => {
                return Ok(LifeStoreResult::Rejected(
                    LifeFailureCode::InsolvencyCompositionChanged,
                ));
            }
            Err(error) if is_composition_unsupported(&error) => {
                return Ok(LifeStoreResult::Rejected(
                    LifeFailureCode::InsolvencyCompositionUnsupported,
                ));
            }
            Err(error) if super::housing::is_retryable_database_error(&error) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy))
}

async fn act_on_insolvency_case_once(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    rules: &dyn InsolvencyRules,
    user_id: u64,
    command: &ActOnInsolvencyCaseCommand,
) -> Result<LifeStoreResult<InsolvencyCaseReceipt>> {
    let fingerprint = action_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(scope) = read_scope_for_user(&mut tx, user_id, true).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_ACTION,
        payload_sha256: &fingerprint,
        cursor: command.cursor,
    };
    if let Some(replay) = inspect_replay(&mut tx, &scope, &identity).await? {
        return finish_replay(tx, scope.save_id, replay).await;
    }
    if !has_current_cursor(&scope, command.cursor) {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::InsolvencyStateConflict,
        ));
    }
    let Some(case) = read_case_by_id(&mut tx, &scope, command.case_id.get(), true).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::InsolvencyResourceNotFound,
        ));
    };
    if case.status != "prepared" {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::InsolvencyStateConflict,
        ));
    }
    write_command_identity(&mut tx, scope.save_id, &identity).await?;

    match command.action {
        InsolvencyActionState::Withdraw => {
            rules
                .plan_withdraw(InsolvencyCaseStatus::Prepared, scope.game_day)
                .context("insolvency withdrawal transition failed")?;
            sqlx::query(
                "UPDATE insolvency_case
                 SET status = 'withdrawn', terminal_game_day = ?
                 WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'prepared'",
            )
            .bind(scope.game_day)
            .bind(case.id)
            .bind(scope.save_id)
            .bind(scope.run_revision)
            .execute(&mut *tx)
            .await?;
            insert_transition(
                &mut tx,
                &scope,
                case.id,
                2,
                Some("prepared"),
                "withdrawn",
                Some(command.command_id.as_str()),
                scope.game_day,
                "playerWithdrew",
            )
            .await?;
        }
        InsolvencyActionState::Submit => {
            submit_case(&mut tx, finance_rules, rules, &scope, &case, command).await?;
        }
    }

    let changed = advance_state_revision(&mut tx, &scope).await?;
    let committed_case = read_case_by_id(&mut tx, &scope, case.id, false)
        .await?
        .context("committed insolvency case disappeared")?;
    let receipt = InsolvencyCaseReceipt {
        command_id: command.command_id.clone(),
        case: case_summary(&committed_case)?,
        replayed: false,
    };
    write_receipt(
        &mut tx,
        &scope,
        case.insolvency_component_version_id,
        command.command_id.as_str(),
        match command.action {
            InsolvencyActionState::Submit => "submitCase",
            InsolvencyActionState::Withdraw => "withdrawCase",
        },
        &fingerprint,
        case.id,
        changed,
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

async fn submit_case(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    rules: &dyn InsolvencyRules,
    scope: &ScopeRow,
    case: &CaseRow,
    command: &ActOnInsolvencyCaseCommand,
) -> Result<()> {
    let loans = read_loan_rows(tx, scope, true).await?;
    let (assessment, positions, unsupported_assets, unsupported_obligations, has_lien) =
        assess_scope(tx, rules, scope, &loans, false).await?;
    let composition = composition_hash(
        rules,
        scope.cash_krw,
        &positions,
        unsupported_assets,
        unsupported_obligations,
        has_lien,
    )?;
    if composition != case.composition_sha256 {
        return Err(anyhow::Error::new(CompositionChanged));
    }
    if assessment.status == InsolvencyEligibilityStatus::CompositionUnsupported {
        return Err(anyhow::Error::new(CompositionUnsupported));
    }
    ensure!(
        assessment.status == InsolvencyEligibilityStatus::Eligible,
        "prepared insolvency case is no longer eligible"
    );
    let protection = rules
        .calculate_cash_protection(crate::life::InsolvencyCashProtectionInput {
            wallet_cash_krw: scope.cash_krw,
            policy: rules.policy_terms(),
        })
        .context("insolvency submit protection failed")?;
    ensure!(
        protection.liquidatable_krw == case.liquidatable_krw
            && assessment.total_supported_claim_krw == case.total_claim_krw,
        "prepared insolvency totals changed without changing the composition hash"
    );

    let claim_rows: Vec<ClaimRow> = sqlx::query_as(
        "SELECT id, loan_contract_id, principal_krw, interest_krw, fee_krw,
                allowed_krw, distributed_krw, discharged_krw
         FROM insolvency_claim
         WHERE save_id = ? AND run_revision = ? AND case_id = ?
         ORDER BY loan_contract_id FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(case.id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(claim_rows.len() == usize::from(case.claim_count));
    let mut runtime_claims = Vec::with_capacity(claim_rows.len());
    for claim in claim_rows {
        let buckets: Vec<BucketRow> = sqlx::query_as(
            "SELECT id, loan_installment_id, bucket_kind,
                    original_amount_krw, paid_amount_krw, status
             FROM loan_obligation_bucket
             WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
               AND status IN ('pending', 'delinquent')
             ORDER BY due_game_day, id FOR UPDATE",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(claim.loan_contract_id)
        .fetch_all(&mut **tx)
        .await?;
        let aggregated = aggregate_buckets(&buckets)?;
        runtime_claims.push(ClaimRuntime {
            claim,
            buckets,
            aggregated,
        });
    }
    let distribution_inputs = runtime_claims
        .iter()
        .map(|runtime| InsolvencyDistributionClaimInput {
            contract_id: ResourceId::from_u64(runtime.claim.loan_contract_id),
            principal_krw: runtime.claim.principal_krw,
            interest_krw: runtime.claim.interest_krw,
            fee_krw: runtime.claim.fee_krw,
            buckets: &runtime.aggregated,
        })
        .collect::<Vec<_>>();
    let distribution = rules
        .allocate_distribution(case.liquidatable_krw, &distribution_inputs)
        .context("insolvency distribution planning failed")?;
    ensure!(
        distribution.total_claim_krw == case.total_claim_krw
            && distribution.total_distributed_krw == case.liquidatable_krw
            && distribution.total_claim_krw
                == distribution.total_distributed_krw + distribution.total_discharged_krw,
        "insolvency distribution totals are inconsistent"
    );

    for (index, planned) in distribution.claims.iter().enumerate() {
        let runtime = runtime_claims
            .iter()
            .find(|runtime| runtime.claim.loan_contract_id == planned.contract_id.get())
            .context("insolvency distribution lost its claim")?;
        let mut payment_id = None;
        if planned.distributed_krw > 0 {
            let id = apply_claim_distribution(
                tx,
                finance_rules,
                scope,
                case.id,
                runtime,
                planned,
                u8::try_from(index + 1).context("too many insolvency distributions")?,
            )
            .await?;
            payment_id = Some(id);
        }
        discharge_claim_authorities(tx, scope, runtime, planned).await?;
        let updated = sqlx::query(
            "UPDATE insolvency_claim
             SET distributed_krw = ?, discharged_krw = ?
             WHERE id = ? AND save_id = ? AND run_revision = ? AND case_id = ?
               AND distributed_krw = 0 AND discharged_krw = 0",
        )
        .bind(planned.distributed_krw)
        .bind(planned.discharged_krw)
        .bind(runtime.claim.id)
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(case.id)
        .execute(&mut **tx)
        .await?;
        ensure!(updated.rows_affected() == 1);
        let _ = payment_id;
    }

    if distribution.total_discharged_krw > 0 {
        let ledger = finance_rules.create_ledger_transaction(LedgerTransactionDraft {
            policy: policy_context(scope),
            source: LedgerSource {
                kind: LedgerSourceKind::InsolvencyDischarge,
                source_id: case.id.to_string(),
            },
            game_day: scope.game_day,
            description: "게임상 도산 면책".to_owned(),
            postings: vec![
                LedgerPosting {
                    account_code: LedgerAccountCode::InsolvencyDischargedDebt,
                    financial_account_id: None,
                    amount_krw: distribution.total_discharged_krw,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::InsolvencyDischargeGain,
                    financial_account_id: None,
                    amount_krw: distribution
                        .total_discharged_krw
                        .checked_neg()
                        .context("insolvency discharge cannot be negated")?,
                },
            ],
        })?;
        write_ledger_transaction(tx, &ledger).await?;
    }

    let principal_reduction = runtime_claims.iter().try_fold(0_i64, |total, runtime| {
        total
            .checked_add(runtime.claim.principal_krw)
            .context("insolvency principal reduction overflowed")
    })?;
    let updated_save = sqlx::query(
        "UPDATE save
         SET cash_krw = cash_krw - ?, debt_krw = debt_krw - ?
         WHERE id = ? AND run_revision = ? AND cash_krw = ? AND debt_krw >= ?",
    )
    .bind(distribution.total_distributed_krw)
    .bind(principal_reduction)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.cash_krw)
    .bind(principal_reduction)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated_save.rows_affected() == 1,
        "insolvency lost the save projection"
    );
    sqlx::query(
        "UPDATE insolvency_asset
         SET distributed_krw = liquidatable_krw
         WHERE save_id = ? AND run_revision = ? AND case_id = ? AND asset_kind = 'wallet'",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(case.id)
    .execute(&mut **tx)
    .await?;

    let submit = rules
        .plan_submit(InsolvencyCaseStatus::Prepared, scope.game_day)
        .context("insolvency submit transitions failed")?;
    for transition in &submit.transitions {
        insert_transition(
            tx,
            scope,
            case.id,
            transition.sequence + 1,
            Some(status_db(transition.from)),
            status_db(transition.to),
            Some(command.command_id.as_str()),
            transition.game_day,
            "cashOnlyLiquidation",
        )
        .await?;
    }
    let updated_case = sqlx::query(
        "UPDATE insolvency_case
         SET status = 'rebuilding', submitted_game_day = ?,
             credit_restriction_end_exclusive = ?,
             distributed_krw = ?, discharged_krw = ?
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'prepared'",
    )
    .bind(scope.game_day)
    .bind(submit.credit_restriction_end_exclusive)
    .bind(distribution.total_distributed_krw)
    .bind(distribution.total_discharged_krw)
    .bind(case.id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .execute(&mut **tx)
    .await?;
    ensure!(updated_case.rows_affected() == 1);
    Ok(())
}

async fn apply_claim_distribution(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    scope: &ScopeRow,
    case_id: u64,
    runtime: &ClaimRuntime,
    planned: &crate::life::InsolvencyClaimDistribution,
    distribution_order: u8,
) -> Result<u64> {
    let payment_no: u32 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(MAX(payment_no), 0) + 1 AS UNSIGNED)
         FROM loan_payment WHERE loan_contract_id = ?",
    )
    .bind(runtime.claim.loan_contract_id)
    .fetch_one(&mut **tx)
    .await?;
    let inserted = sqlx::query(
        "INSERT INTO loan_payment
             (save_id, run_revision, loan_contract_id, payment_no, payment_kind,
              amount_krw, game_day, command_id, insolvency_case_id, status)
         VALUES (?, ?, ?, ?, 'insolvencyDistribution', ?, ?, NULL, ?, 'prepared')",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(runtime.claim.loan_contract_id)
    .bind(payment_no)
    .bind(planned.distributed_krw)
    .bind(scope.game_day)
    .bind(case_id)
    .execute(&mut **tx)
    .await?;
    let payment_id = inserted.last_insert_id();

    let mut allocation_order = 1_u16;
    for allocation in &planned.repayment.buckets {
        let mut remaining = allocation.paid_krw;
        for bucket in runtime
            .buckets
            .iter()
            .filter(|bucket| matches!(bucket_kind(bucket), Ok(kind) if kind == allocation.kind))
        {
            if remaining == 0 {
                break;
            }
            let outstanding = bucket
                .original_amount_krw
                .checked_sub(bucket.paid_amount_krw)
                .context("loan bucket outstanding underflowed")?;
            let paid = remaining.min(outstanding);
            if paid == 0 {
                continue;
            }
            sqlx::query(
                "INSERT INTO loan_payment_allocation
                     (save_id, run_revision, loan_contract_id, loan_payment_id,
                      loan_obligation_bucket_id, allocation_order, allocation_kind, amount_krw)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(scope.save_id)
            .bind(scope.run_revision)
            .bind(runtime.claim.loan_contract_id)
            .bind(payment_id)
            .bind(bucket.id)
            .bind(allocation_order)
            .bind(bucket_kind_db(allocation.kind))
            .bind(paid)
            .execute(&mut **tx)
            .await?;
            apply_bucket_payment(tx, scope, bucket, paid).await?;
            allocation_order = allocation_order
                .checked_add(1)
                .context("insolvency allocation order overflowed")?;
            remaining = remaining
                .checked_sub(paid)
                .context("insolvency allocation underflowed")?;
        }
        ensure!(
            remaining == 0,
            "insolvency allocation exceeded stored buckets"
        );
    }

    let (principal, interest, fee) = paid_totals(&planned.repayment.buckets)?;
    let mut postings = vec![LedgerPosting {
        account_code: LedgerAccountCode::Wallet,
        financial_account_id: None,
        amount_krw: planned
            .distributed_krw
            .checked_neg()
            .context("insolvency distribution cannot be negated")?,
    }];
    let mut references = vec![LoanPostingReference::None];
    for (amount, account) in [
        (principal, LedgerAccountCode::LoanPrincipalLiability),
        (interest, LedgerAccountCode::LoanInterestExpense),
        (fee, LedgerAccountCode::LoanFeeExpense),
    ] {
        if amount > 0 {
            postings.push(LedgerPosting {
                account_code: account,
                financial_account_id: None,
                amount_krw: amount,
            });
            references.push(LoanPostingReference::Contract(
                runtime.claim.loan_contract_id,
            ));
        }
    }
    let ledger = finance_rules.create_ledger_transaction(LedgerTransactionDraft {
        policy: policy_context(scope),
        source: LedgerSource {
            kind: LedgerSourceKind::InsolvencyDistribution,
            source_id: payment_id.to_string(),
        },
        game_day: scope.game_day,
        description: "게임상 도산 청산 배분".to_owned(),
        postings,
    })?;
    let ledger_id = write_loan_ledger_transaction(tx, &ledger, &references).await?;
    sqlx::query(
        "UPDATE loan_payment SET status = 'applied', ledger_transaction_id = ?
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'prepared'",
    )
    .bind(ledger_id)
    .bind(payment_id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO insolvency_distribution
             (save_id, run_revision, case_id, claim_id, distribution_order,
              amount_krw, loan_payment_id, ledger_transaction_id, applied_game_day)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(case_id)
    .bind(runtime.claim.id)
    .bind(distribution_order)
    .bind(planned.distributed_krw)
    .bind(payment_id)
    .bind(ledger_id)
    .bind(scope.game_day)
    .execute(&mut **tx)
    .await?;
    Ok(payment_id)
}

async fn apply_bucket_payment(
    tx: &mut Transaction<'_, MySql>,
    scope: &ScopeRow,
    bucket: &BucketRow,
    paid_krw: i64,
) -> Result<()> {
    let outstanding = bucket.original_amount_krw - bucket.paid_amount_krw;
    let status = if paid_krw == outstanding {
        "paid"
    } else {
        &bucket.status
    };
    sqlx::query(
        "UPDATE loan_obligation_bucket
         SET paid_amount_krw = paid_amount_krw + ?, status = ?
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND paid_amount_krw = ? AND status = ?",
    )
    .bind(paid_krw)
    .bind(status)
    .bind(bucket.id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(bucket.paid_amount_krw)
    .bind(&bucket.status)
    .execute(&mut **tx)
    .await?;
    let column = match bucket.bucket_kind.as_str() {
        "fee" => "paid_fee_krw",
        "interest" => "paid_interest_krw",
        "principal" => "paid_principal_krw",
        _ => bail!("unknown insolvency bucket kind"),
    };
    let query = format!(
        "UPDATE loan_installment
         SET {column} = {column} + ?,
             status = CASE
                 WHEN paid_fee_krw + paid_interest_krw + paid_principal_krw + ?
                    = scheduled_fee_krw + scheduled_interest_krw + scheduled_principal_krw
                 THEN 'paid'
                 ELSE 'partiallyPaid'
             END
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND status IN ('pending', 'due', 'partiallyPaid')"
    );
    let updated = sqlx::query(AssertSqlSafe(query.as_str()))
        .bind(paid_krw)
        .bind(paid_krw)
        .bind(bucket.loan_installment_id)
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .execute(&mut **tx)
        .await?;
    ensure!(
        updated.rows_affected() == 1,
        "insolvency installment payment lost its authority"
    );
    Ok(())
}

async fn discharge_claim_authorities(
    tx: &mut Transaction<'_, MySql>,
    scope: &ScopeRow,
    runtime: &ClaimRuntime,
    planned: &crate::life::InsolvencyClaimDistribution,
) -> Result<()> {
    sqlx::query(
        "UPDATE loan_obligation_bucket
         SET status = 'discharged'
         WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
           AND status IN ('pending', 'delinquent')",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(runtime.claim.loan_contract_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE loan_installment
         SET status = 'discharged'
         WHERE save_id = ? AND run_revision = ? AND loan_contract_id = ?
           AND status IN ('pending', 'due', 'partiallyPaid')",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(runtime.claim.loan_contract_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE scheduled_settlement
         SET status = 'cancelled', cancellation_reason = 'insolvencyDischarge'
         WHERE save_id = ? AND run_revision = ? AND kind = 'loanInstallment'
           AND source_kind = 'loanContract' AND source_id = ? AND status = 'pending'",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(runtime.claim.loan_contract_id.to_string())
    .execute(&mut **tx)
    .await?;
    let updated = sqlx::query(
        "UPDATE loan_contract
         SET status = 'discharged', remaining_principal_krw = 0,
             accrued_interest_krw = 0, accrued_fee_krw = 0,
             interest_remainder_numerator = 0, next_installment_no = NULL,
             oldest_unpaid_due_game_day = NULL
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'defaulted'
           AND remaining_principal_krw = ? AND accrued_interest_krw = ? AND accrued_fee_krw = ?",
    )
    .bind(runtime.claim.loan_contract_id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(runtime.claim.principal_krw)
    .bind(runtime.claim.interest_krw)
    .bind(runtime.claim.fee_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "insolvency loan authority changed"
    );
    ensure!(
        planned.original_claim_krw == planned.distributed_krw + planned.discharged_krw,
        "insolvency claim did not reconcile"
    );
    Ok(())
}

pub(super) async fn read_case_detail(
    pool: &MySqlPool,
    user_id: u64,
    case_id: ResourceId,
) -> Result<InsolvencyReadResult<InsolvencyCaseDetailState>> {
    let mut tx = pool.begin().await?;
    let Some(scope) = read_scope_for_user(&mut tx, user_id, false).await? else {
        tx.commit().await?;
        return Ok(InsolvencyReadResult::Rejected(
            LifeFailureCode::InsolvencyResourceNotFound,
        ));
    };
    let Some(case) = read_case_by_id(&mut tx, &scope, case_id.get(), false).await? else {
        tx.commit().await?;
        return Ok(InsolvencyReadResult::Rejected(
            LifeFailureCode::InsolvencyResourceNotFound,
        ));
    };
    let transition_rows: Vec<(u8, Option<String>, String, u32)> = sqlx::query_as(
        "SELECT transition_no, from_status, to_status, transition_game_day
         FROM insolvency_case_transition
         WHERE save_id = ? AND run_revision = ? AND case_id = ?
         ORDER BY transition_no LIMIT 17",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(case.id)
    .fetch_all(&mut *tx)
    .await?;
    ensure!(
        transition_rows.len() <= 16,
        "insolvency transition bound exceeded"
    );
    let transitions = transition_rows
        .into_iter()
        .map(|(sequence, from_status, to_status, game_day)| {
            Ok(InsolvencyTransitionState {
                sequence,
                from_status: from_status.as_deref().map(parse_case_status).transpose()?,
                to_status: parse_case_status(&to_status)?,
                game_day,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let detail = InsolvencyCaseDetailState {
        summary: case_summary(&case)?,
        policy_set_id: ResourceId::from_u64(case.policy_set_id),
        life_catalog_set_id: ResourceId::from_u64(case.life_catalog_set_id),
        insolvency_component_version_id: ResourceId::from_u64(case.insolvency_component_version_id),
        composition_sha256: case.composition_sha256.clone(),
        automatic_protected_krw: case.automatic_protected_krw,
        additional_protected_krw: case.additional_protected_krw,
        liquidatable_krw: case.liquidatable_krw,
        total_claim_krw: case.total_claim_krw,
        claim_count: case.claim_count,
        transitions,
    };
    tx.commit().await?;
    Ok(InsolvencyReadResult::Found(detail))
}

pub(super) async fn read_claim_page(
    pool: &MySqlPool,
    user_id: u64,
    case_id: ResourceId,
    cursor: Option<String>,
) -> Result<InsolvencyReadResult<InsolvencyClaimPageState>> {
    let mut tx = pool.begin().await?;
    let Some(scope) = read_scope_for_user(&mut tx, user_id, false).await? else {
        tx.commit().await?;
        return Ok(InsolvencyReadResult::Rejected(
            LifeFailureCode::InsolvencyResourceNotFound,
        ));
    };
    if read_case_by_id(&mut tx, &scope, case_id.get(), false)
        .await?
        .is_none()
    {
        tx.commit().await?;
        return Ok(InsolvencyReadResult::Rejected(
            LifeFailureCode::InsolvencyResourceNotFound,
        ));
    }
    let after = decode_page_cursor(cursor.as_deref(), 1, &scope, case_id)?;
    let rows: Vec<ClaimRow> = sqlx::query_as(
        "SELECT id, loan_contract_id, principal_krw, interest_krw, fee_krw,
                allowed_krw, distributed_krw, discharged_krw
         FROM insolvency_claim
         WHERE save_id = ? AND run_revision = ? AND case_id = ? AND id > ?
         ORDER BY id LIMIT 21",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(case_id.get())
    .bind(after)
    .fetch_all(&mut *tx)
    .await?;
    let has_more = rows.len() > PAGE_SIZE;
    let claims = rows
        .iter()
        .take(PAGE_SIZE)
        .map(claim_state)
        .collect::<Vec<_>>();
    let next_cursor = has_more.then(|| {
        encode_page_cursor(PageCursor {
            kind: 1,
            save_id: scope.save_id,
            run_revision: scope.run_revision,
            case_id: case_id.get(),
            after_id: claims.last().map_or(0, |claim| claim.id.get()),
        })
    });
    tx.commit().await?;
    Ok(InsolvencyReadResult::Found(InsolvencyClaimPageState {
        claims,
        next_cursor,
    }))
}

pub(super) async fn read_liquidation_page(
    pool: &MySqlPool,
    user_id: u64,
    case_id: ResourceId,
    cursor: Option<String>,
) -> Result<InsolvencyReadResult<InsolvencyLiquidationPageState>> {
    let mut tx = pool.begin().await?;
    let Some(scope) = read_scope_for_user(&mut tx, user_id, false).await? else {
        tx.commit().await?;
        return Ok(InsolvencyReadResult::Rejected(
            LifeFailureCode::InsolvencyResourceNotFound,
        ));
    };
    if read_case_by_id(&mut tx, &scope, case_id.get(), false)
        .await?
        .is_none()
    {
        tx.commit().await?;
        return Ok(InsolvencyReadResult::Rejected(
            LifeFailureCode::InsolvencyResourceNotFound,
        ));
    }
    let after = decode_page_cursor(cursor.as_deref(), 2, &scope, case_id)?;
    let asset_row: Option<(i64, i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT original_amount_krw, automatic_protected_krw,
                additional_protected_krw, liquidatable_krw, distributed_krw
         FROM insolvency_asset
         WHERE save_id = ? AND run_revision = ? AND case_id = ? AND asset_kind = 'wallet'",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(case_id.get())
    .fetch_optional(&mut *tx)
    .await?;
    let rows: Vec<(u64, u64, i64, u64, u64, u32)> = sqlx::query_as(
        "SELECT id, claim_id, amount_krw, loan_payment_id,
                ledger_transaction_id, applied_game_day
         FROM insolvency_distribution
         WHERE save_id = ? AND run_revision = ? AND case_id = ? AND id > ?
         ORDER BY id LIMIT 21",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(case_id.get())
    .bind(after)
    .fetch_all(&mut *tx)
    .await?;
    let has_more = rows.len() > PAGE_SIZE;
    let distributions = rows
        .iter()
        .take(PAGE_SIZE)
        .map(|row| InsolvencyLiquidationState {
            id: ResourceId::from_u64(row.0),
            claim_id: ResourceId::from_u64(row.1),
            amount_krw: row.2,
            loan_payment_id: ResourceId::from_u64(row.3),
            ledger_transaction_id: ResourceId::from_u64(row.4),
            applied_game_day: row.5,
        })
        .collect::<Vec<_>>();
    let next_cursor = has_more.then(|| {
        encode_page_cursor(PageCursor {
            kind: 2,
            save_id: scope.save_id,
            run_revision: scope.run_revision,
            case_id: case_id.get(),
            after_id: distributions.last().map_or(0, |item| item.id.get()),
        })
    });
    let wallet_asset = asset_row.map(
        |(original, automatic, additional, liquidatable, distributed)| InsolvencyWalletAssetState {
            original_amount_krw: original,
            protected_amount_krw: automatic + additional,
            liquidatable_krw: liquidatable,
            distributed_krw: distributed,
        },
    );
    tx.commit().await?;
    Ok(InsolvencyReadResult::Found(
        InsolvencyLiquidationPageState {
            wallet_asset,
            distributions,
            next_cursor,
        },
    ))
}

pub(super) async fn recover_due_cases_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    target_game_day: u32,
) -> Result<()> {
    let rows: Vec<(u64,)> = sqlx::query_as(
        "SELECT id FROM insolvency_case
         WHERE save_id = ? AND run_revision = ? AND status = 'rebuilding'
           AND credit_restriction_end_exclusive <= ?
         ORDER BY id FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(target_game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() <= 1,
        "multiple rebuilding insolvency cases exist"
    );
    for (case_id,) in rows {
        sqlx::query(
            "UPDATE insolvency_case
             SET status = 'recovered', terminal_game_day = credit_restriction_end_exclusive
             WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'rebuilding'",
        )
        .bind(case_id)
        .bind(save_id)
        .bind(run_revision)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT INTO insolvency_case_transition
                 (save_id, run_revision, case_id, transition_no, from_status, to_status,
                  command_id, transition_game_day, transition_reason)
             SELECT ?, ?, ?, COALESCE(MAX(transition_no), 0) + 1,
                    'rebuilding', 'recovered', NULL, ?, 'restrictionEnded'
             FROM insolvency_case_transition
             WHERE save_id = ? AND run_revision = ? AND case_id = ?",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(case_id)
        .bind(target_game_day)
        .bind(save_id)
        .bind(run_revision)
        .bind(case_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

pub(super) async fn credit_restricted_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
) -> Result<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM insolvency_case
             WHERE save_id = ? AND run_revision = ? AND status = 'rebuilding'
               AND ? < credit_restriction_end_exclusive
         )",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(game_day)
    .fetch_one(&mut **tx)
    .await?)
}

async fn read_snapshot_for_scope(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn InsolvencyRules,
    scope: &ScopeRow,
) -> Result<InsolvencySnapshotState> {
    let loans = read_loan_rows(tx, scope, false).await?;
    let (assessment, _, _, _, _) = assess_scope(tx, rules, scope, &loans, true).await?;
    let current_case = read_current_case(tx, scope)
        .await?
        .map(|row| case_summary(&row))
        .transpose()?;
    ensure!(
        assessment.reasons.len() <= 16,
        "insolvency reasons exceeded snapshot bound"
    );
    Ok(InsolvencySnapshotState {
        availability: if component_active(scope) {
            InsolvencyAvailabilityState::CashOnlyLiquidation
        } else {
            InsolvencyAvailabilityState::Unavailable
        },
        eligibility: assessment.status,
        reasons: assessment.reasons,
        current_case,
    })
}

async fn assess_scope(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn InsolvencyRules,
    scope: &ScopeRow,
    loans: &[LoanRow],
    reject_existing_case: bool,
) -> Result<(
    crate::life::InsolvencyEligibilityAssessment,
    Vec<InsolvencyLoanPosition>,
    u32,
    u32,
    bool,
)> {
    let positions = loans
        .iter()
        .map(loan_position)
        .collect::<Result<Vec<_>>>()?;
    let unsupported_assets = unsupported_asset_count(tx, scope).await?;
    let unsupported_obligations = unsupported_obligation_count(tx, scope).await?;
    let has_lien: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM property_lien
         WHERE save_id = ? AND run_revision = ? AND status = 'active')",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_one(&mut **tx)
    .await?;
    let has_case: bool = if reject_existing_case {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM insolvency_case
             WHERE save_id = ? AND run_revision = ?
               AND status IN ('prepared', 'filed', 'liquidation', 'discharged', 'rebuilding'))",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .fetch_one(&mut **tx)
        .await?
    } else {
        false
    };
    let assessment = rules.assess_eligibility(InsolvencyEligibilityInput {
        policy_available: component_active(scope)
            && scope.policy_key == POLICY_KEY
            && scope.policy_rule_id.is_some()
            && scope
                .minimum_simulation_date
                .is_some_and(|minimum| scope.simulation_date >= minimum),
        component_available: component_active(scope),
        wallet_cash_krw: scope.cash_krw,
        loans: &positions,
        unsupported_asset_position_count: unsupported_assets,
        unsupported_non_loan_obligation_count: unsupported_obligations,
        has_secured_interest: has_lien,
        has_non_terminal_case: has_case,
    })?;
    Ok((
        assessment,
        positions,
        unsupported_assets,
        unsupported_obligations,
        has_lien,
    ))
}

async fn unsupported_asset_count(tx: &mut Transaction<'_, MySql>, scope: &ScopeRow) -> Result<u32> {
    let count: i64 = sqlx::query_scalar(
        "SELECT
            EXISTS(SELECT 1 FROM financial_account
                   WHERE save_id = ? AND run_revision = ? AND status = 'open' AND cash_krw > 0)
          + EXISTS(SELECT 1 FROM cash_product_contract
                   WHERE save_id = ? AND run_revision = ? AND status = 'active')
          + EXISTS(SELECT 1 FROM asset_position WHERE save_id = ? AND quantity > 0)
          + EXISTS(SELECT 1 FROM bond_position
                   WHERE save_id = ? AND run_revision = ? AND bond_units > 0)
          + EXISTS(SELECT 1 FROM gold_position
                   WHERE save_id = ? AND run_revision = ? AND quantity_gram > 0)
          + EXISTS(SELECT 1 FROM physical_gold_holding
                   WHERE save_id = ? AND run_revision = ? AND bar_count > 0)
          + EXISTS(SELECT 1 FROM lease_contract
                   WHERE save_id = ? AND run_revision = ?
                     AND effective_to_game_day IS NULL AND deposit_krw > 0)
          + EXISTS(SELECT 1 FROM property_holding
                   WHERE save_id = ? AND run_revision = ? AND status = 'active')
          + EXISTS(SELECT 1 FROM property_sale_order
                   WHERE save_id = ? AND run_revision = ? AND status = 'active')",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.save_id)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_one(&mut **tx)
    .await?;
    u32::try_from(count).context("unsupported insolvency asset count overflowed")
}

async fn unsupported_obligation_count(
    tx: &mut Transaction<'_, MySql>,
    scope: &ScopeRow,
) -> Result<u32> {
    let count: i64 = sqlx::query_scalar(
        "SELECT
            EXISTS(SELECT 1 FROM essential_arrear
                   WHERE save_id = ? AND run_revision = ? AND status = 'active')
          + EXISTS(SELECT 1 FROM lease_arrear
                   WHERE save_id = ? AND run_revision = ? AND status = 'active')
          + EXISTS(SELECT 1 FROM tax_obligation
                   WHERE save_id = ? AND run_revision = ?
                     AND status IN ('prepared', 'outstanding'))",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_one(&mut **tx)
    .await?;
    u32::try_from(count).context("unsupported insolvency obligation count overflowed")
}

async fn read_scope_for_user(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
    lock: bool,
) -> Result<Option<ScopeRow>> {
    let query = scope_query("save.user_id = ?", lock);
    sqlx::query_as(AssertSqlSafe(query.as_str()))
        .bind(user_id)
        .fetch_optional(&mut **tx)
        .await
        .context("failed to read insolvency scope")
}

async fn read_scope_for_save(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
) -> Result<Option<ScopeRow>> {
    let query = scope_query("save.id = ?", false);
    sqlx::query_as(AssertSqlSafe(query.as_str()))
        .bind(save_id)
        .fetch_optional(&mut **tx)
        .await
        .context("failed to read insolvency snapshot scope")
}

fn scope_query(predicate: &str, lock: bool) -> String {
    format!(
        "SELECT save.id AS save_id, save.run_revision,
                save.state_revision, save.game_day, save.cash_krw,
                EXISTS(SELECT 1 FROM `character` WHERE `character`.save_id = save.id)
                    AS has_character,
                save.policy_set_id, policy.policy_key,
                bundle.life_catalog_set_id,
                catalog.insolvency_component_version_id,
                component.version_key AS component_version_key,
                component.availability AS component_availability,
                profile.minimum_simulation_date,
                COALESCE(daily.market_date, DATE_ADD(world.start_date, INTERVAL save.game_day DAY))
                    AS simulation_date,
                policy_rule.id AS policy_rule_id
         FROM save
         INNER JOIN policy_set AS policy ON policy.id = save.policy_set_id
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         LEFT JOIN market_daily AS daily
           ON daily.world_id = save.market_world_id AND daily.game_day = save.game_day
         LEFT JOIN run_rule_bundle AS bundle
           ON bundle.save_id = save.id AND bundle.run_revision = save.run_revision
         LEFT JOIN life_catalog_set AS catalog ON catalog.id = bundle.life_catalog_set_id
         LEFT JOIN life_component_version AS component
           ON component.id = catalog.insolvency_component_version_id
         LEFT JOIN insolvency_component_profile AS profile
           ON profile.life_component_version_id = component.id
         LEFT JOIN policy_rule
           ON policy_rule.policy_set_id = save.policy_set_id
          AND policy_rule.domain = 'insolvency'
          AND policy_rule.rule_key = 'cashOnlyLiquidation'
          AND policy_rule.effective_from <= COALESCE(
                daily.market_date, DATE_ADD(world.start_date, INTERVAL save.game_day DAY))
          AND (policy_rule.effective_to IS NULL OR policy_rule.effective_to >= COALESCE(
                daily.market_date, DATE_ADD(world.start_date, INTERVAL save.game_day DAY)))
         WHERE {predicate}{}",
        if lock { " FOR UPDATE" } else { "" }
    )
}

async fn read_loan_rows(
    tx: &mut Transaction<'_, MySql>,
    scope: &ScopeRow,
    lock: bool,
) -> Result<Vec<LoanRow>> {
    let query = format!(
        "SELECT id AS contract_id, product_kind, status, read_only,
                remaining_principal_krw, accrued_interest_krw, accrued_fee_krw
         FROM loan_contract
         WHERE save_id = ? AND run_revision = ?
           AND remaining_principal_krw + accrued_interest_krw + accrued_fee_krw > 0
         ORDER BY id LIMIT 21{}",
        if lock { " FOR UPDATE" } else { "" }
    );
    let rows = sqlx::query_as(AssertSqlSafe(query.as_str()))
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .fetch_all(&mut **tx)
        .await?;
    ensure!(
        rows.len() <= 20,
        "insolvency loan composition exceeded its bound"
    );
    Ok(rows)
}

async fn read_current_case(
    tx: &mut Transaction<'_, MySql>,
    scope: &ScopeRow,
) -> Result<Option<CaseRow>> {
    sqlx::query_as(
        "SELECT id, life_catalog_set_id, policy_set_id, insolvency_component_version_id,
                procedure_kind, status, composition_sha256, prepared_game_day,
                submitted_game_day, credit_restriction_end_exclusive,
                wallet_cash_krw, automatic_protected_krw, additional_protected_krw,
                liquidatable_krw, total_claim_krw, claim_count,
                distributed_krw, discharged_krw
         FROM insolvency_case
         WHERE save_id = ? AND run_revision = ?
           AND status IN ('prepared', 'filed', 'liquidation', 'discharged', 'rebuilding')
         ORDER BY id DESC LIMIT 1",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read current insolvency case")
}

async fn read_case_by_id(
    tx: &mut Transaction<'_, MySql>,
    scope: &ScopeRow,
    case_id: u64,
    lock: bool,
) -> Result<Option<CaseRow>> {
    let query = format!(
        "SELECT id, life_catalog_set_id, policy_set_id, insolvency_component_version_id,
                procedure_kind, status, composition_sha256, prepared_game_day,
                submitted_game_day, credit_restriction_end_exclusive,
                wallet_cash_krw, automatic_protected_krw, additional_protected_krw,
                liquidatable_krw, total_claim_krw, claim_count,
                distributed_krw, discharged_krw
         FROM insolvency_case
         WHERE id = ? AND save_id = ? AND run_revision = ?{}",
        if lock { " FOR UPDATE" } else { "" }
    );
    sqlx::query_as(AssertSqlSafe(query.as_str()))
        .bind(case_id)
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .fetch_optional(&mut **tx)
        .await
        .context("failed to read insolvency case")
}

fn component_active(scope: &ScopeRow) -> bool {
    scope.component_version_key.as_deref() == Some(COMPONENT_KEY)
        && scope.component_availability.as_deref() == Some("active")
        && scope.insolvency_component_version_id.is_some()
}

fn loan_position(row: &LoanRow) -> Result<InsolvencyLoanPosition> {
    Ok(InsolvencyLoanPosition {
        contract_id: ResourceId::from_u64(row.contract_id),
        product_kind: parse_product_kind(&row.product_kind)?,
        status: parse_loan_status(&row.status)?,
        read_only: row.read_only,
        remaining_principal_krw: row.remaining_principal_krw,
        accrued_interest_krw: row.accrued_interest_krw,
        accrued_fee_krw: row.accrued_fee_krw,
    })
}

fn composition_hash(
    rules: &dyn InsolvencyRules,
    wallet_cash_krw: i64,
    positions: &[InsolvencyLoanPosition],
    unsupported_assets: u32,
    unsupported_obligations: u32,
    has_lien: bool,
) -> Result<String> {
    let values = [
        unsupported_assets.to_string(),
        unsupported_obligations.to_string(),
        has_lien.to_string(),
    ];
    let facts = [
        InsolvencyCompositionFact {
            authority_key: "unsupportedAssetPositionCount",
            canonical_value: &values[0],
        },
        InsolvencyCompositionFact {
            authority_key: "unsupportedNonLoanObligationCount",
            canonical_value: &values[1],
        },
        InsolvencyCompositionFact {
            authority_key: "hasSecuredInterest",
            canonical_value: &values[2],
        },
    ];
    rules
        .composition_sha256(InsolvencyCompositionInput {
            wallet_cash_krw,
            claims: positions,
            facts: &facts,
        })
        .context("insolvency composition hash failed")
}

fn aggregate_buckets(rows: &[BucketRow]) -> Result<Vec<RepaymentBucketBalance>> {
    let mut totals = BTreeMap::<RepaymentBucketKind, i64>::new();
    for row in rows {
        let kind = bucket_kind(row)?;
        let outstanding = row
            .original_amount_krw
            .checked_sub(row.paid_amount_krw)
            .context("loan bucket outstanding underflowed")?;
        let entry = totals.entry(kind).or_default();
        *entry = entry
            .checked_add(outstanding)
            .context("loan bucket aggregate overflowed")?;
    }
    Ok(totals
        .into_iter()
        .filter(|(_, due_krw)| *due_krw > 0)
        .map(|(kind, due_krw)| RepaymentBucketBalance { kind, due_krw })
        .collect())
}

fn bucket_kind(row: &BucketRow) -> Result<RepaymentBucketKind> {
    match (row.status == "delinquent", row.bucket_kind.as_str()) {
        (true, "fee") => Ok(RepaymentBucketKind::OverdueFee),
        (true, "interest") => Ok(RepaymentBucketKind::OverdueInterest),
        (true, "principal") => Ok(RepaymentBucketKind::OverduePrincipal),
        (false, "fee") => Ok(RepaymentBucketKind::CurrentFee),
        (false, "interest") => Ok(RepaymentBucketKind::CurrentInterest),
        (false, "principal") => Ok(RepaymentBucketKind::CurrentPrincipal),
        _ => bail!("unknown insolvency loan bucket"),
    }
}

fn bucket_kind_db(kind: RepaymentBucketKind) -> &'static str {
    match kind {
        RepaymentBucketKind::OverdueFee => "overdueFee",
        RepaymentBucketKind::OverdueInterest => "overdueInterest",
        RepaymentBucketKind::OverduePrincipal => "overduePrincipal",
        RepaymentBucketKind::CurrentFee => "currentFee",
        RepaymentBucketKind::CurrentInterest => "currentInterest",
        RepaymentBucketKind::CurrentPrincipal => "currentPrincipal",
    }
}

fn paid_totals(allocations: &[crate::life::RepaymentBucketAllocation]) -> Result<(i64, i64, i64)> {
    let mut principal = 0_i64;
    let mut interest = 0_i64;
    let mut fee = 0_i64;
    for allocation in allocations {
        let target = match allocation.kind {
            RepaymentBucketKind::OverduePrincipal | RepaymentBucketKind::CurrentPrincipal => {
                &mut principal
            }
            RepaymentBucketKind::OverdueInterest | RepaymentBucketKind::CurrentInterest => {
                &mut interest
            }
            RepaymentBucketKind::OverdueFee | RepaymentBucketKind::CurrentFee => &mut fee,
        };
        *target = target
            .checked_add(allocation.paid_krw)
            .context("insolvency paid total overflowed")?;
    }
    Ok((principal, interest, fee))
}

fn policy_context(scope: &ScopeRow) -> RunPolicyContext {
    RunPolicyContext {
        run: RunId {
            save_id: ResourceId::from_u64(scope.save_id),
            run_revision: scope.run_revision,
        },
        policy_set_id: ResourceId::from_u64(scope.policy_set_id),
    }
}

fn case_summary(row: &CaseRow) -> Result<InsolvencyCaseSummaryState> {
    Ok(InsolvencyCaseSummaryState {
        id: ResourceId::from_u64(row.id),
        procedure_kind: match row.procedure_kind.as_str() {
            "cashOnlyLiquidation" => InsolvencyProcedureKind::CashOnlyLiquidation,
            _ => bail!("unknown insolvency procedure kind"),
        },
        status: parse_case_status(&row.status)?,
        prepared_game_day: row.prepared_game_day,
        submitted_game_day: row.submitted_game_day,
        wallet_cash_krw: row.wallet_cash_krw,
        protected_cash_krw: row
            .automatic_protected_krw
            .checked_add(row.additional_protected_krw)
            .context("insolvency protected cash overflowed")?,
        distributed_krw: row.distributed_krw,
        discharged_krw: row.discharged_krw,
        credit_restriction_end_exclusive: row.credit_restriction_end_exclusive,
    })
}

fn claim_state(row: &ClaimRow) -> InsolvencyClaimState {
    InsolvencyClaimState {
        id: ResourceId::from_u64(row.id),
        loan_contract_id: ResourceId::from_u64(row.loan_contract_id),
        principal_krw: row.principal_krw,
        interest_krw: row.interest_krw,
        fee_krw: row.fee_krw,
        allowed_krw: row.allowed_krw,
        distributed_krw: row.distributed_krw,
        discharged_krw: row.discharged_krw,
    }
}

fn parse_product_kind(raw: &str) -> Result<LoanProductKind> {
    match raw {
        "studentLoan" => Ok(LoanProductKind::StudentLoan),
        "unsecuredLoan" => Ok(LoanProductKind::UnsecuredLoan),
        "leaseDepositLoan" => Ok(LoanProductKind::LeaseDepositLoan),
        "mortgage" => Ok(LoanProductKind::Mortgage),
        "legacyDebt" => Ok(LoanProductKind::LegacyDebt),
        _ => bail!("unknown loan product kind in insolvency composition"),
    }
}

fn parse_loan_status(raw: &str) -> Result<LoanContractStatus> {
    match raw {
        "pending" => Ok(LoanContractStatus::Pending),
        "active" => Ok(LoanContractStatus::Active),
        "delinquent" => Ok(LoanContractStatus::Delinquent),
        "defaulted" => Ok(LoanContractStatus::Defaulted),
        "paidOff" => Ok(LoanContractStatus::PaidOff),
        "restructured" => Ok(LoanContractStatus::Restructured),
        "discharged" => Ok(LoanContractStatus::Discharged),
        "chargedOff" => Ok(LoanContractStatus::ChargedOff),
        "cancelled" => Ok(LoanContractStatus::Cancelled),
        _ => bail!("unknown loan status in insolvency composition"),
    }
}

fn parse_case_status(raw: &str) -> Result<InsolvencyCaseStatus> {
    match raw {
        "prepared" => Ok(InsolvencyCaseStatus::Prepared),
        "filed" => Ok(InsolvencyCaseStatus::Filed),
        "liquidation" => Ok(InsolvencyCaseStatus::Liquidation),
        "discharged" => Ok(InsolvencyCaseStatus::Discharged),
        "rebuilding" => Ok(InsolvencyCaseStatus::Rebuilding),
        "withdrawn" => Ok(InsolvencyCaseStatus::Withdrawn),
        "recovered" => Ok(InsolvencyCaseStatus::Recovered),
        _ => bail!("unknown insolvency case status"),
    }
}

fn status_db(status: InsolvencyCaseStatus) -> &'static str {
    match status {
        InsolvencyCaseStatus::Prepared => "prepared",
        InsolvencyCaseStatus::Filed => "filed",
        InsolvencyCaseStatus::Liquidation => "liquidation",
        InsolvencyCaseStatus::Discharged => "discharged",
        InsolvencyCaseStatus::Rebuilding => "rebuilding",
        InsolvencyCaseStatus::Withdrawn => "withdrawn",
        InsolvencyCaseStatus::Recovered => "recovered",
    }
}

fn prepare_fingerprint(command: &PrepareInsolvencyCaseCommand) -> String {
    sha256(&format!(
        "lifeledger.life.insolvencyCase.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\nprocedureKind=cashOnlyLiquidation",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
    ))
}

fn action_fingerprint(command: &ActOnInsolvencyCaseCommand) -> String {
    sha256(&format!(
        "lifeledger.life.insolvencyAction.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\ncaseId={}\naction={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.case_id,
        match command.action {
            InsolvencyActionState::Submit => "submit",
            InsolvencyActionState::Withdraw => "withdraw",
        }
    ))
}

fn sha256(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

async fn inspect_replay(
    tx: &mut Transaction<'_, MySql>,
    scope: &ScopeRow,
    identity: &CommandIdentitySpec<'_>,
) -> Result<Option<Result<InsolvencyCaseReceipt, LifeFailureCode>>> {
    match inspect_command_identity(tx, scope.save_id, identity).await? {
        CommandIdentityState::Missing => Ok(None),
        CommandIdentityState::Conflict => Ok(Some(Err(LifeFailureCode::IdempotencyConflict))),
        CommandIdentityState::Matching => {
            let row: Option<(String, String, String)> = sqlx::query_as(
                "SELECT command_kind, payload_sha256, result_json
                 FROM insolvency_command_receipt
                 WHERE save_id = ? AND command_id = ? FOR SHARE",
            )
            .bind(scope.save_id)
            .bind(identity.command_id.as_str())
            .fetch_optional(&mut **tx)
            .await?;
            let (command_kind, payload_sha256, result_json) =
                row.context("insolvency command identity has no receipt")?;
            ensure!(
                payload_sha256 == identity.payload_sha256
                    && (command_kind == "prepareCase"
                        || command_kind == "submitCase"
                        || command_kind == "withdrawCase"),
                "insolvency receipt disagrees with its identity"
            );
            let mut receipt: InsolvencyCaseReceipt =
                serde_json::from_str(&result_json).context("invalid insolvency receipt")?;
            ensure!(!receipt.replayed && receipt.command_id == *identity.command_id);
            receipt.replayed = true;
            Ok(Some(Ok(receipt)))
        }
    }
}

async fn finish_replay(
    mut tx: Transaction<'_, MySql>,
    save_id: u64,
    replay: Result<InsolvencyCaseReceipt, LifeFailureCode>,
) -> Result<LifeStoreResult<InsolvencyCaseReceipt>> {
    match replay {
        Ok(receipt) => {
            let save = read_state(&mut tx, save_id).await?;
            tx.commit().await?;
            Ok(LifeStoreResult::Applied {
                receipt,
                save: Box::new(save),
            })
        }
        Err(code) => {
            tx.commit().await?;
            Ok(LifeStoreResult::Rejected(code))
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn write_receipt<T: Serialize>(
    tx: &mut Transaction<'_, MySql>,
    scope: &ScopeRow,
    component_version_id: u64,
    command_id: &str,
    command_kind: &str,
    payload_sha256: &str,
    case_id: u64,
    committed_state_revision: u64,
    result: &T,
) -> Result<()> {
    let result_json = serde_json::to_string(result)?;
    sqlx::query(
        "INSERT INTO insolvency_command_receipt
             (save_id, run_revision, insolvency_component_version_id,
              command_id, command_kind, payload_sha256, case_id, result_json,
              committed_state_revision, committed_game_day)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(component_version_id)
    .bind(command_id)
    .bind(command_kind)
    .bind(payload_sha256)
    .bind(case_id)
    .bind(result_json)
    .bind(committed_state_revision)
    .bind(scope.game_day)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_transition(
    tx: &mut Transaction<'_, MySql>,
    scope: &ScopeRow,
    case_id: u64,
    transition_no: u8,
    from_status: Option<&str>,
    to_status: &str,
    command_id: Option<&str>,
    game_day: u32,
    reason: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO insolvency_case_transition
             (save_id, run_revision, case_id, transition_no, from_status,
              to_status, command_id, transition_game_day, transition_reason)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(case_id)
    .bind(transition_no)
    .bind(from_status)
    .bind(to_status)
    .bind(command_id)
    .bind(game_day)
    .bind(reason)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn advance_state_revision(tx: &mut Transaction<'_, MySql>, scope: &ScopeRow) -> Result<u64> {
    let next = scope
        .state_revision
        .checked_add(1)
        .context("insolvency state revision overflowed")?;
    let updated = sqlx::query(
        "UPDATE save SET state_revision = ?
         WHERE id = ? AND run_revision = ? AND state_revision = ? AND game_day = ?",
    )
    .bind(next)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.state_revision)
    .bind(scope.game_day)
    .execute(&mut **tx)
    .await?;
    ensure!(updated.rows_affected() == 1, "insolvency cursor changed");
    Ok(next)
}

fn has_current_cursor(scope: &ScopeRow, cursor: crate::finance::CommandCursor) -> bool {
    scope.run_revision == cursor.expected_run_revision
        && scope.state_revision == cursor.expected_state_revision
        && scope.game_day == cursor.expected_game_day
}

fn encode_page_cursor(cursor: PageCursor) -> String {
    let mut payload = Vec::with_capacity(CURSOR_PAYLOAD_BYTES + CURSOR_CHECKSUM_BYTES);
    payload.push(cursor.kind);
    payload.extend_from_slice(&cursor.save_id.to_be_bytes());
    payload.extend_from_slice(&cursor.run_revision.to_be_bytes());
    payload.extend_from_slice(&cursor.case_id.to_be_bytes());
    payload.extend_from_slice(&cursor.after_id.to_be_bytes());
    let checksum = page_checksum(&payload);
    payload.extend_from_slice(&checksum[..CURSOR_CHECKSUM_BYTES]);
    URL_SAFE_NO_PAD.encode(payload)
}

fn decode_page_cursor(
    raw: Option<&str>,
    kind: u8,
    scope: &ScopeRow,
    case_id: ResourceId,
) -> Result<u64> {
    let Some(raw) = raw else {
        return Ok(0);
    };
    ensure!(!raw.is_empty() && raw.len() <= 512 && raw.is_ascii());
    let decoded = URL_SAFE_NO_PAD.decode(raw)?;
    ensure!(decoded.len() == CURSOR_PAYLOAD_BYTES + CURSOR_CHECKSUM_BYTES);
    let (payload, checksum) = decoded.split_at(CURSOR_PAYLOAD_BYTES);
    ensure!(checksum == &page_checksum(payload)[..CURSOR_CHECKSUM_BYTES]);
    let cursor = PageCursor {
        kind: payload[0],
        save_id: read_u64(&payload[1..9])?,
        run_revision: read_u32(&payload[9..13])?,
        case_id: read_u64(&payload[13..21])?,
        after_id: read_u64(&payload[21..29])?,
    };
    ensure!(
        cursor.kind == kind
            && cursor.save_id == scope.save_id
            && cursor.run_revision == scope.run_revision
            && cursor.case_id == case_id.get()
            && cursor.after_id > 0
            && encode_page_cursor(cursor) == raw
    );
    Ok(cursor.after_id)
}

fn page_checksum(payload: &[u8]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(CURSOR_DOMAIN);
    digest.update(payload);
    digest.finalize().into()
}

fn read_u64(bytes: &[u8]) -> Result<u64> {
    Ok(u64::from_be_bytes(bytes.try_into()?))
}

fn read_u32(bytes: &[u8]) -> Result<u32> {
    Ok(u32::from_be_bytes(bytes.try_into()?))
}

#[derive(Debug)]
struct CompositionChanged;

impl std::fmt::Display for CompositionChanged {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("insolvency composition changed")
    }
}

impl std::error::Error for CompositionChanged {}

#[derive(Debug)]
struct CompositionUnsupported;

impl std::fmt::Display for CompositionUnsupported {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("insolvency composition became unsupported")
    }
}

impl std::error::Error for CompositionUnsupported {}

pub(super) fn is_composition_changed(error: &anyhow::Error) -> bool {
    error.downcast_ref::<CompositionChanged>().is_some()
}

fn is_composition_unsupported(error: &anyhow::Error) -> bool {
    error.downcast_ref::<CompositionUnsupported>().is_some()
}
