-- M4-C3 publishes owner-occupied property purchase and mortgage lending without changing
-- the meaning or canonical bytes of any previously sealed housing or credit graph (§5 C3).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;
SET SESSION group_concat_max_len = 1048576;

-- MySQL DDL auto-commits. Reject a non-canonical C2c graph before the first durable change,
-- so a forward migration cannot strand a partially published purchase capability.
CREATE TEMPORARY TABLE m4c3_preflight_guard (
    guard_key VARCHAR(48) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    accepted  TINYINT UNSIGNED NOT NULL,
    CONSTRAINT ck_m4c3_preflight_guard CHECK (accepted = 1)
);

INSERT INTO m4c3_preflight_guard (guard_key, accepted)
SELECT 'sealed-credit-graphs', IF(
    NOT EXISTS (
        SELECT 1
        FROM credit_model_version AS model
        LEFT JOIN credit_model_strict_manifest AS manifest
            ON manifest.credit_model_version_id = model.id
        LEFT JOIN credit_model_strict_projection AS projection
            ON projection.credit_model_version_id = model.id
        WHERE model.version_key IN (
                  'dev-unranked-m4b-credit-2026-v1',
                  'dev-unranked-m4b-credit-2026-v2',
                  'dev-unranked-m4c2c-credit-2026-v3'
              )
          AND (
              model.sealed_at IS NULL
              OR manifest.credit_model_version_id IS NULL
              OR projection.credit_model_version_id IS NULL
              OR BINARY model.canonical_sha256 <> BINARY manifest.canonical_sha256
              OR BINARY manifest.canonical_json <> BINARY projection.canonical_json
          )
    )
        AND (
            SELECT COUNT(*)
            FROM credit_model_version
            WHERE version_key IN (
                      'dev-unranked-m4b-credit-2026-v1',
                      'dev-unranked-m4b-credit-2026-v2',
                      'dev-unranked-m4c2c-credit-2026-v3'
                  )
        ) = 3,
    1,
    0
);

INSERT INTO m4c3_preflight_guard (guard_key, accepted)
SELECT 'sealed-real-estate-graphs', IF(
    NOT EXISTS (
        SELECT 1
        FROM real_estate_model_version AS model
        LEFT JOIN real_estate_model_strict_manifest AS manifest
            ON manifest.real_estate_model_version_id = model.id
        LEFT JOIN real_estate_model_strict_projection AS projection
            ON projection.real_estate_model_version_id = model.id
        WHERE model.version_key IN (
                  'dev-unranked-m4-real-estate-2026-v1',
                  'dev-unranked-m4-real-estate-lease-2026-v2',
                  'dev-unranked-m4-real-estate-rent-2026-v3',
                  'dev-unranked-m4-real-estate-lifecycle-2026-v4'
              )
          AND (
              model.sealed_at IS NULL
              OR manifest.real_estate_model_version_id IS NULL
              OR projection.real_estate_model_version_id IS NULL
              OR BINARY model.canonical_sha256 <> BINARY manifest.canonical_sha256
              OR BINARY manifest.canonical_json <> BINARY projection.canonical_json
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
        ) = 4,
    1,
    0
);

INSERT INTO m4c3_preflight_guard (guard_key, accepted)
SELECT 'sealed-product-manifests', IF(
    NOT EXISTS (
        SELECT 1
        FROM loan_product_version AS product
        LEFT JOIN loan_product_canonical_manifest AS manifest
            ON manifest.loan_product_version_id = product.id
        LEFT JOIN loan_product_canonical_projection AS projection
            ON projection.loan_product_version_id = product.id
        WHERE product.sealed_at IS NOT NULL
          AND (
              manifest.loan_product_version_id IS NULL
              OR projection.loan_product_version_id IS NULL
              OR BINARY product.canonical_sha256 <> BINARY manifest.canonical_sha256
              OR BINARY manifest.canonical_json <> BINARY projection.canonical_json
          )
    ),
    1,
    0
);

INSERT INTO m4c3_preflight_guard (guard_key, accepted)
SELECT 'target-keys-unused', IF(
    NOT EXISTS (
        SELECT 1 FROM policy_set
        WHERE policy_key = 'dev-unranked-m4c3-credit-policy-2026-v2'
    )
        AND NOT EXISTS (
            SELECT 1 FROM credit_model_version
            WHERE version_key = 'dev-unranked-m4c3-credit-2026-v4'
        )
        AND NOT EXISTS (
            SELECT 1 FROM real_estate_model_version
            WHERE version_key = 'dev-unranked-m4-real-estate-purchase-2026-v5'
        )
        AND NOT EXISTS (
            SELECT 1 FROM loan_product_version
            WHERE product_key = 'dev-mortgage-fixed-level-payment-2026-v1'
        ),
    1,
    0
);

DROP TEMPORARY TABLE m4c3_preflight_guard;

INSERT INTO policy_source_document
    (source_key, source_url, checked_on, original_sha256)
VALUES
    (
        'fsc-loan-demand-management-2025-10-15',
        'https://www.fsc.go.kr/comm/getFile?fileNo=1&fileTy=ATTACH&srvcId=BBSTY1&upperNo=85432',
        '2026-07-27',
        '4a7d1f8c5c90f8b4c2a01669f5d668d127eb9119194905ab27c2134f4bc51374'
    ),
    (
        'fsc-loan-demand-management-faq-2025-10-15',
        'https://www.fsc.go.kr/comm/getFile?fileNo=4&fileTy=ATTACH&srvcId=BBSTY1&upperNo=85432',
        '2026-07-27',
        '26a826fcaea1dcaecabcc4ee79b991bfa5522cdf15b3a053893c78d0e6f24613'
    ),
    (
        'fsc-stress-dsr-2026-h1',
        'https://www.fsc.go.kr/comm/getFile?fileNo=4&fileTy=ATTACH&srvcId=BBSTY1&upperNo=85824',
        '2026-07-27',
        'd9653003d75b9d6a603d208a5d9b9c80335d034073ebcdff11214cf572e624a7'
    );

INSERT INTO policy_set (policy_key, basis_date, ranked_eligible)
VALUES ('dev-unranked-m4c3-credit-policy-2026-v2', '2026-07-27', FALSE);

-- The borrower DSR and unsecured stress rules are copied as sourced rules, not inherited by
-- reference. The new immutable set is therefore sufficient to reproduce every v4 decision.
INSERT INTO policy_rule
    (policy_set_id, domain, rule_key, effective_from, effective_to, parameters)
SELECT target.id, source.domain, source.rule_key,
       source.effective_from, source.effective_to, source.parameters
FROM policy_set AS target
INNER JOIN policy_set AS source_set
    ON source_set.policy_key = 'dev-unranked-m4b-credit-policy-2026-v1'
INNER JOIN policy_rule AS source
    ON source.policy_set_id = source_set.id
WHERE target.policy_key = 'dev-unranked-m4c3-credit-policy-2026-v2';

INSERT INTO policy_rule
    (policy_set_id, domain, rule_key, effective_from, effective_to, parameters)
SELECT policy.id,
       'credit',
       'mortgageLtvLimits',
       '2025-10-16',
       NULL,
       JSON_OBJECT(
           'collateralValueRule', 'exactSalePriceAtExecution',
           'ltvNumerator', 'mortgagePrincipalOnly',
           'nonRegulatedProxyLimitPpm', 700000,
           'ownerPurpose', 'ownerOccupied',
           'ratioScalePpm', 1000000,
           'regulatedCapitalProxyLimitPpm', 400000,
           'schemaVersion', 1
       )
FROM policy_set AS policy
WHERE policy.policy_key = 'dev-unranked-m4c3-credit-policy-2026-v2'
UNION ALL
SELECT policy.id,
       'credit',
       'mortgageRegionalPriceCaps',
       '2025-10-16',
       NULL,
       JSON_OBJECT(
           'bands', JSON_ARRAY(
               JSON_OBJECT(
                   'maximumCollateralValueKrw', 1500000000,
                   'maximumMortgageKrw', 600000000
               ),
               JSON_OBJECT(
                   'maximumCollateralValueKrw', 2500000000,
                   'maximumMortgageKrw', 400000000
               ),
               JSON_OBJECT(
                   'maximumCollateralValueKrw', NULL,
                   'maximumMortgageKrw', 200000000
               )
           ),
           'boundaryRule', 'upperInclusive',
           'regionClasses', JSON_ARRAY('regulatedCapitalProxy'),
           'schemaVersion', 1
       )
FROM policy_set AS policy
WHERE policy.policy_key = 'dev-unranked-m4c3-credit-policy-2026-v2'
UNION ALL
SELECT policy.id,
       'credit',
       'mortgageStressDsr2026H1',
       '2026-01-01',
       NULL,
       JSON_OBJECT(
           'fixedPeriodRatioPpm', 1000000,
           'maturityRatioPpm', 1000000,
           'schemaVersion', 1,
           'stressApplicationPpm', 0,
           'stressTreatment', 'fullTermFixed'
       )
FROM policy_set AS policy
WHERE policy.policy_key = 'dev-unranked-m4c3-credit-policy-2026-v2';

-- Preserve the exact source links for copied rules.
INSERT INTO policy_rule_source
    (policy_rule_id, policy_source_document_id, citation_order)
SELECT target_rule.id, source_link.policy_source_document_id, source_link.citation_order
FROM policy_rule AS target_rule
INNER JOIN policy_set AS target_set
    ON target_set.id = target_rule.policy_set_id
INNER JOIN policy_set AS source_set
    ON source_set.policy_key = 'dev-unranked-m4b-credit-policy-2026-v1'
INNER JOIN policy_rule AS source_rule
    ON source_rule.policy_set_id = source_set.id
   AND BINARY source_rule.domain = BINARY target_rule.domain
   AND BINARY source_rule.rule_key = BINARY target_rule.rule_key
   AND source_rule.effective_from = target_rule.effective_from
INNER JOIN policy_rule_source AS source_link
    ON source_link.policy_rule_id = source_rule.id
WHERE target_set.policy_key = 'dev-unranked-m4c3-credit-policy-2026-v2';

INSERT INTO policy_rule_source
    (policy_rule_id, policy_source_document_id, citation_order)
SELECT rule.id, source.id, citation.citation_order
FROM policy_rule AS rule
INNER JOIN policy_set AS policy
    ON policy.id = rule.policy_set_id
INNER JOIN (
    SELECT 'mortgageLtvLimits' AS rule_key,
           'fsc-loan-demand-management-2025-10-15' AS source_key,
           1 AS citation_order
    UNION ALL
    SELECT 'mortgageLtvLimits',
           'fsc-loan-demand-management-faq-2025-10-15', 2
    UNION ALL
    SELECT 'mortgageLtvLimits', 'law-bank-supervision-regulation-2026-04-01', 3
    UNION ALL
    SELECT 'mortgageRegionalPriceCaps',
           'fsc-loan-demand-management-2025-10-15', 1
    UNION ALL
    SELECT 'mortgageRegionalPriceCaps',
           'fsc-loan-demand-management-faq-2025-10-15', 2
    UNION ALL
    SELECT 'mortgageStressDsr2026H1', 'fsc-stress-dsr-2026-h1', 1
    UNION ALL
    SELECT 'mortgageStressDsr2026H1',
           'law-bank-supervision-regulation-2026-04-01', 2
) AS citation
    ON BINARY citation.rule_key = BINARY rule.rule_key
INNER JOIN policy_source_document AS source
    ON BINARY source.source_key = BINARY citation.source_key
WHERE policy.policy_key = 'dev-unranked-m4c3-credit-policy-2026-v2';

CREATE TABLE credit_mortgage_policy_profile (
    policy_set_id                         BIGINT UNSIGNED NOT NULL,
    regulated_capital_ltv_limit_ppm       INT UNSIGNED NOT NULL,
    non_regulated_ltv_limit_ppm           INT UNSIGNED NOT NULL,
    lower_price_threshold_krw             BIGINT NOT NULL,
    upper_price_threshold_krw             BIGINT NOT NULL,
    lower_band_cap_krw                    BIGINT NOT NULL,
    middle_band_cap_krw                   BIGINT NOT NULL,
    upper_band_cap_krw                    BIGINT NOT NULL,
    borrower_dsr_balance_threshold_krw    BIGINT NOT NULL,
    bank_dsr_limit_ppm                    INT UNSIGNED NOT NULL,
    evaluation_horizon_months             TINYINT UNSIGNED NOT NULL,
    full_term_fixed_stress_rate_bp        SMALLINT UNSIGNED NOT NULL,
    created_at                            DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (policy_set_id),
    CONSTRAINT fk_credit_mortgage_policy_profile_set
        FOREIGN KEY (policy_set_id) REFERENCES policy_set (id),
    CONSTRAINT ck_credit_mortgage_policy_profile_ltv CHECK (
        regulated_capital_ltv_limit_ppm = 400000
        AND non_regulated_ltv_limit_ppm = 700000
    ),
    CONSTRAINT ck_credit_mortgage_policy_profile_price_bands CHECK (
        lower_price_threshold_krw = 1500000000
        AND upper_price_threshold_krw = 2500000000
        AND lower_price_threshold_krw < upper_price_threshold_krw
        AND lower_band_cap_krw = 600000000
        AND middle_band_cap_krw = 400000000
        AND upper_band_cap_krw = 200000000
        AND lower_band_cap_krw > middle_band_cap_krw
        AND middle_band_cap_krw > upper_band_cap_krw
    ),
    CONSTRAINT ck_credit_mortgage_policy_profile_dsr CHECK (
        borrower_dsr_balance_threshold_krw = 100000000
        AND bank_dsr_limit_ppm = 400000
        AND evaluation_horizon_months = 12
        AND full_term_fixed_stress_rate_bp = 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_credit_mortgage_policy_profile_draft_insert
BEFORE INSERT ON credit_mortgage_policy_profile
FOR EACH ROW
SET NEW.policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM policy_set AS policy
        WHERE policy.id = NEW.policy_set_id
          AND policy.policy_key = 'dev-unranked-m4c3-credit-policy-2026-v2'
          AND policy.sealed_at IS NULL
    ),
    NEW.policy_set_id,
    NULL
);

CREATE TRIGGER tr_credit_mortgage_policy_profile_no_update
BEFORE UPDATE ON credit_mortgage_policy_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'mortgage policy profiles are immutable';

CREATE TRIGGER tr_credit_mortgage_policy_profile_no_delete
BEFORE DELETE ON credit_mortgage_policy_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'mortgage policy profiles are immutable';

INSERT INTO credit_mortgage_policy_profile
    (
        policy_set_id, regulated_capital_ltv_limit_ppm,
        non_regulated_ltv_limit_ppm, lower_price_threshold_krw,
        upper_price_threshold_krw, lower_band_cap_krw, middle_band_cap_krw,
        upper_band_cap_krw, borrower_dsr_balance_threshold_krw,
        bank_dsr_limit_ppm, evaluation_horizon_months,
        full_term_fixed_stress_rate_bp
    )
SELECT policy.id, 400000, 700000, 1500000000, 2500000000,
       600000000, 400000000, 200000000, 100000000, 400000, 12, 0
FROM policy_set AS policy
WHERE policy.policy_key = 'dev-unranked-m4c3-credit-policy-2026-v2';

INSERT INTO policy_set_canonical_manifest (policy_set_id, canonical_json)
SELECT
    policy.id,
    CONCAT(
        '{"basisDate":', JSON_QUOTE(DATE_FORMAT(policy.basis_date, '%Y-%m-%d')),
        ',"policyKey":', JSON_QUOTE(policy.policy_key),
        ',"rankedEligible":', IF(policy.ranked_eligible, 'true', 'false'),
        ',"rules":[',
        (
            SELECT GROUP_CONCAT(
                CAST(JSON_OBJECT(
                    'domain', rule.domain,
                    'effectiveFrom', DATE_FORMAT(rule.effective_from, '%Y-%m-%d'),
                    'effectiveTo', IF(
                        rule.effective_to IS NULL,
                        NULL,
                        DATE_FORMAT(rule.effective_to, '%Y-%m-%d')
                    ),
                    'parameters', rule.parameters,
                    'ruleId', CAST(rule.id AS CHAR),
                    'ruleKey', rule.rule_key
                ) AS CHAR CHARACTER SET utf8mb4)
                ORDER BY rule.domain, rule.rule_key, rule.effective_from, rule.id
                SEPARATOR ','
            )
            FROM policy_rule AS rule
            WHERE rule.policy_set_id = policy.id
        ),
        '],"schemaVersion":1}'
    )
FROM policy_set AS policy
WHERE policy.policy_key = 'dev-unranked-m4c3-credit-policy-2026-v2';

UPDATE policy_set AS policy
INNER JOIN policy_set_canonical_manifest AS manifest
    ON manifest.policy_set_id = policy.id
INNER JOIN credit_mortgage_policy_profile AS profile
    ON profile.policy_set_id = policy.id
SET policy.canonical_sha256 = manifest.canonical_sha256,
    policy.sealed_at = CURRENT_TIMESTAMP(3)
WHERE policy.policy_key = 'dev-unranked-m4c3-credit-policy-2026-v2'
  AND policy.sealed_at IS NULL;

CREATE TABLE real_estate_purchase_profile (
    real_estate_model_version_id BIGINT UNSIGNED NOT NULL,
    purchase_capability          VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    maximum_active_holdings      TINYINT UNSIGNED NOT NULL,
    supported_offer_kind         VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    supported_purpose            VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    incidental_cost_ppm          INT UNSIGNED NOT NULL,
    minimum_incidental_cost_krw  BIGINT NOT NULL,
    collateral_value_rule        VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ltv_cost_treatment           VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    listing_consumption_scope    VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    provenance_kind              VARCHAR(20) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                   DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (real_estate_model_version_id),
    CONSTRAINT fk_real_estate_purchase_profile_model
        FOREIGN KEY (real_estate_model_version_id) REFERENCES real_estate_model_version (id),
    CONSTRAINT ck_real_estate_purchase_profile_fixture CHECK (
        purchase_capability = 'ownerOccupiedSingleHome'
        AND maximum_active_holdings = 1
        AND supported_offer_kind = 'sale'
        AND supported_purpose = 'ownerOccupied'
        AND incidental_cost_ppm = 10000
        AND minimum_incidental_cost_krw = 1
        AND collateral_value_rule = 'exactSalePriceAtExecution'
        AND ltv_cost_treatment = 'excludeIncidentalAndMoving'
        AND listing_consumption_scope = 'householdRunOnce'
        AND provenance_kind = 'GAME_BALANCE'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE real_estate_purchase_region_mapping (
    real_estate_model_version_id BIGINT UNSIGNED NOT NULL,
    region_key                   VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ltv_region_class             VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    mapping_provenance           VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                   DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (real_estate_model_version_id, region_key),
    CONSTRAINT fk_real_estate_purchase_region_profile
        FOREIGN KEY (real_estate_model_version_id, region_key)
        REFERENCES real_estate_region_profile (real_estate_model_version_id, region_key),
    CONSTRAINT ck_real_estate_purchase_region_class CHECK (
        ltv_region_class IN ('regulatedCapitalProxy', 'nonRegulatedProxy')
    ),
    CONSTRAINT ck_real_estate_purchase_region_mapping CHECK (
        (region_key = 'capitalArea' AND ltv_region_class = 'regulatedCapitalProxy')
        OR (region_key <> 'capitalArea' AND ltv_region_class = 'nonRegulatedProxy')
    ),
    CONSTRAINT ck_real_estate_purchase_region_provenance CHECK (
        mapping_provenance = 'GAME_MAPPING'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_real_estate_purchase_profile_draft_insert
BEFORE INSERT ON real_estate_purchase_profile
FOR EACH ROW
SET NEW.real_estate_model_version_id = IF(
    EXISTS (
        SELECT 1 FROM real_estate_model_version AS model
        WHERE model.id = NEW.real_estate_model_version_id
          AND model.availability = 'active'
          AND model.sealed_at IS NULL
          AND JSON_UNQUOTE(JSON_EXTRACT(model.parameters, '$.schemaVersion')) = '5'
    ),
    NEW.real_estate_model_version_id,
    NULL
);

CREATE TRIGGER tr_real_estate_purchase_profile_no_update
BEFORE UPDATE ON real_estate_purchase_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'real-estate purchase profiles are immutable';

CREATE TRIGGER tr_real_estate_purchase_profile_no_delete
BEFORE DELETE ON real_estate_purchase_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'real-estate purchase profiles are immutable';

CREATE TRIGGER tr_real_estate_purchase_region_draft_insert
BEFORE INSERT ON real_estate_purchase_region_mapping
FOR EACH ROW
SET NEW.real_estate_model_version_id = IF(
    EXISTS (
        SELECT 1
        FROM real_estate_purchase_profile AS profile
        INNER JOIN real_estate_model_version AS model
            ON model.id = profile.real_estate_model_version_id
        WHERE profile.real_estate_model_version_id = NEW.real_estate_model_version_id
          AND model.sealed_at IS NULL
    ),
    NEW.real_estate_model_version_id,
    NULL
);

CREATE TRIGGER tr_real_estate_purchase_region_no_update
BEFORE UPDATE ON real_estate_purchase_region_mapping
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'real-estate purchase mappings are immutable';

CREATE TRIGGER tr_real_estate_purchase_region_no_delete
BEFORE DELETE ON real_estate_purchase_region_mapping
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'real-estate purchase mappings are immutable';

-- Keep the exact v1-v4 expression as a legacy projection. The wrapper adds one explicit v5
-- branch, so adding the purchase child cannot reserialize any old manifest.
DROP TRIGGER tr_real_estate_model_manifest_draft_insert;
DROP TRIGGER tr_real_estate_model_version_seal_only;

RENAME TABLE real_estate_model_strict_projection
    TO real_estate_model_v1_v4_strict_projection;

CREATE VIEW real_estate_model_strict_projection AS
SELECT legacy.real_estate_model_version_id, legacy.canonical_json
FROM real_estate_model_v1_v4_strict_projection AS legacy
INNER JOIN real_estate_model_version AS model
    ON model.id = legacy.real_estate_model_version_id
WHERE JSON_TYPE(JSON_EXTRACT(model.parameters, '$.schemaVersion')) = 'INTEGER'
  AND JSON_UNQUOTE(JSON_EXTRACT(model.parameters, '$.schemaVersion')) IN ('1', '2', '3', '4')
UNION ALL
SELECT
    base.real_estate_model_version_id,
    CAST(
        JSON_MERGE_PATCH(
            CAST(base.canonical_json AS JSON),
            JSON_OBJECT(
                'leaseProfiles', COALESCE((
                    SELECT JSON_ARRAYAGG(JSON_OBJECT(
                               'arrearRepaymentRule', profile.arrear_repayment_rule,
                               'offerKind', profile.offer_kind,
                               'renewalNoticeLeadDays', profile.renewal_notice_lead_days,
                               'renewalRule', profile.renewal_rule,
                               'rentChargeRule', profile.rent_charge_rule,
                               'termMonths', profile.term_months,
                               'terminationReviewAfterDays',
                                   profile.termination_review_after_days,
                               'terminationReviewRule', profile.termination_review_rule
                           )) OVER (
                               ORDER BY profile.offer_kind
                               ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING
                           )
                    FROM real_estate_lease_profile AS profile
                    WHERE profile.real_estate_model_version_id
                            = base.real_estate_model_version_id
                    ORDER BY profile.offer_kind
                    LIMIT 1
                ), JSON_ARRAY()),
                'movingCosts', COALESCE((
                    SELECT JSON_ARRAYAGG(JSON_OBJECT(
                               'movingCostKrw', cost.moving_cost_krw,
                               'regionKey', cost.region_key
                           )) OVER (
                               ORDER BY region.region_order
                               ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING
                           )
                    FROM real_estate_region_moving_cost AS cost
                    INNER JOIN life_region AS region
                        ON BINARY region.region_key = BINARY cost.region_key
                    WHERE cost.real_estate_model_version_id
                            = base.real_estate_model_version_id
                    ORDER BY region.region_order
                    LIMIT 1
                ), JSON_ARRAY()),
                'purchaseProfile', (
                    SELECT JSON_OBJECT(
                               'collateralValueRule', profile.collateral_value_rule,
                               'incidentalCostPpm', profile.incidental_cost_ppm,
                               'listingConsumptionScope', profile.listing_consumption_scope,
                               'ltvCostTreatment', profile.ltv_cost_treatment,
                               'maximumActiveHoldings', profile.maximum_active_holdings,
                               'minimumIncidentalCostKrw',
                                   profile.minimum_incidental_cost_krw,
                               'provenanceKind', profile.provenance_kind,
                               'purchaseCapability', profile.purchase_capability,
                               'regionMappings', COALESCE((
                                   SELECT JSON_ARRAYAGG(JSON_OBJECT(
                                              'ltvRegionClass', mapping.ltv_region_class,
                                              'mappingProvenance', mapping.mapping_provenance,
                                              'regionKey', mapping.region_key
                                          )) OVER (
                                              ORDER BY region.region_order
                                              ROWS BETWEEN UNBOUNDED PRECEDING
                                                  AND UNBOUNDED FOLLOWING
                                          )
                                   FROM real_estate_purchase_region_mapping AS mapping
                                   INNER JOIN life_region AS region
                                       ON BINARY region.region_key = BINARY mapping.region_key
                                   WHERE mapping.real_estate_model_version_id
                                           = profile.real_estate_model_version_id
                                   ORDER BY region.region_order
                                   LIMIT 1
                               ), JSON_ARRAY()),
                               'schemaVersion', 1,
                               'supportedOfferKind', profile.supported_offer_kind,
                               'supportedPurpose', profile.supported_purpose
                           )
                    FROM real_estate_purchase_profile AS profile
                    WHERE profile.real_estate_model_version_id
                            = base.real_estate_model_version_id
                ),
                'schemaVersion', 5
            )
        ) AS CHAR CHARACTER SET utf8mb4
    ) AS canonical_json
FROM real_estate_model_v1_v4_strict_projection AS base
INNER JOIN real_estate_model_version AS model
    ON model.id = base.real_estate_model_version_id
WHERE JSON_TYPE(JSON_EXTRACT(model.parameters, '$.schemaVersion')) = 'INTEGER'
  AND JSON_UNQUOTE(JSON_EXTRACT(model.parameters, '$.schemaVersion')) = '5';

CREATE TEMPORARY TABLE m4c3_real_estate_manifest_guard (
    guard_key VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    accepted  TINYINT UNSIGNED NOT NULL,
    CONSTRAINT ck_m4c3_real_estate_manifest_guard CHECK (accepted = 1)
);

INSERT INTO m4c3_real_estate_manifest_guard (guard_key, accepted)
SELECT 'old-model-bytes', IF(
    NOT EXISTS (
        SELECT 1
        FROM real_estate_model_version AS model
        INNER JOIN real_estate_model_strict_manifest AS manifest
            ON manifest.real_estate_model_version_id = model.id
        LEFT JOIN real_estate_model_strict_projection AS projection
            ON projection.real_estate_model_version_id = model.id
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

DROP TEMPORARY TABLE m4c3_real_estate_manifest_guard;

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
                AND JSON_TYPE(JSON_EXTRACT(OLD.parameters, '$.schemaVersion')) = 'INTEGER'
                AND JSON_UNQUOTE(JSON_EXTRACT(OLD.parameters, '$.schemaVersion')) = '5'
                AND (
                    SELECT COUNT(*) FROM real_estate_region_profile AS profile
                    WHERE profile.real_estate_model_version_id = OLD.id
                ) = (SELECT COUNT(*) FROM life_region)
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
                    SELECT COUNT(*) FROM real_estate_lease_profile AS profile
                    WHERE profile.real_estate_model_version_id = OLD.id
                ) = 2
                AND EXISTS (
                    SELECT 1 FROM real_estate_lease_profile AS profile
                    WHERE profile.real_estate_model_version_id = OLD.id
                      AND profile.offer_kind = 'jeonse'
                      AND profile.renewal_rule = 'fixedTermAutoRenew'
                      AND profile.term_months = 12
                      AND profile.renewal_notice_lead_days = 30
                      AND profile.rent_charge_rule IS NULL
                      AND profile.arrear_repayment_rule IS NULL
                      AND profile.termination_review_rule IS NULL
                      AND profile.termination_review_after_days IS NULL
                )
                AND EXISTS (
                    SELECT 1 FROM real_estate_lease_profile AS profile
                    WHERE profile.real_estate_model_version_id = OLD.id
                      AND profile.offer_kind = 'monthlyRent'
                      AND profile.renewal_rule = 'fixedTermAutoRenew'
                      AND profile.term_months = 12
                      AND profile.renewal_notice_lead_days = 30
                      AND profile.rent_charge_rule = 'nextMonthStartFull'
                      AND profile.arrear_repayment_rule = 'manualOnly'
                      AND profile.termination_review_rule = 'oldestActiveArrearAge'
                      AND profile.termination_review_after_days = 60
                )
                AND (
                    SELECT COUNT(*) FROM real_estate_region_moving_cost AS cost
                    WHERE cost.real_estate_model_version_id = OLD.id
                ) = (SELECT COUNT(*) FROM life_region)
                AND EXISTS (
                    SELECT 1 FROM real_estate_purchase_profile AS profile
                    WHERE profile.real_estate_model_version_id = OLD.id
                )
                AND (
                    SELECT COUNT(*) FROM real_estate_purchase_region_mapping AS mapping
                    WHERE mapping.real_estate_model_version_id = OLD.id
                ) = (SELECT COUNT(*) FROM life_region)
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
        'dev-unranked-m4-real-estate-purchase-2026-v5',
        'active',
        FALSE,
        JSON_OBJECT(
            'entropyVersion', 'sha256-counter-be-v1',
            'generatorVersion', 'm4-c1-v1',
            'schemaVersion', 5
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
    ON source_model.version_key = 'dev-unranked-m4-real-estate-lifecycle-2026-v4'
INNER JOIN real_estate_region_profile AS source
    ON source.real_estate_model_version_id = source_model.id
WHERE target.version_key = 'dev-unranked-m4-real-estate-purchase-2026-v5';

INSERT INTO real_estate_region_property_type
    (real_estate_model_version_id, region_key, property_type, property_type_order)
SELECT target.id, source.region_key, source.property_type, source.property_type_order
FROM real_estate_model_version AS target
INNER JOIN real_estate_model_version AS source_model
    ON source_model.version_key = 'dev-unranked-m4-real-estate-lifecycle-2026-v4'
INNER JOIN real_estate_region_property_type AS source
    ON source.real_estate_model_version_id = source_model.id
WHERE target.version_key = 'dev-unranked-m4-real-estate-purchase-2026-v5';

INSERT INTO real_estate_lease_profile
    (
        real_estate_model_version_id, offer_kind, renewal_rule,
        term_months, renewal_notice_lead_days, rent_charge_rule,
        arrear_repayment_rule, termination_review_rule,
        termination_review_after_days
    )
SELECT target.id, source.offer_kind, source.renewal_rule,
       source.term_months, source.renewal_notice_lead_days,
       source.rent_charge_rule, source.arrear_repayment_rule,
       source.termination_review_rule, source.termination_review_after_days
FROM real_estate_model_version AS target
INNER JOIN real_estate_model_version AS source_model
    ON source_model.version_key = 'dev-unranked-m4-real-estate-lifecycle-2026-v4'
INNER JOIN real_estate_lease_profile AS source
    ON source.real_estate_model_version_id = source_model.id
WHERE target.version_key = 'dev-unranked-m4-real-estate-purchase-2026-v5';

INSERT INTO real_estate_region_moving_cost
    (real_estate_model_version_id, region_key, moving_cost_krw)
SELECT target.id, source.region_key, source.moving_cost_krw
FROM real_estate_model_version AS target
INNER JOIN real_estate_model_version AS source_model
    ON source_model.version_key = 'dev-unranked-m4-real-estate-lifecycle-2026-v4'
INNER JOIN real_estate_region_moving_cost AS source
    ON source.real_estate_model_version_id = source_model.id
WHERE target.version_key = 'dev-unranked-m4-real-estate-purchase-2026-v5';

INSERT INTO real_estate_purchase_profile
    (
        real_estate_model_version_id, purchase_capability, maximum_active_holdings,
        supported_offer_kind, supported_purpose, incidental_cost_ppm,
        minimum_incidental_cost_krw, collateral_value_rule, ltv_cost_treatment,
        listing_consumption_scope, provenance_kind
    )
SELECT model.id, 'ownerOccupiedSingleHome', 1, 'sale', 'ownerOccupied',
       10000, 1, 'exactSalePriceAtExecution', 'excludeIncidentalAndMoving',
       'householdRunOnce', 'GAME_BALANCE'
FROM real_estate_model_version AS model
WHERE model.version_key = 'dev-unranked-m4-real-estate-purchase-2026-v5';

INSERT INTO real_estate_purchase_region_mapping
    (
        real_estate_model_version_id, region_key,
        ltv_region_class, mapping_provenance
    )
SELECT model.id, region.region_key,
       IF(region.region_key = 'capitalArea',
          'regulatedCapitalProxy', 'nonRegulatedProxy'),
       'GAME_MAPPING'
FROM real_estate_model_version AS model
CROSS JOIN life_region AS region
WHERE model.version_key = 'dev-unranked-m4-real-estate-purchase-2026-v5';

INSERT INTO real_estate_model_strict_manifest
    (real_estate_model_version_id, canonical_json)
SELECT real_estate_model_version_id, canonical_json
FROM real_estate_model_strict_projection
WHERE real_estate_model_version_id = (
    SELECT id FROM real_estate_model_version
    WHERE version_key = 'dev-unranked-m4-real-estate-purchase-2026-v5'
);

UPDATE real_estate_model_version AS model
INNER JOIN real_estate_model_strict_manifest AS manifest
    ON manifest.real_estate_model_version_id = model.id
SET model.canonical_sha256 = manifest.canonical_sha256,
    model.sealed_at = CURRENT_TIMESTAMP(3)
WHERE model.version_key = 'dev-unranked-m4-real-estate-purchase-2026-v5'
  AND model.sealed_at IS NULL;

ALTER TABLE loan_product_version
    DROP CHECK ck_loan_product_collateral,
    DROP CHECK ck_loan_product_c2c_shape,
    DROP CHECK ck_loan_product_servicing_shape,
    ADD CONSTRAINT ck_loan_product_collateral CHECK (
        collateral_rule IN (
            'none', 'valuationUnavailable', 'leaseDepositFundingLimit',
            'mortgageLtv', 'notApplicable'
        )
    ),
    ADD CONSTRAINT ck_loan_product_c2c_shape CHECK (
        (
            product_kind NOT IN ('leaseDepositLoan', 'mortgage')
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
        OR (
            product_kind = 'mortgage'
            AND execution_channel = 'housingPurchase'
            AND funding_limit_ppm IS NULL
            AND affordability_rule IS NULL
            AND affordability_limit_ppm IS NULL
            AND regulatory_dsr_treatment = 'includedFullTermFixed'
            AND collateral_rule = 'mortgageLtv'
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
            catalog_scope = 'modelChild'
            AND credit_model_version_id IS NOT NULL
            AND product_kind = 'mortgage'
            AND lender_sector = 'bank'
            AND rate_status = 'available'
            AND rate_type = 'fixed'
            AND fixed_annual_rate_bp = 400
            AND day_count_rule = 'actual365'
            AND repayment_method = 'levelPayment'
            AND term_months = 360
            AND payment_calendar = 'monthEnd'
            AND grace_months = 0
            AND minimum_principal_krw = 1
            AND maximum_principal_krw = 600000000
            AND prepayment_fee_ppm = 10000
            AND prepayment_effect = 'recalculatePayment'
            AND starting_eligible = FALSE
            AND quote_eligible = TRUE
            AND execution_eligible = TRUE
            AND prepayment_allowed = TRUE
            AND dsr_included = TRUE
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

DROP TRIGGER tr_loan_product_manifest_draft_insert;
DROP TRIGGER tr_loan_product_seal_only;

RENAME TABLE loan_product_canonical_projection
    TO loan_product_v1_v2_canonical_projection;

CREATE VIEW loan_product_canonical_projection AS
SELECT legacy.loan_product_version_id, legacy.canonical_json
FROM loan_product_v1_v2_canonical_projection AS legacy
INNER JOIN loan_product_version AS product
    ON product.id = legacy.loan_product_version_id
WHERE product.product_kind <> 'mortgage'
UNION ALL
SELECT
    legacy.loan_product_version_id,
    CAST(
        JSON_MERGE_PATCH(
            CAST(legacy.canonical_json AS JSON),
            JSON_OBJECT(
                'executionChannel', product.execution_channel,
                'regulatoryDsrTreatment', product.regulatory_dsr_treatment,
                'schemaVersion', 3
            )
        ) AS CHAR CHARACTER SET utf8mb4
    ) AS canonical_json
FROM loan_product_v1_v2_canonical_projection AS legacy
INNER JOIN loan_product_version AS product
    ON product.id = legacy.loan_product_version_id
WHERE product.product_kind = 'mortgage';

CREATE TEMPORARY TABLE m4c3_product_manifest_guard (
    guard_key VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    accepted  TINYINT UNSIGNED NOT NULL,
    CONSTRAINT ck_m4c3_product_manifest_guard CHECK (accepted = 1)
);

INSERT INTO m4c3_product_manifest_guard (guard_key, accepted)
SELECT 'old-product-bytes', IF(
    NOT EXISTS (
        SELECT 1
        FROM loan_product_version AS product
        INNER JOIN loan_product_canonical_manifest AS manifest
            ON manifest.loan_product_version_id = product.id
        LEFT JOIN loan_product_canonical_projection AS projection
            ON projection.loan_product_version_id = product.id
        WHERE product.sealed_at IS NOT NULL
          AND product.product_kind <> 'mortgage'
          AND (
              projection.loan_product_version_id IS NULL
              OR BINARY manifest.canonical_json <> BINARY projection.canonical_json
              OR BINARY manifest.canonical_sha256 <> BINARY product.canonical_sha256
          )
    ),
    1,
    0
);

DROP TEMPORARY TABLE m4c3_product_manifest_guard;

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

-- v4 is the only credit branch that carries the typed mortgage-policy projection. The complete
-- profile is embedded rather than merely naming its policy set, sealing every decision input.
DROP TRIGGER tr_credit_model_manifest_draft_insert;
DROP TRIGGER tr_credit_model_version_seal_only;

RENAME TABLE credit_model_strict_projection
    TO credit_model_v1_v3_strict_projection;

CREATE VIEW credit_model_strict_projection AS
SELECT legacy.credit_model_version_id, legacy.canonical_json
FROM credit_model_v1_v3_strict_projection AS legacy
INNER JOIN credit_model_version AS model
    ON model.id = legacy.credit_model_version_id
WHERE JSON_TYPE(JSON_EXTRACT(model.parameters, '$.schemaVersion')) = 'INTEGER'
  AND JSON_UNQUOTE(JSON_EXTRACT(model.parameters, '$.schemaVersion')) IN ('2', '3', '4')
UNION ALL
SELECT
    legacy.credit_model_version_id,
    CAST(
        JSON_MERGE_PATCH(
            CAST(legacy.canonical_json AS JSON),
            JSON_OBJECT(
                'mortgagePolicyProfile', JSON_OBJECT(
                    'bankDsrLimitPpm', profile.bank_dsr_limit_ppm,
                    'borrowerDsrBalanceThresholdKrw',
                        profile.borrower_dsr_balance_threshold_krw,
                    'evaluationHorizonMonths', profile.evaluation_horizon_months,
                    'fullTermFixedStressRateBp',
                        profile.full_term_fixed_stress_rate_bp,
                    'lowerBandCapKrw', profile.lower_band_cap_krw,
                    'lowerPriceThresholdKrw', profile.lower_price_threshold_krw,
                    'middleBandCapKrw', profile.middle_band_cap_krw,
                    'nonRegulatedLtvLimitPpm', profile.non_regulated_ltv_limit_ppm,
                    'regulatedCapitalLtvLimitPpm',
                        profile.regulated_capital_ltv_limit_ppm,
                    'schemaVersion', 1,
                    'upperBandCapKrw', profile.upper_band_cap_krw,
                    'upperPriceThresholdKrw', profile.upper_price_threshold_krw
                ),
                'schemaVersion', 3
            )
        ) AS CHAR CHARACTER SET utf8mb4
    ) AS canonical_json
FROM credit_model_v1_v3_strict_projection AS legacy
INNER JOIN credit_model_version AS model
    ON model.id = legacy.credit_model_version_id
INNER JOIN credit_mortgage_policy_profile AS profile
    ON profile.policy_set_id = model.credit_policy_set_id
WHERE JSON_TYPE(JSON_EXTRACT(model.parameters, '$.schemaVersion')) = 'INTEGER'
  AND JSON_UNQUOTE(JSON_EXTRACT(model.parameters, '$.schemaVersion')) = '5';

CREATE TEMPORARY TABLE m4c3_credit_manifest_guard (
    guard_key VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    accepted  TINYINT UNSIGNED NOT NULL,
    CONSTRAINT ck_m4c3_credit_manifest_guard CHECK (accepted = 1)
);

INSERT INTO m4c3_credit_manifest_guard (guard_key, accepted)
SELECT 'old-credit-bytes', IF(
    NOT EXISTS (
        SELECT 1
        FROM credit_model_version AS model
        INNER JOIN credit_model_strict_manifest AS manifest
            ON manifest.credit_model_version_id = model.id
        LEFT JOIN credit_model_strict_projection AS projection
            ON projection.credit_model_version_id = model.id
        WHERE model.version_key IN (
                  'dev-unranked-m4b-credit-2026-v1',
                  'dev-unranked-m4b-credit-2026-v2',
                  'dev-unranked-m4c2c-credit-2026-v3'
              )
          AND (
              projection.credit_model_version_id IS NULL
              OR BINARY manifest.canonical_json <> BINARY projection.canonical_json
              OR BINARY manifest.canonical_sha256 <> BINARY model.canonical_sha256
          )
    ),
    1,
    0
);

DROP TEMPORARY TABLE m4c3_credit_manifest_guard;

CREATE TRIGGER tr_credit_model_manifest_draft_insert
BEFORE INSERT ON credit_model_strict_manifest
FOR EACH ROW
SET NEW.credit_model_version_id = IF(
    JSON_VALID(NEW.canonical_json)
        AND EXISTS (
            SELECT 1
            FROM credit_model_version AS model
            INNER JOIN credit_model_strict_projection AS projection
                ON projection.credit_model_version_id = model.id
            WHERE model.id = NEW.credit_model_version_id
              AND model.availability = 'active'
              AND model.sealed_at IS NULL
              AND BINARY projection.canonical_json = BINARY NEW.canonical_json
        ),
    NEW.credit_model_version_id,
    NULL
);

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
                AND BINARY OLD.version_key
                    = BINARY 'dev-unranked-m4c3-credit-2026-v4'
                AND EXISTS (
                    SELECT 1
                    FROM policy_set AS policy
                    INNER JOIN credit_mortgage_policy_profile AS profile
                        ON profile.policy_set_id = policy.id
                    WHERE policy.id = OLD.credit_policy_set_id
                      AND policy.policy_key
                            = 'dev-unranked-m4c3-credit-policy-2026-v2'
                      AND policy.sealed_at IS NOT NULL
                      AND policy.ranked_eligible = FALSE
                )
                AND (SELECT COUNT(*) FROM loan_product_version AS product
                     WHERE product.credit_model_version_id = OLD.id
                       AND product.catalog_scope = 'modelChild'
                       AND product.sealed_at IS NOT NULL) = 4
                AND (SELECT COUNT(*) FROM loan_product_version AS product
                     WHERE product.credit_model_version_id = OLD.id
                       AND product.product_kind = 'leaseDepositLoan'
                       AND product.sealed_at IS NOT NULL) = 1
                AND (SELECT COUNT(*) FROM loan_product_version AS product
                     WHERE product.credit_model_version_id = OLD.id
                       AND product.product_kind = 'mortgage'
                       AND product.sealed_at IS NOT NULL) = 1
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

INSERT INTO credit_model_version
    (version_key, availability, ranked_eligible, credit_policy_set_id, parameters)
SELECT
    'dev-unranked-m4c3-credit-2026-v4',
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
            ),
            'mortgage', JSON_OBJECT(
                'allowedCreditBands', JSON_ARRAY('prime', 'standard'),
                'disallowedContractStatuses',
                    JSON_ARRAY('delinquent', 'defaulted', 'restructured'),
                'maximumActiveContracts', 8,
                'maximumActiveHoldings', 0
            )
        ),
        'provenance', 'GAME_BALANCE',
        'schemaVersion', 5
    )
FROM policy_set AS policy
WHERE policy.policy_key = 'dev-unranked-m4c3-credit-policy-2026-v2'
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
    model.id, 'dev-student-fixed-equal-principal-2026-v4',
    '개발 학자금 고정금리 대출', 'modelChild', 'studentLoan', 'bank', 'available',
    'fixed', NULL, 170, NULL, 170, 170, 'none', 'actual365', 'equalPrincipal',
    120, 'monthEnd', 0, 1, 50000000, 0, 'reduceTerm', 'none',
    NULL, NULL, NULL, NULL, NULL,
    TRUE, FALSE, FALSE, TRUE, TRUE, FALSE, 'GAME_BALANCE', 1
FROM credit_model_version AS model
WHERE model.version_key = 'dev-unranked-m4c3-credit-2026-v4'
UNION ALL
SELECT
    model.id, 'dev-unsecured-variable-level-payment-2026-v4',
    '개발 변동금리 신용대출', 'modelChild', 'unsecuredLoan', 'bank', 'available',
    'variable', 'treasury3m', NULL, 400, 300, 1500, 'monthlyDay1', 'actual365',
    'levelPayment', 60, 'monthEnd', 0, 1, 200000000, 10000, 'recalculatePayment',
    'none', NULL, NULL, NULL, NULL, NULL,
    TRUE, TRUE, TRUE, TRUE, TRUE, FALSE, 'GAME_BALANCE', 2
FROM credit_model_version AS model
WHERE model.version_key = 'dev-unranked-m4c3-credit-2026-v4'
UNION ALL
SELECT
    model.id, 'dev-lease-deposit-fixed-bullet-2026-v2',
    '개발 전세보증금 고정금리 대출', 'modelChild', 'leaseDepositLoan', 'bank',
    'available', 'fixed', NULL, 400, NULL, 400, 400, 'none', 'actual365',
    'bullet', 24, 'monthEnd', 0, 1, 400000000, 0, 'reduceTerm',
    'leaseDepositFundingLimit', 'leaseMove', 800000, 'interestOnly', 400000,
    'excludedNoOwnedHome',
    FALSE, TRUE, TRUE, TRUE, FALSE, FALSE, 'GAME_BALANCE', 3
FROM credit_model_version AS model
WHERE model.version_key = 'dev-unranked-m4c3-credit-2026-v4'
UNION ALL
SELECT
    model.id, 'dev-mortgage-fixed-level-payment-2026-v1',
    '개발 주택담보 고정금리 대출', 'modelChild', 'mortgage', 'bank',
    'available', 'fixed', NULL, 400, NULL, 400, 400, 'none', 'actual365',
    'levelPayment', 360, 'monthEnd', 0, 1, 600000000, 10000,
    'recalculatePayment', 'mortgageLtv', 'housingPurchase', NULL, NULL, NULL,
    'includedFullTermFixed',
    FALSE, TRUE, TRUE, TRUE, TRUE, FALSE, 'GAME_BALANCE', 4
FROM credit_model_version AS model
WHERE model.version_key = 'dev-unranked-m4c3-credit-2026-v4';

INSERT INTO loan_product_canonical_manifest (loan_product_version_id, canonical_json)
SELECT projection.loan_product_version_id, projection.canonical_json
FROM loan_product_canonical_projection AS projection
INNER JOIN loan_product_version AS product
    ON product.id = projection.loan_product_version_id
WHERE product.credit_model_version_id = (
    SELECT id FROM credit_model_version
    WHERE version_key = 'dev-unranked-m4c3-credit-2026-v4'
);

UPDATE loan_product_version AS product
INNER JOIN loan_product_canonical_manifest AS manifest
    ON manifest.loan_product_version_id = product.id
SET product.canonical_sha256 = manifest.canonical_sha256,
    product.sealed_at = CURRENT_TIMESTAMP(3)
WHERE product.credit_model_version_id = (
    SELECT id FROM credit_model_version
    WHERE version_key = 'dev-unranked-m4c3-credit-2026-v4'
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
WHERE model.version_key = 'dev-unranked-m4c3-credit-2026-v4'
UNION ALL
SELECT model.id, 'creditLoanKrw', 'unsecuredLoan', product.id, 2
FROM credit_model_version AS model
INNER JOIN loan_product_version AS product
    ON product.credit_model_version_id = model.id
   AND product.product_kind = 'unsecuredLoan'
WHERE model.version_key = 'dev-unranked-m4c3-credit-2026-v4';

INSERT INTO credit_model_strict_manifest (credit_model_version_id, canonical_json)
SELECT credit_model_version_id, canonical_json
FROM credit_model_strict_projection
WHERE credit_model_version_id = (
    SELECT id FROM credit_model_version
    WHERE version_key = 'dev-unranked-m4c3-credit-2026-v4'
);

UPDATE credit_model_version AS model
INNER JOIN credit_model_strict_manifest AS manifest
    ON manifest.credit_model_version_id = model.id
SET model.canonical_sha256 = manifest.canonical_sha256,
    model.sealed_at = CURRENT_TIMESTAMP(3)
WHERE model.version_key = 'dev-unranked-m4c3-credit-2026-v4'
  AND model.sealed_at IS NULL;

ALTER TABLE save
    ADD COLUMN property_book_value_krw BIGINT NOT NULL DEFAULT 0 AFTER debt_krw,
    ADD CONSTRAINT ck_save_property_book_value CHECK (property_book_value_krw >= 0);

ALTER TABLE run_rule_bundle
    ADD UNIQUE KEY uk_run_rule_bundle_real_estate_pin
        (save_id, run_revision, real_estate_model_version_id);

CREATE TABLE property_holding (
    id                                   BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                              BIGINT UNSIGNED NOT NULL,
    run_revision                         INT UNSIGNED NOT NULL,
    household_id                         BIGINT UNSIGNED NOT NULL,
    property_listing_id                  BIGINT UNSIGNED NOT NULL,
    real_estate_model_version_id          BIGINT UNSIGNED NOT NULL,
    acquisition_policy_set_id             BIGINT UNSIGNED NOT NULL,
    acquisition_credit_policy_set_id      BIGINT UNSIGNED NOT NULL,
    acquisition_command_id                CHAR(36) CHARACTER SET ascii COLLATE ascii_bin
                                             NOT NULL,
    status                               VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin
                                             NOT NULL,
    purpose                              VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin
                                             NOT NULL,
    region_key                           VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin
                                             NOT NULL,
    property_type                        VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin
                                             NOT NULL,
    exclusive_area_square_meters         SMALLINT UNSIGNED NOT NULL,
    acquired_game_day                    INT UNSIGNED NOT NULL,
    disposed_game_day                    INT UNSIGNED NULL,
    acquisition_price_krw                BIGINT NOT NULL,
    acquisition_incidental_cost_krw      BIGINT NOT NULL,
    book_value_krw                       BIGINT NOT NULL,
    active_holding_slot                  TINYINT UNSIGNED GENERATED ALWAYS AS (
        CASE WHEN status = 'active' THEN 1 ELSE NULL END
    ) STORED,
    created_at                           DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                           DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_property_holding_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_property_holding_household_listing
        (save_id, run_revision, household_id, property_listing_id),
    UNIQUE KEY uk_property_holding_active (household_id, active_holding_slot),
    UNIQUE KEY uk_property_holding_command
        (save_id, run_revision, acquisition_command_id),
    KEY ix_property_holding_household_status (household_id, status, id),
    KEY ix_property_holding_listing (property_listing_id),
    KEY ix_property_holding_real_estate_model (real_estate_model_version_id),
    KEY ix_property_holding_acquisition_policy (acquisition_policy_set_id),
    KEY ix_property_holding_acquisition_credit_policy
        (acquisition_credit_policy_set_id),
    CONSTRAINT fk_property_holding_household
        FOREIGN KEY (save_id, run_revision, household_id)
        REFERENCES household (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_property_holding_bundle
        FOREIGN KEY (save_id, run_revision, real_estate_model_version_id)
        REFERENCES run_rule_bundle (save_id, run_revision, real_estate_model_version_id)
        ON DELETE CASCADE,
    CONSTRAINT fk_property_holding_listing
        FOREIGN KEY (property_listing_id) REFERENCES property_listing (id),
    CONSTRAINT fk_property_holding_acquisition_policy
        FOREIGN KEY (acquisition_policy_set_id) REFERENCES policy_set (id),
    CONSTRAINT fk_property_holding_acquisition_credit_policy
        FOREIGN KEY (acquisition_credit_policy_set_id) REFERENCES policy_set (id),
    CONSTRAINT fk_property_holding_command
        FOREIGN KEY (save_id, acquisition_command_id)
        REFERENCES command_identity (save_id, command_id),
    CONSTRAINT ck_property_holding_command CHECK (
        acquisition_command_id REGEXP
            '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
    ),
    CONSTRAINT ck_property_holding_status CHECK (status IN ('active', 'disposed')),
    CONSTRAINT ck_property_holding_purpose CHECK (purpose = 'ownerOccupied'),
    CONSTRAINT ck_property_holding_property_type CHECK (
        property_type IN ('apartment', 'multiFamily', 'detached')
    ),
    CONSTRAINT ck_property_holding_area CHECK (
        exclusive_area_square_meters BETWEEN 1 AND 10000
    ),
    CONSTRAINT ck_property_holding_period CHECK (
        (status = 'active' AND disposed_game_day IS NULL)
        OR (
            status = 'disposed'
            AND disposed_game_day IS NOT NULL
            AND disposed_game_day >= acquired_game_day
        )
    ),
    CONSTRAINT ck_property_holding_acquisition_values CHECK (
        acquisition_price_krw BETWEEN 1 AND 9007199254740991
        AND acquisition_incidental_cost_krw >= 1
        AND book_value_krw = acquisition_price_krw
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_property_holding_valid_insert
BEFORE INSERT ON property_holding
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'active'
        AND NEW.disposed_game_day IS NULL
        AND NEW.purpose = 'ownerOccupied'
        AND NEW.book_value_krw = NEW.acquisition_price_krw
        AND EXISTS (
            SELECT 1
            FROM save
            INNER JOIN household
                ON household.save_id = save.id
               AND household.run_revision = save.run_revision
               AND household.id = NEW.household_id
            INNER JOIN run_rule_bundle AS bundle
                ON bundle.save_id = save.id
               AND bundle.run_revision = save.run_revision
               AND bundle.real_estate_model_version_id
                    = NEW.real_estate_model_version_id
            INNER JOIN real_estate_model_version AS real_estate
                ON real_estate.id = bundle.real_estate_model_version_id
               AND real_estate.version_key
                    = 'dev-unranked-m4-real-estate-purchase-2026-v5'
               AND real_estate.availability = 'active'
               AND real_estate.sealed_at IS NOT NULL
            INNER JOIN real_estate_model_strict_manifest AS manifest
                ON manifest.real_estate_model_version_id = real_estate.id
            INNER JOIN real_estate_model_strict_projection AS projection
                ON projection.real_estate_model_version_id = real_estate.id
            INNER JOIN real_estate_purchase_profile AS purchase_profile
                ON purchase_profile.real_estate_model_version_id = real_estate.id
               AND purchase_profile.purchase_capability = 'ownerOccupiedSingleHome'
               AND purchase_profile.maximum_active_holdings = 1
            INNER JOIN credit_model_version AS credit
                ON credit.id = bundle.credit_model_version_id
               AND credit.version_key = 'dev-unranked-m4c3-credit-2026-v4'
               AND credit.sealed_at IS NOT NULL
               AND credit.credit_policy_set_id = NEW.acquisition_credit_policy_set_id
            INNER JOIN credit_mortgage_policy_profile AS mortgage_policy
                ON mortgage_policy.policy_set_id = credit.credit_policy_set_id
            INNER JOIN property_listing AS listing
                ON listing.id = NEW.property_listing_id
               AND listing.market_world_id = bundle.market_world_id
               AND listing.real_estate_model_version_id = real_estate.id
            INNER JOIN property_listing_offer AS offer
                ON offer.property_listing_id = listing.id
               AND offer.offer_kind = 'sale'
            INNER JOIN command_identity AS identity
                ON identity.save_id = save.id
               AND BINARY identity.command_id = BINARY NEW.acquisition_command_id
               AND identity.command_kind = 'purchaseProperty'
            WHERE save.id = NEW.save_id
              AND save.run_revision = NEW.run_revision
              AND bundle.policy_set_id = NEW.acquisition_policy_set_id
              AND BINARY manifest.canonical_sha256 = BINARY real_estate.canonical_sha256
              AND BINARY manifest.canonical_json = BINARY projection.canonical_json
              AND listing.available_from_game_day <= save.game_day
              AND listing.available_to_game_day >= save.game_day
              AND BINARY listing.region_key = BINARY NEW.region_key
              AND BINARY listing.property_type = BINARY NEW.property_type
              AND listing.exclusive_area_square_meters
                    = NEW.exclusive_area_square_meters
              AND offer.price_krw = NEW.acquisition_price_krw
              AND offer.deposit_krw IS NULL
              AND offer.monthly_rent_krw IS NULL
              AND NEW.acquisition_incidental_cost_krw = GREATEST(
                  FLOOR(
                      CAST(offer.price_krw AS DECIMAL(65, 0))
                      * purchase_profile.incidental_cost_ppm / 1000000
                  ),
                  purchase_profile.minimum_incidental_cost_krw
              )
              AND NEW.acquired_game_day = save.game_day
              AND identity.initial_run_revision = save.run_revision
              AND identity.initial_state_revision = save.state_revision
              AND identity.initial_game_day = save.game_day
              AND NOT EXISTS (
                  SELECT 1 FROM property_holding AS active_holding
                  WHERE active_holding.household_id = household.id
                    AND active_holding.status = 'active'
              )
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_property_holding_transition_only
BEFORE UPDATE ON property_holding
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'active'
        AND NEW.status = 'disposed'
        AND NEW.disposed_game_day IS NOT NULL
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.household_id = OLD.household_id
        AND NEW.property_listing_id = OLD.property_listing_id
        AND NEW.real_estate_model_version_id = OLD.real_estate_model_version_id
        AND NEW.acquisition_policy_set_id = OLD.acquisition_policy_set_id
        AND NEW.acquisition_credit_policy_set_id
              = OLD.acquisition_credit_policy_set_id
        AND BINARY NEW.acquisition_command_id = BINARY OLD.acquisition_command_id
        AND BINARY NEW.purpose = BINARY OLD.purpose
        AND BINARY NEW.region_key = BINARY OLD.region_key
        AND BINARY NEW.property_type = BINARY OLD.property_type
        AND NEW.exclusive_area_square_meters = OLD.exclusive_area_square_meters
        AND NEW.acquired_game_day = OLD.acquired_game_day
        AND NEW.acquisition_price_krw = OLD.acquisition_price_krw
        AND NEW.acquisition_incidental_cost_krw
              = OLD.acquisition_incidental_cost_krw
        AND NEW.book_value_krw = OLD.book_value_krw
        AND NEW.created_at = OLD.created_at
        AND EXISTS (
            SELECT 1 FROM save
            WHERE save.id = OLD.save_id
              AND save.run_revision = OLD.run_revision
              AND save.game_day = NEW.disposed_game_day
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_property_holding_no_delete
BEFORE DELETE ON property_holding
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property holding history is immutable';

ALTER TABLE residence
    DROP CHECK ck_residence_lease_shape,
    ADD COLUMN property_holding_id BIGINT UNSIGNED NULL AFTER lease_contract_id,
    ADD UNIQUE KEY uk_residence_property_holding
        (save_id, run_revision, property_holding_id),
    ADD CONSTRAINT fk_residence_property_holding
        FOREIGN KEY (save_id, run_revision, property_holding_id)
        REFERENCES property_holding (save_id, run_revision, id),
    ADD CONSTRAINT ck_residence_lease_shape CHECK (
        (tenure_type = 'owner' AND lease_contract_id IS NULL)
        OR (
            tenure_type IN ('jeonse', 'monthlyRent')
            AND lease_contract_id IS NOT NULL
            AND property_holding_id IS NULL
        )
        OR (
            tenure_type = 'rentFree'
            AND lease_contract_id IS NULL
            AND property_holding_id IS NULL
        )
    );

DROP TRIGGER tr_residence_lease_valid_insert;

CREATE TRIGGER tr_residence_lease_valid_insert
BEFORE INSERT ON residence
FOR EACH ROW
SET NEW.save_id = IF(
    (
        NEW.tenure_type = 'rentFree'
        AND NEW.lease_contract_id IS NULL
        AND NEW.property_holding_id IS NULL
    )
    OR (
        NEW.tenure_type IN ('jeonse', 'monthlyRent')
        AND NEW.lease_contract_id IS NOT NULL
        AND NEW.property_holding_id IS NULL
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
    )
    OR (
        NEW.tenure_type = 'owner'
        AND NEW.lease_contract_id IS NULL
        AND NEW.property_holding_id IS NOT NULL
        AND EXISTS (
            SELECT 1
            FROM property_holding AS holding
            INNER JOIN run_rule_bundle AS bundle
                ON bundle.save_id = holding.save_id
               AND bundle.run_revision = holding.run_revision
               AND bundle.real_estate_model_version_id
                    = holding.real_estate_model_version_id
            INNER JOIN real_estate_model_version AS model
                ON model.id = bundle.real_estate_model_version_id
            WHERE holding.id = NEW.property_holding_id
              AND holding.save_id = NEW.save_id
              AND holding.run_revision = NEW.run_revision
              AND holding.household_id = NEW.household_id
              AND holding.status = 'active'
              AND holding.purpose = 'ownerOccupied'
              AND BINARY holding.region_key = BINARY NEW.region_key
              AND holding.acquired_game_day = NEW.effective_from_game_day
              AND model.version_key
                    = 'dev-unranked-m4-real-estate-purchase-2026-v5'
        )
    )
    OR (
        -- Compatibility runs may retain their pre-C3 owner row without fabricating a holding.
        NEW.tenure_type = 'owner'
        AND NEW.lease_contract_id IS NULL
        AND NEW.property_holding_id IS NULL
        AND EXISTS (
            SELECT 1
            FROM run_rule_bundle AS bundle
            INNER JOIN real_estate_model_version AS model
                ON model.id = bundle.real_estate_model_version_id
            WHERE bundle.save_id = NEW.save_id
              AND bundle.run_revision = NEW.run_revision
              AND model.version_key
                    <> 'dev-unranked-m4-real-estate-purchase-2026-v5'
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
        AND NEW.property_holding_id <=> OLD.property_holding_id
        AND NEW.effective_from_game_day = OLD.effective_from_game_day
        AND NEW.created_at = OLD.created_at,
    OLD.id,
    NULL
);

DROP TRIGGER tr_loan_quote_valid_insert;

ALTER TABLE loan_quote
    DROP CHECK ck_loan_quote_amounts,
    DROP CHECK ck_loan_quote_dsr_shape,
    DROP CHECK ck_loan_quote_affordability_shape,
    DROP CHECK ck_loan_quote_purpose_shape,
    DROP CHECK ck_loan_quote_decision,
    ADD COLUMN current_lease_contract_id BIGINT UNSIGNED NULL
        AFTER property_listing_id,
    ADD COLUMN recognized_collateral_value_krw BIGINT NULL
        AFTER current_lease_contract_id,
    ADD COLUMN ltv_region_class
        VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NULL
        AFTER recognized_collateral_value_krw,
    ADD COLUMN ltv_limit_ppm INT UNSIGNED NULL AFTER ltv_region_class,
    ADD COLUMN maximum_mortgage_krw BIGINT NULL AFTER ltv_limit_ppm,
    ADD COLUMN ltv_numerator_krw BIGINT NULL AFTER maximum_mortgage_krw,
    ADD COLUMN ltv_denominator_krw BIGINT NULL AFTER ltv_numerator_krw,
    ADD COLUMN ltv_ratio_ppm BIGINT NULL AFTER ltv_denominator_krw,
    ADD COLUMN acquisition_incidental_cost_krw BIGINT NULL AFTER ltv_ratio_ppm,
    ADD COLUMN moving_cost_krw BIGINT NULL AFTER acquisition_incidental_cost_krw,
    ADD COLUMN returned_deposit_krw BIGINT NULL AFTER moving_cost_krw,
    ADD COLUMN available_buyer_cash_krw BIGINT NULL AFTER returned_deposit_krw,
    ADD COLUMN required_buyer_cash_krw BIGINT NULL AFTER available_buyer_cash_krw,
    ADD COLUMN stress_treatment
        VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NULL AFTER stress_rate_bp,
    ADD KEY ix_loan_quote_current_lease
        (save_id, run_revision, current_lease_contract_id),
    ADD CONSTRAINT fk_loan_quote_current_lease
        FOREIGN KEY (save_id, run_revision, current_lease_contract_id)
        REFERENCES lease_contract (save_id, run_revision, id),
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
                purpose IN ('leaseDeposit', 'mortgagePurchase')
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
            purpose IN ('unsecured', 'mortgagePurchase')
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
            purpose IN ('unsecured', 'mortgagePurchase')
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
                (decision_code = 'eligible'
                 AND affordability_ratio_ppm <= affordability_limit_ppm)
                OR (decision_code = 'affordabilityLimit'
                    AND affordability_ratio_ppm > affordability_limit_ppm)
            )
        )
    ),
    ADD CONSTRAINT ck_loan_quote_purpose_shape CHECK (
        (
            purpose = 'unsecured'
            AND property_listing_id IS NULL
            AND current_lease_contract_id IS NULL
            AND recognized_collateral_value_krw IS NULL
            AND ltv_region_class IS NULL
            AND ltv_limit_ppm IS NULL
            AND maximum_mortgage_krw IS NULL
            AND ltv_numerator_krw IS NULL
            AND ltv_denominator_krw IS NULL
            AND ltv_ratio_ppm IS NULL
            AND acquisition_incidental_cost_krw IS NULL
            AND moving_cost_krw IS NULL
            AND returned_deposit_krw IS NULL
            AND available_buyer_cash_krw IS NULL
            AND required_buyer_cash_krw IS NULL
            AND lease_deposit_krw IS NULL
            AND funding_limit_ppm IS NULL
            AND maximum_funding_krw IS NULL
            AND replaced_loan_contract_id IS NULL
            AND replaced_loan_principal_krw = 0
            AND regulatory_dsr_applied IS NULL
            AND stress_treatment IS NULL
        )
        OR (
            purpose = 'leaseDeposit'
            AND property_listing_id IS NOT NULL
            AND current_lease_contract_id IS NULL
            AND recognized_collateral_value_krw IS NULL
            AND ltv_region_class IS NULL
            AND ltv_limit_ppm IS NULL
            AND maximum_mortgage_krw IS NULL
            AND ltv_numerator_krw IS NULL
            AND ltv_denominator_krw IS NULL
            AND ltv_ratio_ppm IS NULL
            AND acquisition_incidental_cost_krw IS NULL
            AND moving_cost_krw IS NULL
            AND returned_deposit_krw IS NULL
            AND available_buyer_cash_krw IS NULL
            AND required_buyer_cash_krw IS NULL
            AND lease_deposit_krw > 0
            AND funding_limit_ppm BETWEEN 1 AND 1000000
            AND maximum_funding_krw > 0
            AND maximum_funding_krw <= lease_deposit_krw
            AND (
                (replaced_loan_contract_id IS NULL AND replaced_loan_principal_krw = 0)
                OR (replaced_loan_contract_id IS NOT NULL
                    AND replaced_loan_principal_krw > 0)
            )
            AND regulatory_dsr_applied = FALSE
            AND dsr_numerator_krw IS NULL
            AND dsr_denominator_krw IS NULL
            AND dsr_ratio_ppm IS NULL
            AND dsr_limit_ppm IS NULL
            AND stress_rate_bp = 0
            AND stress_treatment IS NULL
        )
        OR (
            purpose = 'mortgagePurchase'
            AND property_listing_id IS NOT NULL
            AND recognized_collateral_value_krw > 0
            AND ltv_region_class IN ('regulatedCapitalProxy', 'nonRegulatedProxy')
            AND ltv_limit_ppm BETWEEN 1 AND 1000000
            AND maximum_mortgage_krw > 0
            AND ltv_numerator_krw = requested_principal_krw
            AND ltv_denominator_krw = recognized_collateral_value_krw
            AND ltv_ratio_ppm = FLOOR(
                CAST(ltv_numerator_krw AS DECIMAL(65, 0)) * 1000000
                / ltv_denominator_krw
            )
            AND acquisition_incidental_cost_krw >= 1
            AND moving_cost_krw >= 1
            AND returned_deposit_krw >= 0
            AND available_buyer_cash_krw >= 0
            AND required_buyer_cash_krw >= 0
            AND lease_deposit_krw IS NULL
            AND funding_limit_ppm IS NULL
            AND maximum_funding_krw IS NULL
            AND (
                (current_lease_contract_id IS NULL
                 AND returned_deposit_krw = 0
                 AND replaced_loan_contract_id IS NULL
                 AND replaced_loan_principal_krw = 0)
                OR current_lease_contract_id IS NOT NULL
            )
            AND regulatory_dsr_applied IN (FALSE, TRUE)
            AND stress_rate_bp = 0
            AND stress_treatment = 'fullTermFixed'
        )
    ),
    ADD CONSTRAINT ck_loan_quote_decision CHECK (
        decision_code IN (
            'eligible', 'debtServiceLimit', 'incomeUnavailable',
            'creditRestricted', 'valuationUnavailable', 'collateralLimit',
            'affordabilityLimit', 'purchaseRestricted', 'insufficientOwnFunds'
        )
        AND JSON_TYPE(decision_reasons) = 'ARRAY'
        AND JSON_LENGTH(decision_reasons) BETWEEN 1 AND 8
        AND JSON_TYPE(quoted_terms) = 'OBJECT'
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
                  AND model.version_key IN (
                      'dev-unranked-m4c2c-credit-2026-v3',
                      'dev-unranked-m4c3-credit-2026-v4'
                  )
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
                        AND real_estate_model.version_key IN (
                            'dev-unranked-m4-real-estate-lifecycle-2026-v4',
                            'dev-unranked-m4-real-estate-purchase-2026-v5'
                        )
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
                                SELECT 1 FROM loan_obligation_bucket AS bucket
                                WHERE bucket.loan_contract_id = replacement.id
                                  AND bucket.status IN ('pending', 'delinquent')
                                  AND bucket.paid_amount_krw
                                        < bucket.original_amount_krw
                            )
                      )
                  )
              )
              OR (
                  NEW.purpose = 'mortgagePurchase'
                  AND model.version_key = 'dev-unranked-m4c3-credit-2026-v4'
                  AND product.product_kind = 'mortgage'
                  AND product.execution_channel = 'housingPurchase'
                  AND product.collateral_rule = 'mortgageLtv'
                  AND product.regulatory_dsr_treatment = 'includedFullTermFixed'
                  AND NEW.stress_rate_bp = 0
                  AND NEW.stress_treatment = 'fullTermFixed'
                  AND EXISTS (
                      SELECT 1
                      FROM credit_mortgage_policy_profile AS policy
                      INNER JOIN real_estate_model_version AS real_estate
                          ON real_estate.id = bundle.real_estate_model_version_id
                         AND real_estate.version_key
                              = 'dev-unranked-m4-real-estate-purchase-2026-v5'
                         AND real_estate.availability = 'active'
                         AND real_estate.sealed_at IS NOT NULL
                      INNER JOIN real_estate_model_strict_manifest AS real_manifest
                          ON real_manifest.real_estate_model_version_id = real_estate.id
                      INNER JOIN real_estate_model_strict_projection AS real_projection
                          ON real_projection.real_estate_model_version_id = real_estate.id
                      INNER JOIN real_estate_purchase_profile AS purchase_profile
                          ON purchase_profile.real_estate_model_version_id = real_estate.id
                      INNER JOIN property_listing AS listing
                          ON listing.id = NEW.property_listing_id
                         AND listing.market_world_id = bundle.market_world_id
                         AND listing.real_estate_model_version_id = real_estate.id
                      INNER JOIN property_listing_offer AS offer
                          ON offer.property_listing_id = listing.id
                         AND offer.offer_kind = 'sale'
                      INNER JOIN real_estate_purchase_region_mapping AS mapping
                          ON mapping.real_estate_model_version_id = real_estate.id
                         AND BINARY mapping.region_key = BINARY listing.region_key
                      INNER JOIN real_estate_region_moving_cost AS moving_cost
                          ON moving_cost.real_estate_model_version_id = real_estate.id
                         AND BINARY moving_cost.region_key = BINARY listing.region_key
                      WHERE policy.policy_set_id = model.credit_policy_set_id
                        AND BINARY real_estate.canonical_sha256
                              = BINARY real_manifest.canonical_sha256
                        AND BINARY real_manifest.canonical_json
                              = BINARY real_projection.canonical_json
                        AND listing.available_from_game_day <= save.game_day
                        AND listing.available_to_game_day >= save.game_day
                        AND offer.price_krw = NEW.recognized_collateral_value_krw
                        AND offer.deposit_krw IS NULL
                        AND offer.monthly_rent_krw IS NULL
                        AND BINARY mapping.ltv_region_class
                              = BINARY NEW.ltv_region_class
                        AND NEW.ltv_limit_ppm = CASE mapping.ltv_region_class
                            WHEN 'regulatedCapitalProxy'
                                THEN policy.regulated_capital_ltv_limit_ppm
                            ELSE policy.non_regulated_ltv_limit_ppm
                        END
                        AND NEW.ltv_numerator_krw = NEW.requested_principal_krw
                        AND NEW.ltv_denominator_krw = offer.price_krw
                        AND NEW.ltv_ratio_ppm = FLOOR(
                            CAST(NEW.requested_principal_krw AS DECIMAL(65, 0))
                            * 1000000 / offer.price_krw
                        )
                        AND NEW.maximum_mortgage_krw = LEAST(
                            FLOOR(
                                CAST(offer.price_krw AS DECIMAL(65, 0))
                                * NEW.ltv_limit_ppm / 1000000
                            ),
                            CASE
                                WHEN mapping.ltv_region_class = 'nonRegulatedProxy'
                                    THEN product.maximum_principal_krw
                                WHEN offer.price_krw <= policy.lower_price_threshold_krw
                                    THEN policy.lower_band_cap_krw
                                WHEN offer.price_krw <= policy.upper_price_threshold_krw
                                    THEN policy.middle_band_cap_krw
                                ELSE policy.upper_band_cap_krw
                            END,
                            product.maximum_principal_krw
                        )
                        AND NEW.acquisition_incidental_cost_krw = GREATEST(
                            FLOOR(
                                CAST(offer.price_krw AS DECIMAL(65, 0))
                                * purchase_profile.incidental_cost_ppm / 1000000
                            ),
                            purchase_profile.minimum_incidental_cost_krw
                        )
                        AND NEW.moving_cost_krw = moving_cost.moving_cost_krw
                        AND NEW.required_buyer_cash_krw
                              = GREATEST(
                                  offer.price_krw
                                      + NEW.acquisition_incidental_cost_krw
                                      + moving_cost.moving_cost_krw
                                      - NEW.requested_principal_krw,
                                  0
                              )
                        AND NEW.available_buyer_cash_krw
                              = save.cash_krw + NEW.returned_deposit_krw
                                - NEW.replaced_loan_principal_krw
                        AND (
                            NEW.regulatory_dsr_applied = FALSE
                            OR NEW.dsr_limit_ppm IS NULL
                            OR NEW.dsr_limit_ppm = policy.bank_dsr_limit_ppm
                        )
                  )
                  AND EXISTS (
                      SELECT 1 FROM command_identity AS identity
                      WHERE identity.save_id = save.id
                        AND BINARY identity.command_id = BINARY NEW.command_id
                        AND identity.command_kind = 'quoteMortgage'
                        AND identity.initial_run_revision = save.run_revision
                        AND identity.initial_state_revision = save.state_revision
                        AND identity.initial_game_day = save.game_day
                  )
                  AND (
                      (
                          NEW.current_lease_contract_id IS NULL
                          AND NEW.returned_deposit_krw = 0
                          AND NEW.replaced_loan_contract_id IS NULL
                          AND NEW.replaced_loan_principal_krw = 0
                          AND NOT EXISTS (
                              SELECT 1 FROM residence AS current_residence
                              WHERE current_residence.household_id = household.id
                                AND current_residence.effective_to_game_day IS NULL
                                AND current_residence.lease_contract_id IS NOT NULL
                          )
                      )
                      OR EXISTS (
                          SELECT 1
                          FROM lease_contract AS current_lease
                          INNER JOIN residence AS current_residence
                              ON current_residence.save_id = current_lease.save_id
                             AND current_residence.run_revision = current_lease.run_revision
                             AND current_residence.household_id = current_lease.household_id
                             AND current_residence.lease_contract_id = current_lease.id
                             AND current_residence.effective_to_game_day IS NULL
                          WHERE current_lease.id = NEW.current_lease_contract_id
                            AND current_lease.save_id = save.id
                            AND current_lease.run_revision = save.run_revision
                            AND current_lease.household_id = household.id
                            AND current_lease.effective_to_game_day IS NULL
                            AND current_lease.deposit_krw = NEW.returned_deposit_krw
                            AND (
                                (
                                    NEW.replaced_loan_contract_id IS NULL
                                    AND NEW.replaced_loan_principal_krw = 0
                                    AND NOT EXISTS (
                                        SELECT 1 FROM loan_contract AS linked
                                        WHERE linked.save_id = current_lease.save_id
                                          AND linked.run_revision
                                                = current_lease.run_revision
                                          AND linked.lease_contract_id = current_lease.id
                                          AND linked.status IN (
                                              'pending', 'active', 'delinquent',
                                              'defaulted', 'restructured'
                                          )
                                    )
                                )
                                OR EXISTS (
                                    SELECT 1
                                    FROM loan_contract AS replacement
                                    WHERE replacement.id
                                            = NEW.replaced_loan_contract_id
                                      AND replacement.save_id = current_lease.save_id
                                      AND replacement.run_revision
                                            = current_lease.run_revision
                                      AND replacement.lease_contract_id = current_lease.id
                                      AND replacement.product_kind = 'leaseDepositLoan'
                                      AND replacement.status = 'active'
                                      AND replacement.remaining_principal_krw
                                            = NEW.replaced_loan_principal_krw
                                      AND current_lease.deposit_krw
                                            >= replacement.remaining_principal_krw
                                      AND replacement.accrued_interest_krw = 0
                                      AND replacement.accrued_fee_krw = 0
                                      AND NOT EXISTS (
                                          SELECT 1
                                          FROM loan_obligation_bucket AS bucket
                                          WHERE bucket.loan_contract_id = replacement.id
                                            AND bucket.status IN ('pending', 'delinquent')
                                            AND bucket.paid_amount_krw
                                                  < bucket.original_amount_krw
                                      )
                                )
                                OR (
                                    (
                                        (
                                            NEW.decision_code = 'purchaseRestricted'
                                            AND JSON_CONTAINS(
                                                NEW.decision_reasons,
                                                JSON_QUOTE('leaseExitRestricted')
                                            ) = 1
                                        )
                                        OR (
                                            NEW.decision_code = 'creditRestricted'
                                            AND (
                                                JSON_CONTAINS(
                                                    NEW.decision_reasons,
                                                    JSON_QUOTE('activeDefault')
                                                ) = 1
                                                OR JSON_CONTAINS(
                                                    NEW.decision_reasons,
                                                    JSON_QUOTE('activeDelinquency')
                                                ) = 1
                                                OR JSON_CONTAINS(
                                                    NEW.decision_reasons,
                                                    JSON_QUOTE('activeRestructuring')
                                                ) = 1
                                                OR JSON_CONTAINS(
                                                    NEW.decision_reasons,
                                                    JSON_QUOTE('creditBandRestricted')
                                                ) = 1
                                                OR JSON_CONTAINS(
                                                    NEW.decision_reasons,
                                                    JSON_QUOTE('activeLoanLimit')
                                                ) = 1
                                            )
                                        )
                                    )
                                    AND EXISTS (
                                        SELECT 1
                                        FROM loan_contract AS restricted_loan
                                        WHERE restricted_loan.id
                                                = NEW.replaced_loan_contract_id
                                          AND restricted_loan.save_id
                                                = current_lease.save_id
                                          AND restricted_loan.run_revision
                                                = current_lease.run_revision
                                          AND restricted_loan.lease_contract_id
                                                = current_lease.id
                                          AND restricted_loan.product_kind
                                                = 'leaseDepositLoan'
                                          AND restricted_loan.status IN (
                                              'pending', 'active', 'delinquent',
                                              'defaulted', 'restructured'
                                          )
                                          AND restricted_loan.remaining_principal_krw
                                                = NEW.replaced_loan_principal_krw
                                          AND (
                                              restricted_loan.status <> 'active'
                                              OR current_lease.deposit_krw
                                                    < restricted_loan.remaining_principal_krw
                                              OR restricted_loan.accrued_interest_krw <> 0
                                              OR restricted_loan.accrued_fee_krw <> 0
                                              OR EXISTS (
                                                  SELECT 1
                                                  FROM loan_obligation_bucket AS bucket
                                                  WHERE bucket.loan_contract_id
                                                        = restricted_loan.id
                                                    AND bucket.status IN (
                                                        'pending', 'delinquent'
                                                    )
                                                    AND bucket.paid_amount_krw
                                                        < bucket.original_amount_krw
                                              )
                                          )
                                    )
                                )
                            )
                      )
                  )
              )
          )
    ),
    NEW.save_id,
    NULL
);

ALTER TABLE loan_contract
    DROP CHECK ck_loan_contract_origin,
    DROP CHECK ck_loan_contract_kind,
    DROP CHECK ck_loan_contract_lease_shape,
    MODIFY COLUMN origin_kind
        VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ADD COLUMN property_holding_id BIGINT UNSIGNED NULL AFTER lease_contract_id,
    ADD UNIQUE KEY uk_loan_contract_property_holding
        (save_id, run_revision, property_holding_id),
    ADD CONSTRAINT fk_loan_contract_property_holding
        FOREIGN KEY (save_id, run_revision, property_holding_id)
        REFERENCES property_holding (save_id, run_revision, id),
    ADD CONSTRAINT ck_loan_contract_origin CHECK (
        origin_kind IN (
            'characterStartV2', 'legacyV1Mapping', 'quoteExecution',
            'leaseDepositExecution', 'mortgagePurchaseExecution', 'legacyDebtBridge'
        )
        AND (
            (
                origin_kind IN (
                    'quoteExecution', 'leaseDepositExecution',
                    'mortgagePurchaseExecution'
                )
                AND loan_quote_id IS NOT NULL
            )
            OR (
                origin_kind NOT IN (
                    'quoteExecution', 'leaseDepositExecution',
                    'mortgagePurchaseExecution'
                )
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
            'studentLoan', 'unsecuredLoan', 'leaseDepositLoan', 'mortgage', 'legacyDebt'
        )
    ),
    ADD CONSTRAINT ck_loan_contract_lease_shape CHECK (
        (
            product_kind = 'leaseDepositLoan'
            AND origin_kind = 'leaseDepositExecution'
            AND lease_contract_id IS NOT NULL
            AND property_holding_id IS NULL
            AND dsr_included = FALSE
            AND read_only = FALSE
        )
        OR (
            product_kind = 'mortgage'
            AND origin_kind = 'mortgagePurchaseExecution'
            AND lease_contract_id IS NULL
            AND property_holding_id IS NOT NULL
            AND dsr_included = TRUE
            AND read_only = FALSE
        )
        OR (
            product_kind NOT IN ('leaseDepositLoan', 'mortgage')
            AND origin_kind NOT IN (
                'leaseDepositExecution', 'mortgagePurchaseExecution'
            )
            AND lease_contract_id IS NULL
            AND property_holding_id IS NULL
        )
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
        LEFT JOIN property_holding AS holding
            ON holding.id = NEW.property_holding_id
           AND holding.save_id = household.save_id
           AND holding.run_revision = household.run_revision
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
                              'quoteExecution', 'leaseDepositExecution',
                              'mortgagePurchaseExecution'
                          )
                          AND NEW.loan_quote_id IS NULL
                          AND NEW.lease_contract_id IS NULL
                          AND NEW.property_holding_id IS NULL
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
                          AND NEW.property_holding_id IS NULL
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
                          AND NEW.property_holding_id IS NULL
                      )
                      OR (
                          NEW.origin_kind = 'mortgagePurchaseExecution'
                          AND product.execution_channel = 'housingPurchase'
                          AND product.product_kind = 'mortgage'
                          AND quote.purpose = 'mortgagePurchase'
                          AND quote.decision_code = 'eligible'
                          AND quote.created_game_day = NEW.activated_game_day
                          AND quote.expires_game_day = NEW.activated_game_day
                          AND quote.loan_product_version_id = product.id
                          AND quote.requested_principal_krw
                                = NEW.original_principal_krw
                          AND quote.property_listing_id = holding.property_listing_id
                          AND holding.household_id = household.id
                          AND holding.status = 'active'
                          AND holding.purpose = 'ownerOccupied'
                          AND holding.acquired_game_day = NEW.activated_game_day
                          AND holding.acquisition_credit_policy_set_id
                                = model.credit_policy_set_id
                          AND BINARY holding.acquisition_command_id
                                = BINARY NEW.origin_command_id
                          AND NEW.lease_contract_id IS NULL
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
                  AND NEW.property_holding_id IS NULL
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
        AND NEW.property_holding_id <=> OLD.property_holding_id
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

CREATE TABLE property_lien (
    id                    BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id               BIGINT UNSIGNED NOT NULL,
    run_revision          INT UNSIGNED NOT NULL,
    property_holding_id   BIGINT UNSIGNED NOT NULL,
    loan_contract_id      BIGINT UNSIGNED NOT NULL,
    lien_priority         TINYINT UNSIGNED NOT NULL,
    status                VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    released_game_day     INT UNSIGNED NULL,
    created_at            DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_property_lien_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_property_lien_holding_priority
        (save_id, run_revision, property_holding_id, lien_priority),
    UNIQUE KEY uk_property_lien_holding
        (save_id, run_revision, property_holding_id),
    UNIQUE KEY uk_property_lien_loan
        (save_id, run_revision, loan_contract_id),
    CONSTRAINT fk_property_lien_holding
        FOREIGN KEY (save_id, run_revision, property_holding_id)
        REFERENCES property_holding (save_id, run_revision, id),
    CONSTRAINT fk_property_lien_loan
        FOREIGN KEY (save_id, run_revision, loan_contract_id)
        REFERENCES loan_contract (save_id, run_revision, id),
    CONSTRAINT ck_property_lien_priority CHECK (lien_priority = 1),
    CONSTRAINT ck_property_lien_status CHECK (
        (status = 'active' AND released_game_day IS NULL)
        OR (status = 'released' AND released_game_day IS NOT NULL)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_property_lien_valid_insert
BEFORE INSERT ON property_lien
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.lien_priority = 1
        AND NEW.status = 'active'
        AND NEW.released_game_day IS NULL
        AND EXISTS (
            SELECT 1
            FROM property_holding AS holding
            INNER JOIN loan_contract AS contract
                ON contract.save_id = holding.save_id
               AND contract.run_revision = holding.run_revision
               AND contract.property_holding_id = holding.id
            WHERE holding.id = NEW.property_holding_id
              AND holding.save_id = NEW.save_id
              AND holding.run_revision = NEW.run_revision
              AND holding.status = 'active'
              AND contract.id = NEW.loan_contract_id
              AND contract.product_kind = 'mortgage'
              AND contract.origin_kind = 'mortgagePurchaseExecution'
              AND contract.status = 'active'
              AND contract.original_principal_krw > 0
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_property_lien_transition_only
BEFORE UPDATE ON property_lien
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'active'
        AND OLD.released_game_day IS NULL
        AND NEW.status = 'released'
        AND NEW.released_game_day IS NOT NULL
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.property_holding_id = OLD.property_holding_id
        AND NEW.loan_contract_id = OLD.loan_contract_id
        AND NEW.lien_priority = OLD.lien_priority
        AND NEW.created_at = OLD.created_at
        AND EXISTS (
            SELECT 1
            FROM loan_contract AS contract
            INNER JOIN save
                ON save.id = contract.save_id
               AND save.run_revision = contract.run_revision
            WHERE contract.id = OLD.loan_contract_id
              AND contract.save_id = OLD.save_id
              AND contract.run_revision = OLD.run_revision
              AND contract.property_holding_id = OLD.property_holding_id
              AND contract.product_kind = 'mortgage'
              AND contract.origin_kind = 'mortgagePurchaseExecution'
              AND contract.status = 'paidOff'
              AND contract.remaining_principal_krw = 0
              AND contract.accrued_interest_krw = 0
              AND contract.accrued_fee_krw = 0
              AND save.game_day = NEW.released_game_day
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_property_lien_no_delete
BEFORE DELETE ON property_lien
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property liens are immutable';

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
                      AND identity.command_kind IN ('startLease', 'purchaseProperty')
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

ALTER TABLE ledger_transaction
    ADD CONSTRAINT ck_ledger_transaction_property_source CHECK (
        source_kind NOT LIKE 'property%'
        OR source_kind = 'propertyPurchase'
    );

ALTER TABLE ledger_posting
    DROP CHECK ck_ledger_posting_account_code,
    ADD COLUMN property_holding_id BIGINT UNSIGNED NULL AFTER lease_arrear_id,
    ADD KEY ix_ledger_posting_property_holding
        (save_id, run_revision, property_holding_id),
    ADD CONSTRAINT fk_ledger_posting_property_holding
        FOREIGN KEY (save_id, run_revision, property_holding_id)
        REFERENCES property_holding (save_id, run_revision, id),
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
            'leaseRentExpense', 'leaseArrearLiability',
            'propertyAsset', 'acquisitionIncidentalExpense'
        )
    ),
    ADD CONSTRAINT ck_ledger_posting_property_reference CHECK (
        (
            account_code IN ('propertyAsset', 'acquisitionIncidentalExpense')
            AND property_holding_id IS NOT NULL
        )
        OR (
            account_code NOT IN ('propertyAsset', 'acquisitionIncidentalExpense')
            AND property_holding_id IS NULL
        )
    );

CREATE TRIGGER tr_ledger_transaction_property_source_insert
BEFORE INSERT ON ledger_transaction
FOR EACH ROW
FOLLOWS tr_ledger_transaction_lease_source_insert
SET NEW.source_kind = IF(
    (
        NEW.source_kind = 'propertyPurchase'
        AND NEW.source_id REGEXP
            '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
        AND EXISTS (
            SELECT 1
            FROM property_holding AS holding
            WHERE holding.save_id = NEW.save_id
              AND holding.run_revision = NEW.run_revision
              AND BINARY holding.acquisition_command_id = BINARY NEW.source_id
              AND holding.acquired_game_day = NEW.game_day
              AND holding.acquisition_policy_set_id = NEW.policy_set_id
              AND holding.status = 'active'
        )
    )
    OR NEW.source_kind <> 'propertyPurchase',
    NEW.source_kind,
    NULL
);

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
                              SELECT 1 FROM loan_payment AS payoff
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
                  OR (
                      ledger.source_kind = 'propertyPurchase'
                      AND NEW.account_code = 'loanPrincipalLiability'
                      AND (
                          (
                              contract.origin_kind = 'mortgagePurchaseExecution'
                              AND contract.product_kind = 'mortgage'
                              AND BINARY contract.origin_command_id
                                    = BINARY ledger.source_id
                              AND contract.activated_game_day = ledger.game_day
                              AND NEW.amount_krw = -contract.original_principal_krw
                          )
                          OR EXISTS (
                              SELECT 1 FROM loan_payment AS payoff
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

DROP TRIGGER tr_ledger_posting_lease_reference_insert;

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
              AND (
                  (
                      ledger.source_kind = 'leaseMove'
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
                  OR (
                      ledger.source_kind = 'propertyPurchase'
                      AND NEW.amount_krw = -contract.deposit_krw
                      AND contract.effective_to_game_day = ledger.game_day
                  )
              )
        )
    )
    OR (
        NEW.account_code = 'movingExpense'
        AND NEW.lease_contract_id IS NULL
        AND (
            EXISTS (
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
            OR EXISTS (
                SELECT 1
                FROM ledger_transaction AS ledger
                INNER JOIN property_holding AS holding
                    ON holding.save_id = ledger.save_id
                   AND holding.run_revision = ledger.run_revision
                   AND BINARY holding.acquisition_command_id = BINARY ledger.source_id
                INNER JOIN real_estate_region_moving_cost AS moving_cost
                    ON moving_cost.real_estate_model_version_id
                            = holding.real_estate_model_version_id
                   AND BINARY moving_cost.region_key = BINARY holding.region_key
                WHERE ledger.id = NEW.ledger_transaction_id
                  AND ledger.save_id = NEW.save_id
                  AND ledger.run_revision = NEW.run_revision
                  AND ledger.source_kind = 'propertyPurchase'
                  AND NEW.amount_krw = moving_cost.moving_cost_krw
            )
        )
    )
    OR (
        NEW.account_code = 'wallet'
        AND NEW.lease_contract_id IS NULL
        AND NEW.loan_contract_id IS NULL
        AND (
            EXISTS (
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
            OR EXISTS (
                SELECT 1 FROM ledger_transaction AS ledger
                WHERE ledger.id = NEW.ledger_transaction_id
                  AND ledger.save_id = NEW.save_id
                  AND ledger.run_revision = NEW.run_revision
                  AND ledger.source_kind = 'propertyPurchase'
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
              AND ledger.source_kind IN (
                  'loanOrigination', 'debtAuthorityBridge',
                  'loanInstallment', 'loanPrepayment',
                  'leaseMove', 'propertyPurchase'
              )
        )
    )
    OR (
        NEW.account_code NOT IN (
            'leaseDepositAsset', 'movingExpense', 'loanPrincipalLiability'
        )
        AND NOT EXISTS (
            SELECT 1 FROM ledger_transaction AS ledger
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'leaseMove'
        )
    ),
    NEW.account_code,
    NULL
);

CREATE TRIGGER tr_ledger_posting_property_reference_insert
BEFORE INSERT ON ledger_posting
FOR EACH ROW
FOLLOWS tr_ledger_posting_lease_reference_insert
SET NEW.account_code = IF(
    (
        EXISTS (
            SELECT 1 FROM ledger_transaction AS ledger
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'propertyPurchase'
        )
        AND (
            (
                NEW.account_code = 'propertyAsset'
                AND NEW.loan_contract_id IS NULL
                AND NEW.lease_contract_id IS NULL
                AND EXISTS (
                    SELECT 1
                    FROM ledger_transaction AS ledger
                    INNER JOIN property_holding AS holding
                        ON holding.id = NEW.property_holding_id
                       AND holding.save_id = ledger.save_id
                       AND holding.run_revision = ledger.run_revision
                       AND BINARY holding.acquisition_command_id = BINARY ledger.source_id
                    WHERE ledger.id = NEW.ledger_transaction_id
                      AND ledger.save_id = NEW.save_id
                      AND ledger.run_revision = NEW.run_revision
                      AND ledger.source_kind = 'propertyPurchase'
                      AND holding.acquired_game_day = ledger.game_day
                      AND NEW.amount_krw = holding.acquisition_price_krw
                )
            )
            OR (
                NEW.account_code = 'acquisitionIncidentalExpense'
                AND NEW.loan_contract_id IS NULL
                AND NEW.lease_contract_id IS NULL
                AND EXISTS (
                    SELECT 1
                    FROM ledger_transaction AS ledger
                    INNER JOIN property_holding AS holding
                        ON holding.id = NEW.property_holding_id
                       AND holding.save_id = ledger.save_id
                       AND holding.run_revision = ledger.run_revision
                       AND BINARY holding.acquisition_command_id = BINARY ledger.source_id
                    WHERE ledger.id = NEW.ledger_transaction_id
                      AND ledger.save_id = NEW.save_id
                      AND ledger.run_revision = NEW.run_revision
                      AND ledger.source_kind = 'propertyPurchase'
                      AND NEW.amount_krw = holding.acquisition_incidental_cost_krw
                )
            )
            OR (
                NEW.account_code = 'movingExpense'
                AND NEW.property_holding_id IS NULL
                AND NEW.loan_contract_id IS NULL
                AND NEW.lease_contract_id IS NULL
            )
            OR (
                NEW.account_code = 'leaseDepositAsset'
                AND NEW.property_holding_id IS NULL
                AND NEW.loan_contract_id IS NULL
                AND NEW.lease_contract_id IS NOT NULL
            )
            OR (
                NEW.account_code = 'loanPrincipalLiability'
                AND NEW.property_holding_id IS NULL
                AND NEW.loan_contract_id IS NOT NULL
                AND NEW.lease_contract_id IS NULL
            )
            OR (
                NEW.account_code = 'wallet'
                AND NEW.property_holding_id IS NULL
                AND NEW.loan_contract_id IS NULL
                AND NEW.lease_contract_id IS NULL
                AND EXISTS (
                    SELECT 1
                    FROM ledger_transaction AS ledger
                    INNER JOIN property_holding AS holding
                        ON holding.save_id = ledger.save_id
                       AND holding.run_revision = ledger.run_revision
                       AND BINARY holding.acquisition_command_id = BINARY ledger.source_id
                    INNER JOIN real_estate_region_moving_cost AS moving_cost
                        ON moving_cost.real_estate_model_version_id
                                = holding.real_estate_model_version_id
                       AND BINARY moving_cost.region_key = BINARY holding.region_key
                    WHERE ledger.id = NEW.ledger_transaction_id
                      AND ledger.save_id = NEW.save_id
                      AND ledger.run_revision = NEW.run_revision
                      AND ledger.source_kind = 'propertyPurchase'
                      AND NEW.amount_krw =
                          COALESCE((
                              SELECT SUM(ended.deposit_krw)
                              FROM lease_contract AS ended
                              WHERE ended.save_id = ledger.save_id
                                AND ended.run_revision = ledger.run_revision
                                AND ended.household_id = holding.household_id
                                AND ended.effective_to_game_day = ledger.game_day
                          ), 0)
                          - COALESCE((
                              SELECT SUM(payoff.amount_krw)
                              FROM loan_payment AS payoff
                              INNER JOIN loan_contract AS old_loan
                                  ON old_loan.id = payoff.loan_contract_id
                                 AND old_loan.save_id = payoff.save_id
                                 AND old_loan.run_revision = payoff.run_revision
                              INNER JOIN lease_contract AS ended
                                  ON ended.id = old_loan.lease_contract_id
                                 AND ended.save_id = old_loan.save_id
                                 AND ended.run_revision = old_loan.run_revision
                              WHERE payoff.save_id = ledger.save_id
                                AND payoff.run_revision = ledger.run_revision
                                AND payoff.payment_kind = 'leaseMovePayoff'
                                AND payoff.status = 'prepared'
                                AND BINARY payoff.command_id = BINARY ledger.source_id
                                AND ended.household_id = holding.household_id
                                AND ended.effective_to_game_day = ledger.game_day
                          ), 0)
                          + COALESCE((
                              SELECT SUM(mortgage.original_principal_krw)
                              FROM loan_contract AS mortgage
                              WHERE mortgage.save_id = ledger.save_id
                                AND mortgage.run_revision = ledger.run_revision
                                AND mortgage.property_holding_id = holding.id
                                AND mortgage.origin_kind = 'mortgagePurchaseExecution'
                                AND BINARY mortgage.origin_command_id
                                      = BINARY ledger.source_id
                          ), 0)
                          - holding.acquisition_price_krw
                          - holding.acquisition_incidental_cost_krw
                          - moving_cost.moving_cost_krw
                )
            )
        )
    )
    OR (
        NOT EXISTS (
            SELECT 1 FROM ledger_transaction AS ledger
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'propertyPurchase'
        )
        AND NEW.property_holding_id IS NULL
        AND NEW.account_code NOT IN ('propertyAsset', 'acquisitionIncidentalExpense')
    ),
    NEW.account_code,
    NULL
);

CREATE VIEW property_book_value_projection AS
SELECT
    save.id AS save_id,
    save.run_revision,
    COALESCE(SUM(
        CASE
            WHEN holding.status = 'active' THEN holding.book_value_krw
            ELSE 0
        END
    ), 0) AS projected_property_book_value_krw
FROM save
LEFT JOIN property_holding AS holding
    ON holding.save_id = save.id
   AND holding.run_revision = save.run_revision
GROUP BY save.id, save.run_revision;

-- Quote commands retain revision; purchases advance it exactly once. The receipt trigger keeps
-- the two durable command shapes disjoint and prevents a purchase result from losing its ledger.
CREATE TRIGGER tr_command_identity_m4c3_valid_insert
BEFORE INSERT ON command_identity
FOR EACH ROW
SET NEW.command_kind = IF(
    NEW.command_kind NOT IN ('quoteMortgage', 'purchaseProperty')
        OR EXISTS (
            SELECT 1 FROM save
            WHERE save.id = NEW.save_id
              AND save.run_revision = NEW.initial_run_revision
              AND save.state_revision = NEW.initial_state_revision
              AND save.game_day = NEW.initial_game_day
        ),
    NEW.command_kind,
    NULL
);

CREATE TRIGGER tr_command_receipt_m4c3_valid_insert
BEFORE INSERT ON command_receipt
FOR EACH ROW
SET NEW.command_kind = IF(
    NEW.command_kind NOT IN ('quoteMortgage', 'purchaseProperty')
    OR (
        NEW.command_kind = 'quoteMortgage'
        AND NEW.ledger_transaction_id IS NULL
        AND EXISTS (
            SELECT 1
            FROM loan_quote AS quote
            WHERE quote.save_id = NEW.save_id
              AND quote.run_revision = NEW.run_revision
              AND BINARY quote.command_id = BINARY NEW.command_id
              AND quote.purpose = 'mortgagePurchase'
              AND quote.created_game_day = NEW.game_day
              AND quote.expected_state_revision = NEW.state_revision
        )
    )
    OR (
        NEW.command_kind = 'purchaseProperty'
        AND NEW.ledger_transaction_id IS NOT NULL
        AND EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            INNER JOIN property_holding AS holding
                ON holding.save_id = ledger.save_id
               AND holding.run_revision = ledger.run_revision
               AND BINARY holding.acquisition_command_id = BINARY ledger.source_id
            INNER JOIN command_identity AS identity
                ON identity.save_id = holding.save_id
               AND BINARY identity.command_id = BINARY holding.acquisition_command_id
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'propertyPurchase'
              AND BINARY ledger.source_id = BINARY NEW.command_id
              AND ledger.game_day = NEW.game_day
              AND identity.initial_state_revision + 1 = NEW.state_revision
        )
    ),
    NEW.command_kind,
    NULL
);

-- Publication must be complete before the shared assignment can expose either half of C3.
CREATE TEMPORARY TABLE m4c3_sealed_graph_guard (
    guard_key VARCHAR(48) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    accepted  TINYINT UNSIGNED NOT NULL,
    CONSTRAINT ck_m4c3_sealed_graph_guard CHECK (accepted = 1)
);

INSERT INTO m4c3_sealed_graph_guard (guard_key, accepted)
SELECT 'sealed-sourced-credit-policy', IF(
    EXISTS (
        SELECT 1
        FROM policy_set AS policy
        INNER JOIN policy_set_canonical_manifest AS manifest
            ON manifest.policy_set_id = policy.id
        INNER JOIN credit_mortgage_policy_profile AS profile
            ON profile.policy_set_id = policy.id
        WHERE policy.policy_key = 'dev-unranked-m4c3-credit-policy-2026-v2'
          AND policy.basis_date = '2026-07-27'
          AND policy.ranked_eligible = FALSE
          AND policy.sealed_at IS NOT NULL
          AND BINARY policy.canonical_sha256 = BINARY manifest.canonical_sha256
          AND profile.regulated_capital_ltv_limit_ppm = 400000
          AND profile.non_regulated_ltv_limit_ppm = 700000
          AND profile.lower_price_threshold_krw = 1500000000
          AND profile.upper_price_threshold_krw = 2500000000
          AND profile.lower_band_cap_krw = 600000000
          AND profile.middle_band_cap_krw = 400000000
          AND profile.upper_band_cap_krw = 200000000
          AND profile.borrower_dsr_balance_threshold_krw = 100000000
          AND profile.bank_dsr_limit_ppm = 400000
          AND profile.evaluation_horizon_months = 12
          AND profile.full_term_fixed_stress_rate_bp = 0
          AND (SELECT COUNT(*) FROM policy_rule AS rule
               WHERE rule.policy_set_id = policy.id) = 6
          AND (SELECT COUNT(*)
               FROM policy_rule_source AS link
               INNER JOIN policy_rule AS rule ON rule.id = link.policy_rule_id
               WHERE rule.policy_set_id = policy.id) = 13
    ),
    1,
    0
);

INSERT INTO m4c3_sealed_graph_guard (guard_key, accepted)
SELECT 'sealed-real-estate-v5', IF(
    EXISTS (
        SELECT 1
        FROM real_estate_model_version AS model
        INNER JOIN real_estate_model_strict_manifest AS manifest
            ON manifest.real_estate_model_version_id = model.id
        INNER JOIN real_estate_model_strict_projection AS projection
            ON projection.real_estate_model_version_id = model.id
        INNER JOIN real_estate_purchase_profile AS profile
            ON profile.real_estate_model_version_id = model.id
        WHERE model.version_key = 'dev-unranked-m4-real-estate-purchase-2026-v5'
          AND model.availability = 'active'
          AND model.ranked_eligible = FALSE
          AND model.sealed_at IS NOT NULL
          AND BINARY model.canonical_sha256 = BINARY manifest.canonical_sha256
          AND BINARY manifest.canonical_json = BINARY projection.canonical_json
          AND JSON_UNQUOTE(JSON_EXTRACT(model.parameters, '$.schemaVersion')) = '5'
          AND profile.purchase_capability = 'ownerOccupiedSingleHome'
          AND profile.maximum_active_holdings = 1
          AND profile.supported_offer_kind = 'sale'
          AND profile.supported_purpose = 'ownerOccupied'
          AND profile.incidental_cost_ppm = 10000
          AND profile.minimum_incidental_cost_krw = 1
          AND profile.collateral_value_rule = 'exactSalePriceAtExecution'
          AND profile.ltv_cost_treatment = 'excludeIncidentalAndMoving'
          AND profile.listing_consumption_scope = 'householdRunOnce'
          AND profile.provenance_kind = 'GAME_BALANCE'
          AND (SELECT COUNT(*) FROM real_estate_purchase_region_mapping AS mapping
               WHERE mapping.real_estate_model_version_id = model.id)
                = (SELECT COUNT(*) FROM life_region)
          AND (SELECT COUNT(*) FROM real_estate_region_profile AS region_profile
               WHERE region_profile.real_estate_model_version_id = model.id)
                = (SELECT COUNT(*) FROM life_region)
          AND (SELECT COUNT(*) FROM real_estate_region_moving_cost AS moving_cost
               WHERE moving_cost.real_estate_model_version_id = model.id)
                = (SELECT COUNT(*) FROM life_region)
          AND (SELECT COUNT(*) FROM real_estate_lease_profile AS lease_profile
               WHERE lease_profile.real_estate_model_version_id = model.id) = 2
    ),
    1,
    0
);

INSERT INTO m4c3_sealed_graph_guard (guard_key, accepted)
SELECT 'sealed-credit-v4-products', IF(
    EXISTS (
        SELECT 1
        FROM credit_model_version AS model
        INNER JOIN policy_set AS policy
            ON policy.id = model.credit_policy_set_id
        INNER JOIN credit_mortgage_policy_profile AS profile
            ON profile.policy_set_id = policy.id
        INNER JOIN credit_model_strict_manifest AS manifest
            ON manifest.credit_model_version_id = model.id
        INNER JOIN credit_model_strict_projection AS projection
            ON projection.credit_model_version_id = model.id
        WHERE model.version_key = 'dev-unranked-m4c3-credit-2026-v4'
          AND model.availability = 'active'
          AND model.ranked_eligible = FALSE
          AND model.sealed_at IS NOT NULL
          AND policy.policy_key = 'dev-unranked-m4c3-credit-policy-2026-v2'
          AND policy.sealed_at IS NOT NULL
          AND BINARY model.canonical_sha256 = BINARY manifest.canonical_sha256
          AND BINARY manifest.canonical_json = BINARY projection.canonical_json
          AND JSON_UNQUOTE(JSON_EXTRACT(model.parameters, '$.schemaVersion')) = '5'
          AND JSON_UNQUOTE(
                  JSON_EXTRACT(manifest.canonical_json, '$.schemaVersion')
              ) = '3'
          AND JSON_UNQUOTE(JSON_EXTRACT(
                  manifest.canonical_json,
                  '$.mortgagePolicyProfile.schemaVersion'
              )) = '1'
          AND (SELECT COUNT(*) FROM loan_product_version AS product
               WHERE product.credit_model_version_id = model.id
                 AND product.catalog_scope = 'modelChild'
                 AND product.sealed_at IS NOT NULL) = 4
          AND (SELECT COUNT(*) FROM loan_product_legacy_start_mapping AS mapping
               WHERE mapping.credit_model_version_id = model.id) = 2
          AND EXISTS (
              SELECT 1
              FROM loan_product_version AS product
              INNER JOIN loan_product_canonical_manifest AS product_manifest
                  ON product_manifest.loan_product_version_id = product.id
              INNER JOIN loan_product_canonical_projection AS product_projection
                  ON product_projection.loan_product_version_id = product.id
              WHERE product.credit_model_version_id = model.id
                AND product.product_key = 'dev-mortgage-fixed-level-payment-2026-v1'
                AND product.product_kind = 'mortgage'
                AND product.execution_channel = 'housingPurchase'
                AND product.collateral_rule = 'mortgageLtv'
                AND product.regulatory_dsr_treatment = 'includedFullTermFixed'
                AND product.fixed_annual_rate_bp = 400
                AND product.term_months = 360
                AND product.repayment_method = 'levelPayment'
                AND product.maximum_principal_krw = 600000000
                AND product.starting_eligible = FALSE
                AND product.quote_eligible = TRUE
                AND product.execution_eligible = TRUE
                AND product.sealed_at IS NOT NULL
                AND BINARY product.canonical_sha256
                      = BINARY product_manifest.canonical_sha256
                AND BINARY product_manifest.canonical_json
                      = BINARY product_projection.canonical_json
          )
    ),
    1,
    0
);

INSERT INTO m4c3_sealed_graph_guard (guard_key, accepted)
SELECT 'legacy-graphs-byte-exact', IF(
    NOT EXISTS (
        SELECT 1
        FROM credit_model_version AS model
        INNER JOIN credit_model_strict_manifest AS manifest
            ON manifest.credit_model_version_id = model.id
        LEFT JOIN credit_model_strict_projection AS projection
            ON projection.credit_model_version_id = model.id
        WHERE model.version_key IN (
                  'dev-unranked-m4b-credit-2026-v1',
                  'dev-unranked-m4b-credit-2026-v2',
                  'dev-unranked-m4c2c-credit-2026-v3'
              )
          AND (
              projection.credit_model_version_id IS NULL
              OR BINARY model.canonical_sha256 <> BINARY manifest.canonical_sha256
              OR BINARY manifest.canonical_json <> BINARY projection.canonical_json
          )
    )
        AND NOT EXISTS (
            SELECT 1
            FROM real_estate_model_version AS model
            INNER JOIN real_estate_model_strict_manifest AS manifest
                ON manifest.real_estate_model_version_id = model.id
            LEFT JOIN real_estate_model_strict_projection AS projection
                ON projection.real_estate_model_version_id = model.id
            WHERE model.version_key IN (
                      'dev-unranked-m4-real-estate-2026-v1',
                      'dev-unranked-m4-real-estate-lease-2026-v2',
                      'dev-unranked-m4-real-estate-rent-2026-v3',
                      'dev-unranked-m4-real-estate-lifecycle-2026-v4'
                  )
              AND (
                  projection.real_estate_model_version_id IS NULL
                  OR BINARY model.canonical_sha256 <> BINARY manifest.canonical_sha256
                  OR BINARY manifest.canonical_json <> BINARY projection.canonical_json
              )
        )
        AND NOT EXISTS (
            SELECT 1
            FROM loan_product_version AS product
            INNER JOIN loan_product_canonical_manifest AS manifest
                ON manifest.loan_product_version_id = product.id
            LEFT JOIN loan_product_canonical_projection AS projection
                ON projection.loan_product_version_id = product.id
            WHERE product.credit_model_version_id <> (
                      SELECT id FROM credit_model_version
                      WHERE version_key = 'dev-unranked-m4c3-credit-2026-v4'
                  )
              AND product.sealed_at IS NOT NULL
              AND (
                  projection.loan_product_version_id IS NULL
                  OR BINARY product.canonical_sha256 <> BINARY manifest.canonical_sha256
                  OR BINARY manifest.canonical_json <> BINARY projection.canonical_json
              )
        ),
    1,
    0
);

INSERT INTO m4c3_sealed_graph_guard (guard_key, accepted)
SELECT 'existing-run-pins-unchanged', IF(
    NOT EXISTS (
        SELECT 1
        FROM run_rule_bundle AS bundle
        WHERE bundle.credit_model_version_id = (
                  SELECT id FROM credit_model_version
                  WHERE version_key = 'dev-unranked-m4c3-credit-2026-v4'
              )
           OR bundle.real_estate_model_version_id = (
                  SELECT id FROM real_estate_model_version
                  WHERE version_key = 'dev-unranked-m4-real-estate-purchase-2026-v5'
              )
    ),
    1,
    0
);

DROP TEMPORARY TABLE m4c3_sealed_graph_guard;

-- The pair is exposed atomically after both manifests and all typed children have passed the
-- guards above. Existing run_rule_bundle rows are immutable pins and are not touched here.
UPDATE run_rule_bundle_assignment AS assignment
INNER JOIN credit_model_version AS credit
    ON credit.version_key = 'dev-unranked-m4c3-credit-2026-v4'
   AND credit.availability = 'active'
   AND credit.sealed_at IS NOT NULL
INNER JOIN real_estate_model_version AS real_estate
    ON real_estate.version_key = 'dev-unranked-m4-real-estate-purchase-2026-v5'
   AND real_estate.availability = 'active'
   AND real_estate.sealed_at IS NOT NULL
SET assignment.credit_model_version_id = credit.id,
    assignment.real_estate_model_version_id = real_estate.id
WHERE assignment.assignment_key = 'newRun';

CREATE TEMPORARY TABLE m4c3_assignment_guard (
    guard_key VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    accepted  TINYINT UNSIGNED NOT NULL,
    CONSTRAINT ck_m4c3_assignment_guard CHECK (accepted = 1)
);

INSERT INTO m4c3_assignment_guard (guard_key, accepted)
SELECT 'new-run-v5-v4-pair', IF(
    EXISTS (
        SELECT 1
        FROM run_rule_bundle_assignment AS assignment
        INNER JOIN credit_model_version AS credit
            ON credit.id = assignment.credit_model_version_id
        INNER JOIN real_estate_model_version AS real_estate
            ON real_estate.id = assignment.real_estate_model_version_id
        WHERE assignment.assignment_key = 'newRun'
          AND credit.version_key = 'dev-unranked-m4c3-credit-2026-v4'
          AND real_estate.version_key
                = 'dev-unranked-m4-real-estate-purchase-2026-v5'
    ),
    1,
    0
);

DROP TEMPORARY TABLE m4c3_assignment_guard;
