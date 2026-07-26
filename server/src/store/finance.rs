//! Atomic finance commands and current-run ledger reads (§4–§5, §9).

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{MySql, MySqlPool, QueryBuilder};

use super::mysql::{
    CommandIdentitySpec, CommandIdentityState, inspect_command_identity, write_command_identity,
};
use super::tax_accounts::{
    TaxTransferPlanResult, TaxTransferScope, apply_tax_transfer, prepare_tax_transfer,
    read_tax_account_rules_for_game_day,
};
use super::types::{FinanceStore, FinanceStoreResult, M2dAssetStore};
use crate::finance::{
    BondCatalog, BondOrderCommand, BondOrderResponse, CommandId, FinanceFailureCode, FinanceRules,
    FinancialAccount, GoldCatalog, GoldOrderCommand, GoldOrderResponse, GoldWithdrawalCommand,
    GoldWithdrawalResponse, LedgerPage, LedgerPosting, LedgerPostingRecord, LedgerRecord,
    LedgerSource, LedgerSourceKind, LedgerTransactionDraft, M2dAssetCommandResult,
    OpenGoldAccountCommand, OpenGoldAccountResponse, ResourceId, RunId, RunPolicyContext,
    TransferCommand, TransferDirection, TransferInput, TransferReceipt,
};

const COMMAND_KIND_TRANSFER: &str = "transfer";
const MAX_LEDGER_PAGE_SIZE: u32 = 200;

pub struct MySqlFinanceStore {
    pub(super) pool: MySqlPool,
    pub(super) rules: Arc<dyn FinanceRules>,
}

pub fn create_mysql_finance_store(
    pool: MySqlPool,
    rules: Arc<dyn FinanceRules>,
) -> MySqlFinanceStore {
    MySqlFinanceStore { pool, rules }
}

#[async_trait]
impl M2dAssetStore for MySqlFinanceStore {
    async fn bond_catalog(&self, user_id: u64) -> Result<BondCatalog> {
        self.read_bond_catalog(user_id).await
    }

    async fn place_bond_order(
        &self,
        user_id: u64,
        command: &BondOrderCommand,
    ) -> Result<M2dAssetCommandResult<BondOrderResponse>> {
        MySqlFinanceStore::place_bond_order(self, user_id, command).await
    }

    async fn gold_catalog(&self, user_id: u64) -> Result<GoldCatalog> {
        self.read_gold_catalog(user_id).await
    }

    async fn open_gold_account(
        &self,
        user_id: u64,
        command: &OpenGoldAccountCommand,
    ) -> Result<M2dAssetCommandResult<OpenGoldAccountResponse>> {
        MySqlFinanceStore::open_gold_account(self, user_id, command).await
    }

    async fn place_gold_order(
        &self,
        user_id: u64,
        command: &GoldOrderCommand,
    ) -> Result<M2dAssetCommandResult<GoldOrderResponse>> {
        MySqlFinanceStore::place_gold_order(self, user_id, command).await
    }

    async fn withdraw_gold(
        &self,
        user_id: u64,
        command: &GoldWithdrawalCommand,
    ) -> Result<M2dAssetCommandResult<GoldWithdrawalResponse>> {
        MySqlFinanceStore::withdraw_gold(self, user_id, command).await
    }
}

#[async_trait]
impl FinanceStore for MySqlFinanceStore {
    async fn transfer(
        &self,
        user_id: u64,
        command: &TransferCommand,
    ) -> Result<FinanceStoreResult> {
        let fingerprint = transfer_fingerprint(command);
        let mut tx = self.pool.begin().await?;
        let current: Option<LockedSaveRow> = sqlx::query_as(
            "SELECT s.id, s.market_world_id, s.policy_set_id, s.run_revision,
                    s.state_revision, s.game_day, s.cash_krw,
                    EXISTS(SELECT 1 FROM `character` AS c WHERE c.save_id = s.id)
                        AS has_character
             FROM save AS s
             WHERE s.user_id = ?
             FOR UPDATE",
        )
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(current) = current else {
            tx.commit().await?;
            return Ok(FinanceStoreResult::Rejected(
                FinanceFailureCode::CharacterRequired,
            ));
        };

        let identity = CommandIdentitySpec {
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_TRANSFER,
            payload_sha256: &fingerprint,
            cursor: command.cursor,
        };
        match inspect_command_identity(&mut tx, current.id, &identity).await? {
            CommandIdentityState::Conflict => {
                tx.commit().await?;
                return Ok(FinanceStoreResult::Rejected(
                    FinanceFailureCode::IdempotencyConflict,
                ));
            }
            CommandIdentityState::Matching => {
                let receipt =
                    read_command_receipt(&mut tx, current.id, command.command_id.as_str())
                        .await?
                        .context("transfer command identity has no final receipt")?;
                ensure!(
                    receipt.command_kind == COMMAND_KIND_TRANSFER
                        && receipt.payload_sha256 == fingerprint,
                    "transfer receipt disagrees with its command identity"
                );

                let replay = receipt.into_transfer_receipt(command)?;
                tx.commit().await?;
                return Ok(FinanceStoreResult::Transferred(replay));
            }
            CommandIdentityState::Missing => {}
        }

        if !current.has_character {
            tx.commit().await?;
            return Ok(FinanceStoreResult::Rejected(
                FinanceFailureCode::CharacterRequired,
            ));
        }
        if current.run_revision != command.cursor.expected_run_revision
            || current.state_revision != command.cursor.expected_state_revision
            || current.game_day != command.cursor.expected_game_day
        {
            tx.commit().await?;
            return Ok(FinanceStoreResult::Rejected(FinanceFailureCode::Busy));
        }

        let account: Option<LockedAccountRow> = sqlx::query_as(
            "SELECT id, run_revision, account_type, status, cash_krw, is_default
             FROM financial_account
             WHERE save_id = ? AND run_revision = ? AND id = ?
             FOR UPDATE",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(command.account_id.get())
        .fetch_optional(&mut *tx)
        .await?;
        let Some(account) = account else {
            tx.commit().await?;
            return Ok(FinanceStoreResult::Rejected(
                FinanceFailureCode::AccountNotFound,
            ));
        };

        let policy = RunPolicyContext {
            run: RunId {
                save_id: resource_id(current.id, "save")?,
                run_revision: current.run_revision,
            },
            policy_set_id: resource_id(current.policy_set_id, "policy set")?,
        };
        let account = account.into_account(policy.run)?;
        let account_cash_krw = account.cash_krw;
        let mutation = match self.rules.apply_transfer(TransferInput {
            policy,
            command_id: command.command_id.clone(),
            game_day: current.game_day,
            wallet_cash_krw: current.cash_krw,
            account: account.clone(),
            direction: command.direction,
            amount_krw: command.amount_krw,
        }) {
            Ok(mutation) => mutation,
            Err(error) => {
                tx.commit().await?;
                return Ok(FinanceStoreResult::Rejected(error.failure_code()));
            }
        };
        let tax_account_rules = read_tax_account_rules_for_game_day(
            &mut tx,
            current.policy_set_id,
            current.market_world_id,
            current.game_day,
        )
        .await?;
        let tax_transfer = match prepare_tax_transfer(
            &mut tx,
            tax_account_rules.as_ref(),
            TaxTransferScope {
                save_id: current.id,
                run_revision: current.run_revision,
                market_world_id: current.market_world_id,
                game_day: current.game_day,
            },
            &account,
            command,
        )
        .await?
        {
            TaxTransferPlanResult::Planned(plan) => plan,
            TaxTransferPlanResult::Rejected(rejection) => {
                tx.commit().await?;
                return Ok(FinanceStoreResult::Rejected(rejection));
            }
        };

        let committed_state_revision = current
            .state_revision
            .checked_add(1)
            .context("state revision overflowed while transferring cash")?;

        write_command_identity(&mut tx, current.id, &identity).await?;

        let account_update = sqlx::query(
            "UPDATE financial_account
             SET cash_krw = ?
             WHERE save_id = ? AND run_revision = ? AND id = ?
               AND status = 'open' AND cash_krw = ?",
        )
        .bind(mutation.account.cash_krw)
        .bind(current.id)
        .bind(current.run_revision)
        .bind(mutation.account.id.get())
        .bind(account_cash_krw)
        .execute(&mut *tx)
        .await?;
        if account_update.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(FinanceStoreResult::Rejected(FinanceFailureCode::Busy));
        }

        let save_update = sqlx::query(
            "UPDATE save
             SET cash_krw = ?, state_revision = ?
             WHERE id = ? AND market_world_id = ? AND policy_set_id = ?
               AND run_revision = ? AND state_revision = ? AND game_day = ?
               AND cash_krw = ?",
        )
        .bind(mutation.wallet_cash_krw)
        .bind(committed_state_revision)
        .bind(current.id)
        .bind(current.market_world_id)
        .bind(current.policy_set_id)
        .bind(current.run_revision)
        .bind(current.state_revision)
        .bind(current.game_day)
        .bind(current.cash_krw)
        .execute(&mut *tx)
        .await?;
        if save_update.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(FinanceStoreResult::Rejected(FinanceFailureCode::Busy));
        }

        let ledger_transaction_id =
            write_ledger_transaction(&mut tx, current.id, current.run_revision, &mutation.ledger)
                .await?;
        apply_tax_transfer(
            &mut tx,
            current.id,
            current.run_revision,
            current.game_day,
            command,
            ledger_transaction_id,
            tax_transfer,
        )
        .await?;
        let result = StoredTransferResult {
            command_id: command.command_id.as_str().to_owned(),
            account_id: command.account_id.to_string(),
            direction: command.direction,
            amount_krw: command.amount_krw,
            run_revision: current.run_revision,
            state_revision: committed_state_revision,
            game_day: current.game_day,
            replayed: false,
        };
        let result_json = serde_json::to_string(&result)
            .context("failed to serialize the transfer command result")?;

        sqlx::query(
            "INSERT INTO command_receipt
                 (save_id, run_revision, command_id, command_kind, payload_sha256,
                  market_world_id, state_revision, game_day, result,
                  ledger_transaction_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(command.command_id.as_str())
        .bind(COMMAND_KIND_TRANSFER)
        .bind(&fingerprint)
        .bind(current.market_world_id)
        .bind(committed_state_revision)
        .bind(current.game_day)
        .bind(result_json)
        .bind(ledger_transaction_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(FinanceStoreResult::Transferred(result.into_receipt(false)?))
    }

    async fn ledger_page(
        &self,
        user_id: u64,
        before: Option<u64>,
        limit: u32,
    ) -> Result<LedgerPage> {
        validate_ledger_cursor(before, limit)?;
        let mut tx = self.pool.begin().await?;
        let scope: Option<LedgerScopeRow> = sqlx::query_as(
            "SELECT id, run_revision
             FROM save
             WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(scope) = scope else {
            tx.commit().await?;
            return Ok(LedgerPage {
                transactions: Vec::new(),
                next_before: None,
            });
        };

        let fetch_limit = limit
            .checked_add(1)
            .context("ledger page lookahead overflowed")?;
        let mut rows: Vec<LedgerTransactionRow> = match before {
            Some(before) => {
                sqlx::query_as(
                    "SELECT id, game_day, policy_set_id, source_kind, source_id, description
                     FROM ledger_transaction
                     WHERE save_id = ? AND run_revision = ? AND id < ?
                     ORDER BY id DESC
                     LIMIT ?",
                )
                .bind(scope.id)
                .bind(scope.run_revision)
                .bind(before)
                .bind(fetch_limit)
                .fetch_all(&mut *tx)
                .await?
            }
            None => {
                sqlx::query_as(
                    "SELECT id, game_day, policy_set_id, source_kind, source_id, description
                     FROM ledger_transaction
                     WHERE save_id = ? AND run_revision = ?
                     ORDER BY id DESC
                     LIMIT ?",
                )
                .bind(scope.id)
                .bind(scope.run_revision)
                .bind(fetch_limit)
                .fetch_all(&mut *tx)
                .await?
            }
        };
        let has_more = rows.len() > usize::try_from(limit).context("invalid ledger page limit")?;
        if has_more {
            rows.truncate(usize::try_from(limit).context("invalid ledger page limit")?);
        }

        let postings = read_ledger_postings(&mut tx, scope, &rows).await?;
        let transactions = assemble_ledger_records(scope, rows, postings, self.rules.as_ref())?;
        let next_before = if has_more {
            transactions.last().map(|transaction| transaction.id)
        } else {
            None
        };
        tx.commit().await?;

        Ok(LedgerPage {
            transactions,
            next_before,
        })
    }
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
struct LockedSaveRow {
    id: u64,
    market_world_id: u64,
    policy_set_id: u64,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
    cash_krw: i64,
    has_character: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct LockedAccountRow {
    id: u64,
    run_revision: u32,
    account_type: String,
    status: String,
    cash_krw: i64,
    is_default: bool,
}

impl LockedAccountRow {
    fn into_account(self, run: RunId) -> Result<FinancialAccount> {
        ensure!(
            self.run_revision == run.run_revision,
            "locked financial account changed run"
        );
        Ok(FinancialAccount {
            id: resource_id(self.id, "financial account")?,
            run,
            account_type: from_db_str(&self.account_type)?,
            status: from_db_str(&self.status)?,
            is_default: self.is_default,
            cash_krw: self.cash_krw,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct CommandReceiptRow {
    command_kind: String,
    payload_sha256: String,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
    result_json: String,
    ledger_transaction_id: Option<u64>,
}

impl CommandReceiptRow {
    fn into_transfer_receipt(self, command: &TransferCommand) -> Result<TransferReceipt> {
        let stored: StoredTransferResult = serde_json::from_str(&self.result_json)
            .context("stored transfer command result is invalid")?;
        let expected_state_revision = command
            .cursor
            .expected_state_revision
            .checked_add(1)
            .context("stored transfer command cursor cannot advance")?;
        ensure!(
            stored.command_id == command.command_id.as_str()
                && stored.account_id == command.account_id.to_string()
                && stored.direction == command.direction
                && stored.amount_krw == command.amount_krw
                && stored.run_revision == command.cursor.expected_run_revision
                && stored.state_revision == expected_state_revision
                && stored.game_day == command.cursor.expected_game_day
                && stored.run_revision == self.run_revision
                && stored.state_revision == self.state_revision
                && stored.game_day == self.game_day
                && self.ledger_transaction_id.is_some(),
            "stored transfer receipt disagrees with its command result"
        );
        stored.into_receipt(true)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredTransferResult {
    command_id: String,
    account_id: String,
    direction: TransferDirection,
    amount_krw: i64,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
    replayed: bool,
}

impl StoredTransferResult {
    fn into_receipt(self, replayed: bool) -> Result<TransferReceipt> {
        ensure!(
            !self.replayed,
            "stored initial transfer result is marked as replayed"
        );
        Ok(TransferReceipt {
            command_id: CommandId::parse(self.command_id)?,
            account_id: ResourceId::parse(&self.account_id)?,
            direction: self.direction,
            amount_krw: self.amount_krw,
            run_revision: self.run_revision,
            state_revision: self.state_revision,
            game_day: self.game_day,
            replayed,
        })
    }
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
struct LedgerScopeRow {
    id: u64,
    run_revision: u32,
}

#[derive(Debug, sqlx::FromRow)]
struct LedgerTransactionRow {
    id: u64,
    game_day: u32,
    policy_set_id: u64,
    source_kind: String,
    source_id: String,
    description: String,
}

#[derive(Debug, sqlx::FromRow)]
struct LedgerPostingRow {
    ledger_transaction_id: u64,
    account_code: String,
    financial_account_id: Option<u64>,
    amount_krw: i64,
}

async fn read_command_receipt(
    tx: &mut sqlx::Transaction<'_, MySql>,
    save_id: u64,
    command_id: &str,
) -> Result<Option<CommandReceiptRow>> {
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
    .context("failed to read a finance command receipt")
}

async fn write_ledger_transaction(
    tx: &mut sqlx::Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    ledger: &crate::finance::LedgerTransaction,
) -> Result<u64> {
    ensure!(
        ledger.policy().run.save_id.get() == save_id
            && ledger.policy().run.run_revision == run_revision,
        "validated ledger belongs to another run"
    );
    let result = sqlx::query(
        "INSERT INTO ledger_transaction
             (save_id, run_revision, game_day, policy_set_id,
              source_kind, source_id, description)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(ledger.game_day())
    .bind(ledger.policy().policy_set_id.get())
    .bind(to_db_str(&ledger.source().kind)?)
    .bind(&ledger.source().source_id)
    .bind(ledger.description())
    .execute(&mut **tx)
    .await?;
    let ledger_transaction_id = result.last_insert_id();
    ensure!(
        ledger_transaction_id != 0,
        "ledger insert did not return an identifier"
    );

    for (index, posting) in ledger.postings().iter().enumerate() {
        let posting_order = u16::try_from(index + 1).context("too many ledger postings")?;
        sqlx::query(
            "INSERT INTO ledger_posting
                 (save_id, run_revision, ledger_transaction_id, posting_order,
                  account_code, financial_account_id, amount_krw)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(ledger_transaction_id)
        .bind(posting_order)
        .bind(to_db_str(&posting.account_code)?)
        .bind(posting.financial_account_id.map(ResourceId::get))
        .bind(posting.amount_krw)
        .execute(&mut **tx)
        .await?;
    }

    Ok(ledger_transaction_id)
}

async fn read_ledger_postings(
    tx: &mut sqlx::Transaction<'_, MySql>,
    scope: LedgerScopeRow,
    transactions: &[LedgerTransactionRow],
) -> Result<Vec<LedgerPostingRow>> {
    if transactions.is_empty() {
        return Ok(Vec::new());
    }

    let mut query = QueryBuilder::<MySql>::new(
        "SELECT ledger_transaction_id, account_code, financial_account_id, amount_krw
         FROM ledger_posting
         WHERE save_id = ",
    );
    query
        .push_bind(scope.id)
        .push(" AND run_revision = ")
        .push_bind(scope.run_revision)
        .push(" AND ledger_transaction_id IN (");
    {
        let mut separated = query.separated(", ");
        for transaction in transactions {
            separated.push_bind(transaction.id);
        }
        separated.push_unseparated(")");
    }
    query.push(" ORDER BY ledger_transaction_id DESC, posting_order");

    query
        .build_query_as()
        .fetch_all(&mut **tx)
        .await
        .context("failed to read ledger postings")
}

fn assemble_ledger_records(
    scope: LedgerScopeRow,
    transactions: Vec<LedgerTransactionRow>,
    postings: Vec<LedgerPostingRow>,
    rules: &dyn FinanceRules,
) -> Result<Vec<LedgerRecord>> {
    let mut postings_by_transaction = HashMap::<u64, Vec<LedgerPosting>>::new();
    for posting in postings {
        postings_by_transaction
            .entry(posting.ledger_transaction_id)
            .or_default()
            .push(LedgerPosting {
                account_code: from_db_str(&posting.account_code)?,
                financial_account_id: posting
                    .financial_account_id
                    .map(|id| resource_id(id, "financial account"))
                    .transpose()?,
                amount_krw: posting.amount_krw,
            });
    }

    transactions
        .into_iter()
        .map(|transaction| {
            let source_kind: LedgerSourceKind = from_db_str(&transaction.source_kind)?;
            let ledger = rules
                .create_ledger_transaction(LedgerTransactionDraft {
                    policy: RunPolicyContext {
                        run: RunId {
                            save_id: resource_id(scope.id, "save")?,
                            run_revision: scope.run_revision,
                        },
                        policy_set_id: resource_id(transaction.policy_set_id, "policy set")?,
                    },
                    source: LedgerSource {
                        kind: source_kind,
                        source_id: transaction.source_id,
                    },
                    game_day: transaction.game_day,
                    description: transaction.description.clone(),
                    postings: postings_by_transaction
                        .remove(&transaction.id)
                        .unwrap_or_default(),
                })
                .context("stored ledger transaction violates finance invariants")?;

            Ok(LedgerRecord {
                id: resource_id(transaction.id, "ledger transaction")?,
                game_day: transaction.game_day,
                description: transaction.description,
                source_kind,
                postings: ledger
                    .postings()
                    .iter()
                    .map(|posting| LedgerPostingRecord {
                        account_code: posting.account_code,
                        financial_account_id: posting.financial_account_id,
                        amount_krw: posting.amount_krw,
                    })
                    .collect(),
            })
        })
        .collect()
}

fn transfer_fingerprint(command: &TransferCommand) -> String {
    let direction = match command.direction {
        TransferDirection::WalletToAccount => "walletToAccount",
        TransferDirection::AccountToWallet => "accountToWallet",
    };
    let canonical = format!(
        "lifeledger.finance.transfer.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\naccountId={}\ndirection={}\namountKrw={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.account_id,
        direction,
        command.amount_krw
    );
    Sha256::digest(canonical.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn validate_ledger_cursor(before: Option<u64>, limit: u32) -> Result<()> {
    if before == Some(0) {
        bail!("ledger cursor must be a positive resource ID");
    }
    if !(1..=MAX_LEDGER_PAGE_SIZE).contains(&limit) {
        bail!("ledger page limit must be between 1 and {MAX_LEDGER_PAGE_SIZE}");
    }
    Ok(())
}

fn resource_id(value: u64, kind: &str) -> Result<ResourceId> {
    ResourceId::parse(&value.to_string()).with_context(|| format!("stored {kind} ID is invalid"))
}

fn to_db_str<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value)? {
        Value::String(text) => Ok(text),
        other => bail!("value is not storable as a string: {other}"),
    }
}

fn from_db_str<T: DeserializeOwned>(raw: &str) -> Result<T> {
    serde_json::from_value(Value::String(raw.to_owned()))
        .with_context(|| format!("unknown finance value stored: {raw}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finance::{CommandCursor, CommandId};

    fn given_transfer_command() -> TransferCommand {
        TransferCommand {
            command_id: CommandId::parse("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2")
                .expect("표준 UUID여야 한다"),
            cursor: CommandCursor {
                expected_run_revision: 3,
                expected_state_revision: 42,
                expected_game_day: 17,
            },
            account_id: ResourceId::from_u64(99),
            direction: TransferDirection::WalletToAccount,
            amount_krw: 1_000_000,
        }
    }

    mod context_a_transfer_payload_is_fingerprinted {
        use super::*;

        #[test]
        fn given_the_same_fixed_fields_when_hashed_then_the_fingerprint_is_stable() {
            let command = given_transfer_command();

            let first = transfer_fingerprint(&command);
            let second = transfer_fingerprint(&command);

            assert_eq!(first, second);
            assert_eq!(
                first,
                "c6c3332db0081be7bc37f183a3ac6595fbcc1551bac7cf3ffb4a402e214385fe"
            );
        }

        #[test]
        fn given_a_changed_cursor_when_hashed_then_the_fingerprint_changes() {
            let command = given_transfer_command();
            let mut changed = command.clone();
            changed.cursor.expected_state_revision = 43;

            let original = transfer_fingerprint(&command);
            let changed = transfer_fingerprint(&changed);

            assert_ne!(original, changed);
        }
    }

    mod context_a_ledger_cursor_is_validated {
        use super::*;

        #[test]
        fn given_the_limit_boundaries_when_validated_then_they_are_accepted() {
            let minimum = validate_ledger_cursor(None, 1);
            let maximum = validate_ledger_cursor(Some(1), 200);

            assert!(minimum.is_ok());
            assert!(maximum.is_ok());
        }

        #[test]
        fn given_zero_or_an_oversized_limit_when_validated_then_it_is_rejected() {
            let zero = validate_ledger_cursor(None, 0);
            let oversized = validate_ledger_cursor(None, 201);

            assert!(zero.is_err());
            assert!(oversized.is_err());
        }

        #[test]
        fn given_a_zero_before_id_when_validated_then_it_is_rejected() {
            let result = validate_ledger_cursor(Some(0), 50);

            assert!(result.is_err());
        }
    }

    mod context_a_stored_transfer_result_is_replayed {
        use super::*;

        #[test]
        fn given_a_valid_stored_result_when_replayed_then_the_original_cursor_is_preserved() {
            let command = given_transfer_command();
            let stored = StoredTransferResult {
                command_id: "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2".to_owned(),
                account_id: "99".to_owned(),
                direction: TransferDirection::WalletToAccount,
                amount_krw: 1_000_000,
                run_revision: 3,
                state_revision: 43,
                game_day: 17,
                replayed: false,
            };
            let row = CommandReceiptRow {
                command_kind: COMMAND_KIND_TRANSFER.to_owned(),
                payload_sha256: transfer_fingerprint(&command),
                run_revision: 3,
                state_revision: 43,
                game_day: 17,
                result_json: serde_json::to_string(&stored)
                    .expect("저장 결과를 직렬화할 수 있어야 한다"),
                ledger_transaction_id: Some(7),
            };

            let receipt = row
                .into_transfer_receipt(&command)
                .expect("영수증을 복원할 수 있어야 한다");

            assert_eq!(receipt.state_revision, 43);
            assert!(receipt.replayed);
        }
    }
}
