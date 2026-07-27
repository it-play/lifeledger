use std::collections::HashSet;
use std::sync::Arc;

use time::{Date, Month};

use super::types::{
    DualContributionBreakdown, DualContributionRatePolicy, EmployerContributionBreakdown,
    EmploymentWithholdingBreakdown, EmploymentWithholdingRow, LongTermCareBreakdown,
    OtherIncomeRewardBreakdown, OtherIncomeRewardPolicy, PAYROLL_RATE_SCALE_PPM, PayrollBreakdown,
    PayrollCalculationInput, PayrollError, PayrollInsuranceBreakdown, PayrollPeriod,
    PayrollPeriodInput, PayrollPolicy, PayrollRules,
};

struct V1PayrollRules;

pub fn create_payroll_rules() -> Arc<dyn PayrollRules> {
    Arc::new(V1PayrollRules)
}

impl PayrollRules for V1PayrollRules {
    fn validate_policy(&self, policy: &PayrollPolicy) -> Result<(), PayrollError> {
        validate_payroll_policy(policy)
    }

    fn schedule_period(&self, input: PayrollPeriodInput) -> Result<PayrollPeriod, PayrollError> {
        schedule_period(input)
    }

    fn calculate_payroll(
        &self,
        input: PayrollCalculationInput<'_>,
    ) -> Result<PayrollBreakdown, PayrollError> {
        validate_payroll_policy(input.policy)?;
        if input.dependents > super::types::MAX_PAYROLL_DEPENDENTS {
            return Err(PayrollError::InvalidDependents);
        }

        let period = schedule_period(input.period)?;
        if input.wanted_reward_gross_krw.is_some() && period.period_no != 1 {
            return Err(PayrollError::RewardOutsideFirstPeriod);
        }
        let average_monthly_salary_krw = checked_i128_to_i64(
            i128::from(input.period.annual_salary_krw)
                .checked_div(12)
                .ok_or(PayrollError::ArithmeticOverflow)?,
        )?;
        let monthly_insurance_assessed = period.gross_pay_krw > 0
            && (input.period.period_no > 1 || input.period.contract_start_date.day() == 1);

        let pension_policy = input.policy.national_pension;
        let pension_rounded_basis = floor_to_unit(
            average_monthly_salary_krw,
            pension_policy.monthly_income_rounding_unit_krw,
        )?;
        let pension_basis = pension_rounded_basis.clamp(
            pension_policy.minimum_monthly_income_krw,
            pension_policy.maximum_monthly_income_krw,
        );
        let national_pension = calculate_dual_contribution(
            monthly_insurance_assessed,
            pension_basis,
            pension_basis,
            pension_policy.contribution,
        )?;

        let health_policy = input.policy.health_insurance;
        let health_basis = floor_to_unit(
            average_monthly_salary_krw,
            health_policy.monthly_remuneration_rounding_unit_krw,
        )?;
        let health_insurance = calculate_dual_contribution(
            monthly_insurance_assessed,
            health_basis,
            health_basis,
            health_policy.contribution,
        )?;

        let long_term_care_policy = input.policy.long_term_care;
        let long_term_care = LongTermCareBreakdown {
            assessed: monthly_insurance_assessed,
            employee_health_premium_basis_krw: health_insurance.employee_amount_krw,
            employer_health_premium_basis_krw: health_insurance.employer_amount_krw,
            rate_numerator: long_term_care_policy.health_premium_rate_numerator,
            rate_denominator: long_term_care_policy.health_premium_rate_denominator,
            employee_rounding_unit_krw: long_term_care_policy.employee_rounding_unit_krw,
            employer_rounding_unit_krw: long_term_care_policy.employer_rounding_unit_krw,
            employee_amount_krw: calculate_assessed_ratio(
                monthly_insurance_assessed,
                health_insurance.employee_amount_krw,
                long_term_care_policy.health_premium_rate_numerator,
                long_term_care_policy.health_premium_rate_denominator,
                long_term_care_policy.employee_rounding_unit_krw,
            )?,
            employer_amount_krw: calculate_assessed_ratio(
                monthly_insurance_assessed,
                health_insurance.employer_amount_krw,
                long_term_care_policy.health_premium_rate_numerator,
                long_term_care_policy.health_premium_rate_denominator,
                long_term_care_policy.employer_rounding_unit_krw,
            )?,
        };

        let employment_policy = &input.policy.employment_insurance;
        let employment_employer_rate = employment_policy
            .employer_rates
            .iter()
            .find(|row| row.employer_size_band == input.employer_size_band)
            .map(|row| row.rate_ppm)
            .ok_or(PayrollError::MissingEmployerSizeRate)?;
        let employment_insurance = calculate_dual_contribution(
            true,
            period.gross_pay_krw,
            period.gross_pay_krw,
            DualContributionRatePolicy {
                employee_rate_ppm: employment_policy.employee_rate_ppm,
                employer_rate_ppm: employment_employer_rate,
                employee_rounding_unit_krw: employment_policy.employee_rounding_unit_krw,
                employer_rounding_unit_krw: employment_policy.employer_rounding_unit_krw,
            },
        )?;

        let industrial_policy = &input.policy.industrial_accident;
        let industrial_rate = industrial_policy
            .employer_rates
            .iter()
            .find(|row| row.industry == input.industry)
            .map(|row| row.rate_ppm)
            .ok_or(PayrollError::MissingIndustryRate)?;
        let industrial_accident = EmployerContributionBreakdown {
            basis_krw: period.gross_pay_krw,
            rate_ppm: industrial_rate,
            rounding_unit_krw: industrial_policy.employer_rounding_unit_krw,
            employer_amount_krw: floor_rate_ppm(
                period.gross_pay_krw,
                industrial_rate,
                industrial_policy.employer_rounding_unit_krw,
            )?,
        };

        let employee_insurance_total_krw = checked_sum(&[
            national_pension.employee_amount_krw,
            health_insurance.employee_amount_krw,
            long_term_care.employee_amount_krw,
            employment_insurance.employee_amount_krw,
        ])?;
        let employer_insurance_total_krw = checked_sum(&[
            national_pension.employer_amount_krw,
            health_insurance.employer_amount_krw,
            long_term_care.employer_amount_krw,
            employment_insurance.employer_amount_krw,
            industrial_accident.employer_amount_krw,
        ])?;
        let insurance = PayrollInsuranceBreakdown {
            national_pension,
            health_insurance,
            long_term_care,
            employment_insurance,
            industrial_accident,
            employee_total_krw: employee_insurance_total_krw,
            employer_total_krw: employer_insurance_total_krw,
        };

        let family_count = input
            .dependents
            .checked_add(1)
            .ok_or(PayrollError::ArithmeticOverflow)?;
        let withholding_row = find_withholding_row(
            &input.policy.employment_withholding_table,
            period.gross_pay_krw,
            family_count,
            0,
        )?;
        let withheld_income_tax_krw = withholding_row.income_tax_krw;
        let local_policy = input.policy.local_income_withholding;
        let withheld_local_income_tax_krw = floor_rate_ppm(
            withheld_income_tax_krw,
            local_policy.income_tax_rate_ppm,
            local_policy.rounding_unit_krw,
        )?;
        let withholding = EmploymentWithholdingBreakdown {
            taxable_gross_krw: period.gross_pay_krw,
            family_count,
            child_count: 0,
            row_lower_bound_krw: withholding_row.lower_bound_krw,
            row_upper_bound_exclusive_krw: withholding_row.upper_bound_exclusive_krw,
            income_tax_krw: withheld_income_tax_krw,
            local_income_tax_basis_krw: withheld_income_tax_krw,
            local_income_tax_rate_ppm: local_policy.income_tax_rate_ppm,
            local_income_tax_rounding_unit_krw: local_policy.rounding_unit_krw,
            local_income_tax_krw: withheld_local_income_tax_krw,
        };

        let net_salary_pay_krw = checked_i128_to_i64(
            i128::from(period.gross_pay_krw)
                .checked_sub(i128::from(employee_insurance_total_krw))
                .and_then(|value| value.checked_sub(i128::from(withheld_income_tax_krw)))
                .and_then(|value| value.checked_sub(i128::from(withheld_local_income_tax_krw)))
                .ok_or(PayrollError::ArithmeticOverflow)?,
        )?;
        if net_salary_pay_krw < 0 {
            return Err(PayrollError::NegativeNetPay);
        }

        let wanted_reward =
            calculate_reward(input.wanted_reward_gross_krw, input.policy.wanted_reward)?;
        let reward_net_krw = wanted_reward.map_or(0, |reward| reward.net_reward_krw);
        let total_wallet_credit_krw = checked_sum(&[net_salary_pay_krw, reward_net_krw])?;

        Ok(PayrollBreakdown {
            period,
            insurance,
            withholding,
            employee_insurance_total_krw,
            employer_insurance_total_krw,
            withheld_income_tax_krw,
            withheld_local_income_tax_krw,
            net_salary_pay_krw,
            employment_income_accrual_krw: period.gross_pay_krw,
            wanted_reward,
            total_wallet_credit_krw,
        })
    }
}

fn schedule_period(input: PayrollPeriodInput) -> Result<PayrollPeriod, PayrollError> {
    if input.contract_id == 0 {
        return Err(PayrollError::InvalidContractId);
    }
    if input.period_no == 0 {
        return Err(PayrollError::InvalidPeriodNo);
    }
    if input.annual_salary_krw <= 0 {
        return Err(PayrollError::InvalidAnnualSalary);
    }
    if !(1..=31).contains(&input.payday_day_of_month) {
        return Err(PayrollError::InvalidPayday);
    }

    let period_offset = input
        .period_no
        .checked_sub(1)
        .ok_or(PayrollError::ArithmeticOverflow)?;
    let period_month_start = month_start_with_offset(input.contract_start_date, period_offset)?;
    let period_end_exclusive_date =
        month_start_with_offset(input.contract_start_date, input.period_no)?;
    let period_start_date = if input.period_no == 1 {
        input.contract_start_date
    } else {
        period_month_start
    };
    let calendar_days = checked_days(period_month_start, period_end_exclusive_date)?;
    let covered_days = checked_days(period_start_date, period_end_exclusive_date)?;
    if covered_days == 0 || covered_days > calendar_days {
        return Err(PayrollError::InvalidDate);
    }

    let salary_month_ordinal =
        u8::try_from((period_offset % 12) + 1).map_err(|_| PayrollError::ArithmeticOverflow)?;
    let annual_salary = i128::from(input.annual_salary_krw);
    let quotient = annual_salary
        .checked_div(12)
        .ok_or(PayrollError::ArithmeticOverflow)?;
    let remainder = annual_salary
        .checked_rem(12)
        .ok_or(PayrollError::ArithmeticOverflow)?;
    let extra_won = i128::from(salary_month_ordinal) <= remainder;
    let base_monthly_salary_krw = checked_i128_to_i64(
        quotient
            .checked_add(i128::from(extra_won))
            .ok_or(PayrollError::ArithmeticOverflow)?,
    )?;
    let gross_pay_krw = if input.period_no == 1 {
        checked_i128_to_i64(
            i128::from(base_monthly_salary_krw)
                .checked_mul(i128::from(covered_days))
                .and_then(|value| value.checked_div(i128::from(calendar_days)))
                .ok_or(PayrollError::ArithmeticOverflow)?,
        )?
    } else {
        base_monthly_salary_krw
    };
    let payday = clamped_date(
        period_end_exclusive_date.year(),
        period_end_exclusive_date.month(),
        input.payday_day_of_month,
    )?;

    Ok(PayrollPeriod {
        contract_id: input.contract_id,
        period_no: input.period_no,
        salary_month_ordinal,
        period_start_date,
        period_end_exclusive_date,
        payday,
        calendar_days,
        covered_days,
        base_monthly_salary_krw,
        gross_pay_krw,
    })
}

fn validate_payroll_policy(policy: &PayrollPolicy) -> Result<(), PayrollError> {
    let pension = policy.national_pension;
    validate_rounding_unit(pension.monthly_income_rounding_unit_krw)?;
    if pension.minimum_monthly_income_krw <= 0
        || pension.maximum_monthly_income_krw < pension.minimum_monthly_income_krw
    {
        return Err(PayrollError::InvalidNationalPensionBounds);
    }
    validate_dual_rate_policy(pension.contribution)?;

    let health = policy.health_insurance;
    validate_rounding_unit(health.monthly_remuneration_rounding_unit_krw)?;
    validate_dual_rate_policy(health.contribution)?;

    let long_term_care = policy.long_term_care;
    if long_term_care.health_premium_rate_numerator <= 0
        || long_term_care.health_premium_rate_denominator <= 0
        || long_term_care.health_premium_rate_numerator
            > long_term_care.health_premium_rate_denominator
    {
        return Err(PayrollError::InvalidRate);
    }
    validate_rounding_unit(long_term_care.employee_rounding_unit_krw)?;
    validate_rounding_unit(long_term_care.employer_rounding_unit_krw)?;

    let employment = &policy.employment_insurance;
    validate_rate(employment.employee_rate_ppm)?;
    validate_rounding_unit(employment.employee_rounding_unit_krw)?;
    validate_rounding_unit(employment.employer_rounding_unit_krw)?;
    if employment.employer_rates.is_empty() {
        return Err(PayrollError::MissingEmployerSizeRate);
    }
    let mut employer_sizes = HashSet::with_capacity(employment.employer_rates.len());
    for row in &employment.employer_rates {
        validate_rate(row.rate_ppm)?;
        if !employer_sizes.insert(row.employer_size_band) {
            return Err(PayrollError::DuplicateEmployerSizeRate);
        }
    }

    let industrial = &policy.industrial_accident;
    validate_rounding_unit(industrial.employer_rounding_unit_krw)?;
    if industrial.employer_rates.is_empty() {
        return Err(PayrollError::MissingIndustryRate);
    }
    let mut industries = HashSet::with_capacity(industrial.employer_rates.len());
    for row in &industrial.employer_rates {
        validate_rate(row.rate_ppm)?;
        if !industries.insert(row.industry) {
            return Err(PayrollError::DuplicateIndustryRate);
        }
    }

    validate_withholding_table(&policy.employment_withholding_table)?;
    validate_rate(policy.local_income_withholding.income_tax_rate_ppm)?;
    validate_rounding_unit(policy.local_income_withholding.rounding_unit_krw)?;
    if let Some(reward) = policy.wanted_reward {
        validate_reward_policy(reward)?;
    }
    Ok(())
}

fn validate_dual_rate_policy(policy: DualContributionRatePolicy) -> Result<(), PayrollError> {
    validate_rate(policy.employee_rate_ppm)?;
    validate_rate(policy.employer_rate_ppm)?;
    validate_rounding_unit(policy.employee_rounding_unit_krw)?;
    validate_rounding_unit(policy.employer_rounding_unit_krw)
}

fn validate_reward_policy(policy: OtherIncomeRewardPolicy) -> Result<(), PayrollError> {
    validate_rate(policy.income_tax_rate_ppm)?;
    validate_rate(policy.local_income_tax_rate_ppm)?;
    validate_rounding_unit(policy.income_tax_rounding_unit_krw)?;
    validate_rounding_unit(policy.local_income_tax_rounding_unit_krw)
}

fn validate_rate(rate_ppm: i64) -> Result<(), PayrollError> {
    if !(1..=PAYROLL_RATE_SCALE_PPM).contains(&rate_ppm) {
        return Err(PayrollError::InvalidRate);
    }
    Ok(())
}

fn validate_rounding_unit(unit_krw: i64) -> Result<(), PayrollError> {
    if unit_krw <= 0 {
        return Err(PayrollError::InvalidRoundingUnit);
    }
    Ok(())
}

fn validate_withholding_table(rows: &[EmploymentWithholdingRow]) -> Result<(), PayrollError> {
    if rows.is_empty() {
        return Err(PayrollError::MissingWithholdingFamily);
    }
    for row in rows {
        if row.lower_bound_krw < 0
            || row.family_count == 0
            || row.income_tax_krw < 0
            || row
                .upper_bound_exclusive_krw
                .is_some_and(|upper| upper <= row.lower_bound_krw)
        {
            return Err(PayrollError::InvalidWithholdingRow);
        }
    }
    for (index, left) in rows.iter().enumerate() {
        for right in &rows[index + 1..] {
            if left.family_count == right.family_count
                && left.child_count == right.child_count
                && ranges_overlap(*left, *right)
            {
                return Err(PayrollError::OverlappingWithholdingRows);
            }
        }
    }

    for family_count in 1..=super::types::MAX_PAYROLL_DEPENDENTS + 1 {
        let mut family_rows = rows
            .iter()
            .filter(|row| row.family_count == family_count && row.child_count == 0)
            .copied()
            .collect::<Vec<_>>();
        if family_rows.is_empty() {
            return Err(PayrollError::MissingWithholdingFamily);
        }
        family_rows.sort_by_key(|row| row.lower_bound_krw);
        let mut expected_lower_bound_krw = 0;
        let mut reached_endless_row = false;
        for row in family_rows {
            if reached_endless_row || row.lower_bound_krw != expected_lower_bound_krw {
                return Err(PayrollError::WithholdingOutOfRange);
            }
            match row.upper_bound_exclusive_krw {
                Some(upper) => expected_lower_bound_krw = upper,
                None => reached_endless_row = true,
            }
        }
        if !reached_endless_row {
            return Err(PayrollError::WithholdingOutOfRange);
        }
    }
    Ok(())
}

fn ranges_overlap(left: EmploymentWithholdingRow, right: EmploymentWithholdingRow) -> bool {
    let left_reaches_right = left
        .upper_bound_exclusive_krw
        .is_none_or(|upper| right.lower_bound_krw < upper);
    let right_reaches_left = right
        .upper_bound_exclusive_krw
        .is_none_or(|upper| left.lower_bound_krw < upper);
    left_reaches_right && right_reaches_left
}

fn find_withholding_row(
    rows: &[EmploymentWithholdingRow],
    taxable_gross_krw: i64,
    family_count: u8,
    child_count: u8,
) -> Result<EmploymentWithholdingRow, PayrollError> {
    let family_rows = rows
        .iter()
        .filter(|row| row.family_count == family_count && row.child_count == child_count)
        .copied()
        .collect::<Vec<_>>();
    if family_rows.is_empty() {
        return Err(PayrollError::MissingWithholdingFamily);
    }
    family_rows
        .into_iter()
        .find(|row| {
            taxable_gross_krw >= row.lower_bound_krw
                && row
                    .upper_bound_exclusive_krw
                    .is_none_or(|upper| taxable_gross_krw < upper)
        })
        .ok_or(PayrollError::WithholdingOutOfRange)
}

fn calculate_dual_contribution(
    assessed: bool,
    employee_basis_krw: i64,
    employer_basis_krw: i64,
    policy: DualContributionRatePolicy,
) -> Result<DualContributionBreakdown, PayrollError> {
    Ok(DualContributionBreakdown {
        assessed,
        employee_basis_krw,
        employer_basis_krw,
        employee_rate_ppm: policy.employee_rate_ppm,
        employer_rate_ppm: policy.employer_rate_ppm,
        employee_rounding_unit_krw: policy.employee_rounding_unit_krw,
        employer_rounding_unit_krw: policy.employer_rounding_unit_krw,
        employee_amount_krw: calculate_assessed_rate(
            assessed,
            employee_basis_krw,
            policy.employee_rate_ppm,
            policy.employee_rounding_unit_krw,
        )?,
        employer_amount_krw: calculate_assessed_rate(
            assessed,
            employer_basis_krw,
            policy.employer_rate_ppm,
            policy.employer_rounding_unit_krw,
        )?,
    })
}

fn calculate_assessed_rate(
    assessed: bool,
    basis_krw: i64,
    rate_ppm: i64,
    rounding_unit_krw: i64,
) -> Result<i64, PayrollError> {
    if assessed {
        floor_rate_ppm(basis_krw, rate_ppm, rounding_unit_krw)
    } else {
        Ok(0)
    }
}

fn calculate_assessed_ratio(
    assessed: bool,
    basis_krw: i64,
    numerator: i64,
    denominator: i64,
    rounding_unit_krw: i64,
) -> Result<i64, PayrollError> {
    if assessed {
        floor_ratio_to_unit(basis_krw, numerator, denominator, rounding_unit_krw)
    } else {
        Ok(0)
    }
}

fn calculate_reward(
    gross_reward_krw: Option<i64>,
    policy: Option<OtherIncomeRewardPolicy>,
) -> Result<Option<OtherIncomeRewardBreakdown>, PayrollError> {
    let Some(gross_reward_krw) = gross_reward_krw else {
        return Ok(None);
    };
    if gross_reward_krw <= 0 {
        return Err(PayrollError::InvalidReward);
    }
    let policy = policy.ok_or(PayrollError::MissingRewardPolicy)?;
    let withheld_income_tax_krw = floor_rate_ppm(
        gross_reward_krw,
        policy.income_tax_rate_ppm,
        policy.income_tax_rounding_unit_krw,
    )?;
    let withheld_local_income_tax_krw = floor_rate_ppm(
        gross_reward_krw,
        policy.local_income_tax_rate_ppm,
        policy.local_income_tax_rounding_unit_krw,
    )?;
    let net_reward_krw = checked_i128_to_i64(
        i128::from(gross_reward_krw)
            .checked_sub(i128::from(withheld_income_tax_krw))
            .and_then(|value| value.checked_sub(i128::from(withheld_local_income_tax_krw)))
            .ok_or(PayrollError::ArithmeticOverflow)?,
    )?;
    if net_reward_krw < 0 {
        return Err(PayrollError::InvalidReward);
    }
    Ok(Some(OtherIncomeRewardBreakdown {
        gross_reward_krw,
        income_tax_rate_ppm: policy.income_tax_rate_ppm,
        local_income_tax_rate_ppm: policy.local_income_tax_rate_ppm,
        income_tax_rounding_unit_krw: policy.income_tax_rounding_unit_krw,
        local_income_tax_rounding_unit_krw: policy.local_income_tax_rounding_unit_krw,
        withheld_income_tax_krw,
        withheld_local_income_tax_krw,
        net_reward_krw,
    }))
}

fn floor_rate_ppm(
    amount_krw: i64,
    rate_ppm: i64,
    rounding_unit_krw: i64,
) -> Result<i64, PayrollError> {
    floor_ratio_to_unit(
        amount_krw,
        rate_ppm,
        PAYROLL_RATE_SCALE_PPM,
        rounding_unit_krw,
    )
}

fn floor_ratio_to_unit(
    amount_krw: i64,
    numerator: i64,
    denominator: i64,
    rounding_unit_krw: i64,
) -> Result<i64, PayrollError> {
    if amount_krw < 0 {
        return Err(PayrollError::InvalidAnnualSalary);
    }
    let divisor = i128::from(denominator)
        .checked_mul(i128::from(rounding_unit_krw))
        .ok_or(PayrollError::ArithmeticOverflow)?;
    let units = i128::from(amount_krw)
        .checked_mul(i128::from(numerator))
        .and_then(|value| value.checked_div(divisor))
        .ok_or(PayrollError::ArithmeticOverflow)?;
    checked_i128_to_i64(
        units
            .checked_mul(i128::from(rounding_unit_krw))
            .ok_or(PayrollError::ArithmeticOverflow)?,
    )
}

fn floor_to_unit(amount_krw: i64, unit_krw: i64) -> Result<i64, PayrollError> {
    checked_i128_to_i64(
        i128::from(amount_krw)
            .checked_div(i128::from(unit_krw))
            .and_then(|value| value.checked_mul(i128::from(unit_krw)))
            .ok_or(PayrollError::ArithmeticOverflow)?,
    )
}

fn checked_sum(values: &[i64]) -> Result<i64, PayrollError> {
    let total = values.iter().try_fold(0_i128, |total, value| {
        total
            .checked_add(i128::from(*value))
            .ok_or(PayrollError::ArithmeticOverflow)
    })?;
    checked_i128_to_i64(total)
}

fn checked_i128_to_i64(value: i128) -> Result<i64, PayrollError> {
    i64::try_from(value).map_err(|_| PayrollError::ArithmeticOverflow)
}

fn checked_days(start: Date, end_exclusive: Date) -> Result<u16, PayrollError> {
    let days = (end_exclusive - start).whole_days();
    if days <= 0 {
        return Err(PayrollError::InvalidDate);
    }
    u16::try_from(days).map_err(|_| PayrollError::ArithmeticOverflow)
}

fn month_start_with_offset(date: Date, offset: u64) -> Result<Date, PayrollError> {
    let base_month = i128::from(date.year())
        .checked_mul(12)
        .and_then(|value| value.checked_add(i128::from(u8::from(date.month())) - 1))
        .ok_or(PayrollError::ArithmeticOverflow)?;
    let target_month = base_month
        .checked_add(i128::from(offset))
        .ok_or(PayrollError::ArithmeticOverflow)?;
    let year = i32::try_from(target_month.div_euclid(12)).map_err(|_| PayrollError::InvalidDate)?;
    let month_number =
        u8::try_from(target_month.rem_euclid(12) + 1).map_err(|_| PayrollError::InvalidDate)?;
    let month = Month::try_from(month_number).map_err(|_| PayrollError::InvalidDate)?;
    Date::from_calendar_date(year, month, 1).map_err(|_| PayrollError::InvalidDate)
}

fn clamped_date(year: i32, month: Month, desired_day: u8) -> Result<Date, PayrollError> {
    for day in (1..=desired_day).rev() {
        if let Ok(date) = Date::from_calendar_date(year, month, day) {
            return Ok(date);
        }
    }
    Err(PayrollError::InvalidDate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::career::types::{
        DualContributionRatePolicy, EmployerContributionRate, EmployerSizeBand,
        EmploymentInsurancePolicy, HealthInsurancePolicy, IndustrialAccidentPolicy, Industry,
        IndustryContributionRate, LocalIncomeWithholdingPolicy, LongTermCarePolicy,
        NationalPensionPolicy, OtherIncomeRewardPolicy, PayrollCalculationInput, PayrollPolicy,
    };

    fn given_date(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("유효한 테스트 날짜여야 한다")
    }

    fn given_dual_rate_policy() -> DualContributionRatePolicy {
        DualContributionRatePolicy {
            employee_rate_ppm: 50_000,
            employer_rate_ppm: 50_000,
            employee_rounding_unit_krw: 10,
            employer_rounding_unit_krw: 10,
        }
    }

    fn given_policy() -> PayrollPolicy {
        let mut employment_withholding_table = Vec::new();
        for family_count in 1..=7 {
            employment_withholding_table.extend([
                EmploymentWithholdingRow {
                    lower_bound_krw: 0,
                    upper_bound_exclusive_krw: Some(2_000_000),
                    family_count,
                    child_count: 0,
                    income_tax_krw: 0,
                },
                EmploymentWithholdingRow {
                    lower_bound_krw: 2_000_000,
                    upper_bound_exclusive_krw: Some(4_000_000),
                    family_count,
                    child_count: 0,
                    income_tax_krw: 100_000,
                },
                EmploymentWithholdingRow {
                    lower_bound_krw: 4_000_000,
                    upper_bound_exclusive_krw: None,
                    family_count,
                    child_count: 0,
                    income_tax_krw: 300_000,
                },
            ]);
        }

        PayrollPolicy {
            national_pension: NationalPensionPolicy {
                monthly_income_rounding_unit_krw: 1_000,
                minimum_monthly_income_krw: 1_000_000,
                maximum_monthly_income_krw: 5_000_000,
                contribution: given_dual_rate_policy(),
            },
            health_insurance: HealthInsurancePolicy {
                monthly_remuneration_rounding_unit_krw: 1_000,
                contribution: DualContributionRatePolicy {
                    employee_rate_ppm: 40_000,
                    employer_rate_ppm: 40_000,
                    employee_rounding_unit_krw: 10,
                    employer_rounding_unit_krw: 10,
                },
            },
            long_term_care: LongTermCarePolicy {
                health_premium_rate_numerator: 1,
                health_premium_rate_denominator: 10,
                employee_rounding_unit_krw: 10,
                employer_rounding_unit_krw: 10,
            },
            employment_insurance: EmploymentInsurancePolicy {
                employee_rate_ppm: 10_000,
                employer_rates: vec![EmployerContributionRate {
                    employer_size_band: EmployerSizeBand::Under150,
                    rate_ppm: 20_000,
                }],
                employee_rounding_unit_krw: 10,
                employer_rounding_unit_krw: 10,
            },
            industrial_accident: IndustrialAccidentPolicy {
                employer_rates: vec![IndustryContributionRate {
                    industry: Industry::ItSoftware,
                    rate_ppm: 5_000,
                }],
                employer_rounding_unit_krw: 10,
            },
            employment_withholding_table,
            local_income_withholding: LocalIncomeWithholdingPolicy {
                income_tax_rate_ppm: 100_000,
                rounding_unit_krw: 10,
            },
            wanted_reward: Some(OtherIncomeRewardPolicy {
                income_tax_rate_ppm: 200_000,
                local_income_tax_rate_ppm: 20_000,
                income_tax_rounding_unit_krw: 10,
                local_income_tax_rounding_unit_krw: 10,
            }),
        }
    }

    fn given_period(
        contract_id: u64,
        period_no: u64,
        contract_start_date: Date,
        annual_salary_krw: i64,
        payday_day_of_month: u8,
    ) -> PayrollPeriodInput {
        PayrollPeriodInput {
            contract_id,
            period_no,
            contract_start_date,
            annual_salary_krw,
            payday_day_of_month,
        }
    }

    fn when_calculate(
        period: PayrollPeriodInput,
        policy: &PayrollPolicy,
        dependents: u8,
        reward_krw: Option<i64>,
    ) -> Result<PayrollBreakdown, PayrollError> {
        create_payroll_rules().calculate_payroll(PayrollCalculationInput {
            period,
            dependents,
            employer_size_band: EmployerSizeBand::Under150,
            industry: Industry::ItSoftware,
            wanted_reward_gross_krw: reward_krw,
            policy,
        })
    }

    mod context_연봉을_계약_급여월로_분할하는_경우 {
        use super::*;

        #[test]
        fn given_12로_나눈_나머지가_11원_when_12개_period를_계산하면_then_연봉과_정확히_일치한다() {
            let rules = create_payroll_rules();
            let annual_salary_krw = 48_000_011;
            let mut total = 0_i128;

            for period_no in 1..=12 {
                let period = rules
                    .schedule_period(given_period(
                        7,
                        period_no,
                        given_date(2026, Month::January, 1),
                        annual_salary_krw,
                        25,
                    ))
                    .expect("급여 period를 계산해야 한다");
                let expected = if period_no <= 11 {
                    4_000_001
                } else {
                    4_000_000
                };
                assert_eq!(period.base_monthly_salary_krw, expected);
                total += i128::from(period.base_monthly_salary_krw);
            }

            assert_eq!(total, i128::from(annual_salary_krw));
        }

        #[test]
        fn given_같은계약의_12회차와_13회차_when_schedule하면_then_identity는_다르고_ordinal은_1로_돌아온다()
         {
            let rules = create_payroll_rules();
            let start = given_date(2026, Month::January, 1);

            let twelfth = rules
                .schedule_period(given_period(9, 12, start, 48_000_011, 25))
                .expect("12회차를 계산해야 한다");
            let thirteenth = rules
                .schedule_period(given_period(9, 13, start, 48_000_011, 25))
                .expect("13회차를 계산해야 한다");

            assert_ne!(
                (twelfth.contract_id, twelfth.period_no),
                (thirteenth.contract_id, thirteenth.period_no)
            );
            assert_eq!(twelfth.salary_month_ordinal, 12);
            assert_eq!(thirteenth.salary_month_ordinal, 1);
            assert_eq!(thirteenth.base_monthly_salary_krw, 4_000_001);
        }
    }

    mod context_달력_경계에_급여일을_예약하는_경우 {
        use super::*;

        #[test]
        fn given_윤년_1월31일_입사와_31일_payday_when_첫_period를_계산하면_then_2월29일로_당긴다() {
            let period = create_payroll_rules()
                .schedule_period(given_period(
                    1,
                    1,
                    given_date(2024, Month::January, 31),
                    36_000_000,
                    31,
                ))
                .expect("윤년 급여일을 계산해야 한다");

            assert_eq!(
                period.period_start_date,
                given_date(2024, Month::January, 31)
            );
            assert_eq!(
                period.period_end_exclusive_date,
                given_date(2024, Month::February, 1)
            );
            assert_eq!(period.payday, given_date(2024, Month::February, 29));
            assert_eq!(period.covered_days, 1);
            assert_eq!(period.calendar_days, 31);
        }

        #[test]
        fn given_윤년_2월29일_입사_when_첫_period를_계산하면_then_29일중_1일만_일할한다() {
            let period = create_payroll_rules()
                .schedule_period(given_period(
                    1,
                    1,
                    given_date(2024, Month::February, 29),
                    34_800_000,
                    25,
                ))
                .expect("윤년 부분월을 계산해야 한다");

            assert_eq!(period.calendar_days, 29);
            assert_eq!(period.covered_days, 1);
            assert_eq!(period.base_monthly_salary_krw, 2_900_000);
            assert_eq!(period.gross_pay_krw, 100_000);
            assert_eq!(period.payday, given_date(2024, Month::March, 25));
        }

        #[test]
        fn given_1월16일_입사_when_첫_period를_계산하면_then_16일을_31일로_나눠_내림한다() {
            let period = create_payroll_rules()
                .schedule_period(given_period(
                    1,
                    1,
                    given_date(2026, Month::January, 16),
                    36_000_000,
                    25,
                ))
                .expect("부분월을 계산해야 한다");

            assert_eq!(period.covered_days, 16);
            assert_eq!(period.gross_pay_krw, 1_548_387);
        }
    }

    mod context_취득월_보험료를_계산하는_경우 {
        use super::*;

        #[test]
        fn given_1일_입사_when_첫급여를_계산하면_then_월보험과_gross보험을_모두_부과한다() {
            let policy = given_policy();

            let result = when_calculate(
                given_period(1, 1, given_date(2026, Month::January, 1), 48_000_000, 25),
                &policy,
                0,
                None,
            )
            .expect("1일 입사 급여를 계산해야 한다");

            assert!(result.insurance.national_pension.assessed);
            assert!(result.insurance.health_insurance.assessed);
            assert!(result.insurance.long_term_care.assessed);
            assert!(result.insurance.employment_insurance.assessed);
            assert!(result.insurance.national_pension.employee_amount_krw > 0);
            assert!(result.insurance.employment_insurance.employee_amount_krw > 0);
        }

        #[test]
        fn given_2일_입사_when_첫급여를_계산하면_then_월보험은_0이고_gross보험만_부과한다() {
            let policy = given_policy();

            let result = when_calculate(
                given_period(1, 1, given_date(2026, Month::January, 2), 48_000_000, 25),
                &policy,
                0,
                None,
            )
            .expect("2일 입사 급여를 계산해야 한다");

            assert!(!result.insurance.national_pension.assessed);
            assert!(!result.insurance.health_insurance.assessed);
            assert_eq!(result.insurance.national_pension.employee_amount_krw, 0);
            assert_eq!(result.insurance.health_insurance.employee_amount_krw, 0);
            assert_eq!(result.insurance.long_term_care.employee_amount_krw, 0);
            assert!(result.insurance.employment_insurance.employee_amount_krw > 0);
            assert!(result.insurance.industrial_accident.employer_amount_krw > 0);
        }

        #[test]
        fn given_월급_gross가_0원_when_계산하면_then_보험과_세금과_net이_모두_0원이다() {
            let policy = given_policy();

            let result = when_calculate(
                given_period(1, 2, given_date(2026, Month::January, 1), 1, 25),
                &policy,
                0,
                None,
            )
            .expect("0원 급여는 no movement로 계산해야 한다");

            assert_eq!(result.period.gross_pay_krw, 0);
            assert_eq!(result.employee_insurance_total_krw, 0);
            assert_eq!(result.employer_insurance_total_krw, 0);
            assert_eq!(result.withheld_income_tax_krw, 0);
            assert_eq!(result.withheld_local_income_tax_krw, 0);
            assert_eq!(result.net_salary_pay_krw, 0);
            assert_eq!(result.total_wallet_credit_krw, 0);
        }
    }

    mod context_보험_기준액과_절사단위를_적용하는_경우 {
        use super::*;

        #[test]
        fn given_상한보다_높은_월평균과_끝자리_요율_when_계산하면_then_상한후_10원미만을_버린다() {
            let mut policy = given_policy();
            policy.national_pension.contribution.employee_rate_ppm = 47_501;

            let result = when_calculate(
                given_period(1, 1, given_date(2026, Month::January, 1), 120_000_011, 25),
                &policy,
                0,
                None,
            )
            .expect("상한 보험료를 계산해야 한다");

            assert_eq!(
                result.insurance.national_pension.employee_basis_krw,
                5_000_000
            );
            assert_eq!(
                result.insurance.national_pension.employee_amount_krw,
                237_500
            );
            assert_eq!(
                result.insurance.health_insurance.employee_basis_krw,
                10_000_000
            );
        }

        #[test]
        fn given_하한보다_낮은_월평균_when_계산하면_then_국민연금_하한을_기준으로_쓴다() {
            let policy = given_policy();

            let result = when_calculate(
                given_period(1, 1, given_date(2026, Month::January, 1), 6_000_011, 25),
                &policy,
                0,
                None,
            )
            .expect("하한 보험료를 계산해야 한다");

            assert_eq!(
                result.insurance.national_pension.employee_basis_krw,
                1_000_000
            );
            assert_eq!(
                result.insurance.national_pension.employee_amount_krw,
                50_000
            );
        }

        #[test]
        fn given_건강보험_양측금액이_다를때_when_장기요양을_계산하면_then_각_보험료를_독립_basis로_쓴다()
         {
            let mut policy = given_policy();
            policy.health_insurance.contribution.employer_rate_ppm = 50_000;

            let result = when_calculate(
                given_period(1, 1, given_date(2026, Month::January, 1), 48_000_000, 25),
                &policy,
                0,
                None,
            )
            .expect("장기요양 양측 보험료를 계산해야 한다");

            assert_eq!(
                result
                    .insurance
                    .long_term_care
                    .employee_health_premium_basis_krw,
                160_000
            );
            assert_eq!(
                result
                    .insurance
                    .long_term_care
                    .employer_health_premium_basis_krw,
                200_000
            );
            assert_eq!(result.insurance.long_term_care.employee_amount_krw, 16_000);
            assert_eq!(result.insurance.long_term_care.employer_amount_krw, 20_000);
        }
    }

    mod context_간이세액표를_조회하는_경우 {
        use super::*;

        #[test]
        fn given_상한_exclusive와_같은_과세급여_when_조회하면_then_다음_row를_쓴다() {
            let policy = given_policy();

            let result = when_calculate(
                given_period(1, 1, given_date(2026, Month::January, 1), 48_000_000, 25),
                &policy,
                0,
                None,
            )
            .expect("경계 row를 조회해야 한다");

            assert_eq!(result.withholding.row_lower_bound_krw, 4_000_000);
            assert_eq!(result.withheld_income_tax_krw, 300_000);
            assert_eq!(result.withholding.family_count, 1);
            assert_eq!(result.withholding.child_count, 0);
        }

        #[test]
        fn given_부양가족_6명_when_조회하면_then_가족수_7과_자녀수_0을_쓴다() {
            let policy = given_policy();

            let result = when_calculate(
                given_period(1, 1, given_date(2026, Month::January, 1), 48_000_000, 25),
                &policy,
                6,
                None,
            )
            .expect("최대 부양가족 row를 조회해야 한다");

            assert_eq!(result.withholding.family_count, 7);
            assert_eq!(result.withholding.child_count, 0);
        }

        #[test]
        fn given_부양가족에_해당하는_row가_없을때_when_조회하면_then_policy오류다() {
            let mut policy = given_policy();
            policy
                .employment_withholding_table
                .retain(|row| row.family_count != 2);

            let result = when_calculate(
                given_period(1, 1, given_date(2026, Month::January, 1), 48_000_000, 25),
                &policy,
                1,
                None,
            );

            assert_eq!(result, Err(PayrollError::MissingWithholdingFamily));
        }

        #[test]
        fn given_가족row에_급여구간이_비었을때_when_조회하면_then_범위오류다() {
            let mut policy = given_policy();
            policy
                .employment_withholding_table
                .retain(|row| row.family_count != 1 || row.lower_bound_krw != 2_000_000);

            let result = when_calculate(
                given_period(1, 1, given_date(2026, Month::January, 1), 48_000_000, 25),
                &policy,
                0,
                None,
            );

            assert_eq!(result, Err(PayrollError::WithholdingOutOfRange));
        }
    }

    mod context_원티드_보상을_함께_지급하는_경우 {
        use super::*;

        #[test]
        fn given_별도_보상_when_급여와_계산하면_then_보험과_근로소득에서_분리하고_wallet만_합친다()
        {
            let policy = given_policy();

            let result = when_calculate(
                given_period(1, 1, given_date(2026, Month::January, 1), 48_000_000, 25),
                &policy,
                0,
                Some(1_000_000),
            )
            .expect("보상과 급여를 계산해야 한다");
            let reward = result.wanted_reward.expect("보상 breakdown이 있어야 한다");

            assert_eq!(reward.withheld_income_tax_krw, 200_000);
            assert_eq!(reward.withheld_local_income_tax_krw, 20_000);
            assert_eq!(reward.net_reward_krw, 780_000);
            assert_eq!(
                result.employment_income_accrual_krw,
                result.period.gross_pay_krw
            );
            assert_eq!(
                result.total_wallet_credit_krw,
                result.net_salary_pay_krw + reward.net_reward_krw
            );
        }

        #[test]
        fn given_2회차에_보상입력_when_계산하면_then_첫급여_전용오류로_거절한다() {
            let policy = given_policy();

            let result = when_calculate(
                given_period(1, 2, given_date(2026, Month::January, 1), 48_000_000, 25),
                &policy,
                0,
                Some(1_000_000),
            );

            assert_eq!(result, Err(PayrollError::RewardOutsideFirstPeriod));
        }
    }

    mod context_급여_breakdown을_감사하는_경우 {
        use super::*;

        #[test]
        fn given_월400만원과_정확한_policy_when_계산하면_then_모든_보험세금net_합계가_일치한다() {
            let policy = given_policy();

            let result = when_calculate(
                given_period(1, 1, given_date(2026, Month::January, 1), 48_000_000, 25),
                &policy,
                0,
                None,
            )
            .expect("급여 breakdown을 계산해야 한다");

            assert_eq!(
                result.insurance.national_pension.employee_amount_krw,
                200_000
            );
            assert_eq!(
                result.insurance.national_pension.employer_amount_krw,
                200_000
            );
            assert_eq!(
                result.insurance.health_insurance.employee_amount_krw,
                160_000
            );
            assert_eq!(
                result.insurance.health_insurance.employer_amount_krw,
                160_000
            );
            assert_eq!(result.insurance.long_term_care.employee_amount_krw, 16_000);
            assert_eq!(result.insurance.long_term_care.employer_amount_krw, 16_000);
            assert_eq!(
                result.insurance.employment_insurance.employee_amount_krw,
                40_000
            );
            assert_eq!(
                result.insurance.employment_insurance.employer_amount_krw,
                80_000
            );
            assert_eq!(
                result.insurance.industrial_accident.employer_amount_krw,
                20_000
            );
            assert_eq!(result.employee_insurance_total_krw, 416_000);
            assert_eq!(result.employer_insurance_total_krw, 476_000);
            assert_eq!(result.withheld_income_tax_krw, 300_000);
            assert_eq!(result.withheld_local_income_tax_krw, 30_000);
            assert_eq!(result.net_salary_pay_krw, 3_254_000);
        }
    }

    mod context_입력과_policy가_유효하지_않은_경우 {
        use super::*;

        fn given_minimum_rate_policy() -> PayrollPolicy {
            let mut policy = given_policy();
            policy.national_pension.maximum_monthly_income_krw = i64::MAX;
            policy.national_pension.contribution.employee_rate_ppm = 1;
            policy.national_pension.contribution.employer_rate_ppm = 1;
            policy
                .national_pension
                .contribution
                .employee_rounding_unit_krw = 1;
            policy
                .national_pension
                .contribution
                .employer_rounding_unit_krw = 1;
            policy.health_insurance.contribution.employee_rate_ppm = 1;
            policy.health_insurance.contribution.employer_rate_ppm = 1;
            policy
                .health_insurance
                .contribution
                .employee_rounding_unit_krw = 1;
            policy
                .health_insurance
                .contribution
                .employer_rounding_unit_krw = 1;
            policy.long_term_care.health_premium_rate_numerator = 1;
            policy.long_term_care.health_premium_rate_denominator = 1_000_000;
            policy.long_term_care.employee_rounding_unit_krw = 1;
            policy.long_term_care.employer_rounding_unit_krw = 1;
            policy.employment_insurance.employee_rate_ppm = 1;
            policy.employment_insurance.employer_rates[0].rate_ppm = 1;
            policy.employment_insurance.employee_rounding_unit_krw = 1;
            policy.employment_insurance.employer_rounding_unit_krw = 1;
            policy.industrial_accident.employer_rates[0].rate_ppm = 1;
            policy.industrial_accident.employer_rounding_unit_krw = 1;
            policy.local_income_withholding.income_tax_rate_ppm = 1;
            policy.local_income_withholding.rounding_unit_krw = 1;
            policy.wanted_reward = Some(OtherIncomeRewardPolicy {
                income_tax_rate_ppm: 1,
                local_income_tax_rate_ppm: 1,
                income_tax_rounding_unit_krw: 1,
                local_income_tax_rounding_unit_krw: 1,
            });
            policy
        }

        #[test]
        fn given_급여와_보상합이_i64를_넘을때_when_계산하면_then_overflow로_거절한다() {
            let policy = given_minimum_rate_policy();

            let result = when_calculate(
                given_period(1, 1, given_date(2026, Month::January, 1), i64::MAX, 25),
                &policy,
                0,
                Some(i64::MAX),
            );

            assert_eq!(result, Err(PayrollError::ArithmeticOverflow));
        }

        #[test]
        fn given_0인_보험요율_when_계산하면_then_invalid_rate로_거절한다() {
            let mut policy = given_policy();
            policy.employment_insurance.employee_rate_ppm = 0;

            let result = when_calculate(
                given_period(1, 1, given_date(2026, Month::January, 1), 48_000_000, 25),
                &policy,
                0,
                None,
            );

            assert_eq!(result, Err(PayrollError::InvalidRate));
        }

        #[test]
        fn given_0인_절사단위_when_계산하면_then_invalid_rounding으로_거절한다() {
            let mut policy = given_policy();
            policy.long_term_care.employee_rounding_unit_krw = 0;

            let result = when_calculate(
                given_period(1, 1, given_date(2026, Month::January, 1), 48_000_000, 25),
                &policy,
                0,
                None,
            );

            assert_eq!(result, Err(PayrollError::InvalidRoundingUnit));
        }

        #[test]
        fn given_겹친_간이세액표_row_when_계산하면_then_overlap으로_거절한다() {
            let mut policy = given_policy();
            policy
                .employment_withholding_table
                .push(EmploymentWithholdingRow {
                    lower_bound_krw: 3_000_000,
                    upper_bound_exclusive_krw: Some(5_000_000),
                    family_count: 1,
                    child_count: 0,
                    income_tax_krw: 200_000,
                });

            let result = when_calculate(
                given_period(1, 1, given_date(2026, Month::January, 1), 48_000_000, 25),
                &policy,
                0,
                None,
            );

            assert_eq!(result, Err(PayrollError::OverlappingWithholdingRows));
        }
    }
}
