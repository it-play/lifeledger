-- M4-C2b2 fixed lease terms, renewal notices, and arrear-based termination review (§5.6).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;

-- MySQL DDL auto-commits. Reject any state that is not the sealed v1-v3 shape before the
-- first durable change, so a forward migration cannot strand only part of the lifecycle model.
CREATE TEMPORARY TABLE m4c2b2_preflight_guard (
    guard_key VARCHAR(40) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    accepted TINYINT UNSIGNED NOT NULL,
    PRIMARY KEY (guard_key),
    CONSTRAINT ck_m4c2b2_preflight_guard CHECK (accepted = 1)
);

INSERT INTO m4c2b2_preflight_guard (guard_key, accepted)
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
              OR BINARY manifest.canonical_sha256 <> BINARY model.canonical_sha256
          )
    ),
    1,
    0
);

INSERT INTO m4c2b2_preflight_guard (guard_key, accepted)
SELECT 'legacy-model-shapes', IF(
    NOT EXISTS (
        SELECT 1
        FROM real_estate_model_version AS model
        WHERE model.availability = 'active'
          AND (
              JSON_TYPE(JSON_EXTRACT(model.parameters, '$.schemaVersion')) <> 'INTEGER'
              OR JSON_UNQUOTE(JSON_EXTRACT(model.parameters, '$.schemaVersion'))
                    NOT IN ('1', '2', '3')
          )
    )
    AND NOT EXISTS (
        SELECT 1
        FROM real_estate_lease_profile AS profile
        INNER JOIN real_estate_model_version AS model
            ON model.id = profile.real_estate_model_version_id
        WHERE NOT (
            (
                JSON_UNQUOTE(JSON_EXTRACT(model.parameters, '$.schemaVersion')) = '2'
                AND profile.offer_kind = 'jeonse'
                AND profile.renewal_rule = 'openEnded'
                AND profile.rent_charge_rule IS NULL
                AND profile.arrear_repayment_rule IS NULL
            )
            OR (
                JSON_UNQUOTE(JSON_EXTRACT(model.parameters, '$.schemaVersion')) = '3'
                AND profile.renewal_rule = 'openEnded'
                AND (
                    (
                        profile.offer_kind = 'jeonse'
                        AND profile.rent_charge_rule IS NULL
                        AND profile.arrear_repayment_rule IS NULL
                    )
                    OR (
                        profile.offer_kind = 'monthlyRent'
                        AND profile.rent_charge_rule = 'nextMonthStartFull'
                        AND profile.arrear_repayment_rule = 'manualOnly'
                    )
                )
            )
        )
    )
    AND NOT EXISTS (
        SELECT 1
        FROM real_estate_model_version
        WHERE version_key = 'dev-unranked-m4-real-estate-lifecycle-2026-v4'
    ),
    1,
    0
);

INSERT INTO m4c2b2_preflight_guard (guard_key, accepted)
SELECT 'legacy-contract-shapes', IF(
    NOT EXISTS (
        SELECT 1
        FROM lease_contract
        WHERE renewal_rule <> 'openEnded'
           OR (offer_kind = 'jeonse' AND (
                   monthly_rent_krw IS NOT NULL
                   OR rent_charge_rule IS NOT NULL
                   OR arrear_repayment_rule IS NOT NULL
              ))
           OR (offer_kind = 'monthlyRent' AND (
                   monthly_rent_krw IS NULL
                   OR rent_charge_rule <> 'nextMonthStartFull'
                   OR arrear_repayment_rule <> 'manualOnly'
              ))
           OR offer_kind NOT IN ('jeonse', 'monthlyRent')
    ),
    1,
    0
);

DROP TEMPORARY TABLE m4c2b2_preflight_guard;

ALTER TABLE real_estate_lease_profile
    DROP CHECK ck_real_estate_lease_profile_renewal,
    ADD COLUMN term_months SMALLINT UNSIGNED NULL AFTER renewal_rule,
    ADD COLUMN renewal_notice_lead_days SMALLINT UNSIGNED NULL AFTER term_months,
    ADD COLUMN termination_review_rule
        VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NULL
        AFTER arrear_repayment_rule,
    ADD COLUMN termination_review_after_days SMALLINT UNSIGNED NULL
        AFTER termination_review_rule,
    ADD CONSTRAINT ck_real_estate_lease_profile_renewal CHECK (
        renewal_rule IN ('openEnded', 'fixedTermAutoRenew')
    ),
    ADD CONSTRAINT ck_real_estate_lease_profile_lifecycle CHECK (
        (
            renewal_rule = 'openEnded'
            AND term_months IS NULL
            AND renewal_notice_lead_days IS NULL
            AND termination_review_rule IS NULL
            AND termination_review_after_days IS NULL
        )
        OR (
            renewal_rule = 'fixedTermAutoRenew'
            AND term_months BETWEEN 1 AND 1200
            AND renewal_notice_lead_days BETWEEN 1 AND 65535
            AND (
                (
                    offer_kind = 'jeonse'
                    AND termination_review_rule IS NULL
                    AND termination_review_after_days IS NULL
                )
                OR (
                    offer_kind = 'monthlyRent'
                    AND termination_review_rule = 'oldestActiveArrearAge'
                    AND termination_review_after_days BETWEEN 1 AND 65535
                )
            )
        )
    );

-- Branch on the pinned schema version before inspecting child rows. A v4 model also has a
-- monthly-rent profile, so the v3 EXISTS-based discriminator from 0033 cannot distinguish it.
-- The schema-1, schema-2, and schema-3 JSON expressions are otherwise byte-identical to 0033.
CREATE OR REPLACE VIEW real_estate_model_strict_projection AS
SELECT
    base.real_estate_model_version_id,
    CAST(
        CASE
            WHEN JSON_TYPE(JSON_EXTRACT(base.parameters, '$.schemaVersion')) = 'INTEGER'
                 AND JSON_UNQUOTE(JSON_EXTRACT(base.parameters, '$.schemaVersion')) = '4'
            THEN JSON_MERGE_PATCH(
                base.base_json,
                JSON_OBJECT(
                    'leaseProfiles', COALESCE((
                        SELECT JSON_ARRAYAGG(JSON_OBJECT(
                                   'arrearRepaymentRule',
                                       lease_profile.arrear_repayment_rule,
                                   'offerKind', lease_profile.offer_kind,
                                   'renewalNoticeLeadDays',
                                       lease_profile.renewal_notice_lead_days,
                                   'renewalRule', lease_profile.renewal_rule,
                                   'rentChargeRule', lease_profile.rent_charge_rule,
                                   'termMonths', lease_profile.term_months,
                                   'terminationReviewAfterDays',
                                       lease_profile.termination_review_after_days,
                                   'terminationReviewRule',
                                       lease_profile.termination_review_rule
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
                    'schemaVersion', 4
                )
            )
            WHEN JSON_TYPE(JSON_EXTRACT(base.parameters, '$.schemaVersion')) = 'INTEGER'
                 AND JSON_UNQUOTE(JSON_EXTRACT(base.parameters, '$.schemaVersion')) = '3'
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
            WHEN JSON_TYPE(JSON_EXTRACT(base.parameters, '$.schemaVersion')) = 'INTEGER'
                 AND JSON_UNQUOTE(JSON_EXTRACT(base.parameters, '$.schemaVersion')) = '2'
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
        model.parameters,
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

CREATE TEMPORARY TABLE m4c2b2_manifest_guard (
    guard_key VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    accepted TINYINT UNSIGNED NOT NULL,
    PRIMARY KEY (guard_key),
    CONSTRAINT ck_m4c2b2_manifest_guard CHECK (accepted = 1)
);

INSERT INTO m4c2b2_manifest_guard (guard_key, accepted)
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

DROP TEMPORARY TABLE m4c2b2_manifest_guard;

-- Recompile the manifest check after replacing the projection in this migration connection.
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
    DROP CHECK ck_lease_contract_renewal,
    ADD COLUMN term_months SMALLINT UNSIGNED NULL AFTER renewal_rule,
    ADD COLUMN renewal_notice_lead_days SMALLINT UNSIGNED NULL AFTER term_months,
    ADD COLUMN termination_review_rule
        VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NULL
        AFTER arrear_repayment_rule,
    ADD COLUMN termination_review_after_days SMALLINT UNSIGNED NULL
        AFTER termination_review_rule,
    ADD CONSTRAINT ck_lease_contract_renewal CHECK (
        renewal_rule IN ('openEnded', 'fixedTermAutoRenew')
    ),
    ADD CONSTRAINT ck_lease_contract_lifecycle CHECK (
        (
            renewal_rule = 'openEnded'
            AND term_months IS NULL
            AND renewal_notice_lead_days IS NULL
            AND termination_review_rule IS NULL
            AND termination_review_after_days IS NULL
        )
        OR (
            renewal_rule = 'fixedTermAutoRenew'
            AND term_months BETWEEN 1 AND 1200
            AND renewal_notice_lead_days BETWEEN 1 AND 65535
            AND (
                (
                    offer_kind = 'jeonse'
                    AND termination_review_rule IS NULL
                    AND termination_review_after_days IS NULL
                )
                OR (
                    offer_kind = 'monthlyRent'
                    AND termination_review_rule = 'oldestActiveArrearAge'
                    AND termination_review_after_days BETWEEN 1 AND 65535
                )
            )
        )
    );

-- The 0033 insert trigger still validates listing, ownership, and rent terms. This additional
-- trigger binds only the newly copied lifecycle terms without reopening that established surface.
CREATE TRIGGER tr_lease_contract_lifecycle_insert
BEFORE INSERT ON lease_contract
FOR EACH ROW
FOLLOWS tr_lease_contract_valid_insert
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM real_estate_lease_profile AS profile
        WHERE profile.real_estate_model_version_id = NEW.real_estate_model_version_id
          AND BINARY profile.offer_kind = BINARY NEW.offer_kind
          AND BINARY profile.renewal_rule = BINARY NEW.renewal_rule
          AND profile.term_months <=> NEW.term_months
          AND profile.renewal_notice_lead_days <=> NEW.renewal_notice_lead_days
          AND profile.rent_charge_rule <=> NEW.rent_charge_rule
          AND profile.arrear_repayment_rule <=> NEW.arrear_repayment_rule
          AND profile.termination_review_rule <=> NEW.termination_review_rule
          AND profile.termination_review_after_days
                <=> NEW.termination_review_after_days
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_lease_contract_lifecycle_immutable
BEFORE UPDATE ON lease_contract
FOR EACH ROW
FOLLOWS tr_lease_contract_close_only
SET NEW.id = IF(
    NEW.term_months <=> OLD.term_months
        AND NEW.renewal_notice_lead_days <=> OLD.renewal_notice_lead_days
        AND NEW.termination_review_rule <=> OLD.termination_review_rule
        AND NEW.termination_review_after_days <=> OLD.termination_review_after_days,
    NEW.id,
    NULL
);

CREATE TABLE lease_contract_term (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED NOT NULL,
    lease_contract_id           BIGINT UNSIGNED NOT NULL,
    term_no                     INT UNSIGNED NOT NULL,
    effective_from_game_day     INT UNSIGNED NOT NULL,
    effective_to_game_day       INT UNSIGNED NOT NULL,
    status                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    closed_game_day             INT UNSIGNED NULL,
    termination_reason          VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NULL,
    active_slot                 TINYINT UNSIGNED GENERATED ALWAYS AS (
        CASE WHEN status = 'active' THEN 1 ELSE NULL END
    ) STORED,
    created_at                  DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                  DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_lease_contract_term_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_lease_contract_term_number (lease_contract_id, term_no),
    UNIQUE KEY uk_lease_contract_term_start (lease_contract_id, effective_from_game_day),
    UNIQUE KEY uk_lease_contract_term_active (lease_contract_id, active_slot),
    KEY ix_lease_contract_term_due
        (save_id, run_revision, status, effective_to_game_day, id),
    CONSTRAINT fk_lease_contract_term_contract
        FOREIGN KEY (save_id, run_revision, lease_contract_id)
        REFERENCES lease_contract (save_id, run_revision, id),
    CONSTRAINT ck_lease_contract_term_number CHECK (term_no > 0),
    CONSTRAINT ck_lease_contract_term_period CHECK (
        effective_to_game_day > effective_from_game_day
    ),
    CONSTRAINT ck_lease_contract_term_state CHECK (
        (
            status = 'active'
            AND closed_game_day IS NULL
            AND termination_reason IS NULL
        )
        OR (
            status = 'renewed'
            AND closed_game_day = effective_to_game_day
            AND termination_reason IS NULL
        )
        OR (
            status = 'terminated'
            AND closed_game_day BETWEEN effective_from_game_day AND effective_to_game_day
            AND termination_reason IN ('leaseEnded', 'newRun')
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE lease_lifecycle_action (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED NOT NULL,
    lease_contract_id           BIGINT UNSIGNED NOT NULL,
    lease_contract_term_id      BIGINT UNSIGNED NULL,
    lease_arrear_id             BIGINT UNSIGNED NULL,
    action_kind                 VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    payload_version             TINYINT UNSIGNED NOT NULL,
    phase_rank                  SMALLINT UNSIGNED NOT NULL,
    due_game_day                INT UNSIGNED NOT NULL,
    source_kind                 VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    source_id                   BIGINT UNSIGNED NOT NULL,
    occurrence                  BIGINT UNSIGNED NOT NULL,
    status                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    applied_game_day            INT UNSIGNED NULL,
    cancelled_game_day          INT UNSIGNED NULL,
    cancellation_reason         VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NULL,
    pending_review_slot         TINYINT UNSIGNED GENERATED ALWAYS AS (
        CASE
            WHEN action_kind = 'terminationReview' AND status = 'pending' THEN 1
            ELSE NULL
        END
    ) STORED,
    created_at                  DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                  DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_lease_lifecycle_action_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_lease_lifecycle_action_source
        (save_id, run_revision, source_kind, source_id, action_kind, occurrence),
    UNIQUE KEY uk_lease_lifecycle_action_term_kind (lease_contract_term_id, action_kind),
    UNIQUE KEY uk_lease_lifecycle_action_pending_review
        (lease_contract_id, pending_review_slot),
    KEY ix_lease_lifecycle_action_due
        (save_id, run_revision, status, due_game_day, phase_rank, id),
    KEY ix_lease_lifecycle_action_arrear
        (save_id, run_revision, lease_arrear_id, status),
    CONSTRAINT fk_lease_lifecycle_action_contract
        FOREIGN KEY (save_id, run_revision, lease_contract_id)
        REFERENCES lease_contract (save_id, run_revision, id),
    CONSTRAINT fk_lease_lifecycle_action_term
        FOREIGN KEY (save_id, run_revision, lease_contract_term_id)
        REFERENCES lease_contract_term (save_id, run_revision, id),
    CONSTRAINT fk_lease_lifecycle_action_arrear
        FOREIGN KEY (save_id, run_revision, lease_arrear_id)
        REFERENCES lease_arrear (save_id, run_revision, id),
    CONSTRAINT ck_lease_lifecycle_action_kind CHECK (
        action_kind IN ('renewalNotice', 'termRenewal', 'terminationReview')
    ),
    CONSTRAINT ck_lease_lifecycle_action_payload CHECK (
        payload_version = 1
        AND occurrence BETWEEN 1 AND 9007199254740991
        AND (
            (
                action_kind = 'renewalNotice'
                AND phase_rank = 500
                AND source_kind = 'leaseTerm'
                AND source_id = lease_contract_term_id
                AND occurrence > 0
                AND lease_contract_term_id IS NOT NULL
                AND lease_arrear_id IS NULL
            )
            OR (
                action_kind = 'termRenewal'
                AND phase_rank = 600
                AND source_kind = 'leaseTerm'
                AND source_id = lease_contract_term_id
                AND occurrence > 0
                AND lease_contract_term_id IS NOT NULL
                AND lease_arrear_id IS NULL
            )
            OR (
                action_kind = 'terminationReview'
                AND phase_rank = 700
                AND source_kind = 'leaseArrear'
                AND source_id = lease_arrear_id
                AND occurrence = 1
                AND lease_contract_term_id IS NULL
                AND lease_arrear_id IS NOT NULL
            )
        )
    ),
    CONSTRAINT ck_lease_lifecycle_action_state CHECK (
        (
            status = 'pending'
            AND applied_game_day IS NULL
            AND cancelled_game_day IS NULL
            AND cancellation_reason IS NULL
        )
        OR (
            status = 'applied'
            AND applied_game_day = due_game_day
            AND cancelled_game_day IS NULL
            AND cancellation_reason IS NULL
        )
        OR (
            status = 'cancelled'
            AND applied_game_day IS NULL
            AND cancelled_game_day IS NOT NULL
            AND cancellation_reason IN ('leaseEnded', 'arrearPaid', 'superseded', 'newRun')
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE lease_termination_review (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED NOT NULL,
    household_id                        BIGINT UNSIGNED NOT NULL,
    lease_contract_id                   BIGINT UNSIGNED NOT NULL,
    review_no                           INT UNSIGNED NOT NULL,
    trigger_lease_lifecycle_action_id   BIGINT UNSIGNED NOT NULL,
    trigger_lease_arrear_id             BIGINT UNSIGNED NOT NULL,
    status                              VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    opened_game_day                     INT UNSIGNED NOT NULL,
    resolved_game_day                   INT UNSIGNED NULL,
    resolution_reason                   VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NULL,
    open_slot                           TINYINT UNSIGNED GENERATED ALWAYS AS (
        CASE WHEN status = 'open' THEN 1 ELSE NULL END
    ) STORED,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_lease_termination_review_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_lease_termination_review_number (lease_contract_id, review_no),
    UNIQUE KEY uk_lease_termination_review_action (trigger_lease_lifecycle_action_id),
    UNIQUE KEY uk_lease_termination_review_open (lease_contract_id, open_slot),
    KEY ix_lease_termination_review_current
        (save_id, run_revision, status, opened_game_day, id),
    CONSTRAINT fk_lease_termination_review_household
        FOREIGN KEY (save_id, run_revision, household_id)
        REFERENCES household (save_id, run_revision, id),
    CONSTRAINT fk_lease_termination_review_contract
        FOREIGN KEY (save_id, run_revision, lease_contract_id)
        REFERENCES lease_contract (save_id, run_revision, id),
    CONSTRAINT fk_lease_termination_review_action
        FOREIGN KEY (save_id, run_revision, trigger_lease_lifecycle_action_id)
        REFERENCES lease_lifecycle_action (save_id, run_revision, id),
    CONSTRAINT fk_lease_termination_review_arrear
        FOREIGN KEY (save_id, run_revision, trigger_lease_arrear_id)
        REFERENCES lease_arrear (save_id, run_revision, id),
    CONSTRAINT ck_lease_termination_review_number CHECK (review_no > 0),
    CONSTRAINT ck_lease_termination_review_state CHECK (
        (
            status = 'open'
            AND resolved_game_day IS NULL
            AND resolution_reason IS NULL
        )
        OR (
            status = 'resolved'
            AND resolved_game_day >= opened_game_day
            AND resolution_reason IN ('arrearsCleared', 'leaseEnded', 'newRun')
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_lease_contract_term_valid_insert
BEFORE INSERT ON lease_contract_term
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'active'
        AND NEW.closed_game_day IS NULL
        AND NEW.termination_reason IS NULL
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
              AND contract.effective_to_game_day IS NULL
              AND contract.renewal_rule = 'fixedTermAutoRenew'
              AND contract.term_months IS NOT NULL
              AND contract.renewal_notice_lead_days IS NOT NULL
              AND NEW.term_no > 0
              AND NEW.effective_from_game_day = DATEDIFF(
                  DATE_ADD(
                      DATE_ADD(
                          world.start_date,
                          INTERVAL contract.effective_from_game_day DAY
                      ),
                      INTERVAL (
                          contract.term_months
                          * GREATEST(CAST(NEW.term_no AS SIGNED) - 1, 0)
                      ) MONTH
                  ),
                  world.start_date
              )
              AND NEW.effective_to_game_day = DATEDIFF(
                  DATE_ADD(
                      DATE_ADD(
                          world.start_date,
                          INTERVAL contract.effective_from_game_day DAY
                      ),
                      INTERVAL (contract.term_months * NEW.term_no) MONTH
                  ),
                  world.start_date
              )
              AND NEW.effective_to_game_day > NEW.effective_from_game_day
              AND NEW.effective_to_game_day - contract.renewal_notice_lead_days
                    >= NEW.effective_from_game_day
              AND (
                  (
                      NEW.term_no = 1
                      AND NEW.effective_from_game_day = contract.effective_from_game_day
                      AND NOT EXISTS (
                          SELECT 1
                          FROM lease_contract_term AS existing
                          WHERE existing.lease_contract_id = contract.id
                      )
                  )
                  OR (
                      NEW.term_no > 1
                      AND EXISTS (
                          SELECT 1
                          FROM lease_contract_term AS previous_term
                          WHERE previous_term.lease_contract_id = contract.id
                            AND previous_term.term_no = NEW.term_no - 1
                            AND previous_term.status = 'renewed'
                            AND previous_term.effective_to_game_day
                                  = NEW.effective_from_game_day
                            AND previous_term.closed_game_day
                                  = previous_term.effective_to_game_day
                      )
                  )
              )
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_lease_contract_term_transition_only
BEFORE UPDATE ON lease_contract_term
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'active'
        AND NEW.status IN ('renewed', 'terminated')
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.lease_contract_id = OLD.lease_contract_id
        AND NEW.term_no = OLD.term_no
        AND NEW.effective_from_game_day = OLD.effective_from_game_day
        AND NEW.effective_to_game_day = OLD.effective_to_game_day
        AND NEW.created_at = OLD.created_at
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
              AND (
                  (
                      NEW.status = 'renewed'
                      AND NEW.closed_game_day = OLD.effective_to_game_day
                      AND NEW.termination_reason IS NULL
                      AND OLD.effective_to_game_day = save.game_day + 1
                      AND EXISTS (
                          SELECT 1
                          FROM lease_lifecycle_action AS action
                          WHERE action.lease_contract_term_id = OLD.id
                            AND action.lease_contract_id = OLD.lease_contract_id
                            AND action.action_kind = 'termRenewal'
                            AND action.status = 'pending'
                            AND action.due_game_day = OLD.effective_to_game_day
                      )
                  )
                  OR (
                      NEW.status = 'terminated'
                      AND NEW.closed_game_day = save.game_day
                      AND NEW.closed_game_day BETWEEN OLD.effective_from_game_day
                                                   AND OLD.effective_to_game_day
                      AND NEW.termination_reason IN ('leaseEnded', 'newRun')
                      AND NOT EXISTS (
                          SELECT 1
                          FROM lease_lifecycle_action AS action
                          WHERE action.lease_contract_id = OLD.lease_contract_id
                            AND action.status = 'pending'
                      )
                      AND NOT EXISTS (
                          SELECT 1
                          FROM lease_termination_review AS review
                          WHERE review.lease_contract_id = OLD.lease_contract_id
                            AND review.status = 'open'
                      )
                      AND NOT EXISTS (
                          SELECT 1
                          FROM lease_rent_charge AS charge
                          WHERE charge.lease_contract_id = OLD.lease_contract_id
                            AND charge.status = 'pending'
                      )
                  )
              )
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_lease_contract_term_no_delete
BEFORE DELETE ON lease_contract_term
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'lease contract terms are immutable history';

CREATE TRIGGER tr_lease_lifecycle_action_valid_insert
BEFORE INSERT ON lease_lifecycle_action
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'pending'
        AND NEW.applied_game_day IS NULL
        AND NEW.cancelled_game_day IS NULL
        AND NEW.cancellation_reason IS NULL
        AND EXISTS (
            SELECT 1
            FROM lease_contract AS contract
            INNER JOIN save
                ON save.id = contract.save_id
               AND save.run_revision = contract.run_revision
            WHERE contract.id = NEW.lease_contract_id
              AND contract.save_id = NEW.save_id
              AND contract.run_revision = NEW.run_revision
              AND contract.effective_to_game_day IS NULL
              AND contract.renewal_rule = 'fixedTermAutoRenew'
              AND NEW.due_game_day > save.game_day
              AND (
                  (
                      NEW.action_kind = 'renewalNotice'
                      AND NEW.payload_version = 1
                      AND NEW.phase_rank = 500
                      AND NEW.source_kind = 'leaseTerm'
                      AND NEW.source_id = NEW.lease_contract_term_id
                      AND NEW.lease_arrear_id IS NULL
                      AND EXISTS (
                          SELECT 1
                          FROM lease_contract_term AS term
                          WHERE term.id = NEW.lease_contract_term_id
                            AND term.save_id = NEW.save_id
                            AND term.run_revision = NEW.run_revision
                            AND term.lease_contract_id = contract.id
                            AND term.status = 'active'
                            AND NEW.occurrence = term.term_no
                            AND NEW.due_game_day
                                  = term.effective_to_game_day
                                    - contract.renewal_notice_lead_days
                            AND NEW.due_game_day >= term.effective_from_game_day
                      )
                  )
                  OR (
                      NEW.action_kind = 'termRenewal'
                      AND NEW.payload_version = 1
                      AND NEW.phase_rank = 600
                      AND NEW.source_kind = 'leaseTerm'
                      AND NEW.source_id = NEW.lease_contract_term_id
                      AND NEW.lease_arrear_id IS NULL
                      AND EXISTS (
                          SELECT 1
                          FROM lease_contract_term AS term
                          WHERE term.id = NEW.lease_contract_term_id
                            AND term.save_id = NEW.save_id
                            AND term.run_revision = NEW.run_revision
                            AND term.lease_contract_id = contract.id
                            AND term.status = 'active'
                            AND NEW.occurrence = term.term_no
                            AND NEW.due_game_day = term.effective_to_game_day
                      )
                  )
                  OR (
                      NEW.action_kind = 'terminationReview'
                      AND NEW.payload_version = 1
                      AND NEW.phase_rank = 700
                      AND NEW.source_kind = 'leaseArrear'
                      AND NEW.source_id = NEW.lease_arrear_id
                      AND NEW.occurrence = 1
                      AND NEW.lease_contract_term_id IS NULL
                      AND contract.offer_kind = 'monthlyRent'
                      AND contract.termination_review_rule = 'oldestActiveArrearAge'
                      AND contract.termination_review_after_days IS NOT NULL
                      AND EXISTS (
                          SELECT 1
                          FROM lease_arrear AS arrear
                          WHERE arrear.id = NEW.lease_arrear_id
                            AND arrear.save_id = NEW.save_id
                            AND arrear.run_revision = NEW.run_revision
                            AND arrear.lease_contract_id = contract.id
                            AND arrear.status = 'active'
                            AND NEW.due_game_day = arrear.created_game_day
                                  + contract.termination_review_after_days
                            AND NOT EXISTS (
                                SELECT 1
                                FROM lease_arrear AS older
                                WHERE older.lease_contract_id = contract.id
                                  AND older.status = 'active'
                                  AND (
                                      older.created_game_day < arrear.created_game_day
                                      OR (
                                          older.created_game_day = arrear.created_game_day
                                          AND older.id < arrear.id
                                      )
                                  )
                            )
                      )
                      AND NOT EXISTS (
                          SELECT 1
                          FROM lease_termination_review AS review
                          WHERE review.lease_contract_id = contract.id
                            AND review.status = 'open'
                      )
                  )
              )
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_lease_lifecycle_action_transition_only
BEFORE UPDATE ON lease_lifecycle_action
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'pending'
        AND NEW.status IN ('applied', 'cancelled')
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.lease_contract_id = OLD.lease_contract_id
        AND NEW.lease_contract_term_id <=> OLD.lease_contract_term_id
        AND NEW.lease_arrear_id <=> OLD.lease_arrear_id
        AND BINARY NEW.action_kind = BINARY OLD.action_kind
        AND NEW.payload_version = OLD.payload_version
        AND NEW.phase_rank = OLD.phase_rank
        AND NEW.due_game_day = OLD.due_game_day
        AND BINARY NEW.source_kind = BINARY OLD.source_kind
        AND NEW.source_id = OLD.source_id
        AND NEW.occurrence = OLD.occurrence
        AND NEW.created_at = OLD.created_at
        AND EXISTS (
            SELECT 1
            FROM save
            WHERE save.id = OLD.save_id
              AND save.run_revision = OLD.run_revision
              AND (
                  (
                      NEW.status = 'applied'
                      AND NEW.applied_game_day = OLD.due_game_day
                      AND NEW.cancelled_game_day IS NULL
                      AND NEW.cancellation_reason IS NULL
                      AND OLD.due_game_day = save.game_day + 1
                      AND (
                          (
                              OLD.action_kind = 'renewalNotice'
                              AND EXISTS (
                                  SELECT 1
                                  FROM lease_contract_term AS term
                                  WHERE term.id = OLD.lease_contract_term_id
                                    AND term.lease_contract_id = OLD.lease_contract_id
                                    AND term.status = 'active'
                                    AND term.effective_to_game_day = OLD.due_game_day
                                        + (
                                            SELECT contract.renewal_notice_lead_days
                                            FROM lease_contract AS contract
                                            WHERE contract.id = OLD.lease_contract_id
                                        )
                              )
                          )
                          OR (
                              OLD.action_kind = 'termRenewal'
                              AND EXISTS (
                                  SELECT 1
                                  FROM lease_contract_term AS prior_term
                                  INNER JOIN lease_contract_term AS next_term
                                      ON next_term.lease_contract_id
                                            = prior_term.lease_contract_id
                                     AND next_term.term_no = prior_term.term_no + 1
                                  WHERE prior_term.id = OLD.lease_contract_term_id
                                    AND prior_term.lease_contract_id
                                          = OLD.lease_contract_id
                                    AND prior_term.status = 'renewed'
                                    AND prior_term.closed_game_day = OLD.due_game_day
                                    AND next_term.status = 'active'
                                    AND next_term.effective_from_game_day
                                          = OLD.due_game_day
                              )
                          )
                          OR (
                              OLD.action_kind = 'terminationReview'
                              AND EXISTS (
                                  SELECT 1
                                  FROM lease_termination_review AS review
                                  WHERE review.trigger_lease_lifecycle_action_id = OLD.id
                                    AND review.trigger_lease_arrear_id = OLD.lease_arrear_id
                                    AND review.lease_contract_id = OLD.lease_contract_id
                                    AND review.status = 'open'
                                    AND review.opened_game_day = OLD.due_game_day
                              )
                          )
                      )
                  )
                  OR (
                      NEW.status = 'cancelled'
                      AND NEW.applied_game_day IS NULL
                      AND NEW.cancelled_game_day = save.game_day
                      AND NEW.cancellation_reason
                            IN ('leaseEnded', 'arrearPaid', 'superseded', 'newRun')
                      AND (
                          (
                              NEW.cancellation_reason IN ('leaseEnded', 'newRun')
                              AND EXISTS (
                                  SELECT 1
                                  FROM lease_contract AS contract
                                  WHERE contract.id = OLD.lease_contract_id
                                    AND contract.save_id = OLD.save_id
                                    AND contract.run_revision = OLD.run_revision
                                    AND contract.effective_to_game_day IS NULL
                              )
                          )
                          OR (
                              NEW.cancellation_reason = 'arrearPaid'
                              AND OLD.action_kind = 'terminationReview'
                              AND EXISTS (
                                  SELECT 1
                                  FROM lease_arrear AS arrear
                                  WHERE arrear.id = OLD.lease_arrear_id
                                    AND arrear.status = 'paid'
                              )
                          )
                          OR (
                              NEW.cancellation_reason = 'superseded'
                              AND OLD.action_kind = 'terminationReview'
                              AND (
                                  EXISTS (
                                      SELECT 1
                                      FROM lease_termination_review AS review
                                      WHERE review.lease_contract_id
                                            = OLD.lease_contract_id
                                        AND review.status = 'open'
                                  )
                                  OR EXISTS (
                                      SELECT 1
                                      FROM lease_arrear AS older
                                      INNER JOIN lease_arrear AS basis
                                          ON basis.id = OLD.lease_arrear_id
                                      WHERE older.lease_contract_id
                                                = OLD.lease_contract_id
                                        AND older.status = 'active'
                                        AND (
                                            older.created_game_day
                                                  < basis.created_game_day
                                            OR (
                                                older.created_game_day
                                                      = basis.created_game_day
                                                AND older.id < basis.id
                                            )
                                        )
                                  )
                              )
                          )
                      )
                  )
              )
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_lease_lifecycle_action_no_delete
BEFORE DELETE ON lease_lifecycle_action
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'lease lifecycle actions are immutable history';

CREATE TRIGGER tr_lease_termination_review_valid_insert
BEFORE INSERT ON lease_termination_review
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'open'
        AND NEW.resolved_game_day IS NULL
        AND NEW.resolution_reason IS NULL
        AND NEW.review_no = (
            SELECT COALESCE(MAX(existing.review_no), 0) + 1
            FROM lease_termination_review AS existing
            WHERE existing.lease_contract_id = NEW.lease_contract_id
        )
        AND EXISTS (
            SELECT 1
            FROM lease_contract AS contract
            INNER JOIN save
                ON save.id = contract.save_id
               AND save.run_revision = contract.run_revision
            INNER JOIN lease_lifecycle_action AS action
                ON action.id = NEW.trigger_lease_lifecycle_action_id
               AND action.save_id = contract.save_id
               AND action.run_revision = contract.run_revision
               AND action.lease_contract_id = contract.id
            INNER JOIN lease_arrear AS arrear
                ON arrear.id = NEW.trigger_lease_arrear_id
               AND arrear.save_id = contract.save_id
               AND arrear.run_revision = contract.run_revision
               AND arrear.lease_contract_id = contract.id
            WHERE contract.id = NEW.lease_contract_id
              AND contract.save_id = NEW.save_id
              AND contract.run_revision = NEW.run_revision
              AND contract.household_id = NEW.household_id
              AND contract.effective_to_game_day IS NULL
              AND contract.offer_kind = 'monthlyRent'
              AND contract.termination_review_rule = 'oldestActiveArrearAge'
              AND action.action_kind = 'terminationReview'
              AND action.status = 'pending'
              AND action.lease_arrear_id = arrear.id
              AND action.due_game_day = NEW.opened_game_day
              AND arrear.status = 'active'
              AND NEW.opened_game_day = save.game_day + 1
        )
        AND NOT EXISTS (
            SELECT 1
            FROM lease_termination_review AS existing
            WHERE existing.lease_contract_id = NEW.lease_contract_id
              AND existing.status = 'open'
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_lease_termination_review_transition_only
BEFORE UPDATE ON lease_termination_review
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'open'
        AND NEW.status = 'resolved'
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.household_id = OLD.household_id
        AND NEW.lease_contract_id = OLD.lease_contract_id
        AND NEW.review_no = OLD.review_no
        AND NEW.trigger_lease_lifecycle_action_id
              = OLD.trigger_lease_lifecycle_action_id
        AND NEW.trigger_lease_arrear_id = OLD.trigger_lease_arrear_id
        AND NEW.opened_game_day = OLD.opened_game_day
        AND NEW.created_at = OLD.created_at
        AND EXISTS (
            SELECT 1
            FROM save
            WHERE save.id = OLD.save_id
              AND save.run_revision = OLD.run_revision
              AND NEW.resolved_game_day = save.game_day
              AND (
                  (
                      NEW.resolution_reason = 'arrearsCleared'
                      AND NOT EXISTS (
                          SELECT 1
                          FROM lease_arrear AS arrear
                          WHERE arrear.lease_contract_id = OLD.lease_contract_id
                            AND arrear.status = 'active'
                      )
                  )
                  OR (
                      NEW.resolution_reason IN ('leaseEnded', 'newRun')
                      AND EXISTS (
                          SELECT 1
                          FROM lease_contract AS contract
                          WHERE contract.id = OLD.lease_contract_id
                            AND contract.save_id = OLD.save_id
                            AND contract.run_revision = OLD.run_revision
                            AND contract.effective_to_game_day IS NULL
                      )
                  )
              )
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_lease_termination_review_no_delete
BEFORE DELETE ON lease_termination_review
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'lease termination reviews are immutable history';

-- A new run closes its pending rent charge before the generic settlement cleanup. Preserve the
-- C2b1 charge reconciliation while allowing that cleanup to record the more precise run reason.
DROP TRIGGER tr_scheduled_settlement_lease_rent_transition;

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
            AND NEW.cancellation_reason IN ('leaseEnded', 'newRun')
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
                        AND (
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
                              AND lease_profile.term_months IS NULL
                              AND lease_profile.renewal_notice_lead_days IS NULL
                              AND lease_profile.rent_charge_rule IS NULL
                              AND lease_profile.arrear_repayment_rule IS NULL
                              AND lease_profile.termination_review_rule IS NULL
                              AND lease_profile.termination_review_after_days IS NULL
                        )
                    )
                    OR (
                        JSON_TYPE(JSON_EXTRACT(OLD.parameters, '$.schemaVersion')) = 'INTEGER'
                        AND JSON_UNQUOTE(JSON_EXTRACT(OLD.parameters, '$.schemaVersion')) = '3'
                        AND (
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
                              AND lease_profile.term_months IS NULL
                              AND lease_profile.renewal_notice_lead_days IS NULL
                              AND lease_profile.rent_charge_rule IS NULL
                              AND lease_profile.arrear_repayment_rule IS NULL
                              AND lease_profile.termination_review_rule IS NULL
                              AND lease_profile.termination_review_after_days IS NULL
                        )
                        AND EXISTS (
                            SELECT 1
                            FROM real_estate_lease_profile AS lease_profile
                            WHERE lease_profile.real_estate_model_version_id = OLD.id
                              AND lease_profile.offer_kind = 'monthlyRent'
                              AND lease_profile.renewal_rule = 'openEnded'
                              AND lease_profile.term_months IS NULL
                              AND lease_profile.renewal_notice_lead_days IS NULL
                              AND lease_profile.rent_charge_rule = 'nextMonthStartFull'
                              AND lease_profile.arrear_repayment_rule = 'manualOnly'
                              AND lease_profile.termination_review_rule IS NULL
                              AND lease_profile.termination_review_after_days IS NULL
                        )
                    )
                    OR (
                        JSON_TYPE(JSON_EXTRACT(OLD.parameters, '$.schemaVersion')) = 'INTEGER'
                        AND JSON_UNQUOTE(JSON_EXTRACT(OLD.parameters, '$.schemaVersion')) = '4'
                        AND (
                            SELECT COUNT(*)
                            FROM real_estate_lease_profile AS lease_profile
                            WHERE lease_profile.real_estate_model_version_id = OLD.id
                        ) = 2
                        AND EXISTS (
                            SELECT 1
                            FROM real_estate_lease_profile AS lease_profile
                            WHERE lease_profile.real_estate_model_version_id = OLD.id
                              AND lease_profile.offer_kind = 'jeonse'
                              AND lease_profile.renewal_rule = 'fixedTermAutoRenew'
                              AND lease_profile.term_months = 12
                              AND lease_profile.renewal_notice_lead_days = 30
                              AND lease_profile.rent_charge_rule IS NULL
                              AND lease_profile.arrear_repayment_rule IS NULL
                              AND lease_profile.termination_review_rule IS NULL
                              AND lease_profile.termination_review_after_days IS NULL
                        )
                        AND EXISTS (
                            SELECT 1
                            FROM real_estate_lease_profile AS lease_profile
                            WHERE lease_profile.real_estate_model_version_id = OLD.id
                              AND lease_profile.offer_kind = 'monthlyRent'
                              AND lease_profile.renewal_rule = 'fixedTermAutoRenew'
                              AND lease_profile.term_months = 12
                              AND lease_profile.renewal_notice_lead_days = 30
                              AND lease_profile.rent_charge_rule = 'nextMonthStartFull'
                              AND lease_profile.arrear_repayment_rule = 'manualOnly'
                              AND lease_profile.termination_review_rule
                                    = 'oldestActiveArrearAge'
                              AND lease_profile.termination_review_after_days = 60
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
        'dev-unranked-m4-real-estate-lifecycle-2026-v4',
        'active',
        FALSE,
        JSON_OBJECT(
            'entropyVersion', 'sha256-counter-be-v1',
            'generatorVersion', 'm4-c1-v1',
            'schemaVersion', 4
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
    ON source_model.version_key = 'dev-unranked-m4-real-estate-rent-2026-v3'
INNER JOIN real_estate_region_profile AS source
    ON source.real_estate_model_version_id = source_model.id
WHERE target.version_key = 'dev-unranked-m4-real-estate-lifecycle-2026-v4';

INSERT INTO real_estate_region_property_type
    (real_estate_model_version_id, region_key, property_type, property_type_order)
SELECT target.id, source.region_key, source.property_type, source.property_type_order
FROM real_estate_model_version AS target
INNER JOIN real_estate_model_version AS source_model
    ON source_model.version_key = 'dev-unranked-m4-real-estate-rent-2026-v3'
INNER JOIN real_estate_region_property_type AS source
    ON source.real_estate_model_version_id = source_model.id
WHERE target.version_key = 'dev-unranked-m4-real-estate-lifecycle-2026-v4';

INSERT INTO real_estate_lease_profile
    (
        real_estate_model_version_id, offer_kind, renewal_rule,
        term_months, renewal_notice_lead_days, rent_charge_rule,
        arrear_repayment_rule, termination_review_rule,
        termination_review_after_days
    )
SELECT model.id, fixture.offer_kind, 'fixedTermAutoRenew', 12, 30,
       fixture.rent_charge_rule, fixture.arrear_repayment_rule,
       fixture.termination_review_rule, fixture.termination_review_after_days
FROM real_estate_model_version AS model
INNER JOIN (
    SELECT
        'jeonse' AS offer_kind,
        NULL AS rent_charge_rule,
        NULL AS arrear_repayment_rule,
        NULL AS termination_review_rule,
        NULL AS termination_review_after_days
    UNION ALL
    SELECT
        'monthlyRent', 'nextMonthStartFull', 'manualOnly',
        'oldestActiveArrearAge', 60
) AS fixture
WHERE model.version_key = 'dev-unranked-m4-real-estate-lifecycle-2026-v4';

INSERT INTO real_estate_region_moving_cost
    (real_estate_model_version_id, region_key, moving_cost_krw)
SELECT target.id, source.region_key, source.moving_cost_krw
FROM real_estate_model_version AS target
INNER JOIN real_estate_model_version AS source_model
    ON source_model.version_key = 'dev-unranked-m4-real-estate-rent-2026-v3'
INNER JOIN real_estate_region_moving_cost AS source
    ON source.real_estate_model_version_id = source_model.id
WHERE target.version_key = 'dev-unranked-m4-real-estate-lifecycle-2026-v4';

INSERT INTO real_estate_model_strict_manifest
    (real_estate_model_version_id, canonical_json)
SELECT real_estate_model_version_id, canonical_json
FROM real_estate_model_strict_projection
WHERE real_estate_model_version_id = (
    SELECT id
    FROM real_estate_model_version
    WHERE version_key = 'dev-unranked-m4-real-estate-lifecycle-2026-v4'
);

UPDATE real_estate_model_version AS model
INNER JOIN real_estate_model_strict_manifest AS manifest
    ON manifest.real_estate_model_version_id = model.id
SET model.canonical_sha256 = manifest.canonical_sha256,
    model.sealed_at = CURRENT_TIMESTAMP(3)
WHERE model.version_key = 'dev-unranked-m4-real-estate-lifecycle-2026-v4'
  AND model.sealed_at IS NULL;

UPDATE run_rule_bundle_assignment AS assignment
INNER JOIN real_estate_model_version AS active_real_estate
    ON active_real_estate.version_key
        = 'dev-unranked-m4-real-estate-lifecycle-2026-v4'
   AND active_real_estate.availability = 'active'
   AND active_real_estate.sealed_at IS NOT NULL
SET assignment.real_estate_model_version_id = active_real_estate.id
WHERE assignment.assignment_key = 'newRun';
