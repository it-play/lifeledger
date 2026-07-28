use std::sync::Arc;

use sha2::{Digest, Sha256};

use super::types::*;

const ENTROPY_DOMAIN: &str = "lifeledger.corporation.month.v1";
const HMAC_BLOCK_BYTES: usize = 64;

struct V1CorporationRules {
    entropy: Arc<dyn CorporationMonthEntropy>,
}

struct HmacSha256CorporationMonthEntropy;

pub fn create_corporation_rules() -> Arc<dyn CorporationRules> {
    create_corporation_rules_with_entropy(Arc::new(HmacSha256CorporationMonthEntropy))
}

pub fn create_corporation_rules_with_entropy(
    entropy: Arc<dyn CorporationMonthEntropy>,
) -> Arc<dyn CorporationRules> {
    Arc::new(V1CorporationRules { entropy })
}

impl CorporationMonthEntropy for HmacSha256CorporationMonthEntropy {
    fn digest(
        &self,
        world_seed: u64,
        canonical_message: &[u8],
    ) -> Result<[u8; 32], CorporationError> {
        Ok(hmac_sha256(&world_seed.to_be_bytes(), canonical_message))
    }
}

impl CorporationRules for V1CorporationRules {
    fn plan_establishment(
        &self,
        input: CorporationEstablishmentInput<'_>,
    ) -> Result<CorporationEstablishmentPlan, CorporationError> {
        plan_establishment(input)
    }

    fn plan_operating_month(
        &self,
        input: CorporationOperatingMonthInput,
    ) -> Result<CorporationOperatingMonthPlan, CorporationError> {
        plan_operating_month(self.entropy.as_ref(), input)
    }

    fn plan_officer_payroll(
        &self,
        input: CorporationOfficerPayrollInput,
    ) -> Result<CorporationOfficerPayrollPlan, CorporationError> {
        plan_officer_payroll(input)
    }

    fn plan_corporate_tax(
        &self,
        input: CorporationTaxInput<'_>,
    ) -> Result<CorporationTaxPlan, CorporationError> {
        plan_corporate_tax(input)
    }

    fn plan_dividend(
        &self,
        input: CorporationDividendInput,
    ) -> Result<CorporationDividendPlan, CorporationError> {
        plan_dividend(input)
    }
}

fn plan_corporate_tax(
    input: CorporationTaxInput<'_>,
) -> Result<CorporationTaxPlan, CorporationError> {
    validate_tax_brackets(input.policy.national_brackets)?;
    validate_tax_brackets(input.policy.local_brackets)?;
    if !(-CORPORATION_MAX_PUBLIC_MONEY_KRW..=CORPORATION_MAX_PUBLIC_MONEY_KRW)
        .contains(&input.annual_pre_tax_profit_krw)
    {
        return Err(CorporationError::InvalidTaxPolicy);
    }
    let tax_base_krw = input.annual_pre_tax_profit_krw.max(0);
    let corporate_income_tax_krw =
        calculate_bracket_tax(tax_base_krw, input.policy.national_brackets)?;
    let local_corporate_income_tax_krw =
        calculate_bracket_tax(tax_base_krw, input.policy.local_brackets)?;
    let total_tax_krw = corporate_income_tax_krw
        .checked_add(local_corporate_income_tax_krw)
        .ok_or(CorporationError::ArithmeticOverflow)?;
    let after_tax_profit_krw = input
        .annual_pre_tax_profit_krw
        .checked_sub(total_tax_krw)
        .ok_or(CorporationError::ArithmeticOverflow)?;
    Ok(CorporationTaxPlan {
        tax_base_krw,
        corporate_income_tax_krw,
        local_corporate_income_tax_krw,
        total_tax_krw,
        after_tax_profit_krw,
    })
}

fn validate_tax_brackets(brackets: &[CorporationTaxBracket]) -> Result<(), CorporationError> {
    if brackets.is_empty() {
        return Err(CorporationError::InvalidTaxPolicy);
    }
    let mut previous_maximum = 0_i64;
    for (index, bracket) in brackets.iter().enumerate() {
        if !(1..=1_000_000).contains(&bracket.rate_ppm)
            || !(0..=CORPORATION_MAX_PUBLIC_MONEY_KRW).contains(&bracket.progressive_deduction_krw)
            || bracket.maximum_tax_base_krw.is_none() != (index + 1 == brackets.len())
        {
            return Err(CorporationError::InvalidTaxPolicy);
        }
        if let Some(maximum) = bracket.maximum_tax_base_krw {
            if maximum <= previous_maximum || maximum > CORPORATION_MAX_PUBLIC_MONEY_KRW {
                return Err(CorporationError::InvalidTaxPolicy);
            }
            previous_maximum = maximum;
        }
    }
    Ok(())
}

fn calculate_bracket_tax(
    tax_base_krw: i64,
    brackets: &[CorporationTaxBracket],
) -> Result<i64, CorporationError> {
    if tax_base_krw == 0 {
        return Ok(0);
    }
    let bracket = brackets
        .iter()
        .find(|bracket| {
            bracket
                .maximum_tax_base_krw
                .is_none_or(|maximum| tax_base_krw <= maximum)
        })
        .ok_or(CorporationError::InvalidTaxPolicy)?;
    let calculated = i128::from(tax_base_krw)
        .checked_mul(i128::from(bracket.rate_ppm))
        .and_then(|amount| amount.checked_div(1_000_000))
        .and_then(|amount| amount.checked_sub(i128::from(bracket.progressive_deduction_krw)))
        .ok_or(CorporationError::ArithmeticOverflow)?;
    i64::try_from(calculated.max(0)).map_err(|_| CorporationError::ArithmeticOverflow)
}

fn plan_dividend(
    input: CorporationDividendInput,
) -> Result<CorporationDividendPlan, CorporationError> {
    if !(1..=CORPORATION_MAX_PUBLIC_MONEY_KRW).contains(&input.gross_dividend_krw)
        || !(0..=CORPORATION_MAX_PUBLIC_MONEY_KRW).contains(&input.distributable_profit_krw)
        || !(0..=CORPORATION_MAX_PUBLIC_MONEY_KRW).contains(&input.corporation_cash_krw)
        || !(1..=1_000_000).contains(&input.income_tax_rate_ppm)
        || !(1..=1_000_000).contains(&input.local_income_tax_on_income_tax_ppm)
    {
        return Err(CorporationError::InvalidDividend);
    }
    if input.gross_dividend_krw
        > input
            .distributable_profit_krw
            .min(input.corporation_cash_krw)
    {
        return Err(CorporationError::InsufficientDividendCapacity);
    }
    let withheld_income_tax_krw =
        checked_rate_amount(input.gross_dividend_krw, input.income_tax_rate_ppm)?;
    let withheld_local_income_tax_krw = checked_rate_amount(
        withheld_income_tax_krw,
        input.local_income_tax_on_income_tax_ppm,
    )?;
    let net_dividend_krw = input
        .gross_dividend_krw
        .checked_sub(withheld_income_tax_krw)
        .and_then(|amount| amount.checked_sub(withheld_local_income_tax_krw))
        .ok_or(CorporationError::ArithmeticOverflow)?;
    Ok(CorporationDividendPlan {
        gross_dividend_krw: input.gross_dividend_krw,
        withheld_income_tax_krw,
        withheld_local_income_tax_krw,
        net_dividend_krw,
        distributable_profit_after_krw: input
            .distributable_profit_krw
            .checked_sub(input.gross_dividend_krw)
            .ok_or(CorporationError::ArithmeticOverflow)?,
        corporation_cash_after_krw: input
            .corporation_cash_krw
            .checked_sub(net_dividend_krw)
            .ok_or(CorporationError::ArithmeticOverflow)?,
    })
}

fn checked_rate_amount(amount_krw: i64, rate_ppm: i64) -> Result<i64, CorporationError> {
    i64::try_from(
        i128::from(amount_krw)
            .checked_mul(i128::from(rate_ppm))
            .and_then(|amount| amount.checked_div(1_000_000))
            .ok_or(CorporationError::ArithmeticOverflow)?,
    )
    .map_err(|_| CorporationError::ArithmeticOverflow)
}

fn plan_officer_payroll(
    input: CorporationOfficerPayrollInput,
) -> Result<CorporationOfficerPayrollPlan, CorporationError> {
    let amounts = [
        input.cash_after_operating_krw,
        input.gross_salary_krw,
        input.employee_insurance_total_krw,
        input.employer_insurance_total_krw,
        input.withheld_income_tax_krw,
        input.withheld_local_income_tax_krw,
        input.net_salary_krw,
    ];
    if amounts
        .into_iter()
        .any(|amount| !(0..=CORPORATION_MAX_PUBLIC_MONEY_KRW).contains(&amount))
    {
        return Err(CorporationError::InvalidOfficerPayroll);
    }
    let deductions_krw = input
        .employee_insurance_total_krw
        .checked_add(input.withheld_income_tax_krw)
        .and_then(|amount| amount.checked_add(input.withheld_local_income_tax_krw))
        .ok_or(CorporationError::ArithmeticOverflow)?;
    if input
        .net_salary_krw
        .checked_add(deductions_krw)
        .ok_or(CorporationError::ArithmeticOverflow)?
        != input.gross_salary_krw
    {
        return Err(CorporationError::InvalidOfficerPayroll);
    }
    let total_payroll_cost_krw = input
        .gross_salary_krw
        .checked_add(input.employer_insurance_total_krw)
        .ok_or(CorporationError::ArithmeticOverflow)?;
    let withholding_liability_krw = deductions_krw
        .checked_add(input.employer_insurance_total_krw)
        .ok_or(CorporationError::ArithmeticOverflow)?;
    let paid = input.cash_after_operating_krw >= total_payroll_cost_krw;
    Ok(CorporationOfficerPayrollPlan {
        paid,
        total_payroll_cost_krw,
        withholding_liability_krw: if paid { withholding_liability_krw } else { 0 },
        corporation_cash_debit_krw: if paid { input.net_salary_krw } else { 0 },
        operating_payable_increase_krw: if paid { 0 } else { total_payroll_cost_krw },
        personal_wallet_credit_krw: if paid { input.net_salary_krw } else { 0 },
    })
}

fn plan_operating_month(
    entropy: &dyn CorporationMonthEntropy,
    input: CorporationOperatingMonthInput,
) -> Result<CorporationOperatingMonthPlan, CorporationError> {
    validate_operating_month(input)?;
    let canonical_message = format!(
        "{ENTROPY_DOMAIN}\0{}\0{}-{:02}\0{}",
        input.corporation_id.get(),
        input.operating_year,
        input.operating_month,
        input.stream
    );
    let digest = entropy.digest(input.world_seed, canonical_message.as_bytes())?;
    let entropy_word = first_u64(digest);
    let variation_span = u64::try_from(
        input
            .revenue_variation_ppm
            .checked_mul(2)
            .and_then(|value| value.checked_add(1))
            .ok_or(CorporationError::ArithmeticOverflow)?,
    )
    .map_err(|_| CorporationError::ArithmeticOverflow)?;
    let offset = i64::try_from(entropy_word % variation_span)
        .map_err(|_| CorporationError::ArithmeticOverflow)?
        .checked_sub(input.revenue_variation_ppm)
        .ok_or(CorporationError::ArithmeticOverflow)?;
    let shock_ppm = CORPORATION_RATIO_SCALE_PPM
        .checked_add(offset)
        .ok_or(CorporationError::ArithmeticOverflow)?;
    let revenue_krw = scaled_floor_twice(
        input.base_monthly_revenue_krw,
        input.scale.revenue_factor_ppm,
        shock_ppm,
    )?;
    let variable_cost_krw = scaled_floor(revenue_krw, input.variable_cost_ppm)?;
    let operating_expense_krw = variable_cost_krw
        .checked_add(input.fixed_monthly_cost_krw)
        .and_then(|value| value.checked_add(input.scale.fixed_cost_krw))
        .ok_or(CorporationError::ArithmeticOverflow)?;
    let pre_payroll_profit_krw = revenue_krw
        .checked_sub(operating_expense_krw)
        .ok_or(CorporationError::ArithmeticOverflow)?;

    Ok(CorporationOperatingMonthPlan {
        entropy_word,
        shock_ppm,
        revenue_krw,
        variable_cost_krw,
        base_fixed_cost_krw: input.fixed_monthly_cost_krw,
        scale_fixed_cost_krw: input.scale.fixed_cost_krw,
        operating_expense_krw,
        pre_payroll_profit_krw,
    })
}

fn validate_operating_month(input: CorporationOperatingMonthInput) -> Result<(), CorporationError> {
    if !(1..=9999).contains(&input.operating_year) || !(1..=12).contains(&input.operating_month) {
        return Err(CorporationError::InvalidOperatingMonth);
    }
    if !(1..=CORPORATION_MAX_PUBLIC_MONEY_KRW).contains(&input.base_monthly_revenue_krw)
        || !(0..=900_000).contains(&input.revenue_variation_ppm)
        || !(0..=CORPORATION_RATIO_SCALE_PPM).contains(&input.variable_cost_ppm)
        || !(0..=CORPORATION_MAX_PUBLIC_MONEY_KRW).contains(&input.fixed_monthly_cost_krw)
        || !(1..=3_000_000).contains(&input.scale.revenue_factor_ppm)
        || !(0..=CORPORATION_MAX_PUBLIC_MONEY_KRW).contains(&input.scale.fixed_cost_krw)
    {
        return Err(CorporationError::InvalidOperatingTerms);
    }
    Ok(())
}

fn plan_establishment(
    input: CorporationEstablishmentInput<'_>,
) -> Result<CorporationEstablishmentPlan, CorporationError> {
    validate_policy(input.policy)?;
    validate_terms(input.terms)?;
    let canonical_name = validate_name(input.name)?;
    if !(input.terms.minimum_capital_krw..=input.terms.maximum_capital_krw)
        .contains(&input.capital_krw)
    {
        return Err(CorporationError::InvalidCapital);
    }
    if !(0..=CORPORATION_MAX_PUBLIC_MONEY_KRW).contains(&input.wallet_cash_krw) {
        return Err(CorporationError::InvalidWalletCash);
    }

    let proportional_tax = scaled_floor(
        input.capital_krw,
        input.policy.registration_license_tax_rate_ppm,
    )?;
    let registration_license_tax_krw =
        proportional_tax.max(input.policy.minimum_registration_license_tax_krw);
    let local_education_tax_krw = scaled_floor(
        registration_license_tax_krw,
        input.policy.local_education_tax_rate_ppm,
    )?;
    let total_fee_krw = registration_license_tax_krw
        .checked_add(local_education_tax_krw)
        .and_then(|amount| amount.checked_add(input.terms.game_administrative_fee_krw))
        .ok_or(CorporationError::ArithmeticOverflow)?;
    let wallet_debit_krw = input
        .capital_krw
        .checked_add(total_fee_krw)
        .ok_or(CorporationError::ArithmeticOverflow)?;
    if wallet_debit_krw > input.wallet_cash_krw {
        return Err(CorporationError::InsufficientWalletCash);
    }
    let wallet_cash_after_krw = input
        .wallet_cash_krw
        .checked_sub(wallet_debit_krw)
        .ok_or(CorporationError::ArithmeticOverflow)?;

    Ok(CorporationEstablishmentPlan {
        canonical_name,
        capital_krw: input.capital_krw,
        charges: CorporationRegistrationCharges {
            registration_license_tax_krw,
            local_education_tax_krw,
            game_administrative_fee_krw: input.terms.game_administrative_fee_krw,
            total_fee_krw,
        },
        wallet_debit_krw,
        wallet_cash_after_krw,
        corporation_cash_after_krw: input.capital_krw,
    })
}

fn validate_policy(policy: CorporationRegistrationPolicy) -> Result<(), CorporationError> {
    if policy.registered_office_class != CorporationRegisteredOfficeClass::StandardRegisteredOffice
    {
        return Err(CorporationError::UnsupportedRegisteredOffice);
    }
    if !(1..=CORPORATION_RATIO_SCALE_PPM).contains(&policy.registration_license_tax_rate_ppm)
        || !(1..=CORPORATION_MAX_PUBLIC_MONEY_KRW)
            .contains(&policy.minimum_registration_license_tax_krw)
        || !(1..=CORPORATION_RATIO_SCALE_PPM).contains(&policy.local_education_tax_rate_ppm)
    {
        return Err(CorporationError::InvalidPolicy);
    }
    Ok(())
}

fn validate_terms(terms: CorporationEstablishmentTerms) -> Result<(), CorporationError> {
    if !(1..=CORPORATION_MAX_PUBLIC_MONEY_KRW).contains(&terms.minimum_capital_krw)
        || !(terms.minimum_capital_krw..=CORPORATION_MAX_PUBLIC_MONEY_KRW)
            .contains(&terms.maximum_capital_krw)
        || !(0..=CORPORATION_MAX_PUBLIC_MONEY_KRW).contains(&terms.game_administrative_fee_krw)
    {
        return Err(CorporationError::InvalidTerms);
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<String, CorporationError> {
    if name.trim() != name || !(2..=40).contains(&name.chars().count()) {
        return Err(CorporationError::InvalidName);
    }
    if !name.chars().all(|character| {
        character.is_ascii_alphanumeric() || character == ' ' || is_hangul(character)
    }) {
        return Err(CorporationError::InvalidName);
    }
    Ok(name.to_owned())
}

fn is_hangul(character: char) -> bool {
    matches!(character, '\u{1100}'..='\u{11ff}' | '\u{3130}'..='\u{318f}' | '\u{ac00}'..='\u{d7af}')
}

fn scaled_floor(amount_krw: i64, rate_ppm: i64) -> Result<i64, CorporationError> {
    let scaled = i128::from(amount_krw)
        .checked_mul(i128::from(rate_ppm))
        .ok_or(CorporationError::ArithmeticOverflow)?
        / i128::from(CORPORATION_RATIO_SCALE_PPM);
    i64::try_from(scaled).map_err(|_| CorporationError::ArithmeticOverflow)
}

fn scaled_floor_twice(
    amount_krw: i64,
    first_rate_ppm: i64,
    second_rate_ppm: i64,
) -> Result<i64, CorporationError> {
    let denominator = i128::from(CORPORATION_RATIO_SCALE_PPM)
        .checked_mul(i128::from(CORPORATION_RATIO_SCALE_PPM))
        .ok_or(CorporationError::ArithmeticOverflow)?;
    let scaled = i128::from(amount_krw)
        .checked_mul(i128::from(first_rate_ppm))
        .and_then(|value| value.checked_mul(i128::from(second_rate_ppm)))
        .ok_or(CorporationError::ArithmeticOverflow)?
        / denominator;
    i64::try_from(scaled).map_err(|_| CorporationError::ArithmeticOverflow)
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut normalized_key = [0_u8; HMAC_BLOCK_BYTES];
    if key.len() > HMAC_BLOCK_BYTES {
        let digest = Sha256::digest(key);
        normalized_key[..digest.len()].copy_from_slice(&digest);
    } else {
        normalized_key[..key.len()].copy_from_slice(key);
    }
    let mut inner_pad = [0x36_u8; HMAC_BLOCK_BYTES];
    let mut outer_pad = [0x5c_u8; HMAC_BLOCK_BYTES];
    for index in 0..HMAC_BLOCK_BYTES {
        inner_pad[index] ^= normalized_key[index];
        outer_pad[index] ^= normalized_key[index];
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(message);
    let inner_digest = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner_digest);
    outer.finalize().into()
}

fn first_u64(digest: [u8; 32]) -> u64 {
    u64::from_be_bytes([
        digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finance::ResourceId;

    struct FixedEntropy {
        expected_message: &'static [u8],
        entropy_word: u64,
    }

    impl CorporationMonthEntropy for FixedEntropy {
        fn digest(
            &self,
            _world_seed: u64,
            canonical_message: &[u8],
        ) -> Result<[u8; 32], CorporationError> {
            assert_eq!(canonical_message, self.expected_message);
            let mut digest = [0_u8; 32];
            digest[..8].copy_from_slice(&self.entropy_word.to_be_bytes());
            Ok(digest)
        }
    }

    fn given_policy() -> CorporationRegistrationPolicy {
        CorporationRegistrationPolicy {
            registered_office_class: CorporationRegisteredOfficeClass::StandardRegisteredOffice,
            registration_license_tax_rate_ppm: 4_000,
            minimum_registration_license_tax_krw: 112_500,
            local_education_tax_rate_ppm: 200_000,
        }
    }

    fn given_terms() -> CorporationEstablishmentTerms {
        CorporationEstablishmentTerms {
            minimum_capital_krw: 1_000_000,
            maximum_capital_krw: 1_000_000_000,
            game_administrative_fee_krw: 30_000,
        }
    }

    fn when_planning(
        capital_krw: i64,
        wallet_cash_krw: i64,
    ) -> Result<CorporationEstablishmentPlan, CorporationError> {
        create_corporation_rules().plan_establishment(CorporationEstablishmentInput {
            name: "라이프 소프트",
            capital_krw,
            wallet_cash_krw,
            policy: given_policy(),
            terms: given_terms(),
        })
    }

    fn given_operating_input() -> CorporationOperatingMonthInput {
        CorporationOperatingMonthInput {
            world_seed: 20_260_101,
            corporation_id: ResourceId::from_u64(7),
            operating_year: 2026,
            operating_month: 8,
            stream: 0,
            base_monthly_revenue_krw: 8_000_000,
            revenue_variation_ppm: 350_000,
            variable_cost_ppm: 120_000,
            fixed_monthly_cost_krw: 2_400_000,
            scale: CorporationOperatingScaleTerms {
                revenue_factor_ppm: 1_000_000,
                fixed_cost_krw: 2_000_000,
            },
        }
    }

    fn when_planning_operating_month(
        input: CorporationOperatingMonthInput,
        entropy_word: u64,
    ) -> Result<CorporationOperatingMonthPlan, CorporationError> {
        create_corporation_rules_with_entropy(Arc::new(FixedEntropy {
            expected_message: b"lifeledger.corporation.month.v1\x007\x002026-08\x000",
            entropy_word,
        }))
        .plan_operating_month(input)
    }

    fn given_corporate_tax_policy() -> ([CorporationTaxBracket; 4], [CorporationTaxBracket; 4]) {
        (
            [
                CorporationTaxBracket {
                    maximum_tax_base_krw: Some(200_000_000),
                    rate_ppm: 100_000,
                    progressive_deduction_krw: 0,
                },
                CorporationTaxBracket {
                    maximum_tax_base_krw: Some(20_000_000_000),
                    rate_ppm: 200_000,
                    progressive_deduction_krw: 20_000_000,
                },
                CorporationTaxBracket {
                    maximum_tax_base_krw: Some(300_000_000_000),
                    rate_ppm: 220_000,
                    progressive_deduction_krw: 420_000_000,
                },
                CorporationTaxBracket {
                    maximum_tax_base_krw: None,
                    rate_ppm: 250_000,
                    progressive_deduction_krw: 9_420_000_000,
                },
            ],
            [
                CorporationTaxBracket {
                    maximum_tax_base_krw: Some(200_000_000),
                    rate_ppm: 10_000,
                    progressive_deduction_krw: 0,
                },
                CorporationTaxBracket {
                    maximum_tax_base_krw: Some(20_000_000_000),
                    rate_ppm: 20_000,
                    progressive_deduction_krw: 2_000_000,
                },
                CorporationTaxBracket {
                    maximum_tax_base_krw: Some(300_000_000_000),
                    rate_ppm: 22_000,
                    progressive_deduction_krw: 39_800_000,
                },
                CorporationTaxBracket {
                    maximum_tax_base_krw: None,
                    rate_ppm: 25_000,
                    progressive_deduction_krw: 655_800_000,
                },
            ],
        )
    }

    mod context_최저_등록면허세가_적용되는_경우 {
        use super::*;

        #[test]
        fn given_백만원_자본금_when_설립을_계획하면_then_최저세액과_지방교육세를_적용한다() {
            let result = when_planning(1_000_000, 2_000_000).expect("설립 계획이 유효해야 한다");

            assert_eq!(result.charges.registration_license_tax_krw, 112_500);
            assert_eq!(result.charges.local_education_tax_krw, 22_500);
            assert_eq!(result.charges.total_fee_krw, 165_000);
            assert_eq!(result.wallet_cash_after_krw, 835_000);
        }
    }

    mod context_비례_등록면허세가_최저액보다_큰_경우 {
        use super::*;

        #[test]
        fn given_일억원_자본금_when_설립을_계획하면_then_사천ppm을_적용한다() {
            let result =
                when_planning(100_000_000, 110_000_000).expect("설립 계획이 유효해야 한다");

            assert_eq!(result.charges.registration_license_tax_krw, 400_000);
            assert_eq!(result.charges.local_education_tax_krw, 80_000);
            assert_eq!(result.wallet_debit_krw, 100_510_000);
        }
    }

    mod context_지갑이_출자금과_설립비를_감당하지_못하는_경우 {
        use super::*;

        #[test]
        fn given_출자금만_있는_지갑_when_설립을_계획하면_then_잔액부족이다() {
            let result = when_planning(1_000_000, 1_000_000);

            assert_eq!(result, Err(CorporationError::InsufficientWalletCash));
        }
    }

    mod context_월간_entropy가_최저_매출충격을_고른_경우 {
        use super::*;

        #[test]
        fn given_소프트웨어_표준규모_when_월손익을_계획하면_then_canonical_message와_원단위_결과를_고정한다()
         {
            let result = when_planning_operating_month(given_operating_input(), 0)
                .expect("월 손익 계획이 유효해야 한다");

            assert_eq!(result.shock_ppm, 650_000);
            assert_eq!(result.revenue_krw, 5_200_000);
            assert_eq!(result.variable_cost_krw, 624_000);
            assert_eq!(result.operating_expense_krw, 5_024_000);
            assert_eq!(result.pre_payroll_profit_krw, 176_000);
        }
    }

    mod context_매출변동이_없는_성장규모인_경우 {
        use super::*;

        #[test]
        fn given_콘텐츠_성장규모_when_월손익을_계획하면_then_두_ppm과_고정비를_적용한다() {
            let mut input = given_operating_input();
            input.base_monthly_revenue_krw = 10_000_000;
            input.revenue_variation_ppm = 0;
            input.variable_cost_ppm = 250_000;
            input.fixed_monthly_cost_krw = 2_000_000;
            input.scale = CorporationOperatingScaleTerms {
                revenue_factor_ppm: 1_350_000,
                fixed_cost_krw: 5_000_000,
            };

            let result = when_planning_operating_month(input, u64::MAX)
                .expect("월 손익 계획이 유효해야 한다");

            assert_eq!(result.revenue_krw, 13_500_000);
            assert_eq!(result.operating_expense_krw, 10_375_000);
            assert_eq!(result.pre_payroll_profit_krw, 3_125_000);
        }
    }

    mod context_월간_catalog_값이_지원범위를_벗어난_경우 {
        use super::*;

        #[test]
        fn given_과도한_매출변동_when_월손익을_계획하면_then_catalog오류다() {
            let mut input = given_operating_input();
            input.revenue_variation_ppm = 900_001;

            let result = when_planning_operating_month(input, 0);

            assert_eq!(result, Err(CorporationError::InvalidOperatingTerms));
        }
    }

    mod context_법인현금이_급여총비용을_감당하는_경우 {
        use super::*;

        #[test]
        fn given_총비용이하의_법인현금_when_대표급여를_계획하면_then_개인지갑과_예수금을_기록한다()
        {
            let result = create_corporation_rules()
                .plan_officer_payroll(CorporationOfficerPayrollInput {
                    cash_after_operating_krw: 4_000_000,
                    gross_salary_krw: 3_000_000,
                    employee_insurance_total_krw: 270_000,
                    employer_insurance_total_krw: 330_000,
                    withheld_income_tax_krw: 70_000,
                    withheld_local_income_tax_krw: 7_000,
                    net_salary_krw: 2_653_000,
                })
                .expect("대표 급여 계획이 유효해야 한다");

            assert!(result.paid);
            assert_eq!(result.total_payroll_cost_krw, 3_330_000);
            assert_eq!(result.corporation_cash_debit_krw, 2_653_000);
            assert_eq!(result.withholding_liability_krw, 677_000);
            assert_eq!(result.personal_wallet_credit_krw, 2_653_000);
        }
    }

    mod context_법인현금이_급여총비용보다_적은_경우 {
        use super::*;

        #[test]
        fn given_순급여는_있지만_총비용이_부족할때_when_대표급여를_계획하면_then_전액미지급금으로_기록한다()
         {
            let result = create_corporation_rules()
                .plan_officer_payroll(CorporationOfficerPayrollInput {
                    cash_after_operating_krw: 3_000_000,
                    gross_salary_krw: 3_000_000,
                    employee_insurance_total_krw: 270_000,
                    employer_insurance_total_krw: 330_000,
                    withheld_income_tax_krw: 70_000,
                    withheld_local_income_tax_krw: 7_000,
                    net_salary_krw: 2_653_000,
                })
                .expect("대표 급여 계획이 유효해야 한다");

            assert!(!result.paid);
            assert_eq!(result.operating_payable_increase_krw, 3_330_000);
            assert_eq!(result.corporation_cash_debit_krw, 0);
            assert_eq!(result.withholding_liability_krw, 0);
            assert_eq!(result.personal_wallet_credit_krw, 0);
        }
    }

    mod context_법인세_두번째_구간의_마지막_원인_경우 {
        use super::*;

        #[test]
        fn given_과세표준_이억원초과_when_결산하면_then_국세와_지방세_누진공제를_각각_적용한다() {
            let (national, local) = given_corporate_tax_policy();

            let result = create_corporation_rules()
                .plan_corporate_tax(CorporationTaxInput {
                    annual_pre_tax_profit_krw: 200_000_001,
                    policy: CorporationTaxPolicy {
                        national_brackets: &national,
                        local_brackets: &local,
                    },
                })
                .expect("법인세 정책이 유효해야 한다");

            assert_eq!(result.corporate_income_tax_krw, 20_000_000);
            assert_eq!(result.local_corporate_income_tax_krw, 2_000_000);
            assert_eq!(result.after_tax_profit_krw, 178_000_001);
        }
    }

    mod context_결손_사업연도를_결산하는_경우 {
        use super::*;

        #[test]
        fn given_음수_세전손익_when_결산하면_then_과세표준과_세액은_영원이다() {
            let (national, local) = given_corporate_tax_policy();

            let result = create_corporation_rules()
                .plan_corporate_tax(CorporationTaxInput {
                    annual_pre_tax_profit_krw: -1,
                    policy: CorporationTaxPolicy {
                        national_brackets: &national,
                        local_brackets: &local,
                    },
                })
                .expect("결손도 유효한 결산 입력이다");

            assert_eq!(result.tax_base_krw, 0);
            assert_eq!(result.total_tax_krw, 0);
            assert_eq!(result.after_tax_profit_krw, -1);
        }
    }

    mod context_배당가능이익과_현금이_충분한_경우 {
        use super::*;

        #[test]
        fn given_일원_배당_when_지급을_계획하면_then_각_세액을_원단위_내림한다() {
            let result = create_corporation_rules()
                .plan_dividend(CorporationDividendInput {
                    gross_dividend_krw: 1,
                    distributable_profit_krw: 1,
                    corporation_cash_krw: 1,
                    income_tax_rate_ppm: 140_000,
                    local_income_tax_on_income_tax_ppm: 100_000,
                })
                .expect("배당 계획이 유효해야 한다");

            assert_eq!(result.withheld_income_tax_krw, 0);
            assert_eq!(result.withheld_local_income_tax_krw, 0);
            assert_eq!(result.net_dividend_krw, 1);
            assert_eq!(result.distributable_profit_after_krw, 0);
            assert_eq!(result.corporation_cash_after_krw, 0);
        }
    }
}
