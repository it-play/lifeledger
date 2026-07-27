-- M4-C2c publishes lease-deposit lending without mutating prior credit or housing graphs (§5 C2c).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;

CREATE TEMPORARY TABLE m4c2c_preflight_guard (
    guard_key VARCHAR(48) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    accepted  TINYINT UNSIGNED NOT NULL,
    CONSTRAINT ck_m4c2c_preflight_guard CHECK (accepted = 1)
);

INSERT INTO m4c2c_preflight_guard (guard_key, accepted)
SELECT 'existing-credit-manifests', IF(
    NOT EXISTS (
        SELECT 1
        FROM credit_model_strict_manifest AS manifest
        INNER JOIN credit_model_version AS model
            ON model.id = manifest.credit_model_version_id
        LEFT JOIN credit_model_strict_projection AS projection
            ON projection.credit_model_version_id = manifest.credit_model_version_id
        WHERE model.version_key IN (
                  'dev-unranked-m4b-credit-2026-v1',
                  'dev-unranked-m4b-credit-2026-v2'
              )
          AND (
              projection.credit_model_version_id IS NULL
              OR BINARY manifest.canonical_json <> BINARY projection.canonical_json
              OR BINARY manifest.canonical_sha256 <> BINARY model.canonical_sha256
          )
    )
        AND (
            SELECT COUNT(*)
            FROM credit_model_version
            WHERE version_key IN (
                      'dev-unranked-m4b-credit-2026-v1',
                      'dev-unranked-m4b-credit-2026-v2'
                  )
              AND sealed_at IS NOT NULL
        ) = 2,
    1,
    0
);

INSERT INTO m4c2c_preflight_guard (guard_key, accepted)
SELECT 'existing-real-estate-manifests', IF(
    NOT EXISTS (
        SELECT 1
        FROM real_estate_model_strict_manifest AS manifest
        INNER JOIN real_estate_model_version AS model
            ON model.id = manifest.real_estate_model_version_id
        LEFT JOIN real_estate_model_strict_projection AS projection
            ON projection.real_estate_model_version_id = manifest.real_estate_model_version_id
        WHERE model.version_key IN (
                  'dev-unranked-m4-real-estate-2026-v1',
                  'dev-unranked-m4-real-estate-lease-2026-v2',
                  'dev-unranked-m4-real-estate-rent-2026-v3',
                  'dev-unranked-m4-real-estate-lifecycle-2026-v4'
              )
          AND (
              projection.real_estate_model_version_id IS NULL
              OR BINARY manifest.canonical_json <> BINARY projection.canonical_json
              OR BINARY manifest.canonical_sha256 <> BINARY model.canonical_sha256
          )
    )
        AND (
            SELECT COUNT(*)
            FROM real_estate_model_version
            WHERE version_key IN (
                      'dev-unranked-m4-real-estate-2026-v1',
                      'dev-unranked-m4-real-estate-lease-2026-v2',
                      'dev-unranked-m4-real-estate-rent-2026-v3',
                      'dev-unranked-m4-real-estate-lifecycle-2026-v4'
                  )
              AND sealed_at IS NOT NULL
        ) = 4,
    1,
    0
);

DROP TEMPORARY TABLE m4c2c_preflight_guard;

ALTER TABLE loan_product_version
    DROP CHECK ck_loan_product_collateral,
    DROP CHECK ck_loan_product_servicing_shape,
    ADD COLUMN execution_channel
        VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NULL
        AFTER collateral_rule,
    ADD COLUMN funding_limit_ppm INT UNSIGNED NULL AFTER execution_channel,
    ADD COLUMN affordability_rule
        VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NULL
        AFTER funding_limit_ppm,
    ADD COLUMN affordability_limit_ppm INT UNSIGNED NULL AFTER affordability_rule,
    ADD COLUMN regulatory_dsr_treatment
        VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NULL
        AFTER affordability_limit_ppm,
    ADD CONSTRAINT ck_loan_product_collateral CHECK (
        collateral_rule IN (
            'none', 'valuationUnavailable', 'leaseDepositFundingLimit', 'notApplicable'
        )
    ),
    ADD CONSTRAINT ck_loan_product_c2c_shape CHECK (
        (
            product_kind <> 'leaseDepositLoan'
            AND execution_channel IS NULL
            AND funding_limit_ppm IS NULL
            AND affordability_rule IS NULL
            AND affordability_limit_ppm IS NULL
            AND regulatory_dsr_treatment IS NULL
        )
        OR (
            product_kind = 'leaseDepositLoan'
            AND execution_channel = 'leaseMove'
            AND funding_limit_ppm BETWEEN 1 AND 1000000
            AND affordability_rule = 'interestOnly'
            AND affordability_limit_ppm BETWEEN 1 AND 1000000
            AND regulatory_dsr_treatment = 'excludedNoOwnedHome'
            AND collateral_rule = 'leaseDepositFundingLimit'
        )
    ),
    ADD CONSTRAINT ck_loan_product_servicing_shape CHECK (
        (
            catalog_scope = 'modelChild'
            AND credit_model_version_id IS NOT NULL
            AND product_kind IN ('studentLoan', 'unsecuredLoan')
            AND lender_sector IN ('bank', 'nonBank')
            AND day_count_rule = 'actual365'
            AND term_months > 0
            AND payment_calendar = 'monthEnd'
            AND grace_months = 0
            AND minimum_principal_krw > 0
            AND maximum_principal_krw >= minimum_principal_krw
            AND prepayment_fee_ppm IS NOT NULL
            AND prepayment_effect <> 'forbidden'
            AND starting_eligible = TRUE
            AND prepayment_allowed = TRUE
            AND dsr_included = TRUE
            AND read_only = FALSE
            AND provenance_kind = 'GAME_BALANCE'
        )
        OR (
            catalog_scope = 'modelChild'
            AND credit_model_version_id IS NOT NULL
            AND product_kind = 'leaseDepositLoan'
            AND lender_sector = 'bank'
            AND rate_status = 'available'
            AND rate_type = 'fixed'
            AND day_count_rule = 'actual365'
            AND repayment_method = 'bullet'
            AND term_months = 24
            AND payment_calendar = 'monthEnd'
            AND grace_months = 0
            AND minimum_principal_krw > 0
            AND maximum_principal_krw >= minimum_principal_krw
            AND prepayment_fee_ppm = 0
            AND prepayment_effect = 'reduceTerm'
            AND starting_eligible = FALSE
            AND quote_eligible = TRUE
            AND execution_eligible = TRUE
            AND prepayment_allowed = TRUE
            AND dsr_included = FALSE
            AND read_only = FALSE
            AND provenance_kind = 'GAME_BALANCE'
        )
        OR (
            catalog_scope = 'bridgeOnly'
            AND credit_model_version_id IS NULL
            AND product_key = 'compat-legacy-debt-zero-bullet-v1'
            AND product_kind = 'legacyDebt'
            AND lender_sector = 'bridgeOnly'
            AND rate_type = 'unavailable'
            AND day_count_rule = 'unavailable'
            AND repayment_method = 'bullet'
            AND term_months IS NULL
            AND payment_calendar = 'none'
            AND grace_months IS NULL
            AND minimum_principal_krw IS NULL
            AND maximum_principal_krw IS NULL
            AND prepayment_fee_ppm IS NULL
            AND prepayment_effect = 'forbidden'
            AND collateral_rule = 'notApplicable'
            AND starting_eligible = FALSE
            AND quote_eligible = FALSE
            AND execution_eligible = FALSE
            AND prepayment_allowed = FALSE
            AND dsr_included = FALSE
            AND read_only = TRUE
            AND provenance_kind = 'COMPATIBILITY'
        )
    );

CREATE OR REPLACE VIEW loan_product_canonical_projection AS
SELECT
    product.id AS loan_product_version_id,
    CAST(
        CASE
            WHEN product.product_kind = 'leaseDepositLoan' THEN JSON_OBJECT(
                'affordabilityLimitPpm', product.affordability_limit_ppm,
                'affordabilityRule', product.affordability_rule,
                'catalogScope', product.catalog_scope,
                'collateralRule', product.collateral_rule,
                'creditModelVersionId', CAST(product.credit_model_version_id AS CHAR),
                'dayCountRule', product.day_count_rule,
                'displayName', product.display_name,
                'displayOrder', product.display_order,
                'dsrIncluded', product.dsr_included,
                'executionChannel', product.execution_channel,
                'executionEligible', product.execution_eligible,
                'fixedAnnualRateBp', product.fixed_annual_rate_bp,
                'fundingLimitPpm', product.funding_limit_ppm,
                'graceMonths', product.grace_months,
                'lenderSector', product.lender_sector,
                'maximumAnnualRateBp', product.maximum_annual_rate_bp,
                'maximumPrincipalKrw', product.maximum_principal_krw,
                'minimumAnnualRateBp', product.minimum_annual_rate_bp,
                'minimumPrincipalKrw', product.minimum_principal_krw,
                'paymentCalendar', product.payment_calendar,
                'prepaymentAllowed', product.prepayment_allowed,
                'prepaymentEffect', product.prepayment_effect,
                'prepaymentFeePpm', product.prepayment_fee_ppm,
                'productKey', product.product_key,
                'productKind', product.product_kind,
                'provenanceKind', product.provenance_kind,
                'quoteEligible', product.quote_eligible,
                'rateResetRule', product.rate_reset_rule,
                'rateStatus', product.rate_status,
                'rateType', product.rate_type,
                'readOnly', product.read_only,
                'referenceRateKey', product.reference_rate_key,
                'regulatoryDsrTreatment', product.regulatory_dsr_treatment,
                'repaymentMethod', product.repayment_method,
                'schemaVersion', 2,
                'spreadBp', product.spread_bp,
                'startingEligible', product.starting_eligible,
                'termMonths', product.term_months
            )
            ELSE JSON_OBJECT(
                'catalogScope', product.catalog_scope,
                'collateralRule', product.collateral_rule,
                'creditModelVersionId', IF(
                    product.credit_model_version_id IS NULL,
                    NULL,
                    CAST(product.credit_model_version_id AS CHAR)
                ),
                'dayCountRule', product.day_count_rule,
                'displayName', product.display_name,
                'displayOrder', product.display_order,
                'dsrIncluded', product.dsr_included,
                'executionEligible', product.execution_eligible,
                'fixedAnnualRateBp', product.fixed_annual_rate_bp,
                'graceMonths', product.grace_months,
                'lenderSector', product.lender_sector,
                'maximumAnnualRateBp', product.maximum_annual_rate_bp,
                'maximumPrincipalKrw', product.maximum_principal_krw,
                'minimumAnnualRateBp', product.minimum_annual_rate_bp,
                'minimumPrincipalKrw', product.minimum_principal_krw,
                'paymentCalendar', product.payment_calendar,
                'prepaymentAllowed', product.prepayment_allowed,
                'prepaymentEffect', product.prepayment_effect,
                'prepaymentFeePpm', product.prepayment_fee_ppm,
                'productKey', product.product_key,
                'productKind', product.product_kind,
                'provenanceKind', product.provenance_kind,
                'quoteEligible', product.quote_eligible,
                'rateResetRule', product.rate_reset_rule,
                'rateStatus', product.rate_status,
                'rateType', product.rate_type,
                'readOnly', product.read_only,
                'referenceRateKey', product.reference_rate_key,
                'repaymentMethod', product.repayment_method,
                'schemaVersion', 1,
                'spreadBp', product.spread_bp,
                'startingEligible', product.starting_eligible,
                'termMonths', product.term_months
            )
        END
        AS CHAR CHARACTER SET utf8mb4
    ) AS canonical_json
FROM loan_product_version AS product;

CREATE TEMPORARY TABLE m4c2c_manifest_guard (
    guard_key VARCHAR(48) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    accepted  TINYINT UNSIGNED NOT NULL,
    CONSTRAINT ck_m4c2c_manifest_guard CHECK (accepted = 1)
);

INSERT INTO m4c2c_manifest_guard (guard_key, accepted)
SELECT 'old-product-bytes', IF(
    NOT EXISTS (
        SELECT 1
        FROM loan_product_canonical_manifest AS manifest
        INNER JOIN loan_product_version AS product
            ON product.id = manifest.loan_product_version_id
        LEFT JOIN loan_product_canonical_projection AS projection
            ON projection.loan_product_version_id = manifest.loan_product_version_id
        WHERE product.product_kind <> 'leaseDepositLoan'
          AND (
              projection.loan_product_version_id IS NULL
              OR BINARY manifest.canonical_json <> BINARY projection.canonical_json
              OR BINARY manifest.canonical_sha256 <> BINARY product.canonical_sha256
          )
    ),
    1,
    0
);

DROP TEMPORARY TABLE m4c2c_manifest_guard;

DROP TRIGGER tr_loan_product_manifest_draft_insert;

CREATE TRIGGER tr_loan_product_manifest_draft_insert
BEFORE INSERT ON loan_product_canonical_manifest
FOR EACH ROW
SET NEW.loan_product_version_id = IF(
    JSON_VALID(NEW.canonical_json)
        AND EXISTS (
            SELECT 1
            FROM loan_product_version AS product
            INNER JOIN loan_product_canonical_projection AS projection
                ON projection.loan_product_version_id = product.id
            WHERE product.id = NEW.loan_product_version_id
              AND product.sealed_at IS NULL
              AND BINARY projection.canonical_json = BINARY NEW.canonical_json
        ),
    NEW.loan_product_version_id,
    NULL
);

DROP TRIGGER tr_loan_product_seal_only;

CREATE TRIGGER tr_loan_product_seal_only
BEFORE UPDATE ON loan_product_version
FOR EACH ROW
SET NEW.product_key = IF(
    OLD.sealed_at IS NULL
        AND NEW.sealed_at IS NOT NULL
        AND NEW.id = OLD.id
        AND NEW.credit_model_version_id <=> OLD.credit_model_version_id
        AND BINARY NEW.product_key = BINARY OLD.product_key
        AND NEW.display_name = OLD.display_name
        AND BINARY NEW.catalog_scope = BINARY OLD.catalog_scope
        AND BINARY NEW.product_kind = BINARY OLD.product_kind
        AND BINARY NEW.lender_sector = BINARY OLD.lender_sector
        AND BINARY NEW.rate_status = BINARY OLD.rate_status
        AND BINARY NEW.rate_type = BINARY OLD.rate_type
        AND BINARY NEW.reference_rate_key <=> BINARY OLD.reference_rate_key
        AND NEW.fixed_annual_rate_bp <=> OLD.fixed_annual_rate_bp
        AND NEW.spread_bp <=> OLD.spread_bp
        AND NEW.minimum_annual_rate_bp <=> OLD.minimum_annual_rate_bp
        AND NEW.maximum_annual_rate_bp <=> OLD.maximum_annual_rate_bp
        AND BINARY NEW.rate_reset_rule = BINARY OLD.rate_reset_rule
        AND BINARY NEW.day_count_rule = BINARY OLD.day_count_rule
        AND BINARY NEW.repayment_method = BINARY OLD.repayment_method
        AND NEW.term_months <=> OLD.term_months
        AND BINARY NEW.payment_calendar = BINARY OLD.payment_calendar
        AND NEW.grace_months <=> OLD.grace_months
        AND NEW.minimum_principal_krw <=> OLD.minimum_principal_krw
        AND NEW.maximum_principal_krw <=> OLD.maximum_principal_krw
        AND NEW.prepayment_fee_ppm <=> OLD.prepayment_fee_ppm
        AND BINARY NEW.prepayment_effect = BINARY OLD.prepayment_effect
        AND BINARY NEW.collateral_rule = BINARY OLD.collateral_rule
        AND BINARY NEW.execution_channel <=> BINARY OLD.execution_channel
        AND NEW.funding_limit_ppm <=> OLD.funding_limit_ppm
        AND BINARY NEW.affordability_rule <=> BINARY OLD.affordability_rule
        AND NEW.affordability_limit_ppm <=> OLD.affordability_limit_ppm
        AND BINARY NEW.regulatory_dsr_treatment <=> BINARY OLD.regulatory_dsr_treatment
        AND NEW.starting_eligible = OLD.starting_eligible
        AND NEW.quote_eligible = OLD.quote_eligible
        AND NEW.execution_eligible = OLD.execution_eligible
        AND NEW.prepayment_allowed = OLD.prepayment_allowed
        AND NEW.dsr_included = OLD.dsr_included
        AND NEW.read_only = OLD.read_only
        AND BINARY NEW.provenance_kind = BINARY OLD.provenance_kind
        AND NEW.display_order = OLD.display_order
        AND NEW.created_at = OLD.created_at
        AND EXISTS (
            SELECT 1
            FROM loan_product_canonical_manifest AS manifest
            INNER JOIN loan_product_canonical_projection AS projection
                ON projection.loan_product_version_id = manifest.loan_product_version_id
            WHERE manifest.loan_product_version_id = OLD.id
              AND BINARY manifest.canonical_sha256 = BINARY NEW.canonical_sha256
              AND BINARY manifest.canonical_json = BINARY projection.canonical_json
        ),
    OLD.product_key,
    NULL
);

DROP TRIGGER tr_credit_model_version_seal_only;

CREATE TRIGGER tr_credit_model_version_seal_only
BEFORE UPDATE ON credit_model_version
FOR EACH ROW
SET NEW.version_key = IF(
    OLD.sealed_at IS NULL
        AND NEW.sealed_at IS NOT NULL
        AND NEW.id = OLD.id
        AND BINARY NEW.version_key = BINARY OLD.version_key
        AND BINARY NEW.availability = BINARY OLD.availability
        AND NEW.ranked_eligible = OLD.ranked_eligible
        AND NEW.credit_policy_set_id <=> OLD.credit_policy_set_id
        AND NEW.parameters = OLD.parameters
        AND NEW.created_at = OLD.created_at
        AND NEW.canonical_sha256 REGEXP '^[0-9a-f]{64}$'
        AND (
            (
                OLD.availability = 'active'
                AND EXISTS (
                    SELECT 1 FROM policy_set AS policy
                    WHERE policy.id = OLD.credit_policy_set_id
                      AND policy.sealed_at IS NOT NULL
                      AND policy.ranked_eligible = FALSE
                )
                AND (
                    (
                        BINARY OLD.version_key
                            = BINARY 'dev-unranked-m4c2c-credit-2026-v3'
                        AND (SELECT COUNT(*) FROM loan_product_version AS product
                             WHERE product.credit_model_version_id = OLD.id
                               AND product.catalog_scope = 'modelChild'
                               AND product.sealed_at IS NOT NULL) = 3
                        AND (SELECT COUNT(*) FROM loan_product_version AS product
                             WHERE product.credit_model_version_id = OLD.id
                               AND product.product_kind = 'leaseDepositLoan'
                               AND product.sealed_at IS NOT NULL) = 1
                    )
                    OR (
                        BINARY OLD.version_key
                            <> BINARY 'dev-unranked-m4c2c-credit-2026-v3'
                        AND (SELECT COUNT(*) FROM loan_product_version AS product
                             WHERE product.credit_model_version_id = OLD.id
                               AND product.catalog_scope = 'modelChild'
                               AND product.sealed_at IS NOT NULL) = 2
                        AND (SELECT COUNT(*) FROM loan_product_version AS product
                             WHERE product.credit_model_version_id = OLD.id
                               AND product.product_kind = 'leaseDepositLoan') = 0
                    )
                )
                AND (SELECT COUNT(*) FROM loan_product_legacy_start_mapping AS mapping
                     WHERE mapping.credit_model_version_id = OLD.id) = 2
                AND EXISTS (
                    SELECT 1
                    FROM credit_model_strict_manifest AS manifest
                    INNER JOIN credit_model_strict_projection AS projection
                        ON projection.credit_model_version_id
                            = manifest.credit_model_version_id
                    WHERE manifest.credit_model_version_id = OLD.id
                      AND BINARY manifest.canonical_sha256 = BINARY NEW.canonical_sha256
                      AND BINARY manifest.canonical_json = BINARY projection.canonical_json
                )
            )
            OR (
                OLD.availability = 'disabled'
                AND OLD.credit_policy_set_id IS NULL
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

DROP TRIGGER tr_loan_quote_valid_insert;

ALTER TABLE loan_quote
    DROP CHECK ck_loan_quote_amounts,
    DROP CHECK ck_loan_quote_dsr_shape,
    DROP CHECK ck_loan_quote_decision,
    ADD COLUMN purpose
        VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL DEFAULT 'unsecured'
        AFTER loan_product_version_id,
    ADD COLUMN property_listing_id BIGINT UNSIGNED NULL AFTER expires_game_day,
    ADD COLUMN lease_deposit_krw BIGINT NULL AFTER property_listing_id,
    ADD COLUMN funding_limit_ppm INT UNSIGNED NULL AFTER lease_deposit_krw,
    ADD COLUMN maximum_funding_krw BIGINT NULL AFTER funding_limit_ppm,
    ADD COLUMN replaced_loan_contract_id BIGINT UNSIGNED NULL
        AFTER maximum_funding_krw,
    ADD COLUMN replaced_loan_principal_krw BIGINT NOT NULL DEFAULT 0
        AFTER replaced_loan_contract_id,
    ADD COLUMN regulatory_dsr_applied BOOLEAN NULL AFTER stress_rate_bp,
    ADD COLUMN affordability_numerator_krw BIGINT NULL AFTER regulatory_dsr_applied,
    ADD COLUMN affordability_denominator_krw BIGINT NULL
        AFTER affordability_numerator_krw,
    ADD COLUMN affordability_ratio_ppm BIGINT NULL AFTER affordability_denominator_krw,
    ADD COLUMN affordability_limit_ppm INT UNSIGNED NULL AFTER affordability_ratio_ppm,
    ADD KEY ix_loan_quote_listing (property_listing_id),
    ADD KEY ix_loan_quote_replacement
        (save_id, run_revision, replaced_loan_contract_id),
    ADD CONSTRAINT fk_loan_quote_listing
        FOREIGN KEY (property_listing_id) REFERENCES property_listing (id),
    ADD CONSTRAINT fk_loan_quote_replacement
        FOREIGN KEY (save_id, run_revision, replaced_loan_contract_id)
        REFERENCES loan_contract (save_id, run_revision, id),
    ADD CONSTRAINT ck_loan_quote_amounts CHECK (
        requested_principal_krw > 0
        AND existing_loan_balance_krw >= 0
        AND replaced_loan_principal_krw >= 0
        AND post_execution_balance_krw >= 0
        AND (
            (
                purpose = 'unsecured'
                AND post_execution_balance_krw
                    = existing_loan_balance_krw + requested_principal_krw
            )
            OR (
                purpose = 'leaseDeposit'
                AND existing_loan_balance_krw >= replaced_loan_principal_krw
                AND post_execution_balance_krw
                    = existing_loan_balance_krw
                      - replaced_loan_principal_krw
                      + requested_principal_krw
            )
        )
        AND (verified_annual_income_krw IS NULL OR verified_annual_income_krw > 0)
    ),
    ADD CONSTRAINT ck_loan_quote_dsr_shape CHECK (
        (
            dsr_numerator_krw IS NULL
            AND dsr_denominator_krw IS NULL
            AND dsr_ratio_ppm IS NULL
            AND dsr_limit_ppm IS NULL
        )
        OR (
            purpose = 'unsecured'
            AND dsr_numerator_krw >= 0
            AND dsr_denominator_krw > 0
            AND dsr_ratio_ppm >= 0
            AND dsr_limit_ppm > 0
            AND dsr_ratio_ppm = FLOOR(
                CAST(dsr_numerator_krw AS DECIMAL(65, 0)) * 1000000
                / dsr_denominator_krw
            )
        )
    ),
    ADD CONSTRAINT ck_loan_quote_affordability_shape CHECK (
        (
            purpose = 'unsecured'
            AND affordability_numerator_krw IS NULL
            AND affordability_denominator_krw IS NULL
            AND affordability_ratio_ppm IS NULL
            AND affordability_limit_ppm IS NULL
        )
        OR (
            purpose = 'leaseDeposit'
            AND decision_code IN (
                'creditRestricted', 'collateralLimit', 'incomeUnavailable'
            )
            AND affordability_numerator_krw IS NULL
            AND affordability_denominator_krw IS NULL
            AND affordability_ratio_ppm IS NULL
            AND affordability_limit_ppm IS NULL
        )
        OR (
            purpose = 'leaseDeposit'
            AND decision_code IN ('eligible', 'affordabilityLimit')
            AND affordability_numerator_krw >= 0
            AND affordability_denominator_krw > 0
            AND affordability_denominator_krw = verified_annual_income_krw
            AND affordability_ratio_ppm >= 0
            AND affordability_limit_ppm BETWEEN 1 AND 1000000
            AND affordability_ratio_ppm = FLOOR(
                CAST(affordability_numerator_krw AS DECIMAL(65, 0)) * 1000000
                / affordability_denominator_krw
            )
            AND (
                (
                    decision_code = 'eligible'
                    AND affordability_ratio_ppm <= affordability_limit_ppm
                )
                OR (
                    decision_code = 'affordabilityLimit'
                    AND affordability_ratio_ppm > affordability_limit_ppm
                )
            )
        )
    ),
    ADD CONSTRAINT ck_loan_quote_purpose_shape CHECK (
        (
            purpose = 'unsecured'
            AND property_listing_id IS NULL
            AND lease_deposit_krw IS NULL
            AND funding_limit_ppm IS NULL
            AND maximum_funding_krw IS NULL
            AND replaced_loan_contract_id IS NULL
            AND replaced_loan_principal_krw = 0
            AND regulatory_dsr_applied IS NULL
            AND affordability_numerator_krw IS NULL
            AND affordability_denominator_krw IS NULL
            AND affordability_ratio_ppm IS NULL
            AND affordability_limit_ppm IS NULL
        )
        OR (
            purpose = 'leaseDeposit'
            AND property_listing_id IS NOT NULL
            AND lease_deposit_krw > 0
            AND funding_limit_ppm BETWEEN 1 AND 1000000
            AND maximum_funding_krw > 0
            AND maximum_funding_krw <= lease_deposit_krw
            AND (
                (replaced_loan_contract_id IS NULL AND replaced_loan_principal_krw = 0)
                OR (
                    replaced_loan_contract_id IS NOT NULL
                    AND replaced_loan_principal_krw > 0
                )
            )
            AND regulatory_dsr_applied = FALSE
            AND dsr_numerator_krw IS NULL
            AND dsr_denominator_krw IS NULL
            AND dsr_ratio_ppm IS NULL
            AND dsr_limit_ppm IS NULL
            AND stress_rate_bp = 0
        )
    ),
    ADD CONSTRAINT ck_loan_quote_decision CHECK (
        decision_code IN (
            'eligible', 'debtServiceLimit', 'incomeUnavailable',
            'creditRestricted', 'valuationUnavailable',
            'collateralLimit', 'affordabilityLimit'
        )
        AND JSON_TYPE(decision_reasons) = 'ARRAY'
        AND JSON_LENGTH(decision_reasons) BETWEEN 1 AND 8
        AND JSON_TYPE(quoted_terms) = 'OBJECT'
    );

ALTER TABLE loan_contract
    DROP CHECK ck_loan_contract_origin,
    DROP CHECK ck_loan_contract_kind,
    ADD COLUMN lease_contract_id BIGINT UNSIGNED NULL AFTER loan_quote_id,
    ADD UNIQUE KEY uk_loan_contract_lease
        (save_id, run_revision, lease_contract_id),
    ADD CONSTRAINT fk_loan_contract_lease
        FOREIGN KEY (save_id, run_revision, lease_contract_id)
        REFERENCES lease_contract (save_id, run_revision, id),
    ADD CONSTRAINT ck_loan_contract_origin CHECK (
        origin_kind IN (
            'characterStartV2', 'legacyV1Mapping', 'quoteExecution',
            'leaseDepositExecution', 'legacyDebtBridge'
        )
        AND (
            (
                origin_kind IN ('quoteExecution', 'leaseDepositExecution')
                AND loan_quote_id IS NOT NULL
            )
            OR (
                origin_kind NOT IN ('quoteExecution', 'leaseDepositExecution')
                AND loan_quote_id IS NULL
            )
        )
        AND (
            (origin_kind = 'legacyDebtBridge' AND origin_command_id IS NULL)
            OR (
                origin_kind <> 'legacyDebtBridge'
                AND origin_command_id REGEXP
                    '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            )
        )
    ),
    ADD CONSTRAINT ck_loan_contract_kind CHECK (
        product_kind IN (
            'studentLoan', 'unsecuredLoan', 'leaseDepositLoan', 'legacyDebt'
        )
    ),
    ADD CONSTRAINT ck_loan_contract_lease_shape CHECK (
        (
            product_kind = 'leaseDepositLoan'
            AND origin_kind = 'leaseDepositExecution'
            AND lease_contract_id IS NOT NULL
            AND dsr_included = FALSE
            AND read_only = FALSE
        )
        OR (
            product_kind <> 'leaseDepositLoan'
            AND origin_kind <> 'leaseDepositExecution'
            AND lease_contract_id IS NULL
        )
    );

ALTER TABLE loan_payment
    DROP CHECK ck_loan_payment_kind,
    ADD CONSTRAINT ck_loan_payment_kind CHECK (
        payment_kind IN (
            'scheduledInstallment', 'manualPrepayment', 'leaseMovePayoff'
        )
        AND (
            (payment_kind = 'scheduledInstallment' AND command_id IS NULL)
            OR (
                payment_kind IN ('manualPrepayment', 'leaseMovePayoff')
                AND command_id REGEXP
                    '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            )
        )
    );

CREATE TRIGGER tr_loan_quote_valid_insert
BEFORE INSERT ON loan_quote
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM save
        INNER JOIN household
            ON household.save_id = save.id
           AND household.run_revision = save.run_revision
           AND household.id = NEW.household_id
        INNER JOIN run_rule_bundle AS bundle
            ON bundle.save_id = save.id
           AND bundle.run_revision = save.run_revision
           AND bundle.credit_model_version_id = NEW.credit_model_version_id
        INNER JOIN credit_model_version AS model
            ON model.id = bundle.credit_model_version_id
           AND model.availability = 'active'
           AND model.sealed_at IS NOT NULL
        INNER JOIN credit_model_strict_manifest AS manifest
            ON manifest.credit_model_version_id = model.id
        INNER JOIN credit_model_strict_projection AS projection
            ON projection.credit_model_version_id = model.id
        INNER JOIN loan_product_version AS product
            ON product.id = NEW.loan_product_version_id
           AND product.credit_model_version_id = model.id
           AND product.catalog_scope = 'modelChild'
           AND product.quote_eligible = TRUE
           AND product.execution_eligible = TRUE
           AND product.sealed_at IS NOT NULL
        WHERE save.id = NEW.save_id
          AND save.run_revision = NEW.run_revision
          AND save.state_revision = NEW.expected_state_revision
          AND save.game_day = NEW.created_game_day
          AND BINARY model.canonical_sha256 = BINARY manifest.canonical_sha256
          AND BINARY manifest.canonical_json = BINARY projection.canonical_json
          AND NEW.requested_principal_krw
                BETWEEN product.minimum_principal_krw AND product.maximum_principal_krw
          AND (
              (
                  NEW.purpose = 'unsecured'
                  AND product.product_kind = 'unsecuredLoan'
                  AND product.execution_channel IS NULL
              )
              OR (
                  NEW.purpose = 'leaseDeposit'
                  AND BINARY model.version_key
                        = BINARY 'dev-unranked-m4c2c-credit-2026-v3'
                  AND product.product_kind = 'leaseDepositLoan'
                  AND product.execution_channel = 'leaseMove'
                  AND product.funding_limit_ppm = NEW.funding_limit_ppm
                  AND product.affordability_rule = 'interestOnly'
                  AND (
                      NEW.affordability_limit_ppm IS NULL
                      OR product.affordability_limit_ppm = NEW.affordability_limit_ppm
                  )
                  AND product.regulatory_dsr_treatment = 'excludedNoOwnedHome'
                  AND NEW.maximum_funding_krw = LEAST(
                      FLOOR(
                          CAST(NEW.lease_deposit_krw AS DECIMAL(65, 0))
                          * product.funding_limit_ppm / 1000000
                      ),
                      product.maximum_principal_krw
                  )
                  AND EXISTS (
                      SELECT 1
                      FROM real_estate_model_version AS real_estate_model
                      INNER JOIN real_estate_model_strict_manifest AS real_estate_manifest
                          ON real_estate_manifest.real_estate_model_version_id
                                = real_estate_model.id
                      INNER JOIN real_estate_model_strict_projection AS real_estate_projection
                          ON real_estate_projection.real_estate_model_version_id
                                = real_estate_model.id
                      INNER JOIN property_listing AS listing
                          ON listing.id = NEW.property_listing_id
                         AND listing.market_world_id = bundle.market_world_id
                         AND listing.real_estate_model_version_id = real_estate_model.id
                      INNER JOIN property_listing_offer AS offer
                          ON offer.property_listing_id = listing.id
                         AND offer.offer_kind = 'jeonse'
                      WHERE real_estate_model.id = bundle.real_estate_model_version_id
                        AND BINARY real_estate_model.version_key
                              = BINARY 'dev-unranked-m4-real-estate-lifecycle-2026-v4'
                        AND real_estate_model.availability = 'active'
                        AND real_estate_model.sealed_at IS NOT NULL
                        AND BINARY real_estate_model.canonical_sha256
                              = BINARY real_estate_manifest.canonical_sha256
                        AND BINARY real_estate_manifest.canonical_json
                              = BINARY real_estate_projection.canonical_json
                        AND listing.available_from_game_day <= save.game_day
                        AND listing.available_to_game_day >= save.game_day
                        AND offer.price_krw IS NULL
                        AND offer.deposit_krw = NEW.lease_deposit_krw
                        AND offer.monthly_rent_krw IS NULL
                  )
                  AND (
                      (
                          NEW.replaced_loan_contract_id IS NULL
                          AND NOT EXISTS (
                              SELECT 1
                              FROM lease_contract AS current_lease
                              INNER JOIN loan_contract AS linked_loan
                                  ON linked_loan.save_id = current_lease.save_id
                                 AND linked_loan.run_revision = current_lease.run_revision
                                 AND linked_loan.lease_contract_id = current_lease.id
                              WHERE current_lease.save_id = save.id
                                AND current_lease.run_revision = save.run_revision
                                AND current_lease.household_id = household.id
                                AND current_lease.effective_to_game_day IS NULL
                                AND linked_loan.status = 'active'
                          )
                      )
                      OR EXISTS (
                          SELECT 1
                          FROM loan_contract AS replacement
                          INNER JOIN lease_contract AS current_lease
                              ON current_lease.id = replacement.lease_contract_id
                             AND current_lease.save_id = replacement.save_id
                             AND current_lease.run_revision = replacement.run_revision
                          WHERE replacement.id = NEW.replaced_loan_contract_id
                            AND replacement.save_id = save.id
                            AND replacement.run_revision = save.run_revision
                            AND replacement.household_id = household.id
                            AND replacement.product_kind = 'leaseDepositLoan'
                            AND current_lease.household_id = household.id
                            AND current_lease.offer_kind = 'jeonse'
                            AND current_lease.effective_to_game_day IS NULL
                            AND replacement.status = 'active'
                            AND replacement.remaining_principal_krw
                                  = NEW.replaced_loan_principal_krw
                            AND replacement.accrued_interest_krw = 0
                            AND replacement.accrued_fee_krw = 0
                            AND current_lease.deposit_krw
                                  >= replacement.remaining_principal_krw
                            AND NOT EXISTS (
                                SELECT 1
                                FROM loan_obligation_bucket AS bucket
                                WHERE bucket.loan_contract_id = replacement.id
                                  AND bucket.status IN ('pending', 'delinquent')
                                  AND bucket.paid_amount_krw
                                        < bucket.original_amount_krw
                            )
                      )
                  )
              )
          )
    ),
    NEW.save_id,
    NULL
);

DROP TRIGGER tr_loan_contract_valid_insert;

CREATE TRIGGER tr_loan_contract_valid_insert
BEFORE INSERT ON loan_contract
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM household
        INNER JOIN run_rule_bundle AS bundle
            ON bundle.save_id = household.save_id
           AND bundle.run_revision = household.run_revision
           AND bundle.credit_model_version_id = NEW.credit_model_version_id
        INNER JOIN credit_model_version AS model
            ON model.id = bundle.credit_model_version_id
           AND model.sealed_at IS NOT NULL
        INNER JOIN loan_product_version AS product
            ON product.id = NEW.loan_product_version_id
           AND product.sealed_at IS NOT NULL
        LEFT JOIN loan_quote AS quote
            ON quote.id = NEW.loan_quote_id
           AND quote.save_id = household.save_id
           AND quote.run_revision = household.run_revision
        LEFT JOIN lease_contract AS lease
            ON lease.id = NEW.lease_contract_id
           AND lease.save_id = household.save_id
           AND lease.run_revision = household.run_revision
        WHERE household.id = NEW.household_id
          AND household.save_id = NEW.save_id
          AND household.run_revision = NEW.run_revision
          AND (
              (
                  NEW.origin_kind <> 'legacyDebtBridge'
                  AND model.availability = 'active'
                  AND product.catalog_scope = 'modelChild'
                  AND product.credit_model_version_id = model.id
                  AND product.product_kind = NEW.product_kind
                  AND product.lender_sector = NEW.lender_sector
                  AND product.rate_status = NEW.rate_status
                  AND product.rate_type = NEW.rate_type
                  AND BINARY product.reference_rate_key <=> BINARY NEW.reference_rate_key
                  AND product.fixed_annual_rate_bp <=> NEW.fixed_annual_rate_bp
                  AND product.spread_bp <=> NEW.applied_spread_bp
                  AND product.minimum_annual_rate_bp <=> NEW.minimum_annual_rate_bp
                  AND product.maximum_annual_rate_bp <=> NEW.maximum_annual_rate_bp
                  AND BINARY product.rate_reset_rule = BINARY NEW.rate_reset_rule
                  AND product.day_count_rule = 'actual365'
                  AND NEW.day_count_denominator = 365
                  AND product.repayment_method = NEW.repayment_method
                  AND product.term_months <=> NEW.term_months
                  AND NEW.total_installments = product.term_months
                  AND product.payment_calendar = NEW.payment_calendar
                  AND product.grace_months <=> NEW.grace_months
                  AND product.prepayment_fee_ppm <=> NEW.prepayment_fee_ppm
                  AND product.prepayment_effect = NEW.prepayment_effect
                  AND product.dsr_included = NEW.dsr_included
                  AND product.read_only = NEW.read_only
                  AND NEW.original_principal_krw
                        BETWEEN product.minimum_principal_krw
                            AND product.maximum_principal_krw
                  AND (
                      (
                          NEW.origin_kind NOT IN (
                              'quoteExecution', 'leaseDepositExecution'
                          )
                          AND NEW.loan_quote_id IS NULL
                          AND NEW.lease_contract_id IS NULL
                      )
                      OR (
                          NEW.origin_kind = 'quoteExecution'
                          AND quote.purpose = 'unsecured'
                          AND quote.decision_code = 'eligible'
                          AND quote.created_game_day = NEW.activated_game_day
                          AND quote.expires_game_day = NEW.activated_game_day
                          AND quote.loan_product_version_id = product.id
                          AND quote.requested_principal_krw
                                = NEW.original_principal_krw
                          AND NEW.lease_contract_id IS NULL
                      )
                      OR (
                          NEW.origin_kind = 'leaseDepositExecution'
                          AND product.execution_channel = 'leaseMove'
                          AND quote.purpose = 'leaseDeposit'
                          AND quote.decision_code = 'eligible'
                          AND quote.created_game_day = NEW.activated_game_day
                          AND quote.expires_game_day = NEW.activated_game_day
                          AND quote.loan_product_version_id = product.id
                          AND quote.requested_principal_krw
                                = NEW.original_principal_krw
                          AND quote.property_listing_id = lease.property_listing_id
                          AND quote.lease_deposit_krw = lease.deposit_krw
                          AND lease.household_id = household.id
                          AND lease.offer_kind = 'jeonse'
                          AND lease.effective_from_game_day = NEW.activated_game_day
                          AND lease.effective_to_game_day IS NULL
                          AND BINARY lease.command_id = BINARY NEW.origin_command_id
                      )
                  )
              )
              OR (
                  NEW.origin_kind = 'legacyDebtBridge'
                  AND product.catalog_scope = 'bridgeOnly'
                  AND product.credit_model_version_id IS NULL
                  AND product.product_key = 'compat-legacy-debt-zero-bullet-v1'
                  AND model.availability = 'disabled'
                  AND NEW.original_principal_krw
                        = household.legacy_debt_krw_at_activation
                  AND NEW.remaining_principal_krw = NEW.original_principal_krw
                  AND NEW.lease_contract_id IS NULL
              )
          )
    ),
    NEW.save_id,
    NULL
);

DROP TRIGGER tr_loan_contract_transition_only;

CREATE TRIGGER tr_loan_contract_transition_only
BEFORE UPDATE ON loan_contract
FOR EACH ROW
SET NEW.id = IF(
    OLD.read_only = FALSE
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.household_id = OLD.household_id
        AND NEW.credit_model_version_id = OLD.credit_model_version_id
        AND NEW.loan_product_version_id = OLD.loan_product_version_id
        AND NEW.loan_quote_id <=> OLD.loan_quote_id
        AND NEW.lease_contract_id <=> OLD.lease_contract_id
        AND BINARY NEW.origin_kind = BINARY OLD.origin_kind
        AND BINARY NEW.origin_command_id <=> BINARY OLD.origin_command_id
        AND BINARY NEW.product_kind = BINARY OLD.product_kind
        AND BINARY NEW.lender_sector = BINARY OLD.lender_sector
        AND BINARY NEW.rate_status = BINARY OLD.rate_status
        AND BINARY NEW.rate_type = BINARY OLD.rate_type
        AND BINARY NEW.reference_rate_key <=> BINARY OLD.reference_rate_key
        AND NEW.fixed_annual_rate_bp <=> OLD.fixed_annual_rate_bp
        AND NEW.applied_spread_bp <=> OLD.applied_spread_bp
        AND NEW.minimum_annual_rate_bp <=> OLD.minimum_annual_rate_bp
        AND NEW.maximum_annual_rate_bp <=> OLD.maximum_annual_rate_bp
        AND BINARY NEW.rate_reset_rule = BINARY OLD.rate_reset_rule
        AND NEW.day_count_denominator <=> OLD.day_count_denominator
        AND BINARY NEW.repayment_method = BINARY OLD.repayment_method
        AND NEW.term_months <=> OLD.term_months
        AND NEW.total_installments <=> OLD.total_installments
        AND BINARY NEW.payment_calendar = BINARY OLD.payment_calendar
        AND NEW.grace_months <=> OLD.grace_months
        AND NEW.prepayment_fee_ppm <=> OLD.prepayment_fee_ppm
        AND BINARY NEW.prepayment_effect = BINARY OLD.prepayment_effect
        AND NEW.dsr_included = OLD.dsr_included
        AND NEW.read_only = OLD.read_only
        AND NEW.original_principal_krw = OLD.original_principal_krw
        AND NEW.remaining_principal_krw <= OLD.remaining_principal_krw
        AND NEW.activated_game_day = OLD.activated_game_day
        AND NEW.maturity_game_day <=> OLD.maturity_game_day
        AND NEW.created_at = OLD.created_at
        AND (
            (OLD.status = 'pending' AND NEW.status IN ('active', 'cancelled'))
            OR (OLD.status = 'active'
                AND NEW.status IN ('active', 'delinquent', 'paidOff'))
            OR (OLD.status = 'delinquent'
                AND NEW.status IN ('active', 'delinquent', 'defaulted'))
            OR (OLD.status = 'defaulted'
                AND NEW.status IN (
                    'defaulted', 'restructured', 'discharged', 'chargedOff'
                ))
        ),
    OLD.id,
    NULL
);

DROP TRIGGER tr_loan_payment_valid_insert;

CREATE TRIGGER tr_loan_payment_valid_insert
BEFORE INSERT ON loan_payment
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'prepared'
        AND NEW.ledger_transaction_id IS NULL
        AND EXISTS (
            SELECT 1
            FROM loan_contract AS contract
            LEFT JOIN lease_contract AS lease
                ON lease.id = contract.lease_contract_id
               AND lease.save_id = contract.save_id
               AND lease.run_revision = contract.run_revision
            LEFT JOIN command_identity AS identity
                ON identity.save_id = contract.save_id
               AND BINARY identity.command_id = BINARY NEW.command_id
            INNER JOIN save
                ON save.id = contract.save_id
               AND save.run_revision = contract.run_revision
            WHERE contract.id = NEW.loan_contract_id
              AND contract.save_id = NEW.save_id
              AND contract.run_revision = NEW.run_revision
              AND contract.read_only = FALSE
              AND contract.status IN ('active', 'delinquent')
              AND (
                  NEW.payment_kind = 'scheduledInstallment'
                  OR (
                      NEW.payment_kind = 'manualPrepayment'
                      AND contract.status = 'active'
                      AND NOT EXISTS (
                          SELECT 1 FROM loan_obligation_bucket AS bucket
                          WHERE bucket.loan_contract_id = contract.id
                            AND bucket.status = 'delinquent'
                      )
                  )
                  OR (
                      NEW.payment_kind = 'leaseMovePayoff'
                      AND contract.product_kind = 'leaseDepositLoan'
                      AND contract.status = 'active'
                      AND contract.accrued_interest_krw = 0
                      AND contract.accrued_fee_krw = 0
                      AND NEW.amount_krw = contract.remaining_principal_krw
                      AND NEW.amount_krw > 0
                      AND NEW.game_day = save.game_day
                      AND identity.command_kind = 'startLease'
                      AND identity.initial_run_revision = contract.run_revision
                      AND identity.initial_game_day = save.game_day
                      AND lease.deposit_krw >= contract.remaining_principal_krw
                      AND (
                          lease.effective_to_game_day IS NULL
                          OR lease.effective_to_game_day = save.game_day
                      )
                      AND NOT EXISTS (
                          SELECT 1 FROM loan_obligation_bucket AS bucket
                          WHERE bucket.loan_contract_id = contract.id
                            AND bucket.status IN ('pending', 'delinquent')
                            AND bucket.paid_amount_krw < bucket.original_amount_krw
                      )
                  )
              )
        ),
    NEW.save_id,
    NULL
);

DROP TRIGGER tr_ledger_posting_lease_reference_insert;
DROP TRIGGER tr_ledger_posting_loan_reference_insert;

CREATE TRIGGER tr_ledger_posting_loan_reference_insert
BEFORE INSERT ON ledger_posting
FOR EACH ROW
FOLLOWS tr_ledger_posting_life_reference_insert
SET NEW.account_code = IF(
    (
        NEW.account_code IN (
            'loanPrincipalLiability', 'loanInterestExpense',
            'loanInterestLiability', 'loanFeeExpense'
        )
        AND EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            INNER JOIN loan_contract AS contract
                ON contract.id = NEW.loan_contract_id
               AND contract.save_id = ledger.save_id
               AND contract.run_revision = ledger.run_revision
            LEFT JOIN loan_payment AS payment
                ON payment.loan_contract_id = contract.id
               AND BINARY CAST(payment.id AS CHAR) = BINARY ledger.source_id
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND (
                  (
                      ledger.source_kind IN ('loanOrigination', 'debtAuthorityBridge')
                      AND BINARY ledger.source_id = BINARY CAST(contract.id AS CHAR)
                  )
                  OR (
                      ledger.source_kind IN ('loanInstallment', 'loanPrepayment')
                      AND payment.status = 'prepared'
                  )
                  OR (
                      ledger.source_kind = 'leaseMove'
                      AND NEW.account_code = 'loanPrincipalLiability'
                      AND (
                          (
                              contract.origin_kind = 'leaseDepositExecution'
                              AND contract.product_kind = 'leaseDepositLoan'
                              AND BINARY contract.origin_command_id
                                    = BINARY ledger.source_id
                              AND contract.activated_game_day = ledger.game_day
                              AND NEW.amount_krw = -contract.original_principal_krw
                          )
                          OR EXISTS (
                              SELECT 1
                              FROM loan_payment AS payoff
                              WHERE payoff.loan_contract_id = contract.id
                                AND payoff.save_id = ledger.save_id
                                AND payoff.run_revision = ledger.run_revision
                                AND payoff.payment_kind = 'leaseMovePayoff'
                                AND payoff.status = 'prepared'
                                AND payoff.game_day = ledger.game_day
                                AND BINARY payoff.command_id = BINARY ledger.source_id
                                AND NEW.amount_krw = payoff.amount_krw
                          )
                      )
                  )
              )
        )
    )
    OR (
        NEW.account_code = 'taxObligationLiability'
        AND EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            INNER JOIN tax_obligation AS obligation
                ON obligation.id = NEW.tax_obligation_id
               AND obligation.save_id = ledger.save_id
               AND obligation.run_revision = ledger.run_revision
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND obligation.status IN ('prepared', 'outstanding')
        )
    )
    OR (
        NEW.account_code NOT IN (
            'loanPrincipalLiability', 'loanInterestExpense',
            'loanInterestLiability', 'loanFeeExpense', 'taxObligationLiability'
        )
        AND NEW.loan_contract_id IS NULL
        AND NEW.tax_obligation_id IS NULL
    ),
    NEW.account_code,
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
        AND NEW.loan_contract_id IS NULL
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
            LEFT JOIN loan_contract AS originated_loan
                ON originated_loan.save_id = started_contract.save_id
               AND originated_loan.run_revision = started_contract.run_revision
               AND originated_loan.lease_contract_id = started_contract.id
               AND originated_loan.origin_kind = 'leaseDepositExecution'
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'leaseMove'
              AND (
                  NEW.amount_krw = -(
                      started_contract.deposit_krw
                      - COALESCE(originated_loan.original_principal_krw, 0)
                  )
                  OR NEW.amount_krw = -moving_cost.moving_cost_krw
                  OR EXISTS (
                      SELECT 1
                      FROM lease_contract AS ended_contract
                      LEFT JOIN loan_contract AS ended_loan
                          ON ended_loan.save_id = ended_contract.save_id
                         AND ended_loan.run_revision = ended_contract.run_revision
                         AND ended_loan.lease_contract_id = ended_contract.id
                      LEFT JOIN loan_payment AS payoff
                          ON payoff.loan_contract_id = ended_loan.id
                         AND payoff.save_id = ended_contract.save_id
                         AND payoff.run_revision = ended_contract.run_revision
                         AND payoff.payment_kind = 'leaseMovePayoff'
                         AND payoff.status = 'prepared'
                         AND BINARY payoff.command_id = BINARY ledger.source_id
                      WHERE ended_contract.save_id = ledger.save_id
                        AND ended_contract.run_revision = ledger.run_revision
                        AND ended_contract.household_id
                              = started_contract.household_id
                        AND ended_contract.effective_to_game_day = ledger.game_day
                        AND NEW.amount_krw = ended_contract.deposit_krw
                              - COALESCE(payoff.amount_krw, 0)
                  )
              )
        )
    )
    OR (
        NEW.account_code = 'loanPrincipalLiability'
        AND NEW.lease_contract_id IS NULL
        AND NEW.loan_contract_id IS NOT NULL
        AND EXISTS (
            SELECT 1 FROM ledger_transaction AS ledger
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'leaseMove'
        )
    )
    OR (
        NEW.account_code NOT IN (
            'leaseDepositAsset', 'movingExpense', 'loanPrincipalLiability'
        )
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

INSERT INTO credit_model_version
    (version_key, availability, ranked_eligible, credit_policy_set_id, parameters)
SELECT
    'dev-unranked-m4c2c-credit-2026-v3',
    'active',
    FALSE,
    policy.id,
    JSON_OBJECT(
        'bands', JSON_ARRAY(
            JSON_OBJECT('band', 'prime', 'maximumUnits', 1000, 'minimumUnits', 850),
            JSON_OBJECT('band', 'standard', 'maximumUnits', 849, 'minimumUnits', 650),
            JSON_OBJECT('band', 'limited', 'maximumUnits', 649, 'minimumUnits', 450),
            JSON_OBJECT('band', 'distressed', 'maximumUnits', 449, 'minimumUnits', 1),
            JSON_OBJECT('band', 'insolvent', 'maximumUnits', 0, 'minimumUnits', 0)
        ),
        'creditUnits', JSON_OBJECT('initial', 700, 'maximum', 1000, 'minimum', 0),
        'dailyChange', JSON_OBJECT(
            'cleanRecoveryUnits', 1,
            'delinquentOrDefaultedPenaltyUnits', -5
        ),
        'defaultRule', JSON_OBJECT(
            'absoluteOldestBucketDays', 90,
            'amountAndAgeMinimumKrw', 1000000,
            'amountAndAgeOldestBucketDays', 30
        ),
        'eventPenalty', JSON_OBJECT(
            'activeToDelinquentUnits', -80,
            'delinquentToDefaultedUnits', -300,
            'legalProcedureUnits', 0
        ),
        'leaseDepositAffordability', JSON_OBJECT(
            'maximumRatioPpm', 400000,
            'newLoanTreatment', 'interestOnly',
            'replacementLoanTreatment', 'excluded'
        ),
        'loanEligibility', JSON_OBJECT(
            'unsecuredLoan', JSON_OBJECT(
                'allowedCreditBands', JSON_ARRAY('prime', 'standard'),
                'disallowedContractStatuses',
                    JSON_ARRAY('delinquent', 'defaulted', 'restructured'),
                'maximumActiveContracts', 8
            ),
            'leaseDepositLoan', JSON_OBJECT(
                'allowedCreditBands', JSON_ARRAY('prime', 'standard'),
                'disallowedContractStatuses',
                    JSON_ARRAY('delinquent', 'defaulted', 'restructured'),
                'maximumActiveContracts', 8
            )
        ),
        'provenance', 'GAME_BALANCE',
        'schemaVersion', 4
    )
FROM policy_set AS policy
WHERE policy.policy_key = 'dev-unranked-m4b-credit-policy-2026-v1'
  AND policy.sealed_at IS NOT NULL;

INSERT INTO loan_product_version
    (
        credit_model_version_id, product_key, display_name, catalog_scope, product_kind,
        lender_sector, rate_status, rate_type, reference_rate_key, fixed_annual_rate_bp,
        spread_bp, minimum_annual_rate_bp, maximum_annual_rate_bp, rate_reset_rule,
        day_count_rule, repayment_method, term_months, payment_calendar, grace_months,
        minimum_principal_krw, maximum_principal_krw, prepayment_fee_ppm,
        prepayment_effect, collateral_rule, execution_channel, funding_limit_ppm,
        affordability_rule, affordability_limit_ppm, regulatory_dsr_treatment,
        starting_eligible, quote_eligible, execution_eligible, prepayment_allowed,
        dsr_included, read_only, provenance_kind, display_order
    )
SELECT
    model.id, 'dev-student-fixed-equal-principal-2026-v3',
    '개발 학자금 고정금리 대출', 'modelChild', 'studentLoan', 'bank', 'available',
    'fixed', NULL, 170, NULL, 170, 170, 'none', 'actual365', 'equalPrincipal',
    120, 'monthEnd', 0, 1, 50000000, 0, 'reduceTerm', 'none',
    NULL, NULL, NULL, NULL, NULL,
    TRUE, FALSE, FALSE, TRUE, TRUE, FALSE, 'GAME_BALANCE', 1
FROM credit_model_version AS model
WHERE model.version_key = 'dev-unranked-m4c2c-credit-2026-v3'
UNION ALL
SELECT
    model.id, 'dev-unsecured-variable-level-payment-2026-v3',
    '개발 변동금리 신용대출', 'modelChild', 'unsecuredLoan', 'bank', 'available',
    'variable', 'treasury3m', NULL, 400, 300, 1500, 'monthlyDay1', 'actual365',
    'levelPayment', 60, 'monthEnd', 0, 1, 200000000, 10000, 'recalculatePayment',
    'none', NULL, NULL, NULL, NULL, NULL,
    TRUE, TRUE, TRUE, TRUE, TRUE, FALSE, 'GAME_BALANCE', 2
FROM credit_model_version AS model
WHERE model.version_key = 'dev-unranked-m4c2c-credit-2026-v3'
UNION ALL
SELECT
    model.id, 'dev-lease-deposit-fixed-bullet-2026-v1',
    '개발 전세보증금 고정금리 대출', 'modelChild', 'leaseDepositLoan', 'bank',
    'available', 'fixed', NULL, 400, NULL, 400, 400, 'none', 'actual365',
    'bullet', 24, 'monthEnd', 0, 1, 400000000, 0, 'reduceTerm',
    'leaseDepositFundingLimit', 'leaseMove', 800000, 'interestOnly', 400000,
    'excludedNoOwnedHome',
    FALSE, TRUE, TRUE, TRUE, FALSE, FALSE, 'GAME_BALANCE', 3
FROM credit_model_version AS model
WHERE model.version_key = 'dev-unranked-m4c2c-credit-2026-v3';

INSERT INTO loan_product_canonical_manifest (loan_product_version_id, canonical_json)
SELECT projection.loan_product_version_id, projection.canonical_json
FROM loan_product_canonical_projection AS projection
INNER JOIN loan_product_version AS product
    ON product.id = projection.loan_product_version_id
WHERE product.credit_model_version_id = (
    SELECT id FROM credit_model_version
    WHERE version_key = 'dev-unranked-m4c2c-credit-2026-v3'
);

UPDATE loan_product_version AS product
INNER JOIN loan_product_canonical_manifest AS manifest
    ON manifest.loan_product_version_id = product.id
SET product.canonical_sha256 = manifest.canonical_sha256,
    product.sealed_at = CURRENT_TIMESTAMP(3)
WHERE product.credit_model_version_id = (
    SELECT id FROM credit_model_version
    WHERE version_key = 'dev-unranked-m4c2c-credit-2026-v3'
)
  AND product.sealed_at IS NULL;

INSERT INTO loan_product_legacy_start_mapping
    (
        credit_model_version_id, legacy_field_key, product_kind,
        loan_product_version_id, mapping_order
    )
SELECT model.id, 'studentLoanKrw', 'studentLoan', product.id, 1
FROM credit_model_version AS model
INNER JOIN loan_product_version AS product
    ON product.credit_model_version_id = model.id
   AND product.product_kind = 'studentLoan'
WHERE model.version_key = 'dev-unranked-m4c2c-credit-2026-v3'
UNION ALL
SELECT model.id, 'creditLoanKrw', 'unsecuredLoan', product.id, 2
FROM credit_model_version AS model
INNER JOIN loan_product_version AS product
    ON product.credit_model_version_id = model.id
   AND product.product_kind = 'unsecuredLoan'
WHERE model.version_key = 'dev-unranked-m4c2c-credit-2026-v3';

INSERT INTO credit_model_strict_manifest (credit_model_version_id, canonical_json)
SELECT credit_model_version_id, canonical_json
FROM credit_model_strict_projection
WHERE credit_model_version_id = (
    SELECT id FROM credit_model_version
    WHERE version_key = 'dev-unranked-m4c2c-credit-2026-v3'
);

UPDATE credit_model_version AS model
INNER JOIN credit_model_strict_manifest AS manifest
    ON manifest.credit_model_version_id = model.id
SET model.canonical_sha256 = manifest.canonical_sha256,
    model.sealed_at = CURRENT_TIMESTAMP(3)
WHERE model.version_key = 'dev-unranked-m4c2c-credit-2026-v3'
  AND model.sealed_at IS NULL;

-- Existing runs retain their exact credit and real-estate pins. Only future runs move to v3.
UPDATE run_rule_bundle_assignment AS assignment
INNER JOIN credit_model_version AS active_credit
    ON active_credit.version_key = 'dev-unranked-m4c2c-credit-2026-v3'
   AND active_credit.availability = 'active'
   AND active_credit.sealed_at IS NOT NULL
SET assignment.credit_model_version_id = active_credit.id
WHERE assignment.assignment_key = 'newRun';

CREATE TEMPORARY TABLE m4c2c_final_guard (
    guard_key VARCHAR(48) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    accepted  TINYINT UNSIGNED NOT NULL,
    CONSTRAINT ck_m4c2c_final_guard CHECK (accepted = 1)
);

INSERT INTO m4c2c_final_guard (guard_key, accepted)
SELECT 'sealed-v3-graph', IF(
    EXISTS (
        SELECT 1
        FROM credit_model_version AS model
        INNER JOIN credit_model_strict_manifest AS manifest
            ON manifest.credit_model_version_id = model.id
        INNER JOIN credit_model_strict_projection AS projection
            ON projection.credit_model_version_id = model.id
        WHERE model.version_key = 'dev-unranked-m4c2c-credit-2026-v3'
          AND model.sealed_at IS NOT NULL
          AND BINARY model.canonical_sha256 = BINARY manifest.canonical_sha256
          AND BINARY manifest.canonical_json = BINARY projection.canonical_json
          AND (SELECT COUNT(*) FROM loan_product_version AS product
               WHERE product.credit_model_version_id = model.id
                 AND product.sealed_at IS NOT NULL) = 3
          AND (SELECT COUNT(*) FROM loan_product_legacy_start_mapping AS mapping
               WHERE mapping.credit_model_version_id = model.id) = 2
    ),
    1,
    0
);

INSERT INTO m4c2c_final_guard (guard_key, accepted)
SELECT 'old-graphs-still-byte-exact', IF(
    NOT EXISTS (
        SELECT 1
        FROM credit_model_strict_manifest AS manifest
        INNER JOIN credit_model_version AS model
            ON model.id = manifest.credit_model_version_id
        LEFT JOIN credit_model_strict_projection AS projection
            ON projection.credit_model_version_id = manifest.credit_model_version_id
        WHERE model.version_key IN (
                  'dev-unranked-m4b-credit-2026-v1',
                  'dev-unranked-m4b-credit-2026-v2'
              )
          AND (
              projection.credit_model_version_id IS NULL
              OR BINARY manifest.canonical_json <> BINARY projection.canonical_json
              OR BINARY manifest.canonical_sha256 <> BINARY model.canonical_sha256
          )
    )
        AND NOT EXISTS (
            SELECT 1
            FROM real_estate_model_strict_manifest AS manifest
            INNER JOIN real_estate_model_version AS model
                ON model.id = manifest.real_estate_model_version_id
            LEFT JOIN real_estate_model_strict_projection AS projection
                ON projection.real_estate_model_version_id
                    = manifest.real_estate_model_version_id
            WHERE model.version_key IN (
                      'dev-unranked-m4-real-estate-2026-v1',
                      'dev-unranked-m4-real-estate-lease-2026-v2',
                      'dev-unranked-m4-real-estate-rent-2026-v3',
                      'dev-unranked-m4-real-estate-lifecycle-2026-v4'
                  )
              AND (
                  projection.real_estate_model_version_id IS NULL
                  OR BINARY manifest.canonical_json <> BINARY projection.canonical_json
                  OR BINARY manifest.canonical_sha256 <> BINARY model.canonical_sha256
              )
        ),
    1,
    0
);

INSERT INTO m4c2c_final_guard (guard_key, accepted)
SELECT 'new-run-credit-only', IF(
    EXISTS (
        SELECT 1
        FROM run_rule_bundle_assignment AS assignment
        INNER JOIN credit_model_version AS credit
            ON credit.id = assignment.credit_model_version_id
        INNER JOIN real_estate_model_version AS real_estate
            ON real_estate.id = assignment.real_estate_model_version_id
        WHERE assignment.assignment_key = 'newRun'
          AND BINARY credit.version_key
                = BINARY 'dev-unranked-m4c2c-credit-2026-v3'
          AND BINARY real_estate.version_key
                = BINARY 'dev-unranked-m4-real-estate-lifecycle-2026-v4'
    ),
    1,
    0
);

DROP TEMPORARY TABLE m4c2c_final_guard;
