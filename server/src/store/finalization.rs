use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result, bail, ensure};
use serde_json::json;
use sqlx::{MySql, Transaction};

use super::tax_accounts::read_tax_account_rules_for_game_day;
use super::types::{PendingInsuranceClaimState, SaveState, WelfarePaymentStatusState};
use crate::finance::{
    FinancialAccountType, IsaAccountKind, IsaCloseTaxInput, PensionTaxLayers,
    PensionWithdrawalPlanInput, PensionWithdrawalRequestKind,
};
use crate::market::MarketDay;
use crate::runs::{LiquidationComponentInput, LiquidationPlan, LiquidationPlanner};

const LIQUIDATION_POLICY_KEY: &str = "m5c-after-tax-liquidation-v1";
const PPM_DENOMINATOR: i128 = 1_000_000;

#[derive(Debug, sqlx::FromRow)]
struct RankedFinalizationAuthority {
    target_game_day: u32,
    ranking_rule_version_id: u64,
    ranking_rule_sha256: String,
    liquidation_policy_key: String,
}

#[derive(Debug, Clone, Copy, Default)]
struct AccountSale {
    gross_krw: i64,
    cost_krw: i64,
    tax_krw: i64,
    cost_basis_krw: i64,
}

#[derive(Debug, Clone, Copy)]
struct SecurityTotals {
    gross_krw: i64,
    cost_krw: i64,
    tax_krw: i64,
}

pub(super) async fn finalize_ranked_run_if_target_in_tx(
    tx: &mut Transaction<'_, MySql>,
    planner: &dyn LiquidationPlanner,
    save: &SaveState,
    market: &MarketDay,
) -> Result<()> {
    let Some(authority) = read_authority(tx, save).await? else {
        return Ok(());
    };
    if save.game_day != authority.target_game_day {
        return Ok(());
    }
    ensure!(
        market.game_day == save.game_day
            && authority.liquidation_policy_key == LIQUIDATION_POLICY_KEY,
        "ranked finalization authority is unsupported"
    );

    let status: Option<(String,)> = sqlx::query_as(
        "SELECT status FROM run_finalization
         WHERE save_id = ? AND run_revision = ? AND target_game_day = ?
           AND ranking_rule_version_id = ? FOR UPDATE",
    )
    .bind(save.save_id)
    .bind(save.run_revision)
    .bind(authority.target_game_day)
    .bind(authority.ranking_rule_version_id)
    .fetch_optional(&mut **tx)
    .await?;
    if status
        .as_ref()
        .is_some_and(|(status,)| status == "completed" || status == "failed")
    {
        return Ok(());
    }

    let finalization_id = ensure_planning_header(tx, save, &authority).await?;
    let plan = match collect_components(tx, save, market)
        .await
        .and_then(|components| {
            planner.plan(
                &authority.liquidation_policy_key,
                authority.target_game_day,
                components,
            )
        }) {
        Ok(plan) => plan,
        Err(error) if has_sqlx_error(&error) => return Err(error),
        Err(_) => {
            fail_finalization(tx, finalization_id, "liquidationPolicyUnsupported").await?;
            return Ok(());
        }
    };
    write_lines(tx, finalization_id, &plan).await?;
    let insolvency_days = count_insolvency_days(tx, save, authority.target_game_day).await?;
    let player_command_count = count_player_commands(tx, save, authority.target_game_day).await?;
    let line_count =
        u32::try_from(plan.lines.len()).context("liquidation line count overflowed")?;
    let update = sqlx::query(
        "UPDATE run_finalization
         SET status = 'completed', after_tax_net_worth_krw = ?, insolvency_days = ?,
             player_command_count = ?, line_count = ?, liquidation_canonical_json = ?,
             completed_at = CURRENT_TIMESTAMP(6)
         WHERE id = ? AND status = 'planning'",
    )
    .bind(plan.after_tax_net_worth_krw)
    .bind(insolvency_days)
    .bind(player_command_count)
    .bind(line_count)
    .bind(&plan.canonical_json)
    .bind(finalization_id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "finalization did not become terminal"
    );
    Ok(())
}

async fn fail_finalization(
    tx: &mut Transaction<'_, MySql>,
    finalization_id: u64,
    failure_code: &str,
) -> Result<()> {
    let update = sqlx::query(
        "UPDATE run_finalization
         SET status = 'failed', line_count = 0, failure_code = ?,
             completed_at = CURRENT_TIMESTAMP(6)
         WHERE id = ? AND status = 'planning'",
    )
    .bind(failure_code)
    .bind(finalization_id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "finalization failure was not recorded"
    );
    Ok(())
}

fn has_sqlx_error(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.downcast_ref::<sqlx::Error>().is_some())
}

async fn read_authority(
    tx: &mut Transaction<'_, MySql>,
    save: &SaveState,
) -> Result<Option<RankedFinalizationAuthority>> {
    sqlx::query_as(
        "SELECT manifest.target_game_day, manifest.ranking_rule_version_id,
                manifest.ranking_rule_sha256, rule.liquidation_policy_key
         FROM run_manifest AS manifest
         INNER JOIN ranking_rule_version AS rule
           ON rule.id = manifest.ranking_rule_version_id
          AND BINARY rule.ranking_rule_sha256 = BINARY manifest.ranking_rule_sha256
         WHERE manifest.save_id = ? AND manifest.run_revision = ?
           AND manifest.ranking_eligible = TRUE",
    )
    .bind(save.save_id)
    .bind(save.run_revision)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read ranked finalization authority")
}

async fn collect_components(
    tx: &mut Transaction<'_, MySql>,
    save: &SaveState,
    market: &MarketDay,
) -> Result<Vec<LiquidationComponentInput>> {
    let wallet_accounts = sum_i64(
        std::iter::once(save.cash_krw).chain(save.accounts.iter().map(|account| account.cash_krw)),
    )?;
    let product_principal = save.active_product_principal_krw()?;
    let earned_receivable = sum_i64(
        save.m2d_assets
            .llx_distribution_entitlements
            .iter()
            .map(|item| item.gross_amount_krw)
            .chain(
                save.life
                    .active_welfare_applications
                    .iter()
                    .filter_map(|application| {
                        application.next_payment.as_ref().and_then(|payment| {
                            (payment.status == WelfarePaymentStatusState::Pending)
                                .then_some(payment.amount_krw)
                        })
                    }),
            )
            .chain(
                save.life
                    .pending_insurance_claims
                    .iter()
                    .filter_map(|claim| match claim {
                        PendingInsuranceClaimState::Ready { payout_krw, .. } => Some(*payout_krw),
                        PendingInsuranceClaimState::Candidate { .. } => None,
                    }),
            ),
    )?;

    let (security, account_sales) = collect_security_values(tx, save, market).await?;
    let tax_account_closure =
        calculate_tax_account_closure(tx, save, market, &account_sales).await?;
    let (property_gross, property_detail) = property_reference_value(tx, save).await?;
    let (corporation_gross, corporation_tax) = corporation_equity(tx, save).await?;

    Ok(vec![
        component(
            "cash.walletAccounts",
            wallet_accounts,
            0,
            0,
            "save+financialAccount:v1",
            json!({
                "accountIds": save.accounts.iter().map(|account| account.id).collect::<Vec<_>>(),
                "saveId": save.save_id.to_string()
            }),
        ),
        component(
            "cash.productPrincipal",
            product_principal,
            0,
            0,
            "cashProductContract:v1",
            json!({
                "contracts": save.cash_contracts.iter().map(|contract| json!({
                    "contractId": contract.contract_id,
                    "productVersionId": contract.product_version_id
                })).collect::<Vec<_>>()
            }),
        ),
        component(
            "receivable.earned",
            earned_receivable,
            0,
            0,
            "earnedReceivable:v1",
            json!({
                "insuranceReadyIds": save.life.pending_insurance_claims.iter().filter_map(|claim| match claim {
                    PendingInsuranceClaimState::Ready { id, .. } => Some(*id),
                    PendingInsuranceClaimState::Candidate { .. } => None
                }).collect::<Vec<_>>(),
                "llxEntitlementIds": save.m2d_assets.llx_distribution_entitlements.iter().map(|item| item.id).collect::<Vec<_>>(),
                "welfareApplicationIds": save.life.active_welfare_applications.iter().map(|item| item.application_id).collect::<Vec<_>>()
            }),
        ),
        component(
            "asset.marketSecurities",
            security.gross_krw,
            security.cost_krw,
            security.tax_krw,
            "marketProductBundle:v1",
            json!({
                "bondSeriesIds": save.m2d_assets.bond_positions.iter().map(|item| item.series_id).collect::<Vec<_>>(),
                "goldProductVersionIds": save.m2d_assets.gold_accounts.iter().map(|item| item.product_version_id).collect::<Vec<_>>(),
                "indexProductVersionId": save.m2d_assets.product_bundle.as_ref().map(|bundle| bundle.index_product.id),
                "marketGameDay": market.game_day,
                "marketOpen": market.market_open
            }),
        ),
        component(
            "tax.accountClosure",
            0,
            0,
            tax_account_closure,
            "taxAccountPolicy:v1",
            json!({
                "isaAccountIds": save.isa_accounts.iter().map(|item| item.account_id).collect::<Vec<_>>(),
                "pensionAccountIds": save.pension_accounts.iter().map(|item| item.account_id).collect::<Vec<_>>()
            }),
        ),
        component(
            "asset.leaseDeposit",
            save.life.tenant_lease_deposit_krw,
            0,
            0,
            "activeLeaseDeposit:v1",
            json!({"activeLeaseId": save.life.active_lease.as_ref().map(|lease| lease.id)}),
        ),
        component(
            "asset.property",
            property_gross,
            0,
            0,
            "propertyReferenceValueCarry:v1",
            property_detail,
        ),
        component(
            "asset.corporationEquity",
            corporation_gross,
            0,
            corporation_tax,
            "corporationEquity:v1",
            json!({"corporationId": save.life.corporation.current.as_ref().map(|corporation| corporation.id)}),
        ),
        component(
            "liability.personal",
            save.debt_krw
                .checked_neg()
                .context("personal debt cannot be negated")?,
            0,
            0,
            "saveDebtProjection:v1",
            json!({
                "debtProjectionKrw": save.debt_krw,
                "essentialArrearIds": save.life.active_arrears.iter().map(|item| item.id).collect::<Vec<_>>(),
                "leaseArrearIds": save.life.active_lease_arrears.iter().map(|item| item.id).collect::<Vec<_>>(),
                "loanIds": save.life.active_loans.iter().map(|loan| loan.id).collect::<Vec<_>>()
            }),
        ),
    ])
}

async fn property_reference_value(
    tx: &mut Transaction<'_, MySql>,
    save: &SaveState,
) -> Result<(i64, serde_json::Value)> {
    let rows: Vec<(u64, i64, i64, i64, u64)> = sqlx::query_as(
        "SELECT holding.id, holding.acquisition_price_krw,
                acquired.price_index_ppm, current_day.price_index_ppm,
                holding.real_estate_model_version_id
         FROM property_holding AS holding
         INNER JOIN real_estate_daily AS acquired
           ON acquired.market_world_id = ?
          AND acquired.real_estate_model_version_id = holding.real_estate_model_version_id
          AND BINARY acquired.region_key = BINARY holding.region_key
          AND acquired.game_day = holding.acquired_game_day
         INNER JOIN real_estate_daily AS current_day
           ON current_day.market_world_id = acquired.market_world_id
          AND current_day.real_estate_model_version_id = acquired.real_estate_model_version_id
          AND BINARY current_day.region_key = BINARY acquired.region_key
          AND current_day.game_day = ?
         WHERE holding.save_id = ? AND holding.run_revision = ? AND holding.status = 'active'
         ORDER BY holding.id",
    )
    .bind(save.market_world_id)
    .bind(save.game_day)
    .bind(save.save_id)
    .bind(save.run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() == save.life.active_property_holdings.len(),
        "active property valuation authority is incomplete"
    );
    let mut values = Vec::with_capacity(rows.len());
    let mut total = 0_i64;
    for (holding_id, acquisition_price, acquisition_index, current_index, model_id) in rows {
        ensure!(
            acquisition_price > 0 && acquisition_index > 0 && current_index > 0,
            "active property valuation input is invalid"
        );
        let value = i64::try_from(
            i128::from(acquisition_price)
                .checked_mul(i128::from(current_index))
                .and_then(|amount| amount.checked_div(i128::from(acquisition_index)))
                .context("property reference value overflowed")?,
        )
        .context("property reference value is out of range")?;
        total = checked_add(total, value)?;
        values.push(json!({
            "acquisitionIndexPpm": acquisition_index,
            "currentIndexPpm": current_index,
            "holdingId": holding_id.to_string(),
            "realEstateModelVersionId": model_id.to_string(),
            "referenceValueKrw": value
        }));
    }
    Ok((
        total,
        json!({"carryRule": "targetDayReferenceValue", "holdings": values}),
    ))
}

async fn collect_security_values(
    tx: &mut Transaction<'_, MySql>,
    save: &SaveState,
    market: &MarketDay,
) -> Result<(SecurityTotals, HashMap<u64, AccountSale>)> {
    let mut by_account = HashMap::<u64, AccountSale>::new();
    let bundle = save.m2d_assets.product_bundle.as_ref();
    for position in &save.positions {
        let gross = mul_i64(position.quantity, market.equity_close_krw)?;
        let terms = bundle.context("LLX position has no pinned product bundle")?;
        add_sale(
            &mut by_account,
            position.account_id.get(),
            gross,
            rate_amount(gross, terms.index_product.sell_fee_ppm)?,
            rate_amount(gross, terms.index_product.sell_tax_ppm)?,
            position.cost_basis_krw,
        )?;
    }

    for position in &save.m2d_assets.bond_positions {
        let (sell_fee_ppm,): (i64,) = sqlx::query_as(
            "SELECT product.sell_fee_ppm FROM bond_series AS series
             INNER JOIN bond_product_version AS product ON product.id = series.product_version_id
             WHERE series.id = ? AND series.market_world_id = ?",
        )
        .bind(position.series_id.get())
        .bind(save.market_world_id)
        .fetch_one(&mut **tx)
        .await?;
        add_sale(
            &mut by_account,
            position.account_id.get(),
            position.market_value_krw,
            rate_amount(position.market_value_krw, sell_fee_ppm)?,
            0,
            position.total_cost_basis_krw,
        )?;
    }

    for position in &save.m2d_assets.gold_accounts {
        let (sell_fee_ppm, sell_tax_ppm): (i64, i64) = sqlx::query_as(
            "SELECT sell_fee_ppm, sell_tax_ppm FROM gold_product_version WHERE id = ?",
        )
        .bind(position.product_version_id.get())
        .fetch_one(&mut **tx)
        .await?;
        add_sale(
            &mut by_account,
            position.account_id.get(),
            position.market_value_krw,
            rate_amount(position.market_value_krw, sell_fee_ppm)?,
            rate_amount(position.market_value_krw, sell_tax_ppm)?,
            position.total_cost_basis_krw,
        )?;
    }

    let physical_gross = sum_i64(
        save.m2d_assets
            .physical_gold_holdings
            .iter()
            .map(|holding| holding.market_value_krw),
    )?;
    let gross_krw = checked_add(
        sum_i64(by_account.values().map(|sale| sale.gross_krw))?,
        physical_gross,
    )?;
    Ok((
        SecurityTotals {
            gross_krw,
            cost_krw: sum_i64(by_account.values().map(|sale| sale.cost_krw))?,
            tax_krw: sum_i64(by_account.values().map(|sale| sale.tax_krw))?,
        },
        by_account,
    ))
}

async fn calculate_tax_account_closure(
    tx: &mut Transaction<'_, MySql>,
    save: &SaveState,
    market: &MarketDay,
    sales: &HashMap<u64, AccountSale>,
) -> Result<i64> {
    let rules = read_tax_account_rules_for_game_day(
        tx,
        save.policy_set.id.get(),
        save.market_world_id,
        save.game_day,
    )
    .await?;
    let mut tax = 0_i64;
    for isa in &save.isa_accounts {
        let sale = sales
            .get(&isa.account_id.get())
            .copied()
            .unwrap_or_default();
        let realized = i128::from(sale.gross_krw)
            .checked_sub(i128::from(sale.cost_krw))
            .and_then(|value| value.checked_sub(i128::from(sale.tax_krw)))
            .and_then(|value| value.checked_sub(i128::from(sale.cost_basis_krw)))
            .context("ISA finalization gain overflowed")?;
        let profit_delta = i64::try_from(realized.max(0)).context("ISA profit is out of range")?;
        let loss_delta = i64::try_from(
            realized
                .min(0)
                .checked_neg()
                .context("ISA loss overflowed")?,
        )
        .context("ISA loss is out of range")?;
        let opened_on =
            market_date_for_game_day(tx, save.market_world_id, isa.opened_game_day).await?;
        let close = rules.isa_close_tax(IsaCloseTaxInput {
            account_kind: match isa.account_type {
                FinancialAccountType::IsaGeneral => IsaAccountKind::General,
                FinancialAccountType::IsaLowIncome => IsaAccountKind::LowIncome,
                _ => bail!("ISA snapshot has a non-ISA account type"),
            },
            opened_on,
            closed_on: market.market_date,
            isa_tax_profit_krw: checked_add(isa.tax_profit_krw, profit_delta)?,
            isa_deductible_loss_krw: checked_add(isa.deductible_loss_krw, loss_delta)?,
            statutory_unavoidable_reason: false,
        })?;
        tax = checked_add(
            tax,
            checked_add(close.income_tax_krw, close.local_income_tax_krw)?,
        )?;
    }
    for pension in &save.pension_accounts {
        let sale = sales
            .get(&pension.account_id.get())
            .copied()
            .unwrap_or_default();
        let loss = checked_add(sale.cost_krw, sale.tax_krw)?;
        let layers = pension_layers_after_loss(pension.tax_layers, loss)?;
        let total = pension_layers_total(layers)?;
        if total == 0 {
            continue;
        }
        let plan = rules.plan_pension_withdrawal(PensionWithdrawalPlanInput {
            layers,
            requested_amount_krw: total,
            request_kind: PensionWithdrawalRequestKind::ExplicitNonPension,
            holder_age_years: 0,
            pension_started: pension.pension_started,
            opened_on: market.market_date,
            current_on: market.market_date,
            pension_receipt_year: None,
            tax_period_opening_value_krw: 0,
            pension_withdrawn_before_request_krw: 0,
            lifetime_contract: false,
            deferred_retirement_non_pension_tax_rate_ppm: 0,
        })?;
        tax = checked_add(tax, plan.tax_krw)?;
    }
    Ok(tax)
}

async fn corporation_equity(
    tx: &mut Transaction<'_, MySql>,
    save: &SaveState,
) -> Result<(i64, i64)> {
    let Some(corporation) = save.life.corporation.current.as_ref() else {
        return Ok((0, 0));
    };
    let net_assets = i128::from(corporation.cash_krw)
        .checked_sub(i128::from(corporation.operating_payable_krw))
        .and_then(|value| value.checked_sub(i128::from(corporation.corporate_tax_payable_krw)))
        .context("corporation net assets overflowed")?
        .max(0);
    let gross = i64::try_from(net_assets).context("corporation net assets are out of range")?;
    let taxable = gross.saturating_sub(corporation.contributed_capital_krw);
    if taxable == 0 {
        return Ok((gross, 0));
    }
    let row: Option<(i64, i64)> = sqlx::query_as(
        "SELECT CAST(JSON_UNQUOTE(JSON_EXTRACT(rule.parameters, '$.incomeTaxRatePpm')) AS SIGNED),
                CAST(JSON_UNQUOTE(JSON_EXTRACT(rule.parameters, '$.localIncomeTaxOnIncomeTaxPpm')) AS SIGNED)
         FROM policy_rule AS rule
         INNER JOIN market_daily AS daily ON daily.world_id = ? AND daily.game_day = ?
         WHERE rule.policy_set_id = ? AND rule.domain = 'corporation'
           AND rule.rule_key = 'residentDividendWithholding'
           AND rule.effective_from <= daily.market_date
           AND (rule.effective_to IS NULL OR rule.effective_to >= daily.market_date)
           AND JSON_LENGTH(rule.parameters) = 5
           AND JSON_EXTRACT(rule.parameters, '$.schemaVersion') = 1
           AND JSON_UNQUOTE(JSON_EXTRACT(rule.parameters, '$.rounding')) = 'floorEachTax'
           AND JSON_UNQUOTE(JSON_EXTRACT(rule.parameters, '$.supportedRecipient'))
                = 'residentIndividual'
         LIMIT 1",
    )
    .bind(save.market_world_id)
    .bind(save.game_day)
    .bind(save.policy_set.id.get())
    .fetch_optional(&mut **tx)
    .await?;
    let (income_rate, local_on_income_rate) =
        row.context("dividend withholding authority is missing")?;
    let income_tax = rate_amount(taxable, income_rate)?;
    let local_tax = rate_amount(income_tax, local_on_income_rate)?;
    Ok((gross, checked_add(income_tax, local_tax)?))
}

async fn ensure_planning_header(
    tx: &mut Transaction<'_, MySql>,
    save: &SaveState,
    authority: &RankedFinalizationAuthority,
) -> Result<u64> {
    sqlx::query(
        "INSERT INTO run_finalization
             (save_id, run_revision, target_game_day, ranking_rule_version_id,
              ranking_rule_sha256, status)
         VALUES (?, ?, ?, ?, ?, 'planning')
         ON DUPLICATE KEY UPDATE id = LAST_INSERT_ID(id)",
    )
    .bind(save.save_id)
    .bind(save.run_revision)
    .bind(authority.target_game_day)
    .bind(authority.ranking_rule_version_id)
    .bind(&authority.ranking_rule_sha256)
    .execute(&mut **tx)
    .await?;
    let (id, status): (u64, String) = sqlx::query_as(
        "SELECT id, status FROM run_finalization
         WHERE save_id = ? AND run_revision = ? AND target_game_day = ?
           AND ranking_rule_version_id = ? FOR UPDATE",
    )
    .bind(save.save_id)
    .bind(save.run_revision)
    .bind(authority.target_game_day)
    .bind(authority.ranking_rule_version_id)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        status == "planning",
        "finalization source is already terminal"
    );
    Ok(id)
}

async fn write_lines(
    tx: &mut Transaction<'_, MySql>,
    finalization_id: u64,
    plan: &LiquidationPlan,
) -> Result<()> {
    let existing: BTreeMap<u32, String> = sqlx::query_as::<_, (u32, String)>(
        "SELECT line_no, line_sha256 FROM liquidation_line
         WHERE run_finalization_id = ? ORDER BY line_no FOR UPDATE",
    )
    .bind(finalization_id)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .collect();
    for line in &plan.lines {
        if let Some(hash) = existing.get(&line.line_no) {
            ensure!(
                hash == &line.canonical_sha256,
                "stored liquidation line diverged"
            );
            continue;
        }
        sqlx::query(
            "INSERT INTO liquidation_line
                 (run_finalization_id, line_no, component_key, gross_krw, cost_krw,
                  tax_krw, net_krw, policy_reference, canonical_line_json)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(finalization_id)
        .bind(line.line_no)
        .bind(&line.component_key)
        .bind(line.gross_krw)
        .bind(line.cost_krw)
        .bind(line.tax_krw)
        .bind(line.net_krw)
        .bind(&line.policy_reference)
        .bind(&line.canonical_json)
        .execute(&mut **tx)
        .await?;
    }
    ensure!(
        existing
            .keys()
            .all(|line_no| plan.lines.iter().any(|line| &line.line_no == line_no)),
        "stored liquidation contains an unexpected line"
    );
    Ok(())
}

async fn count_insolvency_days(
    tx: &mut Transaction<'_, MySql>,
    save: &SaveState,
    target_game_day: u32,
) -> Result<u32> {
    let rows: Vec<(u32, u32, String)> = sqlx::query_as(
        "SELECT game_day, event_order, after_band FROM credit_history
         WHERE save_id = ? AND run_revision = ? AND game_day <= ?
         ORDER BY game_day, event_order",
    )
    .bind(save.save_id)
    .bind(save.run_revision)
    .bind(target_game_day)
    .fetch_all(&mut **tx)
    .await?;
    let mut final_band_by_day = BTreeMap::<u32, String>::new();
    for (game_day, _, after_band) in rows {
        final_band_by_day.insert(game_day, after_band);
    }
    let mut total = 0_u32;
    let mut band = None::<String>;
    let mut cursor = 0_u32;
    for (game_day, next_band) in final_band_by_day {
        if band.as_deref() == Some("insolvent") {
            total = total
                .checked_add(game_day.saturating_sub(cursor.max(1)))
                .context("insolvency day count overflowed")?;
        }
        cursor = game_day;
        band = Some(next_band);
    }
    if band.as_deref() == Some("insolvent") {
        total = total
            .checked_add(
                target_game_day
                    .checked_add(1)
                    .context("target game day overflowed")?
                    .saturating_sub(cursor.max(1)),
            )
            .context("insolvency day count overflowed")?;
    }
    Ok(total.min(target_game_day))
}

async fn count_player_commands(
    tx: &mut Transaction<'_, MySql>,
    save: &SaveState,
    target_game_day: u32,
) -> Result<u64> {
    let (count,): (u64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM command_identity
         WHERE save_id = ? AND initial_run_revision = ? AND initial_game_day <= ?",
    )
    .bind(save.save_id)
    .bind(save.run_revision)
    .bind(target_game_day)
    .fetch_one(&mut **tx)
    .await?;
    Ok(count)
}

async fn market_date_for_game_day(
    tx: &mut Transaction<'_, MySql>,
    market_world_id: u64,
    game_day: u32,
) -> Result<time::Date> {
    let (date,): (time::Date,) =
        sqlx::query_as("SELECT market_date FROM market_daily WHERE world_id = ? AND game_day = ?")
            .bind(market_world_id)
            .bind(game_day)
            .fetch_one(&mut **tx)
            .await?;
    Ok(date)
}

fn component(
    key: &str,
    gross_krw: i64,
    cost_krw: i64,
    tax_krw: i64,
    policy_reference: &str,
    detail: serde_json::Value,
) -> LiquidationComponentInput {
    LiquidationComponentInput {
        component_key: key.to_owned(),
        gross_krw,
        cost_krw,
        tax_krw,
        policy_reference: policy_reference.to_owned(),
        detail,
    }
}

fn add_sale(
    sales: &mut HashMap<u64, AccountSale>,
    account_id: u64,
    gross_krw: i64,
    cost_krw: i64,
    tax_krw: i64,
    cost_basis_krw: i64,
) -> Result<()> {
    let sale = sales.entry(account_id).or_default();
    sale.gross_krw = checked_add(sale.gross_krw, gross_krw)?;
    sale.cost_krw = checked_add(sale.cost_krw, cost_krw)?;
    sale.tax_krw = checked_add(sale.tax_krw, tax_krw)?;
    sale.cost_basis_krw = checked_add(sale.cost_basis_krw, cost_basis_krw)?;
    Ok(())
}

fn pension_layers_after_loss(
    mut layers: PensionTaxLayers,
    loss_krw: i64,
) -> Result<PensionTaxLayers> {
    ensure!(loss_krw >= 0, "pension liquidation loss is negative");
    let mut remaining = loss_krw;
    for layer in [
        &mut layers.earnings_krw,
        &mut layers.credited_contribution_krw,
        &mut layers.deferred_retirement_income_krw,
        &mut layers.tax_excluded_contribution_krw,
    ] {
        let consumed = (*layer).min(remaining);
        *layer = layer
            .checked_sub(consumed)
            .context("pension layer underflowed")?;
        remaining = remaining
            .checked_sub(consumed)
            .context("pension loss underflowed")?;
    }
    ensure!(
        remaining == 0,
        "pension liquidation costs exceed account value"
    );
    Ok(layers)
}

fn pension_layers_total(layers: PensionTaxLayers) -> Result<i64> {
    sum_i64([
        layers.tax_excluded_contribution_krw,
        layers.deferred_retirement_income_krw,
        layers.credited_contribution_krw,
        layers.earnings_krw,
    ])
}

fn rate_amount(amount_krw: i64, rate_ppm: i64) -> Result<i64> {
    ensure!(
        amount_krw >= 0 && (0..=1_000_000).contains(&rate_ppm),
        "invalid rate input"
    );
    i64::try_from(
        i128::from(amount_krw)
            .checked_mul(i128::from(rate_ppm))
            .and_then(|value| value.checked_div(PPM_DENOMINATOR))
            .context("rate calculation overflowed")?,
    )
    .context("rate result is out of range")
}

fn mul_i64(quantity: u32, price_krw: i64) -> Result<i64> {
    i64::try_from(
        i128::from(quantity)
            .checked_mul(i128::from(price_krw))
            .context("market value overflowed")?,
    )
    .context("market value is out of range")
}

fn checked_add(left: i64, right: i64) -> Result<i64> {
    i64::try_from(
        i128::from(left)
            .checked_add(i128::from(right))
            .context("money sum overflowed")?,
    )
    .context("money sum is out of range")
}

fn sum_i64(values: impl IntoIterator<Item = i64>) -> Result<i64> {
    let total = values.into_iter().try_fold(0_i128, |total, value| {
        total
            .checked_add(i128::from(value))
            .context("money sum overflowed")
    })?;
    i64::try_from(total).context("money sum is out of range")
}
