-- M4-C2a immutable cash-jeonse terms, atomic lease history, and typed ledger ownership (§5.6).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;

CREATE TABLE real_estate_lease_profile (
    real_estate_model_version_id BIGINT UNSIGNED NOT NULL,
    offer_kind                  VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    renewal_rule                VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                  DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (real_estate_model_version_id, offer_kind),
    CONSTRAINT fk_real_estate_lease_profile_model
        FOREIGN KEY (real_estate_model_version_id) REFERENCES real_estate_model_version (id),
    CONSTRAINT ck_real_estate_lease_profile_kind CHECK (offer_kind = 'jeonse'),
    CONSTRAINT ck_real_estate_lease_profile_renewal CHECK (renewal_rule = 'openEnded')
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE real_estate_region_moving_cost (
    real_estate_model_version_id BIGINT UNSIGNED NOT NULL,
    region_key                  VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    moving_cost_krw             BIGINT NOT NULL,
    created_at                  DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (real_estate_model_version_id, region_key),
    CONSTRAINT fk_real_estate_region_moving_cost_profile
        FOREIGN KEY (real_estate_model_version_id, region_key)
        REFERENCES real_estate_region_profile (real_estate_model_version_id, region_key),
    CONSTRAINT ck_real_estate_region_moving_cost_amount CHECK (
        moving_cost_krw BETWEEN 1 AND 9007199254740991
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_real_estate_lease_profile_draft_insert
BEFORE INSERT ON real_estate_lease_profile
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

CREATE TRIGGER tr_real_estate_lease_profile_no_update
BEFORE UPDATE ON real_estate_lease_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'real-estate lease profiles are immutable';

CREATE TRIGGER tr_real_estate_lease_profile_no_delete
BEFORE DELETE ON real_estate_lease_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'real-estate lease profiles are immutable';

CREATE TRIGGER tr_real_estate_region_moving_cost_draft_insert
BEFORE INSERT ON real_estate_region_moving_cost
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

CREATE TRIGGER tr_real_estate_region_moving_cost_no_update
BEFORE UPDATE ON real_estate_region_moving_cost
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'real-estate moving costs are immutable';

CREATE TRIGGER tr_real_estate_region_moving_cost_no_delete
BEFORE DELETE ON real_estate_region_moving_cost
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'real-estate moving costs are immutable';

CREATE OR REPLACE VIEW real_estate_model_strict_projection AS
SELECT
    model.id AS real_estate_model_version_id,
    CAST(
        CASE
            WHEN EXISTS (
                SELECT 1
                FROM real_estate_lease_profile AS lease_profile
                WHERE lease_profile.real_estate_model_version_id = model.id
            )
            THEN JSON_MERGE_PATCH(
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
                                         AND BINARY allowed.region_key
                                               = BINARY profile.region_key
                                       ORDER BY allowed.property_type_order
                                       LIMIT 1
                                   ), JSON_ARRAY()),
                                   'annualGrossRentYieldPpm',
                                       profile.annual_gross_rent_yield_ppm,
                                   'availabilityRule', profile.availability_rule,
                                   'basePricePerSquareMeterKrw',
                                       profile.base_price_per_square_meter_krw,
                                   'jeonseRatioPpm', profile.jeonse_ratio_ppm,
                                   'maximumExclusiveAreaSquareMeters',
                                       profile.maximum_exclusive_area_square_meters,
                                   'maximumIndexPpm', profile.maximum_index_ppm,
                                   'maximumPriceVariationPpm',
                                       profile.maximum_price_variation_ppm,
                                   'minimumExclusiveAreaSquareMeters',
                                       profile.minimum_exclusive_area_square_meters,
                                   'minimumIndexPpm', profile.minimum_index_ppm,
                                   'minimumPriceVariationPpm',
                                       profile.minimum_price_variation_ppm,
                                   'monthlyDepositRatioPpm', profile.monthly_deposit_ratio_ppm,
                                   'monthlyListingSlotCount',
                                       profile.monthly_listing_slot_count,
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
                ),
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
                        WHERE lease_profile.real_estate_model_version_id = model.id
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
                        WHERE moving_cost.real_estate_model_version_id = model.id
                        ORDER BY region.region_order
                        LIMIT 1
                    ), JSON_ARRAY()),
                    'schemaVersion', 2
                )
            )
            ELSE JSON_OBJECT(
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
                               'annualGrossRentYieldPpm',
                                   profile.annual_gross_rent_yield_ppm,
                               'availabilityRule', profile.availability_rule,
                               'basePricePerSquareMeterKrw',
                                   profile.base_price_per_square_meter_krw,
                               'jeonseRatioPpm', profile.jeonse_ratio_ppm,
                               'maximumExclusiveAreaSquareMeters',
                                   profile.maximum_exclusive_area_square_meters,
                               'maximumIndexPpm', profile.maximum_index_ppm,
                               'maximumPriceVariationPpm',
                                   profile.maximum_price_variation_ppm,
                               'minimumExclusiveAreaSquareMeters',
                                   profile.minimum_exclusive_area_square_meters,
                               'minimumIndexPpm', profile.minimum_index_ppm,
                               'minimumPriceVariationPpm',
                                   profile.minimum_price_variation_ppm,
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
            )
        END AS CHAR CHARACTER SET utf8mb4
    ) AS canonical_json
FROM real_estate_model_version AS model
WHERE model.availability = 'active';

-- MySQL can keep the previous view plan inside a trigger for the rest of the
-- connection after CREATE OR REPLACE VIEW. Recreate the trigger so the v2
-- manifest is checked against the lease-aware projection above.
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

CREATE TABLE lease_contract (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    household_id                    BIGINT UNSIGNED NOT NULL,
    real_estate_model_version_id    BIGINT UNSIGNED NOT NULL,
    property_listing_id             BIGINT UNSIGNED NOT NULL,
    command_id                      CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    role                            VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    region_key                      VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    property_type                   VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    exclusive_area_square_meters    SMALLINT UNSIGNED NOT NULL,
    offer_kind                      VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    deposit_krw                     BIGINT NOT NULL,
    monthly_rent_krw                BIGINT NULL,
    renewal_rule                    VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    effective_from_game_day         INT UNSIGNED NOT NULL,
    effective_to_game_day           INT UNSIGNED NULL,
    active_tenant_lease_slot        TINYINT UNSIGNED GENERATED ALWAYS AS (
        CASE
            WHEN role = 'tenant' AND effective_to_game_day IS NULL THEN 1
            ELSE NULL
        END
    ) STORED,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_lease_contract_active_tenant
        (household_id, active_tenant_lease_slot),
    UNIQUE KEY uk_lease_contract_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_lease_contract_command (save_id, command_id),
    KEY ix_lease_contract_history (household_id, effective_from_game_day, id),
    KEY ix_lease_contract_model_listing
        (real_estate_model_version_id, property_listing_id),
    CONSTRAINT fk_lease_contract_household
        FOREIGN KEY (save_id, run_revision, household_id)
        REFERENCES household (save_id, run_revision, id),
    CONSTRAINT fk_lease_contract_model
        FOREIGN KEY (real_estate_model_version_id) REFERENCES real_estate_model_version (id),
    CONSTRAINT fk_lease_contract_listing
        FOREIGN KEY (property_listing_id) REFERENCES property_listing (id),
    CONSTRAINT fk_lease_contract_command
        FOREIGN KEY (save_id, command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT fk_lease_contract_region
        FOREIGN KEY (region_key) REFERENCES life_region (region_key),
    CONSTRAINT ck_lease_contract_command CHECK (
        command_id REGEXP '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
    ),
    CONSTRAINT ck_lease_contract_role CHECK (role = 'tenant'),
    CONSTRAINT ck_lease_contract_property_type CHECK (
        property_type IN ('apartment', 'multiFamily', 'detached')
    ),
    CONSTRAINT ck_lease_contract_area CHECK (
        exclusive_area_square_meters BETWEEN 1 AND 10000
    ),
    CONSTRAINT ck_lease_contract_offer CHECK (
        offer_kind = 'jeonse'
        AND deposit_krw BETWEEN 1 AND 9007199254740991
        AND monthly_rent_krw IS NULL
    ),
    CONSTRAINT ck_lease_contract_renewal CHECK (renewal_rule = 'openEnded'),
    CONSTRAINT ck_lease_contract_period CHECK (
        effective_to_game_day IS NULL
        OR effective_to_game_day > effective_from_game_day
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

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
              AND offer.monthly_rent_krw IS NULL
        ),
    NEW.save_id,
    NULL
);

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

CREATE TRIGGER tr_lease_contract_no_delete
BEFORE DELETE ON lease_contract
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'lease contract history is immutable';

ALTER TABLE residence
    ADD COLUMN lease_contract_id BIGINT UNSIGNED NULL AFTER tenure_type,
    ADD UNIQUE KEY uk_residence_lease_contract
        (save_id, run_revision, lease_contract_id),
    ADD CONSTRAINT fk_residence_lease_contract
        FOREIGN KEY (save_id, run_revision, lease_contract_id)
        REFERENCES lease_contract (save_id, run_revision, id),
    ADD CONSTRAINT ck_residence_lease_shape CHECK (
        (tenure_type = 'jeonse' AND lease_contract_id IS NOT NULL)
        OR (tenure_type <> 'jeonse' AND lease_contract_id IS NULL)
    );

CREATE TRIGGER tr_residence_lease_valid_insert
BEFORE INSERT ON residence
FOR EACH ROW
SET NEW.save_id = IF(
    (
        NEW.lease_contract_id IS NULL
        AND NEW.tenure_type <> 'jeonse'
    )
    OR (
        NEW.lease_contract_id IS NOT NULL
        AND NEW.tenure_type = 'jeonse'
        AND EXISTS (
            SELECT 1
            FROM lease_contract AS contract
            WHERE contract.id = NEW.lease_contract_id
              AND contract.save_id = NEW.save_id
              AND contract.run_revision = NEW.run_revision
              AND contract.household_id = NEW.household_id
              AND contract.role = 'tenant'
              AND contract.offer_kind = 'jeonse'
              AND BINARY contract.region_key = BINARY NEW.region_key
              AND contract.effective_from_game_day = NEW.effective_from_game_day
              AND contract.effective_to_game_day IS NULL
        )
    ),
    NEW.save_id,
    NULL
);

DROP TRIGGER tr_residence_transition_only;

CREATE TRIGGER tr_residence_transition_only
BEFORE UPDATE ON residence
FOR EACH ROW
SET NEW.id = IF(
    OLD.effective_to_game_day IS NULL
        AND NEW.effective_to_game_day IS NOT NULL
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.household_id = OLD.household_id
        AND BINARY NEW.region_key = BINARY OLD.region_key
        AND BINARY NEW.tenure_type = BINARY OLD.tenure_type
        AND NEW.lease_contract_id <=> OLD.lease_contract_id
        AND NEW.effective_from_game_day = OLD.effective_from_game_day
        AND NEW.created_at = OLD.created_at,
    OLD.id,
    NULL
);

ALTER TABLE ledger_transaction
    ADD CONSTRAINT ck_ledger_transaction_lease_source CHECK (
        source_kind NOT LIKE 'lease%'
        OR source_kind = 'leaseMove'
    );

ALTER TABLE ledger_posting
    DROP CHECK ck_ledger_posting_account_code,
    DROP CHECK ck_ledger_posting_account_reference,
    ADD COLUMN lease_contract_id BIGINT UNSIGNED NULL AFTER tax_obligation_id,
    ADD KEY ix_ledger_posting_lease_contract
        (save_id, run_revision, lease_contract_id),
    ADD CONSTRAINT fk_ledger_posting_lease_contract
        FOREIGN KEY (save_id, run_revision, lease_contract_id)
        REFERENCES lease_contract (save_id, run_revision, id),
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
            'leaseDepositAsset', 'movingExpense'
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
                'leaseDepositAsset'
            )
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NULL
            AND living_cost_month_id IS NULL
            AND essential_arrear_id IS NULL
            AND loan_contract_id IS NULL
            AND tax_obligation_id IS NULL
            AND lease_contract_id IS NULL
        )
    );

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
    OR NEW.source_kind <> 'leaseMove',
    NEW.source_kind,
    NULL
);

CREATE TRIGGER tr_ledger_posting_lease_reference_insert
BEFORE INSERT ON ledger_posting
FOR EACH ROW
FOLLOWS tr_ledger_posting_loan_reference_insert
SET NEW.account_code = IF(
    (
        NEW.account_code = 'leaseDepositAsset'
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
        'dev-unranked-m4-real-estate-lease-2026-v2',
        'active',
        FALSE,
        JSON_OBJECT(
            'entropyVersion', 'sha256-counter-be-v1',
            'generatorVersion', 'm4-c1-v1',
            'schemaVersion', 2
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
    ON source_model.version_key = 'dev-unranked-m4-real-estate-2026-v1'
INNER JOIN real_estate_region_profile AS source
    ON source.real_estate_model_version_id = source_model.id
WHERE target.version_key = 'dev-unranked-m4-real-estate-lease-2026-v2';

INSERT INTO real_estate_region_property_type
    (real_estate_model_version_id, region_key, property_type, property_type_order)
SELECT target.id, source.region_key, source.property_type, source.property_type_order
FROM real_estate_model_version AS target
INNER JOIN real_estate_model_version AS source_model
    ON source_model.version_key = 'dev-unranked-m4-real-estate-2026-v1'
INNER JOIN real_estate_region_property_type AS source
    ON source.real_estate_model_version_id = source_model.id
WHERE target.version_key = 'dev-unranked-m4-real-estate-lease-2026-v2';

INSERT INTO real_estate_lease_profile
    (real_estate_model_version_id, offer_kind, renewal_rule)
SELECT model.id, 'jeonse', 'openEnded'
FROM real_estate_model_version AS model
WHERE model.version_key = 'dev-unranked-m4-real-estate-lease-2026-v2';

INSERT INTO real_estate_region_moving_cost
    (real_estate_model_version_id, region_key, moving_cost_krw)
SELECT model.id, fixture.region_key, fixture.moving_cost_krw
FROM real_estate_model_version AS model
INNER JOIN (
    SELECT 'capitalArea' AS region_key, 800000 AS moving_cost_krw
    UNION ALL SELECT 'metropolitan', 600000
    UNION ALL SELECT 'smallCity', 450000
    UNION ALL SELECT 'rural', 300000
) AS fixture
WHERE model.version_key = 'dev-unranked-m4-real-estate-lease-2026-v2';

INSERT INTO real_estate_model_strict_manifest
    (real_estate_model_version_id, canonical_json)
SELECT real_estate_model_version_id, canonical_json
FROM real_estate_model_strict_projection
WHERE real_estate_model_version_id = (
    SELECT id
    FROM real_estate_model_version
    WHERE version_key = 'dev-unranked-m4-real-estate-lease-2026-v2'
);

UPDATE real_estate_model_version AS model
INNER JOIN real_estate_model_strict_manifest AS manifest
    ON manifest.real_estate_model_version_id = model.id
SET model.canonical_sha256 = manifest.canonical_sha256,
    model.sealed_at = CURRENT_TIMESTAMP(3)
WHERE model.version_key = 'dev-unranked-m4-real-estate-lease-2026-v2'
  AND model.sealed_at IS NULL;

UPDATE run_rule_bundle_assignment AS assignment
INNER JOIN real_estate_model_version AS active_real_estate
    ON active_real_estate.version_key = 'dev-unranked-m4-real-estate-lease-2026-v2'
   AND active_real_estate.availability = 'active'
   AND active_real_estate.sealed_at IS NOT NULL
SET assignment.real_estate_model_version_id = active_real_estate.id
WHERE assignment.assignment_key = 'newRun';
