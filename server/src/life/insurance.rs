use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::finance::ResourceId;

use super::types::*;

const ELIGIBILITY_FINGERPRINT_DOMAIN: &[u8] = b"lifeledger.insurance.eligibility.v1";
const CLAIM_PIN_DIGEST_DOMAIN: &[u8] = b"lifeledger.insurance.claim-contract-set.v1";

struct V1InsuranceRules {
    registry: InsuranceFactRegistry,
    hasher: Arc<dyn InsuranceClaimPinHasher>,
}

struct Sha256InsuranceClaimPinHasher;

/// Creates the sealed-schema v1 insurance rules with canonical SHA-256 fingerprints.
pub fn create_insurance_rules() -> Arc<dyn InsuranceRules> {
    create_insurance_rules_with_hasher(Arc::new(Sha256InsuranceClaimPinHasher))
}

/// Creates v1 insurance rules with a replaceable canonical-byte digest primitive.
pub fn create_insurance_rules_with_hasher(
    hasher: Arc<dyn InsuranceClaimPinHasher>,
) -> Arc<dyn InsuranceRules> {
    Arc::new(V1InsuranceRules {
        registry: create_fact_registry(),
        hasher,
    })
}

/// Creates the unranked family-care fixture specified by §7.8.
pub fn create_fictional_family_care_insurance_catalog() -> InsuranceCatalog {
    let registry = create_fact_registry();
    let current_fact = |path: &str, unit| LifeEventOperand::Fact {
        reference: LifeEventFactReference {
            path: path.to_owned(),
            unit,
            window: LifeEventWindowKind::CurrentGameDay,
        },
    };
    let literal = |unit, value| LifeEventOperand::Literal { unit, value };
    let root = LifeEventExpression::All {
        children: vec![
            LifeEventExpression::Between {
                value: Box::new(current_fact("character.age", LifeEventUnit::Years)),
                lower: Box::new(literal(
                    LifeEventUnit::Years,
                    LifeEventLiteralValue::AgeYears(22),
                )),
                upper: Box::new(literal(
                    LifeEventUnit::Years,
                    LifeEventLiteralValue::AgeYears(67),
                )),
            },
            LifeEventExpression::Gte {
                left: Box::new(current_fact(
                    "household.dependentCount",
                    LifeEventUnit::Count,
                )),
                right: Box::new(literal(
                    LifeEventUnit::Count,
                    LifeEventLiteralValue::Count(1),
                )),
            },
            LifeEventExpression::Fact {
                reference: LifeEventFactReference {
                    path: "residence.exists".to_owned(),
                    unit: LifeEventUnit::Boolean,
                    window: LifeEventWindowKind::CurrentGameDay,
                },
            },
            LifeEventExpression::Not {
                child: Box::new(LifeEventExpression::Eq {
                    left: Box::new(current_fact("military.status", LifeEventUnit::Enum)),
                    right: Box::new(literal(
                        LifeEventUnit::Enum,
                        LifeEventLiteralValue::Enum {
                            schema_key: "military".to_owned(),
                            value: "serving".to_owned(),
                        },
                    )),
                }),
            },
        ],
    };

    InsuranceCatalog {
        component_version_id: ResourceId::from_u64(1),
        component_version_key: "dev-unranked-m4-insurance-2026-v1".to_owned(),
        schema_version: INSURANCE_SCHEMA_VERSION,
        fact_registry_schema_version: INSURANCE_FACT_REGISTRY_SCHEMA_VERSION,
        facts: registry.facts,
        products: vec![InsuranceProductDefinition {
            product_version_id: ResourceId::from_u64(100),
            schema_version: INSURANCE_SCHEMA_VERSION,
            product_order: 1,
            product_key: "fictionalFamilyCareCover".to_owned(),
            display_name: "가족 돌봄 비용 보장".to_owned(),
            purpose: InsurancePurpose::GameBalance,
            ranked_availability: InsuranceRankedAvailability::UnrankedOnly,
            eligibility_ast: LifeEventEligibilityAst {
                version: INSURANCE_SCHEMA_VERSION,
                root,
            },
            ast_node_count: 13,
            ast_max_depth: 4,
            premium_krw: 10_000,
            premium_cadence_game_days: 30,
            term_game_days: 360,
            waiting_game_days: 7,
            claim_window_game_days: 7,
            grace_game_days: 0,
            reinstatement_allowed: false,
            automatic_renewal: false,
            coverages: vec![InsuranceCoverageDefinition {
                coverage_version_id: ResourceId::from_u64(101),
                coverage_order: 1,
                coverage_kind: InsuranceCoverageKind::FixedIndemnity,
                event_key: "fictionalDependentCareRequest".to_owned(),
                effect_kind: LifeEventEffectKind::FixedWalletExpense,
                deductible_krw: 20_000,
                occurrence_limit_krw: 100_000,
                term_limit_krw: 200_000,
            }],
        }],
    }
}

impl InsuranceClaimPinHasher for Sha256InsuranceClaimPinHasher {
    fn digest(&self, canonical_bytes: &[u8]) -> Result<[u8; 32], InsuranceError> {
        Ok(Sha256::digest(canonical_bytes).into())
    }
}

impl InsuranceRules for V1InsuranceRules {
    fn fact_registry(&self) -> &InsuranceFactRegistry {
        &self.registry
    }

    fn validate_catalog(&self, catalog: &InsuranceCatalog) -> Result<(), InsuranceError> {
        validate_catalog(catalog)
    }

    fn evaluate_eligibility(
        &self,
        input: InsuranceEligibilityInput<'_>,
    ) -> Result<InsuranceEligibilityEvaluation, InsuranceError> {
        validate_catalog(input.catalog)?;
        let product = find_product(input.catalog, input.product_version_id)?;
        let evidence = prepare_evidence(input.catalog, input.facts)?;
        let truth = evaluate_expression(&product.eligibility_ast.root, &evidence)?;
        let reasons = match truth {
            LifeEventTruth::True => Vec::new(),
            LifeEventTruth::False => {
                vec![InsuranceEligibilityReason::EligibilityExpressionFalse]
            }
            LifeEventTruth::Unknown(_) => input
                .catalog
                .facts
                .iter()
                .filter_map(|definition| {
                    let evidence = evidence.get(definition.fact_key.as_str())?;
                    let LifeEventEvidenceValue::Unknown(reason) = evidence else {
                        return None;
                    };
                    Some(InsuranceEligibilityReason::FactUnknown {
                        fact_key: definition.fact_key.clone(),
                        reason: *reason,
                    })
                })
                .collect(),
        };
        let status = match truth {
            LifeEventTruth::True => InsuranceEligibilityStatus::Eligible,
            LifeEventTruth::False => InsuranceEligibilityStatus::Ineligible,
            LifeEventTruth::Unknown(_) => InsuranceEligibilityStatus::Indeterminate,
        };
        let canonical = canonical_eligibility_fingerprint(input, &evidence)?;
        let fact_fingerprint = hex_digest(self.hasher.digest(&canonical)?);

        Ok(InsuranceEligibilityEvaluation {
            status,
            reasons,
            fact_fingerprint,
        })
    }

    fn plan_contract(
        &self,
        input: InsuranceContractPlanInput<'_>,
    ) -> Result<InsuranceContractPlan, InsuranceError> {
        plan_contract(input)
    }

    fn resolve_premium(
        &self,
        input: InsurancePremiumResolutionInput,
    ) -> Result<InsurancePremiumResolution, InsuranceError> {
        resolve_premium(input)
    }

    fn is_event_covered(&self, input: InsuranceCoverageInput) -> Result<bool, InsuranceError> {
        is_event_covered(input)
    }

    fn terminate_contract(
        &self,
        input: InsuranceTerminationInput,
    ) -> Result<InsuranceTerminationPlan, InsuranceError> {
        terminate_contract(input)
    }

    fn expire_contract(
        &self,
        input: InsuranceContractExpiryInput,
    ) -> Result<InsuranceContractExpiryPlan, InsuranceError> {
        expire_contract(input)
    }

    fn plan_claim_candidate(
        &self,
        input: InsuranceClaimCandidateInput<'_>,
    ) -> Result<InsuranceClaimCandidatePlan, InsuranceError> {
        plan_claim_candidate(self, input)
    }

    fn resolve_claim(
        &self,
        input: InsuranceClaimResolutionInput<'_>,
    ) -> Result<InsuranceClaimResolutionPlan, InsuranceError> {
        resolve_claim(input)
    }

    fn pay_claim(
        &self,
        input: InsuranceClaimPaymentInput<'_>,
    ) -> Result<InsuranceClaimPaymentPlan, InsuranceError> {
        pay_claim(input)
    }

    fn expire_claim(
        &self,
        input: InsuranceClaimExpiryInput<'_>,
    ) -> Result<InsuranceClaimExpiryPlan, InsuranceError> {
        expire_claim(input)
    }

    fn plan_premium_ledger(
        &self,
        input: InsurancePremiumLedgerInput,
    ) -> Result<InsuranceLedgerPlan, InsuranceError> {
        plan_premium_ledger(input)
    }

    fn plan_claim_ledger(
        &self,
        input: InsuranceClaimLedgerInput,
    ) -> Result<InsuranceLedgerPlan, InsuranceError> {
        plan_claim_ledger(input)
    }
}

fn create_fact_registry() -> InsuranceFactRegistry {
    InsuranceFactRegistry {
        schema_version: INSURANCE_FACT_REGISTRY_SCHEMA_VERSION,
        facts: vec![
            expected_fact(
                1,
                "character.age",
                LifeEventValueType::AgeYears,
                LifeEventUnit::Years,
                None,
                LifeEventFactSourceKind::GameDay,
            ),
            expected_fact(
                2,
                "household.dependentCount",
                LifeEventValueType::Count,
                LifeEventUnit::Count,
                None,
                LifeEventFactSourceKind::Household,
            ),
            expected_fact(
                3,
                "residence.exists",
                LifeEventValueType::Boolean,
                LifeEventUnit::Boolean,
                None,
                LifeEventFactSourceKind::Residence,
            ),
            expected_fact(
                4,
                "military.status",
                LifeEventValueType::Enum,
                LifeEventUnit::Enum,
                Some("military"),
                LifeEventFactSourceKind::Military,
            ),
        ],
    }
}

fn expected_fact(
    order: u8,
    key: &str,
    value_type: LifeEventValueType,
    unit: LifeEventUnit,
    enum_schema_key: Option<&str>,
    source_kind: LifeEventFactSourceKind,
) -> LifeEventFactDefinition {
    LifeEventFactDefinition {
        id: ResourceId::from_u64(u64::from(order)),
        fact_order: order,
        fact_key: key.to_owned(),
        value_type,
        unit,
        enum_schema_key: enum_schema_key.map(str::to_owned),
        window_kind: LifeEventWindowKind::CurrentGameDay,
        source_schema_version: INSURANCE_FACT_REGISTRY_SCHEMA_VERSION,
        source_kind,
    }
}

fn validate_catalog(catalog: &InsuranceCatalog) -> Result<(), InsuranceError> {
    if catalog.schema_version != INSURANCE_SCHEMA_VERSION
        || catalog.fact_registry_schema_version != INSURANCE_FACT_REGISTRY_SCHEMA_VERSION
    {
        return Err(InsuranceError::UnsupportedSchemaVersion);
    }
    if !is_component_version_key(&catalog.component_version_key) {
        return Err(InsuranceError::InvalidComponentVersionKey);
    }
    validate_fact_registry(&catalog.facts)?;
    if catalog.products.is_empty() || catalog.products.len() > INSURANCE_MAX_PRODUCTS {
        return Err(InsuranceError::InvalidCatalog);
    }

    let fact_map = catalog
        .facts
        .iter()
        .map(|fact| (fact.fact_key.as_str(), fact))
        .collect::<BTreeMap<_, _>>();
    let mut product_ids = BTreeSet::new();
    let mut product_keys = BTreeSet::new();
    let mut products = catalog.products.iter().collect::<Vec<_>>();
    products.sort_by(|left, right| left.product_key.cmp(&right.product_key));
    for (index, product) in products.into_iter().enumerate() {
        let expected_order =
            u8::try_from(index + 1).map_err(|_| InsuranceError::InvalidProductOrder)?;
        if product.product_order != expected_order {
            return Err(InsuranceError::InvalidProductOrder);
        }
        if !product_ids.insert(product.product_version_id)
            || !product_keys.insert(product.product_key.as_str())
        {
            return Err(InsuranceError::DuplicateProduct);
        }
        validate_product(product, &fact_map)?;
    }
    Ok(())
}

fn validate_fact_registry(facts: &[LifeEventFactDefinition]) -> Result<(), InsuranceError> {
    let expected = create_fact_registry();
    if facts.len() != expected.facts.len() {
        return Err(InsuranceError::InvalidFactRegistry);
    }
    let mut ids = BTreeSet::new();
    let mut keys = BTreeSet::new();
    let mut orders = BTreeSet::new();
    for (fact, expected) in facts.iter().zip(expected.facts) {
        if !ids.insert(fact.id) || !keys.insert(&fact.fact_key) || !orders.insert(fact.fact_order) {
            return Err(InsuranceError::DuplicateFact);
        }
        if fact.fact_order != expected.fact_order
            || fact.fact_key != expected.fact_key
            || fact.value_type != expected.value_type
            || fact.unit != expected.unit
            || fact.enum_schema_key != expected.enum_schema_key
            || fact.window_kind != LifeEventWindowKind::CurrentGameDay
            || fact.source_schema_version != INSURANCE_FACT_REGISTRY_SCHEMA_VERSION
            || fact.source_kind != expected.source_kind
        {
            return Err(InsuranceError::InvalidFactRegistry);
        }
    }
    Ok(())
}

fn validate_product(
    product: &InsuranceProductDefinition,
    facts: &BTreeMap<&str, &LifeEventFactDefinition>,
) -> Result<(), InsuranceError> {
    if product.schema_version != INSURANCE_SCHEMA_VERSION
        || product.eligibility_ast.version != INSURANCE_SCHEMA_VERSION
    {
        return Err(InsuranceError::UnsupportedSchemaVersion);
    }
    if !is_canonical_key(&product.product_key) {
        return Err(InsuranceError::InvalidCanonicalKey);
    }
    if !is_display_name(&product.display_name, 80) {
        return Err(InsuranceError::InvalidDisplayName);
    }
    if product.purpose != InsurancePurpose::GameBalance
        || product.ranked_availability != InsuranceRankedAvailability::UnrankedOnly
    {
        return Err(InsuranceError::InvalidCatalog);
    }
    validate_product_terms(product)?;
    let analysis = analyze_expression(&product.eligibility_ast.root, facts)?;
    if analysis.value_type != LifeEventValueType::Boolean {
        return Err(InsuranceError::InvalidEligibilityRoot);
    }
    if usize::from(product.ast_node_count) != analysis.nodes
        || usize::from(product.ast_max_depth) != analysis.depth
    {
        return Err(InsuranceError::AstProjectionMismatch);
    }
    if product.coverages.is_empty() || product.coverages.len() > INSURANCE_MAX_COVERAGES_PER_PRODUCT
    {
        return Err(InsuranceError::InvalidCoverage);
    }
    let mut ids = BTreeSet::new();
    let mut identities = BTreeSet::new();
    let mut coverages = product.coverages.iter().collect::<Vec<_>>();
    coverages.sort_by(|left, right| {
        left.event_key.cmp(&right.event_key).then_with(|| {
            effect_kind_order(left.effect_kind).cmp(&effect_kind_order(right.effect_kind))
        })
    });
    for (index, coverage) in coverages.into_iter().enumerate() {
        let expected_order =
            u8::try_from(index + 1).map_err(|_| InsuranceError::InvalidCoverageOrder)?;
        if coverage.coverage_order != expected_order {
            return Err(InsuranceError::InvalidCoverageOrder);
        }
        if !ids.insert(coverage.coverage_version_id)
            || !identities.insert((&coverage.event_key, effect_kind_order(coverage.effect_kind)))
        {
            return Err(InsuranceError::DuplicateCoverage);
        }
        validate_coverage(coverage)?;
    }
    Ok(())
}

fn validate_product_terms(product: &InsuranceProductDefinition) -> Result<(), InsuranceError> {
    if !(1..=INSURANCE_MAX_MONEY_KRW).contains(&product.premium_krw)
        || product.premium_cadence_game_days == 0
        || product.term_game_days == 0
        || !product
            .term_game_days
            .is_multiple_of(product.premium_cadence_game_days)
        || product.waiting_game_days >= product.term_game_days
        || product.claim_window_game_days == 0
        || product.grace_game_days != 0
        || product.reinstatement_allowed
        || product.automatic_renewal
    {
        return Err(InsuranceError::InvalidProductTerms);
    }
    Ok(())
}

fn validate_coverage(coverage: &InsuranceCoverageDefinition) -> Result<(), InsuranceError> {
    if coverage.coverage_kind != InsuranceCoverageKind::FixedIndemnity
        || coverage.effect_kind != LifeEventEffectKind::FixedWalletExpense
        || !is_canonical_key(&coverage.event_key)
        || coverage.deductible_krw < 0
        || coverage.deductible_krw > INSURANCE_MAX_MONEY_KRW
        || !(1..=INSURANCE_MAX_MONEY_KRW).contains(&coverage.occurrence_limit_krw)
        || !(coverage.occurrence_limit_krw..=INSURANCE_MAX_MONEY_KRW)
            .contains(&coverage.term_limit_krw)
    {
        return Err(InsuranceError::InvalidCoverage);
    }
    Ok(())
}

const fn effect_kind_order(kind: LifeEventEffectKind) -> u8 {
    match kind {
        LifeEventEffectKind::NoEffect => 0,
        LifeEventEffectKind::FixedWalletExpense => 1,
    }
}

#[derive(Debug)]
struct ExpressionAnalysis {
    value_type: LifeEventValueType,
    enum_schema_key: Option<String>,
    nodes: usize,
    depth: usize,
}

impl ExpressionAnalysis {
    fn boolean_node(children: &[Self]) -> Result<Self, InsuranceError> {
        let child_nodes = children.iter().try_fold(0_usize, |total, child| {
            total
                .checked_add(child.nodes)
                .ok_or(InsuranceError::AstTooLarge)
        })?;
        let nodes = child_nodes
            .checked_add(1)
            .ok_or(InsuranceError::AstTooLarge)?;
        let depth = children
            .iter()
            .map(|child| child.depth)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(InsuranceError::AstTooDeep)?;
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
) -> Result<ExpressionAnalysis, InsuranceError> {
    let analysis = match expression {
        LifeEventExpression::All { children } | LifeEventExpression::Any { children } => {
            if children.is_empty() || children.len() > INSURANCE_MAX_LOGICAL_CHILDREN {
                return Err(InsuranceError::InvalidLogicalArity);
            }
            let analyses = children
                .iter()
                .map(|child| analyze_expression(child, facts))
                .collect::<Result<Vec<_>, _>>()?;
            if analyses
                .iter()
                .any(|child| child.value_type != LifeEventValueType::Boolean)
            {
                return Err(InsuranceError::TypeMismatch);
            }
            ExpressionAnalysis::boolean_node(&analyses)?
        }
        LifeEventExpression::Not { child } => {
            let child = analyze_expression(child, facts)?;
            if child.value_type != LifeEventValueType::Boolean {
                return Err(InsuranceError::TypeMismatch);
            }
            ExpressionAnalysis::boolean_node(&[child])?
        }
        LifeEventExpression::Eq { left, right } | LifeEventExpression::Gte { left, right } => {
            let left = analyze_operand(left, facts)?;
            let right = analyze_operand(right, facts)?;
            ensure_same_type(&left, &right)?;
            if matches!(expression, LifeEventExpression::Gte { .. }) {
                ensure_ordered(left.value_type)?;
            }
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
                return Err(InsuranceError::InvalidBetweenBounds);
            }
            ExpressionAnalysis::boolean_node(&[value_analysis, lower_analysis, upper_analysis])?
        }
        LifeEventExpression::Fact { reference } => {
            let analysis = analyze_fact_reference(reference, facts)?;
            if analysis.value_type != LifeEventValueType::Boolean {
                return Err(InsuranceError::TypeMismatch);
            }
            analysis
        }
    };
    if analysis.nodes > INSURANCE_MAX_AST_NODES {
        return Err(InsuranceError::AstTooLarge);
    }
    if analysis.depth > INSURANCE_MAX_AST_DEPTH {
        return Err(InsuranceError::AstTooDeep);
    }
    Ok(analysis)
}

fn analyze_operand(
    operand: &LifeEventOperand,
    facts: &BTreeMap<&str, &LifeEventFactDefinition>,
) -> Result<ExpressionAnalysis, InsuranceError> {
    match operand {
        LifeEventOperand::Fact { reference } => analyze_fact_reference(reference, facts),
        LifeEventOperand::Literal { unit, value } => analyze_literal(*unit, value),
    }
}

fn analyze_fact_reference(
    reference: &LifeEventFactReference,
    facts: &BTreeMap<&str, &LifeEventFactDefinition>,
) -> Result<ExpressionAnalysis, InsuranceError> {
    let fact = facts
        .get(reference.path.as_str())
        .ok_or(InsuranceError::UnknownFact)?;
    if reference.unit != fact.unit || reference.window != fact.window_kind {
        return Err(InsuranceError::UnitMismatch);
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
) -> Result<ExpressionAnalysis, InsuranceError> {
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
        _ => return Err(InsuranceError::InvalidLiteral),
    };
    if unit != expected_unit {
        return Err(InsuranceError::UnitMismatch);
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
) -> Result<(), InsuranceError> {
    if left.value_type == right.value_type && left.enum_schema_key == right.enum_schema_key {
        Ok(())
    } else {
        Err(InsuranceError::TypeMismatch)
    }
}

fn ensure_ordered(value_type: LifeEventValueType) -> Result<(), InsuranceError> {
    if matches!(
        value_type,
        LifeEventValueType::Count | LifeEventValueType::AgeYears
    ) {
        Ok(())
    } else {
        Err(InsuranceError::UnorderedType)
    }
}

fn literal_value(operand: &LifeEventOperand) -> Option<LifeEventValue> {
    let LifeEventOperand::Literal { value, .. } = operand else {
        return None;
    };
    Some(match value {
        LifeEventLiteralValue::Boolean(value) => LifeEventValue::Boolean(*value),
        LifeEventLiteralValue::Count(value) => LifeEventValue::Count(*value),
        LifeEventLiteralValue::AgeYears(value) => LifeEventValue::AgeYears(*value),
        LifeEventLiteralValue::Enum { schema_key, value } => LifeEventValue::Enum {
            schema_key: schema_key.clone(),
            value: value.clone(),
        },
    })
}

fn validate_enum(schema_key: &str, value: &str) -> Result<(), InsuranceError> {
    if schema_key == "military" && matches!(value, "unserved" | "serving" | "completed" | "exempt")
    {
        Ok(())
    } else {
        Err(InsuranceError::UnknownEnum)
    }
}

fn prepare_evidence<'a>(
    catalog: &InsuranceCatalog,
    evidence: &'a [LifeEventFactEvidence],
) -> Result<BTreeMap<&'a str, &'a LifeEventEvidenceValue>, InsuranceError> {
    if evidence.len() != catalog.facts.len() {
        return Err(InsuranceError::InvalidEvidence);
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
            .ok_or(InsuranceError::InvalidEvidence)?;
        validate_evidence_value(definition, &fact.value)?;
        if values.insert(fact.fact_key.as_str(), &fact.value).is_some() {
            return Err(InsuranceError::InvalidEvidence);
        }
    }
    if definitions.keys().any(|key| !values.contains_key(key)) {
        return Err(InsuranceError::InvalidEvidence);
    }
    Ok(values)
}

fn validate_evidence_value(
    definition: &LifeEventFactDefinition,
    evidence: &LifeEventEvidenceValue,
) -> Result<(), InsuranceError> {
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
        _ => Err(InsuranceError::InvalidEvidence),
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
) -> Result<LifeEventTruth, InsuranceError> {
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
        LifeEventExpression::Fact { reference } => match facts
            .get(reference.path.as_str())
            .ok_or(InsuranceError::InvalidEvidence)?
        {
            LifeEventEvidenceValue::Known(LifeEventValue::Boolean(true)) => {
                Ok(LifeEventTruth::True)
            }
            LifeEventEvidenceValue::Known(LifeEventValue::Boolean(false)) => {
                Ok(LifeEventTruth::False)
            }
            LifeEventEvidenceValue::Unknown(reason) => Ok(LifeEventTruth::Unknown(*reason)),
            LifeEventEvidenceValue::Known(_) => Err(InsuranceError::TypeMismatch),
        },
    }
}

fn evaluate_binary(
    left: &LifeEventOperand,
    right: &LifeEventOperand,
    facts: &BTreeMap<&str, &LifeEventEvidenceValue>,
    operation: impl FnOnce(&LifeEventValue, &LifeEventValue) -> Result<bool, InsuranceError>,
) -> Result<LifeEventTruth, InsuranceError> {
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
) -> Result<EvaluatedValue, InsuranceError> {
    match operand {
        LifeEventOperand::Fact { reference } => match facts
            .get(reference.path.as_str())
            .ok_or(InsuranceError::InvalidEvidence)?
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
) -> Result<Ordering, InsuranceError> {
    match (left, right) {
        (LifeEventValue::Count(left), LifeEventValue::Count(right))
        | (LifeEventValue::AgeYears(left), LifeEventValue::AgeYears(right)) => Ok(left.cmp(right)),
        _ => Err(InsuranceError::UnorderedType),
    }
}

fn canonical_eligibility_fingerprint(
    input: InsuranceEligibilityInput<'_>,
    evidence: &BTreeMap<&str, &LifeEventEvidenceValue>,
) -> Result<Vec<u8>, InsuranceError> {
    let mut output = Vec::new();
    push_bytes(&mut output, ELIGIBILITY_FINGERPRINT_DOMAIN)?;
    output.extend_from_slice(&input.catalog.component_version_id.get().to_be_bytes());
    push_bytes(&mut output, input.catalog.component_version_key.as_bytes())?;
    output.extend_from_slice(&input.product_version_id.get().to_be_bytes());
    output.extend_from_slice(&input.evaluation_game_day.to_be_bytes());
    for definition in &input.catalog.facts {
        push_bytes(&mut output, definition.fact_key.as_bytes())?;
        let value = evidence
            .get(definition.fact_key.as_str())
            .ok_or(InsuranceError::InvalidEvidence)?;
        push_evidence(&mut output, value)?;
    }
    Ok(output)
}

fn push_evidence(
    output: &mut Vec<u8>,
    evidence: &LifeEventEvidenceValue,
) -> Result<(), InsuranceError> {
    match evidence {
        LifeEventEvidenceValue::Known(LifeEventValue::Boolean(value)) => {
            output.extend_from_slice(&[0, u8::from(*value)]);
        }
        LifeEventEvidenceValue::Known(LifeEventValue::Count(value)) => {
            output.push(1);
            output.extend_from_slice(&value.to_be_bytes());
        }
        LifeEventEvidenceValue::Known(LifeEventValue::AgeYears(value)) => {
            output.push(2);
            output.extend_from_slice(&value.to_be_bytes());
        }
        LifeEventEvidenceValue::Known(LifeEventValue::Enum { schema_key, value }) => {
            output.push(3);
            push_bytes(output, schema_key.as_bytes())?;
            push_bytes(output, value.as_bytes())?;
        }
        LifeEventEvidenceValue::Unknown(reason) => {
            output.push(4);
            output.push(match reason {
                LifeEventUnknownReason::AuthorityMissing => 0,
                LifeEventUnknownReason::CollectionLimitExceeded => 1,
                LifeEventUnknownReason::ArithmeticOverflow => 2,
            });
        }
    }
    Ok(())
}

fn plan_contract(
    input: InsuranceContractPlanInput<'_>,
) -> Result<InsuranceContractPlan, InsuranceError> {
    validate_product_terms(input.product)?;
    let waiting_ends_game_day = input
        .start_game_day
        .checked_add(u32::from(input.product.waiting_game_days))
        .ok_or(InsuranceError::ArithmeticOverflow)?;
    let coverage_end_exclusive = input
        .start_game_day
        .checked_add(u32::from(input.product.term_game_days))
        .ok_or(InsuranceError::ArithmeticOverflow)?;
    let charge_count = input.product.term_game_days / input.product.premium_cadence_game_days;
    let mut premium_charges = Vec::with_capacity(usize::from(charge_count));
    for index in 0..charge_count {
        let offset = u32::from(index)
            .checked_mul(u32::from(input.product.premium_cadence_game_days))
            .ok_or(InsuranceError::ArithmeticOverflow)?;
        let due_game_day = input
            .start_game_day
            .checked_add(offset)
            .ok_or(InsuranceError::ArithmeticOverflow)?;
        premium_charges.push(InsurancePremiumChargePlan {
            charge_no: index
                .checked_add(1)
                .ok_or(InsuranceError::ArithmeticOverflow)?,
            due_game_day,
            amount_krw: input.product.premium_krw,
            status: if index == 0 {
                InsurancePremiumChargeStatus::Paid
            } else {
                InsurancePremiumChargeStatus::Scheduled
            },
        });
    }

    Ok(InsuranceContractPlan {
        contract_id: input.contract_id,
        product_version_id: input.product.product_version_id,
        status: InsuranceContractStatus::Active,
        coverage_start_game_day: input.start_game_day,
        waiting_ends_game_day,
        coverage_end_exclusive,
        premium_charges,
    })
}

fn resolve_premium(
    input: InsurancePremiumResolutionInput,
) -> Result<InsurancePremiumResolution, InsuranceError> {
    if input.charge_no == 0 || !(1..=INSURANCE_MAX_MONEY_KRW).contains(&input.premium_krw) {
        return Err(InsuranceError::InvalidPremiumCharge);
    }
    if input.wallet_cash_krw < 0 {
        return Err(InsuranceError::InvalidPremiumCharge);
    }
    if input.wallet_cash_krw >= input.premium_krw {
        let wallet_cash_after_krw = input
            .wallet_cash_krw
            .checked_sub(input.premium_krw)
            .ok_or(InsuranceError::ArithmeticOverflow)?;
        Ok(InsurancePremiumResolution {
            contract_id: input.contract_id,
            charge_no: input.charge_no,
            charge_status: InsurancePremiumChargeStatus::Paid,
            contract_status: InsuranceContractStatus::Active,
            paid_krw: input.premium_krw,
            wallet_cash_before_krw: input.wallet_cash_krw,
            wallet_cash_after_krw,
            coverage_end_exclusive: None,
            cancel_future_charges: false,
        })
    } else {
        Ok(InsurancePremiumResolution {
            contract_id: input.contract_id,
            charge_no: input.charge_no,
            charge_status: InsurancePremiumChargeStatus::Missed,
            contract_status: InsuranceContractStatus::Lapsed,
            paid_krw: 0,
            wallet_cash_before_krw: input.wallet_cash_krw,
            wallet_cash_after_krw: input.wallet_cash_krw,
            coverage_end_exclusive: Some(
                input
                    .due_game_day
                    .checked_add(1)
                    .ok_or(InsuranceError::ArithmeticOverflow)?,
            ),
            cancel_future_charges: true,
        })
    }
}

fn is_event_covered(input: InsuranceCoverageInput) -> Result<bool, InsuranceError> {
    if input.coverage_end_exclusive <= input.coverage_start_game_day
        || input.waiting_ends_game_day < input.coverage_start_game_day
    {
        return Err(InsuranceError::InvalidCoverageWindow);
    }
    Ok(input.event_offered_game_day >= input.waiting_ends_game_day
        && input.event_offered_game_day >= input.coverage_start_game_day
        && input.event_offered_game_day < input.coverage_end_exclusive)
}

fn terminate_contract(
    input: InsuranceTerminationInput,
) -> Result<InsuranceTerminationPlan, InsuranceError> {
    if input.current_coverage_end_exclusive <= input.coverage_start_game_day
        || input.effective_game_day < input.coverage_start_game_day
        || input.effective_game_day >= input.current_coverage_end_exclusive
    {
        return Err(InsuranceError::InvalidContractState);
    }
    let coverage_end_exclusive = input
        .effective_game_day
        .checked_add(1)
        .ok_or(InsuranceError::ArithmeticOverflow)?
        .min(input.current_coverage_end_exclusive);
    let status = match input.kind {
        InsuranceTerminationKind::Lapse => InsuranceContractStatus::Lapsed,
        InsuranceTerminationKind::Cancellation => InsuranceContractStatus::Cancelled,
    };
    Ok(InsuranceTerminationPlan {
        contract_id: input.contract_id,
        status,
        kind: input.kind,
        effective_game_day: input.effective_game_day,
        coverage_end_exclusive,
        cancel_future_charges: true,
    })
}

fn expire_contract(
    input: InsuranceContractExpiryInput,
) -> Result<InsuranceContractExpiryPlan, InsuranceError> {
    if input.current_status != InsuranceContractStatus::Active {
        return Err(InsuranceError::InvalidContractState);
    }
    if input.target_game_day < input.coverage_end_exclusive {
        return Err(InsuranceError::ContractNotExpired);
    }
    Ok(InsuranceContractExpiryPlan {
        contract_id: input.contract_id,
        status: InsuranceContractStatus::Expired,
        expired_game_day: input.coverage_end_exclusive,
        coverage_end_exclusive: input.coverage_end_exclusive,
        cancel_future_charges: true,
    })
}

fn plan_claim_candidate(
    rules: &V1InsuranceRules,
    input: InsuranceClaimCandidateInput<'_>,
) -> Result<InsuranceClaimCandidatePlan, InsuranceError> {
    if input.matching_contracts.len() > INSURANCE_MAX_CLAIM_CONTRACTS {
        return Err(InsuranceError::ClaimContractLimitExceeded);
    }
    let mut contract_pins = input.matching_contracts.to_vec();
    contract_pins.sort_by_key(|pin| pin.contract_id);
    let mut ids = BTreeSet::new();
    for pin in &contract_pins {
        if !ids.insert(pin.contract_id) {
            return Err(InsuranceError::DuplicateContract);
        }
        validate_claim_pin(pin)?;
        if input.offered_game_day < pin.coverage_start_game_day
            || input.offered_game_day >= pin.coverage_end_exclusive
            || pin.waiting_passed != (input.offered_game_day >= pin.waiting_ends_game_day)
        {
            return Err(InsuranceError::InvalidClaimPin);
        }
    }
    let canonical = canonical_claim_pins(&contract_pins)?;
    let contract_set_digest = hex_digest(rules.hasher.digest(&canonical)?);
    Ok(InsuranceClaimCandidatePlan {
        claim_id: input.claim_id,
        event_instance_id: input.event_instance_id,
        offered_game_day: input.offered_game_day,
        status: InsuranceClaimStatus::Candidate,
        contract_set_digest,
        contract_pins,
    })
}

fn validate_claim_pin(pin: &InsuranceClaimContractPin) -> Result<(), InsuranceError> {
    if pin.coverage_end_exclusive <= pin.coverage_start_game_day
        || pin.waiting_ends_game_day < pin.coverage_start_game_day
        || pin.deductible_krw < 0
        || !(1..=INSURANCE_MAX_MONEY_KRW).contains(&pin.occurrence_limit_krw)
        || !(pin.occurrence_limit_krw..=INSURANCE_MAX_MONEY_KRW).contains(&pin.term_limit_krw)
        || pin.paid_krw < 0
        || pin.reserved_krw < 0
    {
        return Err(InsuranceError::InvalidClaimPin);
    }
    let used = i128::from(pin.paid_krw)
        .checked_add(i128::from(pin.reserved_krw))
        .ok_or(InsuranceError::ArithmeticOverflow)?;
    if used > i128::from(pin.term_limit_krw) {
        return Err(InsuranceError::InvalidTermUsage);
    }
    Ok(())
}

fn canonical_claim_pins(pins: &[InsuranceClaimContractPin]) -> Result<Vec<u8>, InsuranceError> {
    let mut output = Vec::new();
    push_bytes(&mut output, CLAIM_PIN_DIGEST_DOMAIN)?;
    let count =
        u16::try_from(pins.len()).map_err(|_| InsuranceError::ClaimContractLimitExceeded)?;
    output.extend_from_slice(&count.to_be_bytes());
    for pin in pins {
        output.extend_from_slice(&pin.contract_id.get().to_be_bytes());
        output.extend_from_slice(&pin.product_version_id.get().to_be_bytes());
        output.extend_from_slice(&pin.coverage_version_id.get().to_be_bytes());
        output.extend_from_slice(&pin.coverage_start_game_day.to_be_bytes());
        output.extend_from_slice(&pin.waiting_ends_game_day.to_be_bytes());
        output.extend_from_slice(&pin.coverage_end_exclusive.to_be_bytes());
        output.push(u8::from(pin.waiting_passed));
        output.extend_from_slice(&pin.deductible_krw.to_be_bytes());
        output.extend_from_slice(&pin.occurrence_limit_krw.to_be_bytes());
        output.extend_from_slice(&pin.term_limit_krw.to_be_bytes());
        output.extend_from_slice(&pin.paid_krw.to_be_bytes());
        output.extend_from_slice(&pin.reserved_krw.to_be_bytes());
    }
    Ok(output)
}

fn resolve_claim(
    input: InsuranceClaimResolutionInput<'_>,
) -> Result<InsuranceClaimResolutionPlan, InsuranceError> {
    if input.current_status != InsuranceClaimStatus::Candidate {
        return Err(InsuranceError::InvalidClaimTransition);
    }
    match input.resolution_kind {
        InsuranceClaimResolutionKind::NoEffect => {
            if input.gross_cost_krw.is_some() {
                return Err(InsuranceError::InvalidClaimAmount);
            }
            Ok(InsuranceClaimResolutionPlan {
                claim_id: input.claim_id,
                status: InsuranceClaimStatus::NotApplicable,
                resolved_game_day: input.resolved_game_day,
                gross_cost_krw: None,
                payout_krw: 0,
                filing_deadline_game_day: None,
                allocations: Vec::new(),
                contract_aggregates: Vec::new(),
            })
        }
        InsuranceClaimResolutionKind::FixedWalletExpense => {
            let gross_cost_krw = input
                .gross_cost_krw
                .filter(|amount| (1..=INSURANCE_MAX_MONEY_KRW).contains(amount))
                .ok_or(InsuranceError::InvalidClaimAmount)?;
            if input.claim_window_game_days == 0
                || input.contract_pins.len() > INSURANCE_MAX_CLAIM_CONTRACTS
            {
                return Err(InsuranceError::InvalidClaimPin);
            }
            let mut pins = input.contract_pins.to_vec();
            pins.sort_by_key(|pin| pin.contract_id);
            let mut ids = BTreeSet::new();
            let mut allocated = 0_i128;
            let mut allocations = Vec::new();
            let mut contract_aggregates = Vec::new();
            for pin in pins {
                if !ids.insert(pin.contract_id) {
                    return Err(InsuranceError::DuplicateContract);
                }
                validate_claim_pin(&pin)?;
                if !pin.waiting_passed {
                    continue;
                }
                let gross = i128::from(gross_cost_krw);
                let deductible = i128::from(pin.deductible_krw);
                let covered_loss = gross
                    .checked_sub(deductible)
                    .ok_or(InsuranceError::ArithmeticOverflow)?
                    .max(0);
                let used = i128::from(pin.paid_krw)
                    .checked_add(i128::from(pin.reserved_krw))
                    .ok_or(InsuranceError::ArithmeticOverflow)?;
                let remaining_term = i128::from(pin.term_limit_krw)
                    .checked_sub(used)
                    .ok_or(InsuranceError::ArithmeticOverflow)?;
                if remaining_term < 0 {
                    return Err(InsuranceError::InvalidTermUsage);
                }
                let raw = covered_loss
                    .min(i128::from(pin.occurrence_limit_krw))
                    .min(remaining_term);
                let remaining_loss = gross
                    .checked_sub(allocated)
                    .ok_or(InsuranceError::ArithmeticOverflow)?;
                if remaining_loss < 0 {
                    return Err(InsuranceError::InvalidClaimAmount);
                }
                let allocation = raw.min(remaining_loss);
                if allocation == 0 {
                    continue;
                }
                allocated = allocated
                    .checked_add(allocation)
                    .ok_or(InsuranceError::ArithmeticOverflow)?;
                let raw_krw = i64::try_from(raw).map_err(|_| InsuranceError::ArithmeticOverflow)?;
                let allocation_krw =
                    i64::try_from(allocation).map_err(|_| InsuranceError::ArithmeticOverflow)?;
                let reserved_after_krw = pin
                    .reserved_krw
                    .checked_add(allocation_krw)
                    .ok_or(InsuranceError::ArithmeticOverflow)?;
                allocations.push(InsuranceClaimAllocation {
                    contract_id: pin.contract_id,
                    deductible_krw: pin.deductible_krw,
                    raw_krw,
                    allocation_krw,
                });
                contract_aggregates.push(InsuranceClaimContractAggregatePlan {
                    contract_id: pin.contract_id,
                    paid_before_krw: pin.paid_krw,
                    paid_after_krw: pin.paid_krw,
                    reserved_before_krw: pin.reserved_krw,
                    reserved_after_krw,
                });
            }
            if allocations.is_empty() {
                return Ok(InsuranceClaimResolutionPlan {
                    claim_id: input.claim_id,
                    status: InsuranceClaimStatus::NotCovered,
                    resolved_game_day: input.resolved_game_day,
                    gross_cost_krw: Some(gross_cost_krw),
                    payout_krw: 0,
                    filing_deadline_game_day: None,
                    allocations,
                    contract_aggregates,
                });
            }
            let payout_krw =
                i64::try_from(allocated).map_err(|_| InsuranceError::ArithmeticOverflow)?;
            let filing_deadline_game_day = input
                .resolved_game_day
                .checked_add(u32::from(input.claim_window_game_days))
                .ok_or(InsuranceError::ArithmeticOverflow)?;
            Ok(InsuranceClaimResolutionPlan {
                claim_id: input.claim_id,
                status: InsuranceClaimStatus::Ready,
                resolved_game_day: input.resolved_game_day,
                gross_cost_krw: Some(gross_cost_krw),
                payout_krw,
                filing_deadline_game_day: Some(filing_deadline_game_day),
                allocations,
                contract_aggregates,
            })
        }
    }
}

fn pay_claim(
    input: InsuranceClaimPaymentInput<'_>,
) -> Result<InsuranceClaimPaymentPlan, InsuranceError> {
    if input.current_status != InsuranceClaimStatus::Ready {
        return Err(InsuranceError::InvalidClaimTransition);
    }
    if input.current_game_day >= input.filing_deadline_game_day {
        return Err(InsuranceError::ClaimExpired);
    }
    let (payout_krw, contract_aggregates) = finalize_claim_contracts(input.contracts, true)?;
    Ok(InsuranceClaimPaymentPlan {
        claim_id: input.claim_id,
        status: InsuranceClaimStatus::Paid,
        paid_game_day: input.current_game_day,
        payout_krw,
        contract_aggregates,
    })
}

fn expire_claim(
    input: InsuranceClaimExpiryInput<'_>,
) -> Result<InsuranceClaimExpiryPlan, InsuranceError> {
    if input.current_status != InsuranceClaimStatus::Ready {
        return Err(InsuranceError::InvalidClaimTransition);
    }
    if input.current_game_day < input.filing_deadline_game_day {
        return Err(InsuranceError::ClaimNotExpired);
    }
    let (released_reservation_krw, contract_aggregates) =
        finalize_claim_contracts(input.contracts, false)?;
    Ok(InsuranceClaimExpiryPlan {
        claim_id: input.claim_id,
        status: InsuranceClaimStatus::Expired,
        expired_game_day: input.current_game_day,
        released_reservation_krw,
        contract_aggregates,
    })
}

fn finalize_claim_contracts(
    contracts: &[InsuranceClaimFinalizationContractInput],
    paid: bool,
) -> Result<(i64, Vec<InsuranceClaimContractAggregatePlan>), InsuranceError> {
    if contracts.is_empty() || contracts.len() > INSURANCE_MAX_CLAIM_CONTRACTS {
        return Err(InsuranceError::InvalidClaimAmount);
    }
    let mut contracts = contracts.to_vec();
    contracts.sort_by_key(|contract| contract.contract_id);
    let mut ids = BTreeSet::new();
    let mut total = 0_i128;
    let mut aggregates = Vec::with_capacity(contracts.len());
    for contract in contracts {
        if !ids.insert(contract.contract_id) {
            return Err(InsuranceError::DuplicateContract);
        }
        if contract.allocation_krw <= 0
            || contract.paid_krw < 0
            || contract.reserved_krw < contract.allocation_krw
        {
            return Err(InsuranceError::InvalidTermUsage);
        }
        total = total
            .checked_add(i128::from(contract.allocation_krw))
            .ok_or(InsuranceError::ArithmeticOverflow)?;
        let reserved_after_krw = contract
            .reserved_krw
            .checked_sub(contract.allocation_krw)
            .ok_or(InsuranceError::ArithmeticOverflow)?;
        let paid_after_krw = if paid {
            contract
                .paid_krw
                .checked_add(contract.allocation_krw)
                .ok_or(InsuranceError::ArithmeticOverflow)?
        } else {
            contract.paid_krw
        };
        aggregates.push(InsuranceClaimContractAggregatePlan {
            contract_id: contract.contract_id,
            paid_before_krw: contract.paid_krw,
            paid_after_krw,
            reserved_before_krw: contract.reserved_krw,
            reserved_after_krw,
        });
    }
    let total = i64::try_from(total).map_err(|_| InsuranceError::ArithmeticOverflow)?;
    if !(1..=INSURANCE_MAX_MONEY_KRW).contains(&total) {
        return Err(InsuranceError::InvalidClaimAmount);
    }
    Ok((total, aggregates))
}

fn plan_premium_ledger(
    input: InsurancePremiumLedgerInput,
) -> Result<InsuranceLedgerPlan, InsuranceError> {
    if input.wallet_cash_krw < 0 || !(1..=INSURANCE_MAX_MONEY_KRW).contains(&input.premium_krw) {
        return Err(InsuranceError::InvalidPremiumCharge);
    }
    if input.wallet_cash_krw < input.premium_krw {
        return Err(InsuranceError::InsufficientWalletCash);
    }
    let wallet_delta_krw = input
        .premium_krw
        .checked_neg()
        .ok_or(InsuranceError::ArithmeticOverflow)?;
    let wallet_cash_after_krw = input
        .wallet_cash_krw
        .checked_add(wallet_delta_krw)
        .ok_or(InsuranceError::ArithmeticOverflow)?;
    balanced_ledger_plan(
        input.wallet_cash_krw,
        wallet_cash_after_krw,
        wallet_delta_krw,
        vec![
            InsuranceLedgerPosting {
                account_code: InsuranceLedgerAccountCode::InsurancePremiumExpense,
                amount_krw: input.premium_krw,
            },
            InsuranceLedgerPosting {
                account_code: InsuranceLedgerAccountCode::Wallet,
                amount_krw: wallet_delta_krw,
            },
        ],
    )
}

fn plan_claim_ledger(
    input: InsuranceClaimLedgerInput,
) -> Result<InsuranceLedgerPlan, InsuranceError> {
    if input.wallet_cash_krw < 0 || !(1..=INSURANCE_MAX_MONEY_KRW).contains(&input.payout_krw) {
        return Err(InsuranceError::InvalidClaimAmount);
    }
    let wallet_cash_after_krw = input
        .wallet_cash_krw
        .checked_add(input.payout_krw)
        .ok_or(InsuranceError::ArithmeticOverflow)?;
    let recovery_krw = input
        .payout_krw
        .checked_neg()
        .ok_or(InsuranceError::ArithmeticOverflow)?;
    balanced_ledger_plan(
        input.wallet_cash_krw,
        wallet_cash_after_krw,
        input.payout_krw,
        vec![
            InsuranceLedgerPosting {
                account_code: InsuranceLedgerAccountCode::Wallet,
                amount_krw: input.payout_krw,
            },
            InsuranceLedgerPosting {
                account_code: InsuranceLedgerAccountCode::InsuranceClaimRecovery,
                amount_krw: recovery_krw,
            },
        ],
    )
}

fn balanced_ledger_plan(
    wallet_cash_before_krw: i64,
    wallet_cash_after_krw: i64,
    wallet_delta_krw: i64,
    postings: Vec<InsuranceLedgerPosting>,
) -> Result<InsuranceLedgerPlan, InsuranceError> {
    let balance = postings.iter().try_fold(0_i128, |total, posting| {
        total
            .checked_add(i128::from(posting.amount_krw))
            .ok_or(InsuranceError::ArithmeticOverflow)
    })?;
    if balance != 0 {
        return Err(InsuranceError::UnbalancedLedgerPlan);
    }
    Ok(InsuranceLedgerPlan {
        wallet_cash_before_krw,
        wallet_cash_after_krw,
        wallet_delta_krw,
        postings,
    })
}

fn find_product(
    catalog: &InsuranceCatalog,
    product_version_id: ResourceId,
) -> Result<&InsuranceProductDefinition, InsuranceError> {
    catalog
        .products
        .iter()
        .find(|product| product.product_version_id == product_version_id)
        .ok_or(InsuranceError::ProductNotFound)
}

fn push_bytes(output: &mut Vec<u8>, value: &[u8]) -> Result<(), InsuranceError> {
    let length = u32::try_from(value.len()).map_err(|_| InsuranceError::ArithmeticOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(value);
    Ok(())
}

fn hex_digest(digest: [u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn given_rules() -> Arc<dyn InsuranceRules> {
        create_insurance_rules()
    }

    fn given_catalog() -> InsuranceCatalog {
        create_fictional_family_care_insurance_catalog()
    }

    fn given_known_facts() -> Vec<LifeEventFactEvidence> {
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

    fn given_pin(contract_id: u64, paid_krw: i64, reserved_krw: i64) -> InsuranceClaimContractPin {
        InsuranceClaimContractPin {
            contract_id: ResourceId::from_u64(contract_id),
            product_version_id: ResourceId::from_u64(100),
            coverage_version_id: ResourceId::from_u64(101),
            coverage_start_game_day: 0,
            waiting_ends_game_day: 7,
            coverage_end_exclusive: 360,
            waiting_passed: true,
            deductible_krw: 20_000,
            occurrence_limit_krw: 100_000,
            term_limit_krw: 200_000,
            paid_krw,
            reserved_krw,
        }
    }

    mod context_보험_자격을_판정하는_경우 {
        use super::*;

        #[test]
        fn given_모든_fact가known_when_판정하면_then_eligible이다() {
            let rules = given_rules();
            let catalog = given_catalog();
            let facts = given_known_facts();

            let evaluation = rules
                .evaluate_eligibility(InsuranceEligibilityInput {
                    catalog: &catalog,
                    product_version_id: ResourceId::from_u64(100),
                    evaluation_game_day: 0,
                    facts: &facts,
                })
                .expect("보험 자격을 판정해야 한다");

            assert_eq!(evaluation.status, InsuranceEligibilityStatus::Eligible);
            assert_eq!(evaluation.fact_fingerprint.len(), 64);
        }

        #[test]
        fn given_필수fact가unknown_when_판정하면_then_indeterminate를_보존한다() {
            let rules = given_rules();
            let catalog = given_catalog();
            let mut facts = given_known_facts();
            facts[2].value =
                LifeEventEvidenceValue::Unknown(LifeEventUnknownReason::AuthorityMissing);

            let evaluation = rules
                .evaluate_eligibility(InsuranceEligibilityInput {
                    catalog: &catalog,
                    product_version_id: ResourceId::from_u64(100),
                    evaluation_game_day: 0,
                    facts: &facts,
                })
                .expect("unknown 자격을 보존해야 한다");

            assert_eq!(evaluation.status, InsuranceEligibilityStatus::Indeterminate);
            assert_eq!(evaluation.reasons.len(), 1);
        }

        #[test]
        fn given_fact타입과_ast단위가다를때_when_catalog를검증하면_then_거절한다() {
            let rules = given_rules();
            let mut catalog = given_catalog();
            let LifeEventExpression::All { children } =
                &mut catalog.products[0].eligibility_ast.root
            else {
                panic!("fixture root는 all이어야 한다");
            };
            let LifeEventExpression::Between { value, .. } = &mut children[0] else {
                panic!("첫 조건은 between이어야 한다");
            };
            let LifeEventOperand::Fact { reference } = value.as_mut() else {
                panic!("between 값은 fact여야 한다");
            };
            reference.unit = LifeEventUnit::Count;

            let result = rules.validate_catalog(&catalog);

            assert_eq!(result, Err(InsuranceError::UnitMismatch));
        }
    }

    mod context_보험료_schedule을_만드는_경우 {
        use super::*;

        #[test]
        fn given_d0가입_when_계약을계획하면_then_d0부터d330까지12회이고_d360은exclusive다() {
            let rules = given_rules();
            let catalog = given_catalog();

            let plan = rules
                .plan_contract(InsuranceContractPlanInput {
                    contract_id: ResourceId::from_u64(1),
                    product: &catalog.products[0],
                    start_game_day: 0,
                })
                .expect("보험 계약을 계획해야 한다");

            let due_days = plan
                .premium_charges
                .iter()
                .map(|charge| charge.due_game_day)
                .collect::<Vec<_>>();
            assert_eq!(
                due_days,
                (0..12).map(|index| index * 30).collect::<Vec<_>>()
            );
            assert_eq!(plan.coverage_end_exclusive, 360);
            assert!(
                due_days
                    .iter()
                    .all(|day| *day < plan.coverage_end_exclusive)
            );
        }

        #[test]
        fn given_game_day상한근처_when_계약을계획하면_then_checked_overflow다() {
            let rules = given_rules();
            let catalog = given_catalog();

            let result = rules.plan_contract(InsuranceContractPlanInput {
                contract_id: ResourceId::from_u64(1),
                product: &catalog.products[0],
                start_game_day: u32::MAX - 6,
            });

            assert_eq!(result, Err(InsuranceError::ArithmeticOverflow));
        }
    }

    mod context_waiting과_보장경계를_판정하는_경우 {
        use super::*;

        fn when_covered(rules: &dyn InsuranceRules, day: u32) -> bool {
            rules
                .is_event_covered(InsuranceCoverageInput {
                    coverage_start_game_day: 0,
                    waiting_ends_game_day: 7,
                    coverage_end_exclusive: 360,
                    event_offered_game_day: day,
                })
                .expect("보장 경계를 판정해야 한다")
        }

        #[test]
        fn given_waiting직전과당일_when_판정하면_then_당일부터보장한다() {
            let rules = given_rules();

            let before = when_covered(rules.as_ref(), 6);
            let boundary = when_covered(rules.as_ref(), 7);

            assert!(!before);
            assert!(boundary);
        }

        #[test]
        fn given_term마지막날과exclusive일_when_판정하면_then_exclusive부터보장하지않는다() {
            let rules = given_rules();

            let last = when_covered(rules.as_ref(), 359);
            let exclusive = when_covered(rules.as_ref(), 360);

            assert!(last);
            assert!(!exclusive);
        }
    }

    mod context_phase250_보험료를_처리하는_경우 {
        use super::*;

        #[test]
        fn given_보험료이상wallet_when_처리하면_then_전액paid다() {
            let rules = given_rules();

            let resolution = rules
                .resolve_premium(InsurancePremiumResolutionInput {
                    contract_id: ResourceId::from_u64(1),
                    charge_no: 2,
                    due_game_day: 30,
                    premium_krw: 10_000,
                    wallet_cash_krw: 10_000,
                })
                .expect("보험료를 처리해야 한다");

            assert_eq!(resolution.charge_status, InsurancePremiumChargeStatus::Paid);
            assert_eq!(resolution.wallet_cash_after_krw, 0);
        }

        #[test]
        fn given_보험료미만wallet_when_처리하면_then_부분납부없이_d다음날부터lapse다() {
            let rules = given_rules();

            let resolution = rules
                .resolve_premium(InsurancePremiumResolutionInput {
                    contract_id: ResourceId::from_u64(1),
                    charge_no: 2,
                    due_game_day: 30,
                    premium_krw: 10_000,
                    wallet_cash_krw: 9_999,
                })
                .expect("보험료 미납을 lapse로 계획해야 한다");

            assert_eq!(
                resolution.charge_status,
                InsurancePremiumChargeStatus::Missed
            );
            assert_eq!(resolution.paid_krw, 0);
            assert_eq!(resolution.wallet_cash_after_krw, 9_999);
            assert_eq!(resolution.coverage_end_exclusive, Some(31));
        }

        #[test]
        fn given_d10활성계약_when_당일취소하면_then_d11이coverage_exclusive다() {
            let rules = given_rules();

            let plan = rules
                .terminate_contract(InsuranceTerminationInput {
                    contract_id: ResourceId::from_u64(1),
                    coverage_start_game_day: 0,
                    current_coverage_end_exclusive: 360,
                    effective_game_day: 10,
                    kind: InsuranceTerminationKind::Cancellation,
                })
                .expect("중도 취소 경계를 계획해야 한다");

            assert_eq!(plan.status, InsuranceContractStatus::Cancelled);
            assert_eq!(plan.coverage_end_exclusive, 11);
        }

        #[test]
        fn given_d360_exclusive계약_when_d359와d360에서만료를판정하면_then_d360에expired다() {
            let rules = given_rules();

            let before = rules.expire_contract(InsuranceContractExpiryInput {
                contract_id: ResourceId::from_u64(1),
                current_status: InsuranceContractStatus::Active,
                coverage_end_exclusive: 360,
                target_game_day: 359,
            });
            let at_boundary = rules
                .expire_contract(InsuranceContractExpiryInput {
                    contract_id: ResourceId::from_u64(1),
                    current_status: InsuranceContractStatus::Active,
                    coverage_end_exclusive: 360,
                    target_game_day: 360,
                })
                .expect("exclusive 경계에서 계약을 만료해야 한다");

            assert_eq!(before, Err(InsuranceError::ContractNotExpired));
            assert_eq!(at_boundary.status, InsuranceContractStatus::Expired);
            assert_eq!(at_boundary.expired_game_day, 360);
        }
    }

    mod context_claim후보를_고정하는_경우 {
        use super::*;

        #[test]
        fn given_matching계약이없을때_when_candidate를계획하면_then_empty_digest를고정한다() {
            let rules = given_rules();

            let first = rules
                .plan_claim_candidate(InsuranceClaimCandidateInput {
                    claim_id: ResourceId::from_u64(1),
                    event_instance_id: ResourceId::from_u64(2),
                    offered_game_day: 0,
                    matching_contracts: &[],
                })
                .expect("빈 계약 집합도 pin해야 한다");
            let second = rules
                .plan_claim_candidate(InsuranceClaimCandidateInput {
                    claim_id: ResourceId::from_u64(9),
                    event_instance_id: ResourceId::from_u64(8),
                    offered_game_day: 99,
                    matching_contracts: &[],
                })
                .expect("빈 계약 집합 digest는 canonical해야 한다");

            assert_eq!(first.contract_set_digest, second.contract_set_digest);
            assert_eq!(first.contract_set_digest.len(), 64);
        }

        #[test]
        fn given_contract_id역순_when_candidate를계획하면_then_id오름차순으로고정한다() {
            let rules = given_rules();
            let pins = [given_pin(2, 0, 0), given_pin(1, 0, 0)];

            let plan = rules
                .plan_claim_candidate(InsuranceClaimCandidateInput {
                    claim_id: ResourceId::from_u64(1),
                    event_instance_id: ResourceId::from_u64(2),
                    offered_game_day: 31,
                    matching_contracts: &pins,
                })
                .expect("계약 pin을 정렬해야 한다");

            assert_eq!(
                plan.contract_pins
                    .iter()
                    .map(|pin| pin.contract_id.get())
                    .collect::<Vec<_>>(),
                vec![1, 2]
            );
        }
    }

    mod context_fixed_indemnity를_배분하는_경우 {
        use super::*;

        #[test]
        fn given_120000손해와두계약_when_배분하면_then_contract_id순으로총손해를넘지않는다() {
            let rules = given_rules();
            let pins = [given_pin(2, 0, 0), given_pin(1, 0, 0)];

            let plan = rules
                .resolve_claim(InsuranceClaimResolutionInput {
                    claim_id: ResourceId::from_u64(1),
                    current_status: InsuranceClaimStatus::Candidate,
                    resolved_game_day: 31,
                    resolution_kind: InsuranceClaimResolutionKind::FixedWalletExpense,
                    gross_cost_krw: Some(120_000),
                    claim_window_game_days: 7,
                    contract_pins: &pins,
                })
                .expect("손해액을 계약 ID 순서로 배분해야 한다");

            assert_eq!(plan.status, InsuranceClaimStatus::Ready);
            assert_eq!(plan.payout_krw, 120_000);
            assert_eq!(plan.allocations[0].contract_id.get(), 1);
            assert_eq!(plan.allocations[0].allocation_krw, 100_000);
            assert_eq!(plan.allocations[1].contract_id.get(), 2);
            assert_eq!(plan.allocations[1].allocation_krw, 20_000);
        }

        #[test]
        fn given_paid와reserved가term한도를채울때_when_배분하면_then_not_covered다() {
            let rules = given_rules();
            let pins = [given_pin(1, 150_000, 50_000)];

            let plan = rules
                .resolve_claim(InsuranceClaimResolutionInput {
                    claim_id: ResourceId::from_u64(1),
                    current_status: InsuranceClaimStatus::Candidate,
                    resolved_game_day: 31,
                    resolution_kind: InsuranceClaimResolutionKind::FixedWalletExpense,
                    gross_cost_krw: Some(120_000),
                    claim_window_game_days: 7,
                    contract_pins: &pins,
                })
                .expect("paid와 reserved를 모두 총 한도에서 차감해야 한다");

            assert_eq!(plan.status, InsuranceClaimStatus::NotCovered);
            assert_eq!(plan.payout_krw, 0);
        }

        #[test]
        fn given_term잔여30000원_when_배분하면_then_occurrence보다term잔여가우선한다() {
            let rules = given_rules();
            let pins = [given_pin(1, 150_000, 20_000)];

            let plan = rules
                .resolve_claim(InsuranceClaimResolutionInput {
                    claim_id: ResourceId::from_u64(1),
                    current_status: InsuranceClaimStatus::Candidate,
                    resolved_game_day: 31,
                    resolution_kind: InsuranceClaimResolutionKind::FixedWalletExpense,
                    gross_cost_krw: Some(120_000),
                    claim_window_game_days: 7,
                    contract_pins: &pins,
                })
                .expect("term 잔여 한도로 배분을 제한해야 한다");

            assert_eq!(plan.allocations[0].raw_krw, 30_000);
            assert_eq!(plan.allocations[0].allocation_krw, 30_000);
            assert_eq!(plan.contract_aggregates[0].reserved_after_krw, 50_000);
        }

        #[test]
        fn given_gross가deductible과같을때_when_배분하면_then_not_covered다() {
            let rules = given_rules();
            let pins = [given_pin(1, 0, 0)];

            let plan = rules
                .resolve_claim(InsuranceClaimResolutionInput {
                    claim_id: ResourceId::from_u64(1),
                    current_status: InsuranceClaimStatus::Candidate,
                    resolved_game_day: 31,
                    resolution_kind: InsuranceClaimResolutionKind::FixedWalletExpense,
                    gross_cost_krw: Some(20_000),
                    claim_window_game_days: 7,
                    contract_pins: &pins,
                })
                .expect("deductible 경계를 원 단위로 계산해야 한다");

            assert_eq!(plan.status, InsuranceClaimStatus::NotCovered);
        }

        #[test]
        fn given_no_effect_resolution_when_전이하면_then_not_applicable이고예약이없다() {
            let rules = given_rules();

            let plan = rules
                .resolve_claim(InsuranceClaimResolutionInput {
                    claim_id: ResourceId::from_u64(1),
                    current_status: InsuranceClaimStatus::Candidate,
                    resolved_game_day: 31,
                    resolution_kind: InsuranceClaimResolutionKind::NoEffect,
                    gross_cost_krw: None,
                    claim_window_game_days: 7,
                    contract_pins: &[],
                })
                .expect("no effect claim을 종료해야 한다");

            assert_eq!(plan.status, InsuranceClaimStatus::NotApplicable);
            assert!(plan.contract_aggregates.is_empty());
        }

        #[test]
        fn given_term사용량이limit을넘을때_when_배분하면_then_거절한다() {
            let rules = given_rules();
            let pins = [given_pin(1, 190_000, 20_000)];

            let result = rules.resolve_claim(InsuranceClaimResolutionInput {
                claim_id: ResourceId::from_u64(1),
                current_status: InsuranceClaimStatus::Candidate,
                resolved_game_day: 31,
                resolution_kind: InsuranceClaimResolutionKind::FixedWalletExpense,
                gross_cost_krw: Some(120_000),
                claim_window_game_days: 7,
                contract_pins: &pins,
            });

            assert_eq!(result, Err(InsuranceError::InvalidTermUsage));
        }

        #[test]
        fn given_resolved_day상한_when_ready를계획하면_then_deadline_checked_overflow다() {
            let rules = given_rules();
            let pins = [given_pin(1, 0, 0)];

            let result = rules.resolve_claim(InsuranceClaimResolutionInput {
                claim_id: ResourceId::from_u64(1),
                current_status: InsuranceClaimStatus::Candidate,
                resolved_game_day: u32::MAX - 6,
                resolution_kind: InsuranceClaimResolutionKind::FixedWalletExpense,
                gross_cost_krw: Some(120_000),
                claim_window_game_days: 7,
                contract_pins: &pins,
            });

            assert_eq!(result, Err(InsuranceError::ArithmeticOverflow));
        }
    }

    mod context_ready_claim을_종결하는_경우 {
        use super::*;

        fn given_finalization() -> [InsuranceClaimFinalizationContractInput; 1] {
            [InsuranceClaimFinalizationContractInput {
                contract_id: ResourceId::from_u64(1),
                allocation_krw: 100_000,
                paid_krw: 0,
                reserved_krw: 100_000,
            }]
        }

        #[test]
        fn given_deadline직전_when_지급하면_then_reserved를paid로옮긴다() {
            let rules = given_rules();
            let contracts = given_finalization();

            let plan = rules
                .pay_claim(InsuranceClaimPaymentInput {
                    claim_id: ResourceId::from_u64(1),
                    current_status: InsuranceClaimStatus::Ready,
                    current_game_day: 37,
                    filing_deadline_game_day: 38,
                    contracts: &contracts,
                })
                .expect("deadline 전 claim을 지급해야 한다");

            assert_eq!(plan.status, InsuranceClaimStatus::Paid);
            assert_eq!(plan.contract_aggregates[0].paid_after_krw, 100_000);
            assert_eq!(plan.contract_aggregates[0].reserved_after_krw, 0);
        }

        #[test]
        fn given_deadline당일_when_지급하면_then_exclusive경계로거절한다() {
            let rules = given_rules();
            let contracts = given_finalization();

            let result = rules.pay_claim(InsuranceClaimPaymentInput {
                claim_id: ResourceId::from_u64(1),
                current_status: InsuranceClaimStatus::Ready,
                current_game_day: 38,
                filing_deadline_game_day: 38,
                contracts: &contracts,
            });

            assert_eq!(result, Err(InsuranceError::ClaimExpired));
        }

        #[test]
        fn given_deadline당일_when_만료하면_then_reservation을해제한다() {
            let rules = given_rules();
            let contracts = given_finalization();

            let plan = rules
                .expire_claim(InsuranceClaimExpiryInput {
                    claim_id: ResourceId::from_u64(1),
                    current_status: InsuranceClaimStatus::Ready,
                    current_game_day: 38,
                    filing_deadline_game_day: 38,
                    contracts: &contracts,
                })
                .expect("deadline 당일 claim을 만료해야 한다");

            assert_eq!(plan.status, InsuranceClaimStatus::Expired);
            assert_eq!(plan.released_reservation_krw, 100_000);
            assert_eq!(plan.contract_aggregates[0].reserved_after_krw, 0);
        }
    }

    mod context_보험원장을_계획하는_경우 {
        use super::*;

        #[test]
        fn given_보험료_when_원장을계획하면_then_expense와wallet이balanced다() {
            let rules = given_rules();

            let plan = rules
                .plan_premium_ledger(InsurancePremiumLedgerInput {
                    wallet_cash_krw: 20_000,
                    premium_krw: 10_000,
                })
                .expect("보험료 원장을 계획해야 한다");

            assert_eq!(plan.wallet_cash_after_krw, 10_000);
            assert_eq!(
                plan.postings
                    .iter()
                    .map(|posting| i128::from(posting.amount_krw))
                    .sum::<i128>(),
                0
            );
        }

        #[test]
        fn given_claim지급_when_원장을계획하면_then_wallet과recovery가balanced다() {
            let rules = given_rules();

            let plan = rules
                .plan_claim_ledger(InsuranceClaimLedgerInput {
                    wallet_cash_krw: 20_000,
                    payout_krw: 100_000,
                })
                .expect("claim 지급 원장을 계획해야 한다");

            assert_eq!(plan.wallet_cash_after_krw, 120_000);
            assert_eq!(
                plan.postings
                    .iter()
                    .map(|posting| i128::from(posting.amount_krw))
                    .sum::<i128>(),
                0
            );
        }

        #[test]
        fn given_wallet상한_when_claim원장을계획하면_then_checked_overflow다() {
            let rules = given_rules();

            let result = rules.plan_claim_ledger(InsuranceClaimLedgerInput {
                wallet_cash_krw: i64::MAX,
                payout_krw: 1,
            });

            assert_eq!(result, Err(InsuranceError::ArithmeticOverflow));
        }
    }
}
