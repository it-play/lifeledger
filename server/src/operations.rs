use anyhow::{Context, ensure};
use serde::Serialize;
use sqlx::MySqlPool;

use crate::ENGINE_VERSION;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OperationsReport {
    schema_version: u16,
    generated_at: String,
    engine_version: &'static str,
    migrations: MigrationMetrics,
    offline_progress: OfflineProgressMetrics,
    seasons: SeasonMetrics,
    finalizations: FinalizationMetrics,
    feedback_retention: FeedbackRetentionMetrics,
    alerts: Vec<OperationsAlertCode>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
struct MigrationMetrics {
    latest_successful_version: i64,
    failed_count: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
struct OfflineProgressMetrics {
    enabled_run_count: i64,
    pending_run_count: i64,
    pending_day_count: i64,
    oldest_accrual_age_seconds: i64,
    paused_run_count: i64,
    active_worker_lease_count: i64,
    expired_worker_lease_count: i64,
    committed_last_hour_count: i64,
    failed_last_hour_count: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
struct SeasonMetrics {
    draft_count: i64,
    registration_open_count: i64,
    active_count: i64,
    locked_count: i64,
    finalized_count: i64,
    archived_count: i64,
    ranked_run_count: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
struct FinalizationMetrics {
    completed_count: i64,
    failed_count: i64,
    failed_last_hour_count: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
struct FeedbackRetentionMetrics {
    active_count: i64,
    expired_count: i64,
    overdue_active_count: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum OperationsAlertCode {
    MigrationFailure,
    OfflineProgressPaused,
    ExpiredWorkerLease,
    RecentFinalizationFailure,
    ExpiredFeedbackRetention,
}

pub(super) async fn run(pool: MySqlPool, require_clean_migrations: bool) -> anyhow::Result<()> {
    let generated_at: String =
        sqlx::query_scalar("SELECT DATE_FORMAT(UTC_TIMESTAMP(6), '%Y-%m-%dT%H:%i:%s.%fZ')")
            .fetch_one(&pool)
            .await
            .context("failed to read database time")?;
    let migrations = read_migration_metrics(&pool).await?;
    let offline_progress = read_offline_progress_metrics(&pool).await?;
    let seasons = read_season_metrics(&pool).await?;
    let finalizations = read_finalization_metrics(&pool).await?;
    let feedback_retention = read_feedback_retention_metrics(&pool).await?;
    validate_nonnegative_metrics(
        &migrations,
        &offline_progress,
        &seasons,
        &finalizations,
        &feedback_retention,
    )?;
    let alerts = classify_alerts(
        &migrations,
        &offline_progress,
        &finalizations,
        &feedback_retention,
    );
    let failed_migration_count = migrations.failed_count;
    let report = OperationsReport {
        schema_version: 2,
        generated_at,
        engine_version: ENGINE_VERSION,
        migrations,
        offline_progress,
        seasons,
        finalizations,
        feedback_retention,
        alerts,
    };
    println!("{}", serde_json::to_string(&report)?);
    ensure!(
        !require_clean_migrations || failed_migration_count == 0,
        "failed database migration exists"
    );
    Ok(())
}

async fn read_migration_metrics(pool: &MySqlPool) -> anyhow::Result<MigrationMetrics> {
    sqlx::query_as(
        "SELECT
            CAST(COALESCE(MAX(CASE WHEN success = TRUE THEN version END), 0) AS SIGNED)
                AS latest_successful_version,
            CAST(COALESCE(SUM(CASE WHEN success = FALSE THEN 1 ELSE 0 END), 0) AS SIGNED)
                AS failed_count
         FROM _sqlx_migrations",
    )
    .fetch_one(pool)
    .await
    .context("failed to read migration metrics")
}

async fn read_offline_progress_metrics(pool: &MySqlPool) -> anyhow::Result<OfflineProgressMetrics> {
    sqlx::query_as(
        "SELECT
            CAST((SELECT COUNT(*) FROM offline_progress_setting WHERE enabled = TRUE) AS SIGNED)
                AS enabled_run_count,
            CAST((SELECT COUNT(*) FROM offline_progress_setting WHERE pending_days > 0) AS SIGNED)
                AS pending_run_count,
            CAST((SELECT COALESCE(SUM(pending_days), 0) FROM offline_progress_setting) AS SIGNED)
                AS pending_day_count,
            CAST(COALESCE((
                SELECT GREATEST(TIMESTAMPDIFF(SECOND, MIN(accrued_through), UTC_TIMESTAMP(6)), 0)
                FROM offline_progress_setting
                WHERE pending_days > 0 AND accrued_through IS NOT NULL
            ), 0) AS SIGNED) AS oldest_accrual_age_seconds,
            CAST((SELECT COUNT(*) FROM offline_progress_setting
                  WHERE status = 'pausedBySystem') AS SIGNED) AS paused_run_count,
            CAST((SELECT COUNT(*) FROM progress_lease
                  WHERE holder_kind = 'worker' AND expires_at > UTC_TIMESTAMP(6)) AS SIGNED)
                AS active_worker_lease_count,
            CAST((SELECT COUNT(*) FROM progress_lease
                  WHERE holder_kind = 'worker' AND expires_at <= UTC_TIMESTAMP(6)) AS SIGNED)
                AS expired_worker_lease_count,
            CAST((SELECT COUNT(*) FROM offline_progress_attempt
                  WHERE event_kind = 'committed'
                    AND created_at >= UTC_TIMESTAMP(6) - INTERVAL 1 HOUR) AS SIGNED)
                AS committed_last_hour_count,
            CAST((SELECT COUNT(*) FROM offline_progress_attempt
                  WHERE event_kind = 'failed'
                    AND created_at >= UTC_TIMESTAMP(6) - INTERVAL 1 HOUR) AS SIGNED)
                AS failed_last_hour_count",
    )
    .fetch_one(pool)
    .await
    .context("failed to read offline progress metrics")
}

async fn read_season_metrics(pool: &MySqlPool) -> anyhow::Result<SeasonMetrics> {
    sqlx::query_as(
        "SELECT
            CAST(COALESCE(SUM(CASE WHEN status = 'draft' THEN 1 ELSE 0 END), 0) AS SIGNED)
                AS draft_count,
            CAST(COALESCE(SUM(CASE WHEN status = 'registrationOpen' THEN 1 ELSE 0 END), 0)
                AS SIGNED) AS registration_open_count,
            CAST(COALESCE(SUM(CASE WHEN status = 'active' THEN 1 ELSE 0 END), 0) AS SIGNED)
                AS active_count,
            CAST(COALESCE(SUM(CASE WHEN status = 'locked' THEN 1 ELSE 0 END), 0) AS SIGNED)
                AS locked_count,
            CAST(COALESCE(SUM(CASE WHEN status = 'finalized' THEN 1 ELSE 0 END), 0) AS SIGNED)
                AS finalized_count,
            CAST(COALESCE(SUM(CASE WHEN status = 'archived' THEN 1 ELSE 0 END), 0) AS SIGNED)
                AS archived_count,
            CAST((SELECT COUNT(*) FROM run_manifest WHERE ranking_eligible = TRUE) AS SIGNED)
                AS ranked_run_count
         FROM season",
    )
    .fetch_one(pool)
    .await
    .context("failed to read season metrics")
}

async fn read_finalization_metrics(pool: &MySqlPool) -> anyhow::Result<FinalizationMetrics> {
    sqlx::query_as(
        "SELECT
            CAST(COALESCE(SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END), 0) AS SIGNED)
                AS completed_count,
            CAST(COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0) AS SIGNED)
                AS failed_count,
            CAST(COALESCE(SUM(CASE WHEN status = 'failed'
                                   AND completed_at >= UTC_TIMESTAMP(6) - INTERVAL 1 HOUR
                                  THEN 1 ELSE 0 END), 0) AS SIGNED) AS failed_last_hour_count
         FROM run_finalization",
    )
    .fetch_one(pool)
    .await
    .context("failed to read finalization metrics")
}

async fn read_feedback_retention_metrics(
    pool: &MySqlPool,
) -> anyhow::Result<FeedbackRetentionMetrics> {
    sqlx::query_as(
        "SELECT
            CAST(COALESCE(SUM(feedback.status = 'active'), 0) AS SIGNED) AS active_count,
            CAST(COALESCE(SUM(feedback.status = 'expired'), 0) AS SIGNED) AS expired_count,
            CAST(COALESCE(SUM(
                feedback.status = 'active'
                AND JSON_EXTRACT(
                    policy.canonical_manifest_json, '$.retentionMaximumDays') IS NOT NULL
                AND TIMESTAMPADD(
                    DAY,
                    CAST(JSON_UNQUOTE(JSON_EXTRACT(
                        policy.canonical_manifest_json, '$.retentionMaximumDays')) AS SIGNED),
                    feedback.created_at
                ) <= UTC_TIMESTAMP(6)
            ), 0) AS SIGNED) AS overdue_active_count
         FROM playtest_feedback AS feedback
         INNER JOIN playtest_consent_event AS event
            ON event.id = feedback.consent_event_id
           AND event.user_id = feedback.user_id
           AND BINARY event.scope = BINARY feedback.scope
         INNER JOIN playtest_consent_policy_version AS policy
            ON policy.id = event.policy_version_id",
    )
    .fetch_one(pool)
    .await
    .context("failed to read playtest feedback retention metrics")
}

fn validate_nonnegative_metrics(
    migrations: &MigrationMetrics,
    offline: &OfflineProgressMetrics,
    seasons: &SeasonMetrics,
    finalizations: &FinalizationMetrics,
    feedback_retention: &FeedbackRetentionMetrics,
) -> anyhow::Result<()> {
    let values = [
        migrations.latest_successful_version,
        migrations.failed_count,
        offline.enabled_run_count,
        offline.pending_run_count,
        offline.pending_day_count,
        offline.oldest_accrual_age_seconds,
        offline.paused_run_count,
        offline.active_worker_lease_count,
        offline.expired_worker_lease_count,
        offline.committed_last_hour_count,
        offline.failed_last_hour_count,
        seasons.draft_count,
        seasons.registration_open_count,
        seasons.active_count,
        seasons.locked_count,
        seasons.finalized_count,
        seasons.archived_count,
        seasons.ranked_run_count,
        finalizations.completed_count,
        finalizations.failed_count,
        finalizations.failed_last_hour_count,
        feedback_retention.active_count,
        feedback_retention.expired_count,
        feedback_retention.overdue_active_count,
    ];
    ensure!(
        values.into_iter().all(|value| value >= 0),
        "negative operations metric"
    );
    Ok(())
}

fn classify_alerts(
    migrations: &MigrationMetrics,
    offline: &OfflineProgressMetrics,
    finalizations: &FinalizationMetrics,
    feedback_retention: &FeedbackRetentionMetrics,
) -> Vec<OperationsAlertCode> {
    let mut alerts = Vec::new();
    if migrations.failed_count > 0 {
        alerts.push(OperationsAlertCode::MigrationFailure);
    }
    if offline.paused_run_count > 0 {
        alerts.push(OperationsAlertCode::OfflineProgressPaused);
    }
    if offline.expired_worker_lease_count > 0 {
        alerts.push(OperationsAlertCode::ExpiredWorkerLease);
    }
    if finalizations.failed_last_hour_count > 0 {
        alerts.push(OperationsAlertCode::RecentFinalizationFailure);
    }
    if feedback_retention.overdue_active_count > 0 {
        alerts.push(OperationsAlertCode::ExpiredFeedbackRetention);
    }
    alerts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn given_migrations(failed_count: i64) -> MigrationMetrics {
        MigrationMetrics {
            latest_successful_version: 58,
            failed_count,
        }
    }

    fn given_offline(
        paused_run_count: i64,
        expired_worker_lease_count: i64,
    ) -> OfflineProgressMetrics {
        OfflineProgressMetrics {
            enabled_run_count: 0,
            pending_run_count: 0,
            pending_day_count: 0,
            oldest_accrual_age_seconds: 0,
            paused_run_count,
            active_worker_lease_count: 0,
            expired_worker_lease_count,
            committed_last_hour_count: 0,
            failed_last_hour_count: 0,
        }
    }

    fn given_finalizations(failed_last_hour_count: i64) -> FinalizationMetrics {
        FinalizationMetrics {
            completed_count: 0,
            failed_count: failed_last_hour_count,
            failed_last_hour_count,
        }
    }

    fn given_feedback_retention(overdue_active_count: i64) -> FeedbackRetentionMetrics {
        FeedbackRetentionMetrics {
            active_count: overdue_active_count,
            expired_count: 0,
            overdue_active_count,
        }
    }

    mod context_이상이_없는_경우 {
        use super::*;

        #[test]
        fn given_모든_지표가_정상일때_when_경고를_분류하면_then_빈_목록이다() {
            let migrations = given_migrations(0);
            let offline = given_offline(0, 0);
            let finalizations = given_finalizations(0);
            let feedback_retention = given_feedback_retention(0);

            let alerts =
                classify_alerts(&migrations, &offline, &finalizations, &feedback_retention);

            assert!(alerts.is_empty());
        }
    }

    mod context_즉시_확인이_필요한_경우 {
        use super::*;

        #[test]
        fn given_실패와_정지가_있을때_when_경고를_분류하면_then_안정된_코드를_모두_반환한다() {
            let migrations = given_migrations(1);
            let offline = given_offline(2, 1);
            let finalizations = given_finalizations(1);
            let feedback_retention = given_feedback_retention(1);

            let alerts =
                classify_alerts(&migrations, &offline, &finalizations, &feedback_retention);

            assert_eq!(
                alerts,
                vec![
                    OperationsAlertCode::MigrationFailure,
                    OperationsAlertCode::OfflineProgressPaused,
                    OperationsAlertCode::ExpiredWorkerLease,
                    OperationsAlertCode::RecentFinalizationFailure,
                    OperationsAlertCode::ExpiredFeedbackRetention,
                ]
            );
        }
    }
}
