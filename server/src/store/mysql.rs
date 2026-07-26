//! MySQL implementation of `SaveStore`.
//!
//! Enums go into string columns using their domain serde representation (§4.3). Keeping
//! the conversion in one place stops API responses and stored values from drifting apart.

use anyhow::{Context, Result, bail, ensure};
use async_trait::async_trait;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::MySqlPool;
use std::sync::Arc;

use super::annual_tax::{
    AnnualTaxRunContext, apply_annual_tax_filing, plan_annual_tax_filing,
    prepare_annual_tax_year_boundary, read_annual_tax_year, read_latest_annual_tax_assessment,
};
use super::career::{
    advance_career_activities_in_tx, initialize_career_run_and_bridge_evidence_in_tx,
    read_career_snapshot_in_tx,
};
use super::cash_products::{
    CashProductSettlementInput, read_cash_product_state, settle_cash_product_by_id_in_tx,
};
use super::m2d_assets::{
    LlxTaxAccountTradeInput, LlxTaxAccountTradeResult, M2dDailyAssetContext,
    PensionAccountMarketValue, apply_pension_mark_to_market_plans_in_tx,
    create_llx_entitlements_in_tx, ensure_monthly_bond_series_in_tx,
    plan_pension_mark_to_market_in_tx, prepare_llx_tax_account_trade_in_tx,
    read_due_pension_bond_principal_in_tx, read_m2d_asset_snapshot_for_run_in_tx,
    settle_bond_cash_flow_by_id_in_tx, settle_llx_entitlement_by_id_in_tx,
};
use super::recruitment::{advance_recruitment_actions_in_tx, advance_recruitment_lifecycle_in_tx};
use super::tax_accounts::{
    TaxAccountStateInput, cancel_tax_accounts_for_new_run, ensure_m2_tax_profile,
    pin_pension_opening_values_for_day, read_tax_account_state,
};
use super::types::{
    ActiveMarketWorld, ActiveRunConfiguration, AdvanceCommandReceipt, AdvanceCommandStepResult,
    AdvanceDayResult, GameCommandCursor, GameCommandRejection, ManualAdvanceCommand, SaveCursor,
    SaveState, SaveStore, StartGameCommand, StartGameReceipt, StartGameResult, TradeStoreResult,
    TradingStore,
};
use crate::character::{Character, create_character};
use crate::finance::{
    AssetOrderSide, CommandCursor, CommandId, FinanceFailureCode, FinanceRules, FinancialAccount,
    FinancialAccountStatus, FinancialAccountType, LedgerAccountCode, LedgerPosting, LedgerSource,
    LedgerSourceKind, LedgerTransaction, LedgerTransactionDraft, PolicySet, PolicySetAssignment,
    ResourceId, RunId, RunPolicyContext, ScheduledSettlement, SettlementKind, SettlementSource,
    SettlementSourceKind, SettlementStatus,
};
use crate::market::MarketDay;
use crate::trading::{
    AccountId, LLX_SYMBOL, OrderSide, PositionState, TradeCharges, TradeExecution, TradeFailure,
    TradeOrder, apply_trade_with_charges, checked_net_worth_krw, value_portfolio,
};

/// Starting cash before a character exists, in KRW.
const INITIAL_CASH_KRW: i64 = 10_000_000;
const COMMAND_KIND_START_GAME: &str = "startGame";
const COMMAND_KIND_ADVANCE: &str = "advance";
const COMMAND_KIND_TRADE: &str = "trade";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommandIdentityState {
    Missing,
    Matching,
    Conflict,
}

pub(super) struct CommandIdentitySpec<'a> {
    pub command_id: &'a CommandId,
    pub command_kind: &'static str,
    pub payload_sha256: &'a str,
    pub cursor: CommandCursor,
}

#[derive(Clone)]
pub struct MySqlSaveStore {
    pool: MySqlPool,
    finance_rules: Arc<dyn FinanceRules>,
}

pub fn create_mysql_save_store(
    pool: MySqlPool,
    finance_rules: Arc<dyn FinanceRules>,
) -> MySqlSaveStore {
    MySqlSaveStore {
        pool,
        finance_rules,
    }
}

/// Finds the account's save, creating it if absent. One save per account (§4.5).
async fn ensure_save(tx: &mut sqlx::Transaction<'_, sqlx::MySql>, user_id: u64) -> Result<u64> {
    sqlx::query(
        "INSERT INTO save
             (user_id, market_world_id, policy_set_id, market_world_product_bundle_id,
              game_day, cash_krw, debt_krw)
         SELECT ?, market_assignment.world_id, policy_assignment.policy_set_id,
                bundle.id, 0, ?, 0
         FROM market_world_assignment AS market_assignment
         CROSS JOIN policy_set_assignment AS policy_assignment
         LEFT JOIN market_world_product_bundle AS bundle
           ON bundle.market_world_id = market_assignment.world_id
          AND bundle.published_at IS NOT NULL
         WHERE market_assignment.assignment_key = 'newRun'
           AND policy_assignment.assignment_key = 'newRun'
         ON DUPLICATE KEY UPDATE id = LAST_INSERT_ID(save.id)",
    )
    .bind(user_id)
    .bind(INITIAL_CASH_KRW)
    .execute(&mut **tx)
    .await?;

    let row: Option<(u64,)> = sqlx::query_as("SELECT id FROM save WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(&mut **tx)
        .await?;

    let save_id = row
        .map(|(save_id,)| save_id)
        .context("active market world assignment is missing")?;
    sqlx::query(
        "UPDATE save
         LEFT JOIN market_world_product_bundle AS bundle
           ON bundle.market_world_id = save.market_world_id
          AND bundle.published_at IS NOT NULL
         SET save.market_world_product_bundle_id = bundle.id
         WHERE save.id = ? AND save.market_world_product_bundle_id IS NULL
           AND NOT EXISTS(SELECT 1 FROM `character` WHERE `character`.save_id = save.id)",
    )
    .bind(save_id)
    .execute(&mut **tx)
    .await?;
    ensure_m2_tax_profile(tx, save_id).await?;
    Ok(save_id)
}

pub(super) async fn inspect_command_identity(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    save_id: u64,
    spec: &CommandIdentitySpec<'_>,
) -> Result<CommandIdentityState> {
    let row: Option<CommandIdentityRow> = sqlx::query_as(
        "SELECT command_kind, payload_sha256, initial_run_revision,
                initial_state_revision, initial_game_day
         FROM command_identity
         WHERE save_id = ? AND command_id = ?
         FOR SHARE",
    )
    .bind(save_id)
    .bind(spec.command_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;

    Ok(match row {
        None => CommandIdentityState::Missing,
        Some(row)
            if row.command_kind == spec.command_kind
                && row.payload_sha256 == spec.payload_sha256
                && row.initial_run_revision == spec.cursor.expected_run_revision
                && row.initial_state_revision == spec.cursor.expected_state_revision
                && row.initial_game_day == spec.cursor.expected_game_day =>
        {
            CommandIdentityState::Matching
        }
        Some(_) => CommandIdentityState::Conflict,
    })
}

pub(super) async fn write_command_identity(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    save_id: u64,
    spec: &CommandIdentitySpec<'_>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO command_identity
             (save_id, command_id, command_kind, payload_sha256,
              initial_run_revision, initial_state_revision, initial_game_day)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(save_id)
    .bind(spec.command_id.as_str())
    .bind(spec.command_kind)
    .bind(spec.payload_sha256)
    .bind(spec.cursor.expected_run_revision)
    .bind(spec.cursor.expected_state_revision)
    .bind(spec.cursor.expected_game_day)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

#[async_trait]
impl SaveStore for MySqlSaveStore {
    async fn load(&self, user_id: u64) -> Result<SaveState> {
        let mut tx = self.pool.begin().await?;
        let save_id = ensure_save(&mut tx, user_id).await?;
        let state = read_state(&mut tx, save_id).await?;
        tx.commit().await?;

        Ok(state)
    }

    async fn active_run_configuration(&self) -> Result<ActiveRunConfiguration> {
        read_active_run_configuration(&self.pool).await
    }

    async fn start_game(
        &self,
        user_id: u64,
        command: &StartGameCommand,
        expected: ActiveRunConfiguration,
    ) -> Result<StartGameResult> {
        let fingerprint = start_game_fingerprint(command)?;
        let identity = CommandIdentitySpec {
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_START_GAME,
            payload_sha256: &fingerprint,
            cursor: command.cursor,
        };
        let mut tx = self.pool.begin().await?;
        let save_id = ensure_save(&mut tx, user_id).await?;
        lock_save(&mut tx, save_id).await?;
        match inspect_command_identity(&mut tx, save_id, &identity).await? {
            CommandIdentityState::Conflict => {
                tx.commit().await?;
                return Ok(StartGameResult::Rejected(
                    GameCommandRejection::IdempotencyConflict,
                ));
            }
            CommandIdentityState::Matching => {
                let receipt =
                    read_game_command_receipt(&mut tx, save_id, command.command_id.as_str())
                        .await?
                        .context("start-game command identity has no final receipt")?
                        .into_start_game_receipt(command, true)?;
                let save = read_state(&mut tx, save_id).await?;
                tx.commit().await?;

                return Ok(StartGameResult::Replayed {
                    save: Box::new(save),
                    receipt,
                });
            }
            CommandIdentityState::Missing => {}
        }

        let character = match create_character(command.draft.clone()) {
            Ok(character) => character,
            Err(errors) => {
                tx.commit().await?;
                return Ok(StartGameResult::Rejected(
                    GameCommandRejection::InvalidCharacter(errors),
                ));
            }
        };

        let current = read_state(&mut tx, save_id).await?;
        if GameCommandCursor::from(&current) != GameCommandCursor::from(command.cursor) {
            tx.commit().await?;
            return Ok(StartGameResult::Rejected(GameCommandRejection::Busy));
        }

        let active = lock_active_run_configuration(&mut tx).await?;
        if active != expected {
            tx.commit().await?;
            return Ok(StartGameResult::ActiveWorldChanged);
        }

        write_command_identity(&mut tx, save_id, &identity).await?;

        cancel_tax_accounts_for_new_run(
            &mut tx,
            save_id,
            current.run_revision,
            current.market_world_id,
            current.game_day,
            &command.command_id,
        )
        .await?;
        close_career_run_for_new_run_in_tx(
            &mut tx,
            save_id,
            current.run_revision,
            current.game_day,
        )
        .await?;

        sqlx::query(
            "UPDATE cash_product_contract AS contract
             INNER JOIN save ON save.id = contract.save_id
             SET contract.status = 'cancelled',
                 contract.closed_game_day = save.game_day,
                 contract.cancellation_reason = 'newRun'
             WHERE contract.save_id = ?
               AND contract.run_revision = save.run_revision
               AND contract.status = 'active'",
        )
        .bind(save_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE savings_installment AS installment
             INNER JOIN save ON save.id = installment.save_id
             SET installment.status = 'cancelled',
                 installment.processed_game_day = save.game_day,
                 installment.cancellation_reason = 'newRun'
             WHERE installment.save_id = ?
               AND installment.run_revision = save.run_revision
               AND installment.status = 'pending'",
        )
        .bind(save_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE scheduled_settlement AS settlement
             INNER JOIN save ON save.id = settlement.save_id
             SET settlement.status = 'cancelled',
                 settlement.cancellation_reason = 'newRun'
             WHERE settlement.save_id = ?
               AND settlement.run_revision = save.run_revision
               AND settlement.status = 'pending'",
        )
        .bind(save_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE financial_account AS account
             INNER JOIN save ON save.id = account.save_id
             SET account.status = 'closed'
             WHERE account.save_id = ?
               AND account.run_revision = save.run_revision
               AND account.status <> 'closed'",
        )
        .bind(save_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "UPDATE save
             SET run_revision = run_revision + 1, state_revision = 0,
                 market_world_id = ?, policy_set_id = ?, market_world_product_bundle_id = ?,
                 game_day = 0,
                 cash_krw = ?, debt_krw = ?
             WHERE id = ?",
        )
        .bind(expected.market_world.world_id)
        .bind(expected.policy_set.policy_set_id.get())
        .bind(expected.product_bundle_id.map(ResourceId::get))
        .bind(character.cash_krw)
        .bind(character.debt_krw)
        .bind(save_id)
        .execute(&mut *tx)
        .await?;

        ensure_m2_tax_profile(&mut tx, save_id).await?;

        create_default_account(&mut tx, save_id, 0).await?;
        let opening_ledger_transaction_id = write_opening_ledger(
            &mut tx,
            &*self.finance_rules,
            save_id,
            character.cash_krw,
            character.debt_krw,
            command.command_id.as_str(),
        )
        .await?;

        // One character per save; replacing reads more clearly than an upsert
        sqlx::query("DELETE FROM `character` WHERE save_id = ?")
            .bind(save_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query(
            "INSERT INTO `character`
                 (save_id, name, age, gender, military, region, background,
                  education, career_years, certifications, health, dependents)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(save_id)
        .bind(&character.name)
        .bind(character.age)
        .bind(to_db_str(&character.gender)?)
        .bind(to_db_str(&character.military)?)
        .bind(to_db_str(&character.region)?)
        .bind(to_db_str(&character.background)?)
        .bind(to_db_str(&character.education)?)
        .bind(character.career_years)
        .bind(character.certifications)
        .bind(to_db_str(&character.health)?)
        .bind(character.dependents)
        .execute(&mut *tx)
        .await?;

        let new_run_revision = current
            .run_revision
            .checked_add(1)
            .context("run revision overflowed while initializing career state")?;
        initialize_career_run_and_bridge_evidence_in_tx(
            &mut tx,
            save_id,
            new_run_revision,
            expected.career_catalog.bundle_id.get(),
            &character,
        )
        .await?;

        let state = read_state(&mut tx, save_id).await?;
        let receipt = StartGameReceipt {
            command_id: command.command_id.clone(),
            committed_cursor: GameCommandCursor::from(&state),
            replayed: false,
        };
        write_game_command_receipt(
            &mut tx,
            GameCommandReceiptWrite {
                save_id,
                command_id: &command.command_id,
                command_kind: COMMAND_KIND_START_GAME,
                payload_sha256: &fingerprint,
                market_world_id: state.market_world_id,
                committed_cursor: receipt.committed_cursor,
                result: &receipt,
                ledger_transaction_id: opening_ledger_transaction_id,
            },
        )
        .await?;
        tx.commit().await?;

        Ok(StartGameResult::Applied {
            save: Box::new(state),
            receipt,
        })
    }

    async fn advance_one_day(
        &self,
        user_id: u64,
        expected: SaveCursor,
        market: &MarketDay,
    ) -> Result<AdvanceDayResult> {
        let mut tx = self.pool.begin().await?;
        let save_id = ensure_save(&mut tx, user_id).await?;
        lock_save(&mut tx, save_id).await?;
        let current = read_state(&mut tx, save_id).await?;

        if current.character.is_none() {
            tx.commit().await?;
            return Ok(AdvanceDayResult::CharacterRequired);
        }
        if SaveCursor::from(&current) != expected {
            tx.commit().await?;
            return Ok(AdvanceDayResult::Stale(current));
        }
        let target_game_day = expected
            .game_day
            .checked_add(1)
            .context("game day overflowed while validating settlement input")?;
        if market.game_day != target_game_day {
            bail!("daily market input does not match the target game day");
        }
        settle_daily_finance_state(
            &mut tx,
            Arc::clone(&self.finance_rules),
            &current,
            target_game_day,
            market,
        )
        .await?;

        // Keep the cursor predicate at the write itself, so this remains safe even if
        // save creation stops taking a row lock in a later storage refactor.
        let result = sqlx::query(
            "UPDATE save
             SET game_day = game_day + 1, state_revision = state_revision + 1
             WHERE id = ? AND market_world_id = ? AND run_revision = ?
                 AND policy_set_id = ? AND state_revision = ? AND game_day = ?",
        )
        .bind(save_id)
        .bind(expected.market_world_id)
        .bind(expected.run_revision)
        .bind(expected.policy_set_id)
        .bind(expected.state_revision)
        .bind(expected.game_day)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() == 0 {
            // Daily planners may have staged state before this cursor guard. A stale
            // write must discard all of it with the cursor update.
            tx.rollback().await?;
            return Ok(AdvanceDayResult::Stale(current));
        }

        let state = read_state(&mut tx, save_id).await?;
        tx.commit().await?;

        Ok(AdvanceDayResult::Advanced(state))
    }

    async fn advance_command_step(
        &self,
        user_id: u64,
        command: &ManualAdvanceCommand,
        market: &MarketDay,
    ) -> Result<AdvanceCommandStepResult> {
        let fingerprint = advance_command_fingerprint(command);
        let identity = CommandIdentitySpec {
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_ADVANCE,
            payload_sha256: &fingerprint,
            cursor: command.cursor,
        };
        let mut tx = self.pool.begin().await?;
        let save_id = ensure_save(&mut tx, user_id).await?;
        lock_save(&mut tx, save_id).await?;
        let identity_state = inspect_command_identity(&mut tx, save_id, &identity).await?;
        if identity_state == CommandIdentityState::Conflict {
            tx.commit().await?;
            return Ok(AdvanceCommandStepResult::Rejected(
                GameCommandRejection::IdempotencyConflict,
            ));
        }

        if identity_state == CommandIdentityState::Matching
            && let Some(receipt) =
                read_game_command_receipt(&mut tx, save_id, command.command_id.as_str()).await?
        {
            let receipt = receipt.into_advance_receipt(command, true)?;
            let save = read_state(&mut tx, save_id).await?;
            tx.commit().await?;

            return Ok(AdvanceCommandStepResult::Replayed {
                save: Box::new(save),
                receipt,
            });
        }

        if !(1..=30).contains(&command.days) {
            tx.commit().await?;
            return Ok(AdvanceCommandStepResult::Rejected(
                GameCommandRejection::InvalidCommand,
            ));
        }

        let current = read_state(&mut tx, save_id).await?;
        if current.character.is_none() {
            tx.commit().await?;
            return Ok(AdvanceCommandStepResult::Rejected(
                GameCommandRejection::CharacterRequired,
            ));
        }

        let steps = if identity_state == CommandIdentityState::Matching {
            read_advance_command_steps(&mut tx, save_id, command.command_id.as_str()).await?
        } else {
            Vec::new()
        };
        validate_advance_steps(command, &steps)?;
        let expected_current_cursor = steps
            .last()
            .map(AdvanceCommandStepRow::after_cursor)
            .unwrap_or_else(|| GameCommandCursor::from(command.cursor));
        if GameCommandCursor::from(&current) != expected_current_cursor {
            tx.commit().await?;
            return Ok(AdvanceCommandStepResult::Rejected(
                GameCommandRejection::Busy,
            ));
        }

        let step_no = u32::try_from(steps.len())
            .context("advance command has too many stored steps")?
            .checked_add(1)
            .context("advance command step number overflowed")?;
        ensure!(
            step_no <= command.days,
            "completed advance steps have no final receipt"
        );
        let target_game_day = current
            .game_day
            .checked_add(1)
            .context("game day overflowed while advancing a command")?;
        if market.game_day != target_game_day {
            tx.commit().await?;
            return Ok(AdvanceCommandStepResult::Stale(Box::new(current)));
        }

        settle_daily_finance_state(
            &mut tx,
            Arc::clone(&self.finance_rules),
            &current,
            target_game_day,
            market,
        )
        .await?;

        if identity_state == CommandIdentityState::Missing {
            write_command_identity(&mut tx, save_id, &identity).await?;
        }

        let before_cursor = GameCommandCursor::from(&current);
        let after_cursor = GameCommandCursor {
            run_revision: before_cursor.run_revision,
            state_revision: before_cursor
                .state_revision
                .checked_add(1)
                .context("state revision overflowed while advancing a command")?,
            game_day: target_game_day,
        };
        let result = sqlx::query(
            "UPDATE save
             SET game_day = ?, state_revision = ?
             WHERE id = ? AND market_world_id = ? AND policy_set_id = ?
               AND run_revision = ? AND state_revision = ? AND game_day = ?",
        )
        .bind(after_cursor.game_day)
        .bind(after_cursor.state_revision)
        .bind(save_id)
        .bind(current.market_world_id)
        .bind(current.policy_set.id.get())
        .bind(before_cursor.run_revision)
        .bind(before_cursor.state_revision)
        .bind(before_cursor.game_day)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(AdvanceCommandStepResult::Stale(Box::new(current)));
        }

        write_advance_command_step(
            &mut tx,
            save_id,
            command.command_id.as_str(),
            step_no,
            before_cursor,
            after_cursor,
        )
        .await?;

        let receipt = if step_no == command.days {
            let receipt = AdvanceCommandReceipt {
                command_id: command.command_id.clone(),
                requested_days: command.days,
                initial_cursor: GameCommandCursor::from(command.cursor),
                committed_cursor: after_cursor,
                replayed: false,
            };
            write_game_command_receipt(
                &mut tx,
                GameCommandReceiptWrite {
                    save_id,
                    command_id: &command.command_id,
                    command_kind: COMMAND_KIND_ADVANCE,
                    payload_sha256: &fingerprint,
                    market_world_id: current.market_world_id,
                    committed_cursor: after_cursor,
                    result: &receipt,
                    ledger_transaction_id: None,
                },
            )
            .await?;
            Some(receipt)
        } else {
            None
        };

        let save = read_state(&mut tx, save_id).await?;
        ensure!(
            GameCommandCursor::from(&save) == after_cursor,
            "committed advance step cursor disagrees with the save"
        );
        tx.commit().await?;

        Ok(AdvanceCommandStepResult::Advanced {
            save: Box::new(save),
            receipt,
        })
    }
}

async fn close_career_run_for_new_run_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
) -> Result<()> {
    sqlx::query(
        "UPDATE job_application
         SET terminal_from_status = status, terminal_game_day = ?, status = 'closed'
         WHERE save_id = ? AND run_revision = ?
           AND status IN (
               'submitted', 'interviewAwaitingConfirmation',
               'interviewConfirmed', 'offered'
           )",
    )
    .bind(game_day)
    .bind(save_id)
    .bind(run_revision)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE job_invitation
         SET status = 'closed', decided_game_day = ?
         WHERE save_id = ? AND run_revision = ? AND status = 'open'",
    )
    .bind(game_day)
    .bind(save_id)
    .bind(run_revision)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE job_offer
         SET status = 'closed', decided_game_day = ?
         WHERE save_id = ? AND run_revision = ? AND status = 'pending'",
    )
    .bind(game_day)
    .bind(save_id)
    .bind(run_revision)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE employment_contract
         SET end_game_day = CASE
                 WHEN status = 'pendingStart' THEN start_game_day
                 ELSE last_credited_game_day + 1
             END,
             status = 'ended'
         WHERE save_id = ? AND run_revision = ?
           AND status IN ('pendingStart', 'active')",
    )
    .bind(save_id)
    .bind(run_revision)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE career_scheduled_action
         SET status = 'cancelled', cancelled_game_day = ?
         WHERE save_id = ? AND run_revision = ? AND status = 'pending'",
    )
    .bind(game_day)
    .bind(save_id)
    .bind(run_revision)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn settle_daily_finance_state(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    finance_rules: Arc<dyn FinanceRules>,
    current: &SaveState,
    target_game_day: u32,
    market: &MarketDay,
) -> Result<()> {
    let annual_tax_context = AnnualTaxRunContext {
        save_id: current.save_id,
        run_revision: current.run_revision,
        policy_set_id: current.policy_set.id.get(),
        game_day: target_game_day,
        market_date: market.market_date,
    };
    let market_world_product_bundle_id: Option<u64> =
        sqlx::query_scalar("SELECT market_world_product_bundle_id FROM save WHERE id = ?")
            .bind(current.save_id)
            .fetch_one(&mut **tx)
            .await?;
    let asset_context = M2dDailyAssetContext {
        save_id: current.save_id,
        market_world_id: current.market_world_id,
        policy_set_id: current.policy_set.id.get(),
        market_world_product_bundle_id,
        run_revision: current.run_revision,
        game_day: target_game_day,
    };

    ensure_monthly_bond_series_in_tx(tx, current.market_world_id, target_game_day).await?;
    validate_due_settlement_envelopes(tx, current.save_id, current.run_revision, target_game_day)
        .await?;
    pin_pension_opening_values_for_day(
        tx,
        current.save_id,
        current.run_revision,
        market.market_date,
    )
    .await?;
    prepare_annual_tax_year_boundary(tx, annual_tax_context).await?;
    advance_recruitment_lifecycle_in_tx(tx, current.save_id, current.run_revision, target_game_day)
        .await?;
    advance_career_activities_in_tx(tx, current.save_id, current.run_revision, target_game_day)
        .await?;
    advance_recruitment_actions_in_tx(tx, current.save_id, current.run_revision, target_game_day)
        .await?;

    if market_world_product_bundle_id.is_some() {
        mark_pension_positions_to_market(
            tx,
            current,
            target_game_day,
            market,
            market_world_product_bundle_id,
        )
        .await?;
        create_llx_entitlements_in_tx(tx, asset_context).await?;
    }

    settle_due_finance_items(tx, finance_rules, annual_tax_context, asset_context, market).await
}

async fn mark_pension_positions_to_market(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    current: &SaveState,
    target_game_day: u32,
    market: &MarketDay,
    market_world_product_bundle_id: Option<u64>,
) -> Result<()> {
    let llx_close_krw = market
        .m2
        .as_ref()
        .context("M2-D pension valuation is missing the LLX close")?
        .llx_close_krw;
    let assets = read_m2d_asset_snapshot_for_run_in_tx(
        tx,
        current.save_id,
        current.market_world_id,
        market_world_product_bundle_id,
        current.run_revision,
        target_game_day,
    )
    .await?;
    let due_pension_principal = read_due_pension_bond_principal_in_tx(
        tx,
        M2dDailyAssetContext {
            save_id: current.save_id,
            market_world_id: current.market_world_id,
            policy_set_id: current.policy_set.id.get(),
            market_world_product_bundle_id,
            run_revision: current.run_revision,
            game_day: target_game_day,
        },
    )
    .await?;
    let mut values = Vec::with_capacity(current.pension_accounts.len());
    for pension in &current.pension_accounts {
        let llx_market_value_krw = current
            .positions
            .iter()
            .filter(|position| {
                position.account_id.get() == pension.account_id.get()
                    && position.symbol == LLX_SYMBOL
            })
            .try_fold(0_i64, |total, position| {
                total
                    .checked_add(checked_llx_market_value(position.quantity, llx_close_krw)?)
                    .context("pension LLX positions overflowed during daily valuation")
            })?;
        let bond_market_value_krw = assets
            .bond_positions
            .iter()
            .filter(|position| position.account_id == pension.account_id)
            .try_fold(0_i64, |total, position| {
                total
                    .checked_add(position.market_value_krw)
                    .context("pension bond positions overflowed during daily valuation")
            })?;
        let due_principal_krw = due_pension_principal
            .iter()
            .find(|principal| principal.account_id == pension.account_id)
            .map_or(0, |principal| principal.due_principal_krw);
        values.push(PensionAccountMarketValue {
            account_id: pension.account_id,
            position_market_value_krw: llx_market_value_krw
                .checked_add(bond_market_value_krw)
                .and_then(|value| value.checked_add(due_principal_krw))
                .context("pension positions overflowed during daily valuation")?,
            risk_asset_value_krw: llx_market_value_krw,
        });
    }
    let plans = plan_pension_mark_to_market_in_tx(
        tx,
        current.save_id,
        current.run_revision,
        target_game_day,
        &values,
    )
    .await?;
    apply_pension_mark_to_market_plans_in_tx(
        tx,
        current.save_id,
        current.run_revision,
        target_game_day,
        &plans,
    )
    .await
}

async fn settle_due_finance_items(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    finance_rules: Arc<dyn FinanceRules>,
    annual_tax_context: AnnualTaxRunContext,
    asset_context: M2dDailyAssetContext,
    market: &MarketDay,
) -> Result<()> {
    let rows: Vec<(u64, u32, String)> = sqlx::query_as(
        "SELECT id, due_game_day, kind FROM scheduled_settlement
         WHERE save_id = ? AND run_revision = ? AND status = 'pending'
           AND due_game_day <= ?
         ORDER BY due_game_day, id",
    )
    .bind(asset_context.save_id)
    .bind(asset_context.run_revision)
    .bind(asset_context.game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.iter()
            .all(|(_, due_day, _)| *due_day == asset_context.game_day),
        "daily finance pipeline found an overdue item"
    );
    for (settlement_id, _, kind) in rows {
        match from_db_str::<SettlementKind>(&kind)? {
            SettlementKind::CmaInterest
            | SettlementKind::DepositMaturity
            | SettlementKind::SavingsInstallment
            | SettlementKind::SavingsMaturity => {
                settle_cash_product_by_id_in_tx(
                    tx,
                    CashProductSettlementInput {
                        rules: Arc::clone(&finance_rules),
                        save_id: asset_context.save_id,
                        run_revision: asset_context.run_revision,
                        policy_set_id: asset_context.policy_set_id,
                        target_game_day: asset_context.game_day,
                        market,
                        settlement_id,
                    },
                )
                .await?;
            }
            SettlementKind::BondCoupon | SettlementKind::BondMaturity => {
                settle_bond_cash_flow_by_id_in_tx(
                    tx,
                    finance_rules.as_ref(),
                    asset_context,
                    market.market_date,
                    settlement_id,
                )
                .await?;
            }
            SettlementKind::LlxDistribution => {
                settle_llx_entitlement_by_id_in_tx(
                    tx,
                    finance_rules.as_ref(),
                    asset_context,
                    market.market_date,
                    settlement_id,
                )
                .await?;
            }
            SettlementKind::FinancialIncomeFiling => {
                settle_annual_tax_filing_by_id(
                    tx,
                    finance_rules.as_ref(),
                    annual_tax_context,
                    settlement_id,
                )
                .await?;
            }
        }
    }
    Ok(())
}

async fn validate_due_settlement_envelopes(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    save_id: u64,
    run_revision: u32,
    target_game_day: u32,
) -> Result<()> {
    let rows: Vec<ScheduledSettlementRow> = sqlx::query_as(
        "SELECT id, due_game_day, kind, CAST(payload AS CHAR) AS payload_json,
                source_kind, source_id, occurrence, status
         FROM scheduled_settlement
         WHERE save_id = ? AND run_revision = ? AND status = 'pending'
           AND due_game_day <= ?
         ORDER BY due_game_day, id",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(target_game_day)
    .fetch_all(&mut **tx)
    .await?;
    for row in rows {
        ensure!(
            row.due_game_day == target_game_day,
            "daily finance pipeline found an overdue settlement"
        );
        let settlement = to_scheduled_settlement(save_id, run_revision, row)?;
        let valid_source = matches!(
            (settlement.kind, settlement.source.kind),
            (
                SettlementKind::CmaInterest,
                SettlementSourceKind::CmaAccount
            ) | (
                SettlementKind::DepositMaturity,
                SettlementSourceKind::DepositContract
            ) | (
                SettlementKind::SavingsInstallment,
                SettlementSourceKind::SavingsContract
            ) | (
                SettlementKind::SavingsMaturity,
                SettlementSourceKind::SavingsContract
            ) | (
                SettlementKind::BondCoupon,
                SettlementSourceKind::BondPosition
            ) | (
                SettlementKind::BondMaturity,
                SettlementSourceKind::BondPosition
            ) | (
                SettlementKind::LlxDistribution,
                SettlementSourceKind::IndexPosition
            ) | (
                SettlementKind::FinancialIncomeFiling,
                SettlementSourceKind::TaxYear
            )
        );
        ensure!(
            valid_source,
            "scheduled settlement kind and source disagree"
        );
        ensure!(
            settlement.payload.is_object(),
            "scheduled settlement payload must be an object"
        );
    }
    Ok(())
}

async fn settle_annual_tax_filing_by_id(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    finance_rules: &dyn FinanceRules,
    context: AnnualTaxRunContext,
    settlement_id: u64,
) -> Result<()> {
    let (wallet_cash_krw, aggregate_debt_krw): (i64, i64) =
        sqlx::query_as("SELECT cash_krw, debt_krw FROM save WHERE id = ? AND run_revision = ?")
            .bind(context.save_id)
            .bind(context.run_revision)
            .fetch_one(&mut **tx)
            .await?;
    let plan = plan_annual_tax_filing(
        tx,
        context,
        settlement_id,
        wallet_cash_krw,
        aggregate_debt_krw,
    )
    .await?;
    apply_annual_tax_filing(tx, finance_rules, context, &plan).await?;

    Ok(())
}

async fn lock_save(tx: &mut sqlx::Transaction<'_, sqlx::MySql>, save_id: u64) -> Result<()> {
    sqlx::query("SELECT id FROM save WHERE id = ? FOR UPDATE")
        .bind(save_id)
        .fetch_one(&mut **tx)
        .await?;
    Ok(())
}

async fn read_game_command_receipt(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    save_id: u64,
    command_id: &str,
) -> Result<Option<GameCommandReceiptRow>> {
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
    .await
    .context("failed to read a game command receipt")
}

pub(super) struct GameCommandReceiptWrite<'a, T> {
    pub save_id: u64,
    pub command_id: &'a CommandId,
    pub command_kind: &'a str,
    pub payload_sha256: &'a str,
    pub market_world_id: u64,
    pub committed_cursor: GameCommandCursor,
    pub result: &'a T,
    pub ledger_transaction_id: Option<u64>,
}

pub(super) async fn write_game_command_receipt<T: Serialize>(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    write: GameCommandReceiptWrite<'_, T>,
) -> Result<()> {
    let result_json =
        serde_json::to_string(write.result).context("failed to serialize a game command result")?;
    let insert = sqlx::query(
        "INSERT INTO command_receipt
             (save_id, run_revision, command_id, command_kind, payload_sha256,
              market_world_id, state_revision, game_day, result,
              ledger_transaction_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(write.save_id)
    .bind(write.committed_cursor.run_revision)
    .bind(write.command_id.as_str())
    .bind(write.command_kind)
    .bind(write.payload_sha256)
    .bind(write.market_world_id)
    .bind(write.committed_cursor.state_revision)
    .bind(write.committed_cursor.game_day)
    .bind(&result_json)
    .bind(write.ledger_transaction_id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        insert.rows_affected() == 1,
        "game command receipt was not inserted"
    );

    Ok(())
}

async fn read_advance_command_steps(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    save_id: u64,
    command_id: &str,
) -> Result<Vec<AdvanceCommandStepRow>> {
    sqlx::query_as(
        "SELECT step_no, before_run_revision, before_state_revision, before_game_day,
                after_run_revision, after_state_revision, after_game_day
         FROM advance_command_step
         WHERE save_id = ? AND command_id = ?
         ORDER BY step_no
         FOR SHARE",
    )
    .bind(save_id)
    .bind(command_id)
    .fetch_all(&mut **tx)
    .await
    .context("failed to read advance command steps")
}

async fn write_advance_command_step(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    save_id: u64,
    command_id: &str,
    step_no: u32,
    before: GameCommandCursor,
    after: GameCommandCursor,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO advance_command_step
             (save_id, command_id, step_no,
              before_run_revision, before_state_revision, before_game_day,
              after_run_revision, after_state_revision, after_game_day)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(save_id)
    .bind(command_id)
    .bind(step_no)
    .bind(before.run_revision)
    .bind(before.state_revision)
    .bind(before.game_day)
    .bind(after.run_revision)
    .bind(after.state_revision)
    .bind(after.game_day)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

fn validate_advance_steps(
    command: &ManualAdvanceCommand,
    steps: &[AdvanceCommandStepRow],
) -> Result<()> {
    ensure!(
        steps.len() <= usize::try_from(command.days).context("invalid requested days")?,
        "advance command has more steps than requested"
    );
    let mut expected_before = GameCommandCursor::from(command.cursor);
    for (index, step) in steps.iter().enumerate() {
        let expected_step_no = u32::try_from(index + 1).context("too many command steps")?;
        ensure!(
            step.step_no == expected_step_no && step.before_cursor() == expected_before,
            "advance command step chain is not contiguous"
        );
        let after = step.after_cursor();
        ensure!(
            after.run_revision == expected_before.run_revision
                && after.state_revision
                    == expected_before
                        .state_revision
                        .checked_add(1)
                        .context("stored advance command state revision overflowed")?
                && after.game_day
                    == expected_before
                        .game_day
                        .checked_add(1)
                        .context("stored advance command game day overflowed")?,
            "advance command step does not move exactly one day"
        );
        expected_before = after;
    }

    Ok(())
}

fn start_game_fingerprint(command: &StartGameCommand) -> Result<String> {
    let draft = &command.draft;
    let canonical = format!(
        concat!(
            "lifeledger.game.start.v1\n",
            "expectedRunRevision={}\n",
            "expectedStateRevision={}\n",
            "expectedGameDay={}\n",
            "name={}\n",
            "age={}\n",
            "gender={}\n",
            "military={}\n",
            "region={}\n",
            "background={}\n",
            "education={}\n",
            "careerYears={}\n",
            "certifications={}\n",
            "startingCashKrw={}\n",
            "studentLoanKrw={}\n",
            "creditLoanKrw={}\n",
            "health={}\n",
            "dependents={}"
        ),
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        serde_json::to_string(&draft.name)?,
        draft.age,
        to_db_str(&draft.gender)?,
        to_db_str(&draft.military)?,
        to_db_str(&draft.region)?,
        to_db_str(&draft.background)?,
        to_db_str(&draft.education)?,
        draft.career_years,
        draft.certifications,
        draft.starting_cash_krw,
        draft.student_loan_krw,
        draft.credit_loan_krw,
        to_db_str(&draft.health)?,
        draft.dependents,
    );

    Ok(sha256_hex(canonical.as_bytes()))
}

fn advance_command_fingerprint(command: &ManualAdvanceCommand) -> String {
    let canonical = format!(
        concat!(
            "lifeledger.game.advance.v1\n",
            "expectedRunRevision={}\n",
            "expectedStateRevision={}\n",
            "expectedGameDay={}\n",
            "days={}"
        ),
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.days,
    );

    sha256_hex(canonical.as_bytes())
}

fn sha256_hex(value: &[u8]) -> String {
    Sha256::digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

async fn create_default_account(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    save_id: u64,
    opened_game_day: u32,
) -> Result<AccountId> {
    sqlx::query(
        "INSERT INTO financial_account
             (save_id, run_revision, account_type, status, cash_krw, is_default,
              opened_game_day)
         SELECT id, run_revision, 'taxableBrokerage', 'open', 0, TRUE, ?
         FROM save WHERE id = ?",
    )
    .bind(opened_game_day)
    .bind(save_id)
    .execute(&mut **tx)
    .await?;

    let row: Option<(u64,)> = sqlx::query_as(
        "SELECT account.id
         FROM financial_account AS account
         INNER JOIN save ON save.id = account.save_id
         WHERE account.save_id = ? AND account.run_revision = save.run_revision
           AND account.is_default = TRUE",
    )
    .bind(save_id)
    .fetch_optional(&mut **tx)
    .await?;
    let account_id = row
        .and_then(|(id,)| AccountId::from_u64(id))
        .context("new run default account was not created")?;

    Ok(account_id)
}

async fn write_opening_ledger(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    rules: &dyn FinanceRules,
    save_id: u64,
    cash_krw: i64,
    debt_krw: i64,
    source_id: &str,
) -> Result<Option<u64>> {
    if cash_krw == 0 && debt_krw == 0 {
        return Ok(None);
    }
    if cash_krw < 0 || debt_krw < 0 {
        bail!("opening balances cannot be negative");
    }

    let row: Option<(u32, u64, u32)> =
        sqlx::query_as("SELECT run_revision, policy_set_id, game_day FROM save WHERE id = ?")
            .bind(save_id)
            .fetch_optional(&mut **tx)
            .await?;
    let (run_revision, policy_set_id, game_day) =
        row.context("save disappeared while writing its opening ledger")?;
    let run = RunId {
        save_id: ResourceId::from_u64(save_id),
        run_revision,
    };
    let mut postings = Vec::with_capacity(3);
    if cash_krw != 0 {
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::Wallet,
            financial_account_id: None,
            amount_krw: cash_krw,
        });
    }
    if debt_krw != 0 {
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::DebtPrincipal,
            financial_account_id: None,
            amount_krw: debt_krw
                .checked_neg()
                .context("opening debt cannot be represented in the ledger")?,
        });
    }
    let equity_krw = cash_krw
        .checked_sub(debt_krw)
        .and_then(i64::checked_neg)
        .context("opening equity overflowed")?;
    if equity_krw != 0 {
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::OpeningEquity,
            financial_account_id: None,
            amount_krw: equity_krw,
        });
    }
    let ledger = rules.create_ledger_transaction(LedgerTransactionDraft {
        policy: RunPolicyContext {
            run,
            policy_set_id: ResourceId::from_u64(policy_set_id),
        },
        source: LedgerSource {
            kind: LedgerSourceKind::M2OpeningBalance,
            source_id: source_id.to_owned(),
        },
        game_day,
        description: "새 런 기초 잔액".to_owned(),
        postings,
    })?;

    write_ledger_transaction(tx, &ledger).await.map(Some)
}

pub(super) async fn write_ledger_transaction(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    ledger: &LedgerTransaction,
) -> Result<u64> {
    let policy = ledger.policy();
    let result = sqlx::query(
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
    let transaction_id = result.last_insert_id();

    for (index, posting) in ledger.postings().iter().enumerate() {
        let posting_order = u16::try_from(index + 1).context("too many ledger postings")?;
        sqlx::query(
            "INSERT INTO ledger_posting
                 (save_id, run_revision, ledger_transaction_id, posting_order,
                  account_code, financial_account_id, amount_krw)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(policy.run.save_id.get())
        .bind(policy.run.run_revision)
        .bind(transaction_id)
        .bind(posting_order)
        .bind(to_db_str(&posting.account_code)?)
        .bind(posting.financial_account_id.map(ResourceId::get))
        .bind(posting.amount_krw)
        .execute(&mut **tx)
        .await?;
    }

    Ok(transaction_id)
}

async fn read_active_run_configuration(pool: &MySqlPool) -> Result<ActiveRunConfiguration> {
    type ActiveRunConfigurationRow = (u64, u64, u64, u64, Option<u64>, u64, u64);
    let row: Option<ActiveRunConfigurationRow> = sqlx::query_as(
        "SELECT market.world_id, market.assignment_revision,
                policy.policy_set_id, policy.assignment_revision, bundle.id,
                career.career_catalog_bundle_id, career.assignment_revision
         FROM market_world_assignment AS market
         CROSS JOIN policy_set_assignment AS policy
         CROSS JOIN career_catalog_assignment AS career
         LEFT JOIN market_world_product_bundle AS bundle
           ON bundle.market_world_id = market.world_id
          AND bundle.published_at IS NOT NULL
         WHERE market.assignment_key = 'newRun'
           AND policy.assignment_key = 'newRun'
           AND career.assignment_key = 'newRun'",
    )
    .fetch_optional(pool)
    .await?;

    row.map(
        |(
            world_id,
            world_revision,
            policy_set_id,
            policy_revision,
            product_bundle_id,
            career_bundle_id,
            career_revision,
        )| {
            ActiveRunConfiguration {
                market_world: ActiveMarketWorld {
                    world_id,
                    assignment_revision: world_revision,
                },
                policy_set: PolicySetAssignment {
                    policy_set_id: ResourceId::from_u64(policy_set_id),
                    assignment_revision: policy_revision,
                },
                product_bundle_id: product_bundle_id.map(ResourceId::from_u64),
                career_catalog: super::types::CareerCatalogAssignment {
                    bundle_id: ResourceId::from_u64(career_bundle_id),
                    assignment_revision: career_revision,
                },
            }
        },
    )
    .context("active market or policy assignment is missing")
}

async fn lock_active_run_configuration(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
) -> Result<ActiveRunConfiguration> {
    type ActiveRunConfigurationRow = (u64, u64, u64, u64, Option<u64>, u64, u64);
    let row: Option<ActiveRunConfigurationRow> = sqlx::query_as(
        "SELECT market.world_id, market.assignment_revision,
                policy.policy_set_id, policy.assignment_revision, bundle.id,
                career.career_catalog_bundle_id, career.assignment_revision
         FROM market_world_assignment AS market
         CROSS JOIN policy_set_assignment AS policy
         CROSS JOIN career_catalog_assignment AS career
         LEFT JOIN market_world_product_bundle AS bundle
           ON bundle.market_world_id = market.world_id
          AND bundle.published_at IS NOT NULL
         WHERE market.assignment_key = 'newRun'
           AND policy.assignment_key = 'newRun'
           AND career.assignment_key = 'newRun'
         FOR SHARE",
    )
    .fetch_optional(&mut **tx)
    .await?;

    row.map(
        |(
            world_id,
            world_revision,
            policy_set_id,
            policy_revision,
            product_bundle_id,
            career_bundle_id,
            career_revision,
        )| {
            ActiveRunConfiguration {
                market_world: ActiveMarketWorld {
                    world_id,
                    assignment_revision: world_revision,
                },
                policy_set: PolicySetAssignment {
                    policy_set_id: ResourceId::from_u64(policy_set_id),
                    assignment_revision: policy_revision,
                },
                product_bundle_id: product_bundle_id.map(ResourceId::from_u64),
                career_catalog: super::types::CareerCatalogAssignment {
                    bundle_id: ResourceId::from_u64(career_bundle_id),
                    assignment_revision: career_revision,
                },
            }
        },
    )
    .context("active market or policy assignment is missing")
}

#[async_trait]
impl TradingStore for MySqlSaveStore {
    async fn execute(&self, user_id: u64, order: &TradeOrder) -> Result<TradeStoreResult> {
        let mut tx = self.pool.begin().await?;
        let save_id = ensure_save(&mut tx, user_id).await?;

        lock_save(&mut tx, save_id).await?;

        let fingerprint = trade_fingerprint(order);
        let command_id = CommandId::parse(order.order_id.as_str().to_owned())
            .context("validated trade order has an invalid command ID")?;
        let identity = CommandIdentitySpec {
            command_id: &command_id,
            command_kind: COMMAND_KIND_TRADE,
            payload_sha256: &fingerprint,
            cursor: CommandCursor {
                expected_run_revision: order.expected_run_revision,
                expected_state_revision: order.expected_state_revision,
                expected_game_day: order.expected_game_day,
            },
        };
        match inspect_command_identity(&mut tx, save_id, &identity).await? {
            CommandIdentityState::Conflict => {
                tx.commit().await?;
                return Ok(TradeStoreResult::Rejected(
                    TradeFailure::idempotency_conflict(),
                ));
            }
            CommandIdentityState::Matching => {
                if let Some(receipt) =
                    read_trade_command_receipt(&mut tx, save_id, order.order_id.as_str()).await?
                {
                    ensure!(
                        receipt.command_kind == COMMAND_KIND_TRADE
                            && receipt.payload_sha256 == fingerprint,
                        "trade receipt disagrees with its command identity"
                    );
                }
                let stored = read_execution(&mut tx, save_id, order.order_id.as_str())
                    .await?
                    .context("trade command identity has no execution")?;
                ensure!(
                    stored.matches(order)?,
                    "trade command identity disagrees with its execution"
                );

                let execution = stored.to_execution(true)?;
                let save = read_state(&mut tx, save_id).await?;
                tx.commit().await?;

                return Ok(TradeStoreResult::Executed {
                    execution,
                    save: Box::new(save),
                });
            }
            CommandIdentityState::Missing => {}
        }

        if let Err(failure) = order.validate() {
            tx.commit().await?;
            return Ok(TradeStoreResult::Rejected(failure));
        }

        let current = read_state(&mut tx, save_id).await?;
        if current.character.is_none() {
            tx.commit().await?;
            return Ok(TradeStoreResult::Rejected(
                TradeFailure::character_required(),
            ));
        }
        if current.run_revision != order.expected_run_revision
            || current.state_revision != order.expected_state_revision
            || current.game_day != order.expected_game_day
        {
            tx.commit().await?;
            return Ok(TradeStoreResult::Rejected(TradeFailure::busy()));
        }

        let account: Option<LockedTradeAccountRow> = sqlx::query_as(
            "SELECT account_type, status, cash_krw
             FROM financial_account
             WHERE save_id = ? AND run_revision = ? AND id = ?
             FOR UPDATE",
        )
        .bind(save_id)
        .bind(current.run_revision)
        .bind(order.account_id.get())
        .fetch_optional(&mut *tx)
        .await?;
        let Some(account) = account else {
            tx.commit().await?;
            return Ok(TradeStoreResult::Rejected(TradeFailure::account_not_found()));
        };
        let account_status: FinancialAccountStatus = from_db_str(&account.status)?;
        if account_status != FinancialAccountStatus::Open {
            tx.commit().await?;
            return Ok(TradeStoreResult::Rejected(TradeFailure::account_closed()));
        }
        let account_type: FinancialAccountType = from_db_str(&account.account_type)?;
        let llx_terms = read_llx_trade_terms(&mut tx, save_id).await?;
        if !account_type_allows_llx(account_type, llx_terms.is_some()) {
            tx.commit().await?;
            return Ok(TradeStoreResult::Rejected(
                TradeFailure::account_type_not_allowed(),
            ));
        }
        if account.cash_krw < 0 {
            bail!("stored financial account cash is negative");
        }

        let selected_position: Option<PositionRow> = sqlx::query_as(
            "SELECT account_id, symbol, quantity, total_cost_basis_krw
             FROM asset_position
             WHERE save_id = ? AND account_id = ? AND symbol = ?
             FOR UPDATE",
        )
        .bind(save_id)
        .bind(order.account_id.get())
        .bind(LLX_SYMBOL)
        .fetch_optional(&mut *tx)
        .await?;
        let selected_position = selected_position.map(to_position).transpose()?;

        let market =
            read_current_market(&mut tx, current.market_world_id, current.game_day).await?;
        if !market.market_open {
            tx.commit().await?;
            return Ok(TradeStoreResult::Rejected(TradeFailure::market_closed()));
        }
        let charges =
            match llx_trade_charges(llx_terms, order.side, order.quantity, market.price_krw) {
                Ok(charges) => charges,
                Err(failure) => {
                    tx.commit().await?;
                    return Ok(TradeStoreResult::Rejected(failure));
                }
            };

        let mutation = match apply_trade_with_charges(
            order.account_id,
            account.cash_krw,
            selected_position.as_ref(),
            order.side,
            order.quantity,
            market.price_krw,
            charges,
        ) {
            Ok(mutation) => mutation,
            Err(failure) => {
                tx.commit().await?;
                return Ok(TradeStoreResult::Rejected(failure));
            }
        };
        let finance_account_id = ResourceId::from_u64(order.account_id.get());
        let llx_market_value_before_krw = checked_llx_market_value(
            selected_position
                .as_ref()
                .map_or(0, |position| position.quantity),
            market.price_krw,
        )?;
        let llx_market_value_after_krw = checked_llx_market_value(
            mutation
                .position
                .as_ref()
                .map_or(0, |position| position.quantity),
            market.price_krw,
        )?;
        let bond_market_value_krw = current
            .m2d_assets
            .bond_positions
            .iter()
            .filter(|position| position.account_id == finance_account_id)
            .try_fold(0_i64, |total, position| {
                total.checked_add(position.market_value_krw)
            })
            .context("bond positions overflowed the LLX account valuation")?;
        let position_market_value_before_krw = bond_market_value_krw
            .checked_add(llx_market_value_before_krw)
            .context("pre-trade position value overflowed")?;
        let position_market_value_after_krw = bond_market_value_krw
            .checked_add(llx_market_value_after_krw)
            .context("post-trade position value overflowed")?;
        let pension = current
            .pension_accounts
            .iter()
            .find(|pension| pension.account_id == finance_account_id);
        let (risk_asset_value_before_krw, risk_asset_value_after_krw, account_total_value_krw) =
            match pension {
                Some(pension) => {
                    let risk_after = pension
                        .risk_asset_value_krw
                        .checked_sub(llx_market_value_before_krw)
                        .and_then(|value| value.checked_add(llx_market_value_after_krw))
                        .context("LLX trade risk-asset value overflowed")?;
                    (
                        pension.risk_asset_value_krw,
                        risk_after,
                        pension.total_value_krw,
                    )
                }
                None => (0, 0, 0),
            };
        match prepare_llx_tax_account_trade_in_tx(
            &mut tx,
            LlxTaxAccountTradeInput {
                save_id,
                market_world_id: current.market_world_id,
                policy_set_id: current.policy_set.id.get(),
                run_revision: current.run_revision,
                game_day: current.game_day,
                account_id: finance_account_id,
                order_id: &command_id,
                side: match order.side {
                    OrderSide::Buy => AssetOrderSide::Buy,
                    OrderSide::Sell => AssetOrderSide::Sell,
                },
                realized_gain_loss_krw: mutation.realized_gain_loss_krw,
                execution_market_value_krw: mutation.gross_amount_krw,
                position_market_value_before_krw,
                position_market_value_after_krw,
                risk_asset_value_before_krw,
                risk_asset_value_after_krw,
                account_total_value_krw,
            },
        )
        .await?
        {
            LlxTaxAccountTradeResult::Applied => {}
            LlxTaxAccountTradeResult::Rejected(code) => {
                tx.rollback().await?;
                return Ok(TradeStoreResult::Rejected(llx_tax_account_trade_failure(
                    code,
                )?));
            }
        }
        let next_positions = positions_after_trade(&current.positions, &mutation.position, order);
        let portfolio = match value_portfolio(&next_positions, market.price_krw) {
            Ok(portfolio) => portfolio,
            Err(_) => {
                tx.commit().await?;
                return Ok(TradeStoreResult::Rejected(TradeFailure::invalid_order(
                    "주문 결과 금액이 처리 범위를 초과했습니다",
                )));
            }
        };
        let liquid_cash_krw = match total_cash_after_trade(
            current.cash_krw,
            &current.accounts,
            order.account_id,
            mutation.account_cash_krw,
        ) {
            Some(total) => total,
            None => {
                tx.commit().await?;
                return Ok(TradeStoreResult::Rejected(TradeFailure::invalid_order(
                    "주문 결과 금액이 처리 범위를 초과했습니다",
                )));
            }
        };
        let total_cash_krw = liquid_cash_krw
            .checked_add(current.active_product_principal_krw()?)
            .context("cash-product principal overflowed while executing an order")?;
        if checked_net_worth_krw(total_cash_krw, current.debt_krw, portfolio.market_value_krw)
            .is_err()
        {
            tx.commit().await?;
            return Ok(TradeStoreResult::Rejected(TradeFailure::invalid_order(
                "주문 결과 금액이 처리 범위를 초과했습니다",
            )));
        }
        let committed_state_revision = current
            .state_revision
            .checked_add(1)
            .context("state revision overflowed while executing an order")?;

        write_command_identity(&mut tx, save_id, &identity).await?;

        let account_update = sqlx::query(
            "UPDATE financial_account
             SET cash_krw = ?
             WHERE save_id = ? AND run_revision = ? AND id = ?
               AND status = 'open' AND cash_krw = ?",
        )
        .bind(mutation.account_cash_krw)
        .bind(save_id)
        .bind(current.run_revision)
        .bind(order.account_id.get())
        .bind(account.cash_krw)
        .execute(&mut *tx)
        .await?;
        if account_update.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(TradeStoreResult::Rejected(TradeFailure::busy()));
        }

        let update = sqlx::query(
            "UPDATE save
             SET state_revision = state_revision + 1
             WHERE id = ? AND market_world_id = ? AND policy_set_id = ? AND run_revision = ?
                 AND state_revision = ? AND game_day = ?",
        )
        .bind(save_id)
        .bind(current.market_world_id)
        .bind(current.policy_set.id.get())
        .bind(order.expected_run_revision)
        .bind(order.expected_state_revision)
        .bind(order.expected_game_day)
        .execute(&mut *tx)
        .await?;
        if update.rows_affected() == 0 {
            tx.rollback().await?;
            return Ok(TradeStoreResult::Rejected(TradeFailure::busy()));
        }

        write_position(
            &mut tx,
            save_id,
            mutation.account_id,
            mutation.position.as_ref(),
        )
        .await?;
        let ledger = create_trade_ledger(
            &*self.finance_rules,
            &current,
            order,
            mutation.gross_amount_krw,
            mutation.fee_krw,
            mutation.tax_krw,
            mutation.removed_cost_basis_krw,
        )?;
        let ledger_transaction_id = write_ledger_transaction(&mut tx, &ledger).await?;
        sqlx::query(
            "INSERT INTO trade_execution
                 (save_id, account_id, ledger_transaction_id, order_id,
                  expected_run_revision, expected_state_revision, expected_game_day,
                  run_revision, state_revision, game_day,
                  side, symbol, quantity, price_krw, gross_amount_krw,
                  fee_krw, tax_krw, removed_cost_basis_krw, realized_gain_loss_krw)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(save_id)
        .bind(order.account_id.get())
        .bind(ledger_transaction_id)
        .bind(order.order_id.as_str())
        .bind(order.expected_run_revision)
        .bind(order.expected_state_revision)
        .bind(order.expected_game_day)
        .bind(order.expected_run_revision)
        .bind(committed_state_revision)
        .bind(order.expected_game_day)
        .bind(to_db_str(&order.side)?)
        .bind(order.symbol())
        .bind(order.quantity)
        .bind(market.price_krw)
        .bind(mutation.gross_amount_krw)
        .bind(mutation.fee_krw)
        .bind(mutation.tax_krw)
        .bind(mutation.removed_cost_basis_krw)
        .bind(mutation.realized_gain_loss_krw)
        .execute(&mut *tx)
        .await?;

        let execution = TradeExecution {
            order_id: order.order_id.as_str().to_owned(),
            account_id: order.account_id,
            symbol: order.symbol().to_owned(),
            side: order.side,
            quantity: order.quantity,
            price_krw: market.price_krw,
            gross_amount_krw: mutation.gross_amount_krw,
            fee_krw: mutation.fee_krw,
            tax_krw: mutation.tax_krw,
            removed_cost_basis_krw: mutation.removed_cost_basis_krw,
            realized_gain_loss_krw: mutation.realized_gain_loss_krw,
            replayed: false,
        };
        write_trade_command_receipt(
            &mut tx,
            &current,
            committed_state_revision,
            &fingerprint,
            ledger_transaction_id,
            &execution,
        )
        .await?;
        let save = read_state(&mut tx, save_id).await?;
        tx.commit().await?;

        Ok(TradeStoreResult::Executed {
            execution,
            save: Box::new(save),
        })
    }
}

fn account_type_allows_llx(account_type: FinancialAccountType, has_m2_bundle: bool) -> bool {
    match account_type {
        FinancialAccountType::TaxableBrokerage => true,
        FinancialAccountType::IsaGeneral
        | FinancialAccountType::IsaLowIncome
        | FinancialAccountType::PensionSavings
        | FinancialAccountType::Irp => has_m2_bundle,
        FinancialAccountType::Cma | FinancialAccountType::KrxGold => false,
    }
}

fn checked_llx_market_value(quantity: u32, price_krw: i64) -> Result<i64> {
    ensure!(price_krw > 0, "LLX market price must be positive");
    i128::from(price_krw)
        .checked_mul(i128::from(quantity))
        .and_then(|value| i64::try_from(value).ok())
        .context("LLX market value overflowed")
}

fn llx_tax_account_trade_failure(code: FinanceFailureCode) -> Result<TradeFailure> {
    let failure = match code {
        FinanceFailureCode::AccountNotFound => TradeFailure::account_not_found(),
        FinanceFailureCode::AccountClosed => TradeFailure::account_closed(),
        FinanceFailureCode::AccountTypeNotAllowed => TradeFailure::account_type_not_allowed(),
        FinanceFailureCode::LimitExceeded | FinanceFailureCode::PositionLimit => {
            TradeFailure::position_limit()
        }
        FinanceFailureCode::Busy => TradeFailure::busy(),
        _ => bail!("LLX tax-account helper returned an unsupported rejection"),
    };
    Ok(failure)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LlxTradeTerms {
    buy_fee_ppm: i64,
    sell_fee_ppm: i64,
    sell_tax_ppm: i64,
}

#[derive(sqlx::FromRow)]
struct NullableLlxTradeTermsRow {
    buy_fee_ppm: Option<i64>,
    sell_fee_ppm: Option<i64>,
    sell_tax_ppm: Option<i64>,
}

async fn read_llx_trade_terms(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    save_id: u64,
) -> Result<Option<LlxTradeTerms>> {
    let row: Option<NullableLlxTradeTermsRow> = sqlx::query_as(
        "SELECT product.buy_fee_ppm, product.sell_fee_ppm,
                product.transaction_tax_ppm AS sell_tax_ppm
         FROM save
         LEFT JOIN market_world_product_bundle AS bundle
           ON bundle.id = save.market_world_product_bundle_id
          AND bundle.market_world_id = save.market_world_id
          AND bundle.published_at IS NOT NULL
         LEFT JOIN index_product_version AS product
           ON product.id = bundle.index_product_version_id
          AND product.published_at IS NOT NULL
         WHERE save.id = ?",
    )
    .bind(save_id)
    .fetch_optional(&mut **tx)
    .await?;
    let row = row.context("save disappeared while loading LLX product terms")?;
    let present = [
        row.buy_fee_ppm.is_some(),
        row.sell_fee_ppm.is_some(),
        row.sell_tax_ppm.is_some(),
    ];
    if present.iter().all(|value| !value) {
        return Ok(None);
    }
    ensure!(
        present.iter().all(|value| *value),
        "LLX product terms are partially populated"
    );
    let terms = LlxTradeTerms {
        buy_fee_ppm: row.buy_fee_ppm.context("LLX buy fee is missing")?,
        sell_fee_ppm: row.sell_fee_ppm.context("LLX sell fee is missing")?,
        sell_tax_ppm: row.sell_tax_ppm.context("LLX sell tax is missing")?,
    };
    ensure!(
        [terms.buy_fee_ppm, terms.sell_fee_ppm, terms.sell_tax_ppm]
            .into_iter()
            .all(|rate| (0..=1_000_000).contains(&rate)),
        "LLX product charge rate is outside the supported range"
    );

    Ok(Some(terms))
}

fn llx_trade_charges(
    terms: Option<LlxTradeTerms>,
    side: OrderSide,
    quantity: u32,
    price_krw: i64,
) -> Result<TradeCharges, TradeFailure> {
    let Some(terms) = terms else {
        return Ok(TradeCharges::default());
    };
    if quantity == 0 || price_krw <= 0 {
        return Err(TradeFailure::invalid_order(
            "주문 수량이나 체결가가 올바르지 않습니다",
        ));
    }
    let gross_amount_krw = i128::from(price_krw)
        .checked_mul(i128::from(quantity))
        .ok_or_else(|| TradeFailure::invalid_order("주문 금액이 처리 범위를 초과했습니다"))?;
    let (fee_rate_ppm, tax_rate_ppm) = match side {
        OrderSide::Buy => (terms.buy_fee_ppm, 0),
        OrderSide::Sell => (terms.sell_fee_ppm, terms.sell_tax_ppm),
    };
    let calculate = |rate_ppm: i64| {
        gross_amount_krw
            .checked_mul(i128::from(rate_ppm))
            .and_then(|amount| amount.checked_div(1_000_000))
            .and_then(|amount| i64::try_from(amount).ok())
            .ok_or_else(|| TradeFailure::invalid_order("주문 금액이 처리 범위를 초과했습니다"))
    };

    Ok(TradeCharges {
        fee_krw: calculate(fee_rate_ppm)?,
        tax_krw: calculate(tax_rate_ppm)?,
    })
}

fn positions_after_trade(
    current: &[PositionState],
    replacement: &Option<PositionState>,
    order: &TradeOrder,
) -> Vec<PositionState> {
    let mut positions = current
        .iter()
        .filter(|position| {
            position.account_id != order.account_id || position.symbol != order.symbol()
        })
        .cloned()
        .collect::<Vec<_>>();
    if let Some(position) = replacement {
        positions.push(position.clone());
    }
    positions
}

fn total_cash_after_trade(
    wallet_cash_krw: i64,
    accounts: &[FinancialAccount],
    selected_account_id: AccountId,
    selected_account_cash_krw: i64,
) -> Option<i64> {
    let mut selected_found = false;
    let total = accounts
        .iter()
        .try_fold(wallet_cash_krw, |total, account| {
            let cash_krw = if account.id.get() == selected_account_id.get() {
                selected_found = true;
                selected_account_cash_krw
            } else {
                account.cash_krw
            };
            total.checked_add(cash_krw)
        })?;

    selected_found.then_some(total)
}

fn create_trade_ledger(
    rules: &dyn FinanceRules,
    current: &SaveState,
    order: &TradeOrder,
    gross_amount_krw: i64,
    fee_krw: i64,
    tax_krw: i64,
    removed_cost_basis_krw: i64,
) -> Result<LedgerTransaction> {
    if gross_amount_krw <= 0 || fee_krw < 0 || tax_krw < 0 || removed_cost_basis_krw < 0 {
        bail!("trade amounts cannot create a valid ledger transaction");
    }

    let account_id = ResourceId::from_u64(order.account_id.get());
    let postings = match order.side {
        OrderSide::Buy => {
            let acquisition_cost_krw = gross_amount_krw
                .checked_add(fee_krw)
                .and_then(|amount| amount.checked_add(tax_krw))
                .context("trade acquisition cost overflowed")?;
            vec![
                LedgerPosting {
                    account_code: LedgerAccountCode::AccountCash,
                    financial_account_id: Some(account_id),
                    amount_krw: acquisition_cost_krw
                        .checked_neg()
                        .context("trade acquisition cost cannot be negated")?,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::ProductPrincipal,
                    financial_account_id: Some(account_id),
                    amount_krw: acquisition_cost_krw,
                },
            ]
        }
        OrderSide::Sell => {
            let net_proceeds_krw = gross_amount_krw
                .checked_sub(fee_krw)
                .and_then(|amount| amount.checked_sub(tax_krw))
                .context("trade net proceeds overflowed")?;
            ensure!(net_proceeds_krw >= 0, "trade charges exceed gross proceeds");
            let mut postings = vec![LedgerPosting {
                account_code: LedgerAccountCode::AccountCash,
                financial_account_id: Some(account_id),
                amount_krw: net_proceeds_krw,
            }];
            if removed_cost_basis_krw != 0 {
                postings.push(LedgerPosting {
                    account_code: LedgerAccountCode::ProductPrincipal,
                    financial_account_id: Some(account_id),
                    amount_krw: removed_cost_basis_krw
                        .checked_neg()
                        .context("removed cost basis cannot be negated")?,
                });
            }
            let realized_gain_loss_krw = removed_cost_basis_krw
                .checked_sub(net_proceeds_krw)
                .context("realized gain or loss overflowed")?;
            if realized_gain_loss_krw != 0 {
                postings.push(LedgerPosting {
                    account_code: LedgerAccountCode::RealizedGainLoss,
                    financial_account_id: None,
                    amount_krw: realized_gain_loss_krw,
                });
            }
            postings
        }
    };

    Ok(rules.create_ledger_transaction(LedgerTransactionDraft {
        policy: RunPolicyContext {
            run: RunId {
                save_id: ResourceId::from_u64(current.save_id),
                run_revision: current.run_revision,
            },
            policy_set_id: current.policy_set.id,
        },
        source: LedgerSource {
            kind: LedgerSourceKind::Trade,
            source_id: order.order_id.as_str().to_owned(),
        },
        game_day: current.game_day,
        description: match order.side {
            OrderSide::Buy => "LLX 매수",
            OrderSide::Sell => "LLX 매도",
        }
        .to_owned(),
        postings,
    })?)
}

fn trade_fingerprint(order: &TradeOrder) -> String {
    let side = match order.side {
        OrderSide::Buy => "buy",
        OrderSide::Sell => "sell",
    };
    let canonical = format!(
        "lifeledger.portfolio.order.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\naccountId={}\nside={}\nsymbol={}\nquantity={}",
        order.expected_run_revision,
        order.expected_state_revision,
        order.expected_game_day,
        order.account_id.get(),
        side,
        order.symbol(),
        order.quantity
    );
    Sha256::digest(canonical.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(sqlx::FromRow)]
struct CommandIdentityRow {
    command_kind: String,
    payload_sha256: String,
    initial_run_revision: u32,
    initial_state_revision: u64,
    initial_game_day: u32,
}

#[derive(sqlx::FromRow)]
struct GameCommandReceiptRow {
    command_kind: String,
    payload_sha256: String,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
    result_json: String,
    ledger_transaction_id: Option<u64>,
}

impl GameCommandReceiptRow {
    fn into_start_game_receipt(
        self,
        command: &StartGameCommand,
        replayed: bool,
    ) -> Result<StartGameReceipt> {
        let fingerprint = start_game_fingerprint(command)?;
        let mut stored: StartGameReceipt = serde_json::from_str(&self.result_json)
            .context("stored start-game command result is invalid")?;
        let expected_run_revision = command
            .cursor
            .expected_run_revision
            .checked_add(1)
            .context("stored start-game run revision overflowed")?;
        let expected_ledger = command.draft.starting_cash_krw != 0
            || command.draft.student_loan_krw != 0
            || command.draft.credit_loan_krw != 0;
        ensure!(
            self.command_kind == COMMAND_KIND_START_GAME
                && self.payload_sha256 == fingerprint
                && stored.command_id == command.command_id
                && !stored.replayed
                && stored.committed_cursor
                    == (GameCommandCursor {
                        run_revision: expected_run_revision,
                        state_revision: 0,
                        game_day: 0,
                    })
                && stored.committed_cursor.run_revision == self.run_revision
                && stored.committed_cursor.state_revision == self.state_revision
                && stored.committed_cursor.game_day == self.game_day
                && self.ledger_transaction_id.is_some() == expected_ledger,
            "stored start-game receipt disagrees with its command result"
        );
        stored.replayed = replayed;

        Ok(stored)
    }

    fn into_advance_receipt(
        self,
        command: &ManualAdvanceCommand,
        replayed: bool,
    ) -> Result<AdvanceCommandReceipt> {
        let fingerprint = advance_command_fingerprint(command);
        let mut stored: AdvanceCommandReceipt = serde_json::from_str(&self.result_json)
            .context("stored advance command result is invalid")?;
        let initial_cursor = GameCommandCursor::from(command.cursor);
        let expected_committed_cursor = GameCommandCursor {
            run_revision: initial_cursor.run_revision,
            state_revision: initial_cursor
                .state_revision
                .checked_add(u64::from(command.days))
                .context("stored advance state revision overflowed")?,
            game_day: initial_cursor
                .game_day
                .checked_add(command.days)
                .context("stored advance game day overflowed")?,
        };
        ensure!(
            self.command_kind == COMMAND_KIND_ADVANCE
                && self.payload_sha256 == fingerprint
                && stored.command_id == command.command_id
                && stored.requested_days == command.days
                && stored.initial_cursor == initial_cursor
                && stored.committed_cursor == expected_committed_cursor
                && !stored.replayed
                && stored.committed_cursor.run_revision == self.run_revision
                && stored.committed_cursor.state_revision == self.state_revision
                && stored.committed_cursor.game_day == self.game_day
                && self.ledger_transaction_id.is_none(),
            "stored advance receipt disagrees with its command result"
        );
        stored.replayed = replayed;

        Ok(stored)
    }
}

#[derive(sqlx::FromRow)]
struct AdvanceCommandStepRow {
    step_no: u32,
    before_run_revision: u32,
    before_state_revision: u64,
    before_game_day: u32,
    after_run_revision: u32,
    after_state_revision: u64,
    after_game_day: u32,
}

impl AdvanceCommandStepRow {
    fn before_cursor(&self) -> GameCommandCursor {
        GameCommandCursor {
            run_revision: self.before_run_revision,
            state_revision: self.before_state_revision,
            game_day: self.before_game_day,
        }
    }

    fn after_cursor(&self) -> GameCommandCursor {
        GameCommandCursor {
            run_revision: self.after_run_revision,
            state_revision: self.after_state_revision,
            game_day: self.after_game_day,
        }
    }
}

#[derive(sqlx::FromRow)]
struct LockedTradeAccountRow {
    account_type: String,
    status: String,
    cash_krw: i64,
}

#[derive(sqlx::FromRow)]
struct TradeCommandReceiptRow {
    command_kind: String,
    payload_sha256: String,
}

#[derive(sqlx::FromRow)]
struct SaveRow {
    market_world_id: u64,
    market_world_product_bundle_id: Option<u64>,
    policy_set_id: u64,
    policy_key: String,
    basis_date: String,
    policy_sealed: bool,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
    cash_krw: i64,
    debt_krw: i64,
}

#[derive(sqlx::FromRow)]
struct PositionRow {
    account_id: u64,
    symbol: String,
    quantity: u32,
    total_cost_basis_krw: i64,
}

#[derive(sqlx::FromRow)]
struct FinancialAccountRow {
    id: u64,
    account_type: String,
    status: String,
    cash_krw: i64,
    is_default: bool,
}

#[derive(sqlx::FromRow)]
struct ScheduledSettlementRow {
    id: u64,
    due_game_day: u32,
    kind: String,
    payload_json: String,
    source_kind: String,
    source_id: String,
    occurrence: u32,
    status: String,
}

#[derive(sqlx::FromRow)]
struct CurrentMarketRow {
    market_open: bool,
    price_krw: i64,
}

#[derive(sqlx::FromRow)]
struct TradeExecutionRow {
    order_id: String,
    account_id: u64,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    side: String,
    symbol: String,
    quantity: u32,
    price_krw: i64,
    gross_amount_krw: i64,
    fee_krw: i64,
    tax_krw: i64,
    removed_cost_basis_krw: i64,
    realized_gain_loss_krw: Option<i64>,
}

impl TradeExecutionRow {
    fn matches(&self, order: &TradeOrder) -> Result<bool> {
        let stored_side: OrderSide = from_db_str(&self.side)?;

        Ok(self.order_id == order.order_id.as_str()
            && self.account_id == order.account_id.get()
            && self.expected_run_revision == order.expected_run_revision
            && self.expected_state_revision == order.expected_state_revision
            && self.expected_game_day == order.expected_game_day
            && stored_side == order.side
            && self.symbol == order.symbol()
            && self.quantity == order.quantity)
    }

    fn to_execution(&self, replayed: bool) -> Result<TradeExecution> {
        let side: OrderSide = from_db_str(&self.side)?;
        ensure!(
            self.fee_krw >= 0 && self.tax_krw >= 0,
            "stored trade execution has negative charges"
        );
        let derived_realized_gain_loss_krw = match side {
            OrderSide::Buy => {
                ensure!(
                    self.removed_cost_basis_krw == 0,
                    "stored buy execution removed cost basis"
                );
                0
            }
            OrderSide::Sell => self
                .gross_amount_krw
                .checked_sub(self.removed_cost_basis_krw)
                .and_then(|amount| amount.checked_sub(self.fee_krw))
                .and_then(|amount| amount.checked_sub(self.tax_krw))
                .context("stored realized gain or loss overflowed")?,
        };
        if let Some(stored) = self.realized_gain_loss_krw {
            ensure!(
                stored == derived_realized_gain_loss_krw,
                "stored trade realized gain or loss does not reconcile"
            );
        }

        Ok(TradeExecution {
            order_id: self.order_id.clone(),
            account_id: AccountId::from_u64(self.account_id)
                .context("stored trade execution account id is zero")?,
            symbol: self.symbol.clone(),
            side,
            quantity: self.quantity,
            price_krw: self.price_krw,
            gross_amount_krw: self.gross_amount_krw,
            fee_krw: self.fee_krw,
            tax_krw: self.tax_krw,
            removed_cost_basis_krw: self.removed_cost_basis_krw,
            realized_gain_loss_krw: derived_realized_gain_loss_krw,
            replayed,
        })
    }
}

#[derive(sqlx::FromRow)]
struct CharacterRow {
    name: String,
    age: u32,
    gender: String,
    military: String,
    region: String,
    background: String,
    education: String,
    career_years: u32,
    certifications: u32,
    health: String,
    dependents: u32,
}

pub(super) async fn read_state(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    save_id: u64,
) -> Result<SaveState> {
    let save: Option<SaveRow> = sqlx::query_as(
        "SELECT save.market_world_id, save.market_world_product_bundle_id,
                save.policy_set_id, policy_set.policy_key,
                DATE_FORMAT(policy_set.basis_date, '%Y-%m-%d') AS basis_date,
                policy_set.sealed_at IS NOT NULL AS policy_sealed,
                save.run_revision, save.state_revision, save.game_day,
                save.cash_krw, save.debt_krw
         FROM save
         INNER JOIN policy_set ON policy_set.id = save.policy_set_id
         WHERE save.id = ?",
    )
    .bind(save_id)
    .fetch_optional(&mut **tx)
    .await?;

    let Some(save) = save else {
        bail!("save {save_id} disappeared");
    };

    let character: Option<CharacterRow> = sqlx::query_as(
        "SELECT name, age, gender, military, region, background,
                education, career_years, certifications, health, dependents
         FROM `character` WHERE save_id = ?",
    )
    .bind(save_id)
    .fetch_optional(&mut **tx)
    .await?;

    let character = match character {
        Some(row) => Some(to_character(row, save.cash_krw, save.debt_krw)?),
        None => None,
    };
    let account_rows: Vec<FinancialAccountRow> = sqlx::query_as(
        "SELECT id, account_type, status, cash_krw, is_default
         FROM financial_account
         WHERE save_id = ? AND run_revision = ?
         ORDER BY id
         LIMIT 33",
    )
    .bind(save_id)
    .bind(save.run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        account_rows.len() <= 32,
        "financial-account snapshot exceeded its account bound"
    );
    let accounts = account_rows
        .into_iter()
        .map(|row| to_financial_account(save_id, save.run_revision, row))
        .collect::<Result<Vec<_>>>()?;
    let position_rows: Vec<PositionRow> = sqlx::query_as(
        "SELECT position.account_id, position.symbol, position.quantity,
                position.total_cost_basis_krw
         FROM asset_position AS position
         INNER JOIN financial_account AS account
           ON account.save_id = position.save_id
          AND account.id = position.account_id
         WHERE position.save_id = ? AND account.run_revision = ?
         ORDER BY position.account_id, position.symbol",
    )
    .bind(save_id)
    .bind(save.run_revision)
    .fetch_all(&mut **tx)
    .await?;
    let positions = position_rows
        .into_iter()
        .map(to_position)
        .collect::<Result<Vec<_>>>()?;
    let settlement_rows: Vec<ScheduledSettlementRow> = sqlx::query_as(
        "SELECT id, due_game_day, kind, CAST(payload AS CHAR) AS payload_json,
                source_kind, source_id, occurrence, status
         FROM scheduled_settlement
         WHERE save_id = ? AND run_revision = ? AND status = 'pending'
         ORDER BY due_game_day, id
         LIMIT 20",
    )
    .bind(save_id)
    .bind(save.run_revision)
    .fetch_all(&mut **tx)
    .await?;
    let pending_settlements = settlement_rows
        .into_iter()
        .map(|row| to_scheduled_settlement(save_id, save.run_revision, row))
        .collect::<Result<Vec<_>>>()?;
    let cash_products = read_cash_product_state(
        tx,
        save_id,
        save.market_world_id,
        save.policy_set_id,
        save.run_revision,
        save.game_day,
    )
    .await?;
    let current_market_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM market_daily WHERE world_id = ? AND game_day = ?
         )",
    )
    .bind(save.market_world_id)
    .bind(save.game_day)
    .fetch_one(&mut **tx)
    .await?;
    let m2d_assets = if !current_market_exists && character.is_none() {
        crate::finance::M2dAssetSnapshot::default()
    } else {
        read_m2d_asset_snapshot_for_run_in_tx(
            tx,
            save_id,
            save.market_world_id,
            save.market_world_product_bundle_id,
            save.run_revision,
            save.game_day,
        )
        .await?
    };
    let tax_accounts = read_tax_account_state(
        tx,
        TaxAccountStateInput {
            save_id,
            market_world_id: save.market_world_id,
            policy_set_id: save.policy_set_id,
            run_revision: save.run_revision,
            game_day: save.game_day,
            cash_contracts: &cash_products.cash_contracts,
            bond_positions: &m2d_assets.bond_positions,
        },
    )
    .await?;
    let market_date: time::Date = sqlx::query_scalar(
        "SELECT COALESCE(daily.market_date, DATE_ADD(world.start_date, INTERVAL save.game_day DAY))
         FROM save
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         LEFT JOIN market_daily AS daily
           ON daily.world_id = save.market_world_id
          AND daily.game_day = save.game_day
         WHERE save.id = ?",
    )
    .bind(save_id)
    .fetch_one(&mut **tx)
    .await
    .context("failed to resolve the current market date for annual tax")?;
    let tax_year = u16::try_from(market_date.year())
        .context("current market year is outside the tax-year contract")?;
    let annual_tax_context = AnnualTaxRunContext {
        save_id,
        run_revision: save.run_revision,
        policy_set_id: save.policy_set_id,
        game_day: save.game_day,
        market_date,
    };
    let current_annual_tax_year = read_annual_tax_year(tx, annual_tax_context, tax_year).await?;
    let latest_financial_income_assessment =
        read_latest_annual_tax_assessment(tx, annual_tax_context).await?;
    let career = read_career_snapshot_in_tx(tx, save_id, save.run_revision, save.game_day).await?;
    Ok(SaveState {
        save_id,
        market_world_id: save.market_world_id,
        policy_set: PolicySet {
            id: ResourceId::from_u64(save.policy_set_id),
            key: save.policy_key,
            basis_date: save.basis_date,
            sealed: save.policy_sealed,
        },
        run_revision: save.run_revision,
        state_revision: save.state_revision,
        game_day: save.game_day,
        cash_krw: save.cash_krw,
        debt_krw: save.debt_krw,
        accounts,
        positions,
        pending_settlements,
        cma_accounts: cash_products.cma_accounts,
        cash_contracts: cash_products.cash_contracts,
        deposit_protection: cash_products.deposit_protection,
        current_financial_income_year: cash_products.current_financial_income_year,
        current_annual_tax_year,
        latest_financial_income_assessment,
        m2d_assets,
        isa_accounts: tax_accounts.isa_accounts,
        pension_accounts: tax_accounts.pension_accounts,
        career,
        character,
    })
}

async fn read_current_market(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    world_id: u64,
    game_day: u32,
) -> Result<CurrentMarketRow> {
    let market: Option<CurrentMarketRow> = sqlx::query_as(
        "SELECT market_open, COALESCE(llx_close_krw, equity_close_krw) AS price_krw
         FROM market_daily WHERE world_id = ? AND game_day = ?",
    )
    .bind(world_id)
    .bind(game_day)
    .fetch_optional(&mut **tx)
    .await?;

    market.with_context(|| {
        format!("market day {game_day} for world {world_id} was not prepared before trading")
    })
}

async fn read_execution(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    save_id: u64,
    order_id: &str,
) -> Result<Option<TradeExecutionRow>> {
    sqlx::query_as(
        "SELECT order_id, account_id, expected_run_revision, expected_state_revision,
                expected_game_day, side, symbol, quantity, price_krw, gross_amount_krw,
                fee_krw, tax_krw, removed_cost_basis_krw, realized_gain_loss_krw
         FROM trade_execution WHERE save_id = ? AND order_id = ?",
    )
    .bind(save_id)
    .bind(order_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to look up the order idempotency record")
}

async fn read_trade_command_receipt(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    save_id: u64,
    command_id: &str,
) -> Result<Option<TradeCommandReceiptRow>> {
    sqlx::query_as(
        "SELECT command_kind, payload_sha256
         FROM command_receipt
         WHERE save_id = ? AND command_id = ?",
    )
    .bind(save_id)
    .bind(command_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to look up the command receipt")
}

async fn write_trade_command_receipt(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    current: &SaveState,
    committed_state_revision: u64,
    fingerprint: &str,
    ledger_transaction_id: u64,
    execution: &TradeExecution,
) -> Result<()> {
    let result =
        serde_json::to_string(execution).context("failed to serialize the trade command result")?;
    sqlx::query(
        "INSERT INTO command_receipt
             (save_id, run_revision, command_id, command_kind, payload_sha256,
              market_world_id, state_revision, game_day, result,
              ledger_transaction_id)
         VALUES (?, ?, ?, 'trade', ?, ?, ?, ?, ?, ?)",
    )
    .bind(current.save_id)
    .bind(current.run_revision)
    .bind(&execution.order_id)
    .bind(fingerprint)
    .bind(current.market_world_id)
    .bind(committed_state_revision)
    .bind(current.game_day)
    .bind(result)
    .bind(ledger_transaction_id)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn write_position(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    save_id: u64,
    account_id: AccountId,
    position: Option<&PositionState>,
) -> Result<()> {
    match position {
        Some(position) => {
            sqlx::query(
                "INSERT INTO asset_position
                     (save_id, account_id, symbol, quantity, total_cost_basis_krw)
                 VALUES (?, ?, ?, ?, ?)
                 ON DUPLICATE KEY UPDATE
                     quantity = ?, total_cost_basis_krw = ?",
            )
            .bind(save_id)
            .bind(account_id.get())
            .bind(&position.symbol)
            .bind(position.quantity)
            .bind(position.cost_basis_krw)
            .bind(position.quantity)
            .bind(position.cost_basis_krw)
            .execute(&mut **tx)
            .await?;
        }
        None => {
            sqlx::query(
                "DELETE FROM asset_position
                 WHERE save_id = ? AND account_id = ? AND symbol = ?",
            )
            .bind(save_id)
            .bind(account_id.get())
            .bind(LLX_SYMBOL)
            .execute(&mut **tx)
            .await?;
        }
    }

    Ok(())
}

fn to_position(row: PositionRow) -> Result<PositionState> {
    if row.symbol != LLX_SYMBOL {
        bail!("unsupported position symbol stored: {}", row.symbol);
    }
    if !(1..=crate::trading::MAX_TRADE_QUANTITY).contains(&row.quantity) {
        bail!("invalid LLX position quantity stored: {}", row.quantity);
    }
    if row.total_cost_basis_krw <= 0 {
        bail!(
            "invalid LLX position cost basis stored: {}",
            row.total_cost_basis_krw
        );
    }

    let account_id = AccountId::from_u64(row.account_id).context("stored account id is zero")?;

    Ok(PositionState {
        account_id,
        symbol: row.symbol,
        quantity: row.quantity,
        cost_basis_krw: row.total_cost_basis_krw,
    })
}

fn to_financial_account(
    save_id: u64,
    run_revision: u32,
    row: FinancialAccountRow,
) -> Result<FinancialAccount> {
    if row.cash_krw < 0 {
        bail!("stored financial account cash is negative");
    }

    Ok(FinancialAccount {
        id: ResourceId::from_u64(row.id),
        run: RunId {
            save_id: ResourceId::from_u64(save_id),
            run_revision,
        },
        account_type: from_db_str(&row.account_type)?,
        status: from_db_str(&row.status)?,
        cash_krw: row.cash_krw,
        is_default: row.is_default,
    })
}

fn to_scheduled_settlement(
    save_id: u64,
    run_revision: u32,
    row: ScheduledSettlementRow,
) -> Result<ScheduledSettlement> {
    Ok(ScheduledSettlement {
        id: ResourceId::from_u64(row.id),
        run: RunId {
            save_id: ResourceId::from_u64(save_id),
            run_revision,
        },
        due_game_day: row.due_game_day,
        kind: from_db_str::<SettlementKind>(&row.kind)?,
        source: SettlementSource {
            kind: from_db_str::<SettlementSourceKind>(&row.source_kind)?,
            source_id: row.source_id,
            occurrence: row.occurrence,
        },
        status: from_db_str::<SettlementStatus>(&row.status)?,
        payload: serde_json::from_str(&row.payload_json)
            .context("stored settlement payload is invalid JSON")?,
    })
}

/// Cash and debt live on the save, not the character row: they change as the game runs.
fn to_character(row: CharacterRow, cash_krw: i64, debt_krw: i64) -> Result<Character> {
    Ok(Character {
        name: row.name,
        age: row.age,
        gender: from_db_str(&row.gender)?,
        military: from_db_str(&row.military)?,
        region: from_db_str(&row.region)?,
        background: from_db_str(&row.background)?,
        education: from_db_str(&row.education)?,
        career_years: row.career_years,
        certifications: row.certifications,
        cash_krw,
        debt_krw,
        health: from_db_str(&row.health)?,
        dependents: row.dependents,
    })
}

/// Enum -> column string, reusing the serde (camelCase) representation.
fn to_db_str<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value)? {
        Value::String(text) => Ok(text),
        other => bail!("value is not storable as a string: {other}"),
    }
}

/// Column string -> enum.
fn from_db_str<T: DeserializeOwned>(raw: &str) -> Result<T> {
    serde_json::from_value(Value::String(raw.to_owned()))
        .with_context(|| format!("unknown value stored: {raw}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{
        CharacterDraft, Education, FamilyBackground, Gender, Health, MilitaryStatus, Region,
    };
    use crate::finance::{CommandCursor, CommandId};
    use crate::trading::TradeOrderRequest;

    fn given_start_game_command() -> StartGameCommand {
        let draft = CharacterDraft {
            name: " 테스터 ".to_owned(),
            age: 25,
            gender: Gender::Other,
            military: MilitaryStatus::Exempted,
            region: Region::CapitalArea,
            background: FamilyBackground::Independent,
            education: Education::Bachelor,
            career_years: 1,
            certifications: 2,
            starting_cash_krw: 10_000_000,
            student_loan_krw: 2_000_000,
            credit_loan_krw: 1_000_000,
            health: Health::Normal,
            dependents: 0,
        };
        StartGameCommand {
            command_id: CommandId::parse("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2")
                .expect("표준 UUID여야 한다"),
            cursor: CommandCursor {
                expected_run_revision: 3,
                expected_state_revision: 42,
                expected_game_day: 17,
            },
            draft,
        }
    }

    fn given_advance_command(days: u32) -> ManualAdvanceCommand {
        ManualAdvanceCommand {
            command_id: CommandId::parse("b6a1cc9d-3c87-44a9-aebe-9ff46677f043")
                .expect("표준 UUID여야 한다"),
            cursor: CommandCursor {
                expected_run_revision: 3,
                expected_state_revision: 42,
                expected_game_day: 17,
            },
            days,
        }
    }

    mod context_a_start_game_payload_is_fingerprinted {
        use super::*;

        #[test]
        fn given_the_same_raw_draft_when_hashed_then_the_fingerprint_is_stable() {
            let command = given_start_game_command();

            let first = start_game_fingerprint(&command).expect("지문을 만들 수 있어야 한다");
            let second = start_game_fingerprint(&command).expect("지문을 만들 수 있어야 한다");

            assert_eq!(first, second);
            assert_eq!(first.len(), 64);
        }

        #[test]
        fn given_equal_total_debt_with_a_different_split_when_hashed_then_it_conflicts() {
            let command = given_start_game_command();
            let mut changed = command.clone();
            changed.draft.student_loan_krw = 1_000_000;
            changed.draft.credit_loan_krw = 2_000_000;

            let original = start_game_fingerprint(&command).expect("지문을 만들 수 있어야 한다");
            let changed_hash =
                start_game_fingerprint(&changed).expect("지문을 만들 수 있어야 한다");

            assert_eq!(
                command.draft.student_loan_krw + command.draft.credit_loan_krw,
                changed.draft.student_loan_krw + changed.draft.credit_loan_krw
            );
            assert_ne!(original, changed_hash);
        }

        #[test]
        fn given_a_name_that_normalizes_equally_when_hashed_then_the_raw_payload_still_conflicts() {
            let command = given_start_game_command();
            let mut changed = command.clone();
            changed.draft.name = "테스터".to_owned();

            let original = start_game_fingerprint(&command).expect("지문을 만들 수 있어야 한다");
            let changed_hash =
                start_game_fingerprint(&changed).expect("지문을 만들 수 있어야 한다");

            assert_eq!(command.draft.name.trim(), changed.draft.name.trim());
            assert_ne!(original, changed_hash);
        }
    }

    mod context_advance_steps_are_validated {
        use super::*;

        #[test]
        fn given_a_contiguous_partial_chain_when_validated_then_resume_is_allowed() {
            let command = given_advance_command(3);
            let steps = vec![AdvanceCommandStepRow {
                step_no: 1,
                before_run_revision: 3,
                before_state_revision: 42,
                before_game_day: 17,
                after_run_revision: 3,
                after_state_revision: 43,
                after_game_day: 18,
            }];

            let result = validate_advance_steps(&command, &steps);

            assert!(result.is_ok());
        }

        #[test]
        fn given_a_gap_in_the_chain_when_validated_then_corrupt_progress_is_rejected() {
            let command = given_advance_command(3);
            let steps = vec![AdvanceCommandStepRow {
                step_no: 2,
                before_run_revision: 3,
                before_state_revision: 42,
                before_game_day: 17,
                after_run_revision: 3,
                after_state_revision: 43,
                after_game_day: 18,
            }];

            let result = validate_advance_steps(&command, &steps);

            assert!(result.is_err());
        }
    }

    /// Database round trips are out of scope for tests. What can silently go wrong is the
    /// enum <-> column string conversion, so that is what is covered here.
    mod context_enum_columns_round_trip {
        use super::*;

        #[test]
        fn given_an_enum_when_stored_and_read_back_then_it_is_unchanged() {
            let military = MilitaryStatus::Alternative;

            let stored = to_db_str(&military).expect("저장 표현으로 바꿀 수 있어야 한다");
            let restored: MilitaryStatus = from_db_str(&stored).expect("되읽을 수 있어야 한다");

            assert_eq!(restored, military);
        }

        #[test]
        fn given_a_multi_word_variant_when_stored_then_it_uses_the_serde_name() {
            let region = Region::CapitalArea;

            let stored = to_db_str(&region).expect("저장 표현으로 바꿀 수 있어야 한다");

            assert_eq!(stored, "capitalArea");
        }
    }

    mod context_a_stored_value_is_not_a_known_variant {
        use super::*;

        #[test]
        fn given_an_unknown_string_when_read_then_it_fails_instead_of_guessing() {
            let stored = "graduateSchool";

            let restored = from_db_str::<Education>(stored);

            assert!(restored.is_err());
        }
    }

    mod context_a_character_row_is_assembled {
        use super::*;

        fn given_row() -> CharacterRow {
            CharacterRow {
                name: "테스터".to_owned(),
                age: 25,
                gender: "male".to_owned(),
                military: "completed".to_owned(),
                region: "capitalArea".to_owned(),
                background: "independent".to_owned(),
                education: "bachelor".to_owned(),
                career_years: 1,
                certifications: 1,
                health: "normal".to_owned(),
                dependents: 0,
            }
        }

        #[test]
        fn given_a_row_when_assembled_then_enums_come_from_their_columns() {
            let row = given_row();

            let character = to_character(row, 10_000_000, 0).expect("조립할 수 있어야 한다");

            assert_eq!(
                (
                    character.gender,
                    character.military,
                    character.region,
                    character.background,
                    character.education
                ),
                (
                    Gender::Male,
                    MilitaryStatus::Completed,
                    Region::CapitalArea,
                    FamilyBackground::Independent,
                    Education::Bachelor
                )
            );
        }

        #[test]
        fn given_a_row_when_assembled_then_money_comes_from_the_save_not_the_row() {
            let row = given_row();

            let character = to_character(row, 7_000_000, 3_000_000).expect("조립할 수 있어야 한다");

            assert_eq!(
                (character.cash_krw, character.debt_krw),
                (7_000_000, 3_000_000)
            );
        }
    }

    mod context_an_asset_position_row_is_assembled {
        use super::*;

        #[test]
        fn given_a_valid_llx_row_when_assembled_then_quantity_and_basis_are_preserved() {
            let row = PositionRow {
                account_id: 7,
                symbol: LLX_SYMBOL.to_owned(),
                quantity: 17,
                total_cost_basis_krw: 1_700_000,
            };

            let position = to_position(row).expect("포지션을 조립할 수 있어야 한다");

            assert_eq!(
                position,
                PositionState {
                    account_id: AccountId::from_u64(7).expect("계좌 ID여야 한다"),
                    symbol: LLX_SYMBOL.to_owned(),
                    quantity: 17,
                    cost_basis_krw: 1_700_000,
                }
            );
        }

        #[test]
        fn given_an_unsupported_symbol_when_assembled_then_the_row_is_rejected() {
            let row = PositionRow {
                account_id: 7,
                symbol: "USD".to_owned(),
                quantity: 1,
                total_cost_basis_krw: 1,
            };

            let position = to_position(row);

            assert!(position.is_err());
        }
    }

    mod context_an_existing_execution_is_compared_for_idempotency {
        use super::*;

        fn given_order() -> TradeOrder {
            TradeOrder::try_from(TradeOrderRequest {
                order_id: "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2".to_owned(),
                account_id: "7".to_owned(),
                expected_run_revision: 3,
                expected_state_revision: 42,
                expected_game_day: 17,
                side: OrderSide::Buy,
                symbol: LLX_SYMBOL.to_owned(),
                quantity: 10,
            })
            .expect("유효한 주문이어야 한다")
        }

        fn given_execution_row() -> TradeExecutionRow {
            TradeExecutionRow {
                order_id: "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2".to_owned(),
                account_id: 7,
                expected_run_revision: 3,
                expected_state_revision: 42,
                expected_game_day: 17,
                side: "buy".to_owned(),
                symbol: LLX_SYMBOL.to_owned(),
                quantity: 10,
                price_krw: 100_000,
                gross_amount_krw: 1_000_000,
                fee_krw: 0,
                tax_krw: 0,
                removed_cost_basis_krw: 0,
                realized_gain_loss_krw: None,
            }
        }

        #[test]
        fn given_the_same_fingerprint_when_compared_then_the_execution_matches() {
            let row = given_execution_row();
            let order = given_order();

            let matches = row.matches(&order).expect("지문을 비교할 수 있어야 한다");

            assert!(matches);
        }

        #[test]
        fn given_a_changed_quantity_when_compared_then_the_execution_conflicts() {
            let mut row = given_execution_row();
            row.quantity = 11;
            let order = given_order();

            let matches = row.matches(&order).expect("지문을 비교할 수 있어야 한다");

            assert!(!matches);
        }

        #[test]
        fn given_a_matching_row_when_replayed_then_the_execution_is_marked_replayed() {
            let row = given_execution_row();

            let execution = row
                .to_execution(true)
                .expect("체결을 조립할 수 있어야 한다");

            assert!(execution.replayed);
            assert_eq!(execution.fee_krw, 0);
            assert_eq!(execution.tax_krw, 0);
            assert_eq!(execution.removed_cost_basis_krw, 0);
            assert_eq!(execution.realized_gain_loss_krw, 0);
        }

        #[test]
        fn given_a_legacy_sell_execution_when_replayed_then_recorded_basis_derives_the_result() {
            let mut row = given_execution_row();
            row.side = "sell".to_owned();
            row.gross_amount_krw = 900_000;
            row.removed_cost_basis_krw = 1_000_000;

            let execution = row
                .to_execution(true)
                .expect("과거 매도 체결을 조립할 수 있어야 한다");

            assert_eq!(execution.removed_cost_basis_krw, 1_000_000);
            assert_eq!(execution.realized_gain_loss_krw, -100_000);
        }

        #[test]
        fn given_비용이_기록된_매도_when_재생하면_then_저장된_실현손익과_대조한다() {
            let mut row = given_execution_row();
            row.side = "sell".to_owned();
            row.gross_amount_krw = 1_200_000;
            row.fee_krw = 10_000;
            row.tax_krw = 5_000;
            row.removed_cost_basis_krw = 1_000_000;
            row.realized_gain_loss_krw = Some(185_000);

            let execution = row
                .to_execution(true)
                .expect("비용이 있는 체결을 조립할 수 있어야 한다");

            assert_eq!(execution.realized_gain_loss_krw, 185_000);
        }
    }

    mod context_llx_계좌_허용표를_판단할_때 {
        use super::*;

        #[test]
        fn given_보존월드_when_판단하면_then_일반계좌만_llx를_허용한다() {
            let allowed = account_type_allows_llx(FinancialAccountType::TaxableBrokerage, false);
            let tax_accounts = [
                FinancialAccountType::IsaGeneral,
                FinancialAccountType::IsaLowIncome,
                FinancialAccountType::PensionSavings,
                FinancialAccountType::Irp,
            ];

            assert!(allowed);
            assert!(
                tax_accounts
                    .into_iter()
                    .all(|account_type| !account_type_allows_llx(account_type, false))
            );
        }

        #[test]
        fn given_v4_상품묶음_when_판단하면_then_금과_cma외_투자계좌를_허용한다() {
            let allowed = [
                FinancialAccountType::TaxableBrokerage,
                FinancialAccountType::IsaGeneral,
                FinancialAccountType::IsaLowIncome,
                FinancialAccountType::PensionSavings,
                FinancialAccountType::Irp,
            ];

            assert!(
                allowed
                    .into_iter()
                    .all(|account_type| account_type_allows_llx(account_type, true))
            );
            assert!(!account_type_allows_llx(FinancialAccountType::Cma, true));
            assert!(!account_type_allows_llx(
                FinancialAccountType::KrxGold,
                true
            ));
        }
    }

    mod context_llx_상품비용을_계산할_때 {
        use super::*;

        fn given_terms() -> LlxTradeTerms {
            LlxTradeTerms {
                buy_fee_ppm: 1_000,
                sell_fee_ppm: 2_000,
                sell_tax_ppm: 3_000,
            }
        }

        #[test]
        fn given_매수_when_계산하면_then_매수수수료만_원미만을_버린다() {
            let charges = llx_trade_charges(Some(given_terms()), OrderSide::Buy, 3, 10_001)
                .expect("상품비용을 계산할 수 있어야 한다");

            assert_eq!(
                charges,
                TradeCharges {
                    fee_krw: 30,
                    tax_krw: 0
                }
            );
        }

        #[test]
        fn given_매도_when_계산하면_then_매도수수료와_거래세를_독립계산한다() {
            let charges = llx_trade_charges(Some(given_terms()), OrderSide::Sell, 3, 10_001)
                .expect("상품비용을 계산할 수 있어야 한다");

            assert_eq!(
                charges,
                TradeCharges {
                    fee_krw: 60,
                    tax_krw: 90
                }
            );
        }

        #[test]
        fn given_보존월드_when_계산하면_then_기존_0원_비용을_유지한다() {
            let charges = llx_trade_charges(None, OrderSide::Sell, 1, 100_000)
                .expect("기존 월드 비용을 계산할 수 있어야 한다");

            assert_eq!(charges, TradeCharges::default());
        }
    }
}
