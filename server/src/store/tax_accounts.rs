//! M2-C tax-account persistence and snapshot assembly (§7.2–§7.3, §9.3).

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use async_trait::async_trait;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{MySql, Transaction};
use time::{Date, Duration};

use super::annual_tax::{AnnualTaxRunContext, accrue_financial_income_source};
use super::finance::MySqlFinanceStore;
use super::mysql::{
    CommandIdentitySpec, CommandIdentityState, GameCommandReceiptWrite, inspect_command_identity,
    read_state, write_command_identity, write_game_command_receipt, write_ledger_transaction,
};
use super::types::{
    CloseIsaAccountCommand, CloseIsaAccountReceipt, GameCommandCursor, IsaAccountState,
    OpenTaxAccountCommand, OpenTaxAccountReceipt, PensionAccountState, PensionWithdrawalCommand,
    PensionWithdrawalReceipt, StartPensionCommand, StartPensionReceipt, TaxAccountStore,
    TaxAccountStoreResult,
};
use crate::finance::{
    BondPositionSnapshot, CashProductContractState, CommandCursor, CommandId, FinanceFailureCode,
    FinancialAccount, FinancialAccountType, FinancialIncomeAccrual, FinancialIncomeSource,
    GeneralFinancialIncomePolicy, IrpWithdrawalReason, IsaAccountKind, IsaCloseTaxInput,
    IsaCloseTaxResult, IsaContributionRoomInput, IsaEligibility, IsaEnrollmentInput, IsaPolicy,
    IsaPriorIncomeComposition, IsaPriorTaxYearIncome, IsaTaxTreatment, LedgerAccountCode,
    LedgerPosting, LedgerSource, LedgerSourceKind, LedgerTransactionDraft, PensionCreditIncome,
    PensionCreditInput, PensionPolicy, PensionReceiptLimit, PensionReceiptLimitInput,
    PensionTaxLayers, PensionTaxRate, PensionTaxSource, PensionWithdrawalPlan,
    PensionWithdrawalPlanInput, PensionWithdrawalRequestKind, PensionWithdrawalTreatment,
    ResourceId, RunId, RunPolicyContext, TaxAccountError, TaxAccountPolicy, TaxAccountRules,
    TransferCommand, TransferDirection, anniversary_game_day, create_tax_account_rules_with_policy,
    current_age_years,
};

const COMMAND_KIND_OPEN_ACCOUNT: &str = "openAccount";
const COMMAND_KIND_CLOSE_ISA: &str = "closeIsa";
const COMMAND_KIND_START_PENSION: &str = "startPension";
const COMMAND_KIND_PENSION_WITHDRAWAL: &str = "pensionWithdrawal";
const MAX_FINANCIAL_ACCOUNTS: i64 = 32;
const LOCK_TAX_ACCOUNT_SAVE_SQL: &str = "SELECT id, market_world_id, policy_set_id, run_revision,
            state_revision, game_day, cash_krw
     FROM save
     WHERE user_id = ?
     FOR UPDATE";

#[async_trait]
impl TaxAccountStore for MySqlFinanceStore {
    async fn open_tax_account(
        &self,
        user_id: u64,
        command: &OpenTaxAccountCommand,
    ) -> Result<TaxAccountStoreResult<OpenTaxAccountReceipt>> {
        let fingerprint = open_tax_account_fingerprint(command);
        let mut tx = self.pool.begin().await?;
        let Some(current) = lock_save_for_user(&mut tx, user_id).await? else {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(
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
                return Ok(TaxAccountStoreResult::Rejected(
                    FinanceFailureCode::IdempotencyConflict,
                ));
            }
            CommandIdentityState::Matching => {
                let mut receipt: OpenTaxAccountReceipt = read_receipt(
                    &mut tx,
                    current.id,
                    &command.command_id,
                    COMMAND_KIND_OPEN_ACCOUNT,
                    &fingerprint,
                )
                .await?
                .context("open tax-account identity has no final receipt")?;
                receipt.replayed = true;
                let save = read_state(&mut tx, current.id).await?;
                tx.commit().await?;
                return Ok(TaxAccountStoreResult::Applied {
                    receipt,
                    save: Box::new(save),
                });
            }
            CommandIdentityState::Missing => {}
        }
        if let Some(rejection) = validate_current(&current, command.cursor) {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(rejection));
        }

        let family = match command.account_type {
            FinancialAccountType::IsaGeneral => TaxAccountFamily::Isa(IsaAccountKind::General),
            FinancialAccountType::IsaLowIncome => TaxAccountFamily::Isa(IsaAccountKind::LowIncome),
            FinancialAccountType::PensionSavings | FinancialAccountType::Irp => {
                TaxAccountFamily::Pension
            }
            FinancialAccountType::TaxableBrokerage
            | FinancialAccountType::Cma
            | FinancialAccountType::KrxGold => {
                tx.commit().await?;
                return Ok(TaxAccountStoreResult::Rejected(
                    FinanceFailureCode::AccountTypeNotAllowed,
                ));
            }
        };

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
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::LimitExceeded,
            ));
        }

        let profile = read_tax_profile(&mut tx, current.id, current.run_revision).await?;
        let Some(profile) = profile else {
            bail!("current run is missing its immutable tax profile");
        };
        let tax_rules =
            read_tax_account_rules(&mut tx, current.policy_set_id, current.game_date).await?;
        let has_same_account = has_active_tax_account(
            &mut tx,
            current.id,
            current.run_revision,
            command.account_type,
        )
        .await?;
        match family {
            TaxAccountFamily::Isa(kind) => {
                let start_age = current
                    .character_start_age
                    .context("validated character has no starting age")?;
                let age_years =
                    current_age_years(start_age, current.world_start_date, current.game_date)?;
                let history = profile
                    .isa_records_complete
                    .then_some([profile.had_comprehensive_financial_income_last_three_years; 3]);
                let prior_income = profile
                    .isa_records_complete
                    .then_some(IsaPriorTaxYearIncome {
                        taxable_wage_income_krw: profile.prior_year_employment_income_krw,
                        total_salary_krw: profile.prior_year_total_salary_krw,
                        comprehensive_income_krw: profile.prior_year_comprehensive_income_krw,
                        composition: if profile.prior_year_employment_only {
                            IsaPriorIncomeComposition::WageOnlyOrComprehensiveTaxExcluded
                        } else {
                            IsaPriorIncomeComposition::IncludesOtherComprehensiveIncome
                        },
                    });
                let eligibility = tax_rules.isa_enrollment_eligibility(IsaEnrollmentInput {
                    requested_kind: kind,
                    age_years,
                    prior_tax_year_income: prior_income,
                    previous_three_tax_years_financial_income_taxed: history,
                    has_open_isa: has_same_account,
                })?;
                if let IsaEligibility::Ineligible(reason) = eligibility {
                    tx.commit().await?;
                    let rejection = if matches!(
                        reason,
                        crate::finance::IsaIneligibilityReason::ExistingAccount
                    ) {
                        FinanceFailureCode::AccountAlreadyExists
                    } else {
                        FinanceFailureCode::PolicyNotEligible
                    };
                    return Ok(TaxAccountStoreResult::Rejected(rejection));
                }
            }
            TaxAccountFamily::Pension if has_same_account => {
                tx.commit().await?;
                return Ok(TaxAccountStoreResult::Rejected(
                    FinanceFailureCode::AccountAlreadyExists,
                ));
            }
            TaxAccountFamily::Pension => {}
        }

        let term_years = match family {
            TaxAccountFamily::Isa(_) => tax_rules.isa_minimum_term_years(),
            TaxAccountFamily::Pension => tax_rules.pension_minimum_enrollment_years(),
        };
        let eligibility_game_day =
            anniversary_game_day(current.world_start_date, current.game_date, term_years)?;
        let current_tax_year = tax_year(current.game_date)?;
        write_command_identity(&mut tx, current.id, &identity).await?;
        let account_insert = sqlx::query(
            "INSERT INTO financial_account
                 (save_id, run_revision, account_type, status, cash_krw,
                  is_default, opened_game_day)
             VALUES (?, ?, ?, 'open', 0, FALSE, ?)",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(account_type_str(command.account_type))
        .bind(current.game_day)
        .execute(&mut *tx)
        .await?;
        let account_id = account_insert.last_insert_id();
        ensure!(account_id != 0, "tax-account insert returned a zero ID");

        let event_kind = match family {
            TaxAccountFamily::Isa(_) => {
                sqlx::query(
                    "INSERT INTO isa_account_contract
                         (save_id, run_revision, financial_account_id, account_type,
                          status, opened_game_day, minimum_term_game_day)
                     VALUES (?, ?, ?, ?, 'active', ?, ?)",
                )
                .bind(current.id)
                .bind(current.run_revision)
                .bind(account_id)
                .bind(account_type_str(command.account_type))
                .bind(current.game_day)
                .bind(eligibility_game_day)
                .execute(&mut *tx)
                .await?;
                "isaOpened"
            }
            TaxAccountFamily::Pension => {
                sqlx::query(
                    "INSERT INTO pension_account_contract
                         (save_id, run_revision, financial_account_id, account_type,
                          status, opened_game_day, eligible_pension_start_game_day)
                     VALUES (?, ?, ?, ?, 'active', ?, ?)",
                )
                .bind(current.id)
                .bind(current.run_revision)
                .bind(account_id)
                .bind(account_type_str(command.account_type))
                .bind(current.game_day)
                .bind(eligibility_game_day)
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "INSERT INTO pension_tax_balance
                         (save_id, run_revision, financial_account_id)
                     VALUES (?, ?, ?)",
                )
                .bind(current.id)
                .bind(current.run_revision)
                .bind(account_id)
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "INSERT INTO pension_withdrawal_year
                         (save_id, run_revision, financial_account_id, tax_year,
                          opening_account_value_krw)
                     VALUES (?, ?, ?, ?, 0)",
                )
                .bind(current.id)
                .bind(current.run_revision)
                .bind(account_id)
                .bind(current_tax_year)
                .execute(&mut *tx)
                .await?;
                "pensionOpened"
            }
        };
        write_tax_event(
            &mut tx,
            TaxEventInsert {
                save_id: current.id,
                run_revision: current.run_revision,
                financial_account_id: account_id,
                command_id: &command.command_id,
                event_order: 1,
                event_kind,
                game_day: current.game_day,
                tax_year: current_tax_year,
                movement_amount_krw: 0,
                payload: serde_json::json!({
                    "version": 1,
                    "accountType": account_type_str(command.account_type),
                    "openedGameDay": current.game_day,
                    "eligibilityGameDay": eligibility_game_day,
                }),
                ledger_transaction_id: None,
            },
        )
        .await?;

        let committed_cursor = increment_state_revision(&mut tx, &current).await?;
        let receipt = OpenTaxAccountReceipt {
            command_id: command.command_id.clone(),
            account_id: resource_id(account_id, "financial account")?,
            account_type: command.account_type,
            replayed: false,
        };
        write_receipt(
            &mut tx,
            &current,
            TaxReceiptWrite {
                command_id: &command.command_id,
                command_kind: COMMAND_KIND_OPEN_ACCOUNT,
                payload_sha256: &fingerprint,
                committed_cursor,
                result: &receipt,
                ledger_transaction_id: None,
            },
        )
        .await?;
        let save = read_state(&mut tx, current.id).await?;
        tx.commit().await?;

        Ok(TaxAccountStoreResult::Applied {
            receipt,
            save: Box::new(save),
        })
    }

    async fn close_isa_account(
        &self,
        user_id: u64,
        command: &CloseIsaAccountCommand,
    ) -> Result<TaxAccountStoreResult<CloseIsaAccountReceipt>> {
        let fingerprint = close_isa_fingerprint(command);
        let mut tx = self.pool.begin().await?;
        let Some(current) = lock_save_for_user(&mut tx, user_id).await? else {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::CharacterRequired,
            ));
        };
        let identity = CommandIdentitySpec {
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_CLOSE_ISA,
            payload_sha256: &fingerprint,
            cursor: command.cursor,
        };
        match inspect_command_identity(&mut tx, current.id, &identity).await? {
            CommandIdentityState::Conflict => {
                tx.commit().await?;
                return Ok(TaxAccountStoreResult::Rejected(
                    FinanceFailureCode::IdempotencyConflict,
                ));
            }
            CommandIdentityState::Matching => {
                let mut receipt: CloseIsaAccountReceipt = read_receipt(
                    &mut tx,
                    current.id,
                    &command.command_id,
                    COMMAND_KIND_CLOSE_ISA,
                    &fingerprint,
                )
                .await?
                .context("close-ISA identity has no final receipt")?;
                receipt.replayed = true;
                let save = read_state(&mut tx, current.id).await?;
                tx.commit().await?;
                return Ok(TaxAccountStoreResult::Applied {
                    receipt,
                    save: Box::new(save),
                });
            }
            CommandIdentityState::Missing => {}
        }
        if let Some(rejection) = validate_current(&current, command.cursor) {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(rejection));
        }

        let account: Option<LockedTaxAccountRow> = sqlx::query_as(
            "SELECT id, account_type, status, cash_krw, opened_game_day
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
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::AccountNotFound,
            ));
        };
        let account_kind = match account.account_type.as_str() {
            "isaGeneral" => IsaAccountKind::General,
            "isaLowIncome" => IsaAccountKind::LowIncome,
            _ => {
                tx.commit().await?;
                return Ok(TaxAccountStoreResult::Rejected(
                    FinanceFailureCode::AccountTypeNotAllowed,
                ));
            }
        };
        if account.status != "open" {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::AccountClosed,
            ));
        }
        let contract: Option<LockedIsaContractRow> = sqlx::query_as(
            "SELECT id, account_type, status, opened_game_day,
                    total_contribution_krw, principal_withdrawal_krw,
                    isa_tax_profit_krw, isa_deductible_loss_krw
             FROM isa_account_contract
             WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
             FOR UPDATE",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(account.id)
        .fetch_optional(&mut *tx)
        .await?;
        let contract = contract.context("ISA account is missing its contract")?;
        ensure!(
            contract.account_type == account.account_type
                && contract.opened_game_day == account.opened_game_day,
            "ISA contract disagrees with its financial account"
        );
        if contract.status != "active" {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::AccountClosed,
            ));
        }

        let position_or_product: (bool, bool) = sqlx::query_as(
            "SELECT
                 EXISTS(
                     SELECT 1 FROM asset_position
                     WHERE save_id = ? AND account_id = ? AND quantity > 0
                 ),
                 EXISTS(
                     SELECT 1 FROM cash_product_contract
                     WHERE save_id = ? AND run_revision = ?
                       AND financial_account_id = ? AND status = 'active'
                 )",
        )
        .bind(current.id)
        .bind(account.id)
        .bind(current.id)
        .bind(current.run_revision)
        .bind(account.id)
        .fetch_one(&mut *tx)
        .await?;
        if position_or_product.0 || position_or_product.1 {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::AccountNotEmpty,
            ));
        }

        let opened_on = date_for_game_day(current.world_start_date, contract.opened_game_day)?;
        let tax_rules =
            read_tax_account_rules(&mut tx, current.policy_set_id, current.game_date).await?;
        let tax = tax_rules.isa_close_tax(IsaCloseTaxInput {
            account_kind,
            opened_on,
            closed_on: current.game_date,
            isa_tax_profit_krw: contract.isa_tax_profit_krw,
            isa_deductible_loss_krw: contract.isa_deductible_loss_krw,
            statutory_unavoidable_reason: false,
        })?;
        let total_tax_krw = tax
            .income_tax_krw
            .checked_add(tax.local_income_tax_krw)
            .context("ISA close tax overflowed")?;
        let net_payout_krw = account
            .cash_krw
            .checked_sub(total_tax_krw)
            .context("ISA close payout overflowed")?;
        if net_payout_krw < 0 {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::InsufficientAccountCash,
            ));
        }
        let current_tax_year = tax_year(current.game_date)?;
        let income_delta = isa_financial_income_delta(tax);
        lock_financial_income_year(&mut tx, current.id, current.run_revision, current_tax_year)
            .await?;

        write_command_identity(&mut tx, current.id, &identity).await?;
        let ledger_transaction_id = if account.cash_krw == 0 {
            None
        } else {
            let ledger = self
                .rules
                .create_ledger_transaction(LedgerTransactionDraft {
                    policy: RunPolicyContext {
                        run: RunId {
                            save_id: resource_id(current.id, "save")?,
                            run_revision: current.run_revision,
                        },
                        policy_set_id: resource_id(current.policy_set_id, "policy set")?,
                    },
                    source: LedgerSource {
                        kind: LedgerSourceKind::IsaClose,
                        source_id: command.command_id.to_string(),
                    },
                    game_day: current.game_day,
                    description: "ISA 해지 정산".to_owned(),
                    postings: close_or_withdrawal_postings(
                        command.account_id,
                        account.cash_krw,
                        net_payout_krw,
                        total_tax_krw,
                    )?,
                })?;
            Some(write_ledger_transaction(&mut tx, &ledger).await?)
        };

        let account_update = sqlx::query(
            "UPDATE financial_account SET status = 'closed', cash_krw = 0
             WHERE save_id = ? AND run_revision = ? AND id = ?
               AND status = 'open' AND cash_krw = ?",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(account.id)
        .bind(account.cash_krw)
        .execute(&mut *tx)
        .await?;
        ensure!(
            account_update.rows_affected() == 1,
            "ISA close lost its account lock"
        );
        let contract_update = sqlx::query(
            "UPDATE isa_account_contract
             SET status = 'closed', closed_game_day = ?,
                 closing_movement_amount_krw = ?, closing_ledger_transaction_id = ?
             WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'active'",
        )
        .bind(current.game_day)
        .bind(account.cash_krw)
        .bind(ledger_transaction_id)
        .bind(current.id)
        .bind(current.run_revision)
        .bind(contract.id)
        .execute(&mut *tx)
        .await?;
        ensure!(
            contract_update.rows_affected() == 1,
            "ISA close lost its contract lock"
        );
        apply_financial_income_delta(
            &mut tx,
            AnnualTaxRunContext {
                save_id: current.id,
                run_revision: current.run_revision,
                policy_set_id: current.policy_set_id,
                game_day: current.game_day,
                market_date: current.game_date,
            },
            current_tax_year,
            income_delta,
        )
        .await?;
        let committed_cursor =
            update_wallet_and_revision(&mut tx, &current, net_payout_krw).await?;
        write_tax_event(
            &mut tx,
            TaxEventInsert {
                save_id: current.id,
                run_revision: current.run_revision,
                financial_account_id: account.id,
                command_id: &command.command_id,
                event_order: 1,
                event_kind: "isaClosed",
                game_day: current.game_day,
                tax_year: current_tax_year,
                movement_amount_krw: account.cash_krw,
                payload: serde_json::json!({
                    "version": 1,
                    "treatment": isa_tax_treatment_str(tax.treatment),
                    "grossTaxProfitKrw": tax.gross_tax_profit_krw,
                    "deductibleLossKrw": tax.deductible_loss_krw,
                    "netTaxProfitKrw": tax.net_tax_profit_krw,
                    "exemptProfitKrw": tax.exempt_profit_krw,
                    "taxableProfitKrw": tax.taxable_profit_krw,
                    "incomeTaxKrw": tax.income_tax_krw,
                    "localIncomeTaxKrw": tax.local_income_tax_krw,
                    "netPayoutKrw": net_payout_krw,
                }),
                ledger_transaction_id,
            },
        )
        .await?;
        let receipt = CloseIsaAccountReceipt {
            command_id: command.command_id.clone(),
            account_id: command.account_id,
            gross_tax_profit_krw: tax.gross_tax_profit_krw,
            deductible_loss_krw: tax.deductible_loss_krw,
            income_tax_krw: tax.income_tax_krw,
            local_income_tax_krw: tax.local_income_tax_krw,
            net_payout_krw,
            replayed: false,
        };
        write_receipt(
            &mut tx,
            &current,
            TaxReceiptWrite {
                command_id: &command.command_id,
                command_kind: COMMAND_KIND_CLOSE_ISA,
                payload_sha256: &fingerprint,
                committed_cursor,
                result: &receipt,
                ledger_transaction_id,
            },
        )
        .await?;
        let save = read_state(&mut tx, current.id).await?;
        tx.commit().await?;

        Ok(TaxAccountStoreResult::Applied {
            receipt,
            save: Box::new(save),
        })
    }

    async fn start_pension(
        &self,
        user_id: u64,
        command: &StartPensionCommand,
    ) -> Result<TaxAccountStoreResult<StartPensionReceipt>> {
        let fingerprint = start_pension_fingerprint(command);
        let mut tx = self.pool.begin().await?;
        let Some(current) = lock_save_for_user(&mut tx, user_id).await? else {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::CharacterRequired,
            ));
        };
        let identity = CommandIdentitySpec {
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_START_PENSION,
            payload_sha256: &fingerprint,
            cursor: command.cursor,
        };
        match inspect_command_identity(&mut tx, current.id, &identity).await? {
            CommandIdentityState::Conflict => {
                tx.commit().await?;
                return Ok(TaxAccountStoreResult::Rejected(
                    FinanceFailureCode::IdempotencyConflict,
                ));
            }
            CommandIdentityState::Matching => {
                let mut receipt: StartPensionReceipt = read_receipt(
                    &mut tx,
                    current.id,
                    &command.command_id,
                    COMMAND_KIND_START_PENSION,
                    &fingerprint,
                )
                .await?
                .context("pension-start identity has no final receipt")?;
                receipt.replayed = true;
                let save = read_state(&mut tx, current.id).await?;
                tx.commit().await?;
                return Ok(TaxAccountStoreResult::Applied {
                    receipt,
                    save: Box::new(save),
                });
            }
            CommandIdentityState::Missing => {}
        }
        if let Some(rejection) = validate_current(&current, command.cursor) {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(rejection));
        }
        if !(5..=100).contains(&command.payment_years) {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::InvalidCommand,
            ));
        }

        let (account, contract) = lock_pension_account(
            &mut tx,
            current.id,
            current.run_revision,
            command.account_id,
        )
        .await?;
        let Some(account) = account else {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::AccountNotFound,
            ));
        };
        if !matches!(account.account_type.as_str(), "pensionSavings" | "irp") {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::AccountTypeNotAllowed,
            ));
        }
        let contract = contract.context("pension account is missing its contract")?;
        if account.status != "open" || contract.status != "active" {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::AccountClosed,
            ));
        }
        if contract.pension_started {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::PolicyNotEligible,
            ));
        }
        let tax_rules =
            read_tax_account_rules(&mut tx, current.policy_set_id, current.game_date).await?;
        let start_age = current
            .character_start_age
            .context("validated character has no starting age")?;
        let age_years = current_age_years(start_age, current.world_start_date, current.game_date)?;
        if age_years < tax_rules.minimum_pension_age()
            || current.game_day < contract.eligible_pension_start_game_day
        {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::PolicyNotEligible,
            ));
        }

        let account_total_value_krw = account
            .cash_krw
            .checked_add(
                read_active_product_principal(
                    &mut tx,
                    current.id,
                    current.run_revision,
                    account.id,
                )
                .await?,
            )
            .context("pension-start account value overflowed")?;
        let current_tax_year = tax_year(current.game_date)?;
        let withdrawal_year = lock_pension_withdrawal_year(
            &mut tx,
            current.id,
            current.run_revision,
            account.id,
            current_tax_year,
        )
        .await?
        .context("pension account is missing its current tax-year opening value")?;
        ensure!(
            withdrawal_year.pension_year_number.is_none()
                && withdrawal_year.pension_limit_krw.is_none()
                && withdrawal_year.pension_withdrawn_krw == 0,
            "unstarted pension has pension-receipt terms in its current tax year"
        );
        let tax_balance =
            lock_pension_tax_balance(&mut tx, current.id, current.run_revision, account.id)
                .await?
                .context("pension account is missing its tax-layer balance")?;
        ensure!(
            pension_layer_total(tax_balance.into_layers())? == account_total_value_krw,
            "pension tax layers disagree with account value before pension start"
        );
        let opening_value_krw = withdrawal_year.opening_account_value_krw;
        let annual_limit_krw = calculate_pension_limit(tax_rules.as_ref(), 1, opening_value_krw)?;

        write_command_identity(&mut tx, current.id, &identity).await?;
        let update = sqlx::query(
            "UPDATE pension_account_contract
             SET pension_started = TRUE, pension_start_game_day = ?,
                 pension_start_tax_year = ?, payment_years = ?, lifetime = ?
             WHERE save_id = ? AND run_revision = ? AND id = ?
               AND status = 'active' AND pension_started = FALSE",
        )
        .bind(current.game_day)
        .bind(current_tax_year)
        .bind(command.payment_years)
        .bind(command.lifetime)
        .bind(current.id)
        .bind(current.run_revision)
        .bind(contract.id)
        .execute(&mut *tx)
        .await?;
        ensure!(
            update.rows_affected() == 1,
            "pension start lost its contract lock"
        );
        upsert_pension_withdrawal_year(
            &mut tx,
            PensionWithdrawalYearWrite {
                save_id: current.id,
                run_revision: current.run_revision,
                financial_account_id: account.id,
                tax_year: current_tax_year,
                opening_account_value_krw: opening_value_krw,
                pension_year_number: Some(1),
                pension_limit_krw: annual_limit_krw,
                pension_withdrawn_delta_krw: 0,
                unavoidable_withdrawn_delta_krw: 0,
                non_pension_withdrawn_delta_krw: 0,
                tax_free_withdrawn_delta_krw: 0,
                withheld_tax_delta_krw: 0,
            },
        )
        .await?;
        write_tax_event(
            &mut tx,
            TaxEventInsert {
                save_id: current.id,
                run_revision: current.run_revision,
                financial_account_id: account.id,
                command_id: &command.command_id,
                event_order: 1,
                event_kind: "pensionStarted",
                game_day: current.game_day,
                tax_year: current_tax_year,
                movement_amount_krw: 0,
                payload: serde_json::json!({
                    "version": 1,
                    "startTaxYear": current_tax_year,
                    "paymentYears": command.payment_years,
                    "lifetime": command.lifetime,
                    "openingAccountValueKrw": opening_value_krw,
                    "pensionLimitKrw": annual_limit_krw,
                }),
                ledger_transaction_id: None,
            },
        )
        .await?;
        let committed_cursor = increment_state_revision(&mut tx, &current).await?;
        let receipt = StartPensionReceipt {
            command_id: command.command_id.clone(),
            account_id: command.account_id,
            start_tax_year: current_tax_year,
            payment_years: command.payment_years,
            lifetime: command.lifetime,
            replayed: false,
        };
        write_receipt(
            &mut tx,
            &current,
            TaxReceiptWrite {
                command_id: &command.command_id,
                command_kind: COMMAND_KIND_START_PENSION,
                payload_sha256: &fingerprint,
                committed_cursor,
                result: &receipt,
                ledger_transaction_id: None,
            },
        )
        .await?;
        let save = read_state(&mut tx, current.id).await?;
        tx.commit().await?;

        Ok(TaxAccountStoreResult::Applied {
            receipt,
            save: Box::new(save),
        })
    }

    async fn withdraw_pension(
        &self,
        user_id: u64,
        command: &PensionWithdrawalCommand,
    ) -> Result<TaxAccountStoreResult<PensionWithdrawalReceipt>> {
        let fingerprint = pension_withdrawal_fingerprint(command);
        let mut tx = self.pool.begin().await?;
        let Some(current) = lock_save_for_user(&mut tx, user_id).await? else {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::CharacterRequired,
            ));
        };
        let identity = CommandIdentitySpec {
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_PENSION_WITHDRAWAL,
            payload_sha256: &fingerprint,
            cursor: command.cursor,
        };
        match inspect_command_identity(&mut tx, current.id, &identity).await? {
            CommandIdentityState::Conflict => {
                tx.commit().await?;
                return Ok(TaxAccountStoreResult::Rejected(
                    FinanceFailureCode::IdempotencyConflict,
                ));
            }
            CommandIdentityState::Matching => {
                let mut receipt: PensionWithdrawalReceipt = read_receipt(
                    &mut tx,
                    current.id,
                    &command.command_id,
                    COMMAND_KIND_PENSION_WITHDRAWAL,
                    &fingerprint,
                )
                .await?
                .context("pension-withdrawal identity has no final receipt")?;
                receipt.replayed = true;
                let save = read_state(&mut tx, current.id).await?;
                tx.commit().await?;
                return Ok(TaxAccountStoreResult::Applied {
                    receipt,
                    save: Box::new(save),
                });
            }
            CommandIdentityState::Missing => {}
        }
        if let Some(rejection) = validate_current(&current, command.cursor) {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(rejection));
        }
        if command.amount_krw <= 0
            || (command.kind == PensionWithdrawalRequestKind::RegularPension
                && command.reason.is_some())
        {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::InvalidCommand,
            ));
        }

        let (account, contract) = lock_pension_account(
            &mut tx,
            current.id,
            current.run_revision,
            command.account_id,
        )
        .await?;
        let Some(account) = account else {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::AccountNotFound,
            ));
        };
        if !matches!(account.account_type.as_str(), "pensionSavings" | "irp") {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::AccountTypeNotAllowed,
            ));
        }
        let contract = contract.context("pension account is missing its contract")?;
        if account.status != "open" || contract.status != "active" {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::AccountClosed,
            ));
        }
        if command.amount_krw > account.cash_krw {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::InsufficientAccountCash,
            ));
        }
        let reason_is_proven = false;
        let reason_required = command.kind == PensionWithdrawalRequestKind::StatutoryUnavoidable
            || (account.account_type == "irp"
                && command.kind == PensionWithdrawalRequestKind::ExplicitNonPension);
        if reason_required && (command.reason.is_none() || !reason_is_proven) {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::PolicyNotEligible,
            ));
        }
        if !reason_required && command.reason.is_some() {
            tx.commit().await?;
            return Ok(TaxAccountStoreResult::Rejected(
                FinanceFailureCode::InvalidCommand,
            ));
        }

        let tax_rules =
            read_tax_account_rules(&mut tx, current.policy_set_id, current.game_date).await?;
        let current_tax_year = tax_year(current.game_date)?;
        let account_total_value_krw = account
            .cash_krw
            .checked_add(
                read_active_product_principal(
                    &mut tx,
                    current.id,
                    current.run_revision,
                    account.id,
                )
                .await?,
            )
            .context("pension-withdrawal account value overflowed")?;
        let year = lock_pension_withdrawal_year(
            &mut tx,
            current.id,
            current.run_revision,
            account.id,
            current_tax_year,
        )
        .await?
        .context("pension account is missing its current tax-year opening value")?;
        let layers =
            lock_pension_tax_balance(&mut tx, current.id, current.run_revision, account.id)
                .await?
                .context("pension account is missing its tax-layer balance")?;
        ensure!(
            pension_layer_total(layers.into_layers())? == account_total_value_krw,
            "pension tax layers disagree with account value before withdrawal"
        );
        let opening_value_krw = year.opening_account_value_krw;
        let pension_year_number = contract
            .pension_start_tax_year
            .map(|start_year| {
                current_tax_year
                    .checked_sub(start_year)
                    .and_then(|elapsed| elapsed.checked_add(1))
                    .context("pension receipt year overflowed")
            })
            .transpose()?;
        let pension_limit_krw = pension_year_number
            .map(|year_number| {
                calculate_pension_limit(tax_rules.as_ref(), year_number, opening_value_krw)
            })
            .transpose()?
            .flatten();
        match (pension_year_number, year.pension_year_number) {
            (Some(expected_year), Some(stored_year)) => ensure!(
                stored_year == expected_year && year.pension_limit_krw == pension_limit_krw,
                "stored pension-receipt terms disagree with their contract"
            ),
            (Some(_), None) => ensure!(
                year.pension_limit_krw.is_none() && year.pension_withdrawn_krw == 0,
                "unpinned pension-receipt terms already contain a withdrawal"
            ),
            (None, None) => ensure!(
                year.pension_limit_krw.is_none() && year.pension_withdrawn_krw == 0,
                "pre-start pension year contains pension-receipt state"
            ),
            (None, Some(_)) => bail!("unstarted pension has a pension-receipt year"),
        }
        let existing_pension_withdrawn_krw = year.pension_withdrawn_krw;
        let start_age = current
            .character_start_age
            .context("validated character has no starting age")?;
        let age_years = current_age_years(start_age, current.world_start_date, current.game_date)?;
        let opened_on = date_for_game_day(current.world_start_date, contract.opened_game_day)?;
        let plan = match tax_rules.plan_pension_withdrawal(PensionWithdrawalPlanInput {
            layers: layers.into_layers(),
            requested_amount_krw: command.amount_krw,
            request_kind: command.kind,
            holder_age_years: age_years,
            pension_started: contract.pension_started,
            opened_on,
            current_on: current.game_date,
            pension_receipt_year: pension_year_number.map(u32::from),
            tax_period_opening_value_krw: opening_value_krw,
            pension_withdrawn_before_request_krw: existing_pension_withdrawn_krw,
            lifetime_contract: contract.lifetime.unwrap_or(false),
            deferred_retirement_non_pension_tax_rate_ppm: 0,
        }) {
            Ok(plan) => plan,
            Err(TaxAccountError::PensionReceiptNotEligible) => {
                tx.commit().await?;
                return Ok(TaxAccountStoreResult::Rejected(
                    FinanceFailureCode::PolicyNotEligible,
                ));
            }
            Err(TaxAccountError::WithdrawalExceedsBalance) => {
                tx.commit().await?;
                return Ok(TaxAccountStoreResult::Rejected(
                    FinanceFailureCode::InsufficientAccountCash,
                ));
            }
            Err(error) => return Err(error.into()),
        };
        write_command_identity(&mut tx, current.id, &identity).await?;
        let ledger = self
            .rules
            .create_ledger_transaction(LedgerTransactionDraft {
                policy: RunPolicyContext {
                    run: RunId {
                        save_id: resource_id(current.id, "save")?,
                        run_revision: current.run_revision,
                    },
                    policy_set_id: resource_id(current.policy_set_id, "policy set")?,
                },
                source: LedgerSource {
                    kind: LedgerSourceKind::PensionWithdrawal,
                    source_id: command.command_id.to_string(),
                },
                game_day: current.game_day,
                description: "연금계좌 인출".to_owned(),
                postings: close_or_withdrawal_postings(
                    command.account_id,
                    plan.gross_amount_krw,
                    plan.net_payout_krw,
                    plan.tax_krw,
                )?,
            })?;
        let ledger_transaction_id = write_ledger_transaction(&mut tx, &ledger).await?;
        let account_cash_after = account
            .cash_krw
            .checked_sub(plan.gross_amount_krw)
            .context("pension account cash underflowed")?;
        let account_update = sqlx::query(
            "UPDATE financial_account SET cash_krw = ?
             WHERE save_id = ? AND run_revision = ? AND id = ?
               AND status = 'open' AND cash_krw = ?",
        )
        .bind(account_cash_after)
        .bind(current.id)
        .bind(current.run_revision)
        .bind(account.id)
        .bind(account.cash_krw)
        .execute(&mut *tx)
        .await?;
        ensure!(
            account_update.rows_affected() == 1,
            "pension withdrawal lost its account lock"
        );
        update_pension_tax_balance(
            &mut tx,
            current.id,
            current.run_revision,
            account.id,
            layers,
            plan.remaining_layers,
        )
        .await?;
        let unavoidable_withdrawn_delta_krw =
            if command.kind == PensionWithdrawalRequestKind::StatutoryUnavoidable {
                plan.pension_amount_krw
            } else {
                0
            };
        let ordinary_pension_delta_krw = plan
            .pension_amount_krw
            .checked_sub(unavoidable_withdrawn_delta_krw)
            .context("pension withdrawal bucket underflowed")?;
        upsert_pension_withdrawal_year(
            &mut tx,
            PensionWithdrawalYearWrite {
                save_id: current.id,
                run_revision: current.run_revision,
                financial_account_id: account.id,
                tax_year: current_tax_year,
                opening_account_value_krw: opening_value_krw,
                pension_year_number,
                pension_limit_krw,
                pension_withdrawn_delta_krw: ordinary_pension_delta_krw,
                unavoidable_withdrawn_delta_krw,
                non_pension_withdrawn_delta_krw: plan.non_pension_amount_krw,
                tax_free_withdrawn_delta_krw: plan.tax_free_amount_krw,
                withheld_tax_delta_krw: plan.tax_krw,
            },
        )
        .await?;
        let committed_cursor =
            update_wallet_and_revision(&mut tx, &current, plan.net_payout_krw).await?;
        write_tax_event(
            &mut tx,
            TaxEventInsert {
                save_id: current.id,
                run_revision: current.run_revision,
                financial_account_id: account.id,
                command_id: &command.command_id,
                event_order: 1,
                event_kind: "pensionWithdrawal",
                game_day: current.game_day,
                tax_year: current_tax_year,
                movement_amount_krw: plan.gross_amount_krw,
                payload: pension_withdrawal_payload(&plan, command.kind, command.reason),
                ledger_transaction_id: Some(ledger_transaction_id),
            },
        )
        .await?;
        let receipt = PensionWithdrawalReceipt {
            command_id: command.command_id.clone(),
            account_id: command.account_id,
            gross_amount_krw: plan.gross_amount_krw,
            pension_amount_krw: plan.pension_amount_krw,
            non_pension_amount_krw: plan.non_pension_amount_krw,
            tax_free_amount_krw: plan.tax_free_amount_krw,
            tax_krw: plan.tax_krw,
            net_payout_krw: plan.net_payout_krw,
            replayed: false,
        };
        write_receipt(
            &mut tx,
            &current,
            TaxReceiptWrite {
                command_id: &command.command_id,
                command_kind: COMMAND_KIND_PENSION_WITHDRAWAL,
                payload_sha256: &fingerprint,
                committed_cursor,
                result: &receipt,
                ledger_transaction_id: Some(ledger_transaction_id),
            },
        )
        .await?;
        let save = read_state(&mut tx, current.id).await?;
        tx.commit().await?;

        Ok(TaxAccountStoreResult::Applied {
            receipt,
            save: Box::new(save),
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum TaxAccountFamily {
    Isa(IsaAccountKind),
    Pension,
}

pub(super) async fn ensure_m2_tax_profile(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO run_tax_profile
             (save_id, run_revision, source, isa_records_complete,
              prior_year_employment_income_krw, prior_year_total_salary_krw,
              prior_year_comprehensive_income_krw, prior_year_employment_only,
              had_comprehensive_financial_income_last_three_years)
         SELECT current_save.id, current_save.run_revision,
                'm2Default', TRUE, 0, 0, 0, TRUE, FALSE
         FROM save AS current_save
         WHERE current_save.id = ?
           AND NOT EXISTS (
               SELECT 1 FROM run_tax_profile AS profile
               WHERE profile.save_id = current_save.id
                 AND profile.run_revision = current_save.run_revision
           )",
    )
    .bind(save_id)
    .execute(&mut **tx)
    .await?;
    let profile_exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS(
             SELECT 1 FROM run_tax_profile AS profile
             INNER JOIN save ON save.id = profile.save_id
               AND save.run_revision = profile.run_revision
             WHERE save.id = ?
         )",
    )
    .bind(save_id)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(profile_exists.0, "current run has no tax profile");
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct NewRunTaxAccountRow {
    id: u64,
    account_type: String,
}

enum NewRunTaxContract {
    Isa(u64),
    Pension(u64),
}

pub(super) async fn cancel_tax_accounts_for_new_run(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    market_world_id: u64,
    game_day: u32,
    command_id: &CommandId,
) -> Result<()> {
    let accounts: Vec<NewRunTaxAccountRow> = sqlx::query_as(
        "SELECT id, account_type
         FROM financial_account
         WHERE save_id = ? AND run_revision = ? AND status = 'open'
           AND account_type IN ('isaGeneral', 'isaLowIncome', 'pensionSavings', 'irp')
         ORDER BY id
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    if accounts.is_empty() {
        return Ok(());
    }
    let (_, game_date) = read_transfer_dates(tx, market_world_id, game_day).await?;
    let current_tax_year = tax_year(game_date)?;
    for (index, account) in accounts.iter().enumerate() {
        let event_order = u16::try_from(index + 1).context("too many new-run tax events")?;
        let contract = match account.account_type.as_str() {
            "isaGeneral" | "isaLowIncome" => {
                let row: Option<(u64,)> = sqlx::query_as(
                    "SELECT id FROM isa_account_contract
                     WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
                       AND status = 'active'
                     FOR UPDATE",
                )
                .bind(save_id)
                .bind(run_revision)
                .bind(account.id)
                .fetch_optional(&mut **tx)
                .await?;
                NewRunTaxContract::Isa(
                    row.context("active ISA account is missing its new-run contract")?
                        .0,
                )
            }
            "pensionSavings" | "irp" => {
                let row: Option<(u64,)> = sqlx::query_as(
                    "SELECT id FROM pension_account_contract
                     WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
                       AND status = 'active'
                     FOR UPDATE",
                )
                .bind(save_id)
                .bind(run_revision)
                .bind(account.id)
                .fetch_optional(&mut **tx)
                .await?;
                NewRunTaxContract::Pension(
                    row.context("active pension account is missing its new-run contract")?
                        .0,
                )
            }
            _ => bail!("new-run tax account has an invalid account type"),
        };
        write_tax_event(
            tx,
            TaxEventInsert {
                save_id,
                run_revision,
                financial_account_id: account.id,
                command_id,
                event_order,
                event_kind: "runCancelled",
                game_day,
                tax_year: current_tax_year,
                movement_amount_krw: 0,
                payload: serde_json::json!({
                    "version": 1,
                    "reason": "newRun",
                    "accountType": account.account_type,
                }),
                ledger_transaction_id: None,
            },
        )
        .await?;
        match contract {
            NewRunTaxContract::Isa(contract_id) => {
                let update = sqlx::query(
                    "UPDATE isa_account_contract
                     SET status = 'cancelled', closed_game_day = ?,
                         cancellation_reason = 'newRun'
                     WHERE save_id = ? AND run_revision = ? AND id = ?
                       AND status = 'active'",
                )
                .bind(game_day)
                .bind(save_id)
                .bind(run_revision)
                .bind(contract_id)
                .execute(&mut **tx)
                .await?;
                ensure!(
                    update.rows_affected() == 1,
                    "new run lost its ISA contract lock"
                );
            }
            NewRunTaxContract::Pension(contract_id) => {
                let update = sqlx::query(
                    "UPDATE pension_account_contract
                     SET status = 'cancelled', cancelled_game_day = ?,
                         cancellation_reason = 'newRun'
                     WHERE save_id = ? AND run_revision = ? AND id = ?
                       AND status = 'active'",
                )
                .bind(game_day)
                .bind(save_id)
                .bind(run_revision)
                .bind(contract_id)
                .execute(&mut **tx)
                .await?;
                ensure!(
                    update.rows_affected() == 1,
                    "new run lost its pension contract lock"
                );
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PensionOpeningAccountRow {
    id: u64,
    account_type: String,
    cash_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PensionOpeningContractRow {
    financial_account_id: u64,
    account_type: String,
}

pub(super) async fn pin_pension_opening_values_for_day(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    market_date: Date,
) -> Result<()> {
    if market_date.ordinal() != 1 {
        return Ok(());
    }

    let accounts: Vec<PensionOpeningAccountRow> = sqlx::query_as(
        "SELECT id, account_type, cash_krw
         FROM financial_account
         WHERE save_id = ? AND run_revision = ? AND status = 'open'
           AND account_type IN ('pensionSavings', 'irp')
         ORDER BY id
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    if accounts.is_empty() {
        return Ok(());
    }
    let contracts: Vec<PensionOpeningContractRow> = sqlx::query_as(
        "SELECT financial_account_id, account_type
         FROM pension_account_contract
         WHERE save_id = ? AND run_revision = ? AND status = 'active'
         ORDER BY financial_account_id
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        accounts.len() == contracts.len()
            && accounts.iter().zip(&contracts).all(|(account, contract)| {
                account.id == contract.financial_account_id
                    && account.account_type == contract.account_type
            }),
        "active pension accounts disagree with their contracts at the tax-year boundary"
    );

    let opening_tax_year = tax_year(market_date)?;
    for account in accounts {
        let has_position: (bool,) = sqlx::query_as(
            "SELECT EXISTS(
                 SELECT 1 FROM asset_position
                 WHERE save_id = ? AND account_id = ? AND quantity > 0
             )",
        )
        .bind(save_id)
        .bind(account.id)
        .fetch_one(&mut **tx)
        .await?;
        ensure!(
            !has_position.0,
            "M2-C pension opening value cannot contain a security position"
        );
        let opening_value_krw = account
            .cash_krw
            .checked_add(
                read_active_product_principal(tx, save_id, run_revision, account.id).await?,
            )
            .context("pension opening value overflowed")?;
        let insert = sqlx::query(
            "INSERT INTO pension_withdrawal_year
                 (save_id, run_revision, financial_account_id, tax_year,
                  opening_account_value_krw)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(account.id)
        .bind(opening_tax_year)
        .bind(opening_value_krw)
        .execute(&mut **tx)
        .await?;
        ensure!(
            insert.rows_affected() == 1,
            "pension opening value was not pinned"
        );
        let balance = lock_pension_tax_balance(tx, save_id, run_revision, account.id)
            .await?
            .context("active pension account is missing its tax-layer balance")?;
        ensure!(
            pension_layer_total(balance.into_layers())? == opening_value_krw,
            "pension tax layers disagree with the tax-year opening value"
        );
    }
    Ok(())
}

pub(super) struct TaxAccountStateRead {
    pub isa_accounts: Vec<IsaAccountState>,
    pub pension_accounts: Vec<PensionAccountState>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct TaxStateMarketRow {
    world_start_date: Date,
    game_date: Date,
    benchmark_close_krw: i64,
    llx_close_krw: Option<i64>,
}

fn tax_account_equity_close_krw(
    benchmark_close_krw: i64,
    llx_close_krw: Option<i64>,
) -> Result<i64> {
    let close_krw = llx_close_krw.unwrap_or(benchmark_close_krw);
    ensure!(
        close_krw > 0,
        "tax-account snapshot equity price is invalid"
    );
    Ok(close_krw)
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct IsaStateRow {
    financial_account_id: u64,
    account_type: String,
    opened_game_day: u32,
    minimum_term_game_day: u32,
    total_contribution_krw: i64,
    principal_withdrawal_krw: i64,
    isa_tax_profit_krw: i64,
    isa_deductible_loss_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PensionStateRow {
    financial_account_id: u64,
    account_type: String,
    cash_krw: i64,
    opened_game_day: u32,
    eligible_pension_start_game_day: u32,
    pension_started: bool,
    pension_start_tax_year: Option<u16>,
    tax_excluded_contribution_krw: i64,
    deferred_retirement_income_krw: i64,
    credited_contribution_krw: i64,
    earnings_krw: i64,
    current_year_contribution_krw: Option<i64>,
    current_year_credit_eligible_krw: Option<i64>,
    expected_credit_krw: Option<i64>,
    opening_account_value_krw: Option<i64>,
    pension_year_number: Option<u16>,
    pension_limit_krw: Option<i64>,
    pension_withdrawn_krw: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct TaxPositionRow {
    account_id: u64,
    symbol: String,
    quantity: u32,
}

pub(super) struct TaxAccountStateInput<'a> {
    pub(super) save_id: u64,
    pub(super) market_world_id: u64,
    pub(super) policy_set_id: u64,
    pub(super) run_revision: u32,
    pub(super) game_day: u32,
    pub(super) cash_contracts: &'a [CashProductContractState],
    pub(super) bond_positions: &'a [BondPositionSnapshot],
}

pub(super) async fn read_tax_account_state(
    tx: &mut Transaction<'_, MySql>,
    input: TaxAccountStateInput<'_>,
) -> Result<TaxAccountStateRead> {
    let TaxAccountStateInput {
        save_id,
        market_world_id,
        policy_set_id,
        run_revision,
        game_day,
        cash_contracts,
        bond_positions,
    } = input;
    let market: Option<TaxStateMarketRow> = sqlx::query_as(
        "SELECT world.start_date AS world_start_date,
                COALESCE(daily.market_date, DATE_ADD(world.start_date, INTERVAL ? DAY))
                    AS game_date,
                COALESCE(daily.equity_close_krw, world.day0_equity_close_krw)
                    AS benchmark_close_krw,
                daily.llx_close_krw
         FROM market_world AS world
         LEFT JOIN market_daily AS daily
           ON daily.world_id = world.id AND daily.game_day = ?
         WHERE world.id = ?",
    )
    .bind(game_day)
    .bind(game_day)
    .bind(market_world_id)
    .fetch_optional(&mut **tx)
    .await?;
    let market = market.context("tax-account snapshot market world is missing")?;
    let equity_close_krw =
        tax_account_equity_close_krw(market.benchmark_close_krw, market.llx_close_krw)?;
    let rules = read_tax_account_rules(tx, policy_set_id, market.game_date).await?;

    let isa_rows: Vec<IsaStateRow> = sqlx::query_as(
        "SELECT contract.financial_account_id, contract.account_type,
                contract.opened_game_day, contract.minimum_term_game_day,
                contract.total_contribution_krw, contract.principal_withdrawal_krw,
                contract.isa_tax_profit_krw, contract.isa_deductible_loss_krw
         FROM isa_account_contract AS contract
         INNER JOIN financial_account AS account
           ON account.save_id = contract.save_id
          AND account.run_revision = contract.run_revision
          AND account.id = contract.financial_account_id
         WHERE contract.save_id = ? AND contract.run_revision = ?
           AND contract.status = 'active' AND account.status = 'open'
         ORDER BY contract.financial_account_id
         LIMIT 2",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        isa_rows.len() <= 1,
        "ISA snapshot exceeded its account bound"
    );
    let isa_accounts = isa_rows
        .into_iter()
        .map(|row| {
            let kind = match row.account_type.as_str() {
                "isaGeneral" => IsaAccountKind::General,
                "isaLowIncome" => IsaAccountKind::LowIncome,
                _ => bail!("ISA snapshot has an invalid account type"),
            };
            let opened_on = date_for_game_day(market.world_start_date, row.opened_game_day)?;
            let room = rules.isa_contribution_room(IsaContributionRoomInput {
                opened_on,
                current_on: market.game_date,
                cumulative_contribution_krw: row.total_contribution_krw,
            })?;
            let close = rules.isa_close_tax(IsaCloseTaxInput {
                account_kind: kind,
                opened_on,
                closed_on: market.game_date,
                isa_tax_profit_krw: row.isa_tax_profit_krw,
                isa_deductible_loss_krw: row.isa_deductible_loss_krw,
                statutory_unavoidable_reason: false,
            })?;
            Ok(IsaAccountState {
                account_id: resource_id(row.financial_account_id, "financial account")?,
                account_type: from_db_str(&row.account_type)?,
                opened_game_day: row.opened_game_day,
                minimum_term_game_day: row.minimum_term_game_day,
                total_contribution_krw: row.total_contribution_krw,
                principal_withdrawal_krw: row.principal_withdrawal_krw,
                contribution_capacity_krw: room.available_contribution_krw,
                tax_profit_krw: row.isa_tax_profit_krw,
                deductible_loss_krw: row.isa_deductible_loss_krw,
                expected_close_income_tax_krw: close.income_tax_krw,
                expected_close_local_income_tax_krw: close.local_income_tax_krw,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let current_tax_year = tax_year(market.game_date)?;
    let pension_rows: Vec<PensionStateRow> = sqlx::query_as(
        "SELECT contract.financial_account_id, contract.account_type, account.cash_krw,
                contract.opened_game_day, contract.eligible_pension_start_game_day,
                contract.pension_started, contract.pension_start_tax_year,
                balance.tax_excluded_contribution_krw,
                balance.deferred_retirement_income_krw,
                balance.credited_contribution_krw, balance.earnings_krw,
                contribution.total_contribution_krw AS current_year_contribution_krw,
                contribution.credit_eligible_krw AS current_year_credit_eligible_krw,
                contribution.expected_credit_krw,
                withdrawal.opening_account_value_krw,
                withdrawal.pension_year_number, withdrawal.pension_limit_krw,
                withdrawal.pension_withdrawn_krw
         FROM pension_account_contract AS contract
         INNER JOIN financial_account AS account
           ON account.save_id = contract.save_id
          AND account.run_revision = contract.run_revision
          AND account.id = contract.financial_account_id
         INNER JOIN pension_tax_balance AS balance
           ON balance.save_id = contract.save_id
          AND balance.run_revision = contract.run_revision
          AND balance.financial_account_id = contract.financial_account_id
         LEFT JOIN pension_contribution_year AS contribution
           ON contribution.save_id = contract.save_id
          AND contribution.run_revision = contract.run_revision
          AND contribution.financial_account_id = contract.financial_account_id
          AND contribution.tax_year = ?
         LEFT JOIN pension_withdrawal_year AS withdrawal
           ON withdrawal.save_id = contract.save_id
          AND withdrawal.run_revision = contract.run_revision
          AND withdrawal.financial_account_id = contract.financial_account_id
          AND withdrawal.tax_year = ?
         WHERE contract.save_id = ? AND contract.run_revision = ?
           AND contract.status = 'active' AND account.status = 'open'
         ORDER BY contract.financial_account_id
         LIMIT 3",
    )
    .bind(current_tax_year)
    .bind(current_tax_year)
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        pension_rows.len() <= 2,
        "pension snapshot exceeded its account bound"
    );
    let position_rows: Vec<TaxPositionRow> = sqlx::query_as(
        "SELECT position.account_id, position.symbol, position.quantity
         FROM asset_position AS position
         INNER JOIN financial_account AS account
           ON account.save_id = position.save_id AND account.id = position.account_id
         WHERE position.save_id = ? AND account.run_revision = ?
           AND account.account_type IN ('pensionSavings', 'irp')
         ORDER BY position.account_id, position.symbol",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    let mut risk_by_account = BTreeMap::<u64, i64>::new();
    for position in position_rows {
        ensure!(
            position.symbol == "LLX",
            "pension snapshot has an unsupported position"
        );
        let value = i64::from(position.quantity)
            .checked_mul(equity_close_krw)
            .context("pension position value overflowed")?;
        let entry = risk_by_account.entry(position.account_id).or_default();
        *entry = entry
            .checked_add(value)
            .context("pension risk-asset value overflowed")?;
    }
    let mut safe_asset_by_account = BTreeMap::<u64, i64>::new();
    for contract in cash_contracts {
        if contract.current_principal_krw == 0 {
            continue;
        }
        let entry = safe_asset_by_account
            .entry(contract.settlement_account_id.get())
            .or_default();
        *entry = entry
            .checked_add(contract.current_principal_krw)
            .context("pension cash-product principal overflowed")?;
    }
    for position in bond_positions {
        let entry = safe_asset_by_account
            .entry(position.account_id.get())
            .or_default();
        *entry = entry
            .checked_add(position.market_value_krw)
            .context("pension bond value overflowed")?;
    }
    let pension_accounts = pension_rows
        .into_iter()
        .map(|row| {
            let risk_asset_value_krw = risk_by_account
                .get(&row.financial_account_id)
                .copied()
                .unwrap_or(0);
            let safe_asset_value_krw = safe_asset_by_account
                .get(&row.financial_account_id)
                .copied()
                .unwrap_or(0);
            let total_value_krw =
                pension_total_value_krw(row.cash_krw, risk_asset_value_krw, safe_asset_value_krw)?;
            let tax_layers = PensionTaxLayers {
                tax_excluded_contribution_krw: row.tax_excluded_contribution_krw,
                deferred_retirement_income_krw: row.deferred_retirement_income_krw,
                credited_contribution_krw: row.credited_contribution_krw,
                earnings_krw: row.earnings_krw,
            };
            ensure!(
                pension_layer_total(tax_layers)? == total_value_krw,
                "pension tax layers disagree with account value"
            );
            let risk_asset_ratio_ppm = ratio_ppm(risk_asset_value_krw, total_value_krw)?;
            let contribution_krw = row.current_year_contribution_krw.unwrap_or(0);
            let opening_value_krw = row
                .opening_account_value_krw
                .context("active pension account has no current tax-year opening value")?;
            let pension_withdrawn_krw = row
                .pension_withdrawn_krw
                .context("active pension account has no current tax-year withdrawal summary")?;
            let current_year_pension_limit_krw = if row.pension_started {
                let start_year = row
                    .pension_start_tax_year
                    .context("started pension has no start tax year")?;
                let pension_year = current_tax_year
                    .checked_sub(start_year)
                    .and_then(|elapsed| elapsed.checked_add(1))
                    .context("pension snapshot receipt year overflowed")?;
                let calculated_limit =
                    calculate_pension_limit(rules.as_ref(), pension_year, opening_value_krw)?;
                if let Some(stored_year) = row.pension_year_number {
                    ensure!(
                        stored_year == pension_year && row.pension_limit_krw == calculated_limit,
                        "pension withdrawal year disagrees with its contract or opening value"
                    );
                } else {
                    ensure!(
                        row.pension_limit_krw.is_none() && pension_withdrawn_krw == 0,
                        "unpinned pension-receipt terms already contain a withdrawal"
                    );
                }
                calculated_limit
            } else {
                ensure!(
                    row.pension_year_number.is_none()
                        && row.pension_limit_krw.is_none()
                        && pension_withdrawn_krw == 0,
                    "unstarted pension has pension-receipt state"
                );
                None
            };
            Ok(PensionAccountState {
                account_id: resource_id(row.financial_account_id, "financial account")?,
                account_type: from_db_str(&row.account_type)?,
                opened_game_day: row.opened_game_day,
                eligible_pension_start_game_day: row.eligible_pension_start_game_day,
                pension_started: row.pension_started,
                tax_layers,
                current_year_contribution_krw: contribution_krw,
                current_year_credit_eligible_krw: row.current_year_credit_eligible_krw.unwrap_or(0),
                expected_credit_krw: row.expected_credit_krw.unwrap_or(0),
                current_year_pension_limit_krw,
                current_year_pension_withdrawn_krw: pension_withdrawn_krw,
                risk_asset_value_krw,
                total_value_krw,
                risk_asset_ratio_ppm,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(TaxAccountStateRead {
        isa_accounts,
        pension_accounts,
    })
}

fn pension_layer_total(layers: PensionTaxLayers) -> Result<i64> {
    layers
        .tax_excluded_contribution_krw
        .checked_add(layers.deferred_retirement_income_krw)
        .and_then(|value| value.checked_add(layers.credited_contribution_krw))
        .and_then(|value| value.checked_add(layers.earnings_krw))
        .context("pension tax-layer total overflowed")
}

fn pension_total_value_krw(
    cash_krw: i64,
    risk_asset_value_krw: i64,
    safe_asset_value_krw: i64,
) -> Result<i64> {
    ensure!(
        cash_krw >= 0 && risk_asset_value_krw >= 0 && safe_asset_value_krw >= 0,
        "pension account value components cannot be negative"
    );
    cash_krw
        .checked_add(risk_asset_value_krw)
        .and_then(|value| value.checked_add(safe_asset_value_krw))
        .context("pension account value overflowed")
}

fn ratio_ppm(numerator: i64, denominator: i64) -> Result<i64> {
    ensure!(
        numerator >= 0 && denominator >= 0 && numerator <= denominator,
        "pension risk ratio inputs are invalid"
    );
    if denominator == 0 {
        return Ok(0);
    }
    i64::try_from(
        i128::from(numerator)
            .checked_mul(1_000_000)
            .and_then(|value| value.checked_div(i128::from(denominator)))
            .context("pension risk ratio overflowed")?,
    )
    .context("pension risk ratio is out of range")
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PensionContributionYearRow {
    financial_account_id: u64,
    account_type: String,
    total_contribution_krw: i64,
}

pub(super) struct PensionContributionYearMutation {
    financial_account_id: u64,
    next_total_contribution_krw: i64,
    next_credit_eligible_krw: i64,
    expected_credit_rate_ppm: u32,
    next_expected_credit_krw: i64,
}

async fn read_transfer_dates(
    tx: &mut Transaction<'_, MySql>,
    market_world_id: u64,
    game_day: u32,
) -> Result<(Date, Date)> {
    let row: Option<(Date, Date)> = sqlx::query_as(
        "SELECT world.start_date,
                COALESCE(daily.market_date, DATE_ADD(world.start_date, INTERVAL ? DAY))
                    AS game_date
         FROM market_world AS world
         LEFT JOIN market_daily AS daily
           ON daily.world_id = world.id AND daily.game_day = ?
         WHERE world.id = ?",
    )
    .bind(game_day)
    .bind(game_day)
    .bind(market_world_id)
    .fetch_optional(&mut **tx)
    .await?;
    row.context("tax-account transfer market world is missing")
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct EffectiveTaxPolicyRuleRow {
    domain: String,
    rule_key: String,
    parameters_json: String,
}

async fn read_tax_account_rules(
    tx: &mut Transaction<'_, MySql>,
    policy_set_id: u64,
    game_date: Date,
) -> Result<Arc<dyn TaxAccountRules>> {
    let rows: Vec<EffectiveTaxPolicyRuleRow> = sqlx::query_as(
        "SELECT domain, rule_key, CAST(parameters AS CHAR) AS parameters_json
         FROM policy_rule
         WHERE policy_set_id = ?
           AND (
               (domain = 'isa' AND rule_key = 'eligibilityAndTax')
               OR (domain = 'pension' AND rule_key = 'contributionAndWithdrawal')
               OR (domain = 'tax' AND rule_key = 'generalFinancialIncome')
           )
           AND effective_from <= ?
           AND (effective_to IS NULL OR effective_to >= ?)
         ORDER BY domain, rule_key, effective_from, id
         FOR SHARE",
    )
    .bind(policy_set_id)
    .bind(game_date)
    .bind(game_date)
    .fetch_all(&mut **tx)
    .await?;
    assemble_tax_account_rules(rows)
}

fn assemble_tax_account_rules(
    rows: Vec<EffectiveTaxPolicyRuleRow>,
) -> Result<Arc<dyn TaxAccountRules>> {
    let mut isa = None;
    let mut pension = None;
    let mut general_financial_income = None;
    for row in rows {
        match (row.domain.as_str(), row.rule_key.as_str()) {
            ("isa", "eligibilityAndTax") => {
                ensure!(isa.is_none(), "effective ISA policy is duplicated");
                isa = Some(
                    serde_json::from_str::<IsaPolicy>(&row.parameters_json)
                        .context("effective ISA policy parameters are invalid")?,
                );
            }
            ("pension", "contributionAndWithdrawal") => {
                ensure!(pension.is_none(), "effective pension policy is duplicated");
                pension = Some(
                    serde_json::from_str::<PensionPolicy>(&row.parameters_json)
                        .context("effective pension policy parameters are invalid")?,
                );
            }
            ("tax", "generalFinancialIncome") => {
                ensure!(
                    general_financial_income.is_none(),
                    "effective financial-income policy is duplicated"
                );
                general_financial_income = Some(
                    serde_json::from_str::<GeneralFinancialIncomePolicy>(&row.parameters_json)
                        .context("effective financial-income policy parameters are invalid")?,
                );
            }
            _ => bail!("tax-account policy query returned an unexpected rule"),
        }
    }
    let policy = TaxAccountPolicy {
        isa: isa.context("pinned policy has no effective ISA rule")?,
        pension: pension.context("pinned policy has no effective pension rule")?,
        general_financial_income: general_financial_income
            .context("pinned policy has no effective financial-income rule")?,
    };
    create_tax_account_rules_with_policy(policy).context("effective tax-account policy is invalid")
}

pub(super) async fn read_tax_account_rules_for_game_day(
    tx: &mut Transaction<'_, MySql>,
    policy_set_id: u64,
    market_world_id: u64,
    game_day: u32,
) -> Result<Arc<dyn TaxAccountRules>> {
    let (_, game_date) = read_transfer_dates(tx, market_world_id, game_day).await?;
    read_tax_account_rules(tx, policy_set_id, game_date).await
}

async fn lock_pension_contribution_years(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    tax_year: u16,
) -> Result<Vec<PensionContributionYearRow>> {
    sqlx::query_as(
        "SELECT year.financial_account_id, account.account_type,
                year.total_contribution_krw
         FROM pension_contribution_year AS year
         INNER JOIN financial_account AS account
           ON account.save_id = year.save_id
          AND account.run_revision = year.run_revision
          AND account.id = year.financial_account_id
         WHERE year.save_id = ? AND year.run_revision = ? AND year.tax_year = ?
         ORDER BY year.financial_account_id
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(tax_year)
    .fetch_all(&mut **tx)
    .await
    .context("failed to lock pension contribution years")
}

async fn upsert_pension_contribution_year(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    tax_year: u16,
    mutation: &PensionContributionYearMutation,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO pension_contribution_year
             (save_id, run_revision, financial_account_id, tax_year,
              total_contribution_krw, credit_eligible_krw,
              expected_credit_rate_ppm, expected_credit_krw)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON DUPLICATE KEY UPDATE
             total_contribution_krw = VALUES(total_contribution_krw),
             credit_eligible_krw = VALUES(credit_eligible_krw),
             expected_credit_krw = VALUES(expected_credit_krw)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(mutation.financial_account_id)
    .bind(tax_year)
    .bind(mutation.next_total_contribution_krw)
    .bind(mutation.next_credit_eligible_krw)
    .bind(mutation.expected_credit_rate_ppm)
    .bind(mutation.next_expected_credit_krw)
    .execute(&mut **tx)
    .await?;
    // Recomputing both accounts can leave the non-target row unchanged, which
    // MySQL may report as zero affected rows even though the locked state is valid.
    Ok(())
}

pub(super) enum TaxTransferPlanResult {
    Planned(TaxTransferPlan),
    Rejected(FinanceFailureCode),
}

pub(super) enum TaxTransferPlan {
    None,
    Isa {
        contract_id: u64,
        previous_total_contribution_krw: i64,
        next_total_contribution_krw: i64,
        previous_principal_withdrawal_krw: i64,
        next_principal_withdrawal_krw: i64,
        tax_year: u16,
    },
    Pension {
        contribution_years: Vec<PensionContributionYearMutation>,
        balance_before: PensionTaxBalanceRow,
        balance_after: PensionTaxLayers,
        tax_year: u16,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaxTransferTreatment {
    None,
    Isa,
    PensionContribution,
    RejectedAccountType,
}

fn tax_transfer_treatment(
    account_type: FinancialAccountType,
    direction: TransferDirection,
) -> TaxTransferTreatment {
    match (account_type, direction) {
        (
            FinancialAccountType::TaxableBrokerage
            | FinancialAccountType::Cma
            | FinancialAccountType::KrxGold,
            _,
        ) => TaxTransferTreatment::None,
        (FinancialAccountType::IsaGeneral | FinancialAccountType::IsaLowIncome, _) => {
            TaxTransferTreatment::Isa
        }
        (
            FinancialAccountType::PensionSavings | FinancialAccountType::Irp,
            TransferDirection::WalletToAccount,
        ) => TaxTransferTreatment::PensionContribution,
        (
            FinancialAccountType::PensionSavings | FinancialAccountType::Irp,
            TransferDirection::AccountToWallet,
        ) => TaxTransferTreatment::RejectedAccountType,
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TaxTransferScope {
    pub(super) save_id: u64,
    pub(super) run_revision: u32,
    pub(super) market_world_id: u64,
    pub(super) game_day: u32,
}

pub(super) async fn prepare_tax_transfer(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn TaxAccountRules,
    scope: TaxTransferScope,
    account: &FinancialAccount,
    command: &TransferCommand,
) -> Result<TaxTransferPlanResult> {
    let TaxTransferScope {
        save_id,
        run_revision,
        market_world_id,
        game_day,
    } = scope;
    match tax_transfer_treatment(account.account_type, command.direction) {
        TaxTransferTreatment::None => Ok(TaxTransferPlanResult::Planned(TaxTransferPlan::None)),
        TaxTransferTreatment::Isa => {
            let contract: Option<LockedIsaContractRow> = sqlx::query_as(
                "SELECT id, account_type, status, opened_game_day,
                        total_contribution_krw, principal_withdrawal_krw,
                        isa_tax_profit_krw, isa_deductible_loss_krw
                 FROM isa_account_contract
                 WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
                 FOR UPDATE",
            )
            .bind(save_id)
            .bind(run_revision)
            .bind(account.id.get())
            .fetch_optional(&mut **tx)
            .await?;
            let contract = contract.context("ISA account is missing its contract")?;
            if contract.status != "active" {
                return Ok(TaxTransferPlanResult::Rejected(
                    FinanceFailureCode::AccountClosed,
                ));
            }
            let (world_start_date, game_date) =
                read_transfer_dates(tx, market_world_id, game_day).await?;
            let (next_total_contribution_krw, next_principal_withdrawal_krw) = match command
                .direction
            {
                TransferDirection::WalletToAccount => {
                    let opened_on = date_for_game_day(world_start_date, contract.opened_game_day)?;
                    let room = rules.isa_contribution_room(IsaContributionRoomInput {
                        opened_on,
                        current_on: game_date,
                        cumulative_contribution_krw: contract.total_contribution_krw,
                    })?;
                    if command.amount_krw > room.available_contribution_krw {
                        return Ok(TaxTransferPlanResult::Rejected(
                            FinanceFailureCode::LimitExceeded,
                        ));
                    }
                    (
                        contract
                            .total_contribution_krw
                            .checked_add(command.amount_krw)
                            .context("ISA contribution total overflowed")?,
                        contract.principal_withdrawal_krw,
                    )
                }
                TransferDirection::AccountToWallet => {
                    let principal_remaining = contract
                        .total_contribution_krw
                        .checked_sub(contract.principal_withdrawal_krw)
                        .context("ISA principal summary is inconsistent")?;
                    if command.amount_krw > principal_remaining {
                        return Ok(TaxTransferPlanResult::Rejected(
                            FinanceFailureCode::LimitExceeded,
                        ));
                    }
                    (
                        contract.total_contribution_krw,
                        contract
                            .principal_withdrawal_krw
                            .checked_add(command.amount_krw)
                            .context("ISA principal withdrawal overflowed")?,
                    )
                }
            };
            Ok(TaxTransferPlanResult::Planned(TaxTransferPlan::Isa {
                contract_id: contract.id,
                previous_total_contribution_krw: contract.total_contribution_krw,
                next_total_contribution_krw,
                previous_principal_withdrawal_krw: contract.principal_withdrawal_krw,
                next_principal_withdrawal_krw,
                tax_year: tax_year(game_date)?,
            }))
        }
        TaxTransferTreatment::PensionContribution => {
            let contract: Option<(u64, String)> = sqlx::query_as(
                "SELECT id, status FROM pension_account_contract
                 WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
                 FOR UPDATE",
            )
            .bind(save_id)
            .bind(run_revision)
            .bind(account.id.get())
            .fetch_optional(&mut **tx)
            .await?;
            let Some((_contract_id, status)) = contract else {
                bail!("pension account is missing its contract");
            };
            if status != "active" {
                return Ok(TaxTransferPlanResult::Rejected(
                    FinanceFailureCode::AccountClosed,
                ));
            }
            let (_, game_date) = read_transfer_dates(tx, market_world_id, game_day).await?;
            let current_tax_year = tax_year(game_date)?;
            let profile = read_tax_profile(tx, save_id, run_revision)
                .await?
                .context("pension contribution has no run tax profile")?;
            let mut rows =
                lock_pension_contribution_years(tx, save_id, run_revision, current_tax_year)
                    .await?;
            let mut pension_savings_total = 0_i64;
            let mut irp_total = 0_i64;
            for row in &rows {
                match row.account_type.as_str() {
                    "pensionSavings" => pension_savings_total = row.total_contribution_krw,
                    "irp" => irp_total = row.total_contribution_krw,
                    _ => bail!("pension contribution row has an invalid account type"),
                }
            }
            match account.account_type {
                FinancialAccountType::PensionSavings => {
                    pension_savings_total = pension_savings_total
                        .checked_add(command.amount_krw)
                        .context("pension-savings contribution overflowed")?;
                }
                FinancialAccountType::Irp => {
                    irp_total = irp_total
                        .checked_add(command.amount_krw)
                        .context("IRP contribution overflowed")?;
                }
                _ => unreachable!(),
            }
            let income = if profile.prior_year_employment_only {
                PensionCreditIncome::WageOnly {
                    total_salary_krw: profile.prior_year_total_salary_krw,
                }
            } else {
                PensionCreditIncome::Other {
                    comprehensive_income_krw: profile.prior_year_comprehensive_income_krw,
                }
            };
            let credit = rules.pension_credit(PensionCreditInput {
                pension_savings_contribution_krw: pension_savings_total,
                irp_contribution_krw: irp_total,
                income,
            })?;
            let savings_credit = rules.pension_credit(PensionCreditInput {
                pension_savings_contribution_krw: pension_savings_total,
                irp_contribution_krw: 0,
                income,
            })?;
            let irp_expected_credit_krw = credit
                .expected_credit_krw
                .checked_sub(savings_credit.expected_credit_krw)
                .context("IRP expected credit allocation underflowed")?;
            let expected_rate_ppm = u32::try_from(credit.expected_credit_rate_ppm)
                .context("pension expected-credit rate is out of storage range")?;

            if !rows
                .iter()
                .any(|row| row.financial_account_id == account.id.get())
            {
                rows.push(PensionContributionYearRow {
                    financial_account_id: account.id.get(),
                    account_type: account_type_str(account.account_type).to_owned(),
                    total_contribution_krw: 0,
                });
                rows.sort_by_key(|row| row.financial_account_id);
            }
            let contribution_years = rows
                .into_iter()
                .map(|row| {
                    let next_total = match row.account_type.as_str() {
                        "pensionSavings" => pension_savings_total,
                        "irp" => irp_total,
                        _ => unreachable!(),
                    };
                    let (next_eligible, next_expected) = match row.account_type.as_str() {
                        "pensionSavings" => (
                            credit.pension_savings_eligible_krw,
                            savings_credit.expected_credit_krw,
                        ),
                        "irp" => (credit.irp_eligible_krw, irp_expected_credit_krw),
                        _ => unreachable!(),
                    };
                    PensionContributionYearMutation {
                        financial_account_id: row.financial_account_id,
                        next_total_contribution_krw: next_total,
                        next_credit_eligible_krw: next_eligible,
                        expected_credit_rate_ppm: expected_rate_ppm,
                        next_expected_credit_krw: next_expected,
                    }
                })
                .collect();
            let balance_before =
                lock_pension_tax_balance(tx, save_id, run_revision, account.id.get())
                    .await?
                    .context("pension contribution has no tax-layer balance")?;
            let mut balance_after = balance_before.into_layers();
            balance_after.tax_excluded_contribution_krw = balance_after
                .tax_excluded_contribution_krw
                .checked_add(command.amount_krw)
                .context("pension tax-excluded contribution overflowed")?;
            Ok(TaxTransferPlanResult::Planned(TaxTransferPlan::Pension {
                contribution_years,
                balance_before,
                balance_after,
                tax_year: current_tax_year,
            }))
        }
        TaxTransferTreatment::RejectedAccountType => Ok(TaxTransferPlanResult::Rejected(
            FinanceFailureCode::AccountTypeNotAllowed,
        )),
    }
}

pub(super) async fn apply_tax_transfer(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
    command: &TransferCommand,
    ledger_transaction_id: u64,
    plan: TaxTransferPlan,
) -> Result<()> {
    match plan {
        TaxTransferPlan::None => Ok(()),
        TaxTransferPlan::Isa {
            contract_id,
            previous_total_contribution_krw,
            next_total_contribution_krw,
            previous_principal_withdrawal_krw,
            next_principal_withdrawal_krw,
            tax_year,
        } => {
            let update = sqlx::query(
                "UPDATE isa_account_contract
                 SET total_contribution_krw = ?, principal_withdrawal_krw = ?
                 WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'active'
                   AND total_contribution_krw = ? AND principal_withdrawal_krw = ?",
            )
            .bind(next_total_contribution_krw)
            .bind(next_principal_withdrawal_krw)
            .bind(save_id)
            .bind(run_revision)
            .bind(contract_id)
            .bind(previous_total_contribution_krw)
            .bind(previous_principal_withdrawal_krw)
            .execute(&mut **tx)
            .await?;
            ensure!(
                update.rows_affected() == 1,
                "ISA transfer lost its contract lock"
            );
            let (event_kind, next_total) = match command.direction {
                TransferDirection::WalletToAccount => {
                    ("isaContribution", next_total_contribution_krw)
                }
                TransferDirection::AccountToWallet => {
                    ("isaPrincipalWithdrawal", next_principal_withdrawal_krw)
                }
            };
            write_tax_event(
                tx,
                TaxEventInsert {
                    save_id,
                    run_revision,
                    financial_account_id: command.account_id.get(),
                    command_id: &command.command_id,
                    event_order: 1,
                    event_kind,
                    game_day,
                    tax_year,
                    movement_amount_krw: command.amount_krw,
                    payload: serde_json::json!({
                        "version": 1,
                        "amountKrw": command.amount_krw,
                        "summaryAfterKrw": next_total,
                    }),
                    ledger_transaction_id: Some(ledger_transaction_id),
                },
            )
            .await?;
            Ok(())
        }
        TaxTransferPlan::Pension {
            contribution_years,
            balance_before,
            balance_after,
            tax_year,
        } => {
            ensure!(
                contribution_years
                    .iter()
                    .any(|year| year.financial_account_id == command.account_id.get()),
                "pension contribution plan lost its target account"
            );
            let allocations = pension_contribution_allocations_value(&contribution_years)?;
            for year in &contribution_years {
                upsert_pension_contribution_year(tx, save_id, run_revision, tax_year, year).await?;
            }
            update_pension_tax_balance(
                tx,
                save_id,
                run_revision,
                command.account_id.get(),
                balance_before,
                balance_after,
            )
            .await?;
            write_tax_event(
                tx,
                TaxEventInsert {
                    save_id,
                    run_revision,
                    financial_account_id: command.account_id.get(),
                    command_id: &command.command_id,
                    event_order: 1,
                    event_kind: "pensionContribution",
                    game_day,
                    tax_year,
                    movement_amount_krw: command.amount_krw,
                    payload: serde_json::json!({
                        "version": 1,
                        "amountKrw": command.amount_krw,
                        "allocations": allocations,
                        "taxLayersAfter": pension_layers_value(balance_after),
                    }),
                    ledger_transaction_id: Some(ledger_transaction_id),
                },
            )
            .await?;
            Ok(())
        }
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
    cash_krw: i64,
    character_start_age: Option<u32>,
    world_start_date: Date,
    game_date: Date,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedSaveBaseRow {
    id: u64,
    market_world_id: u64,
    policy_set_id: u64,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
    cash_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedSaveContextRow {
    character_start_age: Option<u32>,
    world_start_date: Date,
    game_date: Date,
}

impl LockedSaveRow {
    const fn has_character(&self) -> bool {
        self.character_start_age.is_some()
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct RunTaxProfileRow {
    isa_records_complete: bool,
    prior_year_employment_income_krw: i64,
    prior_year_total_salary_krw: i64,
    prior_year_comprehensive_income_krw: i64,
    prior_year_employment_only: bool,
    had_comprehensive_financial_income_last_three_years: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedTaxAccountRow {
    id: u64,
    account_type: String,
    status: String,
    cash_krw: i64,
    opened_game_day: u32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedIsaContractRow {
    id: u64,
    account_type: String,
    status: String,
    opened_game_day: u32,
    total_contribution_krw: i64,
    principal_withdrawal_krw: i64,
    isa_tax_profit_krw: i64,
    isa_deductible_loss_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedPensionContractRow {
    id: u64,
    account_type: String,
    status: String,
    opened_game_day: u32,
    eligible_pension_start_game_day: u32,
    pension_started: bool,
    pension_start_tax_year: Option<u16>,
    lifetime: Option<bool>,
}

async fn lock_pension_account(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    account_id: ResourceId,
) -> Result<(
    Option<LockedTaxAccountRow>,
    Option<LockedPensionContractRow>,
)> {
    let account: Option<LockedTaxAccountRow> = sqlx::query_as(
        "SELECT id, account_type, status, cash_krw, opened_game_day
         FROM financial_account
         WHERE save_id = ? AND run_revision = ? AND id = ?
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(account_id.get())
    .fetch_optional(&mut **tx)
    .await?;
    let Some(account) = account else {
        return Ok((None, None));
    };
    if !matches!(account.account_type.as_str(), "pensionSavings" | "irp") {
        return Ok((Some(account), None));
    }
    let contract: Option<LockedPensionContractRow> = sqlx::query_as(
        "SELECT id, account_type, status, opened_game_day,
                eligible_pension_start_game_day, pension_started,
                pension_start_tax_year, lifetime
         FROM pension_account_contract
         WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(account.id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(contract) = &contract {
        ensure!(
            contract.account_type == account.account_type
                && contract.opened_game_day == account.opened_game_day,
            "pension contract disagrees with its financial account"
        );
    }
    Ok((Some(account), contract))
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PensionWithdrawalYearRow {
    opening_account_value_krw: i64,
    pension_year_number: Option<u16>,
    pension_limit_krw: Option<i64>,
    pension_withdrawn_krw: i64,
}

async fn lock_pension_withdrawal_year(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    financial_account_id: u64,
    tax_year: u16,
) -> Result<Option<PensionWithdrawalYearRow>> {
    sqlx::query_as(
        "SELECT opening_account_value_krw, pension_year_number, pension_limit_krw,
                pension_withdrawn_krw
         FROM pension_withdrawal_year
         WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
           AND tax_year = ?
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(financial_account_id)
    .bind(tax_year)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock the pension withdrawal year")
}

struct PensionWithdrawalYearWrite {
    save_id: u64,
    run_revision: u32,
    financial_account_id: u64,
    tax_year: u16,
    opening_account_value_krw: i64,
    pension_year_number: Option<u16>,
    pension_limit_krw: Option<i64>,
    pension_withdrawn_delta_krw: i64,
    unavoidable_withdrawn_delta_krw: i64,
    non_pension_withdrawn_delta_krw: i64,
    tax_free_withdrawn_delta_krw: i64,
    withheld_tax_delta_krw: i64,
}

async fn upsert_pension_withdrawal_year(
    tx: &mut Transaction<'_, MySql>,
    write: PensionWithdrawalYearWrite,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO pension_withdrawal_year
             (save_id, run_revision, financial_account_id, tax_year,
              opening_account_value_krw, pension_year_number, pension_limit_krw,
              pension_withdrawn_krw, unavoidable_withdrawn_krw, non_pension_withdrawn_krw,
              tax_free_withdrawn_krw, withheld_tax_krw)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON DUPLICATE KEY UPDATE
             pension_limit_krw = IF(pension_year_number IS NULL, VALUES(pension_limit_krw), pension_limit_krw),
             pension_year_number = COALESCE(pension_year_number, VALUES(pension_year_number)),
             pension_withdrawn_krw = pension_withdrawn_krw + VALUES(pension_withdrawn_krw),
             unavoidable_withdrawn_krw = unavoidable_withdrawn_krw + VALUES(unavoidable_withdrawn_krw),
             non_pension_withdrawn_krw = non_pension_withdrawn_krw + VALUES(non_pension_withdrawn_krw),
             tax_free_withdrawn_krw = tax_free_withdrawn_krw + VALUES(tax_free_withdrawn_krw),
             withheld_tax_krw = withheld_tax_krw + VALUES(withheld_tax_krw)",
    )
    .bind(write.save_id)
    .bind(write.run_revision)
    .bind(write.financial_account_id)
    .bind(write.tax_year)
    .bind(write.opening_account_value_krw)
    .bind(write.pension_year_number)
    .bind(write.pension_limit_krw)
    .bind(write.pension_withdrawn_delta_krw)
    .bind(write.unavoidable_withdrawn_delta_krw)
    .bind(write.non_pension_withdrawn_delta_krw)
    .bind(write.tax_free_withdrawn_delta_krw)
    .bind(write.withheld_tax_delta_krw)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub(super) struct PensionTaxBalanceRow {
    tax_excluded_contribution_krw: i64,
    deferred_retirement_income_krw: i64,
    credited_contribution_krw: i64,
    earnings_krw: i64,
}

impl PensionTaxBalanceRow {
    const fn into_layers(self) -> PensionTaxLayers {
        PensionTaxLayers {
            tax_excluded_contribution_krw: self.tax_excluded_contribution_krw,
            deferred_retirement_income_krw: self.deferred_retirement_income_krw,
            credited_contribution_krw: self.credited_contribution_krw,
            earnings_krw: self.earnings_krw,
        }
    }
}

async fn lock_pension_tax_balance(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    financial_account_id: u64,
) -> Result<Option<PensionTaxBalanceRow>> {
    sqlx::query_as(
        "SELECT tax_excluded_contribution_krw, deferred_retirement_income_krw,
                credited_contribution_krw, earnings_krw
         FROM pension_tax_balance
         WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(financial_account_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock the pension tax-layer balance")
}

async fn update_pension_tax_balance(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    financial_account_id: u64,
    before: PensionTaxBalanceRow,
    after: PensionTaxLayers,
) -> Result<()> {
    let update = sqlx::query(
        "UPDATE pension_tax_balance
         SET tax_excluded_contribution_krw = ?, deferred_retirement_income_krw = ?,
             credited_contribution_krw = ?, earnings_krw = ?
         WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
           AND tax_excluded_contribution_krw = ?
           AND deferred_retirement_income_krw = ?
           AND credited_contribution_krw = ? AND earnings_krw = ?",
    )
    .bind(after.tax_excluded_contribution_krw)
    .bind(after.deferred_retirement_income_krw)
    .bind(after.credited_contribution_krw)
    .bind(after.earnings_krw)
    .bind(save_id)
    .bind(run_revision)
    .bind(financial_account_id)
    .bind(before.tax_excluded_contribution_krw)
    .bind(before.deferred_retirement_income_krw)
    .bind(before.credited_contribution_krw)
    .bind(before.earnings_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "pension withdrawal lost its tax-balance lock"
    );
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ActiveProductPrincipalRow {
    id: u64,
    contract_kind: String,
    principal_krw: Option<i64>,
}

async fn read_active_product_principal(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    financial_account_id: u64,
) -> Result<i64> {
    let contracts: Vec<ActiveProductPrincipalRow> = sqlx::query_as(
        "SELECT id, contract_kind, principal_krw
         FROM cash_product_contract
         WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
           AND status = 'active'
         ORDER BY id
         FOR SHARE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(financial_account_id)
    .fetch_all(&mut **tx)
    .await?;
    let mut total = 0_i64;
    for contract in contracts {
        let principal = match contract.contract_kind.as_str() {
            "termDeposit" => contract
                .principal_krw
                .context("active term deposit has no principal")?,
            "installmentSavings" => {
                let rows: Vec<(i64,)> = sqlx::query_as(
                    "SELECT amount_krw FROM savings_installment
                     WHERE save_id = ? AND run_revision = ? AND contract_id = ?
                       AND status = 'paid'
                     ORDER BY installment_no
                     FOR SHARE",
                )
                .bind(save_id)
                .bind(run_revision)
                .bind(contract.id)
                .fetch_all(&mut **tx)
                .await?;
                rows.into_iter().try_fold(0_i64, |subtotal, (amount,)| {
                    subtotal
                        .checked_add(amount)
                        .context("savings principal overflowed")
                })?
            }
            _ => bail!("active tax-account cash product has an invalid kind"),
        };
        total = total
            .checked_add(principal)
            .context("tax-account cash-product principal overflowed")?;
    }
    Ok(total)
}

fn pension_withdrawal_payload(
    plan: &PensionWithdrawalPlan,
    request_kind: PensionWithdrawalRequestKind,
    reason: Option<IrpWithdrawalReason>,
) -> Value {
    let portions = plan
        .portions
        .iter()
        .map(|portion| {
            let lines = portion
                .tax_lines
                .iter()
                .map(|line| {
                    serde_json::json!({
                        "source": pension_tax_source_str(line.source),
                        "grossAmountKrw": line.gross_amount_krw,
                        "taxRate": pension_tax_rate_value(line.tax_rate),
                        "taxKrw": line.tax_krw,
                        "netAmountKrw": line.net_amount_krw,
                    })
                })
                .collect::<Vec<_>>();
            serde_json::json!({
                "treatment": pension_treatment_str(portion.treatment),
                "grossAmountKrw": portion.gross_amount_krw,
                "taxFreeAmountKrw": portion.tax_free_amount_krw,
                "taxKrw": portion.tax_krw,
                "netAmountKrw": portion.net_amount_krw,
                "taxLines": lines,
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "version": 1,
        "requestKind": withdrawal_kind_str(request_kind),
        "reason": reason.map(irp_reason_str),
        "grossAmountKrw": plan.gross_amount_krw,
        "pensionAmountKrw": plan.pension_amount_krw,
        "nonPensionAmountKrw": plan.non_pension_amount_krw,
        "taxFreeAmountKrw": plan.tax_free_amount_krw,
        "taxKrw": plan.tax_krw,
        "netPayoutKrw": plan.net_payout_krw,
        "remainingLayers": pension_layers_value(plan.remaining_layers),
        "portions": portions,
    })
}

fn pension_layers_value(layers: PensionTaxLayers) -> Value {
    serde_json::json!({
        "taxExcludedContributionKrw": layers.tax_excluded_contribution_krw,
        "deferredRetirementIncomeKrw": layers.deferred_retirement_income_krw,
        "creditedContributionKrw": layers.credited_contribution_krw,
        "earningsKrw": layers.earnings_krw,
    })
}

fn pension_contribution_allocations_value(
    allocations: &[PensionContributionYearMutation],
) -> Result<Value> {
    ensure!(
        !allocations.is_empty()
            && allocations
                .windows(2)
                .all(|pair| pair[0].financial_account_id < pair[1].financial_account_id),
        "pension contribution allocations are not in account ID order"
    );
    Ok(Value::Array(
        allocations
            .iter()
            .map(|allocation| {
                serde_json::json!({
                    "accountId": allocation.financial_account_id.to_string(),
                    "totalContributionKrw": allocation.next_total_contribution_krw,
                    "creditEligibleKrw": allocation.next_credit_eligible_krw,
                    "expectedCreditRatePpm": allocation.expected_credit_rate_ppm,
                    "expectedCreditKrw": allocation.next_expected_credit_krw,
                })
            })
            .collect(),
    ))
}

fn calculate_pension_limit(
    rules: &dyn TaxAccountRules,
    pension_year_number: u16,
    opening_value_krw: i64,
) -> Result<Option<i64>> {
    match rules.pension_receipt_limit(PensionReceiptLimitInput {
        pension_receipt_year: u32::from(pension_year_number),
        tax_period_opening_value_krw: opening_value_krw,
    })? {
        PensionReceiptLimit::Limited { annual_limit_krw } => Ok(Some(annual_limit_krw)),
        PensionReceiptLimit::Unlimited => Ok(None),
    }
}

const fn pension_treatment_str(treatment: PensionWithdrawalTreatment) -> &'static str {
    match treatment {
        PensionWithdrawalTreatment::Pension => "pension",
        PensionWithdrawalTreatment::PensionUnavoidable => "pensionUnavoidable",
        PensionWithdrawalTreatment::NonPension => "nonPension",
    }
}

const fn pension_tax_source_str(source: PensionTaxSource) -> &'static str {
    match source {
        PensionTaxSource::TaxExcludedContribution => "taxExcludedContribution",
        PensionTaxSource::DeferredRetirementIncome => "deferredRetirementIncome",
        PensionTaxSource::CreditedContribution => "creditedContribution",
        PensionTaxSource::Earnings => "earnings",
    }
}

fn pension_tax_rate_value(rate: PensionTaxRate) -> Value {
    match rate {
        PensionTaxRate::Exempt => serde_json::json!({ "type": "exempt" }),
        PensionTaxRate::FixedPpm(rate_ppm) => {
            serde_json::json!({ "type": "fixedPpm", "ratePpm": rate_ppm })
        }
        PensionTaxRate::DeferredRetirementPension {
            non_pension_rate_ppm,
            pension_factor_ppm,
        } => serde_json::json!({
            "type": "deferredRetirementPension",
            "nonPensionRatePpm": non_pension_rate_ppm,
            "pensionFactorPpm": pension_factor_ppm,
        }),
    }
}

fn date_for_game_day(world_start_date: Date, game_day: u32) -> Result<Date> {
    world_start_date
        .checked_add(Duration::days(i64::from(game_day)))
        .context("tax-account game date is out of range")
}

fn close_or_withdrawal_postings(
    account_id: ResourceId,
    gross_amount_krw: i64,
    net_payout_krw: i64,
    tax_krw: i64,
) -> Result<Vec<LedgerPosting>> {
    ensure!(
        gross_amount_krw > 0
            && net_payout_krw >= 0
            && tax_krw >= 0
            && net_payout_krw.checked_add(tax_krw) == Some(gross_amount_krw),
        "tax-account payout postings do not reconcile"
    );
    let mut postings = vec![LedgerPosting {
        account_code: LedgerAccountCode::AccountCash,
        financial_account_id: Some(account_id),
        amount_krw: gross_amount_krw
            .checked_neg()
            .context("tax-account gross payout cannot be represented")?,
    }];
    if net_payout_krw > 0 {
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::Wallet,
            financial_account_id: None,
            amount_krw: net_payout_krw,
        });
    }
    if tax_krw > 0 {
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::WithholdingTaxLiability,
            financial_account_id: None,
            amount_krw: tax_krw,
        });
    }
    Ok(postings)
}

async fn update_wallet_and_revision(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedSaveRow,
    wallet_increase_krw: i64,
) -> Result<GameCommandCursor> {
    ensure!(
        wallet_increase_krw >= 0,
        "tax-account wallet increase cannot be negative"
    );
    let next_cash_krw = current
        .cash_krw
        .checked_add(wallet_increase_krw)
        .context("wallet cash overflowed during a tax-account payout")?;
    let next_state_revision = current
        .state_revision
        .checked_add(1)
        .context("state revision overflowed during a tax-account payout")?;
    let update = sqlx::query(
        "UPDATE save SET cash_krw = ?, state_revision = ?
         WHERE id = ? AND market_world_id = ? AND policy_set_id = ?
           AND run_revision = ? AND state_revision = ? AND game_day = ?
           AND cash_krw = ?",
    )
    .bind(next_cash_krw)
    .bind(next_state_revision)
    .bind(current.id)
    .bind(current.market_world_id)
    .bind(current.policy_set_id)
    .bind(current.run_revision)
    .bind(current.state_revision)
    .bind(current.game_day)
    .bind(current.cash_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "tax-account payout lost its save lock"
    );
    Ok(GameCommandCursor {
        run_revision: current.run_revision,
        state_revision: next_state_revision,
        game_day: current.game_day,
    })
}

async fn lock_financial_income_year(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    tax_year: u16,
) -> Result<()> {
    let _: Option<(u16,)> = sqlx::query_as(
        "SELECT tax_year FROM financial_income_year
         WHERE save_id = ? AND run_revision = ? AND tax_year = ?
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(tax_year)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(())
}

async fn apply_financial_income_delta(
    tx: &mut Transaction<'_, MySql>,
    context: AnnualTaxRunContext,
    tax_year: u16,
    delta: IsaFinancialIncomeDelta,
) -> Result<()> {
    ensure!(
        delta.gross_financial_income_krw >= 0
            && delta.withheld_income_tax_krw >= 0
            && delta.withheld_local_income_tax_krw >= 0
            && delta.tax_exempt_financial_income_krw >= 0
            && delta.separate_tax_financial_income_krw >= 0
            && delta.separate_withheld_income_tax_krw >= 0
            && delta.separate_withheld_local_income_tax_krw >= 0,
        "ISA financial-income delta cannot be negative"
    );
    if delta == IsaFinancialIncomeDelta::ZERO {
        return Ok(());
    }
    if delta.gross_financial_income_krw != 0
        || delta.withheld_income_tax_krw != 0
        || delta.withheld_local_income_tax_krw != 0
    {
        accrue_financial_income_source(
            tx,
            context,
            FinancialIncomeAccrual {
                source: FinancialIncomeSource::IsaEarlyClose,
                gross_income_krw: delta.gross_financial_income_krw,
                withheld_income_tax_krw: delta.withheld_income_tax_krw,
                withheld_local_income_tax_krw: delta.withheld_local_income_tax_krw,
            },
        )
        .await?;
    }
    if delta.tax_exempt_financial_income_krw == 0
        && delta.separate_tax_financial_income_krw == 0
        && delta.separate_withheld_income_tax_krw == 0
        && delta.separate_withheld_local_income_tax_krw == 0
    {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO financial_income_year
             (save_id, run_revision, tax_year,
              gross_financial_income_krw, withheld_income_tax_krw,
              withheld_local_income_tax_krw, tax_exempt_financial_income_krw,
              separate_tax_financial_income_krw, separate_withheld_income_tax_krw,
              separate_withheld_local_income_tax_krw)
         VALUES (?, ?, ?, 0, 0, 0, ?, ?, ?, ?)
         ON DUPLICATE KEY UPDATE
             tax_exempt_financial_income_krw = tax_exempt_financial_income_krw + VALUES(tax_exempt_financial_income_krw),
             separate_tax_financial_income_krw = separate_tax_financial_income_krw + VALUES(separate_tax_financial_income_krw),
             separate_withheld_income_tax_krw = separate_withheld_income_tax_krw + VALUES(separate_withheld_income_tax_krw),
             separate_withheld_local_income_tax_krw = separate_withheld_local_income_tax_krw + VALUES(separate_withheld_local_income_tax_krw)",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(tax_year)
    .bind(delta.tax_exempt_financial_income_krw)
    .bind(delta.separate_tax_financial_income_krw)
    .bind(delta.separate_withheld_income_tax_krw)
    .bind(delta.separate_withheld_local_income_tax_krw)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IsaFinancialIncomeDelta {
    gross_financial_income_krw: i64,
    withheld_income_tax_krw: i64,
    withheld_local_income_tax_krw: i64,
    tax_exempt_financial_income_krw: i64,
    separate_tax_financial_income_krw: i64,
    separate_withheld_income_tax_krw: i64,
    separate_withheld_local_income_tax_krw: i64,
}

impl IsaFinancialIncomeDelta {
    const ZERO: Self = Self {
        gross_financial_income_krw: 0,
        withheld_income_tax_krw: 0,
        withheld_local_income_tax_krw: 0,
        tax_exempt_financial_income_krw: 0,
        separate_tax_financial_income_krw: 0,
        separate_withheld_income_tax_krw: 0,
        separate_withheld_local_income_tax_krw: 0,
    };
}

const fn isa_financial_income_delta(tax: IsaCloseTaxResult) -> IsaFinancialIncomeDelta {
    match tax.treatment {
        IsaTaxTreatment::GeneralTaxation => IsaFinancialIncomeDelta {
            gross_financial_income_krw: tax.gross_financial_income_delta_krw,
            withheld_income_tax_krw: tax.income_tax_krw,
            withheld_local_income_tax_krw: tax.local_income_tax_krw,
            tax_exempt_financial_income_krw: 0,
            separate_tax_financial_income_krw: 0,
            separate_withheld_income_tax_krw: 0,
            separate_withheld_local_income_tax_krw: 0,
        },
        IsaTaxTreatment::IsaSeparateTaxation => IsaFinancialIncomeDelta {
            gross_financial_income_krw: 0,
            withheld_income_tax_krw: 0,
            withheld_local_income_tax_krw: 0,
            tax_exempt_financial_income_krw: tax.exempt_profit_krw,
            separate_tax_financial_income_krw: tax.taxable_profit_krw,
            separate_withheld_income_tax_krw: tax.income_tax_krw,
            separate_withheld_local_income_tax_krw: tax.local_income_tax_krw,
        },
    }
}

async fn read_tax_profile(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<Option<RunTaxProfileRow>> {
    sqlx::query_as(
        "SELECT isa_records_complete, prior_year_employment_income_krw,
                prior_year_total_salary_krw, prior_year_comprehensive_income_krw,
                prior_year_employment_only,
                had_comprehensive_financial_income_last_three_years
         FROM run_tax_profile
         WHERE save_id = ? AND run_revision = ?
         FOR SHARE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read the immutable run tax profile")
}

async fn has_active_tax_account(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    account_type: FinancialAccountType,
) -> Result<bool> {
    let row: (bool,) = match account_type {
        FinancialAccountType::IsaGeneral | FinancialAccountType::IsaLowIncome => {
            sqlx::query_as(
                "SELECT EXISTS(
                     SELECT 1 FROM isa_account_contract
                     WHERE save_id = ? AND run_revision = ? AND status = 'active'
                 )",
            )
            .bind(save_id)
            .bind(run_revision)
            .fetch_one(&mut **tx)
            .await?
        }
        FinancialAccountType::PensionSavings | FinancialAccountType::Irp => {
            sqlx::query_as(
                "SELECT EXISTS(
                     SELECT 1 FROM pension_account_contract
                     WHERE save_id = ? AND run_revision = ? AND status = 'active'
                       AND account_type = ?
                 )",
            )
            .bind(save_id)
            .bind(run_revision)
            .bind(account_type_str(account_type))
            .fetch_one(&mut **tx)
            .await?
        }
        FinancialAccountType::TaxableBrokerage
        | FinancialAccountType::Cma
        | FinancialAccountType::KrxGold => return Ok(false),
    };
    Ok(row.0)
}

struct TaxEventInsert<'a> {
    save_id: u64,
    run_revision: u32,
    financial_account_id: u64,
    command_id: &'a CommandId,
    event_order: u16,
    event_kind: &'a str,
    game_day: u32,
    tax_year: u16,
    movement_amount_krw: i64,
    payload: Value,
    ledger_transaction_id: Option<u64>,
}

async fn write_tax_event(
    tx: &mut Transaction<'_, MySql>,
    event: TaxEventInsert<'_>,
) -> Result<u64> {
    ensure!(
        event.event_order > 0 && event.movement_amount_krw >= 0,
        "tax-account event invariants are invalid"
    );
    ensure!(
        event.payload.is_object(),
        "tax-account event payload must be an object"
    );
    let insert = sqlx::query(
        "INSERT INTO tax_account_event
             (save_id, run_revision, financial_account_id, command_id,
              event_order, event_kind, event_schema_version, game_day, tax_year,
              movement_amount_krw, payload, ledger_transaction_id)
         VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?)",
    )
    .bind(event.save_id)
    .bind(event.run_revision)
    .bind(event.financial_account_id)
    .bind(event.command_id.as_str())
    .bind(event.event_order)
    .bind(event.event_kind)
    .bind(event.game_day)
    .bind(event.tax_year)
    .bind(event.movement_amount_krw)
    .bind(serde_json::to_string(&event.payload)?)
    .bind(event.ledger_transaction_id)
    .execute(&mut **tx)
    .await?;
    let event_id = insert.last_insert_id();
    ensure!(event_id != 0, "tax-account event insert returned a zero ID");
    Ok(event_id)
}

fn tax_year(date: Date) -> Result<u16> {
    u16::try_from(date.year()).context("tax-account market date has an invalid tax year")
}

async fn lock_save_for_user(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
) -> Result<Option<LockedSaveRow>> {
    let base: Option<LockedSaveBaseRow> = sqlx::query_as(LOCK_TAX_ACCOUNT_SAVE_SQL)
        .bind(user_id)
        .fetch_optional(&mut **tx)
        .await
        .context("failed to lock the authenticated save for a tax-account command")?;
    let Some(base) = base else {
        return Ok(None);
    };
    let context: Option<LockedSaveContextRow> = sqlx::query_as(
        "SELECT `character`.age AS character_start_age,
                world.start_date AS world_start_date,
                COALESCE(daily.market_date, DATE_ADD(world.start_date, INTERVAL ? DAY))
                    AS game_date
         FROM market_world AS world
         LEFT JOIN market_daily AS daily
           ON daily.world_id = world.id AND daily.game_day = ?
         LEFT JOIN `character` ON `character`.save_id = ?
         WHERE world.id = ?",
    )
    .bind(base.game_day)
    .bind(base.game_day)
    .bind(base.id)
    .bind(base.market_world_id)
    .fetch_optional(&mut **tx)
    .await?;
    let context = context.context("tax-account save has no market world")?;
    Ok(Some(LockedSaveRow {
        id: base.id,
        market_world_id: base.market_world_id,
        policy_set_id: base.policy_set_id,
        run_revision: base.run_revision,
        state_revision: base.state_revision,
        game_day: base.game_day,
        cash_krw: base.cash_krw,
        character_start_age: context.character_start_age,
        world_start_date: context.world_start_date,
        game_date: context.game_date,
    }))
}

fn validate_current(current: &LockedSaveRow, cursor: CommandCursor) -> Option<FinanceFailureCode> {
    if !current.has_character() {
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

async fn increment_state_revision(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedSaveRow,
) -> Result<GameCommandCursor> {
    let committed_state_revision = current
        .state_revision
        .checked_add(1)
        .context("state revision overflowed during a tax-account command")?;
    let update = sqlx::query(
        "UPDATE save SET state_revision = ?
         WHERE id = ? AND market_world_id = ? AND policy_set_id = ?
           AND run_revision = ? AND state_revision = ? AND game_day = ?",
    )
    .bind(committed_state_revision)
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
        "tax-account command lost its save lock"
    );
    Ok(GameCommandCursor {
        run_revision: current.run_revision,
        state_revision: committed_state_revision,
        game_day: current.game_day,
    })
}

#[derive(sqlx::FromRow)]
struct CommandReceiptRow {
    command_kind: String,
    payload_sha256: String,
    result_json: String,
}

async fn read_receipt<T: DeserializeOwned>(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    command_id: &CommandId,
    command_kind: &str,
    payload_sha256: &str,
) -> Result<Option<T>> {
    let row: Option<CommandReceiptRow> = sqlx::query_as(
        "SELECT command_kind, payload_sha256, CAST(result AS CHAR) AS result_json
         FROM command_receipt
         WHERE save_id = ? AND command_id = ?
         FOR SHARE",
    )
    .bind(save_id)
    .bind(command_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;
    row.map(|row| {
        ensure!(
            row.command_kind == command_kind && row.payload_sha256 == payload_sha256,
            "tax-account receipt disagrees with its command identity"
        );
        serde_json::from_str(&row.result_json)
            .context("failed to decode the stored tax-account receipt")
    })
    .transpose()
}

struct TaxReceiptWrite<'a, T> {
    command_id: &'a CommandId,
    command_kind: &'static str,
    payload_sha256: &'a str,
    committed_cursor: GameCommandCursor,
    result: &'a T,
    ledger_transaction_id: Option<u64>,
}

async fn write_receipt<T: Serialize>(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedSaveRow,
    write: TaxReceiptWrite<'_, T>,
) -> Result<()> {
    write_game_command_receipt(
        tx,
        GameCommandReceiptWrite {
            save_id: current.id,
            command_id: write.command_id,
            command_kind: write.command_kind,
            payload_sha256: write.payload_sha256,
            market_world_id: current.market_world_id,
            committed_cursor: write.committed_cursor,
            result: write.result,
            ledger_transaction_id: write.ledger_transaction_id,
        },
    )
    .await
}

fn open_tax_account_fingerprint(command: &OpenTaxAccountCommand) -> String {
    let canonical = format!(
        "lifeledger.finance.openTaxAccount.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\ntype={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        account_type_str(command.account_type),
    );
    fingerprint(&canonical)
}

fn close_isa_fingerprint(command: &CloseIsaAccountCommand) -> String {
    let canonical = format!(
        "lifeledger.finance.closeIsa.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\naccountId={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.account_id,
    );
    fingerprint(&canonical)
}

fn start_pension_fingerprint(command: &StartPensionCommand) -> String {
    let canonical = format!(
        "lifeledger.finance.startPension.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\naccountId={}\npaymentYears={}\nlifetime={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.account_id,
        command.payment_years,
        command.lifetime,
    );
    fingerprint(&canonical)
}

fn pension_withdrawal_fingerprint(command: &PensionWithdrawalCommand) -> String {
    let reason = command.reason.map(irp_reason_str).unwrap_or("null");
    let canonical = format!(
        "lifeledger.finance.pensionWithdrawal.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\naccountId={}\namountKrw={}\ntype={}\nreason={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.account_id,
        command.amount_krw,
        withdrawal_kind_str(command.kind),
        reason,
    );
    fingerprint(&canonical)
}

const fn account_type_str(account_type: FinancialAccountType) -> &'static str {
    match account_type {
        FinancialAccountType::TaxableBrokerage => "taxableBrokerage",
        FinancialAccountType::Cma => "cma",
        FinancialAccountType::IsaGeneral => "isaGeneral",
        FinancialAccountType::IsaLowIncome => "isaLowIncome",
        FinancialAccountType::PensionSavings => "pensionSavings",
        FinancialAccountType::Irp => "irp",
        FinancialAccountType::KrxGold => "krxGold",
    }
}

const fn withdrawal_kind_str(kind: PensionWithdrawalRequestKind) -> &'static str {
    match kind {
        PensionWithdrawalRequestKind::RegularPension => "pension",
        PensionWithdrawalRequestKind::ExplicitNonPension => "nonPension",
        PensionWithdrawalRequestKind::StatutoryUnavoidable => "unavoidable",
    }
}

const fn isa_tax_treatment_str(treatment: IsaTaxTreatment) -> &'static str {
    match treatment {
        IsaTaxTreatment::GeneralTaxation => "generalTaxation",
        IsaTaxTreatment::IsaSeparateTaxation => "isaSeparateTaxation",
    }
}

const fn irp_reason_str(reason: IrpWithdrawalReason) -> &'static str {
    match reason {
        IrpWithdrawalReason::HomePurchase => "homePurchase",
        IrpWithdrawalReason::HousingDeposit => "housingDeposit",
        IrpWithdrawalReason::MedicalCare => "medicalCare",
        IrpWithdrawalReason::Disaster => "disaster",
        IrpWithdrawalReason::Bankruptcy => "bankruptcy",
        IrpWithdrawalReason::Rehabilitation => "rehabilitation",
        IrpWithdrawalReason::SecuredLoanRepayment => "securedLoanRepayment",
    }
}

fn fingerprint(canonical: &str) -> String {
    Sha256::digest(canonical.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn resource_id(value: u64, kind: &str) -> Result<ResourceId> {
    ResourceId::parse(&value.to_string()).with_context(|| format!("stored {kind} ID is invalid"))
}

fn from_db_str<T: DeserializeOwned>(raw: &str) -> Result<T> {
    serde_json::from_value(Value::String(raw.to_owned()))
        .with_context(|| format!("unknown tax-account value stored: {raw}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finance::{CommandCursor, CommandId, create_tax_account_rules};

    fn given_cursor() -> CommandCursor {
        CommandCursor {
            expected_run_revision: 3,
            expected_state_revision: 42,
            expected_game_day: 17,
        }
    }

    fn given_command_id() -> CommandId {
        CommandId::parse("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2").expect("표준 UUID여야 한다")
    }

    fn given_account_id() -> ResourceId {
        ResourceId::from_u64(9)
    }

    fn given_policy_rows() -> Vec<EffectiveTaxPolicyRuleRow> {
        vec![
            EffectiveTaxPolicyRuleRow {
                domain: "isa".to_owned(),
                rule_key: "eligibilityAndTax".to_owned(),
                parameters_json: serde_json::to_string(&IsaPolicy::default())
                    .expect("ISA 정책을 직렬화해야 한다"),
            },
            EffectiveTaxPolicyRuleRow {
                domain: "pension".to_owned(),
                rule_key: "contributionAndWithdrawal".to_owned(),
                parameters_json: serde_json::to_string(&PensionPolicy::default())
                    .expect("연금 정책을 직렬화해야 한다"),
            },
            EffectiveTaxPolicyRuleRow {
                domain: "tax".to_owned(),
                rule_key: "generalFinancialIncome".to_owned(),
                parameters_json: serde_json::to_string(&GeneralFinancialIncomePolicy::default())
                    .expect("금융소득 정책을 직렬화해야 한다"),
            },
        ]
    }

    mod context_금현물계좌의_현금이체를_계획할_때 {
        use super::*;

        #[test]
        fn given_지갑에서_금계좌로_입금_when_세제부수상태를_계획하면_then_추가변경없이_허용한다() {
            let direction = TransferDirection::WalletToAccount;

            let treatment = tax_transfer_treatment(FinancialAccountType::KrxGold, direction);

            assert_eq!(treatment, TaxTransferTreatment::None);
        }

        #[test]
        fn given_금계좌에서_지갑으로_출금_when_세제부수상태를_계획하면_then_추가변경없이_허용한다()
        {
            let direction = TransferDirection::AccountToWallet;

            let treatment = tax_transfer_treatment(FinancialAccountType::KrxGold, direction);

            assert_eq!(treatment, TaxTransferTreatment::None);
        }
    }

    mod context_tax_account_commands_are_fingerprinted {
        use super::*;

        #[test]
        fn given_the_same_open_command_when_hashed_then_the_fingerprint_is_stable() {
            let command = OpenTaxAccountCommand {
                command_id: given_command_id(),
                cursor: given_cursor(),
                account_type: FinancialAccountType::IsaGeneral,
            };

            let first = open_tax_account_fingerprint(&command);
            let second = open_tax_account_fingerprint(&command);

            assert_eq!(first, second);
            assert_eq!(first.len(), 64);
        }

        #[test]
        fn given_a_changed_withdrawal_reason_when_hashed_then_the_fingerprint_changes() {
            let command = PensionWithdrawalCommand {
                command_id: given_command_id(),
                cursor: given_cursor(),
                account_id: ResourceId::from_u64(9),
                amount_krw: 1_000_000,
                kind: PensionWithdrawalRequestKind::ExplicitNonPension,
                reason: Some(IrpWithdrawalReason::HomePurchase),
            };
            let mut changed = command.clone();
            changed.reason = Some(IrpWithdrawalReason::MedicalCare);

            let original = pension_withdrawal_fingerprint(&command);
            let changed = pension_withdrawal_fingerprint(&changed);

            assert_ne!(original, changed);
        }
    }

    mod context_effective_tax_account_policy_is_assembled {
        use super::*;

        #[test]
        fn given_all_three_rules_when_assembled_then_the_rules_are_available() {
            let rows = given_policy_rows();

            let result = assemble_tax_account_rules(rows);

            assert!(result.is_ok());
        }

        #[test]
        fn given_a_missing_rule_when_assembled_then_the_policy_is_rejected() {
            let mut rows = given_policy_rows();
            rows.retain(|row| row.domain != "pension");

            let result = assemble_tax_account_rules(rows);

            assert!(result.is_err());
        }

        #[test]
        fn given_two_effective_isa_rules_when_assembled_then_the_policy_is_rejected() {
            let mut rows = given_policy_rows();
            rows.push(rows[0].clone());

            let result = assemble_tax_account_rules(rows);

            assert!(result.is_err());
        }
    }

    mod context_authenticated_save_is_locked {
        use super::*;

        #[test]
        fn given_a_tax_command_when_the_save_is_locked_then_shared_market_tables_are_not_joined() {
            let normalized = LOCK_TAX_ACCOUNT_SAVE_SQL.to_ascii_uppercase();

            let result = normalized.contains(" JOIN ");

            assert!(!result);
        }
    }

    mod context_연금_인출_요청을_감사할_때 {
        use super::*;

        fn given_연간_연금수령한도를_소진한_인출(
            request_kind: PensionWithdrawalRequestKind,
        ) -> PensionWithdrawalPlan {
            create_tax_account_rules()
                .plan_pension_withdrawal(PensionWithdrawalPlanInput {
                    layers: PensionTaxLayers {
                        tax_excluded_contribution_krw: 0,
                        deferred_retirement_income_krw: 0,
                        credited_contribution_krw: 0,
                        earnings_krw: 1_000_000,
                    },
                    requested_amount_krw: 1_000_000,
                    request_kind,
                    holder_age_years: 60,
                    pension_started: true,
                    opened_on: Date::from_ordinal_date(2020, 1).expect("가입일을 생성해야 한다"),
                    current_on: Date::from_ordinal_date(2026, 1).expect("현재일을 생성해야 한다"),
                    pension_receipt_year: Some(1),
                    tax_period_opening_value_krw: 0,
                    pension_withdrawn_before_request_krw: 0,
                    lifetime_contract: false,
                    deferred_retirement_non_pension_tax_rate_ppm: 300_000,
                })
                .expect("한도 초과분을 연금외수령으로 계산해야 한다")
        }

        #[test]
        fn given_동일한_연금외수령결과_when_원요청이_다르면_then_요청종류를_구분해_기록한다() {
            let regular_plan = given_연간_연금수령한도를_소진한_인출(
                PensionWithdrawalRequestKind::RegularPension,
            );
            let explicit_plan = given_연간_연금수령한도를_소진한_인출(
                PensionWithdrawalRequestKind::ExplicitNonPension,
            );
            assert_eq!(regular_plan, explicit_plan);

            let regular_payload = pension_withdrawal_payload(
                &regular_plan,
                PensionWithdrawalRequestKind::RegularPension,
                None,
            );
            let explicit_payload = pension_withdrawal_payload(
                &explicit_plan,
                PensionWithdrawalRequestKind::ExplicitNonPension,
                None,
            );

            assert_eq!(regular_payload["requestKind"], "pension");
            assert_eq!(explicit_payload["requestKind"], "nonPension");
            assert!(regular_payload["reason"].is_null());
            assert!(explicit_payload["reason"].is_null());
            assert_ne!(regular_payload, explicit_payload);
        }
    }

    mod context_isa_해지소득을_연간세금누계에_반영할_때 {
        use super::*;

        fn given_isa_해지세금(opened_year: i32, closed_year: i32) -> IsaCloseTaxResult {
            create_tax_account_rules()
                .isa_close_tax(IsaCloseTaxInput {
                    account_kind: IsaAccountKind::General,
                    opened_on: Date::from_ordinal_date(opened_year, 1)
                        .expect("가입일을 생성해야 한다"),
                    closed_on: Date::from_ordinal_date(closed_year, 1)
                        .expect("해지일을 생성해야 한다"),
                    isa_tax_profit_krw: 10_000_000,
                    isa_deductible_loss_krw: 1_000_000,
                    statutory_unavoidable_reason: false,
                })
                .expect("ISA 해지세금을 계산해야 한다")
        }

        #[test]
        fn given_정상해지_when_누계를_분류하면_then_비과세와_분리과세버킷만_증가한다() {
            let tax = given_isa_해지세금(2020, 2023);

            let delta = isa_financial_income_delta(tax);

            assert_eq!(
                delta,
                IsaFinancialIncomeDelta {
                    gross_financial_income_krw: 0,
                    withheld_income_tax_krw: 0,
                    withheld_local_income_tax_krw: 0,
                    tax_exempt_financial_income_krw: 2_000_000,
                    separate_tax_financial_income_krw: 7_000_000,
                    separate_withheld_income_tax_krw: 630_000,
                    separate_withheld_local_income_tax_krw: 63_000,
                }
            );
        }

        #[test]
        fn given_의무기간전_일반해지_when_누계를_분류하면_then_일반금융소득버킷만_증가한다() {
            let tax = given_isa_해지세금(2020, 2022);

            let delta = isa_financial_income_delta(tax);

            assert_eq!(
                delta,
                IsaFinancialIncomeDelta {
                    gross_financial_income_krw: 10_000_000,
                    withheld_income_tax_krw: 1_400_000,
                    withheld_local_income_tax_krw: 140_000,
                    tax_exempt_financial_income_krw: 0,
                    separate_tax_financial_income_krw: 0,
                    separate_withheld_income_tax_krw: 0,
                    separate_withheld_local_income_tax_krw: 0,
                }
            );
        }
    }

    mod context_연간금융소득_마이그레이션을_검증할_때 {
        const MIGRATION: &str = include_str!("../../migrations/0012_tax_advantaged_accounts.sql");

        #[test]
        fn given_분리버킷열_when_제약을_확인하면_then_0이상인_누계로_생성된다() {
            let columns = [
                "tax_exempt_financial_income_krw",
                "separate_tax_financial_income_krw",
                "separate_withheld_income_tax_krw",
                "separate_withheld_local_income_tax_krw",
            ];

            for column in columns {
                assert!(
                    MIGRATION.contains(&format!("ADD COLUMN {column} BIGINT NOT NULL DEFAULT 0"))
                );
                assert!(MIGRATION.contains(&format!("{column} >= 0")));
            }
        }

        #[test]
        fn given_기존_연간누계행_when_트리거를_확인하면_then_새버킷도_감소시킬수_없다() {
            let columns = [
                "tax_exempt_financial_income_krw",
                "separate_tax_financial_income_krw",
                "separate_withheld_income_tax_krw",
                "separate_withheld_local_income_tax_krw",
            ];

            assert!(MIGRATION.contains("DROP TRIGGER tr_financial_income_year_identity_only"));
            for column in columns {
                assert!(MIGRATION.contains(&format!("NEW.{column} >= OLD.{column}")));
            }
        }
    }

    mod context_tax_account_payout_postings_are_built {
        use super::*;

        #[test]
        fn given_net_and_tax_when_built_then_the_three_postings_reconcile() {
            let expected = vec![
                LedgerPosting {
                    account_code: LedgerAccountCode::AccountCash,
                    financial_account_id: Some(given_account_id()),
                    amount_krw: -1_000,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::Wallet,
                    financial_account_id: None,
                    amount_krw: 900,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::WithholdingTaxLiability,
                    financial_account_id: None,
                    amount_krw: 100,
                },
            ];

            let result = close_or_withdrawal_postings(given_account_id(), 1_000, 900, 100)
                .expect("세후 지급 posting을 만들어야 한다");

            assert_eq!(result, expected);
        }

        #[test]
        fn given_zero_tax_when_built_then_no_zero_tax_posting_is_created() {
            let result = close_or_withdrawal_postings(given_account_id(), 1_000, 1_000, 0)
                .expect("무세액 지급 posting을 만들어야 한다");

            assert_eq!(result.len(), 2);
        }

        #[test]
        fn given_an_unreconciled_payout_when_built_then_it_is_rejected() {
            let result = close_or_withdrawal_postings(given_account_id(), 1_000, 899, 100);

            assert!(result.is_err());
        }
    }

    mod context_pension_snapshot_helpers_are_evaluated {
        use super::*;

        #[test]
        fn given_v4_llx가격과_벤치마크가_다를때_when_평가가격을_고르면_then_llx가격을_사용한다() {
            let benchmark_close_krw = 101_000;
            let llx_close_krw = Some(100_999);

            let close = tax_account_equity_close_krw(benchmark_close_krw, llx_close_krw)
                .expect("v4 LLX 평가가격을 골라야 한다");

            assert_eq!(close, 100_999);
        }

        #[test]
        fn given_보존월드에_llx가격이_없을때_when_평가가격을_고르면_then_벤치마크를_사용한다() {
            let benchmark_close_krw = 101_000;

            let close = tax_account_equity_close_krw(benchmark_close_krw, None)
                .expect("보존월드 평가가격을 골라야 한다");

            assert_eq!(close, benchmark_close_krw);
        }

        #[test]
        fn given_현금과_국채가_있는_irp_when_총가치를_계산하면_then_국채도_안전자산으로_포함한다() {
            let cash_krw = 2_498_900;
            let bond_market_value_krw = 1_001_100;

            let total = pension_total_value_krw(cash_krw, 0, bond_market_value_krw)
                .expect("IRP 총가치를 계산해야 한다");

            assert_eq!(total, 3_500_000);
        }

        #[test]
        fn given_zero_total_value_when_ratio_is_calculated_then_zero_is_returned() {
            let result = ratio_ppm(0, 0).expect("빈 연금계좌 비율을 계산해야 한다");

            assert_eq!(result, 0);
        }

        #[test]
        fn given_a_fractional_ratio_when_calculated_then_ppm_is_floored() {
            let result = ratio_ppm(1, 3).expect("연금 위험자산 비율을 계산해야 한다");

            assert_eq!(result, 333_333);
        }

        #[test]
        fn given_risk_value_above_total_value_when_calculated_then_it_is_rejected() {
            let result = ratio_ppm(2, 1);

            assert!(result.is_err());
        }

        #[test]
        fn given_same_year_contributions_when_start_limit_is_calculated_then_the_stored_zero_opening_wins()
         {
            let rules = create_tax_account_rules();
            let _current_value_after_contribution_krw = 10_000_000;

            let result = calculate_pension_limit(rules.as_ref(), 1, 0)
                .expect("개설 연도 연금수령한도를 계산해야 한다");

            assert_eq!(result, Some(0));
        }

        #[test]
        fn given_a_next_year_opening_when_current_value_changes_then_the_pinned_opening_is_preserved()
         {
            let rules = create_tax_account_rules();
            let pinned_january_first_value_krw = 10_000_000;
            let _later_current_value_krw = 20_000_000;

            let result = calculate_pension_limit(rules.as_ref(), 1, pinned_january_first_value_krw)
                .expect("다음 연도 연금수령한도를 계산해야 한다");

            assert_eq!(result, Some(1_200_000));
        }
    }

    mod context_pension_contribution_allocations_are_audited {
        use super::*;

        #[test]
        fn given_two_accounts_when_serialized_then_every_post_allocation_is_in_id_order() {
            let allocations = vec![
                PensionContributionYearMutation {
                    financial_account_id: 2,
                    next_total_contribution_krw: 6_000_000,
                    next_credit_eligible_krw: 6_000_000,
                    expected_credit_rate_ppm: 165_000,
                    next_expected_credit_krw: 990_000,
                },
                PensionContributionYearMutation {
                    financial_account_id: 9,
                    next_total_contribution_krw: 9_000_000,
                    next_credit_eligible_krw: 3_000_000,
                    expected_credit_rate_ppm: 165_000,
                    next_expected_credit_krw: 495_000,
                },
            ];
            let expected = serde_json::json!([
                {
                    "accountId": "2",
                    "totalContributionKrw": 6_000_000,
                    "creditEligibleKrw": 6_000_000,
                    "expectedCreditRatePpm": 165_000,
                    "expectedCreditKrw": 990_000,
                },
                {
                    "accountId": "9",
                    "totalContributionKrw": 9_000_000,
                    "creditEligibleKrw": 3_000_000,
                    "expectedCreditRatePpm": 165_000,
                    "expectedCreditKrw": 495_000,
                }
            ]);

            let result = pension_contribution_allocations_value(&allocations)
                .expect("연금 납입 배분을 감사 payload로 만들어야 한다");

            assert_eq!(result, expected);
        }

        #[test]
        fn given_out_of_order_accounts_when_serialized_then_the_payload_is_rejected() {
            let allocations = vec![
                PensionContributionYearMutation {
                    financial_account_id: 9,
                    next_total_contribution_krw: 1,
                    next_credit_eligible_krw: 1,
                    expected_credit_rate_ppm: 165_000,
                    next_expected_credit_krw: 0,
                },
                PensionContributionYearMutation {
                    financial_account_id: 2,
                    next_total_contribution_krw: 1,
                    next_credit_eligible_krw: 1,
                    expected_credit_rate_ppm: 165_000,
                    next_expected_credit_krw: 0,
                },
            ];

            let result = pension_contribution_allocations_value(&allocations);

            assert!(result.is_err());
        }
    }
}
