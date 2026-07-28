//! C4 property-tax assessment, payment, and history persistence.

use anyhow::{Context, Result, bail, ensure};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sqlx::{MySql, MySqlPool, Transaction};
use time::{Date, Duration, Month};

use super::types::{
    PropertyTaxComponentState, PropertyTaxEventKindState, PropertyTaxEventPageQuery,
    PropertyTaxEventPageState, PropertyTaxEventState, PropertyTaxEventStatusState,
    PropertyTaxPaymentState, PropertyTaxPaymentStatusState,
};
use crate::finance::{
    FinanceRules, LedgerAccountCode, LedgerPosting, LedgerSource, LedgerSourceKind,
    LedgerTransaction, LedgerTransactionDraft, ResourceId, RunId, RunPolicyContext,
    ScheduledSettlement, SettlementKind, SettlementSource, SettlementSourceKind, SettlementStatus,
};
use crate::life::{
    AnnualPropertyTaxFairMarketRatioBand, AnnualPropertyTaxInput,
    AnnualPropertyTaxOwnershipCutoffRule, AnnualPropertyTaxPaymentSplitRule,
    AnnualPropertyTaxPolicy, AnnualPropertyTaxRateBracket, AnnualPropertyTaxRateSchedule,
    CapitalGainsTaxPaymentRule, CapitalGainsTaxRateBracket, CapitalGainsTaxScope,
    OneHomeCapitalGainsTaxCalculation, OneHomeCapitalGainsTaxInput, PropertyAcquisitionTaxInput,
    PropertyAcquisitionTaxPolicy, PropertyCapitalGainsTaxPolicy, PropertyRules,
    PropertySaleReferenceValueInput, PropertyTaxError, PropertyTaxPolicy, PropertyTaxRoundingRule,
    PropertyTaxRules,
};

const MAX_PROPERTY_TAX_HISTORY_PAGE_SIZE: u8 = 20;
const PROPERTY_TAX_SETTLEMENT_PAYLOAD_VERSION: u8 = 1;
const C4_REAL_ESTATE_VERSION_KEY: &str = "dev-unranked-m4-real-estate-sale-tax-2026-v6";

#[derive(Debug, Clone, Copy)]
pub(super) struct PropertyTaxRunContext {
    pub save_id: u64,
    pub market_world_id: u64,
    pub policy_set_id: u64,
    pub run_revision: u32,
    pub game_day: u32,
    pub market_date: Date,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PropertyTaxPaymentSettlementPayload {
    version: u8,
    property_tax_event_id: String,
    property_tax_payment_id: String,
    payment_no: u8,
}

struct LoadedPropertyTaxPolicy {
    acquisition_rule_id: u64,
    acquisition_legal_basis_date: Date,
    annual_rule_id: u64,
    annual_legal_basis_date: Date,
    capital_gains_rule_id: u64,
    capital_gains_legal_basis_date: Date,
    annual_exclusion_codes: Vec<String>,
    policy: PropertyTaxPolicy,
}

#[derive(Debug, sqlx::FromRow)]
struct AcquisitionPolicyRow {
    rule_id: u64,
    legal_basis_date: Date,
    supported_home_count: u8,
    lower_price_maximum_krw: i64,
    middle_price_maximum_krw: i64,
    lower_rate_ppm: u32,
    upper_rate_ppm: u32,
    middle_rate_price_divisor_krw: i64,
    middle_rate_offset_ppm: u32,
    middle_rate_rounding: String,
    local_education_rate_ratio_ppm: u32,
    payment_due_days: u16,
}

#[derive(Debug, sqlx::FromRow)]
struct AnnualPolicyRow {
    rule_id: u64,
    legal_basis_date: Date,
    supported_home_count: u8,
    assessment_month: u8,
    assessment_day: u8,
    ownership_cutoff_rule: String,
    official_value_ratio_ppm: u32,
    special_rate_official_value_maximum_krw: i64,
    local_education_rate_ratio_ppm: u32,
    first_payment_month: u8,
    first_payment_day: u8,
    second_payment_month: u8,
    second_payment_day: u8,
    payment_split_rule: String,
    unsupported_exclusion_codes_json: String,
}

#[derive(Debug, sqlx::FromRow)]
struct AnnualRatioBandRow {
    band_order: u8,
    official_value_upper_bound_krw: Option<i64>,
    fair_market_value_ratio_ppm: u32,
}

#[derive(Debug, sqlx::FromRow)]
struct AnnualRateBracketRow {
    rate_schedule: String,
    bracket_order: u8,
    tax_base_upper_bound_krw: Option<i64>,
    rate_ppm: u32,
    progressive_deduction_krw: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct CapitalGainsPolicyRow {
    rule_id: u64,
    legal_basis_date: Date,
    supported_home_count: u8,
    high_value_threshold_krw: i64,
    basic_deduction_krw: i64,
    minimum_holding_years: u16,
    minimum_residence_years: u16,
    holding_deduction_start_years: u16,
    holding_deduction_start_rate_ppm: u32,
    holding_deduction_per_year_ppm: u32,
    holding_deduction_maximum_ppm: u32,
    residence_deduction_start_years: u16,
    residence_deduction_start_rate_ppm: u32,
    residence_deduction_per_year_ppm: u32,
    residence_deduction_maximum_ppm: u32,
    local_income_tax_ratio_ppm: u32,
    payment_rule: String,
}

#[derive(Debug, sqlx::FromRow)]
struct CapitalGainsRateBracketRow {
    tax_scope: String,
    bracket_order: u8,
    taxable_amount_upper_bound_krw: Option<i64>,
    rate_ppm: u32,
    progressive_deduction_krw: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct AnnualAssessmentHoldingRow {
    household_id: u64,
    holding_id: u64,
    acquisition_price_krw: i64,
    acquisition_price_index_ppm: i64,
    valuation_price_index_ppm: i64,
}

pub(super) struct AcquisitionPropertyTaxEventInput {
    pub context: PropertyTaxRunContext,
    pub household_id: u64,
    pub holding_id: u64,
    pub purchase_price_krw: i64,
    pub valuation_price_index_ppm: i64,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CapitalGainsPropertyTaxEventInput {
    pub context: PropertyTaxRunContext,
    pub household_id: u64,
    pub holding_id: u64,
    pub property_sale_execution_id: u64,
    pub sale_price_krw: i64,
    pub acquisition_price_krw: i64,
    pub acquisition_incidental_cost_krw: i64,
    pub acquisition_taxes_krw: i64,
    pub disposition_cost_krw: i64,
    pub acquired_on: Date,
    pub owner_occupied_from: Date,
    pub valuation_price_index_ppm: i64,
}

pub(super) struct CapitalGainsPropertyTaxEventResult {
    pub event_id: ResourceId,
    pub calculation: OneHomeCapitalGainsTaxCalculation,
}

pub(super) async fn calculate_capital_gains_property_tax_in_tx(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn PropertyTaxRules,
    input: CapitalGainsPropertyTaxEventInput,
) -> Result<std::result::Result<OneHomeCapitalGainsTaxCalculation, PropertyTaxError>> {
    let loaded = load_property_tax_policy(tx, input.context.policy_set_id).await?;
    rules
        .validate_policy(&loaded.policy)
        .context("capital-gains property tax policy validation failed")?;
    Ok(
        rules.calculate_one_home_capital_gains_tax(OneHomeCapitalGainsTaxInput {
            sale_price_krw: input.sale_price_krw,
            acquisition_price_krw: input.acquisition_price_krw,
            acquisition_incidental_cost_krw: input.acquisition_incidental_cost_krw,
            acquisition_taxes_krw: input.acquisition_taxes_krw,
            disposition_cost_krw: input.disposition_cost_krw,
            acquired_on: input.acquired_on,
            owner_occupied_from: input.owner_occupied_from,
            sold_on: input.context.market_date,
            household_home_count: 1,
            policy: &loaded.policy.capital_gains,
        }),
    )
}

#[derive(Debug, sqlx::FromRow)]
struct PropertyTaxEventRow {
    id: u64,
    property_holding_id: u64,
    policy_set_id: u64,
    policy_key: String,
    policy_rule_id: u64,
    rule_key: String,
    legal_basis_date: Date,
    event_kind: String,
    status: String,
    tax_year: Option<u16>,
    assessment_game_day: u32,
    taxable_game_day: u32,
    paid_game_day: Option<u32>,
    household_home_count: u8,
    valuation_game_day: Option<u32>,
    valuation_price_index_ppm: Option<i64>,
    valuation_amount_krw: i64,
    official_value_krw: Option<i64>,
    tax_base_krw: i64,
    deduction_krw: i64,
    total_tax_krw: i64,
    paid_tax_krw: i64,
    exclusion_codes_json: String,
}

#[derive(Debug, sqlx::FromRow)]
struct PropertyTaxComponentRow {
    component_order: u8,
    component_kind: String,
    tax_base_krw: i64,
    deduction_krw: i64,
    taxable_amount_krw: i64,
    rate_ppm: u32,
    progressive_deduction_krw: i64,
    tax_amount_krw: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct PropertyTaxPaymentRow {
    payment_no: u8,
    due_game_day: u32,
    paid_game_day: Option<u32>,
    amount_krw: i64,
    status: String,
    paid_from_wallet_krw: i64,
    obligated_amount_krw: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct PropertyTaxSettlementEnvelopeRow {
    id: u64,
    due_game_day: u32,
    kind: String,
    payload_json: String,
    source_kind: String,
    source_id: String,
    occurrence: u64,
    status: String,
}

#[derive(Debug, sqlx::FromRow)]
struct DuePropertyTaxPaymentRow {
    property_tax_event_id: u64,
    payment_no: u8,
    due_game_day: u32,
    amount_krw: i64,
    status: String,
    scheduled_settlement_id: Option<u64>,
}

#[derive(Debug, sqlx::FromRow)]
struct DuePropertyTaxEventRow {
    household_id: u64,
    policy_set_id: u64,
    total_tax_krw: i64,
    paid_tax_krw: i64,
    status: String,
}

async fn load_property_tax_policy(
    tx: &mut Transaction<'_, MySql>,
    policy_set_id: u64,
) -> Result<LoadedPropertyTaxPolicy> {
    let acquisition: AcquisitionPolicyRow = sqlx::query_as(
        "SELECT profile.rule_id, rule.effective_from AS legal_basis_date,
                profile.supported_home_count,
                profile.lower_price_maximum_krw, profile.middle_price_maximum_krw,
                profile.lower_rate_ppm, profile.upper_rate_ppm,
                profile.middle_rate_price_divisor_krw,
                profile.middle_rate_offset_ppm, profile.middle_rate_rounding,
                profile.local_education_rate_ratio_ppm, profile.payment_due_days
         FROM property_acquisition_tax_policy_profile AS profile
         INNER JOIN policy_set AS policy ON policy.id = profile.policy_set_id
         INNER JOIN policy_rule AS rule
           ON rule.id = profile.rule_id AND rule.policy_set_id = profile.policy_set_id
          AND rule.domain = 'propertyTax' AND rule.rule_key = 'singleHomeAcquisitionTax'
         WHERE profile.policy_set_id = ? AND policy.sealed_at IS NOT NULL",
    )
    .bind(policy_set_id)
    .fetch_one(&mut **tx)
    .await
    .context("property acquisition-tax policy is missing")?;
    let annual: AnnualPolicyRow = sqlx::query_as(
        "SELECT profile.rule_id, rule.effective_from AS legal_basis_date,
                profile.supported_home_count, profile.assessment_month,
                profile.assessment_day,
                ownership_cutoff_rule, official_value_ratio_ppm,
                special_rate_official_value_maximum_krw,
                local_education_rate_ratio_ppm, first_payment_month,
                first_payment_day, second_payment_month, second_payment_day,
                payment_split_rule,
                CAST(unsupported_exclusion_codes AS CHAR CHARACTER SET utf8mb4)
                    AS unsupported_exclusion_codes_json
         FROM property_annual_tax_policy_profile AS profile
         INNER JOIN policy_rule AS rule
           ON rule.id = profile.rule_id AND rule.policy_set_id = profile.policy_set_id
          AND rule.domain = 'propertyTax' AND rule.rule_key = 'singleHomeAnnualPropertyTax'
         WHERE profile.policy_set_id = ?",
    )
    .bind(policy_set_id)
    .fetch_one(&mut **tx)
    .await
    .context("annual property-tax policy is missing")?;
    let ratio_rows: Vec<AnnualRatioBandRow> = sqlx::query_as(
        "SELECT band_order, official_value_upper_bound_krw,
                fair_market_value_ratio_ppm
         FROM property_annual_tax_fair_market_ratio_band
         WHERE policy_set_id = ? ORDER BY band_order",
    )
    .bind(policy_set_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        !ratio_rows.is_empty()
            && ratio_rows
                .iter()
                .enumerate()
                .all(|(index, row)| usize::from(row.band_order) == index + 1),
        "annual property-tax fair-market bands are not canonical"
    );
    let rate_rows: Vec<AnnualRateBracketRow> = sqlx::query_as(
        "SELECT rate_schedule, bracket_order, tax_base_upper_bound_krw,
                rate_ppm, progressive_deduction_krw
         FROM property_annual_tax_rate_bracket
         WHERE policy_set_id = ?
         ORDER BY CASE rate_schedule WHEN 'special' THEN 1 ELSE 2 END, bracket_order",
    )
    .bind(policy_set_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        !rate_rows.is_empty(),
        "annual property-tax rate brackets are missing"
    );
    let capital: CapitalGainsPolicyRow = sqlx::query_as(
        "SELECT profile.rule_id, rule.effective_from AS legal_basis_date,
                profile.supported_home_count, profile.high_value_threshold_krw,
                basic_deduction_krw, minimum_holding_years,
                minimum_residence_years, holding_deduction_start_years,
                holding_deduction_start_rate_ppm,
                holding_deduction_per_year_ppm, holding_deduction_maximum_ppm,
                residence_deduction_start_years,
                residence_deduction_start_rate_ppm,
                residence_deduction_per_year_ppm,
                residence_deduction_maximum_ppm, local_income_tax_ratio_ppm,
                payment_rule
         FROM property_capital_gains_tax_policy_profile AS profile
         INNER JOIN policy_rule AS rule
           ON rule.id = profile.rule_id AND rule.policy_set_id = profile.policy_set_id
          AND rule.domain = 'propertyTax' AND rule.rule_key = 'singleHomeCapitalGainsTax'
         WHERE profile.policy_set_id = ?",
    )
    .bind(policy_set_id)
    .fetch_one(&mut **tx)
    .await
    .context("property capital-gains-tax policy is missing")?;
    let capital_rate_rows: Vec<CapitalGainsRateBracketRow> = sqlx::query_as(
        "SELECT tax_scope, bracket_order, taxable_amount_upper_bound_krw,
                rate_ppm, progressive_deduction_krw
         FROM property_capital_gains_tax_rate_bracket
         WHERE policy_set_id = ?
         ORDER BY CASE tax_scope WHEN 'national' THEN 1 ELSE 2 END, bracket_order",
    )
    .bind(policy_set_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        !capital_rate_rows.is_empty(),
        "property capital-gains-tax brackets are missing"
    );
    let annual_exclusion_codes: Vec<String> =
        serde_json::from_str(&annual.unsupported_exclusion_codes_json)
            .context("annual property-tax exclusions are invalid")?;
    ensure!(
        annual_exclusion_codes.iter().all(|code| !code.is_empty()),
        "annual property-tax exclusion code is empty"
    );
    let fair_market_ratio_bands = ratio_rows
        .into_iter()
        .map(|row| AnnualPropertyTaxFairMarketRatioBand {
            official_value_upper_bound_krw: row.official_value_upper_bound_krw,
            fair_market_value_ratio_ppm: i64::from(row.fair_market_value_ratio_ppm),
        })
        .collect();
    let rate_brackets = rate_rows
        .into_iter()
        .map(|row| {
            let rate_schedule = match row.rate_schedule.as_str() {
                "special" => AnnualPropertyTaxRateSchedule::Special,
                "standard" => AnnualPropertyTaxRateSchedule::Standard,
                _ => bail!("annual property-tax rate schedule is invalid"),
            };
            ensure!(
                row.bracket_order > 0,
                "annual property-tax bracket order is zero"
            );
            Ok(AnnualPropertyTaxRateBracket {
                rate_schedule,
                tax_base_upper_bound_krw: row.tax_base_upper_bound_krw,
                rate_ppm: i64::from(row.rate_ppm),
                progressive_deduction_krw: row.progressive_deduction_krw,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let capital_rate_brackets = capital_rate_rows
        .into_iter()
        .map(|row| {
            let tax_scope = match row.tax_scope.as_str() {
                "national" => CapitalGainsTaxScope::National,
                "local" => CapitalGainsTaxScope::Local,
                _ => bail!("capital-gains-tax scope is invalid"),
            };
            ensure!(
                row.bracket_order > 0,
                "capital-gains-tax bracket order is zero"
            );
            Ok(CapitalGainsTaxRateBracket {
                tax_scope,
                taxable_amount_upper_bound_krw: row.taxable_amount_upper_bound_krw,
                rate_ppm: i64::from(row.rate_ppm),
                progressive_deduction_krw: row.progressive_deduction_krw,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let policy = PropertyTaxPolicy {
        acquisition: PropertyAcquisitionTaxPolicy {
            supported_home_count: acquisition.supported_home_count,
            lower_price_maximum_krw: acquisition.lower_price_maximum_krw,
            middle_price_maximum_krw: acquisition.middle_price_maximum_krw,
            lower_rate_ppm: i64::from(acquisition.lower_rate_ppm),
            upper_rate_ppm: i64::from(acquisition.upper_rate_ppm),
            middle_rate_price_divisor_krw: acquisition.middle_rate_price_divisor_krw,
            middle_rate_offset_ppm: i64::from(acquisition.middle_rate_offset_ppm),
            middle_rate_rounding: match acquisition.middle_rate_rounding.as_str() {
                "halfUp" => PropertyTaxRoundingRule::HalfUp,
                _ => bail!("property acquisition-tax rounding rule is invalid"),
            },
            local_education_rate_ratio_ppm: i64::from(acquisition.local_education_rate_ratio_ppm),
            payment_due_days: acquisition.payment_due_days,
        },
        annual: AnnualPropertyTaxPolicy {
            supported_home_count: annual.supported_home_count,
            assessment_month: annual.assessment_month,
            assessment_day: annual.assessment_day,
            ownership_cutoff_rule: match annual.ownership_cutoff_rule.as_str() {
                "priorDayClosingOwner" => {
                    AnnualPropertyTaxOwnershipCutoffRule::PriorDayClosingOwner
                }
                _ => bail!("annual property-tax ownership cutoff is invalid"),
            },
            official_value_ratio_ppm: i64::from(annual.official_value_ratio_ppm),
            fair_market_ratio_bands,
            special_rate_official_value_maximum_krw: annual.special_rate_official_value_maximum_krw,
            rate_brackets,
            local_education_rate_ratio_ppm: i64::from(annual.local_education_rate_ratio_ppm),
            first_payment_month: annual.first_payment_month,
            first_payment_day: annual.first_payment_day,
            second_payment_month: annual.second_payment_month,
            second_payment_day: annual.second_payment_day,
            payment_split_rule: match annual.payment_split_rule.as_str() {
                "floorHalfThenRemainder" => {
                    AnnualPropertyTaxPaymentSplitRule::FloorHalfThenRemainder
                }
                _ => bail!("annual property-tax payment split is invalid"),
            },
        },
        capital_gains: PropertyCapitalGainsTaxPolicy {
            supported_home_count: capital.supported_home_count,
            high_value_threshold_krw: capital.high_value_threshold_krw,
            basic_deduction_krw: capital.basic_deduction_krw,
            minimum_holding_years: capital.minimum_holding_years,
            minimum_residence_years: capital.minimum_residence_years,
            holding_deduction_start_years: capital.holding_deduction_start_years,
            holding_deduction_start_rate_ppm: i64::from(capital.holding_deduction_start_rate_ppm),
            holding_deduction_per_year_ppm: i64::from(capital.holding_deduction_per_year_ppm),
            holding_deduction_maximum_ppm: i64::from(capital.holding_deduction_maximum_ppm),
            residence_deduction_start_years: capital.residence_deduction_start_years,
            residence_deduction_start_rate_ppm: i64::from(
                capital.residence_deduction_start_rate_ppm,
            ),
            residence_deduction_per_year_ppm: i64::from(capital.residence_deduction_per_year_ppm),
            residence_deduction_maximum_ppm: i64::from(capital.residence_deduction_maximum_ppm),
            local_income_tax_ratio_ppm: i64::from(capital.local_income_tax_ratio_ppm),
            rate_brackets: capital_rate_brackets,
            payment_rule: match capital.payment_rule.as_str() {
                "withheldAtSale" => CapitalGainsTaxPaymentRule::WithheldAtSale,
                _ => bail!("capital-gains-tax payment rule is invalid"),
            },
        },
    };
    Ok(LoadedPropertyTaxPolicy {
        acquisition_rule_id: acquisition.rule_id,
        acquisition_legal_basis_date: acquisition.legal_basis_date,
        annual_rule_id: annual.rule_id,
        annual_legal_basis_date: annual.legal_basis_date,
        capital_gains_rule_id: capital.rule_id,
        capital_gains_legal_basis_date: capital.legal_basis_date,
        annual_exclusion_codes,
        policy,
    })
}

async fn is_c4_property_tax_run(
    tx: &mut Transaction<'_, MySql>,
    context: PropertyTaxRunContext,
) -> Result<bool> {
    let scope: Option<(String, bool, bool, bool)> = sqlx::query_as(
        "SELECT model.version_key,
                EXISTS(
                    SELECT 1
                    FROM property_acquisition_tax_policy_profile AS acquisition
                    INNER JOIN property_annual_tax_policy_profile AS annual
                      ON annual.policy_set_id = acquisition.policy_set_id
                    INNER JOIN property_capital_gains_tax_policy_profile AS capital
                      ON capital.policy_set_id = acquisition.policy_set_id
                    WHERE acquisition.policy_set_id = policy.id
                ) AS has_property_tax_policy,
                model.sealed_at IS NOT NULL, policy.sealed_at IS NOT NULL
         FROM run_rule_bundle AS bundle
         INNER JOIN real_estate_model_version AS model
           ON model.id = bundle.real_estate_model_version_id
         INNER JOIN policy_set AS policy ON policy.id = bundle.policy_set_id
         WHERE bundle.save_id = ? AND bundle.run_revision = ?
           AND bundle.market_world_id = ? AND bundle.policy_set_id = ?",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(context.market_world_id)
    .bind(context.policy_set_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((model_key, has_property_tax_policy, model_sealed, policy_sealed)) = scope else {
        return Ok(false);
    };
    let c4_model = model_key == C4_REAL_ESTATE_VERSION_KEY;
    ensure!(
        c4_model == has_property_tax_policy,
        "property-tax model and policy versions are not paired"
    );
    if !c4_model {
        return Ok(false);
    }
    ensure!(
        model_sealed && policy_sealed,
        "C4 property-tax run is not sealed"
    );
    Ok(true)
}

pub(super) async fn create_acquisition_property_tax_event_in_tx(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn PropertyTaxRules,
    input: AcquisitionPropertyTaxEventInput,
) -> Result<ResourceId> {
    let loaded = load_property_tax_policy(tx, input.context.policy_set_id).await?;
    rules
        .validate_policy(&loaded.policy)
        .context("property tax policy validation failed")?;
    let calculation = rules
        .calculate_acquisition_tax(PropertyAcquisitionTaxInput {
            purchase_price_krw: input.purchase_price_krw,
            household_home_count: 1,
            policy: &loaded.policy.acquisition,
        })
        .context("property acquisition-tax calculation failed")?;
    ensure!(
        calculation.total_tax_krw > 0
            && calculation.payment_due_days == loaded.policy.acquisition.payment_due_days,
        "property acquisition-tax calculation is not schedulable"
    );
    let inserted = sqlx::query(
        "INSERT INTO property_tax_event
             (save_id, run_revision, household_id, property_holding_id,
              policy_set_id, policy_rule_id, event_kind, status, tax_year,
              legal_basis_date, assessment_game_day, taxable_game_day,
              household_home_count, valuation_game_day,
              valuation_price_index_ppm, valuation_amount_krw,
              official_value_krw, tax_base_krw, deduction_krw,
              total_tax_krw, paid_tax_krw, exclusion_codes, property_sale_execution_id)
         VALUES (?, ?, ?, ?, ?, ?, 'acquisition', 'prepared', ?, ?, ?, ?, 1,
                 ?, ?, ?, NULL, ?, 0, ?, 0, JSON_ARRAY(), NULL)",
    )
    .bind(input.context.save_id)
    .bind(input.context.run_revision)
    .bind(input.household_id)
    .bind(input.holding_id)
    .bind(input.context.policy_set_id)
    .bind(loaded.acquisition_rule_id)
    .bind(input.context.market_date.year())
    .bind(loaded.acquisition_legal_basis_date)
    .bind(input.context.game_day)
    .bind(input.context.game_day)
    .bind(input.context.game_day)
    .bind(input.valuation_price_index_ppm)
    .bind(input.purchase_price_krw)
    .bind(calculation.tax_base_krw)
    .bind(calculation.total_tax_krw)
    .execute(&mut **tx)
    .await?;
    let event_id = inserted.last_insert_id();
    ensure!(
        event_id > 0,
        "property acquisition-tax event has no identity"
    );
    insert_property_tax_component(
        tx,
        input.context,
        event_id,
        1,
        "acquisitionTax",
        calculation.tax_base_krw,
        0,
        calculation.tax_base_krw,
        calculation.acquisition_tax_rate_ppm,
        0,
        calculation.acquisition_tax_krw,
        serde_json::to_value(calculation)?,
    )
    .await?;
    let local_rate_ppm = i64::try_from(
        i128::from(calculation.acquisition_tax_rate_ppm)
            .checked_mul(i128::from(calculation.local_education_rate_ratio_ppm))
            .and_then(|value| value.checked_div(1_000_000))
            .context("property acquisition local-education rate overflowed")?,
    )?;
    insert_property_tax_component(
        tx,
        input.context,
        event_id,
        2,
        "acquisitionLocalEducationTax",
        calculation.tax_base_krw,
        0,
        calculation.tax_base_krw,
        local_rate_ppm,
        0,
        calculation.local_education_tax_krw,
        serde_json::to_value(calculation)?,
    )
    .await?;
    let due_game_day = input
        .context
        .game_day
        .checked_add(u32::from(calculation.payment_due_days))
        .context("property acquisition-tax due game day overflowed")?;
    let due_date = input
        .context
        .market_date
        .checked_add(Duration::days(i64::from(calculation.payment_due_days)))
        .context("property acquisition-tax due date overflowed")?;
    let world_start_date = input
        .context
        .market_date
        .checked_sub(Duration::days(i64::from(input.context.game_day)))
        .context("property acquisition-tax world start date overflowed")?;
    ensure!(
        game_day_for_date(world_start_date, due_date)? == due_game_day,
        "property acquisition-tax legal and game due dates disagree"
    );
    insert_property_tax_payment_schedule(
        tx,
        input.context,
        event_id,
        1,
        due_game_day,
        calculation.total_tax_krw,
    )
    .await?;
    transition_prepared_property_tax_event(tx, input.context, event_id, "scheduled", 0).await?;
    Ok(ResourceId::from_u64(event_id))
}

pub(super) async fn prepare_annual_property_tax_boundary_in_tx(
    tx: &mut Transaction<'_, MySql>,
    property_rules: &dyn PropertyRules,
    tax_rules: &dyn PropertyTaxRules,
    context: PropertyTaxRunContext,
) -> Result<()> {
    if !is_c4_property_tax_run(tx, context).await? {
        return Ok(());
    }
    let loaded = load_property_tax_policy(tx, context.policy_set_id).await?;
    tax_rules
        .validate_policy(&loaded.policy)
        .context("annual property-tax policy validation failed")?;
    if context.market_date.month() as u8 != loaded.policy.annual.assessment_month
        || context.market_date.day() != loaded.policy.annual.assessment_day
    {
        return Ok(());
    }
    ensure!(
        context.game_day > 0,
        "annual property-tax assessment has no prior closing day"
    );
    let prior_game_day = context
        .game_day
        .checked_sub(1)
        .context("annual property-tax prior day underflowed")?;
    let save_game_day: u32 = sqlx::query_scalar(
        "SELECT game_day FROM save
         WHERE id = ? AND run_revision = ? AND policy_set_id = ? FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(context.policy_set_id)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        save_game_day == prior_game_day,
        "annual property-tax assessment is not using prior-close state"
    );
    let household_ids: Vec<(u64,)> = sqlx::query_as(
        "SELECT id FROM household
         WHERE save_id = ? AND run_revision = ? ORDER BY id FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        household_ids.len() == 1,
        "annual property-tax assessment requires one household"
    );
    let household_id = household_ids[0].0;
    let _: Vec<(u64,)> = sqlx::query_as(
        "SELECT id FROM household_member
         WHERE save_id = ? AND run_revision = ? AND household_id = ?
         ORDER BY id FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(household_id)
    .fetch_all(&mut **tx)
    .await?;
    let residence_ids: Vec<(u64,)> = sqlx::query_as(
        "SELECT id FROM residence
         WHERE save_id = ? AND run_revision = ? AND household_id = ?
           AND effective_from_game_day <= ?
           AND (effective_to_game_day IS NULL OR effective_to_game_day > ?)
         ORDER BY id FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(household_id)
    .bind(prior_game_day)
    .bind(prior_game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        residence_ids.len() == 1,
        "annual property-tax assessment requires one prior-close residence"
    );
    let holdings: Vec<AnnualAssessmentHoldingRow> = sqlx::query_as(
        "SELECT holding.household_id, holding.id AS holding_id,
                holding.acquisition_price_krw,
                acquisition_daily.price_index_ppm AS acquisition_price_index_ppm,
                valuation_daily.price_index_ppm AS valuation_price_index_ppm
         FROM property_holding AS holding
         INNER JOIN residence
           ON residence.save_id = holding.save_id
          AND residence.run_revision = holding.run_revision
          AND residence.household_id = holding.household_id
          AND residence.property_holding_id = holding.id
          AND residence.tenure_type = 'owner'
          AND residence.effective_from_game_day <= ?
          AND (residence.effective_to_game_day IS NULL OR residence.effective_to_game_day > ?)
         INNER JOIN run_rule_bundle AS bundle
           ON bundle.save_id = holding.save_id AND bundle.run_revision = holding.run_revision
          AND bundle.real_estate_model_version_id = holding.real_estate_model_version_id
         INNER JOIN real_estate_daily AS acquisition_daily
           ON acquisition_daily.market_world_id = bundle.market_world_id
          AND acquisition_daily.real_estate_model_version_id = holding.real_estate_model_version_id
          AND BINARY acquisition_daily.region_key = BINARY holding.region_key
          AND acquisition_daily.game_day = holding.acquired_game_day
         INNER JOIN real_estate_daily AS valuation_daily
           ON valuation_daily.market_world_id = acquisition_daily.market_world_id
          AND valuation_daily.real_estate_model_version_id = acquisition_daily.real_estate_model_version_id
          AND BINARY valuation_daily.region_key = BINARY acquisition_daily.region_key
          AND valuation_daily.game_day = ?
         WHERE holding.save_id = ? AND holding.run_revision = ?
           AND holding.household_id = ? AND holding.status = 'active'
           AND holding.purpose = 'ownerOccupied'
         ORDER BY holding.id FOR UPDATE",
    )
    .bind(prior_game_day)
    .bind(prior_game_day)
    .bind(context.game_day)
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(household_id)
    .fetch_all(&mut **tx)
    .await?;
    if holdings.is_empty() {
        return Ok(());
    }
    ensure!(
        holdings.len() == usize::from(loaded.policy.annual.supported_home_count),
        "annual property-tax home count is unsupported"
    );
    let world_start_date: Date =
        sqlx::query_scalar("SELECT start_date FROM market_world WHERE id = ?")
            .bind(context.market_world_id)
            .fetch_one(&mut **tx)
            .await?;
    for holding in holdings {
        let existing: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM property_tax_event
                 WHERE save_id = ? AND run_revision = ?
                   AND property_holding_id = ? AND event_kind = 'annualProperty'
                   AND tax_year = ?
             )",
        )
        .bind(context.save_id)
        .bind(context.run_revision)
        .bind(holding.holding_id)
        .bind(context.market_date.year())
        .fetch_one(&mut **tx)
        .await?;
        if existing {
            continue;
        }
        let reference_value_krw = property_rules
            .calculate_sale_reference_value(PropertySaleReferenceValueInput {
                acquisition_price_krw: holding.acquisition_price_krw,
                acquisition_price_index_ppm: holding.acquisition_price_index_ppm,
                current_price_index_ppm: holding.valuation_price_index_ppm,
            })
            .context("annual property-tax reference valuation failed")?;
        let calculation = tax_rules
            .calculate_annual_property_tax(AnnualPropertyTaxInput {
                reference_value_krw,
                household_home_count: loaded.policy.annual.supported_home_count,
                policy: &loaded.policy.annual,
            })
            .context("annual property-tax calculation failed")?;
        ensure!(
            calculation.total_tax_krw > 0
                && calculation.first_payment_krw > 0
                && calculation.second_payment_krw > 0
                && calculation
                    .first_payment_krw
                    .checked_add(calculation.second_payment_krw)
                    == Some(calculation.total_tax_krw),
            "annual property-tax calculation is not schedulable"
        );
        let exclusions_json = serde_json::to_string(&loaded.annual_exclusion_codes)?;
        let inserted = sqlx::query(
            "INSERT INTO property_tax_event
                 (save_id, run_revision, household_id, property_holding_id,
                  policy_set_id, policy_rule_id, event_kind, status, tax_year,
                  legal_basis_date, assessment_game_day, taxable_game_day,
                  household_home_count, valuation_game_day,
                  valuation_price_index_ppm, valuation_amount_krw,
                  official_value_krw, tax_base_krw, deduction_krw,
                  total_tax_krw, paid_tax_krw, exclusion_codes, property_sale_execution_id)
             VALUES (?, ?, ?, ?, ?, ?, 'annualProperty', 'prepared', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                     0, CAST(? AS JSON), NULL)",
        )
        .bind(context.save_id)
        .bind(context.run_revision)
        .bind(holding.household_id)
        .bind(holding.holding_id)
        .bind(context.policy_set_id)
        .bind(loaded.annual_rule_id)
        .bind(context.market_date.year())
        .bind(loaded.annual_legal_basis_date)
        .bind(context.game_day)
        .bind(context.game_day)
        .bind(loaded.policy.annual.supported_home_count)
        .bind(context.game_day)
        .bind(holding.valuation_price_index_ppm)
        .bind(reference_value_krw)
        .bind(calculation.official_value_krw)
        .bind(calculation.tax_base_krw)
        .bind(0_i64)
        .bind(calculation.total_tax_krw)
        .bind(exclusions_json)
        .execute(&mut **tx)
        .await?;
        let event_id = inserted.last_insert_id();
        ensure!(event_id > 0, "annual property-tax event has no identity");
        insert_property_tax_component(
            tx,
            context,
            event_id,
            1,
            "annualPropertyTax",
            calculation.tax_base_krw,
            0,
            calculation.tax_base_krw,
            calculation.property_tax_rate_ppm,
            calculation.progressive_deduction_krw,
            calculation.property_tax_krw,
            serde_json::to_value(calculation)?,
        )
        .await?;
        insert_property_tax_component(
            tx,
            context,
            event_id,
            2,
            "annualPropertyLocalEducationTax",
            calculation.property_tax_krw,
            0,
            calculation.property_tax_krw,
            calculation.local_education_rate_ratio_ppm,
            0,
            calculation.local_education_tax_krw,
            serde_json::to_value(calculation)?,
        )
        .await?;
        let first_due = Date::from_calendar_date(
            context.market_date.year(),
            Month::try_from(loaded.policy.annual.first_payment_month)?,
            loaded.policy.annual.first_payment_day,
        )?;
        let second_due = Date::from_calendar_date(
            context.market_date.year(),
            Month::try_from(loaded.policy.annual.second_payment_month)?,
            loaded.policy.annual.second_payment_day,
        )?;
        insert_property_tax_payment_schedule(
            tx,
            context,
            event_id,
            1,
            game_day_for_date(world_start_date, first_due)?,
            calculation.first_payment_krw,
        )
        .await?;
        insert_property_tax_payment_schedule(
            tx,
            context,
            event_id,
            2,
            game_day_for_date(world_start_date, second_due)?,
            calculation.second_payment_krw,
        )
        .await?;
        transition_prepared_property_tax_event(tx, context, event_id, "scheduled", 0).await?;
    }
    Ok(())
}

pub(super) async fn create_capital_gains_property_tax_event_in_tx(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn PropertyTaxRules,
    input: CapitalGainsPropertyTaxEventInput,
) -> Result<CapitalGainsPropertyTaxEventResult> {
    let loaded = load_property_tax_policy(tx, input.context.policy_set_id).await?;
    rules
        .validate_policy(&loaded.policy)
        .context("capital-gains property tax policy validation failed")?;
    let calculation = rules
        .calculate_one_home_capital_gains_tax(OneHomeCapitalGainsTaxInput {
            sale_price_krw: input.sale_price_krw,
            acquisition_price_krw: input.acquisition_price_krw,
            acquisition_incidental_cost_krw: input.acquisition_incidental_cost_krw,
            acquisition_taxes_krw: input.acquisition_taxes_krw,
            disposition_cost_krw: input.disposition_cost_krw,
            acquired_on: input.acquired_on,
            owner_occupied_from: input.owner_occupied_from,
            sold_on: input.context.market_date,
            household_home_count: 1,
            policy: &loaded.policy.capital_gains,
        })
        .context("one-home capital-gains tax calculation failed")?;
    let total_deduction_krw = calculation
        .long_term_deduction_krw
        .checked_add(calculation.basic_deduction_krw)
        .context("capital-gains tax deduction overflowed")?;
    let inserted = sqlx::query(
        "INSERT INTO property_tax_event
             (save_id, run_revision, household_id, property_holding_id,
              policy_set_id, policy_rule_id, property_sale_execution_id,
              event_kind, status, tax_year, legal_basis_date,
              assessment_game_day, taxable_game_day, household_home_count,
              valuation_game_day, valuation_price_index_ppm, valuation_amount_krw,
              official_value_krw, tax_base_krw, deduction_krw,
              acquisition_taxes_krw, disposition_cost_krw, gross_gain_krw,
              high_value_gain_krw, long_term_deduction_krw,
              completed_holding_years, completed_residence_years,
              total_tax_krw, paid_tax_krw, exclusion_codes)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'capitalGains', 'prepared', ?, ?, ?, ?, 1,
                 ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, JSON_ARRAY())",
    )
    .bind(input.context.save_id)
    .bind(input.context.run_revision)
    .bind(input.household_id)
    .bind(input.holding_id)
    .bind(input.context.policy_set_id)
    .bind(loaded.capital_gains_rule_id)
    .bind(input.property_sale_execution_id)
    .bind(input.context.market_date.year())
    .bind(loaded.capital_gains_legal_basis_date)
    .bind(input.context.game_day)
    .bind(input.context.game_day)
    .bind(input.context.game_day)
    .bind(input.valuation_price_index_ppm)
    .bind(input.sale_price_krw)
    .bind(calculation.high_value_gain_krw)
    .bind(total_deduction_krw)
    .bind(input.acquisition_taxes_krw)
    .bind(input.disposition_cost_krw)
    .bind(calculation.gross_gain_krw)
    .bind(calculation.high_value_gain_krw)
    .bind(calculation.long_term_deduction_krw)
    .bind(calculation.completed_holding_years)
    .bind(calculation.completed_residence_years)
    .bind(calculation.total_tax_krw)
    .execute(&mut **tx)
    .await?;
    let event_id = inserted.last_insert_id();
    ensure!(event_id > 0, "capital-gains tax event has no identity");
    insert_property_tax_component(
        tx,
        input.context,
        event_id,
        1,
        "capitalGainsTax",
        calculation.high_value_gain_krw,
        total_deduction_krw,
        calculation.taxable_amount_krw,
        calculation.national.rate_ppm,
        calculation.national.progressive_deduction_krw,
        calculation.national.tax_krw,
        serde_json::to_value(calculation)?,
    )
    .await?;
    insert_property_tax_component(
        tx,
        input.context,
        event_id,
        2,
        "capitalGainsLocalIncomeTax",
        calculation.high_value_gain_krw,
        total_deduction_krw,
        calculation.taxable_amount_krw,
        calculation.local.rate_ppm,
        calculation.local.progressive_deduction_krw,
        calculation.local.tax_krw,
        serde_json::to_value(calculation)?,
    )
    .await?;
    let terminal_status = if calculation.total_tax_krw == 0 {
        "noPaymentRequired"
    } else {
        "paid"
    };
    transition_prepared_property_tax_event(
        tx,
        input.context,
        event_id,
        terminal_status,
        calculation.total_tax_krw,
    )
    .await?;
    Ok(CapitalGainsPropertyTaxEventResult {
        event_id: ResourceId::from_u64(event_id),
        calculation,
    })
}

#[allow(clippy::too_many_arguments)]
async fn insert_property_tax_component(
    tx: &mut Transaction<'_, MySql>,
    context: PropertyTaxRunContext,
    event_id: u64,
    component_order: u8,
    component_kind: &str,
    tax_base_krw: i64,
    deduction_krw: i64,
    taxable_amount_krw: i64,
    rate_ppm: i64,
    progressive_deduction_krw: i64,
    tax_amount_krw: i64,
    calculation_evidence: serde_json::Value,
) -> Result<()> {
    let evidence_json = serde_json::to_string(&calculation_evidence)?;
    sqlx::query(
        "INSERT INTO property_tax_component
             (save_id, run_revision, property_tax_event_id, component_order,
              component_kind, tax_base_krw, deduction_krw, taxable_amount_krw,
              rate_ppm, progressive_deduction_krw, tax_amount_krw,
              calculation_evidence)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, CAST(? AS JSON))",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(event_id)
    .bind(component_order)
    .bind(component_kind)
    .bind(tax_base_krw)
    .bind(deduction_krw)
    .bind(taxable_amount_krw)
    .bind(rate_ppm)
    .bind(progressive_deduction_krw)
    .bind(tax_amount_krw)
    .bind(evidence_json)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_property_tax_payment_schedule(
    tx: &mut Transaction<'_, MySql>,
    context: PropertyTaxRunContext,
    event_id: u64,
    payment_no: u8,
    due_game_day: u32,
    amount_krw: i64,
) -> Result<u64> {
    ensure!(
        amount_krw > 0,
        "property tax payment amount must be positive"
    );
    let inserted = sqlx::query(
        "INSERT INTO property_tax_payment
             (save_id, run_revision, property_tax_event_id, payment_no,
              due_game_day, amount_krw, status, paid_game_day,
              cancelled_game_day, cancellation_reason,
              paid_from_wallet_krw, obligated_amount_krw,
              scheduled_settlement_id, ledger_transaction_id, tax_obligation_id)
         VALUES (?, ?, ?, ?, ?, ?, 'pending', NULL, NULL, NULL, 0, 0, NULL, NULL, NULL)",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(event_id)
    .bind(payment_no)
    .bind(due_game_day)
    .bind(amount_krw)
    .execute(&mut **tx)
    .await?;
    let payment_id = inserted.last_insert_id();
    ensure!(payment_id > 0, "property tax payment has no identity");
    let payload = PropertyTaxPaymentSettlementPayload {
        version: PROPERTY_TAX_SETTLEMENT_PAYLOAD_VERSION,
        property_tax_event_id: event_id.to_string(),
        property_tax_payment_id: payment_id.to_string(),
        payment_no,
    };
    let payload_json = serde_json::to_string(&payload)?;
    let settlement = sqlx::query(
        "INSERT INTO scheduled_settlement
             (save_id, run_revision, due_game_day, kind, payload,
              source_kind, source_id, occurrence, status)
         VALUES (?, ?, ?, 'propertyTaxPayment', CAST(? AS JSON),
                 'propertyTaxEvent', ?, ?, 'pending')",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(due_game_day)
    .bind(payload_json)
    .bind(event_id.to_string())
    .bind(u64::from(payment_no))
    .execute(&mut **tx)
    .await?;
    let settlement_id = settlement.last_insert_id();
    ensure!(settlement_id > 0, "property tax settlement has no identity");
    let linked = sqlx::query(
        "UPDATE property_tax_payment SET scheduled_settlement_id = ?
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND property_tax_event_id = ? AND status = 'pending'
           AND scheduled_settlement_id IS NULL",
    )
    .bind(settlement_id)
    .bind(payment_id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(event_id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        linked.rows_affected() == 1,
        "property tax payment link changed"
    );
    Ok(payment_id)
}

async fn transition_prepared_property_tax_event(
    tx: &mut Transaction<'_, MySql>,
    context: PropertyTaxRunContext,
    event_id: u64,
    status: &str,
    paid_tax_krw: i64,
) -> Result<()> {
    ensure!(
        matches!(status, "scheduled" | "paid" | "noPaymentRequired"),
        "property tax event transition status is invalid"
    );
    let update = sqlx::query(
        "UPDATE property_tax_event
         SET status = ?, paid_tax_krw = ?
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'prepared'",
    )
    .bind(status)
    .bind(paid_tax_krw)
    .bind(event_id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "property tax event lost its prepared state"
    );
    Ok(())
}

pub(super) fn validate_property_tax_settlement_envelope(
    settlement: &ScheduledSettlement,
) -> Result<()> {
    ensure!(
        settlement.kind == SettlementKind::PropertyTaxPayment
            && settlement.source.kind == SettlementSourceKind::PropertyTaxEvent,
        "settlement is not a property tax payment"
    );
    let payload = property_tax_payment_payload(&settlement.payload)?;
    ensure!(
        settlement.source.source_id == payload.property_tax_event_id
            && settlement.source.occurrence == u64::from(payload.payment_no),
        "stored property tax settlement identity is invalid"
    );
    Ok(())
}

pub(super) async fn settle_property_tax_payment_by_id_in_tx(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    context: PropertyTaxRunContext,
    settlement_id: u64,
) -> Result<()> {
    let unlocked: PropertyTaxSettlementEnvelopeRow = sqlx::query_as(
        "SELECT id, due_game_day, kind, CAST(payload AS CHAR) AS payload_json,
                source_kind, source_id, occurrence, status
         FROM scheduled_settlement
         WHERE id = ? AND save_id = ? AND run_revision = ?",
    )
    .bind(settlement_id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .fetch_optional(&mut **tx)
    .await?
    .context("property tax settlement is missing")?;
    let unlocked = property_tax_scheduled_settlement(context, unlocked)?;
    validate_property_tax_settlement_envelope(&unlocked)?;
    let payload = property_tax_payment_payload(&unlocked.payload)?;
    let event_id = parse_canonical_u64(&payload.property_tax_event_id, "property tax event")?;
    let payment_id = parse_canonical_u64(&payload.property_tax_payment_id, "property tax payment")?;

    let (cash_before, debt_before): (i64, i64) = sqlx::query_as(
        "SELECT cash_krw, debt_krw FROM save
         WHERE id = ? AND run_revision = ? AND policy_set_id = ? FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(context.policy_set_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("property tax payment run disappeared")?;
    ensure!(
        cash_before >= 0 && debt_before >= 0,
        "property tax payment balances are invalid"
    );
    let payment: DuePropertyTaxPaymentRow = sqlx::query_as(
        "SELECT property_tax_event_id, payment_no, due_game_day, amount_krw,
                status, scheduled_settlement_id
         FROM property_tax_payment
         WHERE id = ? AND save_id = ? AND run_revision = ? FOR UPDATE",
    )
    .bind(payment_id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .fetch_optional(&mut **tx)
    .await?
    .context("property tax payment is missing")?;
    ensure!(
        payment.status == "pending"
            && payment.property_tax_event_id == event_id
            && payment.payment_no == payload.payment_no
            && payment.due_game_day == context.game_day
            && payment.scheduled_settlement_id == Some(settlement_id)
            && payment.amount_krw > 0,
        "property tax payment state is not due"
    );
    let locked: PropertyTaxSettlementEnvelopeRow = sqlx::query_as(
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
    .context("property tax settlement disappeared")?;
    let locked = property_tax_scheduled_settlement(context, locked)?;
    validate_property_tax_settlement_envelope(&locked)?;
    ensure!(
        locked.status == SettlementStatus::Pending
            && locked.due_game_day == context.game_day
            && locked == unlocked,
        "property tax settlement changed while being locked"
    );
    let event: DuePropertyTaxEventRow = sqlx::query_as(
        "SELECT household_id, policy_set_id, total_tax_krw, paid_tax_krw, status
         FROM property_tax_event
         WHERE id = ? AND save_id = ? AND run_revision = ? FOR UPDATE",
    )
    .bind(event_id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .fetch_optional(&mut **tx)
    .await?
    .context("property tax event is missing")?;
    ensure!(
        event.policy_set_id == context.policy_set_id
            && matches!(event.status.as_str(), "scheduled" | "partiallyPaid")
            && event.total_tax_krw > event.paid_tax_krw
            && event
                .paid_tax_krw
                .checked_add(payment.amount_krw)
                .is_some_and(|paid| paid <= event.total_tax_krw),
        "property tax event cannot accept this payment"
    );

    let wallet_paid_krw = cash_before.min(payment.amount_krw);
    let obligated_amount_krw = payment
        .amount_krw
        .checked_sub(wallet_paid_krw)
        .context("property tax payment funding underflowed")?;
    let tax_obligation_id = prepare_property_tax_obligation(
        tx,
        context,
        event.household_id,
        event_id,
        payment.payment_no,
        payment.due_game_day,
        obligated_amount_krw,
    )
    .await?;
    let mut postings = Vec::with_capacity(3);
    postings.push(LedgerPosting {
        account_code: LedgerAccountCode::PropertyTaxExpense,
        financial_account_id: None,
        amount_krw: payment.amount_krw,
    });
    if wallet_paid_krw > 0 {
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::Wallet,
            financial_account_id: None,
            amount_krw: wallet_paid_krw
                .checked_neg()
                .context("property tax wallet posting overflowed")?,
        });
    }
    if obligated_amount_krw > 0 {
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::TaxObligationLiability,
            financial_account_id: None,
            amount_krw: obligated_amount_krw
                .checked_neg()
                .context("property tax obligation posting overflowed")?,
        });
    }
    let ledger = finance_rules
        .create_ledger_transaction(LedgerTransactionDraft {
            policy: RunPolicyContext {
                run: RunId {
                    save_id: ResourceId::from_u64(context.save_id),
                    run_revision: context.run_revision,
                },
                policy_set_id: ResourceId::from_u64(context.policy_set_id),
            },
            source: LedgerSource {
                kind: LedgerSourceKind::PropertyTaxPayment,
                source_id: payment_id.to_string(),
            },
            game_day: context.game_day,
            description: "부동산 세금 납부".to_owned(),
            postings,
        })
        .context("property tax payment ledger is invalid")?;
    let ledger_transaction_id =
        write_property_tax_payment_ledger(tx, &ledger, event_id, tax_obligation_id).await?;
    if let Some(tax_obligation_id) = tax_obligation_id {
        activate_property_tax_obligation(
            tx,
            context,
            event_id,
            payment.payment_no,
            tax_obligation_id,
            ledger_transaction_id,
        )
        .await?;
    }
    let cash_after = cash_before
        .checked_sub(wallet_paid_krw)
        .context("property tax payment wallet underflowed")?;
    let debt_after = debt_before
        .checked_add(obligated_amount_krw)
        .context("property tax payment debt overflowed")?;
    let save_update = sqlx::query(
        "UPDATE save SET cash_krw = ?, debt_krw = ?
         WHERE id = ? AND run_revision = ? AND policy_set_id = ?
           AND cash_krw = ? AND debt_krw = ?",
    )
    .bind(cash_after)
    .bind(debt_after)
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(context.policy_set_id)
    .bind(cash_before)
    .bind(debt_before)
    .execute(&mut **tx)
    .await?;
    ensure!(
        save_update.rows_affected() == 1,
        "property tax payment lost save balances"
    );
    let payment_update = sqlx::query(
        "UPDATE property_tax_payment
         SET status = 'applied', paid_from_wallet_krw = ?, obligated_amount_krw = ?,
             ledger_transaction_id = ?, tax_obligation_id = ?, paid_game_day = ?
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND property_tax_event_id = ? AND status = 'pending'",
    )
    .bind(wallet_paid_krw)
    .bind(obligated_amount_krw)
    .bind(ledger_transaction_id)
    .bind(tax_obligation_id)
    .bind(context.game_day)
    .bind(payment_id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(event_id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        payment_update.rows_affected() == 1,
        "property tax payment lost its pending state"
    );
    let paid_tax_krw = event
        .paid_tax_krw
        .checked_add(payment.amount_krw)
        .context("property tax paid amount overflowed")?;
    let event_status = if paid_tax_krw == event.total_tax_krw {
        "paid"
    } else {
        "partiallyPaid"
    };
    let event_update = sqlx::query(
        "UPDATE property_tax_event SET status = ?, paid_tax_krw = ?
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = ?",
    )
    .bind(event_status)
    .bind(paid_tax_krw)
    .bind(event_id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(&event.status)
    .execute(&mut **tx)
    .await?;
    ensure!(
        event_update.rows_affected() == 1,
        "property tax event lost its payment state"
    );
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
        "property tax settlement lost its pending state"
    );
    super::loans::validate_debt_projection_in_tx(tx, context.save_id, context.run_revision).await?;
    Ok(())
}

pub(super) async fn close_property_tax_for_new_run_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
) -> Result<()> {
    let rows: Vec<(u64, u64)> = sqlx::query_as(
        "SELECT payment.id, settlement.id
         FROM property_tax_payment AS payment
         INNER JOIN scheduled_settlement AS settlement
           ON settlement.id = payment.scheduled_settlement_id
          AND settlement.save_id = payment.save_id
          AND settlement.run_revision = payment.run_revision
         WHERE payment.save_id = ? AND payment.run_revision = ?
           AND payment.status = 'pending' AND settlement.status = 'pending'
         ORDER BY payment.id FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    for (payment_id, settlement_id) in rows {
        let settlement_update = sqlx::query(
            "UPDATE scheduled_settlement
             SET status = 'cancelled', cancellation_reason = 'newRun'
             WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'pending'",
        )
        .bind(settlement_id)
        .bind(save_id)
        .bind(run_revision)
        .execute(&mut **tx)
        .await?;
        ensure!(
            settlement_update.rows_affected() == 1,
            "property tax settlement lost its pending state during new-run cleanup"
        );
        let payment_update = sqlx::query(
            "UPDATE property_tax_payment
             SET status = 'cancelled', cancelled_game_day = ?,
                 cancellation_reason = 'newRun'
             WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'pending'",
        )
        .bind(game_day)
        .bind(payment_id)
        .bind(save_id)
        .bind(run_revision)
        .execute(&mut **tx)
        .await?;
        ensure!(
            payment_update.rows_affected() == 1,
            "property tax payment lost its pending state during new-run cleanup"
        );
    }
    Ok(())
}

async fn prepare_property_tax_obligation(
    tx: &mut Transaction<'_, MySql>,
    context: PropertyTaxRunContext,
    household_id: u64,
    event_id: u64,
    payment_no: u8,
    due_game_day: u32,
    amount_krw: i64,
) -> Result<Option<u64>> {
    if amount_krw == 0 {
        return Ok(None);
    }
    ensure!(
        amount_krw > 0 && due_game_day == context.game_day,
        "property tax obligation is invalid"
    );
    let source_id = event_id.to_string();
    let insert = sqlx::query(
        "INSERT INTO tax_obligation
             (save_id, run_revision, household_id, policy_set_id,
              source_kind, source_id, source_occurrence, due_game_day,
              original_amount_krw, paid_amount_krw, outstanding_amount_krw,
              status, authority_ledger_transaction_id)
         VALUES (?, ?, ?, ?, 'propertyTaxEvent', ?, ?, ?, ?, 0, ?, 'prepared', NULL)",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(household_id)
    .bind(context.policy_set_id)
    .bind(&source_id)
    .bind(u64::from(payment_no))
    .bind(due_game_day)
    .bind(amount_krw)
    .bind(amount_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        insert.rows_affected() == 1,
        "property tax obligation source is already authoritative"
    );
    let obligation_id = insert.last_insert_id();
    ensure!(obligation_id > 0, "property tax obligation has no identity");
    Ok(Some(obligation_id))
}

async fn activate_property_tax_obligation(
    tx: &mut Transaction<'_, MySql>,
    context: PropertyTaxRunContext,
    event_id: u64,
    payment_no: u8,
    obligation_id: u64,
    ledger_transaction_id: u64,
) -> Result<()> {
    let update = sqlx::query(
        "UPDATE tax_obligation
         SET status = 'outstanding', authority_ledger_transaction_id = ?
         WHERE id = ? AND save_id = ? AND run_revision = ? AND policy_set_id = ?
           AND source_kind = 'propertyTaxEvent' AND BINARY source_id = BINARY ?
           AND source_occurrence = ? AND status = 'prepared'
           AND authority_ledger_transaction_id IS NULL",
    )
    .bind(ledger_transaction_id)
    .bind(obligation_id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(context.policy_set_id)
    .bind(event_id.to_string())
    .bind(u64::from(payment_no))
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "property tax obligation lost its prepared state"
    );
    Ok(())
}

async fn write_property_tax_payment_ledger(
    tx: &mut Transaction<'_, MySql>,
    ledger: &LedgerTransaction,
    property_tax_event_id: u64,
    tax_obligation_id: Option<u64>,
) -> Result<u64> {
    ensure!(
        ledger
            .postings()
            .iter()
            .filter(|posting| posting.account_code == LedgerAccountCode::PropertyTaxExpense)
            .count()
            == 1,
        "property tax ledger event reference is incomplete"
    );
    ensure!(
        ledger
            .postings()
            .iter()
            .filter(|posting| posting.account_code == LedgerAccountCode::TaxObligationLiability)
            .count()
            == usize::from(tax_obligation_id.is_some()),
        "property tax ledger obligation reference is incomplete"
    );
    let policy = ledger.policy();
    let insert = sqlx::query(
        "INSERT INTO ledger_transaction
             (save_id, run_revision, game_day, policy_set_id,
              source_kind, source_id, description)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(policy.run.save_id.get())
    .bind(policy.run.run_revision)
    .bind(ledger.game_day())
    .bind(policy.policy_set_id.get())
    .bind(property_tax_db_enum(&ledger.source().kind)?)
    .bind(&ledger.source().source_id)
    .bind(ledger.description())
    .execute(&mut **tx)
    .await?;
    let ledger_transaction_id = insert.last_insert_id();
    ensure!(
        ledger_transaction_id > 0,
        "property tax ledger has no identity"
    );
    for (index, posting) in ledger.postings().iter().enumerate() {
        let posting_order = u16::try_from(index + 1).context("too many property tax postings")?;
        let event_reference = (posting.account_code == LedgerAccountCode::PropertyTaxExpense)
            .then_some(property_tax_event_id);
        let obligation_reference = (posting.account_code
            == LedgerAccountCode::TaxObligationLiability)
            .then_some(tax_obligation_id)
            .flatten();
        sqlx::query(
            "INSERT INTO ledger_posting
                 (save_id, run_revision, ledger_transaction_id, posting_order,
                  account_code, financial_account_id, tax_obligation_id,
                  property_tax_event_id, amount_krw)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(policy.run.save_id.get())
        .bind(policy.run.run_revision)
        .bind(ledger_transaction_id)
        .bind(posting_order)
        .bind(property_tax_db_enum(&posting.account_code)?)
        .bind(posting.financial_account_id.map(ResourceId::get))
        .bind(obligation_reference)
        .bind(event_reference)
        .bind(posting.amount_krw)
        .execute(&mut **tx)
        .await?;
    }
    Ok(ledger_transaction_id)
}

fn property_tax_scheduled_settlement(
    context: PropertyTaxRunContext,
    row: PropertyTaxSettlementEnvelopeRow,
) -> Result<ScheduledSettlement> {
    Ok(ScheduledSettlement {
        id: ResourceId::from_u64(row.id),
        run: RunId {
            save_id: ResourceId::from_u64(context.save_id),
            run_revision: context.run_revision,
        },
        due_game_day: row.due_game_day,
        kind: property_tax_db_parse(&row.kind)?,
        source: SettlementSource {
            kind: property_tax_db_parse(&row.source_kind)?,
            source_id: row.source_id,
            occurrence: row.occurrence,
        },
        status: property_tax_db_parse(&row.status)?,
        payload: serde_json::from_str(&row.payload_json)
            .context("stored property tax settlement payload is invalid JSON")?,
    })
}

fn property_tax_payment_payload(
    value: &serde_json::Value,
) -> Result<PropertyTaxPaymentSettlementPayload> {
    let payload: PropertyTaxPaymentSettlementPayload = serde_json::from_value(value.clone())
        .context("stored property tax settlement payload is invalid")?;
    ensure!(
        payload.version == PROPERTY_TAX_SETTLEMENT_PAYLOAD_VERSION
            && (1..=2).contains(&payload.payment_no),
        "stored property tax settlement payload version or payment number is invalid"
    );
    parse_canonical_u64(&payload.property_tax_event_id, "property tax event")?;
    parse_canonical_u64(&payload.property_tax_payment_id, "property tax payment")?;
    Ok(payload)
}

fn parse_canonical_u64(value: &str, label: &str) -> Result<u64> {
    let parsed = value
        .parse::<u64>()
        .with_context(|| format!("stored {label} identity is invalid"))?;
    ensure!(
        parsed > 0 && parsed.to_string() == value,
        "stored {label} identity is not canonical"
    );
    Ok(parsed)
}

fn property_tax_db_enum<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value)? {
        serde_json::Value::String(value) => Ok(value),
        other => bail!("property tax enum is not storable as a string: {other}"),
    }
}

fn property_tax_db_parse<T: DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_value(serde_json::Value::String(value.to_owned()))
        .context("stored property tax enum is invalid")
}

fn game_day_for_date(world_start_date: Date, date: Date) -> Result<u32> {
    let days = (date - world_start_date).whole_days();
    ensure!(days >= 0, "property tax date predates the market world");
    u32::try_from(days).context("property tax game day is out of range")
}

pub(super) async fn read_property_tax_events(
    pool: &MySqlPool,
    user_id: u64,
    holding_id: ResourceId,
    query: PropertyTaxEventPageQuery,
) -> Result<Option<PropertyTaxEventPageState>> {
    ensure!(
        (1..=MAX_PROPERTY_TAX_HISTORY_PAGE_SIZE).contains(&query.limit),
        "property tax history page limit is invalid"
    );
    let mut tx = pool.begin().await?;
    let owned: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1
             FROM save
             INNER JOIN `character` ON `character`.save_id = save.id
             INNER JOIN property_holding AS holding
               ON holding.save_id = save.id AND holding.run_revision = save.run_revision
             WHERE save.user_id = ? AND holding.id = ?
         )",
    )
    .bind(user_id)
    .bind(holding_id.get())
    .fetch_one(&mut *tx)
    .await?;
    if !owned {
        tx.commit().await?;
        return Ok(None);
    }
    let (save_id, run_revision): (u64, u32) = sqlx::query_as(
        "SELECT save.id, save.run_revision
         FROM save
         INNER JOIN property_holding AS holding
           ON holding.save_id = save.id AND holding.run_revision = save.run_revision
         WHERE save.user_id = ? AND holding.id = ?",
    )
    .bind(user_id)
    .bind(holding_id.get())
    .fetch_one(&mut *tx)
    .await?;
    let before = query.before.map(ResourceId::get);
    let fetch_limit = u32::from(query.limit)
        .checked_add(1)
        .context("property tax history page limit overflowed")?;
    let mut rows: Vec<PropertyTaxEventRow> = sqlx::query_as(
        "SELECT event.id, event.property_holding_id, event.policy_set_id,
                policy.policy_key, event.policy_rule_id, rule.rule_key,
                event.legal_basis_date, event.event_kind, event.status, event.tax_year,
                event.assessment_game_day, event.taxable_game_day,
                CASE
                    WHEN event.event_kind = 'capitalGains' AND event.status = 'paid'
                    THEN event.assessment_game_day
                    ELSE (SELECT MAX(payment.paid_game_day)
                            FROM property_tax_payment AS payment
                           WHERE payment.property_tax_event_id = event.id
                             AND payment.status = 'applied')
                END AS paid_game_day,
                event.household_home_count, event.valuation_game_day,
                event.valuation_price_index_ppm, event.valuation_amount_krw,
                event.official_value_krw, event.tax_base_krw, event.deduction_krw,
                event.total_tax_krw, event.paid_tax_krw,
                CAST(event.exclusion_codes AS CHAR CHARACTER SET utf8mb4)
                    AS exclusion_codes_json
         FROM property_tax_event AS event
         INNER JOIN policy_set AS policy ON policy.id = event.policy_set_id
         INNER JOIN policy_rule AS rule ON rule.id = event.policy_rule_id
         WHERE event.save_id = ? AND event.run_revision = ?
           AND event.property_holding_id = ?
           AND (? IS NULL OR event.id < ?)
         ORDER BY event.id DESC
         LIMIT ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(holding_id.get())
    .bind(before)
    .bind(before)
    .bind(fetch_limit)
    .fetch_all(&mut *tx)
    .await?;
    let has_more = rows.len() > usize::from(query.limit);
    rows.truncate(usize::from(query.limit));
    let next_before = has_more
        .then(|| rows.last().map(|row| ResourceId::from_u64(row.id)))
        .flatten();
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(property_tax_event_from_row(&mut tx, row).await?);
    }
    tx.commit().await?;
    Ok(Some(PropertyTaxEventPageState {
        holding_id,
        items,
        next_before,
    }))
}

async fn property_tax_event_from_row(
    tx: &mut Transaction<'_, MySql>,
    row: PropertyTaxEventRow,
) -> Result<PropertyTaxEventState> {
    let component_rows: Vec<PropertyTaxComponentRow> = sqlx::query_as(
        "SELECT component_order, component_kind, tax_base_krw, deduction_krw,
                taxable_amount_krw, rate_ppm, progressive_deduction_krw,
                tax_amount_krw
         FROM property_tax_component
         WHERE property_tax_event_id = ?
         ORDER BY component_order",
    )
    .bind(row.id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        !component_rows.is_empty(),
        "property tax event has no components"
    );
    let taxable_amount_krw = component_rows
        .iter()
        .map(|component| component.taxable_amount_krw)
        .max()
        .context("property tax event has no taxable amount")?;
    let components = component_rows
        .into_iter()
        .map(|component| PropertyTaxComponentState {
            component_key: component.component_kind,
            component_order: component.component_order,
            tax_base_krw: component.tax_base_krw,
            deduction_krw: component.deduction_krw,
            taxable_amount_krw: component.taxable_amount_krw,
            rate_ppm: i64::from(component.rate_ppm),
            progressive_deduction_krw: component.progressive_deduction_krw,
            amount_krw: component.tax_amount_krw,
        })
        .collect::<Vec<_>>();
    let payment_rows: Vec<PropertyTaxPaymentRow> = sqlx::query_as(
        "SELECT payment_no, due_game_day, paid_game_day, amount_krw, status,
                paid_from_wallet_krw, obligated_amount_krw
         FROM property_tax_payment
         WHERE property_tax_event_id = ?
         ORDER BY payment_no",
    )
    .bind(row.id)
    .fetch_all(&mut **tx)
    .await?;
    let payments = payment_rows
        .into_iter()
        .map(|payment| {
            let status = match payment.status.as_str() {
                "pending" => PropertyTaxPaymentStatusState::Pending,
                "applied" => PropertyTaxPaymentStatusState::Applied,
                "cancelled" => PropertyTaxPaymentStatusState::Cancelled,
                _ => bail!("stored property tax payment status is invalid"),
            };
            ensure!(
                payment
                    .paid_from_wallet_krw
                    .checked_add(payment.obligated_amount_krw)
                    .is_some_and(|total| {
                        (status == PropertyTaxPaymentStatusState::Applied
                            && total == payment.amount_krw)
                            || (status != PropertyTaxPaymentStatusState::Applied && total == 0)
                    }),
                "property tax payment funding disagrees with its status"
            );
            Ok(PropertyTaxPaymentState {
                payment_no: payment.payment_no,
                due_game_day: payment.due_game_day,
                paid_game_day: payment.paid_game_day,
                status,
                amount_krw: payment.amount_krw,
                wallet_paid_krw: payment.paid_from_wallet_krw,
                tax_obligation_krw: payment.obligated_amount_krw,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let kind = match row.event_kind.as_str() {
        "acquisition" => PropertyTaxEventKindState::Acquisition,
        "annualProperty" => PropertyTaxEventKindState::AnnualHolding,
        "capitalGains" => PropertyTaxEventKindState::CapitalGains,
        _ => bail!("stored property tax event kind is invalid"),
    };
    let status = match row.status.as_str() {
        "scheduled" => PropertyTaxEventStatusState::Scheduled,
        "partiallyPaid" => PropertyTaxEventStatusState::PartiallyPaid,
        "paid" => PropertyTaxEventStatusState::Paid,
        "noPaymentRequired" => PropertyTaxEventStatusState::NoPaymentRequired,
        _ => bail!("stored property tax event status is invalid"),
    };
    let exclusion_codes: Vec<String> = serde_json::from_str(&row.exclusion_codes_json)
        .context("stored property tax exclusion codes are invalid")?;
    ensure!(
        exclusion_codes.iter().all(|code| !code.is_empty()),
        "stored property tax exclusion code is empty"
    );
    Ok(PropertyTaxEventState {
        id: ResourceId::from_u64(row.id),
        holding_id: ResourceId::from_u64(row.property_holding_id),
        policy_set_id: ResourceId::from_u64(row.policy_set_id),
        policy_key: row.policy_key,
        rule_id: ResourceId::from_u64(row.policy_rule_id),
        rule_key: row.rule_key,
        legal_basis_date: row.legal_basis_date.to_string(),
        kind,
        status,
        tax_year: row.tax_year.map(i32::from),
        assessed_game_day: row.assessment_game_day,
        taxable_game_day: row.taxable_game_day,
        paid_game_day: row.paid_game_day,
        household_home_count: row.household_home_count,
        gross_amount_krw: row.valuation_amount_krw,
        valuation_game_day: row.valuation_game_day,
        valuation_price_index_ppm: row.valuation_price_index_ppm,
        official_value_krw: row.official_value_krw,
        tax_base_krw: row.tax_base_krw,
        deduction_krw: row.deduction_krw,
        taxable_amount_krw,
        total_tax_krw: row.total_tax_krw,
        paid_tax_krw: row.paid_tax_krw,
        components,
        payments,
        exclusion_codes,
    })
}
