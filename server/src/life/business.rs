use std::collections::BTreeSet;
use std::sync::Arc;

use super::types::*;

struct V1BusinessOperationsRules;

pub fn create_business_operations_rules() -> Arc<dyn BusinessOperationsRules> {
    Arc::new(V1BusinessOperationsRules)
}

impl BusinessOperationsRules for V1BusinessOperationsRules {
    fn plan_month(
        &self,
        input: BusinessMonthInput<'_>,
    ) -> Result<BusinessMonthPlan, BusinessOperationsError> {
        plan_month(input)
    }
}

fn plan_month(input: BusinessMonthInput<'_>) -> Result<BusinessMonthPlan, BusinessOperationsError> {
    if !(1..=1_000).contains(&input.owner_capacity_units) {
        return Err(BusinessOperationsError::InvalidCapacity);
    }
    validate_money(input.marketing_cost_krw)?;
    validate_money(input.cash_buffer_krw)?;

    let mut employee_ids = BTreeSet::new();
    let mut employee_capacity_units = 0_u32;
    let mut employee_gross_wage_krw = 0_i64;
    let mut employee_employer_cost_krw = 0_i64;
    for employee in input.employees {
        if employee.position_id.get() == 0 || !employee_ids.insert(employee.position_id.get()) {
            return Err(BusinessOperationsError::DuplicateIdentity);
        }
        if !(1..=1_000).contains(&employee.capacity_units)
            || employee.gross_wage_krw <= 0
            || employee.gross_wage_krw > CORPORATION_MAX_PUBLIC_MONEY_KRW
            || employee.employer_cost_rate_ppm > 1_000_000
        {
            return Err(BusinessOperationsError::InvalidEmployee);
        }
        employee_capacity_units = employee_capacity_units
            .checked_add(u32::from(employee.capacity_units))
            .ok_or(BusinessOperationsError::ArithmeticOverflow)?;
        employee_gross_wage_krw =
            checked_money_add(employee_gross_wage_krw, employee.gross_wage_krw)?;
        employee_employer_cost_krw = checked_money_add(
            employee_employer_cost_krw,
            checked_rate_amount(employee.gross_wage_krw, employee.employer_cost_rate_ppm)?,
        )?;
    }
    let active_employee_count = u16::try_from(input.employees.len())
        .map_err(|_| BusinessOperationsError::ArithmeticOverflow)?;
    let total_capacity_units = u32::from(input.owner_capacity_units)
        .checked_add(employee_capacity_units)
        .ok_or(BusinessOperationsError::ArithmeticOverflow)?;

    let mut contract_ids = BTreeSet::new();
    let mut priorities = BTreeSet::new();
    let mut contracts = input.contracts.to_vec();
    for contract in &contracts {
        if contract.contract_id.get() == 0
            || !contract_ids.insert(contract.contract_id.get())
            || !priorities.insert(contract.priority_rank)
        {
            return Err(BusinessOperationsError::DuplicateIdentity);
        }
        if !(1..=1_000).contains(&contract.required_capacity_units)
            || !(1..=CORPORATION_MAX_PUBLIC_MONEY_KRW).contains(&contract.revenue_krw)
            || contract.variable_cost_ppm > 1_000_000
            || !(0..=CORPORATION_MAX_PUBLIC_MONEY_KRW).contains(&contract.failure_penalty_krw)
        {
            return Err(BusinessOperationsError::InvalidContract);
        }
    }
    contracts.sort_by_key(|contract| (contract.priority_rank, contract.contract_id.get()));

    let mut used_capacity_units = 0_u32;
    let mut contract_revenue_krw = 0_i64;
    let mut contract_variable_cost_krw = 0_i64;
    let mut failed_contract_penalty_krw = 0_i64;
    let mut completed_contract_count = 0_u16;
    let mut failed_contract_count = 0_u16;
    let mut contract_plans = Vec::with_capacity(contracts.len());
    for contract in contracts {
        let required_capacity_units = u32::from(contract.required_capacity_units);
        let can_complete = used_capacity_units
            .checked_add(required_capacity_units)
            .is_some_and(|required| required <= total_capacity_units);
        if can_complete {
            let variable_cost_krw =
                checked_rate_amount(contract.revenue_krw, contract.variable_cost_ppm)?;
            used_capacity_units = used_capacity_units
                .checked_add(required_capacity_units)
                .ok_or(BusinessOperationsError::ArithmeticOverflow)?;
            contract_revenue_krw = checked_money_add(contract_revenue_krw, contract.revenue_krw)?;
            contract_variable_cost_krw =
                checked_money_add(contract_variable_cost_krw, variable_cost_krw)?;
            completed_contract_count = completed_contract_count
                .checked_add(1)
                .ok_or(BusinessOperationsError::ArithmeticOverflow)?;
            contract_plans.push(BusinessContractMonthPlan {
                contract_id: contract.contract_id,
                outcome: BusinessContractMonthOutcome::Completed,
                used_capacity_units: contract.required_capacity_units,
                recognized_revenue_krw: contract.revenue_krw,
                variable_cost_krw,
                failure_penalty_krw: 0,
            });
        } else {
            failed_contract_penalty_krw =
                checked_money_add(failed_contract_penalty_krw, contract.failure_penalty_krw)?;
            failed_contract_count = failed_contract_count
                .checked_add(1)
                .ok_or(BusinessOperationsError::ArithmeticOverflow)?;
            contract_plans.push(BusinessContractMonthPlan {
                contract_id: contract.contract_id,
                outcome: BusinessContractMonthOutcome::Failed,
                used_capacity_units: 0,
                recognized_revenue_krw: 0,
                variable_cost_krw: 0,
                failure_penalty_krw: contract.failure_penalty_krw,
            });
        }
    }

    Ok(BusinessMonthPlan {
        owner_capacity_units: input.owner_capacity_units,
        employee_capacity_units,
        total_capacity_units,
        used_capacity_units,
        marketing_cost_krw: input.marketing_cost_krw,
        employee_gross_wage_krw,
        employee_employer_cost_krw,
        contract_revenue_krw,
        contract_variable_cost_krw,
        failed_contract_penalty_krw,
        receivable_created_krw: contract_revenue_krw,
        receivable_collected_krw: contract_revenue_krw,
        completed_contract_count,
        failed_contract_count,
        active_employee_count,
        cash_buffer_krw: input.cash_buffer_krw,
        contract_plans,
    })
}

fn validate_money(amount_krw: i64) -> Result<(), BusinessOperationsError> {
    if (0..=CORPORATION_MAX_PUBLIC_MONEY_KRW).contains(&amount_krw) {
        Ok(())
    } else {
        Err(BusinessOperationsError::InvalidMoney)
    }
}

fn checked_money_add(left: i64, right: i64) -> Result<i64, BusinessOperationsError> {
    let result = left
        .checked_add(right)
        .ok_or(BusinessOperationsError::ArithmeticOverflow)?;
    validate_money(result)?;
    Ok(result)
}

fn checked_rate_amount(amount_krw: i64, rate_ppm: u32) -> Result<i64, BusinessOperationsError> {
    i64::try_from(
        i128::from(amount_krw)
            .checked_mul(i128::from(rate_ppm))
            .and_then(|amount| amount.checked_div(i128::from(CORPORATION_RATIO_SCALE_PPM)))
            .ok_or(BusinessOperationsError::ArithmeticOverflow)?,
    )
    .map_err(|_| BusinessOperationsError::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finance::ResourceId;

    fn given_contract(
        id: u64,
        priority_rank: u16,
        required_capacity_units: u16,
    ) -> BusinessContractMonthInput {
        BusinessContractMonthInput {
            contract_id: ResourceId::from_u64(id),
            priority_rank,
            required_capacity_units,
            revenue_krw: 6_000_000,
            variable_cost_ppm: 100_000,
            failure_penalty_krw: 500_000,
        }
    }

    fn given_employee() -> BusinessEmployeeMonthInput {
        BusinessEmployeeMonthInput {
            position_id: ResourceId::from_u64(1),
            capacity_units: 3,
            gross_wage_krw: 3_500_000,
            employer_cost_rate_ppm: 110_000,
        }
    }

    mod context_대표와_직원_capacity로_계약을_처리하는_경우 {
        use super::*;

        #[test]
        fn given_우선순위가_다른_두계약_when_capacity가_하나만_충분하면_then_앞계약만_완료한다() {
            let contracts = [given_contract(2, 2, 4), given_contract(1, 1, 4)];
            let employees = [given_employee()];

            let result = create_business_operations_rules()
                .plan_month(BusinessMonthInput {
                    owner_capacity_units: 2,
                    marketing_cost_krw: 500_000,
                    cash_buffer_krw: 1_000_000,
                    contracts: &contracts,
                    employees: &employees,
                })
                .expect("월 운영계획이 유효해야 한다");

            assert_eq!(result.total_capacity_units, 5);
            assert_eq!(result.used_capacity_units, 4);
            assert_eq!(result.completed_contract_count, 1);
            assert_eq!(result.failed_contract_count, 1);
            assert_eq!(result.contract_plans[0].contract_id.get(), 1);
            assert_eq!(
                result.contract_plans[0].outcome,
                BusinessContractMonthOutcome::Completed
            );
            assert_eq!(
                result.contract_plans[1].outcome,
                BusinessContractMonthOutcome::Failed
            );
        }
    }

    mod context_직원비와_계약비용을_정산하는_경우 {
        use super::*;

        #[test]
        fn given_직원한명과_완료계약_when_월계획하면_then_원단위_비용과_수금을_대조한다() {
            let contracts = [given_contract(1, 1, 4)];
            let employees = [given_employee()];

            let result = create_business_operations_rules()
                .plan_month(BusinessMonthInput {
                    owner_capacity_units: 2,
                    marketing_cost_krw: 500_000,
                    cash_buffer_krw: 1_000_000,
                    contracts: &contracts,
                    employees: &employees,
                })
                .expect("월 운영계획이 유효해야 한다");

            assert_eq!(result.contract_revenue_krw, 6_000_000);
            assert_eq!(result.contract_variable_cost_krw, 600_000);
            assert_eq!(result.employee_gross_wage_krw, 3_500_000);
            assert_eq!(result.employee_employer_cost_krw, 385_000);
            assert_eq!(
                result.receivable_created_krw,
                result.receivable_collected_krw
            );
        }
    }
}
