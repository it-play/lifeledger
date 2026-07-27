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

pub const PAYROLL_RATE_SCALE_PPM: i64 = 1_000_000;
pub const MAX_PAYROLL_DEPENDENTS: u8 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EmployerSizeBand {
    Under150,
    From150To999,
    AtLeast1000,
    Government,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DualContributionRatePolicy {
    pub employee_rate_ppm: i64,
    pub employer_rate_ppm: i64,
    pub employee_rounding_unit_krw: i64,
    pub employer_rounding_unit_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NationalPensionPolicy {
    pub monthly_income_rounding_unit_krw: i64,
    pub minimum_monthly_income_krw: i64,
    pub maximum_monthly_income_krw: i64,
    pub contribution: DualContributionRatePolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HealthInsurancePolicy {
    pub monthly_remuneration_rounding_unit_krw: i64,
    pub contribution: DualContributionRatePolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LongTermCarePolicy {
    pub health_premium_rate_numerator: i64,
    pub health_premium_rate_denominator: i64,
    pub employee_rounding_unit_krw: i64,
    pub employer_rounding_unit_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmployerContributionRate {
    pub employer_size_band: EmployerSizeBand,
    pub rate_ppm: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmploymentInsurancePolicy {
    pub employee_rate_ppm: i64,
    pub employer_rates: Vec<EmployerContributionRate>,
    pub employee_rounding_unit_krw: i64,
    pub employer_rounding_unit_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndustryContributionRate {
    pub industry: Industry,
    pub rate_ppm: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndustrialAccidentPolicy {
    pub employer_rates: Vec<IndustryContributionRate>,
    pub employer_rounding_unit_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmploymentWithholdingRow {
    pub lower_bound_krw: i64,
    pub upper_bound_exclusive_krw: Option<i64>,
    pub family_count: u8,
    pub child_count: u8,
    pub income_tax_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalIncomeWithholdingPolicy {
    pub income_tax_rate_ppm: i64,
    pub rounding_unit_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OtherIncomeRewardPolicy {
    pub income_tax_rate_ppm: i64,
    pub local_income_tax_rate_ppm: i64,
    pub income_tax_rounding_unit_krw: i64,
    pub local_income_tax_rounding_unit_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PayrollPolicy {
    pub national_pension: NationalPensionPolicy,
    pub health_insurance: HealthInsurancePolicy,
    pub long_term_care: LongTermCarePolicy,
    pub employment_insurance: EmploymentInsurancePolicy,
    pub industrial_accident: IndustrialAccidentPolicy,
    pub employment_withholding_table: Vec<EmploymentWithholdingRow>,
    pub local_income_withholding: LocalIncomeWithholdingPolicy,
    pub wanted_reward: Option<OtherIncomeRewardPolicy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PayrollPeriodInput {
    pub contract_id: u64,
    pub period_no: u64,
    pub contract_start_date: Date,
    pub annual_salary_krw: i64,
    pub payday_day_of_month: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct PayrollCalculationInput<'a> {
    pub period: PayrollPeriodInput,
    pub dependents: u8,
    pub employer_size_band: EmployerSizeBand,
    pub industry: Industry,
    pub wanted_reward_gross_krw: Option<i64>,
    pub policy: &'a PayrollPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PayrollPeriod {
    pub contract_id: u64,
    pub period_no: u64,
    pub salary_month_ordinal: u8,
    pub period_start_date: Date,
    pub period_end_exclusive_date: Date,
    pub payday: Date,
    pub calendar_days: u16,
    pub covered_days: u16,
    pub base_monthly_salary_krw: i64,
    pub gross_pay_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DualContributionBreakdown {
    pub assessed: bool,
    pub employee_basis_krw: i64,
    pub employer_basis_krw: i64,
    pub employee_rate_ppm: i64,
    pub employer_rate_ppm: i64,
    pub employee_rounding_unit_krw: i64,
    pub employer_rounding_unit_krw: i64,
    pub employee_amount_krw: i64,
    pub employer_amount_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LongTermCareBreakdown {
    pub assessed: bool,
    pub employee_health_premium_basis_krw: i64,
    pub employer_health_premium_basis_krw: i64,
    pub rate_numerator: i64,
    pub rate_denominator: i64,
    pub employee_rounding_unit_krw: i64,
    pub employer_rounding_unit_krw: i64,
    pub employee_amount_krw: i64,
    pub employer_amount_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmployerContributionBreakdown {
    pub basis_krw: i64,
    pub rate_ppm: i64,
    pub rounding_unit_krw: i64,
    pub employer_amount_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PayrollInsuranceBreakdown {
    pub national_pension: DualContributionBreakdown,
    pub health_insurance: DualContributionBreakdown,
    pub long_term_care: LongTermCareBreakdown,
    pub employment_insurance: DualContributionBreakdown,
    pub industrial_accident: EmployerContributionBreakdown,
    pub employee_total_krw: i64,
    pub employer_total_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmploymentWithholdingBreakdown {
    pub taxable_gross_krw: i64,
    pub family_count: u8,
    pub child_count: u8,
    pub row_lower_bound_krw: i64,
    pub row_upper_bound_exclusive_krw: Option<i64>,
    pub income_tax_krw: i64,
    pub local_income_tax_basis_krw: i64,
    pub local_income_tax_rate_ppm: i64,
    pub local_income_tax_rounding_unit_krw: i64,
    pub local_income_tax_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OtherIncomeRewardBreakdown {
    pub gross_reward_krw: i64,
    pub income_tax_rate_ppm: i64,
    pub local_income_tax_rate_ppm: i64,
    pub income_tax_rounding_unit_krw: i64,
    pub local_income_tax_rounding_unit_krw: i64,
    pub withheld_income_tax_krw: i64,
    pub withheld_local_income_tax_krw: i64,
    pub net_reward_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PayrollBreakdown {
    pub period: PayrollPeriod,
    pub insurance: PayrollInsuranceBreakdown,
    pub withholding: EmploymentWithholdingBreakdown,
    pub employee_insurance_total_krw: i64,
    pub employer_insurance_total_krw: i64,
    pub withheld_income_tax_krw: i64,
    pub withheld_local_income_tax_krw: i64,
    pub net_salary_pay_krw: i64,
    pub employment_income_accrual_krw: i64,
    pub wanted_reward: Option<OtherIncomeRewardBreakdown>,
    pub total_wallet_credit_krw: i64,
}

pub trait PayrollRules: Send + Sync + 'static {
    fn validate_policy(&self, policy: &PayrollPolicy) -> Result<(), PayrollError>;

    fn schedule_period(&self, input: PayrollPeriodInput) -> Result<PayrollPeriod, PayrollError>;

    fn calculate_payroll(
        &self,
        input: PayrollCalculationInput<'_>,
    ) -> Result<PayrollBreakdown, PayrollError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EarnedIncomeDeductionBracket {
    pub lower_bound_krw: i64,
    pub upper_bound_exclusive_krw: Option<i64>,
    pub base_deduction_krw: i64,
    pub marginal_rate_ppm: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressiveEmploymentTaxBracket {
    pub lower_bound_krw: i64,
    pub upper_bound_exclusive_krw: Option<i64>,
    pub rate_ppm: i64,
    pub quick_deduction_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EarnedIncomeTaxCreditPolicy {
    pub low_tax_boundary_krw: i64,
    pub low_tax_rate_ppm: i64,
    pub high_tax_base_credit_krw: i64,
    pub high_tax_marginal_rate_ppm: i64,
    pub salary_boundary_one_krw: i64,
    pub salary_boundary_two_krw: i64,
    pub cap_one_krw: i64,
    pub cap_two_base_krw: i64,
    pub cap_two_reduction_rate_ppm: i64,
    pub cap_two_floor_krw: i64,
    pub cap_three_base_krw: i64,
    pub cap_three_reduction_rate_ppm: i64,
    pub cap_three_floor_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PensionContributionCreditRate {
    pub income_tax_rate_ppm: i64,
    pub income_tax_rounding_unit_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PensionContributionCreditPolicy {
    pub pension_savings_limit_krw: i64,
    pub pension_savings_and_irp_limit_krw: i64,
    pub salary_rate_boundary_krw: i64,
    pub comprehensive_income_rate_boundary_krw: i64,
    pub lower_income_rate: PensionContributionCreditRate,
    pub higher_income_rate: PensionContributionCreditRate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnnualLocalIncomeTaxPolicy {
    pub linked_income_tax_rate_ppm: i64,
    pub rounding_unit_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmploymentAnnualTaxPolicy {
    pub tax_year: u16,
    pub earned_income_deduction_brackets: Vec<EarnedIncomeDeductionBracket>,
    pub basic_personal_deduction_per_person_krw: i64,
    pub taxable_income_rounding_unit_krw: i64,
    pub basic_tax_brackets: Vec<ProgressiveEmploymentTaxBracket>,
    pub calculated_tax_rounding_unit_krw: i64,
    pub earned_income_tax_credit: EarnedIncomeTaxCreditPolicy,
    pub pension_credit: PensionContributionCreditPolicy,
    pub local_income_tax: AnnualLocalIncomeTaxPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmploymentIncomeAuthorityInput {
    pub tax_year: u16,
    pub world_start_year: i32,
    pub legacy_profile_exists: bool,
    pub has_m3_taxable_payroll: bool,
    pub has_employment_income_year: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EmploymentIncomeAuthority {
    None,
    LegacyProfile,
    M3Payroll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmployeeStatutoryInsuranceAmounts {
    pub national_pension_krw: i64,
    pub health_insurance_krw: i64,
    pub long_term_care_krw: i64,
    pub employment_insurance_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PensionContributionAccountKind {
    PensionSavings,
    Irp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PensionOpeningTaxExcludedBalance {
    pub account_id: u64,
    pub amount_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PensionContributionEvent {
    pub contribution_source_id: u64,
    pub account_id: u64,
    pub account_kind: PensionContributionAccountKind,
    pub tax_year: u16,
    pub contribution_game_day: u32,
    pub ledger_transaction_id: u64,
    pub amount_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PensionWithdrawalEvent {
    pub account_id: u64,
    pub tax_year: u16,
    pub withdrawal_game_day: u32,
    pub ledger_transaction_id: u64,
    /// Only the portion consumed from M2's tax-excluded contribution layer.
    pub tax_excluded_withdrawn_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PensionContributionSourceEvent {
    Contribution(PensionContributionEvent),
    Withdrawal(PensionWithdrawalEvent),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PensionCreditIncome {
    EmploymentSalary { total_salary_krw: i64 },
    ComprehensiveIncome { comprehensive_income_krw: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PensionCreditPlanningInput<'a> {
    pub tax_year: u16,
    pub income: PensionCreditIncome,
    pub remaining_income_tax_before_pension_credit_krw: i64,
    pub local_income_tax_before_pension_effect_krw: i64,
    pub opening_tax_excluded_balances: &'a [PensionOpeningTaxExcludedBalance],
    pub source_events: &'a [PensionContributionSourceEvent],
    pub policy: &'a EmploymentAnnualTaxPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PensionContributionAllocation {
    pub contribution_source_id: u64,
    pub account_id: u64,
    pub account_kind: PensionContributionAccountKind,
    pub contribution_game_day: u32,
    pub ledger_transaction_id: u64,
    pub surviving_tax_excluded_contribution_krw: i64,
    pub limit_eligible_contribution_krw: i64,
    pub credited_contribution_krw: i64,
    pub credited_contribution_before_krw: i64,
    pub tax_excluded_contribution_after_krw: i64,
    pub credited_contribution_after_krw: i64,
    pub income_tax_credit_krw: i64,
    pub local_income_tax_effect_krw: i64,
    pub total_credit_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PensionCreditAllocationPlan {
    pub tax_year: u16,
    pub selected_income_tax_rate_ppm: i64,
    pub limit_eligible_contribution_krw: i64,
    pub credited_contribution_krw: i64,
    pub income_tax_credit_krw: i64,
    pub local_income_tax_effect_krw: i64,
    /// Every replayed contribution source, including sources with no layer movement.
    pub allocations: Vec<PensionContributionAllocation>,
}

impl PensionCreditAllocationPlan {
    /// Returns only source allocations that require a tax-layer reclassification.
    pub fn tax_layer_movements(&self) -> impl Iterator<Item = &PensionContributionAllocation> {
        self.allocations
            .iter()
            .filter(|allocation| allocation.credited_contribution_krw > 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmploymentOnlyTaxPlanningInput<'a> {
    pub authority: EmploymentIncomeAuthority,
    pub tax_year: u16,
    pub gross_employment_income_krw: i64,
    pub employee_statutory_insurance: EmployeeStatutoryInsuranceAmounts,
    pub personal_deduction_person_count: u8,
    pub withheld_income_tax_krw: i64,
    pub withheld_local_income_tax_krw: i64,
    pub requires_combined_assessment: bool,
    pub pension_opening_tax_excluded_balances: &'a [PensionOpeningTaxExcludedBalance],
    pub pension_source_events: &'a [PensionContributionSourceEvent],
    pub policy: &'a EmploymentAnnualTaxPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EmploymentTaxAssessmentStatus {
    Provisional,
    Definitive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EmploymentTaxAssessmentSource {
    EmploymentOnly,
    Combined,
    LegacyProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmploymentTaxCalculation {
    pub tax_year: u16,
    pub status: EmploymentTaxAssessmentStatus,
    pub source: EmploymentTaxAssessmentSource,
    pub gross_employment_income_krw: i64,
    pub employee_insurance_deduction_krw: i64,
    pub earned_income_deduction_krw: i64,
    pub personal_deduction_krw: i64,
    pub taxable_income_krw: i64,
    pub calculated_income_tax_krw: i64,
    pub earned_income_tax_credit_krw: i64,
    pub pension_credit_eligible_contribution_krw: i64,
    pub actual_pension_income_tax_credit_krw: i64,
    pub actual_pension_local_income_tax_effect_krw: i64,
    pub assessed_income_tax_krw: i64,
    pub assessed_local_income_tax_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaxReconciliationPlan {
    pub prepaid_income_tax_krw: i64,
    pub prepaid_local_income_tax_krw: i64,
    pub assessed_income_tax_krw: i64,
    pub assessed_local_income_tax_krw: i64,
    pub additional_tax_krw: i64,
    pub refund_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CombinedEmploymentTaxHandoff {
    pub tax_year: u16,
    pub gross_employment_income_krw: i64,
    pub earned_income_deduction_krw: i64,
    pub personal_deduction_krw: i64,
    pub employee_insurance_deduction_krw: i64,
    pub employment_taxable_income_krw: i64,
    pub calculated_employment_income_tax_krw: i64,
    pub earned_income_tax_credit_krw: i64,
    pub final_prepaid_employment_income_tax_krw: i64,
    pub final_prepaid_employment_local_income_tax_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmploymentTaxAssessmentPlan {
    pub calculation: EmploymentTaxCalculation,
    pub reconciliation: TaxReconciliationPlan,
    pub combined_handoff: CombinedEmploymentTaxHandoff,
    pub pension_allocation: Option<PensionCreditAllocationPlan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CombinedEmploymentTaxPlanningInput<'a> {
    pub authority: EmploymentIncomeAuthority,
    pub handoff: CombinedEmploymentTaxHandoff,
    pub comprehensive_income_krw: i64,
    pub calculated_combined_income_tax_krw: i64,
    /// The selected comparison-tax amount after every non-pension income-tax credit.
    pub income_tax_before_pension_credit_krw: i64,
    /// The selected comparison-tax local amount before the pension linkage effect.
    pub local_income_tax_before_pension_effect_krw: i64,
    pub total_prepaid_income_tax_krw: i64,
    pub total_prepaid_local_income_tax_krw: i64,
    pub pension_opening_tax_excluded_balances: &'a [PensionOpeningTaxExcludedBalance],
    pub pension_source_events: &'a [PensionContributionSourceEvent],
    pub policy: &'a EmploymentAnnualTaxPolicy,
}

pub trait EmploymentTaxRules: Send + Sync + 'static {
    fn validate_policy(&self, policy: &EmploymentAnnualTaxPolicy)
    -> Result<(), EmploymentTaxError>;

    fn select_income_authority(
        &self,
        input: EmploymentIncomeAuthorityInput,
    ) -> Result<EmploymentIncomeAuthority, EmploymentTaxError>;

    fn plan_pension_credit(
        &self,
        input: PensionCreditPlanningInput<'_>,
    ) -> Result<PensionCreditAllocationPlan, EmploymentTaxError>;

    fn plan_employment_only(
        &self,
        input: EmploymentOnlyTaxPlanningInput<'_>,
    ) -> Result<EmploymentTaxAssessmentPlan, EmploymentTaxError>;

    fn plan_combined(
        &self,
        input: CombinedEmploymentTaxPlanningInput<'_>,
    ) -> Result<EmploymentTaxAssessmentPlan, EmploymentTaxError>;

    fn plan_reconciliation(
        &self,
        prepaid_income_tax_krw: i64,
        prepaid_local_income_tax_krw: i64,
        assessed_income_tax_krw: i64,
        assessed_local_income_tax_krw: i64,
    ) -> Result<TaxReconciliationPlan, EmploymentTaxError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmploymentTaxError {
    InvalidTaxYear,
    PolicyTaxYearMismatch,
    InvalidMoney,
    InvalidPersonCount,
    InvalidRate,
    InvalidRoundingUnit,
    InvalidEarnedIncomeDeductionBrackets,
    InvalidBasicTaxBrackets,
    InvalidEarnedIncomeTaxCreditPolicy,
    InvalidPensionCreditPolicy,
    M3PayrollAuthorityRequired,
    DuplicatePensionOpeningBalance,
    DuplicatePensionContributionSource,
    DuplicatePensionLedgerTransaction,
    InvalidPensionEvent,
    PensionWithdrawalExceedsHistory,
    InvalidCombinedTaxInput,
    ArithmeticOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayrollError {
    InvalidContractId,
    InvalidPeriodNo,
    InvalidAnnualSalary,
    InvalidPayday,
    InvalidDependents,
    InvalidDate,
    InvalidRate,
    InvalidRoundingUnit,
    InvalidNationalPensionBounds,
    InvalidWithholdingRow,
    DuplicateEmployerSizeRate,
    MissingEmployerSizeRate,
    DuplicateIndustryRate,
    MissingIndustryRate,
    MissingWithholdingFamily,
    OverlappingWithholdingRows,
    WithholdingOutOfRange,
    MissingRewardPolicy,
    RewardOutsideFirstPeriod,
    InvalidReward,
    NegativeNetPay,
    ArithmeticOverflow,
}

pub const MILITARY_RATE_SCALE_PPM: i64 = 1_000_000;
pub const MAX_MILITARY_MONEY_KRW: i64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MilitaryStatus {
    Unserved,
    Serving,
    Completed,
    Exempt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MilitaryServiceType {
    ActiveDuty,
    SocialService,
    IndustrialTechnical,
    ProfessionalResearch,
    CommissionedOfficer,
    NonCommissionedOfficer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MilitaryServiceStatus {
    PendingStart,
    Serving,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitaryHardRequirements {
    pub minimum_education: Option<Education>,
    pub minimum_certification_count: u32,
    pub minimum_experience_days: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MilitaryEligibilityInput {
    pub military_subject: bool,
    pub education: Education,
    pub certification_count: u32,
    pub experience_days: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitaryPayStagePolicy {
    pub start_service_month: u16,
    pub end_exclusive_service_month: u16,
    pub gross_monthly_pay_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MilitaryPayScheduleKind {
    Monthly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MilitaryPartialMonthPayKind {
    FullMonthlyGross,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitaryExperiencePolicy {
    pub job_family_key: String,
    pub daily_credit_ppm: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitaryOptionPolicy {
    pub option_version_id: u64,
    pub service_type: MilitaryServiceType,
    pub service_duration_months: u16,
    pub pay_schedule_kind: MilitaryPayScheduleKind,
    pub payday_day_of_month: u8,
    pub partial_month_pay_kind: MilitaryPartialMonthPayKind,
    pub hard_requirements: MilitaryHardRequirements,
    pub pay_stages: Vec<MilitaryPayStagePolicy>,
    pub effort_life_status: LifeStatus,
    pub daily_effort_capacity_units: u64,
    pub experience: Vec<MilitaryExperiencePolicy>,
}

#[derive(Debug, Clone, Copy)]
pub struct MilitaryServiceStartInput<'a> {
    pub current_status: MilitaryStatus,
    pub current_game_day: u32,
    pub current_date: Date,
    pub eligibility: MilitaryEligibilityInput,
    pub option: &'a MilitaryOptionPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitaryServicePlan {
    pub option_version_id: u64,
    pub service_type: MilitaryServiceType,
    pub external_status: MilitaryStatus,
    pub service_status: MilitaryServiceStatus,
    pub start_game_day: u32,
    pub end_game_day: u32,
    pub start_date: Date,
    pub end_exclusive_date: Date,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MilitaryServiceTransitionInput {
    pub external_status: MilitaryStatus,
    pub service_status: MilitaryServiceStatus,
    pub current_game_day: u32,
    pub start_game_day: u32,
    pub end_game_day: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitaryServiceTransition {
    pub external_status: MilitaryStatus,
    pub service_status: MilitaryServiceStatus,
    pub changed: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct MilitaryPayStageInput<'a> {
    pub service_date: Date,
    pub service_start_date: Date,
    pub service_end_exclusive_date: Date,
    pub option: &'a MilitaryOptionPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitaryPayStage {
    pub service_month: u16,
    pub gross_monthly_pay_krw: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct MilitaryPayScheduleInput<'a> {
    pub service_start_game_day: u32,
    pub service_start_date: Date,
    pub service_end_exclusive_date: Date,
    pub option: &'a MilitaryOptionPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitaryPayPeriod {
    pub payroll_period: u32,
    pub payday: Date,
    pub pay_game_day: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct MilitaryServiceDayInput<'a> {
    pub current_game_day: u32,
    pub service: MilitaryServicePlan,
    pub option: &'a MilitaryOptionPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitaryExperienceCredit {
    pub job_family_key: String,
    pub credit_ppm: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitaryServiceDayEffect {
    pub credited_service_days: u32,
    pub effort_life_status: LifeStatus,
    pub available_effort_units: u64,
    pub experience: Vec<MilitaryExperienceCredit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitarySavingsPolicy {
    pub eligible_service_types: Vec<MilitaryServiceType>,
    pub minimum_remaining_service_months: u16,
    pub maximum_active_contracts: u8,
    pub maximum_contracts_per_institution: u8,
    pub institution_monthly_limit_krw: i64,
    pub total_monthly_limit_krw: i64,
    pub limit_setting_unit_krw: i64,
    pub minimum_installment_krw: i64,
    pub installment_unit_krw: i64,
    pub government_matching_rate_ppm: i64,
    pub government_match_payment_day_of_month: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitarySavingsInterestTier {
    pub minimum_term_months: u16,
    pub maximum_term_months_inclusive: u16,
    pub annual_interest_rate_ppm: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitarySavingsProductPolicy {
    pub product_version_id: u64,
    pub institution_key: String,
    pub interest_tiers: Vec<MilitarySavingsInterestTier>,
    pub day_count_denominator: u16,
    pub interest_rounding_unit_krw: i64,
    pub early_close_annual_interest_rate_ppm: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActiveMilitarySavingsContract {
    pub institution_key: String,
    pub monthly_contribution_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitarySavingsInstallmentDraft {
    pub installment_no: u32,
    pub due_date: Date,
    pub due_game_day: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct MilitarySavingsEnrollmentInput<'a> {
    pub external_status: MilitaryStatus,
    pub service_type: MilitaryServiceType,
    pub current_date: Date,
    pub current_game_day: u32,
    pub service_end_exclusive_date: Date,
    pub service_end_game_day: u32,
    pub institution_key: &'a str,
    pub monthly_contribution_krw: i64,
    pub debit_day_of_month: u8,
    pub active_contracts: &'a [ActiveMilitarySavingsContract],
    pub service_institution_contract_count: u32,
    pub policy: &'a MilitarySavingsPolicy,
    pub product: &'a MilitarySavingsProductPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitarySavingsEnrollmentPlan {
    pub product_version_id: u64,
    pub institution_key: String,
    pub monthly_contribution_krw: i64,
    pub debit_day_of_month: u8,
    pub contract_term_months: u16,
    pub annual_interest_rate_ppm: i64,
    pub maturity_date: Date,
    pub maturity_game_day: u32,
    pub installments: Vec<MilitarySavingsInstallmentDraft>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MilitarySavingsInstallmentStatus {
    Paid,
    Missed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MilitarySavingsMovement {
    PrincipalLocked,
    NoMovement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MilitarySavingsInstallmentInput {
    pub installment_no: u32,
    pub contribution_krw: i64,
    pub wallet_cash_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitarySavingsInstallmentPlan {
    pub installment_no: u32,
    pub status: MilitarySavingsInstallmentStatus,
    pub movement: MilitarySavingsMovement,
    pub wallet_cash_delta_krw: i64,
    pub principal_delta_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PaidMilitarySavingsInstallment {
    pub installment_no: u32,
    pub paid_date: Date,
    pub principal_krw: i64,
    pub government_matching_rate_ppm: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitarySavingsInterestLine {
    pub installment_no: u32,
    pub principal_krw: i64,
    pub held_days: u32,
    pub gross_interest_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitarySavingsGovernmentMatchLine {
    pub installment_no: u32,
    pub principal_krw: i64,
    pub matching_rate_ppm: i64,
    pub matching_amount_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitarySavingsGovernmentMatchPlan {
    pub due_date: Date,
    pub amount_krw: i64,
    pub installments: Vec<MilitarySavingsGovernmentMatchLine>,
}

#[derive(Debug, Clone, Copy)]
pub struct MilitarySavingsMaturityInput<'a> {
    pub maturity_date: Date,
    pub service_completion_confirmed: bool,
    pub annual_interest_rate_ppm: i64,
    pub day_count_denominator: u16,
    pub interest_rounding_unit_krw: i64,
    pub government_match_payment_day_of_month: u8,
    pub paid_installments: &'a [PaidMilitarySavingsInstallment],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitarySavingsMaturityPlan {
    pub principal_krw: i64,
    pub gross_bank_interest_krw: i64,
    pub wallet_credit_krw: i64,
    pub interest: Vec<MilitarySavingsInterestLine>,
    pub government_match: MilitarySavingsGovernmentMatchPlan,
}

#[derive(Debug, Clone, Copy)]
pub struct MilitarySavingsEarlyCloseInput<'a> {
    pub close_date: Date,
    pub maturity_date: Date,
    pub early_close_annual_interest_rate_ppm: i64,
    pub day_count_denominator: u16,
    pub interest_rounding_unit_krw: i64,
    pub paid_installments: &'a [PaidMilitarySavingsInstallment],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MilitarySavingsEarlyClosePlan {
    pub principal_krw: i64,
    pub gross_bank_interest_krw: i64,
    pub wallet_credit_krw: i64,
    pub interest: Vec<MilitarySavingsInterestLine>,
    pub government_match_krw: i64,
    pub tax_exempt: bool,
}

pub trait MilitaryRules: Send + Sync + 'static {
    fn parse_status(&self, value: &str) -> Result<MilitaryStatus, MilitaryError>;

    fn parse_service_type(&self, value: &str) -> Result<MilitaryServiceType, MilitaryError>;

    fn validate_option(&self, option: &MilitaryOptionPolicy) -> Result<(), MilitaryError>;

    fn plan_service_start(
        &self,
        input: MilitaryServiceStartInput<'_>,
    ) -> Result<MilitaryServicePlan, MilitaryError>;

    fn transition_service(
        &self,
        input: MilitaryServiceTransitionInput,
    ) -> Result<MilitaryServiceTransition, MilitaryError>;

    fn select_pay_stage(
        &self,
        input: MilitaryPayStageInput<'_>,
    ) -> Result<MilitaryPayStage, MilitaryError>;

    fn plan_pay_schedule(
        &self,
        input: MilitaryPayScheduleInput<'_>,
    ) -> Result<Vec<MilitaryPayPeriod>, MilitaryError>;

    fn plan_service_day(
        &self,
        input: MilitaryServiceDayInput<'_>,
    ) -> Result<MilitaryServiceDayEffect, MilitaryError>;

    fn validate_savings_policy(&self, policy: &MilitarySavingsPolicy) -> Result<(), MilitaryError>;

    fn validate_savings_product(
        &self,
        product: &MilitarySavingsProductPolicy,
    ) -> Result<(), MilitaryError>;

    fn minimum_remaining_service_met(
        &self,
        current_date: Date,
        service_end_exclusive_date: Date,
        minimum_remaining_service_months: u16,
    ) -> Result<bool, MilitaryError>;

    fn plan_savings_enrollment(
        &self,
        input: MilitarySavingsEnrollmentInput<'_>,
    ) -> Result<MilitarySavingsEnrollmentPlan, MilitaryError>;

    fn settle_savings_installment(
        &self,
        input: MilitarySavingsInstallmentInput,
    ) -> Result<MilitarySavingsInstallmentPlan, MilitaryError>;

    fn plan_savings_maturity(
        &self,
        input: MilitarySavingsMaturityInput<'_>,
    ) -> Result<MilitarySavingsMaturityPlan, MilitaryError>;

    fn plan_savings_early_close(
        &self,
        input: MilitarySavingsEarlyCloseInput<'_>,
    ) -> Result<MilitarySavingsEarlyClosePlan, MilitaryError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MilitaryError {
    UnknownStatus,
    UnknownServiceType,
    MilitaryStateConflict,
    NotEligible,
    InvalidOption,
    InvalidRequirement,
    InvalidServicePeriod,
    InvalidPaySchedule,
    InvalidPayStages,
    MissingPayStage,
    InvalidExperiencePolicy,
    InvalidSavingsPolicy,
    InvalidSavingsProduct,
    DuplicateInstitution,
    ContractLimitExceeded,
    InstitutionLimitExceeded,
    TotalLimitExceeded,
    InvalidContribution,
    InvalidDebitDay,
    InsufficientRemainingService,
    NoInstallments,
    InvalidInstallment,
    DuplicateInstallment,
    InvalidMoney,
    InvalidRate,
    InvalidDate,
    ServiceCompletionRequired,
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
impl_error_display!(PayrollError, "career payroll error");
impl_error_display!(EmploymentTaxError, "career employment tax error");
impl_error_display!(MilitaryError, "career military error");
