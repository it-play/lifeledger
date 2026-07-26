use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex, MutexGuard, Weak};
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
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
use crate::market::{InterestRateState, MarketRegime};
use crate::store::{
    AcceptCareerInvitationCommand, AcceptCareerOfferCommand, AccountUser, AdvanceCommandReceipt,
    AnnualTaxAssessmentState, AnnualTaxCalculatedState, AnnualTaxYearState, ApplyCareerCommand,
    CancelCareerActivityCommand, CareerActivitiesState, CareerActivityCatalogState,
    CareerActivityState, CareerApplicationReceipt, CareerApplicationState,
    CareerApplicationsPageState, CareerArtifactPageQuery, CareerArtifactPageState,
    CareerArtifactState, CareerEmploymentState, CareerEvidenceState, CareerInvitationReceipt,
    CareerInvitationState, CareerJobState, CareerJobsPageQuery, CareerJobsPageState,
    CareerOfferReceipt, CareerPageQuery, CareerSpecsState, CareerStore, CareerStoreResult,
    CashProductStore, CashProductStoreResult, CloseIsaAccountCommand, CloseIsaAccountReceipt,
    ConfirmCareerInterviewCommand, DeclineCareerInvitationCommand, DeclineCareerOfferCommand,
    EmploymentContractState, FinanceStore, FinanceStoreResult, FocusCareerCommand,
    GameCommandCursor, GameCommandRejection, IsaAccountState, M2dAssetStore, ManualAdvanceCommand,
    MarketStore, OpenTaxAccountCommand, OpenTaxAccountReceipt, PensionAccountState,
    PensionWithdrawalCommand, PensionWithdrawalReceipt, PublishCareerArtifactCommand,
    StartCareerActivityCommand, StartGameCommand, StartGameReceipt, StartPensionCommand,
    StartPensionReceipt, TaxAccountStore, TaxAccountStoreResult, TradeStoreResult, TradingStore,
    UserStore, WithdrawCareerApplicationCommand,
};
use crate::trading::{
    Portfolio, TradeExecution, TradeFailure, TradeOrder, checked_net_worth_krw, value_portfolio,
};

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
    pub active_activities: Vec<CareerActivitySnapshot>,
    pub latest_artifacts: Vec<CareerArtifactSnapshot>,
    #[schema(max_items = 10)]
    pub open_applications: Vec<CareerOpenApplicationSnapshot>,
    #[schema(max_items = 5)]
    pub open_invitations: Vec<CareerInvitationSnapshot>,
    #[schema(required = true, nullable)]
    pub employment: Option<CareerEmploymentContractSnapshot>,
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
    pub region: String,
    pub employment_type: String,
    pub required_scores: CareerScoresSnapshot,
    pub possessed_scores: CareerScoresSnapshot,
    pub minimum_annual_salary_krw: i64,
    pub maximum_annual_salary_krw: i64,
    pub salary_step_krw: i64,
    #[schema(value_type = String)]
    pub competition_band: crate::store::CareerCompetitionBand,
    pub military_requirement: String,
    #[schema(required = true, nullable)]
    pub minimum_education: Option<String>,
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
    markets: Arc<dyn MarketStore>,
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
    markets: Arc<dyn MarketStore>,
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
    pub markets: Arc<dyn MarketStore>,
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
        markets,
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
        markets,
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
            markets: dependencies.stores.markets,
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
    let cash_and_product_principal_krw = liquid_cash_krw
        .checked_add(save.active_product_principal_krw()?)
        .context("cash-product principal overflowed net worth")?;
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
        cash_and_product_principal_krw,
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
        },
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
        let markets: Arc<dyn MarketStore> = Arc::new(FakeMarketStore);
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
                    markets,
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
            let markets: Arc<dyn MarketStore> = Arc::new(FakeMarketStore);
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
                        markets,
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
