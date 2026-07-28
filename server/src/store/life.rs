//! M4-A household, living-cost, budget, and essential-arrear persistence.

use std::sync::Arc;

use std::collections::HashSet;

use anyhow::{Context, Result, bail, ensure};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{MySql, MySqlPool, Transaction};
use time::{Date, Month};

use super::corporations::{
    create_corporation, read_corporation_detail, read_corporation_snapshot_in_tx,
    read_corporation_templates,
};
use super::housing::{
    is_retryable_database_error, prepare_current_housing_catalogs,
    prepare_property_daily_for_target, read_housing_listings,
};
use super::insolvency::{
    act_on_insolvency_case, prepare_insolvency_case, read_case_detail, read_claim_page,
    read_insolvency_overview, read_insolvency_snapshot_in_tx, read_liquidation_page,
};
use super::insurance::{
    cancel_insurance_contract, enroll_insurance_contract, file_insurance_claim, read_insurance,
    read_insurance_snapshot_in_tx,
};
use super::leases::{
    lock_current_household, pay_lease_arrear_command, read_active_housing_lease_snapshot_in_tx,
    read_active_lease_arrears_in_tx, read_current_housing_lease, start_housing_lease_command,
    tenant_lease_boundary_conflict_in_tx,
};
use super::life_events::{read_life_events, read_pending_life_events_in_tx, resolve_life_event};
use super::loans::{
    LeaseDepositLoanQuoteCreation, LoanExecutionCreation, LoanPrepaymentCreation,
    LoanQuoteCreation, create_lease_deposit_loan_quote_in_tx, create_loan_quote_in_tx,
    execute_loan_in_tx, prepay_loan_in_tx, read_credit_and_loan_snapshot_in_tx,
    read_loan_detail_in_tx, read_loan_installment_page_in_tx, read_loan_product_catalog_in_tx,
    validate_debt_projection_in_tx,
};
use super::mysql::{
    CommandIdentitySpec, CommandIdentityState, GameCommandReceiptWrite, inspect_command_identity,
    read_state, write_command_identity, write_game_command_receipt,
};
use super::properties::{
    cancel_property_sale_order_command, create_property_sale_order_command,
    purchase_property_command, quote_mortgage_command, read_active_property_holdings_in_tx,
    read_property_holdings, read_property_sale_orders, reprice_property_sale_order_command,
    validate_property_projection_in_tx,
};
use super::types::{
    ActOnInsolvencyCaseCommand, ApplyWelfareProgramCommand, CancelInsuranceContractCommand,
    CancelPropertySaleOrderCommand, CorporationReadResult, CorporationReceipt,
    CorporationSummaryState, CorporationTemplatesState, CreateCorporationCommand,
    CreateLeaseDepositLoanQuoteCommand, CreateLoanQuoteCommand, CreateMortgageQuoteCommand,
    CreatePropertySaleOrderCommand, CreditOverviewState, CreditReasonState,
    EnrollInsuranceContractCommand, EssentialArrearPaymentReceipt, EssentialArrearState,
    ExecuteLoanCommand, FileInsuranceClaimCommand, GameCommandCursor, HousingLeaseCurrentState,
    HousingLeaseMoveReceipt, HousingListingsQueryState, HousingListingsState,
    HousingPropertyHoldingsState, InsolvencyCaseDetailState, InsolvencyCaseReceipt,
    InsolvencyClaimPageState, InsolvencyLiquidationPageState, InsolvencyReadResult,
    InsolvencySnapshotState, InsuranceCancellationReceipt, InsuranceClaimReceipt,
    InsuranceEnrollmentReceipt, InsuranceQueryState, InsuranceReadResult,
    LeaseArrearPaymentReceipt, LeaseDepositLoanQuoteReceipt, LifeBudgetBandState,
    LifeBudgetSelectionState, LifeBudgetState, LifeEventChoiceReceipt, LifeEventsQueryState,
    LifeEventsReadResult, LifeFailureCode, LifeHouseholdState, LifeRateStatus, LifeResidenceState,
    LifeSnapshotState, LifeStore, LifeStoreResult, LivingCostMonthItemState, LivingCostMonthState,
    LoanDetailState, LoanExecutionReceipt, LoanInstallmentPageQuery, LoanInstallmentPageState,
    LoanPrepaymentReceipt, LoanProductCatalogState, LoanQuoteReceipt, MortgageQuoteReceipt,
    PayEssentialArrearCommand, PayLeaseArrearCommand, PrepareInsolvencyCaseCommand,
    PrepayLoanCommand, PropertyPurchaseReceipt, PropertySaleOrderCancellationReceipt,
    PropertySaleOrderListingReceipt, PropertySaleOrderPageQuery, PropertySaleOrderPageState,
    PropertyTaxEventPageQuery, PropertyTaxEventPageState, PurchasePropertyCommand,
    RealEstateDailyPreparationStore, RepricePropertySaleOrderCommand, ResidenceTenureKind,
    ResolveLifeEventCommand, StartHousingLeaseCommand, UpdateLifeBudgetCommand,
    UpdateLifeBudgetReceipt, WelfareApplicationReceipt, WelfareProgramsState,
};
use super::welfare::{
    apply_welfare_program, read_active_welfare_applications_in_tx, read_welfare_programs,
};
use crate::finance::{
    FinanceRules, LedgerAccountCode, LedgerPosting, LedgerSource, LedgerSourceKind,
    LedgerTransaction, LedgerTransactionDraft, ResourceId, RunId, RunPolicyContext,
    ScheduledSettlement, SettlementKind, SettlementSourceKind,
};
use crate::life::{
    CorporationRules, CurrentLivingCostCharge, EssentialArrearBalance, InsolvencyRules,
    InsuranceRules, LifeEventRules, LivingCostAllocationInput, LivingCostCategory,
    LivingCostCategoryCalculationInput, LivingCostMonthCalculationInput, LivingCostProration,
    LivingCostRules, PropertyRules, PropertyTaxRules, RealEstateRules, WelfareRules, YearMonth,
    create_corporation_rules, create_insolvency_rules, create_insurance_rules,
    create_life_event_rules, create_living_cost_rules, create_property_rules,
    create_property_tax_rules, create_real_estate_rules, create_welfare_rules,
};

const COMMAND_KIND_UPDATE_BUDGET: &str = "updateLifeBudget";
const COMMAND_KIND_PAY_ESSENTIAL_ARREAR: &str = "payEssentialArrear";
const COMMAND_KIND_CREATE_LOAN_QUOTE: &str = "createLoanQuote";
const COMMAND_KIND_CREATE_LEASE_DEPOSIT_LOAN_QUOTE: &str = "createLeaseDepositLoanQuote";
const COMMAND_KIND_EXECUTE_LOAN: &str = "executeLoan";
const COMMAND_KIND_PREPAY_LOAN: &str = "prepayLoan";
const LIVING_COST_PAYLOAD_VERSION: u8 = 1;
const LIVING_COST_PRORATION_SCALE: i64 = 377_580;
const MAX_ACTIVE_ARREARS: usize = 20;

#[derive(Clone)]
pub struct MySqlLifeStore {
    pool: MySqlPool,
    finance_rules: Arc<dyn FinanceRules>,
    real_estate_rules: Arc<dyn RealEstateRules>,
    property_rules: Arc<dyn PropertyRules>,
    property_tax_rules: Arc<dyn PropertyTaxRules>,
    life_event_rules: Arc<dyn LifeEventRules>,
    insurance_rules: Arc<dyn InsuranceRules>,
    insolvency_rules: Arc<dyn InsolvencyRules>,
    corporation_rules: Arc<dyn CorporationRules>,
    welfare_rules: Arc<dyn WelfareRules>,
}

pub fn create_mysql_life_store(
    pool: MySqlPool,
    finance_rules: Arc<dyn FinanceRules>,
) -> MySqlLifeStore {
    MySqlLifeStore {
        pool,
        finance_rules,
        real_estate_rules: create_real_estate_rules(),
        property_rules: create_property_rules(),
        property_tax_rules: create_property_tax_rules(),
        life_event_rules: create_life_event_rules(),
        insurance_rules: create_insurance_rules(),
        insolvency_rules: create_insolvency_rules(),
        corporation_rules: create_corporation_rules(),
        welfare_rules: create_welfare_rules(),
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LockedLifeSaveRow {
    id: u64,
    market_world_id: u64,
    policy_set_id: u64,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
    cash_krw: i64,
    debt_krw: i64,
    has_character: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct StoredLifeReceiptRow {
    command_kind: String,
    payload_sha256: String,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
    result_json: String,
    ledger_transaction_id: Option<u64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LifeScopeRow {
    household_id: u64,
    life_catalog_set_id: u64,
    availability: String,
    profile_id: Option<u64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LifeInitializationRow {
    life_catalog_set_id: u64,
    availability: String,
    profile_id: Option<u64>,
    legacy_dependent_age_years: u8,
    birth_date: Date,
    region_key: String,
    dependents: u32,
    debt_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ResidenceRow {
    id: u64,
    region_key: String,
    tenure_type: String,
    effective_from_game_day: u32,
    property_holding_id: Option<u64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct BudgetRow {
    id: u64,
    effective_from_game_day: u32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CategoryCalculationRow {
    category_id: u64,
    category_key: String,
    category_order: u8,
    essential: bool,
    base_amount_krw: i64,
    base_cpi_index: u64,
    band_id: u64,
    budget_factor_ppm: u32,
    region_factor_ppm: u32,
    tenure_replacement_factor_ppm: u32,
    prior_remainder_numerator: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ActiveMemberRow {
    id: u64,
    member_role: String,
    birth_date: Date,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct MemberFactorRow {
    member_role: String,
    minimum_age_years: u8,
    maximum_age_years_exclusive: Option<u8>,
    category_id: u64,
    marginal_factor_ppm: u32,
}

#[derive(Debug, Clone)]
struct PinnedCategoryCalculation {
    row: CategoryCalculationRow,
    category: LivingCostCategory,
    household_factor_ppm: i64,
    gross_krw: i64,
    next_remainder_numerator: i128,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct PendingMonthRow {
    id: u64,
    household_id: u64,
    year_month: Date,
    due_game_day: u32,
    status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PendingMonthItemRow {
    id: u64,
    category_key: String,
    essential: bool,
    gross_amount_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct SettlementEnvelopeRow {
    due_game_day: u32,
    kind: String,
    payload_json: String,
    source_kind: String,
    source_id: String,
    occurrence: u64,
    status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ActiveArrearRow {
    id: u64,
    due_year_month: Date,
    category_key: String,
    original_amount_krw: i64,
    outstanding_amount_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct HouseholdSummaryRow {
    id: u64,
    member_count: i64,
    dependent_count: i64,
    tax_dependent_eligible_count: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct BudgetBandRow {
    id: u64,
    band_key: String,
    display_name: String,
    factor_ppm: u32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct BudgetSelectionRow {
    category_key: String,
    band_id: u64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LivingCostMonthReadRow {
    id: u64,
    profile_id: u64,
    profile_key: String,
    year_month: Date,
    cpi_index: u64,
    activation_game_day: u32,
    settlement_game_day: u32,
    proration_scale: u32,
    proration_units: u32,
    days_in_month: u8,
    status: String,
    total_gross_krw: i64,
    total_paid_krw: i64,
    total_arrear_krw: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LivingCostMonthItemReadRow {
    category_key: String,
    band_id: u64,
    essential: bool,
    base_amount_krw: i64,
    base_cpi_index: u64,
    region_factor_ppm: u32,
    household_factor_ppm: u64,
    budget_factor_ppm: u32,
    tenure_replacement_factor_ppm: u32,
    gross_amount_krw: i64,
    paid_amount_krw: i64,
    arrear_amount_krw: i64,
}

struct ActiveArrearWindow {
    items: Vec<EssentialArrearState>,
    has_more: bool,
    total_krw: i64,
}

#[derive(Debug, Clone, Copy)]
enum LifePostingReference {
    None,
    LivingCostMonth(u64),
    EssentialArrear(u64),
}

struct ArrearPaymentDraft<'a> {
    save_id: u64,
    run_revision: u32,
    arrear_id: u64,
    amount_krw: i64,
    game_day: u32,
    payment_kind: &'a str,
    command_id: Option<&'a str>,
}

struct ArrearPaymentApplication {
    save_id: u64,
    run_revision: u32,
    payment_id: u64,
    arrear_id: u64,
    amount_krw: i64,
    outstanding_before_krw: i64,
    game_day: u32,
    ledger_transaction_id: u64,
}

#[async_trait]
impl LifeStore for MySqlLifeStore {
    async fn corporation_templates(
        &self,
        user_id: u64,
    ) -> Result<CorporationReadResult<CorporationTemplatesState>> {
        read_corporation_templates(&self.pool, user_id).await
    }

    async fn create_corporation(
        &self,
        user_id: u64,
        command: &CreateCorporationCommand,
    ) -> Result<LifeStoreResult<CorporationReceipt>> {
        create_corporation(
            &self.pool,
            self.finance_rules.as_ref(),
            self.corporation_rules.as_ref(),
            user_id,
            command,
        )
        .await
    }

    async fn corporation_detail(
        &self,
        user_id: u64,
        corporation_id: ResourceId,
    ) -> Result<CorporationReadResult<CorporationSummaryState>> {
        read_corporation_detail(&self.pool, user_id, corporation_id).await
    }

    async fn insolvency_overview(
        &self,
        user_id: u64,
    ) -> Result<InsolvencyReadResult<InsolvencySnapshotState>> {
        read_insolvency_overview(&self.pool, self.insolvency_rules.as_ref(), user_id).await
    }

    async fn prepare_insolvency_case(
        &self,
        user_id: u64,
        command: &PrepareInsolvencyCaseCommand,
    ) -> Result<LifeStoreResult<InsolvencyCaseReceipt>> {
        prepare_insolvency_case(&self.pool, self.insolvency_rules.as_ref(), user_id, command).await
    }

    async fn act_on_insolvency_case(
        &self,
        user_id: u64,
        command: &ActOnInsolvencyCaseCommand,
    ) -> Result<LifeStoreResult<InsolvencyCaseReceipt>> {
        act_on_insolvency_case(
            &self.pool,
            self.finance_rules.as_ref(),
            self.insolvency_rules.as_ref(),
            user_id,
            command,
        )
        .await
    }

    async fn insolvency_case_detail(
        &self,
        user_id: u64,
        case_id: ResourceId,
    ) -> Result<InsolvencyReadResult<InsolvencyCaseDetailState>> {
        read_case_detail(&self.pool, user_id, case_id).await
    }

    async fn insolvency_claims(
        &self,
        user_id: u64,
        case_id: ResourceId,
        cursor: Option<String>,
    ) -> Result<InsolvencyReadResult<InsolvencyClaimPageState>> {
        read_claim_page(&self.pool, user_id, case_id, cursor).await
    }

    async fn insolvency_liquidations(
        &self,
        user_id: u64,
        case_id: ResourceId,
        cursor: Option<String>,
    ) -> Result<InsolvencyReadResult<InsolvencyLiquidationPageState>> {
        read_liquidation_page(&self.pool, user_id, case_id, cursor).await
    }

    async fn life_events(
        &self,
        user_id: u64,
        query: LifeEventsQueryState,
    ) -> Result<LifeEventsReadResult> {
        read_life_events(&self.pool, self.life_event_rules.as_ref(), user_id, query).await
    }

    async fn resolve_life_event(
        &self,
        user_id: u64,
        command: &ResolveLifeEventCommand,
    ) -> Result<LifeStoreResult<LifeEventChoiceReceipt>> {
        resolve_life_event(
            &self.pool,
            self.finance_rules.as_ref(),
            self.life_event_rules.as_ref(),
            self.insurance_rules.as_ref(),
            user_id,
            command,
        )
        .await
    }

    async fn insurance(
        &self,
        user_id: u64,
        query: InsuranceQueryState,
    ) -> Result<InsuranceReadResult> {
        read_insurance(&self.pool, self.insurance_rules.as_ref(), user_id, query).await
    }

    async fn enroll_insurance_contract(
        &self,
        user_id: u64,
        command: &EnrollInsuranceContractCommand,
    ) -> Result<LifeStoreResult<InsuranceEnrollmentReceipt>> {
        enroll_insurance_contract(
            &self.pool,
            self.finance_rules.as_ref(),
            self.insurance_rules.as_ref(),
            user_id,
            command,
        )
        .await
    }

    async fn cancel_insurance_contract(
        &self,
        user_id: u64,
        command: &CancelInsuranceContractCommand,
    ) -> Result<LifeStoreResult<InsuranceCancellationReceipt>> {
        cancel_insurance_contract(&self.pool, self.insurance_rules.as_ref(), user_id, command).await
    }

    async fn file_insurance_claim(
        &self,
        user_id: u64,
        command: &FileInsuranceClaimCommand,
    ) -> Result<LifeStoreResult<InsuranceClaimReceipt>> {
        file_insurance_claim(
            &self.pool,
            self.finance_rules.as_ref(),
            self.insurance_rules.as_ref(),
            user_id,
            command,
        )
        .await
    }

    async fn welfare_programs(&self, user_id: u64) -> Result<Option<WelfareProgramsState>> {
        read_welfare_programs(&self.pool, self.welfare_rules.as_ref(), user_id).await
    }

    async fn apply_welfare_program(
        &self,
        user_id: u64,
        command: &ApplyWelfareProgramCommand,
    ) -> Result<LifeStoreResult<WelfareApplicationReceipt>> {
        apply_welfare_program(&self.pool, self.welfare_rules.as_ref(), user_id, command).await
    }

    async fn housing_listings(
        &self,
        user_id: u64,
        query: HousingListingsQueryState,
    ) -> Result<Option<HousingListingsState>> {
        read_housing_listings(&self.pool, self.real_estate_rules.as_ref(), user_id, query).await
    }

    async fn housing_lease_current(
        &self,
        user_id: u64,
    ) -> Result<Option<HousingLeaseCurrentState>> {
        read_current_housing_lease(&self.pool, user_id).await
    }

    async fn start_housing_lease(
        &self,
        user_id: u64,
        command: &StartHousingLeaseCommand,
    ) -> Result<LifeStoreResult<HousingLeaseMoveReceipt>> {
        start_housing_lease_command(
            &self.pool,
            self.finance_rules.as_ref(),
            self.real_estate_rules.as_ref(),
            user_id,
            command,
        )
        .await
    }

    async fn housing_property_holdings(
        &self,
        user_id: u64,
    ) -> Result<Option<HousingPropertyHoldingsState>> {
        read_property_holdings(&self.pool, user_id).await
    }

    async fn quote_mortgage(
        &self,
        user_id: u64,
        command: &CreateMortgageQuoteCommand,
    ) -> Result<LifeStoreResult<MortgageQuoteReceipt>> {
        quote_mortgage_command(
            &self.pool,
            self.real_estate_rules.as_ref(),
            self.property_rules.as_ref(),
            user_id,
            command,
        )
        .await
    }

    async fn purchase_property(
        &self,
        user_id: u64,
        command: &PurchasePropertyCommand,
    ) -> Result<LifeStoreResult<PropertyPurchaseReceipt>> {
        purchase_property_command(
            &self.pool,
            self.finance_rules.as_ref(),
            self.real_estate_rules.as_ref(),
            self.property_rules.as_ref(),
            self.property_tax_rules.as_ref(),
            user_id,
            command,
        )
        .await
    }

    async fn create_property_sale_order(
        &self,
        user_id: u64,
        command: &CreatePropertySaleOrderCommand,
    ) -> Result<LifeStoreResult<PropertySaleOrderListingReceipt>> {
        create_property_sale_order_command(
            &self.pool,
            self.real_estate_rules.as_ref(),
            self.property_rules.as_ref(),
            user_id,
            command,
        )
        .await
    }

    async fn reprice_property_sale_order(
        &self,
        user_id: u64,
        command: &RepricePropertySaleOrderCommand,
    ) -> Result<LifeStoreResult<PropertySaleOrderListingReceipt>> {
        reprice_property_sale_order_command(
            &self.pool,
            self.real_estate_rules.as_ref(),
            self.property_rules.as_ref(),
            user_id,
            command,
        )
        .await
    }

    async fn cancel_property_sale_order(
        &self,
        user_id: u64,
        command: &CancelPropertySaleOrderCommand,
    ) -> Result<LifeStoreResult<PropertySaleOrderCancellationReceipt>> {
        cancel_property_sale_order_command(&self.pool, user_id, command).await
    }

    async fn property_sale_orders(
        &self,
        user_id: u64,
        query: PropertySaleOrderPageQuery,
    ) -> Result<Option<PropertySaleOrderPageState>> {
        read_property_sale_orders(&self.pool, user_id, query).await
    }

    async fn property_tax_events(
        &self,
        user_id: u64,
        holding_id: ResourceId,
        query: PropertyTaxEventPageQuery,
    ) -> Result<Option<PropertyTaxEventPageState>> {
        super::property_tax::read_property_tax_events(&self.pool, user_id, holding_id, query).await
    }

    async fn loan_products(&self, user_id: u64) -> Result<LoanProductCatalogState> {
        let mut tx = self.pool.begin().await?;
        let state = read_loan_product_catalog_in_tx(&mut tx, user_id).await?;
        tx.commit().await?;
        Ok(state)
    }

    async fn loan_detail(
        &self,
        user_id: u64,
        loan_id: ResourceId,
    ) -> Result<Option<LoanDetailState>> {
        let mut tx = self.pool.begin().await?;
        let state = read_loan_detail_in_tx(&mut tx, user_id, loan_id).await?;
        tx.commit().await?;
        Ok(state)
    }

    async fn loan_installments(
        &self,
        user_id: u64,
        loan_id: ResourceId,
        query: LoanInstallmentPageQuery,
    ) -> Result<Option<LoanInstallmentPageState>> {
        let mut tx = self.pool.begin().await?;
        let state = read_loan_installment_page_in_tx(&mut tx, user_id, loan_id, query).await?;
        tx.commit().await?;
        Ok(state)
    }

    async fn credit(&self, user_id: u64) -> Result<CreditOverviewState> {
        let mut tx = self.pool.begin().await?;
        let save: Option<(u64, u32)> =
            sqlx::query_as("SELECT id, run_revision FROM save WHERE user_id = ?")
                .bind(user_id)
                .fetch_optional(&mut *tx)
                .await?;
        let state = match save {
            Some((save_id, run_revision)) => {
                read_credit_and_loan_snapshot_in_tx(&mut tx, save_id, run_revision).await?
            }
            None => CreditOverviewState {
                credit_band: None,
                credit_reasons: vec![CreditReasonState::ModelUnavailable],
                active_loans: Vec::new(),
                next_loan_installment: None,
                total_loan_balance_krw: 0,
            },
        };
        tx.commit().await?;
        Ok(state)
    }

    async fn quote_loan(
        &self,
        user_id: u64,
        command: &CreateLoanQuoteCommand,
    ) -> Result<LifeStoreResult<LoanQuoteReceipt>> {
        quote_loan_command(&self.pool, user_id, command).await
    }

    async fn quote_lease_deposit_loan(
        &self,
        user_id: u64,
        command: &CreateLeaseDepositLoanQuoteCommand,
    ) -> Result<LifeStoreResult<LeaseDepositLoanQuoteReceipt>> {
        quote_lease_deposit_loan_command(
            &self.pool,
            self.real_estate_rules.as_ref(),
            user_id,
            command,
        )
        .await
    }

    async fn execute_loan(
        &self,
        user_id: u64,
        command: &ExecuteLoanCommand,
    ) -> Result<LifeStoreResult<LoanExecutionReceipt>> {
        execute_loan_command(&self.pool, self.finance_rules.as_ref(), user_id, command).await
    }

    async fn prepay_loan(
        &self,
        user_id: u64,
        command: &PrepayLoanCommand,
    ) -> Result<LifeStoreResult<LoanPrepaymentReceipt>> {
        prepay_loan_command(&self.pool, self.finance_rules.as_ref(), user_id, command).await
    }

    async fn budget(&self, user_id: u64) -> Result<LifeBudgetState> {
        let mut tx = self.pool.begin().await?;
        let save_id: u64 = sqlx::query_scalar("SELECT id FROM save WHERE user_id = ?")
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await?
            .context("life budget requires an active save")?;
        let state = read_life_budget_in_tx(&mut tx, save_id).await?;
        tx.commit().await?;
        Ok(state)
    }

    async fn update_budget(
        &self,
        user_id: u64,
        command: &UpdateLifeBudgetCommand,
    ) -> Result<LifeStoreResult<UpdateLifeBudgetReceipt>> {
        update_budget_command(&self.pool, user_id, command).await
    }

    async fn pay_essential_arrear(
        &self,
        user_id: u64,
        command: &PayEssentialArrearCommand,
    ) -> Result<LifeStoreResult<EssentialArrearPaymentReceipt>> {
        pay_essential_arrear_command(&self.pool, self.finance_rules.as_ref(), user_id, command)
            .await
    }

    async fn pay_lease_arrear(
        &self,
        user_id: u64,
        command: &PayLeaseArrearCommand,
    ) -> Result<LifeStoreResult<LeaseArrearPaymentReceipt>> {
        pay_lease_arrear_command(&self.pool, self.finance_rules.as_ref(), user_id, command).await
    }
}

#[async_trait]
impl RealEstateDailyPreparationStore for MySqlLifeStore {
    async fn ensure_property_market_for_user(
        &self,
        user_id: u64,
        target_game_day: u32,
    ) -> Result<()> {
        prepare_property_daily_for_target(
            &self.pool,
            self.real_estate_rules.as_ref(),
            user_id,
            target_game_day,
        )
        .await
    }
}

async fn lock_life_save(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
) -> Result<Option<LockedLifeSaveRow>> {
    sqlx::query_as(
        "SELECT save.id, save.market_world_id, save.policy_set_id,
                save.run_revision, save.state_revision, save.game_day,
                save.cash_krw, save.debt_krw,
                EXISTS(SELECT 1 FROM `character` WHERE `character`.save_id = save.id)
                    AS has_character
         FROM save WHERE save.user_id = ? FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock the life-command save")
}

async fn read_life_scope(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<Option<LifeScopeRow>> {
    sqlx::query_as(
        "SELECT household.id AS household_id, household.life_catalog_set_id,
                component.availability, profile.id AS profile_id
         FROM household
         INNER JOIN life_catalog_set AS catalog ON catalog.id = household.life_catalog_set_id
         INNER JOIN life_component_version AS component
           ON component.id = catalog.living_cost_component_version_id
         LEFT JOIN cost_of_living_profile AS profile
           ON profile.life_component_version_id = component.id
         WHERE household.save_id = ? AND household.run_revision = ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read the run's life catalog scope")
}

async fn lock_household(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    household_id: u64,
) -> Result<()> {
    sqlx::query(
        "SELECT id FROM household
         WHERE save_id = ? AND run_revision = ? AND id = ? FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(household_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(())
}

fn has_current_cursor(current: &LockedLifeSaveRow, command: crate::finance::CommandCursor) -> bool {
    current.run_revision == command.expected_run_revision
        && current.state_revision == command.expected_state_revision
        && current.game_day == command.expected_game_day
}

async fn read_stored_life_receipt(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    command_id: &str,
) -> Result<StoredLifeReceiptRow> {
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
    .context("life command identity has no final receipt")
}

async fn quote_loan_command(
    pool: &MySqlPool,
    user_id: u64,
    command: &CreateLoanQuoteCommand,
) -> Result<LifeStoreResult<LoanQuoteReceipt>> {
    let fingerprint = loan_quote_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(current) = lock_life_save(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_CREATE_LOAN_QUOTE,
        payload_sha256: &fingerprint,
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
                read_stored_life_receipt(&mut tx, current.id, command.command_id.as_str()).await?;
            let mut receipt: LoanQuoteReceipt = serde_json::from_str(&row.result_json)
                .context("stored loan-quote receipt is invalid")?;
            ensure!(
                row.command_kind == COMMAND_KIND_CREATE_LOAN_QUOTE
                    && row.payload_sha256 == fingerprint
                    && row.run_revision == command.cursor.expected_run_revision
                    && row.state_revision == command.cursor.expected_state_revision
                    && row.game_day == command.cursor.expected_game_day
                    && row.ledger_transaction_id.is_none()
                    && receipt.command_id == command.command_id
                    && receipt.product_version_id == command.product_version_id
                    && receipt.requested_principal_krw == command.principal_krw
                    && receipt.created_game_day == command.cursor.expected_game_day
                    && receipt.expires_game_day == receipt.created_game_day
                    && !receipt.replayed,
                "stored loan-quote receipt disagrees with its command"
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
    if !has_current_cursor(&current, command.cursor) {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy));
    }
    validate_debt_projection_in_tx(&mut tx, current.id, current.run_revision).await?;
    let receipt = match create_loan_quote_in_tx(&mut tx, user_id, command, &fingerprint).await? {
        LoanQuoteCreation::Applied(receipt) => *receipt,
        LoanQuoteCreation::Rejected(code) => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(code));
        }
    };
    write_command_identity(&mut tx, current.id, &identity).await?;
    write_game_command_receipt(
        &mut tx,
        GameCommandReceiptWrite {
            save_id: current.id,
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_CREATE_LOAN_QUOTE,
            payload_sha256: &fingerprint,
            market_world_id: current.market_world_id,
            committed_cursor: GameCommandCursor {
                run_revision: current.run_revision,
                state_revision: current.state_revision,
                game_day: current.game_day,
            },
            result: &receipt,
            ledger_transaction_id: None,
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

async fn quote_lease_deposit_loan_command(
    pool: &MySqlPool,
    real_estate_rules: &dyn RealEstateRules,
    user_id: u64,
    command: &CreateLeaseDepositLoanQuoteCommand,
) -> Result<LifeStoreResult<LeaseDepositLoanQuoteReceipt>> {
    if let Err(error) = prepare_current_housing_catalogs(pool, real_estate_rules, user_id).await {
        if is_retryable_database_error(&error) {
            return Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy));
        }
        return Err(error);
    }
    let fingerprint = lease_deposit_loan_quote_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(current) = lock_life_save(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_CREATE_LEASE_DEPOSIT_LOAN_QUOTE,
        payload_sha256: &fingerprint,
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
                read_stored_life_receipt(&mut tx, current.id, command.command_id.as_str()).await?;
            let mut receipt: LeaseDepositLoanQuoteReceipt = serde_json::from_str(&row.result_json)
                .context("stored lease-deposit quote receipt is invalid")?;
            ensure!(
                row.command_kind == COMMAND_KIND_CREATE_LEASE_DEPOSIT_LOAN_QUOTE
                    && row.payload_sha256 == fingerprint
                    && row.run_revision == command.cursor.expected_run_revision
                    && row.state_revision == command.cursor.expected_state_revision
                    && row.game_day == command.cursor.expected_game_day
                    && row.ledger_transaction_id.is_none()
                    && receipt.command_id == command.command_id
                    && receipt.listing_id == command.listing_id
                    && receipt.offer_kind == command.offer_kind
                    && receipt.product_version_id == command.product_version_id
                    && receipt.requested_principal_krw == command.principal_krw
                    && receipt.created_game_day == command.cursor.expected_game_day
                    && receipt.expires_game_day == receipt.created_game_day
                    && !receipt.regulatory_dsr_applied
                    && !receipt.replayed,
                "stored lease-deposit quote receipt disagrees with its command"
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
    if !has_current_cursor(&current, command.cursor) {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy));
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
    validate_debt_projection_in_tx(&mut tx, current.id, current.run_revision).await?;
    let receipt =
        match create_lease_deposit_loan_quote_in_tx(&mut tx, user_id, command, &fingerprint).await?
        {
            LeaseDepositLoanQuoteCreation::Applied(receipt) => *receipt,
            LeaseDepositLoanQuoteCreation::Rejected(code) => {
                tx.commit().await?;
                return Ok(LifeStoreResult::Rejected(code));
            }
        };
    write_command_identity(&mut tx, current.id, &identity).await?;
    write_game_command_receipt(
        &mut tx,
        GameCommandReceiptWrite {
            save_id: current.id,
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_CREATE_LEASE_DEPOSIT_LOAN_QUOTE,
            payload_sha256: &fingerprint,
            market_world_id: current.market_world_id,
            committed_cursor: GameCommandCursor {
                run_revision: current.run_revision,
                state_revision: current.state_revision,
                game_day: current.game_day,
            },
            result: &receipt,
            ledger_transaction_id: None,
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

async fn execute_loan_command(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    user_id: u64,
    command: &ExecuteLoanCommand,
) -> Result<LifeStoreResult<LoanExecutionReceipt>> {
    let fingerprint = loan_execution_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(current) = lock_life_save(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_EXECUTE_LOAN,
        payload_sha256: &fingerprint,
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
                read_stored_life_receipt(&mut tx, current.id, command.command_id.as_str()).await?;
            let mut receipt: LoanExecutionReceipt = serde_json::from_str(&row.result_json)
                .context("stored loan-execution receipt is invalid")?;
            let expected_state_revision = command
                .cursor
                .expected_state_revision
                .checked_add(1)
                .context("stored loan-execution state revision overflowed")?;
            ensure!(
                row.command_kind == COMMAND_KIND_EXECUTE_LOAN
                    && row.payload_sha256 == fingerprint
                    && row.run_revision == command.cursor.expected_run_revision
                    && row.state_revision == expected_state_revision
                    && row.game_day == command.cursor.expected_game_day
                    && row.ledger_transaction_id.is_some()
                    && receipt.command_id == command.command_id
                    && receipt.quote_id == command.quote_id
                    && receipt.activated_game_day == command.cursor.expected_game_day
                    && !receipt.replayed,
                "stored loan-execution receipt disagrees with its command"
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
    if !has_current_cursor(&current, command.cursor) {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy));
    }
    validate_debt_projection_in_tx(&mut tx, current.id, current.run_revision).await?;
    let (receipt, ledger_transaction_id) =
        match execute_loan_in_tx(&mut tx, finance_rules, user_id, current.id, command).await? {
            LoanExecutionCreation::Applied {
                receipt,
                ledger_transaction_id,
            } => (*receipt, ledger_transaction_id),
            LoanExecutionCreation::Rejected(code) => {
                tx.commit().await?;
                return Ok(LifeStoreResult::Rejected(code));
            }
        };
    let cash_krw = current
        .cash_krw
        .checked_add(receipt.principal_krw)
        .context("loan execution wallet update overflowed")?;
    let debt_krw = current
        .debt_krw
        .checked_add(receipt.principal_krw)
        .context("loan execution debt projection overflowed")?;
    let committed_state_revision = current
        .state_revision
        .checked_add(1)
        .context("loan execution state revision overflowed")?;
    write_command_identity(&mut tx, current.id, &identity).await?;
    update_save_after_life_command(
        &mut tx,
        &current,
        committed_state_revision,
        cash_krw,
        debt_krw,
    )
    .await?;
    validate_debt_projection_in_tx(&mut tx, current.id, current.run_revision).await?;
    write_game_command_receipt(
        &mut tx,
        GameCommandReceiptWrite {
            save_id: current.id,
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_EXECUTE_LOAN,
            payload_sha256: &fingerprint,
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

async fn prepay_loan_command(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    user_id: u64,
    command: &PrepayLoanCommand,
) -> Result<LifeStoreResult<LoanPrepaymentReceipt>> {
    let fingerprint = loan_prepayment_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(current) = lock_life_save(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_PREPAY_LOAN,
        payload_sha256: &fingerprint,
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
                read_stored_life_receipt(&mut tx, current.id, command.command_id.as_str()).await?;
            let mut receipt: LoanPrepaymentReceipt = serde_json::from_str(&row.result_json)
                .context("stored loan-prepayment receipt is invalid")?;
            let expected_state_revision = command
                .cursor
                .expected_state_revision
                .checked_add(1)
                .context("stored loan-prepayment state revision overflowed")?;
            ensure!(
                row.command_kind == COMMAND_KIND_PREPAY_LOAN
                    && row.payload_sha256 == fingerprint
                    && row.run_revision == command.cursor.expected_run_revision
                    && row.state_revision == expected_state_revision
                    && row.game_day == command.cursor.expected_game_day
                    && row.ledger_transaction_id.is_some()
                    && receipt.command_id == command.command_id
                    && receipt.loan_id == command.loan_id
                    && receipt.principal_krw == command.principal_krw
                    && receipt.applied_game_day == command.cursor.expected_game_day
                    && !receipt.replayed,
                "stored loan-prepayment receipt disagrees with its command"
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
    if !has_current_cursor(&current, command.cursor) {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy));
    }
    validate_debt_projection_in_tx(&mut tx, current.id, current.run_revision).await?;
    let (receipt, ledger_transaction_id) = match prepay_loan_in_tx(
        &mut tx,
        finance_rules,
        current.id,
        current.run_revision,
        current.cash_krw,
        command,
    )
    .await?
    {
        LoanPrepaymentCreation::Applied {
            receipt,
            ledger_transaction_id,
        } => (*receipt, ledger_transaction_id),
        LoanPrepaymentCreation::Rejected(code) => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(code));
        }
    };
    let cash_krw = current
        .cash_krw
        .checked_sub(receipt.total_debited_krw)
        .context("loan prepayment wallet update overflowed")?;
    let debt_krw = current
        .debt_krw
        .checked_sub(receipt.principal_krw)
        .context("loan prepayment debt projection overflowed")?;
    ensure!(
        cash_krw >= 0 && debt_krw >= 0,
        "loan prepayment produced a negative save balance"
    );
    let committed_state_revision = current
        .state_revision
        .checked_add(1)
        .context("loan prepayment state revision overflowed")?;
    write_command_identity(&mut tx, current.id, &identity).await?;
    update_save_after_life_command(
        &mut tx,
        &current,
        committed_state_revision,
        cash_krw,
        debt_krw,
    )
    .await?;
    validate_debt_projection_in_tx(&mut tx, current.id, current.run_revision).await?;
    write_game_command_receipt(
        &mut tx,
        GameCommandReceiptWrite {
            save_id: current.id,
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_PREPAY_LOAN,
            payload_sha256: &fingerprint,
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

async fn update_budget_command(
    pool: &MySqlPool,
    user_id: u64,
    command: &UpdateLifeBudgetCommand,
) -> Result<LifeStoreResult<UpdateLifeBudgetReceipt>> {
    let fingerprint = update_budget_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(current) = lock_life_save(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_UPDATE_BUDGET,
        payload_sha256: &fingerprint,
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
                read_stored_life_receipt(&mut tx, current.id, command.command_id.as_str()).await?;
            let mut receipt: UpdateLifeBudgetReceipt = serde_json::from_str(&row.result_json)
                .context("stored life-budget receipt is invalid")?;
            let expected_state_revision = command
                .cursor
                .expected_state_revision
                .checked_add(1)
                .context("stored life-budget state revision overflowed")?;
            ensure!(
                row.command_kind == COMMAND_KIND_UPDATE_BUDGET
                    && row.payload_sha256 == fingerprint
                    && row.run_revision == command.cursor.expected_run_revision
                    && row.state_revision == expected_state_revision
                    && row.game_day == command.cursor.expected_game_day
                    && row.ledger_transaction_id.is_none()
                    && receipt.command_id == command.command_id
                    && receipt.applied_game_day == command.cursor.expected_game_day
                    && !receipt.replayed,
                "stored life-budget receipt disagrees with its command"
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
    if !has_current_cursor(&current, command.cursor) {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy));
    }
    let Some(scope) = read_life_scope(&mut tx, current.id, current.run_revision).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::RateUnavailable));
    };
    let Some(profile_id) = scope.profile_id.filter(|_| scope.availability == "active") else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::RateUnavailable));
    };
    lock_household(
        &mut tx,
        current.id,
        current.run_revision,
        scope.household_id,
    )
    .await?;
    validate_debt_projection_in_tx(&mut tx, current.id, current.run_revision).await?;
    let canonical = match validate_and_sort_budget_selections(&command.selections) {
        Ok(selections) => selections,
        Err(_) => {
            tx.commit().await?;
            return Ok(LifeStoreResult::Rejected(LifeFailureCode::InvalidCommand));
        }
    };
    let allowed_band_ids: HashSet<u64> = sqlx::query_scalar(
        "SELECT id FROM living_cost_budget_band
         WHERE cost_of_living_profile_id = ? ORDER BY band_order",
    )
    .bind(profile_id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    if canonical
        .iter()
        .any(|selection| !allowed_band_ids.contains(&selection.band_id.get()))
    {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::InvalidCommand));
    }
    let category_rows: Vec<(u64, String)> = sqlx::query_as(
        "SELECT id, category_key FROM living_cost_category
         WHERE cost_of_living_profile_id = ? ORDER BY category_order",
    )
    .bind(profile_id)
    .fetch_all(&mut *tx)
    .await?;
    ensure!(
        category_rows.len() == LivingCostCategory::ALL.len(),
        "active living-cost profile does not contain nine categories"
    );
    let active_budget_effective_day: u32 = sqlx::query_scalar(
        "SELECT effective_from_game_day FROM household_budget
         WHERE save_id = ? AND run_revision = ? AND household_id = ?
           AND sealed_at IS NOT NULL AND effective_to_game_day IS NULL",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(scope.household_id)
    .fetch_one(&mut *tx)
    .await?;
    if active_budget_effective_day > current.game_day {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy));
    }
    write_command_identity(&mut tx, current.id, &identity).await?;
    let effective_game_day = current
        .game_day
        .checked_add(1)
        .context("life-budget effective day overflowed")?;
    let closed = sqlx::query(
        "UPDATE household_budget SET effective_to_game_day = ?
         WHERE save_id = ? AND run_revision = ? AND household_id = ?
           AND sealed_at IS NOT NULL AND effective_to_game_day IS NULL",
    )
    .bind(effective_game_day)
    .bind(current.id)
    .bind(current.run_revision)
    .bind(scope.household_id)
    .execute(&mut *tx)
    .await?;
    ensure!(closed.rows_affected() == 1, "active life budget is missing");
    let inserted = sqlx::query(
        "INSERT INTO household_budget
             (save_id, run_revision, household_id, cost_of_living_profile_id,
              effective_from_game_day)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(scope.household_id)
    .bind(profile_id)
    .bind(effective_game_day)
    .execute(&mut *tx)
    .await?;
    let budget_id = inserted.last_insert_id();
    for (category_id, category_key) in category_rows {
        let category: LivingCostCategory = category_key
            .parse()
            .context("stored living-cost category is invalid")?;
        let selection = canonical
            .iter()
            .find(|selection| selection.category == category)
            .context("validated life budget lost a category")?;
        sqlx::query(
            "INSERT INTO household_budget_selection
                 (cost_of_living_profile_id, household_budget_id,
                  living_cost_category_id, living_cost_budget_band_id)
             VALUES (?, ?, ?, ?)",
        )
        .bind(profile_id)
        .bind(budget_id)
        .bind(category_id)
        .bind(selection.band_id.get())
        .execute(&mut *tx)
        .await?;
    }
    let sealed =
        sqlx::query("UPDATE household_budget SET sealed_at = CURRENT_TIMESTAMP(3) WHERE id = ?")
            .bind(budget_id)
            .execute(&mut *tx)
            .await?;
    ensure!(
        sealed.rows_affected() == 1,
        "new life budget was not sealed"
    );
    let committed_state_revision = current
        .state_revision
        .checked_add(1)
        .context("life-budget state revision overflowed")?;
    update_save_after_life_command(
        &mut tx,
        &current,
        committed_state_revision,
        current.cash_krw,
        current.debt_krw,
    )
    .await?;
    validate_debt_projection_in_tx(&mut tx, current.id, current.run_revision).await?;
    let receipt = UpdateLifeBudgetReceipt {
        command_id: command.command_id.clone(),
        applied_game_day: current.game_day,
        selections: canonical,
        replayed: false,
    };
    write_game_command_receipt(
        &mut tx,
        GameCommandReceiptWrite {
            save_id: current.id,
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_UPDATE_BUDGET,
            payload_sha256: &fingerprint,
            market_world_id: current.market_world_id,
            committed_cursor: GameCommandCursor {
                run_revision: current.run_revision,
                state_revision: committed_state_revision,
                game_day: current.game_day,
            },
            result: &receipt,
            ledger_transaction_id: None,
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

async fn pay_essential_arrear_command(
    pool: &MySqlPool,
    finance_rules: &dyn FinanceRules,
    user_id: u64,
    command: &PayEssentialArrearCommand,
) -> Result<LifeStoreResult<EssentialArrearPaymentReceipt>> {
    let fingerprint = pay_arrear_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(current) = lock_life_save(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::CharacterRequired,
        ));
    };
    let identity = CommandIdentitySpec {
        command_id: &command.command_id,
        command_kind: COMMAND_KIND_PAY_ESSENTIAL_ARREAR,
        payload_sha256: &fingerprint,
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
                read_stored_life_receipt(&mut tx, current.id, command.command_id.as_str()).await?;
            let mut receipt: EssentialArrearPaymentReceipt = serde_json::from_str(&row.result_json)
                .context("stored essential-arrear receipt is invalid")?;
            let expected_state_revision = command
                .cursor
                .expected_state_revision
                .checked_add(1)
                .context("stored essential-arrear state revision overflowed")?;
            ensure!(
                row.command_kind == COMMAND_KIND_PAY_ESSENTIAL_ARREAR
                    && row.payload_sha256 == fingerprint
                    && row.run_revision == command.cursor.expected_run_revision
                    && row.state_revision == expected_state_revision
                    && row.game_day == command.cursor.expected_game_day
                    && row.ledger_transaction_id.is_some()
                    && receipt.command_id == command.command_id
                    && receipt.arrear_id == command.arrear_id
                    && receipt.paid_krw == command.amount_krw
                    && !receipt.replayed,
                "stored essential-arrear receipt disagrees with its command"
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
    if !has_current_cursor(&current, command.cursor) {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::Busy));
    }
    let Some(scope) = read_life_scope(&mut tx, current.id, current.run_revision).await? else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::RateUnavailable));
    };
    if scope.availability != "active" || scope.profile_id.is_none() {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::RateUnavailable));
    }
    lock_household(
        &mut tx,
        current.id,
        current.run_revision,
        scope.household_id,
    )
    .await?;
    validate_debt_projection_in_tx(&mut tx, current.id, current.run_revision).await?;
    if command.amount_krw <= 0 {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::InvalidCommand));
    }
    let arrear: Option<ActiveArrearRow> = sqlx::query_as(
        "SELECT id, due_year_month, category_key, original_amount_krw,
                outstanding_amount_krw
         FROM essential_arrear
         WHERE save_id = ? AND run_revision = ? AND household_id = ?
           AND id = ? AND status = 'active' FOR UPDATE",
    )
    .bind(current.id)
    .bind(current.run_revision)
    .bind(scope.household_id)
    .bind(command.arrear_id.get())
    .fetch_optional(&mut *tx)
    .await?;
    let Some(arrear) = arrear else {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::InvalidCommand));
    };
    if command.amount_krw > arrear.outstanding_amount_krw {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(LifeFailureCode::InvalidCommand));
    }
    if command.amount_krw > current.cash_krw {
        tx.commit().await?;
        return Ok(LifeStoreResult::Rejected(
            LifeFailureCode::InsufficientWalletCash,
        ));
    }
    write_command_identity(&mut tx, current.id, &identity).await?;
    let payment_id = insert_arrear_payment(
        &mut tx,
        ArrearPaymentDraft {
            save_id: current.id,
            run_revision: current.run_revision,
            arrear_id: arrear.id,
            amount_krw: command.amount_krw,
            game_day: current.game_day,
            payment_kind: "manual",
            command_id: Some(command.command_id.as_str()),
        },
    )
    .await?;
    let ledger = finance_rules.create_ledger_transaction(LedgerTransactionDraft {
        policy: RunPolicyContext {
            run: RunId {
                save_id: ResourceId::from_u64(current.id),
                run_revision: current.run_revision,
            },
            policy_set_id: ResourceId::from_u64(current.policy_set_id),
        },
        source: LedgerSource {
            kind: LedgerSourceKind::EssentialArrearPayment,
            source_id: payment_id.to_string(),
        },
        game_day: current.game_day,
        description: "필수 생활비 미납 상환".to_owned(),
        postings: vec![
            LedgerPosting {
                account_code: LedgerAccountCode::Wallet,
                financial_account_id: None,
                amount_krw: -command.amount_krw,
            },
            LedgerPosting {
                account_code: LedgerAccountCode::EssentialArrearLiability,
                financial_account_id: None,
                amount_krw: command.amount_krw,
            },
        ],
    })?;
    let ledger_transaction_id = write_life_ledger_transaction(
        &mut tx,
        &ledger,
        &[
            LifePostingReference::None,
            LifePostingReference::EssentialArrear(arrear.id),
        ],
    )
    .await?;
    apply_arrear_payment(
        &mut tx,
        ArrearPaymentApplication {
            save_id: current.id,
            run_revision: current.run_revision,
            payment_id,
            arrear_id: arrear.id,
            amount_krw: command.amount_krw,
            outstanding_before_krw: arrear.outstanding_amount_krw,
            game_day: current.game_day,
            ledger_transaction_id,
        },
    )
    .await?;
    let cash_krw = current
        .cash_krw
        .checked_sub(command.amount_krw)
        .context("essential-arrear wallet update overflowed")?;
    let debt_krw = current
        .debt_krw
        .checked_sub(command.amount_krw)
        .context("essential-arrear debt projection overflowed")?;
    ensure!(
        debt_krw >= 0,
        "essential-arrear debt projection became negative"
    );
    let committed_state_revision = current
        .state_revision
        .checked_add(1)
        .context("essential-arrear state revision overflowed")?;
    update_save_after_life_command(
        &mut tx,
        &current,
        committed_state_revision,
        cash_krw,
        debt_krw,
    )
    .await?;
    validate_debt_projection_in_tx(&mut tx, current.id, current.run_revision).await?;
    let remaining_krw = arrear
        .outstanding_amount_krw
        .checked_sub(command.amount_krw)
        .context("essential-arrear balance overflowed")?;
    let receipt = EssentialArrearPaymentReceipt {
        command_id: command.command_id.clone(),
        arrear_id: command.arrear_id,
        paid_krw: command.amount_krw,
        remaining_krw,
        replayed: false,
    };
    write_game_command_receipt(
        &mut tx,
        GameCommandReceiptWrite {
            save_id: current.id,
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_PAY_ESSENTIAL_ARREAR,
            payload_sha256: &fingerprint,
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

async fn update_save_after_life_command(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedLifeSaveRow,
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
    ensure!(updated.rows_affected() == 1, "life command cursor changed");
    Ok(())
}

async fn insert_arrear_payment(
    tx: &mut Transaction<'_, MySql>,
    draft: ArrearPaymentDraft<'_>,
) -> Result<u64> {
    let ArrearPaymentDraft {
        save_id,
        run_revision,
        arrear_id,
        amount_krw,
        game_day,
        payment_kind,
        command_id,
    } = draft;
    let payment_no_raw: u64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(MAX(payment_no), 0) + 1 AS UNSIGNED)
         FROM essential_arrear_payment WHERE essential_arrear_id = ?",
    )
    .bind(arrear_id)
    .fetch_one(&mut **tx)
    .await?;
    let payment_no =
        u32::try_from(payment_no_raw).context("arrear payment count is out of range")?;
    let inserted = sqlx::query(
        "INSERT INTO essential_arrear_payment
             (save_id, run_revision, essential_arrear_id, payment_no,
              payment_kind, amount_krw, game_day, command_id, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'prepared')",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(arrear_id)
    .bind(payment_no)
    .bind(payment_kind)
    .bind(amount_krw)
    .bind(game_day)
    .bind(command_id)
    .execute(&mut **tx)
    .await?;
    Ok(inserted.last_insert_id())
}

async fn apply_arrear_payment(
    tx: &mut Transaction<'_, MySql>,
    application: ArrearPaymentApplication,
) -> Result<()> {
    let ArrearPaymentApplication {
        save_id,
        run_revision,
        payment_id,
        arrear_id,
        amount_krw,
        outstanding_before_krw,
        game_day,
        ledger_transaction_id,
    } = application;
    let payment = sqlx::query(
        "UPDATE essential_arrear_payment
         SET status = 'applied', ledger_transaction_id = ?
         WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'prepared'",
    )
    .bind(ledger_transaction_id)
    .bind(save_id)
    .bind(run_revision)
    .bind(payment_id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        payment.rows_affected() == 1,
        "arrear payment was not applied"
    );
    let remaining_krw = outstanding_before_krw
        .checked_sub(amount_krw)
        .context("essential-arrear balance overflowed")?;
    let arrear = sqlx::query(
        "UPDATE essential_arrear
         SET paid_amount_krw = paid_amount_krw + ?,
             status = IF(? = 0, 'paid', 'active'),
             closed_game_day = IF(? = 0, ?, NULL)
         WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'active'
           AND outstanding_amount_krw = ?",
    )
    .bind(amount_krw)
    .bind(remaining_krw)
    .bind(remaining_krw)
    .bind(game_day)
    .bind(save_id)
    .bind(run_revision)
    .bind(arrear_id)
    .bind(outstanding_before_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        arrear.rows_affected() == 1,
        "essential arrear changed during payment"
    );
    Ok(())
}

async fn write_life_ledger_transaction(
    tx: &mut Transaction<'_, MySql>,
    ledger: &LedgerTransaction,
    references: &[LifePostingReference],
) -> Result<u64> {
    ensure!(
        references.len() == ledger.postings().len(),
        "life ledger posting references are incomplete"
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
        let posting_order = u16::try_from(index + 1).context("too many life ledger postings")?;
        let (living_cost_month_id, essential_arrear_id) = match reference {
            LifePostingReference::None => (None, None),
            LifePostingReference::LivingCostMonth(id) => (Some(*id), None),
            LifePostingReference::EssentialArrear(id) => (None, Some(*id)),
        };
        sqlx::query(
            "INSERT INTO ledger_posting
                 (save_id, run_revision, ledger_transaction_id, posting_order,
                  account_code, financial_account_id, military_savings_contract_id,
                  living_cost_month_id, essential_arrear_id, amount_krw)
             VALUES (?, ?, ?, ?, ?, ?, NULL, ?, ?, ?)",
        )
        .bind(policy.run.save_id.get())
        .bind(policy.run.run_revision)
        .bind(transaction_id)
        .bind(posting_order)
        .bind(to_db_str(&posting.account_code)?)
        .bind(posting.financial_account_id.map(ResourceId::get))
        .bind(living_cost_month_id)
        .bind(essential_arrear_id)
        .bind(posting.amount_krw)
        .execute(&mut **tx)
        .await?;
    }
    Ok(transaction_id)
}

fn to_db_str<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value)? {
        Value::String(text) => Ok(text),
        other => bail!("value is not storable as a string: {other}"),
    }
}

pub(super) async fn initialize_life_run_in_tx(
    tx: &mut Transaction<'_, MySql>,
    living_cost_rules: &dyn LivingCostRules,
    save_id: u64,
    run_revision: u32,
    market_date: Date,
    cpi_index: Option<i64>,
) -> Result<()> {
    let initialization: LifeInitializationRow = sqlx::query_as(
        "SELECT bundle.life_catalog_set_id, component.availability,
                profile.id AS profile_id, catalog.legacy_dependent_age_years,
                career_run.birth_date, `character`.region AS region_key,
                `character`.dependents, save.debt_krw
         FROM save
         INNER JOIN `character` ON `character`.save_id = save.id
         INNER JOIN career_run
           ON career_run.save_id = save.id AND career_run.run_revision = save.run_revision
         INNER JOIN run_rule_bundle AS bundle
           ON bundle.save_id = save.id AND bundle.run_revision = save.run_revision
         INNER JOIN life_catalog_set AS catalog ON catalog.id = bundle.life_catalog_set_id
         INNER JOIN life_component_version AS component
           ON component.id = catalog.living_cost_component_version_id
         LEFT JOIN cost_of_living_profile AS profile
           ON profile.life_component_version_id = component.id
         WHERE save.id = ? AND save.run_revision = ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await
    .context("new run is missing its pinned life catalog")?;
    let household_insert = sqlx::query(
        "INSERT INTO household
             (save_id, run_revision, life_catalog_set_id,
              legacy_debt_krw_at_activation, created_game_day)
         VALUES (?, ?, ?, ?, 0)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(initialization.life_catalog_set_id)
    .bind(initialization.debt_krw)
    .execute(&mut **tx)
    .await?;
    let household_id = household_insert.last_insert_id();
    sqlx::query(
        "INSERT INTO household_member
             (save_id, run_revision, household_id, member_role, ordinal,
              birth_date, joined_game_day, tax_dependent_eligible)
         VALUES (?, ?, ?, 'player', 0, ?, 0, FALSE)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(household_id)
    .bind(initialization.birth_date)
    .execute(&mut **tx)
    .await?;
    let dependent_birth_year = market_date
        .year()
        .checked_sub(i32::from(initialization.legacy_dependent_age_years))
        .context("legacy dependent birth year overflowed")?;
    let dependent_birth_date = Date::from_calendar_date(dependent_birth_year, Month::January, 1)
        .context("legacy dependent birth date is invalid")?;
    for ordinal in 1..=initialization.dependents {
        sqlx::query(
            "INSERT INTO household_member
                 (save_id, run_revision, household_id, member_role, ordinal,
                  birth_date, joined_game_day, tax_dependent_eligible)
             VALUES (?, ?, ?, 'dependent', ?, ?, 0, TRUE)",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(household_id)
        .bind(ordinal)
        .bind(dependent_birth_date)
        .execute(&mut **tx)
        .await?;
    }
    sqlx::query(
        "INSERT INTO residence
             (save_id, run_revision, household_id, region_key, tenure_type,
              effective_from_game_day)
         VALUES (?, ?, ?, ?, 'rentFree', 0)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(household_id)
    .bind(&initialization.region_key)
    .execute(&mut **tx)
    .await?;
    if initialization.availability != "active" {
        ensure!(
            initialization.profile_id.is_none(),
            "disabled living-cost component unexpectedly has a profile"
        );
        return Ok(());
    }
    let profile_id = initialization
        .profile_id
        .context("active living-cost component has no profile")?;
    let budget_insert = sqlx::query(
        "INSERT INTO household_budget
             (save_id, run_revision, household_id, cost_of_living_profile_id,
              effective_from_game_day)
         VALUES (?, ?, ?, ?, 0)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(household_id)
    .bind(profile_id)
    .execute(&mut **tx)
    .await?;
    let budget_id = budget_insert.last_insert_id();
    sqlx::query(
        "INSERT INTO household_budget_selection
             (cost_of_living_profile_id, household_budget_id,
              living_cost_category_id, living_cost_budget_band_id)
         SELECT category.cost_of_living_profile_id, ?, category.id,
                category.default_budget_band_id
         FROM living_cost_category AS category
         WHERE category.cost_of_living_profile_id = ?
         ORDER BY category.category_order",
    )
    .bind(budget_id)
    .bind(profile_id)
    .execute(&mut **tx)
    .await?;
    let sealed =
        sqlx::query("UPDATE household_budget SET sealed_at = CURRENT_TIMESTAMP(3) WHERE id = ?")
            .bind(budget_id)
            .execute(&mut **tx)
            .await?;
    ensure!(
        sealed.rows_affected() == 1,
        "default life budget was not sealed"
    );
    let remainders = sqlx::query(
        "INSERT INTO living_cost_remainder
             (save_id, run_revision, household_id, cost_of_living_profile_id,
              living_cost_category_id, remainder_numerator, last_year_month)
         SELECT ?, ?, ?, category.cost_of_living_profile_id, category.id, 0, NULL
         FROM living_cost_category AS category
         WHERE category.cost_of_living_profile_id = ?
         ORDER BY category.category_order",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(household_id)
    .bind(profile_id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        remainders.rows_affected() == 9,
        "default life remainders are incomplete"
    );
    ensure_living_cost_month_in_tx(
        tx,
        living_cost_rules,
        save_id,
        run_revision,
        0,
        market_date,
        cpi_index,
    )
    .await?;
    Ok(())
}

pub(super) async fn ensure_living_cost_month_in_tx(
    tx: &mut Transaction<'_, MySql>,
    living_cost_rules: &dyn LivingCostRules,
    save_id: u64,
    run_revision: u32,
    target_game_day: u32,
    market_date: Date,
    cpi_index: Option<i64>,
) -> Result<Option<u64>> {
    let Some(scope) = read_life_scope(tx, save_id, run_revision).await? else {
        return Ok(None);
    };
    if scope.availability != "active" {
        ensure!(
            scope.profile_id.is_none(),
            "disabled living-cost component unexpectedly has a profile"
        );
        return Ok(None);
    }
    let profile_id = scope
        .profile_id
        .context("active living-cost component has no profile")?;
    let current_cpi_index = cpi_index.context("active living-cost run has no CPI state")?;
    ensure!(current_cpi_index > 0, "active living-cost CPI is invalid");
    let year_month = month_start(market_date)?;
    let existing: Option<u64> = sqlx::query_scalar(
        "SELECT id FROM living_cost_month
         WHERE household_id = ? AND `year_month` = ?",
    )
    .bind(scope.household_id)
    .bind(year_month)
    .fetch_optional(&mut **tx)
    .await?;
    if existing.is_some() {
        return Ok(existing);
    }
    sqlx::query("SELECT id FROM household WHERE id = ? FOR UPDATE")
        .bind(scope.household_id)
        .fetch_one(&mut **tx)
        .await?;
    let residence: ResidenceRow = sqlx::query_as(
        "SELECT id, region_key, tenure_type, effective_from_game_day, property_holding_id
         FROM residence
         WHERE save_id = ? AND run_revision = ? AND household_id = ?
           AND effective_from_game_day <= ?
           AND (effective_to_game_day IS NULL OR effective_to_game_day > ?)
         ORDER BY effective_from_game_day DESC, id DESC LIMIT 1 FOR SHARE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(scope.household_id)
    .bind(target_game_day)
    .bind(target_game_day)
    .fetch_one(&mut **tx)
    .await
    .context("living-cost month has no active residence")?;
    let budget: BudgetRow = sqlx::query_as(
        "SELECT id, effective_from_game_day
         FROM household_budget
         WHERE save_id = ? AND run_revision = ? AND household_id = ?
           AND cost_of_living_profile_id = ? AND sealed_at IS NOT NULL
           AND effective_from_game_day <= ?
           AND (effective_to_game_day IS NULL OR effective_to_game_day > ?)
         ORDER BY effective_from_game_day DESC, id DESC LIMIT 1 FOR SHARE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(scope.household_id)
    .bind(profile_id)
    .bind(target_game_day)
    .bind(target_game_day)
    .fetch_one(&mut **tx)
    .await
    .context("living-cost month has no effective budget")?;
    let category_rows: Vec<CategoryCalculationRow> = sqlx::query_as(
        "SELECT category.id AS category_id, category.category_key,
                category.category_order, category.essential,
                category.base_amount_krw, profile.base_cpi_index,
                band.id AS band_id, band.factor_ppm AS budget_factor_ppm,
                region.factor_ppm AS region_factor_ppm,
                tenure.housing_replacement_factor_ppm
                    AS tenure_replacement_factor_ppm,
                CAST(remainder.remainder_numerator AS CHAR)
                    AS prior_remainder_numerator
         FROM living_cost_category AS category
         INNER JOIN cost_of_living_profile AS profile ON profile.id = category.cost_of_living_profile_id
         INNER JOIN household_budget_selection AS selection
           ON selection.household_budget_id = ?
          AND selection.living_cost_category_id = category.id
         INNER JOIN living_cost_budget_band AS band
           ON band.id = selection.living_cost_budget_band_id
          AND band.cost_of_living_profile_id = category.cost_of_living_profile_id
         INNER JOIN living_cost_region_factor AS region
           ON region.living_cost_category_id = category.id AND region.region_key = ?
         INNER JOIN living_cost_tenure_factor AS tenure
           ON tenure.cost_of_living_profile_id = category.cost_of_living_profile_id
          AND tenure.tenure_type = ?
         INNER JOIN living_cost_remainder AS remainder
           ON remainder.household_id = ? AND remainder.living_cost_category_id = category.id
         WHERE category.cost_of_living_profile_id = ?
         ORDER BY category.category_order",
    )
    .bind(budget.id)
    .bind(&residence.region_key)
    .bind(&residence.tenure_type)
    .bind(scope.household_id)
    .bind(profile_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        category_rows.len() == LivingCostCategory::ALL.len(),
        "living-cost calculation inputs are incomplete"
    );
    let members: Vec<ActiveMemberRow> = sqlx::query_as(
        "SELECT id, member_role, birth_date FROM household_member
         WHERE save_id = ? AND run_revision = ? AND household_id = ?
           AND member_role <> 'player' AND joined_game_day <= ?
           AND (left_game_day IS NULL OR left_game_day > ?)
         ORDER BY id LIMIT 129",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(scope.household_id)
    .bind(target_game_day)
    .bind(target_game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        members.len() <= 128,
        "active household member bound was exceeded"
    );
    let member_factors: Vec<MemberFactorRow> = sqlx::query_as(
        "SELECT age_band.member_role, age_band.minimum_age_years,
                age_band.maximum_age_years_exclusive,
                factor.living_cost_category_id AS category_id,
                factor.marginal_factor_ppm
         FROM living_cost_member_age_band AS age_band
         INNER JOIN living_cost_member_factor AS factor
           ON factor.living_cost_member_age_band_id = age_band.id
          AND factor.cost_of_living_profile_id = age_band.cost_of_living_profile_id
         WHERE age_band.cost_of_living_profile_id = ?
         ORDER BY age_band.id, factor.living_cost_category_id",
    )
    .bind(profile_id)
    .fetch_all(&mut **tx)
    .await?;
    let days_in_month = days_in_month(year_month)?;
    let remaining_calendar_days = days_in_month
        .checked_sub(market_date.day())
        .and_then(|days| days.checked_add(1))
        .context("living-cost remaining days overflowed")?;
    let proration = (market_date.day() != 1).then_some(LivingCostProration {
        remaining_calendar_days,
        days_in_month,
    });
    let mut inputs = Vec::with_capacity(category_rows.len());
    let mut pinned = Vec::with_capacity(category_rows.len());
    for row in category_rows {
        let category: LivingCostCategory = row
            .category_key
            .parse()
            .context("stored living-cost category is invalid")?;
        ensure!(
            row.category_order == category.order() + 1,
            "living-cost category order is invalid"
        );
        let household_factor_ppm =
            household_factor_for_category(row.category_id, &members, &member_factors, market_date)?;
        let effective_base_amount_krw = apply_tenure_replacement(
            category,
            row.base_amount_krw,
            row.tenure_replacement_factor_ppm,
        )?;
        let prior_remainder_numerator = row
            .prior_remainder_numerator
            .parse::<i128>()
            .context("living-cost remainder is not an i128 integer")?;
        inputs.push(LivingCostCategoryCalculationInput {
            category,
            essential: row.essential,
            base_monthly_krw: effective_base_amount_krw,
            base_cpi_index: i64::try_from(row.base_cpi_index)
                .context("living-cost base CPI is out of range")?,
            current_cpi_index,
            region_factor_ppm: i64::from(row.region_factor_ppm),
            household_factor_ppm,
            budget_factor_ppm: i64::from(row.budget_factor_ppm),
            prior_remainder_numerator,
            proration,
        });
        pinned.push((row, category, household_factor_ppm));
    }
    let calculation = living_cost_rules
        .calculate_month(LivingCostMonthCalculationInput {
            categories: &inputs,
        })
        .context("living-cost month calculation failed")?;
    let pinned = pinned
        .into_iter()
        .zip(calculation.categories)
        .map(|((row, category, household_factor_ppm), calculated)| {
            ensure!(
                category == calculated.category && row.essential == calculated.essential,
                "living-cost calculation reordered category facts"
            );
            Ok(PinnedCategoryCalculation {
                row,
                category,
                household_factor_ppm,
                gross_krw: calculated.gross_krw,
                next_remainder_numerator: calculated.remainder_numerator,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let month_end = next_month_start(year_month)?
        .previous_day()
        .context("living-cost month end is invalid")?;
    let due_offset = u32::try_from((month_end - market_date).whole_days())
        .context("living-cost due offset is out of range")?;
    let due_game_day = target_game_day
        .checked_add(due_offset)
        .context("living-cost due game day overflowed")?;
    let proration_units = u32::from(remaining_calendar_days)
        .checked_mul(
            u32::try_from(LIVING_COST_PRORATION_SCALE)
                .context("living-cost proration scale is invalid")?
                / u32::from(days_in_month),
        )
        .context("living-cost proration units overflowed")?;
    let fingerprint =
        household_fingerprint(scope.household_id, &residence, &budget, &members, &pinned);
    let month_insert = sqlx::query(
        "INSERT INTO living_cost_month
             (save_id, run_revision, household_id, life_catalog_set_id,
              cost_of_living_profile_id, household_budget_id, residence_id,
              `year_month`, activation_date, due_game_day, cpi_game_day, cpi_index,
              region_key, tenure_type, household_fingerprint_sha256,
              proration_scale, proration_units, days_in_month, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending')",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(scope.household_id)
    .bind(scope.life_catalog_set_id)
    .bind(profile_id)
    .bind(budget.id)
    .bind(residence.id)
    .bind(year_month)
    .bind(market_date)
    .bind(due_game_day)
    .bind(target_game_day)
    .bind(current_cpi_index)
    .bind(&residence.region_key)
    .bind(&residence.tenure_type)
    .bind(fingerprint)
    .bind(LIVING_COST_PRORATION_SCALE)
    .bind(proration_units)
    .bind(days_in_month)
    .execute(&mut **tx)
    .await?;
    let month_id = month_insert.last_insert_id();
    for category in &pinned {
        sqlx::query(
            "INSERT INTO living_cost_month_item
                 (save_id, run_revision, living_cost_month_id,
                  cost_of_living_profile_id, living_cost_category_id,
                  living_cost_budget_band_id, category_key, category_order,
                  essential, base_amount_krw, base_cpi_index, current_cpi_index,
                  region_factor_ppm, household_factor_ppm, budget_factor_ppm,
                  tenure_replacement_factor_ppm, prior_remainder_numerator,
                  gross_amount_krw, next_remainder_numerator)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(month_id)
        .bind(profile_id)
        .bind(category.row.category_id)
        .bind(category.row.band_id)
        .bind(category.category.as_str())
        .bind(category.row.category_order)
        .bind(category.row.essential)
        .bind(category.row.base_amount_krw)
        .bind(category.row.base_cpi_index)
        .bind(current_cpi_index)
        .bind(category.row.region_factor_ppm)
        .bind(category.household_factor_ppm)
        .bind(category.row.budget_factor_ppm)
        .bind(category.row.tenure_replacement_factor_ppm)
        .bind(&category.row.prior_remainder_numerator)
        .bind(category.gross_krw)
        .bind(category.next_remainder_numerator.to_string())
        .execute(&mut **tx)
        .await?;
        let remainder = sqlx::query(
            "UPDATE living_cost_remainder
             SET remainder_numerator = ?, last_year_month = ?
             WHERE household_id = ? AND living_cost_category_id = ?",
        )
        .bind(category.next_remainder_numerator.to_string())
        .bind(year_month)
        .bind(scope.household_id)
        .bind(category.row.category_id)
        .execute(&mut **tx)
        .await?;
        ensure!(
            remainder.rows_affected() == 1,
            "living-cost remainder is missing"
        );
    }
    let payload = serde_json::to_string(&LivingCostMonthSettlementPayload {
        version: LIVING_COST_PAYLOAD_VERSION,
        living_cost_month_id: ResourceId::from_u64(month_id),
    })?;
    sqlx::query(
        "INSERT INTO scheduled_settlement
             (save_id, run_revision, due_game_day, kind, payload,
              source_kind, source_id, occurrence, status)
         VALUES (?, ?, ?, 'livingCostMonth', ?, 'livingCostMonth', ?, 1, 'pending')",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(due_game_day)
    .bind(payload)
    .bind(month_id.to_string())
    .execute(&mut **tx)
    .await?;
    Ok(Some(month_id))
}

fn household_factor_for_category(
    category_id: u64,
    members: &[ActiveMemberRow],
    factors: &[MemberFactorRow],
    market_date: Date,
) -> Result<i64> {
    let mut total = 1_000_000_i64;
    for member in members {
        let age = age_on(member.birth_date, market_date)?;
        let matching = factors
            .iter()
            .filter(|factor| {
                factor.category_id == category_id
                    && factor.member_role == member.member_role
                    && age >= u16::from(factor.minimum_age_years)
                    && factor
                        .maximum_age_years_exclusive
                        .is_none_or(|maximum| age < u16::from(maximum))
            })
            .collect::<Vec<_>>();
        ensure!(
            matching.len() == 1,
            "active household member has no unique living-cost factor"
        );
        total = total
            .checked_add(i64::from(matching[0].marginal_factor_ppm))
            .context("household living-cost factor overflowed")?;
    }
    Ok(total)
}

fn apply_tenure_replacement(
    category: LivingCostCategory,
    base_amount_krw: i64,
    tenure_replacement_factor_ppm: u32,
) -> Result<i64> {
    if category != LivingCostCategory::Housing {
        return Ok(base_amount_krw);
    }
    let numerator = i128::from(base_amount_krw)
        .checked_mul(i128::from(tenure_replacement_factor_ppm))
        .context("housing replacement amount overflowed")?;
    ensure!(
        numerator % 1_000_000 == 0,
        "housing replacement would discard a fractional KRW"
    );
    i64::try_from(numerator / 1_000_000).context("housing replacement amount is out of range")
}

fn age_on(birth_date: Date, current_date: Date) -> Result<u16> {
    ensure!(
        birth_date <= current_date,
        "household member birth date is in the future"
    );
    let before_birthday =
        (current_date.month(), current_date.day()) < (birth_date.month(), birth_date.day());
    let years = current_date
        .year()
        .checked_sub(birth_date.year())
        .and_then(|value| value.checked_sub(i32::from(before_birthday)))
        .context("household member age overflowed")?;
    u16::try_from(years).context("household member age is out of range")
}

fn month_start(date: Date) -> Result<Date> {
    date.replace_day(1)
        .context("market date has no month start")
}

fn next_month_start(year_month: Date) -> Result<Date> {
    let (year, month) = if year_month.month() == Month::December {
        (
            year_month
                .year()
                .checked_add(1)
                .context("living-cost year overflowed")?,
            Month::January,
        )
    } else {
        (
            year_month.year(),
            Month::try_from(u8::from(year_month.month()) + 1)
                .context("living-cost next month is invalid")?,
        )
    };
    Date::from_calendar_date(year, month, 1).context("living-cost next month is invalid")
}

fn days_in_month(year_month: Date) -> Result<u8> {
    u8::try_from((next_month_start(year_month)? - year_month).whole_days())
        .context("living-cost month length is out of range")
}

fn household_fingerprint(
    household_id: u64,
    residence: &ResidenceRow,
    budget: &BudgetRow,
    members: &[ActiveMemberRow],
    categories: &[PinnedCategoryCalculation],
) -> String {
    let mut canonical = format!(
        "lifeledger.life.household.v1\nhouseholdId={household_id}\nresidenceId={}\nregion={}\ntenure={}\nresidenceEffectiveDay={}\nbudgetId={}\nbudgetEffectiveDay={}",
        residence.id,
        residence.region_key,
        residence.tenure_type,
        residence.effective_from_game_day,
        budget.id,
        budget.effective_from_game_day,
    );
    for member in members {
        canonical.push_str(&format!(
            "\nmember={}:{}:{}",
            member.id, member.member_role, member.birth_date
        ));
    }
    for category in categories {
        canonical.push_str(&format!(
            "\ncategory={}:{}:{}",
            category.category.as_str(),
            category.row.band_id,
            category.household_factor_ppm
        ));
    }
    sha256(&canonical)
}

pub(super) async fn read_life_snapshot_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
) -> Result<LifeSnapshotState> {
    let Some(scope) = read_life_scope(tx, save_id, run_revision).await? else {
        return Ok(LifeSnapshotState::empty());
    };
    let household =
        read_household_summary(tx, save_id, run_revision, scope.household_id, game_day).await?;
    let residence =
        read_active_residence(tx, save_id, run_revision, scope.household_id, game_day).await?;
    let arrears = if rate_status(&scope) == LifeRateStatus::Active {
        read_active_arrears(tx, save_id, run_revision, scope.household_id).await?
    } else {
        ActiveArrearWindow {
            items: Vec::new(),
            has_more: false,
            total_krw: 0,
        }
    };
    let current_month = if rate_status(&scope) == LifeRateStatus::Active {
        read_current_living_cost_month(tx, save_id, run_revision).await?
    } else {
        None
    };
    validate_debt_projection_in_tx(tx, save_id, run_revision).await?;
    let credit_and_loans = read_credit_and_loan_snapshot_in_tx(tx, save_id, run_revision).await?;
    let (tenant_lease_deposit_krw, active_lease) =
        read_active_housing_lease_snapshot_in_tx(tx, save_id, run_revision, game_day).await?;
    let lease_arrears = read_active_lease_arrears_in_tx(tx, save_id, run_revision).await?;
    validate_property_projection_in_tx(tx, save_id, run_revision).await?;
    let property_holdings = read_active_property_holdings_in_tx(tx, save_id, run_revision).await?;
    let active_welfare_applications =
        read_active_welfare_applications_in_tx(tx, save_id, run_revision).await?;
    let pending_events = read_pending_life_events_in_tx(tx, save_id).await?;
    let insurance =
        read_insurance_snapshot_in_tx(tx, create_insurance_rules().as_ref(), save_id).await?;
    let insolvency =
        read_insolvency_snapshot_in_tx(tx, create_insolvency_rules().as_ref(), save_id).await?;
    let corporation = read_corporation_snapshot_in_tx(tx, save_id).await?;
    Ok(LifeSnapshotState {
        rate_status: rate_status(&scope),
        household: Some(household),
        residence: Some(to_residence_state(residence)?),
        current_month,
        active_arrears: arrears.items,
        has_more_active_arrears: arrears.has_more,
        total_essential_arrear_krw: arrears.total_krw,
        credit_band: credit_and_loans.credit_band,
        credit_reasons: credit_and_loans.credit_reasons,
        active_loans: credit_and_loans.active_loans,
        next_loan_installment: credit_and_loans.next_loan_installment,
        total_loan_balance_krw: credit_and_loans.total_loan_balance_krw,
        tenant_lease_deposit_krw,
        active_lease,
        active_lease_arrears: lease_arrears.items,
        has_more_active_lease_arrears: lease_arrears.has_more,
        total_lease_arrear_krw: lease_arrears.total_krw,
        active_property_holdings: property_holdings.items,
        has_more_active_property_holdings: property_holdings.has_more,
        total_property_book_value_krw: property_holdings.total_book_value_krw,
        active_welfare_applications,
        pending_events,
        insurance_capability: insurance.capability,
        active_insurance_contracts: insurance.active_contracts,
        pending_insurance_claims: insurance.pending_claims,
        insolvency,
        corporation,
    })
}

async fn read_life_budget_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
) -> Result<LifeBudgetState> {
    let (run_revision, game_day): (u32, u32) =
        sqlx::query_as("SELECT run_revision, game_day FROM save WHERE id = ?")
            .bind(save_id)
            .fetch_one(&mut **tx)
            .await?;
    let scope = read_life_scope(tx, save_id, run_revision)
        .await?
        .context("life budget requires an initialized household")?;
    let household =
        read_household_summary(tx, save_id, run_revision, scope.household_id, game_day).await?;
    let residence =
        read_active_residence(tx, save_id, run_revision, scope.household_id, game_day).await?;
    let arrears = if rate_status(&scope) == LifeRateStatus::Active {
        read_active_arrears(tx, save_id, run_revision, scope.household_id).await?
    } else {
        ActiveArrearWindow {
            items: Vec::new(),
            has_more: false,
            total_krw: 0,
        }
    };
    let current_month = if rate_status(&scope) == LifeRateStatus::Active {
        read_current_living_cost_month(tx, save_id, run_revision).await?
    } else {
        None
    };
    validate_debt_projection_in_tx(tx, save_id, run_revision).await?;
    let (allowed_bands, selections) = if let Some(profile_id) = scope.profile_id {
        let band_rows: Vec<BudgetBandRow> = sqlx::query_as(
            "SELECT id, band_key, display_name, factor_ppm
             FROM living_cost_budget_band
             WHERE cost_of_living_profile_id = ? ORDER BY band_order LIMIT 17",
        )
        .bind(profile_id)
        .fetch_all(&mut **tx)
        .await?;
        ensure!(band_rows.len() <= 16, "life budget band bound was exceeded");
        let allowed_bands = band_rows
            .into_iter()
            .map(|row| LifeBudgetBandState {
                id: ResourceId::from_u64(row.id),
                band_key: row.band_key,
                display_name: row.display_name,
                factor_ppm: i64::from(row.factor_ppm),
            })
            .collect();
        let selection_rows: Vec<BudgetSelectionRow> = sqlx::query_as(
            "SELECT category.category_key, selection.living_cost_budget_band_id AS band_id
             FROM household_budget AS budget
             INNER JOIN household_budget_selection AS selection
               ON selection.household_budget_id = budget.id
             INNER JOIN living_cost_category AS category
               ON category.id = selection.living_cost_category_id
             WHERE budget.save_id = ? AND budget.run_revision = ?
               AND budget.household_id = ? AND budget.sealed_at IS NOT NULL
               AND budget.effective_to_game_day IS NULL
             ORDER BY category.category_order",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(scope.household_id)
        .fetch_all(&mut **tx)
        .await?;
        ensure!(
            selection_rows.len() == LivingCostCategory::ALL.len(),
            "active life budget selections are incomplete"
        );
        let selections = selection_rows
            .into_iter()
            .map(|row| {
                Ok(LifeBudgetSelectionState {
                    category: row
                        .category_key
                        .parse()
                        .context("stored life-budget category is invalid")?,
                    band_id: ResourceId::from_u64(row.band_id),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        (allowed_bands, selections)
    } else {
        (Vec::new(), Vec::new())
    };
    Ok(LifeBudgetState {
        rate_status: rate_status(&scope),
        household,
        residence: to_residence_state(residence)?,
        allowed_bands,
        selections,
        current_month,
        active_arrears: arrears.items,
        has_more_active_arrears: arrears.has_more,
        total_essential_arrear_krw: arrears.total_krw,
    })
}

async fn read_household_summary(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    household_id: u64,
    game_day: u32,
) -> Result<LifeHouseholdState> {
    let row: HouseholdSummaryRow = sqlx::query_as(
        "SELECT household.id,
                COUNT(member.id) AS member_count,
                COUNT(CASE WHEN member.member_role <> 'player' THEN 1 END)
                    AS dependent_count,
                COUNT(CASE WHEN member.tax_dependent_eligible = TRUE THEN 1 END)
                    AS tax_dependent_eligible_count
         FROM household
         INNER JOIN household_member AS member
           ON member.household_id = household.id
          AND member.joined_game_day <= ?
          AND (member.left_game_day IS NULL OR member.left_game_day > ?)
         WHERE household.save_id = ? AND household.run_revision = ? AND household.id = ?
         GROUP BY household.id",
    )
    .bind(game_day)
    .bind(game_day)
    .bind(save_id)
    .bind(run_revision)
    .bind(household_id)
    .fetch_one(&mut **tx)
    .await
    .context("life household has no active members")?;
    Ok(LifeHouseholdState {
        id: ResourceId::from_u64(row.id),
        member_count: u32::try_from(row.member_count)
            .context("household member count is out of range")?,
        dependent_count: u32::try_from(row.dependent_count)
            .context("household dependent count is out of range")?,
        tax_dependent_eligible_count: u32::try_from(row.tax_dependent_eligible_count)
            .context("household tax-dependent count is out of range")?,
    })
}

async fn read_active_residence(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    household_id: u64,
    game_day: u32,
) -> Result<ResidenceRow> {
    sqlx::query_as(
        "SELECT id, region_key, tenure_type, effective_from_game_day, property_holding_id
         FROM residence
         WHERE save_id = ? AND run_revision = ? AND household_id = ?
           AND effective_from_game_day <= ?
           AND (effective_to_game_day IS NULL OR effective_to_game_day > ?)
         ORDER BY effective_from_game_day DESC, id DESC LIMIT 1",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(household_id)
    .bind(game_day)
    .bind(game_day)
    .fetch_one(&mut **tx)
    .await
    .context("life household has no active residence")
}

fn to_residence_state(row: ResidenceRow) -> Result<LifeResidenceState> {
    let tenure_kind = match row.tenure_type.as_str() {
        "rentFree" => ResidenceTenureKind::RentFree,
        "owner" => ResidenceTenureKind::Owner,
        "jeonse" => ResidenceTenureKind::Jeonse,
        "monthlyRent" => ResidenceTenureKind::MonthlyRent,
        _ => bail!("stored residence tenure type is invalid"),
    };
    Ok(LifeResidenceState {
        id: ResourceId::from_u64(row.id),
        region_key: row.region_key,
        tenure_kind,
        effective_from_game_day: row.effective_from_game_day,
        property_holding_id: row.property_holding_id.map(ResourceId::from_u64),
    })
}

async fn read_active_arrears(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    household_id: u64,
) -> Result<ActiveArrearWindow> {
    let mut rows: Vec<ActiveArrearRow> = sqlx::query_as(
        "SELECT id, due_year_month, category_key, original_amount_krw,
                outstanding_amount_krw
         FROM essential_arrear
         WHERE save_id = ? AND run_revision = ? AND household_id = ? AND status = 'active'
         ORDER BY due_year_month,
             FIELD(category_key, 'housing', 'food', 'transport', 'communication',
                   'utilities', 'healthcare', 'education', 'dependentCare',
                   'discretionary'), id
         LIMIT 21",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(household_id)
    .fetch_all(&mut **tx)
    .await?;
    let has_more = rows.len() > MAX_ACTIVE_ARREARS;
    rows.truncate(MAX_ACTIVE_ARREARS);
    let items = rows
        .into_iter()
        .map(|row| {
            Ok(EssentialArrearState {
                id: ResourceId::from_u64(row.id),
                due_year_month: to_year_month(row.due_year_month)?,
                category: row
                    .category_key
                    .parse()
                    .context("stored essential-arrear category is invalid")?,
                original_krw: row.original_amount_krw,
                remaining_krw: row.outstanding_amount_krw,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let total_krw: i64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(outstanding_amount_krw), 0) AS SIGNED)
         FROM essential_arrear
         WHERE save_id = ? AND run_revision = ? AND household_id = ? AND status = 'active'",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(household_id)
    .fetch_one(&mut **tx)
    .await?;
    let window_total = items.iter().try_fold(0_i64, |total, arrear| {
        total
            .checked_add(arrear.remaining_krw)
            .context("essential-arrear window total overflowed")
    })?;
    ensure!(
        (!has_more && window_total == total_krw) || (has_more && total_krw > window_total),
        "essential-arrear window disagrees with its total"
    );
    Ok(ActiveArrearWindow {
        items,
        has_more,
        total_krw,
    })
}

async fn read_current_living_cost_month(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<Option<LivingCostMonthState>> {
    let row: Option<LivingCostMonthReadRow> = sqlx::query_as(
        "SELECT living_month.id, profile.id AS profile_id, profile.profile_key,
                living_month.`year_month`, living_month.cpi_index,
                living_month.cpi_game_day AS activation_game_day,
                living_month.due_game_day AS settlement_game_day,
                living_month.proration_scale, living_month.proration_units,
                living_month.days_in_month,
                living_month.status,
                CAST(COALESCE(living_month.gross_amount_krw,
                    (SELECT SUM(item.gross_amount_krw) FROM living_cost_month_item AS item
                     WHERE item.living_cost_month_id = living_month.id)) AS SIGNED)
                    AS total_gross_krw,
                COALESCE(living_month.paid_amount_krw, 0) AS total_paid_krw,
                COALESCE(living_month.arrear_amount_krw, 0) AS total_arrear_krw
         FROM living_cost_month AS living_month
         INNER JOIN cost_of_living_profile AS profile
           ON profile.id = living_month.cost_of_living_profile_id
         INNER JOIN save ON save.id = living_month.save_id
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         LEFT JOIN market_daily AS daily
           ON daily.world_id = save.market_world_id AND daily.game_day = save.game_day
         WHERE living_month.save_id = ? AND living_month.run_revision = ?
           AND living_month.`year_month` = DATE_FORMAT(
               COALESCE(daily.market_date,
                   DATE_ADD(world.start_date, INTERVAL save.game_day DAY)),
               '%Y-%m-01'
           )
         LIMIT 1",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let item_rows: Vec<LivingCostMonthItemReadRow> = sqlx::query_as(
        "SELECT category_key, living_cost_budget_band_id AS band_id, essential,
                base_amount_krw, base_cpi_index, region_factor_ppm, household_factor_ppm,
                budget_factor_ppm, tenure_replacement_factor_ppm, gross_amount_krw,
                COALESCE(paid_amount_krw, 0) AS paid_amount_krw,
                COALESCE(arrear_amount_krw, 0) AS arrear_amount_krw
         FROM living_cost_month_item
         WHERE save_id = ? AND run_revision = ? AND living_cost_month_id = ?
         ORDER BY category_order",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(row.id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        item_rows.len() == LivingCostCategory::ALL.len(),
        "living-cost month snapshot has incomplete items"
    );
    let items = item_rows
        .into_iter()
        .map(|item| {
            Ok(LivingCostMonthItemState {
                category: item
                    .category_key
                    .parse()
                    .context("stored living-cost item category is invalid")?,
                band_id: ResourceId::from_u64(item.band_id),
                essential: item.essential,
                base_monthly_krw: item.base_amount_krw,
                base_cpi_index: i64::try_from(item.base_cpi_index)
                    .context("living-cost base CPI is out of range")?,
                region_factor_ppm: i64::from(item.region_factor_ppm),
                household_factor_ppm: i64::try_from(item.household_factor_ppm)
                    .context("living-cost household factor is out of range")?,
                budget_factor_ppm: i64::from(item.budget_factor_ppm),
                tenure_replacement_factor_ppm: i64::from(item.tenure_replacement_factor_ppm),
                gross_krw: item.gross_amount_krw,
                paid_krw: item.paid_amount_krw,
                arrear_krw: item.arrear_amount_krw,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(LivingCostMonthState {
        id: ResourceId::from_u64(row.id),
        profile_id: ResourceId::from_u64(row.profile_id),
        profile_key: row.profile_key,
        year_month: to_year_month(row.year_month)?,
        current_cpi_index: i64::try_from(row.cpi_index)
            .context("living-cost CPI is out of range")?,
        activation_game_day: row.activation_game_day,
        settlement_game_day: row.settlement_game_day,
        proration_scale: row.proration_scale,
        proration_units: row.proration_units,
        proration_days: proration_days(
            row.proration_scale,
            row.proration_units,
            row.days_in_month,
        )?,
        days_in_month: row.days_in_month,
        settled: row.status == "settled",
        total_gross_krw: row.total_gross_krw,
        total_paid_krw: row.total_paid_krw,
        total_arrear_krw: row.total_arrear_krw,
        items,
    }))
}

fn rate_status(scope: &LifeScopeRow) -> LifeRateStatus {
    if scope.availability == "active" && scope.profile_id.is_some() {
        LifeRateStatus::Active
    } else {
        LifeRateStatus::RateUnavailable
    }
}

fn to_year_month(date: Date) -> Result<YearMonth> {
    let year_month = YearMonth {
        year: date.year(),
        month: u8::from(date.month()),
    };
    ensure!(year_month.is_valid(), "stored living-cost month is invalid");
    Ok(year_month)
}

fn proration_days(scale: u32, units: u32, days_in_month: u8) -> Result<u8> {
    ensure!(
        scale
            == u32::try_from(LIVING_COST_PRORATION_SCALE)
                .context("living-cost proration scale is out of range")?
            && (28..=31).contains(&days_in_month)
            && scale.is_multiple_of(u32::from(days_in_month)),
        "stored living-cost proration basis is invalid"
    );
    let units_per_day = scale / u32::from(days_in_month);
    ensure!(
        units > 0 && units.is_multiple_of(units_per_day),
        "stored living-cost proration units are invalid"
    );
    u8::try_from(units / units_per_day).context("living-cost proration days are out of range")
}

pub(super) async fn read_tax_dependent_count_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
) -> Result<u8> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM household_member
         WHERE save_id = ? AND run_revision = ?
           AND member_role <> 'player' AND tax_dependent_eligible = TRUE
           AND joined_game_day <= ?
           AND (left_game_day IS NULL OR left_game_day > ?)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(game_day)
    .bind(game_day)
    .fetch_one(&mut **tx)
    .await?;
    u8::try_from(count).context("tax-dependent count is out of range")
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LivingCostMonthSettlementPayload {
    version: u8,
    living_cost_month_id: ResourceId,
}

pub(super) fn validate_living_cost_settlement_envelope(
    settlement: &ScheduledSettlement,
) -> Result<()> {
    ensure!(
        settlement.kind == SettlementKind::LivingCostMonth
            && settlement.source.kind == SettlementSourceKind::LivingCostMonth,
        "settlement is not a living-cost month"
    );
    let payload: LivingCostMonthSettlementPayload =
        serde_json::from_value(settlement.payload.clone())
            .context("stored living-cost settlement payload is invalid")?;
    ensure!(
        payload.version == LIVING_COST_PAYLOAD_VERSION
            && settlement.source.source_id == payload.living_cost_month_id.to_string()
            && settlement.source.occurrence == 1,
        "stored living-cost settlement identity is invalid"
    );
    Ok(())
}

pub(super) async fn settle_living_cost_month_by_id_in_tx(
    tx: &mut Transaction<'_, MySql>,
    finance_rules: &dyn FinanceRules,
    save_id: u64,
    run_revision: u32,
    policy_set_id: u64,
    game_day: u32,
    settlement_id: u64,
) -> Result<()> {
    let unlocked_settlement: SettlementEnvelopeRow = sqlx::query_as(
        "SELECT due_game_day, kind, CAST(payload AS CHAR) AS payload_json,
                source_kind, source_id, occurrence, status
         FROM scheduled_settlement
         WHERE save_id = ? AND run_revision = ? AND id = ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(settlement_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("living-cost settlement is missing")?;
    let payload: LivingCostMonthSettlementPayload =
        serde_json::from_str(&unlocked_settlement.payload_json)
            .context("stored living-cost settlement payload is invalid")?;
    ensure!(
        unlocked_settlement.status == "pending"
            && unlocked_settlement.due_game_day == game_day
            && unlocked_settlement.kind == "livingCostMonth"
            && unlocked_settlement.source_kind == "livingCostMonth"
            && unlocked_settlement.source_id == payload.living_cost_month_id.to_string()
            && unlocked_settlement.occurrence == 1
            && payload.version == LIVING_COST_PAYLOAD_VERSION,
        "living-cost settlement identity changed before its lock"
    );
    let unlocked_month: PendingMonthRow = sqlx::query_as(
        "SELECT id, household_id, `year_month`, due_game_day, status
         FROM living_cost_month
         WHERE save_id = ? AND run_revision = ? AND id = ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(payload.living_cost_month_id.get())
    .fetch_optional(&mut **tx)
    .await?
    .context("living-cost settlement month is missing")?;
    sqlx::query(
        "SELECT id FROM household
         WHERE save_id = ? AND run_revision = ? AND id = ? FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(unlocked_month.household_id)
    .fetch_one(&mut **tx)
    .await?;
    let month: PendingMonthRow = sqlx::query_as(
        "SELECT id, household_id, `year_month`, due_game_day, status
         FROM living_cost_month
         WHERE save_id = ? AND run_revision = ? AND id = ? FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(payload.living_cost_month_id.get())
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        month == unlocked_month && month.status == "pending" && month.due_game_day == game_day,
        "living-cost month is not pending on its due day"
    );
    let items: Vec<PendingMonthItemRow> = sqlx::query_as(
        "SELECT id, category_key, essential, gross_amount_krw
         FROM living_cost_month_item
         WHERE save_id = ? AND run_revision = ? AND living_cost_month_id = ?
         ORDER BY category_order FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(month.id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        items.len() == LivingCostCategory::ALL.len(),
        "living-cost settlement has incomplete items"
    );
    let arrears: Vec<ActiveArrearRow> = sqlx::query_as(
        "SELECT id, due_year_month, category_key, original_amount_krw,
                outstanding_amount_krw
         FROM essential_arrear
         WHERE save_id = ? AND run_revision = ? AND household_id = ?
           AND status = 'active'
         ORDER BY due_year_month,
             FIELD(category_key, 'housing', 'food', 'transport', 'communication',
                   'utilities', 'healthcare', 'education', 'dependentCare',
                   'discretionary'), id
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(month.household_id)
    .fetch_all(&mut **tx)
    .await?;
    let locked_settlement: SettlementEnvelopeRow = sqlx::query_as(
        "SELECT due_game_day, kind, CAST(payload AS CHAR) AS payload_json,
                source_kind, source_id, occurrence, status
         FROM scheduled_settlement
         WHERE save_id = ? AND run_revision = ? AND id = ? FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(settlement_id)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        locked_settlement == unlocked_settlement,
        "living-cost settlement changed before its lock"
    );
    let (wallet_cash_krw, debt_krw): (i64, i64) =
        sqlx::query_as("SELECT cash_krw, debt_krw FROM save WHERE id = ? AND run_revision = ?")
            .bind(save_id)
            .bind(run_revision)
            .fetch_one(&mut **tx)
            .await?;
    validate_debt_projection_in_tx(tx, save_id, run_revision).await?;
    let current_charges = items
        .iter()
        .map(|item| {
            Ok(CurrentLivingCostCharge {
                category: item
                    .category_key
                    .parse()
                    .context("stored living-cost item category is invalid")?,
                essential: item.essential,
                gross_krw: item.gross_amount_krw,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let existing_arrears = arrears
        .iter()
        .map(|arrear| {
            Ok(EssentialArrearBalance {
                arrear_id: arrear.id,
                due_year_month: to_year_month(arrear.due_year_month)?,
                category: arrear
                    .category_key
                    .parse()
                    .context("stored essential-arrear category is invalid")?,
                remaining_krw: arrear.outstanding_amount_krw,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let allocation = create_living_cost_rules()
        .allocate_month(LivingCostAllocationInput {
            due_year_month: to_year_month(month.year_month)?,
            wallet_cash_krw,
            current_charges: &current_charges,
            existing_arrears: &existing_arrears,
        })
        .context("living-cost month allocation failed")?;
    let mut created_arrear_ids = Vec::new();
    let mut gross_amount_krw = 0_i64;
    let mut paid_amount_krw = 0_i64;
    let mut arrear_amount_krw = 0_i64;
    for item in &items {
        let category: LivingCostCategory = item
            .category_key
            .parse()
            .context("stored living-cost item category is invalid")?;
        let allocated = allocation
            .current_allocations
            .iter()
            .find(|allocation| allocation.category == category)
            .context("living-cost allocation lost an item")?;
        ensure!(
            allocated.essential == item.essential && allocated.gross_krw == item.gross_amount_krw,
            "living-cost allocation changed pinned item facts"
        );
        let item_arrear_krw = if item.essential {
            allocated.unpaid_krw
        } else {
            0
        };
        let updated = sqlx::query(
            "UPDATE living_cost_month_item
             SET paid_amount_krw = ?, arrear_amount_krw = ?
             WHERE save_id = ? AND run_revision = ? AND id = ?
               AND paid_amount_krw IS NULL AND arrear_amount_krw IS NULL",
        )
        .bind(allocated.paid_krw)
        .bind(item_arrear_krw)
        .bind(save_id)
        .bind(run_revision)
        .bind(item.id)
        .execute(&mut **tx)
        .await?;
        ensure!(
            updated.rows_affected() == 1,
            "living-cost item was already settled"
        );
        if item_arrear_krw > 0 {
            let inserted = sqlx::query(
                "INSERT INTO essential_arrear
                     (save_id, run_revision, household_id, living_cost_month_item_id,
                      category_key, due_year_month, original_amount_krw,
                      paid_amount_krw, status, created_game_day)
                 VALUES (?, ?, ?, ?, ?, ?, ?, 0, 'active', ?)",
            )
            .bind(save_id)
            .bind(run_revision)
            .bind(month.household_id)
            .bind(item.id)
            .bind(category.as_str())
            .bind(month.year_month)
            .bind(item_arrear_krw)
            .bind(game_day)
            .execute(&mut **tx)
            .await?;
            created_arrear_ids.push((inserted.last_insert_id(), item_arrear_krw));
        }
        gross_amount_krw = gross_amount_krw
            .checked_add(item.gross_amount_krw)
            .context("living-cost gross total overflowed")?;
        paid_amount_krw = paid_amount_krw
            .checked_add(allocated.paid_krw)
            .context("living-cost paid total overflowed")?;
        arrear_amount_krw = arrear_amount_krw
            .checked_add(item_arrear_krw)
            .context("living-cost arrear total overflowed")?;
    }
    let current_expense_krw = paid_amount_krw
        .checked_add(arrear_amount_krw)
        .context("living-cost expense overflowed")?;
    ensure!(
        current_expense_krw > 0,
        "living-cost month has no consumed expense"
    );
    let mut postings = Vec::new();
    let mut references = Vec::new();
    postings.push(LedgerPosting {
        account_code: LedgerAccountCode::LivingCostExpense,
        financial_account_id: None,
        amount_krw: current_expense_krw,
    });
    references.push(LifePostingReference::LivingCostMonth(month.id));
    if paid_amount_krw > 0 {
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::Wallet,
            financial_account_id: None,
            amount_krw: -paid_amount_krw,
        });
        references.push(LifePostingReference::None);
    }
    for (arrear_id, amount_krw) in &created_arrear_ids {
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::EssentialArrearLiability,
            financial_account_id: None,
            amount_krw: -*amount_krw,
        });
        references.push(LifePostingReference::EssentialArrear(*arrear_id));
    }
    let month_ledger = finance_rules.create_ledger_transaction(LedgerTransactionDraft {
        policy: RunPolicyContext {
            run: RunId {
                save_id: ResourceId::from_u64(save_id),
                run_revision,
            },
            policy_set_id: ResourceId::from_u64(policy_set_id),
        },
        source: LedgerSource {
            kind: LedgerSourceKind::LivingCostMonth,
            source_id: month.id.to_string(),
        },
        game_day,
        description: "월 생활비 정산".to_owned(),
        postings,
    })?;
    let month_ledger_id = write_life_ledger_transaction(tx, &month_ledger, &references).await?;
    let mut paid_old_arrear_krw = 0_i64;
    for payment in allocation
        .existing_arrear_payments
        .iter()
        .filter(|payment| payment.paid_krw > 0)
    {
        let payment_id = insert_arrear_payment(
            tx,
            ArrearPaymentDraft {
                save_id,
                run_revision,
                arrear_id: payment.arrear_id,
                amount_krw: payment.paid_krw,
                game_day,
                payment_kind: "automatic",
                command_id: None,
            },
        )
        .await?;
        let ledger = finance_rules.create_ledger_transaction(LedgerTransactionDraft {
            policy: RunPolicyContext {
                run: RunId {
                    save_id: ResourceId::from_u64(save_id),
                    run_revision,
                },
                policy_set_id: ResourceId::from_u64(policy_set_id),
            },
            source: LedgerSource {
                kind: LedgerSourceKind::EssentialArrearPayment,
                source_id: payment_id.to_string(),
            },
            game_day,
            description: "필수 생활비 미납 자동 상환".to_owned(),
            postings: vec![
                LedgerPosting {
                    account_code: LedgerAccountCode::Wallet,
                    financial_account_id: None,
                    amount_krw: -payment.paid_krw,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::EssentialArrearLiability,
                    financial_account_id: None,
                    amount_krw: payment.paid_krw,
                },
            ],
        })?;
        let ledger_id = write_life_ledger_transaction(
            tx,
            &ledger,
            &[
                LifePostingReference::None,
                LifePostingReference::EssentialArrear(payment.arrear_id),
            ],
        )
        .await?;
        apply_arrear_payment(
            tx,
            ArrearPaymentApplication {
                save_id,
                run_revision,
                payment_id,
                arrear_id: payment.arrear_id,
                amount_krw: payment.paid_krw,
                outstanding_before_krw: payment.balance_before_krw,
                game_day,
                ledger_transaction_id: ledger_id,
            },
        )
        .await?;
        paid_old_arrear_krw = paid_old_arrear_krw
            .checked_add(payment.paid_krw)
            .context("automatic arrear payment total overflowed")?;
    }
    let updated_month = sqlx::query(
        "UPDATE living_cost_month
         SET status = 'settled', gross_amount_krw = ?, paid_amount_krw = ?,
             arrear_amount_krw = ?, ledger_transaction_id = ?
         WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'pending'",
    )
    .bind(gross_amount_krw)
    .bind(paid_amount_krw)
    .bind(arrear_amount_krw)
    .bind(month_ledger_id)
    .bind(save_id)
    .bind(run_revision)
    .bind(month.id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated_month.rows_affected() == 1,
        "living-cost month was not settled"
    );
    let updated_settlement = sqlx::query(
        "UPDATE scheduled_settlement
         SET status = 'settled', settled_ledger_transaction_id = ?
         WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'pending'",
    )
    .bind(month_ledger_id)
    .bind(save_id)
    .bind(run_revision)
    .bind(settlement_id)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated_settlement.rows_affected() == 1,
        "living-cost schedule was not settled"
    );
    let projected_debt_krw = debt_krw
        .checked_add(arrear_amount_krw)
        .and_then(|value| value.checked_sub(paid_old_arrear_krw))
        .context("living-cost debt projection overflowed")?;
    ensure!(
        projected_debt_krw >= 0,
        "living-cost debt projection became negative"
    );
    let updated_save = sqlx::query(
        "UPDATE save SET cash_krw = ?, debt_krw = ?
         WHERE id = ? AND run_revision = ? AND cash_krw = ? AND debt_krw = ?",
    )
    .bind(allocation.wallet_cash_after_krw)
    .bind(projected_debt_krw)
    .bind(save_id)
    .bind(run_revision)
    .bind(wallet_cash_krw)
    .bind(debt_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        updated_save.rows_affected() == 1,
        "living-cost wallet changed during settlement"
    );
    validate_debt_projection_in_tx(tx, save_id, run_revision).await?;
    Ok(())
}

fn validate_and_sort_budget_selections(
    selections: &[LifeBudgetSelectionState],
) -> Result<Vec<LifeBudgetSelectionState>> {
    ensure!(
        selections.len() == LivingCostCategory::ALL.len(),
        "life budget must contain every category"
    );
    let mut seen = HashSet::with_capacity(LivingCostCategory::ALL.len());
    for selection in selections {
        ensure!(
            seen.insert(selection.category),
            "life budget contains a duplicate category"
        );
    }
    ensure!(
        LivingCostCategory::ALL
            .iter()
            .all(|category| seen.contains(category)),
        "life budget is missing a category"
    );
    let mut canonical = selections.to_vec();
    canonical.sort_by_key(|selection| selection.category.order());
    Ok(canonical)
}

fn update_budget_fingerprint(command: &UpdateLifeBudgetCommand) -> String {
    let mut selections = command.selections.iter().collect::<Vec<_>>();
    selections.sort_by_key(|selection| (selection.category.order(), selection.band_id));
    let mut canonical = format!(
        "lifeledger.life.updateBudget.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
    );
    for selection in selections {
        canonical.push_str("\nselection=");
        canonical.push_str(selection.category.as_str());
        canonical.push(':');
        canonical.push_str(&selection.band_id.to_string());
    }
    sha256(&canonical)
}

fn pay_arrear_fingerprint(command: &PayEssentialArrearCommand) -> String {
    sha256(&format!(
        "lifeledger.life.payEssentialArrear.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\narrearId={}\namountKrw={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.arrear_id,
        command.amount_krw,
    ))
}

fn loan_quote_fingerprint(command: &CreateLoanQuoteCommand) -> String {
    sha256(&format!(
        "lifeledger.life.loanQuote.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\nproductVersionId={}\nprincipalKrw={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.product_version_id,
        command.principal_krw,
    ))
}

fn lease_deposit_loan_quote_fingerprint(command: &CreateLeaseDepositLoanQuoteCommand) -> String {
    let offer_kind = match command.offer_kind {
        crate::life::HousingLeaseOfferKind::Jeonse => "jeonse",
        crate::life::HousingLeaseOfferKind::MonthlyRent => "monthlyRent",
    };
    sha256(&format!(
        "lifeledger.life.quoteLeaseDepositLoan.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\nlistingId={}\nofferKind={}\nproductVersionId={}\nprincipalKrw={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.listing_id,
        offer_kind,
        command.product_version_id,
        command.principal_krw,
    ))
}

fn loan_execution_fingerprint(command: &ExecuteLoanCommand) -> String {
    sha256(&format!(
        "lifeledger.life.executeLoan.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\nquoteId={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.quote_id,
    ))
}

fn loan_prepayment_fingerprint(command: &PrepayLoanCommand) -> String {
    sha256(&format!(
        "lifeledger.life.prepayLoan.v1\nexpectedRunRevision={}\nexpectedStateRevision={}\nexpectedGameDay={}\nloanId={}\nprincipalKrw={}",
        command.cursor.expected_run_revision,
        command.cursor.expected_state_revision,
        command.cursor.expected_game_day,
        command.loan_id,
        command.principal_krw,
    ))
}

fn sha256(canonical: &str) -> String {
    Sha256::digest(canonical.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finance::{CommandCursor, CommandId, RunId, SettlementSource, SettlementStatus};

    fn given_budget_command() -> UpdateLifeBudgetCommand {
        UpdateLifeBudgetCommand {
            command_id: CommandId::parse("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2")
                .expect("표준 UUID여야 한다"),
            cursor: CommandCursor {
                expected_run_revision: 3,
                expected_state_revision: 7,
                expected_game_day: 11,
            },
            selections: LivingCostCategory::ALL
                .iter()
                .enumerate()
                .map(|(index, category)| LifeBudgetSelectionState {
                    category: *category,
                    band_id: ResourceId::from_u64(u64::try_from(index + 1).expect("범위 안이다")),
                })
                .collect(),
        }
    }

    mod context_budget_map_is_validated {
        use super::*;

        #[test]
        fn given_all_categories_when_validated_then_canonical_order_is_returned() {
            let mut selections = given_budget_command().selections;
            selections.reverse();

            let canonical = validate_and_sort_budget_selections(&selections)
                .expect("전체 예산 맵은 유효해야 한다");

            assert_eq!(
                canonical
                    .iter()
                    .map(|selection| selection.category)
                    .collect::<Vec<_>>(),
                LivingCostCategory::ALL
            );
        }

        #[test]
        fn given_a_duplicate_category_when_validated_then_it_is_rejected() {
            let mut selections = given_budget_command().selections;
            selections[8].category = LivingCostCategory::Housing;

            let result = validate_and_sort_budget_selections(&selections);

            assert!(result.is_err());
        }
    }

    mod context_life_command_is_fingerprinted {
        use super::*;

        #[test]
        fn given_the_same_budget_map_in_another_order_when_hashed_then_it_matches() {
            let command = given_budget_command();
            let mut reordered = command.clone();
            reordered.selections.reverse();

            let first = update_budget_fingerprint(&command);
            let second = update_budget_fingerprint(&reordered);

            assert_eq!(first, second);
        }

        #[test]
        fn given_a_changed_arrear_amount_when_hashed_then_it_changes() {
            let command = PayEssentialArrearCommand {
                command_id: CommandId::parse("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2")
                    .expect("표준 UUID여야 한다"),
                cursor: CommandCursor {
                    expected_run_revision: 3,
                    expected_state_revision: 7,
                    expected_game_day: 11,
                },
                arrear_id: ResourceId::from_u64(19),
                amount_krw: 10_000,
            };
            let mut changed = command.clone();
            changed.amount_krw = 10_001;

            let first = pay_arrear_fingerprint(&command);
            let second = pay_arrear_fingerprint(&changed);

            assert_ne!(first, second);
        }

        #[test]
        fn given_a_changed_loan_quote_principal_when_hashed_then_it_changes() {
            let command = CreateLoanQuoteCommand {
                command_id: CommandId::parse("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2")
                    .expect("표준 UUID여야 한다"),
                cursor: CommandCursor {
                    expected_run_revision: 3,
                    expected_state_revision: 7,
                    expected_game_day: 11,
                },
                product_version_id: ResourceId::from_u64(29),
                principal_krw: 10_000_000,
            };
            let mut changed = command.clone();
            changed.principal_krw = 10_000_001;

            let first = loan_quote_fingerprint(&command);
            let second = loan_quote_fingerprint(&changed);

            assert_ne!(first, second);
        }

        #[test]
        fn given_a_changed_loan_execution_quote_when_hashed_then_it_changes() {
            let command = ExecuteLoanCommand {
                command_id: CommandId::parse("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2")
                    .expect("표준 UUID여야 한다"),
                cursor: CommandCursor {
                    expected_run_revision: 3,
                    expected_state_revision: 7,
                    expected_game_day: 11,
                },
                quote_id: ResourceId::from_u64(29),
            };
            let mut changed = command.clone();
            changed.quote_id = ResourceId::from_u64(30);

            let first = loan_execution_fingerprint(&command);
            let second = loan_execution_fingerprint(&changed);

            assert_ne!(first, second);
        }

        #[test]
        fn given_대출id나원금이바뀔때_when_중도상환을hash하면_then_fingerprint가바뀐다() {
            let command = PrepayLoanCommand {
                command_id: CommandId::parse("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2")
                    .expect("표준 UUID여야 한다"),
                cursor: CommandCursor {
                    expected_run_revision: 3,
                    expected_state_revision: 7,
                    expected_game_day: 11,
                },
                loan_id: ResourceId::from_u64(29),
                principal_krw: 1_000_000,
            };
            let mut changed_loan = command.clone();
            changed_loan.loan_id = ResourceId::from_u64(30);
            let mut changed_principal = command.clone();
            changed_principal.principal_krw = 1_000_001;

            let fingerprint = loan_prepayment_fingerprint(&command);

            assert_ne!(fingerprint, loan_prepayment_fingerprint(&changed_loan));
            assert_ne!(fingerprint, loan_prepayment_fingerprint(&changed_principal));
        }
    }

    mod context_housing_replacement_is_applied {
        use super::*;

        #[test]
        fn given_an_exact_owner_factor_when_applied_then_the_replacement_amount_is_preserved() {
            let result = apply_tenure_replacement(LivingCostCategory::Housing, 450_000, 350_000)
                .expect("정확히 원 단위가 되는 대체계수여야 한다");

            assert_eq!(result, 157_500);
        }

        #[test]
        fn given_a_fractional_krw_factor_when_applied_then_it_is_rejected() {
            let result = apply_tenure_replacement(LivingCostCategory::Housing, 1, 350_000);

            assert!(result.is_err());
        }
    }

    mod context_living_cost_settlement_payload_is_parsed {
        use super::*;

        #[test]
        fn given_a_strict_matching_payload_when_validated_then_it_is_accepted() {
            let settlement = ScheduledSettlement {
                id: ResourceId::from_u64(17),
                run: RunId {
                    save_id: ResourceId::from_u64(3),
                    run_revision: 2,
                },
                due_game_day: 31,
                kind: SettlementKind::LivingCostMonth,
                source: SettlementSource {
                    kind: SettlementSourceKind::LivingCostMonth,
                    source_id: "23".to_owned(),
                    occurrence: 1,
                },
                status: SettlementStatus::Pending,
                payload: serde_json::json!({"version": 1, "livingCostMonthId": "23"}),
            };

            let result = validate_living_cost_settlement_envelope(&settlement);

            assert!(result.is_ok());
        }

        #[test]
        fn given_an_unknown_payload_field_when_validated_then_it_is_rejected() {
            let settlement = ScheduledSettlement {
                id: ResourceId::from_u64(17),
                run: RunId {
                    save_id: ResourceId::from_u64(3),
                    run_revision: 2,
                },
                due_game_day: 31,
                kind: SettlementKind::LivingCostMonth,
                source: SettlementSource {
                    kind: SettlementSourceKind::LivingCostMonth,
                    source_id: "23".to_owned(),
                    occurrence: 1,
                },
                status: SettlementStatus::Pending,
                payload: serde_json::json!({
                    "version": 1,
                    "livingCostMonthId": "23",
                    "extra": true,
                }),
            };

            let result = validate_living_cost_settlement_envelope(&settlement);

            assert!(result.is_err());
        }
    }
}
