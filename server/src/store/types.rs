//! Store contracts. The MySQL implementation does not know this file, and callers do
//! not know the implementation.

use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::auth::{OAuthIdentity, ProviderKind};
use crate::career::{
    ActivityStatus, ArtifactDraft, ArtifactKind, CareerFailureCode, DimensionScores, EvidenceKind,
    Industry, LifeStatus,
};
use crate::character::{Character, CharacterDraft, ValidationError};
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
use crate::market::{MarketCalibration, MarketDay, MarketWorld};
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
    pub region: String,
    pub employment_type: String,
    pub required_scores: DimensionScores,
    pub possessed_scores: DimensionScores,
    pub minimum_annual_salary_krw: i64,
    pub maximum_annual_salary_krw: i64,
    pub salary_step_krw: i64,
    pub competition_band: CareerCompetitionBand,
    pub military_requirement: CareerMilitaryRequirement,
    pub minimum_education: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CareerStoreResult<T> {
    Applied { receipt: T, save: Box<SaveState> },
    Rejected(CareerFailureCode),
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
pub struct StartGameCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    /// Semantic validation happens only after the durable identity has been inspected.
    pub draft: CharacterDraft,
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
}

/// Result of one committed daily pipeline attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvanceDayResult {
    Advanced(SaveState),
    /// A save exists, but its character has not been created yet.
    CharacterRequired,
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
        expected: SaveCursor,
        market: &MarketDay,
    ) -> Result<AdvanceDayResult>;

    /// Commits the next missing day of one durable manual command.
    async fn advance_command_step(
        &self,
        user_id: u64,
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
}

/// Prepares shared immutable recruitment postings before a player transaction (§6.1).
#[async_trait]
pub trait RecruitmentPostingStore: Send + Sync + 'static {
    async fn ensure_postings_for_user(&self, user_id: u64, target_game_day: u32) -> Result<()>;
}

/// Shared immutable market paths and their generated daily cache.
#[async_trait]
pub trait MarketStore: Send + Sync + 'static {
    async fn load_world(&self, world_id: u64) -> Result<MarketWorldState>;

    async fn ensure_day(&self, world_id: u64, target_game_day: u32) -> Result<MarketDay>;

    /// Reads only the authenticated account's assigned world through its current game day.
    async fn history_for_user(&self, user_id: u64, limit: u32) -> Result<MarketHistoryState>;
}
