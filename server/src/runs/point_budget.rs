use std::collections::{BTreeMap, BTreeSet};

use serde::de::DeserializeOwned;

use crate::character::{
    CharacterDraft, Education, FamilyBackground, Gender, Health, MilitaryStatus, Region,
};
use crate::finance::ResourceId;

use super::types::{
    PointBudgetCatalog, PointBudgetEvaluation, PointBudgetFailure, PointBudgetFailureCode,
    PointBudgetOption, PointBudgetPreparation, PointBudgetRules, PointCondition, PointCostKind,
    PointEffect, PointFactComparison, PointFactValue, PointLedgerLine, PointSelection,
};

pub(super) struct DefaultPointBudgetRules;

impl PointBudgetRules for DefaultPointBudgetRules {
    fn evaluate(
        &self,
        catalog: &PointBudgetCatalog,
        selections: &[PointSelection],
    ) -> PointBudgetEvaluation {
        evaluate(catalog, selections)
    }

    fn prepare(
        &self,
        catalog: &PointBudgetCatalog,
        selections: &[PointSelection],
    ) -> PointBudgetPreparation {
        prepare(catalog, selections)
    }
}

fn evaluate(catalog: &PointBudgetCatalog, selections: &[PointSelection]) -> PointBudgetEvaluation {
    prepare(catalog, selections).evaluation
}

fn prepare(catalog: &PointBudgetCatalog, selections: &[PointSelection]) -> PointBudgetPreparation {
    let (mut evaluation, facts) = evaluate_raw(catalog, selections);
    let draft = if evaluation.valid {
        match materialize_draft(&facts) {
            Ok(draft) => Some(draft),
            Err(fact_path) => {
                evaluation.failures.push(failure_with_fact(
                    PointBudgetFailureCode::InvalidCatalog,
                    None,
                    &fact_path,
                ));
                evaluation.valid = false;
                None
            }
        }
    } else {
        None
    };

    PointBudgetPreparation { evaluation, draft }
}

fn evaluate_raw(
    catalog: &PointBudgetCatalog,
    selections: &[PointSelection],
) -> (PointBudgetEvaluation, BTreeMap<String, PointFactValue>) {
    let options = catalog
        .options
        .iter()
        .map(|option| (option.id.get(), option))
        .collect::<BTreeMap<_, _>>();
    let mut ordered = selections.to_vec();
    ordered.sort_by_key(|selection| selection.option_id.get());

    let mut failures = validate_catalog(catalog, &options);
    let mut selected_ids = BTreeSet::new();
    let mut valid_selections = Vec::new();
    let mut lines = Vec::new();
    let mut spent_points = Some(0_i64);
    let mut facts = BTreeMap::new();

    for selection in &ordered {
        if !selected_ids.insert(selection.option_id.get()) {
            failures.push(failure(
                PointBudgetFailureCode::DuplicateOption,
                Some(selection.option_id),
            ));
            continue;
        }
        let Some(option) = options.get(&selection.option_id.get()).copied() else {
            failures.push(failure(
                PointBudgetFailureCode::UnknownOption,
                Some(selection.option_id),
            ));
            continue;
        };
        if selection.quantity < option.minimum_quantity
            || selection.quantity > option.maximum_quantity
            || (option.cost_kind == PointCostKind::Fixed && selection.quantity != 1)
        {
            failures.push(failure(
                PointBudgetFailureCode::InvalidQuantity,
                Some(selection.option_id),
            ));
            continue;
        }
        let Some(delta) = option_delta(option, selection.quantity) else {
            spent_points = None;
            failures.push(failure(
                PointBudgetFailureCode::PointOverflow,
                Some(selection.option_id),
            ));
            continue;
        };
        if let Some(spent) = spent_points {
            spent_points = spent.checked_add(delta);
            if spent_points.is_none() {
                failures.push(failure(
                    PointBudgetFailureCode::PointOverflow,
                    Some(selection.option_id),
                ));
            }
        }
        lines.push(PointLedgerLine {
            option_id: option.id,
            option_key: option.option_key.clone(),
            quantity: selection.quantity,
            point_delta: delta,
        });
        apply_effect(option, selection.quantity, &mut facts, &mut failures);
        valid_selections.push((option, selection.quantity));
    }

    validate_groups(catalog, &valid_selections, &mut failures);
    validate_conditions(&valid_selections, &selected_ids, &facts, &mut failures);

    let remaining_points = spent_points.and_then(|spent| catalog.total_points.checked_sub(spent));
    if spent_points.is_some() && remaining_points.is_none() {
        failures.push(failure(PointBudgetFailureCode::PointOverflow, None));
    }
    if spent_points.is_some_and(|spent| spent > catalog.total_points) {
        failures.push(failure(PointBudgetFailureCode::BudgetExceeded, None));
    }
    deduplicate_failures(&mut failures);

    (
        PointBudgetEvaluation {
            point_budget_version_id: catalog.id,
            valid: failures.is_empty(),
            total_points: catalog.total_points,
            spent_points,
            remaining_points,
            lines,
            failures,
        },
        facts,
    )
}

fn validate_catalog<'a>(
    catalog: &'a PointBudgetCatalog,
    options: &BTreeMap<u64, &'a PointBudgetOption>,
) -> Vec<PointBudgetFailure> {
    let mut failures = Vec::new();
    if catalog.total_points < 0 || options.len() != catalog.options.len() {
        failures.push(failure(PointBudgetFailureCode::InvalidCatalog, None));
    }
    for option in &catalog.options {
        let scalar_shape_valid = match option.cost_kind {
            PointCostKind::Fixed | PointCostKind::PerUnit => {
                option.point_delta_per_unit.is_some() && option.tiers.is_empty()
            }
            PointCostKind::Tiered => {
                option.point_delta_per_unit.is_none()
                    && tiers_cover(option.minimum_quantity, option.maximum_quantity, option)
            }
        };
        if option.minimum_quantity == 0
            || option.minimum_quantity > option.maximum_quantity
            || !scalar_shape_valid
            || !effect_shape_valid(&option.effect)
        {
            failures.push(failure(
                PointBudgetFailureCode::InvalidCatalog,
                Some(option.id),
            ));
        }
    }
    failures
}

fn effect_shape_valid(effect: &PointEffect) -> bool {
    match effect {
        PointEffect::SetInteger { fact_path, .. }
        | PointEffect::IncrementInteger { fact_path, .. } => integer_fact_path(fact_path),
        PointEffect::SetText { fact_path, .. } => text_fact_path(fact_path),
    }
}

fn integer_fact_path(path: &str) -> bool {
    matches!(
        path,
        "age"
            | "careerYears"
            | "certifications"
            | "startingCashKrw"
            | "studentLoanKrw"
            | "creditLoanKrw"
            | "dependents"
    )
}

fn text_fact_path(path: &str) -> bool {
    matches!(
        path,
        "name" | "gender" | "military" | "region" | "background" | "education" | "health"
    )
}

fn materialize_draft(facts: &BTreeMap<String, PointFactValue>) -> Result<CharacterDraft, String> {
    let mut draft = custom_base_draft();
    for (path, value) in facts {
        match (path.as_str(), value) {
            ("name", PointFactValue::Text(value)) => draft.name.clone_from(value),
            ("age", PointFactValue::Integer(value)) => draft.age = to_u32(path, *value)?,
            ("gender", PointFactValue::Text(value)) => draft.gender = to_enum(path, value)?,
            ("military", PointFactValue::Text(value)) => draft.military = to_enum(path, value)?,
            ("region", PointFactValue::Text(value)) => draft.region = to_enum(path, value)?,
            ("background", PointFactValue::Text(value)) => {
                draft.background = to_enum(path, value)?;
            }
            ("education", PointFactValue::Text(value)) => {
                draft.education = to_enum(path, value)?;
            }
            ("careerYears", PointFactValue::Integer(value)) => {
                draft.career_years = to_u32(path, *value)?;
            }
            ("certifications", PointFactValue::Integer(value)) => {
                draft.certifications = to_u32(path, *value)?;
            }
            ("startingCashKrw", PointFactValue::Integer(value)) => {
                draft.starting_cash_krw = *value;
            }
            ("studentLoanKrw", PointFactValue::Integer(value)) => {
                draft.student_loan_krw = *value;
            }
            ("creditLoanKrw", PointFactValue::Integer(value)) => {
                draft.credit_loan_krw = *value;
            }
            ("health", PointFactValue::Text(value)) => draft.health = to_enum(path, value)?,
            ("dependents", PointFactValue::Integer(value)) => {
                draft.dependents = to_u32(path, *value)?;
            }
            _ => return Err(path.clone()),
        }
    }

    Ok(draft)
}

fn custom_base_draft() -> CharacterDraft {
    CharacterDraft {
        name: "커스텀 실행".to_owned(),
        age: 25,
        gender: Gender::Other,
        military: MilitaryStatus::Completed,
        region: Region::CapitalArea,
        background: FamilyBackground::Independent,
        education: Education::HighSchool,
        career_years: 0,
        certifications: 0,
        starting_cash_krw: 0,
        student_loan_krw: 0,
        credit_loan_krw: 0,
        health: Health::Normal,
        dependents: 0,
    }
}

fn to_u32(path: &str, value: i64) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| path.to_owned())
}

fn to_enum<T>(path: &str, value: &str) -> Result<T, String>
where
    T: DeserializeOwned,
{
    serde_json::from_value(serde_json::Value::String(value.to_owned())).map_err(|_| path.to_owned())
}

fn tiers_cover(minimum: u32, maximum: u32, option: &PointBudgetOption) -> bool {
    let mut expected = 1_u32;
    for tier in &option.tiers {
        if tier.minimum_quantity != expected || tier.maximum_quantity < tier.minimum_quantity {
            return false;
        }
        let Some(next) = tier.maximum_quantity.checked_add(1) else {
            return false;
        };
        expected = next;
    }
    expected == maximum.saturating_add(1) && minimum >= 1
}

fn option_delta(option: &PointBudgetOption, quantity: u32) -> Option<i64> {
    match option.cost_kind {
        PointCostKind::Fixed => option.point_delta_per_unit,
        PointCostKind::PerUnit => option
            .point_delta_per_unit?
            .checked_mul(i64::from(quantity)),
        PointCostKind::Tiered => option.tiers.iter().try_fold(0_i64, |total, tier| {
            if quantity < tier.minimum_quantity {
                return Some(total);
            }
            let upper = quantity.min(tier.maximum_quantity);
            let units = upper.checked_sub(tier.minimum_quantity)?.checked_add(1)?;
            total.checked_add(tier.point_delta_per_unit.checked_mul(i64::from(units))?)
        }),
    }
}

fn apply_effect(
    option: &PointBudgetOption,
    quantity: u32,
    facts: &mut BTreeMap<String, PointFactValue>,
    failures: &mut Vec<PointBudgetFailure>,
) {
    match &option.effect {
        PointEffect::SetInteger { fact_path, value } => set_fact(
            facts,
            fact_path,
            PointFactValue::Integer(*value),
            option.id,
            failures,
        ),
        PointEffect::IncrementInteger {
            fact_path,
            value_per_unit,
        } => {
            let Some(increment) = value_per_unit.checked_mul(i64::from(quantity)) else {
                failures.push(failure_with_fact(
                    PointBudgetFailureCode::PointOverflow,
                    Some(option.id),
                    fact_path,
                ));
                return;
            };
            let current = match facts.get(fact_path) {
                Some(PointFactValue::Integer(value)) => *value,
                Some(PointFactValue::Text(_)) => {
                    failures.push(failure_with_fact(
                        PointBudgetFailureCode::ConflictingFact,
                        Some(option.id),
                        fact_path,
                    ));
                    return;
                }
                None => 0,
            };
            let Some(value) = current.checked_add(increment) else {
                failures.push(failure_with_fact(
                    PointBudgetFailureCode::PointOverflow,
                    Some(option.id),
                    fact_path,
                ));
                return;
            };
            facts.insert(fact_path.clone(), PointFactValue::Integer(value));
        }
        PointEffect::SetText { fact_path, value } => set_fact(
            facts,
            fact_path,
            PointFactValue::Text(value.clone()),
            option.id,
            failures,
        ),
    }
}

fn set_fact(
    facts: &mut BTreeMap<String, PointFactValue>,
    path: &str,
    value: PointFactValue,
    option_id: ResourceId,
    failures: &mut Vec<PointBudgetFailure>,
) {
    if facts.get(path).is_some_and(|existing| existing != &value) {
        failures.push(failure_with_fact(
            PointBudgetFailureCode::ConflictingFact,
            Some(option_id),
            path,
        ));
        return;
    }
    facts.insert(path.to_owned(), value);
}

fn validate_groups(
    catalog: &PointBudgetCatalog,
    selections: &[(&PointBudgetOption, u32)],
    failures: &mut Vec<PointBudgetFailure>,
) {
    for group in &catalog.groups {
        let count = selections
            .iter()
            .filter(|(option, _)| option.exclusive_group_key.as_deref() == Some(&group.group_key))
            .count();
        if count == 0 {
            failures.push(failure_with_group(
                PointBudgetFailureCode::MissingExclusiveGroup,
                &group.group_key,
            ));
        } else if count > 1 {
            failures.push(failure_with_group(
                PointBudgetFailureCode::MultipleExclusiveGroup,
                &group.group_key,
            ));
        }
    }
}

fn validate_conditions(
    selections: &[(&PointBudgetOption, u32)],
    selected_ids: &BTreeSet<u64>,
    facts: &BTreeMap<String, PointFactValue>,
    failures: &mut Vec<PointBudgetFailure>,
) {
    for (option, _) in selections {
        for condition in &option.conditions {
            match condition {
                PointCondition::RequiresOption { option_id }
                    if !selected_ids.contains(&option_id.get()) =>
                {
                    failures.push(failure_with_related(
                        PointBudgetFailureCode::RequiredOptionMissing,
                        option.id,
                        *option_id,
                    ));
                }
                PointCondition::ForbidsOption { option_id }
                    if selected_ids.contains(&option_id.get()) =>
                {
                    failures.push(failure_with_related(
                        PointBudgetFailureCode::ForbiddenOptionSelected,
                        option.id,
                        *option_id,
                    ));
                }
                PointCondition::RequiresFact {
                    fact_path,
                    comparison,
                    expected,
                } if !facts
                    .get(fact_path)
                    .is_some_and(|actual| fact_matches(actual, *comparison, expected)) =>
                {
                    failures.push(failure_with_fact(
                        PointBudgetFailureCode::RequiredFactMissing,
                        Some(option.id),
                        fact_path,
                    ));
                }
                PointCondition::ForbidsFact {
                    fact_path,
                    comparison,
                    expected,
                } if facts
                    .get(fact_path)
                    .is_some_and(|actual| fact_matches(actual, *comparison, expected)) =>
                {
                    failures.push(failure_with_fact(
                        PointBudgetFailureCode::ForbiddenFactMatched,
                        Some(option.id),
                        fact_path,
                    ));
                }
                _ => {}
            }
        }
    }
}

fn fact_matches(
    actual: &PointFactValue,
    comparison: PointFactComparison,
    expected: &PointFactValue,
) -> bool {
    match (actual, expected) {
        (PointFactValue::Integer(actual), PointFactValue::Integer(expected)) => match comparison {
            PointFactComparison::Equal => actual == expected,
            PointFactComparison::GreaterOrEqual => actual >= expected,
            PointFactComparison::LessOrEqual => actual <= expected,
        },
        (PointFactValue::Text(actual), PointFactValue::Text(expected)) => {
            comparison == PointFactComparison::Equal && actual == expected
        }
        _ => false,
    }
}

fn failure(code: PointBudgetFailureCode, option_id: Option<ResourceId>) -> PointBudgetFailure {
    PointBudgetFailure {
        code,
        option_id,
        related_option_id: None,
        group_key: None,
        fact_path: None,
    }
}

fn failure_with_related(
    code: PointBudgetFailureCode,
    option_id: ResourceId,
    related_option_id: ResourceId,
) -> PointBudgetFailure {
    PointBudgetFailure {
        code,
        option_id: Some(option_id),
        related_option_id: Some(related_option_id),
        group_key: None,
        fact_path: None,
    }
}

fn failure_with_group(code: PointBudgetFailureCode, group_key: &str) -> PointBudgetFailure {
    PointBudgetFailure {
        code,
        option_id: None,
        related_option_id: None,
        group_key: Some(group_key.to_owned()),
        fact_path: None,
    }
}

fn failure_with_fact(
    code: PointBudgetFailureCode,
    option_id: Option<ResourceId>,
    fact_path: &str,
) -> PointBudgetFailure {
    PointBudgetFailure {
        code,
        option_id,
        related_option_id: None,
        group_key: None,
        fact_path: Some(fact_path.to_owned()),
    }
}

fn deduplicate_failures(failures: &mut Vec<PointBudgetFailure>) {
    let mut seen = BTreeSet::new();
    failures.retain(|failure| {
        seen.insert((
            format!("{:?}", failure.code),
            failure.option_id.map(ResourceId::get),
            failure.related_option_id.map(ResourceId::get),
            failure.group_key.clone(),
            failure.fact_path.clone(),
        ))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::{PointExclusiveGroup, PointTier};

    fn given_option(
        id: u64,
        key: &str,
        cost_kind: PointCostKind,
        point_delta_per_unit: Option<i64>,
        minimum_quantity: u32,
        maximum_quantity: u32,
    ) -> PointBudgetOption {
        let fact_path = match id {
            1 => "age",
            2 => "careerYears",
            3 => "certifications",
            _ => "dependents",
        };
        PointBudgetOption {
            id: ResourceId::from_u64(id),
            option_key: key.to_owned(),
            display_name: key.to_owned(),
            description: key.to_owned(),
            cost_kind,
            point_delta_per_unit,
            minimum_quantity,
            maximum_quantity,
            exclusive_group_key: None,
            effect: PointEffect::SetInteger {
                fact_path: fact_path.to_owned(),
                value: i64::try_from(id).expect("fixture id fits i64"),
            },
            tiers: Vec::new(),
            conditions: Vec::new(),
        }
    }

    fn given_catalog(options: Vec<PointBudgetOption>) -> PointBudgetCatalog {
        PointBudgetCatalog {
            id: ResourceId::from_u64(1),
            budget_key: "test-budget".to_owned(),
            version: 1,
            display_name: "테스트 예산".to_owned(),
            description: "테스트".to_owned(),
            total_points: 100,
            ranked_eligible: false,
            canonical_sha256: "0".repeat(64),
            groups: Vec::new(),
            options,
        }
    }

    fn when_evaluated(
        catalog: &PointBudgetCatalog,
        selections: &[PointSelection],
    ) -> PointBudgetEvaluation {
        evaluate(catalog, selections)
    }

    fn when_prepared(
        catalog: &PointBudgetCatalog,
        selections: &[PointSelection],
    ) -> PointBudgetPreparation {
        prepare(catalog, selections)
    }

    mod context_cost_shapes_are_mixed {
        use super::*;

        #[test]
        fn given_fixed_per_unit_tiered_and_refund_when_evaluated_then_integer_ledger_is_returned() {
            let fixed = given_option(1, "fixed", PointCostKind::Fixed, Some(10), 1, 1);
            let per_unit = given_option(2, "per-unit", PointCostKind::PerUnit, Some(3), 1, 5);
            let mut tiered = given_option(3, "tiered", PointCostKind::Tiered, None, 1, 5);
            tiered.tiers = vec![
                PointTier {
                    minimum_quantity: 1,
                    maximum_quantity: 2,
                    point_delta_per_unit: 2,
                },
                PointTier {
                    minimum_quantity: 3,
                    maximum_quantity: 5,
                    point_delta_per_unit: 4,
                },
            ];
            let refund = given_option(4, "refund", PointCostKind::Fixed, Some(-5), 1, 1);
            let catalog = given_catalog(vec![fixed, per_unit, tiered, refund]);
            let selections = vec![
                PointSelection {
                    option_id: ResourceId::from_u64(3),
                    quantity: 4,
                },
                PointSelection {
                    option_id: ResourceId::from_u64(1),
                    quantity: 1,
                },
                PointSelection {
                    option_id: ResourceId::from_u64(4),
                    quantity: 1,
                },
                PointSelection {
                    option_id: ResourceId::from_u64(2),
                    quantity: 2,
                },
            ];

            let result = when_evaluated(&catalog, &selections);

            assert!(result.valid);
            assert_eq!(result.spent_points, Some(23));
            assert_eq!(result.remaining_points, Some(77));
            assert_eq!(
                result
                    .lines
                    .iter()
                    .map(|line| line.option_id.get())
                    .collect::<Vec<_>>(),
                vec![1, 2, 3, 4]
            );
        }
    }

    mod context_selection_breaks_catalog_constraints {
        use super::*;

        #[test]
        fn given_duplicate_and_missing_group_when_evaluated_then_both_failures_are_returned() {
            let mut option = given_option(1, "education", PointCostKind::Fixed, Some(10), 1, 1);
            option.exclusive_group_key = Some("education".to_owned());
            let mut catalog = given_catalog(vec![option]);
            catalog.groups = vec![PointExclusiveGroup {
                group_key: "education".to_owned(),
                display_name: "학력".to_owned(),
            }];
            let selections = vec![
                PointSelection {
                    option_id: ResourceId::from_u64(1),
                    quantity: 2,
                },
                PointSelection {
                    option_id: ResourceId::from_u64(1),
                    quantity: 1,
                },
            ];

            let result = when_evaluated(&catalog, &selections);

            assert!(!result.valid);
            assert!(
                result
                    .failures
                    .iter()
                    .any(|failure| failure.code == PointBudgetFailureCode::DuplicateOption)
            );
            assert!(
                result.failures.iter().any(|failure| {
                    failure.code == PointBudgetFailureCode::MissingExclusiveGroup
                })
            );
        }

        #[test]
        fn given_required_option_is_absent_when_evaluated_then_requirement_fails() {
            let mut option = given_option(1, "master", PointCostKind::Fixed, Some(20), 1, 1);
            option.conditions = vec![PointCondition::RequiresOption {
                option_id: ResourceId::from_u64(2),
            }];
            let catalog = given_catalog(vec![
                option,
                given_option(2, "bachelor", PointCostKind::Fixed, Some(10), 1, 1),
            ]);

            let result = when_evaluated(
                &catalog,
                &[PointSelection {
                    option_id: ResourceId::from_u64(1),
                    quantity: 1,
                }],
            );

            assert!(
                result.failures.iter().any(|failure| {
                    failure.code == PointBudgetFailureCode::RequiredOptionMissing
                })
            );
        }

        #[test]
        fn given_spent_points_exceed_budget_when_evaluated_then_budget_fails() {
            let catalog = given_catalog(vec![given_option(
                1,
                "expensive",
                PointCostKind::Fixed,
                Some(101),
                1,
                1,
            )]);

            let result = when_evaluated(
                &catalog,
                &[PointSelection {
                    option_id: ResourceId::from_u64(1),
                    quantity: 1,
                }],
            );

            assert_eq!(result.spent_points, Some(101));
            assert_eq!(result.remaining_points, Some(-1));
            assert!(
                result
                    .failures
                    .iter()
                    .any(|failure| failure.code == PointBudgetFailureCode::BudgetExceeded)
            );
        }
    }

    mod context_valid_selection_is_materialized {
        use super::*;

        #[test]
        fn given_canonical_options_when_prepared_then_same_ledger_builds_the_character_draft() {
            let mut education = given_option(1, "education", PointCostKind::Fixed, Some(0), 1, 1);
            education.exclusive_group_key = Some("education".to_owned());
            education.effect = PointEffect::SetText {
                fact_path: "education".to_owned(),
                value: "bachelor".to_owned(),
            };
            let mut cash = given_option(2, "cash", PointCostKind::PerUnit, Some(1), 1, 300);
            cash.exclusive_group_key = Some("startingCash".to_owned());
            cash.effect = PointEffect::IncrementInteger {
                fact_path: "startingCashKrw".to_owned(),
                value_per_unit: 1_000_000,
            };
            let mut health = given_option(3, "health", PointCostKind::Fixed, Some(0), 1, 1);
            health.exclusive_group_key = Some("health".to_owned());
            health.effect = PointEffect::SetText {
                fact_path: "health".to_owned(),
                value: "normal".to_owned(),
            };
            let mut background = given_option(4, "background", PointCostKind::Fixed, Some(0), 1, 1);
            background.exclusive_group_key = Some("background".to_owned());
            background.effect = PointEffect::SetText {
                fact_path: "background".to_owned(),
                value: "independent".to_owned(),
            };
            let mut certifications =
                given_option(5, "certifications", PointCostKind::Fixed, Some(0), 1, 1);
            certifications.exclusive_group_key = Some("certifications".to_owned());
            certifications.effect = PointEffect::SetInteger {
                fact_path: "certifications".to_owned(),
                value: 2,
            };
            let mut catalog =
                given_catalog(vec![education, cash, health, background, certifications]);
            catalog.groups = [
                "education",
                "startingCash",
                "health",
                "background",
                "certifications",
            ]
            .into_iter()
            .map(|group_key| PointExclusiveGroup {
                group_key: group_key.to_owned(),
                display_name: group_key.to_owned(),
            })
            .collect();
            let selections = vec![
                PointSelection {
                    option_id: ResourceId::from_u64(5),
                    quantity: 1,
                },
                PointSelection {
                    option_id: ResourceId::from_u64(2),
                    quantity: 10,
                },
                PointSelection {
                    option_id: ResourceId::from_u64(1),
                    quantity: 1,
                },
                PointSelection {
                    option_id: ResourceId::from_u64(4),
                    quantity: 1,
                },
                PointSelection {
                    option_id: ResourceId::from_u64(3),
                    quantity: 1,
                },
            ];

            let result = when_prepared(&catalog, &selections);

            assert!(result.evaluation.valid);
            assert_eq!(result.evaluation.spent_points, Some(10));
            assert_eq!(result.evaluation.remaining_points, Some(90));
            let draft = result.draft.expect("유효한 선택은 draft를 만들어야 한다");
            assert_eq!(draft.name, "커스텀 실행");
            assert_eq!(draft.education, Education::Bachelor);
            assert_eq!(draft.starting_cash_krw, 10_000_000);
            assert_eq!(draft.certifications, 2);
        }

        #[test]
        fn given_unknown_effect_path_when_prepared_then_preview_and_draft_both_fail() {
            let mut option = given_option(1, "unknown", PointCostKind::Fixed, Some(0), 1, 1);
            option.effect = PointEffect::SetText {
                fact_path: "unpublishedField".to_owned(),
                value: "value".to_owned(),
            };
            let catalog = given_catalog(vec![option]);
            let selections = [PointSelection {
                option_id: ResourceId::from_u64(1),
                quantity: 1,
            }];

            let result = when_prepared(&catalog, &selections);

            assert!(!result.evaluation.valid);
            assert!(result.draft.is_none());
            assert!(result.evaluation.failures.iter().any(|failure| {
                failure.code == PointBudgetFailureCode::InvalidCatalog
                    && failure.option_id == Some(ResourceId::from_u64(1))
            }));
        }
    }
}
