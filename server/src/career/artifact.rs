use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use time::Date;

use super::types::{
    ARTIFACT_COMPLETENESS_SCALE_BP, ArtifactChecklistRule, ArtifactCompletenessInput,
    ArtifactError, ArtifactKind, ArtifactRules, ArtifactValidationInput, CanonicalArtifact,
    ChecklistRule, EvidenceKind, EvidencePeriodKind, LinkedinFields, SpecDimension, SpecEvidence,
};

const MAX_HEADLINE_SCALARS: usize = 120;
const MAX_SUMMARY_SCALARS: usize = 2_000;
const MAX_PORTFOLIO_EVIDENCE: usize = 12;
const MAX_RESUME_EVIDENCE: usize = 40;
const MAX_LINKEDIN_EVIDENCE: usize = 30;
const MAX_LINKEDIN_INDUSTRIES: usize = 3;
const MINIMUM_RESUME_AGE_YEARS: u32 = 15;

struct V1ArtifactRules;

pub fn create_artifact_rules() -> Arc<dyn ArtifactRules> {
    Arc::new(V1ArtifactRules)
}

impl ArtifactRules for V1ArtifactRules {
    fn canonicalize(
        &self,
        input: ArtifactValidationInput<'_>,
    ) -> Result<CanonicalArtifact, ArtifactError> {
        let evidence_by_id = validate_owned_evidence(input.owned_evidence)?;
        let headline = input.draft.headline.trim().to_owned();
        let summary = input.draft.summary.trim().to_owned();
        validate_text(&headline, &summary)?;

        let evidence_ids =
            canonical_evidence_ids(input.draft.kind, &input.draft.evidence_ids, &evidence_by_id)?;
        let linkedin = canonical_linkedin(input.draft.kind, input.draft.linkedin.as_ref())?;
        if input.draft.kind == ArtifactKind::Resume {
            validate_resume_periods(
                &evidence_ids,
                &evidence_by_id,
                input.birth_date,
                input.current_date,
            )?;
        }

        Ok(CanonicalArtifact {
            kind: input.draft.kind,
            headline,
            summary,
            evidence_ids,
            linkedin,
        })
    }

    fn validate_checklist(
        &self,
        kind: ArtifactKind,
        checklist: &[ArtifactChecklistRule],
    ) -> Result<(), ArtifactError> {
        validate_checklist(kind, checklist)
    }

    fn calculate_completeness(
        &self,
        input: ArtifactCompletenessInput<'_>,
    ) -> Result<i64, ArtifactError> {
        validate_checklist(input.artifact.kind, input.checklist)?;
        let evidence_by_id = validate_owned_evidence(input.owned_evidence)?;
        validate_canonical_artifact(input.artifact, &evidence_by_id)?;

        let evidence = input
            .artifact
            .evidence_ids
            .iter()
            .map(|id| {
                evidence_by_id
                    .get(id)
                    .copied()
                    .ok_or(ArtifactError::UnknownEvidenceId(*id))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut completeness = 0_i128;
        for checklist_rule in input.checklist {
            if checklist_rule_matches(&checklist_rule.rule, input.artifact, &evidence) {
                completeness = completeness
                    .checked_add(i128::from(checklist_rule.weight_bp))
                    .ok_or(ArtifactError::ArithmeticOverflow)?;
            }
        }

        i64::try_from(completeness).map_err(|_| ArtifactError::ArithmeticOverflow)
    }
}

fn validate_text(headline: &str, summary: &str) -> Result<(), ArtifactError> {
    if headline.is_empty() {
        return Err(ArtifactError::EmptyHeadline);
    }
    if headline.chars().count() > MAX_HEADLINE_SCALARS {
        return Err(ArtifactError::HeadlineTooLong);
    }
    if summary.chars().count() > MAX_SUMMARY_SCALARS {
        return Err(ArtifactError::SummaryTooLong);
    }
    if headline
        .chars()
        .any(|character| is_c0_control(character) || is_non_c0_line_break(character))
    {
        return Err(ArtifactError::ForbiddenHeadlineControl);
    }
    if summary
        .chars()
        .any(|character| is_c0_control(character) && character != '\n' && character != '\t')
    {
        return Err(ArtifactError::ForbiddenSummaryControl);
    }

    Ok(())
}

const fn is_c0_control(character: char) -> bool {
    character <= '\u{001f}'
}

const fn is_non_c0_line_break(character: char) -> bool {
    matches!(character, '\u{0085}' | '\u{2028}' | '\u{2029}')
}

fn validate_owned_evidence(
    evidence: &[SpecEvidence],
) -> Result<HashMap<u64, &SpecEvidence>, ArtifactError> {
    let mut by_id = HashMap::with_capacity(evidence.len());
    for item in evidence {
        if by_id.insert(item.evidence_id, item).is_some() {
            return Err(ArtifactError::DuplicateOwnedEvidenceId(item.evidence_id));
        }
    }

    Ok(by_id)
}

fn canonical_evidence_ids(
    kind: ArtifactKind,
    requested_ids: &[u64],
    evidence_by_id: &HashMap<u64, &SpecEvidence>,
) -> Result<Vec<u64>, ArtifactError> {
    let maximum = maximum_evidence_count(kind);
    let mut seen = HashSet::with_capacity(requested_ids.len());
    let mut canonical = Vec::with_capacity(requested_ids.len().min(maximum));
    for evidence_id in requested_ids {
        if !seen.insert(*evidence_id) {
            continue;
        }
        let evidence = evidence_by_id
            .get(evidence_id)
            .copied()
            .ok_or(ArtifactError::UnknownEvidenceId(*evidence_id))?;
        if !artifact_allows_evidence(kind, evidence.kind) {
            return Err(ArtifactError::EvidenceKindNotAllowed {
                kind,
                evidence_id: *evidence_id,
            });
        }
        canonical.push(*evidence_id);
    }
    if canonical.len() > maximum {
        return Err(ArtifactError::TooManyEvidence { kind, maximum });
    }

    Ok(canonical)
}

fn canonical_linkedin(
    kind: ArtifactKind,
    fields: Option<&LinkedinFields>,
) -> Result<Option<LinkedinFields>, ArtifactError> {
    match (kind, fields) {
        (ArtifactKind::LinkedinProfile, Some(fields)) => {
            let mut seen = HashSet::with_capacity(fields.industries.len());
            let industries = fields
                .industries
                .iter()
                .copied()
                .filter(|industry| seen.insert(*industry))
                .collect::<Vec<_>>();
            if industries.len() > MAX_LINKEDIN_INDUSTRIES {
                return Err(ArtifactError::InvalidLinkedinFields);
            }
            Ok(Some(LinkedinFields {
                open_to_work: fields.open_to_work,
                industries,
            }))
        }
        (ArtifactKind::LinkedinProfile, None) => Err(ArtifactError::InvalidLinkedinFields),
        (_, Some(_)) => Err(ArtifactError::InvalidLinkedinFields),
        (_, None) => Ok(None),
    }
}

const fn maximum_evidence_count(kind: ArtifactKind) -> usize {
    match kind {
        ArtifactKind::Portfolio => MAX_PORTFOLIO_EVIDENCE,
        ArtifactKind::Resume => MAX_RESUME_EVIDENCE,
        ArtifactKind::LinkedinProfile => MAX_LINKEDIN_EVIDENCE,
    }
}

const fn artifact_allows_evidence(kind: ArtifactKind, evidence_kind: EvidenceKind) -> bool {
    match kind {
        ArtifactKind::Portfolio => matches!(
            evidence_kind,
            EvidenceKind::Project | EvidenceKind::Training | EvidenceKind::Certification
        ),
        ArtifactKind::Resume | ArtifactKind::LinkedinProfile => true,
    }
}

fn validate_resume_periods(
    evidence_ids: &[u64],
    evidence_by_id: &HashMap<u64, &SpecEvidence>,
    birth_date: Date,
    current_date: Date,
) -> Result<(), ArtifactError> {
    let minimum_date = add_years_clamped(birth_date, MINIMUM_RESUME_AGE_YEARS)?;
    let mut education_periods = Vec::new();
    let mut experience_periods = Vec::new();

    for evidence_id in evidence_ids {
        let evidence = evidence_by_id
            .get(evidence_id)
            .copied()
            .ok_or(ArtifactError::UnknownEvidenceId(*evidence_id))?;
        let Some((start, end)) = validate_evidence_period(evidence)? else {
            continue;
        };
        if start < minimum_date {
            return Err(ArtifactError::EvidenceBeforeMinimumAge(*evidence_id));
        }
        if end > current_date {
            return Err(ArtifactError::EvidenceEndsInFuture(*evidence_id));
        }
        if start == end {
            continue;
        }
        match evidence.kind {
            EvidenceKind::Education => education_periods.push((*evidence_id, start, end)),
            EvidenceKind::Experience => experience_periods.push((*evidence_id, start, end)),
            _ => {}
        }
    }

    validate_no_overlap(SpecDimension::Education, &education_periods)?;
    validate_no_overlap(SpecDimension::Experience, &experience_periods)
}

fn validate_evidence_period(
    evidence: &SpecEvidence,
) -> Result<Option<(Date, Date)>, ArtifactError> {
    match (
        evidence.period.start_date,
        evidence.period.end_exclusive_date,
        evidence.period.kind,
    ) {
        (None, None, None) => Ok(None),
        (Some(start), Some(end), Some(EvidencePeriodKind::Regular)) if start < end => {
            Ok(Some((start, end)))
        }
        (Some(start), Some(end), Some(EvidencePeriodKind::ZeroYearBridgeExperience))
            if evidence.kind == EvidenceKind::Experience && start == end =>
        {
            Ok(Some((start, end)))
        }
        _ => Err(ArtifactError::InvalidEvidencePeriod(evidence.evidence_id)),
    }
}

fn validate_no_overlap(
    dimension: SpecDimension,
    periods: &[(u64, Date, Date)],
) -> Result<(), ArtifactError> {
    let mut ordered = periods.to_vec();
    ordered.sort_by_key(|(evidence_id, start, end)| (*start, *end, *evidence_id));
    for left_index in 0..ordered.len() {
        for right_index in left_index + 1..ordered.len() {
            let (left_id, left_start, left_end) = ordered[left_index];
            let (right_id, right_start, right_end) = ordered[right_index];
            if left_start < right_end && right_start < left_end {
                return Err(ArtifactError::OverlappingResumeEvidence {
                    dimension,
                    first_evidence_id: left_id,
                    second_evidence_id: right_id,
                });
            }
        }
    }

    Ok(())
}

fn add_years_clamped(date: Date, years: u32) -> Result<Date, ArtifactError> {
    let target_year = date
        .year()
        .checked_add(i32::try_from(years).map_err(|_| ArtifactError::ArithmeticOverflow)?)
        .ok_or(ArtifactError::ArithmeticOverflow)?;
    let mut day = date.day();
    loop {
        if let Ok(candidate) = Date::from_calendar_date(target_year, date.month(), day) {
            return Ok(candidate);
        }
        day = day
            .checked_sub(1)
            .ok_or(ArtifactError::ArithmeticOverflow)?;
    }
}

fn validate_checklist(
    kind: ArtifactKind,
    checklist: &[ArtifactChecklistRule],
) -> Result<(), ArtifactError> {
    if checklist.is_empty() {
        return Err(ArtifactError::EmptyChecklist);
    }
    let mut identities = HashSet::with_capacity(checklist.len());
    let mut weight_total = 0_i128;
    for checklist_rule in checklist {
        if checklist_rule.weight_bp < 0 {
            return Err(ArtifactError::NegativeChecklistWeight);
        }
        if !identities.insert(&checklist_rule.rule) {
            return Err(ArtifactError::DuplicateChecklistRule);
        }
        if !checklist_rule_applies(kind, &checklist_rule.rule) {
            return Err(ArtifactError::InvalidChecklistRule);
        }
        weight_total = weight_total
            .checked_add(i128::from(checklist_rule.weight_bp))
            .ok_or(ArtifactError::ArithmeticOverflow)?;
    }
    if weight_total != i128::from(ARTIFACT_COMPLETENESS_SCALE_BP) {
        return Err(ArtifactError::InvalidChecklistWeightTotal);
    }

    Ok(())
}

fn checklist_rule_applies(kind: ArtifactKind, rule: &ChecklistRule) -> bool {
    match rule {
        ChecklistRule::HeadlinePresent | ChecklistRule::SummaryPresent => true,
        ChecklistRule::MinimumEvidenceCount { count } => {
            *count > 0 && usize::from(*count) <= maximum_evidence_count(kind)
        }
        ChecklistRule::ContainsDimension { dimension } => {
            artifact_allows_evidence(kind, dimension_to_evidence_kind(*dimension))
        }
        ChecklistRule::ContainsEvidenceKind { evidence_kind } => {
            artifact_allows_evidence(kind, *evidence_kind)
        }
        ChecklistRule::ProjectPresent => kind == ArtifactKind::Portfolio,
        ChecklistRule::OpenToWork => kind == ArtifactKind::LinkedinProfile,
        ChecklistRule::IndustryCountAtLeast { count } => {
            kind == ArtifactKind::LinkedinProfile
                && *count > 0
                && usize::from(*count) <= MAX_LINKEDIN_INDUSTRIES
        }
    }
}

const fn dimension_to_evidence_kind(dimension: SpecDimension) -> EvidenceKind {
    match dimension {
        SpecDimension::Education => EvidenceKind::Education,
        SpecDimension::Certification => EvidenceKind::Certification,
        SpecDimension::Language => EvidenceKind::Language,
        SpecDimension::Training => EvidenceKind::Training,
        SpecDimension::Experience => EvidenceKind::Experience,
        SpecDimension::Project => EvidenceKind::Project,
    }
}

fn validate_canonical_artifact(
    artifact: &CanonicalArtifact,
    evidence_by_id: &HashMap<u64, &SpecEvidence>,
) -> Result<(), ArtifactError> {
    let mut evidence_ids = HashSet::with_capacity(artifact.evidence_ids.len());
    for evidence_id in &artifact.evidence_ids {
        if !evidence_ids.insert(*evidence_id) {
            return Err(ArtifactError::DuplicateEvidenceId(*evidence_id));
        }
        let evidence = evidence_by_id
            .get(evidence_id)
            .copied()
            .ok_or(ArtifactError::UnknownEvidenceId(*evidence_id))?;
        if !artifact_allows_evidence(artifact.kind, evidence.kind) {
            return Err(ArtifactError::EvidenceKindNotAllowed {
                kind: artifact.kind,
                evidence_id: *evidence_id,
            });
        }
    }
    let maximum = maximum_evidence_count(artifact.kind);
    if artifact.evidence_ids.len() > maximum {
        return Err(ArtifactError::TooManyEvidence {
            kind: artifact.kind,
            maximum,
        });
    }

    match (artifact.kind, artifact.linkedin.as_ref()) {
        (ArtifactKind::LinkedinProfile, Some(fields)) => {
            let industries = fields.industries.iter().copied().collect::<HashSet<_>>();
            if industries.len() != fields.industries.len()
                || fields.industries.len() > MAX_LINKEDIN_INDUSTRIES
            {
                return Err(ArtifactError::InvalidLinkedinFields);
            }
        }
        (ArtifactKind::LinkedinProfile, None) | (_, Some(_)) => {
            return Err(ArtifactError::InvalidLinkedinFields);
        }
        (_, None) => {}
    }

    Ok(())
}

fn checklist_rule_matches(
    rule: &ChecklistRule,
    artifact: &CanonicalArtifact,
    evidence: &[&SpecEvidence],
) -> bool {
    match rule {
        ChecklistRule::HeadlinePresent => !artifact.headline.trim().is_empty(),
        ChecklistRule::SummaryPresent => !artifact.summary.trim().is_empty(),
        ChecklistRule::MinimumEvidenceCount { count } => {
            artifact.evidence_ids.len() >= usize::from(*count)
        }
        ChecklistRule::ContainsDimension { dimension } => evidence
            .iter()
            .any(|item| item.kind.dimension() == *dimension),
        ChecklistRule::ContainsEvidenceKind { evidence_kind } => {
            evidence.iter().any(|item| item.kind == *evidence_kind)
        }
        ChecklistRule::ProjectPresent => evidence
            .iter()
            .any(|item| item.kind == EvidenceKind::Project),
        ChecklistRule::OpenToWork => artifact
            .linkedin
            .as_ref()
            .is_some_and(|fields| fields.open_to_work),
        ChecklistRule::IndustryCountAtLeast { count } => artifact
            .linkedin
            .as_ref()
            .is_some_and(|fields| fields.industries.len() >= usize::from(*count)),
    }
}

#[cfg(test)]
mod tests {
    use time::Month;

    use super::*;
    use crate::career::types::{ArtifactDraft, EvidencePeriodFields, Industry};

    fn given_date(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("유효한 테스트 날짜여야 한다")
    }

    fn given_evidence(evidence_id: u64, kind: EvidenceKind) -> SpecEvidence {
        SpecEvidence {
            evidence_id,
            evidence_key: format!("evidence-{evidence_id}"),
            catalog_entry_key: format!("catalog-{evidence_id}"),
            kind,
            acquired_game_day: 0,
            expires_on_game_day: None,
            period: EvidencePeriodFields::none(),
        }
    }

    fn given_period_evidence(
        evidence_id: u64,
        kind: EvidenceKind,
        start: Date,
        end: Date,
    ) -> SpecEvidence {
        SpecEvidence {
            period: EvidencePeriodFields::regular(start, end),
            ..given_evidence(evidence_id, kind)
        }
    }

    fn given_draft(kind: ArtifactKind, evidence_ids: Vec<u64>) -> ArtifactDraft {
        ArtifactDraft {
            kind,
            headline: "  경력 요약  ".to_owned(),
            summary: "\n  문제 해결 경험  \n".to_owned(),
            evidence_ids,
            linkedin: (kind == ArtifactKind::LinkedinProfile).then_some(LinkedinFields {
                open_to_work: false,
                industries: Vec::new(),
            }),
        }
    }

    fn when_canonicalize<'a>(
        draft: &'a ArtifactDraft,
        evidence: &'a [SpecEvidence],
    ) -> Result<CanonicalArtifact, ArtifactError> {
        create_artifact_rules().canonicalize(ArtifactValidationInput {
            draft,
            current_date: given_date(2026, Month::January, 1),
            birth_date: given_date(1990, Month::January, 1),
            owned_evidence: evidence,
        })
    }

    mod context_자유문구를_정규화하는_경우 {
        use super::*;

        #[test]
        fn given_unicode_공백과_중복_evidence_when_저장하면_then_trim하고_첫_순서를_보존한다() {
            let evidence = vec![given_evidence(1, EvidenceKind::Project)];
            let draft = given_draft(ArtifactKind::Portfolio, vec![1, 1]);

            let result = when_canonicalize(&draft, &evidence).expect("산출물을 정규화해야 한다");

            assert_eq!(result.headline, "경력 요약");
            assert_eq!(result.summary, "문제 해결 경험");
            assert_eq!(result.evidence_ids, vec![1]);
        }

        #[test]
        fn given_120개_unicode_scalar_when_저장하면_then_허용한다() {
            let mut draft = given_draft(ArtifactKind::Portfolio, Vec::new());
            draft.headline = "🧭".repeat(MAX_HEADLINE_SCALARS);

            let result = when_canonicalize(&draft, &[]).expect("120 scalar 제목을 허용해야 한다");

            assert_eq!(result.headline.chars().count(), MAX_HEADLINE_SCALARS);
        }

        #[test]
        fn given_121개_unicode_scalar_when_저장하면_then_길이오류로_거절한다() {
            let mut draft = given_draft(ArtifactKind::Portfolio, Vec::new());
            draft.headline = "🧭".repeat(MAX_HEADLINE_SCALARS + 1);

            let result = when_canonicalize(&draft, &[]);

            assert_eq!(result, Err(ArtifactError::HeadlineTooLong));
        }

        #[test]
        fn given_summary의_lf와_tab_when_저장하면_then_예외로_허용한다() {
            let mut draft = given_draft(ArtifactKind::Portfolio, Vec::new());
            draft.summary = "첫 줄\n\t둘째 줄".to_owned();

            let result = when_canonicalize(&draft, &[]).expect("LF와 tab을 허용해야 한다");

            assert_eq!(result.summary, "첫 줄\n\t둘째 줄");
        }

        #[test]
        fn given_summary의_c0_control_when_저장하면_then_거절한다() {
            let mut draft = given_draft(ArtifactKind::Portfolio, Vec::new());
            draft.summary = "본문\u{0007}".to_owned();

            let result = when_canonicalize(&draft, &[]);

            assert_eq!(result, Err(ArtifactError::ForbiddenSummaryControl));
        }
    }

    mod context_산출물별_allowlist를_검사하는_경우 {
        use super::*;

        #[test]
        fn given_portfolio에_경력_evidence_when_저장하면_then_허용목록오류로_거절한다() {
            let evidence = vec![given_evidence(1, EvidenceKind::Experience)];
            let draft = given_draft(ArtifactKind::Portfolio, vec![1]);

            let result = when_canonicalize(&draft, &evidence);

            assert_eq!(
                result,
                Err(ArtifactError::EvidenceKindNotAllowed {
                    kind: ArtifactKind::Portfolio,
                    evidence_id: 1,
                })
            );
        }

        #[test]
        fn given_linkedin_업종_중복_when_저장하면_then_첫_순서로_중복을_제거한다() {
            let mut draft = given_draft(ArtifactKind::LinkedinProfile, Vec::new());
            draft.linkedin = Some(LinkedinFields {
                open_to_work: true,
                industries: vec![
                    Industry::ItSoftware,
                    Industry::FinanceInsurance,
                    Industry::ItSoftware,
                ],
            });

            let result = when_canonicalize(&draft, &[]).expect("LinkedIn 필드를 정규화해야 한다");

            assert_eq!(
                result
                    .linkedin
                    .expect("LinkedIn 필드가 있어야 한다")
                    .industries,
                vec![Industry::ItSoftware, Industry::FinanceInsurance]
            );
        }

        #[test]
        fn given_resume에_linkedin_필드_when_저장하면_then_tagged_union_오류로_거절한다() {
            let mut draft = given_draft(ArtifactKind::Resume, Vec::new());
            draft.linkedin = Some(LinkedinFields {
                open_to_work: true,
                industries: Vec::new(),
            });

            let result = when_canonicalize(&draft, &[]);

            assert_eq!(result, Err(ArtifactError::InvalidLinkedinFields));
        }
    }

    mod context_resume_연대기를_검증하는_경우 {
        use super::*;

        #[test]
        fn given_15번째_생일에_시작한_기간_when_저장하면_then_경계를_허용한다() {
            let evidence = vec![given_period_evidence(
                1,
                EvidenceKind::Education,
                given_date(2005, Month::January, 1),
                given_date(2008, Month::January, 1),
            )];
            let draft = given_draft(ArtifactKind::Resume, vec![1]);

            let result = when_canonicalize(&draft, &evidence).expect("15세 경계를 허용해야 한다");

            assert_eq!(result.evidence_ids, vec![1]);
        }

        #[test]
        fn given_15번째_생일보다_하루_이른_기간_when_저장하면_then_거절한다() {
            let evidence = vec![given_period_evidence(
                1,
                EvidenceKind::Education,
                given_date(2004, Month::December, 31),
                given_date(2008, Month::January, 1),
            )];
            let draft = given_draft(ArtifactKind::Resume, vec![1]);

            let result = when_canonicalize(&draft, &evidence);

            assert_eq!(result, Err(ArtifactError::EvidenceBeforeMinimumAge(1)));
        }

        #[test]
        fn given_같은차원에서_겹친_기간_when_저장하면_then_overlap으로_거절한다() {
            let evidence = vec![
                given_period_evidence(
                    1,
                    EvidenceKind::Experience,
                    given_date(2020, Month::January, 1),
                    given_date(2023, Month::January, 1),
                ),
                given_period_evidence(
                    2,
                    EvidenceKind::Experience,
                    given_date(2022, Month::January, 1),
                    given_date(2024, Month::January, 1),
                ),
            ];
            let draft = given_draft(ArtifactKind::Resume, vec![1, 2]);

            let result = when_canonicalize(&draft, &evidence);

            assert!(matches!(
                result,
                Err(ArtifactError::OverlappingResumeEvidence {
                    dimension: SpecDimension::Experience,
                    ..
                })
            ));
        }

        #[test]
        fn given_학력과_경력이_겹친_기간_when_저장하면_then_교차차원_overlap을_허용한다() {
            let evidence = vec![
                given_period_evidence(
                    1,
                    EvidenceKind::Education,
                    given_date(2020, Month::January, 1),
                    given_date(2023, Month::January, 1),
                ),
                given_period_evidence(
                    2,
                    EvidenceKind::Experience,
                    given_date(2022, Month::January, 1),
                    given_date(2024, Month::January, 1),
                ),
            ];
            let draft = given_draft(ArtifactKind::Resume, vec![1, 2]);

            let result =
                when_canonicalize(&draft, &evidence).expect("교차차원 겹침을 허용해야 한다");

            assert_eq!(result.evidence_ids, vec![1, 2]);
        }

        #[test]
        fn given_기간의_한쪽만_있는_evidence_when_저장하면_then_pair_오류로_거절한다() {
            let mut evidence = given_evidence(1, EvidenceKind::Experience);
            evidence.period = EvidencePeriodFields {
                start_date: Some(given_date(2020, Month::January, 1)),
                end_exclusive_date: None,
                kind: Some(EvidencePeriodKind::Regular),
            };
            let draft = given_draft(ArtifactKind::Resume, vec![1]);

            let result = when_canonicalize(&draft, &[evidence]);

            assert_eq!(result, Err(ArtifactError::InvalidEvidencePeriod(1)));
        }

        #[test]
        fn given_0년_bridge_경력의_빈기간_when_저장하면_then_유일한_빈기간으로_허용한다() {
            let mut evidence = given_evidence(1, EvidenceKind::Experience);
            let date = given_date(2026, Month::January, 1);
            evidence.period = EvidencePeriodFields::zero_year_bridge(date);
            let draft = given_draft(ArtifactKind::Resume, vec![1]);

            let result =
                when_canonicalize(&draft, &[evidence]).expect("0년 브리지 빈기간을 허용해야 한다");

            assert_eq!(result.evidence_ids, vec![1]);
        }
    }

    mod context_typed_checklist로_완성도를_계산하는_경우 {
        use super::*;

        #[test]
        fn given_충족한_rule과_미충족한_rule_when_계산하면_then_충족한_weight만_합산한다() {
            let evidence = vec![given_evidence(1, EvidenceKind::Project)];
            let draft = given_draft(ArtifactKind::Portfolio, vec![1]);
            let artifact =
                when_canonicalize(&draft, &evidence).expect("포트폴리오를 정규화해야 한다");
            let checklist = vec![
                ArtifactChecklistRule {
                    rule: ChecklistRule::HeadlinePresent,
                    weight_bp: 2_000,
                },
                ArtifactChecklistRule {
                    rule: ChecklistRule::SummaryPresent,
                    weight_bp: 2_000,
                },
                ArtifactChecklistRule {
                    rule: ChecklistRule::MinimumEvidenceCount { count: 2 },
                    weight_bp: 3_000,
                },
                ArtifactChecklistRule {
                    rule: ChecklistRule::ProjectPresent,
                    weight_bp: 3_000,
                },
            ];

            let result = create_artifact_rules()
                .calculate_completeness(ArtifactCompletenessInput {
                    artifact: &artifact,
                    owned_evidence: &evidence,
                    checklist: &checklist,
                })
                .expect("완성도를 계산해야 한다");

            assert_eq!(result, 7_000);
        }

        #[test]
        fn given_rule_weight합이_만점이_아닐때_when_검증하면_then_게시를_거절한다() {
            let checklist = vec![ArtifactChecklistRule {
                rule: ChecklistRule::HeadlinePresent,
                weight_bp: 9_999,
            }];

            let result =
                create_artifact_rules().validate_checklist(ArtifactKind::Resume, &checklist);

            assert_eq!(result, Err(ArtifactError::InvalidChecklistWeightTotal));
        }

        #[test]
        fn given_resume에_open_to_work_rule_when_검증하면_then_kind_부적합으로_거절한다() {
            let checklist = vec![ArtifactChecklistRule {
                rule: ChecklistRule::OpenToWork,
                weight_bp: ARTIFACT_COMPLETENESS_SCALE_BP,
            }];

            let result =
                create_artifact_rules().validate_checklist(ArtifactKind::Resume, &checklist);

            assert_eq!(result, Err(ArtifactError::InvalidChecklistRule));
        }

        #[test]
        fn given_동일한_rule_identity가_두개일때_when_검증하면_then_중복으로_거절한다() {
            let checklist = vec![
                ArtifactChecklistRule {
                    rule: ChecklistRule::HeadlinePresent,
                    weight_bp: 5_000,
                },
                ArtifactChecklistRule {
                    rule: ChecklistRule::HeadlinePresent,
                    weight_bp: 5_000,
                },
            ];

            let result =
                create_artifact_rules().validate_checklist(ArtifactKind::Portfolio, &checklist);

            assert_eq!(result, Err(ArtifactError::DuplicateChecklistRule));
        }
    }
}
