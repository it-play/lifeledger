use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use time::{Date, Month};

use super::types::{
    BridgeCatalog, BridgeEducationMapping, BridgeError, BridgeEvidenceDraft, BridgeEvidenceKey,
    BridgeEvidencePlanner, BridgeExperienceMapping, BridgePlanInput, DimensionRequirement,
    DimensionScores, EvidenceKind, EvidencePeriodFields, InitialCareerPlan,
    MAX_BRIDGE_CAREER_YEARS, MAX_BRIDGE_CERTIFICATIONS, SPEC_SCORE_CAP_BP, ScoreError, ScoreFit,
    ScoreFitInput, ScoreViews, SpecCatalogEntry, SpecDimension, SpecEvidence, SpecScoreInput,
    SpecScoreRules,
};

struct V1SpecScoreRules;

struct V1BridgeEvidencePlanner;

pub fn create_spec_score_rules() -> Arc<dyn SpecScoreRules> {
    Arc::new(V1SpecScoreRules)
}

pub fn create_bridge_evidence_planner() -> Arc<dyn BridgeEvidencePlanner> {
    Arc::new(V1BridgeEvidencePlanner)
}

impl SpecScoreRules for V1SpecScoreRules {
    fn calculate_score_views(&self, input: SpecScoreInput<'_>) -> Result<ScoreViews, ScoreError> {
        if input.evaluated_job_family_key.trim().is_empty() {
            return Err(ScoreError::EmptyJobFamilyKey);
        }

        let catalog = validate_score_input(&input)?;
        let visible_ids = input
            .visible_evidence_ids
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        for visible_id in &visible_ids {
            if !input
                .evidence
                .iter()
                .any(|evidence| evidence.evidence_id == *visible_id)
            {
                return Err(ScoreError::UnknownVisibleEvidenceId(*visible_id));
            }
        }

        let possessed = calculate_scores(
            input.evaluated_job_family_key,
            input.current_game_day,
            input.evidence,
            &catalog,
            None,
        )?;
        let visible = calculate_scores(
            input.evaluated_job_family_key,
            input.current_game_day,
            input.evidence,
            &catalog,
            Some(&visible_ids),
        )?;

        Ok(ScoreViews { possessed, visible })
    }

    fn calculate_fit(&self, input: ScoreFitInput<'_>) -> Result<ScoreFit, ScoreError> {
        let requirements = validate_requirements(input.candidate_scores, input.requirements)?;
        let mut dimension_fit_bp = DimensionScores::default();
        let mut weighted_total = 0_i128;

        for dimension in SpecDimension::ALL {
            let requirement = requirements
                .get(&dimension)
                .copied()
                .ok_or(ScoreError::MissingRequirement(dimension))?;
            let candidate_score = input.candidate_scores.get(dimension);
            let fit = if requirement.required_score_bp == 0 {
                SPEC_SCORE_CAP_BP
            } else {
                let scaled = i128::from(candidate_score)
                    .checked_mul(i128::from(SPEC_SCORE_CAP_BP))
                    .ok_or(ScoreError::ArithmeticOverflow)?;
                let quotient = scaled
                    .checked_div(i128::from(requirement.required_score_bp))
                    .ok_or(ScoreError::ArithmeticOverflow)?;
                i64::try_from(quotient.min(i128::from(SPEC_SCORE_CAP_BP)))
                    .map_err(|_| ScoreError::ArithmeticOverflow)?
            };
            dimension_fit_bp.set(dimension, fit);

            let weighted = i128::from(fit)
                .checked_mul(i128::from(requirement.weight_bp))
                .ok_or(ScoreError::ArithmeticOverflow)?;
            weighted_total = weighted_total
                .checked_add(weighted)
                .ok_or(ScoreError::ArithmeticOverflow)?;
        }

        let overall = weighted_total
            .checked_div(i128::from(SPEC_SCORE_CAP_BP))
            .ok_or(ScoreError::ArithmeticOverflow)?;

        Ok(ScoreFit {
            dimension_fit_bp,
            overall_fit_bp: i64::try_from(overall).map_err(|_| ScoreError::ArithmeticOverflow)?,
        })
    }
}

fn validate_score_input<'a>(
    input: &'a SpecScoreInput<'a>,
) -> Result<HashMap<&'a str, &'a SpecCatalogEntry>, ScoreError> {
    let mut catalog = HashMap::with_capacity(input.catalog.len());
    for entry in input.catalog {
        if catalog
            .insert(entry.catalog_entry_key.as_str(), entry)
            .is_some()
        {
            return Err(ScoreError::DuplicateCatalogEntryKey(
                entry.catalog_entry_key.clone(),
            ));
        }

        let mut job_families = HashSet::with_capacity(entry.contributions.len());
        for contribution in &entry.contributions {
            if contribution.job_family_key.trim().is_empty() {
                return Err(ScoreError::EmptyJobFamilyKey);
            }
            if contribution.contribution_bp < 0 {
                return Err(ScoreError::NegativeContribution(
                    entry.catalog_entry_key.clone(),
                ));
            }
            if !job_families.insert(contribution.job_family_key.as_str()) {
                return Err(ScoreError::DuplicateJobFamilyContribution(
                    entry.catalog_entry_key.clone(),
                ));
            }
        }
    }

    let mut evidence_ids = HashSet::with_capacity(input.evidence.len());
    for evidence in input.evidence {
        if !evidence_ids.insert(evidence.evidence_id) {
            return Err(ScoreError::DuplicateEvidenceId(evidence.evidence_id));
        }
        let entry = catalog
            .get(evidence.catalog_entry_key.as_str())
            .ok_or_else(|| ScoreError::UnknownCatalogEntry(evidence.catalog_entry_key.clone()))?;
        if entry.kind != evidence.kind {
            return Err(ScoreError::EvidenceKindMismatch(evidence.evidence_id));
        }
    }

    Ok(catalog)
}

fn calculate_scores(
    job_family_key: &str,
    current_game_day: u32,
    evidence: &[SpecEvidence],
    catalog: &HashMap<&str, &SpecCatalogEntry>,
    included_ids: Option<&HashSet<u64>>,
) -> Result<DimensionScores, ScoreError> {
    let mut candidates = evidence
        .iter()
        .filter(|item| {
            included_ids.is_none_or(|ids| ids.contains(&item.evidence_id))
                && item
                    .expires_on_game_day
                    .is_none_or(|expires_on| current_game_day <= expires_on)
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|item| (item.acquired_game_day, item.evidence_id));

    let mut accepted_non_stackable = HashSet::new();
    let mut scores = DimensionScores::default();
    for item in candidates {
        let entry = catalog
            .get(item.catalog_entry_key.as_str())
            .ok_or_else(|| ScoreError::UnknownCatalogEntry(item.catalog_entry_key.clone()))?;
        if !entry.stackable && !accepted_non_stackable.insert(entry.catalog_entry_key.as_str()) {
            continue;
        }
        let contribution = entry
            .contributions
            .iter()
            .find(|contribution| contribution.job_family_key == job_family_key)
            .map_or(0, |contribution| contribution.contribution_bp);
        let dimension = item.kind.dimension();
        let next = scores
            .get(dimension)
            .checked_add(contribution)
            .ok_or(ScoreError::ArithmeticOverflow)?;
        scores.set(dimension, next);
    }
    for dimension in SpecDimension::ALL {
        scores.set(dimension, scores.get(dimension).min(SPEC_SCORE_CAP_BP));
    }

    Ok(scores)
}

fn validate_requirements(
    candidate_scores: DimensionScores,
    requirements: &[DimensionRequirement],
) -> Result<HashMap<SpecDimension, &DimensionRequirement>, ScoreError> {
    for dimension in SpecDimension::ALL {
        if !(0..=SPEC_SCORE_CAP_BP).contains(&candidate_scores.get(dimension)) {
            return Err(ScoreError::InvalidCandidateScore(dimension));
        }
    }

    let mut by_dimension = HashMap::with_capacity(requirements.len());
    let mut weight_total = 0_i128;
    for requirement in requirements {
        if !(0..=SPEC_SCORE_CAP_BP).contains(&requirement.required_score_bp)
            || !(0..=SPEC_SCORE_CAP_BP).contains(&requirement.weight_bp)
        {
            return Err(ScoreError::InvalidRequirement(requirement.dimension));
        }
        if by_dimension
            .insert(requirement.dimension, requirement)
            .is_some()
        {
            return Err(ScoreError::DuplicateRequirement(requirement.dimension));
        }
        weight_total = weight_total
            .checked_add(i128::from(requirement.weight_bp))
            .ok_or(ScoreError::ArithmeticOverflow)?;
    }
    for dimension in SpecDimension::ALL {
        if !by_dimension.contains_key(&dimension) {
            return Err(ScoreError::MissingRequirement(dimension));
        }
    }
    if weight_total != i128::from(SPEC_SCORE_CAP_BP) {
        return Err(ScoreError::InvalidWeightTotal);
    }

    Ok(by_dimension)
}

impl BridgeEvidencePlanner for V1BridgeEvidencePlanner {
    fn plan_initial_evidence(
        &self,
        input: BridgePlanInput<'_>,
    ) -> Result<InitialCareerPlan, BridgeError> {
        validate_bridge_catalog(input.catalog)?;
        if input.certifications > MAX_BRIDGE_CERTIFICATIONS {
            return Err(BridgeError::CertificationCountOutOfRange);
        }
        if input.career_years > MAX_BRIDGE_CAREER_YEARS {
            return Err(BridgeError::CareerYearsOutOfRange);
        }

        let birth_year = input
            .world_start_date
            .year()
            .checked_sub(
                i32::try_from(input.starting_age_years)
                    .map_err(|_| BridgeError::ArithmeticOverflow)?,
            )
            .ok_or(BridgeError::ArithmeticOverflow)?;
        let birth_date = Date::from_calendar_date(birth_year, Month::January, 1)
            .map_err(|_| BridgeError::InvalidDate)?;
        let education = input
            .catalog
            .education_mappings
            .iter()
            .find(|mapping| mapping.education == input.education)
            .ok_or(BridgeError::InvalidEducationMappings)?;
        let experience = input
            .catalog
            .experience_mappings
            .iter()
            .find(|mapping| mapping.career_years == input.career_years)
            .ok_or(BridgeError::InvalidExperienceMappings)?;

        let mut evidence = Vec::with_capacity(
            usize::try_from(input.certifications)
                .map_err(|_| BridgeError::ArithmeticOverflow)?
                .checked_add(2)
                .ok_or(BridgeError::ArithmeticOverflow)?,
        );
        evidence.push(bridge_draft(
            &education.evidence,
            EvidenceKind::Education,
            EvidencePeriodFields::none(),
        ));
        for certification in input.catalog.certification_order.iter().take(
            usize::try_from(input.certifications).map_err(|_| BridgeError::ArithmeticOverflow)?,
        ) {
            evidence.push(bridge_draft(
                certification,
                EvidenceKind::Certification,
                EvidencePeriodFields::none(),
            ));
        }

        let period = if input.career_years == 0 {
            EvidencePeriodFields::zero_year_bridge(input.world_start_date)
        } else {
            let start_date = subtract_years_clamped(input.world_start_date, input.career_years)?;
            EvidencePeriodFields::regular(start_date, input.world_start_date)
        };
        evidence.push(bridge_draft(
            &experience.evidence,
            EvidenceKind::Experience,
            period,
        ));

        Ok(InitialCareerPlan {
            focused_job_family_key: input.catalog.default_focused_job_family_key.clone(),
            birth_date,
            evidence,
        })
    }
}

fn bridge_draft(
    entry: &BridgeEvidenceKey,
    kind: EvidenceKind,
    period: EvidencePeriodFields,
) -> BridgeEvidenceDraft {
    BridgeEvidenceDraft {
        evidence_key: entry.evidence_key.clone(),
        catalog_entry_key: entry.catalog_entry_key.clone(),
        kind,
        acquired_game_day: 0,
        period,
    }
}

fn validate_bridge_catalog(catalog: &BridgeCatalog) -> Result<(), BridgeError> {
    if catalog.default_focused_job_family_key.trim().is_empty() {
        return Err(BridgeError::EmptyDefaultFocus);
    }
    validate_education_mappings(&catalog.education_mappings)?;
    if catalog.certification_order.len()
        != usize::try_from(MAX_BRIDGE_CERTIFICATIONS)
            .map_err(|_| BridgeError::ArithmeticOverflow)?
    {
        return Err(BridgeError::InvalidCertificationOrder);
    }
    validate_experience_mappings(&catalog.experience_mappings)?;

    let all_entries = catalog
        .education_mappings
        .iter()
        .map(|mapping| &mapping.evidence)
        .chain(catalog.certification_order.iter())
        .chain(
            catalog
                .experience_mappings
                .iter()
                .map(|mapping| &mapping.evidence),
        );
    let mut evidence_keys = HashSet::new();
    let mut catalog_keys = HashSet::new();
    for entry in all_entries {
        if entry.evidence_key.trim().is_empty() || entry.catalog_entry_key.trim().is_empty() {
            return Err(BridgeError::EmptyBridgeKey);
        }
        if !evidence_keys.insert(entry.evidence_key.as_str()) {
            return Err(BridgeError::DuplicateEvidenceKey(
                entry.evidence_key.clone(),
            ));
        }
        if !catalog_keys.insert(entry.catalog_entry_key.as_str()) {
            return Err(BridgeError::DuplicateCatalogEntryKey(
                entry.catalog_entry_key.clone(),
            ));
        }
    }

    Ok(())
}

fn validate_education_mappings(mappings: &[BridgeEducationMapping]) -> Result<(), BridgeError> {
    use crate::character::Education;

    let expected = [
        Education::HighSchool,
        Education::Associate,
        Education::Bachelor,
        Education::Master,
        Education::Doctorate,
    ];
    if mappings.len() != expected.len() {
        return Err(BridgeError::InvalidEducationMappings);
    }
    if expected.into_iter().any(|education| {
        mappings
            .iter()
            .filter(|mapping| mapping.education == education)
            .count()
            != 1
    }) {
        return Err(BridgeError::InvalidEducationMappings);
    }

    Ok(())
}

fn validate_experience_mappings(mappings: &[BridgeExperienceMapping]) -> Result<(), BridgeError> {
    let expected_len = usize::try_from(MAX_BRIDGE_CAREER_YEARS)
        .map_err(|_| BridgeError::ArithmeticOverflow)?
        .checked_add(1)
        .ok_or(BridgeError::ArithmeticOverflow)?;
    if mappings.len() != expected_len {
        return Err(BridgeError::InvalidExperienceMappings);
    }
    let actual = mappings
        .iter()
        .map(|mapping| mapping.career_years)
        .collect::<HashSet<_>>();
    if (0..=MAX_BRIDGE_CAREER_YEARS).any(|years| !actual.contains(&years)) {
        return Err(BridgeError::InvalidExperienceMappings);
    }

    Ok(())
}

fn subtract_years_clamped(date: Date, years: u32) -> Result<Date, BridgeError> {
    let target_year = date
        .year()
        .checked_sub(i32::try_from(years).map_err(|_| BridgeError::ArithmeticOverflow)?)
        .ok_or(BridgeError::ArithmeticOverflow)?;
    let mut day = date.day();
    loop {
        if let Ok(candidate) = Date::from_calendar_date(target_year, date.month(), day) {
            return Ok(candidate);
        }
        day = day.checked_sub(1).ok_or(BridgeError::InvalidDate)?;
    }
}

#[cfg(test)]
mod tests {
    use time::Month;

    use super::*;
    use crate::career::types::JobFamilyContribution;
    use crate::character::Education;

    fn given_evidence(
        evidence_id: u64,
        catalog_entry_key: &str,
        kind: EvidenceKind,
        acquired_game_day: u32,
        expires_on_game_day: Option<u32>,
    ) -> SpecEvidence {
        SpecEvidence {
            evidence_id,
            evidence_key: format!("evidence-{evidence_id}"),
            catalog_entry_key: catalog_entry_key.to_owned(),
            kind,
            acquired_game_day,
            expires_on_game_day,
            period: EvidencePeriodFields::none(),
        }
    }

    fn given_catalog_entry(
        key: &str,
        kind: EvidenceKind,
        stackable: bool,
        software_bp: i64,
        finance_bp: i64,
    ) -> SpecCatalogEntry {
        SpecCatalogEntry {
            catalog_entry_key: key.to_owned(),
            kind,
            stackable,
            contributions: vec![
                JobFamilyContribution {
                    job_family_key: "software".to_owned(),
                    contribution_bp: software_bp,
                },
                JobFamilyContribution {
                    job_family_key: "finance".to_owned(),
                    contribution_bp: finance_bp,
                },
            ],
        }
    }

    fn given_date(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("유효한 테스트 날짜여야 한다")
    }

    fn given_bridge_key(prefix: &str, ordinal: u32) -> BridgeEvidenceKey {
        BridgeEvidenceKey {
            evidence_key: format!("{prefix}-evidence-{ordinal}"),
            catalog_entry_key: format!("{prefix}-catalog-{ordinal}"),
        }
    }

    fn given_bridge_catalog() -> BridgeCatalog {
        let educations = [
            Education::HighSchool,
            Education::Associate,
            Education::Bachelor,
            Education::Master,
            Education::Doctorate,
        ];
        BridgeCatalog {
            default_focused_job_family_key: "software".to_owned(),
            education_mappings: educations
                .into_iter()
                .enumerate()
                .map(|(index, education)| BridgeEducationMapping {
                    education,
                    evidence: given_bridge_key(
                        "education",
                        u32::try_from(index).expect("테스트 인덱스를 변환해야 한다"),
                    ),
                })
                .collect(),
            certification_order: (0..MAX_BRIDGE_CERTIFICATIONS)
                .map(|index| given_bridge_key("certification", index))
                .collect(),
            experience_mappings: (0..=MAX_BRIDGE_CAREER_YEARS)
                .map(|career_years| BridgeExperienceMapping {
                    career_years,
                    evidence: given_bridge_key("experience", career_years),
                })
                .collect(),
        }
    }

    mod context_직무별_보유점수와_표시점수를_계산하는_경우 {
        use super::*;

        #[test]
        fn given_직무기여도와_표시목록_when_계산하면_then_두_관점을_분리한다() {
            let catalog = vec![
                given_catalog_entry("degree", EvidenceKind::Education, false, 3_000, 5_000),
                given_catalog_entry("project", EvidenceKind::Project, true, 2_000, 200),
            ];
            let evidence = vec![
                given_evidence(1, "degree", EvidenceKind::Education, 0, None),
                given_evidence(2, "project", EvidenceKind::Project, 1, None),
            ];

            let result = create_spec_score_rules()
                .calculate_score_views(SpecScoreInput {
                    evaluated_job_family_key: "software",
                    current_game_day: 10,
                    evidence: &evidence,
                    catalog: &catalog,
                    visible_evidence_ids: &[2],
                })
                .expect("보유점수와 표시점수를 계산해야 한다");

            assert_eq!(result.possessed.education, 3_000);
            assert_eq!(result.possessed.project, 2_000);
            assert_eq!(result.visible.education, 0);
            assert_eq!(result.visible.project, 2_000);
        }

        #[test]
        fn given_다른_직무군_when_평가하면_then_그_직무의_기여도만_사용한다() {
            let catalog = vec![given_catalog_entry(
                "degree",
                EvidenceKind::Education,
                false,
                3_000,
                5_000,
            )];
            let evidence = vec![given_evidence(
                1,
                "degree",
                EvidenceKind::Education,
                0,
                None,
            )];

            let result = create_spec_score_rules()
                .calculate_score_views(SpecScoreInput {
                    evaluated_job_family_key: "finance",
                    current_game_day: 0,
                    evidence: &evidence,
                    catalog: &catalog,
                    visible_evidence_ids: &[1],
                })
                .expect("공고 직무군 점수를 계산해야 한다");

            assert_eq!(result.possessed.education, 5_000);
            assert_eq!(result.visible.education, 5_000);
        }
    }

    mod context_만료와_중복_증거가_있는_경우 {
        use super::*;

        #[test]
        fn given_만료일_당일과_다음날_when_계산하면_then_다음날부터_제외한다() {
            let catalog = vec![given_catalog_entry(
                "language",
                EvidenceKind::Language,
                true,
                1_500,
                1_500,
            )];
            let evidence = vec![given_evidence(
                1,
                "language",
                EvidenceKind::Language,
                0,
                Some(10),
            )];
            let rules = create_spec_score_rules();

            let on_expiry = rules
                .calculate_score_views(SpecScoreInput {
                    evaluated_job_family_key: "software",
                    current_game_day: 10,
                    evidence: &evidence,
                    catalog: &catalog,
                    visible_evidence_ids: &[1],
                })
                .expect("만료일 점수를 계산해야 한다");
            let after_expiry = rules
                .calculate_score_views(SpecScoreInput {
                    evaluated_job_family_key: "software",
                    current_game_day: 11,
                    evidence: &evidence,
                    catalog: &catalog,
                    visible_evidence_ids: &[1],
                })
                .expect("만료 다음날 점수를 계산해야 한다");

            assert_eq!(on_expiry.possessed.language, 1_500);
            assert_eq!(after_expiry.possessed.language, 0);
        }

        #[test]
        fn given_비중첩_증거의_취득일이_같을때_when_계산하면_then_작은_id_하나만_인정한다() {
            let catalog = vec![given_catalog_entry(
                "certificate",
                EvidenceKind::Certification,
                false,
                7_000,
                7_000,
            )];
            let evidence = vec![
                given_evidence(9, "certificate", EvidenceKind::Certification, 3, None),
                given_evidence(2, "certificate", EvidenceKind::Certification, 3, None),
            ];

            let result = create_spec_score_rules()
                .calculate_score_views(SpecScoreInput {
                    evaluated_job_family_key: "software",
                    current_game_day: 3,
                    evidence: &evidence,
                    catalog: &catalog,
                    visible_evidence_ids: &[9, 2],
                })
                .expect("비중첩 점수를 계산해야 한다");

            assert_eq!(result.possessed.certification, 7_000);
        }

        #[test]
        fn given_중첩가능_기여도의_합이_상한을_넘을때_when_계산하면_then_만점에서_자른다() {
            let catalog = vec![given_catalog_entry(
                "project",
                EvidenceKind::Project,
                true,
                6_000,
                6_000,
            )];
            let evidence = vec![
                given_evidence(1, "project", EvidenceKind::Project, 0, None),
                given_evidence(2, "project", EvidenceKind::Project, 1, None),
            ];

            let result = create_spec_score_rules()
                .calculate_score_views(SpecScoreInput {
                    evaluated_job_family_key: "software",
                    current_game_day: 1,
                    evidence: &evidence,
                    catalog: &catalog,
                    visible_evidence_ids: &[1, 2],
                })
                .expect("상한 점수를 계산해야 한다");

            assert_eq!(result.possessed.project, SPEC_SCORE_CAP_BP);
        }

        #[test]
        fn given_i64_합산범위를_넘는_기여도_when_계산하면_then_상한으로_숨기지_않고_오류를_반환한다()
         {
            let catalog = vec![given_catalog_entry(
                "project",
                EvidenceKind::Project,
                true,
                i64::MAX,
                i64::MAX,
            )];
            let evidence = vec![
                given_evidence(1, "project", EvidenceKind::Project, 0, None),
                given_evidence(2, "project", EvidenceKind::Project, 1, None),
            ];

            let result = create_spec_score_rules().calculate_score_views(SpecScoreInput {
                evaluated_job_family_key: "software",
                current_game_day: 1,
                evidence: &evidence,
                catalog: &catalog,
                visible_evidence_ids: &[1, 2],
            });

            assert_eq!(result, Err(ScoreError::ArithmeticOverflow));
        }
    }

    mod context_여섯_차원_적합도를_계산하는_경우 {
        use super::*;

        fn given_requirements() -> Vec<DimensionRequirement> {
            SpecDimension::ALL
                .into_iter()
                .map(|dimension| DimensionRequirement {
                    dimension,
                    required_score_bp: if dimension == SpecDimension::Language {
                        0
                    } else {
                        5_000
                    },
                    weight_bp: if dimension == SpecDimension::Project {
                        5_000
                    } else {
                        1_000
                    },
                })
                .collect()
        }

        #[test]
        fn given_요구점수_0과_차원가중치_when_계산하면_then_정수_적합도를_반환한다() {
            let scores = DimensionScores {
                education: 2_500,
                certification: 5_000,
                language: 0,
                training: 5_000,
                experience: 5_000,
                project: 5_000,
            };
            let requirements = given_requirements();

            let result = create_spec_score_rules()
                .calculate_fit(ScoreFitInput {
                    candidate_scores: scores,
                    requirements: &requirements,
                })
                .expect("적합도를 계산해야 한다");

            assert_eq!(result.dimension_fit_bp.education, 5_000);
            assert_eq!(result.dimension_fit_bp.language, SPEC_SCORE_CAP_BP);
            assert_eq!(result.overall_fit_bp, 9_500);
        }

        #[test]
        fn given_가중치합이_만점이_아닐때_when_계산하면_then_게시오류로_거절한다() {
            let mut requirements = given_requirements();
            requirements[0].weight_bp = 999;

            let result = create_spec_score_rules().calculate_fit(ScoreFitInput {
                candidate_scores: DimensionScores::default(),
                requirements: &requirements,
            });

            assert_eq!(result, Err(ScoreError::InvalidWeightTotal));
        }
    }

    mod context_기존_캐릭터를_브리지하는_경우 {
        use super::*;

        #[test]
        fn given_학력과_자격증_n개와_경력_n년_when_계획하면_then_고정순서와_기간으로_만든다() {
            let catalog = given_bridge_catalog();
            let world_start = given_date(2026, Month::March, 1);

            let result = create_bridge_evidence_planner()
                .plan_initial_evidence(BridgePlanInput {
                    catalog: &catalog,
                    education: Education::Bachelor,
                    certifications: 2,
                    career_years: 3,
                    starting_age_years: 29,
                    world_start_date: world_start,
                })
                .expect("초기 브리지 증거를 계획해야 한다");

            assert_eq!(result.focused_job_family_key, "software");
            assert_eq!(result.birth_date, given_date(1997, Month::January, 1));
            assert_eq!(result.evidence.len(), 4);
            assert_eq!(result.evidence[0].kind, EvidenceKind::Education);
            assert_eq!(result.evidence[1].evidence_key, "certification-evidence-0");
            assert_eq!(result.evidence[2].evidence_key, "certification-evidence-1");
            assert_eq!(
                result.evidence[3].period,
                EvidencePeriodFields::regular(given_date(2023, Month::March, 1), world_start)
            );
        }

        #[test]
        fn given_경력_0년_when_계획하면_then_같은_시작과_끝의_전용_빈기간을_만든다() {
            let catalog = given_bridge_catalog();
            let world_start = given_date(2026, Month::January, 1);

            let result = create_bridge_evidence_planner()
                .plan_initial_evidence(BridgePlanInput {
                    catalog: &catalog,
                    education: Education::HighSchool,
                    certifications: 0,
                    career_years: 0,
                    starting_age_years: 19,
                    world_start_date: world_start,
                })
                .expect("0년 경력 브리지를 계획해야 한다");

            assert_eq!(result.evidence.len(), 2);
            assert_eq!(
                result.evidence[1].period,
                EvidencePeriodFields::zero_year_bridge(world_start)
            );
        }

        #[test]
        fn given_자격증이_50개를_넘을때_when_계획하면_then_자르지_않고_거절한다() {
            let catalog = given_bridge_catalog();

            let result = create_bridge_evidence_planner().plan_initial_evidence(BridgePlanInput {
                catalog: &catalog,
                education: Education::HighSchool,
                certifications: 51,
                career_years: 0,
                starting_age_years: 19,
                world_start_date: given_date(2026, Month::January, 1),
            });

            assert_eq!(result, Err(BridgeError::CertificationCountOutOfRange));
        }

        #[test]
        fn given_브리지_카탈로그에_중복키가_있을때_when_계획하면_then_게시입력을_거절한다() {
            let mut catalog = given_bridge_catalog();
            catalog.certification_order[1].evidence_key =
                catalog.certification_order[0].evidence_key.clone();

            let result = create_bridge_evidence_planner().plan_initial_evidence(BridgePlanInput {
                catalog: &catalog,
                education: Education::HighSchool,
                certifications: 0,
                career_years: 0,
                starting_age_years: 19,
                world_start_date: given_date(2026, Month::January, 1),
            });

            assert!(matches!(result, Err(BridgeError::DuplicateEvidenceKey(_))));
        }
    }
}
