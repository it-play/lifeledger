use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use super::types::{
    BULLET_DSR_PRINCIPAL_YEARS, DsrAssessment, DsrAssessmentInput, DsrLoanContribution,
    DsrPaymentTreatment, LOAN_RATE_SCALE_BP, LOAN_RATIO_SCALE_PPM,
    LeaseDepositAffordabilityAssessment, LeaseDepositAffordabilityInput, LeaseDepositFundingLimit,
    LeaseDepositFundingLimitInput, LoanInstallmentCalculation, LoanInterestCalculation,
    LoanInterestInput, LoanPrepaymentCalculation, LoanPrepaymentEffect, LoanPrepaymentInput,
    LoanPrepaymentScheduleCalculation, LoanPrepaymentScheduleInput, LoanRateReset, LoanRateType,
    LoanRepaymentMethod, LoanRuleError, LoanRules, LoanScheduleCalculation, LoanScheduleInput,
    LoanSchedulePeriod, LtvAssessment, LtvAssessmentInput, RepaymentAllocation,
    RepaymentAllocationInput, RepaymentBucketAllocation,
};

struct V1LoanRules;

pub fn create_loan_rules() -> Arc<dyn LoanRules> {
    Arc::new(V1LoanRules)
}

impl LoanRules for V1LoanRules {
    fn calculate_interest(
        &self,
        input: LoanInterestInput,
    ) -> Result<LoanInterestCalculation, LoanRuleError> {
        calculate_interest(input)
    }

    fn build_schedule(
        &self,
        input: LoanScheduleInput<'_>,
    ) -> Result<LoanScheduleCalculation, LoanRuleError> {
        build_schedule(input)
    }

    fn calculate_prepayment(
        &self,
        input: LoanPrepaymentInput,
    ) -> Result<LoanPrepaymentCalculation, LoanRuleError> {
        calculate_prepayment(input)
    }

    fn rebuild_prepayment_schedule(
        &self,
        input: LoanPrepaymentScheduleInput<'_>,
    ) -> Result<LoanPrepaymentScheduleCalculation, LoanRuleError> {
        rebuild_prepayment_schedule(input)
    }

    fn allocate_repayment(
        &self,
        input: RepaymentAllocationInput<'_>,
    ) -> Result<RepaymentAllocation, LoanRuleError> {
        allocate_repayment(input)
    }

    fn assess_dsr(&self, input: DsrAssessmentInput<'_>) -> Result<DsrAssessment, LoanRuleError> {
        assess_dsr(input)
    }

    fn assess_ltv(&self, input: LtvAssessmentInput) -> Result<LtvAssessment, LoanRuleError> {
        assess_ltv(input)
    }

    fn calculate_lease_deposit_funding_limit(
        &self,
        input: LeaseDepositFundingLimitInput,
    ) -> Result<LeaseDepositFundingLimit, LoanRuleError> {
        calculate_lease_deposit_funding_limit(input)
    }

    fn assess_lease_deposit_affordability(
        &self,
        input: LeaseDepositAffordabilityInput<'_>,
    ) -> Result<LeaseDepositAffordabilityAssessment, LoanRuleError> {
        assess_lease_deposit_affordability(input)
    }
}

fn calculate_interest(input: LoanInterestInput) -> Result<LoanInterestCalculation, LoanRuleError> {
    validate_interest_input(input)?;

    let denominator = interest_denominator(input.day_count)?;
    let numerator = i128::from(input.principal_krw)
        .checked_mul(i128::from(input.annual_rate_bp))
        .and_then(|value| value.checked_mul(i128::from(input.elapsed_days)))
        .and_then(|value| value.checked_add(input.prior_remainder_numerator))
        .ok_or(LoanRuleError::ArithmeticOverflow)?;
    let quotient = numerator
        .checked_div(denominator)
        .ok_or(LoanRuleError::ArithmeticOverflow)?;
    let remainder = numerator
        .checked_sub(
            quotient
                .checked_mul(denominator)
                .ok_or(LoanRuleError::ArithmeticOverflow)?,
        )
        .ok_or(LoanRuleError::ArithmeticOverflow)?;
    let interest_krw = i64::try_from(quotient).map_err(|_| LoanRuleError::ArithmeticOverflow)?;
    if interest_krw < 0 {
        return Err(LoanRuleError::InvalidInterestRemainder);
    }

    Ok(LoanInterestCalculation {
        interest_krw,
        carried_remainder_numerator: if input.discard_remainder {
            0
        } else {
            remainder
        },
        discarded_remainder_numerator: if input.discard_remainder {
            remainder
        } else {
            0
        },
    })
}

fn build_schedule(input: LoanScheduleInput<'_>) -> Result<LoanScheduleCalculation, LoanRuleError> {
    validate_schedule_input(input)?;

    let rate_resets = input
        .rate_resets
        .iter()
        .map(|reset| (reset.after_installment_sequence, reset.next_annual_rate_bp))
        .collect::<BTreeMap<_, _>>();
    let installment_count =
        u16::try_from(input.periods.len()).map_err(|_| LoanRuleError::ArithmeticOverflow)?;
    let equal_principal_krw = input
        .principal_krw
        .checked_div(i64::from(installment_count))
        .ok_or(LoanRuleError::ArithmeticOverflow)?;
    let mut annual_rate_bp = input.initial_annual_rate_bp;
    let mut principal_krw = input.principal_krw;
    let mut remainder_numerator = input.prior_interest_remainder_numerator;
    let mut level_payment_krw = if input.repayment_method == LoanRepaymentMethod::LevelPayment {
        find_minimum_level_payment(
            principal_krw,
            annual_rate_bp,
            input.day_count,
            remainder_numerator,
            input.periods,
        )?
    } else {
        0
    };
    let mut installments = Vec::with_capacity(input.periods.len());
    let mut total_principal_krw = 0_i64;
    let mut total_interest_krw = 0_i64;

    for (index, period) in input.periods.iter().enumerate() {
        let sequence = u16::try_from(index + 1).map_err(|_| LoanRuleError::ArithmeticOverflow)?;
        let is_final = sequence == installment_count;
        let opening_principal_krw = principal_krw;
        let interest = calculate_interest(LoanInterestInput {
            principal_krw: opening_principal_krw,
            annual_rate_bp,
            elapsed_days: period.elapsed_days,
            day_count: input.day_count,
            prior_remainder_numerator: remainder_numerator,
            discard_remainder: is_final,
        })?;
        let principal_due_krw = match input.repayment_method {
            LoanRepaymentMethod::EqualPrincipal => {
                if is_final {
                    principal_krw
                } else {
                    equal_principal_krw.min(principal_krw)
                }
            }
            LoanRepaymentMethod::LevelPayment => {
                let available_for_principal = level_payment_krw
                    .checked_sub(interest.interest_krw)
                    .ok_or(LoanRuleError::ArithmeticOverflow)?;
                if available_for_principal < 0 {
                    return Err(LoanRuleError::ScheduleDoesNotAmortize);
                }
                available_for_principal.min(principal_krw)
            }
            LoanRepaymentMethod::Bullet => {
                if is_final {
                    principal_krw
                } else {
                    0
                }
            }
        };
        principal_krw = principal_krw
            .checked_sub(principal_due_krw)
            .ok_or(LoanRuleError::ArithmeticOverflow)?;
        let payment_krw = principal_due_krw
            .checked_add(interest.interest_krw)
            .ok_or(LoanRuleError::ArithmeticOverflow)?;
        total_principal_krw = total_principal_krw
            .checked_add(principal_due_krw)
            .ok_or(LoanRuleError::ArithmeticOverflow)?;
        total_interest_krw = total_interest_krw
            .checked_add(interest.interest_krw)
            .ok_or(LoanRuleError::ArithmeticOverflow)?;
        remainder_numerator = interest.carried_remainder_numerator;

        installments.push(LoanInstallmentCalculation {
            sequence,
            due_game_day: period.due_game_day,
            elapsed_days: period.elapsed_days,
            annual_rate_bp,
            opening_principal_krw,
            payment_krw,
            principal_krw: principal_due_krw,
            interest_krw: interest.interest_krw,
            remaining_principal_krw: principal_krw,
            carried_interest_remainder_numerator: interest.carried_remainder_numerator,
            discarded_interest_remainder_numerator: interest.discarded_remainder_numerator,
        });

        if let Some(next_rate_bp) = rate_resets.get(&sequence).copied() {
            annual_rate_bp = next_rate_bp;
            if input.repayment_method == LoanRepaymentMethod::LevelPayment && principal_krw > 0 {
                let remaining_periods = input
                    .periods
                    .get(index + 1..)
                    .ok_or(LoanRuleError::InvalidSchedulePeriod)?;
                level_payment_krw = find_minimum_level_payment(
                    principal_krw,
                    annual_rate_bp,
                    input.day_count,
                    remainder_numerator,
                    remaining_periods,
                )?;
            }
        }
    }

    if principal_krw != 0 || total_principal_krw != input.principal_krw {
        return Err(LoanRuleError::ScheduleDoesNotAmortize);
    }

    Ok(LoanScheduleCalculation {
        installments,
        total_principal_krw,
        total_interest_krw,
    })
}

fn calculate_prepayment(
    input: LoanPrepaymentInput,
) -> Result<LoanPrepaymentCalculation, LoanRuleError> {
    if input.remaining_principal_krw <= 0
        || input.principal_krw <= 0
        || input.principal_krw > input.remaining_principal_krw
        || i64::from(input.fee_ppm) > LOAN_RATIO_SCALE_PPM
    {
        return Err(LoanRuleError::InvalidPrepayment);
    }
    let fee_krw = i128::from(input.principal_krw)
        .checked_mul(i128::from(input.fee_ppm))
        .and_then(|value| value.checked_div(i128::from(LOAN_RATIO_SCALE_PPM)))
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(LoanRuleError::ArithmeticOverflow)?;
    let total_debited_krw = input
        .principal_krw
        .checked_add(fee_krw)
        .ok_or(LoanRuleError::ArithmeticOverflow)?;
    let remaining_principal_krw = input
        .remaining_principal_krw
        .checked_sub(input.principal_krw)
        .ok_or(LoanRuleError::ArithmeticOverflow)?;

    Ok(LoanPrepaymentCalculation {
        principal_krw: input.principal_krw,
        fee_krw,
        total_debited_krw,
        remaining_principal_krw,
    })
}

fn rebuild_prepayment_schedule(
    input: LoanPrepaymentScheduleInput<'_>,
) -> Result<LoanPrepaymentScheduleCalculation, LoanRuleError> {
    validate_prepayment_schedule_input(input)?;
    match input.prepayment_effect {
        LoanPrepaymentEffect::ReduceTerm => rebuild_reduce_term_schedule(input),
        LoanPrepaymentEffect::RecalculatePayment => rebuild_recalculated_payment_schedule(input),
    }
}

fn rebuild_recalculated_payment_schedule(
    input: LoanPrepaymentScheduleInput<'_>,
) -> Result<LoanPrepaymentScheduleCalculation, LoanRuleError> {
    let periods = input
        .periods
        .iter()
        .map(|period| LoanSchedulePeriod {
            due_game_day: period.due_game_day,
            elapsed_days: period.elapsed_days,
        })
        .collect::<Vec<_>>();
    let mut schedule = build_schedule(LoanScheduleInput {
        principal_krw: input.principal_after_prepayment_krw,
        initial_annual_rate_bp: input.annual_rate_bp,
        day_count: input.day_count,
        repayment_method: input.repayment_method,
        prior_interest_remainder_numerator: input.prior_interest_remainder_numerator,
        periods: &periods,
        rate_resets: &[],
    })?;
    if schedule
        .installments
        .iter()
        .any(|installment| installment.opening_principal_krw == 0)
    {
        return Err(LoanRuleError::InvalidPrepaymentSchedule);
    }
    for (calculated, stored) in schedule.installments.iter_mut().zip(input.periods) {
        calculated.sequence = stored.installment_no;
    }

    Ok(LoanPrepaymentScheduleCalculation {
        installments: schedule.installments,
        cancelled_installment_numbers: Vec::new(),
        total_principal_krw: schedule.total_principal_krw,
        total_interest_krw: schedule.total_interest_krw,
    })
}

fn rebuild_reduce_term_schedule(
    input: LoanPrepaymentScheduleInput<'_>,
) -> Result<LoanPrepaymentScheduleCalculation, LoanRuleError> {
    let mut unallocated_principal_krw = input.principal_after_prepayment_krw;
    let mut retained_count = 0_usize;
    for period in input.periods {
        retained_count = retained_count
            .checked_add(1)
            .ok_or(LoanRuleError::ArithmeticOverflow)?;
        unallocated_principal_krw = unallocated_principal_krw
            .checked_sub(
                period
                    .scheduled_principal_cap_krw
                    .min(unallocated_principal_krw),
            )
            .ok_or(LoanRuleError::ArithmeticOverflow)?;
        if unallocated_principal_krw == 0 {
            break;
        }
    }
    if unallocated_principal_krw != 0 {
        return Err(LoanRuleError::InvalidPrepaymentSchedule);
    }

    let mut principal_krw = input.principal_after_prepayment_krw;
    let mut remainder_numerator = input.prior_interest_remainder_numerator;
    let mut total_interest_krw = 0_i64;
    let mut installments = Vec::with_capacity(retained_count);
    for (index, period) in input.periods[..retained_count].iter().enumerate() {
        let is_final = index + 1 == retained_count;
        let opening_principal_krw = principal_krw;
        let interest = calculate_interest(LoanInterestInput {
            principal_krw: opening_principal_krw,
            annual_rate_bp: input.annual_rate_bp,
            elapsed_days: period.elapsed_days,
            day_count: input.day_count,
            prior_remainder_numerator: remainder_numerator,
            discard_remainder: is_final,
        })?;
        let principal_due_krw = period.scheduled_principal_cap_krw.min(principal_krw);
        principal_krw = principal_krw
            .checked_sub(principal_due_krw)
            .ok_or(LoanRuleError::ArithmeticOverflow)?;
        let payment_krw = principal_due_krw
            .checked_add(interest.interest_krw)
            .ok_or(LoanRuleError::ArithmeticOverflow)?;
        total_interest_krw = total_interest_krw
            .checked_add(interest.interest_krw)
            .ok_or(LoanRuleError::ArithmeticOverflow)?;
        remainder_numerator = interest.carried_remainder_numerator;
        installments.push(LoanInstallmentCalculation {
            sequence: period.installment_no,
            due_game_day: period.due_game_day,
            elapsed_days: period.elapsed_days,
            annual_rate_bp: input.annual_rate_bp,
            opening_principal_krw,
            payment_krw,
            principal_krw: principal_due_krw,
            interest_krw: interest.interest_krw,
            remaining_principal_krw: principal_krw,
            carried_interest_remainder_numerator: interest.carried_remainder_numerator,
            discarded_interest_remainder_numerator: interest.discarded_remainder_numerator,
        });
    }
    if principal_krw != 0 {
        return Err(LoanRuleError::InvalidPrepaymentSchedule);
    }

    Ok(LoanPrepaymentScheduleCalculation {
        installments,
        cancelled_installment_numbers: input.periods[retained_count..]
            .iter()
            .map(|period| period.installment_no)
            .collect(),
        total_principal_krw: input.principal_after_prepayment_krw,
        total_interest_krw,
    })
}

fn find_minimum_level_payment(
    principal_krw: i64,
    annual_rate_bp: i64,
    day_count: u16,
    prior_remainder_numerator: i128,
    periods: &[LoanSchedulePeriod],
) -> Result<i64, LoanRuleError> {
    if principal_krw == 0 {
        return Ok(0);
    }
    let first_period = periods
        .first()
        .copied()
        .ok_or(LoanRuleError::EmptySchedule)?;
    let first_interest = calculate_interest(LoanInterestInput {
        principal_krw,
        annual_rate_bp,
        elapsed_days: first_period.elapsed_days,
        day_count,
        prior_remainder_numerator,
        discard_remainder: periods.len() == 1,
    })?
    .interest_krw;
    let mut low = 0_i64;
    let mut high = principal_krw
        .checked_add(first_interest)
        .ok_or(LoanRuleError::ArithmeticOverflow)?;

    while low < high {
        let distance = high
            .checked_sub(low)
            .ok_or(LoanRuleError::ArithmeticOverflow)?;
        let middle = low
            .checked_add(distance / 2)
            .ok_or(LoanRuleError::ArithmeticOverflow)?;
        if level_payment_amortizes(
            principal_krw,
            annual_rate_bp,
            day_count,
            prior_remainder_numerator,
            periods,
            middle,
        )? {
            high = middle;
        } else {
            low = middle
                .checked_add(1)
                .ok_or(LoanRuleError::ArithmeticOverflow)?;
        }
    }

    if !level_payment_amortizes(
        principal_krw,
        annual_rate_bp,
        day_count,
        prior_remainder_numerator,
        periods,
        low,
    )? {
        return Err(LoanRuleError::ScheduleDoesNotAmortize);
    }
    Ok(low)
}

fn level_payment_amortizes(
    mut principal_krw: i64,
    annual_rate_bp: i64,
    day_count: u16,
    mut remainder_numerator: i128,
    periods: &[LoanSchedulePeriod],
    payment_krw: i64,
) -> Result<bool, LoanRuleError> {
    for (index, period) in periods.iter().enumerate() {
        let interest = calculate_interest(LoanInterestInput {
            principal_krw,
            annual_rate_bp,
            elapsed_days: period.elapsed_days,
            day_count,
            prior_remainder_numerator: remainder_numerator,
            discard_remainder: index + 1 == periods.len(),
        })?;
        if payment_krw < interest.interest_krw {
            return Ok(false);
        }
        let principal_payment = payment_krw
            .checked_sub(interest.interest_krw)
            .ok_or(LoanRuleError::ArithmeticOverflow)?
            .min(principal_krw);
        principal_krw = principal_krw
            .checked_sub(principal_payment)
            .ok_or(LoanRuleError::ArithmeticOverflow)?;
        remainder_numerator = interest.carried_remainder_numerator;
    }
    Ok(principal_krw == 0)
}

fn allocate_repayment(
    input: RepaymentAllocationInput<'_>,
) -> Result<RepaymentAllocation, LoanRuleError> {
    if input.wallet_cash_krw < 0 {
        return Err(LoanRuleError::InvalidWalletCash);
    }
    let mut seen = HashSet::with_capacity(input.buckets.len());
    for bucket in input.buckets {
        if bucket.due_krw < 0 {
            return Err(LoanRuleError::InvalidRepaymentBucket(bucket.kind));
        }
        if !seen.insert(bucket.kind) {
            return Err(LoanRuleError::DuplicateRepaymentBucket(bucket.kind));
        }
    }

    let mut ordered = input.buckets.to_vec();
    ordered.sort_by_key(|bucket| bucket.kind.order());
    let wallet_cash_before_krw = input.wallet_cash_krw;
    let mut wallet_cash_after_krw = wallet_cash_before_krw;
    let mut buckets = Vec::with_capacity(ordered.len());
    for bucket in ordered {
        let paid_krw = wallet_cash_after_krw.min(bucket.due_krw);
        wallet_cash_after_krw = wallet_cash_after_krw
            .checked_sub(paid_krw)
            .ok_or(LoanRuleError::ArithmeticOverflow)?;
        let unpaid_krw = bucket
            .due_krw
            .checked_sub(paid_krw)
            .ok_or(LoanRuleError::ArithmeticOverflow)?;
        buckets.push(RepaymentBucketAllocation {
            kind: bucket.kind,
            due_krw: bucket.due_krw,
            paid_krw,
            unpaid_krw,
        });
    }

    Ok(RepaymentAllocation {
        wallet_cash_before_krw,
        wallet_cash_after_krw,
        buckets,
    })
}

fn assess_dsr(input: DsrAssessmentInput<'_>) -> Result<DsrAssessment, LoanRuleError> {
    validate_dsr_input(input)?;

    let mut loan_ids = HashSet::with_capacity(input.loans.len());
    let mut general_loan_balance_krw = 0_i64;
    let mut credit_loan_balance_krw = 0_i64;
    for loan in input.loans {
        if loan.loan_id == 0 {
            return Err(LoanRuleError::InvalidDsrLoan(loan.loan_id));
        }
        if !loan_ids.insert(loan.loan_id) {
            return Err(LoanRuleError::DuplicateLoanId(loan.loan_id));
        }
        validate_schedule_input(loan.schedule)
            .map_err(|_| LoanRuleError::InvalidDsrLoan(loan.loan_id))?;
        if loan
            .schedule
            .periods
            .iter()
            .any(|period| period.due_game_day <= input.evaluation_game_day)
        {
            return Err(LoanRuleError::InvalidDsrLoan(loan.loan_id));
        }
        if loan.counts_toward_general_loan_balance {
            general_loan_balance_krw = general_loan_balance_krw
                .checked_add(loan.schedule.principal_krw)
                .ok_or(LoanRuleError::ArithmeticOverflow)?;
        }
        if loan.counts_toward_credit_stress_balance {
            credit_loan_balance_krw = credit_loan_balance_krw
                .checked_add(loan.schedule.principal_krw)
                .ok_or(LoanRuleError::ArithmeticOverflow)?;
        }
    }

    let gate_applied = general_loan_balance_krw > input.policy.general_loan_balance_gate_krw;
    let stress_gate_applied = credit_loan_balance_krw > input.policy.credit_balance_stress_gate_krw;
    let mut included_loans = input
        .loans
        .iter()
        .filter(|loan| loan.included_in_dsr)
        .collect::<Vec<_>>();
    included_loans.sort_by_key(|loan| loan.loan_id);
    let mut numerator_krw = 0_i64;
    let mut loan_contributions = Vec::with_capacity(included_loans.len());
    for loan in included_loans {
        let stress_rate_bp = if stress_gate_applied && loan.counts_toward_credit_stress_balance {
            calculate_stress_rate_bp(input.policy, loan.rate_type, loan.fixed_rate_period_months)?
        } else {
            0
        };
        let debt_service_krw = match loan.payment_treatment {
            DsrPaymentTreatment::Scheduled => scheduled_debt_service(
                loan.schedule,
                stress_rate_bp,
                input.evaluation_game_day,
                input.evaluation_end_game_day,
            )?,
            DsrPaymentTreatment::BulletCreditFiveYear => {
                if loan.schedule.repayment_method != LoanRepaymentMethod::Bullet
                    || !loan.counts_toward_credit_stress_balance
                {
                    return Err(LoanRuleError::InvalidDsrLoan(loan.loan_id));
                }
                bullet_credit_debt_service(loan.schedule, stress_rate_bp)?
            }
        };
        numerator_krw = numerator_krw
            .checked_add(debt_service_krw)
            .ok_or(LoanRuleError::ArithmeticOverflow)?;
        loan_contributions.push(DsrLoanContribution {
            loan_id: loan.loan_id,
            stress_rate_bp,
            debt_service_krw,
        });
    }

    if !gate_applied {
        return Ok(DsrAssessment {
            gate_applied,
            stress_gate_applied,
            general_loan_balance_krw,
            credit_loan_balance_krw,
            numerator_krw,
            denominator_krw: None,
            ratio_ppm: None,
            maximum_ratio_ppm: input.policy.maximum_ratio_ppm,
            passed: true,
            loan_contributions,
        });
    }

    let denominator_krw = input
        .verified_annual_income_krw
        .filter(|income| *income > 0)
        .ok_or(LoanRuleError::IncomeUnavailable)?;
    let ratio_ppm = floor_ratio_ppm(numerator_krw, denominator_krw)?;
    Ok(DsrAssessment {
        gate_applied,
        stress_gate_applied,
        general_loan_balance_krw,
        credit_loan_balance_krw,
        numerator_krw,
        denominator_krw: Some(denominator_krw),
        ratio_ppm: Some(ratio_ppm),
        maximum_ratio_ppm: input.policy.maximum_ratio_ppm,
        passed: ratio_ppm <= input.policy.maximum_ratio_ppm,
        loan_contributions,
    })
}

fn scheduled_debt_service(
    schedule: LoanScheduleInput<'_>,
    stress_rate_bp: i64,
    evaluation_game_day: u32,
    evaluation_end_game_day: u32,
) -> Result<i64, LoanRuleError> {
    let adjusted_initial_rate_bp = schedule
        .initial_annual_rate_bp
        .checked_add(stress_rate_bp)
        .ok_or(LoanRuleError::ArithmeticOverflow)?;
    let adjusted_resets = schedule
        .rate_resets
        .iter()
        .map(|reset| {
            Ok(LoanRateReset {
                after_installment_sequence: reset.after_installment_sequence,
                next_annual_rate_bp: reset
                    .next_annual_rate_bp
                    .checked_add(stress_rate_bp)
                    .ok_or(LoanRuleError::ArithmeticOverflow)?,
            })
        })
        .collect::<Result<Vec<_>, LoanRuleError>>()?;
    let calculation = build_schedule(LoanScheduleInput {
        principal_krw: schedule.principal_krw,
        initial_annual_rate_bp: adjusted_initial_rate_bp,
        day_count: schedule.day_count,
        repayment_method: schedule.repayment_method,
        prior_interest_remainder_numerator: schedule.prior_interest_remainder_numerator,
        periods: schedule.periods,
        rate_resets: &adjusted_resets,
    })?;

    calculation
        .installments
        .iter()
        .filter(|installment| {
            installment.due_game_day > evaluation_game_day
                && installment.due_game_day <= evaluation_end_game_day
        })
        .try_fold(0_i64, |total, installment| {
            total
                .checked_add(installment.payment_krw)
                .ok_or(LoanRuleError::ArithmeticOverflow)
        })
}

fn bullet_credit_debt_service(
    schedule: LoanScheduleInput<'_>,
    stress_rate_bp: i64,
) -> Result<i64, LoanRuleError> {
    let effective_rate_bp = schedule
        .initial_annual_rate_bp
        .checked_add(stress_rate_bp)
        .ok_or(LoanRuleError::ArithmeticOverflow)?;
    let annual_interest = i128::from(schedule.principal_krw)
        .checked_mul(i128::from(effective_rate_bp))
        .ok_or(LoanRuleError::ArithmeticOverflow)?
        .checked_div(i128::from(LOAN_RATE_SCALE_BP))
        .ok_or(LoanRuleError::ArithmeticOverflow)?;
    let annual_interest_krw =
        i64::try_from(annual_interest).map_err(|_| LoanRuleError::ArithmeticOverflow)?;
    let annualized_principal_krw = schedule
        .principal_krw
        .checked_div(BULLET_DSR_PRINCIPAL_YEARS)
        .ok_or(LoanRuleError::ArithmeticOverflow)?;
    annualized_principal_krw
        .checked_add(annual_interest_krw)
        .ok_or(LoanRuleError::ArithmeticOverflow)
}

fn calculate_stress_rate_bp(
    policy: super::types::DsrPolicy,
    rate_type: LoanRateType,
    fixed_rate_period_months: u16,
) -> Result<i64, LoanRuleError> {
    let multiplier_ppm = match rate_type {
        LoanRateType::Fixed if fixed_rate_period_months >= 60 => 0,
        LoanRateType::Fixed if fixed_rate_period_months >= 36 => {
            policy.medium_fixed_stress_multiplier_ppm
        }
        LoanRateType::Fixed | LoanRateType::Variable => LOAN_RATIO_SCALE_PPM,
    };
    let stress_rate_bp = i128::from(policy.base_stress_rate_bp)
        .checked_mul(i128::from(multiplier_ppm))
        .ok_or(LoanRuleError::ArithmeticOverflow)?
        .checked_div(i128::from(LOAN_RATIO_SCALE_PPM))
        .ok_or(LoanRuleError::ArithmeticOverflow)?;
    i64::try_from(stress_rate_bp).map_err(|_| LoanRuleError::ArithmeticOverflow)
}

fn assess_ltv(input: LtvAssessmentInput) -> Result<LtvAssessment, LoanRuleError> {
    if input.existing_senior_balance_krw < 0
        || input.new_principal_krw < 0
        || input.included_fees_krw < 0
        || !(0..=LOAN_RATIO_SCALE_PPM).contains(&input.maximum_ratio_ppm)
    {
        return Err(LoanRuleError::InvalidLtvInput);
    }
    let denominator_krw = input
        .recognized_collateral_value_krw
        .filter(|value| *value > 0)
        .ok_or(LoanRuleError::ValuationUnavailable)?;
    let numerator_krw = input
        .existing_senior_balance_krw
        .checked_add(input.new_principal_krw)
        .and_then(|value| value.checked_add(input.included_fees_krw))
        .ok_or(LoanRuleError::ArithmeticOverflow)?;
    let ratio_ppm = floor_ratio_ppm(numerator_krw, denominator_krw)?;
    Ok(LtvAssessment {
        numerator_krw,
        denominator_krw,
        ratio_ppm,
        maximum_ratio_ppm: input.maximum_ratio_ppm,
        passed: ratio_ppm <= input.maximum_ratio_ppm,
    })
}

fn calculate_lease_deposit_funding_limit(
    input: LeaseDepositFundingLimitInput,
) -> Result<LeaseDepositFundingLimit, LoanRuleError> {
    if input.deposit_krw <= 0
        || !(1..=LOAN_RATIO_SCALE_PPM).contains(&input.funding_limit_ppm)
        || input.product_maximum_principal_krw <= 0
    {
        return Err(LoanRuleError::InvalidLeaseDepositFundingLimit);
    }

    let deposit_based_limit_krw = i128::from(input.deposit_krw)
        .checked_mul(i128::from(input.funding_limit_ppm))
        .ok_or(LoanRuleError::ArithmeticOverflow)?
        .checked_div(i128::from(LOAN_RATIO_SCALE_PPM))
        .ok_or(LoanRuleError::ArithmeticOverflow)?;
    let deposit_based_limit_krw =
        i64::try_from(deposit_based_limit_krw).map_err(|_| LoanRuleError::ArithmeticOverflow)?;

    Ok(LeaseDepositFundingLimit {
        deposit_based_limit_krw,
        maximum_funding_krw: deposit_based_limit_krw.min(input.product_maximum_principal_krw),
    })
}

fn assess_lease_deposit_affordability(
    input: LeaseDepositAffordabilityInput<'_>,
) -> Result<LeaseDepositAffordabilityAssessment, LoanRuleError> {
    validate_lease_deposit_affordability_input(input)?;

    let mut loan_ids = HashSet::with_capacity(input.existing_loans.len() + 1);
    loan_ids.insert(input.new_loan.loan_id);
    let mut credit_stress_balance_krw = 0_i64;
    for loan in input.existing_loans {
        if loan.loan_id == 0 {
            return Err(LoanRuleError::InvalidDsrLoan(loan.loan_id));
        }
        if !loan_ids.insert(loan.loan_id) {
            return Err(LoanRuleError::DuplicateLoanId(loan.loan_id));
        }
        validate_affordability_schedule(loan.loan_id, loan.schedule, input.evaluation_game_day)?;
        if input.replaced_loan_id != Some(loan.loan_id) && loan.counts_toward_credit_stress_balance
        {
            credit_stress_balance_krw = credit_stress_balance_krw
                .checked_add(loan.schedule.principal_krw)
                .ok_or(LoanRuleError::ArithmeticOverflow)?;
        }
    }
    if let Some(replaced_loan_id) = input.replaced_loan_id
        && !input
            .existing_loans
            .iter()
            .any(|loan| loan.loan_id == replaced_loan_id)
    {
        return Err(LoanRuleError::ReplacementLoanNotFound(replaced_loan_id));
    }

    let stress_gate_applied =
        credit_stress_balance_krw > input.stress_policy.credit_balance_stress_gate_krw;
    let mut included_loans = input
        .existing_loans
        .iter()
        .filter(|loan| loan.included_in_dsr && input.replaced_loan_id != Some(loan.loan_id))
        .collect::<Vec<_>>();
    included_loans.sort_by_key(|loan| loan.loan_id);

    let mut numerator_krw = 0_i64;
    let mut existing_loan_contributions = Vec::with_capacity(included_loans.len());
    for loan in included_loans {
        let stress_rate_bp = if stress_gate_applied && loan.counts_toward_credit_stress_balance {
            calculate_stress_rate_bp(
                input.stress_policy,
                loan.rate_type,
                loan.fixed_rate_period_months,
            )?
        } else {
            0
        };
        let debt_service_krw = dsr_debt_service(
            loan,
            stress_rate_bp,
            input.evaluation_game_day,
            input.evaluation_end_game_day,
        )?;
        numerator_krw = numerator_krw
            .checked_add(debt_service_krw)
            .ok_or(LoanRuleError::ArithmeticOverflow)?;
        existing_loan_contributions.push(DsrLoanContribution {
            loan_id: loan.loan_id,
            stress_rate_bp,
            debt_service_krw,
        });
    }

    let new_loan_interest_krw = interest_only_debt_service(
        input.new_loan.schedule,
        input.evaluation_game_day,
        input.evaluation_end_game_day,
    )?;
    numerator_krw = numerator_krw
        .checked_add(new_loan_interest_krw)
        .ok_or(LoanRuleError::ArithmeticOverflow)?;
    let denominator_krw = input
        .verified_annual_income_krw
        .filter(|income| *income > 0)
        .ok_or(LoanRuleError::IncomeUnavailable)?;
    let ratio_ppm = floor_ratio_ppm(numerator_krw, denominator_krw)?;

    Ok(LeaseDepositAffordabilityAssessment {
        numerator_krw,
        denominator_krw,
        ratio_ppm,
        maximum_ratio_ppm: input.maximum_ratio_ppm,
        passed: ratio_ppm <= input.maximum_ratio_ppm,
        new_loan_interest_krw,
        existing_loan_contributions,
        replaced_loan_id: input.replaced_loan_id,
    })
}

fn dsr_debt_service(
    loan: &super::types::DsrLoanInput<'_>,
    stress_rate_bp: i64,
    evaluation_game_day: u32,
    evaluation_end_game_day: u32,
) -> Result<i64, LoanRuleError> {
    match loan.payment_treatment {
        DsrPaymentTreatment::Scheduled => scheduled_debt_service(
            loan.schedule,
            stress_rate_bp,
            evaluation_game_day,
            evaluation_end_game_day,
        ),
        DsrPaymentTreatment::BulletCreditFiveYear => {
            if loan.schedule.repayment_method != LoanRepaymentMethod::Bullet
                || !loan.counts_toward_credit_stress_balance
            {
                return Err(LoanRuleError::InvalidDsrLoan(loan.loan_id));
            }
            bullet_credit_debt_service(loan.schedule, stress_rate_bp)
        }
    }
}

fn interest_only_debt_service(
    schedule: LoanScheduleInput<'_>,
    evaluation_game_day: u32,
    evaluation_end_game_day: u32,
) -> Result<i64, LoanRuleError> {
    let calculation = build_schedule(schedule)?;
    calculation
        .installments
        .iter()
        .filter(|installment| {
            installment.due_game_day > evaluation_game_day
                && installment.due_game_day <= evaluation_end_game_day
        })
        .try_fold(0_i64, |total, installment| {
            total
                .checked_add(installment.interest_krw)
                .ok_or(LoanRuleError::ArithmeticOverflow)
        })
}

fn validate_affordability_schedule(
    loan_id: u64,
    schedule: LoanScheduleInput<'_>,
    evaluation_game_day: u32,
) -> Result<(), LoanRuleError> {
    validate_schedule_input(schedule).map_err(|_| LoanRuleError::InvalidDsrLoan(loan_id))?;
    if schedule
        .periods
        .iter()
        .any(|period| period.due_game_day <= evaluation_game_day)
    {
        return Err(LoanRuleError::InvalidDsrLoan(loan_id));
    }
    Ok(())
}

fn validate_lease_deposit_affordability_input(
    input: LeaseDepositAffordabilityInput<'_>,
) -> Result<(), LoanRuleError> {
    if input.evaluation_end_game_day <= input.evaluation_game_day
        || input.new_loan.loan_id == 0
        || input.new_loan.schedule.repayment_method != LoanRepaymentMethod::Bullet
        || !(1..=LOAN_RATIO_SCALE_PPM).contains(&input.maximum_ratio_ppm)
        || input.stress_policy.credit_balance_stress_gate_krw < 0
        || input.stress_policy.base_stress_rate_bp < 0
        || !(0..=LOAN_RATIO_SCALE_PPM)
            .contains(&input.stress_policy.medium_fixed_stress_multiplier_ppm)
    {
        return Err(LoanRuleError::InvalidLeaseDepositAffordability);
    }
    validate_affordability_schedule(
        input.new_loan.loan_id,
        input.new_loan.schedule,
        input.evaluation_game_day,
    )
    .map_err(|_| LoanRuleError::InvalidLeaseDepositAffordability)
}

fn floor_ratio_ppm(numerator_krw: i64, denominator_krw: i64) -> Result<i64, LoanRuleError> {
    let ratio = i128::from(numerator_krw)
        .checked_mul(i128::from(LOAN_RATIO_SCALE_PPM))
        .ok_or(LoanRuleError::ArithmeticOverflow)?
        .checked_div(i128::from(denominator_krw))
        .ok_or(LoanRuleError::ArithmeticOverflow)?;
    i64::try_from(ratio).map_err(|_| LoanRuleError::ArithmeticOverflow)
}

fn validate_interest_input(input: LoanInterestInput) -> Result<(), LoanRuleError> {
    if input.principal_krw < 0 {
        return Err(LoanRuleError::InvalidPrincipal);
    }
    if input.annual_rate_bp < 0 {
        return Err(LoanRuleError::InvalidAnnualRate);
    }
    if input.elapsed_days == 0 {
        return Err(LoanRuleError::InvalidElapsedDays);
    }
    let denominator = interest_denominator(input.day_count)?;
    if input.prior_remainder_numerator <= -denominator
        || input.prior_remainder_numerator >= denominator
    {
        return Err(LoanRuleError::InvalidInterestRemainder);
    }
    Ok(())
}

fn validate_schedule_input(input: LoanScheduleInput<'_>) -> Result<(), LoanRuleError> {
    if input.principal_krw <= 0 {
        return Err(LoanRuleError::InvalidPrincipal);
    }
    if input.initial_annual_rate_bp < 0 {
        return Err(LoanRuleError::InvalidAnnualRate);
    }
    if input.periods.is_empty() {
        return Err(LoanRuleError::EmptySchedule);
    }
    let period_count =
        u16::try_from(input.periods.len()).map_err(|_| LoanRuleError::ArithmeticOverflow)?;
    let denominator = interest_denominator(input.day_count)?;
    if input.prior_interest_remainder_numerator <= -denominator
        || input.prior_interest_remainder_numerator >= denominator
    {
        return Err(LoanRuleError::InvalidInterestRemainder);
    }
    let mut prior_due_game_day = None;
    for period in input.periods {
        if period.due_game_day == 0
            || period.elapsed_days == 0
            || prior_due_game_day.is_some_and(|prior| period.due_game_day <= prior)
        {
            return Err(LoanRuleError::InvalidSchedulePeriod);
        }
        prior_due_game_day = Some(period.due_game_day);
    }
    let mut reset_sequences = HashSet::with_capacity(input.rate_resets.len());
    for reset in input.rate_resets {
        if reset.after_installment_sequence == 0
            || reset.after_installment_sequence >= period_count
            || reset.next_annual_rate_bp < 0
        {
            return Err(LoanRuleError::InvalidRateReset(
                reset.after_installment_sequence,
            ));
        }
        if !reset_sequences.insert(reset.after_installment_sequence) {
            return Err(LoanRuleError::DuplicateRateReset(
                reset.after_installment_sequence,
            ));
        }
    }
    Ok(())
}

fn validate_prepayment_schedule_input(
    input: LoanPrepaymentScheduleInput<'_>,
) -> Result<(), LoanRuleError> {
    if input.principal_before_prepayment_krw <= 0
        || input.principal_after_prepayment_krw <= 0
        || input.principal_after_prepayment_krw >= input.principal_before_prepayment_krw
        || input.annual_rate_bp < 0
        || input.day_count == 0
        || input.periods.is_empty()
    {
        return Err(LoanRuleError::InvalidPrepaymentSchedule);
    }
    if input.prior_interest_remainder_numerator.unsigned_abs()
        >= interest_denominator(input.day_count)?.unsigned_abs()
    {
        return Err(LoanRuleError::InvalidInterestRemainder);
    }
    let mut prior_installment_no = 0_u16;
    let mut prior_due_game_day = 0_u32;
    let mut scheduled_principal_krw = 0_i64;
    for period in input.periods {
        if period.installment_no <= prior_installment_no
            || period.due_game_day <= prior_due_game_day
            || period.elapsed_days == 0
            || period.scheduled_principal_cap_krw < 0
        {
            return Err(LoanRuleError::InvalidPrepaymentSchedule);
        }
        scheduled_principal_krw = scheduled_principal_krw
            .checked_add(period.scheduled_principal_cap_krw)
            .ok_or(LoanRuleError::ArithmeticOverflow)?;
        prior_installment_no = period.installment_no;
        prior_due_game_day = period.due_game_day;
    }
    if scheduled_principal_krw != input.principal_before_prepayment_krw {
        return Err(LoanRuleError::InvalidPrepaymentSchedule);
    }
    Ok(())
}

fn validate_dsr_input(input: DsrAssessmentInput<'_>) -> Result<(), LoanRuleError> {
    if input.evaluation_end_game_day <= input.evaluation_game_day {
        return Err(LoanRuleError::InvalidEvaluationPeriod);
    }
    if input.policy.general_loan_balance_gate_krw < 0
        || input.policy.credit_balance_stress_gate_krw < 0
        || input.policy.base_stress_rate_bp < 0
        || !(0..=LOAN_RATIO_SCALE_PPM).contains(&input.policy.maximum_ratio_ppm)
        || !(0..=LOAN_RATIO_SCALE_PPM).contains(&input.policy.medium_fixed_stress_multiplier_ppm)
    {
        return Err(LoanRuleError::InvalidDsrPolicy);
    }
    Ok(())
}

fn interest_denominator(day_count: u16) -> Result<i128, LoanRuleError> {
    if day_count == 0 {
        return Err(LoanRuleError::InvalidDayCount);
    }
    i128::from(day_count)
        .checked_mul(i128::from(LOAN_RATE_SCALE_BP))
        .ok_or(LoanRuleError::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::life::{
        DsrLoanInput, DsrPolicy, LeaseDepositAffordabilityNewLoanInput,
        LoanPrepaymentSchedulePeriod, LoanRepaymentMethod, RepaymentBucketBalance,
        RepaymentBucketKind,
    };

    fn given_rules() -> Arc<dyn LoanRules> {
        create_loan_rules()
    }

    fn given_periods(count: u16, elapsed_days: u16) -> Vec<LoanSchedulePeriod> {
        (1..=count)
            .map(|sequence| LoanSchedulePeriod {
                due_game_day: u32::from(sequence) * u32::from(elapsed_days),
                elapsed_days,
            })
            .collect()
    }

    fn given_schedule<'a>(
        principal_krw: i64,
        annual_rate_bp: i64,
        repayment_method: LoanRepaymentMethod,
        periods: &'a [LoanSchedulePeriod],
        rate_resets: &'a [LoanRateReset],
    ) -> LoanScheduleInput<'a> {
        LoanScheduleInput {
            principal_krw,
            initial_annual_rate_bp: annual_rate_bp,
            day_count: 365,
            repayment_method,
            prior_interest_remainder_numerator: 0,
            periods,
            rate_resets,
        }
    }

    fn given_dsr_policy() -> DsrPolicy {
        DsrPolicy {
            general_loan_balance_gate_krw: 100_000_000,
            maximum_ratio_ppm: 400_000,
            credit_balance_stress_gate_krw: 100_000_000,
            base_stress_rate_bp: 150,
            medium_fixed_stress_multiplier_ppm: 600_000,
        }
    }

    mod context_actual_day_count_이자를_계산하는_경우 {
        use super::*;

        #[test]
        fn given_백만원과365bp와하루_when_actual365를_계산하면_then_이자는100원이다() {
            let input = LoanInterestInput {
                principal_krw: 1_000_000,
                annual_rate_bp: 365,
                elapsed_days: 1,
                day_count: 365,
                prior_remainder_numerator: 0,
                discard_remainder: false,
            };

            let result = given_rules()
                .calculate_interest(input)
                .expect("actual/365 이자를 계산해야 한다");

            assert_eq!(result.interest_krw, 100);
        }

        #[test]
        fn given_음수remainder가_당일분자와같을때_when_계산하면_then_0으로상쇄된다() {
            let input = LoanInterestInput {
                principal_krw: 1,
                annual_rate_bp: 1,
                elapsed_days: 1,
                day_count: 365,
                prior_remainder_numerator: -1,
                discard_remainder: false,
            };

            let result = given_rules()
                .calculate_interest(input)
                .expect("signed remainder를 계산해야 한다");

            assert_eq!(
                (result.interest_krw, result.carried_remainder_numerator),
                (0, 0)
            );
        }

        #[test]
        fn given_원미만분자_when_마지막이자를계산하면_then_숨은1원없이_remainder를폐기한다() {
            let input = LoanInterestInput {
                principal_krw: 1,
                annual_rate_bp: 1,
                elapsed_days: 1,
                day_count: 365,
                prior_remainder_numerator: 0,
                discard_remainder: true,
            };

            let result = given_rules()
                .calculate_interest(input)
                .expect("마지막 이자를 계산해야 한다");

            assert_eq!(
                (
                    result.interest_krw,
                    result.carried_remainder_numerator,
                    result.discarded_remainder_numerator,
                ),
                (0, 0, 1)
            );
        }

        #[test]
        fn given_i128범위를넘는곱_when_이자를계산하면_then_overflow로거절한다() {
            let input = LoanInterestInput {
                principal_krw: i64::MAX,
                annual_rate_bp: i64::MAX,
                elapsed_days: u16::MAX,
                day_count: 365,
                prior_remainder_numerator: 0,
                discard_remainder: false,
            };

            let result = given_rules().calculate_interest(input);

            assert_eq!(result, Err(LoanRuleError::ArithmeticOverflow));
        }
    }

    mod context_equal_principal_schedule을_만드는_경우 {
        use super::*;

        #[test]
        fn given_100원을3회상환_when_schedule을만들면_then_마지막회차가나머지를가진다() {
            let periods = given_periods(3, 30);
            let input = given_schedule(100, 0, LoanRepaymentMethod::EqualPrincipal, &periods, &[]);

            let result = given_rules()
                .build_schedule(input)
                .expect("원금균등 schedule을 계산해야 한다");

            assert_eq!(
                result
                    .installments
                    .iter()
                    .map(|item| item.principal_krw)
                    .collect::<Vec<_>>(),
                vec![33, 33, 34]
            );
        }

        #[test]
        fn given_첫회차뒤금리reset_when_schedule을만들면_then_다음이자구간부터새금리를쓴다() {
            let periods = given_periods(3, 10);
            let resets = [LoanRateReset {
                after_installment_sequence: 1,
                next_annual_rate_bp: 3_650,
            }];
            let input = given_schedule(
                900,
                0,
                LoanRepaymentMethod::EqualPrincipal,
                &periods,
                &resets,
            );

            let result = given_rules()
                .build_schedule(input)
                .expect("변동금리 원금균등 schedule을 계산해야 한다");

            assert_eq!(
                result
                    .installments
                    .iter()
                    .map(|item| (item.annual_rate_bp, item.interest_krw))
                    .collect::<Vec<_>>(),
                vec![(0, 0), (3_650, 6), (3_650, 3)]
            );
        }
    }

    mod context_level_payment_schedule을_만드는_경우 {
        use super::*;

        #[test]
        fn given_100원무이자3회_when_이분탐색하면_then_최소납입34원과마지막32원이다() {
            let periods = given_periods(3, 30);
            let input = given_schedule(100, 0, LoanRepaymentMethod::LevelPayment, &periods, &[]);

            let result = given_rules()
                .build_schedule(input)
                .expect("원리금균등 schedule을 계산해야 한다");

            assert_eq!(
                result
                    .installments
                    .iter()
                    .map(|item| item.payment_krw)
                    .collect::<Vec<_>>(),
                vec![34, 34, 32]
            );
        }

        #[test]
        fn given_1원무이자2회_when_이분탐색하면_then_최소원단위납입액은1원이다() {
            let periods = given_periods(2, 30);
            let input = given_schedule(1, 0, LoanRepaymentMethod::LevelPayment, &periods, &[]);

            let result = given_rules()
                .build_schedule(input)
                .expect("1원 schedule을 계산해야 한다");

            assert_eq!(
                result
                    .installments
                    .iter()
                    .map(|item| item.payment_krw)
                    .collect::<Vec<_>>(),
                vec![1, 0]
            );
        }

        #[test]
        fn given_첫회차뒤금리reset_when_상환액을재산정하면_then_다음회차부터406원으로고정한다() {
            let periods = given_periods(3, 10);
            let resets = [LoanRateReset {
                after_installment_sequence: 1,
                next_annual_rate_bp: 3_650,
            }];
            let input = given_schedule(
                1_200,
                0,
                LoanRepaymentMethod::LevelPayment,
                &periods,
                &resets,
            );

            let result = given_rules()
                .build_schedule(input)
                .expect("reset 뒤 상환액을 재산정해야 한다");

            assert_eq!(
                result
                    .installments
                    .iter()
                    .map(|item| (item.annual_rate_bp, item.payment_krw))
                    .collect::<Vec<_>>(),
                vec![(0, 400), (3_650, 406), (3_650, 406)]
            );
        }

        #[test]
        fn given_계산된최소납입액보다1원작을때_when_상환가능성을검사하면_then_만기잔액이남는다() {
            let periods = given_periods(3, 30);

            let result = level_payment_amortizes(100, 0, 365, 0, &periods, 33)
                .expect("최소액 아래 상환 가능성을 계산해야 한다");

            assert!(!result);
        }
    }

    mod context_bullet_schedule을_만드는_경우 {
        use super::*;

        #[test]
        fn given_1000원과10일당1퍼센트_when_3회schedule을만들면_then_만기에원금전부를낸다() {
            let periods = given_periods(3, 10);
            let input = given_schedule(1_000, 3_650, LoanRepaymentMethod::Bullet, &periods, &[]);

            let result = given_rules()
                .build_schedule(input)
                .expect("bullet schedule을 계산해야 한다");

            assert_eq!(
                result
                    .installments
                    .iter()
                    .map(|item| (item.principal_krw, item.interest_krw))
                    .collect::<Vec<_>>(),
                vec![(0, 10), (0, 10), (1_000, 10)]
            );
        }

        #[test]
        fn given_마지막원미만이자_when_bullet이끝나면_then_추가청구없이폐기한다() {
            let periods = given_periods(1, 1);
            let input = given_schedule(1, 1, LoanRepaymentMethod::Bullet, &periods, &[]);

            let result = given_rules()
                .build_schedule(input)
                .expect("마지막 remainder를 폐기해야 한다");

            assert_eq!(
                (
                    result.installments[0].payment_krw,
                    result.installments[0].discarded_interest_remainder_numerator,
                ),
                (1, 1)
            );
        }

        #[test]
        fn given_두회차원미만이자_when_bullet을계산하면_then_첫remainder를넘기고마지막에폐기한다() {
            let periods = given_periods(2, 1);
            let input = given_schedule(1, 1, LoanRepaymentMethod::Bullet, &periods, &[]);

            let result = given_rules()
                .build_schedule(input)
                .expect("회차 사이 remainder를 이어야 한다");

            assert_eq!(
                (
                    result.installments[0].carried_interest_remainder_numerator,
                    result.installments[1].discarded_interest_remainder_numerator,
                ),
                (1, 2)
            );
        }
    }

    mod context_대출원금을_중도상환하는_경우 {
        use super::*;

        fn given_prepayment_periods() -> Vec<LoanPrepaymentSchedulePeriod> {
            vec![
                LoanPrepaymentSchedulePeriod {
                    installment_no: 4,
                    due_game_day: 120,
                    elapsed_days: 30,
                    scheduled_principal_cap_krw: 30,
                },
                LoanPrepaymentSchedulePeriod {
                    installment_no: 5,
                    due_game_day: 150,
                    elapsed_days: 30,
                    scheduled_principal_cap_krw: 30,
                },
                LoanPrepaymentSchedulePeriod {
                    installment_no: 6,
                    due_game_day: 180,
                    elapsed_days: 30,
                    scheduled_principal_cap_krw: 40,
                },
            ]
        }

        #[test]
        fn given_33333원과15000ppm_when_수수료를계산하면_then_499원내림하고합계를출금한다() {
            let input = LoanPrepaymentInput {
                remaining_principal_krw: 100_000,
                principal_krw: 33_333,
                fee_ppm: 15_000,
            };

            let result = given_rules()
                .calculate_prepayment(input)
                .expect("중도상환 수수료를 원 단위로 내려야 한다");

            assert_eq!(
                (
                    result.fee_krw,
                    result.total_debited_krw,
                    result.remaining_principal_krw,
                ),
                (499, 33_832, 66_667)
            );
        }

        #[test]
        fn given_잔액보다큰원금_when_중도상환을계산하면_then_invalid로거절한다() {
            let input = LoanPrepaymentInput {
                remaining_principal_krw: 100,
                principal_krw: 101,
                fee_ppm: 0,
            };

            let result = given_rules().calculate_prepayment(input);

            assert_eq!(result, Err(LoanRuleError::InvalidPrepayment));
        }

        #[test]
        fn given_수수료0과잔액전액_when_중도상환을계산하면_then_원금만출금하고잔액은0이다() {
            let input = LoanPrepaymentInput {
                remaining_principal_krw: 100,
                principal_krw: 100,
                fee_ppm: 0,
            };

            let result = given_rules()
                .calculate_prepayment(input)
                .expect("수수료 없는 전액 중도상환을 계산해야 한다");

            assert_eq!(
                (
                    result.fee_krw,
                    result.total_debited_krw,
                    result.remaining_principal_krw,
                ),
                (0, 100, 0)
            );
        }

        #[test]
        fn given_recalculate_payment와40원상환_when_재작성하면_then_due회차를유지하고새잔액을분배한다()
         {
            let periods = given_prepayment_periods();
            let input = LoanPrepaymentScheduleInput {
                principal_before_prepayment_krw: 100,
                principal_after_prepayment_krw: 60,
                annual_rate_bp: 0,
                day_count: 365,
                repayment_method: LoanRepaymentMethod::LevelPayment,
                prepayment_effect: LoanPrepaymentEffect::RecalculatePayment,
                prior_interest_remainder_numerator: 0,
                periods: &periods,
            };

            let result = given_rules()
                .rebuild_prepayment_schedule(input)
                .expect("남은 모든 회차를 새 잔액으로 재산정해야 한다");

            assert_eq!(
                result
                    .installments
                    .iter()
                    .map(|item| (item.sequence, item.due_game_day, item.principal_krw))
                    .collect::<Vec<_>>(),
                vec![(4, 120, 20), (5, 150, 20), (6, 180, 20)]
            );
            assert!(result.cancelled_installment_numbers.is_empty());
        }

        #[test]
        fn given_reduce_term과50원잔액_when_재작성하면_then_앞상한을유지하고뒤suffix를취소한다() {
            let periods = given_prepayment_periods();
            let input = LoanPrepaymentScheduleInput {
                principal_before_prepayment_krw: 100,
                principal_after_prepayment_krw: 50,
                annual_rate_bp: 0,
                day_count: 365,
                repayment_method: LoanRepaymentMethod::EqualPrincipal,
                prepayment_effect: LoanPrepaymentEffect::ReduceTerm,
                prior_interest_remainder_numerator: 0,
                periods: &periods,
            };

            let result = given_rules()
                .rebuild_prepayment_schedule(input)
                .expect("앞 회차 상한으로 새 잔액을 배분해야 한다");

            assert_eq!(
                result
                    .installments
                    .iter()
                    .map(|item| (item.sequence, item.principal_krw))
                    .collect::<Vec<_>>(),
                vec![(4, 30), (5, 20)]
            );
            assert_eq!(result.cancelled_installment_numbers, vec![6]);
        }

        #[test]
        fn given_level_payment의_reduce_term_when_재작성하면_then_기존원금cap만따른다() {
            let periods = given_prepayment_periods();
            let input = LoanPrepaymentScheduleInput {
                principal_before_prepayment_krw: 100,
                principal_after_prepayment_krw: 50,
                annual_rate_bp: 0,
                day_count: 365,
                repayment_method: LoanRepaymentMethod::LevelPayment,
                prepayment_effect: LoanPrepaymentEffect::ReduceTerm,
                prior_interest_remainder_numerator: 0,
                periods: &periods,
            };

            let result = given_rules()
                .rebuild_prepayment_schedule(input)
                .expect("상환 방식과 무관하게 저장된 원금 cap을 따라야 한다");

            assert_eq!(result.cancelled_installment_numbers, vec![6]);
        }

        #[test]
        fn given_pending원금합이계약잔액과다를때_when_재작성하면_then_invalid로거절한다() {
            let mut periods = given_prepayment_periods();
            periods[2].scheduled_principal_cap_krw = 39;
            let input = LoanPrepaymentScheduleInput {
                principal_before_prepayment_krw: 100,
                principal_after_prepayment_krw: 50,
                annual_rate_bp: 0,
                day_count: 365,
                repayment_method: LoanRepaymentMethod::EqualPrincipal,
                prepayment_effect: LoanPrepaymentEffect::ReduceTerm,
                prior_interest_remainder_numerator: 0,
                periods: &periods,
            };

            let result = given_rules().rebuild_prepayment_schedule(input);

            assert_eq!(result, Err(LoanRuleError::InvalidPrepaymentSchedule));
        }

        #[test]
        fn given_새잔액이남은회차보다작을때_when_recalculate_payment하면_then_invalid로거절한다() {
            let periods = [
                LoanPrepaymentSchedulePeriod {
                    installment_no: 4,
                    due_game_day: 120,
                    elapsed_days: 30,
                    scheduled_principal_cap_krw: 1,
                },
                LoanPrepaymentSchedulePeriod {
                    installment_no: 5,
                    due_game_day: 150,
                    elapsed_days: 30,
                    scheduled_principal_cap_krw: 1,
                },
            ];
            let input = LoanPrepaymentScheduleInput {
                principal_before_prepayment_krw: 2,
                principal_after_prepayment_krw: 1,
                annual_rate_bp: 0,
                day_count: 365,
                repayment_method: LoanRepaymentMethod::LevelPayment,
                prepayment_effect: LoanPrepaymentEffect::RecalculatePayment,
                prior_interest_remainder_numerator: 0,
                periods: &periods,
            };

            let result = given_rules().rebuild_prepayment_schedule(input);

            assert_eq!(result, Err(LoanRuleError::InvalidPrepaymentSchedule));
        }
    }

    mod context_repayment_bucket을_배분하는_경우 {
        use super::*;

        #[test]
        fn given_뒤섞인6개bucket과250원_when_배분하면_then_연체비용이자원금순으로쓴다() {
            let buckets = [
                RepaymentBucketBalance {
                    kind: RepaymentBucketKind::CurrentPrincipal,
                    due_krw: 100,
                },
                RepaymentBucketBalance {
                    kind: RepaymentBucketKind::OverduePrincipal,
                    due_krw: 100,
                },
                RepaymentBucketBalance {
                    kind: RepaymentBucketKind::CurrentInterest,
                    due_krw: 100,
                },
                RepaymentBucketBalance {
                    kind: RepaymentBucketKind::OverdueFee,
                    due_krw: 100,
                },
                RepaymentBucketBalance {
                    kind: RepaymentBucketKind::CurrentFee,
                    due_krw: 100,
                },
                RepaymentBucketBalance {
                    kind: RepaymentBucketKind::OverdueInterest,
                    due_krw: 100,
                },
            ];

            let result = given_rules()
                .allocate_repayment(RepaymentAllocationInput {
                    wallet_cash_krw: 250,
                    buckets: &buckets,
                })
                .expect("bucket을 순서대로 배분해야 한다");

            assert_eq!(
                result
                    .buckets
                    .iter()
                    .map(|bucket| (bucket.kind, bucket.paid_krw))
                    .collect::<Vec<_>>(),
                vec![
                    (RepaymentBucketKind::OverdueFee, 100),
                    (RepaymentBucketKind::OverdueInterest, 100),
                    (RepaymentBucketKind::OverduePrincipal, 50),
                    (RepaymentBucketKind::CurrentFee, 0),
                    (RepaymentBucketKind::CurrentInterest, 0),
                    (RepaymentBucketKind::CurrentPrincipal, 0),
                ]
            );
        }

        #[test]
        fn given_지갑보다큰bucket_when_배분하면_then_지갑은0아래로내려가지않는다() {
            let buckets = [RepaymentBucketBalance {
                kind: RepaymentBucketKind::CurrentPrincipal,
                due_krw: 100,
            }];

            let result = given_rules()
                .allocate_repayment(RepaymentAllocationInput {
                    wallet_cash_krw: 1,
                    buckets: &buckets,
                })
                .expect("지갑 범위 안에서 배분해야 한다");

            assert_eq!(result.wallet_cash_after_krw, 0);
        }

        #[test]
        fn given_음수지갑_when_배분하면_then_invalid로거절한다() {
            let result = given_rules().allocate_repayment(RepaymentAllocationInput {
                wallet_cash_krw: -1,
                buckets: &[],
            });

            assert_eq!(result, Err(LoanRuleError::InvalidWalletCash));
        }
    }

    mod context_dsr을_심사하는_경우 {
        use super::*;

        #[test]
        fn given_일반대출잔액이gate와같고소득이없을때_when_심사하면_then_gate를적용하지않는다() {
            let periods = given_periods(12, 30);
            let schedule = given_schedule(
                100_000_000,
                0,
                LoanRepaymentMethod::EqualPrincipal,
                &periods,
                &[],
            );
            let loans = [DsrLoanInput {
                loan_id: 1,
                included_in_dsr: true,
                counts_toward_general_loan_balance: true,
                counts_toward_credit_stress_balance: false,
                rate_type: LoanRateType::Fixed,
                fixed_rate_period_months: 120,
                payment_treatment: DsrPaymentTreatment::Scheduled,
                schedule,
            }];

            let result = given_rules()
                .assess_dsr(DsrAssessmentInput {
                    evaluation_game_day: 0,
                    evaluation_end_game_day: 365,
                    verified_annual_income_krw: None,
                    policy: given_dsr_policy(),
                    loans: &loans,
                })
                .expect("gate 이하에서는 소득 없이 심사해야 한다");

            assert_eq!((result.gate_applied, result.ratio_ppm), (false, None));
        }

        #[test]
        fn given_일반대출잔액이gate보다1원많고소득이없을때_when_심사하면_then_income_unavailable이다()
         {
            let periods = given_periods(12, 30);
            let schedule = given_schedule(
                100_000_001,
                0,
                LoanRepaymentMethod::EqualPrincipal,
                &periods,
                &[],
            );
            let loans = [DsrLoanInput {
                loan_id: 1,
                included_in_dsr: true,
                counts_toward_general_loan_balance: true,
                counts_toward_credit_stress_balance: false,
                rate_type: LoanRateType::Fixed,
                fixed_rate_period_months: 120,
                payment_treatment: DsrPaymentTreatment::Scheduled,
                schedule,
            }];

            let result = given_rules().assess_dsr(DsrAssessmentInput {
                evaluation_game_day: 0,
                evaluation_end_game_day: 365,
                verified_annual_income_krw: None,
                policy: given_dsr_policy(),
                loans: &loans,
            });

            assert_eq!(result, Err(LoanRuleError::IncomeUnavailable));
        }

        #[test]
        fn given_24회분할상환_when_다음12개월을심사하면_then_기간안의실제schedule만합산한다() {
            let periods = given_periods(24, 30);
            let schedule =
                given_schedule(120, 0, LoanRepaymentMethod::EqualPrincipal, &periods, &[]);
            let loans = [DsrLoanInput {
                loan_id: 1,
                included_in_dsr: true,
                counts_toward_general_loan_balance: true,
                counts_toward_credit_stress_balance: false,
                rate_type: LoanRateType::Fixed,
                fixed_rate_period_months: 120,
                payment_treatment: DsrPaymentTreatment::Scheduled,
                schedule,
            }];
            let mut policy = given_dsr_policy();
            policy.general_loan_balance_gate_krw = 0;

            let result = given_rules()
                .assess_dsr(DsrAssessmentInput {
                    evaluation_game_day: 0,
                    evaluation_end_game_day: 360,
                    verified_annual_income_krw: Some(100),
                    policy,
                    loans: &loans,
                })
                .expect("다음 12개월 schedule을 합산해야 한다");

            assert_eq!(result.numerator_krw, 60);
        }

        #[test]
        fn given_360개월전기간고정주담대_when_다음12개월을심사하면_then_stress0으로_schedule전액을합산한다()
         {
            let periods = given_periods(360, 30);
            let schedule = given_schedule(360, 0, LoanRepaymentMethod::LevelPayment, &periods, &[]);
            let loans = [DsrLoanInput {
                loan_id: 1,
                included_in_dsr: true,
                counts_toward_general_loan_balance: true,
                counts_toward_credit_stress_balance: false,
                rate_type: LoanRateType::Fixed,
                fixed_rate_period_months: 360,
                payment_treatment: DsrPaymentTreatment::Scheduled,
                schedule,
            }];
            let mut policy = given_dsr_policy();
            policy.general_loan_balance_gate_krw = 0;
            policy.credit_balance_stress_gate_krw = 0;

            let result = given_rules()
                .assess_dsr(DsrAssessmentInput {
                    evaluation_game_day: 0,
                    evaluation_end_game_day: 360,
                    verified_annual_income_krw: Some(1_000),
                    policy,
                    loans: &loans,
                })
                .expect("전 기간 고정 주담대의 실제 schedule을 합산해야 한다");

            let contribution = result
                .loan_contributions
                .first()
                .expect("주담대 DSR 기여분이 있어야 한다");
            assert_eq!(
                (contribution.stress_rate_bp, contribution.debt_service_krw),
                (0, 12)
            );
        }

        #[test]
        fn given_policy에서제외한schedule_when_dsr을심사하면_then_잔액gate에는쓰고분자에서는뺀다() {
            let periods = given_periods(1, 30);
            let schedule = given_schedule(
                100_000_001,
                0,
                LoanRepaymentMethod::EqualPrincipal,
                &periods,
                &[],
            );
            let loans = [DsrLoanInput {
                loan_id: 1,
                included_in_dsr: false,
                counts_toward_general_loan_balance: true,
                counts_toward_credit_stress_balance: false,
                rate_type: LoanRateType::Fixed,
                fixed_rate_period_months: 120,
                payment_treatment: DsrPaymentTreatment::Scheduled,
                schedule,
            }];

            let result = given_rules()
                .assess_dsr(DsrAssessmentInput {
                    evaluation_game_day: 0,
                    evaluation_end_game_day: 30,
                    verified_annual_income_krw: Some(1),
                    policy: given_dsr_policy(),
                    loans: &loans,
                })
                .expect("policy source 제외를 적용해야 한다");

            assert_eq!((result.gate_applied, result.numerator_krw), (true, 0));
        }

        #[test]
        fn given_bullet신용대출_when_심사하면_then_원금5년환산액과연이자를쓴다() {
            let periods = given_periods(1, 365);
            let schedule =
                given_schedule(100_000_000, 500, LoanRepaymentMethod::Bullet, &periods, &[]);
            let loans = [DsrLoanInput {
                loan_id: 1,
                included_in_dsr: true,
                counts_toward_general_loan_balance: true,
                counts_toward_credit_stress_balance: true,
                rate_type: LoanRateType::Fixed,
                fixed_rate_period_months: 60,
                payment_treatment: DsrPaymentTreatment::BulletCreditFiveYear,
                schedule,
            }];
            let mut policy = given_dsr_policy();
            policy.general_loan_balance_gate_krw = 0;

            let result = given_rules()
                .assess_dsr(DsrAssessmentInput {
                    evaluation_game_day: 0,
                    evaluation_end_game_day: 365,
                    verified_annual_income_krw: Some(100_000_000),
                    policy,
                    loans: &loans,
                })
                .expect("bullet 5년 환산액을 계산해야 한다");

            assert_eq!(result.numerator_krw, 25_000_000);
        }

        #[test]
        fn given_1원분자와3원소득_when_ppm을계산하면_then_333333으로내린다() {
            let periods = given_periods(1, 30);
            let schedule = given_schedule(1, 0, LoanRepaymentMethod::EqualPrincipal, &periods, &[]);
            let loans = [DsrLoanInput {
                loan_id: 1,
                included_in_dsr: true,
                counts_toward_general_loan_balance: true,
                counts_toward_credit_stress_balance: false,
                rate_type: LoanRateType::Fixed,
                fixed_rate_period_months: 120,
                payment_treatment: DsrPaymentTreatment::Scheduled,
                schedule,
            }];
            let mut policy = given_dsr_policy();
            policy.general_loan_balance_gate_krw = 0;

            let result = given_rules()
                .assess_dsr(DsrAssessmentInput {
                    evaluation_game_day: 0,
                    evaluation_end_game_day: 30,
                    verified_annual_income_krw: Some(3),
                    policy,
                    loans: &loans,
                })
                .expect("DSR ppm을 계산해야 한다");

            assert_eq!(result.ratio_ppm, Some(333_333));
        }

        #[test]
        fn given_신용잔액이gate를초과한세금리유형_when_stress를계산하면_then_0_90_150bp를쓴다() {
            let periods = given_periods(1, 365);
            let schedule_a =
                given_schedule(40_000_000, 500, LoanRepaymentMethod::Bullet, &periods, &[]);
            let schedule_b = schedule_a;
            let schedule_c = schedule_a;
            let loans = [
                DsrLoanInput {
                    loan_id: 3,
                    included_in_dsr: true,
                    counts_toward_general_loan_balance: true,
                    counts_toward_credit_stress_balance: true,
                    rate_type: LoanRateType::Variable,
                    fixed_rate_period_months: 0,
                    payment_treatment: DsrPaymentTreatment::BulletCreditFiveYear,
                    schedule: schedule_c,
                },
                DsrLoanInput {
                    loan_id: 1,
                    included_in_dsr: true,
                    counts_toward_general_loan_balance: true,
                    counts_toward_credit_stress_balance: true,
                    rate_type: LoanRateType::Fixed,
                    fixed_rate_period_months: 60,
                    payment_treatment: DsrPaymentTreatment::BulletCreditFiveYear,
                    schedule: schedule_a,
                },
                DsrLoanInput {
                    loan_id: 2,
                    included_in_dsr: true,
                    counts_toward_general_loan_balance: true,
                    counts_toward_credit_stress_balance: true,
                    rate_type: LoanRateType::Fixed,
                    fixed_rate_period_months: 48,
                    payment_treatment: DsrPaymentTreatment::BulletCreditFiveYear,
                    schedule: schedule_b,
                },
            ];

            let result = given_rules()
                .assess_dsr(DsrAssessmentInput {
                    evaluation_game_day: 0,
                    evaluation_end_game_day: 365,
                    verified_annual_income_krw: Some(1_000_000_000),
                    policy: given_dsr_policy(),
                    loans: &loans,
                })
                .expect("stress profile을 적용해야 한다");

            assert_eq!(
                result
                    .loan_contributions
                    .iter()
                    .map(|item| (item.loan_id, item.stress_rate_bp))
                    .collect::<Vec<_>>(),
                vec![(1, 0), (2, 90), (3, 150)]
            );
        }

        #[test]
        fn given_신용잔액이stress_gate와같을때_when_심사하면_then_stress를적용하지않는다() {
            let periods = given_periods(1, 365);
            let schedule =
                given_schedule(100_000_000, 500, LoanRepaymentMethod::Bullet, &periods, &[]);
            let loans = [DsrLoanInput {
                loan_id: 1,
                included_in_dsr: true,
                counts_toward_general_loan_balance: true,
                counts_toward_credit_stress_balance: true,
                rate_type: LoanRateType::Variable,
                fixed_rate_period_months: 0,
                payment_treatment: DsrPaymentTreatment::BulletCreditFiveYear,
                schedule,
            }];

            let result = given_rules()
                .assess_dsr(DsrAssessmentInput {
                    evaluation_game_day: 0,
                    evaluation_end_game_day: 365,
                    verified_annual_income_krw: None,
                    policy: given_dsr_policy(),
                    loans: &loans,
                })
                .expect("stress gate 경계를 심사해야 한다");

            assert!(!result.stress_gate_applied);
        }
    }

    mod context_전세대출_보증금한도를_계산하는_경우 {
        use super::*;

        #[test]
        fn given_101원보증금과_80퍼센트_when_계산하면_then_80원으로내린다() {
            let input = LeaseDepositFundingLimitInput {
                deposit_krw: 101,
                funding_limit_ppm: 800_000,
                product_maximum_principal_krw: 400_000_000,
            };

            let limit = given_rules()
                .calculate_lease_deposit_funding_limit(input)
                .expect("보증금 비율 한도를 계산해야 한다");

            assert_eq!(
                (limit.deposit_based_limit_krw, limit.maximum_funding_krw),
                (80, 80)
            );
        }

        #[test]
        fn given_10억원보증금과_4억원상품상한_when_계산하면_then_4억원으로제한한다() {
            let input = LeaseDepositFundingLimitInput {
                deposit_krw: 1_000_000_000,
                funding_limit_ppm: 800_000,
                product_maximum_principal_krw: 400_000_000,
            };

            let limit = given_rules()
                .calculate_lease_deposit_funding_limit(input)
                .expect("상품 원금 상한을 적용해야 한다");

            assert_eq!(
                (limit.deposit_based_limit_krw, limit.maximum_funding_krw),
                (800_000_000, 400_000_000)
            );
        }
    }

    mod context_전세대출_상환여력을_심사하는_경우 {
        use super::*;

        #[test]
        fn given_24개월bullet신규대출_when_심사하면_then_다음12개월이자만합산한다() {
            let periods = given_periods(24, 30);
            let new_schedule =
                given_schedule(120_000_000, 400, LoanRepaymentMethod::Bullet, &periods, &[]);
            let input = LeaseDepositAffordabilityInput {
                evaluation_game_day: 0,
                evaluation_end_game_day: 360,
                verified_annual_income_krw: Some(100_000_000),
                maximum_ratio_ppm: 400_000,
                stress_policy: given_dsr_policy(),
                existing_loans: &[],
                new_loan: LeaseDepositAffordabilityNewLoanInput {
                    loan_id: 2,
                    schedule: new_schedule,
                },
                replaced_loan_id: None,
            };

            let assessment = given_rules()
                .assess_lease_deposit_affordability(input)
                .expect("신규 전세대출 이자만 심사해야 한다");

            assert_eq!(
                (
                    assessment.new_loan_interest_krw,
                    assessment.numerator_krw,
                    assessment.ratio_ppm,
                ),
                (4_734_246, 4_734_246, 47_342)
            );
        }

        #[test]
        fn given_대체상환할기존전세대출_when_심사하면_then_기존분자에서제외한다() {
            let existing_periods = given_periods(1, 365);
            let existing_schedule = given_schedule(
                100_000_000,
                500,
                LoanRepaymentMethod::Bullet,
                &existing_periods,
                &[],
            );
            let existing_loans = [DsrLoanInput {
                loan_id: 1,
                included_in_dsr: true,
                counts_toward_general_loan_balance: true,
                counts_toward_credit_stress_balance: true,
                rate_type: LoanRateType::Fixed,
                fixed_rate_period_months: 60,
                payment_treatment: DsrPaymentTreatment::BulletCreditFiveYear,
                schedule: existing_schedule,
            }];
            let new_periods = given_periods(1, 365);
            let new_schedule =
                given_schedule(10_000, 365, LoanRepaymentMethod::Bullet, &new_periods, &[]);
            let input = LeaseDepositAffordabilityInput {
                evaluation_game_day: 0,
                evaluation_end_game_day: 365,
                verified_annual_income_krw: Some(1_000),
                maximum_ratio_ppm: 400_000,
                stress_policy: given_dsr_policy(),
                existing_loans: &existing_loans,
                new_loan: LeaseDepositAffordabilityNewLoanInput {
                    loan_id: 2,
                    schedule: new_schedule,
                },
                replaced_loan_id: Some(1),
            };

            let assessment = given_rules()
                .assess_lease_deposit_affordability(input)
                .expect("대체 대출을 제외해 심사해야 한다");

            assert_eq!(assessment.numerator_krw, 365);
            assert!(assessment.existing_loan_contributions.is_empty());
            assert_eq!(assessment.replaced_loan_id, Some(1));
        }

        #[test]
        fn given_i64범위를넘는기존상환액_when_심사하면_then_overflow로거절한다() {
            let periods = given_periods(1, 365);
            let schedule = given_schedule(
                i64::MAX,
                0,
                LoanRepaymentMethod::EqualPrincipal,
                &periods,
                &[],
            );
            let existing_loans = [
                DsrLoanInput {
                    loan_id: 1,
                    included_in_dsr: true,
                    counts_toward_general_loan_balance: true,
                    counts_toward_credit_stress_balance: false,
                    rate_type: LoanRateType::Fixed,
                    fixed_rate_period_months: 120,
                    payment_treatment: DsrPaymentTreatment::Scheduled,
                    schedule,
                },
                DsrLoanInput {
                    loan_id: 2,
                    included_in_dsr: true,
                    counts_toward_general_loan_balance: true,
                    counts_toward_credit_stress_balance: false,
                    rate_type: LoanRateType::Fixed,
                    fixed_rate_period_months: 120,
                    payment_treatment: DsrPaymentTreatment::Scheduled,
                    schedule,
                },
            ];
            let new_schedule = given_schedule(1, 0, LoanRepaymentMethod::Bullet, &periods, &[]);
            let input = LeaseDepositAffordabilityInput {
                evaluation_game_day: 0,
                evaluation_end_game_day: 365,
                verified_annual_income_krw: Some(1),
                maximum_ratio_ppm: 400_000,
                stress_policy: given_dsr_policy(),
                existing_loans: &existing_loans,
                new_loan: LeaseDepositAffordabilityNewLoanInput {
                    loan_id: 3,
                    schedule: new_schedule,
                },
                replaced_loan_id: None,
            };

            let result = given_rules().assess_lease_deposit_affordability(input);

            assert_eq!(result, Err(LoanRuleError::ArithmeticOverflow));
        }
    }

    mod context_ltv를_심사하는_경우 {
        use super::*;

        #[test]
        fn given_선순위신규원금비용과담보가치_when_심사하면_then_모두분자에포함한다() {
            let input = LtvAssessmentInput {
                existing_senior_balance_krw: 20,
                new_principal_krw: 30,
                included_fees_krw: 1,
                recognized_collateral_value_krw: Some(100),
                maximum_ratio_ppm: 500_000,
            };

            let result = given_rules()
                .assess_ltv(input)
                .expect("LTV를 계산해야 한다");

            assert_eq!((result.numerator_krw, result.ratio_ppm), (51, 510_000));
        }

        #[test]
        fn given_담보가치provider가값을주지않을때_when_심사하면_then_valuation_unavailable이다() {
            let input = LtvAssessmentInput {
                existing_senior_balance_krw: 0,
                new_principal_krw: 1,
                included_fees_krw: 0,
                recognized_collateral_value_krw: None,
                maximum_ratio_ppm: 500_000,
            };

            let result = given_rules().assess_ltv(input);

            assert_eq!(result, Err(LoanRuleError::ValuationUnavailable));
        }
    }
}
