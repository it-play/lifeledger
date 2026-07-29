use super::{OfflineAccrualInput, OfflineAccrualPlan, OfflineRuleError, OfflineRules};

const MICROS_PER_SECOND: i128 = 1_000_000;

pub(super) struct DefaultOfflineRules;

impl OfflineRules for DefaultOfflineRules {
    fn plan_accrual(
        &self,
        input: OfflineAccrualInput,
    ) -> Result<OfflineAccrualPlan, OfflineRuleError> {
        if input.cadence_seconds == 0 {
            return Err(OfflineRuleError::InvalidCadence);
        }
        if input.window_accrued_days > input.absence_window_cap_days
            || input.accrued_through_unix_micros > input.accrual_limit_unix_micros
        {
            return Err(OfflineRuleError::InvalidWindowState);
        }

        let effective_now = input
            .db_now_unix_micros
            .min(input.accrual_limit_unix_micros);
        let elapsed_micros = effective_now
            .saturating_sub(input.accrued_through_unix_micros)
            .max(0) as i128;
        let cadence_micros = i128::from(input.cadence_seconds)
            .checked_mul(MICROS_PER_SECOND)
            .ok_or(OfflineRuleError::ArithmeticOverflow)?;
        let elapsed_days = elapsed_micros / cadence_micros;
        let remaining_window_days = input
            .absence_window_cap_days
            .checked_sub(input.window_accrued_days)
            .ok_or(OfflineRuleError::InvalidWindowState)?;
        let remaining_target_days = input.remaining_target_days.unwrap_or(u32::MAX);
        let days_to_accrue = elapsed_days
            .min(i128::from(remaining_window_days))
            .min(i128::from(remaining_target_days));
        let days_to_accrue =
            u32::try_from(days_to_accrue).map_err(|_| OfflineRuleError::ArithmeticOverflow)?;
        let accrued_through_advance_micros = i128::from(days_to_accrue)
            .checked_mul(cadence_micros)
            .and_then(|value| i64::try_from(value).ok())
            .ok_or(OfflineRuleError::ArithmeticOverflow)?;

        Ok(OfflineAccrualPlan {
            days_to_accrue,
            accrued_through_advance_micros,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{OfflineAccrualInput, create_offline_rules};

    fn given_input() -> OfflineAccrualInput {
        OfflineAccrualInput {
            db_now_unix_micros: 10 * 60 * 1_000_000,
            accrued_through_unix_micros: 0,
            accrual_limit_unix_micros: 90 * 60 * 1_000_000,
            cadence_seconds: 60,
            absence_window_cap_days: 90,
            window_accrued_days: 0,
            remaining_target_days: None,
        }
    }

    fn when_planned(input: OfflineAccrualInput) -> super::super::OfflineAccrualPlan {
        create_offline_rules()
            .plan_accrual(input)
            .expect("valid accrual must be planned")
    }

    mod context_elapsed_time_exceeds_the_window_cap {
        use super::*;

        #[test]
        fn given_elapsed_days_above_cap_when_planned_then_only_window_remainder_accrues() {
            let input = OfflineAccrualInput {
                db_now_unix_micros: 120 * 60 * 1_000_000,
                window_accrued_days: 88,
                ..given_input()
            };

            let plan = when_planned(input);

            assert_eq!(plan.days_to_accrue, 2);
            assert_eq!(plan.accrued_through_advance_micros, 120_000_000);
        }
    }

    mod context_the_ranked_target_is_near {
        use super::*;

        #[test]
        fn given_one_target_day_remaining_when_planned_then_accrual_stops_at_the_target() {
            let input = OfflineAccrualInput {
                remaining_target_days: Some(1),
                ..given_input()
            };

            let plan = when_planned(input);

            assert_eq!(plan.days_to_accrue, 1);
        }
    }

    mod context_the_database_clock_moves_backwards {
        use super::*;

        #[test]
        fn given_now_before_accrued_through_when_planned_then_no_future_credit_is_created() {
            let input = OfflineAccrualInput {
                db_now_unix_micros: 30_000_000,
                accrued_through_unix_micros: 60_000_000,
                ..given_input()
            };

            let plan = when_planned(input);

            assert_eq!(plan.days_to_accrue, 0);
            assert_eq!(plan.accrued_through_advance_micros, 0);
        }
    }
}
