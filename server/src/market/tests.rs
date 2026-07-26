use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use time::{Date, Month};

use super::generator::{
    apply_equity_rate_shock, equity_rate_shock_ppm, gold_rate_return_adjustment_ppm,
};
use super::{
    IndexProductTerms, InterestRateState, M2MarketState, MarketDay, MarketEntropy, MarketError,
    MarketGenerator, MarketParameters, MarketRegime, MarketWorld, NullableInterestRateState,
    NullableM2MarketState, create_default_market_generator, create_market_generator,
    create_market_generator_registry, create_market_generator_registry_with_entropy,
    create_sha256_market_entropy, default_market_calibration, default_market_world,
    krx_market_calibration, krx_market_world, m2_market_calibration, m2_market_world,
    rate_market_calibration, rate_market_world,
};

const THIRTY_YEARS_IN_DAYS: u32 = 10_958;
const STATISTICAL_HORIZONS_IN_DAYS: [u32; 3] = [9_132, 10_958, 12_784];
const STATISTICAL_SEEDS: [u64; 32] = [
    7,
    11,
    19,
    23,
    42,
    101,
    503,
    2_026,
    31_337,
    1_048_573,
    20_260_101,
    9_223_372_036_854_775_123,
    1,
    2,
    3,
    5,
    13,
    29,
    47,
    97,
    257,
    1_021,
    4_093,
    65_537,
    99_991,
    1_000_003,
    16_777_213,
    4_294_967_291,
    8_589_934_583,
    18_446_744_073_709_551_557,
    18_446_744_073_709_551_601,
    18_446_744_073_709_551_615,
];

struct FixedEntropy(u64);

impl MarketEntropy for FixedEntropy {
    fn sample_u64(&self, _world_seed: u64, _game_day: u32, _stream: u32) -> u64 {
        self.0
    }
}

struct StreamEntropy {
    rate_word: u64,
}

impl MarketEntropy for StreamEntropy {
    fn sample_u64(&self, _world_seed: u64, _game_day: u32, stream: u32) -> u64 {
        if stream == 2 { self.rate_word } else { 0 }
    }
}

struct M2StreamEntropy {
    gold_word: u64,
    gold_samples: AtomicU32,
}

impl MarketEntropy for M2StreamEntropy {
    fn sample_u64(&self, _world_seed: u64, _game_day: u32, stream: u32) -> u64 {
        if stream == 3 {
            self.gold_samples.fetch_add(1, Ordering::Relaxed);
            self.gold_word
        } else {
            0
        }
    }
}

fn given_default_generator() -> Arc<dyn MarketGenerator> {
    create_default_market_generator().expect("default calibration must be valid")
}

fn given_generator_with_entropy(entropy: u64) -> Arc<dyn MarketGenerator> {
    create_market_generator(
        default_market_calibration(),
        Arc::new(FixedEntropy(entropy)),
    )
    .expect("default calibration must be valid")
}

fn given_default_world_with_seed(seed: u64) -> MarketWorld {
    let mut world = default_market_world().expect("default world must be valid");
    world.seed = seed;
    world
}

fn given_rate_generator() -> Arc<dyn MarketGenerator> {
    let registry = create_market_generator_registry().expect("calendar registry must be valid");
    registry
        .generator_for(&rate_market_calibration())
        .expect("v3 generator must be registered")
}

fn given_rate_generator_with_rate_word(rate_word: u64) -> Arc<dyn MarketGenerator> {
    let registry =
        create_market_generator_registry_with_entropy(Arc::new(StreamEntropy { rate_word }))
            .expect("calendar registry must be valid");
    registry
        .generator_for(&rate_market_calibration())
        .expect("v3 generator must be registered")
}

fn given_rate_world_with_seed(seed: u64) -> MarketWorld {
    let mut world = rate_market_world().expect("rate world must be valid");
    world.seed = seed;
    world
}

fn given_m2_generator() -> Arc<dyn MarketGenerator> {
    let registry = create_market_generator_registry().expect("calendar registry must be valid");
    registry
        .generator_for(&m2_market_calibration())
        .expect("v4 generator must be registered")
}

fn given_m2_generator_with_gold_word(gold_word: u64) -> Arc<dyn MarketGenerator> {
    let registry = create_market_generator_registry_with_entropy(Arc::new(M2StreamEntropy {
        gold_word,
        gold_samples: AtomicU32::new(0),
    }))
    .expect("calendar registry must be valid");
    registry
        .generator_for(&m2_market_calibration())
        .expect("v4 generator must be registered")
}

fn given_m2_world() -> MarketWorld {
    m2_market_world(IndexProductTerms {
        product_version_id: 1,
        product_key: "llx-domestic-equity-2026-v1".to_owned(),
        day0_close_krw: 100_000,
        annual_management_fee_ppm: 1_500,
        annual_distribution_rate_ppm: 20_000,
        day_count_denominator: 365,
        buy_fee_ppm: 0,
        sell_fee_ppm: 0,
        transaction_tax_ppm: 0,
    })
    .expect("v4 world must be valid")
}

fn given_2026_date(month: Month, day: u8) -> Date {
    Date::from_calendar_date(2026, month, day).expect("test date must be valid")
}

fn when_generating_through(
    generator: &dyn MarketGenerator,
    world: &MarketWorld,
    target_game_day: u32,
) -> Vec<MarketDay> {
    let day_zero = generator.day_zero(world).expect("day zero must generate");
    let mut days = vec![day_zero.clone()];
    days.extend(
        generator
            .generate_through(world, &day_zero, target_game_day)
            .expect("market path must generate"),
    );
    days
}

mod deterministic_market_path {
    use super::*;

    mod context_same_seed_and_target {
        use super::*;

        #[test]
        fn given_equal_worlds_when_generated_separately_then_every_day_matches() {
            let generator = given_default_generator();
            let first_world = given_default_world_with_seed(404);
            let second_world = given_default_world_with_seed(404);

            let first = when_generating_through(generator.as_ref(), &first_world, 400);
            let second = when_generating_through(generator.as_ref(), &second_world, 400);

            assert_eq!(first, second);
        }
    }

    mod context_different_batch_sizes {
        use super::*;

        #[test]
        fn given_one_world_when_generated_in_chunks_then_path_matches_one_batch() {
            let generator = given_default_generator();
            let world = given_default_world_with_seed(505);
            let day_zero = generator.day_zero(&world).expect("day zero must generate");
            let one_batch = generator
                .generate_through(&world, &day_zero, 400)
                .expect("one batch must generate");

            let mut chunks = generator
                .generate_through(&world, &day_zero, 137)
                .expect("first chunk must generate");
            let cursor = chunks.last().expect("first chunk is non-empty").clone();
            chunks.extend(
                generator
                    .generate_through(&world, &cursor, 400)
                    .expect("second chunk must generate"),
            );

            assert_eq!(one_batch, chunks);
        }
    }

    mod context_different_seeds {
        use super::*;

        #[test]
        fn given_distinct_world_seeds_when_generated_then_paths_diverge() {
            let generator = given_default_generator();
            let first =
                when_generating_through(generator.as_ref(), &given_default_world_with_seed(1), 100);
            let second =
                when_generating_through(generator.as_ref(), &given_default_world_with_seed(2), 100);

            assert_ne!(first, second);
        }
    }
}

mod counter_entropy_contract {
    use super::*;

    mod context_versioned_counter_input {
        use super::*;

        #[test]
        fn given_known_seed_day_and_stream_when_sampled_then_word_matches_fixed_vector() {
            let entropy = create_sha256_market_entropy();

            let word = entropy.sample_u64(20_260_101, 42, 1);

            assert_eq!(word, 7_200_341_983_309_967_106);
        }
    }
}

mod calendar_market_rules {
    use super::*;

    mod context_day_zero {
        use super::*;

        #[test]
        fn given_default_world_when_anchored_then_price_and_session_are_initial_values() {
            let generator = given_default_generator();
            let world = default_market_world().expect("default world must be valid");

            let day = generator.day_zero(&world).expect("day zero must generate");

            assert_eq!(day.equity_close_krw, 100_000);
            assert_eq!(day.session_index, 0);
            assert_eq!(day.equity_return_ppm, 0);
            assert!(day.market_open);
        }
    }

    mod context_weekend {
        use super::*;

        #[test]
        fn given_friday_close_when_weekend_generated_then_price_and_garch_state_carry() {
            let generator = given_default_generator();
            let world = default_market_world().expect("default world must be valid");
            let days = when_generating_through(generator.as_ref(), &world, 3);
            let friday = &days[1];
            let saturday = &days[2];
            let sunday = &days[3];

            assert_eq!(saturday.equity_close_krw, friday.equity_close_krw);
            assert_eq!(sunday.equity_close_krw, friday.equity_close_krw);
            assert_eq!(saturday.equity_residual_ppm, friday.equity_residual_ppm);
            assert_eq!(sunday.equity_residual_ppm, friday.equity_residual_ppm);
            assert_eq!(saturday.session_index, friday.session_index);
            assert_eq!(sunday.session_index, friday.session_index);
            assert_eq!(saturday.equity_return_ppm, 0);
            assert_eq!(sunday.equity_return_ppm, 0);
            assert!(!saturday.market_open);
            assert!(!sunday.market_open);
        }
    }
}

mod versioned_calendar_registry {
    use super::*;

    mod context_legacy_weekend_calibration {
        use super::*;

        #[test]
        fn given_v1_calibration_when_generated_through_registry_then_path_matches_fixed_vector() {
            let registry =
                create_market_generator_registry().expect("calendar registry must be valid");
            let calibration = default_market_calibration();
            let generator = registry
                .generator_for(&calibration)
                .expect("v1 generator must be registered");
            let world = default_market_world().expect("default world must be valid");

            let actual = when_generating_through(generator.as_ref(), &world, 10);

            let expected = vec![
                MarketDay {
                    game_day: 0,
                    market_date: given_2026_date(Month::January, 1),
                    market_open: true,
                    session_index: 0,
                    regime: MarketRegime::Expansion,
                    equity_close_krw: 100_000,
                    equity_return_ppm: 0,
                    equity_variance_ppm2: 144_000_000,
                    equity_residual_ppm: 0,
                    rates: None,
                    m2: None,
                },
                MarketDay {
                    game_day: 1,
                    market_date: given_2026_date(Month::January, 2),
                    market_open: true,
                    session_index: 1,
                    regime: MarketRegime::Expansion,
                    equity_close_krw: 100_905,
                    equity_return_ppm: 9_051,
                    equity_variance_ppm2: 132_480_000,
                    equity_residual_ppm: 8_631,
                    rates: None,
                    m2: None,
                },
                MarketDay {
                    game_day: 2,
                    market_date: given_2026_date(Month::January, 3),
                    market_open: false,
                    session_index: 1,
                    regime: MarketRegime::Expansion,
                    equity_close_krw: 100_905,
                    equity_return_ppm: 0,
                    equity_variance_ppm2: 132_480_000,
                    equity_residual_ppm: 8_631,
                    rates: None,
                    m2: None,
                },
                MarketDay {
                    game_day: 3,
                    market_date: given_2026_date(Month::January, 4),
                    market_open: false,
                    session_index: 1,
                    regime: MarketRegime::Expansion,
                    equity_close_krw: 100_905,
                    equity_return_ppm: 0,
                    equity_variance_ppm2: 132_480_000,
                    equity_residual_ppm: 8_631,
                    rates: None,
                    m2: None,
                },
                MarketDay {
                    game_day: 4,
                    market_date: given_2026_date(Month::January, 5),
                    market_open: true,
                    session_index: 2,
                    regime: MarketRegime::Expansion,
                    equity_close_krw: 101_803,
                    equity_return_ppm: 8_901,
                    equity_variance_ppm2: 127_898_732,
                    equity_residual_ppm: 8_481,
                    rates: None,
                    m2: None,
                },
                MarketDay {
                    game_day: 5,
                    market_date: given_2026_date(Month::January, 6),
                    market_open: true,
                    session_index: 3,
                    regime: MarketRegime::Expansion,
                    equity_close_krw: 102_977,
                    equity_return_ppm: 11_533,
                    equity_variance_ppm2: 123_501_528,
                    equity_residual_ppm: 11_113,
                    rates: None,
                    m2: None,
                },
                MarketDay {
                    game_day: 6,
                    market_date: given_2026_date(Month::January, 7),
                    market_open: true,
                    session_index: 4,
                    regime: MarketRegime::Expansion,
                    equity_close_krw: 102_162,
                    equity_return_ppm: -7_917,
                    equity_variance_ppm2: 123_603_799,
                    equity_residual_ppm: -8_337,
                    rates: None,
                    m2: None,
                },
                MarketDay {
                    game_day: 7,
                    market_date: given_2026_date(Month::January, 8),
                    market_open: true,
                    session_index: 5,
                    regime: MarketRegime::Expansion,
                    equity_close_krw: 102_484,
                    equity_return_ppm: 3_151,
                    equity_variance_ppm2: 119_377_921,
                    equity_residual_ppm: 2_731,
                    rates: None,
                    m2: None,
                },
                MarketDay {
                    game_day: 8,
                    market_date: given_2026_date(Month::January, 9),
                    market_open: true,
                    session_index: 6,
                    regime: MarketRegime::Expansion,
                    equity_close_krw: 102_527,
                    equity_return_ppm: 420,
                    equity_variance_ppm2: 110_547_466,
                    equity_residual_ppm: 0,
                    rates: None,
                    m2: None,
                },
                MarketDay {
                    game_day: 9,
                    market_date: given_2026_date(Month::January, 10),
                    market_open: false,
                    session_index: 6,
                    regime: MarketRegime::Expansion,
                    equity_close_krw: 102_527,
                    equity_return_ppm: 0,
                    equity_variance_ppm2: 110_547_466,
                    equity_residual_ppm: 0,
                    rates: None,
                    m2: None,
                },
                MarketDay {
                    game_day: 10,
                    market_date: given_2026_date(Month::January, 11),
                    market_open: false,
                    session_index: 6,
                    regime: MarketRegime::Expansion,
                    equity_close_krw: 102_527,
                    equity_return_ppm: 0,
                    equity_variance_ppm2: 110_547_466,
                    equity_residual_ppm: 0,
                    rates: None,
                    m2: None,
                },
            ];
            assert_eq!(actual, expected);
        }
    }

    mod context_published_public_holiday {
        use super::*;

        #[test]
        fn given_v2_calibration_when_substitute_holiday_generated_then_market_is_closed() {
            let registry =
                create_market_generator_registry().expect("calendar registry must be valid");
            let calibration = krx_market_calibration();
            let generator = registry
                .generator_for(&calibration)
                .expect("v2 generator must be registered");
            let world = krx_market_world().expect("KRX world must be valid");

            let days = when_generating_through(generator.as_ref(), &world, 60);
            let substitute_holiday = days
                .iter()
                .find(|day| day.market_date == given_2026_date(Month::March, 2))
                .expect("published substitute holiday must be generated");

            assert!(!substitute_holiday.market_open);
        }
    }

    mod context_krx_calibration_fixed_path {
        use super::*;

        #[test]
        fn given_v2_calibration_when_fixed_days_generated_then_legacy_vector_is_unchanged() {
            let registry =
                create_market_generator_registry().expect("calendar registry must be valid");
            let generator = registry
                .generator_for(&krx_market_calibration())
                .expect("v2 generator must be registered");
            let world = krx_market_world().expect("KRX world must be valid");
            let days = when_generating_through(generator.as_ref(), &world, 60);

            let actual: Vec<_> = [0_usize, 1, 43, 46, 49, 60]
                .into_iter()
                .map(|index| {
                    let day = &days[index];
                    (
                        day.game_day,
                        day.market_open,
                        day.session_index,
                        day.regime,
                        day.equity_close_krw,
                        day.equity_return_ppm,
                        day.rates.clone(),
                    )
                })
                .collect();

            assert_eq!(
                actual,
                vec![
                    (0, false, 0, MarketRegime::Expansion, 100_000, 0, None),
                    (1, true, 1, MarketRegime::Expansion, 100_905, 9_051, None),
                    (
                        43,
                        true,
                        31,
                        MarketRegime::Expansion,
                        101_889,
                        -15_998,
                        None,
                    ),
                    (46, false, 31, MarketRegime::Expansion, 101_889, 0, None),
                    (49, true, 32, MarketRegime::Expansion, 102_965, 10_560, None,),
                    (60, false, 38, MarketRegime::Expansion, 101_976, 0, None),
                ]
            );
        }
    }

    mod context_consecutive_lunar_new_year_closures {
        use super::*;

        #[test]
        fn given_prior_session_when_v2_holidays_pass_then_session_index_advances_once() {
            let registry =
                create_market_generator_registry().expect("calendar registry must be valid");
            let calibration = krx_market_calibration();
            let generator = registry
                .generator_for(&calibration)
                .expect("v2 generator must be registered");
            let world = krx_market_world().expect("KRX world must be valid");

            let days = when_generating_through(generator.as_ref(), &world, 49);
            let prior_session = &days[43];
            let holidays = &days[46..=48];
            let next_session = &days[49];

            assert!(holidays.iter().all(|day| !day.market_open));
            assert!(
                holidays
                    .iter()
                    .all(|day| day.session_index == prior_session.session_index)
            );
            assert_eq!(next_session.session_index, prior_session.session_index + 1);
        }
    }

    mod context_unregistered_calibration_version {
        use super::*;

        #[test]
        fn given_unknown_version_when_generator_requested_then_registry_rejects_it() {
            let registry =
                create_market_generator_registry().expect("calendar registry must be valid");
            let mut calibration = default_market_calibration();
            calibration.version = "m1-2026-calibration-unknown".to_owned();

            let result = registry.generator_for(&calibration);

            assert!(matches!(
                result,
                Err(MarketError::InvalidCalibration(
                    "generator version is not registered"
                ))
            ));
        }
    }
}

mod monthly_regime_transition {
    use super::*;

    mod context_before_and_at_boundary {
        use super::*;

        #[test]
        fn given_forced_transition_draw_when_twenty_first_session_opens_then_regime_changes_once() {
            let generator = given_generator_with_entropy(u64::MAX);
            let world = default_market_world().expect("default world must be valid");
            let days = when_generating_through(generator.as_ref(), &world, 40);
            let session_twenty = days
                .iter()
                .find(|day| day.session_index == 20 && day.market_open)
                .expect("session twenty must exist");
            let session_twenty_one = days
                .iter()
                .find(|day| day.session_index == 21 && day.market_open)
                .expect("session twenty-one must exist");
            let session_twenty_two = days
                .iter()
                .find(|day| day.session_index == 22 && day.market_open)
                .expect("session twenty-two must exist");

            assert_eq!(session_twenty.regime, MarketRegime::Expansion);
            assert_eq!(session_twenty_one.regime, MarketRegime::Recovery);
            assert_eq!(session_twenty_two.regime, MarketRegime::Recovery);
        }
    }
}

mod fixed_point_safety {
    use super::*;

    mod context_price_exceeds_i64 {
        use super::*;

        #[test]
        fn given_maximum_anchor_when_positive_return_applied_then_overflow_is_reported() {
            let generator = given_generator_with_entropy(u64::MAX);
            let world = MarketWorld {
                key: "overflow-world".to_owned(),
                seed: 1,
                start_date: Date::from_calendar_date(2026, Month::January, 1)
                    .expect("test date must be valid"),
                day0_equity_close_krw: i64::MAX,
                index_product: None,
            };
            let day_zero = generator.day_zero(&world).expect("day zero must generate");

            let result = generator.next_day(&world, &day_zero);

            assert!(matches!(result, Err(MarketError::ArithmeticOverflow(_))));
        }
    }

    mod context_garch_intermediate_exceeds_i128 {
        use super::*;

        #[test]
        fn given_corrupt_residual_when_variance_updated_then_overflow_is_reported() {
            let generator = given_default_generator();
            let world = default_market_world().expect("default world must be valid");
            let mut day_zero = generator.day_zero(&world).expect("day zero must generate");
            day_zero.equity_residual_ppm = i64::MAX;

            let result = generator.next_day(&world, &day_zero);

            assert!(matches!(result, Err(MarketError::ArithmeticOverflow(_))));
        }
    }

    mod context_long_default_paths {
        use super::*;

        #[test]
        fn given_multiple_thirty_year_paths_when_generated_then_every_price_stays_positive() {
            let generator = given_default_generator();

            let all_positive = STATISTICAL_SEEDS.iter().all(|seed| {
                when_generating_through(
                    generator.as_ref(),
                    &given_default_world_with_seed(*seed),
                    THIRTY_YEARS_IN_DAYS,
                )
                .iter()
                .all(|day| day.equity_close_krw > 0)
            });

            assert!(all_positive);
        }
    }
}

mod calibration_serialization {
    use super::*;

    mod context_default_calibration {
        use super::*;

        #[test]
        fn given_default_calibration_when_json_round_trip_then_parameters_match() {
            let calibration = default_market_calibration();

            let json = serde_json::to_string(&calibration).expect("calibration must serialize");
            let decoded = serde_json::from_str(&json).expect("calibration must deserialize");

            assert_eq!(calibration, decoded);
        }

        #[test]
        fn given_v1_parameters_when_serialized_then_optional_rate_parameters_are_omitted() {
            let parameters = default_market_calibration().parameters;

            let json = serde_json::to_string(&parameters).expect("parameters must serialize");
            let decoded: MarketParameters =
                serde_json::from_str(&json).expect("legacy parameters must deserialize");

            assert!(!json.contains("interestRates"));
            assert!(decoded.interest_rates.is_none());
        }
    }

    mod context_rate_calibration {
        use super::*;

        #[test]
        fn given_v3_calibration_when_json_round_trip_then_rate_parameters_match() {
            let calibration = rate_market_calibration();

            let json = serde_json::to_string(&calibration).expect("calibration must serialize");
            let decoded = serde_json::from_str(&json).expect("calibration must deserialize");

            assert_eq!(calibration, decoded);
        }

        #[test]
        fn given_v4_calibration_when_serialized_then_product_terms_are_not_duplicated() {
            let calibration = m2_market_calibration();

            let json = serde_json::to_string(&calibration).expect("calibration must serialize");
            let decoded = serde_json::from_str(&json).expect("calibration must deserialize");

            assert_eq!(calibration, decoded);
            assert!(!json.contains("annualManagementFeePpm"));
            assert!(!json.contains("day0CloseKrw\""));
        }
    }
}

mod interest_rate_common_factor {
    use super::*;

    mod context_day_zero_anchor {
        use super::*;

        #[test]
        fn given_v3_world_when_day_zero_generated_then_bok_anchor_and_curve_are_fixed() {
            let generator = given_rate_generator();
            let world = rate_market_world().expect("rate world must be valid");

            let day = generator.day_zero(&world).expect("day zero must generate");

            assert_eq!(
                day.rates,
                Some(InterestRateState {
                    policy_rate_bp: 250,
                    treasury_3m_bp: 255,
                    treasury_1y_bp: 265,
                    treasury_3y_bp: 280,
                    treasury_10y_bp: 310,
                    policy_rate_change_bp: 0,
                    equity_rate_shock_ppm: 0,
                })
            );
        }
    }

    mod context_twenty_first_open_session {
        use super::*;

        #[test]
        fn given_positive_rate_innovation_when_boundary_opens_then_rate_changes_once_and_curve_updates()
         {
            let generator = given_rate_generator_with_rate_word(u64::MAX);
            let world = rate_market_world().expect("rate world must be valid");

            let days = when_generating_through(generator.as_ref(), &world, 40);
            let before = days
                .iter()
                .find(|day| day.market_open && day.session_index == 20)
                .expect("twentieth session must exist");
            let boundary = days
                .iter()
                .find(|day| day.market_open && day.session_index == 21)
                .expect("twenty-first session must exist");
            let after = days
                .iter()
                .find(|day| day.market_open && day.session_index == 22)
                .expect("twenty-second session must exist");

            assert_eq!(
                before.rates.as_ref().map(|rates| rates.policy_rate_bp),
                Some(250)
            );
            assert_eq!(
                boundary.rates,
                Some(InterestRateState {
                    policy_rate_bp: 475,
                    treasury_3m_bp: 458,
                    treasury_1y_bp: 434,
                    treasury_3y_bp: 393,
                    treasury_10y_bp: 366,
                    policy_rate_change_bp: 225,
                    equity_rate_shock_ppm: -67_500,
                })
            );
            assert_eq!(
                after.rates.as_ref().map(|rates| rates.policy_rate_bp),
                Some(475)
            );
            assert_eq!(
                after
                    .rates
                    .as_ref()
                    .map(|rates| (rates.policy_rate_change_bp, rates.equity_rate_shock_ppm)),
                Some((0, 0))
            );
        }

        #[test]
        fn given_v3_path_when_non_boundary_days_generated_then_policy_change_is_zero() {
            let generator = given_rate_generator();
            let world = rate_market_world().expect("rate world must be valid");

            let days = when_generating_through(generator.as_ref(), &world, 500);

            assert!(days.iter().all(|day| {
                let rates = day.rates.as_ref().expect("v3 day must contain rates");
                day.market_open && day.session_index > 0 && day.session_index.is_multiple_of(21)
                    || rates.policy_rate_change_bp == 0 && rates.equity_rate_shock_ppm == 0
            }));
        }
    }

    mod context_generation_order {
        use super::*;

        #[test]
        fn given_same_v3_world_when_generated_in_chunks_then_rates_and_equity_match_one_batch() {
            let generator = given_rate_generator();
            let world = given_rate_world_with_seed(8_091);
            let day_zero = generator.day_zero(&world).expect("day zero must generate");
            let one_batch = generator
                .generate_through(&world, &day_zero, 800)
                .expect("one batch must generate");

            let mut chunks = generator
                .generate_through(&world, &day_zero, 263)
                .expect("first chunk must generate");
            let cursor = chunks.last().expect("first chunk is non-empty").clone();
            chunks.extend(
                generator
                    .generate_through(&world, &cursor, 800)
                    .expect("second chunk must generate"),
            );

            assert_eq!(one_batch, chunks);
        }
    }

    mod context_fixed_v3_path {
        use super::*;

        #[test]
        fn given_registered_v3_world_when_monthly_boundaries_generated_then_vector_is_fixed() {
            let generator = given_rate_generator();
            let world = rate_market_world().expect("rate world must be valid");
            let days = when_generating_through(generator.as_ref(), &world, 100);

            let actual: Vec<_> = days
                .iter()
                .filter(|day| {
                    day.market_open && day.session_index > 0 && day.session_index.is_multiple_of(21)
                })
                .map(|day| {
                    let rates = day.rates.as_ref().expect("v3 day must contain rates");
                    (
                        day.game_day,
                        day.regime,
                        day.equity_close_krw,
                        day.equity_return_ppm,
                        rates.policy_rate_bp,
                        rates.treasury_3m_bp,
                        rates.treasury_1y_bp,
                        rates.treasury_3y_bp,
                        rates.treasury_10y_bp,
                        rates.policy_rate_change_bp,
                        rates.equity_rate_shock_ppm,
                    )
                })
                .collect();

            assert_eq!(
                actual,
                vec![
                    (
                        29,
                        MarketRegime::Expansion,
                        103_702,
                        -109,
                        225,
                        233,
                        246,
                        268,
                        304,
                        -25,
                        7_500,
                    ),
                    (
                        64,
                        MarketRegime::Expansion,
                        97_668,
                        -16_635,
                        250,
                        255,
                        265,
                        280,
                        310,
                        25,
                        -7_500,
                    ),
                    (
                        95,
                        MarketRegime::Expansion,
                        98_033,
                        -4_867,
                        250,
                        255,
                        265,
                        280,
                        310,
                        0,
                        0,
                    ),
                ]
            );
        }
    }

    mod context_fixed_multi_seed_long_paths {
        use super::*;

        #[test]
        fn given_v3_paths_when_generated_then_rates_stay_bounded_and_expansion_mean_exceeds_recession()
         {
            let generator = given_rate_generator();
            let mut expansion_total = 0_i128;
            let mut expansion_count = 0_i128;
            let mut recession_total = 0_i128;
            let mut recession_count = 0_i128;

            for seed in STATISTICAL_SEEDS.into_iter().take(12) {
                let days = when_generating_through(
                    generator.as_ref(),
                    &given_rate_world_with_seed(seed),
                    12_700,
                );
                for day in days.into_iter().filter(|day| day.market_open) {
                    let rates = day.rates.expect("v3 day must contain rates");
                    assert!((0..=800).contains(&rates.policy_rate_bp));
                    assert_eq!(rates.policy_rate_bp % 25, 0);
                    assert!((0..=1_500).contains(&rates.treasury_3m_bp));
                    assert!((0..=1_500).contains(&rates.treasury_1y_bp));
                    assert!((0..=1_500).contains(&rates.treasury_3y_bp));
                    assert!((0..=1_500).contains(&rates.treasury_10y_bp));
                    match day.regime {
                        MarketRegime::Expansion => {
                            expansion_total += i128::from(rates.policy_rate_bp);
                            expansion_count += 1;
                        }
                        MarketRegime::Recession => {
                            recession_total += i128::from(rates.policy_rate_bp);
                            recession_count += 1;
                        }
                        MarketRegime::Slowdown | MarketRegime::Recovery => {}
                    }
                }
            }

            assert!(expansion_count > 0 && recession_count > 0);
            assert!(expansion_total * recession_count > recession_total * expansion_count);
        }
    }

    mod context_equity_rate_shock_rule {
        use super::*;

        #[test]
        fn given_equal_base_return_when_policy_hikes_and_cuts_then_hike_lowers_and_cut_raises_return()
         {
            let base_return = 1_000;
            let hike_shock = equity_rate_shock_ppm(25, 300).expect("hike shock must calculate");
            let cut_shock = equity_rate_shock_ppm(-25, 300).expect("cut shock must calculate");

            let hike_return = apply_equity_rate_shock(base_return, hike_shock)
                .expect("hike-adjusted return must calculate");
            let cut_return = apply_equity_rate_shock(base_return, cut_shock)
                .expect("cut-adjusted return must calculate");

            assert_eq!(hike_shock, -7_500);
            assert_eq!(cut_shock, 7_500);
            assert!(hike_return < base_return);
            assert!(cut_return > base_return);
        }
    }

    mod context_nullable_cache_state {
        use super::*;

        #[test]
        fn given_all_null_rate_columns_when_completed_then_legacy_factor_is_absent() {
            let nullable = NullableInterestRateState {
                policy_rate_bp: None,
                treasury_3m_bp: None,
                treasury_1y_bp: None,
                treasury_3y_bp: None,
                treasury_10y_bp: None,
                policy_rate_change_bp: None,
                equity_rate_shock_ppm: None,
            };

            let rates = nullable.into_complete().expect("legacy row must be valid");

            assert_eq!(rates, None);
        }

        #[test]
        fn given_partially_populated_rate_columns_when_completed_then_corruption_is_rejected() {
            let nullable = NullableInterestRateState {
                policy_rate_bp: Some(250),
                treasury_3m_bp: Some(255),
                treasury_1y_bp: None,
                treasury_3y_bp: Some(280),
                treasury_10y_bp: Some(310),
                policy_rate_change_bp: Some(0),
                equity_rate_shock_ppm: Some(0),
            };

            let result = nullable.into_complete();

            assert!(matches!(result, Err(MarketError::InvalidRateState(_))));
        }

        #[test]
        fn given_all_rate_columns_when_completed_then_typed_factor_is_restored() {
            let nullable = NullableInterestRateState {
                policy_rate_bp: Some(250),
                treasury_3m_bp: Some(255),
                treasury_1y_bp: Some(265),
                treasury_3y_bp: Some(280),
                treasury_10y_bp: Some(310),
                policy_rate_change_bp: Some(0),
                equity_rate_shock_ppm: Some(0),
            };

            let rates = nullable
                .into_complete()
                .expect("complete row must be valid");

            assert_eq!(
                rates,
                Some(InterestRateState {
                    policy_rate_bp: 250,
                    treasury_3m_bp: 255,
                    treasury_1y_bp: 265,
                    treasury_3y_bp: 280,
                    treasury_10y_bp: 310,
                    policy_rate_change_bp: 0,
                    equity_rate_shock_ppm: 0,
                })
            );
        }
    }
}

mod m2_market_factors {
    use super::*;

    const ZERO_INNOVATION_WORD: u64 = 0xffff_ffff_0000_0000;

    fn given_m2_state(day: &MarketDay) -> &M2MarketState {
        day.m2.as_ref().expect("v4 day must contain M2 factors")
    }

    mod context_day_zero_anchors {
        use super::*;

        #[test]
        fn given_v4_world_when_day_zero_generated_then_cpi_llx_and_gold_use_fixed_anchors() {
            let generator = given_m2_generator();
            let world = given_m2_world();

            let day = generator.day_zero(&world).expect("day zero must generate");

            assert_eq!(
                day.m2,
                Some(M2MarketState {
                    cpi_index: 1_000_000,
                    cpi_remainder: 0,
                    llx_close_krw: 100_000,
                    llx_return_ppm: 0,
                    llx_fee_remainder: 0,
                    llx_fee_accumulator_ppm: 0,
                    gold_close_krw_per_gram: 120_000,
                    gold_prior_open_cpi_index: 1_000_000,
                    gold_prior_open_treasury_10y_bp: 310,
                })
            );
        }
    }

    mod context_actual_365_remainders {
        use super::*;

        #[test]
        fn given_cpi_anchor_when_two_calendar_days_advance_then_floor_increases_and_remainders_carry()
         {
            let generator = given_m2_generator_with_gold_word(ZERO_INNOVATION_WORD);
            let world = given_m2_world();

            let days = when_generating_through(generator.as_ref(), &world, 2);
            let first = given_m2_state(&days[1]);
            let second = given_m2_state(&days[2]);

            assert_eq!(
                (first.cpi_index, first.cpi_remainder),
                (1_000_054, 290_000_000)
            );
            assert_eq!(
                (second.cpi_index, second.cpi_remainder),
                (1_000_109, 216_080_000)
            );
        }

        #[test]
        fn given_annual_llx_fee_when_days_advance_then_daily_ppm_and_remainder_use_actual_365() {
            let generator = given_m2_generator_with_gold_word(ZERO_INNOVATION_WORD);
            let world = given_m2_world();

            let days = when_generating_through(generator.as_ref(), &world, 3);
            let friday = given_m2_state(&days[1]);
            let saturday = given_m2_state(&days[2]);
            let sunday = given_m2_state(&days[3]);

            assert_eq!(
                (friday.llx_fee_remainder, friday.llx_fee_accumulator_ppm),
                (40, 0)
            );
            assert_eq!(
                (saturday.llx_fee_remainder, saturday.llx_fee_accumulator_ppm),
                (80, 4)
            );
            assert_eq!(
                (sunday.llx_fee_remainder, sunday.llx_fee_accumulator_ppm),
                (120, 8)
            );
        }
    }

    mod context_consecutive_market_closures {
        use super::*;

        #[test]
        fn given_weekend_closures_when_monday_opens_then_prices_carry_and_pending_state_applies_once()
         {
            let generator = given_m2_generator_with_gold_word(ZERO_INNOVATION_WORD);
            let world = given_m2_world();

            let days = when_generating_through(generator.as_ref(), &world, 4);
            let friday = given_m2_state(&days[1]);
            let saturday = given_m2_state(&days[2]);
            let sunday = given_m2_state(&days[3]);
            let monday = given_m2_state(&days[4]);

            assert_eq!(saturday.llx_close_krw, friday.llx_close_krw);
            assert_eq!(sunday.llx_close_krw, friday.llx_close_krw);
            assert_eq!(friday.llx_return_ppm, days[1].equity_return_ppm - 4);
            assert_eq!(saturday.llx_return_ppm, 0);
            assert_eq!(sunday.llx_return_ppm, 0);
            assert_eq!(monday.llx_return_ppm, days[4].equity_return_ppm - 12);
            assert_eq!(
                saturday.gold_close_krw_per_gram,
                friday.gold_close_krw_per_gram
            );
            assert_eq!(
                sunday.gold_close_krw_per_gram,
                friday.gold_close_krw_per_gram
            );
            assert_eq!(monday.llx_fee_accumulator_ppm, 0);
            assert_eq!(monday.gold_prior_open_cpi_index, monday.cpi_index);
            assert_ne!(monday.llx_close_krw, sunday.llx_close_krw);
            assert_ne!(
                monday.gold_close_krw_per_gram,
                sunday.gold_close_krw_per_gram
            );
        }

        #[test]
        fn given_closed_days_when_generated_then_gold_innovation_is_sampled_only_on_open_sessions()
        {
            let entropy = Arc::new(M2StreamEntropy {
                gold_word: ZERO_INNOVATION_WORD,
                gold_samples: AtomicU32::new(0),
            });
            let registry = create_market_generator_registry_with_entropy(entropy.clone())
                .expect("calendar registry must be valid");
            let generator = registry
                .generator_for(&m2_market_calibration())
                .expect("v4 generator must be registered");
            let world = given_m2_world();

            let days = when_generating_through(generator.as_ref(), &world, 4);

            let open_sessions = days
                .iter()
                .filter(|day| day.game_day > 0 && day.market_open)
                .count();
            assert_eq!(
                entropy.gold_samples.load(Ordering::Relaxed),
                u32::try_from(open_sessions).expect("test horizon must fit")
            );
        }
    }

    mod context_independent_gold_entropy {
        use super::*;

        #[test]
        fn given_only_gold_stream_changes_when_v4_paths_generate_then_equity_rates_cpi_and_llx_match()
         {
            let first_generator = given_m2_generator_with_gold_word(0);
            let second_generator = given_m2_generator_with_gold_word(u64::MAX);
            let world = given_m2_world();

            let first = when_generating_through(first_generator.as_ref(), &world, 40);
            let second = when_generating_through(second_generator.as_ref(), &world, 40);

            for (left, right) in first.iter().zip(&second) {
                assert_eq!(left.equity_close_krw, right.equity_close_krw);
                assert_eq!(left.equity_return_ppm, right.equity_return_ppm);
                assert_eq!(left.rates, right.rates);
                let left_m2 = given_m2_state(left);
                let right_m2 = given_m2_state(right);
                assert_eq!(left_m2.cpi_index, right_m2.cpi_index);
                assert_eq!(left_m2.llx_close_krw, right_m2.llx_close_krw);
            }
            assert!(first.iter().zip(&second).any(|(left, right)| {
                given_m2_state(left).gold_close_krw_per_gram
                    != given_m2_state(right).gold_close_krw_per_gram
            }));
        }
    }

    mod context_ten_year_rate_sensitivity {
        use super::*;

        #[test]
        fn given_equal_cpi_and_innovation_when_ten_year_yield_rises_then_gold_return_falls() {
            let rise =
                gold_rate_return_adjustment_ppm(10, -250).expect("rate adjustment must calculate");
            let fall =
                gold_rate_return_adjustment_ppm(-10, -250).expect("rate adjustment must calculate");

            assert_eq!(rise, -2_500);
            assert_eq!(fall, 2_500);
            assert!(rise < fall);
        }
    }

    mod context_nullable_v4_cache_state {
        use super::*;

        fn given_nullable_m2_state(value: Option<i64>) -> NullableM2MarketState {
            NullableM2MarketState {
                cpi_index: value,
                cpi_remainder: value,
                llx_close_krw: value,
                llx_return_ppm: value,
                llx_fee_remainder: value,
                llx_fee_accumulator_ppm: value,
                gold_close_krw_per_gram: value,
                gold_prior_open_cpi_index: value,
                gold_prior_open_treasury_10y_bp: value,
            }
        }

        #[test]
        fn given_all_null_v4_columns_when_completed_then_legacy_factor_is_absent() {
            let nullable = given_nullable_m2_state(None);

            let state = nullable.into_complete().expect("legacy row must be valid");

            assert_eq!(state, None);
        }

        #[test]
        fn given_one_missing_v4_column_when_completed_then_corruption_is_rejected() {
            let mut nullable = given_nullable_m2_state(Some(1));
            nullable.gold_prior_open_cpi_index = None;

            let result = nullable.into_complete();

            assert!(matches!(result, Err(MarketError::InvalidM2State(_))));
        }

        #[test]
        fn given_all_v4_columns_when_completed_then_typed_factor_is_restored() {
            let nullable = NullableM2MarketState {
                cpi_index: Some(1_000_000),
                cpi_remainder: Some(0),
                llx_close_krw: Some(100_000),
                llx_return_ppm: Some(0),
                llx_fee_remainder: Some(0),
                llx_fee_accumulator_ppm: Some(0),
                gold_close_krw_per_gram: Some(120_000),
                gold_prior_open_cpi_index: Some(1_000_000),
                gold_prior_open_treasury_10y_bp: Some(310),
            };

            let state = nullable.into_complete().expect("v4 row must be valid");

            assert_eq!(
                state,
                Some(M2MarketState {
                    cpi_index: 1_000_000,
                    cpi_remainder: 0,
                    llx_close_krw: 100_000,
                    llx_return_ppm: 0,
                    llx_fee_remainder: 0,
                    llx_fee_accumulator_ppm: 0,
                    gold_close_krw_per_gram: 120_000,
                    gold_prior_open_cpi_index: 1_000_000,
                    gold_prior_open_treasury_10y_bp: 310,
                })
            );
        }
    }

    mod context_restart_from_cached_v4_day {
        use super::*;

        #[test]
        fn given_same_v4_world_when_generation_restarts_from_cached_day_then_path_matches_one_batch()
         {
            let generator = given_m2_generator();
            let world = given_m2_world();
            let day_zero = generator.day_zero(&world).expect("day zero must generate");
            let one_batch = generator
                .generate_through(&world, &day_zero, 500)
                .expect("one batch must generate");

            let mut restarted = generator
                .generate_through(&world, &day_zero, 173)
                .expect("first batch must generate");
            let cached = restarted
                .last()
                .expect("first batch must be non-empty")
                .clone();
            restarted.extend(
                generator
                    .generate_through(&world, &cached, 500)
                    .expect("restarted batch must generate"),
            );

            assert_eq!(one_batch, restarted);
        }
    }
}

#[derive(Debug)]
struct PathStatistics {
    cagr: f64,
    annualized_volatility: f64,
    maximum_drawdown: f64,
    monthly_absolute_return_autocorrelation: f64,
    regime_self_transition: f64,
    mean_regime_dwell_months: f64,
}

fn when_measuring_path(days: &[MarketDay]) -> PathStatistics {
    let first = days.first().expect("path has day zero");
    let last = days.last().expect("path has a final day");
    let elapsed_years = f64::from(last.game_day) / 365.2425;
    let cagr = (last.equity_close_krw as f64 / first.equity_close_krw as f64)
        .powf(1.0 / elapsed_years)
        - 1.0;

    let open_returns: Vec<f64> = days
        .iter()
        .filter(|day| day.market_open)
        .map(|day| day.equity_return_ppm as f64 / 1_000_000.0)
        .collect();
    let annualized_volatility = sample_standard_deviation(&open_returns) * 252.0_f64.sqrt();

    let mut peak = first.equity_close_krw as f64;
    let mut maximum_drawdown = 0.0_f64;
    for day in days {
        let close = day.equity_close_krw as f64;
        peak = peak.max(close);
        maximum_drawdown = maximum_drawdown.min(close / peak - 1.0);
    }

    let month_ends: Vec<&MarketDay> = days
        .iter()
        .filter(|day| {
            day.market_open && day.session_index > 0 && day.session_index.is_multiple_of(21)
        })
        .collect();
    let mut monthly_returns = Vec::with_capacity(month_ends.len());
    let mut prior_close = first.equity_close_krw as f64;
    for month_end in &month_ends {
        let close = month_end.equity_close_krw as f64;
        monthly_returns.push(close / prior_close - 1.0);
        prior_close = close;
    }
    let monthly_absolute_returns: Vec<f64> = monthly_returns
        .iter()
        .map(|monthly_return| monthly_return.abs())
        .collect();
    let monthly_absolute_return_autocorrelation = lag_one_correlation(&monthly_absolute_returns);

    let monthly_regimes: Vec<MarketRegime> = month_ends.iter().map(|day| day.regime).collect();
    let self_transitions = monthly_regimes
        .windows(2)
        .filter(|pair| pair[0] == pair[1])
        .count();
    let regime_self_transition =
        self_transitions as f64 / monthly_regimes.len().saturating_sub(1).max(1) as f64;
    let mean_regime_dwell_months = mean_regime_dwell(&monthly_regimes);

    PathStatistics {
        cagr,
        annualized_volatility,
        maximum_drawdown,
        monthly_absolute_return_autocorrelation,
        regime_self_transition,
        mean_regime_dwell_months,
    }
}

fn sample_standard_deviation(values: &[f64]) -> f64 {
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / values.len().saturating_sub(1).max(1) as f64;
    variance.sqrt()
}

fn lag_one_correlation(values: &[f64]) -> f64 {
    let left = &values[..values.len() - 1];
    let right = &values[1..];
    let left_mean = left.iter().sum::<f64>() / left.len() as f64;
    let right_mean = right.iter().sum::<f64>() / right.len() as f64;
    let covariance = left
        .iter()
        .zip(right)
        .map(|(left_value, right_value)| (left_value - left_mean) * (right_value - right_mean))
        .sum::<f64>();
    let left_variance = left
        .iter()
        .map(|value| (value - left_mean).powi(2))
        .sum::<f64>();
    let right_variance = right
        .iter()
        .map(|value| (value - right_mean).powi(2))
        .sum::<f64>();
    covariance / (left_variance * right_variance).sqrt()
}

fn mean_regime_dwell(regimes: &[MarketRegime]) -> f64 {
    if regimes.is_empty() {
        return 0.0;
    }

    let mut runs = Vec::new();
    let mut run_length = 1_u32;
    for pair in regimes.windows(2) {
        if pair[0] == pair[1] {
            run_length += 1;
        } else {
            runs.push(run_length);
            run_length = 1;
        }
    }
    runs.push(run_length);

    runs.iter().map(|length| f64::from(*length)).sum::<f64>() / runs.len() as f64
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    }
}

mod long_horizon_calibration {
    use super::*;

    mod context_fixed_multi_seed_twenty_five_to_thirty_five_year_paths {
        use super::*;

        fn given_path_statistics() -> Vec<PathStatistics> {
            let generator = given_default_generator();
            let mut statistics =
                Vec::with_capacity(STATISTICAL_SEEDS.len() * STATISTICAL_HORIZONS_IN_DAYS.len());
            for target_game_day in STATISTICAL_HORIZONS_IN_DAYS {
                for seed in STATISTICAL_SEEDS {
                    let days = when_generating_through(
                        generator.as_ref(),
                        &given_default_world_with_seed(seed),
                        target_game_day,
                    );
                    statistics.push(when_measuring_path(&days));
                }
            }
            statistics
        }

        #[test]
        fn given_calibrated_paths_when_aggregated_then_cagr_matches_regression_range() {
            let statistics = given_path_statistics();

            let value = median(statistics.iter().map(|statistic| statistic.cagr).collect());

            assert!((0.02..=0.05).contains(&value), "median CAGR: {value}");
        }

        #[test]
        fn given_calibrated_paths_when_aggregated_then_volatility_matches_regression_range() {
            let statistics = given_path_statistics();

            let value = median(
                statistics
                    .iter()
                    .map(|statistic| statistic.annualized_volatility)
                    .collect(),
            );

            assert!(
                (0.17..=0.22).contains(&value),
                "median annualized volatility: {value}"
            );
        }

        #[test]
        fn given_calibrated_paths_when_aggregated_then_drawdown_matches_regression_range() {
            let statistics = given_path_statistics();

            let value = median(
                statistics
                    .iter()
                    .map(|statistic| statistic.maximum_drawdown)
                    .collect(),
            );

            assert!(
                (-0.75..=-0.45).contains(&value),
                "median maximum drawdown: {value}"
            );
        }

        #[test]
        fn given_calibrated_paths_when_aggregated_then_clustering_matches_regression_range() {
            let statistics = given_path_statistics();

            let value = median(
                statistics
                    .iter()
                    .map(|statistic| statistic.monthly_absolute_return_autocorrelation)
                    .collect(),
            );

            assert!(
                (0.15..=0.35).contains(&value),
                "median monthly absolute-return autocorrelation: {value}"
            );
        }

        #[test]
        fn given_calibrated_paths_when_aggregated_then_self_transition_matches_regression_range() {
            let statistics = given_path_statistics();

            let value = median(
                statistics
                    .iter()
                    .map(|statistic| statistic.regime_self_transition)
                    .collect(),
            );

            assert!(
                (0.84..=0.89).contains(&value),
                "median monthly self-transition: {value}"
            );
        }

        #[test]
        fn given_calibrated_paths_when_aggregated_then_dwell_matches_regression_range() {
            let statistics = given_path_statistics();

            let value = median(
                statistics
                    .iter()
                    .map(|statistic| statistic.mean_regime_dwell_months)
                    .collect(),
            );

            assert!(
                (5.0..=10.0).contains(&value),
                "median regime dwell months: {value}"
            );
        }
    }
}
