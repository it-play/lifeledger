mod point_budget;
mod types;

use std::sync::Arc;

pub use types::{
    CharacterPresetVersion, PointBudgetCatalog, PointBudgetEvaluation, PointBudgetFailure,
    PointBudgetFailureCode, PointBudgetOption, PointBudgetPreparation, PointBudgetRules,
    PointCondition, PointCostKind, PointEffect, PointExclusiveGroup, PointFactComparison,
    PointFactValue, PointLedgerLine, PointSelection, PointTier, RunManifestSummary, RunMode,
    RunOptions,
};

pub fn create_point_budget_rules() -> Arc<dyn PointBudgetRules> {
    Arc::new(point_budget::DefaultPointBudgetRules)
}
