use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::character::{Education, Region};

use super::score::create_spec_score_rules;
use super::types::{
    ArtifactKind, DimensionRequirement, DimensionScores, Industry, LifeStatus, ScoreError,
    ScoreFitInput, SpecScoreRules,
};

pub const SCORE_SCALE_BP: i64 = 10_000;
pub const PROBABILITY_SCALE_PPM: u32 = 1_000_000;
pub const APPLICATION_ORDINAL: u32 = 1;

const POSTING_DOMAIN: &[u8] = b"lifeledger.recruitment.posting.v1\0";
const STAGE_DOMAIN: &[u8] = b"lifeledger.recruitment.stage.v1\0";
const INVITATION_DOMAIN: &[u8] = b"lifeledger.recruitment.invitation.v1\0";
const HMAC_BLOCK_BYTES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlatformKey {
    Sarangbang,
    Jobkorea,
    Saramin,
    Wanted,
    Linkedin,
    Work24,
}

impl PlatformKey {
    pub const ALL: [Self; 6] = [
        Self::Sarangbang,
        Self::Jobkorea,
        Self::Saramin,
        Self::Wanted,
        Self::Linkedin,
        Self::Work24,
    ];

    pub const fn as_key(self) -> &'static str {
        match self {
            Self::Sarangbang => "sarangbang",
            Self::Jobkorea => "jobkorea",
            Self::Saramin => "saramin",
            Self::Wanted => "wanted",
            Self::Linkedin => "linkedin",
            Self::Work24 => "work24",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompetitionBand {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InvitationSource {
    None,
    Resume,
    LinkedinProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EmploymentType {
    Regular,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MilitaryPostingRequirement {
    None,
    CompletedOrExempt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MilitaryQualification {
    Pending,
    Serving,
    CompletedOrExempt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApplicationSource {
    Direct,
    Invitation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecruitmentStage {
    Document,
    Interview,
    Invitation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScoreBand {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentWeights {
    pub primary_fit_bp: i64,
    pub supporting_fit_bp: i64,
    pub context_fit_bp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InvitationComponentWeights {
    pub completeness_bp: i64,
    pub language_score_bp: i64,
    pub experience_score_bp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScoreBandBoundaries {
    pub medium_minimum_bp: i64,
    pub high_minimum_bp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScoreBandProbabilities {
    pub low_score_ppm: u32,
    pub medium_score_ppm: u32,
    pub high_score_ppm: u32,
}

impl ScoreBandProbabilities {
    const fn for_score_band(self, score_band: ScoreBand) -> u32 {
        match score_band {
            ScoreBand::Low => self.low_score_ppm,
            ScoreBand::Medium => self.medium_score_ppm,
            ScoreBand::High => self.high_score_ppm,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompetitionProbabilities {
    pub low: ScoreBandProbabilities,
    pub medium: ScoreBandProbabilities,
    pub high: ScoreBandProbabilities,
}

impl CompetitionProbabilities {
    const fn for_competition(self, competition: CompetitionBand) -> ScoreBandProbabilities {
        match competition {
            CompetitionBand::Low => self.low,
            CompetitionBand::Medium => self.medium,
            CompetitionBand::High => self.high,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PassProbabilityTable {
    pub document: CompetitionProbabilities,
    pub interview: CompetitionProbabilities,
    pub invitation: CompetitionProbabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecruitmentRuleset {
    pub ruleset_key: String,
    pub document_weights: ComponentWeights,
    pub interview_weights: ComponentWeights,
    pub linkedin_invitation_weights: InvitationComponentWeights,
    pub score_bands: ScoreBandBoundaries,
    pub pass_probabilities: PassProbabilityTable,
    pub start_delay_days: u32,
    pub monthly_payday: u8,
    pub active_application_limit: u32,
    pub direct_application_daily_limit: u32,
    pub open_invitation_limit: u32,
}

pub fn v1_recruitment_ruleset() -> RecruitmentRuleset {
    let document = CompetitionProbabilities {
        low: ScoreBandProbabilities {
            low_score_ppm: 400_000,
            medium_score_ppm: 700_000,
            high_score_ppm: 900_000,
        },
        medium: ScoreBandProbabilities {
            low_score_ppm: 250_000,
            medium_score_ppm: 550_000,
            high_score_ppm: 800_000,
        },
        high: ScoreBandProbabilities {
            low_score_ppm: 120_000,
            medium_score_ppm: 350_000,
            high_score_ppm: 650_000,
        },
    };
    let interview = CompetitionProbabilities {
        low: ScoreBandProbabilities {
            low_score_ppm: 350_000,
            medium_score_ppm: 650_000,
            high_score_ppm: 880_000,
        },
        medium: ScoreBandProbabilities {
            low_score_ppm: 220_000,
            medium_score_ppm: 500_000,
            high_score_ppm: 760_000,
        },
        high: ScoreBandProbabilities {
            low_score_ppm: 100_000,
            medium_score_ppm: 300_000,
            high_score_ppm: 600_000,
        },
    };
    let invitation = CompetitionProbabilities {
        low: ScoreBandProbabilities {
            low_score_ppm: 50_000,
            medium_score_ppm: 150_000,
            high_score_ppm: 300_000,
        },
        medium: ScoreBandProbabilities {
            low_score_ppm: 35_000,
            medium_score_ppm: 120_000,
            high_score_ppm: 250_000,
        },
        high: ScoreBandProbabilities {
            low_score_ppm: 20_000,
            medium_score_ppm: 80_000,
            high_score_ppm: 200_000,
        },
    };

    RecruitmentRuleset {
        ruleset_key: "dev-unranked-m3-recruitment-v1".to_owned(),
        document_weights: ComponentWeights {
            primary_fit_bp: 6_000,
            supporting_fit_bp: 2_500,
            context_fit_bp: 1_500,
        },
        interview_weights: ComponentWeights {
            primary_fit_bp: 6_000,
            supporting_fit_bp: 2_500,
            context_fit_bp: 1_500,
        },
        linkedin_invitation_weights: InvitationComponentWeights {
            completeness_bp: 5_000,
            language_score_bp: 2_500,
            experience_score_bp: 2_500,
        },
        score_bands: ScoreBandBoundaries {
            medium_minimum_bp: 4_000,
            high_minimum_bp: 7_000,
        },
        pass_probabilities: PassProbabilityTable {
            document,
            interview,
            invitation,
        },
        start_delay_days: 1,
        monthly_payday: 25,
        active_application_limit: 10,
        direct_application_daily_limit: 3,
        open_invitation_limit: 5,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlatformDefinition {
    pub platform: PlatformKey,
    pub daily_slot_count: u32,
    pub competition_band: CompetitionBand,
    pub document_review_days: u32,
    pub same_region_only: bool,
    pub invitation_source: InvitationSource,
    pub required_artifacts: Vec<ArtifactKind>,
    pub first_pay_reward_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlatformIndustryWeight {
    pub industry: Industry,
    pub weight_bp: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobTemplate {
    pub template_key: String,
    pub platform: PlatformKey,
    pub employer_key: String,
    pub employer_name: String,
    pub industry: Industry,
    pub job_family_key: String,
    pub region: Region,
    pub employment_type: EmploymentType,
    pub minimum_education: Option<Education>,
    pub required_certification_entry_key: Option<String>,
    pub minimum_experience_days: u32,
    pub military_requirement: MilitaryPostingRequirement,
    pub minimum_annual_salary_krw: i64,
    pub maximum_annual_salary_krw: i64,
    pub salary_step_krw: i64,
    pub interview_delay_days: u32,
    pub offer_expiry_days: u32,
    pub posting_open_days: u32,
    pub requirements: Vec<DimensionRequirement>,
}

#[derive(Debug, Clone, Copy)]
pub struct PostingSeedInput<'a> {
    pub world_model_version: &'a str,
    pub world_seed: u64,
    pub career_catalog_bundle_key: &'a str,
    pub game_day: u32,
    /// Slot numbers are zero-based and must be below the platform's daily slot count.
    pub slot_no: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct PostingMaterializationInput<'a> {
    pub seed: PostingSeedInput<'a>,
    pub platform: &'a PlatformDefinition,
    pub industry_weights: &'a [PlatformIndustryWeight],
    pub templates: &'a [JobTemplate],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MaterializedPosting {
    pub posting_key: String,
    pub world_model_version: String,
    pub career_catalog_bundle_key: String,
    pub recruitment_ruleset_key: String,
    pub platform: PlatformKey,
    pub template_key: String,
    pub employer_key: String,
    pub employer_name: String,
    pub industry: Industry,
    pub job_family_key: String,
    pub region: Region,
    pub employment_type: EmploymentType,
    pub posted_game_day: u32,
    pub closes_exclusive_game_day: u32,
    pub competition_band: CompetitionBand,
    pub document_review_days: u32,
    pub same_region_only: bool,
    pub required_artifacts: Vec<ArtifactKind>,
    pub first_pay_reward_krw: i64,
    pub minimum_education: Option<Education>,
    pub required_certification_entry_key: Option<String>,
    pub minimum_experience_days: u32,
    pub military_requirement: MilitaryPostingRequirement,
    pub minimum_annual_salary_krw: i64,
    pub maximum_annual_salary_krw: i64,
    pub salary_step_krw: i64,
    pub interview_delay_days: u32,
    pub offer_expiry_days: u32,
    pub requirements: Vec<DimensionRequirement>,
    pub platform_affinity_bp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmittedArtifact {
    pub artifact_version_id: u64,
    pub kind: ArtifactKind,
    pub belongs_to_current_run: bool,
    pub is_public: bool,
    pub completeness_bp: i64,
    pub evidence_ids: Vec<u64>,
    pub open_to_work: bool,
    pub industries: Vec<Industry>,
}

#[derive(Debug, Clone, Copy)]
pub struct CandidateApplicationProfile<'a> {
    pub region: Region,
    pub life_status: LifeStatus,
    pub has_active_or_pending_contract: bool,
    pub education: Education,
    pub valid_catalog_entry_keys: &'a [&'a str],
    pub experience_days: u32,
    pub military_qualification: MilitaryQualification,
}

#[derive(Debug, Clone, Copy)]
pub struct ApplicationEligibilityInput<'a> {
    pub posting: &'a MaterializedPosting,
    pub current_game_day: u32,
    pub source: ApplicationSource,
    pub candidate: CandidateApplicationProfile<'a>,
    pub submitted_artifacts: &'a [SubmittedArtifact],
    pub active_application_count: u32,
    pub direct_applications_today: u32,
    pub already_applied_to_posting: bool,
    pub invitation_decision: Option<&'a StageDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PinnedArtifactVersion {
    pub artifact_version_id: u64,
    pub kind: ArtifactKind,
    pub completeness_bp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationPin {
    pub artifacts: Vec<PinnedArtifactVersion>,
    pub visible_evidence_ids: Vec<u64>,
    pub artifact_completeness_bp: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct DocumentEvaluationInput<'a> {
    pub world_seed: u64,
    pub posting: &'a MaterializedPosting,
    pub visible_scores: DimensionScores,
    pub artifact_completeness_bp: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct InterviewEvaluationInput<'a> {
    pub world_seed: u64,
    pub posting: &'a MaterializedPosting,
    pub possessed_scores: DimensionScores,
    pub pinned_evidence_ids: &'a [u64],
    pub currently_valid_evidence_ids: &'a [u64],
}

#[derive(Debug, Clone, Copy)]
pub struct InvitationEvaluationInput<'a> {
    pub world_seed: u64,
    pub posting: &'a MaterializedPosting,
    pub invitation_game_day: u32,
    pub candidate: CandidateApplicationProfile<'a>,
    pub latest_public_artifact: &'a SubmittedArtifact,
    pub visible_scores: DimensionScores,
    pub open_invitation_count: u32,
    pub platform_invitation_already_generated_today: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StageComponents {
    pub primary_fit_bp: i64,
    pub supporting_fit_bp: i64,
    pub context_fit_bp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StageDecision {
    pub stage: RecruitmentStage,
    pub score_band: ScoreBand,
    pub components: StageComponents,
    pub dimension_fit_bp: Option<DimensionScores>,
    pub score_bp: i64,
    pub probability_ppm: u32,
    pub roll_ppm: u32,
    pub passed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InvitationDecision {
    pub decision: StageDecision,
    pub pin: ApplicationPin,
}

#[derive(Debug, Clone, Copy)]
pub struct OfferSalaryInput<'a> {
    pub world_seed: u64,
    pub posting: &'a MaterializedPosting,
    pub possessed_fit_bp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OfferSalary {
    pub annual_salary_krw: i64,
    pub salary_step_index: u64,
    pub salary_roll_word: u64,
    pub possessed_fit_bp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ApplicationState {
    Submitted {
        submitted_game_day: u32,
        document_decision_game_day: u32,
        interview_delay_days: u32,
        offer_expiry_days: u32,
    },
    DocumentRejected {
        decided_game_day: u32,
        decision: StageDecision,
    },
    InterviewAwaitingConfirmation {
        entry_decision: StageDecision,
        confirmation_deadline_exclusive_game_day: u32,
        interview_game_day: u32,
        offer_expiry_days: u32,
    },
    InterviewConfirmed {
        entry_decision: StageDecision,
        confirmed_game_day: u32,
        interview_game_day: u32,
        offer_expiry_days: u32,
    },
    Withdrawn {
        withdrawn_game_day: u32,
    },
    InterviewRejected {
        decided_game_day: u32,
        entry_decision: StageDecision,
        interview_decision: StageDecision,
    },
    Offered {
        offered_game_day: u32,
        expires_exclusive_game_day: u32,
        entry_decision: StageDecision,
        interview_decision: StageDecision,
        salary: OfferSalary,
    },
    Accepted {
        accepted_game_day: u32,
        entry_decision: StageDecision,
        interview_decision: StageDecision,
        salary: OfferSalary,
    },
    Declined {
        declined_game_day: u32,
    },
    Expired {
        expired_game_day: u32,
    },
    Closed {
        closed_game_day: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplicationAction {
    ResolveDocument {
        game_day: u32,
        decision: StageDecision,
    },
    ConfirmInterview {
        game_day: u32,
    },
    DeclineInterview {
        game_day: u32,
    },
    Withdraw {
        game_day: u32,
    },
    ExpireConfirmation {
        game_day: u32,
    },
    ResolveInterview {
        game_day: u32,
        decision: StageDecision,
        salary: Option<OfferSalary>,
    },
    DeclineOffer {
        game_day: u32,
    },
    ExpireOffer {
        game_day: u32,
    },
    Close {
        game_day: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EmploymentContractStatus {
    PendingStart,
    Active,
    Ended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmploymentContractSummary {
    pub contract_id: u64,
    pub status: EmploymentContractStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmploymentContractDraft {
    pub application_id: u64,
    pub posting_key: String,
    pub status: EmploymentContractStatus,
    pub annual_salary_krw: i64,
    pub start_game_day: u32,
    pub monthly_payday: u8,
    pub career_catalog_bundle_key: String,
    pub recruitment_ruleset_key: String,
    pub first_pay_reward_krw: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct OfferAcceptanceInput<'a> {
    pub application_id: u64,
    pub posting: &'a MaterializedPosting,
    pub state: &'a ApplicationState,
    pub accepted_game_day: u32,
    pub contracts: &'a [EmploymentContractSummary],
    pub other_accepted_offer_count: u32,
    pub other_open_application_ids: &'a [u64],
    pub open_invitation_ids: &'a [u64],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OfferAcceptancePlan {
    pub accepted_state: ApplicationState,
    pub contract: EmploymentContractDraft,
    pub close_application_ids: Vec<u64>,
    pub close_invitation_ids: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationSubmissionPlan {
    pub pin: ApplicationPin,
    pub state: ApplicationState,
}

pub trait RecruitmentRules: Send + Sync + 'static {
    fn ruleset(&self) -> &RecruitmentRuleset;

    fn materialize_posting(
        &self,
        input: PostingMaterializationInput<'_>,
    ) -> Result<MaterializedPosting, RecruitmentError>;

    fn prepare_application(
        &self,
        input: ApplicationEligibilityInput<'_>,
    ) -> Result<ApplicationSubmissionPlan, RecruitmentError>;

    fn evaluate_document(
        &self,
        input: DocumentEvaluationInput<'_>,
    ) -> Result<StageDecision, RecruitmentError>;

    fn evaluate_interview(
        &self,
        input: InterviewEvaluationInput<'_>,
    ) -> Result<StageDecision, RecruitmentError>;

    fn evaluate_invitation(
        &self,
        input: InvitationEvaluationInput<'_>,
    ) -> Result<InvitationDecision, RecruitmentError>;

    fn initial_application_state(
        &self,
        posting: &MaterializedPosting,
        submitted_game_day: u32,
    ) -> Result<ApplicationState, RecruitmentError>;

    fn transition_application(
        &self,
        state: &ApplicationState,
        action: ApplicationAction,
    ) -> Result<ApplicationState, RecruitmentError>;

    fn determine_offer_salary(
        &self,
        input: OfferSalaryInput<'_>,
    ) -> Result<OfferSalary, RecruitmentError>;

    fn plan_offer_acceptance(
        &self,
        input: OfferAcceptanceInput<'_>,
    ) -> Result<OfferAcceptancePlan, RecruitmentError>;

    fn validate_contracts(
        &self,
        contracts: &[EmploymentContractSummary],
    ) -> Result<(), RecruitmentError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecruitmentError {
    InvalidRuleset,
    InvalidStableKey,
    InvalidPlatform(PlatformKey),
    InvalidIndustryWeights,
    InvalidTemplate(String),
    DuplicateTemplateKey(String),
    MissingTemplate,
    InvalidSlot,
    ArithmeticOverflow,
    Score(ScoreError),
    ActiveEmployment,
    ServiceConflict,
    PostingClosed,
    RegionMismatch,
    EducationRequired,
    CertificationRequired,
    ExperienceRequired,
    MilitaryRequirementNotMet,
    ArtifactRequired(ArtifactKind),
    ArtifactNotOwned(u64),
    ArtifactNotPublic(u64),
    DuplicateArtifactKind(ArtifactKind),
    UnexpectedArtifact(ArtifactKind),
    InvalidArtifact(u64),
    ApplicationLimit,
    AlreadyApplied,
    InvitationUnsupported,
    InvitationProfileIneligible,
    InvitationLimit,
    InvalidStageDecision,
    InvalidApplicationState,
    DecisionNotDue,
    InterviewExpired,
    OfferExpired,
    OfferSalaryRequired,
    OfferSalaryUnexpected,
    AlreadyAcceptedOffer,
    DuplicateContractId(u64),
    MultipleActiveContracts,
    DuplicateCloseId(u64),
}

impl Display for RecruitmentError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "career recruitment error: {self:?}")
    }
}

impl Error for RecruitmentError {}

struct V1RecruitmentRules {
    ruleset: RecruitmentRuleset,
    score_rules: Arc<dyn SpecScoreRules>,
}

pub fn create_recruitment_rules(
    ruleset: RecruitmentRuleset,
) -> Result<Arc<dyn RecruitmentRules>, RecruitmentError> {
    validate_ruleset(&ruleset)?;
    Ok(Arc::new(V1RecruitmentRules {
        ruleset,
        score_rules: create_spec_score_rules(),
    }))
}

pub fn create_v1_recruitment_rules() -> Arc<dyn RecruitmentRules> {
    Arc::new(V1RecruitmentRules {
        ruleset: v1_recruitment_ruleset(),
        score_rules: create_spec_score_rules(),
    })
}

impl RecruitmentRules for V1RecruitmentRules {
    fn ruleset(&self) -> &RecruitmentRuleset {
        &self.ruleset
    }

    fn materialize_posting(
        &self,
        input: PostingMaterializationInput<'_>,
    ) -> Result<MaterializedPosting, RecruitmentError> {
        validate_platform(input.platform)?;
        validate_seed(input.seed)?;
        if input.seed.slot_no >= input.platform.daily_slot_count {
            return Err(RecruitmentError::InvalidSlot);
        }

        let weights = validate_industry_weights(input.industry_weights)?;
        let maximum_weight = weights
            .values()
            .copied()
            .max()
            .ok_or(RecruitmentError::InvalidIndustryWeights)?;
        if maximum_weight == 0 {
            return Err(RecruitmentError::InvalidIndustryWeights);
        }

        let mut template_keys = HashSet::with_capacity(input.templates.len());
        let mut candidates = Vec::new();
        for template in input.templates {
            if !template_keys.insert(template.template_key.as_str()) {
                return Err(RecruitmentError::DuplicateTemplateKey(
                    template.template_key.clone(),
                ));
            }
            validate_template(template, &self.score_rules)?;
            if template.platform == input.platform.platform {
                let weight = weights
                    .get(&template.industry)
                    .copied()
                    .ok_or(RecruitmentError::InvalidIndustryWeights)?;
                candidates.push((template, weight));
            }
        }
        if candidates.is_empty() {
            return Err(RecruitmentError::MissingTemplate);
        }
        candidates.sort_by(|left, right| {
            left.0
                .template_key
                .as_bytes()
                .cmp(right.0.template_key.as_bytes())
        });
        let total_weight = candidates.iter().try_fold(0_u64, |total, (_, weight)| {
            total
                .checked_add(u64::from(*weight))
                .ok_or(RecruitmentError::ArithmeticOverflow)
        })?;
        if total_weight == 0 {
            return Err(RecruitmentError::InvalidIndustryWeights);
        }

        let posting_key_digest = posting_digest(
            input.seed,
            &self.ruleset.ruleset_key,
            input.platform.platform,
            0,
        )?;
        let selection_word = first_u64(posting_digest(
            input.seed,
            &self.ruleset.ruleset_key,
            input.platform.platform,
            1,
        )?);
        let selection = scaled_word(selection_word, total_weight)?;
        let mut cumulative = 0_u64;
        let mut selected = None;
        for (template, weight) in candidates {
            cumulative = cumulative
                .checked_add(u64::from(weight))
                .ok_or(RecruitmentError::ArithmeticOverflow)?;
            if selection < cumulative {
                selected = Some((template, weight));
                break;
            }
        }
        let (template, selected_weight) =
            selected.ok_or(RecruitmentError::InvalidIndustryWeights)?;
        let closes_exclusive_game_day = input
            .seed
            .game_day
            .checked_add(template.posting_open_days)
            .ok_or(RecruitmentError::ArithmeticOverflow)?;
        let platform_affinity_bp = i64::try_from(
            u128::from(selected_weight)
                .checked_mul(u128::from(SCORE_SCALE_BP as u64))
                .ok_or(RecruitmentError::ArithmeticOverflow)?
                .checked_div(u128::from(maximum_weight))
                .ok_or(RecruitmentError::ArithmeticOverflow)?,
        )
        .map_err(|_| RecruitmentError::ArithmeticOverflow)?;

        Ok(MaterializedPosting {
            posting_key: lowercase_hex(posting_key_digest),
            world_model_version: input.seed.world_model_version.to_owned(),
            career_catalog_bundle_key: input.seed.career_catalog_bundle_key.to_owned(),
            recruitment_ruleset_key: self.ruleset.ruleset_key.clone(),
            platform: input.platform.platform,
            template_key: template.template_key.clone(),
            employer_key: template.employer_key.clone(),
            employer_name: template.employer_name.clone(),
            industry: template.industry,
            job_family_key: template.job_family_key.clone(),
            region: template.region,
            employment_type: template.employment_type,
            posted_game_day: input.seed.game_day,
            closes_exclusive_game_day,
            competition_band: input.platform.competition_band,
            document_review_days: input.platform.document_review_days,
            same_region_only: input.platform.same_region_only,
            required_artifacts: input.platform.required_artifacts.clone(),
            first_pay_reward_krw: input.platform.first_pay_reward_krw,
            minimum_education: template.minimum_education,
            required_certification_entry_key: template.required_certification_entry_key.clone(),
            minimum_experience_days: template.minimum_experience_days,
            military_requirement: template.military_requirement,
            minimum_annual_salary_krw: template.minimum_annual_salary_krw,
            maximum_annual_salary_krw: template.maximum_annual_salary_krw,
            salary_step_krw: template.salary_step_krw,
            interview_delay_days: template.interview_delay_days,
            offer_expiry_days: template.offer_expiry_days,
            requirements: template.requirements.clone(),
            platform_affinity_bp,
        })
    }

    fn prepare_application(
        &self,
        input: ApplicationEligibilityInput<'_>,
    ) -> Result<ApplicationSubmissionPlan, RecruitmentError> {
        validate_candidate_access(input.posting, input.current_game_day, input.candidate)?;
        validate_qualifications(input.posting, input.candidate)?;
        let pin = pin_application_artifacts(
            &input.posting.required_artifacts,
            input.submitted_artifacts,
        )?;
        if input.active_application_count >= self.ruleset.active_application_limit {
            return Err(RecruitmentError::ApplicationLimit);
        }
        if input.source == ApplicationSource::Direct
            && input.direct_applications_today >= self.ruleset.direct_application_daily_limit
        {
            return Err(RecruitmentError::ApplicationLimit);
        }
        if input.already_applied_to_posting {
            return Err(RecruitmentError::AlreadyApplied);
        }
        let state = match (input.source, input.invitation_decision) {
            (ApplicationSource::Direct, None) => {
                self.initial_application_state(input.posting, input.current_game_day)?
            }
            (ApplicationSource::Invitation, Some(decision)) => {
                validate_stage_decision(decision, RecruitmentStage::Invitation)?;
                if !decision.passed {
                    return Err(RecruitmentError::InvalidStageDecision);
                }
                let interview_game_day = input
                    .current_game_day
                    .checked_add(input.posting.interview_delay_days)
                    .ok_or(RecruitmentError::ArithmeticOverflow)?;
                ApplicationState::InterviewAwaitingConfirmation {
                    entry_decision: decision.clone(),
                    confirmation_deadline_exclusive_game_day: interview_game_day,
                    interview_game_day,
                    offer_expiry_days: input.posting.offer_expiry_days,
                }
            }
            (ApplicationSource::Direct, Some(_)) | (ApplicationSource::Invitation, None) => {
                return Err(RecruitmentError::InvalidStageDecision);
            }
        };

        Ok(ApplicationSubmissionPlan { pin, state })
    }

    fn evaluate_document(
        &self,
        input: DocumentEvaluationInput<'_>,
    ) -> Result<StageDecision, RecruitmentError> {
        validate_component(input.artifact_completeness_bp)?;
        validate_component(input.posting.platform_affinity_bp)?;
        let fit = self
            .score_rules
            .calculate_fit(ScoreFitInput {
                candidate_scores: input.visible_scores,
                requirements: &input.posting.requirements,
            })
            .map_err(RecruitmentError::Score)?;
        let components = StageComponents {
            primary_fit_bp: fit.overall_fit_bp,
            supporting_fit_bp: input.artifact_completeness_bp,
            context_fit_bp: input.posting.platform_affinity_bp,
        };
        let score_bp = weighted_score(components, self.ruleset.document_weights)?;
        self.stage_decision(
            input.world_seed,
            input.posting,
            RecruitmentStage::Document,
            components,
            Some(fit.dimension_fit_bp),
            score_bp,
        )
    }

    fn evaluate_interview(
        &self,
        input: InterviewEvaluationInput<'_>,
    ) -> Result<StageDecision, RecruitmentError> {
        let fit = self
            .score_rules
            .calculate_fit(ScoreFitInput {
                candidate_scores: input.possessed_scores,
                requirements: &input.posting.requirements,
            })
            .map_err(RecruitmentError::Score)?;
        let experience_project_fit_bp = fit
            .dimension_fit_bp
            .experience
            .checked_add(fit.dimension_fit_bp.project)
            .ok_or(RecruitmentError::ArithmeticOverflow)?
            / 2;
        let profile_consistency_bp = profile_consistency(
            input.pinned_evidence_ids,
            input.currently_valid_evidence_ids,
        )?;
        let components = StageComponents {
            primary_fit_bp: fit.overall_fit_bp,
            supporting_fit_bp: experience_project_fit_bp,
            context_fit_bp: profile_consistency_bp,
        };
        let score_bp = weighted_score(components, self.ruleset.interview_weights)?;
        self.stage_decision(
            input.world_seed,
            input.posting,
            RecruitmentStage::Interview,
            components,
            Some(fit.dimension_fit_bp),
            score_bp,
        )
    }

    fn evaluate_invitation(
        &self,
        input: InvitationEvaluationInput<'_>,
    ) -> Result<InvitationDecision, RecruitmentError> {
        validate_candidate_access(input.posting, input.invitation_game_day, input.candidate)?;
        validate_qualifications(input.posting, input.candidate)?;
        if input.open_invitation_count >= self.ruleset.open_invitation_limit
            || input.platform_invitation_already_generated_today
        {
            return Err(RecruitmentError::InvitationLimit);
        }
        let artifact = input.latest_public_artifact;
        validate_artifact(artifact)?;
        if !artifact.belongs_to_current_run {
            return Err(RecruitmentError::ArtifactNotOwned(
                artifact.artifact_version_id,
            ));
        }
        if !artifact.is_public {
            return Err(RecruitmentError::ArtifactNotPublic(
                artifact.artifact_version_id,
            ));
        }

        let score_bp = match invitation_source(input.posting.platform) {
            InvitationSource::Resume if artifact.kind == ArtifactKind::Resume => {
                artifact.completeness_bp
            }
            InvitationSource::LinkedinProfile
                if artifact.kind == ArtifactKind::LinkedinProfile
                    && artifact.open_to_work
                    && artifact.industries.contains(&input.posting.industry) =>
            {
                validate_component(input.visible_scores.language)?;
                validate_component(input.visible_scores.experience)?;
                let weighted = i128::from(artifact.completeness_bp)
                    .checked_mul(i128::from(
                        self.ruleset.linkedin_invitation_weights.completeness_bp,
                    ))
                    .and_then(|value| {
                        i128::from(input.visible_scores.language)
                            .checked_mul(i128::from(
                                self.ruleset.linkedin_invitation_weights.language_score_bp,
                            ))
                            .and_then(|language| value.checked_add(language))
                    })
                    .and_then(|value| {
                        i128::from(input.visible_scores.experience)
                            .checked_mul(i128::from(
                                self.ruleset.linkedin_invitation_weights.experience_score_bp,
                            ))
                            .and_then(|experience| value.checked_add(experience))
                    })
                    .ok_or(RecruitmentError::ArithmeticOverflow)?;
                i64::try_from(weighted / i128::from(SCORE_SCALE_BP))
                    .map_err(|_| RecruitmentError::ArithmeticOverflow)?
            }
            InvitationSource::None => return Err(RecruitmentError::InvitationUnsupported),
            InvitationSource::Resume | InvitationSource::LinkedinProfile => {
                return Err(RecruitmentError::InvitationProfileIneligible);
            }
        };
        validate_component(score_bp)?;
        let score_band = score_band(score_bp, self.ruleset.score_bands)?;
        let probability_ppm = self
            .ruleset
            .pass_probabilities
            .invitation
            .for_competition(input.posting.competition_band)
            .for_score_band(score_band);
        let roll_ppm = invitation_roll(
            input.world_seed,
            &input.posting.posting_key,
            input.posting.platform,
            input.invitation_game_day,
        )?;
        let pin = pin_application_artifacts(&[artifact.kind], std::slice::from_ref(artifact))?;

        Ok(InvitationDecision {
            decision: StageDecision {
                stage: RecruitmentStage::Invitation,
                score_band,
                components: StageComponents {
                    primary_fit_bp: score_bp,
                    supporting_fit_bp: 0,
                    context_fit_bp: 0,
                },
                dimension_fit_bp: None,
                score_bp,
                probability_ppm,
                roll_ppm,
                passed: roll_ppm < probability_ppm,
            },
            pin,
        })
    }

    fn initial_application_state(
        &self,
        posting: &MaterializedPosting,
        submitted_game_day: u32,
    ) -> Result<ApplicationState, RecruitmentError> {
        let document_decision_game_day = submitted_game_day
            .checked_add(posting.document_review_days)
            .ok_or(RecruitmentError::ArithmeticOverflow)?;
        Ok(ApplicationState::Submitted {
            submitted_game_day,
            document_decision_game_day,
            interview_delay_days: posting.interview_delay_days,
            offer_expiry_days: posting.offer_expiry_days,
        })
    }

    fn transition_application(
        &self,
        state: &ApplicationState,
        action: ApplicationAction,
    ) -> Result<ApplicationState, RecruitmentError> {
        transition_application(state, action)
    }

    fn determine_offer_salary(
        &self,
        input: OfferSalaryInput<'_>,
    ) -> Result<OfferSalary, RecruitmentError> {
        validate_component(input.possessed_fit_bp)?;
        let step_count = salary_step_count(input.posting)?;
        let band = score_band(input.possessed_fit_bp, self.ruleset.score_bands)?;
        let first_third = step_count / 3;
        let second_third = step_count
            .checked_mul(2)
            .ok_or(RecruitmentError::ArithmeticOverflow)?
            / 3;
        let (start_index, end_exclusive_index) = match band {
            ScoreBand::Low => (0, first_third),
            ScoreBand::Medium => (first_third, second_third),
            ScoreBand::High => (second_third, step_count),
        };
        let length = end_exclusive_index
            .checked_sub(start_index)
            .ok_or(RecruitmentError::ArithmeticOverflow)?;
        if length == 0 {
            return Err(RecruitmentError::InvalidTemplate(
                input.posting.template_key.clone(),
            ));
        }
        let salary_roll_word = stage_word(
            input.world_seed,
            &input.posting.posting_key,
            "offerSalary",
            0,
        )?;
        let offset = scaled_word(salary_roll_word, length)?;
        let salary_step_index = start_index
            .checked_add(offset)
            .ok_or(RecruitmentError::ArithmeticOverflow)?;
        let annual_salary_krw = i64::try_from(
            i128::from(input.posting.minimum_annual_salary_krw)
                .checked_add(
                    i128::from(input.posting.salary_step_krw)
                        .checked_mul(i128::from(salary_step_index))
                        .ok_or(RecruitmentError::ArithmeticOverflow)?,
                )
                .ok_or(RecruitmentError::ArithmeticOverflow)?,
        )
        .map_err(|_| RecruitmentError::ArithmeticOverflow)?;

        Ok(OfferSalary {
            annual_salary_krw,
            salary_step_index,
            salary_roll_word,
            possessed_fit_bp: input.possessed_fit_bp,
        })
    }

    fn plan_offer_acceptance(
        &self,
        input: OfferAcceptanceInput<'_>,
    ) -> Result<OfferAcceptancePlan, RecruitmentError> {
        self.validate_contracts(input.contracts)?;
        if input.contracts.iter().any(|contract| {
            matches!(
                contract.status,
                EmploymentContractStatus::PendingStart | EmploymentContractStatus::Active
            )
        }) {
            return Err(RecruitmentError::ActiveEmployment);
        }
        if input.other_accepted_offer_count > 0 {
            return Err(RecruitmentError::AlreadyAcceptedOffer);
        }
        let (expires_exclusive_game_day, entry_decision, interview_decision, salary) =
            match input.state {
                ApplicationState::Offered {
                    expires_exclusive_game_day,
                    entry_decision,
                    interview_decision,
                    salary,
                    ..
                } => (
                    *expires_exclusive_game_day,
                    entry_decision.clone(),
                    interview_decision.clone(),
                    *salary,
                ),
                _ => return Err(RecruitmentError::InvalidApplicationState),
            };
        if input.accepted_game_day >= expires_exclusive_game_day {
            return Err(RecruitmentError::OfferExpired);
        }
        let close_application_ids =
            canonical_close_ids(input.other_open_application_ids, Some(input.application_id))?;
        let close_invitation_ids = canonical_close_ids(input.open_invitation_ids, None)?;
        let start_game_day = expires_exclusive_game_day
            .checked_add(self.ruleset.start_delay_days)
            .ok_or(RecruitmentError::ArithmeticOverflow)?;

        Ok(OfferAcceptancePlan {
            accepted_state: ApplicationState::Accepted {
                accepted_game_day: input.accepted_game_day,
                entry_decision,
                interview_decision,
                salary,
            },
            contract: EmploymentContractDraft {
                application_id: input.application_id,
                posting_key: input.posting.posting_key.clone(),
                status: EmploymentContractStatus::PendingStart,
                annual_salary_krw: salary.annual_salary_krw,
                start_game_day,
                monthly_payday: self.ruleset.monthly_payday,
                career_catalog_bundle_key: input.posting.career_catalog_bundle_key.clone(),
                recruitment_ruleset_key: input.posting.recruitment_ruleset_key.clone(),
                first_pay_reward_krw: input.posting.first_pay_reward_krw,
            },
            close_application_ids,
            close_invitation_ids,
        })
    }

    fn validate_contracts(
        &self,
        contracts: &[EmploymentContractSummary],
    ) -> Result<(), RecruitmentError> {
        let mut ids = HashSet::with_capacity(contracts.len());
        let mut current_count = 0_u32;
        for contract in contracts {
            if !ids.insert(contract.contract_id) {
                return Err(RecruitmentError::DuplicateContractId(contract.contract_id));
            }
            if matches!(
                contract.status,
                EmploymentContractStatus::PendingStart | EmploymentContractStatus::Active
            ) {
                current_count = current_count
                    .checked_add(1)
                    .ok_or(RecruitmentError::ArithmeticOverflow)?;
            }
        }
        if current_count > 1 {
            return Err(RecruitmentError::MultipleActiveContracts);
        }
        Ok(())
    }
}

impl V1RecruitmentRules {
    fn stage_decision(
        &self,
        world_seed: u64,
        posting: &MaterializedPosting,
        stage: RecruitmentStage,
        components: StageComponents,
        dimension_fit_bp: Option<DimensionScores>,
        score_bp: i64,
    ) -> Result<StageDecision, RecruitmentError> {
        let score_band = score_band(score_bp, self.ruleset.score_bands)?;
        let probabilities = match stage {
            RecruitmentStage::Document => self.ruleset.pass_probabilities.document,
            RecruitmentStage::Interview => self.ruleset.pass_probabilities.interview,
            RecruitmentStage::Invitation => self.ruleset.pass_probabilities.invitation,
        };
        let probability_ppm = probabilities
            .for_competition(posting.competition_band)
            .for_score_band(score_band);
        let stage_key = match stage {
            RecruitmentStage::Document => "document",
            RecruitmentStage::Interview => "interview",
            RecruitmentStage::Invitation => "invitation",
        };
        let roll_ppm = u32::try_from(
            stage_word(world_seed, &posting.posting_key, stage_key, 0)?
                % u64::from(PROBABILITY_SCALE_PPM),
        )
        .map_err(|_| RecruitmentError::ArithmeticOverflow)?;

        Ok(StageDecision {
            stage,
            score_band,
            components,
            dimension_fit_bp,
            score_bp,
            probability_ppm,
            roll_ppm,
            passed: roll_ppm < probability_ppm,
        })
    }
}

fn validate_ruleset(ruleset: &RecruitmentRuleset) -> Result<(), RecruitmentError> {
    validate_ascii_key(&ruleset.ruleset_key)?;
    validate_weights(ruleset.document_weights)?;
    validate_weights(ruleset.interview_weights)?;
    let invitation_weights = ruleset.linkedin_invitation_weights;
    for value in [
        invitation_weights.completeness_bp,
        invitation_weights.language_score_bp,
        invitation_weights.experience_score_bp,
    ] {
        validate_component(value)?;
    }
    let invitation_total = invitation_weights
        .completeness_bp
        .checked_add(invitation_weights.language_score_bp)
        .and_then(|value| value.checked_add(invitation_weights.experience_score_bp))
        .ok_or(RecruitmentError::ArithmeticOverflow)?;
    if invitation_total != SCORE_SCALE_BP
        || !(1..SCORE_SCALE_BP).contains(&ruleset.score_bands.medium_minimum_bp)
        || !(ruleset.score_bands.medium_minimum_bp..=SCORE_SCALE_BP)
            .contains(&ruleset.score_bands.high_minimum_bp)
        || ruleset.score_bands.medium_minimum_bp == ruleset.score_bands.high_minimum_bp
        || ruleset.start_delay_days == 0
        || !(1..=31).contains(&ruleset.monthly_payday)
        || ruleset.active_application_limit == 0
        || ruleset.direct_application_daily_limit == 0
        || ruleset.open_invitation_limit == 0
    {
        return Err(RecruitmentError::InvalidRuleset);
    }
    validate_probability_table(ruleset.pass_probabilities)
}

fn validate_probability_table(table: PassProbabilityTable) -> Result<(), RecruitmentError> {
    for stage in [table.document, table.interview, table.invitation] {
        for competition in [stage.low, stage.medium, stage.high] {
            for probability in [
                competition.low_score_ppm,
                competition.medium_score_ppm,
                competition.high_score_ppm,
            ] {
                if probability > PROBABILITY_SCALE_PPM {
                    return Err(RecruitmentError::InvalidRuleset);
                }
            }
        }
    }
    Ok(())
}

fn validate_weights(weights: ComponentWeights) -> Result<(), RecruitmentError> {
    for value in [
        weights.primary_fit_bp,
        weights.supporting_fit_bp,
        weights.context_fit_bp,
    ] {
        validate_component(value)?;
    }
    let total = weights
        .primary_fit_bp
        .checked_add(weights.supporting_fit_bp)
        .and_then(|value| value.checked_add(weights.context_fit_bp))
        .ok_or(RecruitmentError::ArithmeticOverflow)?;
    if total != SCORE_SCALE_BP {
        return Err(RecruitmentError::InvalidRuleset);
    }
    Ok(())
}

fn validate_component(value: i64) -> Result<(), RecruitmentError> {
    if !(0..=SCORE_SCALE_BP).contains(&value) {
        return Err(RecruitmentError::InvalidRuleset);
    }
    Ok(())
}

fn validate_seed(seed: PostingSeedInput<'_>) -> Result<(), RecruitmentError> {
    validate_ascii_key(seed.world_model_version)?;
    validate_ascii_key(seed.career_catalog_bundle_key)
}

fn validate_ascii_key(key: &str) -> Result<(), RecruitmentError> {
    if key.is_empty() || !key.is_ascii() || key.len() > u32::MAX as usize {
        return Err(RecruitmentError::InvalidStableKey);
    }
    Ok(())
}

fn validate_platform(platform: &PlatformDefinition) -> Result<(), RecruitmentError> {
    if platform.daily_slot_count == 0
        || platform.document_review_days == 0
        || platform.first_pay_reward_krw < 0
    {
        return Err(RecruitmentError::InvalidPlatform(platform.platform));
    }
    let expected_same_region = platform.platform == PlatformKey::Sarangbang;
    let expected_source = invitation_source(platform.platform);
    let expected_reward = platform.platform == PlatformKey::Wanted;
    if platform.same_region_only != expected_same_region
        || platform.invitation_source != expected_source
        || (expected_reward && platform.first_pay_reward_krw == 0)
        || (!expected_reward && platform.first_pay_reward_krw != 0)
    {
        return Err(RecruitmentError::InvalidPlatform(platform.platform));
    }

    let mut actual = platform.required_artifacts.clone();
    actual.sort_by_key(|kind| artifact_kind_rank(*kind));
    if actual.windows(2).any(|pair| pair[0] == pair[1])
        || actual != expected_platform_artifacts(platform.platform)
    {
        return Err(RecruitmentError::InvalidPlatform(platform.platform));
    }
    Ok(())
}

fn expected_platform_artifacts(platform: PlatformKey) -> Vec<ArtifactKind> {
    match platform {
        PlatformKey::Wanted => vec![ArtifactKind::Portfolio, ArtifactKind::Resume],
        PlatformKey::Linkedin => vec![ArtifactKind::LinkedinProfile],
        PlatformKey::Sarangbang
        | PlatformKey::Jobkorea
        | PlatformKey::Saramin
        | PlatformKey::Work24 => vec![ArtifactKind::Resume],
    }
}

const fn invitation_source(platform: PlatformKey) -> InvitationSource {
    match platform {
        PlatformKey::Saramin => InvitationSource::Resume,
        PlatformKey::Linkedin => InvitationSource::LinkedinProfile,
        PlatformKey::Sarangbang
        | PlatformKey::Jobkorea
        | PlatformKey::Wanted
        | PlatformKey::Work24 => InvitationSource::None,
    }
}

const fn artifact_kind_rank(kind: ArtifactKind) -> u8 {
    match kind {
        ArtifactKind::Portfolio => 0,
        ArtifactKind::Resume => 1,
        ArtifactKind::LinkedinProfile => 2,
    }
}

fn validate_industry_weights(
    input: &[PlatformIndustryWeight],
) -> Result<HashMap<Industry, u32>, RecruitmentError> {
    let mut weights = HashMap::with_capacity(input.len());
    for item in input {
        if weights.insert(item.industry, item.weight_bp).is_some() {
            return Err(RecruitmentError::InvalidIndustryWeights);
        }
    }
    if weights.is_empty() {
        return Err(RecruitmentError::InvalidIndustryWeights);
    }
    Ok(weights)
}

fn validate_template(
    template: &JobTemplate,
    score_rules: &Arc<dyn SpecScoreRules>,
) -> Result<(), RecruitmentError> {
    validate_ascii_key(&template.template_key)
        .map_err(|_| RecruitmentError::InvalidTemplate(template.template_key.clone()))?;
    validate_ascii_key(&template.employer_key)
        .map_err(|_| RecruitmentError::InvalidTemplate(template.template_key.clone()))?;
    validate_ascii_key(&template.job_family_key)
        .map_err(|_| RecruitmentError::InvalidTemplate(template.template_key.clone()))?;
    if template.employer_name.trim().is_empty()
        || template.interview_delay_days == 0
        || template.offer_expiry_days == 0
        || template.posting_open_days == 0
        || template.minimum_annual_salary_krw <= 0
        || template.maximum_annual_salary_krw < template.minimum_annual_salary_krw
        || template.salary_step_krw <= 0
        || template.minimum_annual_salary_krw % template.salary_step_krw != 0
        || template.maximum_annual_salary_krw % template.salary_step_krw != 0
    {
        return Err(RecruitmentError::InvalidTemplate(
            template.template_key.clone(),
        ));
    }
    if template
        .required_certification_entry_key
        .as_deref()
        .is_some_and(|key| validate_ascii_key(key).is_err())
    {
        return Err(RecruitmentError::InvalidTemplate(
            template.template_key.clone(),
        ));
    }
    score_rules
        .calculate_fit(ScoreFitInput {
            candidate_scores: DimensionScores::default(),
            requirements: &template.requirements,
        })
        .map_err(RecruitmentError::Score)?;
    let step_count = ((template.maximum_annual_salary_krw - template.minimum_annual_salary_krw)
        / template.salary_step_krw)
        .checked_add(1)
        .ok_or(RecruitmentError::ArithmeticOverflow)?;
    if step_count < 3 {
        return Err(RecruitmentError::InvalidTemplate(
            template.template_key.clone(),
        ));
    }
    Ok(())
}

fn validate_candidate_access(
    posting: &MaterializedPosting,
    current_game_day: u32,
    candidate: CandidateApplicationProfile<'_>,
) -> Result<(), RecruitmentError> {
    if candidate.has_active_or_pending_contract || candidate.life_status == LifeStatus::Employed {
        return Err(RecruitmentError::ActiveEmployment);
    }
    if candidate.life_status != LifeStatus::Unemployed
        || candidate.military_qualification == MilitaryQualification::Serving
    {
        return Err(RecruitmentError::ServiceConflict);
    }
    if current_game_day < posting.posted_game_day
        || current_game_day >= posting.closes_exclusive_game_day
    {
        return Err(RecruitmentError::PostingClosed);
    }
    if posting.same_region_only && candidate.region != posting.region {
        return Err(RecruitmentError::RegionMismatch);
    }
    Ok(())
}

fn validate_qualifications(
    posting: &MaterializedPosting,
    candidate: CandidateApplicationProfile<'_>,
) -> Result<(), RecruitmentError> {
    if posting
        .minimum_education
        .is_some_and(|minimum| candidate.education < minimum)
    {
        return Err(RecruitmentError::EducationRequired);
    }
    let mut catalog_keys = HashSet::with_capacity(candidate.valid_catalog_entry_keys.len());
    for key in candidate.valid_catalog_entry_keys {
        if !catalog_keys.insert(*key) {
            return Err(RecruitmentError::InvalidStableKey);
        }
    }
    if posting
        .required_certification_entry_key
        .as_deref()
        .is_some_and(|required| !catalog_keys.contains(required))
    {
        return Err(RecruitmentError::CertificationRequired);
    }
    if candidate.experience_days < posting.minimum_experience_days {
        return Err(RecruitmentError::ExperienceRequired);
    }
    if posting.military_requirement == MilitaryPostingRequirement::CompletedOrExempt
        && candidate.military_qualification != MilitaryQualification::CompletedOrExempt
    {
        return Err(RecruitmentError::MilitaryRequirementNotMet);
    }
    Ok(())
}

fn validate_artifact(artifact: &SubmittedArtifact) -> Result<(), RecruitmentError> {
    validate_component(artifact.completeness_bp)
        .map_err(|_| RecruitmentError::InvalidArtifact(artifact.artifact_version_id))?;
    let mut evidence = HashSet::with_capacity(artifact.evidence_ids.len());
    if artifact
        .evidence_ids
        .iter()
        .any(|evidence_id| !evidence.insert(*evidence_id))
    {
        return Err(RecruitmentError::InvalidArtifact(
            artifact.artifact_version_id,
        ));
    }
    let mut industries = HashSet::with_capacity(artifact.industries.len());
    if artifact
        .industries
        .iter()
        .any(|industry| !industries.insert(*industry))
    {
        return Err(RecruitmentError::InvalidArtifact(
            artifact.artifact_version_id,
        ));
    }
    Ok(())
}

fn pin_application_artifacts(
    required: &[ArtifactKind],
    artifacts: &[SubmittedArtifact],
) -> Result<ApplicationPin, RecruitmentError> {
    let mut required_set = HashSet::with_capacity(required.len());
    for kind in required {
        if !required_set.insert(*kind) {
            return Err(RecruitmentError::DuplicateArtifactKind(*kind));
        }
    }
    let mut by_kind = HashMap::with_capacity(artifacts.len());
    for artifact in artifacts {
        validate_artifact(artifact)?;
        if !required_set.contains(&artifact.kind) {
            return Err(RecruitmentError::UnexpectedArtifact(artifact.kind));
        }
        if by_kind.insert(artifact.kind, artifact).is_some() {
            return Err(RecruitmentError::DuplicateArtifactKind(artifact.kind));
        }
        if !artifact.belongs_to_current_run {
            return Err(RecruitmentError::ArtifactNotOwned(
                artifact.artifact_version_id,
            ));
        }
    }
    for kind in required {
        if !by_kind.contains_key(kind) {
            return Err(RecruitmentError::ArtifactRequired(*kind));
        }
    }
    let mut ordered = by_kind.into_values().collect::<Vec<_>>();
    ordered.sort_by_key(|artifact| artifact_kind_rank(artifact.kind));
    let completeness_total = ordered.iter().try_fold(0_i128, |total, artifact| {
        total
            .checked_add(i128::from(artifact.completeness_bp))
            .ok_or(RecruitmentError::ArithmeticOverflow)
    })?;
    let artifact_count =
        i128::try_from(ordered.len()).map_err(|_| RecruitmentError::ArithmeticOverflow)?;
    if artifact_count == 0 {
        return Err(RecruitmentError::InvalidPlatform(PlatformKey::Jobkorea));
    }
    let artifact_completeness_bp = i64::try_from(completeness_total / artifact_count)
        .map_err(|_| RecruitmentError::ArithmeticOverflow)?;
    let mut visible_evidence_ids = ordered
        .iter()
        .flat_map(|artifact| artifact.evidence_ids.iter().copied())
        .collect::<Vec<_>>();
    visible_evidence_ids.sort_unstable();
    visible_evidence_ids.dedup();
    let artifacts = ordered
        .into_iter()
        .map(|artifact| PinnedArtifactVersion {
            artifact_version_id: artifact.artifact_version_id,
            kind: artifact.kind,
            completeness_bp: artifact.completeness_bp,
        })
        .collect();

    Ok(ApplicationPin {
        artifacts,
        visible_evidence_ids,
        artifact_completeness_bp,
    })
}

fn weighted_score(
    components: StageComponents,
    weights: ComponentWeights,
) -> Result<i64, RecruitmentError> {
    for component in [
        components.primary_fit_bp,
        components.supporting_fit_bp,
        components.context_fit_bp,
    ] {
        validate_component(component)?;
    }
    let total = i128::from(components.primary_fit_bp)
        .checked_mul(i128::from(weights.primary_fit_bp))
        .and_then(|value| {
            i128::from(components.supporting_fit_bp)
                .checked_mul(i128::from(weights.supporting_fit_bp))
                .and_then(|supporting| value.checked_add(supporting))
        })
        .and_then(|value| {
            i128::from(components.context_fit_bp)
                .checked_mul(i128::from(weights.context_fit_bp))
                .and_then(|context| value.checked_add(context))
        })
        .ok_or(RecruitmentError::ArithmeticOverflow)?;
    i64::try_from(total / i128::from(SCORE_SCALE_BP))
        .map_err(|_| RecruitmentError::ArithmeticOverflow)
}

fn profile_consistency(pinned: &[u64], currently_valid: &[u64]) -> Result<i64, RecruitmentError> {
    let pinned = pinned.iter().copied().collect::<HashSet<_>>();
    if pinned.is_empty() {
        return Ok(SCORE_SCALE_BP);
    }
    let currently_valid = currently_valid.iter().copied().collect::<HashSet<_>>();
    let valid_count = pinned.intersection(&currently_valid).count();
    let numerator = i128::try_from(valid_count)
        .map_err(|_| RecruitmentError::ArithmeticOverflow)?
        .checked_mul(i128::from(SCORE_SCALE_BP))
        .ok_or(RecruitmentError::ArithmeticOverflow)?;
    let denominator =
        i128::try_from(pinned.len()).map_err(|_| RecruitmentError::ArithmeticOverflow)?;
    i64::try_from(numerator / denominator).map_err(|_| RecruitmentError::ArithmeticOverflow)
}

fn score_band(
    score_bp: i64,
    boundaries: ScoreBandBoundaries,
) -> Result<ScoreBand, RecruitmentError> {
    validate_component(score_bp)?;
    if score_bp < boundaries.medium_minimum_bp {
        Ok(ScoreBand::Low)
    } else if score_bp < boundaries.high_minimum_bp {
        Ok(ScoreBand::Medium)
    } else {
        Ok(ScoreBand::High)
    }
}

fn salary_step_count(posting: &MaterializedPosting) -> Result<u64, RecruitmentError> {
    if posting.minimum_annual_salary_krw <= 0
        || posting.maximum_annual_salary_krw < posting.minimum_annual_salary_krw
        || posting.salary_step_krw <= 0
        || posting.minimum_annual_salary_krw % posting.salary_step_krw != 0
        || posting.maximum_annual_salary_krw % posting.salary_step_krw != 0
    {
        return Err(RecruitmentError::InvalidTemplate(
            posting.template_key.clone(),
        ));
    }
    let count = (posting.maximum_annual_salary_krw - posting.minimum_annual_salary_krw)
        .checked_div(posting.salary_step_krw)
        .and_then(|value| value.checked_add(1))
        .ok_or(RecruitmentError::ArithmeticOverflow)?;
    let count = u64::try_from(count).map_err(|_| RecruitmentError::ArithmeticOverflow)?;
    if count < 3 {
        return Err(RecruitmentError::InvalidTemplate(
            posting.template_key.clone(),
        ));
    }
    Ok(count)
}

fn posting_digest(
    seed: PostingSeedInput<'_>,
    ruleset_key: &str,
    platform: PlatformKey,
    counter: u32,
) -> Result<[u8; 32], RecruitmentError> {
    let mut message = Vec::with_capacity(192);
    message.extend_from_slice(POSTING_DOMAIN);
    push_string(&mut message, seed.world_model_version)?;
    push_string(&mut message, seed.career_catalog_bundle_key)?;
    push_string(&mut message, ruleset_key)?;
    message.extend_from_slice(&seed.game_day.to_be_bytes());
    push_string(&mut message, platform.as_key())?;
    message.extend_from_slice(&seed.slot_no.to_be_bytes());
    message.extend_from_slice(&counter.to_be_bytes());
    Ok(hmac_sha256(&seed.world_seed.to_be_bytes(), &message))
}

fn stage_word(
    world_seed: u64,
    posting_key: &str,
    stage: &str,
    counter: u32,
) -> Result<u64, RecruitmentError> {
    validate_ascii_key(posting_key)?;
    validate_ascii_key(stage)?;
    let mut message = Vec::with_capacity(128);
    message.extend_from_slice(STAGE_DOMAIN);
    push_string(&mut message, posting_key)?;
    push_string(&mut message, stage)?;
    message.extend_from_slice(&APPLICATION_ORDINAL.to_be_bytes());
    message.extend_from_slice(&counter.to_be_bytes());
    Ok(first_u64(hmac_sha256(&world_seed.to_be_bytes(), &message)))
}

fn invitation_roll(
    world_seed: u64,
    posting_key: &str,
    platform: PlatformKey,
    invitation_game_day: u32,
) -> Result<u32, RecruitmentError> {
    validate_ascii_key(posting_key)?;
    let mut message = Vec::with_capacity(128);
    message.extend_from_slice(INVITATION_DOMAIN);
    push_string(&mut message, posting_key)?;
    push_string(&mut message, platform.as_key())?;
    message.extend_from_slice(&invitation_game_day.to_be_bytes());
    message.extend_from_slice(&0_u32.to_be_bytes());
    let word = first_u64(hmac_sha256(&world_seed.to_be_bytes(), &message));
    u32::try_from(word % u64::from(PROBABILITY_SCALE_PPM))
        .map_err(|_| RecruitmentError::ArithmeticOverflow)
}

fn push_string(message: &mut Vec<u8>, value: &str) -> Result<(), RecruitmentError> {
    let length = u32::try_from(value.len()).map_err(|_| RecruitmentError::InvalidStableKey)?;
    message.extend_from_slice(&length.to_be_bytes());
    message.extend_from_slice(value.as_bytes());
    Ok(())
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut normalized_key = [0_u8; HMAC_BLOCK_BYTES];
    if key.len() > HMAC_BLOCK_BYTES {
        let digest = Sha256::digest(key);
        normalized_key[..digest.len()].copy_from_slice(&digest);
    } else {
        normalized_key[..key.len()].copy_from_slice(key);
    }
    let mut inner_pad = [0x36_u8; HMAC_BLOCK_BYTES];
    let mut outer_pad = [0x5c_u8; HMAC_BLOCK_BYTES];
    for index in 0..HMAC_BLOCK_BYTES {
        inner_pad[index] ^= normalized_key[index];
        outer_pad[index] ^= normalized_key[index];
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(message);
    let inner_digest = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner_digest);
    outer.finalize().into()
}

fn first_u64(digest: [u8; 32]) -> u64 {
    u64::from_be_bytes([
        digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
    ])
}

fn scaled_word(word: u64, exclusive_upper: u64) -> Result<u64, RecruitmentError> {
    if exclusive_upper == 0 {
        return Err(RecruitmentError::ArithmeticOverflow);
    }
    let scaled = u128::from(word)
        .checked_mul(u128::from(exclusive_upper))
        .ok_or(RecruitmentError::ArithmeticOverflow)?
        >> 64;
    u64::try_from(scaled).map_err(|_| RecruitmentError::ArithmeticOverflow)
}

fn lowercase_hex(bytes: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn transition_application(
    state: &ApplicationState,
    action: ApplicationAction,
) -> Result<ApplicationState, RecruitmentError> {
    match (state, action) {
        (
            ApplicationState::Submitted {
                document_decision_game_day,
                interview_delay_days,
                offer_expiry_days,
                ..
            },
            ApplicationAction::ResolveDocument { game_day, decision },
        ) => {
            if game_day != *document_decision_game_day {
                return Err(RecruitmentError::DecisionNotDue);
            }
            validate_stage_decision(&decision, RecruitmentStage::Document)?;
            if !decision.passed {
                return Ok(ApplicationState::DocumentRejected {
                    decided_game_day: game_day,
                    decision,
                });
            }
            let interview_game_day = game_day
                .checked_add(*interview_delay_days)
                .ok_or(RecruitmentError::ArithmeticOverflow)?;
            Ok(ApplicationState::InterviewAwaitingConfirmation {
                entry_decision: decision,
                confirmation_deadline_exclusive_game_day: interview_game_day,
                interview_game_day,
                offer_expiry_days: *offer_expiry_days,
            })
        }
        (
            ApplicationState::InterviewAwaitingConfirmation {
                entry_decision,
                confirmation_deadline_exclusive_game_day,
                interview_game_day,
                offer_expiry_days,
            },
            ApplicationAction::ConfirmInterview { game_day },
        ) => {
            if game_day >= *confirmation_deadline_exclusive_game_day {
                return Err(RecruitmentError::InterviewExpired);
            }
            Ok(ApplicationState::InterviewConfirmed {
                entry_decision: entry_decision.clone(),
                confirmed_game_day: game_day,
                interview_game_day: *interview_game_day,
                offer_expiry_days: *offer_expiry_days,
            })
        }
        (
            ApplicationState::InterviewAwaitingConfirmation {
                confirmation_deadline_exclusive_game_day,
                ..
            },
            ApplicationAction::DeclineInterview { game_day },
        ) => {
            if game_day >= *confirmation_deadline_exclusive_game_day {
                return Err(RecruitmentError::InterviewExpired);
            }
            Ok(ApplicationState::Withdrawn {
                withdrawn_game_day: game_day,
            })
        }
        (
            ApplicationState::InterviewAwaitingConfirmation {
                confirmation_deadline_exclusive_game_day,
                ..
            },
            ApplicationAction::ExpireConfirmation { game_day },
        ) => {
            if game_day != *confirmation_deadline_exclusive_game_day {
                return Err(RecruitmentError::DecisionNotDue);
            }
            Ok(ApplicationState::Withdrawn {
                withdrawn_game_day: game_day,
            })
        }
        (ApplicationState::Submitted { .. }, ApplicationAction::Withdraw { game_day }) => {
            Ok(ApplicationState::Withdrawn {
                withdrawn_game_day: game_day,
            })
        }
        (
            ApplicationState::InterviewAwaitingConfirmation {
                confirmation_deadline_exclusive_game_day,
                ..
            },
            ApplicationAction::Withdraw { game_day },
        ) => {
            if game_day >= *confirmation_deadline_exclusive_game_day {
                return Err(RecruitmentError::InterviewExpired);
            }
            Ok(ApplicationState::Withdrawn {
                withdrawn_game_day: game_day,
            })
        }
        (
            ApplicationState::InterviewConfirmed {
                interview_game_day, ..
            },
            ApplicationAction::Withdraw { game_day },
        ) => {
            if game_day >= *interview_game_day {
                return Err(RecruitmentError::InterviewExpired);
            }
            Ok(ApplicationState::Withdrawn {
                withdrawn_game_day: game_day,
            })
        }
        (
            ApplicationState::InterviewConfirmed {
                entry_decision,
                interview_game_day,
                offer_expiry_days,
                ..
            },
            ApplicationAction::ResolveInterview {
                game_day,
                decision,
                salary,
            },
        ) => {
            if game_day != *interview_game_day {
                return Err(RecruitmentError::DecisionNotDue);
            }
            validate_stage_decision(&decision, RecruitmentStage::Interview)?;
            match (decision.passed, salary) {
                (false, None) => Ok(ApplicationState::InterviewRejected {
                    decided_game_day: game_day,
                    entry_decision: entry_decision.clone(),
                    interview_decision: decision,
                }),
                (true, Some(salary)) => {
                    let expires_exclusive_game_day = game_day
                        .checked_add(*offer_expiry_days)
                        .ok_or(RecruitmentError::ArithmeticOverflow)?;
                    Ok(ApplicationState::Offered {
                        offered_game_day: game_day,
                        expires_exclusive_game_day,
                        entry_decision: entry_decision.clone(),
                        interview_decision: decision,
                        salary,
                    })
                }
                (true, None) => Err(RecruitmentError::OfferSalaryRequired),
                (false, Some(_)) => Err(RecruitmentError::OfferSalaryUnexpected),
            }
        }
        (
            ApplicationState::Offered {
                expires_exclusive_game_day,
                ..
            },
            ApplicationAction::DeclineOffer { game_day },
        ) => {
            if game_day >= *expires_exclusive_game_day {
                return Err(RecruitmentError::OfferExpired);
            }
            Ok(ApplicationState::Declined {
                declined_game_day: game_day,
            })
        }
        (
            ApplicationState::Offered {
                expires_exclusive_game_day,
                ..
            },
            ApplicationAction::ExpireOffer { game_day },
        ) => {
            if game_day != *expires_exclusive_game_day {
                return Err(RecruitmentError::DecisionNotDue);
            }
            Ok(ApplicationState::Expired {
                expired_game_day: game_day,
            })
        }
        (
            ApplicationState::Submitted { .. }
            | ApplicationState::InterviewAwaitingConfirmation { .. }
            | ApplicationState::InterviewConfirmed { .. }
            | ApplicationState::Offered { .. },
            ApplicationAction::Close { game_day },
        ) => Ok(ApplicationState::Closed {
            closed_game_day: game_day,
        }),
        _ => Err(RecruitmentError::InvalidApplicationState),
    }
}

fn validate_stage_decision(
    decision: &StageDecision,
    expected_stage: RecruitmentStage,
) -> Result<(), RecruitmentError> {
    if decision.stage != expected_stage
        || decision.roll_ppm >= PROBABILITY_SCALE_PPM
        || decision.probability_ppm > PROBABILITY_SCALE_PPM
        || decision.passed != (decision.roll_ppm < decision.probability_ppm)
    {
        return Err(RecruitmentError::InvalidStageDecision);
    }
    validate_component(decision.score_bp).map_err(|_| RecruitmentError::InvalidStageDecision)
}

fn canonical_close_ids(ids: &[u64], forbidden: Option<u64>) -> Result<Vec<u64>, RecruitmentError> {
    let mut seen = HashSet::with_capacity(ids.len());
    for id in ids {
        if forbidden == Some(*id) || !seen.insert(*id) {
            return Err(RecruitmentError::DuplicateCloseId(*id));
        }
    }
    let mut canonical = ids.to_vec();
    canonical.sort_unstable();
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::career::SpecDimension;

    fn given_requirements() -> Vec<DimensionRequirement> {
        [
            (SpecDimension::Education, 1_800),
            (SpecDimension::Certification, 1_700),
            (SpecDimension::Language, 1_000),
            (SpecDimension::Training, 1_000),
            (SpecDimension::Experience, 2_800),
            (SpecDimension::Project, 1_700),
        ]
        .map(|(dimension, weight_bp)| DimensionRequirement {
            dimension,
            required_score_bp: 5_000,
            weight_bp,
        })
        .to_vec()
    }

    fn given_platform(platform: PlatformKey) -> PlatformDefinition {
        PlatformDefinition {
            platform,
            daily_slot_count: 3,
            competition_band: CompetitionBand::High,
            document_review_days: 3,
            same_region_only: platform == PlatformKey::Sarangbang,
            invitation_source: invitation_source(platform),
            required_artifacts: expected_platform_artifacts(platform),
            first_pay_reward_krw: if platform == PlatformKey::Wanted {
                500_000
            } else {
                0
            },
        }
    }

    fn given_template(platform: PlatformKey, key: &str, industry: Industry) -> JobTemplate {
        JobTemplate {
            template_key: key.to_owned(),
            platform,
            employer_key: format!("{key}-employer"),
            employer_name: "가상 고용주".to_owned(),
            industry,
            job_family_key: "softwareEngineering".to_owned(),
            region: Region::CapitalArea,
            employment_type: EmploymentType::Regular,
            minimum_education: Some(Education::HighSchool),
            required_certification_entry_key: None,
            minimum_experience_days: 0,
            military_requirement: MilitaryPostingRequirement::None,
            minimum_annual_salary_krw: 30_000_000,
            maximum_annual_salary_krw: 38_000_000,
            salary_step_krw: 1_000_000,
            interview_delay_days: 5,
            offer_expiry_days: 7,
            posting_open_days: 14,
            requirements: given_requirements(),
        }
    }

    fn given_posting(platform: PlatformKey) -> MaterializedPosting {
        let definition = given_platform(platform);
        let template = given_template(platform, "template-a", Industry::ItSoftware);
        create_v1_recruitment_rules()
            .materialize_posting(PostingMaterializationInput {
                seed: PostingSeedInput {
                    world_model_version: "wm-v1",
                    world_seed: 7,
                    career_catalog_bundle_key: "career-v1",
                    game_day: 10,
                    slot_no: 0,
                },
                platform: &definition,
                industry_weights: &[PlatformIndustryWeight {
                    industry: Industry::ItSoftware,
                    weight_bp: 2_000,
                }],
                templates: &[template],
            })
            .expect("공고를 만들어야 한다")
    }

    fn given_artifact(
        kind: ArtifactKind,
        artifact_version_id: u64,
        completeness_bp: i64,
        evidence_ids: Vec<u64>,
    ) -> SubmittedArtifact {
        SubmittedArtifact {
            artifact_version_id,
            kind,
            belongs_to_current_run: true,
            is_public: true,
            completeness_bp,
            evidence_ids,
            open_to_work: true,
            industries: vec![Industry::ItSoftware],
        }
    }

    fn given_required_artifacts(posting: &MaterializedPosting) -> Vec<SubmittedArtifact> {
        posting
            .required_artifacts
            .iter()
            .enumerate()
            .map(|(index, kind)| {
                let number = u64::try_from(index).expect("작은 인덱스여야 한다");
                given_artifact(*kind, number + 1, 8_000, vec![number + 10])
            })
            .collect()
    }

    fn given_candidate<'a>(keys: &'a [&'a str]) -> CandidateApplicationProfile<'a> {
        CandidateApplicationProfile {
            region: Region::CapitalArea,
            life_status: LifeStatus::Unemployed,
            has_active_or_pending_contract: false,
            education: Education::Bachelor,
            valid_catalog_entry_keys: keys,
            experience_days: 1_000,
            military_qualification: MilitaryQualification::CompletedOrExempt,
        }
    }

    fn given_stage_decision(stage: RecruitmentStage, passed: bool) -> StageDecision {
        let (probability_ppm, roll_ppm) = if passed { (1, 0) } else { (1, 1) };
        StageDecision {
            stage,
            score_band: ScoreBand::Low,
            components: StageComponents {
                primary_fit_bp: 0,
                supporting_fit_bp: 0,
                context_fit_bp: 0,
            },
            dimension_fit_bp: (stage != RecruitmentStage::Invitation)
                .then_some(DimensionScores::default()),
            score_bp: 0,
            probability_ppm,
            roll_ppm,
            passed,
        }
    }

    fn given_salary() -> OfferSalary {
        OfferSalary {
            annual_salary_krw: 32_000_000,
            salary_step_index: 2,
            salary_roll_word: 123,
            possessed_fit_bp: 5_000,
        }
    }

    fn given_offered_state(expires_exclusive_game_day: u32) -> ApplicationState {
        ApplicationState::Offered {
            offered_game_day: 20,
            expires_exclusive_game_day,
            entry_decision: given_stage_decision(RecruitmentStage::Document, true),
            interview_decision: given_stage_decision(RecruitmentStage::Interview, true),
            salary: given_salary(),
        }
    }

    mod context_공고_entropy를_만드는_경우 {
        use super::*;

        #[test]
        fn given_고정된_세계와_슬롯_when_hmac을_계산하면_then_golden_vector와_같다() {
            let seed = PostingSeedInput {
                world_model_version: "wm-v1",
                world_seed: 0x0102_0304_0506_0708,
                career_catalog_bundle_key: "career-v1",
                game_day: 42,
                slot_no: 0,
            };

            let posting_key = lowercase_hex(
                posting_digest(
                    seed,
                    "dev-unranked-m3-recruitment-v1",
                    PlatformKey::Wanted,
                    0,
                )
                .expect("공고 digest를 계산해야 한다"),
            );
            let template_word = first_u64(
                posting_digest(
                    seed,
                    "dev-unranked-m3-recruitment-v1",
                    PlatformKey::Wanted,
                    1,
                )
                .expect("template word를 계산해야 한다"),
            );
            let document_roll = stage_word(seed.world_seed, &posting_key, "document", 0)
                .expect("서류 roll을 계산해야 한다")
                % u64::from(PROBABILITY_SCALE_PPM);
            let invitation =
                invitation_roll(seed.world_seed, &posting_key, PlatformKey::Wanted, 42)
                    .expect("초대 roll을 계산해야 한다");

            assert_eq!(
                posting_key,
                "ac9e869d279d90e3d9af399dc10d08c0641284e3176b8533bc57f80a73dd6e33"
            );
            assert_eq!(template_word, 14_588_933_986_071_012_440);
            assert_eq!(document_roll, 339_215);
            assert_eq!(invitation, 62_292);
        }

        #[test]
        fn given_같은_catalog를_다른_조회순서로_받을때_when_materialize하면_then_같은_template을_고른다()
         {
            let platform = given_platform(PlatformKey::Wanted);
            let first = given_template(PlatformKey::Wanted, "a-template", Industry::ItSoftware);
            let second = given_template(
                PlatformKey::Wanted,
                "b-template",
                Industry::FinanceInsurance,
            );
            let weights = [
                PlatformIndustryWeight {
                    industry: Industry::ItSoftware,
                    weight_bp: 2_000,
                },
                PlatformIndustryWeight {
                    industry: Industry::FinanceInsurance,
                    weight_bp: 1_000,
                },
            ];
            let seed = PostingSeedInput {
                world_model_version: "wm-v1",
                world_seed: 9,
                career_catalog_bundle_key: "career-v1",
                game_day: 10,
                slot_no: 1,
            };
            let rules = create_v1_recruitment_rules();

            let normal = rules
                .materialize_posting(PostingMaterializationInput {
                    seed,
                    platform: &platform,
                    industry_weights: &weights,
                    templates: &[first.clone(), second.clone()],
                })
                .expect("정방향 공고를 만들어야 한다");
            let reversed = rules
                .materialize_posting(PostingMaterializationInput {
                    seed,
                    platform: &platform,
                    industry_weights: &weights,
                    templates: &[second, first],
                })
                .expect("역방향 공고를 만들어야 한다");

            assert_eq!(normal, reversed);
        }

        #[test]
        fn given_daily_slot_count와_같은_slot_when_materialize하면_then_범위밖으로_거절한다() {
            let platform = given_platform(PlatformKey::Wanted);
            let template = given_template(PlatformKey::Wanted, "template", Industry::ItSoftware);

            let result =
                create_v1_recruitment_rules().materialize_posting(PostingMaterializationInput {
                    seed: PostingSeedInput {
                        world_model_version: "wm-v1",
                        world_seed: 1,
                        career_catalog_bundle_key: "career-v1",
                        game_day: 0,
                        slot_no: platform.daily_slot_count,
                    },
                    platform: &platform,
                    industry_weights: &[PlatformIndustryWeight {
                        industry: Industry::ItSoftware,
                        weight_bp: 1,
                    }],
                    templates: &[template],
                });

            assert_eq!(result, Err(RecruitmentError::InvalidSlot));
        }
    }

    mod context_지원_hard_filter를_검사하는_경우 {
        use super::*;

        #[test]
        fn given_여섯_platform의_exact_artifact_when_지원하면_then_모두_통과한다() {
            let rules = create_v1_recruitment_rules();
            let valid_keys: [&str; 0] = [];

            for platform in PlatformKey::ALL {
                let posting = given_posting(platform);
                let artifacts = given_required_artifacts(&posting);
                let result = rules.prepare_application(ApplicationEligibilityInput {
                    posting: &posting,
                    current_game_day: posting.posted_game_day,
                    source: ApplicationSource::Direct,
                    candidate: given_candidate(&valid_keys),
                    submitted_artifacts: &artifacts,
                    active_application_count: 0,
                    direct_applications_today: 0,
                    already_applied_to_posting: false,
                    invitation_decision: None,
                });

                assert!(result.is_ok(), "{} 지원이어야 한다", platform.as_key());
            }
        }

        #[test]
        fn given_원티드_두_artifact_when_pin하면_then_완성도와_visible_evidence를_정규화한다() {
            let posting = given_posting(PlatformKey::Wanted);
            let artifacts = vec![
                given_artifact(ArtifactKind::Resume, 2, 8_001, vec![30, 10]),
                given_artifact(ArtifactKind::Portfolio, 1, 7_000, vec![20, 10]),
            ];
            let valid_keys: [&str; 0] = [];

            let result = create_v1_recruitment_rules()
                .prepare_application(ApplicationEligibilityInput {
                    posting: &posting,
                    current_game_day: posting.posted_game_day,
                    source: ApplicationSource::Direct,
                    candidate: given_candidate(&valid_keys),
                    submitted_artifacts: &artifacts,
                    active_application_count: 0,
                    direct_applications_today: 0,
                    already_applied_to_posting: false,
                    invitation_decision: None,
                })
                .expect("원티드 지원을 pin해야 한다");

            assert_eq!(result.pin.artifact_completeness_bp, 7_500);
            assert_eq!(result.pin.visible_evidence_ids, vec![10, 20, 30]);
            assert_eq!(result.pin.artifacts[0].kind, ArtifactKind::Portfolio);
        }

        #[test]
        fn given_사랑방_다른지역_when_지원하면_then_지역_filter로_거절한다() {
            let posting = given_posting(PlatformKey::Sarangbang);
            let artifacts = given_required_artifacts(&posting);
            let valid_keys: [&str; 0] = [];
            let mut candidate = given_candidate(&valid_keys);
            candidate.region = Region::Rural;

            let result =
                create_v1_recruitment_rules().prepare_application(ApplicationEligibilityInput {
                    posting: &posting,
                    current_game_day: posting.posted_game_day,
                    source: ApplicationSource::Direct,
                    candidate,
                    submitted_artifacts: &artifacts,
                    active_application_count: 0,
                    direct_applications_today: 0,
                    already_applied_to_posting: false,
                    invitation_decision: None,
                });

            assert_eq!(result, Err(RecruitmentError::RegionMismatch));
        }

        #[test]
        fn given_close_exclusive_day_when_지원하면_then_마감으로_거절한다() {
            let posting = given_posting(PlatformKey::Jobkorea);
            let artifacts = given_required_artifacts(&posting);
            let valid_keys: [&str; 0] = [];

            let result =
                create_v1_recruitment_rules().prepare_application(ApplicationEligibilityInput {
                    posting: &posting,
                    current_game_day: posting.closes_exclusive_game_day,
                    source: ApplicationSource::Direct,
                    candidate: given_candidate(&valid_keys),
                    submitted_artifacts: &artifacts,
                    active_application_count: 0,
                    direct_applications_today: 0,
                    already_applied_to_posting: false,
                    invitation_decision: None,
                });

            assert_eq!(result, Err(RecruitmentError::PostingClosed));
        }

        #[test]
        fn given_직접지원_하루3개_when_직접과_초대를_검사하면_then_직접만_상한에_걸린다() {
            let posting = given_posting(PlatformKey::Saramin);
            let artifacts = given_required_artifacts(&posting);
            let valid_keys: [&str; 0] = [];
            let rules = create_v1_recruitment_rules();
            let invitation_decision = given_stage_decision(RecruitmentStage::Invitation, true);
            let input = |source| ApplicationEligibilityInput {
                posting: &posting,
                current_game_day: posting.posted_game_day,
                source,
                candidate: given_candidate(&valid_keys),
                submitted_artifacts: &artifacts,
                active_application_count: 0,
                direct_applications_today: 3,
                already_applied_to_posting: false,
                invitation_decision: (source == ApplicationSource::Invitation)
                    .then_some(&invitation_decision),
            };

            let direct = rules.prepare_application(input(ApplicationSource::Direct));
            let invitation = rules.prepare_application(input(ApplicationSource::Invitation));

            assert_eq!(direct, Err(RecruitmentError::ApplicationLimit));
            assert!(invitation.is_ok());
        }
    }

    mod context_채용_stage를_판정하는_경우 {
        use super::*;

        #[test]
        fn given_visible_fit_10000_artifact_8000_affinity_10000_when_서류판정하면_then_9500점이다()
        {
            let posting = given_posting(PlatformKey::Wanted);
            let scores = DimensionScores {
                education: 5_000,
                certification: 5_000,
                language: 5_000,
                training: 5_000,
                experience: 5_000,
                project: 5_000,
            };

            let result = create_v1_recruitment_rules()
                .evaluate_document(DocumentEvaluationInput {
                    world_seed: 7,
                    posting: &posting,
                    visible_scores: scores,
                    artifact_completeness_bp: 8_000,
                })
                .expect("서류를 판정해야 한다");

            assert_eq!(result.score_bp, 9_500);
            assert_eq!(result.probability_ppm, 650_000);
        }

        #[test]
        fn given_score_band_경계_when_probability를_조회하면_then_exact_table이_적용된다() {
            let ruleset = v1_recruitment_ruleset();
            let probabilities = ruleset
                .pass_probabilities
                .document
                .for_competition(CompetitionBand::Low);

            let result = [3_999, 4_000, 6_999, 7_000].map(|score| {
                probabilities.for_score_band(
                    score_band(score, ruleset.score_bands).expect("점수 band여야 한다"),
                )
            });

            assert_eq!(result, [400_000, 700_000, 700_000, 900_000]);
        }

        #[test]
        fn given_pinned과_새_evidence_when_면접판정하면_then_pinned의_유효비율만_쓴다() {
            let posting = given_posting(PlatformKey::Jobkorea);
            let scores = DimensionScores {
                education: 5_000,
                certification: 5_000,
                language: 5_000,
                training: 5_000,
                experience: 2_500,
                project: 0,
            };

            let result = create_v1_recruitment_rules()
                .evaluate_interview(InterviewEvaluationInput {
                    world_seed: 7,
                    posting: &posting,
                    possessed_scores: scores,
                    pinned_evidence_ids: &[1, 1, 2, 3],
                    currently_valid_evidence_ids: &[1, 3, 99],
                })
                .expect("면접을 판정해야 한다");

            assert_eq!(result.components.supporting_fit_bp, 2_500);
            assert_eq!(result.components.context_fit_bp, 6_666);
        }

        #[test]
        fn given_사람인_artifact_id만_다를때_when_초대판정하면_then_roll은_같다() {
            let posting = given_posting(PlatformKey::Saramin);
            let first = given_artifact(ArtifactKind::Resume, 1, 7_500, vec![1, 2]);
            let second = given_artifact(ArtifactKind::Resume, 999, 7_500, vec![1, 2]);
            let rules = create_v1_recruitment_rules();
            let evaluate = |artifact: &SubmittedArtifact| {
                rules
                    .evaluate_invitation(InvitationEvaluationInput {
                        world_seed: 77,
                        posting: &posting,
                        invitation_game_day: posting.posted_game_day,
                        candidate: given_candidate(&[]),
                        latest_public_artifact: artifact,
                        visible_scores: DimensionScores::default(),
                        open_invitation_count: 0,
                        platform_invitation_already_generated_today: false,
                    })
                    .expect("사람인 초대를 판정해야 한다")
                    .decision
            };

            assert_eq!(evaluate(&first), evaluate(&second));
        }

        #[test]
        fn given_linkedin_profile_when_초대판정하면_then_5000_2500_2500으로_합친다() {
            let posting = given_posting(PlatformKey::Linkedin);
            let profile = given_artifact(ArtifactKind::LinkedinProfile, 1, 8_000, vec![1, 2]);
            let scores = DimensionScores {
                language: 6_000,
                experience: 4_000,
                ..DimensionScores::default()
            };

            let result = create_v1_recruitment_rules()
                .evaluate_invitation(InvitationEvaluationInput {
                    world_seed: 77,
                    posting: &posting,
                    invitation_game_day: posting.posted_game_day,
                    candidate: given_candidate(&[]),
                    latest_public_artifact: &profile,
                    visible_scores: scores,
                    open_invitation_count: 0,
                    platform_invitation_already_generated_today: false,
                })
                .expect("LinkedIn 초대를 판정해야 한다");

            assert_eq!(result.decision.score_bp, 6_500);
            assert_eq!(result.decision.probability_ppm, 80_000);
        }

        #[test]
        fn given_재직중인_후보_when_초대판정하면_then_초대를_거절한다() {
            let posting = given_posting(PlatformKey::Saramin);
            let resume = given_artifact(ArtifactKind::Resume, 1, 8_000, vec![1]);
            let candidate = CandidateApplicationProfile {
                has_active_or_pending_contract: true,
                ..given_candidate(&[])
            };

            let result =
                create_v1_recruitment_rules().evaluate_invitation(InvitationEvaluationInput {
                    world_seed: 77,
                    posting: &posting,
                    invitation_game_day: posting.posted_game_day,
                    candidate,
                    latest_public_artifact: &resume,
                    visible_scores: DimensionScores::default(),
                    open_invitation_count: 0,
                    platform_invitation_already_generated_today: false,
                });

            assert_eq!(result, Err(RecruitmentError::ActiveEmployment));
        }
    }

    mod context_지원_상태를_전이하는_경우 {
        use super::*;

        #[test]
        fn given_서류통과_when_due_day에_판정하면_then_면접일과_deadline이_같다() {
            let posting = given_posting(PlatformKey::Jobkorea);
            let rules = create_v1_recruitment_rules();
            let initial = rules
                .initial_application_state(&posting, 10)
                .expect("초기 상태를 만들어야 한다");

            let result = rules
                .transition_application(
                    &initial,
                    ApplicationAction::ResolveDocument {
                        game_day: 13,
                        decision: given_stage_decision(RecruitmentStage::Document, true),
                    },
                )
                .expect("서류를 통과해야 한다");

            assert!(matches!(
                result,
                ApplicationState::InterviewAwaitingConfirmation {
                    confirmation_deadline_exclusive_game_day: 18,
                    interview_game_day: 18,
                    ..
                }
            ));
        }

        #[test]
        fn given_confirmation_exclusive_day_when_확인하면_then_interview_expired다() {
            let state = ApplicationState::InterviewAwaitingConfirmation {
                entry_decision: given_stage_decision(RecruitmentStage::Document, true),
                confirmation_deadline_exclusive_game_day: 18,
                interview_game_day: 18,
                offer_expiry_days: 7,
            };

            let result = create_v1_recruitment_rules().transition_application(
                &state,
                ApplicationAction::ConfirmInterview { game_day: 18 },
            );

            assert_eq!(result, Err(RecruitmentError::InterviewExpired));
        }

        #[test]
        fn given_confirmation_exclusive_day_when_expiry_action이면_then_withdrawn이다() {
            let state = ApplicationState::InterviewAwaitingConfirmation {
                entry_decision: given_stage_decision(RecruitmentStage::Document, true),
                confirmation_deadline_exclusive_game_day: 18,
                interview_game_day: 18,
                offer_expiry_days: 7,
            };

            let result = create_v1_recruitment_rules()
                .transition_application(
                    &state,
                    ApplicationAction::ExpireConfirmation { game_day: 18 },
                )
                .expect("확인 기한을 만료해야 한다");

            assert_eq!(
                result,
                ApplicationState::Withdrawn {
                    withdrawn_game_day: 18
                }
            );
        }

        #[test]
        fn given_확정된_면접_when_interview_day에_통과하면_then_7일_offer를_만든다() {
            let state = ApplicationState::InterviewConfirmed {
                entry_decision: given_stage_decision(RecruitmentStage::Document, true),
                confirmed_game_day: 15,
                interview_game_day: 18,
                offer_expiry_days: 7,
            };

            let result = create_v1_recruitment_rules()
                .transition_application(
                    &state,
                    ApplicationAction::ResolveInterview {
                        game_day: 18,
                        decision: given_stage_decision(RecruitmentStage::Interview, true),
                        salary: Some(given_salary()),
                    },
                )
                .expect("면접을 통과해야 한다");

            assert!(matches!(
                result,
                ApplicationState::Offered {
                    offered_game_day: 18,
                    expires_exclusive_game_day: 25,
                    ..
                }
            ));
        }

        #[test]
        fn given_offer_exclusive_day_when_decline하면_then_offer_expired다() {
            let state = given_offered_state(25);

            let result = create_v1_recruitment_rules()
                .transition_application(&state, ApplicationAction::DeclineOffer { game_day: 25 });

            assert_eq!(result, Err(RecruitmentError::OfferExpired));
        }

        #[test]
        fn given_active_지원_when_offer_acceptance로_정리하면_then_closed가_허용된다() {
            let state = ApplicationState::Submitted {
                submitted_game_day: 1,
                document_decision_game_day: 4,
                interview_delay_days: 5,
                offer_expiry_days: 7,
            };

            let result = create_v1_recruitment_rules()
                .transition_application(&state, ApplicationAction::Close { game_day: 8 })
                .expect("active 지원을 닫아야 한다");

            assert_eq!(result, ApplicationState::Closed { closed_game_day: 8 });
        }
    }

    mod context_오퍼_연봉과_계약을_만드는_경우 {
        use super::*;

        #[test]
        fn given_9개_salary_step_when_세_band를_판정하면_then_각_3개_partition에_머문다() {
            let posting = given_posting(PlatformKey::Wanted);
            let rules = create_v1_recruitment_rules();
            let calculate = |score| {
                rules
                    .determine_offer_salary(OfferSalaryInput {
                        world_seed: 7,
                        posting: &posting,
                        possessed_fit_bp: score,
                    })
                    .expect("연봉을 정해야 한다")
            };

            let low = calculate(3_999);
            let medium = calculate(4_000);
            let high = calculate(7_000);

            assert!((0..3).contains(&low.salary_step_index));
            assert!((3..6).contains(&medium.salary_step_index));
            assert!((6..9).contains(&high.salary_step_index));
            assert_eq!(low.annual_salary_krw % posting.salary_step_krw, 0);
            assert_eq!(medium.annual_salary_krw % posting.salary_step_krw, 0);
            assert_eq!(high.annual_salary_krw % posting.salary_step_krw, 0);
        }

        #[test]
        fn given_같은_score_band_when_salary를_재계산하면_then_같은_hmac_step이다() {
            let posting = given_posting(PlatformKey::Wanted);
            let rules = create_v1_recruitment_rules();
            let calculate = |score| {
                rules
                    .determine_offer_salary(OfferSalaryInput {
                        world_seed: 7,
                        posting: &posting,
                        possessed_fit_bp: score,
                    })
                    .expect("연봉을 정해야 한다")
            };

            let first = calculate(4_000);
            let replay = calculate(6_999);

            assert_eq!(first.salary_step_index, replay.salary_step_index);
            assert_eq!(first.salary_roll_word, replay.salary_roll_word);
        }

        #[test]
        fn given_offer를_수락할때_when_계약이_없으면_then_expiry_다음날_계약과_close_plan을_만든다()
        {
            let posting = given_posting(PlatformKey::Wanted);
            let state = given_offered_state(30);

            let result = create_v1_recruitment_rules()
                .plan_offer_acceptance(OfferAcceptanceInput {
                    application_id: 10,
                    posting: &posting,
                    state: &state,
                    accepted_game_day: 29,
                    contracts: &[],
                    other_accepted_offer_count: 0,
                    other_open_application_ids: &[30, 20],
                    open_invitation_ids: &[9, 7],
                })
                .expect("오퍼를 수락해야 한다");

            assert_eq!(result.contract.start_game_day, 31);
            assert_eq!(result.contract.monthly_payday, 25);
            assert_eq!(result.close_application_ids, vec![20, 30]);
            assert_eq!(result.close_invitation_ids, vec![7, 9]);
        }

        #[test]
        fn given_pending_contract가_있을때_when_수락하면_then_active_employment로_거절한다() {
            let posting = given_posting(PlatformKey::Wanted);
            let state = given_offered_state(30);

            let result =
                create_v1_recruitment_rules().plan_offer_acceptance(OfferAcceptanceInput {
                    application_id: 10,
                    posting: &posting,
                    state: &state,
                    accepted_game_day: 29,
                    contracts: &[EmploymentContractSummary {
                        contract_id: 1,
                        status: EmploymentContractStatus::PendingStart,
                    }],
                    other_accepted_offer_count: 0,
                    other_open_application_ids: &[],
                    open_invitation_ids: &[],
                });

            assert_eq!(result, Err(RecruitmentError::ActiveEmployment));
        }

        #[test]
        fn given_pending과_active_계약이_함께있을때_when_검증하면_then_단일성_위반이다() {
            let contracts = [
                EmploymentContractSummary {
                    contract_id: 1,
                    status: EmploymentContractStatus::PendingStart,
                },
                EmploymentContractSummary {
                    contract_id: 2,
                    status: EmploymentContractStatus::Active,
                },
            ];

            let result = create_v1_recruitment_rules().validate_contracts(&contracts);

            assert_eq!(result, Err(RecruitmentError::MultipleActiveContracts));
        }

        #[test]
        fn given_offer_exclusive_day_when_수락하면_then_offer_expired다() {
            let posting = given_posting(PlatformKey::Wanted);
            let state = given_offered_state(30);

            let result =
                create_v1_recruitment_rules().plan_offer_acceptance(OfferAcceptanceInput {
                    application_id: 10,
                    posting: &posting,
                    state: &state,
                    accepted_game_day: 30,
                    contracts: &[],
                    other_accepted_offer_count: 0,
                    other_open_application_ids: &[],
                    open_invitation_ids: &[],
                });

            assert_eq!(result, Err(RecruitmentError::OfferExpired));
        }
    }

    mod context_ruleset을_구성하는_경우 {
        use super::*;

        #[test]
        fn given_weight합이_10000이_아닐때_when_factory를_만들면_then_거절한다() {
            let mut ruleset = v1_recruitment_ruleset();
            ruleset.document_weights.context_fit_bp = 1_499;

            let result = create_recruitment_rules(ruleset);

            assert!(matches!(result, Err(RecruitmentError::InvalidRuleset)));
        }
    }
}
