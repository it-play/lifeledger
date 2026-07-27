use std::collections::HashSet;
use std::sync::Arc;

use super::types::{
    CurrentLivingCostAllocation, EssentialArrearDraft, EssentialArrearPayment,
    LIVING_COST_FACTOR_SCALE_PPM, LIVING_COST_PRORATION_SCALE, LivingCostAllocation,
    LivingCostAllocationInput, LivingCostCategory, LivingCostCategoryCalculation,
    LivingCostCategoryCalculationInput, LivingCostError, LivingCostMonthCalculation,
    LivingCostMonthCalculationInput, LivingCostProration, LivingCostRules,
};

struct V1LivingCostRules;

pub fn create_living_cost_rules() -> Arc<dyn LivingCostRules> {
    Arc::new(V1LivingCostRules)
}

impl LivingCostRules for V1LivingCostRules {
    fn parse_category(&self, value: &str) -> Result<LivingCostCategory, LivingCostError> {
        value.parse()
    }

    fn calculate_category(
        &self,
        input: LivingCostCategoryCalculationInput,
    ) -> Result<LivingCostCategoryCalculation, LivingCostError> {
        validate_calculation_input(input)?;

        let mut numerator = i128::from(input.base_monthly_krw)
            .checked_mul(i128::from(input.current_cpi_index))
            .and_then(|value| value.checked_mul(i128::from(input.region_factor_ppm)))
            .and_then(|value| value.checked_mul(i128::from(input.household_factor_ppm)))
            .and_then(|value| value.checked_mul(i128::from(input.budget_factor_ppm)))
            .ok_or(LivingCostError::ArithmeticOverflow)?;
        let scale = i128::from(LIVING_COST_FACTOR_SCALE_PPM);
        let mut denominator = i128::from(input.base_cpi_index)
            .checked_mul(scale)
            .and_then(|value| value.checked_mul(scale))
            .and_then(|value| value.checked_mul(scale))
            .ok_or(LivingCostError::ArithmeticOverflow)?;

        let proration_multiplier = match input.proration {
            Some(proration) => i128::from(proration.remaining_calendar_days)
                .checked_mul(i128::from(
                    LIVING_COST_PRORATION_SCALE / i64::from(proration.days_in_month),
                ))
                .ok_or(LivingCostError::ArithmeticOverflow)?,
            None => i128::from(LIVING_COST_PRORATION_SCALE),
        };
        numerator = numerator
            .checked_mul(proration_multiplier)
            .ok_or(LivingCostError::ArithmeticOverflow)?;
        denominator = denominator
            .checked_mul(i128::from(LIVING_COST_PRORATION_SCALE))
            .ok_or(LivingCostError::ArithmeticOverflow)?;

        if input.prior_remainder_numerator <= -denominator
            || input.prior_remainder_numerator >= denominator
        {
            return Err(LivingCostError::InvalidRemainder(input.category));
        }

        numerator = numerator
            .checked_add(input.prior_remainder_numerator)
            .ok_or(LivingCostError::ArithmeticOverflow)?;
        if numerator < 0 {
            return Err(LivingCostError::InvalidRemainder(input.category));
        }

        let quotient = numerator
            .checked_div(denominator)
            .ok_or(LivingCostError::ArithmeticOverflow)?;
        let remainder_numerator = numerator
            .checked_sub(
                quotient
                    .checked_mul(denominator)
                    .ok_or(LivingCostError::ArithmeticOverflow)?,
            )
            .ok_or(LivingCostError::ArithmeticOverflow)?;
        let gross_krw = i64::try_from(quotient).map_err(|_| LivingCostError::ArithmeticOverflow)?;

        Ok(LivingCostCategoryCalculation {
            category: input.category,
            essential: input.essential,
            gross_krw,
            remainder_numerator,
        })
    }

    fn calculate_month(
        &self,
        input: LivingCostMonthCalculationInput<'_>,
    ) -> Result<LivingCostMonthCalculation, LivingCostError> {
        validate_category_set(input.categories.iter().map(|category| category.category))?;

        let mut categories = Vec::with_capacity(LivingCostCategory::ALL.len());
        let mut total_gross_krw = 0_i64;
        for category in LivingCostCategory::ALL {
            let category_input = input
                .categories
                .iter()
                .find(|candidate| candidate.category == category)
                .copied()
                .ok_or(LivingCostError::MissingCategory(category))?;
            let calculation = self.calculate_category(category_input)?;
            total_gross_krw = total_gross_krw
                .checked_add(calculation.gross_krw)
                .ok_or(LivingCostError::ArithmeticOverflow)?;
            categories.push(calculation);
        }

        Ok(LivingCostMonthCalculation {
            categories,
            total_gross_krw,
        })
    }

    fn allocate_month(
        &self,
        input: LivingCostAllocationInput<'_>,
    ) -> Result<LivingCostAllocation, LivingCostError> {
        validate_allocation_input(input)?;

        let wallet_cash_before_krw = input.wallet_cash_krw;
        let mut wallet_cash_krw = wallet_cash_before_krw;
        let mut current_allocations = Vec::with_capacity(LivingCostCategory::ALL.len());
        let mut created_arrears = Vec::new();

        for category in LivingCostCategory::ALL {
            let charge = input
                .current_charges
                .iter()
                .find(|charge| charge.category == category && charge.essential);
            if let Some(charge) = charge {
                let paid_krw = pay_from_wallet(&mut wallet_cash_krw, charge.gross_krw);
                let unpaid_krw = charge
                    .gross_krw
                    .checked_sub(paid_krw)
                    .ok_or(LivingCostError::ArithmeticOverflow)?;
                current_allocations.push(CurrentLivingCostAllocation {
                    category,
                    essential: true,
                    gross_krw: charge.gross_krw,
                    paid_krw,
                    unpaid_krw,
                });
                if unpaid_krw > 0 {
                    created_arrears.push(EssentialArrearDraft {
                        due_year_month: input.due_year_month,
                        category,
                        amount_krw: unpaid_krw,
                    });
                }
            }
        }

        let mut arrears = input.existing_arrears.iter().collect::<Vec<_>>();
        arrears.sort_by_key(|arrear| {
            (
                arrear.due_year_month,
                arrear.category.order(),
                arrear.arrear_id,
            )
        });
        let mut existing_arrear_payments = Vec::with_capacity(arrears.len());
        for arrear in arrears {
            let paid_krw = pay_from_wallet(&mut wallet_cash_krw, arrear.remaining_krw);
            let balance_after_krw = arrear
                .remaining_krw
                .checked_sub(paid_krw)
                .ok_or(LivingCostError::ArithmeticOverflow)?;
            existing_arrear_payments.push(EssentialArrearPayment {
                arrear_id: arrear.arrear_id,
                due_year_month: arrear.due_year_month,
                category: arrear.category,
                balance_before_krw: arrear.remaining_krw,
                paid_krw,
                balance_after_krw,
            });
        }

        for category in LivingCostCategory::ALL {
            let charge = input
                .current_charges
                .iter()
                .find(|charge| charge.category == category && !charge.essential);
            if let Some(charge) = charge {
                let paid_krw = pay_from_wallet(&mut wallet_cash_krw, charge.gross_krw);
                let unpaid_krw = charge
                    .gross_krw
                    .checked_sub(paid_krw)
                    .ok_or(LivingCostError::ArithmeticOverflow)?;
                current_allocations.push(CurrentLivingCostAllocation {
                    category,
                    essential: false,
                    gross_krw: charge.gross_krw,
                    paid_krw,
                    unpaid_krw,
                });
            }
        }

        Ok(LivingCostAllocation {
            wallet_cash_before_krw,
            wallet_cash_after_krw: wallet_cash_krw,
            current_allocations,
            existing_arrear_payments,
            created_arrears,
        })
    }
}

fn validate_calculation_input(
    input: LivingCostCategoryCalculationInput,
) -> Result<(), LivingCostError> {
    if input.base_monthly_krw < 0 {
        return Err(LivingCostError::InvalidBaseMonthlyAmount(input.category));
    }
    if input.base_cpi_index <= 0 {
        return Err(LivingCostError::InvalidBaseCpiIndex(input.category));
    }
    if input.current_cpi_index <= 0 {
        return Err(LivingCostError::InvalidCurrentCpiIndex(input.category));
    }
    if input.region_factor_ppm <= 0 {
        return Err(LivingCostError::InvalidRegionFactor(input.category));
    }
    if input.household_factor_ppm < LIVING_COST_FACTOR_SCALE_PPM {
        return Err(LivingCostError::InvalidHouseholdFactor(input.category));
    }
    if input.budget_factor_ppm < 0 {
        return Err(LivingCostError::InvalidBudgetFactor(input.category));
    }
    if input.essential && input.budget_factor_ppm == 0 {
        return Err(LivingCostError::RequiredCategoryHasZeroBudget(
            input.category,
        ));
    }
    if input
        .proration
        .is_some_and(|proration| !is_valid_proration(proration))
    {
        return Err(LivingCostError::InvalidProration(input.category));
    }
    Ok(())
}

fn is_valid_proration(proration: LivingCostProration) -> bool {
    (28..=31).contains(&proration.days_in_month)
        && proration.remaining_calendar_days > 0
        && proration.remaining_calendar_days <= proration.days_in_month
}

fn validate_category_set(
    categories: impl Iterator<Item = LivingCostCategory>,
) -> Result<(), LivingCostError> {
    let mut seen = HashSet::with_capacity(LivingCostCategory::ALL.len());
    for category in categories {
        if !seen.insert(category) {
            return Err(LivingCostError::DuplicateCategory(category));
        }
    }
    for category in LivingCostCategory::ALL {
        if !seen.contains(&category) {
            return Err(LivingCostError::MissingCategory(category));
        }
    }
    Ok(())
}

fn validate_allocation_input(input: LivingCostAllocationInput<'_>) -> Result<(), LivingCostError> {
    if !input.due_year_month.is_valid() {
        return Err(LivingCostError::InvalidYearMonth);
    }
    if input.wallet_cash_krw < 0 {
        return Err(LivingCostError::InvalidWalletCash);
    }
    validate_category_set(input.current_charges.iter().map(|charge| charge.category))?;
    for charge in input.current_charges {
        if charge.gross_krw < 0 {
            return Err(LivingCostError::InvalidCurrentCharge(charge.category));
        }
    }

    let mut arrear_ids = HashSet::with_capacity(input.existing_arrears.len());
    for arrear in input.existing_arrears {
        if arrear.arrear_id == 0
            || !arrear.due_year_month.is_valid()
            || arrear.due_year_month > input.due_year_month
            || arrear.remaining_krw <= 0
        {
            return Err(LivingCostError::InvalidArrear);
        }
        if !arrear_ids.insert(arrear.arrear_id) {
            return Err(LivingCostError::DuplicateArrearId(arrear.arrear_id));
        }
    }
    Ok(())
}

fn pay_from_wallet(wallet_cash_krw: &mut i64, due_krw: i64) -> i64 {
    let paid_krw = (*wallet_cash_krw).min(due_krw);
    *wallet_cash_krw -= paid_krw;
    paid_krw
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::life::{
        CurrentLivingCostCharge, EssentialArrearBalance, LivingCostCategoryCalculationInput,
        LivingCostMonthCalculationInput, YearMonth,
    };

    fn given_rules() -> Arc<dyn LivingCostRules> {
        create_living_cost_rules()
    }

    fn given_calculation_input(category: LivingCostCategory) -> LivingCostCategoryCalculationInput {
        LivingCostCategoryCalculationInput {
            category,
            essential: true,
            base_monthly_krw: 100_000,
            base_cpi_index: 100,
            current_cpi_index: 100,
            region_factor_ppm: 1_000_000,
            household_factor_ppm: 1_000_000,
            budget_factor_ppm: 1_000_000,
            prior_remainder_numerator: 0,
            proration: None,
        }
    }

    fn given_month_inputs() -> Vec<LivingCostCategoryCalculationInput> {
        LivingCostCategory::ALL
            .into_iter()
            .map(given_calculation_input)
            .collect()
    }

    fn given_current_charges() -> Vec<CurrentLivingCostCharge> {
        LivingCostCategory::ALL
            .into_iter()
            .map(|category| CurrentLivingCostCharge {
                category,
                essential: false,
                gross_krw: 0,
            })
            .collect()
    }

    fn given_year_month(year: i32, month: u8) -> YearMonth {
        YearMonth { year, month }
    }

    fn when_calculate_category(
        input: LivingCostCategoryCalculationInput,
    ) -> Result<LivingCostCategoryCalculation, LivingCostError> {
        given_rules().calculate_category(input)
    }

    fn when_allocate(
        wallet_cash_krw: i64,
        current_charges: &[CurrentLivingCostCharge],
        existing_arrears: &[EssentialArrearBalance],
    ) -> Result<LivingCostAllocation, LivingCostError> {
        given_rules().allocate_month(LivingCostAllocationInput {
            due_year_month: given_year_month(2026, 7),
            wallet_cash_krw,
            current_charges,
            existing_arrears,
        })
    }

    mod context_생활비_category를_다루는_경우 {
        use super::*;

        #[test]
        fn given_고정_category_when_순서를_읽으면_then_아홉개가_계약순서대로_나온다() {
            let result = LivingCostCategory::ALL.map(LivingCostCategory::as_str);

            assert_eq!(
                result,
                [
                    "housing",
                    "food",
                    "transport",
                    "communication",
                    "utilities",
                    "healthcare",
                    "education",
                    "dependentCare",
                    "discretionary",
                ]
            );
        }

        #[test]
        fn given_알수없는_category_when_parse하면_then_unknown으로_거절한다() {
            let result = given_rules().parse_category("entertainment");

            assert_eq!(result, Err(LivingCostError::UnknownCategory));
        }
    }

    mod context_cpi와_계수를_적용하는_경우 {
        use super::*;

        #[test]
        fn given_cpi지역가구예산계수_when_계산하면_then_i128에서_한번_나눈_월금액이_나온다() {
            let mut input = given_calculation_input(LivingCostCategory::Food);
            input.current_cpi_index = 110;
            input.region_factor_ppm = 1_200_000;
            input.household_factor_ppm = 1_500_000;
            input.budget_factor_ppm = 800_000;

            let result = when_calculate_category(input).expect("생활비를 계산해야 한다");

            assert_eq!(result.gross_krw, 158_400);
        }

        #[test]
        fn given_원미만_remainder_when_세달계산하면_then_세번째달에_1원이_된다() {
            let mut input = given_calculation_input(LivingCostCategory::Utilities);
            input.base_monthly_krw = 1;
            input.base_cpi_index = 3;
            input.current_cpi_index = 1;

            let first = when_calculate_category(input).expect("첫 달을 계산해야 한다");
            input.prior_remainder_numerator = first.remainder_numerator;
            let second = when_calculate_category(input).expect("둘째 달을 계산해야 한다");
            input.prior_remainder_numerator = second.remainder_numerator;
            let result = when_calculate_category(input).expect("셋째 달을 계산해야 한다");

            assert_eq!(result.gross_krw, 1);
        }

        #[test]
        fn given_signed_remainder_when_분자에_더하면_then_계산전에_정확히_상쇄된다() {
            let mut input = given_calculation_input(LivingCostCategory::Communication);
            input.base_monthly_krw = 1;
            input.base_cpi_index = 3;
            input.current_cpi_index = 4;
            input.prior_remainder_numerator =
                -i128::from(LIVING_COST_PRORATION_SCALE) * 1_000_000_000_000_000_000;

            let result = when_calculate_category(input).expect("signed remainder를 계산해야 한다");

            assert_eq!((result.gross_krw, result.remainder_numerator), (1, 0));
        }

        #[test]
        fn given_월중간_10일_when_30일달을_일할하면_then_월금액의_삼분의일이_된다() {
            let mut input = given_calculation_input(LivingCostCategory::Housing);
            input.base_monthly_krw = 300_000;
            input.proration = Some(LivingCostProration {
                remaining_calendar_days: 10,
                days_in_month: 30,
            });

            let result = when_calculate_category(input).expect("일할 생활비를 계산해야 한다");

            assert_eq!(result.gross_krw, 100_000);
        }

        #[test]
        fn given_월중간_remainder_when_다음완전월을_계산하면_then_같은canonical단위로_이어진다() {
            let mut input = given_calculation_input(LivingCostCategory::Healthcare);
            input.base_monthly_krw = 3;
            input.base_cpi_index = 4;
            input.current_cpi_index = 1;
            input.proration = Some(LivingCostProration {
                remaining_calendar_days: 15,
                days_in_month: 30,
            });

            let partial = when_calculate_category(input).expect("중도 시작 월을 계산해야 한다");
            input.proration = None;
            input.prior_remainder_numerator = partial.remainder_numerator;
            let result = when_calculate_category(input).expect("다음 완전 월을 계산해야 한다");

            assert_eq!(result.gross_krw, 1);
        }
    }

    mod context_월전체를_계산하는_경우 {
        use super::*;

        #[test]
        fn given_뒤섞인_아홉_category_when_계산하면_then_고정_enum순서로_반환한다() {
            let mut inputs = given_month_inputs();
            inputs.reverse();

            let result = given_rules()
                .calculate_month(LivingCostMonthCalculationInput {
                    categories: &inputs,
                })
                .expect("월 생활비를 계산해야 한다");

            assert_eq!(
                result
                    .categories
                    .iter()
                    .map(|category| category.category)
                    .collect::<Vec<_>>(),
                LivingCostCategory::ALL
            );
        }

        #[test]
        fn given_중복_category_when_계산하면_then_duplicate로_거절한다() {
            let mut inputs = given_month_inputs();
            inputs[8].category = LivingCostCategory::Housing;

            let result = given_rules().calculate_month(LivingCostMonthCalculationInput {
                categories: &inputs,
            });

            assert_eq!(
                result,
                Err(LivingCostError::DuplicateCategory(
                    LivingCostCategory::Housing
                ))
            );
        }
    }

    mod context_필수생활비와_기존연체와_선택생활비가_함께있는_경우 {
        use super::*;

        #[test]
        fn given_현금250원_when_배분하면_then_당월필수후_기존연체에_먼저_쓴다() {
            let mut charges = given_current_charges();
            charges[0] = CurrentLivingCostCharge {
                category: LivingCostCategory::Housing,
                essential: true,
                gross_krw: 100,
            };
            charges[1] = CurrentLivingCostCharge {
                category: LivingCostCategory::Food,
                essential: true,
                gross_krw: 100,
            };
            charges[2].gross_krw = 100;
            let arrears = [EssentialArrearBalance {
                arrear_id: 1,
                due_year_month: given_year_month(2026, 6),
                category: LivingCostCategory::Food,
                remaining_krw: 80,
            }];

            let result = when_allocate(250, &charges, &arrears).expect("생활비를 배분해야 한다");

            assert_eq!(
                (
                    result.current_allocations[0].paid_krw,
                    result.current_allocations[1].paid_krw,
                    result.existing_arrear_payments[0].paid_krw,
                    result.current_allocations[2].paid_krw,
                    result.wallet_cash_after_krw,
                ),
                (100, 100, 50, 0, 0)
            );
        }

        #[test]
        fn given_뒤섞인_기존연체_when_배분하면_then_기한categoryid순서로_갚는다() {
            let charges = given_current_charges();
            let arrears = [
                EssentialArrearBalance {
                    arrear_id: 3,
                    due_year_month: given_year_month(2026, 6),
                    category: LivingCostCategory::Food,
                    remaining_krw: 100,
                },
                EssentialArrearBalance {
                    arrear_id: 2,
                    due_year_month: given_year_month(2026, 5),
                    category: LivingCostCategory::Food,
                    remaining_krw: 100,
                },
                EssentialArrearBalance {
                    arrear_id: 1,
                    due_year_month: given_year_month(2026, 5),
                    category: LivingCostCategory::Housing,
                    remaining_krw: 100,
                },
            ];

            let result = when_allocate(150, &charges, &arrears).expect("기존 연체를 배분해야 한다");

            assert_eq!(
                result
                    .existing_arrear_payments
                    .iter()
                    .map(|payment| (payment.arrear_id, payment.paid_krw))
                    .collect::<Vec<_>>(),
                vec![(1, 100), (2, 50), (3, 0)]
            );
        }
    }

    mod context_현금이_부족한_경우 {
        use super::*;

        #[test]
        fn given_필수100원과_현금40원_when_배분하면_then_60원필수연체를_만든다() {
            let mut charges = given_current_charges();
            charges[0] = CurrentLivingCostCharge {
                category: LivingCostCategory::Housing,
                essential: true,
                gross_krw: 100,
            };

            let result = when_allocate(40, &charges, &[]).expect("필수 생활비를 배분해야 한다");

            assert_eq!(
                result.created_arrears,
                vec![EssentialArrearDraft {
                    due_year_month: given_year_month(2026, 7),
                    category: LivingCostCategory::Housing,
                    amount_krw: 60,
                }]
            );
        }

        #[test]
        fn given_선택100원과_현금0원_when_배분하면_then_축소하고_연체는_만들지않는다() {
            let mut charges = given_current_charges();
            charges[8].gross_krw = 100;

            let result = when_allocate(0, &charges, &[]).expect("선택 생활비를 배분해야 한다");

            assert_eq!(
                (
                    result.current_allocations[8].unpaid_krw,
                    result.created_arrears,
                    result.wallet_cash_after_krw,
                ),
                (100, Vec::new(), 0)
            );
        }
    }

    mod context_입력이_유효하지_않은_경우 {
        use super::*;

        #[test]
        fn given_필수항목의_0예산계수_when_계산하면_then_거절한다() {
            let mut input = given_calculation_input(LivingCostCategory::Food);
            input.budget_factor_ppm = 0;

            let result = when_calculate_category(input);

            assert_eq!(
                result,
                Err(LivingCostError::RequiredCategoryHasZeroBudget(
                    LivingCostCategory::Food
                ))
            );
        }

        #[test]
        fn given_canonical분모이상의_remainder_when_계산하면_then_invalid로_거절한다() {
            let mut input = given_calculation_input(LivingCostCategory::Utilities);
            input.prior_remainder_numerator = 100_i128
                * 1_000_000_i128
                * 1_000_000_i128
                * 1_000_000_i128
                * i128::from(LIVING_COST_PRORATION_SCALE);

            let result = when_calculate_category(input);

            assert_eq!(
                result,
                Err(LivingCostError::InvalidRemainder(
                    LivingCostCategory::Utilities
                ))
            );
        }

        #[test]
        fn given_27일인_달_when_일할계산하면_then_invalid로_거절한다() {
            let mut input = given_calculation_input(LivingCostCategory::Housing);
            input.proration = Some(LivingCostProration {
                remaining_calendar_days: 10,
                days_in_month: 27,
            });

            let result = when_calculate_category(input);

            assert_eq!(
                result,
                Err(LivingCostError::InvalidProration(
                    LivingCostCategory::Housing
                ))
            );
        }

        #[test]
        fn given_청구분자보다_작은_음수remainder_when_계산하면_then_invalid로_거절한다() {
            let mut input = given_calculation_input(LivingCostCategory::Utilities);
            input.base_monthly_krw = 0;
            input.prior_remainder_numerator = -1;

            let result = when_calculate_category(input);

            assert_eq!(
                result,
                Err(LivingCostError::InvalidRemainder(
                    LivingCostCategory::Utilities
                ))
            );
        }

        #[test]
        fn given_i128을_넘는_계수_when_계산하면_then_overflow로_거절한다() {
            let mut input = given_calculation_input(LivingCostCategory::Housing);
            input.base_monthly_krw = i64::MAX;
            input.current_cpi_index = i64::MAX;
            input.region_factor_ppm = i64::MAX;
            input.household_factor_ppm = i64::MAX;
            input.budget_factor_ppm = i64::MAX;

            let result = when_calculate_category(input);

            assert_eq!(result, Err(LivingCostError::ArithmeticOverflow));
        }

        #[test]
        fn given_음수지갑_when_배분하면_then_거절한다() {
            let charges = given_current_charges();

            let result = when_allocate(-1, &charges, &[]);

            assert_eq!(result, Err(LivingCostError::InvalidWalletCash));
        }
    }
}
