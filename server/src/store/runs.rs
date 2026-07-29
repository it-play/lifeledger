use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use async_trait::async_trait;
use sqlx::MySqlPool;

use super::types::RunStore;
use crate::character::CharacterDraft;
use crate::finance::ResourceId;
use crate::runs::{
    CharacterPresetVersion, LeagueDefinition, LeagueRankingItem, LeagueRankingPage,
    PointBudgetCatalog, PointBudgetEvaluation, PointBudgetOption, PointBudgetRules, PointCondition,
    PointCostKind, PointEffect, PointExclusiveGroup, PointFactComparison, PointFactValue,
    PointSelection, PointTier, RankedRunContext, RankedRunPreparation, RankingPageCursor,
    RunFinalization, RunFinalizationLine, RunFinalizationStatus, RunManifestSummary, RunMode,
    RunOptions, SeasonLeagues, SeasonStatus, SeasonSummary, create_point_budget_rules,
    encode_ranking_cursor,
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

#[derive(Debug, sqlx::FromRow)]
struct SeasonRow {
    id: u64,
    season_key: String,
    version_no: u32,
    display_name: String,
    status: String,
    target_game_day: u32,
    registration_open_at: String,
    registration_close_at: String,
    operation_close_at: String,
}

#[derive(Debug, sqlx::FromRow)]
struct LeagueRow {
    id: u64,
    season_id: u64,
    league_key: String,
    display_name: String,
    mode: String,
    character_preset_version_id: Option<u64>,
    point_budget_version_id: Option<u64>,
    minimum_participants: u32,
    participant_count: u64,
}

#[derive(Debug, sqlx::FromRow)]
struct LeagueRankingMetaRow {
    id: u64,
    season_id: u64,
    display_name: String,
    minimum_participants: u32,
    season_status: String,
    finalized_count: u64,
    failure_count: u64,
}

#[derive(Debug, sqlx::FromRow)]
struct LeagueRankingRow {
    rank_no: u64,
    save_id: u64,
    run_revision: u32,
    run_id: String,
    character_preset_version_id: Option<u64>,
    point_budget_version_id: Option<u64>,
    after_tax_net_worth_krw: i64,
    insolvency_days: u32,
    player_command_count: u64,
    completed_at: String,
}

#[derive(Debug, sqlx::FromRow)]
struct RunFinalizationRow {
    target_game_day: u32,
    status: Option<String>,
    after_tax_net_worth_krw: Option<i64>,
    insolvency_days: Option<u32>,
    player_command_count: Option<u64>,
    liquidation_sha256: Option<String>,
    failure_code: Option<String>,
    completed_at: Option<String>,
    finalization_id: Option<u64>,
}

#[derive(Debug, sqlx::FromRow)]
struct RunFinalizationLineRow {
    line_no: u32,
    component_key: String,
    gross_krw: i64,
    cost_krw: i64,
    tax_krw: i64,
    net_krw: i64,
    policy_reference: String,
    line_sha256: String,
}

#[derive(Debug, sqlx::FromRow)]
struct RankedContextRow {
    season_id: u64,
    league_definition_id: u64,
    mode: String,
    season_assignment_revision: u64,
    ranked_ruleset_release_id: u64,
    ranked_ruleset_release_sha256: String,
    ranking_rule_version_id: u64,
    ranking_rule_sha256: String,
    target_game_day: u32,
    character_preset_version_id: Option<u64>,
    point_budget_version_id: Option<u64>,
}

#[async_trait]
impl RunStore for MySqlRunStore {
    async fn run_options(&self) -> Result<RunOptions> {
        let active_season_id = sqlx::query_scalar::<_, u64>(
            "SELECT season_row.id
             FROM season_assignment AS assignment
             INNER JOIN season AS season_row ON season_row.id = assignment.season_id
             WHERE assignment.assignment_key = 'rankedRun'
               AND season_row.status IN ('registrationOpen', 'active')
               AND CURRENT_TIMESTAMP(6) >= season_row.registration_open_at
               AND CURRENT_TIMESTAMP(6) < season_row.registration_close_at",
        )
        .fetch_optional(&self.pool)
        .await?
        .map(ResourceId::from_u64);
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
            active_season_id,
            presets,
            point_budgets,
            sandbox_available: true,
        })
    }

    async fn season_leagues(&self, season_id: ResourceId) -> Result<Option<SeasonLeagues>> {
        let season = sqlx::query_as::<_, SeasonRow>(
            "SELECT season_row.id, season_row.season_key, season_row.version_no,
                    season_row.display_name, season_row.status, ranking_rule.target_game_day,
                    DATE_FORMAT(
                        season_row.registration_open_at,
                        '%Y-%m-%dT%H:%i:%s.%fZ'
                    ) AS registration_open_at,
                    DATE_FORMAT(
                        season_row.registration_close_at,
                        '%Y-%m-%dT%H:%i:%s.%fZ'
                    ) AS registration_close_at,
                    DATE_FORMAT(
                        season_row.operation_close_at,
                        '%Y-%m-%dT%H:%i:%s.%fZ'
                    ) AS operation_close_at
             FROM season AS season_row
             INNER JOIN ranking_rule_version AS ranking_rule
                ON ranking_rule.id = season_row.ranking_rule_version_id
               AND BINARY ranking_rule.ranking_rule_sha256
                    = BINARY season_row.ranking_rule_sha256
             WHERE season_row.id = ?",
        )
        .bind(season_id.get())
        .fetch_optional(&self.pool)
        .await?;
        let Some(season) = season else {
            return Ok(None);
        };
        let season_status = to_season_status(&season.status)?;
        let league_rows = sqlx::query_as::<_, LeagueRow>(
            "SELECT league.id, league.season_id, league.league_key, league.display_name,
                    league.mode, league.character_preset_version_id,
                    league.point_budget_version_id, league.minimum_participants,
                    CAST(COUNT(manifest.save_id) AS UNSIGNED) AS participant_count
             FROM league_definition AS league
             LEFT JOIN run_manifest AS manifest
               ON manifest.season_id = league.season_id
              AND manifest.league_definition_id = league.id
              AND manifest.ranking_eligible = TRUE
             WHERE league.season_id = ?
             GROUP BY league.id, league.season_id, league.league_key, league.display_name,
                      league.mode, league.character_preset_version_id,
                      league.point_budget_version_id, league.minimum_participants,
                      league.display_order
             ORDER BY league.display_order, league.id",
        )
        .bind(season_id.get())
        .fetch_all(&self.pool)
        .await?;
        let leagues = league_rows
            .into_iter()
            .map(|league| {
                Ok(LeagueDefinition {
                    id: ResourceId::from_u64(league.id),
                    season_id: ResourceId::from_u64(league.season_id),
                    league_key: league.league_key,
                    display_name: league.display_name,
                    mode: to_run_mode(&league.mode)?,
                    character_preset_version_id: league
                        .character_preset_version_id
                        .map(ResourceId::from_u64),
                    point_budget_version_id: league
                        .point_budget_version_id
                        .map(ResourceId::from_u64),
                    minimum_participants: league.minimum_participants,
                    participant_count: league.participant_count,
                    provisional: season_status != SeasonStatus::Finalized
                        || league.participant_count < u64::from(league.minimum_participants),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Some(SeasonLeagues {
            season: SeasonSummary {
                id: ResourceId::from_u64(season.id),
                season_key: season.season_key,
                version: season.version_no,
                display_name: season.display_name,
                status: season_status,
                target_game_day: season.target_game_day,
                registration_open_at: season.registration_open_at,
                registration_close_at: season.registration_close_at,
                operation_close_at: season.operation_close_at,
            },
            leagues,
        }))
    }

    async fn league_rankings(
        &self,
        league_id: ResourceId,
        cursor: Option<RankingPageCursor>,
        limit: u32,
    ) -> Result<Option<LeagueRankingPage>> {
        ensure!((1..=100).contains(&limit), "ranking page limit is invalid");
        let meta = sqlx::query_as::<_, LeagueRankingMetaRow>(
            "SELECT league.id, league.season_id, league.display_name,
                    league.minimum_participants, season_row.status AS season_status,
                    CAST((
                        SELECT COUNT(*)
                        FROM run_finalization AS finalization
                        INNER JOIN run_manifest AS manifest
                           ON manifest.save_id = finalization.save_id
                          AND manifest.run_revision = finalization.run_revision
                        WHERE manifest.league_definition_id = league.id
                          AND finalization.status = 'completed'
                    ) AS UNSIGNED) AS finalized_count,
                    CAST((
                        SELECT COUNT(*)
                        FROM run_finalization AS finalization
                        INNER JOIN run_manifest AS manifest
                           ON manifest.save_id = finalization.save_id
                          AND manifest.run_revision = finalization.run_revision
                        WHERE manifest.league_definition_id = league.id
                          AND finalization.status = 'failed'
                    ) AS UNSIGNED) AS failure_count
             FROM league_definition AS league
             INNER JOIN season AS season_row ON season_row.id = league.season_id
             WHERE league.id = ?",
        )
        .bind(league_id.get())
        .fetch_optional(&self.pool)
        .await?;
        let Some(meta) = meta else {
            return Ok(None);
        };

        let fetch_limit = u64::from(limit) + 1;
        let mut rows = match cursor {
            Some(cursor) => {
                sqlx::query_as::<_, LeagueRankingRow>(
                    "SELECT ranked.rank_no, ranked.save_id, ranked.run_revision,
                            ranked.run_id, ranked.character_preset_version_id,
                            ranked.point_budget_version_id,
                            ranked.after_tax_net_worth_krw, ranked.insolvency_days,
                            ranked.player_command_count, ranked.completed_at
                     FROM (
                         SELECT CAST(ROW_NUMBER() OVER (
                                    ORDER BY finalization.after_tax_net_worth_krw DESC,
                                             finalization.insolvency_days,
                                             finalization.player_command_count,
                                             finalization.save_id,
                                             finalization.run_revision
                                ) AS UNSIGNED) AS rank_no,
                                finalization.save_id, finalization.run_revision,
                                manifest.manifest_sha256 AS run_id,
                                manifest.character_preset_version_id,
                                manifest.point_budget_version_id,
                                finalization.after_tax_net_worth_krw,
                                finalization.insolvency_days,
                                finalization.player_command_count,
                                DATE_FORMAT(
                                    finalization.completed_at,
                                    '%Y-%m-%dT%H:%i:%s.%fZ'
                                ) AS completed_at
                         FROM run_finalization AS finalization
                         INNER JOIN run_manifest AS manifest
                            ON manifest.save_id = finalization.save_id
                           AND manifest.run_revision = finalization.run_revision
                         WHERE manifest.league_definition_id = ?
                           AND finalization.status = 'completed'
                     ) AS ranked
                     CROSS JOIN (
                         SELECT ? AS after_tax_net_worth_krw,
                                ? AS insolvency_days,
                                ? AS player_command_count,
                                ? AS save_id,
                                ? AS run_revision
                     ) AS cursor_row
                     WHERE ranked.after_tax_net_worth_krw
                                < cursor_row.after_tax_net_worth_krw
                        OR (
                            ranked.after_tax_net_worth_krw
                                = cursor_row.after_tax_net_worth_krw
                            AND ranked.insolvency_days > cursor_row.insolvency_days
                        )
                        OR (
                            ranked.after_tax_net_worth_krw
                                = cursor_row.after_tax_net_worth_krw
                            AND ranked.insolvency_days = cursor_row.insolvency_days
                            AND ranked.player_command_count > cursor_row.player_command_count
                        )
                        OR (
                            ranked.after_tax_net_worth_krw
                                = cursor_row.after_tax_net_worth_krw
                            AND ranked.insolvency_days = cursor_row.insolvency_days
                            AND ranked.player_command_count = cursor_row.player_command_count
                            AND ranked.save_id > cursor_row.save_id
                        )
                        OR (
                            ranked.after_tax_net_worth_krw
                                = cursor_row.after_tax_net_worth_krw
                            AND ranked.insolvency_days = cursor_row.insolvency_days
                            AND ranked.player_command_count = cursor_row.player_command_count
                            AND ranked.save_id = cursor_row.save_id
                            AND ranked.run_revision > cursor_row.run_revision
                        )
                     ORDER BY ranked.after_tax_net_worth_krw DESC,
                              ranked.insolvency_days,
                              ranked.player_command_count,
                              ranked.save_id,
                              ranked.run_revision
                     LIMIT ?",
                )
                .bind(meta.id)
                .bind(cursor.after_tax_net_worth_krw)
                .bind(cursor.insolvency_days)
                .bind(cursor.player_command_count)
                .bind(cursor.save_id)
                .bind(cursor.run_revision)
                .bind(fetch_limit)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, LeagueRankingRow>(
                    "SELECT CAST(ROW_NUMBER() OVER (
                                ORDER BY finalization.after_tax_net_worth_krw DESC,
                                         finalization.insolvency_days,
                                         finalization.player_command_count,
                                         finalization.save_id,
                                         finalization.run_revision
                            ) AS UNSIGNED) AS rank_no,
                            finalization.save_id, finalization.run_revision,
                            manifest.manifest_sha256 AS run_id,
                            manifest.character_preset_version_id,
                            manifest.point_budget_version_id,
                            finalization.after_tax_net_worth_krw,
                            finalization.insolvency_days,
                            finalization.player_command_count,
                            DATE_FORMAT(
                                finalization.completed_at,
                                '%Y-%m-%dT%H:%i:%s.%fZ'
                            ) AS completed_at
                     FROM run_finalization AS finalization
                     INNER JOIN run_manifest AS manifest
                        ON manifest.save_id = finalization.save_id
                       AND manifest.run_revision = finalization.run_revision
                     WHERE manifest.league_definition_id = ?
                       AND finalization.status = 'completed'
                     ORDER BY finalization.after_tax_net_worth_krw DESC,
                              finalization.insolvency_days,
                              finalization.player_command_count,
                              finalization.save_id,
                              finalization.run_revision
                     LIMIT ?",
                )
                .bind(meta.id)
                .bind(fetch_limit)
                .fetch_all(&self.pool)
                .await?
            }
        };
        let has_more = rows.len() > usize::try_from(limit)?;
        rows.truncate(usize::try_from(limit)?);
        let next_cursor = if has_more {
            rows.last()
                .map(ranking_cursor_from_row)
                .map(encode_ranking_cursor)
                .transpose()?
        } else {
            None
        };
        let items = rows
            .into_iter()
            .map(|row| LeagueRankingItem {
                rank: row.rank_no,
                run_id: row.run_id,
                display_name: "익명 플레이어".to_owned(),
                character_preset_version_id: row
                    .character_preset_version_id
                    .map(ResourceId::from_u64),
                point_budget_version_id: row.point_budget_version_id.map(ResourceId::from_u64),
                after_tax_net_worth_krw: row.after_tax_net_worth_krw,
                completed_at: row.completed_at,
            })
            .collect();

        Ok(Some(LeagueRankingPage {
            league_id: ResourceId::from_u64(meta.id),
            season_id: ResourceId::from_u64(meta.season_id),
            league_display_name: meta.display_name,
            provisional: meta.season_status != "finalized"
                || meta.finalized_count < u64::from(meta.minimum_participants)
                || meta.failure_count > 0,
            finalized_count: meta.finalized_count,
            items,
            next_cursor,
        }))
    }

    async fn run_finalization(
        &self,
        user_id: u64,
        run_revision: u32,
    ) -> Result<Option<RunFinalization>> {
        let row = sqlx::query_as::<_, RunFinalizationRow>(
            "SELECT manifest.target_game_day, finalization.status,
                    finalization.after_tax_net_worth_krw, finalization.insolvency_days,
                    finalization.player_command_count, finalization.liquidation_sha256,
                    finalization.failure_code,
                    DATE_FORMAT(finalization.completed_at, '%Y-%m-%dT%H:%i:%s.%fZ') AS completed_at,
                    finalization.id AS finalization_id
             FROM save
             INNER JOIN run_manifest AS manifest ON manifest.save_id = save.id
             LEFT JOIN run_finalization AS finalization
               ON finalization.save_id = manifest.save_id
              AND finalization.run_revision = manifest.run_revision
              AND finalization.target_game_day = manifest.target_game_day
              AND finalization.ranking_rule_version_id = manifest.ranking_rule_version_id
             WHERE save.user_id = ? AND manifest.run_revision = ?
               AND manifest.ranking_eligible = TRUE",
        )
        .bind(user_id)
        .bind(run_revision)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let status = match row.status.as_deref() {
            None | Some("planning") => RunFinalizationStatus::Pending,
            Some("completed") => RunFinalizationStatus::Completed,
            Some("failed") => RunFinalizationStatus::Failed,
            Some(_) => bail!("stored finalization status is invalid"),
        };
        let lines = if status == RunFinalizationStatus::Completed {
            let id = row
                .finalization_id
                .context("completed finalization has no id")?;
            sqlx::query_as::<_, RunFinalizationLineRow>(
                "SELECT line_no, component_key, gross_krw, cost_krw, tax_krw, net_krw,
                        policy_reference, line_sha256
                 FROM liquidation_line WHERE run_finalization_id = ? ORDER BY line_no",
            )
            .bind(id)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|line| RunFinalizationLine {
                line_no: line.line_no,
                component_key: line.component_key,
                gross_krw: line.gross_krw,
                cost_krw: line.cost_krw,
                tax_krw: line.tax_krw,
                net_krw: line.net_krw,
                policy_reference: line.policy_reference,
                line_sha256: line.line_sha256,
            })
            .collect()
        } else {
            Vec::new()
        };
        Ok(Some(RunFinalization {
            run_revision,
            target_game_day: row.target_game_day,
            status,
            after_tax_net_worth_krw: row.after_tax_net_worth_krw,
            insolvency_days: row.insolvency_days,
            player_command_count: row.player_command_count,
            liquidation_sha256: row.liquidation_sha256,
            failure_code: row.failure_code,
            completed_at: row.completed_at,
            lines,
        }))
    }

    async fn prepare_ranked_preset(
        &self,
        preset_version_id: ResourceId,
    ) -> Result<Option<RankedRunPreparation>> {
        let Some(context) =
            read_ranked_context(&self.pool, RunMode::RankedPreset, preset_version_id).await?
        else {
            return Ok(None);
        };
        let preset = sqlx::query_as::<_, PresetRow>(
            "SELECT id, preset_key, version_no, display_name, summary, ranked_eligible,
                    canonical_draft_json, canonical_sha256
             FROM character_preset_version
             WHERE id = ? AND status = 'sealed'",
        )
        .bind(preset_version_id.get())
        .fetch_optional(&self.pool)
        .await?;
        let Some(preset) = preset else {
            return Ok(None);
        };

        Ok(Some(RankedRunPreparation {
            context: to_ranked_context(context, "[]".to_owned())?,
            draft: to_preset(preset)?.draft,
        }))
    }

    async fn prepare_ranked_custom(
        &self,
        budget_version_id: ResourceId,
        selections: &[PointSelection],
    ) -> Result<Option<RankedRunPreparation>> {
        let Some(context) =
            read_ranked_context(&self.pool, RunMode::RankedCustom, budget_version_id).await?
        else {
            return Ok(None);
        };
        let budget = sqlx::query_as::<_, BudgetRow>(
            "SELECT id, budget_key, version_no, display_name, description,
                    total_points, ranked_eligible, canonical_sha256
             FROM point_budget_version
             WHERE id = ? AND status = 'sealed'",
        )
        .bind(budget_version_id.get())
        .fetch_optional(&self.pool)
        .await?;
        let Some(budget) = budget else {
            return Ok(None);
        };
        let catalog = read_budget_children(&self.pool, budget).await?;
        let prepared = self.rules.prepare(&catalog, selections);
        let Some(draft) = prepared.draft else {
            return Ok(None);
        };
        let mut canonical_selections = selections.to_vec();
        canonical_selections.sort_by_key(|selection| selection.option_id.get());
        let canonical_selections_json = serde_json::to_string(&canonical_selections)
            .context("failed to serialize ranked point selections")?;

        Ok(Some(RankedRunPreparation {
            context: to_ranked_context(context, canonical_selections_json)?,
            draft,
        }))
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

async fn read_ranked_context(
    pool: &MySqlPool,
    mode: RunMode,
    version_id: ResourceId,
) -> Result<Option<RankedContextRow>> {
    let mode = match mode {
        RunMode::RankedPreset => "rankedPreset",
        RunMode::RankedCustom => "rankedCustom",
        RunMode::Sandbox => bail!("sandbox has no ranked context"),
    };
    sqlx::query_as::<_, RankedContextRow>(
        "SELECT season_row.id AS season_id, league.id AS league_definition_id,
                league.mode, assignment.assignment_revision AS season_assignment_revision,
                release_row.id AS ranked_ruleset_release_id,
                release_row.release_sha256 AS ranked_ruleset_release_sha256,
                ranking_rule.id AS ranking_rule_version_id,
                ranking_rule.ranking_rule_sha256, ranking_rule.target_game_day,
                league.character_preset_version_id, league.point_budget_version_id
         FROM season_assignment AS assignment
         INNER JOIN season AS season_row ON season_row.id = assignment.season_id
         INNER JOIN ranked_ruleset_release AS release_row
            ON release_row.id = season_row.ranked_ruleset_release_id
           AND BINARY release_row.release_sha256
                = BINARY season_row.ranked_ruleset_release_sha256
         INNER JOIN ranking_rule_version AS ranking_rule
            ON ranking_rule.id = season_row.ranking_rule_version_id
           AND BINARY ranking_rule.ranking_rule_sha256 = BINARY season_row.ranking_rule_sha256
         INNER JOIN league_definition AS league ON league.season_id = season_row.id
         WHERE assignment.assignment_key = 'rankedRun'
           AND season_row.status IN ('registrationOpen', 'active')
           AND CURRENT_TIMESTAMP(6) >= season_row.registration_open_at
           AND CURRENT_TIMESTAMP(6) < season_row.registration_close_at
           AND league.mode = ?
           AND (
               (league.mode = 'rankedPreset' AND league.character_preset_version_id = ?)
               OR (league.mode = 'rankedCustom' AND league.point_budget_version_id = ?)
           )",
    )
    .bind(mode)
    .bind(version_id.get())
    .bind(version_id.get())
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}

fn to_ranked_context(
    row: RankedContextRow,
    canonical_selections_json: String,
) -> Result<RankedRunContext> {
    let mode = to_run_mode(&row.mode)?;
    Ok(RankedRunContext {
        mode,
        season_id: ResourceId::from_u64(row.season_id),
        league_definition_id: ResourceId::from_u64(row.league_definition_id),
        season_assignment_revision: row.season_assignment_revision,
        ranked_ruleset_release_id: ResourceId::from_u64(row.ranked_ruleset_release_id),
        ranked_ruleset_release_sha256: row.ranked_ruleset_release_sha256,
        ranking_rule_version_id: ResourceId::from_u64(row.ranking_rule_version_id),
        ranking_rule_sha256: row.ranking_rule_sha256,
        target_game_day: row.target_game_day,
        character_preset_version_id: row.character_preset_version_id.map(ResourceId::from_u64),
        point_budget_version_id: row.point_budget_version_id.map(ResourceId::from_u64),
        canonical_selections_json,
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

fn ranking_cursor_from_row(row: &LeagueRankingRow) -> RankingPageCursor {
    RankingPageCursor {
        after_tax_net_worth_krw: row.after_tax_net_worth_krw,
        insolvency_days: row.insolvency_days,
        player_command_count: row.player_command_count,
        save_id: row.save_id,
        run_revision: row.run_revision,
    }
}

fn to_season_status(raw: &str) -> Result<SeasonStatus> {
    match raw {
        "draft" => Ok(SeasonStatus::Draft),
        "registrationOpen" => Ok(SeasonStatus::RegistrationOpen),
        "active" => Ok(SeasonStatus::Active),
        "locked" => Ok(SeasonStatus::Locked),
        "finalized" => Ok(SeasonStatus::Finalized),
        "archived" => Ok(SeasonStatus::Archived),
        _ => bail!("stored season status is invalid"),
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
