use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::character::CharacterDraft;
use crate::finance::ResourceId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum RunMode {
    RankedPreset,
    RankedCustom,
    Sandbox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum PointCostKind {
    Fixed,
    PerUnit,
    Tiered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum PointFactComparison {
    Equal,
    GreaterOrEqual,
    LessOrEqual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum PointFactValue {
    Integer(i64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PointEffect {
    SetInteger {
        fact_path: String,
        value: i64,
    },
    IncrementInteger {
        fact_path: String,
        value_per_unit: i64,
    },
    SetText {
        fact_path: String,
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PointCondition {
    RequiresOption {
        option_id: ResourceId,
    },
    ForbidsOption {
        option_id: ResourceId,
    },
    RequiresFact {
        fact_path: String,
        comparison: PointFactComparison,
        expected: PointFactValue,
    },
    ForbidsFact {
        fact_path: String,
        comparison: PointFactComparison,
        expected: PointFactValue,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PointTier {
    pub minimum_quantity: u32,
    pub maximum_quantity: u32,
    pub point_delta_per_unit: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PointBudgetOption {
    pub id: ResourceId,
    pub option_key: String,
    pub display_name: String,
    pub description: String,
    pub cost_kind: PointCostKind,
    #[schema(required = true, nullable)]
    pub point_delta_per_unit: Option<i64>,
    pub minimum_quantity: u32,
    pub maximum_quantity: u32,
    #[schema(required = true, nullable)]
    pub exclusive_group_key: Option<String>,
    pub effect: PointEffect,
    pub tiers: Vec<PointTier>,
    pub conditions: Vec<PointCondition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PointExclusiveGroup {
    pub group_key: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PointBudgetCatalog {
    pub id: ResourceId,
    pub budget_key: String,
    pub version: u32,
    pub display_name: String,
    pub description: String,
    pub total_points: i64,
    pub ranked_eligible: bool,
    pub canonical_sha256: String,
    pub groups: Vec<PointExclusiveGroup>,
    pub options: Vec<PointBudgetOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PointSelection {
    pub option_id: ResourceId,
    pub quantity: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum PointBudgetFailureCode {
    UnknownOption,
    DuplicateOption,
    InvalidQuantity,
    MissingExclusiveGroup,
    MultipleExclusiveGroup,
    RequiredOptionMissing,
    ForbiddenOptionSelected,
    RequiredFactMissing,
    ForbiddenFactMatched,
    ConflictingFact,
    PointOverflow,
    BudgetExceeded,
    InvalidCatalog,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PointBudgetFailure {
    pub code: PointBudgetFailureCode,
    #[schema(required = true, nullable)]
    pub option_id: Option<ResourceId>,
    #[schema(required = true, nullable)]
    pub related_option_id: Option<ResourceId>,
    #[schema(required = true, nullable)]
    pub group_key: Option<String>,
    #[schema(required = true, nullable)]
    pub fact_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PointLedgerLine {
    pub option_id: ResourceId,
    pub option_key: String,
    pub quantity: u32,
    pub point_delta: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PointBudgetEvaluation {
    pub point_budget_version_id: ResourceId,
    pub valid: bool,
    pub total_points: i64,
    #[schema(required = true, nullable)]
    pub spent_points: Option<i64>,
    #[schema(required = true, nullable)]
    pub remaining_points: Option<i64>,
    pub lines: Vec<PointLedgerLine>,
    pub failures: Vec<PointBudgetFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CharacterPresetVersion {
    pub id: ResourceId,
    pub preset_key: String,
    pub version: u32,
    pub display_name: String,
    pub summary: String,
    pub ranked_eligible: bool,
    pub canonical_sha256: String,
    pub draft: CharacterDraft,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunOptions {
    pub modes: Vec<RunMode>,
    #[schema(required = true, nullable)]
    pub active_season_id: Option<ResourceId>,
    pub presets: Vec<CharacterPresetVersion>,
    pub point_budgets: Vec<PointBudgetCatalog>,
    pub sandbox_available: bool,
}

pub trait PointBudgetRules: Send + Sync + 'static {
    fn evaluate(
        &self,
        catalog: &PointBudgetCatalog,
        selections: &[PointSelection],
    ) -> PointBudgetEvaluation;
}

#[cfg(test)]
mod tests {
    mod point_effect_contract {
        use super::super::PointEffect;

        mod context_camel_case_json {
            use super::*;

            #[test]
            fn given_camel_case_effect_when_parsed_then_fields_are_accepted() {
                let given_json = r#"{
                    "kind":"incrementInteger",
                    "factPath":"startingCashKrw",
                    "valuePerUnit":1000000
                }"#;

                let when_parsed = serde_json::from_str::<PointEffect>(given_json)
                    .expect("camelCase point effect should parse");

                assert_eq!(
                    when_parsed,
                    PointEffect::IncrementInteger {
                        fact_path: "startingCashKrw".to_owned(),
                        value_per_unit: 1_000_000,
                    }
                );
            }
        }
    }

    mod point_condition_contract {
        use super::super::{PointCondition, PointFactComparison, PointFactValue};

        mod context_camel_case_json {
            use super::*;

            #[test]
            fn given_camel_case_condition_when_parsed_then_fields_are_accepted() {
                let given_json = r#"{
                    "kind":"requiresFact",
                    "factPath":"certifications",
                    "comparison":"greaterOrEqual",
                    "expected":{"type":"integer","value":1}
                }"#;

                let when_parsed = serde_json::from_str::<PointCondition>(given_json)
                    .expect("camelCase point condition should parse");

                assert_eq!(
                    when_parsed,
                    PointCondition::RequiresFact {
                        fact_path: "certifications".to_owned(),
                        comparison: PointFactComparison::GreaterOrEqual,
                        expected: PointFactValue::Integer(1),
                    }
                );
            }
        }
    }
}
