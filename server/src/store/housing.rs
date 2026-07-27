//! Server-authoritative real-estate index and monthly listing cache (§5.1, §5.5).

use std::str::FromStr;

use anyhow::{Context, Result, bail, ensure};
use sqlx::{MySql, MySqlPool, Transaction};
use time::{Date, Duration, Month};

use super::types::{
    HousingListingState, HousingListingsQueryState, HousingListingsState, HousingRateStatusState,
    HousingRegionState,
};
use crate::finance::ResourceId;
use crate::life::{
    LifeRegionKey, PropertyListing, PropertyListingAvailabilityRule,
    PropertyListingGenerationInput, PropertyListingOffer, PropertyOfferRotationRule, PropertyType,
    REAL_ESTATE_MAX_LISTINGS_PER_REGION, REAL_ESTATE_MAX_PUBLIC_LISTING_ID, RealEstateDaily,
    RealEstateDayZeroInput, RealEstateIndexState, RealEstateNextDayInput, RealEstateRegionProfile,
    RealEstateRules, YearMonth,
};

const C1_LISTING_COUNT: usize = 12;
const MAX_TRANSACTION_ATTEMPTS: usize = 3;

#[derive(Debug, sqlx::FromRow)]
struct SavePresenceRow {
    has_character: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct HousingScopeRow {
    game_day: u32,
    market_world_id: u64,
    real_estate_model_version_id: u64,
    world_seed: u64,
    world_start_date: Date,
    market_date: Date,
    model_availability: String,
    model_sealed: bool,
    model_canonical_sha256: Option<String>,
    residence_region_key: String,
}

#[derive(Debug, sqlx::FromRow)]
struct HousingCatalogScopeRow {
    game_day: u32,
    market_world_id: u64,
    real_estate_model_version_id: u64,
    world_seed: u64,
    world_start_date: Date,
    market_date: Date,
    model_availability: String,
    model_sealed: bool,
    model_canonical_sha256: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct HeldPropertyDailyScopeRow {
    save_game_day: u32,
    market_world_id: u64,
    real_estate_model_version_id: u64,
    world_seed: u64,
    model_availability: String,
    model_sealed: bool,
    model_canonical_sha256: Option<String>,
    region_key: String,
    target_market_exists: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct RegionCatalogRow {
    region_key: String,
    display_name: String,
    region_order: u8,
}

#[derive(Debug, sqlx::FromRow)]
struct ModelManifestRow {
    manifest_sha256: String,
    manifest_json: String,
    projection_json: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct RegionProfileRow {
    region_key: String,
    monthly_listing_slot_count: u8,
    minimum_exclusive_area_square_meters: u16,
    maximum_exclusive_area_square_meters: u16,
    base_price_per_square_meter_krw: i64,
    price_daily_drift_ppm: i32,
    price_daily_shock_amplitude_ppm: u32,
    rent_daily_drift_ppm: i32,
    rent_daily_shock_amplitude_ppm: u32,
    minimum_index_ppm: i64,
    maximum_index_ppm: i64,
    minimum_price_variation_ppm: i64,
    maximum_price_variation_ppm: i64,
    jeonse_ratio_ppm: u32,
    annual_gross_rent_yield_ppm: u32,
    monthly_deposit_ratio_ppm: u32,
    availability_rule: String,
    offer_rotation_rule: String,
}

#[derive(Debug, sqlx::FromRow)]
struct AllowedPropertyTypeRow {
    property_type: String,
    property_type_order: u8,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct StoredDailyRow {
    region_key: String,
    game_day: u32,
    price_index_ppm: i64,
    price_remainder_numerator: i64,
    rent_index_ppm: i64,
    rent_remainder_numerator: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct MonthCatalogRow {
    expected_listing_count: u8,
    completed: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct StoredListingRow {
    id: u64,
    year_month: Date,
    region_key: String,
    slot_no: u8,
    property_type: String,
    exclusive_area_square_meters: u16,
    price_variation_ppm: u64,
    available_from_game_day: u32,
    available_to_game_day: u32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct StoredOfferRow {
    property_listing_id: u64,
    offer_order: u8,
    offer_kind: String,
    price_krw: Option<i64>,
    deposit_krw: Option<i64>,
    monthly_rent_krw: Option<i64>,
}

pub(super) async fn read_housing_listings(
    pool: &MySqlPool,
    rules: &dyn RealEstateRules,
    user_id: u64,
    query: HousingListingsQueryState,
) -> Result<Option<HousingListingsState>> {
    let mut last_retryable = None;
    for _ in 0..MAX_TRANSACTION_ATTEMPTS {
        match read_housing_listings_once(pool, rules, user_id, query).await {
            Ok(state) => return Ok(state),
            Err(error) if is_retryable_database_error(&error) => last_retryable = Some(error),
            Err(error) => return Err(error),
        }
    }

    Err(last_retryable.context("housing transaction retry ended without a database error")?)
}

pub(super) async fn prepare_current_housing_catalogs(
    pool: &MySqlPool,
    rules: &dyn RealEstateRules,
    user_id: u64,
) -> Result<()> {
    let mut last_retryable = None;
    for _ in 0..MAX_TRANSACTION_ATTEMPTS {
        match prepare_current_housing_catalogs_once(pool, rules, user_id).await {
            Ok(()) => return Ok(()),
            Err(error) if is_retryable_database_error(&error) => last_retryable = Some(error),
            Err(error) => return Err(error),
        }
    }

    Err(last_retryable.context("housing catalog retry ended without a database error")?)
}

pub(super) async fn prepare_property_daily_for_target(
    pool: &MySqlPool,
    rules: &dyn RealEstateRules,
    user_id: u64,
    target_game_day: u32,
) -> Result<()> {
    let mut last_retryable = None;
    for _ in 0..MAX_TRANSACTION_ATTEMPTS {
        match prepare_property_daily_for_target_once(pool, rules, user_id, target_game_day).await {
            Ok(()) => return Ok(()),
            Err(error) if is_retryable_database_error(&error) => last_retryable = Some(error),
            Err(error) => return Err(error),
        }
    }

    Err(last_retryable.context("property daily retry ended without a database error")?)
}

async fn prepare_property_daily_for_target_once(
    pool: &MySqlPool,
    rules: &dyn RealEstateRules,
    user_id: u64,
    target_game_day: u32,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let result =
        prepare_property_daily_for_target_in_tx(&mut tx, rules, user_id, target_game_day).await;
    match result {
        Ok(()) => {
            tx.commit().await?;
            Ok(())
        }
        Err(error) => {
            let _ = tx.rollback().await;
            Err(error)
        }
    }
}

async fn prepare_property_daily_for_target_in_tx(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn RealEstateRules,
    user_id: u64,
    target_game_day: u32,
) -> Result<()> {
    let scope: Option<HeldPropertyDailyScopeRow> = sqlx::query_as(
        r#"
        SELECT save.game_day AS save_game_day,
               bundle.market_world_id,
               bundle.real_estate_model_version_id,
               world.seed AS world_seed,
               model.availability AS model_availability,
               (model.sealed_at IS NOT NULL) AS model_sealed,
               model.canonical_sha256 AS model_canonical_sha256,
               holding.region_key,
               EXISTS(
                   SELECT 1 FROM market_daily
                   WHERE market_daily.world_id = bundle.market_world_id
                     AND market_daily.game_day = ?
               ) AS target_market_exists
        FROM save
        INNER JOIN run_rule_bundle AS bundle
            ON bundle.save_id = save.id
           AND bundle.run_revision = save.run_revision
           AND bundle.market_world_id = save.market_world_id
        INNER JOIN market_world AS world ON world.id = bundle.market_world_id
        INNER JOIN real_estate_model_version AS model
            ON model.id = bundle.real_estate_model_version_id
        INNER JOIN property_holding AS holding
            ON holding.save_id = save.id
           AND holding.run_revision = save.run_revision
           AND holding.real_estate_model_version_id = bundle.real_estate_model_version_id
           AND holding.status = 'active'
        WHERE save.user_id = ?
          AND EXISTS (
              SELECT 1 FROM `character`
              WHERE `character`.save_id = save.id
          )
        "#,
    )
    .bind(target_game_day)
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read the held-property daily scope")?;
    let Some(scope) = scope else {
        return Ok(());
    };
    ensure!(
        scope
            .save_game_day
            .checked_add(1)
            .is_some_and(|next| next == target_game_day),
        "property daily preparation target is not the next player day"
    );
    ensure!(
        scope.target_market_exists,
        "property daily preparation requires the target market day"
    );
    ensure!(
        scope.real_estate_model_version_id > 0
            && scope.model_availability == "active"
            && scope.model_sealed,
        "property daily preparation requires an active sealed model"
    );
    let canonical_sha256 = scope
        .model_canonical_sha256
        .as_deref()
        .context("held-property model is missing its canonical hash")?;
    ensure!(
        is_canonical_sha256(canonical_sha256),
        "held-property model canonical hash is malformed"
    );
    validate_active_manifest(tx, scope.real_estate_model_version_id, canonical_sha256).await?;
    let region_key = LifeRegionKey::from_str(&scope.region_key)
        .context("held property has an unknown region")?;
    let profile = read_region_profile(tx, scope.real_estate_model_version_id, region_key).await?;
    rules
        .day_zero(RealEstateDayZeroInput { profile })
        .context("held-property profile is not accepted by the real-estate rules")?;
    ensure_daily_series(
        tx,
        rules,
        scope.market_world_id,
        scope.world_seed,
        ResourceId::from_u64(scope.real_estate_model_version_id),
        profile,
        target_game_day,
    )
    .await?;
    Ok(())
}

async fn prepare_current_housing_catalogs_once(
    pool: &MySqlPool,
    rules: &dyn RealEstateRules,
    user_id: u64,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let result = prepare_current_housing_catalogs_in_tx(&mut tx, rules, user_id).await;
    match result {
        Ok(()) => {
            tx.commit().await?;
            Ok(())
        }
        Err(error) => {
            let _ = tx.rollback().await;
            Err(error)
        }
    }
}

async fn prepare_current_housing_catalogs_in_tx(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn RealEstateRules,
    user_id: u64,
) -> Result<()> {
    let Some(scope) = read_catalog_scope(tx, user_id).await? else {
        return Ok(());
    };
    let expected_market_date = scope
        .world_start_date
        .checked_add(Duration::days(i64::from(scope.game_day)))
        .context("housing catalog market date exceeds the supported calendar")?;
    ensure!(
        scope.market_date == expected_market_date,
        "housing catalog market date does not match the pinned world"
    );
    ensure!(
        scope.real_estate_model_version_id > 0 && scope.model_sealed,
        "housing catalog requires a sealed positive model"
    );
    let model_canonical_sha256 = scope
        .model_canonical_sha256
        .as_deref()
        .context("housing catalog model is missing its canonical hash")?;
    ensure!(
        is_canonical_sha256(model_canonical_sha256),
        "housing catalog model canonical hash is malformed"
    );
    match scope.model_availability.as_str() {
        "disabled" => return Ok(()),
        "active" => {}
        _ => bail!("housing catalog model availability is invalid"),
    }
    validate_active_manifest(
        tx,
        scope.real_estate_model_version_id,
        model_canonical_sha256,
    )
    .await?;

    let year_month = YearMonth {
        year: scope.market_date.year(),
        month: u8::from(scope.market_date.month()),
    };
    ensure!(
        year_month.is_valid(),
        "housing catalog year-month is invalid"
    );
    let window = listing_window(scope.world_start_date, scope.market_date)?;
    ensure!(
        (window.available_from_game_day..=window.available_to_game_day).contains(&scope.game_day),
        "housing catalog game day is outside its market month"
    );
    let model_version_id = ResourceId::from_u64(scope.real_estate_model_version_id);
    for region_key in LifeRegionKey::ALL {
        let profile =
            read_region_profile(tx, scope.real_estate_model_version_id, region_key).await?;
        let allowed_property_types =
            read_allowed_property_types(tx, scope.real_estate_model_version_id, region_key).await?;
        rules
            .day_zero(RealEstateDayZeroInput { profile })
            .context("housing catalog profile is not accepted by the real-estate rules")?;
        ensure!(
            usize::from(profile.monthly_listing_slot_count) == C1_LISTING_COUNT,
            "housing catalog profile must define exactly twelve monthly slots"
        );
        ensure_daily_series(
            tx,
            rules,
            scope.market_world_id,
            scope.world_seed,
            model_version_id,
            profile,
            scope.game_day,
        )
        .await?;
        let month_start_daily = read_daily(
            tx,
            scope.market_world_id,
            scope.real_estate_model_version_id,
            region_key,
            window.available_from_game_day,
        )
        .await?;
        let expected_listings = rules
            .generate_monthly_listings(PropertyListingGenerationInput {
                world_seed: scope.world_seed,
                model_version_id,
                year_month,
                profile,
                allowed_property_types: &allowed_property_types,
                available_from_game_day: window.available_from_game_day,
                available_to_game_day: window.available_to_game_day,
                month_start_daily,
            })
            .context("housing catalog monthly listing generation failed")?;
        validate_generated_catalog(&expected_listings, region_key, year_month, scope.game_day)?;
        ensure_month_catalog(
            tx,
            scope.market_world_id,
            scope.real_estate_model_version_id,
            window.year_month_date,
            region_key,
            &expected_listings,
        )
        .await?;
    }

    Ok(())
}

async fn read_housing_listings_once(
    pool: &MySqlPool,
    rules: &dyn RealEstateRules,
    user_id: u64,
    query: HousingListingsQueryState,
) -> Result<Option<HousingListingsState>> {
    let mut tx = pool.begin().await?;
    let result = read_housing_listings_in_tx(&mut tx, rules, user_id, query).await;
    match result {
        Ok(state) => {
            tx.commit().await?;
            Ok(state)
        }
        Err(error) => {
            let _ = tx.rollback().await;
            Err(error)
        }
    }
}

async fn read_housing_listings_in_tx(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn RealEstateRules,
    user_id: u64,
    query: HousingListingsQueryState,
) -> Result<Option<HousingListingsState>> {
    let presence: Option<SavePresenceRow> = sqlx::query_as(
        r#"
        SELECT EXISTS(
                   SELECT 1
                   FROM `character`
                   WHERE `character`.save_id = save.id
               ) AS has_character
        FROM save
        WHERE save.user_id = ?
        "#,
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(presence) = presence else {
        return Ok(None);
    };
    if !presence.has_character {
        return Ok(None);
    }

    let scope = read_scope(tx, user_id).await?;
    let regions = read_region_catalog(tx).await?;
    let residence_region_key = LifeRegionKey::from_str(&scope.residence_region_key)
        .context("housing residence has an unknown region")?;
    ensure!(
        regions
            .iter()
            .any(|region| region.region_key == residence_region_key),
        "housing residence region is absent from the canonical catalog"
    );
    let selected_region_key = query.region.unwrap_or(residence_region_key);
    ensure!(
        regions
            .iter()
            .any(|region| region.region_key == selected_region_key),
        "housing selected region is absent from the canonical catalog"
    );

    let expected_market_date = scope
        .world_start_date
        .checked_add(Duration::days(i64::from(scope.game_day)))
        .context("housing market date exceeds the supported calendar")?;
    ensure!(
        scope.market_date == expected_market_date,
        "housing current market date does not match the pinned world"
    );
    let year_month = YearMonth {
        year: scope.market_date.year(),
        month: u8::from(scope.market_date.month()),
    };
    ensure!(
        year_month.is_valid(),
        "housing current year-month is invalid"
    );
    ensure!(
        scope.real_estate_model_version_id > 0,
        "housing model ID must be positive"
    );
    let model_version_id = ResourceId::from_u64(scope.real_estate_model_version_id);
    ensure!(scope.model_sealed, "housing model must be sealed");
    let model_canonical_sha256 = scope
        .model_canonical_sha256
        .as_deref()
        .context("housing model is missing its canonical hash")?;
    ensure!(
        is_canonical_sha256(model_canonical_sha256),
        "housing model canonical hash is malformed"
    );

    match scope.model_availability.as_str() {
        "disabled" => {
            return Ok(Some(HousingListingsState {
                rate_status: HousingRateStatusState::RateUnavailable,
                model_version_id,
                game_day: scope.game_day,
                year_month,
                residence_region_key,
                selected_region_key,
                regions,
                price_index_ppm: None,
                rent_index_ppm: None,
                listings: Vec::new(),
            }));
        }
        "active" => {}
        _ => bail!("housing model availability is invalid"),
    }

    validate_active_manifest(
        tx,
        scope.real_estate_model_version_id,
        model_canonical_sha256,
    )
    .await?;
    let profile =
        read_region_profile(tx, scope.real_estate_model_version_id, selected_region_key).await?;
    let allowed_property_types =
        read_allowed_property_types(tx, scope.real_estate_model_version_id, selected_region_key)
            .await?;
    rules
        .day_zero(RealEstateDayZeroInput { profile })
        .context("housing profile is not accepted by the real-estate rules")?;
    ensure!(
        usize::from(profile.monthly_listing_slot_count) == C1_LISTING_COUNT,
        "housing C1 profile must define exactly twelve monthly slots"
    );

    let current_daily = ensure_daily_series(
        tx,
        rules,
        scope.market_world_id,
        scope.world_seed,
        model_version_id,
        profile,
        scope.game_day,
    )
    .await?;
    let window = listing_window(scope.world_start_date, scope.market_date)?;
    ensure!(
        (window.available_from_game_day..=window.available_to_game_day).contains(&scope.game_day),
        "housing current game day is outside its market month"
    );
    let month_start_daily = read_daily(
        tx,
        scope.market_world_id,
        scope.real_estate_model_version_id,
        selected_region_key,
        window.available_from_game_day,
    )
    .await?;

    let expected_listings = rules
        .generate_monthly_listings(PropertyListingGenerationInput {
            world_seed: scope.world_seed,
            model_version_id,
            year_month,
            profile,
            allowed_property_types: &allowed_property_types,
            available_from_game_day: window.available_from_game_day,
            available_to_game_day: window.available_to_game_day,
            month_start_daily,
        })
        .context("housing monthly listing generation failed")?;
    validate_generated_catalog(
        &expected_listings,
        selected_region_key,
        year_month,
        scope.game_day,
    )?;
    let listings = ensure_month_catalog(
        tx,
        scope.market_world_id,
        scope.real_estate_model_version_id,
        window.year_month_date,
        selected_region_key,
        &expected_listings,
    )
    .await?;

    Ok(Some(HousingListingsState {
        rate_status: HousingRateStatusState::Active,
        model_version_id,
        game_day: scope.game_day,
        year_month,
        residence_region_key,
        selected_region_key,
        regions,
        price_index_ppm: Some(current_daily.price.index_ppm),
        rent_index_ppm: Some(current_daily.rent.index_ppm),
        listings,
    }))
}

async fn read_scope(tx: &mut Transaction<'_, MySql>, user_id: u64) -> Result<HousingScopeRow> {
    let mut rows: Vec<HousingScopeRow> = sqlx::query_as(
        r#"
        SELECT save.game_day,
               bundle.market_world_id,
               bundle.real_estate_model_version_id,
               world.seed AS world_seed,
               world.start_date AS world_start_date,
               market_daily.market_date,
               model.availability AS model_availability,
               (model.sealed_at IS NOT NULL) AS model_sealed,
               model.canonical_sha256 AS model_canonical_sha256,
               residence.region_key AS residence_region_key
        FROM save
        INNER JOIN run_rule_bundle AS bundle
            ON bundle.save_id = save.id
           AND bundle.run_revision = save.run_revision
           AND bundle.market_world_id = save.market_world_id
        INNER JOIN market_world AS world ON world.id = bundle.market_world_id
        INNER JOIN market_daily
            ON market_daily.world_id = bundle.market_world_id
           AND market_daily.game_day = save.game_day
        INNER JOIN real_estate_model_version AS model
            ON model.id = bundle.real_estate_model_version_id
        INNER JOIN household
            ON household.save_id = save.id
           AND household.run_revision = save.run_revision
        INNER JOIN residence
            ON residence.save_id = household.save_id
           AND residence.run_revision = household.run_revision
           AND residence.household_id = household.id
           AND residence.effective_from_game_day <= save.game_day
           AND (
               residence.effective_to_game_day IS NULL
               OR residence.effective_to_game_day > save.game_day
           )
        WHERE save.user_id = ?
        ORDER BY residence.id
        "#,
    )
    .bind(user_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() == 1,
        "housing requires exactly one pinned run and current residence"
    );
    rows.pop()
        .context("housing scope disappeared after validation")
}

async fn read_catalog_scope(
    tx: &mut Transaction<'_, MySql>,
    user_id: u64,
) -> Result<Option<HousingCatalogScopeRow>> {
    sqlx::query_as(
        r#"
        SELECT save.game_day,
               bundle.market_world_id,
               bundle.real_estate_model_version_id,
               world.seed AS world_seed,
               world.start_date AS world_start_date,
               market_daily.market_date,
               model.availability AS model_availability,
               (model.sealed_at IS NOT NULL) AS model_sealed,
               model.canonical_sha256 AS model_canonical_sha256
        FROM save
        INNER JOIN run_rule_bundle AS bundle
            ON bundle.save_id = save.id
           AND bundle.run_revision = save.run_revision
           AND bundle.market_world_id = save.market_world_id
        INNER JOIN market_world AS world ON world.id = bundle.market_world_id
        INNER JOIN market_daily
            ON market_daily.world_id = bundle.market_world_id
           AND market_daily.game_day = save.game_day
        INNER JOIN real_estate_model_version AS model
            ON model.id = bundle.real_estate_model_version_id
        WHERE save.user_id = ?
          AND EXISTS (
              SELECT 1 FROM `character`
              WHERE `character`.save_id = save.id
          )
        "#,
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read the current housing catalog scope")
}

async fn read_region_catalog(tx: &mut Transaction<'_, MySql>) -> Result<Vec<HousingRegionState>> {
    let rows: Vec<RegionCatalogRow> = sqlx::query_as(
        r#"
        SELECT region_key, display_name, region_order
        FROM life_region
        ORDER BY region_order, region_key
        "#,
    )
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() == LifeRegionKey::ALL.len(),
        "housing region catalog must contain exactly four rows"
    );

    rows.into_iter()
        .zip(LifeRegionKey::ALL)
        .map(|(row, expected)| {
            let region_key = LifeRegionKey::from_str(&row.region_key)
                .context("housing region catalog contains an unknown key")?;
            ensure!(
                region_key == expected && row.region_order == expected.order(),
                "housing region catalog is not in canonical order"
            );
            ensure!(
                !row.display_name.is_empty() && row.display_name.chars().count() <= 120,
                "housing region display name is invalid"
            );
            Ok(HousingRegionState {
                region_key,
                display_name: row.display_name,
            })
        })
        .collect()
}

async fn validate_active_manifest(
    tx: &mut Transaction<'_, MySql>,
    model_version_id: u64,
    model_canonical_sha256: &str,
) -> Result<()> {
    let rows: Vec<ModelManifestRow> = sqlx::query_as(
        r#"
        SELECT manifest.canonical_sha256 AS manifest_sha256,
               manifest.canonical_json AS manifest_json,
               projection.canonical_json AS projection_json
        FROM real_estate_model_strict_manifest AS manifest
        INNER JOIN real_estate_model_strict_projection AS projection
            ON projection.real_estate_model_version_id
                = manifest.real_estate_model_version_id
        WHERE manifest.real_estate_model_version_id = ?
        "#,
    )
    .bind(model_version_id)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(rows.len() == 1, "housing active model manifest is missing");
    let row = &rows[0];
    ensure!(
        row.manifest_sha256 == model_canonical_sha256,
        "housing active model hash does not match its strict manifest"
    );
    ensure!(
        row.manifest_json == row.projection_json,
        "housing active model strict projection has drifted"
    );
    Ok(())
}

async fn read_region_profile(
    tx: &mut Transaction<'_, MySql>,
    model_version_id: u64,
    region_key: LifeRegionKey,
) -> Result<RealEstateRegionProfile> {
    let rows: Vec<RegionProfileRow> = sqlx::query_as(
        r#"
        SELECT region_key,
               monthly_listing_slot_count,
               minimum_exclusive_area_square_meters,
               maximum_exclusive_area_square_meters,
               base_price_per_square_meter_krw,
               price_daily_drift_ppm,
               price_daily_shock_amplitude_ppm,
               rent_daily_drift_ppm,
               rent_daily_shock_amplitude_ppm,
               minimum_index_ppm,
               maximum_index_ppm,
               minimum_price_variation_ppm,
               maximum_price_variation_ppm,
               jeonse_ratio_ppm,
               annual_gross_rent_yield_ppm,
               monthly_deposit_ratio_ppm,
               availability_rule,
               offer_rotation_rule
        FROM real_estate_region_profile
        WHERE real_estate_model_version_id = ?
          AND BINARY region_key = BINARY ?
        "#,
    )
    .bind(model_version_id)
    .bind(region_key.as_str())
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() == 1,
        "housing selected region profile is missing"
    );
    map_region_profile(&rows[0], region_key)
}

fn map_region_profile(
    row: &RegionProfileRow,
    expected_region_key: LifeRegionKey,
) -> Result<RealEstateRegionProfile> {
    let region_key = LifeRegionKey::from_str(&row.region_key)
        .context("housing profile contains an unknown region")?;
    ensure!(
        region_key == expected_region_key,
        "housing profile region does not match the selected region"
    );
    Ok(RealEstateRegionProfile {
        region_key,
        monthly_listing_slot_count: row.monthly_listing_slot_count,
        minimum_exclusive_area_square_meters: row.minimum_exclusive_area_square_meters,
        maximum_exclusive_area_square_meters: row.maximum_exclusive_area_square_meters,
        base_price_per_square_meter_krw: row.base_price_per_square_meter_krw,
        price_daily_drift_ppm: i64::from(row.price_daily_drift_ppm),
        price_daily_shock_amplitude_ppm: i64::from(row.price_daily_shock_amplitude_ppm),
        rent_daily_drift_ppm: i64::from(row.rent_daily_drift_ppm),
        rent_daily_shock_amplitude_ppm: i64::from(row.rent_daily_shock_amplitude_ppm),
        minimum_index_ppm: row.minimum_index_ppm,
        maximum_index_ppm: row.maximum_index_ppm,
        minimum_price_variation_ppm: row.minimum_price_variation_ppm,
        maximum_price_variation_ppm: row.maximum_price_variation_ppm,
        jeonse_ratio_ppm: i64::from(row.jeonse_ratio_ppm),
        annual_gross_rent_yield_ppm: i64::from(row.annual_gross_rent_yield_ppm),
        monthly_deposit_ratio_ppm: i64::from(row.monthly_deposit_ratio_ppm),
        availability_rule: PropertyListingAvailabilityRule::from_str(&row.availability_rule)
            .context("housing profile availability rule is unknown")?,
        offer_rotation_rule: PropertyOfferRotationRule::from_str(&row.offer_rotation_rule)
            .context("housing profile offer rotation is unknown")?,
    })
}

async fn read_allowed_property_types(
    tx: &mut Transaction<'_, MySql>,
    model_version_id: u64,
    region_key: LifeRegionKey,
) -> Result<Vec<PropertyType>> {
    let rows: Vec<AllowedPropertyTypeRow> = sqlx::query_as(
        r#"
        SELECT property_type, property_type_order
        FROM real_estate_region_property_type
        WHERE real_estate_model_version_id = ?
          AND BINARY region_key = BINARY ?
        ORDER BY property_type_order, property_type
        "#,
    )
    .bind(model_version_id)
    .bind(region_key.as_str())
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        (1..=PropertyType::ALL.len()).contains(&rows.len()),
        "housing profile must allow between one and three property types"
    );

    let mut previous_order = None;
    rows.into_iter()
        .map(|row| {
            let property_type = PropertyType::from_str(&row.property_type)
                .context("housing profile contains an unknown property type")?;
            ensure!(
                row.property_type_order == property_type.order(),
                "housing property type order does not match its key"
            );
            if let Some(previous_order) = previous_order {
                ensure!(
                    previous_order < row.property_type_order,
                    "housing property types are not in strict canonical order"
                );
            }
            previous_order = Some(row.property_type_order);
            Ok(property_type)
        })
        .collect()
}

async fn ensure_daily_series(
    tx: &mut Transaction<'_, MySql>,
    rules: &dyn RealEstateRules,
    market_world_id: u64,
    world_seed: u64,
    model_version_id: ResourceId,
    profile: RealEstateRegionProfile,
    current_game_day: u32,
) -> Result<RealEstateDaily> {
    sqlx::query(
        r#"
        INSERT IGNORE INTO real_estate_region_series_cursor
            (market_world_id, real_estate_model_version_id, region_key, next_game_day)
        VALUES (?, ?, ?, 0)
        "#,
    )
    .bind(market_world_id)
    .bind(model_version_id.get())
    .bind(profile.region_key.as_str())
    .execute(&mut **tx)
    .await?;

    let cursor_rows: Vec<(u32,)> = sqlx::query_as(
        r#"
        SELECT next_game_day
        FROM real_estate_region_series_cursor
        WHERE market_world_id = ?
          AND real_estate_model_version_id = ?
          AND BINARY region_key = BINARY ?
        FOR UPDATE
        "#,
    )
    .bind(market_world_id)
    .bind(model_version_id.get())
    .bind(profile.region_key.as_str())
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        cursor_rows.len() == 1,
        "housing region series cursor is missing"
    );
    let next_game_day = cursor_rows[0].0;
    let stored_rows: Vec<StoredDailyRow> = sqlx::query_as(
        r#"
        SELECT region_key,
               game_day,
               price_index_ppm,
               price_remainder_numerator,
               rent_index_ppm,
               rent_remainder_numerator
        FROM real_estate_daily
        WHERE market_world_id = ?
          AND real_estate_model_version_id = ?
          AND BINARY region_key = BINARY ?
        ORDER BY game_day
        FOR UPDATE
        "#,
    )
    .bind(market_world_id)
    .bind(model_version_id.get())
    .bind(profile.region_key.as_str())
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        stored_rows.len() == usize::try_from(next_game_day)?,
        "housing region cursor and daily row count disagree"
    );

    let mut previous = None;
    let mut requested_daily = None;
    for (expected_game_day, row) in (0..next_game_day).zip(&stored_rows) {
        let actual = map_daily(row, profile.region_key)?;
        ensure!(
            actual.game_day == expected_game_day,
            "housing daily rows are not contiguous"
        );
        let expected = match previous {
            None => rules.day_zero(RealEstateDayZeroInput { profile }),
            Some(previous) => rules.next_day(RealEstateNextDayInput {
                world_seed,
                model_version_id,
                profile,
                previous,
            }),
        }
        .context("housing daily replay failed")?;
        ensure!(
            actual == expected,
            "housing daily cache has drifted from its rules"
        );
        if actual.game_day == current_game_day {
            requested_daily = Some(actual);
        }
        previous = Some(actual);
    }

    if let Some(missing_days) = daily_generation_window(next_game_day, current_game_day) {
        for game_day in missing_days {
            let daily = match previous {
                None => rules.day_zero(RealEstateDayZeroInput { profile }),
                Some(previous) => rules.next_day(RealEstateNextDayInput {
                    world_seed,
                    model_version_id,
                    profile,
                    previous,
                }),
            }
            .context("housing daily generation failed")?;
            ensure!(
                daily.game_day == game_day,
                "housing rules generated the wrong daily sequence number"
            );
            insert_daily(tx, market_world_id, model_version_id, daily).await?;
            let update = sqlx::query(
                r#"
                UPDATE real_estate_region_series_cursor
                SET next_game_day = ?, updated_at = CURRENT_TIMESTAMP(3)
                WHERE market_world_id = ?
                  AND real_estate_model_version_id = ?
                  AND BINARY region_key = BINARY ?
                  AND next_game_day = ?
                "#,
            )
            .bind(
                game_day
                    .checked_add(1)
                    .context("housing daily cursor overflow")?,
            )
            .bind(market_world_id)
            .bind(model_version_id.get())
            .bind(profile.region_key.as_str())
            .bind(game_day)
            .execute(&mut **tx)
            .await?;
            ensure!(
                update.rows_affected() == 1,
                "housing daily cursor did not advance exactly once"
            );
            if daily.game_day == current_game_day {
                requested_daily = Some(daily);
            }
            previous = Some(daily);
        }
    }

    requested_daily.context("housing current daily row is missing")
}

fn daily_generation_window(
    next_game_day: u32,
    requested_game_day: u32,
) -> Option<std::ops::RangeInclusive<u32>> {
    (next_game_day <= requested_game_day).then_some(next_game_day..=requested_game_day)
}

async fn insert_daily(
    tx: &mut Transaction<'_, MySql>,
    market_world_id: u64,
    model_version_id: ResourceId,
    daily: RealEstateDaily,
) -> Result<()> {
    let insert = sqlx::query(
        r#"
        INSERT INTO real_estate_daily
            (
                market_world_id, real_estate_model_version_id, region_key, game_day,
                price_index_ppm, price_remainder_numerator,
                rent_index_ppm, rent_remainder_numerator
            )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(market_world_id)
    .bind(model_version_id.get())
    .bind(daily.region_key.as_str())
    .bind(daily.game_day)
    .bind(daily.price.index_ppm)
    .bind(daily.price.remainder_numerator)
    .bind(daily.rent.index_ppm)
    .bind(daily.rent.remainder_numerator)
    .execute(&mut **tx)
    .await?;
    ensure!(
        insert.rows_affected() == 1,
        "housing daily row was not inserted exactly once"
    );
    Ok(())
}

async fn read_daily(
    tx: &mut Transaction<'_, MySql>,
    market_world_id: u64,
    model_version_id: u64,
    region_key: LifeRegionKey,
    game_day: u32,
) -> Result<RealEstateDaily> {
    let rows: Vec<StoredDailyRow> = sqlx::query_as(
        r#"
        SELECT region_key,
               game_day,
               price_index_ppm,
               price_remainder_numerator,
               rent_index_ppm,
               rent_remainder_numerator
        FROM real_estate_daily
        WHERE market_world_id = ?
          AND real_estate_model_version_id = ?
          AND BINARY region_key = BINARY ?
          AND game_day = ?
        FOR UPDATE
        "#,
    )
    .bind(market_world_id)
    .bind(model_version_id)
    .bind(region_key.as_str())
    .bind(game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(rows.len() == 1, "housing required daily row is missing");
    map_daily(&rows[0], region_key)
}

fn map_daily(row: &StoredDailyRow, expected_region_key: LifeRegionKey) -> Result<RealEstateDaily> {
    let region_key = LifeRegionKey::from_str(&row.region_key)
        .context("housing daily row contains an unknown region")?;
    ensure!(
        region_key == expected_region_key,
        "housing daily row belongs to another region"
    );
    Ok(RealEstateDaily {
        region_key,
        game_day: row.game_day,
        price: RealEstateIndexState {
            index_ppm: row.price_index_ppm,
            remainder_numerator: row.price_remainder_numerator,
        },
        rent: RealEstateIndexState {
            index_ppm: row.rent_index_ppm,
            remainder_numerator: row.rent_remainder_numerator,
        },
    })
}

#[derive(Debug, Clone, Copy)]
struct ListingWindow {
    year_month_date: Date,
    available_from_game_day: u32,
    available_to_game_day: u32,
}

fn listing_window(world_start_date: Date, market_date: Date) -> Result<ListingWindow> {
    let year_month_date = Date::from_calendar_date(market_date.year(), market_date.month(), 1)
        .context("housing month start is invalid")?;
    ensure!(
        year_month_date >= world_start_date,
        "housing world must begin no later than the current market month"
    );
    let (next_year, next_month) = if market_date.month() == Month::December {
        (
            market_date
                .year()
                .checked_add(1)
                .context("housing next year overflow")?,
            Month::January,
        )
    } else {
        (
            market_date.year(),
            Month::try_from(u8::from(market_date.month()) + 1)
                .context("housing next month is invalid")?,
        )
    };
    let next_month_start = Date::from_calendar_date(next_year, next_month, 1)
        .context("housing next month start is invalid")?;
    let month_end = next_month_start
        .previous_day()
        .context("housing month end is invalid")?;
    let available_from_game_day = u32::try_from((year_month_date - world_start_date).whole_days())
        .context("housing month start is before the pinned world")?;
    let available_to_game_day = u32::try_from((month_end - world_start_date).whole_days())
        .context("housing month end is before the pinned world")?;
    ensure!(
        available_from_game_day <= available_to_game_day,
        "housing listing availability window is reversed"
    );
    Ok(ListingWindow {
        year_month_date,
        available_from_game_day,
        available_to_game_day,
    })
}

fn validate_generated_catalog(
    listings: &[PropertyListing],
    region_key: LifeRegionKey,
    year_month: YearMonth,
    current_game_day: u32,
) -> Result<()> {
    ensure!(
        listings.len() == C1_LISTING_COUNT
            && listings.len() <= usize::from(REAL_ESTATE_MAX_LISTINGS_PER_REGION),
        "housing generated catalog must contain exactly twelve bounded listings"
    );
    for (index, listing) in listings.iter().enumerate() {
        ensure!(
            usize::from(listing.slot) == index + 1,
            "housing generated listings are not in strict slot order"
        );
        ensure!(
            listing.id.get() <= REAL_ESTATE_MAX_PUBLIC_LISTING_ID,
            "housing generated listing ID exceeds the public range"
        );
        ensure!(
            listing.region_key == region_key && listing.year_month == year_month,
            "housing generated listing scope does not match the request"
        );
        ensure!(
            (listing.available_from_game_day..=listing.available_to_game_day)
                .contains(&current_game_day),
            "housing generated listing is unavailable on the current game day"
        );
        ensure!(
            listing.offers.len() == 1,
            "housing C1 listings must contain exactly one offer"
        );
        let offer = listing.offers[0];
        ensure!(
            offer.kind().order() == ((listing.slot - 1) % 3) + 1,
            "housing generated offer rotation is not canonical"
        );
    }
    Ok(())
}

async fn ensure_month_catalog(
    tx: &mut Transaction<'_, MySql>,
    market_world_id: u64,
    model_version_id: u64,
    year_month_date: Date,
    region_key: LifeRegionKey,
    expected: &[PropertyListing],
) -> Result<Vec<HousingListingState>> {
    let expected_listing_count = u8::try_from(expected.len())?;
    sqlx::query(
        r#"
        INSERT IGNORE INTO property_listing_month_catalog
            (
                market_world_id, real_estate_model_version_id, `year_month`, region_key,
                expected_listing_count, completed_at
            )
        VALUES (?, ?, ?, ?, ?, NULL)
        "#,
    )
    .bind(market_world_id)
    .bind(model_version_id)
    .bind(year_month_date)
    .bind(region_key.as_str())
    .bind(expected_listing_count)
    .execute(&mut **tx)
    .await?;

    let header_rows: Vec<MonthCatalogRow> = sqlx::query_as(
        r#"
        SELECT expected_listing_count,
               (completed_at IS NOT NULL) AS completed
        FROM property_listing_month_catalog
        WHERE market_world_id = ?
          AND real_estate_model_version_id = ?
          AND `year_month` = ?
          AND BINARY region_key = BINARY ?
        FOR UPDATE
        "#,
    )
    .bind(market_world_id)
    .bind(model_version_id)
    .bind(year_month_date)
    .bind(region_key.as_str())
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        header_rows.len() == 1,
        "housing monthly catalog header is missing"
    );
    let header = &header_rows[0];
    ensure!(
        header.expected_listing_count == expected_listing_count,
        "housing monthly catalog expected count has drifted"
    );

    if !header.completed {
        insert_listing_parents(
            tx,
            market_world_id,
            model_version_id,
            year_month_date,
            region_key,
            expected,
        )
        .await?;
        let parents = read_listing_parents(
            tx,
            market_world_id,
            model_version_id,
            year_month_date,
            region_key,
        )
        .await?;
        validate_stored_parents(&parents, expected, year_month_date, region_key)?;
        insert_listing_offers(tx, expected).await?;
        let offers = read_listing_offers(
            tx,
            market_world_id,
            model_version_id,
            year_month_date,
            region_key,
        )
        .await?;
        validate_stored_offers(&offers, expected)?;

        let update = sqlx::query(
            r#"
            UPDATE property_listing_month_catalog
            SET completed_at = CURRENT_TIMESTAMP(3)
            WHERE market_world_id = ?
              AND real_estate_model_version_id = ?
              AND `year_month` = ?
              AND BINARY region_key = BINARY ?
              AND completed_at IS NULL
            "#,
        )
        .bind(market_world_id)
        .bind(model_version_id)
        .bind(year_month_date)
        .bind(region_key.as_str())
        .execute(&mut **tx)
        .await?;
        ensure!(
            update.rows_affected() == 1,
            "housing monthly catalog did not complete exactly once"
        );
    }

    let parents = read_listing_parents(
        tx,
        market_world_id,
        model_version_id,
        year_month_date,
        region_key,
    )
    .await?;
    validate_stored_parents(&parents, expected, year_month_date, region_key)?;
    let offers = read_listing_offers(
        tx,
        market_world_id,
        model_version_id,
        year_month_date,
        region_key,
    )
    .await?;
    validate_stored_offers(&offers, expected)?;

    Ok(expected.iter().map(to_public_listing_state).collect())
}

async fn insert_listing_parents(
    tx: &mut Transaction<'_, MySql>,
    market_world_id: u64,
    model_version_id: u64,
    year_month_date: Date,
    region_key: LifeRegionKey,
    listings: &[PropertyListing],
) -> Result<()> {
    for listing in listings {
        sqlx::query(
            r#"
            INSERT IGNORE INTO property_listing
                (
                    id, market_world_id, real_estate_model_version_id, `year_month`,
                    region_key, slot_no, property_type, exclusive_area_square_meters,
                    price_variation_ppm, available_from_game_day, available_to_game_day
                )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(listing.id.get())
        .bind(market_world_id)
        .bind(model_version_id)
        .bind(year_month_date)
        .bind(region_key.as_str())
        .bind(listing.slot)
        .bind(listing.property_type.as_str())
        .bind(listing.exclusive_area_square_meters)
        .bind(listing.price_variation_ppm)
        .bind(listing.available_from_game_day)
        .bind(listing.available_to_game_day)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn insert_listing_offers(
    tx: &mut Transaction<'_, MySql>,
    listings: &[PropertyListing],
) -> Result<()> {
    for listing in listings {
        for offer in &listing.offers {
            let (price_krw, deposit_krw, monthly_rent_krw) = match *offer {
                PropertyListingOffer::Sale { price_krw } => (Some(price_krw), None, None),
                PropertyListingOffer::Jeonse { deposit_krw } => (None, Some(deposit_krw), None),
                PropertyListingOffer::MonthlyRent {
                    deposit_krw,
                    monthly_rent_krw,
                } => (None, Some(deposit_krw), Some(monthly_rent_krw)),
            };
            sqlx::query(
                r#"
                INSERT IGNORE INTO property_listing_offer
                    (
                        property_listing_id, offer_order, offer_kind,
                        price_krw, deposit_krw, monthly_rent_krw
                    )
                VALUES (?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(listing.id.get())
            .bind(offer.kind().order())
            .bind(offer.kind().as_str())
            .bind(price_krw)
            .bind(deposit_krw)
            .bind(monthly_rent_krw)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

async fn read_listing_parents(
    tx: &mut Transaction<'_, MySql>,
    market_world_id: u64,
    model_version_id: u64,
    year_month_date: Date,
    region_key: LifeRegionKey,
) -> Result<Vec<StoredListingRow>> {
    sqlx::query_as(
        r#"
        SELECT id,
               `year_month`,
               region_key,
               slot_no,
               property_type,
               exclusive_area_square_meters,
               price_variation_ppm,
               available_from_game_day,
               available_to_game_day
        FROM property_listing
        WHERE market_world_id = ?
          AND real_estate_model_version_id = ?
          AND `year_month` = ?
          AND BINARY region_key = BINARY ?
        ORDER BY slot_no
        FOR UPDATE
        "#,
    )
    .bind(market_world_id)
    .bind(model_version_id)
    .bind(year_month_date)
    .bind(region_key.as_str())
    .fetch_all(&mut **tx)
    .await
    .map_err(Into::into)
}

async fn read_listing_offers(
    tx: &mut Transaction<'_, MySql>,
    market_world_id: u64,
    model_version_id: u64,
    year_month_date: Date,
    region_key: LifeRegionKey,
) -> Result<Vec<StoredOfferRow>> {
    sqlx::query_as(
        r#"
        SELECT offer.property_listing_id,
               offer.offer_order,
               offer.offer_kind,
               offer.price_krw,
               offer.deposit_krw,
               offer.monthly_rent_krw
        FROM property_listing AS listing
        INNER JOIN property_listing_offer AS offer
            ON offer.property_listing_id = listing.id
        WHERE listing.market_world_id = ?
          AND listing.real_estate_model_version_id = ?
          AND listing.`year_month` = ?
          AND BINARY listing.region_key = BINARY ?
        ORDER BY listing.slot_no, offer.offer_order
        FOR UPDATE
        "#,
    )
    .bind(market_world_id)
    .bind(model_version_id)
    .bind(year_month_date)
    .bind(region_key.as_str())
    .fetch_all(&mut **tx)
    .await
    .map_err(Into::into)
}

fn validate_stored_parents(
    stored: &[StoredListingRow],
    expected: &[PropertyListing],
    year_month_date: Date,
    region_key: LifeRegionKey,
) -> Result<()> {
    ensure!(
        stored.len() == expected.len(),
        "housing stored listing count does not match the canonical catalog"
    );
    for (stored, expected) in stored.iter().zip(expected) {
        let stored_region_key = LifeRegionKey::from_str(&stored.region_key)
            .context("housing stored listing contains an unknown region")?;
        let stored_property_type = PropertyType::from_str(&stored.property_type)
            .context("housing stored listing contains an unknown property type")?;
        let stored_variation = i64::try_from(stored.price_variation_ppm)
            .context("housing stored listing variation is out of range")?;
        ensure!(
            stored.id == expected.id.get()
                && stored.id <= REAL_ESTATE_MAX_PUBLIC_LISTING_ID
                && stored.year_month == year_month_date
                && stored_region_key == region_key
                && stored_region_key == expected.region_key
                && stored.slot_no == expected.slot
                && stored_property_type == expected.property_type
                && stored.exclusive_area_square_meters == expected.exclusive_area_square_meters
                && stored_variation == expected.price_variation_ppm
                && stored.available_from_game_day == expected.available_from_game_day
                && stored.available_to_game_day == expected.available_to_game_day,
            "housing stored listing does not match its deterministic canonical row"
        );
    }
    Ok(())
}

fn validate_stored_offers(
    stored: &[StoredOfferRow],
    expected_listings: &[PropertyListing],
) -> Result<()> {
    let expected_count = expected_listings
        .iter()
        .map(|listing| listing.offers.len())
        .sum::<usize>();
    ensure!(
        stored.len() == expected_count,
        "housing stored offer count does not match the canonical catalog"
    );
    let mut stored_index = 0;
    for listing in expected_listings {
        ensure!(
            listing.offers.len() == 1 && listing.offers.len() <= 3,
            "housing expected offer count is outside the public bound"
        );
        let mut previous_order = None;
        for expected_offer in &listing.offers {
            let stored_offer = stored
                .get(stored_index)
                .context("housing stored offer disappeared during validation")?;
            stored_index += 1;
            let actual_offer = map_stored_offer(stored_offer)?;
            ensure!(
                stored_offer.property_listing_id == listing.id.get(),
                "housing stored offer belongs to another listing"
            );
            ensure!(
                stored_offer.offer_order == expected_offer.kind().order(),
                "housing stored offer order is not canonical"
            );
            if let Some(previous_order) = previous_order {
                ensure!(
                    previous_order < stored_offer.offer_order,
                    "housing stored offers are not in strict canonical order"
                );
            }
            previous_order = Some(stored_offer.offer_order);
            ensure!(
                actual_offer == *expected_offer,
                "housing stored offer amount or shape has drifted"
            );
        }
    }
    Ok(())
}

fn map_stored_offer(row: &StoredOfferRow) -> Result<PropertyListingOffer> {
    match row.offer_kind.as_str() {
        "sale" => {
            let price_krw = row
                .price_krw
                .filter(|value| *value > 0)
                .context("housing stored sale price is invalid")?;
            ensure!(
                row.deposit_krw.is_none() && row.monthly_rent_krw.is_none(),
                "housing stored sale has non-sale amount fields"
            );
            Ok(PropertyListingOffer::Sale { price_krw })
        }
        "jeonse" => {
            let deposit_krw = row
                .deposit_krw
                .filter(|value| *value > 0)
                .context("housing stored jeonse deposit is invalid")?;
            ensure!(
                row.price_krw.is_none() && row.monthly_rent_krw.is_none(),
                "housing stored jeonse has non-jeonse amount fields"
            );
            Ok(PropertyListingOffer::Jeonse { deposit_krw })
        }
        "monthlyRent" => {
            let deposit_krw = row
                .deposit_krw
                .filter(|value| *value > 0)
                .context("housing stored monthly-rent deposit is invalid")?;
            let monthly_rent_krw = row
                .monthly_rent_krw
                .filter(|value| *value > 0)
                .context("housing stored monthly rent is invalid")?;
            ensure!(
                row.price_krw.is_none(),
                "housing stored monthly rent has a sale price"
            );
            Ok(PropertyListingOffer::MonthlyRent {
                deposit_krw,
                monthly_rent_krw,
            })
        }
        _ => bail!("housing stored offer kind is unknown"),
    }
}

fn to_public_listing_state(listing: &PropertyListing) -> HousingListingState {
    HousingListingState {
        id: listing.id,
        region_key: listing.region_key,
        property_type: listing.property_type,
        exclusive_area_square_meters: listing.exclusive_area_square_meters,
        available_from_game_day: listing.available_from_game_day,
        available_to_game_day: listing.available_to_game_day,
        offers: listing.offers.clone(),
    }
}

pub(super) fn is_retryable_database_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        let Some(sqlx::Error::Database(database)) = cause.downcast_ref::<sqlx::Error>() else {
            return false;
        };
        database
            .try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>()
            .is_some_and(|error| matches!(error.number(), 1205 | 1213))
    })
}

fn is_canonical_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    mod context_지역_profile_row를_typed_rule로_변환할_때 {
        use super::*;

        #[test]
        fn given_엄격한row_when_typed_profile로변환하면_then_모든rule필드가보존된다() {
            let row = given_profile_row();

            let profile = map_region_profile(&row, LifeRegionKey::CapitalArea)
                .expect("유효한 profile row를 변환해야 한다");

            assert_eq!(profile, given_profile());
        }

        #[test]
        fn given_알수없는rule_when_typed_profile로변환하면_then_거부한다() {
            let mut row = given_profile_row();
            row.offer_rotation_rule = "unknown".to_owned();

            let result = map_region_profile(&row, LifeRegionKey::CapitalArea);

            assert!(result.is_err());
        }
    }

    mod context_저장된_listing을_canonical_catalog와_대조할_때 {
        use super::*;

        #[test]
        fn given_canonical과일치하는row_when_검증하면_then_catalog를허용한다() {
            let expected = given_listings();
            let stored = given_stored_listings(&expected);
            let month = given_date(2026, Month::February, 1);

            let result =
                validate_stored_parents(&stored, &expected, month, LifeRegionKey::CapitalArea);

            assert!(result.is_ok());
        }

        #[test]
        fn given_다른stable_id의row_when_검증하면_then_fail_closed한다() {
            let expected = given_listings();
            let mut stored = given_stored_listings(&expected);
            stored[0].id = 9_999;
            let month = given_date(2026, Month::February, 1);

            let result =
                validate_stored_parents(&stored, &expected, month, LifeRegionKey::CapitalArea);

            assert!(result.is_err());
        }
    }

    mod context_저장된_offer_shape를_검증할_때 {
        use super::*;

        #[test]
        fn given_정확한tagged_offer_when_검증하면_then_금액을허용한다() {
            let expected = given_listings();
            let stored = given_stored_offers(&expected);

            let result = validate_stored_offers(&stored, &expected);

            assert!(result.is_ok());
        }

        #[test]
        fn given_deposit이섞인sale_when_검증하면_then_fail_closed한다() {
            let expected = given_listings();
            let mut stored = given_stored_offers(&expected);
            stored[0].deposit_krw = Some(1);

            let result = validate_stored_offers(&stored, &expected);

            assert!(result.is_err());
        }
    }

    mod context_현재_market_month의_공급기간을_계산할_때 {
        use super::*;

        #[test]
        fn given_윤년2월_when_공급기간을계산하면_then_월의첫날과마지막날을포함한다() {
            let world_start = given_date(2028, Month::January, 1);
            let market_date = given_date(2028, Month::February, 20);

            let window = listing_window(world_start, market_date)
                .expect("윤년 2월 공급기간을 계산해야 한다");

            assert_eq!(window.year_month_date, given_date(2028, Month::February, 1));
            assert_eq!(window.available_from_game_day, 31);
            assert_eq!(window.available_to_game_day, 59);
        }
    }

    mod context_공유_region_series에서_요청일을_준비할_때 {
        use super::*;

        #[test]
        fn given_요청일보다앞선cursor_when_생성범위를정하면_then_요청일까지의누락일만포함한다() {
            let next_game_day = 7;

            let result = daily_generation_window(next_game_day, 10);

            assert_eq!(result, Some(7..=10));
        }

        #[test]
        fn given_다른run이요청일보다앞서생성한cursor_when_생성범위를정하면_then_새행을만들지않는다()
        {
            let next_game_day = 31;

            let result = daily_generation_window(next_game_day, 0);

            assert_eq!(result, None);
        }
    }

    fn given_profile_row() -> RegionProfileRow {
        RegionProfileRow {
            region_key: "capitalArea".to_owned(),
            monthly_listing_slot_count: 12,
            minimum_exclusive_area_square_meters: 20,
            maximum_exclusive_area_square_meters: 180,
            base_price_per_square_meter_krw: 8_000_000,
            price_daily_drift_ppm: 25,
            price_daily_shock_amplitude_ppm: 300,
            rent_daily_drift_ppm: 15,
            rent_daily_shock_amplitude_ppm: 200,
            minimum_index_ppm: 500_000,
            maximum_index_ppm: 2_000_000,
            minimum_price_variation_ppm: 800_000,
            maximum_price_variation_ppm: 1_200_000,
            jeonse_ratio_ppm: 600_000,
            annual_gross_rent_yield_ppm: 45_000,
            monthly_deposit_ratio_ppm: 100_000,
            availability_rule: "marketMonthInclusive".to_owned(),
            offer_rotation_rule: "saleJeonseMonthlyRent".to_owned(),
        }
    }

    fn given_profile() -> RealEstateRegionProfile {
        RealEstateRegionProfile {
            region_key: LifeRegionKey::CapitalArea,
            monthly_listing_slot_count: 12,
            minimum_exclusive_area_square_meters: 20,
            maximum_exclusive_area_square_meters: 180,
            base_price_per_square_meter_krw: 8_000_000,
            price_daily_drift_ppm: 25,
            price_daily_shock_amplitude_ppm: 300,
            rent_daily_drift_ppm: 15,
            rent_daily_shock_amplitude_ppm: 200,
            minimum_index_ppm: 500_000,
            maximum_index_ppm: 2_000_000,
            minimum_price_variation_ppm: 800_000,
            maximum_price_variation_ppm: 1_200_000,
            jeonse_ratio_ppm: 600_000,
            annual_gross_rent_yield_ppm: 45_000,
            monthly_deposit_ratio_ppm: 100_000,
            availability_rule: PropertyListingAvailabilityRule::MarketMonthInclusive,
            offer_rotation_rule: PropertyOfferRotationRule::SaleJeonseMonthlyRent,
        }
    }

    fn given_listings() -> Vec<PropertyListing> {
        (1_u8..=12)
            .map(|slot| PropertyListing {
                id: ResourceId::from_u64(u64::from(slot)),
                year_month: YearMonth {
                    year: 2026,
                    month: 2,
                },
                region_key: LifeRegionKey::CapitalArea,
                slot,
                property_type: PropertyType::Apartment,
                exclusive_area_square_meters: 84,
                price_variation_ppm: 1_000_000,
                available_from_game_day: 31,
                available_to_game_day: 58,
                offers: vec![match (slot - 1) % 3 {
                    0 => PropertyListingOffer::Sale {
                        price_krw: 400_000_000,
                    },
                    1 => PropertyListingOffer::Jeonse {
                        deposit_krw: 240_000_000,
                    },
                    _ => PropertyListingOffer::MonthlyRent {
                        deposit_krw: 40_000_000,
                        monthly_rent_krw: 1_200_000,
                    },
                }],
            })
            .collect()
    }

    fn given_stored_listings(expected: &[PropertyListing]) -> Vec<StoredListingRow> {
        expected
            .iter()
            .map(|listing| StoredListingRow {
                id: listing.id.get(),
                year_month: given_date(2026, Month::February, 1),
                region_key: listing.region_key.as_str().to_owned(),
                slot_no: listing.slot,
                property_type: listing.property_type.as_str().to_owned(),
                exclusive_area_square_meters: listing.exclusive_area_square_meters,
                price_variation_ppm: u64::try_from(listing.price_variation_ppm)
                    .expect("양수 variation이어야 한다"),
                available_from_game_day: listing.available_from_game_day,
                available_to_game_day: listing.available_to_game_day,
            })
            .collect()
    }

    fn given_stored_offers(expected: &[PropertyListing]) -> Vec<StoredOfferRow> {
        expected
            .iter()
            .flat_map(|listing| {
                listing.offers.iter().map(|offer| {
                    let (price_krw, deposit_krw, monthly_rent_krw) = match *offer {
                        PropertyListingOffer::Sale { price_krw } => (Some(price_krw), None, None),
                        PropertyListingOffer::Jeonse { deposit_krw } => {
                            (None, Some(deposit_krw), None)
                        }
                        PropertyListingOffer::MonthlyRent {
                            deposit_krw,
                            monthly_rent_krw,
                        } => (None, Some(deposit_krw), Some(monthly_rent_krw)),
                    };
                    StoredOfferRow {
                        property_listing_id: listing.id.get(),
                        offer_order: offer.kind().order(),
                        offer_kind: offer.kind().as_str().to_owned(),
                        price_krw,
                        deposit_krw,
                        monthly_rent_krw,
                    }
                })
            })
            .collect()
    }

    fn given_date(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("유효한 날짜여야 한다")
    }
}
