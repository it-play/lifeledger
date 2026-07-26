//! Transaction-scoped persistence for M2-D LLX, bond, gold, and pension assets (§8.2–§9.4).

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use sqlx::{MySql, Transaction};
use time::Date;

use super::annual_tax::{AnnualTaxRunContext, accrue_financial_income_source};
use super::finance::MySqlFinanceStore;
use super::mysql::{
    CommandIdentitySpec, CommandIdentityState, GameCommandReceiptWrite, inspect_command_identity,
    write_command_identity, write_game_command_receipt, write_ledger_transaction,
};
use super::types::GameCommandCursor;
use crate::finance::{
    AssetOrderSide, BondCatalog, BondExecutionInput, BondLot, BondOrderCommand, BondOrderReceipt,
    BondOrderResponse, BondOrderSide, BondPositionSnapshot, BondProductCatalogItem,
    BondProductTerms, BondSeriesCatalogItem, BondTerm, CanonicalDate, CommandCursor, CommandId,
    FinanceFailureCode, FinancialIncomeAccrual, FinancialIncomeSource, GoldAccountSnapshot,
    GoldBarSize, GoldCatalog, GoldOrderCommand, GoldOrderInput, GoldOrderReceipt,
    GoldOrderResponse, GoldOrderSide, GoldPosition, GoldProductCatalogItem, GoldProductTerms,
    GoldTaxPolicy, GoldUnit, GoldWithdrawalBar, GoldWithdrawalCommand, GoldWithdrawalInput,
    GoldWithdrawalReceipt, GoldWithdrawalResponse, IndexProductSnapshot, IrpPostOrderRiskDecision,
    IrpPostOrderRiskInput, IrpRiskExposureChange, IrpRiskPolicy, LedgerAccountCode, LedgerPosting,
    LedgerSource, LedgerSourceKind, LedgerTransactionDraft, LlxDistributionEntitlementSnapshot,
    LlxDistributionMovement, LlxEntitlementInput, LlxProductTerms, LlxQuarterRecordDateInput,
    M2dAccountType, M2dAssetCommandResult, M2dAssetError, M2dAssetSnapshot, OpenGoldAccountCommand,
    OpenGoldAccountReceipt, OpenGoldAccountResponse, PendingEntitlementStatus,
    PensionMarkToMarketInput, PensionTaxLayers, PensionTradeSide, PensionValuationBasisInput,
    PensionValueEventDraft, PhysicalGoldHoldingSnapshot, ProductBundleSnapshot, ResourceId, RunId,
    RunPolicyContext, adjust_pension_valuation_basis, create_bond_series,
    decide_irp_post_order_risk, dirty_bond_price_krw, draft_llx_distribution_entitlement,
    draft_pension_mark_to_market_event, is_llx_quarter_record_date, plan_bond_execution,
    plan_gold_order, plan_gold_withdrawal,
};

const COMMAND_KIND_BOND_ORDER: &str = "placeBondOrder";
const COMMAND_KIND_OPEN_GOLD_ACCOUNT: &str = "openGoldAccount";
const COMMAND_KIND_GOLD_ORDER: &str = "placeGoldOrder";
const COMMAND_KIND_GOLD_WITHDRAWAL: &str = "withdrawGold";
const BOND_COUPON_RATE_STEP_BP: i32 = 25;
const MAX_FINANCIAL_ACCOUNTS: i64 = 32;
const SNAPSHOT_QUERY_EXTRA_ROW: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct M2dBundleCatalog {
    pub market_version: String,
    pub index_product: M2dIndexProduct,
    pub bond_products: [M2dBondProduct; 2],
    pub gold_product: M2dGoldProduct,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct M2dIndexProduct {
    pub id: ResourceId,
    pub key: String,
    pub display_name: String,
    pub annual_management_fee_ppm: i64,
    pub annual_distribution_rate_ppm: i64,
    pub day_count_denominator: u32,
    pub buy_fee_ppm: i64,
    pub sell_fee_ppm: i64,
    pub sell_tax_ppm: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct M2dBondProduct {
    pub id: ResourceId,
    pub key: String,
    pub display_name: String,
    pub terms: BondProductTerms,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct M2dGoldProduct {
    pub id: ResourceId,
    pub key: String,
    pub display_name: String,
    pub terms: GoldProductTerms,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedAssetSaveRow {
    id: u64,
    market_world_id: u64,
    policy_set_id: u64,
    market_world_product_bundle_id: Option<u64>,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
    has_character: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct BundleCatalogRow {
    market_version: String,
    index_id: u64,
    index_key: String,
    index_display_name: String,
    index_annual_management_fee_ppm: i64,
    index_annual_distribution_rate_ppm: i64,
    index_day_count_denominator: u32,
    index_buy_fee_ppm: i64,
    index_sell_fee_ppm: i64,
    index_sell_tax_ppm: i64,
    bond_3y_id: u64,
    bond_3y_key: String,
    bond_3y_display_name: String,
    bond_3y_term_years: u8,
    bond_3y_face_value_krw: i64,
    bond_3y_max_order_units: u32,
    bond_3y_max_position_units: u32,
    bond_3y_buy_fee_ppm: i64,
    bond_3y_sell_fee_ppm: i64,
    bond_10y_id: u64,
    bond_10y_key: String,
    bond_10y_display_name: String,
    bond_10y_term_years: u8,
    bond_10y_face_value_krw: i64,
    bond_10y_max_order_units: u32,
    bond_10y_max_position_units: u32,
    bond_10y_buy_fee_ppm: i64,
    bond_10y_sell_fee_ppm: i64,
    gold_id: u64,
    gold_key: String,
    gold_display_name: String,
    gold_unit: String,
    gold_buy_fee_ppm: i64,
    gold_sell_fee_ppm: i64,
    gold_buy_tax_ppm: i64,
    gold_sell_tax_ppm: i64,
    gold_withdrawal_100g_fee_krw: i64,
    gold_withdrawal_1000g_fee_krw: i64,
}

impl BundleCatalogRow {
    fn into_catalog(self) -> Result<M2dBundleCatalog> {
        ensure!(
            self.bond_3y_term_years == 3,
            "bundle 3-year bond has wrong term"
        );
        ensure!(
            self.bond_10y_term_years == 10,
            "bundle 10-year bond has wrong term"
        );
        ensure!(
            self.gold_unit == "gram",
            "bundle gold product has wrong unit"
        );

        let catalog = M2dBundleCatalog {
            market_version: self.market_version,
            index_product: M2dIndexProduct {
                id: resource_id(self.index_id, "index product")?,
                key: self.index_key,
                display_name: self.index_display_name,
                annual_management_fee_ppm: self.index_annual_management_fee_ppm,
                annual_distribution_rate_ppm: self.index_annual_distribution_rate_ppm,
                day_count_denominator: self.index_day_count_denominator,
                buy_fee_ppm: self.index_buy_fee_ppm,
                sell_fee_ppm: self.index_sell_fee_ppm,
                sell_tax_ppm: self.index_sell_tax_ppm,
            },
            bond_products: [
                M2dBondProduct {
                    id: resource_id(self.bond_3y_id, "3-year bond product")?,
                    key: self.bond_3y_key,
                    display_name: self.bond_3y_display_name,
                    terms: BondProductTerms {
                        term: BondTerm::Years3,
                        face_value_krw: self.bond_3y_face_value_krw,
                        maximum_order_units: self.bond_3y_max_order_units,
                        maximum_position_units: self.bond_3y_max_position_units,
                        coupon_rate_step_bp: BOND_COUPON_RATE_STEP_BP,
                        buy_fee_rate_ppm: self.bond_3y_buy_fee_ppm,
                        sell_fee_rate_ppm: self.bond_3y_sell_fee_ppm,
                        buy_tax_rate_ppm: 0,
                        sell_tax_rate_ppm: 0,
                    },
                },
                M2dBondProduct {
                    id: resource_id(self.bond_10y_id, "10-year bond product")?,
                    key: self.bond_10y_key,
                    display_name: self.bond_10y_display_name,
                    terms: BondProductTerms {
                        term: BondTerm::Years10,
                        face_value_krw: self.bond_10y_face_value_krw,
                        maximum_order_units: self.bond_10y_max_order_units,
                        maximum_position_units: self.bond_10y_max_position_units,
                        coupon_rate_step_bp: BOND_COUPON_RATE_STEP_BP,
                        buy_fee_rate_ppm: self.bond_10y_buy_fee_ppm,
                        sell_fee_rate_ppm: self.bond_10y_sell_fee_ppm,
                        buy_tax_rate_ppm: 0,
                        sell_tax_rate_ppm: 0,
                    },
                },
            ],
            gold_product: M2dGoldProduct {
                id: resource_id(self.gold_id, "gold product")?,
                key: self.gold_key,
                display_name: self.gold_display_name,
                terms: GoldProductTerms {
                    buy_fee_ppm: self.gold_buy_fee_ppm,
                    sell_fee_ppm: self.gold_sell_fee_ppm,
                    buy_tax_ppm: self.gold_buy_tax_ppm,
                    sell_tax_ppm: self.gold_sell_tax_ppm,
                    withdrawal_fee_100g_krw: self.gold_withdrawal_100g_fee_krw,
                    withdrawal_fee_1kg_krw: self.gold_withdrawal_1000g_fee_krw,
                },
            },
        };
        validate_bundle_catalog(&catalog)?;
        Ok(catalog)
    }
}

fn validate_bundle_catalog(catalog: &M2dBundleCatalog) -> Result<()> {
    ensure!(
        !catalog.market_version.is_empty(),
        "market version is empty"
    );
    ensure!(
        !catalog.index_product.key.is_empty()
            && !catalog.index_product.display_name.is_empty()
            && catalog.index_product.day_count_denominator > 0,
        "index product contract is incomplete"
    );
    ensure!(
        catalog.bond_products[0].id != catalog.bond_products[1].id,
        "bundle bond products are not distinct"
    );
    for product in &catalog.bond_products {
        ensure!(
            !product.key.is_empty()
                && !product.display_name.is_empty()
                && product.terms.face_value_krw > 0
                && product.terms.maximum_order_units > 0
                && product.terms.maximum_order_units <= product.terms.maximum_position_units,
            "bond product contract is invalid"
        );
    }
    ensure!(
        !catalog.gold_product.key.is_empty() && !catalog.gold_product.display_name.is_empty(),
        "gold product contract is incomplete"
    );
    Ok(())
}

impl MySqlFinanceStore {
    pub(super) async fn read_bond_catalog(&self, user_id: u64) -> Result<BondCatalog> {
        let mut tx = self.pool.begin().await?;
        let save = read_asset_save_for_user(&mut tx, user_id).await?;
        let Some(save) = save else {
            bail!("bond catalog requires an existing save");
        };
        ensure_monthly_bond_series_in_tx(&mut tx, save.market_world_id, save.game_day).await?;
        let catalog = read_bond_catalog_in_tx(&mut tx, &save).await?;
        tx.commit().await?;
        Ok(catalog)
    }

    pub(super) async fn read_gold_catalog(&self, user_id: u64) -> Result<GoldCatalog> {
        let mut tx = self.pool.begin().await?;
        let save = read_asset_save_for_user(&mut tx, user_id).await?;
        let Some(save) = save else {
            bail!("gold catalog requires an existing save");
        };
        let catalog = read_gold_catalog_in_tx(&mut tx, &save).await?;
        tx.commit().await?;
        Ok(catalog)
    }

    pub(super) async fn place_bond_order(
        &self,
        user_id: u64,
        command: &BondOrderCommand,
    ) -> Result<M2dAssetCommandResult<BondOrderResponse>> {
        let fingerprint = bond_order_fingerprint(command);
        let mut tx = self.pool.begin().await?;
        let Some(current) = lock_asset_save_for_user(&mut tx, user_id).await? else {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::CharacterRequired,
            ));
        };
        let identity = CommandIdentitySpec {
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_BOND_ORDER,
            payload_sha256: &fingerprint,
            cursor: command.cursor,
        };
        match inspect_command_identity(&mut tx, current.id, &identity).await? {
            CommandIdentityState::Conflict => {
                tx.commit().await?;
                return Ok(M2dAssetCommandResult::Rejected(
                    FinanceFailureCode::IdempotencyConflict,
                ));
            }
            CommandIdentityState::Matching => {
                let mut receipt: BondOrderReceipt = read_asset_receipt(
                    &mut tx,
                    current.id,
                    &command.command_id,
                    COMMAND_KIND_BOND_ORDER,
                    &fingerprint,
                )
                .await?
                .context("bond-order command identity has no final receipt")?;
                ensure!(!receipt.replayed, "stored bond-order receipt is replayed");
                receipt.replayed = true;
                let snapshot = read_m2d_asset_snapshot_in_tx(&mut tx, &current).await?;
                tx.commit().await?;
                return Ok(M2dAssetCommandResult::Applied(BondOrderResponse {
                    bond_order: receipt,
                    snapshot,
                }));
            }
            CommandIdentityState::Missing => {}
        }
        if command.validate().is_err() {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::InvalidCommand,
            ));
        }
        if let Some(rejection) = validate_asset_current(&current, command.cursor) {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(rejection));
        }

        ensure_monthly_bond_series_in_tx(&mut tx, current.market_world_id, current.game_day)
            .await?;
        let market =
            read_current_market(&mut tx, current.market_world_id, current.game_day).await?;
        if !market.market_open {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::MarketClosed,
            ));
        }
        let Some(bundle) = read_bundle_catalog(
            &mut tx,
            current.market_world_id,
            current.market_world_product_bundle_id,
        )
        .await?
        else {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::ProductNotFound,
            ));
        };
        let series_row: Option<BondSeriesRow> = sqlx::query_as(
            "SELECT id, product_version_id, issued_date, maturity_date,
                    coupon_rate_bp, issue_yield_bp
             FROM bond_series WHERE market_world_id = ? AND id = ?",
        )
        .bind(current.market_world_id)
        .bind(command.series_id.get())
        .fetch_optional(&mut *tx)
        .await?;
        let Some(series_row) = series_row.filter(|row| {
            bond_series_is_tradable(row.issued_date, row.maturity_date, market.market_date)
        }) else {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::ProductNotFound,
            ));
        };
        let Some(product) = bundle
            .bond_products
            .iter()
            .find(|product| product.id.get() == series_row.product_version_id)
        else {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::ProductNotFound,
            ));
        };
        let series = create_bond_series(
            product.terms,
            series_row.issued_date,
            series_row.issue_yield_bp,
        )?;
        ensure!(
            series.maturity_date == series_row.maturity_date
                && series.coupon_rate_bp == series_row.coupon_rate_bp,
            "stored bond series disagrees with immutable product terms"
        );
        let current_yield_bp = match product.terms.term {
            BondTerm::Years3 => market.treasury_3y_bp,
            BondTerm::Years10 => market.treasury_10y_bp,
        }
        .context("current bond yield is missing")?;
        let dirty_price_krw =
            dirty_bond_price_krw(market.market_date, current_yield_bp, &series.cash_flows)?;

        let account: Option<LockedBondAccountRow> = sqlx::query_as(
            "SELECT account_type, status, cash_krw FROM financial_account
             WHERE save_id = ? AND run_revision = ? AND id = ? FOR UPDATE",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(command.account_id.get())
        .fetch_optional(&mut *tx)
        .await?;
        let Some(account) = account else {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::AccountNotFound,
            ));
        };
        if account.status != "open" {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::AccountClosed,
            ));
        }
        if !bond_account_type_allowed(&account.account_type) {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::AccountTypeNotAllowed,
            ));
        }

        let position: Option<LockedBondPositionRow> = sqlx::query_as(
            "SELECT bond_units FROM bond_position
             WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
               AND market_world_id = ? AND series_id = ? FOR UPDATE",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(command.account_id.get())
        .bind(current.market_world_id)
        .bind(command.series_id.get())
        .fetch_optional(&mut *tx)
        .await?;
        let position = position.unwrap_or_default();
        let lot_rows: Vec<LockedBondLotRow> = sqlx::query_as(
            "SELECT id, remaining_units, remaining_cost_basis_krw
             FROM bond_lot
             WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
               AND market_world_id = ? AND series_id = ? AND remaining_units > 0
             ORDER BY acquired_game_day, id FOR UPDATE",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(command.account_id.get())
        .bind(current.market_world_id)
        .bind(command.series_id.get())
        .fetch_all(&mut *tx)
        .await?;
        let plan = match plan_bond_execution(
            product.terms,
            BondExecutionInput {
                side: bond_order_side(command.side),
                units: command.bond_units,
                dirty_price_krw,
                current_position_units: position.bond_units,
                lots: lot_rows
                    .iter()
                    .map(|row| BondLot {
                        units: row.remaining_units,
                        cost_basis_krw: row.remaining_cost_basis_krw,
                    })
                    .collect(),
            },
        ) {
            Ok(plan) => plan,
            Err(error) => {
                let failure = asset_error_failure(error);
                tx.commit().await?;
                return Ok(M2dAssetCommandResult::Rejected(failure));
            }
        };
        let account_cash_after_krw = match command.side {
            AssetOrderSide::Buy => {
                let acquisition_cost = plan
                    .gross_amount_krw
                    .checked_add(plan.fee_krw)
                    .and_then(|value| value.checked_add(plan.tax_krw))
                    .context("bond acquisition cost overflowed")?;
                let cash_after = account
                    .cash_krw
                    .checked_sub(acquisition_cost)
                    .context("bond acquisition cash overflowed")?;
                if cash_after < 0 {
                    tx.commit().await?;
                    return Ok(M2dAssetCommandResult::Rejected(
                        FinanceFailureCode::InsufficientAccountCash,
                    ));
                }
                cash_after
            }
            AssetOrderSide::Sell => account
                .cash_krw
                .checked_add(plan.gross_amount_krw)
                .and_then(|value| value.checked_sub(plan.fee_krw))
                .and_then(|value| value.checked_sub(plan.tax_krw))
                .context("bond sale cash overflowed")?,
        };
        let total_cost_basis_after = plan
            .remaining_lots
            .iter()
            .try_fold(0_i64, |total, lot| total.checked_add(lot.cost_basis_krw))
            .context("bond lot basis total overflowed")?;

        write_command_identity(&mut tx, current.id, &identity).await?;
        let committed = increment_asset_state_revision(&mut tx, &current).await?;
        let ledger = create_asset_trade_ledger(
            &*self.rules,
            AssetTradeLedgerInput {
                current: &current,
                account_id: command.account_id,
                command_id: &command.command_id,
                side: command.side,
                gross_amount_krw: plan.gross_amount_krw,
                fee_krw: plan.fee_krw,
                tax_krw: plan.tax_krw,
                removed_cost_basis_krw: plan.removed_cost_basis_krw,
                description: match command.side {
                    AssetOrderSide::Buy => "국채 매수",
                    AssetOrderSide::Sell => "국채 매도",
                },
            },
        )?;
        let ledger_transaction_id = write_ledger_transaction(&mut tx, &ledger).await?;
        sqlx::query(
            "UPDATE financial_account SET cash_krw = ?
             WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'open'",
        )
        .bind(account_cash_after_krw)
        .bind(current.id)
        .bind(current.run_revision)
        .bind(command.account_id.get())
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO bond_position
                 (save_id, run_revision, financial_account_id, market_world_id,
                  series_id, product_version_id, bond_units, total_cost_basis_krw)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON DUPLICATE KEY UPDATE bond_units = VALUES(bond_units),
                 total_cost_basis_krw = VALUES(total_cost_basis_krw)",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(command.account_id.get())
        .bind(current.market_world_id)
        .bind(command.series_id.get())
        .bind(product.id.get())
        .bind(plan.position_units_after)
        .bind(total_cost_basis_after)
        .execute(&mut *tx)
        .await?;
        let execution_insert = sqlx::query(
            "INSERT INTO bond_execution
                 (save_id, run_revision, state_revision, game_day, command_id,
                  financial_account_id, market_world_id, series_id, product_version_id,
                  side, bond_units, dirty_price_krw, gross_amount_krw, fee_krw,
                  tax_krw, removed_cost_basis_krw, realized_gain_loss_krw,
                  ledger_transaction_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(committed.state_revision)
        .bind(current.game_day)
        .bind(command.command_id.as_str())
        .bind(command.account_id.get())
        .bind(current.market_world_id)
        .bind(command.series_id.get())
        .bind(product.id.get())
        .bind(asset_side_str(command.side))
        .bind(command.bond_units)
        .bind(dirty_price_krw)
        .bind(plan.gross_amount_krw)
        .bind(plan.fee_krw)
        .bind(plan.tax_krw)
        .bind(plan.removed_cost_basis_krw)
        .bind(plan.realized_gain_loss_krw)
        .bind(ledger_transaction_id)
        .execute(&mut *tx)
        .await?;
        let execution_id = execution_insert.last_insert_id();
        ensure!(
            execution_id != 0,
            "bond execution insert returned no identifier"
        );

        match command.side {
            AssetOrderSide::Buy => {
                let acquisition_cost_krw = plan
                    .gross_amount_krw
                    .checked_add(plan.fee_krw)
                    .and_then(|value| value.checked_add(plan.tax_krw))
                    .context("bond lot acquisition cost overflowed")?;
                sqlx::query(
                    "INSERT INTO bond_lot
                         (save_id, run_revision, financial_account_id, market_world_id,
                          series_id, acquired_execution_id, acquired_game_day,
                          original_units, remaining_units, original_cost_basis_krw,
                          remaining_cost_basis_krw)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(current.id)
                .bind(current.run_revision)
                .bind(command.account_id.get())
                .bind(current.market_world_id)
                .bind(command.series_id.get())
                .bind(execution_id)
                .bind(current.game_day)
                .bind(command.bond_units)
                .bind(command.bond_units)
                .bind(acquisition_cost_krw)
                .bind(acquisition_cost_krw)
                .execute(&mut *tx)
                .await?;
                if position.bond_units == 0 {
                    schedule_bond_cash_flows(
                        &mut tx,
                        &current,
                        command.account_id,
                        command.series_id,
                        product.id,
                        &series,
                    )
                    .await?;
                }
            }
            AssetOrderSide::Sell => {
                for removal in &plan.removals {
                    let row = lot_rows
                        .get(removal.lot_index)
                        .context("bond FIFO plan refers to a missing lot")?;
                    sqlx::query(
                        "UPDATE bond_lot
                         SET remaining_units = ?, remaining_cost_basis_krw = ?
                         WHERE id = ? AND save_id = ? AND run_revision = ?",
                    )
                    .bind(
                        row.remaining_units
                            .checked_sub(removal.removed_units)
                            .context("bond lot units underflowed")?,
                    )
                    .bind(
                        row.remaining_cost_basis_krw
                            .checked_sub(removal.removed_cost_basis_krw)
                            .context("bond lot basis underflowed")?,
                    )
                    .bind(row.id)
                    .bind(current.id)
                    .bind(current.run_revision)
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }

        if matches!(account.account_type.as_str(), "pensionSavings" | "irp") {
            apply_pension_trade_basis_adjustment_in_tx(
                &mut tx,
                &current,
                command.account_id,
                &command.command_id,
                command.side,
                plan.gross_amount_krw,
                false,
            )
            .await?;
        }

        let receipt = BondOrderReceipt {
            command_id: command.command_id.clone(),
            execution_id: resource_id(execution_id, "bond execution")?,
            account_id: command.account_id,
            series_id: command.series_id,
            side: command.side,
            bond_units: command.bond_units,
            dirty_price_krw,
            gross_amount_krw: plan.gross_amount_krw,
            fee_krw: plan.fee_krw,
            tax_krw: plan.tax_krw,
            removed_cost_basis_krw: plan.removed_cost_basis_krw,
            realized_gain_loss_krw: plan.realized_gain_loss_krw,
            replayed: false,
        };
        write_game_command_receipt(
            &mut tx,
            GameCommandReceiptWrite {
                save_id: current.id,
                command_id: &command.command_id,
                command_kind: COMMAND_KIND_BOND_ORDER,
                payload_sha256: &fingerprint,
                market_world_id: current.market_world_id,
                committed_cursor: committed,
                result: &receipt,
                ledger_transaction_id: Some(ledger_transaction_id),
            },
        )
        .await?;
        let snapshot = read_m2d_asset_snapshot_in_tx(&mut tx, &current).await?;
        tx.commit().await?;
        Ok(M2dAssetCommandResult::Applied(BondOrderResponse {
            bond_order: receipt,
            snapshot,
        }))
    }

    pub(super) async fn place_gold_order(
        &self,
        user_id: u64,
        command: &GoldOrderCommand,
    ) -> Result<M2dAssetCommandResult<GoldOrderResponse>> {
        let fingerprint = gold_order_fingerprint(command);
        let mut tx = self.pool.begin().await?;
        let Some(current) = lock_asset_save_for_user(&mut tx, user_id).await? else {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::CharacterRequired,
            ));
        };
        let identity = CommandIdentitySpec {
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_GOLD_ORDER,
            payload_sha256: &fingerprint,
            cursor: command.cursor,
        };
        match inspect_command_identity(&mut tx, current.id, &identity).await? {
            CommandIdentityState::Conflict => {
                tx.commit().await?;
                return Ok(M2dAssetCommandResult::Rejected(
                    FinanceFailureCode::IdempotencyConflict,
                ));
            }
            CommandIdentityState::Matching => {
                let mut receipt: GoldOrderReceipt = read_asset_receipt(
                    &mut tx,
                    current.id,
                    &command.command_id,
                    COMMAND_KIND_GOLD_ORDER,
                    &fingerprint,
                )
                .await?
                .context("gold-order command identity has no final receipt")?;
                ensure!(!receipt.replayed, "stored gold-order receipt is replayed");
                receipt.replayed = true;
                let snapshot = read_m2d_asset_snapshot_in_tx(&mut tx, &current).await?;
                tx.commit().await?;
                return Ok(M2dAssetCommandResult::Applied(GoldOrderResponse {
                    gold_order: receipt,
                    snapshot,
                }));
            }
            CommandIdentityState::Missing => {}
        }
        if command.validate().is_err() {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::InvalidCommand,
            ));
        }
        if let Some(rejection) = validate_asset_current(&current, command.cursor) {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(rejection));
        }

        let market =
            read_current_market(&mut tx, current.market_world_id, current.game_day).await?;
        if !market.market_open {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::MarketClosed,
            ));
        }
        let price_krw_per_gram = market
            .gold_close_krw_per_gram
            .context("M2-D gold close is missing on an open day")?;
        let Some(bundle) = read_bundle_catalog(
            &mut tx,
            current.market_world_id,
            current.market_world_product_bundle_id,
        )
        .await?
        else {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::ProductNotFound,
            ));
        };
        let Some(account) = lock_gold_account(&mut tx, &current, command.account_id).await? else {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::AccountNotFound,
            ));
        };
        if account.account_type != "krxGold" {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::AccountTypeNotAllowed,
            ));
        }
        if account.account_status != "open" || account.contract_status.as_deref() != Some("active")
        {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::AccountClosed,
            ));
        }
        let product_version_id = account
            .product_version_id
            .context("open gold account has no active product contract")?;
        let quantity_gram = account
            .quantity_gram
            .context("open gold account has no position row")?;
        let total_cost_basis_krw = account
            .total_cost_basis_krw
            .context("open gold account has no position basis")?;
        if product_version_id != bundle.gold_product.id.get() {
            bail!("active gold account is outside the pinned product bundle");
        }
        let side = gold_order_side(command.side);
        let plan = match plan_gold_order(
            bundle.gold_product.terms,
            GoldOrderInput {
                side,
                quantity_gram: command.quantity_gram,
                price_krw_per_gram,
                account_cash_krw: account.cash_krw,
                position: GoldPosition {
                    quantity_gram,
                    total_cost_basis_krw,
                },
            },
        ) {
            Ok(plan) => plan,
            Err(error) => {
                let failure = asset_error_failure(error);
                tx.commit().await?;
                return Ok(M2dAssetCommandResult::Rejected(failure));
            }
        };

        write_command_identity(&mut tx, current.id, &identity).await?;
        let committed = increment_asset_state_revision(&mut tx, &current).await?;
        let ledger = create_asset_trade_ledger(
            &*self.rules,
            AssetTradeLedgerInput {
                current: &current,
                account_id: command.account_id,
                command_id: &command.command_id,
                side: command.side,
                gross_amount_krw: plan.gross_amount_krw,
                fee_krw: plan.fee_krw,
                tax_krw: plan.tax_krw,
                removed_cost_basis_krw: plan.removed_cost_basis_krw,
                description: match command.side {
                    AssetOrderSide::Buy => "금 현물 매수",
                    AssetOrderSide::Sell => "금 현물 매도",
                },
            },
        )?;
        let ledger_transaction_id = write_ledger_transaction(&mut tx, &ledger).await?;
        sqlx::query(
            "UPDATE financial_account SET cash_krw = ?
             WHERE save_id = ? AND run_revision = ? AND id = ?
               AND status = 'open'",
        )
        .bind(plan.account_cash_after_krw)
        .bind(current.id)
        .bind(current.run_revision)
        .bind(command.account_id.get())
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE gold_position
             SET quantity_gram = ?, total_cost_basis_krw = ?
             WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?",
        )
        .bind(plan.position_after.quantity_gram)
        .bind(plan.position_after.total_cost_basis_krw)
        .bind(current.id)
        .bind(current.run_revision)
        .bind(command.account_id.get())
        .execute(&mut *tx)
        .await?;
        let execution_insert = sqlx::query(
            "INSERT INTO gold_execution
                 (save_id, run_revision, state_revision, game_day, command_id,
                  financial_account_id, product_version_id, side, quantity_gram,
                  price_krw_per_gram, gross_amount_krw, fee_krw, tax_krw,
                  removed_cost_basis_krw, realized_gain_loss_krw,
                  ledger_transaction_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(committed.state_revision)
        .bind(current.game_day)
        .bind(command.command_id.as_str())
        .bind(command.account_id.get())
        .bind(bundle.gold_product.id.get())
        .bind(asset_side_str(command.side))
        .bind(command.quantity_gram)
        .bind(price_krw_per_gram)
        .bind(plan.gross_amount_krw)
        .bind(plan.fee_krw)
        .bind(plan.tax_krw)
        .bind(plan.removed_cost_basis_krw)
        .bind(plan.realized_gain_loss_krw)
        .bind(ledger_transaction_id)
        .execute(&mut *tx)
        .await?;
        let execution_id = execution_insert.last_insert_id();
        ensure!(
            execution_id != 0,
            "gold execution insert returned no identifier"
        );

        let receipt = GoldOrderReceipt {
            command_id: command.command_id.clone(),
            execution_id: resource_id(execution_id, "gold execution")?,
            account_id: command.account_id,
            side: command.side,
            quantity_gram: command.quantity_gram,
            price_krw_per_gram,
            gross_amount_krw: plan.gross_amount_krw,
            fee_krw: plan.fee_krw,
            tax_krw: plan.tax_krw,
            removed_cost_basis_krw: plan.removed_cost_basis_krw,
            realized_gain_loss_krw: plan.realized_gain_loss_krw,
            replayed: false,
        };
        write_game_command_receipt(
            &mut tx,
            GameCommandReceiptWrite {
                save_id: current.id,
                command_id: &command.command_id,
                command_kind: COMMAND_KIND_GOLD_ORDER,
                payload_sha256: &fingerprint,
                market_world_id: current.market_world_id,
                committed_cursor: committed,
                result: &receipt,
                ledger_transaction_id: Some(ledger_transaction_id),
            },
        )
        .await?;
        let snapshot = read_m2d_asset_snapshot_in_tx(&mut tx, &current).await?;
        tx.commit().await?;
        Ok(M2dAssetCommandResult::Applied(GoldOrderResponse {
            gold_order: receipt,
            snapshot,
        }))
    }

    pub(super) async fn withdraw_gold(
        &self,
        user_id: u64,
        command: &GoldWithdrawalCommand,
    ) -> Result<M2dAssetCommandResult<GoldWithdrawalResponse>> {
        let fingerprint = gold_withdrawal_fingerprint(command);
        let mut tx = self.pool.begin().await?;
        let Some(current) = lock_asset_save_for_user(&mut tx, user_id).await? else {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::CharacterRequired,
            ));
        };
        let identity = CommandIdentitySpec {
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_GOLD_WITHDRAWAL,
            payload_sha256: &fingerprint,
            cursor: command.cursor,
        };
        match inspect_command_identity(&mut tx, current.id, &identity).await? {
            CommandIdentityState::Conflict => {
                tx.commit().await?;
                return Ok(M2dAssetCommandResult::Rejected(
                    FinanceFailureCode::IdempotencyConflict,
                ));
            }
            CommandIdentityState::Matching => {
                let mut receipt: GoldWithdrawalReceipt = read_asset_receipt(
                    &mut tx,
                    current.id,
                    &command.command_id,
                    COMMAND_KIND_GOLD_WITHDRAWAL,
                    &fingerprint,
                )
                .await?
                .context("gold-withdrawal command identity has no final receipt")?;
                ensure!(
                    !receipt.replayed,
                    "stored gold-withdrawal receipt is replayed"
                );
                receipt.replayed = true;
                let snapshot = read_m2d_asset_snapshot_in_tx(&mut tx, &current).await?;
                tx.commit().await?;
                return Ok(M2dAssetCommandResult::Applied(GoldWithdrawalResponse {
                    gold_withdrawal: receipt,
                    snapshot,
                }));
            }
            CommandIdentityState::Missing => {}
        }
        if command.validate().is_err() {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::InvalidCommand,
            ));
        }
        if let Some(rejection) = validate_asset_current(&current, command.cursor) {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(rejection));
        }

        let market =
            read_current_market(&mut tx, current.market_world_id, current.game_day).await?;
        let Some(bundle) = read_bundle_catalog(
            &mut tx,
            current.market_world_id,
            current.market_world_product_bundle_id,
        )
        .await?
        else {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::ProductNotFound,
            ));
        };
        let Some(account) = lock_gold_account(&mut tx, &current, command.account_id).await? else {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::AccountNotFound,
            ));
        };
        if account.account_type != "krxGold" {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::AccountTypeNotAllowed,
            ));
        }
        if account.account_status != "open" || account.contract_status.as_deref() != Some("active")
        {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::AccountClosed,
            ));
        }
        let product_version_id = account
            .product_version_id
            .context("open gold account has no active product contract")?;
        let quantity_gram = account
            .quantity_gram
            .context("open gold account has no position row")?;
        let total_cost_basis_krw = account
            .total_cost_basis_krw
            .context("open gold account has no position basis")?;
        ensure!(
            product_version_id == bundle.gold_product.id.get(),
            "active gold account is outside the pinned bundle"
        );
        let policy =
            read_gold_withdrawal_policy(&mut tx, current.policy_set_id, market.market_date).await?;
        let bar_size = GoldBarSize::try_from(command.bar_size_gram)
            .context("validated gold bar size cannot be converted")?;
        let plan = match plan_gold_withdrawal(
            bundle.gold_product.terms,
            GoldTaxPolicy {
                withdrawal_vat_rate_ppm: policy.vat_rate_ppm,
            },
            GoldWithdrawalInput {
                position: GoldPosition {
                    quantity_gram,
                    total_cost_basis_krw,
                },
                account_cash_krw: account.cash_krw,
                bar_size,
                bar_count: command.bar_count,
            },
        ) {
            Ok(plan) => plan,
            Err(error) => {
                let failure = asset_error_failure(error);
                tx.commit().await?;
                return Ok(M2dAssetCommandResult::Rejected(failure));
            }
        };

        write_command_identity(&mut tx, current.id, &identity).await?;
        let committed = increment_asset_state_revision(&mut tx, &current).await?;
        let cash_charged_krw = plan
            .vat_krw
            .checked_add(plan.fee_krw)
            .context("gold withdrawal cash charge overflowed")?;
        let ledger_transaction_id = if cash_charged_krw == 0 {
            None
        } else {
            let ledger = self
                .rules
                .create_ledger_transaction(LedgerTransactionDraft {
                    policy: run_policy(&current)?,
                    source: LedgerSource {
                        kind: LedgerSourceKind::Trade,
                        source_id: command.command_id.as_str().to_owned(),
                    },
                    game_day: current.game_day,
                    description: "금 실물 인출 부가세 및 수수료".to_owned(),
                    postings: vec![
                        LedgerPosting {
                            account_code: LedgerAccountCode::AccountCash,
                            financial_account_id: Some(command.account_id),
                            amount_krw: cash_charged_krw
                                .checked_neg()
                                .context("gold withdrawal charge cannot be negated")?,
                        },
                        LedgerPosting {
                            account_code: LedgerAccountCode::FeeExpense,
                            financial_account_id: None,
                            amount_krw: cash_charged_krw,
                        },
                    ],
                })?;
            Some(write_ledger_transaction(&mut tx, &ledger).await?)
        };
        sqlx::query(
            "UPDATE financial_account SET cash_krw = ?
             WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'open'",
        )
        .bind(plan.account_cash_after_krw)
        .bind(current.id)
        .bind(current.run_revision)
        .bind(command.account_id.get())
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE gold_position SET quantity_gram = ?, total_cost_basis_krw = ?
             WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?",
        )
        .bind(plan.position_after.quantity_gram)
        .bind(plan.position_after.total_cost_basis_krw)
        .bind(current.id)
        .bind(current.run_revision)
        .bind(command.account_id.get())
        .execute(&mut *tx)
        .await?;
        let withdrawal_insert = sqlx::query(
            "INSERT INTO gold_withdrawal
                 (save_id, run_revision, state_revision, game_day, command_id,
                  financial_account_id, product_version_id, bar_size_gram, bar_count,
                  quantity_gram, removed_cost_basis_krw, vat_krw, fee_krw,
                  cash_charged_krw, ledger_transaction_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(committed.state_revision)
        .bind(current.game_day)
        .bind(command.command_id.as_str())
        .bind(command.account_id.get())
        .bind(bundle.gold_product.id.get())
        .bind(command.bar_size_gram)
        .bind(command.bar_count)
        .bind(plan.removed_quantity_gram)
        .bind(plan.removed_cost_basis_krw)
        .bind(plan.vat_krw)
        .bind(plan.fee_krw)
        .bind(cash_charged_krw)
        .bind(ledger_transaction_id)
        .execute(&mut *tx)
        .await?;
        let withdrawal_id = withdrawal_insert.last_insert_id();
        ensure!(
            withdrawal_id != 0,
            "gold withdrawal insert returned no identifier"
        );
        sqlx::query(
            "INSERT INTO physical_gold_holding
                 (save_id, run_revision, financial_account_id, bar_size_gram, bar_count)
             VALUES (?, ?, ?, ?, ?)
             ON DUPLICATE KEY UPDATE bar_count = bar_count + VALUES(bar_count)",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(command.account_id.get())
        .bind(command.bar_size_gram)
        .bind(command.bar_count)
        .execute(&mut *tx)
        .await?;

        let receipt = GoldWithdrawalReceipt {
            command_id: command.command_id.clone(),
            withdrawal_id: resource_id(withdrawal_id, "gold withdrawal")?,
            account_id: command.account_id,
            bar_size_gram: command.bar_size_gram,
            bar_count: command.bar_count,
            quantity_gram: plan.removed_quantity_gram,
            removed_cost_basis_krw: plan.removed_cost_basis_krw,
            vat_krw: plan.vat_krw,
            fee_krw: plan.fee_krw,
            cash_charged_krw,
            replayed: false,
        };
        write_game_command_receipt(
            &mut tx,
            GameCommandReceiptWrite {
                save_id: current.id,
                command_id: &command.command_id,
                command_kind: COMMAND_KIND_GOLD_WITHDRAWAL,
                payload_sha256: &fingerprint,
                market_world_id: current.market_world_id,
                committed_cursor: committed,
                result: &receipt,
                ledger_transaction_id,
            },
        )
        .await?;
        let snapshot = read_m2d_asset_snapshot_in_tx(&mut tx, &current).await?;
        tx.commit().await?;
        Ok(M2dAssetCommandResult::Applied(GoldWithdrawalResponse {
            gold_withdrawal: receipt,
            snapshot,
        }))
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedGoldAccountRow {
    account_type: String,
    account_status: String,
    cash_krw: i64,
    contract_status: Option<String>,
    product_version_id: Option<u64>,
    quantity_gram: Option<u32>,
    total_cost_basis_krw: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedBondAccountRow {
    account_type: String,
    status: String,
    cash_krw: i64,
}

#[derive(Debug, Clone, Default, sqlx::FromRow)]
struct LockedBondPositionRow {
    bond_units: u32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedBondLotRow {
    id: u64,
    remaining_units: u32,
    remaining_cost_basis_krw: i64,
}

async fn schedule_bond_cash_flows(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedAssetSaveRow,
    account_id: ResourceId,
    series_id: ResourceId,
    product_version_id: ResourceId,
    series: &crate::finance::BondSeries,
) -> Result<()> {
    let (world_start_date,): (Date,) =
        sqlx::query_as("SELECT start_date FROM market_world WHERE id = ?")
            .bind(current.market_world_id)
            .fetch_one(&mut **tx)
            .await?;
    let source_id = format!("{}:{}", account_id, series_id);
    ensure!(
        source_id.len() <= 128,
        "bond settlement source ID is too long"
    );
    for (index, cash_flow) in series.cash_flows.iter().enumerate() {
        let whole_days = (cash_flow.payment_date - world_start_date).whole_days();
        let due_game_day = u32::try_from(whole_days)
            .context("bond payment date is outside the market-world game-day range")?;
        let occurrence = u32::try_from(index + 1).context("too many bond cash flows")?;
        let kind = if cash_flow.principal_krw > 0 {
            "bondMaturity"
        } else {
            "bondCoupon"
        };
        let payload = serde_json::json!({
            "version": "v1",
            "accountId": account_id,
            "seriesId": series_id,
            "productVersionId": product_version_id,
            "couponKrwPerUnit": cash_flow.coupon_krw,
            "principalKrwPerUnit": cash_flow.principal_krw,
            "paymentDate": CanonicalDate::from_date(cash_flow.payment_date),
        });
        let payload = serde_json::to_string(&payload)?;
        sqlx::query(
            "INSERT IGNORE INTO scheduled_settlement
                 (save_id, run_revision, due_game_day, kind, payload,
                  source_kind, source_id, occurrence, status)
             VALUES (?, ?, ?, ?, ?, 'bondPosition', ?, ?, 'pending')",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(due_game_day)
        .bind(kind)
        .bind(payload)
        .bind(&source_id)
        .bind(occurrence)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

fn bond_account_type_allowed(account_type: &str) -> bool {
    matches!(
        account_type,
        "taxableBrokerage" | "isaGeneral" | "isaLowIncome" | "pensionSavings" | "irp"
    )
}

const fn bond_order_side(side: AssetOrderSide) -> BondOrderSide {
    match side {
        AssetOrderSide::Buy => BondOrderSide::Buy,
        AssetOrderSide::Sell => BondOrderSide::Sell,
    }
}

async fn lock_gold_account(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedAssetSaveRow,
    account_id: ResourceId,
) -> Result<Option<LockedGoldAccountRow>> {
    sqlx::query_as(
        "SELECT account.account_type, account.status AS account_status, account.cash_krw,
                contract.status AS contract_status, contract.product_version_id,
                position.quantity_gram, position.total_cost_basis_krw
         FROM financial_account AS account
         LEFT JOIN gold_account_contract AS contract
           ON contract.save_id = account.save_id
          AND contract.run_revision = account.run_revision
          AND contract.financial_account_id = account.id
         LEFT JOIN gold_position AS position
           ON position.save_id = account.save_id
          AND position.run_revision = account.run_revision
          AND position.financial_account_id = account.id
         WHERE account.save_id = ? AND account.run_revision = ? AND account.id = ?
         FOR UPDATE",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(account_id.get())
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock the gold account")
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredGoldWithdrawalPolicy {
    vat_rate_ppm: i64,
    withdrawal_units_gram: [u32; 2],
}

async fn read_gold_withdrawal_policy(
    tx: &mut Transaction<'_, MySql>,
    policy_set_id: u64,
    market_date: Date,
) -> Result<StoredGoldWithdrawalPolicy> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT CAST(parameters AS CHAR) FROM policy_rule
         WHERE policy_set_id = ? AND domain = 'gold' AND rule_key = 'krxWithdrawal'
           AND effective_from <= ?
           AND (effective_to IS NULL OR effective_to >= ?)
         ORDER BY effective_from DESC LIMIT 2",
    )
    .bind(policy_set_id)
    .bind(market_date)
    .bind(market_date)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() == 1,
        "gold withdrawal policy is missing or overlapping"
    );
    let policy: StoredGoldWithdrawalPolicy = serde_json::from_str(&rows[0].0)
        .context("gold withdrawal policy does not match its strict schema")?;
    ensure!(
        policy.withdrawal_units_gram == [100, 1_000]
            && (0..=1_000_000).contains(&policy.vat_rate_ppm),
        "gold withdrawal policy has unsupported values"
    );
    Ok(policy)
}

struct AssetTradeLedgerInput<'a> {
    current: &'a LockedAssetSaveRow,
    account_id: ResourceId,
    command_id: &'a CommandId,
    side: AssetOrderSide,
    gross_amount_krw: i64,
    fee_krw: i64,
    tax_krw: i64,
    removed_cost_basis_krw: i64,
    description: &'a str,
}

fn create_asset_trade_ledger(
    rules: &dyn crate::finance::FinanceRules,
    input: AssetTradeLedgerInput<'_>,
) -> Result<crate::finance::LedgerTransaction> {
    let AssetTradeLedgerInput {
        current,
        account_id,
        command_id,
        side,
        gross_amount_krw,
        fee_krw,
        tax_krw,
        removed_cost_basis_krw,
        description,
    } = input;
    ensure!(
        gross_amount_krw > 0 && fee_krw >= 0 && tax_krw >= 0 && removed_cost_basis_krw >= 0,
        "asset trade amounts cannot form a ledger"
    );
    let postings = match side {
        AssetOrderSide::Buy => {
            let acquisition_cost_krw = gross_amount_krw
                .checked_add(fee_krw)
                .and_then(|value| value.checked_add(tax_krw))
                .context("asset acquisition cost overflowed")?;
            vec![
                LedgerPosting {
                    account_code: LedgerAccountCode::AccountCash,
                    financial_account_id: Some(account_id),
                    amount_krw: acquisition_cost_krw
                        .checked_neg()
                        .context("asset acquisition cost cannot be negated")?,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::ProductPrincipal,
                    financial_account_id: Some(account_id),
                    amount_krw: acquisition_cost_krw,
                },
            ]
        }
        AssetOrderSide::Sell => {
            let net_proceeds_krw = gross_amount_krw
                .checked_sub(fee_krw)
                .and_then(|value| value.checked_sub(tax_krw))
                .context("asset net proceeds overflowed")?;
            ensure!(net_proceeds_krw >= 0, "asset charges exceed gross proceeds");
            let realized_ledger_amount = removed_cost_basis_krw
                .checked_sub(net_proceeds_krw)
                .context("asset realized ledger amount overflowed")?;
            let mut postings = vec![
                LedgerPosting {
                    account_code: LedgerAccountCode::AccountCash,
                    financial_account_id: Some(account_id),
                    amount_krw: net_proceeds_krw,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::ProductPrincipal,
                    financial_account_id: Some(account_id),
                    amount_krw: removed_cost_basis_krw
                        .checked_neg()
                        .context("removed asset basis cannot be negated")?,
                },
            ];
            if realized_ledger_amount != 0 {
                postings.push(LedgerPosting {
                    account_code: LedgerAccountCode::RealizedGainLoss,
                    financial_account_id: None,
                    amount_krw: realized_ledger_amount,
                });
            }
            postings
        }
    };
    Ok(rules.create_ledger_transaction(LedgerTransactionDraft {
        policy: run_policy(current)?,
        source: LedgerSource {
            kind: LedgerSourceKind::Trade,
            source_id: command_id.as_str().to_owned(),
        },
        game_day: current.game_day,
        description: description.to_owned(),
        postings,
    })?)
}

fn run_policy(current: &LockedAssetSaveRow) -> Result<RunPolicyContext> {
    Ok(RunPolicyContext {
        run: RunId {
            save_id: resource_id(current.id, "save")?,
            run_revision: current.run_revision,
        },
        policy_set_id: resource_id(current.policy_set_id, "policy set")?,
    })
}

const fn gold_order_side(side: AssetOrderSide) -> GoldOrderSide {
    match side {
        AssetOrderSide::Buy => GoldOrderSide::Buy,
        AssetOrderSide::Sell => GoldOrderSide::Sell,
    }
}

const fn asset_side_str(side: AssetOrderSide) -> &'static str {
    match side {
        AssetOrderSide::Buy => "buy",
        AssetOrderSide::Sell => "sell",
    }
}

const fn asset_error_failure(error: M2dAssetError) -> FinanceFailureCode {
    match error {
        M2dAssetError::InsufficientCash => FinanceFailureCode::InsufficientAccountCash,
        M2dAssetError::InsufficientQuantity => FinanceFailureCode::InsufficientQuantity,
        M2dAssetError::PositionLimitExceeded => FinanceFailureCode::PositionLimit,
        _ => FinanceFailureCode::InvalidCommand,
    }
}

fn gold_order_fingerprint(command: &GoldOrderCommand) -> String {
    fingerprint(&format!(
        "lifeledger.finance.gold-order.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\naccountId={}\nside={}\nquantityGram={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.account_id,
        asset_side_str(command.side),
        command.quantity_gram
    ))
}

fn bond_order_fingerprint(command: &BondOrderCommand) -> String {
    fingerprint(&format!(
        "lifeledger.finance.bond-order.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\naccountId={}\nseriesId={}\nside={}\nbondUnits={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.account_id,
        command.series_id,
        asset_side_str(command.side),
        command.bond_units
    ))
}

fn gold_withdrawal_fingerprint(command: &GoldWithdrawalCommand) -> String {
    fingerprint(&format!(
        "lifeledger.finance.gold-withdrawal.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\naccountId={}\nbarSizeGram={}\nbarCount={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.account_id,
        command.bar_size_gram,
        command.bar_count
    ))
}

#[derive(Debug, Clone, Copy)]
pub(super) struct LlxTaxAccountTradeInput<'a> {
    pub save_id: u64,
    pub market_world_id: u64,
    pub policy_set_id: u64,
    pub run_revision: u32,
    pub game_day: u32,
    pub account_id: ResourceId,
    pub order_id: &'a CommandId,
    pub side: AssetOrderSide,
    pub realized_gain_loss_krw: i64,
    pub execution_market_value_krw: i64,
    pub position_market_value_before_krw: i64,
    pub position_market_value_after_krw: i64,
    pub risk_asset_value_before_krw: i64,
    pub risk_asset_value_after_krw: i64,
    pub account_total_value_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LlxTaxAccountTradeResult {
    Applied,
    Rejected(FinanceFailureCode),
}

/// Applies the tax-account side of one already-planned LLX execution in the caller's transaction.
/// A rejection is decided before any row is changed.
pub(super) async fn prepare_llx_tax_account_trade_in_tx(
    tx: &mut Transaction<'_, MySql>,
    input: LlxTaxAccountTradeInput<'_>,
) -> Result<LlxTaxAccountTradeResult> {
    ensure!(
        input.execution_market_value_krw > 0
            && input.position_market_value_before_krw >= 0
            && input.position_market_value_after_krw >= 0
            && input.risk_asset_value_before_krw >= 0
            && input.risk_asset_value_after_krw >= 0
            && input.account_total_value_krw >= 0,
        "LLX tax-account trade values are invalid"
    );
    let account: Option<(String, String)> = sqlx::query_as(
        "SELECT account.account_type, account.status
         FROM financial_account AS account
         INNER JOIN save
           ON save.id = account.save_id AND save.run_revision = account.run_revision
         WHERE account.save_id = ? AND account.run_revision = ? AND account.id = ?
           AND save.market_world_id = ? AND save.policy_set_id = ?
           AND save.game_day = ? FOR UPDATE",
    )
    .bind(input.save_id)
    .bind(input.run_revision)
    .bind(input.account_id.get())
    .bind(input.market_world_id)
    .bind(input.policy_set_id)
    .bind(input.game_day)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((account_type, status)) = account else {
        return Ok(LlxTaxAccountTradeResult::Rejected(
            FinanceFailureCode::AccountNotFound,
        ));
    };
    if status != "open" {
        return Ok(LlxTaxAccountTradeResult::Rejected(
            FinanceFailureCode::AccountClosed,
        ));
    }

    match account_type.as_str() {
        "taxableBrokerage" => Ok(LlxTaxAccountTradeResult::Applied),
        "isaGeneral" | "isaLowIncome" => {
            if matches!(input.side, AssetOrderSide::Sell) && input.realized_gain_loss_krw != 0 {
                let (profit_delta, loss_delta) = if input.realized_gain_loss_krw > 0 {
                    (input.realized_gain_loss_krw, 0)
                } else {
                    (
                        0,
                        input
                            .realized_gain_loss_krw
                            .checked_neg()
                            .context("ISA realized loss cannot be negated")?,
                    )
                };
                let update = sqlx::query(
                    "UPDATE isa_account_contract
                     SET isa_tax_profit_krw = isa_tax_profit_krw + ?,
                         isa_deductible_loss_krw = isa_deductible_loss_krw + ?
                     WHERE save_id = ? AND run_revision = ?
                       AND financial_account_id = ? AND status = 'active'",
                )
                .bind(profit_delta)
                .bind(loss_delta)
                .bind(input.save_id)
                .bind(input.run_revision)
                .bind(input.account_id.get())
                .execute(&mut **tx)
                .await?;
                ensure!(
                    update.rows_affected() == 1,
                    "active ISA contract is missing"
                );
            }
            Ok(LlxTaxAccountTradeResult::Applied)
        }
        "pensionSavings" | "irp" => {
            if account_type == "irp" {
                let policy = read_pension_risk_policy(
                    tx,
                    input.policy_set_id,
                    input.market_world_id,
                    input.game_day,
                )
                .await?;
                let exposure_change =
                    if input.risk_asset_value_after_krw > input.risk_asset_value_before_krw {
                        IrpRiskExposureChange::Increased
                    } else {
                        IrpRiskExposureChange::NotIncreased
                    };
                let decision = decide_irp_post_order_risk(
                    IrpRiskPolicy {
                        risk_asset_limit_ppm: policy.irp_risk_asset_limit_ppm,
                    },
                    IrpPostOrderRiskInput {
                        post_order_total_value_krw: input.account_total_value_krw,
                        post_order_risk_asset_value_krw: input.risk_asset_value_after_krw,
                        exposure_change,
                    },
                )?;
                if matches!(decision, IrpPostOrderRiskDecision::Rejected { .. }) {
                    return Ok(LlxTaxAccountTradeResult::Rejected(
                        FinanceFailureCode::LimitExceeded,
                    ));
                }
            }
            apply_explicit_pension_trade_basis_in_tx(
                tx,
                PensionTradeBasisWrite {
                    save_id: input.save_id,
                    run_revision: input.run_revision,
                    game_day: input.game_day,
                    account_id: input.account_id,
                    source_kind: "llxOrder",
                    source_id: input.order_id.as_str(),
                    position_market_value_before_krw: input.position_market_value_before_krw,
                    position_market_value_after_krw: input.position_market_value_after_krw,
                    risk_asset_value_after_krw: input.risk_asset_value_after_krw,
                    account_total_value_krw: input.account_total_value_krw,
                },
            )
            .await?;
            Ok(LlxTaxAccountTradeResult::Applied)
        }
        _ => Ok(LlxTaxAccountTradeResult::Rejected(
            FinanceFailureCode::AccountTypeNotAllowed,
        )),
    }
}

#[derive(Debug, Clone, Copy)]
struct PensionTradeBasisWrite<'a> {
    save_id: u64,
    run_revision: u32,
    game_day: u32,
    account_id: ResourceId,
    source_kind: &'a str,
    source_id: &'a str,
    position_market_value_before_krw: i64,
    position_market_value_after_krw: i64,
    risk_asset_value_after_krw: i64,
    account_total_value_krw: i64,
}

async fn apply_explicit_pension_trade_basis_in_tx(
    tx: &mut Transaction<'_, MySql>,
    write: PensionTradeBasisWrite<'_>,
) -> Result<()> {
    ensure!(
        write.position_market_value_before_krw >= 0
            && write.position_market_value_after_krw >= 0
            && write.risk_asset_value_after_krw >= 0
            && write.risk_asset_value_after_krw <= write.position_market_value_after_krw,
        "pension trade valuation basis is invalid"
    );
    let layers = lock_pension_layers(tx, write.save_id, write.run_revision, write.account_id)
        .await?
        .context("active pension tax layers are missing")?;
    ensure!(
        pension_layers_total(layers)? == write.account_total_value_krw,
        "pension trade account total disagrees with tax layers"
    );
    let state: Option<(u32, i64, i64)> = sqlx::query_as(
        "SELECT last_valuation_game_day, position_market_value_krw, risk_asset_value_krw
         FROM pension_valuation_state
         WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
         FOR UPDATE",
    )
    .bind(write.save_id)
    .bind(write.run_revision)
    .bind(write.account_id.get())
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((last_day, position_before, _)) = state {
        ensure!(
            last_day <= write.game_day && position_before == write.position_market_value_before_krw,
            "pension valuation state changed before the trade basis write"
        );
        sqlx::query(
            "UPDATE pension_valuation_state
             SET last_valuation_game_day = ?, position_market_value_krw = ?,
                 risk_asset_value_krw = ?
             WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?",
        )
        .bind(write.game_day)
        .bind(write.position_market_value_after_krw)
        .bind(write.risk_asset_value_after_krw)
        .bind(write.save_id)
        .bind(write.run_revision)
        .bind(write.account_id.get())
        .execute(&mut **tx)
        .await?;
    } else {
        ensure!(
            write.position_market_value_before_krw == 0,
            "missing pension valuation state has a non-zero pre-trade basis"
        );
        sqlx::query(
            "INSERT INTO pension_valuation_state
                 (save_id, run_revision, financial_account_id, last_valuation_game_day,
                  position_market_value_krw, risk_asset_value_krw)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(write.save_id)
        .bind(write.run_revision)
        .bind(write.account_id.get())
        .bind(write.game_day)
        .bind(write.position_market_value_after_krw)
        .bind(write.risk_asset_value_after_krw)
        .execute(&mut **tx)
        .await?;
    }
    sqlx::query(
        "INSERT INTO tax_account_value_event
             (save_id, run_revision, financial_account_id, event_game_day, cause,
              source_kind, source_id, occurrence,
              position_market_value_before_krw, position_market_value_after_krw,
              account_total_before_krw, account_total_after_krw, value_change_krw,
              before_tax_excluded_krw, before_deferred_retirement_krw,
              before_credited_contribution_krw, before_earnings_krw,
              after_tax_excluded_krw, after_deferred_retirement_krw,
              after_credited_contribution_krw, after_earnings_krw)
         VALUES (?, ?, ?, ?, 'tradeBasisAdjustment', ?, ?, 1,
                 ?, ?, ?, ?, 0, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(write.save_id)
    .bind(write.run_revision)
    .bind(write.account_id.get())
    .bind(write.game_day)
    .bind(write.source_kind)
    .bind(write.source_id)
    .bind(write.position_market_value_before_krw)
    .bind(write.position_market_value_after_krw)
    .bind(write.account_total_value_krw)
    .bind(write.account_total_value_krw)
    .bind(layers.tax_excluded_contribution_krw)
    .bind(layers.deferred_retirement_income_krw)
    .bind(layers.credited_contribution_krw)
    .bind(layers.earnings_krw)
    .bind(layers.tax_excluded_contribution_krw)
    .bind(layers.deferred_retirement_income_krw)
    .bind(layers.credited_contribution_krw)
    .bind(layers.earnings_krw)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn apply_pension_trade_basis_adjustment_in_tx(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedAssetSaveRow,
    account_id: ResourceId,
    command_id: &CommandId,
    side: AssetOrderSide,
    execution_market_value_krw: i64,
    risk_asset: bool,
) -> Result<()> {
    let state: Option<(i64, i64)> = sqlx::query_as(
        "SELECT position_market_value_krw, risk_asset_value_krw
         FROM pension_valuation_state
         WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
         FOR UPDATE",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(account_id.get())
    .fetch_optional(&mut **tx)
    .await?;
    let (position_before, risk_before) = state.unwrap_or((0, 0));
    let adjustment = adjust_pension_valuation_basis(PensionValuationBasisInput {
        basis_before_krw: position_before,
        side: match side {
            AssetOrderSide::Buy => PensionTradeSide::Buy,
            AssetOrderSide::Sell => PensionTradeSide::Sell,
        },
        execution_market_value_krw,
    })?;
    let risk_after = if risk_asset {
        adjust_pension_valuation_basis(PensionValuationBasisInput {
            basis_before_krw: risk_before,
            side: match side {
                AssetOrderSide::Buy => PensionTradeSide::Buy,
                AssetOrderSide::Sell => PensionTradeSide::Sell,
            },
            execution_market_value_krw,
        })?
        .basis_after_krw
    } else {
        risk_before
    };
    let layers = lock_pension_layers(tx, current.id, current.run_revision, account_id)
        .await?
        .context("active pension tax layers are missing")?;
    apply_explicit_pension_trade_basis_in_tx(
        tx,
        PensionTradeBasisWrite {
            save_id: current.id,
            run_revision: current.run_revision,
            game_day: current.game_day,
            account_id,
            source_kind: "assetOrder",
            source_id: command_id.as_str(),
            position_market_value_before_krw: position_before,
            position_market_value_after_krw: adjustment.basis_after_krw,
            risk_asset_value_after_krw: risk_after,
            account_total_value_krw: pension_layers_total(layers)?,
        },
    )
    .await
}

async fn lock_pension_layers(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    account_id: ResourceId,
) -> Result<Option<PensionTaxLayers>> {
    let row: Option<(i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT balance.tax_excluded_contribution_krw,
                balance.deferred_retirement_income_krw,
                balance.credited_contribution_krw, balance.earnings_krw
         FROM pension_tax_balance AS balance
         INNER JOIN pension_account_contract AS contract
           ON contract.save_id = balance.save_id
          AND contract.run_revision = balance.run_revision
          AND contract.financial_account_id = balance.financial_account_id
         WHERE balance.save_id = ? AND balance.run_revision = ?
           AND balance.financial_account_id = ? AND contract.status = 'active'
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(account_id.get())
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(
        |(tax_excluded, deferred, credited, earnings)| PensionTaxLayers {
            tax_excluded_contribution_krw: tax_excluded,
            deferred_retirement_income_krw: deferred,
            credited_contribution_krw: credited,
            earnings_krw: earnings,
        },
    ))
}

fn pension_layers_total(layers: PensionTaxLayers) -> Result<i64> {
    i128::from(layers.tax_excluded_contribution_krw)
        .checked_add(i128::from(layers.deferred_retirement_income_krw))
        .and_then(|value| value.checked_add(i128::from(layers.credited_contribution_krw)))
        .and_then(|value| value.checked_add(i128::from(layers.earnings_krw)))
        .and_then(|value| i64::try_from(value).ok())
        .context("pension tax layer total overflowed")
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredPensionPolicy {
    pension_savings_credit_limit_krw: i64,
    combined_credit_limit_krw: i64,
    salary_high_credit_boundary_krw: i64,
    comprehensive_income_high_credit_boundary_krw: i64,
    high_income_tax_credit_rate_ppm: i64,
    high_local_income_tax_credit_rate_ppm: i64,
    standard_income_tax_credit_rate_ppm: i64,
    standard_local_income_tax_credit_rate_ppm: i64,
    minimum_pension_age: u8,
    minimum_enrollment_years: u8,
    irp_risk_asset_limit_ppm: i64,
    under_age70_pension_tax_ppm: i64,
    under_age80_pension_tax_ppm: i64,
    age80_or_older_pension_tax_ppm: i64,
    lifetime_pension_tax_ppm: i64,
    non_pension_withdrawal_tax_ppm: i64,
    pension_receipt_limit_rate_ppm: i64,
    limited_receipt_years: u8,
    deferred_retirement_first10_years_ppm: i64,
    deferred_retirement_years11_to20_ppm: i64,
    deferred_retirement_after20_years_ppm: i64,
}

async fn read_pension_risk_policy(
    tx: &mut Transaction<'_, MySql>,
    policy_set_id: u64,
    market_world_id: u64,
    game_day: u32,
) -> Result<StoredPensionPolicy> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT CAST(rule.parameters AS CHAR)
         FROM policy_rule AS rule
         INNER JOIN market_daily AS daily
           ON daily.world_id = ? AND daily.game_day = ?
         WHERE rule.policy_set_id = ? AND rule.domain = 'pension'
           AND rule.rule_key = 'contributionAndWithdrawal'
           AND rule.effective_from <= daily.market_date
           AND (rule.effective_to IS NULL OR rule.effective_to >= daily.market_date)
         ORDER BY rule.effective_from DESC LIMIT 2",
    )
    .bind(market_world_id)
    .bind(game_day)
    .bind(policy_set_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(rows.len() == 1, "pension policy is missing or overlapping");
    let policy: StoredPensionPolicy = serde_json::from_str(&rows[0].0)
        .context("pension policy does not match its strict schema")?;
    validate_stored_pension_policy(&policy)?;
    Ok(policy)
}

fn validate_stored_pension_policy(policy: &StoredPensionPolicy) -> Result<()> {
    let non_negative_amounts = [
        policy.pension_savings_credit_limit_krw,
        policy.combined_credit_limit_krw,
        policy.salary_high_credit_boundary_krw,
        policy.comprehensive_income_high_credit_boundary_krw,
    ];
    let rates = [
        policy.high_income_tax_credit_rate_ppm,
        policy.high_local_income_tax_credit_rate_ppm,
        policy.standard_income_tax_credit_rate_ppm,
        policy.standard_local_income_tax_credit_rate_ppm,
        policy.irp_risk_asset_limit_ppm,
        policy.under_age70_pension_tax_ppm,
        policy.under_age80_pension_tax_ppm,
        policy.age80_or_older_pension_tax_ppm,
        policy.lifetime_pension_tax_ppm,
        policy.non_pension_withdrawal_tax_ppm,
        policy.deferred_retirement_first10_years_ppm,
        policy.deferred_retirement_years11_to20_ppm,
        policy.deferred_retirement_after20_years_ppm,
    ];
    ensure!(
        non_negative_amounts.into_iter().all(|value| value >= 0)
            && rates
                .into_iter()
                .all(|value| (0..=1_000_000).contains(&value))
            && policy.combined_credit_limit_krw >= policy.pension_savings_credit_limit_krw
            && policy.minimum_pension_age > 0
            && policy.minimum_enrollment_years > 0
            && policy.pension_receipt_limit_rate_ppm >= 1_000_000
            && policy.limited_receipt_years > 0,
        "pension policy contains unsupported values"
    );
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PensionAccountMarketValue {
    pub account_id: ResourceId,
    pub position_market_value_krw: i64,
    pub risk_asset_value_krw: i64,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PensionMarkToMarketPlan {
    pub account_id: ResourceId,
    pub risk_asset_value_after_krw: i64,
    pub draft: PensionValueEventDraft,
}

/// Plans daily pension valuation events without changing state; the caller may apply the
/// returned plans after all daily asset prices have been computed successfully.
pub(super) async fn plan_pension_mark_to_market_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
    values: &[PensionAccountMarketValue],
) -> Result<Vec<PensionMarkToMarketPlan>> {
    let mut plans = Vec::with_capacity(values.len());
    for value in values {
        ensure!(
            value.position_market_value_krw >= 0
                && value.risk_asset_value_krw >= 0
                && value.risk_asset_value_krw <= value.position_market_value_krw,
            "pension market value input is invalid"
        );
        let layers = lock_pension_layers(tx, save_id, run_revision, value.account_id)
            .await?
            .context("pension tax layers are missing during mark-to-market")?;
        let account_total_before_krw = pension_layers_total(layers)?;
        let state: Option<(u32, i64, i64)> = sqlx::query_as(
            "SELECT last_valuation_game_day, position_market_value_krw, risk_asset_value_krw
             FROM pension_valuation_state
             WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
             FOR UPDATE",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(value.account_id.get())
        .fetch_optional(&mut **tx)
        .await?;
        let position_before = match state {
            Some((last_day, position_before, risk_before)) => {
                ensure!(
                    last_day < game_day && risk_before >= 0 && risk_before <= position_before,
                    "pension valuation state is not ready for the next day"
                );
                position_before
            }
            None => 0,
        };
        let value_change = value
            .position_market_value_krw
            .checked_sub(position_before)
            .context("pension market value change overflowed")?;
        let account_total_after_krw = account_total_before_krw
            .checked_add(value_change)
            .context("pension account total overflowed during mark-to-market")?;
        if account_total_after_krw < 0 {
            bail!("pension market loss exceeds the account tax-layer balance");
        }
        let draft = draft_pension_mark_to_market_event(PensionMarkToMarketInput {
            position_market_value_before_krw: position_before,
            position_market_value_after_krw: value.position_market_value_krw,
            account_total_before_krw,
            account_total_after_krw,
            layers_before: layers,
        })?;
        plans.push(PensionMarkToMarketPlan {
            account_id: value.account_id,
            risk_asset_value_after_krw: value.risk_asset_value_krw,
            draft,
        });
    }
    Ok(plans)
}

pub(super) async fn apply_pension_mark_to_market_plans_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
    plans: &[PensionMarkToMarketPlan],
) -> Result<()> {
    for plan in plans {
        ensure!(
            plan.risk_asset_value_after_krw >= 0
                && plan.risk_asset_value_after_krw <= plan.draft.position_market_value_after_krw,
            "pension risk asset value exceeds total positions"
        );
        sqlx::query(
            "INSERT INTO pension_valuation_state
                 (save_id, run_revision, financial_account_id, last_valuation_game_day,
                  position_market_value_krw, risk_asset_value_krw)
             VALUES (?, ?, ?, ?, ?, ?)
             ON DUPLICATE KEY UPDATE
                 last_valuation_game_day = VALUES(last_valuation_game_day),
                 position_market_value_krw = VALUES(position_market_value_krw),
                 risk_asset_value_krw = VALUES(risk_asset_value_krw)",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(plan.account_id.get())
        .bind(game_day)
        .bind(plan.draft.position_market_value_after_krw)
        .bind(plan.risk_asset_value_after_krw)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "UPDATE pension_tax_balance
             SET tax_excluded_contribution_krw = ?,
                 deferred_retirement_income_krw = ?,
                 credited_contribution_krw = ?, earnings_krw = ?
             WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?",
        )
        .bind(plan.draft.layers_after.tax_excluded_contribution_krw)
        .bind(plan.draft.layers_after.deferred_retirement_income_krw)
        .bind(plan.draft.layers_after.credited_contribution_krw)
        .bind(plan.draft.layers_after.earnings_krw)
        .bind(save_id)
        .bind(run_revision)
        .bind(plan.account_id.get())
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT INTO tax_account_value_event
                 (save_id, run_revision, financial_account_id, event_game_day, cause,
                  source_kind, source_id, occurrence,
                  position_market_value_before_krw, position_market_value_after_krw,
                  account_total_before_krw, account_total_after_krw, value_change_krw,
                  before_tax_excluded_krw, before_deferred_retirement_krw,
                  before_credited_contribution_krw, before_earnings_krw,
                  after_tax_excluded_krw, after_deferred_retirement_krw,
                  after_credited_contribution_krw, after_earnings_krw)
             VALUES (?, ?, ?, ?, 'dailyMarketToMarket', 'gameDay', ?, 1,
                     ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(plan.account_id.get())
        .bind(game_day)
        .bind(game_day.to_string())
        .bind(plan.draft.position_market_value_before_krw)
        .bind(plan.draft.position_market_value_after_krw)
        .bind(plan.draft.account_total_before_krw)
        .bind(plan.draft.account_total_after_krw)
        .bind(plan.draft.value_change_krw)
        .bind(plan.draft.layers_before.tax_excluded_contribution_krw)
        .bind(plan.draft.layers_before.deferred_retirement_income_krw)
        .bind(plan.draft.layers_before.credited_contribution_krw)
        .bind(plan.draft.layers_before.earnings_krw)
        .bind(plan.draft.layers_after.tax_excluded_contribution_krw)
        .bind(plan.draft.layers_after.deferred_retirement_income_krw)
        .bind(plan.draft.layers_after.credited_contribution_krw)
        .bind(plan.draft.layers_after.earnings_krw)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn read_asset_save_for_user(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
) -> Result<Option<LockedAssetSaveRow>> {
    sqlx::query_as(
        "SELECT save.id, save.market_world_id, save.policy_set_id,
                save.market_world_product_bundle_id, save.run_revision,
                save.state_revision, save.game_day,
                EXISTS(SELECT 1 FROM `character` WHERE `character`.save_id = save.id)
                    AS has_character
         FROM save WHERE save.user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read the asset save")
}

async fn lock_asset_save_for_user(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
) -> Result<Option<LockedAssetSaveRow>> {
    sqlx::query_as(
        "SELECT save.id, save.market_world_id, save.policy_set_id,
                save.market_world_product_bundle_id, save.run_revision,
                save.state_revision, save.game_day,
                EXISTS(SELECT 1 FROM `character` WHERE `character`.save_id = save.id)
                    AS has_character
         FROM save WHERE save.user_id = ? FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock the asset command save")
}

async fn read_bundle_catalog(
    tx: &mut Transaction<'_, MySql>,
    market_world_id: u64,
    bundle_id: Option<u64>,
) -> Result<Option<M2dBundleCatalog>> {
    let Some(bundle_id) = bundle_id else {
        return Ok(None);
    };
    let row: Option<BundleCatalogRow> = sqlx::query_as(
        "SELECT calibration.version AS market_version,
                index_product.id AS index_id,
                index_product.product_key AS index_key,
                index_product.display_name AS index_display_name,
                index_product.annual_management_fee_ppm AS index_annual_management_fee_ppm,
                index_product.annual_distribution_rate_ppm AS index_annual_distribution_rate_ppm,
                index_product.day_count_denominator AS index_day_count_denominator,
                index_product.buy_fee_ppm AS index_buy_fee_ppm,
                index_product.sell_fee_ppm AS index_sell_fee_ppm,
                index_product.transaction_tax_ppm AS index_sell_tax_ppm,
                bond_3y.id AS bond_3y_id, bond_3y.product_key AS bond_3y_key,
                bond_3y.display_name AS bond_3y_display_name,
                bond_3y.term_years AS bond_3y_term_years,
                bond_3y.face_value_krw AS bond_3y_face_value_krw,
                bond_3y.max_order_units AS bond_3y_max_order_units,
                bond_3y.max_position_units AS bond_3y_max_position_units,
                bond_3y.buy_fee_ppm AS bond_3y_buy_fee_ppm,
                bond_3y.sell_fee_ppm AS bond_3y_sell_fee_ppm,
                bond_10y.id AS bond_10y_id, bond_10y.product_key AS bond_10y_key,
                bond_10y.display_name AS bond_10y_display_name,
                bond_10y.term_years AS bond_10y_term_years,
                bond_10y.face_value_krw AS bond_10y_face_value_krw,
                bond_10y.max_order_units AS bond_10y_max_order_units,
                bond_10y.max_position_units AS bond_10y_max_position_units,
                bond_10y.buy_fee_ppm AS bond_10y_buy_fee_ppm,
                bond_10y.sell_fee_ppm AS bond_10y_sell_fee_ppm,
                gold.id AS gold_id, gold.product_key AS gold_key,
                gold.display_name AS gold_display_name, gold.unit AS gold_unit,
                gold.buy_fee_ppm AS gold_buy_fee_ppm,
                gold.sell_fee_ppm AS gold_sell_fee_ppm,
                gold.buy_tax_ppm AS gold_buy_tax_ppm,
                gold.sell_tax_ppm AS gold_sell_tax_ppm,
                gold.withdrawal_100g_fee_krw AS gold_withdrawal_100g_fee_krw,
                gold.withdrawal_1000g_fee_krw AS gold_withdrawal_1000g_fee_krw
         FROM market_world_product_bundle AS bundle
         INNER JOIN market_world AS world ON world.id = bundle.market_world_id
         INNER JOIN market_calibration AS calibration ON calibration.id = world.calibration_id
         INNER JOIN index_product_version AS index_product
             ON index_product.id = bundle.index_product_version_id
         INNER JOIN bond_product_version AS bond_3y
             ON bond_3y.id = bundle.bond_3y_product_version_id
         INNER JOIN bond_product_version AS bond_10y
             ON bond_10y.id = bundle.bond_10y_product_version_id
         INNER JOIN gold_product_version AS gold
             ON gold.id = bundle.gold_product_version_id
         WHERE bundle.id = ? AND bundle.market_world_id = ?
           AND bundle.published_at IS NOT NULL
           AND index_product.published_at IS NOT NULL
           AND bond_3y.published_at IS NOT NULL
           AND bond_10y.published_at IS NOT NULL
           AND gold.published_at IS NOT NULL",
    )
    .bind(bundle_id)
    .bind(market_world_id)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(BundleCatalogRow::into_catalog).transpose()
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CurrentMarketRow {
    market_version: String,
    market_date: Date,
    market_open: bool,
    treasury_3y_bp: Option<i32>,
    treasury_10y_bp: Option<i32>,
    gold_close_krw_per_gram: Option<i64>,
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
struct BondIssueDayRow {
    market_date: Date,
    market_open: bool,
    treasury_3y_bp: Option<i32>,
    treasury_10y_bp: Option<i32>,
    first_open_of_month: bool,
}

async fn read_current_market(
    tx: &mut Transaction<'_, MySql>,
    market_world_id: u64,
    game_day: u32,
) -> Result<CurrentMarketRow> {
    sqlx::query_as(
        "SELECT calibration.version AS market_version, daily.market_date,
                daily.market_open, daily.treasury_3y_bp, daily.treasury_10y_bp,
                daily.gold_close_krw_per_gram
         FROM market_daily AS daily
         INNER JOIN market_world AS world ON world.id = daily.world_id
         INNER JOIN market_calibration AS calibration ON calibration.id = world.calibration_id
         WHERE daily.world_id = ? AND daily.game_day = ?",
    )
    .bind(market_world_id)
    .bind(game_day)
    .fetch_one(&mut **tx)
    .await
    .context("current M2-D market row is missing")
}

pub(super) async fn ensure_monthly_bond_series_in_tx(
    tx: &mut Transaction<'_, MySql>,
    market_world_id: u64,
    game_day: u32,
) -> Result<()> {
    let row: Option<BondIssueDayRow> = sqlx::query_as(
        "SELECT daily.market_date, daily.market_open, daily.treasury_3y_bp,
                daily.treasury_10y_bp,
                NOT EXISTS(
                    SELECT 1 FROM market_daily AS prior
                    WHERE prior.world_id = daily.world_id
                      AND prior.market_open = TRUE
                      AND YEAR(prior.market_date) = YEAR(daily.market_date)
                      AND MONTH(prior.market_date) = MONTH(daily.market_date)
                      AND prior.game_day < daily.game_day
                ) AS first_open_of_month
         FROM market_daily AS daily
         WHERE daily.world_id = ? AND daily.game_day = ?",
    )
    .bind(market_world_id)
    .bind(game_day)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(BondIssueDayRow {
        market_date,
        market_open,
        treasury_3y_bp: yield_3y,
        treasury_10y_bp: yield_10y,
        first_open_of_month: first_open,
    }) = row
    else {
        bail!("cannot issue bonds without the requested market day");
    };
    if !market_open || !first_open {
        return Ok(());
    }

    let bundle_id: Option<(u64,)> = sqlx::query_as(
        "SELECT id FROM market_world_product_bundle
         WHERE market_world_id = ? AND published_at IS NOT NULL",
    )
    .bind(market_world_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((bundle_id,)) = bundle_id else {
        return Ok(());
    };
    let bundle = read_bundle_catalog(tx, market_world_id, Some(bundle_id))
        .await?
        .context("published market bundle cannot be loaded")?;
    let issue_yields = [
        yield_3y.context("3-year yield is missing on an M2-D issue day")?,
        yield_10y.context("10-year yield is missing on an M2-D issue day")?,
    ];
    for (product, issue_yield_bp) in bundle.bond_products.iter().zip(issue_yields) {
        let series = create_bond_series(product.terms, market_date, issue_yield_bp)
            .context("stored market inputs cannot create a bond series")?;
        sqlx::query(
            "INSERT IGNORE INTO bond_series
                 (market_world_id, product_version_id, issued_date, maturity_date,
                  coupon_rate_bp, issue_yield_bp)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(market_world_id)
        .bind(product.id.get())
        .bind(series.issue_date)
        .bind(series.maturity_date)
        .bind(series.coupon_rate_bp)
        .bind(series.issue_yield_bp)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub(super) struct M2dDailyAssetContext {
    pub save_id: u64,
    pub market_world_id: u64,
    pub policy_set_id: u64,
    pub market_world_product_bundle_id: Option<u64>,
    pub run_revision: u32,
    pub game_day: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BondCashFlowSettlementResult {
    Applied { ledger_transaction_id: u64 },
    NoMovement,
    AlreadyFinalized,
}

#[derive(Debug, Clone, Copy, Deserialize)]
enum BondSettlementPayloadVersion {
    #[serde(rename = "v1")]
    V1,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BondCashFlowSettlementPayload {
    version: BondSettlementPayloadVersion,
    account_id: ResourceId,
    series_id: ResourceId,
    product_version_id: ResourceId,
    coupon_krw_per_unit: i64,
    principal_krw_per_unit: i64,
    payment_date: CanonicalDate,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedBondSettlementRow {
    id: u64,
    due_game_day: u32,
    kind: String,
    payload_json: String,
    source_kind: String,
    source_id: String,
    occurrence: u32,
    status: String,
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
struct LockedBondSettlementPositionRow {
    product_version_id: u64,
    bond_units: u32,
    total_cost_basis_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BondSettlementAccountKind {
    Taxable,
    Isa,
    Pension,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BondCashFlowAmountPlan {
    gross_coupon_krw: i64,
    gross_principal_krw: i64,
    removed_cost_basis_krw: i64,
    principal_realized_gain_loss_krw: i64,
    income_tax_krw: i64,
    local_income_tax_krw: i64,
    account_credit_krw: i64,
    isa_tax_profit_delta_krw: i64,
    isa_deductible_loss_delta_krw: i64,
    pension_earnings_delta_krw: i64,
}

impl BondCashFlowAmountPlan {
    const fn moves_money(self) -> bool {
        self.account_credit_krw > 0
    }
}

fn parse_bond_cash_flow_payload(
    kind: &str,
    payload_json: &str,
) -> Result<BondCashFlowSettlementPayload> {
    let payload: BondCashFlowSettlementPayload = serde_json::from_str(payload_json)
        .context("bond settlement payload has an invalid schema")?;
    let _version = payload.version;
    ensure!(
        payload.coupon_krw_per_unit >= 0 && payload.principal_krw_per_unit >= 0,
        "bond settlement payload has negative money"
    );
    match kind {
        "bondCoupon" => ensure!(
            payload.principal_krw_per_unit == 0,
            "bond coupon payload includes principal"
        ),
        "bondMaturity" => ensure!(
            payload.principal_krw_per_unit > 0,
            "bond maturity payload has no principal"
        ),
        _ => bail!("settlement is not a bond cash flow"),
    }
    Ok(payload)
}

fn bond_settlement_account_kind(account_type: &str) -> Result<BondSettlementAccountKind> {
    match account_type {
        "taxableBrokerage" => Ok(BondSettlementAccountKind::Taxable),
        "isaGeneral" | "isaLowIncome" => Ok(BondSettlementAccountKind::Isa),
        "pensionSavings" | "irp" => Ok(BondSettlementAccountKind::Pension),
        _ => bail!("bond settlement uses a forbidden account type"),
    }
}

fn plan_bond_cash_flow_amounts(
    account_kind: BondSettlementAccountKind,
    bond_units: u32,
    total_cost_basis_krw: i64,
    coupon_krw_per_unit: i64,
    principal_krw_per_unit: i64,
    income_tax_ppm: i64,
    local_income_tax_ppm: i64,
) -> Result<BondCashFlowAmountPlan> {
    ensure!(
        coupon_krw_per_unit >= 0 && principal_krw_per_unit >= 0 && total_cost_basis_krw >= 0,
        "bond cash-flow inputs contain negative money"
    );
    ensure!(
        (bond_units == 0 && total_cost_basis_krw == 0)
            || (bond_units > 0 && total_cost_basis_krw > 0),
        "bond position and cost basis disagree"
    );
    ensure!(
        (0..=1_000_000).contains(&income_tax_ppm)
            && (0..=1_000_000).contains(&local_income_tax_ppm),
        "bond withholding rate is invalid"
    );
    ensure!(
        account_kind == BondSettlementAccountKind::Taxable
            || (income_tax_ppm == 0 && local_income_tax_ppm == 0),
        "a tax-advantaged bond account cannot withhold immediately"
    );

    let gross_coupon_krw = checked_non_negative_product_i64(coupon_krw_per_unit, bond_units)?;
    let gross_principal_krw = checked_non_negative_product_i64(principal_krw_per_unit, bond_units)?;
    let removed_cost_basis_krw = if principal_krw_per_unit > 0 {
        total_cost_basis_krw
    } else {
        0
    };
    let principal_realized_gain_loss_krw = gross_principal_krw
        .checked_sub(removed_cost_basis_krw)
        .context("bond maturity realized gain or loss overflowed")?;
    let (income_tax_krw, local_income_tax_krw) =
        if account_kind == BondSettlementAccountKind::Taxable {
            (
                floor_rate(gross_coupon_krw, income_tax_ppm)?,
                floor_rate(gross_coupon_krw, local_income_tax_ppm)?,
            )
        } else {
            (0, 0)
        };
    let withholding_tax_krw = income_tax_krw
        .checked_add(local_income_tax_krw)
        .context("bond coupon withholding overflowed")?;
    ensure!(
        withholding_tax_krw <= gross_coupon_krw,
        "bond coupon withholding exceeds gross income"
    );
    let account_credit_krw = gross_coupon_krw
        .checked_add(gross_principal_krw)
        .and_then(|amount| amount.checked_sub(withholding_tax_krw))
        .context("bond settlement account credit overflowed")?;
    let (isa_tax_profit_delta_krw, isa_deductible_loss_delta_krw) =
        if account_kind == BondSettlementAccountKind::Isa {
            let maturity_profit = principal_realized_gain_loss_krw.max(0);
            let maturity_loss = principal_realized_gain_loss_krw
                .min(0)
                .checked_neg()
                .context("bond maturity deductible loss overflowed")?;
            (
                gross_coupon_krw
                    .checked_add(maturity_profit)
                    .context("ISA bond profit overflowed")?,
                maturity_loss,
            )
        } else {
            (0, 0)
        };
    let pension_earnings_delta_krw = if account_kind == BondSettlementAccountKind::Pension {
        gross_coupon_krw
    } else {
        0
    };

    Ok(BondCashFlowAmountPlan {
        gross_coupon_krw,
        gross_principal_krw,
        removed_cost_basis_krw,
        principal_realized_gain_loss_krw,
        income_tax_krw,
        local_income_tax_krw,
        account_credit_krw,
        isa_tax_profit_delta_krw,
        isa_deductible_loss_delta_krw,
        pension_earnings_delta_krw,
    })
}

fn checked_non_negative_product_i64(unit_amount_krw: i64, quantity: u32) -> Result<i64> {
    ensure!(unit_amount_krw >= 0, "unit amount cannot be negative");
    i128::from(unit_amount_krw)
        .checked_mul(i128::from(quantity))
        .and_then(|value| i64::try_from(value).ok())
        .context("bond cash-flow amount overflowed")
}

fn create_bond_cash_flow_ledger(
    rules: &dyn crate::finance::FinanceRules,
    context: M2dDailyAssetContext,
    settlement_id: u64,
    account_id: ResourceId,
    kind: &str,
    plan: BondCashFlowAmountPlan,
) -> Result<crate::finance::LedgerTransaction> {
    ensure!(
        plan.moves_money(),
        "zero bond cash flow cannot create a ledger"
    );
    let mut postings = vec![LedgerPosting {
        account_code: LedgerAccountCode::AccountCash,
        financial_account_id: Some(account_id),
        amount_krw: plan.account_credit_krw,
    }];
    if plan.removed_cost_basis_krw > 0 {
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::ProductPrincipal,
            financial_account_id: Some(account_id),
            amount_krw: plan
                .removed_cost_basis_krw
                .checked_neg()
                .context("bond maturity cost basis cannot be negated")?,
        });
    }
    if plan.gross_coupon_krw > 0 {
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::DistributionIncome,
            financial_account_id: None,
            amount_krw: plan
                .gross_coupon_krw
                .checked_neg()
                .context("bond coupon cannot be negated")?,
        });
    }
    let withholding_tax_krw = plan
        .income_tax_krw
        .checked_add(plan.local_income_tax_krw)
        .context("bond withholding total overflowed")?;
    if withholding_tax_krw > 0 {
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::WithholdingTaxLiability,
            financial_account_id: None,
            amount_krw: withholding_tax_krw,
        });
    }
    if plan.principal_realized_gain_loss_krw != 0 {
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::RealizedGainLoss,
            financial_account_id: None,
            amount_krw: plan
                .principal_realized_gain_loss_krw
                .checked_neg()
                .context("bond maturity realized result cannot be negated")?,
        });
    }
    rules
        .create_ledger_transaction(LedgerTransactionDraft {
            policy: RunPolicyContext {
                run: RunId {
                    save_id: resource_id(context.save_id, "save")?,
                    run_revision: context.run_revision,
                },
                policy_set_id: resource_id(context.policy_set_id, "policy set")?,
            },
            source: LedgerSource {
                kind: LedgerSourceKind::ScheduledSettlement,
                source_id: settlement_id.to_string(),
            },
            game_day: context.game_day,
            description: match kind {
                "bondCoupon" => "국채 쿠폰 지급",
                "bondMaturity" => "국채 만기 상환",
                _ => bail!("unsupported bond settlement kind"),
            }
            .to_owned(),
            postings,
        })
        .map_err(Into::into)
}

async fn validate_bond_cash_flow_identity_in_tx(
    tx: &mut Transaction<'_, MySql>,
    context: M2dDailyAssetContext,
    settlement: &LockedBondSettlementRow,
    payload: BondCashFlowSettlementPayload,
) -> Result<()> {
    ensure!(settlement.id != 0, "bond settlement ID is invalid");
    let bundle = read_bundle_catalog(
        tx,
        context.market_world_id,
        context.market_world_product_bundle_id,
    )
    .await?
    .context("bond settlement run has no pinned product bundle")?;
    let series_row: BondSeriesRow = sqlx::query_as(
        "SELECT id, product_version_id, issued_date, maturity_date,
                coupon_rate_bp, issue_yield_bp
         FROM bond_series WHERE market_world_id = ? AND id = ?",
    )
    .bind(context.market_world_id)
    .bind(payload.series_id.get())
    .fetch_optional(&mut **tx)
    .await?
    .context("bond settlement series does not exist in the current world")?;
    ensure!(
        series_row.product_version_id == payload.product_version_id.get(),
        "bond settlement product does not match its series"
    );
    let product = bundle
        .bond_products
        .iter()
        .find(|product| product.id == payload.product_version_id)
        .context("bond settlement product is outside the pinned bundle")?;
    let series = create_bond_series(
        product.terms,
        series_row.issued_date,
        series_row.issue_yield_bp,
    )?;
    ensure!(
        series.maturity_date == series_row.maturity_date
            && series.coupon_rate_bp == series_row.coupon_rate_bp,
        "bond settlement series disagrees with immutable product terms"
    );
    let cash_flow_index = settlement
        .occurrence
        .checked_sub(1)
        .and_then(|value| usize::try_from(value).ok())
        .context("bond settlement occurrence is invalid")?;
    let cash_flow = series
        .cash_flows
        .get(cash_flow_index)
        .context("bond settlement occurrence exceeds its series")?;
    let expected_kind = if cash_flow.principal_krw > 0 {
        "bondMaturity"
    } else {
        "bondCoupon"
    };
    let (world_start_date,): (Date,) =
        sqlx::query_as("SELECT start_date FROM market_world WHERE id = ?")
            .bind(context.market_world_id)
            .fetch_one(&mut **tx)
            .await?;
    let expected_due_game_day =
        u32::try_from((cash_flow.payment_date - world_start_date).whole_days())
            .context("bond settlement payment date is outside its world")?;
    ensure!(
        settlement.kind == expected_kind
            && settlement.due_game_day == expected_due_game_day
            && payload.payment_date.as_date() == cash_flow.payment_date
            && payload.coupon_krw_per_unit == cash_flow.coupon_krw
            && payload.principal_krw_per_unit == cash_flow.principal_krw,
        "bond settlement payload disagrees with its immutable series cash flow"
    );
    let expected_source_id = format!("{}:{}", payload.account_id, payload.series_id);
    ensure!(
        settlement.source_kind == "bondPosition" && settlement.source_id == expected_source_id,
        "bond settlement source identity is invalid"
    );
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PensionBondDuePrincipal {
    pub account_id: ResourceId,
    pub due_principal_krw: i64,
}

/// Returns principal due today by pension account. Add each amount to that account's
/// ex-cash-flow position value before planning the same-day pension mark-to-market.
pub(super) async fn read_due_pension_bond_principal_in_tx(
    tx: &mut Transaction<'_, MySql>,
    context: M2dDailyAssetContext,
) -> Result<Vec<PensionBondDuePrincipal>> {
    let target_market_date: Option<(Date,)> =
        sqlx::query_as("SELECT market_date FROM market_daily WHERE world_id = ? AND game_day = ?")
            .bind(context.market_world_id)
            .bind(context.game_day)
            .fetch_optional(&mut **tx)
            .await?;
    ensure!(
        target_market_date.is_some(),
        "pension due-principal target market day is missing"
    );
    let settlements: Vec<LockedBondSettlementRow> = sqlx::query_as(
        "SELECT id, due_game_day, kind, CAST(payload AS CHAR) AS payload_json,
                source_kind, source_id, occurrence, status
         FROM scheduled_settlement
         WHERE save_id = ? AND run_revision = ? AND status = 'pending'
           AND kind = 'bondMaturity' AND due_game_day <= ?
         ORDER BY due_game_day, id",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(context.game_day)
    .fetch_all(&mut **tx)
    .await?;
    let mut due_by_account = BTreeMap::<u64, i64>::new();
    for settlement in settlements {
        let payload = parse_bond_cash_flow_payload(&settlement.kind, &settlement.payload_json)?;
        validate_bond_cash_flow_identity_in_tx(tx, context, &settlement, payload).await?;
        let account_position: (String, String, u64, u32, i64) = sqlx::query_as(
            "SELECT account.account_type, account.status, position.product_version_id,
                    position.bond_units, position.total_cost_basis_krw
             FROM save
             INNER JOIN financial_account AS account
               ON account.save_id = save.id AND account.run_revision = save.run_revision
             INNER JOIN bond_position AS position
               ON position.save_id = account.save_id
              AND position.run_revision = account.run_revision
              AND position.financial_account_id = account.id
             WHERE save.id = ? AND save.run_revision = ? AND save.market_world_id = ?
               AND save.policy_set_id = ? AND save.market_world_product_bundle_id <=> ?
               AND account.id = ?
               AND position.market_world_id = ? AND position.series_id = ?",
        )
        .bind(context.save_id)
        .bind(context.run_revision)
        .bind(context.market_world_id)
        .bind(context.policy_set_id)
        .bind(context.market_world_product_bundle_id)
        .bind(payload.account_id.get())
        .bind(context.market_world_id)
        .bind(payload.series_id.get())
        .fetch_optional(&mut **tx)
        .await?
        .context("due bond maturity position does not belong to the current run")?;
        let (account_type, account_status, product_version_id, bond_units, cost_basis) =
            account_position;
        ensure!(
            account_status == "open"
                && product_version_id == payload.product_version_id.get()
                && ((bond_units == 0 && cost_basis == 0) || (bond_units > 0 && cost_basis > 0)),
            "due bond maturity position is invalid"
        );
        let account_kind = bond_settlement_account_kind(&account_type)?;
        if account_kind != BondSettlementAccountKind::Pension {
            continue;
        }
        let due_principal =
            checked_non_negative_product_i64(payload.principal_krw_per_unit, bond_units)?;
        let entry = due_by_account.entry(payload.account_id.get()).or_default();
        *entry = entry
            .checked_add(due_principal)
            .context("pension due bond principal overflowed")?;
    }
    due_by_account
        .into_iter()
        .map(|(account_id, due_principal_krw)| {
            Ok(PensionBondDuePrincipal {
                account_id: resource_id(account_id, "financial account")?,
                due_principal_krw,
            })
        })
        .collect()
}

/// Settles one bond cash-flow row in the caller's transaction. The caller must invoke this
/// after daily pension mark-to-market and in global `(due_game_day, settlement_id)` order.
pub(super) async fn settle_bond_cash_flow_by_id_in_tx(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn crate::finance::FinanceRules,
    context: M2dDailyAssetContext,
    market_date: Date,
    settlement_id: u64,
) -> Result<BondCashFlowSettlementResult> {
    let settlement: LockedBondSettlementRow = sqlx::query_as(
        "SELECT id, due_game_day, kind, CAST(payload AS CHAR) AS payload_json,
                source_kind, source_id, occurrence, status
         FROM scheduled_settlement
         WHERE id = ? AND save_id = ? AND run_revision = ? FOR UPDATE",
    )
    .bind(settlement_id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .fetch_optional(&mut **tx)
    .await?
    .context("bond settlement does not belong to the current run")?;
    ensure!(
        settlement.source_kind == "bondPosition",
        "bond settlement has an invalid source kind"
    );
    let payload = parse_bond_cash_flow_payload(&settlement.kind, &settlement.payload_json)?;
    ensure!(
        settlement.status == "pending"
            || settlement.status == "settled"
            || settlement.status == "cancelled",
        "bond settlement has an invalid status"
    );
    if settlement.status != "pending" {
        return Ok(BondCashFlowSettlementResult::AlreadyFinalized);
    }
    ensure!(
        settlement.due_game_day <= context.game_day,
        "bond settlement is not due"
    );

    let stored_market_date: Date = sqlx::query_scalar(
        "SELECT market_date FROM market_daily WHERE world_id = ? AND game_day = ?",
    )
    .bind(context.market_world_id)
    .bind(context.game_day)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        stored_market_date == market_date,
        "bond settlement market date disagrees with its run context"
    );
    validate_bond_cash_flow_identity_in_tx(tx, context, &settlement, payload).await?;

    let account: LockedBondAccountRow = sqlx::query_as(
        "SELECT account.account_type, account.status, account.cash_krw
         FROM save
         INNER JOIN financial_account AS account
           ON account.save_id = save.id AND account.run_revision = save.run_revision
         WHERE save.id = ? AND save.run_revision = ? AND save.market_world_id = ?
           AND save.policy_set_id = ? AND save.market_world_product_bundle_id <=> ?
           AND account.id = ? FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(context.market_world_id)
    .bind(context.policy_set_id)
    .bind(context.market_world_product_bundle_id)
    .bind(payload.account_id.get())
    .fetch_optional(&mut **tx)
    .await?
    .context("bond settlement account does not belong to the current run")?;
    ensure!(
        account.status == "open",
        "bond settlement account is closed"
    );
    let account_kind = bond_settlement_account_kind(&account.account_type)?;
    let position: LockedBondSettlementPositionRow = sqlx::query_as(
        "SELECT product_version_id, bond_units, total_cost_basis_krw
         FROM bond_position
         WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
           AND market_world_id = ? AND series_id = ? FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(payload.account_id.get())
    .bind(context.market_world_id)
    .bind(payload.series_id.get())
    .fetch_optional(&mut **tx)
    .await?
    .context("bond settlement position is missing")?;
    ensure!(
        position.product_version_id == payload.product_version_id.get(),
        "bond settlement position uses another product"
    );

    let lot_rows = if settlement.kind == "bondMaturity" {
        let rows: Vec<LockedBondLotRow> = sqlx::query_as(
            "SELECT id, remaining_units, remaining_cost_basis_krw
             FROM bond_lot
             WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
               AND market_world_id = ? AND series_id = ? AND remaining_units > 0
             ORDER BY acquired_game_day, id FOR UPDATE",
        )
        .bind(context.save_id)
        .bind(context.run_revision)
        .bind(payload.account_id.get())
        .bind(context.market_world_id)
        .bind(payload.series_id.get())
        .fetch_all(&mut **tx)
        .await?;
        let (lot_units, lot_basis) = rows.iter().try_fold(
            (0_u32, 0_i64),
            |(units, basis), row| -> Result<(u32, i64)> {
                Ok((
                    units
                        .checked_add(row.remaining_units)
                        .context("bond maturity lot units overflowed")?,
                    basis
                        .checked_add(row.remaining_cost_basis_krw)
                        .context("bond maturity lot basis overflowed")?,
                ))
            },
        )?;
        ensure!(
            lot_units == position.bond_units && lot_basis == position.total_cost_basis_krw,
            "bond maturity lots disagree with the position"
        );
        rows
    } else {
        Vec::new()
    };

    let tax_policy = if account_kind == BondSettlementAccountKind::Taxable
        && position.bond_units > 0
        && payload.coupon_krw_per_unit > 0
    {
        Some(read_general_income_policy(tx, context.policy_set_id, market_date).await?)
    } else {
        None
    };
    let (income_tax_ppm, local_income_tax_ppm) = tax_policy
        .map(|policy| (policy.income_tax_ppm, policy.local_income_tax_ppm))
        .unwrap_or((0, 0));
    let plan = plan_bond_cash_flow_amounts(
        account_kind,
        position.bond_units,
        position.total_cost_basis_krw,
        payload.coupon_krw_per_unit,
        payload.principal_krw_per_unit,
        income_tax_ppm,
        local_income_tax_ppm,
    )?;

    let isa_before: Option<(i64, i64)> = if account_kind == BondSettlementAccountKind::Isa
        && (plan.isa_tax_profit_delta_krw > 0 || plan.isa_deductible_loss_delta_krw > 0)
    {
        Some(
            sqlx::query_as(
                "SELECT isa_tax_profit_krw, isa_deductible_loss_krw
                 FROM isa_account_contract
                 WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
                   AND status = 'active' FOR UPDATE",
            )
            .bind(context.save_id)
            .bind(context.run_revision)
            .bind(payload.account_id.get())
            .fetch_optional(&mut **tx)
            .await?
            .context("active ISA contract is missing during bond settlement")?,
        )
    } else {
        None
    };
    let pension_layers_before = if account_kind == BondSettlementAccountKind::Pension
        && plan.pension_earnings_delta_krw > 0
    {
        Some(
            lock_pension_layers(
                tx,
                context.save_id,
                context.run_revision,
                payload.account_id,
            )
            .await?
            .context("active pension tax layers are missing during bond settlement")?,
        )
    } else {
        None
    };
    let pension_basis_before: Option<(i64, i64)> =
        if account_kind == BondSettlementAccountKind::Pension && plan.gross_principal_krw > 0 {
            let (last_day, position_value, risk_value): (u32, i64, i64) = sqlx::query_as(
                "SELECT last_valuation_game_day, position_market_value_krw, risk_asset_value_krw
             FROM pension_valuation_state
             WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
             FOR UPDATE",
            )
            .bind(context.save_id)
            .bind(context.run_revision)
            .bind(payload.account_id.get())
            .fetch_optional(&mut **tx)
            .await?
            .context("pension bond maturity requires same-day mark-to-market")?;
            let position_after = position_value
                .checked_sub(plan.gross_principal_krw)
                .context("pension maturity principal exceeds the valuation basis")?;
            ensure!(
                last_day == context.game_day
                    && position_after >= 0
                    && risk_value >= 0
                    && risk_value <= position_after,
                "pension bond maturity valuation basis is not ready"
            );
            Some((position_value, risk_value))
        } else {
            None
        };

    if !plan.moves_money() {
        let reason = if position.bond_units == 0 {
            "zeroBondPosition"
        } else {
            "zeroCoupon"
        };
        let update = sqlx::query(
            "UPDATE scheduled_settlement
             SET status = 'settled', outcome = 'noMovement', outcome_reason = ?
             WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'pending'",
        )
        .bind(reason)
        .bind(settlement_id)
        .bind(context.save_id)
        .bind(context.run_revision)
        .execute(&mut **tx)
        .await?;
        ensure!(
            update.rows_affected() == 1,
            "bond settlement lost its pending state"
        );
        return Ok(BondCashFlowSettlementResult::NoMovement);
    }

    let ledger = create_bond_cash_flow_ledger(
        rules,
        context,
        settlement_id,
        payload.account_id,
        &settlement.kind,
        plan,
    )?;
    let ledger_transaction_id = write_ledger_transaction(tx, &ledger).await?;
    let account_cash_after = account
        .cash_krw
        .checked_add(plan.account_credit_krw)
        .context("bond settlement account cash overflowed")?;
    let account_update = sqlx::query(
        "UPDATE financial_account SET cash_krw = ?
         WHERE save_id = ? AND run_revision = ? AND id = ?
           AND status = 'open' AND cash_krw = ?",
    )
    .bind(account_cash_after)
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(payload.account_id.get())
    .bind(account.cash_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        account_update.rows_affected() == 1,
        "bond settlement lost its account cash lock"
    );

    if plan.gross_principal_krw > 0 {
        let position_update = sqlx::query(
            "UPDATE bond_position SET bond_units = 0, total_cost_basis_krw = 0
             WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
               AND market_world_id = ? AND series_id = ? AND product_version_id = ?
               AND bond_units = ? AND total_cost_basis_krw = ?",
        )
        .bind(context.save_id)
        .bind(context.run_revision)
        .bind(payload.account_id.get())
        .bind(context.market_world_id)
        .bind(payload.series_id.get())
        .bind(payload.product_version_id.get())
        .bind(position.bond_units)
        .bind(position.total_cost_basis_krw)
        .execute(&mut **tx)
        .await?;
        ensure!(
            position_update.rows_affected() == 1,
            "bond maturity lost its position lock"
        );
        for lot in &lot_rows {
            let lot_update = sqlx::query(
                "UPDATE bond_lot SET remaining_units = 0, remaining_cost_basis_krw = 0
                 WHERE id = ? AND save_id = ? AND run_revision = ?
                   AND remaining_units = ? AND remaining_cost_basis_krw = ?",
            )
            .bind(lot.id)
            .bind(context.save_id)
            .bind(context.run_revision)
            .bind(lot.remaining_units)
            .bind(lot.remaining_cost_basis_krw)
            .execute(&mut **tx)
            .await?;
            ensure!(
                lot_update.rows_affected() == 1,
                "bond maturity lost a FIFO lot lock"
            );
        }
    }

    match account_kind {
        BondSettlementAccountKind::Taxable if plan.gross_coupon_krw > 0 => {
            accrue_financial_income_source(
                tx,
                AnnualTaxRunContext {
                    save_id: context.save_id,
                    run_revision: context.run_revision,
                    policy_set_id: context.policy_set_id,
                    game_day: context.game_day,
                    market_date,
                },
                FinancialIncomeAccrual {
                    source: FinancialIncomeSource::BondCoupon,
                    gross_income_krw: plan.gross_coupon_krw,
                    withheld_income_tax_krw: plan.income_tax_krw,
                    withheld_local_income_tax_krw: plan.local_income_tax_krw,
                },
            )
            .await?;
        }
        BondSettlementAccountKind::Isa => {
            if let Some((profit_before, loss_before)) = isa_before {
                let profit_after = profit_before
                    .checked_add(plan.isa_tax_profit_delta_krw)
                    .context("ISA bond profit balance overflowed")?;
                let loss_after = loss_before
                    .checked_add(plan.isa_deductible_loss_delta_krw)
                    .context("ISA bond loss balance overflowed")?;
                let update = sqlx::query(
                    "UPDATE isa_account_contract
                     SET isa_tax_profit_krw = ?, isa_deductible_loss_krw = ?
                     WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
                       AND status = 'active' AND isa_tax_profit_krw = ?
                       AND isa_deductible_loss_krw = ?",
                )
                .bind(profit_after)
                .bind(loss_after)
                .bind(context.save_id)
                .bind(context.run_revision)
                .bind(payload.account_id.get())
                .bind(profit_before)
                .bind(loss_before)
                .execute(&mut **tx)
                .await?;
                ensure!(
                    update.rows_affected() == 1,
                    "ISA bond settlement lost its tax-balance lock"
                );
            }
        }
        BondSettlementAccountKind::Pension => {
            if let Some(layers_before) = pension_layers_before {
                let earnings_after = layers_before
                    .earnings_krw
                    .checked_add(plan.pension_earnings_delta_krw)
                    .context("pension bond coupon earnings overflowed")?;
                let update = sqlx::query(
                    "UPDATE pension_tax_balance SET earnings_krw = ?
                     WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
                       AND tax_excluded_contribution_krw = ?
                       AND deferred_retirement_income_krw = ?
                       AND credited_contribution_krw = ? AND earnings_krw = ?",
                )
                .bind(earnings_after)
                .bind(context.save_id)
                .bind(context.run_revision)
                .bind(payload.account_id.get())
                .bind(layers_before.tax_excluded_contribution_krw)
                .bind(layers_before.deferred_retirement_income_krw)
                .bind(layers_before.credited_contribution_krw)
                .bind(layers_before.earnings_krw)
                .execute(&mut **tx)
                .await?;
                ensure!(
                    update.rows_affected() == 1,
                    "pension bond coupon lost its tax-balance lock"
                );
            }
            if let Some((position_value_before, risk_value)) = pension_basis_before {
                let position_value_after = position_value_before
                    .checked_sub(plan.gross_principal_krw)
                    .context("pension maturity valuation basis underflowed")?;
                let layers = lock_pension_layers(
                    tx,
                    context.save_id,
                    context.run_revision,
                    payload.account_id,
                )
                .await?
                .context("pension tax layers disappeared during bond maturity")?;
                let settlement_source_id = settlement_id.to_string();
                apply_explicit_pension_trade_basis_in_tx(
                    tx,
                    PensionTradeBasisWrite {
                        save_id: context.save_id,
                        run_revision: context.run_revision,
                        game_day: context.game_day,
                        account_id: payload.account_id,
                        source_kind: "bondMaturity",
                        source_id: &settlement_source_id,
                        position_market_value_before_krw: position_value_before,
                        position_market_value_after_krw: position_value_after,
                        risk_asset_value_after_krw: risk_value,
                        account_total_value_krw: pension_layers_total(layers)?,
                    },
                )
                .await?;
            }
        }
        BondSettlementAccountKind::Taxable => {}
    }

    let settlement_update = sqlx::query(
        "UPDATE scheduled_settlement
         SET status = 'settled', outcome = 'applied', settled_ledger_transaction_id = ?
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'pending'",
    )
    .bind(ledger_transaction_id)
    .bind(settlement_id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .execute(&mut **tx)
    .await?;
    ensure!(
        settlement_update.rows_affected() == 1,
        "bond settlement lost its pending state"
    );
    Ok(BondCashFlowSettlementResult::Applied {
        ledger_transaction_id,
    })
}

/// Freezes all non-zero LLX positions on a quarter record date and schedules their T+2 payment.
pub(super) async fn create_llx_entitlements_in_tx(
    tx: &mut Transaction<'_, MySql>,
    context: M2dDailyAssetContext,
) -> Result<u32> {
    let current: (Date, bool, Option<i64>) = sqlx::query_as(
        "SELECT market_date, market_open, llx_close_krw
         FROM market_daily WHERE world_id = ? AND game_day = ?",
    )
    .bind(context.market_world_id)
    .bind(context.game_day)
    .fetch_one(&mut **tx)
    .await?;
    let following_open_days: Vec<(u32, Date)> = sqlx::query_as(
        "SELECT game_day, market_date FROM market_daily
         WHERE world_id = ? AND game_day > ? AND market_open = TRUE
         ORDER BY game_day LIMIT 2",
    )
    .bind(context.market_world_id)
    .bind(context.game_day)
    .fetch_all(&mut **tx)
    .await?;
    let next_open_date = following_open_days.first().map(|row| row.1);
    if !is_llx_quarter_record_date(LlxQuarterRecordDateInput {
        current_date: current.0,
        market_open: current.1,
        next_open_date,
    })? {
        return Ok(0);
    }
    ensure!(
        following_open_days.len() == 2,
        "LLX record date has fewer than two future open sessions"
    );
    let (payment_game_day, payment_date) = following_open_days[1];
    let bundle = read_bundle_catalog(
        tx,
        context.market_world_id,
        context.market_world_product_bundle_id,
    )
    .await?
    .context("LLX entitlement day has no pinned product bundle")?;
    let record_close_krw = current
        .2
        .context("LLX record date is missing the product close")?;
    let terms = LlxProductTerms {
        annual_management_fee_ppm: bundle.index_product.annual_management_fee_ppm,
        annual_distribution_rate_ppm: bundle.index_product.annual_distribution_rate_ppm,
        day_count_denominator: bundle.index_product.day_count_denominator,
    };
    let positions: Vec<(u64, u32)> = sqlx::query_as(
        "SELECT position.account_id, position.quantity
         FROM asset_position AS position
         INNER JOIN financial_account AS account
           ON account.save_id = position.save_id AND account.id = position.account_id
         WHERE position.save_id = ? AND position.symbol = 'LLX'
           AND position.quantity > 0 AND account.run_revision = ?
           AND account.status = 'open'
           AND account.account_type IN (
               'taxableBrokerage', 'isaGeneral', 'isaLowIncome', 'pensionSavings', 'irp'
           )
         ORDER BY position.account_id",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .fetch_all(&mut **tx)
    .await?;
    let mut created = 0_u32;
    for (account_id, quantity) in positions {
        let draft = draft_llx_distribution_entitlement(
            terms,
            LlxEntitlementInput {
                record_date: current.0,
                payment_date,
                record_quantity: quantity,
                record_close_krw,
            },
        )?;
        let insert = sqlx::query(
            "INSERT IGNORE INTO llx_distribution_entitlement
                 (save_id, run_revision, financial_account_id, product_version_id,
                  record_game_day, record_date, payment_game_day, payment_date,
                  record_quantity, record_close_krw, per_share_distribution_krw,
                  gross_distribution_krw, status)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending')",
        )
        .bind(context.save_id)
        .bind(context.run_revision)
        .bind(account_id)
        .bind(bundle.index_product.id.get())
        .bind(context.game_day)
        .bind(draft.record_date)
        .bind(payment_game_day)
        .bind(draft.payment_date)
        .bind(draft.record_quantity)
        .bind(draft.record_close_krw)
        .bind(draft.per_share_distribution_krw)
        .bind(draft.gross_distribution_krw)
        .execute(&mut **tx)
        .await?;
        let entitlement_id = if insert.rows_affected() == 1 {
            created = created
                .checked_add(1)
                .context("LLX entitlement count overflowed")?;
            insert.last_insert_id()
        } else {
            let row: (u64,) = sqlx::query_as(
                "SELECT id FROM llx_distribution_entitlement
                 WHERE save_id = ? AND run_revision = ? AND financial_account_id = ?
                   AND product_version_id = ? AND record_game_day = ?",
            )
            .bind(context.save_id)
            .bind(context.run_revision)
            .bind(account_id)
            .bind(bundle.index_product.id.get())
            .bind(context.game_day)
            .fetch_one(&mut **tx)
            .await?;
            row.0
        };
        ensure!(entitlement_id != 0, "LLX entitlement has no identifier");
        let entitlement_resource = resource_id(entitlement_id, "LLX entitlement")?;
        let payload = serde_json::to_string(&serde_json::json!({
            "version": "v1",
            "entitlementId": entitlement_resource,
            "accountId": resource_id(account_id, "financial account")?,
            "productVersionId": bundle.index_product.id,
            "paymentDate": CanonicalDate::from_date(payment_date),
        }))?;
        sqlx::query(
            "INSERT IGNORE INTO scheduled_settlement
                 (save_id, run_revision, due_game_day, kind, payload,
                  source_kind, source_id, occurrence, status)
             VALUES (?, ?, ?, 'llxDistribution', ?, 'indexPosition', ?, 1, 'pending')",
        )
        .bind(context.save_id)
        .bind(context.run_revision)
        .bind(payment_game_day)
        .bind(payload)
        .bind(entitlement_id.to_string())
        .execute(&mut **tx)
        .await?;
    }
    Ok(created)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LlxDistributionSettlementResult {
    Applied { ledger_transaction_id: u64 },
    NoMovement,
    AlreadyFinalized,
}

#[derive(Debug, Clone, Copy, Deserialize)]
enum LlxSettlementPayloadVersion {
    #[serde(rename = "v1")]
    V1,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LlxDistributionSettlementPayload {
    version: LlxSettlementPayloadVersion,
    entitlement_id: ResourceId,
    account_id: ResourceId,
    product_version_id: ResourceId,
    payment_date: CanonicalDate,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedLlxSettlementRow {
    id: u64,
    due_game_day: u32,
    kind: String,
    payload_json: String,
    source_kind: String,
    source_id: String,
    occurrence: u32,
    status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedLlxEntitlementRow {
    id: u64,
    financial_account_id: u64,
    product_version_id: u64,
    payment_game_day: u32,
    payment_date: Date,
    gross_distribution_krw: i64,
    entitlement_status: String,
    account_type: String,
    account_status: String,
    account_cash_krw: i64,
}

fn parse_llx_distribution_payload(payload_json: &str) -> Result<LlxDistributionSettlementPayload> {
    let payload: LlxDistributionSettlementPayload = serde_json::from_str(payload_json)
        .context("LLX distribution settlement payload has an invalid schema")?;
    let _version = payload.version;
    Ok(payload)
}

fn validate_llx_distribution_identity(
    settlement: &LockedLlxSettlementRow,
    entitlement: &LockedLlxEntitlementRow,
    payload: LlxDistributionSettlementPayload,
) -> Result<()> {
    ensure!(
        settlement.id != 0
            && settlement.kind == "llxDistribution"
            && settlement.source_kind == "indexPosition"
            && settlement.source_id == payload.entitlement_id.to_string()
            && settlement.occurrence == 1,
        "LLX distribution settlement source identity is invalid"
    );
    ensure!(
        entitlement.id == payload.entitlement_id.get()
            && entitlement.financial_account_id == payload.account_id.get()
            && entitlement.product_version_id == payload.product_version_id.get()
            && entitlement.payment_game_day == settlement.due_game_day
            && entitlement.payment_date == payload.payment_date.as_date(),
        "LLX distribution settlement payload disagrees with its entitlement"
    );
    Ok(())
}

fn validate_llx_distribution_account_type(account_type: &str) -> Result<()> {
    ensure!(
        matches!(
            account_type,
            "taxableBrokerage" | "isaGeneral" | "isaLowIncome" | "pensionSavings" | "irp"
        ),
        "LLX entitlement uses a forbidden account type"
    );
    Ok(())
}

fn validate_llx_finalized_state(settlement_status: &str, entitlement_status: &str) -> Result<()> {
    ensure!(
        (settlement_status == "settled" && entitlement_status == "paid")
            || (settlement_status == "cancelled" && entitlement_status == "pending"),
        "LLX distribution finalized state disagrees with its entitlement"
    );
    Ok(())
}

/// Settles one LLX distribution row in the caller's global
/// `(due_game_day, settlement_id)` order.
pub(super) async fn settle_llx_entitlement_by_id_in_tx(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn crate::finance::FinanceRules,
    context: M2dDailyAssetContext,
    market_date: Date,
    settlement_id: u64,
) -> Result<LlxDistributionSettlementResult> {
    let settlement: LockedLlxSettlementRow = sqlx::query_as(
        "SELECT id, due_game_day, kind, CAST(payload AS CHAR) AS payload_json,
                source_kind, source_id, occurrence, status
         FROM scheduled_settlement
         WHERE id = ? AND save_id = ? AND run_revision = ? FOR UPDATE",
    )
    .bind(settlement_id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .fetch_optional(&mut **tx)
    .await?
    .context("LLX distribution settlement does not belong to the current run")?;
    let payload = parse_llx_distribution_payload(&settlement.payload_json)?;
    ensure!(
        settlement.status == "pending"
            || settlement.status == "settled"
            || settlement.status == "cancelled",
        "LLX distribution settlement has an invalid status"
    );

    let row: LockedLlxEntitlementRow = sqlx::query_as(
        "SELECT entitlement.id, entitlement.financial_account_id,
                entitlement.product_version_id, entitlement.payment_game_day,
                entitlement.payment_date,
                entitlement.gross_distribution_krw,
                entitlement.status AS entitlement_status,
                account.account_type, account.status AS account_status,
                account.cash_krw AS account_cash_krw
         FROM llx_distribution_entitlement AS entitlement
         INNER JOIN financial_account AS account
           ON account.save_id = entitlement.save_id
          AND account.run_revision = entitlement.run_revision
          AND account.id = entitlement.financial_account_id
         WHERE entitlement.save_id = ? AND entitlement.run_revision = ?
           AND entitlement.id = ? FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(payload.entitlement_id.get())
    .fetch_optional(&mut **tx)
    .await?
    .context("LLX distribution entitlement does not belong to the current run")?;
    validate_llx_distribution_identity(&settlement, &row, payload)?;
    validate_llx_distribution_account_type(&row.account_type)?;
    if settlement.status != "pending" {
        validate_llx_finalized_state(&settlement.status, &row.entitlement_status)?;
        return Ok(LlxDistributionSettlementResult::AlreadyFinalized);
    }
    ensure!(
        row.entitlement_status == "pending"
            && settlement.due_game_day <= context.game_day
            && row.payment_game_day <= context.game_day,
        "LLX distribution entitlement is not pending and due"
    );
    ensure!(
        row.account_status == "open",
        "LLX distribution entitlement account is closed"
    );
    let bundle = read_bundle_catalog(
        tx,
        context.market_world_id,
        context.market_world_product_bundle_id,
    )
    .await?
    .context("LLX distribution settlement run has no pinned product bundle")?;
    ensure!(
        bundle.index_product.id == payload.product_version_id,
        "LLX distribution settlement product is outside the pinned bundle"
    );
    let tax_policy = if row.account_type == "taxableBrokerage" && row.gross_distribution_krw > 0 {
        Some(read_general_income_policy(tx, context.policy_set_id, market_date).await?)
    } else {
        None
    };
    let movement = if row.gross_distribution_krw == 0 {
        LlxDistributionMovement::NoMovement {
            reason: crate::finance::LlxNoMovementReason::ZeroDistribution,
        }
    } else {
        LlxDistributionMovement::Cash {
            amount_krw: row.gross_distribution_krw,
        }
    };
    let ledger_transaction_id = match movement {
        LlxDistributionMovement::NoMovement { .. } => None,
        LlxDistributionMovement::Cash { amount_krw } => {
            let (income_tax_krw, local_income_tax_krw) = if row.account_type == "taxableBrokerage" {
                let policy = tax_policy.context("LLX withholding policy is missing")?;
                (
                    floor_rate(amount_krw, policy.income_tax_ppm)?,
                    floor_rate(amount_krw, policy.local_income_tax_ppm)?,
                )
            } else {
                (0, 0)
            };
            let net_amount_krw = amount_krw
                .checked_sub(income_tax_krw)
                .and_then(|value| value.checked_sub(local_income_tax_krw))
                .context("LLX distribution net amount overflowed")?;
            let account_cash_after = row
                .account_cash_krw
                .checked_add(net_amount_krw)
                .context("LLX distribution account cash overflowed")?;
            let account_id = resource_id(row.financial_account_id, "financial account")?;
            let mut postings = vec![
                LedgerPosting {
                    account_code: LedgerAccountCode::AccountCash,
                    financial_account_id: Some(account_id),
                    amount_krw: net_amount_krw,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::DistributionIncome,
                    financial_account_id: None,
                    amount_krw: amount_krw
                        .checked_neg()
                        .context("LLX distribution cannot be negated")?,
                },
            ];
            let withholding_tax_krw = income_tax_krw
                .checked_add(local_income_tax_krw)
                .context("LLX withholding tax overflowed")?;
            if withholding_tax_krw > 0 {
                postings.push(LedgerPosting {
                    account_code: LedgerAccountCode::WithholdingTaxLiability,
                    financial_account_id: None,
                    amount_krw: withholding_tax_krw,
                });
            }
            let ledger = rules.create_ledger_transaction(LedgerTransactionDraft {
                policy: RunPolicyContext {
                    run: RunId {
                        save_id: resource_id(context.save_id, "save")?,
                        run_revision: context.run_revision,
                    },
                    policy_set_id: resource_id(context.policy_set_id, "policy set")?,
                },
                source: LedgerSource {
                    kind: LedgerSourceKind::ScheduledSettlement,
                    source_id: settlement_id.to_string(),
                },
                game_day: context.game_day,
                description: "LLX 분배금 지급".to_owned(),
                postings,
            })?;
            let ledger_transaction_id = write_ledger_transaction(tx, &ledger).await?;
            let account_update = sqlx::query(
                "UPDATE financial_account SET cash_krw = ?
                     WHERE save_id = ? AND run_revision = ? AND id = ?
                       AND status = 'open' AND cash_krw = ?",
            )
            .bind(account_cash_after)
            .bind(context.save_id)
            .bind(context.run_revision)
            .bind(row.financial_account_id)
            .bind(row.account_cash_krw)
            .execute(&mut **tx)
            .await?;
            ensure!(
                account_update.rows_affected() == 1,
                "LLX distribution account lost its lock"
            );
            match row.account_type.as_str() {
                "taxableBrokerage" => {
                    accrue_financial_income_source(
                        tx,
                        AnnualTaxRunContext {
                            save_id: context.save_id,
                            run_revision: context.run_revision,
                            policy_set_id: context.policy_set_id,
                            game_day: context.game_day,
                            market_date,
                        },
                        FinancialIncomeAccrual {
                            source: FinancialIncomeSource::LlxDistribution,
                            gross_income_krw: amount_krw,
                            withheld_income_tax_krw: income_tax_krw,
                            withheld_local_income_tax_krw: local_income_tax_krw,
                        },
                    )
                    .await?;
                }
                "isaGeneral" | "isaLowIncome" => {
                    let update = sqlx::query(
                        "UPDATE isa_account_contract
                             SET isa_tax_profit_krw = isa_tax_profit_krw + ?
                             WHERE save_id = ? AND run_revision = ?
                               AND financial_account_id = ? AND status = 'active'",
                    )
                    .bind(amount_krw)
                    .bind(context.save_id)
                    .bind(context.run_revision)
                    .bind(row.financial_account_id)
                    .execute(&mut **tx)
                    .await?;
                    ensure!(
                        update.rows_affected() == 1,
                        "active ISA contract is missing"
                    );
                }
                "pensionSavings" | "irp" => {
                    let update = sqlx::query(
                        "UPDATE pension_tax_balance
                             SET earnings_krw = earnings_krw + ?
                             WHERE save_id = ? AND run_revision = ?
                               AND financial_account_id = ?",
                    )
                    .bind(amount_krw)
                    .bind(context.save_id)
                    .bind(context.run_revision)
                    .bind(row.financial_account_id)
                    .execute(&mut **tx)
                    .await?;
                    ensure!(
                        update.rows_affected() == 1,
                        "active pension tax balance is missing"
                    );
                }
                _ => bail!("LLX entitlement uses a forbidden account type"),
            }
            Some(ledger_transaction_id)
        }
    };
    let (outcome, outcome_reason) = if ledger_transaction_id.is_some() {
        ("applied", None)
    } else {
        ("noMovement", Some("zeroDistribution"))
    };
    let entitlement_update = sqlx::query(
        "UPDATE llx_distribution_entitlement
             SET status = 'paid', outcome = ?, paid_game_day = ?, ledger_transaction_id = ?
             WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'pending'",
    )
    .bind(outcome)
    .bind(context.game_day)
    .bind(ledger_transaction_id)
    .bind(row.id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .execute(&mut **tx)
    .await?;
    ensure!(
        entitlement_update.rows_affected() == 1,
        "LLX distribution entitlement lost its pending state"
    );
    let settlement_update = sqlx::query(
        "UPDATE scheduled_settlement
             SET status = 'settled', outcome = ?, outcome_reason = ?,
                 settled_ledger_transaction_id = ?
             WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'pending'",
    )
    .bind(outcome)
    .bind(outcome_reason)
    .bind(ledger_transaction_id)
    .bind(settlement_id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .execute(&mut **tx)
    .await?;
    ensure!(
        settlement_update.rows_affected() == 1,
        "LLX distribution settlement lost its pending state"
    );
    if let Some(ledger_transaction_id) = ledger_transaction_id {
        Ok(LlxDistributionSettlementResult::Applied {
            ledger_transaction_id,
        })
    } else {
        Ok(LlxDistributionSettlementResult::NoMovement)
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeneralIncomePolicy {
    income_tax_ppm: i64,
    local_income_tax_ppm: i64,
    comprehensive_threshold_krw: i64,
}

async fn read_general_income_policy(
    tx: &mut Transaction<'_, MySql>,
    policy_set_id: u64,
    market_date: Date,
) -> Result<GeneralIncomePolicy> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT CAST(parameters AS CHAR) FROM policy_rule
         WHERE policy_set_id = ? AND domain = 'tax'
           AND rule_key = 'generalFinancialIncome' AND effective_from <= ?
           AND (effective_to IS NULL OR effective_to >= ?)
         ORDER BY effective_from DESC LIMIT 2",
    )
    .bind(policy_set_id)
    .bind(market_date)
    .bind(market_date)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() == 1,
        "general financial-income policy is missing or overlapping"
    );
    let policy: GeneralIncomePolicy = serde_json::from_str(&rows[0].0)
        .context("general financial-income policy has an invalid schema")?;
    ensure!(
        (0..=1_000_000).contains(&policy.income_tax_ppm)
            && (0..=1_000_000).contains(&policy.local_income_tax_ppm)
            && policy.comprehensive_threshold_krw >= 0,
        "general financial-income policy has invalid values"
    );
    Ok(policy)
}

fn floor_rate(amount_krw: i64, rate_ppm: i64) -> Result<i64> {
    ensure!(amount_krw >= 0 && (0..=1_000_000).contains(&rate_ppm));
    i128::from(amount_krw)
        .checked_mul(i128::from(rate_ppm))
        .and_then(|value| value.checked_div(1_000_000))
        .and_then(|value| i64::try_from(value).ok())
        .context("rate calculation overflowed")
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct BondSeriesRow {
    id: u64,
    product_version_id: u64,
    issued_date: Date,
    maturity_date: Date,
    coupon_rate_bp: i32,
    issue_yield_bp: i32,
}

fn bond_series_is_tradable(issued_date: Date, maturity_date: Date, market_date: Date) -> bool {
    issued_date <= market_date && market_date < maturity_date
}

async fn read_bond_catalog_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save: &LockedAssetSaveRow,
) -> Result<BondCatalog> {
    let market = read_current_market(tx, save.market_world_id, save.game_day).await?;
    let Some(bundle) = read_bundle_catalog(
        tx,
        save.market_world_id,
        save.market_world_product_bundle_id,
    )
    .await?
    else {
        return Ok(BondCatalog {
            market_version: market.market_version,
            products: Vec::new(),
            series: Vec::new(),
        });
    };

    let limit = u32::try_from(crate::finance::MAX_BOND_CATALOG_SERIES)?
        .checked_add(SNAPSHOT_QUERY_EXTRA_ROW)
        .context("bond catalog query limit overflowed")?;
    let rows: Vec<BondSeriesRow> = sqlx::query_as(
        "SELECT id, product_version_id, issued_date, maturity_date,
                coupon_rate_bp, issue_yield_bp
         FROM bond_series
         WHERE market_world_id = ? AND issued_date <= ? AND maturity_date > ?
         ORDER BY issued_date, product_version_id, id
         LIMIT ?",
    )
    .bind(save.market_world_id)
    .bind(market.market_date)
    .bind(market.market_date)
    .bind(limit)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() <= crate::finance::MAX_BOND_CATALOG_SERIES,
        "bond catalog exceeds its bounded contract"
    );

    let mut series_items = Vec::with_capacity(rows.len());
    for row in rows {
        ensure!(
            bond_series_is_tradable(row.issued_date, row.maturity_date, market.market_date),
            "bond catalog contains a series outside its trading dates"
        );
        let product = bundle
            .bond_products
            .iter()
            .find(|product| product.id.get() == row.product_version_id)
            .context("bond series references a product outside the pinned bundle")?;
        let series = create_bond_series(product.terms, row.issued_date, row.issue_yield_bp)
            .context("stored bond series cannot be reconstructed")?;
        ensure!(
            series.maturity_date == row.maturity_date
                && series.coupon_rate_bp == row.coupon_rate_bp,
            "stored bond series disagrees with immutable product terms"
        );
        let current_yield_bp = match product.terms.term {
            BondTerm::Years3 => market.treasury_3y_bp,
            BondTerm::Years10 => market.treasury_10y_bp,
        }
        .context("current bond yield is missing")?;
        let dirty_price_krw =
            dirty_bond_price_krw(market.market_date, current_yield_bp, &series.cash_flows)
                .context("bond dirty price cannot be calculated")?;
        let next_coupon_date = series
            .cash_flows
            .iter()
            .find(|flow| flow.payment_date > market.market_date)
            .map(|flow| flow.payment_date)
            .context("unmatured bond series has no future cash flow")?;
        series_items.push(BondSeriesCatalogItem {
            id: resource_id(row.id, "bond series")?,
            product_version_id: product.id,
            issued_date: CanonicalDate::from_date(row.issued_date),
            maturity_date: CanonicalDate::from_date(row.maturity_date),
            coupon_rate_bp: row.coupon_rate_bp,
            issue_yield_bp: row.issue_yield_bp,
            next_coupon_date: CanonicalDate::from_date(next_coupon_date),
            dirty_price_krw,
            current_yield_bp,
        });
    }

    let catalog = BondCatalog {
        market_version: bundle.market_version,
        products: bundle
            .bond_products
            .iter()
            .map(|product| BondProductCatalogItem {
                id: product.id,
                key: product.key.clone(),
                display_name: product.display_name.clone(),
                term_years: match product.terms.term {
                    BondTerm::Years3 => 3,
                    BondTerm::Years10 => 10,
                },
                face_value_krw: product.terms.face_value_krw,
                max_order_units: product.terms.maximum_order_units,
                max_position_units: product.terms.maximum_position_units,
                buy_fee_ppm: product.terms.buy_fee_rate_ppm,
                sell_fee_ppm: product.terms.sell_fee_rate_ppm,
            })
            .collect(),
        series: series_items,
    };
    catalog
        .validate()
        .context("stored bond catalog violates the API contract")?;
    Ok(catalog)
}

async fn read_gold_catalog_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save: &LockedAssetSaveRow,
) -> Result<GoldCatalog> {
    let market = read_current_market(tx, save.market_world_id, save.game_day).await?;
    let Some(bundle) = read_bundle_catalog(
        tx,
        save.market_world_id,
        save.market_world_product_bundle_id,
    )
    .await?
    else {
        return Ok(GoldCatalog {
            market_version: market.market_version,
            products: Vec::new(),
        });
    };
    let product = &bundle.gold_product;
    let catalog = GoldCatalog {
        market_version: bundle.market_version,
        products: vec![GoldProductCatalogItem {
            id: product.id,
            key: product.key.clone(),
            display_name: product.display_name.clone(),
            unit: GoldUnit::Gram,
            buy_fee_ppm: product.terms.buy_fee_ppm,
            sell_fee_ppm: product.terms.sell_fee_ppm,
            buy_tax_ppm: product.terms.buy_tax_ppm,
            sell_tax_ppm: product.terms.sell_tax_ppm,
            withdrawal_bars: [
                GoldWithdrawalBar {
                    bar_size_gram: 100,
                    fee_krw: product.terms.withdrawal_fee_100g_krw,
                },
                GoldWithdrawalBar {
                    bar_size_gram: 1_000,
                    fee_krw: product.terms.withdrawal_fee_1kg_krw,
                },
            ],
        }],
    };
    catalog
        .validate()
        .context("stored gold catalog violates the API contract")?;
    Ok(catalog)
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PendingEntitlementRow {
    id: u64,
    financial_account_id: u64,
    record_date: Date,
    payment_date: Date,
    record_quantity: u32,
    gross_distribution_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct BondPositionRow {
    financial_account_id: u64,
    series_id: u64,
    product_version_id: u64,
    bond_units: u32,
    total_cost_basis_krw: i64,
    issued_date: Date,
    maturity_date: Date,
    coupon_rate_bp: i32,
    issue_yield_bp: i32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct GoldAccountRow {
    financial_account_id: u64,
    product_version_id: u64,
    quantity_gram: u32,
    total_cost_basis_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PhysicalGoldRow {
    bar_size_gram: u32,
    bar_count: u32,
}

async fn read_m2d_asset_snapshot_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save: &LockedAssetSaveRow,
) -> Result<M2dAssetSnapshot> {
    let market = read_current_market(tx, save.market_world_id, save.game_day).await?;
    let bundle = read_bundle_catalog(
        tx,
        save.market_world_id,
        save.market_world_product_bundle_id,
    )
    .await?;
    let Some(bundle) = bundle else {
        return Ok(M2dAssetSnapshot::default());
    };

    let entitlement_limit = u32::try_from(crate::finance::MAX_PENDING_LLX_ENTITLEMENTS)?
        .checked_add(SNAPSHOT_QUERY_EXTRA_ROW)
        .context("entitlement snapshot query limit overflowed")?;
    let entitlement_rows: Vec<PendingEntitlementRow> = sqlx::query_as(
        "SELECT id, financial_account_id, record_date, payment_date,
                record_quantity, gross_distribution_krw
         FROM llx_distribution_entitlement
         WHERE save_id = ? AND run_revision = ? AND status = 'pending'
         ORDER BY payment_game_day, id LIMIT ?",
    )
    .bind(save.id)
    .bind(save.run_revision)
    .bind(entitlement_limit)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        entitlement_rows.len() <= crate::finance::MAX_PENDING_LLX_ENTITLEMENTS,
        "pending LLX entitlement snapshot exceeds its bound"
    );

    let position_limit = u32::try_from(crate::finance::MAX_BOND_POSITION_SNAPSHOTS)?
        .checked_add(SNAPSHOT_QUERY_EXTRA_ROW)
        .context("bond position snapshot query limit overflowed")?;
    let bond_rows: Vec<BondPositionRow> = sqlx::query_as(
        "SELECT position.financial_account_id, position.series_id,
                position.product_version_id, position.bond_units,
                position.total_cost_basis_krw, series.issued_date,
                series.maturity_date, series.coupon_rate_bp, series.issue_yield_bp
         FROM bond_position AS position
         INNER JOIN bond_series AS series
           ON series.market_world_id = position.market_world_id
          AND series.id = position.series_id
         WHERE position.save_id = ? AND position.run_revision = ?
           AND position.bond_units > 0
         ORDER BY position.financial_account_id, position.series_id LIMIT ?",
    )
    .bind(save.id)
    .bind(save.run_revision)
    .bind(position_limit)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        bond_rows.len() <= crate::finance::MAX_BOND_POSITION_SNAPSHOTS,
        "bond position snapshot exceeds its bound"
    );

    let gold_limit = u32::try_from(crate::finance::MAX_GOLD_ACCOUNT_SNAPSHOTS)?
        .checked_add(SNAPSHOT_QUERY_EXTRA_ROW)
        .context("gold account snapshot query limit overflowed")?;
    let gold_rows: Vec<GoldAccountRow> = sqlx::query_as(
        "SELECT contract.financial_account_id, contract.product_version_id,
                position.quantity_gram, position.total_cost_basis_krw
         FROM gold_account_contract AS contract
         INNER JOIN financial_account AS account
           ON account.save_id = contract.save_id
          AND account.run_revision = contract.run_revision
          AND account.id = contract.financial_account_id
         INNER JOIN gold_position AS position
           ON position.save_id = contract.save_id
          AND position.run_revision = contract.run_revision
          AND position.financial_account_id = contract.financial_account_id
         WHERE contract.save_id = ? AND contract.run_revision = ?
           AND contract.status = 'active' AND account.status = 'open'
         ORDER BY contract.financial_account_id LIMIT ?",
    )
    .bind(save.id)
    .bind(save.run_revision)
    .bind(gold_limit)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        gold_rows.len() <= crate::finance::MAX_GOLD_ACCOUNT_SNAPSHOTS,
        "gold account snapshot exceeds its bound"
    );

    let physical_limit = u32::try_from(crate::finance::MAX_PHYSICAL_GOLD_HOLDINGS)?
        .checked_add(SNAPSHOT_QUERY_EXTRA_ROW)
        .context("physical gold snapshot query limit overflowed")?;
    let physical_rows: Vec<PhysicalGoldRow> = sqlx::query_as(
        "SELECT bar_size_gram, bar_count FROM physical_gold_holding
         WHERE save_id = ? AND run_revision = ? ORDER BY bar_size_gram LIMIT ?",
    )
    .bind(save.id)
    .bind(save.run_revision)
    .bind(physical_limit)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        physical_rows.len() <= crate::finance::MAX_PHYSICAL_GOLD_HOLDINGS,
        "physical gold snapshot exceeds its bound"
    );

    let mut bond_positions = Vec::with_capacity(bond_rows.len());
    for row in bond_rows {
        let product = bundle
            .bond_products
            .iter()
            .find(|product| product.id.get() == row.product_version_id)
            .context("bond position product is outside the pinned bundle")?;
        let series = create_bond_series(product.terms, row.issued_date, row.issue_yield_bp)
            .context("bond position series cannot be reconstructed")?;
        ensure!(
            series.maturity_date == row.maturity_date
                && series.coupon_rate_bp == row.coupon_rate_bp,
            "bond position series disagrees with immutable terms"
        );
        let current_yield_bp = match product.terms.term {
            BondTerm::Years3 => market.treasury_3y_bp,
            BondTerm::Years10 => market.treasury_10y_bp,
        }
        .context("bond snapshot yield is missing")?;
        let dirty_price_krw =
            dirty_bond_price_krw(market.market_date, current_yield_bp, &series.cash_flows)?;
        let market_value_krw = checked_product_i64(dirty_price_krw, row.bond_units)?;
        let unrealized_gain_loss_krw = market_value_krw
            .checked_sub(row.total_cost_basis_krw)
            .context("bond unrealized gain or loss overflowed")?;
        bond_positions.push(BondPositionSnapshot {
            account_id: resource_id(row.financial_account_id, "bond account")?,
            series_id: resource_id(row.series_id, "bond series")?,
            bond_units: row.bond_units,
            total_cost_basis_krw: row.total_cost_basis_krw,
            dirty_price_krw,
            market_value_krw,
            unrealized_gain_loss_krw,
        });
    }

    let gold_close_krw_per_gram = market
        .gold_close_krw_per_gram
        .context("M2-D gold close is missing")?;
    let mut gold_accounts = Vec::with_capacity(gold_rows.len());
    for row in gold_rows {
        ensure!(
            row.product_version_id == bundle.gold_product.id.get(),
            "gold account product is outside the pinned bundle"
        );
        let market_value_krw = checked_product_i64(gold_close_krw_per_gram, row.quantity_gram)?;
        let average_cost_krw_per_gram = if row.quantity_gram == 0 {
            None
        } else {
            Some(
                row.total_cost_basis_krw
                    .checked_div(i64::from(row.quantity_gram))
                    .context("gold average cost division failed")?,
            )
        };
        gold_accounts.push(GoldAccountSnapshot {
            account_id: resource_id(row.financial_account_id, "gold account")?,
            product_version_id: resource_id(row.product_version_id, "gold product")?,
            quantity_gram: row.quantity_gram,
            total_cost_basis_krw: row.total_cost_basis_krw,
            average_cost_krw_per_gram,
            close_krw_per_gram: gold_close_krw_per_gram,
            market_value_krw,
            unrealized_gain_loss_krw: market_value_krw
                .checked_sub(row.total_cost_basis_krw)
                .context("gold unrealized gain or loss overflowed")?,
        });
    }

    let physical_gold_holdings = physical_rows
        .into_iter()
        .map(|row| {
            let total_quantity_gram = row
                .bar_size_gram
                .checked_mul(row.bar_count)
                .context("physical gold quantity overflowed")?;
            Ok(PhysicalGoldHoldingSnapshot {
                bar_size_gram: row.bar_size_gram,
                bar_count: row.bar_count,
                total_quantity_gram,
                close_krw_per_gram: gold_close_krw_per_gram,
                market_value_krw: checked_product_i64(
                    gold_close_krw_per_gram,
                    total_quantity_gram,
                )?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let snapshot = M2dAssetSnapshot {
        product_bundle: Some(ProductBundleSnapshot {
            index_product: IndexProductSnapshot {
                id: bundle.index_product.id,
                key: bundle.index_product.key,
                display_name: bundle.index_product.display_name,
                annual_management_fee_ppm: bundle.index_product.annual_management_fee_ppm,
                annual_distribution_rate_ppm: bundle.index_product.annual_distribution_rate_ppm,
                day_count_denominator: bundle.index_product.day_count_denominator,
                buy_fee_ppm: bundle.index_product.buy_fee_ppm,
                sell_fee_ppm: bundle.index_product.sell_fee_ppm,
                sell_tax_ppm: bundle.index_product.sell_tax_ppm,
            },
            bond_product_version_ids: [bundle.bond_products[0].id, bundle.bond_products[1].id],
            gold_product_version_id: bundle.gold_product.id,
        }),
        llx_distribution_entitlements: entitlement_rows
            .into_iter()
            .map(|row| {
                Ok(LlxDistributionEntitlementSnapshot {
                    id: resource_id(row.id, "LLX distribution entitlement")?,
                    account_id: resource_id(row.financial_account_id, "financial account")?,
                    record_date: CanonicalDate::from_date(row.record_date),
                    payment_date: CanonicalDate::from_date(row.payment_date),
                    quantity: row.record_quantity,
                    gross_amount_krw: row.gross_distribution_krw,
                    status: PendingEntitlementStatus::Pending,
                })
            })
            .collect::<Result<Vec<_>>>()?,
        bond_positions,
        gold_accounts,
        physical_gold_holdings,
    };
    snapshot
        .validate()
        .context("stored asset state violates the bounded snapshot contract")?;
    Ok(snapshot)
}

pub(super) async fn read_m2d_asset_snapshot_for_run_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    market_world_id: u64,
    market_world_product_bundle_id: Option<u64>,
    run_revision: u32,
    game_day: u32,
) -> Result<M2dAssetSnapshot> {
    let save = LockedAssetSaveRow {
        id: save_id,
        market_world_id,
        policy_set_id: 0,
        market_world_product_bundle_id,
        run_revision,
        state_revision: 0,
        game_day,
        has_character: true,
    };
    read_m2d_asset_snapshot_in_tx(tx, &save).await
}

fn checked_product_i64(unit_amount_krw: i64, quantity: u32) -> Result<i64> {
    ensure!(unit_amount_krw >= 0, "unit amount cannot be negative");
    i128::from(unit_amount_krw)
        .checked_mul(i128::from(quantity))
        .and_then(|value| i64::try_from(value).ok())
        .context("asset market value overflowed")
}

impl MySqlFinanceStore {
    pub(super) async fn open_gold_account(
        &self,
        user_id: u64,
        command: &OpenGoldAccountCommand,
    ) -> Result<M2dAssetCommandResult<OpenGoldAccountResponse>> {
        let fingerprint = open_gold_account_fingerprint(command);
        let mut tx = self.pool.begin().await?;
        let Some(current) = lock_asset_save_for_user(&mut tx, user_id).await? else {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::CharacterRequired,
            ));
        };
        let identity = CommandIdentitySpec {
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_OPEN_GOLD_ACCOUNT,
            payload_sha256: &fingerprint,
            cursor: command.cursor,
        };
        match inspect_command_identity(&mut tx, current.id, &identity).await? {
            CommandIdentityState::Conflict => {
                tx.commit().await?;
                return Ok(M2dAssetCommandResult::Rejected(
                    FinanceFailureCode::IdempotencyConflict,
                ));
            }
            CommandIdentityState::Matching => {
                let mut receipt: OpenGoldAccountReceipt = read_asset_receipt(
                    &mut tx,
                    current.id,
                    &command.command_id,
                    COMMAND_KIND_OPEN_GOLD_ACCOUNT,
                    &fingerprint,
                )
                .await?
                .context("gold-account command identity has no final receipt")?;
                ensure!(!receipt.replayed, "stored gold-account receipt is replayed");
                receipt.replayed = true;
                let snapshot = read_m2d_asset_snapshot_in_tx(&mut tx, &current).await?;
                tx.commit().await?;
                return Ok(M2dAssetCommandResult::Applied(OpenGoldAccountResponse {
                    account: receipt,
                    snapshot,
                }));
            }
            CommandIdentityState::Missing => {}
        }
        if let Some(rejection) = validate_asset_current(&current, command.cursor) {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(rejection));
        }

        let Some(bundle) = read_bundle_catalog(
            &mut tx,
            current.market_world_id,
            current.market_world_product_bundle_id,
        )
        .await?
        else {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::ProductNotFound,
            ));
        };
        if command.product_version_id != bundle.gold_product.id {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::ProductNotFound,
            ));
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
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::LimitExceeded,
            ));
        }
        let existing: (bool,) = sqlx::query_as(
            "SELECT EXISTS(
                 SELECT 1 FROM gold_account_contract
                 WHERE save_id = ? AND run_revision = ? AND status = 'active'
             )",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .fetch_one(&mut *tx)
        .await?;
        if existing.0 {
            tx.commit().await?;
            return Ok(M2dAssetCommandResult::Rejected(
                FinanceFailureCode::AccountAlreadyExists,
            ));
        }

        write_command_identity(&mut tx, current.id, &identity).await?;
        let account_insert = sqlx::query(
            "INSERT INTO financial_account
                 (save_id, run_revision, account_type, status, cash_krw,
                  is_default, opened_game_day)
             VALUES (?, ?, 'krxGold', 'open', 0, FALSE, ?)",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(current.game_day)
        .execute(&mut *tx)
        .await?;
        let account_id = account_insert.last_insert_id();
        ensure!(
            account_id != 0,
            "gold account insert returned no identifier"
        );
        sqlx::query(
            "INSERT INTO gold_account_contract
                 (save_id, run_revision, financial_account_id, product_version_id,
                  status, opened_game_day)
             VALUES (?, ?, ?, ?, 'active', ?)",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(account_id)
        .bind(bundle.gold_product.id.get())
        .bind(current.game_day)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO gold_position
                 (save_id, run_revision, financial_account_id, product_version_id,
                  quantity_gram, total_cost_basis_krw)
             VALUES (?, ?, ?, ?, 0, 0)",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(account_id)
        .bind(bundle.gold_product.id.get())
        .execute(&mut *tx)
        .await?;

        let committed = increment_asset_state_revision(&mut tx, &current).await?;
        let receipt = OpenGoldAccountReceipt {
            command_id: command.command_id.clone(),
            account_id: resource_id(account_id, "gold account")?,
            account_type: M2dAccountType::KrxGold,
            product_version_id: bundle.gold_product.id,
            replayed: false,
        };
        write_game_command_receipt(
            &mut tx,
            GameCommandReceiptWrite {
                save_id: current.id,
                command_id: &command.command_id,
                command_kind: COMMAND_KIND_OPEN_GOLD_ACCOUNT,
                payload_sha256: &fingerprint,
                market_world_id: current.market_world_id,
                committed_cursor: committed,
                result: &receipt,
                ledger_transaction_id: None,
            },
        )
        .await?;
        let snapshot = read_m2d_asset_snapshot_in_tx(&mut tx, &current).await?;
        tx.commit().await?;
        Ok(M2dAssetCommandResult::Applied(OpenGoldAccountResponse {
            account: receipt,
            snapshot,
        }))
    }
}

fn validate_asset_current(
    current: &LockedAssetSaveRow,
    cursor: CommandCursor,
) -> Option<FinanceFailureCode> {
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

async fn increment_asset_state_revision(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedAssetSaveRow,
) -> Result<GameCommandCursor> {
    let state_revision = current
        .state_revision
        .checked_add(1)
        .context("state revision overflowed in an asset command")?;
    let update = sqlx::query(
        "UPDATE save SET state_revision = ?
         WHERE id = ? AND market_world_id = ? AND policy_set_id = ?
           AND market_world_product_bundle_id <=> ?
           AND run_revision = ? AND state_revision = ? AND game_day = ?",
    )
    .bind(state_revision)
    .bind(current.id)
    .bind(current.market_world_id)
    .bind(current.policy_set_id)
    .bind(current.market_world_product_bundle_id)
    .bind(current.run_revision)
    .bind(current.state_revision)
    .bind(current.game_day)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "asset command save cursor changed under its lock"
    );
    Ok(GameCommandCursor {
        run_revision: current.run_revision,
        state_revision,
        game_day: current.game_day,
    })
}

async fn read_asset_receipt<T: DeserializeOwned>(
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
        "asset receipt disagrees with command identity"
    );
    serde_json::from_str(&result_json)
        .map(Some)
        .context("asset receipt result is invalid")
}

fn open_gold_account_fingerprint(command: &OpenGoldAccountCommand) -> String {
    fingerprint(&format!(
        "lifeledger.finance.open-gold-account.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\ntype=krxGold\nproductVersionId={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.product_version_id
    ))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn command_id() -> CommandId {
        CommandId::parse("11111111-1111-4111-8111-111111111111")
            .expect("유효한 command ID여야 한다")
    }

    fn cursor() -> CommandCursor {
        CommandCursor {
            expected_run_revision: 1,
            expected_state_revision: 2,
            expected_game_day: 3,
        }
    }

    fn bundle_row() -> BundleCatalogRow {
        BundleCatalogRow {
            market_version: "m2-2026-calibration-v4".to_owned(),
            index_id: 1,
            index_key: "llx-domestic-equity-2026-v1".to_owned(),
            index_display_name: "LLX 국내주식 지수".to_owned(),
            index_annual_management_fee_ppm: 1_500,
            index_annual_distribution_rate_ppm: 20_000,
            index_day_count_denominator: 365,
            index_buy_fee_ppm: 0,
            index_sell_fee_ppm: 0,
            index_sell_tax_ppm: 0,
            bond_3y_id: 1,
            bond_3y_key: "kr-government-bond-3y-2026-v1".to_owned(),
            bond_3y_display_name: "대한민국 국고채 3년".to_owned(),
            bond_3y_term_years: 3,
            bond_3y_face_value_krw: 10_000,
            bond_3y_max_order_units: 100_000,
            bond_3y_max_position_units: 100_000,
            bond_3y_buy_fee_ppm: 0,
            bond_3y_sell_fee_ppm: 0,
            bond_10y_id: 2,
            bond_10y_key: "kr-government-bond-10y-2026-v1".to_owned(),
            bond_10y_display_name: "대한민국 국고채 10년".to_owned(),
            bond_10y_term_years: 10,
            bond_10y_face_value_krw: 10_000,
            bond_10y_max_order_units: 100_000,
            bond_10y_max_position_units: 100_000,
            bond_10y_buy_fee_ppm: 0,
            bond_10y_sell_fee_ppm: 0,
            gold_id: 1,
            gold_key: "krx-gold-2026-v1".to_owned(),
            gold_display_name: "KRX 금시장 금 1g".to_owned(),
            gold_unit: "gram".to_owned(),
            gold_buy_fee_ppm: 0,
            gold_sell_fee_ppm: 0,
            gold_buy_tax_ppm: 0,
            gold_sell_tax_ppm: 0,
            gold_withdrawal_100g_fee_krw: 20_000,
            gold_withdrawal_1000g_fee_krw: 100_000,
        }
    }

    mod context_국채_시리즈의_거래가능일을_판단할_때 {
        use super::*;

        fn given_date(raw: &str) -> Date {
            CanonicalDate::parse(raw)
                .expect("유효한 비교 날짜여야 한다")
                .as_date()
        }

        #[test]
        fn given_발행일전_시리즈_when_판단하면_then_거래할수없다() {
            let market_date = given_date("2026-03-31");

            let tradable = bond_series_is_tradable(
                given_date("2026-04-01"),
                given_date("2029-04-01"),
                market_date,
            );

            assert!(!tradable);
        }

        #[test]
        fn given_발행일인_시리즈_when_판단하면_then_거래할수있다() {
            let market_date = given_date("2026-04-01");

            let tradable = bond_series_is_tradable(
                given_date("2026-04-01"),
                given_date("2029-04-01"),
                market_date,
            );

            assert!(tradable);
        }

        #[test]
        fn given_만기일인_시리즈_when_판단하면_then_거래할수없다() {
            let market_date = given_date("2029-04-01");

            let tradable = bond_series_is_tradable(
                given_date("2026-04-01"),
                given_date("2029-04-01"),
                market_date,
            );

            assert!(!tradable);
        }
    }

    mod bundle_row_conversion_rule {
        use super::*;

        mod context_pinned_product_shape {
            use super::*;

            #[test]
            fn given_exact_bundle_row_when_converted_then_typed_terms_are_preserved() {
                let row = bundle_row();

                let catalog = row.into_catalog().expect("고정 번들을 변환해야 한다");

                assert_eq!(catalog.bond_products[0].terms.term, BondTerm::Years3);
                assert_eq!(catalog.bond_products[1].terms.term, BondTerm::Years10);
                assert_eq!(catalog.gold_product.terms.withdrawal_fee_1kg_krw, 100_000);
            }

            #[test]
            fn given_wrong_three_year_slot_when_converted_then_error_is_returned() {
                let mut row = bundle_row();
                row.bond_3y_term_years = 10;

                let result = row.into_catalog();

                assert!(result.is_err());
            }
        }
    }

    mod command_fingerprint_rule {
        use super::*;

        mod context_gold_withdrawal_identity {
            use super::*;

            #[test]
            fn given_bar_count_changes_when_fingerprinted_then_hash_changes() {
                let first = GoldWithdrawalCommand {
                    command_id: command_id(),
                    cursor: cursor(),
                    account_id: ResourceId::from_u64(7),
                    bar_size_gram: 100,
                    bar_count: 1,
                };
                let second = GoldWithdrawalCommand {
                    bar_count: 2,
                    ..first.clone()
                };

                let first_hash = gold_withdrawal_fingerprint(&first);
                let second_hash = gold_withdrawal_fingerprint(&second);

                assert_ne!(first_hash, second_hash);
                assert_eq!(first_hash.len(), 64);
            }
        }
    }

    mod bond_cash_flow_settlement_rule {
        use super::*;

        mod context_taxable_coupon {
            use super::*;

            #[test]
            fn given_coupon_and_taxable_account_when_planned_then_withholding_reduces_cash_credit()
            {
                let plan = plan_bond_cash_flow_amounts(
                    BondSettlementAccountKind::Taxable,
                    10,
                    95_000,
                    200,
                    0,
                    140_000,
                    14_000,
                )
                .expect("일반계좌 쿠폰 정산을 계산해야 한다");

                assert_eq!(plan.gross_coupon_krw, 2_000);
                assert_eq!(plan.income_tax_krw, 280);
                assert_eq!(plan.local_income_tax_krw, 28);
                assert_eq!(plan.account_credit_krw, 1_692);
                assert_eq!(plan.removed_cost_basis_krw, 0);
            }
        }

        mod context_isa_maturity {
            use super::*;

            #[test]
            fn given_principal_above_basis_when_planned_then_coupon_and_gain_are_tax_profit() {
                let plan = plan_bond_cash_flow_amounts(
                    BondSettlementAccountKind::Isa,
                    10,
                    95_000,
                    200,
                    10_000,
                    0,
                    0,
                )
                .expect("ISA 만기 이익을 계산해야 한다");

                assert_eq!(plan.gross_principal_krw, 100_000);
                assert_eq!(plan.principal_realized_gain_loss_krw, 5_000);
                assert_eq!(plan.isa_tax_profit_delta_krw, 7_000);
                assert_eq!(plan.isa_deductible_loss_delta_krw, 0);
                assert_eq!(plan.account_credit_krw, 102_000);
            }

            #[test]
            fn given_principal_below_basis_when_planned_then_only_coupon_is_profit_and_loss_is_deductible()
             {
                let plan = plan_bond_cash_flow_amounts(
                    BondSettlementAccountKind::Isa,
                    10,
                    105_000,
                    200,
                    10_000,
                    0,
                    0,
                )
                .expect("ISA 만기 손실을 계산해야 한다");

                assert_eq!(plan.principal_realized_gain_loss_krw, -5_000);
                assert_eq!(plan.isa_tax_profit_delta_krw, 2_000);
                assert_eq!(plan.isa_deductible_loss_delta_krw, 5_000);
            }
        }

        mod context_pension_maturity {
            use super::*;

            #[test]
            fn given_coupon_and_principal_when_planned_then_only_coupon_increases_earnings() {
                let plan = plan_bond_cash_flow_amounts(
                    BondSettlementAccountKind::Pension,
                    10,
                    95_000,
                    200,
                    10_000,
                    0,
                    0,
                )
                .expect("연금계좌 만기를 계산해야 한다");

                assert_eq!(plan.pension_earnings_delta_krw, 2_000);
                assert_eq!(plan.gross_principal_krw, 100_000);
                assert_eq!(plan.account_credit_krw, 102_000);
            }
        }

        mod context_no_movement {
            use super::*;

            #[test]
            fn given_zero_position_when_planned_then_no_money_moves() {
                let plan = plan_bond_cash_flow_amounts(
                    BondSettlementAccountKind::Taxable,
                    0,
                    0,
                    200,
                    0,
                    140_000,
                    14_000,
                )
                .expect("0수량 쿠폰을 명시적으로 계산해야 한다");

                assert!(!plan.moves_money());
                assert_eq!(plan.account_credit_krw, 0);
            }
        }

        mod context_strict_payload {
            use super::*;

            #[test]
            fn given_unknown_field_when_parsed_then_payload_is_rejected() {
                let raw = r#"{
                    "version":"v1",
                    "accountId":"1",
                    "seriesId":"2",
                    "productVersionId":"3",
                    "couponKrwPerUnit":100,
                    "principalKrwPerUnit":0,
                    "paymentDate":"2027-01-02",
                    "unknown":true
                }"#;

                let result = parse_bond_cash_flow_payload("bondCoupon", raw);

                assert!(result.is_err());
            }

            #[test]
            fn given_principal_in_coupon_payload_when_parsed_then_payload_is_rejected() {
                let raw = r#"{
                    "version":"v1",
                    "accountId":"1",
                    "seriesId":"2",
                    "productVersionId":"3",
                    "couponKrwPerUnit":100,
                    "principalKrwPerUnit":10000,
                    "paymentDate":"2027-01-02"
                }"#;

                let result = parse_bond_cash_flow_payload("bondCoupon", raw);

                assert!(result.is_err());
            }
        }

        mod context_zero_dirty_price {
            use super::*;

            #[test]
            fn given_maturity_day_zero_price_when_valued_then_zero_market_value_is_allowed() {
                let market_value =
                    checked_product_i64(0, 10).expect("만기일 0원 가격을 평가해야 한다");

                assert_eq!(market_value, 0);
            }
        }
    }

    mod llx_distribution_settlement_rule {
        use super::*;

        fn given_payload() -> LlxDistributionSettlementPayload {
            parse_llx_distribution_payload(
                r#"{
                    "version":"v1",
                    "entitlementId":"11",
                    "accountId":"22",
                    "productVersionId":"33",
                    "paymentDate":"2027-01-02"
                }"#,
            )
            .expect("유효한 LLX 분배금 payload여야 한다")
        }

        fn given_settlement() -> LockedLlxSettlementRow {
            LockedLlxSettlementRow {
                id: 44,
                due_game_day: 366,
                kind: "llxDistribution".to_owned(),
                payload_json: String::new(),
                source_kind: "indexPosition".to_owned(),
                source_id: "11".to_owned(),
                occurrence: 1,
                status: "pending".to_owned(),
            }
        }

        fn given_entitlement() -> LockedLlxEntitlementRow {
            let payload = given_payload();
            LockedLlxEntitlementRow {
                id: payload.entitlement_id.get(),
                financial_account_id: payload.account_id.get(),
                product_version_id: payload.product_version_id.get(),
                payment_game_day: 366,
                payment_date: payload.payment_date.as_date(),
                gross_distribution_krw: 1_000,
                entitlement_status: "pending".to_owned(),
                account_type: "taxableBrokerage".to_owned(),
                account_status: "open".to_owned(),
                account_cash_krw: 10_000,
            }
        }

        mod context_엄격한_v1_payload를_파싱할_때 {
            use super::*;

            #[test]
            fn given_알_수_없는_필드_when_파싱하면_then_거부한다() {
                let raw = r#"{
                    "version":"v1",
                    "entitlementId":"11",
                    "accountId":"22",
                    "productVersionId":"33",
                    "paymentDate":"2027-01-02",
                    "unknown":true
                }"#;

                let result = parse_llx_distribution_payload(raw);

                assert!(result.is_err());
            }

            #[test]
            fn given_v1이_아닌_버전_when_파싱하면_then_거부한다() {
                let raw = r#"{
                    "version":"v2",
                    "entitlementId":"11",
                    "accountId":"22",
                    "productVersionId":"33",
                    "paymentDate":"2027-01-02"
                }"#;

                let result = parse_llx_distribution_payload(raw);

                assert!(result.is_err());
            }
        }

        mod context_스케줄과_권리의_identity를_검증할_때 {
            use super::*;

            #[test]
            fn given_일치하는_identity_when_검증하면_then_허용한다() {
                let settlement = given_settlement();
                let entitlement = given_entitlement();
                let payload = given_payload();

                let result = validate_llx_distribution_identity(&settlement, &entitlement, payload);

                assert!(result.is_ok());
            }

            #[test]
            fn given_source_id가_다를_때_when_검증하면_then_거부한다() {
                let mut settlement = given_settlement();
                settlement.source_id = "12".to_owned();
                let entitlement = given_entitlement();
                let payload = given_payload();

                let result = validate_llx_distribution_identity(&settlement, &entitlement, payload);

                assert!(result.is_err());
            }

            #[test]
            fn given_계좌가_다를_때_when_검증하면_then_거부한다() {
                let settlement = given_settlement();
                let mut entitlement = given_entitlement();
                entitlement.financial_account_id = 23;
                let payload = given_payload();

                let result = validate_llx_distribution_identity(&settlement, &entitlement, payload);

                assert!(result.is_err());
            }

            #[test]
            fn given_상품이_다를_때_when_검증하면_then_거부한다() {
                let settlement = given_settlement();
                let mut entitlement = given_entitlement();
                entitlement.product_version_id = 34;
                let payload = given_payload();

                let result = validate_llx_distribution_identity(&settlement, &entitlement, payload);

                assert!(result.is_err());
            }

            #[test]
            fn given_지급일이_다를_때_when_검증하면_then_거부한다() {
                let settlement = given_settlement();
                let mut entitlement = given_entitlement();
                entitlement.payment_date = CanonicalDate::parse("2027-01-03")
                    .expect("비교할 지급일이어야 한다")
                    .as_date();
                let payload = given_payload();

                let result = validate_llx_distribution_identity(&settlement, &entitlement, payload);

                assert!(result.is_err());
            }
        }

        mod context_정산_상태를_검증할_때 {
            use super::*;

            #[test]
            fn given_허용되지_않은_계좌_when_검증하면_then_거부한다() {
                let account_type = "cma";

                let result = validate_llx_distribution_account_type(account_type);

                assert!(result.is_err());
            }

            #[test]
            fn given_settled와_pending_권리_when_검증하면_then_거부한다() {
                let settlement_status = "settled";
                let entitlement_status = "pending";

                let result = validate_llx_finalized_state(settlement_status, entitlement_status);

                assert!(result.is_err());
            }

            #[test]
            fn given_cancelled와_pending_권리_when_검증하면_then_허용한다() {
                let settlement_status = "cancelled";
                let entitlement_status = "pending";

                let result = validate_llx_finalized_state(settlement_status, entitlement_status);

                assert!(result.is_ok());
            }
        }
    }

    mod strict_policy_parser_rule {
        use super::*;

        mod context_unknown_general_income_field {
            use super::*;

            #[test]
            fn given_unknown_field_when_deserialized_then_policy_is_rejected() {
                let raw = r#"{
                    "incomeTaxPpm": 140000,
                    "localIncomeTaxPpm": 14000,
                    "comprehensiveThresholdKrw": 20000000,
                    "unknown": 1
                }"#;

                let result = serde_json::from_str::<GeneralIncomePolicy>(raw);

                assert!(result.is_err());
            }
        }
    }
}
