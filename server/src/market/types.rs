use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};
use time::Date;
use utoipa::ToSchema;

pub const PROBABILITY_SCALE_PPM: u32 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MarketRegime {
    Expansion,
    Slowdown,
    Recession,
    Recovery,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegimeProbabilities {
    pub expansion: u32,
    pub slowdown: u32,
    pub recession: u32,
    pub recovery: u32,
}

impl RegimeProbabilities {
    pub(crate) const fn for_regime(&self, regime: MarketRegime) -> u32 {
        match regime {
            MarketRegime::Expansion => self.expansion,
            MarketRegime::Slowdown => self.slowdown,
            MarketRegime::Recession => self.recession,
            MarketRegime::Recovery => self.recovery,
        }
    }

    pub(crate) fn checked_total(&self) -> Option<u32> {
        self.expansion
            .checked_add(self.slowdown)?
            .checked_add(self.recession)?
            .checked_add(self.recovery)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegimeParameters {
    pub daily_drift_ppm: i64,
    pub transition_ppm: RegimeProbabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketRegimeParameters {
    pub expansion: RegimeParameters,
    pub slowdown: RegimeParameters,
    pub recession: RegimeParameters,
    pub recovery: RegimeParameters,
}

impl MarketRegimeParameters {
    pub(crate) const fn for_regime(&self, regime: MarketRegime) -> &RegimeParameters {
        match regime {
            MarketRegime::Expansion => &self.expansion,
            MarketRegime::Slowdown => &self.slowdown,
            MarketRegime::Recession => &self.recession,
            MarketRegime::Recovery => &self.recovery,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EquityGarchParameters {
    pub initial_variance_ppm2: i64,
    pub omega_ppm2: i64,
    pub alpha_ppm: u32,
    pub beta_ppm: u32,
    pub min_variance_ppm2: i64,
    pub max_variance_ppm2: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRateTargets {
    pub expansion: i64,
    pub slowdown: i64,
    pub recession: i64,
    pub recovery: i64,
}

impl PolicyRateTargets {
    pub(crate) const fn for_regime(&self, regime: MarketRegime) -> i64 {
        match regime {
            MarketRegime::Expansion => self.expansion,
            MarketRegime::Slowdown => self.slowdown,
            MarketRegime::Recession => self.recession,
            MarketRegime::Recovery => self.recovery,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YieldCurveMaturityParameters {
    pub policy_weight_ppm: u32,
    pub neutral_weight_ppm: u32,
    pub term_premium_bp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YieldCurveParameters {
    pub treasury_3m: YieldCurveMaturityParameters,
    pub treasury_1y: YieldCurveMaturityParameters,
    pub treasury_3y: YieldCurveMaturityParameters,
    pub treasury_10y: YieldCurveMaturityParameters,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterestRateParameters {
    pub initial_policy_rate_bp: i64,
    pub neutral_policy_rate_bp: i64,
    pub update_interval_sessions: u32,
    pub mean_reversion_ppm: u32,
    pub innovation_scale_bp: i64,
    pub quantization_step_bp: i64,
    pub min_policy_rate_bp: i64,
    pub max_policy_rate_bp: i64,
    pub targets: PolicyRateTargets,
    pub yield_curve: YieldCurveParameters,
    pub max_yield_bp: i64,
    pub equity_shock_ppm_per_policy_bp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CpiMarketParameters {
    pub day0_index: i64,
    pub annual_rate_ppm: i64,
    pub day_count_denominator: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoldMarketParameters {
    pub day0_close_krw_per_gram: i64,
    pub innovation_scale_ppm: i64,
    pub treasury_10y_sensitivity_ppm_per_bp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct M2MarketParameters {
    pub cpi: CpiMarketParameters,
    pub gold: GoldMarketParameters,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketParameters {
    pub initial_regime: MarketRegime,
    pub sessions_per_regime_transition: u32,
    pub regimes: MarketRegimeParameters,
    pub equity_garch: EquityGarchParameters,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interest_rates: Option<InterestRateParameters>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub m2: Option<M2MarketParameters>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketCalibration {
    pub version: String,
    pub parameters: MarketParameters,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketWorld {
    pub key: String,
    pub seed: u64,
    pub start_date: Date,
    pub day0_equity_close_krw: i64,
    pub index_product: Option<IndexProductTerms>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexProductTerms {
    pub product_version_id: u64,
    pub product_key: String,
    pub day0_close_krw: i64,
    pub annual_management_fee_ppm: i64,
    pub annual_distribution_rate_ppm: i64,
    pub day_count_denominator: u32,
    pub buy_fee_ppm: i64,
    pub sell_fee_ppm: i64,
    pub transaction_tax_ppm: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketDay {
    pub game_day: u32,
    pub market_date: Date,
    pub market_open: bool,
    pub session_index: u32,
    pub regime: MarketRegime,
    pub equity_close_krw: i64,
    pub equity_return_ppm: i64,
    pub equity_variance_ppm2: i64,
    /// The last open-session innovation is carried across closures for GARCH continuity.
    pub equity_residual_ppm: i64,
    pub rates: Option<InterestRateState>,
    pub m2: Option<M2MarketState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterestRateState {
    pub policy_rate_bp: i64,
    pub treasury_3m_bp: i64,
    pub treasury_1y_bp: i64,
    pub treasury_3y_bp: i64,
    pub treasury_10y_bp: i64,
    pub policy_rate_change_bp: i64,
    pub equity_rate_shock_ppm: i64,
}

/// Database-shaped input used to distinguish an absent legacy factor from a corrupt partial row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NullableInterestRateState {
    pub policy_rate_bp: Option<i64>,
    pub treasury_3m_bp: Option<i64>,
    pub treasury_1y_bp: Option<i64>,
    pub treasury_3y_bp: Option<i64>,
    pub treasury_10y_bp: Option<i64>,
    pub policy_rate_change_bp: Option<i64>,
    pub equity_rate_shock_ppm: Option<i64>,
}

impl NullableInterestRateState {
    pub fn into_complete(self) -> Result<Option<InterestRateState>, MarketError> {
        let fields_present = [
            self.policy_rate_bp.is_some(),
            self.treasury_3m_bp.is_some(),
            self.treasury_1y_bp.is_some(),
            self.treasury_3y_bp.is_some(),
            self.treasury_10y_bp.is_some(),
            self.policy_rate_change_bp.is_some(),
            self.equity_rate_shock_ppm.is_some(),
        ];
        if fields_present.iter().all(|present| !present) {
            return Ok(None);
        }
        if !fields_present.iter().all(|present| *present) {
            return Err(MarketError::InvalidRateState(
                "rate factor columns must be either all null or all populated",
            ));
        }

        let Some(policy_rate_bp) = self.policy_rate_bp else {
            return Err(MarketError::InvalidRateState("policy rate is missing"));
        };
        let Some(treasury_3m_bp) = self.treasury_3m_bp else {
            return Err(MarketError::InvalidRateState(
                "three-month yield is missing",
            ));
        };
        let Some(treasury_1y_bp) = self.treasury_1y_bp else {
            return Err(MarketError::InvalidRateState("one-year yield is missing"));
        };
        let Some(treasury_3y_bp) = self.treasury_3y_bp else {
            return Err(MarketError::InvalidRateState("three-year yield is missing"));
        };
        let Some(treasury_10y_bp) = self.treasury_10y_bp else {
            return Err(MarketError::InvalidRateState("ten-year yield is missing"));
        };
        let Some(policy_rate_change_bp) = self.policy_rate_change_bp else {
            return Err(MarketError::InvalidRateState(
                "policy-rate change is missing",
            ));
        };
        let Some(equity_rate_shock_ppm) = self.equity_rate_shock_ppm else {
            return Err(MarketError::InvalidRateState(
                "equity rate shock is missing",
            ));
        };

        Ok(Some(InterestRateState {
            policy_rate_bp,
            treasury_3m_bp,
            treasury_1y_bp,
            treasury_3y_bp,
            treasury_10y_bp,
            policy_rate_change_bp,
            equity_rate_shock_ppm,
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct M2MarketState {
    pub cpi_index: i64,
    pub cpi_remainder: i64,
    pub llx_close_krw: i64,
    pub llx_return_ppm: i64,
    pub llx_fee_remainder: i64,
    pub llx_fee_accumulator_ppm: i64,
    pub gold_close_krw_per_gram: i64,
    pub gold_prior_open_cpi_index: i64,
    pub gold_prior_open_treasury_10y_bp: i64,
}

/// Database-shaped input used to reject partially populated v4 cache rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NullableM2MarketState {
    pub cpi_index: Option<i64>,
    pub cpi_remainder: Option<i64>,
    pub llx_close_krw: Option<i64>,
    pub llx_return_ppm: Option<i64>,
    pub llx_fee_remainder: Option<i64>,
    pub llx_fee_accumulator_ppm: Option<i64>,
    pub gold_close_krw_per_gram: Option<i64>,
    pub gold_prior_open_cpi_index: Option<i64>,
    pub gold_prior_open_treasury_10y_bp: Option<i64>,
}

impl NullableM2MarketState {
    pub fn into_complete(self) -> Result<Option<M2MarketState>, MarketError> {
        let fields_present = [
            self.cpi_index.is_some(),
            self.cpi_remainder.is_some(),
            self.llx_close_krw.is_some(),
            self.llx_return_ppm.is_some(),
            self.llx_fee_remainder.is_some(),
            self.llx_fee_accumulator_ppm.is_some(),
            self.gold_close_krw_per_gram.is_some(),
            self.gold_prior_open_cpi_index.is_some(),
            self.gold_prior_open_treasury_10y_bp.is_some(),
        ];
        if fields_present.iter().all(|present| !present) {
            return Ok(None);
        }
        if !fields_present.iter().all(|present| *present) {
            return Err(MarketError::InvalidM2State(
                "v4 market columns must be either all null or all populated",
            ));
        }

        let Some(cpi_index) = self.cpi_index else {
            return Err(MarketError::InvalidM2State("CPI index is missing"));
        };
        let Some(cpi_remainder) = self.cpi_remainder else {
            return Err(MarketError::InvalidM2State("CPI remainder is missing"));
        };
        let Some(llx_close_krw) = self.llx_close_krw else {
            return Err(MarketError::InvalidM2State("LLX close is missing"));
        };
        let Some(llx_return_ppm) = self.llx_return_ppm else {
            return Err(MarketError::InvalidM2State("LLX return is missing"));
        };
        let Some(llx_fee_remainder) = self.llx_fee_remainder else {
            return Err(MarketError::InvalidM2State("LLX fee remainder is missing"));
        };
        let Some(llx_fee_accumulator_ppm) = self.llx_fee_accumulator_ppm else {
            return Err(MarketError::InvalidM2State(
                "LLX fee accumulator is missing",
            ));
        };
        let Some(gold_close_krw_per_gram) = self.gold_close_krw_per_gram else {
            return Err(MarketError::InvalidM2State("gold close is missing"));
        };
        let Some(gold_prior_open_cpi_index) = self.gold_prior_open_cpi_index else {
            return Err(MarketError::InvalidM2State(
                "gold prior-open CPI index is missing",
            ));
        };
        let Some(gold_prior_open_treasury_10y_bp) = self.gold_prior_open_treasury_10y_bp else {
            return Err(MarketError::InvalidM2State(
                "gold prior-open ten-year yield is missing",
            ));
        };

        Ok(Some(M2MarketState {
            cpi_index,
            cpi_remainder,
            llx_close_krw,
            llx_return_ppm,
            llx_fee_remainder,
            llx_fee_accumulator_ppm,
            gold_close_krw_per_gram,
            gold_prior_open_cpi_index,
            gold_prior_open_treasury_10y_bp,
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketError {
    InvalidCalibration(&'static str),
    InvalidCalendar(&'static str),
    InvalidWorld(&'static str),
    InvalidPreviousDay(&'static str),
    InvalidRateState(&'static str),
    InvalidM2State(&'static str),
    DateOutOfRange,
    ArithmeticOverflow(&'static str),
    NonPositivePrice,
}

impl Display for MarketError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCalibration(reason) => write!(formatter, "invalid calibration: {reason}"),
            Self::InvalidCalendar(reason) => write!(formatter, "invalid market calendar: {reason}"),
            Self::InvalidWorld(reason) => write!(formatter, "invalid market world: {reason}"),
            Self::InvalidPreviousDay(reason) => {
                write!(formatter, "invalid previous market day: {reason}")
            }
            Self::InvalidRateState(reason) => write!(formatter, "invalid rate state: {reason}"),
            Self::InvalidM2State(reason) => write!(formatter, "invalid M2 market state: {reason}"),
            Self::DateOutOfRange => formatter.write_str("market date is out of range"),
            Self::ArithmeticOverflow(operation) => {
                write!(formatter, "market arithmetic overflow while {operation}")
            }
            Self::NonPositivePrice => formatter.write_str("equity price must stay positive"),
        }
    }
}

impl Error for MarketError {}
