use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

use crate::finance::ResourceId;

use super::types::{
    WELFARE_MAX_AST_DEPTH, WELFARE_MAX_COLLECTION_ROWS, WELFARE_MAX_CONDITIONS,
    WELFARE_MAX_CONSTANTS, WELFARE_MAX_IN_LITERALS, WELFARE_MAX_LOGICAL_CHILDREN,
    WELFARE_MAX_PREVIOUS_CLOSED_DAYS, WELFARE_MAX_PROGRAM_NODES, WELFARE_MAX_PUBLIC_FACTS,
    WELFARE_MAX_STRING_SCALARS, WELFARE_SCHEMA_VERSION, WelfareBenefitDefinition,
    WelfareCollectionDefinition, WelfareCollectionEvidence, WelfareCollectionEvidenceValue,
    WelfareConditionResult, WelfareEligibilityExpression, WelfareEnumDefinition, WelfareEnumValue,
    WelfareError, WelfareEvaluation, WelfareEvaluationInput, WelfareEvaluationStatus,
    WelfareEvidenceValue, WelfareExpression, WelfareFactDefinition, WelfareFactEvidence,
    WelfareFactRegistry, WelfareFactSource, WelfareFingerprintInput, WelfarePeriodPin,
    WelfareProgramCondition, WelfareProgramConstant, WelfareProgramDefinition,
    WelfareProgramPurpose, WelfareRankedAvailability, WelfareResolvedWindow, WelfareRules,
    WelfareTruth, WelfareUnknownReason, WelfareValue, WelfareValueType, WelfareWindowConstraint,
    WelfareWindowDays, WelfareWindowSpec,
};

#[derive(Debug)]
struct V1WelfareRules {
    registry: WelfareFactRegistry,
}

pub fn create_welfare_rules() -> Arc<dyn WelfareRules> {
    Arc::new(V1WelfareRules {
        registry: create_fact_registry(),
    })
}

pub fn create_fictional_restart_grant_program() -> WelfareProgramDefinition {
    let current_day_fact = |path: &str| WelfareExpression::Fact {
        path: path.to_owned(),
        window: WelfareWindowSpec::CurrentDay,
    };
    let constant = |key: &str| WelfareExpression::Constant {
        key: key.to_owned(),
    };
    let enum_literal = |schema_key: &str, value: &str| {
        WelfareValue::Enum(WelfareEnumValue {
            schema_key: schema_key.to_owned(),
            value: value.to_owned(),
        })
    };

    WelfareProgramDefinition {
        schema_version: WELFARE_SCHEMA_VERSION,
        program_version_id: ResourceId::from_u64(1),
        program_key: "fictionalRestartGrant".to_owned(),
        purpose: WelfareProgramPurpose::GameBalance,
        ranked_availability: WelfareRankedAvailability::UnrankedOnly,
        duplicate_group_key: "fictionalRestartGrant".to_owned(),
        constants: vec![
            WelfareProgramConstant {
                key: "minimumAgeYears".to_owned(),
                value: WelfareValue::AgeYears(22),
            },
            WelfareProgramConstant {
                key: "maximumAgeYears".to_owned(),
                value: WelfareValue::AgeYears(67),
            },
            WelfareProgramConstant {
                key: "incomeWindowDays".to_owned(),
                value: WelfareValue::Count(30),
            },
            WelfareProgramConstant {
                key: "incomeCapKrw".to_owned(),
                value: WelfareValue::MoneyKrw(1_234_567),
            },
            WelfareProgramConstant {
                key: "assetCapKrw".to_owned(),
                value: WelfareValue::MoneyKrw(12_345_678),
            },
            WelfareProgramConstant {
                key: "benefitKrw".to_owned(),
                value: WelfareValue::MoneyKrw(333_000),
            },
        ],
        conditions: vec![
            WelfareProgramCondition {
                code: "ageWindow".to_owned(),
                expression: WelfareExpression::Between {
                    value: Box::new(current_day_fact("character.age")),
                    lower: Box::new(constant("minimumAgeYears")),
                    upper: Box::new(constant("maximumAgeYears")),
                },
            },
            WelfareProgramCondition {
                code: "workTransition".to_owned(),
                expression: WelfareExpression::Any {
                    children: vec![
                        WelfareExpression::In {
                            value: Box::new(current_day_fact("career.employmentStatus")),
                            literals: vec![
                                enum_literal("welfareEmployment", "none"),
                                enum_literal("welfareEmployment", "ended"),
                            ],
                        },
                        WelfareExpression::Gte {
                            left: Box::new(current_day_fact("household.dependentCount")),
                            right: Box::new(WelfareExpression::Literal {
                                value: WelfareValue::Count(1),
                            }),
                        },
                    ],
                },
            },
            WelfareProgramCondition {
                code: "recentIncome".to_owned(),
                expression: WelfareExpression::Lte {
                    left: Box::new(WelfareExpression::Fact {
                        path: "income.periodTotal".to_owned(),
                        window: WelfareWindowSpec::PreviousClosedDays {
                            days: WelfareWindowDays::Constant {
                                key: "incomeWindowDays".to_owned(),
                            },
                        },
                    }),
                    right: Box::new(constant("incomeCapKrw")),
                },
            },
            WelfareProgramCondition {
                code: "policyAsset".to_owned(),
                expression: WelfareExpression::Lte {
                    left: Box::new(WelfareExpression::Fact {
                        path: "asset.policyValuation".to_owned(),
                        window: WelfareWindowSpec::PriorClose,
                    }),
                    right: Box::new(constant("assetCapKrw")),
                },
            },
            WelfareProgramCondition {
                code: "residenceKnown".to_owned(),
                expression: current_day_fact("residence.exists"),
            },
            WelfareProgramCondition {
                code: "notServing".to_owned(),
                expression: WelfareExpression::Not {
                    child: Box::new(WelfareExpression::Eq {
                        left: Box::new(current_day_fact("military.status")),
                        right: Box::new(WelfareExpression::Literal {
                            value: enum_literal("military", "serving"),
                        }),
                    }),
                },
            },
        ],
        eligibility_root: WelfareEligibilityExpression::All {
            children: [
                "ageWindow",
                "workTransition",
                "recentIncome",
                "policyAsset",
                "residenceKnown",
                "notServing",
            ]
            .into_iter()
            .map(|code| WelfareEligibilityExpression::Condition {
                code: code.to_owned(),
            })
            .collect(),
        },
        benefit: WelfareBenefitDefinition {
            amount_constant_key: "benefitKrw".to_owned(),
            payment_delay_days: 1,
        },
        reassessment_triggers: vec![
            WelfareFactSource::GameDay,
            WelfareFactSource::Household,
            WelfareFactSource::Residence,
            WelfareFactSource::Employment,
            WelfareFactSource::Military,
            WelfareFactSource::Income,
            WelfareFactSource::Asset,
        ],
    }
}

impl WelfareRules for V1WelfareRules {
    fn fact_registry(&self) -> &WelfareFactRegistry {
        &self.registry
    }

    fn validate_program(&self, program: &WelfareProgramDefinition) -> Result<(), WelfareError> {
        validate_program(self, program)
    }

    fn evaluate_program(
        &self,
        program: &WelfareProgramDefinition,
        input: &WelfareEvaluationInput<'_>,
    ) -> Result<WelfareEvaluation, WelfareError> {
        validate_program(self, program)?;
        let (facts, collections) = normalize_program_evidence(self, program, input)?;
        let context = EvaluationContext::new(&facts, &collections)?;
        let constants = constant_map(program)?;

        let mut condition_values = BTreeMap::new();
        let mut condition_results = Vec::with_capacity(program.conditions.len());
        for condition in &program.conditions {
            let value = evaluate_expression(self, &condition.expression, &constants, &context)?;
            let result = runtime_boolean(value)?;
            condition_values.insert(condition.code.clone(), result);
            condition_results.push(WelfareConditionResult {
                code: condition.code.clone(),
                result,
            });
        }

        let eligibility = evaluate_eligibility(&program.eligibility_root, &condition_values)?;
        let status = match eligibility {
            WelfareTruth::True => WelfareEvaluationStatus::Eligible,
            WelfareTruth::False => WelfareEvaluationStatus::Ineligible,
            WelfareTruth::Unknown(_) => WelfareEvaluationStatus::Indeterminate,
        };
        let fact_fingerprint = self.fingerprint(&WelfareFingerprintInput {
            schema_version: program.schema_version,
            program_version_id: program.program_version_id,
            facts: &facts,
            collections: &collections,
            period_pin: input.period_pin,
        })?;

        Ok(WelfareEvaluation {
            status,
            fact_fingerprint,
            conditions: condition_results,
        })
    }

    fn fingerprint(&self, input: &WelfareFingerprintInput<'_>) -> Result<String, WelfareError> {
        fingerprint(self, input)
    }

    fn canonical_fingerprint_json(
        &self,
        input: &WelfareFingerprintInput<'_>,
    ) -> Result<String, WelfareError> {
        canonical_fingerprint_json(self, input)
    }
}

fn create_fact_registry() -> WelfareFactRegistry {
    let current_day = WelfareWindowConstraint::CurrentDay;
    let prior_close = WelfareWindowConstraint::PriorClose;
    let previous_closed_days = WelfareWindowConstraint::PreviousClosedDays {
        minimum: 1,
        maximum: WELFARE_MAX_PREVIOUS_CLOSED_DAYS,
    };

    WelfareFactRegistry {
        schema_version: WELFARE_SCHEMA_VERSION,
        facts: vec![
            fact(
                "character.age",
                WelfareValueType::AgeYears,
                current_day.clone(),
                WelfareFactSource::GameDay,
            ),
            fact(
                "household.memberCount",
                WelfareValueType::Count,
                current_day.clone(),
                WelfareFactSource::Household,
            ),
            fact(
                "household.dependentCount",
                WelfareValueType::Count,
                current_day.clone(),
                WelfareFactSource::Household,
            ),
            fact(
                "residence.exists",
                WelfareValueType::Boolean,
                current_day.clone(),
                WelfareFactSource::Residence,
            ),
            fact(
                "residence.region",
                WelfareValueType::Enum("region".to_owned()),
                current_day.clone(),
                WelfareFactSource::Residence,
            ),
            fact(
                "career.employmentStatus",
                WelfareValueType::Enum("welfareEmployment".to_owned()),
                current_day.clone(),
                WelfareFactSource::Employment,
            ),
            fact(
                "military.status",
                WelfareValueType::Enum("military".to_owned()),
                current_day,
                WelfareFactSource::Military,
            ),
            fact(
                "income.periodTotal",
                WelfareValueType::MoneyKrw,
                previous_closed_days.clone(),
                WelfareFactSource::Income,
            ),
            fact(
                "asset.policyValuation",
                WelfareValueType::MoneyKrw,
                prior_close.clone(),
                WelfareFactSource::Asset,
            ),
            fact(
                "debt.policyBalance",
                WelfareValueType::MoneyKrw,
                prior_close.clone(),
                WelfareFactSource::Debt,
            ),
        ],
        collections: vec![
            collection(
                "income.entries",
                WelfareValueType::MoneyKrw,
                previous_closed_days,
                WelfareFactSource::Income,
            ),
            collection(
                "asset.positions",
                WelfareValueType::MoneyKrw,
                prior_close.clone(),
                WelfareFactSource::Asset,
            ),
            collection(
                "debt.positions",
                WelfareValueType::MoneyKrw,
                prior_close,
                WelfareFactSource::Debt,
            ),
        ],
        enums: vec![
            WelfareEnumDefinition {
                schema_key: "region".to_owned(),
                values: vec![
                    "capitalArea".to_owned(),
                    "metropolitan".to_owned(),
                    "smallCity".to_owned(),
                    "rural".to_owned(),
                ],
            },
            WelfareEnumDefinition {
                schema_key: "welfareEmployment".to_owned(),
                values: vec![
                    "none".to_owned(),
                    "pendingStart".to_owned(),
                    "active".to_owned(),
                    "ended".to_owned(),
                ],
            },
            WelfareEnumDefinition {
                schema_key: "military".to_owned(),
                values: vec![
                    "unserved".to_owned(),
                    "serving".to_owned(),
                    "completed".to_owned(),
                    "exempt".to_owned(),
                ],
            },
        ],
    }
}

fn fact(
    path: &str,
    value_type: WelfareValueType,
    window: WelfareWindowConstraint,
    source: WelfareFactSource,
) -> WelfareFactDefinition {
    WelfareFactDefinition {
        path: path.to_owned(),
        value_type,
        window,
        source,
    }
}

fn collection(
    key: &str,
    item_type: WelfareValueType,
    window: WelfareWindowConstraint,
    source: WelfareFactSource,
) -> WelfareCollectionDefinition {
    WelfareCollectionDefinition {
        key: key.to_owned(),
        item_type,
        window,
        source,
        maximum_rows: WELFARE_MAX_COLLECTION_ROWS as u8,
    }
}

#[derive(Debug)]
struct TypedAnalysis {
    value_type: WelfareValueType,
    static_value: Option<WelfareValue>,
    nodes: usize,
    depth: usize,
    facts: BTreeSet<(String, WelfareResolvedWindow)>,
    collections: BTreeSet<(String, WelfareResolvedWindow)>,
    sources: BTreeSet<WelfareFactSource>,
    constants: BTreeSet<String>,
}

impl TypedAnalysis {
    fn leaf(value_type: WelfareValueType, static_value: Option<WelfareValue>) -> Self {
        Self {
            value_type,
            static_value,
            nodes: 1,
            depth: 1,
            facts: BTreeSet::new(),
            collections: BTreeSet::new(),
            sources: BTreeSet::new(),
            constants: BTreeSet::new(),
        }
    }

    fn branch(value_type: WelfareValueType, children: Vec<Self>) -> Self {
        let mut result = Self::leaf(value_type, None);
        result.depth = 1 + children.iter().map(|child| child.depth).max().unwrap_or(0);
        for child in children {
            result.nodes += child.nodes;
            result.facts.extend(child.facts);
            result.collections.extend(child.collections);
            result.sources.extend(child.sources);
            result.constants.extend(child.constants);
        }
        result
    }
}

#[derive(Debug)]
struct EligibilityAnalysis {
    nodes: usize,
    depth: usize,
    conditions: BTreeSet<String>,
    condition_depths: BTreeMap<String, usize>,
}

fn validate_program(
    rules: &V1WelfareRules,
    program: &WelfareProgramDefinition,
) -> Result<(), WelfareError> {
    if program.schema_version != WELFARE_SCHEMA_VERSION {
        return Err(WelfareError::UnsupportedSchemaVersion);
    }
    if !is_canonical_key(&program.program_key) || !is_canonical_key(&program.duplicate_group_key) {
        return Err(WelfareError::InvalidCanonicalKey);
    }
    if program.constants.len() > WELFARE_MAX_CONSTANTS {
        return Err(WelfareError::TooManyConstants);
    }
    if program.conditions.len() > WELFARE_MAX_CONDITIONS {
        return Err(WelfareError::TooManyConditions);
    }

    let constants = constant_map(program)?;
    for constant in &program.constants {
        validate_value(rules, &constant.value)?;
    }

    let condition_codes = condition_code_set(program)?;
    let mut used_constants = BTreeSet::new();
    let mut used_facts = BTreeSet::new();
    let mut used_collections = BTreeSet::new();
    let mut required_sources = BTreeSet::new();
    let mut node_count = 0;
    let mut condition_depths = BTreeMap::new();
    for condition in &program.conditions {
        let analysis = validate_expression(rules, &condition.expression, &constants)?;
        if analysis.value_type != WelfareValueType::Boolean {
            return Err(WelfareError::TypeMismatch);
        }
        if analysis.depth > WELFARE_MAX_AST_DEPTH {
            return Err(WelfareError::AstTooDeep);
        }
        node_count += analysis.nodes;
        condition_depths.insert(condition.code.clone(), analysis.depth);
        used_constants.extend(analysis.constants);
        used_facts.extend(analysis.facts);
        used_collections.extend(analysis.collections);
        required_sources.extend(analysis.sources);
    }

    let eligibility = validate_eligibility_root(&program.eligibility_root, &condition_codes, true)?;
    node_count += eligibility.nodes;
    if node_count > WELFARE_MAX_PROGRAM_NODES {
        return Err(WelfareError::ProgramTooLarge);
    }
    if eligibility.conditions != condition_codes {
        return Err(WelfareError::UnreachableCondition);
    }
    for (condition, root_depth) in &eligibility.condition_depths {
        let condition_depth = condition_depths
            .get(condition)
            .ok_or(WelfareError::UnknownCondition)?;
        if root_depth + condition_depth > WELFARE_MAX_AST_DEPTH {
            return Err(WelfareError::AstTooDeep);
        }
    }
    if used_facts.len() + used_collections.len() > WELFARE_MAX_PUBLIC_FACTS {
        return Err(WelfareError::TooManyPublicFacts);
    }

    let benefit = constants
        .get(&program.benefit.amount_constant_key)
        .ok_or(WelfareError::UnknownConstant)?;
    if !matches!(benefit, WelfareValue::MoneyKrw(amount) if *amount > 0)
        || program.benefit.payment_delay_days != 1
    {
        return Err(WelfareError::InvalidBenefit);
    }
    used_constants.insert(program.benefit.amount_constant_key.clone());
    if used_constants.len() != constants.len()
        || constants.keys().any(|key| !used_constants.contains(key))
    {
        return Err(WelfareError::UnusedConstant);
    }

    let trigger_set: BTreeSet<_> = program.reassessment_triggers.iter().copied().collect();
    if trigger_set.len() != program.reassessment_triggers.len() {
        return Err(WelfareError::DuplicateReassessmentTrigger);
    }
    if !required_sources.is_subset(&trigger_set) {
        return Err(WelfareError::MissingReassessmentTrigger);
    }

    Ok(())
}

fn constant_map(
    program: &WelfareProgramDefinition,
) -> Result<BTreeMap<String, WelfareValue>, WelfareError> {
    let mut constants = BTreeMap::new();
    for constant in &program.constants {
        if !is_canonical_key(&constant.key) {
            return Err(WelfareError::InvalidCanonicalKey);
        }
        if constants
            .insert(constant.key.clone(), constant.value.clone())
            .is_some()
        {
            return Err(WelfareError::DuplicateConstant);
        }
    }
    Ok(constants)
}

fn condition_code_set(
    program: &WelfareProgramDefinition,
) -> Result<BTreeSet<String>, WelfareError> {
    let mut codes = BTreeSet::new();
    for condition in &program.conditions {
        if !is_canonical_key(&condition.code) {
            return Err(WelfareError::InvalidCanonicalKey);
        }
        if !codes.insert(condition.code.clone()) {
            return Err(WelfareError::DuplicateCondition);
        }
    }
    Ok(codes)
}

fn validate_expression(
    rules: &V1WelfareRules,
    expression: &WelfareExpression,
    constants: &BTreeMap<String, WelfareValue>,
) -> Result<TypedAnalysis, WelfareError> {
    let analysis = match expression {
        WelfareExpression::All { children } | WelfareExpression::Any { children } => {
            validate_logical_arity(children.len())?;
            let children = children
                .iter()
                .map(|child| validate_expression(rules, child, constants))
                .collect::<Result<Vec<_>, _>>()?;
            if children
                .iter()
                .any(|child| child.value_type != WelfareValueType::Boolean)
            {
                return Err(WelfareError::TypeMismatch);
            }
            TypedAnalysis::branch(WelfareValueType::Boolean, children)
        }
        WelfareExpression::Not { child } => {
            let child = validate_expression(rules, child, constants)?;
            if child.value_type != WelfareValueType::Boolean {
                return Err(WelfareError::TypeMismatch);
            }
            TypedAnalysis::branch(WelfareValueType::Boolean, vec![child])
        }
        WelfareExpression::Eq { left, right } => {
            validate_comparison(rules, left, right, constants, false)?
        }
        WelfareExpression::In { value, literals } => {
            if literals.is_empty() || literals.len() > WELFARE_MAX_IN_LITERALS {
                return Err(WelfareError::InvalidInArity);
            }
            let value = validate_expression(rules, value, constants)?;
            for literal in literals {
                validate_value(rules, literal)?;
                ensure_same_type(&value.value_type, &literal.value_type())?;
            }
            let mut result = TypedAnalysis::branch(WelfareValueType::Boolean, vec![value]);
            result.nodes += literals.len();
            result
        }
        WelfareExpression::Lt { left, right }
        | WelfareExpression::Lte { left, right }
        | WelfareExpression::Gt { left, right }
        | WelfareExpression::Gte { left, right } => {
            validate_comparison(rules, left, right, constants, true)?
        }
        WelfareExpression::Between {
            value,
            lower,
            upper,
        } => {
            let value = validate_expression(rules, value, constants)?;
            let lower = validate_expression(rules, lower, constants)?;
            let upper = validate_expression(rules, upper, constants)?;
            ensure_same_type(&value.value_type, &lower.value_type)?;
            ensure_same_type(&value.value_type, &upper.value_type)?;
            ensure_ordered(&value.value_type)?;
            let lower_value = lower
                .static_value
                .as_ref()
                .ok_or(WelfareError::InvalidBetweenBounds)?;
            let upper_value = upper
                .static_value
                .as_ref()
                .ok_or(WelfareError::InvalidBetweenBounds)?;
            if compare_values(lower_value, upper_value)? == Ordering::Greater {
                return Err(WelfareError::InvalidBetweenBounds);
            }
            TypedAnalysis::branch(WelfareValueType::Boolean, vec![value, lower, upper])
        }
        WelfareExpression::Sum { collection, window } => {
            let definition = find_collection(rules, collection)?;
            if !is_numeric(&definition.item_type) {
                return Err(WelfareError::TypeMismatch);
            }
            let (window, used_constant) = resolve_window(window, constants)?;
            validate_window(&definition.window, &window)?;
            let mut result = TypedAnalysis::leaf(definition.item_type.clone(), None);
            result.collections.insert((collection.clone(), window));
            result.sources.insert(definition.source);
            if let Some(key) = used_constant {
                result.constants.insert(key);
            }
            result
        }
        WelfareExpression::Count { collection, window }
        | WelfareExpression::Exists { collection, window } => {
            let definition = find_collection(rules, collection)?;
            let (window, used_constant) = resolve_window(window, constants)?;
            validate_window(&definition.window, &window)?;
            let value_type = if matches!(expression, WelfareExpression::Count { .. }) {
                WelfareValueType::Count
            } else {
                WelfareValueType::Boolean
            };
            let mut result = TypedAnalysis::leaf(value_type, None);
            result.collections.insert((collection.clone(), window));
            result.sources.insert(definition.source);
            if let Some(key) = used_constant {
                result.constants.insert(key);
            }
            result
        }
        WelfareExpression::Fact { path, window } => {
            let definition = find_fact(rules, path)?;
            let (window, used_constant) = resolve_window(window, constants)?;
            validate_window(&definition.window, &window)?;
            let mut result = TypedAnalysis::leaf(definition.value_type.clone(), None);
            result.facts.insert((path.clone(), window));
            result.sources.insert(definition.source);
            if let Some(key) = used_constant {
                result.constants.insert(key);
            }
            result
        }
        WelfareExpression::Constant { key } => {
            let value = constants.get(key).ok_or(WelfareError::UnknownConstant)?;
            let mut result = TypedAnalysis::leaf(value.value_type(), Some(value.clone()));
            result.constants.insert(key.clone());
            result
        }
        WelfareExpression::Literal { value } => {
            validate_value(rules, value)?;
            TypedAnalysis::leaf(value.value_type(), Some(value.clone()))
        }
    };

    if analysis.depth > WELFARE_MAX_AST_DEPTH {
        return Err(WelfareError::AstTooDeep);
    }
    Ok(analysis)
}

fn validate_comparison(
    rules: &V1WelfareRules,
    left: &WelfareExpression,
    right: &WelfareExpression,
    constants: &BTreeMap<String, WelfareValue>,
    ordered: bool,
) -> Result<TypedAnalysis, WelfareError> {
    let left = validate_expression(rules, left, constants)?;
    let right = validate_expression(rules, right, constants)?;
    ensure_same_type(&left.value_type, &right.value_type)?;
    if ordered {
        ensure_ordered(&left.value_type)?;
    }
    Ok(TypedAnalysis::branch(
        WelfareValueType::Boolean,
        vec![left, right],
    ))
}

fn validate_eligibility_root(
    expression: &WelfareEligibilityExpression,
    conditions: &BTreeSet<String>,
    root: bool,
) -> Result<EligibilityAnalysis, WelfareError> {
    if root && matches!(expression, WelfareEligibilityExpression::Condition { .. }) {
        return Err(WelfareError::InvalidEligibilityRoot);
    }
    let result = match expression {
        WelfareEligibilityExpression::All { children }
        | WelfareEligibilityExpression::Any { children } => {
            validate_logical_arity(children.len())?;
            combine_eligibility(children, conditions)?
        }
        WelfareEligibilityExpression::Not { child } => {
            let child = validate_eligibility_root(child, conditions, false)?;
            EligibilityAnalysis {
                nodes: child.nodes + 1,
                depth: child.depth + 1,
                conditions: child.conditions,
                condition_depths: child
                    .condition_depths
                    .into_iter()
                    .map(|(code, depth)| (code, depth + 1))
                    .collect(),
            }
        }
        WelfareEligibilityExpression::Condition { code } => {
            if !conditions.contains(code) {
                return Err(WelfareError::UnknownCondition);
            }
            EligibilityAnalysis {
                nodes: 0,
                depth: 0,
                conditions: BTreeSet::from([code.clone()]),
                condition_depths: BTreeMap::from([(code.clone(), 0)]),
            }
        }
    };
    if result.depth > WELFARE_MAX_AST_DEPTH {
        return Err(WelfareError::AstTooDeep);
    }
    Ok(result)
}

fn combine_eligibility(
    children: &[WelfareEligibilityExpression],
    conditions: &BTreeSet<String>,
) -> Result<EligibilityAnalysis, WelfareError> {
    let children = children
        .iter()
        .map(|child| validate_eligibility_root(child, conditions, false))
        .collect::<Result<Vec<_>, _>>()?;
    let mut result = EligibilityAnalysis {
        nodes: 1,
        depth: 1 + children.iter().map(|child| child.depth).max().unwrap_or(0),
        conditions: BTreeSet::new(),
        condition_depths: BTreeMap::new(),
    };
    for child in children {
        result.nodes += child.nodes;
        result.conditions.extend(child.conditions);
        for (code, depth) in child.condition_depths {
            result
                .condition_depths
                .entry(code)
                .and_modify(|current| *current = (*current).max(depth + 1))
                .or_insert(depth + 1);
        }
    }
    Ok(result)
}

fn validate_logical_arity(length: usize) -> Result<(), WelfareError> {
    if length == 0 || length > WELFARE_MAX_LOGICAL_CHILDREN {
        return Err(WelfareError::InvalidLogicalArity);
    }
    Ok(())
}

fn validate_value(rules: &V1WelfareRules, value: &WelfareValue) -> Result<(), WelfareError> {
    match value {
        WelfareValue::Count(value) | WelfareValue::AgeYears(value) if *value < 0 => {
            Err(WelfareError::InvalidLiteral)
        }
        WelfareValue::String(value) => {
            let scalar_count = value.chars().count();
            if scalar_count == 0 || scalar_count > WELFARE_MAX_STRING_SCALARS {
                Err(WelfareError::InvalidStringLiteral)
            } else {
                Ok(())
            }
        }
        WelfareValue::Enum(value) => {
            let definition = rules
                .registry
                .enums
                .iter()
                .find(|definition| definition.schema_key == value.schema_key)
                .ok_or(WelfareError::InvalidLiteral)?;
            if definition.values.contains(&value.value) {
                Ok(())
            } else {
                Err(WelfareError::InvalidLiteral)
            }
        }
        _ => Ok(()),
    }
}

fn resolve_window(
    window: &WelfareWindowSpec,
    constants: &BTreeMap<String, WelfareValue>,
) -> Result<(WelfareResolvedWindow, Option<String>), WelfareError> {
    match window {
        WelfareWindowSpec::CurrentDay => Ok((WelfareResolvedWindow::CurrentDay, None)),
        WelfareWindowSpec::PriorClose => Ok((WelfareResolvedWindow::PriorClose, None)),
        WelfareWindowSpec::PreviousClosedDays { days } => {
            let (days, constant) = match days {
                WelfareWindowDays::Literal { days } => (*days, None),
                WelfareWindowDays::Constant { key } => {
                    let value = constants.get(key).ok_or(WelfareError::UnknownConstant)?;
                    let WelfareValue::Count(value) = value else {
                        return Err(WelfareError::TypeMismatch);
                    };
                    let days = u16::try_from(*value).map_err(|_| WelfareError::InvalidWindow)?;
                    (days, Some(key.clone()))
                }
            };
            if days == 0 || days > WELFARE_MAX_PREVIOUS_CLOSED_DAYS {
                return Err(WelfareError::InvalidWindow);
            }
            Ok((WelfareResolvedWindow::PreviousClosedDays { days }, constant))
        }
    }
}

fn validate_window(
    constraint: &WelfareWindowConstraint,
    window: &WelfareResolvedWindow,
) -> Result<(), WelfareError> {
    let valid = match (constraint, window) {
        (WelfareWindowConstraint::CurrentDay, WelfareResolvedWindow::CurrentDay)
        | (WelfareWindowConstraint::PriorClose, WelfareResolvedWindow::PriorClose) => true,
        (
            WelfareWindowConstraint::PreviousClosedDays { minimum, maximum },
            WelfareResolvedWindow::PreviousClosedDays { days },
        ) => days >= minimum && days <= maximum && *days <= WELFARE_MAX_PREVIOUS_CLOSED_DAYS,
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(WelfareError::InvalidWindow)
    }
}

fn find_fact<'a>(
    rules: &'a V1WelfareRules,
    path: &str,
) -> Result<&'a WelfareFactDefinition, WelfareError> {
    rules
        .registry
        .facts
        .iter()
        .find(|definition| definition.path == path)
        .ok_or(WelfareError::UnknownFact)
}

fn find_collection<'a>(
    rules: &'a V1WelfareRules,
    key: &str,
) -> Result<&'a WelfareCollectionDefinition, WelfareError> {
    rules
        .registry
        .collections
        .iter()
        .find(|definition| definition.key == key)
        .ok_or(WelfareError::UnknownCollection)
}

fn ensure_same_type(left: &WelfareValueType, right: &WelfareValueType) -> Result<(), WelfareError> {
    if left == right {
        return Ok(());
    }
    if scalar_category(left) == scalar_category(right) {
        Err(WelfareError::UnitMismatch)
    } else {
        Err(WelfareError::TypeMismatch)
    }
}

fn scalar_category(value_type: &WelfareValueType) -> &'static str {
    match value_type {
        WelfareValueType::Boolean => "boolean",
        WelfareValueType::Integer
        | WelfareValueType::MoneyKrw
        | WelfareValueType::Count
        | WelfareValueType::AgeYears => "integer",
        WelfareValueType::Date => "date",
        WelfareValueType::String => "string",
        WelfareValueType::Enum(_) => "enum",
    }
}

fn ensure_ordered(value_type: &WelfareValueType) -> Result<(), WelfareError> {
    if matches!(
        value_type,
        WelfareValueType::Integer
            | WelfareValueType::MoneyKrw
            | WelfareValueType::Count
            | WelfareValueType::AgeYears
            | WelfareValueType::Date
    ) {
        Ok(())
    } else {
        Err(WelfareError::UnorderedType)
    }
}

fn is_numeric(value_type: &WelfareValueType) -> bool {
    matches!(
        value_type,
        WelfareValueType::Integer
            | WelfareValueType::MoneyKrw
            | WelfareValueType::Count
            | WelfareValueType::AgeYears
    )
}

fn is_canonical_key(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && bytes[0].is_ascii_lowercase()
        && bytes[1..].iter().all(u8::is_ascii_alphanumeric)
}

fn compare_values(left: &WelfareValue, right: &WelfareValue) -> Result<Ordering, WelfareError> {
    ensure_same_type(&left.value_type(), &right.value_type())?;
    match (left, right) {
        (WelfareValue::Integer(left), WelfareValue::Integer(right))
        | (WelfareValue::MoneyKrw(left), WelfareValue::MoneyKrw(right))
        | (WelfareValue::Count(left), WelfareValue::Count(right))
        | (WelfareValue::AgeYears(left), WelfareValue::AgeYears(right)) => Ok(left.cmp(right)),
        (WelfareValue::Date(left), WelfareValue::Date(right)) => Ok(left.cmp(right)),
        _ => Err(WelfareError::UnorderedType),
    }
}

#[derive(Debug, Clone)]
enum RuntimeValue {
    Known(WelfareValue),
    Unknown(WelfareUnknownReason),
}

struct EvaluationContext<'a> {
    facts: BTreeMap<(String, WelfareResolvedWindow), &'a WelfareFactEvidence>,
    collections: BTreeMap<(String, WelfareResolvedWindow), &'a WelfareCollectionEvidence>,
}

impl<'a> EvaluationContext<'a> {
    fn new(
        facts: &'a [WelfareFactEvidence],
        collections: &'a [WelfareCollectionEvidence],
    ) -> Result<Self, WelfareError> {
        let mut fact_map = BTreeMap::new();
        for fact in facts {
            if fact_map
                .insert((fact.key.clone(), fact.window.clone()), fact)
                .is_some()
            {
                return Err(WelfareError::DuplicateFact);
            }
        }
        let mut collection_map = BTreeMap::new();
        for collection in collections {
            if collection_map
                .insert(
                    (collection.key.clone(), collection.window.clone()),
                    collection,
                )
                .is_some()
            {
                return Err(WelfareError::DuplicateCollection);
            }
        }
        Ok(Self {
            facts: fact_map,
            collections: collection_map,
        })
    }
}

fn normalize_program_evidence(
    rules: &V1WelfareRules,
    program: &WelfareProgramDefinition,
    input: &WelfareEvaluationInput<'_>,
) -> Result<(Vec<WelfareFactEvidence>, Vec<WelfareCollectionEvidence>), WelfareError> {
    let constants = constant_map(program)?;
    let mut required_facts = BTreeSet::new();
    let mut required_collections = BTreeSet::new();
    for condition in &program.conditions {
        let analysis = validate_expression(rules, &condition.expression, &constants)?;
        required_facts.extend(analysis.facts);
        required_collections.extend(analysis.collections);
    }

    let supplied = validate_evidence(rules, input.facts, input.collections)?;
    let mut facts = Vec::with_capacity(required_facts.len());
    for (key, window) in required_facts {
        if let Some(evidence) = supplied.facts.get(&(key.clone(), window.clone())) {
            facts.push((*evidence).clone());
        } else {
            let definition = find_fact(rules, &key)?;
            facts.push(WelfareFactEvidence {
                key,
                value_type: definition.value_type.clone(),
                window,
                value: WelfareEvidenceValue::Unknown(WelfareUnknownReason::AuthorityMissing),
            });
        }
    }

    let mut collections = Vec::with_capacity(required_collections.len());
    for (key, window) in required_collections {
        if let Some(evidence) = supplied.collections.get(&(key.clone(), window.clone())) {
            collections.push(normalize_collection(rules, evidence)?);
        } else {
            let definition = find_collection(rules, &key)?;
            collections.push(WelfareCollectionEvidence {
                key,
                item_type: definition.item_type.clone(),
                window,
                value: WelfareCollectionEvidenceValue::Unknown(
                    WelfareUnknownReason::AuthorityMissing,
                ),
            });
        }
    }
    Ok((facts, collections))
}

fn validate_evidence<'a>(
    rules: &V1WelfareRules,
    facts: &'a [WelfareFactEvidence],
    collections: &'a [WelfareCollectionEvidence],
) -> Result<EvaluationContext<'a>, WelfareError> {
    for fact in facts {
        let definition = find_fact(rules, &fact.key)?;
        ensure_same_type(&definition.value_type, &fact.value_type)?;
        validate_window(&definition.window, &fact.window)?;
        if let WelfareEvidenceValue::Known(value) = &fact.value {
            validate_value(rules, value)?;
            ensure_same_type(&fact.value_type, &value.value_type())?;
        }
    }
    for collection in collections {
        let definition = find_collection(rules, &collection.key)?;
        if definition.maximum_rows == 0
            || usize::from(definition.maximum_rows) > WELFARE_MAX_COLLECTION_ROWS
        {
            return Err(WelfareError::InvalidCollectionBound);
        }
        ensure_same_type(&definition.item_type, &collection.item_type)?;
        validate_window(&definition.window, &collection.window)?;
        if let WelfareCollectionEvidenceValue::Known(values) = &collection.value {
            for value in values {
                validate_value(rules, value)?;
                ensure_same_type(&collection.item_type, &value.value_type())?;
            }
        }
    }
    EvaluationContext::new(facts, collections)
}

fn normalize_collection(
    rules: &V1WelfareRules,
    evidence: &WelfareCollectionEvidence,
) -> Result<WelfareCollectionEvidence, WelfareError> {
    let definition = find_collection(rules, &evidence.key)?;
    let value = match &evidence.value {
        WelfareCollectionEvidenceValue::Known(values)
            if values.len() > usize::from(definition.maximum_rows)
                || values.len() > WELFARE_MAX_COLLECTION_ROWS =>
        {
            WelfareCollectionEvidenceValue::Unknown(WelfareUnknownReason::CollectionLimitExceeded)
        }
        value => value.clone(),
    };
    Ok(WelfareCollectionEvidence {
        key: evidence.key.clone(),
        item_type: evidence.item_type.clone(),
        window: evidence.window.clone(),
        value,
    })
}

fn evaluate_expression(
    rules: &V1WelfareRules,
    expression: &WelfareExpression,
    constants: &BTreeMap<String, WelfareValue>,
    context: &EvaluationContext<'_>,
) -> Result<RuntimeValue, WelfareError> {
    match expression {
        WelfareExpression::All { children } => {
            let values = children
                .iter()
                .map(|child| evaluate_expression(rules, child, constants, context))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(runtime_from_truth(kleene_all(
                values
                    .into_iter()
                    .map(runtime_boolean)
                    .collect::<Result<Vec<_>, _>>()?,
            )))
        }
        WelfareExpression::Any { children } => {
            let values = children
                .iter()
                .map(|child| evaluate_expression(rules, child, constants, context))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(runtime_from_truth(kleene_any(
                values
                    .into_iter()
                    .map(runtime_boolean)
                    .collect::<Result<Vec<_>, _>>()?,
            )))
        }
        WelfareExpression::Not { child } => {
            let value = runtime_boolean(evaluate_expression(rules, child, constants, context)?)?;
            Ok(runtime_from_truth(kleene_not(value)))
        }
        WelfareExpression::Eq { left, right } => {
            evaluate_equality(rules, left, right, constants, context, |ordering| {
                ordering == Ordering::Equal
            })
        }
        WelfareExpression::In { value, literals } => {
            match evaluate_expression(rules, value, constants, context)? {
                RuntimeValue::Unknown(reason) => Ok(RuntimeValue::Unknown(reason)),
                RuntimeValue::Known(value) => Ok(RuntimeValue::Known(WelfareValue::Boolean(
                    literals.contains(&value),
                ))),
            }
        }
        WelfareExpression::Lt { left, right } => {
            evaluate_equality(rules, left, right, constants, context, |ordering| {
                ordering == Ordering::Less
            })
        }
        WelfareExpression::Lte { left, right } => {
            evaluate_equality(rules, left, right, constants, context, |ordering| {
                ordering != Ordering::Greater
            })
        }
        WelfareExpression::Gt { left, right } => {
            evaluate_equality(rules, left, right, constants, context, |ordering| {
                ordering == Ordering::Greater
            })
        }
        WelfareExpression::Gte { left, right } => {
            evaluate_equality(rules, left, right, constants, context, |ordering| {
                ordering != Ordering::Less
            })
        }
        WelfareExpression::Between {
            value,
            lower,
            upper,
        } => {
            let value = evaluate_expression(rules, value, constants, context)?;
            let lower = evaluate_expression(rules, lower, constants, context)?;
            let upper = evaluate_expression(rules, upper, constants, context)?;
            match first_unknown([&value, &lower, &upper]) {
                Some(reason) => Ok(RuntimeValue::Unknown(reason)),
                None => {
                    let RuntimeValue::Known(value) = value else {
                        return Err(WelfareError::InvalidEvidence);
                    };
                    let RuntimeValue::Known(lower) = lower else {
                        return Err(WelfareError::InvalidEvidence);
                    };
                    let RuntimeValue::Known(upper) = upper else {
                        return Err(WelfareError::InvalidEvidence);
                    };
                    Ok(RuntimeValue::Known(WelfareValue::Boolean(
                        compare_values(&value, &lower)? != Ordering::Less
                            && compare_values(&value, &upper)? != Ordering::Greater,
                    )))
                }
            }
        }
        WelfareExpression::Sum { collection, window } => {
            let (window, _) = resolve_window(window, constants)?;
            let evidence = context
                .collections
                .get(&(collection.clone(), window))
                .ok_or(WelfareError::InvalidEvidence)?;
            evaluate_sum(evidence)
        }
        WelfareExpression::Count { collection, window } => {
            let (window, _) = resolve_window(window, constants)?;
            let evidence = context
                .collections
                .get(&(collection.clone(), window))
                .ok_or(WelfareError::InvalidEvidence)?;
            match &evidence.value {
                WelfareCollectionEvidenceValue::Known(values) => {
                    Ok(RuntimeValue::Known(WelfareValue::Count(
                        i64::try_from(values.len())
                            .map_err(|_| WelfareError::InvalidCollectionBound)?,
                    )))
                }
                WelfareCollectionEvidenceValue::Unknown(reason) => {
                    Ok(RuntimeValue::Unknown(*reason))
                }
            }
        }
        WelfareExpression::Exists { collection, window } => {
            let (window, _) = resolve_window(window, constants)?;
            let evidence = context
                .collections
                .get(&(collection.clone(), window))
                .ok_or(WelfareError::InvalidEvidence)?;
            match &evidence.value {
                WelfareCollectionEvidenceValue::Known(values) => Ok(RuntimeValue::Known(
                    WelfareValue::Boolean(!values.is_empty()),
                )),
                WelfareCollectionEvidenceValue::Unknown(reason) => {
                    Ok(RuntimeValue::Unknown(*reason))
                }
            }
        }
        WelfareExpression::Fact { path, window } => {
            let (window, _) = resolve_window(window, constants)?;
            let evidence = context
                .facts
                .get(&(path.clone(), window))
                .ok_or(WelfareError::InvalidEvidence)?;
            match &evidence.value {
                WelfareEvidenceValue::Known(value) => Ok(RuntimeValue::Known(value.clone())),
                WelfareEvidenceValue::Unknown(reason) => Ok(RuntimeValue::Unknown(*reason)),
            }
        }
        WelfareExpression::Constant { key } => constants
            .get(key)
            .cloned()
            .map(RuntimeValue::Known)
            .ok_or(WelfareError::UnknownConstant),
        WelfareExpression::Literal { value } => Ok(RuntimeValue::Known(value.clone())),
    }
}

fn evaluate_equality(
    rules: &V1WelfareRules,
    left: &WelfareExpression,
    right: &WelfareExpression,
    constants: &BTreeMap<String, WelfareValue>,
    context: &EvaluationContext<'_>,
    predicate: impl FnOnce(Ordering) -> bool,
) -> Result<RuntimeValue, WelfareError> {
    let left = evaluate_expression(rules, left, constants, context)?;
    let right = evaluate_expression(rules, right, constants, context)?;
    if let Some(reason) = first_unknown([&left, &right]) {
        return Ok(RuntimeValue::Unknown(reason));
    }
    let RuntimeValue::Known(left) = left else {
        return Err(WelfareError::InvalidEvidence);
    };
    let RuntimeValue::Known(right) = right else {
        return Err(WelfareError::InvalidEvidence);
    };
    let ordering = if matches!(
        left,
        WelfareValue::Boolean(_) | WelfareValue::String(_) | WelfareValue::Enum(_)
    ) {
        if left == right {
            Ordering::Equal
        } else {
            Ordering::Less
        }
    } else {
        compare_values(&left, &right)?
    };
    Ok(RuntimeValue::Known(WelfareValue::Boolean(predicate(
        ordering,
    ))))
}

fn first_unknown<'a>(
    values: impl IntoIterator<Item = &'a RuntimeValue>,
) -> Option<WelfareUnknownReason> {
    values.into_iter().find_map(|value| match value {
        RuntimeValue::Unknown(reason) => Some(*reason),
        RuntimeValue::Known(_) => None,
    })
}

fn evaluate_sum(evidence: &WelfareCollectionEvidence) -> Result<RuntimeValue, WelfareError> {
    let WelfareCollectionEvidenceValue::Known(values) = &evidence.value else {
        let WelfareCollectionEvidenceValue::Unknown(reason) = evidence.value else {
            return Err(WelfareError::InvalidEvidence);
        };
        return Ok(RuntimeValue::Unknown(reason));
    };
    let total = values.iter().try_fold(0_i128, |total, value| {
        numeric_value(value)
            .and_then(|value| total.checked_add(i128::from(value)))
            .ok_or(WelfareUnknownReason::ArithmeticOverflow)
    });
    let total = match total.and_then(|value| {
        i64::try_from(value).map_err(|_| WelfareUnknownReason::ArithmeticOverflow)
    }) {
        Ok(total) => total,
        Err(reason) => return Ok(RuntimeValue::Unknown(reason)),
    };
    let value = match evidence.item_type {
        WelfareValueType::Integer => WelfareValue::Integer(total),
        WelfareValueType::MoneyKrw => WelfareValue::MoneyKrw(total),
        WelfareValueType::Count => WelfareValue::Count(total),
        WelfareValueType::AgeYears => WelfareValue::AgeYears(total),
        _ => return Err(WelfareError::TypeMismatch),
    };
    Ok(RuntimeValue::Known(value))
}

fn numeric_value(value: &WelfareValue) -> Option<i64> {
    match value {
        WelfareValue::Integer(value)
        | WelfareValue::MoneyKrw(value)
        | WelfareValue::Count(value)
        | WelfareValue::AgeYears(value) => Some(*value),
        _ => None,
    }
}

fn runtime_boolean(value: RuntimeValue) -> Result<WelfareTruth, WelfareError> {
    match value {
        RuntimeValue::Known(WelfareValue::Boolean(true)) => Ok(WelfareTruth::True),
        RuntimeValue::Known(WelfareValue::Boolean(false)) => Ok(WelfareTruth::False),
        RuntimeValue::Known(_) => Err(WelfareError::TypeMismatch),
        RuntimeValue::Unknown(reason) => Ok(WelfareTruth::Unknown(reason)),
    }
}

fn runtime_from_truth(value: WelfareTruth) -> RuntimeValue {
    match value {
        WelfareTruth::True => RuntimeValue::Known(WelfareValue::Boolean(true)),
        WelfareTruth::False => RuntimeValue::Known(WelfareValue::Boolean(false)),
        WelfareTruth::Unknown(reason) => RuntimeValue::Unknown(reason),
    }
}

fn kleene_all(values: Vec<WelfareTruth>) -> WelfareTruth {
    let mut unknown = None;
    for value in values {
        match value {
            WelfareTruth::False => return WelfareTruth::False,
            WelfareTruth::Unknown(reason) if unknown.is_none() => unknown = Some(reason),
            WelfareTruth::True | WelfareTruth::Unknown(_) => {}
        }
    }
    unknown.map_or(WelfareTruth::True, WelfareTruth::Unknown)
}

fn kleene_any(values: Vec<WelfareTruth>) -> WelfareTruth {
    let mut unknown = None;
    for value in values {
        match value {
            WelfareTruth::True => return WelfareTruth::True,
            WelfareTruth::Unknown(reason) if unknown.is_none() => unknown = Some(reason),
            WelfareTruth::False | WelfareTruth::Unknown(_) => {}
        }
    }
    unknown.map_or(WelfareTruth::False, WelfareTruth::Unknown)
}

fn kleene_not(value: WelfareTruth) -> WelfareTruth {
    match value {
        WelfareTruth::True => WelfareTruth::False,
        WelfareTruth::False => WelfareTruth::True,
        WelfareTruth::Unknown(reason) => WelfareTruth::Unknown(reason),
    }
}

fn evaluate_eligibility(
    expression: &WelfareEligibilityExpression,
    conditions: &BTreeMap<String, WelfareTruth>,
) -> Result<WelfareTruth, WelfareError> {
    match expression {
        WelfareEligibilityExpression::All { children } => Ok(kleene_all(
            children
                .iter()
                .map(|child| evaluate_eligibility(child, conditions))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        WelfareEligibilityExpression::Any { children } => Ok(kleene_any(
            children
                .iter()
                .map(|child| evaluate_eligibility(child, conditions))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        WelfareEligibilityExpression::Not { child } => {
            Ok(kleene_not(evaluate_eligibility(child, conditions)?))
        }
        WelfareEligibilityExpression::Condition { code } => conditions
            .get(code)
            .copied()
            .ok_or(WelfareError::UnknownCondition),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CanonicalFingerprint {
    schema_version: u16,
    program_version_id: String,
    period: CanonicalPeriod,
    facts: Vec<CanonicalEvidence>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CanonicalPeriod {
    evaluation_game_day: u32,
    window_bounds: Vec<CanonicalWindowBound>,
    authority_revisions: Vec<CanonicalAuthorityRevision>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CanonicalWindowBound {
    window: String,
    start_game_day: u32,
    end_game_day: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CanonicalAuthorityRevision {
    authority: String,
    revision: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CanonicalEvidence {
    key: String,
    kind: &'static str,
    value_type: String,
    unit: String,
    window: String,
    value: CanonicalEvidenceValue,
}

#[derive(Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
enum CanonicalEvidenceValue {
    Known { value: JsonValue },
    Unknown { reason: &'static str },
}

fn fingerprint(
    rules: &V1WelfareRules,
    input: &WelfareFingerprintInput<'_>,
) -> Result<String, WelfareError> {
    let canonical_json = canonical_fingerprint_json(rules, input)?;
    let digest = Sha256::digest(canonical_json.as_bytes());
    let mut fingerprint = String::with_capacity(64);
    for byte in digest {
        write!(&mut fingerprint, "{byte:02x}").map_err(|_| WelfareError::CanonicalSerialization)?;
    }
    Ok(fingerprint)
}

fn canonical_fingerprint_json(
    rules: &V1WelfareRules,
    input: &WelfareFingerprintInput<'_>,
) -> Result<String, WelfareError> {
    if input.schema_version != WELFARE_SCHEMA_VERSION {
        return Err(WelfareError::UnsupportedSchemaVersion);
    }
    validate_period_pin(input.period_pin)?;
    validate_evidence(rules, input.facts, input.collections)?;
    let pinned_windows = input
        .period_pin
        .window_bounds
        .iter()
        .map(|bound| &bound.window)
        .collect::<BTreeSet<_>>();
    if input
        .facts
        .iter()
        .map(|fact| &fact.window)
        .chain(
            input
                .collections
                .iter()
                .map(|collection| &collection.window),
        )
        .any(|window| !pinned_windows.contains(window))
    {
        return Err(WelfareError::InvalidPeriodPin);
    }

    let mut facts = Vec::with_capacity(input.facts.len() + input.collections.len());
    for fact in input.facts {
        facts.push(canonical_fact(fact)?);
    }
    for collection in input.collections {
        let collection = normalize_collection(rules, collection)?;
        facts.push(canonical_collection(&collection)?);
    }
    facts.sort_by(|left, right| {
        (&left.key, &left.window, left.kind).cmp(&(&right.key, &right.window, right.kind))
    });

    let canonical = CanonicalFingerprint {
        schema_version: input.schema_version,
        program_version_id: input.program_version_id.to_string(),
        period: canonical_period(input.period_pin),
        facts,
    };
    serde_json::to_string(&canonical).map_err(|_| WelfareError::CanonicalSerialization)
}

fn validate_period_pin(period: &WelfarePeriodPin) -> Result<(), WelfareError> {
    let mut windows = BTreeSet::new();
    for bound in &period.window_bounds {
        if bound.start_game_day > bound.end_game_day {
            return Err(WelfareError::InvalidPeriodPin);
        }
        if matches!(bound.window, WelfareResolvedWindow::CurrentDay)
            && (bound.start_game_day != period.evaluation_game_day
                || bound.end_game_day != period.evaluation_game_day)
        {
            return Err(WelfareError::InvalidPeriodPin);
        }
        if !windows.insert(bound.window.clone()) {
            return Err(WelfareError::DuplicatePeriodBound);
        }
    }
    let mut authorities = BTreeSet::new();
    for revision in &period.authority_revisions {
        if !is_canonical_key(&revision.authority)
            || revision.revision.is_empty()
            || revision.revision.chars().count() > 128
        {
            return Err(WelfareError::InvalidPeriodPin);
        }
        if !authorities.insert(revision.authority.clone()) {
            return Err(WelfareError::DuplicateAuthorityRevision);
        }
    }
    Ok(())
}

fn canonical_period(period: &WelfarePeriodPin) -> CanonicalPeriod {
    let mut window_bounds = period
        .window_bounds
        .iter()
        .map(|bound| CanonicalWindowBound {
            window: window_name(&bound.window),
            start_game_day: bound.start_game_day,
            end_game_day: bound.end_game_day,
        })
        .collect::<Vec<_>>();
    window_bounds.sort_by(|left, right| left.window.cmp(&right.window));
    let mut authority_revisions = period
        .authority_revisions
        .iter()
        .map(|revision| CanonicalAuthorityRevision {
            authority: revision.authority.clone(),
            revision: revision.revision.clone(),
        })
        .collect::<Vec<_>>();
    authority_revisions.sort_by(|left, right| left.authority.cmp(&right.authority));
    CanonicalPeriod {
        evaluation_game_day: period.evaluation_game_day,
        window_bounds,
        authority_revisions,
    }
}

fn canonical_fact(fact: &WelfareFactEvidence) -> Result<CanonicalEvidence, WelfareError> {
    let (value_type, unit) = type_and_unit(&fact.value_type);
    let value = match &fact.value {
        WelfareEvidenceValue::Known(value) => CanonicalEvidenceValue::Known {
            value: canonical_value(value)?,
        },
        WelfareEvidenceValue::Unknown(reason) => CanonicalEvidenceValue::Unknown {
            reason: unknown_reason(*reason),
        },
    };
    Ok(CanonicalEvidence {
        key: fact.key.clone(),
        kind: "fact",
        value_type,
        unit,
        window: window_name(&fact.window),
        value,
    })
}

fn canonical_collection(
    collection: &WelfareCollectionEvidence,
) -> Result<CanonicalEvidence, WelfareError> {
    let (value_type, unit) = type_and_unit(&collection.item_type);
    let value = match &collection.value {
        WelfareCollectionEvidenceValue::Known(values) => {
            let mut values = values
                .iter()
                .map(canonical_value)
                .collect::<Result<Vec<_>, _>>()?;
            values.sort_by_key(JsonValue::to_string);
            CanonicalEvidenceValue::Known {
                value: JsonValue::Array(values),
            }
        }
        WelfareCollectionEvidenceValue::Unknown(reason) => CanonicalEvidenceValue::Unknown {
            reason: unknown_reason(*reason),
        },
    };
    Ok(CanonicalEvidence {
        key: collection.key.clone(),
        kind: "collection",
        value_type,
        unit,
        window: window_name(&collection.window),
        value,
    })
}

fn canonical_value(value: &WelfareValue) -> Result<JsonValue, WelfareError> {
    match value {
        WelfareValue::Boolean(value) => Ok(JsonValue::Bool(*value)),
        WelfareValue::Integer(value)
        | WelfareValue::MoneyKrw(value)
        | WelfareValue::Count(value)
        | WelfareValue::AgeYears(value) => Ok(JsonValue::from(*value)),
        WelfareValue::Date(value) => Ok(JsonValue::String(value.to_string())),
        WelfareValue::String(value) => Ok(JsonValue::String(value.clone())),
        WelfareValue::Enum(value) => Ok(JsonValue::String(value.value.clone())),
    }
}

fn type_and_unit(value_type: &WelfareValueType) -> (String, String) {
    match value_type {
        WelfareValueType::Boolean => ("boolean".to_owned(), "boolean".to_owned()),
        WelfareValueType::Integer => ("integer".to_owned(), "integer".to_owned()),
        WelfareValueType::MoneyKrw => ("moneyKrw".to_owned(), "krw".to_owned()),
        WelfareValueType::Count => ("count".to_owned(), "count".to_owned()),
        WelfareValueType::AgeYears => ("ageYears".to_owned(), "years".to_owned()),
        WelfareValueType::Date => ("date".to_owned(), "date".to_owned()),
        WelfareValueType::String => ("string".to_owned(), "string".to_owned()),
        WelfareValueType::Enum(schema_key) => ("enum".to_owned(), schema_key.clone()),
    }
}

fn window_name(window: &WelfareResolvedWindow) -> String {
    match window {
        WelfareResolvedWindow::CurrentDay => "currentDay".to_owned(),
        WelfareResolvedWindow::PreviousClosedDays { days } => {
            format!("previousClosedDays:{days}")
        }
        WelfareResolvedWindow::PriorClose => "priorClose".to_owned(),
    }
}

fn unknown_reason(reason: WelfareUnknownReason) -> &'static str {
    match reason {
        WelfareUnknownReason::AuthorityMissing => "authorityMissing",
        WelfareUnknownReason::ValuationUnavailable => "valuationUnavailable",
        WelfareUnknownReason::CollectionLimitExceeded => "collectionLimitExceeded",
        WelfareUnknownReason::WindowIncomplete => "windowIncomplete",
        WelfareUnknownReason::ArithmeticOverflow => "arithmeticOverflow",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::life::{WELFARE_MAX_COLLECTION_ROWS, WelfareAuthorityRevision, WelfareWindowBound};

    mod context_catalog_validation {
        use super::*;

        #[test]
        fn given_fixture_when_validated_then_it_is_sealed() {
            let rules = given_rules();
            let program = create_fictional_restart_grant_program();

            let result = rules.validate_program(&program);

            assert_eq!(result, Ok(()));
        }

        #[test]
        fn given_fixture_graph_when_analyzed_then_declared_node_count_and_depth_match() {
            let rules = given_rules();
            let program = create_fictional_restart_grant_program();
            let constants = constant_map(&program).unwrap();
            let condition_codes = condition_code_set(&program).unwrap();
            let analyses = program
                .conditions
                .iter()
                .map(|condition| {
                    validate_expression(&rules, &condition.expression, &constants).unwrap()
                })
                .collect::<Vec<_>>();
            let root = validate_eligibility_root(&program.eligibility_root, &condition_codes, true)
                .unwrap();
            let node_count = analyses
                .iter()
                .map(|analysis| analysis.nodes)
                .sum::<usize>()
                + root.nodes;
            let max_depth = root
                .condition_depths
                .iter()
                .map(|(code, root_depth)| {
                    let condition_depth = program
                        .conditions
                        .iter()
                        .zip(&analyses)
                        .find(|(condition, _)| condition.code == *code)
                        .map(|(_, analysis)| analysis.depth)
                        .unwrap();
                    root_depth + condition_depth
                })
                .max()
                .unwrap();

            assert_eq!((node_count, max_depth), (24, 4));
        }

        #[test]
        fn given_schema_v1_when_registry_is_read_then_only_registered_public_facts_are_exposed() {
            let rules = given_rules();

            let paths = rules
                .fact_registry()
                .facts
                .iter()
                .map(|fact| fact.path.as_str())
                .collect::<Vec<_>>();

            assert_eq!(
                paths,
                vec![
                    "character.age",
                    "household.memberCount",
                    "household.dependentCount",
                    "residence.exists",
                    "residence.region",
                    "career.employmentStatus",
                    "military.status",
                    "income.periodTotal",
                    "asset.policyValuation",
                    "debt.policyBalance",
                ]
            );
        }

        #[test]
        fn given_money_fact_and_integer_when_compared_then_unit_mismatch_is_rejected() {
            let rules = given_rules();
            let expression = WelfareExpression::Lte {
                left: Box::new(WelfareExpression::Fact {
                    path: "income.periodTotal".to_owned(),
                    window: WelfareWindowSpec::PreviousClosedDays {
                        days: WelfareWindowDays::Literal { days: 30 },
                    },
                }),
                right: Box::new(WelfareExpression::Literal {
                    value: WelfareValue::Integer(1_234_567),
                }),
            };

            let result = validate_expression(&rules, &expression, &BTreeMap::new());

            assert_eq!(result.unwrap_err(), WelfareError::UnitMismatch);
        }

        #[test]
        fn given_boolean_and_integer_when_compared_then_type_mismatch_is_rejected() {
            let rules = given_rules();
            let expression = WelfareExpression::Eq {
                left: Box::new(WelfareExpression::Literal {
                    value: WelfareValue::Boolean(true),
                }),
                right: Box::new(WelfareExpression::Literal {
                    value: WelfareValue::Integer(1),
                }),
            };

            let result = validate_expression(&rules, &expression, &BTreeMap::new());

            assert_eq!(result.unwrap_err(), WelfareError::TypeMismatch);
        }

        #[test]
        fn given_enum_when_ordered_then_unordered_type_is_rejected() {
            let rules = given_rules();
            let expression = WelfareExpression::Gt {
                left: Box::new(WelfareExpression::Literal {
                    value: welfare_enum("military", "serving"),
                }),
                right: Box::new(WelfareExpression::Literal {
                    value: welfare_enum("military", "completed"),
                }),
            };

            let result = validate_expression(&rules, &expression, &BTreeMap::new());

            assert_eq!(result.unwrap_err(), WelfareError::UnorderedType);
        }

        #[test]
        fn given_reversed_between_bounds_when_validated_then_bounds_are_rejected() {
            let rules = given_rules();
            let expression = WelfareExpression::Between {
                value: Box::new(WelfareExpression::Literal {
                    value: WelfareValue::AgeYears(30),
                }),
                lower: Box::new(WelfareExpression::Literal {
                    value: WelfareValue::AgeYears(67),
                }),
                upper: Box::new(WelfareExpression::Literal {
                    value: WelfareValue::AgeYears(22),
                }),
            };

            let result = validate_expression(&rules, &expression, &BTreeMap::new());

            assert_eq!(result.unwrap_err(), WelfareError::InvalidBetweenBounds);
        }

        #[test]
        fn given_wrong_fact_window_when_validated_then_window_is_rejected() {
            let rules = given_rules();
            let expression = WelfareExpression::Fact {
                path: "character.age".to_owned(),
                window: WelfareWindowSpec::PriorClose,
            };

            let result = validate_expression(&rules, &expression, &BTreeMap::new());

            assert_eq!(result.unwrap_err(), WelfareError::InvalidWindow);
        }

        #[test]
        fn given_window_above_protocol_cap_when_validated_then_window_is_rejected() {
            let rules = given_rules();
            let expression = WelfareExpression::Fact {
                path: "income.periodTotal".to_owned(),
                window: WelfareWindowSpec::PreviousClosedDays {
                    days: WelfareWindowDays::Literal { days: 367 },
                },
            };

            let result = validate_expression(&rules, &expression, &BTreeMap::new());

            assert_eq!(result.unwrap_err(), WelfareError::InvalidWindow);
        }

        #[test]
        fn given_zero_day_window_when_validated_then_window_is_rejected() {
            let rules = given_rules();
            let expression = WelfareExpression::Fact {
                path: "income.periodTotal".to_owned(),
                window: WelfareWindowSpec::PreviousClosedDays {
                    days: WelfareWindowDays::Literal { days: 0 },
                },
            };

            let result = validate_expression(&rules, &expression, &BTreeMap::new());

            assert_eq!(result.unwrap_err(), WelfareError::InvalidWindow);
        }

        #[test]
        fn given_unused_constant_when_program_is_sealed_then_it_is_rejected() {
            let rules = given_rules();
            let mut program = create_fictional_restart_grant_program();
            program.constants.push(WelfareProgramConstant {
                key: "unusedValue".to_owned(),
                value: WelfareValue::Integer(1),
            });

            let result = rules.validate_program(&program);

            assert_eq!(result.unwrap_err(), WelfareError::UnusedConstant);
        }

        #[test]
        fn given_missing_trigger_when_program_is_sealed_then_it_is_rejected() {
            let rules = given_rules();
            let mut program = create_fictional_restart_grant_program();
            program
                .reassessment_triggers
                .retain(|source| *source != WelfareFactSource::Asset);

            let result = rules.validate_program(&program);

            assert_eq!(
                result.unwrap_err(),
                WelfareError::MissingReassessmentTrigger
            );
        }

        #[test]
        fn given_unreachable_condition_when_program_is_sealed_then_it_is_rejected() {
            let rules = given_rules();
            let mut program = create_fictional_restart_grant_program();
            program.conditions.push(WelfareProgramCondition {
                code: "orphanCondition".to_owned(),
                expression: WelfareExpression::Literal {
                    value: WelfareValue::Boolean(true),
                },
            });

            let result = rules.validate_program(&program);

            assert_eq!(result.unwrap_err(), WelfareError::UnreachableCondition);
        }
    }

    mod context_protocol_limits {
        use super::*;

        #[test]
        fn given_depth_thirteen_when_validated_then_depth_is_rejected() {
            let rules = given_rules();
            let mut expression = WelfareExpression::Literal {
                value: WelfareValue::Boolean(true),
            };
            for _ in 0..WELFARE_MAX_AST_DEPTH {
                expression = WelfareExpression::Not {
                    child: Box::new(expression),
                };
            }

            let result = validate_expression(&rules, &expression, &BTreeMap::new());

            assert_eq!(result.unwrap_err(), WelfareError::AstTooDeep);
        }

        #[test]
        fn given_depth_twelve_condition_below_root_when_sealed_then_combined_depth_is_rejected() {
            let rules = given_rules();
            let mut expression = WelfareExpression::Literal {
                value: WelfareValue::Boolean(true),
            };
            for _ in 1..WELFARE_MAX_AST_DEPTH {
                expression = WelfareExpression::Not {
                    child: Box::new(expression),
                };
            }
            let program = given_minimal_program(expression);

            let result = rules.validate_program(&program);

            assert_eq!(result.unwrap_err(), WelfareError::AstTooDeep);
        }

        #[test]
        fn given_empty_all_when_validated_then_arity_is_rejected() {
            let rules = given_rules();
            let expression = WelfareExpression::All { children: vec![] };

            let result = validate_expression(&rules, &expression, &BTreeMap::new());

            assert_eq!(result.unwrap_err(), WelfareError::InvalidLogicalArity);
        }

        #[test]
        fn given_seventeen_any_children_when_validated_then_arity_is_rejected() {
            let rules = given_rules();
            let expression = WelfareExpression::Any {
                children: vec![
                    WelfareExpression::Literal {
                        value: WelfareValue::Boolean(true),
                    };
                    WELFARE_MAX_LOGICAL_CHILDREN + 1
                ],
            };

            let result = validate_expression(&rules, &expression, &BTreeMap::new());

            assert_eq!(result.unwrap_err(), WelfareError::InvalidLogicalArity);
        }

        #[test]
        fn given_thirty_three_in_literals_when_validated_then_list_is_rejected() {
            let rules = given_rules();
            let expression = WelfareExpression::In {
                value: Box::new(WelfareExpression::Literal {
                    value: WelfareValue::Integer(0),
                }),
                literals: (0..=WELFARE_MAX_IN_LITERALS)
                    .map(|value| WelfareValue::Integer(value as i64))
                    .collect(),
            };

            let result = validate_expression(&rules, &expression, &BTreeMap::new());

            assert_eq!(result.unwrap_err(), WelfareError::InvalidInArity);
        }

        #[test]
        fn given_sixty_five_scalar_string_when_validated_then_string_is_rejected() {
            let rules = given_rules();
            let value = WelfareValue::String("가".repeat(WELFARE_MAX_STRING_SCALARS + 1));

            let result = validate_value(&rules, &value);

            assert_eq!(result.unwrap_err(), WelfareError::InvalidStringLiteral);
        }

        #[test]
        fn given_sixty_five_constants_when_validated_then_program_is_rejected() {
            let rules = given_rules();
            let mut program = given_minimal_program(WelfareExpression::Literal {
                value: WelfareValue::Boolean(true),
            });
            program
                .constants
                .extend(
                    (0..WELFARE_MAX_CONSTANTS).map(|index| WelfareProgramConstant {
                        key: format!("extraValue{index}"),
                        value: WelfareValue::Integer(index as i64),
                    }),
                );

            let result = rules.validate_program(&program);

            assert_eq!(result.unwrap_err(), WelfareError::TooManyConstants);
        }

        #[test]
        fn given_thirty_three_conditions_when_validated_then_program_is_rejected() {
            let rules = given_rules();
            let mut program = given_minimal_program(WelfareExpression::Literal {
                value: WelfareValue::Boolean(true),
            });
            program.conditions = (0..=WELFARE_MAX_CONDITIONS)
                .map(|index| WelfareProgramCondition {
                    code: format!("condition{index}"),
                    expression: WelfareExpression::Literal {
                        value: WelfareValue::Boolean(true),
                    },
                })
                .collect();

            let result = rules.validate_program(&program);

            assert_eq!(result.unwrap_err(), WelfareError::TooManyConditions);
        }

        #[test]
        fn given_more_than_one_hundred_twenty_eight_nodes_when_validated_then_program_is_rejected()
        {
            let rules = given_rules();
            let groups = (0..16)
                .map(|_| WelfareExpression::All {
                    children: (0..8)
                        .map(|_| WelfareExpression::Literal {
                            value: WelfareValue::Boolean(true),
                        })
                        .collect(),
                })
                .collect();
            let program = given_minimal_program(WelfareExpression::All { children: groups });

            let result = rules.validate_program(&program);

            assert_eq!(result.unwrap_err(), WelfareError::ProgramTooLarge);
        }

        #[test]
        fn given_thirty_three_public_facts_when_validated_then_program_is_rejected() {
            let rules = given_rules();
            let mut groups = Vec::new();
            for chunk in (1_u16..=33).collect::<Vec<_>>().chunks(11) {
                groups.push(WelfareExpression::All {
                    children: chunk
                        .iter()
                        .map(|days| WelfareExpression::Lte {
                            left: Box::new(WelfareExpression::Fact {
                                path: "income.periodTotal".to_owned(),
                                window: WelfareWindowSpec::PreviousClosedDays {
                                    days: WelfareWindowDays::Literal { days: *days },
                                },
                            }),
                            right: Box::new(WelfareExpression::Literal {
                                value: WelfareValue::MoneyKrw(i64::MAX),
                            }),
                        })
                        .collect(),
                });
            }
            let mut program = given_minimal_program(WelfareExpression::All { children: groups });
            program.reassessment_triggers = vec![WelfareFactSource::Income];

            let result = rules.validate_program(&program);

            assert_eq!(result.unwrap_err(), WelfareError::TooManyPublicFacts);
        }

        #[test]
        fn given_thirty_three_collection_rows_when_evaluated_then_collection_is_unknown() {
            let rules = given_rules();
            let program = given_sum_program();
            let values = vec![WelfareValue::MoneyKrw(0); WELFARE_MAX_COLLECTION_ROWS + 1];
            let collection = given_income_collection(values);
            let period = given_period_pin();
            let input = WelfareEvaluationInput {
                facts: &[],
                collections: &[collection],
                period_pin: &period,
            };

            let result = rules.evaluate_program(&program, &input).unwrap();

            assert_eq!(
                result.conditions[0].result,
                WelfareTruth::Unknown(WelfareUnknownReason::CollectionLimitExceeded)
            );
        }

        #[test]
        fn given_thirty_two_collection_rows_when_evaluated_then_collection_is_complete() {
            let rules = given_rules();
            let program = given_sum_program();
            let values = vec![WelfareValue::MoneyKrw(0); WELFARE_MAX_COLLECTION_ROWS];
            let collection = given_income_collection(values);
            let period = given_period_pin();
            let input = WelfareEvaluationInput {
                facts: &[],
                collections: &[collection],
                period_pin: &period,
            };

            let result = rules.evaluate_program(&program, &input).unwrap();

            assert_eq!(result.conditions[0].result, WelfareTruth::True);
        }
    }

    mod context_kleene_logic {
        use super::*;

        #[test]
        fn given_all_truth_combinations_when_evaluated_then_kleene_table_is_preserved() {
            let unknown = WelfareTruth::Unknown(WelfareUnknownReason::AuthorityMissing);
            let cases = vec![
                kleene_all(vec![WelfareTruth::True, WelfareTruth::True]),
                kleene_all(vec![WelfareTruth::True, unknown]),
                kleene_all(vec![WelfareTruth::False, unknown]),
            ];

            assert_eq!(
                cases,
                vec![WelfareTruth::True, unknown, WelfareTruth::False]
            );
        }

        #[test]
        fn given_any_truth_combinations_when_evaluated_then_kleene_table_is_preserved() {
            let unknown = WelfareTruth::Unknown(WelfareUnknownReason::AuthorityMissing);
            let cases = vec![
                kleene_any(vec![WelfareTruth::False, WelfareTruth::False]),
                kleene_any(vec![WelfareTruth::False, unknown]),
                kleene_any(vec![WelfareTruth::True, unknown]),
            ];

            assert_eq!(
                cases,
                vec![WelfareTruth::False, unknown, WelfareTruth::True]
            );
        }

        #[test]
        fn given_unknown_when_negated_then_reason_is_preserved() {
            let unknown = WelfareTruth::Unknown(WelfareUnknownReason::WindowIncomplete);

            let result = kleene_not(unknown);

            assert_eq!(result, unknown);
        }

        #[test]
        fn given_missing_asset_when_evaluated_then_eligibility_is_indeterminate() {
            let rules = given_rules();
            let program = create_fictional_restart_grant_program();
            let mut facts = given_eligible_facts();
            facts.retain(|fact| fact.key != "asset.policyValuation");
            let period = given_period_pin();
            let input = WelfareEvaluationInput {
                facts: &facts,
                collections: &[],
                period_pin: &period,
            };

            let result = rules.evaluate_program(&program, &input).unwrap();

            assert_eq!(result.status, WelfareEvaluationStatus::Indeterminate);
        }

        #[test]
        fn given_false_age_and_missing_asset_when_evaluated_then_false_dominates_unknown() {
            let rules = given_rules();
            let program = create_fictional_restart_grant_program();
            let mut facts = given_eligible_facts();
            replace_fact(&mut facts, "character.age", WelfareValue::AgeYears(21));
            facts.retain(|fact| fact.key != "asset.policyValuation");
            let period = given_period_pin();
            let input = WelfareEvaluationInput {
                facts: &facts,
                collections: &[],
                period_pin: &period,
            };

            let result = rules.evaluate_program(&program, &input).unwrap();

            assert_eq!(result.status, WelfareEvaluationStatus::Ineligible);
        }

        #[test]
        fn given_sum_over_i64_when_evaluated_then_overflow_is_unknown() {
            let rules = given_rules();
            let program = given_sum_program();
            let collection = given_income_collection(vec![
                WelfareValue::MoneyKrw(i64::MAX),
                WelfareValue::MoneyKrw(i64::MAX),
            ]);
            let period = given_period_pin();
            let input = WelfareEvaluationInput {
                facts: &[],
                collections: &[collection],
                period_pin: &period,
            };

            let result = rules.evaluate_program(&program, &input).unwrap();

            assert_eq!(
                result.conditions[0].result,
                WelfareTruth::Unknown(WelfareUnknownReason::ArithmeticOverflow)
            );
        }

        #[test]
        fn given_incomplete_collection_window_when_evaluated_then_reason_is_preserved() {
            let rules = given_rules();
            let program = given_sum_program();
            let collection = WelfareCollectionEvidence {
                key: "income.entries".to_owned(),
                item_type: WelfareValueType::MoneyKrw,
                window: WelfareResolvedWindow::PreviousClosedDays { days: 30 },
                value: WelfareCollectionEvidenceValue::Unknown(
                    WelfareUnknownReason::WindowIncomplete,
                ),
            };
            let period = given_period_pin();
            let input = WelfareEvaluationInput {
                facts: &[],
                collections: &[collection],
                period_pin: &period,
            };

            let result = rules.evaluate_program(&program, &input).unwrap();

            assert_eq!(
                result.conditions[0].result,
                WelfareTruth::Unknown(WelfareUnknownReason::WindowIncomplete)
            );
        }
    }

    mod context_fingerprint {
        use super::*;

        #[test]
        fn given_boolean_타입_when_canonicalize_then_boolean_단위를_보존한다() {
            let canonical_type = WelfareValueType::Boolean;

            let canonical = type_and_unit(&canonical_type);

            assert_eq!(canonical, ("boolean".to_owned(), "boolean".to_owned()));
        }

        #[test]
        fn given_integer_타입_when_canonicalize_then_integer_단위를_보존한다() {
            let canonical_type = WelfareValueType::Integer;

            let canonical = type_and_unit(&canonical_type);

            assert_eq!(canonical, ("integer".to_owned(), "integer".to_owned()));
        }

        #[test]
        fn given_same_facts_in_different_orders_when_hashed_then_fingerprint_is_identical() {
            let rules = given_rules();
            let facts = given_eligible_facts();
            let mut reordered_facts = facts.clone();
            reordered_facts.reverse();
            let period = given_period_pin();
            let mut reordered_period = period.clone();
            reordered_period.window_bounds.reverse();
            reordered_period.authority_revisions.reverse();

            let first = rules
                .fingerprint(&WelfareFingerprintInput {
                    schema_version: WELFARE_SCHEMA_VERSION,
                    program_version_id: ResourceId::from_u64(1),
                    facts: &facts,
                    collections: &[],
                    period_pin: &period,
                })
                .unwrap();
            let second = rules
                .fingerprint(&WelfareFingerprintInput {
                    schema_version: WELFARE_SCHEMA_VERSION,
                    program_version_id: ResourceId::from_u64(1),
                    facts: &reordered_facts,
                    collections: &[],
                    period_pin: &reordered_period,
                })
                .unwrap();

            assert_eq!(first, second);
        }

        #[test]
        fn given_same_facts_in_different_orders_when_canonicalized_then_json_is_identical() {
            let rules = given_rules();
            let facts = given_eligible_facts();
            let mut reordered_facts = facts.clone();
            reordered_facts.reverse();
            let period = given_period_pin();
            let mut reordered_period = period.clone();
            reordered_period.window_bounds.reverse();
            reordered_period.authority_revisions.reverse();

            let first = rules
                .canonical_fingerprint_json(&WelfareFingerprintInput {
                    schema_version: WELFARE_SCHEMA_VERSION,
                    program_version_id: ResourceId::from_u64(1),
                    facts: &facts,
                    collections: &[],
                    period_pin: &period,
                })
                .unwrap();
            let second = rules
                .canonical_fingerprint_json(&WelfareFingerprintInput {
                    schema_version: WELFARE_SCHEMA_VERSION,
                    program_version_id: ResourceId::from_u64(1),
                    facts: &reordered_facts,
                    collections: &[],
                    period_pin: &reordered_period,
                })
                .unwrap();

            assert_eq!(first, second);
        }

        #[test]
        fn given_one_changed_fact_when_hashed_then_fingerprint_changes() {
            let rules = given_rules();
            let facts = given_eligible_facts();
            let mut changed_facts = facts.clone();
            replace_fact(
                &mut changed_facts,
                "income.periodTotal",
                WelfareValue::MoneyKrw(1_234_568),
            );
            let period = given_period_pin();

            let first = rules
                .fingerprint(&WelfareFingerprintInput {
                    schema_version: WELFARE_SCHEMA_VERSION,
                    program_version_id: ResourceId::from_u64(1),
                    facts: &facts,
                    collections: &[],
                    period_pin: &period,
                })
                .unwrap();
            let second = rules
                .fingerprint(&WelfareFingerprintInput {
                    schema_version: WELFARE_SCHEMA_VERSION,
                    program_version_id: ResourceId::from_u64(1),
                    facts: &changed_facts,
                    collections: &[],
                    period_pin: &period,
                })
                .unwrap();

            assert_ne!(first, second);
        }

        #[test]
        fn given_fact_without_period_bound_when_hashed_then_pin_is_rejected() {
            let rules = given_rules();
            let facts = given_eligible_facts();
            let mut period = given_period_pin();
            period
                .window_bounds
                .retain(|bound| bound.window != WelfareResolvedWindow::PriorClose);

            let result = rules.fingerprint(&WelfareFingerprintInput {
                schema_version: WELFARE_SCHEMA_VERSION,
                program_version_id: ResourceId::from_u64(1),
                facts: &facts,
                collections: &[],
                period_pin: &period,
            });

            assert_eq!(result.unwrap_err(), WelfareError::InvalidPeriodPin);
        }

        #[test]
        fn given_collection_rows_in_different_orders_when_hashed_then_fingerprint_is_identical() {
            let rules = given_rules();
            let first_collection = given_income_collection(vec![
                WelfareValue::MoneyKrw(20),
                WelfareValue::MoneyKrw(10),
            ]);
            let second_collection = given_income_collection(vec![
                WelfareValue::MoneyKrw(10),
                WelfareValue::MoneyKrw(20),
            ]);
            let period = given_period_pin();

            let first = rules
                .fingerprint(&WelfareFingerprintInput {
                    schema_version: WELFARE_SCHEMA_VERSION,
                    program_version_id: ResourceId::from_u64(1),
                    facts: &[],
                    collections: &[first_collection],
                    period_pin: &period,
                })
                .unwrap();
            let second = rules
                .fingerprint(&WelfareFingerprintInput {
                    schema_version: WELFARE_SCHEMA_VERSION,
                    program_version_id: ResourceId::from_u64(1),
                    facts: &[],
                    collections: &[second_collection],
                    period_pin: &period,
                })
                .unwrap();

            assert_eq!(first, second);
        }

        #[test]
        fn given_canonical_fixture_when_hashed_then_digest_is_stable() {
            let rules = given_rules();
            let facts = given_eligible_facts();
            let period = given_period_pin();

            let result = rules
                .fingerprint(&WelfareFingerprintInput {
                    schema_version: WELFARE_SCHEMA_VERSION,
                    program_version_id: ResourceId::from_u64(1),
                    facts: &facts,
                    collections: &[],
                    period_pin: &period,
                })
                .unwrap();

            assert_eq!(
                result,
                "609c1023886ff41117474c685e3ed2607cf17d3ffc3e1eeef8034e6e800376b8"
            );
        }
    }

    mod context_fixture_boundaries {
        use super::*;

        #[test]
        fn given_age_boundaries_when_evaluated_then_twenty_two_through_sixty_seven_are_eligible() {
            let cases = [21, 22, 67, 68]
                .into_iter()
                .map(|age| {
                    let mut facts = given_eligible_facts();
                    replace_fact(&mut facts, "character.age", WelfareValue::AgeYears(age));
                    evaluate_fixture(facts)
                })
                .collect::<Vec<_>>();

            assert_eq!(
                cases,
                vec![
                    WelfareEvaluationStatus::Ineligible,
                    WelfareEvaluationStatus::Eligible,
                    WelfareEvaluationStatus::Eligible,
                    WelfareEvaluationStatus::Ineligible,
                ]
            );
        }

        #[test]
        fn given_employment_states_when_evaluated_then_none_and_ended_are_eligible() {
            let cases = ["none", "ended", "pendingStart", "active"]
                .into_iter()
                .map(|status| {
                    let mut facts = given_eligible_facts();
                    replace_fact(
                        &mut facts,
                        "career.employmentStatus",
                        welfare_enum("welfareEmployment", status),
                    );
                    evaluate_fixture(facts)
                })
                .collect::<Vec<_>>();

            assert_eq!(
                cases,
                vec![
                    WelfareEvaluationStatus::Eligible,
                    WelfareEvaluationStatus::Eligible,
                    WelfareEvaluationStatus::Ineligible,
                    WelfareEvaluationStatus::Ineligible,
                ]
            );
        }

        #[test]
        fn given_active_employment_with_dependent_when_evaluated_then_program_is_eligible() {
            let mut facts = given_eligible_facts();
            replace_fact(
                &mut facts,
                "career.employmentStatus",
                welfare_enum("welfareEmployment", "active"),
            );
            replace_fact(
                &mut facts,
                "household.dependentCount",
                WelfareValue::Count(1),
            );

            let result = evaluate_fixture(facts);

            assert_eq!(result, WelfareEvaluationStatus::Eligible);
        }

        #[test]
        fn given_income_cap_and_one_more_when_evaluated_then_only_cap_is_eligible() {
            let cases = [1_234_567, 1_234_568]
                .into_iter()
                .map(|income| {
                    let mut facts = given_eligible_facts();
                    replace_fact(
                        &mut facts,
                        "income.periodTotal",
                        WelfareValue::MoneyKrw(income),
                    );
                    evaluate_fixture(facts)
                })
                .collect::<Vec<_>>();

            assert_eq!(
                cases,
                vec![
                    WelfareEvaluationStatus::Eligible,
                    WelfareEvaluationStatus::Ineligible,
                ]
            );
        }

        #[test]
        fn given_asset_cap_and_one_more_when_evaluated_then_only_cap_is_eligible() {
            let cases = [12_345_678, 12_345_679]
                .into_iter()
                .map(|asset| {
                    let mut facts = given_eligible_facts();
                    replace_fact(
                        &mut facts,
                        "asset.policyValuation",
                        WelfareValue::MoneyKrw(asset),
                    );
                    evaluate_fixture(facts)
                })
                .collect::<Vec<_>>();

            assert_eq!(
                cases,
                vec![
                    WelfareEvaluationStatus::Eligible,
                    WelfareEvaluationStatus::Ineligible,
                ]
            );
        }

        #[test]
        fn given_no_residence_when_evaluated_then_program_is_ineligible() {
            let mut facts = given_eligible_facts();
            replace_fact(&mut facts, "residence.exists", WelfareValue::Boolean(false));

            let result = evaluate_fixture(facts);

            assert_eq!(result, WelfareEvaluationStatus::Ineligible);
        }

        #[test]
        fn given_serving_military_status_when_evaluated_then_program_is_ineligible() {
            let mut facts = given_eligible_facts();
            replace_fact(
                &mut facts,
                "military.status",
                welfare_enum("military", "serving"),
            );

            let result = evaluate_fixture(facts);

            assert_eq!(result, WelfareEvaluationStatus::Ineligible);
        }
    }

    fn given_rules() -> V1WelfareRules {
        V1WelfareRules {
            registry: create_fact_registry(),
        }
    }

    fn given_minimal_program(expression: WelfareExpression) -> WelfareProgramDefinition {
        WelfareProgramDefinition {
            schema_version: WELFARE_SCHEMA_VERSION,
            program_version_id: ResourceId::from_u64(1),
            program_key: "testProgram".to_owned(),
            purpose: WelfareProgramPurpose::GameBalance,
            ranked_availability: WelfareRankedAvailability::UnrankedOnly,
            duplicate_group_key: "testProgram".to_owned(),
            constants: vec![WelfareProgramConstant {
                key: "benefitKrw".to_owned(),
                value: WelfareValue::MoneyKrw(1),
            }],
            conditions: vec![WelfareProgramCondition {
                code: "condition".to_owned(),
                expression,
            }],
            eligibility_root: WelfareEligibilityExpression::All {
                children: vec![WelfareEligibilityExpression::Condition {
                    code: "condition".to_owned(),
                }],
            },
            benefit: WelfareBenefitDefinition {
                amount_constant_key: "benefitKrw".to_owned(),
                payment_delay_days: 1,
            },
            reassessment_triggers: vec![],
        }
    }

    fn given_sum_program() -> WelfareProgramDefinition {
        let expression = WelfareExpression::Lte {
            left: Box::new(WelfareExpression::Sum {
                collection: "income.entries".to_owned(),
                window: WelfareWindowSpec::PreviousClosedDays {
                    days: WelfareWindowDays::Literal { days: 30 },
                },
            }),
            right: Box::new(WelfareExpression::Literal {
                value: WelfareValue::MoneyKrw(i64::MAX),
            }),
        };
        let mut program = given_minimal_program(expression);
        program.reassessment_triggers = vec![WelfareFactSource::Income];
        program
    }

    fn given_income_collection(values: Vec<WelfareValue>) -> WelfareCollectionEvidence {
        WelfareCollectionEvidence {
            key: "income.entries".to_owned(),
            item_type: WelfareValueType::MoneyKrw,
            window: WelfareResolvedWindow::PreviousClosedDays { days: 30 },
            value: WelfareCollectionEvidenceValue::Known(values),
        }
    }

    fn given_eligible_facts() -> Vec<WelfareFactEvidence> {
        vec![
            evidence(
                "character.age",
                WelfareResolvedWindow::CurrentDay,
                WelfareValue::AgeYears(30),
            ),
            evidence(
                "career.employmentStatus",
                WelfareResolvedWindow::CurrentDay,
                welfare_enum("welfareEmployment", "none"),
            ),
            evidence(
                "household.dependentCount",
                WelfareResolvedWindow::CurrentDay,
                WelfareValue::Count(0),
            ),
            evidence(
                "income.periodTotal",
                WelfareResolvedWindow::PreviousClosedDays { days: 30 },
                WelfareValue::MoneyKrw(1_234_567),
            ),
            evidence(
                "asset.policyValuation",
                WelfareResolvedWindow::PriorClose,
                WelfareValue::MoneyKrw(12_345_678),
            ),
            evidence(
                "residence.exists",
                WelfareResolvedWindow::CurrentDay,
                WelfareValue::Boolean(true),
            ),
            evidence(
                "military.status",
                WelfareResolvedWindow::CurrentDay,
                welfare_enum("military", "completed"),
            ),
        ]
    }

    fn evidence(
        key: &str,
        window: WelfareResolvedWindow,
        value: WelfareValue,
    ) -> WelfareFactEvidence {
        WelfareFactEvidence {
            key: key.to_owned(),
            value_type: value.value_type(),
            window,
            value: WelfareEvidenceValue::Known(value),
        }
    }

    fn welfare_enum(schema_key: &str, value: &str) -> WelfareValue {
        WelfareValue::Enum(WelfareEnumValue {
            schema_key: schema_key.to_owned(),
            value: value.to_owned(),
        })
    }

    fn replace_fact(facts: &mut [WelfareFactEvidence], key: &str, value: WelfareValue) {
        let fact = facts
            .iter_mut()
            .find(|fact| fact.key == key)
            .expect("fixture fact must exist");
        fact.value_type = value.value_type();
        fact.value = WelfareEvidenceValue::Known(value);
    }

    fn given_period_pin() -> WelfarePeriodPin {
        WelfarePeriodPin {
            evaluation_game_day: 40,
            window_bounds: vec![
                WelfareWindowBound {
                    window: WelfareResolvedWindow::CurrentDay,
                    start_game_day: 40,
                    end_game_day: 40,
                },
                WelfareWindowBound {
                    window: WelfareResolvedWindow::PreviousClosedDays { days: 30 },
                    start_game_day: 10,
                    end_game_day: 39,
                },
                WelfareWindowBound {
                    window: WelfareResolvedWindow::PriorClose,
                    start_game_day: 39,
                    end_game_day: 39,
                },
            ],
            authority_revisions: vec![
                WelfareAuthorityRevision {
                    authority: "career".to_owned(),
                    revision: "7".to_owned(),
                },
                WelfareAuthorityRevision {
                    authority: "finance".to_owned(),
                    revision: "11".to_owned(),
                },
            ],
        }
    }

    fn evaluate_fixture(facts: Vec<WelfareFactEvidence>) -> WelfareEvaluationStatus {
        let rules = given_rules();
        let program = create_fictional_restart_grant_program();
        let period = given_period_pin();
        rules
            .evaluate_program(
                &program,
                &WelfareEvaluationInput {
                    facts: &facts,
                    collections: &[],
                    period_pin: &period,
                },
            )
            .expect("fixture evaluation must succeed")
            .status
    }
}
