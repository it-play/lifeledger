-- M4-C4 publishes deterministic property sales and sourced property-tax authority while
-- preserving every previously sealed finance, credit, and real-estate graph (§5 C4).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;
SET SESSION group_concat_max_len = 1048576;

-- MySQL DDL auto-commits. Refuse to start unless every graph that C4 clones is canonical and
-- every target identity is unused, so a failed forward migration cannot silently fork history.
CREATE TEMPORARY TABLE m4c4_preflight_guard (
    guard_key VARCHAR(48) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    accepted  TINYINT UNSIGNED NOT NULL,
    CONSTRAINT ck_m4c4_preflight_guard CHECK (accepted = 1)
);

INSERT INTO m4c4_preflight_guard (guard_key, accepted)
SELECT 'sealed-finance-v2', IF(
    EXISTS (
        SELECT 1
        FROM policy_set AS policy
        INNER JOIN policy_set_canonical_manifest AS manifest
            ON manifest.policy_set_id = policy.id
        WHERE policy.policy_key = 'kr-individual-2026-v2'
          AND policy.sealed_at IS NOT NULL
          AND BINARY policy.canonical_sha256 = BINARY manifest.canonical_sha256
          AND (SELECT COUNT(*) FROM policy_rule AS rule
               WHERE rule.policy_set_id = policy.id) = 7
    ),
    1,
    0
);

INSERT INTO m4c4_preflight_guard (guard_key, accepted)
SELECT 'sealed-real-estate-v5', IF(
    EXISTS (
        SELECT 1
        FROM real_estate_model_version AS model
        INNER JOIN real_estate_model_strict_manifest AS manifest
            ON manifest.real_estate_model_version_id = model.id
        INNER JOIN real_estate_model_strict_projection AS projection
            ON projection.real_estate_model_version_id = model.id
        INNER JOIN real_estate_purchase_profile AS purchase
            ON purchase.real_estate_model_version_id = model.id
        WHERE model.version_key = 'dev-unranked-m4-real-estate-purchase-2026-v5'
          AND model.availability = 'active'
          AND model.sealed_at IS NOT NULL
          AND BINARY model.canonical_sha256 = BINARY manifest.canonical_sha256
          AND BINARY manifest.canonical_json = BINARY projection.canonical_json
          AND purchase.purchase_capability = 'ownerOccupiedSingleHome'
    ),
    1,
    0
);

INSERT INTO m4c4_preflight_guard (guard_key, accepted)
SELECT 'sealed-credit-v4', IF(
    EXISTS (
        SELECT 1
        FROM credit_model_version AS model
        INNER JOIN credit_model_strict_manifest AS manifest
            ON manifest.credit_model_version_id = model.id
        INNER JOIN credit_model_strict_projection AS projection
            ON projection.credit_model_version_id = model.id
        WHERE model.version_key = 'dev-unranked-m4c3-credit-2026-v4'
          AND model.availability = 'active'
          AND model.sealed_at IS NOT NULL
          AND BINARY model.canonical_sha256 = BINARY manifest.canonical_sha256
          AND BINARY manifest.canonical_json = BINARY projection.canonical_json
    ),
    1,
    0
);

INSERT INTO m4c4_preflight_guard (guard_key, accepted)
SELECT 'new-run-v2-v5-v4', IF(
    EXISTS (
        SELECT 1
        FROM run_rule_bundle_assignment AS assignment
        INNER JOIN policy_set AS policy ON policy.id = assignment.policy_set_id
        INNER JOIN credit_model_version AS credit
            ON credit.id = assignment.credit_model_version_id
        INNER JOIN real_estate_model_version AS real_estate
            ON real_estate.id = assignment.real_estate_model_version_id
        WHERE assignment.assignment_key = 'newRun'
          AND policy.policy_key = 'kr-individual-2026-v2'
          AND credit.version_key = 'dev-unranked-m4c3-credit-2026-v4'
          AND real_estate.version_key
                = 'dev-unranked-m4-real-estate-purchase-2026-v5'
    ),
    1,
    0
);

INSERT INTO m4c4_preflight_guard (guard_key, accepted)
SELECT 'target-identities-unused', IF(
    NOT EXISTS (
        SELECT 1 FROM policy_set
        WHERE policy_key = 'dev-unranked-kr-individual-property-2026-v3'
    )
        AND NOT EXISTS (
            SELECT 1 FROM real_estate_model_version
            WHERE version_key
                = 'dev-unranked-m4-real-estate-sale-tax-2026-v6'
        )
        AND NOT EXISTS (
            SELECT 1 FROM policy_source_document
            WHERE source_key IN (
                'law-local-tax-act-article-11-2026-01-01',
                'law-local-tax-act-article-20-2026-01-01',
                'law-local-tax-act-article-110-2026-01-01',
                'law-local-tax-act-articles-111-111-2-2026-01-01',
                'law-local-tax-act-articles-114-115-2026-01-01',
                'law-income-tax-decree-article-154-2026-01-01',
                'nts-high-value-home-capital-gain-2026-07-27',
                'law-income-tax-act-article-95-2026-01-01',
                'law-income-tax-act-article-103-2026-01-01'
            )
        ),
    1,
    0
);

DROP TEMPORARY TABLE m4c4_preflight_guard;

CREATE TEMPORARY TABLE m4c4_legacy_real_estate_bytes AS
SELECT model.id AS real_estate_model_version_id,
       manifest.canonical_json,
       manifest.canonical_sha256,
       model.canonical_sha256 AS model_sha256
FROM real_estate_model_version AS model
INNER JOIN real_estate_model_strict_manifest AS manifest
    ON manifest.real_estate_model_version_id = model.id
WHERE model.sealed_at IS NOT NULL;

CREATE TEMPORARY TABLE m4c4_legacy_policy_bytes AS
SELECT policy.id AS policy_set_id,
       manifest.canonical_json,
       manifest.canonical_sha256,
       policy.canonical_sha256 AS policy_sha256
FROM policy_set AS policy
INNER JOIN policy_set_canonical_manifest AS manifest
    ON manifest.policy_set_id = policy.id
WHERE policy.sealed_at IS NOT NULL;

INSERT INTO policy_source_document
    (source_key, source_url, checked_on, original_sha256)
VALUES
    (
        'law-local-tax-act-article-11-2026-01-01',
        'https://www.law.go.kr/LSW/lsSideInfoP.do?docCls=jo&joBrNo=00&joNo=0011&lsiSeq=282559&urlMode=lsScJoRltInfoR',
        '2026-07-27',
        '3d735c5df7da1eb52953c4e6e2f893f6dc011086a9f78f411b1b8dad061445c7'
    ),
    (
        'law-local-tax-act-article-20-2026-01-01',
        'https://www.law.go.kr/LSW/lsLinkCommonInfo.do?lsJoLnkSeq=1032970405',
        '2026-07-27',
        '0bf293f5b604676adb60ca8133566694db7fdf2a06e78eee31c3a0d78d8c924e'
    ),
    (
        'law-local-tax-act-article-110-2026-01-01',
        'https://law.go.kr/lsLinkCommonInfo.do?chrClsCd=010202&lsJoLnkSeq=1029491211',
        '2026-07-27',
        '36b51e86c6f2a3c21a535040441f9eb3054b005ea33ac3d1ea86c5b71cdbb573'
    ),
    (
        'law-local-tax-act-articles-111-111-2-2026-01-01',
        'https://law.go.kr/LSW/lsLinkCommonInfo.do?chrClsCd=010202&lsJoLnkSeq=1033362253',
        '2026-07-27',
        'f82305583dcaf99d6a45892bb0c5f4c970fd3eba3135bac08ea575fd05a8e1b4'
    ),
    (
        'law-local-tax-act-articles-114-115-2026-01-01',
        'https://law.go.kr/LSW/lsLinkCommonInfo.do?chrClsCd=010202&lsJoLnkSeq=1029491445',
        '2026-07-27',
        '481fb4ff67d8ddf2e053e3b27ea91646403a6ba950014dd935cf07a5959eb40b'
    ),
    (
        'law-income-tax-decree-article-154-2026-01-01',
        'https://www.law.go.kr/lsLinkCommonInfo.do?lsJoLnkSeq=1031481567',
        '2026-07-27',
        '4815333e7e8b27ead040c9305b55d4f680bcf305f88c1da0ab7db71c7342bf13'
    ),
    (
        'nts-high-value-home-capital-gain-2026-07-27',
        'https://www.nts.go.kr/nts/cm/cntnts/cntntsView.do?cntntsId=8799&mi=12271',
        '2026-07-27',
        'c579110fde86d11735684af9df17d8f7012953181e8cecf49689e8ba693bc462'
    ),
    (
        'law-income-tax-act-article-95-2026-01-01',
        'https://www.law.go.kr/lsLinkCommonInfo.do?chrClsCd=010202&lsJoLnkSeq=1032210681',
        '2026-07-27',
        'b11c973cbea1270ebe8fd187d72359724392e615be1925e2ceaad39226ae113e'
    ),
    (
        'law-income-tax-act-article-103-2026-01-01',
        'https://www.law.go.kr/LSW/lsLinkCommonInfo.do?chrClsCd=010202&lsJoLnkSeq=1033240591',
        '2026-07-27',
        '46401c6d865d4c0e52b9a10d12c402bfdabaaee52c685e420589346d477df7a1'
    );

INSERT INTO policy_set (policy_key, basis_date, ranked_eligible)
VALUES ('dev-unranked-kr-individual-property-2026-v3', '2026-07-27', FALSE);

INSERT INTO policy_rule
    (policy_set_id, domain, rule_key, effective_from, effective_to, parameters)
SELECT target.id, source.domain, source.rule_key,
       source.effective_from, source.effective_to, source.parameters
FROM policy_set AS target
INNER JOIN policy_set AS source_set
    ON source_set.policy_key = 'kr-individual-2026-v2'
INNER JOIN policy_rule AS source ON source.policy_set_id = source_set.id
WHERE target.policy_key = 'dev-unranked-kr-individual-property-2026-v3';

-- The bridge intentionally closed direct legacy-provenance inserts. A clone edge preserves the
-- exact sealed source rule without pretending that the pre-M4 rule acquired a new citation.
CREATE TABLE policy_rule_clone_provenance (
    target_policy_rule_id  BIGINT UNSIGNED NOT NULL,
    source_policy_rule_id  BIGINT UNSIGNED NOT NULL,
    clone_kind             VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at             DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (target_policy_rule_id),
    KEY ix_policy_rule_clone_source (source_policy_rule_id),
    CONSTRAINT fk_policy_rule_clone_target
        FOREIGN KEY (target_policy_rule_id) REFERENCES policy_rule (id),
    CONSTRAINT fk_policy_rule_clone_source
        FOREIGN KEY (source_policy_rule_id) REFERENCES policy_rule (id),
    CONSTRAINT ck_policy_rule_clone_kind CHECK (clone_kind = 'sealedExactClone'),
    CONSTRAINT ck_policy_rule_clone_distinct CHECK (
        target_policy_rule_id <> source_policy_rule_id
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_policy_rule_clone_valid_insert
BEFORE INSERT ON policy_rule_clone_provenance
FOR EACH ROW
SET NEW.target_policy_rule_id = IF(
    EXISTS (
        SELECT 1
        FROM policy_rule AS target_rule
        INNER JOIN policy_set AS target_set
            ON target_set.id = target_rule.policy_set_id
        INNER JOIN policy_rule AS source_rule
            ON source_rule.id = NEW.source_policy_rule_id
        INNER JOIN policy_set AS source_set
            ON source_set.id = source_rule.policy_set_id
        WHERE target_rule.id = NEW.target_policy_rule_id
          AND target_set.policy_key
                = 'dev-unranked-kr-individual-property-2026-v3'
          AND target_set.sealed_at IS NULL
          AND source_set.policy_key = 'kr-individual-2026-v2'
          AND source_set.sealed_at IS NOT NULL
          AND BINARY target_rule.domain = BINARY source_rule.domain
          AND BINARY target_rule.rule_key = BINARY source_rule.rule_key
          AND target_rule.effective_from = source_rule.effective_from
          AND target_rule.effective_to <=> source_rule.effective_to
          AND target_rule.parameters = source_rule.parameters
          AND (
              EXISTS (
                  SELECT 1 FROM policy_rule_source AS source_link
                  WHERE source_link.policy_rule_id = source_rule.id
              )
              OR EXISTS (
                  SELECT 1 FROM policy_rule_legacy_provenance AS legacy
                  WHERE legacy.policy_rule_id = source_rule.id
              )
          )
    ),
    NEW.target_policy_rule_id,
    NULL
);

CREATE TRIGGER tr_policy_rule_clone_no_update
BEFORE UPDATE ON policy_rule_clone_provenance
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'policy rule clone provenance is immutable';

CREATE TRIGGER tr_policy_rule_clone_no_delete
BEFORE DELETE ON policy_rule_clone_provenance
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'policy rule clone provenance is immutable';

INSERT INTO policy_rule_clone_provenance
    (target_policy_rule_id, source_policy_rule_id, clone_kind)
SELECT target_rule.id, source_rule.id, 'sealedExactClone'
FROM policy_rule AS target_rule
INNER JOIN policy_set AS target_set
    ON target_set.id = target_rule.policy_set_id
INNER JOIN policy_set AS source_set
    ON source_set.policy_key = 'kr-individual-2026-v2'
INNER JOIN policy_rule AS source_rule
    ON source_rule.policy_set_id = source_set.id
   AND BINARY source_rule.domain = BINARY target_rule.domain
   AND BINARY source_rule.rule_key = BINARY target_rule.rule_key
   AND source_rule.effective_from = target_rule.effective_from
WHERE target_set.policy_key = 'dev-unranked-kr-individual-property-2026-v3';

INSERT INTO policy_rule_source
    (policy_rule_id, policy_source_document_id, citation_order)
SELECT target_rule.id, source_link.policy_source_document_id, source_link.citation_order
FROM policy_rule AS target_rule
INNER JOIN policy_set AS target_set
    ON target_set.id = target_rule.policy_set_id
INNER JOIN policy_set AS source_set
    ON source_set.policy_key = 'kr-individual-2026-v2'
INNER JOIN policy_rule AS source_rule
    ON source_rule.policy_set_id = source_set.id
   AND BINARY source_rule.domain = BINARY target_rule.domain
   AND BINARY source_rule.rule_key = BINARY target_rule.rule_key
   AND source_rule.effective_from = target_rule.effective_from
INNER JOIN policy_rule_source AS source_link
    ON source_link.policy_rule_id = source_rule.id
WHERE target_set.policy_key = 'dev-unranked-kr-individual-property-2026-v3';

INSERT INTO policy_rule
    (policy_set_id, domain, rule_key, effective_from, effective_to, parameters)
SELECT policy.id, 'propertyTax', 'singleHomeAcquisitionTax', '2026-01-01', NULL,
       JSON_OBJECT(
           'profileTable', 'property_acquisition_tax_policy_profile',
           'schemaVersion', 1,
           'supportedHomeCount', 1,
           'unsupportedTreatment', 'policyUnsupported'
       )
FROM policy_set AS policy
WHERE policy.policy_key = 'dev-unranked-kr-individual-property-2026-v3'
UNION ALL
SELECT policy.id, 'propertyTax', 'singleHomeAnnualPropertyTax', '2026-01-01', NULL,
       JSON_OBJECT(
           'profileTable', 'property_annual_tax_policy_profile',
           'schemaVersion', 1,
           'supportedHomeCount', 1,
           'unsupportedTreatment', 'policyUnsupported'
       )
FROM policy_set AS policy
WHERE policy.policy_key = 'dev-unranked-kr-individual-property-2026-v3'
UNION ALL
SELECT policy.id, 'propertyTax', 'singleHomeCapitalGainsTax', '2026-01-01', NULL,
       JSON_OBJECT(
           'profileTable', 'property_capital_gains_tax_policy_profile',
           'schemaVersion', 1,
           'supportedHomeCount', 1,
           'unsupportedTreatment', 'policyUnsupported'
       )
FROM policy_set AS policy
WHERE policy.policy_key = 'dev-unranked-kr-individual-property-2026-v3';

INSERT INTO policy_rule_source
    (policy_rule_id, policy_source_document_id, citation_order)
SELECT rule.id, source.id, citation.citation_order
FROM policy_rule AS rule
INNER JOIN policy_set AS policy ON policy.id = rule.policy_set_id
INNER JOIN (
    SELECT 'singleHomeAcquisitionTax' AS rule_key,
           'law-local-tax-act-article-11-2026-01-01' AS source_key,
           1 AS citation_order
    UNION ALL
    SELECT 'singleHomeAcquisitionTax',
           'law-local-tax-act-article-20-2026-01-01', 2
    UNION ALL
    SELECT 'singleHomeAnnualPropertyTax',
           'law-local-tax-act-article-110-2026-01-01', 1
    UNION ALL
    SELECT 'singleHomeAnnualPropertyTax',
           'law-local-tax-act-articles-111-111-2-2026-01-01', 2
    UNION ALL
    SELECT 'singleHomeAnnualPropertyTax',
           'law-local-tax-act-articles-114-115-2026-01-01', 3
    UNION ALL
    SELECT 'singleHomeCapitalGainsTax',
           'law-income-tax-decree-article-154-2026-01-01', 1
    UNION ALL
    SELECT 'singleHomeCapitalGainsTax',
           'nts-high-value-home-capital-gain-2026-07-27', 2
    UNION ALL
    SELECT 'singleHomeCapitalGainsTax',
           'law-income-tax-act-article-95-2026-01-01', 3
    UNION ALL
    SELECT 'singleHomeCapitalGainsTax',
           'law-income-tax-act-article-103-2026-01-01', 4
) AS citation ON BINARY citation.rule_key = BINARY rule.rule_key
INNER JOIN policy_source_document AS source
    ON BINARY source.source_key = BINARY citation.source_key
WHERE policy.policy_key = 'dev-unranked-kr-individual-property-2026-v3';

CREATE TABLE property_acquisition_tax_policy_profile (
    policy_set_id                       BIGINT UNSIGNED NOT NULL,
    rule_id                             BIGINT UNSIGNED NOT NULL,
    supported_home_count                TINYINT UNSIGNED NOT NULL,
    lower_price_maximum_krw             BIGINT NOT NULL,
    middle_price_maximum_krw            BIGINT NOT NULL,
    lower_rate_ppm                      INT UNSIGNED NOT NULL,
    upper_rate_ppm                      INT UNSIGNED NOT NULL,
    middle_rate_price_divisor_krw       BIGINT NOT NULL,
    middle_rate_offset_ppm              INT UNSIGNED NOT NULL,
    middle_rate_rounding                VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin
                                            NOT NULL,
    local_education_rate_ratio_ppm      INT UNSIGNED NOT NULL,
    payment_due_days                    SMALLINT UNSIGNED NOT NULL,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (policy_set_id),
    UNIQUE KEY uk_property_acquisition_tax_policy_rule (rule_id),
    CONSTRAINT fk_property_acquisition_tax_policy_set
        FOREIGN KEY (policy_set_id) REFERENCES policy_set (id),
    CONSTRAINT fk_property_acquisition_tax_policy_rule
        FOREIGN KEY (rule_id) REFERENCES policy_rule (id),
    CONSTRAINT ck_property_acquisition_tax_fixture CHECK (
        supported_home_count = 1
        AND lower_price_maximum_krw = 600000000
        AND middle_price_maximum_krw = 900000000
        AND lower_rate_ppm = 10000
        AND upper_rate_ppm = 30000
        AND middle_rate_price_divisor_krw = 15000
        AND middle_rate_offset_ppm = 30000
        AND middle_rate_rounding = 'halfUp'
        AND local_education_rate_ratio_ppm = 100000
        AND payment_due_days = 60
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE property_annual_tax_policy_profile (
    policy_set_id                               BIGINT UNSIGNED NOT NULL,
    rule_id                                     BIGINT UNSIGNED NOT NULL,
    supported_home_count                        TINYINT UNSIGNED NOT NULL,
    assessment_month                            TINYINT UNSIGNED NOT NULL,
    assessment_day                              TINYINT UNSIGNED NOT NULL,
    ownership_cutoff_rule                       VARCHAR(32) CHARACTER SET ascii
                                                    COLLATE ascii_bin NOT NULL,
    official_value_ratio_ppm                    INT UNSIGNED NOT NULL,
    special_rate_official_value_maximum_krw     BIGINT NOT NULL,
    local_education_rate_ratio_ppm              INT UNSIGNED NOT NULL,
    first_payment_month                         TINYINT UNSIGNED NOT NULL,
    first_payment_day                           TINYINT UNSIGNED NOT NULL,
    second_payment_month                        TINYINT UNSIGNED NOT NULL,
    second_payment_day                          TINYINT UNSIGNED NOT NULL,
    payment_split_rule                          VARCHAR(32) CHARACTER SET ascii
                                                    COLLATE ascii_bin NOT NULL,
    unsupported_exclusion_codes                 JSON NOT NULL,
    created_at                                  DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (policy_set_id),
    UNIQUE KEY uk_property_annual_tax_policy_rule (rule_id),
    CONSTRAINT fk_property_annual_tax_policy_set
        FOREIGN KEY (policy_set_id) REFERENCES policy_set (id),
    CONSTRAINT fk_property_annual_tax_policy_rule
        FOREIGN KEY (rule_id) REFERENCES policy_rule (id),
    CONSTRAINT ck_property_annual_tax_fixture CHECK (
        supported_home_count = 1
        AND assessment_month = 6
        AND assessment_day = 1
        AND ownership_cutoff_rule = 'priorDayClosingOwner'
        AND official_value_ratio_ppm = 700000
        AND special_rate_official_value_maximum_krw = 900000000
        AND local_education_rate_ratio_ppm = 200000
        AND first_payment_month = 7
        AND first_payment_day = 31
        AND second_payment_month = 9
        AND second_payment_day = 30
        AND payment_split_rule = 'floorHalfThenRemainder'
        AND JSON_TYPE(unsupported_exclusion_codes) = 'ARRAY'
        AND JSON_LENGTH(unsupported_exclusion_codes) = 4
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE property_annual_tax_fair_market_ratio_band (
    policy_set_id                   BIGINT UNSIGNED NOT NULL,
    band_order                      TINYINT UNSIGNED NOT NULL,
    official_value_upper_bound_krw  BIGINT NULL,
    fair_market_value_ratio_ppm     INT UNSIGNED NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (policy_set_id, band_order),
    UNIQUE KEY uk_property_annual_fair_market_bound
        (policy_set_id, official_value_upper_bound_krw),
    CONSTRAINT fk_property_annual_fair_market_policy
        FOREIGN KEY (policy_set_id)
        REFERENCES property_annual_tax_policy_profile (policy_set_id),
    CONSTRAINT ck_property_annual_fair_market_band CHECK (
        band_order BETWEEN 1 AND 3
        AND (official_value_upper_bound_krw IS NULL
             OR official_value_upper_bound_krw > 0)
        AND fair_market_value_ratio_ppm BETWEEN 1 AND 1000000
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE property_annual_tax_rate_bracket (
    policy_set_id               BIGINT UNSIGNED NOT NULL,
    rate_schedule               VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    bracket_order               TINYINT UNSIGNED NOT NULL,
    tax_base_upper_bound_krw    BIGINT NULL,
    rate_ppm                    INT UNSIGNED NOT NULL,
    progressive_deduction_krw   BIGINT NOT NULL,
    created_at                  DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (policy_set_id, rate_schedule, bracket_order),
    UNIQUE KEY uk_property_annual_tax_rate_bound
        (policy_set_id, rate_schedule, tax_base_upper_bound_krw),
    CONSTRAINT fk_property_annual_tax_rate_policy
        FOREIGN KEY (policy_set_id)
        REFERENCES property_annual_tax_policy_profile (policy_set_id),
    CONSTRAINT ck_property_annual_tax_rate_bracket CHECK (
        rate_schedule IN ('special', 'standard')
        AND bracket_order BETWEEN 1 AND 4
        AND (tax_base_upper_bound_krw IS NULL OR tax_base_upper_bound_krw > 0)
        AND rate_ppm BETWEEN 1 AND 1000000
        AND progressive_deduction_krw >= 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE property_capital_gains_tax_policy_profile (
    policy_set_id                       BIGINT UNSIGNED NOT NULL,
    rule_id                             BIGINT UNSIGNED NOT NULL,
    supported_home_count                TINYINT UNSIGNED NOT NULL,
    high_value_threshold_krw            BIGINT NOT NULL,
    basic_deduction_krw                 BIGINT NOT NULL,
    minimum_holding_years               TINYINT UNSIGNED NOT NULL,
    minimum_residence_years             TINYINT UNSIGNED NOT NULL,
    holding_deduction_start_years       TINYINT UNSIGNED NOT NULL,
    holding_deduction_start_rate_ppm    INT UNSIGNED NOT NULL,
    holding_deduction_per_year_ppm      INT UNSIGNED NOT NULL,
    holding_deduction_maximum_ppm       INT UNSIGNED NOT NULL,
    residence_deduction_start_years     TINYINT UNSIGNED NOT NULL,
    residence_deduction_start_rate_ppm  INT UNSIGNED NOT NULL,
    residence_deduction_per_year_ppm    INT UNSIGNED NOT NULL,
    residence_deduction_maximum_ppm     INT UNSIGNED NOT NULL,
    local_income_tax_ratio_ppm          INT UNSIGNED NOT NULL,
    payment_rule                        VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin
                                            NOT NULL,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (policy_set_id),
    UNIQUE KEY uk_property_capital_gains_tax_policy_rule (rule_id),
    CONSTRAINT fk_property_capital_gains_tax_policy_set
        FOREIGN KEY (policy_set_id) REFERENCES policy_set (id),
    CONSTRAINT fk_property_capital_gains_tax_policy_rule
        FOREIGN KEY (rule_id) REFERENCES policy_rule (id),
    CONSTRAINT ck_property_capital_gains_tax_fixture CHECK (
        supported_home_count = 1
        AND high_value_threshold_krw = 1200000000
        AND basic_deduction_krw = 2500000
        AND minimum_holding_years = 2
        AND minimum_residence_years = 2
        AND holding_deduction_start_years = 3
        AND holding_deduction_start_rate_ppm = 120000
        AND holding_deduction_per_year_ppm = 40000
        AND holding_deduction_maximum_ppm = 400000
        AND residence_deduction_start_years = 2
        AND residence_deduction_start_rate_ppm = 80000
        AND residence_deduction_per_year_ppm = 40000
        AND residence_deduction_maximum_ppm = 400000
        AND local_income_tax_ratio_ppm = 100000
        AND payment_rule = 'withheldAtSale'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE property_capital_gains_tax_rate_bracket (
    policy_set_id                       BIGINT UNSIGNED NOT NULL,
    tax_scope                           VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin
                                            NOT NULL,
    bracket_order                       TINYINT UNSIGNED NOT NULL,
    taxable_amount_upper_bound_krw      BIGINT NULL,
    rate_ppm                            INT UNSIGNED NOT NULL,
    progressive_deduction_krw           BIGINT NOT NULL,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (policy_set_id, tax_scope, bracket_order),
    UNIQUE KEY uk_property_capital_gains_tax_bound
        (policy_set_id, tax_scope, taxable_amount_upper_bound_krw),
    CONSTRAINT fk_property_capital_gains_tax_rate_policy
        FOREIGN KEY (policy_set_id)
        REFERENCES property_capital_gains_tax_policy_profile (policy_set_id),
    CONSTRAINT ck_property_capital_gains_tax_rate_bracket CHECK (
        tax_scope IN ('national', 'local')
        AND bracket_order BETWEEN 1 AND 8
        AND (taxable_amount_upper_bound_krw IS NULL
             OR taxable_amount_upper_bound_krw > 0)
        AND rate_ppm BETWEEN 1 AND 1000000
        AND progressive_deduction_krw >= 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_property_acquisition_tax_policy_draft_insert
BEFORE INSERT ON property_acquisition_tax_policy_profile
FOR EACH ROW
SET NEW.policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM policy_set AS policy
        INNER JOIN policy_rule AS rule
            ON rule.policy_set_id = policy.id
           AND rule.id = NEW.rule_id
        WHERE policy.id = NEW.policy_set_id
          AND policy.policy_key = 'dev-unranked-kr-individual-property-2026-v3'
          AND policy.sealed_at IS NULL
          AND rule.domain = 'propertyTax'
          AND rule.rule_key = 'singleHomeAcquisitionTax'
    ),
    NEW.policy_set_id,
    NULL
);

CREATE TRIGGER tr_property_acquisition_tax_policy_no_update
BEFORE UPDATE ON property_acquisition_tax_policy_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property acquisition tax policies are immutable';

CREATE TRIGGER tr_property_acquisition_tax_policy_no_delete
BEFORE DELETE ON property_acquisition_tax_policy_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property acquisition tax policies are immutable';

CREATE TRIGGER tr_property_annual_tax_policy_draft_insert
BEFORE INSERT ON property_annual_tax_policy_profile
FOR EACH ROW
SET NEW.policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM policy_set AS policy
        INNER JOIN policy_rule AS rule
            ON rule.policy_set_id = policy.id
           AND rule.id = NEW.rule_id
        WHERE policy.id = NEW.policy_set_id
          AND policy.policy_key = 'dev-unranked-kr-individual-property-2026-v3'
          AND policy.sealed_at IS NULL
          AND rule.domain = 'propertyTax'
          AND rule.rule_key = 'singleHomeAnnualPropertyTax'
    ),
    NEW.policy_set_id,
    NULL
);

CREATE TRIGGER tr_property_annual_tax_policy_no_update
BEFORE UPDATE ON property_annual_tax_policy_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'annual property tax policies are immutable';

CREATE TRIGGER tr_property_annual_tax_policy_no_delete
BEFORE DELETE ON property_annual_tax_policy_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'annual property tax policies are immutable';

CREATE TRIGGER tr_property_annual_fair_market_draft_insert
BEFORE INSERT ON property_annual_tax_fair_market_ratio_band
FOR EACH ROW
SET NEW.policy_set_id = IF(
    EXISTS (
        SELECT 1 FROM policy_set AS policy
        WHERE policy.id = NEW.policy_set_id
          AND policy.policy_key = 'dev-unranked-kr-individual-property-2026-v3'
          AND policy.sealed_at IS NULL
    ),
    NEW.policy_set_id,
    NULL
);

CREATE TRIGGER tr_property_annual_fair_market_no_update
BEFORE UPDATE ON property_annual_tax_fair_market_ratio_band
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property fair-market bands are immutable';

CREATE TRIGGER tr_property_annual_fair_market_no_delete
BEFORE DELETE ON property_annual_tax_fair_market_ratio_band
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property fair-market bands are immutable';

CREATE TRIGGER tr_property_annual_tax_rate_draft_insert
BEFORE INSERT ON property_annual_tax_rate_bracket
FOR EACH ROW
SET NEW.policy_set_id = IF(
    EXISTS (
        SELECT 1 FROM policy_set AS policy
        WHERE policy.id = NEW.policy_set_id
          AND policy.policy_key = 'dev-unranked-kr-individual-property-2026-v3'
          AND policy.sealed_at IS NULL
    ),
    NEW.policy_set_id,
    NULL
);

CREATE TRIGGER tr_property_annual_tax_rate_no_update
BEFORE UPDATE ON property_annual_tax_rate_bracket
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'annual property tax brackets are immutable';

CREATE TRIGGER tr_property_annual_tax_rate_no_delete
BEFORE DELETE ON property_annual_tax_rate_bracket
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'annual property tax brackets are immutable';

CREATE TRIGGER tr_property_capital_gains_tax_policy_draft_insert
BEFORE INSERT ON property_capital_gains_tax_policy_profile
FOR EACH ROW
SET NEW.policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM policy_set AS policy
        INNER JOIN policy_rule AS rule
            ON rule.policy_set_id = policy.id
           AND rule.id = NEW.rule_id
        WHERE policy.id = NEW.policy_set_id
          AND policy.policy_key = 'dev-unranked-kr-individual-property-2026-v3'
          AND policy.sealed_at IS NULL
          AND rule.domain = 'propertyTax'
          AND rule.rule_key = 'singleHomeCapitalGainsTax'
    ),
    NEW.policy_set_id,
    NULL
);

CREATE TRIGGER tr_property_capital_gains_tax_policy_no_update
BEFORE UPDATE ON property_capital_gains_tax_policy_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property capital-gains policies are immutable';

CREATE TRIGGER tr_property_capital_gains_tax_policy_no_delete
BEFORE DELETE ON property_capital_gains_tax_policy_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property capital-gains policies are immutable';

CREATE TRIGGER tr_property_capital_gains_tax_rate_draft_insert
BEFORE INSERT ON property_capital_gains_tax_rate_bracket
FOR EACH ROW
SET NEW.policy_set_id = IF(
    EXISTS (
        SELECT 1 FROM policy_set AS policy
        WHERE policy.id = NEW.policy_set_id
          AND policy.policy_key = 'dev-unranked-kr-individual-property-2026-v3'
          AND policy.sealed_at IS NULL
    ),
    NEW.policy_set_id,
    NULL
);

CREATE TRIGGER tr_property_capital_gains_tax_rate_no_update
BEFORE UPDATE ON property_capital_gains_tax_rate_bracket
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property capital-gains brackets are immutable';

CREATE TRIGGER tr_property_capital_gains_tax_rate_no_delete
BEFORE DELETE ON property_capital_gains_tax_rate_bracket
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property capital-gains brackets are immutable';

INSERT INTO property_acquisition_tax_policy_profile
    (
        policy_set_id, rule_id, supported_home_count,
        lower_price_maximum_krw, middle_price_maximum_krw,
        lower_rate_ppm, upper_rate_ppm, middle_rate_price_divisor_krw,
        middle_rate_offset_ppm, middle_rate_rounding,
        local_education_rate_ratio_ppm, payment_due_days
    )
SELECT policy.id, rule.id, 1, 600000000, 900000000,
       10000, 30000, 15000, 30000, 'halfUp', 100000, 60
FROM policy_set AS policy
INNER JOIN policy_rule AS rule
    ON rule.policy_set_id = policy.id
   AND rule.rule_key = 'singleHomeAcquisitionTax'
WHERE policy.policy_key = 'dev-unranked-kr-individual-property-2026-v3';

INSERT INTO property_annual_tax_policy_profile
    (
        policy_set_id, rule_id, supported_home_count,
        assessment_month, assessment_day,
        ownership_cutoff_rule, official_value_ratio_ppm,
        special_rate_official_value_maximum_krw,
        local_education_rate_ratio_ppm,
        first_payment_month, first_payment_day,
        second_payment_month, second_payment_day, payment_split_rule,
        unsupported_exclusion_codes
    )
SELECT policy.id, rule.id, 1, 6, 1, 'priorDayClosingOwner', 700000, 900000000,
       200000, 7, 31, 9, 30, 'floorHalfThenRemainder',
       JSON_ARRAY(
           'taxBurdenCap', 'urbanAreaSurtax',
           'regionalResourceFacilityTax', 'localOrdinanceAdjustment'
       )
FROM policy_set AS policy
INNER JOIN policy_rule AS rule
    ON rule.policy_set_id = policy.id
   AND rule.rule_key = 'singleHomeAnnualPropertyTax'
WHERE policy.policy_key = 'dev-unranked-kr-individual-property-2026-v3';

INSERT INTO property_annual_tax_fair_market_ratio_band
    (policy_set_id, band_order, official_value_upper_bound_krw,
     fair_market_value_ratio_ppm)
SELECT policy.id, band.band_order, band.upper_bound_krw, band.ratio_ppm
FROM policy_set AS policy
INNER JOIN (
    SELECT 1 AS band_order, 300000000 AS upper_bound_krw, 430000 AS ratio_ppm
    UNION ALL SELECT 2, 600000000, 440000
    UNION ALL SELECT 3, NULL, 450000
) AS band
WHERE policy.policy_key = 'dev-unranked-kr-individual-property-2026-v3';

INSERT INTO property_annual_tax_rate_bracket
    (policy_set_id, rate_schedule, bracket_order,
     tax_base_upper_bound_krw, rate_ppm, progressive_deduction_krw)
SELECT policy.id, bracket.rate_schedule, bracket.bracket_order,
       bracket.upper_bound_krw, bracket.rate_ppm, bracket.deduction_krw
FROM policy_set AS policy
INNER JOIN (
    SELECT 'special' AS rate_schedule, 1 AS bracket_order,
           60000000 AS upper_bound_krw, 500 AS rate_ppm, 0 AS deduction_krw
    UNION ALL SELECT 'special', 2, 150000000, 1000, 30000
    UNION ALL SELECT 'special', 3, 300000000, 2000, 180000
    UNION ALL SELECT 'special', 4, NULL, 3500, 630000
    UNION ALL SELECT 'standard', 1, 60000000, 1000, 0
    UNION ALL SELECT 'standard', 2, 150000000, 1500, 30000
    UNION ALL SELECT 'standard', 3, 300000000, 2500, 180000
    UNION ALL SELECT 'standard', 4, NULL, 4000, 630000
) AS bracket
WHERE policy.policy_key = 'dev-unranked-kr-individual-property-2026-v3';

INSERT INTO property_capital_gains_tax_policy_profile
    (
        policy_set_id, rule_id, supported_home_count,
        high_value_threshold_krw, basic_deduction_krw,
        minimum_holding_years, minimum_residence_years,
        holding_deduction_start_years, holding_deduction_start_rate_ppm,
        holding_deduction_per_year_ppm, holding_deduction_maximum_ppm,
        residence_deduction_start_years, residence_deduction_start_rate_ppm,
        residence_deduction_per_year_ppm, residence_deduction_maximum_ppm,
        local_income_tax_ratio_ppm, payment_rule
    )
SELECT policy.id, rule.id, 1, 1200000000, 2500000, 2, 2,
       3, 120000, 40000, 400000, 2, 80000, 40000, 400000,
       100000, 'withheldAtSale'
FROM policy_set AS policy
INNER JOIN policy_rule AS rule
    ON rule.policy_set_id = policy.id
   AND rule.rule_key = 'singleHomeCapitalGainsTax'
WHERE policy.policy_key = 'dev-unranked-kr-individual-property-2026-v3';

INSERT INTO property_capital_gains_tax_rate_bracket
    (policy_set_id, tax_scope, bracket_order,
     taxable_amount_upper_bound_krw, rate_ppm, progressive_deduction_krw)
SELECT policy.id, bracket.tax_scope, bracket.bracket_order,
       bracket.upper_bound_krw, bracket.rate_ppm, bracket.deduction_krw
FROM policy_set AS policy
INNER JOIN (
    SELECT 'national' AS tax_scope, 1 AS bracket_order,
           14000000 AS upper_bound_krw, 60000 AS rate_ppm, 0 AS deduction_krw
    UNION ALL SELECT 'national', 2, 50000000, 150000, 1260000
    UNION ALL SELECT 'national', 3, 88000000, 240000, 5760000
    UNION ALL SELECT 'national', 4, 150000000, 350000, 15440000
    UNION ALL SELECT 'national', 5, 300000000, 380000, 19940000
    UNION ALL SELECT 'national', 6, 500000000, 400000, 25940000
    UNION ALL SELECT 'national', 7, 1000000000, 420000, 35940000
    UNION ALL SELECT 'national', 8, NULL, 450000, 65940000
    UNION ALL SELECT 'local', 1, 14000000, 6000, 0
    UNION ALL SELECT 'local', 2, 50000000, 15000, 126000
    UNION ALL SELECT 'local', 3, 88000000, 24000, 576000
    UNION ALL SELECT 'local', 4, 150000000, 35000, 1544000
    UNION ALL SELECT 'local', 5, 300000000, 38000, 1994000
    UNION ALL SELECT 'local', 6, 500000000, 40000, 2594000
    UNION ALL SELECT 'local', 7, 1000000000, 42000, 3594000
    UNION ALL SELECT 'local', 8, NULL, 45000, 6594000
) AS bracket
WHERE policy.policy_key = 'dev-unranked-kr-individual-property-2026-v3';

DROP TRIGGER tr_policy_set_seal_only;

CREATE TRIGGER tr_policy_set_seal_only
BEFORE UPDATE ON policy_set
FOR EACH ROW
FOLLOWS tr_policy_set_v2_financial_rate_match
SET NEW.policy_key = IF(
    OLD.sealed_at IS NULL
        AND NEW.sealed_at IS NOT NULL
        AND NEW.id = OLD.id
        AND BINARY NEW.policy_key = BINARY OLD.policy_key
        AND NEW.basis_date = OLD.basis_date
        AND NEW.ranked_eligible = OLD.ranked_eligible
        AND NEW.created_at = OLD.created_at
        AND NEW.canonical_sha256 REGEXP '^[0-9a-f]{64}$'
        AND EXISTS (
            SELECT 1
            FROM policy_set_canonical_manifest AS manifest
            WHERE manifest.policy_set_id = OLD.id
              AND BINARY manifest.canonical_sha256 = BINARY NEW.canonical_sha256
        )
        AND EXISTS (
            SELECT 1 FROM policy_rule AS rule WHERE rule.policy_set_id = OLD.id
        )
        AND NOT EXISTS (
            SELECT 1
            FROM policy_rule AS rule
            WHERE rule.policy_set_id = OLD.id
              AND NOT EXISTS (
                  SELECT 1
                  FROM policy_rule_source AS source_link
                  WHERE source_link.policy_rule_id = rule.id
              )
              AND NOT EXISTS (
                  SELECT 1
                  FROM policy_rule_legacy_provenance AS legacy
                  WHERE legacy.policy_rule_id = rule.id
              )
              AND NOT EXISTS (
                  SELECT 1
                  FROM policy_rule_clone_provenance AS clone
                  INNER JOIN policy_rule AS source_rule
                      ON source_rule.id = clone.source_policy_rule_id
                  INNER JOIN policy_set AS source_set
                      ON source_set.id = source_rule.policy_set_id
                  WHERE clone.target_policy_rule_id = rule.id
                    AND source_set.sealed_at IS NOT NULL
                    AND BINARY source_rule.domain = BINARY rule.domain
                    AND BINARY source_rule.rule_key = BINARY rule.rule_key
                    AND source_rule.effective_from = rule.effective_from
                    AND source_rule.effective_to <=> rule.effective_to
                    AND source_rule.parameters = rule.parameters
                    AND (
                        EXISTS (
                            SELECT 1 FROM policy_rule_source AS source_link
                            WHERE source_link.policy_rule_id = source_rule.id
                        )
                        OR EXISTS (
                            SELECT 1
                            FROM policy_rule_legacy_provenance AS legacy
                            WHERE legacy.policy_rule_id = source_rule.id
                        )
                    )
              )
        )
        AND (
            NEW.ranked_eligible = FALSE
            OR (
                NOT EXISTS (
                    SELECT 1
                    FROM policy_rule AS rule
                    INNER JOIN policy_rule_legacy_provenance AS legacy
                        ON legacy.policy_rule_id = rule.id
                    WHERE rule.policy_set_id = OLD.id
                )
                AND NOT EXISTS (
                    SELECT 1
                    FROM policy_rule AS rule
                    INNER JOIN policy_rule_clone_provenance AS clone
                        ON clone.target_policy_rule_id = rule.id
                    INNER JOIN policy_rule_legacy_provenance AS legacy
                        ON legacy.policy_rule_id = clone.source_policy_rule_id
                    WHERE rule.policy_set_id = OLD.id
                )
            )
        ),
    NEW.policy_key,
    NULL
);

INSERT INTO policy_set_canonical_manifest (policy_set_id, canonical_json)
SELECT
    policy.id,
    CONCAT(
        '{"basisDate":', JSON_QUOTE(DATE_FORMAT(policy.basis_date, '%Y-%m-%d')),
        ',"policyKey":', JSON_QUOTE(policy.policy_key),
        ',"rankedEligible":', IF(policy.ranked_eligible, 'true', 'false'),
        ',"rules":[',
        COALESCE((
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
        ), ''),
        '],"schemaVersion":1}'
    )
FROM policy_set AS policy
WHERE policy.policy_key = 'dev-unranked-kr-individual-property-2026-v3';

UPDATE policy_set AS policy
INNER JOIN policy_set_canonical_manifest AS manifest
    ON manifest.policy_set_id = policy.id
SET policy.canonical_sha256 = manifest.canonical_sha256,
    policy.sealed_at = CURRENT_TIMESTAMP(3)
WHERE policy.policy_key = 'dev-unranked-kr-individual-property-2026-v3'
  AND policy.sealed_at IS NULL;

CREATE TABLE real_estate_sale_liquidity_profile (
    real_estate_model_version_id     BIGINT UNSIGNED NOT NULL,
    minimum_asking_ratio_ppm         INT UNSIGNED NOT NULL,
    low_band_maximum_ratio_ppm       INT UNSIGNED NOT NULL,
    middle_band_maximum_ratio_ppm    INT UNSIGNED NOT NULL,
    maximum_asking_ratio_ppm         INT UNSIGNED NOT NULL,
    low_delay_minimum_days           SMALLINT UNSIGNED NOT NULL,
    low_delay_maximum_days           SMALLINT UNSIGNED NOT NULL,
    middle_delay_minimum_days        SMALLINT UNSIGNED NOT NULL,
    middle_delay_maximum_days        SMALLINT UNSIGNED NOT NULL,
    high_delay_minimum_days          SMALLINT UNSIGNED NOT NULL,
    high_delay_maximum_days          SMALLINT UNSIGNED NOT NULL,
    candidate_entropy_key            VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin
                                         NOT NULL,
    gross_price_rule                 VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin
                                         NOT NULL,
    disposition_cost_ppm             INT UNSIGNED NOT NULL,
    minimum_disposition_cost_krw     BIGINT NOT NULL,
    minimum_holding_years            TINYINT UNSIGNED NOT NULL,
    minimum_residence_years          TINYINT UNSIGNED NOT NULL,
    deficient_sale_proceeds          VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin
                                         NOT NULL,
    post_sale_tenure_type            VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin
                                         NOT NULL,
    provenance_kind                  VARCHAR(20) CHARACTER SET ascii COLLATE ascii_bin
                                         NOT NULL,
    created_at                       DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (real_estate_model_version_id),
    CONSTRAINT fk_real_estate_sale_liquidity_model
        FOREIGN KEY (real_estate_model_version_id)
        REFERENCES real_estate_model_version (id),
    CONSTRAINT ck_real_estate_sale_liquidity_fixture CHECK (
        minimum_asking_ratio_ppm = 800000
        AND low_band_maximum_ratio_ppm = 950000
        AND middle_band_maximum_ratio_ppm = 1050000
        AND maximum_asking_ratio_ppm = 1200000
        AND low_delay_minimum_days = 1
        AND low_delay_maximum_days = 3
        AND middle_delay_minimum_days = 3
        AND middle_delay_maximum_days = 7
        AND high_delay_minimum_days = 7
        AND high_delay_maximum_days = 30
        AND candidate_entropy_key = 'propertySaleCandidate'
        AND gross_price_rule = 'exactAskingPrice'
        AND disposition_cost_ppm = 5000
        AND minimum_disposition_cost_krw = 1
        AND minimum_holding_years = 2
        AND minimum_residence_years = 2
        AND deficient_sale_proceeds = 'reject'
        AND post_sale_tenure_type = 'rentFree'
        AND provenance_kind = 'GAME_BALANCE'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_real_estate_sale_liquidity_draft_insert
BEFORE INSERT ON real_estate_sale_liquidity_profile
FOR EACH ROW
SET NEW.real_estate_model_version_id = IF(
    EXISTS (
        SELECT 1 FROM real_estate_model_version AS model
        WHERE model.id = NEW.real_estate_model_version_id
          AND model.version_key
                = 'dev-unranked-m4-real-estate-sale-tax-2026-v6'
          AND model.availability = 'active'
          AND model.sealed_at IS NULL
          AND JSON_UNQUOTE(JSON_EXTRACT(model.parameters, '$.schemaVersion')) = '6'
    ),
    NEW.real_estate_model_version_id,
    NULL
);

CREATE TRIGGER tr_real_estate_sale_liquidity_no_update
BEFORE UPDATE ON real_estate_sale_liquidity_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'sale liquidity profiles are immutable';

CREATE TRIGGER tr_real_estate_sale_liquidity_no_delete
BEFORE DELETE ON real_estate_sale_liquidity_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'sale liquidity profiles are immutable';

DROP TRIGGER tr_real_estate_purchase_profile_draft_insert;

CREATE TRIGGER tr_real_estate_purchase_profile_draft_insert
BEFORE INSERT ON real_estate_purchase_profile
FOR EACH ROW
SET NEW.real_estate_model_version_id = IF(
    EXISTS (
        SELECT 1 FROM real_estate_model_version AS model
        WHERE model.id = NEW.real_estate_model_version_id
          AND model.availability = 'active'
          AND model.sealed_at IS NULL
          AND JSON_UNQUOTE(JSON_EXTRACT(model.parameters, '$.schemaVersion'))
                IN ('5', '6')
    ),
    NEW.real_estate_model_version_id,
    NULL
);

DROP TRIGGER tr_real_estate_model_manifest_draft_insert;
DROP TRIGGER tr_real_estate_model_version_seal_only;

RENAME TABLE real_estate_model_strict_projection
    TO real_estate_model_v1_v5_strict_projection;

CREATE VIEW real_estate_model_strict_projection AS
SELECT legacy.real_estate_model_version_id, legacy.canonical_json
FROM real_estate_model_v1_v5_strict_projection AS legacy
INNER JOIN real_estate_model_version AS model
    ON model.id = legacy.real_estate_model_version_id
WHERE JSON_TYPE(JSON_EXTRACT(model.parameters, '$.schemaVersion')) = 'INTEGER'
  AND JSON_UNQUOTE(JSON_EXTRACT(model.parameters, '$.schemaVersion'))
        IN ('1', '2', '3', '4', '5')
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
                'saleLiquidityProfile', (
                    SELECT JSON_OBJECT(
                               'candidateEntropyKey', profile.candidate_entropy_key,
                               'deficientSaleProceeds', profile.deficient_sale_proceeds,
                               'dispositionCostPpm', profile.disposition_cost_ppm,
                               'grossPriceRule', profile.gross_price_rule,
                               'highDelayMaximumDays', profile.high_delay_maximum_days,
                               'highDelayMinimumDays', profile.high_delay_minimum_days,
                               'lowBandMaximumRatioPpm',
                                   profile.low_band_maximum_ratio_ppm,
                               'lowDelayMaximumDays', profile.low_delay_maximum_days,
                               'lowDelayMinimumDays', profile.low_delay_minimum_days,
                               'maximumAskingRatioPpm', profile.maximum_asking_ratio_ppm,
                               'middleBandMaximumRatioPpm',
                                   profile.middle_band_maximum_ratio_ppm,
                               'middleDelayMaximumDays', profile.middle_delay_maximum_days,
                               'middleDelayMinimumDays', profile.middle_delay_minimum_days,
                               'minimumAskingRatioPpm', profile.minimum_asking_ratio_ppm,
                               'minimumDispositionCostKrw',
                                   profile.minimum_disposition_cost_krw,
                               'minimumHoldingYears', profile.minimum_holding_years,
                               'minimumResidenceYears', profile.minimum_residence_years,
                               'postSaleTenureType', profile.post_sale_tenure_type,
                               'provenanceKind', profile.provenance_kind,
                               'schemaVersion', 1
                           )
                    FROM real_estate_sale_liquidity_profile AS profile
                    WHERE profile.real_estate_model_version_id
                            = base.real_estate_model_version_id
                ),
                'schemaVersion', 6
            )
        ) AS CHAR CHARACTER SET utf8mb4
    ) AS canonical_json
FROM real_estate_model_v1_v4_strict_projection AS base
INNER JOIN real_estate_model_version AS model
    ON model.id = base.real_estate_model_version_id
WHERE JSON_TYPE(JSON_EXTRACT(model.parameters, '$.schemaVersion')) = 'INTEGER'
  AND JSON_UNQUOTE(JSON_EXTRACT(model.parameters, '$.schemaVersion')) = '6';

CREATE TEMPORARY TABLE m4c4_real_estate_projection_guard (
    guard_key VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    accepted  TINYINT UNSIGNED NOT NULL,
    CONSTRAINT ck_m4c4_real_estate_projection_guard CHECK (accepted = 1)
);

INSERT INTO m4c4_real_estate_projection_guard (guard_key, accepted)
SELECT 'legacy-model-bytes', IF(
    NOT EXISTS (
        SELECT 1
        FROM m4c4_legacy_real_estate_bytes AS legacy
        INNER JOIN real_estate_model_version AS model
            ON model.id = legacy.real_estate_model_version_id
        INNER JOIN real_estate_model_strict_manifest AS manifest
            ON manifest.real_estate_model_version_id = model.id
        LEFT JOIN real_estate_model_strict_projection AS projection
            ON projection.real_estate_model_version_id = model.id
        WHERE projection.real_estate_model_version_id IS NULL
           OR BINARY legacy.canonical_json <> BINARY manifest.canonical_json
           OR BINARY legacy.canonical_sha256 <> BINARY manifest.canonical_sha256
           OR BINARY legacy.model_sha256 <> BINARY model.canonical_sha256
           OR BINARY manifest.canonical_json <> BINARY projection.canonical_json
    ),
    1,
    0
);

DROP TEMPORARY TABLE m4c4_real_estate_projection_guard;

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
                AND JSON_UNQUOTE(JSON_EXTRACT(OLD.parameters, '$.schemaVersion')) = '6'
                AND OLD.version_key
                    = 'dev-unranked-m4-real-estate-sale-tax-2026-v6'
                AND (
                    SELECT COUNT(*) FROM real_estate_region_profile AS profile
                    WHERE profile.real_estate_model_version_id = OLD.id
                ) = (SELECT COUNT(*) FROM life_region)
                AND NOT EXISTS (
                    SELECT 1
                    FROM real_estate_region_profile AS profile
                    WHERE profile.real_estate_model_version_id = OLD.id
                      AND profile.maximum_exclusive_area_square_meters > 85
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
                    SELECT COUNT(*) FROM real_estate_lease_profile AS profile
                    WHERE profile.real_estate_model_version_id = OLD.id
                ) = 2
                AND (
                    SELECT COUNT(*) FROM real_estate_region_moving_cost AS cost
                    WHERE cost.real_estate_model_version_id = OLD.id
                ) = (SELECT COUNT(*) FROM life_region)
                AND EXISTS (
                    SELECT 1 FROM real_estate_purchase_profile AS profile
                    WHERE profile.real_estate_model_version_id = OLD.id
                      AND profile.purchase_capability = 'ownerOccupiedSingleHome'
                      AND profile.maximum_active_holdings = 1
                )
                AND (
                    SELECT COUNT(*) FROM real_estate_purchase_region_mapping AS mapping
                    WHERE mapping.real_estate_model_version_id = OLD.id
                ) = (SELECT COUNT(*) FROM life_region)
                AND EXISTS (
                    SELECT 1 FROM real_estate_sale_liquidity_profile AS sale
                    WHERE sale.real_estate_model_version_id = OLD.id
                      AND sale.minimum_asking_ratio_ppm = 800000
                      AND sale.maximum_asking_ratio_ppm = 1200000
                      AND sale.deficient_sale_proceeds = 'reject'
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
VALUES (
    'dev-unranked-m4-real-estate-sale-tax-2026-v6',
    'active',
    FALSE,
    JSON_OBJECT(
        'entropyVersion', 'sha256-counter-be-v1',
        'generatorVersion', 'm4-c1-v1',
        'schemaVersion', 6
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
SELECT target.id, source.region_key, source.monthly_listing_slot_count,
       source.minimum_exclusive_area_square_meters,
       LEAST(source.maximum_exclusive_area_square_meters, 85),
       source.base_price_per_square_meter_krw, source.price_daily_drift_ppm,
       source.price_daily_shock_amplitude_ppm, source.rent_daily_drift_ppm,
       source.rent_daily_shock_amplitude_ppm, source.minimum_index_ppm,
       source.maximum_index_ppm, source.minimum_price_variation_ppm,
       source.maximum_price_variation_ppm, source.jeonse_ratio_ppm,
       source.annual_gross_rent_yield_ppm, source.monthly_deposit_ratio_ppm,
       source.availability_rule, source.offer_rotation_rule
FROM real_estate_model_version AS target
INNER JOIN real_estate_model_version AS source_model
    ON source_model.version_key = 'dev-unranked-m4-real-estate-purchase-2026-v5'
INNER JOIN real_estate_region_profile AS source
    ON source.real_estate_model_version_id = source_model.id
WHERE target.version_key = 'dev-unranked-m4-real-estate-sale-tax-2026-v6';

INSERT INTO real_estate_region_property_type
    (real_estate_model_version_id, region_key, property_type, property_type_order)
SELECT target.id, source.region_key, source.property_type, source.property_type_order
FROM real_estate_model_version AS target
INNER JOIN real_estate_model_version AS source_model
    ON source_model.version_key = 'dev-unranked-m4-real-estate-purchase-2026-v5'
INNER JOIN real_estate_region_property_type AS source
    ON source.real_estate_model_version_id = source_model.id
WHERE target.version_key = 'dev-unranked-m4-real-estate-sale-tax-2026-v6';

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
    ON source_model.version_key = 'dev-unranked-m4-real-estate-purchase-2026-v5'
INNER JOIN real_estate_lease_profile AS source
    ON source.real_estate_model_version_id = source_model.id
WHERE target.version_key = 'dev-unranked-m4-real-estate-sale-tax-2026-v6';

INSERT INTO real_estate_region_moving_cost
    (real_estate_model_version_id, region_key, moving_cost_krw)
SELECT target.id, source.region_key, source.moving_cost_krw
FROM real_estate_model_version AS target
INNER JOIN real_estate_model_version AS source_model
    ON source_model.version_key = 'dev-unranked-m4-real-estate-purchase-2026-v5'
INNER JOIN real_estate_region_moving_cost AS source
    ON source.real_estate_model_version_id = source_model.id
WHERE target.version_key = 'dev-unranked-m4-real-estate-sale-tax-2026-v6';

INSERT INTO real_estate_purchase_profile
    (
        real_estate_model_version_id, purchase_capability, maximum_active_holdings,
        supported_offer_kind, supported_purpose, incidental_cost_ppm,
        minimum_incidental_cost_krw, collateral_value_rule, ltv_cost_treatment,
        listing_consumption_scope, provenance_kind
    )
SELECT target.id, source.purchase_capability, source.maximum_active_holdings,
       source.supported_offer_kind, source.supported_purpose, source.incidental_cost_ppm,
       source.minimum_incidental_cost_krw, source.collateral_value_rule,
       source.ltv_cost_treatment, source.listing_consumption_scope,
       source.provenance_kind
FROM real_estate_model_version AS target
INNER JOIN real_estate_model_version AS source_model
    ON source_model.version_key = 'dev-unranked-m4-real-estate-purchase-2026-v5'
INNER JOIN real_estate_purchase_profile AS source
    ON source.real_estate_model_version_id = source_model.id
WHERE target.version_key = 'dev-unranked-m4-real-estate-sale-tax-2026-v6';

INSERT INTO real_estate_purchase_region_mapping
    (real_estate_model_version_id, region_key, ltv_region_class, mapping_provenance)
SELECT target.id, source.region_key, source.ltv_region_class, source.mapping_provenance
FROM real_estate_model_version AS target
INNER JOIN real_estate_model_version AS source_model
    ON source_model.version_key = 'dev-unranked-m4-real-estate-purchase-2026-v5'
INNER JOIN real_estate_purchase_region_mapping AS source
    ON source.real_estate_model_version_id = source_model.id
WHERE target.version_key = 'dev-unranked-m4-real-estate-sale-tax-2026-v6';

INSERT INTO real_estate_sale_liquidity_profile
    (
        real_estate_model_version_id, minimum_asking_ratio_ppm,
        low_band_maximum_ratio_ppm, middle_band_maximum_ratio_ppm,
        maximum_asking_ratio_ppm, low_delay_minimum_days,
        low_delay_maximum_days, middle_delay_minimum_days,
        middle_delay_maximum_days, high_delay_minimum_days,
        high_delay_maximum_days, candidate_entropy_key, gross_price_rule,
        disposition_cost_ppm, minimum_disposition_cost_krw,
        minimum_holding_years, minimum_residence_years,
        deficient_sale_proceeds, post_sale_tenure_type, provenance_kind
    )
SELECT id, 800000, 950000, 1050000, 1200000,
       1, 3, 3, 7, 7, 30, 'propertySaleCandidate', 'exactAskingPrice',
       5000, 1, 2, 2, 'reject', 'rentFree', 'GAME_BALANCE'
FROM real_estate_model_version
WHERE version_key = 'dev-unranked-m4-real-estate-sale-tax-2026-v6';

INSERT INTO real_estate_model_strict_manifest
    (real_estate_model_version_id, canonical_json)
SELECT real_estate_model_version_id, canonical_json
FROM real_estate_model_strict_projection
WHERE real_estate_model_version_id = (
    SELECT id FROM real_estate_model_version
    WHERE version_key = 'dev-unranked-m4-real-estate-sale-tax-2026-v6'
);

UPDATE real_estate_model_version AS model
INNER JOIN real_estate_model_strict_manifest AS manifest
    ON manifest.real_estate_model_version_id = model.id
SET model.canonical_sha256 = manifest.canonical_sha256,
    model.sealed_at = CURRENT_TIMESTAMP(3)
WHERE model.version_key = 'dev-unranked-m4-real-estate-sale-tax-2026-v6'
  AND model.sealed_at IS NULL;

-- C3 did not need the acquisition-day index. C4 backfills it from the immutable daily cache
-- before making it authoritative for every reference-value calculation.
DROP TRIGGER tr_property_holding_transition_only;

ALTER TABLE property_holding
    ADD COLUMN acquisition_price_index_ppm BIGINT NULL
        AFTER acquisition_incidental_cost_krw;

UPDATE property_holding AS holding
INNER JOIN save
    ON save.id = holding.save_id
INNER JOIN real_estate_daily AS daily
    ON daily.market_world_id = save.market_world_id
   AND daily.real_estate_model_version_id = holding.real_estate_model_version_id
   AND BINARY daily.region_key = BINARY holding.region_key
   AND daily.game_day = holding.acquired_game_day
SET holding.acquisition_price_index_ppm = daily.price_index_ppm
WHERE holding.acquisition_price_index_ppm IS NULL;

CREATE TEMPORARY TABLE m4c4_holding_index_guard (
    guard_key VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    accepted  TINYINT UNSIGNED NOT NULL,
    CONSTRAINT ck_m4c4_holding_index_guard CHECK (accepted = 1)
);

INSERT INTO m4c4_holding_index_guard (guard_key, accepted)
SELECT 'all-holdings-indexed', IF(
    NOT EXISTS (
        SELECT 1 FROM property_holding
        WHERE acquisition_price_index_ppm IS NULL
           OR acquisition_price_index_ppm <= 0
    ),
    1,
    0
);

DROP TEMPORARY TABLE m4c4_holding_index_guard;

ALTER TABLE property_holding
    MODIFY COLUMN acquisition_price_index_ppm BIGINT NOT NULL,
    ADD CONSTRAINT ck_property_holding_acquisition_index CHECK (
        acquisition_price_index_ppm BETWEEN 1 AND 9007199254740991
    );

CREATE TABLE property_sale_order (
    id                       BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                  BIGINT UNSIGNED NOT NULL,
    run_revision             INT UNSIGNED NOT NULL,
    household_id             BIGINT UNSIGNED NOT NULL,
    property_holding_id      BIGINT UNSIGNED NOT NULL,
    status                   VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    current_revision_no      SMALLINT UNSIGNED NOT NULL,
    created_game_day         INT UNSIGNED NOT NULL,
    terminal_game_day        INT UNSIGNED NULL,
    terminal_reason          VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NULL,
    active_order_slot        TINYINT UNSIGNED GENERATED ALWAYS AS (
        CASE WHEN status = 'active' THEN 1 ELSE NULL END
    ) STORED,
    created_at               DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at               DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
                                 ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_property_sale_order_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_property_sale_order_active_holding
        (save_id, run_revision, property_holding_id, active_order_slot),
    KEY ix_property_sale_order_household_status
        (household_id, status, id),
    CONSTRAINT fk_property_sale_order_household
        FOREIGN KEY (save_id, run_revision, household_id)
        REFERENCES household (save_id, run_revision, id),
    CONSTRAINT fk_property_sale_order_holding
        FOREIGN KEY (save_id, run_revision, property_holding_id)
        REFERENCES property_holding (save_id, run_revision, id),
    CONSTRAINT ck_property_sale_order_revision CHECK (current_revision_no > 0),
    CONSTRAINT ck_property_sale_order_state CHECK (
        (
            status = 'active'
            AND terminal_game_day IS NULL
            AND terminal_reason IS NULL
        )
        OR (
            status = 'filled'
            AND terminal_game_day IS NOT NULL
            AND terminal_reason IS NULL
        )
        OR (
            status = 'cancelled'
            AND terminal_game_day IS NOT NULL
            AND terminal_reason IN ('userRequest', 'newRun')
        )
        OR (
            status = 'rejected'
            AND terminal_game_day IS NOT NULL
            AND terminal_reason IN (
                'mortgageNotPayable', 'insufficientProceeds', 'policyUnsupported'
            )
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE property_sale_order_revision (
    id                               BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                          BIGINT UNSIGNED NOT NULL,
    run_revision                     INT UNSIGNED NOT NULL,
    property_sale_order_id           BIGINT UNSIGNED NOT NULL,
    property_holding_id              BIGINT UNSIGNED NOT NULL,
    revision_no                      SMALLINT UNSIGNED NOT NULL,
    revision_kind                    VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin
                                         NOT NULL,
    command_id                       CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    cancellation_reason              VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL,
    created_game_day                 INT UNSIGNED NOT NULL,
    real_estate_model_version_id     BIGINT UNSIGNED NOT NULL,
    policy_set_id                    BIGINT UNSIGNED NOT NULL,
    capital_gains_policy_rule_id     BIGINT UNSIGNED NOT NULL,
    reference_value_krw              BIGINT NULL,
    acquisition_price_index_ppm      BIGINT NULL,
    current_price_index_ppm          BIGINT NULL,
    asking_price_krw                 BIGINT NULL,
    asking_ratio_ppm                 INT UNSIGNED NULL,
    candidate_game_day               INT UNSIGNED NULL,
    gross_price_rule                 VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NULL,
    disposition_cost_ppm             INT UNSIGNED NULL,
    minimum_disposition_cost_krw     BIGINT NULL,
    deficient_sale_proceeds          VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NULL,
    minimum_holding_years            TINYINT UNSIGNED NULL,
    minimum_residence_years          TINYINT UNSIGNED NULL,
    created_at                       DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_property_sale_revision_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_property_sale_revision_order_no
        (save_id, run_revision, property_sale_order_id, revision_no),
    UNIQUE KEY uk_property_sale_revision_command
        (save_id, run_revision, command_id),
    KEY ix_property_sale_revision_candidate
        (save_id, run_revision, candidate_game_day, id),
    KEY ix_property_sale_revision_holding
        (save_id, run_revision, property_holding_id),
    KEY ix_property_sale_revision_model (real_estate_model_version_id),
    KEY ix_property_sale_revision_policy (policy_set_id),
    KEY ix_property_sale_revision_rule (capital_gains_policy_rule_id),
    CONSTRAINT fk_property_sale_revision_order
        FOREIGN KEY (save_id, run_revision, property_sale_order_id)
        REFERENCES property_sale_order (save_id, run_revision, id),
    CONSTRAINT fk_property_sale_revision_holding
        FOREIGN KEY (save_id, run_revision, property_holding_id)
        REFERENCES property_holding (save_id, run_revision, id),
    CONSTRAINT fk_property_sale_revision_command
        FOREIGN KEY (save_id, command_id)
        REFERENCES command_identity (save_id, command_id),
    CONSTRAINT fk_property_sale_revision_model
        FOREIGN KEY (real_estate_model_version_id)
        REFERENCES real_estate_model_version (id),
    CONSTRAINT fk_property_sale_revision_policy
        FOREIGN KEY (policy_set_id) REFERENCES policy_set (id),
    CONSTRAINT fk_property_sale_revision_capital_rule
        FOREIGN KEY (capital_gains_policy_rule_id) REFERENCES policy_rule (id),
    CONSTRAINT ck_property_sale_revision_no CHECK (revision_no > 0),
    CONSTRAINT ck_property_sale_revision_command CHECK (
        command_id IS NULL
        OR command_id REGEXP
            '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
    ),
    CONSTRAINT ck_property_sale_revision_shape CHECK (
        (
            revision_kind = 'listing'
            AND command_id IS NOT NULL
            AND cancellation_reason IS NULL
            AND reference_value_krw > 0
            AND acquisition_price_index_ppm > 0
            AND current_price_index_ppm > 0
            AND asking_price_krw > 0
            AND asking_ratio_ppm BETWEEN 800000 AND 1200000
            AND candidate_game_day > created_game_day
            AND gross_price_rule = 'exactAskingPrice'
            AND disposition_cost_ppm = 5000
            AND minimum_disposition_cost_krw = 1
            AND deficient_sale_proceeds = 'reject'
            AND minimum_holding_years = 2
            AND minimum_residence_years = 2
        )
        OR (
            revision_kind = 'cancellation'
            AND cancellation_reason IN ('userRequest', 'newRun')
            AND (
                (cancellation_reason = 'userRequest' AND command_id IS NOT NULL)
                OR (cancellation_reason = 'newRun' AND command_id IS NULL)
            )
            AND reference_value_krw IS NULL
            AND acquisition_price_index_ppm IS NULL
            AND current_price_index_ppm IS NULL
            AND asking_price_krw IS NULL
            AND asking_ratio_ppm IS NULL
            AND candidate_game_day IS NULL
            AND gross_price_rule IS NULL
            AND disposition_cost_ppm IS NULL
            AND minimum_disposition_cost_krw IS NULL
            AND deficient_sale_proceeds IS NULL
            AND minimum_holding_years IS NULL
            AND minimum_residence_years IS NULL
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE property_sale_execution (
    id                               BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                          BIGINT UNSIGNED NOT NULL,
    run_revision                     INT UNSIGNED NOT NULL,
    property_sale_order_id           BIGINT UNSIGNED NOT NULL,
    property_sale_order_revision_id  BIGINT UNSIGNED NOT NULL,
    property_holding_id              BIGINT UNSIGNED NOT NULL,
    execution_game_day               INT UNSIGNED NOT NULL,
    status                           VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin
                                         NOT NULL,
    rejection_reason                 VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NULL,
    book_value_krw                   BIGINT NOT NULL,
    gross_sale_price_krw             BIGINT NOT NULL,
    disposition_cost_krw             BIGINT NOT NULL,
    mortgage_principal_krw           BIGINT NOT NULL,
    mortgage_prepayment_fee_krw      BIGINT NOT NULL,
    transfer_tax_krw                 BIGINT NOT NULL,
    net_wallet_proceeds_krw          BIGINT NOT NULL,
    ledger_transaction_id            BIGINT UNSIGNED NULL,
    replacement_residence_id         BIGINT UNSIGNED NULL,
    created_at                       DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                       DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
                                         ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_property_sale_execution_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_property_sale_execution_order
        (save_id, run_revision, property_sale_order_id),
    UNIQUE KEY uk_property_sale_execution_revision
        (save_id, run_revision, property_sale_order_revision_id),
    UNIQUE KEY uk_property_sale_execution_ledger
        (save_id, run_revision, ledger_transaction_id),
    UNIQUE KEY uk_property_sale_execution_residence
        (save_id, run_revision, replacement_residence_id),
    KEY ix_property_sale_execution_holding
        (save_id, run_revision, property_holding_id),
    CONSTRAINT fk_property_sale_execution_order
        FOREIGN KEY (save_id, run_revision, property_sale_order_id)
        REFERENCES property_sale_order (save_id, run_revision, id),
    CONSTRAINT fk_property_sale_execution_revision
        FOREIGN KEY (save_id, run_revision, property_sale_order_revision_id)
        REFERENCES property_sale_order_revision (save_id, run_revision, id),
    CONSTRAINT fk_property_sale_execution_holding
        FOREIGN KEY (save_id, run_revision, property_holding_id)
        REFERENCES property_holding (save_id, run_revision, id),
    CONSTRAINT fk_property_sale_execution_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT fk_property_sale_execution_residence
        FOREIGN KEY (save_id, run_revision, replacement_residence_id)
        REFERENCES residence (save_id, run_revision, id),
    CONSTRAINT ck_property_sale_execution_amounts CHECK (
        book_value_krw > 0
        AND gross_sale_price_krw > 0
        AND disposition_cost_krw > 0
        AND mortgage_principal_krw >= 0
        AND mortgage_prepayment_fee_krw >= 0
        AND transfer_tax_krw >= 0
        AND net_wallet_proceeds_krw
              = gross_sale_price_krw - disposition_cost_krw
                - mortgage_principal_krw - mortgage_prepayment_fee_krw
                - transfer_tax_krw
    ),
    CONSTRAINT ck_property_sale_execution_state CHECK (
        (
            status = 'prepared'
            AND rejection_reason IS NULL
            AND net_wallet_proceeds_krw >= 0
            AND ledger_transaction_id IS NULL
            AND replacement_residence_id IS NULL
        )
        OR (
            status = 'applied'
            AND rejection_reason IS NULL
            AND net_wallet_proceeds_krw >= 0
            AND ledger_transaction_id IS NOT NULL
            AND replacement_residence_id IS NOT NULL
        )
        OR (
            status = 'rejected'
            AND rejection_reason IN (
                'mortgageNotPayable', 'insufficientProceeds', 'policyUnsupported'
            )
            AND ledger_transaction_id IS NULL
            AND replacement_residence_id IS NULL
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE property_tax_event (
    id                               BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                          BIGINT UNSIGNED NOT NULL,
    run_revision                     INT UNSIGNED NOT NULL,
    household_id                     BIGINT UNSIGNED NOT NULL,
    property_holding_id              BIGINT UNSIGNED NOT NULL,
    policy_set_id                    BIGINT UNSIGNED NOT NULL,
    policy_rule_id                   BIGINT UNSIGNED NOT NULL,
    property_sale_execution_id       BIGINT UNSIGNED NULL,
    event_kind                       VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin
                                         NOT NULL,
    status                           VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin
                                         NOT NULL,
    tax_year                         SMALLINT UNSIGNED NOT NULL,
    legal_basis_date                 DATE NOT NULL,
    assessment_game_day              INT UNSIGNED NOT NULL,
    taxable_game_day                 INT UNSIGNED NOT NULL,
    household_home_count             TINYINT UNSIGNED NOT NULL,
    valuation_game_day               INT UNSIGNED NULL,
    valuation_price_index_ppm        BIGINT NULL,
    valuation_amount_krw             BIGINT NOT NULL,
    official_value_krw               BIGINT NULL,
    tax_base_krw                     BIGINT NOT NULL,
    deduction_krw                    BIGINT NOT NULL,
    acquisition_taxes_krw            BIGINT NULL,
    disposition_cost_krw             BIGINT NULL,
    gross_gain_krw                   BIGINT NULL,
    high_value_gain_krw              BIGINT NULL,
    long_term_deduction_krw          BIGINT NULL,
    completed_holding_years          TINYINT UNSIGNED NULL,
    completed_residence_years        TINYINT UNSIGNED NULL,
    total_tax_krw                    BIGINT NOT NULL,
    paid_tax_krw                     BIGINT NOT NULL DEFAULT 0,
    exclusion_codes                  JSON NOT NULL,
    created_at                       DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                       DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
                                         ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_property_tax_event_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_property_tax_event_holding_kind_year
        (save_id, run_revision, property_holding_id, event_kind, tax_year),
    UNIQUE KEY uk_property_tax_event_sale_execution
        (save_id, run_revision, property_sale_execution_id),
    KEY ix_property_tax_event_holding_history
        (property_holding_id, id),
    KEY ix_property_tax_event_household_status
        (household_id, status, assessment_game_day, id),
    KEY ix_property_tax_event_policy (policy_set_id),
    KEY ix_property_tax_event_rule (policy_rule_id),
    CONSTRAINT fk_property_tax_event_household
        FOREIGN KEY (save_id, run_revision, household_id)
        REFERENCES household (save_id, run_revision, id),
    CONSTRAINT fk_property_tax_event_holding
        FOREIGN KEY (save_id, run_revision, property_holding_id)
        REFERENCES property_holding (save_id, run_revision, id),
    CONSTRAINT fk_property_tax_event_policy
        FOREIGN KEY (policy_set_id) REFERENCES policy_set (id),
    CONSTRAINT fk_property_tax_event_rule
        FOREIGN KEY (policy_rule_id) REFERENCES policy_rule (id),
    CONSTRAINT fk_property_tax_event_sale_execution
        FOREIGN KEY (save_id, run_revision, property_sale_execution_id)
        REFERENCES property_sale_execution (save_id, run_revision, id),
    CONSTRAINT ck_property_tax_event_year CHECK (tax_year BETWEEN 1 AND 9999),
    CONSTRAINT ck_property_tax_event_amounts CHECK (
        valuation_amount_krw > 0
        AND (official_value_krw IS NULL OR official_value_krw > 0)
        AND tax_base_krw >= 0
        AND deduction_krw >= 0
        AND total_tax_krw >= 0
        AND paid_tax_krw BETWEEN 0 AND total_tax_krw
        AND household_home_count > 0
        AND JSON_TYPE(exclusion_codes) = 'ARRAY'
        AND JSON_LENGTH(exclusion_codes) <= 16
    ),
    CONSTRAINT ck_property_tax_event_kind_shape CHECK (
        (
            event_kind = 'acquisition'
            AND property_sale_execution_id IS NULL
            AND official_value_krw IS NULL
            AND acquisition_taxes_krw IS NULL
            AND disposition_cost_krw IS NULL
            AND gross_gain_krw IS NULL
            AND high_value_gain_krw IS NULL
            AND long_term_deduction_krw IS NULL
            AND completed_holding_years IS NULL
            AND completed_residence_years IS NULL
        )
        OR (
            event_kind = 'annualProperty'
            AND property_sale_execution_id IS NULL
            AND official_value_krw IS NOT NULL
            AND acquisition_taxes_krw IS NULL
            AND disposition_cost_krw IS NULL
            AND gross_gain_krw IS NULL
            AND high_value_gain_krw IS NULL
            AND long_term_deduction_krw IS NULL
            AND completed_holding_years IS NULL
            AND completed_residence_years IS NULL
        )
        OR (
            event_kind = 'capitalGains'
            AND property_sale_execution_id IS NOT NULL
            AND official_value_krw IS NULL
            AND acquisition_taxes_krw IS NOT NULL
            AND disposition_cost_krw IS NOT NULL
            AND gross_gain_krw IS NOT NULL
            AND high_value_gain_krw IS NOT NULL
            AND long_term_deduction_krw IS NOT NULL
            AND completed_holding_years IS NOT NULL
            AND completed_residence_years IS NOT NULL
        )
    ),
    CONSTRAINT ck_property_tax_event_status CHECK (
        (status = 'prepared' AND paid_tax_krw = 0)
        OR (status = 'scheduled' AND total_tax_krw > 0 AND paid_tax_krw = 0)
        OR (
            status = 'partiallyPaid'
            AND paid_tax_krw > 0
            AND paid_tax_krw < total_tax_krw
        )
        OR (status = 'paid' AND total_tax_krw > 0 AND paid_tax_krw = total_tax_krw)
        OR (status = 'noPaymentRequired' AND total_tax_krw = 0 AND paid_tax_krw = 0)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE property_tax_component (
    id                       BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                  BIGINT UNSIGNED NOT NULL,
    run_revision             INT UNSIGNED NOT NULL,
    property_tax_event_id    BIGINT UNSIGNED NOT NULL,
    component_order          TINYINT UNSIGNED NOT NULL,
    component_kind           VARCHAR(40) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    tax_base_krw             BIGINT NOT NULL,
    deduction_krw            BIGINT NOT NULL,
    taxable_amount_krw       BIGINT NOT NULL,
    rate_ppm                 INT UNSIGNED NOT NULL,
    progressive_deduction_krw BIGINT NOT NULL,
    tax_amount_krw           BIGINT NOT NULL,
    calculation_evidence     JSON NOT NULL,
    created_at               DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_property_tax_component_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_property_tax_component_order
        (save_id, run_revision, property_tax_event_id, component_order),
    UNIQUE KEY uk_property_tax_component_kind
        (save_id, run_revision, property_tax_event_id, component_kind),
    CONSTRAINT fk_property_tax_component_event
        FOREIGN KEY (save_id, run_revision, property_tax_event_id)
        REFERENCES property_tax_event (save_id, run_revision, id),
    CONSTRAINT ck_property_tax_component_kind CHECK (
        component_kind IN (
            'acquisitionTax', 'acquisitionLocalEducationTax',
            'annualPropertyTax', 'annualPropertyLocalEducationTax',
            'capitalGainsTax', 'capitalGainsLocalIncomeTax'
        )
    ),
    CONSTRAINT ck_property_tax_component_amounts CHECK (
        component_order BETWEEN 1 AND 8
        AND tax_base_krw >= 0
        AND deduction_krw >= 0
        AND taxable_amount_krw >= 0
        AND rate_ppm <= 1000000
        AND progressive_deduction_krw >= 0
        AND tax_amount_krw >= 0
        AND JSON_TYPE(calculation_evidence) = 'OBJECT'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

ALTER TABLE scheduled_settlement
    ADD UNIQUE KEY uk_scheduled_settlement_save_run_id (save_id, run_revision, id);

CREATE TABLE property_tax_payment (
    id                       BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                  BIGINT UNSIGNED NOT NULL,
    run_revision             INT UNSIGNED NOT NULL,
    property_tax_event_id    BIGINT UNSIGNED NOT NULL,
    payment_no               TINYINT UNSIGNED NOT NULL,
    due_game_day             INT UNSIGNED NOT NULL,
    amount_krw               BIGINT NOT NULL,
    status                   VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    paid_from_wallet_krw     BIGINT NOT NULL DEFAULT 0,
    obligated_amount_krw     BIGINT NOT NULL DEFAULT 0,
    scheduled_settlement_id  BIGINT UNSIGNED NULL,
    ledger_transaction_id    BIGINT UNSIGNED NULL,
    tax_obligation_id        BIGINT UNSIGNED NULL,
    paid_game_day            INT UNSIGNED NULL,
    cancelled_game_day       INT UNSIGNED NULL,
    cancellation_reason      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL,
    created_at               DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at               DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
                                 ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_property_tax_payment_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_property_tax_payment_event_no
        (save_id, run_revision, property_tax_event_id, payment_no),
    UNIQUE KEY uk_property_tax_payment_settlement
        (save_id, run_revision, scheduled_settlement_id),
    UNIQUE KEY uk_property_tax_payment_ledger
        (save_id, run_revision, ledger_transaction_id),
    UNIQUE KEY uk_property_tax_payment_obligation
        (save_id, run_revision, tax_obligation_id),
    KEY ix_property_tax_payment_due
        (save_id, run_revision, status, due_game_day, id),
    CONSTRAINT fk_property_tax_payment_event
        FOREIGN KEY (save_id, run_revision, property_tax_event_id)
        REFERENCES property_tax_event (save_id, run_revision, id),
    CONSTRAINT fk_property_tax_payment_settlement
        FOREIGN KEY (save_id, run_revision, scheduled_settlement_id)
        REFERENCES scheduled_settlement (save_id, run_revision, id),
    CONSTRAINT fk_property_tax_payment_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT fk_property_tax_payment_obligation
        FOREIGN KEY (save_id, run_revision, tax_obligation_id)
        REFERENCES tax_obligation (save_id, run_revision, id),
    CONSTRAINT ck_property_tax_payment_amount CHECK (
        payment_no BETWEEN 1 AND 2
        AND amount_krw > 0
        AND paid_from_wallet_krw >= 0
        AND obligated_amount_krw >= 0
    ),
    CONSTRAINT ck_property_tax_payment_state CHECK (
        (
            status = 'pending'
            AND paid_from_wallet_krw = 0
            AND obligated_amount_krw = 0
            AND ledger_transaction_id IS NULL
            AND tax_obligation_id IS NULL
            AND paid_game_day IS NULL
            AND cancelled_game_day IS NULL
            AND cancellation_reason IS NULL
        )
        OR (
            status = 'applied'
            AND paid_from_wallet_krw + obligated_amount_krw = amount_krw
            AND ledger_transaction_id IS NOT NULL
            AND (
                (obligated_amount_krw = 0 AND tax_obligation_id IS NULL)
                OR (obligated_amount_krw > 0 AND tax_obligation_id IS NOT NULL)
            )
            AND paid_game_day IS NOT NULL
            AND cancelled_game_day IS NULL
            AND cancellation_reason IS NULL
        )
        OR (
            status = 'cancelled'
            AND paid_from_wallet_krw = 0
            AND obligated_amount_krw = 0
            AND ledger_transaction_id IS NULL
            AND tax_obligation_id IS NULL
            AND paid_game_day IS NULL
            AND cancelled_game_day IS NOT NULL
            AND cancellation_reason = 'newRun'
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

ALTER TABLE loan_payment
    DROP CHECK ck_loan_payment_kind,
    ADD COLUMN property_sale_execution_id BIGINT UNSIGNED NULL AFTER command_id,
    ADD UNIQUE KEY uk_loan_payment_property_sale_execution
        (save_id, run_revision, property_sale_execution_id),
    ADD CONSTRAINT fk_loan_payment_property_sale_execution
        FOREIGN KEY (save_id, run_revision, property_sale_execution_id)
        REFERENCES property_sale_execution (save_id, run_revision, id),
    ADD CONSTRAINT ck_loan_payment_kind CHECK (
        payment_kind IN (
            'scheduledInstallment', 'manualPrepayment',
            'leaseMovePayoff', 'propertySalePayoff'
        )
        AND (
            (
                payment_kind = 'scheduledInstallment'
                AND command_id IS NULL
                AND property_sale_execution_id IS NULL
            )
            OR (
                payment_kind IN ('manualPrepayment', 'leaseMovePayoff')
                AND command_id REGEXP
                    '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                AND property_sale_execution_id IS NULL
            )
            OR (
                payment_kind = 'propertySalePayoff'
                AND command_id IS NULL
                AND property_sale_execution_id IS NOT NULL
            )
        )
    );

CREATE TRIGGER tr_property_sale_order_valid_insert
BEFORE INSERT ON property_sale_order
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'active'
        AND NEW.current_revision_no = 1
        AND NEW.terminal_game_day IS NULL
        AND NEW.terminal_reason IS NULL
        AND EXISTS (
            SELECT 1
            FROM save
            INNER JOIN market_world AS world ON world.id = save.market_world_id
            INNER JOIN household
                ON household.save_id = save.id
               AND household.run_revision = save.run_revision
               AND household.id = NEW.household_id
            INNER JOIN run_rule_bundle AS bundle
                ON bundle.save_id = save.id
               AND bundle.run_revision = save.run_revision
            INNER JOIN policy_set AS policy
                ON policy.id = bundle.policy_set_id
               AND policy.policy_key
                    = 'dev-unranked-kr-individual-property-2026-v3'
               AND policy.sealed_at IS NOT NULL
            INNER JOIN real_estate_model_version AS model
                ON model.id = bundle.real_estate_model_version_id
               AND model.version_key
                    = 'dev-unranked-m4-real-estate-sale-tax-2026-v6'
               AND model.availability = 'active'
               AND model.sealed_at IS NOT NULL
            INNER JOIN real_estate_model_strict_manifest AS manifest
                ON manifest.real_estate_model_version_id = model.id
            INNER JOIN real_estate_model_strict_projection AS projection
                ON projection.real_estate_model_version_id = model.id
            INNER JOIN real_estate_sale_liquidity_profile AS liquidity
                ON liquidity.real_estate_model_version_id = model.id
            INNER JOIN property_holding AS holding
                ON holding.id = NEW.property_holding_id
               AND holding.save_id = save.id
               AND holding.run_revision = save.run_revision
               AND holding.household_id = household.id
               AND holding.real_estate_model_version_id = model.id
            INNER JOIN residence
                ON residence.save_id = holding.save_id
               AND residence.run_revision = holding.run_revision
               AND residence.household_id = holding.household_id
               AND residence.property_holding_id = holding.id
               AND residence.tenure_type = 'owner'
               AND residence.effective_to_game_day IS NULL
            WHERE save.id = NEW.save_id
              AND save.run_revision = NEW.run_revision
              AND save.game_day = NEW.created_game_day
              AND holding.status = 'active'
              AND holding.purpose = 'ownerOccupied'
              AND holding.exclusive_area_square_meters <= 85
              AND BINARY model.canonical_sha256 = BINARY manifest.canonical_sha256
              AND BINARY manifest.canonical_json = BINARY projection.canonical_json
              AND DATE_ADD(
                      DATE_ADD(world.start_date, INTERVAL holding.acquired_game_day DAY),
                      INTERVAL liquidity.minimum_holding_years YEAR
                  ) <= DATE_ADD(world.start_date, INTERVAL save.game_day DAY)
              AND DATE_ADD(
                      DATE_ADD(
                          world.start_date,
                          INTERVAL residence.effective_from_game_day DAY
                      ),
                      INTERVAL liquidity.minimum_residence_years YEAR
                  ) <= DATE_ADD(world.start_date, INTERVAL save.game_day DAY)
              AND NOT EXISTS (
                  SELECT 1 FROM property_sale_order AS active_order
                  WHERE active_order.save_id = save.id
                    AND active_order.run_revision = save.run_revision
                    AND active_order.property_holding_id = holding.id
                    AND active_order.status = 'active'
              )
        ),
    NEW.save_id,
    NULL
);

-- Sale execution and deferred property-tax settlement are both ledger-authoritative. Keep the
-- existing C3 account surface intact and add only the two C4 expenses plus their typed links.
ALTER TABLE ledger_transaction
    DROP CHECK ck_ledger_transaction_property_source,
    ADD CONSTRAINT ck_ledger_transaction_property_source CHECK (
        source_kind NOT LIKE 'property%'
        OR source_kind IN (
            'propertyPurchase', 'propertySale', 'propertyTaxPayment'
        )
    );

ALTER TABLE ledger_posting
    DROP CHECK ck_ledger_posting_account_code,
    DROP CHECK ck_ledger_posting_property_reference,
    ADD COLUMN property_tax_event_id BIGINT UNSIGNED NULL
        AFTER property_holding_id,
    ADD KEY ix_ledger_posting_property_tax_event
        (save_id, run_revision, property_tax_event_id),
    ADD CONSTRAINT fk_ledger_posting_property_tax_event
        FOREIGN KEY (save_id, run_revision, property_tax_event_id)
        REFERENCES property_tax_event (save_id, run_revision, id),
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
            'propertyAsset', 'acquisitionIncidentalExpense',
            'propertyDispositionExpense', 'propertyTaxExpense'
        )
    ),
    ADD CONSTRAINT ck_ledger_posting_property_reference CHECK (
        (
            account_code IN (
                'propertyAsset', 'acquisitionIncidentalExpense',
                'propertyDispositionExpense'
            )
            AND property_holding_id IS NOT NULL
        )
        OR account_code = 'realizedGainLoss'
        OR (
            account_code NOT IN (
                'propertyAsset', 'acquisitionIncidentalExpense',
                'propertyDispositionExpense', 'realizedGainLoss'
            )
            AND property_holding_id IS NULL
        )
    ),
    ADD CONSTRAINT ck_ledger_posting_property_tax_reference CHECK (
        (
            account_code = 'propertyTaxExpense'
            AND property_tax_event_id IS NOT NULL
        )
        OR (
            account_code <> 'propertyTaxExpense'
            AND property_tax_event_id IS NULL
        )
    );

DROP TRIGGER tr_ledger_transaction_property_source_insert;

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
    OR (
        NEW.source_kind = 'propertySale'
        AND NEW.source_id REGEXP '^[1-9][0-9]{0,19}$'
        AND EXISTS (
            SELECT 1
            FROM property_sale_execution AS execution
            INNER JOIN property_sale_order_revision AS revision
                ON revision.id = execution.property_sale_order_revision_id
               AND revision.save_id = execution.save_id
               AND revision.run_revision = execution.run_revision
            WHERE execution.id = CAST(NEW.source_id AS UNSIGNED)
              AND BINARY NEW.source_id = BINARY CAST(execution.id AS CHAR)
              AND execution.save_id = NEW.save_id
              AND execution.run_revision = NEW.run_revision
              AND execution.execution_game_day = NEW.game_day
              AND execution.status = 'prepared'
              AND revision.policy_set_id = NEW.policy_set_id
        )
    )
    OR (
        NEW.source_kind = 'propertyTaxPayment'
        AND NEW.source_id REGEXP '^[1-9][0-9]{0,19}$'
        AND EXISTS (
            SELECT 1
            FROM property_tax_payment AS payment
            INNER JOIN property_tax_event AS event
                ON event.id = payment.property_tax_event_id
               AND event.save_id = payment.save_id
               AND event.run_revision = payment.run_revision
            WHERE payment.id = CAST(NEW.source_id AS UNSIGNED)
              AND BINARY NEW.source_id = BINARY CAST(payment.id AS CHAR)
              AND payment.save_id = NEW.save_id
              AND payment.run_revision = NEW.run_revision
              AND payment.due_game_day = NEW.game_day
              AND payment.status = 'pending'
              AND event.status IN ('scheduled', 'partiallyPaid')
              AND event.policy_set_id = NEW.policy_set_id
        )
    )
    OR NEW.source_kind NOT LIKE 'property%',
    NEW.source_kind,
    NULL
);

CREATE TRIGGER tr_property_sale_revision_valid_insert
BEFORE INSERT ON property_sale_order_revision
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM property_sale_order AS sale_order
        INNER JOIN property_holding AS holding
            ON holding.id = sale_order.property_holding_id
           AND holding.save_id = sale_order.save_id
           AND holding.run_revision = sale_order.run_revision
        INNER JOIN save
            ON save.id = sale_order.save_id
           AND save.run_revision = sale_order.run_revision
        INNER JOIN run_rule_bundle AS bundle
            ON bundle.save_id = save.id
           AND bundle.run_revision = save.run_revision
        INNER JOIN real_estate_model_version AS model
            ON model.id = bundle.real_estate_model_version_id
           AND model.id = NEW.real_estate_model_version_id
           AND model.version_key
                = 'dev-unranked-m4-real-estate-sale-tax-2026-v6'
           AND model.sealed_at IS NOT NULL
        INNER JOIN policy_set AS policy
            ON policy.id = bundle.policy_set_id
           AND policy.id = NEW.policy_set_id
           AND policy.policy_key
                = 'dev-unranked-kr-individual-property-2026-v3'
           AND policy.sealed_at IS NOT NULL
        INNER JOIN policy_rule AS capital_rule
            ON capital_rule.id = NEW.capital_gains_policy_rule_id
           AND capital_rule.policy_set_id = policy.id
           AND capital_rule.domain = 'propertyTax'
           AND capital_rule.rule_key = 'singleHomeCapitalGainsTax'
        INNER JOIN real_estate_sale_liquidity_profile AS liquidity
            ON liquidity.real_estate_model_version_id = model.id
        WHERE sale_order.id = NEW.property_sale_order_id
          AND sale_order.save_id = NEW.save_id
          AND sale_order.run_revision = NEW.run_revision
          AND sale_order.property_holding_id = NEW.property_holding_id
          AND sale_order.status = 'active'
          AND holding.status = 'active'
          AND save.game_day = NEW.created_game_day
          AND (
              (
                  NEW.revision_no = 1
                  AND sale_order.current_revision_no = 1
                  AND NOT EXISTS (
                      SELECT 1 FROM property_sale_order_revision AS prior
                      WHERE prior.property_sale_order_id = sale_order.id
                  )
              )
              OR NEW.revision_no = sale_order.current_revision_no + 1
          )
          AND (
              (
                  NEW.revision_kind = 'listing'
                  AND EXISTS (
                      SELECT 1
                      FROM command_identity AS identity
                      WHERE identity.save_id = save.id
                        AND BINARY identity.command_id = BINARY NEW.command_id
                        AND identity.command_kind = IF(
                            NEW.revision_no = 1,
                            'createPropertySaleOrder',
                            'repricePropertySaleOrder'
                        )
                        AND identity.initial_run_revision = save.run_revision
                        AND identity.initial_state_revision = save.state_revision
                        AND identity.initial_game_day = save.game_day
                  )
                  AND NEW.acquisition_price_index_ppm
                        = holding.acquisition_price_index_ppm
                  AND EXISTS (
                      SELECT 1
                      FROM real_estate_daily AS daily
                      WHERE daily.market_world_id = save.market_world_id
                        AND daily.real_estate_model_version_id = model.id
                        AND BINARY daily.region_key = BINARY holding.region_key
                        AND daily.game_day = save.game_day
                        AND daily.price_index_ppm = NEW.current_price_index_ppm
                  )
                  AND NEW.reference_value_krw = FLOOR(
                      CAST(holding.acquisition_price_krw AS DECIMAL(65, 0))
                      * NEW.current_price_index_ppm
                      / holding.acquisition_price_index_ppm
                  )
                  AND CAST(NEW.asking_price_krw AS DECIMAL(65, 0)) * 1000000
                        >= CAST(NEW.reference_value_krw AS DECIMAL(65, 0))
                           * liquidity.minimum_asking_ratio_ppm
                  AND CAST(NEW.asking_price_krw AS DECIMAL(65, 0)) * 1000000
                        <= CAST(NEW.reference_value_krw AS DECIMAL(65, 0))
                           * liquidity.maximum_asking_ratio_ppm
                  AND NEW.asking_ratio_ppm = FLOOR(
                      CAST(NEW.asking_price_krw AS DECIMAL(65, 0)) * 1000000
                      / NEW.reference_value_krw
                  )
                  AND NEW.gross_price_rule = liquidity.gross_price_rule
                  AND NEW.disposition_cost_ppm = liquidity.disposition_cost_ppm
                  AND NEW.minimum_disposition_cost_krw
                        = liquidity.minimum_disposition_cost_krw
                  AND NEW.deficient_sale_proceeds
                        = liquidity.deficient_sale_proceeds
                  AND NEW.minimum_holding_years = liquidity.minimum_holding_years
                  AND NEW.minimum_residence_years
                        = liquidity.minimum_residence_years
                  AND (
                      (
                          NEW.asking_ratio_ppm
                              <= liquidity.low_band_maximum_ratio_ppm
                          AND NEW.candidate_game_day - NEW.created_game_day
                              BETWEEN liquidity.low_delay_minimum_days
                                  AND liquidity.low_delay_maximum_days
                      )
                      OR (
                          NEW.asking_ratio_ppm
                              > liquidity.low_band_maximum_ratio_ppm
                          AND NEW.asking_ratio_ppm
                              <= liquidity.middle_band_maximum_ratio_ppm
                          AND NEW.candidate_game_day - NEW.created_game_day
                              BETWEEN liquidity.middle_delay_minimum_days
                                  AND liquidity.middle_delay_maximum_days
                      )
                      OR (
                          NEW.asking_ratio_ppm
                              > liquidity.middle_band_maximum_ratio_ppm
                          AND NEW.candidate_game_day - NEW.created_game_day
                              BETWEEN liquidity.high_delay_minimum_days
                                  AND liquidity.high_delay_maximum_days
                      )
                  )
              )
              OR (
                  NEW.revision_kind = 'cancellation'
                  AND (
                      (
                          NEW.cancellation_reason = 'userRequest'
                          AND EXISTS (
                              SELECT 1
                              FROM command_identity AS identity
                              WHERE identity.save_id = save.id
                                AND BINARY identity.command_id
                                      = BINARY NEW.command_id
                                AND identity.command_kind
                                      = 'cancelPropertySaleOrder'
                                AND identity.initial_run_revision = save.run_revision
                                AND identity.initial_state_revision = save.state_revision
                                AND identity.initial_game_day = save.game_day
                          )
                      )
                      OR (
                          NEW.cancellation_reason = 'newRun'
                          AND NEW.command_id IS NULL
                      )
                  )
              )
          )
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_property_sale_revision_no_update
BEFORE UPDATE ON property_sale_order_revision
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property sale revisions are immutable';

CREATE TRIGGER tr_property_sale_revision_no_delete
BEFORE DELETE ON property_sale_order_revision
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property sale revisions are immutable';

CREATE TRIGGER tr_property_sale_execution_valid_insert
BEFORE INSERT ON property_sale_execution
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM property_sale_order AS sale_order
        INNER JOIN property_sale_order_revision AS revision
            ON revision.id = NEW.property_sale_order_revision_id
           AND revision.save_id = sale_order.save_id
           AND revision.run_revision = sale_order.run_revision
           AND revision.property_sale_order_id = sale_order.id
           AND revision.revision_no = sale_order.current_revision_no
        INNER JOIN property_holding AS holding
            ON holding.id = sale_order.property_holding_id
           AND holding.save_id = sale_order.save_id
           AND holding.run_revision = sale_order.run_revision
        INNER JOIN save
            ON save.id = sale_order.save_id
           AND save.run_revision = sale_order.run_revision
        INNER JOIN residence
            ON residence.save_id = holding.save_id
           AND residence.run_revision = holding.run_revision
           AND residence.household_id = holding.household_id
           AND residence.property_holding_id = holding.id
           AND residence.tenure_type = 'owner'
           AND residence.effective_to_game_day IS NULL
        WHERE sale_order.id = NEW.property_sale_order_id
          AND sale_order.save_id = NEW.save_id
          AND sale_order.run_revision = NEW.run_revision
          AND sale_order.property_holding_id = NEW.property_holding_id
          AND sale_order.status = 'active'
          AND revision.revision_kind = 'listing'
          AND revision.candidate_game_day = NEW.execution_game_day
          AND NEW.execution_game_day = save.game_day + 1
          AND holding.status = 'active'
          AND holding.book_value_krw = NEW.book_value_krw
          AND revision.asking_price_krw = NEW.gross_sale_price_krw
          AND NEW.disposition_cost_krw = GREATEST(
              FLOOR(
                  CAST(NEW.gross_sale_price_krw AS DECIMAL(65, 0))
                  * revision.disposition_cost_ppm / 1000000
              ),
              revision.minimum_disposition_cost_krw
          )
          AND (
              NEW.status = 'rejected'
              OR (
                  NEW.status = 'prepared'
                  AND (
                      (
                          NEW.mortgage_principal_krw = 0
                          AND NEW.mortgage_prepayment_fee_krw = 0
                          AND NOT EXISTS (
                              SELECT 1
                              FROM property_lien AS lien
                              WHERE lien.save_id = holding.save_id
                                AND lien.run_revision = holding.run_revision
                                AND lien.property_holding_id = holding.id
                                AND lien.status = 'active'
                          )
                      )
                      OR EXISTS (
                          SELECT 1
                          FROM property_lien AS lien
                          INNER JOIN loan_contract AS mortgage
                              ON mortgage.id = lien.loan_contract_id
                             AND mortgage.save_id = lien.save_id
                             AND mortgage.run_revision = lien.run_revision
                             AND mortgage.property_holding_id
                                  = lien.property_holding_id
                          WHERE lien.save_id = holding.save_id
                            AND lien.run_revision = holding.run_revision
                            AND lien.property_holding_id = holding.id
                            AND lien.status = 'active'
                            AND mortgage.product_kind = 'mortgage'
                            AND mortgage.status = 'active'
                            AND mortgage.accrued_interest_krw = 0
                            AND mortgage.accrued_fee_krw = 0
                            AND mortgage.remaining_principal_krw
                                  = NEW.mortgage_principal_krw
                            AND NEW.mortgage_prepayment_fee_krw = FLOOR(
                                CAST(mortgage.remaining_principal_krw AS DECIMAL(65, 0))
                                * mortgage.prepayment_fee_ppm / 1000000
                            )
                            AND NOT EXISTS (
                                SELECT 1
                                FROM loan_obligation_bucket AS bucket
                                WHERE bucket.loan_contract_id = mortgage.id
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

CREATE TRIGGER tr_property_sale_execution_transition_only
BEFORE UPDATE ON property_sale_execution
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'prepared'
        AND NEW.status = 'applied'
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.property_sale_order_id = OLD.property_sale_order_id
        AND NEW.property_sale_order_revision_id
              = OLD.property_sale_order_revision_id
        AND NEW.property_holding_id = OLD.property_holding_id
        AND NEW.execution_game_day = OLD.execution_game_day
        AND NEW.rejection_reason <=> OLD.rejection_reason
        AND NEW.book_value_krw = OLD.book_value_krw
        AND NEW.gross_sale_price_krw = OLD.gross_sale_price_krw
        AND NEW.disposition_cost_krw = OLD.disposition_cost_krw
        AND NEW.mortgage_principal_krw = OLD.mortgage_principal_krw
        AND NEW.mortgage_prepayment_fee_krw
              = OLD.mortgage_prepayment_fee_krw
        AND NEW.transfer_tax_krw = OLD.transfer_tax_krw
        AND NEW.net_wallet_proceeds_krw = OLD.net_wallet_proceeds_krw
        AND NEW.created_at = OLD.created_at
        AND EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            INNER JOIN property_holding AS holding
                ON holding.id = OLD.property_holding_id
               AND holding.save_id = OLD.save_id
               AND holding.run_revision = OLD.run_revision
            INNER JOIN residence
                ON residence.id = NEW.replacement_residence_id
               AND residence.save_id = OLD.save_id
               AND residence.run_revision = OLD.run_revision
               AND residence.household_id = holding.household_id
            INNER JOIN property_tax_event AS tax_event
                ON tax_event.save_id = OLD.save_id
               AND tax_event.run_revision = OLD.run_revision
               AND tax_event.property_sale_execution_id = OLD.id
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = OLD.save_id
              AND ledger.run_revision = OLD.run_revision
              AND ledger.game_day = OLD.execution_game_day
              AND ledger.source_kind = 'propertySale'
              AND BINARY ledger.source_id = BINARY CAST(OLD.id AS CHAR)
              AND holding.status = 'disposed'
              AND holding.disposed_game_day = OLD.execution_game_day
              AND residence.tenure_type = 'rentFree'
              AND BINARY residence.region_key = BINARY holding.region_key
              AND residence.effective_from_game_day = OLD.execution_game_day
              AND residence.effective_to_game_day IS NULL
              AND tax_event.event_kind = 'capitalGains'
              AND tax_event.status IN ('paid', 'noPaymentRequired')
              AND tax_event.total_tax_krw = OLD.transfer_tax_krw
              AND COALESCE((
                  SELECT SUM(posting.amount_krw)
                  FROM ledger_posting AS posting
                  WHERE posting.ledger_transaction_id = ledger.id
                    AND posting.account_code = 'propertyAsset'
                    AND posting.property_holding_id = holding.id
              ), 0) = -OLD.book_value_krw
              AND COALESCE((
                  SELECT SUM(posting.amount_krw)
                  FROM ledger_posting AS posting
                  WHERE posting.ledger_transaction_id = ledger.id
                    AND posting.account_code = 'realizedGainLoss'
                    AND posting.property_holding_id = holding.id
              ), 0) = OLD.book_value_krw - OLD.gross_sale_price_krw
              AND COALESCE((
                  SELECT SUM(posting.amount_krw)
                  FROM ledger_posting AS posting
                  WHERE posting.ledger_transaction_id = ledger.id
                    AND posting.account_code = 'propertyDispositionExpense'
                    AND posting.property_holding_id = holding.id
              ), 0) = OLD.disposition_cost_krw
              AND COALESCE((
                  SELECT SUM(posting.amount_krw)
                  FROM ledger_posting AS posting
                  WHERE posting.ledger_transaction_id = ledger.id
                    AND posting.account_code = 'loanPrincipalLiability'
              ), 0) = OLD.mortgage_principal_krw
              AND COALESCE((
                  SELECT SUM(posting.amount_krw)
                  FROM ledger_posting AS posting
                  WHERE posting.ledger_transaction_id = ledger.id
                    AND posting.account_code = 'loanFeeExpense'
              ), 0) = OLD.mortgage_prepayment_fee_krw
              AND COALESCE((
                  SELECT SUM(posting.amount_krw)
                  FROM ledger_posting AS posting
                  WHERE posting.ledger_transaction_id = ledger.id
                    AND posting.account_code = 'propertyTaxExpense'
                    AND posting.property_tax_event_id = tax_event.id
              ), 0) = OLD.transfer_tax_krw
              AND (
                  SELECT COUNT(*)
                  FROM ledger_posting AS posting
                  WHERE posting.ledger_transaction_id = ledger.id
                    AND posting.account_code = 'propertyTaxExpense'
                    AND posting.property_tax_event_id = tax_event.id
              ) = (
                  SELECT COUNT(*)
                  FROM property_tax_component AS component
                  WHERE component.property_tax_event_id = tax_event.id
                    AND component.tax_amount_krw > 0
              )
              AND COALESCE((
                  SELECT SUM(posting.amount_krw)
                  FROM ledger_posting AS posting
                  WHERE posting.ledger_transaction_id = ledger.id
                    AND posting.account_code = 'wallet'
              ), 0) = OLD.net_wallet_proceeds_krw
              AND (
                  (
                      OLD.mortgage_principal_krw = 0
                      AND OLD.mortgage_prepayment_fee_krw = 0
                      AND NOT EXISTS (
                          SELECT 1 FROM loan_payment AS payoff
                          WHERE payoff.property_sale_execution_id = OLD.id
                      )
                  )
                  OR EXISTS (
                      SELECT 1
                      FROM loan_payment AS payoff
                      INNER JOIN loan_contract AS mortgage
                          ON mortgage.id = payoff.loan_contract_id
                         AND mortgage.save_id = payoff.save_id
                         AND mortgage.run_revision = payoff.run_revision
                      INNER JOIN property_lien AS lien
                          ON lien.loan_contract_id = mortgage.id
                         AND lien.save_id = mortgage.save_id
                         AND lien.run_revision = mortgage.run_revision
                      WHERE payoff.save_id = OLD.save_id
                        AND payoff.run_revision = OLD.run_revision
                        AND payoff.property_sale_execution_id = OLD.id
                        AND payoff.payment_kind = 'propertySalePayoff'
                        AND payoff.status = 'applied'
                        AND payoff.amount_krw
                              = OLD.mortgage_principal_krw
                                + OLD.mortgage_prepayment_fee_krw
                        AND mortgage.status = 'paidOff'
                        AND mortgage.remaining_principal_krw = 0
                        AND lien.status = 'released'
                        AND lien.released_game_day = OLD.execution_game_day
                  )
              )
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_property_sale_execution_no_delete
BEFORE DELETE ON property_sale_execution
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property sale executions are immutable';

CREATE TRIGGER tr_property_sale_order_transition_only
BEFORE UPDATE ON property_sale_order
FOR EACH ROW
SET NEW.id = IF(
    NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.household_id = OLD.household_id
        AND NEW.property_holding_id = OLD.property_holding_id
        AND NEW.created_game_day = OLD.created_game_day
        AND NEW.created_at = OLD.created_at
        AND OLD.status = 'active'
        AND (
            (
                NEW.status = 'active'
                AND NEW.current_revision_no = OLD.current_revision_no + 1
                AND NEW.terminal_game_day IS NULL
                AND NEW.terminal_reason IS NULL
                AND EXISTS (
                    SELECT 1 FROM property_sale_order_revision AS revision
                    WHERE revision.save_id = OLD.save_id
                      AND revision.run_revision = OLD.run_revision
                      AND revision.property_sale_order_id = OLD.id
                      AND revision.revision_no = NEW.current_revision_no
                      AND revision.revision_kind = 'listing'
                )
            )
            OR (
                NEW.status = 'cancelled'
                AND NEW.current_revision_no = OLD.current_revision_no + 1
                AND EXISTS (
                    SELECT 1 FROM property_sale_order_revision AS revision
                    WHERE revision.save_id = OLD.save_id
                      AND revision.run_revision = OLD.run_revision
                      AND revision.property_sale_order_id = OLD.id
                      AND revision.revision_no = NEW.current_revision_no
                      AND revision.revision_kind = 'cancellation'
                      AND BINARY revision.cancellation_reason
                            = BINARY NEW.terminal_reason
                      AND revision.created_game_day = NEW.terminal_game_day
                )
            )
            OR (
                NEW.status = 'filled'
                AND NEW.current_revision_no = OLD.current_revision_no
                AND EXISTS (
                    SELECT 1 FROM property_sale_execution AS execution
                    WHERE execution.save_id = OLD.save_id
                      AND execution.run_revision = OLD.run_revision
                      AND execution.property_sale_order_id = OLD.id
                      AND execution.status = 'applied'
                      AND execution.execution_game_day = NEW.terminal_game_day
                )
            )
            OR (
                NEW.status = 'rejected'
                AND NEW.current_revision_no = OLD.current_revision_no
                AND EXISTS (
                    SELECT 1 FROM property_sale_execution AS execution
                    WHERE execution.save_id = OLD.save_id
                      AND execution.run_revision = OLD.run_revision
                      AND execution.property_sale_order_id = OLD.id
                      AND execution.status = 'rejected'
                      AND BINARY execution.rejection_reason
                            = BINARY NEW.terminal_reason
                      AND execution.execution_game_day = NEW.terminal_game_day
                )
            )
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_property_sale_order_no_delete
BEFORE DELETE ON property_sale_order
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property sale order history is immutable';

CREATE TRIGGER tr_property_tax_event_valid_insert
BEFORE INSERT ON property_tax_event
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'prepared'
        AND NEW.paid_tax_krw = 0
        AND EXISTS (
            SELECT 1
            FROM save
            INNER JOIN market_world AS world ON world.id = save.market_world_id
            INNER JOIN household
                ON household.save_id = save.id
               AND household.run_revision = save.run_revision
               AND household.id = NEW.household_id
            INNER JOIN run_rule_bundle AS bundle
                ON bundle.save_id = save.id
               AND bundle.run_revision = save.run_revision
            INNER JOIN policy_set AS policy
                ON policy.id = bundle.policy_set_id
               AND policy.id = NEW.policy_set_id
               AND policy.policy_key
                    = 'dev-unranked-kr-individual-property-2026-v3'
               AND policy.sealed_at IS NOT NULL
            INNER JOIN policy_rule AS rule
                ON rule.id = NEW.policy_rule_id
               AND rule.policy_set_id = policy.id
               AND rule.domain = 'propertyTax'
            INNER JOIN property_holding AS holding
                ON holding.id = NEW.property_holding_id
               AND holding.save_id = save.id
               AND holding.run_revision = save.run_revision
               AND holding.household_id = household.id
            INNER JOIN real_estate_model_version AS model
                ON model.id = holding.real_estate_model_version_id
               AND model.version_key
                    = 'dev-unranked-m4-real-estate-sale-tax-2026-v6'
               AND model.sealed_at IS NOT NULL
            WHERE save.id = NEW.save_id
              AND save.run_revision = NEW.run_revision
              AND NEW.legal_basis_date = rule.effective_from
              AND NEW.household_home_count = 1
              AND NEW.tax_year = YEAR(
                  DATE_ADD(world.start_date, INTERVAL NEW.taxable_game_day DAY)
              )
              AND (
                  (
                      NEW.event_kind = 'acquisition'
                      AND rule.rule_key = 'singleHomeAcquisitionTax'
                      AND EXISTS (
                          SELECT 1
                          FROM property_acquisition_tax_policy_profile AS profile
                          WHERE profile.policy_set_id = policy.id
                            AND profile.rule_id = rule.id
                            AND profile.supported_home_count
                                  = NEW.household_home_count
                      )
                      AND holding.status = 'active'
                      AND holding.acquired_game_day = save.game_day
                      AND NEW.assessment_game_day = holding.acquired_game_day
                      AND NEW.taxable_game_day = holding.acquired_game_day
                      AND NEW.valuation_game_day = holding.acquired_game_day
                      AND NEW.valuation_price_index_ppm
                            = holding.acquisition_price_index_ppm
                      AND NEW.valuation_amount_krw = holding.acquisition_price_krw
                      AND NEW.tax_base_krw = holding.acquisition_price_krw
                      AND NEW.deduction_krw = 0
                      AND JSON_LENGTH(NEW.exclusion_codes) = 0
                  )
                  OR (
                      NEW.event_kind = 'annualProperty'
                      AND rule.rule_key = 'singleHomeAnnualPropertyTax'
                      AND EXISTS (
                          SELECT 1
                          FROM property_annual_tax_policy_profile AS profile
                          WHERE profile.policy_set_id = policy.id
                            AND profile.rule_id = rule.id
                            AND profile.supported_home_count
                                  = NEW.household_home_count
                            AND MONTH(
                                DATE_ADD(
                                    world.start_date,
                                    INTERVAL NEW.assessment_game_day DAY
                                )
                            ) = profile.assessment_month
                            AND DAYOFMONTH(
                                DATE_ADD(
                                    world.start_date,
                                    INTERVAL NEW.assessment_game_day DAY
                                )
                            ) = profile.assessment_day
                            AND NEW.official_value_krw = FLOOR(
                                CAST(NEW.valuation_amount_krw AS DECIMAL(65, 0))
                                * profile.official_value_ratio_ppm / 1000000
                            )
                            AND NEW.tax_base_krw = FLOOR(
                                CAST(NEW.official_value_krw AS DECIMAL(65, 0))
                                * (
                                    SELECT band.fair_market_value_ratio_ppm
                                    FROM property_annual_tax_fair_market_ratio_band AS band
                                    WHERE band.policy_set_id = policy.id
                                      AND (
                                          band.official_value_upper_bound_krw IS NULL
                                          OR NEW.official_value_krw
                                               <= band.official_value_upper_bound_krw
                                      )
                                    ORDER BY band.band_order
                                    LIMIT 1
                                ) / 1000000
                            )
                            AND NEW.exclusion_codes
                                  = profile.unsupported_exclusion_codes
                      )
                      AND holding.status = 'active'
                      AND NEW.assessment_game_day = save.game_day + 1
                      AND NEW.taxable_game_day = NEW.assessment_game_day
                      AND NEW.valuation_game_day = NEW.assessment_game_day
                      AND NEW.deduction_krw = 0
                      AND EXISTS (
                          SELECT 1
                          FROM real_estate_daily AS daily
                          WHERE daily.market_world_id = save.market_world_id
                            AND daily.real_estate_model_version_id = model.id
                            AND BINARY daily.region_key = BINARY holding.region_key
                            AND daily.game_day = NEW.valuation_game_day
                            AND daily.price_index_ppm
                                  = NEW.valuation_price_index_ppm
                            AND NEW.valuation_amount_krw = FLOOR(
                                CAST(holding.acquisition_price_krw AS DECIMAL(65, 0))
                                * daily.price_index_ppm
                                / holding.acquisition_price_index_ppm
                            )
                      )
                  )
                  OR (
                      NEW.event_kind = 'capitalGains'
                      AND rule.rule_key = 'singleHomeCapitalGainsTax'
                      AND EXISTS (
                          SELECT 1
                          FROM property_capital_gains_tax_policy_profile AS profile
                          INNER JOIN property_sale_execution AS execution
                              ON execution.id = NEW.property_sale_execution_id
                             AND execution.save_id = save.id
                             AND execution.run_revision = save.run_revision
                             AND execution.property_holding_id = holding.id
                          WHERE profile.policy_set_id = policy.id
                            AND profile.rule_id = rule.id
                            AND profile.supported_home_count
                                  = NEW.household_home_count
                            AND execution.status = 'prepared'
                            AND execution.execution_game_day
                                  = NEW.assessment_game_day
                            AND execution.execution_game_day
                                  = NEW.taxable_game_day
                            AND execution.gross_sale_price_krw
                                  = NEW.valuation_amount_krw
                            AND execution.disposition_cost_krw
                                  = NEW.disposition_cost_krw
                            AND execution.transfer_tax_krw = NEW.total_tax_krw
                            AND NEW.completed_holding_years
                                  >= profile.minimum_holding_years
                            AND NEW.completed_residence_years
                                  >= profile.minimum_residence_years
                      )
                      AND NEW.assessment_game_day = save.game_day + 1
                      AND NEW.valuation_game_day = NEW.assessment_game_day
                      AND NEW.acquisition_taxes_krw >= 0
                      AND NEW.gross_gain_krw = GREATEST(
                          NEW.valuation_amount_krw
                            - holding.acquisition_price_krw
                            - holding.acquisition_incidental_cost_krw
                            - NEW.acquisition_taxes_krw
                            - NEW.disposition_cost_krw,
                          0
                      )
                      AND NEW.high_value_gain_krw
                            BETWEEN 0 AND NEW.gross_gain_krw
                      AND NEW.long_term_deduction_krw
                            BETWEEN 0 AND NEW.high_value_gain_krw
                      AND EXISTS (
                          SELECT 1
                          FROM real_estate_daily AS daily
                          WHERE daily.market_world_id = save.market_world_id
                            AND daily.real_estate_model_version_id = model.id
                            AND BINARY daily.region_key = BINARY holding.region_key
                            AND daily.game_day = NEW.valuation_game_day
                            AND daily.price_index_ppm
                                  = NEW.valuation_price_index_ppm
                      )
                  )
              )
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_property_tax_component_valid_insert
BEFORE INSERT ON property_tax_component
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM property_tax_event AS event
        WHERE event.id = NEW.property_tax_event_id
          AND event.save_id = NEW.save_id
          AND event.run_revision = NEW.run_revision
          AND event.status = 'prepared'
          AND (
              (
                  event.event_kind = 'acquisition'
                  AND NEW.component_kind IN (
                      'acquisitionTax', 'acquisitionLocalEducationTax'
                  )
              )
              OR (
                  event.event_kind = 'annualProperty'
                  AND NEW.component_kind IN (
                      'annualPropertyTax', 'annualPropertyLocalEducationTax'
                  )
              )
              OR (
                  event.event_kind = 'capitalGains'
                  AND NEW.component_kind IN (
                      'capitalGainsTax', 'capitalGainsLocalIncomeTax'
                  )
              )
          )
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_property_tax_component_no_update
BEFORE UPDATE ON property_tax_component
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property tax components are immutable';

CREATE TRIGGER tr_property_tax_component_no_delete
BEFORE DELETE ON property_tax_component
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property tax components are immutable';

CREATE TRIGGER tr_property_tax_payment_valid_insert
BEFORE INSERT ON property_tax_payment
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'pending'
        AND NEW.scheduled_settlement_id IS NULL
        AND EXISTS (
            SELECT 1
            FROM property_tax_event AS event
            INNER JOIN save
                ON save.id = event.save_id
               AND save.run_revision = event.run_revision
            INNER JOIN market_world AS world ON world.id = save.market_world_id
            WHERE event.id = NEW.property_tax_event_id
              AND event.save_id = NEW.save_id
              AND event.run_revision = NEW.run_revision
              AND event.status = 'prepared'
              AND event.total_tax_krw > 0
              AND (
                  (
                      event.event_kind = 'acquisition'
                      AND NEW.payment_no = 1
                      AND NEW.amount_krw = event.total_tax_krw
                      AND EXISTS (
                          SELECT 1
                          FROM property_acquisition_tax_policy_profile AS profile
                          WHERE profile.policy_set_id = event.policy_set_id
                            AND NEW.due_game_day
                                  = event.taxable_game_day + profile.payment_due_days
                      )
                  )
                  OR (
                      event.event_kind = 'annualProperty'
                      AND NEW.payment_no IN (1, 2)
                      AND EXISTS (
                          SELECT 1
                          FROM property_annual_tax_policy_profile AS profile
                          WHERE profile.policy_set_id = event.policy_set_id
                            AND NEW.due_game_day = DATEDIFF(
                                STR_TO_DATE(
                                    CONCAT(
                                        event.tax_year, '-',
                                        IF(NEW.payment_no = 1,
                                           profile.first_payment_month,
                                           profile.second_payment_month), '-',
                                        IF(NEW.payment_no = 1,
                                           profile.first_payment_day,
                                           profile.second_payment_day)
                                    ),
                                    '%Y-%c-%e'
                                ),
                                world.start_date
                            )
                            AND NEW.amount_krw = IF(
                                NEW.payment_no = 1,
                                FLOOR(event.total_tax_krw / 2),
                                event.total_tax_krw - FLOOR(event.total_tax_krw / 2)
                            )
                      )
                  )
              )
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_property_tax_payment_transition_only
BEFORE UPDATE ON property_tax_payment
FOR EACH ROW
SET NEW.id = IF(
    NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.property_tax_event_id = OLD.property_tax_event_id
        AND NEW.payment_no = OLD.payment_no
        AND NEW.due_game_day = OLD.due_game_day
        AND NEW.amount_krw = OLD.amount_krw
        AND NEW.created_at = OLD.created_at
        AND OLD.status = 'pending'
        AND (
            (
                NEW.status = 'pending'
                AND OLD.scheduled_settlement_id IS NULL
                AND NEW.scheduled_settlement_id IS NOT NULL
                AND NEW.ledger_transaction_id IS NULL
                AND NEW.tax_obligation_id IS NULL
                AND EXISTS (
                    SELECT 1
                    FROM scheduled_settlement AS settlement
                    WHERE settlement.id = NEW.scheduled_settlement_id
                      AND settlement.save_id = OLD.save_id
                      AND settlement.run_revision = OLD.run_revision
                      AND settlement.kind = 'propertyTaxPayment'
                      AND settlement.source_kind = 'propertyTaxEvent'
                      AND BINARY settlement.source_id
                            = BINARY CAST(OLD.property_tax_event_id AS CHAR)
                      AND settlement.occurrence = OLD.payment_no
                      AND settlement.due_game_day = OLD.due_game_day
                      AND settlement.status = 'pending'
                )
            )
            OR (
                NEW.status = 'applied'
                AND NEW.scheduled_settlement_id = OLD.scheduled_settlement_id
                AND NEW.paid_game_day = OLD.due_game_day
                AND EXISTS (
                    SELECT 1
                    FROM ledger_transaction AS ledger
                    WHERE ledger.id = NEW.ledger_transaction_id
                      AND ledger.save_id = OLD.save_id
                      AND ledger.run_revision = OLD.run_revision
                      AND ledger.game_day = NEW.paid_game_day
                      AND ledger.source_kind = 'propertyTaxPayment'
                      AND BINARY ledger.source_id = BINARY CAST(OLD.id AS CHAR)
                      AND COALESCE((
                          SELECT SUM(posting.amount_krw)
                          FROM ledger_posting AS posting
                          WHERE posting.ledger_transaction_id = ledger.id
                            AND posting.account_code = 'propertyTaxExpense'
                            AND posting.property_tax_event_id
                                  = OLD.property_tax_event_id
                      ), 0) = OLD.amount_krw
                      AND COALESCE((
                          SELECT -SUM(posting.amount_krw)
                          FROM ledger_posting AS posting
                          WHERE posting.ledger_transaction_id = ledger.id
                            AND posting.account_code = 'wallet'
                      ), 0) = NEW.paid_from_wallet_krw
                      AND COALESCE((
                          SELECT -SUM(posting.amount_krw)
                          FROM ledger_posting AS posting
                          WHERE posting.ledger_transaction_id = ledger.id
                            AND posting.account_code = 'taxObligationLiability'
                            AND posting.tax_obligation_id = NEW.tax_obligation_id
                      ), 0) = NEW.obligated_amount_krw
                )
                AND (
                    (
                        NEW.obligated_amount_krw = 0
                        AND NEW.tax_obligation_id IS NULL
                    )
                    OR EXISTS (
                        SELECT 1
                        FROM tax_obligation AS obligation
                        WHERE obligation.id = NEW.tax_obligation_id
                          AND obligation.save_id = OLD.save_id
                          AND obligation.run_revision = OLD.run_revision
                          AND obligation.source_kind = 'propertyTaxEvent'
                          AND BINARY obligation.source_id
                                = BINARY CAST(OLD.property_tax_event_id AS CHAR)
                          AND obligation.source_occurrence = OLD.payment_no
                          AND obligation.original_amount_krw
                                = NEW.obligated_amount_krw
                          AND obligation.status = 'outstanding'
                    )
                )
            )
            OR (
                NEW.status = 'cancelled'
                AND NEW.scheduled_settlement_id = OLD.scheduled_settlement_id
                AND NEW.cancellation_reason = 'newRun'
                AND EXISTS (
                    SELECT 1
                    FROM scheduled_settlement AS settlement
                    WHERE settlement.id = OLD.scheduled_settlement_id
                      AND settlement.save_id = OLD.save_id
                      AND settlement.run_revision = OLD.run_revision
                      AND settlement.status = 'cancelled'
                      AND settlement.cancellation_reason = 'newRun'
                )
            )
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_property_tax_payment_no_delete
BEFORE DELETE ON property_tax_payment
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property tax payment history is immutable';

CREATE TRIGGER tr_property_tax_event_transition_only
BEFORE UPDATE ON property_tax_event
FOR EACH ROW
SET NEW.id = IF(
    NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.household_id = OLD.household_id
        AND NEW.property_holding_id = OLD.property_holding_id
        AND NEW.policy_set_id = OLD.policy_set_id
        AND NEW.policy_rule_id = OLD.policy_rule_id
        AND NEW.property_sale_execution_id <=> OLD.property_sale_execution_id
        AND BINARY NEW.event_kind = BINARY OLD.event_kind
        AND NEW.tax_year = OLD.tax_year
        AND NEW.legal_basis_date = OLD.legal_basis_date
        AND NEW.assessment_game_day = OLD.assessment_game_day
        AND NEW.taxable_game_day = OLD.taxable_game_day
        AND NEW.household_home_count = OLD.household_home_count
        AND NEW.valuation_game_day <=> OLD.valuation_game_day
        AND NEW.valuation_price_index_ppm <=> OLD.valuation_price_index_ppm
        AND NEW.valuation_amount_krw = OLD.valuation_amount_krw
        AND NEW.official_value_krw <=> OLD.official_value_krw
        AND NEW.tax_base_krw = OLD.tax_base_krw
        AND NEW.deduction_krw = OLD.deduction_krw
        AND NEW.acquisition_taxes_krw <=> OLD.acquisition_taxes_krw
        AND NEW.disposition_cost_krw <=> OLD.disposition_cost_krw
        AND NEW.gross_gain_krw <=> OLD.gross_gain_krw
        AND NEW.high_value_gain_krw <=> OLD.high_value_gain_krw
        AND NEW.long_term_deduction_krw <=> OLD.long_term_deduction_krw
        AND NEW.completed_holding_years <=> OLD.completed_holding_years
        AND NEW.completed_residence_years <=> OLD.completed_residence_years
        AND NEW.total_tax_krw = OLD.total_tax_krw
        AND NEW.exclusion_codes = OLD.exclusion_codes
        AND NEW.created_at = OLD.created_at
        AND (
            (
                OLD.status = 'prepared'
                AND NEW.status IN ('scheduled', 'paid', 'noPaymentRequired')
                AND NEW.paid_tax_krw = IF(
                    NEW.status = 'paid',
                    OLD.total_tax_krw,
                    0
                )
                AND (
                    SELECT COUNT(*)
                    FROM property_tax_component AS component
                    WHERE component.property_tax_event_id = OLD.id
                ) = 2
                AND OLD.total_tax_krw = (
                    SELECT COALESCE(SUM(component.tax_amount_krw), 0)
                    FROM property_tax_component AS component
                    WHERE component.property_tax_event_id = OLD.id
                )
                AND (
                    (
                        OLD.event_kind IN ('acquisition', 'annualProperty')
                        AND NEW.status = 'scheduled'
                        AND OLD.total_tax_krw > 0
                        AND (
                            SELECT COUNT(*)
                            FROM property_tax_payment AS payment
                            WHERE payment.property_tax_event_id = OLD.id
                              AND payment.status = 'pending'
                              AND payment.scheduled_settlement_id IS NOT NULL
                        ) = IF(OLD.event_kind = 'acquisition', 1, 2)
                        AND OLD.total_tax_krw = (
                            SELECT COALESCE(SUM(payment.amount_krw), 0)
                            FROM property_tax_payment AS payment
                            WHERE payment.property_tax_event_id = OLD.id
                        )
                    )
                    OR (
                        OLD.event_kind = 'capitalGains'
                        AND (
                            (OLD.total_tax_krw > 0 AND NEW.status = 'paid')
                            OR (
                                OLD.total_tax_krw = 0
                                AND NEW.status = 'noPaymentRequired'
                            )
                        )
                        AND NOT EXISTS (
                            SELECT 1 FROM property_tax_payment AS payment
                            WHERE payment.property_tax_event_id = OLD.id
                        )
                    )
                )
            )
            OR (
                OLD.status IN ('scheduled', 'partiallyPaid')
                AND NEW.status IN ('partiallyPaid', 'paid')
                AND NEW.paid_tax_krw = (
                    SELECT COALESCE(SUM(payment.amount_krw), 0)
                    FROM property_tax_payment AS payment
                    WHERE payment.property_tax_event_id = OLD.id
                      AND payment.status = 'applied'
                )
                AND (
                    (NEW.paid_tax_krw < OLD.total_tax_krw
                     AND NEW.status = 'partiallyPaid')
                    OR (NEW.paid_tax_krw = OLD.total_tax_krw
                        AND NEW.status = 'paid')
                )
            )
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_property_tax_event_no_delete
BEFORE DELETE ON property_tax_event
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'property tax event history is immutable';

ALTER TABLE scheduled_settlement
    DROP CHECK ck_scheduled_settlement_kind,
    DROP CHECK ck_scheduled_settlement_source_kind,
    ADD CONSTRAINT ck_scheduled_settlement_kind CHECK (
        kind IN (
            'cmaInterest', 'depositMaturity', 'savingsInstallment',
            'savingsMaturity', 'bondCoupon', 'bondMaturity',
            'llxDistribution', 'financialIncomeFiling',
            'employmentPayroll', 'employmentReconciliation',
            'militaryPay', 'militarySavingsInstallment',
            'militarySavingsMaturity', 'militarySavingsGovernmentMatch',
            'livingCostMonth', 'loanInstallment', 'leaseRent',
            'propertyTaxPayment'
        )
    ),
    ADD CONSTRAINT ck_scheduled_settlement_source_kind CHECK (
        source_kind IN (
            'cmaAccount', 'depositContract', 'savingsContract',
            'bondPosition', 'indexPosition', 'taxYear',
            'employmentContract', 'yearEndTaxAssessment',
            'militaryService', 'militarySavingsContract',
            'militarySavingsInstallment', 'livingCostMonth',
            'loanContract', 'leaseContract', 'propertyTaxEvent'
        )
    ),
    ADD CONSTRAINT ck_scheduled_settlement_property_tax_payload CHECK (
        kind <> 'propertyTaxPayment'
        OR (
            JSON_TYPE(payload) = 'OBJECT'
            AND JSON_LENGTH(payload) = 4
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.version')) = 'INTEGER'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.version')) = '1'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.propertyTaxEventId')) = 'STRING'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.propertyTaxEventId'))
                REGEXP '^[1-9][0-9]{0,19}$'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.propertyTaxPaymentId')) = 'STRING'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.propertyTaxPaymentId'))
                REGEXP '^[1-9][0-9]{0,19}$'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.paymentNo')) = 'INTEGER'
            AND CAST(
                JSON_UNQUOTE(JSON_EXTRACT(payload, '$.paymentNo')) AS UNSIGNED
            ) BETWEEN 1 AND 2
            AND source_kind = 'propertyTaxEvent'
            AND BINARY source_id = BINARY JSON_UNQUOTE(
                JSON_EXTRACT(payload, '$.propertyTaxEventId')
            )
            AND occurrence = CAST(
                JSON_UNQUOTE(JSON_EXTRACT(payload, '$.paymentNo')) AS UNSIGNED
            )
        )
    );

CREATE TRIGGER tr_scheduled_settlement_property_tax_insert
BEFORE INSERT ON scheduled_settlement
FOR EACH ROW
FOLLOWS tr_scheduled_settlement_lease_rent_insert
SET NEW.status = IF(
    NEW.kind <> 'propertyTaxPayment'
        OR EXISTS (
            SELECT 1
            FROM property_tax_event AS event
            INNER JOIN property_tax_payment AS payment
                ON payment.id = CAST(
                    JSON_UNQUOTE(
                        JSON_EXTRACT(NEW.payload, '$.propertyTaxPaymentId')
                    ) AS UNSIGNED
                )
               AND payment.save_id = event.save_id
               AND payment.run_revision = event.run_revision
               AND payment.property_tax_event_id = event.id
            WHERE event.id = CAST(
                      JSON_UNQUOTE(
                          JSON_EXTRACT(NEW.payload, '$.propertyTaxEventId')
                      ) AS UNSIGNED
                  )
              AND event.save_id = NEW.save_id
              AND event.run_revision = NEW.run_revision
              AND event.status = 'prepared'
              AND payment.payment_no = NEW.occurrence
              AND payment.due_game_day = NEW.due_game_day
              AND payment.status = 'pending'
              AND payment.scheduled_settlement_id IS NULL
        ),
    NEW.status,
    NULL
);

CREATE TRIGGER tr_scheduled_settlement_property_tax_transition
BEFORE UPDATE ON scheduled_settlement
FOR EACH ROW
FOLLOWS tr_scheduled_settlement_lease_rent_transition
SET NEW.status = IF(
    OLD.kind <> 'propertyTaxPayment'
        OR (
            NEW.status = 'settled'
            AND EXISTS (
                SELECT 1
                FROM property_tax_payment AS payment
                WHERE payment.id = CAST(
                          JSON_UNQUOTE(
                              JSON_EXTRACT(OLD.payload, '$.propertyTaxPaymentId')
                          ) AS UNSIGNED
                      )
                  AND payment.save_id = OLD.save_id
                  AND payment.run_revision = OLD.run_revision
                  AND payment.property_tax_event_id = CAST(
                      JSON_UNQUOTE(
                          JSON_EXTRACT(OLD.payload, '$.propertyTaxEventId')
                      ) AS UNSIGNED
                  )
                  AND payment.payment_no = OLD.occurrence
                  AND payment.status = 'applied'
                  AND payment.ledger_transaction_id
                        = NEW.settled_ledger_transaction_id
            )
        )
        OR (
            NEW.status = 'cancelled'
            AND NEW.cancellation_reason = 'newRun'
            AND NEW.cancellation_ledger_transaction_id IS NULL
            AND EXISTS (
                SELECT 1
                FROM property_tax_payment AS payment
                WHERE payment.id = CAST(
                          JSON_UNQUOTE(
                              JSON_EXTRACT(OLD.payload, '$.propertyTaxPaymentId')
                          ) AS UNSIGNED
                      )
                  AND payment.save_id = OLD.save_id
                  AND payment.run_revision = OLD.run_revision
                  AND payment.status = 'pending'
                  AND payment.scheduled_settlement_id = OLD.id
            )
        ),
    NEW.status,
    NULL
);

ALTER TABLE tax_obligation
    DROP INDEX uk_tax_obligation_source,
    DROP CHECK ck_tax_obligation_source,
    ADD COLUMN source_occurrence BIGINT UNSIGNED NOT NULL DEFAULT 1 AFTER source_id,
    ADD UNIQUE KEY uk_tax_obligation_source
        (save_id, run_revision, source_kind, source_id, source_occurrence),
    ADD CONSTRAINT ck_tax_obligation_source CHECK (
        source_kind IN (
            'financialIncomeAssessment', 'yearEndTaxAssessment', 'propertyTaxEvent'
        )
        AND source_id REGEXP '^[1-9][0-9]{0,19}$'
        AND source_occurrence > 0
        AND (
            source_kind = 'propertyTaxEvent'
            OR source_occurrence = 1
        )
    );

DROP TRIGGER tr_tax_obligation_valid_insert;
DROP TRIGGER tr_tax_obligation_transition_only;

CREATE TRIGGER tr_tax_obligation_valid_insert
BEFORE INSERT ON tax_obligation
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM save
        INNER JOIN household
            ON household.save_id = save.id
           AND household.run_revision = save.run_revision
           AND household.id = NEW.household_id
        WHERE save.id = NEW.save_id
          AND save.run_revision = NEW.run_revision
          AND save.policy_set_id = NEW.policy_set_id
    )
        AND (
            (
                NEW.source_kind = 'financialIncomeAssessment'
                AND NEW.source_occurrence = 1
                AND EXISTS (
                    SELECT 1 FROM financial_income_assessment AS assessment
                    WHERE assessment.save_id = NEW.save_id
                      AND assessment.run_revision = NEW.run_revision
                      AND assessment.tax_year = CAST(NEW.source_id AS UNSIGNED)
                      AND BINARY NEW.source_id = BINARY CAST(assessment.tax_year AS CHAR)
                      AND assessment.status = 'filed'
                      AND NEW.original_amount_krw <= assessment.additional_tax_krw
                )
            )
            OR (
                NEW.source_kind = 'yearEndTaxAssessment'
                AND NEW.source_occurrence = 1
                AND EXISTS (
                    SELECT 1 FROM year_end_tax_assessment AS assessment
                    WHERE assessment.id = CAST(NEW.source_id AS UNSIGNED)
                      AND BINARY NEW.source_id = BINARY CAST(assessment.id AS CHAR)
                      AND assessment.save_id = NEW.save_id
                      AND assessment.run_revision = NEW.run_revision
                      AND assessment.assessment_status = 'definitive'
                      AND NEW.original_amount_krw <= assessment.additional_tax_krw
                )
            )
            OR (
                NEW.source_kind = 'propertyTaxEvent'
                AND EXISTS (
                    SELECT 1
                    FROM property_tax_event AS event
                    INNER JOIN property_tax_payment AS payment
                        ON payment.property_tax_event_id = event.id
                       AND payment.payment_no = NEW.source_occurrence
                    WHERE event.id = CAST(NEW.source_id AS UNSIGNED)
                      AND BINARY NEW.source_id = BINARY CAST(event.id AS CHAR)
                      AND event.save_id = NEW.save_id
                      AND event.run_revision = NEW.run_revision
                      AND event.household_id = NEW.household_id
                      AND event.policy_set_id = NEW.policy_set_id
                      AND event.status IN ('scheduled', 'partiallyPaid')
                      AND payment.save_id = NEW.save_id
                      AND payment.run_revision = NEW.run_revision
                      AND payment.status = 'pending'
                      AND NEW.due_game_day = payment.due_game_day
                      AND NEW.original_amount_krw <= payment.amount_krw
                )
            )
        )
        AND (
            (
                NEW.status = 'prepared'
                AND NEW.authority_ledger_transaction_id IS NULL
            )
            OR (
                NEW.status = 'outstanding'
                AND EXISTS (
                    SELECT 1
                    FROM ledger_transaction AS ledger
                    WHERE ledger.id = NEW.authority_ledger_transaction_id
                      AND ledger.save_id = NEW.save_id
                      AND ledger.run_revision = NEW.run_revision
                      AND NEW.original_amount_krw = -(
                          SELECT COALESCE(SUM(posting.amount_krw), 0)
                          FROM ledger_posting AS posting
                          WHERE posting.ledger_transaction_id = ledger.id
                            AND posting.account_code IN (
                                'debtPrincipal', 'taxObligationLiability'
                            )
                      )
                )
            )
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_tax_obligation_transition_only
BEFORE UPDATE ON tax_obligation
FOR EACH ROW
SET NEW.id = IF(
    NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.household_id = OLD.household_id
        AND NEW.policy_set_id = OLD.policy_set_id
        AND BINARY NEW.source_kind = BINARY OLD.source_kind
        AND BINARY NEW.source_id = BINARY OLD.source_id
        AND NEW.source_occurrence = OLD.source_occurrence
        AND NEW.due_game_day = OLD.due_game_day
        AND NEW.original_amount_krw = OLD.original_amount_krw
        AND NEW.paid_amount_krw >= OLD.paid_amount_krw
        AND NEW.created_at = OLD.created_at
        AND (
            (
                OLD.status = 'prepared'
                AND NEW.status = 'outstanding'
                AND OLD.authority_ledger_transaction_id IS NULL
                AND NEW.authority_ledger_transaction_id IS NOT NULL
                AND NEW.paid_amount_krw = 0
                AND EXISTS (
                    SELECT 1
                    FROM ledger_transaction AS ledger
                    WHERE ledger.id = NEW.authority_ledger_transaction_id
                      AND ledger.save_id = OLD.save_id
                      AND ledger.run_revision = OLD.run_revision
                      AND OLD.original_amount_krw = -(
                          SELECT COALESCE(SUM(posting.amount_krw), 0)
                          FROM ledger_posting AS posting
                          WHERE posting.ledger_transaction_id = ledger.id
                            AND posting.account_code = 'taxObligationLiability'
                            AND posting.tax_obligation_id = OLD.id
                      )
                )
            )
            OR (
                OLD.status = 'outstanding'
                AND NEW.status IN ('outstanding', 'paid', 'discharged', 'chargedOff')
                AND NEW.authority_ledger_transaction_id
                    = OLD.authority_ledger_transaction_id
            )
        ),
    OLD.id,
    NULL
);

DROP TRIGGER tr_loan_payment_valid_insert;
DROP TRIGGER tr_loan_payment_transition_only;

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
                  (
                      NEW.payment_kind = 'scheduledInstallment'
                      AND NEW.property_sale_execution_id IS NULL
                  )
                  OR (
                      NEW.payment_kind = 'manualPrepayment'
                      AND NEW.property_sale_execution_id IS NULL
                      AND contract.status = 'active'
                      AND NOT EXISTS (
                          SELECT 1 FROM loan_obligation_bucket AS bucket
                          WHERE bucket.loan_contract_id = contract.id
                            AND bucket.status = 'delinquent'
                      )
                  )
                  OR (
                      NEW.payment_kind = 'leaseMovePayoff'
                      AND NEW.property_sale_execution_id IS NULL
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
                  OR (
                      NEW.payment_kind = 'propertySalePayoff'
                      AND contract.product_kind = 'mortgage'
                      AND contract.status = 'active'
                      AND contract.accrued_interest_krw = 0
                      AND contract.accrued_fee_krw = 0
                      AND NEW.game_day = save.game_day + 1
                      AND EXISTS (
                          SELECT 1
                          FROM property_sale_execution AS execution
                          WHERE execution.id = NEW.property_sale_execution_id
                            AND execution.save_id = contract.save_id
                            AND execution.run_revision = contract.run_revision
                            AND execution.property_holding_id
                                  = contract.property_holding_id
                            AND execution.execution_game_day = NEW.game_day
                            AND execution.status = 'prepared'
                            AND execution.mortgage_principal_krw
                                  = contract.remaining_principal_krw
                            AND execution.mortgage_prepayment_fee_krw = FLOOR(
                                CAST(contract.remaining_principal_krw AS DECIMAL(65, 0))
                                * contract.prepayment_fee_ppm / 1000000
                            )
                            AND NEW.amount_krw
                                  = execution.mortgage_principal_krw
                                    + execution.mortgage_prepayment_fee_krw
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

CREATE TRIGGER tr_loan_payment_transition_only
BEFORE UPDATE ON loan_payment
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'prepared'
        AND NEW.status = 'applied'
        AND NEW.ledger_transaction_id IS NOT NULL
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.loan_contract_id = OLD.loan_contract_id
        AND NEW.payment_no = OLD.payment_no
        AND BINARY NEW.payment_kind = BINARY OLD.payment_kind
        AND NEW.amount_krw = OLD.amount_krw
        AND NEW.game_day = OLD.game_day
        AND BINARY NEW.command_id <=> BINARY OLD.command_id
        AND NEW.property_sale_execution_id <=> OLD.property_sale_execution_id
        AND NEW.created_at = OLD.created_at
        AND NEW.amount_krw = (
            SELECT COALESCE(SUM(allocation.amount_krw), 0)
            FROM loan_payment_allocation AS allocation
            WHERE allocation.loan_payment_id = OLD.id
        ),
    OLD.id,
    NULL
);

DROP TRIGGER tr_property_holding_valid_insert;

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
               AND real_estate.version_key IN (
                    'dev-unranked-m4-real-estate-purchase-2026-v5',
                    'dev-unranked-m4-real-estate-sale-tax-2026-v6'
               )
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
            INNER JOIN real_estate_daily AS daily
                ON daily.market_world_id = bundle.market_world_id
               AND daily.real_estate_model_version_id = real_estate.id
               AND BINARY daily.region_key = BINARY listing.region_key
               AND daily.game_day = save.game_day
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
              AND (
                  real_estate.version_key
                        <> 'dev-unranked-m4-real-estate-sale-tax-2026-v6'
                  OR NEW.exclusive_area_square_meters <= 85
              )
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
              AND NEW.acquisition_price_index_ppm = daily.price_index_ppm
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
        AND NEW.acquisition_price_index_ppm = OLD.acquisition_price_index_ppm
        AND NEW.book_value_krw = OLD.book_value_krw
        AND NEW.created_at = OLD.created_at
        AND (
            EXISTS (
                SELECT 1 FROM save
                WHERE save.id = OLD.save_id
                  AND save.run_revision = OLD.run_revision
                  AND save.game_day = NEW.disposed_game_day
            )
            OR EXISTS (
                SELECT 1
                FROM property_sale_execution AS execution
                INNER JOIN property_sale_order AS sale_order
                    ON sale_order.id = execution.property_sale_order_id
                   AND sale_order.save_id = execution.save_id
                   AND sale_order.run_revision = execution.run_revision
                INNER JOIN property_sale_order_revision AS revision
                    ON revision.id = execution.property_sale_order_revision_id
                   AND revision.property_sale_order_id = sale_order.id
                INNER JOIN save
                    ON save.id = execution.save_id
                   AND save.run_revision = execution.run_revision
                WHERE execution.save_id = OLD.save_id
                  AND execution.run_revision = OLD.run_revision
                  AND execution.property_holding_id = OLD.id
                  AND execution.status = 'prepared'
                  AND execution.execution_game_day = NEW.disposed_game_day
                  AND revision.candidate_game_day = NEW.disposed_game_day
                  AND save.game_day + 1 = NEW.disposed_game_day
            )
        ),
    OLD.id,
    NULL
);

DROP TRIGGER tr_property_lien_transition_only;

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
              AND (
                  save.game_day = NEW.released_game_day
                  OR EXISTS (
                      SELECT 1
                      FROM property_sale_execution AS execution
                      WHERE execution.save_id = OLD.save_id
                        AND execution.run_revision = OLD.run_revision
                        AND execution.property_holding_id
                              = OLD.property_holding_id
                        AND execution.status = 'prepared'
                        AND execution.execution_game_day = NEW.released_game_day
                        AND save.game_day + 1 = NEW.released_game_day
                  )
              )
        ),
    OLD.id,
    NULL
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
              AND model.version_key IN (
                  'dev-unranked-m4-real-estate-purchase-2026-v5',
                  'dev-unranked-m4-real-estate-sale-tax-2026-v6'
              )
        )
    )
    OR (
        -- Compatibility runs retain their pre-C3 owner row without a fabricated holding.
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
              AND model.version_key NOT IN (
                  'dev-unranked-m4-real-estate-purchase-2026-v5',
                  'dev-unranked-m4-real-estate-sale-tax-2026-v6'
              )
        )
    ),
    NEW.save_id,
    NULL
);

DROP TRIGGER tr_loan_quote_valid_insert;

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
                            'dev-unranked-m4-real-estate-purchase-2026-v5',
                            'dev-unranked-m4-real-estate-sale-tax-2026-v6'
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
                         AND real_estate.version_key IN (
                              'dev-unranked-m4-real-estate-purchase-2026-v5',
                              'dev-unranked-m4-real-estate-sale-tax-2026-v6'
                         )
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

-- Rebuild the ordered posting chain so each earlier domain trigger explicitly admits C4 rows,
-- while the final C4 trigger validates every sale and property-tax posting amount and reference.
DROP TRIGGER tr_ledger_posting_lease_rent_reference_insert;
DROP TRIGGER tr_ledger_posting_property_reference_insert;
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
               AND (
                   BINARY CAST(payment.id AS CHAR) = BINARY ledger.source_id
                   OR (
                       ledger.source_kind = 'propertySale'
                       AND BINARY CAST(payment.property_sale_execution_id AS CHAR)
                             = BINARY ledger.source_id
                   )
               )
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
                  OR (
                      ledger.source_kind = 'propertySale'
                      AND NEW.account_code IN (
                          'loanPrincipalLiability', 'loanFeeExpense'
                      )
                      AND payment.payment_kind = 'propertySalePayoff'
                      AND payment.status = 'prepared'
                      AND EXISTS (
                          SELECT 1
                          FROM property_sale_execution AS execution
                          WHERE execution.id = payment.property_sale_execution_id
                            AND execution.save_id = ledger.save_id
                            AND execution.run_revision = ledger.run_revision
                            AND execution.status = 'prepared'
                            AND BINARY ledger.source_id
                                  = BINARY CAST(execution.id AS CHAR)
                            AND (
                                (
                                    NEW.account_code = 'loanPrincipalLiability'
                                    AND NEW.amount_krw
                                          = execution.mortgage_principal_krw
                                )
                                OR (
                                    NEW.account_code = 'loanFeeExpense'
                                    AND NEW.amount_krw
                                          = execution.mortgage_prepayment_fee_krw
                                )
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

CREATE TRIGGER tr_command_identity_m4c4_valid_insert
BEFORE INSERT ON command_identity
FOR EACH ROW
FOLLOWS tr_command_identity_m4c3_valid_insert
SET NEW.command_kind = IF(
    NEW.command_kind NOT IN (
        'createPropertySaleOrder',
        'repricePropertySaleOrder',
        'cancelPropertySaleOrder'
    )
        OR EXISTS (
            SELECT 1
            FROM save
            WHERE save.id = NEW.save_id
              AND save.run_revision = NEW.initial_run_revision
              AND save.state_revision = NEW.initial_state_revision
              AND save.game_day = NEW.initial_game_day
        ),
    NEW.command_kind,
    NULL
);

CREATE TRIGGER tr_command_receipt_m4c4_valid_insert
BEFORE INSERT ON command_receipt
FOR EACH ROW
FOLLOWS tr_command_receipt_m4c3_valid_insert
SET NEW.command_kind = IF(
    NEW.command_kind NOT IN (
        'purchaseProperty',
        'createPropertySaleOrder',
        'repricePropertySaleOrder',
        'cancelPropertySaleOrder'
    )
    OR (
        NEW.command_kind = 'purchaseProperty'
        AND (
            NOT EXISTS (
                SELECT 1
                FROM ledger_transaction AS ledger
                INNER JOIN property_holding AS holding
                    ON holding.save_id = ledger.save_id
                   AND holding.run_revision = ledger.run_revision
                   AND BINARY holding.acquisition_command_id
                         = BINARY ledger.source_id
                INNER JOIN real_estate_model_version AS model
                    ON model.id = holding.real_estate_model_version_id
                   AND model.version_key
                        = 'dev-unranked-m4-real-estate-sale-tax-2026-v6'
                WHERE ledger.id = NEW.ledger_transaction_id
                  AND ledger.save_id = NEW.save_id
                  AND ledger.run_revision = NEW.run_revision
                  AND ledger.source_kind = 'propertyPurchase'
                  AND BINARY ledger.source_id = BINARY NEW.command_id
            )
            OR EXISTS (
                SELECT 1
                FROM ledger_transaction AS ledger
                INNER JOIN property_holding AS holding
                    ON holding.save_id = ledger.save_id
                   AND holding.run_revision = ledger.run_revision
                   AND BINARY holding.acquisition_command_id
                         = BINARY ledger.source_id
                INNER JOIN real_estate_model_version AS model
                    ON model.id = holding.real_estate_model_version_id
                   AND model.version_key
                        = 'dev-unranked-m4-real-estate-sale-tax-2026-v6'
                   AND model.sealed_at IS NOT NULL
                INNER JOIN property_tax_event AS event
                    ON event.save_id = holding.save_id
                   AND event.run_revision = holding.run_revision
                   AND event.property_holding_id = holding.id
                   AND event.policy_set_id = holding.acquisition_policy_set_id
                   AND event.event_kind = 'acquisition'
                INNER JOIN policy_rule AS rule
                    ON rule.id = event.policy_rule_id
                   AND rule.policy_set_id = event.policy_set_id
                   AND rule.domain = 'propertyTax'
                   AND rule.rule_key = 'singleHomeAcquisitionTax'
                INNER JOIN property_tax_payment AS payment
                    ON payment.save_id = event.save_id
                   AND payment.run_revision = event.run_revision
                   AND payment.property_tax_event_id = event.id
                   AND payment.payment_no = 1
                INNER JOIN scheduled_settlement AS settlement
                    ON settlement.id = payment.scheduled_settlement_id
                   AND settlement.save_id = payment.save_id
                   AND settlement.run_revision = payment.run_revision
                WHERE ledger.id = NEW.ledger_transaction_id
                  AND ledger.save_id = NEW.save_id
                  AND ledger.run_revision = NEW.run_revision
                  AND ledger.source_kind = 'propertyPurchase'
                  AND BINARY ledger.source_id = BINARY NEW.command_id
                  AND event.status = 'scheduled'
                  AND event.assessment_game_day = NEW.game_day
                  AND event.taxable_game_day = NEW.game_day
                  AND event.total_tax_krw > 0
                  AND event.paid_tax_krw = 0
                  AND (
                      SELECT COUNT(*)
                      FROM property_tax_component AS component
                      WHERE component.save_id = event.save_id
                        AND component.run_revision = event.run_revision
                        AND component.property_tax_event_id = event.id
                  ) = 2
                  AND event.total_tax_krw = (
                      SELECT COALESCE(SUM(component.tax_amount_krw), 0)
                      FROM property_tax_component AS component
                      WHERE component.save_id = event.save_id
                        AND component.run_revision = event.run_revision
                        AND component.property_tax_event_id = event.id
                  )
                  AND (
                      SELECT COUNT(*)
                      FROM property_tax_payment AS event_payment
                      WHERE event_payment.save_id = event.save_id
                        AND event_payment.run_revision = event.run_revision
                        AND event_payment.property_tax_event_id = event.id
                  ) = 1
                  AND payment.status = 'pending'
                  AND payment.amount_krw = event.total_tax_krw
                  AND settlement.kind = 'propertyTaxPayment'
                  AND settlement.source_kind = 'propertyTaxEvent'
                  AND BINARY settlement.source_id = BINARY CAST(event.id AS CHAR)
                  AND settlement.occurrence = payment.payment_no
                  AND settlement.due_game_day = payment.due_game_day
                  AND settlement.status = 'pending'
            )
        )
    )
    OR (
        NEW.command_kind IN (
            'createPropertySaleOrder',
            'repricePropertySaleOrder',
            'cancelPropertySaleOrder'
        )
        AND NEW.ledger_transaction_id IS NULL
        AND JSON_TYPE(NEW.result) = 'OBJECT'
        AND EXISTS (
            SELECT 1
            FROM command_identity AS identity
            INNER JOIN save
                ON save.id = identity.save_id
               AND save.run_revision = NEW.run_revision
               AND save.state_revision = NEW.state_revision
               AND save.game_day = NEW.game_day
               AND save.market_world_id = NEW.market_world_id
            INNER JOIN property_sale_order_revision AS revision
                ON revision.save_id = identity.save_id
               AND revision.run_revision = NEW.run_revision
               AND BINARY revision.command_id = BINARY identity.command_id
            INNER JOIN property_sale_order AS sale_order
                ON sale_order.id = revision.property_sale_order_id
               AND sale_order.save_id = revision.save_id
               AND sale_order.run_revision = revision.run_revision
               AND sale_order.property_holding_id = revision.property_holding_id
            WHERE identity.save_id = NEW.save_id
              AND BINARY identity.command_id = BINARY NEW.command_id
              AND BINARY identity.command_kind = BINARY NEW.command_kind
              AND BINARY identity.payload_sha256 = BINARY NEW.payload_sha256
              AND identity.initial_run_revision = NEW.run_revision
              AND identity.initial_state_revision + 1 = NEW.state_revision
              AND identity.initial_game_day = NEW.game_day
              AND revision.created_game_day = NEW.game_day
              AND sale_order.current_revision_no = revision.revision_no
              AND (
                  (
                      NEW.command_kind = 'createPropertySaleOrder'
                      AND revision.revision_kind = 'listing'
                      AND revision.revision_no = 1
                      AND sale_order.status = 'active'
                      AND sale_order.created_game_day = NEW.game_day
                  )
                  OR (
                      NEW.command_kind = 'repricePropertySaleOrder'
                      AND revision.revision_kind = 'listing'
                      AND revision.revision_no > 1
                      AND sale_order.status = 'active'
                  )
                  OR (
                      NEW.command_kind = 'cancelPropertySaleOrder'
                      AND revision.revision_kind = 'cancellation'
                      AND revision.cancellation_reason = 'userRequest'
                      AND revision.revision_no > 1
                      AND sale_order.status = 'cancelled'
                      AND sale_order.terminal_game_day = NEW.game_day
                      AND sale_order.terminal_reason = 'userRequest'
                  )
              )
        )
    ),
    NEW.command_kind,
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
                  'leaseMove', 'propertyPurchase', 'propertySale'
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
        EXISTS (
            SELECT 1 FROM ledger_transaction AS ledger
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind IN ('propertySale', 'propertyTaxPayment')
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

CREATE TRIGGER tr_ledger_posting_property_tax_reference_insert
BEFORE INSERT ON ledger_posting
FOR EACH ROW
FOLLOWS tr_ledger_posting_property_reference_insert
SET NEW.account_code = IF(
    (
        EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            INNER JOIN property_sale_execution AS execution
                ON BINARY ledger.source_id = BINARY CAST(execution.id AS CHAR)
               AND execution.save_id = ledger.save_id
               AND execution.run_revision = ledger.run_revision
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'propertySale'
              AND execution.status = 'prepared'
              AND (
                  (
                      NEW.account_code = 'propertyAsset'
                      AND NEW.property_holding_id = execution.property_holding_id
                      AND NEW.property_tax_event_id IS NULL
                      AND NEW.loan_contract_id IS NULL
                      AND NEW.tax_obligation_id IS NULL
                      AND NEW.amount_krw = -execution.book_value_krw
                  )
                  OR (
                      NEW.account_code = 'realizedGainLoss'
                      AND NEW.property_holding_id = execution.property_holding_id
                      AND NEW.property_tax_event_id IS NULL
                      AND NEW.loan_contract_id IS NULL
                      AND NEW.tax_obligation_id IS NULL
                      AND NEW.amount_krw
                            = execution.book_value_krw
                              - execution.gross_sale_price_krw
                  )
                  OR (
                      NEW.account_code = 'propertyDispositionExpense'
                      AND NEW.property_holding_id = execution.property_holding_id
                      AND NEW.property_tax_event_id IS NULL
                      AND NEW.loan_contract_id IS NULL
                      AND NEW.tax_obligation_id IS NULL
                      AND NEW.amount_krw = execution.disposition_cost_krw
                  )
                  OR (
                      NEW.account_code IN (
                          'loanPrincipalLiability', 'loanFeeExpense'
                      )
                      AND NEW.property_holding_id IS NULL
                      AND NEW.property_tax_event_id IS NULL
                      AND NEW.tax_obligation_id IS NULL
                      AND EXISTS (
                          SELECT 1
                          FROM loan_payment AS payoff
                          WHERE payoff.loan_contract_id = NEW.loan_contract_id
                            AND payoff.save_id = ledger.save_id
                            AND payoff.run_revision = ledger.run_revision
                            AND payoff.property_sale_execution_id = execution.id
                            AND payoff.payment_kind = 'propertySalePayoff'
                            AND payoff.status = 'prepared'
                            AND (
                                (
                                    NEW.account_code = 'loanPrincipalLiability'
                                    AND NEW.amount_krw
                                          = execution.mortgage_principal_krw
                                )
                                OR (
                                    NEW.account_code = 'loanFeeExpense'
                                    AND NEW.amount_krw
                                          = execution.mortgage_prepayment_fee_krw
                                )
                            )
                      )
                  )
                  OR (
                      NEW.account_code = 'propertyTaxExpense'
                      AND NEW.property_holding_id IS NULL
                      AND NEW.loan_contract_id IS NULL
                      AND NEW.tax_obligation_id IS NULL
                      AND EXISTS (
                          SELECT 1
                          FROM property_tax_event AS event
                          INNER JOIN property_tax_component AS component
                              ON component.save_id = event.save_id
                             AND component.run_revision = event.run_revision
                             AND component.property_tax_event_id = event.id
                          WHERE event.id = NEW.property_tax_event_id
                            AND event.save_id = ledger.save_id
                            AND event.run_revision = ledger.run_revision
                            AND event.property_sale_execution_id = execution.id
                            AND event.event_kind = 'capitalGains'
                            AND event.status IN ('prepared', 'paid')
                            AND event.total_tax_krw = execution.transfer_tax_krw
                            AND component.component_kind IN (
                                'capitalGainsTax', 'capitalGainsLocalIncomeTax'
                            )
                            AND component.tax_amount_krw = NEW.amount_krw
                      )
                  )
                  OR (
                      NEW.account_code = 'wallet'
                      AND NEW.property_holding_id IS NULL
                      AND NEW.property_tax_event_id IS NULL
                      AND NEW.loan_contract_id IS NULL
                      AND NEW.tax_obligation_id IS NULL
                      AND NEW.amount_krw = execution.net_wallet_proceeds_krw
                  )
              )
        )
    )
    OR (
        EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            INNER JOIN property_tax_payment AS payment
                ON BINARY ledger.source_id = BINARY CAST(payment.id AS CHAR)
               AND payment.save_id = ledger.save_id
               AND payment.run_revision = ledger.run_revision
            INNER JOIN property_tax_event AS event
                ON event.id = payment.property_tax_event_id
               AND event.save_id = payment.save_id
               AND event.run_revision = payment.run_revision
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'propertyTaxPayment'
              AND payment.status = 'pending'
              AND event.status IN ('scheduled', 'partiallyPaid')
              AND (
                  (
                      NEW.account_code = 'propertyTaxExpense'
                      AND NEW.property_tax_event_id = event.id
                      AND NEW.property_holding_id IS NULL
                      AND NEW.loan_contract_id IS NULL
                      AND NEW.tax_obligation_id IS NULL
                      AND NEW.amount_krw = payment.amount_krw
                  )
                  OR (
                      NEW.account_code = 'wallet'
                      AND NEW.property_tax_event_id IS NULL
                      AND NEW.property_holding_id IS NULL
                      AND NEW.loan_contract_id IS NULL
                      AND NEW.tax_obligation_id IS NULL
                      AND NEW.amount_krw BETWEEN -payment.amount_krw AND -1
                  )
                  OR (
                      NEW.account_code = 'taxObligationLiability'
                      AND NEW.property_tax_event_id IS NULL
                      AND NEW.property_holding_id IS NULL
                      AND NEW.loan_contract_id IS NULL
                      AND NEW.amount_krw < 0
                      AND EXISTS (
                          SELECT 1
                          FROM tax_obligation AS obligation
                          WHERE obligation.id = NEW.tax_obligation_id
                            AND obligation.save_id = ledger.save_id
                            AND obligation.run_revision = ledger.run_revision
                            AND obligation.source_kind = 'propertyTaxEvent'
                            AND BINARY obligation.source_id
                                  = BINARY CAST(event.id AS CHAR)
                            AND obligation.source_occurrence = payment.payment_no
                            AND obligation.status = 'prepared'
                            AND NEW.amount_krw = -obligation.original_amount_krw
                      )
                  )
              )
        )
    )
    OR (
        NOT EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind IN ('propertySale', 'propertyTaxPayment')
        )
        AND NEW.account_code NOT IN (
            'propertyDispositionExpense', 'propertyTaxExpense'
        )
        AND NEW.property_tax_event_id IS NULL
    ),
    NEW.account_code,
    NULL
);


CREATE TRIGGER tr_ledger_posting_lease_rent_reference_insert
BEFORE INSERT ON ledger_posting
FOR EACH ROW
FOLLOWS tr_ledger_posting_property_tax_reference_insert
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

-- The employment graph is compatible with v3 because its seven finance rules are sealed-exact
-- clones of v2; the three added property-tax rules do not alter payroll interpretation.
DROP TRIGGER tr_employment_finance_compatibility_valid_insert;

INSERT INTO employment_finance_compatibility
    (employment_policy_set_id, policy_set_id)
SELECT employment_assignment.employment_policy_set_id, policy.id
FROM employment_policy_assignment AS employment_assignment
INNER JOIN policy_set AS policy
    ON policy.policy_key = 'dev-unranked-kr-individual-property-2026-v3'
   AND policy.sealed_at IS NOT NULL
WHERE employment_assignment.assignment_key = 'newRun';

CREATE TRIGGER tr_employment_finance_compatibility_valid_insert
BEFORE INSERT ON employment_finance_compatibility
FOR EACH ROW
SET NEW.employment_policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM employment_policy_set AS employment_policy
        INNER JOIN policy_set AS finance_policy
            ON finance_policy.id = NEW.policy_set_id
        WHERE employment_policy.id = NEW.employment_policy_set_id
          AND employment_policy.published_at IS NULL
          AND finance_policy.sealed_at IS NOT NULL
    ),
    NEW.employment_policy_set_id,
    NULL
);

-- Auto-committing DDL above is exposed to future runs only after every new graph, legacy byte,
-- compatibility edge, and existing-run pin passes one fail-closed publication barrier.
CREATE TEMPORARY TABLE m4c4_publication_guard (
    guard_key VARCHAR(48) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    accepted  TINYINT UNSIGNED NOT NULL,
    CONSTRAINT ck_m4c4_publication_guard CHECK (accepted = 1)
);

INSERT INTO m4c4_publication_guard (guard_key, accepted)
SELECT 'sealed-finance-v3', IF(
    EXISTS (
        SELECT 1
        FROM policy_set AS policy
        INNER JOIN policy_set_canonical_manifest AS manifest
            ON manifest.policy_set_id = policy.id
        INNER JOIN property_acquisition_tax_policy_profile AS acquisition
            ON acquisition.policy_set_id = policy.id
        INNER JOIN property_annual_tax_policy_profile AS annual
            ON annual.policy_set_id = policy.id
        INNER JOIN property_capital_gains_tax_policy_profile AS capital
            ON capital.policy_set_id = policy.id
        WHERE policy.policy_key
                = 'dev-unranked-kr-individual-property-2026-v3'
          AND policy.basis_date = '2026-07-27'
          AND policy.ranked_eligible = FALSE
          AND policy.sealed_at IS NOT NULL
          AND BINARY policy.canonical_sha256 = BINARY manifest.canonical_sha256
          AND acquisition.supported_home_count = 1
          AND acquisition.payment_due_days = 60
          AND annual.supported_home_count = 1
          AND annual.assessment_month = 6
          AND annual.assessment_day = 1
          AND annual.first_payment_month = 7
          AND annual.first_payment_day = 31
          AND annual.second_payment_month = 9
          AND annual.second_payment_day = 30
          AND capital.supported_home_count = 1
          AND capital.payment_rule = 'withheldAtSale'
          AND (SELECT COUNT(*) FROM policy_rule AS rule
               WHERE rule.policy_set_id = policy.id) = 10
          AND (
              SELECT COUNT(*)
              FROM policy_rule_clone_provenance AS clone
              INNER JOIN policy_rule AS target_rule
                  ON target_rule.id = clone.target_policy_rule_id
              WHERE target_rule.policy_set_id = policy.id
          ) = 7
          AND (
              SELECT COUNT(*)
              FROM policy_rule_source AS link
              INNER JOIN policy_rule AS rule ON rule.id = link.policy_rule_id
              WHERE rule.policy_set_id = policy.id
          ) = 9
          AND (SELECT COUNT(*)
               FROM property_annual_tax_fair_market_ratio_band AS band
               WHERE band.policy_set_id = policy.id) = 3
          AND (SELECT COUNT(*)
               FROM property_annual_tax_rate_bracket AS bracket
               WHERE bracket.policy_set_id = policy.id) = 8
          AND (SELECT COUNT(*)
               FROM property_capital_gains_tax_rate_bracket AS bracket
               WHERE bracket.policy_set_id = policy.id) = 16
          AND EXISTS (
              SELECT 1
              FROM employment_policy_assignment AS employment_assignment
              INNER JOIN employment_finance_compatibility AS compatibility
                  ON compatibility.employment_policy_set_id
                        = employment_assignment.employment_policy_set_id
                 AND compatibility.policy_set_id = policy.id
              WHERE employment_assignment.assignment_key = 'newRun'
          )
    ),
    1,
    0
);

INSERT INTO m4c4_publication_guard (guard_key, accepted)
SELECT 'sealed-real-estate-v6', IF(
    EXISTS (
        SELECT 1
        FROM real_estate_model_version AS model
        INNER JOIN real_estate_model_strict_manifest AS manifest
            ON manifest.real_estate_model_version_id = model.id
        INNER JOIN real_estate_model_strict_projection AS projection
            ON projection.real_estate_model_version_id = model.id
        INNER JOIN real_estate_sale_liquidity_profile AS liquidity
            ON liquidity.real_estate_model_version_id = model.id
        INNER JOIN real_estate_purchase_profile AS purchase
            ON purchase.real_estate_model_version_id = model.id
        WHERE model.version_key
                = 'dev-unranked-m4-real-estate-sale-tax-2026-v6'
          AND model.availability = 'active'
          AND model.ranked_eligible = FALSE
          AND model.sealed_at IS NOT NULL
          AND JSON_UNQUOTE(JSON_EXTRACT(model.parameters, '$.schemaVersion')) = '6'
          AND BINARY model.canonical_sha256 = BINARY manifest.canonical_sha256
          AND BINARY manifest.canonical_json = BINARY projection.canonical_json
          AND purchase.purchase_capability = 'ownerOccupiedSingleHome'
          AND purchase.maximum_active_holdings = 1
          AND liquidity.minimum_asking_ratio_ppm = 800000
          AND liquidity.low_band_maximum_ratio_ppm = 950000
          AND liquidity.middle_band_maximum_ratio_ppm = 1050000
          AND liquidity.maximum_asking_ratio_ppm = 1200000
          AND liquidity.low_delay_minimum_days = 1
          AND liquidity.low_delay_maximum_days = 3
          AND liquidity.middle_delay_minimum_days = 3
          AND liquidity.middle_delay_maximum_days = 7
          AND liquidity.high_delay_minimum_days = 7
          AND liquidity.high_delay_maximum_days = 30
          AND liquidity.candidate_entropy_key = 'propertySaleCandidate'
          AND liquidity.gross_price_rule = 'exactAskingPrice'
          AND liquidity.disposition_cost_ppm = 5000
          AND liquidity.minimum_disposition_cost_krw = 1
          AND liquidity.minimum_holding_years = 2
          AND liquidity.minimum_residence_years = 2
          AND liquidity.deficient_sale_proceeds = 'reject'
          AND liquidity.post_sale_tenure_type = 'rentFree'
          AND liquidity.provenance_kind = 'GAME_BALANCE'
          AND (SELECT COUNT(*) FROM real_estate_region_profile AS profile
               WHERE profile.real_estate_model_version_id = model.id)
                = (SELECT COUNT(*) FROM life_region)
          AND NOT EXISTS (
              SELECT 1 FROM real_estate_region_profile AS profile
              WHERE profile.real_estate_model_version_id = model.id
                AND profile.maximum_exclusive_area_square_meters > 85
          )
          AND (SELECT COUNT(*) FROM real_estate_region_property_type AS type
               WHERE type.real_estate_model_version_id = model.id)
                = (
                    SELECT COUNT(*)
                    FROM real_estate_region_property_type AS source_type
                    INNER JOIN real_estate_model_version AS source_model
                        ON source_model.id
                              = source_type.real_estate_model_version_id
                    WHERE source_model.version_key
                            = 'dev-unranked-m4-real-estate-purchase-2026-v5'
                )
          AND (SELECT COUNT(*) FROM real_estate_lease_profile AS lease
               WHERE lease.real_estate_model_version_id = model.id) = 2
          AND (SELECT COUNT(*) FROM real_estate_region_moving_cost AS cost
               WHERE cost.real_estate_model_version_id = model.id)
                = (SELECT COUNT(*) FROM life_region)
          AND (SELECT COUNT(*) FROM real_estate_purchase_region_mapping AS mapping
               WHERE mapping.real_estate_model_version_id = model.id)
                = (SELECT COUNT(*) FROM life_region)
    ),
    1,
    0
);

INSERT INTO m4c4_publication_guard (guard_key, accepted)
SELECT 'credit-v4-still-canonical', IF(
    EXISTS (
        SELECT 1
        FROM credit_model_version AS model
        INNER JOIN credit_model_strict_manifest AS manifest
            ON manifest.credit_model_version_id = model.id
        INNER JOIN credit_model_strict_projection AS projection
            ON projection.credit_model_version_id = model.id
        WHERE model.version_key = 'dev-unranked-m4c3-credit-2026-v4'
          AND model.availability = 'active'
          AND model.sealed_at IS NOT NULL
          AND BINARY model.canonical_sha256 = BINARY manifest.canonical_sha256
          AND BINARY manifest.canonical_json = BINARY projection.canonical_json
    ),
    1,
    0
);

INSERT INTO m4c4_publication_guard (guard_key, accepted)
SELECT 'legacy-manifests-byte-exact', IF(
    NOT EXISTS (
        SELECT 1
        FROM m4c4_legacy_real_estate_bytes AS legacy
        LEFT JOIN real_estate_model_version AS model
            ON model.id = legacy.real_estate_model_version_id
        LEFT JOIN real_estate_model_strict_manifest AS manifest
            ON manifest.real_estate_model_version_id
                  = legacy.real_estate_model_version_id
        LEFT JOIN real_estate_model_strict_projection AS projection
            ON projection.real_estate_model_version_id
                  = legacy.real_estate_model_version_id
        WHERE model.id IS NULL
           OR manifest.real_estate_model_version_id IS NULL
           OR projection.real_estate_model_version_id IS NULL
           OR BINARY legacy.canonical_json <> BINARY manifest.canonical_json
           OR BINARY legacy.canonical_sha256 <> BINARY manifest.canonical_sha256
           OR BINARY legacy.model_sha256 <> BINARY model.canonical_sha256
           OR BINARY manifest.canonical_json <> BINARY projection.canonical_json
    )
        AND NOT EXISTS (
            SELECT 1
            FROM m4c4_legacy_policy_bytes AS legacy
            LEFT JOIN policy_set AS policy
                ON policy.id = legacy.policy_set_id
            LEFT JOIN policy_set_canonical_manifest AS manifest
                ON manifest.policy_set_id = legacy.policy_set_id
            WHERE policy.id IS NULL
               OR manifest.policy_set_id IS NULL
               OR BINARY legacy.canonical_json <> BINARY manifest.canonical_json
               OR BINARY legacy.canonical_sha256 <> BINARY manifest.canonical_sha256
               OR BINARY legacy.policy_sha256 <> BINARY policy.canonical_sha256
        ),
    1,
    0
);

INSERT INTO m4c4_publication_guard (guard_key, accepted)
SELECT 'existing-run-pins-unchanged', IF(
    NOT EXISTS (
        SELECT 1
        FROM run_rule_bundle AS bundle
        WHERE bundle.policy_set_id = (
                  SELECT id FROM policy_set
                  WHERE policy_key
                        = 'dev-unranked-kr-individual-property-2026-v3'
              )
           OR bundle.real_estate_model_version_id = (
                  SELECT id FROM real_estate_model_version
                  WHERE version_key
                        = 'dev-unranked-m4-real-estate-sale-tax-2026-v6'
              )
    ),
    1,
    0
);

DROP TEMPORARY TABLE m4c4_publication_guard;

-- Finance assignment moves first because the composite assignment trigger requires the exact
-- current finance revision. The bundle then exposes policy v3 + real-estate v6 in one update;
-- credit remains pinned to the already canonical v4 graph.
UPDATE policy_set_assignment AS assignment
INNER JOIN policy_set AS policy
    ON policy.policy_key = 'dev-unranked-kr-individual-property-2026-v3'
   AND policy.sealed_at IS NOT NULL
SET assignment.policy_set_id = policy.id
WHERE assignment.assignment_key = 'newRun';

UPDATE run_rule_bundle_assignment AS assignment
INNER JOIN policy_set_assignment AS finance_assignment
    ON finance_assignment.assignment_key = 'newRun'
INNER JOIN policy_set AS policy
    ON policy.id = finance_assignment.policy_set_id
   AND policy.policy_key = 'dev-unranked-kr-individual-property-2026-v3'
   AND policy.sealed_at IS NOT NULL
INNER JOIN real_estate_model_version AS real_estate
    ON real_estate.version_key
        = 'dev-unranked-m4-real-estate-sale-tax-2026-v6'
   AND real_estate.availability = 'active'
   AND real_estate.sealed_at IS NOT NULL
INNER JOIN credit_model_version AS credit
    ON credit.version_key = 'dev-unranked-m4c3-credit-2026-v4'
   AND credit.availability = 'active'
   AND credit.sealed_at IS NOT NULL
SET assignment.policy_set_id = policy.id,
    assignment.real_estate_model_version_id = real_estate.id,
    assignment.credit_model_version_id = credit.id,
    assignment.finance_assignment_revision
        = finance_assignment.assignment_revision
WHERE assignment.assignment_key = 'newRun';

CREATE TEMPORARY TABLE m4c4_assignment_guard (
    guard_key VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    accepted  TINYINT UNSIGNED NOT NULL,
    CONSTRAINT ck_m4c4_assignment_guard CHECK (accepted = 1)
);

INSERT INTO m4c4_assignment_guard (guard_key, accepted)
SELECT 'new-run-v3-v6-v4', IF(
    EXISTS (
        SELECT 1
        FROM run_rule_bundle_assignment AS assignment
        INNER JOIN policy_set_assignment AS finance_assignment
            ON finance_assignment.assignment_key = assignment.assignment_key
           AND finance_assignment.policy_set_id = assignment.policy_set_id
           AND finance_assignment.assignment_revision
                = assignment.finance_assignment_revision
        INNER JOIN policy_set AS policy ON policy.id = assignment.policy_set_id
        INNER JOIN real_estate_model_version AS real_estate
            ON real_estate.id = assignment.real_estate_model_version_id
        INNER JOIN credit_model_version AS credit
            ON credit.id = assignment.credit_model_version_id
        WHERE assignment.assignment_key = 'newRun'
          AND policy.policy_key
                = 'dev-unranked-kr-individual-property-2026-v3'
          AND real_estate.version_key
                = 'dev-unranked-m4-real-estate-sale-tax-2026-v6'
          AND credit.version_key = 'dev-unranked-m4c3-credit-2026-v4'
    )
        AND NOT EXISTS (
            SELECT 1
            FROM run_rule_bundle AS bundle
            WHERE bundle.policy_set_id = (
                      SELECT id FROM policy_set
                      WHERE policy_key
                            = 'dev-unranked-kr-individual-property-2026-v3'
                  )
               OR bundle.real_estate_model_version_id = (
                      SELECT id FROM real_estate_model_version
                      WHERE version_key
                            = 'dev-unranked-m4-real-estate-sale-tax-2026-v6'
                  )
        ),
    1,
    0
);

DROP TEMPORARY TABLE m4c4_assignment_guard;
DROP TEMPORARY TABLE m4c4_legacy_real_estate_bytes;
DROP TEMPORARY TABLE m4c4_legacy_policy_bytes;
