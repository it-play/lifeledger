//! M2-B cash-product persistence and daily settlement application (§9.2, §11).

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use async_trait::async_trait;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{MySql, Transaction};
use time::Date;

use super::annual_tax::{
    AnnualTaxRunContext, AnnualTaxYearState, accrue_financial_income_source, read_annual_tax_year,
};
use super::finance::MySqlFinanceStore;
use super::mysql::{
    CommandIdentitySpec, CommandIdentityState, GameCommandReceiptWrite, inspect_command_identity,
    read_state, write_command_identity, write_game_command_receipt, write_ledger_transaction,
};
use super::types::{CashProductStore, CashProductStoreResult, GameCommandCursor};
use crate::finance::{
    CashPayloadVersion, CashProductCatalog, CashProductContractState, CashProductContractStatus,
    CashProductKind, CashProductMutation, CashProductPolicy, CashProductVersion,
    CashSettlementExecution, CashSettlementExecutorRegistry, CashSettlementKind,
    CashSettlementOutcome, CashSettlementPayload, CashSettlementSource, CashSettlementTask,
    CloseCashProductCommand, CloseCashProductReceipt, CloseCmaAccountCommand,
    CloseCmaAccountReceipt, CmaAccountContractState, CmaDailyAccrualInput, CmaDailyTerms,
    CmaInterestPayloadV1, CommandCursor, CommandId, DailyCashSettlementPlanInput, DayCountBasis,
    DepositMaturityPayloadV1, DepositProtectionPolicy, DepositProtectionState, FinanceFailureCode,
    FinanceRules, FinancialAccountType, FinancialIncomeAccrual, FinancialIncomeDelta,
    FinancialIncomeSource, FinancialIncomeYear, FinancialInstitution, InstallmentSavingsContract,
    InstallmentSavingsScheduleInput, InterestTaxPolicy, LedgerSource, LedgerSourceKind,
    OpenCashProductCommand, OpenCashProductReceipt, OpenCmaAccountCommand, OpenCmaAccountReceipt,
    ProtectedDepositAmount, ResourceId, RunId, RunPolicyContext, SavingsInstallmentPayloadV1,
    SavingsInstallmentPrincipal, SavingsMaturityPayloadV1, SettlementLedgerContext,
    TaxAdvantagedInterestDelta, TermDepositContract, WithholdingTax, accrue_cma_daily,
    aggregate_deposit_protection, calculate_simple_interest_krw, cash_product_tax_treatment,
    collect_savings_installment, create_cash_settlement_planner,
    create_installment_savings_schedule, create_interest_payout_ledger,
    create_interest_payout_ledger_for_account, create_product_principal_funding_ledger,
    settle_installment_savings_early_close_for_account,
    settle_installment_savings_maturity_for_account, settle_term_deposit_early_close_for_account,
    settle_term_deposit_maturity_for_account,
};

const COMMAND_KIND_OPEN_ACCOUNT: &str = "openAccount";
const COMMAND_KIND_CLOSE_ACCOUNT: &str = "closeAccount";
const COMMAND_KIND_OPEN_DEPOSIT: &str = "openDeposit";
const COMMAND_KIND_CLOSE_DEPOSIT: &str = "closeDeposit";
const MAX_FINANCIAL_ACCOUNTS: i64 = 32;
const MAX_CASH_PRODUCT_CONTRACTS: i64 = 100;
const CANCELLATION_ACCOUNT_CLOSED: &str = "accountClosed";
const CANCELLATION_CONTRACT_CLOSED: &str = "contractClosed";

#[async_trait]
impl CashProductStore for MySqlFinanceStore {
    async fn cash_product_catalog(&self) -> Result<CashProductCatalog> {
        let rows: Vec<CatalogRow> = sqlx::query_as(
            "SELECT product.id, product.product_key, product.display_name,
                    product.product_kind, product.institution_id,
                    institution.institution_key, institution.display_name AS institution_name,
                    product.is_deposit_protection_eligible, product.rate_reference,
                    product.spread_bp, product.minimum_interest_balance_krw,
                    product.minimum_amount_krw, product.maximum_amount_krw,
                    product.term_days, product.term_months, product.installment_count,
                    product.early_termination_rate_bp, product.day_count_denominator
             FROM cash_product_version AS product
             INNER JOIN financial_institution AS institution
               ON institution.id = product.institution_id
             ORDER BY product.id",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(CashProductCatalog {
            products: rows
                .into_iter()
                .map(CatalogRow::into_product)
                .collect::<Result<Vec<_>>>()?,
        })
    }

    async fn open_cma_account(
        &self,
        user_id: u64,
        command: &OpenCmaAccountCommand,
    ) -> Result<CashProductStoreResult<OpenCmaAccountReceipt>> {
        let fingerprint = open_cma_fingerprint(command);
        let mut tx = self.pool.begin().await?;
        let Some(current) = lock_save_for_user(&mut tx, user_id).await? else {
            tx.commit().await?;
            return Ok(CashProductStoreResult::Rejected(
                FinanceFailureCode::CharacterRequired,
            ));
        };
        let identity = CommandIdentitySpec {
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_OPEN_ACCOUNT,
            payload_sha256: &fingerprint,
            cursor: command.cursor,
        };
        match inspect_command_identity(&mut tx, current.id, &identity).await? {
            CommandIdentityState::Conflict => {
                tx.commit().await?;
                return Ok(CashProductStoreResult::Rejected(
                    FinanceFailureCode::IdempotencyConflict,
                ));
            }
            CommandIdentityState::Matching => {
                let mut receipt: OpenCmaAccountReceipt = read_receipt(
                    &mut tx,
                    current.id,
                    &command.command_id,
                    COMMAND_KIND_OPEN_ACCOUNT,
                    &fingerprint,
                )
                .await?
                .context("open-account command identity has no final receipt")?;
                receipt.replayed = true;
                let save = read_state(&mut tx, current.id).await?;
                tx.commit().await?;
                return Ok(CashProductStoreResult::Applied {
                    receipt,
                    save: Box::new(save),
                });
            }
            CommandIdentityState::Missing => {}
        }
        if let Some(rejection) = validate_current(&current, command.cursor) {
            tx.commit().await?;
            return Ok(CashProductStoreResult::Rejected(rejection));
        }

        let account_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM financial_account
             WHERE save_id = ? AND run_revision = ?",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .fetch_one(&mut *tx)
        .await?;
        if account_count.0 >= MAX_FINANCIAL_ACCOUNTS {
            tx.commit().await?;
            return Ok(CashProductStoreResult::Rejected(
                FinanceFailureCode::LimitExceeded,
            ));
        }

        let product = read_product_terms(&mut tx, command.product_version_id).await?;
        let Some(product) = product else {
            tx.commit().await?;
            return Ok(CashProductStoreResult::Rejected(
                FinanceFailureCode::ProductNotFound,
            ));
        };
        let kind: CashProductKind = from_db_str(&product.product_kind)?;
        if !kind.is_cma() {
            tx.commit().await?;
            return Ok(CashProductStoreResult::Rejected(
                FinanceFailureCode::ProductNotFound,
            ));
        }
        let minimum_interest_balance_krw = product
            .minimum_interest_balance_krw
            .context("CMA catalog row has no minimum interest balance")?;
        if product.day_count_denominator != 365 || product.rate_reference != "treasury3mBp" {
            bail!("CMA catalog row uses unsupported rate terms");
        }
        let market = read_market_terms(&mut tx, current.market_world_id, current.game_day).await?;
        let valid_rate = market
            .treasury_3m_bp
            .and_then(|rate| i32::try_from(rate).ok())
            .and_then(|rate| rate.checked_add(product.spread_bp))
            .is_some_and(|rate| product_rate_available(rate, None));
        if !valid_rate {
            tx.commit().await?;
            return Ok(CashProductStoreResult::Rejected(
                FinanceFailureCode::RateUnavailable,
            ));
        }

        write_command_identity(&mut tx, current.id, &identity).await?;
        let account_insert = sqlx::query(
            "INSERT INTO financial_account
                 (save_id, run_revision, account_type, status, cash_krw,
                  is_default, opened_game_day)
             VALUES (?, ?, 'cma', 'open', 0, FALSE, ?)",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(current.game_day)
        .execute(&mut *tx)
        .await?;
        let account_id = account_insert.last_insert_id();
        let terms_insert = sqlx::query(
            "INSERT INTO cma_account_contract
                 (save_id, run_revision, financial_account_id, product_version_id,
                  rate_reference, spread_bp, minimum_interest_balance_krw,
                  day_count_denominator, interest_remainder)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0)",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(account_id)
        .bind(product.id)
        .bind(&product.rate_reference)
        .bind(product.spread_bp)
        .bind(minimum_interest_balance_krw)
        .bind(product.day_count_denominator)
        .execute(&mut *tx)
        .await?;
        let terms_id = terms_insert.last_insert_id();
        let due_game_day = current
            .game_day
            .checked_add(1)
            .context("game day overflowed while opening a CMA")?;
        let payload = serde_json::to_string(&CmaInterestPayloadV1 {
            version: CashPayloadVersion::V1,
            account_id: resource_id(account_id, "financial account")?,
            cma_terms_id: resource_id(terms_id, "CMA terms")?,
        })?;
        insert_scheduled_settlement(
            &mut tx,
            ScheduledSettlementInsert {
                save_id: current.id,
                run_revision: current.run_revision,
                due_game_day,
                kind: "cmaInterest",
                payload: &payload,
                source_kind: "cmaAccount",
                source_id: &account_id.to_string(),
                occurrence: 1,
            },
        )
        .await?;

        let committed = increment_state_revision(&mut tx, &current).await?;
        let receipt = OpenCmaAccountReceipt {
            command_id: command.command_id.clone(),
            account_id: resource_id(account_id, "financial account")?,
            product_version_id: command.product_version_id,
            replayed: false,
        };
        write_game_command_receipt(
            &mut tx,
            GameCommandReceiptWrite {
                save_id: current.id,
                command_id: &command.command_id,
                command_kind: COMMAND_KIND_OPEN_ACCOUNT,
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

        Ok(CashProductStoreResult::Applied {
            receipt,
            save: Box::new(save),
        })
    }

    async fn close_cma_account(
        &self,
        user_id: u64,
        command: &CloseCmaAccountCommand,
    ) -> Result<CashProductStoreResult<CloseCmaAccountReceipt>> {
        let fingerprint = close_cma_fingerprint(command);
        let mut tx = self.pool.begin().await?;
        let Some(current) = lock_save_for_user(&mut tx, user_id).await? else {
            tx.commit().await?;
            return Ok(CashProductStoreResult::Rejected(
                FinanceFailureCode::CharacterRequired,
            ));
        };
        let identity = CommandIdentitySpec {
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_CLOSE_ACCOUNT,
            payload_sha256: &fingerprint,
            cursor: command.cursor,
        };
        match inspect_command_identity(&mut tx, current.id, &identity).await? {
            CommandIdentityState::Conflict => {
                tx.commit().await?;
                return Ok(CashProductStoreResult::Rejected(
                    FinanceFailureCode::IdempotencyConflict,
                ));
            }
            CommandIdentityState::Matching => {
                let mut receipt: CloseCmaAccountReceipt = read_receipt(
                    &mut tx,
                    current.id,
                    &command.command_id,
                    COMMAND_KIND_CLOSE_ACCOUNT,
                    &fingerprint,
                )
                .await?
                .context("close-account command identity has no final receipt")?;
                receipt.replayed = true;
                let save = read_state(&mut tx, current.id).await?;
                tx.commit().await?;
                return Ok(CashProductStoreResult::Applied {
                    receipt,
                    save: Box::new(save),
                });
            }
            CommandIdentityState::Missing => {}
        }
        if let Some(rejection) = validate_current(&current, command.cursor) {
            tx.commit().await?;
            return Ok(CashProductStoreResult::Rejected(rejection));
        }

        let account: Option<LockedAccountRow> = sqlx::query_as(
            "SELECT id, account_type, status, cash_krw, is_default
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
            return Ok(CashProductStoreResult::Rejected(
                FinanceFailureCode::AccountNotFound,
            ));
        };
        if account.account_type != "cma" || account.is_default {
            tx.commit().await?;
            return Ok(CashProductStoreResult::Rejected(
                FinanceFailureCode::AccountTypeNotAllowed,
            ));
        }
        if account.status != "open" {
            tx.commit().await?;
            return Ok(CashProductStoreResult::Rejected(
                FinanceFailureCode::AccountClosed,
            ));
        }
        if account.cash_krw != 0 {
            tx.commit().await?;
            return Ok(CashProductStoreResult::Rejected(
                FinanceFailureCode::AccountNotEmpty,
            ));
        }

        write_command_identity(&mut tx, current.id, &identity).await?;
        let update = sqlx::query(
            "UPDATE financial_account SET status = 'closed'
             WHERE save_id = ? AND run_revision = ? AND id = ?
               AND status = 'open' AND cash_krw = 0",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(account.id)
        .execute(&mut *tx)
        .await?;
        ensure!(
            update.rows_affected() == 1,
            "CMA account close lost its lock"
        );
        sqlx::query(
            "UPDATE scheduled_settlement
             SET status = 'cancelled', cancellation_reason = ?
             WHERE save_id = ? AND run_revision = ? AND status = 'pending'
               AND source_kind = 'cmaAccount' AND source_id = ?",
        )
        .bind(CANCELLATION_ACCOUNT_CLOSED)
        .bind(current.id)
        .bind(current.run_revision)
        .bind(account.id.to_string())
        .execute(&mut *tx)
        .await?;

        let committed = increment_state_revision(&mut tx, &current).await?;
        let receipt = CloseCmaAccountReceipt {
            command_id: command.command_id.clone(),
            account_id: command.account_id,
            replayed: false,
        };
        write_game_command_receipt(
            &mut tx,
            GameCommandReceiptWrite {
                save_id: current.id,
                command_id: &command.command_id,
                command_kind: COMMAND_KIND_CLOSE_ACCOUNT,
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

        Ok(CashProductStoreResult::Applied {
            receipt,
            save: Box::new(save),
        })
    }

    async fn open_cash_product(
        &self,
        user_id: u64,
        command: &OpenCashProductCommand,
    ) -> Result<CashProductStoreResult<OpenCashProductReceipt>> {
        open_cash_product(self, user_id, command).await
    }

    async fn close_cash_product(
        &self,
        user_id: u64,
        command: &CloseCashProductCommand,
    ) -> Result<CashProductStoreResult<CloseCashProductReceipt>> {
        close_cash_product(self, user_id, command).await
    }

    async fn financial_income_year(
        &self,
        user_id: u64,
        tax_year: u16,
    ) -> Result<AnnualTaxYearState> {
        if tax_year == 0 || tax_year > 9999 {
            bail!("tax year must be between 1 and 9999");
        }
        let mut tx = self.pool.begin().await?;
        let row: Option<(u64, u64, u32, u32, Date)> = sqlx::query_as(
            "SELECT save.id, save.policy_set_id, save.run_revision, save.game_day,
                    COALESCE(daily.market_date,
                             DATE_ADD(world.start_date, INTERVAL save.game_day DAY)) AS market_date
             FROM save
             INNER JOIN market_world AS world ON world.id = save.market_world_id
             LEFT JOIN market_daily AS daily
               ON daily.world_id = save.market_world_id
              AND daily.game_day = save.game_day
             WHERE save.user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await?;
        let (save_id, policy_set_id, run_revision, game_day, market_date) =
            row.context("financial-income year requires an existing save")?;
        let income = read_annual_tax_year(
            &mut tx,
            AnnualTaxRunContext {
                save_id,
                run_revision,
                policy_set_id,
                game_day,
                market_date,
            },
            tax_year,
        )
        .await?;
        tx.commit().await?;
        Ok(income)
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedSaveRow {
    id: u64,
    market_world_id: u64,
    policy_set_id: u64,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
    has_character: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedAccountRow {
    id: u64,
    account_type: String,
    status: String,
    cash_krw: i64,
    is_default: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ProductTermsRow {
    id: u64,
    product_kind: String,
    rate_reference: String,
    spread_bp: i32,
    minimum_interest_balance_krw: Option<i64>,
    minimum_amount_krw: Option<i64>,
    maximum_amount_krw: Option<i64>,
    term_days: Option<u32>,
    term_months: Option<u32>,
    installment_count: Option<u32>,
    early_termination_rate_bp: Option<u32>,
    day_count_denominator: u32,
}

#[derive(sqlx::FromRow)]
struct CatalogRow {
    id: u64,
    product_key: String,
    display_name: String,
    product_kind: String,
    institution_id: u64,
    institution_key: String,
    institution_name: String,
    is_deposit_protection_eligible: bool,
    rate_reference: String,
    spread_bp: i32,
    minimum_interest_balance_krw: Option<i64>,
    minimum_amount_krw: Option<i64>,
    maximum_amount_krw: Option<i64>,
    term_days: Option<u32>,
    term_months: Option<u32>,
    installment_count: Option<u32>,
    early_termination_rate_bp: Option<u32>,
    day_count_denominator: u32,
}

impl CatalogRow {
    fn into_product(self) -> Result<CashProductVersion> {
        ensure!(
            self.day_count_denominator == 365,
            "cash-product catalog uses an unsupported day-count basis"
        );
        Ok(CashProductVersion {
            id: resource_id(self.id, "cash product")?,
            key: self.product_key,
            display_name: self.display_name,
            kind: from_db_str(&self.product_kind)?,
            institution: FinancialInstitution {
                id: resource_id(self.institution_id, "financial institution")?,
                key: self.institution_key,
                display_name: self.institution_name,
            },
            protection_eligible: self.is_deposit_protection_eligible,
            rate_reference: from_db_str(&self.rate_reference)?,
            spread_bp: self.spread_bp,
            minimum_interest_balance_krw: self.minimum_interest_balance_krw,
            minimum_contribution_krw: self.minimum_amount_krw,
            maximum_contribution_krw: self.maximum_amount_krw,
            term_days: self.term_days,
            term_months: self.term_months,
            installment_count: self.installment_count,
            early_termination_rate_bp: self
                .early_termination_rate_bp
                .map(i32::try_from)
                .transpose()
                .context("cash-product early-termination rate does not fit basis points")?,
            day_count_denominator: self.day_count_denominator,
        })
    }
}

async fn lock_save_for_user(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
) -> Result<Option<LockedSaveRow>> {
    sqlx::query_as(
        "SELECT save.id, save.market_world_id, save.policy_set_id,
                save.run_revision, save.state_revision, save.game_day,
                EXISTS(SELECT 1 FROM `character` WHERE `character`.save_id = save.id)
                    AS has_character
         FROM save WHERE save.user_id = ? FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock the finance command save")
}

fn validate_current(current: &LockedSaveRow, cursor: CommandCursor) -> Option<FinanceFailureCode> {
    if !current.has_character {
        return Some(FinanceFailureCode::CharacterRequired);
    }
    if current.run_revision != cursor.expected_run_revision
        || current.state_revision != cursor.expected_state_revision
        || current.game_day != cursor.expected_game_day
    {
        return Some(FinanceFailureCode::Busy);
    }
    None
}

async fn read_product_terms(
    tx: &mut Transaction<'_, MySql>,
    product_version_id: ResourceId,
) -> Result<Option<ProductTermsRow>> {
    sqlx::query_as(
        "SELECT id, product_kind, rate_reference, spread_bp, minimum_interest_balance_krw,
                minimum_amount_krw, maximum_amount_krw, term_days, term_months,
                installment_count, early_termination_rate_bp, day_count_denominator
         FROM cash_product_version WHERE id = ?",
    )
    .bind(product_version_id.get())
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read cash-product terms")
}

async fn increment_state_revision(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedSaveRow,
) -> Result<GameCommandCursor> {
    let state_revision = current
        .state_revision
        .checked_add(1)
        .context("state revision overflowed in a cash-product command")?;
    let update = sqlx::query(
        "UPDATE save SET state_revision = ?
         WHERE id = ? AND market_world_id = ? AND policy_set_id = ?
           AND run_revision = ? AND state_revision = ? AND game_day = ?",
    )
    .bind(state_revision)
    .bind(current.id)
    .bind(current.market_world_id)
    .bind(current.policy_set_id)
    .bind(current.run_revision)
    .bind(current.state_revision)
    .bind(current.game_day)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "cash-product save cursor changed under its lock"
    );

    Ok(GameCommandCursor {
        run_revision: current.run_revision,
        state_revision,
        game_day: current.game_day,
    })
}

async fn insert_scheduled_settlement(
    tx: &mut Transaction<'_, MySql>,
    insert: ScheduledSettlementInsert<'_>,
) -> Result<u64> {
    let insert = sqlx::query(
        "INSERT INTO scheduled_settlement
             (save_id, run_revision, due_game_day, kind, payload,
              source_kind, source_id, occurrence, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'pending')",
    )
    .bind(insert.save_id)
    .bind(insert.run_revision)
    .bind(insert.due_game_day)
    .bind(insert.kind)
    .bind(insert.payload)
    .bind(insert.source_kind)
    .bind(insert.source_id)
    .bind(insert.occurrence)
    .execute(&mut **tx)
    .await?;
    Ok(insert.last_insert_id())
}

struct ScheduledSettlementInsert<'a> {
    save_id: u64,
    run_revision: u32,
    due_game_day: u32,
    kind: &'a str,
    payload: &'a str,
    source_kind: &'a str,
    source_id: &'a str,
    occurrence: u32,
}

async fn read_receipt<T: DeserializeOwned>(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    command_id: &CommandId,
    command_kind: &str,
    payload_sha256: &str,
) -> Result<Option<T>> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT command_kind, payload_sha256, CAST(result AS CHAR)
         FROM command_receipt
         WHERE save_id = ? AND command_id = ? FOR SHARE",
    )
    .bind(save_id)
    .bind(command_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;
    let Some((stored_kind, stored_hash, result_json)) = row else {
        return Ok(None);
    };
    ensure!(
        stored_kind == command_kind && stored_hash == payload_sha256,
        "cash-product receipt disagrees with command identity"
    );
    serde_json::from_str(&result_json)
        .map(Some)
        .context("cash-product receipt result is invalid")
}

fn open_cma_fingerprint(command: &OpenCmaAccountCommand) -> String {
    fingerprint(&format!(
        "lifeledger.finance.open-account.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\ntype=cma\nproductVersionId={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.product_version_id
    ))
}

fn close_cma_fingerprint(command: &CloseCmaAccountCommand) -> String {
    fingerprint(&format!(
        "lifeledger.finance.close-account.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\naccountId={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.account_id
    ))
}

fn open_deposit_fingerprint(command: &OpenCashProductCommand) -> String {
    fingerprint(&format!(
        "lifeledger.finance.open-deposit.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\nkind={}\nproductVersionId={}\nsettlementAccountId={}\namountKrw={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        cash_product_kind_str(command.kind),
        command.product_version_id,
        command.settlement_account_id,
        command.amount_krw
    ))
}

fn close_deposit_fingerprint(command: &CloseCashProductCommand) -> String {
    fingerprint(&format!(
        "lifeledger.finance.close-deposit.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\ncontractId={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.contract_id
    ))
}

fn fingerprint(canonical: &str) -> String {
    Sha256::digest(canonical.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

const fn cash_product_kind_str(kind: CashProductKind) -> &'static str {
    match kind {
        CashProductKind::CmaRp => "cmaRp",
        CashProductKind::CmaIssuedNote => "cmaIssuedNote",
        CashProductKind::TermDeposit => "termDeposit",
        CashProductKind::InstallmentSavings => "installmentSavings",
    }
}

const fn product_rate_available(annual_rate_bp: i32, early_rate_bp: Option<i32>) -> bool {
    annual_rate_bp >= 0
        && annual_rate_bp <= 10_000
        && match early_rate_bp {
            Some(early_rate_bp) => early_rate_bp >= 0 && annual_rate_bp >= early_rate_bp,
            None => true,
        }
}

fn resource_id(value: u64, kind: &str) -> Result<ResourceId> {
    ResourceId::parse(&value.to_string()).with_context(|| format!("stored {kind} ID is invalid"))
}

fn from_db_str<T: DeserializeOwned>(raw: &str) -> Result<T> {
    serde_json::from_value(Value::String(raw.to_owned()))
        .with_context(|| format!("unknown cash-product value stored: {raw}"))
}

async fn open_cash_product(
    store: &MySqlFinanceStore,
    user_id: u64,
    command: &OpenCashProductCommand,
) -> Result<CashProductStoreResult<OpenCashProductReceipt>> {
    let fingerprint = open_deposit_fingerprint(command);
    let mut tx = store.pool.begin().await?;
    let Some(current) = lock_save_for_user(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(CashProductStoreResult::Rejected(
            FinanceFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_OPEN_DEPOSIT,
        payload_sha256: &fingerprint,
        cursor: command.cursor,
    };
    match inspect_command_identity(&mut tx, current.id, &identity).await? {
        CommandIdentityState::Conflict => {
            tx.commit().await?;
            return Ok(CashProductStoreResult::Rejected(
                FinanceFailureCode::IdempotencyConflict,
            ));
        }
        CommandIdentityState::Matching => {
            let mut receipt: OpenCashProductReceipt = read_receipt(
                &mut tx,
                current.id,
                &command.command_id,
                COMMAND_KIND_OPEN_DEPOSIT,
                &fingerprint,
            )
            .await?
            .context("open-deposit command identity has no final receipt")?;
            receipt.replayed = true;
            let save = read_state(&mut tx, current.id).await?;
            tx.commit().await?;
            return Ok(CashProductStoreResult::Applied {
                receipt,
                save: Box::new(save),
            });
        }
        CommandIdentityState::Missing => {}
    }
    if let Some(rejection) = validate_current(&current, command.cursor) {
        tx.commit().await?;
        return Ok(CashProductStoreResult::Rejected(rejection));
    }
    if !command.kind.is_deposit() || command.amount_krw <= 0 {
        tx.commit().await?;
        return Ok(CashProductStoreResult::Rejected(
            FinanceFailureCode::InvalidCommand,
        ));
    }

    let account: Option<LockedAccountRow> = sqlx::query_as(
        "SELECT id, account_type, status, cash_krw, is_default
         FROM financial_account
         WHERE save_id = ? AND run_revision = ? AND id = ?
         FOR UPDATE",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(command.settlement_account_id.get())
    .fetch_optional(&mut *tx)
    .await?;
    let Some(account) = account else {
        tx.commit().await?;
        return Ok(CashProductStoreResult::Rejected(
            FinanceFailureCode::AccountNotFound,
        ));
    };
    let account_type: FinancialAccountType = from_db_str(&account.account_type)?;
    if cash_product_tax_treatment(account_type).is_none() {
        tx.commit().await?;
        return Ok(CashProductStoreResult::Rejected(
            FinanceFailureCode::AccountTypeNotAllowed,
        ));
    }
    if account.status != "open" {
        tx.commit().await?;
        return Ok(CashProductStoreResult::Rejected(
            FinanceFailureCode::AccountClosed,
        ));
    }
    if account.cash_krw < command.amount_krw {
        tx.commit().await?;
        return Ok(CashProductStoreResult::Rejected(
            FinanceFailureCode::InsufficientAccountCash,
        ));
    }

    let contract_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM cash_product_contract
         WHERE save_id = ? AND run_revision = ?",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .fetch_one(&mut *tx)
    .await?;
    if contract_count.0 >= MAX_CASH_PRODUCT_CONTRACTS {
        tx.commit().await?;
        return Ok(CashProductStoreResult::Rejected(
            FinanceFailureCode::LimitExceeded,
        ));
    }
    let Some(product) = read_product_terms(&mut tx, command.product_version_id).await? else {
        tx.commit().await?;
        return Ok(CashProductStoreResult::Rejected(
            FinanceFailureCode::ProductNotFound,
        ));
    };
    let product_kind: CashProductKind = from_db_str(&product.product_kind)?;
    if product_kind != command.kind {
        tx.commit().await?;
        return Ok(CashProductStoreResult::Rejected(
            FinanceFailureCode::ProductNotFound,
        ));
    }
    let (Some(minimum), Some(maximum)) = (product.minimum_amount_krw, product.maximum_amount_krw)
    else {
        bail!("deposit catalog row has no contribution range");
    };
    if !(minimum..=maximum).contains(&command.amount_krw) {
        tx.commit().await?;
        return Ok(CashProductStoreResult::Rejected(
            FinanceFailureCode::InvalidCommand,
        ));
    }
    let market = read_market_terms(&mut tx, current.market_world_id, current.game_day).await?;
    let Some(market_date) = market.market_date else {
        tx.commit().await?;
        return Ok(CashProductStoreResult::Rejected(
            FinanceFailureCode::RateUnavailable,
        ));
    };
    let Some(treasury_3m_bp) = market.treasury_3m_bp else {
        tx.commit().await?;
        return Ok(CashProductStoreResult::Rejected(
            FinanceFailureCode::RateUnavailable,
        ));
    };
    if product.rate_reference != "treasury3mBp" {
        bail!("cash-product catalog uses an unsupported rate reference");
    }
    let early_rate = product
        .early_termination_rate_bp
        .context("deposit catalog row has no early-termination rate")?;
    let early_rate = i32::try_from(early_rate)
        .context("deposit early-termination rate does not fit basis points")?;
    let annual_rate_bp = i32::try_from(treasury_3m_bp)
        .ok()
        .and_then(|rate| rate.checked_add(product.spread_bp));
    let Some(annual_rate_bp) =
        annual_rate_bp.filter(|rate| product_rate_available(*rate, Some(early_rate)))
    else {
        tx.commit().await?;
        return Ok(CashProductStoreResult::Rejected(
            FinanceFailureCode::RateUnavailable,
        ));
    };
    if product.day_count_denominator != 365 {
        bail!("cash-product catalog uses an unsupported day-count basis");
    }

    let (maturity_game_day, savings_schedule) = match command.kind {
        CashProductKind::TermDeposit => {
            let term_days = product.term_days.context("term deposit has no term days")?;
            let maturity = current
                .game_day
                .checked_add(term_days)
                .context("term-deposit maturity day overflowed")?;
            (maturity, None)
        }
        CashProductKind::InstallmentSavings => {
            let schedule = create_installment_savings_schedule(InstallmentSavingsScheduleInput {
                world_start_date: market.world_start_date,
                opened_market_date: market_date,
                opened_game_day: current.game_day,
                term_months: product
                    .term_months
                    .context("installment savings has no term months")?,
                installment_count: product
                    .installment_count
                    .context("installment savings has no installment count")?,
            })?;
            (schedule.maturity_game_day, Some(schedule))
        }
        CashProductKind::CmaRp | CashProductKind::CmaIssuedNote => unreachable!(),
    };

    let policy_context = RunPolicyContext {
        run: RunId {
            save_id: resource_id(current.id, "save")?,
            run_revision: current.run_revision,
        },
        policy_set_id: resource_id(current.policy_set_id, "policy set")?,
    };
    let ledger = create_product_principal_funding_ledger(
        &*store.rules,
        SettlementLedgerContext {
            policy: policy_context,
            source: LedgerSource {
                kind: LedgerSourceKind::CashProductEnrollment,
                source_id: command.command_id.as_str().to_owned(),
            },
            game_day: current.game_day,
            description: match command.kind {
                CashProductKind::TermDeposit => "정기예금 가입".to_owned(),
                CashProductKind::InstallmentSavings => "정기적금 첫 납입".to_owned(),
                CashProductKind::CmaRp | CashProductKind::CmaIssuedNote => unreachable!(),
            },
            account_id: command.settlement_account_id,
        },
        command.amount_krw,
    )?;

    write_command_identity(&mut tx, current.id, &identity).await?;
    let ledger_id = write_ledger_transaction(&mut tx, &ledger).await?;
    let next_account_cash = account
        .cash_krw
        .checked_sub(command.amount_krw)
        .context("settlement-account cash underflowed during enrollment")?;
    let account_update = sqlx::query(
        "UPDATE financial_account SET cash_krw = ?
         WHERE save_id = ? AND run_revision = ? AND id = ?
           AND status = 'open' AND cash_krw = ?",
    )
    .bind(next_account_cash)
    .bind(current.id)
    .bind(current.run_revision)
    .bind(account.id)
    .bind(account.cash_krw)
    .execute(&mut *tx)
    .await?;
    ensure!(
        account_update.rows_affected() == 1,
        "enrollment account lost its lock"
    );

    let contract_insert = sqlx::query(
        "INSERT INTO cash_product_contract
             (save_id, run_revision, financial_account_id, product_version_id,
              contract_kind, status, principal_krw, installment_amount_krw,
              term_days, term_months, installment_count, annual_rate_bp,
              early_termination_rate_bp, day_count_denominator,
              opened_game_day, maturity_game_day)
         VALUES (?, ?, ?, ?, ?, 'active', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(account.id)
    .bind(product.id)
    .bind(cash_product_kind_str(command.kind))
    .bind((command.kind == CashProductKind::TermDeposit).then_some(command.amount_krw))
    .bind((command.kind == CashProductKind::InstallmentSavings).then_some(command.amount_krw))
    .bind(product.term_days)
    .bind(product.term_months)
    .bind(product.installment_count)
    .bind(annual_rate_bp)
    .bind(early_rate)
    .bind(product.day_count_denominator)
    .bind(current.game_day)
    .bind(maturity_game_day)
    .execute(&mut *tx)
    .await?;
    let contract_id = contract_insert.last_insert_id();

    match (command.kind, savings_schedule) {
        (CashProductKind::TermDeposit, None) => {
            let payload = serde_json::to_string(&DepositMaturityPayloadV1 {
                version: CashPayloadVersion::V1,
                account_id: command.settlement_account_id,
                contract_id: resource_id(contract_id, "cash-product contract")?,
            })?;
            insert_scheduled_settlement(
                &mut tx,
                ScheduledSettlementInsert {
                    save_id: current.id,
                    run_revision: current.run_revision,
                    due_game_day: maturity_game_day,
                    kind: "depositMaturity",
                    payload: &payload,
                    source_kind: "depositContract",
                    source_id: &contract_id.to_string(),
                    occurrence: 0,
                },
            )
            .await?;
        }
        (CashProductKind::InstallmentSavings, Some(schedule)) => {
            for installment in schedule.installments {
                let first = installment.installment_no == 1;
                sqlx::query(
                    "INSERT INTO savings_installment
                         (save_id, run_revision, contract_id, installment_no,
                          due_game_day, amount_krw, status, processed_game_day,
                          ledger_transaction_id)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(current.id)
                .bind(current.run_revision)
                .bind(contract_id)
                .bind(installment.installment_no)
                .bind(installment.due_game_day)
                .bind(command.amount_krw)
                .bind(if first { "paid" } else { "pending" })
                .bind(first.then_some(current.game_day))
                .bind(first.then_some(ledger_id))
                .execute(&mut *tx)
                .await?;
                if !first {
                    let payload = serde_json::to_string(&SavingsInstallmentPayloadV1 {
                        version: CashPayloadVersion::V1,
                        account_id: command.settlement_account_id,
                        contract_id: resource_id(contract_id, "cash-product contract")?,
                        installment_no: installment.installment_no,
                    })?;
                    insert_scheduled_settlement(
                        &mut tx,
                        ScheduledSettlementInsert {
                            save_id: current.id,
                            run_revision: current.run_revision,
                            due_game_day: installment.due_game_day,
                            kind: "savingsInstallment",
                            payload: &payload,
                            source_kind: "savingsContract",
                            source_id: &contract_id.to_string(),
                            occurrence: installment.installment_no,
                        },
                    )
                    .await?;
                }
            }
            let payload = serde_json::to_string(&SavingsMaturityPayloadV1 {
                version: CashPayloadVersion::V1,
                account_id: command.settlement_account_id,
                contract_id: resource_id(contract_id, "cash-product contract")?,
            })?;
            insert_scheduled_settlement(
                &mut tx,
                ScheduledSettlementInsert {
                    save_id: current.id,
                    run_revision: current.run_revision,
                    due_game_day: maturity_game_day,
                    kind: "savingsMaturity",
                    payload: &payload,
                    source_kind: "savingsContract",
                    source_id: &contract_id.to_string(),
                    occurrence: 0,
                },
            )
            .await?;
        }
        _ => bail!("cash-product schedule disagrees with its kind"),
    }

    let committed = increment_state_revision(&mut tx, &current).await?;
    let receipt = OpenCashProductReceipt {
        command_id: command.command_id.clone(),
        contract_id: resource_id(contract_id, "cash-product contract")?,
        product_version_id: command.product_version_id,
        settlement_account_id: command.settlement_account_id,
        kind: command.kind,
        amount_krw: command.amount_krw,
        replayed: false,
    };
    write_game_command_receipt(
        &mut tx,
        GameCommandReceiptWrite {
            save_id: current.id,
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_OPEN_DEPOSIT,
            payload_sha256: &fingerprint,
            market_world_id: current.market_world_id,
            committed_cursor: committed,
            result: &receipt,
            ledger_transaction_id: Some(ledger_id),
        },
    )
    .await?;
    let save = read_state(&mut tx, current.id).await?;
    tx.commit().await?;

    Ok(CashProductStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

async fn close_cash_product(
    store: &MySqlFinanceStore,
    user_id: u64,
    command: &CloseCashProductCommand,
) -> Result<CashProductStoreResult<CloseCashProductReceipt>> {
    let fingerprint = close_deposit_fingerprint(command);
    let mut tx = store.pool.begin().await?;
    let Some(current) = lock_save_for_user(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(CashProductStoreResult::Rejected(
            FinanceFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_CLOSE_DEPOSIT,
        payload_sha256: &fingerprint,
        cursor: command.cursor,
    };
    match inspect_command_identity(&mut tx, current.id, &identity).await? {
        CommandIdentityState::Conflict => {
            tx.commit().await?;
            return Ok(CashProductStoreResult::Rejected(
                FinanceFailureCode::IdempotencyConflict,
            ));
        }
        CommandIdentityState::Matching => {
            let mut receipt: CloseCashProductReceipt = read_receipt(
                &mut tx,
                current.id,
                &command.command_id,
                COMMAND_KIND_CLOSE_DEPOSIT,
                &fingerprint,
            )
            .await?
            .context("close-deposit command identity has no final receipt")?;
            receipt.replayed = true;
            let save = read_state(&mut tx, current.id).await?;
            tx.commit().await?;
            return Ok(CashProductStoreResult::Applied {
                receipt,
                save: Box::new(save),
            });
        }
        CommandIdentityState::Missing => {}
    }
    if let Some(rejection) = validate_current(&current, command.cursor) {
        tx.commit().await?;
        return Ok(CashProductStoreResult::Rejected(rejection));
    }

    // The first read resolves the parent account; the lock is taken in the global order below.
    let candidate: Option<(u64,)> = sqlx::query_as(
        "SELECT financial_account_id FROM cash_product_contract
         WHERE save_id = ? AND run_revision = ? AND id = ?",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(command.contract_id.get())
    .fetch_optional(&mut *tx)
    .await?;
    let Some((account_id,)) = candidate else {
        tx.commit().await?;
        return Ok(CashProductStoreResult::Rejected(
            FinanceFailureCode::ContractNotFound,
        ));
    };
    let account: LockedAccountRow = sqlx::query_as(
        "SELECT id, account_type, status, cash_krw, is_default
         FROM financial_account
         WHERE save_id = ? AND run_revision = ? AND id = ? FOR UPDATE",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(account_id)
    .fetch_one(&mut *tx)
    .await?;
    let contract: Option<LockedContractRow> = sqlx::query_as(
        "SELECT id, financial_account_id, contract_kind, status,
                principal_krw, annual_rate_bp,
                early_termination_rate_bp, day_count_denominator,
                opened_game_day, maturity_game_day
         FROM cash_product_contract
         WHERE save_id = ? AND run_revision = ? AND id = ? FOR UPDATE",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(command.contract_id.get())
    .fetch_optional(&mut *tx)
    .await?;
    let Some(contract) = contract else {
        tx.commit().await?;
        return Ok(CashProductStoreResult::Rejected(
            FinanceFailureCode::ContractNotFound,
        ));
    };
    ensure!(
        contract.financial_account_id == account.id,
        "contract parent account changed"
    );
    if contract.status != "active" || current.game_day >= contract.maturity_game_day {
        tx.commit().await?;
        return Ok(CashProductStoreResult::Rejected(
            FinanceFailureCode::ContractClosed,
        ));
    }
    if account.status != "open" {
        tx.commit().await?;
        return Ok(CashProductStoreResult::Rejected(
            FinanceFailureCode::AccountClosed,
        ));
    }
    let account_type: FinancialAccountType = from_db_str(&account.account_type)?;
    ensure!(
        cash_product_tax_treatment(account_type).is_some(),
        "cash-product contract has a disallowed parent account type"
    );
    if contract.day_count_denominator != 365 {
        bail!("cash-product contract uses an unsupported day-count basis");
    }

    let market = read_market_terms(&mut tx, current.market_world_id, current.game_day).await?;
    let market_date = market
        .market_date
        .context("current market day is missing during early close")?;
    let policy = read_cash_product_policy(&mut tx, current.policy_set_id, market_date).await?;
    let kind: CashProductKind = from_db_str(&contract.contract_kind)?;
    let early_close_rate_bp = i32::try_from(contract.early_termination_rate_bp)
        .context("stored early-termination rate does not fit basis points")?;
    let (
        principal_krw,
        withholding,
        financial_income_delta,
        tax_advantaged_interest_delta,
        cash_payout_krw,
    ) = match kind {
        CashProductKind::TermDeposit => {
            let payout = settle_term_deposit_early_close_for_account(
                TermDepositContract {
                    principal_krw: contract
                        .principal_krw
                        .context("term-deposit contract has no principal")?,
                    annual_rate_bp: contract.annual_rate_bp,
                    early_close_rate_bp,
                    opened_game_day: contract.opened_game_day,
                    maturity_game_day: contract.maturity_game_day,
                    day_count_basis: crate::finance::DayCountBasis::Actual365,
                },
                current.game_day,
                account_type,
                policy.interest_tax,
            )?;
            (
                payout.principal_krw,
                payout.withholding,
                payout.financial_income_delta,
                payout.tax_advantaged_interest_delta,
                payout.cash_payout_krw,
            )
        }
        CashProductKind::InstallmentSavings => {
            let installments: Vec<PaidInstallmentRow> = sqlx::query_as(
                "SELECT installment_no, amount_krw, processed_game_day
                 FROM savings_installment
                 WHERE save_id = ? AND run_revision = ? AND contract_id = ?
                   AND status = 'paid'
                 ORDER BY installment_no FOR UPDATE",
            )
            .bind(current.id)
            .bind(current.run_revision)
            .bind(contract.id)
            .fetch_all(&mut *tx)
            .await?;
            let payout = settle_installment_savings_early_close_for_account(
                &InstallmentSavingsContract {
                    annual_rate_bp: contract.annual_rate_bp,
                    early_close_rate_bp,
                    maturity_game_day: contract.maturity_game_day,
                    day_count_basis: crate::finance::DayCountBasis::Actual365,
                    paid_installments: installments
                        .into_iter()
                        .map(PaidInstallmentRow::into_principal)
                        .collect::<Result<Vec<_>>>()?,
                },
                current.game_day,
                account_type,
                policy.interest_tax,
            )?;
            (
                payout.principal_krw,
                payout.withholding,
                payout.financial_income_delta,
                payout.tax_advantaged_interest_delta,
                payout.cash_payout_krw,
            )
        }
        CashProductKind::CmaRp | CashProductKind::CmaIssuedNote => {
            bail!("CMA catalog kind was stored as a deposit contract")
        }
    };
    let next_account_cash = account
        .cash_krw
        .checked_add(cash_payout_krw)
        .context("settlement-account cash overflowed during early close")?;
    let ledger = create_interest_payout_ledger_for_account(
        &*store.rules,
        SettlementLedgerContext {
            policy: RunPolicyContext {
                run: RunId {
                    save_id: resource_id(current.id, "save")?,
                    run_revision: current.run_revision,
                },
                policy_set_id: resource_id(current.policy_set_id, "policy set")?,
            },
            source: LedgerSource {
                kind: LedgerSourceKind::CashProductClose,
                source_id: command.command_id.as_str().to_owned(),
            },
            game_day: current.game_day,
            description: "예적금 중도해지".to_owned(),
            account_id: resource_id(account.id, "financial account")?,
        },
        principal_krw,
        withholding,
        account_type,
        policy.interest_tax,
    )?
    .context("cash-product early close produced no ledger movement")?;

    write_command_identity(&mut tx, current.id, &identity).await?;
    let ledger_id = write_ledger_transaction(&mut tx, &ledger).await?;
    let account_update = sqlx::query(
        "UPDATE financial_account SET cash_krw = ?
         WHERE save_id = ? AND run_revision = ? AND id = ?
           AND status = 'open' AND cash_krw = ?",
    )
    .bind(next_account_cash)
    .bind(current.id)
    .bind(current.run_revision)
    .bind(account.id)
    .bind(account.cash_krw)
    .execute(&mut *tx)
    .await?;
    ensure!(
        account_update.rows_affected() == 1,
        "early-close account lost its lock"
    );
    sqlx::query(
        "UPDATE savings_installment
         SET status = 'cancelled', processed_game_day = ?, cancellation_reason = ?
         WHERE save_id = ? AND run_revision = ? AND contract_id = ? AND status = 'pending'",
    )
    .bind(current.game_day)
    .bind(CANCELLATION_CONTRACT_CLOSED)
    .bind(current.id)
    .bind(current.run_revision)
    .bind(contract.id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE scheduled_settlement
         SET status = 'cancelled', cancellation_reason = ?
         WHERE save_id = ? AND run_revision = ? AND status = 'pending'
           AND source_kind IN ('depositContract', 'savingsContract') AND source_id = ?",
    )
    .bind(CANCELLATION_CONTRACT_CLOSED)
    .bind(current.id)
    .bind(current.run_revision)
    .bind(contract.id.to_string())
    .execute(&mut *tx)
    .await?;
    let contract_update = sqlx::query(
        "UPDATE cash_product_contract
         SET status = 'closedEarly', closed_game_day = ?, closing_ledger_transaction_id = ?
         WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'active'",
    )
    .bind(current.game_day)
    .bind(ledger_id)
    .bind(current.id)
    .bind(current.run_revision)
    .bind(contract.id)
    .execute(&mut *tx)
    .await?;
    ensure!(
        contract_update.rows_affected() == 1,
        "early-close contract lost its lock"
    );
    accrue_cash_financial_income(
        &mut tx,
        AnnualTaxRunContext {
            save_id: current.id,
            run_revision: current.run_revision,
            policy_set_id: current.policy_set_id,
            game_day: current.game_day,
            market_date,
        },
        FinancialIncomeSource::DepositInterest,
        financial_income_delta,
    )
    .await?;
    apply_tax_advantaged_interest_delta(
        &mut tx,
        current.id,
        current.run_revision,
        account.id,
        tax_advantaged_interest_delta,
    )
    .await?;

    let committed = increment_state_revision(&mut tx, &current).await?;
    let receipt = CloseCashProductReceipt {
        command_id: command.command_id.clone(),
        contract_id: command.contract_id,
        gross_interest_krw: withholding.gross_interest_krw,
        income_tax_krw: withholding.income_tax_krw,
        local_income_tax_krw: withholding.local_income_tax_krw,
        net_payout_krw: cash_payout_krw,
        replayed: false,
    };
    write_game_command_receipt(
        &mut tx,
        GameCommandReceiptWrite {
            save_id: current.id,
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_CLOSE_DEPOSIT,
            payload_sha256: &fingerprint,
            market_world_id: current.market_world_id,
            committed_cursor: committed,
            result: &receipt,
            ledger_transaction_id: Some(ledger_id),
        },
    )
    .await?;
    let save = read_state(&mut tx, current.id).await?;
    tx.commit().await?;

    Ok(CashProductStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct MarketTermsRow {
    world_start_date: Date,
    market_date: Option<Date>,
    treasury_3m_bp: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedContractRow {
    id: u64,
    financial_account_id: u64,
    contract_kind: String,
    status: String,
    principal_krw: Option<i64>,
    annual_rate_bp: i32,
    early_termination_rate_bp: u32,
    day_count_denominator: u32,
    opened_game_day: u32,
    maturity_game_day: u32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PaidInstallmentRow {
    installment_no: u32,
    amount_krw: i64,
    processed_game_day: Option<u32>,
}

impl PaidInstallmentRow {
    fn into_principal(self) -> Result<SavingsInstallmentPrincipal> {
        Ok(SavingsInstallmentPrincipal {
            installment_no: self.installment_no,
            principal_krw: self.amount_krw,
            paid_game_day: self
                .processed_game_day
                .context("paid savings installment has no processed game day")?,
        })
    }
}

async fn read_market_terms(
    tx: &mut Transaction<'_, MySql>,
    market_world_id: u64,
    game_day: u32,
) -> Result<MarketTermsRow> {
    sqlx::query_as(
        "SELECT world.start_date AS world_start_date,
                daily.market_date, daily.treasury_3m_bp
         FROM market_world AS world
         LEFT JOIN market_daily AS daily
           ON daily.world_id = world.id AND daily.game_day = ?
         WHERE world.id = ?",
    )
    .bind(game_day)
    .bind(market_world_id)
    .fetch_one(&mut **tx)
    .await
    .context("failed to read cash-product market terms")
}

async fn read_cash_product_policy(
    tx: &mut Transaction<'_, MySql>,
    policy_set_id: u64,
    market_date: Date,
) -> Result<CashProductPolicy> {
    let tax: Option<(i64, i64)> = sqlx::query_as(
        "SELECT CAST(JSON_UNQUOTE(JSON_EXTRACT(parameters, '$.incomeTaxPpm')) AS SIGNED),
                CAST(JSON_UNQUOTE(JSON_EXTRACT(parameters, '$.localIncomeTaxPpm')) AS SIGNED)
         FROM policy_rule
         WHERE policy_set_id = ? AND domain = 'tax'
           AND rule_key = 'generalFinancialIncome'
           AND effective_from <= ?
           AND (effective_to IS NULL OR effective_to >= ?)
         ORDER BY effective_from DESC LIMIT 1",
    )
    .bind(policy_set_id)
    .bind(market_date)
    .bind(market_date)
    .fetch_optional(&mut **tx)
    .await?;
    let protection: Option<(i64,)> = sqlx::query_as(
        "SELECT CAST(JSON_UNQUOTE(JSON_EXTRACT(parameters, '$.limitKrw')) AS SIGNED)
         FROM policy_rule
         WHERE policy_set_id = ? AND domain = 'deposit' AND rule_key = 'protection'
           AND effective_from <= ?
           AND (effective_to IS NULL OR effective_to >= ?)
         ORDER BY effective_from DESC LIMIT 1",
    )
    .bind(policy_set_id)
    .bind(market_date)
    .bind(market_date)
    .fetch_optional(&mut **tx)
    .await?;
    let (income_tax_rate_ppm, local_income_tax_rate_ppm) =
        tax.context("pinned policy has no financial-income tax rule")?;
    let (limit_krw,) = protection.context("pinned policy has no deposit-protection rule")?;
    let policy = CashProductPolicy {
        interest_tax: InterestTaxPolicy {
            income_tax_rate_ppm,
            local_income_tax_rate_ppm,
        },
        deposit_protection: DepositProtectionPolicy { limit_krw },
    };
    policy.validate()?;
    Ok(policy)
}

async fn accrue_cash_financial_income(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    source: FinancialIncomeSource,
    delta: FinancialIncomeDelta,
) -> Result<()> {
    if delta == FinancialIncomeDelta::ZERO {
        return Ok(());
    }
    accrue_financial_income_source(
        tx,
        context,
        FinancialIncomeAccrual {
            source,
            gross_income_krw: delta.gross_financial_income_krw,
            withheld_income_tax_krw: delta.withheld_income_tax_krw,
            withheld_local_income_tax_krw: delta.withheld_local_income_tax_krw,
        },
    )
    .await?;
    Ok(())
}

async fn apply_tax_advantaged_interest_delta(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    account_id: u64,
    delta: TaxAdvantagedInterestDelta,
) -> Result<()> {
    let update = match delta {
        TaxAdvantagedInterestDelta::None => return Ok(()),
        TaxAdvantagedInterestDelta::IsaTaxProfit { amount_krw } => {
            ensure!(
                amount_krw > 0,
                "ISA cash-product profit delta is not positive"
            );
            sqlx::query(
                "UPDATE isa_account_contract
                 SET isa_tax_profit_krw = isa_tax_profit_krw + ?
                 WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
                   AND status = 'active'",
            )
            .bind(amount_krw)
            .bind(save_id)
            .bind(run_revision)
            .bind(account_id)
            .execute(&mut **tx)
            .await?
        }
        TaxAdvantagedInterestDelta::PensionEarnings { amount_krw } => {
            ensure!(
                amount_krw > 0,
                "pension cash-product earnings delta is not positive"
            );
            sqlx::query(
                "UPDATE pension_tax_balance AS balance
                 INNER JOIN pension_account_contract AS contract
                   ON contract.save_id = balance.save_id
                  AND contract.run_revision = balance.run_revision
                  AND contract.financial_account_id = balance.financial_account_id
                 SET balance.earnings_krw = balance.earnings_krw + ?
                 WHERE balance.save_id = ? AND balance.run_revision = ?
                   AND balance.financial_account_id = ? AND contract.status = 'active'",
            )
            .bind(amount_krw)
            .bind(save_id)
            .bind(run_revision)
            .bind(account_id)
            .execute(&mut **tx)
            .await?
        }
    };
    ensure!(
        update.rows_affected() == 1,
        "cash-product tax summary lost its active parent contract"
    );
    Ok(())
}

pub(super) struct CashProductStateRead {
    pub cma_accounts: Vec<CmaAccountContractState>,
    pub cash_contracts: Vec<CashProductContractState>,
    pub deposit_protection: Vec<DepositProtectionState>,
    pub current_financial_income_year: FinancialIncomeYear,
}

pub(super) async fn read_cash_product_state(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    market_world_id: u64,
    policy_set_id: u64,
    run_revision: u32,
    game_day: u32,
) -> Result<CashProductStateRead> {
    let market: StateMarketRow = sqlx::query_as(
        "SELECT COALESCE(daily.market_date, DATE_ADD(world.start_date, INTERVAL ? DAY))
                    AS game_date,
                daily.treasury_3m_bp
         FROM market_world AS world
         LEFT JOIN market_daily AS daily
           ON daily.world_id = world.id AND daily.game_day = ?
         WHERE world.id = ?",
    )
    .bind(game_day)
    .bind(game_day)
    .bind(market_world_id)
    .fetch_one(&mut **tx)
    .await?;
    let policy = read_cash_product_policy(tx, policy_set_id, market.game_date).await?;

    let cma_rows: Vec<CmaStateRow> = sqlx::query_as(
        "SELECT contract.financial_account_id, contract.product_version_id,
                contract.rate_reference, contract.spread_bp,
                contract.minimum_interest_balance_krw,
                contract.day_count_denominator, contract.interest_remainder
         FROM cma_account_contract AS contract
         WHERE contract.save_id = ? AND contract.run_revision = ?
         ORDER BY contract.financial_account_id
         LIMIT 33",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        cma_rows.len() <= 32,
        "CMA snapshot exceeded its account bound"
    );
    let cma_accounts = cma_rows
        .into_iter()
        .map(|row| {
            ensure!(
                row.rate_reference == "treasury3mBp" && row.day_count_denominator == 365,
                "stored CMA contract uses unsupported rate terms"
            );
            let annual_rate_bp = match market.treasury_3m_bp {
                Some(rate) => {
                    let rate = i32::try_from(rate)
                        .context("stored CMA market rate does not fit basis points")?
                        .checked_add(row.spread_bp)
                        .context("stored CMA annual rate overflowed")?;
                    ensure!(
                        product_rate_available(rate, None),
                        "stored CMA annual rate is invalid"
                    );
                    Some(rate)
                }
                None => None,
            };
            Ok(CmaAccountContractState {
                account_id: resource_id(row.financial_account_id, "financial account")?,
                product_version_id: resource_id(row.product_version_id, "cash product")?,
                annual_rate_bp,
                minimum_interest_balance_krw: row.minimum_interest_balance_krw,
                interest_remainder: row.interest_remainder,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let contract_rows: Vec<ContractStateRow> = sqlx::query_as(
        "SELECT contract.id, contract.financial_account_id, account.account_type,
                contract.product_version_id,
                contract.contract_kind, contract.status, contract.principal_krw,
                contract.installment_amount_krw, contract.annual_rate_bp,
                contract.early_termination_rate_bp, contract.day_count_denominator,
                contract.opened_game_day, contract.maturity_game_day,
                product.institution_id, product.is_deposit_protection_eligible,
                CAST(SUM(CASE WHEN installment.status = 'paid' THEN installment.amount_krw ELSE 0 END) AS SIGNED)
                    AS paid_principal_krw,
                CAST(SUM(CASE WHEN installment.status = 'paid' THEN 1 ELSE 0 END) AS UNSIGNED)
                    AS paid_installment_count,
                CAST(SUM(CASE WHEN installment.status = 'missed' THEN 1 ELSE 0 END) AS UNSIGNED)
                    AS missed_installment_count
         FROM cash_product_contract AS contract
         INNER JOIN financial_account AS account
           ON account.save_id = contract.save_id
          AND account.run_revision = contract.run_revision
          AND account.id = contract.financial_account_id
         INNER JOIN cash_product_version AS product ON product.id = contract.product_version_id
         LEFT JOIN savings_installment AS installment
           ON installment.save_id = contract.save_id
          AND installment.run_revision = contract.run_revision
          AND installment.contract_id = contract.id
         WHERE contract.save_id = ? AND contract.run_revision = ?
         GROUP BY contract.id, contract.financial_account_id, account.account_type,
                  contract.product_version_id,
                  contract.contract_kind, contract.status, contract.principal_krw,
                  contract.installment_amount_krw, contract.annual_rate_bp,
                  contract.early_termination_rate_bp, contract.day_count_denominator,
                  contract.opened_game_day, contract.maturity_game_day,
                  product.institution_id, product.is_deposit_protection_eligible
         ORDER BY contract.id
         LIMIT 101",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        contract_rows.len() <= 100,
        "cash-product snapshot exceeded its contract bound"
    );
    let installment_rows: Vec<StateInstallmentRow> = sqlx::query_as(
        "SELECT contract_id, installment_no, amount_krw, processed_game_day
         FROM savings_installment
         WHERE save_id = ? AND run_revision = ? AND status = 'paid'
         ORDER BY contract_id, installment_no",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    let mut installments_by_contract = BTreeMap::<u64, Vec<SavingsInstallmentPrincipal>>::new();
    for installment in installment_rows {
        installments_by_contract
            .entry(installment.contract_id)
            .or_default()
            .push(installment.into_principal()?);
    }

    let mut cash_contracts = Vec::with_capacity(contract_rows.len());
    let mut protected_deposits = Vec::new();
    for row in contract_rows {
        if row.day_count_denominator != 365 {
            bail!("cash-product contract uses an unsupported day-count basis");
        }
        let kind: CashProductKind = from_db_str(&row.contract_kind)?;
        let account_type: FinancialAccountType = from_db_str(&row.account_type)?;
        ensure!(
            cash_product_tax_treatment(account_type).is_some(),
            "stored cash-product contract has a disallowed parent account type"
        );
        let status: CashProductContractStatus = from_db_str(&row.status)?;
        let early_close_rate_bp = i32::try_from(row.early_termination_rate_bp)
            .context("stored early-termination rate does not fit basis points")?;
        let paid_installment_count = u32::try_from(row.paid_installment_count)
            .context("paid installment count exceeded its snapshot range")?;
        let missed_installment_count = u32::try_from(row.missed_installment_count)
            .context("missed installment count exceeded its snapshot range")?;
        let active = status == CashProductContractStatus::Active;
        let current_principal_krw = if active {
            match kind {
                CashProductKind::TermDeposit => row
                    .principal_krw
                    .context("active term deposit has no principal")?,
                CashProductKind::InstallmentSavings => row.paid_principal_krw,
                CashProductKind::CmaRp | CashProductKind::CmaIssuedNote => {
                    bail!("CMA kind was stored in cash_product_contract")
                }
            }
        } else {
            0
        };
        let paid_installments = installments_by_contract.get(&row.id).cloned();
        let accrued_gross_interest_krw = if active {
            match kind {
                CashProductKind::TermDeposit => calculate_simple_interest_krw(
                    current_principal_krw,
                    row.annual_rate_bp,
                    game_day
                        .min(row.maturity_game_day)
                        .checked_sub(row.opened_game_day)
                        .context("active term deposit predates its opening day")?,
                    DayCountBasis::Actual365,
                )?,
                CashProductKind::InstallmentSavings => paid_installments
                    .as_ref()
                    .context("active savings contract has no paid installment")?
                    .iter()
                    .try_fold(0_i64, |total, installment| {
                        let held_days = game_day
                            .min(row.maturity_game_day)
                            .checked_sub(installment.paid_game_day)
                            .ok_or(crate::finance::CashProductError::InvalidGameDay)?;
                        let interest = calculate_simple_interest_krw(
                            installment.principal_krw,
                            row.annual_rate_bp,
                            held_days,
                            DayCountBasis::Actual365,
                        )?;
                        total
                            .checked_add(interest)
                            .ok_or(crate::finance::CashProductError::ArithmeticOverflow)
                    })?,
                CashProductKind::CmaRp | CashProductKind::CmaIssuedNote => unreachable!(),
            }
        } else {
            0
        };
        let expected = if active {
            Some(match kind {
                CashProductKind::TermDeposit => {
                    let payout = settle_term_deposit_maturity_for_account(
                        TermDepositContract {
                            principal_krw: current_principal_krw,
                            annual_rate_bp: row.annual_rate_bp,
                            early_close_rate_bp,
                            opened_game_day: row.opened_game_day,
                            maturity_game_day: row.maturity_game_day,
                            day_count_basis: crate::finance::DayCountBasis::Actual365,
                        },
                        row.maturity_game_day,
                        account_type,
                        policy.interest_tax,
                    )?;
                    ExpectedPayout::from_interest(payout.withholding, payout.cash_payout_krw)
                }
                CashProductKind::InstallmentSavings => {
                    let payout = settle_installment_savings_maturity_for_account(
                        &InstallmentSavingsContract {
                            annual_rate_bp: row.annual_rate_bp,
                            early_close_rate_bp,
                            maturity_game_day: row.maturity_game_day,
                            day_count_basis: crate::finance::DayCountBasis::Actual365,
                            paid_installments: paid_installments
                                .clone()
                                .context("active savings contract has no paid installment")?,
                        },
                        row.maturity_game_day,
                        account_type,
                        policy.interest_tax,
                    )?;
                    ExpectedPayout::from_interest(payout.withholding, payout.cash_payout_krw)
                }
                CashProductKind::CmaRp | CashProductKind::CmaIssuedNote => unreachable!(),
            })
        } else {
            None
        };
        if active && row.is_deposit_protection_eligible {
            protected_deposits.push(ProtectedDepositAmount {
                institution_id: row.institution_id.to_string(),
                principal_krw: current_principal_krw,
                prescribed_interest_krw: accrued_gross_interest_krw,
            });
        }
        cash_contracts.push(CashProductContractState {
            contract_id: resource_id(row.id, "cash-product contract")?,
            product_version_id: resource_id(row.product_version_id, "cash product")?,
            settlement_account_id: resource_id(row.financial_account_id, "financial account")?,
            kind,
            status,
            installment_amount_krw: row.installment_amount_krw,
            annual_rate_bp: row.annual_rate_bp,
            current_principal_krw,
            opened_game_day: row.opened_game_day,
            maturity_game_day: row.maturity_game_day,
            paid_installment_count,
            missed_installment_count,
            expected_gross_interest_krw: expected.as_ref().map(|value| value.gross_interest_krw),
            expected_income_tax_krw: expected.as_ref().map(|value| value.income_tax_krw),
            expected_local_income_tax_krw: expected
                .as_ref()
                .map(|value| value.local_income_tax_krw),
            expected_net_payout_krw: expected.as_ref().map(|value| value.net_payout_krw),
        });
    }
    let protection = aggregate_deposit_protection(&protected_deposits, policy.deposit_protection)?;
    ensure!(
        protection.len() <= 16,
        "deposit-protection snapshot exceeded its institution bound"
    );
    let deposit_protection = protection
        .into_iter()
        .map(|summary| {
            Ok(DepositProtectionState {
                institution_id: ResourceId::parse(&summary.institution_id)
                    .context("deposit-protection institution ID is invalid")?,
                eligible_amount_krw: summary.eligible_amount_krw,
                protected_amount_krw: summary.protected_amount_krw,
                unprotected_amount_krw: summary.unprotected_amount_krw,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let tax_year = u16::try_from(market.game_date.year())
        .context("current market date has an invalid tax year")?;
    let income_row: Option<FinancialIncomeYearStoredRow> = sqlx::query_as(
        "SELECT tax_year, gross_financial_income_krw, withheld_income_tax_krw,
                withheld_local_income_tax_krw
         FROM financial_income_year
         WHERE save_id = ? AND run_revision = ? AND tax_year = ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(tax_year)
    .fetch_optional(&mut **tx)
    .await?;
    let current_financial_income_year = income_row
        .map(FinancialIncomeYearStoredRow::into_income_year)
        .unwrap_or_else(|| FinancialIncomeYear::zero(tax_year));

    Ok(CashProductStateRead {
        cma_accounts,
        cash_contracts,
        deposit_protection,
        current_financial_income_year,
    })
}

#[derive(sqlx::FromRow)]
struct StateMarketRow {
    game_date: Date,
    treasury_3m_bp: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct CmaStateRow {
    financial_account_id: u64,
    product_version_id: u64,
    rate_reference: String,
    spread_bp: i32,
    minimum_interest_balance_krw: i64,
    day_count_denominator: u32,
    interest_remainder: i64,
}

#[derive(sqlx::FromRow)]
struct ContractStateRow {
    id: u64,
    financial_account_id: u64,
    account_type: String,
    product_version_id: u64,
    contract_kind: String,
    status: String,
    principal_krw: Option<i64>,
    installment_amount_krw: Option<i64>,
    annual_rate_bp: i32,
    early_termination_rate_bp: u32,
    day_count_denominator: u32,
    opened_game_day: u32,
    maturity_game_day: u32,
    institution_id: u64,
    is_deposit_protection_eligible: bool,
    paid_principal_krw: i64,
    paid_installment_count: u64,
    missed_installment_count: u64,
}

#[derive(sqlx::FromRow)]
struct StateInstallmentRow {
    contract_id: u64,
    installment_no: u32,
    amount_krw: i64,
    processed_game_day: Option<u32>,
}

impl StateInstallmentRow {
    fn into_principal(self) -> Result<SavingsInstallmentPrincipal> {
        Ok(SavingsInstallmentPrincipal {
            installment_no: self.installment_no,
            principal_krw: self.amount_krw,
            paid_game_day: self
                .processed_game_day
                .context("paid savings installment has no processed day")?,
        })
    }
}

struct ExpectedPayout {
    gross_interest_krw: i64,
    income_tax_krw: i64,
    local_income_tax_krw: i64,
    net_payout_krw: i64,
}

impl ExpectedPayout {
    const fn from_interest(withholding: WithholdingTax, net_payout_krw: i64) -> Self {
        Self {
            gross_interest_krw: withholding.gross_interest_krw,
            income_tax_krw: withholding.income_tax_krw,
            local_income_tax_krw: withholding.local_income_tax_krw,
            net_payout_krw,
        }
    }
}

#[derive(sqlx::FromRow)]
struct FinancialIncomeYearStoredRow {
    tax_year: u16,
    gross_financial_income_krw: i64,
    withheld_income_tax_krw: i64,
    withheld_local_income_tax_krw: i64,
}

impl FinancialIncomeYearStoredRow {
    const fn into_income_year(self) -> FinancialIncomeYear {
        FinancialIncomeYear {
            tax_year: self.tax_year,
            gross_financial_income_krw: self.gross_financial_income_krw,
            withheld_income_tax_krw: self.withheld_income_tax_krw,
            withheld_local_income_tax_krw: self.withheld_local_income_tax_krw,
        }
    }
}

pub(super) struct CashProductSettlementInput<'a> {
    pub(super) rules: Arc<dyn FinanceRules>,
    pub(super) save_id: u64,
    pub(super) run_revision: u32,
    pub(super) policy_set_id: u64,
    pub(super) target_game_day: u32,
    pub(super) market: &'a crate::market::MarketDay,
    pub(super) settlement_id: u64,
}

pub(super) async fn settle_cash_product_by_id_in_tx(
    tx: &mut Transaction<'_, MySql>,
    input: CashProductSettlementInput<'_>,
) -> Result<()> {
    let CashProductSettlementInput {
        rules,
        save_id,
        run_revision,
        policy_set_id,
        target_game_day,
        market,
        settlement_id,
    } = input;
    let candidates = read_due_rows(
        tx,
        save_id,
        run_revision,
        target_game_day,
        settlement_id,
        false,
    )
    .await?;
    ensure!(
        candidates.len() == 1 && candidates[0].id == settlement_id,
        "cash settlement does not belong to the due set"
    );
    ensure!(
        candidates
            .iter()
            .all(|settlement| settlement.due_game_day == target_game_day),
        "cash settlement pipeline found an overdue item"
    );
    let tasks = candidates
        .iter()
        .map(DueSettlementRow::decode)
        .collect::<Result<Vec<_>>>()?;
    let tasks_by_id = tasks
        .iter()
        .map(|task| (task.id, *task))
        .collect::<BTreeMap<_, _>>();

    let mut account_ids = tasks
        .iter()
        .map(|task| task.payload.account_id().get())
        .collect::<Vec<_>>();
    account_ids.sort_unstable();
    account_ids.dedup();
    let mut accounts = BTreeMap::new();
    let mut account_types = BTreeMap::new();
    for account_id in account_ids {
        let row: Option<SettlementAccountRow> = sqlx::query_as(
            "SELECT id, account_type, cash_krw FROM financial_account
             WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'open'
             FOR UPDATE",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(account_id)
        .fetch_optional(&mut **tx)
        .await?;
        let row = row.context("due cash settlement references a missing open account")?;
        let account_id = resource_id(row.id, "financial account")?;
        let account_type: FinancialAccountType = from_db_str(&row.account_type)?;
        accounts.insert(account_id, row.cash_krw);
        account_types.insert(account_id, account_type);
    }

    let mut contract_ids = tasks
        .iter()
        .filter_map(|task| match task.payload {
            CashSettlementPayload::DepositMaturity(payload) => Some(payload.contract_id.get()),
            CashSettlementPayload::SavingsInstallment(payload) => Some(payload.contract_id.get()),
            CashSettlementPayload::SavingsMaturity(payload) => Some(payload.contract_id.get()),
            CashSettlementPayload::CmaInterest(_) => None,
        })
        .collect::<Vec<_>>();
    contract_ids.sort_unstable();
    contract_ids.dedup();
    let mut contracts = BTreeMap::new();
    for contract_id in contract_ids {
        let row: Option<SettlementContractRow> = sqlx::query_as(
            "SELECT id, financial_account_id, contract_kind, status, principal_krw,
                    annual_rate_bp, early_termination_rate_bp,
                    day_count_denominator, opened_game_day, maturity_game_day
             FROM cash_product_contract
             WHERE save_id = ? AND run_revision = ? AND id = ? FOR UPDATE",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(contract_id)
        .fetch_optional(&mut **tx)
        .await?;
        let row = row.context("due cash settlement references a missing contract")?;
        contracts.insert(resource_id(row.id, "cash-product contract")?, row);
    }

    let mut installments = BTreeMap::<ResourceId, Vec<SettlementInstallmentRow>>::new();
    for (contract_id, contract) in &contracts {
        if contract.contract_kind != "installmentSavings" {
            continue;
        }
        let rows: Vec<SettlementInstallmentRow> = sqlx::query_as(
            "SELECT installment_no, due_game_day, amount_krw, status, processed_game_day
             FROM savings_installment
             WHERE save_id = ? AND run_revision = ? AND contract_id = ?
             ORDER BY installment_no FOR UPDATE",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(contract_id.get())
        .fetch_all(&mut **tx)
        .await?;
        installments.insert(*contract_id, rows);
    }

    let mut cma_terms = BTreeMap::new();
    for task in &tasks {
        let CashSettlementPayload::CmaInterest(payload) = task.payload else {
            continue;
        };
        let row: Option<SettlementCmaRow> = sqlx::query_as(
            "SELECT id, financial_account_id, spread_bp,
                    minimum_interest_balance_krw, day_count_denominator,
                    interest_remainder
             FROM cma_account_contract
             WHERE save_id = ? AND run_revision = ? AND id = ? FOR UPDATE",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(payload.cma_terms_id.get())
        .fetch_optional(&mut **tx)
        .await?;
        let row = row.context("due CMA settlement references missing copied terms")?;
        ensure!(
            row.financial_account_id == payload.account_id.get(),
            "CMA payload account disagrees with copied terms"
        );
        cma_terms.insert(payload.account_id, row);
    }

    let locked = read_due_rows(
        tx,
        save_id,
        run_revision,
        target_game_day,
        settlement_id,
        true,
    )
    .await?;
    ensure!(
        locked == candidates,
        "due settlement set changed after parent locks"
    );
    let treasury_3m_bp = market
        .rates
        .as_ref()
        .map(|rates| rates.treasury_3m_bp)
        .map(i32::try_from)
        .transpose()
        .context("treasury 3-month rate does not fit basis points")?;
    let policy = read_cash_product_policy(tx, policy_set_id, market.market_date).await?;
    let registry = Arc::new(SqlCashSettlementRegistry {
        rules,
        policy_context: RunPolicyContext {
            run: RunId {
                save_id: resource_id(save_id, "save")?,
                run_revision,
            },
            policy_set_id: resource_id(policy_set_id, "policy set")?,
        },
        treasury_3m_bp,
        cma_terms,
        contracts,
        installments,
    });
    let plan = create_cash_settlement_planner(registry).plan(DailyCashSettlementPlanInput {
        game_day: target_game_day,
        policy,
        account_cash_by_id: accounts.clone(),
        account_type_by_id: account_types,
        settlements: tasks,
    })?;

    for (account_id, next_cash) in &plan.account_cash_by_id {
        let previous = accounts
            .get(account_id)
            .context("planned cash settlement created an unknown account")?;
        if next_cash == previous {
            continue;
        }
        let update = sqlx::query(
            "UPDATE financial_account SET cash_krw = ?
             WHERE save_id = ? AND run_revision = ? AND id = ?
               AND status = 'open' AND cash_krw = ?",
        )
        .bind(next_cash)
        .bind(save_id)
        .bind(run_revision)
        .bind(account_id.get())
        .bind(previous)
        .execute(&mut **tx)
        .await?;
        ensure!(
            update.rows_affected() == 1,
            "settlement account lost its lock"
        );
    }

    for settlement in plan.settlements {
        let task = tasks_by_id
            .get(&settlement.settlement_id)
            .context("planned settlement has no locked source task")?;
        let ledger_id = match &settlement.execution {
            CashSettlementExecution::Applied { ledger, .. } => {
                Some(write_ledger_transaction(tx, ledger).await?)
            }
            CashSettlementExecution::NoMovement { .. } => None,
        };
        apply_product_mutation(
            tx,
            save_id,
            run_revision,
            target_game_day,
            task,
            &settlement.execution,
            ledger_id,
        )
        .await?;
        apply_tax_advantaged_interest_delta(
            tx,
            save_id,
            run_revision,
            task.payload.account_id().get(),
            settlement.tax_advantaged_interest_delta,
        )
        .await?;
        let source = match task.payload {
            CashSettlementPayload::CmaInterest(_) => FinancialIncomeSource::CmaInterest,
            CashSettlementPayload::DepositMaturity(_)
            | CashSettlementPayload::SavingsInstallment(_)
            | CashSettlementPayload::SavingsMaturity(_) => FinancialIncomeSource::DepositInterest,
        };
        accrue_cash_financial_income(
            tx,
            AnnualTaxRunContext {
                save_id,
                run_revision,
                policy_set_id,
                game_day: target_game_day,
                market_date: market.market_date,
            },
            source,
            settlement.financial_income_delta,
        )
        .await?;
        match settlement.outcome {
            CashSettlementOutcome::Applied => {
                let update = sqlx::query(
                    "UPDATE scheduled_settlement
                     SET status = 'settled', outcome = 'applied',
                         settled_ledger_transaction_id = ?
                     WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'pending'",
                )
                .bind(ledger_id.context("applied cash settlement has no ledger")?)
                .bind(save_id)
                .bind(run_revision)
                .bind(settlement.settlement_id.get())
                .execute(&mut **tx)
                .await?;
                ensure!(
                    update.rows_affected() == 1,
                    "applied settlement lost its lock"
                );
            }
            CashSettlementOutcome::NoMovement(reason) => {
                let update = sqlx::query(
                    "UPDATE scheduled_settlement
                     SET status = 'settled', outcome = 'noMovement', outcome_reason = ?
                     WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'pending'",
                )
                .bind(to_db_str(&reason)?)
                .bind(save_id)
                .bind(run_revision)
                .bind(settlement.settlement_id.get())
                .execute(&mut **tx)
                .await?;
                ensure!(
                    update.rows_affected() == 1,
                    "no-movement settlement lost its lock"
                );
            }
        }
        if let Some(follow_up) = settlement.follow_up {
            let (kind, payload_json) = encode_cash_payload(follow_up.payload)?;
            insert_scheduled_settlement(
                tx,
                ScheduledSettlementInsert {
                    save_id,
                    run_revision,
                    due_game_day: follow_up.due_game_day,
                    kind: &to_db_str(&kind)?,
                    payload: &payload_json,
                    source_kind: &to_db_str(&follow_up.source.kind)?,
                    source_id: &follow_up.source.source_id.to_string(),
                    occurrence: follow_up.source.occurrence,
                },
            )
            .await?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct DueSettlementRow {
    id: u64,
    due_game_day: u32,
    kind: String,
    payload_json: String,
    source_kind: String,
    source_id: String,
    occurrence: u32,
}

impl DueSettlementRow {
    fn decode(&self) -> Result<CashSettlementTask> {
        let source_id = ResourceId::parse(&self.source_id)
            .context("cash settlement source ID is not canonical")?;
        CashSettlementTask::decode(
            resource_id(self.id, "scheduled settlement")?,
            self.due_game_day,
            CashSettlementSource {
                kind: from_db_str(&self.source_kind)?,
                source_id,
                occurrence: self.occurrence,
            },
            from_db_str(&self.kind)?,
            serde_json::from_str(&self.payload_json)
                .context("stored cash settlement payload is invalid JSON")?,
        )
        .map_err(Into::into)
    }
}

async fn read_due_rows(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    target_game_day: u32,
    settlement_id: u64,
    for_update: bool,
) -> Result<Vec<DueSettlementRow>> {
    if for_update {
        sqlx::query_as(
            "SELECT id, due_game_day, kind, CAST(payload AS CHAR) AS payload_json,
                    source_kind, source_id, occurrence
             FROM scheduled_settlement
             WHERE save_id = ? AND run_revision = ? AND status = 'pending'
               AND due_game_day <= ?
               AND id = ?
               AND kind IN ('cmaInterest', 'depositMaturity',
                            'savingsInstallment', 'savingsMaturity')
             ORDER BY due_game_day, id
             FOR UPDATE",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(target_game_day)
        .bind(settlement_id)
        .fetch_all(&mut **tx)
        .await
        .context("failed to lock due cash settlements")
    } else {
        sqlx::query_as(
            "SELECT id, due_game_day, kind, CAST(payload AS CHAR) AS payload_json,
                    source_kind, source_id, occurrence
             FROM scheduled_settlement
             WHERE save_id = ? AND run_revision = ? AND status = 'pending'
               AND due_game_day <= ?
               AND id = ?
               AND kind IN ('cmaInterest', 'depositMaturity',
                            'savingsInstallment', 'savingsMaturity')
             ORDER BY due_game_day, id",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(target_game_day)
        .bind(settlement_id)
        .fetch_all(&mut **tx)
        .await
        .context("failed to read due cash settlements")
    }
}

#[derive(sqlx::FromRow)]
struct SettlementAccountRow {
    id: u64,
    account_type: String,
    cash_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct SettlementCmaRow {
    id: u64,
    financial_account_id: u64,
    spread_bp: i32,
    minimum_interest_balance_krw: i64,
    day_count_denominator: u32,
    interest_remainder: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct SettlementContractRow {
    id: u64,
    financial_account_id: u64,
    contract_kind: String,
    status: String,
    principal_krw: Option<i64>,
    annual_rate_bp: i32,
    early_termination_rate_bp: u32,
    day_count_denominator: u32,
    opened_game_day: u32,
    maturity_game_day: u32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct SettlementInstallmentRow {
    installment_no: u32,
    due_game_day: u32,
    amount_krw: i64,
    status: String,
    processed_game_day: Option<u32>,
}

struct SqlCashSettlementRegistry {
    rules: Arc<dyn FinanceRules>,
    policy_context: RunPolicyContext,
    treasury_3m_bp: Option<i32>,
    cma_terms: BTreeMap<ResourceId, SettlementCmaRow>,
    contracts: BTreeMap<ResourceId, SettlementContractRow>,
    installments: BTreeMap<ResourceId, Vec<SettlementInstallmentRow>>,
}

impl CashSettlementExecutorRegistry for SqlCashSettlementRegistry {
    fn execute(
        &self,
        task: &CashSettlementTask,
        game_day: u32,
        current_account_cash_krw: i64,
        account_type: FinancialAccountType,
        policy: CashProductPolicy,
    ) -> std::result::Result<CashSettlementExecution, crate::finance::CashProductError> {
        match task.payload {
            CashSettlementPayload::CmaInterest(payload) => {
                let terms = self
                    .cma_terms
                    .get(&payload.account_id)
                    .ok_or(crate::finance::CashProductError::InvalidPayload)?;
                if terms.id != payload.cma_terms_id.get() || terms.day_count_denominator != 365 {
                    return Err(crate::finance::CashProductError::InvalidPayload);
                }
                let treasury_3m_bp = self
                    .treasury_3m_bp
                    .ok_or(crate::finance::CashProductError::InvalidRate)?;
                let annual_rate_bp = treasury_3m_bp
                    .checked_add(terms.spread_bp)
                    .ok_or(crate::finance::CashProductError::ArithmeticOverflow)?;
                if !product_rate_available(annual_rate_bp, None) {
                    return Err(crate::finance::CashProductError::InvalidRate);
                }
                let accrual = accrue_cma_daily(
                    &*self.rules,
                    CmaDailyAccrualInput {
                        principal_krw: current_account_cash_krw,
                        treasury_3m_bp,
                        interest_remainder: terms.interest_remainder,
                        terms: CmaDailyTerms {
                            spread_bp: terms.spread_bp,
                            minimum_interest_balance_krw: terms.minimum_interest_balance_krw,
                            day_count_basis: DayCountBasis::Actual365,
                        },
                    },
                    policy.interest_tax,
                )?;
                let mutation = CashProductMutation::CmaAccrued {
                    next_principal_krw: accrual.next_principal_krw,
                    next_interest_remainder: accrual.next_interest_remainder,
                };
                let follow_up = task.required_follow_up(game_day)?;
                match accrual.outcome {
                    CashSettlementOutcome::Applied => {
                        let ledger = create_interest_payout_ledger(
                            &*self.rules,
                            settlement_ledger_context(
                                self.policy_context,
                                task,
                                game_day,
                                "CMA 일 이자",
                            ),
                            0,
                            accrual.withholding,
                            policy.interest_tax,
                        )?
                        .ok_or(crate::finance::CashProductError::InvalidSettlementExecution)?;
                        Ok(CashSettlementExecution::Applied {
                            next_account_cash_krw: accrual.next_principal_krw,
                            ledger,
                            product_mutation: mutation,
                            financial_income_delta: FinancialIncomeDelta::from(accrual.withholding),
                            follow_up,
                        })
                    }
                    CashSettlementOutcome::NoMovement(reason) => {
                        Ok(CashSettlementExecution::NoMovement {
                            reason,
                            product_mutation: mutation,
                            financial_income_delta: FinancialIncomeDelta::ZERO,
                            follow_up,
                        })
                    }
                }
            }
            CashSettlementPayload::DepositMaturity(payload) => {
                let stored = self.active_contract(payload.contract_id, payload.account_id)?;
                if task.due_game_day != stored.maturity_game_day {
                    return Err(crate::finance::CashProductError::InvalidPayload);
                }
                let contract = self.term_contract(payload.contract_id, payload.account_id)?;
                let payout = settle_term_deposit_maturity_for_account(
                    contract,
                    game_day,
                    account_type,
                    policy.interest_tax,
                )?;
                let ledger = create_interest_payout_ledger_for_account(
                    &*self.rules,
                    settlement_ledger_context(self.policy_context, task, game_day, "정기예금 만기"),
                    payout.principal_krw,
                    payout.withholding,
                    account_type,
                    policy.interest_tax,
                )?
                .ok_or(crate::finance::CashProductError::InvalidSettlementExecution)?;
                let next_account_cash_krw = current_account_cash_krw
                    .checked_add(payout.cash_payout_krw)
                    .ok_or(crate::finance::CashProductError::ArithmeticOverflow)?;
                Ok(CashSettlementExecution::Applied {
                    next_account_cash_krw,
                    ledger,
                    product_mutation: CashProductMutation::DepositMatured,
                    financial_income_delta: payout.financial_income_delta,
                    follow_up: None,
                })
            }
            CashSettlementPayload::SavingsInstallment(payload) => {
                let contract = self.active_contract(payload.contract_id, payload.account_id)?;
                let installment = self
                    .installments
                    .get(&payload.contract_id)
                    .and_then(|rows| {
                        rows.iter().find(|row| {
                            row.installment_no == payload.installment_no && row.status == "pending"
                        })
                    })
                    .ok_or(crate::finance::CashProductError::InvalidPayload)?;
                if contract.contract_kind != "installmentSavings"
                    || installment.due_game_day != task.due_game_day
                {
                    return Err(crate::finance::CashProductError::InvalidPayload);
                }
                let collection =
                    collect_savings_installment(current_account_cash_krw, installment.amount_krw)?;
                match collection.outcome {
                    CashSettlementOutcome::Applied => {
                        let ledger = create_product_principal_funding_ledger(
                            &*self.rules,
                            settlement_ledger_context(
                                self.policy_context,
                                task,
                                game_day,
                                "정기적금 납입",
                            ),
                            collection.collected_principal_krw,
                        )?;
                        Ok(CashSettlementExecution::Applied {
                            next_account_cash_krw: collection.next_account_cash_krw,
                            ledger,
                            product_mutation: CashProductMutation::SavingsInstallmentPaid {
                                installment_no: payload.installment_no,
                                principal_krw: collection.collected_principal_krw,
                            },
                            financial_income_delta: FinancialIncomeDelta::ZERO,
                            follow_up: None,
                        })
                    }
                    CashSettlementOutcome::NoMovement(reason) => {
                        Ok(CashSettlementExecution::NoMovement {
                            reason,
                            product_mutation: CashProductMutation::SavingsInstallmentMissed {
                                installment_no: payload.installment_no,
                            },
                            financial_income_delta: FinancialIncomeDelta::ZERO,
                            follow_up: None,
                        })
                    }
                }
            }
            CashSettlementPayload::SavingsMaturity(payload) => {
                let stored = self.active_contract(payload.contract_id, payload.account_id)?;
                if task.due_game_day != stored.maturity_game_day {
                    return Err(crate::finance::CashProductError::InvalidPayload);
                }
                let contract = self.savings_contract(payload.contract_id, payload.account_id)?;
                let payout = settle_installment_savings_maturity_for_account(
                    &contract,
                    game_day,
                    account_type,
                    policy.interest_tax,
                )?;
                let ledger = create_interest_payout_ledger_for_account(
                    &*self.rules,
                    settlement_ledger_context(self.policy_context, task, game_day, "정기적금 만기"),
                    payout.principal_krw,
                    payout.withholding,
                    account_type,
                    policy.interest_tax,
                )?
                .ok_or(crate::finance::CashProductError::InvalidSettlementExecution)?;
                let next_account_cash_krw = current_account_cash_krw
                    .checked_add(payout.cash_payout_krw)
                    .ok_or(crate::finance::CashProductError::ArithmeticOverflow)?;
                Ok(CashSettlementExecution::Applied {
                    next_account_cash_krw,
                    ledger,
                    product_mutation: CashProductMutation::SavingsMatured,
                    financial_income_delta: payout.financial_income_delta,
                    follow_up: None,
                })
            }
        }
    }
}

impl SqlCashSettlementRegistry {
    fn active_contract(
        &self,
        contract_id: ResourceId,
        account_id: ResourceId,
    ) -> std::result::Result<&SettlementContractRow, crate::finance::CashProductError> {
        let contract = self
            .contracts
            .get(&contract_id)
            .ok_or(crate::finance::CashProductError::InvalidPayload)?;
        if contract.financial_account_id != account_id.get() || contract.status != "active" {
            return Err(crate::finance::CashProductError::InvalidPayload);
        }
        Ok(contract)
    }

    fn term_contract(
        &self,
        contract_id: ResourceId,
        account_id: ResourceId,
    ) -> std::result::Result<TermDepositContract, crate::finance::CashProductError> {
        let contract = self.active_contract(contract_id, account_id)?;
        if contract.contract_kind != "termDeposit" || contract.day_count_denominator != 365 {
            return Err(crate::finance::CashProductError::InvalidPayload);
        }
        Ok(TermDepositContract {
            principal_krw: contract
                .principal_krw
                .ok_or(crate::finance::CashProductError::InvalidPayload)?,
            annual_rate_bp: contract.annual_rate_bp,
            early_close_rate_bp: i32::try_from(contract.early_termination_rate_bp)
                .map_err(|_| crate::finance::CashProductError::InvalidRate)?,
            opened_game_day: contract.opened_game_day,
            maturity_game_day: contract.maturity_game_day,
            day_count_basis: DayCountBasis::Actual365,
        })
    }

    fn savings_contract(
        &self,
        contract_id: ResourceId,
        account_id: ResourceId,
    ) -> std::result::Result<InstallmentSavingsContract, crate::finance::CashProductError> {
        let contract = self.active_contract(contract_id, account_id)?;
        if contract.contract_kind != "installmentSavings" || contract.day_count_denominator != 365 {
            return Err(crate::finance::CashProductError::InvalidPayload);
        }
        let paid_installments = self
            .installments
            .get(&contract_id)
            .ok_or(crate::finance::CashProductError::InvalidPayload)?
            .iter()
            .filter(|row| row.status == "paid")
            .map(|row| {
                Ok::<_, crate::finance::CashProductError>(SavingsInstallmentPrincipal {
                    installment_no: row.installment_no,
                    principal_krw: row.amount_krw,
                    paid_game_day: row
                        .processed_game_day
                        .ok_or(crate::finance::CashProductError::InvalidInstallment)?,
                })
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(InstallmentSavingsContract {
            annual_rate_bp: contract.annual_rate_bp,
            early_close_rate_bp: i32::try_from(contract.early_termination_rate_bp)
                .map_err(|_| crate::finance::CashProductError::InvalidRate)?,
            maturity_game_day: contract.maturity_game_day,
            day_count_basis: DayCountBasis::Actual365,
            paid_installments,
        })
    }
}

fn settlement_ledger_context(
    policy: RunPolicyContext,
    task: &CashSettlementTask,
    game_day: u32,
    description: &str,
) -> SettlementLedgerContext {
    SettlementLedgerContext {
        policy,
        source: LedgerSource {
            kind: LedgerSourceKind::ScheduledSettlement,
            source_id: task.id.to_string(),
        },
        game_day,
        description: description.to_owned(),
        account_id: task.payload.account_id(),
    }
}

async fn apply_product_mutation(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
    task: &CashSettlementTask,
    execution: &CashSettlementExecution,
    ledger_id: Option<u64>,
) -> Result<()> {
    let mutation = match execution {
        CashSettlementExecution::Applied {
            product_mutation, ..
        }
        | CashSettlementExecution::NoMovement {
            product_mutation, ..
        } => product_mutation,
    };
    match mutation {
        CashProductMutation::CmaAccrued {
            next_interest_remainder,
            ..
        } => {
            let CashSettlementPayload::CmaInterest(payload) = task.payload else {
                bail!("CMA mutation disagrees with settlement payload");
            };
            sqlx::query(
                "UPDATE cma_account_contract
                 SET interest_remainder = ?
                 WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?",
            )
            .bind(next_interest_remainder)
            .bind(save_id)
            .bind(run_revision)
            .bind(payload.account_id.get())
            .execute(&mut **tx)
            .await?;
        }
        CashProductMutation::DepositMatured => {
            let CashSettlementPayload::DepositMaturity(payload) = task.payload else {
                bail!("deposit maturity mutation disagrees with settlement payload");
            };
            finish_contract(
                tx,
                save_id,
                run_revision,
                payload.contract_id,
                task.due_game_day,
                ledger_id,
                "matured",
            )
            .await?;
        }
        CashProductMutation::SavingsInstallmentPaid { installment_no, .. } => {
            let CashSettlementPayload::SavingsInstallment(payload) = task.payload else {
                bail!("savings installment mutation disagrees with settlement payload");
            };
            transition_installment(
                tx,
                InstallmentTransition {
                    save_id,
                    run_revision,
                    contract_id: payload.contract_id,
                    installment_no: *installment_no,
                    game_day,
                    status: "paid",
                    ledger_id,
                },
            )
            .await?;
        }
        CashProductMutation::SavingsInstallmentMissed { installment_no } => {
            let CashSettlementPayload::SavingsInstallment(payload) = task.payload else {
                bail!("missed installment mutation disagrees with settlement payload");
            };
            transition_installment(
                tx,
                InstallmentTransition {
                    save_id,
                    run_revision,
                    contract_id: payload.contract_id,
                    installment_no: *installment_no,
                    game_day,
                    status: "missed",
                    ledger_id: None,
                },
            )
            .await?;
        }
        CashProductMutation::SavingsMatured => {
            let CashSettlementPayload::SavingsMaturity(payload) = task.payload else {
                bail!("savings maturity mutation disagrees with settlement payload");
            };
            finish_contract(
                tx,
                save_id,
                run_revision,
                payload.contract_id,
                task.due_game_day,
                ledger_id,
                "matured",
            )
            .await?;
        }
    }
    Ok(())
}

async fn finish_contract(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    contract_id: ResourceId,
    game_day: u32,
    ledger_id: Option<u64>,
    status: &str,
) -> Result<()> {
    let update = sqlx::query(
        "UPDATE cash_product_contract
         SET status = ?, closed_game_day = ?, closing_ledger_transaction_id = ?
         WHERE save_id = ? AND run_revision = ? AND id = ?
           AND status = 'active' AND maturity_game_day <= ?",
    )
    .bind(status)
    .bind(game_day)
    .bind(ledger_id)
    .bind(save_id)
    .bind(run_revision)
    .bind(contract_id.get())
    .bind(game_day)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "maturity contract mutation was ambiguous"
    );
    Ok(())
}

async fn transition_installment(
    tx: &mut Transaction<'_, MySql>,
    transition: InstallmentTransition<'_>,
) -> Result<()> {
    let update = sqlx::query(
        "UPDATE savings_installment
         SET status = ?, processed_game_day = ?, ledger_transaction_id = ?
         WHERE save_id = ? AND run_revision = ? AND contract_id = ?
           AND installment_no = ? AND status = 'pending'",
    )
    .bind(transition.status)
    .bind(transition.game_day)
    .bind(transition.ledger_id)
    .bind(transition.save_id)
    .bind(transition.run_revision)
    .bind(transition.contract_id.get())
    .bind(transition.installment_no)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "savings installment lost its lock"
    );
    Ok(())
}

struct InstallmentTransition<'a> {
    save_id: u64,
    run_revision: u32,
    contract_id: ResourceId,
    installment_no: u32,
    game_day: u32,
    status: &'a str,
    ledger_id: Option<u64>,
}

#[cfg(test)]
fn checked_add_income_delta(
    left: FinancialIncomeDelta,
    right: FinancialIncomeDelta,
) -> Result<FinancialIncomeDelta> {
    Ok(FinancialIncomeDelta {
        gross_financial_income_krw: left
            .gross_financial_income_krw
            .checked_add(right.gross_financial_income_krw)
            .context("gross financial-income delta overflowed")?,
        withheld_income_tax_krw: left
            .withheld_income_tax_krw
            .checked_add(right.withheld_income_tax_krw)
            .context("withheld income-tax delta overflowed")?,
        withheld_local_income_tax_krw: left
            .withheld_local_income_tax_krw
            .checked_add(right.withheld_local_income_tax_krw)
            .context("withheld local-income-tax delta overflowed")?,
    })
}

fn encode_cash_payload(payload: CashSettlementPayload) -> Result<(CashSettlementKind, String)> {
    match payload {
        CashSettlementPayload::CmaInterest(payload) => Ok((
            CashSettlementKind::CmaInterest,
            serde_json::to_string(&payload)?,
        )),
        CashSettlementPayload::DepositMaturity(payload) => Ok((
            CashSettlementKind::DepositMaturity,
            serde_json::to_string(&payload)?,
        )),
        CashSettlementPayload::SavingsInstallment(payload) => Ok((
            CashSettlementKind::SavingsInstallment,
            serde_json::to_string(&payload)?,
        )),
        CashSettlementPayload::SavingsMaturity(payload) => Ok((
            CashSettlementKind::SavingsMaturity,
            serde_json::to_string(&payload)?,
        )),
    }
}

fn to_db_str<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value)? {
        Value::String(text) => Ok(text),
        other => bail!("cash-product enum is not storable as a string: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn given_command_id() -> CommandId {
        CommandId::parse("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2").expect("표준 UUID여야 한다")
    }

    fn given_cursor() -> CommandCursor {
        CommandCursor {
            expected_run_revision: 3,
            expected_state_revision: 42,
            expected_game_day: 17,
        }
    }

    fn given_locked_save() -> LockedSaveRow {
        LockedSaveRow {
            id: 1,
            market_world_id: 3,
            policy_set_id: 1,
            run_revision: 3,
            state_revision: 42,
            game_day: 17,
            has_character: true,
        }
    }

    mod context_a_deposit_command_is_fingerprinted {
        use super::*;

        fn given_open_deposit_command() -> OpenCashProductCommand {
            OpenCashProductCommand {
                command_id: given_command_id(),
                cursor: given_cursor(),
                kind: CashProductKind::TermDeposit,
                product_version_id: ResourceId::from_u64(3),
                settlement_account_id: ResourceId::from_u64(7),
                amount_krw: 1_000_000,
            }
        }

        #[test]
        fn given_the_same_fields_when_hashed_then_the_fingerprint_is_stable() {
            let command = given_open_deposit_command();

            let first = open_deposit_fingerprint(&command);
            let second = open_deposit_fingerprint(&command);

            assert_eq!(first, second);
            assert_eq!(first.len(), 64);
        }

        #[test]
        fn given_a_changed_path_resource_when_hashed_then_the_fingerprint_changes() {
            let first = CloseCashProductCommand {
                command_id: given_command_id(),
                cursor: given_cursor(),
                contract_id: ResourceId::from_u64(9),
            };
            let second = CloseCashProductCommand {
                contract_id: ResourceId::from_u64(10),
                ..first.clone()
            };

            let first_hash = close_deposit_fingerprint(&first);
            let second_hash = close_deposit_fingerprint(&second);

            assert_ne!(first_hash, second_hash);
        }

        #[test]
        fn given_a_changed_deposit_kind_when_hashed_then_the_fingerprint_changes() {
            let first = given_open_deposit_command();
            let second = OpenCashProductCommand {
                kind: CashProductKind::InstallmentSavings,
                ..first.clone()
            };

            let first_hash = open_deposit_fingerprint(&first);
            let second_hash = open_deposit_fingerprint(&second);

            assert_ne!(first_hash, second_hash);
        }
    }

    mod context_a_cash_product_cursor_is_checked {
        use super::*;

        #[test]
        fn given_no_character_when_checked_then_character_required_is_returned() {
            let current = LockedSaveRow {
                has_character: false,
                ..given_locked_save()
            };

            let rejection = validate_current(&current, given_cursor());

            assert_eq!(rejection, Some(FinanceFailureCode::CharacterRequired));
        }

        #[test]
        fn given_a_stale_revision_when_checked_then_busy_is_returned() {
            let current = given_locked_save();
            let cursor = CommandCursor {
                expected_state_revision: 41,
                ..given_cursor()
            };

            let rejection = validate_current(&current, cursor);

            assert_eq!(rejection, Some(FinanceFailureCode::Busy));
        }

        #[test]
        fn given_the_current_cursor_when_checked_then_it_is_accepted() {
            let current = given_locked_save();

            let rejection = validate_current(&current, given_cursor());

            assert_eq!(rejection, None);
        }
    }

    mod context_financial_income_deltas_are_accumulated {
        use super::*;

        #[test]
        fn given_two_valid_deltas_when_added_then_each_tax_component_is_preserved() {
            let first = FinancialIncomeDelta {
                gross_financial_income_krw: 100,
                withheld_income_tax_krw: 14,
                withheld_local_income_tax_krw: 1,
            };
            let second = FinancialIncomeDelta {
                gross_financial_income_krw: 200,
                withheld_income_tax_krw: 28,
                withheld_local_income_tax_krw: 2,
            };

            let total = checked_add_income_delta(first, second)
                .expect("금융소득 누계를 더할 수 있어야 한다");

            assert_eq!(
                total,
                FinancialIncomeDelta {
                    gross_financial_income_krw: 300,
                    withheld_income_tax_krw: 42,
                    withheld_local_income_tax_krw: 3,
                }
            );
        }

        #[test]
        fn given_an_overflowing_gross_delta_when_added_then_it_is_rejected() {
            let first = FinancialIncomeDelta {
                gross_financial_income_krw: i64::MAX,
                withheld_income_tax_krw: 0,
                withheld_local_income_tax_krw: 0,
            };
            let second = FinancialIncomeDelta {
                gross_financial_income_krw: 1,
                withheld_income_tax_krw: 0,
                withheld_local_income_tax_krw: 0,
            };

            let result = checked_add_income_delta(first, second);

            assert!(result.is_err());
        }
    }

    mod context_a_catalog_rate_is_resolved {
        use super::*;

        #[test]
        fn given_an_annual_rate_below_the_early_rate_when_checked_then_it_is_unavailable() {
            let available = product_rate_available(40, Some(50));

            assert!(!available);
        }

        #[test]
        fn given_an_annual_rate_at_the_early_rate_when_checked_then_it_is_available() {
            let available = product_rate_available(50, Some(50));

            assert!(available);
        }
    }
}
