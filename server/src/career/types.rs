use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};
use time::Date;

use crate::character::Education;

pub const SPEC_SCORE_CAP_BP: i64 = 10_000;
pub const ARTIFACT_COMPLETENESS_SCALE_BP: i64 = 10_000;
pub const MAX_ACTIVE_ACTIVITIES: usize = 3;
pub const MAX_BRIDGE_CERTIFICATIONS: u32 = 50;
pub const MAX_BRIDGE_CAREER_YEARS: u32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CareerFailureCode {
    InvalidCommand,
    CharacterRequired,
    PolicyUnavailable,
    CatalogUnavailable,
    NotEligible,
    ActivityLimit,
    ArtifactRequired,
    PostingClosed,
    ApplicationLimit,
    AlreadyApplied,
    InterviewExpired,
    OfferExpired,
    AlreadyEmployed,
    MilitaryStateConflict,
    InsufficientWalletCash,
    LimitExceeded,
    IdempotencyConflict,
    SettlementConflict,
    Busy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpecDimension {
    Education,
    Certification,
    Language,
    Training,
    Experience,
    Project,
}

impl SpecDimension {
    pub const ALL: [Self; 6] = [
        Self::Education,
        Self::Certification,
        Self::Language,
        Self::Training,
        Self::Experience,
        Self::Project,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvidenceKind {
    Education,
    Certification,
    Language,
    Training,
    Experience,
    Project,
}

impl EvidenceKind {
    pub const fn dimension(self) -> SpecDimension {
        match self {
            Self::Education => SpecDimension::Education,
            Self::Certification => SpecDimension::Certification,
            Self::Language => SpecDimension::Language,
            Self::Training => SpecDimension::Training,
            Self::Experience => SpecDimension::Experience,
            Self::Project => SpecDimension::Project,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvidencePeriodKind {
    Regular,
    ZeroYearBridgeExperience,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidencePeriodFields {
    pub start_date: Option<Date>,
    pub end_exclusive_date: Option<Date>,
    pub kind: Option<EvidencePeriodKind>,
}

impl EvidencePeriodFields {
    pub const fn none() -> Self {
        Self {
            start_date: None,
            end_exclusive_date: None,
            kind: None,
        }
    }

    pub const fn regular(start_date: Date, end_exclusive_date: Date) -> Self {
        Self {
            start_date: Some(start_date),
            end_exclusive_date: Some(end_exclusive_date),
            kind: Some(EvidencePeriodKind::Regular),
        }
    }

    pub const fn zero_year_bridge(date: Date) -> Self {
        Self {
            start_date: Some(date),
            end_exclusive_date: Some(date),
            kind: Some(EvidencePeriodKind::ZeroYearBridgeExperience),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpecEvidence {
    pub evidence_id: u64,
    pub evidence_key: String,
    pub catalog_entry_key: String,
    pub kind: EvidenceKind,
    pub acquired_game_day: u32,
    pub expires_on_game_day: Option<u32>,
    pub period: EvidencePeriodFields,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobFamilyContribution {
    pub job_family_key: String,
    pub contribution_bp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpecCatalogEntry {
    pub catalog_entry_key: String,
    pub kind: EvidenceKind,
    pub stackable: bool,
    pub contributions: Vec<JobFamilyContribution>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DimensionScores {
    pub education: i64,
    pub certification: i64,
    pub language: i64,
    pub training: i64,
    pub experience: i64,
    pub project: i64,
}

impl DimensionScores {
    pub const fn get(self, dimension: SpecDimension) -> i64 {
        match dimension {
            SpecDimension::Education => self.education,
            SpecDimension::Certification => self.certification,
            SpecDimension::Language => self.language,
            SpecDimension::Training => self.training,
            SpecDimension::Experience => self.experience,
            SpecDimension::Project => self.project,
        }
    }

    pub(crate) fn set(&mut self, dimension: SpecDimension, value: i64) {
        match dimension {
            SpecDimension::Education => self.education = value,
            SpecDimension::Certification => self.certification = value,
            SpecDimension::Language => self.language = value,
            SpecDimension::Training => self.training = value,
            SpecDimension::Experience => self.experience = value,
            SpecDimension::Project => self.project = value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScoreViews {
    pub possessed: DimensionScores,
    pub visible: DimensionScores,
}

#[derive(Debug, Clone, Copy)]
pub struct SpecScoreInput<'a> {
    pub evaluated_job_family_key: &'a str,
    pub current_game_day: u32,
    pub evidence: &'a [SpecEvidence],
    pub catalog: &'a [SpecCatalogEntry],
    pub visible_evidence_ids: &'a [u64],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DimensionRequirement {
    pub dimension: SpecDimension,
    pub required_score_bp: i64,
    pub weight_bp: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct ScoreFitInput<'a> {
    pub candidate_scores: DimensionScores,
    pub requirements: &'a [DimensionRequirement],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScoreFit {
    pub dimension_fit_bp: DimensionScores,
    pub overall_fit_bp: i64,
}

pub trait SpecScoreRules: Send + Sync + 'static {
    fn calculate_score_views(&self, input: SpecScoreInput<'_>) -> Result<ScoreViews, ScoreError>;

    fn calculate_fit(&self, input: ScoreFitInput<'_>) -> Result<ScoreFit, ScoreError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoreError {
    EmptyJobFamilyKey,
    DuplicateEvidenceId(u64),
    DuplicateCatalogEntryKey(String),
    DuplicateJobFamilyContribution(String),
    UnknownCatalogEntry(String),
    EvidenceKindMismatch(u64),
    NegativeContribution(String),
    UnknownVisibleEvidenceId(u64),
    InvalidCandidateScore(SpecDimension),
    InvalidRequirement(SpecDimension),
    DuplicateRequirement(SpecDimension),
    MissingRequirement(SpecDimension),
    InvalidWeightTotal,
    ArithmeticOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeEvidenceKey {
    pub evidence_key: String,
    pub catalog_entry_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeEducationMapping {
    pub education: Education,
    pub evidence: BridgeEvidenceKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeExperienceMapping {
    pub career_years: u32,
    pub evidence: BridgeEvidenceKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeCatalog {
    pub default_focused_job_family_key: String,
    pub education_mappings: Vec<BridgeEducationMapping>,
    pub certification_order: Vec<BridgeEvidenceKey>,
    pub experience_mappings: Vec<BridgeExperienceMapping>,
}

#[derive(Debug, Clone, Copy)]
pub struct BridgePlanInput<'a> {
    pub catalog: &'a BridgeCatalog,
    pub education: Education,
    pub certifications: u32,
    pub career_years: u32,
    pub starting_age_years: u32,
    pub world_start_date: Date,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeEvidenceDraft {
    pub evidence_key: String,
    pub catalog_entry_key: String,
    pub kind: EvidenceKind,
    pub acquired_game_day: u32,
    pub period: EvidencePeriodFields,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InitialCareerPlan {
    pub focused_job_family_key: String,
    pub birth_date: Date,
    pub evidence: Vec<BridgeEvidenceDraft>,
}

pub trait BridgeEvidencePlanner: Send + Sync + 'static {
    fn plan_initial_evidence(
        &self,
        input: BridgePlanInput<'_>,
    ) -> Result<InitialCareerPlan, BridgeError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeError {
    EmptyDefaultFocus,
    InvalidEducationMappings,
    InvalidCertificationOrder,
    InvalidExperienceMappings,
    EmptyBridgeKey,
    DuplicateEvidenceKey(String),
    DuplicateCatalogEntryKey(String),
    CertificationCountOutOfRange,
    CareerYearsOutOfRange,
    ArithmeticOverflow,
    InvalidDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifeStatus {
    Unemployed,
    Employed,
    ActiveDuty,
    SocialService,
    SpecialService,
    OfficerOrNco,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActivityStatus {
    Planned,
    Active,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeStatusEffortCapacities {
    pub unemployed: u64,
    pub employed: u64,
    pub active_duty: u64,
    pub social_service: u64,
    pub special_service: u64,
    pub officer_or_nco: u64,
}

impl LifeStatusEffortCapacities {
    pub const fn for_status(self, status: LifeStatus) -> u64 {
        match status {
            LifeStatus::Unemployed => self.unemployed,
            LifeStatus::Employed => self.employed,
            LifeStatus::ActiveDuty => self.active_duty,
            LifeStatus::SocialService => self.social_service,
            LifeStatus::SpecialService => self.special_service,
            LifeStatus::OfficerOrNco => self.officer_or_nco,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivityCatalogEntry {
    pub catalog_entry_key: String,
    pub minimum_calendar_days: u32,
    pub required_effort_units: u64,
    pub daily_effort_cap_units: u64,
    pub allowed_life_statuses: Vec<LifeStatus>,
    pub cost_krw: i64,
    pub evidence_catalog_entry_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpecActivity {
    pub activity_id: u64,
    pub catalog_entry_key: String,
    pub status: ActivityStatus,
    pub priority: Option<u8>,
    pub started_game_day: Option<u32>,
    pub accumulated_effort_units: u64,
    pub completed_game_day: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
pub struct ActivityDayInput<'a> {
    pub current_game_day: u32,
    pub life_status: LifeStatus,
    pub capacities: LifeStatusEffortCapacities,
    pub catalog: &'a [ActivityCatalogEntry],
    pub activities: &'a [SpecActivity],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivityEffortAllocation {
    pub activity_id: u64,
    pub allocated_effort_units: u64,
    pub accumulated_effort_units: u64,
    pub elapsed_calendar_days: u32,
    pub status: ActivityStatus,
    pub completed_game_day: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivityDayPlan {
    pub available_effort_units: u64,
    pub remaining_effort_units: u64,
    pub allocations: Vec<ActivityEffortAllocation>,
    pub completed_activity_ids: Vec<u64>,
}

pub trait ActivityPlanner: Send + Sync + 'static {
    fn plan_day(&self, input: ActivityDayInput<'_>) -> Result<ActivityDayPlan, ActivityError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityError {
    EmptyCatalogEntryKey,
    DuplicateCatalogEntryKey(String),
    InvalidCatalogEntry(String),
    DuplicateAllowedLifeStatus(String),
    DuplicateActivityId(u64),
    UnknownCatalogEntry(String),
    TooManyActiveActivities,
    InvalidActivePriority(u64),
    DuplicateActivePriority(u8),
    InvalidActiveDates(u64),
    EffortExceedsRequirement(u64),
    ArithmeticOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArtifactKind {
    Portfolio,
    Resume,
    LinkedinProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Industry {
    ItSoftware,
    FinanceInsurance,
    Manufacturing,
    ConstructionEngineering,
    RetailService,
    PublicSocial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LinkedinFields {
    pub open_to_work: bool,
    pub industries: Vec<Industry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactDraft {
    pub kind: ArtifactKind,
    pub headline: String,
    pub summary: String,
    pub evidence_ids: Vec<u64>,
    pub linkedin: Option<LinkedinFields>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanonicalArtifact {
    pub kind: ArtifactKind,
    pub headline: String,
    pub summary: String,
    pub evidence_ids: Vec<u64>,
    pub linkedin: Option<LinkedinFields>,
}

#[derive(Debug, Clone, Copy)]
pub struct ArtifactValidationInput<'a> {
    pub draft: &'a ArtifactDraft,
    pub current_date: Date,
    pub birth_date: Date,
    pub owned_evidence: &'a [SpecEvidence],
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChecklistRule {
    HeadlinePresent,
    SummaryPresent,
    MinimumEvidenceCount { count: u8 },
    ContainsDimension { dimension: SpecDimension },
    ContainsEvidenceKind { evidence_kind: EvidenceKind },
    ProjectPresent,
    OpenToWork,
    IndustryCountAtLeast { count: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactChecklistRule {
    pub rule: ChecklistRule,
    pub weight_bp: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct ArtifactCompletenessInput<'a> {
    pub artifact: &'a CanonicalArtifact,
    pub owned_evidence: &'a [SpecEvidence],
    pub checklist: &'a [ArtifactChecklistRule],
}

pub trait ArtifactRules: Send + Sync + 'static {
    fn canonicalize(
        &self,
        input: ArtifactValidationInput<'_>,
    ) -> Result<CanonicalArtifact, ArtifactError>;

    fn validate_checklist(
        &self,
        kind: ArtifactKind,
        checklist: &[ArtifactChecklistRule],
    ) -> Result<(), ArtifactError>;

    fn calculate_completeness(
        &self,
        input: ArtifactCompletenessInput<'_>,
    ) -> Result<i64, ArtifactError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactError {
    EmptyHeadline,
    HeadlineTooLong,
    SummaryTooLong,
    ForbiddenHeadlineControl,
    ForbiddenSummaryControl,
    InvalidLinkedinFields,
    TooManyEvidence {
        kind: ArtifactKind,
        maximum: usize,
    },
    DuplicateOwnedEvidenceId(u64),
    DuplicateEvidenceId(u64),
    UnknownEvidenceId(u64),
    EvidenceKindNotAllowed {
        kind: ArtifactKind,
        evidence_id: u64,
    },
    InvalidEvidencePeriod(u64),
    EvidenceBeforeMinimumAge(u64),
    EvidenceEndsInFuture(u64),
    OverlappingResumeEvidence {
        dimension: SpecDimension,
        first_evidence_id: u64,
        second_evidence_id: u64,
    },
    EmptyChecklist,
    NegativeChecklistWeight,
    DuplicateChecklistRule,
    InvalidChecklistRule,
    InvalidChecklistWeightTotal,
    ArithmeticOverflow,
}

macro_rules! impl_error_display {
    ($type_name:ty, $label:literal) => {
        impl Display for $type_name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "{}: {self:?}", $label)
            }
        }

        impl Error for $type_name {}
    };
}

impl_error_display!(ScoreError, "career score error");
impl_error_display!(BridgeError, "career bridge error");
impl_error_display!(ActivityError, "career activity error");
impl_error_display!(ArtifactError, "career artifact error");
