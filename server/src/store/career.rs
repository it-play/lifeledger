use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use async_trait::async_trait;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use sqlx::{MySql, MySqlPool, Transaction};
use time::Date;

use super::mysql::{
    CommandIdentitySpec, CommandIdentityState, GameCommandReceiptWrite, inspect_command_identity,
    read_state, write_command_identity, write_game_command_receipt, write_ledger_transaction,
};
use super::recruitment::{
    accept_career_invitation_command, accept_career_offer_command, apply_career_command,
    confirm_career_interview_command, decline_career_invitation_command,
    decline_career_offer_command, ensure_recruitment_postings_for_user, read_career_applications,
    read_career_employment, read_career_jobs, read_recruitment_snapshot_in_tx,
    withdraw_career_application_command,
};
use super::types::{
    AcceptCareerInvitationCommand, AcceptCareerOfferCommand, ApplyCareerCommand,
    CancelCareerActivityCommand, CareerActivitiesState, CareerActivityCatalogState,
    CareerActivityReceipt, CareerActivityState, CareerApplicationReceipt,
    CareerApplicationsPageState, CareerArtifactPageQuery, CareerArtifactPageState,
    CareerArtifactReceipt, CareerArtifactState, CareerEmploymentState, CareerEvidenceState,
    CareerInvitationReceipt, CareerJobsPageQuery, CareerJobsPageState, CareerOfferReceipt,
    CareerPageQuery, CareerSnapshotState, CareerSpecsState, CareerStore, CareerStoreResult,
    ConfirmCareerInterviewCommand, DeclineCareerInvitationCommand, DeclineCareerOfferCommand,
    FocusCareerCommand, FocusCareerReceipt, GameCommandCursor, PublishCareerArtifactCommand,
    RecruitmentPostingStore, StartCareerActivityCommand, WithdrawCareerApplicationCommand,
};
use crate::career::{
    ActivityCatalogEntry, ActivityDayInput, ActivityStatus, ArtifactChecklistRule,
    ArtifactCompletenessInput, ArtifactError, ArtifactKind, ArtifactValidationInput, BridgeCatalog,
    BridgeEducationMapping, BridgeEvidenceKey, BridgeExperienceMapping, BridgePlanInput,
    ChecklistRule, EvidenceKind, EvidencePeriodFields, Industry, LifeStatus,
    LifeStatusEffortCapacities, SpecActivity, SpecCatalogEntry, SpecEvidence, SpecScoreInput,
    create_activity_planner, create_artifact_rules, create_bridge_evidence_planner,
    create_spec_score_rules,
};
use crate::character::Character;
use crate::finance::{
    CommandCursor, CommandId, FinanceRules, LedgerAccountCode, LedgerPosting, LedgerSource,
    LedgerSourceKind, LedgerTransactionDraft, ResourceId, RunId, RunPolicyContext,
};

const COMMAND_KIND_FOCUS: &str = "careerFocus";
const COMMAND_KIND_ACTIVITY_START: &str = "careerActivityStart";
const COMMAND_KIND_ACTIVITY_CANCEL: &str = "careerActivityCancel";
const COMMAND_KIND_ARTIFACT_PUBLISH: &str = "careerArtifactPublish";
const MAX_PAGE_LIMIT: u32 = 200;
const MAX_ACTIVE_ACTIVITIES: usize = 3;

#[derive(Clone)]
pub struct MySqlCareerStore {
    pool: MySqlPool,
    finance_rules: Arc<dyn FinanceRules>,
}

pub fn create_mysql_career_store(
    pool: MySqlPool,
    finance_rules: Arc<dyn FinanceRules>,
) -> MySqlCareerStore {
    MySqlCareerStore {
        pool,
        finance_rules,
    }
}

#[derive(sqlx::FromRow)]
struct CareerScopeRow {
    save_id: u64,
    run_revision: u32,
    game_day: u32,
    career_catalog_bundle_id: u64,
    focused_job_family_key: String,
    birth_date: Date,
}

#[derive(sqlx::FromRow)]
struct LockedCareerSaveRow {
    id: u64,
    market_world_id: u64,
    policy_set_id: u64,
    run_revision: u32,
    state_revision: u64,
    game_day: u32,
    cash_krw: i64,
    has_character: bool,
    career_catalog_bundle_id: Option<u64>,
    focused_job_family_key: Option<String>,
    birth_date: Option<Date>,
}

#[derive(sqlx::FromRow)]
struct EvidenceRow {
    id: u64,
    evidence_key: String,
    catalog_entry_id: u64,
    catalog_entry_key: String,
    display_name: String,
    kind: String,
    acquired_game_day: u32,
    expires_on_game_day: Option<u32>,
    period_start_date: Option<Date>,
    period_end_exclusive_date: Option<Date>,
    source_kind: String,
}

#[derive(sqlx::FromRow)]
struct SpecCatalogContributionRow {
    entry_key: String,
    kind: String,
    stackable: bool,
    job_family_key: String,
    contribution_bp: i64,
}

#[derive(sqlx::FromRow)]
struct ActivityCatalogRow {
    id: u64,
    activity_key: String,
    display_name: String,
    output_kind: String,
    evidence_entry_key: String,
    minimum_calendar_days: u32,
    required_effort_units: u64,
    daily_effort_cap_units: u64,
    cost_krw: i64,
    life_status: String,
}

#[derive(sqlx::FromRow)]
struct ActivityRow {
    id: u64,
    catalog_entry_id: u64,
    activity_key: String,
    display_name: String,
    status: String,
    priority: Option<u8>,
    started_game_day: Option<u32>,
    accumulated_effort_units: u64,
    required_effort_units: u64,
    minimum_calendar_days: u32,
    daily_effort_cap_units: u64,
    completed_game_day: Option<u32>,
    cancelled_game_day: Option<u32>,
}

#[derive(sqlx::FromRow)]
struct ArtifactRow {
    id: u64,
    artifact_kind: String,
    version_no: u32,
    headline: String,
    summary: String,
    open_to_work: Option<bool>,
    completeness_bp: i64,
    created_game_day: u32,
}

#[derive(sqlx::FromRow)]
struct ChecklistRow {
    rule_kind: String,
    minimum_count: Option<u8>,
    dimension: Option<String>,
    evidence_kind: Option<String>,
    weight_bp: i64,
}

#[derive(sqlx::FromRow)]
struct ActivityStartCatalogRow {
    id: u64,
    display_name: String,
    cost_krw: i64,
    allowed_now: bool,
}

#[derive(sqlx::FromRow)]
struct BridgeEducationRow {
    education: String,
    evidence_key: String,
    entry_key: String,
}

#[derive(sqlx::FromRow)]
struct BridgeOrderedRow {
    position: u32,
    evidence_key: String,
    entry_key: String,
}

#[derive(sqlx::FromRow)]
struct BridgeCatalogEntryRow {
    id: u64,
    entry_key: String,
    kind: String,
    validity_days: Option<u32>,
}

#[derive(sqlx::FromRow)]
struct CompletedActivityEvidenceRow {
    evidence_catalog_entry_id: u64,
    evidence_kind: String,
    validity_days: Option<u32>,
}

#[async_trait]
impl CareerStore for MySqlCareerStore {
    async fn specs(&self, user_id: u64, query: CareerPageQuery) -> Result<CareerSpecsState> {
        validate_page_query(&query)?;
        let mut tx = self.pool.begin().await?;
        let scope = read_scope_for_user(&mut tx, user_id).await?;
        let possessed_scores = read_possessed_scores(&mut tx, &scope).await?;
        let mut rows = read_evidence_page(&mut tx, &scope, &query).await?;
        let has_more = truncate_page_rows(&mut rows, query.limit)?;
        let items = rows
            .into_iter()
            .map(evidence_state_from_row)
            .collect::<Result<Vec<_>>>()?;
        let next_before = has_more.then(|| items.last().map(|item| item.id)).flatten();
        tx.commit().await?;

        Ok(CareerSpecsState {
            focused_job_family_key: scope.focused_job_family_key,
            possessed_scores,
            items,
            next_before,
        })
    }

    async fn activities(
        &self,
        user_id: u64,
        query: CareerPageQuery,
    ) -> Result<CareerActivitiesState> {
        validate_page_query(&query)?;
        let mut tx = self.pool.begin().await?;
        let scope = read_scope_for_user(&mut tx, user_id).await?;
        let catalog = read_activity_catalog_states(&mut tx, &scope).await?;
        let active = read_active_activity_states(&mut tx, &scope).await?;
        let mut rows = read_activity_page(&mut tx, &scope, &query).await?;
        let has_more = truncate_page_rows(&mut rows, query.limit)?;
        let items = rows
            .into_iter()
            .map(|row| activity_state_from_row(row, scope.game_day))
            .collect::<Result<Vec<_>>>()?;
        let next_before = has_more.then(|| items.last().map(|item| item.id)).flatten();
        tx.commit().await?;

        Ok(CareerActivitiesState {
            catalog,
            active,
            items,
            next_before,
        })
    }

    async fn artifacts(
        &self,
        user_id: u64,
        query: CareerArtifactPageQuery,
    ) -> Result<CareerArtifactPageState> {
        validate_page_query(&query.page)?;
        let mut tx = self.pool.begin().await?;
        let scope = read_scope_for_user(&mut tx, user_id).await?;
        let kind = query.kind.map(|value| enum_to_db(&value)).transpose()?;
        let mut rows =
            read_artifact_page_rows(&mut tx, &scope, kind.as_deref(), &query.page).await?;
        let has_more = truncate_page_rows(&mut rows, query.page.limit)?;
        let items = hydrate_artifacts(&mut tx, &scope, rows).await?;
        let next_before = has_more.then(|| items.last().map(|item| item.id)).flatten();
        tx.commit().await?;

        Ok(CareerArtifactPageState { items, next_before })
    }

    async fn jobs(&self, user_id: u64, query: CareerJobsPageQuery) -> Result<CareerJobsPageState> {
        read_career_jobs(&self.pool, user_id, query).await
    }

    async fn applications(
        &self,
        user_id: u64,
        query: CareerPageQuery,
    ) -> Result<CareerApplicationsPageState> {
        read_career_applications(&self.pool, user_id, query).await
    }

    async fn employment(&self, user_id: u64) -> Result<CareerEmploymentState> {
        read_career_employment(&self.pool, user_id).await
    }

    async fn focus(
        &self,
        user_id: u64,
        command: &FocusCareerCommand,
    ) -> Result<CareerStoreResult<FocusCareerReceipt>> {
        let fingerprint = focus_fingerprint(command);
        let mut tx = self.pool.begin().await?;
        let Some(current) = lock_save_for_user(&mut tx, user_id).await? else {
            tx.commit().await?;
            return Ok(CareerStoreResult::Rejected(
                crate::career::CareerFailureCode::CharacterRequired,
            ));
        };
        let identity = CommandIdentitySpec {
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_FOCUS,
            payload_sha256: &fingerprint,
            cursor: command.cursor,
        };
        if let Some(result) =
            replay_or_conflict::<FocusCareerReceipt>(&mut tx, &current, &identity, &fingerprint)
                .await?
        {
            return finish_focus_replay(tx, current.id, result).await;
        }
        if let Some(failure) = validate_current(&current, command.cursor) {
            tx.commit().await?;
            return Ok(CareerStoreResult::Rejected(failure));
        }
        let bundle_id = current
            .career_catalog_bundle_id
            .context("character save has no career run")?;
        let focus_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM career_job_family
                 WHERE career_catalog_bundle_id = ? AND BINARY job_family_key = BINARY ?
             )",
        )
        .bind(bundle_id)
        .bind(&command.focused_job_family_key)
        .fetch_one(&mut *tx)
        .await?;
        if !focus_exists {
            tx.commit().await?;
            return Ok(CareerStoreResult::Rejected(
                crate::career::CareerFailureCode::CatalogUnavailable,
            ));
        }

        write_command_identity(&mut tx, current.id, &identity).await?;
        if current.focused_job_family_key.as_deref() != Some(&command.focused_job_family_key) {
            let update = sqlx::query(
                "UPDATE career_run SET focused_job_family_key = ?
                 WHERE save_id = ? AND run_revision = ? AND career_catalog_bundle_id = ?",
            )
            .bind(&command.focused_job_family_key)
            .bind(current.id)
            .bind(current.run_revision)
            .bind(bundle_id)
            .execute(&mut *tx)
            .await?;
            ensure!(update.rows_affected() == 1, "career focus was not updated");
        }
        let committed = increment_state_revision(&mut tx, &current, current.cash_krw).await?;
        let receipt = FocusCareerReceipt {
            command_id: command.command_id.clone(),
            focused_job_family_key: command.focused_job_family_key.clone(),
            replayed: false,
        };
        write_game_command_receipt(
            &mut tx,
            GameCommandReceiptWrite {
                save_id: current.id,
                command_id: &command.command_id,
                command_kind: COMMAND_KIND_FOCUS,
                payload_sha256: &fingerprint,
                market_world_id: current.market_world_id,
                committed_cursor: committed,
                result: &receipt,
                ledger_transaction_id: None,
            },
        )
        .await?;
        let save = read_state(&mut tx, current.id).await?;
        tx.commit().await?;
        Ok(CareerStoreResult::Applied {
            receipt,
            save: Box::new(save),
        })
    }

    async fn start_activity(
        &self,
        user_id: u64,
        command: &StartCareerActivityCommand,
    ) -> Result<CareerStoreResult<CareerActivityReceipt>> {
        let fingerprint = activity_start_fingerprint(command);
        let mut tx = self.pool.begin().await?;
        let Some(current) = lock_save_for_user(&mut tx, user_id).await? else {
            tx.commit().await?;
            return Ok(CareerStoreResult::Rejected(
                crate::career::CareerFailureCode::CharacterRequired,
            ));
        };
        let identity = CommandIdentitySpec {
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_ACTIVITY_START,
            payload_sha256: &fingerprint,
            cursor: command.cursor,
        };
        if let Some(result) =
            replay_or_conflict::<CareerActivityReceipt>(&mut tx, &current, &identity, &fingerprint)
                .await?
        {
            return finish_activity_replay(tx, current.id, result).await;
        }
        if let Some(failure) = validate_current(&current, command.cursor) {
            tx.commit().await?;
            return Ok(CareerStoreResult::Rejected(failure));
        }
        if !(1..=3).contains(&command.priority) {
            tx.commit().await?;
            return Ok(CareerStoreResult::Rejected(
                crate::career::CareerFailureCode::InvalidCommand,
            ));
        }
        let bundle_id = current
            .career_catalog_bundle_id
            .context("character save has no career run")?;
        let catalog: Option<ActivityStartCatalogRow> = sqlx::query_as(
            "SELECT catalog.id, catalog.display_name, catalog.cost_krw,
                    EXISTS(
                        SELECT 1 FROM activity_catalog_allowed_status AS allowed
                        WHERE allowed.career_catalog_bundle_id = catalog.career_catalog_bundle_id
                          AND allowed.activity_catalog_entry_id = catalog.id
                          AND allowed.life_status = 'unemployed'
                    ) AS allowed_now
             FROM activity_catalog_entry AS catalog
             WHERE catalog.career_catalog_bundle_id = ? AND catalog.id = ?",
        )
        .bind(bundle_id)
        .bind(command.activity_catalog_entry_id.get())
        .fetch_optional(&mut *tx)
        .await?;
        let Some(catalog) = catalog else {
            tx.commit().await?;
            return Ok(CareerStoreResult::Rejected(
                crate::career::CareerFailureCode::CatalogUnavailable,
            ));
        };
        if !catalog.allowed_now {
            tx.commit().await?;
            return Ok(CareerStoreResult::Rejected(
                crate::career::CareerFailureCode::NotEligible,
            ));
        }
        let (active_count, priority_count): (i64, i64) = sqlx::query_as(
            "SELECT COUNT(*), COUNT(CASE WHEN priority = ? THEN 1 END)
             FROM spec_activity
             WHERE save_id = ? AND run_revision = ? AND status = 'active'",
        )
        .bind(command.priority)
        .bind(current.id)
        .bind(current.run_revision)
        .fetch_one(&mut *tx)
        .await?;
        if active_count >= MAX_ACTIVE_ACTIVITIES as i64 || priority_count != 0 {
            tx.commit().await?;
            return Ok(CareerStoreResult::Rejected(
                crate::career::CareerFailureCode::ActivityLimit,
            ));
        }
        let next_cash = match current.cash_krw.checked_sub(catalog.cost_krw) {
            Some(value) if value >= 0 => value,
            _ => {
                tx.commit().await?;
                return Ok(CareerStoreResult::Rejected(
                    crate::career::CareerFailureCode::InsufficientWalletCash,
                ));
            }
        };

        write_command_identity(&mut tx, current.id, &identity).await?;
        let activity_insert = sqlx::query(
            "INSERT INTO spec_activity
                 (save_id, run_revision, career_catalog_bundle_id,
                  activity_catalog_entry_id, status, priority, planned_game_day,
                  started_game_day, accumulated_effort_units)
             VALUES (?, ?, ?, ?, 'planned', NULL, ?, NULL, 0)",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(bundle_id)
        .bind(catalog.id)
        .bind(current.game_day)
        .execute(&mut *tx)
        .await?;
        let activity_id = activity_insert.last_insert_id();
        let ledger_transaction_id = if catalog.cost_krw == 0 {
            None
        } else {
            let ledger = self
                .finance_rules
                .create_ledger_transaction(LedgerTransactionDraft {
                    policy: RunPolicyContext {
                        run: RunId {
                            save_id: ResourceId::from_u64(current.id),
                            run_revision: current.run_revision,
                        },
                        policy_set_id: ResourceId::from_u64(current.policy_set_id),
                    },
                    source: LedgerSource {
                        kind: LedgerSourceKind::SpecActivity,
                        source_id: activity_id.to_string(),
                    },
                    game_day: current.game_day,
                    description: format!("{} 활동 시작", catalog.display_name),
                    postings: vec![
                        LedgerPosting {
                            account_code: LedgerAccountCode::Wallet,
                            financial_account_id: None,
                            amount_krw: catalog.cost_krw.checked_neg().context(
                                "career activity cost cannot be represented as a wallet posting",
                            )?,
                        },
                        LedgerPosting {
                            account_code: LedgerAccountCode::CareerDevelopmentExpense,
                            financial_account_id: None,
                            amount_krw: catalog.cost_krw,
                        },
                    ],
                })?;
            Some(write_ledger_transaction(&mut tx, &ledger).await?)
        };
        let activity_update = sqlx::query(
            "UPDATE spec_activity
             SET status = 'active', priority = ?, started_game_day = ?,
                 cost_ledger_transaction_id = ?
             WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'planned'",
        )
        .bind(command.priority)
        .bind(current.game_day)
        .bind(ledger_transaction_id)
        .bind(current.id)
        .bind(current.run_revision)
        .bind(activity_id)
        .execute(&mut *tx)
        .await?;
        ensure!(
            activity_update.rows_affected() == 1,
            "career activity was not activated"
        );
        let committed = increment_state_revision(&mut tx, &current, next_cash).await?;
        let receipt = CareerActivityReceipt {
            command_id: command.command_id.clone(),
            activity_id: ResourceId::from_u64(activity_id),
            status: ActivityStatus::Active,
            replayed: false,
        };
        write_game_command_receipt(
            &mut tx,
            GameCommandReceiptWrite {
                save_id: current.id,
                command_id: &command.command_id,
                command_kind: COMMAND_KIND_ACTIVITY_START,
                payload_sha256: &fingerprint,
                market_world_id: current.market_world_id,
                committed_cursor: committed,
                result: &receipt,
                ledger_transaction_id,
            },
        )
        .await?;
        let save = read_state(&mut tx, current.id).await?;
        tx.commit().await?;
        Ok(CareerStoreResult::Applied {
            receipt,
            save: Box::new(save),
        })
    }

    async fn cancel_activity(
        &self,
        user_id: u64,
        command: &CancelCareerActivityCommand,
    ) -> Result<CareerStoreResult<CareerActivityReceipt>> {
        let fingerprint = activity_cancel_fingerprint(command);
        let mut tx = self.pool.begin().await?;
        let Some(current) = lock_save_for_user(&mut tx, user_id).await? else {
            tx.commit().await?;
            return Ok(CareerStoreResult::Rejected(
                crate::career::CareerFailureCode::CharacterRequired,
            ));
        };
        let identity = CommandIdentitySpec {
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_ACTIVITY_CANCEL,
            payload_sha256: &fingerprint,
            cursor: command.cursor,
        };
        if let Some(result) =
            replay_or_conflict::<CareerActivityReceipt>(&mut tx, &current, &identity, &fingerprint)
                .await?
        {
            return finish_activity_replay(tx, current.id, result).await;
        }
        if let Some(failure) = validate_current(&current, command.cursor) {
            tx.commit().await?;
            return Ok(CareerStoreResult::Rejected(failure));
        }
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM spec_activity
                 WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'active'
             )",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(command.activity_id.get())
        .fetch_one(&mut *tx)
        .await?;
        if !exists {
            tx.commit().await?;
            return Ok(CareerStoreResult::Rejected(
                crate::career::CareerFailureCode::InvalidCommand,
            ));
        }

        write_command_identity(&mut tx, current.id, &identity).await?;
        let update = sqlx::query(
            "UPDATE spec_activity
             SET status = 'cancelled', cancelled_game_day = ?
             WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'active'",
        )
        .bind(current.game_day)
        .bind(current.id)
        .bind(current.run_revision)
        .bind(command.activity_id.get())
        .execute(&mut *tx)
        .await?;
        ensure!(
            update.rows_affected() == 1,
            "career activity was not cancelled"
        );
        let committed = increment_state_revision(&mut tx, &current, current.cash_krw).await?;
        let receipt = CareerActivityReceipt {
            command_id: command.command_id.clone(),
            activity_id: command.activity_id,
            status: ActivityStatus::Cancelled,
            replayed: false,
        };
        write_game_command_receipt(
            &mut tx,
            GameCommandReceiptWrite {
                save_id: current.id,
                command_id: &command.command_id,
                command_kind: COMMAND_KIND_ACTIVITY_CANCEL,
                payload_sha256: &fingerprint,
                market_world_id: current.market_world_id,
                committed_cursor: committed,
                result: &receipt,
                ledger_transaction_id: None,
            },
        )
        .await?;
        let save = read_state(&mut tx, current.id).await?;
        tx.commit().await?;
        Ok(CareerStoreResult::Applied {
            receipt,
            save: Box::new(save),
        })
    }

    async fn publish_artifact(
        &self,
        user_id: u64,
        command: &PublishCareerArtifactCommand,
    ) -> Result<CareerStoreResult<CareerArtifactReceipt>> {
        let fingerprint = artifact_publish_fingerprint(command)?;
        let mut tx = self.pool.begin().await?;
        let Some(current) = lock_save_for_user(&mut tx, user_id).await? else {
            tx.commit().await?;
            return Ok(CareerStoreResult::Rejected(
                crate::career::CareerFailureCode::CharacterRequired,
            ));
        };
        let identity = CommandIdentitySpec {
            command_id: &command.command_id,
            command_kind: COMMAND_KIND_ARTIFACT_PUBLISH,
            payload_sha256: &fingerprint,
            cursor: command.cursor,
        };
        if let Some(result) =
            replay_or_conflict::<CareerArtifactReceipt>(&mut tx, &current, &identity, &fingerprint)
                .await?
        {
            return finish_artifact_replay(tx, current.id, result).await;
        }
        if let Some(failure) = validate_current(&current, command.cursor) {
            tx.commit().await?;
            return Ok(CareerStoreResult::Rejected(failure));
        }
        let scope = scope_from_locked(&current)?;
        let owned_rows = read_all_evidence(&mut tx, &scope).await?;
        let owned_evidence = owned_rows
            .iter()
            .map(evidence_domain_from_row)
            .collect::<Result<Vec<_>>>()?;
        let current_date: Date = sqlx::query_scalar(
            "SELECT DATE_ADD(start_date, INTERVAL ? DAY) FROM market_world WHERE id = ?",
        )
        .bind(current.game_day)
        .bind(current.market_world_id)
        .fetch_one(&mut *tx)
        .await?;
        let artifact_rules = create_artifact_rules();
        let canonical = match artifact_rules.canonicalize(ArtifactValidationInput {
            draft: &command.draft,
            current_date,
            birth_date: scope.birth_date,
            owned_evidence: &owned_evidence,
        }) {
            Ok(value) => value,
            Err(error) => {
                tx.commit().await?;
                return Ok(CareerStoreResult::Rejected(artifact_failure(&error)));
            }
        };
        let checklist =
            read_artifact_checklist(&mut tx, scope.career_catalog_bundle_id, canonical.kind)
                .await?;
        let completeness_bp =
            match artifact_rules.calculate_completeness(ArtifactCompletenessInput {
                artifact: &canonical,
                owned_evidence: &owned_evidence,
                checklist: &checklist,
            }) {
                Ok(value) => value,
                Err(error) => {
                    tx.commit().await?;
                    return Ok(CareerStoreResult::Rejected(artifact_failure(&error)));
                }
            };
        let industry_ids = resolve_industry_ids(
            &mut tx,
            scope.career_catalog_bundle_id,
            canonical
                .linkedin
                .as_ref()
                .map_or(&[][..], |fields| fields.industries.as_slice()),
        )
        .await?;
        let artifact_kind = enum_to_db(&canonical.kind)?;
        let current_version: Option<u32> = sqlx::query_scalar(
            "SELECT MAX(version_no) FROM profile_artifact_version
             WHERE save_id = ? AND run_revision = ? AND BINARY artifact_kind = BINARY ?",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(&artifact_kind)
        .fetch_one(&mut *tx)
        .await?;
        let version_no = current_version
            .unwrap_or(0)
            .checked_add(1)
            .context("career artifact version overflowed")?;

        write_command_identity(&mut tx, current.id, &identity).await?;
        let artifact_insert = sqlx::query(
            "INSERT INTO profile_artifact_version
                 (save_id, run_revision, career_catalog_bundle_id, artifact_kind,
                  version_no, headline, summary, open_to_work, completeness_bp,
                  created_game_day, sealed_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL)",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(scope.career_catalog_bundle_id)
        .bind(&artifact_kind)
        .bind(version_no)
        .bind(&canonical.headline)
        .bind(&canonical.summary)
        .bind(
            canonical
                .linkedin
                .as_ref()
                .map(|fields| fields.open_to_work),
        )
        .bind(completeness_bp)
        .bind(current.game_day)
        .execute(&mut *tx)
        .await?;
        let artifact_id = artifact_insert.last_insert_id();
        for (index, evidence_id) in canonical.evidence_ids.iter().enumerate() {
            sqlx::query(
                "INSERT INTO profile_artifact_evidence
                     (save_id, run_revision, career_catalog_bundle_id,
                      profile_artifact_version_id, evidence_id, selection_order)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(current.id)
            .bind(current.run_revision)
            .bind(scope.career_catalog_bundle_id)
            .bind(artifact_id)
            .bind(evidence_id)
            .bind(u8::try_from(index + 1).context("too many artifact evidence selections")?)
            .execute(&mut *tx)
            .await?;
        }
        for (index, industry_id) in industry_ids.iter().enumerate() {
            sqlx::query(
                "INSERT INTO profile_artifact_industry
                     (save_id, run_revision, career_catalog_bundle_id,
                      profile_artifact_version_id, career_industry_id, selection_order)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(current.id)
            .bind(current.run_revision)
            .bind(scope.career_catalog_bundle_id)
            .bind(artifact_id)
            .bind(industry_id)
            .bind(u8::try_from(index + 1).context("too many artifact industries")?)
            .execute(&mut *tx)
            .await?;
        }
        let seal = sqlx::query(
            "UPDATE profile_artifact_version SET sealed_at = CURRENT_TIMESTAMP(3)
             WHERE save_id = ? AND run_revision = ? AND id = ? AND sealed_at IS NULL",
        )
        .bind(current.id)
        .bind(current.run_revision)
        .bind(artifact_id)
        .execute(&mut *tx)
        .await?;
        ensure!(seal.rows_affected() == 1, "career artifact was not sealed");
        let committed = increment_state_revision(&mut tx, &current, current.cash_krw).await?;
        let receipt = CareerArtifactReceipt {
            command_id: command.command_id.clone(),
            artifact_version_id: ResourceId::from_u64(artifact_id),
            kind: canonical.kind,
            version_no,
            replayed: false,
        };
        write_game_command_receipt(
            &mut tx,
            GameCommandReceiptWrite {
                save_id: current.id,
                command_id: &command.command_id,
                command_kind: COMMAND_KIND_ARTIFACT_PUBLISH,
                payload_sha256: &fingerprint,
                market_world_id: current.market_world_id,
                committed_cursor: committed,
                result: &receipt,
                ledger_transaction_id: None,
            },
        )
        .await?;
        let save = read_state(&mut tx, current.id).await?;
        tx.commit().await?;
        Ok(CareerStoreResult::Applied {
            receipt,
            save: Box::new(save),
        })
    }

    async fn apply(
        &self,
        user_id: u64,
        command: &ApplyCareerCommand,
    ) -> Result<CareerStoreResult<CareerApplicationReceipt>> {
        apply_career_command(&self.pool, user_id, command).await
    }

    async fn confirm_interview(
        &self,
        user_id: u64,
        command: &ConfirmCareerInterviewCommand,
    ) -> Result<CareerStoreResult<CareerApplicationReceipt>> {
        confirm_career_interview_command(&self.pool, user_id, command).await
    }

    async fn withdraw_application(
        &self,
        user_id: u64,
        command: &WithdrawCareerApplicationCommand,
    ) -> Result<CareerStoreResult<CareerApplicationReceipt>> {
        withdraw_career_application_command(&self.pool, user_id, command).await
    }

    async fn accept_invitation(
        &self,
        user_id: u64,
        command: &AcceptCareerInvitationCommand,
    ) -> Result<CareerStoreResult<CareerInvitationReceipt>> {
        accept_career_invitation_command(&self.pool, user_id, command).await
    }

    async fn decline_invitation(
        &self,
        user_id: u64,
        command: &DeclineCareerInvitationCommand,
    ) -> Result<CareerStoreResult<CareerInvitationReceipt>> {
        decline_career_invitation_command(&self.pool, user_id, command).await
    }

    async fn accept_offer(
        &self,
        user_id: u64,
        command: &AcceptCareerOfferCommand,
    ) -> Result<CareerStoreResult<CareerOfferReceipt>> {
        accept_career_offer_command(&self.pool, user_id, command).await
    }

    async fn decline_offer(
        &self,
        user_id: u64,
        command: &DeclineCareerOfferCommand,
    ) -> Result<CareerStoreResult<CareerOfferReceipt>> {
        decline_career_offer_command(&self.pool, user_id, command).await
    }
}

#[async_trait]
impl RecruitmentPostingStore for MySqlCareerStore {
    async fn ensure_postings_for_user(&self, user_id: u64, target_game_day: u32) -> Result<()> {
        ensure_recruitment_postings_for_user(&self.pool, user_id, target_game_day).await
    }
}

async fn replay_or_conflict<T: DeserializeOwned>(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedCareerSaveRow,
    identity: &CommandIdentitySpec<'_>,
    fingerprint: &str,
) -> Result<Option<Result<T, crate::career::CareerFailureCode>>> {
    match inspect_command_identity(tx, current.id, identity).await? {
        CommandIdentityState::Conflict => Ok(Some(Err(
            crate::career::CareerFailureCode::IdempotencyConflict,
        ))),
        CommandIdentityState::Matching => {
            let receipt = read_receipt(
                tx,
                current.id,
                identity.command_id,
                identity.command_kind,
                fingerprint,
            )
            .await?
            .context("career command identity has no final receipt")?;
            Ok(Some(Ok(receipt)))
        }
        CommandIdentityState::Missing => Ok(None),
    }
}

async fn finish_focus_replay(
    mut tx: Transaction<'_, MySql>,
    save_id: u64,
    result: Result<FocusCareerReceipt, crate::career::CareerFailureCode>,
) -> Result<CareerStoreResult<FocusCareerReceipt>> {
    match result {
        Err(failure) => {
            tx.commit().await?;
            Ok(CareerStoreResult::Rejected(failure))
        }
        Ok(mut receipt) => {
            ensure!(
                !receipt.replayed,
                "stored career focus receipt is marked as replayed"
            );
            receipt.replayed = true;
            let save = read_state(&mut tx, save_id).await?;
            tx.commit().await?;
            Ok(CareerStoreResult::Applied {
                receipt,
                save: Box::new(save),
            })
        }
    }
}

async fn finish_activity_replay(
    mut tx: Transaction<'_, MySql>,
    save_id: u64,
    result: Result<CareerActivityReceipt, crate::career::CareerFailureCode>,
) -> Result<CareerStoreResult<CareerActivityReceipt>> {
    match result {
        Err(failure) => {
            tx.commit().await?;
            Ok(CareerStoreResult::Rejected(failure))
        }
        Ok(mut receipt) => {
            ensure!(
                !receipt.replayed,
                "stored career activity receipt is marked as replayed"
            );
            receipt.replayed = true;
            let save = read_state(&mut tx, save_id).await?;
            tx.commit().await?;
            Ok(CareerStoreResult::Applied {
                receipt,
                save: Box::new(save),
            })
        }
    }
}

async fn finish_artifact_replay(
    mut tx: Transaction<'_, MySql>,
    save_id: u64,
    result: Result<CareerArtifactReceipt, crate::career::CareerFailureCode>,
) -> Result<CareerStoreResult<CareerArtifactReceipt>> {
    match result {
        Err(failure) => {
            tx.commit().await?;
            Ok(CareerStoreResult::Rejected(failure))
        }
        Ok(mut receipt) => {
            ensure!(
                !receipt.replayed,
                "stored career artifact receipt is marked as replayed"
            );
            receipt.replayed = true;
            let save = read_state(&mut tx, save_id).await?;
            tx.commit().await?;
            Ok(CareerStoreResult::Applied {
                receipt,
                save: Box::new(save),
            })
        }
    }
}

async fn lock_save_for_user(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
) -> Result<Option<LockedCareerSaveRow>> {
    sqlx::query_as(
        "SELECT save.id, save.market_world_id, save.policy_set_id,
                save.run_revision, save.state_revision, save.game_day, save.cash_krw,
                EXISTS(SELECT 1 FROM `character` WHERE `character`.save_id = save.id)
                    AS has_character,
                career_run.career_catalog_bundle_id,
                career_run.focused_job_family_key,
                career_run.birth_date
         FROM save
         LEFT JOIN career_run
           ON career_run.save_id = save.id
          AND career_run.run_revision = save.run_revision
         WHERE save.user_id = ?
         FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to lock a career command save")
}

fn validate_current(
    current: &LockedCareerSaveRow,
    cursor: CommandCursor,
) -> Option<crate::career::CareerFailureCode> {
    if !current.has_character {
        return Some(crate::career::CareerFailureCode::CharacterRequired);
    }
    if current.career_catalog_bundle_id.is_none()
        || current.focused_job_family_key.is_none()
        || current.birth_date.is_none()
    {
        return Some(crate::career::CareerFailureCode::CatalogUnavailable);
    }
    if current.run_revision != cursor.expected_run_revision
        || current.state_revision != cursor.expected_state_revision
        || current.game_day != cursor.expected_game_day
    {
        return Some(crate::career::CareerFailureCode::SettlementConflict);
    }
    None
}

fn scope_from_locked(current: &LockedCareerSaveRow) -> Result<CareerScopeRow> {
    Ok(CareerScopeRow {
        save_id: current.id,
        run_revision: current.run_revision,
        game_day: current.game_day,
        career_catalog_bundle_id: current
            .career_catalog_bundle_id
            .context("career run has no catalog bundle")?,
        focused_job_family_key: current
            .focused_job_family_key
            .clone()
            .context("career run has no focus")?,
        birth_date: current.birth_date.context("career run has no birth date")?,
    })
}

async fn increment_state_revision(
    tx: &mut Transaction<'_, MySql>,
    current: &LockedCareerSaveRow,
    cash_krw: i64,
) -> Result<GameCommandCursor> {
    let state_revision = current
        .state_revision
        .checked_add(1)
        .context("state revision overflowed in a career command")?;
    let update = sqlx::query(
        "UPDATE save SET cash_krw = ?, state_revision = ?
         WHERE id = ? AND market_world_id = ? AND policy_set_id = ?
           AND run_revision = ? AND state_revision = ? AND game_day = ? AND cash_krw = ?",
    )
    .bind(cash_krw)
    .bind(state_revision)
    .bind(current.id)
    .bind(current.market_world_id)
    .bind(current.policy_set_id)
    .bind(current.run_revision)
    .bind(current.state_revision)
    .bind(current.game_day)
    .bind(current.cash_krw)
    .execute(&mut **tx)
    .await?;
    ensure!(
        update.rows_affected() == 1,
        "career save cursor changed under its lock"
    );
    Ok(GameCommandCursor {
        run_revision: current.run_revision,
        state_revision,
        game_day: current.game_day,
    })
}

async fn read_receipt<T: DeserializeOwned>(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    command_id: &CommandId,
    command_kind: &str,
    payload_sha256: &str,
) -> Result<Option<T>> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT command_kind, payload_sha256, CAST(result AS CHAR)
         FROM command_receipt
         WHERE save_id = ? AND command_id = ? FOR SHARE",
    )
    .bind(save_id)
    .bind(command_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;
    let Some((stored_kind, stored_hash, result_json)) = row else {
        return Ok(None);
    };
    ensure!(
        stored_kind == command_kind && stored_hash == payload_sha256,
        "career receipt disagrees with command identity"
    );
    serde_json::from_str(&result_json)
        .map(Some)
        .context("career receipt result is invalid")
}

fn validate_page_query(query: &CareerPageQuery) -> Result<()> {
    ensure!(
        (1..=MAX_PAGE_LIMIT).contains(&query.limit),
        "career page limit must be between 1 and {MAX_PAGE_LIMIT}"
    );
    ensure!(
        query.before != Some(0),
        "career page cursor must be positive"
    );
    Ok(())
}

fn truncate_page_rows<T>(rows: &mut Vec<T>, limit: u32) -> Result<bool> {
    let limit = usize::try_from(limit).context("career page limit does not fit usize")?;
    let has_more = rows.len() > limit;
    rows.truncate(limit);
    Ok(has_more)
}

async fn read_scope_for_user(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
) -> Result<CareerScopeRow> {
    sqlx::query_as(
        "SELECT save.id AS save_id, save.run_revision, save.game_day,
                career_run.career_catalog_bundle_id,
                career_run.focused_job_family_key, career_run.birth_date
         FROM save
         INNER JOIN career_run
           ON career_run.save_id = save.id
          AND career_run.run_revision = save.run_revision
         INNER JOIN `character` ON `character`.save_id = save.id
         WHERE save.user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("career state requires an active character")
}

async fn read_evidence_page(
    tx: &mut Transaction<'_, MySql>,
    scope: &CareerScopeRow,
    query: &CareerPageQuery,
) -> Result<Vec<EvidenceRow>> {
    sqlx::query_as(
        "SELECT evidence.id, evidence.evidence_key,
                evidence.spec_catalog_entry_id AS catalog_entry_id,
                catalog.entry_key AS catalog_entry_key, catalog.display_name,
                evidence.kind, evidence.acquired_game_day, evidence.expires_on_game_day,
                evidence.period_start_date, evidence.period_end_exclusive_date,
                evidence.source_kind
         FROM spec_evidence AS evidence
         INNER JOIN spec_catalog_entry AS catalog
           ON catalog.career_catalog_bundle_id = evidence.career_catalog_bundle_id
          AND catalog.id = evidence.spec_catalog_entry_id
         WHERE evidence.save_id = ? AND evidence.run_revision = ?
           AND (? IS NULL OR evidence.id < ?)
         ORDER BY evidence.id DESC
         LIMIT ?",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(query.before)
    .bind(query.before)
    .bind(
        query
            .limit
            .checked_add(1)
            .context("career page limit overflowed")?,
    )
    .fetch_all(&mut **tx)
    .await
    .context("failed to read career evidence page")
}

async fn read_all_evidence(
    tx: &mut Transaction<'_, MySql>,
    scope: &CareerScopeRow,
) -> Result<Vec<EvidenceRow>> {
    sqlx::query_as(
        "SELECT evidence.id, evidence.evidence_key,
                evidence.spec_catalog_entry_id AS catalog_entry_id,
                catalog.entry_key AS catalog_entry_key, catalog.display_name,
                evidence.kind, evidence.acquired_game_day, evidence.expires_on_game_day,
                evidence.period_start_date, evidence.period_end_exclusive_date,
                evidence.source_kind
         FROM spec_evidence AS evidence
         INNER JOIN spec_catalog_entry AS catalog
           ON catalog.career_catalog_bundle_id = evidence.career_catalog_bundle_id
          AND catalog.id = evidence.spec_catalog_entry_id
         WHERE evidence.save_id = ? AND evidence.run_revision = ?
         ORDER BY evidence.acquired_game_day, evidence.id",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_all(&mut **tx)
    .await
    .context("failed to read owned career evidence")
}

fn evidence_state_from_row(row: EvidenceRow) -> Result<CareerEvidenceState> {
    Ok(CareerEvidenceState {
        id: ResourceId::from_u64(row.id),
        evidence_key: row.evidence_key,
        catalog_entry_id: ResourceId::from_u64(row.catalog_entry_id),
        catalog_entry_key: row.catalog_entry_key,
        display_name: row.display_name,
        kind: enum_from_db(&row.kind)?,
        acquired_game_day: row.acquired_game_day,
        expires_on_game_day: row.expires_on_game_day,
        period_start_date: row.period_start_date.map(|date| date.to_string()),
        period_end_exclusive_date: row.period_end_exclusive_date.map(|date| date.to_string()),
    })
}

fn evidence_domain_from_row(row: &EvidenceRow) -> Result<SpecEvidence> {
    let kind: EvidenceKind = enum_from_db(&row.kind)?;
    let period = match (row.period_start_date, row.period_end_exclusive_date) {
        (None, None) => EvidencePeriodFields::none(),
        (Some(start), Some(end)) if row.source_kind == "bridgeExperience" && start == end => {
            EvidencePeriodFields::zero_year_bridge(start)
        }
        (Some(start), Some(end)) => EvidencePeriodFields::regular(start, end),
        _ => bail!("stored career evidence has an incomplete period"),
    };
    Ok(SpecEvidence {
        evidence_id: row.id,
        evidence_key: row.evidence_key.clone(),
        catalog_entry_key: row.catalog_entry_key.clone(),
        kind,
        acquired_game_day: row.acquired_game_day,
        expires_on_game_day: row.expires_on_game_day,
        period,
    })
}

async fn read_possessed_scores(
    tx: &mut Transaction<'_, MySql>,
    scope: &CareerScopeRow,
) -> Result<crate::career::DimensionScores> {
    let evidence_rows = read_all_evidence(tx, scope).await?;
    let evidence = evidence_rows
        .iter()
        .map(evidence_domain_from_row)
        .collect::<Result<Vec<_>>>()?;
    let catalog = read_spec_catalog(tx, scope.career_catalog_bundle_id).await?;
    let views = create_spec_score_rules().calculate_score_views(SpecScoreInput {
        evaluated_job_family_key: &scope.focused_job_family_key,
        current_game_day: scope.game_day,
        evidence: &evidence,
        catalog: &catalog,
        visible_evidence_ids: &[],
    })?;
    Ok(views.possessed)
}

async fn read_spec_catalog(
    tx: &mut Transaction<'_, MySql>,
    bundle_id: u64,
) -> Result<Vec<SpecCatalogEntry>> {
    let rows: Vec<SpecCatalogContributionRow> = sqlx::query_as(
        "SELECT entry.entry_key, entry.kind, entry.stackable,
                family.job_family_key, contribution.contribution_bp
         FROM spec_catalog_entry AS entry
         INNER JOIN spec_catalog_contribution AS contribution
           ON contribution.career_catalog_bundle_id = entry.career_catalog_bundle_id
          AND contribution.spec_catalog_entry_id = entry.id
         INNER JOIN career_job_family AS family
           ON family.career_catalog_bundle_id = contribution.career_catalog_bundle_id
          AND family.id = contribution.career_job_family_id
         WHERE entry.career_catalog_bundle_id = ?
         ORDER BY entry.id, family.id",
    )
    .bind(bundle_id)
    .fetch_all(&mut **tx)
    .await?;
    let mut grouped: BTreeMap<
        String,
        (
            EvidenceKind,
            bool,
            Vec<crate::career::JobFamilyContribution>,
        ),
    > = BTreeMap::new();
    for row in rows {
        let kind = enum_from_db(&row.kind)?;
        let entry = grouped
            .entry(row.entry_key)
            .or_insert_with(|| (kind, row.stackable, Vec::new()));
        ensure!(
            entry.0 == kind && entry.1 == row.stackable,
            "spec catalog row drifted"
        );
        entry.2.push(crate::career::JobFamilyContribution {
            job_family_key: row.job_family_key,
            contribution_bp: row.contribution_bp,
        });
    }
    Ok(grouped
        .into_iter()
        .map(
            |(catalog_entry_key, (kind, stackable, contributions))| SpecCatalogEntry {
                catalog_entry_key,
                kind,
                stackable,
                contributions,
            },
        )
        .collect())
}

async fn read_activity_catalog_rows(
    tx: &mut Transaction<'_, MySql>,
    bundle_id: u64,
) -> Result<Vec<ActivityCatalogRow>> {
    sqlx::query_as(
        "SELECT activity.id, activity.activity_key, activity.display_name,
                evidence.kind AS output_kind, evidence.entry_key AS evidence_entry_key,
                activity.minimum_calendar_days, activity.required_effort_units,
                activity.daily_effort_cap_units, activity.cost_krw, allowed.life_status
         FROM activity_catalog_entry AS activity
         INNER JOIN spec_catalog_entry AS evidence
           ON evidence.career_catalog_bundle_id = activity.career_catalog_bundle_id
          AND evidence.id = activity.evidence_catalog_entry_id
         INNER JOIN activity_catalog_allowed_status AS allowed
           ON allowed.career_catalog_bundle_id = activity.career_catalog_bundle_id
          AND allowed.activity_catalog_entry_id = activity.id
         WHERE activity.career_catalog_bundle_id = ?
         ORDER BY activity.activity_key, allowed.life_status",
    )
    .bind(bundle_id)
    .fetch_all(&mut **tx)
    .await
    .context("failed to read career activity catalog")
}

async fn read_activity_catalog_states(
    tx: &mut Transaction<'_, MySql>,
    scope: &CareerScopeRow,
) -> Result<Vec<CareerActivityCatalogState>> {
    let rows = read_activity_catalog_rows(tx, scope.career_catalog_bundle_id).await?;
    let grouped = group_activity_catalog(rows)?;
    ensure!(
        grouped.len() <= MAX_PAGE_LIMIT as usize,
        "career activity catalog exceeds 200 rows"
    );
    Ok(grouped
        .into_values()
        .map(|entry| CareerActivityCatalogState {
            id: ResourceId::from_u64(entry.id),
            activity_key: entry.activity_key,
            display_name: entry.display_name,
            output_kind: entry.output_kind,
            minimum_calendar_days: entry.minimum_calendar_days,
            required_effort_units: entry.required_effort_units,
            daily_effort_cap_units: entry.daily_effort_cap_units,
            allowed_life_statuses: entry.allowed_life_statuses,
            cost_krw: entry.cost_krw,
        })
        .collect())
}

struct GroupedActivityCatalog {
    id: u64,
    activity_key: String,
    display_name: String,
    output_kind: EvidenceKind,
    evidence_entry_key: String,
    minimum_calendar_days: u32,
    required_effort_units: u64,
    daily_effort_cap_units: u64,
    cost_krw: i64,
    allowed_life_statuses: Vec<LifeStatus>,
}

fn group_activity_catalog(
    rows: Vec<ActivityCatalogRow>,
) -> Result<BTreeMap<String, GroupedActivityCatalog>> {
    let mut grouped = BTreeMap::new();
    for row in rows {
        let output_kind = enum_from_db(&row.output_kind)?;
        let life_status = enum_from_db(&row.life_status)?;
        let entry =
            grouped
                .entry(row.activity_key.clone())
                .or_insert_with(|| GroupedActivityCatalog {
                    id: row.id,
                    activity_key: row.activity_key,
                    display_name: row.display_name,
                    output_kind,
                    evidence_entry_key: row.evidence_entry_key,
                    minimum_calendar_days: row.minimum_calendar_days,
                    required_effort_units: row.required_effort_units,
                    daily_effort_cap_units: row.daily_effort_cap_units,
                    cost_krw: row.cost_krw,
                    allowed_life_statuses: Vec::new(),
                });
        ensure!(
            entry.id == row.id && entry.output_kind == output_kind,
            "activity catalog row drifted"
        );
        entry.allowed_life_statuses.push(life_status);
    }
    Ok(grouped)
}

async fn read_active_activity_states(
    tx: &mut Transaction<'_, MySql>,
    scope: &CareerScopeRow,
) -> Result<Vec<CareerActivityState>> {
    let rows: Vec<ActivityRow> = sqlx::query_as(
        "SELECT activity.id, activity.activity_catalog_entry_id AS catalog_entry_id,
                catalog.activity_key, catalog.display_name, activity.status, activity.priority,
                activity.started_game_day, activity.accumulated_effort_units,
                catalog.required_effort_units, catalog.minimum_calendar_days,
                catalog.daily_effort_cap_units, activity.completed_game_day,
                activity.cancelled_game_day
         FROM spec_activity AS activity
         INNER JOIN activity_catalog_entry AS catalog
           ON catalog.career_catalog_bundle_id = activity.career_catalog_bundle_id
          AND catalog.id = activity.activity_catalog_entry_id
         WHERE activity.save_id = ? AND activity.run_revision = ? AND activity.status = 'active'
         ORDER BY activity.priority, activity.id
         LIMIT 4",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() <= MAX_ACTIVE_ACTIVITIES,
        "career active activity bound was exceeded"
    );
    rows.into_iter()
        .map(|row| activity_state_from_row(row, scope.game_day))
        .collect()
}

async fn read_activity_page(
    tx: &mut Transaction<'_, MySql>,
    scope: &CareerScopeRow,
    query: &CareerPageQuery,
) -> Result<Vec<ActivityRow>> {
    sqlx::query_as(
        "SELECT activity.id, activity.activity_catalog_entry_id AS catalog_entry_id,
                catalog.activity_key, catalog.display_name, activity.status, activity.priority,
                activity.started_game_day, activity.accumulated_effort_units,
                catalog.required_effort_units, catalog.minimum_calendar_days,
                catalog.daily_effort_cap_units, activity.completed_game_day,
                activity.cancelled_game_day
         FROM spec_activity AS activity
         INNER JOIN activity_catalog_entry AS catalog
           ON catalog.career_catalog_bundle_id = activity.career_catalog_bundle_id
          AND catalog.id = activity.activity_catalog_entry_id
         WHERE activity.save_id = ? AND activity.run_revision = ?
           AND (? IS NULL OR activity.id < ?)
         ORDER BY activity.id DESC
         LIMIT ?",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(query.before)
    .bind(query.before)
    .bind(
        query
            .limit
            .checked_add(1)
            .context("career page limit overflowed")?,
    )
    .fetch_all(&mut **tx)
    .await
    .context("failed to read career activity page")
}

fn activity_state_from_row(row: ActivityRow, current_game_day: u32) -> Result<CareerActivityState> {
    let status: ActivityStatus = enum_from_db(&row.status)?;
    let elapsed_calendar_days = match row.started_game_day {
        None => 0,
        Some(started) => row
            .completed_game_day
            .or(row.cancelled_game_day)
            .unwrap_or(current_game_day)
            .checked_sub(started)
            .and_then(|days| days.checked_add(1))
            .context("stored career activity dates are invalid")?,
    };
    Ok(CareerActivityState {
        id: ResourceId::from_u64(row.id),
        catalog_entry_id: ResourceId::from_u64(row.catalog_entry_id),
        activity_key: row.activity_key,
        display_name: row.display_name,
        status,
        priority: row.priority,
        started_game_day: row.started_game_day,
        accumulated_effort_units: row.accumulated_effort_units,
        required_effort_units: row.required_effort_units,
        elapsed_calendar_days,
        minimum_calendar_days: row.minimum_calendar_days,
        daily_effort_cap_units: row.daily_effort_cap_units,
        completed_game_day: row.completed_game_day,
        cancelled_game_day: row.cancelled_game_day,
    })
}

async fn read_artifact_page_rows(
    tx: &mut Transaction<'_, MySql>,
    scope: &CareerScopeRow,
    kind: Option<&str>,
    query: &CareerPageQuery,
) -> Result<Vec<ArtifactRow>> {
    sqlx::query_as(
        "SELECT id, artifact_kind, version_no, headline, summary, open_to_work,
                completeness_bp, created_game_day
         FROM profile_artifact_version
         WHERE save_id = ? AND run_revision = ? AND sealed_at IS NOT NULL
           AND (? IS NULL OR BINARY artifact_kind = BINARY ?)
           AND (? IS NULL OR id < ?)
         ORDER BY id DESC
         LIMIT ?",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .bind(kind)
    .bind(kind)
    .bind(query.before)
    .bind(query.before)
    .bind(
        query
            .limit
            .checked_add(1)
            .context("career page limit overflowed")?,
    )
    .fetch_all(&mut **tx)
    .await
    .context("failed to read career artifact page")
}

async fn read_latest_artifact_rows(
    tx: &mut Transaction<'_, MySql>,
    scope: &CareerScopeRow,
) -> Result<Vec<ArtifactRow>> {
    let rows = sqlx::query_as(
        "SELECT artifact.id, artifact.artifact_kind, artifact.version_no,
                artifact.headline, artifact.summary, artifact.open_to_work,
                artifact.completeness_bp, artifact.created_game_day
         FROM profile_artifact_version AS artifact
         WHERE artifact.save_id = ? AND artifact.run_revision = ?
           AND artifact.sealed_at IS NOT NULL
           AND NOT EXISTS (
               SELECT 1 FROM profile_artifact_version AS newer
               WHERE newer.save_id = artifact.save_id
                 AND newer.run_revision = artifact.run_revision
                 AND BINARY newer.artifact_kind = BINARY artifact.artifact_kind
                 AND newer.version_no > artifact.version_no
                 AND newer.sealed_at IS NOT NULL
           )
         ORDER BY artifact.artifact_kind
         LIMIT 4",
    )
    .bind(scope.save_id)
    .bind(scope.run_revision)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(rows.len() <= 3, "latest career artifact bound was exceeded");
    Ok(rows)
}

async fn hydrate_artifacts(
    tx: &mut Transaction<'_, MySql>,
    scope: &CareerScopeRow,
    rows: Vec<ArtifactRow>,
) -> Result<Vec<CareerArtifactState>> {
    let mut artifacts = Vec::with_capacity(rows.len());
    for row in rows {
        let evidence_ids: Vec<u64> = sqlx::query_scalar(
            "SELECT evidence_id FROM profile_artifact_evidence
             WHERE save_id = ? AND run_revision = ? AND profile_artifact_version_id = ?
             ORDER BY selection_order",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(row.id)
        .fetch_all(&mut **tx)
        .await?;
        let industry_keys: Vec<String> = sqlx::query_scalar(
            "SELECT industry.industry_key
             FROM profile_artifact_industry AS selected
             INNER JOIN career_industry AS industry
               ON industry.career_catalog_bundle_id = selected.career_catalog_bundle_id
              AND industry.id = selected.career_industry_id
             WHERE selected.save_id = ? AND selected.run_revision = ?
               AND selected.profile_artifact_version_id = ?
             ORDER BY selected.selection_order",
        )
        .bind(scope.save_id)
        .bind(scope.run_revision)
        .bind(row.id)
        .fetch_all(&mut **tx)
        .await?;
        artifacts.push(CareerArtifactState {
            id: ResourceId::from_u64(row.id),
            kind: enum_from_db(&row.artifact_kind)?,
            version_no: row.version_no,
            headline: row.headline,
            summary: row.summary,
            evidence_ids: evidence_ids.into_iter().map(ResourceId::from_u64).collect(),
            completeness_bp: row.completeness_bp,
            created_game_day: row.created_game_day,
            open_to_work: row.open_to_work,
            industries: industry_keys
                .into_iter()
                .map(|key| enum_from_db(&key))
                .collect::<Result<Vec<_>>>()?,
        });
    }
    Ok(artifacts)
}

async fn read_artifact_checklist(
    tx: &mut Transaction<'_, MySql>,
    bundle_id: u64,
    kind: ArtifactKind,
) -> Result<Vec<ArtifactChecklistRule>> {
    let kind = enum_to_db(&kind)?;
    let rows: Vec<ChecklistRow> = sqlx::query_as(
        "SELECT rule_kind, minimum_count, dimension, evidence_kind, weight_bp
         FROM artifact_checklist_rule
         WHERE career_catalog_bundle_id = ? AND BINARY artifact_kind = BINARY ?
         ORDER BY id",
    )
    .bind(bundle_id)
    .bind(kind)
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter()
        .map(|row| {
            let rule = match row.rule_kind.as_str() {
                "headlinePresent" => ChecklistRule::HeadlinePresent,
                "summaryPresent" => ChecklistRule::SummaryPresent,
                "minimumEvidenceCount" => ChecklistRule::MinimumEvidenceCount {
                    count: row.minimum_count.context("checklist count is missing")?,
                },
                "containsDimension" => ChecklistRule::ContainsDimension {
                    dimension: enum_from_db(
                        row.dimension
                            .as_deref()
                            .context("checklist dimension is missing")?,
                    )?,
                },
                "containsEvidenceKind" => ChecklistRule::ContainsEvidenceKind {
                    evidence_kind: enum_from_db(
                        row.evidence_kind
                            .as_deref()
                            .context("checklist evidence kind is missing")?,
                    )?,
                },
                "projectPresent" => ChecklistRule::ProjectPresent,
                "openToWork" => ChecklistRule::OpenToWork,
                "industryCountAtLeast" => ChecklistRule::IndustryCountAtLeast {
                    count: row.minimum_count.context("checklist count is missing")?,
                },
                _ => bail!("unknown artifact checklist rule kind"),
            };
            Ok(ArtifactChecklistRule {
                rule,
                weight_bp: row.weight_bp,
            })
        })
        .collect()
}

async fn resolve_industry_ids(
    tx: &mut Transaction<'_, MySql>,
    bundle_id: u64,
    industries: &[Industry],
) -> Result<Vec<u64>> {
    let mut ids = Vec::with_capacity(industries.len());
    for industry in industries {
        let key = enum_to_db(industry)?;
        let id: Option<u64> = sqlx::query_scalar(
            "SELECT id FROM career_industry
             WHERE career_catalog_bundle_id = ? AND BINARY industry_key = BINARY ?",
        )
        .bind(bundle_id)
        .bind(key)
        .fetch_optional(&mut **tx)
        .await?;
        ids.push(id.context("artifact industry is not in the pinned career bundle")?);
    }
    Ok(ids)
}

fn artifact_failure(error: &ArtifactError) -> crate::career::CareerFailureCode {
    match error {
        ArtifactError::ArithmeticOverflow => crate::career::CareerFailureCode::LimitExceeded,
        _ => crate::career::CareerFailureCode::InvalidCommand,
    }
}

pub(super) async fn read_career_snapshot_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
) -> Result<CareerSnapshotState> {
    let scope: Option<CareerScopeRow> = sqlx::query_as(
        "SELECT save.id AS save_id, save.run_revision, save.game_day,
                career_run.career_catalog_bundle_id,
                career_run.focused_job_family_key, career_run.birth_date
         FROM save
         INNER JOIN career_run
           ON career_run.save_id = save.id
          AND career_run.run_revision = save.run_revision
         WHERE save.id = ? AND save.run_revision = ? AND save.game_day = ?",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(game_day)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(scope) = scope else {
        let has_character: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM `character` WHERE save_id = ?)")
                .bind(save_id)
                .fetch_one(&mut **tx)
                .await?;
        ensure!(!has_character, "character save has no career run");
        let default_focus: String = sqlx::query_scalar(
            "SELECT bundle.default_focused_job_family_key
             FROM career_catalog_assignment AS assignment
             INNER JOIN career_catalog_bundle AS bundle
               ON bundle.id = assignment.career_catalog_bundle_id
              AND bundle.published_at IS NOT NULL
             WHERE assignment.assignment_key = 'newRun'",
        )
        .fetch_optional(&mut **tx)
        .await?
        .context("active career catalog assignment is missing")?;
        return Ok(CareerSnapshotState::empty(default_focus));
    };
    let possessed_scores = read_possessed_scores(tx, &scope).await?;
    let active_activities = read_active_activity_states(tx, &scope).await?;
    let latest_rows = read_latest_artifact_rows(tx, &scope).await?;
    let latest_artifacts = hydrate_artifacts(tx, &scope, latest_rows).await?;
    let (open_applications, open_invitations, employment) =
        read_recruitment_snapshot_in_tx(tx, scope.save_id, scope.run_revision, game_day).await?;
    Ok(CareerSnapshotState {
        focused_job_family_key: scope.focused_job_family_key,
        possessed_scores,
        active_activities,
        latest_artifacts,
        open_applications,
        open_invitations,
        employment,
    })
}

pub(super) async fn initialize_career_run_and_bridge_evidence_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    career_catalog_bundle_id: u64,
    character: &Character,
) -> Result<()> {
    let (default_focus, world_start_date): (String, Date) = sqlx::query_as(
        "SELECT bundle.default_focused_job_family_key, world.start_date
         FROM career_catalog_bundle AS bundle
         INNER JOIN save ON save.id = ?
         INNER JOIN market_world AS world ON world.id = save.market_world_id
         WHERE bundle.id = ? AND bundle.published_at IS NOT NULL
           AND save.run_revision = ?",
    )
    .bind(save_id)
    .bind(career_catalog_bundle_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await?
    .context("new run career bundle is unavailable")?;
    let education_rows: Vec<BridgeEducationRow> = sqlx::query_as(
        "SELECT bridge.education, bridge.evidence_key, entry.entry_key
         FROM career_bridge_education_mapping AS bridge
         INNER JOIN spec_catalog_entry AS entry
           ON entry.career_catalog_bundle_id = bridge.career_catalog_bundle_id
          AND entry.id = bridge.spec_catalog_entry_id
         WHERE bridge.career_catalog_bundle_id = ? ORDER BY bridge.education",
    )
    .bind(career_catalog_bundle_id)
    .fetch_all(&mut **tx)
    .await?;
    let certification_rows: Vec<BridgeOrderedRow> = sqlx::query_as(
        "SELECT bridge.certification_order AS position, bridge.evidence_key, entry.entry_key
         FROM career_bridge_certification_order AS bridge
         INNER JOIN spec_catalog_entry AS entry
           ON entry.career_catalog_bundle_id = bridge.career_catalog_bundle_id
          AND entry.id = bridge.spec_catalog_entry_id
         WHERE bridge.career_catalog_bundle_id = ? ORDER BY bridge.certification_order",
    )
    .bind(career_catalog_bundle_id)
    .fetch_all(&mut **tx)
    .await?;
    let experience_rows: Vec<BridgeOrderedRow> = sqlx::query_as(
        "SELECT bridge.career_years AS position, bridge.evidence_key, entry.entry_key
         FROM career_bridge_experience_mapping AS bridge
         INNER JOIN spec_catalog_entry AS entry
           ON entry.career_catalog_bundle_id = bridge.career_catalog_bundle_id
          AND entry.id = bridge.spec_catalog_entry_id
         WHERE bridge.career_catalog_bundle_id = ? ORDER BY bridge.career_years",
    )
    .bind(career_catalog_bundle_id)
    .fetch_all(&mut **tx)
    .await?;
    for (index, row) in certification_rows.iter().enumerate() {
        ensure!(
            row.position as usize == index + 1,
            "certification bridge order is incomplete"
        );
    }
    let bridge_catalog = BridgeCatalog {
        default_focused_job_family_key: default_focus,
        education_mappings: education_rows
            .into_iter()
            .map(|row| {
                Ok(BridgeEducationMapping {
                    education: enum_from_db(&row.education)?,
                    evidence: BridgeEvidenceKey {
                        evidence_key: row.evidence_key,
                        catalog_entry_key: row.entry_key,
                    },
                })
            })
            .collect::<Result<Vec<_>>>()?,
        certification_order: certification_rows
            .into_iter()
            .map(|row| BridgeEvidenceKey {
                evidence_key: row.evidence_key,
                catalog_entry_key: row.entry_key,
            })
            .collect(),
        experience_mappings: experience_rows
            .into_iter()
            .map(|row| BridgeExperienceMapping {
                career_years: row.position,
                evidence: BridgeEvidenceKey {
                    evidence_key: row.evidence_key,
                    catalog_entry_key: row.entry_key,
                },
            })
            .collect(),
    };
    let plan = create_bridge_evidence_planner().plan_initial_evidence(BridgePlanInput {
        catalog: &bridge_catalog,
        education: character.education,
        certifications: character.certifications,
        career_years: character.career_years,
        starting_age_years: character.age,
        world_start_date,
    })?;
    sqlx::query(
        "INSERT INTO career_run
             (save_id, run_revision, career_catalog_bundle_id,
              focused_job_family_key, birth_date)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(career_catalog_bundle_id)
    .bind(&plan.focused_job_family_key)
    .bind(plan.birth_date)
    .execute(&mut **tx)
    .await?;
    let catalog_rows: Vec<BridgeCatalogEntryRow> = sqlx::query_as(
        "SELECT id, entry_key, kind, validity_days
         FROM spec_catalog_entry WHERE career_catalog_bundle_id = ?",
    )
    .bind(career_catalog_bundle_id)
    .fetch_all(&mut **tx)
    .await?;
    let catalog = catalog_rows
        .into_iter()
        .map(|row| (row.entry_key.clone(), row))
        .collect::<HashMap<_, _>>();
    for evidence in plan.evidence {
        let entry = catalog
            .get(&evidence.catalog_entry_key)
            .context("bridge evidence catalog entry is missing")?;
        let kind: EvidenceKind = enum_from_db(&entry.kind)?;
        ensure!(
            kind == evidence.kind,
            "bridge evidence kind disagrees with its catalog entry"
        );
        let expires_on_game_day = match entry.validity_days {
            Some(days) => Some(
                evidence
                    .acquired_game_day
                    .checked_add(days)
                    .context("bridge evidence expiry overflowed")?,
            ),
            None => None,
        };
        let source_kind = match evidence.kind {
            EvidenceKind::Education => "bridgeEducation",
            EvidenceKind::Certification => "bridgeCertification",
            EvidenceKind::Experience => "bridgeExperience",
            _ => bail!("bridge produced an unsupported evidence kind"),
        };
        sqlx::query(
            "INSERT INTO spec_evidence
                 (save_id, run_revision, career_catalog_bundle_id, evidence_key,
                  spec_catalog_entry_id, kind, acquired_game_day, expires_on_game_day,
                  period_start_date, period_end_exclusive_date, source_kind)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(save_id)
        .bind(run_revision)
        .bind(career_catalog_bundle_id)
        .bind(&evidence.evidence_key)
        .bind(entry.id)
        .bind(enum_to_db(&evidence.kind)?)
        .bind(evidence.acquired_game_day)
        .bind(expires_on_game_day)
        .bind(evidence.period.start_date)
        .bind(evidence.period.end_exclusive_date)
        .bind(source_kind)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

pub(super) async fn advance_career_activities_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    target_game_day: u32,
) -> Result<()> {
    let scope: CareerScopeRow = sqlx::query_as(
        "SELECT save.id AS save_id, save.run_revision, ? AS game_day,
                career_run.career_catalog_bundle_id,
                career_run.focused_job_family_key, career_run.birth_date
         FROM save
         INNER JOIN career_run
           ON career_run.save_id = save.id AND career_run.run_revision = save.run_revision
         WHERE save.id = ? AND save.run_revision = ?",
    )
    .bind(target_game_day)
    .bind(save_id)
    .bind(run_revision)
    .fetch_optional(&mut **tx)
    .await?
    .context("daily career planner requires an active career run")?;
    let catalog_grouped = group_activity_catalog(
        read_activity_catalog_rows(tx, scope.career_catalog_bundle_id).await?,
    )?;
    let catalog = catalog_grouped
        .values()
        .map(|entry| ActivityCatalogEntry {
            catalog_entry_key: entry.activity_key.clone(),
            minimum_calendar_days: entry.minimum_calendar_days,
            required_effort_units: entry.required_effort_units,
            daily_effort_cap_units: entry.daily_effort_cap_units,
            allowed_life_statuses: entry.allowed_life_statuses.clone(),
            cost_krw: entry.cost_krw,
            evidence_catalog_entry_key: entry.evidence_entry_key.clone(),
        })
        .collect::<Vec<_>>();
    type ActiveCareerActivityRow = (
        u64,
        String,
        String,
        Option<u8>,
        Option<u32>,
        u64,
        Option<u32>,
    );
    let active_rows: Vec<ActiveCareerActivityRow> = sqlx::query_as(
        "SELECT activity.id, catalog.activity_key, activity.status, activity.priority,
                    activity.started_game_day, activity.accumulated_effort_units,
                    activity.completed_game_day
             FROM spec_activity AS activity
             INNER JOIN activity_catalog_entry AS catalog
               ON catalog.career_catalog_bundle_id = activity.career_catalog_bundle_id
              AND catalog.id = activity.activity_catalog_entry_id
             WHERE activity.save_id = ? AND activity.run_revision = ?
               AND activity.status = 'active'
             ORDER BY activity.priority, activity.id
             FOR UPDATE",
    )
    .bind(save_id)
    .bind(run_revision)
    .fetch_all(&mut **tx)
    .await?;
    let activities = active_rows
        .into_iter()
        .map(
            |(
                id,
                catalog_entry_key,
                status,
                priority,
                started_game_day,
                accumulated,
                completed,
            )| {
                Ok(SpecActivity {
                    activity_id: id,
                    catalog_entry_key,
                    status: enum_from_db(&status)?,
                    priority,
                    started_game_day,
                    accumulated_effort_units: accumulated,
                    completed_game_day: completed,
                })
            },
        )
        .collect::<Result<Vec<_>>>()?;
    let capacity_rows: Vec<(String, u64)> = sqlx::query_as(
        "SELECT life_status, effort_units FROM career_effort_capacity
         WHERE career_catalog_bundle_id = ? ORDER BY life_status",
    )
    .bind(scope.career_catalog_bundle_id)
    .fetch_all(&mut **tx)
    .await?;
    let mut capacities = LifeStatusEffortCapacities::default();
    let mut capacity_seen = BTreeMap::new();
    for (status, units) in capacity_rows {
        let status: LifeStatus = enum_from_db(&status)?;
        ensure!(
            capacity_seen.insert(status, ()).is_none(),
            "duplicate career capacity"
        );
        match status {
            LifeStatus::Unemployed => capacities.unemployed = units,
            LifeStatus::Employed => capacities.employed = units,
            LifeStatus::ActiveDuty => capacities.active_duty = units,
            LifeStatus::SocialService => capacities.social_service = units,
            LifeStatus::SpecialService => capacities.special_service = units,
            LifeStatus::OfficerOrNco => capacities.officer_or_nco = units,
        }
    }
    ensure!(
        capacity_seen.len() == 6,
        "career capacity catalog is incomplete"
    );
    let has_active_employment: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM employment_contract
             WHERE save_id = ? AND run_revision = ? AND status = 'active'
               AND start_game_day <= ?
               AND (end_game_day IS NULL OR end_game_day > ?)
         )",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(target_game_day)
    .bind(target_game_day)
    .fetch_one(&mut **tx)
    .await?;
    let plan = create_activity_planner().plan_day(ActivityDayInput {
        current_game_day: target_game_day,
        life_status: if has_active_employment {
            LifeStatus::Employed
        } else {
            LifeStatus::Unemployed
        },
        capacities,
        catalog: &catalog,
        activities: &activities,
    })?;
    for allocation in plan.allocations {
        let status = enum_to_db(&allocation.status)?;
        let update = sqlx::query(
            "UPDATE spec_activity
             SET status = ?, accumulated_effort_units = ?, completed_game_day = ?
             WHERE save_id = ? AND run_revision = ? AND id = ? AND status = 'active'",
        )
        .bind(&status)
        .bind(allocation.accumulated_effort_units)
        .bind(allocation.completed_game_day)
        .bind(save_id)
        .bind(run_revision)
        .bind(allocation.activity_id)
        .execute(&mut **tx)
        .await?;
        ensure!(
            update.rows_affected() == 1,
            "daily career activity update was lost"
        );
        if allocation.status == ActivityStatus::Completed {
            let evidence: CompletedActivityEvidenceRow = sqlx::query_as(
                "SELECT output.id AS evidence_catalog_entry_id,
                        output.kind AS evidence_kind, output.validity_days
                 FROM spec_activity AS activity
                 INNER JOIN activity_catalog_entry AS catalog
                   ON catalog.career_catalog_bundle_id = activity.career_catalog_bundle_id
                  AND catalog.id = activity.activity_catalog_entry_id
                 INNER JOIN spec_catalog_entry AS output
                   ON output.career_catalog_bundle_id = catalog.career_catalog_bundle_id
                  AND output.id = catalog.evidence_catalog_entry_id
                 WHERE activity.save_id = ? AND activity.run_revision = ? AND activity.id = ?",
            )
            .bind(save_id)
            .bind(run_revision)
            .bind(allocation.activity_id)
            .fetch_one(&mut **tx)
            .await?;
            let expires_on_game_day = match evidence.validity_days {
                Some(days) => Some(
                    target_game_day
                        .checked_add(days)
                        .context("activity evidence expiry overflowed")?,
                ),
                None => None,
            };
            sqlx::query(
                "INSERT INTO spec_evidence
                     (save_id, run_revision, career_catalog_bundle_id, evidence_key,
                      spec_catalog_entry_id, kind, acquired_game_day, expires_on_game_day,
                      period_start_date, period_end_exclusive_date, source_kind,
                      source_activity_id)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, 'activity', ?)",
            )
            .bind(save_id)
            .bind(run_revision)
            .bind(scope.career_catalog_bundle_id)
            .bind(format!("activity:{}", allocation.activity_id))
            .bind(evidence.evidence_catalog_entry_id)
            .bind(&evidence.evidence_kind)
            .bind(target_game_day)
            .bind(expires_on_game_day)
            .bind(allocation.activity_id)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

fn focus_fingerprint(command: &FocusCareerCommand) -> String {
    let mut canonical = command_prefix("lifeledger.career.focus.v1", command.cursor);
    push_fingerprint_field(
        &mut canonical,
        "focusedJobFamilyKey",
        &command.focused_job_family_key,
    );
    fingerprint(&canonical)
}

fn activity_start_fingerprint(command: &StartCareerActivityCommand) -> String {
    let mut canonical = command_prefix("lifeledger.career.activity-start.v1", command.cursor);
    push_fingerprint_field(
        &mut canonical,
        "activityCatalogEntryId",
        &command.activity_catalog_entry_id.to_string(),
    );
    push_fingerprint_field(&mut canonical, "priority", &command.priority.to_string());
    fingerprint(&canonical)
}

fn activity_cancel_fingerprint(command: &CancelCareerActivityCommand) -> String {
    let mut canonical = command_prefix("lifeledger.career.activity-cancel.v1", command.cursor);
    push_fingerprint_field(
        &mut canonical,
        "activityId",
        &command.activity_id.to_string(),
    );
    fingerprint(&canonical)
}

fn artifact_publish_fingerprint(command: &PublishCareerArtifactCommand) -> Result<String> {
    let mut canonical = command_prefix("lifeledger.career.artifact-publish.v1", command.cursor);
    push_fingerprint_field(&mut canonical, "kind", &enum_to_db(&command.draft.kind)?);
    push_fingerprint_field(&mut canonical, "headline", &command.draft.headline);
    push_fingerprint_field(&mut canonical, "summary", &command.draft.summary);
    push_fingerprint_field(
        &mut canonical,
        "evidenceCount",
        &command.draft.evidence_ids.len().to_string(),
    );
    for evidence_id in &command.draft.evidence_ids {
        push_fingerprint_field(&mut canonical, "evidenceId", &evidence_id.to_string());
    }
    match &command.draft.linkedin {
        None => push_fingerprint_field(&mut canonical, "linkedin", "none"),
        Some(linkedin) => {
            push_fingerprint_field(&mut canonical, "linkedin", "present");
            push_fingerprint_field(
                &mut canonical,
                "openToWork",
                if linkedin.open_to_work {
                    "true"
                } else {
                    "false"
                },
            );
            push_fingerprint_field(
                &mut canonical,
                "industryCount",
                &linkedin.industries.len().to_string(),
            );
            for industry in &linkedin.industries {
                push_fingerprint_field(&mut canonical, "industry", &enum_to_db(industry)?);
            }
        }
    }
    Ok(fingerprint(&canonical))
}

fn command_prefix(version: &str, cursor: CommandCursor) -> String {
    let mut canonical = String::new();
    push_fingerprint_field(&mut canonical, "version", version);
    push_fingerprint_field(
        &mut canonical,
        "expectedRunRevision",
        &cursor.expected_run_revision.to_string(),
    );
    push_fingerprint_field(
        &mut canonical,
        "expectedStateRevision",
        &cursor.expected_state_revision.to_string(),
    );
    push_fingerprint_field(
        &mut canonical,
        "expectedGameDay",
        &cursor.expected_game_day.to_string(),
    );
    canonical
}

fn push_fingerprint_field(canonical: &mut String, name: &str, value: &str) {
    canonical.push_str(name);
    canonical.push('=');
    canonical.push_str(&value.len().to_string());
    canonical.push(':');
    canonical.push_str(value);
    canonical.push('\n');
}

fn fingerprint(canonical: &str) -> String {
    Sha256::digest(canonical.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn enum_to_db<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_value(value)?
        .as_str()
        .map(str::to_owned)
        .context("career enum did not serialize as a string")
}

fn enum_from_db<T: DeserializeOwned>(raw: &str) -> Result<T> {
    serde_json::from_value(serde_json::Value::String(raw.to_owned()))
        .with_context(|| format!("invalid stored career enum value: {raw}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::career::{ArtifactDraft, LinkedinFields};

    fn given_cursor() -> CommandCursor {
        CommandCursor {
            expected_run_revision: 2,
            expected_state_revision: 7,
            expected_game_day: 11,
        }
    }

    mod context_커리어_명령_payload를_지문화하는_경우 {
        use super::*;

        #[test]
        fn given_줄바꿈이_있는_서로_다른_문자열_when_지문화하면_then_경계가_충돌하지_않는다() {
            let first = PublishCareerArtifactCommand {
                command_id: CommandId::parse("10000000-0000-0000-0000-000000000001")
                    .expect("명령 ID가 유효해야 한다"),
                cursor: given_cursor(),
                draft: ArtifactDraft {
                    kind: ArtifactKind::Resume,
                    headline: "a\nb".to_owned(),
                    summary: "c".to_owned(),
                    evidence_ids: vec![],
                    linkedin: None,
                },
            };
            let mut second = first.clone();
            second.draft.headline = "a".to_owned();
            second.draft.summary = "b\nc".to_owned();

            let first_hash =
                artifact_publish_fingerprint(&first).expect("지문을 만들 수 있어야 한다");
            let second_hash =
                artifact_publish_fingerprint(&second).expect("지문을 만들 수 있어야 한다");

            assert_ne!(first_hash, second_hash);
        }

        #[test]
        fn given_linked_in_업종_순서가_바뀐_명령_when_지문화하면_then_다른_payload로_본다() {
            let command = PublishCareerArtifactCommand {
                command_id: CommandId::parse("10000000-0000-0000-0000-000000000002")
                    .expect("명령 ID가 유효해야 한다"),
                cursor: given_cursor(),
                draft: ArtifactDraft {
                    kind: ArtifactKind::LinkedinProfile,
                    headline: "프로필".to_owned(),
                    summary: String::new(),
                    evidence_ids: vec![1],
                    linkedin: Some(LinkedinFields {
                        open_to_work: true,
                        industries: vec![Industry::ItSoftware, Industry::FinanceInsurance],
                    }),
                },
            };
            let mut changed = command.clone();
            changed
                .draft
                .linkedin
                .as_mut()
                .expect("LinkedIn 필드가 있어야 한다")
                .industries
                .reverse();

            let original =
                artifact_publish_fingerprint(&command).expect("지문을 만들 수 있어야 한다");
            let reordered =
                artifact_publish_fingerprint(&changed).expect("지문을 만들 수 있어야 한다");

            assert_ne!(original, reordered);
        }
    }

    mod context_커리어_이력을_cursor로_나누는_경우 {
        use super::*;

        #[test]
        fn given_limit보다_한건_많은_행_when_page를_자르면_then_다음_cursor가_필요하다() {
            let mut rows = vec![3_u64, 2, 1];

            let has_more = truncate_page_rows(&mut rows, 2).expect("페이지를 자를 수 있어야 한다");

            assert!(has_more);
            assert_eq!(rows, vec![3, 2]);
        }

        #[test]
        fn given_limit안의_행_when_page를_자르면_then_다음_cursor가_필요하지_않다() {
            let mut rows = vec![2_u64, 1];

            let has_more = truncate_page_rows(&mut rows, 2).expect("페이지를 자를 수 있어야 한다");

            assert!(!has_more);
            assert_eq!(rows, vec![2, 1]);
        }
    }

    mod context_커리어_명령의_cursor가_오래된_경우 {
        use super::*;

        #[test]
        fn given_현재상태와_다른_cursor_when_검증하면_then_정산충돌로_거절한다() {
            let current = LockedCareerSaveRow {
                id: 1,
                market_world_id: 1,
                policy_set_id: 1,
                run_revision: 2,
                state_revision: 8,
                game_day: 11,
                cash_krw: 0,
                has_character: true,
                career_catalog_bundle_id: Some(1),
                focused_job_family_key: Some("softwareEngineering".to_owned()),
                birth_date: Some(Date::MIN),
            };

            let result = validate_current(&current, given_cursor());

            assert_eq!(
                result,
                Some(crate::career::CareerFailureCode::SettlementConflict)
            );
        }
    }
}
