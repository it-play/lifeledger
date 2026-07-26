//! MySQL implementation of the deterministic market cache (§4.2, §5).

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use async_trait::async_trait;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sqlx::MySqlPool;
use time::Date;
use tokio::sync::Mutex;

use super::types::{MarketHistoryState, MarketStore, MarketWorldState};
use crate::market::{
    IndexProductTerms, MarketCalibration, MarketDay, MarketGeneratorRegistry, MarketParameters,
    MarketWorld, NullableInterestRateState, NullableM2MarketState,
};

/// History is paged above this layer; one query cannot expose an unbounded shared path.
pub const MAX_MARKET_HISTORY_ROWS: u32 = 3_660;

pub struct MySqlMarketStore {
    pool: MySqlPool,
    generators: MarketGeneratorRegistry,
    world_locks: Mutex<HashMap<u64, Arc<Mutex<()>>>>,
}

pub fn create_mysql_market_store(
    pool: MySqlPool,
    generators: MarketGeneratorRegistry,
) -> MySqlMarketStore {
    MySqlMarketStore {
        pool,
        generators,
        world_locks: Mutex::new(HashMap::new()),
    }
}

#[async_trait]
impl MarketStore for MySqlMarketStore {
    async fn load_world(&self, world_id: u64) -> Result<MarketWorldState> {
        let row: Option<MarketWorldRow> = sqlx::query_as(
            "SELECT w.id, w.world_key, w.seed, w.start_date, w.day0_equity_close_krw,
                    c.version AS calibration_version,
                    CAST(c.parameters AS CHAR) AS calibration_parameters,
                    index_product.id AS index_product_version_id,
                    index_product.product_key AS index_product_key,
                    index_product.day0_close_krw AS index_day0_close_krw,
                    index_product.annual_management_fee_ppm AS index_annual_management_fee_ppm,
                    index_product.annual_distribution_rate_ppm AS index_annual_distribution_rate_ppm,
                    index_product.day_count_denominator AS index_day_count_denominator,
                    index_product.buy_fee_ppm AS index_buy_fee_ppm,
                    index_product.sell_fee_ppm AS index_sell_fee_ppm,
                    index_product.transaction_tax_ppm AS index_transaction_tax_ppm
             FROM market_world AS w
             INNER JOIN market_calibration AS c ON c.id = w.calibration_id
             LEFT JOIN market_world_product_bundle AS bundle
                 ON bundle.market_world_id = w.id
                AND bundle.published_at IS NOT NULL
             LEFT JOIN index_product_version AS index_product
                 ON index_product.id = bundle.index_product_version_id
                AND index_product.published_at IS NOT NULL
             WHERE w.id = ?",
        )
        .bind(world_id)
        .fetch_optional(&self.pool)
        .await?;

        row.context("market world does not exist")?.into_state()
    }

    async fn ensure_day(&self, world_id: u64, target_game_day: u32) -> Result<MarketDay> {
        let world_state = self.load_world(world_id).await?;
        let generator = self
            .generators
            .generator_for(&world_state.calibration)
            .context("market world uses an unregistered generator")?;

        if let Some(day) = fetch_day(&self.pool, world_id, target_game_day).await? {
            return Ok(day);
        }

        let world_lock = self.world_lock(world_id).await;
        let _guard = world_lock.lock().await;

        if let Some(day) = fetch_day(&self.pool, world_id, target_game_day).await? {
            return Ok(day);
        }

        let latest = fetch_latest_day(&self.pool, world_id, target_game_day).await?;
        let mut cursor = match latest {
            Some(day) => day,
            None => {
                let generated = generator.day_zero(&world_state.world)?;
                ensure!(
                    generated.game_day == 0,
                    "market generator returned a nonzero anchor"
                );
                insert_or_load_day(&self.pool, world_id, generated).await?
            }
        };

        while cursor.game_day < target_game_day {
            let expected_game_day = cursor
                .game_day
                .checked_add(1)
                .context("market game day overflowed")?;
            let generated = generator.next_day(&world_state.world, &cursor)?;
            ensure!(
                generated.game_day == expected_game_day,
                "market generator skipped a game day"
            );
            cursor = insert_or_load_day(&self.pool, world_id, generated).await?;
        }

        ensure!(
            cursor.game_day == target_game_day,
            "market cache advanced beyond the requested game day"
        );
        Ok(cursor)
    }

    async fn history_for_user(&self, user_id: u64, limit: u32) -> Result<MarketHistoryState> {
        let limit = bounded_history_limit(limit)?;
        let mut tx = self.pool.begin().await?;
        let scope: Option<MarketHistoryScopeRow> = sqlx::query_as(
            "SELECT w.world_key, s.game_day AS through_game_day
             FROM save AS s
             INNER JOIN market_world AS w ON w.id = s.market_world_id
             WHERE s.user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await?;
        let scope = scope.context("authenticated user has no save")?;
        let from_day = history_start_day(scope.through_game_day, limit);

        let rows: Vec<MarketDayRow> = sqlx::query_as(
            "SELECT d.game_day, d.market_date, d.market_open, d.session_index, d.regime,
                    d.equity_close_krw, d.equity_return_ppm, d.equity_residual_ppm,
                    d.equity_variance_ppm2, d.policy_rate_bp, d.treasury_3m_bp,
                    d.treasury_1y_bp, d.treasury_3y_bp, d.treasury_10y_bp,
                    d.policy_rate_change_bp, d.equity_rate_shock_ppm,
                    d.cpi_index, d.cpi_remainder, d.llx_close_krw, d.llx_return_ppm,
                    d.llx_fee_remainder, d.llx_fee_accumulator_ppm,
                    d.gold_close_krw_per_gram, d.gold_prior_open_cpi_index,
                    d.gold_prior_open_treasury_10y_bp
             FROM save AS s
             INNER JOIN market_daily AS d
                 ON d.world_id = s.market_world_id
                AND d.game_day <= s.game_day
             WHERE s.user_id = ? AND d.game_day >= ?
             ORDER BY d.game_day
             LIMIT ?",
        )
        .bind(user_id)
        .bind(from_day)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;

        Ok(MarketHistoryState {
            world_key: scope.world_key,
            through_game_day: scope.through_game_day,
            days: rows
                .into_iter()
                .map(MarketDayRow::into_day)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

impl MySqlMarketStore {
    async fn world_lock(&self, world_id: u64) -> Arc<Mutex<()>> {
        let mut locks = self.world_locks.lock().await;
        Arc::clone(
            locks
                .entry(world_id)
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    }
}

#[derive(sqlx::FromRow)]
struct MarketWorldRow {
    id: u64,
    world_key: String,
    seed: u64,
    start_date: Date,
    day0_equity_close_krw: i64,
    calibration_version: String,
    calibration_parameters: String,
    index_product_version_id: Option<u64>,
    index_product_key: Option<String>,
    index_day0_close_krw: Option<i64>,
    index_annual_management_fee_ppm: Option<i64>,
    index_annual_distribution_rate_ppm: Option<i64>,
    index_day_count_denominator: Option<u32>,
    index_buy_fee_ppm: Option<i64>,
    index_sell_fee_ppm: Option<i64>,
    index_transaction_tax_ppm: Option<i64>,
}

impl MarketWorldRow {
    fn into_state(self) -> Result<MarketWorldState> {
        let parameters: MarketParameters = serde_json::from_str(&self.calibration_parameters)
            .context("market calibration JSON is invalid")?;
        let index_product = self.parse_index_product_terms()?;

        Ok(MarketWorldState {
            id: self.id,
            world: MarketWorld {
                key: self.world_key,
                seed: self.seed,
                start_date: self.start_date,
                day0_equity_close_krw: self.day0_equity_close_krw,
                index_product,
            },
            calibration: MarketCalibration {
                version: self.calibration_version,
                parameters,
            },
        })
    }

    fn parse_index_product_terms(&self) -> Result<Option<IndexProductTerms>> {
        let present = [
            self.index_product_version_id.is_some(),
            self.index_product_key.is_some(),
            self.index_day0_close_krw.is_some(),
            self.index_annual_management_fee_ppm.is_some(),
            self.index_annual_distribution_rate_ppm.is_some(),
            self.index_day_count_denominator.is_some(),
            self.index_buy_fee_ppm.is_some(),
            self.index_sell_fee_ppm.is_some(),
            self.index_transaction_tax_ppm.is_some(),
        ];
        if present.iter().all(|value| !value) {
            return Ok(None);
        }
        ensure!(
            present.iter().all(|value| *value),
            "market world index product terms are partially populated"
        );

        Ok(Some(IndexProductTerms {
            product_version_id: self
                .index_product_version_id
                .context("index product version id is missing")?,
            product_key: self
                .index_product_key
                .clone()
                .context("index product key is missing")?,
            day0_close_krw: self
                .index_day0_close_krw
                .context("index product day-zero close is missing")?,
            annual_management_fee_ppm: self
                .index_annual_management_fee_ppm
                .context("index product management fee is missing")?,
            annual_distribution_rate_ppm: self
                .index_annual_distribution_rate_ppm
                .context("index product distribution rate is missing")?,
            day_count_denominator: self
                .index_day_count_denominator
                .context("index product day-count denominator is missing")?,
            buy_fee_ppm: self
                .index_buy_fee_ppm
                .context("index product buy fee is missing")?,
            sell_fee_ppm: self
                .index_sell_fee_ppm
                .context("index product sell fee is missing")?,
            transaction_tax_ppm: self
                .index_transaction_tax_ppm
                .context("index product transaction tax is missing")?,
        }))
    }
}

#[derive(sqlx::FromRow)]
struct MarketDayRow {
    game_day: u32,
    market_date: Date,
    market_open: bool,
    session_index: u32,
    regime: String,
    equity_close_krw: i64,
    equity_return_ppm: i64,
    equity_residual_ppm: i64,
    equity_variance_ppm2: i64,
    policy_rate_bp: Option<i64>,
    treasury_3m_bp: Option<i64>,
    treasury_1y_bp: Option<i64>,
    treasury_3y_bp: Option<i64>,
    treasury_10y_bp: Option<i64>,
    policy_rate_change_bp: Option<i64>,
    equity_rate_shock_ppm: Option<i64>,
    cpi_index: Option<i64>,
    cpi_remainder: Option<i64>,
    llx_close_krw: Option<i64>,
    llx_return_ppm: Option<i64>,
    llx_fee_remainder: Option<i64>,
    llx_fee_accumulator_ppm: Option<i64>,
    gold_close_krw_per_gram: Option<i64>,
    gold_prior_open_cpi_index: Option<i64>,
    gold_prior_open_treasury_10y_bp: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct MarketHistoryScopeRow {
    world_key: String,
    through_game_day: u32,
}

impl MarketDayRow {
    fn into_day(self) -> Result<MarketDay> {
        Ok(MarketDay {
            game_day: self.game_day,
            market_date: self.market_date,
            market_open: self.market_open,
            session_index: self.session_index,
            regime: from_db_str(&self.regime)?,
            equity_close_krw: self.equity_close_krw,
            equity_return_ppm: self.equity_return_ppm,
            equity_variance_ppm2: self.equity_variance_ppm2,
            equity_residual_ppm: self.equity_residual_ppm,
            rates: NullableInterestRateState {
                policy_rate_bp: self.policy_rate_bp,
                treasury_3m_bp: self.treasury_3m_bp,
                treasury_1y_bp: self.treasury_1y_bp,
                treasury_3y_bp: self.treasury_3y_bp,
                treasury_10y_bp: self.treasury_10y_bp,
                policy_rate_change_bp: self.policy_rate_change_bp,
                equity_rate_shock_ppm: self.equity_rate_shock_ppm,
            }
            .into_complete()?,
            m2: NullableM2MarketState {
                cpi_index: self.cpi_index,
                cpi_remainder: self.cpi_remainder,
                llx_close_krw: self.llx_close_krw,
                llx_return_ppm: self.llx_return_ppm,
                llx_fee_remainder: self.llx_fee_remainder,
                llx_fee_accumulator_ppm: self.llx_fee_accumulator_ppm,
                gold_close_krw_per_gram: self.gold_close_krw_per_gram,
                gold_prior_open_cpi_index: self.gold_prior_open_cpi_index,
                gold_prior_open_treasury_10y_bp: self.gold_prior_open_treasury_10y_bp,
            }
            .into_complete()?,
        })
    }
}

async fn fetch_day(pool: &MySqlPool, world_id: u64, game_day: u32) -> Result<Option<MarketDay>> {
    let row: Option<MarketDayRow> = sqlx::query_as(
        "SELECT game_day, market_date, market_open, session_index, regime,
                equity_close_krw, equity_return_ppm, equity_residual_ppm,
                equity_variance_ppm2, policy_rate_bp, treasury_3m_bp,
                treasury_1y_bp, treasury_3y_bp, treasury_10y_bp,
                policy_rate_change_bp, equity_rate_shock_ppm,
                cpi_index, cpi_remainder, llx_close_krw, llx_return_ppm, llx_fee_remainder,
                llx_fee_accumulator_ppm, gold_close_krw_per_gram,
                gold_prior_open_cpi_index, gold_prior_open_treasury_10y_bp
         FROM market_daily
         WHERE world_id = ? AND game_day = ?",
    )
    .bind(world_id)
    .bind(game_day)
    .fetch_optional(pool)
    .await?;

    row.map(MarketDayRow::into_day).transpose()
}

async fn fetch_latest_day(
    pool: &MySqlPool,
    world_id: u64,
    through_game_day: u32,
) -> Result<Option<MarketDay>> {
    let row: Option<MarketDayRow> = sqlx::query_as(
        "SELECT game_day, market_date, market_open, session_index, regime,
                equity_close_krw, equity_return_ppm, equity_residual_ppm,
                equity_variance_ppm2, policy_rate_bp, treasury_3m_bp,
                treasury_1y_bp, treasury_3y_bp, treasury_10y_bp,
                policy_rate_change_bp, equity_rate_shock_ppm,
                cpi_index, cpi_remainder, llx_close_krw, llx_return_ppm, llx_fee_remainder,
                llx_fee_accumulator_ppm, gold_close_krw_per_gram,
                gold_prior_open_cpi_index, gold_prior_open_treasury_10y_bp
         FROM market_daily
         WHERE world_id = ? AND game_day <= ?
         ORDER BY game_day DESC
         LIMIT 1",
    )
    .bind(world_id)
    .bind(through_game_day)
    .fetch_optional(pool)
    .await?;

    row.map(MarketDayRow::into_day).transpose()
}

async fn insert_or_load_day(
    pool: &MySqlPool,
    world_id: u64,
    generated: MarketDay,
) -> Result<MarketDay> {
    let regime = to_db_str(&generated.regime)?;
    let policy_rate_bp = generated.rates.as_ref().map(|rates| rates.policy_rate_bp);
    let treasury_3m_bp = generated.rates.as_ref().map(|rates| rates.treasury_3m_bp);
    let treasury_1y_bp = generated.rates.as_ref().map(|rates| rates.treasury_1y_bp);
    let treasury_3y_bp = generated.rates.as_ref().map(|rates| rates.treasury_3y_bp);
    let treasury_10y_bp = generated.rates.as_ref().map(|rates| rates.treasury_10y_bp);
    let policy_rate_change_bp = generated
        .rates
        .as_ref()
        .map(|rates| rates.policy_rate_change_bp);
    let equity_rate_shock_ppm = generated
        .rates
        .as_ref()
        .map(|rates| rates.equity_rate_shock_ppm);
    let cpi_index = generated.m2.as_ref().map(|state| state.cpi_index);
    let cpi_remainder = generated.m2.as_ref().map(|state| state.cpi_remainder);
    let llx_close_krw = generated.m2.as_ref().map(|state| state.llx_close_krw);
    let llx_return_ppm = generated.m2.as_ref().map(|state| state.llx_return_ppm);
    let llx_fee_remainder = generated.m2.as_ref().map(|state| state.llx_fee_remainder);
    let llx_fee_accumulator_ppm = generated
        .m2
        .as_ref()
        .map(|state| state.llx_fee_accumulator_ppm);
    let gold_close_krw_per_gram = generated
        .m2
        .as_ref()
        .map(|state| state.gold_close_krw_per_gram);
    let gold_prior_open_cpi_index = generated
        .m2
        .as_ref()
        .map(|state| state.gold_prior_open_cpi_index);
    let gold_prior_open_treasury_10y_bp = generated
        .m2
        .as_ref()
        .map(|state| state.gold_prior_open_treasury_10y_bp);
    let result = sqlx::query(
        "INSERT INTO market_daily
             (world_id, game_day, market_date, market_open, session_index, regime,
              equity_close_krw, equity_return_ppm, equity_residual_ppm,
              equity_variance_ppm2, policy_rate_bp, treasury_3m_bp, treasury_1y_bp,
              treasury_3y_bp, treasury_10y_bp, policy_rate_change_bp,
              equity_rate_shock_ppm, cpi_index, cpi_remainder, llx_close_krw,
              llx_return_ppm, llx_fee_remainder, llx_fee_accumulator_ppm, gold_close_krw_per_gram,
              gold_prior_open_cpi_index, gold_prior_open_treasury_10y_bp)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(world_id)
    .bind(generated.game_day)
    .bind(generated.market_date)
    .bind(generated.market_open)
    .bind(generated.session_index)
    .bind(regime)
    .bind(generated.equity_close_krw)
    .bind(generated.equity_return_ppm)
    .bind(generated.equity_residual_ppm)
    .bind(generated.equity_variance_ppm2)
    .bind(policy_rate_bp)
    .bind(treasury_3m_bp)
    .bind(treasury_1y_bp)
    .bind(treasury_3y_bp)
    .bind(treasury_10y_bp)
    .bind(policy_rate_change_bp)
    .bind(equity_rate_shock_ppm)
    .bind(cpi_index)
    .bind(cpi_remainder)
    .bind(llx_close_krw)
    .bind(llx_return_ppm)
    .bind(llx_fee_remainder)
    .bind(llx_fee_accumulator_ppm)
    .bind(gold_close_krw_per_gram)
    .bind(gold_prior_open_cpi_index)
    .bind(gold_prior_open_treasury_10y_bp)
    .execute(pool)
    .await;

    match result {
        Ok(_) => Ok(generated),
        Err(error)
            if error
                .as_database_error()
                .is_some_and(sqlx::error::DatabaseError::is_unique_violation) =>
        {
            let stored = fetch_day(pool, world_id, generated.game_day)
                .await?
                .context("concurrent market insert won but its row is missing")?;
            ensure!(
                stored == generated,
                "deterministic market generation disagrees with the cached row"
            );
            Ok(stored)
        }
        Err(error) => Err(error.into()),
    }
}

fn bounded_history_limit(requested: u32) -> Result<u32> {
    if requested == 0 {
        bail!("market history limit must be positive");
    }

    Ok(requested.min(MAX_MARKET_HISTORY_ROWS))
}

const fn history_start_day(through_game_day: u32, limit: u32) -> u32 {
    through_game_day.saturating_add(1).saturating_sub(limit)
}

fn to_db_str<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value)? {
        Value::String(text) => Ok(text),
        other => bail!("value is not storable as a string: {other}"),
    }
}

fn from_db_str<T: DeserializeOwned>(raw: &str) -> Result<T> {
    serde_json::from_value(Value::String(raw.to_owned()))
        .with_context(|| format!("unknown market value stored: {raw}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::{MarketRegime, default_market_calibration, m2_market_calibration};
    use time::Month;

    mod context_a_market_world_row_is_loaded {
        use super::*;

        fn given_world_row() -> MarketWorldRow {
            let calibration = default_market_calibration();
            MarketWorldRow {
                id: 1,
                world_key: "m1-2026-v1".to_owned(),
                seed: 20_260_101,
                start_date: Date::from_calendar_date(2026, Month::January, 1)
                    .expect("테스트 날짜가 유효해야 한다"),
                day0_equity_close_krw: 100_000,
                calibration_version: calibration.version,
                calibration_parameters: serde_json::to_string(&calibration.parameters)
                    .expect("캘리브레이션을 직렬화할 수 있어야 한다"),
                index_product_version_id: None,
                index_product_key: None,
                index_day0_close_krw: None,
                index_annual_management_fee_ppm: None,
                index_annual_distribution_rate_ppm: None,
                index_day_count_denominator: None,
                index_buy_fee_ppm: None,
                index_sell_fee_ppm: None,
                index_transaction_tax_ppm: None,
            }
        }

        #[test]
        fn given_camel_case_json_when_converted_then_the_typed_calibration_is_restored() {
            let expected = default_market_calibration();
            let row = given_world_row();

            let loaded = row.into_state().expect("월드를 변환할 수 있어야 한다");

            assert_eq!(loaded.calibration, expected);
        }

        #[test]
        fn given_world_columns_when_converted_then_the_domain_world_is_restored() {
            let row = given_world_row();

            let loaded = row.into_state().expect("월드를 변환할 수 있어야 한다");

            assert_eq!(loaded.world.key, "m1-2026-v1");
        }

        #[test]
        fn given_a_published_index_bundle_when_converted_then_product_terms_are_restored() {
            let mut row = given_world_row();
            let calibration = m2_market_calibration();
            row.world_key = "m2-2026-v4".to_owned();
            row.calibration_version = calibration.version;
            row.calibration_parameters = serde_json::to_string(&calibration.parameters)
                .expect("캘리브레이션을 직렬화할 수 있어야 한다");
            row.index_product_version_id = Some(1);
            row.index_product_key = Some("llx-domestic-equity-2026-v1".to_owned());
            row.index_day0_close_krw = Some(100_000);
            row.index_annual_management_fee_ppm = Some(1_500);
            row.index_annual_distribution_rate_ppm = Some(20_000);
            row.index_day_count_denominator = Some(365);
            row.index_buy_fee_ppm = Some(0);
            row.index_sell_fee_ppm = Some(0);
            row.index_transaction_tax_ppm = Some(0);

            let loaded = row.into_state().expect("v4 월드를 변환할 수 있어야 한다");

            assert_eq!(
                loaded
                    .world
                    .index_product
                    .map(|product| product.annual_management_fee_ppm),
                Some(1_500)
            );
        }
    }

    mod context_a_market_regime_is_stored {
        use super::*;

        #[test]
        fn given_a_multiword_safe_enum_when_round_tripped_then_it_is_unchanged() {
            let regime = MarketRegime::Slowdown;

            let stored = to_db_str(&regime).expect("저장 표현으로 바꿀 수 있어야 한다");
            let restored: MarketRegime =
                from_db_str(&stored).expect("저장 표현을 읽을 수 있어야 한다");

            assert_eq!(restored, regime);
        }
    }

    mod context_a_market_day_row_is_loaded {
        use super::*;

        fn given_v4_market_day_row() -> MarketDayRow {
            MarketDayRow {
                game_day: 1,
                market_date: Date::from_calendar_date(2026, Month::January, 2)
                    .expect("테스트 날짜가 유효해야 한다"),
                market_open: true,
                session_index: 1,
                regime: "expansion".to_owned(),
                equity_close_krw: 100_905,
                equity_return_ppm: 9_051,
                equity_residual_ppm: 8_631,
                equity_variance_ppm2: 132_480_000,
                policy_rate_bp: Some(250),
                treasury_3m_bp: Some(255),
                treasury_1y_bp: Some(265),
                treasury_3y_bp: Some(280),
                treasury_10y_bp: Some(310),
                policy_rate_change_bp: Some(0),
                equity_rate_shock_ppm: Some(0),
                cpi_index: Some(1_000_054),
                cpi_remainder: Some(290_000_000),
                llx_close_krw: Some(100_905),
                llx_return_ppm: Some(9_047),
                llx_fee_remainder: Some(40),
                llx_fee_accumulator_ppm: Some(0),
                gold_close_krw_per_gram: Some(120_006),
                gold_prior_open_cpi_index: Some(1_000_054),
                gold_prior_open_treasury_10y_bp: Some(310),
            }
        }

        #[test]
        fn given_a_carried_residual_when_converted_then_garch_state_is_preserved() {
            let row = MarketDayRow {
                game_day: 3,
                market_date: Date::from_calendar_date(2026, Month::January, 4)
                    .expect("테스트 날짜가 유효해야 한다"),
                market_open: false,
                session_index: 1,
                regime: "expansion".to_owned(),
                equity_close_krw: 100_100,
                equity_return_ppm: 0,
                equity_residual_ppm: -12_345,
                equity_variance_ppm2: 144_000_000,
                policy_rate_bp: None,
                treasury_3m_bp: None,
                treasury_1y_bp: None,
                treasury_3y_bp: None,
                treasury_10y_bp: None,
                policy_rate_change_bp: None,
                equity_rate_shock_ppm: None,
                cpi_index: None,
                cpi_remainder: None,
                llx_close_krw: None,
                llx_return_ppm: None,
                llx_fee_remainder: None,
                llx_fee_accumulator_ppm: None,
                gold_close_krw_per_gram: None,
                gold_prior_open_cpi_index: None,
                gold_prior_open_treasury_10y_bp: None,
            };

            let loaded = row.into_day().expect("일봉을 변환할 수 있어야 한다");

            assert_eq!(loaded.equity_residual_ppm, -12_345);
        }

        #[test]
        fn given_all_v4_columns_when_converted_then_m2_state_is_restored() {
            let row = given_v4_market_day_row();

            let loaded = row.into_day().expect("v4 일봉을 변환할 수 있어야 한다");

            assert_eq!(loaded.m2.map(|state| state.llx_return_ppm), Some(9_047));
        }

        #[test]
        fn given_a_partial_v4_row_when_converted_then_corruption_is_rejected() {
            let mut row = given_v4_market_day_row();
            row.gold_prior_open_cpi_index = None;

            let loaded = row.into_day();

            assert!(loaded.is_err());
        }
    }

    mod context_a_history_limit_is_applied {
        use super::*;

        #[test]
        fn given_a_limit_above_the_cap_when_bounded_then_it_uses_the_cap() {
            let requested = MAX_MARKET_HISTORY_ROWS + 1;

            let bounded = bounded_history_limit(requested).expect("상한을 적용할 수 있어야 한다");

            assert_eq!(bounded, MAX_MARKET_HISTORY_ROWS);
        }

        #[test]
        fn given_a_zero_limit_when_bounded_then_it_is_rejected() {
            let bounded = bounded_history_limit(0);

            assert!(bounded.is_err());
        }

        #[test]
        fn given_a_cursor_shorter_than_the_limit_when_bounded_then_history_starts_at_zero() {
            let from_day = history_start_day(12, 365);

            assert_eq!(from_day, 0);
        }

        #[test]
        fn given_a_long_cursor_when_bounded_then_the_window_contains_the_requested_count() {
            let from_day = history_start_day(500, 365);

            assert_eq!(from_day, 136);
        }
    }
}
