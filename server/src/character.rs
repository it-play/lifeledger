//! Character creation domain (§3).
//!
//! Starting conditions can contradict each other (§3.5), so combination validation is
//! this module's core job. It stays pure to be testable without a store or HTTP.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// School entry age; the baseline for checking that study, service and career fit in a life.
const SCHOOL_ENTRY_AGE: u32 = 6;
const MIN_AGE: u32 = 19;
const MAX_AGE: u32 = 50;
const MAX_NAME_CHARACTERS: usize = 20;
/// Age at which unserved status is treated as exempt in practice (§3.5).
const DE_FACTO_EXEMPT_AGE: u32 = 40;
/// Safety ceiling until the point budget lands (M5).
const MAX_STARTING_CASH_KRW: i64 = 10_000_000_000;
const MAX_STUDENT_LOAN_KRW: i64 = 50_000_000;
const MAX_CREDIT_LOAN_KRW: i64 = 200_000_000;
const MAX_CAREER_YEARS: u32 = 30;
const MAX_CERTIFICATIONS: u32 = 50;
const MAX_DEPENDENTS: u32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum Gender {
    Male,
    Female,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MilitaryStatus {
    /// Not served. Some postings require served-or-exempt, narrowing the job pool.
    NotServed,
    Serving,
    Completed,
    Exempted,
    /// Alternative service, which requires a certification or a master's degree.
    Alternative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum Education {
    HighSchool,
    Associate,
    Bachelor,
    Master,
    Doctorate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum Region {
    CapitalArea,
    Metropolitan,
    SmallCity,
    Rural,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum FamilyBackground {
    /// Supported: receives money from parents.
    Supportive,
    Independent,
    /// Dependent: pays support to family.
    Dependent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum Health {
    Good,
    Normal,
    Poor,
}

impl Education {
    /// Cumulative years of schooling this level takes, counted from school entry.
    const fn school_years(self) -> u32 {
        match self {
            Self::HighSchool => 12,
            Self::Associate => 14,
            Self::Bachelor => 16,
            Self::Master => 18,
            Self::Doctorate => 22,
        }
    }
}

impl MilitaryStatus {
    /// Years of service that do not overlap study or career.
    const fn service_years(self) -> u32 {
        match self {
            Self::Completed => 2,
            Self::Alternative => 3,
            Self::NotServed | Self::Serving | Self::Exempted => 0,
        }
    }

    /// Youngest age at which this service status is credible.
    const fn min_age(self) -> u32 {
        match self {
            Self::Completed => MIN_AGE + 2,
            Self::Alternative => MIN_AGE + 3,
            Self::NotServed | Self::Serving | Self::Exempted => MIN_AGE,
        }
    }
}

/// Starting conditions as sent by a client, before validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CharacterDraft {
    pub name: String,
    pub age: u32,
    pub gender: Gender,
    pub military: MilitaryStatus,
    pub region: Region,
    pub background: FamilyBackground,
    pub education: Education,
    pub career_years: u32,
    pub certifications: u32,
    pub starting_cash_krw: i64,
    pub student_loan_krw: i64,
    pub credit_loan_krw: i64,
    pub health: Health,
    pub dependents: u32,
}

/// A validated character. Values are trustworthy from here on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Character {
    pub name: String,
    pub age: u32,
    pub gender: Gender,
    pub military: MilitaryStatus,
    pub region: Region,
    pub background: FamilyBackground,
    pub education: Education,
    pub career_years: u32,
    pub certifications: u32,
    pub cash_krw: i64,
    pub debt_krw: i64,
    pub health: Health,
    pub dependents: u32,
}

/// Says which field combination is contradictory and why (§3.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidationError {
    /// Matches the client form field name.
    pub field: &'static str,
    pub message: String,
}

impl ValidationError {
    fn new(field: &'static str, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
        }
    }
}

/// Validates starting conditions into a character, reporting every error at once so the
/// form is not corrected one round trip at a time.
pub fn create_character(draft: CharacterDraft) -> Result<Character, Vec<ValidationError>> {
    let mut errors = Vec::new();

    let name = draft.name.trim();
    if name.is_empty() {
        errors.push(ValidationError::new("name", "이름을 입력하세요"));
    } else if name.chars().count() > MAX_NAME_CHARACTERS {
        errors.push(ValidationError::new(
            "name",
            format!("이름은 {MAX_NAME_CHARACTERS}자를 넘을 수 없습니다"),
        ));
    }
    if !(MIN_AGE..=MAX_AGE).contains(&draft.age) {
        errors.push(ValidationError::new(
            "age",
            format!("나이는 {MIN_AGE}세 이상 {MAX_AGE}세 이하여야 합니다"),
        ));
    }
    if draft.career_years > MAX_CAREER_YEARS {
        errors.push(ValidationError::new(
            "careerYears",
            format!("경력은 {MAX_CAREER_YEARS}년을 넘을 수 없습니다"),
        ));
    }
    if draft.certifications > MAX_CERTIFICATIONS {
        errors.push(ValidationError::new(
            "certifications",
            format!("자격증은 {MAX_CERTIFICATIONS}개를 넘을 수 없습니다"),
        ));
    }
    if draft.dependents > MAX_DEPENDENTS {
        errors.push(ValidationError::new(
            "dependents",
            format!("부양가족은 {MAX_DEPENDENTS}명을 넘을 수 없습니다"),
        ));
    }

    errors.extend(validate_money(&draft));
    let debt_krw = match draft.student_loan_krw.checked_add(draft.credit_loan_krw) {
        Some(total) => total,
        None => {
            errors.push(ValidationError::new(
                "creditLoanKrw",
                "합산 부채가 허용 범위를 넘었습니다",
            ));
            0
        }
    };
    // Age-dependent rules only run on a valid age, so one cause yields one error
    if (MIN_AGE..=MAX_AGE).contains(&draft.age) {
        errors.extend(validate_military(&draft));
        errors.extend(validate_timeline(&draft));
    }
    errors.extend(validate_education(&draft));

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(Character {
        name: draft.name.trim().to_owned(),
        age: draft.age,
        gender: draft.gender,
        military: normalize_military(draft.military, draft.age),
        region: draft.region,
        background: draft.background,
        education: draft.education,
        career_years: draft.career_years,
        certifications: draft.certifications,
        cash_krw: draft.starting_cash_krw,
        debt_krw,
        health: draft.health,
        dependents: draft.dependents,
    })
}

/// Unserved past a certain age counts as exempt in practice (§3.5).
const fn normalize_military(status: MilitaryStatus, age: u32) -> MilitaryStatus {
    match status {
        MilitaryStatus::NotServed if age >= DE_FACTO_EXEMPT_AGE => MilitaryStatus::Exempted,
        other => other,
    }
}

fn validate_money(draft: &CharacterDraft) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    if draft.starting_cash_krw < 0 {
        errors.push(ValidationError::new(
            "startingCashKrw",
            "시작 자금은 음수가 될 수 없습니다",
        ));
    }
    if draft.starting_cash_krw > MAX_STARTING_CASH_KRW {
        errors.push(ValidationError::new(
            "startingCashKrw",
            "시작 자금이 허용 범위를 넘었습니다",
        ));
    }
    if draft.student_loan_krw < 0 {
        errors.push(ValidationError::new(
            "studentLoanKrw",
            "학자금 부채는 음수가 될 수 없습니다",
        ));
    } else if draft.student_loan_krw > MAX_STUDENT_LOAN_KRW {
        errors.push(ValidationError::new(
            "studentLoanKrw",
            "학자금 부채가 시작 대출 한도를 넘었습니다",
        ));
    }
    if draft.credit_loan_krw < 0 {
        errors.push(ValidationError::new(
            "creditLoanKrw",
            "신용 부채는 음수가 될 수 없습니다",
        ));
    } else if draft.credit_loan_krw > MAX_CREDIT_LOAN_KRW {
        errors.push(ValidationError::new(
            "creditLoanKrw",
            "신용 부채가 시작 대출 한도를 넘었습니다",
        ));
    }
    errors
}

fn validate_military(draft: &CharacterDraft) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    if draft.age < draft.military.min_age() {
        errors.push(ValidationError::new(
            "military",
            format!("{}세에는 이 병역 상태가 될 수 없습니다", draft.age),
        ));
    }

    // Alternative service requires a certification or a master's degree (§8.1)
    if draft.military == MilitaryStatus::Alternative
        && draft.certifications == 0
        && draft.education < Education::Master
    {
        errors.push(ValidationError::new(
            "military",
            "특례복무는 자격증 보유 또는 석사 이상 학력이 필요합니다",
        ));
    }

    errors
}

fn validate_education(draft: &CharacterDraft) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    if draft.student_loan_krw > 0 && draft.education == Education::HighSchool {
        errors.push(ValidationError::new(
            "studentLoanKrw",
            "고졸 학력에는 학자금 부채가 있을 수 없습니다",
        ));
    }
    errors
}

/// Study plus service plus career cannot exceed the years actually lived (§3.5).
fn validate_timeline(draft: &CharacterDraft) -> Vec<ValidationError> {
    let available = draft.age.saturating_sub(SCHOOL_ENTRY_AGE);
    let required =
        draft.education.school_years() + draft.military.service_years() + draft.career_years;

    if required > available {
        return vec![ValidationError::new(
            "careerYears",
            format!(
                "학업 {}년 + 복무 {}년 + 경력 {}년 = {}년은 {}세가 살아온 {}년을 넘습니다",
                draft.education.school_years(),
                draft.military.service_years(),
                draft.career_years,
                required,
                draft.age,
                available
            ),
        )];
    }
    Vec::new()
}

/// Starting presets (§3.3). Content data, so this moves to a file later.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub summary: &'static str,
    pub age: u32,
    pub military: MilitaryStatus,
    pub education: Education,
    pub region: Region,
    pub background: FamilyBackground,
    pub career_years: u32,
    pub certifications: u32,
    pub starting_cash_krw: i64,
    pub student_loan_krw: i64,
    pub credit_loan_krw: i64,
    pub health: Health,
    pub dependents: u32,
}

pub fn presets() -> &'static [Preset] {
    &[
        Preset {
            id: "rookie",
            label: "사회초년생",
            summary: "기본값. 균형 잡힌 출발",
            age: 25,
            military: MilitaryStatus::Completed,
            education: Education::Bachelor,
            region: Region::CapitalArea,
            background: FamilyBackground::Independent,
            career_years: 1,
            certifications: 1,
            starting_cash_krw: 10_000_000,
            student_loan_krw: 20_000_000,
            credit_loan_krw: 0,
            health: Health::Normal,
            dependents: 0,
        },
        Preset {
            id: "early-start",
            label: "이른 출발",
            summary: "시간은 많고 자본은 없음",
            age: 19,
            military: MilitaryStatus::NotServed,
            education: Education::HighSchool,
            region: Region::SmallCity,
            background: FamilyBackground::Dependent,
            career_years: 0,
            certifications: 0,
            starting_cash_krw: 2_000_000,
            student_loan_krw: 0,
            credit_loan_krw: 0,
            health: Health::Good,
            dependents: 1,
        },
        Preset {
            id: "late-start",
            label: "늦은 출발",
            summary: "자본은 있고 시간이 짧음",
            age: 38,
            military: MilitaryStatus::Completed,
            education: Education::Bachelor,
            region: Region::Metropolitan,
            background: FamilyBackground::Independent,
            career_years: 10,
            certifications: 2,
            starting_cash_krw: 50_000_000,
            student_loan_krw: 0,
            credit_loan_krw: 30_000_000,
            health: Health::Normal,
            dependents: 2,
        },
        Preset {
            id: "supported",
            label: "지원 받는 출발",
            summary: "쉬운 난이도. 세제 한도 최적화가 주 과제",
            age: 25,
            military: MilitaryStatus::Exempted,
            education: Education::Master,
            region: Region::CapitalArea,
            background: FamilyBackground::Supportive,
            career_years: 0,
            certifications: 1,
            starting_cash_krw: 300_000_000,
            student_loan_krw: 0,
            credit_loan_krw: 0,
            health: Health::Good,
            dependents: 0,
        },
        Preset {
            id: "restart",
            label: "재기",
            summary: "신용 제약 하에서의 복구 플레이",
            age: 45,
            military: MilitaryStatus::Completed,
            education: Education::HighSchool,
            region: Region::Rural,
            background: FamilyBackground::Independent,
            career_years: 20,
            certifications: 0,
            starting_cash_krw: 0,
            student_loan_krw: 0,
            credit_loan_krw: 0,
            health: Health::Poor,
            dependents: 0,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A draft that passes validation, based on the "사회초년생" preset.
    fn given_valid_draft() -> CharacterDraft {
        CharacterDraft {
            name: "테스터".to_owned(),
            age: 25,
            gender: Gender::Male,
            military: MilitaryStatus::Completed,
            region: Region::CapitalArea,
            background: FamilyBackground::Independent,
            education: Education::Bachelor,
            career_years: 1,
            certifications: 1,
            starting_cash_krw: 10_000_000,
            student_loan_krw: 20_000_000,
            credit_loan_krw: 0,
            health: Health::Normal,
            dependents: 0,
        }
    }

    fn fields_of(errors: &[ValidationError]) -> Vec<&str> {
        errors.iter().map(|error| error.field).collect()
    }

    mod context_all_conditions_consistent {
        use super::*;

        #[test]
        fn given_valid_draft_when_creating_then_character_is_returned() {
            let draft = given_valid_draft();

            let result = create_character(draft);

            let character = result.expect("유효한 초안은 통과해야 한다");
            assert_eq!(character.cash_krw, 10_000_000);
            assert_eq!(character.debt_krw, 20_000_000);
        }

        #[test]
        fn given_name_with_spaces_when_created_then_it_is_trimmed() {
            let mut draft = given_valid_draft();
            draft.name = "  김앤디  ".to_owned();

            let character = create_character(draft).expect("통과해야 한다");

            assert_eq!(character.name, "김앤디");
        }
    }

    mod context_age_conflicts_with_military_status {
        use super::*;

        #[test]
        fn given_age_19_when_military_is_completed_then_rejected() {
            let mut draft = given_valid_draft();
            draft.age = 19;
            draft.education = Education::HighSchool;
            draft.career_years = 0;
            draft.student_loan_krw = 0;

            let errors = create_character(draft).expect_err("19세는 복무를 마칠 수 없다");

            assert!(fields_of(&errors).contains(&"military"));
        }

        #[test]
        fn given_age_40_when_military_is_not_served_then_normalized_to_exempted() {
            let mut draft = given_valid_draft();
            draft.age = 40;
            draft.military = MilitaryStatus::NotServed;

            let character = create_character(draft).expect("통과해야 한다");

            assert_eq!(character.military, MilitaryStatus::Exempted);
        }

        #[test]
        fn given_age_below_minimum_when_creating_then_only_age_is_reported() {
            let mut draft = given_valid_draft();
            draft.age = 15;

            let errors = create_character(draft).expect_err("19세 미만은 거부된다");

            // An invalid age skips age-dependent rules, so one cause yields one error
            assert_eq!(fields_of(&errors), vec!["age"]);
        }
    }

    mod context_timeline_does_not_fit_in_a_lifetime {
        use super::*;

        #[test]
        fn given_doctorate_and_long_career_when_age_is_young_then_rejected() {
            let mut draft = given_valid_draft();
            draft.age = 25;
            draft.education = Education::Doctorate;
            draft.career_years = 5;

            let errors = create_character(draft).expect_err("25세에 박사 + 경력 5년은 불가능하다");

            assert!(fields_of(&errors).contains(&"careerYears"));
        }

        #[test]
        fn given_alternative_service_when_timeline_is_tight_then_service_years_are_counted() {
            let mut draft = given_valid_draft();
            draft.age = 24;
            draft.education = Education::Bachelor; // 16년
            draft.military = MilitaryStatus::Alternative; // 3년
            draft.career_years = 0;

            // 16 + 3 = 19 > 24 - 6 = 18
            let errors =
                create_character(draft).expect_err("복무 연수가 타임라인에 포함되어야 한다");

            assert!(fields_of(&errors).contains(&"careerYears"));
        }
    }

    mod context_education_conflicts_with_debt_or_service {
        use super::*;

        #[test]
        fn given_high_school_when_student_loan_exists_then_rejected() {
            let mut draft = given_valid_draft();
            draft.education = Education::HighSchool;
            draft.career_years = 0;

            let errors = create_character(draft).expect_err("고졸에 학자금 부채는 모순이다");

            assert!(fields_of(&errors).contains(&"studentLoanKrw"));
        }

        #[test]
        fn given_alternative_service_when_no_certification_and_bachelor_then_rejected() {
            let mut draft = given_valid_draft();
            draft.age = 30;
            draft.military = MilitaryStatus::Alternative;
            draft.certifications = 0;
            draft.education = Education::Bachelor;

            let errors = create_character(draft).expect_err("특례복무 요건을 못 채운다");

            assert!(fields_of(&errors).contains(&"military"));
        }

        #[test]
        fn given_alternative_service_when_master_without_certification_then_accepted() {
            let mut draft = given_valid_draft();
            draft.age = 30;
            draft.military = MilitaryStatus::Alternative;
            draft.certifications = 0;
            draft.education = Education::Master;
            draft.career_years = 0;

            let result = create_character(draft);

            assert!(result.is_ok(), "석사 이상은 특례복무 요건을 만족한다");
        }
    }

    mod context_presets_are_offered_as_starting_points {
        use super::*;

        #[test]
        fn given_every_preset_when_creating_a_character_then_it_passes_validation() {
            for preset in presets() {
                let draft = CharacterDraft {
                    name: preset.label.to_owned(),
                    age: preset.age,
                    gender: Gender::Male,
                    military: preset.military,
                    region: preset.region,
                    background: preset.background,
                    education: preset.education,
                    career_years: preset.career_years,
                    certifications: preset.certifications,
                    starting_cash_krw: preset.starting_cash_krw,
                    student_loan_krw: preset.student_loan_krw,
                    credit_loan_krw: preset.credit_loan_krw,
                    health: preset.health,
                    dependents: preset.dependents,
                };

                let result = create_character(draft);

                assert!(
                    result.is_ok(),
                    "프리셋 {} 이 자체 검증을 통과하지 못했다: {:?}",
                    preset.id,
                    result.err()
                );
            }
        }
    }

    mod context_money_values_are_out_of_range {
        use super::*;

        #[test]
        fn given_negative_cash_when_creating_then_rejected() {
            let mut draft = given_valid_draft();
            draft.starting_cash_krw = -1;

            let errors = create_character(draft).expect_err("음수 자금은 거부된다");

            assert!(fields_of(&errors).contains(&"startingCashKrw"));
        }

        #[test]
        fn given_multiple_broken_rules_when_creating_then_all_errors_are_reported() {
            let mut draft = given_valid_draft();
            draft.name = "   ".to_owned();
            draft.starting_cash_krw = -1;
            draft.dependents = 99;

            let errors = create_character(draft).expect_err("여러 오류가 함께 보고된다");

            let fields = fields_of(&errors);
            assert!(fields.contains(&"name"));
            assert!(fields.contains(&"startingCashKrw"));
            assert!(fields.contains(&"dependents"));
        }

        #[test]
        fn given_debts_whose_sum_overflows_when_creating_then_rejected() {
            let mut draft = given_valid_draft();
            draft.student_loan_krw = i64::MAX;
            draft.credit_loan_krw = 1;

            let errors = create_character(draft).expect_err("합산할 수 없는 부채는 거부된다");

            assert!(fields_of(&errors).contains(&"creditLoanKrw"));
        }

        #[test]
        fn given_학자금_대출이_상품_한도를_넘음_when_캐릭터를_생성함_then_거부됨() {
            let mut draft = given_valid_draft();
            draft.student_loan_krw = MAX_STUDENT_LOAN_KRW + 1;

            let errors = create_character(draft).expect_err("학자금 대출 한도를 지켜야 한다");

            assert!(fields_of(&errors).contains(&"studentLoanKrw"));
        }

        #[test]
        fn given_신용_대출이_상품_한도를_넘음_when_캐릭터를_생성함_then_거부됨() {
            let mut draft = given_valid_draft();
            draft.credit_loan_krw = MAX_CREDIT_LOAN_KRW + 1;

            let errors = create_character(draft).expect_err("신용 대출 한도를 지켜야 한다");

            assert!(fields_of(&errors).contains(&"creditLoanKrw"));
        }
    }

    mod context_name_is_out_of_range {
        use super::*;

        #[test]
        fn given_name_longer_than_database_limit_when_creating_then_rejected() {
            let mut draft = given_valid_draft();
            draft.name = "가".repeat(21);

            let errors = create_character(draft).expect_err("DB 한도를 넘는 이름은 거부된다");

            assert!(fields_of(&errors).contains(&"name"));
        }
    }

    mod context_certification_count_is_out_of_range {
        use super::*;

        #[test]
        fn given_자격증이_50개를_넘을때_when_캐릭터를_만들면_then_거절한다() {
            let mut draft = given_valid_draft();
            draft.certifications = 51;

            let errors = create_character(draft).expect_err("bridge 상한을 넘으면 거부해야 한다");

            assert!(fields_of(&errors).contains(&"certifications"));
        }
    }
}
