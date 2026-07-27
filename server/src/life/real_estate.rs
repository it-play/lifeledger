use std::collections::HashSet;
use std::sync::Arc;

use sha2::{Digest, Sha256};

use super::types::{
    LifeRegionKey, PropertyListing, PropertyListingAvailabilityRule, PropertyListingEntropyKey,
    PropertyListingGenerationInput, PropertyListingOffer, PropertyOfferKind,
    PropertyOfferRotationRule, PropertyType, REAL_ESTATE_INDEX_SCALE_PPM,
    REAL_ESTATE_MAX_EXCLUSIVE_AREA_SQUARE_METERS, REAL_ESTATE_MAX_INDEX_PPM,
    REAL_ESTATE_MAX_LISTINGS_PER_REGION, REAL_ESTATE_MAX_PUBLIC_LISTING_ID,
    REAL_ESTATE_MAX_PUBLIC_MONEY_KRW, REAL_ESTATE_MAX_VARIATION_PPM, RealEstateDaily,
    RealEstateDayZeroInput, RealEstateError, RealEstateIndexState, RealEstateNextDayInput,
    RealEstateRegionProfile, RealEstateRules,
};
use crate::finance::ResourceId;

const INDEX_ENTROPY_DOMAIN: &[u8] = b"lifeledger.real-estate.index.v1\0";
const LISTING_ENTROPY_DOMAIN: &[u8] = b"lifeledger.real-estate.listing.v1\0";

#[derive(Clone, Copy)]
enum IndexSeries {
    Price,
    Rent,
}

impl IndexSeries {
    const fn tag(self) -> u8 {
        match self {
            Self::Price => 1,
            Self::Rent => 2,
        }
    }
}

#[derive(Clone, Copy)]
enum ListingEntropyStream {
    PublicId,
    PropertyType,
    ExclusiveArea,
    PriceVariation,
}

impl ListingEntropyStream {
    const fn tag(self) -> u8 {
        match self {
            Self::PublicId => 1,
            Self::PropertyType => 2,
            Self::ExclusiveArea => 3,
            Self::PriceVariation => 4,
        }
    }
}

struct V1RealEstateRules;

pub fn create_real_estate_rules() -> Arc<dyn RealEstateRules> {
    Arc::new(V1RealEstateRules)
}

impl RealEstateRules for V1RealEstateRules {
    fn day_zero(&self, input: RealEstateDayZeroInput) -> Result<RealEstateDaily, RealEstateError> {
        validate_profile(input.profile)?;

        Ok(RealEstateDaily {
            region_key: input.profile.region_key,
            game_day: 0,
            price: RealEstateIndexState {
                index_ppm: REAL_ESTATE_INDEX_SCALE_PPM,
                remainder_numerator: 0,
            },
            rent: RealEstateIndexState {
                index_ppm: REAL_ESTATE_INDEX_SCALE_PPM,
                remainder_numerator: 0,
            },
        })
    }

    fn next_day(&self, input: RealEstateNextDayInput) -> Result<RealEstateDaily, RealEstateError> {
        validate_profile(input.profile)?;
        if input.previous.region_key != input.profile.region_key {
            return Err(RealEstateError::InvalidPreviousDay);
        }
        validate_index_state(input.profile, input.previous.price)?;
        validate_index_state(input.profile, input.previous.rent)?;

        let game_day = input
            .previous
            .game_day
            .checked_add(1)
            .ok_or(RealEstateError::ArithmeticOverflow)?;
        let price = next_index_state(
            input.world_seed,
            input.model_version_id,
            input.profile,
            game_day,
            IndexSeries::Price,
            input.previous.price,
        )?;
        let rent = next_index_state(
            input.world_seed,
            input.model_version_id,
            input.profile,
            game_day,
            IndexSeries::Rent,
            input.previous.rent,
        )?;

        Ok(RealEstateDaily {
            region_key: input.profile.region_key,
            game_day,
            price,
            rent,
        })
    }

    fn stable_listing_id(
        &self,
        key: PropertyListingEntropyKey,
    ) -> Result<ResourceId, RealEstateError> {
        validate_listing_entropy_key(key)?;
        let value = sample_listing_inclusive(
            key,
            ListingEntropyStream::PublicId,
            1,
            REAL_ESTATE_MAX_PUBLIC_LISTING_ID,
        )?;
        Ok(ResourceId::from_u64(value))
    }

    fn generate_monthly_listings(
        &self,
        input: PropertyListingGenerationInput<'_>,
    ) -> Result<Vec<PropertyListing>, RealEstateError> {
        validate_listing_input(input)?;

        let mut listings =
            Vec::with_capacity(usize::from(input.profile.monthly_listing_slot_count));
        let mut listing_ids = HashSet::with_capacity(listings.capacity());
        for slot in 1..=input.profile.monthly_listing_slot_count {
            let key = PropertyListingEntropyKey {
                world_seed: input.world_seed,
                model_version_id: input.model_version_id,
                year_month: input.year_month,
                region_key: input.profile.region_key,
                slot,
            };
            let id = self.stable_listing_id(key)?;
            if !listing_ids.insert(id) {
                return Err(RealEstateError::ListingIdCollision(id));
            }

            let property_type_index = sample_listing_inclusive(
                key,
                ListingEntropyStream::PropertyType,
                0,
                u64::try_from(input.allowed_property_types.len() - 1)
                    .map_err(|_| RealEstateError::ArithmeticOverflow)?,
            )?;
            let property_type = input.allowed_property_types[usize::try_from(property_type_index)
                .map_err(|_| RealEstateError::ArithmeticOverflow)?];
            let exclusive_area_square_meters = u16::try_from(sample_listing_inclusive(
                key,
                ListingEntropyStream::ExclusiveArea,
                u64::from(input.profile.minimum_exclusive_area_square_meters),
                u64::from(input.profile.maximum_exclusive_area_square_meters),
            )?)
            .map_err(|_| RealEstateError::ArithmeticOverflow)?;
            let price_variation_ppm = i64::try_from(sample_listing_inclusive(
                key,
                ListingEntropyStream::PriceVariation,
                u64::try_from(input.profile.minimum_price_variation_ppm)
                    .map_err(|_| RealEstateError::ArithmeticOverflow)?,
                u64::try_from(input.profile.maximum_price_variation_ppm)
                    .map_err(|_| RealEstateError::ArithmeticOverflow)?,
            )?)
            .map_err(|_| RealEstateError::ArithmeticOverflow)?;
            let offer_kind = offer_kind_for_slot(slot)?;
            let offer = calculate_offer(
                input.profile,
                input.month_start_daily,
                exclusive_area_square_meters,
                price_variation_ppm,
                offer_kind,
            )?;

            listings.push(PropertyListing {
                id,
                year_month: input.year_month,
                region_key: input.profile.region_key,
                slot,
                property_type,
                exclusive_area_square_meters,
                price_variation_ppm,
                available_from_game_day: input.available_from_game_day,
                available_to_game_day: input.available_to_game_day,
                offers: vec![offer],
            });
        }

        Ok(listings)
    }
}

fn next_index_state(
    world_seed: u64,
    model_version_id: ResourceId,
    profile: RealEstateRegionProfile,
    game_day: u32,
    series: IndexSeries,
    previous: RealEstateIndexState,
) -> Result<RealEstateIndexState, RealEstateError> {
    let (drift_ppm, shock_amplitude_ppm) = match series {
        IndexSeries::Price => (
            profile.price_daily_drift_ppm,
            profile.price_daily_shock_amplitude_ppm,
        ),
        IndexSeries::Rent => (
            profile.rent_daily_drift_ppm,
            profile.rent_daily_shock_amplitude_ppm,
        ),
    };
    let shock_ppm = sample_index_signed(
        world_seed,
        model_version_id,
        profile.region_key,
        game_day,
        series,
        -shock_amplitude_ppm,
        shock_amplitude_ppm,
    )?;
    let factor_ppm = REAL_ESTATE_INDEX_SCALE_PPM
        .checked_add(drift_ppm)
        .and_then(|value| value.checked_add(shock_ppm))
        .ok_or(RealEstateError::ArithmeticOverflow)?;
    if factor_ppm <= 0 {
        return Err(RealEstateError::InvalidProfile);
    }

    let numerator = i128::from(previous.index_ppm)
        .checked_mul(i128::from(factor_ppm))
        .and_then(|value| value.checked_add(i128::from(previous.remainder_numerator)))
        .ok_or(RealEstateError::ArithmeticOverflow)?;
    let scale = i128::from(REAL_ESTATE_INDEX_SCALE_PPM);
    let quotient = numerator
        .checked_div(scale)
        .ok_or(RealEstateError::ArithmeticOverflow)?;
    let remainder = numerator
        .checked_rem(scale)
        .ok_or(RealEstateError::ArithmeticOverflow)?;
    let minimum = i128::from(profile.minimum_index_ppm);
    let maximum = i128::from(profile.maximum_index_ppm);
    let (index_ppm, remainder_numerator) = if quotient <= minimum {
        (profile.minimum_index_ppm, 0)
    } else if quotient >= maximum {
        (profile.maximum_index_ppm, 0)
    } else {
        (
            i64::try_from(quotient).map_err(|_| RealEstateError::ArithmeticOverflow)?,
            i64::try_from(remainder).map_err(|_| RealEstateError::ArithmeticOverflow)?,
        )
    };

    Ok(RealEstateIndexState {
        index_ppm,
        remainder_numerator,
    })
}

fn offer_kind_for_slot(slot: u8) -> Result<PropertyOfferKind, RealEstateError> {
    let zero_based = slot
        .checked_sub(1)
        .ok_or(RealEstateError::InvalidListingSlot)?;
    match zero_based % 3 {
        0 => Ok(PropertyOfferKind::Sale),
        1 => Ok(PropertyOfferKind::Jeonse),
        2 => Ok(PropertyOfferKind::MonthlyRent),
        _ => Err(RealEstateError::InvalidListingSlot),
    }
}

fn calculate_offer(
    profile: RealEstateRegionProfile,
    month_start_daily: RealEstateDaily,
    exclusive_area_square_meters: u16,
    price_variation_ppm: i64,
    kind: PropertyOfferKind,
) -> Result<PropertyListingOffer, RealEstateError> {
    let base_krw = i128::from(exclusive_area_square_meters)
        .checked_mul(i128::from(profile.base_price_per_square_meter_krw))
        .ok_or(RealEstateError::ArithmeticOverflow)?;
    let scale = i128::from(REAL_ESTATE_INDEX_SCALE_PPM);
    let squared_scale = scale
        .checked_mul(scale)
        .ok_or(RealEstateError::ArithmeticOverflow)?;
    let indexed_amount = |index_ppm: i64| {
        base_krw
            .checked_mul(i128::from(price_variation_ppm))
            .and_then(|value| value.checked_mul(i128::from(index_ppm)))
            .and_then(|value| value.checked_div(squared_scale))
            .ok_or(RealEstateError::ArithmeticOverflow)
    };
    let sale_price_krw = || positive_krw(indexed_amount(month_start_daily.price.index_ppm)?);

    match kind {
        PropertyOfferKind::Sale => Ok(PropertyListingOffer::Sale {
            price_krw: sale_price_krw()?,
        }),
        PropertyOfferKind::Jeonse => {
            let deposit_krw = i128::from(sale_price_krw()?)
                .checked_mul(i128::from(profile.jeonse_ratio_ppm))
                .and_then(|value| value.checked_div(scale))
                .ok_or(RealEstateError::ArithmeticOverflow)?;
            Ok(PropertyListingOffer::Jeonse {
                deposit_krw: positive_krw(deposit_krw)?,
            })
        }
        PropertyOfferKind::MonthlyRent => {
            let sale_price_krw = sale_price_krw()?;
            let monthly_deposit_krw = i128::from(sale_price_krw)
                .checked_mul(i128::from(profile.monthly_deposit_ratio_ppm))
                .and_then(|value| value.checked_div(scale))
                .ok_or(RealEstateError::ArithmeticOverflow)?;
            let monthly_deposit_krw = positive_krw(monthly_deposit_krw)?;
            let rent_valuation_krw =
                positive_krw(indexed_amount(month_start_daily.rent.index_ppm)?)?;
            let rentable_valuation_krw = rent_valuation_krw
                .checked_sub(monthly_deposit_krw)
                .filter(|value| *value > 0)
                .ok_or(RealEstateError::InvalidOffer)?;
            let annual_denominator = i128::from(12)
                .checked_mul(scale)
                .ok_or(RealEstateError::ArithmeticOverflow)?;
            let monthly_rent_krw = i128::from(rentable_valuation_krw)
                .checked_mul(i128::from(profile.annual_gross_rent_yield_ppm))
                .and_then(|value| value.checked_div(annual_denominator))
                .ok_or(RealEstateError::ArithmeticOverflow)?;

            Ok(PropertyListingOffer::MonthlyRent {
                deposit_krw: monthly_deposit_krw,
                monthly_rent_krw: positive_krw(monthly_rent_krw)?,
            })
        }
    }
}

fn positive_krw(value: i128) -> Result<i64, RealEstateError> {
    let value = i64::try_from(value).map_err(|_| RealEstateError::ArithmeticOverflow)?;
    if !(1..=REAL_ESTATE_MAX_PUBLIC_MONEY_KRW).contains(&value) {
        return Err(RealEstateError::InvalidOffer);
    }
    Ok(value)
}

fn validate_profile(profile: RealEstateRegionProfile) -> Result<(), RealEstateError> {
    let valid_slots =
        (1..=REAL_ESTATE_MAX_LISTINGS_PER_REGION).contains(&profile.monthly_listing_slot_count);
    let valid_area = profile.minimum_exclusive_area_square_meters > 0
        && profile.minimum_exclusive_area_square_meters
            <= profile.maximum_exclusive_area_square_meters
        && profile.maximum_exclusive_area_square_meters
            <= REAL_ESTATE_MAX_EXCLUSIVE_AREA_SQUARE_METERS;
    let valid_index_bounds = profile.minimum_index_ppm > 0
        && profile.minimum_index_ppm <= REAL_ESTATE_INDEX_SCALE_PPM
        && profile.maximum_index_ppm >= REAL_ESTATE_INDEX_SCALE_PPM
        && profile.minimum_index_ppm <= profile.maximum_index_ppm
        && profile.maximum_index_ppm <= REAL_ESTATE_MAX_INDEX_PPM;
    let valid_variation = profile.minimum_price_variation_ppm > 0
        && profile.minimum_price_variation_ppm <= profile.maximum_price_variation_ppm
        && profile.maximum_price_variation_ppm <= REAL_ESTATE_MAX_VARIATION_PPM;
    let valid_price_process = valid_index_process(
        profile.price_daily_drift_ppm,
        profile.price_daily_shock_amplitude_ppm,
    );
    let valid_rent_process = valid_index_process(
        profile.rent_daily_drift_ppm,
        profile.rent_daily_shock_amplitude_ppm,
    );
    let valid_ratios = (1..REAL_ESTATE_INDEX_SCALE_PPM).contains(&profile.jeonse_ratio_ppm)
        && (1..REAL_ESTATE_INDEX_SCALE_PPM).contains(&profile.monthly_deposit_ratio_ppm)
        && (1..=REAL_ESTATE_INDEX_SCALE_PPM).contains(&profile.annual_gross_rent_yield_ppm);
    let valid_rules = profile.availability_rule
        == PropertyListingAvailabilityRule::MarketMonthInclusive
        && profile.offer_rotation_rule == PropertyOfferRotationRule::SaleJeonseMonthlyRent;

    if valid_slots
        && valid_area
        && (1..=1_000_000_000_000).contains(&profile.base_price_per_square_meter_krw)
        && valid_index_bounds
        && valid_variation
        && valid_price_process
        && valid_rent_process
        && valid_ratios
        && valid_rules
    {
        Ok(())
    } else {
        Err(RealEstateError::InvalidProfile)
    }
}

fn valid_index_process(drift_ppm: i64, shock_amplitude_ppm: i64) -> bool {
    (-999_999..=999_999).contains(&drift_ppm)
        && (0..=999_999).contains(&shock_amplitude_ppm)
        && REAL_ESTATE_INDEX_SCALE_PPM
            .checked_add(drift_ppm)
            .and_then(|value| value.checked_sub(shock_amplitude_ppm))
            .is_some_and(|minimum_factor| minimum_factor > 0)
}

fn validate_index_state(
    profile: RealEstateRegionProfile,
    state: RealEstateIndexState,
) -> Result<(), RealEstateError> {
    if !(profile.minimum_index_ppm..=profile.maximum_index_ppm).contains(&state.index_ppm) {
        return Err(RealEstateError::InvalidIndex);
    }
    if !(-REAL_ESTATE_INDEX_SCALE_PPM..REAL_ESTATE_INDEX_SCALE_PPM)
        .contains(&state.remainder_numerator)
    {
        return Err(RealEstateError::InvalidRemainder);
    }
    if (state.index_ppm == profile.minimum_index_ppm
        || state.index_ppm == profile.maximum_index_ppm)
        && state.remainder_numerator != 0
    {
        return Err(RealEstateError::InvalidRemainder);
    }
    Ok(())
}

fn validate_listing_entropy_key(key: PropertyListingEntropyKey) -> Result<(), RealEstateError> {
    if !key.year_month.is_valid() {
        return Err(RealEstateError::InvalidYearMonth);
    }
    if !(1..=REAL_ESTATE_MAX_LISTINGS_PER_REGION).contains(&key.slot) {
        return Err(RealEstateError::InvalidListingSlot);
    }
    Ok(())
}

fn validate_listing_input(
    input: PropertyListingGenerationInput<'_>,
) -> Result<(), RealEstateError> {
    validate_profile(input.profile)?;
    if !input.year_month.is_valid() {
        return Err(RealEstateError::InvalidYearMonth);
    }
    if input.available_from_game_day > input.available_to_game_day
        || input.month_start_daily.game_day != input.available_from_game_day
        || input.month_start_daily.region_key != input.profile.region_key
    {
        return Err(RealEstateError::InvalidListingWindow);
    }
    validate_index_state(input.profile, input.month_start_daily.price)?;
    validate_index_state(input.profile, input.month_start_daily.rent)?;
    if input.allowed_property_types.is_empty()
        || input.allowed_property_types.len() > PropertyType::ALL.len()
        || input
            .allowed_property_types
            .windows(2)
            .any(|pair| pair[0].order() >= pair[1].order())
    {
        return Err(RealEstateError::InvalidPropertyTypes);
    }
    Ok(())
}

fn sample_index_signed(
    world_seed: u64,
    model_version_id: ResourceId,
    region_key: LifeRegionKey,
    game_day: u32,
    series: IndexSeries,
    minimum: i64,
    maximum: i64,
) -> Result<i64, RealEstateError> {
    sample_i64_inclusive(minimum, maximum, |counter| {
        Ok(index_entropy_word(
            world_seed,
            model_version_id,
            region_key,
            game_day,
            series,
            counter,
        ))
    })
}

fn sample_listing_inclusive(
    key: PropertyListingEntropyKey,
    stream: ListingEntropyStream,
    minimum: u64,
    maximum: u64,
) -> Result<u64, RealEstateError> {
    if minimum > maximum {
        return Err(RealEstateError::ArithmeticOverflow);
    }
    let width = maximum
        .checked_sub(minimum)
        .and_then(|value| value.checked_add(1))
        .ok_or(RealEstateError::ArithmeticOverflow)?;
    let offset = sample_bounded_u64(width, |counter| {
        Ok(listing_entropy_word(key, stream, counter))
    })?;
    minimum
        .checked_add(offset)
        .ok_or(RealEstateError::ArithmeticOverflow)
}

fn sample_i64_inclusive<F>(
    minimum: i64,
    maximum: i64,
    sample_word: F,
) -> Result<i64, RealEstateError>
where
    F: FnMut(u32) -> Result<u64, RealEstateError>,
{
    if minimum > maximum {
        return Err(RealEstateError::ArithmeticOverflow);
    }
    let width = i128::from(maximum)
        .checked_sub(i128::from(minimum))
        .and_then(|value| value.checked_add(1))
        .and_then(|value| u64::try_from(value).ok())
        .ok_or(RealEstateError::ArithmeticOverflow)?;
    let offset = sample_bounded_u64(width, sample_word)?;
    i128::from(minimum)
        .checked_add(i128::from(offset))
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(RealEstateError::ArithmeticOverflow)
}

fn sample_bounded_u64<F>(exclusive_upper: u64, mut sample_word: F) -> Result<u64, RealEstateError>
where
    F: FnMut(u32) -> Result<u64, RealEstateError>,
{
    if exclusive_upper == 0 {
        return Err(RealEstateError::ArithmeticOverflow);
    }
    let rejection_threshold = exclusive_upper.wrapping_neg() % exclusive_upper;
    let mut counter = 0_u32;
    loop {
        let word = sample_word(counter)?;
        if word >= rejection_threshold {
            return Ok(word % exclusive_upper);
        }
        counter = counter
            .checked_add(1)
            .ok_or(RealEstateError::EntropyExhausted)?;
    }
}

fn index_entropy_word(
    world_seed: u64,
    model_version_id: ResourceId,
    region_key: LifeRegionKey,
    game_day: u32,
    series: IndexSeries,
    counter: u32,
) -> u64 {
    let mut digest = Sha256::new();
    digest.update(INDEX_ENTROPY_DOMAIN);
    digest.update(world_seed.to_be_bytes());
    digest.update(model_version_id.get().to_be_bytes());
    push_region_key(&mut digest, region_key);
    digest.update(game_day.to_be_bytes());
    digest.update([series.tag()]);
    digest.update(counter.to_be_bytes());
    first_u64(digest.finalize().into())
}

fn listing_entropy_word(
    key: PropertyListingEntropyKey,
    stream: ListingEntropyStream,
    counter: u32,
) -> u64 {
    let mut digest = Sha256::new();
    digest.update(LISTING_ENTROPY_DOMAIN);
    digest.update(key.world_seed.to_be_bytes());
    digest.update(key.model_version_id.get().to_be_bytes());
    digest.update(key.year_month.year.to_be_bytes());
    digest.update([key.year_month.month]);
    push_region_key(&mut digest, key.region_key);
    digest.update([key.slot]);
    digest.update([stream.tag()]);
    digest.update(counter.to_be_bytes());
    first_u64(digest.finalize().into())
}

fn push_region_key(digest: &mut Sha256, region_key: LifeRegionKey) {
    let value = region_key.as_str().as_bytes();
    digest.update([u8::try_from(value.len()).unwrap_or(0)]);
    digest.update(value);
}

fn first_u64(digest: [u8; 32]) -> u64 {
    u64::from_be_bytes([
        digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::life::YearMonth;

    fn given_profile(region_key: LifeRegionKey) -> RealEstateRegionProfile {
        RealEstateRegionProfile {
            region_key,
            monthly_listing_slot_count: 12,
            minimum_exclusive_area_square_meters: 30,
            maximum_exclusive_area_square_meters: 120,
            base_price_per_square_meter_krw: 10_000_000,
            price_daily_drift_ppm: 80,
            price_daily_shock_amplitude_ppm: 1_200,
            rent_daily_drift_ppm: 50,
            rent_daily_shock_amplitude_ppm: 500,
            minimum_index_ppm: 500_000,
            maximum_index_ppm: 2_000_000,
            minimum_price_variation_ppm: 850_000,
            maximum_price_variation_ppm: 1_150_000,
            jeonse_ratio_ppm: 550_000,
            annual_gross_rent_yield_ppm: 35_000,
            monthly_deposit_ratio_ppm: 100_000,
            availability_rule: PropertyListingAvailabilityRule::MarketMonthInclusive,
            offer_rotation_rule: PropertyOfferRotationRule::SaleJeonseMonthlyRent,
        }
    }

    fn given_day(profile: RealEstateRegionProfile, game_day: u32) -> RealEstateDaily {
        RealEstateDaily {
            region_key: profile.region_key,
            game_day,
            price: RealEstateIndexState {
                index_ppm: 1_000_003,
                remainder_numerator: 0,
            },
            rent: RealEstateIndexState {
                index_ppm: 1_000_007,
                remainder_numerator: 0,
            },
        }
    }

    fn given_listing_input<'a>(
        profile: RealEstateRegionProfile,
        allowed_property_types: &'a [PropertyType],
    ) -> PropertyListingGenerationInput<'a> {
        PropertyListingGenerationInput {
            world_seed: 20_260_101,
            model_version_id: ResourceId::from_u64(17),
            year_month: YearMonth {
                year: 2026,
                month: 7,
            },
            profile,
            allowed_property_types,
            available_from_game_day: 181,
            available_to_game_day: 211,
            month_start_daily: given_day(profile, 181),
        }
    }

    mod context_지역_지수를_계산하는_경우 {
        use super::*;

        #[test]
        fn given_day0_when_초기화하면_then_두_index는_백만이고_remainder는_0이다() {
            let profile = given_profile(LifeRegionKey::CapitalArea);

            let daily = create_real_estate_rules()
                .day_zero(RealEstateDayZeroInput { profile })
                .expect("day 0 지수를 만들어야 한다");

            assert_eq!(daily.price.index_ppm, 1_000_000);
            assert_eq!(daily.price.remainder_numerator, 0);
            assert_eq!(daily.rent.index_ppm, 1_000_000);
            assert_eq!(daily.rent.remainder_numerator, 0);
        }

        #[test]
        fn given_누적_remainder_when_다음날을_계산하면_then_i128_몫과_나머지를_보존한다() {
            let mut profile = given_profile(LifeRegionKey::CapitalArea);
            profile.price_daily_drift_ppm = 1;
            profile.price_daily_shock_amplitude_ppm = 0;
            let previous = RealEstateDaily {
                region_key: profile.region_key,
                game_day: 4,
                price: RealEstateIndexState {
                    index_ppm: 1_000_001,
                    remainder_numerator: 999_998,
                },
                rent: RealEstateIndexState {
                    index_ppm: 1_000_000,
                    remainder_numerator: 0,
                },
            };

            let daily = create_real_estate_rules()
                .next_day(RealEstateNextDayInput {
                    world_seed: 1,
                    model_version_id: ResourceId::from_u64(1),
                    profile,
                    previous,
                })
                .expect("다음날 지수를 계산해야 한다");

            assert_eq!(daily.price.index_ppm, 1_000_002);
            assert_eq!(daily.price.remainder_numerator, 999_999);
        }

        #[test]
        fn given_상한에_닿는_변화_when_다음날을_계산하면_then_상한에서_remainder를_버린다() {
            let mut profile = given_profile(LifeRegionKey::CapitalArea);
            profile.price_daily_drift_ppm = 1;
            profile.price_daily_shock_amplitude_ppm = 0;
            profile.maximum_index_ppm = 1_000_002;
            let previous = RealEstateDaily {
                region_key: profile.region_key,
                game_day: 4,
                price: RealEstateIndexState {
                    index_ppm: 1_000_001,
                    remainder_numerator: 999_999,
                },
                rent: RealEstateIndexState {
                    index_ppm: 1_000_000,
                    remainder_numerator: 0,
                },
            };

            let daily = create_real_estate_rules()
                .next_day(RealEstateNextDayInput {
                    world_seed: 1,
                    model_version_id: ResourceId::from_u64(1),
                    profile,
                    previous,
                })
                .expect("상한을 적용해야 한다");

            assert_eq!(daily.price.index_ppm, 1_000_002);
            assert_eq!(daily.price.remainder_numerator, 0);
        }

        #[test]
        fn given_하한보다_작은_변화_when_다음날을_계산하면_then_하한에서_remainder를_버린다() {
            let mut profile = given_profile(LifeRegionKey::CapitalArea);
            profile.price_daily_drift_ppm = -1;
            profile.price_daily_shock_amplitude_ppm = 0;
            profile.minimum_index_ppm = 999_999;
            let previous = RealEstateDaily {
                region_key: profile.region_key,
                game_day: 4,
                price: RealEstateIndexState {
                    index_ppm: 1_000_000,
                    remainder_numerator: -999_999,
                },
                rent: RealEstateIndexState {
                    index_ppm: 1_000_000,
                    remainder_numerator: 0,
                },
            };

            let daily = create_real_estate_rules()
                .next_day(RealEstateNextDayInput {
                    world_seed: 1,
                    model_version_id: ResourceId::from_u64(1),
                    profile,
                    previous,
                })
                .expect("하한을 적용해야 한다");

            assert_eq!(daily.price.index_ppm, 999_999);
            assert_eq!(daily.price.remainder_numerator, 0);
        }

        #[test]
        fn given_같은_counter_when_price와_rent를_읽으면_then_domain_word가_독립적이다() {
            let price = index_entropy_word(
                7,
                ResourceId::from_u64(11),
                LifeRegionKey::SmallCity,
                31,
                IndexSeries::Price,
                0,
            );

            let rent = index_entropy_word(
                7,
                ResourceId::from_u64(11),
                LifeRegionKey::SmallCity,
                31,
                IndexSeries::Rent,
                0,
            );

            assert_ne!(price, rent);
        }
    }

    mod context_bounded_entropy를_변환하는_경우 {
        use super::*;

        #[test]
        fn given_threshold보다_작은_word_when_범위로_바꾸면_then_다음_counter를_사용한다() {
            let words = [5_u64, 16_u64];
            let mut sampled_counters = Vec::new();

            let value = sample_bounded_u64(10, |counter| {
                sampled_counters.push(counter);
                Ok(words[usize::try_from(counter).expect("작은 counter여야 한다")])
            })
            .expect("편향 없이 범위로 바꿔야 한다");

            assert_eq!(value, 6);
            assert_eq!(sampled_counters, vec![0, 1]);
        }
    }

    mod context_월별_매물을_생성하는_경우 {
        use super::*;

        #[test]
        fn given_같은_key_when_두번_생성하면_then_byte_identical_catalog다() {
            let profile = given_profile(LifeRegionKey::CapitalArea);
            let allowed = [PropertyType::Apartment, PropertyType::MultiFamily];
            let input = given_listing_input(profile, &allowed);

            let first = create_real_estate_rules()
                .generate_monthly_listings(input)
                .expect("첫 매물 카탈로그를 만들어야 한다");
            let replay = create_real_estate_rules()
                .generate_monthly_listings(input)
                .expect("같은 매물 카탈로그를 다시 만들어야 한다");

            assert_eq!(
                serde_json::to_vec(&first).expect("첫 카탈로그를 직렬화해야 한다"),
                serde_json::to_vec(&replay).expect("재생 카탈로그를 직렬화해야 한다")
            );
        }

        #[test]
        fn given_지역_월_slot이_다른_key_when_id를_만들면_then_각각_다른_63bit_id다() {
            let rules = create_real_estate_rules();
            let base = PropertyListingEntropyKey {
                world_seed: 9,
                model_version_id: ResourceId::from_u64(3),
                year_month: YearMonth {
                    year: 2026,
                    month: 7,
                },
                region_key: LifeRegionKey::CapitalArea,
                slot: 1,
            };

            let base_id = rules
                .stable_listing_id(base)
                .expect("기준 ID를 만들어야 한다");
            let region_id = rules
                .stable_listing_id(PropertyListingEntropyKey {
                    region_key: LifeRegionKey::Metropolitan,
                    ..base
                })
                .expect("지역별 ID를 만들어야 한다");
            let month_id = rules
                .stable_listing_id(PropertyListingEntropyKey {
                    year_month: YearMonth {
                        year: 2026,
                        month: 8,
                    },
                    ..base
                })
                .expect("월별 ID를 만들어야 한다");
            let slot_id = rules
                .stable_listing_id(PropertyListingEntropyKey { slot: 2, ..base })
                .expect("slot별 ID를 만들어야 한다");

            assert!(base_id.get() <= REAL_ESTATE_MAX_PUBLIC_LISTING_ID);
            assert_ne!(base_id, region_id);
            assert_ne!(base_id, month_id);
            assert_ne!(base_id, slot_id);
        }

        #[test]
        fn given_고정된_canonical_key_when_id를_만들면_then_golden_63bit_id와_같다() {
            let key = PropertyListingEntropyKey {
                world_seed: 9,
                model_version_id: ResourceId::from_u64(3),
                year_month: YearMonth {
                    year: 2026,
                    month: 7,
                },
                region_key: LifeRegionKey::CapitalArea,
                slot: 1,
            };

            let id = create_real_estate_rules()
                .stable_listing_id(key)
                .expect("고정 ID를 만들어야 한다");

            assert_eq!(id.get(), 5_545_348_532_385_317_248);
        }

        #[test]
        fn given_12개_slot_when_생성하면_then_offer는_sale_jeonse_monthly_rent가_네번씩_회전한다() {
            let profile = given_profile(LifeRegionKey::CapitalArea);
            let allowed = [PropertyType::Apartment, PropertyType::MultiFamily];

            let listings = create_real_estate_rules()
                .generate_monthly_listings(given_listing_input(profile, &allowed))
                .expect("매물 카탈로그를 만들어야 한다");

            let kinds = listings
                .iter()
                .map(|listing| listing.offers[0].kind())
                .collect::<Vec<_>>();
            assert_eq!(
                kinds,
                vec![
                    PropertyOfferKind::Sale,
                    PropertyOfferKind::Jeonse,
                    PropertyOfferKind::MonthlyRent,
                    PropertyOfferKind::Sale,
                    PropertyOfferKind::Jeonse,
                    PropertyOfferKind::MonthlyRent,
                    PropertyOfferKind::Sale,
                    PropertyOfferKind::Jeonse,
                    PropertyOfferKind::MonthlyRent,
                    PropertyOfferKind::Sale,
                    PropertyOfferKind::Jeonse,
                    PropertyOfferKind::MonthlyRent,
                ]
            );
        }

        #[test]
        fn given_허용_type과_면적_variation범위_when_생성하면_then_모든_매물이_범위안에_있다() {
            let profile = given_profile(LifeRegionKey::CapitalArea);
            let allowed = [PropertyType::Apartment, PropertyType::MultiFamily];

            let listings = create_real_estate_rules()
                .generate_monthly_listings(given_listing_input(profile, &allowed))
                .expect("매물 카탈로그를 만들어야 한다");

            assert!(listings.iter().all(|listing| {
                allowed.contains(&listing.property_type)
                    && (profile.minimum_exclusive_area_square_meters
                        ..=profile.maximum_exclusive_area_square_meters)
                        .contains(&listing.exclusive_area_square_meters)
                    && (profile.minimum_price_variation_ppm..=profile.maximum_price_variation_ppm)
                        .contains(&listing.price_variation_ppm)
            }));
        }

        #[test]
        fn given_고정된_면적과_index_when_offer를_계산하면_then_각_식을_한번_나누어_내린다() {
            let profile = given_profile(LifeRegionKey::CapitalArea);
            let daily = given_day(profile, 0);

            let sale = calculate_offer(profile, daily, 84, 1_000_001, PropertyOfferKind::Sale)
                .expect("매매가를 계산해야 한다");
            let jeonse = calculate_offer(profile, daily, 84, 1_000_001, PropertyOfferKind::Jeonse)
                .expect("전세보증금을 계산해야 한다");
            let monthly = calculate_offer(
                profile,
                daily,
                84,
                1_000_001,
                PropertyOfferKind::MonthlyRent,
            )
            .expect("월세 조건을 계산해야 한다");

            assert_eq!(
                sale,
                PropertyListingOffer::Sale {
                    price_krw: 840_003_360
                }
            );
            assert_eq!(
                jeonse,
                PropertyListingOffer::Jeonse {
                    deposit_krw: 462_001_848
                }
            );
            assert_eq!(
                monthly,
                PropertyListingOffer::MonthlyRent {
                    deposit_krw: 84_000_336,
                    monthly_rent_krw: 2_205_018,
                }
            );
        }

        #[test]
        fn given_rent가_deposit이하인_profile_when_sale_slot만_계산하면_then_sale은_게시된다() {
            let mut profile = given_profile(LifeRegionKey::CapitalArea);
            profile.monthly_listing_slot_count = 1;
            profile.minimum_index_ppm = 1;
            let allowed = [PropertyType::Apartment];
            let mut input = given_listing_input(profile, &allowed);
            input.month_start_daily.rent.index_ppm = 50_000;

            let listings = create_real_estate_rules()
                .generate_monthly_listings(input)
                .expect("불필요한 월세 식은 계산하지 않아야 한다");

            assert!(matches!(
                listings[0].offers[0],
                PropertyListingOffer::Sale { .. }
            ));
        }

        #[test]
        fn given_rent가_deposit이하인_profile_when_monthly_rent_slot을_계산하면_then_invariant오류다()
         {
            let mut profile = given_profile(LifeRegionKey::CapitalArea);
            profile.monthly_listing_slot_count = 3;
            profile.minimum_index_ppm = 1;
            let allowed = [PropertyType::Apartment];
            let mut input = given_listing_input(profile, &allowed);
            input.month_start_daily.rent.index_ppm = 50_000;

            let result = create_real_estate_rules().generate_monthly_listings(input);

            assert_eq!(result, Err(RealEstateError::InvalidOffer));
        }
    }
}
