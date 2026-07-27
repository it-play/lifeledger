use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;

use time::{Date, Month};

use super::types::{
    ActiveMilitarySavingsContract, MAX_MILITARY_MONEY_KRW, MILITARY_RATE_SCALE_PPM, MilitaryError,
    MilitaryExperienceCredit, MilitaryOptionPolicy, MilitaryPayPeriod, MilitaryPayScheduleInput,
    MilitaryPayStage, MilitaryPayStageInput, MilitaryRules, MilitarySavingsEarlyCloseInput,
    MilitarySavingsEarlyClosePlan, MilitarySavingsEnrollmentInput, MilitarySavingsEnrollmentPlan,
    MilitarySavingsGovernmentMatchLine, MilitarySavingsGovernmentMatchPlan,
    MilitarySavingsInstallmentDraft, MilitarySavingsInstallmentInput,
    MilitarySavingsInstallmentPlan, MilitarySavingsInstallmentStatus, MilitarySavingsInterestLine,
    MilitarySavingsMaturityInput, MilitarySavingsMaturityPlan, MilitarySavingsMovement,
    MilitarySavingsPolicy, MilitarySavingsProductPolicy, MilitaryServiceDayEffect,
    MilitaryServiceDayInput, MilitaryServicePlan, MilitaryServiceStartInput, MilitaryServiceStatus,
    MilitaryServiceTransition, MilitaryServiceTransitionInput, MilitaryServiceType, MilitaryStatus,
    PaidMilitarySavingsInstallment,
};

struct V1MilitaryRules;

pub fn create_military_rules() -> Arc<dyn MilitaryRules> {
    Arc::new(V1MilitaryRules)
}

impl MilitaryStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unserved => "unserved",
            Self::Serving => "serving",
            Self::Completed => "completed",
            Self::Exempt => "exempt",
        }
    }
}

impl FromStr for MilitaryStatus {
    type Err = MilitaryError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "unserved" => Ok(Self::Unserved),
            "serving" => Ok(Self::Serving),
            "completed" => Ok(Self::Completed),
            "exempt" => Ok(Self::Exempt),
            _ => Err(MilitaryError::UnknownStatus),
        }
    }
}

impl MilitaryServiceType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ActiveDuty => "activeDuty",
            Self::SocialService => "socialService",
            Self::IndustrialTechnical => "industrialTechnical",
            Self::ProfessionalResearch => "professionalResearch",
            Self::CommissionedOfficer => "commissionedOfficer",
            Self::NonCommissionedOfficer => "nonCommissionedOfficer",
        }
    }
}

impl FromStr for MilitaryServiceType {
    type Err = MilitaryError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "activeDuty" => Ok(Self::ActiveDuty),
            "socialService" => Ok(Self::SocialService),
            "industrialTechnical" => Ok(Self::IndustrialTechnical),
            "professionalResearch" => Ok(Self::ProfessionalResearch),
            "commissionedOfficer" => Ok(Self::CommissionedOfficer),
            "nonCommissionedOfficer" => Ok(Self::NonCommissionedOfficer),
            _ => Err(MilitaryError::UnknownServiceType),
        }
    }
}

impl MilitaryRules for V1MilitaryRules {
    fn parse_status(&self, value: &str) -> Result<MilitaryStatus, MilitaryError> {
        value.parse()
    }

    fn parse_service_type(&self, value: &str) -> Result<MilitaryServiceType, MilitaryError> {
        value.parse()
    }

    fn validate_option(&self, option: &MilitaryOptionPolicy) -> Result<(), MilitaryError> {
        validate_option(option)
    }

    fn plan_service_start(
        &self,
        input: MilitaryServiceStartInput<'_>,
    ) -> Result<MilitaryServicePlan, MilitaryError> {
        validate_option(input.option)?;
        if input.current_status != MilitaryStatus::Unserved {
            return Err(MilitaryError::MilitaryStateConflict);
        }
        if !input.eligibility.military_subject
            || input
                .option
                .hard_requirements
                .minimum_education
                .is_some_and(|minimum| input.eligibility.education < minimum)
            || input.eligibility.certification_count
                < input.option.hard_requirements.minimum_certification_count
            || input.eligibility.experience_days
                < input.option.hard_requirements.minimum_experience_days
        {
            return Err(MilitaryError::NotEligible);
        }

        let start_game_day = input
            .current_game_day
            .checked_add(1)
            .ok_or(MilitaryError::ArithmeticOverflow)?;
        let start_date = input
            .current_date
            .next_day()
            .ok_or(MilitaryError::InvalidDate)?;
        let end_exclusive_date =
            add_months_clamped(start_date, u32::from(input.option.service_duration_months))?;
        let service_days = positive_days_between(start_date, end_exclusive_date)?;
        let end_game_day = start_game_day
            .checked_add(service_days)
            .ok_or(MilitaryError::ArithmeticOverflow)?;

        Ok(MilitaryServicePlan {
            option_version_id: input.option.option_version_id,
            service_type: input.option.service_type,
            external_status: MilitaryStatus::Serving,
            service_status: MilitaryServiceStatus::PendingStart,
            start_game_day,
            end_game_day,
            start_date,
            end_exclusive_date,
        })
    }

    fn transition_service(
        &self,
        input: MilitaryServiceTransitionInput,
    ) -> Result<MilitaryServiceTransition, MilitaryError> {
        if input.start_game_day >= input.end_game_day
            || !matches!(
                (input.external_status, input.service_status),
                (
                    MilitaryStatus::Serving,
                    MilitaryServiceStatus::PendingStart | MilitaryServiceStatus::Serving
                ) | (MilitaryStatus::Completed, MilitaryServiceStatus::Completed)
            )
            || (input.current_game_day < input.start_game_day
                && input.service_status != MilitaryServiceStatus::PendingStart)
            || (input.current_game_day < input.end_game_day
                && input.service_status == MilitaryServiceStatus::Completed)
        {
            return Err(MilitaryError::MilitaryStateConflict);
        }

        let (external_status, service_status) = if input.current_game_day < input.start_game_day {
            (MilitaryStatus::Serving, MilitaryServiceStatus::PendingStart)
        } else if input.current_game_day < input.end_game_day {
            (MilitaryStatus::Serving, MilitaryServiceStatus::Serving)
        } else {
            (MilitaryStatus::Completed, MilitaryServiceStatus::Completed)
        };

        Ok(MilitaryServiceTransition {
            changed: external_status != input.external_status
                || service_status != input.service_status,
            external_status,
            service_status,
        })
    }

    fn select_pay_stage(
        &self,
        input: MilitaryPayStageInput<'_>,
    ) -> Result<MilitaryPayStage, MilitaryError> {
        validate_option(input.option)?;
        let expected_end = add_months_clamped(
            input.service_start_date,
            u32::from(input.option.service_duration_months),
        )?;
        if input.service_end_exclusive_date != expected_end
            || input.service_date < input.service_start_date
            || input.service_date >= input.service_end_exclusive_date
        {
            return Err(MilitaryError::InvalidServicePeriod);
        }
        let service_month =
            completed_calendar_months(input.service_start_date, input.service_date)?;
        let stage = input
            .option
            .pay_stages
            .iter()
            .find(|stage| {
                stage.start_service_month <= service_month
                    && service_month < stage.end_exclusive_service_month
            })
            .ok_or(MilitaryError::MissingPayStage)?;

        Ok(MilitaryPayStage {
            service_month,
            gross_monthly_pay_krw: stage.gross_monthly_pay_krw,
        })
    }

    fn plan_pay_schedule(
        &self,
        input: MilitaryPayScheduleInput<'_>,
    ) -> Result<Vec<MilitaryPayPeriod>, MilitaryError> {
        validate_option(input.option)?;
        let expected_end = add_months_clamped(
            input.service_start_date,
            u32::from(input.option.service_duration_months),
        )?;
        if input.service_end_exclusive_date != expected_end {
            return Err(MilitaryError::InvalidServicePeriod);
        }

        let current_month_payday = clamped_day_in_month(
            input.service_start_date.year(),
            input.service_start_date.month(),
            input.option.payday_day_of_month,
        )?;
        let mut month_offset = u32::from(current_month_payday < input.service_start_date);
        let mut periods = Vec::new();
        loop {
            let payday = debit_date_with_month_offset(
                input.service_start_date,
                month_offset,
                input.option.payday_day_of_month,
            )?;
            if payday >= input.service_end_exclusive_date {
                break;
            }
            let elapsed_days = u32::try_from((payday - input.service_start_date).whole_days())
                .map_err(|_| MilitaryError::InvalidDate)?;
            let payroll_period = u32::try_from(periods.len())
                .map_err(|_| MilitaryError::ArithmeticOverflow)?
                .checked_add(1)
                .ok_or(MilitaryError::ArithmeticOverflow)?;
            periods.push(MilitaryPayPeriod {
                payroll_period,
                payday,
                pay_game_day: input
                    .service_start_game_day
                    .checked_add(elapsed_days)
                    .ok_or(MilitaryError::ArithmeticOverflow)?,
            });
            month_offset = month_offset
                .checked_add(1)
                .ok_or(MilitaryError::ArithmeticOverflow)?;
        }
        Ok(periods)
    }

    fn plan_service_day(
        &self,
        input: MilitaryServiceDayInput<'_>,
    ) -> Result<MilitaryServiceDayEffect, MilitaryError> {
        validate_option(input.option)?;
        if input.service.option_version_id != input.option.option_version_id
            || input.service.service_type != input.option.service_type
            || input.service.service_status != MilitaryServiceStatus::Serving
            || input.current_game_day < input.service.start_game_day
            || input.current_game_day >= input.service.end_game_day
            || input.service.external_status != MilitaryStatus::Serving
        {
            return Err(MilitaryError::MilitaryStateConflict);
        }

        Ok(MilitaryServiceDayEffect {
            credited_service_days: 1,
            effort_life_status: input.option.effort_life_status,
            available_effort_units: input.option.daily_effort_capacity_units,
            experience: input
                .option
                .experience
                .iter()
                .map(|rule| MilitaryExperienceCredit {
                    job_family_key: rule.job_family_key.clone(),
                    credit_ppm: rule.daily_credit_ppm,
                })
                .collect(),
        })
    }

    fn validate_savings_policy(&self, policy: &MilitarySavingsPolicy) -> Result<(), MilitaryError> {
        validate_savings_policy(policy)
    }

    fn validate_savings_product(
        &self,
        product: &MilitarySavingsProductPolicy,
    ) -> Result<(), MilitaryError> {
        validate_savings_product(product)
    }

    fn minimum_remaining_service_met(
        &self,
        current_date: Date,
        service_end_exclusive_date: Date,
        minimum_remaining_service_months: u16,
    ) -> Result<bool, MilitaryError> {
        if minimum_remaining_service_months == 0 || current_date >= service_end_exclusive_date {
            return Err(MilitaryError::InvalidServicePeriod);
        }
        Ok(
            add_months_clamped(current_date, u32::from(minimum_remaining_service_months))?
                <= service_end_exclusive_date,
        )
    }

    fn plan_savings_enrollment(
        &self,
        input: MilitarySavingsEnrollmentInput<'_>,
    ) -> Result<MilitarySavingsEnrollmentPlan, MilitaryError> {
        validate_savings_policy(input.policy)?;
        validate_savings_product(input.product)?;
        validate_savings_enrollment(&input)?;

        let first_due_date = first_future_debit_date(input.current_date, input.debit_day_of_month)?;
        let mut installments = Vec::new();
        let mut due_date = first_due_date;
        let mut installment_no = 1_u32;
        while due_date < input.service_end_exclusive_date {
            let days_after_current = positive_days_between(input.current_date, due_date)?;
            let due_game_day = input
                .current_game_day
                .checked_add(days_after_current)
                .ok_or(MilitaryError::ArithmeticOverflow)?;
            if due_game_day >= input.service_end_game_day {
                return Err(MilitaryError::InvalidServicePeriod);
            }
            installments.push(MilitarySavingsInstallmentDraft {
                installment_no,
                due_date,
                due_game_day,
            });
            installment_no = installment_no
                .checked_add(1)
                .ok_or(MilitaryError::ArithmeticOverflow)?;
            due_date = debit_date_with_month_offset(
                first_due_date,
                u32::try_from(installments.len()).map_err(|_| MilitaryError::ArithmeticOverflow)?,
                input.debit_day_of_month,
            )?;
        }
        if installments.is_empty() {
            return Err(MilitaryError::NoInstallments);
        }

        let contract_term_months =
            u16::try_from(installments.len()).map_err(|_| MilitaryError::ArithmeticOverflow)?;
        let annual_interest_rate_ppm = input
            .product
            .interest_tiers
            .iter()
            .find(|tier| {
                tier.minimum_term_months <= contract_term_months
                    && contract_term_months <= tier.maximum_term_months_inclusive
            })
            .map(|tier| tier.annual_interest_rate_ppm)
            .ok_or(MilitaryError::InvalidSavingsProduct)?;

        Ok(MilitarySavingsEnrollmentPlan {
            product_version_id: input.product.product_version_id,
            institution_key: input.institution_key.to_owned(),
            monthly_contribution_krw: input.monthly_contribution_krw,
            debit_day_of_month: input.debit_day_of_month,
            contract_term_months,
            annual_interest_rate_ppm,
            maturity_date: input.service_end_exclusive_date,
            maturity_game_day: input.service_end_game_day,
            installments,
        })
    }

    fn settle_savings_installment(
        &self,
        input: MilitarySavingsInstallmentInput,
    ) -> Result<MilitarySavingsInstallmentPlan, MilitaryError> {
        if input.installment_no == 0 || !valid_positive_money(input.contribution_krw) {
            return Err(MilitaryError::InvalidInstallment);
        }
        if !valid_money(input.wallet_cash_krw) {
            return Err(MilitaryError::InvalidMoney);
        }

        if input.wallet_cash_krw < input.contribution_krw {
            return Ok(MilitarySavingsInstallmentPlan {
                installment_no: input.installment_no,
                status: MilitarySavingsInstallmentStatus::Missed,
                movement: MilitarySavingsMovement::NoMovement,
                wallet_cash_delta_krw: 0,
                principal_delta_krw: 0,
            });
        }

        Ok(MilitarySavingsInstallmentPlan {
            installment_no: input.installment_no,
            status: MilitarySavingsInstallmentStatus::Paid,
            movement: MilitarySavingsMovement::PrincipalLocked,
            wallet_cash_delta_krw: input
                .contribution_krw
                .checked_neg()
                .ok_or(MilitaryError::ArithmeticOverflow)?,
            principal_delta_krw: input.contribution_krw,
        })
    }

    fn plan_savings_maturity(
        &self,
        input: MilitarySavingsMaturityInput<'_>,
    ) -> Result<MilitarySavingsMaturityPlan, MilitaryError> {
        if !input.service_completion_confirmed {
            return Err(MilitaryError::ServiceCompletionRequired);
        }
        validate_interest_terms(
            input.annual_interest_rate_ppm,
            input.day_count_denominator,
            input.interest_rounding_unit_krw,
        )?;
        if !(1..=31).contains(&input.government_match_payment_day_of_month) {
            return Err(MilitaryError::InvalidDate);
        }

        let payout = calculate_savings_payout(
            input.paid_installments,
            input.maturity_date,
            input.annual_interest_rate_ppm,
            input.day_count_denominator,
            input.interest_rounding_unit_krw,
            true,
        )?;
        let government_match_due_date = next_month_clamped_day(
            input.maturity_date,
            input.government_match_payment_day_of_month,
        )?;

        Ok(MilitarySavingsMaturityPlan {
            principal_krw: payout.principal_krw,
            gross_bank_interest_krw: payout.gross_interest_krw,
            wallet_credit_krw: checked_money(
                i128::from(payout.principal_krw)
                    .checked_add(i128::from(payout.gross_interest_krw))
                    .ok_or(MilitaryError::ArithmeticOverflow)?,
            )?,
            interest: payout.interest,
            government_match: MilitarySavingsGovernmentMatchPlan {
                due_date: government_match_due_date,
                amount_krw: payout.government_match_krw,
                installments: payout.government_match,
            },
        })
    }

    fn plan_savings_early_close(
        &self,
        input: MilitarySavingsEarlyCloseInput<'_>,
    ) -> Result<MilitarySavingsEarlyClosePlan, MilitaryError> {
        if input.close_date >= input.maturity_date {
            return Err(MilitaryError::InvalidDate);
        }
        validate_interest_terms(
            input.early_close_annual_interest_rate_ppm,
            input.day_count_denominator,
            input.interest_rounding_unit_krw,
        )?;
        let payout = calculate_savings_payout(
            input.paid_installments,
            input.close_date,
            input.early_close_annual_interest_rate_ppm,
            input.day_count_denominator,
            input.interest_rounding_unit_krw,
            false,
        )?;

        Ok(MilitarySavingsEarlyClosePlan {
            principal_krw: payout.principal_krw,
            gross_bank_interest_krw: payout.gross_interest_krw,
            wallet_credit_krw: checked_money(
                i128::from(payout.principal_krw)
                    .checked_add(i128::from(payout.gross_interest_krw))
                    .ok_or(MilitaryError::ArithmeticOverflow)?,
            )?,
            interest: payout.interest,
            government_match_krw: 0,
            tax_exempt: false,
        })
    }
}

fn validate_option(option: &MilitaryOptionPolicy) -> Result<(), MilitaryError> {
    if option.option_version_id == 0 || option.service_duration_months == 0 {
        return Err(MilitaryError::InvalidOption);
    }
    if !(1..=31).contains(&option.payday_day_of_month) {
        return Err(MilitaryError::InvalidPaySchedule);
    }
    let mut expected_start = 0_u16;
    for stage in &option.pay_stages {
        if stage.start_service_month != expected_start
            || stage.start_service_month >= stage.end_exclusive_service_month
            || stage.end_exclusive_service_month > option.service_duration_months
            || !valid_positive_money(stage.gross_monthly_pay_krw)
        {
            return Err(MilitaryError::InvalidPayStages);
        }
        expected_start = stage.end_exclusive_service_month;
    }
    if expected_start != option.service_duration_months {
        return Err(MilitaryError::InvalidPayStages);
    }

    let mut job_families = HashSet::with_capacity(option.experience.len());
    for rule in &option.experience {
        if rule.job_family_key.trim().is_empty()
            || !(1..=MILITARY_RATE_SCALE_PPM).contains(&rule.daily_credit_ppm)
            || !job_families.insert(rule.job_family_key.as_str())
        {
            return Err(MilitaryError::InvalidExperiencePolicy);
        }
    }
    Ok(())
}

fn validate_savings_policy(policy: &MilitarySavingsPolicy) -> Result<(), MilitaryError> {
    let mut service_types = HashSet::with_capacity(policy.eligible_service_types.len());
    if policy.eligible_service_types.is_empty()
        || policy
            .eligible_service_types
            .iter()
            .any(|service_type| !service_types.insert(*service_type))
        || !(1..=600).contains(&policy.minimum_remaining_service_months)
        || policy.maximum_active_contracts == 0
        || policy.maximum_contracts_per_institution == 0
        || policy.maximum_contracts_per_institution > policy.maximum_active_contracts
        || !valid_positive_money(policy.institution_monthly_limit_krw)
        || !valid_positive_money(policy.total_monthly_limit_krw)
        || policy.institution_monthly_limit_krw > policy.total_monthly_limit_krw
        || !valid_positive_money(policy.limit_setting_unit_krw)
        || !valid_positive_money(policy.minimum_installment_krw)
        || !valid_positive_money(policy.installment_unit_krw)
        || !valid_rate(policy.government_matching_rate_ppm)
        || policy.minimum_installment_krw > policy.institution_monthly_limit_krw
        || policy.limit_setting_unit_krw > policy.institution_monthly_limit_krw
        || !(1..=31).contains(&policy.government_match_payment_day_of_month)
    {
        return Err(MilitaryError::InvalidSavingsPolicy);
    }
    Ok(())
}

fn validate_savings_product(product: &MilitarySavingsProductPolicy) -> Result<(), MilitaryError> {
    if product.product_version_id == 0
        || product.institution_key.trim().is_empty()
        || product.interest_tiers.is_empty()
        || product.day_count_denominator == 0
        || !valid_positive_money(product.interest_rounding_unit_krw)
        || !valid_rate(product.early_close_annual_interest_rate_ppm)
    {
        return Err(MilitaryError::InvalidSavingsProduct);
    }
    let mut expected_minimum = 1_u16;
    for tier in &product.interest_tiers {
        if tier.minimum_term_months != expected_minimum
            || tier.minimum_term_months > tier.maximum_term_months_inclusive
            || !valid_rate(tier.annual_interest_rate_ppm)
        {
            return Err(MilitaryError::InvalidSavingsProduct);
        }
        expected_minimum = tier
            .maximum_term_months_inclusive
            .checked_add(1)
            .ok_or(MilitaryError::ArithmeticOverflow)?;
    }
    Ok(())
}

fn validate_savings_enrollment(
    input: &MilitarySavingsEnrollmentInput<'_>,
) -> Result<(), MilitaryError> {
    if input.external_status != MilitaryStatus::Serving
        || !input
            .policy
            .eligible_service_types
            .contains(&input.service_type)
    {
        return Err(MilitaryError::NotEligible);
    }
    if input.institution_key.trim().is_empty()
        || input.product.institution_key != input.institution_key
    {
        return Err(MilitaryError::InvalidSavingsProduct);
    }
    if !(1..=31).contains(&input.debit_day_of_month) {
        return Err(MilitaryError::InvalidDebitDay);
    }
    if input.current_date >= input.service_end_exclusive_date
        || input.current_game_day >= input.service_end_game_day
    {
        return Err(MilitaryError::InvalidServicePeriod);
    }
    let minimum_eligible_end = add_months_clamped(
        input.current_date,
        u32::from(input.policy.minimum_remaining_service_months),
    )?;
    if input.service_end_exclusive_date < minimum_eligible_end {
        return Err(MilitaryError::InsufficientRemainingService);
    }
    let remaining_days =
        positive_days_between(input.current_date, input.service_end_exclusive_date)?;
    if input
        .current_game_day
        .checked_add(remaining_days)
        .ok_or(MilitaryError::ArithmeticOverflow)?
        != input.service_end_game_day
    {
        return Err(MilitaryError::InvalidServicePeriod);
    }
    if input.active_contracts.len() >= usize::from(input.policy.maximum_active_contracts) {
        return Err(MilitaryError::ContractLimitExceeded);
    }
    if input.service_institution_contract_count
        >= u32::from(input.policy.maximum_contracts_per_institution)
    {
        return Err(MilitaryError::InstitutionLimitExceeded);
    }

    let mut existing_total = 0_i128;
    for contract in input.active_contracts {
        validate_existing_contract(contract, input.policy)?;
        existing_total = existing_total
            .checked_add(i128::from(contract.monthly_contribution_krw))
            .ok_or(MilitaryError::ArithmeticOverflow)?;
    }
    if !valid_positive_money(input.monthly_contribution_krw)
        || input.monthly_contribution_krw < input.policy.minimum_installment_krw
        || input.monthly_contribution_krw > input.policy.institution_monthly_limit_krw
        || input.monthly_contribution_krw % input.policy.limit_setting_unit_krw != 0
        || input.monthly_contribution_krw % input.policy.installment_unit_krw != 0
    {
        return Err(MilitaryError::InvalidContribution);
    }
    let combined_total = existing_total
        .checked_add(i128::from(input.monthly_contribution_krw))
        .ok_or(MilitaryError::ArithmeticOverflow)?;
    if combined_total > i128::from(input.policy.total_monthly_limit_krw) {
        return Err(MilitaryError::TotalLimitExceeded);
    }
    Ok(())
}

fn validate_existing_contract(
    contract: &ActiveMilitarySavingsContract,
    policy: &MilitarySavingsPolicy,
) -> Result<(), MilitaryError> {
    if contract.institution_key.trim().is_empty()
        || !valid_positive_money(contract.monthly_contribution_krw)
        || contract.monthly_contribution_krw > policy.institution_monthly_limit_krw
    {
        return Err(MilitaryError::InvalidContribution);
    }
    Ok(())
}

struct SavingsPayout {
    principal_krw: i64,
    gross_interest_krw: i64,
    government_match_krw: i64,
    interest: Vec<MilitarySavingsInterestLine>,
    government_match: Vec<MilitarySavingsGovernmentMatchLine>,
}

fn calculate_savings_payout(
    paid_installments: &[PaidMilitarySavingsInstallment],
    payout_date: Date,
    annual_rate_ppm: i64,
    day_count_denominator: u16,
    interest_rounding_unit_krw: i64,
    include_government_match: bool,
) -> Result<SavingsPayout, MilitaryError> {
    let mut ordered = paid_installments.to_vec();
    ordered.sort_by_key(|installment| installment.installment_no);
    let mut identities = HashSet::with_capacity(ordered.len());
    let mut principal = 0_i128;
    let mut gross_interest = 0_i128;
    let mut government_match_total = 0_i128;
    let mut interest = Vec::with_capacity(ordered.len());
    let mut government_match = Vec::with_capacity(ordered.len());

    for installment in ordered {
        if installment.installment_no == 0 || !identities.insert(installment.installment_no) {
            return Err(if installment.installment_no == 0 {
                MilitaryError::InvalidInstallment
            } else {
                MilitaryError::DuplicateInstallment
            });
        }
        if !valid_positive_money(installment.principal_krw)
            || !valid_rate(installment.government_matching_rate_ppm)
            || installment.paid_date > payout_date
            || (include_government_match && installment.paid_date == payout_date)
        {
            return Err(MilitaryError::InvalidInstallment);
        }
        let held_days = u32::try_from((payout_date - installment.paid_date).whole_days())
            .map_err(|_| MilitaryError::InvalidDate)?;
        let line_interest = simple_interest(
            installment.principal_krw,
            annual_rate_ppm,
            held_days,
            day_count_denominator,
            interest_rounding_unit_krw,
        )?;
        let matching_amount = if include_government_match {
            checked_money(
                i128::from(installment.principal_krw)
                    .checked_mul(i128::from(installment.government_matching_rate_ppm))
                    .and_then(|value| value.checked_div(i128::from(MILITARY_RATE_SCALE_PPM)))
                    .ok_or(MilitaryError::ArithmeticOverflow)?,
            )?
        } else {
            0
        };
        principal = principal
            .checked_add(i128::from(installment.principal_krw))
            .ok_or(MilitaryError::ArithmeticOverflow)?;
        gross_interest = gross_interest
            .checked_add(i128::from(line_interest))
            .ok_or(MilitaryError::ArithmeticOverflow)?;
        government_match_total = government_match_total
            .checked_add(i128::from(matching_amount))
            .ok_or(MilitaryError::ArithmeticOverflow)?;
        interest.push(MilitarySavingsInterestLine {
            installment_no: installment.installment_no,
            principal_krw: installment.principal_krw,
            held_days,
            gross_interest_krw: line_interest,
        });
        if include_government_match {
            government_match.push(MilitarySavingsGovernmentMatchLine {
                installment_no: installment.installment_no,
                principal_krw: installment.principal_krw,
                matching_rate_ppm: installment.government_matching_rate_ppm,
                matching_amount_krw: matching_amount,
            });
        }
    }

    Ok(SavingsPayout {
        principal_krw: checked_money(principal)?,
        gross_interest_krw: checked_money(gross_interest)?,
        government_match_krw: checked_money(government_match_total)?,
        interest,
        government_match,
    })
}

fn simple_interest(
    principal_krw: i64,
    annual_rate_ppm: i64,
    held_days: u32,
    day_count_denominator: u16,
    rounding_unit_krw: i64,
) -> Result<i64, MilitaryError> {
    let numerator = i128::from(principal_krw)
        .checked_mul(i128::from(annual_rate_ppm))
        .and_then(|value| value.checked_mul(i128::from(held_days)))
        .ok_or(MilitaryError::ArithmeticOverflow)?;
    let denominator = i128::from(MILITARY_RATE_SCALE_PPM)
        .checked_mul(i128::from(day_count_denominator))
        .ok_or(MilitaryError::ArithmeticOverflow)?;
    let raw_interest = numerator
        .checked_div(denominator)
        .ok_or(MilitaryError::ArithmeticOverflow)?;
    let rounded = raw_interest
        .checked_div(i128::from(rounding_unit_krw))
        .and_then(|value| value.checked_mul(i128::from(rounding_unit_krw)))
        .ok_or(MilitaryError::ArithmeticOverflow)?;
    checked_money(rounded)
}

fn validate_interest_terms(
    annual_rate_ppm: i64,
    day_count_denominator: u16,
    interest_rounding_unit_krw: i64,
) -> Result<(), MilitaryError> {
    if !valid_rate(annual_rate_ppm) {
        return Err(MilitaryError::InvalidRate);
    }
    if day_count_denominator == 0 || !valid_positive_money(interest_rounding_unit_krw) {
        return Err(MilitaryError::InvalidSavingsProduct);
    }
    Ok(())
}

fn valid_rate(rate_ppm: i64) -> bool {
    (0..=MILITARY_RATE_SCALE_PPM).contains(&rate_ppm)
}

fn valid_money(value: i64) -> bool {
    (0..=MAX_MILITARY_MONEY_KRW).contains(&value)
}

fn valid_positive_money(value: i64) -> bool {
    (1..=MAX_MILITARY_MONEY_KRW).contains(&value)
}

fn checked_money(value: i128) -> Result<i64, MilitaryError> {
    if !(0..=i128::from(MAX_MILITARY_MONEY_KRW)).contains(&value) {
        return Err(MilitaryError::ArithmeticOverflow);
    }
    i64::try_from(value).map_err(|_| MilitaryError::ArithmeticOverflow)
}

fn positive_days_between(start: Date, end_exclusive: Date) -> Result<u32, MilitaryError> {
    let days = (end_exclusive - start).whole_days();
    if days <= 0 {
        return Err(MilitaryError::InvalidDate);
    }
    u32::try_from(days).map_err(|_| MilitaryError::ArithmeticOverflow)
}

fn completed_calendar_months(start: Date, current: Date) -> Result<u16, MilitaryError> {
    if current < start {
        return Err(MilitaryError::InvalidDate);
    }
    let start_month = i64::from(start.year())
        .checked_mul(12)
        .and_then(|value| value.checked_add(i64::from(u8::from(start.month())) - 1))
        .ok_or(MilitaryError::ArithmeticOverflow)?;
    let current_month = i64::from(current.year())
        .checked_mul(12)
        .and_then(|value| value.checked_add(i64::from(u8::from(current.month())) - 1))
        .ok_or(MilitaryError::ArithmeticOverflow)?;
    let mut months = u32::try_from(
        current_month
            .checked_sub(start_month)
            .ok_or(MilitaryError::ArithmeticOverflow)?,
    )
    .map_err(|_| MilitaryError::ArithmeticOverflow)?;
    if add_months_clamped(start, months)? > current {
        months = months
            .checked_sub(1)
            .ok_or(MilitaryError::ArithmeticOverflow)?;
    }
    u16::try_from(months).map_err(|_| MilitaryError::ArithmeticOverflow)
}

fn first_future_debit_date(current: Date, desired_day: u8) -> Result<Date, MilitaryError> {
    if !(1..=31).contains(&desired_day) {
        return Err(MilitaryError::InvalidDebitDay);
    }
    let current_month_due = clamped_day_in_month(current.year(), current.month(), desired_day)?;
    if current_month_due > current {
        Ok(current_month_due)
    } else {
        debit_date_with_month_offset(current, 1, desired_day)
    }
}

fn debit_date_with_month_offset(
    base: Date,
    month_offset: u32,
    desired_day: u8,
) -> Result<Date, MilitaryError> {
    let month_start = Date::from_calendar_date(base.year(), base.month(), 1)
        .map_err(|_| MilitaryError::InvalidDate)?;
    let target_month = add_months_clamped(month_start, month_offset)?;
    clamped_day_in_month(target_month.year(), target_month.month(), desired_day)
}

fn next_month_clamped_day(date: Date, desired_day: u8) -> Result<Date, MilitaryError> {
    debit_date_with_month_offset(date, 1, desired_day)
}

fn clamped_day_in_month(year: i32, month: Month, desired_day: u8) -> Result<Date, MilitaryError> {
    for day in (1..=desired_day).rev() {
        if let Ok(date) = Date::from_calendar_date(year, month, day) {
            return Ok(date);
        }
    }
    Err(MilitaryError::InvalidDate)
}

fn add_months_clamped(date: Date, months: u32) -> Result<Date, MilitaryError> {
    let base_month = i64::from(date.year())
        .checked_mul(12)
        .and_then(|value| value.checked_add(i64::from(u8::from(date.month())) - 1))
        .ok_or(MilitaryError::ArithmeticOverflow)?;
    let target_month = base_month
        .checked_add(i64::from(months))
        .ok_or(MilitaryError::ArithmeticOverflow)?;
    let year =
        i32::try_from(target_month.div_euclid(12)).map_err(|_| MilitaryError::InvalidDate)?;
    let month_number =
        u8::try_from(target_month.rem_euclid(12) + 1).map_err(|_| MilitaryError::InvalidDate)?;
    let month = Month::try_from(month_number).map_err(|_| MilitaryError::InvalidDate)?;
    clamped_day_in_month(year, month, date.day())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::career::types::{
        LifeStatus, MilitaryEligibilityInput, MilitaryExperiencePolicy, MilitaryHardRequirements,
        MilitaryPartialMonthPayKind, MilitaryPayScheduleKind, MilitaryPayStagePolicy,
        MilitarySavingsInterestTier,
    };
    use crate::character::Education;

    fn given_date(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("유효한 테스트 날짜여야 한다")
    }

    fn given_requirements(
        minimum_education: Option<Education>,
        minimum_certification_count: u32,
        minimum_experience_days: u32,
    ) -> MilitaryHardRequirements {
        MilitaryHardRequirements {
            minimum_education,
            minimum_certification_count,
            minimum_experience_days,
        }
    }

    fn given_option(
        service_type: MilitaryServiceType,
        duration_months: u16,
        requirements: MilitaryHardRequirements,
    ) -> MilitaryOptionPolicy {
        MilitaryOptionPolicy {
            option_version_id: 7,
            service_type,
            service_duration_months: duration_months,
            pay_schedule_kind: MilitaryPayScheduleKind::Monthly,
            payday_day_of_month: 10,
            partial_month_pay_kind: MilitaryPartialMonthPayKind::FullMonthlyGross,
            hard_requirements: requirements,
            pay_stages: vec![MilitaryPayStagePolicy {
                start_service_month: 0,
                end_exclusive_service_month: duration_months,
                gross_monthly_pay_krw: 1_000_000,
            }],
            effort_life_status: LifeStatus::SpecialService,
            daily_effort_capacity_units: 4,
            experience: Vec::new(),
        }
    }

    fn given_eligible_candidate() -> MilitaryEligibilityInput {
        MilitaryEligibilityInput {
            military_subject: true,
            education: Education::Master,
            certification_count: 2,
            experience_days: 700,
        }
    }

    fn when_plan_service_start(
        current_date: Date,
        option: &MilitaryOptionPolicy,
        eligibility: MilitaryEligibilityInput,
    ) -> Result<MilitaryServicePlan, MilitaryError> {
        create_military_rules().plan_service_start(MilitaryServiceStartInput {
            current_status: MilitaryStatus::Unserved,
            current_game_day: 10,
            current_date,
            eligibility,
            option,
        })
    }

    fn given_savings_policy() -> MilitarySavingsPolicy {
        MilitarySavingsPolicy {
            eligible_service_types: vec![
                MilitaryServiceType::ActiveDuty,
                MilitaryServiceType::SocialService,
            ],
            minimum_remaining_service_months: 1,
            maximum_active_contracts: 2,
            maximum_contracts_per_institution: 1,
            institution_monthly_limit_krw: 300_000,
            total_monthly_limit_krw: 550_000,
            limit_setting_unit_krw: 50_000,
            minimum_installment_krw: 1_000,
            installment_unit_krw: 1,
            government_matching_rate_ppm: 1_000_000,
            government_match_payment_day_of_month: 25,
        }
    }

    fn given_savings_product(institution_key: &str) -> MilitarySavingsProductPolicy {
        MilitarySavingsProductPolicy {
            product_version_id: 11,
            institution_key: institution_key.to_owned(),
            interest_tiers: vec![MilitarySavingsInterestTier {
                minimum_term_months: 1,
                maximum_term_months_inclusive: 24,
                annual_interest_rate_ppm: 50_000,
            }],
            day_count_denominator: 365,
            interest_rounding_unit_krw: 1,
            early_close_annual_interest_rate_ppm: 0,
        }
    }

    fn when_plan_enrollment(
        current_date: Date,
        service_end: Date,
        debit_day: u8,
        contribution_krw: i64,
        active_contracts: &[ActiveMilitarySavingsContract],
        policy: &MilitarySavingsPolicy,
        product: &MilitarySavingsProductPolicy,
    ) -> Result<MilitarySavingsEnrollmentPlan, MilitaryError> {
        let service_institution_contract_count = u32::try_from(
            active_contracts
                .iter()
                .filter(|contract| contract.institution_key == product.institution_key)
                .count(),
        )
        .expect("테스트 계약 수는 u32 범위여야 한다");
        when_plan_enrollment_with_institution_history(
            current_date,
            service_end,
            debit_day,
            contribution_krw,
            active_contracts,
            service_institution_contract_count,
            policy,
            product,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn when_plan_enrollment_with_institution_history(
        current_date: Date,
        service_end: Date,
        debit_day: u8,
        contribution_krw: i64,
        active_contracts: &[ActiveMilitarySavingsContract],
        service_institution_contract_count: u32,
        policy: &MilitarySavingsPolicy,
        product: &MilitarySavingsProductPolicy,
    ) -> Result<MilitarySavingsEnrollmentPlan, MilitaryError> {
        let current_game_day = 100;
        let remaining_days = u32::try_from((service_end - current_date).whole_days())
            .expect("테스트 복무 종료일은 현재보다 뒤여야 한다");
        create_military_rules().plan_savings_enrollment(MilitarySavingsEnrollmentInput {
            external_status: MilitaryStatus::Serving,
            service_type: MilitaryServiceType::ActiveDuty,
            current_date,
            current_game_day,
            service_end_exclusive_date: service_end,
            service_end_game_day: current_game_day + remaining_days,
            institution_key: product.institution_key.as_str(),
            monthly_contribution_krw: contribution_krw,
            debit_day_of_month: debit_day,
            active_contracts,
            service_institution_contract_count,
            policy,
            product,
        })
    }

    mod context_durable_병역값을_해석하는_경우 {
        use super::*;

        #[test]
        fn given_exact_외부상태_when_parse하면_then_canonical_문자열로_왕복한다() {
            let rules = create_military_rules();

            for value in ["unserved", "serving", "completed", "exempt"] {
                let parsed = rules
                    .parse_status(value)
                    .expect("외부 상태를 해석해야 한다");

                assert_eq!(parsed.as_str(), value);
            }
        }

        #[test]
        fn given_special_service_외부상태_when_parse하면_then_별도상태로_허용하지_않는다() {
            let result = create_military_rules().parse_status("specialService");

            assert_eq!(result, Err(MilitaryError::UnknownStatus));
        }

        #[test]
        fn given_여섯_service_type_when_parse하면_then_canonical_문자열로_왕복한다() {
            let rules = create_military_rules();
            let values = [
                "activeDuty",
                "socialService",
                "industrialTechnical",
                "professionalResearch",
                "commissionedOfficer",
                "nonCommissionedOfficer",
            ];

            for value in values {
                let parsed = rules
                    .parse_service_type(value)
                    .expect("복무 형태를 해석해야 한다");

                assert_eq!(parsed.as_str(), value);
            }
        }
    }

    mod context_복무를_시작하는_경우 {
        use super::*;

        #[test]
        fn given_1월30일과_1개월_option_when_시작하면_then_다음날부터_2월말까지의_exclusive기간을_만든다()
         {
            let option = given_option(
                MilitaryServiceType::ActiveDuty,
                1,
                given_requirements(None, 0, 0),
            );

            let plan = when_plan_service_start(
                given_date(2026, Month::January, 30),
                &option,
                given_eligible_candidate(),
            )
            .expect("복무 시작을 계획해야 한다");

            assert_eq!(plan.start_game_day, 11);
            assert_eq!(plan.start_date, given_date(2026, Month::January, 31));
            assert_eq!(
                plan.end_exclusive_date,
                given_date(2026, Month::February, 28)
            );
            assert_eq!(plan.end_game_day, 39);
            assert_eq!(plan.external_status, MilitaryStatus::Serving);
            assert_eq!(plan.service_status, MilitaryServiceStatus::PendingStart);
        }

        #[test]
        fn given_산업기능_option의_자격증1개요건_when_자격증이_없으면_then_자격없음으로_거절한다() {
            let option = given_option(
                MilitaryServiceType::IndustrialTechnical,
                2,
                given_requirements(None, 1, 0),
            );
            let mut candidate = given_eligible_candidate();
            candidate.certification_count = 0;

            let result =
                when_plan_service_start(given_date(2026, Month::January, 1), &option, candidate);

            assert_eq!(result, Err(MilitaryError::NotEligible));
        }

        #[test]
        fn given_전문연구_option의_석사요건_when_학사이면_then_자격없음으로_거절한다() {
            let option = given_option(
                MilitaryServiceType::ProfessionalResearch,
                2,
                given_requirements(Some(Education::Master), 0, 0),
            );
            let mut candidate = given_eligible_candidate();
            candidate.education = Education::Bachelor;

            let result =
                when_plan_service_start(given_date(2026, Month::January, 1), &option, candidate);

            assert_eq!(result, Err(MilitaryError::NotEligible));
        }

        #[test]
        fn given_이미_serving인_상태_when_새복무를_시작하면_then_상태충돌로_거절한다() {
            let option = given_option(
                MilitaryServiceType::ActiveDuty,
                1,
                given_requirements(None, 0, 0),
            );

            let result = create_military_rules().plan_service_start(MilitaryServiceStartInput {
                current_status: MilitaryStatus::Serving,
                current_game_day: 0,
                current_date: given_date(2026, Month::January, 1),
                eligibility: given_eligible_candidate(),
                option: &option,
            });

            assert_eq!(result, Err(MilitaryError::MilitaryStateConflict));
        }
    }

    mod context_복무_lifecycle을_전이하는_경우 {
        use super::*;

        fn when_transition(
            current_game_day: u32,
            service_status: MilitaryServiceStatus,
        ) -> Result<MilitaryServiceTransition, MilitaryError> {
            create_military_rules().transition_service(MilitaryServiceTransitionInput {
                external_status: MilitaryStatus::Serving,
                service_status,
                current_game_day,
                start_game_day: 10,
                end_game_day: 20,
            })
        }

        #[test]
        fn given_pending_start와_시작일_when_전이하면_then_serving이_된다() {
            let transition = when_transition(10, MilitaryServiceStatus::PendingStart)
                .expect("시작일 전이를 계산해야 한다");

            assert_eq!(transition.external_status, MilitaryStatus::Serving);
            assert_eq!(transition.service_status, MilitaryServiceStatus::Serving);
            assert!(transition.changed);
        }

        #[test]
        fn given_serving과_exclusive_end_day_when_전이하면_then_completed가_된다() {
            let transition = when_transition(20, MilitaryServiceStatus::Serving)
                .expect("전역일 전이를 계산해야 한다");

            assert_eq!(transition.external_status, MilitaryStatus::Completed);
            assert_eq!(transition.service_status, MilitaryServiceStatus::Completed);
            assert!(transition.changed);
        }

        #[test]
        fn given_시작전_serving_service_when_전이하면_then_불가능한_역전상태로_거절한다() {
            let result = when_transition(9, MilitaryServiceStatus::Serving);

            assert_eq!(result, Err(MilitaryError::MilitaryStateConflict));
        }
    }

    mod context_복무월별_급여단계를_고르는_경우 {
        use super::*;

        fn given_staged_option() -> MilitaryOptionPolicy {
            let mut option = given_option(
                MilitaryServiceType::ActiveDuty,
                4,
                given_requirements(None, 0, 0),
            );
            option.pay_stages = vec![
                MilitaryPayStagePolicy {
                    start_service_month: 0,
                    end_exclusive_service_month: 2,
                    gross_monthly_pay_krw: 700_000,
                },
                MilitaryPayStagePolicy {
                    start_service_month: 2,
                    end_exclusive_service_month: 4,
                    gross_monthly_pay_krw: 900_000,
                },
            ];
            option
        }

        #[test]
        fn given_1월31일_시작_when_2월말과_3월말을_조회하면_then_완료한_달력월_경계로_stage를_고른다()
         {
            let option = given_staged_option();
            let rules = create_military_rules();
            let start = given_date(2026, Month::January, 31);
            let end = given_date(2026, Month::May, 31);

            let first = rules
                .select_pay_stage(MilitaryPayStageInput {
                    service_date: given_date(2026, Month::February, 28),
                    service_start_date: start,
                    service_end_exclusive_date: end,
                    option: &option,
                })
                .expect("첫 급여 단계를 골라야 한다");
            let second = rules
                .select_pay_stage(MilitaryPayStageInput {
                    service_date: given_date(2026, Month::March, 31),
                    service_start_date: start,
                    service_end_exclusive_date: end,
                    option: &option,
                })
                .expect("두 번째 급여 단계를 골라야 한다");

            assert_eq!(
                (first.service_month, first.gross_monthly_pay_krw),
                (1, 700_000)
            );
            assert_eq!(
                (second.service_month, second.gross_monthly_pay_krw),
                (2, 900_000)
            );
        }

        #[test]
        fn given_gap이_있는_stage_when_policy를_검증하면_then_거절한다() {
            let mut option = given_staged_option();
            option.pay_stages[1].start_service_month = 3;

            let result = create_military_rules().validate_option(&option);

            assert_eq!(result, Err(MilitaryError::InvalidPayStages));
        }
    }

    mod context_월별_군급여_지급일을_예약하는_경우 {
        use super::*;

        fn when_schedule(
            start_game_day: u32,
            start_date: Date,
            end_exclusive_date: Date,
            option: &MilitaryOptionPolicy,
        ) -> Result<Vec<MilitaryPayPeriod>, MilitaryError> {
            create_military_rules().plan_pay_schedule(MilitaryPayScheduleInput {
                service_start_game_day: start_game_day,
                service_start_date: start_date,
                service_end_exclusive_date: end_exclusive_date,
                option,
            })
        }

        #[test]
        fn given_지급일에_복무를_시작_when_예약하면_then_시작일을_첫회차로_포함하고_end는_제외한다()
        {
            let option = given_option(
                MilitaryServiceType::ActiveDuty,
                2,
                given_requirements(None, 0, 0),
            );

            let periods = when_schedule(
                40,
                given_date(2026, Month::January, 10),
                given_date(2026, Month::March, 10),
                &option,
            )
            .expect("월별 군 급여를 예약해야 한다");

            assert_eq!(
                periods
                    .iter()
                    .map(|period| (period.payroll_period, period.payday, period.pay_game_day))
                    .collect::<Vec<_>>(),
                vec![
                    (1, given_date(2026, Month::January, 10), 40),
                    (2, given_date(2026, Month::February, 10), 71),
                ]
            );
        }

        #[test]
        fn given_31일_지급_when_예약하면_then_없는_날은_각달_말일로_보정한다() {
            let mut option = given_option(
                MilitaryServiceType::ActiveDuty,
                3,
                given_requirements(None, 0, 0),
            );
            option.payday_day_of_month = 31;

            let periods = when_schedule(
                10,
                given_date(2026, Month::January, 31),
                given_date(2026, Month::April, 30),
                &option,
            )
            .expect("말일 보정 급여를 예약해야 한다");

            assert_eq!(
                periods
                    .iter()
                    .map(|period| period.payday)
                    .collect::<Vec<_>>(),
                vec![
                    given_date(2026, Month::January, 31),
                    given_date(2026, Month::February, 28),
                    given_date(2026, Month::March, 31),
                ]
            );
        }

        #[test]
        fn given_이번달_지급일이_지난_복무_when_예약하면_then_다음달부터_전액회차를_만든다() {
            let option = given_option(
                MilitaryServiceType::SocialService,
                1,
                given_requirements(None, 0, 0),
            );

            let periods = when_schedule(
                100,
                given_date(2026, Month::January, 11),
                given_date(2026, Month::February, 11),
                &option,
            )
            .expect("다음 달 지급일부터 예약해야 한다");

            assert_eq!(periods.len(), 1);
            assert_eq!(periods[0].payday, given_date(2026, Month::February, 10));
            assert_eq!(periods[0].pay_game_day, 130);
        }

        #[test]
        fn given_범위를_벗어난_지급일_when_policy를_검증하면_then_거절한다() {
            let mut option = given_option(
                MilitaryServiceType::ActiveDuty,
                1,
                given_requirements(None, 0, 0),
            );
            option.payday_day_of_month = 0;

            let result = create_military_rules().validate_option(&option);

            assert_eq!(result, Err(MilitaryError::InvalidPaySchedule));
        }
    }

    mod context_복무일의_effort와_경력을_계산하는_경우 {
        use super::*;

        #[test]
        fn given_경력인정형_serving_service_when_하루를_계획하면_then_policy비율과_effort를_그대로_낸다()
         {
            let mut option = given_option(
                MilitaryServiceType::CommissionedOfficer,
                1,
                given_requirements(None, 0, 0),
            );
            option.daily_effort_capacity_units = 3;
            option.effort_life_status = LifeStatus::OfficerOrNco;
            option.experience = vec![MilitaryExperiencePolicy {
                job_family_key: "publicAdministration".to_owned(),
                daily_credit_ppm: 500_000,
            }];
            let service = MilitaryServicePlan {
                option_version_id: option.option_version_id,
                service_type: option.service_type,
                external_status: MilitaryStatus::Serving,
                service_status: MilitaryServiceStatus::Serving,
                start_game_day: 10,
                end_game_day: 40,
                start_date: given_date(2026, Month::January, 1),
                end_exclusive_date: given_date(2026, Month::February, 1),
            };

            let effect = create_military_rules()
                .plan_service_day(MilitaryServiceDayInput {
                    current_game_day: 10,
                    service,
                    option: &option,
                })
                .expect("복무일 효과를 계산해야 한다");

            assert_eq!(effect.credited_service_days, 1);
            assert_eq!(effect.effort_life_status, LifeStatus::OfficerOrNco);
            assert_eq!(effect.available_effort_units, 3);
            assert_eq!(effect.experience[0].credit_ppm, 500_000);
        }

        #[test]
        fn given_exclusive_end_day_when_하루를_계획하면_then_복무일로_인정하지_않는다() {
            let option = given_option(
                MilitaryServiceType::ActiveDuty,
                1,
                given_requirements(None, 0, 0),
            );
            let service = MilitaryServicePlan {
                option_version_id: option.option_version_id,
                service_type: option.service_type,
                external_status: MilitaryStatus::Serving,
                service_status: MilitaryServiceStatus::Serving,
                start_game_day: 10,
                end_game_day: 20,
                start_date: given_date(2026, Month::January, 1),
                end_exclusive_date: given_date(2026, Month::February, 1),
            };

            let result = create_military_rules().plan_service_day(MilitaryServiceDayInput {
                current_game_day: 20,
                service,
                option: &option,
            });

            assert_eq!(result, Err(MilitaryError::MilitaryStateConflict));
        }
    }

    mod context_장병적금에_가입하는_경우 {
        use super::*;

        #[test]
        fn given_월말보정한_최소잔여1개월_when_가입하면_then_잔여복무요건을_충족한다() {
            let policy = given_savings_policy();
            let product = given_savings_product("life-bank-a");

            let plan = when_plan_enrollment(
                given_date(2026, Month::January, 31),
                given_date(2026, Month::February, 28),
                27,
                300_000,
                &[],
                &policy,
                &product,
            )
            .expect("달력월 기준 최소 잔여복무를 충족해야 한다");

            assert_eq!(plan.installments.len(), 1);
        }

        #[test]
        fn given_1월31일에_31일_debit_when_가입하면_then_2월말부터_각달의_말일을_회차로_만든다() {
            let policy = given_savings_policy();
            let product = given_savings_product("life-bank-a");

            let plan = when_plan_enrollment(
                given_date(2026, Month::January, 31),
                given_date(2026, Month::April, 1),
                31,
                300_000,
                &[],
                &policy,
                &product,
            )
            .expect("장병적금 가입을 계획해야 한다");

            assert_eq!(
                plan.installments
                    .iter()
                    .map(|installment| installment.due_date)
                    .collect::<Vec<_>>(),
                vec![
                    given_date(2026, Month::February, 28),
                    given_date(2026, Month::March, 31)
                ]
            );
            assert_eq!(plan.contract_term_months, 2);
        }

        #[test]
        fn given_명령일보다_뒤인_이번달_debit_when_가입하면_then_이번달을_첫회차로_쓴다() {
            let policy = given_savings_policy();
            let product = given_savings_product("life-bank-a");

            let plan = when_plan_enrollment(
                given_date(2026, Month::January, 10),
                given_date(2026, Month::March, 1),
                25,
                300_000,
                &[],
                &policy,
                &product,
            )
            .expect("이번 달 첫 회차를 계획해야 한다");

            assert_eq!(
                plan.installments[0].due_date,
                given_date(2026, Month::January, 25)
            );
            assert!(
                plan.installments
                    .iter()
                    .all(|item| item.due_date < plan.maturity_date)
            );
        }

        #[test]
        fn given_같은기관의_active계약_when_가입하면_then_기관계좌상한으로_거절한다() {
            let policy = given_savings_policy();
            let product = given_savings_product("life-bank-a");
            let active = vec![ActiveMilitarySavingsContract {
                institution_key: "life-bank-a".to_owned(),
                monthly_contribution_krw: 250_000,
            }];

            let result = when_plan_enrollment(
                given_date(2026, Month::January, 1),
                given_date(2026, Month::May, 1),
                25,
                300_000,
                &active,
                &policy,
                &product,
            );

            assert_eq!(result, Err(MilitaryError::InstitutionLimitExceeded));
        }

        #[test]
        fn given_같은기관의_중도해지계약이력_when_가입하면_then_기관계좌상한으로_거절한다() {
            let policy = given_savings_policy();
            let product = given_savings_product("life-bank-a");

            let result = when_plan_enrollment_with_institution_history(
                given_date(2026, Month::January, 1),
                given_date(2026, Month::May, 1),
                25,
                300_000,
                &[],
                1,
                &policy,
                &product,
            );

            assert_eq!(result, Err(MilitaryError::InstitutionLimitExceeded));
        }

        #[test]
        fn given_다른기관_30만원계약_when_추가30만원으로_가입하면_then_개인합산상한으로_거절한다() {
            let policy = given_savings_policy();
            let product = given_savings_product("life-bank-a");
            let active = vec![ActiveMilitarySavingsContract {
                institution_key: "life-bank-b".to_owned(),
                monthly_contribution_krw: 300_000,
            }];

            let result = when_plan_enrollment(
                given_date(2026, Month::January, 1),
                given_date(2026, Month::May, 1),
                25,
                300_000,
                &active,
                &policy,
                &product,
            );

            assert_eq!(result, Err(MilitaryError::TotalLimitExceeded));
        }

        #[test]
        fn given_한도설정단위에_맞지않는_납입액_when_가입하면_then_금액을_거절한다() {
            let policy = given_savings_policy();
            let product = given_savings_product("life-bank-a");

            let result = when_plan_enrollment(
                given_date(2026, Month::January, 1),
                given_date(2026, Month::May, 1),
                25,
                275_000,
                &[],
                &policy,
                &product,
            );

            assert_eq!(result, Err(MilitaryError::InvalidContribution));
        }

        #[test]
        fn given_최소잔여개월보다_짧은_service_when_가입하면_then_잔여복무요건으로_거절한다() {
            let policy = given_savings_policy();
            let product = given_savings_product("life-bank-a");

            let result = when_plan_enrollment(
                given_date(2026, Month::January, 1),
                given_date(2026, Month::January, 20),
                10,
                300_000,
                &[],
                &policy,
                &product,
            );

            assert_eq!(result, Err(MilitaryError::InsufficientRemainingService));
        }
    }

    mod context_장병적금_회차를_정산하는_경우 {
        use super::*;

        #[test]
        fn given_지갑이_충분한_회차_when_정산하면_then_원금으로_이동한다() {
            let plan = create_military_rules()
                .settle_savings_installment(MilitarySavingsInstallmentInput {
                    installment_no: 1,
                    contribution_krw: 300_000,
                    wallet_cash_krw: 300_000,
                })
                .expect("납입 회차를 정산해야 한다");

            assert_eq!(plan.status, MilitarySavingsInstallmentStatus::Paid);
            assert_eq!(plan.movement, MilitarySavingsMovement::PrincipalLocked);
            assert_eq!(plan.wallet_cash_delta_krw, -300_000);
            assert_eq!(plan.principal_delta_krw, 300_000);
        }

        #[test]
        fn given_지갑이_부족한_회차_when_정산하면_then_missed_no_movement로_확정한다() {
            let plan = create_military_rules()
                .settle_savings_installment(MilitarySavingsInstallmentInput {
                    installment_no: 2,
                    contribution_krw: 300_000,
                    wallet_cash_krw: 299_999,
                })
                .expect("부족한 납입 회차를 확정해야 한다");

            assert_eq!(plan.status, MilitarySavingsInstallmentStatus::Missed);
            assert_eq!(plan.movement, MilitarySavingsMovement::NoMovement);
            assert_eq!(
                (plan.wallet_cash_delta_krw, plan.principal_delta_krw),
                (0, 0)
            );
        }
    }

    mod context_장병적금_만기를_정산하는_경우 {
        use super::*;

        #[test]
        fn given_actual365_납입과_100퍼센트매칭_when_만기하면_then_은행지급과_다음달정부지원을_분리한다()
         {
            let installments = vec![PaidMilitarySavingsInstallment {
                installment_no: 1,
                paid_date: given_date(2026, Month::January, 31),
                principal_krw: 365_000,
                government_matching_rate_ppm: 1_000_000,
            }];

            let plan = create_military_rules()
                .plan_savings_maturity(MilitarySavingsMaturityInput {
                    maturity_date: given_date(2027, Month::January, 31),
                    service_completion_confirmed: true,
                    annual_interest_rate_ppm: 100_000,
                    day_count_denominator: 365,
                    interest_rounding_unit_krw: 1,
                    government_match_payment_day_of_month: 25,
                    paid_installments: &installments,
                })
                .expect("만기 지급을 계산해야 한다");

            assert_eq!(plan.principal_krw, 365_000);
            assert_eq!(plan.gross_bank_interest_krw, 36_500);
            assert_eq!(plan.wallet_credit_krw, 401_500);
            assert_eq!(plan.government_match.amount_krw, 365_000);
            assert_eq!(
                plan.government_match.due_date,
                given_date(2027, Month::February, 25)
            );
        }

        #[test]
        fn given_모든회차가_missed인_계약_when_만기하면_then_0원지급계획도_결정적으로_만든다() {
            let plan = create_military_rules()
                .plan_savings_maturity(MilitarySavingsMaturityInput {
                    maturity_date: given_date(2027, Month::January, 31),
                    service_completion_confirmed: true,
                    annual_interest_rate_ppm: 50_000,
                    day_count_denominator: 365,
                    interest_rounding_unit_krw: 1,
                    government_match_payment_day_of_month: 25,
                    paid_installments: &[],
                })
                .expect("0원 만기도 확정해야 한다");

            assert_eq!((plan.principal_krw, plan.gross_bank_interest_krw), (0, 0));
            assert_eq!(plan.government_match.amount_krw, 0);
        }

        #[test]
        fn given_복무완료가_확정되지않은_만기_when_정산하면_then_선행상태를_요구한다() {
            let result =
                create_military_rules().plan_savings_maturity(MilitarySavingsMaturityInput {
                    maturity_date: given_date(2027, Month::January, 1),
                    service_completion_confirmed: false,
                    annual_interest_rate_ppm: 50_000,
                    day_count_denominator: 365,
                    interest_rounding_unit_krw: 1,
                    government_match_payment_day_of_month: 25,
                    paid_installments: &[],
                });

            assert_eq!(result, Err(MilitaryError::ServiceCompletionRequired));
        }

        #[test]
        fn given_중복_installment_identity_when_만기하면_then_중복으로_거절한다() {
            let installments = vec![
                PaidMilitarySavingsInstallment {
                    installment_no: 1,
                    paid_date: given_date(2026, Month::January, 1),
                    principal_krw: 100_000,
                    government_matching_rate_ppm: 1_000_000,
                },
                PaidMilitarySavingsInstallment {
                    installment_no: 1,
                    paid_date: given_date(2026, Month::February, 1),
                    principal_krw: 100_000,
                    government_matching_rate_ppm: 1_000_000,
                },
            ];

            let result =
                create_military_rules().plan_savings_maturity(MilitarySavingsMaturityInput {
                    maturity_date: given_date(2027, Month::January, 1),
                    service_completion_confirmed: true,
                    annual_interest_rate_ppm: 50_000,
                    day_count_denominator: 365,
                    interest_rounding_unit_krw: 1,
                    government_match_payment_day_of_month: 25,
                    paid_installments: &installments,
                });

            assert_eq!(result, Err(MilitaryError::DuplicateInstallment));
        }

        #[test]
        fn given_safe_integer를_넘는_원금합_when_만기하면_then_overflow로_거절한다() {
            let installments = vec![
                PaidMilitarySavingsInstallment {
                    installment_no: 1,
                    paid_date: given_date(2026, Month::January, 1),
                    principal_krw: MAX_MILITARY_MONEY_KRW,
                    government_matching_rate_ppm: 0,
                },
                PaidMilitarySavingsInstallment {
                    installment_no: 2,
                    paid_date: given_date(2026, Month::February, 1),
                    principal_krw: 1,
                    government_matching_rate_ppm: 0,
                },
            ];

            let result =
                create_military_rules().plan_savings_maturity(MilitarySavingsMaturityInput {
                    maturity_date: given_date(2027, Month::January, 1),
                    service_completion_confirmed: true,
                    annual_interest_rate_ppm: 0,
                    day_count_denominator: 365,
                    interest_rounding_unit_krw: 1,
                    government_match_payment_day_of_month: 25,
                    paid_installments: &installments,
                });

            assert_eq!(result, Err(MilitaryError::ArithmeticOverflow));
        }
    }

    mod context_장병적금을_중도해지하는_경우 {
        use super::*;

        #[test]
        fn given_0퍼센트_중도해지율_when_만기전에_해지하면_then_원금만_주고_지원과_비과세를_주지않는다()
         {
            let installments = vec![PaidMilitarySavingsInstallment {
                installment_no: 1,
                paid_date: given_date(2026, Month::January, 1),
                principal_krw: 300_000,
                government_matching_rate_ppm: 1_000_000,
            }];

            let plan = create_military_rules()
                .plan_savings_early_close(MilitarySavingsEarlyCloseInput {
                    close_date: given_date(2026, Month::June, 1),
                    maturity_date: given_date(2027, Month::January, 1),
                    early_close_annual_interest_rate_ppm: 0,
                    day_count_denominator: 365,
                    interest_rounding_unit_krw: 1,
                    paid_installments: &installments,
                })
                .expect("중도해지 지급을 계산해야 한다");

            assert_eq!(plan.wallet_credit_krw, 300_000);
            assert_eq!(plan.gross_bank_interest_krw, 0);
            assert_eq!(plan.government_match_krw, 0);
            assert!(!plan.tax_exempt);
        }

        #[test]
        fn given_만기일_when_중도해지를_요청하면_then_중도해지로_처리하지_않는다() {
            let date = given_date(2027, Month::January, 1);

            let result =
                create_military_rules().plan_savings_early_close(MilitarySavingsEarlyCloseInput {
                    close_date: date,
                    maturity_date: date,
                    early_close_annual_interest_rate_ppm: 0,
                    day_count_denominator: 365,
                    interest_rounding_unit_krw: 1,
                    paid_installments: &[],
                });

            assert_eq!(result, Err(MilitaryError::InvalidDate));
        }
    }
}
