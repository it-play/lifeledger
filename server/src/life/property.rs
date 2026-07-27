use std::sync::Arc;

use sha2::{Digest, Sha256};
use time::Date;

use super::types::{
    AcquisitionIncidentalCostInput, CapitalGainsTaxScope, LOAN_RATIO_SCALE_PPM,
    MortgageFundingLimit, MortgageFundingLimitInput, MortgageRegionalPriceCapInput,
    PropertyDispositionCostInput, PropertyError, PropertyPurchaseFundingInput,
    PropertyPurchaseFundingPlan, PropertyRules, PropertySaleCandidateInput,
    PropertySaleCandidatePlan, PropertySaleLedgerPosting, PropertySaleLiquidityBand,
    PropertySaleLiquidityProfile, PropertySalePeriod, PropertySalePeriodInput,
    PropertySaleProceedsInput, PropertySaleProceedsPlan, PropertySaleReferenceValueInput,
};
use crate::finance::LedgerAccountCode;

const PROPERTY_SALE_CANDIDATE_ENTROPY_TAG: &[u8] = b"propertySaleCandidate";

#[derive(Debug, Default)]
struct V1PropertyRules;

pub fn create_property_rules() -> Arc<dyn PropertyRules> {
    Arc::new(V1PropertyRules)
}

impl PropertyRules for V1PropertyRules {
    fn calculate_acquisition_incidental_cost(
        &self,
        input: AcquisitionIncidentalCostInput,
    ) -> Result<i64, PropertyError> {
        if input.purchase_price_krw <= 0 || !(1..=LOAN_RATIO_SCALE_PPM).contains(&input.cost_ppm) {
            return Err(PropertyError::InvalidAcquisitionCost);
        }

        let cost_krw = i128::from(input.purchase_price_krw)
            .checked_mul(i128::from(input.cost_ppm))
            .ok_or(PropertyError::ArithmeticOverflow)?
            .checked_div(i128::from(LOAN_RATIO_SCALE_PPM))
            .ok_or(PropertyError::ArithmeticOverflow)?
            .max(1);
        i64::try_from(cost_krw).map_err(|_| PropertyError::ArithmeticOverflow)
    }

    fn calculate_mortgage_funding_limit(
        &self,
        input: MortgageFundingLimitInput,
    ) -> Result<MortgageFundingLimit, PropertyError> {
        if input.recognized_collateral_value_krw <= 0
            || !(1..=LOAN_RATIO_SCALE_PPM).contains(&input.ltv_limit_ppm)
            || input.product_maximum_principal_krw <= 0
            || input.regional_price_cap_krw.is_some_and(|cap| cap <= 0)
        {
            return Err(PropertyError::InvalidMortgageFundingLimit);
        }

        let ltv_based_limit_krw = i128::from(input.recognized_collateral_value_krw)
            .checked_mul(i128::from(input.ltv_limit_ppm))
            .ok_or(PropertyError::ArithmeticOverflow)?
            .checked_div(i128::from(LOAN_RATIO_SCALE_PPM))
            .ok_or(PropertyError::ArithmeticOverflow)?;
        let ltv_based_limit_krw =
            i64::try_from(ltv_based_limit_krw).map_err(|_| PropertyError::ArithmeticOverflow)?;
        let maximum_mortgage_krw = input
            .regional_price_cap_krw
            .map_or(ltv_based_limit_krw, |cap| ltv_based_limit_krw.min(cap))
            .min(input.product_maximum_principal_krw);

        Ok(MortgageFundingLimit {
            ltv_based_limit_krw,
            regional_price_cap_krw: input.regional_price_cap_krw,
            maximum_mortgage_krw,
        })
    }

    fn select_mortgage_regional_price_cap(
        &self,
        input: MortgageRegionalPriceCapInput,
    ) -> Result<Option<i64>, PropertyError> {
        if input.recognized_collateral_value_krw <= 0 {
            return Err(PropertyError::InvalidMortgageRegionalPriceCap);
        }
        let Some(policy) = input.policy else {
            return Ok(None);
        };
        if policy.lower_price_threshold_krw <= 0
            || policy.upper_price_threshold_krw <= policy.lower_price_threshold_krw
            || policy.lower_band_cap_krw <= 0
            || policy.middle_band_cap_krw <= 0
            || policy.upper_band_cap_krw <= 0
        {
            return Err(PropertyError::InvalidMortgageRegionalPriceCap);
        }

        let cap_krw = if input.recognized_collateral_value_krw <= policy.lower_price_threshold_krw {
            policy.lower_band_cap_krw
        } else if input.recognized_collateral_value_krw <= policy.upper_price_threshold_krw {
            policy.middle_band_cap_krw
        } else {
            policy.upper_band_cap_krw
        };
        Ok(Some(cap_krw))
    }

    fn plan_purchase_funding(
        &self,
        input: PropertyPurchaseFundingInput,
    ) -> Result<PropertyPurchaseFundingPlan, PropertyError> {
        if input.wallet_cash_krw < 0
            || input.returned_deposit_krw < 0
            || input.repaid_loan_principal_krw < 0
            || input.repaid_loan_principal_krw > input.returned_deposit_krw
            || input.purchase_price_krw <= 0
            || input.acquisition_incidental_cost_krw < 0
            || input.moving_cost_krw < 0
            || input.new_mortgage_principal_krw < 0
            || input.new_mortgage_principal_krw > input.purchase_price_krw
        {
            return Err(PropertyError::InvalidPurchaseFunding);
        }

        let net_returned_deposit_krw = input
            .returned_deposit_krw
            .checked_sub(input.repaid_loan_principal_krw)
            .ok_or(PropertyError::ArithmeticOverflow)?;
        let available_buyer_cash_krw = input
            .wallet_cash_krw
            .checked_add(net_returned_deposit_krw)
            .ok_or(PropertyError::ArithmeticOverflow)?;
        let total_purchase_cash_krw = input
            .purchase_price_krw
            .checked_add(input.acquisition_incidental_cost_krw)
            .and_then(|value| value.checked_add(input.moving_cost_krw))
            .ok_or(PropertyError::ArithmeticOverflow)?;
        let required_buyer_cash_krw = total_purchase_cash_krw
            .checked_sub(input.new_mortgage_principal_krw)
            .ok_or(PropertyError::ArithmeticOverflow)?;
        if available_buyer_cash_krw < required_buyer_cash_krw {
            return Err(PropertyError::InsufficientWalletCash);
        }

        let wallet_cash_after_krw = available_buyer_cash_krw
            .checked_sub(required_buyer_cash_krw)
            .ok_or(PropertyError::ArithmeticOverflow)?;
        let wallet_delta_krw = wallet_cash_after_krw
            .checked_sub(input.wallet_cash_krw)
            .ok_or(PropertyError::ArithmeticOverflow)?;
        let debt_delta_krw = input
            .new_mortgage_principal_krw
            .checked_sub(input.repaid_loan_principal_krw)
            .ok_or(PropertyError::ArithmeticOverflow)?;

        Ok(PropertyPurchaseFundingPlan {
            wallet_cash_before_krw: input.wallet_cash_krw,
            wallet_cash_after_krw,
            available_buyer_cash_krw,
            required_buyer_cash_krw,
            returned_deposit_krw: input.returned_deposit_krw,
            repaid_loan_principal_krw: input.repaid_loan_principal_krw,
            purchase_price_krw: input.purchase_price_krw,
            acquisition_incidental_cost_krw: input.acquisition_incidental_cost_krw,
            moving_cost_krw: input.moving_cost_krw,
            new_mortgage_principal_krw: input.new_mortgage_principal_krw,
            wallet_delta_krw,
            debt_delta_krw,
            property_book_value_delta_krw: input.purchase_price_krw,
        })
    }

    fn calculate_sale_reference_value(
        &self,
        input: PropertySaleReferenceValueInput,
    ) -> Result<i64, PropertyError> {
        if input.acquisition_price_krw <= 0
            || input.acquisition_price_index_ppm <= 0
            || input.current_price_index_ppm <= 0
        {
            return Err(PropertyError::InvalidSaleReferenceValue);
        }

        let reference_value_krw = i128::from(input.acquisition_price_krw)
            .checked_mul(i128::from(input.current_price_index_ppm))
            .ok_or(PropertyError::ArithmeticOverflow)?
            .checked_div(i128::from(input.acquisition_price_index_ppm))
            .ok_or(PropertyError::ArithmeticOverflow)?;
        if reference_value_krw <= 0 {
            return Err(PropertyError::InvalidSaleReferenceValue);
        }
        i64::try_from(reference_value_krw).map_err(|_| PropertyError::ArithmeticOverflow)
    }

    fn plan_sale_candidate(
        &self,
        input: PropertySaleCandidateInput,
    ) -> Result<PropertySaleCandidatePlan, PropertyError> {
        validate_sale_liquidity(input.liquidity)?;
        if input.order_revision == 0
            || input.reference_value_krw <= 0
            || input.asking_price_krw <= 0
        {
            return Err(PropertyError::InvalidSaleCandidate);
        }

        let scaled_asking = i128::from(input.asking_price_krw)
            .checked_mul(i128::from(LOAN_RATIO_SCALE_PPM))
            .ok_or(PropertyError::ArithmeticOverflow)?;
        let scaled_reference = |ratio_ppm: i64| {
            i128::from(input.reference_value_krw)
                .checked_mul(i128::from(ratio_ppm))
                .ok_or(PropertyError::ArithmeticOverflow)
        };
        if scaled_asking < scaled_reference(input.liquidity.minimum_asking_ratio_ppm)?
            || scaled_asking > scaled_reference(input.liquidity.maximum_asking_ratio_ppm)?
        {
            return Err(PropertyError::AskingPriceOutOfRange);
        }

        let (liquidity_band, minimum_delay_days, maximum_delay_days) = if scaled_asking
            <= scaled_reference(input.liquidity.fast_band_maximum_asking_ratio_ppm)?
        {
            (
                PropertySaleLiquidityBand::Fast,
                input.liquidity.fast_band_minimum_delay_days,
                input.liquidity.fast_band_maximum_delay_days,
            )
        } else if scaled_asking
            <= scaled_reference(input.liquidity.normal_band_maximum_asking_ratio_ppm)?
        {
            (
                PropertySaleLiquidityBand::Normal,
                input.liquidity.normal_band_minimum_delay_days,
                input.liquidity.normal_band_maximum_delay_days,
            )
        } else {
            (
                PropertySaleLiquidityBand::Slow,
                input.liquidity.slow_band_minimum_delay_days,
                input.liquidity.slow_band_maximum_delay_days,
            )
        };
        let delay_days = sample_sale_delay(input, minimum_delay_days, maximum_delay_days)?;
        let candidate_game_day = input
            .current_game_day
            .checked_add(u32::from(delay_days))
            .ok_or(PropertyError::ArithmeticOverflow)?;

        Ok(PropertySaleCandidatePlan {
            liquidity_band,
            delay_days,
            candidate_game_day,
        })
    }

    fn calculate_sale_period(
        &self,
        input: PropertySalePeriodInput,
    ) -> Result<PropertySalePeriod, PropertyError> {
        if input.minimum_holding_years == 0
            || input.minimum_residence_years == 0
            || input.owner_occupied_from < input.acquired_on
            || input.as_of < input.owner_occupied_from
        {
            return Err(PropertyError::InvalidSalePeriod);
        }

        let completed_holding_years = completed_calendar_years(input.acquired_on, input.as_of)?;
        let completed_residence_years =
            completed_calendar_years(input.owner_occupied_from, input.as_of)?;

        Ok(PropertySalePeriod {
            completed_holding_years,
            completed_residence_years,
            minimum_holding_years: input.minimum_holding_years,
            minimum_residence_years: input.minimum_residence_years,
            is_eligible: completed_holding_years >= input.minimum_holding_years
                && completed_residence_years >= input.minimum_residence_years,
        })
    }

    fn calculate_disposition_cost(
        &self,
        input: PropertyDispositionCostInput,
    ) -> Result<i64, PropertyError> {
        if input.gross_sale_price_krw <= 0
            || !(1..=LOAN_RATIO_SCALE_PPM).contains(&input.disposition_cost_rate_ppm)
            || input.minimum_disposition_cost_krw <= 0
        {
            return Err(PropertyError::InvalidDispositionCost);
        }

        let cost_krw = i128::from(input.gross_sale_price_krw)
            .checked_mul(i128::from(input.disposition_cost_rate_ppm))
            .ok_or(PropertyError::ArithmeticOverflow)?
            .checked_div(i128::from(LOAN_RATIO_SCALE_PPM))
            .ok_or(PropertyError::ArithmeticOverflow)?
            .max(i128::from(input.minimum_disposition_cost_krw));
        i64::try_from(cost_krw).map_err(|_| PropertyError::ArithmeticOverflow)
    }

    fn plan_sale_proceeds(
        &self,
        input: PropertySaleProceedsInput,
    ) -> Result<PropertySaleProceedsPlan, PropertyError> {
        if input.gross_sale_price_krw <= 0
            || input.property_book_value_krw <= 0
            || input.disposition_cost_krw <= 0
            || input.mortgage_principal_payoff_krw < 0
            || input.mortgage_prepayment_fee_krw < 0
            || input.national_capital_gains_tax_krw < 0
            || input.local_capital_gains_tax_krw < 0
        {
            return Err(PropertyError::InvalidSaleProceeds);
        }

        let total_capital_gains_tax_krw = i64::try_from(
            i128::from(input.national_capital_gains_tax_krw)
                .checked_add(i128::from(input.local_capital_gains_tax_krw))
                .ok_or(PropertyError::ArithmeticOverflow)?,
        )
        .map_err(|_| PropertyError::ArithmeticOverflow)?;
        let total_deductions_krw = i128::from(input.disposition_cost_krw)
            .checked_add(i128::from(input.mortgage_principal_payoff_krw))
            .and_then(|value| value.checked_add(i128::from(input.mortgage_prepayment_fee_krw)))
            .and_then(|value| value.checked_add(i128::from(total_capital_gains_tax_krw)))
            .ok_or(PropertyError::ArithmeticOverflow)?;
        let wallet_proceeds_krw = i128::from(input.gross_sale_price_krw)
            .checked_sub(total_deductions_krw)
            .ok_or(PropertyError::ArithmeticOverflow)?;
        if wallet_proceeds_krw < 0 {
            return Err(PropertyError::InsufficientSaleProceeds);
        }
        let wallet_proceeds_krw =
            i64::try_from(wallet_proceeds_krw).map_err(|_| PropertyError::ArithmeticOverflow)?;
        let realized_gain_loss_krw = input
            .property_book_value_krw
            .checked_sub(input.gross_sale_price_krw)
            .ok_or(PropertyError::ArithmeticOverflow)?;

        let mut postings = Vec::with_capacity(8);
        push_sale_posting(
            &mut postings,
            LedgerAccountCode::PropertyAsset,
            None,
            input
                .property_book_value_krw
                .checked_neg()
                .ok_or(PropertyError::ArithmeticOverflow)?,
        );
        push_sale_posting(
            &mut postings,
            LedgerAccountCode::RealizedGainLoss,
            None,
            realized_gain_loss_krw,
        );
        push_sale_posting(
            &mut postings,
            LedgerAccountCode::PropertyDispositionExpense,
            None,
            input.disposition_cost_krw,
        );
        push_sale_posting(
            &mut postings,
            LedgerAccountCode::LoanPrincipalLiability,
            None,
            input.mortgage_principal_payoff_krw,
        );
        push_sale_posting(
            &mut postings,
            LedgerAccountCode::LoanFeeExpense,
            None,
            input.mortgage_prepayment_fee_krw,
        );
        push_sale_posting(
            &mut postings,
            LedgerAccountCode::PropertyTaxExpense,
            Some(CapitalGainsTaxScope::National),
            input.national_capital_gains_tax_krw,
        );
        push_sale_posting(
            &mut postings,
            LedgerAccountCode::PropertyTaxExpense,
            Some(CapitalGainsTaxScope::Local),
            input.local_capital_gains_tax_krw,
        );
        push_sale_posting(
            &mut postings,
            LedgerAccountCode::Wallet,
            None,
            wallet_proceeds_krw,
        );
        let posting_sum = postings.iter().try_fold(0_i128, |sum, posting| {
            sum.checked_add(i128::from(posting.amount_krw))
                .ok_or(PropertyError::ArithmeticOverflow)
        })?;
        if posting_sum != 0 {
            return Err(PropertyError::ArithmeticOverflow);
        }

        Ok(PropertySaleProceedsPlan {
            gross_sale_price_krw: input.gross_sale_price_krw,
            property_book_value_krw: input.property_book_value_krw,
            disposition_cost_krw: input.disposition_cost_krw,
            mortgage_principal_payoff_krw: input.mortgage_principal_payoff_krw,
            mortgage_prepayment_fee_krw: input.mortgage_prepayment_fee_krw,
            national_capital_gains_tax_krw: input.national_capital_gains_tax_krw,
            local_capital_gains_tax_krw: input.local_capital_gains_tax_krw,
            total_capital_gains_tax_krw,
            wallet_proceeds_krw,
            postings,
        })
    }
}

fn validate_sale_liquidity(liquidity: PropertySaleLiquidityProfile) -> Result<(), PropertyError> {
    if liquidity.minimum_asking_ratio_ppm <= 0
        || liquidity.fast_band_maximum_asking_ratio_ppm < liquidity.minimum_asking_ratio_ppm
        || liquidity.normal_band_maximum_asking_ratio_ppm
            <= liquidity.fast_band_maximum_asking_ratio_ppm
        || liquidity.maximum_asking_ratio_ppm <= liquidity.normal_band_maximum_asking_ratio_ppm
        || liquidity.fast_band_minimum_delay_days == 0
        || liquidity.fast_band_minimum_delay_days > liquidity.fast_band_maximum_delay_days
        || liquidity.normal_band_minimum_delay_days == 0
        || liquidity.normal_band_minimum_delay_days > liquidity.normal_band_maximum_delay_days
        || liquidity.slow_band_minimum_delay_days == 0
        || liquidity.slow_band_minimum_delay_days > liquidity.slow_band_maximum_delay_days
        || !(1..=LOAN_RATIO_SCALE_PPM).contains(&liquidity.disposition_cost_rate_ppm)
        || liquidity.minimum_disposition_cost_krw <= 0
    {
        return Err(PropertyError::InvalidSaleLiquidityProfile);
    }
    Ok(())
}

fn sample_sale_delay(
    input: PropertySaleCandidateInput,
    minimum_delay_days: u16,
    maximum_delay_days: u16,
) -> Result<u16, PropertyError> {
    let exclusive_upper = u64::from(maximum_delay_days)
        .checked_sub(u64::from(minimum_delay_days))
        .and_then(|value| value.checked_add(1))
        .ok_or(PropertyError::ArithmeticOverflow)?;
    let rejection_threshold = exclusive_upper.wrapping_neg() % exclusive_upper;
    for counter in 0..=u32::MAX {
        let mut digest = Sha256::new();
        digest.update(input.world_seed.to_be_bytes());
        digest.update(input.listing_id.get().to_be_bytes());
        digest.update(input.order_revision.to_be_bytes());
        digest.update(PROPERTY_SALE_CANDIDATE_ENTROPY_TAG);
        digest.update(counter.to_be_bytes());
        let digest: [u8; 32] = digest.finalize().into();
        let word = u64::from_be_bytes([
            digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
        ]);
        if word >= rejection_threshold {
            let offset = word % exclusive_upper;
            let delay_days = u64::from(minimum_delay_days)
                .checked_add(offset)
                .ok_or(PropertyError::ArithmeticOverflow)?;
            return u16::try_from(delay_days).map_err(|_| PropertyError::ArithmeticOverflow);
        }
    }
    Err(PropertyError::EntropyExhausted)
}

fn completed_calendar_years(start: Date, end: Date) -> Result<u16, PropertyError> {
    if end < start {
        return Err(PropertyError::InvalidSalePeriod);
    }
    let year_difference = end
        .year()
        .checked_sub(start.year())
        .ok_or(PropertyError::ArithmeticOverflow)?;
    let year_difference =
        u16::try_from(year_difference).map_err(|_| PropertyError::ArithmeticOverflow)?;
    let anniversary = add_years_clamped(start, year_difference)?;
    if end >= anniversary {
        Ok(year_difference)
    } else {
        year_difference
            .checked_sub(1)
            .ok_or(PropertyError::ArithmeticOverflow)
    }
}

fn add_years_clamped(date: Date, years: u16) -> Result<Date, PropertyError> {
    let target_year = date
        .year()
        .checked_add(i32::from(years))
        .ok_or(PropertyError::ArithmeticOverflow)?;
    for day in (1..=date.day()).rev() {
        if let Ok(candidate) = Date::from_calendar_date(target_year, date.month(), day) {
            return Ok(candidate);
        }
    }
    Err(PropertyError::InvalidSalePeriod)
}

fn push_sale_posting(
    postings: &mut Vec<PropertySaleLedgerPosting>,
    account_code: LedgerAccountCode,
    capital_gains_tax_scope: Option<CapitalGainsTaxScope>,
    amount_krw: i64,
) {
    if amount_krw != 0 {
        postings.push(PropertySaleLedgerPosting {
            account_code,
            capital_gains_tax_scope,
            amount_krw,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finance::ResourceId;
    use crate::life::MortgageRegionalPriceCapPolicy;
    use time::Month;

    fn given_regulated_capital_policy() -> MortgageRegionalPriceCapPolicy {
        MortgageRegionalPriceCapPolicy {
            lower_price_threshold_krw: 1_500_000_000,
            upper_price_threshold_krw: 2_500_000_000,
            lower_band_cap_krw: 600_000_000,
            middle_band_cap_krw: 400_000_000,
            upper_band_cap_krw: 200_000_000,
        }
    }

    fn given_sale_liquidity() -> PropertySaleLiquidityProfile {
        PropertySaleLiquidityProfile {
            minimum_asking_ratio_ppm: 800_000,
            fast_band_maximum_asking_ratio_ppm: 950_000,
            normal_band_maximum_asking_ratio_ppm: 1_050_000,
            maximum_asking_ratio_ppm: 1_200_000,
            fast_band_minimum_delay_days: 1,
            fast_band_maximum_delay_days: 3,
            normal_band_minimum_delay_days: 3,
            normal_band_maximum_delay_days: 7,
            slow_band_minimum_delay_days: 7,
            slow_band_maximum_delay_days: 30,
            disposition_cost_rate_ppm: 5_000,
            minimum_disposition_cost_krw: 1,
        }
    }

    fn given_sale_candidate(order_revision: u32) -> PropertySaleCandidateInput {
        PropertySaleCandidateInput {
            world_seed: 42,
            listing_id: ResourceId::from_u64(17),
            order_revision,
            current_game_day: 100,
            reference_value_krw: 100_000_000,
            asking_price_krw: 110_000_000,
            liquidity: given_sale_liquidity(),
        }
    }

    fn given_date(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("테스트 날짜는 유효해야 한다")
    }

    mod context_1퍼센트_취득부대비용을_계산하는_경우 {
        use super::*;

        #[test]
        fn given_원단위로_나누어떨어지지않는_매수가_when_계산하면_then_독립내림한다() {
            let rules = create_property_rules();

            let result =
                rules.calculate_acquisition_incidental_cost(AcquisitionIncidentalCostInput {
                    purchase_price_krw: 123_456_789,
                    cost_ppm: 10_000,
                });

            assert_eq!(result, Ok(1_234_567));
        }

        #[test]
        fn given_내림결과가_0원인_매수가_when_계산하면_then_최소_1원이다() {
            let rules = create_property_rules();

            let result =
                rules.calculate_acquisition_incidental_cost(AcquisitionIncidentalCostInput {
                    purchase_price_krw: 1,
                    cost_ppm: 10_000,
                });

            assert_eq!(result, Ok(1));
        }
    }

    mod context_ltv와_지역가격한도와_상품한도가_함께_있는_경우 {
        use super::*;

        #[test]
        fn given_지역가격한도가_가장작을때_when_계산하면_then_그_한도를_선택한다() {
            let rules = create_property_rules();

            let result = rules.calculate_mortgage_funding_limit(MortgageFundingLimitInput {
                recognized_collateral_value_krw: 1_600_000_000,
                ltv_limit_ppm: 400_000,
                regional_price_cap_krw: Some(400_000_000),
                product_maximum_principal_krw: 600_000_000,
            });

            assert_eq!(
                result,
                Ok(MortgageFundingLimit {
                    ltv_based_limit_krw: 640_000_000,
                    regional_price_cap_krw: Some(400_000_000),
                    maximum_mortgage_krw: 400_000_000,
                })
            );
        }

        #[test]
        fn given_비규제proxy의_5억원주택_when_계산하면_then_70퍼센트까지다() {
            let rules = create_property_rules();

            let result = rules.calculate_mortgage_funding_limit(MortgageFundingLimitInput {
                recognized_collateral_value_krw: 500_000_000,
                ltv_limit_ppm: 700_000,
                regional_price_cap_krw: None,
                product_maximum_principal_krw: 600_000_000,
            });

            assert_eq!(
                result.map(|limit| limit.maximum_mortgage_krw),
                Ok(350_000_000)
            );
        }
    }

    mod context_수도권규제proxy_가격상한을_선택하는_경우 {
        use super::*;

        #[test]
        fn given_담보가치가_15억원일때_when_선택하면_then_6억원이다() {
            let rules = create_property_rules();

            let result = rules.select_mortgage_regional_price_cap(MortgageRegionalPriceCapInput {
                recognized_collateral_value_krw: 1_500_000_000,
                policy: Some(given_regulated_capital_policy()),
            });

            assert_eq!(result, Ok(Some(600_000_000)));
        }

        #[test]
        fn given_담보가치가_15억원을_1원초과할때_when_선택하면_then_4억원이다() {
            let rules = create_property_rules();

            let result = rules.select_mortgage_regional_price_cap(MortgageRegionalPriceCapInput {
                recognized_collateral_value_krw: 1_500_000_001,
                policy: Some(given_regulated_capital_policy()),
            });

            assert_eq!(result, Ok(Some(400_000_000)));
        }

        #[test]
        fn given_담보가치가_25억원일때_when_선택하면_then_4억원이다() {
            let rules = create_property_rules();

            let result = rules.select_mortgage_regional_price_cap(MortgageRegionalPriceCapInput {
                recognized_collateral_value_krw: 2_500_000_000,
                policy: Some(given_regulated_capital_policy()),
            });

            assert_eq!(result, Ok(Some(400_000_000)));
        }

        #[test]
        fn given_담보가치가_25억원을_1원초과할때_when_선택하면_then_2억원이다() {
            let rules = create_property_rules();

            let result = rules.select_mortgage_regional_price_cap(MortgageRegionalPriceCapInput {
                recognized_collateral_value_krw: 2_500_000_001,
                policy: Some(given_regulated_capital_policy()),
            });

            assert_eq!(result, Ok(Some(200_000_000)));
        }
    }

    mod context_비규제proxy_가격상한을_선택하는_경우 {
        use super::*;

        #[test]
        fn given_지역가격상한정책이_없을때_when_선택하면_then_상한이없다() {
            let rules = create_property_rules();

            let result = rules.select_mortgage_regional_price_cap(MortgageRegionalPriceCapInput {
                recognized_collateral_value_krw: 3_000_000_000,
                policy: None,
            });

            assert_eq!(result, Ok(None));
        }
    }

    mod context_전세보증금과_주담대로_주택을_매수하는_경우 {
        use super::*;

        #[test]
        fn given_자기자금이_충분할때_when_계획하면_then_wallet과_부채변화를_함께_계산한다() {
            let rules = create_property_rules();

            let result = rules.plan_purchase_funding(PropertyPurchaseFundingInput {
                wallet_cash_krw: 100_000_000,
                returned_deposit_krw: 50_000_000,
                repaid_loan_principal_krw: 40_000_000,
                purchase_price_krw: 200_000_000,
                acquisition_incidental_cost_krw: 2_000_000,
                moving_cost_krw: 1_000_000,
                new_mortgage_principal_krw: 120_000_000,
            });

            let plan = result.expect("purchase funding must be valid");
            assert_eq!(plan.available_buyer_cash_krw, 110_000_000);
            assert_eq!(plan.required_buyer_cash_krw, 83_000_000);
            assert_eq!(plan.wallet_cash_after_krw, 27_000_000);
            assert_eq!(plan.wallet_delta_krw, -73_000_000);
            assert_eq!(plan.debt_delta_krw, 80_000_000);
            assert_eq!(plan.property_book_value_delta_krw, 200_000_000);
        }
    }

    mod context_최종_자기자금이_부족한_경우 {
        use super::*;

        #[test]
        fn given_wallet과_순보증금이_필요액보다작을때_when_계획하면_then_거절한다() {
            let rules = create_property_rules();

            let result = rules.plan_purchase_funding(PropertyPurchaseFundingInput {
                wallet_cash_krw: 10_000_000,
                returned_deposit_krw: 0,
                repaid_loan_principal_krw: 0,
                purchase_price_krw: 200_000_000,
                acquisition_incidental_cost_krw: 2_000_000,
                moving_cost_krw: 1_000_000,
                new_mortgage_principal_krw: 120_000_000,
            });

            assert_eq!(result, Err(PropertyError::InsufficientWalletCash));
        }
    }

    mod context_현재_지역지수로_매도_기준가를_계산하는_경우 {
        use super::*;

        #[test]
        fn given_취득시점보다_지수가_상승했을때_when_계산하면_then_원단위로_내림한다() {
            let rules = create_property_rules();

            let result = rules.calculate_sale_reference_value(PropertySaleReferenceValueInput {
                acquisition_price_krw: 123_456_789,
                acquisition_price_index_ppm: 1_000_000,
                current_price_index_ppm: 1_100_001,
            });

            assert_eq!(result, Ok(135_802_591));
        }

        #[test]
        fn given_i64범위를_넘는_기준가_when_계산하면_then_overflow로_거절한다() {
            let rules = create_property_rules();

            let result = rules.calculate_sale_reference_value(PropertySaleReferenceValueInput {
                acquisition_price_krw: i64::MAX,
                acquisition_price_index_ppm: 1,
                current_price_index_ppm: i64::MAX,
            });

            assert_eq!(result, Err(PropertyError::ArithmeticOverflow));
        }
    }

    mod context_매도_주문가가_기준가_허용범위에_있는_경우 {
        use super::*;

        #[test]
        fn given_정확히_최저비율인_주문가_when_후보를_계획하면_then_fast구간이다() {
            let rules = create_property_rules();
            let mut input = given_sale_candidate(1);
            input.asking_price_krw = 80_000_000;

            let result = rules
                .plan_sale_candidate(input)
                .expect("최저 경계 주문가는 허용되어야 한다");

            assert_eq!(result.liquidity_band, PropertySaleLiquidityBand::Fast);
            assert!((1..=3).contains(&result.delay_days));
        }

        #[test]
        fn given_정확히_최고비율인_주문가_when_후보를_계획하면_then_slow구간이다() {
            let rules = create_property_rules();
            let mut input = given_sale_candidate(1);
            input.asking_price_krw = 120_000_000;

            let result = rules
                .plan_sale_candidate(input)
                .expect("최고 경계 주문가는 허용되어야 한다");

            assert_eq!(result.liquidity_band, PropertySaleLiquidityBand::Slow);
            assert!((7..=30).contains(&result.delay_days));
        }

        #[test]
        fn given_최고비율을_1원초과한_주문가_when_후보를_계획하면_then_거절한다() {
            let rules = create_property_rules();
            let mut input = given_sale_candidate(1);
            input.asking_price_krw = 120_000_001;

            let result = rules.plan_sale_candidate(input);

            assert_eq!(result, Err(PropertyError::AskingPriceOutOfRange));
        }

        #[test]
        fn given_최저비율보다_1원낮은_주문가_when_후보를_계획하면_then_거절한다() {
            let rules = create_property_rules();
            let mut input = given_sale_candidate(1);
            input.asking_price_krw = 79_999_999;

            let result = rules.plan_sale_candidate(input);

            assert_eq!(result, Err(PropertyError::AskingPriceOutOfRange));
        }
    }

    mod context_같은_매도_revision을_다시_계산하는_경우 {
        use super::*;

        #[test]
        fn given_같은_seed와_listing과_revision_when_두번_계획하면_then_같은_후보일이다() {
            let rules = create_property_rules();
            let input = given_sale_candidate(1);

            let first = rules.plan_sale_candidate(input);
            let replay = rules.plan_sale_candidate(input);

            assert_eq!(first, replay);
        }

        #[test]
        fn given_revision만_증가했을때_when_후보를_계획하면_then_새_entropy를_쓴다() {
            let rules = create_property_rules();

            let first = rules
                .plan_sale_candidate(given_sale_candidate(1))
                .expect("첫 revision 후보를 계산해야 한다");
            let repriced = rules
                .plan_sale_candidate(given_sale_candidate(2))
                .expect("가격 변경 revision 후보를 계산해야 한다");

            assert_ne!(first.delay_days, repriced.delay_days);
        }
    }

    mod context_달력상_2년_보유와_거주를_판정하는_경우 {
        use super::*;

        #[test]
        fn given_2주년_하루전_when_판정하면_then_매도할수없다() {
            let rules = create_property_rules();

            let result = rules
                .calculate_sale_period(PropertySalePeriodInput {
                    acquired_on: given_date(2026, Month::July, 27),
                    owner_occupied_from: given_date(2026, Month::July, 27),
                    as_of: given_date(2028, Month::July, 26),
                    minimum_holding_years: 2,
                    minimum_residence_years: 2,
                })
                .expect("2주년 전 보유기간을 계산해야 한다");

            assert!(!result.is_eligible);
            assert_eq!(result.completed_holding_years, 1);
        }

        #[test]
        fn given_정확히_2주년_when_판정하면_then_매도할수있다() {
            let rules = create_property_rules();

            let result = rules
                .calculate_sale_period(PropertySalePeriodInput {
                    acquired_on: given_date(2026, Month::July, 27),
                    owner_occupied_from: given_date(2026, Month::July, 27),
                    as_of: given_date(2028, Month::July, 27),
                    minimum_holding_years: 2,
                    minimum_residence_years: 2,
                })
                .expect("2주년 보유기간을 계산해야 한다");

            assert!(result.is_eligible);
            assert_eq!(result.completed_residence_years, 2);
        }

        #[test]
        fn given_윤일에_취득했을때_when_평년_2월말에_판정하면_then_달력_1년을_완료한다() {
            let rules = create_property_rules();

            let result = rules
                .calculate_sale_period(PropertySalePeriodInput {
                    acquired_on: given_date(2028, Month::February, 29),
                    owner_occupied_from: given_date(2028, Month::February, 29),
                    as_of: given_date(2029, Month::February, 28),
                    minimum_holding_years: 2,
                    minimum_residence_years: 2,
                })
                .expect("윤일 기념일을 월말로 고정해야 한다");

            assert_eq!(result.completed_holding_years, 1);
        }
    }

    mod context_매도_거래비용을_계산하는_경우 {
        use super::*;

        #[test]
        fn given_5천ppm과_원단위_나머지_when_계산하면_then_독립내림한다() {
            let rules = create_property_rules();

            let result = rules.calculate_disposition_cost(PropertyDispositionCostInput {
                gross_sale_price_krw: 123_456_789,
                disposition_cost_rate_ppm: 5_000,
                minimum_disposition_cost_krw: 1,
            });

            assert_eq!(result, Ok(617_283));
        }

        #[test]
        fn given_내림결과가_0원인_매도가_when_계산하면_then_최소_1원이다() {
            let rules = create_property_rules();

            let result = rules.calculate_disposition_cost(PropertyDispositionCostInput {
                gross_sale_price_krw: 1,
                disposition_cost_rate_ppm: 5_000,
                minimum_disposition_cost_krw: 1,
            });

            assert_eq!(result, Ok(1));
        }
    }

    mod context_매도대금으로_비용과_담보와_세금을_모두_지급하는_경우 {
        use super::*;

        #[test]
        fn given_충분한_매도대금_when_계획하면_then_waterfall과_원장합이_일치한다() {
            let rules = create_property_rules();

            let plan = rules
                .plan_sale_proceeds(PropertySaleProceedsInput {
                    gross_sale_price_krw: 1_500_000_000,
                    property_book_value_krw: 800_000_000,
                    disposition_cost_krw: 7_500_000,
                    mortgage_principal_payoff_krw: 300_000_000,
                    mortgage_prepayment_fee_krw: 3_000_000,
                    national_capital_gains_tax_krw: 25_000_000,
                    local_capital_gains_tax_krw: 2_500_000,
                })
                .expect("매도대금 waterfall을 계획해야 한다");
            let posting_sum = plan
                .postings
                .iter()
                .map(|posting| i128::from(posting.amount_krw))
                .sum::<i128>();

            assert_eq!(plan.wallet_proceeds_krw, 1_162_000_000);
            assert_eq!(plan.total_capital_gains_tax_krw, 27_500_000);
            assert_eq!(posting_sum, 0);
        }

        #[test]
        fn given_공제액이_매도대금보다_큰경우_when_계획하면_then_부족대금을_거절한다() {
            let rules = create_property_rules();

            let result = rules.plan_sale_proceeds(PropertySaleProceedsInput {
                gross_sale_price_krw: 100_000_000,
                property_book_value_krw: 80_000_000,
                disposition_cost_krw: 500_000,
                mortgage_principal_payoff_krw: 100_000_000,
                mortgage_prepayment_fee_krw: 0,
                national_capital_gains_tax_krw: 0,
                local_capital_gains_tax_krw: 0,
            });

            assert_eq!(result, Err(PropertyError::InsufficientSaleProceeds));
        }

        #[test]
        fn given_두_양도세_component합이_i64을_넘을때_when_계획하면_then_overflow로_거절한다() {
            let rules = create_property_rules();

            let result = rules.plan_sale_proceeds(PropertySaleProceedsInput {
                gross_sale_price_krw: i64::MAX,
                property_book_value_krw: 1,
                disposition_cost_krw: 1,
                mortgage_principal_payoff_krw: 0,
                mortgage_prepayment_fee_krw: 0,
                national_capital_gains_tax_krw: i64::MAX,
                local_capital_gains_tax_krw: 1,
            });

            assert_eq!(result, Err(PropertyError::ArithmeticOverflow));
        }
    }
}
