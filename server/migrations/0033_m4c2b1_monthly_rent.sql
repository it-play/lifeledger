-- M4-C2b1 immutable monthly-rent terms, phase-300 rent settlement, and typed arrears (§5.6).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;

-- MySQL DDL auto-commits. Reject legacy rows that cannot satisfy the tagged lease shape before
-- changing any durable object, so a failed forward migration cannot stop halfway through.
CREATE TEMPORARY TABLE m4c2b1_preflight_guard (
    guard_key VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    accepted TINYINT UNSIGNED NOT NULL,
    PRIMARY KEY (guard_key),
    CONSTRAINT ck_m4c2b1_preflight_guard CHECK (accepted = 1)
);

INSERT INTO m4c2b1_preflight_guard (guard_key, accepted)
SELECT 'residence-shape', IF(
    NOT EXISTS (
        SELECT 1
        FROM residence
        WHERE tenure_type = 'monthlyRent'
           OR (tenure_type = 'jeonse' AND lease_contract_id IS NULL)
           OR (tenure_type <> 'jeonse' AND lease_contract_id IS NOT NULL)
    ),
    1,
    0
);

DROP TEMPORARY TABLE m4c2b1_preflight_guard;

ALTER TABLE real_estate_lease_profile
    DROP CHECK ck_real_estate_lease_profile_kind,
    DROP CHECK ck_real_estate_lease_profile_renewal,
    ADD COLUMN rent_charge_rule VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NULL
        AFTER renewal_rule,
    ADD COLUMN arrear_repayment_rule VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NULL
        AFTER rent_charge_rule,
    ADD CONSTRAINT ck_real_estate_lease_profile_kind CHECK (
        offer_kind IN ('jeonse', 'monthlyRent')
    ),
    ADD CONSTRAINT ck_real_estate_lease_profile_renewal CHECK (
        renewal_rule = 'openEnded'
    ),
    ADD CONSTRAINT ck_real_estate_lease_profile_terms CHECK (
        (
            offer_kind = 'jeonse'
            AND rent_charge_rule IS NULL
            AND arrear_repayment_rule IS NULL
        )
        OR (
            offer_kind = 'monthlyRent'
            AND rent_charge_rule = 'nextMonthStartFull'
            AND arrear_repayment_rule = 'manualOnly'
        )
    );

-- The base JSON expression is unchanged from 0032. Only models with the v3 monthly-rent
-- profile receive the schema-3 merge, preserving v1 and v2 manifests byte-for-byte.
CREATE OR REPLACE VIEW real_estate_model_strict_projection AS
SELECT
    base.real_estate_model_version_id,
    CAST(
        CASE
            WHEN EXISTS (
                SELECT 1
                FROM real_estate_lease_profile AS monthly_profile
                WHERE monthly_profile.real_estate_model_version_id
                        = base.real_estate_model_version_id
                  AND monthly_profile.offer_kind = 'monthlyRent'
            )
            THEN JSON_MERGE_PATCH(
                base.base_json,
                JSON_OBJECT(
                    'leaseProfiles', COALESCE((
                        SELECT JSON_ARRAYAGG(JSON_OBJECT(
                                   'arrearRepaymentRule',
                                       lease_profile.arrear_repayment_rule,
                                   'offerKind', lease_profile.offer_kind,
                                   'renewalRule', lease_profile.renewal_rule,
                                   'rentChargeRule', lease_profile.rent_charge_rule
                               )) OVER (
                                   ORDER BY lease_profile.offer_kind
                                   ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING
                               )
                        FROM real_estate_lease_profile AS lease_profile
                        WHERE lease_profile.real_estate_model_version_id
                                = base.real_estate_model_version_id
                        ORDER BY lease_profile.offer_kind
                        LIMIT 1
                    ), JSON_ARRAY()),
                    'movingCosts', COALESCE((
                        SELECT JSON_ARRAYAGG(JSON_OBJECT(
                                   'movingCostKrw', moving_cost.moving_cost_krw,
                                   'regionKey', moving_cost.region_key
                               )) OVER (
                                   ORDER BY region.region_order
                                   ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING
                               )
                        FROM real_estate_region_moving_cost AS moving_cost
                        INNER JOIN life_region AS region
                            ON BINARY region.region_key = BINARY moving_cost.region_key
                        WHERE moving_cost.real_estate_model_version_id
                                = base.real_estate_model_version_id
                        ORDER BY region.region_order
                        LIMIT 1
                    ), JSON_ARRAY()),
                    'schemaVersion', 3
                )
            )
            WHEN EXISTS (
                SELECT 1
                FROM real_estate_lease_profile AS lease_profile
                WHERE lease_profile.real_estate_model_version_id
                        = base.real_estate_model_version_id
            )
            THEN JSON_MERGE_PATCH(
                base.base_json,
                JSON_OBJECT(
                    'leaseProfiles', COALESCE((
                        SELECT JSON_ARRAYAGG(JSON_OBJECT(
                                   'offerKind', lease_profile.offer_kind,
                                   'renewalRule', lease_profile.renewal_rule
                               )) OVER (
                                   ORDER BY lease_profile.offer_kind
                                   ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING
                               )
                        FROM real_estate_lease_profile AS lease_profile
                        WHERE lease_profile.real_estate_model_version_id
                                = base.real_estate_model_version_id
                        ORDER BY lease_profile.offer_kind
                        LIMIT 1
                    ), JSON_ARRAY()),
                    'movingCosts', COALESCE((
                        SELECT JSON_ARRAYAGG(JSON_OBJECT(
                                   'movingCostKrw', moving_cost.moving_cost_krw,
                                   'regionKey', moving_cost.region_key
                               )) OVER (
                                   ORDER BY region.region_order
                                   ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING
                               )
                        FROM real_estate_region_moving_cost AS moving_cost
                        INNER JOIN life_region AS region
                            ON BINARY region.region_key = BINARY moving_cost.region_key
                        WHERE moving_cost.real_estate_model_version_id
                                = base.real_estate_model_version_id
                        ORDER BY region.region_order
                        LIMIT 1
                    ), JSON_ARRAY()),
                    'schemaVersion', 2
                )
            )
            ELSE base.base_json
        END AS CHAR CHARACTER SET utf8mb4
    ) AS canonical_json
FROM (
    SELECT
        model.id AS real_estate_model_version_id,
        JSON_OBJECT(
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
                           'basePricePerSquareMeterKrw',
                               profile.base_price_per_square_meter_krw,
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
                           'rentDailyShockAmplitudePpm',
                               profile.rent_daily_shock_amplitude_ppm
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
        ) AS base_json
    FROM real_estate_model_version AS model
    WHERE model.availability = 'active'
) AS base;

CREATE TEMPORARY TABLE m4c2b1_manifest_guard (
    guard_key VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    accepted TINYINT UNSIGNED NOT NULL,
    PRIMARY KEY (guard_key),
    CONSTRAINT ck_m4c2b1_manifest_guard CHECK (accepted = 1)
);

INSERT INTO m4c2b1_manifest_guard (guard_key, accepted)
SELECT 'existing-manifests', IF(
    NOT EXISTS (
        SELECT 1
        FROM real_estate_model_strict_manifest AS manifest
        INNER JOIN real_estate_model_version AS model
            ON model.id = manifest.real_estate_model_version_id
        LEFT JOIN real_estate_model_strict_projection AS projection
            ON projection.real_estate_model_version_id = manifest.real_estate_model_version_id
        WHERE model.availability = 'active'
          AND (
              projection.real_estate_model_version_id IS NULL
              OR BINARY projection.canonical_json <> BINARY manifest.canonical_json
          )
    ),
    1,
    0
);

DROP TEMPORARY TABLE m4c2b1_manifest_guard;

-- Recompilation is required after replacing the projection in this same migration connection.
DROP TRIGGER tr_real_estate_model_manifest_draft_insert;

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

ALTER TABLE lease_contract
    DROP CHECK ck_lease_contract_offer,
    ADD COLUMN rent_charge_rule VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NULL
        AFTER renewal_rule,
    ADD COLUMN arrear_repayment_rule VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NULL
        AFTER rent_charge_rule,
    ADD CONSTRAINT ck_lease_contract_offer CHECK (
        deposit_krw BETWEEN 1 AND 9007199254740991
        AND (
            (
                offer_kind = 'jeonse'
                AND monthly_rent_krw IS NULL
                AND rent_charge_rule IS NULL
                AND arrear_repayment_rule IS NULL
            )
            OR (
                offer_kind = 'monthlyRent'
                AND monthly_rent_krw BETWEEN 1 AND 9007199254740991
                AND rent_charge_rule = 'nextMonthStartFull'
                AND arrear_repayment_rule = 'manualOnly'
            )
        )
    );

DROP TRIGGER tr_lease_contract_valid_insert;

CREATE TRIGGER tr_lease_contract_valid_insert
BEFORE INSERT ON lease_contract
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.effective_to_game_day IS NULL
        AND EXISTS (
            SELECT 1
            FROM save
            INNER JOIN command_identity AS identity
                ON identity.save_id = save.id
               AND BINARY identity.command_id = BINARY NEW.command_id
            INNER JOIN household
                ON household.save_id = save.id
               AND household.run_revision = save.run_revision
               AND household.id = NEW.household_id
            INNER JOIN run_rule_bundle AS bundle
                ON bundle.save_id = save.id
               AND bundle.run_revision = save.run_revision
            INNER JOIN real_estate_model_version AS model
                ON model.id = bundle.real_estate_model_version_id
            INNER JOIN real_estate_model_strict_manifest AS manifest
                ON manifest.real_estate_model_version_id = model.id
            INNER JOIN real_estate_model_strict_projection AS projection
                ON projection.real_estate_model_version_id = model.id
            INNER JOIN real_estate_lease_profile AS lease_profile
                ON lease_profile.real_estate_model_version_id = model.id
               AND BINARY lease_profile.offer_kind = BINARY NEW.offer_kind
               AND BINARY lease_profile.renewal_rule = BINARY NEW.renewal_rule
               AND lease_profile.rent_charge_rule <=> NEW.rent_charge_rule
               AND lease_profile.arrear_repayment_rule <=> NEW.arrear_repayment_rule
            INNER JOIN real_estate_region_moving_cost AS moving_cost
                ON moving_cost.real_estate_model_version_id = model.id
               AND BINARY moving_cost.region_key = BINARY NEW.region_key
            INNER JOIN property_listing AS listing
                ON listing.id = NEW.property_listing_id
               AND listing.market_world_id = bundle.market_world_id
               AND listing.real_estate_model_version_id = model.id
            INNER JOIN property_listing_offer AS offer
                ON offer.property_listing_id = listing.id
               AND BINARY offer.offer_kind = BINARY NEW.offer_kind
            WHERE save.id = NEW.save_id
              AND save.run_revision = NEW.run_revision
              AND save.game_day = NEW.effective_from_game_day
              AND identity.command_kind = 'startLease'
              AND identity.initial_run_revision = NEW.run_revision
              AND identity.initial_state_revision = save.state_revision
              AND identity.initial_game_day = save.game_day
              AND model.id = NEW.real_estate_model_version_id
              AND model.availability = 'active'
              AND model.sealed_at IS NOT NULL
              AND BINARY model.canonical_sha256 = BINARY manifest.canonical_sha256
              AND BINARY manifest.canonical_json = BINARY projection.canonical_json
              AND listing.available_from_game_day <= save.game_day
              AND listing.available_to_game_day >= save.game_day
              AND BINARY listing.region_key = BINARY NEW.region_key
              AND BINARY listing.property_type = BINARY NEW.property_type
              AND listing.exclusive_area_square_meters
                    = NEW.exclusive_area_square_meters
              AND offer.price_krw IS NULL
              AND offer.deposit_krw = NEW.deposit_krw
              AND offer.monthly_rent_krw <=> NEW.monthly_rent_krw
        ),
    NEW.save_id,
    NULL
);

DROP TRIGGER tr_lease_contract_close_only;

CREATE TRIGGER tr_lease_contract_close_only
BEFORE UPDATE ON lease_contract
FOR EACH ROW
SET NEW.id = IF(
    OLD.effective_to_game_day IS NULL
        AND NEW.effective_to_game_day IS NOT NULL
        AND NEW.effective_to_game_day > OLD.effective_from_game_day
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.household_id = OLD.household_id
        AND NEW.real_estate_model_version_id = OLD.real_estate_model_version_id
        AND NEW.property_listing_id = OLD.property_listing_id
        AND BINARY NEW.command_id = BINARY OLD.command_id
        AND BINARY NEW.role = BINARY OLD.role
        AND BINARY NEW.region_key = BINARY OLD.region_key
        AND BINARY NEW.property_type = BINARY OLD.property_type
        AND NEW.exclusive_area_square_meters = OLD.exclusive_area_square_meters
        AND BINARY NEW.offer_kind = BINARY OLD.offer_kind
        AND NEW.deposit_krw = OLD.deposit_krw
        AND NEW.monthly_rent_krw <=> OLD.monthly_rent_krw
        AND BINARY NEW.renewal_rule = BINARY OLD.renewal_rule
        AND NEW.rent_charge_rule <=> OLD.rent_charge_rule
        AND NEW.arrear_repayment_rule <=> OLD.arrear_repayment_rule
        AND NEW.effective_from_game_day = OLD.effective_from_game_day
        AND NEW.created_at = OLD.created_at
        AND EXISTS (
            SELECT 1
            FROM save
            WHERE save.id = OLD.save_id
              AND save.run_revision = OLD.run_revision
              AND save.game_day = NEW.effective_to_game_day
        ),
    OLD.id,
    NULL
);

ALTER TABLE residence
    DROP CHECK ck_residence_lease_shape,
    ADD CONSTRAINT ck_residence_lease_shape CHECK (
        (tenure_type IN ('jeonse', 'monthlyRent') AND lease_contract_id IS NOT NULL)
        OR (tenure_type NOT IN ('jeonse', 'monthlyRent') AND lease_contract_id IS NULL)
    );

DROP TRIGGER tr_residence_lease_valid_insert;

CREATE TRIGGER tr_residence_lease_valid_insert
BEFORE INSERT ON residence
FOR EACH ROW
SET NEW.save_id = IF(
    (
        NEW.lease_contract_id IS NULL
        AND NEW.tenure_type NOT IN ('jeonse', 'monthlyRent')
    )
    OR (
        NEW.lease_contract_id IS NOT NULL
        AND NEW.tenure_type IN ('jeonse', 'monthlyRent')
        AND EXISTS (
            SELECT 1
            FROM lease_contract AS contract
            WHERE contract.id = NEW.lease_contract_id
              AND contract.save_id = NEW.save_id
              AND contract.run_revision = NEW.run_revision
              AND contract.household_id = NEW.household_id
              AND contract.role = 'tenant'
              AND BINARY contract.offer_kind = BINARY NEW.tenure_type
              AND BINARY contract.region_key = BINARY NEW.region_key
              AND contract.effective_from_game_day = NEW.effective_from_game_day
              AND contract.effective_to_game_day IS NULL
        )
    ),
    NEW.save_id,
    NULL
);

CREATE TABLE lease_rent_charge (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED NOT NULL,
    lease_contract_id           BIGINT UNSIGNED NOT NULL,
    charge_no                   INT UNSIGNED NOT NULL,
    due_year_month              DATE NOT NULL,
    due_game_day                INT UNSIGNED NOT NULL,
    amount_krw                  BIGINT NOT NULL,
    status                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    paid_krw                    BIGINT NULL,
    arrear_krw                  BIGINT NULL,
    ledger_transaction_id       BIGINT UNSIGNED NULL,
    pending_slot                TINYINT UNSIGNED GENERATED ALWAYS AS (
        CASE WHEN status = 'pending' THEN 1 ELSE NULL END
    ) STORED,
    created_at                  DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                  DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_lease_rent_charge_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_lease_rent_charge_number (lease_contract_id, charge_no),
    UNIQUE KEY uk_lease_rent_charge_due_day (lease_contract_id, due_game_day),
    UNIQUE KEY uk_lease_rent_charge_pending (lease_contract_id, pending_slot),
    KEY ix_lease_rent_charge_due
        (save_id, run_revision, status, due_game_day, id),
    UNIQUE KEY uk_lease_rent_charge_ledger
        (save_id, run_revision, ledger_transaction_id),
    CONSTRAINT fk_lease_rent_charge_contract
        FOREIGN KEY (save_id, run_revision, lease_contract_id)
        REFERENCES lease_contract (save_id, run_revision, id),
    CONSTRAINT fk_lease_rent_charge_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT ck_lease_rent_charge_number CHECK (charge_no > 0),
    CONSTRAINT ck_lease_rent_charge_month CHECK (DAY(due_year_month) = 1),
    CONSTRAINT ck_lease_rent_charge_amount CHECK (
        amount_krw BETWEEN 1 AND 9007199254740991
    ),
    CONSTRAINT ck_lease_rent_charge_state CHECK (
        (
            status = 'pending'
            AND paid_krw IS NULL
            AND arrear_krw IS NULL
            AND ledger_transaction_id IS NULL
        )
        OR (
            status = 'settled'
            AND paid_krw >= 0
            AND arrear_krw >= 0
            AND paid_krw + arrear_krw = amount_krw
            AND ledger_transaction_id IS NOT NULL
        )
        OR (
            status = 'cancelled'
            AND paid_krw IS NULL
            AND arrear_krw IS NULL
            AND ledger_transaction_id IS NULL
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE lease_arrear (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED NOT NULL,
    household_id                BIGINT UNSIGNED NOT NULL,
    lease_contract_id           BIGINT UNSIGNED NOT NULL,
    lease_rent_charge_id        BIGINT UNSIGNED NOT NULL,
    due_year_month              DATE NOT NULL,
    original_krw                BIGINT NOT NULL,
    paid_krw                    BIGINT NOT NULL DEFAULT 0,
    remaining_krw               BIGINT GENERATED ALWAYS AS (original_krw - paid_krw) STORED,
    status                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_game_day            INT UNSIGNED NOT NULL,
    closed_game_day             INT UNSIGNED NULL,
    created_at                  DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                  DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_lease_arrear_charge (lease_rent_charge_id),
    UNIQUE KEY uk_lease_arrear_save_run_id (save_id, run_revision, id),
    KEY ix_lease_arrear_priority
        (save_id, run_revision, status, due_year_month, id),
    CONSTRAINT fk_lease_arrear_household
        FOREIGN KEY (save_id, run_revision, household_id)
        REFERENCES household (save_id, run_revision, id),
    CONSTRAINT fk_lease_arrear_contract
        FOREIGN KEY (save_id, run_revision, lease_contract_id)
        REFERENCES lease_contract (save_id, run_revision, id),
    CONSTRAINT fk_lease_arrear_charge
        FOREIGN KEY (save_id, run_revision, lease_rent_charge_id)
        REFERENCES lease_rent_charge (save_id, run_revision, id),
    CONSTRAINT ck_lease_arrear_month CHECK (DAY(due_year_month) = 1),
    CONSTRAINT ck_lease_arrear_amount CHECK (
        original_krw BETWEEN 1 AND 9007199254740991
        AND paid_krw BETWEEN 0 AND original_krw
    ),
    CONSTRAINT ck_lease_arrear_state CHECK (
        (
            status = 'active'
            AND paid_krw < original_krw
            AND closed_game_day IS NULL
        )
        OR (
            status = 'paid'
            AND paid_krw = original_krw
            AND closed_game_day IS NOT NULL
            AND closed_game_day >= created_game_day
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE lease_arrear_payment (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED NOT NULL,
    lease_arrear_id             BIGINT UNSIGNED NOT NULL,
    payment_no                  INT UNSIGNED NOT NULL,
    amount_krw                  BIGINT NOT NULL,
    game_day                    INT UNSIGNED NOT NULL,
    command_id                  CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    status                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ledger_transaction_id       BIGINT UNSIGNED NULL,
    created_at                  DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                  DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_lease_arrear_payment_number (lease_arrear_id, payment_no),
    UNIQUE KEY uk_lease_arrear_payment_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_lease_arrear_payment_command (save_id, command_id),
    UNIQUE KEY uk_lease_arrear_payment_ledger
        (save_id, run_revision, ledger_transaction_id),
    KEY ix_lease_arrear_payment_arrear
        (save_id, run_revision, lease_arrear_id),
    CONSTRAINT fk_lease_arrear_payment_arrear
        FOREIGN KEY (save_id, run_revision, lease_arrear_id)
        REFERENCES lease_arrear (save_id, run_revision, id),
    CONSTRAINT fk_lease_arrear_payment_command
        FOREIGN KEY (save_id, command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT fk_lease_arrear_payment_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT ck_lease_arrear_payment_number CHECK (payment_no > 0),
    CONSTRAINT ck_lease_arrear_payment_amount CHECK (
        amount_krw BETWEEN 1 AND 9007199254740991
    ),
    CONSTRAINT ck_lease_arrear_payment_command CHECK (
        command_id REGEXP '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
    ),
    CONSTRAINT ck_lease_arrear_payment_state CHECK (
        (status = 'prepared' AND ledger_transaction_id IS NULL)
        OR (status = 'applied' AND ledger_transaction_id IS NOT NULL)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_lease_rent_charge_valid_insert
BEFORE INSERT ON lease_rent_charge
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'pending'
        AND NEW.paid_krw IS NULL
        AND NEW.arrear_krw IS NULL
        AND NEW.ledger_transaction_id IS NULL
        AND EXISTS (
            SELECT 1
            FROM lease_contract AS contract
            INNER JOIN save
                ON save.id = contract.save_id
               AND save.run_revision = contract.run_revision
            INNER JOIN market_world AS world
                ON world.id = save.market_world_id
            WHERE contract.id = NEW.lease_contract_id
              AND contract.save_id = NEW.save_id
              AND contract.run_revision = NEW.run_revision
              AND contract.offer_kind = 'monthlyRent'
              AND contract.monthly_rent_krw = NEW.amount_krw
              AND contract.rent_charge_rule = 'nextMonthStartFull'
              AND contract.arrear_repayment_rule = 'manualOnly'
              AND contract.effective_to_game_day IS NULL
              AND NEW.due_game_day > contract.effective_from_game_day
              AND NEW.due_year_month
                    = DATE_ADD(world.start_date, INTERVAL NEW.due_game_day DAY)
              AND (
                  (
                      NEW.charge_no = 1
                      AND NEW.due_year_month = DATE_ADD(
                          LAST_DAY(DATE_ADD(
                              world.start_date,
                              INTERVAL contract.effective_from_game_day DAY
                          )),
                          INTERVAL 1 DAY
                      )
                      AND NOT EXISTS (
                          SELECT 1
                          FROM lease_rent_charge AS existing
                          WHERE existing.lease_contract_id = contract.id
                      )
                  )
                  OR EXISTS (
                      SELECT 1
                      FROM lease_rent_charge AS previous_charge
                      WHERE previous_charge.lease_contract_id = contract.id
                        AND previous_charge.charge_no + 1 = NEW.charge_no
                        AND previous_charge.status = 'settled'
                        AND NEW.due_year_month
                              = DATE_ADD(previous_charge.due_year_month, INTERVAL 1 MONTH)
                  )
              )
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_lease_rent_charge_transition_only
BEFORE UPDATE ON lease_rent_charge
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'pending'
        AND NEW.status IN ('settled', 'cancelled')
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.lease_contract_id = OLD.lease_contract_id
        AND NEW.charge_no = OLD.charge_no
        AND NEW.due_year_month = OLD.due_year_month
        AND NEW.due_game_day = OLD.due_game_day
        AND NEW.amount_krw = OLD.amount_krw
        AND NEW.created_at = OLD.created_at
        AND (
            (
                NEW.status = 'settled'
                AND NEW.paid_krw >= 0
                AND NEW.arrear_krw >= 0
                AND NEW.paid_krw + NEW.arrear_krw = OLD.amount_krw
                AND EXISTS (
                    SELECT 1
                    FROM ledger_transaction AS ledger
                    WHERE ledger.id = NEW.ledger_transaction_id
                      AND ledger.save_id = OLD.save_id
                      AND ledger.run_revision = OLD.run_revision
                      AND ledger.game_day = OLD.due_game_day
                      AND ledger.source_kind = 'leaseRent'
                      AND BINARY ledger.source_id = BINARY CAST(OLD.id AS CHAR)
                )
                AND (
                    (
                        NEW.arrear_krw = 0
                        AND NOT EXISTS (
                            SELECT 1
                            FROM lease_arrear AS arrear
                            WHERE arrear.lease_rent_charge_id = OLD.id
                        )
                    )
                    OR EXISTS (
                        SELECT 1
                        FROM lease_arrear AS arrear
                        WHERE arrear.lease_rent_charge_id = OLD.id
                          AND arrear.save_id = OLD.save_id
                          AND arrear.run_revision = OLD.run_revision
                          AND arrear.original_krw = NEW.arrear_krw
                          AND arrear.paid_krw = 0
                          AND arrear.status = 'active'
                    )
                )
            )
            OR (
                NEW.status = 'cancelled'
                AND NEW.paid_krw IS NULL
                AND NEW.arrear_krw IS NULL
                AND NEW.ledger_transaction_id IS NULL
                AND EXISTS (
                    SELECT 1
                    FROM lease_contract AS contract
                    INNER JOIN save
                        ON save.id = contract.save_id
                       AND save.run_revision = contract.run_revision
                    WHERE contract.id = OLD.lease_contract_id
                      AND contract.save_id = OLD.save_id
                      AND contract.run_revision = OLD.run_revision
                      AND contract.effective_to_game_day IS NULL
                      AND OLD.due_game_day > save.game_day
                )
            )
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_lease_rent_charge_no_delete
BEFORE DELETE ON lease_rent_charge
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'lease rent charges are immutable history';

CREATE TRIGGER tr_lease_arrear_valid_insert
BEFORE INSERT ON lease_arrear
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'active'
        AND NEW.paid_krw = 0
        AND NEW.closed_game_day IS NULL
        AND EXISTS (
            SELECT 1
            FROM lease_rent_charge AS charge
            INNER JOIN lease_contract AS contract
                ON contract.id = charge.lease_contract_id
               AND contract.save_id = charge.save_id
               AND contract.run_revision = charge.run_revision
            INNER JOIN save
                ON save.id = charge.save_id
               AND save.run_revision = charge.run_revision
            WHERE charge.id = NEW.lease_rent_charge_id
              AND charge.save_id = NEW.save_id
              AND charge.run_revision = NEW.run_revision
              AND charge.lease_contract_id = NEW.lease_contract_id
              AND contract.household_id = NEW.household_id
              AND charge.status = 'pending'
              AND charge.due_year_month = NEW.due_year_month
              AND NEW.original_krw BETWEEN 1 AND charge.amount_krw
              AND NEW.created_game_day = charge.due_game_day
              AND save.game_day + 1 = NEW.created_game_day
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_lease_arrear_transition_only
BEFORE UPDATE ON lease_arrear
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'active'
        AND NEW.status IN ('active', 'paid')
        AND NEW.paid_krw > OLD.paid_krw
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.household_id = OLD.household_id
        AND NEW.lease_contract_id = OLD.lease_contract_id
        AND NEW.lease_rent_charge_id = OLD.lease_rent_charge_id
        AND NEW.due_year_month = OLD.due_year_month
        AND NEW.original_krw = OLD.original_krw
        AND NEW.created_game_day = OLD.created_game_day
        AND NEW.created_at = OLD.created_at
        AND NEW.paid_krw = (
            SELECT COALESCE(SUM(payment.amount_krw), 0)
            FROM lease_arrear_payment AS payment
            WHERE payment.lease_arrear_id = OLD.id
              AND payment.status = 'applied'
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_lease_arrear_no_delete
BEFORE DELETE ON lease_arrear
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'lease arrears are immutable obligations';

CREATE TRIGGER tr_lease_arrear_payment_valid_insert
BEFORE INSERT ON lease_arrear_payment
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'prepared'
        AND NEW.ledger_transaction_id IS NULL
        AND NEW.payment_no = (
            SELECT COALESCE(MAX(existing.payment_no), 0) + 1
            FROM lease_arrear_payment AS existing
            WHERE existing.lease_arrear_id = NEW.lease_arrear_id
        )
        AND EXISTS (
            SELECT 1
            FROM lease_arrear AS arrear
            INNER JOIN save
                ON save.id = arrear.save_id
               AND save.run_revision = arrear.run_revision
            INNER JOIN command_identity AS identity
                ON identity.save_id = arrear.save_id
               AND BINARY identity.command_id = BINARY NEW.command_id
            WHERE arrear.id = NEW.lease_arrear_id
              AND arrear.save_id = NEW.save_id
              AND arrear.run_revision = NEW.run_revision
              AND arrear.status = 'active'
              AND NEW.amount_krw <= arrear.remaining_krw
              AND NEW.game_day = save.game_day
              AND identity.command_kind = 'payLeaseArrear'
              AND identity.initial_run_revision = arrear.run_revision
              AND identity.initial_state_revision = save.state_revision
              AND identity.initial_game_day = save.game_day
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_lease_arrear_payment_transition_only
BEFORE UPDATE ON lease_arrear_payment
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'prepared'
        AND NEW.status = 'applied'
        AND NEW.ledger_transaction_id IS NOT NULL
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.lease_arrear_id = OLD.lease_arrear_id
        AND NEW.payment_no = OLD.payment_no
        AND NEW.amount_krw = OLD.amount_krw
        AND NEW.game_day = OLD.game_day
        AND BINARY NEW.command_id = BINARY OLD.command_id
        AND NEW.created_at = OLD.created_at
        AND EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = OLD.save_id
              AND ledger.run_revision = OLD.run_revision
              AND ledger.game_day = OLD.game_day
              AND ledger.source_kind = 'leaseArrearPayment'
              AND BINARY ledger.source_id = BINARY CAST(OLD.id AS CHAR)
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_lease_arrear_payment_no_delete
BEFORE DELETE ON lease_arrear_payment
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'lease arrear payments are immutable';

ALTER TABLE scheduled_settlement
    DROP CHECK ck_scheduled_settlement_kind,
    DROP CHECK ck_scheduled_settlement_source_kind,
    ADD CONSTRAINT ck_scheduled_settlement_kind CHECK (
        kind IN (
            'cmaInterest', 'depositMaturity', 'savingsInstallment', 'savingsMaturity',
            'bondCoupon', 'bondMaturity', 'llxDistribution', 'financialIncomeFiling',
            'employmentPayroll', 'employmentReconciliation', 'militaryPay',
            'militarySavingsInstallment', 'militarySavingsMaturity',
            'militarySavingsGovernmentMatch', 'livingCostMonth', 'loanInstallment',
            'leaseRent'
        )
    ),
    ADD CONSTRAINT ck_scheduled_settlement_source_kind CHECK (
        source_kind IN (
            'cmaAccount', 'depositContract', 'savingsContract', 'bondPosition',
            'indexPosition', 'taxYear', 'employmentContract', 'yearEndTaxAssessment',
            'militaryService', 'militarySavingsContract', 'militarySavingsInstallment',
            'livingCostMonth', 'loanContract', 'leaseContract'
        )
    ),
    ADD CONSTRAINT ck_scheduled_settlement_lease_rent_payload CHECK (
        kind <> 'leaseRent'
        OR (
            JSON_TYPE(payload) = 'OBJECT'
            AND JSON_LENGTH(payload) = 4
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.version')) = 'INTEGER'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.version')) = '1'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.leaseContractId')) = 'STRING'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.leaseContractId'))
                REGEXP '^[1-9][0-9]{0,19}$'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.rentChargeId')) = 'STRING'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.rentChargeId'))
                REGEXP '^[1-9][0-9]{0,19}$'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.chargeNo')) = 'INTEGER'
            AND CAST(JSON_UNQUOTE(JSON_EXTRACT(payload, '$.chargeNo')) AS UNSIGNED) > 0
            AND source_kind = 'leaseContract'
            AND BINARY source_id
                = BINARY JSON_UNQUOTE(JSON_EXTRACT(payload, '$.leaseContractId'))
            AND occurrence
                = CAST(JSON_UNQUOTE(JSON_EXTRACT(payload, '$.chargeNo')) AS UNSIGNED)
        )
    );

CREATE TRIGGER tr_scheduled_settlement_lease_rent_insert
BEFORE INSERT ON scheduled_settlement
FOR EACH ROW
FOLLOWS tr_scheduled_settlement_loan_insert
SET NEW.status = IF(
    NEW.kind <> 'leaseRent'
        OR EXISTS (
            SELECT 1
            FROM lease_contract AS contract
            INNER JOIN lease_rent_charge AS charge
                ON charge.lease_contract_id = contract.id
               AND charge.id = CAST(
                   JSON_UNQUOTE(JSON_EXTRACT(NEW.payload, '$.rentChargeId')) AS UNSIGNED
               )
            WHERE contract.id = CAST(
                      JSON_UNQUOTE(JSON_EXTRACT(NEW.payload, '$.leaseContractId')) AS UNSIGNED
                  )
              AND contract.save_id = NEW.save_id
              AND contract.run_revision = NEW.run_revision
              AND contract.offer_kind = 'monthlyRent'
              AND contract.effective_to_game_day IS NULL
              AND charge.save_id = NEW.save_id
              AND charge.run_revision = NEW.run_revision
              AND charge.status = 'pending'
              AND charge.charge_no = NEW.occurrence
              AND charge.due_game_day = NEW.due_game_day
        ),
    NEW.status,
    NULL
);

CREATE TRIGGER tr_scheduled_settlement_lease_rent_transition
BEFORE UPDATE ON scheduled_settlement
FOR EACH ROW
FOLLOWS tr_scheduled_settlement_transition_only
SET NEW.status = IF(
    OLD.kind <> 'leaseRent'
        OR (
            NEW.status = 'settled'
            AND EXISTS (
                SELECT 1
                FROM lease_rent_charge AS charge
                WHERE charge.id = CAST(
                          JSON_UNQUOTE(JSON_EXTRACT(OLD.payload, '$.rentChargeId')) AS UNSIGNED
                      )
                  AND charge.save_id = OLD.save_id
                  AND charge.run_revision = OLD.run_revision
                  AND charge.lease_contract_id = CAST(
                      JSON_UNQUOTE(JSON_EXTRACT(OLD.payload, '$.leaseContractId')) AS UNSIGNED
                  )
                  AND charge.charge_no = OLD.occurrence
                  AND charge.status = 'settled'
                  AND charge.ledger_transaction_id = NEW.settled_ledger_transaction_id
            )
        )
        OR (
            NEW.status = 'cancelled'
            AND NEW.cancellation_reason = 'leaseEnded'
            AND NEW.cancellation_ledger_transaction_id IS NULL
            AND EXISTS (
                SELECT 1
                FROM lease_rent_charge AS charge
                WHERE charge.id = CAST(
                          JSON_UNQUOTE(JSON_EXTRACT(OLD.payload, '$.rentChargeId')) AS UNSIGNED
                      )
                  AND charge.save_id = OLD.save_id
                  AND charge.run_revision = OLD.run_revision
                  AND charge.lease_contract_id = CAST(
                      JSON_UNQUOTE(JSON_EXTRACT(OLD.payload, '$.leaseContractId')) AS UNSIGNED
                  )
                  AND charge.charge_no = OLD.occurrence
                  AND charge.status = 'cancelled'
            )
        ),
    NEW.status,
    NULL
);

ALTER TABLE ledger_transaction
    DROP CHECK ck_ledger_transaction_lease_source,
    ADD CONSTRAINT ck_ledger_transaction_lease_source CHECK (
        source_kind NOT LIKE 'lease%'
        OR source_kind IN ('leaseMove', 'leaseRent', 'leaseArrearPayment')
    );

ALTER TABLE ledger_posting
    DROP CHECK ck_ledger_posting_account_code,
    DROP CHECK ck_ledger_posting_account_reference,
    ADD COLUMN lease_rent_charge_id BIGINT UNSIGNED NULL AFTER lease_contract_id,
    ADD COLUMN lease_arrear_id BIGINT UNSIGNED NULL AFTER lease_rent_charge_id,
    ADD KEY ix_ledger_posting_lease_rent_charge
        (save_id, run_revision, lease_rent_charge_id),
    ADD KEY ix_ledger_posting_lease_arrear
        (save_id, run_revision, lease_arrear_id),
    ADD CONSTRAINT fk_ledger_posting_lease_rent_charge
        FOREIGN KEY (save_id, run_revision, lease_rent_charge_id)
        REFERENCES lease_rent_charge (save_id, run_revision, id),
    ADD CONSTRAINT fk_ledger_posting_lease_arrear
        FOREIGN KEY (save_id, run_revision, lease_arrear_id)
        REFERENCES lease_arrear (save_id, run_revision, id),
    ADD CONSTRAINT ck_ledger_posting_account_code CHECK (
        account_code IN (
            'wallet', 'accountCash', 'productPrincipal', 'debtPrincipal',
            'openingEquity', 'withholdingTaxLiability', 'interestIncome',
            'feeExpense', 'distributionIncome', 'realizedGainLoss', 'taxSettlement',
            'careerDevelopmentExpense', 'salaryIncome',
            'employeeNationalPensionExpense', 'employeeHealthInsuranceExpense',
            'employeeLongTermCareExpense', 'employeeEmploymentInsuranceExpense',
            'employmentIncomeTaxWithholding', 'employmentLocalIncomeTaxWithholding',
            'otherIncomeReward', 'otherIncomeTaxWithholding',
            'otherLocalIncomeTaxWithholding', 'pensionTaxExcludedContribution',
            'pensionCreditedContribution', 'militaryPayIncome',
            'militarySavingsPrincipal', 'militarySavingsBankInterest',
            'militarySavingsGovernmentMatchIncome',
            'livingCostExpense', 'essentialArrearLiability',
            'loanPrincipalLiability', 'loanInterestExpense', 'loanInterestLiability',
            'loanFeeExpense', 'taxObligationLiability',
            'leaseDepositAsset', 'movingExpense',
            'leaseRentExpense', 'leaseArrearLiability'
        )
    ),
    ADD CONSTRAINT ck_ledger_posting_account_reference CHECK (
        (
            account_code IN (
                'accountCash', 'productPrincipal',
                'pensionTaxExcludedContribution', 'pensionCreditedContribution'
            )
            AND financial_account_id IS NOT NULL
            AND military_savings_contract_id IS NULL
            AND living_cost_month_id IS NULL
            AND essential_arrear_id IS NULL
            AND loan_contract_id IS NULL
            AND tax_obligation_id IS NULL
            AND lease_contract_id IS NULL
            AND lease_rent_charge_id IS NULL
            AND lease_arrear_id IS NULL
        )
        OR (
            account_code IN (
                'militarySavingsPrincipal', 'militarySavingsBankInterest',
                'militarySavingsGovernmentMatchIncome'
            )
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NOT NULL
            AND living_cost_month_id IS NULL
            AND essential_arrear_id IS NULL
            AND loan_contract_id IS NULL
            AND tax_obligation_id IS NULL
            AND lease_contract_id IS NULL
            AND lease_rent_charge_id IS NULL
            AND lease_arrear_id IS NULL
        )
        OR (
            account_code = 'livingCostExpense'
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NULL
            AND living_cost_month_id IS NOT NULL
            AND essential_arrear_id IS NULL
            AND loan_contract_id IS NULL
            AND tax_obligation_id IS NULL
            AND lease_contract_id IS NULL
            AND lease_rent_charge_id IS NULL
            AND lease_arrear_id IS NULL
        )
        OR (
            account_code = 'essentialArrearLiability'
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NULL
            AND living_cost_month_id IS NULL
            AND essential_arrear_id IS NOT NULL
            AND loan_contract_id IS NULL
            AND tax_obligation_id IS NULL
            AND lease_contract_id IS NULL
            AND lease_rent_charge_id IS NULL
            AND lease_arrear_id IS NULL
        )
        OR (
            account_code IN (
                'loanPrincipalLiability', 'loanInterestExpense',
                'loanInterestLiability', 'loanFeeExpense'
            )
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NULL
            AND living_cost_month_id IS NULL
            AND essential_arrear_id IS NULL
            AND loan_contract_id IS NOT NULL
            AND tax_obligation_id IS NULL
            AND lease_contract_id IS NULL
            AND lease_rent_charge_id IS NULL
            AND lease_arrear_id IS NULL
        )
        OR (
            account_code = 'taxObligationLiability'
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NULL
            AND living_cost_month_id IS NULL
            AND essential_arrear_id IS NULL
            AND loan_contract_id IS NULL
            AND tax_obligation_id IS NOT NULL
            AND lease_contract_id IS NULL
            AND lease_rent_charge_id IS NULL
            AND lease_arrear_id IS NULL
        )
        OR (
            account_code = 'leaseDepositAsset'
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NULL
            AND living_cost_month_id IS NULL
            AND essential_arrear_id IS NULL
            AND loan_contract_id IS NULL
            AND tax_obligation_id IS NULL
            AND lease_contract_id IS NOT NULL
            AND lease_rent_charge_id IS NULL
            AND lease_arrear_id IS NULL
        )
        OR (
            account_code = 'leaseRentExpense'
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NULL
            AND living_cost_month_id IS NULL
            AND essential_arrear_id IS NULL
            AND loan_contract_id IS NULL
            AND tax_obligation_id IS NULL
            AND lease_contract_id IS NULL
            AND lease_rent_charge_id IS NOT NULL
            AND lease_arrear_id IS NULL
        )
        OR (
            account_code = 'leaseArrearLiability'
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NULL
            AND living_cost_month_id IS NULL
            AND essential_arrear_id IS NULL
            AND loan_contract_id IS NULL
            AND tax_obligation_id IS NULL
            AND lease_contract_id IS NULL
            AND lease_rent_charge_id IS NULL
            AND lease_arrear_id IS NOT NULL
        )
        OR (
            account_code NOT IN (
                'accountCash', 'productPrincipal',
                'pensionTaxExcludedContribution', 'pensionCreditedContribution',
                'militarySavingsPrincipal', 'militarySavingsBankInterest',
                'militarySavingsGovernmentMatchIncome',
                'livingCostExpense', 'essentialArrearLiability',
                'loanPrincipalLiability', 'loanInterestExpense',
                'loanInterestLiability', 'loanFeeExpense', 'taxObligationLiability',
                'leaseDepositAsset', 'leaseRentExpense', 'leaseArrearLiability'
            )
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NULL
            AND living_cost_month_id IS NULL
            AND essential_arrear_id IS NULL
            AND loan_contract_id IS NULL
            AND tax_obligation_id IS NULL
            AND lease_contract_id IS NULL
            AND lease_rent_charge_id IS NULL
            AND lease_arrear_id IS NULL
        )
    );

DROP TRIGGER tr_ledger_transaction_lease_source_insert;

CREATE TRIGGER tr_ledger_transaction_lease_source_insert
BEFORE INSERT ON ledger_transaction
FOR EACH ROW
FOLLOWS tr_ledger_transaction_loan_source_insert
SET NEW.source_kind = IF(
    (
        NEW.source_kind = 'leaseMove'
        AND NEW.source_id REGEXP
            '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
        AND EXISTS (
            SELECT 1
            FROM lease_contract AS contract
            WHERE contract.save_id = NEW.save_id
              AND contract.run_revision = NEW.run_revision
              AND BINARY contract.command_id = BINARY NEW.source_id
              AND contract.effective_from_game_day = NEW.game_day
              AND contract.effective_to_game_day IS NULL
        )
    )
    OR (
        NEW.source_kind = 'leaseRent'
        AND NEW.source_id REGEXP '^[1-9][0-9]{0,19}$'
        AND EXISTS (
            SELECT 1
            FROM lease_rent_charge AS charge
            WHERE charge.id = CAST(NEW.source_id AS UNSIGNED)
              AND BINARY NEW.source_id = BINARY CAST(charge.id AS CHAR)
              AND charge.save_id = NEW.save_id
              AND charge.run_revision = NEW.run_revision
              AND charge.due_game_day = NEW.game_day
              AND charge.status = 'pending'
        )
    )
    OR (
        NEW.source_kind = 'leaseArrearPayment'
        AND NEW.source_id REGEXP '^[1-9][0-9]{0,19}$'
        AND EXISTS (
            SELECT 1
            FROM lease_arrear_payment AS payment
            WHERE payment.id = CAST(NEW.source_id AS UNSIGNED)
              AND BINARY NEW.source_id = BINARY CAST(payment.id AS CHAR)
              AND payment.save_id = NEW.save_id
              AND payment.run_revision = NEW.run_revision
              AND payment.game_day = NEW.game_day
              AND payment.status = 'prepared'
        )
    )
    OR NEW.source_kind NOT IN ('leaseMove', 'leaseRent', 'leaseArrearPayment'),
    NEW.source_kind,
    NULL
);

DROP TRIGGER tr_ledger_posting_lease_reference_insert;

CREATE TRIGGER tr_ledger_posting_lease_reference_insert
BEFORE INSERT ON ledger_posting
FOR EACH ROW
FOLLOWS tr_ledger_posting_loan_reference_insert
SET NEW.account_code = IF(
    (
        NEW.account_code = 'leaseDepositAsset'
        AND NEW.lease_rent_charge_id IS NULL
        AND NEW.lease_arrear_id IS NULL
        AND EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            INNER JOIN lease_contract AS contract
                ON contract.id = NEW.lease_contract_id
               AND contract.save_id = ledger.save_id
               AND contract.run_revision = ledger.run_revision
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'leaseMove'
              AND (
                  (
                      NEW.amount_krw = contract.deposit_krw
                      AND contract.effective_from_game_day = ledger.game_day
                      AND BINARY contract.command_id = BINARY ledger.source_id
                  )
                  OR (
                      NEW.amount_krw = -contract.deposit_krw
                      AND contract.effective_to_game_day = ledger.game_day
                  )
              )
        )
    )
    OR (
        NEW.account_code = 'movingExpense'
        AND NEW.lease_contract_id IS NULL
        AND NEW.lease_rent_charge_id IS NULL
        AND NEW.lease_arrear_id IS NULL
        AND EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            INNER JOIN lease_contract AS contract
                ON contract.save_id = ledger.save_id
               AND contract.run_revision = ledger.run_revision
               AND BINARY contract.command_id = BINARY ledger.source_id
            INNER JOIN real_estate_region_moving_cost AS moving_cost
                ON moving_cost.real_estate_model_version_id
                        = contract.real_estate_model_version_id
               AND BINARY moving_cost.region_key = BINARY contract.region_key
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'leaseMove'
              AND NEW.amount_krw = moving_cost.moving_cost_krw
        )
    )
    OR (
        NEW.account_code = 'wallet'
        AND NEW.lease_contract_id IS NULL
        AND NEW.lease_rent_charge_id IS NULL
        AND NEW.lease_arrear_id IS NULL
        AND EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            INNER JOIN lease_contract AS started_contract
                ON started_contract.save_id = ledger.save_id
               AND started_contract.run_revision = ledger.run_revision
               AND BINARY started_contract.command_id = BINARY ledger.source_id
            INNER JOIN real_estate_region_moving_cost AS moving_cost
                ON moving_cost.real_estate_model_version_id
                        = started_contract.real_estate_model_version_id
               AND BINARY moving_cost.region_key = BINARY started_contract.region_key
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'leaseMove'
              AND (
                  NEW.amount_krw = -started_contract.deposit_krw
                  OR NEW.amount_krw = -moving_cost.moving_cost_krw
                  OR EXISTS (
                      SELECT 1
                      FROM lease_contract AS ended_contract
                      WHERE ended_contract.save_id = ledger.save_id
                        AND ended_contract.run_revision = ledger.run_revision
                        AND ended_contract.household_id = started_contract.household_id
                        AND ended_contract.effective_to_game_day = ledger.game_day
                        AND NEW.amount_krw = ended_contract.deposit_krw
                  )
              )
        )
    )
    OR (
        NEW.account_code NOT IN ('leaseDepositAsset', 'movingExpense')
        AND NOT EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'leaseMove'
        )
    ),
    NEW.account_code,
    NULL
);

CREATE TRIGGER tr_ledger_posting_lease_rent_reference_insert
BEFORE INSERT ON ledger_posting
FOR EACH ROW
FOLLOWS tr_ledger_posting_lease_reference_insert
SET NEW.account_code = IF(
    (
        NEW.account_code = 'leaseRentExpense'
        AND NEW.lease_arrear_id IS NULL
        AND EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            INNER JOIN lease_rent_charge AS charge
                ON charge.id = NEW.lease_rent_charge_id
               AND charge.save_id = ledger.save_id
               AND charge.run_revision = ledger.run_revision
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'leaseRent'
              AND BINARY ledger.source_id = BINARY CAST(charge.id AS CHAR)
              AND charge.status = 'pending'
              AND NEW.amount_krw = charge.amount_krw
        )
    )
    OR (
        NEW.account_code = 'leaseArrearLiability'
        AND NEW.lease_rent_charge_id IS NULL
        AND EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            INNER JOIN lease_arrear AS arrear
                ON arrear.id = NEW.lease_arrear_id
               AND arrear.save_id = ledger.save_id
               AND arrear.run_revision = ledger.run_revision
            INNER JOIN lease_rent_charge AS charge
                ON charge.id = arrear.lease_rent_charge_id
            LEFT JOIN lease_arrear_payment AS payment
                ON payment.lease_arrear_id = arrear.id
               AND BINARY CAST(payment.id AS CHAR) = BINARY ledger.source_id
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND (
                  (
                      ledger.source_kind = 'leaseRent'
                      AND BINARY ledger.source_id = BINARY CAST(charge.id AS CHAR)
                      AND charge.status = 'pending'
                      AND arrear.status = 'active'
                      AND arrear.paid_krw = 0
                      AND NEW.amount_krw = -arrear.original_krw
                  )
                  OR (
                      ledger.source_kind = 'leaseArrearPayment'
                      AND payment.status = 'prepared'
                      AND NEW.amount_krw = payment.amount_krw
                  )
              )
        )
    )
    OR (
        NEW.account_code = 'wallet'
        AND NEW.lease_rent_charge_id IS NULL
        AND NEW.lease_arrear_id IS NULL
        AND EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            LEFT JOIN lease_rent_charge AS charge
                ON BINARY CAST(charge.id AS CHAR) = BINARY ledger.source_id
               AND charge.save_id = ledger.save_id
               AND charge.run_revision = ledger.run_revision
            LEFT JOIN lease_arrear_payment AS payment
                ON BINARY CAST(payment.id AS CHAR) = BINARY ledger.source_id
               AND payment.save_id = ledger.save_id
               AND payment.run_revision = ledger.run_revision
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND (
                  (
                      ledger.source_kind = 'leaseRent'
                      AND charge.status = 'pending'
                      AND NEW.amount_krw BETWEEN -charge.amount_krw AND -1
                  )
                  OR (
                      ledger.source_kind = 'leaseArrearPayment'
                      AND payment.status = 'prepared'
                      AND NEW.amount_krw = -payment.amount_krw
                  )
              )
        )
    )
    OR (
        NEW.account_code NOT IN ('leaseRentExpense', 'leaseArrearLiability', 'wallet')
        AND NEW.lease_rent_charge_id IS NULL
        AND NEW.lease_arrear_id IS NULL
        AND NOT EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind IN ('leaseRent', 'leaseArrearPayment')
        )
    )
    OR (
        NEW.account_code = 'wallet'
        AND NEW.lease_rent_charge_id IS NULL
        AND NEW.lease_arrear_id IS NULL
        AND NOT EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind IN ('leaseRent', 'leaseArrearPayment')
        )
    ),
    NEW.account_code,
    NULL
);

DROP TRIGGER tr_real_estate_model_version_seal_only;

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
                AND (
                    (
                        JSON_TYPE(JSON_EXTRACT(OLD.parameters, '$.schemaVersion')) = 'INTEGER'
                        AND JSON_UNQUOTE(JSON_EXTRACT(OLD.parameters, '$.schemaVersion')) = '2'
                        AND
                        (
                            SELECT COUNT(*)
                            FROM real_estate_lease_profile AS lease_profile
                            WHERE lease_profile.real_estate_model_version_id = OLD.id
                        ) = 1
                        AND EXISTS (
                            SELECT 1
                            FROM real_estate_lease_profile AS lease_profile
                            WHERE lease_profile.real_estate_model_version_id = OLD.id
                              AND lease_profile.offer_kind = 'jeonse'
                              AND lease_profile.renewal_rule = 'openEnded'
                              AND lease_profile.rent_charge_rule IS NULL
                              AND lease_profile.arrear_repayment_rule IS NULL
                        )
                    )
                    OR (
                        JSON_TYPE(JSON_EXTRACT(OLD.parameters, '$.schemaVersion')) = 'INTEGER'
                        AND JSON_UNQUOTE(JSON_EXTRACT(OLD.parameters, '$.schemaVersion')) = '3'
                        AND
                        (
                            SELECT COUNT(*)
                            FROM real_estate_lease_profile AS lease_profile
                            WHERE lease_profile.real_estate_model_version_id = OLD.id
                        ) = 2
                        AND EXISTS (
                            SELECT 1
                            FROM real_estate_lease_profile AS lease_profile
                            WHERE lease_profile.real_estate_model_version_id = OLD.id
                              AND lease_profile.offer_kind = 'jeonse'
                              AND lease_profile.renewal_rule = 'openEnded'
                              AND lease_profile.rent_charge_rule IS NULL
                              AND lease_profile.arrear_repayment_rule IS NULL
                        )
                        AND EXISTS (
                            SELECT 1
                            FROM real_estate_lease_profile AS lease_profile
                            WHERE lease_profile.real_estate_model_version_id = OLD.id
                              AND lease_profile.offer_kind = 'monthlyRent'
                              AND lease_profile.renewal_rule = 'openEnded'
                              AND lease_profile.rent_charge_rule = 'nextMonthStartFull'
                              AND lease_profile.arrear_repayment_rule = 'manualOnly'
                        )
                    )
                )
                AND (
                    SELECT COUNT(*)
                    FROM real_estate_region_moving_cost AS moving_cost
                    WHERE moving_cost.real_estate_model_version_id = OLD.id
                ) = (SELECT COUNT(*) FROM life_region)
                AND NOT EXISTS (
                    SELECT 1
                    FROM life_region AS region
                    WHERE NOT EXISTS (
                        SELECT 1
                        FROM real_estate_region_moving_cost AS moving_cost
                        WHERE moving_cost.real_estate_model_version_id = OLD.id
                          AND BINARY moving_cost.region_key = BINARY region.region_key
                    )
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
        'dev-unranked-m4-real-estate-rent-2026-v3',
        'active',
        FALSE,
        JSON_OBJECT(
            'entropyVersion', 'sha256-counter-be-v1',
            'generatorVersion', 'm4-c1-v1',
            'schemaVersion', 3
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
    target.id, source.region_key, source.monthly_listing_slot_count,
    source.minimum_exclusive_area_square_meters,
    source.maximum_exclusive_area_square_meters,
    source.base_price_per_square_meter_krw, source.price_daily_drift_ppm,
    source.price_daily_shock_amplitude_ppm, source.rent_daily_drift_ppm,
    source.rent_daily_shock_amplitude_ppm, source.minimum_index_ppm,
    source.maximum_index_ppm, source.minimum_price_variation_ppm,
    source.maximum_price_variation_ppm, source.jeonse_ratio_ppm,
    source.annual_gross_rent_yield_ppm, source.monthly_deposit_ratio_ppm,
    source.availability_rule, source.offer_rotation_rule
FROM real_estate_model_version AS target
INNER JOIN real_estate_model_version AS source_model
    ON source_model.version_key = 'dev-unranked-m4-real-estate-lease-2026-v2'
INNER JOIN real_estate_region_profile AS source
    ON source.real_estate_model_version_id = source_model.id
WHERE target.version_key = 'dev-unranked-m4-real-estate-rent-2026-v3';

INSERT INTO real_estate_region_property_type
    (real_estate_model_version_id, region_key, property_type, property_type_order)
SELECT target.id, source.region_key, source.property_type, source.property_type_order
FROM real_estate_model_version AS target
INNER JOIN real_estate_model_version AS source_model
    ON source_model.version_key = 'dev-unranked-m4-real-estate-lease-2026-v2'
INNER JOIN real_estate_region_property_type AS source
    ON source.real_estate_model_version_id = source_model.id
WHERE target.version_key = 'dev-unranked-m4-real-estate-rent-2026-v3';

INSERT INTO real_estate_lease_profile
    (
        real_estate_model_version_id, offer_kind, renewal_rule,
        rent_charge_rule, arrear_repayment_rule
    )
SELECT target.id, source.offer_kind, source.renewal_rule,
       source.rent_charge_rule, source.arrear_repayment_rule
FROM real_estate_model_version AS target
INNER JOIN real_estate_model_version AS source_model
    ON source_model.version_key = 'dev-unranked-m4-real-estate-lease-2026-v2'
INNER JOIN real_estate_lease_profile AS source
    ON source.real_estate_model_version_id = source_model.id
WHERE target.version_key = 'dev-unranked-m4-real-estate-rent-2026-v3';

INSERT INTO real_estate_lease_profile
    (
        real_estate_model_version_id, offer_kind, renewal_rule,
        rent_charge_rule, arrear_repayment_rule
    )
SELECT model.id, 'monthlyRent', 'openEnded', 'nextMonthStartFull', 'manualOnly'
FROM real_estate_model_version AS model
WHERE model.version_key = 'dev-unranked-m4-real-estate-rent-2026-v3';

INSERT INTO real_estate_region_moving_cost
    (real_estate_model_version_id, region_key, moving_cost_krw)
SELECT target.id, source.region_key, source.moving_cost_krw
FROM real_estate_model_version AS target
INNER JOIN real_estate_model_version AS source_model
    ON source_model.version_key = 'dev-unranked-m4-real-estate-lease-2026-v2'
INNER JOIN real_estate_region_moving_cost AS source
    ON source.real_estate_model_version_id = source_model.id
WHERE target.version_key = 'dev-unranked-m4-real-estate-rent-2026-v3';

INSERT INTO real_estate_model_strict_manifest
    (real_estate_model_version_id, canonical_json)
SELECT real_estate_model_version_id, canonical_json
FROM real_estate_model_strict_projection
WHERE real_estate_model_version_id = (
    SELECT id
    FROM real_estate_model_version
    WHERE version_key = 'dev-unranked-m4-real-estate-rent-2026-v3'
);

UPDATE real_estate_model_version AS model
INNER JOIN real_estate_model_strict_manifest AS manifest
    ON manifest.real_estate_model_version_id = model.id
SET model.canonical_sha256 = manifest.canonical_sha256,
    model.sealed_at = CURRENT_TIMESTAMP(3)
WHERE model.version_key = 'dev-unranked-m4-real-estate-rent-2026-v3'
  AND model.sealed_at IS NULL;

UPDATE run_rule_bundle_assignment AS assignment
INNER JOIN real_estate_model_version AS active_real_estate
    ON active_real_estate.version_key = 'dev-unranked-m4-real-estate-rent-2026-v3'
   AND active_real_estate.availability = 'active'
   AND active_real_estate.sealed_at IS NOT NULL
SET assignment.real_estate_model_version_id = active_real_estate.id
WHERE assignment.assignment_key = 'newRun';
