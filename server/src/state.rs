use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex as StdMutex, MutexGuard, Weak};
use std::time::Duration;

use anyhow::{Context as _, Result, bail, ensure};
use async_trait::async_trait;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tokio::sync::{Mutex, broadcast, watch};
use utoipa::ToSchema;

use crate::auth::{Providers, token_hash_of};
use crate::career::{
    ActivityStatus, ArtifactKind, CareerFailureCode, EvidenceKind, Industry, LifeStatus,
};
use crate::day::{
    CommittedGameState, DailyAdvanceResult, DailyCommandAdvanceResult, DailyPipeline,
    DailyStartGameResult,
};
use crate::finance::{
    BondCatalog, BondOrderCommand, BondOrderReceipt, BondPositionSnapshot, CashProductCatalog,
    CashProductContractState, CashProductContractStatus, CashProductKind, CashRateReference,
    CloseCashProductCommand, CloseCashProductReceipt, CloseCmaAccountCommand,
    CloseCmaAccountReceipt, CmaAccountContractState, DepositProtectionState, FinanceFailureCode,
    FinancialAccountStatus, FinancialAccountType, GoldAccountSnapshot, GoldCatalog,
    GoldOrderCommand, GoldOrderReceipt, GoldWithdrawalCommand, GoldWithdrawalReceipt,
    LedgerAccountCode, LedgerPage, LedgerSourceKind, LlxDistributionEntitlementSnapshot,
    M2dAssetCommandResult, OpenCashProductCommand, OpenCashProductReceipt, OpenCmaAccountCommand,
    OpenCmaAccountReceipt, OpenGoldAccountCommand, OpenGoldAccountReceipt,
    PhysicalGoldHoldingSnapshot, ProductBundleSnapshot, ResourceId, SettlementKind,
    TransferCommand, TransferDirection,
};
use crate::life::{
    CreditBand, HousingLeaseArrearRepaymentRule, HousingLeaseCapability, HousingLeaseOfferKind,
    HousingLeaseRenewalRule, HousingLeaseRole, HousingLeaseTerminationReviewRule,
    HousingRentChargeRule, InsolvencyCaseStatus, InsolvencyEligibilityReason,
    InsolvencyEligibilityStatus, InsolvencyProcedureKind, LifeRegionKey, LivingCostCategory,
    LoanContractStatus, LoanDayCountRule, LoanLenderSector, LoanPaymentCalendar,
    LoanPrepaymentEffect, LoanProductKind, LoanProductProvenance, LoanRateReference,
    LoanRateResetRule, LoanRateStatus, LoanRateType, LoanRepaymentMethod, PropertyListingOffer,
    PropertyType, YearMonth,
};
use crate::market::{InterestRateState, MarketRegime};
use crate::runs::{PointBudgetEvaluation, PointSelection, RunOptions};
use crate::store::{
    AcceptCareerInvitationCommand, AcceptCareerOfferCommand, AccountUser,
    ActOnInsolvencyCaseCommand, ActiveHousingLeaseState, ActiveLeaseTermState,
    ActiveMilitarySavingsState, ActiveMilitaryServiceState, ActiveWelfareApplicationState,
    AdvanceCommandReceipt, AnnualTaxAssessmentState, AnnualTaxCalculatedState, AnnualTaxYearState,
    ApplyCareerCommand, ApplyWelfareProgramCommand, CancelCareerActivityCommand,
    CancelInsuranceContractCommand, CancelPropertySaleOrderCommand, CareerActivitiesState,
    CareerActivityCatalogState, CareerActivityState, CareerApplicationReceipt,
    CareerApplicationState, CareerApplicationsPageState, CareerArtifactPageQuery,
    CareerArtifactPageState, CareerArtifactState, CareerEmploymentState,
    CareerEmploymentTaxYearSource, CareerEmploymentTaxYearState, CareerEmploymentTaxYearStatus,
    CareerEvidenceState, CareerInvitationReceipt, CareerInvitationState, CareerJobState,
    CareerJobsPageQuery, CareerJobsPageState, CareerOfferReceipt, CareerPageQuery,
    CareerPayrollPageState, CareerPayrollState, CareerPendingScheduleItemState,
    CareerRewardPaymentState, CareerScheduledActionKind, CareerScheduledSettlementKind,
    CareerSpecsState, CareerStore, CareerStoreResult, CashProductStore, CashProductStoreResult,
    CloseIsaAccountCommand, CloseIsaAccountReceipt, CloseMilitarySavingsCommand,
    ConfirmCareerInterviewCommand, CorporationAvailabilityState, CorporationDividendReceipt,
    CorporationNextMonthSettingState, CorporationOperatingMonthPageState,
    CorporationOperatingMonthState, CorporationOperatingScaleState,
    CorporationOperatingSettingState, CorporationReadResult, CorporationReceipt,
    CorporationSettingsReceipt, CorporationSnapshotState, CorporationStatusState,
    CorporationSummaryState, CorporationTemplateState, CorporationTemplatesState,
    CreateCorporationCommand, CreateLeaseDepositLoanQuoteCommand, CreateLoanQuoteCommand,
    CreateMortgageQuoteCommand, CreatePropertySaleOrderCommand, CreditOverviewState,
    CreditReasonState, DeclineCareerInvitationCommand, DeclineCareerOfferCommand,
    DepositLoanExecutionReceipt, EmploymentContractState, EnrollInsuranceContractCommand,
    EssentialArrearPaymentReceipt, EssentialArrearState, ExecuteLoanCommand,
    FileInsuranceClaimCommand, FinanceStore, FinanceStoreResult, FocusCareerCommand,
    GameCommandCursor, GameCommandRejection, HousingLeaseCurrentState, HousingLeaseMoveReceipt,
    HousingListingState, HousingListingsQueryState, HousingListingsState, HousingMovingCostState,
    HousingPropertyHoldingsState, HousingPurchaseCapabilityState, HousingRateStatusState,
    HousingRegionState, InsolvencyAvailabilityState, InsolvencyCaseDetailState,
    InsolvencyCaseReceipt, InsolvencyCaseSummaryState, InsolvencyClaimPageState,
    InsolvencyClaimState, InsolvencyLiquidationPageState, InsolvencyLiquidationState,
    InsolvencyReadResult, InsolvencySnapshotState, InsolvencyWalletAssetState,
    InsuranceCancellationReceipt, InsuranceCapabilityState, InsuranceClaimAllocationState,
    InsuranceClaimHistoryState, InsuranceClaimReceipt, InsuranceContractState,
    InsuranceContractStatusState, InsuranceEligibilityReasonState, InsuranceEligibilityStatusState,
    InsuranceEnrollmentReceipt, InsuranceProductState, InsuranceQueryState, InsuranceReadResult,
    InsuranceState, IsaAccountState, LeaseArrearPaymentReceipt, LeaseArrearState,
    LeaseDepositLoanAffordabilityState, LeaseDepositLoanQuoteDecisionState,
    LeaseDepositLoanQuoteReasonState, LeaseDepositLoanQuoteReceipt, LeaseLifecycleTermsState,
    LeaseRenewalNoticeState, LeaseTerminationReviewState, LeaseTerminationReviewStatusState,
    LifeBudgetBandState, LifeBudgetSelectionState, LifeBudgetState, LifeEventCapabilityState,
    LifeEventChoiceReceipt, LifeEventChoiceState, LifeEventDecisionKindState,
    LifeEventEffectSummaryState, LifeEventHistoryItemState, LifeEventResolutionKindState,
    LifeEventsQueryState, LifeEventsReadResult, LifeEventsState, LifeFailureCode,
    LifeHouseholdState, LifeRateStatus, LifeResidenceState, LifeSnapshotState, LifeStore,
    LifeStoreResult, LivingCostMonthItemState, LivingCostMonthState, LoanDetailState,
    LoanExecutionReceipt, LoanInstallmentPageQuery, LoanInstallmentPageState, LoanInstallmentState,
    LoanInstallmentStatusState, LoanPaymentAllocationKindState, LoanPaymentAllocationState,
    LoanPaymentKindState, LoanPaymentState, LoanPrepaymentReceipt, LoanPrepaymentStatusState,
    LoanProductCatalogState, LoanProductState, LoanQuoteDecisionState, LoanQuoteDsrState,
    LoanQuoteFirstInstallmentState, LoanQuoteLtvState, LoanQuoteReasonState, LoanQuoteReceipt,
    LoanQuotedTermsState, LoanSummaryState, M2dAssetStore, ManualAdvanceCommand, MarketStore,
    MilitaryCompensationKind, MilitaryOptionIneligibilityReason, MilitaryOptionState,
    MilitaryOptionsState, MilitarySavingsClosureReason, MilitarySavingsCommandReceipt,
    MilitarySavingsContractStatus, MilitarySavingsDayCountConvention,
    MilitarySavingsHistoryItemState, MilitarySavingsIneligibilityReason,
    MilitarySavingsInstallmentState, MilitarySavingsInstallmentStatusState,
    MilitarySavingsInterestRounding, MilitarySavingsInterestTierState,
    MilitarySavingsMaturityProjectionState, MilitarySavingsPageState, MilitarySavingsProductState,
    MilitarySavingsProductsState, MilitarySavingsProjectionAssumption,
    MilitaryServiceCommandReceipt, MilitaryServiceHistoryState, MilitaryServiceSourceKind,
    MilitaryServiceState, MonthlyRentTerminationReviewTermsState, MonthlyRentTermsState,
    MortgageExecutionReceipt, MortgageLtvRegionClassState, MortgageQuoteDecisionState,
    MortgageQuoteReasonState, MortgageQuoteReceipt, MortgageStressTreatmentState,
    NextLoanInstallmentState, OpenMilitarySavingsCommand, OpenTaxAccountCommand,
    OpenTaxAccountReceipt, PayCorporationDividendCommand, PayEssentialArrearCommand,
    PayLeaseArrearCommand, PendingInsuranceClaimState, PendingLifeEventState, PensionAccountState,
    PensionWithdrawalCommand, PensionWithdrawalReceipt, PrepareInsolvencyCaseCommand,
    PrepayLoanCommand, PropertyHoldingPurposeState, PropertyHoldingState,
    PropertyHoldingStatusState, PropertyPurchaseReceipt, PropertySaleExecutionState,
    PropertySaleOrderCancellationReceipt, PropertySaleOrderListingReceipt,
    PropertySaleOrderPageQuery, PropertySaleOrderPageState, PropertySaleOrderRejectionReasonState,
    PropertySaleOrderRevisionKindState, PropertySaleOrderStatusState,
    PropertySaleOrderSummaryState, PropertyTaxComponentState, PropertyTaxEventKindState,
    PropertyTaxEventPageQuery, PropertyTaxEventPageState, PropertyTaxEventState,
    PropertyTaxEventStatusState, PropertyTaxPaymentState, PropertyTaxPaymentStatusState,
    PublishCareerArtifactCommand, PurchasePropertyCommand, RepaidDepositLoanReceipt,
    RepricePropertySaleOrderCommand, ResidenceTenureKind, ResolveLifeEventCommand, RunStore,
    StartCareerActivityCommand, StartGameCommand, StartGameReceipt, StartHousingLeaseCommand,
    StartMilitaryServiceCommand, StartPensionCommand, StartPensionReceipt, TaxAccountStore,
    TaxAccountStoreResult, TradeStoreResult, TradingStore, UpdateCorporationSettingsCommand,
    UpdateLifeBudgetCommand, UpdateLifeBudgetReceipt, UserStore, VerifiedIncomeSourceState,
    WelfareApplicationReceipt, WelfareApplicationStatusState, WelfareApplicationSummaryState,
    WelfareConditionOutcomeState, WelfareConditionResultState, WelfareEvaluationStatusState,
    WelfarePaymentState, WelfarePaymentStatusState, WelfareProgramState, WelfareProgramsState,
    WithdrawCareerApplicationCommand,
};
use crate::trading::{
    Portfolio, TradeExecution, TradeFailure, TradeOrder, checked_net_worth_krw, value_portfolio,
};

const MAX_SAFE_JSON_INTEGER: i64 = 9_007_199_254_740_991;

/// Supported online automatic speeds. Values are numeric in both JSON and OpenAPI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToSchema)]
#[repr(u8)]
pub enum AutoSpeed {
    X1 = 1,
    X2 = 2,
    X4 = 4,
    X8 = 8,
}

impl AutoSpeed {
    pub const fn interval(self) -> Duration {
        match self {
            Self::X1 => Duration::from_millis(500),
            Self::X2 => Duration::from_millis(250),
            Self::X4 => Duration::from_millis(125),
            Self::X8 => Duration::from_millis(62),
        }
    }
}

impl TryFrom<u8> for AutoSpeed {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::X1),
            2 => Ok(Self::X2),
            4 => Ok(Self::X4),
            8 => Ok(Self::X8),
            _ => Err("지원하지 않는 게임 속도입니다"),
        }
    }
}

impl Serialize for AutoSpeed {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for AutoSpeed {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        Self::try_from(value).map_err(serde::de::Error::custom)
    }
}

/// The game state sent to a client.
///
/// Carries the start date plus elapsed days rather than a formatted date: the
/// calculation is deterministic, so letting the client do it costs no authority.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GameSnapshot {
    pub run_revision: u32,
    pub state_revision: u64,
    pub game_day: u32,
    pub start_date: String,
    pub cash_krw: i64,
    pub debt_krw: i64,
    pub net_worth_krw: i64,
    /// `None` until a character exists; the client routes to creation.
    #[schema(required = true)]
    pub character_name: Option<String>,
    /// Runtime-only control state. It deliberately does not live in the database.
    #[schema(required = true)]
    pub auto_speed: Option<AutoSpeed>,
    pub market: MarketSnapshot,
    pub portfolio: Portfolio,
    pub finance: FinanceSnapshot,
    pub career: CareerSnapshot,
    pub life: LifeSnapshot,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LifeRateStatusSnapshot {
    Active,
    RateUnavailable,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ResidenceTenureKindSnapshot {
    RentFree,
    Owner,
    Jeonse,
    MonthlyRent,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LivingCostCategorySnapshot {
    Housing,
    Food,
    Transport,
    Communication,
    Utilities,
    Healthcare,
    Education,
    DependentCare,
    Discretionary,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct YearMonthSnapshot {
    #[schema(minimum = 1, maximum = 9999)]
    pub year: i32,
    #[schema(minimum = 1, maximum = 12)]
    pub month: u8,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LifeHouseholdSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(minimum = 1)]
    pub member_count: u32,
    pub dependent_count: u32,
    pub tax_dependent_eligible_count: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LifeResidenceSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(min_length = 1, max_length = 64)]
    pub region_key: String,
    pub tenure_kind: ResidenceTenureKindSnapshot,
    #[schema(required = true, nullable, value_type = Option<String>)]
    pub property_holding_id: Option<ResourceId>,
    pub effective_from_game_day: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LifeBudgetBandSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(min_length = 1, max_length = 64)]
    pub band_key: String,
    #[schema(min_length = 1, max_length = 120)]
    pub display_name: String,
    #[schema(minimum = 1)]
    pub factor_ppm: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LifeBudgetSelectionSnapshot {
    pub category: LivingCostCategorySnapshot,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub band_id: ResourceId,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LivingCostMonthItemSnapshot {
    pub category: LivingCostCategorySnapshot,
    pub essential: bool,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub band_id: ResourceId,
    #[schema(minimum = 0)]
    pub base_monthly_krw: i64,
    #[schema(minimum = 1)]
    pub base_cpi_index: i64,
    #[schema(minimum = 1)]
    pub region_factor_ppm: i64,
    #[schema(minimum = 1)]
    pub household_factor_ppm: i64,
    #[schema(minimum = 1)]
    pub budget_factor_ppm: i64,
    #[schema(minimum = 0, maximum = 1000000)]
    pub tenure_replacement_factor_ppm: i64,
    #[schema(minimum = 0)]
    pub gross_krw: i64,
    #[schema(minimum = 0)]
    pub paid_krw: i64,
    #[schema(minimum = 0)]
    pub arrear_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LivingCostMonthSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub profile_id: ResourceId,
    #[schema(min_length = 1, max_length = 96)]
    pub profile_key: String,
    #[schema(minimum = 1)]
    pub current_cpi_index: i64,
    #[schema(minimum = 377580, maximum = 377580)]
    pub proration_scale: u32,
    #[schema(minimum = 1)]
    pub proration_units: u32,
    #[schema(minimum = 1, maximum = 31)]
    pub proration_days: u8,
    #[schema(minimum = 28, maximum = 31)]
    pub days_in_month: u8,
    pub year_month: YearMonthSnapshot,
    pub activation_game_day: u32,
    pub settlement_game_day: u32,
    pub settled: bool,
    #[schema(minimum = 0)]
    pub total_gross_krw: i64,
    #[schema(minimum = 0)]
    pub total_paid_krw: i64,
    #[schema(minimum = 0)]
    pub total_arrear_krw: i64,
    #[schema(min_items = 9, max_items = 9)]
    pub items: Vec<LivingCostMonthItemSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EssentialArrearSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    pub due_year_month: YearMonthSnapshot,
    pub category: LivingCostCategorySnapshot,
    #[schema(minimum = 1)]
    pub original_krw: i64,
    #[schema(minimum = 1)]
    pub remaining_krw: i64,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum CreditBandSnapshot {
    Prime,
    Standard,
    Limited,
    Distressed,
    Insolvent,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum CreditReasonSnapshot {
    ModelUnavailable,
    ActiveDefault,
    ActiveDelinquency,
    CleanHistory,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LoanProductKindSnapshot {
    StudentLoan,
    UnsecuredLoan,
    LeaseDepositLoan,
    Mortgage,
    LegacyDebt,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LoanRateStatusSnapshot {
    Available,
    RateUnavailable,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LoanLenderSectorSnapshot {
    Bank,
    NonBank,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LoanRateTypeSnapshot {
    Fixed,
    Variable,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LoanRateReferenceSnapshot {
    Treasury3m,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LoanRateResetRuleSnapshot {
    None,
    MonthlyDay1,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LoanDayCountRuleSnapshot {
    Actual365,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LoanRepaymentMethodSnapshot {
    EqualPrincipal,
    LevelPayment,
    Bullet,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LoanPaymentCalendarSnapshot {
    MonthEnd,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LoanPrepaymentEffectSnapshot {
    ReduceTerm,
    RecalculatePayment,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LoanProductProvenanceSnapshot {
    GameBalance,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanProductSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(min_length = 1, max_length = 96)]
    pub key: String,
    #[schema(min_length = 1, max_length = 80)]
    pub display_name: String,
    pub kind: LoanProductKindSnapshot,
    pub lender_sector: LoanLenderSectorSnapshot,
    pub rate_status: LoanRateStatusSnapshot,
    pub rate_type: LoanRateTypeSnapshot,
    #[schema(required = true, nullable, minimum = 0)]
    pub current_annual_rate_bp: Option<i64>,
    #[schema(required = true, nullable)]
    pub reference_rate_key: Option<LoanRateReferenceSnapshot>,
    #[schema(required = true, nullable, minimum = -10000, maximum = 10000)]
    pub spread_bp: Option<i64>,
    #[schema(minimum = 0)]
    pub minimum_annual_rate_bp: i64,
    #[schema(minimum = 0)]
    pub maximum_annual_rate_bp: i64,
    pub rate_reset_rule: LoanRateResetRuleSnapshot,
    pub day_count_rule: LoanDayCountRuleSnapshot,
    pub repayment_method: LoanRepaymentMethodSnapshot,
    #[schema(minimum = 1)]
    pub term_months: u16,
    pub payment_calendar: LoanPaymentCalendarSnapshot,
    pub grace_months: u16,
    #[schema(minimum = 1)]
    pub minimum_principal_krw: i64,
    #[schema(minimum = 1)]
    pub maximum_principal_krw: i64,
    #[schema(maximum = 1000000)]
    pub prepayment_fee_ppm: u32,
    pub prepayment_effect: LoanPrepaymentEffectSnapshot,
    pub starting_eligible: bool,
    pub quote_eligible: bool,
    pub execution_eligible: bool,
    pub prepayment_allowed: bool,
    pub dsr_included: bool,
    pub provenance: LoanProductProvenanceSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanProductCatalogResponse {
    #[schema(required = true, nullable, value_type = Option<String>)]
    pub credit_model_version_id: Option<ResourceId>,
    #[schema(max_items = 16)]
    pub products: Vec<LoanProductSnapshot>,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LoanContractStatusSnapshot {
    Pending,
    Active,
    Delinquent,
    Defaulted,
    PaidOff,
    Restructured,
    Discharged,
    ChargedOff,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanSummarySnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub product_version_id: ResourceId,
    pub product_kind: LoanProductKindSnapshot,
    #[schema(min_length = 1, max_length = 80)]
    pub display_name: String,
    pub rate_status: LoanRateStatusSnapshot,
    #[schema(required = true, nullable, minimum = 0)]
    pub current_annual_rate_bp: Option<i64>,
    pub status: LoanContractStatusSnapshot,
    #[schema(minimum = 0)]
    pub remaining_principal_krw: i64,
    #[schema(minimum = 0)]
    pub overdue_krw: i64,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanDetailResponse {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub product_version_id: ResourceId,
    pub product_kind: LoanProductKindSnapshot,
    #[schema(min_length = 1, max_length = 80)]
    pub display_name: String,
    pub rate_status: LoanRateStatusSnapshot,
    #[schema(required = true, nullable, minimum = 0)]
    pub current_annual_rate_bp: Option<i64>,
    pub status: LoanContractStatusSnapshot,
    pub read_only: bool,
    #[schema(minimum = 1)]
    pub original_principal_krw: i64,
    #[schema(minimum = 0)]
    pub remaining_principal_krw: i64,
    #[schema(minimum = 0)]
    pub accrued_interest_krw: i64,
    #[schema(minimum = 0)]
    pub accrued_fee_krw: i64,
    #[schema(minimum = 0)]
    pub overdue_krw: i64,
    pub repayment_method: LoanRepaymentMethodSnapshot,
    #[schema(required = true, nullable, minimum = 1)]
    pub term_months: Option<u16>,
    #[schema(required = true, nullable, minimum = 1)]
    pub total_installments: Option<u16>,
    pub activated_game_day: u32,
    #[schema(required = true, nullable)]
    pub maturity_game_day: Option<u32>,
    #[schema(required = true, nullable)]
    pub final_installment_due_game_day: Option<u32>,
    #[schema(required = true, nullable, minimum = 1)]
    pub next_installment_no: Option<u16>,
    #[schema(required = true, nullable)]
    pub oldest_unpaid_due_game_day: Option<u32>,
    pub prepayment_allowed: bool,
    #[schema(required = true, nullable, maximum = 1000000)]
    pub prepayment_fee_ppm: Option<u32>,
    #[schema(required = true, nullable)]
    pub prepayment_effect: Option<LoanPrepaymentEffectSnapshot>,
    pub dsr_included: bool,
    #[schema(required = true, nullable, value_type = Option<String>)]
    pub lease_contract_id: Option<ResourceId>,
    #[schema(required = true, nullable, value_type = Option<String>)]
    pub property_holding_id: Option<ResourceId>,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LoanInstallmentStatusSnapshot {
    Pending,
    Due,
    PartiallyPaid,
    Paid,
    Cancelled,
    Discharged,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanInstallmentSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(minimum = 1)]
    pub installment_no: u16,
    pub due_game_day: u32,
    pub interest_period_start_game_day: u32,
    #[schema(minimum = 1)]
    pub elapsed_days: u16,
    #[schema(minimum = 0)]
    pub annual_rate_bp: i64,
    #[schema(minimum = 1)]
    pub opening_principal_krw: i64,
    #[schema(minimum = 0)]
    pub scheduled_fee_krw: i64,
    #[schema(minimum = 0)]
    pub scheduled_interest_krw: i64,
    #[schema(minimum = 0)]
    pub scheduled_principal_krw: i64,
    #[schema(minimum = 0)]
    pub paid_fee_krw: i64,
    #[schema(minimum = 0)]
    pub paid_interest_krw: i64,
    #[schema(minimum = 0)]
    pub paid_principal_krw: i64,
    #[schema(minimum = 0)]
    pub remaining_due_krw: i64,
    pub status: LoanInstallmentStatusSnapshot,
    #[schema(minimum = 1)]
    pub schedule_revision: u32,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LoanPaymentKindSnapshot {
    ScheduledInstallment,
    ManualPrepayment,
    LeaseMovePayoff,
    PropertySalePayoff,
    InsolvencyDistribution,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LoanPaymentAllocationKindSnapshot {
    OverdueFee,
    OverdueInterest,
    OverduePrincipal,
    CurrentFee,
    CurrentInterest,
    CurrentPrincipal,
    PrepaymentFee,
    PrepaymentPrincipal,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanPaymentAllocationSnapshot {
    pub kind: LoanPaymentAllocationKindSnapshot,
    #[schema(minimum = 1)]
    pub amount_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanPaymentSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(minimum = 1)]
    pub payment_no: u32,
    pub kind: LoanPaymentKindSnapshot,
    pub game_day: u32,
    #[schema(minimum = 1)]
    pub amount_krw: i64,
    #[schema(min_items = 1, max_items = 8)]
    pub allocations: Vec<LoanPaymentAllocationSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanInstallmentsResponse {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub loan_id: ResourceId,
    #[schema(max_items = 50)]
    pub installments: Vec<LoanInstallmentSnapshot>,
    #[schema(max_items = 50)]
    pub payments: Vec<LoanPaymentSnapshot>,
    pub has_more_installments: bool,
    pub has_more_payments: bool,
    #[schema(
        required = true,
        nullable,
        min_length = 11,
        max_length = 43,
        pattern = "^v1\\.l[1-9][0-9]*\\.i(?:0|[1-9][0-9]*)\\.p(?:0|[1-9][0-9]*)$"
    )]
    pub next_before: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NextLoanInstallmentSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub loan_id: ResourceId,
    #[schema(minimum = 1)]
    pub installment_no: u16,
    pub due_game_day: u32,
    #[schema(minimum = 0)]
    pub fee_krw: i64,
    #[schema(minimum = 0)]
    pub interest_krw: i64,
    #[schema(minimum = 0)]
    pub principal_krw: i64,
    #[schema(minimum = 0)]
    pub remaining_due_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreditResponse {
    #[schema(required = true, nullable)]
    pub credit_band: Option<CreditBandSnapshot>,
    #[schema(max_items = 8)]
    pub credit_reasons: Vec<CreditReasonSnapshot>,
    #[schema(max_items = 8)]
    pub active_loans: Vec<LoanSummarySnapshot>,
    #[schema(required = true, nullable)]
    pub next_loan_installment: Option<NextLoanInstallmentSnapshot>,
    #[schema(minimum = 0)]
    pub total_loan_balance_krw: i64,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LoanQuoteDecisionSnapshot {
    Eligible,
    DebtServiceLimit,
    IncomeUnavailable,
    CreditRestricted,
    ValuationUnavailable,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LoanQuoteReasonSnapshot {
    InsolvencyRebuilding,
    ActiveDefault,
    ActiveDelinquency,
    ActiveRestructuring,
    CreditBandRestricted,
    ActiveLoanLimit,
    IncomeUnavailable,
    DebtServiceLimit,
    Eligible,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum VerifiedIncomeSourceSnapshot {
    ActiveEmploymentContract,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanQuoteDsrSnapshot {
    #[schema(minimum = 0)]
    pub numerator_krw: i64,
    #[schema(minimum = 1)]
    pub denominator_krw: i64,
    #[schema(minimum = 0)]
    pub ratio_ppm: i64,
    #[schema(minimum = 0)]
    pub limit_ppm: i64,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanQuoteFirstInstallmentSnapshot {
    pub due_game_day: u32,
    #[schema(minimum = 0)]
    pub fee_krw: i64,
    #[schema(minimum = 0)]
    pub principal_krw: i64,
    #[schema(minimum = 0)]
    pub interest_krw: i64,
    #[schema(minimum = 0)]
    pub total_krw: i64,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanQuotedTermsSnapshot {
    #[schema(minimum = 0, maximum = 20000)]
    pub annual_rate_bp: i64,
    pub repayment_method: LoanRepaymentMethodSnapshot,
    #[schema(minimum = 1)]
    pub term_months: u16,
    pub first_installment: LoanQuoteFirstInstallmentSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanQuoteResultSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub quote_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub product_version_id: ResourceId,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub requested_principal_krw: i64,
    pub created_game_day: u32,
    pub expires_game_day: u32,
    pub decision_code: LoanQuoteDecisionSnapshot,
    #[schema(min_items = 1, max_items = 8)]
    pub decision_reasons: Vec<LoanQuoteReasonSnapshot>,
    #[schema(required = true, nullable, minimum = 1)]
    pub verified_annual_income_krw: Option<i64>,
    #[schema(required = true, nullable)]
    pub verified_income_source: Option<VerifiedIncomeSourceSnapshot>,
    #[schema(minimum = 0)]
    pub existing_loan_balance_krw: i64,
    #[schema(minimum = 1)]
    pub post_execution_balance_krw: i64,
    pub dsr_applied: bool,
    #[schema(required = true, nullable)]
    pub dsr: Option<LoanQuoteDsrSnapshot>,
    #[schema(minimum = 0, maximum = 0)]
    pub stress_rate_bp: i64,
    pub quoted_terms: LoanQuotedTermsSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanQuoteResponse {
    pub result: LoanQuoteResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LeaseDepositLoanQuoteDecisionSnapshot {
    Eligible,
    CreditRestricted,
    CollateralLimit,
    IncomeUnavailable,
    AffordabilityLimit,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LeaseDepositLoanQuoteReasonSnapshot {
    InsolvencyRebuilding,
    ActiveDefault,
    ActiveDelinquency,
    ActiveRestructuring,
    CreditBandRestricted,
    ActiveLoanLimit,
    CollateralLimit,
    IncomeUnavailable,
    AffordabilityLimit,
    Eligible,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LeaseDepositLoanAffordabilitySnapshot {
    #[schema(minimum = 0)]
    pub numerator_krw: i64,
    #[schema(minimum = 1)]
    pub denominator_krw: i64,
    #[schema(minimum = 0)]
    pub ratio_ppm: i64,
    #[schema(minimum = 1)]
    pub limit_ppm: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegulatoryDsrAppliedSnapshot;

impl Serialize for RegulatoryDsrAppliedSnapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bool(false)
    }
}

impl utoipa::PartialSchema for RegulatoryDsrAppliedSnapshot {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::Boolean)
            .enum_values(Some([false]))
            .into()
    }
}

impl ToSchema for RegulatoryDsrAppliedSnapshot {}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LeaseDepositLoanQuoteResultSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub quote_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub listing_id: ResourceId,
    pub offer_kind: JeonseHousingLeaseOfferKindSnapshot,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub product_version_id: ResourceId,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub requested_principal_krw: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub deposit_krw: i64,
    #[schema(minimum = 1, maximum = 1000000)]
    pub funding_limit_ppm: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub maximum_funding_krw: i64,
    pub created_game_day: u32,
    pub expires_game_day: u32,
    pub decision_code: LeaseDepositLoanQuoteDecisionSnapshot,
    #[schema(min_items = 1, max_items = 8)]
    pub decision_reasons: Vec<LeaseDepositLoanQuoteReasonSnapshot>,
    #[schema(required = true, nullable, minimum = 1)]
    pub verified_annual_income_krw: Option<i64>,
    #[schema(required = true, nullable)]
    pub verified_income_source: Option<VerifiedIncomeSourceSnapshot>,
    #[schema(minimum = 0)]
    pub existing_loan_balance_krw: i64,
    #[schema(minimum = 1)]
    pub post_execution_balance_krw: i64,
    pub regulatory_dsr_applied: RegulatoryDsrAppliedSnapshot,
    #[schema(required = true, nullable)]
    pub affordability: Option<LeaseDepositLoanAffordabilitySnapshot>,
    pub quoted_terms: LoanQuotedTermsSnapshot,
    #[schema(required = true, nullable, value_type = Option<String>)]
    pub replaced_loan_id: Option<ResourceId>,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub replaced_loan_principal_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LeaseDepositLoanQuoteResponse {
    pub result: LeaseDepositLoanQuoteResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanExecutionResultSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub loan_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub quote_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub product_version_id: ResourceId,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub principal_krw: i64,
    pub activated_game_day: u32,
    pub maturity_game_day: u32,
    #[schema(minimum = 0, maximum = 20000)]
    pub annual_rate_bp: i64,
    pub repayment_method: LoanRepaymentMethodSnapshot,
    #[schema(minimum = 1)]
    pub term_months: u16,
    pub first_installment: LoanQuoteFirstInstallmentSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanExecutionResponse {
    pub result: LoanExecutionResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LoanPrepaymentStatusSnapshot {
    Active,
    PaidOff,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanPrepaymentNextInstallmentSnapshot {
    #[schema(minimum = 1)]
    pub installment_no: u16,
    pub due_game_day: u32,
    #[schema(minimum = 0)]
    pub fee_krw: i64,
    #[schema(minimum = 0)]
    pub principal_krw: i64,
    #[schema(minimum = 0)]
    pub interest_krw: i64,
    #[schema(minimum = 0)]
    pub total_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanPrepaymentResultSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub loan_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub payment_id: ResourceId,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub principal_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub fee_krw: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub total_debited_krw: i64,
    pub applied_game_day: u32,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub remaining_principal_krw: i64,
    pub status: LoanPrepaymentStatusSnapshot,
    pub prepayment_effect: LoanPrepaymentEffectSnapshot,
    pub remaining_installments: u16,
    #[schema(required = true, nullable)]
    pub next_installment: Option<LoanPrepaymentNextInstallmentSnapshot>,
    #[schema(required = true, nullable)]
    pub final_installment_due_game_day: Option<u32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanPrepaymentResponse {
    pub result: LoanPrepaymentResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum WelfareEvaluationStatusSnapshot {
    Eligible,
    Ineligible,
    Indeterminate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum WelfareConditionOutcomeSnapshot {
    Passed,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum WelfareApplicationStatusSnapshot {
    Applied,
    Approved,
    Rejected,
    Active,
    Exhausted,
    Terminated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ActiveWelfareApplicationStatusSnapshot {
    Active,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum WelfarePaymentStatusSnapshot {
    Pending,
    Paid,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WelfareConditionResultSnapshot {
    #[schema(min_length = 1, max_length = 64, pattern = "^[a-z][a-zA-Z0-9]{0,63}$")]
    pub code: String,
    #[schema(min_length = 1, max_length = 120)]
    pub label: String,
    pub outcome: WelfareConditionOutcomeSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WelfarePaymentSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(minimum = 1, maximum = 65535)]
    pub payment_no: u16,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub amount_krw: i64,
    pub due_game_day: u32,
    pub status: WelfarePaymentStatusSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WelfareApplicationSummarySnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    pub status: WelfareApplicationStatusSnapshot,
    pub application_game_day: u32,
    #[schema(required = true, nullable)]
    pub approval_game_day: Option<u32>,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub paid_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WelfareProgramSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(min_length = 1, max_length = 64, pattern = "^[a-z][a-zA-Z0-9]{0,63}$")]
    pub program_key: String,
    #[schema(min_length = 1, max_length = 120)]
    pub display_name: String,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub benefit_krw: i64,
    #[schema(minimum = 1, maximum = 365)]
    pub payment_delay_game_days: u16,
    pub evaluation_status: WelfareEvaluationStatusSnapshot,
    #[schema(min_length = 64, max_length = 64, pattern = "^[0-9a-f]{64}$")]
    pub fact_fingerprint: String,
    #[schema(min_items = 1, max_items = 32)]
    pub conditions: Vec<WelfareConditionResultSnapshot>,
    pub application_available: bool,
    #[schema(required = true, nullable)]
    pub latest_application: Option<WelfareApplicationSummarySnapshot>,
    #[schema(required = true, nullable)]
    pub next_payment: Option<WelfarePaymentSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WelfareProgramsResponse {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub component_version_id: ResourceId,
    pub game_day: u32,
    #[schema(max_items = 16)]
    pub programs: Vec<WelfareProgramSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActiveWelfareApplicationSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub application_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub program_version_id: ResourceId,
    #[schema(min_length = 1, max_length = 64, pattern = "^[a-z][a-zA-Z0-9]{0,63}$")]
    pub program_key: String,
    #[schema(min_length = 1, max_length = 120)]
    pub display_name: String,
    pub status: ActiveWelfareApplicationStatusSnapshot,
    pub application_game_day: u32,
    pub approval_game_day: u32,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub benefit_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub paid_krw: i64,
    #[schema(required = true, nullable)]
    pub next_payment: Option<WelfarePaymentSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WelfareApplicationResultSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub application_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub program_version_id: ResourceId,
    pub status: ActiveWelfareApplicationStatusSnapshot,
    pub application_game_day: u32,
    pub approval_game_day: u32,
    #[schema(min_items = 1, max_items = 32)]
    pub eligibility_at_application: Vec<WelfareConditionResultSnapshot>,
    pub payment: WelfarePaymentSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WelfareApplicationResponse {
    pub result: WelfareApplicationResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LifeEventCapabilitySnapshot {
    DeterministicChoices,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum InsuranceCapabilitySnapshot {
    ContractsAndClaims,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum InsuranceEligibilityStatusSnapshot {
    Eligible,
    Ineligible,
    Indeterminate,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum InsuranceEligibilityReasonSnapshot {
    AgeOutsideRange,
    DependentRequired,
    ResidenceRequired,
    MilitaryServing,
    AuthorityUnavailable,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum InsuranceContractStatusSnapshot {
    Active,
    Lapsed,
    Expired,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LifeEventDecisionKindSnapshot {
    Accepted,
    Declined,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LifeEventResolutionKindSnapshot {
    Accepted,
    Declined,
    Expired,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LifeEventEffectSummarySnapshot {
    NoEffect,
    WalletExpense {
        #[schema(minimum = 1, maximum = 9007199254740991_i64)]
        amount_krw: i64,
    },
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LifeEventChoiceSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(min_length = 1, max_length = 120)]
    pub display_name: String,
    pub decision_kind: LifeEventDecisionKindSnapshot,
    pub effect_summary: LifeEventEffectSummarySnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PendingLifeEventSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(min_length = 1, max_length = 64, pattern = "^[a-z][a-zA-Z0-9]{0,63}$")]
    pub event_key: String,
    #[schema(min_length = 1, max_length = 80)]
    pub display_name: String,
    pub offered_game_day: u32,
    pub expires_game_day: u32,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub default_choice_id: ResourceId,
    #[schema(min_items = 2, max_items = 8)]
    pub choices: Vec<LifeEventChoiceSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LifeEventHistoryItemSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(min_length = 1, max_length = 64, pattern = "^[a-z][a-zA-Z0-9]{0,63}$")]
    pub event_key: String,
    #[schema(min_length = 1, max_length = 80)]
    pub display_name: String,
    pub offered_game_day: u32,
    pub resolved_game_day: u32,
    pub resolution_kind: LifeEventResolutionKindSnapshot,
    pub choice: LifeEventChoiceSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LifeEventsResponse {
    pub life_event_capability: LifeEventCapabilitySnapshot,
    pub insurance_capability: InsuranceCapabilitySnapshot,
    #[schema(max_items = 8)]
    pub pending_events: Vec<PendingLifeEventSnapshot>,
    #[schema(max_items = 20)]
    pub history: Vec<LifeEventHistoryItemSnapshot>,
    #[schema(required = true, nullable, max_length = 512)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LifeEventChoiceResultSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub event_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub choice_id: ResourceId,
    pub resolution_kind: LifeEventDecisionKindSnapshot,
    pub resolved_game_day: u32,
    #[schema(minimum = -9007199254740991_i64, maximum = 0)]
    pub wallet_delta_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LifeEventChoiceResponse {
    pub result: LifeEventChoiceResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsuranceProductSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(min_length = 1, max_length = 64, pattern = "^[a-z][a-zA-Z0-9]{0,63}$")]
    pub product_key: String,
    #[schema(min_length = 1, max_length = 80)]
    pub display_name: String,
    pub eligibility_status: InsuranceEligibilityStatusSnapshot,
    #[schema(max_items = 8)]
    pub reasons: Vec<InsuranceEligibilityReasonSnapshot>,
    #[schema(min_length = 1, max_length = 64, pattern = "^[a-z][a-zA-Z0-9]{0,63}$")]
    pub covered_event_key: String,
    #[schema(min_length = 1, max_length = 80)]
    pub covered_event_display_name: String,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub premium_krw: i64,
    pub premium_interval_game_days: u16,
    pub term_game_days: u16,
    pub waiting_period_game_days: u16,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub deductible_krw: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub occurrence_limit_krw: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub term_limit_krw: i64,
    pub claim_window_game_days: u16,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsuranceContractSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub product_version_id: ResourceId,
    #[schema(min_length = 1, max_length = 64, pattern = "^[a-z][a-zA-Z0-9]{0,63}$")]
    pub product_key: String,
    #[schema(min_length = 1, max_length = 80)]
    pub display_name: String,
    pub status: InsuranceContractStatusSnapshot,
    pub coverage_start_game_day: u32,
    pub waiting_ends_game_day: u32,
    pub coverage_end_exclusive: u32,
    #[schema(required = true, nullable)]
    pub next_premium_due_game_day: Option<u32>,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub premium_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub paid_benefit_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub reserved_benefit_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub remaining_benefit_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsuranceClaimAllocationSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub contract_id: ResourceId,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub deductible_krw: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub payout_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PendingInsuranceClaimSnapshot {
    Candidate {
        #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
        id: ResourceId,
        #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
        event_id: ResourceId,
        #[schema(min_length = 1, max_length = 64)]
        event_key: String,
        #[schema(min_length = 1, max_length = 80)]
        event_display_name: String,
        offered_game_day: u32,
        #[schema(required = true, nullable)]
        gross_cost_krw: Option<i64>,
        #[schema(required = true, nullable)]
        payout_krw: Option<i64>,
        #[schema(required = true, nullable)]
        filing_deadline_game_day: Option<u32>,
    },
    Ready {
        #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
        id: ResourceId,
        #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
        event_id: ResourceId,
        #[schema(min_length = 1, max_length = 64)]
        event_key: String,
        #[schema(min_length = 1, max_length = 80)]
        event_display_name: String,
        offered_game_day: u32,
        #[schema(minimum = 1, maximum = 9007199254740991_i64)]
        gross_cost_krw: i64,
        #[schema(minimum = 1, maximum = 9007199254740991_i64)]
        payout_krw: i64,
        filing_deadline_game_day: u32,
        #[schema(min_items = 1, max_items = 8)]
        contract_allocations: Vec<InsuranceClaimAllocationSnapshot>,
    },
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum InsuranceClaimHistoryItemSnapshot {
    NotApplicable {
        #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
        id: ResourceId,
        #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
        event_id: ResourceId,
        event_key: String,
        event_display_name: String,
        offered_game_day: u32,
        resolved_game_day: u32,
        #[schema(required = true, nullable)]
        gross_cost_krw: Option<i64>,
        #[schema(required = true, nullable)]
        payout_krw: Option<i64>,
        #[schema(required = true, nullable)]
        filing_deadline_game_day: Option<u32>,
    },
    NotCovered {
        #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
        id: ResourceId,
        #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
        event_id: ResourceId,
        event_key: String,
        event_display_name: String,
        offered_game_day: u32,
        resolved_game_day: u32,
        #[schema(minimum = 1, maximum = 9007199254740991_i64)]
        gross_cost_krw: i64,
        #[schema(minimum = 0, maximum = 0)]
        payout_krw: i64,
        #[schema(required = true, nullable)]
        filing_deadline_game_day: Option<u32>,
    },
    Paid {
        #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
        id: ResourceId,
        #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
        event_id: ResourceId,
        event_key: String,
        event_display_name: String,
        offered_game_day: u32,
        resolved_game_day: u32,
        #[schema(minimum = 1, maximum = 9007199254740991_i64)]
        gross_cost_krw: i64,
        #[schema(minimum = 1, maximum = 9007199254740991_i64)]
        payout_krw: i64,
        filing_deadline_game_day: u32,
        paid_game_day: u32,
        #[schema(min_items = 1, max_items = 8)]
        contract_allocations: Vec<InsuranceClaimAllocationSnapshot>,
    },
    Expired {
        #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
        id: ResourceId,
        #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
        event_id: ResourceId,
        event_key: String,
        event_display_name: String,
        offered_game_day: u32,
        resolved_game_day: u32,
        #[schema(minimum = 1, maximum = 9007199254740991_i64)]
        gross_cost_krw: i64,
        #[schema(minimum = 1, maximum = 9007199254740991_i64)]
        payout_krw: i64,
        filing_deadline_game_day: u32,
        #[schema(min_items = 1, max_items = 8)]
        contract_allocations: Vec<InsuranceClaimAllocationSnapshot>,
    },
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsuranceContractsResponse {
    pub insurance_capability: InsuranceCapabilitySnapshot,
    #[schema(max_items = 16)]
    pub products: Vec<InsuranceProductSnapshot>,
    #[schema(max_items = 20)]
    pub contracts: Vec<InsuranceContractSnapshot>,
    #[schema(max_items = 8)]
    pub pending_claims: Vec<PendingInsuranceClaimSnapshot>,
    #[schema(max_items = 20)]
    pub history: Vec<InsuranceClaimHistoryItemSnapshot>,
    #[schema(required = true, nullable, max_length = 512)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsuranceEnrollmentResultSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub contract_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub product_version_id: ResourceId,
    pub status: InsuranceContractStatusSnapshot,
    pub coverage_start_game_day: u32,
    pub waiting_ends_game_day: u32,
    pub coverage_end_exclusive: u32,
    pub next_premium_due_game_day: u32,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub premium_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsuranceEnrollmentResponse {
    pub result: InsuranceEnrollmentResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsuranceCancellationResultSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub contract_id: ResourceId,
    pub status: InsuranceContractStatusSnapshot,
    pub coverage_end_exclusive: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsuranceCancellationResponse {
    pub result: InsuranceCancellationResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsuranceClaimResultSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub claim_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub event_id: ResourceId,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub payout_krw: i64,
    pub paid_game_day: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsuranceClaimResponse {
    pub result: InsuranceClaimResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum InsolvencyAvailabilitySnapshot {
    Unavailable,
    CashOnlyLiquidation,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum InsolvencyEligibilityStatusSnapshot {
    Eligible,
    Ineligible,
    CompositionUnsupported,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum InsolvencyEligibilityReasonSnapshot {
    PolicyUnavailable,
    ComponentUnavailable,
    InvalidWalletCash,
    NoSupportedDefaultedDebt,
    DebtNotGreaterThanCash,
    UnsupportedLoanComposition,
    UnsupportedAssetComposition,
    UnsupportedNonLoanObligation,
    ExistingNonTerminalCase,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum InsolvencyProcedureKindSnapshot {
    CashOnlyLiquidation,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum InsolvencyCaseStatusSnapshot {
    Prepared,
    Filed,
    Liquidation,
    Discharged,
    Rebuilding,
    Withdrawn,
    Recovered,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsolvencyCaseSummarySnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    pub procedure_kind: InsolvencyProcedureKindSnapshot,
    pub status: InsolvencyCaseStatusSnapshot,
    pub prepared_game_day: u32,
    #[schema(required = true, nullable)]
    pub submitted_game_day: Option<u32>,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub wallet_cash_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub protected_cash_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub distributed_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub discharged_krw: i64,
    #[schema(required = true, nullable)]
    pub credit_restriction_end_exclusive: Option<u32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsolvencySnapshot {
    pub availability: InsolvencyAvailabilitySnapshot,
    pub eligibility: InsolvencyEligibilityStatusSnapshot,
    #[schema(max_items = 16)]
    pub reasons: Vec<InsolvencyEligibilityReasonSnapshot>,
    #[schema(required = true, nullable)]
    pub current_case: Option<InsolvencyCaseSummarySnapshot>,
}

pub type InsolvencyOverviewResponse = InsolvencySnapshot;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsolvencyCaseCommandResponse {
    pub result: InsolvencyCaseSummarySnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsolvencyTransitionSnapshot {
    pub sequence: u8,
    #[schema(required = true, nullable)]
    pub from_status: Option<InsolvencyCaseStatusSnapshot>,
    pub to_status: InsolvencyCaseStatusSnapshot,
    pub game_day: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsolvencyCaseDetailResponse {
    pub summary: InsolvencyCaseSummarySnapshot,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub policy_set_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub life_catalog_set_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub insolvency_component_version_id: ResourceId,
    #[schema(min_length = 64, max_length = 64, pattern = "^[0-9a-f]{64}$")]
    pub composition_sha256: String,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub automatic_protected_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub additional_protected_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub liquidatable_krw: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub total_claim_krw: i64,
    #[schema(minimum = 1, maximum = 8)]
    pub claim_count: u8,
    #[schema(min_items = 1, max_items = 16)]
    pub transitions: Vec<InsolvencyTransitionSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsolvencyClaimSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub loan_contract_id: ResourceId,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub principal_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub interest_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub fee_krw: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub allowed_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub distributed_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub discharged_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsolvencyClaimPageResponse {
    #[schema(max_items = 20)]
    pub claims: Vec<InsolvencyClaimSnapshot>,
    #[schema(required = true, nullable, max_length = 512)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsolvencyWalletAssetSnapshot {
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub original_amount_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub protected_amount_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub liquidatable_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub distributed_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsolvencyLiquidationSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub claim_id: ResourceId,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub amount_krw: i64,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub loan_payment_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub ledger_transaction_id: ResourceId,
    pub applied_game_day: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsolvencyLiquidationPageResponse {
    #[schema(required = true, nullable)]
    pub wallet_asset: Option<InsolvencyWalletAssetSnapshot>,
    #[schema(max_items = 20)]
    pub distributions: Vec<InsolvencyLiquidationSnapshot>,
    #[schema(required = true, nullable, max_length = 512)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum CorporationAvailabilitySnapshot {
    Unavailable,
    Active,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum CorporationStatusSnapshot {
    Draft,
    Active,
    Dormant,
    Insolvent,
    Dissolved,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CorporationOperatingScaleSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    pub scale_key: String,
    pub scale_order: u8,
    #[schema(minimum = 1, maximum = 3000000)]
    pub revenue_factor_ppm: u32,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub fixed_cost_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CorporationTemplateSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    pub template_key: String,
    pub display_name: String,
    pub template_order: u8,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub base_monthly_revenue_krw: i64,
    #[schema(maximum = 900000)]
    pub revenue_variation_ppm: u32,
    #[schema(maximum = 1000000)]
    pub variable_cost_ppm: u32,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub fixed_monthly_cost_krw: i64,
    #[schema(max_items = 3)]
    pub operating_scales: Vec<CorporationOperatingScaleSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CorporationOperatingSettingSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub corporation_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub operating_scale_id: ResourceId,
    pub scale_key: String,
    pub scale_order: u8,
    #[schema(minimum = 1, maximum = 3000000)]
    pub revenue_factor_ppm: u32,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub fixed_cost_krw: i64,
    #[schema(minimum = 1, maximum = 9999)]
    pub effective_year: u16,
    #[schema(minimum = 1, maximum = 12)]
    pub effective_month: u8,
    #[schema(minimum = 0, maximum = 100000000)]
    pub officer_gross_salary_krw: i64,
    pub created_game_day: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CorporationNextMonthSettingSnapshot {
    #[schema(required = true, nullable, value_type = Option<String>, pattern = "^[1-9][0-9]*$")]
    pub setting_id: Option<ResourceId>,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub operating_scale_id: ResourceId,
    pub scale_key: String,
    pub scale_order: u8,
    #[schema(minimum = 1, maximum = 3000000)]
    pub revenue_factor_ppm: u32,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub fixed_cost_krw: i64,
    #[schema(minimum = 1, maximum = 9999)]
    pub effective_year: u16,
    #[schema(minimum = 1, maximum = 12)]
    pub effective_month: u8,
    #[schema(minimum = 0, maximum = 100000000)]
    pub officer_gross_salary_krw: i64,
    #[schema(required = true, nullable)]
    pub created_game_day: Option<u32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CorporationTemplatesResponse {
    pub availability: CorporationAvailabilitySnapshot,
    #[schema(required = true, nullable, value_type = Option<String>, pattern = "^[1-9][0-9]*$")]
    pub component_version_id: Option<ResourceId>,
    #[schema(required = true, nullable)]
    pub registered_office_class: Option<String>,
    #[schema(required = true, nullable, minimum = 1, maximum = 9007199254740991_i64)]
    pub minimum_capital_krw: Option<i64>,
    #[schema(required = true, nullable, minimum = 1, maximum = 9007199254740991_i64)]
    pub maximum_capital_krw: Option<i64>,
    #[schema(required = true, nullable, minimum = 0, maximum = 9007199254740991_i64)]
    pub game_administrative_fee_krw: Option<i64>,
    #[schema(max_items = 3)]
    pub templates: Vec<CorporationTemplateSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CorporationSummarySnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub component_version_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub industry_template_id: ResourceId,
    pub template_key: String,
    pub template_display_name: String,
    pub name: String,
    pub representative_name: String,
    pub status: CorporationStatusSnapshot,
    pub established_game_day: u32,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub capital_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub registration_license_tax_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub local_education_tax_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub game_administrative_fee_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub total_establishment_fee_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub cash_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub contributed_capital_krw: i64,
    pub retained_earnings_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub operating_payable_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub corporate_tax_payable_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub distributable_profit_krw: i64,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub personal_ledger_transaction_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub corporation_ledger_transaction_id: ResourceId,
    pub next_month_setting: CorporationNextMonthSettingSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CorporationSnapshot {
    pub availability: CorporationAvailabilitySnapshot,
    #[schema(required = true, nullable)]
    pub current: Option<CorporationSummarySnapshot>,
}

pub type CorporationDetailResponse = CorporationSummarySnapshot;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CorporationCreateResponse {
    pub result: CorporationSummarySnapshot,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub wallet_debit_krw: i64,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CorporationSettingsResponse {
    pub result: CorporationOperatingSettingSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CorporationDividendSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub corporation_id: ResourceId,
    pub tax_year: u16,
    pub gross_dividend_krw: i64,
    pub withheld_income_tax_krw: i64,
    pub withheld_local_income_tax_krw: i64,
    pub net_dividend_krw: i64,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub corporation_ledger_transaction_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub personal_ledger_transaction_id: ResourceId,
    pub paid_game_day: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CorporationDividendResponse {
    pub result: CorporationDividendSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum CorporationPayrollStatusSnapshot {
    NotConfigured,
    Paid,
    Unpaid,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CorporationOperatingMonthSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    pub operating_year: u16,
    pub operating_month: u8,
    pub scale_key: String,
    pub officer_gross_salary_krw: i64,
    pub revenue_krw: i64,
    pub operating_expense_krw: i64,
    pub total_payroll_cost_krw: i64,
    pub pre_tax_profit_krw: i64,
    pub payroll_status: CorporationPayrollStatusSnapshot,
    pub cash_after_krw: i64,
    pub operating_payable_after_krw: i64,
    pub retained_earnings_after_krw: i64,
    pub applied_game_day: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CorporationOperatingMonthPageResponse {
    #[schema(max_items = 20)]
    pub months: Vec<CorporationOperatingMonthSnapshot>,
    #[schema(required = true, nullable, max_length = 512)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LifeSnapshot {
    pub rate_status: LifeRateStatusSnapshot,
    #[schema(required = true, nullable)]
    pub household: Option<LifeHouseholdSnapshot>,
    #[schema(required = true, nullable)]
    pub residence: Option<LifeResidenceSnapshot>,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub tenant_lease_deposit_krw: i64,
    #[schema(required = true, nullable)]
    pub active_lease: Option<ActiveHousingLeaseSnapshot>,
    #[schema(max_items = 20)]
    pub active_lease_arrears: Vec<LeaseArrearSnapshot>,
    pub has_more_active_lease_arrears: bool,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub total_lease_arrear_krw: i64,
    #[schema(max_items = 4)]
    pub active_property_holdings: Vec<PropertyHoldingSnapshot>,
    pub has_more_active_property_holdings: bool,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub total_property_book_value_krw: i64,
    #[schema(required = true, nullable)]
    pub current_month: Option<LivingCostMonthSnapshot>,
    #[schema(max_items = 20)]
    pub active_arrears: Vec<EssentialArrearSnapshot>,
    pub has_more_active_arrears: bool,
    #[schema(minimum = 0)]
    pub total_essential_arrear_krw: i64,
    #[schema(required = true, nullable)]
    pub credit_band: Option<CreditBandSnapshot>,
    #[schema(max_items = 8)]
    pub credit_reasons: Vec<CreditReasonSnapshot>,
    #[schema(max_items = 8)]
    pub active_loans: Vec<LoanSummarySnapshot>,
    #[schema(required = true, nullable)]
    pub next_loan_installment: Option<NextLoanInstallmentSnapshot>,
    #[schema(minimum = 0)]
    pub total_loan_balance_krw: i64,
    #[schema(max_items = 8)]
    pub active_welfare_applications: Vec<ActiveWelfareApplicationSnapshot>,
    pub insurance_capability: InsuranceCapabilitySnapshot,
    #[schema(max_items = 8)]
    pub active_insurance_contracts: Vec<InsuranceContractSnapshot>,
    #[schema(max_items = 8)]
    pub pending_insurance_claims: Vec<PendingInsuranceClaimSnapshot>,
    #[schema(max_items = 8)]
    pub pending_events: Vec<PendingLifeEventSnapshot>,
    pub insolvency: InsolvencySnapshot,
    pub corporation: CorporationSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LifeBudgetResponse {
    pub rate_status: LifeRateStatusSnapshot,
    pub household: LifeHouseholdSnapshot,
    pub residence: LifeResidenceSnapshot,
    #[schema(max_items = 16)]
    pub allowed_bands: Vec<LifeBudgetBandSnapshot>,
    #[schema(max_items = 9)]
    pub selections: Vec<LifeBudgetSelectionSnapshot>,
    #[schema(required = true, nullable)]
    pub current_month: Option<LivingCostMonthSnapshot>,
    #[schema(max_items = 20)]
    pub active_arrears: Vec<EssentialArrearSnapshot>,
    pub has_more_active_arrears: bool,
    #[schema(minimum = 0)]
    pub total_essential_arrear_krw: i64,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HousingRegionKeySnapshot {
    CapitalArea,
    Metropolitan,
    SmallCity,
    Rural,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HousingRateStatusSnapshot {
    Active,
    RateUnavailable,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HousingPropertyTypeSnapshot {
    Apartment,
    MultiFamily,
    Detached,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HousingLeaseCapabilitySnapshot {
    CashJeonse,
    CashJeonseAndMonthlyRent,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HousingLeaseRenewalRuleSnapshot {
    FixedTermAutoRenew,
    OpenEnded,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HousingLeaseTerminationReviewRuleSnapshot {
    OldestActiveArrearAge,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HousingLeaseRoleSnapshot {
    Tenant,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HousingLeaseOfferKindSnapshot {
    Jeonse,
    MonthlyRent,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum JeonseHousingLeaseOfferKindSnapshot {
    Jeonse,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HousingRentChargeRuleSnapshot {
    NextMonthStartFull,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HousingLeaseArrearRepaymentRuleSnapshot {
    ManualOnly,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyRentTermsSnapshot {
    pub rent_charge_rule: HousingRentChargeRuleSnapshot,
    pub arrear_repayment_rule: HousingLeaseArrearRepaymentRuleSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyRentTerminationReviewTermsSnapshot {
    pub rule: HousingLeaseTerminationReviewRuleSnapshot,
    #[schema(minimum = 1)]
    pub after_game_days: u16,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LeaseLifecycleTermsSnapshot {
    #[schema(minimum = 1)]
    pub term_months: u16,
    #[schema(minimum = 1)]
    pub renewal_notice_lead_days: u16,
    #[schema(required = true, nullable)]
    pub monthly_rent_termination_review: Option<MonthlyRentTerminationReviewTermsSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActiveLeaseTermSnapshot {
    #[schema(minimum = 1)]
    pub term_no: u32,
    pub effective_from_game_day: u32,
    pub effective_to_game_day: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LeaseRenewalNoticeSnapshot {
    #[schema(minimum = 1)]
    pub term_no: u32,
    pub published_game_day: u32,
    pub renews_on_game_day: u32,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LeaseTerminationReviewStatusSnapshot {
    UnderReview,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LeaseTerminationReviewSnapshot {
    pub status: LeaseTerminationReviewStatusSnapshot,
    pub opened_game_day: u32,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub trigger_arrear_id: ResourceId,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub active_lease_arrear_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LeaseArrearSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub lease_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub rent_charge_id: ResourceId,
    pub due_year_month: YearMonthSnapshot,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub original_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub paid_krw: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub remaining_krw: i64,
    pub created_game_day: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HousingMovingCostSnapshot {
    pub region_key: HousingRegionKeySnapshot,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub moving_cost_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActiveHousingLeaseSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub listing_id: ResourceId,
    pub role: HousingLeaseRoleSnapshot,
    pub offer_kind: HousingLeaseOfferKindSnapshot,
    pub region_key: HousingRegionKeySnapshot,
    pub property_type: HousingPropertyTypeSnapshot,
    #[schema(minimum = 1)]
    pub exclusive_area_square_meters: u16,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub deposit_krw: i64,
    #[schema(required = true, nullable, minimum = 1, maximum = 9007199254740991_i64)]
    pub monthly_rent_krw: Option<i64>,
    #[schema(required = true, nullable)]
    pub next_rent_due_game_day: Option<u32>,
    pub effective_from_game_day: u32,
    #[schema(required = true, nullable)]
    pub effective_to_game_day: Option<u32>,
    pub renewal_rule: HousingLeaseRenewalRuleSnapshot,
    #[schema(required = true, nullable)]
    pub current_term: Option<ActiveLeaseTermSnapshot>,
    #[schema(required = true, nullable)]
    pub renewal_notice: Option<LeaseRenewalNoticeSnapshot>,
    #[schema(required = true, nullable)]
    pub termination_review: Option<LeaseTerminationReviewSnapshot>,
    #[schema(required = true, nullable, value_type = Option<String>)]
    pub deposit_loan_id: Option<ResourceId>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HousingLeaseCurrentResponse {
    pub lease_capability: HousingLeaseCapabilitySnapshot,
    #[schema(required = true, nullable)]
    pub renewal_rule: Option<HousingLeaseRenewalRuleSnapshot>,
    #[schema(required = true, nullable)]
    pub lease_lifecycle_terms: Option<LeaseLifecycleTermsSnapshot>,
    #[schema(max_items = 4)]
    pub moving_costs: Vec<HousingMovingCostSnapshot>,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub tenant_lease_deposit_krw: i64,
    #[schema(required = true, nullable)]
    pub active_lease: Option<ActiveHousingLeaseSnapshot>,
    #[schema(required = true, nullable)]
    pub monthly_rent_terms: Option<MonthlyRentTermsSnapshot>,
    #[schema(max_items = 20)]
    pub active_arrears: Vec<LeaseArrearSnapshot>,
    pub has_more_active_arrears: bool,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub total_lease_arrear_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HousingLeaseMoveResultSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub lease_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub residence_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub listing_id: ResourceId,
    pub offer_kind: HousingLeaseOfferKindSnapshot,
    pub region_key: HousingRegionKeySnapshot,
    pub property_type: HousingPropertyTypeSnapshot,
    #[schema(minimum = 1)]
    pub exclusive_area_square_meters: u16,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub deposit_krw: i64,
    #[schema(required = true, nullable, minimum = 1, maximum = 9007199254740991_i64)]
    pub monthly_rent_krw: Option<i64>,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub returned_deposit_krw: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub moving_cost_krw: i64,
    #[schema(minimum = -9007199254740991_i64, maximum = 9007199254740991_i64)]
    pub wallet_delta_krw: i64,
    pub effective_from_game_day: u32,
    #[schema(required = true, nullable, value_type = Option<String>)]
    pub ended_lease_id: Option<ResourceId>,
    pub renewal_rule: HousingLeaseRenewalRuleSnapshot,
    #[schema(required = true, nullable)]
    pub deposit_loan_execution: Option<DepositLoanExecutionSnapshot>,
    #[schema(required = true, nullable)]
    pub repaid_deposit_loan: Option<RepaidDepositLoanSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DepositLoanExecutionSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub loan_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub quote_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub product_version_id: ResourceId,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub principal_krw: i64,
    #[schema(minimum = 0, maximum = 20000)]
    pub annual_rate_bp: i64,
    pub maturity_game_day: u32,
    pub first_installment: LoanQuoteFirstInstallmentSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepaidDepositLoanSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub loan_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub payment_id: ResourceId,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub principal_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HousingLeaseMoveResponse {
    pub result: HousingLeaseMoveResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HousingPurchaseCapabilitySnapshot {
    OwnerOccupiedSingleHome,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum PropertyHoldingStatusSnapshot {
    Active,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum PropertyHoldingPurposeSnapshot {
    OwnerOccupied,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PropertyHoldingSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub listing_id: ResourceId,
    pub status: PropertyHoldingStatusSnapshot,
    pub purpose: PropertyHoldingPurposeSnapshot,
    pub region_key: HousingRegionKeySnapshot,
    pub property_type: HousingPropertyTypeSnapshot,
    #[schema(minimum = 1)]
    pub exclusive_area_square_meters: u16,
    pub acquired_game_day: u32,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub acquisition_price_krw: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub acquisition_incidental_cost_krw: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub book_value_krw: i64,
    #[schema(required = true, nullable, value_type = Option<String>)]
    pub mortgage_loan_id: Option<ResourceId>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HousingPropertyHoldingsResponse {
    pub purchase_capability: HousingPurchaseCapabilitySnapshot,
    #[schema(maximum = 1)]
    pub maximum_active_holdings: u8,
    #[schema(max_items = 4)]
    pub holdings: Vec<PropertyHoldingSnapshot>,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub total_property_book_value_krw: i64,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MortgageQuoteDecisionSnapshot {
    CreditRestricted,
    PurchaseRestricted,
    CollateralLimit,
    IncomeUnavailable,
    DebtServiceLimit,
    InsufficientOwnFunds,
    Eligible,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MortgageQuoteReasonSnapshot {
    InsolvencyRebuilding,
    ActiveDefault,
    ActiveDelinquency,
    ActiveRestructuring,
    CreditBandRestricted,
    ActiveLoanLimit,
    ActiveHolding,
    ResidenceChangedToday,
    LeaseExitRestricted,
    CollateralLimit,
    IncomeUnavailable,
    DebtServiceLimit,
    InsufficientOwnFunds,
    Eligible,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MortgageLtvRegionClassSnapshot {
    RegulatedCapitalProxy,
    NonRegulatedProxy,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MortgageStressTreatmentSnapshot {
    FullTermFixed,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MortgageLtvSnapshot {
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub numerator_krw: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub denominator_krw: i64,
    #[schema(minimum = 0)]
    pub ratio_ppm: i64,
    #[schema(minimum = 1, maximum = 1000000)]
    pub limit_ppm: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MortgageQuoteResultSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub quote_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub listing_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub product_version_id: ResourceId,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub requested_principal_krw: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub purchase_price_krw: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub recognized_collateral_value_krw: i64,
    pub ltv_region_class: MortgageLtvRegionClassSnapshot,
    #[schema(minimum = 1, maximum = 1000000)]
    pub ltv_limit_ppm: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub maximum_mortgage_krw: i64,
    pub ltv: MortgageLtvSnapshot,
    pub created_game_day: u32,
    pub expires_game_day: u32,
    pub decision_code: MortgageQuoteDecisionSnapshot,
    #[schema(min_items = 1, max_items = 8)]
    pub decision_reasons: Vec<MortgageQuoteReasonSnapshot>,
    #[schema(required = true, nullable, minimum = 1, maximum = 9007199254740991_i64)]
    pub verified_annual_income_krw: Option<i64>,
    #[schema(required = true, nullable)]
    pub verified_income_source: Option<VerifiedIncomeSourceSnapshot>,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub existing_loan_balance_krw: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub post_execution_balance_krw: i64,
    pub dsr_applied: bool,
    #[schema(required = true, nullable)]
    pub dsr: Option<LoanQuoteDsrSnapshot>,
    #[schema(minimum = 0, maximum = 0)]
    pub stress_rate_bp: i64,
    pub stress_treatment: MortgageStressTreatmentSnapshot,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub acquisition_incidental_cost_krw: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub moving_cost_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub returned_deposit_krw: i64,
    #[schema(required = true, nullable, value_type = Option<String>)]
    pub replaced_loan_id: Option<ResourceId>,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub replaced_loan_principal_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub available_buyer_cash_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub required_buyer_cash_krw: i64,
    pub quoted_terms: LoanQuotedTermsSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MortgageQuoteResponse {
    pub result: MortgageQuoteResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MortgageExecutionSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub loan_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub quote_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub product_version_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub property_holding_id: ResourceId,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub principal_krw: i64,
    pub activated_game_day: u32,
    pub maturity_game_day: u32,
    #[schema(minimum = 0, maximum = 20000)]
    pub annual_rate_bp: i64,
    pub repayment_method: LoanRepaymentMethodSnapshot,
    #[schema(minimum = 1)]
    pub term_months: u16,
    pub first_installment: LoanQuoteFirstInstallmentSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PropertyPurchaseResultSnapshot {
    pub holding: PropertyHoldingSnapshot,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub residence_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub listing_id: ResourceId,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub purchase_price_krw: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub acquisition_incidental_cost_krw: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub moving_cost_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub returned_deposit_krw: i64,
    #[schema(minimum = -9007199254740991_i64, maximum = 9007199254740991_i64)]
    pub wallet_delta_krw: i64,
    pub effective_from_game_day: u32,
    #[schema(required = true, nullable, value_type = Option<String>)]
    pub ended_lease_id: Option<ResourceId>,
    #[schema(required = true, nullable)]
    pub repaid_deposit_loan: Option<RepaidDepositLoanSnapshot>,
    #[schema(required = true, nullable)]
    pub mortgage_execution: Option<MortgageExecutionSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PropertyPurchaseResponse {
    pub result: PropertyPurchaseResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum PropertySaleOrderStatusSnapshot {
    Active,
    Filled,
    Cancelled,
    Rejected,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum PropertySaleOrderRevisionKindSnapshot {
    Listing,
    Cancellation,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum PropertySaleOrderRejectionReasonSnapshot {
    MortgageNotPayable,
    InsufficientProceeds,
    PolicyUnsupported,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PropertySaleOrderListingResultSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub order_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub holding_id: ResourceId,
    #[schema(minimum = 1)]
    pub revision_no: u32,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub asking_price_krw: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub reference_value_krw: i64,
    #[schema(minimum = 800000, maximum = 1200000)]
    pub asking_to_reference_ppm: i64,
    pub candidate_game_day: u32,
    pub status: PropertySaleOrderStatusSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PropertySaleOrderCancellationResultSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub order_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub holding_id: ResourceId,
    #[schema(minimum = 1)]
    pub revision_no: u32,
    pub cancelled_game_day: u32,
    pub status: PropertySaleOrderStatusSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PropertySaleOrderListingResponse {
    pub result: PropertySaleOrderListingResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PropertySaleOrderCancellationResponse {
    pub result: PropertySaleOrderCancellationResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PropertySaleExecutionSnapshot {
    pub filled_game_day: u32,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub gross_sale_price_krw: i64,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub transaction_cost_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub mortgage_principal_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub mortgage_fee_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub capital_gains_tax_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub wallet_proceeds_krw: i64,
    #[schema(minimum = -9007199254740991_i64, maximum = 9007199254740991_i64)]
    pub realized_gain_loss_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PropertySaleOrderSummarySnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub order_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub holding_id: ResourceId,
    #[schema(minimum = 1)]
    pub revision_no: u32,
    pub revision_kind: PropertySaleOrderRevisionKindSnapshot,
    #[schema(required = true, nullable, minimum = 1, maximum = 9007199254740991_i64)]
    pub asking_price_krw: Option<i64>,
    #[schema(required = true, nullable, minimum = 1, maximum = 9007199254740991_i64)]
    pub reference_value_krw: Option<i64>,
    #[schema(required = true, nullable, minimum = 800000, maximum = 1200000)]
    pub asking_to_reference_ppm: Option<i64>,
    #[schema(required = true, nullable)]
    pub candidate_game_day: Option<u32>,
    pub status: PropertySaleOrderStatusSnapshot,
    #[schema(required = true, nullable)]
    pub cancelled_game_day: Option<u32>,
    #[schema(required = true, nullable)]
    pub rejection_reason: Option<PropertySaleOrderRejectionReasonSnapshot>,
    #[schema(required = true, nullable)]
    pub execution: Option<PropertySaleExecutionSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PropertySaleOrdersResponse {
    #[schema(max_items = 20)]
    pub items: Vec<PropertySaleOrderSummarySnapshot>,
    #[schema(required = true, nullable, value_type = Option<String>, pattern = "^[1-9][0-9]*$")]
    pub next_before: Option<ResourceId>,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum PropertyTaxEventKindSnapshot {
    Acquisition,
    AnnualHolding,
    CapitalGains,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum PropertyTaxEventStatusSnapshot {
    Scheduled,
    PartiallyPaid,
    Paid,
    NoPaymentRequired,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum PropertyTaxPaymentStatusSnapshot {
    Pending,
    Applied,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PropertyTaxComponentSnapshot {
    #[schema(min_length = 1, max_length = 64)]
    pub component_key: String,
    pub component_order: u8,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub tax_base_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub deduction_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub taxable_amount_krw: i64,
    #[schema(minimum = 0, maximum = 1000000)]
    pub rate_ppm: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub progressive_deduction_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub amount_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PropertyTaxPaymentSnapshot {
    #[schema(minimum = 1)]
    pub payment_no: u8,
    pub due_game_day: u32,
    #[schema(required = true, nullable)]
    pub paid_game_day: Option<u32>,
    pub status: PropertyTaxPaymentStatusSnapshot,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub amount_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub wallet_paid_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub tax_obligation_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PropertyTaxEventSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub holding_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub policy_set_id: ResourceId,
    #[schema(min_length = 1, max_length = 120)]
    pub policy_key: String,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub rule_id: ResourceId,
    #[schema(min_length = 1, max_length = 120)]
    pub rule_key: String,
    #[schema(format = Date)]
    pub legal_basis_date: String,
    pub kind: PropertyTaxEventKindSnapshot,
    pub status: PropertyTaxEventStatusSnapshot,
    pub assessed_game_day: u32,
    pub taxable_game_day: u32,
    #[schema(required = true, nullable)]
    pub paid_game_day: Option<u32>,
    #[schema(minimum = 1)]
    pub household_home_count: u8,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub gross_amount_krw: i64,
    #[schema(required = true, nullable)]
    pub valuation_game_day: Option<u32>,
    #[schema(required = true, nullable, minimum = 1)]
    pub valuation_price_index_ppm: Option<i64>,
    #[schema(required = true, nullable, minimum = 0, maximum = 9007199254740991_i64)]
    pub official_value_krw: Option<i64>,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub tax_base_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub deduction_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub taxable_amount_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub total_tax_krw: i64,
    #[schema(max_items = 16)]
    pub components: Vec<PropertyTaxComponentSnapshot>,
    #[schema(max_items = 2)]
    pub payments: Vec<PropertyTaxPaymentSnapshot>,
    #[schema(max_items = 16)]
    pub exclusion_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PropertyTaxEventsResponse {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub holding_id: ResourceId,
    #[schema(max_items = 20)]
    pub items: Vec<PropertyTaxEventSnapshot>,
    #[schema(required = true, nullable, value_type = Option<String>, pattern = "^[1-9][0-9]*$")]
    pub next_before: Option<ResourceId>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum HousingOfferSnapshot {
    Sale {
        #[schema(minimum = 1)]
        price_krw: i64,
    },
    Jeonse {
        #[schema(minimum = 1)]
        deposit_krw: i64,
    },
    MonthlyRent {
        #[schema(minimum = 1)]
        deposit_krw: i64,
        #[schema(minimum = 1)]
        monthly_rent_krw: i64,
    },
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HousingRegionSnapshot {
    pub region_key: HousingRegionKeySnapshot,
    #[schema(min_length = 1, max_length = 120)]
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HousingListingSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub id: ResourceId,
    pub region_key: HousingRegionKeySnapshot,
    pub property_type: HousingPropertyTypeSnapshot,
    #[schema(minimum = 1)]
    pub exclusive_area_square_meters: u16,
    pub available_from_game_day: u32,
    pub available_to_game_day: u32,
    #[schema(min_items = 1, max_items = 3)]
    pub offers: Vec<HousingOfferSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HousingListingsResponse {
    pub rate_status: HousingRateStatusSnapshot,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub model_version_id: ResourceId,
    pub game_day: u32,
    pub year_month: YearMonthSnapshot,
    pub residence_region_key: HousingRegionKeySnapshot,
    pub selected_region_key: HousingRegionKeySnapshot,
    #[schema(min_items = 1, max_items = 4)]
    pub regions: Vec<HousingRegionSnapshot>,
    #[schema(required = true, nullable, minimum = 1)]
    pub price_index_ppm: Option<i64>,
    #[schema(required = true, nullable, minimum = 1)]
    pub rent_index_ppm: Option<i64>,
    #[schema(max_items = 24)]
    pub listings: Vec<HousingListingSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LifeBudgetUpdateResultSnapshot {
    pub applied_game_day: u32,
    #[schema(min_items = 9, max_items = 9)]
    pub selections: Vec<LifeBudgetSelectionSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LifeBudgetUpdateResponse {
    pub result: LifeBudgetUpdateResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EssentialArrearPaymentResultSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub arrear_id: ResourceId,
    #[schema(minimum = 1)]
    pub paid_krw: i64,
    #[schema(minimum = 0)]
    pub remaining_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EssentialArrearPaymentResponse {
    pub result: EssentialArrearPaymentResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LeaseArrearPaymentResultSnapshot {
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub arrear_id: ResourceId,
    #[schema(value_type = String, pattern = "^[1-9][0-9]*$")]
    pub payment_id: ResourceId,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    pub paid_krw: i64,
    #[schema(minimum = 0, maximum = 9007199254740991_i64)]
    pub remaining_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LeaseArrearPaymentResponse {
    pub result: LeaseArrearPaymentResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone)]
pub enum LifeCommandResult<T> {
    Applied(Box<T>),
    Rejected(LifeFailureCode),
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerScoresSnapshot {
    pub education: i64,
    pub certification: i64,
    pub language: i64,
    pub training: i64,
    pub experience: i64,
    pub project: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerActivitySnapshot {
    pub id: ResourceId,
    pub catalog_entry_id: ResourceId,
    pub activity_key: String,
    pub display_name: String,
    #[schema(value_type = String)]
    pub status: ActivityStatus,
    #[schema(required = true)]
    pub priority: Option<u8>,
    #[schema(required = true)]
    pub started_game_day: Option<u32>,
    pub accumulated_effort_units: u64,
    pub required_effort_units: u64,
    pub elapsed_calendar_days: u32,
    pub minimum_calendar_days: u32,
    pub daily_effort_cap_units: u64,
    #[schema(required = true)]
    pub completed_game_day: Option<u32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerArtifactSnapshot {
    pub id: ResourceId,
    #[schema(value_type = String)]
    pub kind: ArtifactKind,
    pub version_no: u32,
    pub completeness_bp: i64,
    pub created_game_day: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerSnapshot {
    pub focused_job_family_key: String,
    pub possessed_scores: CareerScoresSnapshot,
    #[schema(max_items = 3)]
    pub active_activities: Vec<CareerActivitySnapshot>,
    #[schema(max_items = 3)]
    pub latest_artifacts: Vec<CareerArtifactSnapshot>,
    #[schema(max_items = 10)]
    pub open_applications: Vec<CareerOpenApplicationSnapshot>,
    #[schema(max_items = 5)]
    pub open_invitations: Vec<CareerInvitationSnapshot>,
    #[schema(required = true, nullable)]
    pub employment: Option<CareerEmploymentContractSnapshot>,
    #[schema(required = true, nullable)]
    pub latest_payroll: Option<CareerPayrollSnapshot>,
    pub current_employment_tax_year: CareerEmploymentTaxYearSnapshot,
    #[schema(required = true, nullable)]
    pub latest_employment_tax_assessment: Option<CareerEmploymentTaxYearSnapshot>,
    pub military_status: MilitaryStatusSnapshot,
    #[schema(required = true, nullable)]
    pub active_military_service: Option<ActiveMilitaryServiceSummarySnapshot>,
    #[schema(max_items = 2)]
    pub active_military_savings: Vec<ActiveMilitarySavingsSummarySnapshot>,
    #[schema(max_items = 20)]
    pub pending_career_schedule: Vec<CareerPendingScheduleItemSnapshot>,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum CareerScheduledActionKindSnapshot {
    EmploymentStart,
    MilitaryServiceStart,
    MilitaryServiceCompletion,
    DocumentReview,
    ConfirmationExpiry,
    InterviewDecision,
    OfferExpiry,
    InvitationGeneration,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum CareerScheduledSettlementKindSnapshot {
    EmploymentPayroll,
    EmploymentReconciliation,
    MilitaryPay,
    MilitarySavingsInstallment,
    MilitarySavingsMaturity,
    MilitarySavingsGovernmentMatch,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(
    tag = "sourceKind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CareerPendingScheduleItemSnapshot {
    CareerAction {
        id: ResourceId,
        due_game_day: u32,
        kind: CareerScheduledActionKindSnapshot,
    },
    Settlement {
        id: ResourceId,
        due_game_day: u32,
        kind: CareerScheduledSettlementKindSnapshot,
    },
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerJobSnapshot {
    pub posting_key: String,
    pub posted_game_day: u32,
    pub closes_exclusive_game_day: u32,
    #[schema(value_type = String)]
    pub platform: crate::store::CareerPlatform,
    #[schema(value_type = String)]
    pub industry: Industry,
    pub job_family_key: String,
    pub employer_name: String,
    pub region: crate::character::Region,
    #[schema(value_type = String)]
    pub employment_type: crate::career::EmploymentType,
    pub required_scores: CareerScoresSnapshot,
    pub possessed_scores: CareerScoresSnapshot,
    pub minimum_annual_salary_krw: i64,
    pub maximum_annual_salary_krw: i64,
    pub salary_step_krw: i64,
    #[schema(value_type = String)]
    pub competition_band: crate::store::CareerCompetitionBand,
    pub military_requirement: String,
    #[schema(required = true, nullable)]
    pub minimum_education: Option<crate::character::Education>,
    #[schema(required = true, nullable)]
    pub required_certification_name: Option<String>,
    pub minimum_experience_days: u32,
    #[schema(value_type = Vec<String>, max_items = 3)]
    pub required_artifacts: Vec<ArtifactKind>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerJobsResponse {
    #[schema(max_items = 200)]
    pub items: Vec<CareerJobSnapshot>,
    #[schema(required = true, nullable)]
    pub next_before: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerOfferSnapshot {
    pub id: ResourceId,
    #[schema(value_type = String)]
    pub status: crate::store::CareerOfferStatus,
    pub annual_salary_krw: i64,
    pub payday_day_of_month: u8,
    pub start_game_day: u32,
    pub expires_exclusive_game_day: u32,
    pub wanted_reward_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerApplicationSnapshot {
    pub id: ResourceId,
    pub posting_key: String,
    #[schema(value_type = String)]
    pub platform: crate::store::CareerPlatform,
    #[schema(value_type = String)]
    pub industry: Industry,
    pub employer_name: String,
    pub job_family_key: String,
    #[schema(value_type = String)]
    pub source: crate::store::CareerApplicationSource,
    #[schema(value_type = String)]
    pub status: crate::store::CareerApplicationStatus,
    pub submitted_game_day: u32,
    pub visible_scores: CareerScoresSnapshot,
    pub possessed_scores: CareerScoresSnapshot,
    #[schema(required = true, nullable)]
    pub document_score_bp: Option<i64>,
    #[schema(required = true, nullable)]
    pub document_decision_game_day: Option<u32>,
    #[schema(required = true, nullable)]
    pub interview_game_day: Option<u32>,
    #[schema(required = true, nullable)]
    pub confirmation_deadline_exclusive_game_day: Option<u32>,
    #[schema(required = true, nullable)]
    pub interview_score_bp: Option<i64>,
    #[schema(required = true, nullable)]
    pub offer: Option<CareerOfferSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerOpenApplicationSnapshot {
    pub id: ResourceId,
    pub posting_key: String,
    #[schema(value_type = String)]
    pub platform: crate::store::CareerPlatform,
    #[schema(value_type = String)]
    pub industry: Industry,
    pub employer_name: String,
    pub job_family_key: String,
    #[schema(value_type = String)]
    pub status: crate::store::CareerApplicationStatus,
    #[schema(required = true, nullable)]
    pub confirmation_deadline_exclusive_game_day: Option<u32>,
    #[schema(required = true, nullable)]
    pub interview_game_day: Option<u32>,
    #[schema(required = true, nullable)]
    pub offer: Option<CareerOfferSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerInvitationSnapshot {
    pub id: ResourceId,
    pub posting_key: String,
    #[schema(value_type = String)]
    pub platform: crate::store::CareerPlatform,
    #[schema(value_type = String)]
    pub industry: Industry,
    pub job_family_key: String,
    pub employer_name: String,
    pub artifact_version_id: ResourceId,
    pub created_game_day: u32,
    pub expires_exclusive_game_day: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerEmploymentContractSnapshot {
    pub id: ResourceId,
    #[schema(value_type = String)]
    pub status: crate::store::EmploymentStatus,
    pub job_family_key: String,
    pub employer_name: String,
    pub region: String,
    pub annual_salary_krw: i64,
    pub payday_day_of_month: u8,
    pub start_game_day: u32,
    #[schema(required = true, nullable)]
    pub end_game_day: Option<u32>,
    pub credited_experience_days: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerApplicationsResponse {
    #[schema(max_items = 200)]
    pub items: Vec<CareerApplicationSnapshot>,
    #[schema(required = true, nullable)]
    pub next_before: Option<ResourceId>,
    #[schema(max_items = 5)]
    pub open_invitations: Vec<CareerInvitationSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerEmploymentResponse {
    #[schema(required = true, nullable)]
    pub contract: Option<CareerEmploymentContractSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerRewardPaymentSnapshot {
    pub payment_id: ResourceId,
    pub gross_reward_krw: i64,
    pub withheld_income_tax_krw: i64,
    pub withheld_local_income_tax_krw: i64,
    pub net_reward_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerPayrollSnapshot {
    pub id: ResourceId,
    pub contract_id: ResourceId,
    pub period_no: u64,
    pub salary_month_ordinal: u8,
    #[schema(format = Date)]
    pub period_start_date: String,
    #[schema(format = Date)]
    pub period_end_exclusive_date: String,
    pub paid_game_day: u32,
    pub gross_pay_krw: i64,
    pub employee_national_pension_krw: i64,
    pub employer_national_pension_krw: i64,
    pub employee_health_insurance_krw: i64,
    pub employer_health_insurance_krw: i64,
    pub employee_long_term_care_krw: i64,
    pub employer_long_term_care_krw: i64,
    pub employee_employment_insurance_krw: i64,
    pub employer_employment_insurance_krw: i64,
    pub employer_industrial_accident_krw: i64,
    pub withheld_income_tax_krw: i64,
    pub withheld_local_income_tax_krw: i64,
    pub net_pay_krw: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reward: Option<CareerRewardPaymentSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerPayrollResponse {
    #[schema(max_items = 200)]
    pub items: Vec<CareerPayrollSnapshot>,
    #[schema(required = true, nullable)]
    pub next_before: Option<ResourceId>,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum CareerEmploymentTaxYearStatusSnapshot {
    Open,
    Provisional,
    Definitive,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum CareerEmploymentTaxYearSourceSnapshot {
    EmploymentOnly,
    Combined,
    LegacyProfile,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerEmploymentTaxYearSnapshot {
    pub tax_year: u16,
    pub status: CareerEmploymentTaxYearStatusSnapshot,
    pub source: CareerEmploymentTaxYearSourceSnapshot,
    pub gross_employment_income_krw: i64,
    #[schema(required = true, nullable)]
    pub employee_insurance_deduction_krw: Option<i64>,
    #[schema(required = true, nullable)]
    pub earned_income_deduction_krw: Option<i64>,
    #[schema(required = true, nullable)]
    pub personal_deduction_krw: Option<i64>,
    #[schema(required = true, nullable)]
    pub taxable_income_krw: Option<i64>,
    #[schema(required = true, nullable)]
    pub calculated_income_tax_krw: Option<i64>,
    #[schema(required = true, nullable)]
    pub earned_income_tax_credit_krw: Option<i64>,
    #[schema(required = true, nullable)]
    pub pension_credit_eligible_contribution_krw: Option<i64>,
    #[schema(required = true, nullable)]
    pub actual_pension_income_tax_credit_krw: Option<i64>,
    #[schema(required = true, nullable)]
    pub actual_pension_local_income_tax_effect_krw: Option<i64>,
    #[schema(required = true, nullable)]
    pub withheld_income_tax_krw: Option<i64>,
    #[schema(required = true, nullable)]
    pub withheld_local_income_tax_krw: Option<i64>,
    #[schema(required = true, nullable)]
    pub assessed_income_tax_krw: Option<i64>,
    #[schema(required = true, nullable)]
    pub assessed_local_income_tax_krw: Option<i64>,
    #[schema(required = true, nullable)]
    pub additional_tax_krw: Option<i64>,
    #[schema(required = true, nullable)]
    pub refund_krw: Option<i64>,
    #[schema(required = true, nullable)]
    pub reconciliation_game_day: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MilitaryStatusSnapshot {
    Unserved,
    Serving,
    Completed,
    Exempt,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MilitaryServiceTypeSnapshot {
    ActiveDuty,
    SocialService,
    IndustrialTechnical,
    ProfessionalResearch,
    CommissionedOfficer,
    NonCommissionedOfficer,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MilitaryServiceStatusSnapshot {
    PendingStart,
    Serving,
    Completed,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ActiveMilitaryServiceStatusSnapshot {
    PendingStart,
    Serving,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MilitaryServiceSourceKindSnapshot {
    UserCommand,
    LegacyBridge,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MilitaryCompensationKindSnapshot {
    MilitaryPay,
    EmploymentPayroll,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MilitaryPayScheduleSnapshot {
    Monthly,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MilitaryOptionIneligibilityReasonSnapshot {
    MilitarySubjectRequired,
    MilitaryStateConflict,
    MinimumEducation,
    MinimumCertificationCount,
    MinimumExperienceDays,
    PolicyUnavailable,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MilitarySavingsIneligibilityReasonSnapshot {
    MilitaryStateConflict,
    ServiceTypeNotEligible,
    MinimumRemainingService,
    ActiveContractLimit,
    InstitutionLimit,
    JoinWindowClosed,
    PolicyUnavailable,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MilitarySavingsContractStatusSnapshot {
    Active,
    Matured,
    Closed,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ActiveMilitarySavingsStatusSnapshot {
    Active,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MilitarySavingsInstallmentStatusSnapshot {
    Scheduled,
    Paid,
    Missed,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MilitarySavingsClosureReasonSnapshot {
    Maturity,
    EarlyClose,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MilitarySavingsDayCountConventionSnapshot {
    Actual365,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MilitarySavingsInterestRoundingSnapshot {
    FloorToKrw,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MilitarySavingsProjectionAssumptionSnapshot {
    AllScheduledInstallmentsPaid,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MilitaryHardRequirementsSnapshot {
    #[schema(required = true, nullable)]
    pub minimum_education: Option<crate::character::Education>,
    pub required_certification_count: u32,
    pub minimum_experience_days: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MilitaryPayStageSnapshot {
    pub start_service_month: u16,
    pub end_exclusive_service_month: u16,
    pub gross_monthly_pay_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MilitaryExperienceCreditSnapshot {
    pub job_family_key: String,
    pub daily_credit_ppm: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MilitaryOptionSnapshot {
    pub id: ResourceId,
    pub option_key: String,
    pub service_type: MilitaryServiceTypeSnapshot,
    pub display_name: String,
    pub eligible: bool,
    #[schema(max_items = 6)]
    pub ineligibility_reasons: Vec<MilitaryOptionIneligibilityReasonSnapshot>,
    pub service_duration_months: u16,
    pub hard_requirements: MilitaryHardRequirementsSnapshot,
    pub compensation_kind: MilitaryCompensationKindSnapshot,
    pub pay_schedule: MilitaryPayScheduleSnapshot,
    #[schema(max_items = 12)]
    pub pay_stages: Vec<MilitaryPayStageSnapshot>,
    #[schema(value_type = String)]
    pub effort_life_status: LifeStatus,
    pub daily_effort_capacity_units: u64,
    pub grants_career_experience: bool,
    #[schema(max_items = 8)]
    pub experience_credits: Vec<MilitaryExperienceCreditSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MilitaryOptionsResponse {
    #[schema(max_items = 6)]
    pub items: Vec<MilitaryOptionSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActiveMilitaryServiceSummarySnapshot {
    pub id: ResourceId,
    pub option_version_id: ResourceId,
    pub service_type: MilitaryServiceTypeSnapshot,
    pub display_name: String,
    pub status: ActiveMilitaryServiceStatusSnapshot,
    pub start_game_day: u32,
    pub end_game_day: u32,
    pub credited_service_days: u32,
    pub total_service_days: u32,
    #[schema(value_type = String)]
    pub effort_life_status: LifeStatus,
    pub grants_career_experience: bool,
    #[schema(required = true, nullable)]
    pub next_pay_game_day: Option<u32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MilitaryServiceHistorySnapshot {
    pub id: ResourceId,
    pub option_version_id: ResourceId,
    pub service_type: MilitaryServiceTypeSnapshot,
    pub display_name: String,
    pub status: MilitaryServiceStatusSnapshot,
    pub source_kind: MilitaryServiceSourceKindSnapshot,
    pub start_game_day: u32,
    pub end_game_day: u32,
    #[schema(format = Date)]
    pub start_date: String,
    #[schema(format = Date)]
    pub end_exclusive_date: String,
    pub credited_service_days: u32,
    pub total_service_days: u32,
    #[schema(value_type = String)]
    pub effort_life_status: LifeStatus,
    pub grants_career_experience: bool,
    #[schema(required = true, nullable)]
    pub next_pay_game_day: Option<u32>,
    #[schema(required = true, nullable)]
    pub completed_game_day: Option<u32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MilitaryServiceResponse {
    pub military_status: MilitaryStatusSnapshot,
    #[schema(required = true, nullable)]
    pub service: Option<MilitaryServiceHistorySnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MilitarySavingsInterestTierSnapshot {
    pub minimum_term_months: u16,
    pub maximum_term_months_inclusive: u16,
    pub annual_interest_rate_ppm: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MilitarySavingsProductSnapshot {
    pub id: ResourceId,
    pub product_key: String,
    pub institution_key: String,
    pub institution_display_name: String,
    pub eligible: bool,
    #[schema(max_items = 7)]
    pub ineligibility_reasons: Vec<MilitarySavingsIneligibilityReasonSnapshot>,
    #[schema(max_items = 6)]
    pub eligible_service_types: Vec<MilitaryServiceTypeSnapshot>,
    #[schema(format = Date)]
    pub join_start_date: String,
    #[schema(format = Date)]
    pub join_end_date: String,
    pub minimum_remaining_service_months: u16,
    pub maximum_active_contracts: u8,
    pub maximum_contracts_per_institution: u8,
    pub minimum_monthly_contribution_krw: i64,
    pub maximum_institution_monthly_contribution_krw: i64,
    pub maximum_total_monthly_contribution_krw: i64,
    pub limit_setting_unit_krw: i64,
    pub installment_unit_krw: i64,
    #[schema(max_items = 12)]
    pub interest_tiers: Vec<MilitarySavingsInterestTierSnapshot>,
    pub day_count_convention: MilitarySavingsDayCountConventionSnapshot,
    pub interest_rounding: MilitarySavingsInterestRoundingSnapshot,
    pub early_close_annual_interest_rate_ppm: i64,
    pub government_matching_rate_ppm: i64,
    pub government_match_payment_day_of_month: u8,
    pub maturity_tax_exempt: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MilitarySavingsProductsResponse {
    #[schema(max_items = 20)]
    pub items: Vec<MilitarySavingsProductSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActiveMilitarySavingsSummarySnapshot {
    pub id: ResourceId,
    pub product_version_id: ResourceId,
    pub institution_key: String,
    pub status: ActiveMilitarySavingsStatusSnapshot,
    pub monthly_contribution_krw: i64,
    pub debit_day_of_month: u8,
    pub principal_krw: i64,
    pub paid_installment_count: u32,
    pub missed_installment_count: u32,
    #[schema(required = true, nullable)]
    pub next_installment_game_day: Option<u32>,
    pub maturity_game_day: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MilitarySavingsInstallmentSnapshot {
    pub id: ResourceId,
    pub installment_no: u32,
    pub due_game_day: u32,
    pub status: MilitarySavingsInstallmentStatusSnapshot,
    #[schema(required = true, nullable)]
    pub paid_game_day: Option<u32>,
    pub principal_krw: i64,
    #[schema(required = true, nullable)]
    pub government_matching_policy_version_id: Option<ResourceId>,
    #[schema(required = true, nullable)]
    pub government_matching_rate_ppm: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MilitarySavingsMaturityProjectionSnapshot {
    pub assumption: MilitarySavingsProjectionAssumptionSnapshot,
    pub principal_krw: i64,
    pub gross_bank_interest_krw: i64,
    pub government_match_krw: i64,
    pub bank_payout_krw: i64,
    pub total_benefit_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MilitarySavingsHistoryItemSnapshot {
    pub id: ResourceId,
    pub service_id: ResourceId,
    pub product_version_id: ResourceId,
    pub product_key: String,
    pub institution_key: String,
    pub institution_display_name: String,
    pub status: MilitarySavingsContractStatusSnapshot,
    pub monthly_contribution_krw: i64,
    pub debit_day_of_month: u8,
    pub principal_krw: i64,
    pub paid_installment_count: u32,
    pub missed_installment_count: u32,
    #[schema(required = true, nullable)]
    pub next_installment_game_day: Option<u32>,
    pub maturity_game_day: u32,
    pub opened_game_day: u32,
    pub first_installment_game_day: u32,
    pub contract_term_months: u16,
    pub annual_interest_rate_ppm: i64,
    #[schema(required = true, nullable)]
    pub closed_game_day: Option<u32>,
    #[schema(required = true, nullable)]
    pub closure_reason: Option<MilitarySavingsClosureReasonSnapshot>,
    pub settled_principal_krw: i64,
    pub gross_bank_interest_krw: i64,
    pub government_match_krw: i64,
    pub bank_payout_krw: i64,
    #[schema(required = true, nullable)]
    pub government_match_paid_game_day: Option<u32>,
    #[schema(required = true, nullable)]
    pub projected_maturity: Option<MilitarySavingsMaturityProjectionSnapshot>,
    #[schema(max_items = 120)]
    pub installments: Vec<MilitarySavingsInstallmentSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MilitarySavingsHistoryResponse {
    #[schema(max_items = 200)]
    pub items: Vec<MilitarySavingsHistoryItemSnapshot>,
    #[schema(required = true, nullable)]
    pub next_before: Option<ResourceId>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MilitaryServiceResultSnapshot {
    pub military_service_id: ResourceId,
    pub status: ActiveMilitaryServiceStatusSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MilitarySavingsResultSnapshot {
    pub military_savings_contract_id: ResourceId,
    pub status: MilitarySavingsContractStatusSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MilitaryServiceCommandResponse {
    pub result: MilitaryServiceResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MilitarySavingsCommandResponse {
    pub result: MilitarySavingsResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerEvidenceSnapshot {
    pub id: ResourceId,
    pub evidence_key: String,
    pub catalog_entry_id: ResourceId,
    pub catalog_entry_key: String,
    pub display_name: String,
    #[schema(value_type = String)]
    pub kind: EvidenceKind,
    pub acquired_game_day: u32,
    #[schema(required = true, nullable)]
    pub expires_on_game_day: Option<u32>,
    #[schema(required = true, nullable, format = Date)]
    pub period_start_date: Option<String>,
    #[schema(required = true, nullable, format = Date)]
    pub period_end_exclusive_date: Option<String>,
    #[schema(required = true, nullable)]
    pub credited_experience_days: Option<u32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerSpecsResponse {
    pub focused_job_family_key: String,
    pub possessed_scores: CareerScoresSnapshot,
    #[schema(max_items = 200)]
    pub items: Vec<CareerEvidenceSnapshot>,
    #[schema(required = true, nullable)]
    pub next_before: Option<ResourceId>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerActivityCatalogSnapshot {
    pub id: ResourceId,
    pub activity_key: String,
    pub display_name: String,
    #[schema(value_type = String)]
    pub output_kind: EvidenceKind,
    pub minimum_calendar_days: u32,
    pub required_effort_units: u64,
    pub daily_effort_cap_units: u64,
    #[schema(value_type = Vec<String>, max_items = 6)]
    pub allowed_life_statuses: Vec<LifeStatus>,
    pub cost_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerActivityHistorySnapshot {
    pub id: ResourceId,
    pub catalog_entry_id: ResourceId,
    pub activity_key: String,
    pub display_name: String,
    #[schema(value_type = String)]
    pub status: ActivityStatus,
    #[schema(required = true, nullable)]
    pub priority: Option<u8>,
    #[schema(required = true, nullable)]
    pub started_game_day: Option<u32>,
    pub accumulated_effort_units: u64,
    pub required_effort_units: u64,
    pub elapsed_calendar_days: u32,
    pub minimum_calendar_days: u32,
    pub daily_effort_cap_units: u64,
    #[schema(required = true, nullable)]
    pub completed_game_day: Option<u32>,
    #[schema(required = true, nullable)]
    pub cancelled_game_day: Option<u32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerActivitiesResponse {
    #[schema(max_items = 200)]
    pub catalog: Vec<CareerActivityCatalogSnapshot>,
    #[schema(max_items = 3)]
    pub active: Vec<CareerActivitySnapshot>,
    #[schema(max_items = 200)]
    pub items: Vec<CareerActivityHistorySnapshot>,
    #[schema(required = true, nullable)]
    pub next_before: Option<ResourceId>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CareerArtifactVersionSnapshot {
    Portfolio {
        id: ResourceId,
        version_no: u32,
        headline: String,
        summary: String,
        #[schema(max_items = 12)]
        evidence_ids: Vec<ResourceId>,
        completeness_bp: i64,
        created_game_day: u32,
    },
    Resume {
        id: ResourceId,
        version_no: u32,
        headline: String,
        summary: String,
        #[schema(max_items = 40)]
        evidence_ids: Vec<ResourceId>,
        completeness_bp: i64,
        created_game_day: u32,
    },
    LinkedinProfile {
        id: ResourceId,
        version_no: u32,
        headline: String,
        summary: String,
        #[schema(max_items = 30)]
        evidence_ids: Vec<ResourceId>,
        completeness_bp: i64,
        created_game_day: u32,
        open_to_work: bool,
        #[schema(value_type = Vec<String>, max_items = 3)]
        industries: Vec<Industry>,
    },
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerArtifactsResponse {
    #[schema(max_items = 200)]
    pub items: Vec<CareerArtifactVersionSnapshot>,
    #[schema(required = true, nullable)]
    pub next_before: Option<ResourceId>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerFocusResultSnapshot {
    pub focused_job_family_key: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerActivityResultSnapshot {
    pub activity_id: ResourceId,
    #[schema(value_type = String)]
    pub status: ActivityStatus,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerArtifactResultSnapshot {
    pub artifact_version_id: ResourceId,
    #[schema(value_type = String)]
    pub kind: ArtifactKind,
    pub version_no: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerFocusResponse {
    pub result: CareerFocusResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerActivityResponse {
    pub result: CareerActivityResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerArtifactResponse {
    pub result: CareerArtifactResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerApplicationResultSnapshot {
    pub application_id: ResourceId,
    #[schema(value_type = String)]
    pub status: crate::store::CareerApplicationStatus,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerInvitationResultSnapshot {
    pub invitation_id: ResourceId,
    #[schema(value_type = String)]
    pub status: crate::store::CareerInvitationStatus,
    #[schema(required = true, nullable)]
    pub application_id: Option<ResourceId>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerOfferResultSnapshot {
    pub offer_id: ResourceId,
    #[schema(value_type = String)]
    pub status: crate::store::CareerApplicationStatus,
    #[schema(required = true, nullable)]
    pub employment_contract_id: Option<ResourceId>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerApplicationResponse {
    pub result: CareerApplicationResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerInvitationResponse {
    pub result: CareerInvitationResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CareerOfferResponse {
    pub result: CareerOfferResultSnapshot,
    pub replayed: bool,
    pub snapshot: GameSnapshot,
}

pub enum CareerCommandResult<T> {
    Applied(Box<T>),
    Rejected(CareerFailureCode),
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GameCommandCursorSnapshot {
    pub run_revision: u32,
    pub state_revision: u64,
    pub game_day: u32,
}

impl From<GameCommandCursor> for GameCommandCursorSnapshot {
    fn from(cursor: GameCommandCursor) -> Self {
        Self {
            run_revision: cursor.run_revision,
            state_revision: cursor.state_revision,
            game_day: cursor.game_day,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CharacterStartSnapshot {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    pub command_id: String,
    pub committed_cursor: GameCommandCursorSnapshot,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CharacterStartResponse {
    pub start: CharacterStartSnapshot,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdvanceCommandSnapshot {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    pub command_id: String,
    pub requested_days: u32,
    pub initial_cursor: GameCommandCursorSnapshot,
    pub committed_cursor: GameCommandCursorSnapshot,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdvanceResponse {
    pub advance: AdvanceCommandSnapshot,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FinanceSnapshot {
    pub policy_set: PolicySetSnapshot,
    #[schema(max_items = 32)]
    pub accounts: Vec<FinancialAccountSnapshot>,
    #[schema(max_items = 32)]
    pub cma_accounts: Vec<CmaAccountSnapshot>,
    #[schema(max_items = 100)]
    pub cash_contracts: Vec<CashContractSnapshot>,
    #[schema(max_items = 16)]
    pub deposit_protection: Vec<DepositProtectionSnapshot>,
    pub current_tax_year: FinancialIncomeYearSnapshot,
    #[schema(max_items = 1)]
    pub isa_accounts: Vec<IsaAccountSnapshot>,
    #[schema(max_items = 2)]
    pub pension_accounts: Vec<PensionAccountSnapshot>,
    #[schema(required = true, nullable)]
    pub product_bundle: Option<ProductBundleSnapshot>,
    #[schema(max_items = 8)]
    pub llx_distribution_entitlements: Vec<LlxDistributionEntitlementSnapshot>,
    #[schema(max_items = 640)]
    pub bond_positions: Vec<BondPositionSnapshot>,
    #[schema(max_items = 1)]
    pub gold_accounts: Vec<GoldAccountSnapshot>,
    #[schema(max_items = 2)]
    pub physical_gold_holdings: Vec<PhysicalGoldHoldingSnapshot>,
    #[schema(required = true, nullable)]
    pub latest_financial_income_assessment: Option<FinancialIncomeAssessmentSnapshot>,
    #[schema(max_items = 20)]
    pub pending_settlements: Vec<PendingSettlementSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IsaAccountSnapshot {
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub account_id: ResourceId,
    #[serde(rename = "type")]
    pub account_type: FinancialAccountType,
    pub opened_game_day: u32,
    pub minimum_term_game_day: u32,
    #[schema(minimum = 0)]
    pub total_contribution_krw: i64,
    #[schema(minimum = 0)]
    pub principal_withdrawal_krw: i64,
    #[schema(minimum = 0)]
    pub contribution_capacity_krw: i64,
    #[schema(minimum = 0)]
    pub tax_profit_krw: i64,
    #[schema(minimum = 0)]
    pub deductible_loss_krw: i64,
    #[schema(minimum = 0)]
    pub expected_close_income_tax_krw: i64,
    #[schema(minimum = 0)]
    pub expected_close_local_income_tax_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PensionTaxLayersSnapshot {
    #[schema(minimum = 0)]
    pub tax_excluded_contribution_krw: i64,
    #[schema(minimum = 0)]
    pub deferred_retirement_income_krw: i64,
    #[schema(minimum = 0)]
    pub credited_contribution_krw: i64,
    #[schema(minimum = 0)]
    pub earnings_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PensionAccountSnapshot {
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub account_id: ResourceId,
    #[serde(rename = "type")]
    pub account_type: FinancialAccountType,
    pub opened_game_day: u32,
    pub eligible_pension_start_game_day: u32,
    pub pension_started: bool,
    pub tax_layers: PensionTaxLayersSnapshot,
    #[schema(minimum = 0)]
    pub current_year_contribution_krw: i64,
    #[schema(minimum = 0)]
    pub current_year_credit_eligible_krw: i64,
    #[schema(minimum = 0)]
    pub expected_credit_krw: i64,
    #[schema(required = true, nullable, minimum = 0)]
    pub current_year_pension_limit_krw: Option<i64>,
    #[schema(minimum = 0)]
    pub current_year_pension_withdrawn_krw: i64,
    #[schema(minimum = 0)]
    pub risk_asset_value_krw: i64,
    #[schema(minimum = 0)]
    pub total_value_krw: i64,
    #[schema(minimum = 0, maximum = 1000000)]
    pub risk_asset_ratio_ppm: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CmaAccountSnapshot {
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub account_id: ResourceId,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub product_version_id: ResourceId,
    #[schema(required = true, nullable, minimum = 0)]
    pub annual_rate_bp: Option<i32>,
    #[schema(minimum = 1)]
    pub minimum_interest_balance_krw: i64,
    #[schema(minimum = 0)]
    pub interest_remainder: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum DepositKindSnapshot {
    TermDeposit,
    InstallmentSavings,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CashContractSnapshot {
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub contract_id: ResourceId,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub product_version_id: ResourceId,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub settlement_account_id: ResourceId,
    pub kind: DepositKindSnapshot,
    pub status: CashProductContractStatus,
    #[schema(minimum = 0, maximum = 10000)]
    pub annual_rate_bp: i32,
    #[schema(minimum = 0)]
    pub current_principal_krw: i64,
    #[schema(required = true, nullable, minimum = 1)]
    pub installment_amount_krw: Option<i64>,
    pub paid_installment_count: u32,
    pub missed_installment_count: u32,
    pub opened_game_day: u32,
    pub maturity_game_day: u32,
    #[schema(required = true, nullable, minimum = 0)]
    pub expected_gross_interest_krw: Option<i64>,
    #[schema(required = true, nullable, minimum = 0)]
    pub expected_income_tax_krw: Option<i64>,
    #[schema(required = true, nullable, minimum = 0)]
    pub expected_local_income_tax_krw: Option<i64>,
    #[schema(required = true, nullable, minimum = 0)]
    pub expected_net_payout_krw: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DepositProtectionSnapshot {
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub institution_id: ResourceId,
    #[schema(minimum = 0)]
    pub eligible_amount_krw: i64,
    #[schema(minimum = 0)]
    pub protected_amount_krw: i64,
    #[schema(minimum = 0)]
    pub unprotected_amount_krw: i64,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum FinancialIncomeYearStatusSnapshot {
    NotApplicable,
    Open,
    FinalizedNoFiling,
    FilingPending,
    Filed,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FinancialIncomeSourceSnapshot {
    pub source: crate::finance::FinancialIncomeSource,
    #[schema(minimum = 0)]
    pub gross_financial_income_krw: i64,
    #[schema(minimum = 0)]
    pub withheld_income_tax_krw: i64,
    #[schema(minimum = 0)]
    pub withheld_local_income_tax_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FinancialIncomeYearSnapshot {
    #[schema(minimum = 1, maximum = 9999)]
    pub tax_year: u16,
    pub status: FinancialIncomeYearStatusSnapshot,
    #[schema(max_items = 5)]
    pub sources: Vec<FinancialIncomeSourceSnapshot>,
    #[schema(minimum = 0)]
    pub gross_financial_income_krw: i64,
    #[schema(minimum = 0)]
    pub withheld_income_tax_krw: i64,
    #[schema(minimum = 0)]
    pub withheld_local_income_tax_krw: i64,
    #[schema(required = true, nullable, minimum = 0)]
    pub comparison_a_income_tax_krw: Option<i64>,
    #[schema(required = true, nullable, minimum = 0)]
    pub comparison_a_local_income_tax_krw: Option<i64>,
    #[schema(required = true, nullable, minimum = 0)]
    pub comparison_b_income_tax_krw: Option<i64>,
    #[schema(required = true, nullable, minimum = 0)]
    pub comparison_b_local_income_tax_krw: Option<i64>,
    #[schema(required = true, nullable, minimum = 0)]
    pub assessed_income_tax_krw: Option<i64>,
    #[schema(required = true, nullable, minimum = 0)]
    pub assessed_local_income_tax_krw: Option<i64>,
    #[schema(required = true, nullable, minimum = 0)]
    pub additional_tax_krw: Option<i64>,
    #[schema(required = true, nullable, minimum = 0)]
    pub refund_krw: Option<i64>,
    #[schema(required = true, nullable, format = Date)]
    pub filing_due_date: Option<String>,
    #[schema(required = true, nullable, minimum = 0)]
    pub filed_game_day: Option<u32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FinancialIncomeAssessmentSnapshot {
    #[schema(minimum = 1, maximum = 9999)]
    pub tax_year: u16,
    pub status: FinancialIncomeYearStatusSnapshot,
    #[schema(minimum = 0)]
    pub gross_financial_income_krw: i64,
    #[schema(minimum = 0)]
    pub withheld_income_tax_krw: i64,
    #[schema(minimum = 0)]
    pub withheld_local_income_tax_krw: i64,
    #[schema(minimum = 0)]
    pub comparison_a_income_tax_krw: i64,
    #[schema(minimum = 0)]
    pub comparison_a_local_income_tax_krw: i64,
    #[schema(minimum = 0)]
    pub comparison_b_income_tax_krw: i64,
    #[schema(minimum = 0)]
    pub comparison_b_local_income_tax_krw: i64,
    #[schema(minimum = 0)]
    pub assessed_income_tax_krw: i64,
    #[schema(minimum = 0)]
    pub assessed_local_income_tax_krw: i64,
    #[schema(minimum = 0)]
    pub additional_tax_krw: i64,
    #[schema(minimum = 0)]
    pub refund_krw: i64,
    #[schema(required = true, nullable, format = Date)]
    pub filing_due_date: Option<String>,
    #[schema(required = true, nullable, minimum = 0)]
    pub filed_game_day: Option<u32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PolicySetSnapshot {
    #[schema(min_length = 1)]
    pub key: String,
    #[schema(format = Date)]
    pub basis_date: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FinancialAccountSnapshot {
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub id: ResourceId,
    #[serde(rename = "type")]
    pub account_type: FinancialAccountType,
    pub status: FinancialAccountStatus,
    #[schema(minimum = 0)]
    pub cash_krw: i64,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PendingSettlementSnapshot {
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub id: ResourceId,
    pub due_game_day: u32,
    pub kind: SettlementKind,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioOrderResponse {
    pub execution: TradeExecution,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BondOrderResponse {
    pub bond_order: BondOrderReceipt,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GoldAccountOpenResponse {
    pub account: OpenGoldAccountReceipt,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GoldOrderResponse {
    pub gold_order: GoldOrderReceipt,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GoldWithdrawalResponse {
    pub gold_withdrawal: GoldWithdrawalReceipt,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone)]
pub enum AssetCommandResult<T> {
    Applied(Box<T>),
    Rejected(FinanceFailureCode),
}

#[derive(Debug, Clone)]
pub enum PlaceOrderResult {
    Executed(Box<PortfolioOrderResponse>),
    Rejected(TradeFailure),
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FinanceAccountsResponse {
    pub policy_set: PolicySetSnapshot,
    pub accounts: Vec<FinancialAccountSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CashProductCatalogResponse {
    #[schema(max_items = 100)]
    pub products: Vec<CashProductVersionSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FinancialInstitutionSnapshot {
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub id: ResourceId,
    #[schema(min_length = 1, max_length = 64)]
    pub key: String,
    #[schema(min_length = 1, max_length = 100)]
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CashProductVersionSnapshot {
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub id: ResourceId,
    #[schema(min_length = 1, max_length = 64)]
    pub key: String,
    pub kind: CashProductKind,
    #[schema(min_length = 1, max_length = 100)]
    pub display_name: String,
    pub institution: FinancialInstitutionSnapshot,
    pub protection_eligible: bool,
    pub rate_reference: CashRateReference,
    #[schema(minimum = -10000, maximum = 10000)]
    pub spread_bp: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(minimum = 1)]
    pub minimum_interest_balance_krw: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(minimum = 1)]
    pub minimum_contribution_krw: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(minimum = 1)]
    pub maximum_contribution_krw: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(minimum = 1)]
    pub term_days: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(minimum = 1)]
    pub term_months: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(minimum = 1)]
    pub installment_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(minimum = 0, maximum = 10000)]
    pub early_termination_rate_bp: Option<i32>,
    #[schema(minimum = 1)]
    pub day_count_denominator: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CmaAccountOpenResponse {
    pub account: CmaAccountOpenSnapshot,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CmaAccountOpenSnapshot {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    pub command_id: String,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub account_id: ResourceId,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub product_version_id: ResourceId,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CmaAccountCloseResponse {
    pub account_close: CmaAccountCloseSnapshot,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CmaAccountCloseSnapshot {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    pub command_id: String,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub account_id: ResourceId,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DepositOpenResponse {
    pub deposit: DepositOpenSnapshot,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DepositOpenSnapshot {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    pub command_id: String,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub contract_id: ResourceId,
    pub kind: DepositKindSnapshot,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub product_version_id: ResourceId,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub settlement_account_id: ResourceId,
    #[schema(minimum = 1)]
    pub amount_krw: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DepositCloseResponse {
    pub deposit_close: DepositCloseSnapshot,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DepositCloseSnapshot {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    pub command_id: String,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub contract_id: ResourceId,
    #[schema(minimum = 0)]
    pub gross_interest_krw: i64,
    #[schema(minimum = 0)]
    pub income_tax_krw: i64,
    #[schema(minimum = 0)]
    pub local_income_tax_krw: i64,
    #[schema(minimum = 0)]
    pub net_payout_krw: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone)]
pub enum CashProductCommandResult<T> {
    Applied(Box<T>),
    Rejected(FinanceFailureCode),
}

#[derive(Debug, Clone)]
pub enum TaxAccountCommandResult<T> {
    Applied(Box<T>),
    Rejected(FinanceFailureCode),
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaxAccountOpenResponse {
    pub account: TaxAccountOpenSnapshot,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaxAccountOpenSnapshot {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    pub command_id: String,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub account_id: ResourceId,
    #[serde(rename = "type")]
    pub account_type: FinancialAccountType,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IsaCloseResponse {
    pub isa_close: IsaCloseSnapshot,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IsaCloseSnapshot {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    pub command_id: String,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub account_id: ResourceId,
    #[schema(minimum = 0)]
    pub gross_tax_profit_krw: i64,
    #[schema(minimum = 0)]
    pub deductible_loss_krw: i64,
    #[schema(minimum = 0)]
    pub income_tax_krw: i64,
    #[schema(minimum = 0)]
    pub local_income_tax_krw: i64,
    #[schema(minimum = 0)]
    pub net_payout_krw: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PensionStartResponse {
    pub pension_start: PensionStartSnapshot,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PensionStartSnapshot {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    pub command_id: String,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub account_id: ResourceId,
    #[schema(minimum = 1, maximum = 9999)]
    pub start_tax_year: u16,
    #[schema(minimum = 5, maximum = 100)]
    pub payment_years: u16,
    pub lifetime: bool,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PensionWithdrawalResponse {
    pub pension_withdrawal: PensionWithdrawalSnapshot,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PensionWithdrawalSnapshot {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    pub command_id: String,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub account_id: ResourceId,
    #[schema(minimum = 1)]
    pub gross_amount_krw: i64,
    #[schema(minimum = 0)]
    pub pension_amount_krw: i64,
    #[schema(minimum = 0)]
    pub non_pension_amount_krw: i64,
    #[schema(minimum = 0)]
    pub tax_free_amount_krw: i64,
    #[schema(minimum = 0)]
    pub tax_krw: i64,
    #[schema(minimum = 0)]
    pub net_payout_krw: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FinanceTransferResponse {
    pub transfer: FinanceTransferSnapshot,
    pub snapshot: GameSnapshot,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FinanceTransferSnapshot {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    pub command_id: String,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub account_id: ResourceId,
    pub direction: TransferDirection,
    #[schema(minimum = 1)]
    pub amount_krw: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone)]
pub enum FinanceCommandResult {
    Transferred(Box<FinanceTransferResponse>),
    Rejected(FinanceFailureCode),
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LedgerPageResponse {
    #[schema(max_items = 200)]
    pub transactions: Vec<LedgerTransactionSnapshot>,
    #[schema(
        required = true,
        value_type = String,
        nullable,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub next_before: Option<ResourceId>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LedgerTransactionSnapshot {
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub id: ResourceId,
    pub game_day: u32,
    #[schema(min_length = 1)]
    pub description: String,
    pub source_kind: LedgerSourceKind,
    #[schema(min_items = 2)]
    pub postings: Vec<LedgerPostingSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LedgerPostingSnapshot {
    pub account_code: LedgerAccountCode,
    #[schema(
        required = true,
        value_type = String,
        nullable,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    pub account_id: Option<ResourceId>,
    pub amount_krw: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketSnapshot {
    pub world: String,
    pub date: String,
    pub open: bool,
    pub regime: MarketRegime,
    pub index: MarketIndexSnapshot,
    #[schema(required = true)]
    pub rates: Option<MarketRatesSnapshot>,
    #[schema(required = true)]
    pub m2_factors: Option<M2MarketFactorsSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct M2MarketFactorsSnapshot {
    pub cpi_index: i64,
    pub llx_close_krw: i64,
    pub gold_close_krw_per_gram: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketRatesSnapshot {
    pub policy_rate_bp: i64,
    pub treasury_3m_bp: i64,
    pub treasury_1y_bp: i64,
    pub treasury_3y_bp: i64,
    pub treasury_10y_bp: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketIndexSnapshot {
    pub symbol: &'static str,
    pub name: &'static str,
    pub close_krw: i64,
    pub daily_return_ppm: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketHistoryPoint {
    pub game_day: u32,
    pub date: String,
    pub open: bool,
    pub close_krw: i64,
    pub daily_return_ppm: i64,
    #[schema(required = true)]
    pub llx_close_krw: Option<i64>,
    #[schema(required = true)]
    pub llx_daily_return_ppm: Option<i64>,
    pub regime: MarketRegime,
    #[schema(required = true)]
    pub rates: Option<MarketRatesSnapshot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketHistoryResponse {
    pub world: String,
    pub symbol: &'static str,
    pub through_game_day: u32,
    pub points: Vec<MarketHistoryPoint>,
}

#[derive(Debug)]
pub enum GameLoopError {
    InvalidCommand,
    InvalidCharacter(Vec<crate::character::ValidationError>),
    IdempotencyConflict,
    Busy,
    CharacterRequired,
    ActiveStreamRequired,
    Internal(anyhow::Error),
}

impl From<GameCommandRejection> for GameLoopError {
    fn from(rejection: GameCommandRejection) -> Self {
        match rejection {
            GameCommandRejection::InvalidCommand => Self::InvalidCommand,
            GameCommandRejection::InvalidCharacter(errors) => Self::InvalidCharacter(errors),
            GameCommandRejection::IdempotencyConflict => Self::IdempotencyConflict,
            GameCommandRejection::Busy => Self::Busy,
            GameCommandRejection::CharacterRequired => Self::CharacterRequired,
        }
    }
}

impl From<anyhow::Error> for GameLoopError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(error)
    }
}

#[async_trait]
trait GameTimer: Send + Sync + 'static {
    async fn wait(&self, duration: Duration);
}

struct TokioGameTimer;

#[async_trait]
impl GameTimer for TokioGameTimer {
    async fn wait(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RuntimeSignal {
    generation: u64,
    speed: Option<AutoSpeed>,
}

enum RuntimeClock {
    Paused {
        last_committed: Option<CommittedGameState>,
    },
    Running {
        speed: AutoSpeed,
        last_committed: CommittedGameState,
    },
}

struct RuntimeControl {
    generation: u64,
    clock: RuntimeClock,
    active_streams: usize,
}

impl RuntimeControl {
    fn signal(&self) -> RuntimeSignal {
        RuntimeSignal {
            generation: self.generation,
            speed: match &self.clock {
                RuntimeClock::Paused { .. } => None,
                RuntimeClock::Running { speed, .. } => Some(*speed),
            },
        }
    }

    fn record_committed(&mut self, state: &CommittedGameState) {
        match &mut self.clock {
            RuntimeClock::Paused { last_committed } => {
                *last_committed = Some(state.clone());
            }
            RuntimeClock::Running { last_committed, .. } => {
                *last_committed = state.clone();
            }
        }
    }
}

struct SaveRuntime {
    /// All mutations of one account-owned save linearize through this lock.
    operation: Mutex<()>,
    control: StdMutex<RuntimeControl>,
    changes: watch::Sender<RuntimeSignal>,
    ticks: broadcast::Sender<GameSnapshot>,
}

impl SaveRuntime {
    fn new() -> Self {
        let signal = RuntimeSignal {
            generation: 0,
            speed: None,
        };
        let (changes, _) = watch::channel(signal);
        let (ticks, _) = broadcast::channel(256);

        Self {
            operation: Mutex::new(()),
            control: StdMutex::new(RuntimeControl {
                generation: signal.generation,
                clock: RuntimeClock::Paused {
                    last_committed: None,
                },
                active_streams: 0,
            }),
            changes,
            ticks,
        }
    }

    fn control(&self) -> MutexGuard<'_, RuntimeControl> {
        self.control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn auto_speed(&self) -> Option<AutoSpeed> {
        self.control().signal().speed
    }

    fn is_active(&self, expected: RuntimeSignal) -> bool {
        self.control().signal() == expected
    }

    fn record_committed(&self, state: &CommittedGameState) -> Option<AutoSpeed> {
        let mut control = self.control();
        control.record_committed(state);
        control.signal().speed
    }

    fn start(&self, speed: AutoSpeed, state: &CommittedGameState) -> Result<(), GameLoopError> {
        let mut control = self.control();
        if control.active_streams == 0 {
            return Err(GameLoopError::ActiveStreamRequired);
        }
        if let RuntimeClock::Running {
            speed: current,
            last_committed,
        } = &mut control.clock
            && *current == speed
        {
            *last_committed = state.clone();
            return Ok(());
        }

        control.generation = control.generation.wrapping_add(1);
        control.clock = RuntimeClock::Running {
            speed,
            last_committed: state.clone(),
        };
        // Control and its watch publication are one linearized transition. Every reader
        // that observes the new watch value can therefore validate it against control.
        self.changes.send_replace(control.signal());

        Ok(())
    }

    fn pause(&self) -> Option<CommittedGameState> {
        let mut control = self.control();
        let last_committed = match &control.clock {
            RuntimeClock::Paused { .. } => return None,
            RuntimeClock::Running { last_committed, .. } => last_committed.clone(),
        };
        control.generation = control.generation.wrapping_add(1);
        control.clock = RuntimeClock::Paused {
            last_committed: Some(last_committed.clone()),
        };
        self.changes.send_replace(control.signal());

        Some(last_committed)
    }

    fn pause_if_active(&self, expected: RuntimeSignal) -> Option<CommittedGameState> {
        let mut control = self.control();
        if control.signal() != expected {
            return None;
        }
        let last_committed = match &control.clock {
            RuntimeClock::Paused { .. } => return None,
            RuntimeClock::Running { last_committed, .. } => last_committed.clone(),
        };
        control.generation = control.generation.wrapping_add(1);
        control.clock = RuntimeClock::Paused {
            last_committed: Some(last_committed.clone()),
        };
        self.changes.send_replace(control.signal());

        Some(last_committed)
    }

    fn connect(self: &Arc<Self>) -> StreamConnection {
        self.control().active_streams += 1;
        StreamConnection {
            runtime: Arc::clone(self),
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<GameSnapshot> {
        self.ticks.subscribe()
    }

    fn disconnect(&self) {
        let mut control = self.control();
        if control.active_streams == 0 {
            return;
        }

        control.active_streams -= 1;
        if control.active_streams > 0 {
            return;
        }
        let last_committed = match &control.clock {
            RuntimeClock::Paused { .. } => return,
            RuntimeClock::Running { last_committed, .. } => last_committed.clone(),
        };
        control.generation = control.generation.wrapping_add(1);
        control.clock = RuntimeClock::Paused {
            last_committed: Some(last_committed),
        };
        self.changes.send_replace(control.signal());
    }

    #[cfg(test)]
    fn control_matches_published_signal(&self) -> bool {
        let control = self.control();
        control.signal() == *self.changes.borrow()
    }
}

/// Keeps an SSE connection counted until Axum drops its response body.
pub(crate) struct StreamConnection {
    runtime: Arc<SaveRuntime>,
}

impl Drop for StreamConnection {
    fn drop(&mut self) {
        self.runtime.disconnect();
    }
}

pub(crate) struct StreamSubscription {
    current: GameSnapshot,
    receiver: broadcast::Receiver<GameSnapshot>,
    connection: StreamConnection,
}

impl StreamSubscription {
    pub(crate) fn into_parts(
        self,
    ) -> (
        GameSnapshot,
        broadcast::Receiver<GameSnapshot>,
        StreamConnection,
    ) {
        (self.current, self.receiver, self.connection)
    }
}

/// The server owns day advancement (§4.2); a client only asks how far.
///
/// State itself lives in the database (§4.4). Runtime entries serialize save mutations,
/// own online clock state and broadcast only what has been committed.
pub struct AppState {
    games: Arc<dyn DailyPipeline>,
    trades: Arc<dyn TradingStore>,
    finances: Arc<dyn FinanceStore>,
    cash_products: Arc<dyn CashProductStore>,
    assets: Arc<dyn M2dAssetStore>,
    tax_accounts: Arc<dyn TaxAccountStore>,
    careers: Arc<dyn CareerStore>,
    lives: Arc<dyn LifeStore>,
    markets: Arc<dyn MarketStore>,
    runs: Arc<dyn RunStore>,
    users: Arc<dyn UserStore>,
    pub providers: Providers,
    runtimes: StdMutex<HashMap<u64, Arc<SaveRuntime>>>,
    timer: Arc<dyn GameTimer>,
}

pub struct AppStores {
    games: Arc<dyn DailyPipeline>,
    trades: Arc<dyn TradingStore>,
    finances: Arc<dyn FinanceStore>,
    cash_products: Arc<dyn CashProductStore>,
    assets: Arc<dyn M2dAssetStore>,
    tax_accounts: Arc<dyn TaxAccountStore>,
    careers: Arc<dyn CareerStore>,
    lives: Arc<dyn LifeStore>,
    markets: Arc<dyn MarketStore>,
    runs: Arc<dyn RunStore>,
    users: Arc<dyn UserStore>,
}

pub struct AppStoreDependencies {
    pub games: Arc<dyn DailyPipeline>,
    pub trades: Arc<dyn TradingStore>,
    pub finances: Arc<dyn FinanceStore>,
    pub cash_products: Arc<dyn CashProductStore>,
    pub assets: Arc<dyn M2dAssetStore>,
    pub tax_accounts: Arc<dyn TaxAccountStore>,
    pub careers: Arc<dyn CareerStore>,
    pub lives: Arc<dyn LifeStore>,
    pub markets: Arc<dyn MarketStore>,
    pub runs: Arc<dyn RunStore>,
    pub users: Arc<dyn UserStore>,
}

pub fn create_app_stores(dependencies: AppStoreDependencies) -> AppStores {
    let AppStoreDependencies {
        games,
        trades,
        finances,
        cash_products,
        assets,
        tax_accounts,
        careers,
        lives,
        markets,
        runs,
        users,
    } = dependencies;
    AppStores {
        games,
        trades,
        finances,
        cash_products,
        assets,
        tax_accounts,
        careers,
        lives,
        markets,
        runs,
        users,
    }
}

struct AppStateDependencies {
    stores: AppStores,
    providers: Providers,
}

impl AppState {
    pub fn new(stores: AppStores, providers: Providers) -> Arc<Self> {
        Self::from_dependencies(
            AppStateDependencies { stores, providers },
            Arc::new(TokioGameTimer),
        )
    }

    #[cfg(test)]
    fn new_with_timer(dependencies: AppStateDependencies, timer: Arc<dyn GameTimer>) -> Arc<Self> {
        Self::from_dependencies(dependencies, timer)
    }

    fn from_dependencies(
        dependencies: AppStateDependencies,
        timer: Arc<dyn GameTimer>,
    ) -> Arc<Self> {
        Arc::new(Self {
            games: dependencies.stores.games,
            trades: dependencies.stores.trades,
            finances: dependencies.stores.finances,
            cash_products: dependencies.stores.cash_products,
            assets: dependencies.stores.assets,
            tax_accounts: dependencies.stores.tax_accounts,
            careers: dependencies.stores.careers,
            lives: dependencies.stores.lives,
            markets: dependencies.stores.markets,
            runs: dependencies.stores.runs,
            users: dependencies.stores.users,
            providers: dependencies.providers,
            runtimes: StdMutex::new(HashMap::new()),
            timer,
        })
    }

    fn runtime(self: &Arc<Self>, user_id: u64) -> Arc<SaveRuntime> {
        let runtime = {
            let mut runtimes = self
                .runtimes
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(runtime) = runtimes.get(&user_id) {
                return Arc::clone(runtime);
            }

            let runtime = Arc::new(SaveRuntime::new());
            runtimes.insert(user_id, Arc::clone(&runtime));
            runtime
        };

        self.spawn_runner(user_id, &runtime);
        runtime
    }

    pub async fn run_options(&self) -> Result<RunOptions> {
        self.runs.run_options().await
    }

    pub async fn preview_point_budget(
        &self,
        version_id: ResourceId,
        selections: &[PointSelection],
    ) -> Result<Option<PointBudgetEvaluation>> {
        self.runs.preview_point_budget(version_id, selections).await
    }

    fn spawn_runner(self: &Arc<Self>, user_id: u64, runtime: &Arc<SaveRuntime>) {
        let state = Arc::downgrade(self);
        let runtime = Arc::downgrade(runtime);
        let timer = Arc::clone(&self.timer);

        tokio::spawn(async move {
            run_automatic_clock(user_id, state, runtime, timer).await;
        });
    }

    /// Resolves a session cookie token to a user. `None` when absent or expired.
    pub async fn authenticate(&self, token: &str) -> Result<Option<AccountUser>> {
        self.users.find_by_session(&token_hash_of(token)).await
    }

    pub fn users(&self) -> &Arc<dyn UserStore> {
        &self.users
    }

    /// Opens a session and returns the raw token to put in the cookie.
    pub async fn open_session(&self, user_id: u64, ttl: Duration) -> Result<String> {
        let token = crate::auth::random_token()?;
        self.users
            .open_session(user_id, &token_hash_of(&token), ttl)
            .await?;

        Ok(token)
    }

    pub async fn close_session(&self, token: &str) -> Result<()> {
        self.users.close_session(&token_hash_of(token)).await
    }

    pub async fn snapshot(self: &Arc<Self>, user_id: u64) -> Result<GameSnapshot> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let state = self.games.load(user_id).await?;

        to_snapshot(&state, runtime.auto_speed())
    }

    /// Atomically subscribes before releasing the save operation lock, closing the
    /// current-snapshot versus next-tick race.
    pub(crate) async fn open_stream(self: &Arc<Self>, user_id: u64) -> Result<StreamSubscription> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let receiver = runtime.subscribe();
        let state = self.games.load(user_id).await?;
        let connection = runtime.connect();
        let auto_speed = runtime.record_committed(&state);

        Ok(StreamSubscription {
            current: to_snapshot(&state, auto_speed)?,
            receiver,
            connection,
        })
    }

    /// Advances in daily transactions and pushes every committed snapshot.
    pub async fn advance(
        self: &Arc<Self>,
        user_id: u64,
        command: &ManualAdvanceCommand,
    ) -> Result<AdvanceResponse, GameLoopError> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let mut paused_state = runtime.pause();
        for _ in 0..command.days.max(1) {
            let outcome = match self.games.advance_command_step(user_id, command).await {
                Ok(outcome) => outcome,
                Err(error) => {
                    if let Some(state) = paused_state.take() {
                        self.broadcast(&state, &runtime)?;
                    }
                    return Err(error.into());
                }
            };
            match outcome {
                DailyCommandAdvanceResult::Advanced { state, receipt } => {
                    // The first daily tick carries the externally visible paused state.
                    paused_state = None;
                    let snapshot = self.broadcast(&state, &runtime)?;
                    if let Some(receipt) = receipt {
                        return Ok(to_advance_response(receipt, snapshot));
                    }
                }
                DailyCommandAdvanceResult::Replayed { state, receipt } => {
                    let snapshot = to_snapshot(&state, runtime.auto_speed())?;
                    return Ok(to_advance_response(receipt, snapshot));
                }
                DailyCommandAdvanceResult::Rejected(rejection) => {
                    if let Some(state) = paused_state.take() {
                        self.broadcast(&state, &runtime)?;
                    }
                    return Err(rejection.into());
                }
            }
        }

        Err(GameLoopError::Internal(anyhow::anyhow!(
            "manual advance exhausted its requested steps without a receipt"
        )))
    }

    pub async fn set_clock(
        self: &Arc<Self>,
        user_id: u64,
        speed: Option<AutoSpeed>,
    ) -> Result<GameSnapshot, GameLoopError> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;

        if speed.is_none() {
            self.pause_and_broadcast(&runtime)?;
            let state = self.games.load(user_id).await?;
            return Ok(to_snapshot(&state, None)?);
        }

        let state = self.games.load(user_id).await?;
        if let Some(speed) = speed {
            if state.save.character.is_none() {
                return Err(GameLoopError::CharacterRequired);
            }
            runtime.start(speed, &state)?;
        }

        Ok(self.broadcast(&state, &runtime)?)
    }

    /// Commits a character, increments the run generation and resets the game to day 0.
    pub async fn start_game(
        self: &Arc<Self>,
        user_id: u64,
        command: &StartGameCommand,
    ) -> Result<CharacterStartResponse, GameLoopError> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let paused_state = runtime.pause();
        let outcome = match self.games.start_game(user_id, command).await {
            Ok(outcome) => outcome,
            Err(error) => {
                if let Some(state) = paused_state {
                    self.broadcast(&state, &runtime)?;
                }
                return Err(error.into());
            }
        };
        match outcome {
            DailyStartGameResult::Applied { state, receipt } => {
                let snapshot = self.broadcast(&state, &runtime)?;
                Ok(to_character_start_response(receipt, snapshot))
            }
            DailyStartGameResult::Replayed { state, receipt } => {
                let snapshot = to_snapshot(&state, runtime.auto_speed())?;
                Ok(to_character_start_response(receipt, snapshot))
            }
            DailyStartGameResult::Rejected(rejection) => {
                if let Some(state) = paused_state {
                    self.broadcast(&state, &runtime)?;
                }
                Err(rejection.into())
            }
        }
    }

    /// Executes one idempotent order while sharing the save's runtime mutation lock.
    pub async fn place_order(
        self: &Arc<Self>,
        user_id: u64,
        order: &TradeOrder,
    ) -> Result<PlaceOrderResult> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;

        let (execution, save) = match self.trades.execute(user_id, order).await? {
            TradeStoreResult::Executed { execution, save } => (execution, save),
            TradeStoreResult::Rejected(failure) => {
                return Ok(PlaceOrderResult::Rejected(failure));
            }
        };

        let state = self.games.load(user_id).await?;
        if state.save.run_revision < save.run_revision
            || (state.save.run_revision == save.run_revision
                && state.save.state_revision < save.state_revision)
        {
            bail!("reloaded save is older than the committed trade");
        }

        // A replay may be recovering from a response assembly failure after the original
        // database commit. Re-publishing is safe because equal revisions are idempotent.
        let snapshot = self.broadcast(&state, &runtime)?;

        Ok(PlaceOrderResult::Executed(Box::new(
            PortfolioOrderResponse {
                execution,
                snapshot,
            },
        )))
    }

    /// Reads the current run's policy and account balances under the save operation lock.
    pub async fn finance_accounts(
        self: &Arc<Self>,
        user_id: u64,
    ) -> Result<FinanceAccountsResponse> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let state = self.games.load(user_id).await?;

        Ok(FinanceAccountsResponse {
            policy_set: PolicySetSnapshot {
                key: state.save.policy_set.key.clone(),
                basis_date: state.save.policy_set.basis_date.clone(),
            },
            accounts: state
                .save
                .accounts
                .iter()
                .map(to_financial_account_snapshot)
                .collect(),
        })
    }

    /// Lists the immutable M2-B catalog. Authentication is enforced by the route.
    pub async fn cash_product_catalog(&self) -> Result<CashProductCatalogResponse> {
        self.cash_products
            .cash_product_catalog()
            .await
            .map(to_cash_product_catalog_response)
    }

    pub async fn open_cma_account(
        self: &Arc<Self>,
        user_id: u64,
        command: &OpenCmaAccountCommand,
    ) -> Result<CashProductCommandResult<CmaAccountOpenResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self
            .cash_products
            .open_cma_account(user_id, command)
            .await?
        {
            CashProductStoreResult::Applied { receipt, save } => (receipt, save),
            CashProductStoreResult::Rejected(code) => {
                return Ok(CashProductCommandResult::Rejected(code));
            }
        };
        let snapshot = self
            .reload_and_broadcast_finance(user_id, &runtime, &committed)
            .await?;

        Ok(CashProductCommandResult::Applied(Box::new(
            CmaAccountOpenResponse {
                account: to_cma_account_open_snapshot(receipt),
                snapshot,
            },
        )))
    }

    pub async fn close_cma_account(
        self: &Arc<Self>,
        user_id: u64,
        command: &CloseCmaAccountCommand,
    ) -> Result<CashProductCommandResult<CmaAccountCloseResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self
            .cash_products
            .close_cma_account(user_id, command)
            .await?
        {
            CashProductStoreResult::Applied { receipt, save } => (receipt, save),
            CashProductStoreResult::Rejected(code) => {
                return Ok(CashProductCommandResult::Rejected(code));
            }
        };
        let snapshot = self
            .reload_and_broadcast_finance(user_id, &runtime, &committed)
            .await?;

        Ok(CashProductCommandResult::Applied(Box::new(
            CmaAccountCloseResponse {
                account_close: to_cma_account_close_snapshot(receipt),
                snapshot,
            },
        )))
    }

    pub async fn open_deposit(
        self: &Arc<Self>,
        user_id: u64,
        command: &OpenCashProductCommand,
    ) -> Result<CashProductCommandResult<DepositOpenResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self
            .cash_products
            .open_cash_product(user_id, command)
            .await?
        {
            CashProductStoreResult::Applied { receipt, save } => (receipt, save),
            CashProductStoreResult::Rejected(code) => {
                return Ok(CashProductCommandResult::Rejected(code));
            }
        };
        let snapshot = self
            .reload_and_broadcast_finance(user_id, &runtime, &committed)
            .await?;

        Ok(CashProductCommandResult::Applied(Box::new(
            DepositOpenResponse {
                deposit: to_deposit_open_snapshot(receipt)?,
                snapshot,
            },
        )))
    }

    pub async fn close_deposit(
        self: &Arc<Self>,
        user_id: u64,
        command: &CloseCashProductCommand,
    ) -> Result<CashProductCommandResult<DepositCloseResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self
            .cash_products
            .close_cash_product(user_id, command)
            .await?
        {
            CashProductStoreResult::Applied { receipt, save } => (receipt, save),
            CashProductStoreResult::Rejected(code) => {
                return Ok(CashProductCommandResult::Rejected(code));
            }
        };
        let snapshot = self
            .reload_and_broadcast_finance(user_id, &runtime, &committed)
            .await?;

        Ok(CashProductCommandResult::Applied(Box::new(
            DepositCloseResponse {
                deposit_close: to_deposit_close_snapshot(receipt),
                snapshot,
            },
        )))
    }

    pub async fn open_tax_account(
        self: &Arc<Self>,
        user_id: u64,
        command: &OpenTaxAccountCommand,
    ) -> Result<TaxAccountCommandResult<TaxAccountOpenResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) =
            match self.tax_accounts.open_tax_account(user_id, command).await? {
                TaxAccountStoreResult::Applied { receipt, save } => (receipt, save),
                TaxAccountStoreResult::Rejected(code) => {
                    return Ok(TaxAccountCommandResult::Rejected(code));
                }
            };
        let snapshot = self
            .reload_and_broadcast_finance(user_id, &runtime, &committed)
            .await?;

        Ok(TaxAccountCommandResult::Applied(Box::new(
            TaxAccountOpenResponse {
                account: to_tax_account_open_snapshot(receipt),
                snapshot,
            },
        )))
    }

    pub async fn close_isa_account(
        self: &Arc<Self>,
        user_id: u64,
        command: &CloseIsaAccountCommand,
    ) -> Result<TaxAccountCommandResult<IsaCloseResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self
            .tax_accounts
            .close_isa_account(user_id, command)
            .await?
        {
            TaxAccountStoreResult::Applied { receipt, save } => (receipt, save),
            TaxAccountStoreResult::Rejected(code) => {
                return Ok(TaxAccountCommandResult::Rejected(code));
            }
        };
        let snapshot = self
            .reload_and_broadcast_finance(user_id, &runtime, &committed)
            .await?;

        Ok(TaxAccountCommandResult::Applied(Box::new(
            IsaCloseResponse {
                isa_close: to_isa_close_snapshot(receipt),
                snapshot,
            },
        )))
    }

    pub async fn start_pension(
        self: &Arc<Self>,
        user_id: u64,
        command: &StartPensionCommand,
    ) -> Result<TaxAccountCommandResult<PensionStartResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.tax_accounts.start_pension(user_id, command).await? {
            TaxAccountStoreResult::Applied { receipt, save } => (receipt, save),
            TaxAccountStoreResult::Rejected(code) => {
                return Ok(TaxAccountCommandResult::Rejected(code));
            }
        };
        let snapshot = self
            .reload_and_broadcast_finance(user_id, &runtime, &committed)
            .await?;

        Ok(TaxAccountCommandResult::Applied(Box::new(
            PensionStartResponse {
                pension_start: to_pension_start_snapshot(receipt),
                snapshot,
            },
        )))
    }

    pub async fn withdraw_pension(
        self: &Arc<Self>,
        user_id: u64,
        command: &PensionWithdrawalCommand,
    ) -> Result<TaxAccountCommandResult<PensionWithdrawalResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) =
            match self.tax_accounts.withdraw_pension(user_id, command).await? {
                TaxAccountStoreResult::Applied { receipt, save } => (receipt, save),
                TaxAccountStoreResult::Rejected(code) => {
                    return Ok(TaxAccountCommandResult::Rejected(code));
                }
            };
        let snapshot = self
            .reload_and_broadcast_finance(user_id, &runtime, &committed)
            .await?;

        Ok(TaxAccountCommandResult::Applied(Box::new(
            PensionWithdrawalResponse {
                pension_withdrawal: to_pension_withdrawal_snapshot(receipt),
                snapshot,
            },
        )))
    }

    pub async fn bond_catalog(self: &Arc<Self>, user_id: u64) -> Result<BondCatalog> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games.load(user_id).await?;
        self.assets.bond_catalog(user_id).await
    }

    pub async fn place_bond_order(
        self: &Arc<Self>,
        user_id: u64,
        command: &BondOrderCommand,
    ) -> Result<AssetCommandResult<BondOrderResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let receipt = match self.assets.place_bond_order(user_id, command).await? {
            M2dAssetCommandResult::Applied(response) => response.bond_order,
            M2dAssetCommandResult::Rejected(code) => {
                return Ok(AssetCommandResult::Rejected(code));
            }
        };
        let snapshot = self.reload_and_broadcast_asset(user_id, &runtime).await?;
        Ok(AssetCommandResult::Applied(Box::new(BondOrderResponse {
            bond_order: receipt,
            snapshot,
        })))
    }

    pub async fn gold_catalog(self: &Arc<Self>, user_id: u64) -> Result<GoldCatalog> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games.load(user_id).await?;
        self.assets.gold_catalog(user_id).await
    }

    pub async fn open_gold_account(
        self: &Arc<Self>,
        user_id: u64,
        command: &OpenGoldAccountCommand,
    ) -> Result<AssetCommandResult<GoldAccountOpenResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let receipt = match self.assets.open_gold_account(user_id, command).await? {
            M2dAssetCommandResult::Applied(response) => response.account,
            M2dAssetCommandResult::Rejected(code) => {
                return Ok(AssetCommandResult::Rejected(code));
            }
        };
        let snapshot = self.reload_and_broadcast_asset(user_id, &runtime).await?;
        Ok(AssetCommandResult::Applied(Box::new(
            GoldAccountOpenResponse {
                account: receipt,
                snapshot,
            },
        )))
    }

    pub async fn place_gold_order(
        self: &Arc<Self>,
        user_id: u64,
        command: &GoldOrderCommand,
    ) -> Result<AssetCommandResult<GoldOrderResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let receipt = match self.assets.place_gold_order(user_id, command).await? {
            M2dAssetCommandResult::Applied(response) => response.gold_order,
            M2dAssetCommandResult::Rejected(code) => {
                return Ok(AssetCommandResult::Rejected(code));
            }
        };
        let snapshot = self.reload_and_broadcast_asset(user_id, &runtime).await?;
        Ok(AssetCommandResult::Applied(Box::new(GoldOrderResponse {
            gold_order: receipt,
            snapshot,
        })))
    }

    pub async fn withdraw_gold(
        self: &Arc<Self>,
        user_id: u64,
        command: &GoldWithdrawalCommand,
    ) -> Result<AssetCommandResult<GoldWithdrawalResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let receipt = match self.assets.withdraw_gold(user_id, command).await? {
            M2dAssetCommandResult::Applied(response) => response.gold_withdrawal,
            M2dAssetCommandResult::Rejected(code) => {
                return Ok(AssetCommandResult::Rejected(code));
            }
        };
        let snapshot = self.reload_and_broadcast_asset(user_id, &runtime).await?;
        Ok(AssetCommandResult::Applied(Box::new(
            GoldWithdrawalResponse {
                gold_withdrawal: receipt,
                snapshot,
            },
        )))
    }

    pub async fn finance_tax_year(
        self: &Arc<Self>,
        user_id: u64,
        tax_year: u16,
    ) -> Result<FinancialIncomeYearSnapshot> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games.load(user_id).await?;
        let income = self
            .cash_products
            .financial_income_year(user_id, tax_year)
            .await?;

        Ok(to_financial_income_year_snapshot(&income))
    }

    /// Moves cash atomically and broadcasts the committed or replayed snapshot.
    pub async fn transfer_finance(
        self: &Arc<Self>,
        user_id: u64,
        command: &TransferCommand,
    ) -> Result<FinanceCommandResult> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;

        let receipt = match self.finances.transfer(user_id, command).await? {
            FinanceStoreResult::Transferred(receipt) => receipt,
            FinanceStoreResult::Rejected(code) => {
                return Ok(FinanceCommandResult::Rejected(code));
            }
        };
        let state = self.games.load(user_id).await?;
        if state.save.run_revision < receipt.run_revision
            || (state.save.run_revision == receipt.run_revision
                && state.save.state_revision < receipt.state_revision)
        {
            bail!("reloaded save is older than the committed finance command");
        }
        let snapshot = self.broadcast(&state, &runtime)?;

        Ok(FinanceCommandResult::Transferred(Box::new(
            FinanceTransferResponse {
                transfer: FinanceTransferSnapshot {
                    command_id: receipt.command_id.to_string(),
                    account_id: receipt.account_id,
                    direction: receipt.direction,
                    amount_krw: receipt.amount_krw,
                    replayed: receipt.replayed,
                },
                snapshot,
            },
        )))
    }

    /// Reads one bounded page from the current run's append-only ledger.
    pub async fn finance_ledger(
        self: &Arc<Self>,
        user_id: u64,
        before: Option<u64>,
        limit: u32,
    ) -> Result<LedgerPageResponse> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games.load(user_id).await?;
        let page = self.finances.ledger_page(user_id, before, limit).await?;

        Ok(to_ledger_page_response(page))
    }

    pub async fn career_specs(
        self: &Arc<Self>,
        user_id: u64,
        query: CareerPageQuery,
    ) -> Result<CareerSpecsResponse> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games.load(user_id).await?;
        self.careers
            .specs(user_id, query)
            .await
            .map(to_career_specs_response)
    }

    pub async fn career_activities(
        self: &Arc<Self>,
        user_id: u64,
        query: CareerPageQuery,
    ) -> Result<CareerActivitiesResponse> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games.load(user_id).await?;
        self.careers
            .activities(user_id, query)
            .await
            .map(to_career_activities_response)
    }

    pub async fn career_artifacts(
        self: &Arc<Self>,
        user_id: u64,
        query: CareerArtifactPageQuery,
    ) -> Result<CareerArtifactsResponse> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games.load(user_id).await?;
        self.careers
            .artifacts(user_id, query)
            .await
            .and_then(to_career_artifacts_response)
    }

    pub async fn career_jobs(
        self: &Arc<Self>,
        user_id: u64,
        query: CareerJobsPageQuery,
    ) -> Result<CareerJobsResponse> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games.load(user_id).await?;
        self.careers
            .jobs(user_id, query)
            .await
            .map(to_career_jobs_response)
    }

    pub async fn career_applications(
        self: &Arc<Self>,
        user_id: u64,
        query: CareerPageQuery,
    ) -> Result<CareerApplicationsResponse> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games.load(user_id).await?;
        self.careers
            .applications(user_id, query)
            .await
            .map(to_career_applications_response)
    }

    pub async fn career_employment(
        self: &Arc<Self>,
        user_id: u64,
    ) -> Result<CareerEmploymentResponse> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games.load(user_id).await?;
        self.careers
            .employment(user_id)
            .await
            .map(to_career_employment_response)
    }

    pub async fn career_payroll(
        self: &Arc<Self>,
        user_id: u64,
        query: CareerPageQuery,
    ) -> Result<CareerPayrollResponse> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games.load(user_id).await?;
        self.careers
            .payroll(user_id, query)
            .await
            .map(to_career_payroll_response)
    }

    pub async fn career_employment_tax_year(
        self: &Arc<Self>,
        user_id: u64,
        tax_year: u16,
    ) -> Result<CareerEmploymentTaxYearSnapshot> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games.load(user_id).await?;
        let tax_year = self.careers.employment_tax_year(user_id, tax_year).await?;

        Ok(to_career_employment_tax_year_snapshot(&tax_year))
    }

    pub async fn military_options(
        self: &Arc<Self>,
        user_id: u64,
    ) -> Result<MilitaryOptionsResponse> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games.load(user_id).await?;
        self.careers
            .military_options(user_id)
            .await
            .map(to_military_options_response)
    }

    pub async fn military_service(
        self: &Arc<Self>,
        user_id: u64,
    ) -> Result<MilitaryServiceResponse> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games.load(user_id).await?;
        self.careers
            .military_service(user_id)
            .await
            .map(to_military_service_response)
    }

    pub async fn military_savings_products(
        self: &Arc<Self>,
        user_id: u64,
    ) -> Result<MilitarySavingsProductsResponse> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games.load(user_id).await?;
        self.careers
            .military_savings_products(user_id)
            .await
            .map(to_military_savings_products_response)
    }

    pub async fn military_savings(
        self: &Arc<Self>,
        user_id: u64,
        query: CareerPageQuery,
    ) -> Result<MilitarySavingsHistoryResponse> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games.load(user_id).await?;
        self.careers
            .military_savings(user_id, query)
            .await
            .map(to_military_savings_history_response)
    }

    pub async fn life_events(
        self: &Arc<Self>,
        user_id: u64,
        query: LifeEventsQueryState,
    ) -> Result<LifeCommandResult<LifeEventsResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        match self.lives.life_events(user_id, query).await? {
            LifeEventsReadResult::Found(state) => Ok(LifeCommandResult::Applied(Box::new(
                to_life_events_response(state)?,
            ))),
            LifeEventsReadResult::Rejected(code) => Ok(LifeCommandResult::Rejected(code)),
        }
    }

    pub async fn resolve_life_event(
        self: &Arc<Self>,
        user_id: u64,
        command: &ResolveLifeEventCommand,
    ) -> Result<LifeCommandResult<LifeEventChoiceResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.lives.resolve_life_event(user_id, command).await? {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = if receipt.replayed {
            self.reload_life_without_broadcast(user_id, &runtime, &committed)
                .await?
        } else {
            self.reload_and_broadcast_life(user_id, &runtime, &committed)
                .await?
        };
        Ok(LifeCommandResult::Applied(Box::new(
            to_life_event_choice_response(receipt, snapshot)?,
        )))
    }

    pub async fn insolvency(
        self: &Arc<Self>,
        user_id: u64,
    ) -> Result<LifeCommandResult<InsolvencyOverviewResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        match self.lives.insolvency_overview(user_id).await? {
            InsolvencyReadResult::Found(state) => Ok(LifeCommandResult::Applied(Box::new(
                to_insolvency_snapshot(&state)?,
            ))),
            InsolvencyReadResult::Rejected(code) => Ok(LifeCommandResult::Rejected(code)),
        }
    }

    pub async fn corporation_templates(
        self: &Arc<Self>,
        user_id: u64,
    ) -> Result<LifeCommandResult<CorporationTemplatesResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        match self.lives.corporation_templates(user_id).await? {
            CorporationReadResult::Found(state) => Ok(LifeCommandResult::Applied(Box::new(
                to_corporation_templates_response(state)?,
            ))),
            CorporationReadResult::Rejected(code) => Ok(LifeCommandResult::Rejected(code)),
        }
    }

    pub async fn create_corporation(
        self: &Arc<Self>,
        user_id: u64,
        command: &CreateCorporationCommand,
    ) -> Result<LifeCommandResult<CorporationCreateResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.lives.create_corporation(user_id, command).await? {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = if receipt.replayed {
            self.reload_life_without_broadcast(user_id, &runtime, &committed)
                .await?
        } else {
            self.reload_and_broadcast_life(user_id, &runtime, &committed)
                .await?
        };
        Ok(LifeCommandResult::Applied(Box::new(
            to_corporation_create_response(receipt, snapshot)?,
        )))
    }

    pub async fn corporation_detail(
        self: &Arc<Self>,
        user_id: u64,
        corporation_id: ResourceId,
    ) -> Result<LifeCommandResult<CorporationDetailResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        match self
            .lives
            .corporation_detail(user_id, corporation_id)
            .await?
        {
            CorporationReadResult::Found(state) => Ok(LifeCommandResult::Applied(Box::new(
                to_corporation_summary_snapshot(&state)?,
            ))),
            CorporationReadResult::Rejected(code) => Ok(LifeCommandResult::Rejected(code)),
        }
    }

    pub async fn update_corporation_settings(
        self: &Arc<Self>,
        user_id: u64,
        command: &UpdateCorporationSettingsCommand,
    ) -> Result<LifeCommandResult<CorporationSettingsResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self
            .lives
            .update_corporation_settings(user_id, command)
            .await?
        {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = if receipt.replayed {
            self.reload_life_without_broadcast(user_id, &runtime, &committed)
                .await?
        } else {
            self.reload_and_broadcast_life(user_id, &runtime, &committed)
                .await?
        };
        Ok(LifeCommandResult::Applied(Box::new(
            to_corporation_settings_response(receipt, snapshot)?,
        )))
    }

    pub async fn pay_corporation_dividend(
        self: &Arc<Self>,
        user_id: u64,
        command: &PayCorporationDividendCommand,
    ) -> Result<LifeCommandResult<CorporationDividendResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self
            .lives
            .pay_corporation_dividend(user_id, command)
            .await?
        {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = if receipt.replayed {
            self.reload_life_without_broadcast(user_id, &runtime, &committed)
                .await?
        } else {
            self.reload_and_broadcast_life(user_id, &runtime, &committed)
                .await?
        };
        Ok(LifeCommandResult::Applied(Box::new(
            to_corporation_dividend_response(receipt, snapshot)?,
        )))
    }

    pub async fn corporation_operating_months(
        self: &Arc<Self>,
        user_id: u64,
        corporation_id: ResourceId,
        cursor: Option<String>,
    ) -> Result<LifeCommandResult<CorporationOperatingMonthPageResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        match self
            .lives
            .corporation_operating_months(user_id, corporation_id, cursor)
            .await?
        {
            CorporationReadResult::Found(state) => Ok(LifeCommandResult::Applied(Box::new(
                to_corporation_operating_month_page_response(state)?,
            ))),
            CorporationReadResult::Rejected(code) => Ok(LifeCommandResult::Rejected(code)),
        }
    }

    pub async fn prepare_insolvency_case(
        self: &Arc<Self>,
        user_id: u64,
        command: &PrepareInsolvencyCaseCommand,
    ) -> Result<LifeCommandResult<InsolvencyCaseCommandResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) =
            match self.lives.prepare_insolvency_case(user_id, command).await? {
                LifeStoreResult::Applied { receipt, save } => (receipt, save),
                LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
            };
        let snapshot = if receipt.replayed {
            self.reload_life_without_broadcast(user_id, &runtime, &committed)
                .await?
        } else {
            self.reload_and_broadcast_life(user_id, &runtime, &committed)
                .await?
        };
        Ok(LifeCommandResult::Applied(Box::new(
            to_insolvency_case_command_response(receipt, snapshot)?,
        )))
    }

    pub async fn act_on_insolvency_case(
        self: &Arc<Self>,
        user_id: u64,
        command: &ActOnInsolvencyCaseCommand,
    ) -> Result<LifeCommandResult<InsolvencyCaseCommandResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.lives.act_on_insolvency_case(user_id, command).await?
        {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = if receipt.replayed {
            self.reload_life_without_broadcast(user_id, &runtime, &committed)
                .await?
        } else {
            self.reload_and_broadcast_life(user_id, &runtime, &committed)
                .await?
        };
        Ok(LifeCommandResult::Applied(Box::new(
            to_insolvency_case_command_response(receipt, snapshot)?,
        )))
    }

    pub async fn insolvency_case_detail(
        self: &Arc<Self>,
        user_id: u64,
        case_id: ResourceId,
    ) -> Result<LifeCommandResult<InsolvencyCaseDetailResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        match self.lives.insolvency_case_detail(user_id, case_id).await? {
            InsolvencyReadResult::Found(state) => Ok(LifeCommandResult::Applied(Box::new(
                to_insolvency_case_detail_response(state)?,
            ))),
            InsolvencyReadResult::Rejected(code) => Ok(LifeCommandResult::Rejected(code)),
        }
    }

    pub async fn insolvency_claims(
        self: &Arc<Self>,
        user_id: u64,
        case_id: ResourceId,
        cursor: Option<String>,
    ) -> Result<LifeCommandResult<InsolvencyClaimPageResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        match self
            .lives
            .insolvency_claims(user_id, case_id, cursor)
            .await?
        {
            InsolvencyReadResult::Found(state) => Ok(LifeCommandResult::Applied(Box::new(
                to_insolvency_claim_page_response(state)?,
            ))),
            InsolvencyReadResult::Rejected(code) => Ok(LifeCommandResult::Rejected(code)),
        }
    }

    pub async fn insolvency_liquidations(
        self: &Arc<Self>,
        user_id: u64,
        case_id: ResourceId,
        cursor: Option<String>,
    ) -> Result<LifeCommandResult<InsolvencyLiquidationPageResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        match self
            .lives
            .insolvency_liquidations(user_id, case_id, cursor)
            .await?
        {
            InsolvencyReadResult::Found(state) => Ok(LifeCommandResult::Applied(Box::new(
                to_insolvency_liquidation_page_response(state)?,
            ))),
            InsolvencyReadResult::Rejected(code) => Ok(LifeCommandResult::Rejected(code)),
        }
    }

    pub async fn insurance(
        self: &Arc<Self>,
        user_id: u64,
        query: InsuranceQueryState,
    ) -> Result<LifeCommandResult<InsuranceContractsResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        match self.lives.insurance(user_id, query).await? {
            InsuranceReadResult::Found(state) => Ok(LifeCommandResult::Applied(Box::new(
                to_insurance_contracts_response(state)?,
            ))),
            InsuranceReadResult::Rejected(code) => Ok(LifeCommandResult::Rejected(code)),
        }
    }

    pub async fn enroll_insurance_contract(
        self: &Arc<Self>,
        user_id: u64,
        command: &EnrollInsuranceContractCommand,
    ) -> Result<LifeCommandResult<InsuranceEnrollmentResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self
            .lives
            .enroll_insurance_contract(user_id, command)
            .await?
        {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = if receipt.replayed {
            self.reload_life_without_broadcast(user_id, &runtime, &committed)
                .await?
        } else {
            self.reload_and_broadcast_life(user_id, &runtime, &committed)
                .await?
        };
        Ok(LifeCommandResult::Applied(Box::new(
            to_insurance_enrollment_response(receipt, snapshot)?,
        )))
    }

    pub async fn cancel_insurance_contract(
        self: &Arc<Self>,
        user_id: u64,
        command: &CancelInsuranceContractCommand,
    ) -> Result<LifeCommandResult<InsuranceCancellationResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self
            .lives
            .cancel_insurance_contract(user_id, command)
            .await?
        {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = if receipt.replayed {
            self.reload_life_without_broadcast(user_id, &runtime, &committed)
                .await?
        } else {
            self.reload_and_broadcast_life(user_id, &runtime, &committed)
                .await?
        };
        Ok(LifeCommandResult::Applied(Box::new(
            to_insurance_cancellation_response(receipt, snapshot)?,
        )))
    }

    pub async fn file_insurance_claim(
        self: &Arc<Self>,
        user_id: u64,
        command: &FileInsuranceClaimCommand,
    ) -> Result<LifeCommandResult<InsuranceClaimResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.lives.file_insurance_claim(user_id, command).await? {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = if receipt.replayed {
            self.reload_life_without_broadcast(user_id, &runtime, &committed)
                .await?
        } else {
            self.reload_and_broadcast_life(user_id, &runtime, &committed)
                .await?
        };
        Ok(LifeCommandResult::Applied(Box::new(
            to_insurance_claim_response(receipt, snapshot)?,
        )))
    }

    pub async fn welfare_programs(
        self: &Arc<Self>,
        user_id: u64,
    ) -> Result<Option<WelfareProgramsResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.lives
            .welfare_programs(user_id)
            .await?
            .map(to_welfare_programs_response)
            .transpose()
    }

    pub async fn apply_welfare_program(
        self: &Arc<Self>,
        user_id: u64,
        command: &ApplyWelfareProgramCommand,
    ) -> Result<LifeCommandResult<WelfareApplicationResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.lives.apply_welfare_program(user_id, command).await? {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = if receipt.replayed {
            self.reload_life_without_broadcast(user_id, &runtime, &committed)
                .await?
        } else {
            self.reload_and_broadcast_life(user_id, &runtime, &committed)
                .await?
        };

        Ok(LifeCommandResult::Applied(Box::new(
            to_welfare_application_response(receipt, snapshot)?,
        )))
    }

    pub async fn housing_listings(
        self: &Arc<Self>,
        user_id: u64,
        query: HousingListingsQueryState,
    ) -> Result<Option<HousingListingsResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.lives
            .housing_listings(user_id, query)
            .await
            .map(|state| state.map(to_housing_listings_response))
    }

    pub async fn housing_lease_current(
        self: &Arc<Self>,
        user_id: u64,
    ) -> Result<Option<HousingLeaseCurrentResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.lives
            .housing_lease_current(user_id)
            .await
            .map(|state| state.map(to_housing_lease_current_response))
    }

    pub async fn housing_property_holdings(
        self: &Arc<Self>,
        user_id: u64,
    ) -> Result<Option<HousingPropertyHoldingsResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.lives
            .housing_property_holdings(user_id)
            .await?
            .map(to_housing_property_holdings_response)
            .transpose()
    }

    pub async fn start_housing_lease(
        self: &Arc<Self>,
        user_id: u64,
        command: &StartHousingLeaseCommand,
    ) -> Result<LifeCommandResult<HousingLeaseMoveResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.lives.start_housing_lease(user_id, command).await? {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = if receipt.replayed {
            self.reload_life_without_broadcast(user_id, &runtime, &committed)
                .await?
        } else {
            self.reload_and_broadcast_life(user_id, &runtime, &committed)
                .await?
        };

        Ok(LifeCommandResult::Applied(Box::new(
            to_housing_lease_move_response(receipt, snapshot),
        )))
    }

    pub async fn life_budget(self: &Arc<Self>, user_id: u64) -> Result<LifeBudgetResponse> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games.load(user_id).await?;
        self.lives
            .budget(user_id)
            .await
            .map(to_life_budget_response)
    }

    pub async fn loan_products(
        self: &Arc<Self>,
        user_id: u64,
    ) -> Result<LoanProductCatalogResponse> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games.load(user_id).await?;
        self.lives
            .loan_products(user_id)
            .await
            .map(to_loan_product_catalog_response)
    }

    pub async fn loan_detail(
        self: &Arc<Self>,
        user_id: u64,
        loan_id: ResourceId,
    ) -> Result<Option<LoanDetailResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games.load(user_id).await?;
        self.lives
            .loan_detail(user_id, loan_id)
            .await
            .map(|state| state.map(to_loan_detail_response))
    }

    pub async fn loan_installments(
        self: &Arc<Self>,
        user_id: u64,
        loan_id: ResourceId,
        query: LoanInstallmentPageQuery,
    ) -> Result<Option<LoanInstallmentsResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games.load(user_id).await?;
        self.lives
            .loan_installments(user_id, loan_id, query)
            .await
            .map(|state| state.map(to_loan_installments_response))
    }

    pub async fn credit(self: &Arc<Self>, user_id: u64) -> Result<CreditResponse> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games.load(user_id).await?;
        self.lives.credit(user_id).await.map(to_credit_response)
    }

    pub async fn quote_loan(
        self: &Arc<Self>,
        user_id: u64,
        command: &CreateLoanQuoteCommand,
    ) -> Result<LifeCommandResult<LoanQuoteResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.lives.quote_loan(user_id, command).await? {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = self
            .reload_life_without_broadcast(user_id, &runtime, &committed)
            .await?;

        Ok(LifeCommandResult::Applied(Box::new(
            to_loan_quote_response(receipt, snapshot),
        )))
    }

    pub async fn quote_lease_deposit_loan(
        self: &Arc<Self>,
        user_id: u64,
        command: &CreateLeaseDepositLoanQuoteCommand,
    ) -> Result<LifeCommandResult<LeaseDepositLoanQuoteResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self
            .lives
            .quote_lease_deposit_loan(user_id, command)
            .await?
        {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = self
            .reload_life_without_broadcast(user_id, &runtime, &committed)
            .await?;

        Ok(LifeCommandResult::Applied(Box::new(
            to_lease_deposit_loan_quote_response(receipt, snapshot)?,
        )))
    }

    pub async fn quote_mortgage(
        self: &Arc<Self>,
        user_id: u64,
        command: &CreateMortgageQuoteCommand,
    ) -> Result<LifeCommandResult<MortgageQuoteResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.lives.quote_mortgage(user_id, command).await? {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = self
            .reload_life_without_broadcast(user_id, &runtime, &committed)
            .await?;

        Ok(LifeCommandResult::Applied(Box::new(
            to_mortgage_quote_response(receipt, snapshot)?,
        )))
    }

    pub async fn purchase_property(
        self: &Arc<Self>,
        user_id: u64,
        command: &PurchasePropertyCommand,
    ) -> Result<LifeCommandResult<PropertyPurchaseResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.lives.purchase_property(user_id, command).await? {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = if receipt.replayed {
            self.reload_life_without_broadcast(user_id, &runtime, &committed)
                .await?
        } else {
            self.reload_and_broadcast_life(user_id, &runtime, &committed)
                .await?
        };

        Ok(LifeCommandResult::Applied(Box::new(
            to_property_purchase_response(receipt, snapshot)?,
        )))
    }

    pub async fn create_property_sale_order(
        self: &Arc<Self>,
        user_id: u64,
        command: &CreatePropertySaleOrderCommand,
    ) -> Result<LifeCommandResult<PropertySaleOrderListingResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self
            .lives
            .create_property_sale_order(user_id, command)
            .await?
        {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = if receipt.replayed {
            self.reload_life_without_broadcast(user_id, &runtime, &committed)
                .await?
        } else {
            self.reload_and_broadcast_life(user_id, &runtime, &committed)
                .await?
        };

        Ok(LifeCommandResult::Applied(Box::new(
            to_property_sale_order_listing_response(receipt, snapshot)?,
        )))
    }

    pub async fn reprice_property_sale_order(
        self: &Arc<Self>,
        user_id: u64,
        command: &RepricePropertySaleOrderCommand,
    ) -> Result<LifeCommandResult<PropertySaleOrderListingResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self
            .lives
            .reprice_property_sale_order(user_id, command)
            .await?
        {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = if receipt.replayed {
            self.reload_life_without_broadcast(user_id, &runtime, &committed)
                .await?
        } else {
            self.reload_and_broadcast_life(user_id, &runtime, &committed)
                .await?
        };

        Ok(LifeCommandResult::Applied(Box::new(
            to_property_sale_order_listing_response(receipt, snapshot)?,
        )))
    }

    pub async fn cancel_property_sale_order(
        self: &Arc<Self>,
        user_id: u64,
        command: &CancelPropertySaleOrderCommand,
    ) -> Result<LifeCommandResult<PropertySaleOrderCancellationResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self
            .lives
            .cancel_property_sale_order(user_id, command)
            .await?
        {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = if receipt.replayed {
            self.reload_life_without_broadcast(user_id, &runtime, &committed)
                .await?
        } else {
            self.reload_and_broadcast_life(user_id, &runtime, &committed)
                .await?
        };

        Ok(LifeCommandResult::Applied(Box::new(
            to_property_sale_order_cancellation_response(receipt, snapshot)?,
        )))
    }

    pub async fn property_sale_orders(
        self: &Arc<Self>,
        user_id: u64,
        query: PropertySaleOrderPageQuery,
    ) -> Result<LifeCommandResult<PropertySaleOrdersResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        if self.games.load(user_id).await?.save.character.is_none() {
            return Ok(LifeCommandResult::Rejected(
                LifeFailureCode::CharacterRequired,
            ));
        }
        let state = self
            .lives
            .property_sale_orders(user_id, query)
            .await?
            .context("property sale order page is missing for an active run")?;
        Ok(LifeCommandResult::Applied(Box::new(
            to_property_sale_orders_response(state)?,
        )))
    }

    pub async fn property_tax_events(
        self: &Arc<Self>,
        user_id: u64,
        holding_id: ResourceId,
        query: PropertyTaxEventPageQuery,
    ) -> Result<LifeCommandResult<PropertyTaxEventsResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        if self.games.load(user_id).await?.save.character.is_none() {
            return Ok(LifeCommandResult::Rejected(
                LifeFailureCode::CharacterRequired,
            ));
        }
        let Some(state) = self
            .lives
            .property_tax_events(user_id, holding_id, query)
            .await?
        else {
            return Ok(LifeCommandResult::Rejected(
                LifeFailureCode::HousingResourceNotFound,
            ));
        };
        Ok(LifeCommandResult::Applied(Box::new(
            to_property_tax_events_response(state)?,
        )))
    }

    pub async fn execute_loan(
        self: &Arc<Self>,
        user_id: u64,
        command: &ExecuteLoanCommand,
    ) -> Result<LifeCommandResult<LoanExecutionResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.lives.execute_loan(user_id, command).await? {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = if receipt.replayed {
            self.reload_life_without_broadcast(user_id, &runtime, &committed)
                .await?
        } else {
            self.reload_and_broadcast_life(user_id, &runtime, &committed)
                .await?
        };

        Ok(LifeCommandResult::Applied(Box::new(
            to_loan_execution_response(receipt, snapshot),
        )))
    }

    pub async fn prepay_loan(
        self: &Arc<Self>,
        user_id: u64,
        command: &PrepayLoanCommand,
    ) -> Result<LifeCommandResult<LoanPrepaymentResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.lives.prepay_loan(user_id, command).await? {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = if receipt.replayed {
            self.reload_life_without_broadcast(user_id, &runtime, &committed)
                .await?
        } else {
            self.reload_and_broadcast_life(user_id, &runtime, &committed)
                .await?
        };

        Ok(LifeCommandResult::Applied(Box::new(
            to_loan_prepayment_response(receipt, snapshot),
        )))
    }

    pub async fn update_life_budget(
        self: &Arc<Self>,
        user_id: u64,
        command: &UpdateLifeBudgetCommand,
    ) -> Result<LifeCommandResult<LifeBudgetUpdateResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.lives.update_budget(user_id, command).await? {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = self
            .reload_and_broadcast_life(user_id, &runtime, &committed)
            .await?;

        Ok(LifeCommandResult::Applied(Box::new(
            to_life_budget_update_response(receipt, snapshot),
        )))
    }

    pub async fn pay_essential_arrear(
        self: &Arc<Self>,
        user_id: u64,
        command: &PayEssentialArrearCommand,
    ) -> Result<LifeCommandResult<EssentialArrearPaymentResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.lives.pay_essential_arrear(user_id, command).await? {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = self
            .reload_and_broadcast_life(user_id, &runtime, &committed)
            .await?;

        Ok(LifeCommandResult::Applied(Box::new(
            to_essential_arrear_payment_response(receipt, snapshot),
        )))
    }

    pub async fn pay_lease_arrear(
        self: &Arc<Self>,
        user_id: u64,
        command: &PayLeaseArrearCommand,
    ) -> Result<LifeCommandResult<LeaseArrearPaymentResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.lives.pay_lease_arrear(user_id, command).await? {
            LifeStoreResult::Applied { receipt, save } => (receipt, save),
            LifeStoreResult::Rejected(code) => return Ok(LifeCommandResult::Rejected(code)),
        };
        let snapshot = if receipt.replayed {
            self.reload_life_without_broadcast(user_id, &runtime, &committed)
                .await?
        } else {
            self.reload_and_broadcast_life(user_id, &runtime, &committed)
                .await?
        };

        Ok(LifeCommandResult::Applied(Box::new(
            to_lease_arrear_payment_response(receipt, snapshot),
        )))
    }

    pub async fn focus_career(
        self: &Arc<Self>,
        user_id: u64,
        command: &FocusCareerCommand,
    ) -> Result<CareerCommandResult<CareerFocusResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.careers.focus(user_id, command).await? {
            CareerStoreResult::Applied { receipt, save } => (receipt, save),
            CareerStoreResult::Rejected(code) => {
                return Ok(CareerCommandResult::Rejected(code));
            }
        };
        let snapshot = self
            .reload_and_broadcast_career(user_id, &runtime, &committed)
            .await?;
        Ok(CareerCommandResult::Applied(Box::new(
            CareerFocusResponse {
                result: CareerFocusResultSnapshot {
                    focused_job_family_key: receipt.focused_job_family_key,
                },
                replayed: receipt.replayed,
                snapshot,
            },
        )))
    }

    pub async fn start_career_activity(
        self: &Arc<Self>,
        user_id: u64,
        command: &StartCareerActivityCommand,
    ) -> Result<CareerCommandResult<CareerActivityResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.careers.start_activity(user_id, command).await? {
            CareerStoreResult::Applied { receipt, save } => (receipt, save),
            CareerStoreResult::Rejected(code) => {
                return Ok(CareerCommandResult::Rejected(code));
            }
        };
        let snapshot = self
            .reload_and_broadcast_career(user_id, &runtime, &committed)
            .await?;
        Ok(CareerCommandResult::Applied(Box::new(
            CareerActivityResponse {
                result: CareerActivityResultSnapshot {
                    activity_id: receipt.activity_id,
                    status: receipt.status,
                },
                replayed: receipt.replayed,
                snapshot,
            },
        )))
    }

    pub async fn cancel_career_activity(
        self: &Arc<Self>,
        user_id: u64,
        command: &CancelCareerActivityCommand,
    ) -> Result<CareerCommandResult<CareerActivityResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.careers.cancel_activity(user_id, command).await? {
            CareerStoreResult::Applied { receipt, save } => (receipt, save),
            CareerStoreResult::Rejected(code) => {
                return Ok(CareerCommandResult::Rejected(code));
            }
        };
        let snapshot = self
            .reload_and_broadcast_career(user_id, &runtime, &committed)
            .await?;
        Ok(CareerCommandResult::Applied(Box::new(
            CareerActivityResponse {
                result: CareerActivityResultSnapshot {
                    activity_id: receipt.activity_id,
                    status: receipt.status,
                },
                replayed: receipt.replayed,
                snapshot,
            },
        )))
    }

    pub async fn publish_career_artifact(
        self: &Arc<Self>,
        user_id: u64,
        command: &PublishCareerArtifactCommand,
    ) -> Result<CareerCommandResult<CareerArtifactResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.careers.publish_artifact(user_id, command).await? {
            CareerStoreResult::Applied { receipt, save } => (receipt, save),
            CareerStoreResult::Rejected(code) => {
                return Ok(CareerCommandResult::Rejected(code));
            }
        };
        let snapshot = self
            .reload_and_broadcast_career(user_id, &runtime, &committed)
            .await?;
        Ok(CareerCommandResult::Applied(Box::new(
            CareerArtifactResponse {
                result: CareerArtifactResultSnapshot {
                    artifact_version_id: receipt.artifact_version_id,
                    kind: receipt.kind,
                    version_no: receipt.version_no,
                },
                replayed: receipt.replayed,
                snapshot,
            },
        )))
    }

    pub async fn apply_career(
        self: &Arc<Self>,
        user_id: u64,
        command: &ApplyCareerCommand,
    ) -> Result<CareerCommandResult<CareerApplicationResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.careers.apply(user_id, command).await? {
            CareerStoreResult::Applied { receipt, save } => (receipt, save),
            CareerStoreResult::Rejected(code) => return Ok(CareerCommandResult::Rejected(code)),
        };
        let snapshot = self
            .reload_and_broadcast_career(user_id, &runtime, &committed)
            .await?;
        Ok(CareerCommandResult::Applied(Box::new(
            to_career_application_command_response(receipt, snapshot),
        )))
    }

    pub async fn confirm_career_interview(
        self: &Arc<Self>,
        user_id: u64,
        command: &ConfirmCareerInterviewCommand,
    ) -> Result<CareerCommandResult<CareerApplicationResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.careers.confirm_interview(user_id, command).await? {
            CareerStoreResult::Applied { receipt, save } => (receipt, save),
            CareerStoreResult::Rejected(code) => return Ok(CareerCommandResult::Rejected(code)),
        };
        let snapshot = self
            .reload_and_broadcast_career(user_id, &runtime, &committed)
            .await?;
        Ok(CareerCommandResult::Applied(Box::new(
            to_career_application_command_response(receipt, snapshot),
        )))
    }

    pub async fn withdraw_career_application(
        self: &Arc<Self>,
        user_id: u64,
        command: &WithdrawCareerApplicationCommand,
    ) -> Result<CareerCommandResult<CareerApplicationResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.careers.withdraw_application(user_id, command).await?
        {
            CareerStoreResult::Applied { receipt, save } => (receipt, save),
            CareerStoreResult::Rejected(code) => return Ok(CareerCommandResult::Rejected(code)),
        };
        let snapshot = self
            .reload_and_broadcast_career(user_id, &runtime, &committed)
            .await?;
        Ok(CareerCommandResult::Applied(Box::new(
            to_career_application_command_response(receipt, snapshot),
        )))
    }

    pub async fn accept_career_invitation(
        self: &Arc<Self>,
        user_id: u64,
        command: &AcceptCareerInvitationCommand,
    ) -> Result<CareerCommandResult<CareerInvitationResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.careers.accept_invitation(user_id, command).await? {
            CareerStoreResult::Applied { receipt, save } => (receipt, save),
            CareerStoreResult::Rejected(code) => return Ok(CareerCommandResult::Rejected(code)),
        };
        let snapshot = self
            .reload_and_broadcast_career(user_id, &runtime, &committed)
            .await?;
        Ok(CareerCommandResult::Applied(Box::new(
            to_career_invitation_command_response(receipt, snapshot),
        )))
    }

    pub async fn decline_career_invitation(
        self: &Arc<Self>,
        user_id: u64,
        command: &DeclineCareerInvitationCommand,
    ) -> Result<CareerCommandResult<CareerInvitationResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.careers.decline_invitation(user_id, command).await? {
            CareerStoreResult::Applied { receipt, save } => (receipt, save),
            CareerStoreResult::Rejected(code) => return Ok(CareerCommandResult::Rejected(code)),
        };
        let snapshot = self
            .reload_and_broadcast_career(user_id, &runtime, &committed)
            .await?;
        Ok(CareerCommandResult::Applied(Box::new(
            to_career_invitation_command_response(receipt, snapshot),
        )))
    }

    pub async fn accept_career_offer(
        self: &Arc<Self>,
        user_id: u64,
        command: &AcceptCareerOfferCommand,
    ) -> Result<CareerCommandResult<CareerOfferResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.careers.accept_offer(user_id, command).await? {
            CareerStoreResult::Applied { receipt, save } => (receipt, save),
            CareerStoreResult::Rejected(code) => return Ok(CareerCommandResult::Rejected(code)),
        };
        let snapshot = self
            .reload_and_broadcast_career(user_id, &runtime, &committed)
            .await?;
        Ok(CareerCommandResult::Applied(Box::new(
            to_career_offer_command_response(receipt, snapshot),
        )))
    }

    pub async fn decline_career_offer(
        self: &Arc<Self>,
        user_id: u64,
        command: &DeclineCareerOfferCommand,
    ) -> Result<CareerCommandResult<CareerOfferResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self.careers.decline_offer(user_id, command).await? {
            CareerStoreResult::Applied { receipt, save } => (receipt, save),
            CareerStoreResult::Rejected(code) => return Ok(CareerCommandResult::Rejected(code)),
        };
        let snapshot = self
            .reload_and_broadcast_career(user_id, &runtime, &committed)
            .await?;
        Ok(CareerCommandResult::Applied(Box::new(
            to_career_offer_command_response(receipt, snapshot),
        )))
    }

    pub async fn start_military_service(
        self: &Arc<Self>,
        user_id: u64,
        command: &StartMilitaryServiceCommand,
    ) -> Result<CareerCommandResult<MilitaryServiceCommandResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self
            .careers
            .start_military_service(user_id, command)
            .await?
        {
            CareerStoreResult::Applied { receipt, save } => (receipt, save),
            CareerStoreResult::Rejected(code) => return Ok(CareerCommandResult::Rejected(code)),
        };
        let snapshot = self
            .reload_and_broadcast_career(user_id, &runtime, &committed)
            .await?;

        Ok(CareerCommandResult::Applied(Box::new(
            to_military_service_command_response(receipt, snapshot)?,
        )))
    }

    pub async fn open_military_savings(
        self: &Arc<Self>,
        user_id: u64,
        command: &OpenMilitarySavingsCommand,
    ) -> Result<CareerCommandResult<MilitarySavingsCommandResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) =
            match self.careers.open_military_savings(user_id, command).await? {
                CareerStoreResult::Applied { receipt, save } => (receipt, save),
                CareerStoreResult::Rejected(code) => {
                    return Ok(CareerCommandResult::Rejected(code));
                }
            };
        let snapshot = self
            .reload_and_broadcast_career(user_id, &runtime, &committed)
            .await?;

        Ok(CareerCommandResult::Applied(Box::new(
            to_military_savings_command_response(receipt, snapshot),
        )))
    }

    pub async fn close_military_savings(
        self: &Arc<Self>,
        user_id: u64,
        command: &CloseMilitarySavingsCommand,
    ) -> Result<CareerCommandResult<MilitarySavingsCommandResponse>> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        let (receipt, committed) = match self
            .careers
            .close_military_savings(user_id, command)
            .await?
        {
            CareerStoreResult::Applied { receipt, save } => (receipt, save),
            CareerStoreResult::Rejected(code) => return Ok(CareerCommandResult::Rejected(code)),
        };
        let snapshot = self
            .reload_and_broadcast_career(user_id, &runtime, &committed)
            .await?;

        Ok(CareerCommandResult::Applied(Box::new(
            to_military_savings_command_response(receipt, snapshot),
        )))
    }

    async fn reload_and_broadcast_life(
        &self,
        user_id: u64,
        runtime: &SaveRuntime,
        committed: &crate::store::SaveState,
    ) -> Result<GameSnapshot> {
        let state = self.games.load(user_id).await?;
        if state.save.run_revision < committed.run_revision
            || (state.save.run_revision == committed.run_revision
                && state.save.state_revision < committed.state_revision)
        {
            bail!("reloaded save is older than the committed life command");
        }
        self.broadcast(&state, runtime)
    }

    async fn reload_life_without_broadcast(
        &self,
        user_id: u64,
        runtime: &SaveRuntime,
        committed: &crate::store::SaveState,
    ) -> Result<GameSnapshot> {
        let state = self.games.load(user_id).await?;
        if state.save.run_revision < committed.run_revision
            || (state.save.run_revision == committed.run_revision
                && state.save.state_revision < committed.state_revision)
        {
            bail!("reloaded save is older than the committed non-broadcast life command");
        }
        to_snapshot(&state, runtime.auto_speed())
    }

    async fn reload_and_broadcast_career(
        &self,
        user_id: u64,
        runtime: &SaveRuntime,
        committed: &crate::store::SaveState,
    ) -> Result<GameSnapshot> {
        let state = self.games.load(user_id).await?;
        if state.save.run_revision < committed.run_revision
            || (state.save.run_revision == committed.run_revision
                && state.save.state_revision < committed.state_revision)
        {
            bail!("reloaded save is older than the committed career command");
        }
        self.broadcast(&state, runtime)
    }

    async fn reload_and_broadcast_finance(
        &self,
        user_id: u64,
        runtime: &SaveRuntime,
        committed: &crate::store::SaveState,
    ) -> Result<GameSnapshot> {
        let state = self.games.load(user_id).await?;
        if state.save.run_revision < committed.run_revision
            || (state.save.run_revision == committed.run_revision
                && state.save.state_revision < committed.state_revision)
        {
            bail!("reloaded save is older than the committed cash-product command");
        }

        self.broadcast(&state, runtime)
    }

    async fn reload_and_broadcast_asset(
        &self,
        user_id: u64,
        runtime: &SaveRuntime,
    ) -> Result<GameSnapshot> {
        let state = self.games.load(user_id).await?;
        self.broadcast(&state, runtime)
    }

    /// Returns the authenticated save's recent LLX path, never shared future cache rows.
    pub async fn market_history(
        self: &Arc<Self>,
        user_id: u64,
        days: u32,
    ) -> Result<MarketHistoryResponse> {
        let runtime = self.runtime(user_id);
        let _operation = runtime.operation.lock().await;
        self.games
            .load(user_id)
            .await
            .context("failed to prepare the visible market history")?;
        let history = self.markets.history_for_user(user_id, days).await?;

        Ok(MarketHistoryResponse {
            world: history.world_key,
            symbol: "LLX",
            through_game_day: history.through_game_day,
            points: history
                .days
                .into_iter()
                .map(|day| MarketHistoryPoint {
                    game_day: day.game_day,
                    date: day.market_date.to_string(),
                    open: day.market_open,
                    close_krw: day.equity_close_krw,
                    daily_return_ppm: day.equity_return_ppm,
                    llx_close_krw: day.m2.as_ref().map(|m2| m2.llx_close_krw),
                    llx_daily_return_ppm: day.m2.as_ref().map(|m2| m2.llx_return_ppm),
                    regime: day.regime,
                    rates: day.rates.as_ref().map(to_market_rates_snapshot),
                })
                .collect(),
        })
    }

    async fn advance_one_day(
        &self,
        user_id: u64,
        runtime: &SaveRuntime,
    ) -> Result<GameSnapshot, GameLoopError> {
        match self.games.advance_one_day(user_id).await? {
            DailyAdvanceResult::Advanced(state) => Ok(self.broadcast(&state, runtime)?),
            DailyAdvanceResult::CharacterRequired => Err(GameLoopError::CharacterRequired),
        }
    }

    fn broadcast(&self, state: &CommittedGameState, runtime: &SaveRuntime) -> Result<GameSnapshot> {
        let auto_speed = runtime.record_committed(state);
        let snapshot = to_snapshot(state, auto_speed)?;
        // Sending with no subscribers errors, which is a normal state here.
        let _ = runtime.ticks.send(snapshot.clone());

        Ok(snapshot)
    }

    /// Commands call this while holding `operation`, before any fallible store work.
    /// A real running-to-paused transition is pushed once from the last committed state.
    fn pause_and_broadcast(&self, runtime: &SaveRuntime) -> Result<Option<GameSnapshot>> {
        runtime
            .pause()
            .map(|last_committed| self.broadcast(&last_committed, runtime))
            .transpose()
    }
}

async fn run_automatic_clock(
    user_id: u64,
    state: Weak<AppState>,
    runtime: Weak<SaveRuntime>,
    timer: Arc<dyn GameTimer>,
) {
    let mut changes = {
        let Some(runtime) = runtime.upgrade() else {
            return;
        };
        runtime.changes.subscribe()
    };

    loop {
        let signal = { *changes.borrow_and_update() };
        let Some(speed) = signal.speed else {
            if changes.changed().await.is_err() {
                return;
            }
            continue;
        };

        tokio::select! {
            changed = changes.changed() => {
                if changed.is_err() {
                    return;
                }
                continue;
            }
            () = timer.wait(speed.interval()) => {}
        }

        let Some(active_runtime) = runtime.upgrade() else {
            return;
        };
        let _operation = active_runtime.operation.lock().await;
        if !active_runtime.is_active(signal) {
            continue;
        }
        let Some(state) = state.upgrade() else {
            return;
        };

        if let Err(error) = state.advance_one_day(user_id, &active_runtime).await {
            tracing::error!(user_id, error = ?error, "automatic game day stopped");
            if let Some(last_committed) = active_runtime.pause_if_active(signal)
                && let Err(error) = state.broadcast(&last_committed, &active_runtime)
            {
                tracing::error!(user_id, error = ?error, "failed to broadcast automatic pause");
            }
        }
        // The next wait is created only after this commit and broadcast have completed.
    }
}

fn to_character_start_response(
    receipt: StartGameReceipt,
    snapshot: GameSnapshot,
) -> CharacterStartResponse {
    CharacterStartResponse {
        start: CharacterStartSnapshot {
            command_id: receipt.command_id.to_string(),
            committed_cursor: receipt.committed_cursor.into(),
            replayed: receipt.replayed,
        },
        snapshot,
    }
}

fn to_advance_response(receipt: AdvanceCommandReceipt, snapshot: GameSnapshot) -> AdvanceResponse {
    AdvanceResponse {
        advance: AdvanceCommandSnapshot {
            command_id: receipt.command_id.to_string(),
            requested_days: receipt.requested_days,
            initial_cursor: receipt.initial_cursor.into(),
            committed_cursor: receipt.committed_cursor.into(),
            replayed: receipt.replayed,
        },
        snapshot,
    }
}

fn to_career_specs_response(state: CareerSpecsState) -> CareerSpecsResponse {
    CareerSpecsResponse {
        focused_job_family_key: state.focused_job_family_key,
        possessed_scores: to_career_scores_snapshot(state.possessed_scores),
        items: state
            .items
            .into_iter()
            .map(to_career_evidence_snapshot)
            .collect(),
        next_before: state.next_before,
    }
}

fn to_career_evidence_snapshot(state: CareerEvidenceState) -> CareerEvidenceSnapshot {
    CareerEvidenceSnapshot {
        id: state.id,
        evidence_key: state.evidence_key,
        catalog_entry_id: state.catalog_entry_id,
        catalog_entry_key: state.catalog_entry_key,
        display_name: state.display_name,
        kind: state.kind,
        acquired_game_day: state.acquired_game_day,
        expires_on_game_day: state.expires_on_game_day,
        period_start_date: state.period_start_date,
        period_end_exclusive_date: state.period_end_exclusive_date,
        credited_experience_days: state.credited_experience_days,
    }
}

fn to_career_activities_response(state: CareerActivitiesState) -> CareerActivitiesResponse {
    CareerActivitiesResponse {
        catalog: state
            .catalog
            .into_iter()
            .map(to_career_activity_catalog_snapshot)
            .collect(),
        active: state
            .active
            .into_iter()
            .map(to_career_activity_snapshot)
            .collect(),
        items: state
            .items
            .into_iter()
            .map(to_career_activity_history_snapshot)
            .collect(),
        next_before: state.next_before,
    }
}

fn to_career_activity_catalog_snapshot(
    state: CareerActivityCatalogState,
) -> CareerActivityCatalogSnapshot {
    CareerActivityCatalogSnapshot {
        id: state.id,
        activity_key: state.activity_key,
        display_name: state.display_name,
        output_kind: state.output_kind,
        minimum_calendar_days: state.minimum_calendar_days,
        required_effort_units: state.required_effort_units,
        daily_effort_cap_units: state.daily_effort_cap_units,
        allowed_life_statuses: state.allowed_life_statuses,
        cost_krw: state.cost_krw,
    }
}

fn to_career_activity_snapshot(state: CareerActivityState) -> CareerActivitySnapshot {
    CareerActivitySnapshot {
        id: state.id,
        catalog_entry_id: state.catalog_entry_id,
        activity_key: state.activity_key,
        display_name: state.display_name,
        status: state.status,
        priority: state.priority,
        started_game_day: state.started_game_day,
        accumulated_effort_units: state.accumulated_effort_units,
        required_effort_units: state.required_effort_units,
        elapsed_calendar_days: state.elapsed_calendar_days,
        minimum_calendar_days: state.minimum_calendar_days,
        daily_effort_cap_units: state.daily_effort_cap_units,
        completed_game_day: state.completed_game_day,
    }
}

fn to_career_activity_history_snapshot(
    state: CareerActivityState,
) -> CareerActivityHistorySnapshot {
    CareerActivityHistorySnapshot {
        id: state.id,
        catalog_entry_id: state.catalog_entry_id,
        activity_key: state.activity_key,
        display_name: state.display_name,
        status: state.status,
        priority: state.priority,
        started_game_day: state.started_game_day,
        accumulated_effort_units: state.accumulated_effort_units,
        required_effort_units: state.required_effort_units,
        elapsed_calendar_days: state.elapsed_calendar_days,
        minimum_calendar_days: state.minimum_calendar_days,
        daily_effort_cap_units: state.daily_effort_cap_units,
        completed_game_day: state.completed_game_day,
        cancelled_game_day: state.cancelled_game_day,
    }
}

fn to_career_artifacts_response(state: CareerArtifactPageState) -> Result<CareerArtifactsResponse> {
    Ok(CareerArtifactsResponse {
        items: state
            .items
            .into_iter()
            .map(to_career_artifact_version_snapshot)
            .collect::<Result<Vec<_>>>()?,
        next_before: state.next_before,
    })
}

fn to_career_artifact_version_snapshot(
    state: CareerArtifactState,
) -> Result<CareerArtifactVersionSnapshot> {
    let CareerArtifactState {
        id,
        kind,
        version_no,
        headline,
        summary,
        evidence_ids,
        completeness_bp,
        created_game_day,
        open_to_work,
        industries,
    } = state;
    Ok(match kind {
        ArtifactKind::Portfolio => {
            ensure_artifact_common_shape(open_to_work, &industries)?;
            CareerArtifactVersionSnapshot::Portfolio {
                id,
                version_no,
                headline,
                summary,
                evidence_ids,
                completeness_bp,
                created_game_day,
            }
        }
        ArtifactKind::Resume => {
            ensure_artifact_common_shape(open_to_work, &industries)?;
            CareerArtifactVersionSnapshot::Resume {
                id,
                version_no,
                headline,
                summary,
                evidence_ids,
                completeness_bp,
                created_game_day,
            }
        }
        ArtifactKind::LinkedinProfile => CareerArtifactVersionSnapshot::LinkedinProfile {
            id,
            version_no,
            headline,
            summary,
            evidence_ids,
            completeness_bp,
            created_game_day,
            open_to_work: open_to_work
                .context("stored LinkedIn artifact has no open-to-work flag")?,
            industries,
        },
    })
}

fn ensure_artifact_common_shape(open_to_work: Option<bool>, industries: &[Industry]) -> Result<()> {
    if open_to_work.is_some() || !industries.is_empty() {
        bail!("stored non-LinkedIn artifact has LinkedIn-only fields");
    }
    Ok(())
}

fn to_career_scores_snapshot(scores: crate::career::DimensionScores) -> CareerScoresSnapshot {
    CareerScoresSnapshot {
        education: scores.education,
        certification: scores.certification,
        language: scores.language,
        training: scores.training,
        experience: scores.experience,
        project: scores.project,
    }
}

fn to_career_jobs_response(state: CareerJobsPageState) -> CareerJobsResponse {
    CareerJobsResponse {
        items: state
            .items
            .into_iter()
            .map(to_career_job_snapshot)
            .collect(),
        next_before: state.next_before,
    }
}

fn to_career_job_snapshot(state: CareerJobState) -> CareerJobSnapshot {
    CareerJobSnapshot {
        posting_key: state.posting_key,
        posted_game_day: state.posted_game_day,
        closes_exclusive_game_day: state.closes_exclusive_game_day,
        platform: state.platform,
        industry: state.industry,
        job_family_key: state.job_family_key,
        employer_name: state.employer_name,
        region: state.region,
        employment_type: state.employment_type,
        required_scores: to_career_scores_snapshot(state.required_scores),
        possessed_scores: to_career_scores_snapshot(state.possessed_scores),
        minimum_annual_salary_krw: state.minimum_annual_salary_krw,
        maximum_annual_salary_krw: state.maximum_annual_salary_krw,
        salary_step_krw: state.salary_step_krw,
        competition_band: state.competition_band,
        military_requirement: match state.military_requirement {
            crate::career::MilitaryPostingRequirement::None => "any".to_owned(),
            crate::career::MilitaryPostingRequirement::CompletedOrExempt => {
                "completedOrExempt".to_owned()
            }
        },
        minimum_education: state.minimum_education,
        required_certification_name: state.required_certification_name,
        minimum_experience_days: state.minimum_experience_days,
        required_artifacts: state.required_artifacts,
    }
}

fn to_career_applications_response(
    state: CareerApplicationsPageState,
) -> CareerApplicationsResponse {
    CareerApplicationsResponse {
        items: state
            .items
            .into_iter()
            .map(to_career_application_snapshot)
            .collect(),
        next_before: state.next_before,
        open_invitations: state
            .open_invitations
            .into_iter()
            .map(to_career_invitation_snapshot)
            .collect(),
    }
}

fn to_career_application_snapshot(state: CareerApplicationState) -> CareerApplicationSnapshot {
    CareerApplicationSnapshot {
        id: state.id,
        posting_key: state.posting_key,
        platform: state.platform,
        industry: state.industry,
        employer_name: state.employer_name,
        job_family_key: state.job_family_key,
        source: state.source,
        status: state.status,
        submitted_game_day: state.submitted_game_day,
        visible_scores: to_career_scores_snapshot(state.visible_scores),
        possessed_scores: to_career_scores_snapshot(state.possessed_scores),
        document_score_bp: state.document_score_bp,
        document_decision_game_day: state.document_decision_game_day,
        interview_game_day: state.interview_game_day,
        confirmation_deadline_exclusive_game_day: state.confirmation_deadline_exclusive_game_day,
        interview_score_bp: state.interview_score_bp,
        offer: state.offer.map(to_career_offer_snapshot),
    }
}

fn to_career_open_application_snapshot(
    state: CareerApplicationState,
) -> CareerOpenApplicationSnapshot {
    CareerOpenApplicationSnapshot {
        id: state.id,
        posting_key: state.posting_key,
        platform: state.platform,
        industry: state.industry,
        employer_name: state.employer_name,
        job_family_key: state.job_family_key,
        status: state.status,
        confirmation_deadline_exclusive_game_day: state.confirmation_deadline_exclusive_game_day,
        interview_game_day: state.interview_game_day,
        offer: state.offer.map(to_career_offer_snapshot),
    }
}

fn to_career_offer_snapshot(state: crate::store::CareerOfferState) -> CareerOfferSnapshot {
    CareerOfferSnapshot {
        id: state.id,
        status: state.status,
        annual_salary_krw: state.annual_salary_krw,
        payday_day_of_month: state.payday_day_of_month,
        start_game_day: state.start_game_day,
        expires_exclusive_game_day: state.expires_exclusive_game_day,
        wanted_reward_krw: state.wanted_reward_krw,
    }
}

fn to_career_invitation_snapshot(state: CareerInvitationState) -> CareerInvitationSnapshot {
    CareerInvitationSnapshot {
        id: state.id,
        posting_key: state.posting_key,
        platform: state.platform,
        industry: state.industry,
        job_family_key: state.job_family_key,
        employer_name: state.employer_name,
        artifact_version_id: state.artifact_version_id,
        created_game_day: state.created_game_day,
        expires_exclusive_game_day: state.expires_exclusive_game_day,
    }
}

fn to_career_employment_response(state: CareerEmploymentState) -> CareerEmploymentResponse {
    CareerEmploymentResponse {
        contract: state
            .contract
            .as_ref()
            .map(to_career_employment_contract_snapshot),
    }
}

fn to_career_employment_contract_snapshot(
    state: &EmploymentContractState,
) -> CareerEmploymentContractSnapshot {
    CareerEmploymentContractSnapshot {
        id: state.id,
        status: state.status,
        job_family_key: state.job_family_key.clone(),
        employer_name: state.employer_name.clone(),
        region: state.region.clone(),
        annual_salary_krw: state.annual_salary_krw,
        payday_day_of_month: state.payday_day_of_month,
        start_game_day: state.start_game_day,
        end_game_day: state.end_game_day,
        credited_experience_days: state.credited_experience_days,
    }
}

fn to_career_payroll_response(state: CareerPayrollPageState) -> CareerPayrollResponse {
    CareerPayrollResponse {
        items: state
            .items
            .into_iter()
            .map(to_career_payroll_snapshot)
            .collect(),
        next_before: state.next_before,
    }
}

fn to_career_payroll_snapshot(state: CareerPayrollState) -> CareerPayrollSnapshot {
    CareerPayrollSnapshot {
        id: state.id,
        contract_id: state.contract_id,
        period_no: state.period_no,
        salary_month_ordinal: state.salary_month_ordinal,
        period_start_date: state.period_start_date,
        period_end_exclusive_date: state.period_end_exclusive_date,
        paid_game_day: state.paid_game_day,
        gross_pay_krw: state.gross_pay_krw,
        employee_national_pension_krw: state.employee_national_pension_krw,
        employer_national_pension_krw: state.employer_national_pension_krw,
        employee_health_insurance_krw: state.employee_health_insurance_krw,
        employer_health_insurance_krw: state.employer_health_insurance_krw,
        employee_long_term_care_krw: state.employee_long_term_care_krw,
        employer_long_term_care_krw: state.employer_long_term_care_krw,
        employee_employment_insurance_krw: state.employee_employment_insurance_krw,
        employer_employment_insurance_krw: state.employer_employment_insurance_krw,
        employer_industrial_accident_krw: state.employer_industrial_accident_krw,
        withheld_income_tax_krw: state.withheld_income_tax_krw,
        withheld_local_income_tax_krw: state.withheld_local_income_tax_krw,
        net_pay_krw: state.net_pay_krw,
        reward: state.reward.map(to_career_reward_payment_snapshot),
    }
}

fn to_career_employment_tax_year_snapshot(
    state: &CareerEmploymentTaxYearState,
) -> CareerEmploymentTaxYearSnapshot {
    CareerEmploymentTaxYearSnapshot {
        tax_year: state.tax_year,
        status: match state.status {
            CareerEmploymentTaxYearStatus::Open => CareerEmploymentTaxYearStatusSnapshot::Open,
            CareerEmploymentTaxYearStatus::Provisional => {
                CareerEmploymentTaxYearStatusSnapshot::Provisional
            }
            CareerEmploymentTaxYearStatus::Definitive => {
                CareerEmploymentTaxYearStatusSnapshot::Definitive
            }
        },
        source: match state.source {
            CareerEmploymentTaxYearSource::EmploymentOnly => {
                CareerEmploymentTaxYearSourceSnapshot::EmploymentOnly
            }
            CareerEmploymentTaxYearSource::Combined => {
                CareerEmploymentTaxYearSourceSnapshot::Combined
            }
            CareerEmploymentTaxYearSource::LegacyProfile => {
                CareerEmploymentTaxYearSourceSnapshot::LegacyProfile
            }
        },
        gross_employment_income_krw: state.gross_employment_income_krw,
        employee_insurance_deduction_krw: state.employee_insurance_deduction_krw,
        earned_income_deduction_krw: state.earned_income_deduction_krw,
        personal_deduction_krw: state.personal_deduction_krw,
        taxable_income_krw: state.taxable_income_krw,
        calculated_income_tax_krw: state.calculated_income_tax_krw,
        earned_income_tax_credit_krw: state.earned_income_tax_credit_krw,
        pension_credit_eligible_contribution_krw: state.pension_credit_eligible_contribution_krw,
        actual_pension_income_tax_credit_krw: state.actual_pension_income_tax_credit_krw,
        actual_pension_local_income_tax_effect_krw: state
            .actual_pension_local_income_tax_effect_krw,
        withheld_income_tax_krw: state.withheld_income_tax_krw,
        withheld_local_income_tax_krw: state.withheld_local_income_tax_krw,
        assessed_income_tax_krw: state.assessed_income_tax_krw,
        assessed_local_income_tax_krw: state.assessed_local_income_tax_krw,
        additional_tax_krw: state.additional_tax_krw,
        refund_krw: state.refund_krw,
        reconciliation_game_day: state.reconciliation_game_day,
    }
}

fn to_military_options_response(state: MilitaryOptionsState) -> MilitaryOptionsResponse {
    MilitaryOptionsResponse {
        items: state
            .items
            .into_iter()
            .map(to_military_option_snapshot)
            .collect(),
    }
}

fn to_military_option_snapshot(state: MilitaryOptionState) -> MilitaryOptionSnapshot {
    let pay_schedule = match state.pay_schedule {
        crate::career::MilitaryPayScheduleKind::Monthly => MilitaryPayScheduleSnapshot::Monthly,
    };
    MilitaryOptionSnapshot {
        id: state.id,
        option_key: state.option_key,
        service_type: to_military_service_type_snapshot(state.service_type),
        display_name: state.display_name,
        eligible: state.eligible,
        ineligibility_reasons: state
            .ineligibility_reasons
            .into_iter()
            .map(to_military_option_ineligibility_reason_snapshot)
            .collect(),
        service_duration_months: state.service_duration_months,
        hard_requirements: MilitaryHardRequirementsSnapshot {
            minimum_education: state.hard_requirements.minimum_education,
            required_certification_count: state.hard_requirements.minimum_certification_count,
            minimum_experience_days: state.hard_requirements.minimum_experience_days,
        },
        compensation_kind: to_military_compensation_kind_snapshot(state.compensation_kind),
        pay_schedule,
        pay_stages: state
            .pay_stages
            .into_iter()
            .map(|stage| MilitaryPayStageSnapshot {
                start_service_month: stage.start_service_month,
                end_exclusive_service_month: stage.end_exclusive_service_month,
                gross_monthly_pay_krw: stage.gross_monthly_pay_krw,
            })
            .collect(),
        effort_life_status: state.effort_life_status,
        daily_effort_capacity_units: state.daily_effort_capacity_units,
        grants_career_experience: state.grants_career_experience,
        experience_credits: state
            .experience_credits
            .into_iter()
            .map(|credit| MilitaryExperienceCreditSnapshot {
                job_family_key: credit.job_family_key,
                daily_credit_ppm: credit.daily_credit_ppm,
            })
            .collect(),
    }
}

fn to_military_service_response(state: MilitaryServiceState) -> MilitaryServiceResponse {
    MilitaryServiceResponse {
        military_status: to_military_status_snapshot(state.military_status),
        service: state.service.map(to_military_service_history_snapshot),
    }
}

fn to_active_military_service_snapshot(
    state: &ActiveMilitaryServiceState,
) -> Result<ActiveMilitaryServiceSummarySnapshot> {
    let status = match state.status {
        crate::career::MilitaryServiceStatus::PendingStart => {
            ActiveMilitaryServiceStatusSnapshot::PendingStart
        }
        crate::career::MilitaryServiceStatus::Serving => {
            ActiveMilitaryServiceStatusSnapshot::Serving
        }
        crate::career::MilitaryServiceStatus::Completed => {
            bail!("active military service cannot be completed")
        }
    };
    Ok(ActiveMilitaryServiceSummarySnapshot {
        id: state.id,
        option_version_id: state.option_version_id,
        service_type: to_military_service_type_snapshot(state.service_type),
        display_name: state.display_name.clone(),
        status,
        start_game_day: state.start_game_day,
        end_game_day: state.end_game_day,
        credited_service_days: state.credited_service_days,
        total_service_days: state.total_service_days,
        effort_life_status: state.effort_life_status,
        grants_career_experience: state.grants_career_experience,
        next_pay_game_day: state.next_pay_game_day,
    })
}

fn to_military_service_history_snapshot(
    state: MilitaryServiceHistoryState,
) -> MilitaryServiceHistorySnapshot {
    MilitaryServiceHistorySnapshot {
        id: state.id,
        option_version_id: state.option_version_id,
        service_type: to_military_service_type_snapshot(state.service_type),
        display_name: state.display_name,
        status: to_military_service_status_snapshot(state.status),
        source_kind: to_military_service_source_kind_snapshot(state.source_kind),
        start_game_day: state.start_game_day,
        end_game_day: state.end_game_day,
        start_date: state.start_date,
        end_exclusive_date: state.end_exclusive_date,
        credited_service_days: state.credited_service_days,
        total_service_days: state.total_service_days,
        effort_life_status: state.effort_life_status,
        grants_career_experience: state.grants_career_experience,
        next_pay_game_day: state.next_pay_game_day,
        completed_game_day: state.completed_game_day,
    }
}

fn to_military_savings_products_response(
    state: MilitarySavingsProductsState,
) -> MilitarySavingsProductsResponse {
    MilitarySavingsProductsResponse {
        items: state
            .items
            .into_iter()
            .map(to_military_savings_product_snapshot)
            .collect(),
    }
}

fn to_military_savings_product_snapshot(
    state: MilitarySavingsProductState,
) -> MilitarySavingsProductSnapshot {
    let day_count_convention = match state.day_count_convention {
        MilitarySavingsDayCountConvention::Actual365 => {
            MilitarySavingsDayCountConventionSnapshot::Actual365
        }
    };
    let interest_rounding = match state.interest_rounding {
        MilitarySavingsInterestRounding::FloorToKrw => {
            MilitarySavingsInterestRoundingSnapshot::FloorToKrw
        }
    };
    MilitarySavingsProductSnapshot {
        id: state.id,
        product_key: state.product_key,
        institution_key: state.institution_key,
        institution_display_name: state.institution_display_name,
        eligible: state.eligible,
        ineligibility_reasons: state
            .ineligibility_reasons
            .into_iter()
            .map(to_military_savings_ineligibility_reason_snapshot)
            .collect(),
        eligible_service_types: state
            .eligible_service_types
            .into_iter()
            .map(to_military_service_type_snapshot)
            .collect(),
        join_start_date: state.join_start_date,
        join_end_date: state.join_end_date,
        minimum_remaining_service_months: state.minimum_remaining_service_months,
        maximum_active_contracts: state.maximum_active_contracts,
        maximum_contracts_per_institution: state.maximum_contracts_per_institution,
        minimum_monthly_contribution_krw: state.minimum_monthly_contribution_krw,
        maximum_institution_monthly_contribution_krw: state
            .maximum_institution_monthly_contribution_krw,
        maximum_total_monthly_contribution_krw: state.maximum_total_monthly_contribution_krw,
        limit_setting_unit_krw: state.limit_setting_unit_krw,
        installment_unit_krw: state.installment_unit_krw,
        interest_tiers: state
            .interest_tiers
            .into_iter()
            .map(to_military_savings_interest_tier_snapshot)
            .collect(),
        day_count_convention,
        interest_rounding,
        early_close_annual_interest_rate_ppm: state.early_close_annual_interest_rate_ppm,
        government_matching_rate_ppm: state.government_matching_rate_ppm,
        government_match_payment_day_of_month: state.government_match_payment_day_of_month,
        maturity_tax_exempt: state.maturity_tax_exempt,
    }
}

fn to_military_savings_interest_tier_snapshot(
    state: MilitarySavingsInterestTierState,
) -> MilitarySavingsInterestTierSnapshot {
    MilitarySavingsInterestTierSnapshot {
        minimum_term_months: state.minimum_term_months,
        maximum_term_months_inclusive: state.maximum_term_months_inclusive,
        annual_interest_rate_ppm: state.annual_interest_rate_ppm,
    }
}

fn to_active_military_savings_snapshot(
    state: &ActiveMilitarySavingsState,
) -> Result<ActiveMilitarySavingsSummarySnapshot> {
    if state.status != MilitarySavingsContractStatus::Active {
        bail!("active military savings summary cannot contain a closed contract");
    }
    Ok(ActiveMilitarySavingsSummarySnapshot {
        id: state.id,
        product_version_id: state.product_version_id,
        institution_key: state.institution_key.clone(),
        status: ActiveMilitarySavingsStatusSnapshot::Active,
        monthly_contribution_krw: state.monthly_contribution_krw,
        debit_day_of_month: state.debit_day_of_month,
        principal_krw: state.principal_krw,
        paid_installment_count: u32::from(state.paid_installment_count),
        missed_installment_count: u32::from(state.missed_installment_count),
        next_installment_game_day: state.next_installment_game_day,
        maturity_game_day: state.maturity_game_day,
    })
}

fn to_military_savings_history_response(
    state: MilitarySavingsPageState,
) -> MilitarySavingsHistoryResponse {
    MilitarySavingsHistoryResponse {
        items: state
            .items
            .into_iter()
            .map(to_military_savings_history_item_snapshot)
            .collect(),
        next_before: state.next_before,
    }
}

fn to_military_savings_history_item_snapshot(
    state: MilitarySavingsHistoryItemState,
) -> MilitarySavingsHistoryItemSnapshot {
    MilitarySavingsHistoryItemSnapshot {
        id: state.id,
        service_id: state.service_id,
        product_version_id: state.product_version_id,
        product_key: state.product_key,
        institution_key: state.institution_key,
        institution_display_name: state.institution_display_name,
        status: to_military_savings_contract_status_snapshot(state.status),
        monthly_contribution_krw: state.monthly_contribution_krw,
        debit_day_of_month: state.debit_day_of_month,
        principal_krw: state.principal_krw,
        paid_installment_count: u32::from(state.paid_installment_count),
        missed_installment_count: u32::from(state.missed_installment_count),
        next_installment_game_day: state.next_installment_game_day,
        maturity_game_day: state.maturity_game_day,
        opened_game_day: state.opened_game_day,
        first_installment_game_day: state.first_installment_game_day,
        contract_term_months: state.contract_term_months,
        annual_interest_rate_ppm: state.annual_interest_rate_ppm,
        closed_game_day: state.closed_game_day,
        closure_reason: state
            .closure_reason
            .map(to_military_savings_closure_reason_snapshot),
        settled_principal_krw: state.settled_principal_krw,
        gross_bank_interest_krw: state.gross_bank_interest_krw,
        government_match_krw: state.government_match_krw,
        bank_payout_krw: state.bank_payout_krw,
        government_match_paid_game_day: state.government_match_paid_game_day,
        projected_maturity: state
            .projected_maturity
            .map(to_military_savings_maturity_projection_snapshot),
        installments: state
            .installments
            .into_iter()
            .map(to_military_savings_installment_snapshot)
            .collect(),
    }
}

fn to_military_savings_installment_snapshot(
    state: MilitarySavingsInstallmentState,
) -> MilitarySavingsInstallmentSnapshot {
    MilitarySavingsInstallmentSnapshot {
        id: state.id,
        installment_no: u32::from(state.installment_no),
        due_game_day: state.due_game_day,
        status: to_military_savings_installment_status_snapshot(state.status),
        paid_game_day: state.paid_game_day,
        principal_krw: state.principal_krw,
        government_matching_policy_version_id: state.government_matching_policy_version_id,
        government_matching_rate_ppm: state.government_matching_rate_ppm,
    }
}

fn to_military_savings_maturity_projection_snapshot(
    state: MilitarySavingsMaturityProjectionState,
) -> MilitarySavingsMaturityProjectionSnapshot {
    let assumption = match state.assumption {
        MilitarySavingsProjectionAssumption::AllScheduledInstallmentsPaid => {
            MilitarySavingsProjectionAssumptionSnapshot::AllScheduledInstallmentsPaid
        }
    };
    MilitarySavingsMaturityProjectionSnapshot {
        assumption,
        principal_krw: state.principal_krw,
        gross_bank_interest_krw: state.gross_bank_interest_krw,
        government_match_krw: state.government_match_krw,
        bank_payout_krw: state.bank_payout_krw,
        total_benefit_krw: state.total_benefit_krw,
    }
}

fn to_military_service_command_response(
    receipt: MilitaryServiceCommandReceipt,
    snapshot: GameSnapshot,
) -> Result<MilitaryServiceCommandResponse> {
    let status = match receipt.status {
        crate::career::MilitaryServiceStatus::PendingStart => {
            ActiveMilitaryServiceStatusSnapshot::PendingStart
        }
        crate::career::MilitaryServiceStatus::Serving => {
            ActiveMilitaryServiceStatusSnapshot::Serving
        }
        crate::career::MilitaryServiceStatus::Completed => {
            bail!("military service start command cannot return completed status")
        }
    };
    Ok(MilitaryServiceCommandResponse {
        result: MilitaryServiceResultSnapshot {
            military_service_id: receipt.military_service_id,
            status,
        },
        replayed: receipt.replayed,
        snapshot,
    })
}

fn to_military_savings_command_response(
    receipt: MilitarySavingsCommandReceipt,
    snapshot: GameSnapshot,
) -> MilitarySavingsCommandResponse {
    MilitarySavingsCommandResponse {
        result: MilitarySavingsResultSnapshot {
            military_savings_contract_id: receipt.military_savings_contract_id,
            status: to_military_savings_contract_status_snapshot(receipt.status),
        },
        replayed: receipt.replayed,
        snapshot,
    }
}

const fn to_military_status_snapshot(
    status: crate::career::MilitaryStatus,
) -> MilitaryStatusSnapshot {
    match status {
        crate::career::MilitaryStatus::Unserved => MilitaryStatusSnapshot::Unserved,
        crate::career::MilitaryStatus::Serving => MilitaryStatusSnapshot::Serving,
        crate::career::MilitaryStatus::Completed => MilitaryStatusSnapshot::Completed,
        crate::career::MilitaryStatus::Exempt => MilitaryStatusSnapshot::Exempt,
    }
}

const fn to_military_service_type_snapshot(
    service_type: crate::career::MilitaryServiceType,
) -> MilitaryServiceTypeSnapshot {
    match service_type {
        crate::career::MilitaryServiceType::ActiveDuty => MilitaryServiceTypeSnapshot::ActiveDuty,
        crate::career::MilitaryServiceType::SocialService => {
            MilitaryServiceTypeSnapshot::SocialService
        }
        crate::career::MilitaryServiceType::IndustrialTechnical => {
            MilitaryServiceTypeSnapshot::IndustrialTechnical
        }
        crate::career::MilitaryServiceType::ProfessionalResearch => {
            MilitaryServiceTypeSnapshot::ProfessionalResearch
        }
        crate::career::MilitaryServiceType::CommissionedOfficer => {
            MilitaryServiceTypeSnapshot::CommissionedOfficer
        }
        crate::career::MilitaryServiceType::NonCommissionedOfficer => {
            MilitaryServiceTypeSnapshot::NonCommissionedOfficer
        }
    }
}

const fn to_military_service_status_snapshot(
    status: crate::career::MilitaryServiceStatus,
) -> MilitaryServiceStatusSnapshot {
    match status {
        crate::career::MilitaryServiceStatus::PendingStart => {
            MilitaryServiceStatusSnapshot::PendingStart
        }
        crate::career::MilitaryServiceStatus::Serving => MilitaryServiceStatusSnapshot::Serving,
        crate::career::MilitaryServiceStatus::Completed => MilitaryServiceStatusSnapshot::Completed,
    }
}

const fn to_military_service_source_kind_snapshot(
    source: MilitaryServiceSourceKind,
) -> MilitaryServiceSourceKindSnapshot {
    match source {
        MilitaryServiceSourceKind::UserCommand => MilitaryServiceSourceKindSnapshot::UserCommand,
        MilitaryServiceSourceKind::LegacyBridge => MilitaryServiceSourceKindSnapshot::LegacyBridge,
    }
}

const fn to_military_compensation_kind_snapshot(
    kind: MilitaryCompensationKind,
) -> MilitaryCompensationKindSnapshot {
    match kind {
        MilitaryCompensationKind::MilitaryPay => MilitaryCompensationKindSnapshot::MilitaryPay,
        MilitaryCompensationKind::EmploymentPayroll => {
            MilitaryCompensationKindSnapshot::EmploymentPayroll
        }
    }
}

const fn to_military_option_ineligibility_reason_snapshot(
    reason: MilitaryOptionIneligibilityReason,
) -> MilitaryOptionIneligibilityReasonSnapshot {
    match reason {
        MilitaryOptionIneligibilityReason::MilitarySubjectRequired => {
            MilitaryOptionIneligibilityReasonSnapshot::MilitarySubjectRequired
        }
        MilitaryOptionIneligibilityReason::MilitaryStateConflict => {
            MilitaryOptionIneligibilityReasonSnapshot::MilitaryStateConflict
        }
        MilitaryOptionIneligibilityReason::MinimumEducation => {
            MilitaryOptionIneligibilityReasonSnapshot::MinimumEducation
        }
        MilitaryOptionIneligibilityReason::MinimumCertificationCount => {
            MilitaryOptionIneligibilityReasonSnapshot::MinimumCertificationCount
        }
        MilitaryOptionIneligibilityReason::MinimumExperienceDays => {
            MilitaryOptionIneligibilityReasonSnapshot::MinimumExperienceDays
        }
        MilitaryOptionIneligibilityReason::PolicyUnavailable => {
            MilitaryOptionIneligibilityReasonSnapshot::PolicyUnavailable
        }
    }
}

const fn to_military_savings_ineligibility_reason_snapshot(
    reason: MilitarySavingsIneligibilityReason,
) -> MilitarySavingsIneligibilityReasonSnapshot {
    match reason {
        MilitarySavingsIneligibilityReason::MilitaryStateConflict => {
            MilitarySavingsIneligibilityReasonSnapshot::MilitaryStateConflict
        }
        MilitarySavingsIneligibilityReason::ServiceTypeNotEligible => {
            MilitarySavingsIneligibilityReasonSnapshot::ServiceTypeNotEligible
        }
        MilitarySavingsIneligibilityReason::MinimumRemainingService => {
            MilitarySavingsIneligibilityReasonSnapshot::MinimumRemainingService
        }
        MilitarySavingsIneligibilityReason::ActiveContractLimit => {
            MilitarySavingsIneligibilityReasonSnapshot::ActiveContractLimit
        }
        MilitarySavingsIneligibilityReason::InstitutionLimit => {
            MilitarySavingsIneligibilityReasonSnapshot::InstitutionLimit
        }
        MilitarySavingsIneligibilityReason::JoinWindowClosed => {
            MilitarySavingsIneligibilityReasonSnapshot::JoinWindowClosed
        }
        MilitarySavingsIneligibilityReason::PolicyUnavailable => {
            MilitarySavingsIneligibilityReasonSnapshot::PolicyUnavailable
        }
    }
}

const fn to_military_savings_contract_status_snapshot(
    status: MilitarySavingsContractStatus,
) -> MilitarySavingsContractStatusSnapshot {
    match status {
        MilitarySavingsContractStatus::Active => MilitarySavingsContractStatusSnapshot::Active,
        MilitarySavingsContractStatus::Matured => MilitarySavingsContractStatusSnapshot::Matured,
        MilitarySavingsContractStatus::Closed => MilitarySavingsContractStatusSnapshot::Closed,
    }
}

const fn to_military_savings_installment_status_snapshot(
    status: MilitarySavingsInstallmentStatusState,
) -> MilitarySavingsInstallmentStatusSnapshot {
    match status {
        MilitarySavingsInstallmentStatusState::Scheduled => {
            MilitarySavingsInstallmentStatusSnapshot::Scheduled
        }
        MilitarySavingsInstallmentStatusState::Paid => {
            MilitarySavingsInstallmentStatusSnapshot::Paid
        }
        MilitarySavingsInstallmentStatusState::Missed => {
            MilitarySavingsInstallmentStatusSnapshot::Missed
        }
    }
}

const fn to_military_savings_closure_reason_snapshot(
    reason: MilitarySavingsClosureReason,
) -> MilitarySavingsClosureReasonSnapshot {
    match reason {
        MilitarySavingsClosureReason::Maturity => MilitarySavingsClosureReasonSnapshot::Maturity,
        MilitarySavingsClosureReason::EarlyClose => {
            MilitarySavingsClosureReasonSnapshot::EarlyClose
        }
    }
}

const fn to_career_scheduled_action_kind_snapshot(
    kind: CareerScheduledActionKind,
) -> CareerScheduledActionKindSnapshot {
    match kind {
        CareerScheduledActionKind::EmploymentStart => {
            CareerScheduledActionKindSnapshot::EmploymentStart
        }
        CareerScheduledActionKind::MilitaryServiceStart => {
            CareerScheduledActionKindSnapshot::MilitaryServiceStart
        }
        CareerScheduledActionKind::MilitaryServiceCompletion => {
            CareerScheduledActionKindSnapshot::MilitaryServiceCompletion
        }
        CareerScheduledActionKind::DocumentReview => {
            CareerScheduledActionKindSnapshot::DocumentReview
        }
        CareerScheduledActionKind::ConfirmationExpiry => {
            CareerScheduledActionKindSnapshot::ConfirmationExpiry
        }
        CareerScheduledActionKind::InterviewDecision => {
            CareerScheduledActionKindSnapshot::InterviewDecision
        }
        CareerScheduledActionKind::OfferExpiry => CareerScheduledActionKindSnapshot::OfferExpiry,
        CareerScheduledActionKind::InvitationGeneration => {
            CareerScheduledActionKindSnapshot::InvitationGeneration
        }
    }
}

const fn to_career_scheduled_settlement_kind_snapshot(
    kind: CareerScheduledSettlementKind,
) -> CareerScheduledSettlementKindSnapshot {
    match kind {
        CareerScheduledSettlementKind::EmploymentPayroll => {
            CareerScheduledSettlementKindSnapshot::EmploymentPayroll
        }
        CareerScheduledSettlementKind::EmploymentReconciliation => {
            CareerScheduledSettlementKindSnapshot::EmploymentReconciliation
        }
        CareerScheduledSettlementKind::MilitaryPay => {
            CareerScheduledSettlementKindSnapshot::MilitaryPay
        }
        CareerScheduledSettlementKind::MilitarySavingsInstallment => {
            CareerScheduledSettlementKindSnapshot::MilitarySavingsInstallment
        }
        CareerScheduledSettlementKind::MilitarySavingsMaturity => {
            CareerScheduledSettlementKindSnapshot::MilitarySavingsMaturity
        }
        CareerScheduledSettlementKind::MilitarySavingsGovernmentMatch => {
            CareerScheduledSettlementKindSnapshot::MilitarySavingsGovernmentMatch
        }
    }
}

fn to_career_pending_schedule_item_snapshot(
    state: &CareerPendingScheduleItemState,
) -> CareerPendingScheduleItemSnapshot {
    match state {
        CareerPendingScheduleItemState::CareerAction {
            id,
            due_game_day,
            kind,
        } => CareerPendingScheduleItemSnapshot::CareerAction {
            id: *id,
            due_game_day: *due_game_day,
            kind: to_career_scheduled_action_kind_snapshot(*kind),
        },
        CareerPendingScheduleItemState::Settlement {
            id,
            due_game_day,
            kind,
        } => CareerPendingScheduleItemSnapshot::Settlement {
            id: *id,
            due_game_day: *due_game_day,
            kind: to_career_scheduled_settlement_kind_snapshot(*kind),
        },
    }
}

fn to_career_reward_payment_snapshot(
    state: CareerRewardPaymentState,
) -> CareerRewardPaymentSnapshot {
    CareerRewardPaymentSnapshot {
        payment_id: state.payment_id,
        gross_reward_krw: state.gross_reward_krw,
        withheld_income_tax_krw: state.withheld_income_tax_krw,
        withheld_local_income_tax_krw: state.withheld_local_income_tax_krw,
        net_reward_krw: state.net_reward_krw,
    }
}

fn to_career_application_command_response(
    receipt: CareerApplicationReceipt,
    snapshot: GameSnapshot,
) -> CareerApplicationResponse {
    CareerApplicationResponse {
        result: CareerApplicationResultSnapshot {
            application_id: receipt.application_id,
            status: receipt.status,
        },
        replayed: receipt.replayed,
        snapshot,
    }
}

fn to_career_invitation_command_response(
    receipt: CareerInvitationReceipt,
    snapshot: GameSnapshot,
) -> CareerInvitationResponse {
    CareerInvitationResponse {
        result: CareerInvitationResultSnapshot {
            invitation_id: receipt.invitation_id,
            status: receipt.status,
            application_id: receipt.application_id,
        },
        replayed: receipt.replayed,
        snapshot,
    }
}

fn to_career_offer_command_response(
    receipt: CareerOfferReceipt,
    snapshot: GameSnapshot,
) -> CareerOfferResponse {
    CareerOfferResponse {
        result: CareerOfferResultSnapshot {
            offer_id: receipt.offer_id,
            status: receipt.status,
            employment_contract_id: receipt.employment_contract_id,
        },
        replayed: receipt.replayed,
        snapshot,
    }
}

const fn to_life_rate_status_snapshot(status: LifeRateStatus) -> LifeRateStatusSnapshot {
    match status {
        LifeRateStatus::Active => LifeRateStatusSnapshot::Active,
        LifeRateStatus::RateUnavailable => LifeRateStatusSnapshot::RateUnavailable,
    }
}

const fn to_residence_tenure_snapshot(tenure: ResidenceTenureKind) -> ResidenceTenureKindSnapshot {
    match tenure {
        ResidenceTenureKind::RentFree => ResidenceTenureKindSnapshot::RentFree,
        ResidenceTenureKind::Owner => ResidenceTenureKindSnapshot::Owner,
        ResidenceTenureKind::Jeonse => ResidenceTenureKindSnapshot::Jeonse,
        ResidenceTenureKind::MonthlyRent => ResidenceTenureKindSnapshot::MonthlyRent,
    }
}

const fn to_living_cost_category_snapshot(
    category: LivingCostCategory,
) -> LivingCostCategorySnapshot {
    match category {
        LivingCostCategory::Housing => LivingCostCategorySnapshot::Housing,
        LivingCostCategory::Food => LivingCostCategorySnapshot::Food,
        LivingCostCategory::Transport => LivingCostCategorySnapshot::Transport,
        LivingCostCategory::Communication => LivingCostCategorySnapshot::Communication,
        LivingCostCategory::Utilities => LivingCostCategorySnapshot::Utilities,
        LivingCostCategory::Healthcare => LivingCostCategorySnapshot::Healthcare,
        LivingCostCategory::Education => LivingCostCategorySnapshot::Education,
        LivingCostCategory::DependentCare => LivingCostCategorySnapshot::DependentCare,
        LivingCostCategory::Discretionary => LivingCostCategorySnapshot::Discretionary,
    }
}

const fn to_year_month_snapshot(year_month: YearMonth) -> YearMonthSnapshot {
    YearMonthSnapshot {
        year: year_month.year,
        month: year_month.month,
    }
}

fn to_life_household_snapshot(state: &LifeHouseholdState) -> LifeHouseholdSnapshot {
    LifeHouseholdSnapshot {
        id: state.id,
        member_count: state.member_count,
        dependent_count: state.dependent_count,
        tax_dependent_eligible_count: state.tax_dependent_eligible_count,
    }
}

fn to_life_residence_snapshot(state: &LifeResidenceState) -> LifeResidenceSnapshot {
    LifeResidenceSnapshot {
        id: state.id,
        region_key: state.region_key.clone(),
        tenure_kind: to_residence_tenure_snapshot(state.tenure_kind),
        property_holding_id: state.property_holding_id,
        effective_from_game_day: state.effective_from_game_day,
    }
}

fn to_life_budget_band_snapshot(state: &LifeBudgetBandState) -> LifeBudgetBandSnapshot {
    LifeBudgetBandSnapshot {
        id: state.id,
        band_key: state.band_key.clone(),
        display_name: state.display_name.clone(),
        factor_ppm: state.factor_ppm,
    }
}

fn to_life_budget_selection_snapshot(
    state: &LifeBudgetSelectionState,
) -> LifeBudgetSelectionSnapshot {
    LifeBudgetSelectionSnapshot {
        category: to_living_cost_category_snapshot(state.category),
        band_id: state.band_id,
    }
}

fn to_living_cost_month_item_snapshot(
    state: &LivingCostMonthItemState,
) -> LivingCostMonthItemSnapshot {
    LivingCostMonthItemSnapshot {
        category: to_living_cost_category_snapshot(state.category),
        essential: state.essential,
        band_id: state.band_id,
        base_monthly_krw: state.base_monthly_krw,
        base_cpi_index: state.base_cpi_index,
        region_factor_ppm: state.region_factor_ppm,
        household_factor_ppm: state.household_factor_ppm,
        budget_factor_ppm: state.budget_factor_ppm,
        tenure_replacement_factor_ppm: state.tenure_replacement_factor_ppm,
        gross_krw: state.gross_krw,
        paid_krw: state.paid_krw,
        arrear_krw: state.arrear_krw,
    }
}

fn to_living_cost_month_snapshot(state: &LivingCostMonthState) -> LivingCostMonthSnapshot {
    LivingCostMonthSnapshot {
        id: state.id,
        profile_id: state.profile_id,
        profile_key: state.profile_key.clone(),
        current_cpi_index: state.current_cpi_index,
        proration_scale: state.proration_scale,
        proration_units: state.proration_units,
        proration_days: state.proration_days,
        days_in_month: state.days_in_month,
        year_month: to_year_month_snapshot(state.year_month),
        activation_game_day: state.activation_game_day,
        settlement_game_day: state.settlement_game_day,
        settled: state.settled,
        total_gross_krw: state.total_gross_krw,
        total_paid_krw: state.total_paid_krw,
        total_arrear_krw: state.total_arrear_krw,
        items: state
            .items
            .iter()
            .map(to_living_cost_month_item_snapshot)
            .collect(),
    }
}

fn to_essential_arrear_snapshot(state: &EssentialArrearState) -> EssentialArrearSnapshot {
    EssentialArrearSnapshot {
        id: state.id,
        due_year_month: to_year_month_snapshot(state.due_year_month),
        category: to_living_cost_category_snapshot(state.category),
        original_krw: state.original_krw,
        remaining_krw: state.remaining_krw,
    }
}

const fn to_housing_region_key_snapshot(region: LifeRegionKey) -> HousingRegionKeySnapshot {
    match region {
        LifeRegionKey::CapitalArea => HousingRegionKeySnapshot::CapitalArea,
        LifeRegionKey::Metropolitan => HousingRegionKeySnapshot::Metropolitan,
        LifeRegionKey::SmallCity => HousingRegionKeySnapshot::SmallCity,
        LifeRegionKey::Rural => HousingRegionKeySnapshot::Rural,
    }
}

const fn to_housing_rate_status_snapshot(
    status: HousingRateStatusState,
) -> HousingRateStatusSnapshot {
    match status {
        HousingRateStatusState::Active => HousingRateStatusSnapshot::Active,
        HousingRateStatusState::RateUnavailable => HousingRateStatusSnapshot::RateUnavailable,
    }
}

const fn to_housing_property_type_snapshot(
    property_type: PropertyType,
) -> HousingPropertyTypeSnapshot {
    match property_type {
        PropertyType::Apartment => HousingPropertyTypeSnapshot::Apartment,
        PropertyType::MultiFamily => HousingPropertyTypeSnapshot::MultiFamily,
        PropertyType::Detached => HousingPropertyTypeSnapshot::Detached,
    }
}

const fn to_housing_lease_capability_snapshot(
    capability: HousingLeaseCapability,
) -> HousingLeaseCapabilitySnapshot {
    match capability {
        HousingLeaseCapability::CashJeonse => HousingLeaseCapabilitySnapshot::CashJeonse,
        HousingLeaseCapability::CashJeonseAndMonthlyRent => {
            HousingLeaseCapabilitySnapshot::CashJeonseAndMonthlyRent
        }
        HousingLeaseCapability::Unavailable => HousingLeaseCapabilitySnapshot::Unavailable,
    }
}

const fn to_housing_lease_renewal_rule_snapshot(
    renewal_rule: HousingLeaseRenewalRule,
) -> HousingLeaseRenewalRuleSnapshot {
    match renewal_rule {
        HousingLeaseRenewalRule::FixedTermAutoRenew => {
            HousingLeaseRenewalRuleSnapshot::FixedTermAutoRenew
        }
        HousingLeaseRenewalRule::OpenEnded => HousingLeaseRenewalRuleSnapshot::OpenEnded,
    }
}

const fn to_housing_lease_termination_review_rule_snapshot(
    rule: HousingLeaseTerminationReviewRule,
) -> HousingLeaseTerminationReviewRuleSnapshot {
    match rule {
        HousingLeaseTerminationReviewRule::OldestActiveArrearAge => {
            HousingLeaseTerminationReviewRuleSnapshot::OldestActiveArrearAge
        }
    }
}

const fn to_housing_lease_role_snapshot(role: HousingLeaseRole) -> HousingLeaseRoleSnapshot {
    match role {
        HousingLeaseRole::Tenant => HousingLeaseRoleSnapshot::Tenant,
    }
}

const fn to_housing_lease_offer_kind_snapshot(
    offer_kind: HousingLeaseOfferKind,
) -> HousingLeaseOfferKindSnapshot {
    match offer_kind {
        HousingLeaseOfferKind::Jeonse => HousingLeaseOfferKindSnapshot::Jeonse,
        HousingLeaseOfferKind::MonthlyRent => HousingLeaseOfferKindSnapshot::MonthlyRent,
    }
}

const fn to_housing_rent_charge_rule_snapshot(
    rule: HousingRentChargeRule,
) -> HousingRentChargeRuleSnapshot {
    match rule {
        HousingRentChargeRule::NextMonthStartFull => {
            HousingRentChargeRuleSnapshot::NextMonthStartFull
        }
    }
}

const fn to_housing_lease_arrear_repayment_rule_snapshot(
    rule: HousingLeaseArrearRepaymentRule,
) -> HousingLeaseArrearRepaymentRuleSnapshot {
    match rule {
        HousingLeaseArrearRepaymentRule::ManualOnly => {
            HousingLeaseArrearRepaymentRuleSnapshot::ManualOnly
        }
    }
}

fn to_monthly_rent_terms_snapshot(terms: MonthlyRentTermsState) -> MonthlyRentTermsSnapshot {
    MonthlyRentTermsSnapshot {
        rent_charge_rule: to_housing_rent_charge_rule_snapshot(terms.rent_charge_rule),
        arrear_repayment_rule: to_housing_lease_arrear_repayment_rule_snapshot(
            terms.arrear_repayment_rule,
        ),
    }
}

fn to_monthly_rent_termination_review_terms_snapshot(
    terms: MonthlyRentTerminationReviewTermsState,
) -> MonthlyRentTerminationReviewTermsSnapshot {
    MonthlyRentTerminationReviewTermsSnapshot {
        rule: to_housing_lease_termination_review_rule_snapshot(terms.rule),
        after_game_days: terms.after_game_days,
    }
}

fn to_lease_lifecycle_terms_snapshot(
    terms: LeaseLifecycleTermsState,
) -> LeaseLifecycleTermsSnapshot {
    LeaseLifecycleTermsSnapshot {
        term_months: terms.term_months,
        renewal_notice_lead_days: terms.renewal_notice_lead_days,
        monthly_rent_termination_review: terms
            .monthly_rent_termination_review
            .map(to_monthly_rent_termination_review_terms_snapshot),
    }
}

fn to_active_lease_term_snapshot(term: ActiveLeaseTermState) -> ActiveLeaseTermSnapshot {
    ActiveLeaseTermSnapshot {
        term_no: term.term_no,
        effective_from_game_day: term.effective_from_game_day,
        effective_to_game_day: term.effective_to_game_day,
    }
}

fn to_lease_renewal_notice_snapshot(notice: LeaseRenewalNoticeState) -> LeaseRenewalNoticeSnapshot {
    LeaseRenewalNoticeSnapshot {
        term_no: notice.term_no,
        published_game_day: notice.published_game_day,
        renews_on_game_day: notice.renews_on_game_day,
    }
}

const fn to_lease_termination_review_status_snapshot(
    status: LeaseTerminationReviewStatusState,
) -> LeaseTerminationReviewStatusSnapshot {
    match status {
        LeaseTerminationReviewStatusState::UnderReview => {
            LeaseTerminationReviewStatusSnapshot::UnderReview
        }
    }
}

fn to_lease_termination_review_snapshot(
    review: LeaseTerminationReviewState,
) -> LeaseTerminationReviewSnapshot {
    LeaseTerminationReviewSnapshot {
        status: to_lease_termination_review_status_snapshot(review.status),
        opened_game_day: review.opened_game_day,
        trigger_arrear_id: review.trigger_arrear_id,
        active_lease_arrear_krw: review.active_lease_arrear_krw,
    }
}

fn to_lease_arrear_snapshot(arrear: &LeaseArrearState) -> LeaseArrearSnapshot {
    LeaseArrearSnapshot {
        id: arrear.id,
        lease_id: arrear.lease_id,
        rent_charge_id: arrear.rent_charge_id,
        due_year_month: to_year_month_snapshot(arrear.due_year_month),
        original_krw: arrear.original_krw,
        paid_krw: arrear.paid_krw,
        remaining_krw: arrear.remaining_krw,
        created_game_day: arrear.created_game_day,
    }
}

fn to_housing_moving_cost_snapshot(
    moving_cost: HousingMovingCostState,
) -> HousingMovingCostSnapshot {
    HousingMovingCostSnapshot {
        region_key: to_housing_region_key_snapshot(moving_cost.region_key),
        moving_cost_krw: moving_cost.moving_cost_krw,
    }
}

fn to_active_housing_lease_snapshot(lease: &ActiveHousingLeaseState) -> ActiveHousingLeaseSnapshot {
    ActiveHousingLeaseSnapshot {
        id: lease.id,
        listing_id: lease.listing_id,
        role: to_housing_lease_role_snapshot(lease.role),
        offer_kind: to_housing_lease_offer_kind_snapshot(lease.offer_kind),
        region_key: to_housing_region_key_snapshot(lease.region_key),
        property_type: to_housing_property_type_snapshot(lease.property_type),
        exclusive_area_square_meters: lease.exclusive_area_square_meters,
        deposit_krw: lease.deposit_krw,
        monthly_rent_krw: lease.monthly_rent_krw,
        next_rent_due_game_day: lease.next_rent_due_game_day,
        effective_from_game_day: lease.effective_from_game_day,
        effective_to_game_day: lease.effective_to_game_day,
        renewal_rule: to_housing_lease_renewal_rule_snapshot(lease.renewal_rule),
        current_term: lease.current_term.map(to_active_lease_term_snapshot),
        renewal_notice: lease.renewal_notice.map(to_lease_renewal_notice_snapshot),
        termination_review: lease
            .termination_review
            .map(to_lease_termination_review_snapshot),
        deposit_loan_id: lease.deposit_loan_id,
    }
}

fn to_housing_lease_current_response(
    current: HousingLeaseCurrentState,
) -> HousingLeaseCurrentResponse {
    HousingLeaseCurrentResponse {
        lease_capability: to_housing_lease_capability_snapshot(current.lease_capability),
        renewal_rule: current
            .renewal_rule
            .map(to_housing_lease_renewal_rule_snapshot),
        lease_lifecycle_terms: current
            .lease_lifecycle_terms
            .map(to_lease_lifecycle_terms_snapshot),
        moving_costs: current
            .moving_costs
            .into_iter()
            .map(to_housing_moving_cost_snapshot)
            .collect(),
        tenant_lease_deposit_krw: current.tenant_lease_deposit_krw,
        active_lease: current
            .active_lease
            .as_ref()
            .map(to_active_housing_lease_snapshot),
        monthly_rent_terms: current
            .monthly_rent_terms
            .map(to_monthly_rent_terms_snapshot),
        active_arrears: current
            .active_arrears
            .iter()
            .map(to_lease_arrear_snapshot)
            .collect(),
        has_more_active_arrears: current.has_more_active_arrears,
        total_lease_arrear_krw: current.total_lease_arrear_krw,
    }
}

fn to_housing_lease_move_response(
    receipt: HousingLeaseMoveReceipt,
    snapshot: GameSnapshot,
) -> HousingLeaseMoveResponse {
    HousingLeaseMoveResponse {
        result: HousingLeaseMoveResultSnapshot {
            lease_id: receipt.lease_id,
            residence_id: receipt.residence_id,
            listing_id: receipt.listing_id,
            offer_kind: to_housing_lease_offer_kind_snapshot(receipt.offer_kind),
            region_key: to_housing_region_key_snapshot(receipt.region_key),
            property_type: to_housing_property_type_snapshot(receipt.property_type),
            exclusive_area_square_meters: receipt.exclusive_area_square_meters,
            deposit_krw: receipt.deposit_krw,
            monthly_rent_krw: receipt.monthly_rent_krw,
            returned_deposit_krw: receipt.returned_deposit_krw,
            moving_cost_krw: receipt.moving_cost_krw,
            wallet_delta_krw: receipt.wallet_delta_krw,
            effective_from_game_day: receipt.effective_from_game_day,
            ended_lease_id: receipt.ended_lease_id,
            renewal_rule: to_housing_lease_renewal_rule_snapshot(receipt.renewal_rule),
            deposit_loan_execution: receipt
                .deposit_loan_execution
                .map(to_deposit_loan_execution_snapshot),
            repaid_deposit_loan: receipt
                .repaid_deposit_loan
                .map(to_repaid_deposit_loan_snapshot),
        },
        replayed: receipt.replayed,
        snapshot,
    }
}

fn to_deposit_loan_execution_snapshot(
    receipt: DepositLoanExecutionReceipt,
) -> DepositLoanExecutionSnapshot {
    DepositLoanExecutionSnapshot {
        loan_id: receipt.loan_id,
        quote_id: receipt.quote_id,
        product_version_id: receipt.product_version_id,
        principal_krw: receipt.principal_krw,
        annual_rate_bp: receipt.annual_rate_bp,
        maturity_game_day: receipt.maturity_game_day,
        first_installment: to_loan_quote_first_installment_snapshot(receipt.first_installment),
    }
}

fn to_repaid_deposit_loan_snapshot(receipt: RepaidDepositLoanReceipt) -> RepaidDepositLoanSnapshot {
    RepaidDepositLoanSnapshot {
        loan_id: receipt.loan_id,
        payment_id: receipt.payment_id,
        principal_krw: receipt.principal_krw,
    }
}

const fn to_housing_purchase_capability_snapshot(
    capability: HousingPurchaseCapabilityState,
) -> HousingPurchaseCapabilitySnapshot {
    match capability {
        HousingPurchaseCapabilityState::OwnerOccupiedSingleHome => {
            HousingPurchaseCapabilitySnapshot::OwnerOccupiedSingleHome
        }
        HousingPurchaseCapabilityState::Unavailable => {
            HousingPurchaseCapabilitySnapshot::Unavailable
        }
    }
}

fn to_property_holding_snapshot(state: &PropertyHoldingState) -> Result<PropertyHoldingSnapshot> {
    ensure!(
        state.status == PropertyHoldingStatusState::Active,
        "public property windows may contain only active holdings"
    );
    let purpose = match state.purpose {
        PropertyHoldingPurposeState::OwnerOccupied => PropertyHoldingPurposeSnapshot::OwnerOccupied,
    };
    ensure!(
        state.acquisition_price_krw > 0
            && state.acquisition_incidental_cost_krw > 0
            && state.book_value_krw == state.acquisition_price_krw,
        "property holding amounts are inconsistent"
    );
    Ok(PropertyHoldingSnapshot {
        id: state.id,
        listing_id: state.listing_id,
        status: PropertyHoldingStatusSnapshot::Active,
        purpose,
        region_key: to_housing_region_key_snapshot(state.region_key),
        property_type: to_housing_property_type_snapshot(state.property_type),
        exclusive_area_square_meters: state.exclusive_area_square_meters,
        acquired_game_day: state.acquired_game_day,
        acquisition_price_krw: state.acquisition_price_krw,
        acquisition_incidental_cost_krw: state.acquisition_incidental_cost_krw,
        book_value_krw: state.book_value_krw,
        mortgage_loan_id: state.mortgage_loan_id,
    })
}

fn to_housing_property_holdings_response(
    state: HousingPropertyHoldingsState,
) -> Result<HousingPropertyHoldingsResponse> {
    let holdings = state
        .holdings
        .iter()
        .map(to_property_holding_snapshot)
        .collect::<Result<Vec<_>>>()?;
    let visible_total = holdings.iter().try_fold(0_i64, |total, holding| {
        total
            .checked_add(holding.book_value_krw)
            .context("property holding total overflowed")
    })?;
    match state.purchase_capability {
        HousingPurchaseCapabilityState::OwnerOccupiedSingleHome => ensure!(
            state.maximum_active_holdings == 1 && holdings.len() <= 1,
            "single-home capability has an invalid holding limit"
        ),
        HousingPurchaseCapabilityState::Unavailable => ensure!(
            state.maximum_active_holdings == 0 && holdings.is_empty(),
            "unavailable purchase capability exposes holdings"
        ),
    }
    ensure!(
        visible_total == state.total_property_book_value_krw,
        "property holdings do not reconcile with their public total"
    );

    Ok(HousingPropertyHoldingsResponse {
        purchase_capability: to_housing_purchase_capability_snapshot(state.purchase_capability),
        maximum_active_holdings: state.maximum_active_holdings,
        holdings,
        total_property_book_value_krw: state.total_property_book_value_krw,
    })
}

const fn to_mortgage_quote_decision_snapshot(
    decision: MortgageQuoteDecisionState,
) -> MortgageQuoteDecisionSnapshot {
    match decision {
        MortgageQuoteDecisionState::CreditRestricted => {
            MortgageQuoteDecisionSnapshot::CreditRestricted
        }
        MortgageQuoteDecisionState::PurchaseRestricted => {
            MortgageQuoteDecisionSnapshot::PurchaseRestricted
        }
        MortgageQuoteDecisionState::CollateralLimit => {
            MortgageQuoteDecisionSnapshot::CollateralLimit
        }
        MortgageQuoteDecisionState::IncomeUnavailable => {
            MortgageQuoteDecisionSnapshot::IncomeUnavailable
        }
        MortgageQuoteDecisionState::DebtServiceLimit => {
            MortgageQuoteDecisionSnapshot::DebtServiceLimit
        }
        MortgageQuoteDecisionState::InsufficientOwnFunds => {
            MortgageQuoteDecisionSnapshot::InsufficientOwnFunds
        }
        MortgageQuoteDecisionState::Eligible => MortgageQuoteDecisionSnapshot::Eligible,
    }
}

const fn to_mortgage_quote_reason_snapshot(
    reason: MortgageQuoteReasonState,
) -> MortgageQuoteReasonSnapshot {
    match reason {
        MortgageQuoteReasonState::InsolvencyRebuilding => {
            MortgageQuoteReasonSnapshot::InsolvencyRebuilding
        }
        MortgageQuoteReasonState::ActiveDefault => MortgageQuoteReasonSnapshot::ActiveDefault,
        MortgageQuoteReasonState::ActiveDelinquency => {
            MortgageQuoteReasonSnapshot::ActiveDelinquency
        }
        MortgageQuoteReasonState::ActiveRestructuring => {
            MortgageQuoteReasonSnapshot::ActiveRestructuring
        }
        MortgageQuoteReasonState::CreditBandRestricted => {
            MortgageQuoteReasonSnapshot::CreditBandRestricted
        }
        MortgageQuoteReasonState::ActiveLoanLimit => MortgageQuoteReasonSnapshot::ActiveLoanLimit,
        MortgageQuoteReasonState::ActiveHolding => MortgageQuoteReasonSnapshot::ActiveHolding,
        MortgageQuoteReasonState::ResidenceChangedToday => {
            MortgageQuoteReasonSnapshot::ResidenceChangedToday
        }
        MortgageQuoteReasonState::LeaseExitRestricted => {
            MortgageQuoteReasonSnapshot::LeaseExitRestricted
        }
        MortgageQuoteReasonState::CollateralLimit => MortgageQuoteReasonSnapshot::CollateralLimit,
        MortgageQuoteReasonState::IncomeUnavailable => {
            MortgageQuoteReasonSnapshot::IncomeUnavailable
        }
        MortgageQuoteReasonState::DebtServiceLimit => MortgageQuoteReasonSnapshot::DebtServiceLimit,
        MortgageQuoteReasonState::InsufficientOwnFunds => {
            MortgageQuoteReasonSnapshot::InsufficientOwnFunds
        }
        MortgageQuoteReasonState::Eligible => MortgageQuoteReasonSnapshot::Eligible,
    }
}

const fn to_mortgage_ltv_region_class_snapshot(
    region_class: MortgageLtvRegionClassState,
) -> MortgageLtvRegionClassSnapshot {
    match region_class {
        MortgageLtvRegionClassState::RegulatedCapitalProxy => {
            MortgageLtvRegionClassSnapshot::RegulatedCapitalProxy
        }
        MortgageLtvRegionClassState::NonRegulatedProxy => {
            MortgageLtvRegionClassSnapshot::NonRegulatedProxy
        }
    }
}

const fn to_mortgage_stress_treatment_snapshot(
    treatment: MortgageStressTreatmentState,
) -> MortgageStressTreatmentSnapshot {
    match treatment {
        MortgageStressTreatmentState::FullTermFixed => {
            MortgageStressTreatmentSnapshot::FullTermFixed
        }
    }
}

const fn to_mortgage_ltv_snapshot(state: LoanQuoteLtvState) -> MortgageLtvSnapshot {
    MortgageLtvSnapshot {
        numerator_krw: state.numerator_krw,
        denominator_krw: state.denominator_krw,
        ratio_ppm: state.ratio_ppm,
        limit_ppm: state.limit_ppm,
    }
}

fn to_mortgage_quote_response(
    receipt: MortgageQuoteReceipt,
    snapshot: GameSnapshot,
) -> Result<MortgageQuoteResponse> {
    ensure!(
        receipt.created_game_day == receipt.expires_game_day
            && receipt.purchase_price_krw == receipt.recognized_collateral_value_krw
            && receipt.ltv.numerator_krw == receipt.requested_principal_krw
            && receipt.ltv.denominator_krw == receipt.recognized_collateral_value_krw
            && receipt.ltv.limit_ppm == receipt.ltv_limit_ppm
            && receipt.stress_rate_bp == 0,
        "mortgage quote evidence is inconsistent"
    );
    let ltv_ratio_ppm = i128::from(receipt.ltv.numerator_krw)
        .checked_mul(1_000_000)
        .and_then(|value| value.checked_div(i128::from(receipt.ltv.denominator_krw)))
        .context("mortgage LTV evidence is invalid")?;
    ensure!(
        ltv_ratio_ppm == i128::from(receipt.ltv.ratio_ppm),
        "mortgage LTV ratio is inconsistent"
    );
    let expected_post_execution_balance = i128::from(receipt.existing_loan_balance_krw)
        - i128::from(receipt.replaced_loan_principal_krw)
        + i128::from(receipt.requested_principal_krw);
    let expected_required_cash = (i128::from(receipt.purchase_price_krw)
        + i128::from(receipt.acquisition_incidental_cost_krw)
        + i128::from(receipt.moving_cost_krw)
        - i128::from(receipt.requested_principal_krw))
    .max(0);
    let dsr_shape_is_valid = receipt.dsr.as_ref().is_none_or(|dsr| {
        let expected_ratio_ppm = i128::from(dsr.numerator_krw)
            .checked_mul(1_000_000)
            .and_then(|value| value.checked_div(i128::from(dsr.denominator_krw)));
        receipt.dsr_applied
            && dsr.numerator_krw >= 0
            && dsr.denominator_krw > 0
            && dsr.limit_ppm >= 0
            && receipt.verified_annual_income_krw == Some(dsr.denominator_krw)
            && expected_ratio_ppm == Some(i128::from(dsr.ratio_ppm))
    });
    let dsr_decision_is_valid = match receipt.decision_code {
        MortgageQuoteDecisionState::IncomeUnavailable => {
            receipt.dsr_applied
                && receipt.verified_annual_income_krw.is_none()
                && receipt.dsr.is_none()
        }
        MortgageQuoteDecisionState::DebtServiceLimit => receipt
            .dsr
            .as_ref()
            .is_some_and(|dsr| dsr.ratio_ppm > dsr.limit_ppm),
        MortgageQuoteDecisionState::InsufficientOwnFunds | MortgageQuoteDecisionState::Eligible => {
            !receipt.dsr_applied
                || receipt
                    .dsr
                    .as_ref()
                    .is_some_and(|dsr| dsr.ratio_ppm <= dsr.limit_ppm)
        }
        MortgageQuoteDecisionState::CreditRestricted
        | MortgageQuoteDecisionState::PurchaseRestricted
        | MortgageQuoteDecisionState::CollateralLimit => true,
    };
    ensure!(
        expected_post_execution_balance == i128::from(receipt.post_execution_balance_krw)
            && expected_required_cash == i128::from(receipt.required_buyer_cash_krw)
            && receipt.decision_reasons.len() <= 8
            && !receipt.decision_reasons.is_empty()
            && receipt.replaced_loan_id.is_some() == (receipt.replaced_loan_principal_krw > 0)
            && receipt.verified_annual_income_krw.is_some()
                == receipt.verified_income_source.is_some()
            && (receipt.dsr_applied || receipt.dsr.is_none())
            && dsr_shape_is_valid
            && dsr_decision_is_valid,
        "mortgage quote affordability evidence is inconsistent"
    );

    Ok(MortgageQuoteResponse {
        result: MortgageQuoteResultSnapshot {
            quote_id: receipt.quote_id,
            listing_id: receipt.listing_id,
            product_version_id: receipt.product_version_id,
            requested_principal_krw: receipt.requested_principal_krw,
            purchase_price_krw: receipt.purchase_price_krw,
            recognized_collateral_value_krw: receipt.recognized_collateral_value_krw,
            ltv_region_class: to_mortgage_ltv_region_class_snapshot(receipt.ltv_region_class),
            ltv_limit_ppm: receipt.ltv_limit_ppm,
            maximum_mortgage_krw: receipt.maximum_mortgage_krw,
            ltv: to_mortgage_ltv_snapshot(receipt.ltv),
            created_game_day: receipt.created_game_day,
            expires_game_day: receipt.expires_game_day,
            decision_code: to_mortgage_quote_decision_snapshot(receipt.decision_code),
            decision_reasons: receipt
                .decision_reasons
                .into_iter()
                .map(to_mortgage_quote_reason_snapshot)
                .collect(),
            verified_annual_income_krw: receipt.verified_annual_income_krw,
            verified_income_source: receipt
                .verified_income_source
                .map(to_verified_income_source_snapshot),
            existing_loan_balance_krw: receipt.existing_loan_balance_krw,
            post_execution_balance_krw: receipt.post_execution_balance_krw,
            dsr_applied: receipt.dsr_applied,
            dsr: receipt.dsr.map(to_loan_quote_dsr_snapshot),
            stress_rate_bp: receipt.stress_rate_bp,
            stress_treatment: to_mortgage_stress_treatment_snapshot(receipt.stress_treatment),
            acquisition_incidental_cost_krw: receipt.acquisition_incidental_cost_krw,
            moving_cost_krw: receipt.moving_cost_krw,
            returned_deposit_krw: receipt.returned_deposit_krw,
            replaced_loan_id: receipt.replaced_loan_id,
            replaced_loan_principal_krw: receipt.replaced_loan_principal_krw,
            available_buyer_cash_krw: receipt.available_buyer_cash_krw,
            required_buyer_cash_krw: receipt.required_buyer_cash_krw,
            quoted_terms: to_loan_quoted_terms_snapshot(receipt.quoted_terms),
        },
        replayed: receipt.replayed,
        snapshot,
    })
}

fn to_mortgage_execution_snapshot(receipt: MortgageExecutionReceipt) -> MortgageExecutionSnapshot {
    MortgageExecutionSnapshot {
        loan_id: receipt.loan_id,
        quote_id: receipt.quote_id,
        product_version_id: receipt.product_version_id,
        property_holding_id: receipt.property_holding_id,
        principal_krw: receipt.principal_krw,
        activated_game_day: receipt.activated_game_day,
        maturity_game_day: receipt.maturity_game_day,
        annual_rate_bp: receipt.annual_rate_bp,
        repayment_method: to_loan_repayment_method_snapshot(receipt.repayment_method),
        term_months: receipt.term_months,
        first_installment: to_loan_quote_first_installment_snapshot(receipt.first_installment),
    }
}

fn to_property_purchase_response(
    receipt: PropertyPurchaseReceipt,
    snapshot: GameSnapshot,
) -> Result<PropertyPurchaseResponse> {
    let holding = to_property_holding_snapshot(&receipt.holding)?;
    ensure!(
        holding.listing_id == receipt.listing_id
            && holding.acquired_game_day == receipt.effective_from_game_day
            && holding.acquisition_price_krw == receipt.purchase_price_krw
            && holding.acquisition_incidental_cost_krw == receipt.acquisition_incidental_cost_krw,
        "property purchase receipt disagrees with its holding"
    );
    let mortgage_execution = receipt
        .mortgage_execution
        .map(to_mortgage_execution_snapshot);
    ensure!(
        mortgage_execution
            .as_ref()
            .map(|execution| execution.loan_id)
            == holding.mortgage_loan_id
            && mortgage_execution
                .as_ref()
                .is_none_or(|execution| execution.property_holding_id == holding.id),
        "property purchase mortgage disagrees with its lien"
    );
    let repaid_deposit_loan = receipt
        .repaid_deposit_loan
        .map(to_repaid_deposit_loan_snapshot);
    ensure!(
        (receipt.ended_lease_id.is_none() && receipt.returned_deposit_krw == 0)
            || (receipt.ended_lease_id.is_some() && receipt.returned_deposit_krw > 0),
        "property purchase lease exit evidence is inconsistent"
    );
    ensure!(
        receipt.ended_lease_id.is_some() || repaid_deposit_loan.is_none(),
        "property purchase repaid a lease loan without ending a lease"
    );
    let expected_wallet_delta = i128::from(receipt.returned_deposit_krw)
        - i128::from(
            repaid_deposit_loan
                .as_ref()
                .map_or(0, |loan| loan.principal_krw),
        )
        + i128::from(
            mortgage_execution
                .as_ref()
                .map_or(0, |execution| execution.principal_krw),
        )
        - i128::from(receipt.purchase_price_krw)
        - i128::from(receipt.acquisition_incidental_cost_krw)
        - i128::from(receipt.moving_cost_krw);
    ensure!(
        expected_wallet_delta == i128::from(receipt.wallet_delta_krw),
        "property purchase wallet delta is inconsistent"
    );
    ensure!(
        receipt.replayed
            || (snapshot
                .life
                .active_property_holdings
                .iter()
                .any(|current| current.id == holding.id)
                && snapshot.life.residence.as_ref().is_some_and(|residence| {
                    residence.id == receipt.residence_id
                        && residence.property_holding_id == Some(holding.id)
                        && matches!(residence.tenure_kind, ResidenceTenureKindSnapshot::Owner)
                })
                && snapshot.life.active_lease.is_none()
                && snapshot.life.tenant_lease_deposit_krw == 0),
        "new property purchase snapshot does not expose the acquired owner residence"
    );

    Ok(PropertyPurchaseResponse {
        result: PropertyPurchaseResultSnapshot {
            holding,
            residence_id: receipt.residence_id,
            listing_id: receipt.listing_id,
            purchase_price_krw: receipt.purchase_price_krw,
            acquisition_incidental_cost_krw: receipt.acquisition_incidental_cost_krw,
            moving_cost_krw: receipt.moving_cost_krw,
            returned_deposit_krw: receipt.returned_deposit_krw,
            wallet_delta_krw: receipt.wallet_delta_krw,
            effective_from_game_day: receipt.effective_from_game_day,
            ended_lease_id: receipt.ended_lease_id,
            repaid_deposit_loan,
            mortgage_execution,
        },
        replayed: receipt.replayed,
        snapshot,
    })
}

const fn to_property_sale_order_status_snapshot(
    status: PropertySaleOrderStatusState,
) -> PropertySaleOrderStatusSnapshot {
    match status {
        PropertySaleOrderStatusState::Active => PropertySaleOrderStatusSnapshot::Active,
        PropertySaleOrderStatusState::Filled => PropertySaleOrderStatusSnapshot::Filled,
        PropertySaleOrderStatusState::Cancelled => PropertySaleOrderStatusSnapshot::Cancelled,
        PropertySaleOrderStatusState::Rejected => PropertySaleOrderStatusSnapshot::Rejected,
    }
}

const fn to_property_sale_order_revision_kind_snapshot(
    kind: PropertySaleOrderRevisionKindState,
) -> PropertySaleOrderRevisionKindSnapshot {
    match kind {
        PropertySaleOrderRevisionKindState::Listing => {
            PropertySaleOrderRevisionKindSnapshot::Listing
        }
        PropertySaleOrderRevisionKindState::Cancellation => {
            PropertySaleOrderRevisionKindSnapshot::Cancellation
        }
    }
}

const fn to_property_sale_order_rejection_reason_snapshot(
    reason: PropertySaleOrderRejectionReasonState,
) -> PropertySaleOrderRejectionReasonSnapshot {
    match reason {
        PropertySaleOrderRejectionReasonState::MortgageNotPayable => {
            PropertySaleOrderRejectionReasonSnapshot::MortgageNotPayable
        }
        PropertySaleOrderRejectionReasonState::InsufficientProceeds => {
            PropertySaleOrderRejectionReasonSnapshot::InsufficientProceeds
        }
        PropertySaleOrderRejectionReasonState::PolicyUnsupported => {
            PropertySaleOrderRejectionReasonSnapshot::PolicyUnsupported
        }
    }
}

fn validate_property_sale_listing_values(
    asking_price_krw: i64,
    reference_value_krw: i64,
    asking_to_reference_ppm: i64,
) -> Result<()> {
    ensure!(
        asking_price_krw > 0
            && reference_value_krw > 0
            && (800_000..=1_200_000).contains(&asking_to_reference_ppm),
        "property sale listing values are outside the public range"
    );
    let expected_ratio = i128::from(asking_price_krw)
        .checked_mul(1_000_000)
        .and_then(|value| value.checked_div(i128::from(reference_value_krw)))
        .context("property sale listing ratio overflowed")?;
    ensure!(
        expected_ratio == i128::from(asking_to_reference_ppm),
        "property sale listing ratio is inconsistent"
    );
    Ok(())
}

fn to_property_sale_order_listing_response(
    receipt: PropertySaleOrderListingReceipt,
    snapshot: GameSnapshot,
) -> Result<PropertySaleOrderListingResponse> {
    ensure!(
        receipt.revision_no > 0
            && (receipt.replayed || receipt.candidate_game_day > snapshot.game_day)
            && receipt.status == PropertySaleOrderStatusState::Active,
        "new property sale order receipt has an invalid state"
    );
    validate_property_sale_listing_values(
        receipt.asking_price_krw,
        receipt.reference_value_krw,
        receipt.asking_to_reference_ppm,
    )?;

    Ok(PropertySaleOrderListingResponse {
        result: PropertySaleOrderListingResultSnapshot {
            order_id: receipt.order_id,
            holding_id: receipt.holding_id,
            revision_no: receipt.revision_no,
            asking_price_krw: receipt.asking_price_krw,
            reference_value_krw: receipt.reference_value_krw,
            asking_to_reference_ppm: receipt.asking_to_reference_ppm,
            candidate_game_day: receipt.candidate_game_day,
            status: to_property_sale_order_status_snapshot(receipt.status),
        },
        replayed: receipt.replayed,
        snapshot,
    })
}

fn to_property_sale_order_cancellation_response(
    receipt: PropertySaleOrderCancellationReceipt,
    snapshot: GameSnapshot,
) -> Result<PropertySaleOrderCancellationResponse> {
    ensure!(
        receipt.revision_no > 0
            && receipt.status == PropertySaleOrderStatusState::Cancelled
            && (receipt.replayed || receipt.cancelled_game_day == snapshot.game_day),
        "property sale cancellation receipt has an invalid state"
    );
    Ok(PropertySaleOrderCancellationResponse {
        result: PropertySaleOrderCancellationResultSnapshot {
            order_id: receipt.order_id,
            holding_id: receipt.holding_id,
            revision_no: receipt.revision_no,
            cancelled_game_day: receipt.cancelled_game_day,
            status: to_property_sale_order_status_snapshot(receipt.status),
        },
        replayed: receipt.replayed,
        snapshot,
    })
}

fn to_property_sale_execution_snapshot(
    state: PropertySaleExecutionState,
) -> Result<PropertySaleExecutionSnapshot> {
    let expected_wallet = i128::from(state.gross_sale_price_krw)
        - i128::from(state.transaction_cost_krw)
        - i128::from(state.mortgage_principal_krw)
        - i128::from(state.mortgage_fee_krw)
        - i128::from(state.capital_gains_tax_krw);
    ensure!(
        state.gross_sale_price_krw > 0
            && state.transaction_cost_krw > 0
            && state.mortgage_principal_krw >= 0
            && state.mortgage_fee_krw >= 0
            && state.capital_gains_tax_krw >= 0
            && state.wallet_proceeds_krw >= 0
            && expected_wallet == i128::from(state.wallet_proceeds_krw),
        "property sale execution waterfall is inconsistent"
    );
    Ok(PropertySaleExecutionSnapshot {
        filled_game_day: state.filled_game_day,
        gross_sale_price_krw: state.gross_sale_price_krw,
        transaction_cost_krw: state.transaction_cost_krw,
        mortgage_principal_krw: state.mortgage_principal_krw,
        mortgage_fee_krw: state.mortgage_fee_krw,
        capital_gains_tax_krw: state.capital_gains_tax_krw,
        wallet_proceeds_krw: state.wallet_proceeds_krw,
        realized_gain_loss_krw: state.realized_gain_loss_krw,
    })
}

fn to_property_sale_order_summary_snapshot(
    state: PropertySaleOrderSummaryState,
) -> Result<PropertySaleOrderSummarySnapshot> {
    ensure!(
        state.revision_no > 0,
        "property sale order revision is zero"
    );
    let listing_fields = (
        state.asking_price_krw,
        state.reference_value_krw,
        state.asking_to_reference_ppm,
        state.candidate_game_day,
    );
    match state.revision_kind {
        PropertySaleOrderRevisionKindState::Listing => {
            let (Some(asking), Some(reference), Some(ratio), Some(_)) = listing_fields else {
                bail!("property sale listing revision is incomplete");
            };
            validate_property_sale_listing_values(asking, reference, ratio)?;
        }
        PropertySaleOrderRevisionKindState::Cancellation => ensure!(
            listing_fields == (None, None, None, None),
            "property sale cancellation revision exposes listing values"
        ),
    }
    let shape_is_valid = match state.status {
        PropertySaleOrderStatusState::Active => {
            state.revision_kind == PropertySaleOrderRevisionKindState::Listing
                && state.cancelled_game_day.is_none()
                && state.rejection_reason.is_none()
                && state.execution.is_none()
        }
        PropertySaleOrderStatusState::Filled => {
            state.revision_kind == PropertySaleOrderRevisionKindState::Listing
                && state.cancelled_game_day.is_none()
                && state.rejection_reason.is_none()
                && state.execution.is_some()
        }
        PropertySaleOrderStatusState::Cancelled => {
            state.revision_kind == PropertySaleOrderRevisionKindState::Cancellation
                && state.cancelled_game_day.is_some()
                && state.rejection_reason.is_none()
                && state.execution.is_none()
        }
        PropertySaleOrderStatusState::Rejected => {
            state.revision_kind == PropertySaleOrderRevisionKindState::Listing
                && state.cancelled_game_day.is_none()
                && state.rejection_reason.is_some()
                && state.execution.is_none()
        }
    };
    ensure!(
        shape_is_valid,
        "property sale order status shape is inconsistent"
    );
    let execution = state
        .execution
        .map(to_property_sale_execution_snapshot)
        .transpose()?;
    if let (Some(candidate_game_day), Some(execution)) = (state.candidate_game_day, &execution) {
        ensure!(
            execution.filled_game_day == candidate_game_day
                && state.asking_price_krw == Some(execution.gross_sale_price_krw),
            "property sale execution disagrees with the accepted revision"
        );
    }

    Ok(PropertySaleOrderSummarySnapshot {
        order_id: state.order_id,
        holding_id: state.holding_id,
        revision_no: state.revision_no,
        revision_kind: to_property_sale_order_revision_kind_snapshot(state.revision_kind),
        asking_price_krw: state.asking_price_krw,
        reference_value_krw: state.reference_value_krw,
        asking_to_reference_ppm: state.asking_to_reference_ppm,
        candidate_game_day: state.candidate_game_day,
        status: to_property_sale_order_status_snapshot(state.status),
        cancelled_game_day: state.cancelled_game_day,
        rejection_reason: state
            .rejection_reason
            .map(to_property_sale_order_rejection_reason_snapshot),
        execution,
    })
}

fn to_property_sale_orders_response(
    state: PropertySaleOrderPageState,
) -> Result<PropertySaleOrdersResponse> {
    ensure!(
        state.items.len() <= 20,
        "property sale order page is unbounded"
    );
    let items = state
        .items
        .into_iter()
        .map(to_property_sale_order_summary_snapshot)
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        items
            .windows(2)
            .all(|pair| pair[0].order_id > pair[1].order_id),
        "property sale order page is not in descending ID order"
    );
    ensure!(
        state
            .next_before
            .is_none_or(|cursor| items.last().is_some_and(|item| item.order_id == cursor)),
        "property sale order cursor does not match the oldest item"
    );
    Ok(PropertySaleOrdersResponse {
        items,
        next_before: state.next_before,
    })
}

const fn to_property_tax_event_kind_snapshot(
    kind: PropertyTaxEventKindState,
) -> PropertyTaxEventKindSnapshot {
    match kind {
        PropertyTaxEventKindState::Acquisition => PropertyTaxEventKindSnapshot::Acquisition,
        PropertyTaxEventKindState::AnnualHolding => PropertyTaxEventKindSnapshot::AnnualHolding,
        PropertyTaxEventKindState::CapitalGains => PropertyTaxEventKindSnapshot::CapitalGains,
    }
}

const fn to_property_tax_event_status_snapshot(
    status: PropertyTaxEventStatusState,
) -> PropertyTaxEventStatusSnapshot {
    match status {
        PropertyTaxEventStatusState::Scheduled => PropertyTaxEventStatusSnapshot::Scheduled,
        PropertyTaxEventStatusState::PartiallyPaid => PropertyTaxEventStatusSnapshot::PartiallyPaid,
        PropertyTaxEventStatusState::Paid => PropertyTaxEventStatusSnapshot::Paid,
        PropertyTaxEventStatusState::NoPaymentRequired => {
            PropertyTaxEventStatusSnapshot::NoPaymentRequired
        }
    }
}

const fn to_property_tax_payment_status_snapshot(
    status: PropertyTaxPaymentStatusState,
) -> PropertyTaxPaymentStatusSnapshot {
    match status {
        PropertyTaxPaymentStatusState::Pending => PropertyTaxPaymentStatusSnapshot::Pending,
        PropertyTaxPaymentStatusState::Applied => PropertyTaxPaymentStatusSnapshot::Applied,
        PropertyTaxPaymentStatusState::Cancelled => PropertyTaxPaymentStatusSnapshot::Cancelled,
    }
}

fn to_property_tax_component_snapshot(
    state: PropertyTaxComponentState,
) -> Result<PropertyTaxComponentSnapshot> {
    ensure!(
        !state.component_key.is_empty()
            && state.tax_base_krw >= 0
            && state.deduction_krw >= 0
            && state.taxable_amount_krw >= 0
            && (0..=1_000_000).contains(&state.rate_ppm)
            && state.progressive_deduction_krw >= 0
            && state.amount_krw >= 0,
        "property tax component contains invalid public values"
    );
    Ok(PropertyTaxComponentSnapshot {
        component_key: state.component_key,
        component_order: state.component_order,
        tax_base_krw: state.tax_base_krw,
        deduction_krw: state.deduction_krw,
        taxable_amount_krw: state.taxable_amount_krw,
        rate_ppm: state.rate_ppm,
        progressive_deduction_krw: state.progressive_deduction_krw,
        amount_krw: state.amount_krw,
    })
}

fn to_property_tax_payment_snapshot(
    state: PropertyTaxPaymentState,
) -> Result<PropertyTaxPaymentSnapshot> {
    let funded_krw = i128::from(state.wallet_paid_krw) + i128::from(state.tax_obligation_krw);
    let status_shape_is_valid = match state.status {
        PropertyTaxPaymentStatusState::Pending | PropertyTaxPaymentStatusState::Cancelled => {
            state.paid_game_day.is_none() && funded_krw == 0
        }
        PropertyTaxPaymentStatusState::Applied => {
            state.paid_game_day.is_some() && funded_krw == i128::from(state.amount_krw)
        }
    };
    ensure!(
        state.payment_no > 0
            && state.amount_krw >= 0
            && state.wallet_paid_krw >= 0
            && state.tax_obligation_krw >= 0
            && status_shape_is_valid,
        "property tax payment contains invalid public values"
    );
    Ok(PropertyTaxPaymentSnapshot {
        payment_no: state.payment_no,
        due_game_day: state.due_game_day,
        paid_game_day: state.paid_game_day,
        status: to_property_tax_payment_status_snapshot(state.status),
        amount_krw: state.amount_krw,
        wallet_paid_krw: state.wallet_paid_krw,
        tax_obligation_krw: state.tax_obligation_krw,
    })
}

fn to_property_tax_event_snapshot(
    state: PropertyTaxEventState,
) -> Result<PropertyTaxEventSnapshot> {
    ensure!(
        !state.policy_key.is_empty()
            && !state.rule_key.is_empty()
            && !state.legal_basis_date.is_empty()
            && state.household_home_count > 0
            && state.gross_amount_krw >= 0
            && state.tax_base_krw >= 0
            && state.deduction_krw >= 0
            && state.taxable_amount_krw >= 0
            && state.total_tax_krw >= 0
            && state.components.len() <= 16
            && state.payments.len() <= 2
            && state.exclusion_codes.len() <= 16,
        "property tax event contains invalid public values"
    );
    let valuation_shape_is_valid = match state.kind {
        PropertyTaxEventKindState::AnnualHolding => {
            state.valuation_game_day.is_some()
                && state
                    .valuation_price_index_ppm
                    .is_some_and(|value| value > 0)
                && state.official_value_krw.is_some_and(|value| value >= 0)
        }
        PropertyTaxEventKindState::Acquisition | PropertyTaxEventKindState::CapitalGains => {
            state.valuation_game_day.is_some()
                && state
                    .valuation_price_index_ppm
                    .is_some_and(|value| value > 0)
                && state.official_value_krw.is_none()
        }
    };
    ensure!(
        valuation_shape_is_valid,
        "property tax valuation evidence is inconsistent"
    );
    let components = state
        .components
        .into_iter()
        .map(to_property_tax_component_snapshot)
        .collect::<Result<Vec<_>>>()?;
    let payments = state
        .payments
        .into_iter()
        .map(to_property_tax_payment_snapshot)
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        components
            .windows(2)
            .all(|pair| pair[0].component_order < pair[1].component_order)
            && payments
                .windows(2)
                .all(|pair| pair[0].payment_no < pair[1].payment_no),
        "property tax evidence is not in canonical order"
    );
    let component_total = components
        .iter()
        .try_fold(0_i128, |sum, component| {
            sum.checked_add(i128::from(component.amount_krw))
        })
        .context("property tax component total overflowed")?;
    let payment_total = payments.iter().try_fold(0_i128, |sum, payment| {
        sum.checked_add(i128::from(payment.amount_krw))
    });
    ensure!(
        component_total == i128::from(state.total_tax_krw)
            && payment_total == Some(i128::from(state.total_tax_krw))
            && (state.status == PropertyTaxEventStatusState::NoPaymentRequired)
                == (state.total_tax_krw == 0),
        "property tax totals do not reconcile"
    );

    Ok(PropertyTaxEventSnapshot {
        id: state.id,
        holding_id: state.holding_id,
        policy_set_id: state.policy_set_id,
        policy_key: state.policy_key,
        rule_id: state.rule_id,
        rule_key: state.rule_key,
        legal_basis_date: state.legal_basis_date,
        kind: to_property_tax_event_kind_snapshot(state.kind),
        status: to_property_tax_event_status_snapshot(state.status),
        assessed_game_day: state.assessed_game_day,
        taxable_game_day: state.taxable_game_day,
        paid_game_day: state.paid_game_day,
        household_home_count: state.household_home_count,
        gross_amount_krw: state.gross_amount_krw,
        valuation_game_day: state.valuation_game_day,
        valuation_price_index_ppm: state.valuation_price_index_ppm,
        official_value_krw: state.official_value_krw,
        tax_base_krw: state.tax_base_krw,
        deduction_krw: state.deduction_krw,
        taxable_amount_krw: state.taxable_amount_krw,
        total_tax_krw: state.total_tax_krw,
        components,
        payments,
        exclusion_codes: state.exclusion_codes,
    })
}

fn to_property_tax_events_response(
    state: PropertyTaxEventPageState,
) -> Result<PropertyTaxEventsResponse> {
    ensure!(
        state.items.len() <= 20,
        "property tax event page is unbounded"
    );
    ensure!(
        state
            .items
            .iter()
            .all(|event| event.holding_id == state.holding_id),
        "property tax event page mixes holdings"
    );
    let items = state
        .items
        .into_iter()
        .map(to_property_tax_event_snapshot)
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        items.windows(2).all(|pair| pair[0].id > pair[1].id),
        "property tax event page is not in descending ID order"
    );
    ensure!(
        state
            .next_before
            .is_none_or(|cursor| items.last().is_some_and(|item| item.id == cursor)),
        "property tax event cursor does not match the oldest item"
    );
    Ok(PropertyTaxEventsResponse {
        holding_id: state.holding_id,
        items,
        next_before: state.next_before,
    })
}

fn to_housing_offer_snapshot(offer: PropertyListingOffer) -> HousingOfferSnapshot {
    match offer {
        PropertyListingOffer::Sale { price_krw } => HousingOfferSnapshot::Sale { price_krw },
        PropertyListingOffer::Jeonse { deposit_krw } => {
            HousingOfferSnapshot::Jeonse { deposit_krw }
        }
        PropertyListingOffer::MonthlyRent {
            deposit_krw,
            monthly_rent_krw,
        } => HousingOfferSnapshot::MonthlyRent {
            deposit_krw,
            monthly_rent_krw,
        },
    }
}

fn to_housing_region_snapshot(region: HousingRegionState) -> HousingRegionSnapshot {
    HousingRegionSnapshot {
        region_key: to_housing_region_key_snapshot(region.region_key),
        display_name: region.display_name,
    }
}

fn to_housing_listing_snapshot(listing: HousingListingState) -> HousingListingSnapshot {
    HousingListingSnapshot {
        id: listing.id,
        region_key: to_housing_region_key_snapshot(listing.region_key),
        property_type: to_housing_property_type_snapshot(listing.property_type),
        exclusive_area_square_meters: listing.exclusive_area_square_meters,
        available_from_game_day: listing.available_from_game_day,
        available_to_game_day: listing.available_to_game_day,
        offers: listing
            .offers
            .into_iter()
            .map(to_housing_offer_snapshot)
            .collect(),
    }
}

fn to_housing_listings_response(state: HousingListingsState) -> HousingListingsResponse {
    HousingListingsResponse {
        rate_status: to_housing_rate_status_snapshot(state.rate_status),
        model_version_id: state.model_version_id,
        game_day: state.game_day,
        year_month: to_year_month_snapshot(state.year_month),
        residence_region_key: to_housing_region_key_snapshot(state.residence_region_key),
        selected_region_key: to_housing_region_key_snapshot(state.selected_region_key),
        regions: state
            .regions
            .into_iter()
            .map(to_housing_region_snapshot)
            .collect(),
        price_index_ppm: state.price_index_ppm,
        rent_index_ppm: state.rent_index_ppm,
        listings: state
            .listings
            .into_iter()
            .map(to_housing_listing_snapshot)
            .collect(),
    }
}

const fn to_credit_band_snapshot(band: CreditBand) -> CreditBandSnapshot {
    match band {
        CreditBand::Prime => CreditBandSnapshot::Prime,
        CreditBand::Standard => CreditBandSnapshot::Standard,
        CreditBand::Limited => CreditBandSnapshot::Limited,
        CreditBand::Distressed => CreditBandSnapshot::Distressed,
        CreditBand::Insolvent => CreditBandSnapshot::Insolvent,
    }
}

const fn to_credit_reason_snapshot(reason: CreditReasonState) -> CreditReasonSnapshot {
    match reason {
        CreditReasonState::ModelUnavailable => CreditReasonSnapshot::ModelUnavailable,
        CreditReasonState::ActiveDefault => CreditReasonSnapshot::ActiveDefault,
        CreditReasonState::ActiveDelinquency => CreditReasonSnapshot::ActiveDelinquency,
        CreditReasonState::CleanHistory => CreditReasonSnapshot::CleanHistory,
    }
}

const fn to_loan_product_kind_snapshot(kind: LoanProductKind) -> LoanProductKindSnapshot {
    match kind {
        LoanProductKind::StudentLoan => LoanProductKindSnapshot::StudentLoan,
        LoanProductKind::UnsecuredLoan => LoanProductKindSnapshot::UnsecuredLoan,
        LoanProductKind::LeaseDepositLoan => LoanProductKindSnapshot::LeaseDepositLoan,
        LoanProductKind::Mortgage => LoanProductKindSnapshot::Mortgage,
        LoanProductKind::LegacyDebt => LoanProductKindSnapshot::LegacyDebt,
    }
}

const fn to_loan_rate_status_snapshot(status: LoanRateStatus) -> LoanRateStatusSnapshot {
    match status {
        LoanRateStatus::Available => LoanRateStatusSnapshot::Available,
        LoanRateStatus::RateUnavailable => LoanRateStatusSnapshot::RateUnavailable,
    }
}

const fn to_loan_lender_sector_snapshot(sector: LoanLenderSector) -> LoanLenderSectorSnapshot {
    match sector {
        LoanLenderSector::Bank => LoanLenderSectorSnapshot::Bank,
        LoanLenderSector::NonBank => LoanLenderSectorSnapshot::NonBank,
    }
}

const fn to_loan_rate_type_snapshot(rate_type: LoanRateType) -> LoanRateTypeSnapshot {
    match rate_type {
        LoanRateType::Fixed => LoanRateTypeSnapshot::Fixed,
        LoanRateType::Variable => LoanRateTypeSnapshot::Variable,
    }
}

const fn to_loan_rate_reference_snapshot(
    reference: LoanRateReference,
) -> LoanRateReferenceSnapshot {
    match reference {
        LoanRateReference::Treasury3m => LoanRateReferenceSnapshot::Treasury3m,
    }
}

const fn to_loan_rate_reset_rule_snapshot(rule: LoanRateResetRule) -> LoanRateResetRuleSnapshot {
    match rule {
        LoanRateResetRule::None => LoanRateResetRuleSnapshot::None,
        LoanRateResetRule::MonthlyDay1 => LoanRateResetRuleSnapshot::MonthlyDay1,
    }
}

const fn to_loan_day_count_rule_snapshot(rule: LoanDayCountRule) -> LoanDayCountRuleSnapshot {
    match rule {
        LoanDayCountRule::Actual365 => LoanDayCountRuleSnapshot::Actual365,
    }
}

const fn to_loan_repayment_method_snapshot(
    method: LoanRepaymentMethod,
) -> LoanRepaymentMethodSnapshot {
    match method {
        LoanRepaymentMethod::EqualPrincipal => LoanRepaymentMethodSnapshot::EqualPrincipal,
        LoanRepaymentMethod::LevelPayment => LoanRepaymentMethodSnapshot::LevelPayment,
        LoanRepaymentMethod::Bullet => LoanRepaymentMethodSnapshot::Bullet,
    }
}

const fn to_loan_payment_calendar_snapshot(
    calendar: LoanPaymentCalendar,
) -> LoanPaymentCalendarSnapshot {
    match calendar {
        LoanPaymentCalendar::MonthEnd => LoanPaymentCalendarSnapshot::MonthEnd,
    }
}

const fn to_loan_prepayment_effect_snapshot(
    effect: LoanPrepaymentEffect,
) -> LoanPrepaymentEffectSnapshot {
    match effect {
        LoanPrepaymentEffect::ReduceTerm => LoanPrepaymentEffectSnapshot::ReduceTerm,
        LoanPrepaymentEffect::RecalculatePayment => {
            LoanPrepaymentEffectSnapshot::RecalculatePayment
        }
    }
}

const fn to_loan_product_provenance_snapshot(
    provenance: LoanProductProvenance,
) -> LoanProductProvenanceSnapshot {
    match provenance {
        LoanProductProvenance::GameBalance => LoanProductProvenanceSnapshot::GameBalance,
    }
}

const fn to_loan_contract_status_snapshot(
    status: LoanContractStatus,
) -> LoanContractStatusSnapshot {
    match status {
        LoanContractStatus::Pending => LoanContractStatusSnapshot::Pending,
        LoanContractStatus::Active => LoanContractStatusSnapshot::Active,
        LoanContractStatus::Delinquent => LoanContractStatusSnapshot::Delinquent,
        LoanContractStatus::Defaulted => LoanContractStatusSnapshot::Defaulted,
        LoanContractStatus::PaidOff => LoanContractStatusSnapshot::PaidOff,
        LoanContractStatus::Restructured => LoanContractStatusSnapshot::Restructured,
        LoanContractStatus::Discharged => LoanContractStatusSnapshot::Discharged,
        LoanContractStatus::ChargedOff => LoanContractStatusSnapshot::ChargedOff,
        LoanContractStatus::Cancelled => LoanContractStatusSnapshot::Cancelled,
    }
}

fn to_loan_summary_snapshot(state: &LoanSummaryState) -> LoanSummarySnapshot {
    LoanSummarySnapshot {
        id: state.id,
        product_version_id: state.product_version_id,
        product_kind: to_loan_product_kind_snapshot(state.product_kind),
        display_name: state.display_name.clone(),
        rate_status: to_loan_rate_status_snapshot(state.rate_status),
        current_annual_rate_bp: state.current_annual_rate_bp,
        status: to_loan_contract_status_snapshot(state.status),
        remaining_principal_krw: state.remaining_principal_krw,
        overdue_krw: state.overdue_krw,
        read_only: state.read_only,
    }
}

fn to_loan_detail_response(state: LoanDetailState) -> LoanDetailResponse {
    LoanDetailResponse {
        id: state.id,
        product_version_id: state.product_version_id,
        product_kind: to_loan_product_kind_snapshot(state.product_kind),
        display_name: state.display_name,
        rate_status: to_loan_rate_status_snapshot(state.rate_status),
        current_annual_rate_bp: state.current_annual_rate_bp,
        status: to_loan_contract_status_snapshot(state.status),
        read_only: state.read_only,
        original_principal_krw: state.original_principal_krw,
        remaining_principal_krw: state.remaining_principal_krw,
        accrued_interest_krw: state.accrued_interest_krw,
        accrued_fee_krw: state.accrued_fee_krw,
        overdue_krw: state.overdue_krw,
        repayment_method: to_loan_repayment_method_snapshot(state.repayment_method),
        term_months: state.term_months,
        total_installments: state.total_installments,
        activated_game_day: state.activated_game_day,
        maturity_game_day: state.maturity_game_day,
        final_installment_due_game_day: state.final_installment_due_game_day,
        next_installment_no: state.next_installment_no,
        oldest_unpaid_due_game_day: state.oldest_unpaid_due_game_day,
        prepayment_allowed: state.prepayment_allowed,
        prepayment_fee_ppm: state.prepayment_fee_ppm,
        prepayment_effect: state
            .prepayment_effect
            .map(to_loan_prepayment_effect_snapshot),
        dsr_included: state.dsr_included,
        lease_contract_id: state.lease_contract_id,
        property_holding_id: state.property_holding_id,
    }
}

fn to_loan_installment_snapshot(state: LoanInstallmentState) -> LoanInstallmentSnapshot {
    LoanInstallmentSnapshot {
        id: state.id,
        installment_no: state.installment_no,
        due_game_day: state.due_game_day,
        interest_period_start_game_day: state.interest_period_start_game_day,
        elapsed_days: state.elapsed_days,
        annual_rate_bp: state.annual_rate_bp,
        opening_principal_krw: state.opening_principal_krw,
        scheduled_fee_krw: state.scheduled_fee_krw,
        scheduled_interest_krw: state.scheduled_interest_krw,
        scheduled_principal_krw: state.scheduled_principal_krw,
        paid_fee_krw: state.paid_fee_krw,
        paid_interest_krw: state.paid_interest_krw,
        paid_principal_krw: state.paid_principal_krw,
        remaining_due_krw: state.remaining_due_krw,
        status: match state.status {
            LoanInstallmentStatusState::Pending => LoanInstallmentStatusSnapshot::Pending,
            LoanInstallmentStatusState::Due => LoanInstallmentStatusSnapshot::Due,
            LoanInstallmentStatusState::PartiallyPaid => {
                LoanInstallmentStatusSnapshot::PartiallyPaid
            }
            LoanInstallmentStatusState::Paid => LoanInstallmentStatusSnapshot::Paid,
            LoanInstallmentStatusState::Cancelled => LoanInstallmentStatusSnapshot::Cancelled,
            LoanInstallmentStatusState::Discharged => LoanInstallmentStatusSnapshot::Discharged,
        },
        schedule_revision: state.schedule_revision,
    }
}

const fn to_loan_payment_allocation_kind_snapshot(
    kind: LoanPaymentAllocationKindState,
) -> LoanPaymentAllocationKindSnapshot {
    match kind {
        LoanPaymentAllocationKindState::OverdueFee => LoanPaymentAllocationKindSnapshot::OverdueFee,
        LoanPaymentAllocationKindState::OverdueInterest => {
            LoanPaymentAllocationKindSnapshot::OverdueInterest
        }
        LoanPaymentAllocationKindState::OverduePrincipal => {
            LoanPaymentAllocationKindSnapshot::OverduePrincipal
        }
        LoanPaymentAllocationKindState::CurrentFee => LoanPaymentAllocationKindSnapshot::CurrentFee,
        LoanPaymentAllocationKindState::CurrentInterest => {
            LoanPaymentAllocationKindSnapshot::CurrentInterest
        }
        LoanPaymentAllocationKindState::CurrentPrincipal => {
            LoanPaymentAllocationKindSnapshot::CurrentPrincipal
        }
        LoanPaymentAllocationKindState::PrepaymentFee => {
            LoanPaymentAllocationKindSnapshot::PrepaymentFee
        }
        LoanPaymentAllocationKindState::PrepaymentPrincipal => {
            LoanPaymentAllocationKindSnapshot::PrepaymentPrincipal
        }
    }
}

fn to_loan_payment_allocation_snapshot(
    state: LoanPaymentAllocationState,
) -> LoanPaymentAllocationSnapshot {
    LoanPaymentAllocationSnapshot {
        kind: to_loan_payment_allocation_kind_snapshot(state.kind),
        amount_krw: state.amount_krw,
    }
}

fn to_loan_payment_snapshot(state: LoanPaymentState) -> LoanPaymentSnapshot {
    LoanPaymentSnapshot {
        id: state.id,
        payment_no: state.payment_no,
        kind: match state.kind {
            LoanPaymentKindState::ScheduledInstallment => {
                LoanPaymentKindSnapshot::ScheduledInstallment
            }
            LoanPaymentKindState::ManualPrepayment => LoanPaymentKindSnapshot::ManualPrepayment,
            LoanPaymentKindState::LeaseMovePayoff => LoanPaymentKindSnapshot::LeaseMovePayoff,
            LoanPaymentKindState::PropertySalePayoff => LoanPaymentKindSnapshot::PropertySalePayoff,
            LoanPaymentKindState::InsolvencyDistribution => {
                LoanPaymentKindSnapshot::InsolvencyDistribution
            }
        },
        game_day: state.game_day,
        amount_krw: state.amount_krw,
        allocations: state
            .allocations
            .into_iter()
            .map(to_loan_payment_allocation_snapshot)
            .collect(),
    }
}

fn to_loan_installments_response(state: LoanInstallmentPageState) -> LoanInstallmentsResponse {
    LoanInstallmentsResponse {
        loan_id: state.loan_id,
        installments: state
            .installments
            .into_iter()
            .map(to_loan_installment_snapshot)
            .collect(),
        payments: state
            .payments
            .into_iter()
            .map(to_loan_payment_snapshot)
            .collect(),
        has_more_installments: state.has_more_installments,
        has_more_payments: state.has_more_payments,
        next_before: state.next_before.map(|before| {
            format!(
                "v1.l{}.i{}.p{}",
                before.loan_id,
                before.installment_before.unwrap_or(0),
                before.payment_before.unwrap_or(0)
            )
        }),
    }
}

fn to_loan_product_snapshot(state: LoanProductState) -> LoanProductSnapshot {
    LoanProductSnapshot {
        id: state.id,
        key: state.key,
        display_name: state.display_name,
        kind: to_loan_product_kind_snapshot(state.kind),
        lender_sector: to_loan_lender_sector_snapshot(state.lender_sector),
        rate_status: to_loan_rate_status_snapshot(state.rate_status),
        rate_type: to_loan_rate_type_snapshot(state.rate_type),
        current_annual_rate_bp: state.current_annual_rate_bp,
        reference_rate_key: state
            .reference_rate_key
            .map(to_loan_rate_reference_snapshot),
        spread_bp: state.spread_bp,
        minimum_annual_rate_bp: state.minimum_annual_rate_bp,
        maximum_annual_rate_bp: state.maximum_annual_rate_bp,
        rate_reset_rule: to_loan_rate_reset_rule_snapshot(state.rate_reset_rule),
        day_count_rule: to_loan_day_count_rule_snapshot(state.day_count_rule),
        repayment_method: to_loan_repayment_method_snapshot(state.repayment_method),
        term_months: state.term_months,
        payment_calendar: to_loan_payment_calendar_snapshot(state.payment_calendar),
        grace_months: state.grace_months,
        minimum_principal_krw: state.minimum_principal_krw,
        maximum_principal_krw: state.maximum_principal_krw,
        prepayment_fee_ppm: state.prepayment_fee_ppm,
        prepayment_effect: to_loan_prepayment_effect_snapshot(state.prepayment_effect),
        starting_eligible: state.starting_eligible,
        quote_eligible: state.quote_eligible,
        execution_eligible: state.execution_eligible,
        prepayment_allowed: state.prepayment_allowed,
        dsr_included: state.dsr_included,
        provenance: to_loan_product_provenance_snapshot(state.provenance),
    }
}

fn to_loan_product_catalog_response(state: LoanProductCatalogState) -> LoanProductCatalogResponse {
    LoanProductCatalogResponse {
        credit_model_version_id: state.credit_model_version_id,
        products: state
            .products
            .into_iter()
            .map(to_loan_product_snapshot)
            .collect(),
    }
}

fn to_credit_response(state: CreditOverviewState) -> CreditResponse {
    CreditResponse {
        credit_band: state.credit_band.map(to_credit_band_snapshot),
        credit_reasons: state
            .credit_reasons
            .into_iter()
            .map(to_credit_reason_snapshot)
            .collect(),
        active_loans: state
            .active_loans
            .iter()
            .map(to_loan_summary_snapshot)
            .collect(),
        next_loan_installment: state
            .next_loan_installment
            .as_ref()
            .map(to_next_loan_installment_snapshot),
        total_loan_balance_krw: state.total_loan_balance_krw,
    }
}

const fn to_loan_quote_decision_snapshot(
    decision: LoanQuoteDecisionState,
) -> LoanQuoteDecisionSnapshot {
    match decision {
        LoanQuoteDecisionState::Eligible => LoanQuoteDecisionSnapshot::Eligible,
        LoanQuoteDecisionState::DebtServiceLimit => LoanQuoteDecisionSnapshot::DebtServiceLimit,
        LoanQuoteDecisionState::IncomeUnavailable => LoanQuoteDecisionSnapshot::IncomeUnavailable,
        LoanQuoteDecisionState::CreditRestricted => LoanQuoteDecisionSnapshot::CreditRestricted,
        LoanQuoteDecisionState::ValuationUnavailable => {
            LoanQuoteDecisionSnapshot::ValuationUnavailable
        }
    }
}

const fn to_loan_quote_reason_snapshot(reason: LoanQuoteReasonState) -> LoanQuoteReasonSnapshot {
    match reason {
        LoanQuoteReasonState::InsolvencyRebuilding => LoanQuoteReasonSnapshot::InsolvencyRebuilding,
        LoanQuoteReasonState::ActiveDefault => LoanQuoteReasonSnapshot::ActiveDefault,
        LoanQuoteReasonState::ActiveDelinquency => LoanQuoteReasonSnapshot::ActiveDelinquency,
        LoanQuoteReasonState::ActiveRestructuring => LoanQuoteReasonSnapshot::ActiveRestructuring,
        LoanQuoteReasonState::CreditBandRestricted => LoanQuoteReasonSnapshot::CreditBandRestricted,
        LoanQuoteReasonState::ActiveLoanLimit => LoanQuoteReasonSnapshot::ActiveLoanLimit,
        LoanQuoteReasonState::IncomeUnavailable => LoanQuoteReasonSnapshot::IncomeUnavailable,
        LoanQuoteReasonState::DebtServiceLimit => LoanQuoteReasonSnapshot::DebtServiceLimit,
        LoanQuoteReasonState::Eligible => LoanQuoteReasonSnapshot::Eligible,
    }
}

const fn to_verified_income_source_snapshot(
    source: VerifiedIncomeSourceState,
) -> VerifiedIncomeSourceSnapshot {
    match source {
        VerifiedIncomeSourceState::ActiveEmploymentContract => {
            VerifiedIncomeSourceSnapshot::ActiveEmploymentContract
        }
    }
}

const fn to_loan_quote_dsr_snapshot(state: LoanQuoteDsrState) -> LoanQuoteDsrSnapshot {
    LoanQuoteDsrSnapshot {
        numerator_krw: state.numerator_krw,
        denominator_krw: state.denominator_krw,
        ratio_ppm: state.ratio_ppm,
        limit_ppm: state.limit_ppm,
    }
}

const fn to_loan_quote_first_installment_snapshot(
    state: LoanQuoteFirstInstallmentState,
) -> LoanQuoteFirstInstallmentSnapshot {
    LoanQuoteFirstInstallmentSnapshot {
        due_game_day: state.due_game_day,
        fee_krw: state.fee_krw,
        principal_krw: state.principal_krw,
        interest_krw: state.interest_krw,
        total_krw: state.total_krw,
    }
}

const fn to_loan_quoted_terms_snapshot(state: LoanQuotedTermsState) -> LoanQuotedTermsSnapshot {
    LoanQuotedTermsSnapshot {
        annual_rate_bp: state.annual_rate_bp,
        repayment_method: to_loan_repayment_method_snapshot(state.repayment_method),
        term_months: state.term_months,
        first_installment: to_loan_quote_first_installment_snapshot(state.first_installment),
    }
}

fn to_loan_quote_response(receipt: LoanQuoteReceipt, snapshot: GameSnapshot) -> LoanQuoteResponse {
    LoanQuoteResponse {
        result: LoanQuoteResultSnapshot {
            quote_id: receipt.quote_id,
            product_version_id: receipt.product_version_id,
            requested_principal_krw: receipt.requested_principal_krw,
            created_game_day: receipt.created_game_day,
            expires_game_day: receipt.expires_game_day,
            decision_code: to_loan_quote_decision_snapshot(receipt.decision_code),
            decision_reasons: receipt
                .decision_reasons
                .into_iter()
                .map(to_loan_quote_reason_snapshot)
                .collect(),
            verified_annual_income_krw: receipt.verified_annual_income_krw,
            verified_income_source: receipt
                .verified_income_source
                .map(to_verified_income_source_snapshot),
            existing_loan_balance_krw: receipt.existing_loan_balance_krw,
            post_execution_balance_krw: receipt.post_execution_balance_krw,
            dsr_applied: receipt.dsr_applied,
            dsr: receipt.dsr.map(to_loan_quote_dsr_snapshot),
            stress_rate_bp: receipt.stress_rate_bp,
            quoted_terms: to_loan_quoted_terms_snapshot(receipt.quoted_terms),
        },
        replayed: receipt.replayed,
        snapshot,
    }
}

const fn to_lease_deposit_loan_quote_decision_snapshot(
    decision: LeaseDepositLoanQuoteDecisionState,
) -> LeaseDepositLoanQuoteDecisionSnapshot {
    match decision {
        LeaseDepositLoanQuoteDecisionState::Eligible => {
            LeaseDepositLoanQuoteDecisionSnapshot::Eligible
        }
        LeaseDepositLoanQuoteDecisionState::CreditRestricted => {
            LeaseDepositLoanQuoteDecisionSnapshot::CreditRestricted
        }
        LeaseDepositLoanQuoteDecisionState::CollateralLimit => {
            LeaseDepositLoanQuoteDecisionSnapshot::CollateralLimit
        }
        LeaseDepositLoanQuoteDecisionState::IncomeUnavailable => {
            LeaseDepositLoanQuoteDecisionSnapshot::IncomeUnavailable
        }
        LeaseDepositLoanQuoteDecisionState::AffordabilityLimit => {
            LeaseDepositLoanQuoteDecisionSnapshot::AffordabilityLimit
        }
    }
}

const fn to_lease_deposit_loan_quote_reason_snapshot(
    reason: LeaseDepositLoanQuoteReasonState,
) -> LeaseDepositLoanQuoteReasonSnapshot {
    match reason {
        LeaseDepositLoanQuoteReasonState::InsolvencyRebuilding => {
            LeaseDepositLoanQuoteReasonSnapshot::InsolvencyRebuilding
        }
        LeaseDepositLoanQuoteReasonState::ActiveDefault => {
            LeaseDepositLoanQuoteReasonSnapshot::ActiveDefault
        }
        LeaseDepositLoanQuoteReasonState::ActiveDelinquency => {
            LeaseDepositLoanQuoteReasonSnapshot::ActiveDelinquency
        }
        LeaseDepositLoanQuoteReasonState::ActiveRestructuring => {
            LeaseDepositLoanQuoteReasonSnapshot::ActiveRestructuring
        }
        LeaseDepositLoanQuoteReasonState::CreditBandRestricted => {
            LeaseDepositLoanQuoteReasonSnapshot::CreditBandRestricted
        }
        LeaseDepositLoanQuoteReasonState::ActiveLoanLimit => {
            LeaseDepositLoanQuoteReasonSnapshot::ActiveLoanLimit
        }
        LeaseDepositLoanQuoteReasonState::CollateralLimit => {
            LeaseDepositLoanQuoteReasonSnapshot::CollateralLimit
        }
        LeaseDepositLoanQuoteReasonState::IncomeUnavailable => {
            LeaseDepositLoanQuoteReasonSnapshot::IncomeUnavailable
        }
        LeaseDepositLoanQuoteReasonState::AffordabilityLimit => {
            LeaseDepositLoanQuoteReasonSnapshot::AffordabilityLimit
        }
        LeaseDepositLoanQuoteReasonState::Eligible => LeaseDepositLoanQuoteReasonSnapshot::Eligible,
    }
}

const fn to_lease_deposit_loan_affordability_snapshot(
    state: LeaseDepositLoanAffordabilityState,
) -> LeaseDepositLoanAffordabilitySnapshot {
    LeaseDepositLoanAffordabilitySnapshot {
        numerator_krw: state.numerator_krw,
        denominator_krw: state.denominator_krw,
        ratio_ppm: state.ratio_ppm,
        limit_ppm: state.limit_ppm,
    }
}

fn to_lease_deposit_loan_quote_response(
    receipt: LeaseDepositLoanQuoteReceipt,
    snapshot: GameSnapshot,
) -> Result<LeaseDepositLoanQuoteResponse> {
    if receipt.offer_kind != HousingLeaseOfferKind::Jeonse || receipt.regulatory_dsr_applied {
        bail!("lease-deposit quote exposes unsupported regulatory evidence");
    }
    Ok(LeaseDepositLoanQuoteResponse {
        result: LeaseDepositLoanQuoteResultSnapshot {
            quote_id: receipt.quote_id,
            listing_id: receipt.listing_id,
            offer_kind: JeonseHousingLeaseOfferKindSnapshot::Jeonse,
            product_version_id: receipt.product_version_id,
            requested_principal_krw: receipt.requested_principal_krw,
            deposit_krw: receipt.deposit_krw,
            funding_limit_ppm: receipt.funding_limit_ppm,
            maximum_funding_krw: receipt.maximum_funding_krw,
            created_game_day: receipt.created_game_day,
            expires_game_day: receipt.expires_game_day,
            decision_code: to_lease_deposit_loan_quote_decision_snapshot(receipt.decision_code),
            decision_reasons: receipt
                .decision_reasons
                .into_iter()
                .map(to_lease_deposit_loan_quote_reason_snapshot)
                .collect(),
            verified_annual_income_krw: receipt.verified_annual_income_krw,
            verified_income_source: receipt
                .verified_income_source
                .map(to_verified_income_source_snapshot),
            existing_loan_balance_krw: receipt.existing_loan_balance_krw,
            post_execution_balance_krw: receipt.post_execution_balance_krw,
            regulatory_dsr_applied: RegulatoryDsrAppliedSnapshot,
            affordability: receipt
                .affordability
                .map(to_lease_deposit_loan_affordability_snapshot),
            quoted_terms: to_loan_quoted_terms_snapshot(receipt.quoted_terms),
            replaced_loan_id: receipt.replaced_loan_id,
            replaced_loan_principal_krw: receipt.replaced_loan_principal_krw,
        },
        replayed: receipt.replayed,
        snapshot,
    })
}

fn to_loan_execution_response(
    receipt: LoanExecutionReceipt,
    snapshot: GameSnapshot,
) -> LoanExecutionResponse {
    LoanExecutionResponse {
        result: LoanExecutionResultSnapshot {
            loan_id: receipt.loan_id,
            quote_id: receipt.quote_id,
            product_version_id: receipt.product_version_id,
            principal_krw: receipt.principal_krw,
            activated_game_day: receipt.activated_game_day,
            maturity_game_day: receipt.maturity_game_day,
            annual_rate_bp: receipt.annual_rate_bp,
            repayment_method: to_loan_repayment_method_snapshot(receipt.repayment_method),
            term_months: receipt.term_months,
            first_installment: to_loan_quote_first_installment_snapshot(receipt.first_installment),
        },
        replayed: receipt.replayed,
        snapshot,
    }
}

fn to_loan_prepayment_response(
    receipt: LoanPrepaymentReceipt,
    snapshot: GameSnapshot,
) -> LoanPrepaymentResponse {
    LoanPrepaymentResponse {
        result: LoanPrepaymentResultSnapshot {
            loan_id: receipt.loan_id,
            payment_id: receipt.payment_id,
            principal_krw: receipt.principal_krw,
            fee_krw: receipt.fee_krw,
            total_debited_krw: receipt.total_debited_krw,
            applied_game_day: receipt.applied_game_day,
            remaining_principal_krw: receipt.remaining_principal_krw,
            status: match receipt.status {
                LoanPrepaymentStatusState::Active => LoanPrepaymentStatusSnapshot::Active,
                LoanPrepaymentStatusState::PaidOff => LoanPrepaymentStatusSnapshot::PaidOff,
            },
            prepayment_effect: to_loan_prepayment_effect_snapshot(receipt.prepayment_effect),
            remaining_installments: receipt.remaining_installments,
            next_installment: receipt.next_installment.map(|installment| {
                LoanPrepaymentNextInstallmentSnapshot {
                    installment_no: installment.installment_no,
                    due_game_day: installment.due_game_day,
                    fee_krw: installment.fee_krw,
                    principal_krw: installment.principal_krw,
                    interest_krw: installment.interest_krw,
                    total_krw: installment.total_krw,
                }
            }),
            final_installment_due_game_day: receipt.final_installment_due_game_day,
        },
        replayed: receipt.replayed,
        snapshot,
    }
}

fn to_next_loan_installment_snapshot(
    state: &NextLoanInstallmentState,
) -> NextLoanInstallmentSnapshot {
    NextLoanInstallmentSnapshot {
        loan_id: state.loan_id,
        installment_no: state.installment_no,
        due_game_day: state.due_game_day,
        fee_krw: state.fee_krw,
        interest_krw: state.interest_krw,
        principal_krw: state.principal_krw,
        remaining_due_krw: state.remaining_due_krw,
    }
}

const fn to_life_event_capability_snapshot(
    capability: LifeEventCapabilityState,
) -> LifeEventCapabilitySnapshot {
    match capability {
        LifeEventCapabilityState::DeterministicChoices => {
            LifeEventCapabilitySnapshot::DeterministicChoices
        }
        LifeEventCapabilityState::Unavailable => LifeEventCapabilitySnapshot::Unavailable,
    }
}

const fn to_insurance_capability_snapshot(
    capability: InsuranceCapabilityState,
) -> InsuranceCapabilitySnapshot {
    match capability {
        InsuranceCapabilityState::ContractsAndClaims => {
            InsuranceCapabilitySnapshot::ContractsAndClaims
        }
        InsuranceCapabilityState::Unavailable => InsuranceCapabilitySnapshot::Unavailable,
    }
}

const fn to_insurance_eligibility_status_snapshot(
    status: InsuranceEligibilityStatusState,
) -> InsuranceEligibilityStatusSnapshot {
    match status {
        InsuranceEligibilityStatusState::Eligible => InsuranceEligibilityStatusSnapshot::Eligible,
        InsuranceEligibilityStatusState::Ineligible => {
            InsuranceEligibilityStatusSnapshot::Ineligible
        }
        InsuranceEligibilityStatusState::Indeterminate => {
            InsuranceEligibilityStatusSnapshot::Indeterminate
        }
    }
}

const fn to_insurance_eligibility_reason_snapshot(
    reason: InsuranceEligibilityReasonState,
) -> InsuranceEligibilityReasonSnapshot {
    match reason {
        InsuranceEligibilityReasonState::AgeOutsideRange => {
            InsuranceEligibilityReasonSnapshot::AgeOutsideRange
        }
        InsuranceEligibilityReasonState::DependentRequired => {
            InsuranceEligibilityReasonSnapshot::DependentRequired
        }
        InsuranceEligibilityReasonState::ResidenceRequired => {
            InsuranceEligibilityReasonSnapshot::ResidenceRequired
        }
        InsuranceEligibilityReasonState::MilitaryServing => {
            InsuranceEligibilityReasonSnapshot::MilitaryServing
        }
        InsuranceEligibilityReasonState::AuthorityUnavailable => {
            InsuranceEligibilityReasonSnapshot::AuthorityUnavailable
        }
    }
}

const fn to_insurance_contract_status_snapshot(
    status: InsuranceContractStatusState,
) -> InsuranceContractStatusSnapshot {
    match status {
        InsuranceContractStatusState::Active => InsuranceContractStatusSnapshot::Active,
        InsuranceContractStatusState::Lapsed => InsuranceContractStatusSnapshot::Lapsed,
        InsuranceContractStatusState::Expired => InsuranceContractStatusSnapshot::Expired,
        InsuranceContractStatusState::Cancelled => InsuranceContractStatusSnapshot::Cancelled,
    }
}

const fn to_life_event_decision_snapshot(
    decision: LifeEventDecisionKindState,
) -> LifeEventDecisionKindSnapshot {
    match decision {
        LifeEventDecisionKindState::Accepted => LifeEventDecisionKindSnapshot::Accepted,
        LifeEventDecisionKindState::Declined => LifeEventDecisionKindSnapshot::Declined,
    }
}

const fn to_life_event_resolution_snapshot(
    resolution: LifeEventResolutionKindState,
) -> LifeEventResolutionKindSnapshot {
    match resolution {
        LifeEventResolutionKindState::Accepted => LifeEventResolutionKindSnapshot::Accepted,
        LifeEventResolutionKindState::Declined => LifeEventResolutionKindSnapshot::Declined,
        LifeEventResolutionKindState::Expired => LifeEventResolutionKindSnapshot::Expired,
    }
}

fn to_life_event_choice_snapshot(state: LifeEventChoiceState) -> Result<LifeEventChoiceSnapshot> {
    ensure!(
        !state.display_name.is_empty() && state.display_name.chars().count() <= 120,
        "life event choice name is empty or too long"
    );
    let effect_summary = match state.effect_summary {
        LifeEventEffectSummaryState::NoEffect => LifeEventEffectSummarySnapshot::NoEffect,
        LifeEventEffectSummaryState::WalletExpense { amount_krw } => {
            ensure!(
                (1..=MAX_SAFE_JSON_INTEGER).contains(&amount_krw),
                "life event expense is outside the public money range"
            );
            LifeEventEffectSummarySnapshot::WalletExpense { amount_krw }
        }
    };
    Ok(LifeEventChoiceSnapshot {
        id: state.id,
        display_name: state.display_name,
        decision_kind: to_life_event_decision_snapshot(state.decision_kind),
        effect_summary,
    })
}

fn to_pending_life_event_snapshot(
    state: PendingLifeEventState,
) -> Result<PendingLifeEventSnapshot> {
    ensure!(
        is_canonical_life_event_identifier(&state.event_key),
        "life event key is not canonical"
    );
    ensure!(
        !state.display_name.is_empty() && state.display_name.chars().count() <= 80,
        "life event name is empty or too long"
    );
    ensure!(
        state.offered_game_day < state.expires_game_day,
        "life event expiry does not follow its offer"
    );
    ensure!(
        (2..=8).contains(&state.choices.len()),
        "life event choice window is outside the public bound"
    );
    let mut choice_ids = HashSet::with_capacity(state.choices.len());
    let choices = state
        .choices
        .into_iter()
        .map(|choice| {
            ensure!(
                choice_ids.insert(choice.id),
                "life event choices contain duplicate IDs"
            );
            to_life_event_choice_snapshot(choice)
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        choices.iter().any(|choice| {
            choice.id == state.default_choice_id
                && matches!(
                    choice.effect_summary,
                    LifeEventEffectSummarySnapshot::NoEffect
                )
        }),
        "life event no-effect default choice is absent"
    );
    Ok(PendingLifeEventSnapshot {
        id: state.id,
        event_key: state.event_key,
        display_name: state.display_name,
        offered_game_day: state.offered_game_day,
        expires_game_day: state.expires_game_day,
        default_choice_id: state.default_choice_id,
        choices,
    })
}

fn to_life_event_history_snapshot(
    state: LifeEventHistoryItemState,
) -> Result<LifeEventHistoryItemSnapshot> {
    ensure!(
        is_canonical_life_event_identifier(&state.event_key),
        "life event history key is not canonical"
    );
    ensure!(
        !state.display_name.is_empty() && state.display_name.chars().count() <= 80,
        "life event history name is empty or too long"
    );
    ensure!(
        state.offered_game_day <= state.resolved_game_day,
        "life event resolution precedes its offer"
    );
    let choice = to_life_event_choice_snapshot(state.choice)?;
    ensure!(
        matches!(
            (
                state.resolution_kind,
                choice.decision_kind,
                &choice.effect_summary
            ),
            (
                LifeEventResolutionKindState::Accepted,
                LifeEventDecisionKindSnapshot::Accepted,
                _
            ) | (
                LifeEventResolutionKindState::Declined,
                LifeEventDecisionKindSnapshot::Declined,
                _
            ) | (
                LifeEventResolutionKindState::Expired,
                _,
                LifeEventEffectSummarySnapshot::NoEffect
            )
        ),
        "life event resolution disagrees with its choice"
    );
    Ok(LifeEventHistoryItemSnapshot {
        id: state.id,
        event_key: state.event_key,
        display_name: state.display_name,
        offered_game_day: state.offered_game_day,
        resolved_game_day: state.resolved_game_day,
        resolution_kind: to_life_event_resolution_snapshot(state.resolution_kind),
        choice,
    })
}

fn to_life_events_response(state: LifeEventsState) -> Result<LifeEventsResponse> {
    ensure!(
        state.pending_events.len() <= 8 && state.history.len() <= 20,
        "life event response window is unbounded"
    );
    ensure!(
        state.next_cursor.as_ref().is_none_or(|cursor| {
            !cursor.is_empty() && cursor.len() <= 512 && cursor.is_ascii()
        }),
        "life event cursor is invalid"
    );
    if state.capability == LifeEventCapabilityState::Unavailable {
        ensure!(
            state.pending_events.is_empty()
                && state.history.is_empty()
                && state.next_cursor.is_none(),
            "unavailable life events exposed runtime state"
        );
    }
    let mut previous_pending_id = None;
    let pending_events = state
        .pending_events
        .into_iter()
        .map(|event| {
            ensure!(
                previous_pending_id.is_none_or(|previous| previous < event.id),
                "pending life events are not ordered by ID"
            );
            previous_pending_id = Some(event.id);
            to_pending_life_event_snapshot(event)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut previous_history = None;
    let history = state
        .history
        .into_iter()
        .map(|event| {
            ensure!(
                previous_history.is_none_or(|(day, id)| {
                    day > event.resolved_game_day
                        || (day == event.resolved_game_day && id > event.id)
                }),
                "life event history is not in reverse canonical order"
            );
            previous_history = Some((event.resolved_game_day, event.id));
            to_life_event_history_snapshot(event)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(LifeEventsResponse {
        life_event_capability: to_life_event_capability_snapshot(state.capability),
        insurance_capability: to_insurance_capability_snapshot(state.insurance_capability),
        pending_events,
        history,
        next_cursor: state.next_cursor,
    })
}

fn to_life_event_choice_response(
    receipt: LifeEventChoiceReceipt,
    snapshot: GameSnapshot,
) -> Result<LifeEventChoiceResponse> {
    ensure!(
        receipt.replayed || receipt.resolved_game_day == snapshot.game_day,
        "life event choice receipt day disagrees with its snapshot"
    );
    ensure!(
        (-MAX_SAFE_JSON_INTEGER..=0).contains(&receipt.wallet_delta_krw),
        "life event wallet delta is outside the public range"
    );
    ensure!(
        !snapshot
            .life
            .pending_events
            .iter()
            .any(|event| event.id == receipt.event_id),
        "resolved life event remains pending in the committed snapshot"
    );
    Ok(LifeEventChoiceResponse {
        result: LifeEventChoiceResultSnapshot {
            event_id: receipt.event_id,
            choice_id: receipt.choice_id,
            resolution_kind: to_life_event_decision_snapshot(receipt.resolution_kind),
            resolved_game_day: receipt.resolved_game_day,
            wallet_delta_krw: receipt.wallet_delta_krw,
        },
        replayed: receipt.replayed,
        snapshot,
    })
}

fn to_insurance_product_snapshot(state: InsuranceProductState) -> Result<InsuranceProductSnapshot> {
    ensure!(
        !state.product_key.is_empty()
            && state.product_key.len() <= 64
            && !state.display_name.is_empty()
            && state.display_name.len() <= 80
            && !state.covered_event_key.is_empty()
            && state.covered_event_key.len() <= 64
            && !state.covered_event_display_name.is_empty()
            && state.covered_event_display_name.len() <= 80,
        "insurance product text is invalid"
    );
    ensure!(
        state.reasons.len() <= 8
            && state
                .reasons
                .windows(2)
                .all(|pair| insurance_reason_rank(pair[0]) < insurance_reason_rank(pair[1])),
        "insurance eligibility reasons are not canonical"
    );
    match state.eligibility_status {
        InsuranceEligibilityStatusState::Eligible => ensure!(
            state.reasons.is_empty(),
            "eligible insurance product has rejection reasons"
        ),
        InsuranceEligibilityStatusState::Ineligible => ensure!(
            !state.reasons.is_empty()
                && !state
                    .reasons
                    .contains(&InsuranceEligibilityReasonState::AuthorityUnavailable),
            "ineligible insurance product reasons are invalid"
        ),
        InsuranceEligibilityStatusState::Indeterminate => ensure!(
            state
                .reasons
                .contains(&InsuranceEligibilityReasonState::AuthorityUnavailable),
            "indeterminate insurance product lacks an authority reason"
        ),
    }
    ensure!(
        (1..=MAX_SAFE_JSON_INTEGER).contains(&state.premium_krw)
            && (0..=MAX_SAFE_JSON_INTEGER).contains(&state.deductible_krw)
            && (1..=MAX_SAFE_JSON_INTEGER).contains(&state.occurrence_limit_krw)
            && (1..=MAX_SAFE_JSON_INTEGER).contains(&state.term_limit_krw)
            && state.occurrence_limit_krw <= state.term_limit_krw
            && state.premium_interval_game_days > 0
            && state.term_game_days > 0
            && state.waiting_period_game_days < state.term_game_days
            && state.claim_window_game_days > 0,
        "insurance product terms are invalid"
    );
    Ok(InsuranceProductSnapshot {
        id: state.id,
        product_key: state.product_key,
        display_name: state.display_name,
        eligibility_status: to_insurance_eligibility_status_snapshot(state.eligibility_status),
        reasons: state
            .reasons
            .into_iter()
            .map(to_insurance_eligibility_reason_snapshot)
            .collect(),
        covered_event_key: state.covered_event_key,
        covered_event_display_name: state.covered_event_display_name,
        premium_krw: state.premium_krw,
        premium_interval_game_days: state.premium_interval_game_days,
        term_game_days: state.term_game_days,
        waiting_period_game_days: state.waiting_period_game_days,
        deductible_krw: state.deductible_krw,
        occurrence_limit_krw: state.occurrence_limit_krw,
        term_limit_krw: state.term_limit_krw,
        claim_window_game_days: state.claim_window_game_days,
    })
}

const fn insurance_reason_rank(reason: InsuranceEligibilityReasonState) -> u8 {
    match reason {
        InsuranceEligibilityReasonState::AgeOutsideRange => 1,
        InsuranceEligibilityReasonState::DependentRequired => 2,
        InsuranceEligibilityReasonState::ResidenceRequired => 3,
        InsuranceEligibilityReasonState::MilitaryServing => 4,
        InsuranceEligibilityReasonState::AuthorityUnavailable => 5,
    }
}

fn to_insurance_contract_snapshot(
    state: InsuranceContractState,
) -> Result<InsuranceContractSnapshot> {
    ensure!(
        state.coverage_start_game_day < state.coverage_end_exclusive
            && state.waiting_ends_game_day >= state.coverage_start_game_day,
        "insurance contract coverage boundaries are invalid"
    );
    if matches!(
        state.status,
        InsuranceContractStatusState::Active | InsuranceContractStatusState::Expired
    ) {
        ensure!(
            state.waiting_ends_game_day < state.coverage_end_exclusive,
            "unshortened insurance contract ends before its waiting period"
        );
    }
    ensure!(
        (1..=MAX_SAFE_JSON_INTEGER).contains(&state.premium_krw)
            && (0..=MAX_SAFE_JSON_INTEGER).contains(&state.paid_benefit_krw)
            && (0..=MAX_SAFE_JSON_INTEGER).contains(&state.reserved_benefit_krw)
            && (0..=MAX_SAFE_JSON_INTEGER).contains(&state.remaining_benefit_krw),
        "insurance contract money is outside the public range"
    );
    if state.status != InsuranceContractStatusState::Active {
        ensure!(
            state.next_premium_due_game_day.is_none(),
            "terminal insurance contract exposes a future premium"
        );
    }
    Ok(InsuranceContractSnapshot {
        id: state.id,
        product_version_id: state.product_version_id,
        product_key: state.product_key,
        display_name: state.display_name,
        status: to_insurance_contract_status_snapshot(state.status),
        coverage_start_game_day: state.coverage_start_game_day,
        waiting_ends_game_day: state.waiting_ends_game_day,
        coverage_end_exclusive: state.coverage_end_exclusive,
        next_premium_due_game_day: state.next_premium_due_game_day,
        premium_krw: state.premium_krw,
        paid_benefit_krw: state.paid_benefit_krw,
        reserved_benefit_krw: state.reserved_benefit_krw,
        remaining_benefit_krw: state.remaining_benefit_krw,
    })
}

fn to_insurance_claim_allocations(
    states: Vec<InsuranceClaimAllocationState>,
    expected_payout_krw: i64,
) -> Result<Vec<InsuranceClaimAllocationSnapshot>> {
    ensure!(
        (1..=8).contains(&states.len()),
        "insurance claim allocation window is invalid"
    );
    let mut previous_contract_id = None;
    let mut total = 0_i64;
    let allocations = states
        .into_iter()
        .map(|state| {
            ensure!(
                previous_contract_id.is_none_or(|previous| previous < state.contract_id),
                "insurance claim allocations are not ordered by contract ID"
            );
            ensure!(
                (0..=MAX_SAFE_JSON_INTEGER).contains(&state.deductible_krw)
                    && (1..=MAX_SAFE_JSON_INTEGER).contains(&state.payout_krw),
                "insurance claim allocation money is invalid"
            );
            previous_contract_id = Some(state.contract_id);
            total = total
                .checked_add(state.payout_krw)
                .context("insurance claim allocation total overflowed")?;
            Ok(InsuranceClaimAllocationSnapshot {
                contract_id: state.contract_id,
                deductible_krw: state.deductible_krw,
                payout_krw: state.payout_krw,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        total == expected_payout_krw,
        "insurance claim allocations disagree with the payout"
    );
    Ok(allocations)
}

fn pending_insurance_claim_id(state: &PendingInsuranceClaimState) -> ResourceId {
    match state {
        PendingInsuranceClaimState::Candidate { id, .. }
        | PendingInsuranceClaimState::Ready { id, .. } => *id,
    }
}

fn to_pending_insurance_claim_snapshot(
    state: PendingInsuranceClaimState,
) -> Result<PendingInsuranceClaimSnapshot> {
    match state {
        PendingInsuranceClaimState::Candidate {
            id,
            event_id,
            event_key,
            event_display_name,
            offered_game_day,
        } => Ok(PendingInsuranceClaimSnapshot::Candidate {
            id,
            event_id,
            event_key,
            event_display_name,
            offered_game_day,
            gross_cost_krw: None,
            payout_krw: None,
            filing_deadline_game_day: None,
        }),
        PendingInsuranceClaimState::Ready {
            id,
            event_id,
            event_key,
            event_display_name,
            offered_game_day,
            gross_cost_krw,
            payout_krw,
            filing_deadline_game_day,
            contract_allocations,
        } => {
            ensure!(
                (1..=MAX_SAFE_JSON_INTEGER).contains(&gross_cost_krw)
                    && (1..=gross_cost_krw).contains(&payout_krw)
                    && filing_deadline_game_day > offered_game_day,
                "ready insurance claim is invalid"
            );
            Ok(PendingInsuranceClaimSnapshot::Ready {
                id,
                event_id,
                event_key,
                event_display_name,
                offered_game_day,
                gross_cost_krw,
                payout_krw,
                filing_deadline_game_day,
                contract_allocations: to_insurance_claim_allocations(
                    contract_allocations,
                    payout_krw,
                )?,
            })
        }
    }
}

fn insurance_history_order(state: &InsuranceClaimHistoryState) -> (u32, ResourceId) {
    match state {
        InsuranceClaimHistoryState::NotApplicable {
            id,
            resolved_game_day,
            ..
        }
        | InsuranceClaimHistoryState::NotCovered {
            id,
            resolved_game_day,
            ..
        }
        | InsuranceClaimHistoryState::Paid {
            id,
            resolved_game_day,
            ..
        }
        | InsuranceClaimHistoryState::Expired {
            id,
            resolved_game_day,
            ..
        } => (*resolved_game_day, *id),
    }
}

fn to_insurance_claim_history_snapshot(
    state: InsuranceClaimHistoryState,
) -> Result<InsuranceClaimHistoryItemSnapshot> {
    match state {
        InsuranceClaimHistoryState::NotApplicable {
            id,
            event_id,
            event_key,
            event_display_name,
            offered_game_day,
            resolved_game_day,
        } => {
            ensure!(
                resolved_game_day >= offered_game_day,
                "not-applicable insurance claim resolved before its event"
            );
            Ok(InsuranceClaimHistoryItemSnapshot::NotApplicable {
                id,
                event_id,
                event_key,
                event_display_name,
                offered_game_day,
                resolved_game_day,
                gross_cost_krw: None,
                payout_krw: None,
                filing_deadline_game_day: None,
            })
        }
        InsuranceClaimHistoryState::NotCovered {
            id,
            event_id,
            event_key,
            event_display_name,
            offered_game_day,
            resolved_game_day,
            gross_cost_krw,
        } => {
            ensure!(
                resolved_game_day >= offered_game_day
                    && (1..=MAX_SAFE_JSON_INTEGER).contains(&gross_cost_krw),
                "not-covered insurance claim is invalid"
            );
            Ok(InsuranceClaimHistoryItemSnapshot::NotCovered {
                id,
                event_id,
                event_key,
                event_display_name,
                offered_game_day,
                resolved_game_day,
                gross_cost_krw,
                payout_krw: 0,
                filing_deadline_game_day: None,
            })
        }
        InsuranceClaimHistoryState::Paid {
            id,
            event_id,
            event_key,
            event_display_name,
            offered_game_day,
            resolved_game_day,
            gross_cost_krw,
            payout_krw,
            filing_deadline_game_day,
            paid_game_day,
            contract_allocations,
        } => {
            ensure!(
                resolved_game_day >= offered_game_day
                    && paid_game_day >= resolved_game_day
                    && paid_game_day < filing_deadline_game_day
                    && (1..=gross_cost_krw).contains(&payout_krw),
                "paid insurance claim is invalid"
            );
            Ok(InsuranceClaimHistoryItemSnapshot::Paid {
                id,
                event_id,
                event_key,
                event_display_name,
                offered_game_day,
                resolved_game_day,
                gross_cost_krw,
                payout_krw,
                filing_deadline_game_day,
                paid_game_day,
                contract_allocations: to_insurance_claim_allocations(
                    contract_allocations,
                    payout_krw,
                )?,
            })
        }
        InsuranceClaimHistoryState::Expired {
            id,
            event_id,
            event_key,
            event_display_name,
            offered_game_day,
            resolved_game_day,
            gross_cost_krw,
            payout_krw,
            filing_deadline_game_day,
            contract_allocations,
        } => {
            ensure!(
                resolved_game_day >= offered_game_day
                    && filing_deadline_game_day > resolved_game_day
                    && (1..=gross_cost_krw).contains(&payout_krw),
                "expired insurance claim is invalid"
            );
            Ok(InsuranceClaimHistoryItemSnapshot::Expired {
                id,
                event_id,
                event_key,
                event_display_name,
                offered_game_day,
                resolved_game_day,
                gross_cost_krw,
                payout_krw,
                filing_deadline_game_day,
                contract_allocations: to_insurance_claim_allocations(
                    contract_allocations,
                    payout_krw,
                )?,
            })
        }
    }
}

fn to_insurance_contracts_response(state: InsuranceState) -> Result<InsuranceContractsResponse> {
    ensure!(
        state.products.len() <= 16
            && state.contracts.len() <= 20
            && state.pending_claims.len() <= 8
            && state.history.len() <= 20,
        "insurance response window is unbounded"
    );
    ensure!(
        state.next_cursor.as_ref().is_none_or(|cursor| {
            !cursor.is_empty() && cursor.len() <= 512 && cursor.is_ascii()
        }),
        "insurance cursor is invalid"
    );
    if state.capability == InsuranceCapabilityState::Unavailable {
        ensure!(
            state.products.is_empty()
                && state.contracts.is_empty()
                && state.pending_claims.is_empty()
                && state.history.is_empty()
                && state.next_cursor.is_none(),
            "unavailable insurance exposed runtime state"
        );
    }
    let mut previous_product_id = None;
    let products = state
        .products
        .into_iter()
        .map(|product| {
            ensure!(
                previous_product_id.is_none_or(|previous| previous < product.id),
                "insurance products are not ordered by ID"
            );
            previous_product_id = Some(product.id);
            to_insurance_product_snapshot(product)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut previous_contract_id = None;
    let contracts = state
        .contracts
        .into_iter()
        .map(|contract| {
            ensure!(
                previous_contract_id.is_none_or(|previous| previous > contract.id),
                "insurance contracts are not in reverse ID order"
            );
            previous_contract_id = Some(contract.id);
            to_insurance_contract_snapshot(contract)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut previous_pending_id = None;
    let pending_claims = state
        .pending_claims
        .into_iter()
        .map(|claim| {
            let id = pending_insurance_claim_id(&claim);
            ensure!(
                previous_pending_id.is_none_or(|previous| previous < id),
                "pending insurance claims are not ordered by ID"
            );
            previous_pending_id = Some(id);
            to_pending_insurance_claim_snapshot(claim)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut previous_history = None;
    let history = state
        .history
        .into_iter()
        .map(|claim| {
            let (day, id) = insurance_history_order(&claim);
            ensure!(
                previous_history.is_none_or(|(previous_day, previous_id)| {
                    previous_day > day || (previous_day == day && previous_id > id)
                }),
                "insurance claim history is not in reverse canonical order"
            );
            previous_history = Some((day, id));
            to_insurance_claim_history_snapshot(claim)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(InsuranceContractsResponse {
        insurance_capability: to_insurance_capability_snapshot(state.capability),
        products,
        contracts,
        pending_claims,
        history,
        next_cursor: state.next_cursor,
    })
}

fn to_insurance_enrollment_response(
    receipt: InsuranceEnrollmentReceipt,
    snapshot: GameSnapshot,
) -> Result<InsuranceEnrollmentResponse> {
    ensure!(
        receipt.status == InsuranceContractStatusState::Active
            && receipt.coverage_start_game_day < receipt.coverage_end_exclusive
            && receipt.waiting_ends_game_day >= receipt.coverage_start_game_day
            && receipt.waiting_ends_game_day < receipt.coverage_end_exclusive
            && receipt.next_premium_due_game_day > receipt.coverage_start_game_day
            && receipt.next_premium_due_game_day < receipt.coverage_end_exclusive
            && (1..=MAX_SAFE_JSON_INTEGER).contains(&receipt.premium_krw),
        "insurance enrollment receipt is invalid"
    );
    if !receipt.replayed {
        ensure!(
            receipt.coverage_start_game_day == snapshot.game_day
                && snapshot
                    .life
                    .active_insurance_contracts
                    .iter()
                    .any(|contract| {
                        contract.id == receipt.contract_id
                            && contract.product_version_id == receipt.product_version_id
                    }),
            "insurance enrollment result disagrees with its snapshot"
        );
    }
    Ok(InsuranceEnrollmentResponse {
        result: InsuranceEnrollmentResultSnapshot {
            contract_id: receipt.contract_id,
            product_version_id: receipt.product_version_id,
            status: to_insurance_contract_status_snapshot(receipt.status),
            coverage_start_game_day: receipt.coverage_start_game_day,
            waiting_ends_game_day: receipt.waiting_ends_game_day,
            coverage_end_exclusive: receipt.coverage_end_exclusive,
            next_premium_due_game_day: receipt.next_premium_due_game_day,
            premium_krw: receipt.premium_krw,
        },
        replayed: receipt.replayed,
        snapshot,
    })
}

fn to_insurance_cancellation_response(
    receipt: InsuranceCancellationReceipt,
    snapshot: GameSnapshot,
) -> Result<InsuranceCancellationResponse> {
    ensure!(
        receipt.status == InsuranceContractStatusState::Cancelled
            && receipt.coverage_end_exclusive > 0
            && !snapshot
                .life
                .active_insurance_contracts
                .iter()
                .any(|contract| contract.id == receipt.contract_id),
        "insurance cancellation result disagrees with its snapshot"
    );
    Ok(InsuranceCancellationResponse {
        result: InsuranceCancellationResultSnapshot {
            contract_id: receipt.contract_id,
            status: to_insurance_contract_status_snapshot(receipt.status),
            coverage_end_exclusive: receipt.coverage_end_exclusive,
        },
        replayed: receipt.replayed,
        snapshot,
    })
}

fn to_insurance_claim_response(
    receipt: InsuranceClaimReceipt,
    snapshot: GameSnapshot,
) -> Result<InsuranceClaimResponse> {
    ensure!(
        (1..=MAX_SAFE_JSON_INTEGER).contains(&receipt.payout_krw)
            && (receipt.replayed || receipt.paid_game_day == snapshot.game_day)
            && !snapshot
                .life
                .pending_insurance_claims
                .iter()
                .any(|claim| { pending_insurance_claim_snapshot_id(claim) == receipt.claim_id }),
        "insurance claim result disagrees with its snapshot"
    );
    Ok(InsuranceClaimResponse {
        result: InsuranceClaimResultSnapshot {
            claim_id: receipt.claim_id,
            event_id: receipt.event_id,
            payout_krw: receipt.payout_krw,
            paid_game_day: receipt.paid_game_day,
        },
        replayed: receipt.replayed,
        snapshot,
    })
}

fn pending_insurance_claim_snapshot_id(state: &PendingInsuranceClaimSnapshot) -> ResourceId {
    match state {
        PendingInsuranceClaimSnapshot::Candidate { id, .. }
        | PendingInsuranceClaimSnapshot::Ready { id, .. } => *id,
    }
}

fn to_welfare_evaluation_status_snapshot(
    status: WelfareEvaluationStatusState,
) -> WelfareEvaluationStatusSnapshot {
    match status {
        WelfareEvaluationStatusState::Eligible => WelfareEvaluationStatusSnapshot::Eligible,
        WelfareEvaluationStatusState::Ineligible => WelfareEvaluationStatusSnapshot::Ineligible,
        WelfareEvaluationStatusState::Indeterminate => {
            WelfareEvaluationStatusSnapshot::Indeterminate
        }
    }
}

fn to_welfare_condition_outcome_snapshot(
    outcome: WelfareConditionOutcomeState,
) -> WelfareConditionOutcomeSnapshot {
    match outcome {
        WelfareConditionOutcomeState::Passed => WelfareConditionOutcomeSnapshot::Passed,
        WelfareConditionOutcomeState::Failed => WelfareConditionOutcomeSnapshot::Failed,
        WelfareConditionOutcomeState::Unknown => WelfareConditionOutcomeSnapshot::Unknown,
    }
}

fn to_welfare_application_status_snapshot(
    status: WelfareApplicationStatusState,
) -> WelfareApplicationStatusSnapshot {
    match status {
        WelfareApplicationStatusState::Applied => WelfareApplicationStatusSnapshot::Applied,
        WelfareApplicationStatusState::Approved => WelfareApplicationStatusSnapshot::Approved,
        WelfareApplicationStatusState::Rejected => WelfareApplicationStatusSnapshot::Rejected,
        WelfareApplicationStatusState::Active => WelfareApplicationStatusSnapshot::Active,
        WelfareApplicationStatusState::Exhausted => WelfareApplicationStatusSnapshot::Exhausted,
        WelfareApplicationStatusState::Terminated => WelfareApplicationStatusSnapshot::Terminated,
    }
}

fn to_welfare_payment_status_snapshot(
    status: WelfarePaymentStatusState,
) -> WelfarePaymentStatusSnapshot {
    match status {
        WelfarePaymentStatusState::Pending => WelfarePaymentStatusSnapshot::Pending,
        WelfarePaymentStatusState::Paid => WelfarePaymentStatusSnapshot::Paid,
        WelfarePaymentStatusState::Cancelled => WelfarePaymentStatusSnapshot::Cancelled,
    }
}

fn to_welfare_conditions_snapshot(
    states: &[WelfareConditionResultState],
) -> Result<Vec<WelfareConditionResultSnapshot>> {
    ensure!(
        (1..=32).contains(&states.len()),
        "welfare condition window is empty or unbounded"
    );
    let mut conditions = Vec::with_capacity(states.len());
    for state in states {
        ensure!(
            is_canonical_welfare_identifier(&state.code),
            "welfare condition code is not canonical"
        );
        ensure!(
            (1..=120).contains(&state.label.chars().count()),
            "welfare condition label is empty or too long"
        );
        ensure!(
            !conditions
                .iter()
                .any(|condition: &WelfareConditionResultSnapshot| condition.code == state.code),
            "welfare condition codes are not unique"
        );
        conditions.push(WelfareConditionResultSnapshot {
            code: state.code.clone(),
            label: state.label.clone(),
            outcome: to_welfare_condition_outcome_snapshot(state.outcome),
        });
    }
    Ok(conditions)
}

fn to_welfare_payment_snapshot(state: &WelfarePaymentState) -> Result<WelfarePaymentSnapshot> {
    ensure!(state.payment_no > 0, "welfare payment number is zero");
    ensure!(
        (1..=9_007_199_254_740_991).contains(&state.amount_krw),
        "welfare payment amount is outside the JSON-safe money range"
    );
    Ok(WelfarePaymentSnapshot {
        id: state.id,
        payment_no: state.payment_no,
        amount_krw: state.amount_krw,
        due_game_day: state.due_game_day,
        status: to_welfare_payment_status_snapshot(state.status),
    })
}

fn to_welfare_application_summary_snapshot(
    state: &WelfareApplicationSummaryState,
) -> Result<WelfareApplicationSummarySnapshot> {
    ensure!(
        (0..=9_007_199_254_740_991).contains(&state.paid_krw),
        "welfare paid amount is outside the JSON-safe money range"
    );
    if matches!(
        state.status,
        WelfareApplicationStatusState::Approved
            | WelfareApplicationStatusState::Active
            | WelfareApplicationStatusState::Exhausted
    ) {
        ensure!(
            state.approval_game_day == Some(state.application_game_day),
            "the D1 welfare fixture must approve on its application day"
        );
    }
    Ok(WelfareApplicationSummarySnapshot {
        id: state.id,
        status: to_welfare_application_status_snapshot(state.status),
        application_game_day: state.application_game_day,
        approval_game_day: state.approval_game_day,
        paid_krw: state.paid_krw,
    })
}

fn to_welfare_program_snapshot(state: &WelfareProgramState) -> Result<WelfareProgramSnapshot> {
    ensure!(
        is_canonical_welfare_identifier(&state.program_key),
        "welfare program key is not canonical"
    );
    ensure!(
        (1..=120).contains(&state.display_name.chars().count()),
        "welfare program name is empty or too long"
    );
    ensure!(
        (1..=9_007_199_254_740_991).contains(&state.benefit_krw),
        "welfare benefit is outside the JSON-safe money range"
    );
    ensure!(
        (1..=365).contains(&state.payment_delay_game_days),
        "welfare payment delay is outside the public range"
    );
    ensure!(
        state.fact_fingerprint.len() == 64
            && state
                .fact_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "welfare fact fingerprint is not canonical SHA-256 hex"
    );
    let evaluation_status = to_welfare_evaluation_status_snapshot(state.evaluation_status);
    let conditions = to_welfare_conditions_snapshot(&state.conditions)?;
    let latest_application = state
        .latest_application
        .as_ref()
        .map(to_welfare_application_summary_snapshot)
        .transpose()?;
    let next_payment = state
        .next_payment
        .as_ref()
        .map(to_welfare_payment_snapshot)
        .transpose()?;
    ensure!(
        !state.application_available
            || (state.evaluation_status == WelfareEvaluationStatusState::Eligible
                && latest_application.is_none()),
        "welfare application availability disagrees with evaluation and history"
    );
    if latest_application.is_none() {
        ensure!(next_payment.is_none(), "welfare payment has no application");
    }
    if let Some(application) = latest_application.as_ref() {
        match application.status {
            WelfareApplicationStatusSnapshot::Active => ensure!(
                next_payment
                    .as_ref()
                    .is_some_and(|payment| payment.status == WelfarePaymentStatusSnapshot::Pending),
                "active welfare application has no pending payment"
            ),
            WelfareApplicationStatusSnapshot::Exhausted => ensure!(
                next_payment.is_none() && application.paid_krw == state.benefit_krw,
                "exhausted welfare application does not reconcile with its benefit"
            ),
            _ => {}
        }
        if let Some(payment) = next_payment.as_ref() {
            let due_game_day = application
                .application_game_day
                .checked_add(u32::from(state.payment_delay_game_days))
                .context("welfare payment due day overflowed")?;
            ensure!(
                payment.payment_no == 1
                    && payment.amount_krw == state.benefit_krw
                    && payment.due_game_day == due_game_day,
                "welfare payment disagrees with the published benefit schedule"
            );
        }
    }

    Ok(WelfareProgramSnapshot {
        id: state.id,
        program_key: state.program_key.clone(),
        display_name: state.display_name.clone(),
        benefit_krw: state.benefit_krw,
        payment_delay_game_days: state.payment_delay_game_days,
        evaluation_status,
        fact_fingerprint: state.fact_fingerprint.clone(),
        conditions,
        application_available: state.application_available,
        latest_application,
        next_payment,
    })
}

fn to_welfare_programs_response(state: WelfareProgramsState) -> Result<WelfareProgramsResponse> {
    ensure!(
        state.programs.len() <= 16,
        "welfare program catalog window is unbounded"
    );
    for (index, program) in state.programs.iter().enumerate() {
        ensure!(
            !state.programs[..index].iter().any(|previous| {
                previous.id == program.id || previous.program_key == program.program_key
            }),
            "welfare program catalog contains duplicate identity"
        );
        if let Some(previous) = index
            .checked_sub(1)
            .and_then(|previous| state.programs.get(previous))
        {
            ensure!(
                previous.program_key < program.program_key
                    || (previous.program_key == program.program_key
                        && previous.id.get() < program.id.get()),
                "welfare program catalog is not canonically ordered"
            );
        }
    }
    Ok(WelfareProgramsResponse {
        component_version_id: state.component_version_id,
        game_day: state.game_day,
        programs: state
            .programs
            .iter()
            .map(to_welfare_program_snapshot)
            .collect::<Result<Vec<_>>>()?,
    })
}

fn to_active_welfare_application_snapshot(
    state: &ActiveWelfareApplicationState,
    game_day: u32,
) -> Result<ActiveWelfareApplicationSnapshot> {
    ensure!(
        state.status == WelfareApplicationStatusState::Active,
        "life snapshot contains a non-active welfare application"
    );
    ensure!(
        is_canonical_welfare_identifier(&state.program_key),
        "active welfare program key is not canonical"
    );
    ensure!(
        (1..=120).contains(&state.display_name.chars().count()),
        "active welfare program name is empty or too long"
    );
    ensure!(
        state.application_game_day <= game_day
            && state.approval_game_day == state.application_game_day,
        "active welfare application dates disagree with the committed day"
    );
    ensure!(
        (1..=9_007_199_254_740_991).contains(&state.benefit_krw)
            && (0..=state.benefit_krw).contains(&state.paid_krw),
        "active welfare amount is outside the public range"
    );
    let next_payment = state
        .next_payment
        .as_ref()
        .map(to_welfare_payment_snapshot)
        .transpose()?;
    let payment = next_payment
        .as_ref()
        .context("active welfare application has no pending payment")?;
    ensure!(
        payment.status == WelfarePaymentStatusSnapshot::Pending
            && payment.payment_no == 1
            && payment
                .amount_krw
                .checked_add(state.paid_krw)
                .is_some_and(|total| total == state.benefit_krw)
            && payment.due_game_day
                == state
                    .application_game_day
                    .checked_add(1)
                    .context("active welfare due day overflowed")?
            && payment.due_game_day > game_day,
        "active welfare payment does not reconcile with its D+1 benefit"
    );
    Ok(ActiveWelfareApplicationSnapshot {
        application_id: state.application_id,
        program_version_id: state.program_version_id,
        program_key: state.program_key.clone(),
        display_name: state.display_name.clone(),
        status: ActiveWelfareApplicationStatusSnapshot::Active,
        application_game_day: state.application_game_day,
        approval_game_day: state.approval_game_day,
        benefit_krw: state.benefit_krw,
        paid_krw: state.paid_krw,
        next_payment,
    })
}

fn to_welfare_application_response(
    receipt: WelfareApplicationReceipt,
    snapshot: GameSnapshot,
) -> Result<WelfareApplicationResponse> {
    ensure!(
        receipt.status == WelfareApplicationStatusState::Active
            && receipt.approval_game_day == receipt.application_game_day,
        "the D1 welfare application did not become active on its application day"
    );
    let eligibility_at_application =
        to_welfare_conditions_snapshot(&receipt.eligibility_at_application)?;
    let payment = to_welfare_payment_snapshot(&receipt.payment)?;
    ensure!(
        payment.status == WelfarePaymentStatusSnapshot::Pending
            && payment.payment_no == 1
            && payment.due_game_day
                == receipt
                    .application_game_day
                    .checked_add(1)
                    .context("welfare application due day overflowed")?,
        "the D1 welfare application has an invalid pending payment"
    );
    if !receipt.replayed {
        ensure!(
            snapshot
                .life
                .active_welfare_applications
                .iter()
                .any(|application| {
                    application.application_id == receipt.application_id
                        && application.program_version_id == receipt.program_version_id
                        && application.next_payment.as_ref().is_some_and(|pending| {
                            pending.id == payment.id && pending.amount_krw == payment.amount_krw
                        })
                }),
            "new welfare application is absent from its committed snapshot"
        );
    }
    Ok(WelfareApplicationResponse {
        result: WelfareApplicationResultSnapshot {
            application_id: receipt.application_id,
            program_version_id: receipt.program_version_id,
            status: ActiveWelfareApplicationStatusSnapshot::Active,
            application_game_day: receipt.application_game_day,
            approval_game_day: receipt.approval_game_day,
            eligibility_at_application,
            payment,
        },
        replayed: receipt.replayed,
        snapshot,
    })
}

fn is_canonical_welfare_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=64).contains(&bytes.len())
        && bytes.first().is_some_and(u8::is_ascii_lowercase)
        && bytes.iter().all(u8::is_ascii_alphanumeric)
}

fn is_canonical_life_event_identifier(value: &str) -> bool {
    is_canonical_welfare_identifier(value)
}

const fn to_insolvency_availability_snapshot(
    value: InsolvencyAvailabilityState,
) -> InsolvencyAvailabilitySnapshot {
    match value {
        InsolvencyAvailabilityState::Unavailable => InsolvencyAvailabilitySnapshot::Unavailable,
        InsolvencyAvailabilityState::CashOnlyLiquidation => {
            InsolvencyAvailabilitySnapshot::CashOnlyLiquidation
        }
    }
}

const fn to_insolvency_eligibility_status_snapshot(
    value: InsolvencyEligibilityStatus,
) -> InsolvencyEligibilityStatusSnapshot {
    match value {
        InsolvencyEligibilityStatus::Eligible => InsolvencyEligibilityStatusSnapshot::Eligible,
        InsolvencyEligibilityStatus::Ineligible => InsolvencyEligibilityStatusSnapshot::Ineligible,
        InsolvencyEligibilityStatus::CompositionUnsupported => {
            InsolvencyEligibilityStatusSnapshot::CompositionUnsupported
        }
        InsolvencyEligibilityStatus::Unavailable => {
            InsolvencyEligibilityStatusSnapshot::Unavailable
        }
    }
}

const fn to_insolvency_eligibility_reason_snapshot(
    value: InsolvencyEligibilityReason,
) -> InsolvencyEligibilityReasonSnapshot {
    match value {
        InsolvencyEligibilityReason::PolicyUnavailable => {
            InsolvencyEligibilityReasonSnapshot::PolicyUnavailable
        }
        InsolvencyEligibilityReason::ComponentUnavailable => {
            InsolvencyEligibilityReasonSnapshot::ComponentUnavailable
        }
        InsolvencyEligibilityReason::InvalidWalletCash => {
            InsolvencyEligibilityReasonSnapshot::InvalidWalletCash
        }
        InsolvencyEligibilityReason::NoSupportedDefaultedDebt => {
            InsolvencyEligibilityReasonSnapshot::NoSupportedDefaultedDebt
        }
        InsolvencyEligibilityReason::DebtNotGreaterThanCash => {
            InsolvencyEligibilityReasonSnapshot::DebtNotGreaterThanCash
        }
        InsolvencyEligibilityReason::UnsupportedLoanComposition => {
            InsolvencyEligibilityReasonSnapshot::UnsupportedLoanComposition
        }
        InsolvencyEligibilityReason::UnsupportedAssetComposition => {
            InsolvencyEligibilityReasonSnapshot::UnsupportedAssetComposition
        }
        InsolvencyEligibilityReason::UnsupportedNonLoanObligation => {
            InsolvencyEligibilityReasonSnapshot::UnsupportedNonLoanObligation
        }
        InsolvencyEligibilityReason::ExistingNonTerminalCase => {
            InsolvencyEligibilityReasonSnapshot::ExistingNonTerminalCase
        }
    }
}

const fn to_insolvency_procedure_snapshot(
    value: InsolvencyProcedureKind,
) -> InsolvencyProcedureKindSnapshot {
    match value {
        InsolvencyProcedureKind::CashOnlyLiquidation => {
            InsolvencyProcedureKindSnapshot::CashOnlyLiquidation
        }
    }
}

const fn to_insolvency_case_status_snapshot(
    value: InsolvencyCaseStatus,
) -> InsolvencyCaseStatusSnapshot {
    match value {
        InsolvencyCaseStatus::Prepared => InsolvencyCaseStatusSnapshot::Prepared,
        InsolvencyCaseStatus::Filed => InsolvencyCaseStatusSnapshot::Filed,
        InsolvencyCaseStatus::Liquidation => InsolvencyCaseStatusSnapshot::Liquidation,
        InsolvencyCaseStatus::Discharged => InsolvencyCaseStatusSnapshot::Discharged,
        InsolvencyCaseStatus::Rebuilding => InsolvencyCaseStatusSnapshot::Rebuilding,
        InsolvencyCaseStatus::Withdrawn => InsolvencyCaseStatusSnapshot::Withdrawn,
        InsolvencyCaseStatus::Recovered => InsolvencyCaseStatusSnapshot::Recovered,
    }
}

fn to_insolvency_case_summary_snapshot(
    state: &InsolvencyCaseSummaryState,
) -> Result<InsolvencyCaseSummarySnapshot> {
    ensure!(
        state.protected_cash_krw >= 0
            && state.protected_cash_krw <= state.wallet_cash_krw
            && state.distributed_krw >= 0
            && state.discharged_krw >= 0,
        "insolvency case summary has invalid money"
    );
    Ok(InsolvencyCaseSummarySnapshot {
        id: state.id,
        procedure_kind: to_insolvency_procedure_snapshot(state.procedure_kind),
        status: to_insolvency_case_status_snapshot(state.status),
        prepared_game_day: state.prepared_game_day,
        submitted_game_day: state.submitted_game_day,
        wallet_cash_krw: state.wallet_cash_krw,
        protected_cash_krw: state.protected_cash_krw,
        distributed_krw: state.distributed_krw,
        discharged_krw: state.discharged_krw,
        credit_restriction_end_exclusive: state.credit_restriction_end_exclusive,
    })
}

fn to_insolvency_snapshot(state: &InsolvencySnapshotState) -> Result<InsolvencySnapshot> {
    ensure!(
        state.reasons.len() <= 16,
        "insolvency eligibility reasons exceed the public bound"
    );
    Ok(InsolvencySnapshot {
        availability: to_insolvency_availability_snapshot(state.availability),
        eligibility: to_insolvency_eligibility_status_snapshot(state.eligibility),
        reasons: state
            .reasons
            .iter()
            .copied()
            .map(to_insolvency_eligibility_reason_snapshot)
            .collect(),
        current_case: state
            .current_case
            .as_ref()
            .map(to_insolvency_case_summary_snapshot)
            .transpose()?,
    })
}

fn to_insolvency_case_command_response(
    receipt: InsolvencyCaseReceipt,
    snapshot: GameSnapshot,
) -> Result<InsolvencyCaseCommandResponse> {
    Ok(InsolvencyCaseCommandResponse {
        result: to_insolvency_case_summary_snapshot(&receipt.case)?,
        replayed: receipt.replayed,
        snapshot,
    })
}

fn to_insolvency_case_detail_response(
    state: InsolvencyCaseDetailState,
) -> Result<InsolvencyCaseDetailResponse> {
    ensure!(
        (1..=16).contains(&state.transitions.len())
            && state.composition_sha256.len() == 64
            && state
                .composition_sha256
                .bytes()
                .all(|byte| { byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte) }),
        "insolvency case detail is outside the public bounds"
    );
    let mut previous_sequence = 0_u8;
    let transitions = state
        .transitions
        .into_iter()
        .map(|transition| {
            ensure!(
                transition.sequence == previous_sequence.saturating_add(1),
                "insolvency transitions are not canonically ordered"
            );
            previous_sequence = transition.sequence;
            Ok(InsolvencyTransitionSnapshot {
                sequence: transition.sequence,
                from_status: transition
                    .from_status
                    .map(to_insolvency_case_status_snapshot),
                to_status: to_insolvency_case_status_snapshot(transition.to_status),
                game_day: transition.game_day,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(InsolvencyCaseDetailResponse {
        summary: to_insolvency_case_summary_snapshot(&state.summary)?,
        policy_set_id: state.policy_set_id,
        life_catalog_set_id: state.life_catalog_set_id,
        insolvency_component_version_id: state.insolvency_component_version_id,
        composition_sha256: state.composition_sha256,
        automatic_protected_krw: state.automatic_protected_krw,
        additional_protected_krw: state.additional_protected_krw,
        liquidatable_krw: state.liquidatable_krw,
        total_claim_krw: state.total_claim_krw,
        claim_count: state.claim_count,
        transitions,
    })
}

fn to_insolvency_claim_snapshot(state: InsolvencyClaimState) -> Result<InsolvencyClaimSnapshot> {
    let allowed = state
        .principal_krw
        .checked_add(state.interest_krw)
        .and_then(|amount| amount.checked_add(state.fee_krw))
        .context("insolvency claim total overflowed")?;
    let reconciled = state
        .distributed_krw
        .checked_add(state.discharged_krw)
        .context("insolvency claim reconciliation overflowed")?;
    ensure!(
        state.principal_krw >= 0
            && state.interest_krw >= 0
            && state.fee_krw >= 0
            && allowed == state.allowed_krw
            && state.distributed_krw >= 0
            && state.discharged_krw >= 0
            && reconciled <= state.allowed_krw,
        "insolvency claim totals are inconsistent"
    );
    Ok(InsolvencyClaimSnapshot {
        id: state.id,
        loan_contract_id: state.loan_contract_id,
        principal_krw: state.principal_krw,
        interest_krw: state.interest_krw,
        fee_krw: state.fee_krw,
        allowed_krw: state.allowed_krw,
        distributed_krw: state.distributed_krw,
        discharged_krw: state.discharged_krw,
    })
}

fn to_insolvency_claim_page_response(
    state: InsolvencyClaimPageState,
) -> Result<InsolvencyClaimPageResponse> {
    ensure!(
        state.claims.len() <= 20
            && state.next_cursor.as_ref().is_none_or(|cursor| {
                !cursor.is_empty() && cursor.len() <= 512 && cursor.is_ascii()
            }),
        "insolvency claim page is outside the public bounds"
    );
    let mut previous_id = None;
    let claims = state
        .claims
        .into_iter()
        .map(|claim| {
            ensure!(
                previous_id.is_none_or(|previous| previous < claim.id),
                "insolvency claims are not ordered by ID"
            );
            previous_id = Some(claim.id);
            to_insolvency_claim_snapshot(claim)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(InsolvencyClaimPageResponse {
        claims,
        next_cursor: state.next_cursor,
    })
}

fn to_insolvency_wallet_asset_snapshot(
    state: InsolvencyWalletAssetState,
) -> Result<InsolvencyWalletAssetSnapshot> {
    let protected_and_liquidatable = state
        .protected_amount_krw
        .checked_add(state.liquidatable_krw)
        .context("insolvency wallet total overflowed")?;
    ensure!(
        state.protected_amount_krw >= 0
            && state.liquidatable_krw >= 0
            && state.distributed_krw >= 0
            && protected_and_liquidatable == state.original_amount_krw
            && state.distributed_krw <= state.liquidatable_krw,
        "insolvency wallet liquidation totals are inconsistent"
    );
    Ok(InsolvencyWalletAssetSnapshot {
        original_amount_krw: state.original_amount_krw,
        protected_amount_krw: state.protected_amount_krw,
        liquidatable_krw: state.liquidatable_krw,
        distributed_krw: state.distributed_krw,
    })
}

fn to_insolvency_liquidation_page_response(
    state: InsolvencyLiquidationPageState,
) -> Result<InsolvencyLiquidationPageResponse> {
    ensure!(
        state.distributions.len() <= 20
            && state.next_cursor.as_ref().is_none_or(|cursor| {
                !cursor.is_empty() && cursor.len() <= 512 && cursor.is_ascii()
            }),
        "insolvency liquidation page is outside the public bounds"
    );
    let mut previous_id = None;
    let distributions = state
        .distributions
        .into_iter()
        .map(|distribution: InsolvencyLiquidationState| {
            ensure!(
                distribution.amount_krw > 0
                    && previous_id.is_none_or(|previous| previous < distribution.id),
                "insolvency distributions are not canonically ordered"
            );
            previous_id = Some(distribution.id);
            Ok(InsolvencyLiquidationSnapshot {
                id: distribution.id,
                claim_id: distribution.claim_id,
                amount_krw: distribution.amount_krw,
                loan_payment_id: distribution.loan_payment_id,
                ledger_transaction_id: distribution.ledger_transaction_id,
                applied_game_day: distribution.applied_game_day,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(InsolvencyLiquidationPageResponse {
        wallet_asset: state
            .wallet_asset
            .map(to_insolvency_wallet_asset_snapshot)
            .transpose()?,
        distributions,
        next_cursor: state.next_cursor,
    })
}

const fn to_corporation_availability_snapshot(
    value: CorporationAvailabilityState,
) -> CorporationAvailabilitySnapshot {
    match value {
        CorporationAvailabilityState::Unavailable => CorporationAvailabilitySnapshot::Unavailable,
        CorporationAvailabilityState::Active => CorporationAvailabilitySnapshot::Active,
    }
}

const fn to_corporation_status_snapshot(
    value: CorporationStatusState,
) -> CorporationStatusSnapshot {
    match value {
        CorporationStatusState::Draft => CorporationStatusSnapshot::Draft,
        CorporationStatusState::Active => CorporationStatusSnapshot::Active,
        CorporationStatusState::Dormant => CorporationStatusSnapshot::Dormant,
        CorporationStatusState::Insolvent => CorporationStatusSnapshot::Insolvent,
        CorporationStatusState::Dissolved => CorporationStatusSnapshot::Dissolved,
    }
}

fn to_corporation_template_snapshot(
    state: CorporationTemplateState,
) -> Result<CorporationTemplateSnapshot> {
    ensure!(
        is_canonical_welfare_identifier(&state.template_key)
            && !state.display_name.trim().is_empty()
            && state.display_name.len() <= 40
            && (1..=3).contains(&state.template_order)
            && state.base_monthly_revenue_krw > 0
            && state.revenue_variation_ppm <= 900_000
            && state.variable_cost_ppm <= 1_000_000
            && state.fixed_monthly_cost_krw >= 0
            && state.operating_scales.len() == 3,
        "corporation template is outside the public bounds"
    );
    let mut previous_scale_order = 0_u8;
    let operating_scales = state
        .operating_scales
        .into_iter()
        .map(|scale| {
            ensure!(
                is_canonical_welfare_identifier(&scale.scale_key)
                    && scale.scale_order == previous_scale_order.saturating_add(1)
                    && (1..=3_000_000).contains(&scale.revenue_factor_ppm)
                    && scale.fixed_cost_krw >= 0,
                "corporation operating scale is outside the public bounds"
            );
            previous_scale_order = scale.scale_order;
            Ok(to_corporation_operating_scale_snapshot(scale))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(CorporationTemplateSnapshot {
        id: state.id,
        template_key: state.template_key,
        display_name: state.display_name,
        template_order: state.template_order,
        base_monthly_revenue_krw: state.base_monthly_revenue_krw,
        revenue_variation_ppm: state.revenue_variation_ppm,
        variable_cost_ppm: state.variable_cost_ppm,
        fixed_monthly_cost_krw: state.fixed_monthly_cost_krw,
        operating_scales,
    })
}

fn to_corporation_operating_scale_snapshot(
    state: CorporationOperatingScaleState,
) -> CorporationOperatingScaleSnapshot {
    CorporationOperatingScaleSnapshot {
        id: state.id,
        scale_key: state.scale_key,
        scale_order: state.scale_order,
        revenue_factor_ppm: state.revenue_factor_ppm,
        fixed_cost_krw: state.fixed_cost_krw,
    }
}

fn to_corporation_operating_setting_snapshot(
    state: CorporationOperatingSettingState,
) -> Result<CorporationOperatingSettingSnapshot> {
    ensure!(
        is_canonical_welfare_identifier(&state.scale_key)
            && (1..=3).contains(&state.scale_order)
            && (1..=3_000_000).contains(&state.revenue_factor_ppm)
            && state.fixed_cost_krw >= 0
            && (1..=9999).contains(&state.effective_year)
            && (1..=12).contains(&state.effective_month)
            && (0..=100_000_000).contains(&state.officer_gross_salary_krw),
        "corporation operating setting is outside the public bounds"
    );
    Ok(CorporationOperatingSettingSnapshot {
        id: state.id,
        corporation_id: state.corporation_id,
        operating_scale_id: state.operating_scale_id,
        scale_key: state.scale_key,
        scale_order: state.scale_order,
        revenue_factor_ppm: state.revenue_factor_ppm,
        fixed_cost_krw: state.fixed_cost_krw,
        effective_year: state.effective_year,
        effective_month: state.effective_month,
        officer_gross_salary_krw: state.officer_gross_salary_krw,
        created_game_day: state.created_game_day,
    })
}

fn to_corporation_next_month_setting_snapshot(
    state: &CorporationNextMonthSettingState,
) -> Result<CorporationNextMonthSettingSnapshot> {
    ensure!(
        is_canonical_welfare_identifier(&state.scale_key)
            && (1..=3).contains(&state.scale_order)
            && (1..=3_000_000).contains(&state.revenue_factor_ppm)
            && state.fixed_cost_krw >= 0
            && (1..=9999).contains(&state.effective_year)
            && (1..=12).contains(&state.effective_month)
            && (0..=100_000_000).contains(&state.officer_gross_salary_krw)
            && (state.setting_id.is_some() == state.created_game_day.is_some()),
        "corporation next-month setting is outside the public bounds"
    );
    Ok(CorporationNextMonthSettingSnapshot {
        setting_id: state.setting_id,
        operating_scale_id: state.operating_scale_id,
        scale_key: state.scale_key.clone(),
        scale_order: state.scale_order,
        revenue_factor_ppm: state.revenue_factor_ppm,
        fixed_cost_krw: state.fixed_cost_krw,
        effective_year: state.effective_year,
        effective_month: state.effective_month,
        officer_gross_salary_krw: state.officer_gross_salary_krw,
        created_game_day: state.created_game_day,
    })
}

fn to_corporation_templates_response(
    state: CorporationTemplatesState,
) -> Result<CorporationTemplatesResponse> {
    match state.availability {
        CorporationAvailabilityState::Unavailable => {
            ensure!(
                state.component_version_id.is_none()
                    && state.registered_office_class.is_none()
                    && state.minimum_capital_krw.is_none()
                    && state.maximum_capital_krw.is_none()
                    && state.game_administrative_fee_krw.is_none()
                    && state.templates.is_empty(),
                "unavailable corporation catalog exposed configuration"
            );
        }
        CorporationAvailabilityState::Active => {
            let minimum_capital = state
                .minimum_capital_krw
                .context("active corporation catalog has no minimum capital")?;
            let maximum_capital = state
                .maximum_capital_krw
                .context("active corporation catalog has no maximum capital")?;
            ensure!(
                state.component_version_id.is_some()
                    && state.registered_office_class.as_deref() == Some("standardRegisteredOffice")
                    && minimum_capital > 0
                    && minimum_capital <= maximum_capital
                    && state
                        .game_administrative_fee_krw
                        .is_some_and(|fee| fee >= 0)
                    && state.templates.len() == 3,
                "active corporation catalog is incomplete"
            );
        }
    }
    let mut previous_order = 0_u8;
    let mut template_ids = std::collections::HashSet::new();
    let mut template_keys = std::collections::HashSet::new();
    let templates = state
        .templates
        .into_iter()
        .map(|template| {
            ensure!(
                template.template_order == previous_order.saturating_add(1)
                    && template_ids.insert(template.id)
                    && template_keys.insert(template.template_key.clone()),
                "corporation templates are not canonically ordered and unique"
            );
            previous_order = template.template_order;
            to_corporation_template_snapshot(template)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(CorporationTemplatesResponse {
        availability: to_corporation_availability_snapshot(state.availability),
        component_version_id: state.component_version_id,
        registered_office_class: state.registered_office_class,
        minimum_capital_krw: state.minimum_capital_krw,
        maximum_capital_krw: state.maximum_capital_krw,
        game_administrative_fee_krw: state.game_administrative_fee_krw,
        templates,
    })
}

fn to_corporation_summary_snapshot(
    state: &CorporationSummaryState,
) -> Result<CorporationSummarySnapshot> {
    let total_fee = state
        .registration_license_tax_krw
        .checked_add(state.local_education_tax_krw)
        .and_then(|amount| amount.checked_add(state.game_administrative_fee_krw))
        .context("corporation establishment fee overflowed")?;
    ensure!(
        is_canonical_welfare_identifier(&state.template_key)
            && !state.template_display_name.trim().is_empty()
            && state.template_display_name.len() <= 40
            && state.name == state.name.trim()
            && (2..=40).contains(&state.name.chars().count())
            && !state.representative_name.trim().is_empty()
            && state.representative_name.len() <= 40
            && state.capital_krw > 0
            && state.registration_license_tax_krw >= 0
            && state.local_education_tax_krw >= 0
            && state.game_administrative_fee_krw >= 0
            && total_fee == state.total_establishment_fee_krw
            && state.cash_krw >= 0
            && state.contributed_capital_krw > 0
            && state.operating_payable_krw >= 0
            && state.corporate_tax_payable_krw >= 0
            && state.distributable_profit_krw >= 0,
        "corporation summary is inconsistent"
    );
    Ok(CorporationSummarySnapshot {
        id: state.id,
        component_version_id: state.component_version_id,
        industry_template_id: state.industry_template_id,
        template_key: state.template_key.clone(),
        template_display_name: state.template_display_name.clone(),
        name: state.name.clone(),
        representative_name: state.representative_name.clone(),
        status: to_corporation_status_snapshot(state.status),
        established_game_day: state.established_game_day,
        capital_krw: state.capital_krw,
        registration_license_tax_krw: state.registration_license_tax_krw,
        local_education_tax_krw: state.local_education_tax_krw,
        game_administrative_fee_krw: state.game_administrative_fee_krw,
        total_establishment_fee_krw: state.total_establishment_fee_krw,
        cash_krw: state.cash_krw,
        contributed_capital_krw: state.contributed_capital_krw,
        retained_earnings_krw: state.retained_earnings_krw,
        operating_payable_krw: state.operating_payable_krw,
        corporate_tax_payable_krw: state.corporate_tax_payable_krw,
        distributable_profit_krw: state.distributable_profit_krw,
        personal_ledger_transaction_id: state.personal_ledger_transaction_id,
        corporation_ledger_transaction_id: state.corporation_ledger_transaction_id,
        next_month_setting: to_corporation_next_month_setting_snapshot(&state.next_month_setting)?,
    })
}

fn to_corporation_snapshot(state: &CorporationSnapshotState) -> Result<CorporationSnapshot> {
    ensure!(
        state.availability == CorporationAvailabilityState::Active || state.current.is_none(),
        "unavailable corporation component exposed a corporation"
    );
    Ok(CorporationSnapshot {
        availability: to_corporation_availability_snapshot(state.availability),
        current: state
            .current
            .as_ref()
            .map(to_corporation_summary_snapshot)
            .transpose()?,
    })
}

fn to_corporation_create_response(
    receipt: CorporationReceipt,
    snapshot: GameSnapshot,
) -> Result<CorporationCreateResponse> {
    let expected_debit = receipt
        .corporation
        .capital_krw
        .checked_add(receipt.corporation.total_establishment_fee_krw)
        .context("corporation wallet debit overflowed")?;
    ensure!(
        receipt.wallet_debit_krw == expected_debit
            && snapshot
                .life
                .corporation
                .current
                .as_ref()
                .is_some_and(|current| current.id == receipt.corporation.id),
        "corporation receipt disagrees with the committed snapshot"
    );
    Ok(CorporationCreateResponse {
        result: to_corporation_summary_snapshot(&receipt.corporation)?,
        wallet_debit_krw: receipt.wallet_debit_krw,
        replayed: receipt.replayed,
        snapshot,
    })
}

fn to_corporation_settings_response(
    receipt: CorporationSettingsReceipt,
    snapshot: GameSnapshot,
) -> Result<CorporationSettingsResponse> {
    ensure!(
        snapshot
            .life
            .corporation
            .current
            .as_ref()
            .is_some_and(|current| current.id == receipt.setting.corporation_id),
        "corporation setting receipt disagrees with the committed snapshot"
    );
    Ok(CorporationSettingsResponse {
        result: to_corporation_operating_setting_snapshot(receipt.setting)?,
        replayed: receipt.replayed,
        snapshot,
    })
}

fn to_corporation_dividend_response(
    receipt: CorporationDividendReceipt,
    snapshot: GameSnapshot,
) -> Result<CorporationDividendResponse> {
    ensure!(
        receipt.gross_dividend_krw > 0
            && receipt.net_dividend_krw
                + receipt.withheld_income_tax_krw
                + receipt.withheld_local_income_tax_krw
                == receipt.gross_dividend_krw
            && snapshot
                .life
                .corporation
                .current
                .as_ref()
                .is_some_and(|current| current.id == receipt.corporation_id),
        "corporation dividend receipt is inconsistent"
    );
    Ok(CorporationDividendResponse {
        result: CorporationDividendSnapshot {
            id: receipt.id,
            corporation_id: receipt.corporation_id,
            tax_year: receipt.tax_year,
            gross_dividend_krw: receipt.gross_dividend_krw,
            withheld_income_tax_krw: receipt.withheld_income_tax_krw,
            withheld_local_income_tax_krw: receipt.withheld_local_income_tax_krw,
            net_dividend_krw: receipt.net_dividend_krw,
            corporation_ledger_transaction_id: receipt.corporation_ledger_transaction_id,
            personal_ledger_transaction_id: receipt.personal_ledger_transaction_id,
            paid_game_day: receipt.paid_game_day,
        },
        replayed: receipt.replayed,
        snapshot,
    })
}

fn to_corporation_operating_month_page_response(
    state: CorporationOperatingMonthPageState,
) -> Result<CorporationOperatingMonthPageResponse> {
    ensure!(
        state.months.len() <= 20
            && state.next_cursor.as_ref().is_none_or(|cursor| {
                !cursor.is_empty() && cursor.len() <= 512 && cursor.is_ascii()
            }),
        "corporation month page is outside public bounds"
    );
    let mut previous_key = None;
    let months = state
        .months
        .into_iter()
        .map(|month| {
            let key = (month.operating_year, month.operating_month, month.id.get());
            ensure!(
                (1..=12).contains(&month.operating_month)
                    && previous_key.is_none_or(|previous| previous < key),
                "corporation months are not canonically ordered"
            );
            previous_key = Some(key);
            to_corporation_operating_month_snapshot(month)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(CorporationOperatingMonthPageResponse {
        months,
        next_cursor: state.next_cursor,
    })
}

fn to_corporation_operating_month_snapshot(
    state: CorporationOperatingMonthState,
) -> Result<CorporationOperatingMonthSnapshot> {
    let payroll_status = match state.payroll_status.as_str() {
        "notConfigured" => CorporationPayrollStatusSnapshot::NotConfigured,
        "paid" => CorporationPayrollStatusSnapshot::Paid,
        "unpaid" => CorporationPayrollStatusSnapshot::Unpaid,
        _ => bail!("corporation month payroll status is invalid"),
    };
    Ok(CorporationOperatingMonthSnapshot {
        id: state.id,
        operating_year: state.operating_year,
        operating_month: state.operating_month,
        scale_key: state.scale_key,
        officer_gross_salary_krw: state.officer_gross_salary_krw,
        revenue_krw: state.revenue_krw,
        operating_expense_krw: state.operating_expense_krw,
        total_payroll_cost_krw: state.total_payroll_cost_krw,
        pre_tax_profit_krw: state.pre_tax_profit_krw,
        payroll_status,
        cash_after_krw: state.cash_after_krw,
        operating_payable_after_krw: state.operating_payable_after_krw,
        retained_earnings_after_krw: state.retained_earnings_after_krw,
        applied_game_day: state.applied_game_day,
    })
}

fn to_life_snapshot(state: &LifeSnapshotState, game_day: u32) -> Result<LifeSnapshot> {
    ensure!(
        state.active_welfare_applications.len() <= 8,
        "active welfare application window is unbounded"
    );
    for (index, application) in state.active_welfare_applications.iter().enumerate() {
        if let Some(previous) = index
            .checked_sub(1)
            .and_then(|previous| state.active_welfare_applications.get(previous))
        {
            ensure!(
                previous.application_id < application.application_id,
                "active welfare applications are not canonically ordered"
            );
        }
        ensure!(
            !state.active_welfare_applications[..index]
                .iter()
                .any(|previous| previous.program_version_id == application.program_version_id),
            "active welfare applications contain a duplicate program"
        );
    }
    let active_welfare_applications = state
        .active_welfare_applications
        .iter()
        .map(|application| to_active_welfare_application_snapshot(application, game_day))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        state.active_insurance_contracts.len() <= 8 && state.pending_insurance_claims.len() <= 8,
        "insurance snapshot window is unbounded"
    );
    if state.insurance_capability == InsuranceCapabilityState::Unavailable {
        ensure!(
            state.active_insurance_contracts.is_empty()
                && state.pending_insurance_claims.is_empty(),
            "unavailable insurance exposed snapshot state"
        );
    }
    let mut previous_contract_id = None;
    let active_insurance_contracts = state
        .active_insurance_contracts
        .iter()
        .cloned()
        .map(|contract| {
            ensure!(
                contract.status == InsuranceContractStatusState::Active
                    && previous_contract_id.is_none_or(|previous| previous < contract.id),
                "active insurance contracts are not canonical"
            );
            previous_contract_id = Some(contract.id);
            to_insurance_contract_snapshot(contract)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut previous_claim_id = None;
    let pending_insurance_claims = state
        .pending_insurance_claims
        .iter()
        .cloned()
        .map(|claim| {
            let claim_id = pending_insurance_claim_id(&claim);
            ensure!(
                previous_claim_id.is_none_or(|previous| previous < claim_id),
                "pending insurance claims are not canonical"
            );
            previous_claim_id = Some(claim_id);
            to_pending_insurance_claim_snapshot(claim)
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        state.pending_events.len() <= 8,
        "pending life event window is unbounded"
    );
    let mut previous_event_id = None;
    let pending_events = state
        .pending_events
        .iter()
        .cloned()
        .map(|event| {
            ensure!(
                previous_event_id.is_none_or(|previous| previous < event.id),
                "pending life events are not canonically ordered"
            );
            ensure!(
                event.offered_game_day <= game_day && game_day < event.expires_game_day,
                "pending life event does not contain the snapshot day"
            );
            previous_event_id = Some(event.id);
            to_pending_life_event_snapshot(event)
        })
        .collect::<Result<Vec<_>>>()?;
    let active_property_holdings = state
        .active_property_holdings
        .iter()
        .map(to_property_holding_snapshot)
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        active_property_holdings.len() <= 4,
        "life snapshot property window is unbounded"
    );
    let visible_property_book_value_krw =
        active_property_holdings
            .iter()
            .try_fold(0_i64, |total, holding| {
                total
                    .checked_add(holding.book_value_krw)
                    .context("life property book-value total overflowed")
            })?;
    if state.has_more_active_property_holdings {
        ensure!(
            active_property_holdings.len() == 4
                && visible_property_book_value_krw < state.total_property_book_value_krw,
            "truncated property window has an invalid total"
        );
    } else {
        ensure!(
            visible_property_book_value_krw == state.total_property_book_value_krw,
            "complete property window disagrees with its total"
        );
    }
    let residence_holding_id = state
        .residence
        .as_ref()
        .and_then(|residence| residence.property_holding_id);
    let owner_residence = state
        .residence
        .as_ref()
        .is_some_and(|residence| residence.tenure_kind == ResidenceTenureKind::Owner);
    ensure!(
        owner_residence == residence_holding_id.is_some(),
        "owner residence and property holding reference disagree"
    );
    if let Some(holding_id) = residence_holding_id {
        ensure!(
            active_property_holdings
                .iter()
                .any(|holding| holding.id == holding_id),
            "owner residence references a property outside the active window"
        );
    } else {
        ensure!(
            active_property_holdings.is_empty(),
            "owner-occupied property has no owner residence"
        );
    }
    for (index, holding) in active_property_holdings.iter().enumerate() {
        ensure!(
            !active_property_holdings[..index].iter().any(|previous| {
                previous.id == holding.id
                    || previous.listing_id == holding.listing_id
                    || (previous.mortgage_loan_id.is_some()
                        && previous.mortgage_loan_id == holding.mortgage_loan_id)
            }),
            "active property window contains duplicate ownership or lien identity"
        );
        if let Some(loan_id) = holding.mortgage_loan_id {
            ensure!(
                state.active_loans.iter().any(|loan| {
                    loan.id == loan_id && loan.product_kind == LoanProductKind::Mortgage
                }),
                "property lien references a non-mortgage or inactive loan"
            );
        }
    }
    ensure!(
        state
            .active_loans
            .iter()
            .filter(|loan| loan.product_kind == LoanProductKind::Mortgage)
            .all(|loan| active_property_holdings
                .iter()
                .any(|holding| holding.mortgage_loan_id == Some(loan.id))),
        "active mortgage has no property lien"
    );

    Ok(LifeSnapshot {
        rate_status: to_life_rate_status_snapshot(state.rate_status),
        household: state.household.as_ref().map(to_life_household_snapshot),
        residence: state.residence.as_ref().map(to_life_residence_snapshot),
        tenant_lease_deposit_krw: state.tenant_lease_deposit_krw,
        active_lease: state
            .active_lease
            .as_ref()
            .map(to_active_housing_lease_snapshot),
        active_lease_arrears: state
            .active_lease_arrears
            .iter()
            .map(to_lease_arrear_snapshot)
            .collect(),
        has_more_active_lease_arrears: state.has_more_active_lease_arrears,
        total_lease_arrear_krw: state.total_lease_arrear_krw,
        active_property_holdings,
        has_more_active_property_holdings: state.has_more_active_property_holdings,
        total_property_book_value_krw: state.total_property_book_value_krw,
        current_month: state
            .current_month
            .as_ref()
            .map(to_living_cost_month_snapshot),
        active_arrears: state
            .active_arrears
            .iter()
            .map(to_essential_arrear_snapshot)
            .collect(),
        has_more_active_arrears: state.has_more_active_arrears,
        total_essential_arrear_krw: state.total_essential_arrear_krw,
        credit_band: state.credit_band.map(to_credit_band_snapshot),
        credit_reasons: state
            .credit_reasons
            .iter()
            .copied()
            .map(to_credit_reason_snapshot)
            .collect(),
        active_loans: state
            .active_loans
            .iter()
            .map(to_loan_summary_snapshot)
            .collect(),
        next_loan_installment: state
            .next_loan_installment
            .as_ref()
            .map(to_next_loan_installment_snapshot),
        total_loan_balance_krw: state.total_loan_balance_krw,
        active_welfare_applications,
        insurance_capability: to_insurance_capability_snapshot(state.insurance_capability),
        active_insurance_contracts,
        pending_insurance_claims,
        pending_events,
        insolvency: to_insolvency_snapshot(&state.insolvency)?,
        corporation: to_corporation_snapshot(&state.corporation)?,
    })
}

fn to_life_budget_response(state: LifeBudgetState) -> LifeBudgetResponse {
    LifeBudgetResponse {
        rate_status: to_life_rate_status_snapshot(state.rate_status),
        household: to_life_household_snapshot(&state.household),
        residence: to_life_residence_snapshot(&state.residence),
        allowed_bands: state
            .allowed_bands
            .iter()
            .map(to_life_budget_band_snapshot)
            .collect(),
        selections: state
            .selections
            .iter()
            .map(to_life_budget_selection_snapshot)
            .collect(),
        current_month: state
            .current_month
            .as_ref()
            .map(to_living_cost_month_snapshot),
        active_arrears: state
            .active_arrears
            .iter()
            .map(to_essential_arrear_snapshot)
            .collect(),
        has_more_active_arrears: state.has_more_active_arrears,
        total_essential_arrear_krw: state.total_essential_arrear_krw,
    }
}

fn to_life_budget_update_response(
    receipt: UpdateLifeBudgetReceipt,
    snapshot: GameSnapshot,
) -> LifeBudgetUpdateResponse {
    LifeBudgetUpdateResponse {
        result: LifeBudgetUpdateResultSnapshot {
            applied_game_day: receipt.applied_game_day,
            selections: receipt
                .selections
                .iter()
                .map(to_life_budget_selection_snapshot)
                .collect(),
        },
        replayed: receipt.replayed,
        snapshot,
    }
}

fn to_essential_arrear_payment_response(
    receipt: EssentialArrearPaymentReceipt,
    snapshot: GameSnapshot,
) -> EssentialArrearPaymentResponse {
    EssentialArrearPaymentResponse {
        result: EssentialArrearPaymentResultSnapshot {
            arrear_id: receipt.arrear_id,
            paid_krw: receipt.paid_krw,
            remaining_krw: receipt.remaining_krw,
        },
        replayed: receipt.replayed,
        snapshot,
    }
}

fn to_lease_arrear_payment_response(
    receipt: LeaseArrearPaymentReceipt,
    snapshot: GameSnapshot,
) -> LeaseArrearPaymentResponse {
    LeaseArrearPaymentResponse {
        result: LeaseArrearPaymentResultSnapshot {
            arrear_id: receipt.arrear_id,
            payment_id: receipt.payment_id,
            paid_krw: receipt.paid_krw,
            remaining_krw: receipt.remaining_krw,
        },
        replayed: receipt.replayed,
        snapshot,
    }
}

fn to_snapshot(state: &CommittedGameState, auto_speed: Option<AutoSpeed>) -> Result<GameSnapshot> {
    let save = &state.save;
    let llx_close_krw = state
        .market
        .m2
        .as_ref()
        .map_or(state.market.equity_close_krw, |m2| m2.llx_close_krw);
    let portfolio =
        value_portfolio(&save.positions, llx_close_krw).context("failed to value the portfolio")?;
    let liquid_cash_krw = save
        .accounts
        .iter()
        .try_fold(save.cash_krw, |total, account| {
            total
                .checked_add(account.cash_krw)
                .context("account cash overflowed net worth")
        })?;
    ensure!(
        save.property_book_value_krw >= 0
            && save.property_book_value_krw == save.life.total_property_book_value_krw,
        "save and life property book values disagree"
    );
    let cash_product_lease_deposit_and_property_krw = liquid_cash_krw
        .checked_add(save.active_product_principal_krw()?)
        .and_then(|value| value.checked_add(save.life.tenant_lease_deposit_krw))
        .and_then(|value| value.checked_add(save.property_book_value_krw))
        .context("cash products, lease deposit, or property overflowed net worth")?;
    let bond_market_value_krw = save
        .m2d_assets
        .bond_positions
        .iter()
        .try_fold(0_i64, |total, position| {
            total.checked_add(position.market_value_krw)
        })
        .context("bond market value overflowed net worth")?;
    let account_gold_market_value_krw = save
        .m2d_assets
        .gold_accounts
        .iter()
        .try_fold(0_i64, |total, account| {
            total.checked_add(account.market_value_krw)
        })
        .context("gold-account market value overflowed net worth")?;
    let physical_gold_market_value_krw = save
        .m2d_assets
        .physical_gold_holdings
        .iter()
        .try_fold(0_i64, |total, holding| {
            total.checked_add(holding.market_value_krw)
        })
        .context("physical-gold market value overflowed net worth")?;
    let investment_market_value_krw = portfolio
        .market_value_krw
        .checked_add(bond_market_value_krw)
        .and_then(|value| value.checked_add(account_gold_market_value_krw))
        .and_then(|value| value.checked_add(physical_gold_market_value_krw))
        .context("market-valued assets overflowed net worth")?;
    let net_worth_krw = checked_net_worth_krw(
        cash_product_lease_deposit_and_property_krw,
        save.debt_krw,
        investment_market_value_krw,
    )
    .context("failed to calculate net worth")?;

    Ok(GameSnapshot {
        run_revision: save.run_revision,
        state_revision: save.state_revision,
        game_day: save.game_day,
        start_date: state.world.start_date.to_string(),
        cash_krw: save.cash_krw,
        debt_krw: save.debt_krw,
        net_worth_krw,
        character_name: save.character.as_ref().map(|c| c.name.clone()),
        auto_speed,
        market: MarketSnapshot {
            world: state.world.key.clone(),
            date: state.market.market_date.to_string(),
            open: state.market.market_open,
            regime: state.market.regime,
            index: MarketIndexSnapshot {
                symbol: "LLX",
                name: "라이프 한국 종합지수",
                close_krw: state.market.equity_close_krw,
                daily_return_ppm: state.market.equity_return_ppm,
            },
            rates: state.market.rates.as_ref().map(to_market_rates_snapshot),
            m2_factors: state
                .market
                .m2
                .as_ref()
                .map(|factors| M2MarketFactorsSnapshot {
                    cpi_index: factors.cpi_index,
                    llx_close_krw: factors.llx_close_krw,
                    gold_close_krw_per_gram: factors.gold_close_krw_per_gram,
                }),
        },
        portfolio,
        finance: FinanceSnapshot {
            policy_set: PolicySetSnapshot {
                key: save.policy_set.key.clone(),
                basis_date: save.policy_set.basis_date.clone(),
            },
            accounts: save
                .accounts
                .iter()
                .map(to_financial_account_snapshot)
                .collect(),
            cma_accounts: save
                .cma_accounts
                .iter()
                .map(to_cma_account_snapshot)
                .collect(),
            cash_contracts: save
                .cash_contracts
                .iter()
                .map(to_cash_contract_snapshot)
                .collect::<Result<Vec<_>>>()?,
            deposit_protection: save
                .deposit_protection
                .iter()
                .map(to_deposit_protection_snapshot)
                .collect(),
            current_tax_year: to_financial_income_year_snapshot(&save.current_annual_tax_year),
            isa_accounts: save
                .isa_accounts
                .iter()
                .map(to_isa_account_snapshot)
                .collect(),
            pension_accounts: save
                .pension_accounts
                .iter()
                .map(to_pension_account_snapshot)
                .collect(),
            product_bundle: save.m2d_assets.product_bundle.clone(),
            llx_distribution_entitlements: save.m2d_assets.llx_distribution_entitlements.clone(),
            bond_positions: save.m2d_assets.bond_positions.clone(),
            gold_accounts: save.m2d_assets.gold_accounts.clone(),
            physical_gold_holdings: save.m2d_assets.physical_gold_holdings.clone(),
            latest_financial_income_assessment: save
                .latest_financial_income_assessment
                .as_ref()
                .map(to_financial_income_assessment_snapshot)
                .transpose()?,
            pending_settlements: save
                .pending_settlements
                .iter()
                .take(20)
                .map(|settlement| PendingSettlementSnapshot {
                    id: settlement.id,
                    due_game_day: settlement.due_game_day,
                    kind: settlement.kind,
                })
                .collect(),
        },
        career: CareerSnapshot {
            focused_job_family_key: save.career.focused_job_family_key.clone(),
            possessed_scores: CareerScoresSnapshot {
                education: save.career.possessed_scores.education,
                certification: save.career.possessed_scores.certification,
                language: save.career.possessed_scores.language,
                training: save.career.possessed_scores.training,
                experience: save.career.possessed_scores.experience,
                project: save.career.possessed_scores.project,
            },
            active_activities: save
                .career
                .active_activities
                .iter()
                .map(|activity| CareerActivitySnapshot {
                    id: activity.id,
                    catalog_entry_id: activity.catalog_entry_id,
                    activity_key: activity.activity_key.clone(),
                    display_name: activity.display_name.clone(),
                    status: activity.status,
                    priority: activity.priority,
                    started_game_day: activity.started_game_day,
                    accumulated_effort_units: activity.accumulated_effort_units,
                    required_effort_units: activity.required_effort_units,
                    elapsed_calendar_days: activity.elapsed_calendar_days,
                    minimum_calendar_days: activity.minimum_calendar_days,
                    daily_effort_cap_units: activity.daily_effort_cap_units,
                    completed_game_day: activity.completed_game_day,
                })
                .collect(),
            latest_artifacts: save
                .career
                .latest_artifacts
                .iter()
                .map(|artifact| CareerArtifactSnapshot {
                    id: artifact.id,
                    kind: artifact.kind,
                    version_no: artifact.version_no,
                    completeness_bp: artifact.completeness_bp,
                    created_game_day: artifact.created_game_day,
                })
                .collect(),
            open_applications: save
                .career
                .open_applications
                .iter()
                .filter(|application| application.status.is_open())
                .take(10)
                .cloned()
                .map(to_career_open_application_snapshot)
                .collect(),
            open_invitations: save
                .career
                .open_invitations
                .iter()
                .filter(|invitation| {
                    invitation.status == crate::store::CareerInvitationStatus::Open
                })
                .take(5)
                .cloned()
                .map(to_career_invitation_snapshot)
                .collect(),
            employment: save
                .career
                .employment
                .as_ref()
                .map(to_career_employment_contract_snapshot),
            latest_payroll: save
                .career
                .latest_payroll
                .clone()
                .map(to_career_payroll_snapshot),
            current_employment_tax_year: to_career_employment_tax_year_snapshot(
                &save.career.current_employment_tax_year,
            ),
            latest_employment_tax_assessment: save
                .career
                .latest_employment_tax_assessment
                .as_ref()
                .map(to_career_employment_tax_year_snapshot),
            military_status: to_military_status_snapshot(save.career.military_status),
            active_military_service: save
                .career
                .active_military_service
                .as_ref()
                .map(to_active_military_service_snapshot)
                .transpose()?,
            active_military_savings: save
                .career
                .active_military_savings
                .iter()
                .take(2)
                .map(to_active_military_savings_snapshot)
                .collect::<Result<Vec<_>>>()?,
            pending_career_schedule: save
                .career
                .pending_career_schedule
                .iter()
                .take(20)
                .map(to_career_pending_schedule_item_snapshot)
                .collect(),
        },
        life: to_life_snapshot(&save.life, save.game_day)?,
    })
}

fn to_financial_account_snapshot(
    account: &crate::finance::FinancialAccount,
) -> FinancialAccountSnapshot {
    FinancialAccountSnapshot {
        id: account.id,
        account_type: account.account_type,
        status: account.status,
        cash_krw: account.cash_krw,
        is_default: account.is_default,
    }
}

fn to_cma_account_snapshot(account: &CmaAccountContractState) -> CmaAccountSnapshot {
    CmaAccountSnapshot {
        account_id: account.account_id,
        product_version_id: account.product_version_id,
        annual_rate_bp: account.annual_rate_bp,
        minimum_interest_balance_krw: account.minimum_interest_balance_krw,
        interest_remainder: account.interest_remainder,
    }
}

fn to_isa_account_snapshot(account: &IsaAccountState) -> IsaAccountSnapshot {
    IsaAccountSnapshot {
        account_id: account.account_id,
        account_type: account.account_type,
        opened_game_day: account.opened_game_day,
        minimum_term_game_day: account.minimum_term_game_day,
        total_contribution_krw: account.total_contribution_krw,
        principal_withdrawal_krw: account.principal_withdrawal_krw,
        contribution_capacity_krw: account.contribution_capacity_krw,
        tax_profit_krw: account.tax_profit_krw,
        deductible_loss_krw: account.deductible_loss_krw,
        expected_close_income_tax_krw: account.expected_close_income_tax_krw,
        expected_close_local_income_tax_krw: account.expected_close_local_income_tax_krw,
    }
}

fn to_pension_account_snapshot(account: &PensionAccountState) -> PensionAccountSnapshot {
    PensionAccountSnapshot {
        account_id: account.account_id,
        account_type: account.account_type,
        opened_game_day: account.opened_game_day,
        eligible_pension_start_game_day: account.eligible_pension_start_game_day,
        pension_started: account.pension_started,
        tax_layers: PensionTaxLayersSnapshot {
            tax_excluded_contribution_krw: account.tax_layers.tax_excluded_contribution_krw,
            deferred_retirement_income_krw: account.tax_layers.deferred_retirement_income_krw,
            credited_contribution_krw: account.tax_layers.credited_contribution_krw,
            earnings_krw: account.tax_layers.earnings_krw,
        },
        current_year_contribution_krw: account.current_year_contribution_krw,
        current_year_credit_eligible_krw: account.current_year_credit_eligible_krw,
        expected_credit_krw: account.expected_credit_krw,
        current_year_pension_limit_krw: account.current_year_pension_limit_krw,
        current_year_pension_withdrawn_krw: account.current_year_pension_withdrawn_krw,
        risk_asset_value_krw: account.risk_asset_value_krw,
        total_value_krw: account.total_value_krw,
        risk_asset_ratio_ppm: account.risk_asset_ratio_ppm,
    }
}

fn to_cash_product_catalog_response(catalog: CashProductCatalog) -> CashProductCatalogResponse {
    CashProductCatalogResponse {
        products: catalog
            .products
            .into_iter()
            .map(|product| CashProductVersionSnapshot {
                id: product.id,
                key: product.key,
                kind: product.kind,
                display_name: product.display_name,
                institution: FinancialInstitutionSnapshot {
                    id: product.institution.id,
                    key: product.institution.key,
                    display_name: product.institution.display_name,
                },
                protection_eligible: product.protection_eligible,
                rate_reference: product.rate_reference,
                spread_bp: product.spread_bp,
                minimum_interest_balance_krw: product.minimum_interest_balance_krw,
                minimum_contribution_krw: product.minimum_contribution_krw,
                maximum_contribution_krw: product.maximum_contribution_krw,
                term_days: product.term_days,
                term_months: product.term_months,
                installment_count: product.installment_count,
                early_termination_rate_bp: product.early_termination_rate_bp,
                day_count_denominator: product.day_count_denominator,
            })
            .collect(),
    }
}

fn to_cma_account_open_snapshot(receipt: OpenCmaAccountReceipt) -> CmaAccountOpenSnapshot {
    CmaAccountOpenSnapshot {
        command_id: receipt.command_id.to_string(),
        account_id: receipt.account_id,
        product_version_id: receipt.product_version_id,
        replayed: receipt.replayed,
    }
}

fn to_cma_account_close_snapshot(receipt: CloseCmaAccountReceipt) -> CmaAccountCloseSnapshot {
    CmaAccountCloseSnapshot {
        command_id: receipt.command_id.to_string(),
        account_id: receipt.account_id,
        replayed: receipt.replayed,
    }
}

fn to_deposit_open_snapshot(receipt: OpenCashProductReceipt) -> Result<DepositOpenSnapshot> {
    Ok(DepositOpenSnapshot {
        command_id: receipt.command_id.to_string(),
        contract_id: receipt.contract_id,
        kind: deposit_kind_snapshot(receipt.kind)?,
        product_version_id: receipt.product_version_id,
        settlement_account_id: receipt.settlement_account_id,
        amount_krw: receipt.amount_krw,
        replayed: receipt.replayed,
    })
}

fn to_deposit_close_snapshot(receipt: CloseCashProductReceipt) -> DepositCloseSnapshot {
    DepositCloseSnapshot {
        command_id: receipt.command_id.to_string(),
        contract_id: receipt.contract_id,
        gross_interest_krw: receipt.gross_interest_krw,
        income_tax_krw: receipt.income_tax_krw,
        local_income_tax_krw: receipt.local_income_tax_krw,
        net_payout_krw: receipt.net_payout_krw,
        replayed: receipt.replayed,
    }
}

fn to_tax_account_open_snapshot(receipt: OpenTaxAccountReceipt) -> TaxAccountOpenSnapshot {
    TaxAccountOpenSnapshot {
        command_id: receipt.command_id.to_string(),
        account_id: receipt.account_id,
        account_type: receipt.account_type,
        replayed: receipt.replayed,
    }
}

fn to_isa_close_snapshot(receipt: CloseIsaAccountReceipt) -> IsaCloseSnapshot {
    IsaCloseSnapshot {
        command_id: receipt.command_id.to_string(),
        account_id: receipt.account_id,
        gross_tax_profit_krw: receipt.gross_tax_profit_krw,
        deductible_loss_krw: receipt.deductible_loss_krw,
        income_tax_krw: receipt.income_tax_krw,
        local_income_tax_krw: receipt.local_income_tax_krw,
        net_payout_krw: receipt.net_payout_krw,
        replayed: receipt.replayed,
    }
}

fn to_pension_start_snapshot(receipt: StartPensionReceipt) -> PensionStartSnapshot {
    PensionStartSnapshot {
        command_id: receipt.command_id.to_string(),
        account_id: receipt.account_id,
        start_tax_year: receipt.start_tax_year,
        payment_years: receipt.payment_years,
        lifetime: receipt.lifetime,
        replayed: receipt.replayed,
    }
}

fn to_pension_withdrawal_snapshot(receipt: PensionWithdrawalReceipt) -> PensionWithdrawalSnapshot {
    PensionWithdrawalSnapshot {
        command_id: receipt.command_id.to_string(),
        account_id: receipt.account_id,
        gross_amount_krw: receipt.gross_amount_krw,
        pension_amount_krw: receipt.pension_amount_krw,
        non_pension_amount_krw: receipt.non_pension_amount_krw,
        tax_free_amount_krw: receipt.tax_free_amount_krw,
        tax_krw: receipt.tax_krw,
        net_payout_krw: receipt.net_payout_krw,
        replayed: receipt.replayed,
    }
}

fn to_cash_contract_snapshot(contract: &CashProductContractState) -> Result<CashContractSnapshot> {
    Ok(CashContractSnapshot {
        contract_id: contract.contract_id,
        product_version_id: contract.product_version_id,
        settlement_account_id: contract.settlement_account_id,
        kind: deposit_kind_snapshot(contract.kind)?,
        status: contract.status,
        annual_rate_bp: contract.annual_rate_bp,
        current_principal_krw: contract.current_principal_krw,
        installment_amount_krw: contract.installment_amount_krw,
        paid_installment_count: contract.paid_installment_count,
        missed_installment_count: contract.missed_installment_count,
        opened_game_day: contract.opened_game_day,
        maturity_game_day: contract.maturity_game_day,
        expected_gross_interest_krw: contract.expected_gross_interest_krw,
        expected_income_tax_krw: contract.expected_income_tax_krw,
        expected_local_income_tax_krw: contract.expected_local_income_tax_krw,
        expected_net_payout_krw: contract.expected_net_payout_krw,
    })
}

fn deposit_kind_snapshot(kind: CashProductKind) -> Result<DepositKindSnapshot> {
    match kind {
        CashProductKind::TermDeposit => Ok(DepositKindSnapshot::TermDeposit),
        CashProductKind::InstallmentSavings => Ok(DepositKindSnapshot::InstallmentSavings),
        CashProductKind::CmaRp | CashProductKind::CmaIssuedNote => {
            bail!("CMA product was stored as a deposit contract")
        }
    }
}

fn to_deposit_protection_snapshot(
    protection: &DepositProtectionState,
) -> DepositProtectionSnapshot {
    DepositProtectionSnapshot {
        institution_id: protection.institution_id,
        eligible_amount_krw: protection.eligible_amount_krw,
        protected_amount_krw: protection.protected_amount_krw,
        unprotected_amount_krw: protection.unprotected_amount_krw,
    }
}

#[derive(Debug, Clone, Copy)]
struct AnnualAssessmentSnapshotFields {
    status: FinancialIncomeYearStatusSnapshot,
    calculated: Option<AnnualTaxCalculatedState>,
    filing_due_date: Option<time::Date>,
    filed_game_day: Option<u32>,
}

fn annual_assessment_snapshot_fields(
    assessment: AnnualTaxAssessmentState,
) -> AnnualAssessmentSnapshotFields {
    match assessment {
        AnnualTaxAssessmentState::NotApplicable => AnnualAssessmentSnapshotFields {
            status: FinancialIncomeYearStatusSnapshot::NotApplicable,
            calculated: None,
            filing_due_date: None,
            filed_game_day: None,
        },
        AnnualTaxAssessmentState::Open => AnnualAssessmentSnapshotFields {
            status: FinancialIncomeYearStatusSnapshot::Open,
            calculated: None,
            filing_due_date: None,
            filed_game_day: None,
        },
        AnnualTaxAssessmentState::FinalizedNoFiling { calculated } => {
            AnnualAssessmentSnapshotFields {
                status: FinancialIncomeYearStatusSnapshot::FinalizedNoFiling,
                calculated: Some(calculated),
                filing_due_date: None,
                filed_game_day: None,
            }
        }
        AnnualTaxAssessmentState::FilingPending {
            calculated,
            filing_due_date,
        } => AnnualAssessmentSnapshotFields {
            status: FinancialIncomeYearStatusSnapshot::FilingPending,
            calculated: Some(calculated),
            filing_due_date: Some(filing_due_date),
            filed_game_day: None,
        },
        AnnualTaxAssessmentState::Filed {
            calculated,
            filing_due_date,
            filed_game_day,
        } => AnnualAssessmentSnapshotFields {
            status: FinancialIncomeYearStatusSnapshot::Filed,
            calculated: Some(calculated),
            filing_due_date: Some(filing_due_date),
            filed_game_day: Some(filed_game_day),
        },
    }
}

fn to_financial_income_year_snapshot(income: &AnnualTaxYearState) -> FinancialIncomeYearSnapshot {
    let assessment = annual_assessment_snapshot_fields(income.assessment);
    let calculated = assessment.calculated;
    FinancialIncomeYearSnapshot {
        tax_year: income.tax_year,
        status: assessment.status,
        sources: income
            .sources
            .iter()
            .map(|source| FinancialIncomeSourceSnapshot {
                source: source.source,
                gross_financial_income_krw: source.gross_financial_income_krw,
                withheld_income_tax_krw: source.withheld_income_tax_krw,
                withheld_local_income_tax_krw: source.withheld_local_income_tax_krw,
            })
            .collect(),
        gross_financial_income_krw: income.gross_financial_income_krw,
        withheld_income_tax_krw: income.withheld_income_tax_krw,
        withheld_local_income_tax_krw: income.withheld_local_income_tax_krw,
        comparison_a_income_tax_krw: calculated.map(|value| value.comparison_a_income_tax_krw),
        comparison_a_local_income_tax_krw: calculated
            .map(|value| value.comparison_a_local_income_tax_krw),
        comparison_b_income_tax_krw: calculated.map(|value| value.comparison_b_income_tax_krw),
        comparison_b_local_income_tax_krw: calculated
            .map(|value| value.comparison_b_local_income_tax_krw),
        assessed_income_tax_krw: calculated.map(|value| value.assessed_income_tax_krw),
        assessed_local_income_tax_krw: calculated.map(|value| value.assessed_local_income_tax_krw),
        additional_tax_krw: calculated.map(|value| value.additional_tax_krw),
        refund_krw: calculated.map(|value| value.refund_krw),
        filing_due_date: assessment.filing_due_date.map(|date| date.to_string()),
        filed_game_day: assessment.filed_game_day,
    }
}

fn to_financial_income_assessment_snapshot(
    income: &AnnualTaxYearState,
) -> Result<FinancialIncomeAssessmentSnapshot> {
    let assessment = annual_assessment_snapshot_fields(income.assessment);
    let calculated = assessment
        .calculated
        .context("latest financial-income assessment is not finalized")?;
    Ok(FinancialIncomeAssessmentSnapshot {
        tax_year: income.tax_year,
        status: assessment.status,
        gross_financial_income_krw: income.gross_financial_income_krw,
        withheld_income_tax_krw: income.withheld_income_tax_krw,
        withheld_local_income_tax_krw: income.withheld_local_income_tax_krw,
        comparison_a_income_tax_krw: calculated.comparison_a_income_tax_krw,
        comparison_a_local_income_tax_krw: calculated.comparison_a_local_income_tax_krw,
        comparison_b_income_tax_krw: calculated.comparison_b_income_tax_krw,
        comparison_b_local_income_tax_krw: calculated.comparison_b_local_income_tax_krw,
        assessed_income_tax_krw: calculated.assessed_income_tax_krw,
        assessed_local_income_tax_krw: calculated.assessed_local_income_tax_krw,
        additional_tax_krw: calculated.additional_tax_krw,
        refund_krw: calculated.refund_krw,
        filing_due_date: assessment.filing_due_date.map(|date| date.to_string()),
        filed_game_day: assessment.filed_game_day,
    })
}

fn to_ledger_page_response(page: LedgerPage) -> LedgerPageResponse {
    LedgerPageResponse {
        transactions: page
            .transactions
            .into_iter()
            .map(|transaction| LedgerTransactionSnapshot {
                id: transaction.id,
                game_day: transaction.game_day,
                description: transaction.description,
                source_kind: transaction.source_kind,
                postings: transaction
                    .postings
                    .into_iter()
                    .map(|posting| LedgerPostingSnapshot {
                        account_code: posting.account_code,
                        account_id: posting.financial_account_id,
                        amount_krw: posting.amount_krw,
                    })
                    .collect(),
            })
            .collect(),
        next_before: page.next_before,
    }
}

fn to_market_rates_snapshot(rates: &InterestRateState) -> MarketRatesSnapshot {
    MarketRatesSnapshot {
        policy_rate_bp: rates.policy_rate_bp,
        treasury_3m_bp: rates.treasury_3m_bp,
        treasury_1y_bp: rates.treasury_1y_bp,
        treasury_3y_bp: rates.treasury_3y_bp,
        treasury_10y_bp: rates.treasury_10y_bp,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use anyhow::Context;
    use tokio::sync::Semaphore;

    use super::*;
    use crate::auth::OAuthIdentity;
    use crate::character::{
        Character, CharacterDraft, Education, FamilyBackground, Gender, Health, MilitaryStatus,
        Region, create_character,
    };
    use crate::finance::{
        CommandCursor, CommandId, FinancialAccount, FinancialIncomeYear, PolicySet, RunId,
    };
    use crate::market::{create_default_market_generator, default_market_world};
    use crate::store::{MarketHistoryState, MarketWorldState, SaveState};
    use crate::trading::AccountId;

    const USER_ID: u64 = 7;
    const SAVE_ID: u64 = 11;
    const ACCOUNT_ID: u64 = 17;

    mod context_생애사건_effect를_공개하는_경우 {
        use super::*;

        #[test]
        fn given_지갑비용_when_json으로직렬화하면_then_camel_case금액을사용한다() {
            let effect = LifeEventEffectSummarySnapshot::WalletExpense {
                amount_krw: 120_000,
            };

            let result = serde_json::to_value(effect).expect("생애 사건 effect를 직렬화해야 한다");

            assert_eq!(
                result,
                serde_json::json!({ "kind": "walletExpense", "amountKrw": 120_000 })
            );
        }
    }

    mod context_보험계약_보장기간을_공개하는_경우 {
        use super::*;

        #[test]
        fn given_가입일에해지하여대기기간보다먼저종료된계약_when_변환하면_then_종료이력을허용한다()
        {
            let state = InsuranceContractState {
                id: ResourceId::from_u64(91),
                product_version_id: ResourceId::from_u64(71),
                product_key: "fictionalFamilyCareCover".to_owned(),
                display_name: "가족 돌봄 비용 보장".to_owned(),
                status: InsuranceContractStatusState::Cancelled,
                coverage_start_game_day: 0,
                waiting_ends_game_day: 7,
                coverage_end_exclusive: 1,
                next_premium_due_game_day: None,
                premium_krw: 10_000,
                paid_benefit_krw: 0,
                reserved_benefit_krw: 0,
                remaining_benefit_krw: 200_000,
            };

            let result = to_insurance_contract_snapshot(state)
                .expect("즉시 해지한 계약 이력을 공개할 수 있어야 한다");

            assert!(matches!(
                result.status,
                InsuranceContractStatusSnapshot::Cancelled
            ));
            assert_eq!(result.coverage_end_exclusive, 1);
        }
    }

    mod context_이전실행의_보험금청구를_재생하는_경우 {
        use super::*;

        #[tokio::test]
        async fn given_보험금지급일보다이른새실행_snapshot_when_응답을만들면_then_저장결과를허용한다()
         {
            let (state, _store, _timer) = given_state(Some(given_character("테스터")));
            let snapshot = state
                .snapshot(USER_ID)
                .await
                .expect("새 실행 snapshot을 읽어야 한다");
            let receipt = InsuranceClaimReceipt {
                command_id: CommandId::parse("c0841c03-2dd7-4ce6-a6e3-fbeefeb7a5dd")
                    .expect("표준 UUID여야 한다"),
                claim_id: ResourceId::from_u64(101),
                event_id: ResourceId::from_u64(81),
                payout_krw: 100_000,
                paid_game_day: 31,
                replayed: true,
            };

            let response = to_insurance_claim_response(receipt, snapshot)
                .expect("이전 실행의 보험금 지급 결과를 재생해야 한다");

            assert_eq!((response.replayed, response.snapshot.game_day), (true, 0));
        }
    }

    mod context_이전실행의_생애사건선택을_재생하는_경우 {
        use super::*;

        #[tokio::test]
        async fn given_사건해결일보다이른새실행_snapshot_when_응답을만들면_then_저장결과를허용한다()
        {
            let (state, _store, _timer) = given_state(Some(given_character("테스터")));
            let snapshot = state
                .snapshot(USER_ID)
                .await
                .expect("새 실행 snapshot을 읽어야 한다");
            let receipt = LifeEventChoiceReceipt {
                command_id: CommandId::parse("1bc6e6cf-02e4-4aad-a807-c46e9f79db75")
                    .expect("표준 UUID여야 한다"),
                event_id: ResourceId::from_u64(81),
                choice_id: ResourceId::from_u64(91),
                resolution_kind: LifeEventDecisionKindState::Accepted,
                resolved_game_day: 31,
                wallet_delta_krw: -120_000,
                replayed: true,
            };

            let response = to_life_event_choice_response(receipt, snapshot)
                .expect("이전 실행의 생애 사건 결과를 재생해야 한다");

            assert_eq!((response.replayed, response.snapshot.game_day), (true, 0));
        }
    }

    mod context_부동산_매도_체결_공개값을_검증하는_경우 {
        use super::*;

        #[test]
        fn given_순수령액이_waterfall과다를때_when_변환하면_then_거절한다() {
            let state = PropertySaleExecutionState {
                filled_game_day: 900,
                gross_sale_price_krw: 500_000_000,
                transaction_cost_krw: 2_500_000,
                mortgage_principal_krw: 200_000_000,
                mortgage_fee_krw: 1_000_000,
                capital_gains_tax_krw: 10_000_000,
                wallet_proceeds_krw: 286_500_001,
                realized_gain_loss_krw: 80_000_000,
            };

            let result = to_property_sale_execution_snapshot(state);

            assert!(result.is_err());
        }
    }

    mod context_부동산세_납부_공개값을_검증하는_경우 {
        use super::*;

        #[test]
        fn given_new_run으로취소한미납회차_when_변환하면_then_미지급상태를허용한다() {
            let state = PropertyTaxPaymentState {
                payment_no: 1,
                due_game_day: 120,
                paid_game_day: None,
                status: PropertyTaxPaymentStatusState::Cancelled,
                amount_krw: 300_000,
                wallet_paid_krw: 0,
                tax_obligation_krw: 0,
            };

            let result = to_property_tax_payment_snapshot(state)
                .expect("취소한 미납 회차는 공개할 수 있어야 한다");

            assert!(matches!(
                result.status,
                PropertyTaxPaymentStatusSnapshot::Cancelled
            ));
            assert_eq!(result.paid_game_day, None);
        }

        #[test]
        fn given_전액조달되지않은적용회차_when_변환하면_then_거절한다() {
            let state = PropertyTaxPaymentState {
                payment_no: 1,
                due_game_day: 120,
                paid_game_day: Some(120),
                status: PropertyTaxPaymentStatusState::Applied,
                amount_krw: 300_000,
                wallet_paid_krw: 200_000,
                tax_obligation_krw: 99_999,
            };

            let result = to_property_tax_payment_snapshot(state);

            assert!(result.is_err());
        }
    }

    struct FakeDailyPipeline {
        state: StdMutex<SaveState>,
        committed_days: StdMutex<Vec<u32>>,
        active_advances: AtomicUsize,
        max_active_advances: AtomicUsize,
        fail_next_load: AtomicBool,
        fail_next_advance: AtomicBool,
        fail_on_manual_step: AtomicUsize,
        start_commands: StdMutex<HashMap<String, StartGameCommand>>,
        start_receipts: StdMutex<HashMap<String, StartGameReceipt>>,
        manual_commands: StdMutex<HashMap<String, (ManualAdvanceCommand, u32)>>,
        manual_receipts: StdMutex<HashMap<String, AdvanceCommandReceipt>>,
    }

    impl FakeDailyPipeline {
        fn new(character: Option<Character>) -> Self {
            Self {
                state: StdMutex::new(SaveState {
                    save_id: SAVE_ID,
                    market_world_id: 1,
                    policy_set: given_policy_set(),
                    run_revision: 0,
                    state_revision: 0,
                    game_day: 0,
                    cash_krw: 10_000_000,
                    debt_krw: 0,
                    property_book_value_krw: 0,
                    accounts: vec![given_financial_account(0)],
                    positions: Vec::new(),
                    pending_settlements: Vec::new(),
                    cma_accounts: Vec::new(),
                    cash_contracts: Vec::new(),
                    deposit_protection: Vec::new(),
                    current_financial_income_year: FinancialIncomeYear::zero(2026),
                    current_annual_tax_year: crate::store::AnnualTaxYearState::empty_not_applicable(
                        2026,
                    ),
                    latest_financial_income_assessment: None,
                    m2d_assets: crate::finance::M2dAssetSnapshot::default(),
                    isa_accounts: Vec::new(),
                    pension_accounts: Vec::new(),
                    career: crate::store::CareerSnapshotState::empty(
                        "softwareEngineering".to_owned(),
                    ),
                    life: LifeSnapshotState::empty(),
                    character,
                }),
                committed_days: StdMutex::new(Vec::new()),
                active_advances: AtomicUsize::new(0),
                max_active_advances: AtomicUsize::new(0),
                fail_next_load: AtomicBool::new(false),
                fail_next_advance: AtomicBool::new(false),
                fail_on_manual_step: AtomicUsize::new(0),
                start_commands: StdMutex::new(HashMap::new()),
                start_receipts: StdMutex::new(HashMap::new()),
                manual_commands: StdMutex::new(HashMap::new()),
                manual_receipts: StdMutex::new(HashMap::new()),
            }
        }

        fn state(&self) -> SaveState {
            self.state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }

        fn committed_days(&self) -> Vec<u32> {
            self.committed_days
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }

        fn max_active_advances(&self) -> usize {
            self.max_active_advances.load(Ordering::SeqCst)
        }

        fn fail_next_advance(&self) {
            self.fail_next_advance.store(true, Ordering::SeqCst);
        }

        fn fail_on_manual_step(&self, step_no: usize) {
            self.fail_on_manual_step.store(step_no, Ordering::SeqCst);
        }

        fn fail_next_load(&self) {
            self.fail_next_load.store(true, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl DailyPipeline for FakeDailyPipeline {
        async fn load(&self, _user_id: u64) -> Result<CommittedGameState> {
            if self.fail_next_load.swap(false, Ordering::SeqCst) {
                anyhow::bail!("injected save load failure");
            }
            committed_state(self.state())
        }

        async fn start_game(
            &self,
            _user_id: u64,
            command: &StartGameCommand,
        ) -> Result<DailyStartGameResult> {
            if let Some(receipt) = self
                .start_receipts
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(command.command_id.as_str())
                .cloned()
            {
                let commands = self
                    .start_commands
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if commands.get(command.command_id.as_str()) != Some(command) {
                    return Ok(DailyStartGameResult::Rejected(
                        GameCommandRejection::IdempotencyConflict,
                    ));
                }
                let mut receipt = receipt;
                receipt.replayed = true;
                return Ok(DailyStartGameResult::Replayed {
                    state: Box::new(committed_state(self.state())?),
                    receipt,
                });
            }
            let character = match create_character(command.draft.clone()) {
                Ok(character) => character,
                Err(errors) => {
                    return Ok(DailyStartGameResult::Rejected(
                        GameCommandRejection::InvalidCharacter(errors),
                    ));
                }
            };
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.run_revision += 1;
            state.state_revision = 0;
            state.game_day = 0;
            state.cash_krw = character.cash_krw;
            state.debt_krw = character.debt_krw;
            state.accounts = vec![given_financial_account(state.run_revision)];
            state.positions.clear();
            state.pending_settlements.clear();
            state.cma_accounts.clear();
            state.cash_contracts.clear();
            state.deposit_protection.clear();
            state.isa_accounts.clear();
            state.pension_accounts.clear();
            state.character = Some(character);

            let committed = state.clone();
            drop(state);

            let committed = committed_state(committed)?;
            let receipt = StartGameReceipt {
                command_id: command.command_id.clone(),
                committed_cursor: GameCommandCursor::from(&committed.save),
                replayed: false,
            };
            self.start_commands
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(command.command_id.to_string(), command.clone());
            self.start_receipts
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(command.command_id.to_string(), receipt.clone());
            Ok(DailyStartGameResult::Applied {
                receipt,
                state: Box::new(committed),
            })
        }

        async fn advance_one_day(&self, _user_id: u64) -> Result<DailyAdvanceResult> {
            if self.state().character.is_none() {
                return Ok(DailyAdvanceResult::CharacterRequired);
            }
            if self.fail_next_advance.swap(false, Ordering::SeqCst) {
                anyhow::bail!("injected daily commit failure");
            }

            let active = self.active_advances.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active_advances.fetch_max(active, Ordering::SeqCst);
            tokio::task::yield_now().await;

            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.game_day += 1;
            state.state_revision += 1;
            let committed = state.clone();
            self.committed_days
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(state.game_day);
            self.active_advances.fetch_sub(1, Ordering::SeqCst);

            Ok(DailyAdvanceResult::Advanced(Box::new(committed_state(
                committed,
            )?)))
        }

        async fn advance_command_step(
            &self,
            _user_id: u64,
            command: &ManualAdvanceCommand,
        ) -> Result<DailyCommandAdvanceResult> {
            if let Some(receipt) = self
                .manual_receipts
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(command.command_id.as_str())
                .cloned()
            {
                if receipt.requested_days != command.days
                    || receipt.initial_cursor != GameCommandCursor::from(command.cursor)
                {
                    return Ok(DailyCommandAdvanceResult::Rejected(
                        GameCommandRejection::IdempotencyConflict,
                    ));
                }
                let mut receipt = receipt;
                receipt.replayed = true;
                return Ok(DailyCommandAdvanceResult::Replayed {
                    state: Box::new(committed_state(self.state())?),
                    receipt,
                });
            }
            if !(1..=30).contains(&command.days) {
                return Ok(DailyCommandAdvanceResult::Rejected(
                    GameCommandRejection::InvalidCommand,
                ));
            }
            if self.state().character.is_none() {
                return Ok(DailyCommandAdvanceResult::Rejected(
                    GameCommandRejection::CharacterRequired,
                ));
            }
            if self.fail_next_advance.swap(false, Ordering::SeqCst) {
                anyhow::bail!("injected daily commit failure");
            }

            let active = self.active_advances.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active_advances.fetch_max(active, Ordering::SeqCst);
            tokio::task::yield_now().await;

            let initial_cursor = GameCommandCursor::from(command.cursor);
            let mut commands = self
                .manual_commands
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let completed = match commands.get(command.command_id.as_str()) {
                Some((stored, completed)) if stored == command => *completed,
                Some(_) => {
                    self.active_advances.fetch_sub(1, Ordering::SeqCst);
                    return Ok(DailyCommandAdvanceResult::Rejected(
                        GameCommandRejection::IdempotencyConflict,
                    ));
                }
                None => {
                    commands.insert(command.command_id.to_string(), (command.clone(), 0));
                    0
                }
            };
            if self.fail_on_manual_step.load(Ordering::SeqCst)
                == usize::try_from(completed + 1).expect("테스트 step 번호여야 한다")
            {
                self.fail_on_manual_step.store(0, Ordering::SeqCst);
                self.active_advances.fetch_sub(1, Ordering::SeqCst);
                anyhow::bail!("injected manual step failure");
            }
            let expected_cursor = GameCommandCursor {
                run_revision: initial_cursor.run_revision,
                state_revision: initial_cursor.state_revision + u64::from(completed),
                game_day: initial_cursor.game_day + completed,
            };
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if GameCommandCursor::from(&*state) != expected_cursor {
                self.active_advances.fetch_sub(1, Ordering::SeqCst);
                return Ok(DailyCommandAdvanceResult::Rejected(
                    GameCommandRejection::Busy,
                ));
            }

            state.game_day += 1;
            state.state_revision += 1;
            let committed_save = state.clone();
            self.committed_days
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(state.game_day);
            let completed = completed + 1;
            commands.insert(command.command_id.to_string(), (command.clone(), completed));
            self.active_advances.fetch_sub(1, Ordering::SeqCst);
            drop(state);
            drop(commands);

            let committed = committed_state(committed_save)?;
            let receipt = if completed == command.days {
                let receipt = AdvanceCommandReceipt {
                    command_id: command.command_id.clone(),
                    requested_days: command.days,
                    initial_cursor,
                    committed_cursor: GameCommandCursor::from(&committed.save),
                    replayed: false,
                };
                self.manual_receipts
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(command.command_id.to_string(), receipt.clone());
                Some(receipt)
            } else {
                None
            };

            Ok(DailyCommandAdvanceResult::Advanced {
                state: Box::new(committed),
                receipt,
            })
        }
    }

    fn committed_state(save: SaveState) -> Result<CommittedGameState> {
        let world = default_market_world()?;
        let generator = create_default_market_generator()?;
        let day_zero = generator.day_zero(&world)?;
        let market = if save.game_day == 0 {
            day_zero
        } else {
            generator
                .generate_through(&world, &day_zero, save.game_day)?
                .pop()
                .context("test market path must reach the save game day")?
        };

        Ok(CommittedGameState {
            save,
            world,
            market,
        })
    }

    struct FakeUserStore;

    struct FakeRunStore;

    #[async_trait]
    impl RunStore for FakeRunStore {
        async fn run_options(&self) -> Result<RunOptions> {
            Ok(RunOptions {
                modes: Vec::new(),
                active_season_id: None,
                presets: Vec::new(),
                point_budgets: Vec::new(),
                sandbox_available: true,
            })
        }

        async fn preview_point_budget(
            &self,
            _version_id: ResourceId,
            _selections: &[PointSelection],
        ) -> Result<Option<PointBudgetEvaluation>> {
            Ok(None)
        }
    }

    #[async_trait]
    impl UserStore for FakeUserStore {
        async fn upsert(&self, _identity: &OAuthIdentity) -> Result<AccountUser> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn open_session(
            &self,
            _user_id: u64,
            _token_hash: &str,
            _ttl: Duration,
        ) -> Result<()> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn find_by_session(&self, _token_hash: &str) -> Result<Option<AccountUser>> {
            Ok(None)
        }

        async fn close_session(&self, _token_hash: &str) -> Result<()> {
            Ok(())
        }
    }

    struct FakeTradingStore;

    #[async_trait]
    impl TradingStore for FakeTradingStore {
        async fn execute(&self, _user_id: u64, _order: &TradeOrder) -> Result<TradeStoreResult> {
            anyhow::bail!("not used by game-loop tests")
        }
    }

    struct FakeFinanceStore;

    #[async_trait]
    impl FinanceStore for FakeFinanceStore {
        async fn transfer(
            &self,
            _user_id: u64,
            _command: &TransferCommand,
        ) -> Result<FinanceStoreResult> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn ledger_page(
            &self,
            _user_id: u64,
            _before: Option<u64>,
            _limit: u32,
        ) -> Result<LedgerPage> {
            anyhow::bail!("not used by game-loop tests")
        }
    }

    struct FakeCashProductStore;

    struct FakeM2dAssetStore;

    #[async_trait]
    impl M2dAssetStore for FakeM2dAssetStore {
        async fn bond_catalog(&self, _user_id: u64) -> Result<BondCatalog> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn place_bond_order(
            &self,
            _user_id: u64,
            _command: &BondOrderCommand,
        ) -> Result<M2dAssetCommandResult<crate::finance::BondOrderResponse>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn gold_catalog(&self, _user_id: u64) -> Result<GoldCatalog> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn open_gold_account(
            &self,
            _user_id: u64,
            _command: &OpenGoldAccountCommand,
        ) -> Result<M2dAssetCommandResult<crate::finance::OpenGoldAccountResponse>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn place_gold_order(
            &self,
            _user_id: u64,
            _command: &GoldOrderCommand,
        ) -> Result<M2dAssetCommandResult<crate::finance::GoldOrderResponse>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn withdraw_gold(
            &self,
            _user_id: u64,
            _command: &GoldWithdrawalCommand,
        ) -> Result<M2dAssetCommandResult<crate::finance::GoldWithdrawalResponse>> {
            anyhow::bail!("not used by game-loop tests")
        }
    }

    #[async_trait]
    impl CashProductStore for FakeCashProductStore {
        async fn cash_product_catalog(&self) -> Result<CashProductCatalog> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn open_cma_account(
            &self,
            _user_id: u64,
            _command: &OpenCmaAccountCommand,
        ) -> Result<CashProductStoreResult<OpenCmaAccountReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn close_cma_account(
            &self,
            _user_id: u64,
            _command: &CloseCmaAccountCommand,
        ) -> Result<CashProductStoreResult<CloseCmaAccountReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn open_cash_product(
            &self,
            _user_id: u64,
            _command: &OpenCashProductCommand,
        ) -> Result<CashProductStoreResult<OpenCashProductReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn close_cash_product(
            &self,
            _user_id: u64,
            _command: &CloseCashProductCommand,
        ) -> Result<CashProductStoreResult<CloseCashProductReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn financial_income_year(
            &self,
            _user_id: u64,
            _tax_year: u16,
        ) -> Result<AnnualTaxYearState> {
            anyhow::bail!("not used by game-loop tests")
        }
    }

    struct FakeTaxAccountStore;

    #[async_trait]
    impl TaxAccountStore for FakeTaxAccountStore {
        async fn open_tax_account(
            &self,
            _user_id: u64,
            _command: &OpenTaxAccountCommand,
        ) -> Result<TaxAccountStoreResult<OpenTaxAccountReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn close_isa_account(
            &self,
            _user_id: u64,
            _command: &CloseIsaAccountCommand,
        ) -> Result<TaxAccountStoreResult<CloseIsaAccountReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn start_pension(
            &self,
            _user_id: u64,
            _command: &StartPensionCommand,
        ) -> Result<TaxAccountStoreResult<StartPensionReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn withdraw_pension(
            &self,
            _user_id: u64,
            _command: &PensionWithdrawalCommand,
        ) -> Result<TaxAccountStoreResult<PensionWithdrawalReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }
    }

    struct FakeCareerStore;

    #[async_trait]
    impl CareerStore for FakeCareerStore {
        async fn specs(
            &self,
            _user_id: u64,
            _query: crate::store::CareerPageQuery,
        ) -> Result<crate::store::CareerSpecsState> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn activities(
            &self,
            _user_id: u64,
            _query: crate::store::CareerPageQuery,
        ) -> Result<crate::store::CareerActivitiesState> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn artifacts(
            &self,
            _user_id: u64,
            _query: crate::store::CareerArtifactPageQuery,
        ) -> Result<crate::store::CareerArtifactPageState> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn focus(
            &self,
            _user_id: u64,
            _command: &crate::store::FocusCareerCommand,
        ) -> Result<crate::store::CareerStoreResult<crate::store::FocusCareerReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn start_activity(
            &self,
            _user_id: u64,
            _command: &crate::store::StartCareerActivityCommand,
        ) -> Result<crate::store::CareerStoreResult<crate::store::CareerActivityReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn cancel_activity(
            &self,
            _user_id: u64,
            _command: &crate::store::CancelCareerActivityCommand,
        ) -> Result<crate::store::CareerStoreResult<crate::store::CareerActivityReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn publish_artifact(
            &self,
            _user_id: u64,
            _command: &crate::store::PublishCareerArtifactCommand,
        ) -> Result<crate::store::CareerStoreResult<crate::store::CareerArtifactReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }
    }

    struct FakeLifeStore;

    #[async_trait]
    impl LifeStore for FakeLifeStore {
        async fn life_events(
            &self,
            _user_id: u64,
            _query: LifeEventsQueryState,
        ) -> Result<LifeEventsReadResult> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn resolve_life_event(
            &self,
            _user_id: u64,
            _command: &ResolveLifeEventCommand,
        ) -> Result<LifeStoreResult<LifeEventChoiceReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn insurance(
            &self,
            _user_id: u64,
            _query: InsuranceQueryState,
        ) -> Result<InsuranceReadResult> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn enroll_insurance_contract(
            &self,
            _user_id: u64,
            _command: &EnrollInsuranceContractCommand,
        ) -> Result<LifeStoreResult<InsuranceEnrollmentReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn cancel_insurance_contract(
            &self,
            _user_id: u64,
            _command: &CancelInsuranceContractCommand,
        ) -> Result<LifeStoreResult<InsuranceCancellationReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn file_insurance_claim(
            &self,
            _user_id: u64,
            _command: &FileInsuranceClaimCommand,
        ) -> Result<LifeStoreResult<InsuranceClaimReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn welfare_programs(&self, _user_id: u64) -> Result<Option<WelfareProgramsState>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn apply_welfare_program(
            &self,
            _user_id: u64,
            _command: &ApplyWelfareProgramCommand,
        ) -> Result<LifeStoreResult<WelfareApplicationReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn housing_listings(
            &self,
            _user_id: u64,
            _query: HousingListingsQueryState,
        ) -> Result<Option<HousingListingsState>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn housing_lease_current(
            &self,
            _user_id: u64,
        ) -> Result<Option<HousingLeaseCurrentState>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn start_housing_lease(
            &self,
            _user_id: u64,
            _command: &StartHousingLeaseCommand,
        ) -> Result<LifeStoreResult<HousingLeaseMoveReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn housing_property_holdings(
            &self,
            _user_id: u64,
        ) -> Result<Option<HousingPropertyHoldingsState>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn quote_mortgage(
            &self,
            _user_id: u64,
            _command: &CreateMortgageQuoteCommand,
        ) -> Result<LifeStoreResult<MortgageQuoteReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn purchase_property(
            &self,
            _user_id: u64,
            _command: &PurchasePropertyCommand,
        ) -> Result<LifeStoreResult<PropertyPurchaseReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn create_property_sale_order(
            &self,
            _user_id: u64,
            _command: &CreatePropertySaleOrderCommand,
        ) -> Result<LifeStoreResult<PropertySaleOrderListingReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn reprice_property_sale_order(
            &self,
            _user_id: u64,
            _command: &RepricePropertySaleOrderCommand,
        ) -> Result<LifeStoreResult<PropertySaleOrderListingReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn cancel_property_sale_order(
            &self,
            _user_id: u64,
            _command: &CancelPropertySaleOrderCommand,
        ) -> Result<LifeStoreResult<PropertySaleOrderCancellationReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn property_sale_orders(
            &self,
            _user_id: u64,
            _query: PropertySaleOrderPageQuery,
        ) -> Result<Option<PropertySaleOrderPageState>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn property_tax_events(
            &self,
            _user_id: u64,
            _holding_id: ResourceId,
            _query: PropertyTaxEventPageQuery,
        ) -> Result<Option<PropertyTaxEventPageState>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn loan_products(&self, _user_id: u64) -> Result<LoanProductCatalogState> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn loan_detail(
            &self,
            _user_id: u64,
            _loan_id: ResourceId,
        ) -> Result<Option<LoanDetailState>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn loan_installments(
            &self,
            _user_id: u64,
            _loan_id: ResourceId,
            _query: LoanInstallmentPageQuery,
        ) -> Result<Option<LoanInstallmentPageState>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn credit(&self, _user_id: u64) -> Result<CreditOverviewState> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn quote_loan(
            &self,
            _user_id: u64,
            _command: &CreateLoanQuoteCommand,
        ) -> Result<LifeStoreResult<LoanQuoteReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn quote_lease_deposit_loan(
            &self,
            _user_id: u64,
            _command: &CreateLeaseDepositLoanQuoteCommand,
        ) -> Result<LifeStoreResult<LeaseDepositLoanQuoteReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn execute_loan(
            &self,
            _user_id: u64,
            _command: &ExecuteLoanCommand,
        ) -> Result<LifeStoreResult<LoanExecutionReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn prepay_loan(
            &self,
            _user_id: u64,
            _command: &PrepayLoanCommand,
        ) -> Result<LifeStoreResult<LoanPrepaymentReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn budget(&self, _user_id: u64) -> Result<LifeBudgetState> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn update_budget(
            &self,
            _user_id: u64,
            _command: &UpdateLifeBudgetCommand,
        ) -> Result<LifeStoreResult<UpdateLifeBudgetReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn pay_essential_arrear(
            &self,
            _user_id: u64,
            _command: &PayEssentialArrearCommand,
        ) -> Result<LifeStoreResult<EssentialArrearPaymentReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn pay_lease_arrear(
            &self,
            _user_id: u64,
            _command: &PayLeaseArrearCommand,
        ) -> Result<LifeStoreResult<LeaseArrearPaymentReceipt>> {
            anyhow::bail!("not used by game-loop tests")
        }
    }

    struct RecoverableTradingStore {
        games: Arc<FakeDailyPipeline>,
        executed: StdMutex<Option<TradeOrder>>,
    }

    impl RecoverableTradingStore {
        fn new(games: Arc<FakeDailyPipeline>) -> Self {
            Self {
                games,
                executed: StdMutex::new(None),
            }
        }
    }

    #[async_trait]
    impl TradingStore for RecoverableTradingStore {
        async fn execute(&self, _user_id: u64, order: &TradeOrder) -> Result<TradeStoreResult> {
            let replayed = {
                let mut executed = self
                    .executed
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                match executed.as_ref() {
                    Some(stored) if stored != order => {
                        return Ok(TradeStoreResult::Rejected(
                            TradeFailure::idempotency_conflict(),
                        ));
                    }
                    Some(_) => true,
                    None => {
                        if order.symbol() != crate::trading::LLX_SYMBOL
                            || !(1..=crate::trading::MAX_TRADE_QUANTITY).contains(&order.quantity)
                        {
                            return Ok(TradeStoreResult::Rejected(TradeFailure::invalid_order(
                                "주문 형식이 올바르지 않습니다",
                            )));
                        }
                        *executed = Some(order.clone());
                        false
                    }
                }
            };
            if !replayed {
                let mut save = self
                    .games
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                save.cash_krw -= 100_000;
                save.state_revision += 1;
                save.positions = vec![crate::trading::PositionState {
                    account_id: order.account_id,
                    symbol: crate::trading::LLX_SYMBOL.to_owned(),
                    quantity: 1,
                    cost_basis_krw: 100_000,
                }];
            }

            Ok(TradeStoreResult::Executed {
                execution: TradeExecution {
                    order_id: order.order_id.as_str().to_owned(),
                    account_id: order.account_id,
                    symbol: order.symbol().to_owned(),
                    side: order.side,
                    quantity: order.quantity,
                    price_krw: 100_000,
                    gross_amount_krw: 100_000,
                    fee_krw: 0,
                    tax_krw: 0,
                    removed_cost_basis_krw: 0,
                    realized_gain_loss_krw: 0,
                    replayed,
                },
                save: Box::new(self.games.state()),
            })
        }
    }

    struct FakeMarketStore;

    #[async_trait]
    impl MarketStore for FakeMarketStore {
        async fn load_world(&self, _world_id: u64) -> Result<MarketWorldState> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn ensure_day(
            &self,
            _world_id: u64,
            _target_game_day: u32,
        ) -> Result<crate::market::MarketDay> {
            anyhow::bail!("not used by game-loop tests")
        }

        async fn history_for_user(&self, _user_id: u64, _limit: u32) -> Result<MarketHistoryState> {
            anyhow::bail!("not used by game-loop tests")
        }
    }

    struct ManualTimer {
        waits: StdMutex<Vec<Duration>>,
        permits: Semaphore,
        wait_count: AtomicUsize,
    }

    impl ManualTimer {
        fn new() -> Self {
            Self {
                waits: StdMutex::new(Vec::new()),
                permits: Semaphore::new(0),
                wait_count: AtomicUsize::new(0),
            }
        }

        fn waits(&self) -> Vec<Duration> {
            self.waits
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }

        fn release_one(&self) {
            self.permits.add_permits(1);
        }

        async fn wait_until_armed(&self, count: usize) {
            for _ in 0..1_000 {
                if self.wait_count.load(Ordering::SeqCst) >= count {
                    return;
                }
                tokio::task::yield_now().await;
            }
            panic!("timer was not armed {count} times");
        }
    }

    #[async_trait]
    impl GameTimer for ManualTimer {
        async fn wait(&self, duration: Duration) {
            self.waits
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(duration);
            self.wait_count.fetch_add(1, Ordering::SeqCst);
            let permit = self
                .permits
                .acquire()
                .await
                .expect("manual timer semaphore must stay open");
            permit.forget();
        }
    }

    fn given_character(name: &str) -> Character {
        Character {
            name: name.to_owned(),
            age: 25,
            gender: Gender::Other,
            military: MilitaryStatus::Exempted,
            region: Region::CapitalArea,
            background: FamilyBackground::Independent,
            education: Education::Bachelor,
            career_years: 1,
            certifications: 1,
            cash_krw: 10_000_000,
            debt_krw: 0,
            health: Health::Normal,
            dependents: 0,
        }
    }

    fn given_character_draft(name: &str) -> CharacterDraft {
        CharacterDraft {
            name: name.to_owned(),
            age: 25,
            gender: Gender::Other,
            military: MilitaryStatus::Exempted,
            region: Region::CapitalArea,
            background: FamilyBackground::Independent,
            education: Education::Bachelor,
            career_years: 1,
            certifications: 1,
            starting_cash_krw: 10_000_000,
            student_loan_krw: 0,
            credit_loan_krw: 0,
            health: Health::Normal,
            dependents: 0,
        }
    }

    fn given_advance_command(days: u32) -> ManualAdvanceCommand {
        given_advance_command_with_id(days, "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2")
    }

    fn given_advance_command_with_id(days: u32, command_id: &str) -> ManualAdvanceCommand {
        ManualAdvanceCommand {
            command_id: CommandId::parse(command_id).expect("표준 UUID여야 한다"),
            cursor: CommandCursor {
                expected_run_revision: 0,
                expected_state_revision: 0,
                expected_game_day: 0,
            },
            days,
        }
    }

    fn given_start_game_command(name: &str) -> StartGameCommand {
        StartGameCommand {
            command_id: CommandId::parse("b6a1cc9d-3c87-44a9-aebe-9ff46677f043")
                .expect("표준 UUID여야 한다"),
            cursor: CommandCursor {
                expected_run_revision: 0,
                expected_state_revision: 0,
                expected_game_day: 0,
            },
            draft: given_character_draft(name),
            starting_loans: None,
        }
    }

    fn given_policy_set() -> PolicySet {
        PolicySet {
            id: ResourceId::from_u64(1),
            key: "kr-individual-2026-v1".to_owned(),
            basis_date: "2026-01-01".to_owned(),
            sealed: true,
        }
    }

    fn given_financial_account(run_revision: u32) -> FinancialAccount {
        FinancialAccount {
            id: ResourceId::from_u64(ACCOUNT_ID),
            run: RunId {
                save_id: ResourceId::from_u64(SAVE_ID),
                run_revision,
            },
            account_type: FinancialAccountType::TaxableBrokerage,
            status: FinancialAccountStatus::Open,
            is_default: true,
            cash_krw: 0,
        }
    }

    fn given_account_id() -> AccountId {
        AccountId::from_u64(ACCOUNT_ID).expect("테스트 계좌 ID는 0이 아니어야 한다")
    }

    fn given_state(
        character: Option<Character>,
    ) -> (Arc<AppState>, Arc<FakeDailyPipeline>, Arc<ManualTimer>) {
        let store = Arc::new(FakeDailyPipeline::new(character));
        let games: Arc<dyn DailyPipeline> = store.clone();
        let trades: Arc<dyn TradingStore> = Arc::new(FakeTradingStore);
        let finances: Arc<dyn FinanceStore> = Arc::new(FakeFinanceStore);
        let cash_products: Arc<dyn CashProductStore> = Arc::new(FakeCashProductStore);
        let assets: Arc<dyn M2dAssetStore> = Arc::new(FakeM2dAssetStore);
        let tax_accounts: Arc<dyn TaxAccountStore> = Arc::new(FakeTaxAccountStore);
        let careers: Arc<dyn CareerStore> = Arc::new(FakeCareerStore);
        let lives: Arc<dyn LifeStore> = Arc::new(FakeLifeStore);
        let markets: Arc<dyn MarketStore> = Arc::new(FakeMarketStore);
        let runs: Arc<dyn RunStore> = Arc::new(FakeRunStore);
        let users: Arc<dyn UserStore> = Arc::new(FakeUserStore);
        let timer = Arc::new(ManualTimer::new());
        let game_timer: Arc<dyn GameTimer> = timer.clone();
        let providers = Providers::from_env("http://localhost:8080".to_owned())
            .expect("test provider configuration must be valid");
        let state = AppState::new_with_timer(
            AppStateDependencies {
                stores: create_app_stores(AppStoreDependencies {
                    games,
                    trades,
                    finances,
                    cash_products,
                    assets,
                    tax_accounts,
                    careers,
                    lives,
                    markets,
                    runs,
                    users,
                }),
                providers,
            },
            game_timer,
        );

        (state, store, timer)
    }

    async fn when_game_day_reaches(store: &FakeDailyPipeline, expected: u32) {
        for _ in 0..1_000 {
            if store.state().game_day == expected {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("game day did not reach {expected}");
    }

    async fn when_tick_arrives(receiver: &mut broadcast::Receiver<GameSnapshot>) -> GameSnapshot {
        for _ in 0..1_000 {
            match receiver.try_recv() {
                Ok(snapshot) => return snapshot,
                Err(broadcast::error::TryRecvError::Empty) => tokio::task::yield_now().await,
                Err(error) => panic!("tick stream failed before the expected snapshot: {error}"),
            }
        }
        panic!("expected tick did not arrive");
    }

    mod context_cash_product_principal_is_valued {
        use super::*;

        #[test]
        fn given_an_active_term_deposit_when_snapshotted_then_principal_stays_in_net_worth() {
            let mut save = FakeDailyPipeline::new(Some(given_character("테스터"))).state();
            save.cash_contracts = vec![CashProductContractState {
                contract_id: ResourceId::from_u64(31),
                product_version_id: ResourceId::from_u64(41),
                settlement_account_id: ResourceId::from_u64(ACCOUNT_ID),
                kind: CashProductKind::TermDeposit,
                status: CashProductContractStatus::Active,
                installment_amount_krw: None,
                annual_rate_bp: 300,
                current_principal_krw: 500_000,
                opened_game_day: 0,
                maturity_game_day: 365,
                paid_installment_count: 0,
                missed_installment_count: 0,
                expected_gross_interest_krw: Some(15_000),
                expected_income_tax_krw: Some(2_100),
                expected_local_income_tax_krw: Some(210),
                expected_net_payout_krw: Some(512_690),
            }];
            let state = committed_state(save).expect("테스트 시장 상태를 만들 수 있어야 한다");

            let snapshot = to_snapshot(&state, None).expect("순자산을 계산할 수 있어야 한다");

            assert_eq!(snapshot.net_worth_krw, 10_500_000);
            assert_eq!(
                snapshot.finance.cash_contracts[0].current_principal_krw,
                500_000
            );
        }
    }

    mod context_소유주택_장부가가_있는_경우 {
        use super::*;

        #[test]
        fn given_현금매수_주택_when_snapshot_then_순자산에_포함한다() {
            let mut save = FakeDailyPipeline::new(Some(given_character("테스터"))).state();
            let holding_id = ResourceId::from_u64(51);
            save.property_book_value_krw = 8_000_000;
            save.life.residence = Some(LifeResidenceState {
                id: ResourceId::from_u64(52),
                region_key: "smallCity".to_owned(),
                tenure_kind: ResidenceTenureKind::Owner,
                property_holding_id: Some(holding_id),
                effective_from_game_day: 0,
            });
            save.life.active_property_holdings = vec![PropertyHoldingState {
                id: holding_id,
                listing_id: ResourceId::from_u64(53),
                status: PropertyHoldingStatusState::Active,
                purpose: PropertyHoldingPurposeState::OwnerOccupied,
                region_key: LifeRegionKey::SmallCity,
                property_type: PropertyType::Apartment,
                exclusive_area_square_meters: 59,
                acquired_game_day: 0,
                acquisition_price_krw: 8_000_000,
                acquisition_incidental_cost_krw: 80_000,
                book_value_krw: 8_000_000,
                mortgage_loan_id: None,
            }];
            save.life.total_property_book_value_krw = 8_000_000;
            let state = committed_state(save).expect("테스트 시장 상태를 만들 수 있어야 한다");

            let snapshot = to_snapshot(&state, None).expect("순자산을 계산할 수 있어야 한다");

            assert_eq!(snapshot.net_worth_krw, 18_000_000);
            assert_eq!(snapshot.life.total_property_book_value_krw, 8_000_000);
        }
    }

    mod context_runtime_control_is_published {
        use super::*;

        #[test]
        fn given_concurrent_start_and_disconnect_when_published_then_control_and_watch_never_diverge()
         {
            let runtime = Arc::new(SaveRuntime::new());
            let committed =
                committed_state(FakeDailyPipeline::new(Some(given_character("테스터"))).state())
                    .expect("test state must have a market");
            let mismatch = AtomicBool::new(false);
            let workers_done = AtomicBool::new(false);

            std::thread::scope(|scope| {
                let observer = scope.spawn(|| {
                    while !workers_done.load(Ordering::SeqCst) {
                        if !runtime.control_matches_published_signal() {
                            mismatch.store(true, Ordering::SeqCst);
                            return;
                        }
                        std::thread::yield_now();
                    }
                });
                let workers = (0..4)
                    .map(|worker| {
                        let runtime = Arc::clone(&runtime);
                        let committed = committed.clone();
                        scope.spawn(move || {
                            for iteration in 0..500 {
                                let connection = runtime.connect();
                                let speed = if (worker + iteration) % 2 == 0 {
                                    AutoSpeed::X2
                                } else {
                                    AutoSpeed::X8
                                };
                                runtime
                                    .start(speed, &committed)
                                    .expect("the worker owns an active stream");
                                std::thread::yield_now();
                                drop(connection);
                            }
                        })
                    })
                    .collect::<Vec<_>>();

                for worker in workers {
                    worker.join().expect("control worker must finish");
                }
                workers_done.store(true, Ordering::SeqCst);
                observer.join().expect("control observer must finish");
            });

            assert!(!mismatch.load(Ordering::SeqCst));
            assert!(runtime.control_matches_published_signal());
        }
    }

    mod context_users_have_independent_tick_streams {
        use super::*;

        #[tokio::test]
        async fn given_many_ticks_for_one_user_when_another_user_waits_then_they_do_not_arrive_or_lag()
         {
            let (state, store, _timer) = given_state(Some(given_character("테스터")));
            let first = state
                .open_stream(USER_ID)
                .await
                .expect("first stream must open");
            let second = state
                .open_stream(USER_ID + 1)
                .await
                .expect("second stream must open");
            let (_, mut first_receiver, _first_connection) = first.into_parts();
            let (_, mut second_receiver, _second_connection) = second.into_parts();
            let first_runtime = state.runtime(USER_ID);
            let mut committed =
                committed_state(store.state()).expect("test state must have a market");

            for game_day in 1..=257 {
                committed.save.game_day = game_day;
                committed.save.state_revision = u64::from(game_day);
                state
                    .broadcast(&committed, &first_runtime)
                    .expect("test snapshot must be valid");
            }

            assert!(matches!(
                first_receiver.try_recv(),
                Err(broadcast::error::TryRecvError::Lagged(_))
            ));
            assert!(matches!(
                second_receiver.try_recv(),
                Err(broadcast::error::TryRecvError::Empty)
            ));
        }
    }

    mod context_an_order_commit_needs_response_recovery {
        use super::*;
        use crate::trading::{OrderSide, TradeFailureCode, TradeOrderRequest};

        fn given_order_request(order_id: &str) -> TradeOrderRequest {
            TradeOrderRequest {
                order_id: order_id.to_owned(),
                account_id: given_account_id().get().to_string(),
                expected_run_revision: 0,
                expected_state_revision: 0,
                expected_game_day: 0,
                side: OrderSide::Buy,
                symbol: crate::trading::LLX_SYMBOL.to_owned(),
                quantity: 1,
            }
        }

        fn given_order() -> TradeOrder {
            TradeOrder::try_from(given_order_request("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2"))
                .expect("테스트 주문은 유효해야 한다")
        }

        fn given_order_state() -> (Arc<AppState>, Arc<FakeDailyPipeline>) {
            let games = Arc::new(FakeDailyPipeline::new(Some(given_character("테스터"))));
            let game_pipeline: Arc<dyn DailyPipeline> = games.clone();
            let trades: Arc<dyn TradingStore> =
                Arc::new(RecoverableTradingStore::new(games.clone()));
            let finances: Arc<dyn FinanceStore> = Arc::new(FakeFinanceStore);
            let cash_products: Arc<dyn CashProductStore> = Arc::new(FakeCashProductStore);
            let assets: Arc<dyn M2dAssetStore> = Arc::new(FakeM2dAssetStore);
            let tax_accounts: Arc<dyn TaxAccountStore> = Arc::new(FakeTaxAccountStore);
            let careers: Arc<dyn CareerStore> = Arc::new(FakeCareerStore);
            let lives: Arc<dyn LifeStore> = Arc::new(FakeLifeStore);
            let markets: Arc<dyn MarketStore> = Arc::new(FakeMarketStore);
            let runs: Arc<dyn RunStore> = Arc::new(FakeRunStore);
            let users: Arc<dyn UserStore> = Arc::new(FakeUserStore);
            let timer = Arc::new(ManualTimer::new());
            let providers = Providers::from_env("http://localhost:8080".to_owned())
                .expect("테스트 공급자 설정은 유효해야 한다");
            let state = AppState::new_with_timer(
                AppStateDependencies {
                    stores: create_app_stores(AppStoreDependencies {
                        games: game_pipeline,
                        trades,
                        finances,
                        cash_products,
                        assets,
                        tax_accounts,
                        careers,
                        lives,
                        markets,
                        runs,
                        users,
                    }),
                    providers,
                },
                timer,
            );

            (state, games)
        }

        #[tokio::test]
        async fn given_snapshot_load_failed_after_commit_when_same_order_is_replayed_then_committed_snapshot_is_pushed()
         {
            let (state, games) = given_order_state();
            let subscription = state
                .open_stream(USER_ID)
                .await
                .expect("스트림을 열어야 한다");
            let (_, mut receiver, _connection) = subscription.into_parts();
            let order = given_order();
            games.fail_next_load();

            let first = state.place_order(USER_ID, &order).await;
            let replay = state
                .place_order(USER_ID, &order)
                .await
                .expect("같은 주문 재시도는 저장된 체결을 복구해야 한다");
            let pushed = when_tick_arrives(&mut receiver).await;

            assert!(first.is_err());
            let PlaceOrderResult::Executed(response) = replay else {
                panic!("재시도는 체결 응답이어야 한다");
            };
            assert!(response.execution.replayed);
            assert_eq!(response.snapshot.state_revision, 1);
            assert_eq!(pushed.state_revision, 1);
            assert_eq!(pushed.cash_krw, 9_900_000);
        }

        #[tokio::test]
        async fn given_an_unseen_invalid_order_when_submitted_then_invalid_order_is_returned() {
            let (state, _games) = given_order_state();
            let mut request = given_order_request("b6a1cc9d-3c87-44a9-aebe-9ff46677f043");
            request.quantity = 0;
            let order = TradeOrder::try_from(request)
                .expect("구문상 식별 가능한 주문은 저장소까지 전달되어야 한다");

            let result = state
                .place_order(USER_ID, &order)
                .await
                .expect("주문 거절은 서비스 결과여야 한다");

            assert!(matches!(
                result,
                PlaceOrderResult::Rejected(TradeFailure {
                    code: TradeFailureCode::InvalidOrder,
                    ..
                })
            ));
        }

        #[tokio::test]
        async fn given_a_successful_order_id_with_changed_invalid_payload_when_submitted_then_idempotency_conflict_is_returned()
         {
            let (state, _games) = given_order_state();
            let order = given_order();
            state
                .place_order(USER_ID, &order)
                .await
                .expect("첫 주문은 체결되어야 한다");
            let mut changed_request = given_order_request("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2");
            changed_request.symbol = "USD".to_owned();
            let changed = TradeOrder::try_from(changed_request)
                .expect("구문상 식별 가능한 주문은 저장소까지 전달되어야 한다");

            let result = state
                .place_order(USER_ID, &changed)
                .await
                .expect("주문 거절은 서비스 결과여야 한다");

            assert!(matches!(
                result,
                PlaceOrderResult::Rejected(TradeFailure {
                    code: TradeFailureCode::IdempotencyConflict,
                    ..
                })
            ));
        }
    }

    mod context_speed_is_selected {
        use super::*;

        #[test]
        fn given_supported_speeds_when_read_then_intervals_match_the_contract() {
            let intervals = [
                AutoSpeed::X1.interval(),
                AutoSpeed::X2.interval(),
                AutoSpeed::X4.interval(),
                AutoSpeed::X8.interval(),
            ];

            assert_eq!(
                intervals,
                [
                    Duration::from_millis(500),
                    Duration::from_millis(250),
                    Duration::from_millis(125),
                    Duration::from_millis(62),
                ]
            );
        }

        #[test]
        fn given_a_numeric_speed_when_serialized_then_it_stays_numeric() {
            let serialized =
                serde_json::to_value(AutoSpeed::X4).expect("speed must serialize for a snapshot");

            assert_eq!(serialized, serde_json::json!(4));
        }

        #[test]
        fn given_an_unsupported_speed_when_deserialized_then_it_is_rejected() {
            let parsed = serde_json::from_value::<AutoSpeed>(serde_json::json!(3));

            assert!(parsed.is_err());
        }
    }

    mod context_manual_days_are_requested {
        use super::*;

        #[tokio::test]
        async fn given_three_days_when_advanced_then_each_day_is_committed_and_pushed() {
            let (state, store, _timer) = given_state(Some(given_character("테스터")));
            let subscription = state.open_stream(USER_ID).await.expect("stream must open");
            let (_, mut receiver, _connection) = subscription.into_parts();

            let command = given_advance_command(3);
            let response = state
                .advance(USER_ID, &command)
                .await
                .expect("advance must pass");
            let mut pushed_days = Vec::new();
            for _ in 0..3 {
                pushed_days.push(
                    receiver
                        .try_recv()
                        .expect("every committed day must be pushed")
                        .game_day,
                );
            }

            assert_eq!(response.snapshot.game_day, 3);
            assert_eq!(response.advance.requested_days, 3);
            assert_eq!(store.committed_days(), vec![1, 2, 3]);
            assert_eq!(pushed_days, vec![1, 2, 3]);
        }

        #[tokio::test]
        async fn given_the_final_response_was_lost_when_retried_then_no_day_or_tick_is_added() {
            let (state, store, _timer) = given_state(Some(given_character("테스터")));
            let subscription = state.open_stream(USER_ID).await.expect("stream must open");
            let (_, mut receiver, _connection) = subscription.into_parts();
            let command = given_advance_command(3);
            state
                .advance(USER_ID, &command)
                .await
                .expect("first advance must pass");
            for _ in 0..3 {
                receiver
                    .try_recv()
                    .expect("first execution must push a tick");
            }

            let replay = state
                .advance(USER_ID, &command)
                .await
                .expect("same command must replay");

            assert!(replay.advance.replayed);
            assert_eq!(replay.advance.committed_cursor.game_day, 3);
            assert_eq!(replay.snapshot.game_day, 3);
            assert_eq!(store.committed_days(), vec![1, 2, 3]);
            assert!(matches!(
                receiver.try_recv(),
                Err(broadcast::error::TryRecvError::Empty)
            ));
        }

        #[tokio::test]
        async fn given_step_two_failed_when_retried_then_only_the_two_missing_days_are_committed() {
            let (state, store, _timer) = given_state(Some(given_character("테스터")));
            let subscription = state.open_stream(USER_ID).await.expect("stream must open");
            let (_, mut receiver, _connection) = subscription.into_parts();
            let command = given_advance_command(3);
            store.fail_on_manual_step(2);

            let first = state.advance(USER_ID, &command).await;
            let first_tick = receiver
                .try_recv()
                .expect("the durable first step must already be pushed");
            let resumed = state
                .advance(USER_ID, &command)
                .await
                .expect("retry must resume after the first step");
            let second_tick = receiver.try_recv().expect("day two must be pushed");
            let third_tick = receiver.try_recv().expect("day three must be pushed");

            assert!(matches!(first, Err(GameLoopError::Internal(_))));
            assert_eq!(
                [
                    first_tick.game_day,
                    second_tick.game_day,
                    third_tick.game_day
                ],
                [1, 2, 3]
            );
            assert!(!resumed.advance.replayed);
            assert_eq!(resumed.advance.initial_cursor.game_day, 0);
            assert_eq!(resumed.advance.committed_cursor.game_day, 3);
            assert_eq!(store.committed_days(), vec![1, 2, 3]);
        }

        #[tokio::test]
        async fn given_days_outside_the_range_when_advanced_then_they_are_rejected() {
            let (state, store, _timer) = given_state(Some(given_character("테스터")));

            let zero_command = given_advance_command(0);
            let thirty_one_command = given_advance_command(31);
            let zero = state.advance(USER_ID, &zero_command).await;
            let thirty_one = state.advance(USER_ID, &thirty_one_command).await;

            assert!(matches!(zero, Err(GameLoopError::InvalidCommand)));
            assert!(matches!(thirty_one, Err(GameLoopError::InvalidCommand)));
            assert!(store.committed_days().is_empty());
        }

        #[tokio::test]
        async fn given_a_successful_command_id_with_changed_invalid_days_when_advanced_then_idempotency_conflict_is_returned()
         {
            let (state, store, _timer) = given_state(Some(given_character("테스터")));
            let command = given_advance_command(1);
            state
                .advance(USER_ID, &command)
                .await
                .expect("첫 수동 전진은 성공해야 한다");
            let mut changed = command;
            changed.days = 0;

            let result = state.advance(USER_ID, &changed).await;

            assert!(matches!(result, Err(GameLoopError::IdempotencyConflict)));
            assert_eq!(store.committed_days(), vec![1]);
        }

        #[tokio::test]
        async fn given_no_character_when_advanced_then_conflict_is_returned_without_a_commit() {
            let (state, store, _timer) = given_state(None);

            let command = given_advance_command(1);
            let result = state.advance(USER_ID, &command).await;

            assert!(matches!(result, Err(GameLoopError::CharacterRequired)));
            assert!(store.committed_days().is_empty());
        }

        #[tokio::test]
        async fn given_concurrent_requests_when_advanced_then_one_save_is_serialized() {
            let (state, store, _timer) = given_state(Some(given_character("테스터")));

            let first_command =
                given_advance_command_with_id(2, "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2");
            let second_command =
                given_advance_command_with_id(2, "b6a1cc9d-3c87-44a9-aebe-9ff46677f043");
            let (first, second) = tokio::join!(
                state.advance(USER_ID, &first_command),
                state.advance(USER_ID, &second_command),
            );

            assert!(first.is_ok());
            assert!(matches!(second, Err(GameLoopError::Busy)));
            assert_eq!(store.committed_days(), vec![1, 2]);
            assert_eq!(store.max_active_advances(), 1);
        }
    }

    mod context_online_clock_is_controlled {
        use super::*;

        #[tokio::test]
        async fn given_no_stream_when_started_then_active_stream_is_required() {
            let (state, _store, _timer) = given_state(Some(given_character("테스터")));

            let result = state.set_clock(USER_ID, Some(AutoSpeed::X1)).await;

            assert!(matches!(result, Err(GameLoopError::ActiveStreamRequired)));
        }

        #[tokio::test]
        async fn given_no_character_when_started_then_character_is_required() {
            let (state, _store, _timer) = given_state(None);
            let _subscription = state.open_stream(USER_ID).await.expect("stream must open");

            let result = state.set_clock(USER_ID, Some(AutoSpeed::X1)).await;

            assert!(matches!(result, Err(GameLoopError::CharacterRequired)));
        }

        #[tokio::test]
        async fn given_an_active_stream_when_started_then_the_first_step_waits_for_its_interval() {
            let (state, store, timer) = given_state(Some(given_character("테스터")));
            let _subscription = state.open_stream(USER_ID).await.expect("stream must open");

            let snapshot = state
                .set_clock(USER_ID, Some(AutoSpeed::X8))
                .await
                .expect("clock must start");
            timer.wait_until_armed(1).await;

            assert_eq!(snapshot.auto_speed, Some(AutoSpeed::X8));
            assert_eq!(store.state().game_day, 0);
            assert_eq!(timer.waits(), vec![Duration::from_millis(62)]);
        }

        #[tokio::test]
        async fn given_clock_commands_when_applied_then_each_control_snapshot_is_pushed() {
            let (state, _store, _timer) = given_state(Some(given_character("테스터")));
            let subscription = state.open_stream(USER_ID).await.expect("stream must open");
            let (_, mut receiver, _connection) = subscription.into_parts();

            state
                .set_clock(USER_ID, Some(AutoSpeed::X4))
                .await
                .expect("clock must start");
            state
                .set_clock(USER_ID, None)
                .await
                .expect("clock must pause");
            let started = receiver.try_recv().expect("start must be pushed");
            let paused = receiver.try_recv().expect("pause must be pushed");

            assert_eq!(started.auto_speed, Some(AutoSpeed::X4));
            assert_eq!(paused.auto_speed, None);
            assert_eq!(started.game_day, paused.game_day);
            assert!(matches!(
                receiver.try_recv(),
                Err(broadcast::error::TryRecvError::Empty)
            ));
        }

        #[tokio::test]
        async fn given_pause_load_fails_when_running_then_the_cached_pause_is_already_pushed() {
            let (state, store, timer) = given_state(Some(given_character("테스터")));
            let subscription = state.open_stream(USER_ID).await.expect("stream must open");
            let (_, mut receiver, _connection) = subscription.into_parts();
            state
                .set_clock(USER_ID, Some(AutoSpeed::X2))
                .await
                .expect("clock must start");
            let started = when_tick_arrives(&mut receiver).await;
            timer.wait_until_armed(1).await;
            store.fail_next_load();

            let result = state.set_clock(USER_ID, None).await;
            let paused = when_tick_arrives(&mut receiver).await;
            for _ in 0..20 {
                tokio::task::yield_now().await;
            }

            assert!(matches!(result, Err(GameLoopError::Internal(_))));
            assert_eq!(started.auto_speed, Some(AutoSpeed::X2));
            assert_eq!(paused.auto_speed, None);
            assert_eq!(paused.game_day, started.game_day);
            assert_eq!(timer.waits(), vec![Duration::from_millis(250)]);
        }

        #[tokio::test]
        async fn given_the_same_speed_when_selected_again_then_the_existing_wait_is_kept() {
            let (state, _store, timer) = given_state(Some(given_character("테스터")));
            let _subscription = state.open_stream(USER_ID).await.expect("stream must open");
            state
                .set_clock(USER_ID, Some(AutoSpeed::X2))
                .await
                .expect("clock must start");
            timer.wait_until_armed(1).await;

            state
                .set_clock(USER_ID, Some(AutoSpeed::X2))
                .await
                .expect("same speed must be maintained");
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }

            assert_eq!(timer.waits(), vec![Duration::from_millis(250)]);
        }

        #[tokio::test]
        async fn given_a_different_speed_when_selected_then_the_wait_is_replaced() {
            let (state, _store, timer) = given_state(Some(given_character("테스터")));
            let _subscription = state.open_stream(USER_ID).await.expect("stream must open");
            state
                .set_clock(USER_ID, Some(AutoSpeed::X1))
                .await
                .expect("clock must start");
            timer.wait_until_armed(1).await;

            state
                .set_clock(USER_ID, Some(AutoSpeed::X4))
                .await
                .expect("speed must change");
            timer.wait_until_armed(2).await;

            assert_eq!(
                timer.waits(),
                vec![Duration::from_millis(500), Duration::from_millis(125)]
            );
        }

        #[tokio::test]
        async fn given_a_timer_release_when_running_then_one_day_finishes_before_the_next_wait() {
            let (state, store, timer) = given_state(Some(given_character("테스터")));
            let _subscription = state.open_stream(USER_ID).await.expect("stream must open");
            state
                .set_clock(USER_ID, Some(AutoSpeed::X8))
                .await
                .expect("clock must start");
            timer.wait_until_armed(1).await;

            timer.release_one();
            when_game_day_reaches(&store, 1).await;
            timer.wait_until_armed(2).await;

            assert_eq!(store.committed_days(), vec![1]);
            assert_eq!(timer.waits().len(), 2);
        }

        #[tokio::test]
        async fn given_the_daily_commit_fails_when_running_then_a_pause_tick_is_pushed_and_no_timer_restarts()
         {
            let (state, store, timer) = given_state(Some(given_character("테스터")));
            let subscription = state.open_stream(USER_ID).await.expect("stream must open");
            let (_, mut receiver, _connection) = subscription.into_parts();
            state
                .set_clock(USER_ID, Some(AutoSpeed::X8))
                .await
                .expect("clock must start");
            let started = when_tick_arrives(&mut receiver).await;
            timer.wait_until_armed(1).await;
            store.fail_next_advance();

            timer.release_one();
            let paused = when_tick_arrives(&mut receiver).await;
            for _ in 0..50 {
                tokio::task::yield_now().await;
            }

            assert_eq!(started.auto_speed, Some(AutoSpeed::X8));
            assert_eq!(paused.auto_speed, None);
            assert_eq!(paused.game_day, started.game_day);
            assert!(store.committed_days().is_empty());
            assert_eq!(timer.waits(), vec![Duration::from_millis(62)]);
        }

        #[tokio::test]
        async fn given_multiple_streams_when_closed_then_only_the_last_one_pauses() {
            let (state, _store, timer) = given_state(Some(given_character("테스터")));
            let first = state
                .open_stream(USER_ID)
                .await
                .expect("first stream must open");
            let second = state
                .open_stream(USER_ID)
                .await
                .expect("second stream must open");
            state
                .set_clock(USER_ID, Some(AutoSpeed::X1))
                .await
                .expect("clock must start");
            timer.wait_until_armed(1).await;

            drop(first);
            let while_connected = state.snapshot(USER_ID).await.expect("snapshot must load");
            drop(second);
            let after_last_close = state.snapshot(USER_ID).await.expect("snapshot must load");

            assert_eq!(while_connected.auto_speed, Some(AutoSpeed::X1));
            assert_eq!(after_last_close.auto_speed, None);
        }

        #[tokio::test]
        async fn given_auto_running_when_manual_days_start_then_auto_is_paused_first() {
            let (state, store, timer) = given_state(Some(given_character("테스터")));
            let _subscription = state.open_stream(USER_ID).await.expect("stream must open");
            state
                .set_clock(USER_ID, Some(AutoSpeed::X8))
                .await
                .expect("clock must start");
            timer.wait_until_armed(1).await;

            let command = given_advance_command(2);
            let response = state
                .advance(USER_ID, &command)
                .await
                .expect("manual advance must pass");
            timer.release_one();
            for _ in 0..20 {
                tokio::task::yield_now().await;
            }

            assert_eq!(response.snapshot.auto_speed, None);
            assert_eq!(store.committed_days(), vec![1, 2]);
        }

        #[tokio::test]
        async fn given_manual_commit_fails_when_running_then_the_cached_pause_is_already_pushed() {
            let (state, store, timer) = given_state(Some(given_character("테스터")));
            let subscription = state.open_stream(USER_ID).await.expect("stream must open");
            let (_, mut receiver, _connection) = subscription.into_parts();
            state
                .set_clock(USER_ID, Some(AutoSpeed::X8))
                .await
                .expect("clock must start");
            let started = when_tick_arrives(&mut receiver).await;
            timer.wait_until_armed(1).await;
            store.fail_next_advance();

            let command = given_advance_command(1);
            let result = state.advance(USER_ID, &command).await;
            let paused = when_tick_arrives(&mut receiver).await;
            for _ in 0..20 {
                tokio::task::yield_now().await;
            }

            assert!(matches!(result, Err(GameLoopError::Internal(_))));
            assert_eq!(started.auto_speed, Some(AutoSpeed::X8));
            assert_eq!(paused.auto_speed, None);
            assert_eq!(paused.game_day, started.game_day);
            assert!(store.committed_days().is_empty());
            assert_eq!(timer.waits(), vec![Duration::from_millis(62)]);
        }

        #[tokio::test]
        async fn given_auto_running_when_character_is_recreated_then_revision_advances_and_clock_stops()
         {
            let (state, _store, timer) = given_state(Some(given_character("첫 캐릭터")));
            let _subscription = state.open_stream(USER_ID).await.expect("stream must open");
            state
                .set_clock(USER_ID, Some(AutoSpeed::X1))
                .await
                .expect("clock must start");
            timer.wait_until_armed(1).await;

            let command = given_start_game_command("새 캐릭터");
            let response = state
                .start_game(USER_ID, &command)
                .await
                .expect("character recreation must pass");

            assert_eq!(response.snapshot.run_revision, 1);
            assert_eq!(response.snapshot.game_day, 0);
            assert_eq!(response.snapshot.auto_speed, None);
            assert_eq!(
                response.snapshot.character_name.as_deref(),
                Some("새 캐릭터")
            );
        }

        #[tokio::test]
        async fn given_start_response_was_lost_when_retried_then_the_run_is_not_created_twice() {
            let (state, store, _timer) = given_state(None);
            let subscription = state.open_stream(USER_ID).await.expect("stream must open");
            let (_, mut receiver, _connection) = subscription.into_parts();
            let command = given_start_game_command("새 캐릭터");
            let first = state
                .start_game(USER_ID, &command)
                .await
                .expect("first start must pass");
            let first_tick = receiver.try_recv().expect("new run must be pushed");

            let replay = state
                .start_game(USER_ID, &command)
                .await
                .expect("same start must replay");

            assert!(!first.start.replayed);
            assert!(replay.start.replayed);
            assert_eq!(first_tick.run_revision, 1);
            assert_eq!(store.state().run_revision, 1);
            assert!(matches!(
                receiver.try_recv(),
                Err(broadcast::error::TryRecvError::Empty)
            ));
        }

        #[tokio::test]
        async fn given_an_unseen_invalid_character_when_started_then_it_is_rejected_without_a_new_run()
         {
            let (state, store, _timer) = given_state(None);
            let mut command = given_start_game_command("새 캐릭터");
            command.draft.name.clear();

            let result = state.start_game(USER_ID, &command).await;

            let Err(GameLoopError::InvalidCharacter(errors)) = result else {
                panic!("새 invalid 캐릭터는 도메인 검증 오류여야 한다");
            };
            assert!(errors.iter().any(|error| error.field == "name"));
            assert_eq!(store.state().run_revision, 0);
            assert!(store.state().character.is_none());
        }

        #[tokio::test]
        async fn given_a_successful_start_id_with_changed_invalid_character_when_started_then_idempotency_conflict_is_returned()
         {
            let (state, store, _timer) = given_state(None);
            let command = given_start_game_command("새 캐릭터");
            state
                .start_game(USER_ID, &command)
                .await
                .expect("첫 캐릭터 시작은 성공해야 한다");
            let mut changed = command;
            changed.draft.name.clear();

            let result = state.start_game(USER_ID, &changed).await;

            assert!(matches!(result, Err(GameLoopError::IdempotencyConflict)));
            assert_eq!(store.state().run_revision, 1);
        }
    }
}
