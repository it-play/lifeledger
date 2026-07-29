use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use sqlx::MySqlPool;

use crate::auth::{random_token, token_hash_of};
use crate::day::{DailyAdvanceResult, DailyPipeline};
use crate::store::{
    OfflineAttemptEvent, OfflineAttemptEventKind, OfflineAttemptIdentity, OfflineProgressStore,
    OfflineWorkClaim, ProgressStepContext,
};
use crate::{ENGINE_VERSION, day, finance, market, offline, shutdown_signal, store};

const DEFAULT_POLL_MILLIS: u64 = 5_000;
const MIN_POLL_MILLIS: u64 = 250;
const MAX_POLL_MILLIS: u64 = 60_000;
const MAX_RETRY_BACKOFF: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkerIteration {
    Continue,
    Wait(Duration),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClaimOutcome {
    Completed,
    RetryAfter { retry_no: u16 },
}

pub async fn run(pool: MySqlPool) -> Result<()> {
    let market_generators = market::create_market_generator_registry()
        .context("failed to create the market generator registry")?;
    let markets = Arc::new(store::create_mysql_market_store(
        pool.clone(),
        market_generators,
    ));
    let finance_rules = finance::create_finance_rules();
    let saves = Arc::new(store::create_mysql_save_store(
        pool.clone(),
        finance_rules.clone(),
    ));
    let careers = Arc::new(store::create_mysql_career_store(
        pool.clone(),
        finance_rules.clone(),
    ));
    let lives = Arc::new(store::create_mysql_life_store(pool.clone(), finance_rules));
    let games = day::create_daily_pipeline(saves, markets, careers, lives);
    let offline_progress: Arc<dyn OfflineProgressStore> = Arc::new(
        store::create_mysql_offline_progress_store(pool, offline::create_offline_rules()),
    );
    let holder_token_sha256 = token_hash_of(&random_token()?);
    let poll_interval = poll_interval()?;
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);

    tracing::info!(
        engine_version = ENGINE_VERSION,
        poll_millis = poll_interval.as_millis(),
        "offline progress worker started"
    );

    loop {
        let iteration = tokio::select! {
            () = &mut shutdown => break,
            result = process_next_claim(
                games.as_ref(),
                offline_progress.as_ref(),
                &holder_token_sha256,
            ) => match result {
                Ok(iteration) => iteration,
                Err(error) => {
                    tracing::error!(error = %error, "offline worker iteration failed");
                    WorkerIteration::Wait(poll_interval)
                }
            },
        };
        let delay = match iteration {
            WorkerIteration::Continue => continue,
            WorkerIteration::Wait(delay) => delay,
        };
        tokio::select! {
            () = &mut shutdown => break,
            () = tokio::time::sleep(delay) => {}
        }
    }

    tracing::info!("offline progress worker stopped");
    Ok(())
}

async fn process_next_claim(
    games: &dyn DailyPipeline,
    offline_progress: &dyn OfflineProgressStore,
    holder_token_sha256: &str,
) -> Result<WorkerIteration> {
    let Some(claim) = offline_progress
        .claim_offline_work(holder_token_sha256, ENGINE_VERSION)
        .await?
    else {
        return Ok(WorkerIteration::Wait(poll_interval()?));
    };
    let lease = claim.lease.clone();
    let result = process_claim_days(games, offline_progress, claim).await;
    let release_result = offline_progress.release_lease(&lease).await;

    match (result, release_result) {
        (Err(error), Err(release_error)) => Err(error.context(format!(
            "failed to release offline lease after error: {release_error:#}"
        ))),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error).context("failed to release offline lease"),
        (Ok(ClaimOutcome::Completed), Ok(())) => Ok(WorkerIteration::Continue),
        (Ok(ClaimOutcome::RetryAfter { retry_no }), Ok(())) => Ok(WorkerIteration::Wait(
            retry_backoff(poll_interval()?, retry_no),
        )),
    }
}

async fn process_claim_days(
    games: &dyn DailyPipeline,
    offline_progress: &dyn OfflineProgressStore,
    claim: OfflineWorkClaim,
) -> Result<ClaimOutcome> {
    for batch_index in 0..claim.max_batch_days {
        let game_day = claim
            .next_game_day
            .checked_add(u32::from(batch_index))
            .context("offline game day overflowed")?;
        let attempt_key = random_uuid_v4()?;
        let retry_no = if batch_index == 0 { claim.retry_no } else { 0 };
        let attempt = OfflineAttemptIdentity {
            attempt_key,
            retry_no,
            engine_version: ENGINE_VERSION.to_owned(),
        };
        record_attempt(
            offline_progress,
            &claim,
            game_day,
            &attempt,
            OfflineAttemptEventKind::Started,
            None,
        )
        .await?;
        let progress = ProgressStepContext {
            lease: claim.lease.clone(),
            offline_attempt: Some(attempt.clone()),
        };

        let outcome = match games.advance_one_day(claim.user_id, &progress).await {
            Ok(outcome) => outcome,
            Err(error) => {
                let transient = is_transient_database_error(&error);
                let error_code = if transient {
                    "transientDatabase"
                } else {
                    "dailyPipelinePermanent"
                };
                record_attempt(
                    offline_progress,
                    &claim,
                    game_day,
                    &attempt,
                    OfflineAttemptEventKind::Failed,
                    Some(error_code),
                )
                .await?;
                tracing::error!(
                    save_id = claim.save_id,
                    run_revision = claim.run_revision,
                    game_day,
                    error = %error,
                    "offline day failed"
                );
                if transient {
                    return Ok(ClaimOutcome::RetryAfter {
                        retry_no: attempt.retry_no,
                    });
                }
                let paused = offline_progress
                    .pause_after_permanent_failure(&claim.lease, error_code)
                    .await?;
                tracing::warn!(
                    save_id = claim.save_id,
                    run_revision = claim.run_revision,
                    game_day,
                    paused,
                    "offline setting handled permanent failure"
                );
                return Ok(ClaimOutcome::Completed);
            }
        };

        match outcome {
            DailyAdvanceResult::Advanced(state) => {
                if state.save.game_day != game_day {
                    bail!("offline daily pipeline committed an unexpected game day");
                }
                tracing::info!(
                    save_id = claim.save_id,
                    run_revision = claim.run_revision,
                    game_day,
                    "offline day committed"
                );
            }
            DailyAdvanceResult::CharacterRequired => {
                record_terminal_failure(
                    offline_progress,
                    &claim,
                    game_day,
                    &attempt,
                    "characterRequired",
                )
                .await?;
                offline_progress
                    .pause_after_permanent_failure(&claim.lease, "characterRequired")
                    .await?;
                return Ok(ClaimOutcome::Completed);
            }
            DailyAdvanceResult::TargetReached(_) => {
                record_terminal_failure(
                    offline_progress,
                    &claim,
                    game_day,
                    &attempt,
                    "targetReached",
                )
                .await?;
                offline_progress
                    .pause_after_permanent_failure(&claim.lease, "targetReached")
                    .await?;
                return Ok(ClaimOutcome::Completed);
            }
            DailyAdvanceResult::ProgressBusy(_) => {
                record_terminal_failure(
                    offline_progress,
                    &claim,
                    game_day,
                    &attempt,
                    "progressBusy",
                )
                .await?;
                return Ok(ClaimOutcome::Completed);
            }
        }
    }

    Ok(ClaimOutcome::Completed)
}

async fn record_terminal_failure(
    offline_progress: &dyn OfflineProgressStore,
    claim: &OfflineWorkClaim,
    game_day: u32,
    attempt: &OfflineAttemptIdentity,
    error_code: &'static str,
) -> Result<()> {
    record_attempt(
        offline_progress,
        claim,
        game_day,
        attempt,
        OfflineAttemptEventKind::Failed,
        Some(error_code),
    )
    .await?;
    tracing::warn!(
        save_id = claim.save_id,
        run_revision = claim.run_revision,
        game_day,
        error_code,
        "offline claim stopped"
    );
    Ok(())
}

async fn record_attempt(
    offline_progress: &dyn OfflineProgressStore,
    claim: &OfflineWorkClaim,
    game_day: u32,
    attempt: &OfflineAttemptIdentity,
    event_kind: OfflineAttemptEventKind,
    error_code: Option<&str>,
) -> Result<()> {
    offline_progress
        .record_attempt(OfflineAttemptEvent {
            attempt_key: &attempt.attempt_key,
            event_kind,
            save_id: claim.save_id,
            run_revision: claim.run_revision,
            game_day,
            lease_generation: claim.lease.generation,
            retry_no: attempt.retry_no,
            engine_version: &attempt.engine_version,
            error_code,
        })
        .await
}

fn poll_interval() -> Result<Duration> {
    let millis = match std::env::var("OFFLINE_WORKER_POLL_MILLIS") {
        Ok(raw) => raw
            .parse::<u64>()
            .with_context(|| format!("invalid OFFLINE_WORKER_POLL_MILLIS: {raw}"))?,
        Err(std::env::VarError::NotPresent) => DEFAULT_POLL_MILLIS,
        Err(error) => return Err(error).context("failed to read OFFLINE_WORKER_POLL_MILLIS"),
    };
    if !(MIN_POLL_MILLIS..=MAX_POLL_MILLIS).contains(&millis) {
        bail!("OFFLINE_WORKER_POLL_MILLIS must be between {MIN_POLL_MILLIS} and {MAX_POLL_MILLIS}");
    }
    Ok(Duration::from_millis(millis))
}

fn retry_backoff(base: Duration, retry_no: u16) -> Duration {
    let factor = 1u32 << u32::from(retry_no.min(5));
    base.checked_mul(factor)
        .unwrap_or(MAX_RETRY_BACKOFF)
        .min(MAX_RETRY_BACKOFF)
}

fn is_transient_database_error(error: &anyhow::Error) -> bool {
    error
        .chain()
        .filter_map(|cause| cause.downcast_ref::<sqlx::Error>())
        .any(|error| match error {
            sqlx::Error::Database(database) => {
                matches!(database.code().as_deref(), Some("1205" | "1213"))
            }
            sqlx::Error::Io(_)
            | sqlx::Error::PoolTimedOut
            | sqlx::Error::PoolClosed
            | sqlx::Error::Tls(_) => true,
            _ => false,
        })
}

fn random_uuid_v4() -> Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| anyhow::anyhow!("failed to generate offline attempt id: {error}"))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    Ok(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    mod context_transient_database_failure_repeats {
        use super::*;

        #[test]
        fn given_retry_two_when_calculated_then_wait_is_four_times_the_base() {
            let base = Duration::from_secs(2);

            let delay = retry_backoff(base, 2);

            assert_eq!(delay, Duration::from_secs(8));
        }

        #[test]
        fn given_large_retry_when_calculated_then_wait_is_capped_at_sixty_seconds() {
            let base = Duration::from_secs(5);

            let delay = retry_backoff(base, u16::MAX);

            assert_eq!(delay, MAX_RETRY_BACKOFF);
        }
    }
}
