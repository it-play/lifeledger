use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result, bail, ensure};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{MySql, MySqlPool, Transaction};

use super::mysql::{
    CommandIdentitySpec, CommandIdentityState, GameCommandReceiptWrite, inspect_command_identity,
    read_state, write_command_identity, write_game_command_receipt,
};
use super::types::{
    AcceptCareerInvitationCommand, AcceptCareerOfferCommand, ApplyCareerCommand,
    CareerApplicationReceipt, CareerApplicationState, CareerApplicationStatus,
    CareerApplicationsPageState, CareerEmploymentState, CareerInvitationReceipt,
    CareerInvitationState, CareerInvitationStatus, CareerJobState, CareerJobsPageQuery,
    CareerJobsPageState, CareerOfferReceipt, CareerOfferState, CareerOfferStatus, CareerPageQuery, CareerStoreResult,
    ConfirmCareerInterviewCommand, DeclineCareerInvitationCommand, DeclineCareerOfferCommand,
    EmploymentContractState, GameCommandCursor, InterviewDecision,
    WithdrawCareerApplicationCommand,
};
use crate::career::{
    ApplicationAction, ApplicationEligibilityInput, ApplicationSource, ApplicationState,
    ArtifactKind, CandidateApplicationProfile, CompetitionBand, CompetitionProbabilities,
    ComponentWeights, DimensionRequirement, DimensionScores, DocumentEvaluationInput,
    EmploymentContractStatus, EmploymentContractSummary, EmploymentType, EvidenceKind,
    EvidencePeriodFields, Industry, InterviewEvaluationInput, InvitationComponentWeights,
    InvitationEvaluationInput, InvitationSource, JobFamilyContribution, JobTemplate, LifeStatus,
    MaterializedPosting, MilitaryPostingRequirement, MilitaryQualification, OfferAcceptanceInput,
    OfferSalary, OfferSalaryInput, PassProbabilityTable, PlatformDefinition, PlatformIndustryWeight,
    PlatformKey, PostingMaterializationInput, PostingSeedInput, RecruitmentError,
    RecruitmentRules, RecruitmentRuleset, RecruitmentStage, ScoreBand, ScoreBandBoundaries, ScoreBandProbabilities,
    SpecCatalogEntry, SpecDimension, SpecEvidence, SpecScoreInput, StageComponents, StageDecision,
    SubmittedArtifact, create_recruitment_rules, create_spec_score_rules,
};
use crate::character::{Education, MilitaryStatus, Region};
use crate::finance::{CommandCursor, CommandId, ResourceId};

const MAX_PAGE_LIMIT: u32 = 200;
const COMMAND_KIND_APPLY: &str = "careerApplicationSubmit";
const COMMAND_KIND_INTERVIEW_CONFIRMATION: &str = "careerInterviewConfirmation";
const COMMAND_KIND_APPLICATION_WITHDRAW: &str = "careerApplicationWithdraw";
const COMMAND_KIND_INVITATION_ACCEPT: &str = "careerInvitationAccept";
const COMMAND_KIND_INVITATION_DECLINE: &str = "careerInvitationDecline";
const COMMAND_KIND_OFFER_ACCEPT: &str = "careerOfferAccept";
const COMMAND_KIND_OFFER_DECLINE: &str = "careerOfferDecline";

#[derive(Debug, sqlx::FromRow)]
struct PostingScopeRow {
    market_world_id: u64,
    world_seed: u64,
    world_model_version: String,
    career_catalog_bundle_id: u64,
    career_catalog_bundle_key: String,
}

#[derive(Debug, sqlx::FromRow)]
struct RulesetRow {
    id: u64,
    ruleset_key: String,
    active_application_limit: u8,
    daily_application_limit: u8,
    open_invitation_limit: u8,
    employment_start_delay_days: u32,
    payday_day_of_month: u8,
}

#[derive(Debug, sqlx::FromRow)]
struct ComponentWeightRow {
    stage: String,
    component: String,
    weight_bp: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct ScoreBandRow {
    score_band_key: String,
    minimum_score_bp: i64,
    maximum_exclusive_score_bp: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct ProbabilityRow {
    stage: String,
    competition_band: String,
    score_band_key: String,
    pass_probability_ppm: u32,
}

#[derive(Debug, sqlx::FromRow)]
struct PlatformRow {
    id: u64,
    platform_key: String,
    daily_slot_count: u32,
    competition_band: String,
    document_review_days: u32,
    same_region_only: bool,
    invitation_source: String,
    first_pay_reward_krw: i64,
    requires_resume: bool,
    requires_portfolio: bool,
    requires_linkedin_profile: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct IndustryWeightRow {
    platform_catalog_id: u64,
    industry_key: String,
    weight_bp: u32,
}

#[derive(Debug, sqlx::FromRow)]
struct TemplateRequirementRow {
    id: u64,
    platform_catalog_id: u64,
    career_industry_id: u64,
    career_job_family_id: u64,
    virtual_employer_id: u64,
    template_key: String,
    platform_key: String,
    employer_key: String,
    employer_name: String,
    industry_key: String,
    job_family_key: String,
    region: String,
    employment_type: String,
    minimum_education: Option<String>,
    required_certification_entry_key: Option<String>,
    minimum_experience_days: u32,
    military_requirement: String,
    minimum_annual_salary_krw: i64,
    maximum_annual_salary_krw: i64,
    salary_step_krw: i64,
    interview_delay_days: u32,
    offer_expiry_days: u32,
    posting_open_days: u32,
    dimension: String,
    required_score_bp: i64,
    weight_bp: i64,
}

#[derive(Debug, Clone)]
struct CatalogTemplate {
    id: u64,
    platform_catalog_id: u64,
    career_industry_id: u64,
    career_job_family_id: u64,
    virtual_employer_id: u64,
    template: JobTemplate,
}

struct TemplateAccumulator {
    ids: (u64, u64, u64, u64, u64),
    template: JobTemplate,
}

#[derive(Debug, sqlx::FromRow)]
struct RecruitmentScopeRow {
    save_id: u64,
    market_world_id: u64,
    world_seed: u64,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
    cash_krw: i64,
    career_catalog_bundle_id: u64,
    character_region: String,
    character_education: String,
    character_military: String,
}

#[derive(Debug, sqlx::FromRow)]
struct PostingRow {
    id: u64,
    posting_key: String,
    market_world_id: u64,
    world_seed: u64,
    world_model_version: String,
    career_catalog_bundle_id: u64,
    career_catalog_bundle_key: String,
    recruitment_ruleset_id: u64,
    recruitment_ruleset_key: String,
    platform_catalog_id: u64,
    platform_key: String,
    template_key: String,
    employer_key: String,
    employer_name: String,
    industry_key: String,
    job_family_key: String,
    region: String,
    employment_type: String,
    posted_game_day: u32,
    closes_exclusive_game_day: u32,
    competition_band: String,
    document_review_days: u32,
    same_region_only: bool,
    requires_resume: bool,
    requires_portfolio: bool,
    requires_linkedin_profile: bool,
    first_pay_reward_krw: i64,
    minimum_education: Option<String>,
    required_certification_entry_key: Option<String>,
    required_certification_name: Option<String>,
    minimum_experience_days: u32,
    military_requirement: String,
    minimum_annual_salary_krw: i64,
    maximum_annual_salary_krw: i64,
    salary_step_krw: i64,
    interview_delay_days: u32,
    offer_expiry_days: u32,
    platform_affinity_bp: i64,
    education_required_score_bp: i64,
    education_weight_bp: i64,
    certification_required_score_bp: i64,
    certification_weight_bp: i64,
    language_required_score_bp: i64,
    language_weight_bp: i64,
    training_required_score_bp: i64,
    training_weight_bp: i64,
    experience_required_score_bp: i64,
    experience_weight_bp: i64,
    project_required_score_bp: i64,
    project_weight_bp: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct EvidenceDbRow {
    id: u64,
    evidence_key: String,
    catalog_entry_key: String,
    kind: String,
    acquired_game_day: u32,
    expires_on_game_day: Option<u32>,
    period_start_date: Option<time::Date>,
    period_end_exclusive_date: Option<time::Date>,
    source_kind: String,
}

#[derive(Debug, sqlx::FromRow)]
struct CatalogContributionDbRow {
    entry_key: String,
    kind: String,
    stackable: bool,
    job_family_key: String,
    contribution_bp: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct ArtifactDbRow {
    id: u64,
    artifact_kind: String,
    completeness_bp: i64,
    open_to_work: Option<bool>,
    created_game_day: u32,
    is_public: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct ApplicationReadRow {
    id: u64,
    posting_key: String,
    platform_key: String,
    industry_key: String,
    employer_name: String,
    job_family_key: String,
    source_kind: String,
    status: String,
    submitted_game_day: u32,
    visible_education_score_bp: i64,
    visible_certification_score_bp: i64,
    visible_language_score_bp: i64,
    visible_training_score_bp: i64,
    visible_experience_score_bp: i64,
    visible_project_score_bp: i64,
    possessed_education_score_bp: Option<i64>,
    possessed_certification_score_bp: Option<i64>,
    possessed_language_score_bp: Option<i64>,
    possessed_training_score_bp: Option<i64>,
    possessed_experience_score_bp: Option<i64>,
    possessed_project_score_bp: Option<i64>,
    document_score_bp: Option<i64>,
    document_decided_game_day: Option<u32>,
    interview_game_day: Option<u32>,
    confirmation_expires_exclusive_game_day: Option<u32>,
    interview_score_bp: Option<i64>,
    offer_id: Option<u64>,
    offer_status: Option<String>,
    annual_salary_krw: Option<i64>,
    payday_day_of_month: Option<u8>,
    start_game_day: Option<u32>,
    expires_exclusive_game_day: Option<u32>,
    first_pay_reward_krw: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
struct InvitationReadRow {
    id: u64,
    posting_key: String,
    platform_key: String,
    industry_key: String,
    job_family_key: String,
    employer_name: String,
    profile_artifact_version_id: u64,
    invitation_game_day: u32,
    expires_exclusive_game_day: u32,
    status: String,
}

#[derive(Debug, sqlx::FromRow)]
struct EmploymentReadRow {
    id: u64,
    status: String,
    job_family_key: String,
    employer_name: String,
    region: String,
    annual_salary_krw: i64,
    payday_day_of_month: u8,
    start_game_day: u32,
    end_game_day: Option<u32>,
    credited_experience_days: u32,
}

#[derive(Debug, sqlx::FromRow)]
struct ScheduledActionRow {
    id: u64,
    action_kind: String,
    payload_version: u8,
    phase_rank: u8,
    due_game_day: u32,
    source_kind: String,
    source_id: u64,
    occurrence: u64,
    recruitment_ruleset_id: u64,
    employment_contract_id: Option<u64>,
    job_application_id: Option<u64>,
    platform_catalog_id: Option<u64>,
    invitation_generation_game_day: Option<u32>,
}

pub(super) async fn ensure_recruitment_postings_for_user(
    pool: &MySqlPool,
    user_id: u64,
    target_game_day: u32,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let Some(scope) = read_posting_scope(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(());
    };
    let (ruleset_id, rules) = read_recruitment_rules(&mut tx, scope.career_catalog_bundle_id)
        .await
        .context("failed to load the assigned recruitment ruleset")?;
    let platforms = read_platforms(&mut tx, scope.career_catalog_bundle_id).await?;
    let industry_weights =
        read_platform_industry_weights(&mut tx, scope.career_catalog_bundle_id).await?;
    let templates = read_job_templates(&mut tx, scope.career_catalog_bundle_id).await?;
    let maximum_open_days = templates
        .iter()
        .map(|template| template.template.posting_open_days)
        .max()
        .context("career bundle has no recruitment templates")?;
    ensure!(
        maximum_open_days > 0,
        "posting open duration must be positive"
    );
    let first_game_day = target_game_day.saturating_sub(maximum_open_days - 1);

    for game_day in first_game_day..=target_game_day {
        for (platform_id, platform) in &platforms {
            let platform_weights = industry_weights
                .get(platform_id)
                .context("platform has no industry weights")?;
            let platform_templates = templates
                .iter()
                .filter(|template| template.platform_catalog_id == *platform_id)
                .map(|template| template.template.clone())
                .collect::<Vec<_>>();
            for slot_no in 0..platform.daily_slot_count {
                let posting = rules
                    .materialize_posting(PostingMaterializationInput {
                        seed: PostingSeedInput {
                            world_model_version: &scope.world_model_version,
                            world_seed: scope.world_seed,
                            career_catalog_bundle_key: &scope.career_catalog_bundle_key,
                            game_day,
                            slot_no,
                        },
                        platform,
                        industry_weights: platform_weights,
                        templates: &platform_templates,
                    })
                    .map_err(anyhow::Error::new)?;
                let selected = templates
                    .iter()
                    .find(|template| {
                        template.platform_catalog_id == *platform_id
                            && template.template.template_key == posting.template_key
                    })
                    .context("posting generator selected an unknown template")?;
                insert_materialized_posting(
                    &mut tx,
                    &scope,
                    ruleset_id,
                    *platform_id,
                    selected,
                    slot_no,
                    &posting,
                )
                .await?;
            }
        }
    }

    tx.commit().await?;
    Ok(())
}

async fn read_posting_scope(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
) -> Result<Option<PostingScopeRow>> {
    sqlx::query_as(
        "SELECT save.market_world_id, world.seed AS world_seed,
                calibration.version AS world_model_version,
                career_run.career_catalog_bundle_id,
                bundle.bundle_key AS career_catalog_bundle_key
         FROM save
         INNER JOIN `character` ON `character`.save_id = save.id
         INNER JOIN career_run
           ON career_run.save_id = save.id
          AND career_run.run_revision = save.run_revision
         INNER JOIN career_catalog_bundle AS bundle
           ON bundle.id = career_run.career_catalog_bundle_id
          AND bundle.published_at IS NOT NULL
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         INNER JOIN market_calibration AS calibration ON calibration.id = world.calibration_id
         WHERE save.user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read recruitment posting scope")
}

async fn read_recruitment_rules(
    tx: &mut Transaction<'_, MySql>,
    career_catalog_bundle_id: u64,
) -> Result<(u64, std::sync::Arc<dyn RecruitmentRules>)> {
    let row: RulesetRow = sqlx::query_as(
        "SELECT ruleset.id, ruleset.ruleset_key, ruleset.active_application_limit,
                ruleset.daily_application_limit, ruleset.open_invitation_limit,
                ruleset.employment_start_delay_days, ruleset.payday_day_of_month
         FROM recruitment_ruleset_assignment AS assignment
         INNER JOIN recruitment_ruleset AS ruleset
           ON ruleset.id = assignment.recruitment_ruleset_id
          AND ruleset.published_at IS NOT NULL
         INNER JOIN career_recruitment_compatibility AS compatibility
           ON compatibility.career_catalog_bundle_id = assignment.career_catalog_bundle_id
          AND compatibility.recruitment_ruleset_id = ruleset.id
         WHERE assignment.career_catalog_bundle_id = ?
           AND BINARY assignment.assignment_key = BINARY 'newPosting'
         FOR SHARE",
    )
    .bind(career_catalog_bundle_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("career bundle has no assigned recruitment ruleset")?;
    build_recruitment_rules(tx, row).await
}

async fn read_recruitment_rules_by_id(
    tx: &mut Transaction<'_, MySql>,
    career_catalog_bundle_id: u64,
    recruitment_ruleset_id: u64,
) -> Result<std::sync::Arc<dyn RecruitmentRules>> {
    let row: RulesetRow = sqlx::query_as(
        "SELECT ruleset.id, ruleset.ruleset_key, ruleset.active_application_limit,
                ruleset.daily_application_limit, ruleset.open_invitation_limit,
                ruleset.employment_start_delay_days, ruleset.payday_day_of_month
         FROM recruitment_ruleset AS ruleset
         INNER JOIN career_recruitment_compatibility AS compatibility
           ON compatibility.recruitment_ruleset_id = ruleset.id
          AND compatibility.career_catalog_bundle_id = ?
         WHERE ruleset.id = ? AND ruleset.published_at IS NOT NULL
         FOR SHARE",
    )
    .bind(career_catalog_bundle_id)
    .bind(recruitment_ruleset_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("posting recruitment ruleset is unavailable")?;
    Ok(build_recruitment_rules(tx, row).await?.1)
}

async fn build_recruitment_rules(
    tx: &mut Transaction<'_, MySql>,
    row: RulesetRow,
) -> Result<(u64, std::sync::Arc<dyn RecruitmentRules>)> {
    let weights: Vec<ComponentWeightRow> = sqlx::query_as(
        "SELECT stage, component, weight_bp
         FROM recruitment_stage_component_weight
         WHERE recruitment_ruleset_id = ?
         ORDER BY stage, component",
    )
    .bind(row.id)
    .fetch_all(&mut **tx)
    .await?;
    let bands: Vec<ScoreBandRow> = sqlx::query_as(
        "SELECT score_band_key, minimum_score_bp, maximum_exclusive_score_bp
         FROM recruitment_score_band
         WHERE recruitment_ruleset_id = ?
         ORDER BY minimum_score_bp",
    )
    .bind(row.id)
    .fetch_all(&mut **tx)
    .await?;
    let probabilities: Vec<ProbabilityRow> = sqlx::query_as(
        "SELECT stage, competition_band, score_band_key, pass_probability_ppm
         FROM recruitment_pass_probability
         WHERE recruitment_ruleset_id = ?
         ORDER BY stage, competition_band, score_band_key",
    )
    .bind(row.id)
    .fetch_all(&mut **tx)
    .await?;

    let document_weights = component_weights(
        &weights,
        "document",
        ["visibleFit", "artifactCompleteness", "platformAffinity"],
    )?;
    let interview_weights = component_weights(
        &weights,
        "interview",
        ["possessedFit", "experienceProjectFit", "profileConsistency"],
    )?;
    let invitation_weights = invitation_component_weights(&weights)?;
    let score_bands = score_band_boundaries(&bands)?;
    let pass_probabilities = pass_probability_table(&probabilities)?;
    let ruleset = RecruitmentRuleset {
        ruleset_key: row.ruleset_key,
        document_weights,
        interview_weights,
        linkedin_invitation_weights: invitation_weights,
        score_bands,
        pass_probabilities,
        start_delay_days: row.employment_start_delay_days,
        monthly_payday: row.payday_day_of_month,
        active_application_limit: u32::from(row.active_application_limit),
        direct_application_daily_limit: u32::from(row.daily_application_limit),
        open_invitation_limit: u32::from(row.open_invitation_limit),
    };
    let rules = create_recruitment_rules(ruleset).map_err(anyhow::Error::new)?;
    Ok((row.id, rules))
}

fn component_weights(
    rows: &[ComponentWeightRow],
    stage: &str,
    components: [&str; 3],
) -> Result<ComponentWeights> {
    Ok(ComponentWeights {
        primary_fit_bp: exact_component_weight(rows, stage, components[0])?,
        supporting_fit_bp: exact_component_weight(rows, stage, components[1])?,
        context_fit_bp: exact_component_weight(rows, stage, components[2])?,
    })
}

fn invitation_component_weights(rows: &[ComponentWeightRow]) -> Result<InvitationComponentWeights> {
    Ok(InvitationComponentWeights {
        completeness_bp: exact_component_weight(rows, "invitation", "artifactCompleteness")?,
        language_score_bp: exact_component_weight(rows, "invitation", "languageScore")?,
        experience_score_bp: exact_component_weight(rows, "invitation", "experienceScore")?,
    })
}

fn exact_component_weight(
    rows: &[ComponentWeightRow],
    stage: &str,
    component: &str,
) -> Result<i64> {
    let matches = rows
        .iter()
        .filter(|row| row.stage == stage && row.component == component)
        .collect::<Vec<_>>();
    ensure!(
        matches.len() == 1,
        "recruitment component weight is incomplete"
    );
    Ok(matches[0].weight_bp)
}

fn score_band_boundaries(rows: &[ScoreBandRow]) -> Result<ScoreBandBoundaries> {
    ensure!(rows.len() == 3, "recruitment score bands are incomplete");
    let low = rows
        .iter()
        .find(|row| row.score_band_key == "low")
        .context("low score band is missing")?;
    let medium = rows
        .iter()
        .find(|row| row.score_band_key == "medium")
        .context("medium score band is missing")?;
    let high = rows
        .iter()
        .find(|row| row.score_band_key == "high")
        .context("high score band is missing")?;
    ensure!(
        low.minimum_score_bp == 0 && low.maximum_exclusive_score_bp == medium.minimum_score_bp,
        "low recruitment score band is not contiguous"
    );
    ensure!(
        medium.maximum_exclusive_score_bp == high.minimum_score_bp
            && high.maximum_exclusive_score_bp == 10_001,
        "recruitment score bands are not contiguous"
    );
    Ok(ScoreBandBoundaries {
        medium_minimum_bp: medium.minimum_score_bp,
        high_minimum_bp: high.minimum_score_bp,
    })
}

fn pass_probability_table(rows: &[ProbabilityRow]) -> Result<PassProbabilityTable> {
    Ok(PassProbabilityTable {
        document: competition_probabilities(rows, "document")?,
        interview: competition_probabilities(rows, "interview")?,
        invitation: competition_probabilities(rows, "invitation")?,
    })
}

fn competition_probabilities(
    rows: &[ProbabilityRow],
    stage: &str,
) -> Result<CompetitionProbabilities> {
    Ok(CompetitionProbabilities {
        low: score_band_probabilities(rows, stage, "low")?,
        medium: score_band_probabilities(rows, stage, "medium")?,
        high: score_band_probabilities(rows, stage, "high")?,
    })
}

fn score_band_probabilities(
    rows: &[ProbabilityRow],
    stage: &str,
    competition: &str,
) -> Result<ScoreBandProbabilities> {
    let value = |score_band: &str| -> Result<u32> {
        let matches = rows
            .iter()
            .filter(|row| {
                row.stage == stage
                    && row.competition_band == competition
                    && row.score_band_key == score_band
            })
            .collect::<Vec<_>>();
        ensure!(
            matches.len() == 1,
            "recruitment probability table is incomplete"
        );
        Ok(matches[0].pass_probability_ppm)
    };
    Ok(ScoreBandProbabilities {
        low_score_ppm: value("low")?,
        medium_score_ppm: value("medium")?,
        high_score_ppm: value("high")?,
    })
}

async fn read_platforms(
    tx: &mut Transaction<'_, MySql>,
    career_catalog_bundle_id: u64,
) -> Result<BTreeMap<u64, PlatformDefinition>> {
    let rows: Vec<PlatformRow> = sqlx::query_as(
        "SELECT platform.id, platform.platform_key, platform.daily_slot_count,
                platform.competition_band, platform.document_review_days,
                platform.same_region_only, platform.invitation_source,
                platform.first_pay_reward_krw,
                EXISTS(SELECT 1 FROM platform_artifact_requirement requirement
                       WHERE requirement.career_catalog_bundle_id = platform.career_catalog_bundle_id
                         AND requirement.platform_catalog_id = platform.id
                         AND requirement.artifact_kind = 'resume') AS requires_resume,
                EXISTS(SELECT 1 FROM platform_artifact_requirement requirement
                       WHERE requirement.career_catalog_bundle_id = platform.career_catalog_bundle_id
                         AND requirement.platform_catalog_id = platform.id
                         AND requirement.artifact_kind = 'portfolio') AS requires_portfolio,
                EXISTS(SELECT 1 FROM platform_artifact_requirement requirement
                       WHERE requirement.career_catalog_bundle_id = platform.career_catalog_bundle_id
                         AND requirement.platform_catalog_id = platform.id
                         AND requirement.artifact_kind = 'linkedinProfile') AS requires_linkedin_profile
         FROM platform_catalog AS platform
         WHERE platform.career_catalog_bundle_id = ?
         ORDER BY platform.platform_key",
    )
    .bind(career_catalog_bundle_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() == 6,
        "career bundle must contain six recruitment platforms"
    );
    rows.into_iter()
        .map(|row| {
            let mut required_artifacts = Vec::new();
            if row.requires_resume {
                required_artifacts.push(ArtifactKind::Resume);
            }
            if row.requires_portfolio {
                required_artifacts.push(ArtifactKind::Portfolio);
            }
            if row.requires_linkedin_profile {
                required_artifacts.push(ArtifactKind::LinkedinProfile);
            }
            Ok((
                row.id,
                PlatformDefinition {
                    platform: enum_from_db(&row.platform_key)?,
                    daily_slot_count: row.daily_slot_count,
                    competition_band: enum_from_db(&row.competition_band)?,
                    document_review_days: row.document_review_days,
                    same_region_only: row.same_region_only,
                    invitation_source: enum_from_db(&row.invitation_source)?,
                    required_artifacts,
                    first_pay_reward_krw: row.first_pay_reward_krw,
                },
            ))
        })
        .collect()
}

async fn read_platform_industry_weights(
    tx: &mut Transaction<'_, MySql>,
    career_catalog_bundle_id: u64,
) -> Result<HashMap<u64, Vec<PlatformIndustryWeight>>> {
    let rows: Vec<IndustryWeightRow> = sqlx::query_as(
        "SELECT weight.platform_catalog_id, industry.industry_key, weight.weight_bp
         FROM platform_industry_weight AS weight
         INNER JOIN career_industry AS industry
           ON industry.career_catalog_bundle_id = weight.career_catalog_bundle_id
          AND industry.id = weight.career_industry_id
         WHERE weight.career_catalog_bundle_id = ?
         ORDER BY weight.platform_catalog_id, industry.industry_key",
    )
    .bind(career_catalog_bundle_id)
    .fetch_all(&mut **tx)
    .await?;
    let mut grouped = HashMap::<u64, Vec<PlatformIndustryWeight>>::new();
    for row in rows {
        grouped
            .entry(row.platform_catalog_id)
            .or_default()
            .push(PlatformIndustryWeight {
                industry: enum_from_db(&row.industry_key)?,
                weight_bp: row.weight_bp,
            });
    }
    Ok(grouped)
}

async fn read_job_templates(
    tx: &mut Transaction<'_, MySql>,
    career_catalog_bundle_id: u64,
) -> Result<Vec<CatalogTemplate>> {
    let rows: Vec<TemplateRequirementRow> = sqlx::query_as(
        "SELECT template.id, template.platform_catalog_id, template.career_industry_id,
                template.career_job_family_id, template.virtual_employer_id,
                template.template_key,
                platform.platform_key, employer.employer_key, employer.display_name AS employer_name,
                industry.industry_key, family.job_family_key, employer.region,
                template.employment_type, template.minimum_education,
                certification.entry_key AS required_certification_entry_key,
                template.minimum_experience_days, template.military_requirement,
                template.minimum_annual_salary_krw, template.maximum_annual_salary_krw,
                template.salary_step_krw, template.interview_delay_days,
                template.offer_expiry_days, template.posting_open_days,
                requirement.dimension, requirement.required_score_bp, requirement.weight_bp
         FROM job_template AS template
         INNER JOIN platform_catalog AS platform
           ON platform.career_catalog_bundle_id = template.career_catalog_bundle_id
          AND platform.id = template.platform_catalog_id
         INNER JOIN career_industry AS industry
           ON industry.career_catalog_bundle_id = template.career_catalog_bundle_id
          AND industry.id = template.career_industry_id
         INNER JOIN career_job_family AS family
           ON family.career_catalog_bundle_id = template.career_catalog_bundle_id
          AND family.id = template.career_job_family_id
         INNER JOIN virtual_employer AS employer
           ON employer.career_catalog_bundle_id = template.career_catalog_bundle_id
          AND employer.id = template.virtual_employer_id
         LEFT JOIN spec_catalog_entry AS certification
           ON certification.career_catalog_bundle_id = template.career_catalog_bundle_id
          AND certification.id = template.required_certification_entry_id
         INNER JOIN job_template_dimension_requirement AS requirement
           ON requirement.career_catalog_bundle_id = template.career_catalog_bundle_id
          AND requirement.job_template_id = template.id
         WHERE template.career_catalog_bundle_id = ?
         ORDER BY template.template_key, requirement.dimension",
    )
    .bind(career_catalog_bundle_id)
    .fetch_all(&mut **tx)
    .await?;
    let mut grouped = BTreeMap::<u64, TemplateAccumulator>::new();
    for row in rows {
        let requirement = DimensionRequirement {
            dimension: enum_from_db(&row.dimension)?,
            required_score_bp: row.required_score_bp,
            weight_bp: row.weight_bp,
        };
        if let Some(existing) = grouped.get_mut(&row.id) {
            existing.template.requirements.push(requirement);
            continue;
        }
        grouped.insert(
            row.id,
            TemplateAccumulator {
                ids: (
                    row.id,
                    row.platform_catalog_id,
                    row.career_industry_id,
                    row.career_job_family_id,
                    row.virtual_employer_id,
                ),
                template: JobTemplate {
                    template_key: row.template_key,
                    platform: enum_from_db(&row.platform_key)?,
                    employer_key: row.employer_key,
                    employer_name: row.employer_name,
                    industry: enum_from_db(&row.industry_key)?,
                    job_family_key: row.job_family_key,
                    region: enum_from_db(&row.region)?,
                    employment_type: enum_from_db(&row.employment_type)?,
                    minimum_education: row
                        .minimum_education
                        .as_deref()
                        .map(enum_from_db)
                        .transpose()?,
                    required_certification_entry_key: row.required_certification_entry_key,
                    minimum_experience_days: row.minimum_experience_days,
                    military_requirement: enum_from_db(&row.military_requirement)?,
                    minimum_annual_salary_krw: row.minimum_annual_salary_krw,
                    maximum_annual_salary_krw: row.maximum_annual_salary_krw,
                    salary_step_krw: row.salary_step_krw,
                    interview_delay_days: row.interview_delay_days,
                    offer_expiry_days: row.offer_expiry_days,
                    posting_open_days: row.posting_open_days,
                    requirements: vec![requirement],
                },
            },
        );
    }
    grouped
        .into_values()
        .map(|entry| {
            ensure!(
                entry.template.requirements.len() == SpecDimension::ALL.len(),
                "job template must contain all dimension requirements"
            );
            let (
                id,
                platform_catalog_id,
                career_industry_id,
                career_job_family_id,
                virtual_employer_id,
            ) = entry.ids;
            Ok(CatalogTemplate {
                id,
                platform_catalog_id,
                career_industry_id,
                career_job_family_id,
                virtual_employer_id,
                template: entry.template,
            })
        })
        .collect()
}

async fn insert_materialized_posting(
    tx: &mut Transaction<'_, MySql>,
    scope: &PostingScopeRow,
    recruitment_ruleset_id: u64,
    platform_catalog_id: u64,
    selected: &CatalogTemplate,
    slot_no: u32,
    posting: &MaterializedPosting,
) -> Result<()> {
    let requirements = selected
        .template
        .requirements
        .iter()
        .map(|requirement| {
            (
                requirement.dimension,
                (requirement.required_score_bp, requirement.weight_bp),
            )
        })
        .collect::<HashMap<_, _>>();
    let requirement = |dimension: SpecDimension| -> Result<(i64, i64)> {
        requirements
            .get(&dimension)
            .copied()
            .context("job template dimension is missing")
    };
    let education = requirement(SpecDimension::Education)?;
    let certification = requirement(SpecDimension::Certification)?;
    let language = requirement(SpecDimension::Language)?;
    let training = requirement(SpecDimension::Training)?;
    let experience = requirement(SpecDimension::Experience)?;
    let project = requirement(SpecDimension::Project)?;
    sqlx::query(
        "INSERT IGNORE INTO job_posting
             (posting_key, market_world_id, career_catalog_bundle_id,
              recruitment_ruleset_id, platform_catalog_id, job_template_id,
              career_industry_id, career_job_family_id, virtual_employer_id,
              slot_no, posted_game_day, closes_exclusive_game_day, region,
              employment_type, competition_band, minimum_education,
              required_certification_entry_id, minimum_experience_days,
              military_requirement, requires_resume, requires_portfolio,
              requires_linkedin_profile, minimum_annual_salary_krw,
              maximum_annual_salary_krw, salary_step_krw, document_review_days,
              interview_delay_days, offer_expiry_days,
              education_required_score_bp, education_weight_bp,
              certification_required_score_bp, certification_weight_bp,
              language_required_score_bp, language_weight_bp,
              training_required_score_bp, training_weight_bp,
              experience_required_score_bp, experience_weight_bp,
              project_required_score_bp, project_weight_bp)
         SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, employer.region,
                template.employment_type, platform.competition_band,
                template.minimum_education, template.required_certification_entry_id,
                template.minimum_experience_days, template.military_requirement,
                EXISTS(SELECT 1 FROM platform_artifact_requirement requirement
                       WHERE requirement.career_catalog_bundle_id = template.career_catalog_bundle_id
                         AND requirement.platform_catalog_id = platform.id
                         AND requirement.artifact_kind = 'resume'),
                EXISTS(SELECT 1 FROM platform_artifact_requirement requirement
                       WHERE requirement.career_catalog_bundle_id = template.career_catalog_bundle_id
                         AND requirement.platform_catalog_id = platform.id
                         AND requirement.artifact_kind = 'portfolio'),
                EXISTS(SELECT 1 FROM platform_artifact_requirement requirement
                       WHERE requirement.career_catalog_bundle_id = template.career_catalog_bundle_id
                         AND requirement.platform_catalog_id = platform.id
                         AND requirement.artifact_kind = 'linkedinProfile'),
                template.minimum_annual_salary_krw, template.maximum_annual_salary_krw,
                template.salary_step_krw, platform.document_review_days,
                template.interview_delay_days, template.offer_expiry_days,
                ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
         FROM job_template AS template
         INNER JOIN platform_catalog AS platform
           ON platform.career_catalog_bundle_id = template.career_catalog_bundle_id
          AND platform.id = template.platform_catalog_id
         INNER JOIN virtual_employer AS employer
           ON employer.career_catalog_bundle_id = template.career_catalog_bundle_id
          AND employer.id = template.virtual_employer_id
         WHERE template.career_catalog_bundle_id = ? AND template.id = ?",
    )
    .bind(&posting.posting_key)
    .bind(scope.market_world_id)
    .bind(scope.career_catalog_bundle_id)
    .bind(recruitment_ruleset_id)
    .bind(platform_catalog_id)
    .bind(selected.id)
    .bind(selected.career_industry_id)
    .bind(selected.career_job_family_id)
    .bind(selected.virtual_employer_id)
    .bind(slot_no)
    .bind(posting.posted_game_day)
    .bind(posting.closes_exclusive_game_day)
    .bind(education.0).bind(education.1)
    .bind(certification.0).bind(certification.1)
    .bind(language.0).bind(language.1)
    .bind(training.0).bind(training.1)
    .bind(experience.0).bind(experience.1)
    .bind(project.0).bind(project.1)
    .bind(scope.career_catalog_bundle_id)
    .bind(selected.id)
    .execute(&mut **tx)
    .await?;
    let stored: Option<(String,)> = sqlx::query_as(
        "SELECT posting_key FROM job_posting
         WHERE market_world_id = ? AND career_catalog_bundle_id = ?
           AND posted_game_day = ? AND platform_catalog_id = ? AND slot_no = ?",
    )
    .bind(scope.market_world_id)
    .bind(scope.career_catalog_bundle_id)
    .bind(posting.posted_game_day)
    .bind(platform_catalog_id)
    .bind(slot_no)
    .fetch_optional(&mut **tx)
    .await?;
    ensure!(
        stored.is_some(),
        "job posting did not converge to an immutable occurrence"
    );
    Ok(())
}

fn enum_from_db<T: DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_value(Value::String(value.to_owned()))
        .with_context(|| format!("stored enum value is invalid: {value}"))
}

fn enum_to_db<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value)? {
        Value::String(value) => Ok(value),
        _ => bail!("domain enum did not serialize as a string"),
    }
}

async fn read_scope_for_user(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
) -> Result<RecruitmentScopeRow> {
    sqlx::query_as(
        "SELECT save.id AS save_id, save.market_world_id, world.seed AS world_seed,
                save.run_revision, save.state_revision, save.game_day, save.cash_krw,
                career_run.career_catalog_bundle_id,
                `character`.region AS character_region,
                `character`.education AS character_education,
                `character`.military AS character_military
         FROM save
         INNER JOIN `character` ON `character`.save_id = save.id
         INNER JOIN career_run
           ON career_run.save_id = save.id
          AND career_run.run_revision = save.run_revision
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         WHERE save.user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("recruitment state requires an active character")
}

async fn lock_scope_for_user(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
) -> Result<Option<RecruitmentScopeRow>> {
    sqlx::query_as(
        "SELECT save.id AS save_id, save.market_world_id, world.seed AS world_seed,
                save.run_revision, save.state_revision, save.game_day, save.cash_krw,
                career_run.career_catalog_bundle_id,
                `character`.region AS character_region,
                `character`.education AS character_education,
                `character`.military AS character_military
         FROM save
         LEFT JOIN `character` ON `character`.save_id = save.id
         LEFT JOIN career_run
           ON career_run.save_id = save.id
          AND career_run.run_revision = save.run_revision
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         WHERE save.user_id = ?
           AND `character`.save_id IS NOT NULL
           AND career_run.save_id IS NOT NULL
         FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock recruitment command save")
}

const POSTING_SELECT: &str =
    "SELECT posting.id, posting.posting_key, posting.market_world_id,
            world.seed AS world_seed, calibration.version AS world_model_version,
            posting.career_catalog_bundle_id,
            bundle.bundle_key AS career_catalog_bundle_key,
            posting.recruitment_ruleset_id,
            ruleset.ruleset_key AS recruitment_ruleset_key,
            posting.platform_catalog_id, platform.platform_key,
            template.template_key, employer.employer_key,
            employer.display_name AS employer_name, industry.industry_key,
            family.job_family_key, posting.region, posting.employment_type,
            posting.posted_game_day, posting.closes_exclusive_game_day,
            posting.competition_band, posting.document_review_days,
            platform.same_region_only, posting.requires_resume,
            posting.requires_portfolio, posting.requires_linkedin_profile,
            platform.first_pay_reward_krw, posting.minimum_education,
            certification.entry_key AS required_certification_entry_key,
            certification.display_name AS required_certification_name,
            posting.minimum_experience_days, posting.military_requirement,
            posting.minimum_annual_salary_krw, posting.maximum_annual_salary_krw,
            posting.salary_step_krw, posting.interview_delay_days,
            posting.offer_expiry_days,
            CAST((
                SELECT weight.weight_bp * 10000 DIV maximum.maximum_weight_bp
                FROM platform_industry_weight AS weight
                INNER JOIN (
                    SELECT platform_catalog_id, MAX(weight_bp) AS maximum_weight_bp
                    FROM platform_industry_weight
                    WHERE career_catalog_bundle_id = posting.career_catalog_bundle_id
                    GROUP BY platform_catalog_id
                ) AS maximum
                  ON maximum.platform_catalog_id = weight.platform_catalog_id
                WHERE weight.career_catalog_bundle_id = posting.career_catalog_bundle_id
                  AND weight.platform_catalog_id = posting.platform_catalog_id
                  AND weight.career_industry_id = posting.career_industry_id
            ) AS SIGNED) AS platform_affinity_bp,
            posting.education_required_score_bp, posting.education_weight_bp,
            posting.certification_required_score_bp, posting.certification_weight_bp,
            posting.language_required_score_bp, posting.language_weight_bp,
            posting.training_required_score_bp, posting.training_weight_bp,
            posting.experience_required_score_bp, posting.experience_weight_bp,
            posting.project_required_score_bp, posting.project_weight_bp
     FROM job_posting AS posting
     INNER JOIN market_world AS world ON world.id = posting.market_world_id
     INNER JOIN market_calibration AS calibration ON calibration.id = world.calibration_id
     INNER JOIN career_catalog_bundle AS bundle
       ON bundle.id = posting.career_catalog_bundle_id
     INNER JOIN recruitment_ruleset AS ruleset
       ON ruleset.id = posting.recruitment_ruleset_id
     INNER JOIN platform_catalog AS platform
       ON platform.career_catalog_bundle_id = posting.career_catalog_bundle_id
      AND platform.id = posting.platform_catalog_id
     INNER JOIN job_template AS template
       ON template.career_catalog_bundle_id = posting.career_catalog_bundle_id
      AND template.id = posting.job_template_id
     INNER JOIN virtual_employer AS employer
       ON employer.career_catalog_bundle_id = posting.career_catalog_bundle_id
      AND employer.id = posting.virtual_employer_id
     INNER JOIN career_industry AS industry
       ON industry.career_catalog_bundle_id = posting.career_catalog_bundle_id
      AND industry.id = posting.career_industry_id
     INNER JOIN career_job_family AS family
       ON family.career_catalog_bundle_id = posting.career_catalog_bundle_id
      AND family.id = posting.career_job_family_id
     LEFT JOIN spec_catalog_entry AS certification
       ON certification.career_catalog_bundle_id = posting.career_catalog_bundle_id
      AND certification.id = posting.required_certification_entry_id
     WHERE posting.market_world_id = ?
       AND posting.career_catalog_bundle_id = ?
       AND (? IS NULL OR posting.id = ?)
       AND (? IS NULL OR BINARY posting.posting_key = BINARY ?)
       AND (? IS NULL OR posting.posted_game_day <= ?)
       AND (? IS NULL OR posting.closes_exclusive_game_day > ?)
       AND (? IS NULL OR BINARY posting.posting_key < BINARY ?)
       AND (? IS NULL OR BINARY platform.platform_key = BINARY ?)
       AND (? IS NULL OR BINARY industry.industry_key = BINARY ?)
     ORDER BY posting.posting_key DESC
     LIMIT ?";

async fn read_posting_by_key(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
    posting_key: &str,
) -> Result<Option<PostingRow>> {
    sqlx::query_as(POSTING_SELECT)
        .bind(scope.market_world_id)
        .bind(scope.career_catalog_bundle_id)
        .bind(Option::<u64>::None)
        .bind(Option::<u64>::None)
        .bind(Some(posting_key))
        .bind(posting_key)
        .bind(Option::<u32>::None)
        .bind(Option::<u32>::None)
        .bind(Option::<u32>::None)
        .bind(Option::<u32>::None)
        .bind(Option::<&str>::None)
        .bind(Option::<&str>::None)
        .bind(Option::<&str>::None)
        .bind(Option::<&str>::None)
        .bind(Option::<&str>::None)
        .bind(Option::<&str>::None)
        .bind(1_u32)
        .fetch_optional(&mut **tx)
        .await
        .context("failed to read a recruitment posting")
}

async fn read_posting_by_id(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
    posting_id: u64,
) -> Result<Option<PostingRow>> {
    sqlx::query_as(POSTING_SELECT)
        .bind(scope.market_world_id)
        .bind(scope.career_catalog_bundle_id)
        .bind(Some(posting_id))
        .bind(posting_id)
        .bind(Option::<&str>::None)
        .bind(Option::<&str>::None)
        .bind(Option::<u32>::None)
        .bind(Option::<u32>::None)
        .bind(Option::<u32>::None)
        .bind(Option::<u32>::None)
        .bind(Option::<&str>::None)
        .bind(Option::<&str>::None)
        .bind(Option::<&str>::None)
        .bind(Option::<&str>::None)
        .bind(Option::<&str>::None)
        .bind(Option::<&str>::None)
        .bind(1_u32)
        .fetch_optional(&mut **tx)
        .await
        .context("failed to read a recruitment posting")
}

fn posting_from_row(row: &PostingRow) -> Result<MaterializedPosting> {
    let mut required_artifacts = Vec::new();
    if row.requires_resume {
        required_artifacts.push(ArtifactKind::Resume);
    }
    if row.requires_portfolio {
        required_artifacts.push(ArtifactKind::Portfolio);
    }
    if row.requires_linkedin_profile {
        required_artifacts.push(ArtifactKind::LinkedinProfile);
    }
    Ok(MaterializedPosting {
        posting_key: row.posting_key.clone(),
        world_model_version: row.world_model_version.clone(),
        career_catalog_bundle_key: row.career_catalog_bundle_key.clone(),
        recruitment_ruleset_key: row.recruitment_ruleset_key.clone(),
        platform: enum_from_db(&row.platform_key)?,
        template_key: row.template_key.clone(),
        employer_key: row.employer_key.clone(),
        employer_name: row.employer_name.clone(),
        industry: enum_from_db(&row.industry_key)?,
        job_family_key: row.job_family_key.clone(),
        region: enum_from_db(&row.region)?,
        employment_type: enum_from_db(&row.employment_type)?,
        posted_game_day: row.posted_game_day,
        closes_exclusive_game_day: row.closes_exclusive_game_day,
        competition_band: enum_from_db(&row.competition_band)?,
        document_review_days: row.document_review_days,
        same_region_only: row.same_region_only,
        required_artifacts,
        first_pay_reward_krw: row.first_pay_reward_krw,
        minimum_education: row.minimum_education.as_deref().map(enum_from_db).transpose()?,
        required_certification_entry_key: row.required_certification_entry_key.clone(),
        minimum_experience_days: row.minimum_experience_days,
        military_requirement: enum_from_db(&row.military_requirement)?,
        minimum_annual_salary_krw: row.minimum_annual_salary_krw,
        maximum_annual_salary_krw: row.maximum_annual_salary_krw,
        salary_step_krw: row.salary_step_krw,
        interview_delay_days: row.interview_delay_days,
        offer_expiry_days: row.offer_expiry_days,
        requirements: vec![
            DimensionRequirement { dimension: SpecDimension::Education, required_score_bp: row.education_required_score_bp, weight_bp: row.education_weight_bp },
            DimensionRequirement { dimension: SpecDimension::Certification, required_score_bp: row.certification_required_score_bp, weight_bp: row.certification_weight_bp },
            DimensionRequirement { dimension: SpecDimension::Language, required_score_bp: row.language_required_score_bp, weight_bp: row.language_weight_bp },
            DimensionRequirement { dimension: SpecDimension::Training, required_score_bp: row.training_required_score_bp, weight_bp: row.training_weight_bp },
            DimensionRequirement { dimension: SpecDimension::Experience, required_score_bp: row.experience_required_score_bp, weight_bp: row.experience_weight_bp },
            DimensionRequirement { dimension: SpecDimension::Project, required_score_bp: row.project_required_score_bp, weight_bp: row.project_weight_bp },
        ],
        platform_affinity_bp: row.platform_affinity_bp,
    })
}

async fn read_score_inputs(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    bundle_id: u64,
) -> Result<(Vec<SpecEvidence>, Vec<SpecCatalogEntry>)> {
    let evidence_rows: Vec<EvidenceDbRow> = sqlx::query_as(
        "SELECT evidence.id, evidence.evidence_key, catalog.entry_key AS catalog_entry_key,
                evidence.kind, evidence.acquired_game_day, evidence.expires_on_game_day,
                evidence.period_start_date, evidence.period_end_exclusive_date,
                evidence.source_kind
         FROM spec_evidence AS evidence
         INNER JOIN spec_catalog_entry AS catalog
           ON catalog.career_catalog_bundle_id = evidence.career_catalog_bundle_id
          AND catalog.id = evidence.spec_catalog_entry_id
         WHERE evidence.save_id = ? AND evidence.run_revision = ?
         ORDER BY evidence.acquired_game_day, evidence.id",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    let evidence = evidence_rows
        .into_iter()
        .map(|row| {
            let period = match (row.period_start_date, row.period_end_exclusive_date) {
                (None, None) => EvidencePeriodFields::none(),
                (Some(start), Some(end)) if row.source_kind == "bridgeExperience" && start == end => {
                    EvidencePeriodFields::zero_year_bridge(start)
                }
                (Some(start), Some(end)) => EvidencePeriodFields::regular(start, end),
                _ => bail!("stored recruitment evidence has an incomplete period"),
            };
            Ok(SpecEvidence {
                evidence_id: row.id,
                evidence_key: row.evidence_key,
                catalog_entry_key: row.catalog_entry_key,
                kind: enum_from_db(&row.kind)?,
                acquired_game_day: row.acquired_game_day,
                expires_on_game_day: row.expires_on_game_day,
                period,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let contribution_rows: Vec<CatalogContributionDbRow> = sqlx::query_as(
        "SELECT entry.entry_key, entry.kind, entry.stackable,
                family.job_family_key, contribution.contribution_bp
         FROM spec_catalog_entry AS entry
         INNER JOIN spec_catalog_contribution AS contribution
           ON contribution.career_catalog_bundle_id = entry.career_catalog_bundle_id
          AND contribution.spec_catalog_entry_id = entry.id
         INNER JOIN career_job_family AS family
           ON family.career_catalog_bundle_id = contribution.career_catalog_bundle_id
          AND family.id = contribution.career_job_family_id
         WHERE entry.career_catalog_bundle_id = ?
         ORDER BY entry.id, family.id",
    )
    .bind(bundle_id)
    .fetch_all(&mut **tx)
    .await?;
    let mut grouped = BTreeMap::<String, SpecCatalogEntry>::new();
    for row in contribution_rows {
        let kind = enum_from_db(&row.kind)?;
        let entry = grouped.entry(row.entry_key.clone()).or_insert_with(|| SpecCatalogEntry {
            catalog_entry_key: row.entry_key,
            kind,
            stackable: row.stackable,
            contributions: Vec::new(),
        });
        ensure!(entry.kind == kind && entry.stackable == row.stackable, "spec catalog row drifted");
        entry.contributions.push(JobFamilyContribution {
            job_family_key: row.job_family_key,
            contribution_bp: row.contribution_bp,
        });
    }
    Ok((evidence, grouped.into_values().collect()))
}

fn calculate_scores(
    game_day: u32,
    job_family_key: &str,
    visible_evidence_ids: &[u64],
    evidence: &[SpecEvidence],
    catalog: &[SpecCatalogEntry],
) -> Result<(DimensionScores, DimensionScores)> {
    let views = create_spec_score_rules().calculate_score_views(SpecScoreInput {
        evaluated_job_family_key: job_family_key,
        current_game_day: game_day,
        evidence,
        catalog,
        visible_evidence_ids,
    })?;
    Ok((views.possessed, views.visible))
}

pub(super) async fn read_career_jobs(
    pool: &MySqlPool,
    user_id: u64,
    query: CareerJobsPageQuery,
) -> Result<CareerJobsPageState> {
    validate_jobs_query(&query)?;
    let current_game_day: u32 = sqlx::query_scalar(
        "SELECT game_day FROM save WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .context("career jobs require a save")?;
    ensure_recruitment_postings_for_user(pool, user_id, current_game_day).await?;

    let mut tx = pool.begin().await?;
    let scope = read_scope_for_user(&mut tx, user_id).await?;
    let platform = query.platform.map(|value| enum_to_db(&value)).transpose()?;
    let industry = query.industry.map(|value| enum_to_db(&value)).transpose()?;
    let rows: Vec<PostingRow> = sqlx::query_as(POSTING_SELECT)
        .bind(scope.market_world_id)
        .bind(scope.career_catalog_bundle_id)
        .bind(Option::<u64>::None)
        .bind(Option::<u64>::None)
        .bind(Option::<&str>::None)
        .bind(Option::<&str>::None)
        .bind(Some(scope.game_day))
        .bind(scope.game_day)
        .bind(Some(scope.game_day))
        .bind(scope.game_day)
        .bind(query.before.as_deref())
        .bind(query.before.as_deref())
        .bind(platform.as_deref())
        .bind(platform.as_deref())
        .bind(industry.as_deref())
        .bind(industry.as_deref())
        .bind(query.limit.checked_add(1).context("career jobs page limit overflowed")?)
        .fetch_all(&mut *tx)
        .await?;
    let (evidence, catalog) = read_score_inputs(
        &mut tx,
        scope.save_id,
        scope.run_revision,
        scope.career_catalog_bundle_id,
    )
    .await?;
    let has_more = rows.len() > query.limit as usize;
    let mut items = Vec::with_capacity(rows.len().min(query.limit as usize));
    for row in rows.into_iter().take(query.limit as usize) {
        let posting = posting_from_row(&row)?;
        let (possessed_scores, _) = calculate_scores(
            scope.game_day,
            &posting.job_family_key,
            &[],
            &evidence,
            &catalog,
        )?;
        items.push(CareerJobState {
            posting_key: posting.posting_key,
            posted_game_day: posting.posted_game_day,
            closes_exclusive_game_day: posting.closes_exclusive_game_day,
            platform: posting.platform,
            industry: posting.industry,
            job_family_key: posting.job_family_key,
            employer_name: posting.employer_name,
            region: enum_to_db(&posting.region)?,
            employment_type: enum_to_db(&posting.employment_type)?,
            required_scores: required_scores(&posting.requirements)?,
            possessed_scores,
            minimum_annual_salary_krw: posting.minimum_annual_salary_krw,
            maximum_annual_salary_krw: posting.maximum_annual_salary_krw,
            salary_step_krw: posting.salary_step_krw,
            competition_band: posting.competition_band,
            military_requirement: posting.military_requirement,
            minimum_education: posting.minimum_education.map(|value| enum_to_db(&value)).transpose()?,
            required_certification_name: row.required_certification_name,
            minimum_experience_days: posting.minimum_experience_days,
            required_artifacts: posting.required_artifacts,
        });
    }
    let next_before = has_more.then(|| items.last().map(|item| item.posting_key.clone())).flatten();
    tx.commit().await?;
    Ok(CareerJobsPageState { items, next_before })
}

fn required_scores(requirements: &[DimensionRequirement]) -> Result<DimensionScores> {
    let mut values = HashMap::new();
    for requirement in requirements {
        ensure!(values.insert(requirement.dimension, requirement.required_score_bp).is_none(), "duplicate posting requirement");
    }
    Ok(DimensionScores {
        education: *values.get(&SpecDimension::Education).context("education requirement is missing")?,
        certification: *values.get(&SpecDimension::Certification).context("certification requirement is missing")?,
        language: *values.get(&SpecDimension::Language).context("language requirement is missing")?,
        training: *values.get(&SpecDimension::Training).context("training requirement is missing")?,
        experience: *values.get(&SpecDimension::Experience).context("experience requirement is missing")?,
        project: *values.get(&SpecDimension::Project).context("project requirement is missing")?,
    })
}

fn validate_jobs_query(query: &CareerJobsPageQuery) -> Result<()> {
    validate_page_limit(query.limit)?;
    if let Some(before) = &query.before {
        ensure!(before.len() == 64 && before.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)), "career job cursor is invalid");
    }
    Ok(())
}

fn validate_page_limit(limit: u32) -> Result<()> {
    ensure!((1..=MAX_PAGE_LIMIT).contains(&limit), "career page limit must be between 1 and {MAX_PAGE_LIMIT}");
    Ok(())
}

const APPLICATION_SELECT: &str =
    "SELECT application.id, posting.posting_key, platform.platform_key,
            industry.industry_key, employer.display_name AS employer_name,
            family.job_family_key, application.source_kind, application.status,
            application.submitted_game_day,
            application.visible_education_score_bp,
            application.visible_certification_score_bp,
            application.visible_language_score_bp,
            application.visible_training_score_bp,
            application.visible_experience_score_bp,
            application.visible_project_score_bp,
            application.possessed_education_score_bp,
            application.possessed_certification_score_bp,
            application.possessed_language_score_bp,
            application.possessed_training_score_bp,
            application.possessed_experience_score_bp,
            application.possessed_project_score_bp,
            application.document_score_bp, application.document_decided_game_day,
            application.interview_game_day,
            application.confirmation_expires_exclusive_game_day,
            application.interview_score_bp,
            offer.id AS offer_id, offer.status AS offer_status,
            offer.annual_salary_krw, offer.payday_day_of_month,
            offer.start_game_day, offer.expires_exclusive_game_day,
            offer.first_pay_reward_krw
     FROM job_application AS application
     INNER JOIN job_posting AS posting
       ON posting.career_catalog_bundle_id = application.career_catalog_bundle_id
      AND posting.id = application.job_posting_id
     INNER JOIN platform_catalog AS platform
       ON platform.career_catalog_bundle_id = posting.career_catalog_bundle_id
      AND platform.id = posting.platform_catalog_id
     INNER JOIN career_industry AS industry
       ON industry.career_catalog_bundle_id = posting.career_catalog_bundle_id
      AND industry.id = posting.career_industry_id
     INNER JOIN career_job_family AS family
       ON family.career_catalog_bundle_id = posting.career_catalog_bundle_id
      AND family.id = posting.career_job_family_id
     INNER JOIN virtual_employer AS employer
       ON employer.career_catalog_bundle_id = posting.career_catalog_bundle_id
      AND employer.id = posting.virtual_employer_id
     LEFT JOIN job_offer AS offer
       ON offer.save_id = application.save_id
      AND offer.run_revision = application.run_revision
      AND offer.job_application_id = application.id
     WHERE application.save_id = ? AND application.run_revision = ?
       AND (? IS NULL OR application.id < ?)
       AND (? = FALSE OR application.status IN (
           'submitted', 'interviewAwaitingConfirmation', 'interviewConfirmed', 'offered'
       ))
     ORDER BY application.id DESC
     LIMIT ?";

async fn read_application_rows(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    before: Option<u64>,
    limit: u32,
    open_only: bool,
) -> Result<Vec<ApplicationReadRow>> {
    sqlx::query_as(APPLICATION_SELECT)
        .bind(save_id)
        .bind(run_revision)
        .bind(before)
        .bind(before)
        .bind(open_only)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await
        .context("failed to read recruitment applications")
}

async fn read_invitation_rows(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    open_only: bool,
    limit: u32,
) -> Result<Vec<InvitationReadRow>> {
    sqlx::query_as(
        "SELECT invitation.id, posting.posting_key, platform.platform_key,
                industry.industry_key, family.job_family_key,
                employer.display_name AS employer_name,
                invitation.profile_artifact_version_id,
                invitation.invitation_game_day,
                invitation.expires_exclusive_game_day, invitation.status
         FROM job_invitation AS invitation
         INNER JOIN job_posting AS posting
           ON posting.career_catalog_bundle_id = invitation.career_catalog_bundle_id
          AND posting.id = invitation.job_posting_id
         INNER JOIN platform_catalog AS platform
           ON platform.career_catalog_bundle_id = posting.career_catalog_bundle_id
          AND platform.id = posting.platform_catalog_id
         INNER JOIN career_industry AS industry
           ON industry.career_catalog_bundle_id = posting.career_catalog_bundle_id
          AND industry.id = posting.career_industry_id
         INNER JOIN career_job_family AS family
           ON family.career_catalog_bundle_id = posting.career_catalog_bundle_id
          AND family.id = posting.career_job_family_id
         INNER JOIN virtual_employer AS employer
           ON employer.career_catalog_bundle_id = posting.career_catalog_bundle_id
          AND employer.id = posting.virtual_employer_id
         WHERE invitation.save_id = ? AND invitation.run_revision = ?
           AND (? = FALSE OR invitation.status = 'open')
         ORDER BY invitation.id DESC
         LIMIT ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(open_only)
    .bind(limit)
    .fetch_all(&mut **tx)
    .await
    .context("failed to read recruitment invitations")
}

fn invitation_state(row: InvitationReadRow) -> Result<CareerInvitationState> {
    Ok(CareerInvitationState {
        id: ResourceId::from_u64(row.id),
        posting_key: row.posting_key,
        platform: enum_from_db(&row.platform_key)?,
        industry: enum_from_db(&row.industry_key)?,
        job_family_key: row.job_family_key,
        employer_name: row.employer_name,
        artifact_version_id: ResourceId::from_u64(row.profile_artifact_version_id),
        created_game_day: row.invitation_game_day,
        expires_exclusive_game_day: row.expires_exclusive_game_day,
        status: enum_from_db(&row.status)?,
    })
}

async fn hydrate_application_states(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
    rows: Vec<ApplicationReadRow>,
) -> Result<Vec<CareerApplicationState>> {
    let (evidence, catalog) = read_score_inputs(
        tx,
        scope.save_id,
        scope.run_revision,
        scope.career_catalog_bundle_id,
    )
    .await?;
    let mut score_cache = HashMap::<String, DimensionScores>::new();
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let visible_scores = DimensionScores {
            education: row.visible_education_score_bp,
            certification: row.visible_certification_score_bp,
            language: row.visible_language_score_bp,
            training: row.visible_training_score_bp,
            experience: row.visible_experience_score_bp,
            project: row.visible_project_score_bp,
        };
        let possessed_scores = match (
            row.possessed_education_score_bp,
            row.possessed_certification_score_bp,
            row.possessed_language_score_bp,
            row.possessed_training_score_bp,
            row.possessed_experience_score_bp,
            row.possessed_project_score_bp,
        ) {
            (Some(education), Some(certification), Some(language), Some(training), Some(experience), Some(project)) => DimensionScores {
                education, certification, language, training, experience, project,
            },
            (None, None, None, None, None, None) => {
                if let Some(scores) = score_cache.get(&row.job_family_key) {
                    *scores
                } else {
                    let (scores, _) = calculate_scores(scope.game_day, &row.job_family_key, &[], &evidence, &catalog)?;
                    score_cache.insert(row.job_family_key.clone(), scores);
                    scores
                }
            }
            _ => bail!("stored application has partial possessed scores"),
        };
        let offer = match (
            row.offer_id,
            row.offer_status.as_deref(),
            row.annual_salary_krw,
            row.payday_day_of_month,
            row.start_game_day,
            row.expires_exclusive_game_day,
            row.first_pay_reward_krw,
        ) {
            (None, None, None, None, None, None, None) => None,
            (Some(id), Some(status), Some(annual_salary_krw), Some(payday_day_of_month), Some(start_game_day), Some(expires_exclusive_game_day), Some(wanted_reward_krw)) => Some(CareerOfferState {
                id: ResourceId::from_u64(id),
                status: offer_application_status(status)?,
                annual_salary_krw,
                payday_day_of_month,
                start_game_day,
                expires_exclusive_game_day,
                wanted_reward_krw,
            }),
            _ => bail!("stored application has a partial offer"),
        };
        items.push(CareerApplicationState {
            id: ResourceId::from_u64(row.id),
            posting_key: row.posting_key,
            platform: enum_from_db(&row.platform_key)?,
            industry: enum_from_db(&row.industry_key)?,
            employer_name: row.employer_name,
            job_family_key: row.job_family_key,
            source: enum_from_db(&row.source_kind)?,
            status: enum_from_db(&row.status)?,
            submitted_game_day: row.submitted_game_day,
            visible_scores,
            possessed_scores,
            document_score_bp: row.document_score_bp,
            document_decision_game_day: row.document_decided_game_day,
            interview_game_day: row.interview_game_day,
            confirmation_deadline_exclusive_game_day: row.confirmation_expires_exclusive_game_day,
            interview_score_bp: row.interview_score_bp,
            offer,
        });
    }
    Ok(items)
}

fn offer_application_status(value: &str) -> Result<CareerOfferStatus> {
    Ok(match value {
        "pending" => CareerOfferStatus::Offered,
        "accepted" => CareerOfferStatus::Accepted,
        "declined" => CareerOfferStatus::Declined,
        "expired" => CareerOfferStatus::Expired,
        "closed" => CareerOfferStatus::Closed,
        _ => bail!("stored offer status is invalid"),
    })
}

pub(super) async fn read_career_applications(
    pool: &MySqlPool,
    user_id: u64,
    query: CareerPageQuery,
) -> Result<CareerApplicationsPageState> {
    validate_page_limit(query.limit)?;
    ensure!(query.before != Some(0), "career application cursor must be positive");
    let mut tx = pool.begin().await?;
    let scope = read_scope_for_user(&mut tx, user_id).await?;
    let rows = read_application_rows(
        &mut tx,
        scope.save_id,
        scope.run_revision,
        query.before,
        query.limit.checked_add(1).context("career application page limit overflowed")?,
        false,
    )
    .await?;
    let has_more = rows.len() > query.limit as usize;
    let items = hydrate_application_states(
        &mut tx,
        &scope,
        rows.into_iter().take(query.limit as usize).collect(),
    )
    .await?;
    let next_before = has_more.then(|| items.last().map(|item| item.id)).flatten();
    let invitation_rows = read_invitation_rows(&mut tx, scope.save_id, scope.run_revision, true, 6).await?;
    ensure!(invitation_rows.len() <= 5, "open invitation bound was exceeded");
    let open_invitations = invitation_rows.into_iter().map(invitation_state).collect::<Result<Vec<_>>>()?;
    tx.commit().await?;
    Ok(CareerApplicationsPageState { items, next_before, open_invitations })
}

async fn read_employment_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<Option<EmploymentContractState>> {
    let rows: Vec<EmploymentReadRow> = sqlx::query_as(
        "SELECT contract.id, contract.status, family.job_family_key,
                employer.display_name AS employer_name, contract.region,
                contract.annual_salary_krw, contract.payday_day_of_month,
                contract.start_game_day, contract.end_game_day,
                contract.credited_experience_days
         FROM employment_contract AS contract
         INNER JOIN career_job_family AS family
           ON family.career_catalog_bundle_id = contract.career_catalog_bundle_id
          AND family.id = contract.career_job_family_id
         INNER JOIN virtual_employer AS employer
           ON employer.career_catalog_bundle_id = contract.career_catalog_bundle_id
          AND employer.id = contract.virtual_employer_id
         WHERE contract.save_id = ? AND contract.run_revision = ?
           AND contract.status IN ('pendingStart', 'active')
         ORDER BY contract.id DESC
         LIMIT 2",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(rows.len() <= 1, "multiple current employment contracts exist");
    rows.into_iter().next().map(|row| {
        Ok(EmploymentContractState {
            id: ResourceId::from_u64(row.id),
            status: enum_from_db(&row.status)?,
            job_family_key: row.job_family_key,
            employer_name: row.employer_name,
            region: row.region,
            annual_salary_krw: row.annual_salary_krw,
            payday_day_of_month: row.payday_day_of_month,
            start_game_day: row.start_game_day,
            end_game_day: row.end_game_day,
            credited_experience_days: row.credited_experience_days,
        })
    }).transpose()
}

pub(super) async fn read_career_employment(
    pool: &MySqlPool,
    user_id: u64,
) -> Result<CareerEmploymentState> {
    let mut tx = pool.begin().await?;
    let scope = read_scope_for_user(&mut tx, user_id).await?;
    let contract = read_employment_in_tx(&mut tx, scope.save_id, scope.run_revision).await?;
    tx.commit().await?;
    Ok(CareerEmploymentState { contract })
}

pub(super) async fn read_recruitment_snapshot_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
) -> Result<(Vec<CareerApplicationState>, Vec<CareerInvitationState>, Option<EmploymentContractState>)> {
    let scope: Option<RecruitmentScopeRow> = sqlx::query_as(
        "SELECT save.id AS save_id, save.market_world_id, world.seed AS world_seed,
                save.run_revision, save.state_revision, ? AS game_day, save.cash_krw,
                career_run.career_catalog_bundle_id,
                `character`.region AS character_region,
                `character`.education AS character_education,
                `character`.military AS character_military
         FROM save
         INNER JOIN `character` ON `character`.save_id = save.id
         INNER JOIN career_run
           ON career_run.save_id = save.id AND career_run.run_revision = save.run_revision
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         WHERE save.id = ? AND save.run_revision = ?",
    )
    .bind(game_day)
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(scope) = scope else {
        return Ok((Vec::new(), Vec::new(), None));
    };
    let rows = read_application_rows(tx, save_id, run_revision, None, 11, true).await?;
    ensure!(rows.len() <= 10, "open application bound was exceeded");
    let applications = hydrate_application_states(tx, &scope, rows).await?;
    let invitation_rows = read_invitation_rows(tx, save_id, run_revision, true, 6).await?;
    ensure!(invitation_rows.len() <= 5, "open invitation bound was exceeded");
    let invitations = invitation_rows.into_iter().map(invitation_state).collect::<Result<Vec<_>>>()?;
    let employment = read_employment_in_tx(tx, save_id, run_revision).await?;
    Ok((applications, invitations, employment))
}

struct CandidateDbProfile {
    region: Region,
    education: Education,
    military_qualification: MilitaryQualification,
    valid_catalog_entry_keys: Vec<String>,
    experience_days: u32,
    has_active_or_pending_contract: bool,
}

async fn read_candidate_profile(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
) -> Result<CandidateDbProfile> {
    let valid_catalog_entry_keys: Vec<String> = sqlx::query_scalar(
        "SELECT catalog.entry_key
         FROM spec_evidence AS evidence
         INNER JOIN spec_catalog_entry AS catalog
           ON catalog.career_catalog_bundle_id = evidence.career_catalog_bundle_id
          AND catalog.id = evidence.spec_catalog_entry_id
         WHERE evidence.save_id = ? AND evidence.run_revision = ?
           AND evidence.acquired_game_day <= ?
           AND (evidence.expires_on_game_day IS NULL OR evidence.expires_on_game_day >= ?)
         ORDER BY catalog.entry_key",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.game_day)
    .bind(scope.game_day)
    .fetch_all(&mut **tx)
    .await?;
    let experience_days: u64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(
             CASE
               WHEN evidence.kind = 'experience'
                AND evidence.period_start_date IS NOT NULL
                AND evidence.period_end_exclusive_date IS NOT NULL
               THEN DATEDIFF(evidence.period_end_exclusive_date, evidence.period_start_date)
               ELSE 0
             END
         ), 0)
         FROM spec_evidence AS evidence
         WHERE evidence.save_id = ? AND evidence.run_revision = ?
           AND evidence.acquired_game_day <= ?
           AND (evidence.expires_on_game_day IS NULL OR evidence.expires_on_game_day >= ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.game_day)
    .bind(scope.game_day)
    .fetch_one(&mut **tx)
    .await?;
    let has_active_or_pending_contract: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM employment_contract
             WHERE save_id = ? AND run_revision = ?
               AND status IN ('pendingStart', 'active')
         )",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_one(&mut **tx)
    .await?;
    let military: MilitaryStatus = enum_from_db(&scope.character_military)?;
    let military_qualification = match military {
        MilitaryStatus::NotServed => MilitaryQualification::Pending,
        MilitaryStatus::Serving | MilitaryStatus::Alternative => MilitaryQualification::Serving,
        MilitaryStatus::Completed | MilitaryStatus::Exempted => {
            MilitaryQualification::CompletedOrExempt
        }
    };
    Ok(CandidateDbProfile {
        region: enum_from_db(&scope.character_region)?,
        education: enum_from_db(&scope.character_education)?,
        military_qualification,
        valid_catalog_entry_keys,
        experience_days: u32::try_from(experience_days).context("career experience days exceed u32")?,
        has_active_or_pending_contract,
    })
}

fn candidate_domain<'a>(
    candidate: &'a CandidateDbProfile,
    valid_catalog_entry_keys: &'a [&'a str],
) -> CandidateApplicationProfile<'a> {
    CandidateApplicationProfile {
        region: candidate.region,
        life_status: LifeStatus::Unemployed,
        has_active_or_pending_contract: candidate.has_active_or_pending_contract,
        education: candidate.education,
        valid_catalog_entry_keys,
        experience_days: candidate.experience_days,
        military_qualification: candidate.military_qualification,
    }
}

async fn read_artifact(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
    artifact_id: u64,
) -> Result<Option<SubmittedArtifact>> {
    let row: Option<ArtifactDbRow> = sqlx::query_as(
        "SELECT id, artifact_kind, completeness_bp, open_to_work, created_game_day,
                sealed_at IS NOT NULL AS is_public
         FROM profile_artifact_version
         WHERE save_id = ? AND run_revision = ?
           AND career_catalog_bundle_id = ? AND id = ?",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.career_catalog_bundle_id)
    .bind(artifact_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let evidence_ids: Vec<u64> = sqlx::query_scalar(
        "SELECT evidence_id FROM profile_artifact_evidence
         WHERE save_id = ? AND run_revision = ? AND profile_artifact_version_id = ?
         ORDER BY selection_order",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(row.id)
    .fetch_all(&mut **tx)
    .await?;
    let industry_keys: Vec<String> = sqlx::query_scalar(
        "SELECT industry.industry_key
         FROM profile_artifact_industry AS selected
         INNER JOIN career_industry AS industry
           ON industry.career_catalog_bundle_id = selected.career_catalog_bundle_id
          AND industry.id = selected.career_industry_id
         WHERE selected.save_id = ? AND selected.run_revision = ?
           AND selected.profile_artifact_version_id = ?
         ORDER BY selected.selection_order",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(row.id)
    .fetch_all(&mut **tx)
    .await?;
    Ok(Some(SubmittedArtifact {
        artifact_version_id: row.id,
        kind: enum_from_db(&row.artifact_kind)?,
        belongs_to_current_run: true,
        is_public: row.is_public && row.created_game_day <= scope.game_day,
        completeness_bp: row.completeness_bp,
        evidence_ids,
        open_to_work: row.open_to_work.unwrap_or(false),
        industries: industry_keys.into_iter().map(|key| enum_from_db(&key)).collect::<Result<Vec<_>>>()?,
    }))
}

async fn read_command_artifacts(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
    ids: [Option<ResourceId>; 3],
) -> Result<Vec<SubmittedArtifact>> {
    let mut artifacts = Vec::new();
    for id in ids.into_iter().flatten() {
        if let Some(artifact) = read_artifact(tx, scope, id.get()).await? {
            artifacts.push(artifact);
        } else {
            artifacts.push(SubmittedArtifact {
                artifact_version_id: id.get(),
                kind: ArtifactKind::Resume,
                belongs_to_current_run: false,
                is_public: false,
                completeness_bp: 0,
                evidence_ids: Vec::new(),
                open_to_work: false,
                industries: Vec::new(),
            });
        }
    }
    Ok(artifacts)
}

async fn application_counts(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
    posting_id: u64,
) -> Result<(u32, u32, bool)> {
    let (active, today, already): (u64, u64, bool) = sqlx::query_as(
        "SELECT
             (SELECT COUNT(*) FROM job_application
              WHERE save_id = ? AND run_revision = ?
                AND status IN ('submitted', 'interviewAwaitingConfirmation', 'interviewConfirmed', 'offered')),
             (SELECT COUNT(*) FROM job_application
              WHERE save_id = ? AND run_revision = ? AND source_kind = 'direct'
                AND submitted_game_day = ?),
             EXISTS(SELECT 1 FROM job_application
                    WHERE save_id = ? AND run_revision = ? AND job_posting_id = ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.game_day)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(posting_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok((
        u32::try_from(active).context("active application count exceeds u32")?,
        u32::try_from(today).context("daily application count exceeds u32")?,
        already,
    ))
}

fn visible_evidence_ids(artifacts: &[SubmittedArtifact]) -> Vec<u64> {
    let mut ids = artifacts.iter().flat_map(|artifact| artifact.evidence_ids.iter().copied()).collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    ids
}

async fn insert_application(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
    posting: &PostingRow,
    source: ApplicationSource,
    source_invitation_id: Option<u64>,
    artifacts: &[SubmittedArtifact],
    visible_scores: DimensionScores,
    state: &ApplicationState,
) -> Result<u64> {
    let artifact_id = |kind: ArtifactKind| {
        artifacts.iter().find(|artifact| artifact.kind == kind).map(|artifact| artifact.artifact_version_id)
    };
    let (status, document_decided_game_day, confirmation_expires_exclusive_game_day, interview_game_day) = match state {
        ApplicationState::Submitted { .. } => ("submitted", None, None, None),
        ApplicationState::InterviewAwaitingConfirmation {
            confirmation_deadline_exclusive_game_day,
            interview_game_day,
            ..
        } => (
            "interviewAwaitingConfirmation",
            Some(scope.game_day),
            Some(*confirmation_deadline_exclusive_game_day),
            Some(*interview_game_day),
        ),
        _ => bail!("application insert received an invalid initial state"),
    };
    let completeness = if artifacts.is_empty() {
        0
    } else {
        artifacts.iter().try_fold(0_i64, |total, artifact| total.checked_add(artifact.completeness_bp).context("artifact completeness overflowed"))?
            / i64::try_from(artifacts.len()).context("artifact count exceeds i64")?
    };
    let insert = sqlx::query(
        "INSERT INTO job_application
             (save_id, run_revision, career_catalog_bundle_id, recruitment_ruleset_id,
              job_posting_id, application_ordinal, source_kind, source_invitation_id,
              status, submitted_game_day, resume_version_id, portfolio_version_id,
              linkedin_profile_version_id, artifact_completeness_bp,
              visible_education_score_bp, visible_certification_score_bp,
              visible_language_score_bp, visible_training_score_bp,
              visible_experience_score_bp, visible_project_score_bp,
              document_decided_game_day, confirmation_expires_exclusive_game_day,
              interview_game_day)
         VALUES (?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.career_catalog_bundle_id)
    .bind(posting.recruitment_ruleset_id)
    .bind(posting.id)
    .bind(enum_to_db(&source)?)
    .bind(source_invitation_id)
    .bind(status)
    .bind(scope.game_day)
    .bind(artifact_id(ArtifactKind::Resume))
    .bind(artifact_id(ArtifactKind::Portfolio))
    .bind(artifact_id(ArtifactKind::LinkedinProfile))
    .bind(completeness)
    .bind(visible_scores.education)
    .bind(visible_scores.certification)
    .bind(visible_scores.language)
    .bind(visible_scores.training)
    .bind(visible_scores.experience)
    .bind(visible_scores.project)
    .bind(document_decided_game_day)
    .bind(confirmation_expires_exclusive_game_day)
    .bind(interview_game_day)
    .execute(&mut **tx)
    .await?;
    Ok(insert.last_insert_id())
}

async fn insert_application_action(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
    recruitment_ruleset_id: u64,
    application_id: u64,
    action_kind: &str,
    phase_rank: u8,
    due_game_day: u32,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO career_scheduled_action
             (save_id, run_revision, career_catalog_bundle_id, recruitment_ruleset_id,
              action_kind, payload_version, phase_rank, due_game_day, status,
              source_kind, source_id, occurrence, job_application_id)
         VALUES (?, ?, ?, ?, ?, 1, ?, ?, 'pending', ?, ?, 1, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.career_catalog_bundle_id)
    .bind(recruitment_ruleset_id)
    .bind(action_kind)
    .bind(phase_rank)
    .bind(due_game_day)
    .bind(action_kind)
    .bind(application_id)
    .bind(application_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn validate_command_cursor(scope: &RecruitmentScopeRow, cursor: CommandCursor) -> Option<crate::career::CareerFailureCode> {
    (scope.run_revision != cursor.expected_run_revision
        || scope.state_revision != cursor.expected_state_revision
        || scope.game_day != cursor.expected_game_day)
        .then_some(crate::career::CareerFailureCode::SettlementConflict)
}

async fn increment_state_revision(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
) -> Result<GameCommandCursor> {
    let state_revision = scope.state_revision.checked_add(1).context("career state revision overflowed")?;
    let update = sqlx::query(
        "UPDATE save SET state_revision = ?
         WHERE id = ? AND market_world_id = ? AND run_revision = ?
           AND state_revision = ? AND game_day = ? AND cash_krw = ?",
    )
    .bind(state_revision)
    .bind(scope.save_id)
    .bind(scope.market_world_id)
    .bind(scope.run_revision)
    .bind(scope.state_revision)
    .bind(scope.game_day)
    .bind(scope.cash_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(update.rows_affected() == 1, "career save cursor changed under its lock");
    Ok(GameCommandCursor { run_revision: scope.run_revision, state_revision, game_day: scope.game_day })
}

async fn read_receipt<T: DeserializeOwned>(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    command_id: &CommandId,
    command_kind: &str,
    fingerprint: &str,
) -> Result<Option<T>> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT command_kind, payload_sha256, CAST(result AS CHAR)
         FROM command_receipt WHERE save_id = ? AND command_id = ? FOR SHARE",
    )
    .bind(save_id)
    .bind(command_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;
    let Some((stored_kind, stored_fingerprint, result)) = row else {
        return Ok(None);
    };
    ensure!(stored_kind == command_kind && stored_fingerprint == fingerprint, "career receipt disagrees with command identity");
    serde_json::from_str(&result).map(Some).context("career receipt result is invalid")
}

trait ReplayableReceipt {
    fn mark_replayed(&mut self);
}

impl ReplayableReceipt for CareerApplicationReceipt { fn mark_replayed(&mut self) { self.replayed = true; } }
impl ReplayableReceipt for CareerInvitationReceipt { fn mark_replayed(&mut self) { self.replayed = true; } }
impl ReplayableReceipt for CareerOfferReceipt { fn mark_replayed(&mut self) { self.replayed = true; } }

async fn replay_or_conflict<T: DeserializeOwned + ReplayableReceipt>(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
    identity: &CommandIdentitySpec<'_>,
) -> Result<Option<Result<T, crate::career::CareerFailureCode>>> {
    match inspect_command_identity(tx, scope.save_id, identity).await? {
        CommandIdentityState::Missing => Ok(None),
        CommandIdentityState::Conflict => Ok(Some(Err(crate::career::CareerFailureCode::IdempotencyConflict))),
        CommandIdentityState::Matching => {
            let mut receipt: T = read_receipt(tx, scope.save_id, identity.command_id, identity.command_kind, identity.payload_sha256)
                .await?.context("career command identity has no final receipt")?;
            receipt.mark_replayed();
            Ok(Some(Ok(receipt)))
        }
    }
}

async fn finish_replay<T>(
    mut tx: Transaction<'_, MySql>,
    save_id: u64,
    result: Result<T, crate::career::CareerFailureCode>,
) -> Result<CareerStoreResult<T>> {
    match result {
        Err(failure) => {
            tx.commit().await?;
            Ok(CareerStoreResult::Rejected(failure))
        }
        Ok(receipt) => {
            let save = read_state(&mut tx, save_id).await?;
            tx.commit().await?;
            Ok(CareerStoreResult::Applied { receipt, save: Box::new(save) })
        }
    }
}

async fn finish_command<T: Serialize>(
    mut tx: Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
    identity: &CommandIdentitySpec<'_>,
    receipt: T,
) -> Result<CareerStoreResult<T>> {
    let committed_cursor = increment_state_revision(&mut tx, scope).await?;
    write_game_command_receipt(
        &mut tx,
        GameCommandReceiptWrite {
            save_id: scope.save_id,
            command_id: identity.command_id,
            command_kind: identity.command_kind,
            payload_sha256: identity.payload_sha256,
            market_world_id: scope.market_world_id,
            committed_cursor,
            result: &receipt,
            ledger_transaction_id: None,
        },
    )
    .await?;
    let save = read_state(&mut tx, scope.save_id).await?;
    tx.commit().await?;
    Ok(CareerStoreResult::Applied { receipt, save: Box::new(save) })
}

fn recruitment_failure(error: &RecruitmentError) -> crate::career::CareerFailureCode {
    use crate::career::CareerFailureCode as Failure;
    match error {
        RecruitmentError::ActiveEmployment | RecruitmentError::AlreadyAcceptedOffer | RecruitmentError::MultipleActiveContracts => Failure::AlreadyEmployed,
        RecruitmentError::ServiceConflict | RecruitmentError::MilitaryRequirementNotMet => Failure::MilitaryStateConflict,
        RecruitmentError::PostingClosed => Failure::PostingClosed,
        RecruitmentError::ApplicationLimit | RecruitmentError::InvitationLimit => Failure::ApplicationLimit,
        RecruitmentError::AlreadyApplied => Failure::AlreadyApplied,
        RecruitmentError::ArtifactRequired(_) | RecruitmentError::ArtifactNotOwned(_) | RecruitmentError::ArtifactNotPublic(_) => Failure::ArtifactRequired,
        RecruitmentError::InterviewExpired => Failure::InterviewExpired,
        RecruitmentError::OfferExpired => Failure::OfferExpired,
        RecruitmentError::ArithmeticOverflow => Failure::LimitExceeded,
        RecruitmentError::RegionMismatch | RecruitmentError::EducationRequired | RecruitmentError::CertificationRequired | RecruitmentError::ExperienceRequired | RecruitmentError::InvitationProfileIneligible => Failure::NotEligible,
        _ => Failure::InvalidCommand,
    }
}

pub(super) async fn apply_career_command(
    pool: &MySqlPool,
    user_id: u64,
    command: &ApplyCareerCommand,
) -> Result<CareerStoreResult<CareerApplicationReceipt>> {
    let fingerprint = apply_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(scope) = lock_scope_for_user(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::CharacterRequired));
    };
    let identity = CommandIdentitySpec { command_id: &command.command_id, command_kind: COMMAND_KIND_APPLY, payload_sha256: &fingerprint, cursor: command.cursor };
    if let Some(result) = replay_or_conflict::<CareerApplicationReceipt>(&mut tx, &scope, &identity).await? {
        return finish_replay(tx, scope.save_id, result).await;
    }
    if let Some(failure) = validate_command_cursor(&scope, command.cursor) {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(failure));
    }
    let Some(posting_row) = read_posting_by_key(&mut tx, &scope, &command.posting_key).await? else {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::PostingClosed));
    };
    let posting = posting_from_row(&posting_row)?;
    let rules = read_recruitment_rules_by_id(&mut tx, scope.career_catalog_bundle_id, posting_row.recruitment_ruleset_id).await?;
    let artifacts = read_command_artifacts(&mut tx, &scope, [command.resume_version_id, command.portfolio_version_id, command.linkedin_profile_version_id]).await?;
    let candidate = read_candidate_profile(&mut tx, &scope).await?;
    let key_refs = candidate.valid_catalog_entry_keys.iter().map(String::as_str).collect::<Vec<_>>();
    let (active_application_count, direct_applications_today, already_applied_to_posting) = application_counts(&mut tx, &scope, posting_row.id).await?;
    let plan = match rules.prepare_application(ApplicationEligibilityInput {
        posting: &posting,
        current_game_day: scope.game_day,
        source: ApplicationSource::Direct,
        candidate: candidate_domain(&candidate, &key_refs),
        submitted_artifacts: &artifacts,
        active_application_count,
        direct_applications_today,
        already_applied_to_posting,
        invitation_decision: None,
    }) {
        Ok(plan) => plan,
        Err(error) => {
            tx.commit().await?;
            return Ok(CareerStoreResult::Rejected(recruitment_failure(&error)));
        }
    };
    let (evidence, catalog) = read_score_inputs(&mut tx, scope.save_id, scope.run_revision, scope.career_catalog_bundle_id).await?;
    let visible_ids = visible_evidence_ids(&artifacts);
    let (_, visible_scores) = calculate_scores(scope.game_day, &posting.job_family_key, &visible_ids, &evidence, &catalog)?;
    write_command_identity(&mut tx, scope.save_id, &identity).await?;
    let application_id = insert_application(&mut tx, &scope, &posting_row, ApplicationSource::Direct, None, &artifacts, visible_scores, &plan.state).await?;
    let document_game_day = match plan.state {
        ApplicationState::Submitted { document_decision_game_day, .. } => document_decision_game_day,
        _ => bail!("direct application did not start as submitted"),
    };
    insert_application_action(&mut tx, &scope, posting_row.recruitment_ruleset_id, application_id, "documentReview", 20, document_game_day).await?;
    let receipt = CareerApplicationReceipt {
        command_id: command.command_id.clone(),
        application_id: ResourceId::from_u64(application_id),
        status: CareerApplicationStatus::Submitted,
        replayed: false,
    };
    finish_command(tx, &scope, &identity, receipt).await
}

#[derive(Debug, sqlx::FromRow)]
struct ApplicationCommandRow {
    id: u64,
    recruitment_ruleset_id: u64,
    status: String,
    confirmation_expires_exclusive_game_day: Option<u32>,
    interview_game_day: Option<u32>,
}

async fn lock_application(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
    application_id: u64,
) -> Result<Option<ApplicationCommandRow>> {
    sqlx::query_as(
        "SELECT id, recruitment_ruleset_id, status,
                confirmation_expires_exclusive_game_day, interview_game_day
         FROM job_application
         WHERE save_id = ? AND run_revision = ? AND id = ?
         FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(application_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock a recruitment application")
}

async fn cancel_pending_application_actions(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
    application_id: u64,
) -> Result<()> {
    sqlx::query(
        "UPDATE career_scheduled_action
         SET status = 'cancelled', cancelled_game_day = ?
         WHERE save_id = ? AND run_revision = ? AND job_application_id = ?
           AND status = 'pending'",
    )
    .bind(scope.game_day)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(application_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(super) async fn confirm_career_interview_command(
    pool: &MySqlPool,
    user_id: u64,
    command: &ConfirmCareerInterviewCommand,
) -> Result<CareerStoreResult<CareerApplicationReceipt>> {
    let fingerprint = interview_confirmation_fingerprint(command);
    let mut tx = pool.begin().await?;
    let Some(scope) = lock_scope_for_user(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::CharacterRequired));
    };
    let identity = CommandIdentitySpec { command_id: &command.command_id, command_kind: COMMAND_KIND_INTERVIEW_CONFIRMATION, payload_sha256: &fingerprint, cursor: command.cursor };
    if let Some(result) = replay_or_conflict::<CareerApplicationReceipt>(&mut tx, &scope, &identity).await? {
        return finish_replay(tx, scope.save_id, result).await;
    }
    if let Some(failure) = validate_command_cursor(&scope, command.cursor) {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(failure));
    }
    let Some(application) = lock_application(&mut tx, &scope, command.application_id.get()).await? else {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::InvalidCommand));
    };
    if application.status != "interviewAwaitingConfirmation" {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::InvalidCommand));
    }
    let deadline = application.confirmation_expires_exclusive_game_day.context("application confirmation deadline is missing")?;
    if scope.game_day >= deadline {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::InterviewExpired));
    }
    write_command_identity(&mut tx, scope.save_id, &identity).await?;
    let response_status = match command.decision {
        InterviewDecision::Confirm => {
            let update = sqlx::query(
                "UPDATE job_application SET status = 'interviewConfirmed'
                 WHERE save_id = ? AND run_revision = ? AND id = ?
                   AND status = 'interviewAwaitingConfirmation'",
            )
            .bind(scope.save_id)
            .bind(scope.run_revision)
            .bind(application.id)
            .execute(&mut *tx)
            .await?;
            ensure!(update.rows_affected() == 1, "interview confirmation was lost");
            cancel_pending_application_actions(&mut tx, &scope, application.id).await?;
            insert_application_action(
                &mut tx,
                &scope,
                application.recruitment_ruleset_id,
                application.id,
                "interviewDecision",
                40,
                application.interview_game_day.context("application interview day is missing")?,
            )
            .await?;
            CareerApplicationStatus::InterviewConfirmed
        }
        InterviewDecision::Decline => {
            let update = sqlx::query(
                "UPDATE job_application
                 SET terminal_from_status = status, terminal_game_day = ?,
                     status = 'withdrawn'
                 WHERE save_id = ? AND run_revision = ? AND id = ?
                   AND status = 'interviewAwaitingConfirmation'",
            )
            .bind(scope.game_day)
            .bind(scope.save_id)
            .bind(scope.run_revision)
            .bind(application.id)
            .execute(&mut *tx)
            .await?;
            ensure!(update.rows_affected() == 1, "interview decline was lost");
            cancel_pending_application_actions(&mut tx, &scope, application.id).await?;
            CareerApplicationStatus::Withdrawn
        }
    };
    let receipt = CareerApplicationReceipt {
        command_id: command.command_id.clone(),
        application_id: command.application_id,
        status: response_status,
        replayed: false,
    };
    finish_command(tx, &scope, &identity, receipt).await
}

pub(super) async fn withdraw_career_application_command(
    pool: &MySqlPool,
    user_id: u64,
    command: &WithdrawCareerApplicationCommand,
) -> Result<CareerStoreResult<CareerApplicationReceipt>> {
    let fingerprint = single_id_fingerprint("lifeledger.career.application-withdraw.v1", command.cursor, "applicationId", command.application_id.get());
    let mut tx = pool.begin().await?;
    let Some(scope) = lock_scope_for_user(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::CharacterRequired));
    };
    let identity = CommandIdentitySpec { command_id: &command.command_id, command_kind: COMMAND_KIND_APPLICATION_WITHDRAW, payload_sha256: &fingerprint, cursor: command.cursor };
    if let Some(result) = replay_or_conflict::<CareerApplicationReceipt>(&mut tx, &scope, &identity).await? {
        return finish_replay(tx, scope.save_id, result).await;
    }
    if let Some(failure) = validate_command_cursor(&scope, command.cursor) {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(failure));
    }
    let Some(application) = lock_application(&mut tx, &scope, command.application_id.get()).await? else {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::InvalidCommand));
    };
    if !matches!(application.status.as_str(), "submitted" | "interviewAwaitingConfirmation" | "interviewConfirmed") {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::InvalidCommand));
    }
    write_command_identity(&mut tx, scope.save_id, &identity).await?;
    let update = sqlx::query(
        "UPDATE job_application
         SET terminal_from_status = status, terminal_game_day = ?,
             status = 'withdrawn'
         WHERE save_id = ? AND run_revision = ? AND id = ? AND status = ?",
    )
    .bind(scope.game_day)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(application.id)
    .bind(&application.status)
    .execute(&mut *tx)
    .await?;
    ensure!(update.rows_affected() == 1, "application withdrawal was lost");
    cancel_pending_application_actions(&mut tx, &scope, application.id).await?;
    let receipt = CareerApplicationReceipt {
        command_id: command.command_id.clone(),
        application_id: command.application_id,
        status: CareerApplicationStatus::Withdrawn,
        replayed: false,
    };
    finish_command(tx, &scope, &identity, receipt).await
}

#[derive(Debug, sqlx::FromRow)]
struct InvitationCommandRow {
    id: u64,
    career_catalog_bundle_id: u64,
    recruitment_ruleset_id: u64,
    job_posting_id: u64,
    profile_artifact_version_id: u64,
    status: String,
    invitation_game_day: u32,
    expires_exclusive_game_day: u32,
    artifact_completeness_bp: i64,
    visible_education_score_bp: i64,
    visible_certification_score_bp: i64,
    visible_language_score_bp: i64,
    visible_training_score_bp: i64,
    visible_experience_score_bp: i64,
    visible_project_score_bp: i64,
    invitation_score_bp: i64,
    invitation_probability_ppm: u32,
    invitation_roll: u32,
}

async fn lock_invitation(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
    invitation_id: u64,
) -> Result<Option<InvitationCommandRow>> {
    sqlx::query_as(
        "SELECT id, career_catalog_bundle_id, recruitment_ruleset_id,
                job_posting_id, profile_artifact_version_id, status,
                invitation_game_day, expires_exclusive_game_day,
                artifact_completeness_bp, visible_education_score_bp,
                visible_certification_score_bp, visible_language_score_bp,
                visible_training_score_bp, visible_experience_score_bp,
                visible_project_score_bp,
                invitation_score_bp, invitation_probability_ppm, invitation_roll
         FROM job_invitation
         WHERE save_id = ? AND run_revision = ? AND id = ? FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(invitation_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock a recruitment invitation")
}

pub(super) async fn decline_career_invitation_command(
    pool: &MySqlPool,
    user_id: u64,
    command: &DeclineCareerInvitationCommand,
) -> Result<CareerStoreResult<CareerInvitationReceipt>> {
    let fingerprint = single_id_fingerprint("lifeledger.career.invitation-decline.v1", command.cursor, "invitationId", command.invitation_id.get());
    let mut tx = pool.begin().await?;
    let Some(scope) = lock_scope_for_user(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::CharacterRequired));
    };
    let identity = CommandIdentitySpec { command_id: &command.command_id, command_kind: COMMAND_KIND_INVITATION_DECLINE, payload_sha256: &fingerprint, cursor: command.cursor };
    if let Some(result) = replay_or_conflict::<CareerInvitationReceipt>(&mut tx, &scope, &identity).await? {
        return finish_replay(tx, scope.save_id, result).await;
    }
    if let Some(failure) = validate_command_cursor(&scope, command.cursor) {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(failure));
    }
    let Some(invitation) = lock_invitation(&mut tx, &scope, command.invitation_id.get()).await? else {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::InvalidCommand));
    };
    if invitation.status != "open" {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::InvalidCommand));
    }
    if scope.game_day >= invitation.expires_exclusive_game_day {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::PostingClosed));
    }
    write_command_identity(&mut tx, scope.save_id, &identity).await?;
    let update = sqlx::query(
        "UPDATE job_invitation SET status = 'declined', decided_game_day = ?
         WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'open'",
    )
    .bind(scope.game_day)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(invitation.id)
    .execute(&mut *tx)
    .await?;
    ensure!(update.rows_affected() == 1, "invitation decline was lost");
    let receipt = CareerInvitationReceipt {
        command_id: command.command_id.clone(),
        invitation_id: command.invitation_id,
        status: CareerInvitationStatus::Declined,
        application_id: None,
        replayed: false,
    };
    finish_command(tx, &scope, &identity, receipt).await
}

pub(super) async fn accept_career_invitation_command(
    pool: &MySqlPool,
    user_id: u64,
    command: &AcceptCareerInvitationCommand,
) -> Result<CareerStoreResult<CareerInvitationReceipt>> {
    let fingerprint = single_id_fingerprint("lifeledger.career.invitation-accept.v1", command.cursor, "invitationId", command.invitation_id.get());
    let mut tx = pool.begin().await?;
    let Some(scope) = lock_scope_for_user(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::CharacterRequired));
    };
    let identity = CommandIdentitySpec { command_id: &command.command_id, command_kind: COMMAND_KIND_INVITATION_ACCEPT, payload_sha256: &fingerprint, cursor: command.cursor };
    if let Some(result) = replay_or_conflict::<CareerInvitationReceipt>(&mut tx, &scope, &identity).await? {
        return finish_replay(tx, scope.save_id, result).await;
    }
    if let Some(failure) = validate_command_cursor(&scope, command.cursor) {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(failure));
    }
    let Some(invitation) = lock_invitation(&mut tx, &scope, command.invitation_id.get()).await? else {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::InvalidCommand));
    };
    if invitation.status != "open" {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::InvalidCommand));
    }
    if scope.game_day >= invitation.expires_exclusive_game_day {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::PostingClosed));
    }
    let posting_row = read_posting_by_id(&mut tx, &scope, invitation.job_posting_id)
        .await?.context("invitation posting is unavailable")?;
    ensure!(posting_row.recruitment_ruleset_id == invitation.recruitment_ruleset_id, "invitation ruleset drifted");
    let posting = posting_from_row(&posting_row)?;
    let rules = read_recruitment_rules_by_id(&mut tx, scope.career_catalog_bundle_id, invitation.recruitment_ruleset_id).await?;
    let artifact = read_artifact(&mut tx, &scope, invitation.profile_artifact_version_id)
        .await?.context("invitation artifact is unavailable")?;
    let (evidence, catalog) = read_score_inputs(&mut tx, scope.save_id, scope.run_revision, scope.career_catalog_bundle_id).await?;
    let (_, visible_scores) = calculate_scores(invitation.invitation_game_day, &posting.job_family_key, &artifact.evidence_ids, &evidence, &catalog)?;
    let candidate = read_candidate_profile(&mut tx, &scope).await?;
    let key_refs = candidate.valid_catalog_entry_keys.iter().map(String::as_str).collect::<Vec<_>>();
    let invitation_decision = rules.evaluate_invitation(InvitationEvaluationInput {
        world_seed: scope.world_seed,
        posting: &posting,
        invitation_game_day: invitation.invitation_game_day,
        latest_public_artifact: &artifact,
        visible_scores,
        open_invitation_count: 0,
        platform_invitation_already_generated_today: false,
        candidate: candidate_domain(&candidate, &key_refs),
    }).map_err(anyhow::Error::new)?;
    ensure!(
        invitation_decision.decision.score_bp == invitation.invitation_score_bp
            && invitation_decision.decision.probability_ppm == invitation.invitation_probability_ppm
            && invitation_decision.decision.roll_ppm == invitation.invitation_roll
            && invitation_decision.decision.passed,
        "stored invitation decision drifted"
    );
    ensure!(
        visible_scores == DimensionScores {
            education: invitation.visible_education_score_bp,
            certification: invitation.visible_certification_score_bp,
            language: invitation.visible_language_score_bp,
            training: invitation.visible_training_score_bp,
            experience: invitation.visible_experience_score_bp,
            project: invitation.visible_project_score_bp,
        } && artifact.completeness_bp == invitation.artifact_completeness_bp,
        "stored invitation pin drifted"
    );
    let (active_application_count, direct_applications_today, already_applied_to_posting) = application_counts(&mut tx, &scope, posting_row.id).await?;
    let plan = match rules.prepare_application(ApplicationEligibilityInput {
        posting: &posting,
        current_game_day: scope.game_day,
        source: ApplicationSource::Invitation,
        candidate: candidate_domain(&candidate, &key_refs),
        submitted_artifacts: std::slice::from_ref(&artifact),
        active_application_count,
        direct_applications_today,
        already_applied_to_posting,
        invitation_decision: Some(&invitation_decision.decision),
    }) {
        Ok(plan) => plan,
        Err(error) => {
            tx.commit().await?;
            return Ok(CareerStoreResult::Rejected(recruitment_failure(&error)));
        }
    };
    write_command_identity(&mut tx, scope.save_id, &identity).await?;
    let application_id = insert_application(
        &mut tx,
        &scope,
        &posting_row,
        ApplicationSource::Invitation,
        Some(invitation.id),
        std::slice::from_ref(&artifact),
        visible_scores,
        &plan.state,
    )
    .await?;
    let interview_game_day = match plan.state {
        ApplicationState::InterviewAwaitingConfirmation { interview_game_day, .. } => interview_game_day,
        _ => bail!("invitation application did not await confirmation"),
    };
    insert_application_action(
        &mut tx,
        &scope,
        invitation.recruitment_ruleset_id,
        application_id,
        "confirmationExpiry",
        30,
        interview_game_day,
    )
    .await?;
    let update = sqlx::query(
        "UPDATE job_invitation
         SET status = 'accepted', accepted_application_id = ?, decided_game_day = ?
         WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'open'",
    )
    .bind(application_id)
    .bind(scope.game_day)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(invitation.id)
    .execute(&mut *tx)
    .await?;
    ensure!(update.rows_affected() == 1, "invitation acceptance was lost");
    let receipt = CareerInvitationReceipt {
        command_id: command.command_id.clone(),
        invitation_id: command.invitation_id,
        status: CareerInvitationStatus::Accepted,
        application_id: Some(ResourceId::from_u64(application_id)),
        replayed: false,
    };
    finish_command(tx, &scope, &identity, receipt).await
}

#[derive(Debug, sqlx::FromRow)]
struct OfferCommandRow {
    id: u64,
    job_application_id: u64,
    job_posting_id: u64,
    recruitment_ruleset_id: u64,
    offer_status: String,
    application_status: String,
    source_kind: String,
    annual_salary_krw: i64,
    payday_day_of_month: u8,
    start_game_day: u32,
    expires_exclusive_game_day: u32,
    first_pay_reward_krw: i64,
    document_visible_fit_bp: Option<i64>,
    artifact_completeness_bp: i64,
    document_platform_affinity_bp: Option<i64>,
    document_score_bp: Option<i64>,
    document_probability_ppm: Option<u32>,
    document_roll: Option<u32>,
    possessed_fit_bp: i64,
    experience_project_fit_bp: i64,
    profile_consistency_bp: i64,
    interview_score_bp: i64,
    interview_probability_ppm: u32,
    interview_roll: u32,
    interview_decided_game_day: u32,
}

async fn lock_offer(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
    offer_id: u64,
) -> Result<Option<OfferCommandRow>> {
    sqlx::query_as(
        "SELECT offer.id, offer.job_application_id, offer.job_posting_id,
                offer.recruitment_ruleset_id, offer.status AS offer_status,
                application.status AS application_status, application.source_kind,
                offer.annual_salary_krw, offer.payday_day_of_month,
                offer.start_game_day, offer.expires_exclusive_game_day,
                offer.first_pay_reward_krw,
                application.document_visible_fit_bp,
                application.artifact_completeness_bp,
                application.document_platform_affinity_bp,
                application.document_score_bp,
                application.document_probability_ppm, application.document_roll,
                application.possessed_fit_bp, application.experience_project_fit_bp,
                application.profile_consistency_bp, application.interview_score_bp,
                application.interview_probability_ppm, application.interview_roll,
                application.interview_decided_game_day
         FROM job_offer AS offer
         INNER JOIN job_application AS application
           ON application.save_id = offer.save_id
          AND application.run_revision = offer.run_revision
          AND application.id = offer.job_application_id
         WHERE offer.save_id = ? AND offer.run_revision = ? AND offer.id = ?
         FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(offer_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock a recruitment offer")
}

pub(super) async fn decline_career_offer_command(
    pool: &MySqlPool,
    user_id: u64,
    command: &DeclineCareerOfferCommand,
) -> Result<CareerStoreResult<CareerOfferReceipt>> {
    let fingerprint = single_id_fingerprint("lifeledger.career.offer-decline.v1", command.cursor, "offerId", command.offer_id.get());
    let mut tx = pool.begin().await?;
    let Some(scope) = lock_scope_for_user(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::CharacterRequired));
    };
    let identity = CommandIdentitySpec { command_id: &command.command_id, command_kind: COMMAND_KIND_OFFER_DECLINE, payload_sha256: &fingerprint, cursor: command.cursor };
    if let Some(result) = replay_or_conflict::<CareerOfferReceipt>(&mut tx, &scope, &identity).await? {
        return finish_replay(tx, scope.save_id, result).await;
    }
    if let Some(failure) = validate_command_cursor(&scope, command.cursor) {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(failure));
    }
    let Some(offer) = lock_offer(&mut tx, &scope, command.offer_id.get()).await? else {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::InvalidCommand));
    };
    if offer.offer_status != "pending" || offer.application_status != "offered" {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::InvalidCommand));
    }
    if scope.game_day >= offer.expires_exclusive_game_day {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::OfferExpired));
    }
    write_command_identity(&mut tx, scope.save_id, &identity).await?;
    let offer_update = sqlx::query(
        "UPDATE job_offer SET status = 'declined', decided_game_day = ?
         WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'pending'",
    )
    .bind(scope.game_day)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(offer.id)
    .execute(&mut *tx)
    .await?;
    ensure!(offer_update.rows_affected() == 1, "offer decline was lost");
    let application_update = sqlx::query(
        "UPDATE job_application SET status = 'declined'
         WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'offered'",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(offer.job_application_id)
    .execute(&mut *tx)
    .await?;
    ensure!(application_update.rows_affected() == 1, "offer application decline was lost");
    cancel_pending_application_actions(&mut tx, &scope, offer.job_application_id).await?;
    let receipt = CareerOfferReceipt {
        command_id: command.command_id.clone(),
        offer_id: command.offer_id,
        status: CareerApplicationStatus::Declined,
        employment_contract_id: None,
        replayed: false,
    };
    finish_command(tx, &scope, &identity, receipt).await
}

fn score_band_for(score: i64, rules: &RecruitmentRuleset) -> Result<ScoreBand> {
    ensure!((0..=10_000).contains(&score), "stored recruitment score is invalid");
    Ok(if score < rules.score_bands.medium_minimum_bp {
        ScoreBand::Low
    } else if score < rules.score_bands.high_minimum_bp {
        ScoreBand::Medium
    } else {
        ScoreBand::High
    })
}

fn stored_entry_decision(offer: &OfferCommandRow, rules: &RecruitmentRuleset) -> Result<StageDecision> {
    let stage = if offer.source_kind == "invitation" { RecruitmentStage::Invitation } else { RecruitmentStage::Document };
    let score_bp = offer.document_score_bp.unwrap_or(10_000);
    Ok(StageDecision {
        stage,
        score_band: score_band_for(score_bp, rules)?,
        components: StageComponents {
            primary_fit_bp: offer.document_visible_fit_bp.unwrap_or(score_bp),
            supporting_fit_bp: offer.artifact_completeness_bp,
            context_fit_bp: offer.document_platform_affinity_bp.unwrap_or(0),
        },
        dimension_fit_bp: None,
        score_bp,
        probability_ppm: offer.document_probability_ppm.unwrap_or(1_000_000),
        roll_ppm: offer.document_roll.unwrap_or(0),
        passed: true,
    })
}

fn stored_interview_decision(offer: &OfferCommandRow, rules: &RecruitmentRuleset) -> Result<StageDecision> {
    Ok(StageDecision {
        stage: RecruitmentStage::Interview,
        score_band: score_band_for(offer.interview_score_bp, rules)?,
        components: StageComponents {
            primary_fit_bp: offer.possessed_fit_bp,
            supporting_fit_bp: offer.experience_project_fit_bp,
            context_fit_bp: offer.profile_consistency_bp,
        },
        dimension_fit_bp: None,
        score_bp: offer.interview_score_bp,
        probability_ppm: offer.interview_probability_ppm,
        roll_ppm: offer.interview_roll,
        passed: true,
    })
}

pub(super) async fn accept_career_offer_command(
    pool: &MySqlPool,
    user_id: u64,
    command: &AcceptCareerOfferCommand,
) -> Result<CareerStoreResult<CareerOfferReceipt>> {
    let fingerprint = single_id_fingerprint("lifeledger.career.offer-accept.v1", command.cursor, "offerId", command.offer_id.get());
    let mut tx = pool.begin().await?;
    let Some(scope) = lock_scope_for_user(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::CharacterRequired));
    };
    let identity = CommandIdentitySpec { command_id: &command.command_id, command_kind: COMMAND_KIND_OFFER_ACCEPT, payload_sha256: &fingerprint, cursor: command.cursor };
    if let Some(result) = replay_or_conflict::<CareerOfferReceipt>(&mut tx, &scope, &identity).await? {
        return finish_replay(tx, scope.save_id, result).await;
    }
    if let Some(failure) = validate_command_cursor(&scope, command.cursor) {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(failure));
    }
    let Some(offer) = lock_offer(&mut tx, &scope, command.offer_id.get()).await? else {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::InvalidCommand));
    };
    if offer.offer_status != "pending" || offer.application_status != "offered" {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::InvalidCommand));
    }
    if scope.game_day >= offer.expires_exclusive_game_day {
        tx.commit().await?;
        return Ok(CareerStoreResult::Rejected(crate::career::CareerFailureCode::OfferExpired));
    }
    let posting_row = read_posting_by_id(&mut tx, &scope, offer.job_posting_id).await?.context("offer posting is unavailable")?;
    let posting = posting_from_row(&posting_row)?;
    let rules = read_recruitment_rules_by_id(&mut tx, scope.career_catalog_bundle_id, offer.recruitment_ruleset_id).await?;
    let salary = rules.determine_offer_salary(OfferSalaryInput {
        world_seed: scope.world_seed,
        posting: &posting,
        possessed_fit_bp: offer.possessed_fit_bp,
    }).map_err(anyhow::Error::new)?;
    ensure!(salary.annual_salary_krw == offer.annual_salary_krw, "stored offer salary drifted");
    let state = ApplicationState::Offered {
        offered_game_day: offer.interview_decided_game_day,
        expires_exclusive_game_day: offer.expires_exclusive_game_day,
        entry_decision: stored_entry_decision(&offer, rules.ruleset())?,
        interview_decision: stored_interview_decision(&offer, rules.ruleset())?,
        salary,
    };
    let contract_rows: Vec<(u64, String)> = sqlx::query_as(
        "SELECT id, status FROM employment_contract
         WHERE save_id = ? AND run_revision = ? ORDER BY id FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_all(&mut *tx)
    .await?;
    let contracts = contract_rows.into_iter().map(|(id, status)| {
        Ok(EmploymentContractSummary { contract_id: id, status: enum_from_db(&status)? })
    }).collect::<Result<Vec<_>>>()?;
    let other_accepted_offer_count: u32 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM job_offer
         WHERE save_id = ? AND run_revision = ? AND status = 'accepted' AND id <> ?",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(offer.id)
    .fetch_one(&mut *tx)
    .await?;
    let other_open_application_ids: Vec<u64> = sqlx::query_scalar(
        "SELECT id FROM job_application
         WHERE save_id = ? AND run_revision = ?
           AND status IN ('submitted', 'interviewAwaitingConfirmation', 'interviewConfirmed', 'offered')
         ORDER BY id FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_all(&mut *tx)
    .await?;
    let open_invitation_ids: Vec<u64> = sqlx::query_scalar(
        "SELECT id FROM job_invitation
         WHERE save_id = ? AND run_revision = ? AND status = 'open'
         ORDER BY id FOR UPDATE",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_all(&mut *tx)
    .await?;
    let plan = match rules.plan_offer_acceptance(OfferAcceptanceInput {
        application_id: offer.job_application_id,
        posting: &posting,
        state: &state,
        accepted_game_day: scope.game_day,
        contracts: &contracts,
        other_accepted_offer_count,
        other_open_application_ids: &other_open_application_ids,
        open_invitation_ids: &open_invitation_ids,
    }) {
        Ok(plan) => plan,
        Err(error) => {
            tx.commit().await?;
            return Ok(CareerStoreResult::Rejected(recruitment_failure(&error)));
        }
    };
    ensure!(plan.contract.start_game_day == offer.start_game_day && plan.contract.monthly_payday == offer.payday_day_of_month, "offer contract terms drifted");
    write_command_identity(&mut tx, scope.save_id, &identity).await?;
    let contract_insert = sqlx::query(
        "INSERT INTO employment_contract
             (save_id, run_revision, career_catalog_bundle_id, recruitment_ruleset_id,
              job_offer_id, job_application_id, job_posting_id,
              career_industry_id, career_job_family_id, virtual_employer_id,
              status, annual_salary_krw, region, employment_type,
              payday_day_of_month, start_game_day, first_pay_reward_krw,
              created_game_day)
         SELECT ?, ?, posting.career_catalog_bundle_id, posting.recruitment_ruleset_id,
                ?, ?, posting.id, posting.career_industry_id,
                posting.career_job_family_id, posting.virtual_employer_id,
                'pendingStart', ?, posting.region, posting.employment_type,
                ?, ?, ?, ?
         FROM job_posting AS posting WHERE posting.id = ?",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(offer.id)
    .bind(offer.job_application_id)
    .bind(offer.annual_salary_krw)
    .bind(offer.payday_day_of_month)
    .bind(offer.start_game_day)
    .bind(offer.first_pay_reward_krw)
    .bind(scope.game_day)
    .bind(offer.job_posting_id)
    .execute(&mut *tx)
    .await?;
    let contract_id = contract_insert.last_insert_id();
    let offer_update = sqlx::query(
        "UPDATE job_offer
         SET status = 'accepted', employment_contract_id = ?, decided_game_day = ?
         WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'pending'",
    )
    .bind(contract_id)
    .bind(scope.game_day)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(offer.id)
    .execute(&mut *tx)
    .await?;
    ensure!(offer_update.rows_affected() == 1, "offer acceptance was lost");
    let application_update = sqlx::query(
        "UPDATE job_application SET status = 'accepted'
         WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'offered'",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(offer.job_application_id)
    .execute(&mut *tx)
    .await?;
    ensure!(application_update.rows_affected() == 1, "accepted application transition was lost");
    for application_id in &plan.close_application_ids {
        let pending_offer_id: Option<u64> = sqlx::query_scalar(
            "SELECT id FROM job_offer
             WHERE save_id = ? AND run_revision = ? AND job_application_id = ?
               AND status = 'pending' FOR UPDATE",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(application_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(other_offer_id) = pending_offer_id {
            sqlx::query(
                "UPDATE job_offer SET status = 'closed', decided_game_day = ?
                 WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'pending'",
            )
            .bind(scope.game_day)
            .bind(scope.save_id)
            .bind(scope.run_revision)
            .bind(other_offer_id)
            .execute(&mut *tx)
            .await?;
        }
        let update = sqlx::query(
            "UPDATE job_application
             SET terminal_from_status = status, terminal_game_day = ?, status = 'closed'
             WHERE save_id = ? AND run_revision = ? AND id = ?
               AND status IN ('submitted', 'interviewAwaitingConfirmation', 'interviewConfirmed', 'offered')",
        )
        .bind(scope.game_day)
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(application_id)
        .execute(&mut *tx)
        .await?;
        ensure!(update.rows_affected() == 1, "open application close was lost");
        cancel_pending_application_actions(&mut tx, &scope, *application_id).await?;
    }
    for invitation_id in &plan.close_invitation_ids {
        let update = sqlx::query(
            "UPDATE job_invitation SET status = 'closed', decided_game_day = ?
             WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'open'",
        )
        .bind(scope.game_day)
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(invitation_id)
        .execute(&mut *tx)
        .await?;
        ensure!(update.rows_affected() == 1, "open invitation close was lost");
    }
    cancel_pending_application_actions(&mut tx, &scope, offer.job_application_id).await?;
    sqlx::query(
        "INSERT INTO career_scheduled_action
             (save_id, run_revision, career_catalog_bundle_id, recruitment_ruleset_id,
              action_kind, payload_version, phase_rank, due_game_day, status,
              source_kind, source_id, occurrence, employment_contract_id)
         VALUES (?, ?, ?, ?, 'employmentStart', 1, 10, ?, 'pending',
                 'employmentStart', ?, 1, ?)",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(scope.career_catalog_bundle_id)
    .bind(offer.recruitment_ruleset_id)
    .bind(offer.start_game_day)
    .bind(contract_id)
    .bind(contract_id)
    .execute(&mut *tx)
    .await?;
    let receipt = CareerOfferReceipt {
        command_id: command.command_id.clone(),
        offer_id: command.offer_id,
        status: CareerApplicationStatus::Accepted,
        employment_contract_id: Some(ResourceId::from_u64(contract_id)),
        replayed: false,
    };
    finish_command(tx, &scope, &identity, receipt).await
}

pub(super) async fn advance_recruitment_lifecycle_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    target_game_day: u32,
) -> Result<()> {
    let previous_game_day: u32 = sqlx::query_scalar(
        "SELECT game_day FROM save WHERE id = ? AND run_revision = ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await?
    .context("daily recruitment lifecycle requires an active save")?;
    ensure!(
        previous_game_day.checked_add(1) == Some(target_game_day),
        "daily recruitment lifecycle target is not the next game day"
    );
    let action_rows: Vec<ScheduledActionRow> = sqlx::query_as(
        "SELECT id, action_kind, payload_version, phase_rank, due_game_day,
                source_kind, source_id, occurrence, recruitment_ruleset_id,
                employment_contract_id, job_application_id, platform_catalog_id,
                invitation_generation_game_day
         FROM career_scheduled_action
         WHERE save_id = ? AND run_revision = ? AND status = 'pending'
           AND phase_rank = 10 AND due_game_day = ?
         ORDER BY due_game_day, id",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(target_game_day)
    .fetch_all(&mut **tx)
    .await?;
    let mut contract_ids = action_rows
        .iter()
        .map(|action| {
            validate_scheduled_action(action)?;
            action.employment_contract_id.context("employment start action has no contract")
        })
        .collect::<Result<Vec<_>>>()?;
    contract_ids.sort_unstable();
    contract_ids.dedup();
    for contract_id in &contract_ids {
        let _: (u64,) = sqlx::query_as(
            "SELECT id FROM employment_contract
             WHERE save_id = ? AND run_revision = ? AND id = ? FOR UPDATE",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(contract_id)
        .fetch_optional(&mut **tx)
        .await?
        .context("employment start contract is unavailable")?;
    }
    for action in action_rows {
        let contract_id = action.employment_contract_id.context("employment start action has no contract")?;
        let locked: Option<(u64,)> = sqlx::query_as(
            "SELECT id FROM career_scheduled_action
             WHERE save_id = ? AND run_revision = ? AND id = ?
               AND status = 'pending' AND due_game_day = ? FOR UPDATE",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(action.id)
        .bind(target_game_day)
        .fetch_optional(&mut **tx)
        .await?;
        ensure!(locked.is_some(), "employment start action changed under lock");
        let update = sqlx::query(
            "UPDATE employment_contract
             SET status = 'active', credited_experience_days = 1,
                 last_credited_game_day = ?
             WHERE save_id = ? AND run_revision = ? AND id = ?
               AND status = 'pendingStart' AND start_game_day = ?",
        )
        .bind(target_game_day)
        .bind(save_id)
        .bind(run_revision)
        .bind(contract_id)
        .bind(target_game_day)
        .execute(&mut **tx)
        .await?;
        ensure!(update.rows_affected() == 1, "employment start transition was lost");
        complete_action(tx, action.id, target_game_day).await?;
    }
    let stale_active: u64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM employment_contract
         WHERE save_id = ? AND run_revision = ? AND status = 'active'
           AND last_credited_game_day < ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(target_game_day.saturating_sub(1))
    .fetch_one(&mut **tx)
    .await?;
    ensure!(stale_active == 0, "active employment experience has a game-day gap");
    sqlx::query(
        "UPDATE employment_contract
         SET credited_experience_days = credited_experience_days + 1,
             last_credited_game_day = ?
         WHERE save_id = ? AND run_revision = ? AND status = 'active'
           AND last_credited_game_day = ?",
    )
    .bind(target_game_day)
    .bind(save_id)
    .bind(run_revision)
    .bind(target_game_day.saturating_sub(1))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn validate_scheduled_action(action: &ScheduledActionRow) -> Result<()> {
    ensure!(action.payload_version == 1, "unknown career scheduled action payload version");
    match action.action_kind.as_str() {
        "employmentStart" => ensure!(
            action.phase_rank == 10
                && action.source_kind == "employmentStart"
                && action.employment_contract_id == Some(action.source_id)
                && action.job_application_id.is_none()
                && action.platform_catalog_id.is_none()
                && action.invitation_generation_game_day.is_none()
                && action.occurrence == 1,
            "invalid employment start action payload"
        ),
        "documentReview" => ensure!(action.phase_rank == 20 && action.source_kind == "documentReview" && action.job_application_id == Some(action.source_id) && action.employment_contract_id.is_none() && action.platform_catalog_id.is_none() && action.invitation_generation_game_day.is_none() && action.occurrence == 1, "invalid document action payload"),
        "confirmationExpiry" => ensure!(action.phase_rank == 30 && action.source_kind == "confirmationExpiry" && action.job_application_id == Some(action.source_id) && action.employment_contract_id.is_none() && action.platform_catalog_id.is_none() && action.invitation_generation_game_day.is_none() && action.occurrence == 1, "invalid confirmation action payload"),
        "interviewDecision" => ensure!(action.phase_rank == 40 && action.source_kind == "interviewDecision" && action.job_application_id == Some(action.source_id) && action.employment_contract_id.is_none() && action.platform_catalog_id.is_none() && action.invitation_generation_game_day.is_none() && action.occurrence == 1, "invalid interview action payload"),
        "offerExpiry" => ensure!(action.phase_rank == 50 && action.source_kind == "offerExpiry" && action.job_application_id == Some(action.source_id) && action.employment_contract_id.is_none() && action.platform_catalog_id.is_none() && action.invitation_generation_game_day.is_none() && action.occurrence == 1, "invalid offer expiry action payload"),
        "invitationGeneration" => ensure!(action.phase_rank == 60 && action.source_kind == "invitationGeneration" && action.platform_catalog_id == Some(action.source_id) && action.employment_contract_id.is_none() && action.job_application_id.is_none() && action.invitation_generation_game_day == Some(action.due_game_day) && action.occurrence == u64::from(action.due_game_day), "invalid invitation generation action payload"),
        _ => bail!("unknown career scheduled action kind"),
    }
    Ok(())
}

async fn complete_action(
    tx: &mut Transaction<'_, MySql>,
    action_id: u64,
    due_game_day: u32,
) -> Result<()> {
    let update = sqlx::query(
        "UPDATE career_scheduled_action
         SET status = 'completed', completed_game_day = ?
         WHERE id = ? AND status = 'pending' AND due_game_day = ?",
    )
    .bind(due_game_day)
    .bind(action_id)
    .bind(due_game_day)
    .execute(&mut **tx)
    .await?;
    ensure!(update.rows_affected() == 1, "career scheduled action completion was lost");
    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
struct EvaluationApplicationRow {
    id: u64,
    job_posting_id: u64,
    recruitment_ruleset_id: u64,
    source_kind: String,
    status: String,
    submitted_game_day: u32,
    artifact_completeness_bp: i64,
    resume_version_id: Option<u64>,
    portfolio_version_id: Option<u64>,
    linkedin_profile_version_id: Option<u64>,
    visible_education_score_bp: i64,
    visible_certification_score_bp: i64,
    visible_language_score_bp: i64,
    visible_training_score_bp: i64,
    visible_experience_score_bp: i64,
    visible_project_score_bp: i64,
    document_visible_fit_bp: Option<i64>,
    document_platform_affinity_bp: Option<i64>,
    document_score_bp: Option<i64>,
    document_probability_ppm: Option<u32>,
    document_roll: Option<u32>,
    document_decided_game_day: Option<u32>,
    confirmation_expires_exclusive_game_day: Option<u32>,
    interview_game_day: Option<u32>,
}

async fn read_scope_for_save_day(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
) -> Result<RecruitmentScopeRow> {
    sqlx::query_as(
        "SELECT save.id AS save_id, save.market_world_id, world.seed AS world_seed,
                save.run_revision, save.state_revision, ? AS game_day, save.cash_krw,
                career_run.career_catalog_bundle_id,
                `character`.region AS character_region,
                `character`.education AS character_education,
                `character`.military AS character_military
         FROM save
         INNER JOIN `character` ON `character`.save_id = save.id
         INNER JOIN career_run
           ON career_run.save_id = save.id AND career_run.run_revision = save.run_revision
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         WHERE save.id = ? AND save.run_revision = ?",
    )
    .bind(game_day)
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await?
    .context("daily recruitment actions require an active career run")
}

async fn load_evaluation_application(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
    application_id: u64,
) -> Result<EvaluationApplicationRow> {
    sqlx::query_as(
        "SELECT id, job_posting_id, recruitment_ruleset_id, source_kind, status,
                submitted_game_day, artifact_completeness_bp,
                resume_version_id, portfolio_version_id, linkedin_profile_version_id,
                visible_education_score_bp, visible_certification_score_bp,
                visible_language_score_bp, visible_training_score_bp,
                visible_experience_score_bp, visible_project_score_bp,
                document_visible_fit_bp, document_platform_affinity_bp,
                document_score_bp, document_probability_ppm, document_roll,
                document_decided_game_day,
                confirmation_expires_exclusive_game_day, interview_game_day
         FROM job_application
         WHERE save_id = ? AND run_revision = ? AND id = ?",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(application_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("scheduled recruitment application is unavailable")
}

fn application_visible_scores(application: &EvaluationApplicationRow) -> DimensionScores {
    DimensionScores {
        education: application.visible_education_score_bp,
        certification: application.visible_certification_score_bp,
        language: application.visible_language_score_bp,
        training: application.visible_training_score_bp,
        experience: application.visible_experience_score_bp,
        project: application.visible_project_score_bp,
    }
}

async fn process_document_action(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
    action: &ScheduledActionRow,
) -> Result<()> {
    let application_id = action.job_application_id.context("document action has no application")?;
    let application = load_evaluation_application(tx, scope, application_id).await?;
    ensure!(application.status == "submitted" && application.source_kind == "direct", "document action application is not submitted");
    let posting_row = read_posting_by_id(tx, scope, application.job_posting_id).await?.context("document posting is unavailable")?;
    let posting = posting_from_row(&posting_row)?;
    let rules = read_recruitment_rules_by_id(tx, scope.career_catalog_bundle_id, application.recruitment_ruleset_id).await?;
    let decision = rules.evaluate_document(DocumentEvaluationInput {
        world_seed: scope.world_seed,
        posting: &posting,
        visible_scores: application_visible_scores(&application),
        artifact_completeness_bp: application.artifact_completeness_bp,
    }).map_err(anyhow::Error::new)?;
    let initial = ApplicationState::Submitted {
        submitted_game_day: application.submitted_game_day,
        document_decision_game_day: application.submitted_game_day.checked_add(posting.document_review_days).context("document day overflowed")?,
        interview_delay_days: posting.interview_delay_days,
        offer_expiry_days: posting.offer_expiry_days,
    };
    let next = rules.transition_application(&initial, ApplicationAction::ResolveDocument {
        game_day: action.due_game_day,
        decision: decision.clone(),
    }).map_err(anyhow::Error::new)?;
    let (status, confirmation_day, interview_day) = match next {
        ApplicationState::DocumentRejected { .. } => ("documentRejected", None, None),
        ApplicationState::InterviewAwaitingConfirmation {
            confirmation_deadline_exclusive_game_day,
            interview_game_day,
            ..
        } => ("interviewAwaitingConfirmation", Some(confirmation_deadline_exclusive_game_day), Some(interview_game_day)),
        _ => bail!("document transition produced an invalid state"),
    };
    let update = sqlx::query(
        "UPDATE job_application
         SET status = ?, document_visible_fit_bp = ?,
             document_platform_affinity_bp = ?, document_score_bp = ?,
             document_probability_ppm = ?, document_roll = ?,
             document_decided_game_day = ?,
             confirmation_expires_exclusive_game_day = ?, interview_game_day = ?
         WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'submitted'",
    )
    .bind(status)
    .bind(decision.components.primary_fit_bp)
    .bind(decision.components.context_fit_bp)
    .bind(decision.score_bp)
    .bind(decision.probability_ppm)
    .bind(decision.roll_ppm)
    .bind(action.due_game_day)
    .bind(confirmation_day)
    .bind(interview_day)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(application.id)
    .execute(&mut **tx)
    .await?;
    ensure!(update.rows_affected() == 1, "document decision was lost");
    if let Some(interview_day) = interview_day {
        insert_application_action(tx, scope, application.recruitment_ruleset_id, application.id, "confirmationExpiry", 30, interview_day).await?;
    }
    complete_action(tx, action.id, action.due_game_day).await
}

async fn process_confirmation_expiry_action(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
    action: &ScheduledActionRow,
) -> Result<()> {
    let application_id = action.job_application_id.context("confirmation action has no application")?;
    let application = load_evaluation_application(tx, scope, application_id).await?;
    ensure!(application.status == "interviewAwaitingConfirmation", "confirmation expiry application is not awaiting confirmation");
    ensure!(application.confirmation_expires_exclusive_game_day == Some(action.due_game_day), "confirmation expiry day drifted");
    let update = sqlx::query(
        "UPDATE job_application
         SET terminal_from_status = status, terminal_game_day = ?, status = 'withdrawn'
         WHERE save_id = ? AND run_revision = ? AND id = ?
           AND status = 'interviewAwaitingConfirmation'",
    )
    .bind(action.due_game_day)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(application.id)
    .execute(&mut **tx)
    .await?;
    ensure!(update.rows_affected() == 1, "confirmation expiry was lost");
    complete_action(tx, action.id, action.due_game_day).await
}

async fn pinned_application_evidence_ids(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
    application: &EvaluationApplicationRow,
) -> Result<Vec<u64>> {
    let mut ids = Vec::new();
    for artifact_id in [application.resume_version_id, application.portfolio_version_id, application.linkedin_profile_version_id].into_iter().flatten() {
        let mut artifact_ids: Vec<u64> = sqlx::query_scalar(
            "SELECT evidence_id FROM profile_artifact_evidence
             WHERE save_id = ? AND run_revision = ? AND profile_artifact_version_id = ?
             ORDER BY selection_order",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(artifact_id)
        .fetch_all(&mut **tx)
        .await?;
        ids.append(&mut artifact_ids);
    }
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

async fn process_interview_action(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
    action: &ScheduledActionRow,
) -> Result<()> {
    let application_id = action.job_application_id.context("interview action has no application")?;
    let application = load_evaluation_application(tx, scope, application_id).await?;
    ensure!(application.status == "interviewConfirmed" && application.interview_game_day == Some(action.due_game_day), "interview action application is not due");
    let posting_row = read_posting_by_id(tx, scope, application.job_posting_id).await?.context("interview posting is unavailable")?;
    let posting = posting_from_row(&posting_row)?;
    let rules = read_recruitment_rules_by_id(tx, scope.career_catalog_bundle_id, application.recruitment_ruleset_id).await?;
    let (evidence, catalog) = read_score_inputs(tx, scope.save_id, scope.run_revision, scope.career_catalog_bundle_id).await?;
    let pinned_ids = pinned_application_evidence_ids(tx, scope, &application).await?;
    let currently_valid_ids = evidence.iter().filter(|item| item.acquired_game_day <= action.due_game_day && item.expires_on_game_day.is_none_or(|expiry| action.due_game_day <= expiry)).map(|item| item.evidence_id).collect::<Vec<_>>();
    let (possessed_scores, _) = calculate_scores(action.due_game_day, &posting.job_family_key, &[], &evidence, &catalog)?;
    let decision = rules.evaluate_interview(InterviewEvaluationInput {
        world_seed: scope.world_seed,
        posting: &posting,
        possessed_scores,
        pinned_evidence_ids: &pinned_ids,
        currently_valid_evidence_ids: &currently_valid_ids,
    }).map_err(anyhow::Error::new)?;
    let salary = if decision.passed {
        Some(rules.determine_offer_salary(OfferSalaryInput {
            world_seed: scope.world_seed,
            posting: &posting,
            possessed_fit_bp: decision.components.primary_fit_bp,
        }).map_err(anyhow::Error::new)?)
    } else {
        None
    };
    let entry_decision = entry_decision_for_application(tx, scope, &application, rules.ruleset()).await?;
    let initial = ApplicationState::InterviewConfirmed {
        entry_decision,
        confirmed_game_day: action.due_game_day.saturating_sub(1),
        interview_game_day: action.due_game_day,
        offer_expiry_days: posting.offer_expiry_days,
    };
    let next = rules.transition_application(&initial, ApplicationAction::ResolveInterview {
        game_day: action.due_game_day,
        decision: decision.clone(),
        salary,
    }).map_err(anyhow::Error::new)?;
    let (status, expires_game_day, salary) = match next {
        ApplicationState::InterviewRejected { .. } => ("interviewRejected", None, None),
        ApplicationState::Offered { expires_exclusive_game_day, salary, .. } => ("offered", Some(expires_exclusive_game_day), Some(salary)),
        _ => bail!("interview transition produced an invalid state"),
    };
    let offer_id = if let (Some(expires_game_day), Some(salary)) = (expires_game_day, salary) {
        let start_game_day = expires_game_day.checked_add(rules.ruleset().start_delay_days).context("employment start day overflowed")?;
        let insert = sqlx::query(
            "INSERT INTO job_offer
                 (save_id, run_revision, career_catalog_bundle_id, recruitment_ruleset_id,
                  job_application_id, job_posting_id, career_industry_id,
                  career_job_family_id, virtual_employer_id, status,
                  annual_salary_krw, region, employment_type, payday_day_of_month,
                  offered_game_day, start_game_day, expires_exclusive_game_day,
                  first_pay_reward_krw)
             SELECT ?, ?, posting.career_catalog_bundle_id, posting.recruitment_ruleset_id,
                    ?, posting.id, posting.career_industry_id,
                    posting.career_job_family_id, posting.virtual_employer_id, 'pending',
                    ?, posting.region, posting.employment_type, ?, ?, ?, ?,
                    platform.first_pay_reward_krw
             FROM job_posting AS posting
             INNER JOIN platform_catalog AS platform
               ON platform.career_catalog_bundle_id = posting.career_catalog_bundle_id
              AND platform.id = posting.platform_catalog_id
             WHERE posting.id = ?",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(application.id)
        .bind(salary.annual_salary_krw)
        .bind(rules.ruleset().monthly_payday)
        .bind(action.due_game_day)
        .bind(start_game_day)
        .bind(expires_game_day)
        .bind(posting_row.id)
        .execute(&mut **tx)
        .await?;
        Some(insert.last_insert_id())
    } else {
        None
    };
    let update = sqlx::query(
        "UPDATE job_application
         SET status = ?,
             possessed_education_score_bp = ?, possessed_certification_score_bp = ?,
             possessed_language_score_bp = ?, possessed_training_score_bp = ?,
             possessed_experience_score_bp = ?, possessed_project_score_bp = ?,
             possessed_fit_bp = ?, experience_project_fit_bp = ?,
             profile_consistency_bp = ?, interview_score_bp = ?,
             interview_probability_ppm = ?, interview_roll = ?,
             interview_decided_game_day = ?
         WHERE save_id = ? AND run_revision = ? AND id = ?
           AND status = 'interviewConfirmed'",
    )
    .bind(status)
    .bind(possessed_scores.education)
    .bind(possessed_scores.certification)
    .bind(possessed_scores.language)
    .bind(possessed_scores.training)
    .bind(possessed_scores.experience)
    .bind(possessed_scores.project)
    .bind(decision.components.primary_fit_bp)
    .bind(decision.components.supporting_fit_bp)
    .bind(decision.components.context_fit_bp)
    .bind(decision.score_bp)
    .bind(decision.probability_ppm)
    .bind(decision.roll_ppm)
    .bind(action.due_game_day)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(application.id)
    .execute(&mut **tx)
    .await?;
    ensure!(update.rows_affected() == 1, "interview decision was lost");
    if offer_id.is_some() {
        insert_application_action(tx, scope, application.recruitment_ruleset_id, application.id, "offerExpiry", 50, expires_game_day.context("offer expiry is missing")?).await?;
    }
    complete_action(tx, action.id, action.due_game_day).await
}

async fn entry_decision_for_application(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
    application: &EvaluationApplicationRow,
    rules: &RecruitmentRuleset,
) -> Result<StageDecision> {
    if application.source_kind == "direct" {
        let score = application.document_score_bp.context("document score is missing")?;
        return Ok(StageDecision {
            stage: RecruitmentStage::Document,
            score_band: score_band_for(score, rules)?,
            components: StageComponents {
                primary_fit_bp: application.document_visible_fit_bp.context("document fit is missing")?,
                supporting_fit_bp: application.artifact_completeness_bp,
                context_fit_bp: application.document_platform_affinity_bp.context("platform affinity is missing")?,
            },
            dimension_fit_bp: None,
            score_bp: score,
            probability_ppm: application.document_probability_ppm.context("document probability is missing")?,
            roll_ppm: application.document_roll.context("document roll is missing")?,
            passed: true,
        });
    }
    let row: (i64, u32, u32) = sqlx::query_as(
        "SELECT invitation.invitation_score_bp, invitation.invitation_probability_ppm,
                invitation.invitation_roll
         FROM job_application AS application
         INNER JOIN job_invitation AS invitation
           ON invitation.save_id = application.save_id
          AND invitation.run_revision = application.run_revision
          AND invitation.id = application.source_invitation_id
         WHERE application.save_id = ? AND application.run_revision = ? AND application.id = ?",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(application.id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(StageDecision {
        stage: RecruitmentStage::Invitation,
        score_band: score_band_for(row.0, rules)?,
        components: StageComponents { primary_fit_bp: row.0, supporting_fit_bp: 0, context_fit_bp: 0 },
        dimension_fit_bp: None,
        score_bp: row.0,
        probability_ppm: row.1,
        roll_ppm: row.2,
        passed: true,
    })
}

async fn process_offer_expiry_action(
    tx: &mut Transaction<'_, MySql>,
    scope: &RecruitmentScopeRow,
    action: &ScheduledActionRow,
) -> Result<()> {
    let application_id = action.job_application_id.context("offer expiry action has no application")?;
    let offer: (u64, u32) = sqlx::query_as(
        "SELECT id, expires_exclusive_game_day FROM job_offer
         WHERE save_id = ? AND run_revision = ? AND job_application_id = ?
           AND status = 'pending'",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(application_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("offer expiry has no pending offer")?;
    ensure!(offer.1 == action.due_game_day, "offer expiry day drifted");
    let offer_update = sqlx::query(
        "UPDATE job_offer SET status = 'expired', decided_game_day = ?
         WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'pending'",
    )
    .bind(action.due_game_day)
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(offer.0)
    .execute(&mut **tx)
    .await?;
    ensure!(offer_update.rows_affected() == 1, "offer expiry was lost");
    let app_update = sqlx::query(
        "UPDATE job_application SET status = 'expired'
         WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'offered'",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(application_id)
    .execute(&mut **tx)
    .await?;
    ensure!(app_update.rows_affected() == 1, "offer application expiry was lost");
    complete_action(tx, action.id, action.due_game_day).await
}

fn command_prefix(version: &str, cursor: CommandCursor) -> String {
    let mut canonical = String::new();
    push_fingerprint_field(&mut canonical, "version", version);
    push_fingerprint_field(
        &mut canonical,
        "expectedRunRevision",
        &cursor.expected_run_revision.to_string(),
    );
    push_fingerprint_field(
        &mut canonical,
        "expectedStateRevision",
        &cursor.expected_state_revision.to_string(),
    );
    push_fingerprint_field(
        &mut canonical,
        "expectedGameDay",
        &cursor.expected_game_day.to_string(),
    );
    canonical
}

fn push_fingerprint_field(canonical: &mut String, name: &str, value: &str) {
    canonical.push_str(name);
    canonical.push('=');
    canonical.push_str(&value.len().to_string());
    canonical.push(':');
    canonical.push_str(value);
    canonical.push('\n');
}

fn fingerprint(canonical: &str) -> String {
    Sha256::digest(canonical.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn apply_fingerprint(command: &ApplyCareerCommand) -> String {
    let mut canonical = command_prefix("lifeledger.career.application-submit.v1", command.cursor);
    push_fingerprint_field(&mut canonical, "postingKey", &command.posting_key);
    push_fingerprint_field(&mut canonical, "resumeVersionId", &optional_resource_id(command.resume_version_id));
    push_fingerprint_field(&mut canonical, "portfolioVersionId", &optional_resource_id(command.portfolio_version_id));
    push_fingerprint_field(&mut canonical, "linkedinProfileVersionId", &optional_resource_id(command.linkedin_profile_version_id));
    fingerprint(&canonical)
}

fn optional_resource_id(value: Option<ResourceId>) -> String {
    value.map_or_else(|| "none".to_owned(), |id| id.get().to_string())
}

fn interview_confirmation_fingerprint(command: &ConfirmCareerInterviewCommand) -> String {
    let mut canonical = command_prefix("lifeledger.career.interview-confirmation.v1", command.cursor);
    push_fingerprint_field(&mut canonical, "applicationId", &command.application_id.get().to_string());
    push_fingerprint_field(&mut canonical, "decision", match command.decision {
        InterviewDecision::Confirm => "confirm",
        InterviewDecision::Decline => "decline",
    });
    fingerprint(&canonical)
}

fn single_id_fingerprint(version: &str, cursor: CommandCursor, id_name: &str, id_value: u64) -> String {
    let mut canonical = command_prefix(version, cursor);
    push_fingerprint_field(&mut canonical, id_name, &id_value.to_string());
    fingerprint(&canonical)
}
