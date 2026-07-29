use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use sqlx::MySqlPool;

use super::types::RunStore;
use crate::character::CharacterDraft;
use crate::finance::ResourceId;
use crate::runs::{
    CharacterPresetVersion, PointBudgetCatalog, PointBudgetEvaluation, PointBudgetOption,
    PointBudgetRules, PointCondition, PointCostKind, PointEffect, PointExclusiveGroup,
    PointFactComparison, PointFactValue, PointSelection, PointTier, RunManifestSummary, RunMode,
    RunOptions, create_point_budget_rules,
};

#[derive(Clone)]
pub struct MySqlRunStore {
    pool: MySqlPool,
    rules: Arc<dyn PointBudgetRules>,
}

pub fn create_mysql_run_store(pool: MySqlPool) -> MySqlRunStore {
    MySqlRunStore {
        pool,
        rules: create_point_budget_rules(),
    }
}

#[derive(Debug, sqlx::FromRow)]
struct PresetRow {
    id: u64,
    preset_key: String,
    version_no: u32,
    display_name: String,
    summary: String,
    ranked_eligible: bool,
    canonical_draft_json: String,
    canonical_sha256: String,
}

#[derive(Debug, sqlx::FromRow)]
struct BudgetRow {
    id: u64,
    budget_key: String,
    version_no: u32,
    display_name: String,
    description: String,
    total_points: i64,
    ranked_eligible: bool,
    canonical_sha256: String,
}

#[derive(Debug, sqlx::FromRow)]
struct GroupRow {
    group_key: String,
    display_name: String,
}

#[derive(Debug, sqlx::FromRow)]
struct OptionRow {
    id: u64,
    option_key: String,
    display_name: String,
    description: String,
    cost_kind: String,
    point_delta_per_unit: Option<i64>,
    minimum_quantity: u32,
    maximum_quantity: u32,
    exclusive_group_key: Option<String>,
    effect_json: String,
}

#[derive(Debug, sqlx::FromRow)]
struct TierRow {
    point_budget_option_id: u64,
    minimum_quantity: u32,
    maximum_quantity: u32,
    point_delta_per_unit: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct ConditionRow {
    point_budget_option_id: u64,
    condition_kind: String,
    related_option_id: Option<u64>,
    fact_path: Option<String>,
    comparison_kind: Option<String>,
    fact_value_kind: Option<String>,
    fact_integer_value: Option<i64>,
    fact_text_value: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct ManifestRow {
    run_revision: u32,
    mode: String,
    manifest_sha256: String,
}

#[async_trait]
impl RunStore for MySqlRunStore {
    async fn run_options(&self) -> Result<RunOptions> {
        let presets = sqlx::query_as::<_, PresetRow>(
            "SELECT id, preset_key, version_no, display_name, summary, ranked_eligible,
                    canonical_draft_json, canonical_sha256
             FROM character_preset_version
             WHERE status = 'sealed'
             ORDER BY preset_key, version_no, id",
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(to_preset)
        .collect::<Result<Vec<_>>>()?;

        let budgets = sqlx::query_as::<_, BudgetRow>(
            "SELECT version_row.id, version_row.budget_key, version_row.version_no,
                    version_row.display_name, version_row.description,
                    version_row.total_points, version_row.ranked_eligible,
                    version_row.canonical_sha256
             FROM point_budget_assignment AS assignment
             INNER JOIN point_budget_version AS version_row
                ON version_row.id = assignment.point_budget_version_id
             WHERE assignment.assignment_key = 'newRun'
               AND version_row.status = 'sealed'
             ORDER BY version_row.budget_key, version_row.version_no, version_row.id",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut point_budgets = Vec::with_capacity(budgets.len());
        for budget in budgets {
            point_budgets.push(read_budget_children(&self.pool, budget).await?);
        }

        Ok(RunOptions {
            modes: vec![
                RunMode::RankedPreset,
                RunMode::RankedCustom,
                RunMode::Sandbox,
            ],
            active_season_id: None,
            presets,
            point_budgets,
            sandbox_available: true,
        })
    }

    async fn preview_point_budget(
        &self,
        version_id: ResourceId,
        selections: &[PointSelection],
    ) -> Result<Option<PointBudgetEvaluation>> {
        let budget = sqlx::query_as::<_, BudgetRow>(
            "SELECT id, budget_key, version_no, display_name, description,
                    total_points, ranked_eligible, canonical_sha256
             FROM point_budget_version
             WHERE id = ? AND status = 'sealed'",
        )
        .bind(version_id.get())
        .fetch_optional(&self.pool)
        .await?;
        let Some(budget) = budget else {
            return Ok(None);
        };
        let catalog = read_budget_children(&self.pool, budget).await?;

        Ok(Some(self.rules.evaluate(&catalog, selections)))
    }

    async fn run_manifest(
        &self,
        user_id: u64,
        run_revision: u32,
    ) -> Result<Option<RunManifestSummary>> {
        sqlx::query_as::<_, ManifestRow>(
            "SELECT manifest.run_revision, manifest.mode, manifest.manifest_sha256
             FROM run_manifest AS manifest
             INNER JOIN save ON save.id = manifest.save_id
             WHERE save.user_id = ? AND manifest.run_revision = ?",
        )
        .bind(user_id)
        .bind(run_revision)
        .fetch_optional(&self.pool)
        .await?
        .map(|row| {
            Ok(RunManifestSummary {
                run_revision: row.run_revision,
                mode: to_run_mode(&row.mode)?,
                manifest_sha256: row.manifest_sha256,
            })
        })
        .transpose()
    }
}

fn to_preset(row: PresetRow) -> Result<CharacterPresetVersion> {
    let draft = serde_json::from_str::<CharacterDraft>(&row.canonical_draft_json)
        .context("stored character preset draft is invalid")?;
    Ok(CharacterPresetVersion {
        id: ResourceId::from_u64(row.id),
        preset_key: row.preset_key,
        version: row.version_no,
        display_name: row.display_name,
        summary: row.summary,
        ranked_eligible: row.ranked_eligible,
        canonical_sha256: row.canonical_sha256,
        draft,
    })
}

async fn read_budget_children(pool: &MySqlPool, row: BudgetRow) -> Result<PointBudgetCatalog> {
    let groups = sqlx::query_as::<_, GroupRow>(
        "SELECT group_key, display_name
         FROM point_budget_exclusive_group
         WHERE point_budget_version_id = ?
         ORDER BY display_order, group_key",
    )
    .bind(row.id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|group| PointExclusiveGroup {
        group_key: group.group_key,
        display_name: group.display_name,
    })
    .collect::<Vec<_>>();
    let option_rows = sqlx::query_as::<_, OptionRow>(
        "SELECT id, option_key, display_name, description, cost_kind,
                point_delta_per_unit, minimum_quantity, maximum_quantity,
                exclusive_group_key, CAST(effect_json AS CHAR) AS effect_json
         FROM point_budget_option
         WHERE point_budget_version_id = ?
         ORDER BY display_order, id",
    )
    .bind(row.id)
    .fetch_all(pool)
    .await?;
    let option_ids = option_rows
        .iter()
        .map(|option| option.id)
        .collect::<Vec<_>>();
    let mut tiers = BTreeMap::<u64, Vec<PointTier>>::new();
    let mut conditions = BTreeMap::<u64, Vec<PointCondition>>::new();
    if !option_ids.is_empty() {
        let tier_rows = sqlx::query_as::<_, TierRow>(
            "SELECT tier.point_budget_option_id, tier.minimum_quantity,
                    tier.maximum_quantity, tier.point_delta_per_unit
             FROM point_budget_option_tier AS tier
             INNER JOIN point_budget_option AS option_row
                ON option_row.id = tier.point_budget_option_id
             WHERE option_row.point_budget_version_id = ?
             ORDER BY tier.point_budget_option_id, tier.tier_order",
        )
        .bind(row.id)
        .fetch_all(pool)
        .await?;
        for tier in tier_rows {
            tiers
                .entry(tier.point_budget_option_id)
                .or_default()
                .push(PointTier {
                    minimum_quantity: tier.minimum_quantity,
                    maximum_quantity: tier.maximum_quantity,
                    point_delta_per_unit: tier.point_delta_per_unit,
                });
        }
        let condition_rows = sqlx::query_as::<_, ConditionRow>(
            "SELECT point_budget_option_id, condition_kind, related_option_id,
                    fact_path, comparison_kind, fact_value_kind,
                    fact_integer_value, fact_text_value
             FROM point_budget_option_condition
             WHERE point_budget_version_id = ?
             ORDER BY point_budget_option_id, condition_order, id",
        )
        .bind(row.id)
        .fetch_all(pool)
        .await?;
        for condition in condition_rows {
            conditions
                .entry(condition.point_budget_option_id)
                .or_default()
                .push(to_condition(condition)?);
        }
    }
    let options = option_rows
        .into_iter()
        .map(|option| {
            let id = option.id;
            Ok(PointBudgetOption {
                id: ResourceId::from_u64(id),
                option_key: option.option_key,
                display_name: option.display_name,
                description: option.description,
                cost_kind: to_cost_kind(&option.cost_kind)?,
                point_delta_per_unit: option.point_delta_per_unit,
                minimum_quantity: option.minimum_quantity,
                maximum_quantity: option.maximum_quantity,
                exclusive_group_key: option.exclusive_group_key,
                effect: serde_json::from_str::<PointEffect>(&option.effect_json)
                    .context("stored point-budget effect is invalid")?,
                tiers: tiers.remove(&id).unwrap_or_default(),
                conditions: conditions.remove(&id).unwrap_or_default(),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(PointBudgetCatalog {
        id: ResourceId::from_u64(row.id),
        budget_key: row.budget_key,
        version: row.version_no,
        display_name: row.display_name,
        description: row.description,
        total_points: row.total_points,
        ranked_eligible: row.ranked_eligible,
        canonical_sha256: row.canonical_sha256,
        groups,
        options,
    })
}

fn to_cost_kind(raw: &str) -> Result<PointCostKind> {
    match raw {
        "fixed" => Ok(PointCostKind::Fixed),
        "perUnit" => Ok(PointCostKind::PerUnit),
        "tiered" => Ok(PointCostKind::Tiered),
        _ => bail!("stored point-budget cost kind is invalid"),
    }
}

fn to_run_mode(raw: &str) -> Result<RunMode> {
    match raw {
        "rankedPreset" => Ok(RunMode::RankedPreset),
        "rankedCustom" => Ok(RunMode::RankedCustom),
        "sandbox" => Ok(RunMode::Sandbox),
        _ => bail!("stored run mode is invalid"),
    }
}

fn to_condition(row: ConditionRow) -> Result<PointCondition> {
    let related = || {
        row.related_option_id
            .map(ResourceId::from_u64)
            .context("stored option condition has no related option")
    };
    match row.condition_kind.as_str() {
        "requiresOption" => Ok(PointCondition::RequiresOption {
            option_id: related()?,
        }),
        "forbidsOption" => Ok(PointCondition::ForbidsOption {
            option_id: related()?,
        }),
        "requiresFact" => Ok(PointCondition::RequiresFact {
            fact_path: row
                .fact_path
                .clone()
                .context("stored fact condition has no path")?,
            comparison: to_comparison(row.comparison_kind.as_deref())?,
            expected: to_fact_value(&row)?,
        }),
        "forbidsFact" => Ok(PointCondition::ForbidsFact {
            fact_path: row
                .fact_path
                .clone()
                .context("stored fact condition has no path")?,
            comparison: to_comparison(row.comparison_kind.as_deref())?,
            expected: to_fact_value(&row)?,
        }),
        _ => bail!("stored point-budget condition kind is invalid"),
    }
}

fn to_comparison(raw: Option<&str>) -> Result<PointFactComparison> {
    match raw {
        Some("equal") => Ok(PointFactComparison::Equal),
        Some("greaterOrEqual") => Ok(PointFactComparison::GreaterOrEqual),
        Some("lessOrEqual") => Ok(PointFactComparison::LessOrEqual),
        _ => bail!("stored fact comparison is invalid"),
    }
}

fn to_fact_value(row: &ConditionRow) -> Result<PointFactValue> {
    match row.fact_value_kind.as_deref() {
        Some("integer") => row
            .fact_integer_value
            .map(PointFactValue::Integer)
            .context("stored integer fact condition has no value"),
        Some("text") => row
            .fact_text_value
            .clone()
            .map(PointFactValue::Text)
            .context("stored text fact condition has no value"),
        _ => bail!("stored fact value kind is invalid"),
    }
}
