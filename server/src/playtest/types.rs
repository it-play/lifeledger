use async_trait::async_trait;

pub const MAXIMUM_ACTIVE_FEEDBACK: u64 = 20;
pub const MAXIMUM_FEEDBACK_CHARACTERS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentAction {
    Grant,
    Withdraw,
}

impl ConsentAction {
    pub const fn as_status(self) -> ConsentStoredStatus {
        match self {
            Self::Grant => ConsentStoredStatus::Granted,
            Self::Withdraw => ConsentStoredStatus::Withdrawn,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentStoredStatus {
    Granted,
    Withdrawn,
}

impl ConsentStoredStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::Withdrawn => "withdrawn",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentDisplayStatus {
    NotGranted,
    Granted,
    Withdrawn,
    PolicyChanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentPolicy {
    pub id: u64,
    pub scope: String,
    pub policy_key: String,
    pub version: u32,
    pub schema_version: u16,
    pub display_name: String,
    pub notice_text: String,
    pub canonical_sha256: String,
    pub analytics_collection: AnalyticsCollection,
    pub retention_maximum_days: u16,
    pub maximum_active_feedback: u64,
    pub message_maximum_characters: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyticsCollection {
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredConsent {
    pub policy_version_id: u64,
    pub status: ConsentStoredStatus,
    pub revision: u64,
    pub granted_at: String,
    pub withdrawn_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentState {
    pub status: ConsentDisplayStatus,
    pub revision: u64,
    pub policy_version_id: Option<u64>,
    pub granted_at: Option<String>,
    pub withdrawn_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackItem {
    pub id: String,
    pub category: FeedbackCategory,
    pub severity: FeedbackSeverity,
    pub message: String,
    pub run_revision: Option<u32>,
    pub run_manifest_sha256: Option<String>,
    pub finalization_sha256: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaytestFeedbackOverview {
    pub policy: ConsentPolicy,
    pub consent: ConsentState,
    pub feedback: Vec<FeedbackItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackCategory {
    Bug,
    Balance,
    Usability,
    Performance,
    Rules,
    Other,
}

impl FeedbackCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bug => "bug",
            Self::Balance => "balance",
            Self::Usability => "usability",
            Self::Performance => "performance",
            Self::Rules => "rules",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackSeverity {
    Blocking,
    Major,
    Minor,
    Suggestion,
}

impl FeedbackSeverity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Blocking => "blocking",
            Self::Major => "major",
            Self::Minor => "minor",
            Self::Suggestion => "suggestion",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentCommand {
    pub policy_version_id: u64,
    pub expected_revision: u64,
    pub action: ConsentAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackDraft {
    pub expected_consent_revision: u64,
    pub category: FeedbackCategory,
    pub severity: FeedbackSeverity,
    pub message: String,
    pub privacy_confirmed: bool,
    pub run_revision: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedFeedbackDraft {
    pub expected_consent_revision: u64,
    pub category: FeedbackCategory,
    pub severity: FeedbackSeverity,
    pub message: String,
    pub run_revision: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentTransition {
    pub changed: bool,
    pub policy_version_id: u64,
    pub status: ConsentStoredStatus,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentUpdate {
    pub consent: ConsentState,
    pub purged_feedback_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackDeletion {
    pub id: String,
    pub withdrawn_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaytestFailureCode {
    InvalidCommand,
    PolicyUnavailable,
    RevisionConflict,
    ConsentRequired,
    PrivacyConfirmationRequired,
    FeedbackCapacityReached,
    RunReferenceNotFound,
    FeedbackNotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaytestStoreResult<T> {
    Accepted(T),
    Rejected(PlaytestFailureCode),
}

pub trait PlaytestRules: Send + Sync {
    fn plan_consent_transition(
        &self,
        active_policy_version_id: u64,
        current: Option<&StoredConsent>,
        command: &ConsentCommand,
    ) -> Result<ConsentTransition, PlaytestFailureCode>;

    fn normalize_feedback(
        &self,
        draft: FeedbackDraft,
    ) -> Result<NormalizedFeedbackDraft, PlaytestFailureCode>;
}

#[async_trait]
pub trait PlaytestStore: Send + Sync {
    async fn overview(&self, user_id: u64) -> anyhow::Result<PlaytestFeedbackOverview>;

    async fn set_consent(
        &self,
        user_id: u64,
        command: ConsentCommand,
    ) -> anyhow::Result<PlaytestStoreResult<ConsentUpdate>>;

    async fn submit_feedback(
        &self,
        user_id: u64,
        draft: FeedbackDraft,
    ) -> anyhow::Result<PlaytestStoreResult<FeedbackItem>>;

    async fn delete_feedback(
        &self,
        user_id: u64,
        feedback_id: &str,
    ) -> anyhow::Result<PlaytestStoreResult<FeedbackDeletion>>;
}

#[async_trait]
pub trait PlaytestMaintenanceStore: Send + Sync {
    async fn purge_expired_feedback(&self) -> anyhow::Result<u64>;
}
