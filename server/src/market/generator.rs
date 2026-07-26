use std::sync::Arc;

use time::{Date, Duration, Month};

use super::calendar::{KrxClosureCalendar, MarketCalendar, WeekendOnlyCalendar};
use super::entropy::{MarketEntropy, create_sha256_market_entropy};
use super::types::{
    CpiMarketParameters, EquityGarchParameters, GoldMarketParameters, InterestRateParameters,
    InterestRateState, M2MarketParameters, M2MarketState, MarketCalibration, MarketDay,
    MarketError, MarketParameters, MarketRegime, MarketRegimeParameters, MarketWorld,
    PROBABILITY_SCALE_PPM, PolicyRateTargets, RegimeParameters, RegimeProbabilities,
    YieldCurveMaturityParameters, YieldCurveParameters,
};

const RETURN_SCALE_PPM: i128 = 1_000_000;
const REGIME_STREAM: u32 = 0;
const EQUITY_INNOVATION_STREAM: u32 = 1;
const RATE_POLICY_STREAM: u32 = 2;
const GOLD_INNOVATION_STREAM: u32 = 3;
const INNOVATION_QUARTER_PPM: i64 = 250_000;
const MAX_ABS_INNOVATION_PPM: i64 = 8_000_000;

pub const DEFAULT_MARKET_CALIBRATION_VERSION: &str = "m1-2026-calibration-v1";
pub const DEFAULT_MARKET_WORLD_KEY: &str = "m1-2026-v1";
pub const KRX_MARKET_CALIBRATION_VERSION: &str = "m1-2026-calibration-v2";
pub const KRX_MARKET_WORLD_KEY: &str = "m1-2026-v2";
pub const RATE_MARKET_CALIBRATION_VERSION: &str = "m1-2026-calibration-v3";
pub const RATE_MARKET_WORLD_KEY: &str = "m1-2026-v3";
pub const M2_MARKET_CALIBRATION_VERSION: &str = "m2-2026-calibration-v4";
pub const M2_MARKET_WORLD_KEY: &str = "m2-2026-v4";
pub const DEFAULT_MARKET_WORLD_SEED: u64 = 20_260_101;
pub const DEFAULT_MARKET_WORLD_START_DATE: &str = "2026-01-01";
pub const DEFAULT_DAY0_EQUITY_CLOSE_KRW: i64 = 100_000;

pub trait MarketGenerator: Send + Sync {
    fn day_zero(&self, world: &MarketWorld) -> Result<MarketDay, MarketError>;

    fn next_day(&self, world: &MarketWorld, previous: &MarketDay)
    -> Result<MarketDay, MarketError>;

    fn generate_through(
        &self,
        world: &MarketWorld,
        previous: &MarketDay,
        target_game_day: u32,
    ) -> Result<Vec<MarketDay>, MarketError>;
}

struct DeterministicMarketGenerator {
    calibration: MarketCalibration,
    entropy: Arc<dyn MarketEntropy>,
    calendar: Arc<dyn MarketCalendar>,
}

pub struct MarketGeneratorRegistry {
    entropy: Arc<dyn MarketEntropy>,
    weekend_calendar: Arc<dyn MarketCalendar>,
    krx_calendar: Arc<dyn MarketCalendar>,
}

impl MarketGenerator for DeterministicMarketGenerator {
    fn day_zero(&self, world: &MarketWorld) -> Result<MarketDay, MarketError> {
        validate_world(&self.calibration.parameters, world)?;

        let market_open = self.calendar.is_market_open(world.start_date)?;
        let rates = self
            .calibration
            .parameters
            .interest_rates
            .as_ref()
            .map(initial_interest_rate_state)
            .transpose()?;
        let m2 = self
            .calibration
            .parameters
            .m2
            .as_ref()
            .map(|parameters| initial_m2_market_state(parameters, world, rates.as_ref()))
            .transpose()?;

        Ok(MarketDay {
            game_day: 0,
            market_date: world.start_date,
            market_open,
            session_index: 0,
            regime: self.calibration.parameters.initial_regime,
            equity_close_krw: world.day0_equity_close_krw,
            equity_return_ppm: 0,
            equity_variance_ppm2: self
                .calibration
                .parameters
                .equity_garch
                .initial_variance_ppm2,
            equity_residual_ppm: 0,
            rates,
            m2,
        })
    }

    fn next_day(
        &self,
        world: &MarketWorld,
        previous: &MarketDay,
    ) -> Result<MarketDay, MarketError> {
        validate_world(&self.calibration.parameters, world)?;
        validate_previous_day(&self.calibration.parameters, world, previous)?;

        let game_day = previous
            .game_day
            .checked_add(1)
            .ok_or(MarketError::ArithmeticOverflow("incrementing game day"))?;
        let market_date = date_for_game_day(world.start_date, game_day)?;
        if !self.calendar.is_market_open(market_date)? {
            let rates = previous.rates.as_ref().map(carried_interest_rate_state);
            let m2 = self
                .calibration
                .parameters
                .m2
                .as_ref()
                .map(|parameters| {
                    next_m2_market_state(
                        self.entropy.as_ref(),
                        parameters,
                        world,
                        previous,
                        false,
                        previous.session_index,
                        0,
                        rates.as_ref(),
                    )
                })
                .transpose()?;
            return Ok(MarketDay {
                game_day,
                market_date,
                market_open: false,
                session_index: previous.session_index,
                regime: previous.regime,
                equity_close_krw: previous.equity_close_krw,
                equity_return_ppm: 0,
                equity_variance_ppm2: previous.equity_variance_ppm2,
                equity_residual_ppm: previous.equity_residual_ppm,
                rates,
                m2,
            });
        }

        let session_index =
            previous
                .session_index
                .checked_add(1)
                .ok_or(MarketError::ArithmeticOverflow(
                    "incrementing session index",
                ))?;
        let regime = self.regime_for_session(world, previous.regime, game_day, session_index)?;
        let rates = self
            .calibration
            .parameters
            .interest_rates
            .as_ref()
            .map(|parameters| {
                next_interest_rate_state(
                    self.entropy.as_ref(),
                    parameters,
                    world,
                    previous,
                    game_day,
                    session_index,
                    regime,
                )
            })
            .transpose()?;
        let equity_rate_shock_ppm = rates
            .as_ref()
            .map_or(0, |rate_state| rate_state.equity_rate_shock_ppm);
        let variance = next_variance(&self.calibration.parameters, previous)?;
        let innovation_ppm = innovation_ppm(self.entropy.sample_u64(
            world.seed,
            game_day,
            EQUITY_INNOVATION_STREAM,
        ))?;
        let base_return_ppm = equity_return_ppm(
            self.calibration
                .parameters
                .regimes
                .for_regime(regime)
                .daily_drift_ppm,
            variance,
            innovation_ppm,
        )?;
        let return_ppm = apply_equity_rate_shock(base_return_ppm, equity_rate_shock_ppm)?;
        let equity_residual_ppm = return_ppm
            .checked_sub(
                self.calibration
                    .parameters
                    .regimes
                    .for_regime(regime)
                    .daily_drift_ppm,
            )
            .ok_or(MarketError::ArithmeticOverflow(
                "calculating the equity residual",
            ))?
            .checked_sub(equity_rate_shock_ppm)
            .ok_or(MarketError::ArithmeticOverflow(
                "removing the rate shock from the equity residual",
            ))?;
        let equity_close_krw = next_price(previous.equity_close_krw, return_ppm)?;
        let m2 = self
            .calibration
            .parameters
            .m2
            .as_ref()
            .map(|parameters| {
                next_m2_market_state(
                    self.entropy.as_ref(),
                    parameters,
                    world,
                    previous,
                    true,
                    session_index,
                    return_ppm,
                    rates.as_ref(),
                )
            })
            .transpose()?;

        Ok(MarketDay {
            game_day,
            market_date,
            market_open: true,
            session_index,
            regime,
            equity_close_krw,
            equity_return_ppm: return_ppm,
            equity_variance_ppm2: variance,
            equity_residual_ppm,
            rates,
            m2,
        })
    }

    fn generate_through(
        &self,
        world: &MarketWorld,
        previous: &MarketDay,
        target_game_day: u32,
    ) -> Result<Vec<MarketDay>, MarketError> {
        if target_game_day < previous.game_day {
            return Err(MarketError::InvalidPreviousDay(
                "target game day precedes the supplied day",
            ));
        }

        let capacity = target_game_day.saturating_sub(previous.game_day) as usize;
        let mut generated = Vec::with_capacity(capacity);
        let mut cursor = previous.clone();
        while cursor.game_day < target_game_day {
            cursor = self.next_day(world, &cursor)?;
            generated.push(cursor.clone());
        }

        Ok(generated)
    }
}

impl DeterministicMarketGenerator {
    fn regime_for_session(
        &self,
        world: &MarketWorld,
        previous: MarketRegime,
        game_day: u32,
        session_index: u32,
    ) -> Result<MarketRegime, MarketError> {
        let transition_sessions = self.calibration.parameters.sessions_per_regime_transition;
        if !session_index.is_multiple_of(transition_sessions) {
            return Ok(previous);
        }

        let draw = probability_draw(self.entropy.sample_u64(world.seed, game_day, REGIME_STREAM));
        choose_regime(
            &self
                .calibration
                .parameters
                .regimes
                .for_regime(previous)
                .transition_ppm,
            draw,
        )
    }
}

impl MarketGeneratorRegistry {
    pub fn generator_for(
        &self,
        calibration: &MarketCalibration,
    ) -> Result<Arc<dyn MarketGenerator>, MarketError> {
        let calendar = match calibration.version.as_str() {
            DEFAULT_MARKET_CALIBRATION_VERSION => Arc::clone(&self.weekend_calendar),
            KRX_MARKET_CALIBRATION_VERSION
            | RATE_MARKET_CALIBRATION_VERSION
            | M2_MARKET_CALIBRATION_VERSION => Arc::clone(&self.krx_calendar),
            _ => {
                return Err(MarketError::InvalidCalibration(
                    "generator version is not registered",
                ));
            }
        };

        create_generator(calibration.clone(), Arc::clone(&self.entropy), calendar)
    }
}

pub fn create_market_generator_registry() -> Result<MarketGeneratorRegistry, MarketError> {
    create_market_generator_registry_with_entropy(Arc::new(create_sha256_market_entropy()))
}

pub fn create_market_generator_registry_with_entropy(
    entropy: Arc<dyn MarketEntropy>,
) -> Result<MarketGeneratorRegistry, MarketError> {
    Ok(MarketGeneratorRegistry {
        entropy,
        weekend_calendar: Arc::new(WeekendOnlyCalendar),
        krx_calendar: Arc::new(KrxClosureCalendar::from_embedded_snapshot()?),
    })
}

pub fn create_market_generator(
    calibration: MarketCalibration,
    entropy: Arc<dyn MarketEntropy>,
) -> Result<Arc<dyn MarketGenerator>, MarketError> {
    create_market_generator_registry_with_entropy(entropy)?.generator_for(&calibration)
}

fn create_generator(
    calibration: MarketCalibration,
    entropy: Arc<dyn MarketEntropy>,
    calendar: Arc<dyn MarketCalendar>,
) -> Result<Arc<dyn MarketGenerator>, MarketError> {
    validate_calibration(&calibration)?;

    Ok(Arc::new(DeterministicMarketGenerator {
        calibration,
        entropy,
        calendar,
    }))
}

pub fn create_default_market_generator() -> Result<Arc<dyn MarketGenerator>, MarketError> {
    create_generator(
        default_market_calibration(),
        Arc::new(create_sha256_market_entropy()),
        Arc::new(WeekendOnlyCalendar),
    )
}

pub fn default_market_world() -> Result<MarketWorld, MarketError> {
    let start_date = Date::from_calendar_date(2026, Month::January, 1)
        .map_err(|_| MarketError::DateOutOfRange)?;

    Ok(MarketWorld {
        key: DEFAULT_MARKET_WORLD_KEY.to_owned(),
        seed: DEFAULT_MARKET_WORLD_SEED,
        start_date,
        day0_equity_close_krw: DEFAULT_DAY0_EQUITY_CLOSE_KRW,
        index_product: None,
    })
}

pub fn default_market_calibration() -> MarketCalibration {
    MarketCalibration {
        version: DEFAULT_MARKET_CALIBRATION_VERSION.to_owned(),
        parameters: MarketParameters {
            initial_regime: MarketRegime::Expansion,
            sessions_per_regime_transition: 21,
            regimes: MarketRegimeParameters {
                expansion: RegimeParameters {
                    daily_drift_ppm: 420,
                    transition_ppm: RegimeProbabilities {
                        expansion: 870_000,
                        slowdown: 110_000,
                        recession: 10_000,
                        recovery: 10_000,
                    },
                },
                slowdown: RegimeParameters {
                    daily_drift_ppm: 20,
                    transition_ppm: RegimeProbabilities {
                        expansion: 20_000,
                        slowdown: 870_000,
                        recession: 90_000,
                        recovery: 20_000,
                    },
                },
                recession: RegimeParameters {
                    daily_drift_ppm: -630,
                    transition_ppm: RegimeProbabilities {
                        expansion: 5_000,
                        slowdown: 20_000,
                        recession: 870_000,
                        recovery: 105_000,
                    },
                },
                recovery: RegimeParameters {
                    daily_drift_ppm: 620,
                    transition_ppm: RegimeProbabilities {
                        expansion: 105_000,
                        slowdown: 10_000,
                        recession: 15_000,
                        recovery: 870_000,
                    },
                },
            },
            equity_garch: EquityGarchParameters {
                initial_variance_ppm2: 144_000_000,
                omega_ppm2: 720_000,
                alpha_ppm: 80_000,
                beta_ppm: 915_000,
                min_variance_ppm2: 16_000_000,
                max_variance_ppm2: 2_500_000_000,
            },
            interest_rates: None,
            m2: None,
        },
    }
}

pub fn krx_market_calibration() -> MarketCalibration {
    let mut calibration = default_market_calibration();
    calibration.version = KRX_MARKET_CALIBRATION_VERSION.to_owned();
    calibration
}

pub fn krx_market_world() -> Result<MarketWorld, MarketError> {
    let mut world = default_market_world()?;
    world.key = KRX_MARKET_WORLD_KEY.to_owned();
    Ok(world)
}

pub fn rate_market_calibration() -> MarketCalibration {
    let mut calibration = krx_market_calibration();
    calibration.version = RATE_MARKET_CALIBRATION_VERSION.to_owned();
    calibration.parameters.interest_rates = Some(InterestRateParameters {
        initial_policy_rate_bp: 250,
        neutral_policy_rate_bp: 250,
        update_interval_sessions: 21,
        mean_reversion_ppm: 250_000,
        innovation_scale_bp: 25,
        quantization_step_bp: 25,
        min_policy_rate_bp: 0,
        max_policy_rate_bp: 800,
        targets: PolicyRateTargets {
            expansion: 350,
            slowdown: 250,
            recession: 100,
            recovery: 200,
        },
        yield_curve: YieldCurveParameters {
            treasury_3m: YieldCurveMaturityParameters {
                policy_weight_ppm: 900_000,
                neutral_weight_ppm: 100_000,
                term_premium_bp: 5,
            },
            treasury_1y: YieldCurveMaturityParameters {
                policy_weight_ppm: 750_000,
                neutral_weight_ppm: 250_000,
                term_premium_bp: 15,
            },
            treasury_3y: YieldCurveMaturityParameters {
                policy_weight_ppm: 500_000,
                neutral_weight_ppm: 500_000,
                term_premium_bp: 30,
            },
            treasury_10y: YieldCurveMaturityParameters {
                policy_weight_ppm: 250_000,
                neutral_weight_ppm: 750_000,
                term_premium_bp: 60,
            },
        },
        max_yield_bp: 1_500,
        equity_shock_ppm_per_policy_bp: 300,
    });
    calibration
}

pub fn rate_market_world() -> Result<MarketWorld, MarketError> {
    let mut world = krx_market_world()?;
    world.key = RATE_MARKET_WORLD_KEY.to_owned();
    Ok(world)
}

pub fn m2_market_calibration() -> MarketCalibration {
    let mut calibration = rate_market_calibration();
    calibration.version = M2_MARKET_CALIBRATION_VERSION.to_owned();
    calibration.parameters.m2 = Some(M2MarketParameters {
        cpi: CpiMarketParameters {
            day0_index: 1_000_000,
            annual_rate_ppm: 20_000,
            day_count_denominator: 365,
        },
        gold: GoldMarketParameters {
            day0_close_krw_per_gram: 120_000,
            innovation_scale_ppm: 11_000,
            treasury_10y_sensitivity_ppm_per_bp: -250,
        },
    });
    calibration
}

pub fn m2_market_world(
    index_product: super::types::IndexProductTerms,
) -> Result<MarketWorld, MarketError> {
    let mut world = rate_market_world()?;
    world.key = M2_MARKET_WORLD_KEY.to_owned();
    world.index_product = Some(index_product);
    Ok(world)
}

fn validate_calibration(calibration: &MarketCalibration) -> Result<(), MarketError> {
    if calibration.version.trim().is_empty() {
        return Err(MarketError::InvalidCalibration("version is empty"));
    }
    let parameters = &calibration.parameters;
    match (
        calibration.version.as_str(),
        parameters.interest_rates.as_ref(),
        parameters.m2.as_ref(),
    ) {
        (DEFAULT_MARKET_CALIBRATION_VERSION | KRX_MARKET_CALIBRATION_VERSION, None, None) => {}
        (RATE_MARKET_CALIBRATION_VERSION, Some(interest_rates), None) => {
            validate_interest_rate_parameters(interest_rates)?;
        }
        (M2_MARKET_CALIBRATION_VERSION, Some(interest_rates), Some(m2)) => {
            validate_interest_rate_parameters(interest_rates)?;
            validate_m2_market_parameters(m2)?;
        }
        (DEFAULT_MARKET_CALIBRATION_VERSION | KRX_MARKET_CALIBRATION_VERSION, _, _) => {
            return Err(MarketError::InvalidCalibration(
                "legacy generator versions cannot define rate or M2 parameters",
            ));
        }
        (RATE_MARKET_CALIBRATION_VERSION, None, _) => {
            return Err(MarketError::InvalidCalibration(
                "rate generator version requires interest-rate parameters",
            ));
        }
        (RATE_MARKET_CALIBRATION_VERSION, Some(_), Some(_)) => {
            return Err(MarketError::InvalidCalibration(
                "v3 generator cannot define M2 market parameters",
            ));
        }
        (M2_MARKET_CALIBRATION_VERSION, _, _) => {
            return Err(MarketError::InvalidCalibration(
                "v4 generator requires interest-rate and M2 market parameters",
            ));
        }
        _ => {
            return Err(MarketError::InvalidCalibration(
                "generator version is not registered",
            ));
        }
    }
    if parameters.sessions_per_regime_transition == 0 {
        return Err(MarketError::InvalidCalibration(
            "transition session count must be positive",
        ));
    }

    let maximum_rate_shock_ppm = parameters
        .interest_rates
        .as_ref()
        .map(maximum_equity_rate_shock_ppm)
        .transpose()?
        .unwrap_or(0);
    for regime in [
        MarketRegime::Expansion,
        MarketRegime::Slowdown,
        MarketRegime::Recession,
        MarketRegime::Recovery,
    ] {
        let regime_parameters = parameters.regimes.for_regime(regime);
        if regime_parameters.transition_ppm.checked_total() != Some(PROBABILITY_SCALE_PPM) {
            return Err(MarketError::InvalidCalibration(
                "each transition row must total one million ppm",
            ));
        }
    }

    let garch = &parameters.equity_garch;
    if garch.initial_variance_ppm2 <= 0
        || garch.omega_ppm2 <= 0
        || garch.min_variance_ppm2 <= 0
        || garch.max_variance_ppm2 < garch.min_variance_ppm2
        || !(garch.min_variance_ppm2..=garch.max_variance_ppm2)
            .contains(&garch.initial_variance_ppm2)
    {
        return Err(MarketError::InvalidCalibration(
            "GARCH variances must form a positive ordered range",
        ));
    }
    let persistence =
        garch
            .alpha_ppm
            .checked_add(garch.beta_ppm)
            .ok_or(MarketError::InvalidCalibration(
                "GARCH coefficient sum overflowed",
            ))?;
    if persistence >= PROBABILITY_SCALE_PPM {
        return Err(MarketError::InvalidCalibration(
            "GARCH alpha plus beta must be below one million ppm",
        ));
    }

    let max_standard_deviation = integer_sqrt(garch.max_variance_ppm2)?;
    let max_shock = checked_scaled_product(
        max_standard_deviation,
        MAX_ABS_INNOVATION_PPM,
        "validating maximum equity shock",
    )?;
    for regime in [
        MarketRegime::Expansion,
        MarketRegime::Slowdown,
        MarketRegime::Recession,
        MarketRegime::Recovery,
    ] {
        let drift = parameters.regimes.for_regime(regime).daily_drift_ppm;
        if drift
            .checked_sub(max_shock)
            .ok_or(MarketError::InvalidCalibration(
                "minimum possible return overflowed",
            ))?
            .checked_sub(maximum_rate_shock_ppm)
            .ok_or(MarketError::InvalidCalibration(
                "minimum rate-adjusted return overflowed",
            ))?
            <= -1_000_000
        {
            return Err(MarketError::InvalidCalibration(
                "calibration can produce a non-positive price",
            ));
        }
    }

    Ok(())
}

fn validate_interest_rate_parameters(
    parameters: &InterestRateParameters,
) -> Result<(), MarketError> {
    if parameters.update_interval_sessions == 0 {
        return Err(MarketError::InvalidCalibration(
            "policy-rate update interval must be positive",
        ));
    }
    if parameters.mean_reversion_ppm > PROBABILITY_SCALE_PPM {
        return Err(MarketError::InvalidCalibration(
            "policy-rate mean reversion must not exceed one million ppm",
        ));
    }
    if parameters.innovation_scale_bp <= 0 || parameters.quantization_step_bp <= 0 {
        return Err(MarketError::InvalidCalibration(
            "policy-rate innovation and quantization must be positive",
        ));
    }
    if parameters.min_policy_rate_bp < 0
        || parameters.max_policy_rate_bp < parameters.min_policy_rate_bp
        || parameters.max_yield_bp != 1_500
        || parameters.equity_shock_ppm_per_policy_bp <= 0
    {
        return Err(MarketError::InvalidCalibration(
            "interest-rate bounds or equity shock coefficient are invalid",
        ));
    }

    let policy_range = parameters.min_policy_rate_bp..=parameters.max_policy_rate_bp;
    for rate in [
        parameters.initial_policy_rate_bp,
        parameters.neutral_policy_rate_bp,
        parameters.targets.expansion,
        parameters.targets.slowdown,
        parameters.targets.recession,
        parameters.targets.recovery,
    ] {
        if !policy_range.contains(&rate) {
            return Err(MarketError::InvalidCalibration(
                "policy-rate anchor or target is outside its bounds",
            ));
        }
    }
    for quantized_rate in [
        parameters.initial_policy_rate_bp,
        parameters.min_policy_rate_bp,
        parameters.max_policy_rate_bp,
    ] {
        if quantized_rate.rem_euclid(parameters.quantization_step_bp) != 0 {
            return Err(MarketError::InvalidCalibration(
                "policy-rate anchors and bounds must align to the quantization step",
            ));
        }
    }

    for maturity in [
        &parameters.yield_curve.treasury_3m,
        &parameters.yield_curve.treasury_1y,
        &parameters.yield_curve.treasury_3y,
        &parameters.yield_curve.treasury_10y,
    ] {
        if maturity
            .policy_weight_ppm
            .checked_add(maturity.neutral_weight_ppm)
            != Some(PROBABILITY_SCALE_PPM)
            || !(0..=parameters.max_yield_bp).contains(&maturity.term_premium_bp)
        {
            return Err(MarketError::InvalidCalibration(
                "yield-curve weights or term premium are invalid",
            ));
        }
    }

    yield_curve_for(parameters, parameters.initial_policy_rate_bp)?;
    maximum_equity_rate_shock_ppm(parameters)?;
    Ok(())
}

fn validate_m2_market_parameters(parameters: &M2MarketParameters) -> Result<(), MarketError> {
    if parameters.cpi.day0_index <= 0
        || parameters.cpi.annual_rate_ppm < 0
        || parameters.cpi.day_count_denominator != 365
    {
        return Err(MarketError::InvalidCalibration(
            "CPI anchor, rate, or Actual/365 denominator is invalid",
        ));
    }
    if parameters.gold.day0_close_krw_per_gram <= 0
        || parameters.gold.innovation_scale_ppm <= 0
        || parameters.gold.treasury_10y_sensitivity_ppm_per_bp >= 0
    {
        return Err(MarketError::InvalidCalibration(
            "gold anchor, innovation, or rate sensitivity is invalid",
        ));
    }

    Ok(())
}

fn maximum_equity_rate_shock_ppm(parameters: &InterestRateParameters) -> Result<i64, MarketError> {
    let range = i128::from(parameters.max_policy_rate_bp)
        .checked_sub(i128::from(parameters.min_policy_rate_bp))
        .ok_or(MarketError::ArithmeticOverflow(
            "calculating the policy-rate range",
        ))?;
    let shock = range
        .checked_mul(i128::from(parameters.equity_shock_ppm_per_policy_bp))
        .ok_or(MarketError::ArithmeticOverflow(
            "validating the maximum equity rate shock",
        ))?;
    i64::try_from(shock)
        .map_err(|_| MarketError::ArithmeticOverflow("converting the maximum equity rate shock"))
}

fn validate_world(parameters: &MarketParameters, world: &MarketWorld) -> Result<(), MarketError> {
    if world.key.trim().is_empty() {
        return Err(MarketError::InvalidWorld("world key is empty"));
    }
    if world.day0_equity_close_krw <= 0 {
        return Err(MarketError::InvalidWorld(
            "day zero equity price must be positive",
        ));
    }

    match (&parameters.m2, &world.index_product) {
        (None, None) => {}
        (Some(_), Some(product)) => {
            if product.product_version_id == 0
                || product.product_key.trim().is_empty()
                || product.day0_close_krw <= 0
                || product.annual_management_fee_ppm < 0
                || product.annual_distribution_rate_ppm < 0
                || product.day_count_denominator != 365
                || product.buy_fee_ppm < 0
                || product.sell_fee_ppm < 0
                || product.transaction_tax_ppm < 0
            {
                return Err(MarketError::InvalidWorld(
                    "the bundled index product terms are invalid",
                ));
            }
        }
        (None, Some(_)) => {
            return Err(MarketError::InvalidWorld(
                "legacy world unexpectedly contains an index product",
            ));
        }
        (Some(_), None) => {
            return Err(MarketError::InvalidWorld(
                "v4 world is missing its bundled index product",
            ));
        }
    }

    Ok(())
}

fn validate_previous_day(
    parameters: &MarketParameters,
    world: &MarketWorld,
    previous: &MarketDay,
) -> Result<(), MarketError> {
    let expected_date = date_for_game_day(world.start_date, previous.game_day)?;
    if expected_date != previous.market_date {
        return Err(MarketError::InvalidPreviousDay(
            "date does not match the world game day",
        ));
    }
    if previous.equity_close_krw <= 0 {
        return Err(MarketError::NonPositivePrice);
    }
    if previous.equity_variance_ppm2 <= 0 {
        return Err(MarketError::InvalidPreviousDay(
            "equity variance must be positive",
        ));
    }

    match (&parameters.interest_rates, &previous.rates) {
        (None, None) => {}
        (Some(rate_parameters), Some(rates)) => {
            validate_interest_rate_state(rate_parameters, rates)?;
        }
        (None, Some(_)) => {
            return Err(MarketError::InvalidPreviousDay(
                "legacy market day unexpectedly contains a rate factor",
            ));
        }
        (Some(_), None) => {
            return Err(MarketError::InvalidPreviousDay(
                "rate-enabled market day is missing its rate factor",
            ));
        }
    }

    match (&parameters.m2, &previous.m2) {
        (None, None) => {}
        (Some(m2_parameters), Some(m2)) => {
            let product = world
                .index_product
                .as_ref()
                .ok_or(MarketError::InvalidWorld(
                    "v4 world is missing its bundled index product",
                ))?;
            validate_m2_market_state(m2_parameters, product, m2)?;
            if !previous.market_open && m2.llx_return_ppm != 0 {
                return Err(MarketError::InvalidM2State(
                    "closed v4 day has a nonzero LLX return",
                ));
            }
        }
        (None, Some(_)) => {
            return Err(MarketError::InvalidPreviousDay(
                "legacy market day unexpectedly contains M2 factors",
            ));
        }
        (Some(_), None) => {
            return Err(MarketError::InvalidPreviousDay(
                "v4 market day is missing its M2 factors",
            ));
        }
    }

    Ok(())
}

fn validate_m2_market_state(
    parameters: &M2MarketParameters,
    product: &super::types::IndexProductTerms,
    state: &M2MarketState,
) -> Result<(), MarketError> {
    let cpi_remainder_limit = i128::from(parameters.cpi.day_count_denominator)
        .checked_mul(RETURN_SCALE_PPM)
        .ok_or(MarketError::ArithmeticOverflow(
            "validating the CPI remainder limit",
        ))?;
    if state.cpi_index <= 0
        || state.cpi_remainder < 0
        || i128::from(state.cpi_remainder) >= cpi_remainder_limit
    {
        return Err(MarketError::InvalidM2State(
            "CPI index or remainder is outside its bounds",
        ));
    }
    if state.llx_close_krw <= 0
        || state.llx_fee_remainder < 0
        || state.llx_fee_remainder >= i64::from(product.day_count_denominator)
        || state.llx_fee_accumulator_ppm < 0
    {
        return Err(MarketError::InvalidM2State(
            "LLX close or fee state is outside its bounds",
        ));
    }
    if state.gold_close_krw_per_gram <= 0
        || state.gold_prior_open_cpi_index <= 0
        || state.gold_prior_open_cpi_index > state.cpi_index
        || !(0..=1_500).contains(&state.gold_prior_open_treasury_10y_bp)
    {
        return Err(MarketError::InvalidM2State(
            "gold close or prior-open state is outside its bounds",
        ));
    }

    Ok(())
}

fn initial_m2_market_state(
    parameters: &M2MarketParameters,
    world: &MarketWorld,
    rates: Option<&InterestRateState>,
) -> Result<M2MarketState, MarketError> {
    let rates = rates.ok_or(MarketError::InvalidCalibration(
        "v4 generator requires the interest-rate factor",
    ))?;
    let product = world
        .index_product
        .as_ref()
        .ok_or(MarketError::InvalidWorld(
            "v4 world is missing its bundled index product",
        ))?;
    Ok(M2MarketState {
        cpi_index: parameters.cpi.day0_index,
        cpi_remainder: 0,
        llx_close_krw: product.day0_close_krw,
        llx_return_ppm: 0,
        llx_fee_remainder: 0,
        llx_fee_accumulator_ppm: 0,
        gold_close_krw_per_gram: parameters.gold.day0_close_krw_per_gram,
        gold_prior_open_cpi_index: parameters.cpi.day0_index,
        gold_prior_open_treasury_10y_bp: rates.treasury_10y_bp,
    })
}

#[allow(clippy::too_many_arguments)]
fn next_m2_market_state(
    entropy: &dyn MarketEntropy,
    parameters: &M2MarketParameters,
    world: &MarketWorld,
    previous: &MarketDay,
    market_open: bool,
    session_index: u32,
    benchmark_return_ppm: i64,
    rates: Option<&InterestRateState>,
) -> Result<M2MarketState, MarketError> {
    let previous_m2 = previous.m2.as_ref().ok_or(MarketError::InvalidPreviousDay(
        "v4 market day is missing its M2 factors",
    ))?;
    let (cpi_index, cpi_remainder) = next_cpi_state(&parameters.cpi, previous_m2)?;
    let product = world
        .index_product
        .as_ref()
        .ok_or(MarketError::InvalidWorld(
            "v4 world is missing its bundled index product",
        ))?;
    let (llx_fee_remainder, accrued_fee_ppm) = next_llx_fee_state(product, previous_m2)?;
    let llx_fee_accumulator_ppm = previous_m2
        .llx_fee_accumulator_ppm
        .checked_add(accrued_fee_ppm)
        .ok_or(MarketError::ArithmeticOverflow(
            "accumulating the LLX management fee",
        ))?;

    if !market_open {
        return Ok(M2MarketState {
            cpi_index,
            cpi_remainder,
            llx_close_krw: previous_m2.llx_close_krw,
            llx_return_ppm: 0,
            llx_fee_remainder,
            llx_fee_accumulator_ppm,
            gold_close_krw_per_gram: previous_m2.gold_close_krw_per_gram,
            gold_prior_open_cpi_index: previous_m2.gold_prior_open_cpi_index,
            gold_prior_open_treasury_10y_bp: previous_m2.gold_prior_open_treasury_10y_bp,
        });
    }

    let llx_return_ppm = benchmark_return_ppm
        .checked_sub(llx_fee_accumulator_ppm)
        .ok_or(MarketError::ArithmeticOverflow(
            "subtracting accumulated LLX management fees",
        ))?;
    let llx_close_krw = next_price(previous_m2.llx_close_krw, llx_return_ppm)?;
    let rates = rates.ok_or(MarketError::InvalidM2State(
        "open v4 day is missing the interest-rate factor",
    ))?;
    let cpi_return_ppm = cpi_return_ppm(cpi_index, previous_m2.gold_prior_open_cpi_index)?;
    let treasury_10y_change_bp = rates
        .treasury_10y_bp
        .checked_sub(previous_m2.gold_prior_open_treasury_10y_bp)
        .ok_or(MarketError::ArithmeticOverflow(
            "calculating the ten-year yield change for gold",
        ))?;
    let rate_adjustment_ppm = gold_rate_return_adjustment_ppm(
        treasury_10y_change_bp,
        parameters.gold.treasury_10y_sensitivity_ppm_per_bp,
    )?;
    let normalized_innovation =
        innovation_ppm(entropy.sample_u64(world.seed, session_index, GOLD_INNOVATION_STREAM))?;
    let gold_innovation_ppm = checked_scaled_product(
        parameters.gold.innovation_scale_ppm,
        normalized_innovation,
        "scaling the gold innovation",
    )?;
    let gold_return_ppm = i128::from(cpi_return_ppm)
        .checked_add(i128::from(rate_adjustment_ppm))
        .and_then(|value| value.checked_add(i128::from(gold_innovation_ppm)))
        .ok_or(MarketError::ArithmeticOverflow("forming the gold return"))?;
    let gold_return_ppm = i64::try_from(gold_return_ppm)
        .map_err(|_| MarketError::ArithmeticOverflow("converting the gold return"))?;
    let gold_close_krw_per_gram = next_price(previous_m2.gold_close_krw_per_gram, gold_return_ppm)?;

    Ok(M2MarketState {
        cpi_index,
        cpi_remainder,
        llx_close_krw,
        llx_return_ppm,
        llx_fee_remainder,
        llx_fee_accumulator_ppm: 0,
        gold_close_krw_per_gram,
        gold_prior_open_cpi_index: cpi_index,
        gold_prior_open_treasury_10y_bp: rates.treasury_10y_bp,
    })
}

fn next_cpi_state(
    parameters: &CpiMarketParameters,
    previous: &M2MarketState,
) -> Result<(i64, i64), MarketError> {
    let denominator = i128::from(parameters.day_count_denominator)
        .checked_mul(RETURN_SCALE_PPM)
        .ok_or(MarketError::ArithmeticOverflow(
            "forming the CPI day-count denominator",
        ))?;
    let numerator = i128::from(previous.cpi_index)
        .checked_mul(i128::from(parameters.annual_rate_ppm))
        .and_then(|value| value.checked_add(i128::from(previous.cpi_remainder)))
        .ok_or(MarketError::ArithmeticOverflow(
            "forming the daily CPI numerator",
        ))?;
    let increase = numerator
        .checked_div(denominator)
        .ok_or(MarketError::ArithmeticOverflow(
            "calculating the daily CPI increase",
        ))?;
    let remainder = numerator
        .checked_rem(denominator)
        .ok_or(MarketError::ArithmeticOverflow(
            "calculating the daily CPI remainder",
        ))?;
    let index = i128::from(previous.cpi_index)
        .checked_add(increase)
        .ok_or(MarketError::ArithmeticOverflow("increasing the CPI index"))?;

    Ok((
        i64::try_from(index)
            .map_err(|_| MarketError::ArithmeticOverflow("converting the CPI index"))?,
        i64::try_from(remainder)
            .map_err(|_| MarketError::ArithmeticOverflow("converting the CPI remainder"))?,
    ))
}

fn next_llx_fee_state(
    product: &super::types::IndexProductTerms,
    previous: &M2MarketState,
) -> Result<(i64, i64), MarketError> {
    let numerator = i128::from(product.annual_management_fee_ppm)
        .checked_add(i128::from(previous.llx_fee_remainder))
        .ok_or(MarketError::ArithmeticOverflow(
            "forming the daily LLX fee numerator",
        ))?;
    let denominator = i128::from(product.day_count_denominator);
    let daily_fee_ppm =
        numerator
            .checked_div(denominator)
            .ok_or(MarketError::ArithmeticOverflow(
                "calculating the daily LLX fee",
            ))?;
    let remainder = numerator
        .checked_rem(denominator)
        .ok_or(MarketError::ArithmeticOverflow(
            "calculating the LLX fee remainder",
        ))?;

    Ok((
        i64::try_from(remainder)
            .map_err(|_| MarketError::ArithmeticOverflow("converting the LLX fee remainder"))?,
        i64::try_from(daily_fee_ppm)
            .map_err(|_| MarketError::ArithmeticOverflow("converting the daily LLX fee"))?,
    ))
}

fn cpi_return_ppm(current_index: i64, prior_open_index: i64) -> Result<i64, MarketError> {
    let difference = i128::from(current_index)
        .checked_sub(i128::from(prior_open_index))
        .ok_or(MarketError::ArithmeticOverflow(
            "calculating the CPI change for gold",
        ))?;
    if difference < 0 || prior_open_index <= 0 {
        return Err(MarketError::InvalidM2State(
            "gold prior-open CPI index is invalid",
        ));
    }
    let numerator =
        difference
            .checked_mul(RETURN_SCALE_PPM)
            .ok_or(MarketError::ArithmeticOverflow(
                "scaling the CPI return for gold",
            ))?;
    let rounded = round_half_up_nonnegative(numerator, i128::from(prior_open_index))?;
    i64::try_from(rounded)
        .map_err(|_| MarketError::ArithmeticOverflow("converting the CPI return for gold"))
}

pub(super) fn gold_rate_return_adjustment_ppm(
    treasury_10y_change_bp: i64,
    sensitivity_ppm_per_bp: i64,
) -> Result<i64, MarketError> {
    let adjustment = i128::from(treasury_10y_change_bp)
        .checked_mul(i128::from(sensitivity_ppm_per_bp))
        .ok_or(MarketError::ArithmeticOverflow(
            "calculating the gold rate adjustment",
        ))?;
    i64::try_from(adjustment)
        .map_err(|_| MarketError::ArithmeticOverflow("converting the gold rate adjustment"))
}

fn round_half_up_nonnegative(numerator: i128, denominator: i128) -> Result<i128, MarketError> {
    if numerator < 0 || denominator <= 0 {
        return Err(MarketError::InvalidM2State(
            "round-half-up inputs must be non-negative over a positive denominator",
        ));
    }
    numerator
        .checked_add(denominator / 2)
        .and_then(|value| value.checked_div(denominator))
        .ok_or(MarketError::ArithmeticOverflow(
            "rounding a fixed-point market value",
        ))
}

fn validate_interest_rate_state(
    parameters: &InterestRateParameters,
    rates: &InterestRateState,
) -> Result<(), MarketError> {
    if !(parameters.min_policy_rate_bp..=parameters.max_policy_rate_bp)
        .contains(&rates.policy_rate_bp)
        || rates
            .policy_rate_bp
            .rem_euclid(parameters.quantization_step_bp)
            != 0
    {
        return Err(MarketError::InvalidRateState(
            "policy rate is outside bounds or not quantized",
        ));
    }
    let maximum_change = i128::from(parameters.max_policy_rate_bp)
        .checked_sub(i128::from(parameters.min_policy_rate_bp))
        .ok_or(MarketError::ArithmeticOverflow(
            "validating the policy-rate change range",
        ))?;
    if i128::from(rates.policy_rate_change_bp).abs() > maximum_change
        || rates
            .policy_rate_change_bp
            .rem_euclid(parameters.quantization_step_bp)
            != 0
    {
        return Err(MarketError::InvalidRateState(
            "policy-rate change is outside bounds or not quantized",
        ));
    }
    let expected_shock = equity_rate_shock_ppm(
        rates.policy_rate_change_bp,
        parameters.equity_shock_ppm_per_policy_bp,
    )?;
    if rates.equity_rate_shock_ppm != expected_shock {
        return Err(MarketError::InvalidRateState(
            "equity rate shock does not match the policy-rate change",
        ));
    }
    let expected_curve = yield_curve_for(parameters, rates.policy_rate_bp)?;
    if rates.treasury_3m_bp != expected_curve.treasury_3m_bp
        || rates.treasury_1y_bp != expected_curve.treasury_1y_bp
        || rates.treasury_3y_bp != expected_curve.treasury_3y_bp
        || rates.treasury_10y_bp != expected_curve.treasury_10y_bp
    {
        return Err(MarketError::InvalidRateState(
            "stored yield curve does not match the policy rate",
        ));
    }

    Ok(())
}

fn initial_interest_rate_state(
    parameters: &InterestRateParameters,
) -> Result<InterestRateState, MarketError> {
    let mut rates = yield_curve_for(parameters, parameters.initial_policy_rate_bp)?;
    rates.policy_rate_change_bp = 0;
    rates.equity_rate_shock_ppm = 0;
    Ok(rates)
}

fn carried_interest_rate_state(previous: &InterestRateState) -> InterestRateState {
    let mut rates = previous.clone();
    rates.policy_rate_change_bp = 0;
    rates.equity_rate_shock_ppm = 0;
    rates
}

fn next_interest_rate_state(
    entropy: &dyn MarketEntropy,
    parameters: &InterestRateParameters,
    world: &MarketWorld,
    previous: &MarketDay,
    game_day: u32,
    session_index: u32,
    regime: MarketRegime,
) -> Result<InterestRateState, MarketError> {
    let previous_rates = previous
        .rates
        .as_ref()
        .ok_or(MarketError::InvalidPreviousDay(
            "rate-enabled market day is missing its rate factor",
        ))?;
    let is_policy_update = session_index.is_multiple_of(parameters.update_interval_sessions);
    let policy_rate_bp = if is_policy_update {
        next_policy_rate_bp(entropy, parameters, world, game_day, regime, previous_rates)?
    } else {
        previous_rates.policy_rate_bp
    };
    let policy_rate_change_bp = policy_rate_bp
        .checked_sub(previous_rates.policy_rate_bp)
        .ok_or(MarketError::ArithmeticOverflow(
            "calculating the policy-rate change",
        ))?;
    let equity_rate_shock_ppm = equity_rate_shock_ppm(
        policy_rate_change_bp,
        parameters.equity_shock_ppm_per_policy_bp,
    )?;

    let mut rates = if is_policy_update || regime != previous.regime {
        yield_curve_for(parameters, policy_rate_bp)?
    } else {
        carried_interest_rate_state(previous_rates)
    };
    rates.policy_rate_bp = policy_rate_bp;
    rates.policy_rate_change_bp = policy_rate_change_bp;
    rates.equity_rate_shock_ppm = equity_rate_shock_ppm;
    Ok(rates)
}

fn next_policy_rate_bp(
    entropy: &dyn MarketEntropy,
    parameters: &InterestRateParameters,
    world: &MarketWorld,
    game_day: u32,
    regime: MarketRegime,
    previous: &InterestRateState,
) -> Result<i64, MarketError> {
    let difference = i128::from(parameters.targets.for_regime(regime))
        .checked_sub(i128::from(previous.policy_rate_bp))
        .ok_or(MarketError::ArithmeticOverflow(
            "calculating the policy-rate target gap",
        ))?;
    let reversion = difference
        .checked_mul(i128::from(parameters.mean_reversion_ppm))
        .ok_or(MarketError::ArithmeticOverflow(
            "scaling policy-rate mean reversion",
        ))?
        / RETURN_SCALE_PPM;
    let innovation = innovation_ppm(entropy.sample_u64(world.seed, game_day, RATE_POLICY_STREAM))?;
    let innovation_bp = i128::from(parameters.innovation_scale_bp)
        .checked_mul(i128::from(innovation))
        .ok_or(MarketError::ArithmeticOverflow(
            "scaling the policy-rate innovation",
        ))?
        / RETURN_SCALE_PPM;
    let candidate = i128::from(previous.policy_rate_bp)
        .checked_add(reversion)
        .and_then(|value| value.checked_add(innovation_bp))
        .ok_or(MarketError::ArithmeticOverflow(
            "forming the next policy rate",
        ))?;
    let quantized = round_to_step(candidate, parameters.quantization_step_bp)?;
    let bounded = quantized.clamp(
        i128::from(parameters.min_policy_rate_bp),
        i128::from(parameters.max_policy_rate_bp),
    );
    i64::try_from(bounded)
        .map_err(|_| MarketError::ArithmeticOverflow("converting the next policy rate"))
}

fn round_to_step(value: i128, step: i64) -> Result<i128, MarketError> {
    if step <= 0 {
        return Err(MarketError::InvalidCalibration(
            "policy-rate quantization step must be positive",
        ));
    }
    let step = i128::from(step);
    let half = step / 2;
    let adjusted = if value >= 0 {
        value.checked_add(half)
    } else {
        value.checked_sub(half)
    }
    .ok_or(MarketError::ArithmeticOverflow("rounding the policy rate"))?;
    adjusted
        .checked_div(step)
        .and_then(|quotient| quotient.checked_mul(step))
        .ok_or(MarketError::ArithmeticOverflow(
            "quantizing the policy rate",
        ))
}

fn yield_curve_for(
    parameters: &InterestRateParameters,
    policy_rate_bp: i64,
) -> Result<InterestRateState, MarketError> {
    Ok(InterestRateState {
        policy_rate_bp,
        treasury_3m_bp: maturity_yield_bp(
            parameters,
            policy_rate_bp,
            &parameters.yield_curve.treasury_3m,
        )?,
        treasury_1y_bp: maturity_yield_bp(
            parameters,
            policy_rate_bp,
            &parameters.yield_curve.treasury_1y,
        )?,
        treasury_3y_bp: maturity_yield_bp(
            parameters,
            policy_rate_bp,
            &parameters.yield_curve.treasury_3y,
        )?,
        treasury_10y_bp: maturity_yield_bp(
            parameters,
            policy_rate_bp,
            &parameters.yield_curve.treasury_10y,
        )?,
        policy_rate_change_bp: 0,
        equity_rate_shock_ppm: 0,
    })
}

fn maturity_yield_bp(
    parameters: &InterestRateParameters,
    policy_rate_bp: i64,
    maturity: &YieldCurveMaturityParameters,
) -> Result<i64, MarketError> {
    let weighted_policy = i128::from(policy_rate_bp)
        .checked_mul(i128::from(maturity.policy_weight_ppm))
        .ok_or(MarketError::ArithmeticOverflow(
            "weighting the policy rate for the yield curve",
        ))?;
    let weighted_neutral = i128::from(parameters.neutral_policy_rate_bp)
        .checked_mul(i128::from(maturity.neutral_weight_ppm))
        .ok_or(MarketError::ArithmeticOverflow(
            "weighting the neutral rate for the yield curve",
        ))?;
    let weighted_average = weighted_policy
        .checked_add(weighted_neutral)
        .and_then(|value| value.checked_add(RETURN_SCALE_PPM / 2))
        .ok_or(MarketError::ArithmeticOverflow(
            "forming the yield-curve weighted average",
        ))?
        / RETURN_SCALE_PPM;
    let with_premium = weighted_average
        .checked_add(i128::from(maturity.term_premium_bp))
        .ok_or(MarketError::ArithmeticOverflow(
            "adding the yield-curve term premium",
        ))?;
    let bounded = with_premium.clamp(0, i128::from(parameters.max_yield_bp));
    i64::try_from(bounded)
        .map_err(|_| MarketError::ArithmeticOverflow("converting the maturity yield"))
}

pub(super) fn equity_rate_shock_ppm(
    policy_rate_change_bp: i64,
    shock_ppm_per_policy_bp: i64,
) -> Result<i64, MarketError> {
    let shock = i128::from(policy_rate_change_bp)
        .checked_mul(i128::from(shock_ppm_per_policy_bp))
        .and_then(i128::checked_neg)
        .ok_or(MarketError::ArithmeticOverflow(
            "calculating the equity rate shock",
        ))?;
    i64::try_from(shock)
        .map_err(|_| MarketError::ArithmeticOverflow("converting the equity rate shock"))
}

pub(super) fn apply_equity_rate_shock(
    base_return_ppm: i64,
    rate_shock_ppm: i64,
) -> Result<i64, MarketError> {
    let adjusted = i128::from(base_return_ppm)
        .checked_add(i128::from(rate_shock_ppm))
        .ok_or(MarketError::ArithmeticOverflow(
            "adding the equity rate shock",
        ))?;
    i64::try_from(adjusted)
        .map_err(|_| MarketError::ArithmeticOverflow("converting the rate-adjusted return"))
}

fn date_for_game_day(start_date: Date, game_day: u32) -> Result<Date, MarketError> {
    start_date
        .checked_add(Duration::days(i64::from(game_day)))
        .ok_or(MarketError::DateOutOfRange)
}

fn next_variance(parameters: &MarketParameters, previous: &MarketDay) -> Result<i64, MarketError> {
    let residual_squared = i128::from(previous.equity_residual_ppm)
        .checked_mul(i128::from(previous.equity_residual_ppm))
        .ok_or(MarketError::ArithmeticOverflow(
            "squaring the prior equity residual",
        ))?;
    let garch = &parameters.equity_garch;
    let shock_term = residual_squared
        .checked_mul(i128::from(garch.alpha_ppm))
        .ok_or(MarketError::ArithmeticOverflow(
            "scaling the GARCH shock term",
        ))?;
    let persistence_term = i128::from(previous.equity_variance_ppm2)
        .checked_mul(i128::from(garch.beta_ppm))
        .ok_or(MarketError::ArithmeticOverflow(
            "scaling the GARCH persistence term",
        ))?;
    let dynamic_variance =
        shock_term
            .checked_add(persistence_term)
            .ok_or(MarketError::ArithmeticOverflow(
                "adding GARCH dynamic terms",
            ))?
            / RETURN_SCALE_PPM;
    let variance = i128::from(garch.omega_ppm2)
        .checked_add(dynamic_variance)
        .ok_or(MarketError::ArithmeticOverflow(
            "adding GARCH baseline variance",
        ))?
        .clamp(
            i128::from(garch.min_variance_ppm2),
            i128::from(garch.max_variance_ppm2),
        );

    i64::try_from(variance)
        .map_err(|_| MarketError::ArithmeticOverflow("converting equity variance"))
}

fn innovation_ppm(entropy: u64) -> Result<i64, MarketError> {
    let centered_quarters =
        i64::from(entropy.count_ones())
            .checked_sub(32)
            .ok_or(MarketError::ArithmeticOverflow(
                "centering the equity innovation",
            ))?;
    centered_quarters
        .checked_mul(INNOVATION_QUARTER_PPM)
        .ok_or(MarketError::ArithmeticOverflow(
            "scaling the equity innovation",
        ))
}

fn equity_return_ppm(
    drift_ppm: i64,
    variance_ppm2: i64,
    innovation_ppm: i64,
) -> Result<i64, MarketError> {
    let standard_deviation = integer_sqrt(variance_ppm2)?;
    let shock = checked_scaled_product(
        standard_deviation,
        innovation_ppm,
        "scaling the equity return shock",
    )?;
    drift_ppm
        .checked_add(shock)
        .ok_or(MarketError::ArithmeticOverflow(
            "adding equity drift and shock",
        ))
}

fn checked_scaled_product(
    left: i64,
    right: i64,
    operation: &'static str,
) -> Result<i64, MarketError> {
    let product = i128::from(left)
        .checked_mul(i128::from(right))
        .ok_or(MarketError::ArithmeticOverflow(operation))?;
    let scaled = product / RETURN_SCALE_PPM;
    i64::try_from(scaled).map_err(|_| MarketError::ArithmeticOverflow(operation))
}

fn next_price(previous_price: i64, return_ppm: i64) -> Result<i64, MarketError> {
    let multiplier =
        1_000_000_i64
            .checked_add(return_ppm)
            .ok_or(MarketError::ArithmeticOverflow(
                "forming the equity price multiplier",
            ))?;
    if multiplier <= 0 {
        return Err(MarketError::NonPositivePrice);
    }
    let numerator = i128::from(previous_price)
        .checked_mul(i128::from(multiplier))
        .ok_or(MarketError::ArithmeticOverflow(
            "multiplying the equity price",
        ))?
        .checked_add(RETURN_SCALE_PPM / 2)
        .ok_or(MarketError::ArithmeticOverflow("rounding the equity price"))?;
    let price = numerator / RETURN_SCALE_PPM;
    let price = i64::try_from(price)
        .map_err(|_| MarketError::ArithmeticOverflow("converting the equity price"))?;
    if price <= 0 {
        return Err(MarketError::NonPositivePrice);
    }

    Ok(price)
}

fn integer_sqrt(value: i64) -> Result<i64, MarketError> {
    let value = u64::try_from(value).map_err(|_| {
        MarketError::InvalidCalibration("variance must be non-negative before square root")
    })?;
    let root = value.isqrt();
    i64::try_from(root)
        .map_err(|_| MarketError::ArithmeticOverflow("converting equity standard deviation"))
}

fn probability_draw(entropy: u64) -> u32 {
    ((u128::from(entropy) * u128::from(PROBABILITY_SCALE_PPM)) >> 64) as u32
}

fn choose_regime(
    probabilities: &RegimeProbabilities,
    draw: u32,
) -> Result<MarketRegime, MarketError> {
    let mut boundary = 0_u32;
    for regime in [
        MarketRegime::Expansion,
        MarketRegime::Slowdown,
        MarketRegime::Recession,
        MarketRegime::Recovery,
    ] {
        boundary = boundary
            .checked_add(probabilities.for_regime(regime))
            .ok_or(MarketError::ArithmeticOverflow(
                "accumulating regime probabilities",
            ))?;
        if draw < boundary {
            return Ok(regime);
        }
    }

    Err(MarketError::InvalidCalibration(
        "regime probabilities did not cover the draw",
    ))
}
