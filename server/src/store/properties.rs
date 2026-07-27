//! M4-C3 owner-occupied property purchase persistence.

use std::str::FromStr;

use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use sqlx::{MySql, MySqlPool, Transaction};
use time::Date;

use super::housing::{is_retryable_database_error, prepare_current_housing_catalogs};
use super::leases::{
    LeaseProjectionRow, LockedLeaseSaveRow, ResidenceProjectionRow,
    cancel_future_lease_rent_charge, close_existing_lease, close_existing_lease_lifecycle,
    close_existing_residence, lock_existing_lease,
};
use super::loans::{
    LeaseMovePayoffPreparation, MortgageLoanAssessment, MortgageLoanAssessmentResult,
    PreparedLeaseMovePayoff, PropertySalePayoffPreparation, apply_lease_move_payoff_in_tx,
    apply_property_sale_payoff_in_tx, assess_mortgage_loan_in_tx,
    mark_lease_move_payoff_applied_in_tx, mark_property_sale_payoff_applied_in_tx,
    originate_mortgage_in_tx, prepare_lease_move_payoff_in_tx, prepare_property_sale_payoff_in_tx,
    validate_debt_projection_in_tx,
};
use super::mysql::{
    CommandIdentitySpec, CommandIdentityState, GameCommandReceiptWrite, inspect_command_identity,
    read_state, write_command_identity, write_game_command_receipt,
};
use super::property_tax::{
    AcquisitionPropertyTaxEventInput, CapitalGainsPropertyTaxEventInput, PropertyTaxRunContext,
    calculate_capital_gains_property_tax_in_tx, create_acquisition_property_tax_event_in_tx,
    create_capital_gains_property_tax_event_in_tx,
};

use super::types::{
    CancelPropertySaleOrderCommand, CreateMortgageQuoteCommand, CreatePropertySaleOrderCommand,
    GameCommandCursor, HousingPropertyHoldingsState, HousingPurchaseCapabilityState,
    LifeFailureCode, LifeStoreResult, LoanQuoteLtvState, MortgageLtvRegionClassState,
    MortgageQuoteDecisionState, MortgageQuoteReasonState, MortgageQuoteReceipt,
    MortgageStressTreatmentState, PropertyHoldingPurposeState, PropertyHoldingState,
    PropertyHoldingStatusState, PropertyPurchaseReceipt, PropertySaleExecutionState,
    PropertySaleOrderCancellationReceipt, PropertySaleOrderListingReceipt,
    PropertySaleOrderPageQuery, PropertySaleOrderPageState, PropertySaleOrderRejectionReasonState,
    PropertySaleOrderRevisionKindState, PropertySaleOrderStatusState,
    PropertySaleOrderSummaryState, PurchasePropertyCommand, RepricePropertySaleOrderCommand,
};
use crate::finance::{
    CommandCursor, FinanceRules, LedgerAccountCode, LedgerPosting, LedgerSource, LedgerSourceKind,
    LedgerTransaction, LedgerTransactionDraft, ResourceId, RunId, RunPolicyContext,
};
use crate::life::{
    AcquisitionIncidentalCostInput, LifeRegionKey, LtvAssessmentInput, MortgageFundingLimitInput,
    MortgageRegionalPriceCapInput, MortgageRegionalPriceCapPolicy, PropertyDispositionCostInput,
    PropertyError, PropertyPurchaseFundingInput, PropertyPurchaseFundingPlan, PropertyRules,
    PropertySaleCandidateInput, PropertySaleLiquidityProfile, PropertySalePeriodInput,
    PropertySaleProceedsInput, PropertySaleProceedsPlan, PropertySaleReferenceValueInput,
    PropertyTaxError, PropertyTaxRules, PropertyType, RealEstateRules,
};

const MAX_SNAPSHOT_HOLDINGS: usize = 4;
const MAX_TRANSACTION_ATTEMPTS: usize = 3;
const COMMAND_KIND_QUOTE_MORTGAGE: &str = "quoteMortgage";
const COMMAND_KIND_PURCHASE_PROPERTY: &str = "purchaseProperty";
const COMMAND_KIND_CREATE_PROPERTY_SALE_ORDER: &str = "createPropertySaleOrder";
const COMMAND_KIND_REPRICE_PROPERTY_SALE_ORDER: &str = "repricePropertySaleOrder";
const COMMAND_KIND_CANCEL_PROPERTY_SALE_ORDER: &str = "cancelPropertySaleOrder";
const MAX_SALE_ORDER_PAGE_SIZE: u8 = 20;

#[derive(Debug, sqlx::FromRow)]
struct HoldingsScopeRow {
    save_id: u64,
    run_revision: u32,
    purchase_capability: Option<String>,
    maximum_active_holdings: Option<u8>,
}

#[derive(Debug, sqlx::FromRow)]
struct PropertyHoldingRow {
    id: u64,
    property_listing_id: u64,
    status: String,
    purpose: String,
    region_key: String,
    property_type: String,
    exclusive_area_square_meters: u16,
    acquired_game_day: u32,
    acquisition_price_krw: i64,
    acquisition_incidental_cost_krw: i64,
    book_value_krw: i64,
    mortgage_loan_id: Option<u64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedPropertySaveRow {
    id: u64,
    market_world_id: u64,
    policy_set_id: u64,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
    cash_krw: i64,
    debt_krw: i64,
    property_book_value_krw: i64,
    has_character: bool,
}

impl LockedPropertySaveRow {
    fn lease_view(&self) -> LockedLeaseSaveRow {
        LockedLeaseSaveRow {
            id: self.id,
            market_world_id: self.market_world_id,
            policy_set_id: self.policy_set_id,
            run_revision: self.run_revision,
            state_revision: self.state_revision,
            game_day: self.game_day,
            cash_krw: self.cash_krw,
            debt_krw: self.debt_krw,
            has_character: self.has_character,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PropertyRunScopeRow {
    real_estate_model_version_id: u64,
    real_estate_version_key: String,
    real_estate_availability: String,
    real_estate_sealed: bool,
    credit_model_version_id: u64,
    credit_version_key: String,
    credit_sealed: bool,
    credit_policy_set_id: u64,
    market_date: Date,
    purchase_capability: String,
    maximum_active_holdings: u8,
    supported_offer_kind: String,
    supported_purpose: String,
    incidental_cost_ppm: u32,
    minimum_incidental_cost_krw: i64,
    collateral_value_rule: String,
    ltv_cost_treatment: String,
    listing_consumption_scope: String,
    provenance_kind: String,
    regulated_capital_ltv_limit_ppm: u32,
    non_regulated_ltv_limit_ppm: u32,
    lower_price_threshold_krw: i64,
    upper_price_threshold_krw: i64,
    lower_band_cap_krw: i64,
    middle_band_cap_krw: i64,
    upper_band_cap_krw: i64,
    full_term_fixed_stress_rate_bp: u16,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PurchaseListingRow {
    id: u64,
    region_key: String,
    property_type: String,
    exclusive_area_square_meters: u16,
    available_from_game_day: u32,
    available_to_game_day: u32,
    price_krw: i64,
    current_price_index_ppm: i64,
    ltv_region_class: String,
    moving_cost_krw: i64,
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
struct LinkedLeaseLoanEvidenceRow {
    id: u64,
    remaining_principal_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct StoredCommandReceiptRow {
    command_kind: String,
    payload_sha256: String,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
    result_json: String,
    ledger_transaction_id: Option<u64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ExecutableMortgageQuoteRow {
    id: u64,
    command_id: String,
    loan_product_version_id: u64,
    requested_principal_krw: i64,
    created_game_day: u32,
    expires_game_day: u32,
    decision_code: String,
    result_json: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PropertySaleOrderPageRow {
    order_id: u64,
    property_holding_id: u64,
    status: String,
    terminal_game_day: Option<u32>,
    terminal_reason: Option<String>,
    revision_no: u32,
    revision_kind: String,
    asking_price_krw: Option<i64>,
    reference_value_krw: Option<i64>,
    asking_ratio_ppm: Option<u32>,
    candidate_game_day: Option<u32>,
    execution_status: Option<String>,
    execution_game_day: Option<u32>,
    execution_rejection_reason: Option<String>,
    book_value_krw: Option<i64>,
    gross_sale_price_krw: Option<i64>,
    disposition_cost_krw: Option<i64>,
    mortgage_principal_krw: Option<i64>,
    mortgage_prepayment_fee_krw: Option<i64>,
    transfer_tax_krw: Option<i64>,
    net_wallet_proceeds_krw: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PropertySaleScopeRow {
    real_estate_model_version_id: u64,
    capital_gains_policy_rule_id: u64,
    real_estate_version_key: String,
    real_estate_availability: String,
    real_estate_sealed: bool,
    policy_key: String,
    policy_sealed: bool,
    world_seed: u64,
    market_date: Date,
    minimum_asking_ratio_ppm: u32,
    low_band_maximum_ratio_ppm: u32,
    middle_band_maximum_ratio_ppm: u32,
    maximum_asking_ratio_ppm: u32,
    low_delay_minimum_days: u16,
    low_delay_maximum_days: u16,
    middle_delay_minimum_days: u16,
    middle_delay_maximum_days: u16,
    high_delay_minimum_days: u16,
    high_delay_maximum_days: u16,
    candidate_entropy_key: String,
    gross_price_rule: String,
    disposition_cost_ppm: u32,
    minimum_disposition_cost_krw: i64,
    minimum_holding_years: u16,
    minimum_residence_years: u16,
    deficient_sale_proceeds: String,
    post_sale_tenure_type: String,
    provenance_kind: String,
}

impl PropertySaleScopeRow {
    fn liquidity(&self) -> PropertySaleLiquidityProfile {
        PropertySaleLiquidityProfile {
            minimum_asking_ratio_ppm: i64::from(self.minimum_asking_ratio_ppm),
            fast_band_maximum_asking_ratio_ppm: i64::from(self.low_band_maximum_ratio_ppm),
            normal_band_maximum_asking_ratio_ppm: i64::from(self.middle_band_maximum_ratio_ppm),
            maximum_asking_ratio_ppm: i64::from(self.maximum_asking_ratio_ppm),
            fast_band_minimum_delay_days: self.low_delay_minimum_days,
            fast_band_maximum_delay_days: self.low_delay_maximum_days,
            normal_band_minimum_delay_days: self.middle_delay_minimum_days,
            normal_band_maximum_delay_days: self.middle_delay_maximum_days,
            slow_band_minimum_delay_days: self.high_delay_minimum_days,
            slow_band_maximum_delay_days: self.high_delay_maximum_days,
            disposition_cost_rate_ppm: i64::from(self.disposition_cost_ppm),
            minimum_disposition_cost_krw: self.minimum_disposition_cost_krw,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedSaleHoldingRow {
    id: u64,
    household_id: u64,
    property_listing_id: u64,
    region_key: String,
    acquisition_price_krw: i64,
    acquisition_incidental_cost_krw: i64,
    book_value_krw: i64,
    acquisition_price_index_ppm: i64,
    current_price_index_ppm: i64,
    acquired_on: Date,
    owner_residence_id: u64,
    owner_occupied_from: Date,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedSaleOrderRow {
    id: u64,
    property_holding_id: u64,
    status: String,
    current_revision_no: u32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct DueSaleRevisionRow {
    id: u64,
    revision_no: u32,
    candidate_game_day: u32,
    asking_price_krw: i64,
    disposition_cost_ppm: u32,
    minimum_disposition_cost_krw: i64,
    minimum_holding_years: u16,
    minimum_residence_years: u16,
    capital_gains_policy_rule_id: u64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedSaleLienRow {
    loan_contract_id: u64,
    remaining_principal_krw: i64,
    prepayment_fee_ppm: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PropertySaleMortgagePlan {
    None,
    Payable { principal_krw: i64, fee_krw: i64 },
    NotPayable { principal_krw: i64, fee_krw: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PropertySaleFinancialPlan {
    mortgage_principal_krw: i64,
    mortgage_fee_krw: i64,
    transfer_tax_krw: i64,
    net_wallet_proceeds_krw: i64,
    rejection_reason: Option<PropertySaleOrderRejectionReasonState>,
    proceeds: Option<PropertySaleProceedsPlan>,
}

struct PlannedSaleListingRevision {
    reference_value_krw: i64,
    asking_ratio_ppm: i64,
    candidate_game_day: u32,
}

struct LockedPurchaseContext {
    current: LockedPropertySaveRow,
    household_id: u64,
    scope: PropertyRunScopeRow,
    residence: ResidenceProjectionRow,
    active_holding_count: usize,
    existing_lease: Option<LeaseProjectionRow>,
    linked_loan_evidence: Option<LinkedLeaseLoanEvidenceRow>,
    prepared_payoff: Option<Box<PreparedLeaseMovePayoff>>,
    lease_exit_restricted: bool,
    listing: PurchaseListingRow,
}

struct AssessedMortgagePurchase {
    context: LockedPurchaseContext,
    mortgage: Box<MortgageLoanAssessment>,
    incidental_cost_krw: i64,
    ltv_region_class: MortgageLtvRegionClassState,
    ltv_limit_ppm: i64,
    maximum_mortgage_krw: i64,
    ltv: LoanQuoteLtvState,
    available_buyer_cash_krw: i64,
    required_buyer_cash_krw: i64,
    decision_code: MortgageQuoteDecisionState,
    decision_reasons: Vec<MortgageQuoteReasonState>,
}

#[derive(Debug, Clone, Copy)]
enum PropertyLedgerReference {
    None,
    Holding(u64),
    Lease(u64),
    Loan(u64),
    TaxEvent(u64),
}

pub(super) struct ActivePropertyHoldingWindow {
    pub items: Vec<PropertyHoldingState>,
    pub has_more: bool,
    pub total_book_value_krw: i64,
}

pub(super) async fn read_property_holdings(
    pool: &MySqlPool,
    user_id: u64,
) -> Result<Option<HousingPropertyHoldingsState>> {
    let mut tx = pool.begin().await?;
    let scope: Option<HoldingsScopeRow> = sqlx::query_as(
        "SELECT save.id AS save_id, save.run_revision,
                profile.purchase_capability, profile.maximum_active_holdings
         FROM save
         INNER JOIN `character` ON `character`.save_id = save.id
         LEFT JOIN run_rule_bundle AS bundle
           ON bundle.save_id = save.id AND bundle.run_revision = save.run_revision
         LEFT JOIN real_estate_purchase_profile AS profile
           ON profile.real_estate_model_version_id = bundle.real_estate_model_version_id
         WHERE save.user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(scope) = scope else {
        tx.commit().await?;
        return Ok(None);
    };
    let capability = match scope.purchase_capability.as_deref() {
        Some("ownerOccupiedSingleHome") => HousingPurchaseCapabilityState::OwnerOccupiedSingleHome,
        None => HousingPurchaseCapabilityState::Unavailable,
        Some(_) => bail!("stored housing purchase capability is invalid"),
    };
    let maximum_active_holdings = match capability {
        HousingPurchaseCapabilityState::OwnerOccupiedSingleHome => scope
            .maximum_active_holdings
            .filter(|maximum| *maximum > 0)
            .context("active purchase capability has no holding limit")?,
        HousingPurchaseCapabilityState::Unavailable => {
            ensure!(
                scope.maximum_active_holdings.is_none(),
                "unavailable purchase capability exposes a holding limit"
            );
            0
        }
    };
    let window =
        read_active_property_holdings_in_tx(&mut tx, scope.save_id, scope.run_revision).await?;
    if capability == HousingPurchaseCapabilityState::Unavailable {
        ensure!(
            window.items.is_empty() && window.total_book_value_krw == 0,
            "run without purchase capability owns property"
        );
    } else {
        ensure!(
            window.items.len() <= usize::from(maximum_active_holdings),
            "active property holdings exceed the run limit"
        );
    }
    tx.commit().await?;
    Ok(Some(HousingPropertyHoldingsState {
        purchase_capability: capability,
        maximum_active_holdings,
        holdings: window.items,
        total_property_book_value_krw: window.total_book_value_krw,
    }))
}

pub(super) async fn read_property_sale_orders(
    pool: &MySqlPool,
    user_id: u64,
    query: PropertySaleOrderPageQuery,
) -> Result<Option<PropertySaleOrderPageState>> {
    ensure!(
        (1..=MAX_SALE_ORDER_PAGE_SIZE).contains(&query.limit),
        "property sale order page limit is invalid"
    );
    let mut tx = pool.begin().await?;
    let scope: Option<(u64, u32, bool)> = sqlx::query_as(
        "SELECT save.id, save.run_revision,
                EXISTS(SELECT 1 FROM `character` WHERE `character`.save_id = save.id)
         FROM save WHERE save.user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((save_id, run_revision, has_character)) = scope else {
        tx.commit().await?;
        return Ok(None);
    };
    if !has_character {
        tx.commit().await?;
        return Ok(None);
    }
    let fetch_limit = u32::from(query.limit)
        .checked_add(1)
        .context("property sale order page limit overflowed")?;
    let before = query.before.map(ResourceId::get);
    let mut rows: Vec<PropertySaleOrderPageRow> = sqlx::query_as(
        "SELECT sale_order.id AS order_id, sale_order.property_holding_id,
                sale_order.status, sale_order.terminal_game_day, sale_order.terminal_reason,
                revision.revision_no, revision.revision_kind,
                revision.asking_price_krw, revision.reference_value_krw,
                revision.asking_ratio_ppm, revision.candidate_game_day,
                execution.status AS execution_status,
                execution.execution_game_day, execution.rejection_reason
                    AS execution_rejection_reason,
                execution.book_value_krw, execution.gross_sale_price_krw,
                execution.disposition_cost_krw, execution.mortgage_principal_krw,
                execution.mortgage_prepayment_fee_krw, execution.transfer_tax_krw,
                execution.net_wallet_proceeds_krw
         FROM property_sale_order AS sale_order
         INNER JOIN property_sale_order_revision AS revision
           ON revision.save_id = sale_order.save_id
          AND revision.run_revision = sale_order.run_revision
          AND revision.property_sale_order_id = sale_order.id
          AND revision.revision_no = sale_order.current_revision_no
         LEFT JOIN property_sale_execution AS execution
           ON execution.save_id = revision.save_id
          AND execution.run_revision = revision.run_revision
          AND execution.property_sale_order_id = revision.property_sale_order_id
          AND execution.property_sale_order_revision_id = revision.id
          AND execution.status IN ('applied', 'rejected')
         WHERE sale_order.save_id = ? AND sale_order.run_revision = ?
           AND (? IS NULL OR sale_order.id < ?)
         ORDER BY sale_order.id DESC
         LIMIT ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(before)
    .bind(before)
    .bind(fetch_limit)
    .fetch_all(&mut *tx)
    .await?;
    let has_more = rows.len() > usize::from(query.limit);
    rows.truncate(usize::from(query.limit));
    let next_before = has_more
        .then(|| rows.last().map(|row| ResourceId::from_u64(row.order_id)))
        .flatten();
    let items = rows
        .into_iter()
        .map(property_sale_order_summary_from_row)
        .collect::<Result<Vec<_>>>()?;
    tx.commit().await?;
    Ok(Some(PropertySaleOrderPageState { items, next_before }))
}

fn property_sale_order_summary_from_row(
    row: PropertySaleOrderPageRow,
) -> Result<PropertySaleOrderSummaryState> {
    let status = property_sale_order_status_from_db(&row.status)?;
    let revision_kind = match row.revision_kind.as_str() {
        "listing" => PropertySaleOrderRevisionKindState::Listing,
        "cancellation" => PropertySaleOrderRevisionKindState::Cancellation,
        _ => bail!("stored property sale revision kind is invalid"),
    };
    match revision_kind {
        PropertySaleOrderRevisionKindState::Listing => ensure!(
            row.asking_price_krw.is_some()
                && row.reference_value_krw.is_some()
                && row.asking_ratio_ppm.is_some()
                && row.candidate_game_day.is_some(),
            "listing sale revision is missing its immutable terms"
        ),
        PropertySaleOrderRevisionKindState::Cancellation => ensure!(
            row.asking_price_krw.is_none()
                && row.reference_value_krw.is_none()
                && row.asking_ratio_ppm.is_none()
                && row.candidate_game_day.is_none(),
            "cancellation sale revision retained listing terms"
        ),
    }
    let rejection_reason = match row.execution_rejection_reason.as_deref() {
        Some(reason) => Some(property_sale_rejection_reason_from_db(reason)?),
        None => None,
    };
    let execution = match row.execution_status.as_deref() {
        Some("applied") => {
            ensure!(
                status == PropertySaleOrderStatusState::Filled && rejection_reason.is_none(),
                "applied property sale execution disagrees with its order"
            );
            let book_value_krw = row
                .book_value_krw
                .context("applied property sale execution has no book value")?;
            let gross_sale_price_krw = row
                .gross_sale_price_krw
                .context("applied property sale execution has no gross price")?;
            Some(PropertySaleExecutionState {
                filled_game_day: row
                    .execution_game_day
                    .context("applied property sale execution has no game day")?,
                gross_sale_price_krw,
                transaction_cost_krw: row
                    .disposition_cost_krw
                    .context("applied property sale execution has no disposition cost")?,
                mortgage_principal_krw: row
                    .mortgage_principal_krw
                    .context("applied property sale execution has no mortgage principal")?,
                mortgage_fee_krw: row
                    .mortgage_prepayment_fee_krw
                    .context("applied property sale execution has no mortgage fee")?,
                capital_gains_tax_krw: row
                    .transfer_tax_krw
                    .context("applied property sale execution has no transfer tax")?,
                wallet_proceeds_krw: row
                    .net_wallet_proceeds_krw
                    .context("applied property sale execution has no wallet proceeds")?,
                realized_gain_loss_krw: book_value_krw
                    .checked_sub(gross_sale_price_krw)
                    .context("property sale realized gain/loss overflowed")?,
            })
        }
        Some("rejected") => {
            ensure!(
                status == PropertySaleOrderStatusState::Rejected
                    && rejection_reason.is_some()
                    && row.execution_game_day.is_some(),
                "rejected property sale execution disagrees with its order"
            );
            None
        }
        Some(_) => bail!("stored property sale execution status is invalid"),
        None => {
            ensure!(
                matches!(
                    status,
                    PropertySaleOrderStatusState::Active | PropertySaleOrderStatusState::Cancelled
                ),
                "terminal property sale order has no execution"
            );
            None
        }
    };
    if status == PropertySaleOrderStatusState::Cancelled {
        ensure!(
            matches!(
                row.terminal_reason.as_deref(),
                Some("userRequest" | "newRun")
            ) && row.terminal_game_day.is_some(),
            "cancelled property sale order has invalid terminal evidence"
        );
    }
    Ok(PropertySaleOrderSummaryState {
        order_id: ResourceId::from_u64(row.order_id),
        holding_id: ResourceId::from_u64(row.property_holding_id),
        revision_no: row.revision_no,
        revision_kind,
        asking_price_krw: row.asking_price_krw,
        reference_value_krw: row.reference_value_krw,
        asking_to_reference_ppm: row.asking_ratio_ppm.map(i64::from),
        candidate_game_day: row.candidate_game_day,
        status,
        cancelled_game_day: (status == PropertySaleOrderStatusState::Cancelled)
            .then_some(row.terminal_game_day)
            .flatten(),
        rejection_reason,
        execution,
    })
}

fn property_sale_order_status_from_db(value: &str) -> Result<PropertySaleOrderStatusState> {
    match value {
        "active" => Ok(PropertySaleOrderStatusState::Active),
        "filled" => Ok(PropertySaleOrderStatusState::Filled),
        "cancelled" => Ok(PropertySaleOrderStatusState::Cancelled),
        "rejected" => Ok(PropertySaleOrderStatusState::Rejected),
        _ => bail!("stored property sale order status is invalid"),
    }
}

fn property_sale_rejection_reason_from_db(
    value: &str,
) -> Result<PropertySaleOrderRejectionReasonState> {
    match value {
        "mortgageNotPayable" => Ok(PropertySaleOrderRejectionReasonState::MortgageNotPayable),
        "insufficientProceeds" => Ok(PropertySaleOrderRejectionReasonState::InsufficientProceeds),
        "policyUnsupported" => Ok(PropertySaleOrderRejectionReasonState::PolicyUnsupported),
        _ => bail!("stored property sale rejection reason is invalid"),
    }
}

pub(super) async fn read_active_property_holdings_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<ActivePropertyHoldingWindow> {
    let mut rows: Vec<PropertyHoldingRow> = sqlx::query_as(
        "SELECT holding.id, holding.property_listing_id, holding.status, holding.purpose,
                holding.region_key, holding.property_type,
                holding.exclusive_area_square_meters, holding.acquired_game_day,
                holding.acquisition_price_krw,
                holding.acquisition_incidental_cost_krw, holding.book_value_krw,
                mortgage.id AS mortgage_loan_id
         FROM property_holding AS holding
         LEFT JOIN property_lien AS lien
           ON lien.save_id = holding.save_id
          AND lien.run_revision = holding.run_revision
          AND lien.property_holding_id = holding.id
          AND lien.status = 'active'
         LEFT JOIN loan_contract AS mortgage
           ON mortgage.id = lien.loan_contract_id
          AND mortgage.save_id = holding.save_id
          AND mortgage.run_revision = holding.run_revision
          AND mortgage.property_holding_id = holding.id
          AND mortgage.product_kind = 'mortgage'
          AND mortgage.status IN ('active', 'delinquent', 'defaulted', 'restructured')
         WHERE holding.save_id = ? AND holding.run_revision = ? AND holding.status = 'active'
         ORDER BY holding.id
         LIMIT 5",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    let has_more = rows.len() > MAX_SNAPSHOT_HOLDINGS;
    rows.truncate(MAX_SNAPSHOT_HOLDINGS);
    let items = rows
        .into_iter()
        .map(property_holding_from_row)
        .collect::<Result<Vec<_>>>()?;
    let total_book_value_krw = items.iter().try_fold(0_i64, |total, item| {
        total
            .checked_add(item.book_value_krw)
            .context("property book-value projection overflowed")
    })?;
    Ok(ActivePropertyHoldingWindow {
        items,
        has_more,
        total_book_value_krw,
    })
}

pub(super) async fn validate_property_projection_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<()> {
    let (projected, stored): (String, i64) = sqlx::query_as(
        "SELECT CAST(COALESCE(SUM(holding.book_value_krw), 0) AS CHAR), save.property_book_value_krw
         FROM save
         LEFT JOIN property_holding AS holding
           ON holding.save_id = save.id
          AND holding.run_revision = save.run_revision
          AND holding.status = 'active'
         WHERE save.id = ? AND save.run_revision = ?
         GROUP BY save.id, save.property_book_value_krw",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        projected
            .parse::<i64>()
            .context("property book-value sum is out of range")?
            == stored,
        "save property book value disagrees with active holdings"
    );
    Ok(())
}

fn property_holding_from_row(row: PropertyHoldingRow) -> Result<PropertyHoldingState> {
    let status = match row.status.as_str() {
        "active" => PropertyHoldingStatusState::Active,
        "disposed" => PropertyHoldingStatusState::Disposed,
        _ => bail!("stored property holding status is invalid"),
    };
    let purpose = match row.purpose.as_str() {
        "ownerOccupied" => PropertyHoldingPurposeState::OwnerOccupied,
        _ => bail!("stored property holding purpose is invalid"),
    };
    ensure!(
        row.acquisition_price_krw > 0
            && row.acquisition_incidental_cost_krw > 0
            && row.book_value_krw == row.acquisition_price_krw,
        "stored property holding amounts are invalid"
    );
    Ok(PropertyHoldingState {
        id: ResourceId::from_u64(row.id),
        listing_id: ResourceId::from_u64(row.property_listing_id),
        status,
        purpose,
        region_key: LifeRegionKey::from_str(&row.region_key)
            .context("stored property holding region is invalid")?,
        property_type: PropertyType::from_str(&row.property_type)
            .context("stored property holding type is invalid")?,
        exclusive_area_square_meters: row.exclusive_area_square_meters,
        acquired_game_day: row.acquired_game_day,
        acquisition_price_krw: row.acquisition_price_krw,
        acquisition_incidental_cost_krw: row.acquisition_incidental_cost_krw,
        book_value_krw: row.book_value_krw,
        mortgage_loan_id: row.mortgage_loan_id.map(ResourceId::from_u64),
    })
}

pub(super) async fn quote_mortgage_command(
    pool: &MySqlPool,
    real_estate_rules: &dyn RealEstateRules,
    property_rules: &dyn PropertyRules,
    user_id: u64,
    command: &CreateMortgageQuoteCommand,
) -> Result<LifeStoreResult<MortgageQuoteReceipt>> {
    if let Err(error) = prepare_current_housing_catalogs(pool, real_estate_rules, user_id).await {
        if is_retryable_database_error(&error) {
            return Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy));
        }
        return Err(error);
    }
    let fingerprint = mortgage_quote_fingerprint(command);
    for _ in 0..MAX_TRANSACTION_ATTEMPTS {
        match quote_mortgage_once(pool, property_rules, user_id, command, &fingerprint).await {
            Ok(result) => return Ok(result),
            Err(error) if is_retryable_database_error(&error) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy))
}

pub(super) async fn purchase_property_command(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    real_estate_rules: &dyn RealEstateRules,
    property_rules: &dyn PropertyRules,
    property_tax_rules: &dyn PropertyTaxRules,
    user_id: u64,
    command: &PurchasePropertyCommand,
) -> Result<LifeStoreResult<PropertyPurchaseReceipt>> {
    if let Err(error) = prepare_current_housing_catalogs(pool, real_estate_rules, user_id).await {
        if is_retryable_database_error(&error) {
            return Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy));
        }
        return Err(error);
    }
    let fingerprint = property_purchase_fingerprint(command);
    for _ in 0..MAX_TRANSACTION_ATTEMPTS {
        match purchase_property_once(
            pool,
            finance_rules,
            property_rules,
            property_tax_rules,
            user_id,
            command,
            &fingerprint,
        )
        .await
        {
            Ok(result) => return Ok(result),
            Err(error) if is_retryable_database_error(&error) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy))
}

pub(super) async fn create_property_sale_order_command(
    pool: &MySqlPool,
    real_estate_rules: &dyn RealEstateRules,
    property_rules: &dyn PropertyRules,
    user_id: u64,
    command: &CreatePropertySaleOrderCommand,
) -> Result<LifeStoreResult<PropertySaleOrderListingReceipt>> {
    if let Err(error) = prepare_current_housing_catalogs(pool, real_estate_rules, user_id).await {
        if is_retryable_database_error(&error) {
            return Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy));
        }
        return Err(error);
    }
    let fingerprint = create_property_sale_order_fingerprint(command);
    for _ in 0..MAX_TRANSACTION_ATTEMPTS {
        match create_property_sale_order_once(pool, property_rules, user_id, command, &fingerprint)
            .await
        {
            Ok(result) => return Ok(result),
            Err(error) if is_retryable_database_error(&error) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy))
}

pub(super) async fn reprice_property_sale_order_command(
    pool: &MySqlPool,
    real_estate_rules: &dyn RealEstateRules,
    property_rules: &dyn PropertyRules,
    user_id: u64,
    command: &RepricePropertySaleOrderCommand,
) -> Result<LifeStoreResult<PropertySaleOrderListingReceipt>> {
    if let Err(error) = prepare_current_housing_catalogs(pool, real_estate_rules, user_id).await {
        if is_retryable_database_error(&error) {
            return Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy));
        }
        return Err(error);
    }
    let fingerprint = reprice_property_sale_order_fingerprint(command);
    for _ in 0..MAX_TRANSACTION_ATTEMPTS {
        match reprice_property_sale_order_once(pool, property_rules, user_id, command, &fingerprint)
            .await
        {
            Ok(result) => return Ok(result),
            Err(error) if is_retryable_database_error(&error) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy))
}

pub(super) async fn cancel_property_sale_order_command(
    pool: &MySqlPool,
    user_id: u64,
    command: &CancelPropertySaleOrderCommand,
) -> Result<LifeStoreResult<PropertySaleOrderCancellationReceipt>> {
    let fingerprint = cancel_property_sale_order_fingerprint(command);
    for _ in 0..MAX_TRANSACTION_ATTEMPTS {
        match cancel_property_sale_order_once(pool, user_id, command, &fingerprint).await {
            Ok(result) => return Ok(result),
            Err(error) if is_retryable_database_error(&error) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy))
}

pub(super) async fn close_property_sales_for_new_run_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
) -> Result<()> {
    let due: Vec<(u64, u64, u64, u64, u64)> = sqlx::query_as(
        "SELECT sale_order.id, sale_order.property_holding_id,
                revision.real_estate_model_version_id, revision.policy_set_id,
                revision.capital_gains_policy_rule_id
         FROM property_sale_order AS sale_order
         INNER JOIN property_sale_order_revision AS revision
           ON revision.save_id = sale_order.save_id
          AND revision.run_revision = sale_order.run_revision
          AND revision.property_sale_order_id = sale_order.id
          AND revision.revision_no = sale_order.current_revision_no
         WHERE sale_order.save_id = ? AND sale_order.run_revision = ?
           AND sale_order.status = 'active' AND revision.revision_kind = 'listing'
         ORDER BY sale_order.property_holding_id, sale_order.id",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    if due.is_empty() {
        return Ok(());
    }
    let mut holding_ids = due
        .iter()
        .map(|(_, holding_id, _, _, _)| *holding_id)
        .collect::<Vec<_>>();
    holding_ids.sort_unstable();
    holding_ids.dedup();
    for holding_id in holding_ids {
        let locked: Option<u64> = sqlx::query_scalar(
            "SELECT id FROM property_holding
             WHERE id = ? AND save_id = ? AND run_revision = ? FOR UPDATE",
        )
        .bind(holding_id)
        .bind(save_id)
        .bind(run_revision)
        .fetch_optional(&mut **tx)
        .await?;
        ensure!(
            locked == Some(holding_id),
            "new-run property sale cleanup lost a holding"
        );
    }
    for (order_id, holding_id, model_id, policy_set_id, capital_rule_id) in due {
        let order: LockedSaleOrderRow = sqlx::query_as(
            "SELECT id, property_holding_id, status, current_revision_no
             FROM property_sale_order
             WHERE id = ? AND save_id = ? AND run_revision = ?
               AND property_holding_id = ? FOR UPDATE",
        )
        .bind(order_id)
        .bind(save_id)
        .bind(run_revision)
        .bind(holding_id)
        .fetch_one(&mut **tx)
        .await?;
        ensure!(
            order.status == "active",
            "new-run property sale order changed after collection"
        );
        let revision_no = order
            .current_revision_no
            .checked_add(1)
            .context("new-run property sale revision overflowed")?;
        sqlx::query(
            "INSERT INTO property_sale_order_revision
                 (save_id, run_revision, property_sale_order_id, property_holding_id,
                  revision_no, revision_kind, command_id, cancellation_reason,
                  created_game_day, real_estate_model_version_id, policy_set_id,
                  capital_gains_policy_rule_id,
                  asking_price_krw, reference_value_krw,
                  acquisition_price_index_ppm, current_price_index_ppm,
                  asking_ratio_ppm, candidate_game_day, gross_price_rule,
                  disposition_cost_ppm, minimum_disposition_cost_krw,
                  deficient_sale_proceeds, minimum_holding_years,
                  minimum_residence_years)
             VALUES (?, ?, ?, ?, ?, 'cancellation', NULL, 'newRun', ?, ?, ?, ?,
                     NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
                     NULL, NULL)",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(order_id)
        .bind(holding_id)
        .bind(revision_no)
        .bind(game_day)
        .bind(model_id)
        .bind(policy_set_id)
        .bind(capital_rule_id)
        .execute(&mut **tx)
        .await?;
        let updated = sqlx::query(
            "UPDATE property_sale_order
             SET status = 'cancelled', current_revision_no = ?,
                 terminal_game_day = ?, terminal_reason = 'newRun'
             WHERE id = ? AND save_id = ? AND run_revision = ?
               AND property_holding_id = ? AND status = 'active'
               AND current_revision_no = ?",
        )
        .bind(revision_no)
        .bind(game_day)
        .bind(order_id)
        .bind(save_id)
        .bind(run_revision)
        .bind(holding_id)
        .bind(order.current_revision_no)
        .execute(&mut **tx)
        .await?;
        ensure!(
            updated.rows_affected() == 1,
            "new-run property sale cleanup changed concurrently"
        );
    }
    Ok(())
}

pub(super) async fn execute_due_property_sales_in_tx(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    property_rules: &dyn PropertyRules,
    property_tax_rules: &dyn PropertyTaxRules,
    context: PropertyTaxRunContext,
) -> Result<()> {
    let due: Vec<(u64, u64, u32)> = sqlx::query_as(
        "SELECT sale_order.id, sale_order.property_holding_id,
                revision.candidate_game_day
         FROM property_sale_order AS sale_order
         INNER JOIN property_sale_order_revision AS revision
           ON revision.save_id = sale_order.save_id
          AND revision.run_revision = sale_order.run_revision
          AND revision.property_sale_order_id = sale_order.id
          AND revision.revision_no = sale_order.current_revision_no
         WHERE sale_order.save_id = ? AND sale_order.run_revision = ?
           AND sale_order.status = 'active' AND revision.revision_kind = 'listing'
           AND revision.candidate_game_day <= ?
         ORDER BY sale_order.property_holding_id, sale_order.id",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(context.game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        due.iter()
            .all(|(_, _, candidate_game_day)| *candidate_game_day == context.game_day),
        "property sale pipeline found an overdue candidate"
    );
    if due.is_empty() {
        return Ok(());
    }
    ensure!(
        due.len() == 1,
        "single-home sale pipeline found multiple due orders"
    );
    let current: LockedPropertySaveRow = sqlx::query_as(
        "SELECT save.id, save.market_world_id, save.policy_set_id,
                save.run_revision, save.state_revision, save.game_day,
                save.cash_krw, save.debt_krw, save.property_book_value_krw,
                EXISTS(SELECT 1 FROM `character` WHERE `character`.save_id = save.id)
                    AS has_character
         FROM save WHERE save.id = ? AND save.run_revision = ?
           AND save.policy_set_id = ? FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(context.policy_set_id)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        current.market_world_id == context.market_world_id
            && current.game_day.checked_add(1) == Some(context.game_day)
            && current.has_character,
        "property sale pipeline context is stale"
    );
    let mut target = current.clone();
    target.game_day = context.game_day;
    let scope = read_property_sale_scope(tx, &target)
        .await?
        .context("due property sale has no sealed C4 scope")?;
    ensure!(
        scope.market_date == context.market_date,
        "property sale market date disagrees with the daily pipeline"
    );
    let household_id = lock_current_household(tx, &current).await?;
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

    for (order_id, holding_id, _) in due {
        execute_one_due_property_sale(
            tx,
            finance_rules,
            property_rules,
            property_tax_rules,
            context,
            &current,
            &target,
            &scope,
            household_id,
            order_id,
            holding_id,
        )
        .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn execute_one_due_property_sale(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    property_rules: &dyn PropertyRules,
    property_tax_rules: &dyn PropertyTaxRules,
    context: PropertyTaxRunContext,
    current_at_phase_start: &LockedPropertySaveRow,
    target: &LockedPropertySaveRow,
    scope: &PropertySaleScopeRow,
    household_id: u64,
    order_id: u64,
    holding_id: u64,
) -> Result<()> {
    let residence_ids: Vec<(u64,)> = sqlx::query_as(
        "SELECT id FROM residence
         WHERE save_id = ? AND run_revision = ? AND household_id = ?
           AND property_holding_id = ? AND tenure_type = 'owner'
           AND effective_from_game_day <= ?
           AND (effective_to_game_day IS NULL OR effective_to_game_day > ?)
         ORDER BY id FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(household_id)
    .bind(holding_id)
    .bind(context.game_day)
    .bind(context.game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        residence_ids.len() == 1,
        "due property sale has no unique owner residence"
    );
    let holding = lock_sale_holding(
        tx,
        target,
        scope,
        household_id,
        ResourceId::from_u64(holding_id),
    )
    .await?
    .context("due property sale holding disappeared")?;
    let order = lock_sale_order(
        tx,
        target,
        household_id,
        ResourceId::from_u64(holding_id),
        Some(ResourceId::from_u64(order_id)),
    )
    .await?
    .context("due property sale order disappeared")?;
    ensure!(
        order.status == "active",
        "due property sale order is not active"
    );
    let revision: DueSaleRevisionRow = sqlx::query_as(
        "SELECT id, revision_no, candidate_game_day, asking_price_krw,
                disposition_cost_ppm, minimum_disposition_cost_krw,
                minimum_holding_years, minimum_residence_years,
                capital_gains_policy_rule_id
         FROM property_sale_order_revision
         WHERE save_id = ? AND run_revision = ? AND property_sale_order_id = ?
           AND property_holding_id = ? AND revision_no = ?
           AND revision_kind = 'listing' FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(order_id)
    .bind(holding_id)
    .bind(order.current_revision_no)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        revision.revision_no == order.current_revision_no
            && revision.candidate_game_day == context.game_day
            && revision.capital_gains_policy_rule_id == scope.capital_gains_policy_rule_id,
        "due property sale revision is not authoritative"
    );
    let lien_rows: Vec<LockedSaleLienRow> = sqlx::query_as(
        "SELECT lien.loan_contract_id, mortgage.remaining_principal_krw,
                mortgage.prepayment_fee_ppm
         FROM property_lien AS lien
         INNER JOIN loan_contract AS mortgage
           ON mortgage.id = lien.loan_contract_id
          AND mortgage.save_id = lien.save_id
          AND mortgage.run_revision = lien.run_revision
          AND mortgage.property_holding_id = lien.property_holding_id
         WHERE lien.save_id = ? AND lien.run_revision = ?
           AND lien.property_holding_id = ? AND lien.status = 'active'
         ORDER BY lien.lien_priority, lien.id FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(holding_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        lien_rows.len() <= 1,
        "property holding has multiple active liens"
    );
    if lien_rows.is_empty() {
        let unlinked: Vec<(u64,)> = sqlx::query_as(
            "SELECT id FROM loan_contract
             WHERE save_id = ? AND run_revision = ? AND property_holding_id = ?
               AND product_kind = 'mortgage' AND remaining_principal_krw > 0
               AND status IN ('active', 'delinquent', 'defaulted', 'restructured')
             ORDER BY id FOR UPDATE",
        )
        .bind(context.save_id)
        .bind(context.run_revision)
        .bind(holding_id)
        .fetch_all(&mut **tx)
        .await?;
        ensure!(unlinked.is_empty(), "active mortgage has no property lien");
    }
    let lien = lien_rows.first();
    let payoff_preparation = prepare_property_sale_payoff_in_tx(
        tx,
        context.save_id,
        context.run_revision,
        holding_id,
        lien.map(|row| row.loan_contract_id),
    )
    .await?;
    let mortgage_plan = property_sale_mortgage_plan(lien, &payoff_preparation)?;
    let disposition_cost_krw = property_rules
        .calculate_disposition_cost(PropertyDispositionCostInput {
            gross_sale_price_krw: revision.asking_price_krw,
            disposition_cost_rate_ppm: i64::from(revision.disposition_cost_ppm),
            minimum_disposition_cost_krw: revision.minimum_disposition_cost_krw,
        })
        .context("property sale disposition-cost calculation failed")?;
    let period = property_rules
        .calculate_sale_period(PropertySalePeriodInput {
            acquired_on: holding.acquired_on,
            owner_occupied_from: holding.owner_occupied_from,
            as_of: context.market_date,
            minimum_holding_years: revision.minimum_holding_years,
            minimum_residence_years: revision.minimum_residence_years,
        })
        .context("property sale period calculation failed")?;
    let acquisition_tax_rows: Vec<(u64, i64)> = sqlx::query_as(
        "SELECT id, total_tax_krw FROM property_tax_event
         WHERE save_id = ? AND run_revision = ? AND property_holding_id = ?
           AND event_kind = 'acquisition' ORDER BY id FOR UPDATE",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(holding_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        acquisition_tax_rows.len() == 1 && acquisition_tax_rows[0].1 >= 0,
        "property sale has no unique acquisition-tax basis"
    );
    let acquisition_taxes_krw = acquisition_tax_rows[0].1;
    let tax_input = CapitalGainsPropertyTaxEventInput {
        context,
        household_id,
        holding_id,
        property_sale_execution_id: 0,
        sale_price_krw: revision.asking_price_krw,
        acquisition_price_krw: holding.acquisition_price_krw,
        acquisition_incidental_cost_krw: holding.acquisition_incidental_cost_krw,
        acquisition_taxes_krw,
        disposition_cost_krw,
        acquired_on: holding.acquired_on,
        owner_occupied_from: holding.owner_occupied_from,
        valuation_price_index_ppm: holding.current_price_index_ppm,
    };
    let mut policy_unsupported = !period.is_eligible;
    let capital_gains = if policy_unsupported {
        None
    } else {
        match calculate_capital_gains_property_tax_in_tx(tx, property_tax_rules, tax_input).await? {
            Ok(calculation) => Some(calculation),
            Err(PropertyTaxError::PolicyUnsupported) => {
                policy_unsupported = true;
                None
            }
            Err(error) => return Err(error).context("property sale capital-gains tax failed"),
        }
    };
    let (national_tax_krw, local_tax_krw) = capital_gains
        .map(|calculation| (calculation.national.tax_krw, calculation.local.tax_krw))
        .unwrap_or((0, 0));
    let mut financial = plan_property_sale_financials(
        property_rules,
        revision.asking_price_krw,
        holding.book_value_krw,
        disposition_cost_krw,
        mortgage_plan,
        national_tax_krw,
        local_tax_krw,
    )?;
    if policy_unsupported {
        financial.rejection_reason = Some(PropertySaleOrderRejectionReasonState::PolicyUnsupported);
        financial.proceeds = None;
    }
    let execution_id = insert_property_sale_execution(
        tx,
        context,
        order_id,
        revision.id,
        holding_id,
        holding.book_value_krw,
        revision.asking_price_krw,
        disposition_cost_krw,
        &financial,
    )
    .await?;
    if let Some(reason) = financial.rejection_reason {
        reject_property_sale_order(tx, context, order_id, execution_id, reason).await?;
        return Ok(());
    }
    let proceeds = financial
        .proceeds
        .as_ref()
        .context("applicable property sale has no proceeds plan")?;
    let mut applied_tax_input = tax_input;
    applied_tax_input.property_sale_execution_id = execution_id;
    let tax_event =
        create_capital_gains_property_tax_event_in_tx(tx, property_tax_rules, applied_tax_input)
            .await?;
    ensure!(
        capital_gains == Some(tax_event.calculation)
            && tax_event.calculation.total_tax_krw == financial.transfer_tax_krw,
        "property sale capital-gains tax changed during application"
    );
    let payoff_application = match payoff_preparation {
        PropertySalePayoffPreparation::None => None,
        PropertySalePayoffPreparation::Prepared(prepared) => Some(
            apply_property_sale_payoff_in_tx(
                tx,
                context.save_id,
                context.run_revision,
                holding_id,
                execution_id,
                context.game_day,
                *prepared,
            )
            .await?,
        ),
        PropertySalePayoffPreparation::MortgageNotPayable => {
            bail!("rejected mortgage payoff reached property sale application")
        }
    };
    close_owner_residence_for_property_sale(tx, context, holding.owner_residence_id, holding_id)
        .await?;
    let replacement_residence_id = insert_rent_free_residence_after_property_sale(
        tx,
        context,
        household_id,
        &holding.region_key,
    )
    .await?;
    dispose_property_holding_after_sale(tx, context, holding_id).await?;
    let ledger_transaction_id = write_property_sale_ledger(
        tx,
        finance_rules,
        context,
        execution_id,
        holding_id,
        payoff_application.map(|payoff| payoff.loan_id.get()),
        tax_event.event_id.get(),
        proceeds,
    )
    .await?;
    if let Some(payoff) = payoff_application {
        mark_property_sale_payoff_applied_in_tx(
            tx,
            context.save_id,
            context.run_revision,
            execution_id,
            payoff,
            ledger_transaction_id,
        )
        .await?;
    }
    apply_property_sale_execution(
        tx,
        context,
        execution_id,
        ledger_transaction_id,
        replacement_residence_id,
    )
    .await?;
    fill_property_sale_order(tx, context, order_id, execution_id).await?;
    update_save_after_property_sale(
        tx,
        context,
        current_at_phase_start,
        financial.net_wallet_proceeds_krw,
        financial.mortgage_principal_krw,
        holding.book_value_krw,
    )
    .await?;
    validate_debt_projection_in_tx(tx, context.save_id, context.run_revision).await?;
    validate_property_projection_in_tx(tx, context.save_id, context.run_revision).await?;
    Ok(())
}

fn property_sale_mortgage_plan(
    lien: Option<&LockedSaleLienRow>,
    preparation: &PropertySalePayoffPreparation,
) -> Result<PropertySaleMortgagePlan> {
    match (lien, preparation) {
        (None, PropertySalePayoffPreparation::None) => Ok(PropertySaleMortgagePlan::None),
        (Some(lien), PropertySalePayoffPreparation::Prepared(prepared)) => {
            ensure!(
                prepared.loan_id().get() == lien.loan_contract_id
                    && prepared.principal_krw() == lien.remaining_principal_krw,
                "prepared property-sale mortgage disagrees with its lien"
            );
            Ok(PropertySaleMortgagePlan::Payable {
                principal_krw: prepared.principal_krw(),
                fee_krw: prepared.fee_krw(),
            })
        }
        (Some(lien), PropertySalePayoffPreparation::MortgageNotPayable) => {
            let principal_krw = lien.remaining_principal_krw.max(0);
            let fee_krw = match lien.prepayment_fee_ppm {
                Some(rate_ppm) => i64::try_from(
                    i128::from(principal_krw)
                        .checked_mul(i128::from(rate_ppm))
                        .and_then(|value| value.checked_div(1_000_000))
                        .context("property-sale rejected mortgage fee overflowed")?,
                )?,
                None => 0,
            };
            Ok(PropertySaleMortgagePlan::NotPayable {
                principal_krw,
                fee_krw,
            })
        }
        _ => bail!("property-sale mortgage preparation disagrees with its lien"),
    }
}

#[allow(clippy::too_many_arguments)]
async fn insert_property_sale_execution(
    tx: &mut Transaction<'_, MySql>,
    context: PropertyTaxRunContext,
    order_id: u64,
    revision_id: u64,
    holding_id: u64,
    book_value_krw: i64,
    gross_sale_price_krw: i64,
    disposition_cost_krw: i64,
    financial: &PropertySaleFinancialPlan,
) -> Result<u64> {
    let (status, rejection_reason) = match financial.rejection_reason {
        Some(reason) => ("rejected", Some(property_sale_rejection_db(reason))),
        None => ("prepared", None),
    };
    let inserted = sqlx::query(
        "INSERT INTO property_sale_execution
             (save_id, run_revision, property_sale_order_id,
              property_sale_order_revision_id, property_holding_id,
              execution_game_day, status, rejection_reason, book_value_krw,
              gross_sale_price_krw, disposition_cost_krw,
              mortgage_principal_krw, mortgage_prepayment_fee_krw,
              transfer_tax_krw, net_wallet_proceeds_krw,
              ledger_transaction_id, replacement_residence_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL)",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(order_id)
    .bind(revision_id)
    .bind(holding_id)
    .bind(context.game_day)
    .bind(status)
    .bind(rejection_reason)
    .bind(book_value_krw)
    .bind(gross_sale_price_krw)
    .bind(disposition_cost_krw)
    .bind(financial.mortgage_principal_krw)
    .bind(financial.mortgage_fee_krw)
    .bind(financial.transfer_tax_krw)
    .bind(financial.net_wallet_proceeds_krw)
    .execute(&mut **tx)
    .await?;
    let execution_id = inserted.last_insert_id();
    ensure!(execution_id > 0, "property sale execution has no identity");
    Ok(execution_id)
}

async fn reject_property_sale_order(
    tx: &mut Transaction<'_, MySql>,
    context: PropertyTaxRunContext,
    order_id: u64,
    execution_id: u64,
    reason: PropertySaleOrderRejectionReasonState,
) -> Result<()> {
    let execution_status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM property_sale_execution
         WHERE id = ? AND save_id = ? AND run_revision = ?",
    )
    .bind(execution_id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .fetch_optional(&mut **tx)
    .await?;
    ensure!(
        execution_status.as_deref() == Some("rejected"),
        "property sale rejection has no rejected execution"
    );
    let update = sqlx::query(
        "UPDATE property_sale_order
         SET status = 'rejected', terminal_game_day = ?, terminal_reason = ?
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'active'",
    )
    .bind(context.game_day)
    .bind(property_sale_rejection_db(reason))
    .bind(order_id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "property sale order lost its active state before rejection"
    );
    Ok(())
}

async fn close_owner_residence_for_property_sale(
    tx: &mut Transaction<'_, MySql>,
    context: PropertyTaxRunContext,
    residence_id: u64,
    holding_id: u64,
) -> Result<()> {
    let update = sqlx::query(
        "UPDATE residence SET effective_to_game_day = ?
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND property_holding_id = ? AND tenure_type = 'owner'
           AND effective_from_game_day < ? AND effective_to_game_day IS NULL",
    )
    .bind(context.game_day)
    .bind(residence_id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(holding_id)
    .bind(context.game_day)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "property sale owner residence changed before closing"
    );
    Ok(())
}

async fn insert_rent_free_residence_after_property_sale(
    tx: &mut Transaction<'_, MySql>,
    context: PropertyTaxRunContext,
    household_id: u64,
    region_key: &str,
) -> Result<u64> {
    let inserted = sqlx::query(
        "INSERT INTO residence
             (save_id, run_revision, household_id, region_key, tenure_type,
              lease_contract_id, property_holding_id,
              effective_from_game_day, effective_to_game_day)
         VALUES (?, ?, ?, ?, 'rentFree', NULL, NULL, ?, NULL)",
    )
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(household_id)
    .bind(region_key)
    .bind(context.game_day)
    .execute(&mut **tx)
    .await?;
    let residence_id = inserted.last_insert_id();
    ensure!(
        residence_id > 0,
        "property sale replacement residence has no identity"
    );
    Ok(residence_id)
}

async fn dispose_property_holding_after_sale(
    tx: &mut Transaction<'_, MySql>,
    context: PropertyTaxRunContext,
    holding_id: u64,
) -> Result<()> {
    let update = sqlx::query(
        "UPDATE property_holding
         SET status = 'disposed', disposed_game_day = ?
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND status = 'active' AND disposed_game_day IS NULL",
    )
    .bind(context.game_day)
    .bind(holding_id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "property holding changed before sale disposal"
    );
    Ok(())
}

async fn apply_property_sale_execution(
    tx: &mut Transaction<'_, MySql>,
    context: PropertyTaxRunContext,
    execution_id: u64,
    ledger_transaction_id: u64,
    replacement_residence_id: u64,
) -> Result<()> {
    let update = sqlx::query(
        "UPDATE property_sale_execution
         SET status = 'applied', ledger_transaction_id = ?, replacement_residence_id = ?
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'prepared'",
    )
    .bind(ledger_transaction_id)
    .bind(replacement_residence_id)
    .bind(execution_id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "property sale execution lost its prepared state"
    );
    Ok(())
}

async fn fill_property_sale_order(
    tx: &mut Transaction<'_, MySql>,
    context: PropertyTaxRunContext,
    order_id: u64,
    execution_id: u64,
) -> Result<()> {
    let execution_status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM property_sale_execution
         WHERE id = ? AND save_id = ? AND run_revision = ?",
    )
    .bind(execution_id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .fetch_optional(&mut **tx)
    .await?;
    ensure!(
        execution_status.as_deref() == Some("applied"),
        "filled property sale has no applied execution"
    );
    let update = sqlx::query(
        "UPDATE property_sale_order
         SET status = 'filled', terminal_game_day = ?, terminal_reason = NULL
         WHERE id = ? AND save_id = ? AND run_revision = ? AND status = 'active'",
    )
    .bind(context.game_day)
    .bind(order_id)
    .bind(context.save_id)
    .bind(context.run_revision)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "property sale order lost its active state before filling"
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn update_save_after_property_sale(
    tx: &mut Transaction<'_, MySql>,
    context: PropertyTaxRunContext,
    current: &LockedPropertySaveRow,
    wallet_proceeds_krw: i64,
    mortgage_principal_krw: i64,
    property_book_value_krw: i64,
) -> Result<()> {
    let cash_after = current
        .cash_krw
        .checked_add(wallet_proceeds_krw)
        .context("property sale wallet overflowed")?;
    let debt_after = current
        .debt_krw
        .checked_sub(mortgage_principal_krw)
        .context("property sale debt underflowed")?;
    let property_after = current
        .property_book_value_krw
        .checked_sub(property_book_value_krw)
        .context("property sale book value underflowed")?;
    ensure!(
        cash_after >= 0 && debt_after >= 0 && property_after >= 0,
        "property sale projected balances are negative"
    );
    let update = sqlx::query(
        "UPDATE save
         SET cash_krw = ?, debt_krw = ?, property_book_value_krw = ?
         WHERE id = ? AND run_revision = ? AND policy_set_id = ? AND game_day = ?
           AND cash_krw = ? AND debt_krw = ? AND property_book_value_krw = ?",
    )
    .bind(cash_after)
    .bind(debt_after)
    .bind(property_after)
    .bind(context.save_id)
    .bind(context.run_revision)
    .bind(context.policy_set_id)
    .bind(current.game_day)
    .bind(current.cash_krw)
    .bind(current.debt_krw)
    .bind(current.property_book_value_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "property sale lost the save balance projection"
    );
    Ok(())
}

fn property_sale_rejection_db(reason: PropertySaleOrderRejectionReasonState) -> &'static str {
    match reason {
        PropertySaleOrderRejectionReasonState::MortgageNotPayable => "mortgageNotPayable",
        PropertySaleOrderRejectionReasonState::InsufficientProceeds => "insufficientProceeds",
        PropertySaleOrderRejectionReasonState::PolicyUnsupported => "policyUnsupported",
    }
}

async fn read_property_sale_scope(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedPropertySaveRow,
) -> Result<Option<PropertySaleScopeRow>> {
    let scope: Option<PropertySaleScopeRow> = sqlx::query_as(
        "SELECT bundle.real_estate_model_version_id,
                model.version_key AS real_estate_version_key,
                model.availability AS real_estate_availability,
                model.sealed_at IS NOT NULL AS real_estate_sealed,
                capital_profile.rule_id AS capital_gains_policy_rule_id,
                policy.policy_key, policy.sealed_at IS NOT NULL AS policy_sealed,
                world.seed AS world_seed, daily.market_date,
                liquidity.minimum_asking_ratio_ppm,
                liquidity.low_band_maximum_ratio_ppm,
                liquidity.middle_band_maximum_ratio_ppm,
                liquidity.maximum_asking_ratio_ppm,
                liquidity.low_delay_minimum_days, liquidity.low_delay_maximum_days,
                liquidity.middle_delay_minimum_days, liquidity.middle_delay_maximum_days,
                liquidity.high_delay_minimum_days, liquidity.high_delay_maximum_days,
                liquidity.candidate_entropy_key, liquidity.gross_price_rule,
                liquidity.disposition_cost_ppm,
                liquidity.minimum_disposition_cost_krw,
                liquidity.minimum_holding_years, liquidity.minimum_residence_years,
                liquidity.deficient_sale_proceeds, liquidity.post_sale_tenure_type,
                liquidity.provenance_kind
         FROM run_rule_bundle AS bundle
         INNER JOIN real_estate_model_version AS model
           ON model.id = bundle.real_estate_model_version_id
         INNER JOIN real_estate_sale_liquidity_profile AS liquidity
           ON liquidity.real_estate_model_version_id = model.id
         INNER JOIN policy_set AS policy ON policy.id = bundle.policy_set_id
         INNER JOIN property_capital_gains_tax_policy_profile AS capital_profile
           ON capital_profile.policy_set_id = policy.id
         INNER JOIN market_world AS world ON world.id = bundle.market_world_id
         INNER JOIN market_daily AS daily
           ON daily.world_id = bundle.market_world_id AND daily.game_day = ?
         WHERE bundle.save_id = ? AND bundle.run_revision = ?
           AND bundle.market_world_id = ? AND bundle.policy_set_id = ?",
    )
    .bind(current.game_day)
    .bind(current.id)
    .bind(current.run_revision)
    .bind(current.market_world_id)
    .bind(current.policy_set_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(scope) = scope else {
        return Ok(None);
    };
    ensure!(
        scope.real_estate_version_key == "dev-unranked-m4-real-estate-sale-tax-2026-v6"
            && scope.real_estate_availability == "active"
            && scope.real_estate_sealed
            && scope.policy_key == "dev-unranked-kr-individual-property-2026-v3"
            && scope.policy_sealed
            && scope.candidate_entropy_key == "propertySaleCandidate"
            && scope.gross_price_rule == "exactAskingPrice"
            && scope.minimum_disposition_cost_krw == 1
            && scope.deficient_sale_proceeds == "reject"
            && scope.post_sale_tenure_type == "rentFree"
            && scope.provenance_kind == "GAME_BALANCE",
        "property sale scope is not the sealed C4 fixture"
    );
    Ok(Some(scope))
}

async fn lock_sale_holding(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedPropertySaveRow,
    scope: &PropertySaleScopeRow,
    household_id: u64,
    holding_id: ResourceId,
) -> Result<Option<LockedSaleHoldingRow>> {
    let rows: Vec<LockedSaleHoldingRow> = sqlx::query_as(
        "SELECT holding.id, holding.household_id, holding.property_listing_id,
                holding.region_key,
                holding.acquisition_price_krw,
                holding.acquisition_incidental_cost_krw, holding.book_value_krw,
                acquired_daily.price_index_ppm AS acquisition_price_index_ppm,
                current_daily.price_index_ppm AS current_price_index_ppm,
                acquired_market.market_date AS acquired_on,
                residence.id AS owner_residence_id,
                residence_market.market_date AS owner_occupied_from
         FROM property_holding AS holding
         INNER JOIN residence
           ON residence.save_id = holding.save_id
          AND residence.run_revision = holding.run_revision
          AND residence.household_id = holding.household_id
          AND residence.property_holding_id = holding.id
          AND residence.tenure_type = 'owner'
          AND residence.effective_from_game_day <= ?
          AND (residence.effective_to_game_day IS NULL
               OR residence.effective_to_game_day > ?)
         INNER JOIN real_estate_daily AS acquired_daily
           ON acquired_daily.market_world_id = ?
          AND acquired_daily.real_estate_model_version_id = holding.real_estate_model_version_id
          AND BINARY acquired_daily.region_key = BINARY holding.region_key
          AND acquired_daily.game_day = holding.acquired_game_day
         INNER JOIN real_estate_daily AS current_daily
           ON current_daily.market_world_id = acquired_daily.market_world_id
          AND current_daily.real_estate_model_version_id = acquired_daily.real_estate_model_version_id
          AND BINARY current_daily.region_key = BINARY acquired_daily.region_key
          AND current_daily.game_day = ?
         INNER JOIN market_daily AS acquired_market
           ON acquired_market.world_id = ? AND acquired_market.game_day = holding.acquired_game_day
         INNER JOIN market_daily AS residence_market
           ON residence_market.world_id = acquired_market.world_id
          AND residence_market.game_day = residence.effective_from_game_day
         WHERE holding.id = ? AND holding.save_id = ? AND holding.run_revision = ?
           AND holding.household_id = ? AND holding.status = 'active'
           AND holding.purpose = 'ownerOccupied'
           AND holding.real_estate_model_version_id = ?
         ORDER BY residence.id FOR UPDATE",
    )
    .bind(current.game_day)
    .bind(current.game_day)
    .bind(current.market_world_id)
    .bind(current.game_day)
    .bind(current.market_world_id)
    .bind(holding_id.get())
    .bind(current.id)
    .bind(current.run_revision)
    .bind(household_id)
    .bind(scope.real_estate_model_version_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() <= 1,
        "property holding has multiple owner residences"
    );
    Ok(rows.into_iter().next())
}

async fn read_sale_order_holding_id(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedPropertySaveRow,
    order_id: ResourceId,
) -> Result<Option<ResourceId>> {
    Ok(sqlx::query_scalar(
        "SELECT property_holding_id FROM property_sale_order
         WHERE id = ? AND save_id = ? AND run_revision = ?",
    )
    .bind(order_id.get())
    .bind(current.id)
    .bind(current.run_revision)
    .fetch_optional(&mut **tx)
    .await?
    .map(ResourceId::from_u64))
}

async fn lock_sale_order(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedPropertySaveRow,
    household_id: u64,
    holding_id: ResourceId,
    order_id: Option<ResourceId>,
) -> Result<Option<LockedSaleOrderRow>> {
    let rows: Vec<LockedSaleOrderRow> = match order_id {
        Some(order_id) => {
            sqlx::query_as(
                "SELECT id, property_holding_id, status, current_revision_no
                 FROM property_sale_order
                 WHERE id = ? AND save_id = ? AND run_revision = ?
                   AND household_id = ? AND property_holding_id = ?
                 FOR UPDATE",
            )
            .bind(order_id.get())
            .bind(current.id)
            .bind(current.run_revision)
            .bind(household_id)
            .bind(holding_id.get())
            .fetch_all(&mut **tx)
            .await?
        }
        None => {
            sqlx::query_as(
                "SELECT id, property_holding_id, status, current_revision_no
                 FROM property_sale_order
                 WHERE save_id = ? AND run_revision = ? AND household_id = ?
                   AND property_holding_id = ? AND status = 'active'
                 ORDER BY id FOR UPDATE",
            )
            .bind(current.id)
            .bind(current.run_revision)
            .bind(household_id)
            .bind(holding_id.get())
            .fetch_all(&mut **tx)
            .await?
        }
    };
    ensure!(
        rows.len() <= 1,
        "property holding has multiple matching sale orders"
    );
    Ok(rows.into_iter().next())
}

fn plan_sale_listing_revision(
    property_rules: &dyn PropertyRules,
    current: &LockedPropertySaveRow,
    scope: &PropertySaleScopeRow,
    holding: &LockedSaleHoldingRow,
    revision_no: u32,
    asking_price_krw: i64,
) -> Result<Result<PlannedSaleListingRevision, LifeFailureCode>> {
    let period = property_rules
        .calculate_sale_period(PropertySalePeriodInput {
            acquired_on: holding.acquired_on,
            owner_occupied_from: holding.owner_occupied_from,
            as_of: scope.market_date,
            minimum_holding_years: scope.minimum_holding_years,
            minimum_residence_years: scope.minimum_residence_years,
        })
        .context("property sale period calculation failed")?;
    if !period.is_eligible {
        return Ok(Err(LifeFailureCode::PolicyUnsupported));
    }
    let reference_value_krw = property_rules
        .calculate_sale_reference_value(PropertySaleReferenceValueInput {
            acquisition_price_krw: holding.acquisition_price_krw,
            acquisition_price_index_ppm: holding.acquisition_price_index_ppm,
            current_price_index_ppm: holding.current_price_index_ppm,
        })
        .context("property sale reference-value calculation failed")?;
    let candidate = match property_rules.plan_sale_candidate(PropertySaleCandidateInput {
        world_seed: scope.world_seed,
        listing_id: ResourceId::from_u64(holding.property_listing_id),
        order_revision: revision_no,
        current_game_day: current.game_day,
        reference_value_krw,
        asking_price_krw,
        liquidity: scope.liquidity(),
    }) {
        Ok(candidate) => candidate,
        Err(PropertyError::AskingPriceOutOfRange | PropertyError::InvalidSaleCandidate) => {
            return Ok(Err(LifeFailureCode::InvalidCommand));
        }
        Err(error) => return Err(error).context("property sale candidate calculation failed"),
    };
    let asking_ratio_ppm = i64::try_from(
        i128::from(asking_price_krw)
            .checked_mul(1_000_000)
            .and_then(|value| value.checked_div(i128::from(reference_value_krw)))
            .context("property sale asking ratio overflowed")?,
    )
    .context("property sale asking ratio is out of range")?;
    Ok(Ok(PlannedSaleListingRevision {
        reference_value_krw,
        asking_ratio_ppm,
        candidate_game_day: candidate.candidate_game_day,
    }))
}

fn plan_property_sale_financials(
    property_rules: &dyn PropertyRules,
    gross_sale_price_krw: i64,
    property_book_value_krw: i64,
    disposition_cost_krw: i64,
    mortgage: PropertySaleMortgagePlan,
    national_capital_gains_tax_krw: i64,
    local_capital_gains_tax_krw: i64,
) -> Result<PropertySaleFinancialPlan> {
    let (mortgage_principal_krw, mortgage_fee_krw, mortgage_not_payable) = match mortgage {
        PropertySaleMortgagePlan::None => (0, 0, false),
        PropertySaleMortgagePlan::Payable {
            principal_krw,
            fee_krw,
        } => (principal_krw, fee_krw, false),
        PropertySaleMortgagePlan::NotPayable {
            principal_krw,
            fee_krw,
        } => (principal_krw, fee_krw, true),
    };
    ensure!(
        mortgage_principal_krw >= 0
            && mortgage_fee_krw >= 0
            && national_capital_gains_tax_krw >= 0
            && local_capital_gains_tax_krw >= 0,
        "property sale financial inputs are negative"
    );
    let transfer_tax_krw = national_capital_gains_tax_krw
        .checked_add(local_capital_gains_tax_krw)
        .context("property sale transfer tax overflowed")?;
    let net_wallet_proceeds_krw = gross_sale_price_krw
        .checked_sub(disposition_cost_krw)
        .and_then(|value| value.checked_sub(mortgage_principal_krw))
        .and_then(|value| value.checked_sub(mortgage_fee_krw))
        .and_then(|value| value.checked_sub(transfer_tax_krw))
        .context("property sale net proceeds overflowed")?;
    if mortgage_not_payable {
        return Ok(PropertySaleFinancialPlan {
            mortgage_principal_krw,
            mortgage_fee_krw,
            transfer_tax_krw,
            net_wallet_proceeds_krw,
            rejection_reason: Some(PropertySaleOrderRejectionReasonState::MortgageNotPayable),
            proceeds: None,
        });
    }
    let proceeds = match property_rules.plan_sale_proceeds(PropertySaleProceedsInput {
        gross_sale_price_krw,
        property_book_value_krw,
        disposition_cost_krw,
        mortgage_principal_payoff_krw: mortgage_principal_krw,
        mortgage_prepayment_fee_krw: mortgage_fee_krw,
        national_capital_gains_tax_krw,
        local_capital_gains_tax_krw,
    }) {
        Ok(proceeds) => proceeds,
        Err(PropertyError::InsufficientSaleProceeds) => {
            return Ok(PropertySaleFinancialPlan {
                mortgage_principal_krw,
                mortgage_fee_krw,
                transfer_tax_krw,
                net_wallet_proceeds_krw,
                rejection_reason: Some(PropertySaleOrderRejectionReasonState::InsufficientProceeds),
                proceeds: None,
            });
        }
        Err(error) => return Err(error).context("property sale proceeds planning failed"),
    };
    ensure!(
        proceeds.wallet_proceeds_krw == net_wallet_proceeds_krw
            && proceeds.total_capital_gains_tax_krw == transfer_tax_krw
            && proceeds
                .postings
                .iter()
                .try_fold(0_i64, |total, posting| total
                    .checked_add(posting.amount_krw))
                == Some(0),
        "property sale financial plan does not reconcile"
    );
    Ok(PropertySaleFinancialPlan {
        mortgage_principal_krw,
        mortgage_fee_krw,
        transfer_tax_krw,
        net_wallet_proceeds_krw,
        rejection_reason: None,
        proceeds: Some(proceeds),
    })
}

async fn create_property_sale_order_once(
    pool: &MySqlPool,
    property_rules: &dyn PropertyRules,
    user_id: u64,
    command: &CreatePropertySaleOrderCommand,
    fingerprint: &str,
) -> Result<LifeStoreResult<PropertySaleOrderListingReceipt>> {
    let mut tx = pool.begin().await?;
    let Some(current) = lock_property_save(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_CREATE_PROPERTY_SALE_ORDER,
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
                read_stored_property_receipt(&mut tx, current.id, command.command_id.as_str())
                    .await?;
            if row.run_revision != current.run_revision {
                tx.commit().await?;
                return Ok(LifeStoreResult::Rejected(
                    LifeFailureCode::IdempotencyConflict,
                ));
            }
            let mut receipt: PropertySaleOrderListingReceipt =
                serde_json::from_str(&row.result_json)
                    .context("stored property-sale creation receipt is invalid")?;
            ensure_sale_listing_replay(
                &row,
                COMMAND_KIND_CREATE_PROPERTY_SALE_ORDER,
                fingerprint,
                command.cursor,
                command.command_id.as_str(),
                command.holding_id,
                None,
                command.asking_price_krw,
                &receipt,
            )?;
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
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::InvalidCommand));
    }
    let Some(scope) = read_property_sale_scope(&mut tx, &current).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::RateUnavailable));
    };
    let household_id = lock_current_household(&mut tx, &current).await?;
    let Some(holding) =
        lock_sale_holding(&mut tx, &current, &scope, household_id, command.holding_id).await?
    else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::HousingResourceNotFound,
        ));
    };
    ensure!(
        holding.id == command.holding_id.get()
            && holding.household_id == household_id
            && holding.owner_residence_id > 0,
        "property sale holding scope is invalid"
    );
    if lock_sale_order(&mut tx, &current, household_id, command.holding_id, None)
        .await?
        .is_some()
    {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
    }
    let revision_no = 1;
    let planned = match plan_sale_listing_revision(
        property_rules,
        &current,
        &scope,
        &holding,
        revision_no,
        command.asking_price_krw,
    )? {
        Ok(planned) => planned,
        Err(code) => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(code));
        }
    };
    write_command_identity(&mut tx, current.id, &identity).await?;
    let order_id = insert_property_sale_order(
        &mut tx,
        &current,
        household_id,
        command.holding_id,
        revision_no,
    )
    .await?;
    insert_property_sale_listing_revision(
        &mut tx,
        &current,
        &scope,
        &holding,
        order_id,
        revision_no,
        command.command_id.as_str(),
        command.asking_price_krw,
        &planned,
    )
    .await?;
    let committed_state_revision = update_save_after_sale_order_command(&mut tx, &current).await?;
    let receipt = PropertySaleOrderListingReceipt {
        command_id: command.command_id.clone(),
        order_id: ResourceId::from_u64(order_id),
        holding_id: command.holding_id,
        revision_no,
        asking_price_krw: command.asking_price_krw,
        reference_value_krw: planned.reference_value_krw,
        asking_to_reference_ppm: planned.asking_ratio_ppm,
        candidate_game_day: planned.candidate_game_day,
        status: PropertySaleOrderStatusState::Active,
        replayed: false,
    };
    write_sale_order_receipt(
        &mut tx,
        &current,
        committed_state_revision,
        &command.command_id,
        COMMAND_KIND_CREATE_PROPERTY_SALE_ORDER,
        fingerprint,
        &receipt,
    )
    .await?;
    let save = read_state(&mut tx, current.id).await?;
    tx.commit().await?;
    Ok(LifeStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

async fn reprice_property_sale_order_once(
    pool: &MySqlPool,
    property_rules: &dyn PropertyRules,
    user_id: u64,
    command: &RepricePropertySaleOrderCommand,
    fingerprint: &str,
) -> Result<LifeStoreResult<PropertySaleOrderListingReceipt>> {
    let mut tx = pool.begin().await?;
    let Some(current) = lock_property_save(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_REPRICE_PROPERTY_SALE_ORDER,
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
                read_stored_property_receipt(&mut tx, current.id, command.command_id.as_str())
                    .await?;
            if row.run_revision != current.run_revision {
                tx.commit().await?;
                return Ok(LifeStoreResult::Rejected(
                    LifeFailureCode::IdempotencyConflict,
                ));
            }
            let mut receipt: PropertySaleOrderListingReceipt =
                serde_json::from_str(&row.result_json)
                    .context("stored property-sale repricing receipt is invalid")?;
            ensure_sale_listing_replay(
                &row,
                COMMAND_KIND_REPRICE_PROPERTY_SALE_ORDER,
                fingerprint,
                command.cursor,
                command.command_id.as_str(),
                receipt.holding_id,
                Some(command.order_id),
                command.asking_price_krw,
                &receipt,
            )?;
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
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::InvalidCommand));
    }
    let Some(scope) = read_property_sale_scope(&mut tx, &current).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::RateUnavailable));
    };
    let household_id = lock_current_household(&mut tx, &current).await?;
    let Some(holding_id) = read_sale_order_holding_id(&mut tx, &current, command.order_id).await?
    else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::HousingResourceNotFound,
        ));
    };
    let Some(holding) =
        lock_sale_holding(&mut tx, &current, &scope, household_id, holding_id).await?
    else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
    };
    let Some(order) = lock_sale_order(
        &mut tx,
        &current,
        household_id,
        holding_id,
        Some(command.order_id),
    )
    .await?
    else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::HousingResourceNotFound,
        ));
    };
    if order.status != "active" {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
    }
    let revision_no = order
        .current_revision_no
        .checked_add(1)
        .context("property sale order revision overflowed")?;
    let planned = match plan_sale_listing_revision(
        property_rules,
        &current,
        &scope,
        &holding,
        revision_no,
        command.asking_price_krw,
    )? {
        Ok(planned) => planned,
        Err(code) => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(code));
        }
    };
    write_command_identity(&mut tx, current.id, &identity).await?;
    insert_property_sale_listing_revision(
        &mut tx,
        &current,
        &scope,
        &holding,
        order.id,
        revision_no,
        command.command_id.as_str(),
        command.asking_price_krw,
        &planned,
    )
    .await?;
    advance_property_sale_order_revision(&mut tx, &current, &order, revision_no).await?;
    let committed_state_revision = update_save_after_sale_order_command(&mut tx, &current).await?;
    let receipt = PropertySaleOrderListingReceipt {
        command_id: command.command_id.clone(),
        order_id: command.order_id,
        holding_id,
        revision_no,
        asking_price_krw: command.asking_price_krw,
        reference_value_krw: planned.reference_value_krw,
        asking_to_reference_ppm: planned.asking_ratio_ppm,
        candidate_game_day: planned.candidate_game_day,
        status: PropertySaleOrderStatusState::Active,
        replayed: false,
    };
    write_sale_order_receipt(
        &mut tx,
        &current,
        committed_state_revision,
        &command.command_id,
        COMMAND_KIND_REPRICE_PROPERTY_SALE_ORDER,
        fingerprint,
        &receipt,
    )
    .await?;
    let save = read_state(&mut tx, current.id).await?;
    tx.commit().await?;
    Ok(LifeStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

async fn cancel_property_sale_order_once(
    pool: &MySqlPool,
    user_id: u64,
    command: &CancelPropertySaleOrderCommand,
    fingerprint: &str,
) -> Result<LifeStoreResult<PropertySaleOrderCancellationReceipt>> {
    let mut tx = pool.begin().await?;
    let Some(current) = lock_property_save(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_CANCEL_PROPERTY_SALE_ORDER,
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
                read_stored_property_receipt(&mut tx, current.id, command.command_id.as_str())
                    .await?;
            if row.run_revision != current.run_revision {
                tx.commit().await?;
                return Ok(LifeStoreResult::Rejected(
                    LifeFailureCode::IdempotencyConflict,
                ));
            }
            let mut receipt: PropertySaleOrderCancellationReceipt =
                serde_json::from_str(&row.result_json)
                    .context("stored property-sale cancellation receipt is invalid")?;
            let expected_state_revision = command
                .cursor
                .expected_state_revision
                .checked_add(1)
                .context("stored property-sale cancellation state revision overflowed")?;
            ensure!(
                row.command_kind == COMMAND_KIND_CANCEL_PROPERTY_SALE_ORDER
                    && row.payload_sha256 == fingerprint
                    && row.run_revision == command.cursor.expected_run_revision
                    && row.state_revision == expected_state_revision
                    && row.game_day == command.cursor.expected_game_day
                    && row.ledger_transaction_id.is_none()
                    && receipt.command_id == command.command_id
                    && receipt.order_id == command.order_id
                    && receipt.status == PropertySaleOrderStatusState::Cancelled
                    && receipt.cancelled_game_day == command.cursor.expected_game_day
                    && !receipt.replayed,
                "stored property-sale cancellation receipt disagrees with its command"
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
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::InvalidCommand));
    }
    let Some(scope) = read_property_sale_scope(&mut tx, &current).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::RateUnavailable));
    };
    let household_id = lock_current_household(&mut tx, &current).await?;
    let Some(holding_id) = read_sale_order_holding_id(&mut tx, &current, command.order_id).await?
    else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::HousingResourceNotFound,
        ));
    };
    if lock_sale_holding(&mut tx, &current, &scope, household_id, holding_id)
        .await?
        .is_none()
    {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
    }
    let Some(order) = lock_sale_order(
        &mut tx,
        &current,
        household_id,
        holding_id,
        Some(command.order_id),
    )
    .await?
    else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::HousingResourceNotFound,
        ));
    };
    if order.status != "active" {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
    }
    let revision_no = order
        .current_revision_no
        .checked_add(1)
        .context("property sale cancellation revision overflowed")?;
    write_command_identity(&mut tx, current.id, &identity).await?;
    insert_property_sale_cancellation_revision(
        &mut tx,
        &current,
        &scope,
        PropertySaleCancellationRevision {
            order_id: order.id,
            holding_id,
            revision_no,
            command_id: Some(command.command_id.as_str()),
            cancellation_reason: "userRequest",
        },
    )
    .await?;
    cancel_property_sale_order_header(
        &mut tx,
        &current,
        &order,
        revision_no,
        current.game_day,
        "userRequest",
    )
    .await?;
    let committed_state_revision = update_save_after_sale_order_command(&mut tx, &current).await?;
    let receipt = PropertySaleOrderCancellationReceipt {
        command_id: command.command_id.clone(),
        order_id: command.order_id,
        holding_id,
        revision_no,
        cancelled_game_day: current.game_day,
        status: PropertySaleOrderStatusState::Cancelled,
        replayed: false,
    };
    write_sale_order_receipt(
        &mut tx,
        &current,
        committed_state_revision,
        &command.command_id,
        COMMAND_KIND_CANCEL_PROPERTY_SALE_ORDER,
        fingerprint,
        &receipt,
    )
    .await?;
    let save = read_state(&mut tx, current.id).await?;
    tx.commit().await?;
    Ok(LifeStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

#[allow(clippy::too_many_arguments)]
fn ensure_sale_listing_replay(
    row: &StoredCommandReceiptRow,
    command_kind: &str,
    fingerprint: &str,
    cursor: CommandCursor,
    command_id: &str,
    holding_id: ResourceId,
    order_id: Option<ResourceId>,
    asking_price_krw: i64,
    receipt: &PropertySaleOrderListingReceipt,
) -> Result<()> {
    let expected_state_revision = cursor
        .expected_state_revision
        .checked_add(1)
        .context("stored property-sale listing state revision overflowed")?;
    ensure!(
        row.command_kind == command_kind
            && row.payload_sha256 == fingerprint
            && row.run_revision == cursor.expected_run_revision
            && row.state_revision == expected_state_revision
            && row.game_day == cursor.expected_game_day
            && row.ledger_transaction_id.is_none()
            && receipt.command_id.as_str() == command_id
            && receipt.holding_id == holding_id
            && order_id.is_none_or(|id| receipt.order_id == id)
            && receipt.asking_price_krw == asking_price_krw
            && receipt.revision_no > 0
            && receipt.reference_value_krw > 0
            && receipt.asking_to_reference_ppm > 0
            && receipt.candidate_game_day > cursor.expected_game_day
            && receipt.status == PropertySaleOrderStatusState::Active
            && !receipt.replayed,
        "stored property-sale listing receipt disagrees with its command"
    );
    Ok(())
}

async fn insert_property_sale_order(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedPropertySaveRow,
    household_id: u64,
    holding_id: ResourceId,
    revision_no: u32,
) -> Result<u64> {
    let inserted = sqlx::query(
        "INSERT INTO property_sale_order
             (save_id, run_revision, household_id, property_holding_id,
              status, current_revision_no, created_game_day,
              terminal_game_day, terminal_reason)
         VALUES (?, ?, ?, ?, 'active', ?, ?, NULL, NULL)",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(household_id)
    .bind(holding_id.get())
    .bind(revision_no)
    .bind(current.game_day)
    .execute(&mut **tx)
    .await?;
    let order_id = inserted.last_insert_id();
    ensure!(order_id > 0, "property sale order has no durable identity");
    Ok(order_id)
}

#[allow(clippy::too_many_arguments)]
async fn insert_property_sale_listing_revision(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedPropertySaveRow,
    scope: &PropertySaleScopeRow,
    holding: &LockedSaleHoldingRow,
    order_id: u64,
    revision_no: u32,
    command_id: &str,
    asking_price_krw: i64,
    planned: &PlannedSaleListingRevision,
) -> Result<u64> {
    let inserted = sqlx::query(
        "INSERT INTO property_sale_order_revision
             (save_id, run_revision, property_sale_order_id, property_holding_id,
              revision_no, revision_kind, command_id, cancellation_reason,
              created_game_day, real_estate_model_version_id, policy_set_id,
              capital_gains_policy_rule_id,
              asking_price_krw, reference_value_krw,
              acquisition_price_index_ppm, current_price_index_ppm,
              asking_ratio_ppm, candidate_game_day, gross_price_rule,
              disposition_cost_ppm, minimum_disposition_cost_krw,
              deficient_sale_proceeds, minimum_holding_years,
              minimum_residence_years)
         VALUES (?, ?, ?, ?, ?, 'listing', ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(order_id)
    .bind(holding.id)
    .bind(revision_no)
    .bind(command_id)
    .bind(current.game_day)
    .bind(scope.real_estate_model_version_id)
    .bind(current.policy_set_id)
    .bind(scope.capital_gains_policy_rule_id)
    .bind(asking_price_krw)
    .bind(planned.reference_value_krw)
    .bind(holding.acquisition_price_index_ppm)
    .bind(holding.current_price_index_ppm)
    .bind(planned.asking_ratio_ppm)
    .bind(planned.candidate_game_day)
    .bind(&scope.gross_price_rule)
    .bind(scope.disposition_cost_ppm)
    .bind(scope.minimum_disposition_cost_krw)
    .bind(&scope.deficient_sale_proceeds)
    .bind(scope.minimum_holding_years)
    .bind(scope.minimum_residence_years)
    .execute(&mut **tx)
    .await?;
    let revision_id = inserted.last_insert_id();
    ensure!(
        revision_id > 0,
        "property sale revision has no durable identity"
    );
    Ok(revision_id)
}

struct PropertySaleCancellationRevision<'a> {
    order_id: u64,
    holding_id: ResourceId,
    revision_no: u32,
    command_id: Option<&'a str>,
    cancellation_reason: &'a str,
}

async fn insert_property_sale_cancellation_revision(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedPropertySaveRow,
    scope: &PropertySaleScopeRow,
    revision: PropertySaleCancellationRevision<'_>,
) -> Result<u64> {
    let inserted = sqlx::query(
        "INSERT INTO property_sale_order_revision
             (save_id, run_revision, property_sale_order_id, property_holding_id,
              revision_no, revision_kind, command_id, cancellation_reason,
              created_game_day, real_estate_model_version_id, policy_set_id,
              capital_gains_policy_rule_id,
              asking_price_krw, reference_value_krw,
              acquisition_price_index_ppm, current_price_index_ppm,
              asking_ratio_ppm, candidate_game_day, gross_price_rule,
              disposition_cost_ppm, minimum_disposition_cost_krw,
              deficient_sale_proceeds, minimum_holding_years,
              minimum_residence_years)
         VALUES (?, ?, ?, ?, ?, 'cancellation', ?, ?, ?, ?, ?, ?,
                 NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
                 NULL, NULL)",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(revision.order_id)
    .bind(revision.holding_id.get())
    .bind(revision.revision_no)
    .bind(revision.command_id)
    .bind(revision.cancellation_reason)
    .bind(current.game_day)
    .bind(scope.real_estate_model_version_id)
    .bind(current.policy_set_id)
    .bind(scope.capital_gains_policy_rule_id)
    .execute(&mut **tx)
    .await?;
    let revision_id = inserted.last_insert_id();
    ensure!(
        revision_id > 0,
        "property sale cancellation revision has no identity"
    );
    Ok(revision_id)
}

async fn advance_property_sale_order_revision(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedPropertySaveRow,
    order: &LockedSaleOrderRow,
    revision_no: u32,
) -> Result<()> {
    let updated = sqlx::query(
        "UPDATE property_sale_order SET current_revision_no = ?
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND property_holding_id = ? AND status = 'active'
           AND current_revision_no = ?",
    )
    .bind(revision_no)
    .bind(order.id)
    .bind(current.id)
    .bind(current.run_revision)
    .bind(order.property_holding_id)
    .bind(order.current_revision_no)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "property sale order revision changed concurrently"
    );
    Ok(())
}

async fn cancel_property_sale_order_header(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedPropertySaveRow,
    order: &LockedSaleOrderRow,
    revision_no: u32,
    cancelled_game_day: u32,
    cancellation_reason: &str,
) -> Result<()> {
    let updated = sqlx::query(
        "UPDATE property_sale_order
         SET status = 'cancelled', current_revision_no = ?,
             terminal_game_day = ?, terminal_reason = ?
         WHERE id = ? AND save_id = ? AND run_revision = ?
           AND property_holding_id = ? AND status = 'active'
           AND current_revision_no = ?",
    )
    .bind(revision_no)
    .bind(cancelled_game_day)
    .bind(cancellation_reason)
    .bind(order.id)
    .bind(current.id)
    .bind(current.run_revision)
    .bind(order.property_holding_id)
    .bind(order.current_revision_no)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "property sale order changed before cancellation"
    );
    Ok(())
}

async fn update_save_after_sale_order_command(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedPropertySaveRow,
) -> Result<u64> {
    let committed_state_revision = current
        .state_revision
        .checked_add(1)
        .context("property sale command state revision overflowed")?;
    let updated = sqlx::query(
        "UPDATE save SET state_revision = ?
         WHERE id = ? AND market_world_id = ? AND policy_set_id = ?
           AND run_revision = ? AND state_revision = ? AND game_day = ?
           AND cash_krw = ? AND debt_krw = ? AND property_book_value_krw = ?",
    )
    .bind(committed_state_revision)
    .bind(current.id)
    .bind(current.market_world_id)
    .bind(current.policy_set_id)
    .bind(current.run_revision)
    .bind(current.state_revision)
    .bind(current.game_day)
    .bind(current.cash_krw)
    .bind(current.debt_krw)
    .bind(current.property_book_value_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "property sale command cursor changed"
    );
    Ok(committed_state_revision)
}

#[allow(clippy::too_many_arguments)]
async fn write_sale_order_receipt<T: serde::Serialize>(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedPropertySaveRow,
    committed_state_revision: u64,
    command_id: &crate::finance::CommandId,
    command_kind: &'static str,
    fingerprint: &str,
    receipt: &T,
) -> Result<()> {
    write_game_command_receipt(
        tx,
        GameCommandReceiptWrite {
            save_id: current.id,
            command_id,
            command_kind,
            payload_sha256: fingerprint,
            market_world_id: current.market_world_id,
            committed_cursor: GameCommandCursor {
                run_revision: current.run_revision,
                state_revision: committed_state_revision,
                game_day: current.game_day,
            },
            result: receipt,
            ledger_transaction_id: None,
        },
    )
    .await
}

async fn lock_property_save(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
) -> Result<Option<LockedPropertySaveRow>> {
    sqlx::query_as(
        "SELECT save.id, save.market_world_id, save.policy_set_id,
                save.run_revision, save.state_revision, save.game_day,
                save.cash_krw, save.debt_krw, save.property_book_value_krw,
                EXISTS(SELECT 1 FROM `character` WHERE `character`.save_id = save.id)
                    AS has_character
         FROM save WHERE save.user_id = ? FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock the property save")
}

async fn lock_current_household(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedPropertySaveRow,
) -> Result<u64> {
    let rows: Vec<(u64,)> = sqlx::query_as(
        "SELECT id FROM household
         WHERE save_id = ? AND run_revision = ? ORDER BY id FOR UPDATE",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() == 1,
        "property command requires exactly one current household"
    );
    Ok(rows[0].0)
}

async fn read_property_run_scope(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedPropertySaveRow,
) -> Result<Option<PropertyRunScopeRow>> {
    let scope: Option<PropertyRunScopeRow> = sqlx::query_as(
        "SELECT bundle.real_estate_model_version_id,
                real_estate.version_key AS real_estate_version_key,
                real_estate.availability AS real_estate_availability,
                real_estate.sealed_at IS NOT NULL AS real_estate_sealed,
                bundle.credit_model_version_id,
                credit.version_key AS credit_version_key,
                credit.sealed_at IS NOT NULL AS credit_sealed,
                credit.credit_policy_set_id,
                market_daily.market_date,
                purchase.purchase_capability, purchase.maximum_active_holdings,
                purchase.supported_offer_kind, purchase.supported_purpose,
                purchase.incidental_cost_ppm, purchase.minimum_incidental_cost_krw,
                purchase.collateral_value_rule, purchase.ltv_cost_treatment,
                purchase.listing_consumption_scope, purchase.provenance_kind,
                mortgage.regulated_capital_ltv_limit_ppm,
                mortgage.non_regulated_ltv_limit_ppm,
                mortgage.lower_price_threshold_krw,
                mortgage.upper_price_threshold_krw,
                mortgage.lower_band_cap_krw, mortgage.middle_band_cap_krw,
                mortgage.upper_band_cap_krw,
                mortgage.full_term_fixed_stress_rate_bp
         FROM run_rule_bundle AS bundle
         INNER JOIN real_estate_model_version AS real_estate
           ON real_estate.id = bundle.real_estate_model_version_id
         INNER JOIN credit_model_version AS credit
           ON credit.id = bundle.credit_model_version_id
         INNER JOIN market_daily
           ON market_daily.world_id = bundle.market_world_id
          AND market_daily.game_day = ?
         INNER JOIN real_estate_purchase_profile AS purchase
           ON purchase.real_estate_model_version_id = real_estate.id
         INNER JOIN credit_mortgage_policy_profile AS mortgage
           ON mortgage.policy_set_id = credit.credit_policy_set_id
         WHERE bundle.save_id = ? AND bundle.run_revision = ?
           AND bundle.market_world_id = ? AND bundle.policy_set_id = ?",
    )
    .bind(current.game_day)
    .bind(current.id)
    .bind(current.run_revision)
    .bind(current.market_world_id)
    .bind(current.policy_set_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(scope) = scope else {
        return Ok(None);
    };
    ensure!(
        matches!(
            scope.real_estate_version_key.as_str(),
            "dev-unranked-m4-real-estate-purchase-2026-v5"
                | "dev-unranked-m4-real-estate-sale-tax-2026-v6"
        ) && scope.real_estate_availability == "active"
            && scope.real_estate_sealed
            && scope.credit_version_key == "dev-unranked-m4c3-credit-2026-v4"
            && scope.credit_sealed
            && scope.purchase_capability == "ownerOccupiedSingleHome"
            && scope.maximum_active_holdings == 1
            && scope.supported_offer_kind == "sale"
            && scope.supported_purpose == "ownerOccupied"
            && scope.incidental_cost_ppm > 0
            && scope.minimum_incidental_cost_krw == 1
            && scope.collateral_value_rule == "exactSalePriceAtExecution"
            && scope.ltv_cost_treatment == "excludeIncidentalAndMoving"
            && scope.listing_consumption_scope == "householdRunOnce"
            && scope.provenance_kind == "GAME_BALANCE"
            && scope.full_term_fixed_stress_rate_bp == 0,
        "property run capability is not the sealed C3 fixture"
    );
    Ok(Some(scope))
}

async fn lock_current_residence(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedPropertySaveRow,
    household_id: u64,
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
    .bind(current.id)
    .bind(current.run_revision)
    .bind(household_id)
    .bind(current.game_day)
    .bind(current.game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() == 1,
        "property command requires exactly one current residence"
    );
    rows.pop().context("current property residence disappeared")
}

async fn lock_active_holding_count(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedPropertySaveRow,
    household_id: u64,
) -> Result<usize> {
    let rows: Vec<(u64,)> = sqlx::query_as(
        "SELECT id FROM property_holding
         WHERE save_id = ? AND run_revision = ? AND household_id = ? AND status = 'active'
         ORDER BY id LIMIT 2 FOR UPDATE",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(household_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() <= 1,
        "household has multiple active property holdings"
    );
    Ok(rows.len())
}

async fn lock_purchase_listing(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedPropertySaveRow,
    scope: &PropertyRunScopeRow,
    listing_id: ResourceId,
) -> Result<Option<PurchaseListingRow>> {
    sqlx::query_as(
        "SELECT listing.id, listing.region_key, listing.property_type,
                listing.exclusive_area_square_meters,
                listing.available_from_game_day, listing.available_to_game_day,
                offer.price_krw, daily.price_index_ppm AS current_price_index_ppm,
                mapping.ltv_region_class, moving.moving_cost_krw
         FROM property_listing AS listing
         INNER JOIN property_listing_offer AS offer
           ON offer.property_listing_id = listing.id AND offer.offer_kind = 'sale'
         INNER JOIN real_estate_purchase_region_mapping AS mapping
           ON mapping.real_estate_model_version_id = listing.real_estate_model_version_id
          AND BINARY mapping.region_key = BINARY listing.region_key
         INNER JOIN real_estate_region_moving_cost AS moving
           ON moving.real_estate_model_version_id = listing.real_estate_model_version_id
          AND BINARY moving.region_key = BINARY listing.region_key
         INNER JOIN real_estate_daily AS daily
           ON daily.market_world_id = listing.market_world_id
          AND daily.real_estate_model_version_id = listing.real_estate_model_version_id
          AND BINARY daily.region_key = BINARY listing.region_key
          AND daily.game_day = ?
         WHERE listing.id = ? AND listing.market_world_id = ?
           AND listing.real_estate_model_version_id = ?
           AND listing.year_month = DATE_FORMAT(?, '%Y-%m-01')
           AND listing.available_from_game_day <= ?
           AND listing.available_to_game_day >= ?
         FOR UPDATE",
    )
    .bind(current.game_day)
    .bind(listing_id.get())
    .bind(current.market_world_id)
    .bind(scope.real_estate_model_version_id)
    .bind(scope.market_date)
    .bind(current.game_day)
    .bind(current.game_day)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock the property listing")
}

async fn lock_linked_lease_loan_evidence(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedPropertySaveRow,
    household_id: u64,
    lease_id: Option<u64>,
) -> Result<Option<LinkedLeaseLoanEvidenceRow>> {
    let Some(lease_id) = lease_id else {
        return Ok(None);
    };
    let mut rows: Vec<LinkedLeaseLoanEvidenceRow> = sqlx::query_as(
        "SELECT id, remaining_principal_krw
         FROM loan_contract
         WHERE save_id = ? AND run_revision = ? AND household_id = ?
           AND lease_contract_id = ? AND product_kind = 'leaseDepositLoan'
           AND status IN ('pending', 'active', 'delinquent', 'defaulted', 'restructured')
         ORDER BY id FOR UPDATE",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(household_id)
    .bind(lease_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(rows.len() <= 1, "lease has multiple open linked loans");
    Ok(rows.pop())
}

async fn lock_purchase_context(
    tx: &mut Transaction<'_, MySql>,
    current: LockedPropertySaveRow,
    listing_id: ResourceId,
) -> Result<Option<LockedPurchaseContext>> {
    let household_id = lock_current_household(tx, &current).await?;
    let Some(scope) = read_property_run_scope(tx, &current).await? else {
        return Ok(None);
    };
    let residence = lock_current_residence(tx, &current, household_id).await?;
    ensure!(
        (residence.tenure_type == "owner")
            == (residence.property_holding_id.is_some() && residence.lease_contract_id.is_none()),
        "current owner residence has an invalid holding reference"
    );
    let lease_current = current.lease_view();
    let existing_lease = if residence.lease_contract_id.is_some() {
        ensure!(
            residence.property_holding_id.is_none(),
            "tenant residence also references a property holding"
        );
        lock_existing_lease(
            tx,
            &lease_current,
            household_id,
            &residence,
            scope.real_estate_model_version_id,
        )
        .await?
    } else {
        None
    };
    let active_holding_count = lock_active_holding_count(tx, &current, household_id).await?;
    let Some(listing) = lock_purchase_listing(tx, &current, &scope, listing_id).await? else {
        return Ok(None);
    };
    ensure!(
        listing.id == listing_id.get()
            && (listing.available_from_game_day..=listing.available_to_game_day)
                .contains(&current.game_day)
            && listing.price_krw > 0
            && listing.moving_cost_krw > 0,
        "property listing projection is invalid"
    );
    let linked_loan_evidence = lock_linked_lease_loan_evidence(
        tx,
        &current,
        household_id,
        existing_lease.as_ref().map(|lease| lease.id),
    )
    .await?;
    let payoff = prepare_lease_move_payoff_in_tx(
        tx,
        current.id,
        current.run_revision,
        household_id,
        existing_lease.as_ref().map(|lease| lease.id),
        existing_lease.as_ref().map_or(0, |lease| lease.deposit_krw),
    )
    .await?;
    let (prepared_payoff, lease_exit_restricted) = match payoff {
        LeaseMovePayoffPreparation::None => (None, false),
        LeaseMovePayoffPreparation::Prepared(prepared) => (Some(prepared), false),
        LeaseMovePayoffPreparation::Rejected(_) => (None, true),
    };
    ensure!(
        lease_exit_restricted
            || linked_loan_evidence.as_ref().map(|loan| loan.id)
                == prepared_payoff
                    .as_ref()
                    .map(|payoff| payoff.loan_id().get()),
        "lease payoff preparation disagrees with linked-loan evidence"
    );
    Ok(Some(LockedPurchaseContext {
        current,
        household_id,
        scope,
        residence,
        active_holding_count,
        existing_lease,
        linked_loan_evidence,
        prepared_payoff,
        lease_exit_restricted,
        listing,
    }))
}

fn property_purchase_amounts(
    property_rules: &dyn PropertyRules,
    context: &LockedPurchaseContext,
    mortgage_principal_krw: i64,
) -> Result<(i64, i64, i64)> {
    let incidental_cost_krw = property_rules
        .calculate_acquisition_incidental_cost(AcquisitionIncidentalCostInput {
            purchase_price_krw: context.listing.price_krw,
            cost_ppm: i64::from(context.scope.incidental_cost_ppm),
        })
        .context("property incidental-cost calculation failed")?;
    ensure!(
        incidental_cost_krw >= context.scope.minimum_incidental_cost_krw,
        "property incidental cost is below the sealed minimum"
    );
    let returned_deposit_krw = context
        .existing_lease
        .as_ref()
        .map_or(0, |lease| lease.deposit_krw);
    let repaid_loan_principal_krw = context
        .linked_loan_evidence
        .as_ref()
        .map_or(0, |loan| loan.remaining_principal_krw);
    let available_buyer_cash_krw = context
        .current
        .cash_krw
        .checked_add(returned_deposit_krw)
        .and_then(|amount| amount.checked_sub(repaid_loan_principal_krw))
        .context("available property buyer cash overflowed")?;
    let required_buyer_cash_krw = calculate_required_buyer_cash(
        context.listing.price_krw,
        incidental_cost_krw,
        context.listing.moving_cost_krw,
        mortgage_principal_krw,
    )?;
    ensure!(
        available_buyer_cash_krw >= 0,
        "available property buyer cash is negative"
    );
    Ok((
        incidental_cost_krw,
        available_buyer_cash_krw,
        required_buyer_cash_krw,
    ))
}

fn calculate_required_buyer_cash(
    purchase_price_krw: i64,
    incidental_cost_krw: i64,
    moving_cost_krw: i64,
    mortgage_principal_krw: i64,
) -> Result<i64> {
    ensure!(
        purchase_price_krw > 0
            && incidental_cost_krw >= 0
            && moving_cost_krw >= 0
            && mortgage_principal_krw >= 0,
        "property buyer cash inputs are invalid"
    );
    let total_cost_krw = purchase_price_krw
        .checked_add(incidental_cost_krw)
        .and_then(|amount| amount.checked_add(moving_cost_krw))
        .context("total property purchase cost overflowed")?;
    if mortgage_principal_krw >= total_cost_krw {
        return Ok(0);
    }
    total_cost_krw
        .checked_sub(mortgage_principal_krw)
        .context("required property buyer cash overflowed")
}

fn active_holding_limit_reached(active_holding_count: usize, maximum_active_holdings: u8) -> bool {
    active_holding_count >= usize::from(maximum_active_holdings)
}

async fn assess_mortgage_purchase(
    tx: &mut Transaction<'_, MySql>,
    property_rules: &dyn PropertyRules,
    user_id: u64,
    context: LockedPurchaseContext,
    product_version_id: ResourceId,
    principal_krw: i64,
) -> Result<Result<AssessedMortgagePurchase, LifeFailureCode>> {
    let replaced_loan_id = context
        .linked_loan_evidence
        .as_ref()
        .map(|loan| ResourceId::from_u64(loan.id));
    let replaced_loan_principal_krw = context
        .linked_loan_evidence
        .as_ref()
        .map_or(0, |loan| loan.remaining_principal_krw);
    let mortgage = match assess_mortgage_loan_in_tx(
        tx,
        user_id,
        context.current.id,
        context.current.run_revision,
        product_version_id,
        principal_krw,
        replaced_loan_id,
        replaced_loan_principal_krw,
    )
    .await?
    {
        MortgageLoanAssessmentResult::Assessed(assessment) => assessment,
        MortgageLoanAssessmentResult::Rejected(code) => return Ok(Err(code)),
    };
    let ltv_region_class = match context.listing.ltv_region_class.as_str() {
        "regulatedCapitalProxy" => MortgageLtvRegionClassState::RegulatedCapitalProxy,
        "nonRegulatedProxy" => MortgageLtvRegionClassState::NonRegulatedProxy,
        _ => bail!("property listing has an unsupported LTV region class"),
    };
    let ltv_limit_ppm = match ltv_region_class {
        MortgageLtvRegionClassState::RegulatedCapitalProxy => {
            i64::from(context.scope.regulated_capital_ltv_limit_ppm)
        }
        MortgageLtvRegionClassState::NonRegulatedProxy => {
            i64::from(context.scope.non_regulated_ltv_limit_ppm)
        }
    };
    let regional_price_cap_krw = property_rules
        .select_mortgage_regional_price_cap(MortgageRegionalPriceCapInput {
            recognized_collateral_value_krw: context.listing.price_krw,
            policy: (ltv_region_class == MortgageLtvRegionClassState::RegulatedCapitalProxy)
                .then_some(MortgageRegionalPriceCapPolicy {
                    lower_price_threshold_krw: context.scope.lower_price_threshold_krw,
                    upper_price_threshold_krw: context.scope.upper_price_threshold_krw,
                    lower_band_cap_krw: context.scope.lower_band_cap_krw,
                    middle_band_cap_krw: context.scope.middle_band_cap_krw,
                    upper_band_cap_krw: context.scope.upper_band_cap_krw,
                }),
        })
        .context("mortgage regional price-cap selection failed")?;
    let funding = property_rules
        .calculate_mortgage_funding_limit(MortgageFundingLimitInput {
            recognized_collateral_value_krw: context.listing.price_krw,
            ltv_limit_ppm,
            regional_price_cap_krw,
            product_maximum_principal_krw: mortgage.product.maximum_principal_krw,
        })
        .context("mortgage funding-limit calculation failed")?;
    let ltv_assessment = crate::life::create_loan_rules()
        .assess_ltv(LtvAssessmentInput {
            existing_senior_balance_krw: 0,
            new_principal_krw: principal_krw,
            included_fees_krw: 0,
            recognized_collateral_value_krw: Some(context.listing.price_krw),
            maximum_ratio_ppm: ltv_limit_ppm,
        })
        .context("mortgage LTV assessment failed")?;
    let ltv = LoanQuoteLtvState {
        numerator_krw: ltv_assessment.numerator_krw,
        denominator_krw: ltv_assessment.denominator_krw,
        ratio_ppm: ltv_assessment.ratio_ppm,
        limit_ppm: ltv_assessment.maximum_ratio_ppm,
    };
    let (incidental_cost_krw, available_buyer_cash_krw, required_buyer_cash_krw) =
        property_purchase_amounts(property_rules, &context, principal_krw)?;
    let mut purchase_reasons = Vec::new();
    ensure!(
        mortgage.maximum_active_holdings == 0,
        "mortgage eligibility must require no active holding"
    );
    if active_holding_limit_reached(
        context.active_holding_count,
        context.scope.maximum_active_holdings,
    ) {
        purchase_reasons.push(MortgageQuoteReasonState::ActiveHolding);
    }
    if context.residence.effective_from_game_day == context.current.game_day {
        purchase_reasons.push(MortgageQuoteReasonState::ResidenceChangedToday);
    }
    if context.lease_exit_restricted {
        purchase_reasons.push(MortgageQuoteReasonState::LeaseExitRestricted);
    }
    let (decision_code, decision_reasons) = if !mortgage.credit_reasons.is_empty() {
        (
            MortgageQuoteDecisionState::CreditRestricted,
            mortgage.credit_reasons.clone(),
        )
    } else if !purchase_reasons.is_empty() {
        (
            MortgageQuoteDecisionState::PurchaseRestricted,
            purchase_reasons,
        )
    } else if principal_krw > funding.maximum_mortgage_krw || !ltv_assessment.passed {
        (
            MortgageQuoteDecisionState::CollateralLimit,
            vec![MortgageQuoteReasonState::CollateralLimit],
        )
    } else if mortgage.dsr_applied && mortgage.verified_annual_income_krw.is_none() {
        (
            MortgageQuoteDecisionState::IncomeUnavailable,
            vec![MortgageQuoteReasonState::IncomeUnavailable],
        )
    } else if mortgage
        .dsr
        .as_ref()
        .is_some_and(|dsr| dsr.ratio_ppm > dsr.limit_ppm)
    {
        (
            MortgageQuoteDecisionState::DebtServiceLimit,
            vec![MortgageQuoteReasonState::DebtServiceLimit],
        )
    } else if available_buyer_cash_krw < required_buyer_cash_krw {
        (
            MortgageQuoteDecisionState::InsufficientOwnFunds,
            vec![MortgageQuoteReasonState::InsufficientOwnFunds],
        )
    } else {
        (
            MortgageQuoteDecisionState::Eligible,
            vec![MortgageQuoteReasonState::Eligible],
        )
    };
    ensure!(
        mortgage.stress_rate_bp == i64::from(context.scope.full_term_fixed_stress_rate_bp),
        "mortgage quote stress evidence disagrees with the policy"
    );
    Ok(Ok(AssessedMortgagePurchase {
        context,
        mortgage,
        incidental_cost_krw,
        ltv_region_class,
        ltv_limit_ppm,
        maximum_mortgage_krw: funding.maximum_mortgage_krw,
        ltv,
        available_buyer_cash_krw,
        required_buyer_cash_krw,
        decision_code,
        decision_reasons,
    }))
}

fn build_mortgage_quote_receipt(
    command_id: crate::finance::CommandId,
    quote_id: ResourceId,
    principal_krw: i64,
    assessed: &AssessedMortgagePurchase,
) -> MortgageQuoteReceipt {
    MortgageQuoteReceipt {
        command_id,
        quote_id,
        listing_id: ResourceId::from_u64(assessed.context.listing.id),
        product_version_id: assessed.mortgage.product.id,
        requested_principal_krw: principal_krw,
        purchase_price_krw: assessed.context.listing.price_krw,
        recognized_collateral_value_krw: assessed.context.listing.price_krw,
        ltv_region_class: assessed.ltv_region_class,
        ltv_limit_ppm: assessed.ltv_limit_ppm,
        maximum_mortgage_krw: assessed.maximum_mortgage_krw,
        ltv: assessed.ltv,
        created_game_day: assessed.context.current.game_day,
        expires_game_day: assessed.context.current.game_day,
        decision_code: assessed.decision_code,
        decision_reasons: assessed.decision_reasons.clone(),
        verified_annual_income_krw: assessed.mortgage.verified_annual_income_krw,
        verified_income_source: assessed.mortgage.verified_income_source,
        existing_loan_balance_krw: assessed.mortgage.existing_loan_balance_krw,
        post_execution_balance_krw: assessed.mortgage.post_execution_balance_krw,
        dsr_applied: assessed.mortgage.dsr_applied,
        dsr: assessed.mortgage.dsr,
        stress_rate_bp: assessed.mortgage.stress_rate_bp,
        stress_treatment: MortgageStressTreatmentState::FullTermFixed,
        acquisition_incidental_cost_krw: assessed.incidental_cost_krw,
        moving_cost_krw: assessed.context.listing.moving_cost_krw,
        returned_deposit_krw: assessed
            .context
            .existing_lease
            .as_ref()
            .map_or(0, |lease| lease.deposit_krw),
        replaced_loan_id: assessed
            .context
            .linked_loan_evidence
            .as_ref()
            .map(|loan| ResourceId::from_u64(loan.id)),
        replaced_loan_principal_krw: assessed
            .context
            .linked_loan_evidence
            .as_ref()
            .map_or(0, |loan| loan.remaining_principal_krw),
        available_buyer_cash_krw: assessed.available_buyer_cash_krw,
        required_buyer_cash_krw: assessed.required_buyer_cash_krw,
        quoted_terms: assessed.mortgage.quoted_terms,
        replayed: false,
    }
}

async fn quote_mortgage_once(
    pool: &MySqlPool,
    property_rules: &dyn PropertyRules,
    user_id: u64,
    command: &CreateMortgageQuoteCommand,
    fingerprint: &str,
) -> Result<LifeStoreResult<MortgageQuoteReceipt>> {
    let mut tx = pool.begin().await?;
    let Some(current) = lock_property_save(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_QUOTE_MORTGAGE,
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
                read_stored_property_receipt(&mut tx, current.id, command.command_id.as_str())
                    .await?;
            if row.run_revision != current.run_revision {
                tx.commit().await?;
                return Ok(LifeStoreResult::Rejected(
                    LifeFailureCode::IdempotencyConflict,
                ));
            }
            let mut receipt: MortgageQuoteReceipt = serde_json::from_str(&row.result_json)
                .context("stored mortgage quote receipt is invalid")?;
            ensure!(
                row.command_kind == COMMAND_KIND_QUOTE_MORTGAGE
                    && row.payload_sha256 == fingerprint
                    && row.run_revision == command.cursor.expected_run_revision
                    && row.state_revision == command.cursor.expected_state_revision
                    && row.game_day == command.cursor.expected_game_day
                    && row.ledger_transaction_id.is_none()
                    && receipt.command_id == command.command_id
                    && receipt.listing_id == command.listing_id
                    && receipt.product_version_id == command.product_version_id
                    && receipt.requested_principal_krw == command.principal_krw
                    && !receipt.replayed,
                "stored mortgage quote receipt disagrees with its command"
            );
            let quote_exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(
                     SELECT 1 FROM loan_quote
                     WHERE id = ? AND save_id = ? AND run_revision = ?
                       AND purpose = 'mortgagePurchase' AND command_id = ?
                 )",
            )
            .bind(receipt.quote_id.get())
            .bind(current.id)
            .bind(current.run_revision)
            .bind(command.command_id.as_str())
            .fetch_one(&mut *tx)
            .await?;
            ensure!(
                quote_exists,
                "stored mortgage quote lost its durable evidence"
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
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::InvalidCommand));
    }
    let Some(context) = lock_purchase_context(&mut tx, current, command.listing_id).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::RateUnavailable));
    };
    let assessed = match assess_mortgage_purchase(
        &mut tx,
        property_rules,
        user_id,
        context,
        command.product_version_id,
        command.principal_krw,
    )
    .await?
    {
        Ok(assessed) => assessed,
        Err(code) => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(code));
        }
    };
    write_command_identity(&mut tx, assessed.context.current.id, &identity).await?;
    let quote_id = insert_mortgage_quote(&mut tx, command, fingerprint, &assessed).await?;
    let receipt = build_mortgage_quote_receipt(
        command.command_id.clone(),
        ResourceId::from_u64(quote_id),
        command.principal_krw,
        &assessed,
    );
    write_game_command_receipt(
        &mut tx,
        GameCommandReceiptWrite {
            save_id: assessed.context.current.id,
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_QUOTE_MORTGAGE,
            payload_sha256: fingerprint,
            market_world_id: assessed.context.current.market_world_id,
            committed_cursor: GameCommandCursor {
                run_revision: assessed.context.current.run_revision,
                state_revision: assessed.context.current.state_revision,
                game_day: assessed.context.current.game_day,
            },
            result: &receipt,
            ledger_transaction_id: None,
        },
    )
    .await?;
    let save = read_state(&mut tx, assessed.context.current.id).await?;
    tx.commit().await?;
    Ok(LifeStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

async fn insert_mortgage_quote(
    tx: &mut Transaction<'_, MySql>,
    command: &CreateMortgageQuoteCommand,
    fingerprint: &str,
    assessed: &AssessedMortgagePurchase,
) -> Result<u64> {
    let reasons_json = serde_json::to_string(&assessed.decision_reasons)
        .context("failed to serialize mortgage quote reasons")?;
    let terms_json = serde_json::to_string(&assessed.mortgage.quoted_terms)
        .context("failed to serialize mortgage quote terms")?;
    let (dsr_numerator_krw, dsr_denominator_krw, dsr_ratio_ppm, dsr_limit_ppm) = assessed
        .mortgage
        .dsr
        .as_ref()
        .map_or((None, None, None, None), |dsr| {
            (
                Some(dsr.numerator_krw),
                Some(dsr.denominator_krw),
                Some(dsr.ratio_ppm),
                Some(dsr.limit_ppm),
            )
        });
    let inserted = sqlx::query(
        "INSERT INTO loan_quote
             (save_id, run_revision, household_id, credit_model_version_id,
              loan_product_version_id, purpose, command_id, payload_sha256,
              expected_state_revision, created_game_day, expires_game_day,
              property_listing_id, current_lease_contract_id,
              recognized_collateral_value_krw, ltv_region_class, ltv_limit_ppm,
              maximum_mortgage_krw, ltv_numerator_krw, ltv_denominator_krw,
              ltv_ratio_ppm, acquisition_incidental_cost_krw, moving_cost_krw,
              returned_deposit_krw, available_buyer_cash_krw,
              required_buyer_cash_krw, requested_principal_krw,
              verified_annual_income_krw, verified_income_source,
              existing_loan_balance_krw, post_execution_balance_krw,
              dsr_numerator_krw, dsr_denominator_krw, dsr_ratio_ppm,
              dsr_limit_ppm, stress_rate_bp, stress_treatment,
              replaced_loan_contract_id, replaced_loan_principal_krw,
              regulatory_dsr_applied, decision_code, decision_reasons, quoted_terms)
         VALUES (?, ?, ?, ?, ?, 'mortgagePurchase', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                 ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'fullTermFixed', ?, ?, ?, ?, ?, ?)",
    )
    .bind(assessed.context.current.id)
    .bind(assessed.context.current.run_revision)
    .bind(assessed.context.household_id)
    .bind(assessed.context.scope.credit_model_version_id)
    .bind(assessed.mortgage.product.id.get())
    .bind(command.command_id.as_str())
    .bind(fingerprint)
    .bind(assessed.context.current.state_revision)
    .bind(assessed.context.current.game_day)
    .bind(assessed.context.current.game_day)
    .bind(assessed.context.listing.id)
    .bind(
        assessed
            .context
            .existing_lease
            .as_ref()
            .map(|lease| lease.id),
    )
    .bind(assessed.context.listing.price_krw)
    .bind(mortgage_ltv_region_class_db(assessed.ltv_region_class))
    .bind(assessed.ltv_limit_ppm)
    .bind(assessed.maximum_mortgage_krw)
    .bind(assessed.ltv.numerator_krw)
    .bind(assessed.ltv.denominator_krw)
    .bind(assessed.ltv.ratio_ppm)
    .bind(assessed.incidental_cost_krw)
    .bind(assessed.context.listing.moving_cost_krw)
    .bind(
        assessed
            .context
            .existing_lease
            .as_ref()
            .map_or(0, |lease| lease.deposit_krw),
    )
    .bind(assessed.available_buyer_cash_krw)
    .bind(assessed.required_buyer_cash_krw)
    .bind(command.principal_krw)
    .bind(assessed.mortgage.verified_annual_income_krw)
    .bind(
        assessed
            .mortgage
            .verified_income_source
            .map(|_| "activeEmploymentContract"),
    )
    .bind(assessed.mortgage.existing_loan_balance_krw)
    .bind(assessed.mortgage.post_execution_balance_krw)
    .bind(dsr_numerator_krw)
    .bind(dsr_denominator_krw)
    .bind(dsr_ratio_ppm)
    .bind(dsr_limit_ppm)
    .bind(assessed.mortgage.stress_rate_bp)
    .bind(
        assessed
            .context
            .linked_loan_evidence
            .as_ref()
            .map(|loan| loan.id),
    )
    .bind(
        assessed
            .context
            .linked_loan_evidence
            .as_ref()
            .map_or(0, |loan| loan.remaining_principal_krw),
    )
    .bind(assessed.mortgage.dsr_applied)
    .bind(mortgage_quote_decision_db(assessed.decision_code))
    .bind(reasons_json)
    .bind(terms_json)
    .execute(&mut **tx)
    .await?;
    let quote_id = inserted.last_insert_id();
    ensure!(quote_id > 0, "mortgage quote has no durable identity");
    Ok(quote_id)
}

async fn read_stored_property_receipt(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    command_id: &str,
) -> Result<StoredCommandReceiptRow> {
    sqlx::query_as(
        "SELECT command_kind, payload_sha256, run_revision, state_revision, game_day,
                CAST(result AS CHAR) AS result_json, ledger_transaction_id
         FROM command_receipt
         WHERE save_id = ? AND command_id = ? FOR SHARE",
    )
    .bind(save_id)
    .bind(command_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("property command identity has no final receipt")
}

async fn read_executable_mortgage_quote(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    household_id: u64,
    listing_id: ResourceId,
    quote_id: ResourceId,
) -> Result<Option<ExecutableMortgageQuoteRow>> {
    sqlx::query_as(
        "SELECT quote.id, quote.command_id, quote.loan_product_version_id,
                quote.requested_principal_krw, quote.created_game_day,
                quote.expires_game_day, quote.decision_code,
                CAST(receipt.result AS CHAR) AS result_json
         FROM loan_quote AS quote
         INNER JOIN command_receipt AS receipt
           ON receipt.save_id = quote.save_id
          AND receipt.run_revision = quote.run_revision
          AND BINARY receipt.command_id = BINARY quote.command_id
          AND receipt.command_kind = 'quoteMortgage'
          AND receipt.ledger_transaction_id IS NULL
         WHERE quote.id = ? AND quote.save_id = ? AND quote.run_revision = ?
           AND quote.household_id = ? AND quote.property_listing_id = ?
           AND quote.purpose = 'mortgagePurchase'",
    )
    .bind(quote_id.get())
    .bind(save_id)
    .bind(run_revision)
    .bind(household_id)
    .bind(listing_id.get())
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read the executable mortgage quote")
}

async fn purchase_property_once(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    property_rules: &dyn PropertyRules,
    property_tax_rules: &dyn PropertyTaxRules,
    user_id: u64,
    command: &PurchasePropertyCommand,
    fingerprint: &str,
) -> Result<LifeStoreResult<PropertyPurchaseReceipt>> {
    let mut tx = pool.begin().await?;
    let Some(current) = lock_property_save(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_PURCHASE_PROPERTY,
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
                read_stored_property_receipt(&mut tx, current.id, command.command_id.as_str())
                    .await?;
            if row.run_revision != current.run_revision {
                tx.commit().await?;
                return Ok(LifeStoreResult::Rejected(
                    LifeFailureCode::IdempotencyConflict,
                ));
            }
            let mut receipt: PropertyPurchaseReceipt = serde_json::from_str(&row.result_json)
                .context("stored property purchase receipt is invalid")?;
            let expected_state_revision = command
                .cursor
                .expected_state_revision
                .checked_add(1)
                .context("stored property purchase state revision overflowed")?;
            ensure!(
                row.command_kind == COMMAND_KIND_PURCHASE_PROPERTY
                    && row.payload_sha256 == fingerprint
                    && row.run_revision == command.cursor.expected_run_revision
                    && row.state_revision == expected_state_revision
                    && row.game_day == command.cursor.expected_game_day
                    && row.ledger_transaction_id.is_some()
                    && receipt.command_id == command.command_id
                    && receipt.listing_id == command.listing_id
                    && receipt.holding.listing_id == command.listing_id
                    && receipt.holding.status == PropertyHoldingStatusState::Active
                    && receipt.holding.purpose == PropertyHoldingPurposeState::OwnerOccupied
                    && receipt.holding.acquired_game_day == command.cursor.expected_game_day
                    && receipt.holding.acquisition_price_krw == receipt.purchase_price_krw
                    && receipt.holding.acquisition_incidental_cost_krw
                        == receipt.acquisition_incidental_cost_krw
                    && receipt.effective_from_game_day == command.cursor.expected_game_day
                    && receipt
                        .mortgage_execution
                        .as_ref()
                        .map(|execution| execution.quote_id)
                        == command.mortgage_quote_id
                    && receipt
                        .mortgage_execution
                        .as_ref()
                        .map(|execution| execution.property_holding_id)
                        .is_none_or(|holding_id| holding_id == receipt.holding.id)
                    && receipt
                        .mortgage_execution
                        .as_ref()
                        .map(|execution| execution.loan_id)
                        == receipt.holding.mortgage_loan_id
                    && !receipt.replayed,
                "stored property purchase receipt disagrees with its command"
            );
            let holding_exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(
                     SELECT 1 FROM property_holding
                     WHERE id = ? AND save_id = ? AND run_revision = ?
                       AND property_listing_id = ? AND acquisition_command_id = ?
                       AND purpose = 'ownerOccupied' AND acquired_game_day = ?
                       AND acquisition_price_krw = ?
                       AND acquisition_incidental_cost_krw = ?
                 )",
            )
            .bind(receipt.holding.id.get())
            .bind(current.id)
            .bind(current.run_revision)
            .bind(command.listing_id.get())
            .bind(command.command_id.as_str())
            .bind(receipt.holding.acquired_game_day)
            .bind(receipt.holding.acquisition_price_krw)
            .bind(receipt.holding.acquisition_incidental_cost_krw)
            .fetch_one(&mut *tx)
            .await?;
            ensure!(holding_exists, "stored property purchase lost its holding");
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
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::InvalidCommand));
    }
    let Some(mut context) = lock_purchase_context(&mut tx, current, command.listing_id).await?
    else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::RateUnavailable));
    };
    if context.active_holding_count != 0
        || context.residence.effective_from_game_day == context.current.game_day
        || context.lease_exit_restricted
    {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
    }
    let mut mortgage_execution_plan: Option<(ResourceId, i64, Box<MortgageLoanAssessment>)> = None;
    let incidental_cost_krw;
    if let Some(quote_id) = command.mortgage_quote_id {
        let Some(stored_quote) = read_executable_mortgage_quote(
            &mut tx,
            context.current.id,
            context.current.run_revision,
            context.household_id,
            command.listing_id,
            quote_id,
        )
        .await?
        else {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
        };
        if stored_quote.id != quote_id.get()
            || stored_quote.created_game_day != context.current.game_day
            || stored_quote.expires_game_day != context.current.game_day
            || stored_quote.decision_code != "eligible"
        {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
        }
        let assessed = match assess_mortgage_purchase(
            &mut tx,
            property_rules,
            user_id,
            context,
            ResourceId::from_u64(stored_quote.loan_product_version_id),
            stored_quote.requested_principal_krw,
        )
        .await?
        {
            Ok(assessed) => assessed,
            Err(code) => {
                tx.commit().await?;
                return Ok(LifeStoreResult::Rejected(code));
            }
        };
        let failure = mortgage_decision_failure(assessed.decision_code);
        if let Some(code) = failure {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(code));
        }
        let stored_receipt: MortgageQuoteReceipt = serde_json::from_str(&stored_quote.result_json)
            .context("stored executable mortgage receipt is invalid")?;
        ensure!(
            stored_receipt.command_id.as_str() == stored_quote.command_id
                && !stored_receipt.replayed,
            "executable mortgage receipt disagrees with its quote identity"
        );
        let fresh_receipt = build_mortgage_quote_receipt(
            stored_receipt.command_id.clone(),
            quote_id,
            stored_quote.requested_principal_krw,
            &assessed,
        );
        if fresh_receipt != stored_receipt {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
        }
        let existing_contract: Option<u64> =
            sqlx::query_scalar("SELECT id FROM loan_contract WHERE loan_quote_id = ? FOR UPDATE")
                .bind(quote_id.get())
                .fetch_optional(&mut *tx)
                .await?;
        if existing_contract.is_some() {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(LifeFailureCode::ContractConflict));
        }
        incidental_cost_krw = assessed.incidental_cost_krw;
        context = assessed.context;
        mortgage_execution_plan = Some((
            quote_id,
            stored_quote.requested_principal_krw,
            assessed.mortgage,
        ));
    } else {
        incidental_cost_krw = property_rules
            .calculate_acquisition_incidental_cost(AcquisitionIncidentalCostInput {
                purchase_price_krw: context.listing.price_krw,
                cost_ppm: i64::from(context.scope.incidental_cost_ppm),
            })
            .context("cash property incidental-cost calculation failed")?;
    }
    let mortgage_principal_krw = mortgage_execution_plan
        .as_ref()
        .map_or(0, |(_, principal, _)| *principal);
    let returned_deposit_krw = context
        .existing_lease
        .as_ref()
        .map_or(0, |lease| lease.deposit_krw);
    let repaid_loan_principal_krw = context
        .prepared_payoff
        .as_ref()
        .map_or(0, |payoff| payoff.principal_krw());
    let funding_plan = match property_rules.plan_purchase_funding(PropertyPurchaseFundingInput {
        wallet_cash_krw: context.current.cash_krw,
        returned_deposit_krw,
        repaid_loan_principal_krw,
        purchase_price_krw: context.listing.price_krw,
        acquisition_incidental_cost_krw: incidental_cost_krw,
        moving_cost_krw: context.listing.moving_cost_krw,
        new_mortgage_principal_krw: mortgage_principal_krw,
    }) {
        Ok(plan) => plan,
        Err(PropertyError::InsufficientWalletCash) => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(
                LifeFailureCode::InsufficientWalletCash,
            ));
        }
        Err(error) => return Err(error).context("property purchase funding failed"),
    };
    write_command_identity(&mut tx, context.current.id, &identity).await?;
    let repaid_deposit_loan = match context.prepared_payoff.take() {
        Some(prepared) => Some(
            apply_lease_move_payoff_in_tx(
                &mut tx,
                context.current.id,
                context.current.run_revision,
                context.current.game_day,
                command.command_id.as_str(),
                *prepared,
            )
            .await?,
        ),
        None => None,
    };
    let ended_lease_id = context.existing_lease.as_ref().map(|lease| lease.id);
    if let Some(lease) = &context.existing_lease {
        let lease_current = context.current.lease_view();
        cancel_future_lease_rent_charge(&mut tx, &lease_current, lease).await?;
        close_existing_lease_lifecycle(&mut tx, &lease_current, lease).await?;
        close_existing_lease(&mut tx, &lease_current, lease).await?;
    }
    close_existing_residence(&mut tx, &context.current.lease_view(), &context.residence).await?;
    let holding_id =
        insert_property_holding(&mut tx, &context, command, incidental_cost_krw).await?;
    if context.scope.real_estate_version_key == "dev-unranked-m4-real-estate-sale-tax-2026-v6" {
        create_acquisition_property_tax_event_in_tx(
            &mut tx,
            property_tax_rules,
            AcquisitionPropertyTaxEventInput {
                context: PropertyTaxRunContext {
                    save_id: context.current.id,
                    market_world_id: context.current.market_world_id,
                    policy_set_id: context.current.policy_set_id,
                    run_revision: context.current.run_revision,
                    game_day: context.current.game_day,
                    market_date: context.scope.market_date,
                },
                household_id: context.household_id,
                holding_id,
                purchase_price_krw: context.listing.price_krw,
                valuation_price_index_ppm: context.listing.current_price_index_ppm,
            },
        )
        .await?;
    }
    let mortgage_execution = match mortgage_execution_plan {
        Some((quote_id, principal_krw, assessment)) => {
            let execution = originate_mortgage_in_tx(
                &mut tx,
                command.command_id.as_str(),
                holding_id,
                quote_id,
                principal_krw,
                assessment,
            )
            .await?;
            insert_property_lien(&mut tx, &context, holding_id, execution.loan_id.get()).await?;
            Some(execution)
        }
        None => None,
    };
    let residence_id = insert_owner_residence(&mut tx, &context, holding_id).await?;
    let ledger_transaction_id = write_property_purchase_ledger(
        &mut tx,
        finance_rules,
        &context,
        command.command_id.as_str(),
        holding_id,
        &funding_plan,
        ended_lease_id,
        repaid_deposit_loan
            .as_ref()
            .map(|payoff| payoff.loan_id.get()),
        mortgage_execution
            .as_ref()
            .map(|execution| execution.loan_id.get()),
    )
    .await?;
    if let Some(payoff) = &repaid_deposit_loan {
        mark_lease_move_payoff_applied_in_tx(
            &mut tx,
            context.current.id,
            context.current.run_revision,
            payoff,
            ledger_transaction_id,
        )
        .await?;
    }
    let committed_state_revision = context
        .current
        .state_revision
        .checked_add(1)
        .context("property purchase state revision overflowed")?;
    update_save_after_property_purchase(
        &mut tx,
        &context.current,
        committed_state_revision,
        &funding_plan,
    )
    .await?;
    validate_debt_projection_in_tx(&mut tx, context.current.id, context.current.run_revision)
        .await?;
    validate_property_projection_in_tx(&mut tx, context.current.id, context.current.run_revision)
        .await?;
    let holding = read_holding_by_id(
        &mut tx,
        context.current.id,
        context.current.run_revision,
        holding_id,
    )
    .await?;
    let receipt = PropertyPurchaseReceipt {
        command_id: command.command_id.clone(),
        holding,
        residence_id: ResourceId::from_u64(residence_id),
        listing_id: command.listing_id,
        purchase_price_krw: context.listing.price_krw,
        acquisition_incidental_cost_krw: incidental_cost_krw,
        moving_cost_krw: context.listing.moving_cost_krw,
        returned_deposit_krw,
        wallet_delta_krw: funding_plan.wallet_delta_krw,
        effective_from_game_day: context.current.game_day,
        ended_lease_id: ended_lease_id.map(ResourceId::from_u64),
        repaid_deposit_loan,
        mortgage_execution,
        replayed: false,
    };
    write_game_command_receipt(
        &mut tx,
        GameCommandReceiptWrite {
            save_id: context.current.id,
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_PURCHASE_PROPERTY,
            payload_sha256: fingerprint,
            market_world_id: context.current.market_world_id,
            committed_cursor: GameCommandCursor {
                run_revision: context.current.run_revision,
                state_revision: committed_state_revision,
                game_day: context.current.game_day,
            },
            result: &receipt,
            ledger_transaction_id: Some(ledger_transaction_id),
        },
    )
    .await?;
    let save = read_state(&mut tx, context.current.id).await?;
    tx.commit().await?;
    Ok(LifeStoreResult::Applied {
        receipt,
        save: Box::new(save),
    })
}

async fn insert_property_holding(
    tx: &mut Transaction<'_, MySql>,
    context: &LockedPurchaseContext,
    command: &PurchasePropertyCommand,
    incidental_cost_krw: i64,
) -> Result<u64> {
    let inserted = sqlx::query(
        "INSERT INTO property_holding
             (save_id, run_revision, household_id, property_listing_id,
              real_estate_model_version_id, acquisition_policy_set_id,
              acquisition_credit_policy_set_id, acquisition_command_id,
              status, purpose, region_key, property_type,
              exclusive_area_square_meters, acquired_game_day, disposed_game_day,
              acquisition_price_krw, acquisition_incidental_cost_krw, book_value_krw,
              acquisition_price_index_ppm)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'active', 'ownerOccupied', ?, ?, ?, ?, NULL, ?, ?, ?, ?)",
    )
    .bind(context.current.id)
    .bind(context.current.run_revision)
    .bind(context.household_id)
    .bind(context.listing.id)
    .bind(context.scope.real_estate_model_version_id)
    .bind(context.current.policy_set_id)
    .bind(context.scope.credit_policy_set_id)
    .bind(command.command_id.as_str())
    .bind(&context.listing.region_key)
    .bind(&context.listing.property_type)
    .bind(context.listing.exclusive_area_square_meters)
    .bind(context.current.game_day)
    .bind(context.listing.price_krw)
    .bind(incidental_cost_krw)
    .bind(context.listing.price_krw)
    .bind(context.listing.current_price_index_ppm)
    .execute(&mut **tx)
    .await?;
    let holding_id = inserted.last_insert_id();
    ensure!(holding_id > 0, "property holding has no durable identity");
    Ok(holding_id)
}

async fn insert_property_lien(
    tx: &mut Transaction<'_, MySql>,
    context: &LockedPurchaseContext,
    holding_id: u64,
    loan_contract_id: u64,
) -> Result<()> {
    let inserted = sqlx::query(
        "INSERT INTO property_lien
             (save_id, run_revision, property_holding_id, loan_contract_id,
              lien_priority, status)
         VALUES (?, ?, ?, ?, 1, 'active')",
    )
    .bind(context.current.id)
    .bind(context.current.run_revision)
    .bind(holding_id)
    .bind(loan_contract_id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        inserted.rows_affected() == 1,
        "property lien was not inserted"
    );
    Ok(())
}

async fn insert_owner_residence(
    tx: &mut Transaction<'_, MySql>,
    context: &LockedPurchaseContext,
    holding_id: u64,
) -> Result<u64> {
    let inserted = sqlx::query(
        "INSERT INTO residence
             (save_id, run_revision, household_id, region_key, tenure_type,
              lease_contract_id, property_holding_id,
              effective_from_game_day, effective_to_game_day)
         VALUES (?, ?, ?, ?, 'owner', NULL, ?, ?, NULL)",
    )
    .bind(context.current.id)
    .bind(context.current.run_revision)
    .bind(context.household_id)
    .bind(&context.listing.region_key)
    .bind(holding_id)
    .bind(context.current.game_day)
    .execute(&mut **tx)
    .await?;
    let residence_id = inserted.last_insert_id();
    ensure!(residence_id > 0, "owner residence has no durable identity");
    Ok(residence_id)
}

async fn read_holding_by_id(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    holding_id: u64,
) -> Result<PropertyHoldingState> {
    let row: PropertyHoldingRow = sqlx::query_as(
        "SELECT holding.id, holding.property_listing_id, holding.status, holding.purpose,
                holding.region_key, holding.property_type,
                holding.exclusive_area_square_meters, holding.acquired_game_day,
                holding.acquisition_price_krw,
                holding.acquisition_incidental_cost_krw, holding.book_value_krw,
                mortgage.id AS mortgage_loan_id
         FROM property_holding AS holding
         LEFT JOIN property_lien AS lien
           ON lien.save_id = holding.save_id
          AND lien.run_revision = holding.run_revision
          AND lien.property_holding_id = holding.id
          AND lien.status = 'active'
         LEFT JOIN loan_contract AS mortgage
           ON mortgage.id = lien.loan_contract_id
          AND mortgage.save_id = holding.save_id
          AND mortgage.run_revision = holding.run_revision
          AND mortgage.property_holding_id = holding.id
          AND mortgage.product_kind = 'mortgage'
          AND mortgage.status IN ('active', 'delinquent', 'defaulted', 'restructured')
         WHERE holding.id = ? AND holding.save_id = ? AND holding.run_revision = ?",
    )
    .bind(holding_id)
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await?;
    property_holding_from_row(row)
}

async fn update_save_after_property_purchase(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedPropertySaveRow,
    committed_state_revision: u64,
    plan: &PropertyPurchaseFundingPlan,
) -> Result<()> {
    let debt_krw = current
        .debt_krw
        .checked_add(plan.debt_delta_krw)
        .context("property purchase debt projection overflowed")?;
    let property_book_value_krw = current
        .property_book_value_krw
        .checked_add(plan.property_book_value_delta_krw)
        .context("property book-value projection overflowed")?;
    let updated = sqlx::query(
        "UPDATE save
         SET state_revision = ?, cash_krw = ?, debt_krw = ?, property_book_value_krw = ?
         WHERE id = ? AND market_world_id = ? AND policy_set_id = ?
           AND run_revision = ? AND state_revision = ? AND game_day = ?
           AND cash_krw = ? AND debt_krw = ? AND property_book_value_krw = ?",
    )
    .bind(committed_state_revision)
    .bind(plan.wallet_cash_after_krw)
    .bind(debt_krw)
    .bind(property_book_value_krw)
    .bind(current.id)
    .bind(current.market_world_id)
    .bind(current.policy_set_id)
    .bind(current.run_revision)
    .bind(current.state_revision)
    .bind(current.game_day)
    .bind(current.cash_krw)
    .bind(current.debt_krw)
    .bind(current.property_book_value_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "property purchase cursor changed"
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn write_property_purchase_ledger(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    context: &LockedPurchaseContext,
    command_id: &str,
    holding_id: u64,
    plan: &PropertyPurchaseFundingPlan,
    ended_lease_id: Option<u64>,
    repaid_loan_id: Option<u64>,
    mortgage_loan_id: Option<u64>,
) -> Result<u64> {
    let mut postings = Vec::new();
    let mut references = Vec::new();
    let mut push = |account_code, amount_krw, reference| {
        if amount_krw != 0 {
            postings.push(LedgerPosting {
                account_code,
                financial_account_id: None,
                amount_krw,
            });
            references.push(reference);
        }
    };
    push(
        LedgerAccountCode::LeaseDepositAsset,
        -plan.returned_deposit_krw,
        ended_lease_id
            .map(PropertyLedgerReference::Lease)
            .unwrap_or(PropertyLedgerReference::None),
    );
    push(
        LedgerAccountCode::LoanPrincipalLiability,
        plan.repaid_loan_principal_krw,
        repaid_loan_id
            .map(PropertyLedgerReference::Loan)
            .unwrap_or(PropertyLedgerReference::None),
    );
    push(
        LedgerAccountCode::PropertyAsset,
        plan.purchase_price_krw,
        PropertyLedgerReference::Holding(holding_id),
    );
    push(
        LedgerAccountCode::AcquisitionIncidentalExpense,
        plan.acquisition_incidental_cost_krw,
        PropertyLedgerReference::Holding(holding_id),
    );
    push(
        LedgerAccountCode::MovingExpense,
        plan.moving_cost_krw,
        PropertyLedgerReference::None,
    );
    push(
        LedgerAccountCode::LoanPrincipalLiability,
        -plan.new_mortgage_principal_krw,
        mortgage_loan_id
            .map(PropertyLedgerReference::Loan)
            .unwrap_or(PropertyLedgerReference::None),
    );
    push(
        LedgerAccountCode::Wallet,
        plan.wallet_delta_krw,
        PropertyLedgerReference::None,
    );
    let ledger = finance_rules.create_ledger_transaction(LedgerTransactionDraft {
        policy: RunPolicyContext {
            run: RunId {
                save_id: ResourceId::from_u64(context.current.id),
                run_revision: context.current.run_revision,
            },
            policy_set_id: ResourceId::from_u64(context.current.policy_set_id),
        },
        source: LedgerSource {
            kind: LedgerSourceKind::PropertyPurchase,
            source_id: command_id.to_owned(),
        },
        game_day: context.current.game_day,
        description: "주택 매수".to_owned(),
        postings,
    })?;
    write_property_ledger_transaction(tx, &ledger, &references).await
}

#[allow(clippy::too_many_arguments)]
async fn write_property_sale_ledger(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    context: PropertyTaxRunContext,
    execution_id: u64,
    holding_id: u64,
    loan_id: Option<u64>,
    property_tax_event_id: u64,
    proceeds: &PropertySaleProceedsPlan,
) -> Result<u64> {
    let mut postings = Vec::with_capacity(proceeds.postings.len());
    let mut references = Vec::with_capacity(proceeds.postings.len());
    for posting in &proceeds.postings {
        let reference = match posting.account_code {
            LedgerAccountCode::PropertyAsset
            | LedgerAccountCode::RealizedGainLoss
            | LedgerAccountCode::PropertyDispositionExpense => {
                PropertyLedgerReference::Holding(holding_id)
            }
            LedgerAccountCode::LoanPrincipalLiability | LedgerAccountCode::LoanFeeExpense => {
                PropertyLedgerReference::Loan(
                    loan_id.context("property sale mortgage posting has no loan")?,
                )
            }
            LedgerAccountCode::PropertyTaxExpense => {
                ensure!(
                    posting.capital_gains_tax_scope.is_some(),
                    "property sale tax posting has no tax scope"
                );
                PropertyLedgerReference::TaxEvent(property_tax_event_id)
            }
            LedgerAccountCode::Wallet => PropertyLedgerReference::None,
            _ => bail!("property sale plan contains an unsupported ledger account"),
        };
        postings.push(LedgerPosting {
            account_code: posting.account_code,
            financial_account_id: None,
            amount_krw: posting.amount_krw,
        });
        references.push(reference);
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
                kind: LedgerSourceKind::PropertySale,
                source_id: execution_id.to_string(),
            },
            game_day: context.game_day,
            description: "주택 매도".to_owned(),
            postings,
        })
        .context("property sale ledger is invalid")?;
    write_property_ledger_transaction(tx, &ledger, &references).await
}

async fn write_property_ledger_transaction(
    tx: &mut Transaction<'_, MySql>,
    ledger: &LedgerTransaction,
    references: &[PropertyLedgerReference],
) -> Result<u64> {
    ensure!(
        ledger.postings().len() == references.len(),
        "property ledger posting ownership is incomplete"
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
    .bind(property_ledger_source_db(ledger.source().kind)?)
    .bind(&ledger.source().source_id)
    .bind(ledger.description())
    .execute(&mut **tx)
    .await?;
    let transaction_id = inserted.last_insert_id();
    ensure!(
        transaction_id > 0,
        "property ledger has no durable identity"
    );
    for (index, (posting, reference)) in ledger.postings().iter().zip(references.iter()).enumerate()
    {
        let (lease_contract_id, loan_contract_id, property_holding_id, property_tax_event_id) =
            match reference {
                PropertyLedgerReference::None => (None, None, None, None),
                PropertyLedgerReference::Holding(id) => (None, None, Some(*id), None),
                PropertyLedgerReference::Lease(id) => (Some(*id), None, None, None),
                PropertyLedgerReference::Loan(id) => (None, Some(*id), None, None),
                PropertyLedgerReference::TaxEvent(id) => (None, None, None, Some(*id)),
            };
        let valid_reference = match posting.account_code {
            LedgerAccountCode::PropertyAsset
            | LedgerAccountCode::AcquisitionIncidentalExpense
            | LedgerAccountCode::RealizedGainLoss
            | LedgerAccountCode::PropertyDispositionExpense => {
                matches!(reference, PropertyLedgerReference::Holding(_))
            }
            LedgerAccountCode::LeaseDepositAsset => {
                matches!(reference, PropertyLedgerReference::Lease(_))
            }
            LedgerAccountCode::LoanPrincipalLiability | LedgerAccountCode::LoanFeeExpense => {
                matches!(reference, PropertyLedgerReference::Loan(_))
            }
            LedgerAccountCode::PropertyTaxExpense => {
                matches!(reference, PropertyLedgerReference::TaxEvent(_))
            }
            LedgerAccountCode::MovingExpense | LedgerAccountCode::Wallet => {
                matches!(reference, PropertyLedgerReference::None)
            }
            _ => false,
        };
        ensure!(
            valid_reference,
            "property ledger posting has invalid ownership"
        );
        let posting_order =
            u16::try_from(index + 1).context("too many property ledger postings")?;
        sqlx::query(
            "INSERT INTO ledger_posting
                 (save_id, run_revision, ledger_transaction_id, posting_order,
                  account_code, financial_account_id, lease_contract_id,
                  loan_contract_id, lease_rent_charge_id, lease_arrear_id,
                  property_holding_id, property_tax_event_id, amount_krw)
             VALUES (?, ?, ?, ?, ?, NULL, ?, ?, NULL, NULL, ?, ?, ?)",
        )
        .bind(policy.run.save_id.get())
        .bind(policy.run.run_revision)
        .bind(transaction_id)
        .bind(posting_order)
        .bind(property_ledger_account_db(posting.account_code)?)
        .bind(lease_contract_id)
        .bind(loan_contract_id)
        .bind(property_holding_id)
        .bind(property_tax_event_id)
        .bind(posting.amount_krw)
        .execute(&mut **tx)
        .await?;
    }
    Ok(transaction_id)
}

fn mortgage_decision_failure(decision: MortgageQuoteDecisionState) -> Option<LifeFailureCode> {
    match decision {
        MortgageQuoteDecisionState::Eligible => None,
        MortgageQuoteDecisionState::CreditRestricted => Some(LifeFailureCode::CreditRestricted),
        MortgageQuoteDecisionState::PurchaseRestricted => Some(LifeFailureCode::ContractConflict),
        MortgageQuoteDecisionState::CollateralLimit => Some(LifeFailureCode::CollateralLimit),
        MortgageQuoteDecisionState::IncomeUnavailable => Some(LifeFailureCode::IncomeUnavailable),
        MortgageQuoteDecisionState::DebtServiceLimit => Some(LifeFailureCode::DebtServiceLimit),
        MortgageQuoteDecisionState::InsufficientOwnFunds => {
            Some(LifeFailureCode::InsufficientWalletCash)
        }
    }
}

const fn mortgage_quote_decision_db(decision: MortgageQuoteDecisionState) -> &'static str {
    match decision {
        MortgageQuoteDecisionState::Eligible => "eligible",
        MortgageQuoteDecisionState::CreditRestricted => "creditRestricted",
        MortgageQuoteDecisionState::PurchaseRestricted => "purchaseRestricted",
        MortgageQuoteDecisionState::CollateralLimit => "collateralLimit",
        MortgageQuoteDecisionState::IncomeUnavailable => "incomeUnavailable",
        MortgageQuoteDecisionState::DebtServiceLimit => "debtServiceLimit",
        MortgageQuoteDecisionState::InsufficientOwnFunds => "insufficientOwnFunds",
    }
}

const fn mortgage_ltv_region_class_db(class: MortgageLtvRegionClassState) -> &'static str {
    match class {
        MortgageLtvRegionClassState::RegulatedCapitalProxy => "regulatedCapitalProxy",
        MortgageLtvRegionClassState::NonRegulatedProxy => "nonRegulatedProxy",
    }
}

fn property_ledger_account_db(account: LedgerAccountCode) -> Result<&'static str> {
    match account {
        LedgerAccountCode::Wallet => Ok("wallet"),
        LedgerAccountCode::LeaseDepositAsset => Ok("leaseDepositAsset"),
        LedgerAccountCode::LoanPrincipalLiability => Ok("loanPrincipalLiability"),
        LedgerAccountCode::MovingExpense => Ok("movingExpense"),
        LedgerAccountCode::PropertyAsset => Ok("propertyAsset"),
        LedgerAccountCode::AcquisitionIncidentalExpense => Ok("acquisitionIncidentalExpense"),
        LedgerAccountCode::RealizedGainLoss => Ok("realizedGainLoss"),
        LedgerAccountCode::PropertyDispositionExpense => Ok("propertyDispositionExpense"),
        LedgerAccountCode::LoanFeeExpense => Ok("loanFeeExpense"),
        LedgerAccountCode::PropertyTaxExpense => Ok("propertyTaxExpense"),
        _ => bail!("unsupported property ledger account"),
    }
}

fn property_ledger_source_db(source: LedgerSourceKind) -> Result<&'static str> {
    match source {
        LedgerSourceKind::PropertyPurchase => Ok("propertyPurchase"),
        LedgerSourceKind::PropertySale => Ok("propertySale"),
        _ => bail!("unsupported property ledger source"),
    }
}

fn has_cursor(current: &LockedPropertySaveRow, cursor: CommandCursor) -> bool {
    cursor.expected_run_revision == current.run_revision
        && cursor.expected_state_revision == current.state_revision
        && cursor.expected_game_day == current.game_day
}

fn mortgage_quote_fingerprint(command: &CreateMortgageQuoteCommand) -> String {
    let canonical = format!(
        concat!(
            "lifeledger.life.quoteMortgage.v1\n",
            "expectedRunRevision={}\n",
            "expectedStateRevision={}\n",
            "expectedGameDay={}\n",
            "listingId={}\n",
            "productVersionId={}\n",
            "principalKrw={}"
        ),
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.listing_id.get(),
        command.product_version_id.get(),
        command.principal_krw,
    );
    sha256(&canonical)
}

fn property_purchase_fingerprint(command: &PurchasePropertyCommand) -> String {
    let mortgage_quote_id = command
        .mortgage_quote_id
        .map_or_else(|| "null".to_owned(), |id| id.get().to_string());
    let canonical = format!(
        concat!(
            "lifeledger.life.purchaseProperty.v1\n",
            "expectedRunRevision={}\n",
            "expectedStateRevision={}\n",
            "expectedGameDay={}\n",
            "listingId={}\n",
            "mortgageQuoteId={}"
        ),
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.listing_id.get(),
        mortgage_quote_id,
    );
    sha256(&canonical)
}

fn create_property_sale_order_fingerprint(command: &CreatePropertySaleOrderCommand) -> String {
    let canonical = format!(
        concat!(
            "lifeledger.life.createPropertySaleOrder.v1\n",
            "expectedRunRevision={}\n",
            "expectedStateRevision={}\n",
            "expectedGameDay={}\n",
            "holdingId={}\n",
            "askingPriceKrw={}"
        ),
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.holding_id.get(),
        command.asking_price_krw,
    );
    sha256(&canonical)
}

fn reprice_property_sale_order_fingerprint(command: &RepricePropertySaleOrderCommand) -> String {
    let canonical = format!(
        concat!(
            "lifeledger.life.repricePropertySaleOrder.v1\n",
            "expectedRunRevision={}\n",
            "expectedStateRevision={}\n",
            "expectedGameDay={}\n",
            "orderId={}\n",
            "askingPriceKrw={}"
        ),
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.order_id.get(),
        command.asking_price_krw,
    );
    sha256(&canonical)
}

fn cancel_property_sale_order_fingerprint(command: &CancelPropertySaleOrderCommand) -> String {
    let canonical = format!(
        concat!(
            "lifeledger.life.cancelPropertySaleOrder.v1\n",
            "expectedRunRevision={}\n",
            "expectedStateRevision={}\n",
            "expectedGameDay={}\n",
            "orderId={}"
        ),
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.order_id.get(),
    );
    sha256(&canonical)
}

fn sha256(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn given_payable_mortgage() -> PropertySaleMortgagePlan {
        PropertySaleMortgagePlan::Payable {
            principal_krw: 100,
            fee_krw: 5,
        }
    }

    fn when_sale_financials_are_planned(
        gross_sale_price_krw: i64,
        disposition_cost_krw: i64,
        mortgage: PropertySaleMortgagePlan,
    ) -> PropertySaleFinancialPlan {
        plan_property_sale_financials(
            crate::life::create_property_rules().as_ref(),
            gross_sale_price_krw,
            600,
            disposition_cost_krw,
            mortgage,
            20,
            2,
        )
        .expect("매각 자금 계획을 계산할 수 있어야 한다")
    }

    mod context_주담대가_총매수비용보다_큰_경우 {
        use super::*;

        #[test]
        fn given_총비용보다_1원큰_주담대_when_필요자기자금을_계산하면_then_0원이다() {
            let purchase_price_krw = 100;
            let incidental_cost_krw = 2;
            let moving_cost_krw = 1;
            let mortgage_principal_krw = 104;

            let result = calculate_required_buyer_cash(
                purchase_price_krw,
                incidental_cost_krw,
                moving_cost_krw,
                mortgage_principal_krw,
            );

            assert_eq!(result.expect("필요 자기자금을 계산해야 한다"), 0);
        }
    }

    mod context_활성보유한도를_판단하는_경우 {
        use super::*;

        #[test]
        fn given_1채보유와_1채한도_when_판단하면_then_추가매수를_제한한다() {
            let active_holding_count = 1;
            let maximum_active_holdings = 1;

            let result =
                active_holding_limit_reached(active_holding_count, maximum_active_holdings);

            assert!(result);
        }
    }

    mod context_매각대금으로_모든_비용을_정산할_수_있는_경우 {
        use super::*;

        #[test]
        fn given_정상주담대와_세금_when_매각자금계획_then_워터폴과_원장합계가_일치한다() {
            let mortgage = given_payable_mortgage();

            let result = when_sale_financials_are_planned(1_000, 10, mortgage);

            let proceeds = result.proceeds.expect("적용 가능한 매각 계획이어야 한다");
            let ledger_sum = proceeds
                .postings
                .iter()
                .map(|posting| posting.amount_krw)
                .sum::<i64>();
            assert_eq!(result.rejection_reason, None);
            assert_eq!(result.mortgage_principal_krw, 100);
            assert_eq!(result.mortgage_fee_krw, 5);
            assert_eq!(result.transfer_tax_krw, 22);
            assert_eq!(result.net_wallet_proceeds_krw, 863);
            assert_eq!(proceeds.wallet_proceeds_krw, 863);
            assert_eq!(ledger_sum, 0);
        }
    }

    mod context_매각대금이_정산액보다_부족한_경우 {
        use super::*;

        #[test]
        fn given_부족한매각대금_when_매각자금계획_then_부족대금으로_거절한다() {
            let mortgage = given_payable_mortgage();

            let result = when_sale_financials_are_planned(100, 10, mortgage);

            assert_eq!(
                result.rejection_reason,
                Some(PropertySaleOrderRejectionReasonState::InsufficientProceeds)
            );
            assert_eq!(result.net_wallet_proceeds_krw, -37);
            assert!(result.proceeds.is_none());
        }
    }

    mod context_주담대를_정상상환할_수_없는_경우 {
        use super::*;

        #[test]
        fn given_상환불가주담대_when_매각자금계획_then_주담대사유로_거절한다() {
            let mortgage = PropertySaleMortgagePlan::NotPayable {
                principal_krw: 100,
                fee_krw: 5,
            };

            let result = when_sale_financials_are_planned(1_000, 10, mortgage);

            assert_eq!(
                result.rejection_reason,
                Some(PropertySaleOrderRejectionReasonState::MortgageNotPayable)
            );
            assert_eq!(result.net_wallet_proceeds_krw, 863);
            assert!(result.proceeds.is_none());
        }
    }
}
