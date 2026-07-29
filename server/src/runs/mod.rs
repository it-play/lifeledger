mod content_bundle;
mod point_budget;
mod types;

use std::sync::Arc;

pub use types::{
    CharacterPresetVersion, ContentAuthorityKind, ContentBundleDraft, ContentBundleFailure,
    ContentBundleFailureCode, ContentBundleMember, ContentBundlePublication, ContentBundleRules,
    LeagueDefinition, PointBudgetCatalog, PointBudgetEvaluation, PointBudgetFailure,
    PointBudgetFailureCode, PointBudgetOption, PointBudgetPreparation, PointBudgetRules,
    PointCondition, PointCostKind, PointEffect, PointExclusiveGroup, PointFactComparison,
    PointFactValue, PointLedgerLine, PointSelection, PointTier, RankedRunContext,
    RankedRunPreparation, RunManifestSummary, RunMode, RunOptions, SeasonLeagues, SeasonStatus,
    SeasonSummary,
};

pub fn create_content_bundle_rules() -> Arc<dyn ContentBundleRules> {
    Arc::new(content_bundle::DefaultContentBundleRules)
}

pub fn create_point_budget_rules() -> Arc<dyn PointBudgetRules> {
    Arc::new(point_budget::DefaultPointBudgetRules)
}
