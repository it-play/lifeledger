//! Store contracts. The MySQL implementation does not know this file, and callers do
//! not know the implementation.

use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::auth::{OAuthIdentity, ProviderKind};
use crate::career::{
    ActivityStatus, ArtifactDraft, ArtifactKind, CareerFailureCode, DimensionScores,
    EmploymentType, EvidenceKind, Industry, LifeStatus,
};
use crate::character::{Character, CharacterDraft, Education, Region, ValidationError};
use crate::finance::{
    BondCatalog, BondOrderCommand, BondOrderResponse, CashProductCatalog, CashProductContractState,
    CloseCashProductCommand, CloseCashProductReceipt, CloseCmaAccountCommand,
    CloseCmaAccountReceipt, CmaAccountContractState, CommandCursor, CommandId,
    DepositProtectionState, FinanceFailureCode, FinancialAccount, FinancialIncomeYear, GoldCatalog,
    GoldOrderCommand, GoldOrderResponse, GoldWithdrawalCommand, GoldWithdrawalResponse, LedgerPage,
    M2dAssetCommandResult, M2dAssetSnapshot, OpenCashProductCommand, OpenCashProductReceipt,
    OpenCmaAccountCommand, OpenCmaAccountReceipt, OpenGoldAccountCommand, OpenGoldAccountResponse,
    PolicySet, PolicySetAssignment, ResourceId, ScheduledSettlement, TransferCommand,
    TransferReceipt,
};
use crate::finance::{IrpWithdrawalReason, PensionTaxLayers, PensionWithdrawalRequestKind};
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
use crate::market::{MarketCalibration, MarketDay, MarketWorld};
use crate::runs::{
    LeagueRankingPage, PointBudgetEvaluation, PointSelection, RankedRunContext,
    RankedRunPreparation, RankingPageCursor, RunFinalization, RunManifestSummary, RunMode,
    RunOptions, SeasonLeagues,
};
use crate::trading::{PositionState, TradeExecution, TradeFailure, TradeOrder};

use super::annual_tax::AnnualTaxYearState;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerEvidenceState {
    pub id: ResourceId,
    pub evidence_key: String,
    pub catalog_entry_id: ResourceId,
    pub catalog_entry_key: String,
    pub display_name: String,
    pub kind: EvidenceKind,
    pub acquired_game_day: u32,
    pub expires_on_game_day: Option<u32>,
    pub period_start_date: Option<String>,
    pub period_end_exclusive_date: Option<String>,
    pub credited_experience_days: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerActivityCatalogState {
    pub id: ResourceId,
    pub activity_key: String,
    pub display_name: String,
    pub output_kind: EvidenceKind,
    pub minimum_calendar_days: u32,
    pub required_effort_units: u64,
    pub daily_effort_cap_units: u64,
    pub allowed_life_statuses: Vec<LifeStatus>,
    pub cost_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerActivityState {
    pub id: ResourceId,
    pub catalog_entry_id: ResourceId,
    pub activity_key: String,
    pub display_name: String,
    pub status: ActivityStatus,
    pub priority: Option<u8>,
    pub started_game_day: Option<u32>,
    pub accumulated_effort_units: u64,
    pub required_effort_units: u64,
    pub elapsed_calendar_days: u32,
    pub minimum_calendar_days: u32,
    pub daily_effort_cap_units: u64,
    pub completed_game_day: Option<u32>,
    pub cancelled_game_day: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerArtifactState {
    pub id: ResourceId,
    pub kind: ArtifactKind,
    pub version_no: u32,
    pub headline: String,
    pub summary: String,
    pub evidence_ids: Vec<ResourceId>,
    pub completeness_bp: i64,
    pub created_game_day: u32,
    pub open_to_work: Option<bool>,
    pub industries: Vec<Industry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerSnapshotState {
    pub focused_job_family_key: String,
    pub possessed_scores: DimensionScores,
    pub active_activities: Vec<CareerActivityState>,
    pub latest_artifacts: Vec<CareerArtifactState>,
    pub open_applications: Vec<CareerApplicationState>,
    pub open_invitations: Vec<CareerInvitationState>,
    pub employment: Option<EmploymentContractState>,
    pub latest_payroll: Option<CareerPayrollState>,
    pub current_employment_tax_year: CareerEmploymentTaxYearState,
    pub latest_employment_tax_assessment: Option<CareerEmploymentTaxYearState>,
    pub military_status: CareerMilitaryStatus,
    pub active_military_service: Option<ActiveMilitaryServiceState>,
    pub active_military_savings: Vec<ActiveMilitarySavingsState>,
    pub pending_career_schedule: Vec<CareerPendingScheduleItemState>,
}

impl CareerSnapshotState {
    pub fn empty(focused_job_family_key: String) -> Self {
        Self {
            focused_job_family_key,
            possessed_scores: DimensionScores::default(),
            active_activities: Vec::new(),
            latest_artifacts: Vec::new(),
            open_applications: Vec::new(),
            open_invitations: Vec::new(),
            employment: None,
            latest_payroll: None,
            current_employment_tax_year: CareerEmploymentTaxYearState::open(1),
            latest_employment_tax_assessment: None,
            military_status: CareerMilitaryStatus::Unserved,
            active_military_service: None,
            active_military_savings: Vec::new(),
            pending_career_schedule: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CareerPageQuery {
    pub before: Option<u64>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CareerArtifactPageQuery {
    pub kind: Option<ArtifactKind>,
    pub page: CareerPageQuery,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerSpecsState {
    pub focused_job_family_key: String,
    pub possessed_scores: DimensionScores,
    pub items: Vec<CareerEvidenceState>,
    pub next_before: Option<ResourceId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerActivitiesState {
    pub catalog: Vec<CareerActivityCatalogState>,
    pub active: Vec<CareerActivityState>,
    pub items: Vec<CareerActivityState>,
    pub next_before: Option<ResourceId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerArtifactPageState {
    pub items: Vec<CareerArtifactState>,
    pub next_before: Option<ResourceId>,
}

/// The catalog's stable platform identity is owned by the pure recruitment domain.
pub type CareerPlatform = crate::career::PlatformKey;
pub type CareerCompetitionBand = crate::career::CompetitionBand;

pub type CareerMilitaryRequirement = crate::career::MilitaryPostingRequirement;

pub type CareerMilitaryStatus = crate::career::MilitaryStatus;
pub type MilitaryServiceType = crate::career::MilitaryServiceType;
pub type MilitaryServiceStatus = crate::career::MilitaryServiceStatus;
pub type MilitaryHardRequirementsState = crate::career::MilitaryHardRequirements;
pub type MilitaryPayStageState = crate::career::MilitaryPayStagePolicy;
pub type MilitaryExperienceCreditState = crate::career::MilitaryExperiencePolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MilitaryServiceSourceKind {
    UserCommand,
    LegacyBridge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MilitaryCompensationKind {
    MilitaryPay,
    EmploymentPayroll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MilitaryOptionIneligibilityReason {
    MilitarySubjectRequired,
    MilitaryStateConflict,
    MinimumEducation,
    MinimumCertificationCount,
    MinimumExperienceDays,
    PolicyUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitaryOptionState {
    pub id: ResourceId,
    pub option_key: String,
    pub service_type: MilitaryServiceType,
    pub display_name: String,
    pub eligible: bool,
    pub ineligibility_reasons: Vec<MilitaryOptionIneligibilityReason>,
    pub service_duration_months: u16,
    pub hard_requirements: MilitaryHardRequirementsState,
    pub compensation_kind: MilitaryCompensationKind,
    pub pay_schedule: crate::career::MilitaryPayScheduleKind,
    pub pay_stages: Vec<MilitaryPayStageState>,
    pub effort_life_status: LifeStatus,
    pub daily_effort_capacity_units: u64,
    pub grants_career_experience: bool,
    pub experience_credits: Vec<MilitaryExperienceCreditState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitaryOptionsState {
    pub items: Vec<MilitaryOptionState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActiveMilitaryServiceState {
    pub id: ResourceId,
    pub option_version_id: ResourceId,
    pub service_type: MilitaryServiceType,
    pub display_name: String,
    pub status: MilitaryServiceStatus,
    pub start_game_day: u32,
    pub end_game_day: u32,
    pub credited_service_days: u32,
    pub total_service_days: u32,
    pub effort_life_status: LifeStatus,
    pub grants_career_experience: bool,
    pub next_pay_game_day: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitaryServiceHistoryState {
    pub id: ResourceId,
    pub option_version_id: ResourceId,
    pub service_type: MilitaryServiceType,
    pub display_name: String,
    pub status: MilitaryServiceStatus,
    pub source_kind: MilitaryServiceSourceKind,
    pub start_game_day: u32,
    pub end_game_day: u32,
    pub credited_service_days: u32,
    pub total_service_days: u32,
    pub effort_life_status: LifeStatus,
    pub grants_career_experience: bool,
    pub next_pay_game_day: Option<u32>,
    pub start_date: String,
    pub end_exclusive_date: String,
    pub completed_game_day: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitaryServiceState {
    pub military_status: CareerMilitaryStatus,
    pub service: Option<MilitaryServiceHistoryState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MilitarySavingsIneligibilityReason {
    MilitaryStateConflict,
    ServiceTypeNotEligible,
    MinimumRemainingService,
    ActiveContractLimit,
    InstitutionLimit,
    JoinWindowClosed,
    PolicyUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MilitarySavingsContractStatus {
    Active,
    Matured,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MilitarySavingsInstallmentStatusState {
    Scheduled,
    Paid,
    Missed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MilitarySavingsClosureReason {
    Maturity,
    EarlyClose,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitarySavingsInterestTierState {
    pub minimum_term_months: u16,
    pub maximum_term_months_inclusive: u16,
    pub annual_interest_rate_ppm: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitarySavingsProductState {
    pub id: ResourceId,
    pub product_key: String,
    pub institution_key: String,
    pub institution_display_name: String,
    pub eligible: bool,
    pub ineligibility_reasons: Vec<MilitarySavingsIneligibilityReason>,
    pub eligible_service_types: Vec<MilitaryServiceType>,
    pub join_start_date: String,
    pub join_end_date: String,
    pub minimum_remaining_service_months: u16,
    pub maximum_active_contracts: u8,
    pub maximum_contracts_per_institution: u8,
    pub minimum_monthly_contribution_krw: i64,
    pub maximum_institution_monthly_contribution_krw: i64,
    pub maximum_total_monthly_contribution_krw: i64,
    pub limit_setting_unit_krw: i64,
    pub installment_unit_krw: i64,
    pub interest_tiers: Vec<MilitarySavingsInterestTierState>,
    pub day_count_convention: MilitarySavingsDayCountConvention,
    pub interest_rounding: MilitarySavingsInterestRounding,
    pub early_close_annual_interest_rate_ppm: i64,
    pub government_matching_rate_ppm: i64,
    pub government_match_payment_day_of_month: u8,
    pub maturity_tax_exempt: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MilitarySavingsDayCountConvention {
    Actual365,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MilitarySavingsInterestRounding {
    FloorToKrw,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitarySavingsProductsState {
    pub items: Vec<MilitarySavingsProductState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActiveMilitarySavingsState {
    pub id: ResourceId,
    pub product_version_id: ResourceId,
    pub institution_key: String,
    pub status: MilitarySavingsContractStatus,
    pub monthly_contribution_krw: i64,
    pub debit_day_of_month: u8,
    pub principal_krw: i64,
    pub paid_installment_count: u16,
    pub missed_installment_count: u16,
    pub next_installment_game_day: Option<u32>,
    pub maturity_game_day: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitarySavingsInstallmentState {
    pub id: ResourceId,
    pub installment_no: u16,
    pub due_game_day: u32,
    pub status: MilitarySavingsInstallmentStatusState,
    pub paid_game_day: Option<u32>,
    pub principal_krw: i64,
    pub government_matching_policy_version_id: Option<ResourceId>,
    pub government_matching_rate_ppm: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitarySavingsMaturityProjectionState {
    pub assumption: MilitarySavingsProjectionAssumption,
    pub principal_krw: i64,
    pub gross_bank_interest_krw: i64,
    pub government_match_krw: i64,
    pub bank_payout_krw: i64,
    pub total_benefit_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MilitarySavingsProjectionAssumption {
    AllScheduledInstallmentsPaid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitarySavingsHistoryItemState {
    pub id: ResourceId,
    pub service_id: ResourceId,
    pub product_version_id: ResourceId,
    pub product_key: String,
    pub institution_key: String,
    pub institution_display_name: String,
    pub status: MilitarySavingsContractStatus,
    pub monthly_contribution_krw: i64,
    pub debit_day_of_month: u8,
    pub principal_krw: i64,
    pub paid_installment_count: u16,
    pub missed_installment_count: u16,
    pub next_installment_game_day: Option<u32>,
    pub maturity_game_day: u32,
    pub opened_game_day: u32,
    pub first_installment_game_day: u32,
    pub contract_term_months: u16,
    pub annual_interest_rate_ppm: i64,
    pub closed_game_day: Option<u32>,
    pub closure_reason: Option<MilitarySavingsClosureReason>,
    pub settled_principal_krw: i64,
    pub gross_bank_interest_krw: i64,
    pub government_match_krw: i64,
    pub bank_payout_krw: i64,
    pub government_match_paid_game_day: Option<u32>,
    pub projected_maturity: Option<MilitarySavingsMaturityProjectionState>,
    pub installments: Vec<MilitarySavingsInstallmentState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitarySavingsPageState {
    pub items: Vec<MilitarySavingsHistoryItemState>,
    pub next_before: Option<ResourceId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CareerScheduledActionKind {
    EmploymentStart,
    MilitaryServiceStart,
    MilitaryServiceCompletion,
    DocumentReview,
    ConfirmationExpiry,
    InterviewDecision,
    OfferExpiry,
    InvitationGeneration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CareerScheduledSettlementKind {
    EmploymentPayroll,
    EmploymentReconciliation,
    MilitaryPay,
    MilitarySavingsInstallment,
    MilitarySavingsMaturity,
    MilitarySavingsGovernmentMatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "sourceKind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CareerPendingScheduleItemState {
    CareerAction {
        id: ResourceId,
        due_game_day: u32,
        kind: CareerScheduledActionKind,
    },
    Settlement {
        id: ResourceId,
        due_game_day: u32,
        kind: CareerScheduledSettlementKind,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CareerApplicationStatus {
    Submitted,
    DocumentRejected,
    InterviewAwaitingConfirmation,
    InterviewConfirmed,
    InterviewRejected,
    Offered,
    Accepted,
    Declined,
    Expired,
    Withdrawn,
    Closed,
}

impl CareerApplicationStatus {
    pub const fn is_open(self) -> bool {
        matches!(
            self,
            Self::Submitted
                | Self::InterviewAwaitingConfirmation
                | Self::InterviewConfirmed
                | Self::Offered
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CareerInvitationStatus {
    Open,
    Accepted,
    Declined,
    Expired,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CareerOfferStatus {
    Offered,
    Accepted,
    Declined,
    Expired,
    Closed,
}

pub type CareerApplicationSource = crate::career::ApplicationSource;
pub type EmploymentStatus = crate::career::EmploymentContractStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CareerJobsPageQuery {
    pub before: Option<String>,
    pub limit: u32,
    pub platform: Option<CareerPlatform>,
    pub industry: Option<Industry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerJobState {
    pub posting_key: String,
    pub posted_game_day: u32,
    pub closes_exclusive_game_day: u32,
    pub platform: CareerPlatform,
    pub industry: Industry,
    pub job_family_key: String,
    pub employer_name: String,
    pub region: Region,
    pub employment_type: EmploymentType,
    pub required_scores: DimensionScores,
    pub possessed_scores: DimensionScores,
    pub minimum_annual_salary_krw: i64,
    pub maximum_annual_salary_krw: i64,
    pub salary_step_krw: i64,
    pub competition_band: CareerCompetitionBand,
    pub military_requirement: CareerMilitaryRequirement,
    pub minimum_education: Option<Education>,
    pub required_certification_name: Option<String>,
    pub minimum_experience_days: u32,
    pub required_artifacts: Vec<ArtifactKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerJobsPageState {
    pub items: Vec<CareerJobState>,
    pub next_before: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerOfferState {
    pub id: ResourceId,
    pub status: CareerOfferStatus,
    pub annual_salary_krw: i64,
    pub payday_day_of_month: u8,
    pub start_game_day: u32,
    pub expires_exclusive_game_day: u32,
    pub wanted_reward_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerApplicationState {
    pub id: ResourceId,
    pub posting_key: String,
    pub platform: CareerPlatform,
    pub industry: Industry,
    pub employer_name: String,
    pub job_family_key: String,
    pub source: CareerApplicationSource,
    pub status: CareerApplicationStatus,
    pub submitted_game_day: u32,
    pub visible_scores: DimensionScores,
    pub possessed_scores: DimensionScores,
    pub document_score_bp: Option<i64>,
    pub document_decision_game_day: Option<u32>,
    pub interview_game_day: Option<u32>,
    pub confirmation_deadline_exclusive_game_day: Option<u32>,
    pub interview_score_bp: Option<i64>,
    pub offer: Option<CareerOfferState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerInvitationState {
    pub id: ResourceId,
    pub posting_key: String,
    pub platform: CareerPlatform,
    pub industry: Industry,
    pub job_family_key: String,
    pub employer_name: String,
    pub artifact_version_id: ResourceId,
    pub created_game_day: u32,
    pub expires_exclusive_game_day: u32,
    pub status: CareerInvitationStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmploymentContractState {
    pub id: ResourceId,
    pub status: EmploymentStatus,
    pub job_family_key: String,
    pub employer_name: String,
    pub region: String,
    pub annual_salary_krw: i64,
    pub payday_day_of_month: u8,
    pub start_game_day: u32,
    pub end_game_day: Option<u32>,
    pub credited_experience_days: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerApplicationsPageState {
    pub items: Vec<CareerApplicationState>,
    pub next_before: Option<ResourceId>,
    pub open_invitations: Vec<CareerInvitationState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerEmploymentState {
    pub contract: Option<EmploymentContractState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerRewardPaymentState {
    pub payment_id: ResourceId,
    pub gross_reward_krw: i64,
    pub withheld_income_tax_krw: i64,
    pub withheld_local_income_tax_krw: i64,
    pub net_reward_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerPayrollState {
    pub id: ResourceId,
    pub contract_id: ResourceId,
    pub period_no: u64,
    pub salary_month_ordinal: u8,
    pub period_start_date: String,
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
    pub reward: Option<CareerRewardPaymentState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerPayrollPageState {
    pub items: Vec<CareerPayrollState>,
    pub next_before: Option<ResourceId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CareerEmploymentTaxYearStatus {
    Open,
    Provisional,
    Definitive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CareerEmploymentTaxYearSource {
    EmploymentOnly,
    Combined,
    LegacyProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerEmploymentTaxYearState {
    pub tax_year: u16,
    pub status: CareerEmploymentTaxYearStatus,
    pub source: CareerEmploymentTaxYearSource,
    pub gross_employment_income_krw: i64,
    pub employee_insurance_deduction_krw: Option<i64>,
    pub earned_income_deduction_krw: Option<i64>,
    pub personal_deduction_krw: Option<i64>,
    pub taxable_income_krw: Option<i64>,
    pub calculated_income_tax_krw: Option<i64>,
    pub earned_income_tax_credit_krw: Option<i64>,
    pub pension_credit_eligible_contribution_krw: Option<i64>,
    pub actual_pension_income_tax_credit_krw: Option<i64>,
    pub actual_pension_local_income_tax_effect_krw: Option<i64>,
    pub withheld_income_tax_krw: Option<i64>,
    pub withheld_local_income_tax_krw: Option<i64>,
    pub assessed_income_tax_krw: Option<i64>,
    pub assessed_local_income_tax_krw: Option<i64>,
    pub additional_tax_krw: Option<i64>,
    pub refund_krw: Option<i64>,
    pub reconciliation_game_day: Option<u32>,
}

impl CareerEmploymentTaxYearState {
    pub const fn open(tax_year: u16) -> Self {
        Self {
            tax_year,
            status: CareerEmploymentTaxYearStatus::Open,
            source: CareerEmploymentTaxYearSource::EmploymentOnly,
            gross_employment_income_krw: 0,
            employee_insurance_deduction_krw: Some(0),
            earned_income_deduction_krw: None,
            personal_deduction_krw: None,
            taxable_income_krw: None,
            calculated_income_tax_krw: None,
            earned_income_tax_credit_krw: None,
            pension_credit_eligible_contribution_krw: None,
            actual_pension_income_tax_credit_krw: None,
            actual_pension_local_income_tax_effect_krw: None,
            withheld_income_tax_krw: Some(0),
            withheld_local_income_tax_krw: Some(0),
            assessed_income_tax_krw: None,
            assessed_local_income_tax_krw: None,
            additional_tax_krw: None,
            refund_krw: None,
            reconciliation_game_day: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusCareerCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub focused_job_family_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartCareerActivityCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub activity_catalog_entry_id: ResourceId,
    pub priority: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelCareerActivityCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub activity_id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishCareerArtifactCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub draft: ArtifactDraft,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyCareerCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub posting_key: String,
    pub resume_version_id: Option<ResourceId>,
    pub portfolio_version_id: Option<ResourceId>,
    pub linkedin_profile_version_id: Option<ResourceId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterviewDecision {
    Confirm,
    Decline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmCareerInterviewCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub application_id: ResourceId,
    pub decision: InterviewDecision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WithdrawCareerApplicationCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub application_id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptCareerInvitationCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub invitation_id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclineCareerInvitationCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub invitation_id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptCareerOfferCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub offer_id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclineCareerOfferCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub offer_id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartMilitaryServiceCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub military_option_version_id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenMilitarySavingsCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub product_version_id: ResourceId,
    pub monthly_contribution_krw: i64,
    pub debit_day_of_month: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseMilitarySavingsCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub contract_id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FocusCareerReceipt {
    pub command_id: CommandId,
    pub focused_job_family_key: String,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerActivityReceipt {
    pub command_id: CommandId,
    pub activity_id: ResourceId,
    pub status: ActivityStatus,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerArtifactReceipt {
    pub command_id: CommandId,
    pub artifact_version_id: ResourceId,
    pub kind: ArtifactKind,
    pub version_no: u32,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerApplicationReceipt {
    pub command_id: CommandId,
    pub application_id: ResourceId,
    pub status: CareerApplicationStatus,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerInvitationReceipt {
    pub command_id: CommandId,
    pub invitation_id: ResourceId,
    pub status: CareerInvitationStatus,
    pub application_id: Option<ResourceId>,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerOfferReceipt {
    pub command_id: CommandId,
    pub offer_id: ResourceId,
    pub status: CareerApplicationStatus,
    pub employment_contract_id: Option<ResourceId>,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitaryServiceCommandReceipt {
    pub command_id: CommandId,
    pub military_service_id: ResourceId,
    pub status: MilitaryServiceStatus,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitarySavingsCommandReceipt {
    pub command_id: CommandId,
    pub military_savings_contract_id: ResourceId,
    pub status: MilitarySavingsContractStatus,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CareerStoreResult<T> {
    Applied { receipt: T, save: Box<SaveState> },
    Rejected(CareerFailureCode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifeRateStatus {
    Active,
    RateUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResidenceTenureKind {
    RentFree,
    Owner,
    Jeonse,
    MonthlyRent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeHouseholdState {
    pub id: ResourceId,
    pub member_count: u32,
    pub dependent_count: u32,
    pub tax_dependent_eligible_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeResidenceState {
    pub id: ResourceId,
    pub region_key: String,
    pub tenure_kind: ResidenceTenureKind,
    pub property_holding_id: Option<ResourceId>,
    pub effective_from_game_day: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeBudgetBandState {
    pub id: ResourceId,
    pub band_key: String,
    pub display_name: String,
    pub factor_ppm: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeBudgetSelectionState {
    pub category: LivingCostCategory,
    pub band_id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LivingCostMonthItemState {
    pub category: LivingCostCategory,
    pub band_id: ResourceId,
    pub essential: bool,
    pub base_monthly_krw: i64,
    pub base_cpi_index: i64,
    pub region_factor_ppm: i64,
    pub household_factor_ppm: i64,
    pub budget_factor_ppm: i64,
    pub tenure_replacement_factor_ppm: i64,
    pub gross_krw: i64,
    pub paid_krw: i64,
    pub arrear_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LivingCostMonthState {
    pub id: ResourceId,
    pub profile_id: ResourceId,
    pub profile_key: String,
    pub year_month: YearMonth,
    pub current_cpi_index: i64,
    pub activation_game_day: u32,
    pub settlement_game_day: u32,
    pub proration_scale: u32,
    pub proration_units: u32,
    pub proration_days: u8,
    pub days_in_month: u8,
    pub settled: bool,
    pub total_gross_krw: i64,
    pub total_paid_krw: i64,
    pub total_arrear_krw: i64,
    pub items: Vec<LivingCostMonthItemState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EssentialArrearState {
    pub id: ResourceId,
    pub due_year_month: YearMonth,
    pub category: LivingCostCategory,
    pub original_krw: i64,
    pub remaining_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CreditReasonState {
    ModelUnavailable,
    ActiveDefault,
    ActiveDelinquency,
    CleanHistory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoanProductState {
    pub id: ResourceId,
    pub key: String,
    pub display_name: String,
    pub kind: LoanProductKind,
    pub lender_sector: LoanLenderSector,
    pub rate_status: LoanRateStatus,
    pub rate_type: LoanRateType,
    pub current_annual_rate_bp: Option<i64>,
    pub reference_rate_key: Option<LoanRateReference>,
    pub spread_bp: Option<i64>,
    pub minimum_annual_rate_bp: i64,
    pub maximum_annual_rate_bp: i64,
    pub rate_reset_rule: LoanRateResetRule,
    pub day_count_rule: LoanDayCountRule,
    pub repayment_method: LoanRepaymentMethod,
    pub term_months: u16,
    pub payment_calendar: LoanPaymentCalendar,
    pub grace_months: u16,
    pub minimum_principal_krw: i64,
    pub maximum_principal_krw: i64,
    pub prepayment_fee_ppm: u32,
    pub prepayment_effect: LoanPrepaymentEffect,
    pub starting_eligible: bool,
    pub quote_eligible: bool,
    pub execution_eligible: bool,
    pub prepayment_allowed: bool,
    pub dsr_included: bool,
    pub provenance: LoanProductProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoanProductCatalogState {
    pub credit_model_version_id: Option<ResourceId>,
    pub products: Vec<LoanProductState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoanSummaryState {
    pub id: ResourceId,
    pub product_version_id: ResourceId,
    pub product_kind: LoanProductKind,
    pub display_name: String,
    pub rate_status: LoanRateStatus,
    pub current_annual_rate_bp: Option<i64>,
    pub status: LoanContractStatus,
    pub remaining_principal_krw: i64,
    pub overdue_krw: i64,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoanDetailState {
    pub id: ResourceId,
    pub product_version_id: ResourceId,
    pub product_kind: LoanProductKind,
    pub display_name: String,
    pub rate_status: LoanRateStatus,
    pub current_annual_rate_bp: Option<i64>,
    pub status: LoanContractStatus,
    pub read_only: bool,
    pub original_principal_krw: i64,
    pub remaining_principal_krw: i64,
    pub accrued_interest_krw: i64,
    pub accrued_fee_krw: i64,
    pub overdue_krw: i64,
    pub repayment_method: LoanRepaymentMethod,
    pub term_months: Option<u16>,
    pub total_installments: Option<u16>,
    pub activated_game_day: u32,
    pub maturity_game_day: Option<u32>,
    pub final_installment_due_game_day: Option<u32>,
    pub next_installment_no: Option<u16>,
    pub oldest_unpaid_due_game_day: Option<u32>,
    pub prepayment_allowed: bool,
    pub prepayment_fee_ppm: Option<u32>,
    pub prepayment_effect: Option<LoanPrepaymentEffect>,
    pub dsr_included: bool,
    pub lease_contract_id: Option<ResourceId>,
    pub property_holding_id: Option<ResourceId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoanInstallmentStatusState {
    Pending,
    Due,
    PartiallyPaid,
    Paid,
    Cancelled,
    Discharged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoanInstallmentState {
    pub id: ResourceId,
    pub installment_no: u16,
    pub due_game_day: u32,
    pub interest_period_start_game_day: u32,
    pub elapsed_days: u16,
    pub annual_rate_bp: i64,
    pub opening_principal_krw: i64,
    pub scheduled_fee_krw: i64,
    pub scheduled_interest_krw: i64,
    pub scheduled_principal_krw: i64,
    pub paid_fee_krw: i64,
    pub paid_interest_krw: i64,
    pub paid_principal_krw: i64,
    pub remaining_due_krw: i64,
    pub status: LoanInstallmentStatusState,
    pub schedule_revision: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoanPaymentKindState {
    ScheduledInstallment,
    ManualPrepayment,
    LeaseMovePayoff,
    PropertySalePayoff,
    InsolvencyDistribution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LoanPaymentAllocationKindState {
    OverdueFee,
    OverdueInterest,
    OverduePrincipal,
    CurrentFee,
    CurrentInterest,
    CurrentPrincipal,
    PrepaymentFee,
    PrepaymentPrincipal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoanPaymentAllocationState {
    pub kind: LoanPaymentAllocationKindState,
    pub amount_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoanPaymentState {
    pub id: ResourceId,
    pub payment_no: u32,
    pub kind: LoanPaymentKindState,
    pub game_day: u32,
    pub amount_krw: i64,
    pub allocations: Vec<LoanPaymentAllocationState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoanInstallmentPageCursor {
    pub loan_id: ResourceId,
    pub installment_before: Option<u16>,
    pub payment_before: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoanInstallmentPageQuery {
    pub before: Option<LoanInstallmentPageCursor>,
    pub limit: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoanInstallmentPageState {
    pub loan_id: ResourceId,
    pub installments: Vec<LoanInstallmentState>,
    pub payments: Vec<LoanPaymentState>,
    pub has_more_installments: bool,
    pub has_more_payments: bool,
    pub next_before: Option<LoanInstallmentPageCursor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NextLoanInstallmentState {
    pub loan_id: ResourceId,
    pub installment_no: u16,
    pub due_game_day: u32,
    pub fee_krw: i64,
    pub interest_krw: i64,
    pub principal_krw: i64,
    pub remaining_due_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreditOverviewState {
    pub credit_band: Option<CreditBand>,
    pub credit_reasons: Vec<CreditReasonState>,
    pub active_loans: Vec<LoanSummaryState>,
    pub next_loan_installment: Option<NextLoanInstallmentState>,
    pub total_loan_balance_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoanQuoteDecisionState {
    Eligible,
    DebtServiceLimit,
    IncomeUnavailable,
    CreditRestricted,
    ValuationUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoanQuoteReasonState {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VerifiedIncomeSourceState {
    ActiveEmploymentContract,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoanQuoteDsrState {
    pub numerator_krw: i64,
    pub denominator_krw: i64,
    pub ratio_ppm: i64,
    pub limit_ppm: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoanQuoteFirstInstallmentState {
    pub due_game_day: u32,
    pub fee_krw: i64,
    pub principal_krw: i64,
    pub interest_krw: i64,
    pub total_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoanQuotedTermsState {
    pub annual_rate_bp: i64,
    pub repayment_method: LoanRepaymentMethod,
    pub term_months: u16,
    pub first_installment: LoanQuoteFirstInstallmentState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeBudgetState {
    pub rate_status: LifeRateStatus,
    pub household: LifeHouseholdState,
    pub residence: LifeResidenceState,
    pub allowed_bands: Vec<LifeBudgetBandState>,
    pub selections: Vec<LifeBudgetSelectionState>,
    pub current_month: Option<LivingCostMonthState>,
    pub active_arrears: Vec<EssentialArrearState>,
    pub has_more_active_arrears: bool,
    pub total_essential_arrear_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WelfareEvaluationStatusState {
    Eligible,
    Ineligible,
    Indeterminate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WelfareConditionOutcomeState {
    Passed,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WelfareApplicationStatusState {
    Applied,
    Approved,
    Rejected,
    Active,
    Exhausted,
    Terminated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WelfarePaymentStatusState {
    Pending,
    Paid,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WelfareConditionResultState {
    pub code: String,
    pub label: String,
    pub outcome: WelfareConditionOutcomeState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WelfarePaymentState {
    pub id: ResourceId,
    pub payment_no: u16,
    pub amount_krw: i64,
    pub due_game_day: u32,
    pub status: WelfarePaymentStatusState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WelfareApplicationSummaryState {
    pub id: ResourceId,
    pub status: WelfareApplicationStatusState,
    pub application_game_day: u32,
    pub approval_game_day: Option<u32>,
    pub paid_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WelfareProgramState {
    pub id: ResourceId,
    pub program_key: String,
    pub display_name: String,
    pub benefit_krw: i64,
    pub payment_delay_game_days: u16,
    pub evaluation_status: WelfareEvaluationStatusState,
    pub fact_fingerprint: String,
    pub conditions: Vec<WelfareConditionResultState>,
    pub application_available: bool,
    pub latest_application: Option<WelfareApplicationSummaryState>,
    pub next_payment: Option<WelfarePaymentState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WelfareProgramsState {
    pub component_version_id: ResourceId,
    pub game_day: u32,
    pub programs: Vec<WelfareProgramState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActiveWelfareApplicationState {
    pub application_id: ResourceId,
    pub program_version_id: ResourceId,
    pub program_key: String,
    pub display_name: String,
    pub status: WelfareApplicationStatusState,
    pub application_game_day: u32,
    pub approval_game_day: u32,
    pub benefit_krw: i64,
    pub paid_krw: i64,
    pub next_payment: Option<WelfarePaymentState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyWelfareProgramCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub program_version_id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WelfareApplicationReceipt {
    pub command_id: CommandId,
    pub application_id: ResourceId,
    pub program_version_id: ResourceId,
    pub status: WelfareApplicationStatusState,
    pub application_game_day: u32,
    pub approval_game_day: u32,
    pub eligibility_at_application: Vec<WelfareConditionResultState>,
    pub payment: WelfarePaymentState,
    pub replayed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifeEventCapabilityState {
    DeterministicChoices,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsuranceCapabilityState {
    ContractsAndClaims,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsuranceEligibilityStatusState {
    Eligible,
    Ineligible,
    Indeterminate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsuranceEligibilityReasonState {
    AgeOutsideRange,
    DependentRequired,
    ResidenceRequired,
    MilitaryServing,
    AuthorityUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsuranceContractStatusState {
    Active,
    Lapsed,
    Expired,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceProductState {
    pub id: ResourceId,
    pub product_key: String,
    pub display_name: String,
    pub eligibility_status: InsuranceEligibilityStatusState,
    pub reasons: Vec<InsuranceEligibilityReasonState>,
    pub covered_event_key: String,
    pub covered_event_display_name: String,
    pub premium_krw: i64,
    pub premium_interval_game_days: u16,
    pub term_game_days: u16,
    pub waiting_period_game_days: u16,
    pub deductible_krw: i64,
    pub occurrence_limit_krw: i64,
    pub term_limit_krw: i64,
    pub claim_window_game_days: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceContractState {
    pub id: ResourceId,
    pub product_version_id: ResourceId,
    pub product_key: String,
    pub display_name: String,
    pub status: InsuranceContractStatusState,
    pub coverage_start_game_day: u32,
    pub waiting_ends_game_day: u32,
    pub coverage_end_exclusive: u32,
    pub next_premium_due_game_day: Option<u32>,
    pub premium_krw: i64,
    pub paid_benefit_krw: i64,
    pub reserved_benefit_krw: i64,
    pub remaining_benefit_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceClaimAllocationState {
    pub contract_id: ResourceId,
    pub deductible_krw: i64,
    pub payout_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PendingInsuranceClaimState {
    Candidate {
        id: ResourceId,
        event_id: ResourceId,
        event_key: String,
        event_display_name: String,
        offered_game_day: u32,
    },
    Ready {
        id: ResourceId,
        event_id: ResourceId,
        event_key: String,
        event_display_name: String,
        offered_game_day: u32,
        gross_cost_krw: i64,
        payout_krw: i64,
        filing_deadline_game_day: u32,
        contract_allocations: Vec<InsuranceClaimAllocationState>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum InsuranceClaimHistoryState {
    NotApplicable {
        id: ResourceId,
        event_id: ResourceId,
        event_key: String,
        event_display_name: String,
        offered_game_day: u32,
        resolved_game_day: u32,
    },
    NotCovered {
        id: ResourceId,
        event_id: ResourceId,
        event_key: String,
        event_display_name: String,
        offered_game_day: u32,
        resolved_game_day: u32,
        gross_cost_krw: i64,
    },
    Paid {
        id: ResourceId,
        event_id: ResourceId,
        event_key: String,
        event_display_name: String,
        offered_game_day: u32,
        resolved_game_day: u32,
        gross_cost_krw: i64,
        payout_krw: i64,
        filing_deadline_game_day: u32,
        paid_game_day: u32,
        contract_allocations: Vec<InsuranceClaimAllocationState>,
    },
    Expired {
        id: ResourceId,
        event_id: ResourceId,
        event_key: String,
        event_display_name: String,
        offered_game_day: u32,
        resolved_game_day: u32,
        gross_cost_krw: i64,
        payout_krw: i64,
        filing_deadline_game_day: u32,
        contract_allocations: Vec<InsuranceClaimAllocationState>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsuranceQueryState {
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsuranceState {
    pub capability: InsuranceCapabilityState,
    pub products: Vec<InsuranceProductState>,
    pub contracts: Vec<InsuranceContractState>,
    pub pending_claims: Vec<PendingInsuranceClaimState>,
    pub history: Vec<InsuranceClaimHistoryState>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsuranceReadResult {
    Found(InsuranceState),
    Rejected(LifeFailureCode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsuranceSnapshotState {
    pub capability: InsuranceCapabilityState,
    pub active_contracts: Vec<InsuranceContractState>,
    pub pending_claims: Vec<PendingInsuranceClaimState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsolvencyAvailabilityState {
    Unavailable,
    CashOnlyLiquidation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsolvencyCaseSummaryState {
    pub id: ResourceId,
    pub procedure_kind: InsolvencyProcedureKind,
    pub status: InsolvencyCaseStatus,
    pub prepared_game_day: u32,
    pub submitted_game_day: Option<u32>,
    pub wallet_cash_krw: i64,
    pub protected_cash_krw: i64,
    pub distributed_krw: i64,
    pub discharged_krw: i64,
    pub credit_restriction_end_exclusive: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsolvencySnapshotState {
    pub availability: InsolvencyAvailabilityState,
    pub eligibility: InsolvencyEligibilityStatus,
    pub reasons: Vec<InsolvencyEligibilityReason>,
    pub current_case: Option<InsolvencyCaseSummaryState>,
}

impl InsolvencySnapshotState {
    pub fn unavailable() -> Self {
        Self {
            availability: InsolvencyAvailabilityState::Unavailable,
            eligibility: InsolvencyEligibilityStatus::Unavailable,
            reasons: vec![InsolvencyEligibilityReason::ComponentUnavailable],
            current_case: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsolvencyReadResult<T> {
    Found(T),
    Rejected(LifeFailureCode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsolvencyActionState {
    Submit,
    Withdraw,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareInsolvencyCaseCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub procedure_kind: InsolvencyProcedureKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActOnInsolvencyCaseCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub case_id: ResourceId,
    pub action: InsolvencyActionState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsolvencyCaseReceipt {
    pub command_id: CommandId,
    pub case: InsolvencyCaseSummaryState,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsolvencyTransitionState {
    pub sequence: u8,
    pub from_status: Option<InsolvencyCaseStatus>,
    pub to_status: InsolvencyCaseStatus,
    pub game_day: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsolvencyCaseDetailState {
    pub summary: InsolvencyCaseSummaryState,
    pub policy_set_id: ResourceId,
    pub life_catalog_set_id: ResourceId,
    pub insolvency_component_version_id: ResourceId,
    pub composition_sha256: String,
    pub automatic_protected_krw: i64,
    pub additional_protected_krw: i64,
    pub liquidatable_krw: i64,
    pub total_claim_krw: i64,
    pub claim_count: u8,
    pub transitions: Vec<InsolvencyTransitionState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsolvencyClaimState {
    pub id: ResourceId,
    pub loan_contract_id: ResourceId,
    pub principal_krw: i64,
    pub interest_krw: i64,
    pub fee_krw: i64,
    pub allowed_krw: i64,
    pub distributed_krw: i64,
    pub discharged_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsolvencyClaimPageState {
    pub claims: Vec<InsolvencyClaimState>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsolvencyLiquidationState {
    pub id: ResourceId,
    pub claim_id: ResourceId,
    pub amount_krw: i64,
    pub loan_payment_id: ResourceId,
    pub ledger_transaction_id: ResourceId,
    pub applied_game_day: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsolvencyLiquidationPageState {
    pub wallet_asset: Option<InsolvencyWalletAssetState>,
    pub distributions: Vec<InsolvencyLiquidationState>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsolvencyWalletAssetState {
    pub original_amount_krw: i64,
    pub protected_amount_krw: i64,
    pub liquidatable_krw: i64,
    pub distributed_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CorporationAvailabilityState {
    Unavailable,
    Active,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CorporationStatusState {
    Draft,
    Active,
    Dormant,
    Insolvent,
    Dissolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorporationOperatingScaleState {
    pub id: ResourceId,
    pub scale_key: String,
    pub scale_order: u8,
    pub revenue_factor_ppm: u32,
    pub fixed_cost_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorporationTemplateState {
    pub id: ResourceId,
    pub template_key: String,
    pub display_name: String,
    pub template_order: u8,
    pub base_monthly_revenue_krw: i64,
    pub revenue_variation_ppm: u32,
    pub variable_cost_ppm: u32,
    pub fixed_monthly_cost_krw: i64,
    pub operating_scales: Vec<CorporationOperatingScaleState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CorporationOperatingSettingState {
    pub id: ResourceId,
    pub corporation_id: ResourceId,
    pub operating_scale_id: ResourceId,
    pub scale_key: String,
    pub scale_order: u8,
    pub revenue_factor_ppm: u32,
    pub fixed_cost_krw: i64,
    pub effective_year: u16,
    pub effective_month: u8,
    pub officer_gross_salary_krw: i64,
    pub created_game_day: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CorporationNextMonthSettingState {
    pub setting_id: Option<ResourceId>,
    pub operating_scale_id: ResourceId,
    pub scale_key: String,
    pub scale_order: u8,
    pub revenue_factor_ppm: u32,
    pub fixed_cost_krw: i64,
    pub effective_year: u16,
    pub effective_month: u8,
    pub officer_gross_salary_krw: i64,
    pub created_game_day: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorporationTemplatesState {
    pub availability: CorporationAvailabilityState,
    pub component_version_id: Option<ResourceId>,
    pub registered_office_class: Option<String>,
    pub minimum_capital_krw: Option<i64>,
    pub maximum_capital_krw: Option<i64>,
    pub game_administrative_fee_krw: Option<i64>,
    pub templates: Vec<CorporationTemplateState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CorporationSummaryState {
    pub id: ResourceId,
    pub component_version_id: ResourceId,
    pub industry_template_id: ResourceId,
    pub template_key: String,
    pub template_display_name: String,
    pub name: String,
    pub representative_name: String,
    pub status: CorporationStatusState,
    pub established_game_day: u32,
    pub capital_krw: i64,
    pub registration_license_tax_krw: i64,
    pub local_education_tax_krw: i64,
    pub game_administrative_fee_krw: i64,
    pub total_establishment_fee_krw: i64,
    pub cash_krw: i64,
    pub contributed_capital_krw: i64,
    pub retained_earnings_krw: i64,
    pub operating_payable_krw: i64,
    pub corporate_tax_payable_krw: i64,
    pub distributable_profit_krw: i64,
    pub personal_ledger_transaction_id: ResourceId,
    pub corporation_ledger_transaction_id: ResourceId,
    pub next_month_setting: CorporationNextMonthSettingState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CorporationSnapshotState {
    pub availability: CorporationAvailabilityState,
    pub current: Option<CorporationSummaryState>,
}

impl CorporationSnapshotState {
    pub const fn unavailable() -> Self {
        Self {
            availability: CorporationAvailabilityState::Unavailable,
            current: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateCorporationCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub industry_template_id: ResourceId,
    pub name: String,
    pub capital_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CorporationReceipt {
    pub command_id: CommandId,
    pub corporation: CorporationSummaryState,
    pub wallet_debit_krw: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCorporationSettingsCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub corporation_id: ResourceId,
    pub operating_scale_id: ResourceId,
    pub officer_gross_salary_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CorporationSettingsReceipt {
    pub command_id: CommandId,
    pub setting: CorporationOperatingSettingState,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayCorporationDividendCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub corporation_id: ResourceId,
    pub gross_dividend_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CorporationDividendReceipt {
    pub command_id: CommandId,
    pub id: ResourceId,
    pub corporation_id: ResourceId,
    pub tax_year: u16,
    pub gross_dividend_krw: i64,
    pub withheld_income_tax_krw: i64,
    pub withheld_local_income_tax_krw: i64,
    pub net_dividend_krw: i64,
    pub corporation_ledger_transaction_id: ResourceId,
    pub personal_ledger_transaction_id: ResourceId,
    pub paid_game_day: u32,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorporationOperatingMonthState {
    pub id: ResourceId,
    pub operating_year: u16,
    pub operating_month: u8,
    pub scale_key: String,
    pub officer_gross_salary_krw: i64,
    pub revenue_krw: i64,
    pub operating_expense_krw: i64,
    pub total_payroll_cost_krw: i64,
    pub pre_tax_profit_krw: i64,
    pub payroll_status: String,
    pub cash_after_krw: i64,
    pub operating_payable_after_krw: i64,
    pub retained_earnings_after_krw: i64,
    pub applied_game_day: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorporationOperatingMonthPageState {
    pub months: Vec<CorporationOperatingMonthState>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BusinessOperationsAvailabilityState {
    Unavailable,
    Active,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BusinessContractStatusState {
    Offered,
    Accepted,
    Active,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BusinessPositionStatusState {
    Vacant,
    Hired,
    Active,
    Resigned,
    Terminated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BusinessMarketingBandState {
    pub id: ResourceId,
    pub band_key: String,
    pub display_name: String,
    pub band_order: u16,
    pub monthly_cost_krw: i64,
    pub offer_slots: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BusinessLoanProductState {
    pub id: ResourceId,
    pub product_key: String,
    pub display_name: String,
    pub minimum_principal_krw: i64,
    pub maximum_principal_krw: i64,
    pub principal_step_krw: i64,
    pub monthly_interest_rate_ppm: u32,
    pub term_months: u16,
    pub personal_guarantee: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BusinessWorkingCapitalLoanStatusState {
    Active,
    Matured,
    Repaid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BusinessWorkingCapitalLoanState {
    pub id: ResourceId,
    pub product_id: ResourceId,
    pub product_key: String,
    pub display_name: String,
    pub status: BusinessWorkingCapitalLoanStatusState,
    pub original_principal_krw: i64,
    pub outstanding_principal_krw: i64,
    pub monthly_interest_rate_ppm: u32,
    pub term_months: u16,
    pub originated_year: u16,
    pub originated_month: u8,
    pub maturity_year: u16,
    pub maturity_month: u8,
    pub personal_guarantee: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BusinessContractState {
    pub id: ResourceId,
    pub template_key: String,
    pub display_name: String,
    pub status: BusinessContractStatusState,
    pub service_year: u16,
    pub service_month: u8,
    pub required_capacity_units: u16,
    pub revenue_krw: i64,
    pub variable_cost_ppm: u32,
    pub failure_penalty_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BusinessPositionState {
    pub id: ResourceId,
    pub role_key: String,
    pub display_name: String,
    pub status: BusinessPositionStatusState,
    pub capacity_units: u16,
    pub monthly_gross_wage_krw: i64,
    pub employer_cost_rate_ppm: u32,
    pub effective_year: Option<u16>,
    pub effective_month: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BusinessMonthlyPlanState {
    pub id: ResourceId,
    pub effective_year: u16,
    pub effective_month: u8,
    pub plan_revision: u64,
    pub marketing_band_id: ResourceId,
    pub marketing_band_key: String,
    pub cash_buffer_krw: i64,
    pub contract_priority_ids: Vec<ResourceId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BusinessMonthState {
    pub id: ResourceId,
    pub operating_year: u16,
    pub operating_month: u8,
    pub total_capacity_units: u32,
    pub used_capacity_units: u32,
    pub contract_revenue_krw: i64,
    pub contract_variable_cost_krw: i64,
    pub marketing_cost_krw: i64,
    pub employee_cost_krw: i64,
    pub failed_contract_penalty_krw: i64,
    pub loan_interest_cost_krw: i64,
    pub completed_contract_count: u16,
    pub failed_contract_count: u16,
    pub active_employee_count: u16,
    pub applied_game_day: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BusinessOperationsState {
    pub availability: BusinessOperationsAvailabilityState,
    pub corporation_id: ResourceId,
    pub catalog_version_id: Option<ResourceId>,
    pub catalog_sha256: Option<String>,
    pub revision: u64,
    pub next_operating_year: Option<u16>,
    pub next_operating_month: Option<u8>,
    pub marketing_bands: Vec<BusinessMarketingBandState>,
    pub loan_products: Vec<BusinessLoanProductState>,
    pub working_capital_loans: Vec<BusinessWorkingCapitalLoanState>,
    pub contracts: Vec<BusinessContractState>,
    pub positions: Vec<BusinessPositionState>,
    pub plan: Option<BusinessMonthlyPlanState>,
    pub latest_month: Option<BusinessMonthState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusinessOperationAction {
    AcceptContract {
        contract_id: ResourceId,
    },
    CancelContract {
        contract_id: ResourceId,
    },
    HirePosition {
        position_id: ResourceId,
    },
    TerminatePosition {
        position_id: ResourceId,
    },
    SetMonthlyPlan {
        marketing_band_id: ResourceId,
        cash_buffer_krw: i64,
        contract_priority_ids: Vec<ResourceId>,
    },
    CapitalContribution {
        amount_krw: i64,
    },
    DrawWorkingCapitalLoan {
        loan_product_id: ResourceId,
        principal_krw: i64,
    },
    RepayWorkingCapitalLoan {
        loan_id: ResourceId,
        principal_krw: i64,
    },
    Dissolve,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManageBusinessOperationsCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub corporation_id: ResourceId,
    pub expected_revision: u64,
    pub action: BusinessOperationAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub enum BusinessOperationResultState {
    AcceptContract {
        contract: BusinessContractState,
    },
    CancelContract {
        contract: BusinessContractState,
    },
    HirePosition {
        position: BusinessPositionState,
    },
    TerminatePosition {
        position: BusinessPositionState,
    },
    SetMonthlyPlan {
        plan: BusinessMonthlyPlanState,
    },
    CapitalContribution {
        contribution_id: ResourceId,
        amount_krw: i64,
        corporation_cash_after_krw: i64,
        contributed_capital_after_krw: i64,
        wallet_cash_after_krw: i64,
        corporation_ledger_transaction_id: ResourceId,
        personal_ledger_transaction_id: ResourceId,
    },
    DrawWorkingCapitalLoan {
        loan: BusinessWorkingCapitalLoanState,
    },
    RepayWorkingCapitalLoan {
        loan: BusinessWorkingCapitalLoanState,
    },
    Dissolve {
        dissolution_id: ResourceId,
        distribution_krw: i64,
        capital_basis_krw: i64,
        realized_gain_loss_krw: i64,
        wallet_cash_after_krw: i64,
        corporation_ledger_transaction_id: ResourceId,
        personal_ledger_transaction_id: ResourceId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BusinessOperationReceipt {
    pub command_id: CommandId,
    pub result: BusinessOperationResultState,
    pub revision: u64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorporationReadResult<T> {
    Found(T),
    Rejected(LifeFailureCode),
}

impl InsuranceSnapshotState {
    pub fn unavailable() -> Self {
        Self {
            capability: InsuranceCapabilityState::Unavailable,
            active_contracts: Vec::new(),
            pending_claims: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrollInsuranceContractCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub product_version_id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelInsuranceContractCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub contract_id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInsuranceClaimCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub claim_id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceEnrollmentReceipt {
    pub command_id: CommandId,
    pub contract_id: ResourceId,
    pub product_version_id: ResourceId,
    pub status: InsuranceContractStatusState,
    pub coverage_start_game_day: u32,
    pub waiting_ends_game_day: u32,
    pub coverage_end_exclusive: u32,
    pub next_premium_due_game_day: u32,
    pub premium_krw: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceCancellationReceipt {
    pub command_id: CommandId,
    pub contract_id: ResourceId,
    pub status: InsuranceContractStatusState,
    pub coverage_end_exclusive: u32,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsuranceClaimReceipt {
    pub command_id: CommandId,
    pub claim_id: ResourceId,
    pub event_id: ResourceId,
    pub payout_krw: i64,
    pub paid_game_day: u32,
    pub replayed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifeEventDecisionKindState {
    Accepted,
    Declined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifeEventResolutionKindState {
    Accepted,
    Declined,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum LifeEventEffectSummaryState {
    NoEffect,
    WalletExpense { amount_krw: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeEventChoiceState {
    pub id: ResourceId,
    pub display_name: String,
    pub decision_kind: LifeEventDecisionKindState,
    pub effect_summary: LifeEventEffectSummaryState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingLifeEventState {
    pub id: ResourceId,
    pub event_key: String,
    pub display_name: String,
    pub offered_game_day: u32,
    pub expires_game_day: u32,
    pub default_choice_id: ResourceId,
    pub choices: Vec<LifeEventChoiceState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifeEventHistoryItemState {
    pub id: ResourceId,
    pub event_key: String,
    pub display_name: String,
    pub offered_game_day: u32,
    pub resolved_game_day: u32,
    pub resolution_kind: LifeEventResolutionKindState,
    pub choice: LifeEventChoiceState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifeEventsQueryState {
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifeEventsState {
    pub capability: LifeEventCapabilityState,
    pub insurance_capability: InsuranceCapabilityState,
    pub pending_events: Vec<PendingLifeEventState>,
    pub history: Vec<LifeEventHistoryItemState>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifeEventsReadResult {
    Found(LifeEventsState),
    Rejected(LifeFailureCode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveLifeEventCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub event_id: ResourceId,
    pub choice_id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeEventChoiceReceipt {
    pub command_id: CommandId,
    pub event_id: ResourceId,
    pub choice_id: ResourceId,
    pub resolution_kind: LifeEventDecisionKindState,
    pub resolved_game_day: u32,
    pub wallet_delta_krw: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeSnapshotState {
    pub rate_status: LifeRateStatus,
    pub household: Option<LifeHouseholdState>,
    pub residence: Option<LifeResidenceState>,
    pub current_month: Option<LivingCostMonthState>,
    pub active_arrears: Vec<EssentialArrearState>,
    pub has_more_active_arrears: bool,
    pub total_essential_arrear_krw: i64,
    pub credit_band: Option<CreditBand>,
    pub credit_reasons: Vec<CreditReasonState>,
    pub active_loans: Vec<LoanSummaryState>,
    pub next_loan_installment: Option<NextLoanInstallmentState>,
    pub total_loan_balance_krw: i64,
    pub tenant_lease_deposit_krw: i64,
    pub active_lease: Option<ActiveHousingLeaseState>,
    pub active_lease_arrears: Vec<LeaseArrearState>,
    pub has_more_active_lease_arrears: bool,
    pub total_lease_arrear_krw: i64,
    pub active_property_holdings: Vec<PropertyHoldingState>,
    pub has_more_active_property_holdings: bool,
    pub total_property_book_value_krw: i64,
    pub active_welfare_applications: Vec<ActiveWelfareApplicationState>,
    pub insurance_capability: InsuranceCapabilityState,
    pub active_insurance_contracts: Vec<InsuranceContractState>,
    pub pending_insurance_claims: Vec<PendingInsuranceClaimState>,
    pub insolvency: InsolvencySnapshotState,
    pub corporation: CorporationSnapshotState,
    pub pending_events: Vec<PendingLifeEventState>,
}

impl LifeSnapshotState {
    pub fn empty() -> Self {
        Self {
            rate_status: LifeRateStatus::RateUnavailable,
            household: None,
            residence: None,
            current_month: None,
            active_arrears: Vec::new(),
            has_more_active_arrears: false,
            total_essential_arrear_krw: 0,
            credit_band: None,
            credit_reasons: vec![CreditReasonState::ModelUnavailable],
            active_loans: Vec::new(),
            next_loan_installment: None,
            total_loan_balance_krw: 0,
            tenant_lease_deposit_krw: 0,
            active_lease: None,
            active_lease_arrears: Vec::new(),
            has_more_active_lease_arrears: false,
            total_lease_arrear_krw: 0,
            active_property_holdings: Vec::new(),
            has_more_active_property_holdings: false,
            total_property_book_value_krw: 0,
            active_welfare_applications: Vec::new(),
            insurance_capability: InsuranceCapabilityState::Unavailable,
            active_insurance_contracts: Vec::new(),
            pending_insurance_claims: Vec::new(),
            insolvency: InsolvencySnapshotState::unavailable(),
            corporation: CorporationSnapshotState::unavailable(),
            pending_events: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateLifeBudgetCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub selections: Vec<LifeBudgetSelectionState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayEssentialArrearCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub arrear_id: ResourceId,
    pub amount_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayLeaseArrearCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub arrear_id: ResourceId,
    pub amount_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateLoanQuoteCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub product_version_id: ResourceId,
    pub principal_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateLeaseDepositLoanQuoteCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub listing_id: ResourceId,
    pub offer_kind: HousingLeaseOfferKind,
    pub product_version_id: ResourceId,
    pub principal_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateMortgageQuoteCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub listing_id: ResourceId,
    pub product_version_id: ResourceId,
    pub principal_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecuteLoanCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub quote_id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepayLoanCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub loan_id: ResourceId,
    pub principal_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateLifeBudgetReceipt {
    pub command_id: CommandId,
    pub applied_game_day: u32,
    pub selections: Vec<LifeBudgetSelectionState>,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EssentialArrearPaymentReceipt {
    pub command_id: CommandId,
    pub arrear_id: ResourceId,
    pub paid_krw: i64,
    pub remaining_krw: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LeaseArrearPaymentReceipt {
    pub command_id: CommandId,
    pub arrear_id: ResourceId,
    pub payment_id: ResourceId,
    pub paid_krw: i64,
    pub remaining_krw: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoanQuoteReceipt {
    pub command_id: CommandId,
    pub quote_id: ResourceId,
    pub product_version_id: ResourceId,
    pub requested_principal_krw: i64,
    pub created_game_day: u32,
    pub expires_game_day: u32,
    pub decision_code: LoanQuoteDecisionState,
    pub decision_reasons: Vec<LoanQuoteReasonState>,
    pub verified_annual_income_krw: Option<i64>,
    pub verified_income_source: Option<VerifiedIncomeSourceState>,
    pub existing_loan_balance_krw: i64,
    pub post_execution_balance_krw: i64,
    pub dsr_applied: bool,
    pub dsr: Option<LoanQuoteDsrState>,
    pub stress_rate_bp: i64,
    pub quoted_terms: LoanQuotedTermsState,
    pub replayed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LeaseDepositLoanQuoteDecisionState {
    Eligible,
    CreditRestricted,
    CollateralLimit,
    IncomeUnavailable,
    AffordabilityLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LeaseDepositLoanQuoteReasonState {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LeaseDepositLoanAffordabilityState {
    pub numerator_krw: i64,
    pub denominator_krw: i64,
    pub ratio_ppm: i64,
    pub limit_ppm: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LeaseDepositLoanQuoteReceipt {
    pub command_id: CommandId,
    pub quote_id: ResourceId,
    pub listing_id: ResourceId,
    pub offer_kind: HousingLeaseOfferKind,
    pub product_version_id: ResourceId,
    pub requested_principal_krw: i64,
    pub deposit_krw: i64,
    pub funding_limit_ppm: i64,
    pub maximum_funding_krw: i64,
    pub created_game_day: u32,
    pub expires_game_day: u32,
    pub decision_code: LeaseDepositLoanQuoteDecisionState,
    pub decision_reasons: Vec<LeaseDepositLoanQuoteReasonState>,
    pub verified_annual_income_krw: Option<i64>,
    pub verified_income_source: Option<VerifiedIncomeSourceState>,
    pub existing_loan_balance_krw: i64,
    pub post_execution_balance_krw: i64,
    pub regulatory_dsr_applied: bool,
    pub affordability: Option<LeaseDepositLoanAffordabilityState>,
    pub quoted_terms: LoanQuotedTermsState,
    pub replaced_loan_id: Option<ResourceId>,
    pub replaced_loan_principal_krw: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MortgageQuoteDecisionState {
    Eligible,
    CreditRestricted,
    PurchaseRestricted,
    CollateralLimit,
    IncomeUnavailable,
    DebtServiceLimit,
    InsufficientOwnFunds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MortgageQuoteReasonState {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MortgageLtvRegionClassState {
    RegulatedCapitalProxy,
    NonRegulatedProxy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MortgageStressTreatmentState {
    FullTermFixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoanQuoteLtvState {
    pub numerator_krw: i64,
    pub denominator_krw: i64,
    pub ratio_ppm: i64,
    pub limit_ppm: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MortgageQuoteReceipt {
    pub command_id: CommandId,
    pub quote_id: ResourceId,
    pub listing_id: ResourceId,
    pub product_version_id: ResourceId,
    pub requested_principal_krw: i64,
    pub purchase_price_krw: i64,
    pub recognized_collateral_value_krw: i64,
    pub ltv_region_class: MortgageLtvRegionClassState,
    pub ltv_limit_ppm: i64,
    pub maximum_mortgage_krw: i64,
    pub ltv: LoanQuoteLtvState,
    pub created_game_day: u32,
    pub expires_game_day: u32,
    pub decision_code: MortgageQuoteDecisionState,
    pub decision_reasons: Vec<MortgageQuoteReasonState>,
    pub verified_annual_income_krw: Option<i64>,
    pub verified_income_source: Option<VerifiedIncomeSourceState>,
    pub existing_loan_balance_krw: i64,
    pub post_execution_balance_krw: i64,
    pub dsr_applied: bool,
    pub dsr: Option<LoanQuoteDsrState>,
    pub stress_rate_bp: i64,
    pub stress_treatment: MortgageStressTreatmentState,
    pub acquisition_incidental_cost_krw: i64,
    pub moving_cost_krw: i64,
    pub returned_deposit_krw: i64,
    pub replaced_loan_id: Option<ResourceId>,
    pub replaced_loan_principal_krw: i64,
    pub available_buyer_cash_krw: i64,
    pub required_buyer_cash_krw: i64,
    pub quoted_terms: LoanQuotedTermsState,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoanExecutionReceipt {
    pub command_id: CommandId,
    pub loan_id: ResourceId,
    pub quote_id: ResourceId,
    pub product_version_id: ResourceId,
    pub principal_krw: i64,
    pub activated_game_day: u32,
    pub maturity_game_day: u32,
    pub annual_rate_bp: i64,
    pub repayment_method: LoanRepaymentMethod,
    pub term_months: u16,
    pub first_installment: LoanQuoteFirstInstallmentState,
    pub replayed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoanPrepaymentStatusState {
    Active,
    PaidOff,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoanPrepaymentNextInstallmentState {
    pub installment_no: u16,
    pub due_game_day: u32,
    pub fee_krw: i64,
    pub principal_krw: i64,
    pub interest_krw: i64,
    pub total_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoanPrepaymentReceipt {
    pub command_id: CommandId,
    pub loan_id: ResourceId,
    pub payment_id: ResourceId,
    pub principal_krw: i64,
    pub fee_krw: i64,
    pub total_debited_krw: i64,
    pub applied_game_day: u32,
    pub remaining_principal_krw: i64,
    pub status: LoanPrepaymentStatusState,
    pub prepayment_effect: LoanPrepaymentEffect,
    pub remaining_installments: u16,
    pub next_installment: Option<LoanPrepaymentNextInstallmentState>,
    pub final_installment_due_game_day: Option<u32>,
    pub replayed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifeFailureCode {
    InvalidCommand,
    CharacterRequired,
    HousingResourceNotFound,
    WelfareResourceNotFound,
    EventNotFound,
    EventExpired,
    InsuranceResourceNotFound,
    InsolvencyResourceNotFound,
    InsolvencyCompositionUnsupported,
    InsolvencyCompositionChanged,
    InsolvencyStateConflict,
    CorporationResourceNotFound,
    CorporationStateConflict,
    ClaimNotCovered,
    InsufficientWalletCash,
    RateUnavailable,
    CreditRestricted,
    IncomeUnavailable,
    DebtServiceLimit,
    CollateralLimit,
    AffordabilityLimit,
    ContractConflict,
    IdempotencyConflict,
    SettlementConflict,
    PolicyUnsupported,
    Ineligible,
    ValuationUnavailable,
    Busy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HousingListingsQueryState {
    pub region: Option<LifeRegionKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HousingRateStatusState {
    Active,
    RateUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HousingRegionState {
    pub region_key: LifeRegionKey,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HousingListingState {
    pub id: ResourceId,
    pub region_key: LifeRegionKey,
    pub property_type: PropertyType,
    pub exclusive_area_square_meters: u16,
    pub available_from_game_day: u32,
    pub available_to_game_day: u32,
    pub offers: Vec<PropertyListingOffer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HousingListingsState {
    pub rate_status: HousingRateStatusState,
    pub model_version_id: ResourceId,
    pub game_day: u32,
    pub year_month: YearMonth,
    pub residence_region_key: LifeRegionKey,
    pub selected_region_key: LifeRegionKey,
    pub regions: Vec<HousingRegionState>,
    pub price_index_ppm: Option<i64>,
    pub rent_index_ppm: Option<i64>,
    pub listings: Vec<HousingListingState>,
}

pub type HousingLeaseCapabilityState = HousingLeaseCapability;
pub type HousingLeaseRenewalRuleState = HousingLeaseRenewalRule;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MonthlyRentTermsState {
    pub rent_charge_rule: HousingRentChargeRule,
    pub arrear_repayment_rule: HousingLeaseArrearRepaymentRule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MonthlyRentTerminationReviewTermsState {
    pub rule: HousingLeaseTerminationReviewRule,
    pub after_game_days: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LeaseLifecycleTermsState {
    pub term_months: u16,
    pub renewal_notice_lead_days: u16,
    pub monthly_rent_termination_review: Option<MonthlyRentTerminationReviewTermsState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActiveLeaseTermState {
    pub term_no: u32,
    pub effective_from_game_day: u32,
    pub effective_to_game_day: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LeaseRenewalNoticeState {
    pub term_no: u32,
    pub published_game_day: u32,
    pub renews_on_game_day: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum LeaseTerminationReviewStatusState {
    UnderReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LeaseTerminationReviewState {
    pub status: LeaseTerminationReviewStatusState,
    pub opened_game_day: u32,
    pub trigger_arrear_id: ResourceId,
    pub active_lease_arrear_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LeaseArrearState {
    pub id: ResourceId,
    pub lease_id: ResourceId,
    pub rent_charge_id: ResourceId,
    pub due_year_month: YearMonth,
    pub original_krw: i64,
    pub paid_krw: i64,
    pub remaining_krw: i64,
    pub created_game_day: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HousingMovingCostState {
    pub region_key: LifeRegionKey,
    pub moving_cost_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActiveHousingLeaseState {
    pub id: ResourceId,
    pub listing_id: ResourceId,
    pub role: HousingLeaseRole,
    pub offer_kind: HousingLeaseOfferKind,
    pub region_key: LifeRegionKey,
    pub property_type: PropertyType,
    pub exclusive_area_square_meters: u16,
    pub deposit_krw: i64,
    pub monthly_rent_krw: Option<i64>,
    pub next_rent_due_game_day: Option<u32>,
    pub effective_from_game_day: u32,
    pub effective_to_game_day: Option<u32>,
    pub renewal_rule: HousingLeaseRenewalRuleState,
    pub current_term: Option<ActiveLeaseTermState>,
    pub renewal_notice: Option<LeaseRenewalNoticeState>,
    pub termination_review: Option<LeaseTerminationReviewState>,
    pub deposit_loan_id: Option<ResourceId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HousingLeaseCurrentState {
    pub lease_capability: HousingLeaseCapabilityState,
    pub renewal_rule: Option<HousingLeaseRenewalRuleState>,
    pub lease_lifecycle_terms: Option<LeaseLifecycleTermsState>,
    pub moving_costs: Vec<HousingMovingCostState>,
    pub tenant_lease_deposit_krw: i64,
    pub active_lease: Option<ActiveHousingLeaseState>,
    pub monthly_rent_terms: Option<MonthlyRentTermsState>,
    pub active_arrears: Vec<LeaseArrearState>,
    pub has_more_active_arrears: bool,
    pub total_lease_arrear_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartHousingLeaseCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub listing_id: ResourceId,
    pub offer_kind: HousingLeaseOfferKind,
    pub loan_quote_id: Option<ResourceId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DepositLoanExecutionReceipt {
    pub loan_id: ResourceId,
    pub quote_id: ResourceId,
    pub product_version_id: ResourceId,
    pub principal_krw: i64,
    pub annual_rate_bp: i64,
    pub maturity_game_day: u32,
    pub first_installment: LoanQuoteFirstInstallmentState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepaidDepositLoanReceipt {
    pub loan_id: ResourceId,
    pub payment_id: ResourceId,
    pub principal_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HousingPurchaseCapabilityState {
    OwnerOccupiedSingleHome,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertyHoldingStatusState {
    Active,
    Disposed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertyHoldingPurposeState {
    OwnerOccupied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertyHoldingState {
    pub id: ResourceId,
    pub listing_id: ResourceId,
    pub status: PropertyHoldingStatusState,
    pub purpose: PropertyHoldingPurposeState,
    pub region_key: LifeRegionKey,
    pub property_type: PropertyType,
    pub exclusive_area_square_meters: u16,
    pub acquired_game_day: u32,
    pub acquisition_price_krw: i64,
    pub acquisition_incidental_cost_krw: i64,
    pub book_value_krw: i64,
    pub mortgage_loan_id: Option<ResourceId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HousingPropertyHoldingsState {
    pub purchase_capability: HousingPurchaseCapabilityState,
    pub maximum_active_holdings: u8,
    pub holdings: Vec<PropertyHoldingState>,
    pub total_property_book_value_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurchasePropertyCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub listing_id: ResourceId,
    pub mortgage_quote_id: Option<ResourceId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MortgageExecutionReceipt {
    pub loan_id: ResourceId,
    pub quote_id: ResourceId,
    pub product_version_id: ResourceId,
    pub property_holding_id: ResourceId,
    pub principal_krw: i64,
    pub activated_game_day: u32,
    pub annual_rate_bp: i64,
    pub maturity_game_day: u32,
    pub repayment_method: LoanRepaymentMethod,
    pub term_months: u16,
    pub first_installment: LoanQuoteFirstInstallmentState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertyPurchaseReceipt {
    pub command_id: CommandId,
    pub holding: PropertyHoldingState,
    pub residence_id: ResourceId,
    pub listing_id: ResourceId,
    pub purchase_price_krw: i64,
    pub acquisition_incidental_cost_krw: i64,
    pub moving_cost_krw: i64,
    pub returned_deposit_krw: i64,
    pub wallet_delta_krw: i64,
    pub effective_from_game_day: u32,
    pub ended_lease_id: Option<ResourceId>,
    pub repaid_deposit_loan: Option<RepaidDepositLoanReceipt>,
    pub mortgage_execution: Option<MortgageExecutionReceipt>,
    pub replayed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertySaleOrderStatusState {
    Active,
    Filled,
    Cancelled,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertySaleOrderRevisionKindState {
    Listing,
    Cancellation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertySaleOrderRejectionReasonState {
    MortgageNotPayable,
    InsufficientProceeds,
    PolicyUnsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatePropertySaleOrderCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub holding_id: ResourceId,
    pub asking_price_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepricePropertySaleOrderCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub order_id: ResourceId,
    pub asking_price_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelPropertySaleOrderCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub order_id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertySaleOrderListingReceipt {
    pub command_id: CommandId,
    pub order_id: ResourceId,
    pub holding_id: ResourceId,
    pub revision_no: u32,
    pub asking_price_krw: i64,
    pub reference_value_krw: i64,
    pub asking_to_reference_ppm: i64,
    pub candidate_game_day: u32,
    pub status: PropertySaleOrderStatusState,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertySaleOrderCancellationReceipt {
    pub command_id: CommandId,
    pub order_id: ResourceId,
    pub holding_id: ResourceId,
    pub revision_no: u32,
    pub cancelled_game_day: u32,
    pub status: PropertySaleOrderStatusState,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertySaleExecutionState {
    pub filled_game_day: u32,
    pub gross_sale_price_krw: i64,
    pub transaction_cost_krw: i64,
    pub mortgage_principal_krw: i64,
    pub mortgage_fee_krw: i64,
    pub capital_gains_tax_krw: i64,
    pub wallet_proceeds_krw: i64,
    pub realized_gain_loss_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertySaleOrderSummaryState {
    pub order_id: ResourceId,
    pub holding_id: ResourceId,
    pub revision_no: u32,
    pub revision_kind: PropertySaleOrderRevisionKindState,
    pub asking_price_krw: Option<i64>,
    pub reference_value_krw: Option<i64>,
    pub asking_to_reference_ppm: Option<i64>,
    pub candidate_game_day: Option<u32>,
    pub status: PropertySaleOrderStatusState,
    pub cancelled_game_day: Option<u32>,
    pub rejection_reason: Option<PropertySaleOrderRejectionReasonState>,
    pub execution: Option<PropertySaleExecutionState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PropertySaleOrderPageQuery {
    pub before: Option<ResourceId>,
    pub limit: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertySaleOrderPageState {
    pub items: Vec<PropertySaleOrderSummaryState>,
    pub next_before: Option<ResourceId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertyTaxEventKindState {
    Acquisition,
    AnnualHolding,
    CapitalGains,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertyTaxEventStatusState {
    Scheduled,
    PartiallyPaid,
    Paid,
    NoPaymentRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertyTaxPaymentStatusState {
    Pending,
    Applied,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertyTaxComponentState {
    pub component_key: String,
    pub component_order: u8,
    pub tax_base_krw: i64,
    pub deduction_krw: i64,
    pub taxable_amount_krw: i64,
    pub rate_ppm: i64,
    pub progressive_deduction_krw: i64,
    pub amount_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertyTaxPaymentState {
    pub payment_no: u8,
    pub due_game_day: u32,
    pub paid_game_day: Option<u32>,
    pub status: PropertyTaxPaymentStatusState,
    pub amount_krw: i64,
    pub wallet_paid_krw: i64,
    pub tax_obligation_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertyTaxEventState {
    pub id: ResourceId,
    pub holding_id: ResourceId,
    pub policy_set_id: ResourceId,
    pub policy_key: String,
    pub rule_id: ResourceId,
    pub rule_key: String,
    pub legal_basis_date: String,
    pub kind: PropertyTaxEventKindState,
    pub status: PropertyTaxEventStatusState,
    pub tax_year: Option<i32>,
    pub assessed_game_day: u32,
    pub taxable_game_day: u32,
    pub paid_game_day: Option<u32>,
    pub household_home_count: u8,
    pub gross_amount_krw: i64,
    pub valuation_game_day: Option<u32>,
    pub valuation_price_index_ppm: Option<i64>,
    pub official_value_krw: Option<i64>,
    pub tax_base_krw: i64,
    pub deduction_krw: i64,
    pub taxable_amount_krw: i64,
    pub total_tax_krw: i64,
    pub paid_tax_krw: i64,
    pub components: Vec<PropertyTaxComponentState>,
    pub payments: Vec<PropertyTaxPaymentState>,
    pub exclusion_codes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PropertyTaxEventPageQuery {
    pub before: Option<ResourceId>,
    pub limit: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertyTaxEventPageState {
    pub holding_id: ResourceId,
    pub items: Vec<PropertyTaxEventState>,
    pub next_before: Option<ResourceId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HousingLeaseMoveReceipt {
    pub command_id: CommandId,
    pub lease_id: ResourceId,
    pub residence_id: ResourceId,
    pub listing_id: ResourceId,
    pub offer_kind: HousingLeaseOfferKind,
    pub region_key: LifeRegionKey,
    pub property_type: PropertyType,
    pub exclusive_area_square_meters: u16,
    pub deposit_krw: i64,
    pub monthly_rent_krw: Option<i64>,
    pub returned_deposit_krw: i64,
    pub moving_cost_krw: i64,
    pub wallet_delta_krw: i64,
    pub effective_from_game_day: u32,
    pub ended_lease_id: Option<ResourceId>,
    pub renewal_rule: HousingLeaseRenewalRuleState,
    #[serde(default)]
    pub deposit_loan_execution: Option<DepositLoanExecutionReceipt>,
    #[serde(default)]
    pub repaid_deposit_loan: Option<RepaidDepositLoanReceipt>,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifeStoreResult<T> {
    Applied { receipt: T, save: Box<SaveState> },
    Rejected(LifeFailureCode),
}

/// The durable state of one save, mirroring what the database holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveState {
    pub save_id: u64,
    /// The immutable market path assigned to this save.
    pub market_world_id: u64,
    /// The immutable Korean policy rules assigned to this run.
    pub policy_set: PolicySet,
    /// Increments whenever a new character replaces the current run.
    pub run_revision: u32,
    /// Increments for every committed player-state mutation within a run.
    pub state_revision: u64,
    pub game_day: u32,
    pub cash_krw: i64,
    pub debt_krw: i64,
    pub property_book_value_krw: i64,
    pub accounts: Vec<FinancialAccount>,
    pub positions: Vec<PositionState>,
    /// The first due items are enough for the bounded snapshot summary.
    pub pending_settlements: Vec<ScheduledSettlement>,
    pub cma_accounts: Vec<CmaAccountContractState>,
    pub cash_contracts: Vec<CashProductContractState>,
    pub deposit_protection: Vec<DepositProtectionState>,
    pub current_financial_income_year: FinancialIncomeYear,
    pub current_annual_tax_year: AnnualTaxYearState,
    pub latest_financial_income_assessment: Option<AnnualTaxYearState>,
    pub m2d_assets: M2dAssetSnapshot,
    pub isa_accounts: Vec<IsaAccountState>,
    pub pension_accounts: Vec<PensionAccountState>,
    pub career: CareerSnapshotState,
    pub life: LifeSnapshotState,
    /// `None` until a character has been created.
    pub character: Option<Character>,
}

impl SaveState {
    pub fn active_product_principal_krw(&self) -> Result<i64> {
        self.cash_contracts
            .iter()
            .try_fold(0_i64, |total, contract| {
                total.checked_add(contract.current_principal_krw)
            })
            .ok_or_else(|| anyhow::anyhow!("active cash-product principal overflowed"))
    }
}

/// Optimistic cursor checked again when a daily player transaction commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveCursor {
    pub market_world_id: u64,
    pub policy_set_id: u64,
    pub run_revision: u32,
    pub state_revision: u64,
    pub game_day: u32,
}

/// The public three-part cursor carried by durable state-changing commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameCommandCursor {
    pub run_revision: u32,
    pub state_revision: u64,
    pub game_day: u32,
}

impl From<CommandCursor> for GameCommandCursor {
    fn from(cursor: CommandCursor) -> Self {
        Self {
            run_revision: cursor.expected_run_revision,
            state_revision: cursor.expected_state_revision,
            game_day: cursor.expected_game_day,
        }
    }
}

impl From<&SaveState> for GameCommandCursor {
    fn from(state: &SaveState) -> Self {
        Self {
            run_revision: state.run_revision,
            state_revision: state.state_revision,
            game_day: state.game_day,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartingLoanCommand {
    pub product_version_id: ResourceId,
    pub product_kind: LoanProductKind,
    pub principal_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartGameManifestKind {
    LegacySandbox,
    Sandbox,
    Ranked(Box<RankedRunContext>),
}

impl StartGameManifestKind {
    pub const fn run_mode(&self) -> RunMode {
        match self {
            Self::LegacySandbox | Self::Sandbox => RunMode::Sandbox,
            Self::Ranked(context) => context.mode,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartGameCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    /// Semantic validation happens only after the durable identity has been inspected.
    pub draft: CharacterDraft,
    /// `None` preserves the v1 amount-only fingerprint and legacy catalog mapping.
    pub starting_loans: Option<Vec<StartingLoanCommand>>,
    pub manifest_kind: StartGameManifestKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualAdvanceCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub days: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartGameReceipt {
    pub command_id: CommandId,
    pub committed_cursor: GameCommandCursor,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdvanceCommandReceipt {
    pub command_id: CommandId,
    pub requested_days: u32,
    #[serde(default)]
    pub committed_days: u32,
    #[serde(default)]
    pub truncated_days: u32,
    pub initial_cursor: GameCommandCursor,
    pub committed_cursor: GameCommandCursor,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameCommandRejection {
    InvalidCommand,
    InvalidCharacter(Vec<ValidationError>),
    IdempotencyConflict,
    Busy,
    CharacterRequired,
    ModeUnavailable,
}

/// Versioned pointer used to prepare a new run without an active-world ABA race.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveMarketWorld {
    pub world_id: u64,
    pub assignment_revision: u64,
}

/// Immutable career content selected for a newly started run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CareerCatalogAssignment {
    pub bundle_id: ResourceId,
    pub assignment_revision: u64,
}

/// Immutable employment policy selected for a newly started run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmploymentPolicyAssignment {
    pub policy_set_id: ResourceId,
    pub assignment_revision: u64,
}

/// Immutable M4 catalog and model versions selected for a newly started run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunRuleBundleAssignment {
    pub life_catalog_set_id: ResourceId,
    pub credit_model_version_id: ResourceId,
    pub real_estate_model_version_id: ResourceId,
    pub assignment_revision: u64,
}

/// Immutable M5 content publication selected for a newly started run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentBundleAssignment {
    pub bundle_id: ResourceId,
    pub assignment_revision: u64,
}

/// Immutable offline policy selected for a newly started sandbox run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OfflinePolicyAssignment {
    pub policy_version_id: ResourceId,
    pub assignment_revision: u64,
}

/// Immutable business catalog selected for a newly started sandbox run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusinessCatalogAssignment {
    pub catalog_version_id: ResourceId,
    pub assignment_revision: u64,
}

impl From<&SaveState> for SaveCursor {
    fn from(state: &SaveState) -> Self {
        Self {
            market_world_id: state.market_world_id,
            policy_set_id: state.policy_set.id.get(),
            run_revision: state.run_revision,
            state_revision: state.state_revision,
            game_day: state.game_day,
        }
    }
}

/// Both versioned assignments that a new run pins atomically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveRunConfiguration {
    pub market_world: ActiveMarketWorld,
    pub policy_set: PolicySetAssignment,
    pub product_bundle_id: Option<ResourceId>,
    pub career_catalog: CareerCatalogAssignment,
    pub employment_policy: EmploymentPolicyAssignment,
    pub rule_bundle: RunRuleBundleAssignment,
    pub content_bundle: ContentBundleAssignment,
    pub offline_policy: OfflinePolicyAssignment,
    pub business_catalog: BusinessCatalogAssignment,
}

/// Result of one committed daily pipeline attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvanceDayResult {
    Advanced(SaveState),
    /// A save exists, but its character has not been created yet.
    CharacterRequired,
    /// A ranked run already reached the immutable target day.
    TargetReached(SaveState),
    /// The DB-time lease was lost or an online intent requires the worker to yield.
    ProgressBusy(SaveState),
    /// Another process changed the save after the market target was selected.
    Stale(SaveState),
}

/// Result of committing a prepared new run against the active-world pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartGameResult {
    Applied {
        save: Box<SaveState>,
        receipt: StartGameReceipt,
    },
    Replayed {
        save: Box<SaveState>,
        receipt: StartGameReceipt,
    },
    Rejected(GameCommandRejection),
    /// The pointer changed after its market day 0 was prepared; prepare again.
    ActiveWorldChanged,
}

/// Result of one durable manual-command step. Automatic clock steps use
/// `AdvanceDayResult` and never create command rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvanceCommandStepResult {
    Advanced {
        save: Box<SaveState>,
        receipt: Option<AdvanceCommandReceipt>,
    },
    Replayed {
        save: Box<SaveState>,
        receipt: AdvanceCommandReceipt,
    },
    Rejected(GameCommandRejection),
    ProgressBusy(Box<SaveState>),
    /// The same command advanced after its market target was prepared. Retry from DB.
    Stale(Box<SaveState>),
}

/// Outcome of an atomic order attempt. Replays are returned as `Executed` with the
/// execution's `replayed` flag set and do not increment the state revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TradeStoreResult {
    Executed {
        execution: TradeExecution,
        save: Box<SaveState>,
    },
    Rejected(TradeFailure),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinanceStoreResult {
    Transferred(TransferReceipt),
    Rejected(FinanceFailureCode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CashProductStoreResult<T> {
    Applied { receipt: T, save: Box<SaveState> },
    Rejected(FinanceFailureCode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaxAccountStoreResult<T> {
    Applied { receipt: T, save: Box<SaveState> },
    Rejected(FinanceFailureCode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenTaxAccountCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub account_type: crate::finance::FinancialAccountType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseIsaAccountCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub account_id: crate::finance::ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartPensionCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub account_id: crate::finance::ResourceId,
    pub payment_years: u16,
    pub lifetime: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PensionWithdrawalCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub account_id: crate::finance::ResourceId,
    pub amount_krw: i64,
    pub kind: PensionWithdrawalRequestKind,
    pub reason: Option<IrpWithdrawalReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenTaxAccountReceipt {
    pub command_id: CommandId,
    pub account_id: crate::finance::ResourceId,
    #[serde(rename = "type")]
    pub account_type: crate::finance::FinancialAccountType,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloseIsaAccountReceipt {
    pub command_id: CommandId,
    pub account_id: crate::finance::ResourceId,
    pub gross_tax_profit_krw: i64,
    pub deductible_loss_krw: i64,
    pub income_tax_krw: i64,
    pub local_income_tax_krw: i64,
    pub net_payout_krw: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartPensionReceipt {
    pub command_id: CommandId,
    pub account_id: crate::finance::ResourceId,
    pub start_tax_year: u16,
    pub payment_years: u16,
    pub lifetime: bool,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PensionWithdrawalReceipt {
    pub command_id: CommandId,
    pub account_id: crate::finance::ResourceId,
    pub gross_amount_krw: i64,
    pub pension_amount_krw: i64,
    pub non_pension_amount_krw: i64,
    pub tax_free_amount_krw: i64,
    pub tax_krw: i64,
    pub net_payout_krw: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsaAccountState {
    pub account_id: crate::finance::ResourceId,
    pub account_type: crate::finance::FinancialAccountType,
    pub opened_game_day: u32,
    pub minimum_term_game_day: u32,
    pub total_contribution_krw: i64,
    pub principal_withdrawal_krw: i64,
    pub contribution_capacity_krw: i64,
    pub tax_profit_krw: i64,
    pub deductible_loss_krw: i64,
    pub expected_close_income_tax_krw: i64,
    pub expected_close_local_income_tax_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PensionAccountState {
    pub account_id: crate::finance::ResourceId,
    pub account_type: crate::finance::FinancialAccountType,
    pub opened_game_day: u32,
    pub eligible_pension_start_game_day: u32,
    pub pension_started: bool,
    pub tax_layers: PensionTaxLayers,
    pub current_year_contribution_krw: i64,
    pub current_year_credit_eligible_krw: i64,
    pub expected_credit_krw: i64,
    pub current_year_pension_limit_krw: Option<i64>,
    pub current_year_pension_withdrawn_krw: i64,
    pub risk_asset_value_krw: i64,
    pub total_value_krw: i64,
    pub risk_asset_ratio_ppm: i64,
}

/// One immutable market world plus the parameters that generate its path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketWorldState {
    pub id: u64,
    pub world: MarketWorld,
    pub calibration: MarketCalibration,
}

/// An authenticated save's visible market window. It never extends past its cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketHistoryState {
    pub world_key: String,
    pub through_game_day: u32,
    pub days: Vec<MarketDay>,
}

/// A signed-in account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountUser {
    pub id: u64,
    pub provider: ProviderKind,
    pub email: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfflinePolicyState {
    pub id: ResourceId,
    pub canonical_sha256: String,
    pub engine_version: String,
    pub cadence_seconds: u32,
    pub absence_window_cap_days: u32,
    pub max_worker_batch_days: u16,
    pub lease_seconds: u16,
    pub presence_ttl_seconds: u16,
    pub heartbeat_seconds: u16,
    pub online_intent_ttl_seconds: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfflineProgressSettingStatus {
    Active,
    PausedBySystem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressLeaseState {
    pub holder_kind: ProgressHolderKind,
    pub generation: u64,
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfflineProgressState {
    pub run_revision: u32,
    pub policy: Option<OfflinePolicyState>,
    pub enabled: bool,
    pub setting_status: OfflineProgressSettingStatus,
    pub absence_started_at: Option<String>,
    pub accrued_through: Option<String>,
    pub accrual_limit_at: Option<String>,
    pub window_accrued_days: u32,
    pub pending_days: u32,
    pub processed_days: u64,
    pub cancelled_pending_days: u64,
    pub revision: u64,
    pub last_error_code: Option<String>,
    pub online: bool,
    pub lease: Option<ProgressLeaseState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfflineProgressFailure {
    CharacterRequired,
    PolicyUnavailable,
    RevisionConflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OfflineProgressUpdateResult {
    Updated(Box<OfflineProgressState>),
    Rejected(OfflineProgressFailure),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressHolderKind {
    Online,
    Worker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressLeaseGuard {
    pub save_id: u64,
    pub run_revision: u32,
    pub holder_kind: ProgressHolderKind,
    pub holder_token_sha256: String,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfflineAttemptIdentity {
    pub attempt_key: String,
    pub retry_no: u16,
    pub engine_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressStepContext {
    pub lease: ProgressLeaseGuard,
    pub offline_attempt: Option<OfflineAttemptIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressLeaseAcquireResult {
    Acquired(ProgressLeaseGuard),
    Busy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnlinePresenceRegistration {
    pub heartbeat_seconds: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfflineWorkClaim {
    pub user_id: u64,
    pub save_id: u64,
    pub run_revision: u32,
    pub next_game_day: u32,
    pub max_batch_days: u16,
    pub retry_no: u16,
    pub lease: ProgressLeaseGuard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfflineAttemptEventKind {
    Started,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfflineAttemptEvent<'a> {
    pub attempt_key: &'a str,
    pub event_kind: OfflineAttemptEventKind,
    pub save_id: u64,
    pub run_revision: u32,
    pub game_day: u32,
    pub lease_generation: u64,
    pub retry_no: u16,
    pub engine_version: &'a str,
    pub error_code: Option<&'a str>,
}

/// Accounts and sessions.
#[async_trait]
pub trait UserStore: Send + Sync + 'static {
    /// Records an OAuth-verified user: creates the account, or refreshes it if known.
    async fn upsert(&self, identity: &OAuthIdentity) -> Result<AccountUser>;

    /// Opens a session, storing only the token hash.
    async fn open_session(&self, user_id: u64, token_hash: &str, ttl: Duration) -> Result<()>;

    /// Finds a user by token hash. An expired session counts as absent.
    async fn find_by_session(&self, token_hash: &str) -> Result<Option<AccountUser>>;

    /// Closes one session (logout).
    async fn close_session(&self, token_hash: &str) -> Result<()>;

    /// Permanently deletes one account and all rows owned through database cascades.
    async fn delete_account(&self, user_id: u64) -> Result<bool>;
}

#[async_trait]
pub trait OfflineProgressStore: Send + Sync + 'static {
    async fn status(&self, user_id: u64) -> Result<OfflineProgressState>;

    async fn set_enabled(
        &self,
        user_id: u64,
        expected_revision: u64,
        enabled: bool,
    ) -> Result<OfflineProgressUpdateResult>;

    async fn register_online_presence(
        &self,
        user_id: u64,
        connection_token_sha256: &str,
    ) -> Result<Option<OnlinePresenceRegistration>>;

    async fn heartbeat_online_presence(
        &self,
        user_id: u64,
        connection_token_sha256: &str,
    ) -> Result<()>;

    async fn close_online_presence(&self, connection_token_sha256: &str) -> Result<()>;

    async fn acquire_online_lease(
        &self,
        user_id: u64,
        holder_token_sha256: &str,
    ) -> Result<ProgressLeaseAcquireResult>;

    async fn release_lease(&self, lease: &ProgressLeaseGuard) -> Result<()>;

    async fn claim_offline_work(
        &self,
        holder_token_sha256: &str,
        engine_version: &str,
    ) -> Result<Option<OfflineWorkClaim>>;

    async fn record_attempt(&self, event: OfflineAttemptEvent<'_>) -> Result<()>;

    async fn pause_after_permanent_failure(
        &self,
        lease: &ProgressLeaseGuard,
        error_code: &str,
    ) -> Result<bool>;
}

#[async_trait]
pub trait RunStore: Send + Sync + 'static {
    async fn run_options(&self) -> Result<RunOptions>;

    async fn season_leagues(&self, season_id: ResourceId) -> Result<Option<SeasonLeagues>>;

    async fn league_rankings(
        &self,
        league_id: ResourceId,
        cursor: Option<RankingPageCursor>,
        limit: u32,
    ) -> Result<Option<LeagueRankingPage>>;

    async fn run_finalization(
        &self,
        user_id: u64,
        run_revision: u32,
    ) -> Result<Option<RunFinalization>>;

    async fn prepare_ranked_preset(
        &self,
        preset_version_id: ResourceId,
    ) -> Result<Option<RankedRunPreparation>>;

    async fn prepare_ranked_custom(
        &self,
        budget_version_id: ResourceId,
        selections: &[PointSelection],
    ) -> Result<Option<RankedRunPreparation>>;

    async fn preview_point_budget(
        &self,
        version_id: ResourceId,
        selections: &[PointSelection],
    ) -> Result<Option<PointBudgetEvaluation>>;

    async fn run_manifest(
        &self,
        user_id: u64,
        run_revision: u32,
    ) -> Result<Option<RunManifestSummary>>;
}

/// Save reads and writes. Every access is scoped to an account (§4.5).
#[async_trait]
pub trait SaveStore: Send + Sync + 'static {
    /// Reads the account's save, creating it in its initial state if absent.
    async fn load(&self, user_id: u64) -> Result<SaveState>;

    /// Reads both versioned pointers used only for new runs.
    async fn active_run_configuration(&self) -> Result<ActiveRunConfiguration>;

    /// Commits a character only if the prepared active-world pointer is still current.
    async fn start_game(
        &self,
        user_id: u64,
        command: &StartGameCommand,
        expected: ActiveRunConfiguration,
    ) -> Result<StartGameResult>;

    /// Commits exactly one game day. Multi-day commands repeat this call (§4.2).
    async fn advance_one_day(
        &self,
        user_id: u64,
        progress: &ProgressStepContext,
        expected: SaveCursor,
        market: &MarketDay,
    ) -> Result<AdvanceDayResult>;

    /// Commits the next missing day of one durable manual command.
    async fn advance_command_step(
        &self,
        user_id: u64,
        progress: &ProgressStepContext,
        command: &ManualAdvanceCommand,
        market: &MarketDay,
    ) -> Result<AdvanceCommandStepResult>;
}

/// Atomic LLX execution against an account-owned save and its current market close.
#[async_trait]
pub trait TradingStore: Send + Sync + 'static {
    async fn execute(&self, user_id: u64, order: &TradeOrder) -> Result<TradeStoreResult>;
}

#[async_trait]
pub trait FinanceStore: Send + Sync + 'static {
    async fn transfer(&self, user_id: u64, command: &TransferCommand)
    -> Result<FinanceStoreResult>;

    async fn ledger_page(
        &self,
        user_id: u64,
        before: Option<u64>,
        limit: u32,
    ) -> Result<LedgerPage>;
}

/// M2-B cash-product catalog, commands, and tax-year reads.
#[async_trait]
pub trait CashProductStore: Send + Sync + 'static {
    async fn cash_product_catalog(&self) -> Result<CashProductCatalog>;

    async fn open_cma_account(
        &self,
        user_id: u64,
        command: &OpenCmaAccountCommand,
    ) -> Result<CashProductStoreResult<OpenCmaAccountReceipt>>;

    async fn close_cma_account(
        &self,
        user_id: u64,
        command: &CloseCmaAccountCommand,
    ) -> Result<CashProductStoreResult<CloseCmaAccountReceipt>>;

    async fn open_cash_product(
        &self,
        user_id: u64,
        command: &OpenCashProductCommand,
    ) -> Result<CashProductStoreResult<OpenCashProductReceipt>>;

    async fn close_cash_product(
        &self,
        user_id: u64,
        command: &CloseCashProductCommand,
    ) -> Result<CashProductStoreResult<CloseCashProductReceipt>>;

    async fn financial_income_year(
        &self,
        user_id: u64,
        tax_year: u16,
    ) -> Result<AnnualTaxYearState>;
}

/// M2-D market-valued asset catalogs and atomic commands.
#[async_trait]
pub trait M2dAssetStore: Send + Sync + 'static {
    async fn bond_catalog(&self, user_id: u64) -> Result<BondCatalog>;

    async fn place_bond_order(
        &self,
        user_id: u64,
        command: &BondOrderCommand,
    ) -> Result<M2dAssetCommandResult<BondOrderResponse>>;

    async fn gold_catalog(&self, user_id: u64) -> Result<GoldCatalog>;

    async fn open_gold_account(
        &self,
        user_id: u64,
        command: &OpenGoldAccountCommand,
    ) -> Result<M2dAssetCommandResult<OpenGoldAccountResponse>>;

    async fn place_gold_order(
        &self,
        user_id: u64,
        command: &GoldOrderCommand,
    ) -> Result<M2dAssetCommandResult<GoldOrderResponse>>;

    async fn withdraw_gold(
        &self,
        user_id: u64,
        command: &GoldWithdrawalCommand,
    ) -> Result<M2dAssetCommandResult<GoldWithdrawalResponse>>;
}

/// M2-C tax-account commands. Every mutation is save-owned and commits its
/// receipt, event, ledger (when money moves), summaries, and cursor atomically.
#[async_trait]
pub trait TaxAccountStore: Send + Sync + 'static {
    async fn open_tax_account(
        &self,
        user_id: u64,
        command: &OpenTaxAccountCommand,
    ) -> Result<TaxAccountStoreResult<OpenTaxAccountReceipt>>;

    async fn close_isa_account(
        &self,
        user_id: u64,
        command: &CloseIsaAccountCommand,
    ) -> Result<TaxAccountStoreResult<CloseIsaAccountReceipt>>;

    async fn start_pension(
        &self,
        user_id: u64,
        command: &StartPensionCommand,
    ) -> Result<TaxAccountStoreResult<StartPensionReceipt>>;

    async fn withdraw_pension(
        &self,
        user_id: u64,
        command: &PensionWithdrawalCommand,
    ) -> Result<TaxAccountStoreResult<PensionWithdrawalReceipt>>;
}

/// M3 career catalog, recruitment, immutable artifacts, and cursor-protected commands.
#[async_trait]
pub trait CareerStore: Send + Sync + 'static {
    async fn specs(&self, user_id: u64, query: CareerPageQuery) -> Result<CareerSpecsState>;

    async fn activities(
        &self,
        user_id: u64,
        query: CareerPageQuery,
    ) -> Result<CareerActivitiesState>;

    async fn artifacts(
        &self,
        user_id: u64,
        query: CareerArtifactPageQuery,
    ) -> Result<CareerArtifactPageState>;

    async fn jobs(
        &self,
        _user_id: u64,
        _query: CareerJobsPageQuery,
    ) -> Result<CareerJobsPageState> {
        Err(anyhow::anyhow!("M3-B recruitment jobs are not wired"))
    }

    async fn applications(
        &self,
        _user_id: u64,
        _query: CareerPageQuery,
    ) -> Result<CareerApplicationsPageState> {
        Err(anyhow::anyhow!(
            "M3-B recruitment applications are not wired"
        ))
    }

    async fn employment(&self, _user_id: u64) -> Result<CareerEmploymentState> {
        Err(anyhow::anyhow!("M3-B employment is not wired"))
    }

    async fn payroll(
        &self,
        _user_id: u64,
        _query: CareerPageQuery,
    ) -> Result<CareerPayrollPageState> {
        Err(anyhow::anyhow!("M3-C payroll is not wired"))
    }

    async fn employment_tax_year(
        &self,
        _user_id: u64,
        _tax_year: u16,
    ) -> Result<CareerEmploymentTaxYearState> {
        Err(anyhow::anyhow!("M3-C employment tax years are not wired"))
    }

    async fn military_options(&self, _user_id: u64) -> Result<MilitaryOptionsState> {
        Err(anyhow::anyhow!("M3-D military options are not wired"))
    }

    async fn military_service(&self, _user_id: u64) -> Result<MilitaryServiceState> {
        Err(anyhow::anyhow!("M3-D military service is not wired"))
    }

    async fn military_savings_products(
        &self,
        _user_id: u64,
    ) -> Result<MilitarySavingsProductsState> {
        Err(anyhow::anyhow!(
            "M3-D military savings products are not wired"
        ))
    }

    async fn military_savings(
        &self,
        _user_id: u64,
        _query: CareerPageQuery,
    ) -> Result<MilitarySavingsPageState> {
        Err(anyhow::anyhow!("M3-D military savings are not wired"))
    }

    async fn focus(
        &self,
        user_id: u64,
        command: &FocusCareerCommand,
    ) -> Result<CareerStoreResult<FocusCareerReceipt>>;

    async fn start_activity(
        &self,
        user_id: u64,
        command: &StartCareerActivityCommand,
    ) -> Result<CareerStoreResult<CareerActivityReceipt>>;

    async fn cancel_activity(
        &self,
        user_id: u64,
        command: &CancelCareerActivityCommand,
    ) -> Result<CareerStoreResult<CareerActivityReceipt>>;

    async fn publish_artifact(
        &self,
        user_id: u64,
        command: &PublishCareerArtifactCommand,
    ) -> Result<CareerStoreResult<CareerArtifactReceipt>>;

    async fn apply(
        &self,
        _user_id: u64,
        _command: &ApplyCareerCommand,
    ) -> Result<CareerStoreResult<CareerApplicationReceipt>> {
        Err(anyhow::anyhow!(
            "M3-B recruitment applications are not wired"
        ))
    }

    async fn confirm_interview(
        &self,
        _user_id: u64,
        _command: &ConfirmCareerInterviewCommand,
    ) -> Result<CareerStoreResult<CareerApplicationReceipt>> {
        Err(anyhow::anyhow!("M3-B recruitment interviews are not wired"))
    }

    async fn withdraw_application(
        &self,
        _user_id: u64,
        _command: &WithdrawCareerApplicationCommand,
    ) -> Result<CareerStoreResult<CareerApplicationReceipt>> {
        Err(anyhow::anyhow!(
            "M3-B recruitment applications are not wired"
        ))
    }

    async fn accept_invitation(
        &self,
        _user_id: u64,
        _command: &AcceptCareerInvitationCommand,
    ) -> Result<CareerStoreResult<CareerInvitationReceipt>> {
        Err(anyhow::anyhow!(
            "M3-B recruitment invitations are not wired"
        ))
    }

    async fn decline_invitation(
        &self,
        _user_id: u64,
        _command: &DeclineCareerInvitationCommand,
    ) -> Result<CareerStoreResult<CareerInvitationReceipt>> {
        Err(anyhow::anyhow!(
            "M3-B recruitment invitations are not wired"
        ))
    }

    async fn accept_offer(
        &self,
        _user_id: u64,
        _command: &AcceptCareerOfferCommand,
    ) -> Result<CareerStoreResult<CareerOfferReceipt>> {
        Err(anyhow::anyhow!("M3-B recruitment offers are not wired"))
    }

    async fn decline_offer(
        &self,
        _user_id: u64,
        _command: &DeclineCareerOfferCommand,
    ) -> Result<CareerStoreResult<CareerOfferReceipt>> {
        Err(anyhow::anyhow!("M3-B recruitment offers are not wired"))
    }

    async fn start_military_service(
        &self,
        _user_id: u64,
        _command: &StartMilitaryServiceCommand,
    ) -> Result<CareerStoreResult<MilitaryServiceCommandReceipt>> {
        Err(anyhow::anyhow!("M3-D military service is not wired"))
    }

    async fn open_military_savings(
        &self,
        _user_id: u64,
        _command: &OpenMilitarySavingsCommand,
    ) -> Result<CareerStoreResult<MilitarySavingsCommandReceipt>> {
        Err(anyhow::anyhow!("M3-D military savings are not wired"))
    }

    async fn close_military_savings(
        &self,
        _user_id: u64,
        _command: &CloseMilitarySavingsCommand,
    ) -> Result<CareerStoreResult<MilitarySavingsCommandReceipt>> {
        Err(anyhow::anyhow!("M3-D military savings are not wired"))
    }
}

/// M4 household costs, credit, loans, and cursor-protected commands.
#[async_trait]
pub trait LifeStore: Send + Sync + 'static {
    async fn corporation_templates(
        &self,
        user_id: u64,
    ) -> Result<CorporationReadResult<CorporationTemplatesState>> {
        let _ = user_id;
        Err(anyhow::anyhow!("M4-E2 corporation is not wired"))
    }

    async fn create_corporation(
        &self,
        user_id: u64,
        command: &CreateCorporationCommand,
    ) -> Result<LifeStoreResult<CorporationReceipt>> {
        let _ = (user_id, command);
        Err(anyhow::anyhow!("M4-E2 corporation is not wired"))
    }

    async fn corporation_detail(
        &self,
        user_id: u64,
        corporation_id: ResourceId,
    ) -> Result<CorporationReadResult<CorporationSummaryState>> {
        let _ = (user_id, corporation_id);
        Err(anyhow::anyhow!("M4-E2 corporation is not wired"))
    }

    async fn update_corporation_settings(
        &self,
        user_id: u64,
        command: &UpdateCorporationSettingsCommand,
    ) -> Result<LifeStoreResult<CorporationSettingsReceipt>> {
        let _ = (user_id, command);
        Err(anyhow::anyhow!("M4-E2 corporation settings are not wired"))
    }

    async fn pay_corporation_dividend(
        &self,
        user_id: u64,
        command: &PayCorporationDividendCommand,
    ) -> Result<LifeStoreResult<CorporationDividendReceipt>> {
        let _ = (user_id, command);
        Err(anyhow::anyhow!("M4-E2 corporation dividend is not wired"))
    }

    async fn corporation_operating_months(
        &self,
        user_id: u64,
        corporation_id: ResourceId,
        cursor: Option<String>,
    ) -> Result<CorporationReadResult<CorporationOperatingMonthPageState>> {
        let _ = (user_id, corporation_id, cursor);
        Err(anyhow::anyhow!("M4-E2 corporation months are not wired"))
    }

    async fn corporation_operations(
        &self,
        user_id: u64,
        corporation_id: ResourceId,
    ) -> Result<CorporationReadResult<BusinessOperationsState>> {
        let _ = (user_id, corporation_id);
        Err(anyhow::anyhow!("M5-E corporation operations are not wired"))
    }

    async fn manage_corporation_operations(
        &self,
        user_id: u64,
        command: &ManageBusinessOperationsCommand,
    ) -> Result<LifeStoreResult<BusinessOperationReceipt>> {
        let _ = (user_id, command);
        Err(anyhow::anyhow!(
            "M5-E corporation operation commands are not wired"
        ))
    }

    async fn insolvency_overview(
        &self,
        user_id: u64,
    ) -> Result<InsolvencyReadResult<InsolvencySnapshotState>> {
        let _ = user_id;
        Err(anyhow::anyhow!("M4-E1 insolvency is not wired"))
    }

    async fn prepare_insolvency_case(
        &self,
        user_id: u64,
        command: &PrepareInsolvencyCaseCommand,
    ) -> Result<LifeStoreResult<InsolvencyCaseReceipt>> {
        let _ = (user_id, command);
        Err(anyhow::anyhow!("M4-E1 insolvency is not wired"))
    }

    async fn act_on_insolvency_case(
        &self,
        user_id: u64,
        command: &ActOnInsolvencyCaseCommand,
    ) -> Result<LifeStoreResult<InsolvencyCaseReceipt>> {
        let _ = (user_id, command);
        Err(anyhow::anyhow!("M4-E1 insolvency is not wired"))
    }

    async fn insolvency_case_detail(
        &self,
        user_id: u64,
        case_id: ResourceId,
    ) -> Result<InsolvencyReadResult<InsolvencyCaseDetailState>> {
        let _ = (user_id, case_id);
        Err(anyhow::anyhow!("M4-E1 insolvency is not wired"))
    }

    async fn insolvency_claims(
        &self,
        user_id: u64,
        case_id: ResourceId,
        cursor: Option<String>,
    ) -> Result<InsolvencyReadResult<InsolvencyClaimPageState>> {
        let _ = (user_id, case_id, cursor);
        Err(anyhow::anyhow!("M4-E1 insolvency is not wired"))
    }

    async fn insolvency_liquidations(
        &self,
        user_id: u64,
        case_id: ResourceId,
        cursor: Option<String>,
    ) -> Result<InsolvencyReadResult<InsolvencyLiquidationPageState>> {
        let _ = (user_id, case_id, cursor);
        Err(anyhow::anyhow!("M4-E1 insolvency is not wired"))
    }

    async fn life_events(
        &self,
        user_id: u64,
        query: LifeEventsQueryState,
    ) -> Result<LifeEventsReadResult>;

    async fn resolve_life_event(
        &self,
        user_id: u64,
        command: &ResolveLifeEventCommand,
    ) -> Result<LifeStoreResult<LifeEventChoiceReceipt>>;

    async fn insurance(
        &self,
        user_id: u64,
        query: InsuranceQueryState,
    ) -> Result<InsuranceReadResult>;

    async fn enroll_insurance_contract(
        &self,
        user_id: u64,
        command: &EnrollInsuranceContractCommand,
    ) -> Result<LifeStoreResult<InsuranceEnrollmentReceipt>>;

    async fn cancel_insurance_contract(
        &self,
        user_id: u64,
        command: &CancelInsuranceContractCommand,
    ) -> Result<LifeStoreResult<InsuranceCancellationReceipt>>;

    async fn file_insurance_claim(
        &self,
        user_id: u64,
        command: &FileInsuranceClaimCommand,
    ) -> Result<LifeStoreResult<InsuranceClaimReceipt>>;

    async fn welfare_programs(&self, user_id: u64) -> Result<Option<WelfareProgramsState>>;

    async fn apply_welfare_program(
        &self,
        user_id: u64,
        command: &ApplyWelfareProgramCommand,
    ) -> Result<LifeStoreResult<WelfareApplicationReceipt>>;

    async fn housing_listings(
        &self,
        user_id: u64,
        query: HousingListingsQueryState,
    ) -> Result<Option<HousingListingsState>>;

    async fn housing_lease_current(&self, user_id: u64)
    -> Result<Option<HousingLeaseCurrentState>>;

    async fn start_housing_lease(
        &self,
        user_id: u64,
        command: &StartHousingLeaseCommand,
    ) -> Result<LifeStoreResult<HousingLeaseMoveReceipt>>;

    async fn housing_property_holdings(
        &self,
        user_id: u64,
    ) -> Result<Option<HousingPropertyHoldingsState>>;

    async fn quote_mortgage(
        &self,
        user_id: u64,
        command: &CreateMortgageQuoteCommand,
    ) -> Result<LifeStoreResult<MortgageQuoteReceipt>>;

    async fn purchase_property(
        &self,
        user_id: u64,
        command: &PurchasePropertyCommand,
    ) -> Result<LifeStoreResult<PropertyPurchaseReceipt>>;

    async fn create_property_sale_order(
        &self,
        user_id: u64,
        command: &CreatePropertySaleOrderCommand,
    ) -> Result<LifeStoreResult<PropertySaleOrderListingReceipt>>;

    async fn reprice_property_sale_order(
        &self,
        user_id: u64,
        command: &RepricePropertySaleOrderCommand,
    ) -> Result<LifeStoreResult<PropertySaleOrderListingReceipt>>;

    async fn cancel_property_sale_order(
        &self,
        user_id: u64,
        command: &CancelPropertySaleOrderCommand,
    ) -> Result<LifeStoreResult<PropertySaleOrderCancellationReceipt>>;

    async fn property_sale_orders(
        &self,
        user_id: u64,
        query: PropertySaleOrderPageQuery,
    ) -> Result<Option<PropertySaleOrderPageState>>;

    async fn property_tax_events(
        &self,
        user_id: u64,
        holding_id: ResourceId,
        query: PropertyTaxEventPageQuery,
    ) -> Result<Option<PropertyTaxEventPageState>>;

    async fn loan_products(&self, user_id: u64) -> Result<LoanProductCatalogState>;

    async fn loan_detail(
        &self,
        user_id: u64,
        loan_id: ResourceId,
    ) -> Result<Option<LoanDetailState>>;

    async fn loan_installments(
        &self,
        user_id: u64,
        loan_id: ResourceId,
        query: LoanInstallmentPageQuery,
    ) -> Result<Option<LoanInstallmentPageState>>;

    async fn credit(&self, user_id: u64) -> Result<CreditOverviewState>;

    async fn quote_loan(
        &self,
        user_id: u64,
        command: &CreateLoanQuoteCommand,
    ) -> Result<LifeStoreResult<LoanQuoteReceipt>>;

    async fn quote_lease_deposit_loan(
        &self,
        user_id: u64,
        command: &CreateLeaseDepositLoanQuoteCommand,
    ) -> Result<LifeStoreResult<LeaseDepositLoanQuoteReceipt>>;

    async fn execute_loan(
        &self,
        user_id: u64,
        command: &ExecuteLoanCommand,
    ) -> Result<LifeStoreResult<LoanExecutionReceipt>>;

    async fn prepay_loan(
        &self,
        user_id: u64,
        command: &PrepayLoanCommand,
    ) -> Result<LifeStoreResult<LoanPrepaymentReceipt>>;

    async fn budget(&self, user_id: u64) -> Result<LifeBudgetState>;

    async fn update_budget(
        &self,
        user_id: u64,
        command: &UpdateLifeBudgetCommand,
    ) -> Result<LifeStoreResult<UpdateLifeBudgetReceipt>>;

    async fn pay_essential_arrear(
        &self,
        user_id: u64,
        command: &PayEssentialArrearCommand,
    ) -> Result<LifeStoreResult<EssentialArrearPaymentReceipt>>;

    async fn pay_lease_arrear(
        &self,
        user_id: u64,
        command: &PayLeaseArrearCommand,
    ) -> Result<LifeStoreResult<LeaseArrearPaymentReceipt>>;
}

/// Prepares shared immutable recruitment postings before a player transaction (§6.1).
#[async_trait]
pub trait RecruitmentPostingStore: Send + Sync + 'static {
    async fn ensure_postings_for_user(&self, user_id: u64, target_game_day: u32) -> Result<()>;
}

/// Prepares the held property's shared immutable price row before a player-day transaction.
#[async_trait]
pub trait RealEstateDailyPreparationStore: Send + Sync + 'static {
    async fn ensure_property_market_for_user(
        &self,
        user_id: u64,
        target_game_day: u32,
    ) -> Result<()>;
}

/// Shared immutable market paths and their generated daily cache.
#[async_trait]
pub trait MarketStore: Send + Sync + 'static {
    async fn load_world(&self, world_id: u64) -> Result<MarketWorldState>;

    async fn ensure_day(&self, world_id: u64, target_game_day: u32) -> Result<MarketDay>;

    /// Reads only the authenticated account's assigned world through its current game day.
    async fn history_for_user(&self, user_id: u64, limit: u32) -> Result<MarketHistoryState>;
}
