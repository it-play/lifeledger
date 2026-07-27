use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::finance::ResourceId;

use super::types::*;

const ENTROPY_DOMAIN: &str = "lifeledger.lifeEvent.v1";
const ENTROPY_STAGE: &str = "eligibility";
const HMAC_BLOCK_BYTES: usize = 64;
const MAX_CANDIDATE_OCCURRENCE: u16 = 256;

struct V1LifeEventRules {
    entropy: Arc<dyn LifeEventEntropy>,
}

struct HmacSha256LifeEventEntropy;

/// Creates the sealed-schema v1 life-event rules with the canonical HMAC stream.
pub fn create_life_event_rules() -> Arc<dyn LifeEventRules> {
    create_life_event_rules_with_entropy(Arc::new(HmacSha256LifeEventEntropy))
}

/// Creates v1 rules with a replaceable entropy primitive for deterministic tests.
pub fn create_life_event_rules_with_entropy(
    entropy: Arc<dyn LifeEventEntropy>,
) -> Arc<dyn LifeEventRules> {
    Arc::new(V1LifeEventRules { entropy })
}

impl LifeEventEntropy for HmacSha256LifeEventEntropy {
    fn digest(
        &self,
        world_seed: u64,
        canonical_message: &[u8],
    ) -> Result<[u8; 32], LifeEventError> {
        Ok(hmac_sha256(&world_seed.to_be_bytes(), canonical_message))
    }
}

impl LifeEventRules for V1LifeEventRules {
    fn validate_catalog(&self, catalog: &LifeEventCatalog) -> Result<(), LifeEventError> {
        validate_catalog(catalog)
    }

    fn evaluate_eligibility(
        &self,
        input: LifeEventEligibilityInput<'_>,
    ) -> Result<LifeEventTruth, LifeEventError> {
        validate_catalog(input.catalog)?;
        let definition = find_definition(input.catalog, input.event_definition_id)?;
        let evidence = prepare_evidence(input.catalog, input.facts)?;
        evaluate_expression(&definition.eligibility_ast.root, &evidence)
    }

    fn eligibility_digest(
        &self,
        input: LifeEventEntropyInput<'_>,
    ) -> Result<[u8; 32], LifeEventError> {
        let message = canonical_entropy_message(input)?;
        self.entropy.digest(input.world_seed, &message)
    }

    fn eligibility_roll_ppm(
        &self,
        input: LifeEventEntropyInput<'_>,
    ) -> Result<u32, LifeEventError> {
        let digest = self.eligibility_digest(input)?;
        Ok(scale_word_to_ppm(first_u64(digest)))
    }

    fn plan_month(
        &self,
        input: LifeEventMonthPlanInput<'_>,
    ) -> Result<LifeEventMonthPlan, LifeEventError> {
        plan_month(self, input)
    }

    fn plan_effect(
        &self,
        input: LifeEventEffectPlanInput<'_>,
    ) -> Result<LifeEventEffectPlan, LifeEventError> {
        plan_effect(input)
    }

    fn resolve_choice(
        &self,
        input: LifeEventChoiceResolutionInput<'_>,
    ) -> Result<LifeEventResolutionPlan, LifeEventError> {
        resolve_choice(input)
    }

    fn resolve_expired(
        &self,
        input: LifeEventExpiryResolutionInput<'_>,
    ) -> Result<LifeEventResolutionPlan, LifeEventError> {
        resolve_expired(input)
    }
}

#[derive(Debug, Clone, Copy)]
struct ExpectedFact {
    order: u8,
    key: &'static str,
    value_type: LifeEventValueType,
    unit: LifeEventUnit,
    enum_schema_key: Option<&'static str>,
    source_kind: LifeEventFactSourceKind,
}

const EXPECTED_FACTS: [ExpectedFact; 4] = [
    ExpectedFact {
        order: 1,
        key: "character.age",
        value_type: LifeEventValueType::AgeYears,
        unit: LifeEventUnit::Years,
        enum_schema_key: None,
        source_kind: LifeEventFactSourceKind::GameDay,
    },
    ExpectedFact {
        order: 2,
        key: "household.dependentCount",
        value_type: LifeEventValueType::Count,
        unit: LifeEventUnit::Count,
        enum_schema_key: None,
        source_kind: LifeEventFactSourceKind::Household,
    },
    ExpectedFact {
        order: 3,
        key: "residence.exists",
        value_type: LifeEventValueType::Boolean,
        unit: LifeEventUnit::Boolean,
        enum_schema_key: None,
        source_kind: LifeEventFactSourceKind::Residence,
    },
    ExpectedFact {
        order: 4,
        key: "military.status",
        value_type: LifeEventValueType::Enum,
        unit: LifeEventUnit::Enum,
        enum_schema_key: Some("military"),
        source_kind: LifeEventFactSourceKind::Military,
    },
];

fn validate_catalog(catalog: &LifeEventCatalog) -> Result<(), LifeEventError> {
    if catalog.fact_registry_schema_version != LIFE_EVENT_FACT_REGISTRY_SCHEMA_VERSION {
        return Err(LifeEventError::UnsupportedSchemaVersion);
    }
    if !is_component_version_key(&catalog.component_version_key) {
        return Err(LifeEventError::InvalidComponentVersionKey);
    }
    validate_fact_registry(&catalog.facts)?;
    if catalog.definitions.is_empty() || catalog.definitions.len() > LIFE_EVENT_MAX_DEFINITIONS {
        return Err(LifeEventError::InvalidDefinitionLimits);
    }

    let fact_map = catalog
        .facts
        .iter()
        .map(|fact| (fact.fact_key.as_str(), fact))
        .collect::<BTreeMap<_, _>>();
    let mut definition_ids = BTreeSet::new();
    let mut definition_keys = BTreeSet::new();
    let mut choice_ids = BTreeSet::new();
    let mut definitions = catalog.definitions.iter().collect::<Vec<_>>();
    definitions.sort_by(|left, right| left.event_key.cmp(&right.event_key));

    for (index, definition) in definitions.into_iter().enumerate() {
        let expected_order =
            u8::try_from(index + 1).map_err(|_| LifeEventError::InvalidDefinitionOrder)?;
        if definition.event_order != expected_order {
            return Err(LifeEventError::InvalidDefinitionOrder);
        }
        if !definition_ids.insert(definition.id) || !definition_keys.insert(&definition.event_key) {
            return Err(LifeEventError::DuplicateDefinition);
        }
        validate_definition(definition, &fact_map, &mut choice_ids)?;
    }
    Ok(())
}

fn validate_fact_registry(facts: &[LifeEventFactDefinition]) -> Result<(), LifeEventError> {
    if facts.len() != EXPECTED_FACTS.len() {
        return Err(LifeEventError::InvalidFactRegistry);
    }
    let mut ids = BTreeSet::new();
    let mut keys = BTreeSet::new();
    let mut orders = BTreeSet::new();

    for (fact, expected) in facts.iter().zip(EXPECTED_FACTS) {
        if !ids.insert(fact.id) || !keys.insert(&fact.fact_key) || !orders.insert(fact.fact_order) {
            return Err(LifeEventError::DuplicateFact);
        }
        if fact.fact_order != expected.order
            || fact.fact_key != expected.key
            || fact.value_type != expected.value_type
            || fact.unit != expected.unit
            || fact.enum_schema_key.as_deref() != expected.enum_schema_key
            || fact.window_kind != LifeEventWindowKind::CurrentGameDay
            || fact.source_schema_version != LIFE_EVENT_FACT_REGISTRY_SCHEMA_VERSION
            || fact.source_kind != expected.source_kind
        {
            return Err(LifeEventError::InvalidFactRegistry);
        }
    }
    Ok(())
}

fn validate_definition(
    definition: &LifeEventDefinition,
    facts: &BTreeMap<&str, &LifeEventFactDefinition>,
    choice_ids: &mut BTreeSet<ResourceId>,
) -> Result<(), LifeEventError> {
    if definition.schema_version != LIFE_EVENT_SCHEMA_VERSION
        || definition.eligibility_ast.version != LIFE_EVENT_SCHEMA_VERSION
    {
        return Err(LifeEventError::UnsupportedSchemaVersion);
    }
    if definition.entropy_stream_version != LIFE_EVENT_ENTROPY_STREAM_VERSION {
        return Err(LifeEventError::UnsupportedEntropyStreamVersion);
    }
    if !is_canonical_key(&definition.event_key)
        || !is_canonical_key(&definition.default_choice_key)
        || definition
            .exclusive_group_key
            .as_deref()
            .is_some_and(|key| !is_canonical_key(key))
    {
        return Err(LifeEventError::InvalidCanonicalKey);
    }
    if !is_display_name(&definition.display_name, 80) {
        return Err(LifeEventError::InvalidDisplayName);
    }
    if definition.hazard_ppm > LIFE_EVENT_PROBABILITY_SCALE_PPM {
        return Err(LifeEventError::InvalidProbability);
    }
    if definition.cooldown_game_days > 3_660
        || !(1..=255).contains(&definition.maximum_occurrences)
        || !(1..=366).contains(&definition.offer_duration_game_days)
    {
        return Err(LifeEventError::InvalidDefinitionLimits);
    }
    if !matches!(
        definition.eligibility_ast.root,
        LifeEventExpression::All { .. }
    ) {
        return Err(LifeEventError::InvalidEligibilityRoot);
    }

    let analysis = analyze_expression(&definition.eligibility_ast.root, facts)?;
    if analysis.value_type != LifeEventValueType::Boolean {
        return Err(LifeEventError::InvalidEligibilityRoot);
    }
    if analysis.nodes > LIFE_EVENT_MAX_AST_NODES {
        return Err(LifeEventError::AstTooLarge);
    }
    if analysis.depth > LIFE_EVENT_MAX_AST_DEPTH {
        return Err(LifeEventError::AstTooDeep);
    }
    if usize::from(definition.ast_node_count) != analysis.nodes
        || usize::from(definition.ast_max_depth) != analysis.depth
    {
        return Err(LifeEventError::AstProjectionMismatch);
    }

    if !(LIFE_EVENT_MIN_CHOICES..=LIFE_EVENT_MAX_CHOICES).contains(&definition.choices.len()) {
        return Err(LifeEventError::InvalidDefinitionLimits);
    }
    let mut choice_keys = BTreeSet::new();
    let mut default_choice = None;
    for (index, choice) in definition.choices.iter().enumerate() {
        let expected_order =
            u8::try_from(index + 1).map_err(|_| LifeEventError::InvalidChoiceOrder)?;
        if choice.choice_order != expected_order {
            return Err(LifeEventError::InvalidChoiceOrder);
        }
        if !choice_ids.insert(choice.id) || !choice_keys.insert(&choice.choice_key) {
            return Err(LifeEventError::DuplicateChoice);
        }
        validate_choice(choice)?;
        if choice.choice_key == definition.default_choice_key {
            default_choice = Some(choice);
        }
    }
    if !matches!(
        default_choice,
        Some(LifeEventChoiceDefinition {
            effect_kind: LifeEventEffectKind::NoEffect,
            effect_ast: LifeEventEffectAst {
                effect: LifeEventEffect::NoEffect,
                ..
            },
            ..
        })
    ) {
        return Err(LifeEventError::InvalidDefaultChoice);
    }
    Ok(())
}

fn validate_choice(choice: &LifeEventChoiceDefinition) -> Result<(), LifeEventError> {
    if !is_canonical_key(&choice.choice_key) {
        return Err(LifeEventError::InvalidCanonicalKey);
    }
    if !is_display_name(&choice.display_name, 120) {
        return Err(LifeEventError::InvalidDisplayName);
    }
    if choice.effect_ast.version != LIFE_EVENT_SCHEMA_VERSION {
        return Err(LifeEventError::UnsupportedSchemaVersion);
    }
    match (
        choice.effect_kind,
        choice.effect_amount_krw,
        choice.effect_account_code,
        &choice.effect_ast.effect,
    ) {
        (LifeEventEffectKind::NoEffect, None, None, LifeEventEffect::NoEffect) => Ok(()),
        (
            LifeEventEffectKind::FixedWalletExpense,
            Some(projected_amount),
            Some(LifeEventEffectAccountCode::LifeEventExpense),
            LifeEventEffect::FixedWalletExpense {
                amount_krw,
                account_code: LifeEventEffectAccountCode::LifeEventExpense,
            },
        ) if projected_amount == *amount_krw
            && (1..=LIFE_EVENT_MAX_EFFECT_KRW).contains(amount_krw) =>
        {
            Ok(())
        }
        _ => Err(LifeEventError::InvalidEffect),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExpressionAnalysis {
    value_type: LifeEventValueType,
    enum_schema_key: Option<String>,
    nodes: usize,
    depth: usize,
}

impl ExpressionAnalysis {
    fn boolean_node(children: &[Self]) -> Result<Self, LifeEventError> {
        let child_nodes = children.iter().try_fold(0_usize, |total, child| {
            total
                .checked_add(child.nodes)
                .ok_or(LifeEventError::AstTooLarge)
        })?;
        let nodes = child_nodes
            .checked_add(1)
            .ok_or(LifeEventError::AstTooLarge)?;
        let depth = children
            .iter()
            .map(|child| child.depth)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(LifeEventError::AstTooDeep)?;
        Ok(Self {
            value_type: LifeEventValueType::Boolean,
            enum_schema_key: None,
            nodes,
            depth,
        })
    }
}

fn analyze_expression(
    expression: &LifeEventExpression,
    facts: &BTreeMap<&str, &LifeEventFactDefinition>,
) -> Result<ExpressionAnalysis, LifeEventError> {
    let analysis = match expression {
        LifeEventExpression::All { children } | LifeEventExpression::Any { children } => {
            if children.is_empty() || children.len() > LIFE_EVENT_MAX_LOGICAL_CHILDREN {
                return Err(LifeEventError::InvalidLogicalArity);
            }
            let analyses = children
                .iter()
                .map(|child| analyze_expression(child, facts))
                .collect::<Result<Vec<_>, _>>()?;
            if analyses
                .iter()
                .any(|child| child.value_type != LifeEventValueType::Boolean)
            {
                return Err(LifeEventError::TypeMismatch);
            }
            ExpressionAnalysis::boolean_node(&analyses)?
        }
        LifeEventExpression::Not { child } => {
            let child = analyze_expression(child, facts)?;
            if child.value_type != LifeEventValueType::Boolean {
                return Err(LifeEventError::TypeMismatch);
            }
            ExpressionAnalysis::boolean_node(&[child])?
        }
        LifeEventExpression::Eq { left, right } => {
            let left = analyze_operand(left, facts)?;
            let right = analyze_operand(right, facts)?;
            ensure_same_type(&left, &right)?;
            ExpressionAnalysis::boolean_node(&[left, right])?
        }
        LifeEventExpression::Gte { left, right } => {
            let left = analyze_operand(left, facts)?;
            let right = analyze_operand(right, facts)?;
            ensure_same_type(&left, &right)?;
            ensure_ordered(left.value_type)?;
            ExpressionAnalysis::boolean_node(&[left, right])?
        }
        LifeEventExpression::Between {
            value,
            lower,
            upper,
        } => {
            let value_analysis = analyze_operand(value, facts)?;
            let lower_analysis = analyze_operand(lower, facts)?;
            let upper_analysis = analyze_operand(upper, facts)?;
            ensure_same_type(&value_analysis, &lower_analysis)?;
            ensure_same_type(&value_analysis, &upper_analysis)?;
            ensure_ordered(value_analysis.value_type)?;
            if let (Some(lower), Some(upper)) = (literal_value(lower), literal_value(upper))
                && compare_known_values(&lower, &upper)? == Ordering::Greater
            {
                return Err(LifeEventError::InvalidBetweenBounds);
            }
            ExpressionAnalysis::boolean_node(&[value_analysis, lower_analysis, upper_analysis])?
        }
        LifeEventExpression::Fact { reference } => {
            let analysis = analyze_fact_reference(reference, facts)?;
            if analysis.value_type != LifeEventValueType::Boolean {
                return Err(LifeEventError::TypeMismatch);
            }
            analysis
        }
    };
    if analysis.nodes > LIFE_EVENT_MAX_AST_NODES {
        return Err(LifeEventError::AstTooLarge);
    }
    if analysis.depth > LIFE_EVENT_MAX_AST_DEPTH {
        return Err(LifeEventError::AstTooDeep);
    }
    Ok(analysis)
}

fn analyze_operand(
    operand: &LifeEventOperand,
    facts: &BTreeMap<&str, &LifeEventFactDefinition>,
) -> Result<ExpressionAnalysis, LifeEventError> {
    match operand {
        LifeEventOperand::Fact { reference } => analyze_fact_reference(reference, facts),
        LifeEventOperand::Literal { unit, value } => analyze_literal(*unit, value),
    }
}

fn analyze_fact_reference(
    reference: &LifeEventFactReference,
    facts: &BTreeMap<&str, &LifeEventFactDefinition>,
) -> Result<ExpressionAnalysis, LifeEventError> {
    let fact = facts
        .get(reference.path.as_str())
        .ok_or(LifeEventError::UnknownFact)?;
    if reference.unit != fact.unit || reference.window != fact.window_kind {
        return Err(LifeEventError::UnitMismatch);
    }
    Ok(ExpressionAnalysis {
        value_type: fact.value_type,
        enum_schema_key: fact.enum_schema_key.clone(),
        nodes: 1,
        depth: 1,
    })
}

fn analyze_literal(
    unit: LifeEventUnit,
    value: &LifeEventLiteralValue,
) -> Result<ExpressionAnalysis, LifeEventError> {
    let (value_type, expected_unit, enum_schema_key) = match value {
        LifeEventLiteralValue::Boolean(_) => {
            (LifeEventValueType::Boolean, LifeEventUnit::Boolean, None)
        }
        LifeEventLiteralValue::Count(value) if *value >= 0 => {
            (LifeEventValueType::Count, LifeEventUnit::Count, None)
        }
        LifeEventLiteralValue::AgeYears(value) if *value >= 0 => {
            (LifeEventValueType::AgeYears, LifeEventUnit::Years, None)
        }
        LifeEventLiteralValue::Enum { schema_key, value } => {
            validate_enum(schema_key, value)?;
            (
                LifeEventValueType::Enum,
                LifeEventUnit::Enum,
                Some(schema_key.clone()),
            )
        }
        _ => return Err(LifeEventError::InvalidLiteral),
    };
    if unit != expected_unit {
        return Err(LifeEventError::UnitMismatch);
    }
    Ok(ExpressionAnalysis {
        value_type,
        enum_schema_key,
        nodes: 1,
        depth: 1,
    })
}

fn ensure_same_type(
    left: &ExpressionAnalysis,
    right: &ExpressionAnalysis,
) -> Result<(), LifeEventError> {
    if left.value_type == right.value_type && left.enum_schema_key == right.enum_schema_key {
        Ok(())
    } else {
        Err(LifeEventError::TypeMismatch)
    }
}

fn ensure_ordered(value_type: LifeEventValueType) -> Result<(), LifeEventError> {
    if matches!(
        value_type,
        LifeEventValueType::Count | LifeEventValueType::AgeYears
    ) {
        Ok(())
    } else {
        Err(LifeEventError::UnorderedType)
    }
}

fn literal_value(operand: &LifeEventOperand) -> Option<LifeEventValue> {
    match operand {
        LifeEventOperand::Literal { value, .. } => Some(match value {
            LifeEventLiteralValue::Boolean(value) => LifeEventValue::Boolean(*value),
            LifeEventLiteralValue::Count(value) => LifeEventValue::Count(*value),
            LifeEventLiteralValue::AgeYears(value) => LifeEventValue::AgeYears(*value),
            LifeEventLiteralValue::Enum { schema_key, value } => LifeEventValue::Enum {
                schema_key: schema_key.clone(),
                value: value.clone(),
            },
        }),
        LifeEventOperand::Fact { .. } => None,
    }
}

fn prepare_evidence<'a>(
    catalog: &LifeEventCatalog,
    evidence: &'a [LifeEventFactEvidence],
) -> Result<BTreeMap<&'a str, &'a LifeEventEvidenceValue>, LifeEventError> {
    if evidence.len() != catalog.facts.len() {
        return Err(LifeEventError::InvalidEvidence);
    }
    let definitions = catalog
        .facts
        .iter()
        .map(|fact| (fact.fact_key.as_str(), fact))
        .collect::<BTreeMap<_, _>>();
    let mut values = BTreeMap::new();
    for fact in evidence {
        let definition = definitions
            .get(fact.fact_key.as_str())
            .ok_or(LifeEventError::InvalidEvidence)?;
        validate_evidence_value(definition, &fact.value)?;
        if values.insert(fact.fact_key.as_str(), &fact.value).is_some() {
            return Err(LifeEventError::InvalidEvidence);
        }
    }
    if definitions.keys().any(|key| !values.contains_key(key)) {
        return Err(LifeEventError::InvalidEvidence);
    }
    Ok(values)
}

fn validate_evidence_value(
    definition: &LifeEventFactDefinition,
    evidence: &LifeEventEvidenceValue,
) -> Result<(), LifeEventError> {
    let LifeEventEvidenceValue::Known(value) = evidence else {
        return Ok(());
    };
    match (definition.value_type, value) {
        (LifeEventValueType::Boolean, LifeEventValue::Boolean(_))
        | (LifeEventValueType::Count, LifeEventValue::Count(0..))
        | (LifeEventValueType::AgeYears, LifeEventValue::AgeYears(0..)) => Ok(()),
        (LifeEventValueType::Enum, LifeEventValue::Enum { schema_key, value })
            if definition.enum_schema_key.as_deref() == Some(schema_key.as_str()) =>
        {
            validate_enum(schema_key, value)
        }
        _ => Err(LifeEventError::InvalidEvidence),
    }
}

fn validate_enum(schema_key: &str, value: &str) -> Result<(), LifeEventError> {
    if schema_key == "military" && matches!(value, "unserved" | "serving" | "completed" | "exempt")
    {
        Ok(())
    } else {
        Err(LifeEventError::UnknownEnum)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EvaluatedValue {
    Known(LifeEventValue),
    Unknown(LifeEventUnknownReason),
}

fn evaluate_expression(
    expression: &LifeEventExpression,
    facts: &BTreeMap<&str, &LifeEventEvidenceValue>,
) -> Result<LifeEventTruth, LifeEventError> {
    match expression {
        LifeEventExpression::All { children } => {
            let mut unknown = None;
            for child in children {
                match evaluate_expression(child, facts)? {
                    LifeEventTruth::False => return Ok(LifeEventTruth::False),
                    LifeEventTruth::Unknown(reason) if unknown.is_none() => unknown = Some(reason),
                    LifeEventTruth::True | LifeEventTruth::Unknown(_) => {}
                }
            }
            Ok(unknown.map_or(LifeEventTruth::True, LifeEventTruth::Unknown))
        }
        LifeEventExpression::Any { children } => {
            let mut unknown = None;
            for child in children {
                match evaluate_expression(child, facts)? {
                    LifeEventTruth::True => return Ok(LifeEventTruth::True),
                    LifeEventTruth::Unknown(reason) if unknown.is_none() => unknown = Some(reason),
                    LifeEventTruth::False | LifeEventTruth::Unknown(_) => {}
                }
            }
            Ok(unknown.map_or(LifeEventTruth::False, LifeEventTruth::Unknown))
        }
        LifeEventExpression::Not { child } => match evaluate_expression(child, facts)? {
            LifeEventTruth::True => Ok(LifeEventTruth::False),
            LifeEventTruth::False => Ok(LifeEventTruth::True),
            LifeEventTruth::Unknown(reason) => Ok(LifeEventTruth::Unknown(reason)),
        },
        LifeEventExpression::Eq { left, right } => {
            evaluate_binary(left, right, facts, |left, right| Ok(left == right))
        }
        LifeEventExpression::Gte { left, right } => {
            evaluate_binary(left, right, facts, |left, right| {
                Ok(compare_known_values(left, right)? != Ordering::Less)
            })
        }
        LifeEventExpression::Between {
            value,
            lower,
            upper,
        } => {
            let value = evaluate_operand(value, facts)?;
            let lower = evaluate_operand(lower, facts)?;
            let upper = evaluate_operand(upper, facts)?;
            match (value, lower, upper) {
                (EvaluatedValue::Unknown(reason), _, _)
                | (_, EvaluatedValue::Unknown(reason), _)
                | (_, _, EvaluatedValue::Unknown(reason)) => Ok(LifeEventTruth::Unknown(reason)),
                (
                    EvaluatedValue::Known(value),
                    EvaluatedValue::Known(lower),
                    EvaluatedValue::Known(upper),
                ) => Ok(
                    if compare_known_values(&value, &lower)? != Ordering::Less
                        && compare_known_values(&value, &upper)? != Ordering::Greater
                    {
                        LifeEventTruth::True
                    } else {
                        LifeEventTruth::False
                    },
                ),
            }
        }
        LifeEventExpression::Fact { reference } => {
            let value = facts
                .get(reference.path.as_str())
                .ok_or(LifeEventError::InvalidEvidence)?;
            match value {
                LifeEventEvidenceValue::Known(LifeEventValue::Boolean(true)) => {
                    Ok(LifeEventTruth::True)
                }
                LifeEventEvidenceValue::Known(LifeEventValue::Boolean(false)) => {
                    Ok(LifeEventTruth::False)
                }
                LifeEventEvidenceValue::Unknown(reason) => Ok(LifeEventTruth::Unknown(*reason)),
                LifeEventEvidenceValue::Known(_) => Err(LifeEventError::TypeMismatch),
            }
        }
    }
}

fn evaluate_binary(
    left: &LifeEventOperand,
    right: &LifeEventOperand,
    facts: &BTreeMap<&str, &LifeEventEvidenceValue>,
    operation: impl FnOnce(&LifeEventValue, &LifeEventValue) -> Result<bool, LifeEventError>,
) -> Result<LifeEventTruth, LifeEventError> {
    match (
        evaluate_operand(left, facts)?,
        evaluate_operand(right, facts)?,
    ) {
        (EvaluatedValue::Unknown(reason), _) | (_, EvaluatedValue::Unknown(reason)) => {
            Ok(LifeEventTruth::Unknown(reason))
        }
        (EvaluatedValue::Known(left), EvaluatedValue::Known(right)) => {
            Ok(if operation(&left, &right)? {
                LifeEventTruth::True
            } else {
                LifeEventTruth::False
            })
        }
    }
}

fn evaluate_operand(
    operand: &LifeEventOperand,
    facts: &BTreeMap<&str, &LifeEventEvidenceValue>,
) -> Result<EvaluatedValue, LifeEventError> {
    match operand {
        LifeEventOperand::Fact { reference } => match facts
            .get(reference.path.as_str())
            .ok_or(LifeEventError::InvalidEvidence)?
        {
            LifeEventEvidenceValue::Known(value) => Ok(EvaluatedValue::Known(value.clone())),
            LifeEventEvidenceValue::Unknown(reason) => Ok(EvaluatedValue::Unknown(*reason)),
        },
        LifeEventOperand::Literal { value, .. } => Ok(EvaluatedValue::Known(match value {
            LifeEventLiteralValue::Boolean(value) => LifeEventValue::Boolean(*value),
            LifeEventLiteralValue::Count(value) => LifeEventValue::Count(*value),
            LifeEventLiteralValue::AgeYears(value) => LifeEventValue::AgeYears(*value),
            LifeEventLiteralValue::Enum { schema_key, value } => LifeEventValue::Enum {
                schema_key: schema_key.clone(),
                value: value.clone(),
            },
        })),
    }
}

fn compare_known_values(
    left: &LifeEventValue,
    right: &LifeEventValue,
) -> Result<Ordering, LifeEventError> {
    match (left, right) {
        (LifeEventValue::Count(left), LifeEventValue::Count(right))
        | (LifeEventValue::AgeYears(left), LifeEventValue::AgeYears(right)) => Ok(left.cmp(right)),
        _ => Err(LifeEventError::UnorderedType),
    }
}

fn canonical_entropy_message(input: LifeEventEntropyInput<'_>) -> Result<Vec<u8>, LifeEventError> {
    if !input.year_month.is_valid() {
        return Err(LifeEventError::InvalidYearMonth);
    }
    if !is_canonical_key(input.event_key)
        || !(1..=MAX_CANDIDATE_OCCURRENCE).contains(&input.occurrence_no)
    {
        return Err(LifeEventError::InvalidCanonicalKey);
    }
    let year_month = format!("{:04}-{:02}", input.year_month.year, input.year_month.month);
    let mut message = Vec::with_capacity(128);
    push_string(&mut message, ENTROPY_DOMAIN)?;
    message.extend_from_slice(&input.save_id.get().to_be_bytes());
    message.extend_from_slice(&input.run_revision.to_be_bytes());
    push_string(&mut message, &year_month)?;
    push_string(&mut message, input.event_key)?;
    message.extend_from_slice(&input.occurrence_no.to_be_bytes());
    push_string(&mut message, ENTROPY_STAGE)?;
    Ok(message)
}

fn push_string(output: &mut Vec<u8>, value: &str) -> Result<(), LifeEventError> {
    let length = u32::try_from(value.len()).map_err(|_| LifeEventError::ArithmeticOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(value.as_bytes());
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

fn scale_word_to_ppm(word: u64) -> u32 {
    ((u128::from(word) * u128::from(LIFE_EVENT_PROBABILITY_SCALE_PPM)) >> 64) as u32
}

#[derive(Debug)]
struct CandidateWork<'a> {
    definition: &'a LifeEventDefinition,
    plan: LifeEventCandidatePlan,
    selected: bool,
}

fn plan_month(
    rules: &V1LifeEventRules,
    input: LifeEventMonthPlanInput<'_>,
) -> Result<LifeEventMonthPlan, LifeEventError> {
    validate_catalog(input.catalog)?;
    if !input.year_month.is_valid() {
        return Err(LifeEventError::InvalidYearMonth);
    }
    if !is_lower_hex_64(input.eligibility_fact_fingerprint) {
        return Err(LifeEventError::InvalidFactFingerprint);
    }
    if input.existing_pending_count > LIFE_EVENT_MAX_PENDING {
        return Err(LifeEventError::PendingLimitExceeded);
    }
    let evidence = prepare_evidence(input.catalog, input.facts)?;
    let history = validate_occurrence_history(
        input.catalog,
        input.prior_occurrences,
        input.target_game_day,
    )?;
    let mut definitions = input.catalog.definitions.iter().collect::<Vec<_>>();
    definitions.sort_by(|left, right| left.event_key.cmp(&right.event_key));
    let mut candidates = Vec::with_capacity(definitions.len());

    for definition in definitions {
        let occurrences = history.get(&definition.id).map_or(&[][..], Vec::as_slice);
        let occurrence_no = u16::try_from(occurrences.len() + 1)
            .map_err(|_| LifeEventError::InvalidOccurrenceHistory)?;
        let eligibility = evaluate_expression(&definition.eligibility_ast.root, &evidence)?;
        let mut result = LifeEventCandidateResult::Ineligible;
        let mut unknown_reason = None;
        let mut roll_ppm = None;
        let mut selected = false;

        match eligibility {
            LifeEventTruth::False => {}
            LifeEventTruth::Unknown(reason) => {
                result = LifeEventCandidateResult::Indeterminate;
                unknown_reason = Some(reason);
            }
            LifeEventTruth::True => {
                let within_occurrence_limit = occurrence_no <= definition.maximum_occurrences;
                let outside_cooldown = match occurrences.last() {
                    Some(last) => {
                        last.offered_game_day
                            .checked_add(u32::from(definition.cooldown_game_days))
                            .ok_or(LifeEventError::ArithmeticOverflow)?
                            <= input.target_game_day
                    }
                    None => true,
                };
                if within_occurrence_limit && outside_cooldown {
                    let roll = rules.eligibility_roll_ppm(LifeEventEntropyInput {
                        world_seed: input.world_seed,
                        save_id: input.save_id,
                        run_revision: input.run_revision,
                        year_month: input.year_month,
                        event_key: &definition.event_key,
                        occurrence_no,
                    })?;
                    roll_ppm = Some(roll);
                    selected = roll < definition.hazard_ppm;
                    result = if selected {
                        LifeEventCandidateResult::Offered
                    } else {
                        LifeEventCandidateResult::NotSelected
                    };
                }
            }
        }
        candidates.push(CandidateWork {
            definition,
            plan: LifeEventCandidatePlan {
                candidate_order: definition.event_order,
                event_definition_id: definition.id,
                event_key: definition.event_key.clone(),
                occurrence_no,
                eligibility_fact_fingerprint: input.eligibility_fact_fingerprint.to_owned(),
                result,
                unknown_reason,
                roll_ppm,
            },
            selected,
        });
    }

    let mut selected_indices = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| candidate.selected.then_some(index))
        .collect::<Vec<_>>();
    selected_indices.sort_by(|left, right| {
        let left = candidates[*left].definition;
        let right = candidates[*right].definition;
        left.priority
            .cmp(&right.priority)
            .then_with(|| left.event_key.cmp(&right.event_key))
    });
    let mut claimed_groups = BTreeSet::new();
    for index in selected_indices {
        let group = candidates[index].definition.exclusive_group_key.as_deref();
        let offered = group.is_none_or(|group| claimed_groups.insert(group.to_owned()));
        candidates[index].plan.result = if offered {
            LifeEventCandidateResult::Offered
        } else {
            LifeEventCandidateResult::Suppressed
        };
    }

    let offered_count = candidates
        .iter()
        .filter(|candidate| candidate.plan.result == LifeEventCandidateResult::Offered)
        .count();
    let total_pending = input
        .existing_pending_count
        .checked_add(offered_count)
        .ok_or(LifeEventError::PendingLimitExceeded)?;
    if total_pending > LIFE_EVENT_MAX_PENDING {
        return Err(LifeEventError::PendingLimitExceeded);
    }

    let offers = candidates
        .iter()
        .filter(|candidate| candidate.plan.result == LifeEventCandidateResult::Offered)
        .map(|candidate| {
            let expires_game_day = input
                .target_game_day
                .checked_add(u32::from(candidate.definition.offer_duration_game_days))
                .ok_or(LifeEventError::ArithmeticOverflow)?;
            Ok(LifeEventOfferPlan {
                event_definition_id: candidate.definition.id,
                event_key: candidate.definition.event_key.clone(),
                occurrence_no: candidate.plan.occurrence_no,
                offered_game_day: input.target_game_day,
                expires_game_day,
            })
        })
        .collect::<Result<Vec<_>, LifeEventError>>()?;
    Ok(LifeEventMonthPlan {
        save_id: input.save_id,
        run_revision: input.run_revision,
        component_version_id: input.catalog.component_version_id,
        year_month: input.year_month,
        target_game_day: input.target_game_day,
        authority_state_revision: input.authority_state_revision,
        fact_registry_schema_version: input.catalog.fact_registry_schema_version,
        entropy_stream_version: LIFE_EVENT_ENTROPY_STREAM_VERSION,
        candidates: candidates
            .into_iter()
            .map(|candidate| candidate.plan)
            .collect(),
        offers,
    })
}

fn validate_occurrence_history<'a>(
    catalog: &LifeEventCatalog,
    occurrences: &'a [LifeEventOccurrence],
    target_game_day: u32,
) -> Result<BTreeMap<ResourceId, Vec<&'a LifeEventOccurrence>>, LifeEventError> {
    let definitions = catalog
        .definitions
        .iter()
        .map(|definition| (definition.id, definition))
        .collect::<BTreeMap<_, _>>();
    let mut grouped = BTreeMap::<ResourceId, Vec<_>>::new();
    for occurrence in occurrences {
        if !definitions.contains_key(&occurrence.event_definition_id)
            || occurrence.occurrence_no == 0
            || occurrence.offered_game_day > target_game_day
        {
            return Err(LifeEventError::InvalidOccurrenceHistory);
        }
        grouped
            .entry(occurrence.event_definition_id)
            .or_default()
            .push(occurrence);
    }
    for (definition_id, rows) in &mut grouped {
        rows.sort_by_key(|row| row.occurrence_no);
        let definition = definitions
            .get(definition_id)
            .ok_or(LifeEventError::InvalidOccurrenceHistory)?;
        if rows.len() > usize::from(definition.maximum_occurrences) {
            return Err(LifeEventError::InvalidOccurrenceHistory);
        }
        let mut prior_day = None;
        for (index, row) in rows.iter().enumerate() {
            let expected =
                u16::try_from(index + 1).map_err(|_| LifeEventError::InvalidOccurrenceHistory)?;
            if row.occurrence_no != expected
                || prior_day.is_some_and(|day| row.offered_game_day < day)
            {
                return Err(LifeEventError::InvalidOccurrenceHistory);
            }
            prior_day = Some(row.offered_game_day);
        }
    }
    Ok(grouped)
}

fn plan_effect(input: LifeEventEffectPlanInput<'_>) -> Result<LifeEventEffectPlan, LifeEventError> {
    match input.effect {
        LifeEventEffect::NoEffect => Ok(LifeEventEffectPlan {
            wallet_cash_before_krw: input.wallet_cash_krw,
            wallet_cash_after_krw: input.wallet_cash_krw,
            wallet_delta_krw: 0,
            postings: Vec::new(),
        }),
        LifeEventEffect::FixedWalletExpense {
            amount_krw,
            account_code: LifeEventEffectAccountCode::LifeEventExpense,
        } => {
            if !(1..=LIFE_EVENT_MAX_EFFECT_KRW).contains(amount_krw) {
                return Err(LifeEventError::InvalidEffect);
            }
            if input.wallet_cash_krw < 0 {
                return Err(LifeEventError::InvalidWalletCash);
            }
            if input.wallet_cash_krw < *amount_krw {
                return Err(LifeEventError::InsufficientWalletCash);
            }
            let wallet_delta_krw = amount_krw
                .checked_neg()
                .ok_or(LifeEventError::ArithmeticOverflow)?;
            let wallet_cash_after_krw = input
                .wallet_cash_krw
                .checked_add(wallet_delta_krw)
                .ok_or(LifeEventError::ArithmeticOverflow)?;
            let postings = vec![
                LifeEventLedgerPosting {
                    account_code: LifeEventLedgerAccountCode::LifeEventExpense,
                    amount_krw: *amount_krw,
                },
                LifeEventLedgerPosting {
                    account_code: LifeEventLedgerAccountCode::Wallet,
                    amount_krw: wallet_delta_krw,
                },
            ];
            let balance = postings.iter().try_fold(0_i64, |total, posting| {
                total
                    .checked_add(posting.amount_krw)
                    .ok_or(LifeEventError::ArithmeticOverflow)
            })?;
            if balance != 0 {
                return Err(LifeEventError::UnbalancedLedgerPlan);
            }
            Ok(LifeEventEffectPlan {
                wallet_cash_before_krw: input.wallet_cash_krw,
                wallet_cash_after_krw,
                wallet_delta_krw,
                postings,
            })
        }
    }
}

fn resolve_choice(
    input: LifeEventChoiceResolutionInput<'_>,
) -> Result<LifeEventResolutionPlan, LifeEventError> {
    validate_catalog(input.catalog)?;
    let definition = find_definition(input.catalog, input.event_definition_id)?;
    validate_offer_period(definition, input.offered_game_day, input.expires_game_day)?;
    if input.current_game_day < input.offered_game_day {
        return Err(LifeEventError::InvalidOfferPeriod);
    }
    if input.current_game_day >= input.expires_game_day {
        return Err(LifeEventError::EventExpired);
    }
    let choice = definition
        .choices
        .iter()
        .find(|choice| choice.id == input.choice_id)
        .ok_or(LifeEventError::ChoiceNotFound)?;
    let effect = plan_effect(LifeEventEffectPlanInput {
        effect: &choice.effect_ast.effect,
        wallet_cash_krw: input.wallet_cash_krw,
    })?;
    let resolution_kind = match choice.decision_kind {
        LifeEventDecisionKind::Accepted => LifeEventResolutionKind::Accepted,
        LifeEventDecisionKind::Declined => LifeEventResolutionKind::Declined,
    };
    Ok(LifeEventResolutionPlan {
        event_instance_id: input.event_instance_id,
        choice_id: choice.id,
        resolution_kind,
        resolved_game_day: input.current_game_day,
        effect,
    })
}

fn resolve_expired(
    input: LifeEventExpiryResolutionInput<'_>,
) -> Result<LifeEventResolutionPlan, LifeEventError> {
    validate_catalog(input.catalog)?;
    let definition = find_definition(input.catalog, input.event_definition_id)?;
    validate_offer_period(definition, input.offered_game_day, input.expires_game_day)?;
    if input.current_game_day < input.expires_game_day {
        return Err(LifeEventError::EventNotExpired);
    }
    if input.current_game_day != input.expires_game_day {
        return Err(LifeEventError::InvalidOfferPeriod);
    }
    let choice = definition
        .choices
        .iter()
        .find(|choice| choice.choice_key == definition.default_choice_key)
        .ok_or(LifeEventError::InvalidDefaultChoice)?;
    if !matches!(choice.effect_ast.effect, LifeEventEffect::NoEffect) {
        return Err(LifeEventError::InvalidDefaultChoice);
    }
    let effect = plan_effect(LifeEventEffectPlanInput {
        effect: &choice.effect_ast.effect,
        wallet_cash_krw: input.wallet_cash_krw,
    })?;
    Ok(LifeEventResolutionPlan {
        event_instance_id: input.event_instance_id,
        choice_id: choice.id,
        resolution_kind: LifeEventResolutionKind::Expired,
        resolved_game_day: input.current_game_day,
        effect,
    })
}

fn validate_offer_period(
    definition: &LifeEventDefinition,
    offered_game_day: u32,
    expires_game_day: u32,
) -> Result<(), LifeEventError> {
    let expected_expiry = offered_game_day
        .checked_add(u32::from(definition.offer_duration_game_days))
        .ok_or(LifeEventError::ArithmeticOverflow)?;
    if expires_game_day == expected_expiry {
        Ok(())
    } else {
        Err(LifeEventError::InvalidOfferPeriod)
    }
}

fn find_definition(
    catalog: &LifeEventCatalog,
    definition_id: ResourceId,
) -> Result<&LifeEventDefinition, LifeEventError> {
    catalog
        .definitions
        .iter()
        .find(|definition| definition.id == definition_id)
        .ok_or(LifeEventError::EventNotFound)
}

fn is_canonical_key(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && bytes[0].is_ascii_lowercase()
        && bytes[1..].iter().all(u8::is_ascii_alphanumeric)
}

fn is_component_version_key(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 96
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn is_display_name(value: &str, max_chars: usize) -> bool {
    let count = value.chars().count();
    count != 0 && count <= max_chars
}

fn is_lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_YEAR_MONTH: YearMonth = YearMonth {
        year: 2026,
        month: 7,
    };

    struct FixedWordEntropy(u64);

    impl LifeEventEntropy for FixedWordEntropy {
        fn digest(
            &self,
            _world_seed: u64,
            _canonical_message: &[u8],
        ) -> Result<[u8; 32], LifeEventError> {
            let mut digest = [0_u8; 32];
            digest[..8].copy_from_slice(&self.0.to_be_bytes());
            Ok(digest)
        }
    }

    fn given_rules_with_word(word: u64) -> Arc<dyn LifeEventRules> {
        create_life_event_rules_with_entropy(Arc::new(FixedWordEntropy(word)))
    }

    fn given_fact_reference(path: &str, unit: LifeEventUnit) -> LifeEventFactReference {
        LifeEventFactReference {
            path: path.to_owned(),
            unit,
            window: LifeEventWindowKind::CurrentGameDay,
        }
    }

    fn given_fact_operand(path: &str, unit: LifeEventUnit) -> Box<LifeEventOperand> {
        Box::new(LifeEventOperand::Fact {
            reference: given_fact_reference(path, unit),
        })
    }

    fn given_literal(unit: LifeEventUnit, value: LifeEventLiteralValue) -> Box<LifeEventOperand> {
        Box::new(LifeEventOperand::Literal { unit, value })
    }

    fn given_eligibility_ast() -> LifeEventEligibilityAst {
        LifeEventEligibilityAst {
            version: LIFE_EVENT_SCHEMA_VERSION,
            root: LifeEventExpression::All {
                children: vec![
                    LifeEventExpression::Between {
                        value: given_fact_operand("character.age", LifeEventUnit::Years),
                        lower: given_literal(
                            LifeEventUnit::Years,
                            LifeEventLiteralValue::AgeYears(22),
                        ),
                        upper: given_literal(
                            LifeEventUnit::Years,
                            LifeEventLiteralValue::AgeYears(67),
                        ),
                    },
                    LifeEventExpression::Gte {
                        left: given_fact_operand("household.dependentCount", LifeEventUnit::Count),
                        right: given_literal(LifeEventUnit::Count, LifeEventLiteralValue::Count(1)),
                    },
                    LifeEventExpression::Fact {
                        reference: given_fact_reference("residence.exists", LifeEventUnit::Boolean),
                    },
                    LifeEventExpression::Not {
                        child: Box::new(LifeEventExpression::Eq {
                            left: given_fact_operand("military.status", LifeEventUnit::Enum),
                            right: given_literal(
                                LifeEventUnit::Enum,
                                LifeEventLiteralValue::Enum {
                                    schema_key: "military".to_owned(),
                                    value: "serving".to_owned(),
                                },
                            ),
                        }),
                    },
                ],
            },
        }
    }

    fn given_definition(id: u64, event_key: &str) -> LifeEventDefinition {
        LifeEventDefinition {
            id: ResourceId::from_u64(id),
            schema_version: LIFE_EVENT_SCHEMA_VERSION,
            entropy_stream_version: LIFE_EVENT_ENTROPY_STREAM_VERSION,
            event_order: 1,
            event_key: event_key.to_owned(),
            display_name: "가족 돌봄 요청".to_owned(),
            purpose: LifeEventPurpose::GameBalance,
            ranked_availability: LifeEventRankedAvailability::UnrankedOnly,
            eligibility_ast: given_eligibility_ast(),
            ast_node_count: 13,
            ast_max_depth: 4,
            hazard_ppm: LIFE_EVENT_PROBABILITY_SCALE_PPM,
            cooldown_game_days: 365,
            maximum_occurrences: 1,
            priority: 100,
            exclusive_group_key: Some("familyCare".to_owned()),
            offer_duration_game_days: 7,
            default_choice_key: "decline".to_owned(),
            choices: vec![
                LifeEventChoiceDefinition {
                    id: ResourceId::from_u64(id + 1),
                    choice_order: 1,
                    choice_key: "supportNow".to_owned(),
                    display_name: "지금 돕는다".to_owned(),
                    decision_kind: LifeEventDecisionKind::Accepted,
                    effect_kind: LifeEventEffectKind::FixedWalletExpense,
                    effect_amount_krw: Some(120_000),
                    effect_account_code: Some(LifeEventEffectAccountCode::LifeEventExpense),
                    effect_ast: LifeEventEffectAst {
                        version: LIFE_EVENT_SCHEMA_VERSION,
                        effect: LifeEventEffect::FixedWalletExpense {
                            amount_krw: 120_000,
                            account_code: LifeEventEffectAccountCode::LifeEventExpense,
                        },
                    },
                },
                LifeEventChoiceDefinition {
                    id: ResourceId::from_u64(id + 2),
                    choice_order: 2,
                    choice_key: "decline".to_owned(),
                    display_name: "이번에는 돕지 않는다".to_owned(),
                    decision_kind: LifeEventDecisionKind::Declined,
                    effect_kind: LifeEventEffectKind::NoEffect,
                    effect_amount_krw: None,
                    effect_account_code: None,
                    effect_ast: LifeEventEffectAst {
                        version: LIFE_EVENT_SCHEMA_VERSION,
                        effect: LifeEventEffect::NoEffect,
                    },
                },
            ],
        }
    }

    fn given_catalog_with_definitions(
        mut definitions: Vec<LifeEventDefinition>,
    ) -> LifeEventCatalog {
        definitions.sort_by(|left, right| left.event_key.cmp(&right.event_key));
        for (index, definition) in definitions.iter_mut().enumerate() {
            definition.event_order =
                u8::try_from(index + 1).expect("정의 순서를 표현할 수 있어야 한다");
        }
        LifeEventCatalog {
            component_version_id: ResourceId::from_u64(10),
            component_version_key: "dev-unranked-m4-life-event-2026-v1".to_owned(),
            fact_registry_schema_version: LIFE_EVENT_FACT_REGISTRY_SCHEMA_VERSION,
            facts: vec![
                LifeEventFactDefinition {
                    id: ResourceId::from_u64(1),
                    fact_order: 1,
                    fact_key: "character.age".to_owned(),
                    value_type: LifeEventValueType::AgeYears,
                    unit: LifeEventUnit::Years,
                    enum_schema_key: None,
                    window_kind: LifeEventWindowKind::CurrentGameDay,
                    source_schema_version: LIFE_EVENT_FACT_REGISTRY_SCHEMA_VERSION,
                    source_kind: LifeEventFactSourceKind::GameDay,
                },
                LifeEventFactDefinition {
                    id: ResourceId::from_u64(2),
                    fact_order: 2,
                    fact_key: "household.dependentCount".to_owned(),
                    value_type: LifeEventValueType::Count,
                    unit: LifeEventUnit::Count,
                    enum_schema_key: None,
                    window_kind: LifeEventWindowKind::CurrentGameDay,
                    source_schema_version: LIFE_EVENT_FACT_REGISTRY_SCHEMA_VERSION,
                    source_kind: LifeEventFactSourceKind::Household,
                },
                LifeEventFactDefinition {
                    id: ResourceId::from_u64(3),
                    fact_order: 3,
                    fact_key: "residence.exists".to_owned(),
                    value_type: LifeEventValueType::Boolean,
                    unit: LifeEventUnit::Boolean,
                    enum_schema_key: None,
                    window_kind: LifeEventWindowKind::CurrentGameDay,
                    source_schema_version: LIFE_EVENT_FACT_REGISTRY_SCHEMA_VERSION,
                    source_kind: LifeEventFactSourceKind::Residence,
                },
                LifeEventFactDefinition {
                    id: ResourceId::from_u64(4),
                    fact_order: 4,
                    fact_key: "military.status".to_owned(),
                    value_type: LifeEventValueType::Enum,
                    unit: LifeEventUnit::Enum,
                    enum_schema_key: Some("military".to_owned()),
                    window_kind: LifeEventWindowKind::CurrentGameDay,
                    source_schema_version: LIFE_EVENT_FACT_REGISTRY_SCHEMA_VERSION,
                    source_kind: LifeEventFactSourceKind::Military,
                },
            ],
            definitions,
        }
    }

    fn given_catalog() -> LifeEventCatalog {
        given_catalog_with_definitions(vec![given_definition(100, "fictionalDependentCareRequest")])
    }

    fn given_eligible_facts() -> Vec<LifeEventFactEvidence> {
        vec![
            LifeEventFactEvidence {
                fact_key: "character.age".to_owned(),
                value: LifeEventEvidenceValue::Known(LifeEventValue::AgeYears(30)),
            },
            LifeEventFactEvidence {
                fact_key: "household.dependentCount".to_owned(),
                value: LifeEventEvidenceValue::Known(LifeEventValue::Count(1)),
            },
            LifeEventFactEvidence {
                fact_key: "residence.exists".to_owned(),
                value: LifeEventEvidenceValue::Known(LifeEventValue::Boolean(true)),
            },
            LifeEventFactEvidence {
                fact_key: "military.status".to_owned(),
                value: LifeEventEvidenceValue::Known(LifeEventValue::Enum {
                    schema_key: "military".to_owned(),
                    value: "completed".to_owned(),
                }),
            },
        ]
    }

    fn given_fact_value(
        facts: &mut [LifeEventFactEvidence],
        fact_key: &str,
        value: LifeEventEvidenceValue,
    ) {
        facts
            .iter_mut()
            .find(|fact| fact.fact_key == fact_key)
            .expect("사실 fixture가 있어야 한다")
            .value = value;
    }

    fn when_month_is_planned(
        rules: &Arc<dyn LifeEventRules>,
        catalog: &LifeEventCatalog,
        facts: &[LifeEventFactEvidence],
        occurrences: &[LifeEventOccurrence],
        existing_pending_count: usize,
        target_game_day: u32,
    ) -> Result<LifeEventMonthPlan, LifeEventError> {
        let fingerprint = "a".repeat(64);
        rules.plan_month(LifeEventMonthPlanInput {
            catalog,
            world_seed: 0x0102_0304_0506_0708,
            save_id: ResourceId::from_u64(42),
            run_revision: 3,
            year_month: TEST_YEAR_MONTH,
            target_game_day,
            authority_state_revision: 17,
            eligibility_fact_fingerprint: &fingerprint,
            facts,
            prior_occurrences: occurrences,
            existing_pending_count,
        })
    }

    fn lowercase_hex(bytes: [u8; 32]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut result = String::with_capacity(64);
        for byte in bytes {
            result.push(char::from(HEX[usize::from(byte >> 4)]));
            result.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        result
    }

    mod context_봉인된_카탈로그를_검증하는_경우 {
        use super::*;

        #[test]
        fn given_0039_fixture_when_검증하면_then_schema_v1_계약을_통과한다() {
            let catalog = given_catalog();

            let result = create_life_event_rules().validate_catalog(&catalog);

            assert_eq!(result, Ok(()));
        }

        #[test]
        fn given_ast_projection이_다른_정의_when_검증하면_then_fail_closed한다() {
            let mut catalog = given_catalog();
            catalog.definitions[0].ast_node_count = 12;

            let result = create_life_event_rules().validate_catalog(&catalog);

            assert_eq!(result, Err(LifeEventError::AstProjectionMismatch));
        }

        #[test]
        fn given_effect가_있는_default_choice_when_검증하면_then_게시를_거절한다() {
            let mut catalog = given_catalog();
            let choice = &mut catalog.definitions[0].choices[1];
            choice.effect_kind = LifeEventEffectKind::FixedWalletExpense;
            choice.effect_amount_krw = Some(1);
            choice.effect_account_code = Some(LifeEventEffectAccountCode::LifeEventExpense);
            choice.effect_ast.effect = LifeEventEffect::FixedWalletExpense {
                amount_krw: 1,
                account_code: LifeEventEffectAccountCode::LifeEventExpense,
            };

            let result = create_life_event_rules().validate_catalog(&catalog);

            assert_eq!(result, Err(LifeEventError::InvalidDefaultChoice));
        }

        #[test]
        fn given_허용되지_않은_fact_registry_when_검증하면_then_fail_closed한다() {
            let mut catalog = given_catalog();
            catalog.facts[3].enum_schema_key = Some("unknown".to_owned());

            let result = create_life_event_rules().validate_catalog(&catalog);

            assert_eq!(result, Err(LifeEventError::InvalidFactRegistry));
        }

        #[test]
        fn given_알려지지_않은_enum_literal_when_검증하면_then_fail_closed한다() {
            let mut catalog = given_catalog();
            let LifeEventExpression::All { children } =
                &mut catalog.definitions[0].eligibility_ast.root
            else {
                panic!("fixture root는 all이어야 한다");
            };
            let LifeEventExpression::Not { child } = &mut children[3] else {
                panic!("fixture의 네 번째 node는 not이어야 한다");
            };
            let LifeEventExpression::Eq { right, .. } = child.as_mut() else {
                panic!("fixture의 not child는 eq여야 한다");
            };
            let LifeEventOperand::Literal { value, .. } = right.as_mut() else {
                panic!("fixture의 eq right는 literal이어야 한다");
            };
            let LifeEventLiteralValue::Enum { value, .. } = value else {
                panic!("fixture literal은 enum이어야 한다");
            };
            *value = "unknown".to_owned();

            let result = create_life_event_rules().validate_catalog(&catalog);

            assert_eq!(result, Err(LifeEventError::UnknownEnum));
        }
    }

    mod context_월별_entropy를_계산하는_경우 {
        use super::*;

        #[test]
        fn given_고정된_seed와_context_when_hmac을_계산하면_then_golden_vector와_같다() {
            let input = LifeEventEntropyInput {
                world_seed: 0x0102_0304_0506_0708,
                save_id: ResourceId::from_u64(42),
                run_revision: 3,
                year_month: TEST_YEAR_MONTH,
                event_key: "fictionalDependentCareRequest",
                occurrence_no: 1,
            };

            let digest = create_life_event_rules()
                .eligibility_digest(input)
                .expect("digest를 계산해야 한다");
            let roll = create_life_event_rules()
                .eligibility_roll_ppm(input)
                .expect("roll을 계산해야 한다");

            assert_eq!(
                lowercase_hex(digest),
                "3d4afc815bdbd96e174261f42dca40d2048c7bfb22bf71d087e8195e778d3494"
            );
            assert_eq!(roll, 239_425);
        }

        #[test]
        fn given_roll_0과_hazard_0_1_when_계획하면_then_strict_less_than_경계를_지킨다() {
            let mut zero = given_definition(100, "hazardZero");
            zero.hazard_ppm = 0;
            zero.exclusive_group_key = None;
            let mut one = given_definition(200, "hazardOne");
            one.hazard_ppm = 1;
            one.exclusive_group_key = None;
            let catalog = given_catalog_with_definitions(vec![zero, one]);

            let plan = when_month_is_planned(
                &given_rules_with_word(0),
                &catalog,
                &given_eligible_facts(),
                &[],
                0,
                100,
            )
            .expect("월 계획을 만들어야 한다");

            assert_eq!(plan.candidates[0].result, LifeEventCandidateResult::Offered);
            assert_eq!(
                plan.candidates[1].result,
                LifeEventCandidateResult::NotSelected
            );
        }

        #[test]
        fn given_roll_999999와_hazard_999999_1000000_when_계획하면_then_ppm_상한을_지킨다() {
            let mut full = given_definition(100, "hazardFull");
            full.hazard_ppm = LIFE_EVENT_PROBABILITY_SCALE_PPM;
            full.exclusive_group_key = None;
            let mut last = given_definition(200, "hazardLast");
            last.hazard_ppm = 999_999;
            last.exclusive_group_key = None;
            let catalog = given_catalog_with_definitions(vec![full, last]);

            let plan = when_month_is_planned(
                &given_rules_with_word(u64::MAX),
                &catalog,
                &given_eligible_facts(),
                &[],
                0,
                100,
            )
            .expect("월 계획을 만들어야 한다");

            assert_eq!(plan.candidates[0].roll_ppm, Some(999_999));
            assert_eq!(plan.candidates[0].result, LifeEventCandidateResult::Offered);
            assert_eq!(
                plan.candidates[1].result,
                LifeEventCandidateResult::NotSelected
            );
        }

        #[test]
        fn given_같은_event와_추가된_앞선_definition_when_계획하면_then_기존_roll은_같다() {
            let mut middle = given_definition(100, "middleEvent");
            middle.hazard_ppm = 0;
            middle.exclusive_group_key = None;
            let base = given_catalog_with_definitions(vec![middle.clone()]);
            let mut alpha = given_definition(200, "alphaEvent");
            alpha.hazard_ppm = 0;
            alpha.exclusive_group_key = None;
            let expanded = given_catalog_with_definitions(vec![middle, alpha]);
            let rules = create_life_event_rules();
            let facts = given_eligible_facts();

            let base_plan = when_month_is_planned(&rules, &base, &facts, &[], 0, 100)
                .expect("기본 월 계획을 만들어야 한다");
            let expanded_plan = when_month_is_planned(&rules, &expanded, &facts, &[], 0, 100)
                .expect("확장 월 계획을 만들어야 한다");

            let expanded_middle = expanded_plan
                .candidates
                .iter()
                .find(|candidate| candidate.event_key == "middleEvent")
                .expect("기존 후보가 있어야 한다");
            assert_eq!(base_plan.candidates[0].roll_ppm, expanded_middle.roll_ppm);
        }
    }

    mod context_eligibility를_평가하는_경우 {
        use super::*;

        #[test]
        fn given_authority가_모르는_사실_when_평가하면_then_indeterminate를_보존한다() {
            let catalog = given_catalog();
            let mut facts = given_eligible_facts();
            given_fact_value(
                &mut facts,
                "character.age",
                LifeEventEvidenceValue::Unknown(LifeEventUnknownReason::AuthorityMissing),
            );

            let plan =
                when_month_is_planned(&given_rules_with_word(0), &catalog, &facts, &[], 0, 100)
                    .expect("indeterminate 후보를 기록해야 한다");

            assert_eq!(
                plan.candidates[0].unknown_reason,
                Some(LifeEventUnknownReason::AuthorityMissing)
            );
            assert_eq!(
                plan.candidates[0].result,
                LifeEventCandidateResult::Indeterminate
            );
            assert_eq!(plan.candidates[0].roll_ppm, None);
        }

        #[test]
        fn given_unknown과_false가_함께_있는_all_when_평가하면_then_false가_우선한다() {
            let catalog = given_catalog();
            let mut facts = given_eligible_facts();
            given_fact_value(
                &mut facts,
                "character.age",
                LifeEventEvidenceValue::Unknown(LifeEventUnknownReason::AuthorityMissing),
            );
            given_fact_value(
                &mut facts,
                "residence.exists",
                LifeEventEvidenceValue::Known(LifeEventValue::Boolean(false)),
            );

            let result = create_life_event_rules()
                .evaluate_eligibility(LifeEventEligibilityInput {
                    catalog: &catalog,
                    event_definition_id: catalog.definitions[0].id,
                    facts: &facts,
                })
                .expect("Kleene truth를 평가해야 한다");

            assert_eq!(result, LifeEventTruth::False);
        }
    }

    mod context_발생_횟수와_cooldown을_적용하는_경우 {
        use super::*;

        #[test]
        fn given_maximum_occurrence를_소비한_event_when_계획하면_then_roll_없이_ineligible이다() {
            let catalog = given_catalog();
            let occurrence = LifeEventOccurrence {
                event_definition_id: catalog.definitions[0].id,
                occurrence_no: 1,
                offered_game_day: 10,
            };

            let plan = when_month_is_planned(
                &given_rules_with_word(0),
                &catalog,
                &given_eligible_facts(),
                &[occurrence],
                0,
                500,
            )
            .expect("ineligible 후보를 기록해야 한다");

            assert_eq!(plan.candidates[0].occurrence_no, 2);
            assert_eq!(
                plan.candidates[0].result,
                LifeEventCandidateResult::Ineligible
            );
            assert_eq!(plan.candidates[0].roll_ppm, None);
        }

        #[test]
        fn given_cooldown_마지막_날_when_계획하면_then_다음_날부터_다시_선택한다() {
            let mut definition = given_definition(100, "repeatableEvent");
            definition.maximum_occurrences = 3;
            let catalog = given_catalog_with_definitions(vec![definition]);
            let occurrence = LifeEventOccurrence {
                event_definition_id: catalog.definitions[0].id,
                occurrence_no: 1,
                offered_game_day: 100,
            };
            let rules = given_rules_with_word(0);
            let facts = given_eligible_facts();

            let before = when_month_is_planned(&rules, &catalog, &facts, &[occurrence], 0, 464)
                .expect("cooldown 중 후보를 기록해야 한다");
            let boundary = when_month_is_planned(&rules, &catalog, &facts, &[occurrence], 0, 465)
                .expect("cooldown 경계 후보를 기록해야 한다");

            assert_eq!(
                before.candidates[0].result,
                LifeEventCandidateResult::Ineligible
            );
            assert_eq!(
                boundary.candidates[0].result,
                LifeEventCandidateResult::Offered
            );
        }

        #[test]
        fn given_game_day를_넘는_cooldown_when_계획하면_then_checked_overflow_error다() {
            let mut definition = given_definition(100, "repeatableEvent");
            definition.maximum_occurrences = 3;
            let catalog = given_catalog_with_definitions(vec![definition]);
            let occurrence = LifeEventOccurrence {
                event_definition_id: catalog.definitions[0].id,
                occurrence_no: 1,
                offered_game_day: u32::MAX - 100,
            };

            let result = when_month_is_planned(
                &given_rules_with_word(0),
                &catalog,
                &given_eligible_facts(),
                &[occurrence],
                0,
                u32::MAX,
            );

            assert_eq!(result, Err(LifeEventError::ArithmeticOverflow));
        }
    }

    mod context_selected_후보를_조정하는_경우 {
        use super::*;

        #[test]
        fn given_같은_exclusive_group의_두_event_when_계획하면_then_priority가_앞선_하나만_offered이다()
         {
            let mut alpha = given_definition(100, "alphaEvent");
            alpha.priority = 200;
            let mut beta = given_definition(200, "betaEvent");
            beta.priority = 100;
            let catalog = given_catalog_with_definitions(vec![alpha, beta]);

            let plan = when_month_is_planned(
                &given_rules_with_word(0),
                &catalog,
                &given_eligible_facts(),
                &[],
                0,
                100,
            )
            .expect("exclusive 후보를 조정해야 한다");

            assert_eq!(
                plan.candidates[0].result,
                LifeEventCandidateResult::Suppressed
            );
            assert_eq!(plan.candidates[1].result, LifeEventCandidateResult::Offered);
            assert_eq!(plan.offers.len(), 1);
            assert_eq!(plan.offers[0].event_key, "betaEvent");
        }

        #[test]
        fn given_기존_pending_8건과_새_승자_when_계획하면_then_자르지_않고_invariant_error다() {
            let catalog = given_catalog();

            let result = when_month_is_planned(
                &given_rules_with_word(0),
                &catalog,
                &given_eligible_facts(),
                &[],
                LIFE_EVENT_MAX_PENDING,
                100,
            );

            assert_eq!(result, Err(LifeEventError::PendingLimitExceeded));
        }

        #[test]
        fn given_같은_priority와_exclusive_group_when_계획하면_then_event_key가_앞선_후보가_승리한다()
         {
            let alpha = given_definition(100, "alphaEvent");
            let beta = given_definition(200, "betaEvent");
            let catalog = given_catalog_with_definitions(vec![beta, alpha]);

            let plan = when_month_is_planned(
                &given_rules_with_word(0),
                &catalog,
                &given_eligible_facts(),
                &[],
                0,
                100,
            )
            .expect("event key tie-break를 적용해야 한다");

            assert_eq!(plan.candidates[0].result, LifeEventCandidateResult::Offered);
            assert_eq!(
                plan.candidates[1].result,
                LifeEventCandidateResult::Suppressed
            );
        }
    }

    mod context_offer를_해결하는_경우 {
        use super::*;

        #[test]
        fn given_expiry_직전의_support_choice_when_해결하면_then_accepted_effect를_계획한다() {
            let catalog = given_catalog();
            let definition = &catalog.definitions[0];

            let plan = create_life_event_rules()
                .resolve_choice(LifeEventChoiceResolutionInput {
                    catalog: &catalog,
                    event_definition_id: definition.id,
                    event_instance_id: ResourceId::from_u64(900),
                    offered_game_day: 100,
                    expires_game_day: 107,
                    choice_id: definition.choices[0].id,
                    current_game_day: 106,
                    wallet_cash_krw: 120_000,
                })
                .expect("exclusive expiry 전에는 선택할 수 있어야 한다");

            assert_eq!(plan.resolution_kind, LifeEventResolutionKind::Accepted);
            assert_eq!(plan.effect.wallet_delta_krw, -120_000);
            assert_eq!(plan.effect.postings.len(), 2);
        }

        #[test]
        fn given_expiry와_같은_날의_explicit_choice_when_해결하면_then_event_expired다() {
            let catalog = given_catalog();
            let definition = &catalog.definitions[0];

            let result = create_life_event_rules().resolve_choice(LifeEventChoiceResolutionInput {
                catalog: &catalog,
                event_definition_id: definition.id,
                event_instance_id: ResourceId::from_u64(900),
                offered_game_day: 100,
                expires_game_day: 107,
                choice_id: definition.choices[0].id,
                current_game_day: 107,
                wallet_cash_krw: 120_000,
            });

            assert_eq!(result, Err(LifeEventError::EventExpired));
        }

        #[test]
        fn given_expiry_이전의_offer_when_자동_해결하면_then_아직_만료되지_않았다() {
            let catalog = given_catalog();
            let definition = &catalog.definitions[0];

            let result =
                create_life_event_rules().resolve_expired(LifeEventExpiryResolutionInput {
                    catalog: &catalog,
                    event_definition_id: definition.id,
                    event_instance_id: ResourceId::from_u64(900),
                    offered_game_day: 100,
                    expires_game_day: 107,
                    current_game_day: 106,
                    wallet_cash_krw: 0,
                });

            assert_eq!(result, Err(LifeEventError::EventNotExpired));
        }

        #[test]
        fn given_expiry_당일의_offer_when_자동_해결하면_then_default_no_effect로_expired다() {
            let catalog = given_catalog();
            let definition = &catalog.definitions[0];

            let plan = create_life_event_rules()
                .resolve_expired(LifeEventExpiryResolutionInput {
                    catalog: &catalog,
                    event_definition_id: definition.id,
                    event_instance_id: ResourceId::from_u64(900),
                    offered_game_day: 100,
                    expires_game_day: 107,
                    current_game_day: 107,
                    wallet_cash_krw: i64::MIN,
                })
                .expect("default no-effect는 잔액과 무관하게 만료되어야 한다");

            assert_eq!(plan.resolution_kind, LifeEventResolutionKind::Expired);
            assert_eq!(plan.choice_id, definition.choices[1].id);
            assert!(plan.effect.postings.is_empty());
            assert_eq!(plan.effect.wallet_delta_krw, 0);
        }
    }

    mod context_effect_plan을_만드는_경우 {
        use super::*;

        #[test]
        fn given_충분한_wallet의_fixed_expense_when_계획하면_then_balanced_posting을_만든다() {
            let effect = LifeEventEffect::FixedWalletExpense {
                amount_krw: 120_000,
                account_code: LifeEventEffectAccountCode::LifeEventExpense,
            };

            let plan = create_life_event_rules()
                .plan_effect(LifeEventEffectPlanInput {
                    effect: &effect,
                    wallet_cash_krw: 200_000,
                })
                .expect("비용 effect를 계획해야 한다");

            assert_eq!(plan.wallet_cash_after_krw, 80_000);
            assert_eq!(plan.wallet_delta_krw, -120_000);
            assert_eq!(
                plan.postings,
                vec![
                    LifeEventLedgerPosting {
                        account_code: LifeEventLedgerAccountCode::LifeEventExpense,
                        amount_krw: 120_000,
                    },
                    LifeEventLedgerPosting {
                        account_code: LifeEventLedgerAccountCode::Wallet,
                        amount_krw: -120_000,
                    },
                ]
            );
            assert_eq!(
                plan.postings
                    .iter()
                    .map(|posting| posting.amount_krw)
                    .sum::<i64>(),
                0
            );
        }

        #[test]
        fn given_부족한_wallet의_fixed_expense_when_계획하면_then_insufficient_error다() {
            let effect = LifeEventEffect::FixedWalletExpense {
                amount_krw: 120_000,
                account_code: LifeEventEffectAccountCode::LifeEventExpense,
            };

            let result = create_life_event_rules().plan_effect(LifeEventEffectPlanInput {
                effect: &effect,
                wallet_cash_krw: 119_999,
            });

            assert_eq!(result, Err(LifeEventError::InsufficientWalletCash));
        }

        #[test]
        fn given_최대_허용_정수의_fixed_expense_when_계획하면_then_checked_math로_balance를_보존한다()
         {
            let effect = LifeEventEffect::FixedWalletExpense {
                amount_krw: LIFE_EVENT_MAX_EFFECT_KRW,
                account_code: LifeEventEffectAccountCode::LifeEventExpense,
            };

            let plan = create_life_event_rules()
                .plan_effect(LifeEventEffectPlanInput {
                    effect: &effect,
                    wallet_cash_krw: i64::MAX,
                })
                .expect("최대 허용 비용을 안전하게 계산해야 한다");

            assert_eq!(
                plan.wallet_cash_after_krw,
                i64::MAX - LIFE_EVENT_MAX_EFFECT_KRW
            );
            assert_eq!(
                plan.postings
                    .iter()
                    .map(|posting| posting.amount_krw)
                    .sum::<i64>(),
                0
            );
        }
    }
}
