use std::collections::HashSet;
use std::sync::Arc;

use super::types::{
    CreditBand, CreditDayCalculation, CreditDayInput, CreditDefaultAssessment,
    CreditDefaultAssessmentInput, CreditEventApplication, CreditEventKind, CreditModelTerms,
    CreditRuleError, CreditRules, LoanContractStatus, LoanContractTransitionInput,
    LoanEndOfDayStatusInput,
};

struct V1CreditRules;

pub fn create_credit_rules() -> Arc<dyn CreditRules> {
    Arc::new(V1CreditRules)
}

impl CreditRules for V1CreditRules {
    fn starting_units(&self, model: CreditModelTerms) -> Result<i64, CreditRuleError> {
        validate_model(model)?;
        Ok(model.starting_units)
    }

    fn band(&self, model: CreditModelTerms, units: i64) -> Result<CreditBand, CreditRuleError> {
        determine_band(model, units)
    }

    fn calculate_day(
        &self,
        input: CreditDayInput<'_>,
    ) -> Result<CreditDayCalculation, CreditRuleError> {
        calculate_day(input)
    }

    fn assess_default(
        &self,
        input: CreditDefaultAssessmentInput<'_>,
    ) -> Result<CreditDefaultAssessment, CreditRuleError> {
        assess_default(input)
    }

    fn is_transition_allowed(
        &self,
        input: LoanContractTransitionInput,
    ) -> Result<bool, CreditRuleError> {
        Ok(is_transition_allowed(input))
    }

    fn resolve_end_of_day_status(
        &self,
        input: LoanEndOfDayStatusInput,
    ) -> Result<LoanContractStatus, CreditRuleError> {
        resolve_end_of_day_status(input)
    }
}

fn calculate_day(input: CreditDayInput<'_>) -> Result<CreditDayCalculation, CreditRuleError> {
    validate_model(input.model)?;
    if !(input.model.minimum_units..=input.model.maximum_units).contains(&input.current_units) {
        return Err(CreditRuleError::UnitsOutOfRange);
    }

    let mut seen = HashSet::with_capacity(input.events.len());
    let mut events = input.events.to_vec();
    for event in &events {
        if event.contract_id == 0 {
            return Err(CreditRuleError::InvalidContractId);
        }
        if !seen.insert((event.contract_id, event.kind)) {
            return Err(CreditRuleError::DuplicateEvent(
                event.contract_id,
                event.kind,
            ));
        }
    }
    events.sort_by_key(|event| (event.contract_id, event.kind.order()));

    let mut event_delta_units = 0_i64;
    let mut event_applications = Vec::with_capacity(events.len());
    for event in events {
        let delta_units = match event.kind {
            CreditEventKind::EnteredDelinquency => input.model.delinquency_event_penalty_units,
            CreditEventKind::EnteredDefault => input.model.default_event_penalty_units,
            CreditEventKind::EnteredLegalProcedure => {
                input.model.legal_procedure_event_penalty_units
            }
        };
        event_delta_units = event_delta_units
            .checked_add(delta_units)
            .ok_or(CreditRuleError::ArithmeticOverflow)?;
        event_applications.push(CreditEventApplication {
            contract_id: event.contract_id,
            kind: event.kind,
            delta_units,
        });
    }
    let daily_delta_units = if input.adverse_contract_count > 0 {
        input.model.adverse_day_penalty_units
    } else {
        input.model.recovery_units
    };
    let unclamped_units = i128::from(input.current_units)
        .checked_add(i128::from(event_delta_units))
        .and_then(|value| value.checked_add(i128::from(daily_delta_units)))
        .ok_or(CreditRuleError::ArithmeticOverflow)?;
    let clamped_units = unclamped_units.clamp(
        i128::from(input.model.minimum_units),
        i128::from(input.model.maximum_units),
    );
    let units_after =
        i64::try_from(clamped_units).map_err(|_| CreditRuleError::ArithmeticOverflow)?;
    let band = determine_band(input.model, units_after)?;

    Ok(CreditDayCalculation {
        units_before: input.current_units,
        event_applications,
        event_delta_units,
        daily_delta_units,
        units_after,
        band,
    })
}

fn determine_band(model: CreditModelTerms, units: i64) -> Result<CreditBand, CreditRuleError> {
    validate_model(model)?;
    if !(model.minimum_units..=model.maximum_units).contains(&units) {
        return Err(CreditRuleError::UnitsOutOfRange);
    }
    let thresholds = model.band_thresholds;
    if units >= thresholds.prime_min_units {
        Ok(CreditBand::Prime)
    } else if units >= thresholds.standard_min_units {
        Ok(CreditBand::Standard)
    } else if units >= thresholds.limited_min_units {
        Ok(CreditBand::Limited)
    } else if units >= thresholds.distressed_min_units {
        Ok(CreditBand::Distressed)
    } else {
        Ok(CreditBand::Insolvent)
    }
}

fn assess_default(
    input: CreditDefaultAssessmentInput<'_>,
) -> Result<CreditDefaultAssessment, CreditRuleError> {
    validate_model(input.model)?;
    let mut bucket_ids = HashSet::with_capacity(input.buckets.len());
    let mut total_outstanding_krw = 0_i64;
    let mut oldest_days_past_due = 0_u32;
    for bucket in input.buckets {
        if bucket.bucket_id == 0 || bucket.outstanding_krw <= 0 {
            return Err(CreditRuleError::InvalidDelinquencyBucket);
        }
        if !bucket_ids.insert(bucket.bucket_id) {
            return Err(CreditRuleError::DuplicateDelinquencyBucket(
                bucket.bucket_id,
            ));
        }
        total_outstanding_krw = total_outstanding_krw
            .checked_add(bucket.outstanding_krw)
            .ok_or(CreditRuleError::ArithmeticOverflow)?;
        oldest_days_past_due = oldest_days_past_due.max(bucket.days_past_due);
    }
    let should_default = oldest_days_past_due >= input.model.default_oldest_days
        || (total_outstanding_krw >= input.model.amount_default_threshold_krw
            && oldest_days_past_due >= input.model.amount_default_oldest_days);
    Ok(CreditDefaultAssessment {
        total_outstanding_krw,
        oldest_days_past_due,
        should_default,
    })
}

fn is_transition_allowed(input: LoanContractTransitionInput) -> bool {
    if input.from == input.to {
        return true;
    }
    match (input.from, input.to) {
        (LoanContractStatus::Pending, LoanContractStatus::Active) => true,
        (LoanContractStatus::Pending, LoanContractStatus::Cancelled) => !input.money_moved,
        (LoanContractStatus::Active, LoanContractStatus::Delinquent)
        | (LoanContractStatus::Active, LoanContractStatus::PaidOff)
        | (LoanContractStatus::Delinquent, LoanContractStatus::Active)
        | (LoanContractStatus::Delinquent, LoanContractStatus::Defaulted)
        | (LoanContractStatus::Defaulted, LoanContractStatus::Restructured)
        | (LoanContractStatus::Defaulted, LoanContractStatus::Discharged)
        | (LoanContractStatus::Defaulted, LoanContractStatus::ChargedOff) => true,
        _ => false,
    }
}

fn resolve_end_of_day_status(
    input: LoanEndOfDayStatusInput,
) -> Result<LoanContractStatus, CreditRuleError> {
    match input.current {
        LoanContractStatus::Active => {
            if input.default_triggered {
                return Err(CreditRuleError::InvalidStatusResolution);
            }
            if input.has_unpaid_buckets {
                Ok(LoanContractStatus::Delinquent)
            } else {
                Ok(LoanContractStatus::Active)
            }
        }
        LoanContractStatus::Delinquent => {
            if input.default_triggered && !input.has_unpaid_buckets {
                return Err(CreditRuleError::InvalidStatusResolution);
            }
            if input.default_triggered {
                Ok(LoanContractStatus::Defaulted)
            } else if input.has_unpaid_buckets {
                Ok(LoanContractStatus::Delinquent)
            } else {
                Ok(LoanContractStatus::Active)
            }
        }
        _ => Err(CreditRuleError::InvalidStatusResolution),
    }
}

fn validate_model(model: CreditModelTerms) -> Result<(), CreditRuleError> {
    let thresholds = model.band_thresholds;
    if model.minimum_units > model.maximum_units
        || !(model.minimum_units..=model.maximum_units).contains(&model.starting_units)
        || thresholds.insolvent_min_units != model.minimum_units
        || thresholds.prime_min_units > model.maximum_units
        || thresholds.prime_min_units <= thresholds.standard_min_units
        || thresholds.standard_min_units <= thresholds.limited_min_units
        || thresholds.limited_min_units <= thresholds.distressed_min_units
        || thresholds.distressed_min_units <= thresholds.insolvent_min_units
        || model.delinquency_event_penalty_units > 0
        || model.default_event_penalty_units > 0
        || model.legal_procedure_event_penalty_units > 0
        || model.adverse_day_penalty_units > 0
        || model.recovery_units < 0
        || model.default_oldest_days == 0
        || model.amount_default_threshold_krw <= 0
        || model.amount_default_oldest_days == 0
        || model.amount_default_oldest_days > model.default_oldest_days
    {
        return Err(CreditRuleError::InvalidModel);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::life::{
        CreditBandThresholds, CreditDayEvent, CreditDefaultAssessmentInput, CreditDelinquencyBucket,
    };

    fn given_rules() -> Arc<dyn CreditRules> {
        create_credit_rules()
    }

    fn given_model() -> CreditModelTerms {
        CreditModelTerms {
            minimum_units: 0,
            maximum_units: 1_000,
            starting_units: 700,
            band_thresholds: CreditBandThresholds {
                prime_min_units: 850,
                standard_min_units: 650,
                limited_min_units: 450,
                distressed_min_units: 1,
                insolvent_min_units: 0,
            },
            delinquency_event_penalty_units: -80,
            default_event_penalty_units: -300,
            legal_procedure_event_penalty_units: 0,
            adverse_day_penalty_units: -5,
            recovery_units: 1,
            default_oldest_days: 90,
            amount_default_threshold_krw: 1_000_000,
            amount_default_oldest_days: 30,
        }
    }

    mod context_credit_band를_판정하는_경우 {
        use super::*;

        #[test]
        fn given_개발model_when_시작하면_then_700units_standard이다() {
            let rules = given_rules();
            let model = given_model();

            let units = rules
                .starting_units(model)
                .expect("시작 units를 읽어야 한다");
            let result = rules.band(model, units).expect("시작 band를 판정해야 한다");

            assert_eq!((units, result), (700, CreditBand::Standard));
        }

        #[test]
        fn given_band경계값_when_판정하면_then_각하한을포함한다() {
            let rules = given_rules();
            let model = given_model();

            let result = [850, 650, 450, 1, 0]
                .into_iter()
                .map(|units| rules.band(model, units).expect("band를 판정해야 한다"))
                .collect::<Vec<_>>();

            assert_eq!(
                result,
                vec![
                    CreditBand::Prime,
                    CreditBand::Standard,
                    CreditBand::Limited,
                    CreditBand::Distressed,
                    CreditBand::Insolvent,
                ]
            );
        }

        #[test]
        fn given_범위밖units_when_판정하면_then_out_of_range로거절한다() {
            let result = given_rules().band(given_model(), 1_001);

            assert_eq!(result, Err(CreditRuleError::UnitsOutOfRange));
        }
    }

    mod context_credit_units를_하루갱신하는_경우 {
        use super::*;

        #[test]
        fn given_뒤섞인연체default이벤트와연체계약_when_갱신하면_then_id순합계후daily를한번적용한다()
         {
            let events = [
                CreditDayEvent {
                    contract_id: 3,
                    kind: CreditEventKind::EnteredDelinquency,
                },
                CreditDayEvent {
                    contract_id: 1,
                    kind: CreditEventKind::EnteredDefault,
                },
            ];

            let result = given_rules()
                .calculate_day(CreditDayInput {
                    model: given_model(),
                    current_units: 700,
                    events: &events,
                    adverse_contract_count: 2,
                })
                .expect("credit units를 갱신해야 한다");

            assert_eq!(
                (
                    result
                        .event_applications
                        .iter()
                        .map(|event| event.contract_id)
                        .collect::<Vec<_>>(),
                    result.event_delta_units,
                    result.daily_delta_units,
                    result.units_after,
                    result.band,
                ),
                (vec![1, 3], -380, -5, 315, CreditBand::Distressed)
            );
        }

        #[test]
        fn given_연체계약이세개일때_when_daily를적용하면_then_계약수와무관하게5만차감한다() {
            let result = given_rules()
                .calculate_day(CreditDayInput {
                    model: given_model(),
                    current_units: 700,
                    events: &[],
                    adverse_contract_count: 3,
                })
                .expect("daily penalty를 적용해야 한다");

            assert_eq!((result.daily_delta_units, result.units_after), (-5, 695));
        }

        #[test]
        fn given_연체가없는849units_when_갱신하면_then_1회복해prime이된다() {
            let result = given_rules()
                .calculate_day(CreditDayInput {
                    model: given_model(),
                    current_units: 849,
                    events: &[],
                    adverse_contract_count: 0,
                })
                .expect("credit units를 회복해야 한다");

            assert_eq!((result.units_after, result.band), (850, CreditBand::Prime));
        }

        #[test]
        fn given_100units와default이벤트_when_갱신하면_then_모든변화뒤0으로한번clamp한다() {
            let events = [CreditDayEvent {
                contract_id: 1,
                kind: CreditEventKind::EnteredDefault,
            }];

            let result = given_rules()
                .calculate_day(CreditDayInput {
                    model: given_model(),
                    current_units: 100,
                    events: &events,
                    adverse_contract_count: 1,
                })
                .expect("최솟값으로 clamp해야 한다");

            assert_eq!(
                (result.units_after, result.band),
                (0, CreditBand::Insolvent)
            );
        }

        #[test]
        fn given_최대units와무연체_when_회복하면_then_1000에서clamp한다() {
            let result = given_rules()
                .calculate_day(CreditDayInput {
                    model: given_model(),
                    current_units: 1_000,
                    events: &[],
                    adverse_contract_count: 0,
                })
                .expect("최댓값으로 clamp해야 한다");

            assert_eq!(result.units_after, 1_000);
        }

        #[test]
        fn given_m4e이전legal_procedure이벤트_when_갱신하면_then_event_penalty는명시적0이다() {
            let events = [CreditDayEvent {
                contract_id: 1,
                kind: CreditEventKind::EnteredLegalProcedure,
            }];

            let result = given_rules()
                .calculate_day(CreditDayInput {
                    model: given_model(),
                    current_units: 700,
                    events: &events,
                    adverse_contract_count: 0,
                })
                .expect("M4-E 이전 legal event를 계산해야 한다");

            assert_eq!(result.event_delta_units, 0);
        }

        #[test]
        fn given_같은계약의중복이벤트_when_갱신하면_then_duplicate로거절한다() {
            let event = CreditDayEvent {
                contract_id: 1,
                kind: CreditEventKind::EnteredDelinquency,
            };
            let events = [event, event];

            let result = given_rules().calculate_day(CreditDayInput {
                model: given_model(),
                current_units: 700,
                events: &events,
                adverse_contract_count: 1,
            });

            assert_eq!(
                result,
                Err(CreditRuleError::DuplicateEvent(
                    1,
                    CreditEventKind::EnteredDelinquency
                ))
            );
        }
    }

    mod context_default조건을_판정하는_경우 {
        use super::*;

        #[test]
        fn given_소액bucket이90일일때_when_판정하면_then_default한다() {
            let buckets = [CreditDelinquencyBucket {
                bucket_id: 1,
                days_past_due: 90,
                outstanding_krw: 1,
            }];

            let result = given_rules()
                .assess_default(CreditDefaultAssessmentInput {
                    model: given_model(),
                    buckets: &buckets,
                })
                .expect("90일 default를 판정해야 한다");

            assert!(result.should_default);
        }

        #[test]
        fn given_소액bucket이89일일때_when_판정하면_then_default하지않는다() {
            let buckets = [CreditDelinquencyBucket {
                bucket_id: 1,
                days_past_due: 89,
                outstanding_krw: 1,
            }];

            let result = given_rules()
                .assess_default(CreditDefaultAssessmentInput {
                    model: given_model(),
                    buckets: &buckets,
                })
                .expect("89일 상태를 판정해야 한다");

            assert!(!result.should_default);
        }

        #[test]
        fn given_미납합이100만원이고30일일때_when_판정하면_then_default한다() {
            let buckets = [
                CreditDelinquencyBucket {
                    bucket_id: 2,
                    days_past_due: 10,
                    outstanding_krw: 400_000,
                },
                CreditDelinquencyBucket {
                    bucket_id: 1,
                    days_past_due: 30,
                    outstanding_krw: 600_000,
                },
            ];

            let result = given_rules()
                .assess_default(CreditDefaultAssessmentInput {
                    model: given_model(),
                    buckets: &buckets,
                })
                .expect("금액과 일수 default를 판정해야 한다");

            assert!(result.should_default);
        }

        #[test]
        fn given_미납합이999999원이고30일일때_when_판정하면_then_default하지않는다() {
            let buckets = [CreditDelinquencyBucket {
                bucket_id: 1,
                days_past_due: 30,
                outstanding_krw: 999_999,
            }];

            let result = given_rules()
                .assess_default(CreditDefaultAssessmentInput {
                    model: given_model(),
                    buckets: &buckets,
                })
                .expect("금액 경계를 판정해야 한다");

            assert!(!result.should_default);
        }
    }

    mod context_loan_status를_전이하는_경우 {
        use super::*;

        #[test]
        fn given_돈이움직이지않은pending_when_cancel하면_then_허용한다() {
            let result = given_rules()
                .is_transition_allowed(LoanContractTransitionInput {
                    from: LoanContractStatus::Pending,
                    to: LoanContractStatus::Cancelled,
                    money_moved: false,
                })
                .expect("pending 취소를 판정해야 한다");

            assert!(result);
        }

        #[test]
        fn given_돈이움직인pending_when_cancel하면_then_거절한다() {
            let result = given_rules()
                .is_transition_allowed(LoanContractTransitionInput {
                    from: LoanContractStatus::Pending,
                    to: LoanContractStatus::Cancelled,
                    money_moved: true,
                })
                .expect("money moved 취소를 판정해야 한다");

            assert!(!result);
        }

        #[test]
        fn given_active와미납bucket_when_하루를마치면_then_delinquent가된다() {
            let result = given_rules()
                .resolve_end_of_day_status(LoanEndOfDayStatusInput {
                    current: LoanContractStatus::Active,
                    has_unpaid_buckets: true,
                    default_triggered: false,
                })
                .expect("연체 상태를 판정해야 한다");

            assert_eq!(result, LoanContractStatus::Delinquent);
        }

        #[test]
        fn given_delinquent와해소된bucket_when_하루를마치면_then_active로돌아간다() {
            let result = given_rules()
                .resolve_end_of_day_status(LoanEndOfDayStatusInput {
                    current: LoanContractStatus::Delinquent,
                    has_unpaid_buckets: false,
                    default_triggered: false,
                })
                .expect("연체 해소 상태를 판정해야 한다");

            assert_eq!(result, LoanContractStatus::Active);
        }

        #[test]
        fn given_delinquent와default조건_when_하루를마치면_then_defaulted가된다() {
            let result = given_rules()
                .resolve_end_of_day_status(LoanEndOfDayStatusInput {
                    current: LoanContractStatus::Delinquent,
                    has_unpaid_buckets: true,
                    default_triggered: true,
                })
                .expect("default 상태를 판정해야 한다");

            assert_eq!(result, LoanContractStatus::Defaulted);
        }

        #[test]
        fn given_active에서default직행_when_하루를마치면_then_전이불변식으로거절한다() {
            let result = given_rules().resolve_end_of_day_status(LoanEndOfDayStatusInput {
                current: LoanContractStatus::Active,
                has_unpaid_buckets: true,
                default_triggered: true,
            });

            assert_eq!(result, Err(CreditRuleError::InvalidStatusResolution));
        }

        #[test]
        fn given_defaulted_when_종료상태로전이하면_then_세경로만허용한다() {
            let rules = given_rules();

            let result = [
                LoanContractStatus::Restructured,
                LoanContractStatus::Discharged,
                LoanContractStatus::ChargedOff,
            ]
            .into_iter()
            .map(|to| {
                rules
                    .is_transition_allowed(LoanContractTransitionInput {
                        from: LoanContractStatus::Defaulted,
                        to,
                        money_moved: true,
                    })
                    .expect("default 종료 전이를 판정해야 한다")
            })
            .collect::<Vec<_>>();

            assert_eq!(result, vec![true, true, true]);
        }
    }

    mod context_model이_유효하지않은_경우 {
        use super::*;

        #[test]
        fn given_겹치는band하한_when_시작값을읽으면_then_invalid_model이다() {
            let mut model = given_model();
            model.band_thresholds.standard_min_units = 850;

            let result = given_rules().starting_units(model);

            assert_eq!(result, Err(CreditRuleError::InvalidModel));
        }
    }
}
