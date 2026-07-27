use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::types::{
    AnnualLocalIncomeTaxPolicy, CombinedEmploymentTaxHandoff, CombinedEmploymentTaxPlanningInput,
    EarnedIncomeDeductionBracket, EarnedIncomeTaxCreditPolicy, EmployeeStatutoryInsuranceAmounts,
    EmploymentAnnualTaxPolicy, EmploymentIncomeAuthority, EmploymentIncomeAuthorityInput,
    EmploymentOnlyTaxPlanningInput, EmploymentTaxAssessmentPlan, EmploymentTaxAssessmentSource,
    EmploymentTaxAssessmentStatus, EmploymentTaxCalculation, EmploymentTaxError,
    EmploymentTaxRules, PensionContributionAccountKind, PensionContributionAllocation,
    PensionContributionCreditPolicy, PensionContributionCreditRate, PensionContributionEvent,
    PensionContributionSourceEvent, PensionCreditAllocationPlan, PensionCreditIncome,
    PensionCreditPlanningInput, PensionOpeningTaxExcludedBalance, ProgressiveEmploymentTaxBracket,
    TaxReconciliationPlan,
};

const RATE_SCALE_PPM: i128 = 1_000_000;
const MAX_PERSONAL_DEDUCTION_PEOPLE: u8 = 7;

pub fn create_employment_tax_rules() -> Arc<dyn EmploymentTaxRules> {
    Arc::new(DefaultEmploymentTaxRules)
}

struct DefaultEmploymentTaxRules;

impl EmploymentTaxRules for DefaultEmploymentTaxRules {
    fn validate_policy(
        &self,
        policy: &EmploymentAnnualTaxPolicy,
    ) -> Result<(), EmploymentTaxError> {
        validate_policy(policy)
    }

    fn select_income_authority(
        &self,
        input: EmploymentIncomeAuthorityInput,
    ) -> Result<EmploymentIncomeAuthority, EmploymentTaxError> {
        validate_tax_year(input.tax_year)?;
        if !(1..=9999).contains(&input.world_start_year) {
            return Err(EmploymentTaxError::InvalidTaxYear);
        }

        if input.has_m3_taxable_payroll || input.has_employment_income_year {
            return Ok(EmploymentIncomeAuthority::M3Payroll);
        }

        let legacy_tax_year = input
            .world_start_year
            .checked_sub(1)
            .ok_or(EmploymentTaxError::InvalidTaxYear)?;
        if input.legacy_profile_exists && i32::from(input.tax_year) == legacy_tax_year {
            Ok(EmploymentIncomeAuthority::LegacyProfile)
        } else {
            Ok(EmploymentIncomeAuthority::None)
        }
    }

    fn plan_pension_credit(
        &self,
        input: PensionCreditPlanningInput<'_>,
    ) -> Result<PensionCreditAllocationPlan, EmploymentTaxError> {
        validate_policy(input.policy)?;
        validate_matching_tax_year(input.tax_year, input.policy)?;
        validate_non_negative_money(&[
            input.remaining_income_tax_before_pension_credit_krw,
            input.local_income_tax_before_pension_effect_krw,
        ])?;
        let selected_rate = select_pension_rate(input.income, &input.policy.pension_credit)?;
        let sources = replay_pension_sources(
            input.tax_year,
            input.opening_tax_excluded_balances,
            input.source_events,
        )?;
        let eligible_sources = allocate_pension_limits(&sources, &input.policy.pension_credit)?;
        let total_eligible_krw = sum_i64(
            eligible_sources
                .iter()
                .map(|source| source.limit_eligible_krw),
        )?;
        let credited_contribution_krw = maximum_creditable_contribution(
            total_eligible_krw,
            input.remaining_income_tax_before_pension_credit_krw,
            selected_rate,
        )?;
        let income_tax_credit_krw =
            contribution_income_tax_credit(credited_contribution_krw, selected_rate)?;
        let income_tax_after_credit_krw = input
            .remaining_income_tax_before_pension_credit_krw
            .checked_sub(income_tax_credit_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        let linked_local_before_krw = calculate_linked_local_income_tax(
            input.remaining_income_tax_before_pension_credit_krw,
            input.policy.local_income_tax,
        )?;
        let linked_local_after_krw = calculate_linked_local_income_tax(
            income_tax_after_credit_krw,
            input.policy.local_income_tax,
        )?;
        let local_income_tax_effect_krw = linked_local_before_krw
            .checked_sub(linked_local_after_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?
            .min(input.local_income_tax_before_pension_effect_krw);
        let allocations = build_source_allocations(
            &eligible_sources,
            credited_contribution_krw,
            selected_rate,
            input.remaining_income_tax_before_pension_credit_krw,
            input.local_income_tax_before_pension_effect_krw,
            input.policy.local_income_tax,
        )?;

        Ok(PensionCreditAllocationPlan {
            tax_year: input.tax_year,
            selected_income_tax_rate_ppm: selected_rate.income_tax_rate_ppm,
            limit_eligible_contribution_krw: total_eligible_krw,
            credited_contribution_krw,
            income_tax_credit_krw,
            local_income_tax_effect_krw,
            allocations,
        })
    }

    fn plan_employment_only(
        &self,
        input: EmploymentOnlyTaxPlanningInput<'_>,
    ) -> Result<EmploymentTaxAssessmentPlan, EmploymentTaxError> {
        require_m3_authority(input.authority)?;
        validate_policy(input.policy)?;
        validate_matching_tax_year(input.tax_year, input.policy)?;
        validate_non_negative_money(&[
            input.gross_employment_income_krw,
            input.withheld_income_tax_krw,
            input.withheld_local_income_tax_krw,
        ])?;
        if !(1..=MAX_PERSONAL_DEDUCTION_PEOPLE).contains(&input.personal_deduction_person_count) {
            return Err(EmploymentTaxError::InvalidPersonCount);
        }

        let employee_insurance_deduction_krw =
            sum_employee_insurance(input.employee_statutory_insurance)?;
        let earned_income_deduction_krw = calculate_earned_income_deduction(
            input.gross_employment_income_krw,
            &input.policy.earned_income_deduction_brackets,
        )?;
        let personal_deduction_krw = checked_i128_to_i64(
            i128::from(input.policy.basic_personal_deduction_per_person_krw)
                .checked_mul(i128::from(input.personal_deduction_person_count))
                .ok_or(EmploymentTaxError::ArithmeticOverflow)?,
        )?;
        let earned_income_krw = input
            .gross_employment_income_krw
            .checked_sub(earned_income_deduction_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        let total_income_deductions_krw = i128::from(personal_deduction_krw)
            .checked_add(i128::from(employee_insurance_deduction_krw))
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        let taxable_before_rounding_krw = (i128::from(earned_income_krw)
            .checked_sub(total_income_deductions_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?)
        .max(0);
        let taxable_income_krw = checked_i128_to_i64(floor_i128_to_unit(
            taxable_before_rounding_krw,
            input.policy.taxable_income_rounding_unit_krw,
        )?)?;
        let calculated_income_tax_krw = calculate_basic_income_tax(
            taxable_income_krw,
            &input.policy.basic_tax_brackets,
            input.policy.calculated_tax_rounding_unit_krw,
        )?;
        let earned_income_tax_credit_krw = calculate_earned_income_tax_credit(
            calculated_income_tax_krw,
            input.gross_employment_income_krw,
            input.policy.earned_income_tax_credit,
        )?;
        let income_tax_before_pension_credit_krw = calculated_income_tax_krw
            .checked_sub(earned_income_tax_credit_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        let local_income_tax_before_pension_effect_krw = calculate_linked_local_income_tax(
            income_tax_before_pension_credit_krw,
            input.policy.local_income_tax,
        )?;
        let pension = self.plan_pension_credit(PensionCreditPlanningInput {
            tax_year: input.tax_year,
            income: PensionCreditIncome::EmploymentSalary {
                total_salary_krw: input.gross_employment_income_krw,
            },
            remaining_income_tax_before_pension_credit_krw: income_tax_before_pension_credit_krw,
            local_income_tax_before_pension_effect_krw,
            opening_tax_excluded_balances: input.pension_opening_tax_excluded_balances,
            source_events: input.pension_source_events,
            policy: input.policy,
        })?;
        let assessed_income_tax_krw = income_tax_before_pension_credit_krw
            .checked_sub(pension.income_tax_credit_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        let assessed_local_income_tax_krw = local_income_tax_before_pension_effect_krw
            .checked_sub(pension.local_income_tax_effect_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        let reconciliation = self.plan_reconciliation(
            input.withheld_income_tax_krw,
            input.withheld_local_income_tax_krw,
            assessed_income_tax_krw,
            assessed_local_income_tax_krw,
        )?;
        let status = if input.requires_combined_assessment {
            EmploymentTaxAssessmentStatus::Provisional
        } else {
            EmploymentTaxAssessmentStatus::Definitive
        };
        let combined_handoff = CombinedEmploymentTaxHandoff {
            tax_year: input.tax_year,
            gross_employment_income_krw: input.gross_employment_income_krw,
            earned_income_deduction_krw,
            personal_deduction_krw,
            employee_insurance_deduction_krw,
            employment_taxable_income_krw: taxable_income_krw,
            calculated_employment_income_tax_krw: calculated_income_tax_krw,
            earned_income_tax_credit_krw,
            final_prepaid_employment_income_tax_krw: assessed_income_tax_krw,
            final_prepaid_employment_local_income_tax_krw: assessed_local_income_tax_krw,
        };

        Ok(EmploymentTaxAssessmentPlan {
            calculation: EmploymentTaxCalculation {
                tax_year: input.tax_year,
                status,
                source: EmploymentTaxAssessmentSource::EmploymentOnly,
                gross_employment_income_krw: input.gross_employment_income_krw,
                employee_insurance_deduction_krw,
                earned_income_deduction_krw,
                personal_deduction_krw,
                taxable_income_krw,
                calculated_income_tax_krw,
                earned_income_tax_credit_krw,
                pension_credit_eligible_contribution_krw: pension.limit_eligible_contribution_krw,
                actual_pension_income_tax_credit_krw: pension.income_tax_credit_krw,
                actual_pension_local_income_tax_effect_krw: pension.local_income_tax_effect_krw,
                assessed_income_tax_krw,
                assessed_local_income_tax_krw,
            },
            reconciliation,
            combined_handoff,
            pension_allocation: if input.requires_combined_assessment {
                None
            } else {
                Some(pension)
            },
        })
    }

    fn plan_combined(
        &self,
        input: CombinedEmploymentTaxPlanningInput<'_>,
    ) -> Result<EmploymentTaxAssessmentPlan, EmploymentTaxError> {
        require_m3_authority(input.authority)?;
        validate_policy(input.policy)?;
        validate_matching_tax_year(input.handoff.tax_year, input.policy)?;
        validate_non_negative_money(&[
            input.handoff.gross_employment_income_krw,
            input.handoff.earned_income_deduction_krw,
            input.handoff.personal_deduction_krw,
            input.handoff.employee_insurance_deduction_krw,
            input.handoff.employment_taxable_income_krw,
            input.handoff.calculated_employment_income_tax_krw,
            input.handoff.earned_income_tax_credit_krw,
            input.handoff.final_prepaid_employment_income_tax_krw,
            input.handoff.final_prepaid_employment_local_income_tax_krw,
            input.comprehensive_income_krw,
            input.calculated_combined_income_tax_krw,
            input.income_tax_before_pension_credit_krw,
            input.local_income_tax_before_pension_effect_krw,
            input.total_prepaid_income_tax_krw,
            input.total_prepaid_local_income_tax_krw,
        ])?;
        if input.income_tax_before_pension_credit_krw > input.calculated_combined_income_tax_krw
            || input.handoff.earned_income_tax_credit_krw
                > input.handoff.calculated_employment_income_tax_krw
        {
            return Err(EmploymentTaxError::InvalidCombinedTaxInput);
        }

        let pension = self.plan_pension_credit(PensionCreditPlanningInput {
            tax_year: input.handoff.tax_year,
            income: PensionCreditIncome::ComprehensiveIncome {
                comprehensive_income_krw: input.comprehensive_income_krw,
            },
            remaining_income_tax_before_pension_credit_krw: input
                .income_tax_before_pension_credit_krw,
            local_income_tax_before_pension_effect_krw: input
                .local_income_tax_before_pension_effect_krw,
            opening_tax_excluded_balances: input.pension_opening_tax_excluded_balances,
            source_events: input.pension_source_events,
            policy: input.policy,
        })?;
        let assessed_income_tax_krw = input
            .income_tax_before_pension_credit_krw
            .checked_sub(pension.income_tax_credit_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        let assessed_local_income_tax_krw = input
            .local_income_tax_before_pension_effect_krw
            .checked_sub(pension.local_income_tax_effect_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        let reconciliation = self.plan_reconciliation(
            input.total_prepaid_income_tax_krw,
            input.total_prepaid_local_income_tax_krw,
            assessed_income_tax_krw,
            assessed_local_income_tax_krw,
        )?;

        Ok(EmploymentTaxAssessmentPlan {
            calculation: EmploymentTaxCalculation {
                tax_year: input.handoff.tax_year,
                status: EmploymentTaxAssessmentStatus::Definitive,
                source: EmploymentTaxAssessmentSource::Combined,
                gross_employment_income_krw: input.handoff.gross_employment_income_krw,
                employee_insurance_deduction_krw: input.handoff.employee_insurance_deduction_krw,
                earned_income_deduction_krw: input.handoff.earned_income_deduction_krw,
                personal_deduction_krw: input.handoff.personal_deduction_krw,
                taxable_income_krw: input.handoff.employment_taxable_income_krw,
                calculated_income_tax_krw: input.calculated_combined_income_tax_krw,
                earned_income_tax_credit_krw: input.handoff.earned_income_tax_credit_krw,
                pension_credit_eligible_contribution_krw: pension.limit_eligible_contribution_krw,
                actual_pension_income_tax_credit_krw: pension.income_tax_credit_krw,
                actual_pension_local_income_tax_effect_krw: pension.local_income_tax_effect_krw,
                assessed_income_tax_krw,
                assessed_local_income_tax_krw,
            },
            reconciliation,
            combined_handoff: input.handoff,
            pension_allocation: Some(pension),
        })
    }

    fn plan_reconciliation(
        &self,
        prepaid_income_tax_krw: i64,
        prepaid_local_income_tax_krw: i64,
        assessed_income_tax_krw: i64,
        assessed_local_income_tax_krw: i64,
    ) -> Result<TaxReconciliationPlan, EmploymentTaxError> {
        validate_non_negative_money(&[
            prepaid_income_tax_krw,
            prepaid_local_income_tax_krw,
            assessed_income_tax_krw,
            assessed_local_income_tax_krw,
        ])?;
        let prepaid_total_krw = i128::from(prepaid_income_tax_krw)
            .checked_add(i128::from(prepaid_local_income_tax_krw))
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        let assessed_total_krw = i128::from(assessed_income_tax_krw)
            .checked_add(i128::from(assessed_local_income_tax_krw))
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        let difference_krw = assessed_total_krw
            .checked_sub(prepaid_total_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        let (additional_tax_krw, refund_krw) = if difference_krw > 0 {
            (checked_i128_to_i64(difference_krw)?, 0)
        } else if difference_krw < 0 {
            (
                0,
                checked_i128_to_i64(
                    difference_krw
                        .checked_neg()
                        .ok_or(EmploymentTaxError::ArithmeticOverflow)?,
                )?,
            )
        } else {
            (0, 0)
        };

        Ok(TaxReconciliationPlan {
            prepaid_income_tax_krw,
            prepaid_local_income_tax_krw,
            assessed_income_tax_krw,
            assessed_local_income_tax_krw,
            additional_tax_krw,
            refund_krw,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct ReplayedPensionSource {
    contribution: PensionContributionEvent,
    surviving_tax_excluded_krw: i64,
}

#[derive(Debug, Clone, Copy)]
struct LimitEligiblePensionSource {
    source: ReplayedPensionSource,
    limit_eligible_krw: i64,
}

#[derive(Debug, Default)]
struct PensionAccountReplay {
    opening_tax_excluded_krw: i64,
    sources: Vec<ReplayedPensionSource>,
}

fn validate_policy(policy: &EmploymentAnnualTaxPolicy) -> Result<(), EmploymentTaxError> {
    validate_tax_year(policy.tax_year)?;
    validate_non_negative_money(&[policy.basic_personal_deduction_per_person_krw])?;
    validate_rounding_unit(policy.taxable_income_rounding_unit_krw)?;
    validate_rounding_unit(policy.calculated_tax_rounding_unit_krw)?;
    validate_earned_income_deduction_brackets(&policy.earned_income_deduction_brackets)?;
    validate_basic_tax_brackets(&policy.basic_tax_brackets)?;
    validate_earned_income_tax_credit_policy(policy.earned_income_tax_credit)?;
    validate_pension_credit_policy(policy.pension_credit)?;
    validate_rate(policy.local_income_tax.linked_income_tax_rate_ppm)?;
    validate_rounding_unit(policy.local_income_tax.rounding_unit_krw)?;
    validate_pension_credit_total_rate(
        policy.pension_credit.lower_income_rate,
        policy.local_income_tax,
    )?;
    validate_pension_credit_total_rate(
        policy.pension_credit.higher_income_rate,
        policy.local_income_tax,
    )
}

fn validate_earned_income_deduction_brackets(
    brackets: &[EarnedIncomeDeductionBracket],
) -> Result<(), EmploymentTaxError> {
    if brackets.is_empty() || brackets[0].lower_bound_krw != 0 {
        return Err(EmploymentTaxError::InvalidEarnedIncomeDeductionBrackets);
    }
    let mut expected_lower_krw = 0;
    for (index, bracket) in brackets.iter().enumerate() {
        if bracket.lower_bound_krw != expected_lower_krw
            || bracket.base_deduction_krw < 0
            || validate_rate(bracket.marginal_rate_ppm).is_err()
        {
            return Err(EmploymentTaxError::InvalidEarnedIncomeDeductionBrackets);
        }
        match bracket.upper_bound_exclusive_krw {
            Some(upper_bound_krw) => {
                if index + 1 == brackets.len() || upper_bound_krw <= bracket.lower_bound_krw {
                    return Err(EmploymentTaxError::InvalidEarnedIncomeDeductionBrackets);
                }
                expected_lower_krw = upper_bound_krw;
            }
            None => {
                if index + 1 != brackets.len() {
                    return Err(EmploymentTaxError::InvalidEarnedIncomeDeductionBrackets);
                }
            }
        }
    }
    if brackets
        .last()
        .and_then(|bracket| bracket.upper_bound_exclusive_krw)
        .is_some()
    {
        return Err(EmploymentTaxError::InvalidEarnedIncomeDeductionBrackets);
    }

    for pair in brackets.windows(2) {
        let previous = pair[0];
        let current = pair[1];
        let upper_bound_krw = previous
            .upper_bound_exclusive_krw
            .ok_or(EmploymentTaxError::InvalidEarnedIncomeDeductionBrackets)?;
        let previous_slice_krw = upper_bound_krw
            .checked_sub(previous.lower_bound_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        let expected_base_krw = previous
            .base_deduction_krw
            .checked_add(floor_rate_to_unit(
                previous_slice_krw,
                previous.marginal_rate_ppm,
                1,
            )?)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        if current.base_deduction_krw != expected_base_krw {
            return Err(EmploymentTaxError::InvalidEarnedIncomeDeductionBrackets);
        }
    }
    Ok(())
}

fn validate_basic_tax_brackets(
    brackets: &[ProgressiveEmploymentTaxBracket],
) -> Result<(), EmploymentTaxError> {
    if brackets.is_empty() || brackets[0].lower_bound_krw != 0 {
        return Err(EmploymentTaxError::InvalidBasicTaxBrackets);
    }
    let mut expected_lower_krw = 0;
    for (index, bracket) in brackets.iter().enumerate() {
        if bracket.lower_bound_krw != expected_lower_krw
            || bracket.quick_deduction_krw < 0
            || validate_rate(bracket.rate_ppm).is_err()
        {
            return Err(EmploymentTaxError::InvalidBasicTaxBrackets);
        }
        let tax_at_lower_krw = floor_rate_to_unit(bracket.lower_bound_krw, bracket.rate_ppm, 1)?;
        if bracket.quick_deduction_krw > tax_at_lower_krw {
            return Err(EmploymentTaxError::InvalidBasicTaxBrackets);
        }
        match bracket.upper_bound_exclusive_krw {
            Some(upper_bound_krw) => {
                if index + 1 == brackets.len() || upper_bound_krw <= bracket.lower_bound_krw {
                    return Err(EmploymentTaxError::InvalidBasicTaxBrackets);
                }
                expected_lower_krw = upper_bound_krw;
            }
            None => {
                if index + 1 != brackets.len() {
                    return Err(EmploymentTaxError::InvalidBasicTaxBrackets);
                }
            }
        }
    }
    if brackets
        .last()
        .and_then(|bracket| bracket.upper_bound_exclusive_krw)
        .is_some()
    {
        Err(EmploymentTaxError::InvalidBasicTaxBrackets)
    } else {
        Ok(())
    }
}

fn validate_earned_income_tax_credit_policy(
    policy: EarnedIncomeTaxCreditPolicy,
) -> Result<(), EmploymentTaxError> {
    if policy.low_tax_boundary_krw <= 0
        || policy.salary_boundary_one_krw <= 0
        || policy.salary_boundary_two_krw <= policy.salary_boundary_one_krw
        || validate_rate(policy.low_tax_rate_ppm).is_err()
        || validate_rate(policy.high_tax_marginal_rate_ppm).is_err()
        || validate_rate(policy.cap_two_reduction_rate_ppm).is_err()
        || validate_rate(policy.cap_three_reduction_rate_ppm).is_err()
    {
        return Err(EmploymentTaxError::InvalidEarnedIncomeTaxCreditPolicy);
    }
    validate_non_negative_money(&[
        policy.high_tax_base_credit_krw,
        policy.cap_one_krw,
        policy.cap_two_base_krw,
        policy.cap_two_floor_krw,
        policy.cap_three_base_krw,
        policy.cap_three_floor_krw,
    ])?;
    if policy.cap_two_base_krw < policy.cap_two_floor_krw
        || policy.cap_three_base_krw < policy.cap_three_floor_krw
    {
        Err(EmploymentTaxError::InvalidEarnedIncomeTaxCreditPolicy)
    } else {
        Ok(())
    }
}

fn validate_pension_credit_policy(
    policy: PensionContributionCreditPolicy,
) -> Result<(), EmploymentTaxError> {
    validate_non_negative_money(&[
        policy.pension_savings_limit_krw,
        policy.pension_savings_and_irp_limit_krw,
        policy.salary_rate_boundary_krw,
        policy.comprehensive_income_rate_boundary_krw,
    ])?;
    if policy.pension_savings_limit_krw > policy.pension_savings_and_irp_limit_krw {
        return Err(EmploymentTaxError::InvalidPensionCreditPolicy);
    }
    validate_pension_credit_rate(policy.lower_income_rate)?;
    validate_pension_credit_rate(policy.higher_income_rate)
}

fn validate_pension_credit_rate(
    rate: PensionContributionCreditRate,
) -> Result<(), EmploymentTaxError> {
    validate_rate(rate.income_tax_rate_ppm)
        .map_err(|_| EmploymentTaxError::InvalidPensionCreditPolicy)?;
    validate_rounding_unit(rate.income_tax_rounding_unit_krw)
        .map_err(|_| EmploymentTaxError::InvalidPensionCreditPolicy)
}

fn validate_pension_credit_total_rate(
    pension_rate: PensionContributionCreditRate,
    local_policy: AnnualLocalIncomeTaxPolicy,
) -> Result<(), EmploymentTaxError> {
    let maximum_local_effect_rate = i128::from(pension_rate.income_tax_rate_ppm)
        .checked_mul(i128::from(local_policy.linked_income_tax_rate_ppm))
        .ok_or(EmploymentTaxError::ArithmeticOverflow)?
        .checked_add(RATE_SCALE_PPM - 1)
        .ok_or(EmploymentTaxError::ArithmeticOverflow)?
        .checked_div(RATE_SCALE_PPM)
        .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
    let maximum_total_rate = i128::from(pension_rate.income_tax_rate_ppm)
        .checked_add(maximum_local_effect_rate)
        .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
    if maximum_total_rate <= RATE_SCALE_PPM {
        Ok(())
    } else {
        Err(EmploymentTaxError::InvalidPensionCreditPolicy)
    }
}

fn replay_pension_sources(
    tax_year: u16,
    opening_balances: &[PensionOpeningTaxExcludedBalance],
    events: &[PensionContributionSourceEvent],
) -> Result<Vec<ReplayedPensionSource>, EmploymentTaxError> {
    validate_tax_year(tax_year)?;
    let mut accounts = HashMap::<u64, PensionAccountReplay>::new();
    for opening in opening_balances {
        if opening.account_id == 0 || opening.amount_krw < 0 {
            return Err(EmploymentTaxError::InvalidPensionEvent);
        }
        if accounts
            .insert(
                opening.account_id,
                PensionAccountReplay {
                    opening_tax_excluded_krw: opening.amount_krw,
                    sources: Vec::new(),
                },
            )
            .is_some()
        {
            return Err(EmploymentTaxError::DuplicatePensionOpeningBalance);
        }
    }

    let mut contribution_source_ids = HashSet::new();
    let mut ledger_transaction_ids = HashSet::new();
    let mut account_kinds = HashMap::new();
    let mut ordered_events = events.to_vec();
    ordered_events.sort_by_key(pension_event_sort_key);
    for event in &ordered_events {
        let (event_tax_year, ledger_transaction_id) = match event {
            PensionContributionSourceEvent::Contribution(contribution) => {
                if contribution.contribution_source_id == 0
                    || contribution.account_id == 0
                    || contribution.ledger_transaction_id == 0
                    || contribution.amount_krw <= 0
                    || contribution.tax_year != tax_year
                {
                    return Err(EmploymentTaxError::InvalidPensionEvent);
                }
                if !contribution_source_ids.insert(contribution.contribution_source_id) {
                    return Err(EmploymentTaxError::DuplicatePensionContributionSource);
                }
                if let Some(existing_kind) =
                    account_kinds.insert(contribution.account_id, contribution.account_kind)
                    && existing_kind != contribution.account_kind
                {
                    return Err(EmploymentTaxError::InvalidPensionEvent);
                }
                (contribution.tax_year, contribution.ledger_transaction_id)
            }
            PensionContributionSourceEvent::Withdrawal(withdrawal) => {
                if withdrawal.account_id == 0
                    || withdrawal.ledger_transaction_id == 0
                    || withdrawal.tax_excluded_withdrawn_krw <= 0
                    || withdrawal.tax_year != tax_year
                {
                    return Err(EmploymentTaxError::InvalidPensionEvent);
                }
                (withdrawal.tax_year, withdrawal.ledger_transaction_id)
            }
        };
        if event_tax_year != tax_year {
            return Err(EmploymentTaxError::InvalidPensionEvent);
        }
        if !ledger_transaction_ids.insert(ledger_transaction_id) {
            return Err(EmploymentTaxError::DuplicatePensionLedgerTransaction);
        }
    }

    for event in ordered_events {
        match event {
            PensionContributionSourceEvent::Contribution(contribution) => {
                accounts
                    .entry(contribution.account_id)
                    .or_default()
                    .sources
                    .push(ReplayedPensionSource {
                        contribution,
                        surviving_tax_excluded_krw: contribution.amount_krw,
                    });
            }
            PensionContributionSourceEvent::Withdrawal(withdrawal) => {
                let account = accounts
                    .get_mut(&withdrawal.account_id)
                    .ok_or(EmploymentTaxError::PensionWithdrawalExceedsHistory)?;
                let opening_consumed_krw = account
                    .opening_tax_excluded_krw
                    .min(withdrawal.tax_excluded_withdrawn_krw);
                account.opening_tax_excluded_krw = account
                    .opening_tax_excluded_krw
                    .checked_sub(opening_consumed_krw)
                    .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
                let mut remaining_withdrawal_krw = withdrawal
                    .tax_excluded_withdrawn_krw
                    .checked_sub(opening_consumed_krw)
                    .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
                for source in &mut account.sources {
                    let consumed_krw = source
                        .surviving_tax_excluded_krw
                        .min(remaining_withdrawal_krw);
                    source.surviving_tax_excluded_krw = source
                        .surviving_tax_excluded_krw
                        .checked_sub(consumed_krw)
                        .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
                    remaining_withdrawal_krw = remaining_withdrawal_krw
                        .checked_sub(consumed_krw)
                        .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
                    if remaining_withdrawal_krw == 0 {
                        break;
                    }
                }
                if remaining_withdrawal_krw != 0 {
                    return Err(EmploymentTaxError::PensionWithdrawalExceedsHistory);
                }
            }
        }
    }

    let mut sources: Vec<_> = accounts
        .into_values()
        .flat_map(|account| account.sources)
        .collect();
    sources.sort_by_key(pension_source_pool_sort_key);
    Ok(sources)
}

fn allocate_pension_limits(
    sources: &[ReplayedPensionSource],
    policy: &PensionContributionCreditPolicy,
) -> Result<Vec<LimitEligiblePensionSource>, EmploymentTaxError> {
    let mut pension_savings_remaining_krw = policy.pension_savings_limit_krw;
    let mut combined_remaining_krw = policy.pension_savings_and_irp_limit_krw;
    let mut eligible_sources = Vec::with_capacity(sources.len());
    for source in sources {
        let limit_eligible_krw = match source.contribution.account_kind {
            PensionContributionAccountKind::PensionSavings => source
                .surviving_tax_excluded_krw
                .min(pension_savings_remaining_krw)
                .min(combined_remaining_krw),
            PensionContributionAccountKind::Irp => source
                .surviving_tax_excluded_krw
                .min(combined_remaining_krw),
        };
        if source.contribution.account_kind == PensionContributionAccountKind::PensionSavings {
            pension_savings_remaining_krw = pension_savings_remaining_krw
                .checked_sub(limit_eligible_krw)
                .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        }
        combined_remaining_krw = combined_remaining_krw
            .checked_sub(limit_eligible_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        eligible_sources.push(LimitEligiblePensionSource {
            source: *source,
            limit_eligible_krw,
        });
    }
    Ok(eligible_sources)
}

fn build_source_allocations(
    sources: &[LimitEligiblePensionSource],
    credited_contribution_krw: i64,
    rate: PensionContributionCreditRate,
    income_tax_before_pension_credit_krw: i64,
    local_income_tax_before_pension_effect_krw: i64,
    local_income_tax_policy: AnnualLocalIncomeTaxPolicy,
) -> Result<Vec<PensionContributionAllocation>, EmploymentTaxError> {
    let mut remaining_creditable_contribution_krw = credited_contribution_krw;
    let mut cumulative_credited_contribution_krw = 0_i64;
    let mut cumulative_income_tax_credit_krw = 0_i64;
    let mut cumulative_local_income_tax_effect_krw = 0_i64;
    let linked_local_before_krw = calculate_linked_local_income_tax(
        income_tax_before_pension_credit_krw,
        local_income_tax_policy,
    )?;
    let mut allocations = Vec::with_capacity(sources.len());
    for source in sources {
        let credited_for_source_krw = source
            .limit_eligible_krw
            .min(remaining_creditable_contribution_krw);
        remaining_creditable_contribution_krw = remaining_creditable_contribution_krw
            .checked_sub(credited_for_source_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        cumulative_credited_contribution_krw = cumulative_credited_contribution_krw
            .checked_add(credited_for_source_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        let next_cumulative_credit_krw =
            contribution_income_tax_credit(cumulative_credited_contribution_krw, rate)?;
        let source_income_tax_credit_krw = next_cumulative_credit_krw
            .checked_sub(cumulative_income_tax_credit_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        cumulative_income_tax_credit_krw = next_cumulative_credit_krw;
        let income_tax_after_cumulative_credit_krw = income_tax_before_pension_credit_krw
            .checked_sub(cumulative_income_tax_credit_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        let linked_local_after_krw = calculate_linked_local_income_tax(
            income_tax_after_cumulative_credit_krw,
            local_income_tax_policy,
        )?;
        let next_cumulative_local_effect_krw = linked_local_before_krw
            .checked_sub(linked_local_after_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?
            .min(local_income_tax_before_pension_effect_krw);
        let source_local_income_tax_effect_krw = next_cumulative_local_effect_krw
            .checked_sub(cumulative_local_income_tax_effect_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        cumulative_local_income_tax_effect_krw = next_cumulative_local_effect_krw;
        let source_total_credit_krw = source_income_tax_credit_krw
            .checked_add(source_local_income_tax_effect_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        let tax_excluded_after_krw = source
            .source
            .surviving_tax_excluded_krw
            .checked_sub(credited_for_source_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        allocations.push(PensionContributionAllocation {
            contribution_source_id: source.source.contribution.contribution_source_id,
            account_id: source.source.contribution.account_id,
            account_kind: source.source.contribution.account_kind,
            contribution_game_day: source.source.contribution.contribution_game_day,
            ledger_transaction_id: source.source.contribution.ledger_transaction_id,
            surviving_tax_excluded_contribution_krw: source.source.surviving_tax_excluded_krw,
            limit_eligible_contribution_krw: source.limit_eligible_krw,
            credited_contribution_krw: credited_for_source_krw,
            credited_contribution_before_krw: 0,
            tax_excluded_contribution_after_krw: tax_excluded_after_krw,
            credited_contribution_after_krw: credited_for_source_krw,
            income_tax_credit_krw: source_income_tax_credit_krw,
            local_income_tax_effect_krw: source_local_income_tax_effect_krw,
            total_credit_krw: source_total_credit_krw,
        });
    }
    if remaining_creditable_contribution_krw != 0 {
        return Err(EmploymentTaxError::ArithmeticOverflow);
    }
    let expected_income_tax_credit_krw =
        contribution_income_tax_credit(credited_contribution_krw, rate)?;
    let income_tax_after_credit_krw = income_tax_before_pension_credit_krw
        .checked_sub(expected_income_tax_credit_krw)
        .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
    let expected_local_income_tax_effect_krw = linked_local_before_krw
        .checked_sub(calculate_linked_local_income_tax(
            income_tax_after_credit_krw,
            local_income_tax_policy,
        )?)
        .ok_or(EmploymentTaxError::ArithmeticOverflow)?
        .min(local_income_tax_before_pension_effect_krw);
    if cumulative_income_tax_credit_krw != expected_income_tax_credit_krw
        || cumulative_local_income_tax_effect_krw != expected_local_income_tax_effect_krw
    {
        return Err(EmploymentTaxError::ArithmeticOverflow);
    }
    Ok(allocations)
}

fn maximum_creditable_contribution(
    total_eligible_krw: i64,
    remaining_income_tax_krw: i64,
    rate: PensionContributionCreditRate,
) -> Result<i64, EmploymentTaxError> {
    if total_eligible_krw == 0 || remaining_income_tax_krw == 0 {
        return Ok(0);
    }
    let full_credit_krw = contribution_income_tax_credit(total_eligible_krw, rate)?;
    if full_credit_krw == 0 {
        return Ok(0);
    }
    if full_credit_krw <= remaining_income_tax_krw {
        return Ok(total_eligible_krw);
    }

    let mut low_krw = 0_i64;
    let mut high_krw = total_eligible_krw;
    while low_krw < high_krw {
        let distance_krw = high_krw
            .checked_sub(low_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        let midpoint_krw = low_krw
            .checked_add(
                distance_krw
                    .checked_add(1)
                    .ok_or(EmploymentTaxError::ArithmeticOverflow)?
                    / 2,
            )
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        if contribution_income_tax_credit(midpoint_krw, rate)? <= remaining_income_tax_krw {
            low_krw = midpoint_krw;
        } else {
            high_krw = midpoint_krw
                .checked_sub(1)
                .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        }
    }
    if contribution_income_tax_credit(low_krw, rate)? == 0 {
        Ok(0)
    } else {
        Ok(low_krw)
    }
}

fn select_pension_rate(
    income: PensionCreditIncome,
    policy: &PensionContributionCreditPolicy,
) -> Result<PensionContributionCreditRate, EmploymentTaxError> {
    let lower_income_rate = match income {
        PensionCreditIncome::EmploymentSalary { total_salary_krw } => {
            validate_non_negative_money(&[total_salary_krw])?;
            total_salary_krw <= policy.salary_rate_boundary_krw
        }
        PensionCreditIncome::ComprehensiveIncome {
            comprehensive_income_krw,
        } => {
            validate_non_negative_money(&[comprehensive_income_krw])?;
            comprehensive_income_krw <= policy.comprehensive_income_rate_boundary_krw
        }
    };
    Ok(if lower_income_rate {
        policy.lower_income_rate
    } else {
        policy.higher_income_rate
    })
}

fn calculate_earned_income_deduction(
    gross_income_krw: i64,
    brackets: &[EarnedIncomeDeductionBracket],
) -> Result<i64, EmploymentTaxError> {
    let bracket = brackets
        .iter()
        .find(|bracket| {
            gross_income_krw >= bracket.lower_bound_krw
                && bracket
                    .upper_bound_exclusive_krw
                    .is_none_or(|upper_bound_krw| gross_income_krw < upper_bound_krw)
        })
        .ok_or(EmploymentTaxError::InvalidEarnedIncomeDeductionBrackets)?;
    let marginal_income_krw = gross_income_krw
        .checked_sub(bracket.lower_bound_krw)
        .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
    let deduction_krw = bracket
        .base_deduction_krw
        .checked_add(floor_rate_to_unit(
            marginal_income_krw,
            bracket.marginal_rate_ppm,
            1,
        )?)
        .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
    if deduction_krw > gross_income_krw {
        Err(EmploymentTaxError::InvalidEarnedIncomeDeductionBrackets)
    } else {
        Ok(deduction_krw)
    }
}

fn calculate_basic_income_tax(
    taxable_income_krw: i64,
    brackets: &[ProgressiveEmploymentTaxBracket],
    rounding_unit_krw: i64,
) -> Result<i64, EmploymentTaxError> {
    let bracket = brackets
        .iter()
        .find(|bracket| {
            taxable_income_krw >= bracket.lower_bound_krw
                && bracket
                    .upper_bound_exclusive_krw
                    .is_none_or(|upper_bound_krw| taxable_income_krw < upper_bound_krw)
        })
        .ok_or(EmploymentTaxError::InvalidBasicTaxBrackets)?;
    let gross_tax_krw = floor_rate_to_unit(taxable_income_krw, bracket.rate_ppm, 1)?;
    let tax_before_rounding_krw = gross_tax_krw
        .checked_sub(bracket.quick_deduction_krw)
        .ok_or(EmploymentTaxError::InvalidBasicTaxBrackets)?;
    checked_i128_to_i64(floor_i128_to_unit(
        i128::from(tax_before_rounding_krw),
        rounding_unit_krw,
    )?)
}

fn calculate_earned_income_tax_credit(
    calculated_income_tax_krw: i64,
    total_salary_krw: i64,
    policy: EarnedIncomeTaxCreditPolicy,
) -> Result<i64, EmploymentTaxError> {
    let formula_credit_krw = if calculated_income_tax_krw <= policy.low_tax_boundary_krw {
        floor_rate_to_unit(calculated_income_tax_krw, policy.low_tax_rate_ppm, 1)?
    } else {
        let excess_tax_krw = calculated_income_tax_krw
            .checked_sub(policy.low_tax_boundary_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        policy
            .high_tax_base_credit_krw
            .checked_add(floor_rate_to_unit(
                excess_tax_krw,
                policy.high_tax_marginal_rate_ppm,
                1,
            )?)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?
    };
    let cap_krw = if total_salary_krw <= policy.salary_boundary_one_krw {
        policy.cap_one_krw
    } else if total_salary_krw <= policy.salary_boundary_two_krw {
        let salary_excess_krw = total_salary_krw
            .checked_sub(policy.salary_boundary_one_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        policy
            .cap_two_base_krw
            .checked_sub(floor_rate_to_unit(
                salary_excess_krw,
                policy.cap_two_reduction_rate_ppm,
                1,
            )?)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?
            .max(policy.cap_two_floor_krw)
    } else {
        let salary_excess_krw = total_salary_krw
            .checked_sub(policy.salary_boundary_two_krw)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
        policy
            .cap_three_base_krw
            .checked_sub(floor_rate_to_unit(
                salary_excess_krw,
                policy.cap_three_reduction_rate_ppm,
                1,
            )?)
            .ok_or(EmploymentTaxError::ArithmeticOverflow)?
            .max(policy.cap_three_floor_krw)
    };
    Ok(formula_credit_krw
        .min(cap_krw)
        .min(calculated_income_tax_krw))
}

fn calculate_linked_local_income_tax(
    income_tax_krw: i64,
    policy: AnnualLocalIncomeTaxPolicy,
) -> Result<i64, EmploymentTaxError> {
    floor_rate_to_unit(
        income_tax_krw,
        policy.linked_income_tax_rate_ppm,
        policy.rounding_unit_krw,
    )
}

fn contribution_income_tax_credit(
    contribution_krw: i64,
    rate: PensionContributionCreditRate,
) -> Result<i64, EmploymentTaxError> {
    floor_rate_to_unit(
        contribution_krw,
        rate.income_tax_rate_ppm,
        rate.income_tax_rounding_unit_krw,
    )
}

fn sum_employee_insurance(
    insurance: EmployeeStatutoryInsuranceAmounts,
) -> Result<i64, EmploymentTaxError> {
    validate_non_negative_money(&[
        insurance.national_pension_krw,
        insurance.health_insurance_krw,
        insurance.long_term_care_krw,
        insurance.employment_insurance_krw,
    ])?;
    sum_i64([
        insurance.national_pension_krw,
        insurance.health_insurance_krw,
        insurance.long_term_care_krw,
        insurance.employment_insurance_krw,
    ])
}

fn pension_event_sort_key(event: &PensionContributionSourceEvent) -> (u32, u64, u8, u64) {
    match event {
        PensionContributionSourceEvent::Contribution(contribution) => (
            contribution.contribution_game_day,
            contribution.ledger_transaction_id,
            0,
            contribution.contribution_source_id,
        ),
        PensionContributionSourceEvent::Withdrawal(withdrawal) => (
            withdrawal.withdrawal_game_day,
            withdrawal.ledger_transaction_id,
            1,
            withdrawal.account_id,
        ),
    }
}

fn pension_source_pool_sort_key(source: &ReplayedPensionSource) -> (u8, u32, u64, u64, u64) {
    let account_kind_rank = match source.contribution.account_kind {
        PensionContributionAccountKind::PensionSavings => 0,
        PensionContributionAccountKind::Irp => 1,
    };
    (
        account_kind_rank,
        source.contribution.contribution_game_day,
        source.contribution.ledger_transaction_id,
        source.contribution.account_id,
        source.contribution.contribution_source_id,
    )
}

fn floor_rate_to_unit(
    amount_krw: i64,
    rate_ppm: i64,
    rounding_unit_krw: i64,
) -> Result<i64, EmploymentTaxError> {
    if amount_krw < 0 {
        return Err(EmploymentTaxError::InvalidMoney);
    }
    validate_rate(rate_ppm)?;
    validate_rounding_unit(rounding_unit_krw)?;
    let raw_krw = i128::from(amount_krw)
        .checked_mul(i128::from(rate_ppm))
        .ok_or(EmploymentTaxError::ArithmeticOverflow)?
        .checked_div(RATE_SCALE_PPM)
        .ok_or(EmploymentTaxError::ArithmeticOverflow)?;
    checked_i128_to_i64(floor_i128_to_unit(raw_krw, rounding_unit_krw)?)
}

fn floor_i128_to_unit(value: i128, unit_krw: i64) -> Result<i128, EmploymentTaxError> {
    if value < 0 {
        return Err(EmploymentTaxError::InvalidMoney);
    }
    validate_rounding_unit(unit_krw)?;
    let unit = i128::from(unit_krw);
    value
        .checked_div(unit)
        .and_then(|quotient| quotient.checked_mul(unit))
        .ok_or(EmploymentTaxError::ArithmeticOverflow)
}

fn sum_i64(values: impl IntoIterator<Item = i64>) -> Result<i64, EmploymentTaxError> {
    let total = values.into_iter().try_fold(0_i128, |total, value| {
        total
            .checked_add(i128::from(value))
            .ok_or(EmploymentTaxError::ArithmeticOverflow)
    })?;
    checked_i128_to_i64(total)
}

fn checked_i128_to_i64(value: i128) -> Result<i64, EmploymentTaxError> {
    i64::try_from(value).map_err(|_| EmploymentTaxError::ArithmeticOverflow)
}

fn validate_tax_year(tax_year: u16) -> Result<(), EmploymentTaxError> {
    if (1..=9999).contains(&tax_year) {
        Ok(())
    } else {
        Err(EmploymentTaxError::InvalidTaxYear)
    }
}

fn validate_matching_tax_year(
    tax_year: u16,
    policy: &EmploymentAnnualTaxPolicy,
) -> Result<(), EmploymentTaxError> {
    validate_tax_year(tax_year)?;
    if tax_year == policy.tax_year {
        Ok(())
    } else {
        Err(EmploymentTaxError::PolicyTaxYearMismatch)
    }
}

fn validate_non_negative_money(values: &[i64]) -> Result<(), EmploymentTaxError> {
    if values.iter().any(|value| *value < 0) {
        Err(EmploymentTaxError::InvalidMoney)
    } else {
        Ok(())
    }
}

fn validate_rate(rate_ppm: i64) -> Result<(), EmploymentTaxError> {
    if (1..=1_000_000).contains(&rate_ppm) {
        Ok(())
    } else {
        Err(EmploymentTaxError::InvalidRate)
    }
}

fn validate_rounding_unit(unit_krw: i64) -> Result<(), EmploymentTaxError> {
    if unit_krw > 0 {
        Ok(())
    } else {
        Err(EmploymentTaxError::InvalidRoundingUnit)
    }
}

fn require_m3_authority(authority: EmploymentIncomeAuthority) -> Result<(), EmploymentTaxError> {
    if authority == EmploymentIncomeAuthority::M3Payroll {
        Ok(())
    } else {
        Err(EmploymentTaxError::M3PayrollAuthorityRequired)
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::PensionWithdrawalEvent;
    use super::*;

    const TAX_YEAR: u16 = 2026;

    fn given_policy() -> EmploymentAnnualTaxPolicy {
        EmploymentAnnualTaxPolicy {
            tax_year: TAX_YEAR,
            earned_income_deduction_brackets: vec![
                EarnedIncomeDeductionBracket {
                    lower_bound_krw: 0,
                    upper_bound_exclusive_krw: Some(5_000_000),
                    base_deduction_krw: 0,
                    marginal_rate_ppm: 700_000,
                },
                EarnedIncomeDeductionBracket {
                    lower_bound_krw: 5_000_000,
                    upper_bound_exclusive_krw: Some(15_000_000),
                    base_deduction_krw: 3_500_000,
                    marginal_rate_ppm: 400_000,
                },
                EarnedIncomeDeductionBracket {
                    lower_bound_krw: 15_000_000,
                    upper_bound_exclusive_krw: Some(45_000_000),
                    base_deduction_krw: 7_500_000,
                    marginal_rate_ppm: 150_000,
                },
                EarnedIncomeDeductionBracket {
                    lower_bound_krw: 45_000_000,
                    upper_bound_exclusive_krw: Some(100_000_000),
                    base_deduction_krw: 12_000_000,
                    marginal_rate_ppm: 50_000,
                },
                EarnedIncomeDeductionBracket {
                    lower_bound_krw: 100_000_000,
                    upper_bound_exclusive_krw: None,
                    base_deduction_krw: 14_750_000,
                    marginal_rate_ppm: 20_000,
                },
            ],
            basic_personal_deduction_per_person_krw: 1_500_000,
            taxable_income_rounding_unit_krw: 10_000,
            basic_tax_brackets: vec![
                ProgressiveEmploymentTaxBracket {
                    lower_bound_krw: 0,
                    upper_bound_exclusive_krw: Some(14_000_000),
                    rate_ppm: 60_000,
                    quick_deduction_krw: 0,
                },
                ProgressiveEmploymentTaxBracket {
                    lower_bound_krw: 14_000_000,
                    upper_bound_exclusive_krw: Some(50_000_000),
                    rate_ppm: 150_000,
                    quick_deduction_krw: 1_260_000,
                },
                ProgressiveEmploymentTaxBracket {
                    lower_bound_krw: 50_000_000,
                    upper_bound_exclusive_krw: Some(88_000_000),
                    rate_ppm: 240_000,
                    quick_deduction_krw: 5_760_000,
                },
                ProgressiveEmploymentTaxBracket {
                    lower_bound_krw: 88_000_000,
                    upper_bound_exclusive_krw: Some(150_000_000),
                    rate_ppm: 350_000,
                    quick_deduction_krw: 15_440_000,
                },
                ProgressiveEmploymentTaxBracket {
                    lower_bound_krw: 150_000_000,
                    upper_bound_exclusive_krw: Some(300_000_000),
                    rate_ppm: 380_000,
                    quick_deduction_krw: 19_940_000,
                },
                ProgressiveEmploymentTaxBracket {
                    lower_bound_krw: 300_000_000,
                    upper_bound_exclusive_krw: Some(500_000_000),
                    rate_ppm: 400_000,
                    quick_deduction_krw: 25_940_000,
                },
                ProgressiveEmploymentTaxBracket {
                    lower_bound_krw: 500_000_000,
                    upper_bound_exclusive_krw: Some(1_000_000_000),
                    rate_ppm: 420_000,
                    quick_deduction_krw: 35_940_000,
                },
                ProgressiveEmploymentTaxBracket {
                    lower_bound_krw: 1_000_000_000,
                    upper_bound_exclusive_krw: None,
                    rate_ppm: 450_000,
                    quick_deduction_krw: 65_940_000,
                },
            ],
            calculated_tax_rounding_unit_krw: 10,
            earned_income_tax_credit: EarnedIncomeTaxCreditPolicy {
                low_tax_boundary_krw: 1_300_000,
                low_tax_rate_ppm: 550_000,
                high_tax_base_credit_krw: 715_000,
                high_tax_marginal_rate_ppm: 300_000,
                salary_boundary_one_krw: 33_000_000,
                salary_boundary_two_krw: 70_000_000,
                cap_one_krw: 740_000,
                cap_two_base_krw: 740_000,
                cap_two_reduction_rate_ppm: 8_000,
                cap_two_floor_krw: 660_000,
                cap_three_base_krw: 660_000,
                cap_three_reduction_rate_ppm: 500_000,
                cap_three_floor_krw: 500_000,
            },
            pension_credit: PensionContributionCreditPolicy {
                pension_savings_limit_krw: 6_000_000,
                pension_savings_and_irp_limit_krw: 9_000_000,
                salary_rate_boundary_krw: 55_000_000,
                comprehensive_income_rate_boundary_krw: 45_000_000,
                lower_income_rate: PensionContributionCreditRate {
                    income_tax_rate_ppm: 150_000,
                    income_tax_rounding_unit_krw: 1,
                },
                higher_income_rate: PensionContributionCreditRate {
                    income_tax_rate_ppm: 120_000,
                    income_tax_rounding_unit_krw: 1,
                },
            },
            local_income_tax: AnnualLocalIncomeTaxPolicy {
                linked_income_tax_rate_ppm: 100_000,
                rounding_unit_krw: 10,
            },
        }
    }

    fn given_employee_insurance() -> EmployeeStatutoryInsuranceAmounts {
        EmployeeStatutoryInsuranceAmounts {
            national_pension_krw: 1_200_000,
            health_insurance_krw: 1_200_000,
            long_term_care_krw: 200_000,
            employment_insurance_krw: 400_000,
        }
    }

    fn given_contribution(
        source_id: u64,
        account_id: u64,
        account_kind: PensionContributionAccountKind,
        game_day: u32,
        ledger_transaction_id: u64,
        amount_krw: i64,
    ) -> PensionContributionSourceEvent {
        PensionContributionSourceEvent::Contribution(PensionContributionEvent {
            contribution_source_id: source_id,
            account_id,
            account_kind,
            tax_year: TAX_YEAR,
            contribution_game_day: game_day,
            ledger_transaction_id,
            amount_krw,
        })
    }

    fn given_withdrawal(
        account_id: u64,
        game_day: u32,
        ledger_transaction_id: u64,
        amount_krw: i64,
    ) -> PensionContributionSourceEvent {
        PensionContributionSourceEvent::Withdrawal(PensionWithdrawalEvent {
            account_id,
            tax_year: TAX_YEAR,
            withdrawal_game_day: game_day,
            ledger_transaction_id,
            tax_excluded_withdrawn_krw: amount_krw,
        })
    }

    fn given_pension_events() -> Vec<PensionContributionSourceEvent> {
        vec![
            given_contribution(
                101,
                1,
                PensionContributionAccountKind::PensionSavings,
                10,
                10,
                6_000_000,
            ),
            given_contribution(
                102,
                2,
                PensionContributionAccountKind::Irp,
                20,
                20,
                3_000_000,
            ),
        ]
    }

    fn when_plan_employment_only(
        policy: &EmploymentAnnualTaxPolicy,
        pension_events: &[PensionContributionSourceEvent],
        requires_combined_assessment: bool,
    ) -> Result<EmploymentTaxAssessmentPlan, EmploymentTaxError> {
        create_employment_tax_rules().plan_employment_only(EmploymentOnlyTaxPlanningInput {
            authority: EmploymentIncomeAuthority::M3Payroll,
            tax_year: TAX_YEAR,
            gross_employment_income_krw: 36_000_000,
            employee_statutory_insurance: given_employee_insurance(),
            personal_deduction_person_count: 1,
            withheld_income_tax_krw: 1_000_000,
            withheld_local_income_tax_krw: 100_000,
            requires_combined_assessment,
            pension_opening_tax_excluded_balances: &[],
            pension_source_events: pension_events,
            policy,
        })
    }

    fn when_plan_pension_credit(
        policy: &EmploymentAnnualTaxPolicy,
        opening: &[PensionOpeningTaxExcludedBalance],
        events: &[PensionContributionSourceEvent],
        remaining_income_tax_krw: i64,
    ) -> Result<PensionCreditAllocationPlan, EmploymentTaxError> {
        create_employment_tax_rules().plan_pension_credit(PensionCreditPlanningInput {
            tax_year: TAX_YEAR,
            income: PensionCreditIncome::EmploymentSalary {
                total_salary_krw: 36_000_000,
            },
            remaining_income_tax_before_pension_credit_krw: remaining_income_tax_krw,
            local_income_tax_before_pension_effect_krw: 1_000_000,
            opening_tax_excluded_balances: opening,
            source_events: events,
            policy,
        })
    }

    mod context_legacy_권위_경계 {
        use super::*;

        #[test]
        fn given_legacy와_m3가_같은_연도에_존재_when_권위를_고르면_then_m3만_선택한다() {
            let rules = create_employment_tax_rules();

            let result = rules.select_income_authority(EmploymentIncomeAuthorityInput {
                tax_year: 2025,
                world_start_year: 2026,
                legacy_profile_exists: true,
                has_m3_taxable_payroll: true,
                has_employment_income_year: false,
            });

            assert_eq!(result, Ok(EmploymentIncomeAuthority::M3Payroll));
        }

        #[test]
        fn given_급여는_없고_m3_연간행만_존재_when_권위를_고르면_then_m3를_선택한다() {
            let rules = create_employment_tax_rules();

            let result = rules.select_income_authority(EmploymentIncomeAuthorityInput {
                tax_year: 2025,
                world_start_year: 2026,
                legacy_profile_exists: true,
                has_m3_taxable_payroll: false,
                has_employment_income_year: true,
            });

            assert_eq!(result, Ok(EmploymentIncomeAuthority::M3Payroll));
        }

        #[test]
        fn given_legacy만_존재_when_시작연도_직전연도를_고르면_then_legacy를_선택한다() {
            let rules = create_employment_tax_rules();

            let result = rules.select_income_authority(EmploymentIncomeAuthorityInput {
                tax_year: 2025,
                world_start_year: 2026,
                legacy_profile_exists: true,
                has_m3_taxable_payroll: false,
                has_employment_income_year: false,
            });

            assert_eq!(result, Ok(EmploymentIncomeAuthority::LegacyProfile));
        }

        #[test]
        fn given_legacy만_존재_when_다른연도를_고르면_then_반복사용하지_않는다() {
            let rules = create_employment_tax_rules();

            let result = rules.select_income_authority(EmploymentIncomeAuthorityInput {
                tax_year: 2026,
                world_start_year: 2026,
                legacy_profile_exists: true,
                has_m3_taxable_payroll: false,
                has_employment_income_year: false,
            });

            assert_eq!(result, Ok(EmploymentIncomeAuthority::None));
        }

        #[test]
        fn given_legacy_권위_when_m3_계산을_요청하면_then_혼합을_거절한다() {
            let policy = given_policy();
            let rules = create_employment_tax_rules();

            let result = rules.plan_employment_only(EmploymentOnlyTaxPlanningInput {
                authority: EmploymentIncomeAuthority::LegacyProfile,
                tax_year: TAX_YEAR,
                gross_employment_income_krw: 1,
                employee_statutory_insurance: EmployeeStatutoryInsuranceAmounts::default(),
                personal_deduction_person_count: 1,
                withheld_income_tax_krw: 0,
                withheld_local_income_tax_krw: 0,
                requires_combined_assessment: false,
                pension_opening_tax_excluded_balances: &[],
                pension_source_events: &[],
                policy: &policy,
            });

            assert_eq!(result, Err(EmploymentTaxError::M3PayrollAuthorityRequired));
        }
    }

    mod context_근로소득_연말정산 {
        use super::*;

        #[test]
        fn given_고정_fixture_when_확정계산하면_then_golden_금액을_만든다() {
            let policy = given_policy();
            let events = given_pension_events();

            let result = when_plan_employment_only(&policy, &events, false)
                .expect("연말정산 계산에 성공해야 한다");

            assert_eq!(
                result.calculation,
                EmploymentTaxCalculation {
                    tax_year: TAX_YEAR,
                    status: EmploymentTaxAssessmentStatus::Definitive,
                    source: EmploymentTaxAssessmentSource::EmploymentOnly,
                    gross_employment_income_krw: 36_000_000,
                    employee_insurance_deduction_krw: 3_000_000,
                    earned_income_deduction_krw: 10_650_000,
                    personal_deduction_krw: 1_500_000,
                    taxable_income_krw: 20_850_000,
                    calculated_income_tax_krw: 1_867_500,
                    earned_income_tax_credit_krw: 716_000,
                    pension_credit_eligible_contribution_krw: 9_000_000,
                    actual_pension_income_tax_credit_krw: 1_151_500,
                    actual_pension_local_income_tax_effect_krw: 115_150,
                    assessed_income_tax_krw: 0,
                    assessed_local_income_tax_krw: 0,
                }
            );
        }

        #[test]
        fn given_고정_fixture_when_원천세를_대사하면_then_환급만_계획한다() {
            let policy = given_policy();
            let events = given_pension_events();

            let result = when_plan_employment_only(&policy, &events, false)
                .expect("연말정산 계산에 성공해야 한다");

            assert_eq!(
                (
                    result.reconciliation.additional_tax_krw,
                    result.reconciliation.refund_krw,
                ),
                (0, 1_100_000)
            );
        }

        #[test]
        fn given_근로소득공제_구간경계_when_계산하면_then_다음구간의_연속금액을_쓴다() {
            let policy = given_policy();
            let rules = create_employment_tax_rules();

            let result = rules
                .plan_employment_only(EmploymentOnlyTaxPlanningInput {
                    authority: EmploymentIncomeAuthority::M3Payroll,
                    tax_year: TAX_YEAR,
                    gross_employment_income_krw: 5_000_000,
                    employee_statutory_insurance: EmployeeStatutoryInsuranceAmounts::default(),
                    personal_deduction_person_count: 1,
                    withheld_income_tax_krw: 0,
                    withheld_local_income_tax_krw: 0,
                    requires_combined_assessment: false,
                    pension_opening_tax_excluded_balances: &[],
                    pension_source_events: &[],
                    policy: &policy,
                })
                .expect("구간 경계 계산에 성공해야 한다");

            assert_eq!(result.calculation.earned_income_deduction_krw, 3_500_000);
        }

        #[test]
        fn given_금융소득_종합과세대상_when_회사계산하면_then_provisional로_보존한다() {
            let policy = given_policy();
            let events = given_pension_events();

            let result = when_plan_employment_only(&policy, &events, true)
                .expect("잠정 연말정산 계산에 성공해야 한다");

            assert_eq!(
                result.calculation.status,
                EmploymentTaxAssessmentStatus::Provisional
            );
            assert_eq!(result.pension_allocation, None);
        }

        #[test]
        fn given_잠정계산_when_combined_handoff를_만들면_then_공제와_최종선납세액을_분리한다() {
            let policy = given_policy();
            let events = given_pension_events();

            let result = when_plan_employment_only(&policy, &events, true)
                .expect("잠정 연말정산 계산에 성공해야 한다");

            assert_eq!(
                result.combined_handoff,
                CombinedEmploymentTaxHandoff {
                    tax_year: TAX_YEAR,
                    gross_employment_income_krw: 36_000_000,
                    earned_income_deduction_krw: 10_650_000,
                    personal_deduction_krw: 1_500_000,
                    employee_insurance_deduction_krw: 3_000_000,
                    employment_taxable_income_krw: 20_850_000,
                    calculated_employment_income_tax_krw: 1_867_500,
                    earned_income_tax_credit_krw: 716_000,
                    final_prepaid_employment_income_tax_krw: 0,
                    final_prepaid_employment_local_income_tax_krw: 0,
                }
            );
        }
    }

    mod context_연금_납입원천_확정 {
        use super::*;

        #[test]
        fn given_오래된_두_납입과_인출_when_replay하면_then_fifo로_소진한다() {
            let policy = given_policy();
            let events = vec![
                given_contribution(
                    1,
                    11,
                    PensionContributionAccountKind::PensionSavings,
                    10,
                    10,
                    100,
                ),
                given_contribution(
                    2,
                    11,
                    PensionContributionAccountKind::PensionSavings,
                    20,
                    20,
                    100,
                ),
                given_withdrawal(11, 30, 30, 150),
            ];

            let result = when_plan_pension_credit(&policy, &[], &events, 100)
                .expect("연금 source replay에 성공해야 한다");

            assert_eq!(
                result
                    .allocations
                    .iter()
                    .map(|allocation| allocation.surviving_tax_excluded_contribution_krw)
                    .collect::<Vec<_>>(),
                vec![0, 50]
            );
        }

        #[test]
        fn given_과거세원층보다_큰_인출_when_replay하면_then_불완전한_history를_거절한다() {
            let policy = given_policy();
            let events = vec![given_withdrawal(11, 10, 10, 1)];

            let result = when_plan_pension_credit(&policy, &[], &events, 100);

            assert_eq!(
                result,
                Err(EmploymentTaxError::PensionWithdrawalExceedsHistory)
            );
        }

        #[test]
        fn given_연초잔액과_당해납입후_인출_when_replay하면_then_연초잔액을_먼저_소진한다() {
            let policy = given_policy();
            let opening = [PensionOpeningTaxExcludedBalance {
                account_id: 11,
                amount_krw: 100,
            }];
            let events = vec![
                given_contribution(
                    1,
                    11,
                    PensionContributionAccountKind::PensionSavings,
                    10,
                    10,
                    100,
                ),
                given_withdrawal(11, 20, 20, 150),
            ];

            let result = when_plan_pension_credit(&policy, &opening, &events, 100)
                .expect("연금 source replay에 성공해야 한다");

            assert_eq!(
                result.allocations[0].surviving_tax_excluded_contribution_krw,
                50
            );
        }

        #[test]
        fn given_irp가_먼저_납입되고_연금저축이_나중에_납입_when_한도를_배분하면_then_연금저축부터_채운다()
         {
            let policy = given_policy();
            let events = vec![
                given_contribution(1, 2, PensionContributionAccountKind::Irp, 1, 1, 9_000_000),
                given_contribution(
                    2,
                    1,
                    PensionContributionAccountKind::PensionSavings,
                    2,
                    2,
                    6_000_000,
                ),
            ];

            let result = when_plan_pension_credit(&policy, &[], &events, 2_000_000)
                .expect("연금 한도 배분에 성공해야 한다");

            assert_eq!(
                result
                    .allocations
                    .iter()
                    .map(|allocation| {
                        (
                            allocation.account_kind,
                            allocation.limit_eligible_contribution_krw,
                        )
                    })
                    .collect::<Vec<_>>(),
                vec![
                    (PensionContributionAccountKind::PensionSavings, 6_000_000),
                    (PensionContributionAccountKind::Irp, 3_000_000),
                ]
            );
        }

        #[test]
        fn given_남은산출세액이_공제예상액보다_작음_when_확정하면_then_원단위_최대납입액을_이분탐색한다()
         {
            let policy = given_policy();
            let events = given_pension_events();

            let result = when_plan_pension_credit(&policy, &[], &events, 1_151_500)
                .expect("실제 공제 한도 탐색에 성공해야 한다");

            assert_eq!(
                (
                    result.credited_contribution_krw,
                    result.income_tax_credit_krw,
                ),
                (7_676_673, 1_151_500)
            );
        }

        #[test]
        fn given_공제액이_0인_소액납입_when_확정하면_then_세원층을_옮기지_않는다() {
            let policy = given_policy();
            let events = vec![given_contribution(
                1,
                1,
                PensionContributionAccountKind::PensionSavings,
                1,
                1,
                1,
            )];

            let result = when_plan_pension_credit(&policy, &[], &events, 100)
                .expect("소액 공제 판정에 성공해야 한다");

            assert_eq!(
                (
                    result.credited_contribution_krw,
                    result.allocations[0].credited_contribution_krw,
                ),
                (0, 0)
            );
            assert_eq!(result.tax_layer_movements().count(), 0);
        }

        #[test]
        fn given_작은원천들이_함께_절사단위를_채움_when_확정하면_then_연간합산공제를_source순서로_배분한다()
         {
            let policy = given_policy();
            let events = vec![
                given_contribution(
                    1,
                    1,
                    PensionContributionAccountKind::PensionSavings,
                    1,
                    1,
                    1,
                ),
                given_contribution(
                    2,
                    1,
                    PensionContributionAccountKind::PensionSavings,
                    2,
                    2,
                    6,
                ),
            ];

            let result = when_plan_pension_credit(&policy, &[], &events, 100)
                .expect("연간 합산 공제 배분에 성공해야 한다");

            assert_eq!(
                result
                    .tax_layer_movements()
                    .map(|allocation| {
                        (
                            allocation.contribution_source_id,
                            allocation.credited_contribution_krw,
                            allocation.income_tax_credit_krw,
                        )
                    })
                    .collect::<Vec<_>>(),
                vec![(1, 1, 0), (2, 6, 1)]
            );
            assert_eq!(
                (
                    result.credited_contribution_krw,
                    result.income_tax_credit_krw,
                ),
                (7, 1)
            );
        }

        #[test]
        fn given_지방세_절사경계의_연금공제_when_확정하면_then_소득세전후를_각각_연동한다() {
            let policy = given_policy();
            let events = vec![given_contribution(
                1,
                1,
                PensionContributionAccountKind::PensionSavings,
                1,
                1,
                7,
            )];

            let result = when_plan_pension_credit(&policy, &[], &events, 101)
                .expect("지방소득세 연동 계산에 성공해야 한다");

            assert_eq!(
                (
                    result.income_tax_credit_krw,
                    result.local_income_tax_effect_krw,
                ),
                (1, 0)
            );
        }

        #[test]
        fn given_세원층_배분_when_확정하면_then_source별_총액을_보존한다() {
            let policy = given_policy();
            let events = given_pension_events();

            let result = when_plan_pension_credit(&policy, &[], &events, 1_151_500)
                .expect("세원층 배분에 성공해야 한다");

            assert_eq!(
                (
                    result.allocations.iter().all(|allocation| {
                        allocation.surviving_tax_excluded_contribution_krw
                            + allocation.credited_contribution_before_krw
                            == allocation.tax_excluded_contribution_after_krw
                                + allocation.credited_contribution_after_krw
                    }),
                    result
                        .allocations
                        .iter()
                        .map(|allocation| allocation.income_tax_credit_krw)
                        .sum::<i64>(),
                    result
                        .allocations
                        .iter()
                        .map(|allocation| allocation.local_income_tax_effect_krw)
                        .sum::<i64>(),
                ),
                (
                    true,
                    result.income_tax_credit_krw,
                    result.local_income_tax_effect_krw,
                )
            );
        }

        #[test]
        fn given_같은_event를_다른순서로_전달_when_replay하면_then_같은allocation에_수렴한다() {
            let policy = given_policy();
            let ordered = vec![
                given_contribution(
                    1,
                    1,
                    PensionContributionAccountKind::PensionSavings,
                    10,
                    10,
                    100,
                ),
                given_contribution(
                    2,
                    1,
                    PensionContributionAccountKind::PensionSavings,
                    20,
                    20,
                    100,
                ),
                given_withdrawal(1, 30, 30, 50),
            ];
            let shuffled = vec![ordered[2], ordered[1], ordered[0]];

            let first = when_plan_pension_credit(&policy, &[], &ordered, 100)
                .expect("첫 replay에 성공해야 한다");
            let replay = when_plan_pension_credit(&policy, &[], &shuffled, 100)
                .expect("재시도 replay에 성공해야 한다");

            assert_eq!(first, replay);
        }
    }

    mod context_combined_확정 {
        use super::*;

        #[test]
        fn given_provisional_handoff와_종합소득_when_combined를_확정하면_then_별도rate와_선납세액을_쓴다()
         {
            let policy = given_policy();
            let events = given_pension_events();
            let provisional = when_plan_employment_only(&policy, &events, true)
                .expect("잠정 계산에 성공해야 한다");
            let rules = create_employment_tax_rules();

            let result = rules
                .plan_combined(CombinedEmploymentTaxPlanningInput {
                    authority: EmploymentIncomeAuthority::M3Payroll,
                    handoff: provisional.combined_handoff,
                    comprehensive_income_krw: 50_000_000,
                    calculated_combined_income_tax_krw: 2_000_000,
                    income_tax_before_pension_credit_krw: 1_000_000,
                    local_income_tax_before_pension_effect_krw: 100_000,
                    total_prepaid_income_tax_krw: provisional
                        .combined_handoff
                        .final_prepaid_employment_income_tax_krw,
                    total_prepaid_local_income_tax_krw: provisional
                        .combined_handoff
                        .final_prepaid_employment_local_income_tax_krw,
                    pension_opening_tax_excluded_balances: &[],
                    pension_source_events: &events,
                    policy: &policy,
                })
                .expect("combined 확정에 성공해야 한다");

            assert_eq!(
                (
                    result.calculation.source,
                    result.calculation.status,
                    result
                        .pension_allocation
                        .expect("확정 allocation이 있어야 한다")
                        .selected_income_tax_rate_ppm,
                ),
                (
                    EmploymentTaxAssessmentSource::Combined,
                    EmploymentTaxAssessmentStatus::Definitive,
                    120_000,
                )
            );
        }
    }

    mod context_정수_안전성 {
        use super::*;

        #[test]
        fn given_bigint범위를_넘는_보험료합계_when_계산하면_then_overflow로_거절한다() {
            let policy = given_policy();
            let rules = create_employment_tax_rules();

            let result = rules.plan_employment_only(EmploymentOnlyTaxPlanningInput {
                authority: EmploymentIncomeAuthority::M3Payroll,
                tax_year: TAX_YEAR,
                gross_employment_income_krw: i64::MAX,
                employee_statutory_insurance: EmployeeStatutoryInsuranceAmounts {
                    national_pension_krw: i64::MAX,
                    health_insurance_krw: i64::MAX,
                    long_term_care_krw: i64::MAX,
                    employment_insurance_krw: i64::MAX,
                },
                personal_deduction_person_count: 1,
                withheld_income_tax_krw: 0,
                withheld_local_income_tax_krw: 0,
                requires_combined_assessment: false,
                pension_opening_tax_excluded_balances: &[],
                pension_source_events: &[],
                policy: &policy,
            });

            assert_eq!(result, Err(EmploymentTaxError::ArithmeticOverflow));
        }

        #[test]
        fn given_bigint범위를_넘는_환급차액_when_대사하면_then_overflow로_거절한다() {
            let rules = create_employment_tax_rules();

            let result = rules.plan_reconciliation(i64::MAX, i64::MAX, 0, 0);

            assert_eq!(result, Err(EmploymentTaxError::ArithmeticOverflow));
        }

        #[test]
        fn given_산출세액이_선납세액보다_큼_when_대사하면_then_추가세액만_계획한다() {
            let rules = create_employment_tax_rules();

            let result = rules
                .plan_reconciliation(100, 10, 200, 20)
                .expect("추가세액 대사에 성공해야 한다");

            assert_eq!((result.additional_tax_krw, result.refund_krw), (110, 0));
        }
    }
}
