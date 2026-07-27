-- M4-C1 immutable real-estate model, regional indices, and finite monthly listings (§5.1, §5.5).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;

CREATE TABLE real_estate_region_profile (
    real_estate_model_version_id         BIGINT UNSIGNED NOT NULL,
    region_key                          VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    monthly_listing_slot_count           TINYINT UNSIGNED  NOT NULL,
    minimum_exclusive_area_square_meters SMALLINT UNSIGNED NOT NULL,
    maximum_exclusive_area_square_meters SMALLINT UNSIGNED NOT NULL,
    base_price_per_square_meter_krw      BIGINT            NOT NULL,
    price_daily_drift_ppm                INT               NOT NULL,
    price_daily_shock_amplitude_ppm      INT UNSIGNED      NOT NULL,
    rent_daily_drift_ppm                 INT               NOT NULL,
    rent_daily_shock_amplitude_ppm       INT UNSIGNED      NOT NULL,
    minimum_index_ppm                    BIGINT            NOT NULL,
    maximum_index_ppm                    BIGINT            NOT NULL,
    minimum_price_variation_ppm          BIGINT            NOT NULL,
    maximum_price_variation_ppm          BIGINT            NOT NULL,
    jeonse_ratio_ppm                     INT UNSIGNED      NOT NULL,
    annual_gross_rent_yield_ppm          INT UNSIGNED      NOT NULL,
    monthly_deposit_ratio_ppm            INT UNSIGNED      NOT NULL,
    availability_rule                    VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    offer_rotation_rule                  VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                           DATETIME(3)        NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (real_estate_model_version_id, region_key),
    CONSTRAINT fk_real_estate_region_profile_model
        FOREIGN KEY (real_estate_model_version_id) REFERENCES real_estate_model_version (id),
    CONSTRAINT fk_real_estate_region_profile_region
        FOREIGN KEY (region_key) REFERENCES life_region (region_key),
    CONSTRAINT ck_real_estate_region_profile_slots CHECK (
        monthly_listing_slot_count BETWEEN 1 AND 24
    ),
    CONSTRAINT ck_real_estate_region_profile_area CHECK (
        minimum_exclusive_area_square_meters BETWEEN 1 AND 10000
        AND maximum_exclusive_area_square_meters
            BETWEEN minimum_exclusive_area_square_meters AND 10000
    ),
    CONSTRAINT ck_real_estate_region_profile_base_price CHECK (
        base_price_per_square_meter_krw BETWEEN 1 AND 1000000000000
    ),
    CONSTRAINT ck_real_estate_region_profile_price_process CHECK (
        price_daily_drift_ppm BETWEEN -999999 AND 999999
        AND price_daily_shock_amplitude_ppm BETWEEN 0 AND 999999
        AND 1000000 + price_daily_drift_ppm - price_daily_shock_amplitude_ppm > 0
    ),
    CONSTRAINT ck_real_estate_region_profile_rent_process CHECK (
        rent_daily_drift_ppm BETWEEN -999999 AND 999999
        AND rent_daily_shock_amplitude_ppm BETWEEN 0 AND 999999
        AND 1000000 + rent_daily_drift_ppm - rent_daily_shock_amplitude_ppm > 0
    ),
    CONSTRAINT ck_real_estate_region_profile_index_bounds CHECK (
        minimum_index_ppm BETWEEN 1 AND 9007199254740991
        AND maximum_index_ppm BETWEEN minimum_index_ppm AND 9007199254740991
        AND minimum_index_ppm <= 1000000
        AND maximum_index_ppm >= 1000000
    ),
    CONSTRAINT ck_real_estate_region_profile_variation CHECK (
        minimum_price_variation_ppm BETWEEN 1 AND 10000000
        AND maximum_price_variation_ppm
            BETWEEN minimum_price_variation_ppm AND 10000000
    ),
    CONSTRAINT ck_real_estate_region_profile_ratios CHECK (
        jeonse_ratio_ppm BETWEEN 1 AND 999999
        AND annual_gross_rent_yield_ppm BETWEEN 1 AND 1000000
        AND monthly_deposit_ratio_ppm BETWEEN 1 AND 999999
    ),
    CONSTRAINT ck_real_estate_region_profile_rules CHECK (
        availability_rule = 'marketMonthInclusive'
        AND offer_rotation_rule = 'saleJeonseMonthlyRent'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE real_estate_region_property_type (
    real_estate_model_version_id BIGINT UNSIGNED NOT NULL,
    region_key                  VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    property_type               VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    property_type_order         TINYINT UNSIGNED NOT NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (real_estate_model_version_id, region_key, property_type),
    UNIQUE KEY uk_real_estate_region_property_type_order
        (real_estate_model_version_id, region_key, property_type_order),
    CONSTRAINT fk_real_estate_region_property_type_profile
        FOREIGN KEY (real_estate_model_version_id, region_key)
        REFERENCES real_estate_region_profile (real_estate_model_version_id, region_key),
    CONSTRAINT ck_real_estate_region_property_type_enum CHECK (
        property_type IN ('apartment', 'multiFamily', 'detached')
    ),
    CONSTRAINT ck_real_estate_region_property_type_order CHECK (
        (property_type = 'apartment' AND property_type_order = 1)
        OR (property_type = 'multiFamily' AND property_type_order = 2)
        OR (property_type = 'detached' AND property_type_order = 3)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE real_estate_model_strict_manifest (
    real_estate_model_version_id BIGINT UNSIGNED NOT NULL,
    canonical_json              LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    canonical_sha256            CHAR(64) CHARACTER SET ascii COLLATE ascii_bin
        GENERATED ALWAYS AS (SHA2(canonical_json, 256)) STORED,
    created_at                  DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (real_estate_model_version_id),
    UNIQUE KEY uk_real_estate_model_strict_manifest_sha (canonical_sha256),
    CONSTRAINT fk_real_estate_model_strict_manifest_model
        FOREIGN KEY (real_estate_model_version_id) REFERENCES real_estate_model_version (id),
    CONSTRAINT ck_real_estate_model_strict_manifest_json CHECK (JSON_VALID(canonical_json))
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE VIEW real_estate_model_strict_projection AS
SELECT
    model.id AS real_estate_model_version_id,
    CAST(JSON_OBJECT(
        'availability', model.availability,
        'parameters', model.parameters,
        'rankedEligible', model.ranked_eligible,
        'regionalProfiles', COALESCE((
            SELECT JSON_ARRAYAGG(JSON_OBJECT(
                       'allowedPropertyTypes', COALESCE((
                           SELECT JSON_ARRAYAGG(allowed.property_type) OVER (
                                      ORDER BY allowed.property_type_order
                                      ROWS BETWEEN UNBOUNDED PRECEDING
                                          AND UNBOUNDED FOLLOWING
                                  )
                           FROM real_estate_region_property_type AS allowed
                           WHERE allowed.real_estate_model_version_id
                                   = profile.real_estate_model_version_id
                             AND BINARY allowed.region_key = BINARY profile.region_key
                           ORDER BY allowed.property_type_order
                           LIMIT 1
                       ), JSON_ARRAY()),
                       'annualGrossRentYieldPpm', profile.annual_gross_rent_yield_ppm,
                       'availabilityRule', profile.availability_rule,
                       'basePricePerSquareMeterKrw', profile.base_price_per_square_meter_krw,
                       'jeonseRatioPpm', profile.jeonse_ratio_ppm,
                       'maximumExclusiveAreaSquareMeters',
                           profile.maximum_exclusive_area_square_meters,
                       'maximumIndexPpm', profile.maximum_index_ppm,
                       'maximumPriceVariationPpm', profile.maximum_price_variation_ppm,
                       'minimumExclusiveAreaSquareMeters',
                           profile.minimum_exclusive_area_square_meters,
                       'minimumIndexPpm', profile.minimum_index_ppm,
                       'minimumPriceVariationPpm', profile.minimum_price_variation_ppm,
                       'monthlyDepositRatioPpm', profile.monthly_deposit_ratio_ppm,
                       'monthlyListingSlotCount', profile.monthly_listing_slot_count,
                       'offerRotationRule', profile.offer_rotation_rule,
                       'priceDailyDriftPpm', profile.price_daily_drift_ppm,
                       'priceDailyShockAmplitudePpm',
                           profile.price_daily_shock_amplitude_ppm,
                       'regionKey', profile.region_key,
                       'rentDailyDriftPpm', profile.rent_daily_drift_ppm,
                       'rentDailyShockAmplitudePpm', profile.rent_daily_shock_amplitude_ppm
                   )) OVER (
                       ORDER BY region.region_order
                       ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING
                   )
            FROM real_estate_region_profile AS profile
            INNER JOIN life_region AS region
                ON BINARY region.region_key = BINARY profile.region_key
            WHERE profile.real_estate_model_version_id = model.id
            ORDER BY region.region_order
            LIMIT 1
        ), JSON_ARRAY()),
        'schemaVersion', 1,
        'versionKey', model.version_key
    ) AS CHAR CHARACTER SET utf8mb4) AS canonical_json
FROM real_estate_model_version AS model
WHERE model.availability = 'active';

CREATE TABLE real_estate_region_series_cursor (
    market_world_id              BIGINT UNSIGNED NOT NULL,
    real_estate_model_version_id BIGINT UNSIGNED NOT NULL,
    region_key                  VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    next_game_day               INT UNSIGNED    NOT NULL DEFAULT 0,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (market_world_id, real_estate_model_version_id, region_key),
    KEY ix_real_estate_region_series_cursor_profile
        (real_estate_model_version_id, region_key),
    CONSTRAINT fk_real_estate_region_series_cursor_world
        FOREIGN KEY (market_world_id) REFERENCES market_world (id),
    CONSTRAINT fk_real_estate_region_series_cursor_profile
        FOREIGN KEY (real_estate_model_version_id, region_key)
        REFERENCES real_estate_region_profile (real_estate_model_version_id, region_key)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE real_estate_daily (
    market_world_id              BIGINT UNSIGNED NOT NULL,
    real_estate_model_version_id BIGINT UNSIGNED NOT NULL,
    region_key                  VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    game_day                    INT UNSIGNED    NOT NULL,
    price_index_ppm             BIGINT          NOT NULL,
    price_remainder_numerator   BIGINT          NOT NULL,
    rent_index_ppm              BIGINT          NOT NULL,
    rent_remainder_numerator    BIGINT          NOT NULL,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (market_world_id, real_estate_model_version_id, region_key, game_day),
    KEY ix_real_estate_daily_profile (real_estate_model_version_id, region_key),
    CONSTRAINT fk_real_estate_daily_world
        FOREIGN KEY (market_world_id) REFERENCES market_world (id),
    CONSTRAINT fk_real_estate_daily_profile
        FOREIGN KEY (real_estate_model_version_id, region_key)
        REFERENCES real_estate_region_profile (real_estate_model_version_id, region_key),
    CONSTRAINT ck_real_estate_daily_index CHECK (
        price_index_ppm BETWEEN 1 AND 9007199254740991
        AND rent_index_ppm BETWEEN 1 AND 9007199254740991
    ),
    CONSTRAINT ck_real_estate_daily_remainder CHECK (
        price_remainder_numerator BETWEEN -999999 AND 999999
        AND rent_remainder_numerator BETWEEN -999999 AND 999999
    ),
    CONSTRAINT ck_real_estate_daily_day0 CHECK (
        game_day <> 0
        OR (
            price_index_ppm = 1000000
            AND price_remainder_numerator = 0
            AND rent_index_ppm = 1000000
            AND rent_remainder_numerator = 0
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE property_listing_month_catalog (
    market_world_id              BIGINT UNSIGNED NOT NULL,
    real_estate_model_version_id BIGINT UNSIGNED NOT NULL,
    `year_month`                 DATE            NOT NULL,
    region_key                  VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    expected_listing_count      TINYINT UNSIGNED NOT NULL,
    completed_at                DATETIME(3)         NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (
        market_world_id, real_estate_model_version_id, `year_month`, region_key
    ),
    KEY ix_property_listing_month_catalog_profile
        (real_estate_model_version_id, region_key),
    CONSTRAINT fk_property_listing_month_catalog_world
        FOREIGN KEY (market_world_id) REFERENCES market_world (id),
    CONSTRAINT fk_property_listing_month_catalog_profile
        FOREIGN KEY (real_estate_model_version_id, region_key)
        REFERENCES real_estate_region_profile (real_estate_model_version_id, region_key),
    CONSTRAINT ck_property_listing_month_catalog_month CHECK (
        DAYOFMONTH(`year_month`) = 1
    ),
    CONSTRAINT ck_property_listing_month_catalog_count CHECK (
        expected_listing_count BETWEEN 1 AND 24
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE property_listing (
    id                               BIGINT UNSIGNED NOT NULL,
    market_world_id                  BIGINT UNSIGNED NOT NULL,
    real_estate_model_version_id     BIGINT UNSIGNED NOT NULL,
    `year_month`                     DATE            NOT NULL,
    region_key                      VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    slot_no                         TINYINT UNSIGNED NOT NULL,
    property_type                   VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    exclusive_area_square_meters    SMALLINT UNSIGNED NOT NULL,
    price_variation_ppm             BIGINT UNSIGNED NOT NULL,
    available_from_game_day         INT UNSIGNED    NOT NULL,
    available_to_game_day           INT UNSIGNED    NOT NULL,
    created_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_property_listing_canonical
        (market_world_id, real_estate_model_version_id, `year_month`, region_key, slot_no),
    KEY ix_property_listing_profile
        (real_estate_model_version_id, region_key),
    KEY ix_property_listing_allowed_type
        (real_estate_model_version_id, region_key, property_type),
    KEY ix_property_listing_current_region
        (market_world_id, real_estate_model_version_id, region_key,
         available_from_game_day, available_to_game_day, slot_no),
    CONSTRAINT fk_property_listing_world
        FOREIGN KEY (market_world_id) REFERENCES market_world (id),
    CONSTRAINT fk_property_listing_month_catalog
        FOREIGN KEY (
            market_world_id, real_estate_model_version_id, `year_month`, region_key
        ) REFERENCES property_listing_month_catalog
            (market_world_id, real_estate_model_version_id, `year_month`, region_key),
    CONSTRAINT fk_property_listing_profile
        FOREIGN KEY (real_estate_model_version_id, region_key)
        REFERENCES real_estate_region_profile (real_estate_model_version_id, region_key),
    CONSTRAINT fk_property_listing_allowed_type
        FOREIGN KEY (real_estate_model_version_id, region_key, property_type)
        REFERENCES real_estate_region_property_type
            (real_estate_model_version_id, region_key, property_type),
    CONSTRAINT ck_property_listing_public_id CHECK (
        id BETWEEN 1 AND 9223372036854775807
    ),
    CONSTRAINT ck_property_listing_year_month CHECK (DAYOFMONTH(`year_month`) = 1),
    CONSTRAINT ck_property_listing_slot CHECK (slot_no BETWEEN 1 AND 24),
    CONSTRAINT ck_property_listing_property_type CHECK (
        property_type IN ('apartment', 'multiFamily', 'detached')
    ),
    CONSTRAINT ck_property_listing_area CHECK (
        exclusive_area_square_meters BETWEEN 1 AND 10000
    ),
    CONSTRAINT ck_property_listing_variation CHECK (
        price_variation_ppm BETWEEN 1 AND 10000000
    ),
    CONSTRAINT ck_property_listing_window CHECK (
        available_from_game_day <= available_to_game_day
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE property_listing_offer (
    property_listing_id BIGINT UNSIGNED NOT NULL,
    offer_order         TINYINT UNSIGNED NOT NULL,
    offer_kind          VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    price_krw           BIGINT NULL,
    deposit_krw         BIGINT NULL,
    monthly_rent_krw    BIGINT NULL,
    created_at          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (property_listing_id, offer_order),
    UNIQUE KEY uk_property_listing_offer_kind (property_listing_id, offer_kind),
    CONSTRAINT fk_property_listing_offer_listing
        FOREIGN KEY (property_listing_id) REFERENCES property_listing (id),
    CONSTRAINT ck_property_listing_offer_order CHECK (
        (offer_kind = 'sale' AND offer_order = 1)
        OR (offer_kind = 'jeonse' AND offer_order = 2)
        OR (offer_kind = 'monthlyRent' AND offer_order = 3)
    ),
    CONSTRAINT ck_property_listing_offer_shape CHECK (
        (
            offer_kind = 'sale'
            AND price_krw BETWEEN 1 AND 9007199254740991
            AND deposit_krw IS NULL
            AND monthly_rent_krw IS NULL
        )
        OR (
            offer_kind = 'jeonse'
            AND price_krw IS NULL
            AND deposit_krw BETWEEN 1 AND 9007199254740991
            AND monthly_rent_krw IS NULL
        )
        OR (
            offer_kind = 'monthlyRent'
            AND price_krw IS NULL
            AND deposit_krw BETWEEN 1 AND 9007199254740991
            AND monthly_rent_krw BETWEEN 1 AND 9007199254740991
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_real_estate_region_profile_draft_insert
BEFORE INSERT ON real_estate_region_profile
FOR EACH ROW
SET NEW.real_estate_model_version_id = IF(
    EXISTS (
        SELECT 1
        FROM real_estate_model_version AS model
        WHERE model.id = NEW.real_estate_model_version_id
          AND model.availability = 'active'
          AND model.sealed_at IS NULL
          AND model.canonical_sha256 IS NULL
    ),
    NEW.real_estate_model_version_id,
    NULL
);

CREATE TRIGGER tr_real_estate_region_profile_no_update
BEFORE UPDATE ON real_estate_region_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'real-estate region profiles are immutable';

CREATE TRIGGER tr_real_estate_region_profile_no_delete
BEFORE DELETE ON real_estate_region_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'real-estate region profiles are immutable';

CREATE TRIGGER tr_real_estate_region_property_type_draft_insert
BEFORE INSERT ON real_estate_region_property_type
FOR EACH ROW
SET NEW.real_estate_model_version_id = IF(
    EXISTS (
        SELECT 1
        FROM real_estate_region_profile AS profile
        INNER JOIN real_estate_model_version AS model
            ON model.id = profile.real_estate_model_version_id
        WHERE profile.real_estate_model_version_id = NEW.real_estate_model_version_id
          AND BINARY profile.region_key = BINARY NEW.region_key
          AND model.availability = 'active'
          AND model.sealed_at IS NULL
          AND model.canonical_sha256 IS NULL
    ),
    NEW.real_estate_model_version_id,
    NULL
);

CREATE TRIGGER tr_real_estate_region_property_type_no_update
BEFORE UPDATE ON real_estate_region_property_type
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'real-estate property types are immutable';

CREATE TRIGGER tr_real_estate_region_property_type_no_delete
BEFORE DELETE ON real_estate_region_property_type
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'real-estate property types are immutable';

CREATE TRIGGER tr_real_estate_model_manifest_draft_insert
BEFORE INSERT ON real_estate_model_strict_manifest
FOR EACH ROW
SET NEW.real_estate_model_version_id = IF(
    JSON_VALID(NEW.canonical_json)
        AND EXISTS (
            SELECT 1
            FROM real_estate_model_version AS model
            INNER JOIN real_estate_model_strict_projection AS projection
                ON projection.real_estate_model_version_id = model.id
            WHERE model.id = NEW.real_estate_model_version_id
              AND model.availability = 'active'
              AND model.sealed_at IS NULL
              AND BINARY projection.canonical_json = BINARY NEW.canonical_json
        ),
    NEW.real_estate_model_version_id,
    NULL
);

CREATE TRIGGER tr_real_estate_model_manifest_no_update
BEFORE UPDATE ON real_estate_model_strict_manifest
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'real-estate model manifests are immutable';

CREATE TRIGGER tr_real_estate_model_manifest_no_delete
BEFORE DELETE ON real_estate_model_strict_manifest
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'real-estate model manifests are immutable';

CREATE TRIGGER tr_real_estate_region_cursor_valid_insert
BEFORE INSERT ON real_estate_region_series_cursor
FOR EACH ROW
SET NEW.market_world_id = IF(
    NEW.next_game_day = 0
        AND EXISTS (
            SELECT 1
            FROM real_estate_model_version AS model
            INNER JOIN real_estate_region_profile AS profile
                ON profile.real_estate_model_version_id = model.id
               AND BINARY profile.region_key = BINARY NEW.region_key
            WHERE model.id = NEW.real_estate_model_version_id
              AND model.availability = 'active'
              AND model.sealed_at IS NOT NULL
        ),
    NEW.market_world_id,
    NULL
);

CREATE TRIGGER tr_real_estate_region_cursor_advance_only
BEFORE UPDATE ON real_estate_region_series_cursor
FOR EACH ROW
SET NEW.market_world_id = IF(
    NEW.market_world_id = OLD.market_world_id
        AND NEW.real_estate_model_version_id = OLD.real_estate_model_version_id
        AND BINARY NEW.region_key = BINARY OLD.region_key
        AND OLD.next_game_day < 4294967295
        AND NEW.next_game_day = OLD.next_game_day + 1
        AND NEW.created_at = OLD.created_at
        AND EXISTS (
            SELECT 1
            FROM real_estate_daily AS daily
            WHERE daily.market_world_id = OLD.market_world_id
              AND daily.real_estate_model_version_id = OLD.real_estate_model_version_id
              AND BINARY daily.region_key = BINARY OLD.region_key
              AND daily.game_day = OLD.next_game_day
        ),
    OLD.market_world_id,
    NULL
);

CREATE TRIGGER tr_real_estate_region_cursor_no_delete
BEFORE DELETE ON real_estate_region_series_cursor
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'real-estate region cursors are durable state';

CREATE TRIGGER tr_real_estate_daily_ordered_insert
BEFORE INSERT ON real_estate_daily
FOR EACH ROW
SET NEW.market_world_id = IF(
    EXISTS (
        SELECT 1
        FROM real_estate_region_series_cursor AS series_cursor
        INNER JOIN real_estate_model_version AS model
            ON model.id = series_cursor.real_estate_model_version_id
        INNER JOIN real_estate_region_profile AS profile
            ON profile.real_estate_model_version_id = series_cursor.real_estate_model_version_id
           AND BINARY profile.region_key = BINARY series_cursor.region_key
        WHERE series_cursor.market_world_id = NEW.market_world_id
          AND series_cursor.real_estate_model_version_id = NEW.real_estate_model_version_id
          AND BINARY series_cursor.region_key = BINARY NEW.region_key
          AND series_cursor.next_game_day = NEW.game_day
          AND model.availability = 'active'
          AND model.sealed_at IS NOT NULL
          AND NEW.price_index_ppm BETWEEN profile.minimum_index_ppm
              AND profile.maximum_index_ppm
          AND NEW.rent_index_ppm BETWEEN profile.minimum_index_ppm
              AND profile.maximum_index_ppm
          AND (
              NEW.price_index_ppm NOT IN
                  (profile.minimum_index_ppm, profile.maximum_index_ppm)
              OR NEW.price_remainder_numerator = 0
          )
          AND (
              NEW.rent_index_ppm NOT IN
                  (profile.minimum_index_ppm, profile.maximum_index_ppm)
              OR NEW.rent_remainder_numerator = 0
          )
    )
        AND (
            NEW.game_day = 0
            OR EXISTS (
                SELECT 1
                FROM real_estate_daily AS previous
                WHERE previous.market_world_id = NEW.market_world_id
                  AND previous.real_estate_model_version_id
                      = NEW.real_estate_model_version_id
                  AND BINARY previous.region_key = BINARY NEW.region_key
                  AND previous.game_day = NEW.game_day - 1
            )
        ),
    NEW.market_world_id,
    NULL
);

CREATE TRIGGER tr_real_estate_daily_no_update
BEFORE UPDATE ON real_estate_daily
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'real-estate daily rows are immutable';

CREATE TRIGGER tr_real_estate_daily_no_delete
BEFORE DELETE ON real_estate_daily
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'real-estate daily rows are immutable';

CREATE TRIGGER tr_property_listing_month_catalog_valid_insert
BEFORE INSERT ON property_listing_month_catalog
FOR EACH ROW
SET NEW.market_world_id = IF(
    NEW.completed_at IS NULL
        AND EXISTS (
            SELECT 1
            FROM real_estate_model_version AS model
            INNER JOIN real_estate_region_profile AS profile
                ON profile.real_estate_model_version_id = model.id
               AND BINARY profile.region_key = BINARY NEW.region_key
            INNER JOIN market_world AS world ON world.id = NEW.market_world_id
            WHERE model.id = NEW.real_estate_model_version_id
              AND model.availability = 'active'
              AND model.sealed_at IS NOT NULL
              AND profile.monthly_listing_slot_count = NEW.expected_listing_count
              AND NEW.`year_month` >= DATE_FORMAT(world.start_date, '%Y-%m-01')
        ),
    NEW.market_world_id,
    NULL
);

CREATE TRIGGER tr_property_listing_month_catalog_complete_only
BEFORE UPDATE ON property_listing_month_catalog
FOR EACH ROW
SET NEW.market_world_id = IF(
    NEW.market_world_id = OLD.market_world_id
        AND NEW.real_estate_model_version_id = OLD.real_estate_model_version_id
        AND NEW.`year_month` = OLD.`year_month`
        AND BINARY NEW.region_key = BINARY OLD.region_key
        AND NEW.expected_listing_count = OLD.expected_listing_count
        AND OLD.completed_at IS NULL
        AND NEW.completed_at IS NOT NULL
        AND NEW.created_at = OLD.created_at
        AND (
            SELECT COUNT(*)
            FROM property_listing AS listing
            WHERE listing.market_world_id = OLD.market_world_id
              AND listing.real_estate_model_version_id
                  = OLD.real_estate_model_version_id
              AND listing.`year_month` = OLD.`year_month`
              AND BINARY listing.region_key = BINARY OLD.region_key
        ) = OLD.expected_listing_count
        AND NOT EXISTS (
            SELECT 1
            FROM property_listing AS listing
            WHERE listing.market_world_id = OLD.market_world_id
              AND listing.real_estate_model_version_id
                  = OLD.real_estate_model_version_id
              AND listing.`year_month` = OLD.`year_month`
              AND BINARY listing.region_key = BINARY OLD.region_key
              AND (
                  SELECT COUNT(*)
                  FROM property_listing_offer AS offer
                  WHERE offer.property_listing_id = listing.id
              ) <> 1
        ),
    OLD.market_world_id,
    NULL
);

CREATE TRIGGER tr_property_listing_month_catalog_no_delete
BEFORE DELETE ON property_listing_month_catalog
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property listing month catalogs are durable state';

CREATE TRIGGER tr_property_listing_valid_insert
BEFORE INSERT ON property_listing
FOR EACH ROW
SET NEW.id = IF(
    NEW.id BETWEEN 1 AND 9223372036854775807
        AND NOT EXISTS (
            SELECT 1
            FROM property_listing AS existing
            WHERE existing.id = NEW.id
              AND NOT (
                  existing.market_world_id = NEW.market_world_id
                  AND existing.real_estate_model_version_id
                      = NEW.real_estate_model_version_id
                  AND existing.`year_month` = NEW.`year_month`
                  AND BINARY existing.region_key = BINARY NEW.region_key
                  AND existing.slot_no = NEW.slot_no
              )
        )
        AND EXISTS (
            SELECT 1
            FROM real_estate_model_version AS model
            INNER JOIN real_estate_region_profile AS profile
                ON profile.real_estate_model_version_id = model.id
               AND BINARY profile.region_key = BINARY NEW.region_key
            INNER JOIN real_estate_region_property_type AS allowed
                ON allowed.real_estate_model_version_id = profile.real_estate_model_version_id
               AND BINARY allowed.region_key = BINARY profile.region_key
               AND BINARY allowed.property_type = BINARY NEW.property_type
            INNER JOIN market_world AS world ON world.id = NEW.market_world_id
            INNER JOIN real_estate_daily AS daily
                ON daily.market_world_id = NEW.market_world_id
               AND daily.real_estate_model_version_id = model.id
               AND BINARY daily.region_key = BINARY profile.region_key
               AND daily.game_day = NEW.available_from_game_day
            WHERE model.id = NEW.real_estate_model_version_id
              AND model.availability = 'active'
              AND model.sealed_at IS NOT NULL
              AND NEW.slot_no BETWEEN 1 AND profile.monthly_listing_slot_count
              AND NEW.exclusive_area_square_meters
                  BETWEEN profile.minimum_exclusive_area_square_meters
                      AND profile.maximum_exclusive_area_square_meters
              AND NEW.price_variation_ppm
                  BETWEEN profile.minimum_price_variation_ppm
                      AND profile.maximum_price_variation_ppm
              AND DATE_ADD(
                      world.start_date,
                      INTERVAL NEW.available_from_game_day DAY
                  ) = NEW.`year_month`
              AND DATE_ADD(
                      world.start_date,
                      INTERVAL NEW.available_to_game_day DAY
                  ) = LAST_DAY(NEW.`year_month`)
        ),
    NEW.id,
    NULL
);

CREATE TRIGGER tr_property_listing_no_update
BEFORE UPDATE ON property_listing
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property listings are immutable';

CREATE TRIGGER tr_property_listing_no_delete
BEFORE DELETE ON property_listing
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property listings are immutable';

CREATE TRIGGER tr_property_listing_offer_valid_insert
BEFORE INSERT ON property_listing_offer
FOR EACH ROW
SET NEW.property_listing_id = IF(
    EXISTS (
        SELECT 1
        FROM property_listing AS listing
        INNER JOIN real_estate_region_profile AS profile
            ON profile.real_estate_model_version_id = listing.real_estate_model_version_id
           AND BINARY profile.region_key = BINARY listing.region_key
        WHERE listing.id = NEW.property_listing_id
          AND profile.offer_rotation_rule = 'saleJeonseMonthlyRent'
          AND (
              (MOD(listing.slot_no - 1, 3) = 0 AND NEW.offer_kind = 'sale')
              OR (MOD(listing.slot_no - 1, 3) = 1 AND NEW.offer_kind = 'jeonse')
              OR (MOD(listing.slot_no - 1, 3) = 2 AND NEW.offer_kind = 'monthlyRent')
          )
    ),
    NEW.property_listing_id,
    NULL
);

CREATE TRIGGER tr_property_listing_offer_no_update
BEFORE UPDATE ON property_listing_offer
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property listing offers are immutable';

CREATE TRIGGER tr_property_listing_offer_no_delete
BEFORE DELETE ON property_listing_offer
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property listing offers are immutable';

DROP TRIGGER tr_real_estate_model_version_draft_insert;
DROP TRIGGER tr_real_estate_model_version_seal_only;

CREATE TRIGGER tr_real_estate_model_version_draft_insert
BEFORE INSERT ON real_estate_model_version
FOR EACH ROW
SET NEW.version_key = IF(
    NEW.sealed_at IS NULL
        AND NEW.canonical_sha256 IS NULL
        AND JSON_TYPE(NEW.parameters) = 'OBJECT'
        AND (NEW.availability <> 'disabled' OR NEW.ranked_eligible = FALSE),
    NEW.version_key,
    NULL
);

CREATE TRIGGER tr_real_estate_model_version_seal_only
BEFORE UPDATE ON real_estate_model_version
FOR EACH ROW
SET NEW.version_key = IF(
    OLD.sealed_at IS NULL
        AND NEW.sealed_at IS NOT NULL
        AND NEW.id = OLD.id
        AND BINARY NEW.version_key = BINARY OLD.version_key
        AND BINARY NEW.availability = BINARY OLD.availability
        AND NEW.ranked_eligible = OLD.ranked_eligible
        AND NEW.parameters = OLD.parameters
        AND NEW.created_at = OLD.created_at
        AND NEW.canonical_sha256 REGEXP '^[0-9a-f]{64}$'
        AND (
            (
                OLD.availability = 'active'
                AND (
                    SELECT COUNT(*)
                    FROM real_estate_region_profile AS profile
                    WHERE profile.real_estate_model_version_id = OLD.id
                ) = (SELECT COUNT(*) FROM life_region)
                AND NOT EXISTS (
                    SELECT 1
                    FROM life_region AS region
                    WHERE NOT EXISTS (
                        SELECT 1
                        FROM real_estate_region_profile AS profile
                        WHERE profile.real_estate_model_version_id = OLD.id
                          AND BINARY profile.region_key = BINARY region.region_key
                    )
                )
                AND NOT EXISTS (
                    SELECT 1
                    FROM real_estate_region_profile AS profile
                    WHERE profile.real_estate_model_version_id = OLD.id
                      AND (
                          SELECT COUNT(*)
                          FROM real_estate_region_property_type AS allowed
                          WHERE allowed.real_estate_model_version_id
                                  = profile.real_estate_model_version_id
                            AND BINARY allowed.region_key = BINARY profile.region_key
                      ) NOT BETWEEN 1 AND 3
                )
                AND EXISTS (
                    SELECT 1
                    FROM real_estate_model_strict_manifest AS manifest
                    INNER JOIN real_estate_model_strict_projection AS projection
                        ON projection.real_estate_model_version_id
                            = manifest.real_estate_model_version_id
                    WHERE manifest.real_estate_model_version_id = OLD.id
                      AND BINARY manifest.canonical_sha256 = BINARY NEW.canonical_sha256
                      AND BINARY manifest.canonical_json = BINARY projection.canonical_json
                )
            )
            OR (
                OLD.availability = 'disabled'
                AND OLD.ranked_eligible = FALSE
                AND NEW.canonical_sha256 = SHA2(
                    CAST(JSON_OBJECT(
                        'availability', OLD.availability,
                        'parameters', OLD.parameters,
                        'schemaVersion', 1,
                        'versionKey', OLD.version_key
                    ) AS CHAR CHARACTER SET utf8mb4),
                    256
                )
            )
        ),
    OLD.version_key,
    NULL
);

INSERT INTO real_estate_model_version
    (version_key, availability, ranked_eligible, parameters)
VALUES
    (
        'dev-unranked-m4-real-estate-2026-v1',
        'active',
        FALSE,
        JSON_OBJECT(
            'entropyVersion', 'sha256-counter-be-v1',
            'generatorVersion', 'm4-c1-v1',
            'schemaVersion', 1
        )
    );

INSERT INTO real_estate_region_profile
    (
        real_estate_model_version_id, region_key, monthly_listing_slot_count,
        minimum_exclusive_area_square_meters, maximum_exclusive_area_square_meters,
        base_price_per_square_meter_krw, price_daily_drift_ppm,
        price_daily_shock_amplitude_ppm, rent_daily_drift_ppm,
        rent_daily_shock_amplitude_ppm, minimum_index_ppm, maximum_index_ppm,
        minimum_price_variation_ppm, maximum_price_variation_ppm, jeonse_ratio_ppm,
        annual_gross_rent_yield_ppm, monthly_deposit_ratio_ppm,
        availability_rule, offer_rotation_rule
    )
SELECT
    model.id, fixture.region_key, 12, fixture.minimum_area, fixture.maximum_area,
    fixture.base_price_per_square_meter_krw, fixture.price_daily_drift_ppm,
    fixture.price_daily_shock_amplitude_ppm, fixture.rent_daily_drift_ppm,
    fixture.rent_daily_shock_amplitude_ppm, 500000, 2000000, 850000, 1150000,
    fixture.jeonse_ratio_ppm, fixture.annual_gross_rent_yield_ppm, 100000,
    'marketMonthInclusive', 'saleJeonseMonthlyRent'
FROM real_estate_model_version AS model
INNER JOIN (
    SELECT
        'capitalArea' AS region_key, 30 AS minimum_area, 120 AS maximum_area,
        10000000 AS base_price_per_square_meter_krw,
        80 AS price_daily_drift_ppm, 1200 AS price_daily_shock_amplitude_ppm,
        50 AS rent_daily_drift_ppm, 500 AS rent_daily_shock_amplitude_ppm,
        550000 AS jeonse_ratio_ppm, 35000 AS annual_gross_rent_yield_ppm
    UNION ALL
    SELECT 'metropolitan', 35, 135, 5000000, 60, 1000, 40, 400, 600000, 42000
    UNION ALL
    SELECT 'smallCity', 40, 160, 3000000, 40, 800, 30, 350, 650000, 48000
    UNION ALL
    SELECT 'rural', 50, 200, 1500000, 20, 600, 20, 300, 600000, 55000
) AS fixture
WHERE model.version_key = 'dev-unranked-m4-real-estate-2026-v1';

INSERT INTO real_estate_region_property_type
    (real_estate_model_version_id, region_key, property_type, property_type_order)
SELECT model.id, allowed.region_key, allowed.property_type, allowed.property_type_order
FROM real_estate_model_version AS model
INNER JOIN (
    SELECT 'capitalArea' AS region_key, 'apartment' AS property_type, 1 AS property_type_order
    UNION ALL SELECT 'capitalArea', 'multiFamily', 2
    UNION ALL SELECT 'metropolitan', 'apartment', 1
    UNION ALL SELECT 'metropolitan', 'multiFamily', 2
    UNION ALL SELECT 'metropolitan', 'detached', 3
    UNION ALL SELECT 'smallCity', 'apartment', 1
    UNION ALL SELECT 'smallCity', 'multiFamily', 2
    UNION ALL SELECT 'smallCity', 'detached', 3
    UNION ALL SELECT 'rural', 'multiFamily', 2
    UNION ALL SELECT 'rural', 'detached', 3
) AS allowed
WHERE model.version_key = 'dev-unranked-m4-real-estate-2026-v1';

INSERT INTO real_estate_model_strict_manifest
    (real_estate_model_version_id, canonical_json)
SELECT real_estate_model_version_id, canonical_json
FROM real_estate_model_strict_projection
WHERE real_estate_model_version_id = (
    SELECT id
    FROM real_estate_model_version
    WHERE version_key = 'dev-unranked-m4-real-estate-2026-v1'
);

UPDATE real_estate_model_version AS model
INNER JOIN real_estate_model_strict_manifest AS manifest
    ON manifest.real_estate_model_version_id = model.id
SET model.canonical_sha256 = manifest.canonical_sha256,
    model.sealed_at = CURRENT_TIMESTAMP(3)
WHERE model.version_key = 'dev-unranked-m4-real-estate-2026-v1'
  AND model.sealed_at IS NULL;

UPDATE run_rule_bundle_assignment AS assignment
INNER JOIN real_estate_model_version AS active_real_estate
    ON active_real_estate.version_key = 'dev-unranked-m4-real-estate-2026-v1'
   AND active_real_estate.availability = 'active'
   AND active_real_estate.sealed_at IS NOT NULL
SET assignment.real_estate_model_version_id = active_real_estate.id
WHERE assignment.assignment_key = 'newRun';
