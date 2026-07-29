use std::sync::Arc;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use sqlx::MySqlPool;
use time::PrimitiveDateTime;

use super::types::{
    OfflineAttemptEvent, OfflineAttemptEventKind, OfflinePolicyState, OfflineProgressFailure,
    OfflineProgressSettingStatus, OfflineProgressState, OfflineProgressStore,
    OfflineProgressUpdateResult, OfflineWorkClaim, OnlinePresenceRegistration, ProgressHolderKind,
    ProgressLeaseAcquireResult, ProgressLeaseGuard, ProgressLeaseState, ProgressStepContext,
};
use crate::finance::ResourceId;
use crate::offline::{OfflineAccrualInput, OfflineRules};

#[derive(Clone)]
pub struct MySqlOfflineProgressStore {
    pool: MySqlPool,
    rules: Arc<dyn OfflineRules>,
}

pub fn create_mysql_offline_progress_store(
    pool: MySqlPool,
    rules: Arc<dyn OfflineRules>,
) -> MySqlOfflineProgressStore {
    MySqlOfflineProgressStore { pool, rules }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PolicyRow {
    id: u64,
    canonical_sha256: String,
    cadence_seconds: u32,
    absence_window_cap_days: u32,
    lease_seconds: u16,
    presence_ttl_seconds: u16,
    heartbeat_seconds: u16,
}

#[derive(Debug, sqlx::FromRow)]
struct StatusRow {
    run_revision: u32,
    policy_id: Option<u64>,
    policy_sha256: Option<String>,
    policy_engine_version: Option<String>,
    cadence_seconds: Option<u32>,
    absence_window_cap_days: Option<u32>,
    max_worker_batch_days: Option<u16>,
    lease_seconds: Option<u16>,
    presence_ttl_seconds: Option<u16>,
    heartbeat_seconds: Option<u16>,
    online_intent_ttl_seconds: Option<u16>,
    enabled: bool,
    setting_status: String,
    absence_started_at: Option<String>,
    accrued_through: Option<String>,
    accrual_limit_at: Option<String>,
    window_accrued_days: u64,
    pending_days: u64,
    processed_days: u64,
    cancelled_pending_days: u64,
    revision: u64,
    last_error_code: Option<String>,
    online: bool,
    lease_holder_kind: Option<String>,
    lease_generation: Option<u64>,
    lease_expires_at: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct OwnedRunPolicyRow {
    save_id: u64,
    run_revision: u32,
    has_character: bool,
    policy_id: Option<u64>,
    policy_sha256: Option<String>,
    cadence_seconds: Option<u32>,
    absence_window_cap_days: Option<u32>,
    lease_seconds: Option<u16>,
    presence_ttl_seconds: Option<u16>,
    heartbeat_seconds: Option<u16>,
}

impl OwnedRunPolicyRow {
    fn policy(&self) -> Option<PolicyRow> {
        Some(PolicyRow {
            id: self.policy_id?,
            canonical_sha256: self.policy_sha256.clone()?,
            cadence_seconds: self.cadence_seconds?,
            absence_window_cap_days: self.absence_window_cap_days?,
            lease_seconds: self.lease_seconds?,
            presence_ttl_seconds: self.presence_ttl_seconds?,
            heartbeat_seconds: self.heartbeat_seconds?,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct SettingControlRow {
    enabled: bool,
    pending_days: u32,
    cancelled_pending_days: u64,
    revision: u64,
}

#[derive(Debug, sqlx::FromRow)]
struct AccrualRow {
    accrued_through: PrimitiveDateTime,
    accrual_limit_at: PrimitiveDateTime,
    window_accrued_days: u32,
    pending_days: u32,
    cadence_seconds: u32,
    absence_window_cap_days: u32,
    target_game_day: Option<u32>,
    game_day: u32,
}

#[derive(Debug, sqlx::FromRow)]
struct LeaseRow {
    run_revision: u32,
    holder_kind: String,
    holder_token_sha256: String,
    generation: u64,
    expires_at: PrimitiveDateTime,
}

#[derive(Debug, sqlx::FromRow)]
struct WorkCandidateRow {
    user_id: u64,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
    max_worker_batch_days: u16,
    lease_seconds: u16,
    retry_no: u16,
}

#[async_trait]
impl OfflineProgressStore for MySqlOfflineProgressStore {
    async fn status(&self, user_id: u64) -> Result<OfflineProgressState> {
        read_status(&self.pool, user_id).await
    }

    async fn set_enabled(
        &self,
        user_id: u64,
        expected_revision: u64,
        enabled: bool,
    ) -> Result<OfflineProgressUpdateResult> {
        let mut tx = self.pool.begin().await?;
        let Some(owned) = read_owned_run_policy(&mut tx, user_id).await? else {
            tx.commit().await?;
            return Ok(OfflineProgressUpdateResult::Rejected(
                OfflineProgressFailure::CharacterRequired,
            ));
        };
        if !owned.has_character {
            tx.commit().await?;
            return Ok(OfflineProgressUpdateResult::Rejected(
                OfflineProgressFailure::CharacterRequired,
            ));
        }
        let Some(policy) = owned.policy() else {
            tx.commit().await?;
            return Ok(OfflineProgressUpdateResult::Rejected(
                OfflineProgressFailure::PolicyUnavailable,
            ));
        };
        let current = read_setting_control(&mut tx, owned.save_id, owned.run_revision).await?;
        let current_revision = current.as_ref().map_or(0, |row| row.revision);
        if current_revision != expected_revision {
            tx.commit().await?;
            return Ok(OfflineProgressUpdateResult::Rejected(
                OfflineProgressFailure::RevisionConflict,
            ));
        }
        let next_revision = current_revision
            .checked_add(1)
            .context("offline setting revision overflowed")?;
        let db_now = db_now(&mut tx).await?;
        delete_expired_presences(&mut tx, owned.save_id, db_now).await?;
        let online =
            has_online_presence(&mut tx, owned.save_id, owned.run_revision, db_now).await?;

        match current {
            None => {
                insert_setting(
                    &mut tx,
                    &owned,
                    &policy,
                    enabled,
                    online,
                    db_now,
                    next_revision,
                )
                .await?;
            }
            Some(current) if current.enabled == enabled => {
                sqlx::query(
                    "UPDATE offline_progress_setting
                     SET revision = ?
                     WHERE save_id = ? AND run_revision = ?",
                )
                .bind(next_revision)
                .bind(owned.save_id)
                .bind(owned.run_revision)
                .execute(&mut *tx)
                .await?;
            }
            Some(_) if enabled => {
                let limit_seconds = window_limit_seconds(&policy)?;
                sqlx::query(
                    "UPDATE offline_progress_setting
                     SET enabled = TRUE, status = 'active', last_error_code = NULL,
                         absence_started_at = IF(?, NULL, ?),
                         accrued_through = IF(?, NULL, ?),
                         accrual_limit_at = IF(?, NULL, TIMESTAMPADD(SECOND, ?, ?)),
                         window_accrued_days = 0, online_intent_at = NULL, revision = ?
                     WHERE save_id = ? AND run_revision = ?",
                )
                .bind(online)
                .bind(db_now)
                .bind(online)
                .bind(db_now)
                .bind(online)
                .bind(limit_seconds)
                .bind(db_now)
                .bind(next_revision)
                .bind(owned.save_id)
                .bind(owned.run_revision)
                .execute(&mut *tx)
                .await?;
            }
            Some(current) => {
                let cancelled = current
                    .cancelled_pending_days
                    .checked_add(u64::from(current.pending_days))
                    .context("cancelled offline day count overflowed")?;
                sqlx::query(
                    "UPDATE offline_progress_setting
                     SET enabled = FALSE, status = 'active', absence_started_at = NULL,
                         accrued_through = NULL, accrual_limit_at = NULL,
                         window_accrued_days = 0, pending_days = 0,
                         cancelled_pending_days = ?, online_intent_at = NULL,
                         last_error_code = NULL, revision = ?
                     WHERE save_id = ? AND run_revision = ?",
                )
                .bind(cancelled)
                .bind(next_revision)
                .bind(owned.save_id)
                .bind(owned.run_revision)
                .execute(&mut *tx)
                .await?;
            }
        }
        tx.commit().await?;

        Ok(OfflineProgressUpdateResult::Updated(Box::new(
            read_status(&self.pool, user_id).await?,
        )))
    }

    async fn register_online_presence(
        &self,
        user_id: u64,
        connection_token_sha256: &str,
    ) -> Result<Option<OnlinePresenceRegistration>> {
        upsert_online_presence(
            &self.pool,
            self.rules.as_ref(),
            user_id,
            connection_token_sha256,
        )
        .await
    }

    async fn heartbeat_online_presence(
        &self,
        user_id: u64,
        connection_token_sha256: &str,
    ) -> Result<()> {
        let _ = upsert_online_presence(
            &self.pool,
            self.rules.as_ref(),
            user_id,
            connection_token_sha256,
        )
        .await?;
        Ok(())
    }

    async fn close_online_presence(&self, connection_token_sha256: &str) -> Result<()> {
        close_online_presence(&self.pool, connection_token_sha256).await
    }

    async fn acquire_online_lease(
        &self,
        user_id: u64,
        holder_token_sha256: &str,
    ) -> Result<ProgressLeaseAcquireResult> {
        let mut tx = self.pool.begin().await?;
        let Some(owned) = read_owned_run_policy(&mut tx, user_id).await? else {
            tx.commit().await?;
            return Ok(ProgressLeaseAcquireResult::Busy);
        };
        sqlx::query(
            "UPDATE offline_progress_setting
             SET online_intent_at = CURRENT_TIMESTAMP(6)
             WHERE save_id = ? AND run_revision = ?",
        )
        .bind(owned.save_id)
        .bind(owned.run_revision)
        .execute(&mut *tx)
        .await?;
        let lease_seconds = match owned.policy() {
            Some(policy) => policy.lease_seconds,
            None => active_policy(&mut tx).await?.lease_seconds,
        };
        let acquired = acquire_lease_in_tx(
            &mut tx,
            owned.save_id,
            owned.run_revision,
            ProgressHolderKind::Online,
            holder_token_sha256,
            lease_seconds,
        )
        .await?;
        tx.commit().await?;
        Ok(acquired)
    }

    async fn release_lease(&self, lease: &ProgressLeaseGuard) -> Result<()> {
        sqlx::query(
            "DELETE FROM progress_lease
             WHERE save_id = ? AND run_revision = ? AND holder_kind = ?
               AND holder_token_sha256 = ? AND generation = ?",
        )
        .bind(lease.save_id)
        .bind(lease.run_revision)
        .bind(holder_kind_db(lease.holder_kind))
        .bind(&lease.holder_token_sha256)
        .bind(lease.generation)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn claim_offline_work(
        &self,
        holder_token_sha256: &str,
        engine_version: &str,
    ) -> Result<Option<OfflineWorkClaim>> {
        let mut tx = self.pool.begin().await?;
        let db_now = db_now(&mut tx).await?;
        let candidate_key = sqlx::query_as::<_, (u64, u32)>(
            "SELECT setting.save_id, setting.run_revision
             FROM offline_progress_setting AS setting
             INNER JOIN save ON save.id = setting.save_id
                AND save.run_revision = setting.run_revision
             INNER JOIN run_manifest AS manifest ON manifest.save_id = setting.save_id
                AND manifest.run_revision = setting.run_revision
             INNER JOIN offline_policy_version AS policy
                ON policy.id = setting.offline_policy_version_id
               AND BINARY policy.canonical_sha256 = BINARY setting.offline_policy_sha256
             WHERE setting.enabled = TRUE AND setting.status = 'active'
               AND BINARY manifest.engine_version = BINARY ?
               AND BINARY policy.engine_version = BINARY ?
               AND (manifest.target_game_day IS NULL OR save.game_day < manifest.target_game_day)
               AND NOT EXISTS (
                   SELECT 1 FROM offline_online_presence AS presence
                   WHERE presence.save_id = setting.save_id
                     AND presence.run_revision = setting.run_revision
                     AND presence.expires_at > ?
               )
               AND (
                   setting.online_intent_at IS NULL
                   OR setting.online_intent_at <= TIMESTAMPADD(
                       SECOND, -policy.online_intent_ttl_seconds, ?)
               )
               AND (
                   setting.pending_days > 0
                   OR setting.absence_started_at IS NULL
                   OR TIMESTAMPADD(SECOND, policy.cadence_seconds, setting.accrued_through)
                        <= LEAST(?, setting.accrual_limit_at)
               )
               AND NOT EXISTS (
                   SELECT 1 FROM progress_lease AS active_lease
                   WHERE active_lease.save_id = setting.save_id
                     AND active_lease.expires_at > ?
               )
             ORDER BY (setting.pending_days > 0) DESC,
                      setting.accrued_through, setting.save_id
             LIMIT 1",
        )
        .bind(engine_version)
        .bind(engine_version)
        .bind(db_now)
        .bind(db_now)
        .bind(db_now)
        .bind(db_now)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((save_id, run_revision)) = candidate_key else {
            tx.commit().await?;
            return Ok(None);
        };
        let locked_save: Option<u64> = sqlx::query_scalar(
            "SELECT id FROM save WHERE id = ? AND run_revision = ?
             FOR UPDATE SKIP LOCKED",
        )
        .bind(save_id)
        .bind(run_revision)
        .fetch_optional(&mut *tx)
        .await?;
        if locked_save.is_none() {
            tx.commit().await?;
            return Ok(None);
        }
        let candidate = sqlx::query_as::<_, WorkCandidateRow>(
            "SELECT save.user_id, setting.save_id, setting.run_revision, save.game_day,
                    policy.max_worker_batch_days, policy.lease_seconds,
                    COALESCE((
                        SELECT MAX(attempt.retry_no) + 1
                        FROM offline_progress_attempt AS attempt
                        WHERE attempt.save_id = setting.save_id
                          AND attempt.run_revision = setting.run_revision
                          AND attempt.game_day = save.game_day + 1
                          AND attempt.event_kind = 'started'
                    ), 0) AS retry_no
             FROM offline_progress_setting AS setting
             INNER JOIN save ON save.id = setting.save_id
                AND save.run_revision = setting.run_revision
             INNER JOIN run_manifest AS manifest ON manifest.save_id = setting.save_id
                AND manifest.run_revision = setting.run_revision
             INNER JOIN offline_policy_version AS policy
                ON policy.id = setting.offline_policy_version_id
               AND BINARY policy.canonical_sha256 = BINARY setting.offline_policy_sha256
             WHERE setting.enabled = TRUE AND setting.status = 'active'
               AND BINARY manifest.engine_version = BINARY ?
               AND BINARY policy.engine_version = BINARY ?
               AND (manifest.target_game_day IS NULL OR save.game_day < manifest.target_game_day)
               AND NOT EXISTS (
                   SELECT 1 FROM offline_online_presence AS presence
                   WHERE presence.save_id = setting.save_id
                     AND presence.run_revision = setting.run_revision
                     AND presence.expires_at > ?
               )
               AND (
                   setting.online_intent_at IS NULL
                   OR setting.online_intent_at <= TIMESTAMPADD(
                       SECOND, -policy.online_intent_ttl_seconds, ?)
               )
               AND (
                   setting.pending_days > 0
                   OR setting.absence_started_at IS NULL
                   OR TIMESTAMPADD(SECOND, policy.cadence_seconds, setting.accrued_through)
                        <= LEAST(?, setting.accrual_limit_at)
               )
               AND NOT EXISTS (
                   SELECT 1 FROM progress_lease AS active_lease
                   WHERE active_lease.save_id = setting.save_id
                     AND active_lease.expires_at > ?
               )
               AND setting.save_id = ? AND setting.run_revision = ?
             FOR UPDATE",
        )
        .bind(engine_version)
        .bind(engine_version)
        .bind(db_now)
        .bind(db_now)
        .bind(db_now)
        .bind(db_now)
        .bind(save_id)
        .bind(run_revision)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(candidate) = candidate else {
            tx.commit().await?;
            return Ok(None);
        };

        if setting_window_is_closed(&mut tx, candidate.save_id, candidate.run_revision).await? {
            open_absence_window(&mut tx, candidate.save_id, candidate.run_revision, db_now).await?;
        }
        accrue_open_window_in_tx(
            &mut tx,
            self.rules.as_ref(),
            candidate.save_id,
            candidate.run_revision,
            db_now,
        )
        .await?;
        let pending_days: u32 = sqlx::query_scalar(
            "SELECT pending_days FROM offline_progress_setting
             WHERE save_id = ? AND run_revision = ? FOR UPDATE",
        )
        .bind(candidate.save_id)
        .bind(candidate.run_revision)
        .fetch_one(&mut *tx)
        .await?;
        if pending_days == 0 {
            tx.commit().await?;
            return Ok(None);
        }
        let lease = acquire_lease_in_tx(
            &mut tx,
            candidate.save_id,
            candidate.run_revision,
            ProgressHolderKind::Worker,
            holder_token_sha256,
            candidate.lease_seconds,
        )
        .await?;
        let ProgressLeaseAcquireResult::Acquired(lease) = lease else {
            tx.commit().await?;
            return Ok(None);
        };
        let next_game_day = candidate
            .game_day
            .checked_add(1)
            .context("offline target game day overflowed")?;
        tx.commit().await?;

        Ok(Some(OfflineWorkClaim {
            user_id: candidate.user_id,
            save_id: candidate.save_id,
            run_revision: candidate.run_revision,
            next_game_day,
            max_batch_days: candidate
                .max_worker_batch_days
                .min(u16::try_from(pending_days).unwrap_or(u16::MAX)),
            retry_no: candidate.retry_no,
            lease,
        }))
    }

    async fn record_attempt(&self, event: OfflineAttemptEvent<'_>) -> Result<()> {
        sqlx::query(
            "INSERT INTO offline_progress_attempt
                 (attempt_key, event_kind, save_id, run_revision, game_day,
                  lease_generation, retry_no, engine_version, error_code)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(event.attempt_key)
        .bind(attempt_event_db(event.event_kind))
        .bind(event.save_id)
        .bind(event.run_revision)
        .bind(event.game_day)
        .bind(event.lease_generation)
        .bind(event.retry_no)
        .bind(event.engine_version)
        .bind(event.error_code)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn pause_after_permanent_failure(
        &self,
        lease: &ProgressLeaseGuard,
        error_code: &str,
    ) -> Result<bool> {
        if lease.holder_kind != ProgressHolderKind::Worker {
            bail!("only a worker lease can pause offline progress");
        }
        if error_code.is_empty()
            || error_code.len() > 64
            || !error_code.bytes().all(|byte| byte.is_ascii_alphanumeric())
        {
            bail!("offline progress error code is invalid");
        }
        let mut tx = self.pool.begin().await?;
        let locked_save: Option<u64> =
            sqlx::query_scalar("SELECT id FROM save WHERE id = ? AND run_revision = ? FOR UPDATE")
                .bind(lease.save_id)
                .bind(lease.run_revision)
                .fetch_optional(&mut *tx)
                .await?;
        if locked_save.is_none()
            || !authorize_progress_step_in_tx(
                &mut tx,
                &ProgressStepContext {
                    lease: lease.clone(),
                    offline_attempt: None,
                },
            )
            .await?
        {
            tx.commit().await?;
            return Ok(false);
        }
        let updated = sqlx::query(
            "UPDATE offline_progress_setting
             SET status = 'pausedBySystem', absence_started_at = NULL,
                 accrued_through = NULL, accrual_limit_at = NULL,
                 online_intent_at = NULL, last_error_code = ?, revision = revision + 1
             WHERE save_id = ? AND run_revision = ? AND enabled = TRUE
               AND status = 'active'",
        )
        .bind(error_code)
        .bind(lease.save_id)
        .bind(lease.run_revision)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(updated.rows_affected() == 1)
    }
}

pub(super) async fn authorize_progress_step_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    progress: &ProgressStepContext,
) -> Result<bool> {
    let db_now = db_now(tx).await?;
    if progress.lease.holder_kind == ProgressHolderKind::Worker {
        let allowed: Option<bool> = sqlx::query_scalar(
            "SELECT setting.pending_days > 0
                AND setting.enabled = TRUE
                AND setting.status = 'active'
                AND NOT EXISTS (
                    SELECT 1 FROM offline_online_presence AS presence
                    WHERE presence.save_id = setting.save_id
                      AND presence.run_revision = setting.run_revision
                      AND presence.expires_at > ?
                )
                AND (
                    setting.online_intent_at IS NULL
                    OR setting.online_intent_at <= TIMESTAMPADD(
                        SECOND, -policy.online_intent_ttl_seconds, ?)
                )
             FROM offline_progress_setting AS setting
             INNER JOIN offline_policy_version AS policy
                ON policy.id = setting.offline_policy_version_id
             WHERE setting.save_id = ? AND setting.run_revision = ?
             FOR UPDATE",
        )
        .bind(db_now)
        .bind(db_now)
        .bind(progress.lease.save_id)
        .bind(progress.lease.run_revision)
        .fetch_optional(&mut **tx)
        .await?;
        if allowed != Some(true) {
            return Ok(false);
        }
    } else {
        sqlx::query(
            "SELECT revision FROM offline_progress_setting
             WHERE save_id = ? AND run_revision = ? FOR UPDATE",
        )
        .bind(progress.lease.save_id)
        .bind(progress.lease.run_revision)
        .fetch_optional(&mut **tx)
        .await?;
    }

    let lease = sqlx::query_as::<_, LeaseRow>(
        "SELECT run_revision, holder_kind, holder_token_sha256, generation, expires_at
         FROM progress_lease WHERE save_id = ? FOR UPDATE",
    )
    .bind(progress.lease.save_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(lease.is_some_and(|lease| {
        lease.run_revision == progress.lease.run_revision
            && lease.holder_kind == holder_kind_db(progress.lease.holder_kind)
            && lease.holder_token_sha256 == progress.lease.holder_token_sha256
            && lease.generation == progress.lease.generation
            && lease.expires_at > db_now
    }))
}

pub(super) async fn complete_progress_step_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    progress: &ProgressStepContext,
    committed_game_day: u32,
) -> Result<()> {
    let lease_seconds: u16 = sqlx::query_scalar(
        "SELECT COALESCE(run_policy.lease_seconds, active_policy.lease_seconds)
         FROM run_manifest AS manifest
         LEFT JOIN offline_policy_version AS run_policy
            ON run_policy.id = manifest.offline_policy_version_id
         INNER JOIN offline_policy_assignment AS assignment
            ON assignment.assignment_key = 'newSandboxRun'
         INNER JOIN offline_policy_version AS active_policy
            ON active_policy.id = assignment.offline_policy_version_id
         WHERE manifest.save_id = ? AND manifest.run_revision = ?",
    )
    .bind(progress.lease.save_id)
    .bind(progress.lease.run_revision)
    .fetch_one(&mut **tx)
    .await?;
    let renewed = sqlx::query(
        "UPDATE progress_lease
         SET renewed_at = CURRENT_TIMESTAMP(6),
             expires_at = TIMESTAMPADD(SECOND, ?, CURRENT_TIMESTAMP(6))
         WHERE save_id = ? AND run_revision = ? AND holder_kind = ?
           AND holder_token_sha256 = ? AND generation = ?
           AND expires_at > CURRENT_TIMESTAMP(6)",
    )
    .bind(lease_seconds)
    .bind(progress.lease.save_id)
    .bind(progress.lease.run_revision)
    .bind(holder_kind_db(progress.lease.holder_kind))
    .bind(&progress.lease.holder_token_sha256)
    .bind(progress.lease.generation)
    .execute(&mut **tx)
    .await?;
    if renewed.rows_affected() != 1 {
        bail!("progress lease was lost before daily commit");
    }

    if progress.lease.holder_kind == ProgressHolderKind::Worker {
        let updated = sqlx::query(
            "UPDATE offline_progress_setting
             SET pending_days = pending_days - 1, processed_days = processed_days + 1
             WHERE save_id = ? AND run_revision = ? AND enabled = TRUE
               AND status = 'active' AND pending_days > 0",
        )
        .bind(progress.lease.save_id)
        .bind(progress.lease.run_revision)
        .execute(&mut **tx)
        .await?;
        if updated.rows_affected() != 1 {
            bail!("offline pending day was lost before daily commit");
        }
        let attempt = progress
            .offline_attempt
            .as_ref()
            .context("worker progress has no attempt identity")?;
        sqlx::query(
            "INSERT INTO offline_progress_attempt
                 (attempt_key, event_kind, save_id, run_revision, game_day,
                  lease_generation, retry_no, engine_version, error_code)
             VALUES (?, 'committed', ?, ?, ?, ?, ?, ?, NULL)",
        )
        .bind(&attempt.attempt_key)
        .bind(progress.lease.save_id)
        .bind(progress.lease.run_revision)
        .bind(committed_game_day)
        .bind(progress.lease.generation)
        .bind(attempt.retry_no)
        .bind(&attempt.engine_version)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn read_status(pool: &MySqlPool, user_id: u64) -> Result<OfflineProgressState> {
    let row = sqlx::query_as::<_, StatusRow>(
        "SELECT save.run_revision,
                policy.id AS policy_id, policy.canonical_sha256 AS policy_sha256,
                policy.engine_version AS policy_engine_version,
                policy.cadence_seconds, policy.absence_window_cap_days,
                policy.max_worker_batch_days, policy.lease_seconds,
                policy.presence_ttl_seconds, policy.heartbeat_seconds,
                policy.online_intent_ttl_seconds,
                COALESCE(setting.enabled, FALSE) AS enabled,
                COALESCE(setting.status, 'active') AS setting_status,
                DATE_FORMAT(setting.absence_started_at, '%Y-%m-%dT%H:%i:%s.%fZ')
                    AS absence_started_at,
                DATE_FORMAT(setting.accrued_through, '%Y-%m-%dT%H:%i:%s.%fZ')
                    AS accrued_through,
                DATE_FORMAT(setting.accrual_limit_at, '%Y-%m-%dT%H:%i:%s.%fZ')
                    AS accrual_limit_at,
                COALESCE(setting.window_accrued_days, 0) AS window_accrued_days,
                COALESCE(setting.pending_days, 0) AS pending_days,
                COALESCE(setting.processed_days, 0) AS processed_days,
                COALESCE(setting.cancelled_pending_days, 0) AS cancelled_pending_days,
                COALESCE(setting.revision, 0) AS revision, setting.last_error_code,
                EXISTS(
                    SELECT 1 FROM offline_online_presence AS presence
                    WHERE presence.save_id = save.id
                      AND presence.run_revision = save.run_revision
                      AND presence.expires_at > CURRENT_TIMESTAMP(6)
                ) AS online,
                lease.holder_kind AS lease_holder_kind,
                lease.generation AS lease_generation,
                DATE_FORMAT(lease.expires_at, '%Y-%m-%dT%H:%i:%s.%fZ') AS lease_expires_at
         FROM save
         LEFT JOIN run_manifest AS manifest ON manifest.save_id = save.id
            AND manifest.run_revision = save.run_revision
         LEFT JOIN offline_policy_version AS policy
            ON policy.id = manifest.offline_policy_version_id
           AND BINARY policy.canonical_sha256 = BINARY manifest.offline_policy_sha256
         LEFT JOIN offline_progress_setting AS setting ON setting.save_id = save.id
            AND setting.run_revision = save.run_revision
         LEFT JOIN progress_lease AS lease ON lease.save_id = save.id
            AND lease.run_revision = save.run_revision
            AND lease.expires_at > CURRENT_TIMESTAMP(6)
         WHERE save.user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(OfflineProgressState {
            run_revision: 0,
            policy: None,
            enabled: false,
            setting_status: OfflineProgressSettingStatus::Active,
            absence_started_at: None,
            accrued_through: None,
            accrual_limit_at: None,
            window_accrued_days: 0,
            pending_days: 0,
            processed_days: 0,
            cancelled_pending_days: 0,
            revision: 0,
            last_error_code: None,
            online: false,
            lease: None,
        });
    };
    to_status(row)
}

fn to_status(row: StatusRow) -> Result<OfflineProgressState> {
    let policy = match row.policy_id {
        Some(id) => Some(OfflinePolicyState {
            id: ResourceId::from_u64(id),
            canonical_sha256: row.policy_sha256.context("offline policy SHA is missing")?,
            engine_version: row
                .policy_engine_version
                .context("offline policy engine version is missing")?,
            cadence_seconds: row.cadence_seconds.context("offline cadence is missing")?,
            absence_window_cap_days: row
                .absence_window_cap_days
                .context("offline window cap is missing")?,
            max_worker_batch_days: row
                .max_worker_batch_days
                .context("offline worker batch is missing")?,
            lease_seconds: row
                .lease_seconds
                .context("offline lease duration is missing")?,
            presence_ttl_seconds: row
                .presence_ttl_seconds
                .context("offline presence TTL is missing")?,
            heartbeat_seconds: row
                .heartbeat_seconds
                .context("offline heartbeat is missing")?,
            online_intent_ttl_seconds: row
                .online_intent_ttl_seconds
                .context("offline intent TTL is missing")?,
        }),
        None => None,
    };
    let setting_status = match row.setting_status.as_str() {
        "active" => OfflineProgressSettingStatus::Active,
        "pausedBySystem" => OfflineProgressSettingStatus::PausedBySystem,
        _ => bail!("stored offline setting status is invalid"),
    };
    let lease = match row.lease_holder_kind {
        Some(kind) => Some(ProgressLeaseState {
            holder_kind: parse_holder_kind(&kind)?,
            generation: row
                .lease_generation
                .context("progress lease generation is missing")?,
            expires_at: row
                .lease_expires_at
                .context("progress lease expiry is missing")?,
        }),
        None => None,
    };
    Ok(OfflineProgressState {
        run_revision: row.run_revision,
        policy,
        enabled: row.enabled,
        setting_status,
        absence_started_at: row.absence_started_at,
        accrued_through: row.accrued_through,
        accrual_limit_at: row.accrual_limit_at,
        window_accrued_days: u32::try_from(row.window_accrued_days)
            .context("offline window accrued day count exceeds u32")?,
        pending_days: u32::try_from(row.pending_days)
            .context("offline pending day count exceeds u32")?,
        processed_days: row.processed_days,
        cancelled_pending_days: row.cancelled_pending_days,
        revision: row.revision,
        last_error_code: row.last_error_code,
        online: row.online,
        lease,
    })
}

async fn read_owned_run_policy(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    user_id: u64,
) -> Result<Option<OwnedRunPolicyRow>> {
    sqlx::query_as::<_, OwnedRunPolicyRow>(
        "SELECT save.id AS save_id, save.run_revision,
                EXISTS(SELECT 1 FROM `character` WHERE `character`.save_id = save.id)
                    AS has_character,
                policy.id AS policy_id, policy.canonical_sha256 AS policy_sha256,
                policy.cadence_seconds, policy.absence_window_cap_days,
                policy.lease_seconds, policy.presence_ttl_seconds,
                policy.heartbeat_seconds
         FROM save
         LEFT JOIN run_manifest AS manifest ON manifest.save_id = save.id
            AND manifest.run_revision = save.run_revision
         LEFT JOIN offline_policy_version AS policy
            ON policy.id = manifest.offline_policy_version_id
           AND BINARY policy.canonical_sha256 = BINARY manifest.offline_policy_sha256
         WHERE save.user_id = ?
         FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(Into::into)
}

async fn read_setting_control(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<Option<SettingControlRow>> {
    sqlx::query_as(
        "SELECT enabled, pending_days, cancelled_pending_days, revision
         FROM offline_progress_setting
         WHERE save_id = ? AND run_revision = ? FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await
    .map_err(Into::into)
}

async fn insert_setting(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    owned: &OwnedRunPolicyRow,
    policy: &PolicyRow,
    enabled: bool,
    online: bool,
    db_now: PrimitiveDateTime,
    revision: u64,
) -> Result<()> {
    let opens_window = enabled && !online;
    let limit_seconds = window_limit_seconds(policy)?;
    sqlx::query(
        "INSERT INTO offline_progress_setting
             (save_id, run_revision, offline_policy_version_id, offline_policy_sha256,
              enabled, status, absence_started_at, accrued_through, accrual_limit_at,
              revision)
         VALUES (?, ?, ?, ?, ?, 'active', IF(?, ?, NULL), IF(?, ?, NULL),
                 IF(?, TIMESTAMPADD(SECOND, ?, ?), NULL), ?)",
    )
    .bind(owned.save_id)
    .bind(owned.run_revision)
    .bind(policy.id)
    .bind(&policy.canonical_sha256)
    .bind(enabled)
    .bind(opens_window)
    .bind(db_now)
    .bind(opens_window)
    .bind(db_now)
    .bind(opens_window)
    .bind(limit_seconds)
    .bind(db_now)
    .bind(revision)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn upsert_online_presence(
    pool: &MySqlPool,
    rules: &dyn OfflineRules,
    user_id: u64,
    token_sha256: &str,
) -> Result<Option<OnlinePresenceRegistration>> {
    let mut tx = pool.begin().await?;
    let Some(owned) = read_owned_run_policy(&mut tx, user_id).await? else {
        tx.commit().await?;
        return Ok(None);
    };
    let policy = match owned.policy() {
        Some(policy) => policy,
        None => active_policy(&mut tx).await?,
    };
    let db_now = db_now(&mut tx).await?;
    delete_expired_presences(&mut tx, owned.save_id, db_now).await?;
    read_setting_control(&mut tx, owned.save_id, owned.run_revision).await?;
    accrue_open_window_in_tx(&mut tx, rules, owned.save_id, owned.run_revision, db_now).await?;
    sqlx::query(
        "INSERT INTO offline_online_presence
             (connection_token_sha256, save_id, run_revision, expires_at,
              opened_at, heartbeat_at)
         VALUES (?, ?, ?, TIMESTAMPADD(SECOND, ?, ?), ?, ?)
         ON DUPLICATE KEY UPDATE save_id = VALUES(save_id),
             run_revision = VALUES(run_revision), expires_at = VALUES(expires_at),
             heartbeat_at = VALUES(heartbeat_at)",
    )
    .bind(token_sha256)
    .bind(owned.save_id)
    .bind(owned.run_revision)
    .bind(policy.presence_ttl_seconds)
    .bind(db_now)
    .bind(db_now)
    .bind(db_now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE offline_progress_setting
         SET absence_started_at = NULL, accrued_through = NULL, accrual_limit_at = NULL
         WHERE save_id = ? AND run_revision = ? AND enabled = TRUE",
    )
    .bind(owned.save_id)
    .bind(owned.run_revision)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(OnlinePresenceRegistration {
        heartbeat_seconds: policy.heartbeat_seconds,
    }))
}

async fn close_online_presence(pool: &MySqlPool, token_sha256: &str) -> Result<()> {
    let mut tx = pool.begin().await?;
    let save_id: Option<u64> = sqlx::query_scalar(
        "SELECT save_id FROM offline_online_presence
         WHERE connection_token_sha256 = ?",
    )
    .bind(token_sha256)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(save_id) = save_id else {
        tx.commit().await?;
        return Ok(());
    };
    let current_run: Option<u32> =
        sqlx::query_scalar("SELECT run_revision FROM save WHERE id = ? FOR UPDATE")
            .bind(save_id)
            .fetch_optional(&mut *tx)
            .await?;
    let presence: Option<(u64, u32)> = sqlx::query_as(
        "SELECT save_id, run_revision FROM offline_online_presence
         WHERE connection_token_sha256 = ? FOR UPDATE",
    )
    .bind(token_sha256)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((save_id, run_revision)) = presence else {
        tx.commit().await?;
        return Ok(());
    };
    read_setting_control(&mut tx, save_id, run_revision).await?;
    sqlx::query("DELETE FROM offline_online_presence WHERE connection_token_sha256 = ?")
        .bind(token_sha256)
        .execute(&mut *tx)
        .await?;
    let db_now = db_now(&mut tx).await?;
    delete_expired_presences(&mut tx, save_id, db_now).await?;
    if !has_online_presence(&mut tx, save_id, run_revision, db_now).await?
        && current_run == Some(run_revision)
    {
        open_absence_window(&mut tx, save_id, run_revision, db_now).await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn accrue_open_window_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    rules: &dyn OfflineRules,
    save_id: u64,
    run_revision: u32,
    db_now: PrimitiveDateTime,
) -> Result<()> {
    let row = sqlx::query_as::<_, AccrualRow>(
        "SELECT setting.accrued_through, setting.accrual_limit_at,
                setting.window_accrued_days, setting.pending_days,
                policy.cadence_seconds, policy.absence_window_cap_days,
                manifest.target_game_day, save.game_day
         FROM offline_progress_setting AS setting
         INNER JOIN offline_policy_version AS policy
            ON policy.id = setting.offline_policy_version_id
         INNER JOIN run_manifest AS manifest ON manifest.save_id = setting.save_id
            AND manifest.run_revision = setting.run_revision
         INNER JOIN save ON save.id = setting.save_id
            AND save.run_revision = setting.run_revision
         WHERE setting.save_id = ? AND setting.run_revision = ?
           AND setting.enabled = TRUE AND setting.status = 'active'
           AND setting.accrued_through IS NOT NULL
         FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else {
        return Ok(());
    };
    let remaining_target_days = row.target_game_day.map(|target| {
        target
            .saturating_sub(row.game_day)
            .saturating_sub(row.pending_days)
    });
    let plan = rules.plan_accrual(OfflineAccrualInput {
        db_now_unix_micros: unix_micros(db_now)?,
        accrued_through_unix_micros: unix_micros(row.accrued_through)?,
        accrual_limit_unix_micros: unix_micros(row.accrual_limit_at)?,
        cadence_seconds: row.cadence_seconds,
        absence_window_cap_days: row.absence_window_cap_days,
        window_accrued_days: row.window_accrued_days,
        remaining_target_days,
    })?;
    if plan.days_to_accrue == 0 {
        return Ok(());
    }
    sqlx::query(
        "UPDATE offline_progress_setting
         SET pending_days = pending_days + ?,
             window_accrued_days = window_accrued_days + ?,
             accrued_through = TIMESTAMPADD(MICROSECOND, ?, accrued_through)
         WHERE save_id = ? AND run_revision = ?",
    )
    .bind(plan.days_to_accrue)
    .bind(plan.days_to_accrue)
    .bind(plan.accrued_through_advance_micros)
    .bind(save_id)
    .bind(run_revision)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn acquire_lease_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    save_id: u64,
    run_revision: u32,
    holder_kind: ProgressHolderKind,
    holder_token_sha256: &str,
    lease_seconds: u16,
) -> Result<ProgressLeaseAcquireResult> {
    let db_now = db_now(tx).await?;
    let current = sqlx::query_as::<_, LeaseRow>(
        "SELECT run_revision, holder_kind, holder_token_sha256, generation, expires_at
         FROM progress_lease WHERE save_id = ? FOR UPDATE",
    )
    .bind(save_id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(current) = &current
        && current.expires_at > db_now
        && (current.run_revision != run_revision
            || current.holder_kind != holder_kind_db(holder_kind)
            || current.holder_token_sha256 != holder_token_sha256)
    {
        return Ok(ProgressLeaseAcquireResult::Busy);
    }
    let generation = match current {
        Some(current)
            if current.expires_at > db_now
                && current.run_revision == run_revision
                && current.holder_kind == holder_kind_db(holder_kind)
                && current.holder_token_sha256 == holder_token_sha256 =>
        {
            current.generation
        }
        Some(current) => current
            .generation
            .checked_add(1)
            .context("progress lease generation overflowed")?,
        None => 1,
    };
    sqlx::query(
        "INSERT INTO progress_lease
             (save_id, run_revision, holder_kind, holder_token_sha256, generation,
              expires_at, acquired_at, renewed_at)
         VALUES (?, ?, ?, ?, ?, TIMESTAMPADD(SECOND, ?, ?), ?, ?)
         ON DUPLICATE KEY UPDATE run_revision = VALUES(run_revision),
             holder_kind = VALUES(holder_kind), holder_token_sha256 = VALUES(holder_token_sha256),
             generation = VALUES(generation), expires_at = VALUES(expires_at),
             acquired_at = VALUES(acquired_at), renewed_at = VALUES(renewed_at)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(holder_kind_db(holder_kind))
    .bind(holder_token_sha256)
    .bind(generation)
    .bind(lease_seconds)
    .bind(db_now)
    .bind(db_now)
    .bind(db_now)
    .execute(&mut **tx)
    .await?;
    Ok(ProgressLeaseAcquireResult::Acquired(ProgressLeaseGuard {
        save_id,
        run_revision,
        holder_kind,
        holder_token_sha256: holder_token_sha256.to_owned(),
        generation,
    }))
}

async fn active_policy(tx: &mut sqlx::Transaction<'_, sqlx::MySql>) -> Result<PolicyRow> {
    sqlx::query_as(
        "SELECT policy.id, policy.canonical_sha256,
                policy.cadence_seconds, policy.absence_window_cap_days,
                policy.lease_seconds, policy.presence_ttl_seconds,
                policy.heartbeat_seconds
         FROM offline_policy_assignment AS assignment
         INNER JOIN offline_policy_version AS policy
            ON policy.id = assignment.offline_policy_version_id
         WHERE assignment.assignment_key = 'newSandboxRun'",
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(Into::into)
}

async fn setting_window_is_closed(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    save_id: u64,
    run_revision: u32,
) -> Result<bool> {
    sqlx::query_scalar(
        "SELECT absence_started_at IS NULL FROM offline_progress_setting
         WHERE save_id = ? AND run_revision = ? FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_one(&mut **tx)
    .await
    .map_err(Into::into)
}

async fn open_absence_window(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    save_id: u64,
    run_revision: u32,
    db_now: PrimitiveDateTime,
) -> Result<()> {
    sqlx::query(
        "UPDATE offline_progress_setting AS setting
         INNER JOIN offline_policy_version AS policy
            ON policy.id = setting.offline_policy_version_id
         SET setting.absence_started_at = ?, setting.accrued_through = ?,
             setting.accrual_limit_at = TIMESTAMPADD(
                 SECOND, policy.cadence_seconds * policy.absence_window_cap_days, ?),
             setting.window_accrued_days = 0
         WHERE setting.save_id = ? AND setting.run_revision = ?
           AND setting.enabled = TRUE AND setting.status = 'active'
           AND setting.absence_started_at IS NULL",
    )
    .bind(db_now)
    .bind(db_now)
    .bind(db_now)
    .bind(save_id)
    .bind(run_revision)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn delete_expired_presences(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    save_id: u64,
    db_now: PrimitiveDateTime,
) -> Result<()> {
    sqlx::query("DELETE FROM offline_online_presence WHERE save_id = ? AND expires_at <= ?")
        .bind(save_id)
        .bind(db_now)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn has_online_presence(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    save_id: u64,
    run_revision: u32,
    db_now: PrimitiveDateTime,
) -> Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM offline_online_presence
             WHERE save_id = ? AND run_revision = ? AND expires_at > ?)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(db_now)
    .fetch_one(&mut **tx)
    .await
    .map_err(Into::into)
}

async fn db_now(tx: &mut sqlx::Transaction<'_, sqlx::MySql>) -> Result<PrimitiveDateTime> {
    sqlx::query_scalar("SELECT CURRENT_TIMESTAMP(6)")
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
}

fn window_limit_seconds(policy: &PolicyRow) -> Result<i64> {
    i64::from(policy.cadence_seconds)
        .checked_mul(i64::from(policy.absence_window_cap_days))
        .context("offline window duration overflowed")
}

fn unix_micros(value: PrimitiveDateTime) -> Result<i64> {
    let micros = value.assume_utc().unix_timestamp_nanos() / 1_000;
    i64::try_from(micros).context("offline timestamp is outside i64 microseconds")
}

fn holder_kind_db(kind: ProgressHolderKind) -> &'static str {
    match kind {
        ProgressHolderKind::Online => "online",
        ProgressHolderKind::Worker => "worker",
    }
}

fn parse_holder_kind(raw: &str) -> Result<ProgressHolderKind> {
    match raw {
        "online" => Ok(ProgressHolderKind::Online),
        "worker" => Ok(ProgressHolderKind::Worker),
        _ => bail!("stored progress holder kind is invalid"),
    }
}

fn attempt_event_db(kind: OfflineAttemptEventKind) -> &'static str {
    match kind {
        OfflineAttemptEventKind::Started => "started",
        OfflineAttemptEventKind::Failed => "failed",
    }
}
