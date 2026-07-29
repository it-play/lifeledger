mod content_bundle;
mod liquidation;
mod point_budget;
mod ranking;
mod types;

use std::sync::Arc;

pub use ranking::{encode_ranking_cursor, parse_ranking_cursor};
pub use types::{
    CharacterPresetVersion, ContentAuthorityKind, ContentBundleDraft, ContentBundleFailure,
    ContentBundleFailureCode, ContentBundleMember, ContentBundlePublication, ContentBundleRules,
    LeagueDefinition, LeagueRankingItem, LeagueRankingPage, LiquidationComponentInput,
    LiquidationLine, LiquidationPlan, LiquidationPlanner, PointBudgetCatalog,
    PointBudgetEvaluation, PointBudgetFailure, PointBudgetFailureCode, PointBudgetOption,
    PointBudgetPreparation, PointBudgetRules, PointCondition, PointCostKind, PointEffect,
    PointExclusiveGroup, PointFactComparison, PointFactValue, PointLedgerLine, PointSelection,
    PointTier, RankedRunContext, RankedRunPreparation, RankingPageCursor, RunFinalization,
    RunFinalizationLine, RunFinalizationStatus, RunManifestSummary, RunMode, RunOptions,
    SeasonLeagues, SeasonStatus, SeasonSummary,
};

pub fn create_liquidation_planner() -> Arc<dyn LiquidationPlanner> {
    Arc::new(liquidation::DefaultLiquidationPlanner)
}

pub fn create_content_bundle_rules() -> Arc<dyn ContentBundleRules> {
    Arc::new(content_bundle::DefaultContentBundleRules)
}

pub fn create_point_budget_rules() -> Arc<dyn PointBudgetRules> {
    Arc::new(point_budget::DefaultPointBudgetRules)
}
