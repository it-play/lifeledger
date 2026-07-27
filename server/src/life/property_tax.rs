use std::sync::Arc;

use time::{Date, Month};

use super::types::{
    AnnualPropertyTaxCalculation, AnnualPropertyTaxFairMarketRatioBand, AnnualPropertyTaxInput,
    AnnualPropertyTaxPolicy, AnnualPropertyTaxRateBracket, AnnualPropertyTaxRateSchedule,
    CapitalGainsTaxComponentCalculation, CapitalGainsTaxRateBracket, CapitalGainsTaxScope,
    CapitalGainsTaxTreatment, LOAN_RATIO_SCALE_PPM, OneHomeCapitalGainsTaxCalculation,
    OneHomeCapitalGainsTaxInput, PropertyAcquisitionTaxCalculation, PropertyAcquisitionTaxInput,
    PropertyAcquisitionTaxPolicy, PropertyCapitalGainsTaxPolicy, PropertyTaxError,
    PropertyTaxPolicy, PropertyTaxRoundingRule, PropertyTaxRules,
};

#[derive(Debug, Default)]
struct V1PropertyTaxRules;

pub fn create_property_tax_rules() -> Arc<dyn PropertyTaxRules> {
    Arc::new(V1PropertyTaxRules)
}

impl PropertyTaxRules for V1PropertyTaxRules {
    fn validate_policy(&self, policy: &PropertyTaxPolicy) -> Result<(), PropertyTaxError> {
        validate_acquisition_policy(&policy.acquisition)?;
        validate_annual_policy(&policy.annual)?;
        validate_capital_gains_policy(&policy.capital_gains)
    }

    fn calculate_acquisition_tax(
        &self,
        input: PropertyAcquisitionTaxInput<'_>,
    ) -> Result<PropertyAcquisitionTaxCalculation, PropertyTaxError> {
        validate_acquisition_policy(input.policy)?;
        if input.purchase_price_krw <= 0 {
            return Err(PropertyTaxError::InvalidAcquisitionTaxInput);
        }
        if input.household_home_count != input.policy.supported_home_count {
            return Err(PropertyTaxError::PolicyUnsupported);
        }

        let acquisition_tax_rate_ppm =
            if input.purchase_price_krw <= input.policy.lower_price_maximum_krw {
                input.policy.lower_rate_ppm
            } else if input.purchase_price_krw <= input.policy.middle_price_maximum_krw {
                let rounded_rate_before_offset = match input.policy.middle_rate_rounding {
                    PropertyTaxRoundingRule::HalfUp => round_half_up_positive(
                        i128::from(input.purchase_price_krw),
                        i128::from(input.policy.middle_rate_price_divisor_krw),
                    )?,
                };
                let rate = rounded_rate_before_offset
                    .checked_sub(i128::from(input.policy.middle_rate_offset_ppm))
                    .ok_or(PropertyTaxError::ArithmeticOverflow)?
                    .clamp(
                        i128::from(input.policy.lower_rate_ppm),
                        i128::from(input.policy.upper_rate_ppm),
                    );
                i64::try_from(rate).map_err(|_| PropertyTaxError::ArithmeticOverflow)?
            } else {
                input.policy.upper_rate_ppm
            };
        let acquisition_tax_krw = floor_rate(input.purchase_price_krw, acquisition_tax_rate_ppm)?;
        let local_education_tax_krw = floor_compound_rate(
            input.purchase_price_krw,
            acquisition_tax_rate_ppm,
            input.policy.local_education_rate_ratio_ppm,
        )?;
        let total_tax_krw = checked_money_sum(&[acquisition_tax_krw, local_education_tax_krw])?;

        Ok(PropertyAcquisitionTaxCalculation {
            tax_base_krw: input.purchase_price_krw,
            acquisition_tax_rate_ppm,
            acquisition_tax_krw,
            local_education_rate_ratio_ppm: input.policy.local_education_rate_ratio_ppm,
            local_education_tax_krw,
            total_tax_krw,
            payment_due_days: input.policy.payment_due_days,
        })
    }

    fn calculate_annual_property_tax(
        &self,
        input: AnnualPropertyTaxInput<'_>,
    ) -> Result<AnnualPropertyTaxCalculation, PropertyTaxError> {
        validate_annual_policy(input.policy)?;
        if input.reference_value_krw <= 0 {
            return Err(PropertyTaxError::InvalidAnnualPropertyTaxInput);
        }
        if input.household_home_count != input.policy.supported_home_count {
            return Err(PropertyTaxError::PolicyUnsupported);
        }

        let official_value_krw = floor_rate(
            input.reference_value_krw,
            input.policy.official_value_ratio_ppm,
        )?;
        let fair_market_band =
            select_fair_market_band(official_value_krw, &input.policy.fair_market_ratio_bands)?;
        let tax_base_krw = floor_rate(
            official_value_krw,
            fair_market_band.fair_market_value_ratio_ppm,
        )?;
        let rate_schedule =
            if official_value_krw <= input.policy.special_rate_official_value_maximum_krw {
                AnnualPropertyTaxRateSchedule::Special
            } else {
                AnnualPropertyTaxRateSchedule::Standard
            };
        let bracket =
            select_annual_bracket(tax_base_krw, rate_schedule, &input.policy.rate_brackets)?;
        let property_tax_krw = calculate_progressive_tax(
            tax_base_krw,
            bracket.rate_ppm,
            bracket.progressive_deduction_krw,
        )?;
        let local_education_tax_krw = floor_rate(
            property_tax_krw,
            input.policy.local_education_rate_ratio_ppm,
        )?;
        let total_tax_krw = checked_money_sum(&[property_tax_krw, local_education_tax_krw])?;
        let first_payment_krw = total_tax_krw
            .checked_div(2)
            .ok_or(PropertyTaxError::ArithmeticOverflow)?;
        let second_payment_krw = total_tax_krw
            .checked_sub(first_payment_krw)
            .ok_or(PropertyTaxError::ArithmeticOverflow)?;

        Ok(AnnualPropertyTaxCalculation {
            reference_value_krw: input.reference_value_krw,
            official_value_krw,
            fair_market_value_ratio_ppm: fair_market_band.fair_market_value_ratio_ppm,
            tax_base_krw,
            rate_schedule,
            property_tax_rate_ppm: bracket.rate_ppm,
            progressive_deduction_krw: bracket.progressive_deduction_krw,
            property_tax_krw,
            local_education_rate_ratio_ppm: input.policy.local_education_rate_ratio_ppm,
            local_education_tax_krw,
            total_tax_krw,
            first_payment_krw,
            second_payment_krw,
        })
    }

    fn calculate_one_home_capital_gains_tax(
        &self,
        input: OneHomeCapitalGainsTaxInput<'_>,
    ) -> Result<OneHomeCapitalGainsTaxCalculation, PropertyTaxError> {
        validate_capital_gains_policy(input.policy)?;
        if input.sale_price_krw <= 0
            || input.acquisition_price_krw <= 0
            || input.acquisition_incidental_cost_krw < 0
            || input.acquisition_taxes_krw < 0
            || input.disposition_cost_krw < 0
            || input.owner_occupied_from < input.acquired_on
            || input.sold_on < input.owner_occupied_from
        {
            return Err(PropertyTaxError::InvalidCapitalGainsTaxInput);
        }
        if input.household_home_count != input.policy.supported_home_count {
            return Err(PropertyTaxError::PolicyUnsupported);
        }

        let completed_holding_years = completed_calendar_years(input.acquired_on, input.sold_on)?;
        let completed_residence_years =
            completed_calendar_years(input.owner_occupied_from, input.sold_on)?;
        if completed_holding_years < input.policy.minimum_holding_years
            || completed_residence_years < input.policy.minimum_residence_years
        {
            return Err(PropertyTaxError::PolicyUnsupported);
        }

        let total_basis_krw = i128::from(input.acquisition_price_krw)
            .checked_add(i128::from(input.acquisition_incidental_cost_krw))
            .and_then(|value| value.checked_add(i128::from(input.acquisition_taxes_krw)))
            .and_then(|value| value.checked_add(i128::from(input.disposition_cost_krw)))
            .ok_or(PropertyTaxError::ArithmeticOverflow)?;
        let gross_gain_krw = i128::from(input.sale_price_krw)
            .checked_sub(total_basis_krw)
            .ok_or(PropertyTaxError::ArithmeticOverflow)?
            .max(0);
        let gross_gain_krw =
            i64::try_from(gross_gain_krw).map_err(|_| PropertyTaxError::ArithmeticOverflow)?;

        if input.sale_price_krw <= input.policy.high_value_threshold_krw {
            return Ok(zero_capital_gains_tax(
                CapitalGainsTaxTreatment::OneHomeExempt,
                completed_holding_years,
                completed_residence_years,
                gross_gain_krw,
            ));
        }

        let high_value_excess_krw = input
            .sale_price_krw
            .checked_sub(input.policy.high_value_threshold_krw)
            .ok_or(PropertyTaxError::ArithmeticOverflow)?;
        let high_value_gain_krw = i128::from(gross_gain_krw)
            .checked_mul(i128::from(high_value_excess_krw))
            .ok_or(PropertyTaxError::ArithmeticOverflow)?
            .checked_div(i128::from(input.sale_price_krw))
            .ok_or(PropertyTaxError::ArithmeticOverflow)?;
        let high_value_gain_krw =
            i64::try_from(high_value_gain_krw).map_err(|_| PropertyTaxError::ArithmeticOverflow)?;
        let holding_deduction_rate_ppm = deduction_rate(
            completed_holding_years,
            input.policy.holding_deduction_start_years,
            input.policy.holding_deduction_start_rate_ppm,
            input.policy.holding_deduction_per_year_ppm,
            input.policy.holding_deduction_maximum_ppm,
        )?;
        let residence_deduction_rate_ppm = deduction_rate(
            completed_residence_years,
            input.policy.residence_deduction_start_years,
            input.policy.residence_deduction_start_rate_ppm,
            input.policy.residence_deduction_per_year_ppm,
            input.policy.residence_deduction_maximum_ppm,
        )?;
        let long_term_deduction_rate_ppm = holding_deduction_rate_ppm
            .checked_add(residence_deduction_rate_ppm)
            .ok_or(PropertyTaxError::ArithmeticOverflow)?;
        let long_term_deduction_krw =
            floor_rate(high_value_gain_krw, long_term_deduction_rate_ppm)?;
        let gain_after_long_term_deduction_krw = high_value_gain_krw
            .checked_sub(long_term_deduction_krw)
            .ok_or(PropertyTaxError::ArithmeticOverflow)?;
        let basic_deduction_krw = input
            .policy
            .basic_deduction_krw
            .min(gain_after_long_term_deduction_krw);
        let taxable_amount_krw = gain_after_long_term_deduction_krw
            .checked_sub(basic_deduction_krw)
            .ok_or(PropertyTaxError::ArithmeticOverflow)?;
        let national = calculate_capital_gains_component(
            taxable_amount_krw,
            CapitalGainsTaxScope::National,
            &input.policy.rate_brackets,
        )?;
        let local = calculate_capital_gains_component(
            taxable_amount_krw,
            CapitalGainsTaxScope::Local,
            &input.policy.rate_brackets,
        )?;
        let total_tax_krw = checked_money_sum(&[national.tax_krw, local.tax_krw])?;

        Ok(OneHomeCapitalGainsTaxCalculation {
            treatment: CapitalGainsTaxTreatment::HighValueHome,
            completed_holding_years,
            completed_residence_years,
            gross_gain_krw,
            high_value_gain_krw,
            holding_deduction_rate_ppm,
            residence_deduction_rate_ppm,
            long_term_deduction_rate_ppm,
            long_term_deduction_krw,
            basic_deduction_krw,
            taxable_amount_krw,
            national,
            local,
            total_tax_krw,
        })
    }
}

fn validate_acquisition_policy(
    policy: &PropertyAcquisitionTaxPolicy,
) -> Result<(), PropertyTaxError> {
    if policy.supported_home_count != 1
        || policy.lower_price_maximum_krw <= 0
        || policy.middle_price_maximum_krw <= policy.lower_price_maximum_krw
        || !is_positive_rate(policy.lower_rate_ppm)
        || policy.upper_rate_ppm < policy.lower_rate_ppm
        || policy.upper_rate_ppm > LOAN_RATIO_SCALE_PPM
        || policy.middle_rate_price_divisor_krw <= 0
        || policy.middle_rate_offset_ppm < 0
        || !is_positive_rate(policy.local_education_rate_ratio_ppm)
        || policy.payment_due_days == 0
    {
        return Err(PropertyTaxError::InvalidPolicy);
    }
    Ok(())
}

fn validate_annual_policy(policy: &AnnualPropertyTaxPolicy) -> Result<(), PropertyTaxError> {
    if policy.supported_home_count != 1
        || !is_valid_month_day(policy.assessment_month, policy.assessment_day)
        || !is_positive_rate(policy.official_value_ratio_ppm)
        || policy.special_rate_official_value_maximum_krw <= 0
        || !is_positive_rate(policy.local_education_rate_ratio_ppm)
        || !is_valid_month_day(policy.first_payment_month, policy.first_payment_day)
        || !is_valid_month_day(policy.second_payment_month, policy.second_payment_day)
        || (policy.first_payment_month, policy.first_payment_day)
            >= (policy.second_payment_month, policy.second_payment_day)
    {
        return Err(PropertyTaxError::InvalidPolicy);
    }
    validate_fair_market_bands(&policy.fair_market_ratio_bands)?;
    validate_annual_brackets(&policy.rate_brackets)
}

fn validate_capital_gains_policy(
    policy: &PropertyCapitalGainsTaxPolicy,
) -> Result<(), PropertyTaxError> {
    if policy.supported_home_count != 1
        || policy.high_value_threshold_krw <= 0
        || policy.basic_deduction_krw < 0
        || policy.minimum_holding_years == 0
        || policy.minimum_residence_years == 0
        || policy.holding_deduction_start_years < policy.minimum_holding_years
        || policy.residence_deduction_start_years < policy.minimum_residence_years
        || !valid_deduction_policy(
            policy.holding_deduction_start_rate_ppm,
            policy.holding_deduction_per_year_ppm,
            policy.holding_deduction_maximum_ppm,
        )
        || !valid_deduction_policy(
            policy.residence_deduction_start_rate_ppm,
            policy.residence_deduction_per_year_ppm,
            policy.residence_deduction_maximum_ppm,
        )
        || policy
            .holding_deduction_maximum_ppm
            .checked_add(policy.residence_deduction_maximum_ppm)
            .is_none_or(|rate| rate > LOAN_RATIO_SCALE_PPM)
        || !is_positive_rate(policy.local_income_tax_ratio_ppm)
    {
        return Err(PropertyTaxError::InvalidPolicy);
    }
    validate_capital_gains_brackets(&policy.rate_brackets, policy.local_income_tax_ratio_ppm)
}

fn validate_fair_market_bands(
    bands: &[AnnualPropertyTaxFairMarketRatioBand],
) -> Result<(), PropertyTaxError> {
    if bands.is_empty() {
        return Err(PropertyTaxError::InvalidPolicy);
    }
    let mut previous_upper_bound_krw = 0_i64;
    for (index, band) in bands.iter().enumerate() {
        if !is_positive_rate(band.fair_market_value_ratio_ppm) {
            return Err(PropertyTaxError::InvalidPolicy);
        }
        match band.official_value_upper_bound_krw {
            Some(upper_bound_krw) => {
                if index + 1 == bands.len() || upper_bound_krw <= previous_upper_bound_krw {
                    return Err(PropertyTaxError::InvalidPolicy);
                }
                previous_upper_bound_krw = upper_bound_krw;
            }
            None if index + 1 == bands.len() => {}
            None => return Err(PropertyTaxError::InvalidPolicy),
        }
    }
    Ok(())
}

fn validate_annual_brackets(
    brackets: &[AnnualPropertyTaxRateBracket],
) -> Result<(), PropertyTaxError> {
    for schedule in [
        AnnualPropertyTaxRateSchedule::Special,
        AnnualPropertyTaxRateSchedule::Standard,
    ] {
        let selected: Vec<_> = brackets
            .iter()
            .filter(|bracket| bracket.rate_schedule == schedule)
            .collect();
        validate_progressive_brackets(
            &selected
                .iter()
                .map(|bracket| {
                    (
                        bracket.tax_base_upper_bound_krw,
                        bracket.rate_ppm,
                        bracket.progressive_deduction_krw,
                    )
                })
                .collect::<Vec<_>>(),
        )?;
    }
    Ok(())
}

fn validate_capital_gains_brackets(
    brackets: &[CapitalGainsTaxRateBracket],
    local_income_tax_ratio_ppm: i64,
) -> Result<(), PropertyTaxError> {
    let national: Vec<_> = brackets
        .iter()
        .filter(|bracket| bracket.tax_scope == CapitalGainsTaxScope::National)
        .collect();
    let local: Vec<_> = brackets
        .iter()
        .filter(|bracket| bracket.tax_scope == CapitalGainsTaxScope::Local)
        .collect();
    validate_progressive_brackets(
        &national
            .iter()
            .map(|bracket| {
                (
                    bracket.taxable_amount_upper_bound_krw,
                    bracket.rate_ppm,
                    bracket.progressive_deduction_krw,
                )
            })
            .collect::<Vec<_>>(),
    )?;
    validate_progressive_brackets(
        &local
            .iter()
            .map(|bracket| {
                (
                    bracket.taxable_amount_upper_bound_krw,
                    bracket.rate_ppm,
                    bracket.progressive_deduction_krw,
                )
            })
            .collect::<Vec<_>>(),
    )?;
    if national.len() != local.len() {
        return Err(PropertyTaxError::InvalidPolicy);
    }
    for (national, local) in national.into_iter().zip(local) {
        let expected_local_rate = floor_rate(national.rate_ppm, local_income_tax_ratio_ppm)?;
        let expected_local_deduction = floor_rate(
            national.progressive_deduction_krw,
            local_income_tax_ratio_ppm,
        )?;
        if national.taxable_amount_upper_bound_krw != local.taxable_amount_upper_bound_krw
            || local.rate_ppm != expected_local_rate
            || local.progressive_deduction_krw != expected_local_deduction
        {
            return Err(PropertyTaxError::InvalidPolicy);
        }
    }
    Ok(())
}

fn validate_progressive_brackets(
    brackets: &[(Option<i64>, i64, i64)],
) -> Result<(), PropertyTaxError> {
    if brackets.is_empty() || brackets[0].2 != 0 {
        return Err(PropertyTaxError::InvalidPolicy);
    }
    let mut previous: Option<(i64, i64, i64)> = None;
    for (index, &(upper_bound_krw, rate_ppm, progressive_deduction_krw)) in
        brackets.iter().enumerate()
    {
        if !is_positive_rate(rate_ppm) || progressive_deduction_krw < 0 {
            return Err(PropertyTaxError::InvalidPolicy);
        }
        match upper_bound_krw {
            Some(upper_bound_krw) => {
                if index + 1 == brackets.len()
                    || upper_bound_krw <= previous.map_or(0, |value| value.0)
                {
                    return Err(PropertyTaxError::InvalidPolicy);
                }
                if let Some((previous_upper_bound_krw, previous_rate_ppm, previous_deduction)) =
                    previous
                {
                    let previous_tax = calculate_progressive_tax(
                        previous_upper_bound_krw,
                        previous_rate_ppm,
                        previous_deduction,
                    )?;
                    let current_tax = calculate_progressive_tax(
                        previous_upper_bound_krw,
                        rate_ppm,
                        progressive_deduction_krw,
                    )?;
                    if previous_tax != current_tax {
                        return Err(PropertyTaxError::InvalidPolicy);
                    }
                }
                previous = Some((upper_bound_krw, rate_ppm, progressive_deduction_krw));
            }
            None if index + 1 == brackets.len() => {
                if let Some((previous_upper_bound_krw, previous_rate_ppm, previous_deduction)) =
                    previous
                {
                    let previous_tax = calculate_progressive_tax(
                        previous_upper_bound_krw,
                        previous_rate_ppm,
                        previous_deduction,
                    )?;
                    let current_tax = calculate_progressive_tax(
                        previous_upper_bound_krw,
                        rate_ppm,
                        progressive_deduction_krw,
                    )?;
                    if previous_tax != current_tax {
                        return Err(PropertyTaxError::InvalidPolicy);
                    }
                }
            }
            None => return Err(PropertyTaxError::InvalidPolicy),
        }
    }
    Ok(())
}

fn select_fair_market_band(
    official_value_krw: i64,
    bands: &[AnnualPropertyTaxFairMarketRatioBand],
) -> Result<AnnualPropertyTaxFairMarketRatioBand, PropertyTaxError> {
    bands
        .iter()
        .copied()
        .find(|band| {
            band.official_value_upper_bound_krw
                .is_none_or(|upper_bound_krw| official_value_krw <= upper_bound_krw)
        })
        .ok_or(PropertyTaxError::InvalidPolicy)
}

fn select_annual_bracket(
    tax_base_krw: i64,
    rate_schedule: AnnualPropertyTaxRateSchedule,
    brackets: &[AnnualPropertyTaxRateBracket],
) -> Result<AnnualPropertyTaxRateBracket, PropertyTaxError> {
    brackets
        .iter()
        .copied()
        .filter(|bracket| bracket.rate_schedule == rate_schedule)
        .find(|bracket| {
            bracket
                .tax_base_upper_bound_krw
                .is_none_or(|upper_bound_krw| tax_base_krw <= upper_bound_krw)
        })
        .ok_or(PropertyTaxError::InvalidPolicy)
}

fn calculate_capital_gains_component(
    taxable_amount_krw: i64,
    tax_scope: CapitalGainsTaxScope,
    brackets: &[CapitalGainsTaxRateBracket],
) -> Result<CapitalGainsTaxComponentCalculation, PropertyTaxError> {
    if taxable_amount_krw == 0 {
        return Ok(CapitalGainsTaxComponentCalculation {
            tax_scope,
            rate_ppm: 0,
            progressive_deduction_krw: 0,
            tax_krw: 0,
        });
    }
    let bracket = brackets
        .iter()
        .find(|bracket| {
            bracket.tax_scope == tax_scope
                && bracket
                    .taxable_amount_upper_bound_krw
                    .is_none_or(|upper_bound_krw| taxable_amount_krw <= upper_bound_krw)
        })
        .ok_or(PropertyTaxError::InvalidPolicy)?;
    let tax_krw = calculate_progressive_tax(
        taxable_amount_krw,
        bracket.rate_ppm,
        bracket.progressive_deduction_krw,
    )?;
    Ok(CapitalGainsTaxComponentCalculation {
        tax_scope,
        rate_ppm: bracket.rate_ppm,
        progressive_deduction_krw: bracket.progressive_deduction_krw,
        tax_krw,
    })
}

fn zero_capital_gains_tax(
    treatment: CapitalGainsTaxTreatment,
    completed_holding_years: u16,
    completed_residence_years: u16,
    gross_gain_krw: i64,
) -> OneHomeCapitalGainsTaxCalculation {
    OneHomeCapitalGainsTaxCalculation {
        treatment,
        completed_holding_years,
        completed_residence_years,
        gross_gain_krw,
        high_value_gain_krw: 0,
        holding_deduction_rate_ppm: 0,
        residence_deduction_rate_ppm: 0,
        long_term_deduction_rate_ppm: 0,
        long_term_deduction_krw: 0,
        basic_deduction_krw: 0,
        taxable_amount_krw: 0,
        national: CapitalGainsTaxComponentCalculation {
            tax_scope: CapitalGainsTaxScope::National,
            rate_ppm: 0,
            progressive_deduction_krw: 0,
            tax_krw: 0,
        },
        local: CapitalGainsTaxComponentCalculation {
            tax_scope: CapitalGainsTaxScope::Local,
            rate_ppm: 0,
            progressive_deduction_krw: 0,
            tax_krw: 0,
        },
        total_tax_krw: 0,
    }
}

fn deduction_rate(
    completed_years: u16,
    start_years: u16,
    start_rate_ppm: i64,
    per_year_ppm: i64,
    maximum_rate_ppm: i64,
) -> Result<i64, PropertyTaxError> {
    if completed_years < start_years {
        return Ok(0);
    }
    let additional_years = completed_years
        .checked_sub(start_years)
        .ok_or(PropertyTaxError::ArithmeticOverflow)?;
    let rate = i128::from(additional_years)
        .checked_mul(i128::from(per_year_ppm))
        .and_then(|value| value.checked_add(i128::from(start_rate_ppm)))
        .ok_or(PropertyTaxError::ArithmeticOverflow)?
        .min(i128::from(maximum_rate_ppm));
    i64::try_from(rate).map_err(|_| PropertyTaxError::ArithmeticOverflow)
}

fn completed_calendar_years(start: Date, end: Date) -> Result<u16, PropertyTaxError> {
    if end < start {
        return Err(PropertyTaxError::InvalidCapitalGainsTaxInput);
    }
    let year_difference = end
        .year()
        .checked_sub(start.year())
        .ok_or(PropertyTaxError::ArithmeticOverflow)?;
    let year_difference =
        u16::try_from(year_difference).map_err(|_| PropertyTaxError::ArithmeticOverflow)?;
    let anniversary = add_years_clamped(start, year_difference)?;
    if end >= anniversary {
        Ok(year_difference)
    } else {
        year_difference
            .checked_sub(1)
            .ok_or(PropertyTaxError::ArithmeticOverflow)
    }
}

fn add_years_clamped(date: Date, years: u16) -> Result<Date, PropertyTaxError> {
    let target_year = date
        .year()
        .checked_add(i32::from(years))
        .ok_or(PropertyTaxError::ArithmeticOverflow)?;
    for day in (1..=date.day()).rev() {
        if let Ok(candidate) = Date::from_calendar_date(target_year, date.month(), day) {
            return Ok(candidate);
        }
    }
    Err(PropertyTaxError::InvalidCapitalGainsTaxInput)
}

fn round_half_up_positive(numerator: i128, denominator: i128) -> Result<i128, PropertyTaxError> {
    numerator
        .checked_add(
            denominator
                .checked_div(2)
                .ok_or(PropertyTaxError::ArithmeticOverflow)?,
        )
        .ok_or(PropertyTaxError::ArithmeticOverflow)?
        .checked_div(denominator)
        .ok_or(PropertyTaxError::ArithmeticOverflow)
}

fn floor_rate(amount_krw: i64, rate_ppm: i64) -> Result<i64, PropertyTaxError> {
    let amount = i128::from(amount_krw)
        .checked_mul(i128::from(rate_ppm))
        .ok_or(PropertyTaxError::ArithmeticOverflow)?
        .checked_div(i128::from(LOAN_RATIO_SCALE_PPM))
        .ok_or(PropertyTaxError::ArithmeticOverflow)?;
    i64::try_from(amount).map_err(|_| PropertyTaxError::ArithmeticOverflow)
}

fn floor_compound_rate(
    amount_krw: i64,
    first_rate_ppm: i64,
    second_rate_ppm: i64,
) -> Result<i64, PropertyTaxError> {
    let denominator = i128::from(LOAN_RATIO_SCALE_PPM)
        .checked_mul(i128::from(LOAN_RATIO_SCALE_PPM))
        .ok_or(PropertyTaxError::ArithmeticOverflow)?;
    let amount = i128::from(amount_krw)
        .checked_mul(i128::from(first_rate_ppm))
        .and_then(|value| value.checked_mul(i128::from(second_rate_ppm)))
        .ok_or(PropertyTaxError::ArithmeticOverflow)?
        .checked_div(denominator)
        .ok_or(PropertyTaxError::ArithmeticOverflow)?;
    i64::try_from(amount).map_err(|_| PropertyTaxError::ArithmeticOverflow)
}

fn calculate_progressive_tax(
    tax_base_krw: i64,
    rate_ppm: i64,
    progressive_deduction_krw: i64,
) -> Result<i64, PropertyTaxError> {
    floor_rate(tax_base_krw, rate_ppm)?
        .checked_sub(progressive_deduction_krw)
        .filter(|tax_krw| *tax_krw >= 0)
        .ok_or(PropertyTaxError::InvalidPolicy)
}

fn checked_money_sum(amounts_krw: &[i64]) -> Result<i64, PropertyTaxError> {
    let total = amounts_krw.iter().try_fold(0_i128, |sum, amount_krw| {
        sum.checked_add(i128::from(*amount_krw))
            .ok_or(PropertyTaxError::ArithmeticOverflow)
    })?;
    i64::try_from(total).map_err(|_| PropertyTaxError::ArithmeticOverflow)
}

fn is_positive_rate(rate_ppm: i64) -> bool {
    (1..=LOAN_RATIO_SCALE_PPM).contains(&rate_ppm)
}

fn valid_deduction_policy(start_rate_ppm: i64, per_year_ppm: i64, maximum_rate_ppm: i64) -> bool {
    is_positive_rate(start_rate_ppm)
        && is_positive_rate(per_year_ppm)
        && maximum_rate_ppm >= start_rate_ppm
        && maximum_rate_ppm <= LOAN_RATIO_SCALE_PPM
}

fn is_valid_month_day(month: u8, day: u8) -> bool {
    Month::try_from(month)
        .ok()
        .and_then(|month| Date::from_calendar_date(2000, month, day).ok())
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::life::{
        AnnualPropertyTaxOwnershipCutoffRule, AnnualPropertyTaxPaymentSplitRule,
        CapitalGainsTaxPaymentRule,
    };

    fn given_acquisition_policy() -> PropertyAcquisitionTaxPolicy {
        PropertyAcquisitionTaxPolicy {
            supported_home_count: 1,
            lower_price_maximum_krw: 600_000_000,
            middle_price_maximum_krw: 900_000_000,
            lower_rate_ppm: 10_000,
            upper_rate_ppm: 30_000,
            middle_rate_price_divisor_krw: 15_000,
            middle_rate_offset_ppm: 30_000,
            middle_rate_rounding: PropertyTaxRoundingRule::HalfUp,
            local_education_rate_ratio_ppm: 100_000,
            payment_due_days: 60,
        }
    }

    fn given_annual_policy() -> AnnualPropertyTaxPolicy {
        let mut rate_brackets = Vec::new();
        for (rate_schedule, rates) in [
            (
                AnnualPropertyTaxRateSchedule::Special,
                [500, 1_000, 2_000, 3_500],
            ),
            (
                AnnualPropertyTaxRateSchedule::Standard,
                [1_000, 1_500, 2_500, 4_000],
            ),
        ] {
            for ((tax_base_upper_bound_krw, progressive_deduction_krw), rate_ppm) in [
                (Some(60_000_000), 0),
                (Some(150_000_000), 30_000),
                (Some(300_000_000), 180_000),
                (None, 630_000),
            ]
            .into_iter()
            .zip(rates)
            {
                rate_brackets.push(AnnualPropertyTaxRateBracket {
                    rate_schedule,
                    tax_base_upper_bound_krw,
                    rate_ppm,
                    progressive_deduction_krw,
                });
            }
        }
        AnnualPropertyTaxPolicy {
            supported_home_count: 1,
            assessment_month: 6,
            assessment_day: 1,
            ownership_cutoff_rule: AnnualPropertyTaxOwnershipCutoffRule::PriorDayClosingOwner,
            official_value_ratio_ppm: 700_000,
            fair_market_ratio_bands: vec![
                AnnualPropertyTaxFairMarketRatioBand {
                    official_value_upper_bound_krw: Some(300_000_000),
                    fair_market_value_ratio_ppm: 430_000,
                },
                AnnualPropertyTaxFairMarketRatioBand {
                    official_value_upper_bound_krw: Some(600_000_000),
                    fair_market_value_ratio_ppm: 440_000,
                },
                AnnualPropertyTaxFairMarketRatioBand {
                    official_value_upper_bound_krw: None,
                    fair_market_value_ratio_ppm: 450_000,
                },
            ],
            special_rate_official_value_maximum_krw: 900_000_000,
            rate_brackets,
            local_education_rate_ratio_ppm: 200_000,
            first_payment_month: 7,
            first_payment_day: 31,
            second_payment_month: 9,
            second_payment_day: 30,
            payment_split_rule: AnnualPropertyTaxPaymentSplitRule::FloorHalfThenRemainder,
        }
    }

    fn given_capital_gains_policy() -> PropertyCapitalGainsTaxPolicy {
        let national = [
            (Some(14_000_000), 60_000, 0),
            (Some(50_000_000), 150_000, 1_260_000),
            (Some(88_000_000), 240_000, 5_760_000),
            (Some(150_000_000), 350_000, 15_440_000),
            (Some(300_000_000), 380_000, 19_940_000),
            (Some(500_000_000), 400_000, 25_940_000),
            (Some(1_000_000_000), 420_000, 35_940_000),
            (None, 450_000, 65_940_000),
        ];
        let mut rate_brackets = Vec::new();
        for (taxable_amount_upper_bound_krw, rate_ppm, progressive_deduction_krw) in national {
            rate_brackets.push(CapitalGainsTaxRateBracket {
                tax_scope: CapitalGainsTaxScope::National,
                taxable_amount_upper_bound_krw,
                rate_ppm,
                progressive_deduction_krw,
            });
        }
        for (taxable_amount_upper_bound_krw, rate_ppm, progressive_deduction_krw) in national {
            rate_brackets.push(CapitalGainsTaxRateBracket {
                tax_scope: CapitalGainsTaxScope::Local,
                taxable_amount_upper_bound_krw,
                rate_ppm: rate_ppm / 10,
                progressive_deduction_krw: progressive_deduction_krw / 10,
            });
        }
        PropertyCapitalGainsTaxPolicy {
            supported_home_count: 1,
            high_value_threshold_krw: 1_200_000_000,
            basic_deduction_krw: 2_500_000,
            minimum_holding_years: 2,
            minimum_residence_years: 2,
            holding_deduction_start_years: 3,
            holding_deduction_start_rate_ppm: 120_000,
            holding_deduction_per_year_ppm: 40_000,
            holding_deduction_maximum_ppm: 400_000,
            residence_deduction_start_years: 2,
            residence_deduction_start_rate_ppm: 80_000,
            residence_deduction_per_year_ppm: 40_000,
            residence_deduction_maximum_ppm: 400_000,
            local_income_tax_ratio_ppm: 100_000,
            rate_brackets,
            payment_rule: CapitalGainsTaxPaymentRule::WithheldAtSale,
        }
    }

    fn given_policy() -> PropertyTaxPolicy {
        PropertyTaxPolicy {
            acquisition: given_acquisition_policy(),
            annual: given_annual_policy(),
            capital_gains: given_capital_gains_policy(),
        }
    }

    fn given_date(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("테스트 날짜는 유효해야 한다")
    }

    fn given_capital_gains_input(
        policy: &PropertyCapitalGainsTaxPolicy,
    ) -> OneHomeCapitalGainsTaxInput<'_> {
        OneHomeCapitalGainsTaxInput {
            sale_price_krw: 1_500_000_000,
            acquisition_price_krw: 800_000_000,
            acquisition_incidental_cost_krw: 10_000_000,
            acquisition_taxes_krw: 10_000_000,
            disposition_cost_krw: 10_000_000,
            acquired_on: given_date(2010, Month::January, 3),
            owner_occupied_from: given_date(2012, Month::January, 3),
            sold_on: given_date(2022, Month::January, 3),
            household_home_count: 1,
            policy,
        }
    }

    mod context_6억원초과_9억원이하_주택을_취득하는_경우 {
        use super::*;

        #[test]
        fn given_중간세율의_절반미만_나머지_when_계산하면_then_아래ppm으로_반올림한다() {
            let rules = create_property_tax_rules();
            let policy = given_acquisition_policy();

            let result = rules
                .calculate_acquisition_tax(PropertyAcquisitionTaxInput {
                    purchase_price_krw: 600_007_499,
                    household_home_count: 1,
                    policy: &policy,
                })
                .expect("중간 취득세율을 계산해야 한다");

            assert_eq!(result.acquisition_tax_rate_ppm, 10_000);
        }

        #[test]
        fn given_중간세율의_정확한_절반_나머지_when_계산하면_then_위ppm으로_반올림한다() {
            let rules = create_property_tax_rules();
            let policy = given_acquisition_policy();

            let result = rules
                .calculate_acquisition_tax(PropertyAcquisitionTaxInput {
                    purchase_price_krw: 600_007_500,
                    household_home_count: 1,
                    policy: &policy,
                })
                .expect("중간 취득세율을 계산해야 한다");

            assert_eq!(result.acquisition_tax_rate_ppm, 10_001);
            assert_eq!(result.acquisition_tax_krw, 6_000_675);
            assert_eq!(result.local_education_tax_krw, 600_067);
        }
    }

    mod context_지원하지않는_주택수로_취득세를_계산하는_경우 {
        use super::*;

        #[test]
        fn given_세대_2주택_when_계산하면_then_0세율로_보지않고_거절한다() {
            let rules = create_property_tax_rules();
            let policy = given_acquisition_policy();

            let result = rules.calculate_acquisition_tax(PropertyAcquisitionTaxInput {
                purchase_price_krw: 500_000_000,
                household_home_count: 2,
                policy: &policy,
            });

            assert_eq!(result, Err(PropertyTaxError::PolicyUnsupported));
        }
    }

    mod context_1주택_재산세_과표를_계산하는_경우 {
        use super::*;

        #[test]
        fn given_기준가_6억원_when_계산하면_then_공시가와_공정시장비율을_순서대로_내림한다() {
            let rules = create_property_tax_rules();
            let policy = given_annual_policy();

            let result = rules
                .calculate_annual_property_tax(AnnualPropertyTaxInput {
                    reference_value_krw: 600_000_000,
                    household_home_count: 1,
                    policy: &policy,
                })
                .expect("연간 재산세를 계산해야 한다");

            assert_eq!(result.official_value_krw, 420_000_000);
            assert_eq!(result.fair_market_value_ratio_ppm, 440_000);
            assert_eq!(result.tax_base_krw, 184_800_000);
            assert_eq!(result.property_tax_krw, 189_600);
            assert_eq!(result.local_education_tax_krw, 37_920);
        }
    }

    mod context_1주택_특례_재산세의_누진경계인_경우 {
        use super::*;

        #[test]
        fn given_과표가_정확히_6천만원_when_계산하면_then_첫_구간을_적용한다() {
            let rules = create_property_tax_rules();
            let policy = given_annual_policy();

            let result = rules
                .calculate_annual_property_tax(AnnualPropertyTaxInput {
                    reference_value_krw: 199_335_549,
                    household_home_count: 1,
                    policy: &policy,
                })
                .expect("첫 재산세 경계를 계산해야 한다");

            assert_eq!(result.tax_base_krw, 60_000_000);
            assert_eq!(result.property_tax_rate_ppm, 500);
            assert_eq!(result.property_tax_krw, 30_000);
        }

        #[test]
        fn given_과표가_6천만원을_1원초과할때_when_계산하면_then_둘째_구간을_적용한다() {
            let rules = create_property_tax_rules();
            let policy = given_annual_policy();

            let result = rules
                .calculate_annual_property_tax(AnnualPropertyTaxInput {
                    reference_value_krw: 199_335_553,
                    household_home_count: 1,
                    policy: &policy,
                })
                .expect("둘째 재산세 경계를 계산해야 한다");

            assert_eq!(result.tax_base_krw, 60_000_001);
            assert_eq!(result.property_tax_rate_ppm, 1_000);
            assert_eq!(result.property_tax_krw, 30_000);
        }
    }

    mod context_공시가격이_9억원을_초과하는_1주택인_경우 {
        use super::*;

        #[test]
        fn given_공시가격_9억1천만원_when_계산하면_then_표준세율을_적용한다() {
            let rules = create_property_tax_rules();
            let policy = given_annual_policy();

            let result = rules
                .calculate_annual_property_tax(AnnualPropertyTaxInput {
                    reference_value_krw: 1_300_000_000,
                    household_home_count: 1,
                    policy: &policy,
                })
                .expect("표준 재산세율을 계산해야 한다");

            assert_eq!(
                result.rate_schedule,
                AnnualPropertyTaxRateSchedule::Standard
            );
            assert_eq!(result.tax_base_krw, 409_500_000);
            assert_eq!(result.property_tax_rate_ppm, 4_000);
            assert_eq!(result.property_tax_krw, 1_008_000);
        }
    }

    mod context_연간_재산세를_두번에_나누어_납부하는_경우 {
        use super::*;

        #[test]
        fn given_총세액이_홀수_when_분할하면_then_첫납부를_내리고_나머지를_둘째에_둔다() {
            let rules = create_property_tax_rules();
            let policy = given_annual_policy();

            let result = rules
                .calculate_annual_property_tax(AnnualPropertyTaxInput {
                    reference_value_krw: 100_006_646,
                    household_home_count: 1,
                    policy: &policy,
                })
                .expect("홀수 총세액을 분할해야 한다");

            assert_eq!(result.total_tax_krw, 18_061);
            assert_eq!(result.first_payment_krw, 9_030);
            assert_eq!(result.second_payment_krw, 9_031);
        }
    }

    mod context_12억원이하_1세대1주택을_양도하는_경우 {
        use super::*;

        #[test]
        fn given_2년보유와_거주를_완료한_12억원주택_when_계산하면_then_국세와_지방세가_0원이다() {
            let rules = create_property_tax_rules();
            let policy = given_capital_gains_policy();
            let mut input = given_capital_gains_input(&policy);
            input.sale_price_krw = 1_200_000_000;

            let result = rules
                .calculate_one_home_capital_gains_tax(input)
                .expect("1주택 비과세를 판정해야 한다");

            assert_eq!(result.treatment, CapitalGainsTaxTreatment::OneHomeExempt);
            assert_eq!(result.national.tax_krw, 0);
            assert_eq!(result.local.tax_krw, 0);
        }
    }

    mod context_12억원초과_장기보유_1세대1주택을_양도하는_경우 {
        use super::*;

        #[test]
        fn given_15억원양도와_6억7천만원차익_when_계산하면_then_고가주택안분과_80퍼센트공제를_적용한다()
         {
            let rules = create_property_tax_rules();
            let policy = given_capital_gains_policy();

            let result = rules
                .calculate_one_home_capital_gains_tax(given_capital_gains_input(&policy))
                .expect("고가주택 양도세를 계산해야 한다");

            assert_eq!(result.gross_gain_krw, 670_000_000);
            assert_eq!(result.high_value_gain_krw, 134_000_000);
            assert_eq!(result.long_term_deduction_rate_ppm, 800_000);
            assert_eq!(result.long_term_deduction_krw, 107_200_000);
            assert_eq!(result.taxable_amount_krw, 24_300_000);
            assert_eq!(result.national.tax_krw, 2_385_000);
            assert_eq!(result.local.tax_krw, 238_500);
        }

        #[test]
        fn given_보유3년과_거주2년_when_계산하면_then_각각_12퍼센트와_8퍼센트를_공제한다() {
            let rules = create_property_tax_rules();
            let policy = given_capital_gains_policy();
            let mut input = given_capital_gains_input(&policy);
            input.acquired_on = given_date(2027, Month::January, 3);
            input.owner_occupied_from = given_date(2028, Month::January, 3);
            input.sold_on = given_date(2030, Month::January, 3);

            let result = rules
                .calculate_one_home_capital_gains_tax(input)
                .expect("최초 장기보유 공제율을 계산해야 한다");

            assert_eq!(result.holding_deduction_rate_ppm, 120_000);
            assert_eq!(result.residence_deduction_rate_ppm, 80_000);
        }

        #[test]
        fn given_과표가_10억원을_초과할때_when_계산하면_then_45퍼센트_누진세율을_적용한다() {
            let rules = create_property_tax_rules();
            let policy = given_capital_gains_policy();
            let mut input = given_capital_gains_input(&policy);
            input.sale_price_krw = 4_000_000_000;
            input.acquisition_price_krw = 1_000_000_000;
            input.acquisition_incidental_cost_krw = 0;
            input.acquisition_taxes_krw = 0;
            input.disposition_cost_krw = 0;
            input.acquired_on = given_date(2028, Month::January, 3);
            input.owner_occupied_from = given_date(2028, Month::January, 3);
            input.sold_on = given_date(2030, Month::January, 3);

            let result = rules
                .calculate_one_home_capital_gains_tax(input)
                .expect("최고 누진세율을 계산해야 한다");

            assert!(result.taxable_amount_krw > 1_000_000_000);
            assert_eq!(result.national.rate_ppm, 450_000);
            assert_eq!(result.local.rate_ppm, 45_000);
        }

        #[test]
        fn given_15억원에_팔아도_필요경비가_더클때_when_계산하면_then_양도차익과_세액은_0원이다() {
            let rules = create_property_tax_rules();
            let policy = given_capital_gains_policy();
            let mut input = given_capital_gains_input(&policy);
            input.acquisition_price_krw = 1_500_000_000;

            let result = rules
                .calculate_one_home_capital_gains_tax(input)
                .expect("고가주택의 0원 차익도 계산 이력을 만들어야 한다");

            assert_eq!(result.gross_gain_krw, 0);
            assert_eq!(result.high_value_gain_krw, 0);
            assert_eq!(result.total_tax_krw, 0);
        }
    }

    mod context_양도세_최소보유기간을_완료하지않은_경우 {
        use super::*;

        #[test]
        fn given_2주년_하루전_when_계산하면_then_지원하지않는_정책으로_거절한다() {
            let rules = create_property_tax_rules();
            let policy = given_capital_gains_policy();
            let mut input = given_capital_gains_input(&policy);
            input.acquired_on = given_date(2028, Month::January, 3);
            input.owner_occupied_from = given_date(2028, Month::January, 3);
            input.sold_on = given_date(2030, Month::January, 2);

            let result = rules.calculate_one_home_capital_gains_tax(input);

            assert_eq!(result, Err(PropertyTaxError::PolicyUnsupported));
        }
    }

    mod context_정책이나_입력값이_유효하지않은_경우 {
        use super::*;

        #[test]
        fn given_지방세_누진표가_누락된_정책_when_검증하면_then_거절한다() {
            let rules = create_property_tax_rules();
            let mut policy = given_policy();
            policy
                .capital_gains
                .rate_brackets
                .retain(|bracket| bracket.tax_scope == CapitalGainsTaxScope::National);

            let result = rules.validate_policy(&policy);

            assert_eq!(result, Err(PropertyTaxError::InvalidPolicy));
        }

        #[test]
        fn given_음수_취득부대비용_when_양도세를_계산하면_then_거절한다() {
            let rules = create_property_tax_rules();
            let policy = given_capital_gains_policy();
            let mut input = given_capital_gains_input(&policy);
            input.acquisition_incidental_cost_krw = -1;

            let result = rules.calculate_one_home_capital_gains_tax(input);

            assert_eq!(result, Err(PropertyTaxError::InvalidCapitalGainsTaxInput));
        }

        #[test]
        fn given_세액합이_i64을_넘는_정책과_기준가_when_재산세를_계산하면_then_overflow로_거절한다()
        {
            let rules = create_property_tax_rules();
            let mut policy = given_annual_policy();
            policy.official_value_ratio_ppm = 1_000_000;
            policy.fair_market_ratio_bands = vec![AnnualPropertyTaxFairMarketRatioBand {
                official_value_upper_bound_krw: None,
                fair_market_value_ratio_ppm: 1_000_000,
            }];
            policy.special_rate_official_value_maximum_krw = i64::MAX;
            policy.rate_brackets = [
                AnnualPropertyTaxRateSchedule::Special,
                AnnualPropertyTaxRateSchedule::Standard,
            ]
            .into_iter()
            .map(|rate_schedule| AnnualPropertyTaxRateBracket {
                rate_schedule,
                tax_base_upper_bound_krw: None,
                rate_ppm: 1_000_000,
                progressive_deduction_krw: 0,
            })
            .collect();
            policy.local_education_rate_ratio_ppm = 1_000_000;

            let result = rules.calculate_annual_property_tax(AnnualPropertyTaxInput {
                reference_value_krw: i64::MAX,
                household_home_count: 1,
                policy: &policy,
            });

            assert_eq!(result, Err(PropertyTaxError::ArithmeticOverflow));
        }
    }
}
