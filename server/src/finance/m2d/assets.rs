//! M2-D LLX, bond, gold, and pension valuation rules (§8.2–§8.5).

use std::error::Error;
use std::fmt::{Display, Formatter};

use time::{Date, Month};

use super::super::tax_accounts::PensionTaxLayers;

const RATE_SCALE_PPM: i128 = 1_000_000;
const BOND_PRICE_SCALE: i128 = 3_650_000;
const LLX_DISTRIBUTIONS_PER_YEAR: i128 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum M2dAssetError {
    InvalidMoney,
    InvalidRate,
    InvalidDateRange,
    InvalidFeeState,
    InvalidOpenSessionCalendar,
    MissingOpenSession,
    InvalidQuantity,
    InvalidPosition,
    PositionLimitExceeded,
    InsufficientCash,
    InsufficientQuantity,
    UnsupportedGoldBarSize,
    InvalidTaxLayerState,
    LossExceedsBalance,
    InsufficientValuationBasis,
    ArithmeticOverflow,
}

impl Display for M2dAssetError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidMoney => "asset money must satisfy its non-negative constraints",
            Self::InvalidRate => "asset rate is invalid",
            Self::InvalidDateRange => "asset date range is invalid",
            Self::InvalidFeeState => "LLX fee carry state is invalid",
            Self::InvalidOpenSessionCalendar => "open-session calendar is invalid",
            Self::MissingOpenSession => "required future open session is missing",
            Self::InvalidQuantity => "asset quantity is invalid",
            Self::InvalidPosition => "asset position violates its invariants",
            Self::PositionLimitExceeded => "bond position limit would be exceeded",
            Self::InsufficientCash => "account cash is insufficient",
            Self::InsufficientQuantity => "asset quantity is insufficient",
            Self::UnsupportedGoldBarSize => "gold bar size is not supported",
            Self::InvalidTaxLayerState => "pension tax layers do not match account value",
            Self::LossExceedsBalance => "pension market loss exceeds account balance",
            Self::InsufficientValuationBasis => "pension valuation basis is insufficient",
            Self::ArithmeticOverflow => "asset arithmetic overflowed",
        };
        formatter.write_str(message)
    }
}

impl Error for M2dAssetError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LlxProductTerms {
    pub annual_management_fee_ppm: i64,
    pub annual_distribution_rate_ppm: i64,
    pub day_count_denominator: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LlxFeeState {
    pub remainder_numerator: i64,
    pub pending_fee_ppm: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LlxFeeAccrual {
    pub accrued_today_ppm: i64,
    pub applied_fee_ppm: i64,
    pub next_state: LlxFeeState,
}

pub fn accrue_llx_management_fee(
    terms: LlxProductTerms,
    state: LlxFeeState,
    market_open: bool,
) -> Result<LlxFeeAccrual, M2dAssetError> {
    validate_llx_product_terms(terms)?;
    let day_count_denominator = i64::from(terms.day_count_denominator);
    if !(0..day_count_denominator).contains(&state.remainder_numerator) || state.pending_fee_ppm < 0
    {
        return Err(M2dAssetError::InvalidFeeState);
    }

    let day_count_denominator = i128::from(terms.day_count_denominator);
    let numerator = i128::from(terms.annual_management_fee_ppm)
        .checked_add(i128::from(state.remainder_numerator))
        .ok_or(M2dAssetError::ArithmeticOverflow)?;
    let accrued_today_ppm = checked_i128_to_i64(
        numerator
            .checked_div(day_count_denominator)
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;
    let remainder_numerator = checked_i128_to_i64(
        numerator
            .checked_rem(day_count_denominator)
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;
    let accumulated_fee_ppm = checked_i128_to_i64(
        i128::from(state.pending_fee_ppm)
            .checked_add(i128::from(accrued_today_ppm))
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;
    let (applied_fee_ppm, pending_fee_ppm) = if market_open {
        (accumulated_fee_ppm, 0)
    } else {
        (0, accumulated_fee_ppm)
    };

    Ok(LlxFeeAccrual {
        accrued_today_ppm,
        applied_fee_ppm,
        next_state: LlxFeeState {
            remainder_numerator,
            pending_fee_ppm,
        },
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LlxQuarterRecordDateInput {
    pub current_date: Date,
    pub market_open: bool,
    pub next_open_date: Option<Date>,
}

pub fn is_llx_quarter_record_date(input: LlxQuarterRecordDateInput) -> Result<bool, M2dAssetError> {
    let is_quarter_end_month = matches!(
        input.current_date.month(),
        Month::March | Month::June | Month::September | Month::December
    );
    if !input.market_open || !is_quarter_end_month {
        return Ok(false);
    }

    let next_open_date = input
        .next_open_date
        .ok_or(M2dAssetError::MissingOpenSession)?;
    if next_open_date <= input.current_date {
        return Err(M2dAssetError::InvalidOpenSessionCalendar);
    }

    Ok(next_open_date.year() != input.current_date.year()
        || next_open_date.month() != input.current_date.month())
}

pub fn llx_t_plus_two_open_date(
    record_date: Date,
    following_open_dates: &[Date],
) -> Result<Date, M2dAssetError> {
    let mut previous = record_date;
    for open_date in following_open_dates {
        if *open_date <= previous {
            return Err(M2dAssetError::InvalidOpenSessionCalendar);
        }
        previous = *open_date;
    }

    following_open_dates
        .get(1)
        .copied()
        .ok_or(M2dAssetError::MissingOpenSession)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlxNoMovementReason {
    ZeroDistribution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlxDistributionMovement {
    Cash { amount_krw: i64 },
    NoMovement { reason: LlxNoMovementReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LlxEntitlementInput {
    pub record_date: Date,
    pub payment_date: Date,
    pub record_quantity: u32,
    pub record_close_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LlxEntitlementDraft {
    pub record_date: Date,
    pub payment_date: Date,
    pub record_quantity: u32,
    pub record_close_krw: i64,
    pub per_share_distribution_krw: i64,
    pub gross_distribution_krw: i64,
    pub movement: LlxDistributionMovement,
}

pub fn llx_distribution_per_share_krw(
    terms: LlxProductTerms,
    record_close_krw: i64,
) -> Result<i64, M2dAssetError> {
    validate_llx_product_terms(terms)?;
    validate_positive_money(record_close_krw)?;
    let numerator = i128::from(record_close_krw)
        .checked_mul(i128::from(terms.annual_distribution_rate_ppm))
        .ok_or(M2dAssetError::ArithmeticOverflow)?;
    let denominator = LLX_DISTRIBUTIONS_PER_YEAR
        .checked_mul(RATE_SCALE_PPM)
        .ok_or(M2dAssetError::ArithmeticOverflow)?;
    checked_i128_to_i64(
        numerator
            .checked_div(denominator)
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )
}

pub fn draft_llx_distribution_entitlement(
    terms: LlxProductTerms,
    input: LlxEntitlementInput,
) -> Result<LlxEntitlementDraft, M2dAssetError> {
    if input.record_quantity == 0 {
        return Err(M2dAssetError::InvalidQuantity);
    }
    if input.payment_date <= input.record_date {
        return Err(M2dAssetError::InvalidDateRange);
    }

    let per_share_distribution_krw = llx_distribution_per_share_krw(terms, input.record_close_krw)?;
    let gross_distribution_krw = checked_i128_to_i64(
        i128::from(per_share_distribution_krw)
            .checked_mul(i128::from(input.record_quantity))
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;
    let movement = if gross_distribution_krw == 0 {
        LlxDistributionMovement::NoMovement {
            reason: LlxNoMovementReason::ZeroDistribution,
        }
    } else {
        LlxDistributionMovement::Cash {
            amount_krw: gross_distribution_krw,
        }
    };

    Ok(LlxEntitlementDraft {
        record_date: input.record_date,
        payment_date: input.payment_date,
        record_quantity: input.record_quantity,
        record_close_krw: input.record_close_krw,
        per_share_distribution_krw,
        gross_distribution_krw,
        movement,
    })
}

fn validate_llx_product_terms(terms: LlxProductTerms) -> Result<(), M2dAssetError> {
    validate_rate_ppm(terms.annual_management_fee_ppm)?;
    validate_rate_ppm(terms.annual_distribution_rate_ppm)?;
    if terms.day_count_denominator == 0 {
        return Err(M2dAssetError::InvalidRate);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BondTerm {
    Years3,
    Years10,
}

impl BondTerm {
    pub const fn coupon_periods(self) -> u32 {
        match self {
            Self::Years3 => 6,
            Self::Years10 => 20,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BondProductTerms {
    pub term: BondTerm,
    pub face_value_krw: i64,
    pub maximum_order_units: u32,
    pub maximum_position_units: u32,
    pub coupon_rate_step_bp: i32,
    pub buy_fee_rate_ppm: i64,
    pub sell_fee_rate_ppm: i64,
    pub buy_tax_rate_ppm: i64,
    pub sell_tax_rate_ppm: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BondCashFlow {
    pub payment_date: Date,
    pub coupon_krw: i64,
    pub principal_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BondSeries {
    pub product_terms: BondProductTerms,
    pub issue_date: Date,
    pub maturity_date: Date,
    pub issue_yield_bp: i32,
    pub coupon_rate_bp: i32,
    pub annual_coupon_krw: i64,
    pub cash_flows: Vec<BondCashFlow>,
}

pub fn round_bond_coupon_rate_bp(
    terms: BondProductTerms,
    issue_yield_bp: i32,
) -> Result<i32, M2dAssetError> {
    validate_bond_product_terms(terms)?;
    if issue_yield_bp < 0 {
        return Err(M2dAssetError::InvalidRate);
    }

    let step = i128::from(terms.coupon_rate_step_bp);
    let rounded = i128::from(issue_yield_bp)
        .checked_add(
            step.checked_div(2)
                .ok_or(M2dAssetError::ArithmeticOverflow)?,
        )
        .and_then(|value| value.checked_div(step))
        .and_then(|value| value.checked_mul(step))
        .ok_or(M2dAssetError::ArithmeticOverflow)?;
    i32::try_from(rounded).map_err(|_| M2dAssetError::ArithmeticOverflow)
}

pub fn create_bond_series(
    terms: BondProductTerms,
    issue_date: Date,
    issue_yield_bp: i32,
) -> Result<BondSeries, M2dAssetError> {
    validate_bond_product_terms(terms)?;
    let coupon_rate_bp = round_bond_coupon_rate_bp(terms, issue_yield_bp)?;
    let annual_coupon_krw = checked_i128_to_i64(
        i128::from(terms.face_value_krw)
            .checked_mul(i128::from(coupon_rate_bp))
            .and_then(|value| value.checked_div(10_000))
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;
    let first_half_coupon_krw = checked_i128_to_i64(
        i128::from(annual_coupon_krw)
            .checked_div(2)
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;
    let second_half_coupon_krw = checked_i128_to_i64(
        i128::from(annual_coupon_krw)
            .checked_sub(i128::from(first_half_coupon_krw))
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;
    let coupon_periods = terms.term.coupon_periods();
    let capacity =
        usize::try_from(coupon_periods).map_err(|_| M2dAssetError::ArithmeticOverflow)?;
    let mut cash_flows = Vec::with_capacity(capacity);

    for period in 1..=coupon_periods {
        let months = period
            .checked_mul(6)
            .ok_or(M2dAssetError::ArithmeticOverflow)?;
        let payment_date = add_months_clamped(issue_date, months)?;
        let coupon_krw = if period % 2 == 1 {
            first_half_coupon_krw
        } else {
            second_half_coupon_krw
        };
        let principal_krw = if period == coupon_periods {
            terms.face_value_krw
        } else {
            0
        };
        cash_flows.push(BondCashFlow {
            payment_date,
            coupon_krw,
            principal_krw,
        });
    }

    let maturity_date = cash_flows
        .last()
        .map(|cash_flow| cash_flow.payment_date)
        .ok_or(M2dAssetError::InvalidDateRange)?;
    Ok(BondSeries {
        product_terms: terms,
        issue_date,
        maturity_date,
        issue_yield_bp,
        coupon_rate_bp,
        annual_coupon_krw,
        cash_flows,
    })
}

pub fn dirty_bond_price_krw(
    valuation_date: Date,
    yield_bp: i32,
    cash_flows: &[BondCashFlow],
) -> Result<i64, M2dAssetError> {
    if yield_bp < 0 {
        return Err(M2dAssetError::InvalidRate);
    }

    let mut dirty_price_krw = 0_i128;
    for cash_flow in cash_flows {
        validate_non_negative_money(cash_flow.coupon_krw)?;
        validate_non_negative_money(cash_flow.principal_krw)?;
        if cash_flow.payment_date <= valuation_date {
            continue;
        }

        let remaining_days = i128::from((cash_flow.payment_date - valuation_date).whole_days());
        let cash_flow_krw = i128::from(cash_flow.coupon_krw)
            .checked_add(i128::from(cash_flow.principal_krw))
            .ok_or(M2dAssetError::ArithmeticOverflow)?;
        let numerator = cash_flow_krw
            .checked_mul(BOND_PRICE_SCALE)
            .ok_or(M2dAssetError::ArithmeticOverflow)?;
        let denominator = i128::from(yield_bp)
            .checked_mul(remaining_days)
            .and_then(|value| value.checked_add(BOND_PRICE_SCALE))
            .ok_or(M2dAssetError::ArithmeticOverflow)?;
        let present_value_krw = round_half_up_non_negative(numerator, denominator)?;
        dirty_price_krw = dirty_price_krw
            .checked_add(present_value_krw)
            .ok_or(M2dAssetError::ArithmeticOverflow)?;
    }

    checked_i128_to_i64(dirty_price_krw)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BondOrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BondLot {
    pub units: u32,
    pub cost_basis_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BondLotRemoval {
    pub lot_index: usize,
    pub removed_units: u32,
    pub removed_cost_basis_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BondExecutionInput {
    pub side: BondOrderSide,
    pub units: u32,
    pub dirty_price_krw: i64,
    pub current_position_units: u32,
    pub lots: Vec<BondLot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BondExecutionPlan {
    pub side: BondOrderSide,
    pub units: u32,
    pub gross_amount_krw: i64,
    pub fee_krw: i64,
    pub tax_krw: i64,
    pub removed_cost_basis_krw: i64,
    pub realized_gain_loss_krw: i64,
    pub position_units_after: u32,
    pub remaining_lots: Vec<BondLot>,
    pub removals: Vec<BondLotRemoval>,
}

pub fn plan_bond_execution(
    terms: BondProductTerms,
    input: BondExecutionInput,
) -> Result<BondExecutionPlan, M2dAssetError> {
    validate_bond_product_terms(terms)?;
    validate_bond_position(terms, input.current_position_units, &input.lots)?;
    if input.units == 0 || input.units > terms.maximum_order_units {
        return Err(M2dAssetError::InvalidQuantity);
    }
    validate_positive_money(input.dirty_price_krw)?;

    let gross_amount_krw = checked_i128_to_i64(
        i128::from(input.dirty_price_krw)
            .checked_mul(i128::from(input.units))
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;

    let fee_rate_ppm = match input.side {
        BondOrderSide::Buy => terms.buy_fee_rate_ppm,
        BondOrderSide::Sell => terms.sell_fee_rate_ppm,
    };
    let tax_rate_ppm = match input.side {
        BondOrderSide::Buy => terms.buy_tax_rate_ppm,
        BondOrderSide::Sell => terms.sell_tax_rate_ppm,
    };
    let fee_krw = floor_rate_ppm(gross_amount_krw, fee_rate_ppm)?;
    let tax_krw = floor_rate_ppm(gross_amount_krw, tax_rate_ppm)?;

    match input.side {
        BondOrderSide::Buy => plan_bond_buy(terms, input, gross_amount_krw, fee_krw, tax_krw),
        BondOrderSide::Sell => plan_bond_sale(input, gross_amount_krw, fee_krw, tax_krw),
    }
}

fn plan_bond_buy(
    terms: BondProductTerms,
    input: BondExecutionInput,
    gross_amount_krw: i64,
    fee_krw: i64,
    tax_krw: i64,
) -> Result<BondExecutionPlan, M2dAssetError> {
    let position_units_after = checked_i128_to_u32(
        i128::from(input.current_position_units)
            .checked_add(i128::from(input.units))
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;
    if position_units_after > terms.maximum_position_units {
        return Err(M2dAssetError::PositionLimitExceeded);
    }

    let acquisition_cost_krw = checked_i128_to_i64(
        i128::from(gross_amount_krw)
            .checked_add(i128::from(fee_krw))
            .and_then(|value| value.checked_add(i128::from(tax_krw)))
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;

    let mut remaining_lots = input.lots;
    remaining_lots.push(BondLot {
        units: input.units,
        cost_basis_krw: acquisition_cost_krw,
    });
    Ok(BondExecutionPlan {
        side: input.side,
        units: input.units,
        gross_amount_krw,
        fee_krw,
        tax_krw,
        removed_cost_basis_krw: 0,
        realized_gain_loss_krw: 0,
        position_units_after,
        remaining_lots,
        removals: Vec::new(),
    })
}

fn plan_bond_sale(
    input: BondExecutionInput,
    gross_amount_krw: i64,
    fee_krw: i64,
    tax_krw: i64,
) -> Result<BondExecutionPlan, M2dAssetError> {
    if input.units > input.current_position_units {
        return Err(M2dAssetError::InsufficientQuantity);
    }

    let mut units_to_remove = input.units;
    let mut removed_cost_basis_krw = 0_i128;
    let mut remaining_lots = Vec::with_capacity(input.lots.len());
    let mut removals = Vec::new();
    for (lot_index, lot) in input.lots.iter().copied().enumerate() {
        if units_to_remove == 0 {
            remaining_lots.push(lot);
            continue;
        }

        let removed_units = units_to_remove.min(lot.units);
        let lot_removed_cost_basis_krw = if removed_units == lot.units {
            lot.cost_basis_krw
        } else {
            checked_i128_to_i64(
                i128::from(lot.cost_basis_krw)
                    .checked_mul(i128::from(removed_units))
                    .and_then(|value| value.checked_div(i128::from(lot.units)))
                    .ok_or(M2dAssetError::ArithmeticOverflow)?,
            )?
        };
        removed_cost_basis_krw = removed_cost_basis_krw
            .checked_add(i128::from(lot_removed_cost_basis_krw))
            .ok_or(M2dAssetError::ArithmeticOverflow)?;
        removals.push(BondLotRemoval {
            lot_index,
            removed_units,
            removed_cost_basis_krw: lot_removed_cost_basis_krw,
        });
        units_to_remove = checked_i128_to_u32(
            i128::from(units_to_remove)
                .checked_sub(i128::from(removed_units))
                .ok_or(M2dAssetError::ArithmeticOverflow)?,
        )?;

        if removed_units < lot.units {
            remaining_lots.push(BondLot {
                units: checked_i128_to_u32(
                    i128::from(lot.units)
                        .checked_sub(i128::from(removed_units))
                        .ok_or(M2dAssetError::ArithmeticOverflow)?,
                )?,
                cost_basis_krw: checked_i128_to_i64(
                    i128::from(lot.cost_basis_krw)
                        .checked_sub(i128::from(lot_removed_cost_basis_krw))
                        .ok_or(M2dAssetError::ArithmeticOverflow)?,
                )?,
            });
        }
    }

    if units_to_remove != 0 {
        return Err(M2dAssetError::InvalidPosition);
    }
    let removed_cost_basis_krw = checked_i128_to_i64(removed_cost_basis_krw)?;
    let realized_gain_loss_krw = checked_i128_to_i64(
        i128::from(gross_amount_krw)
            .checked_sub(i128::from(fee_krw))
            .and_then(|value| value.checked_sub(i128::from(tax_krw)))
            .and_then(|value| value.checked_sub(i128::from(removed_cost_basis_krw)))
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;
    let position_units_after = checked_i128_to_u32(
        i128::from(input.current_position_units)
            .checked_sub(i128::from(input.units))
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;

    Ok(BondExecutionPlan {
        side: input.side,
        units: input.units,
        gross_amount_krw,
        fee_krw,
        tax_krw,
        removed_cost_basis_krw,
        realized_gain_loss_krw,
        position_units_after,
        remaining_lots,
        removals,
    })
}

fn validate_bond_position(
    terms: BondProductTerms,
    current_position_units: u32,
    lots: &[BondLot],
) -> Result<(), M2dAssetError> {
    if current_position_units > terms.maximum_position_units {
        return Err(M2dAssetError::InvalidPosition);
    }

    let mut lot_units = 0_i128;
    for lot in lots {
        if lot.units == 0 || lot.cost_basis_krw < 0 {
            return Err(M2dAssetError::InvalidPosition);
        }
        lot_units = lot_units
            .checked_add(i128::from(lot.units))
            .ok_or(M2dAssetError::ArithmeticOverflow)?;
    }
    if lot_units != i128::from(current_position_units) {
        return Err(M2dAssetError::InvalidPosition);
    }
    Ok(())
}

fn validate_bond_product_terms(terms: BondProductTerms) -> Result<(), M2dAssetError> {
    validate_positive_money(terms.face_value_krw)?;
    if terms.maximum_order_units == 0
        || terms.maximum_position_units == 0
        || terms.maximum_order_units > terms.maximum_position_units
        || terms.coupon_rate_step_bp <= 0
    {
        return Err(M2dAssetError::InvalidRate);
    }
    validate_rate_ppm(terms.buy_fee_rate_ppm)?;
    validate_rate_ppm(terms.sell_fee_rate_ppm)?;
    validate_rate_ppm(terms.buy_tax_rate_ppm)?;
    validate_rate_ppm(terms.sell_tax_rate_ppm)
}

fn add_months_clamped(date: Date, months: u32) -> Result<Date, M2dAssetError> {
    let base_month = i128::from(date.year())
        .checked_mul(12)
        .and_then(|value| value.checked_add(i128::from(u8::from(date.month())) - 1))
        .ok_or(M2dAssetError::ArithmeticOverflow)?;
    let target_month = base_month
        .checked_add(i128::from(months))
        .ok_or(M2dAssetError::ArithmeticOverflow)?;
    let year =
        i32::try_from(target_month.div_euclid(12)).map_err(|_| M2dAssetError::InvalidDateRange)?;
    let month_number = u8::try_from(target_month.rem_euclid(12) + 1)
        .map_err(|_| M2dAssetError::InvalidDateRange)?;
    let month = Month::try_from(month_number).map_err(|_| M2dAssetError::InvalidDateRange)?;

    for day in (1..=date.day()).rev() {
        if let Ok(candidate) = Date::from_calendar_date(year, month, day) {
            return Ok(candidate);
        }
    }
    Err(M2dAssetError::InvalidDateRange)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoldProductTerms {
    pub buy_fee_ppm: i64,
    pub sell_fee_ppm: i64,
    pub buy_tax_ppm: i64,
    pub sell_tax_ppm: i64,
    pub withdrawal_fee_100g_krw: i64,
    pub withdrawal_fee_1kg_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoldTaxPolicy {
    pub withdrawal_vat_rate_ppm: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GoldPosition {
    pub quantity_gram: u32,
    pub total_cost_basis_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoldCostRemoval {
    pub removed_quantity_gram: u32,
    pub removed_cost_basis_krw: i64,
    pub remaining_position: GoldPosition,
}

pub fn remove_gold_cost_basis(
    position: GoldPosition,
    removed_quantity_gram: u32,
) -> Result<GoldCostRemoval, M2dAssetError> {
    validate_gold_position(position)?;
    if removed_quantity_gram == 0 {
        return Err(M2dAssetError::InvalidQuantity);
    }
    if removed_quantity_gram > position.quantity_gram {
        return Err(M2dAssetError::InsufficientQuantity);
    }

    let removed_cost_basis_krw = if removed_quantity_gram == position.quantity_gram {
        position.total_cost_basis_krw
    } else {
        checked_i128_to_i64(
            i128::from(position.total_cost_basis_krw)
                .checked_mul(i128::from(removed_quantity_gram))
                .and_then(|value| value.checked_div(i128::from(position.quantity_gram)))
                .ok_or(M2dAssetError::ArithmeticOverflow)?,
        )?
    };
    let remaining_position = GoldPosition {
        quantity_gram: checked_i128_to_u32(
            i128::from(position.quantity_gram)
                .checked_sub(i128::from(removed_quantity_gram))
                .ok_or(M2dAssetError::ArithmeticOverflow)?,
        )?,
        total_cost_basis_krw: checked_i128_to_i64(
            i128::from(position.total_cost_basis_krw)
                .checked_sub(i128::from(removed_cost_basis_krw))
                .ok_or(M2dAssetError::ArithmeticOverflow)?,
        )?,
    };

    Ok(GoldCostRemoval {
        removed_quantity_gram,
        removed_cost_basis_krw,
        remaining_position,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoldOrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoldOrderInput {
    pub side: GoldOrderSide,
    pub quantity_gram: u32,
    pub price_krw_per_gram: i64,
    pub account_cash_krw: i64,
    pub position: GoldPosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoldOrderPlan {
    pub side: GoldOrderSide,
    pub quantity_gram: u32,
    pub gross_amount_krw: i64,
    pub fee_krw: i64,
    pub tax_krw: i64,
    pub removed_cost_basis_krw: i64,
    pub realized_gain_loss_krw: i64,
    pub account_cash_after_krw: i64,
    pub position_after: GoldPosition,
}

pub fn plan_gold_order(
    terms: GoldProductTerms,
    input: GoldOrderInput,
) -> Result<GoldOrderPlan, M2dAssetError> {
    validate_gold_product_terms(terms)?;
    validate_gold_position(input.position)?;
    validate_non_negative_money(input.account_cash_krw)?;
    validate_positive_money(input.price_krw_per_gram)?;
    if input.quantity_gram == 0 {
        return Err(M2dAssetError::InvalidQuantity);
    }

    let gross_amount_krw = checked_i128_to_i64(
        i128::from(input.price_krw_per_gram)
            .checked_mul(i128::from(input.quantity_gram))
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;
    let fee_rate_ppm = match input.side {
        GoldOrderSide::Buy => terms.buy_fee_ppm,
        GoldOrderSide::Sell => terms.sell_fee_ppm,
    };
    let fee_krw = floor_rate_ppm(gross_amount_krw, fee_rate_ppm)?;
    let tax_rate_ppm = match input.side {
        GoldOrderSide::Buy => terms.buy_tax_ppm,
        GoldOrderSide::Sell => terms.sell_tax_ppm,
    };
    let tax_krw = floor_rate_ppm(gross_amount_krw, tax_rate_ppm)?;
    match input.side {
        GoldOrderSide::Buy => plan_gold_buy(input, gross_amount_krw, fee_krw, tax_krw),
        GoldOrderSide::Sell => plan_gold_sale(input, gross_amount_krw, fee_krw, tax_krw),
    }
}

fn plan_gold_buy(
    input: GoldOrderInput,
    gross_amount_krw: i64,
    fee_krw: i64,
    tax_krw: i64,
) -> Result<GoldOrderPlan, M2dAssetError> {
    let purchase_cost_krw = checked_i128_to_i64(
        i128::from(gross_amount_krw)
            .checked_add(i128::from(fee_krw))
            .and_then(|value| value.checked_add(i128::from(tax_krw)))
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;
    if purchase_cost_krw > input.account_cash_krw {
        return Err(M2dAssetError::InsufficientCash);
    }

    let account_cash_after_krw = checked_i128_to_i64(
        i128::from(input.account_cash_krw)
            .checked_sub(i128::from(purchase_cost_krw))
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;
    let position_after = GoldPosition {
        quantity_gram: checked_i128_to_u32(
            i128::from(input.position.quantity_gram)
                .checked_add(i128::from(input.quantity_gram))
                .ok_or(M2dAssetError::ArithmeticOverflow)?,
        )?,
        total_cost_basis_krw: checked_i128_to_i64(
            i128::from(input.position.total_cost_basis_krw)
                .checked_add(i128::from(purchase_cost_krw))
                .ok_or(M2dAssetError::ArithmeticOverflow)?,
        )?,
    };

    Ok(GoldOrderPlan {
        side: input.side,
        quantity_gram: input.quantity_gram,
        gross_amount_krw,
        fee_krw,
        tax_krw,
        removed_cost_basis_krw: 0,
        realized_gain_loss_krw: 0,
        account_cash_after_krw,
        position_after,
    })
}

fn plan_gold_sale(
    input: GoldOrderInput,
    gross_amount_krw: i64,
    fee_krw: i64,
    tax_krw: i64,
) -> Result<GoldOrderPlan, M2dAssetError> {
    let removal = remove_gold_cost_basis(input.position, input.quantity_gram)?;
    let cash_after = i128::from(input.account_cash_krw)
        .checked_add(i128::from(gross_amount_krw))
        .and_then(|value| value.checked_sub(i128::from(fee_krw)))
        .and_then(|value| value.checked_sub(i128::from(tax_krw)))
        .ok_or(M2dAssetError::ArithmeticOverflow)?;
    if cash_after < 0 {
        return Err(M2dAssetError::InsufficientCash);
    }
    let account_cash_after_krw = checked_i128_to_i64(cash_after)?;
    let realized_gain_loss_krw = checked_i128_to_i64(
        i128::from(gross_amount_krw)
            .checked_sub(i128::from(fee_krw))
            .and_then(|value| value.checked_sub(i128::from(tax_krw)))
            .and_then(|value| value.checked_sub(i128::from(removal.removed_cost_basis_krw)))
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;

    Ok(GoldOrderPlan {
        side: input.side,
        quantity_gram: input.quantity_gram,
        gross_amount_krw,
        fee_krw,
        tax_krw,
        removed_cost_basis_krw: removal.removed_cost_basis_krw,
        realized_gain_loss_krw,
        account_cash_after_krw,
        position_after: removal.remaining_position,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoldBarSize {
    Gram100,
    Gram1000,
}

impl GoldBarSize {
    pub const fn quantity_gram(self) -> u32 {
        match self {
            Self::Gram100 => 100,
            Self::Gram1000 => 1_000,
        }
    }

    pub const fn withdrawal_fee_krw(self, terms: GoldProductTerms) -> i64 {
        match self {
            Self::Gram100 => terms.withdrawal_fee_100g_krw,
            Self::Gram1000 => terms.withdrawal_fee_1kg_krw,
        }
    }
}

impl TryFrom<u32> for GoldBarSize {
    type Error = M2dAssetError;

    fn try_from(quantity_gram: u32) -> Result<Self, Self::Error> {
        match quantity_gram {
            100 => Ok(Self::Gram100),
            1_000 => Ok(Self::Gram1000),
            _ => Err(M2dAssetError::UnsupportedGoldBarSize),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoldWithdrawalInput {
    pub position: GoldPosition,
    pub account_cash_krw: i64,
    pub bar_size: GoldBarSize,
    pub bar_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalGoldHoldingDelta {
    pub bar_size: GoldBarSize,
    pub bar_count: u32,
    pub quantity_gram: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoldWithdrawalPlan {
    pub removed_quantity_gram: u32,
    pub removed_cost_basis_krw: i64,
    pub vat_krw: i64,
    pub fee_krw: i64,
    pub account_cash_after_krw: i64,
    pub position_after: GoldPosition,
    pub physical_holding_delta: PhysicalGoldHoldingDelta,
}

pub fn plan_gold_withdrawal(
    terms: GoldProductTerms,
    policy: GoldTaxPolicy,
    input: GoldWithdrawalInput,
) -> Result<GoldWithdrawalPlan, M2dAssetError> {
    validate_gold_product_terms(terms)?;
    validate_rate_ppm(policy.withdrawal_vat_rate_ppm)?;
    validate_gold_position(input.position)?;
    validate_non_negative_money(input.account_cash_krw)?;
    if input.bar_count == 0 {
        return Err(M2dAssetError::InvalidQuantity);
    }

    let removed_quantity_gram = checked_i128_to_u32(
        i128::from(input.bar_size.quantity_gram())
            .checked_mul(i128::from(input.bar_count))
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;
    let removal = remove_gold_cost_basis(input.position, removed_quantity_gram)?;
    let vat_krw = checked_i128_to_i64(
        i128::from(removal.removed_cost_basis_krw)
            .checked_mul(i128::from(policy.withdrawal_vat_rate_ppm))
            .and_then(|value| value.checked_div(RATE_SCALE_PPM))
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;
    let fee_krw = checked_i128_to_i64(
        i128::from(input.bar_size.withdrawal_fee_krw(terms))
            .checked_mul(i128::from(input.bar_count))
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;
    let required_cash_krw = checked_i128_to_i64(
        i128::from(vat_krw)
            .checked_add(i128::from(fee_krw))
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;
    if required_cash_krw > input.account_cash_krw {
        return Err(M2dAssetError::InsufficientCash);
    }
    let account_cash_after_krw = checked_i128_to_i64(
        i128::from(input.account_cash_krw)
            .checked_sub(i128::from(required_cash_krw))
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;

    Ok(GoldWithdrawalPlan {
        removed_quantity_gram,
        removed_cost_basis_krw: removal.removed_cost_basis_krw,
        vat_krw,
        fee_krw,
        account_cash_after_krw,
        position_after: removal.remaining_position,
        physical_holding_delta: PhysicalGoldHoldingDelta {
            bar_size: input.bar_size,
            bar_count: input.bar_count,
            quantity_gram: removed_quantity_gram,
        },
    })
}

fn validate_gold_position(position: GoldPosition) -> Result<(), M2dAssetError> {
    if position.total_cost_basis_krw < 0
        || (position.quantity_gram == 0 && position.total_cost_basis_krw != 0)
        || (position.quantity_gram > 0 && position.total_cost_basis_krw == 0)
    {
        return Err(M2dAssetError::InvalidPosition);
    }
    Ok(())
}

fn validate_gold_product_terms(terms: GoldProductTerms) -> Result<(), M2dAssetError> {
    validate_rate_ppm(terms.buy_fee_ppm)?;
    validate_rate_ppm(terms.sell_fee_ppm)?;
    validate_rate_ppm(terms.buy_tax_ppm)?;
    validate_rate_ppm(terms.sell_tax_ppm)?;
    validate_non_negative_money(terms.withdrawal_fee_100g_krw)?;
    validate_non_negative_money(terms.withdrawal_fee_1kg_krw)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PensionValueEventCause {
    DailyMarketToMarket,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PensionMarkToMarketInput {
    pub position_market_value_before_krw: i64,
    pub position_market_value_after_krw: i64,
    pub account_total_before_krw: i64,
    pub account_total_after_krw: i64,
    pub layers_before: PensionTaxLayers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PensionValueEventDraft {
    pub cause: PensionValueEventCause,
    pub position_market_value_before_krw: i64,
    pub position_market_value_after_krw: i64,
    pub account_total_before_krw: i64,
    pub account_total_after_krw: i64,
    pub value_change_krw: i64,
    pub layers_before: PensionTaxLayers,
    pub layers_after: PensionTaxLayers,
}

pub fn pension_layer_total(layers: PensionTaxLayers) -> Result<i64, M2dAssetError> {
    validate_pension_layers(layers)?;
    checked_i128_to_i64(
        i128::from(layers.tax_excluded_contribution_krw)
            .checked_add(i128::from(layers.deferred_retirement_income_krw))
            .and_then(|value| value.checked_add(i128::from(layers.credited_contribution_krw)))
            .and_then(|value| value.checked_add(i128::from(layers.earnings_krw)))
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )
}

pub fn draft_pension_mark_to_market_event(
    input: PensionMarkToMarketInput,
) -> Result<PensionValueEventDraft, M2dAssetError> {
    validate_non_negative_money(input.position_market_value_before_krw)?;
    validate_non_negative_money(input.position_market_value_after_krw)?;
    validate_non_negative_money(input.account_total_before_krw)?;
    validate_non_negative_money(input.account_total_after_krw)?;

    let layer_total_before_krw = pension_layer_total(input.layers_before)?;
    if layer_total_before_krw != input.account_total_before_krw {
        return Err(M2dAssetError::InvalidTaxLayerState);
    }
    let value_change = i128::from(input.position_market_value_after_krw)
        .checked_sub(i128::from(input.position_market_value_before_krw))
        .ok_or(M2dAssetError::ArithmeticOverflow)?;
    if value_change < 0 {
        let loss_krw = value_change
            .checked_neg()
            .ok_or(M2dAssetError::ArithmeticOverflow)?;
        if loss_krw > i128::from(layer_total_before_krw) {
            return Err(M2dAssetError::LossExceedsBalance);
        }
    }

    let expected_account_total_after_krw = checked_i128_to_i64(
        i128::from(input.account_total_before_krw)
            .checked_add(value_change)
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;
    if expected_account_total_after_krw != input.account_total_after_krw {
        return Err(M2dAssetError::InvalidTaxLayerState);
    }

    let layers_after = apply_pension_value_change(input.layers_before, value_change)?;
    if pension_layer_total(layers_after)? != input.account_total_after_krw {
        return Err(M2dAssetError::InvalidTaxLayerState);
    }

    Ok(PensionValueEventDraft {
        cause: PensionValueEventCause::DailyMarketToMarket,
        position_market_value_before_krw: input.position_market_value_before_krw,
        position_market_value_after_krw: input.position_market_value_after_krw,
        account_total_before_krw: input.account_total_before_krw,
        account_total_after_krw: input.account_total_after_krw,
        value_change_krw: checked_i128_to_i64(value_change)?,
        layers_before: input.layers_before,
        layers_after,
    })
}

fn apply_pension_value_change(
    mut layers: PensionTaxLayers,
    value_change: i128,
) -> Result<PensionTaxLayers, M2dAssetError> {
    if value_change >= 0 {
        layers.earnings_krw = checked_i128_to_i64(
            i128::from(layers.earnings_krw)
                .checked_add(value_change)
                .ok_or(M2dAssetError::ArithmeticOverflow)?,
        )?;
        return Ok(layers);
    }

    let mut remaining_loss_krw = value_change
        .checked_neg()
        .ok_or(M2dAssetError::ArithmeticOverflow)?;
    consume_pension_loss_layer(&mut layers.earnings_krw, &mut remaining_loss_krw)?;
    consume_pension_loss_layer(
        &mut layers.credited_contribution_krw,
        &mut remaining_loss_krw,
    )?;
    consume_pension_loss_layer(
        &mut layers.deferred_retirement_income_krw,
        &mut remaining_loss_krw,
    )?;
    consume_pension_loss_layer(
        &mut layers.tax_excluded_contribution_krw,
        &mut remaining_loss_krw,
    )?;
    if remaining_loss_krw != 0 {
        return Err(M2dAssetError::LossExceedsBalance);
    }
    Ok(layers)
}

fn consume_pension_loss_layer(
    layer_krw: &mut i64,
    remaining_loss_krw: &mut i128,
) -> Result<(), M2dAssetError> {
    let consumed_krw = i128::from(*layer_krw).min(*remaining_loss_krw);
    *layer_krw = checked_i128_to_i64(
        i128::from(*layer_krw)
            .checked_sub(consumed_krw)
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )?;
    *remaining_loss_krw = remaining_loss_krw
        .checked_sub(consumed_krw)
        .ok_or(M2dAssetError::ArithmeticOverflow)?;
    Ok(())
}

fn validate_pension_layers(layers: PensionTaxLayers) -> Result<(), M2dAssetError> {
    validate_non_negative_money(layers.tax_excluded_contribution_krw)?;
    validate_non_negative_money(layers.deferred_retirement_income_krw)?;
    validate_non_negative_money(layers.credited_contribution_krw)?;
    validate_non_negative_money(layers.earnings_krw)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PensionTradeSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PensionValuationBasisInput {
    pub basis_before_krw: i64,
    pub side: PensionTradeSide,
    pub execution_market_value_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PensionValuationBasisAdjustment {
    pub basis_before_krw: i64,
    pub adjustment_krw: i64,
    pub basis_after_krw: i64,
}

pub fn adjust_pension_valuation_basis(
    input: PensionValuationBasisInput,
) -> Result<PensionValuationBasisAdjustment, M2dAssetError> {
    validate_non_negative_money(input.basis_before_krw)?;
    validate_positive_money(input.execution_market_value_krw)?;

    let signed_adjustment = match input.side {
        PensionTradeSide::Buy => i128::from(input.execution_market_value_krw),
        PensionTradeSide::Sell => i128::from(input.execution_market_value_krw)
            .checked_neg()
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    };
    let basis_after = i128::from(input.basis_before_krw)
        .checked_add(signed_adjustment)
        .ok_or(M2dAssetError::ArithmeticOverflow)?;
    if basis_after < 0 {
        return Err(M2dAssetError::InsufficientValuationBasis);
    }

    Ok(PensionValuationBasisAdjustment {
        basis_before_krw: input.basis_before_krw,
        adjustment_krw: checked_i128_to_i64(signed_adjustment)?,
        basis_after_krw: checked_i128_to_i64(basis_after)?,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrpRiskPolicy {
    pub risk_asset_limit_ppm: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrpRiskExposureChange {
    Increased,
    NotIncreased,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrpPostOrderRiskInput {
    pub post_order_total_value_krw: i64,
    pub post_order_risk_asset_value_krw: i64,
    pub exposure_change: IrpRiskExposureChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrpPostOrderRiskRejection {
    RiskLimitExceeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrpPostOrderRiskDecision {
    Allowed {
        post_order_risk_ratio_ppm: i64,
    },
    Rejected {
        reason: IrpPostOrderRiskRejection,
        post_order_risk_ratio_ppm: i64,
    },
}

pub fn decide_irp_post_order_risk(
    policy: IrpRiskPolicy,
    input: IrpPostOrderRiskInput,
) -> Result<IrpPostOrderRiskDecision, M2dAssetError> {
    validate_rate_ppm(policy.risk_asset_limit_ppm)?;
    validate_non_negative_money(input.post_order_total_value_krw)?;
    validate_non_negative_money(input.post_order_risk_asset_value_krw)?;
    if input.post_order_risk_asset_value_krw > input.post_order_total_value_krw {
        return Err(M2dAssetError::InvalidPosition);
    }

    let post_order_risk_ratio_ppm = if input.post_order_total_value_krw == 0 {
        0
    } else {
        checked_i128_to_i64(
            i128::from(input.post_order_risk_asset_value_krw)
                .checked_mul(RATE_SCALE_PPM)
                .and_then(|value| value.checked_div(i128::from(input.post_order_total_value_krw)))
                .ok_or(M2dAssetError::ArithmeticOverflow)?,
        )?
    };
    let exceeds_limit = input.exposure_change == IrpRiskExposureChange::Increased
        && i128::from(input.post_order_risk_asset_value_krw)
            .checked_mul(RATE_SCALE_PPM)
            .ok_or(M2dAssetError::ArithmeticOverflow)?
            > i128::from(input.post_order_total_value_krw)
                .checked_mul(i128::from(policy.risk_asset_limit_ppm))
                .ok_or(M2dAssetError::ArithmeticOverflow)?;

    if exceeds_limit {
        Ok(IrpPostOrderRiskDecision::Rejected {
            reason: IrpPostOrderRiskRejection::RiskLimitExceeded,
            post_order_risk_ratio_ppm,
        })
    } else {
        Ok(IrpPostOrderRiskDecision::Allowed {
            post_order_risk_ratio_ppm,
        })
    }
}

fn validate_non_negative_money(value: i64) -> Result<(), M2dAssetError> {
    if value < 0 {
        return Err(M2dAssetError::InvalidMoney);
    }
    Ok(())
}

fn validate_positive_money(value: i64) -> Result<(), M2dAssetError> {
    if value <= 0 {
        return Err(M2dAssetError::InvalidMoney);
    }
    Ok(())
}

fn validate_rate_ppm(rate_ppm: i64) -> Result<(), M2dAssetError> {
    if !(0..=i64::try_from(RATE_SCALE_PPM).map_err(|_| M2dAssetError::ArithmeticOverflow)?)
        .contains(&rate_ppm)
    {
        return Err(M2dAssetError::InvalidRate);
    }
    Ok(())
}

fn floor_rate_ppm(amount_krw: i64, rate_ppm: i64) -> Result<i64, M2dAssetError> {
    validate_non_negative_money(amount_krw)?;
    validate_rate_ppm(rate_ppm)?;
    checked_i128_to_i64(
        i128::from(amount_krw)
            .checked_mul(i128::from(rate_ppm))
            .and_then(|value| value.checked_div(RATE_SCALE_PPM))
            .ok_or(M2dAssetError::ArithmeticOverflow)?,
    )
}

fn round_half_up_non_negative(numerator: i128, denominator: i128) -> Result<i128, M2dAssetError> {
    if numerator < 0 || denominator <= 0 {
        return Err(M2dAssetError::InvalidMoney);
    }
    numerator
        .checked_add(
            denominator
                .checked_div(2)
                .ok_or(M2dAssetError::ArithmeticOverflow)?,
        )
        .and_then(|value| value.checked_div(denominator))
        .ok_or(M2dAssetError::ArithmeticOverflow)
}

fn checked_i128_to_i64(value: i128) -> Result<i64, M2dAssetError> {
    i64::try_from(value).map_err(|_| M2dAssetError::ArithmeticOverflow)
}

fn checked_i128_to_u32(value: i128) -> Result<u32, M2dAssetError> {
    u32::try_from(value).map_err(|_| M2dAssetError::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use time::{Date, Month};

    use super::*;

    fn date(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("유효한 테스트 날짜여야 한다")
    }

    fn pension_layers(
        tax_excluded_contribution_krw: i64,
        deferred_retirement_income_krw: i64,
        credited_contribution_krw: i64,
        earnings_krw: i64,
    ) -> PensionTaxLayers {
        PensionTaxLayers {
            tax_excluded_contribution_krw,
            deferred_retirement_income_krw,
            credited_contribution_krw,
            earnings_krw,
        }
    }

    fn llx_terms() -> LlxProductTerms {
        LlxProductTerms {
            annual_management_fee_ppm: 1_500,
            annual_distribution_rate_ppm: 20_000,
            day_count_denominator: 365,
        }
    }

    fn bond_terms(term: BondTerm) -> BondProductTerms {
        BondProductTerms {
            term,
            face_value_krw: 10_000,
            maximum_order_units: 100_000,
            maximum_position_units: 100_000,
            coupon_rate_step_bp: 25,
            buy_fee_rate_ppm: 0,
            sell_fee_rate_ppm: 0,
            buy_tax_rate_ppm: 0,
            sell_tax_rate_ppm: 0,
        }
    }

    fn gold_terms() -> GoldProductTerms {
        GoldProductTerms {
            buy_fee_ppm: 50_000,
            sell_fee_ppm: 0,
            buy_tax_ppm: 0,
            sell_tax_ppm: 0,
            withdrawal_fee_100g_krw: 20_000,
            withdrawal_fee_1kg_krw: 100_000,
        }
    }

    fn gold_tax_policy() -> GoldTaxPolicy {
        GoldTaxPolicy {
            withdrawal_vat_rate_ppm: 100_000,
        }
    }

    fn irp_risk_policy() -> IrpRiskPolicy {
        IrpRiskPolicy {
            risk_asset_limit_ppm: 700_000,
        }
    }

    mod llx_fee_rule {
        use super::*;

        mod context_actual_365 {
            use super::*;

            #[test]
            fn given_365_open_days_when_accrued_then_exact_annual_fee_is_applied() {
                let mut state = LlxFeeState::default();
                let mut applied_fee_ppm = 0_i64;

                for _ in 0..365 {
                    let accrual = accrue_llx_management_fee(llx_terms(), state, true)
                        .expect("유효한 보수 상태는 계산되어야 한다");
                    applied_fee_ppm += accrual.applied_fee_ppm;
                    state = accrual.next_state;
                }

                assert_eq!(applied_fee_ppm, 1_500);
                assert_eq!(state, LlxFeeState::default());
            }
        }

        mod context_market_closure {
            use super::*;

            #[test]
            fn given_two_closed_days_when_market_opens_then_pending_fee_is_applied_once() {
                let first = accrue_llx_management_fee(llx_terms(), LlxFeeState::default(), false)
                    .expect("첫 휴장일 보수는 계산되어야 한다");
                let second = accrue_llx_management_fee(llx_terms(), first.next_state, false)
                    .expect("둘째 휴장일 보수는 계산되어야 한다");

                let opened = accrue_llx_management_fee(llx_terms(), second.next_state, true)
                    .expect("개장일 누적 보수는 계산되어야 한다");

                assert_eq!(opened.accrued_today_ppm, 4);
                assert_eq!(opened.applied_fee_ppm, 12);
                assert_eq!(opened.next_state.pending_fee_ppm, 0);
                assert_eq!(opened.next_state.remainder_numerator, 120);
            }

            #[test]
            fn given_invalid_remainder_when_accrued_then_typed_error_is_returned() {
                let state = LlxFeeState {
                    remainder_numerator: 365,
                    pending_fee_ppm: 0,
                };

                let result = accrue_llx_management_fee(llx_terms(), state, true);

                assert_eq!(result, Err(M2dAssetError::InvalidFeeState));
            }
        }
    }

    mod llx_distribution_rule {
        use super::*;

        mod context_quarter_record_date {
            use super::*;

            #[test]
            fn given_next_open_day_in_april_when_decided_then_march_open_day_is_record_date() {
                let input = LlxQuarterRecordDateInput {
                    current_date: date(2026, Month::March, 31),
                    market_open: true,
                    next_open_date: Some(date(2026, Month::April, 1)),
                };

                let result =
                    is_llx_quarter_record_date(input).expect("분기 마지막 개장일을 판정해야 한다");

                assert!(result);
            }

            #[test]
            fn given_later_open_day_in_same_month_when_decided_then_it_is_not_record_date() {
                let input = LlxQuarterRecordDateInput {
                    current_date: date(2026, Month::June, 29),
                    market_open: true,
                    next_open_date: Some(date(2026, Month::June, 30)),
                };

                let result = is_llx_quarter_record_date(input)
                    .expect("같은 달의 다음 개장일을 비교해야 한다");

                assert!(!result);
            }

            #[test]
            fn given_closed_quarter_end_when_decided_then_it_is_not_record_date() {
                let input = LlxQuarterRecordDateInput {
                    current_date: date(2026, Month::September, 30),
                    market_open: false,
                    next_open_date: None,
                };

                let result = is_llx_quarter_record_date(input)
                    .expect("휴장일은 다음 개장일 없이 판정할 수 있어야 한다");

                assert!(!result);
            }
        }

        mod context_t_plus_two {
            use super::*;

            #[test]
            fn given_following_open_sessions_when_selected_then_second_session_is_payment_date() {
                let record_date = date(2026, Month::September, 30);
                let open_dates = [
                    date(2026, Month::October, 1),
                    date(2026, Month::October, 5),
                    date(2026, Month::October, 6),
                ];

                let payment_date = llx_t_plus_two_open_date(record_date, &open_dates)
                    .expect("두 번째 후속 개장일을 찾아야 한다");

                assert_eq!(payment_date, date(2026, Month::October, 5));
            }

            #[test]
            fn given_unsorted_open_sessions_when_selected_then_calendar_error_is_returned() {
                let record_date = date(2026, Month::September, 30);
                let open_dates = [date(2026, Month::October, 2), date(2026, Month::October, 1)];

                let result = llx_t_plus_two_open_date(record_date, &open_dates);

                assert_eq!(result, Err(M2dAssetError::InvalidOpenSessionCalendar));
            }
        }

        mod context_entitlement {
            use super::*;

            #[test]
            fn given_fractional_distribution_when_drafted_then_per_share_amount_is_floored() {
                let input = LlxEntitlementInput {
                    record_date: date(2026, Month::March, 31),
                    payment_date: date(2026, Month::April, 2),
                    record_quantity: 10,
                    record_close_krw: 100_001,
                };

                let draft = draft_llx_distribution_entitlement(llx_terms(), input)
                    .expect("유효한 분배금 권리를 생성해야 한다");

                assert_eq!(draft.per_share_distribution_krw, 500);
                assert_eq!(draft.gross_distribution_krw, 5_000);
                assert_eq!(
                    draft.movement,
                    LlxDistributionMovement::Cash { amount_krw: 5_000 }
                );
            }

            #[test]
            fn given_sub_won_distribution_when_drafted_then_no_movement_is_explicit() {
                let input = LlxEntitlementInput {
                    record_date: date(2026, Month::June, 30),
                    payment_date: date(2026, Month::July, 2),
                    record_quantity: 3,
                    record_close_krw: 199,
                };

                let draft = draft_llx_distribution_entitlement(llx_terms(), input)
                    .expect("0원 권리도 불변 초안으로 생성되어야 한다");

                assert_eq!(draft.gross_distribution_krw, 0);
                assert_eq!(
                    draft.movement,
                    LlxDistributionMovement::NoMovement {
                        reason: LlxNoMovementReason::ZeroDistribution,
                    }
                );
            }
        }
    }

    mod bond_series_rule {
        use super::*;

        mod context_coupon_rate {
            use super::*;

            #[test]
            fn given_values_around_midpoint_when_rounded_then_nearest_25bp_boundary_is_used() {
                let below = round_bond_coupon_rate_bp(bond_terms(BondTerm::Years3), 312)
                    .expect("중간값 아래 금리를 반올림해야 한다");
                let above = round_bond_coupon_rate_bp(bond_terms(BondTerm::Years3), 313)
                    .expect("중간값 위 금리를 반올림해야 한다");

                assert_eq!(below, 300);
                assert_eq!(above, 325);
            }
        }

        mod context_calendar_schedule {
            use super::*;

            #[test]
            fn given_month_end_issue_when_scheduled_then_each_coupon_anchors_to_issue_day() {
                let series = create_bond_series(
                    bond_terms(BondTerm::Years3),
                    date(2026, Month::August, 31),
                    325,
                )
                .expect("3년물 일정을 생성해야 한다");

                assert_eq!(
                    series.cash_flows[0].payment_date,
                    date(2027, Month::February, 28)
                );
                assert_eq!(
                    series.cash_flows[1].payment_date,
                    date(2027, Month::August, 31)
                );
            }

            #[test]
            fn given_odd_annual_coupon_when_scheduled_then_remainder_goes_to_second_half() {
                let series = create_bond_series(
                    bond_terms(BondTerm::Years3),
                    date(2026, Month::January, 15),
                    325,
                )
                .expect("홀수 연 쿠폰 일정을 생성해야 한다");

                assert_eq!(series.annual_coupon_krw, 325);
                assert_eq!(series.cash_flows[0].coupon_krw, 162);
                assert_eq!(series.cash_flows[1].coupon_krw, 163);
                assert_eq!(series.cash_flows[5].coupon_krw, 163);
                assert_eq!(series.cash_flows[5].principal_krw, 10_000);
            }

            #[test]
            fn given_supported_terms_when_scheduled_then_three_and_ten_year_counts_are_exact() {
                let issued_on = date(2026, Month::January, 2);

                let three_year = create_bond_series(bond_terms(BondTerm::Years3), issued_on, 300)
                    .expect("3년물을 생성해야 한다");
                let ten_year = create_bond_series(bond_terms(BondTerm::Years10), issued_on, 300)
                    .expect("10년물을 생성해야 한다");

                assert_eq!(three_year.cash_flows.len(), 6);
                assert_eq!(ten_year.cash_flows.len(), 20);
            }
        }

        mod context_dirty_price {
            use super::*;

            #[test]
            fn given_future_cash_flow_when_priced_then_each_value_uses_half_up_rounding() {
                let valuation_date = date(2026, Month::January, 1);
                let cash_flows = [BondCashFlow {
                    payment_date: date(2027, Month::January, 1),
                    coupon_krw: 0,
                    principal_krw: 10_000,
                }];

                let price = dirty_bond_price_krw(valuation_date, 500, &cash_flows)
                    .expect("dirty price를 계산해야 한다");

                assert_eq!(price, 9_524);
            }

            #[test]
            fn given_cash_flow_due_today_when_priced_then_today_flow_is_excluded() {
                let valuation_date = date(2026, Month::July, 1);
                let cash_flows = [
                    BondCashFlow {
                        payment_date: valuation_date,
                        coupon_krw: 100,
                        principal_krw: 0,
                    },
                    BondCashFlow {
                        payment_date: date(2026, Month::July, 2),
                        coupon_krw: 200,
                        principal_krw: 0,
                    },
                ];

                let price = dirty_bond_price_krw(valuation_date, 0, &cash_flows)
                    .expect("당일 지급분을 제외해야 한다");

                assert_eq!(price, 200);
            }

            #[test]
            fn given_rate_increase_when_priced_then_ten_year_price_falls_more_than_three_year() {
                let issued_on = date(2026, Month::January, 2);
                let three_year = create_bond_series(bond_terms(BondTerm::Years3), issued_on, 300)
                    .expect("3년물을 생성해야 한다");
                let ten_year = create_bond_series(bond_terms(BondTerm::Years10), issued_on, 300)
                    .expect("10년물을 생성해야 한다");

                let three_before = dirty_bond_price_krw(issued_on, 300, &three_year.cash_flows)
                    .expect("3년물 기준 가격을 계산해야 한다");
                let three_after = dirty_bond_price_krw(issued_on, 400, &three_year.cash_flows)
                    .expect("3년물 상승 금리 가격을 계산해야 한다");
                let ten_before = dirty_bond_price_krw(issued_on, 300, &ten_year.cash_flows)
                    .expect("10년물 기준 가격을 계산해야 한다");
                let ten_after = dirty_bond_price_krw(issued_on, 400, &ten_year.cash_flows)
                    .expect("10년물 상승 금리 가격을 계산해야 한다");

                assert!(ten_before - ten_after > three_before - three_after);
            }

            #[test]
            fn given_unrepresentable_sum_when_priced_then_overflow_error_is_returned() {
                let valuation_date = date(2026, Month::January, 1);
                let cash_flows = [
                    BondCashFlow {
                        payment_date: date(2026, Month::January, 2),
                        coupon_krw: i64::MAX,
                        principal_krw: 0,
                    },
                    BondCashFlow {
                        payment_date: date(2026, Month::January, 3),
                        coupon_krw: i64::MAX,
                        principal_krw: 0,
                    },
                ];

                let result = dirty_bond_price_krw(valuation_date, 0, &cash_flows);

                assert_eq!(result, Err(M2dAssetError::ArithmeticOverflow));
            }
        }
    }

    mod bond_execution_rule {
        use super::*;

        fn lot(units: u32, cost_basis_krw: i64) -> BondLot {
            BondLot {
                units,
                cost_basis_krw,
            }
        }

        mod context_position_limit {
            use super::*;

            #[test]
            fn given_position_at_limit_when_buying_more_then_limit_error_is_returned() {
                let input = BondExecutionInput {
                    side: BondOrderSide::Buy,
                    units: 1,
                    dirty_price_krw: 10_000,
                    current_position_units: 100_000,
                    lots: vec![lot(100_000, 1_000_000_000)],
                };

                let result = plan_bond_execution(bond_terms(BondTerm::Years3), input);

                assert_eq!(result, Err(M2dAssetError::PositionLimitExceeded));
            }

            #[test]
            fn given_order_above_limit_when_planned_then_quantity_error_is_returned() {
                let input = BondExecutionInput {
                    side: BondOrderSide::Buy,
                    units: 100_001,
                    dirty_price_krw: 10_000,
                    current_position_units: 0,
                    lots: Vec::new(),
                };

                let result = plan_bond_execution(bond_terms(BondTerm::Years3), input);

                assert_eq!(result, Err(M2dAssetError::InvalidQuantity));
            }
        }

        mod context_fifo_sale {
            use super::*;

            #[test]
            fn given_multiple_lots_when_partially_sold_then_oldest_costs_are_removed_first() {
                let input = BondExecutionInput {
                    side: BondOrderSide::Sell,
                    units: 4,
                    dirty_price_krw: 50,
                    current_position_units: 5,
                    lots: vec![lot(3, 100), lot(2, 90)],
                };

                let plan = plan_bond_execution(bond_terms(BondTerm::Years3), input)
                    .expect("FIFO 매도 계획을 생성해야 한다");

                assert_eq!(plan.removed_cost_basis_krw, 145);
                assert_eq!(plan.realized_gain_loss_krw, 55);
                assert_eq!(plan.remaining_lots, vec![lot(1, 45)]);
                assert_eq!(
                    plan.removals,
                    vec![
                        BondLotRemoval {
                            lot_index: 0,
                            removed_units: 3,
                            removed_cost_basis_krw: 100,
                        },
                        BondLotRemoval {
                            lot_index: 1,
                            removed_units: 1,
                            removed_cost_basis_krw: 45,
                        },
                    ]
                );
            }

            #[test]
            fn given_entire_lot_when_sold_then_all_remaining_basis_is_removed_exactly() {
                let input = BondExecutionInput {
                    side: BondOrderSide::Sell,
                    units: 3,
                    dirty_price_krw: 34,
                    current_position_units: 3,
                    lots: vec![lot(3, 100)],
                };

                let plan = plan_bond_execution(bond_terms(BondTerm::Years3), input)
                    .expect("전량 매도 계획을 생성해야 한다");

                assert_eq!(plan.removed_cost_basis_krw, 100);
                assert_eq!(plan.realized_gain_loss_krw, 2);
                assert!(plan.remaining_lots.is_empty());
            }

            #[test]
            fn given_insufficient_units_when_sold_then_quantity_error_is_returned() {
                let input = BondExecutionInput {
                    side: BondOrderSide::Sell,
                    units: 2,
                    dirty_price_krw: 10_000,
                    current_position_units: 1,
                    lots: vec![lot(1, 10_000)],
                };

                let result = plan_bond_execution(bond_terms(BondTerm::Years3), input);

                assert_eq!(result, Err(M2dAssetError::InsufficientQuantity));
            }
        }
    }

    mod gold_position_rule {
        use super::*;

        mod context_moving_average_removal {
            use super::*;

            #[test]
            fn given_partial_quantity_when_removed_then_basis_is_floored() {
                let position = GoldPosition {
                    quantity_gram: 3,
                    total_cost_basis_krw: 1_000,
                };

                let removal =
                    remove_gold_cost_basis(position, 1).expect("부분 제거원가를 계산해야 한다");

                assert_eq!(removal.removed_cost_basis_krw, 333);
                assert_eq!(
                    removal.remaining_position,
                    GoldPosition {
                        quantity_gram: 2,
                        total_cost_basis_krw: 667,
                    }
                );
            }

            #[test]
            fn given_entire_quantity_when_removed_then_remaining_basis_is_exactly_zero() {
                let position = GoldPosition {
                    quantity_gram: 3,
                    total_cost_basis_krw: 1_000,
                };

                let removal =
                    remove_gold_cost_basis(position, 3).expect("전량 제거원가를 계산해야 한다");

                assert_eq!(removal.removed_cost_basis_krw, 1_000);
                assert_eq!(removal.remaining_position, GoldPosition::default());
            }

            #[test]
            fn given_positive_quantity_without_basis_when_removed_then_position_error_is_returned()
            {
                let position = GoldPosition {
                    quantity_gram: 1,
                    total_cost_basis_krw: 0,
                };

                let result = remove_gold_cost_basis(position, 1);

                assert_eq!(result, Err(M2dAssetError::InvalidPosition));
            }
        }

        mod context_order_plan {
            use super::*;

            #[test]
            fn given_sufficient_cash_when_buying_then_cash_and_basis_include_fee() {
                let input = GoldOrderInput {
                    side: GoldOrderSide::Buy,
                    quantity_gram: 10,
                    price_krw_per_gram: 100,
                    account_cash_krw: 2_000,
                    position: GoldPosition::default(),
                };

                let plan =
                    plan_gold_order(gold_terms(), input).expect("금 매수 계획을 생성해야 한다");

                assert_eq!(plan.gross_amount_krw, 1_000);
                assert_eq!(plan.account_cash_after_krw, 950);
                assert_eq!(plan.position_after.total_cost_basis_krw, 1_050);
            }

            #[test]
            fn given_average_cost_position_when_selling_then_realized_gain_uses_removed_basis() {
                let input = GoldOrderInput {
                    side: GoldOrderSide::Sell,
                    quantity_gram: 4,
                    price_krw_per_gram: 120,
                    account_cash_krw: 100,
                    position: GoldPosition {
                        quantity_gram: 10,
                        total_cost_basis_krw: 1_050,
                    },
                };

                let plan =
                    plan_gold_order(gold_terms(), input).expect("금 매도 계획을 생성해야 한다");

                assert_eq!(plan.removed_cost_basis_krw, 420);
                assert_eq!(plan.realized_gain_loss_krw, 60);
                assert_eq!(plan.account_cash_after_krw, 580);
            }

            #[test]
            fn given_insufficient_cash_when_buying_then_cash_error_is_returned() {
                let input = GoldOrderInput {
                    side: GoldOrderSide::Buy,
                    quantity_gram: 10,
                    price_krw_per_gram: 100,
                    account_cash_krw: 1_000,
                    position: GoldPosition::default(),
                };

                let result = plan_gold_order(gold_terms(), input);

                assert_eq!(result, Err(M2dAssetError::InsufficientCash));
            }

            #[test]
            fn given_unrepresentable_gross_when_buying_then_overflow_error_is_returned() {
                let input = GoldOrderInput {
                    side: GoldOrderSide::Buy,
                    quantity_gram: 2,
                    price_krw_per_gram: i64::MAX,
                    account_cash_krw: i64::MAX,
                    position: GoldPosition::default(),
                };

                let result = plan_gold_order(gold_terms(), input);

                assert_eq!(result, Err(M2dAssetError::ArithmeticOverflow));
            }

            #[test]
            fn given_catalog_buy_tax_when_buying_then_tax_is_included_in_cash_and_basis() {
                let mut terms = gold_terms();
                terms.buy_fee_ppm = 0;
                terms.buy_tax_ppm = 100_000;
                let input = GoldOrderInput {
                    side: GoldOrderSide::Buy,
                    quantity_gram: 10,
                    price_krw_per_gram: 100,
                    account_cash_krw: 1_100,
                    position: GoldPosition::default(),
                };

                let plan = plan_gold_order(terms, input)
                    .expect("catalog 매수세를 적용한 계획을 생성해야 한다");

                assert_eq!(plan.tax_krw, 100);
                assert_eq!(plan.account_cash_after_krw, 0);
                assert_eq!(plan.position_after.total_cost_basis_krw, 1_100);
            }

            #[test]
            fn given_catalog_sell_tax_when_selling_then_tax_reduces_cash_and_realized_gain() {
                let mut terms = gold_terms();
                terms.buy_fee_ppm = 0;
                terms.sell_tax_ppm = 100_000;
                let input = GoldOrderInput {
                    side: GoldOrderSide::Sell,
                    quantity_gram: 4,
                    price_krw_per_gram: 120,
                    account_cash_krw: 0,
                    position: GoldPosition {
                        quantity_gram: 10,
                        total_cost_basis_krw: 1_000,
                    },
                };

                let plan = plan_gold_order(terms, input)
                    .expect("catalog 매도세를 적용한 계획을 생성해야 한다");

                assert_eq!(plan.tax_krw, 48);
                assert_eq!(plan.account_cash_after_krw, 432);
                assert_eq!(plan.realized_gain_loss_krw, 32);
            }

            #[test]
            fn given_sub_won_trade_tax_when_buying_then_tax_is_floored_to_zero() {
                let mut terms = gold_terms();
                terms.buy_fee_ppm = 0;
                terms.buy_tax_ppm = 1_000;
                let input = GoldOrderInput {
                    side: GoldOrderSide::Buy,
                    quantity_gram: 3,
                    price_krw_per_gram: 333,
                    account_cash_krw: 999,
                    position: GoldPosition::default(),
                };

                let plan = plan_gold_order(terms, input).expect("원 미만 매수세를 내림해야 한다");

                assert_eq!(plan.tax_krw, 0);
                assert_eq!(plan.account_cash_after_krw, 0);
            }
        }
    }

    mod gold_withdrawal_rule {
        use super::*;

        mod context_supported_bars {
            use super::*;

            #[test]
            fn given_100g_bar_when_withdrawn_then_vat_is_floored_and_fixed_fee_is_charged() {
                let input = GoldWithdrawalInput {
                    position: GoldPosition {
                        quantity_gram: 200,
                        total_cost_basis_krw: 24_000_001,
                    },
                    account_cash_krw: 1_220_000,
                    bar_size: GoldBarSize::Gram100,
                    bar_count: 1,
                };

                let plan = plan_gold_withdrawal(gold_terms(), gold_tax_policy(), input)
                    .expect("100g 인출 계획을 생성해야 한다");

                assert_eq!(plan.removed_cost_basis_krw, 12_000_000);
                assert_eq!(plan.vat_krw, 1_200_000);
                assert_eq!(plan.fee_krw, 20_000);
                assert_eq!(plan.account_cash_after_krw, 0);
                assert_eq!(
                    plan.physical_holding_delta,
                    PhysicalGoldHoldingDelta {
                        bar_size: GoldBarSize::Gram100,
                        bar_count: 1,
                        quantity_gram: 100,
                    }
                );
            }

            #[test]
            fn given_1kg_bar_when_withdrawn_then_1kg_fixed_fee_is_charged() {
                let input = GoldWithdrawalInput {
                    position: GoldPosition {
                        quantity_gram: 1_000,
                        total_cost_basis_krw: 1_000_000,
                    },
                    account_cash_krw: 200_000,
                    bar_size: GoldBarSize::Gram1000,
                    bar_count: 1,
                };

                let plan = plan_gold_withdrawal(gold_terms(), gold_tax_policy(), input)
                    .expect("1kg 인출 계획을 생성해야 한다");

                assert_eq!(plan.vat_krw, 100_000);
                assert_eq!(plan.fee_krw, 100_000);
            }

            #[test]
            fn given_unsupported_bar_size_when_converted_then_typed_error_is_returned() {
                let result = GoldBarSize::try_from(500);

                assert_eq!(result, Err(M2dAssetError::UnsupportedGoldBarSize));
            }
        }

        mod context_atomic_validation {
            use super::*;

            #[test]
            fn given_insufficient_cash_when_withdrawn_then_cash_error_is_returned() {
                let input = GoldWithdrawalInput {
                    position: GoldPosition {
                        quantity_gram: 100,
                        total_cost_basis_krw: 1_000_000,
                    },
                    account_cash_krw: 119_999,
                    bar_size: GoldBarSize::Gram100,
                    bar_count: 1,
                };

                let result = plan_gold_withdrawal(gold_terms(), gold_tax_policy(), input);

                assert_eq!(result, Err(M2dAssetError::InsufficientCash));
            }

            #[test]
            fn given_insufficient_gold_when_withdrawn_then_quantity_error_is_returned() {
                let input = GoldWithdrawalInput {
                    position: GoldPosition {
                        quantity_gram: 99,
                        total_cost_basis_krw: 990_000,
                    },
                    account_cash_krw: 200_000,
                    bar_size: GoldBarSize::Gram100,
                    bar_count: 1,
                };

                let result = plan_gold_withdrawal(gold_terms(), gold_tax_policy(), input);

                assert_eq!(result, Err(M2dAssetError::InsufficientQuantity));
            }
        }
    }

    mod pension_mark_to_market_rule {
        use super::*;

        fn input_for_change(
            change_krw: i64,
            layers_before: PensionTaxLayers,
        ) -> PensionMarkToMarketInput {
            let account_total_before_krw =
                pension_layer_total(layers_before).expect("유효한 세원층 합계를 계산해야 한다");
            PensionMarkToMarketInput {
                position_market_value_before_krw: 100,
                position_market_value_after_krw: 100 + change_krw,
                account_total_before_krw,
                account_total_after_krw: account_total_before_krw + change_krw,
                layers_before,
            }
        }

        mod context_market_gain {
            use super::*;

            #[test]
            fn given_positive_value_change_when_applied_then_gain_is_added_to_earnings() {
                let input = input_for_change(20, pension_layers(400, 300, 200, 100));

                let event = draft_pension_mark_to_market_event(input)
                    .expect("양의 시가손익 이벤트를 생성해야 한다");

                assert_eq!(event.value_change_krw, 20);
                assert_eq!(event.layers_after, pension_layers(400, 300, 200, 120));
                assert_eq!(event.cause, PensionValueEventCause::DailyMarketToMarket);
            }
        }

        mod context_loss_waterfall {
            use super::*;

            #[test]
            fn given_loss_below_earnings_when_applied_then_only_earnings_decrease() {
                let input = input_for_change(-9, pension_layers(40, 30, 20, 10));

                let event = draft_pension_mark_to_market_event(input)
                    .expect("earnings 이내 손실을 적용해야 한다");

                assert_eq!(event.layers_after, pension_layers(40, 30, 20, 1));
            }

            #[test]
            fn given_loss_equal_to_earnings_when_applied_then_earnings_reaches_zero() {
                let input = input_for_change(-10, pension_layers(40, 30, 20, 10));

                let event = draft_pension_mark_to_market_event(input)
                    .expect("earnings 경계 손실을 적용해야 한다");

                assert_eq!(event.layers_after, pension_layers(40, 30, 20, 0));
            }

            #[test]
            fn given_loss_above_earnings_when_applied_then_credited_layer_is_next() {
                let input = input_for_change(-11, pension_layers(40, 30, 20, 10));

                let event = draft_pension_mark_to_market_event(input)
                    .expect("earnings 초과 손실을 적용해야 한다");

                assert_eq!(event.layers_after, pension_layers(40, 30, 19, 0));
            }

            #[test]
            fn given_loss_across_all_layers_when_applied_then_order_is_preserved() {
                let input = input_for_change(-65, pension_layers(40, 30, 20, 10));

                let event = draft_pension_mark_to_market_event(input)
                    .expect("네 층 손실 순서를 적용해야 한다");

                assert_eq!(event.layers_after, pension_layers(35, 0, 0, 0));
            }

            #[test]
            fn given_loss_beyond_account_when_applied_then_balance_error_is_returned() {
                let input = PensionMarkToMarketInput {
                    position_market_value_before_krw: 100,
                    position_market_value_after_krw: 0,
                    account_total_before_krw: 90,
                    account_total_after_krw: 0,
                    layers_before: pension_layers(40, 30, 10, 10),
                };

                let result = draft_pension_mark_to_market_event(input);

                assert_eq!(result, Err(M2dAssetError::LossExceedsBalance));
            }
        }

        mod context_state_integrity {
            use super::*;

            #[test]
            fn given_layer_total_mismatch_when_applied_then_integrity_error_is_returned() {
                let input = PensionMarkToMarketInput {
                    position_market_value_before_krw: 50,
                    position_market_value_after_krw: 50,
                    account_total_before_krw: 101,
                    account_total_after_krw: 101,
                    layers_before: pension_layers(40, 30, 20, 10),
                };

                let result = draft_pension_mark_to_market_event(input);

                assert_eq!(result, Err(M2dAssetError::InvalidTaxLayerState));
            }

            #[test]
            fn given_post_value_mismatch_when_applied_then_integrity_error_is_returned() {
                let input = PensionMarkToMarketInput {
                    position_market_value_before_krw: 50,
                    position_market_value_after_krw: 60,
                    account_total_before_krw: 100,
                    account_total_after_krw: 109,
                    layers_before: pension_layers(40, 30, 20, 10),
                };

                let result = draft_pension_mark_to_market_event(input);

                assert_eq!(result, Err(M2dAssetError::InvalidTaxLayerState));
            }
        }

        mod context_trade_basis_adjustment {
            use super::*;

            #[test]
            fn given_internal_purchase_when_basis_is_adjusted_then_cash_conversion_has_no_gain() {
                let adjustment = adjust_pension_valuation_basis(PensionValuationBasisInput {
                    basis_before_krw: 100,
                    side: PensionTradeSide::Buy,
                    execution_market_value_krw: 40,
                })
                .expect("매수 체결가만큼 평가 기준을 늘려야 한다");
                let input = PensionMarkToMarketInput {
                    position_market_value_before_krw: adjustment.basis_after_krw,
                    position_market_value_after_krw: 140,
                    account_total_before_krw: 1_000,
                    account_total_after_krw: 1_000,
                    layers_before: pension_layers(400, 300, 200, 100),
                };

                let event = draft_pension_mark_to_market_event(input)
                    .expect("현금과 포지션 전환은 시가손익이 아니어야 한다");

                assert_eq!(event.value_change_krw, 0);
                assert_eq!(event.layers_after, input.layers_before);
            }

            #[test]
            fn given_sale_above_basis_when_adjusted_then_basis_error_is_returned() {
                let input = PensionValuationBasisInput {
                    basis_before_krw: 100,
                    side: PensionTradeSide::Sell,
                    execution_market_value_krw: 101,
                };

                let result = adjust_pension_valuation_basis(input);

                assert_eq!(result, Err(M2dAssetError::InsufficientValuationBasis));
            }
        }
    }

    mod irp_risk_rule {
        use super::*;

        mod context_post_order_ratio {
            use super::*;

            #[test]
            fn given_exactly_70_percent_when_decided_then_order_is_allowed() {
                let input = IrpPostOrderRiskInput {
                    post_order_total_value_krw: 100,
                    post_order_risk_asset_value_krw: 70,
                    exposure_change: IrpRiskExposureChange::Increased,
                };

                let decision = decide_irp_post_order_risk(irp_risk_policy(), input)
                    .expect("70% 경계 주문을 판정해야 한다");

                assert_eq!(
                    decision,
                    IrpPostOrderRiskDecision::Allowed {
                        post_order_risk_ratio_ppm: 700_000,
                    }
                );
            }

            #[test]
            fn given_risk_increase_above_70_percent_when_decided_then_order_is_rejected() {
                let input = IrpPostOrderRiskInput {
                    post_order_total_value_krw: 100,
                    post_order_risk_asset_value_krw: 71,
                    exposure_change: IrpRiskExposureChange::Increased,
                };

                let decision = decide_irp_post_order_risk(irp_risk_policy(), input)
                    .expect("70% 초과 주문을 판정해야 한다");

                assert_eq!(
                    decision,
                    IrpPostOrderRiskDecision::Rejected {
                        reason: IrpPostOrderRiskRejection::RiskLimitExceeded,
                        post_order_risk_ratio_ppm: 710_000,
                    }
                );
            }

            #[test]
            fn given_appreciation_above_limit_when_decided_then_non_increasing_order_is_allowed() {
                let input = IrpPostOrderRiskInput {
                    post_order_total_value_krw: 100,
                    post_order_risk_asset_value_krw: 80,
                    exposure_change: IrpRiskExposureChange::NotIncreased,
                };

                let decision = decide_irp_post_order_risk(irp_risk_policy(), input)
                    .expect("가격 상승 후 비증가 주문을 판정해야 한다");

                assert_eq!(
                    decision,
                    IrpPostOrderRiskDecision::Allowed {
                        post_order_risk_ratio_ppm: 800_000,
                    }
                );
            }
        }
    }
}
