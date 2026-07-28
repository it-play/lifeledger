use std::sync::Arc;

use super::types::*;

struct V1CorporationRules;

pub fn create_corporation_rules() -> Arc<dyn CorporationRules> {
    Arc::new(V1CorporationRules)
}

impl CorporationRules for V1CorporationRules {
    fn plan_establishment(
        &self,
        input: CorporationEstablishmentInput<'_>,
    ) -> Result<CorporationEstablishmentPlan, CorporationError> {
        plan_establishment(input)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
