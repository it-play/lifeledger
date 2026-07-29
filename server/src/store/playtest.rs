//! MySQL implementation of the M5-F consent and feedback boundary.

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use sqlx::{MySqlConnection, MySqlPool};

use crate::playtest::{
    AnalyticsCollection, ConsentCommand, ConsentDisplayStatus, ConsentPolicy, ConsentState,
    ConsentStoredStatus, ConsentUpdate, FeedbackCategory, FeedbackDeletion, FeedbackDraft,
    FeedbackItem, FeedbackSeverity, MAXIMUM_ACTIVE_FEEDBACK, PlaytestFailureCode,
    PlaytestFeedbackOverview, PlaytestRules, PlaytestStore, PlaytestStoreResult, StoredConsent,
};

const ACTIVE_POLICY_QUERY: &str =
    "SELECT policy.id, policy.scope, policy.policy_key, policy.version_no,
            policy.schema_version, policy.display_name, policy.notice_text,
            policy.canonical_sha256
     FROM playtest_consent_policy_assignment AS assignment
     INNER JOIN playtest_consent_policy_version AS policy
       ON policy.id = assignment.policy_version_id
     WHERE assignment.scope = 'feedbackSubmission'";

const ACTIVE_POLICY_LOCKED_QUERY: &str =
    "SELECT policy.id, policy.scope, policy.policy_key, policy.version_no,
            policy.schema_version, policy.display_name, policy.notice_text,
            policy.canonical_sha256
     FROM playtest_consent_policy_assignment AS assignment
     INNER JOIN playtest_consent_policy_version AS policy
       ON policy.id = assignment.policy_version_id
     WHERE assignment.scope = 'feedbackSubmission'
     FOR SHARE";

const CURRENT_CONSENT_QUERY: &str =
    "SELECT consent.policy_version_id, consent.status, consent.revision,
            DATE_FORMAT(consent.granted_at, '%Y-%m-%dT%H:%i:%s.%fZ') AS granted_at,
            DATE_FORMAT(consent.withdrawn_at, '%Y-%m-%dT%H:%i:%s.%fZ') AS withdrawn_at
     FROM playtest_consent AS consent
     WHERE consent.user_id = ? AND consent.scope = 'feedbackSubmission'";

const CURRENT_CONSENT_LOCKED_QUERY: &str =
    "SELECT consent.policy_version_id, consent.status, consent.revision,
            DATE_FORMAT(consent.granted_at, '%Y-%m-%dT%H:%i:%s.%fZ') AS granted_at,
            DATE_FORMAT(consent.withdrawn_at, '%Y-%m-%dT%H:%i:%s.%fZ') AS withdrawn_at
     FROM playtest_consent AS consent
     WHERE consent.user_id = ? AND consent.scope = 'feedbackSubmission'
     FOR UPDATE";

#[derive(Clone)]
pub struct MySqlPlaytestStore {
    pool: MySqlPool,
    rules: Arc<dyn PlaytestRules>,
}

pub const fn create_mysql_playtest_store(
    pool: MySqlPool,
    rules: Arc<dyn PlaytestRules>,
) -> MySqlPlaytestStore {
    MySqlPlaytestStore { pool, rules }
}

#[derive(Debug, sqlx::FromRow)]
struct PolicyRow {
    id: u64,
    scope: String,
    policy_key: String,
    version_no: u32,
    schema_version: u16,
    display_name: String,
    notice_text: String,
    canonical_sha256: String,
}

#[derive(Debug, sqlx::FromRow)]
struct ConsentRow {
    policy_version_id: u64,
    status: String,
    revision: u64,
    granted_at: String,
    withdrawn_at: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct FeedbackRow {
    public_id: String,
    category: String,
    severity: String,
    message: String,
    run_revision: Option<u32>,
    run_manifest_sha256: Option<String>,
    finalization_sha256: Option<String>,
    created_at: String,
}

#[derive(Debug, sqlx::FromRow)]
struct RunReferenceRow {
    manifest_sha256: String,
    finalization_sha256: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct FeedbackDeletionRow {
    public_id: String,
    status: String,
    withdrawn_at: Option<String>,
}

#[async_trait]
impl PlaytestStore for MySqlPlaytestStore {
    async fn overview(&self, user_id: u64) -> Result<PlaytestFeedbackOverview> {
        let mut connection = self.pool.acquire().await?;
        let policy = read_active_policy(&mut connection, false)
            .await?
            .context("active playtest consent policy is missing")?;
        let current = read_current_consent(&mut connection, user_id, false).await?;
        let consent = to_consent_state(current.as_ref(), policy.id);
        let rows = sqlx::query_as::<_, FeedbackRow>(
            "SELECT public_id, category, severity, message, run_revision,
                    run_manifest_sha256, finalization_sha256,
                    DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%s.%fZ') AS created_at
             FROM playtest_feedback
             WHERE user_id = ? AND scope = 'feedbackSubmission' AND status = 'active'
             ORDER BY created_at DESC, id DESC
             LIMIT 20",
        )
        .bind(user_id)
        .fetch_all(&mut *connection)
        .await?;
        let feedback = rows
            .into_iter()
            .map(to_feedback_item)
            .collect::<Result<Vec<_>>>()?;

        Ok(PlaytestFeedbackOverview {
            policy: policy.into_policy(),
            consent,
            feedback,
        })
    }

    async fn set_consent(
        &self,
        user_id: u64,
        command: ConsentCommand,
    ) -> Result<PlaytestStoreResult<ConsentUpdate>> {
        let mut transaction = self.pool.begin().await?;
        lock_user(&mut transaction, user_id).await?;
        let Some(policy) = read_active_policy(&mut transaction, true).await? else {
            transaction.rollback().await?;
            return Ok(PlaytestStoreResult::Rejected(
                PlaytestFailureCode::PolicyUnavailable,
            ));
        };
        let current = read_current_consent(&mut transaction, user_id, true).await?;
        let transition =
            match self
                .rules
                .plan_consent_transition(policy.id, current.as_ref(), &command)
            {
                Ok(transition) => transition,
                Err(code) => {
                    transaction.rollback().await?;
                    return Ok(PlaytestStoreResult::Rejected(code));
                }
            };

        if !transition.changed {
            let consent = to_consent_state(current.as_ref(), policy.id);
            transaction.commit().await?;
            return Ok(PlaytestStoreResult::Accepted(ConsentUpdate {
                consent,
                purged_feedback_count: 0,
            }));
        }

        match current {
            None => {
                sqlx::query(
                    "INSERT INTO playtest_consent
                        (user_id, scope, policy_version_id, status, revision, granted_at)
                     VALUES (?, 'feedbackSubmission', ?, 'granted', ?, UTC_TIMESTAMP(6))",
                )
                .bind(user_id)
                .bind(transition.policy_version_id)
                .bind(transition.revision)
                .execute(&mut *transaction)
                .await?;
            }
            Some(_) => match transition.status {
                ConsentStoredStatus::Granted => {
                    sqlx::query(
                        "UPDATE playtest_consent
                         SET policy_version_id = ?, status = 'granted', revision = ?,
                             granted_at = UTC_TIMESTAMP(6), withdrawn_at = NULL
                         WHERE user_id = ? AND scope = 'feedbackSubmission'",
                    )
                    .bind(transition.policy_version_id)
                    .bind(transition.revision)
                    .bind(user_id)
                    .execute(&mut *transaction)
                    .await?;
                }
                ConsentStoredStatus::Withdrawn => {
                    sqlx::query(
                        "UPDATE playtest_consent
                         SET policy_version_id = ?, status = 'withdrawn', revision = ?,
                             withdrawn_at = UTC_TIMESTAMP(6)
                         WHERE user_id = ? AND scope = 'feedbackSubmission'",
                    )
                    .bind(transition.policy_version_id)
                    .bind(transition.revision)
                    .bind(user_id)
                    .execute(&mut *transaction)
                    .await?;
                }
            },
        }

        sqlx::query(
            "INSERT INTO playtest_consent_event
                (user_id, scope, policy_version_id, consent_revision, action)
             VALUES (?, 'feedbackSubmission', ?, ?, ?)",
        )
        .bind(user_id)
        .bind(transition.policy_version_id)
        .bind(transition.revision)
        .bind(transition.status.as_str())
        .execute(&mut *transaction)
        .await?;

        let purged_feedback_count = if transition.status == ConsentStoredStatus::Withdrawn {
            sqlx::query(
                "UPDATE playtest_feedback
                 SET status = 'withdrawn', category = NULL, severity = NULL, message = NULL,
                     run_revision = NULL, run_manifest_sha256 = NULL,
                     finalization_sha256 = NULL, withdrawn_at = UTC_TIMESTAMP(6)
                 WHERE user_id = ? AND scope = 'feedbackSubmission' AND status = 'active'",
            )
            .bind(user_id)
            .execute(&mut *transaction)
            .await?
            .rows_affected()
        } else {
            0
        };
        let updated = read_current_consent(&mut transaction, user_id, false)
            .await?
            .context("updated playtest consent is missing")?;
        let consent = to_consent_state(Some(&updated), policy.id);

        transaction.commit().await?;
        Ok(PlaytestStoreResult::Accepted(ConsentUpdate {
            consent,
            purged_feedback_count,
        }))
    }

    async fn submit_feedback(
        &self,
        user_id: u64,
        draft: FeedbackDraft,
    ) -> Result<PlaytestStoreResult<FeedbackItem>> {
        let draft = match self.rules.normalize_feedback(draft) {
            Ok(draft) => draft,
            Err(code) => return Ok(PlaytestStoreResult::Rejected(code)),
        };
        let mut transaction = self.pool.begin().await?;
        lock_user(&mut transaction, user_id).await?;
        let Some(policy) = read_active_policy(&mut transaction, true).await? else {
            transaction.rollback().await?;
            return Ok(PlaytestStoreResult::Rejected(
                PlaytestFailureCode::PolicyUnavailable,
            ));
        };
        let Some(consent) = read_current_consent(&mut transaction, user_id, true).await? else {
            transaction.rollback().await?;
            return Ok(PlaytestStoreResult::Rejected(
                PlaytestFailureCode::ConsentRequired,
            ));
        };
        if draft.expected_consent_revision != consent.revision {
            transaction.rollback().await?;
            return Ok(PlaytestStoreResult::Rejected(
                PlaytestFailureCode::RevisionConflict,
            ));
        }
        if consent.status != ConsentStoredStatus::Granted || consent.policy_version_id != policy.id
        {
            transaction.rollback().await?;
            return Ok(PlaytestStoreResult::Rejected(
                PlaytestFailureCode::ConsentRequired,
            ));
        }

        let active_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM playtest_feedback
             WHERE user_id = ? AND scope = 'feedbackSubmission' AND status = 'active'",
        )
        .bind(user_id)
        .fetch_one(&mut *transaction)
        .await?;
        let active_count = u64::try_from(active_count)
            .context("active playtest feedback count cannot be negative")?;
        if active_count >= MAXIMUM_ACTIVE_FEEDBACK {
            transaction.rollback().await?;
            return Ok(PlaytestStoreResult::Rejected(
                PlaytestFailureCode::FeedbackCapacityReached,
            ));
        }

        let run_reference = if let Some(run_revision) = draft.run_revision {
            let reference = sqlx::query_as::<_, RunReferenceRow>(
                "SELECT manifest.manifest_sha256,
                        (
                            SELECT finalization.liquidation_sha256
                            FROM run_finalization AS finalization
                            WHERE finalization.save_id = manifest.save_id
                              AND finalization.run_revision = manifest.run_revision
                              AND finalization.status = 'completed'
                            ORDER BY finalization.id DESC
                            LIMIT 1
                        ) AS finalization_sha256
                 FROM run_manifest AS manifest
                 INNER JOIN save ON save.id = manifest.save_id
                 WHERE save.user_id = ? AND manifest.run_revision = ?",
            )
            .bind(user_id)
            .bind(run_revision)
            .fetch_optional(&mut *transaction)
            .await?;
            let Some(reference) = reference else {
                transaction.rollback().await?;
                return Ok(PlaytestStoreResult::Rejected(
                    PlaytestFailureCode::RunReferenceNotFound,
                ));
            };
            Some(reference)
        } else {
            None
        };
        let consent_event_id: u64 = sqlx::query_scalar(
            "SELECT id FROM playtest_consent_event
             WHERE user_id = ? AND scope = 'feedbackSubmission'
               AND consent_revision = ? AND action = 'granted'",
        )
        .bind(user_id)
        .bind(consent.revision)
        .fetch_one(&mut *transaction)
        .await?;
        let result = sqlx::query(
            "INSERT INTO playtest_feedback
                (public_id, user_id, scope, consent_event_id, status, category, severity,
                 message, run_revision, run_manifest_sha256, finalization_sha256)
             VALUES (LOWER(UUID()), ?, 'feedbackSubmission', ?, 'active', ?, ?, ?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(consent_event_id)
        .bind(draft.category.as_str())
        .bind(draft.severity.as_str())
        .bind(&draft.message)
        .bind(draft.run_revision)
        .bind(
            run_reference
                .as_ref()
                .map(|reference| &reference.manifest_sha256),
        )
        .bind(
            run_reference
                .as_ref()
                .and_then(|reference| reference.finalization_sha256.as_deref()),
        )
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() != 1 {
            bail!("playtest feedback insert did not affect exactly one row");
        }
        let feedback_id = result.last_insert_id();

        transaction.commit().await?;
        let row = sqlx::query_as::<_, FeedbackRow>(
            "SELECT public_id, category, severity, message, run_revision,
                    run_manifest_sha256, finalization_sha256,
                    DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%s.%fZ') AS created_at
             FROM playtest_feedback WHERE id = ?",
        )
        .bind(feedback_id)
        .fetch_one(&self.pool)
        .await?;
        let item = to_feedback_item(row)?;

        Ok(PlaytestStoreResult::Accepted(item))
    }

    async fn delete_feedback(
        &self,
        user_id: u64,
        feedback_id: &str,
    ) -> Result<PlaytestStoreResult<FeedbackDeletion>> {
        if !is_lowercase_uuid(feedback_id) {
            return Ok(PlaytestStoreResult::Rejected(
                PlaytestFailureCode::FeedbackNotFound,
            ));
        }

        let mut transaction = self.pool.begin().await?;
        lock_user(&mut transaction, user_id).await?;
        let row = sqlx::query_as::<_, FeedbackDeletionRow>(
            "SELECT public_id, status,
                    DATE_FORMAT(withdrawn_at, '%Y-%m-%dT%H:%i:%s.%fZ') AS withdrawn_at
             FROM playtest_feedback
             WHERE public_id = ? AND user_id = ? AND scope = 'feedbackSubmission'
             FOR UPDATE",
        )
        .bind(feedback_id)
        .bind(user_id)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(row) = row else {
            transaction.rollback().await?;
            return Ok(PlaytestStoreResult::Rejected(
                PlaytestFailureCode::FeedbackNotFound,
            ));
        };
        if row.status == "withdrawn" {
            let withdrawn_at = row
                .withdrawn_at
                .context("withdrawn feedback has no withdrawal timestamp")?;
            transaction.commit().await?;
            return Ok(PlaytestStoreResult::Accepted(FeedbackDeletion {
                id: row.public_id,
                withdrawn_at,
            }));
        }
        if row.status != "active" {
            bail!("stored playtest feedback status is invalid");
        }

        sqlx::query(
            "UPDATE playtest_feedback
             SET status = 'withdrawn', category = NULL, severity = NULL, message = NULL,
                 run_revision = NULL, run_manifest_sha256 = NULL,
                 finalization_sha256 = NULL, withdrawn_at = UTC_TIMESTAMP(6)
             WHERE public_id = ? AND user_id = ? AND status = 'active'",
        )
        .bind(feedback_id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await?;
        let withdrawn_at: String = sqlx::query_scalar(
            "SELECT DATE_FORMAT(withdrawn_at, '%Y-%m-%dT%H:%i:%s.%fZ')
             FROM playtest_feedback WHERE public_id = ? AND user_id = ?",
        )
        .bind(feedback_id)
        .bind(user_id)
        .fetch_one(&mut *transaction)
        .await?;

        transaction.commit().await?;
        Ok(PlaytestStoreResult::Accepted(FeedbackDeletion {
            id: feedback_id.to_owned(),
            withdrawn_at,
        }))
    }
}

impl PolicyRow {
    fn into_policy(self) -> ConsentPolicy {
        ConsentPolicy {
            id: self.id,
            scope: self.scope,
            policy_key: self.policy_key,
            version: self.version_no,
            schema_version: self.schema_version,
            display_name: self.display_name,
            notice_text: self.notice_text,
            canonical_sha256: self.canonical_sha256,
            analytics_collection: AnalyticsCollection::Disabled,
            maximum_active_feedback: MAXIMUM_ACTIVE_FEEDBACK,
            message_maximum_characters: 500,
        }
    }
}

impl ConsentRow {
    fn into_stored(self) -> Result<StoredConsent> {
        let status = match self.status.as_str() {
            "granted" => ConsentStoredStatus::Granted,
            "withdrawn" => ConsentStoredStatus::Withdrawn,
            _ => bail!("stored playtest consent status is invalid"),
        };
        Ok(StoredConsent {
            policy_version_id: self.policy_version_id,
            status,
            revision: self.revision,
            granted_at: self.granted_at,
            withdrawn_at: self.withdrawn_at,
        })
    }
}

async fn read_active_policy(
    connection: &mut MySqlConnection,
    lock: bool,
) -> Result<Option<PolicyRow>> {
    let query = if lock {
        ACTIVE_POLICY_LOCKED_QUERY
    } else {
        ACTIVE_POLICY_QUERY
    };
    Ok(sqlx::query_as::<_, PolicyRow>(query)
        .fetch_optional(connection)
        .await?)
}

async fn read_current_consent(
    connection: &mut MySqlConnection,
    user_id: u64,
    lock: bool,
) -> Result<Option<StoredConsent>> {
    let query = if lock {
        CURRENT_CONSENT_LOCKED_QUERY
    } else {
        CURRENT_CONSENT_QUERY
    };
    sqlx::query_as::<_, ConsentRow>(query)
        .bind(user_id)
        .fetch_optional(connection)
        .await?
        .map(ConsentRow::into_stored)
        .transpose()
}

async fn lock_user(connection: &mut MySqlConnection, user_id: u64) -> Result<()> {
    let exists = sqlx::query_scalar::<_, u64>("SELECT id FROM user WHERE id = ? FOR UPDATE")
        .bind(user_id)
        .fetch_optional(connection)
        .await?;
    if exists.is_none() {
        bail!("authenticated playtest user is missing");
    }
    Ok(())
}

fn to_consent_state(current: Option<&StoredConsent>, active_policy_id: u64) -> ConsentState {
    let Some(current) = current else {
        return ConsentState {
            status: ConsentDisplayStatus::NotGranted,
            revision: 0,
            policy_version_id: None,
            granted_at: None,
            withdrawn_at: None,
        };
    };
    let status = match current.status {
        ConsentStoredStatus::Granted if current.policy_version_id == active_policy_id => {
            ConsentDisplayStatus::Granted
        }
        ConsentStoredStatus::Granted => ConsentDisplayStatus::PolicyChanged,
        ConsentStoredStatus::Withdrawn => ConsentDisplayStatus::Withdrawn,
    };
    ConsentState {
        status,
        revision: current.revision,
        policy_version_id: Some(current.policy_version_id),
        granted_at: Some(current.granted_at.clone()),
        withdrawn_at: current.withdrawn_at.clone(),
    }
}

fn to_feedback_item(row: FeedbackRow) -> Result<FeedbackItem> {
    let category = match row.category.as_str() {
        "bug" => FeedbackCategory::Bug,
        "balance" => FeedbackCategory::Balance,
        "usability" => FeedbackCategory::Usability,
        "performance" => FeedbackCategory::Performance,
        "rules" => FeedbackCategory::Rules,
        "other" => FeedbackCategory::Other,
        _ => bail!("stored playtest feedback category is invalid"),
    };
    let severity = match row.severity.as_str() {
        "blocking" => FeedbackSeverity::Blocking,
        "major" => FeedbackSeverity::Major,
        "minor" => FeedbackSeverity::Minor,
        "suggestion" => FeedbackSeverity::Suggestion,
        _ => bail!("stored playtest feedback severity is invalid"),
    };
    Ok(FeedbackItem {
        id: row.public_id,
        category,
        severity,
        message: row.message,
        run_revision: row.run_revision,
        run_manifest_sha256: row.run_manifest_sha256,
        finalization_sha256: row.finalization_sha256,
        created_at: row.created_at,
    })
}

fn is_lowercase_uuid(value: &str) -> bool {
    if value.len() != 36 {
        return false;
    }
    value.bytes().enumerate().all(|(index, byte)| match index {
        8 | 13 | 18 | 23 => byte == b'-',
        _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte),
    })
}
