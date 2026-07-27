-- M4-B credit policy provenance, active credit model, and immutable loan catalog (§2, §4.5).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;
SET SESSION group_concat_max_len = 1048576;

-- Credit policy is a separate sourced graph. The finance policy assignment remains unchanged,
-- so pre-M4 tax rules are never copied into a newly published graph without provenance.
INSERT INTO policy_source_document
    (source_key, source_url, checked_on, original_sha256)
VALUES
    (
        'fsc-borrower-dsr-threshold-2022-07-01',
        'https://www.fsc.go.kr/comm/getFile?srvcId=BBSTY1&upperNo=78428&fileTy=ATTACH&fileNo=2',
        '2026-07-27',
        'e00f024722d28265e564b596cd66543f9e6c87f3ece48b89b00a1faa956e049e'
    ),
    (
        'fsc-dsr-exclusions-and-credit-loan-faq-2021-10-26',
        'https://www.fsc.go.kr/comm/getFile?srvcId=BBSTY1&upperNo=76750&fileTy=ATTACH&fileNo=2',
        '2026-07-27',
        '1723273868a5d99a335adc560434eb90f748771e846faa07d6c6274bbc9a435d'
    ),
    (
        'fsc-stress-dsr-plan-2023-12-27',
        'https://www.fsc.go.kr/comm/getFile?srvcId=CARDNEWS&upperNo=2035&fileTy=ATTACH&fileNo=11',
        '2026-07-27',
        '5df6e071a37afa179f780d7016b0da2d0cd32dfb5eb9b675cf1f64745af05080'
    ),
    (
        'fsc-stage-three-stress-dsr-2025-07-01',
        'https://www.fsc.go.kr/comm/getFile?srvcId=BBSTY1&upperNo=84617&fileTy=ATTACH&fileNo=1',
        '2026-07-27',
        'dfa6ad7e9ce86446b74cfd31182fb0b6985b86341b6073d057c8c33488a72be4'
    ),
    (
        'law-bank-supervision-regulation-2026-04-01',
        'https://www.law.go.kr/admRulLsInfoR.do?admRulSeq=2100000276094&joTpYn=Y&languageType=KO&chrClsCd=010202',
        '2026-07-27',
        '432cb06640d90ea13b4f8d57bf649081de9862b10e01204b5da49abe40937d88'
    );

INSERT INTO policy_set (policy_key, basis_date, ranked_eligible)
VALUES ('dev-unranked-m4b-credit-policy-2026-v1', '2026-07-27', FALSE);

INSERT INTO policy_rule
    (policy_set_id, domain, rule_key, effective_from, effective_to, parameters)
SELECT policy.id,
       'credit',
       'borrowerDsrLimits',
       '2022-07-01',
       NULL,
       JSON_OBJECT(
           'annualIncomeStatusRequired', 'verified',
           'applicationBalanceBoundary', 'strictlyGreaterThan',
           'applicationBalanceThresholdKrw', 100000000,
           'bankLimitPpm', 400000,
           'evaluationHorizonMonths', 12,
           'nonBankLimitPpm', 500000,
           'ratioScalePpm', 1000000,
           'schemaVersion', 1
       )
FROM policy_set AS policy
WHERE policy.policy_key = 'dev-unranked-m4b-credit-policy-2026-v1'
UNION ALL
SELECT policy.id,
       'credit',
       'otherLoanDsrInclusion',
       '2021-10-26',
       NULL,
       JSON_OBJECT(
           'bulletAmortizationMonths', 60,
           'includedProductKinds', JSON_ARRAY('studentLoan', 'unsecuredLoan'),
           'scheduledLoanMeasure', 'nextTwelveMonthsPrincipalAndInterest',
           'schemaVersion', 1,
           'studentLoanClassification', 'otherHouseholdLoan'
       )
FROM policy_set AS policy
WHERE policy.policy_key = 'dev-unranked-m4b-credit-policy-2026-v1'
UNION ALL
SELECT policy.id,
       'credit',
       'unsecuredStressDsr2026H2',
       '2026-07-01',
       NULL,
       JSON_OBJECT(
           'applicationBalanceBoundary', 'strictlyGreaterThan',
           'applicationBalanceThresholdKrw', 100000000,
           'fixedAtLeastFiveYearsApplicationPpm', 0,
           'fixedAtLeastThreeYearsApplicationPpm', 600000,
           'otherFixedOrVariableApplicationPpm', 1000000,
           'schemaVersion', 1,
           'stressRateBp', 150
       )
FROM policy_set AS policy
WHERE policy.policy_key = 'dev-unranked-m4b-credit-policy-2026-v1';

INSERT INTO policy_rule_source
    (policy_rule_id, policy_source_document_id, citation_order)
SELECT rule.id, source.id, citation.citation_order
FROM policy_rule AS rule
INNER JOIN policy_set AS policy
    ON policy.id = rule.policy_set_id
INNER JOIN (
    SELECT 'borrowerDsrLimits' AS rule_key,
           'fsc-borrower-dsr-threshold-2022-07-01' AS source_key,
           1 AS citation_order
    UNION ALL
    SELECT 'borrowerDsrLimits', 'law-bank-supervision-regulation-2026-04-01', 2
    UNION ALL
    SELECT 'otherLoanDsrInclusion',
           'fsc-dsr-exclusions-and-credit-loan-faq-2021-10-26', 1
    UNION ALL
    SELECT 'unsecuredStressDsr2026H2', 'fsc-stress-dsr-plan-2023-12-27', 1
    UNION ALL
    SELECT 'unsecuredStressDsr2026H2',
           'fsc-stage-three-stress-dsr-2025-07-01', 2
    UNION ALL
    SELECT 'unsecuredStressDsr2026H2', 'law-bank-supervision-regulation-2026-04-01', 3
) AS citation ON BINARY citation.rule_key = BINARY rule.rule_key
INNER JOIN policy_source_document AS source
    ON BINARY source.source_key = BINARY citation.source_key
WHERE policy.policy_key = 'dev-unranked-m4b-credit-policy-2026-v1';

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
WHERE policy.policy_key = 'dev-unranked-m4b-credit-policy-2026-v1';

UPDATE policy_set AS policy
INNER JOIN policy_set_canonical_manifest AS manifest
    ON manifest.policy_set_id = policy.id
SET policy.canonical_sha256 = manifest.canonical_sha256,
    policy.sealed_at = CURRENT_TIMESTAMP(3)
WHERE policy.policy_key = 'dev-unranked-m4b-credit-policy-2026-v1'
  AND policy.sealed_at IS NULL;

-- Active credit models pin their own sourced credit policy. Disabled compatibility models keep
-- a null pin, so the finance policy assignment and existing run bundles remain untouched.
ALTER TABLE credit_model_version
    DROP CHECK ck_credit_model_version_shape,
    ADD COLUMN credit_policy_set_id BIGINT UNSIGNED NULL AFTER ranked_eligible,
    ADD KEY ix_credit_model_policy (credit_policy_set_id),
    ADD CONSTRAINT fk_credit_model_policy
        FOREIGN KEY (credit_policy_set_id) REFERENCES policy_set (id),
    ADD CONSTRAINT ck_credit_model_version_shape CHECK (
        JSON_TYPE(parameters) = 'OBJECT'
        AND (
            (availability = 'active' AND credit_policy_set_id IS NOT NULL)
            OR (availability = 'disabled' AND credit_policy_set_id IS NULL)
        )
        AND (
            (sealed_at IS NULL AND canonical_sha256 IS NULL)
            OR (sealed_at IS NOT NULL AND canonical_sha256 REGEXP '^[0-9a-f]{64}$')
        )
        AND (availability <> 'disabled' OR ranked_eligible = FALSE)
    );

DROP TRIGGER tr_credit_model_version_draft_insert;
DROP TRIGGER tr_credit_model_version_seal_only;

CREATE TABLE loan_product_version (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    credit_model_version_id     BIGINT UNSIGNED     NULL,
    product_key                 VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                VARCHAR(80)     NOT NULL,
    catalog_scope               VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    product_kind                VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    lender_sector               VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    rate_status                 VARCHAR(20) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    rate_type                   VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    reference_rate_key          VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin     NULL,
    fixed_annual_rate_bp        SMALLINT UNSIGNED     NULL,
    spread_bp                   SMALLINT              NULL,
    minimum_annual_rate_bp      SMALLINT UNSIGNED     NULL,
    maximum_annual_rate_bp      SMALLINT UNSIGNED     NULL,
    rate_reset_rule             VARCHAR(20) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    day_count_rule              VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    repayment_method            VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    term_months                 SMALLINT UNSIGNED     NULL,
    payment_calendar            VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    grace_months                SMALLINT UNSIGNED     NULL,
    minimum_principal_krw       BIGINT                NULL,
    maximum_principal_krw       BIGINT                NULL,
    prepayment_fee_ppm          INT UNSIGNED          NULL,
    prepayment_effect           VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    collateral_rule             VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    starting_eligible           BOOLEAN         NOT NULL,
    quote_eligible              BOOLEAN         NOT NULL,
    execution_eligible          BOOLEAN         NOT NULL,
    prepayment_allowed          BOOLEAN         NOT NULL,
    dsr_included                BOOLEAN         NOT NULL,
    read_only                   BOOLEAN         NOT NULL,
    provenance_kind             VARCHAR(20) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_order               SMALLINT UNSIGNED NOT NULL,
    canonical_sha256            CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    sealed_at                   DATETIME(3)          NULL,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_loan_product_version_key (product_key),
    UNIQUE KEY uk_loan_product_model_order (credit_model_version_id, display_order),
    UNIQUE KEY uk_loan_product_model_id (credit_model_version_id, id),
    KEY ix_loan_product_model_kind (credit_model_version_id, product_kind, display_order),
    CONSTRAINT fk_loan_product_model
        FOREIGN KEY (credit_model_version_id) REFERENCES credit_model_version (id),
    CONSTRAINT ck_loan_product_key CHECK (
        product_key REGEXP '^[a-z0-9][a-zA-Z0-9._-]{0,95}$'
    ),
    CONSTRAINT ck_loan_product_scope CHECK (
        catalog_scope IN ('modelChild', 'bridgeOnly')
    ),
    CONSTRAINT ck_loan_product_kind CHECK (
        product_kind IN (
            'studentLoan', 'unsecuredLoan', 'leaseDepositLoan', 'mortgage', 'legacyDebt'
        )
    ),
    CONSTRAINT ck_loan_product_lender CHECK (
        lender_sector IN ('bank', 'nonBank', 'bridgeOnly')
    ),
    CONSTRAINT ck_loan_product_rate_status CHECK (
        rate_status IN ('available', 'rateUnavailable')
    ),
    CONSTRAINT ck_loan_product_rate_type CHECK (
        rate_type IN ('fixed', 'variable', 'unavailable')
    ),
    CONSTRAINT ck_loan_product_reset CHECK (
        rate_reset_rule IN ('none', 'monthlyDay1')
    ),
    CONSTRAINT ck_loan_product_day_count CHECK (
        day_count_rule IN ('actual365', 'unavailable')
    ),
    CONSTRAINT ck_loan_product_repayment CHECK (
        repayment_method IN ('equalPrincipal', 'levelPayment', 'bullet')
    ),
    CONSTRAINT ck_loan_product_calendar CHECK (
        payment_calendar IN ('monthEnd', 'none')
    ),
    CONSTRAINT ck_loan_product_prepayment_effect CHECK (
        prepayment_effect IN ('reduceTerm', 'recalculatePayment', 'forbidden')
    ),
    CONSTRAINT ck_loan_product_collateral CHECK (
        collateral_rule IN ('none', 'valuationUnavailable', 'notApplicable')
    ),
    CONSTRAINT ck_loan_product_flags CHECK (
        starting_eligible IN (FALSE, TRUE)
        AND quote_eligible IN (FALSE, TRUE)
        AND execution_eligible IN (FALSE, TRUE)
        AND prepayment_allowed IN (FALSE, TRUE)
        AND dsr_included IN (FALSE, TRUE)
        AND read_only IN (FALSE, TRUE)
    ),
    CONSTRAINT ck_loan_product_provenance CHECK (
        provenance_kind IN ('GAME_BALANCE', 'COMPATIBILITY')
    ),
    CONSTRAINT ck_loan_product_publication CHECK (
        (sealed_at IS NULL AND canonical_sha256 IS NULL)
        OR (sealed_at IS NOT NULL AND canonical_sha256 REGEXP '^[0-9a-f]{64}$')
    ),
    CONSTRAINT ck_loan_product_rate_shape CHECK (
        (
            rate_type = 'fixed'
            AND rate_status = 'available'
            AND fixed_annual_rate_bp IS NOT NULL
            AND spread_bp IS NULL
            AND reference_rate_key IS NULL
            AND minimum_annual_rate_bp = fixed_annual_rate_bp
            AND maximum_annual_rate_bp = fixed_annual_rate_bp
            AND rate_reset_rule = 'none'
        )
        OR (
            rate_type = 'variable'
            AND rate_status = 'available'
            AND fixed_annual_rate_bp IS NULL
            AND spread_bp IS NOT NULL
            AND CHAR_LENGTH(reference_rate_key) > 0
            AND minimum_annual_rate_bp IS NOT NULL
            AND maximum_annual_rate_bp IS NOT NULL
            AND minimum_annual_rate_bp <= maximum_annual_rate_bp
            AND rate_reset_rule <> 'none'
        )
        OR (
            rate_type = 'unavailable'
            AND rate_status = 'rateUnavailable'
            AND fixed_annual_rate_bp IS NULL
            AND spread_bp IS NULL
            AND reference_rate_key IS NULL
            AND minimum_annual_rate_bp IS NULL
            AND maximum_annual_rate_bp IS NULL
            AND rate_reset_rule = 'none'
        )
    ),
    CONSTRAINT ck_loan_product_servicing_shape CHECK (
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
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE loan_product_canonical_manifest (
    loan_product_version_id BIGINT UNSIGNED NOT NULL,
    canonical_json          LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    canonical_sha256        CHAR(64) CHARACTER SET ascii COLLATE ascii_bin
        GENERATED ALWAYS AS (SHA2(canonical_json, 256)) STORED,
    created_at              DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (loan_product_version_id),
    UNIQUE KEY uk_loan_product_manifest_sha (canonical_sha256),
    CONSTRAINT fk_loan_product_manifest_product
        FOREIGN KEY (loan_product_version_id) REFERENCES loan_product_version (id),
    CONSTRAINT ck_loan_product_manifest_json CHECK (JSON_VALID(canonical_json))
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE loan_product_legacy_start_mapping (
    credit_model_version_id BIGINT UNSIGNED NOT NULL,
    legacy_field_key        VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    product_kind            VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    loan_product_version_id BIGINT UNSIGNED NOT NULL,
    mapping_order           TINYINT UNSIGNED NOT NULL,
    created_at              DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (credit_model_version_id, legacy_field_key),
    UNIQUE KEY uk_loan_legacy_mapping_order (credit_model_version_id, mapping_order),
    UNIQUE KEY uk_loan_legacy_mapping_kind (credit_model_version_id, product_kind),
    CONSTRAINT fk_loan_legacy_mapping_model
        FOREIGN KEY (credit_model_version_id) REFERENCES credit_model_version (id),
    CONSTRAINT fk_loan_legacy_mapping_product
        FOREIGN KEY (credit_model_version_id, loan_product_version_id)
        REFERENCES loan_product_version (credit_model_version_id, id),
    CONSTRAINT ck_loan_legacy_mapping_field CHECK (
        (legacy_field_key = 'studentLoanKrw' AND product_kind = 'studentLoan')
        OR (legacy_field_key = 'creditLoanKrw' AND product_kind = 'unsecuredLoan')
    ),
    CONSTRAINT ck_loan_legacy_mapping_order CHECK (mapping_order IN (1, 2))
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE credit_model_strict_manifest (
    credit_model_version_id BIGINT UNSIGNED NOT NULL,
    canonical_json          LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    canonical_sha256        CHAR(64) CHARACTER SET ascii COLLATE ascii_bin
        GENERATED ALWAYS AS (SHA2(canonical_json, 256)) STORED,
    created_at              DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (credit_model_version_id),
    UNIQUE KEY uk_credit_model_manifest_sha (canonical_sha256),
    CONSTRAINT fk_credit_model_manifest_model
        FOREIGN KEY (credit_model_version_id) REFERENCES credit_model_version (id),
    CONSTRAINT ck_credit_model_manifest_json CHECK (JSON_VALID(canonical_json))
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

-- These projections are the only canonical representation accepted by publication triggers.
CREATE VIEW loan_product_canonical_projection AS
SELECT
    product.id AS loan_product_version_id,
    CAST(JSON_OBJECT(
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
    ) AS CHAR CHARACTER SET utf8mb4) AS canonical_json
FROM loan_product_version AS product;

CREATE VIEW credit_model_strict_projection AS
SELECT
    model.id AS credit_model_version_id,
    CAST(JSON_OBJECT(
        'availability', model.availability,
        'creditPolicySetId', CAST(model.credit_policy_set_id AS CHAR),
        'legacyStartMappings', COALESCE((
            SELECT JSON_ARRAYAGG(JSON_OBJECT(
                       'legacyFieldKey', mapping.legacy_field_key,
                       'mappingOrder', mapping.mapping_order,
                       'productKey', product.product_key,
                       'productKind', mapping.product_kind
                   )) OVER (
                       ORDER BY mapping.mapping_order
                       ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING
                   )
            FROM loan_product_legacy_start_mapping AS mapping
            INNER JOIN loan_product_version AS product
                ON product.id = mapping.loan_product_version_id
            WHERE mapping.credit_model_version_id = model.id
            ORDER BY mapping.mapping_order
            LIMIT 1
        ), JSON_ARRAY()),
        'parameters', model.parameters,
        'products', COALESCE((
            SELECT JSON_ARRAYAGG(CAST(manifest.canonical_json AS JSON)) OVER (
                       ORDER BY product.display_order
                       ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING
                   )
            FROM loan_product_version AS product
            INNER JOIN loan_product_canonical_manifest AS manifest
                ON manifest.loan_product_version_id = product.id
            WHERE product.credit_model_version_id = model.id
              AND product.catalog_scope = 'modelChild'
              AND product.sealed_at IS NOT NULL
            ORDER BY product.display_order
            LIMIT 1
        ), JSON_ARRAY()),
        'schemaVersion', 2,
        'versionKey', model.version_key
    ) AS CHAR CHARACTER SET utf8mb4) AS canonical_json
FROM credit_model_version AS model
WHERE model.availability = 'active';

CREATE TRIGGER tr_loan_product_draft_insert
BEFORE INSERT ON loan_product_version
FOR EACH ROW
SET NEW.product_key = IF(
    NEW.sealed_at IS NULL
        AND NEW.canonical_sha256 IS NULL
        AND (
            (
                NEW.catalog_scope = 'modelChild'
                AND EXISTS (
                    SELECT 1 FROM credit_model_version AS model
                    WHERE model.id = NEW.credit_model_version_id
                      AND model.availability = 'active'
                      AND model.sealed_at IS NULL
                )
            )
            OR (
                NEW.catalog_scope = 'bridgeOnly'
                AND NEW.credit_model_version_id IS NULL
            )
        ),
    NEW.product_key,
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

CREATE TRIGGER tr_loan_product_no_delete
BEFORE DELETE ON loan_product_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'loan product versions are immutable';

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

CREATE TRIGGER tr_loan_product_manifest_no_update
BEFORE UPDATE ON loan_product_canonical_manifest
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'loan product manifests are immutable';

CREATE TRIGGER tr_loan_product_manifest_no_delete
BEFORE DELETE ON loan_product_canonical_manifest
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'loan product manifests are immutable';

CREATE TRIGGER tr_loan_legacy_mapping_draft_insert
BEFORE INSERT ON loan_product_legacy_start_mapping
FOR EACH ROW
SET NEW.credit_model_version_id = IF(
    EXISTS (
        SELECT 1 FROM credit_model_version AS model
        WHERE model.id = NEW.credit_model_version_id
          AND model.availability = 'active'
          AND model.sealed_at IS NULL
    ),
    NEW.credit_model_version_id,
    NULL
);

CREATE TRIGGER tr_loan_legacy_mapping_no_update
BEFORE UPDATE ON loan_product_legacy_start_mapping
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'legacy loan mappings are immutable';

CREATE TRIGGER tr_loan_legacy_mapping_no_delete
BEFORE DELETE ON loan_product_legacy_start_mapping
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'legacy loan mappings are immutable';

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

CREATE TRIGGER tr_credit_model_manifest_no_update
BEFORE UPDATE ON credit_model_strict_manifest
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'credit model manifests are immutable';

CREATE TRIGGER tr_credit_model_manifest_no_delete
BEFORE DELETE ON credit_model_strict_manifest
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'credit model manifests are immutable';

CREATE TRIGGER tr_credit_model_version_draft_insert
BEFORE INSERT ON credit_model_version
FOR EACH ROW
SET NEW.version_key = IF(
    NEW.sealed_at IS NULL
        AND NEW.canonical_sha256 IS NULL
        AND JSON_TYPE(NEW.parameters) = 'OBJECT'
        AND (
            (
                NEW.availability = 'active'
                AND NEW.credit_policy_set_id IS NOT NULL
                AND EXISTS (
                    SELECT 1 FROM policy_set AS policy
                    WHERE policy.id = NEW.credit_policy_set_id
                      AND policy.sealed_at IS NOT NULL
                      AND policy.ranked_eligible = FALSE
                )
            )
            OR (
                NEW.availability = 'disabled'
                AND NEW.credit_policy_set_id IS NULL
                AND NEW.ranked_eligible = FALSE
            )
        ),
    NEW.version_key,
    NULL
);

-- Active publication requires the exact generated strict manifest and complete typed children.
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
                AND (SELECT COUNT(*) FROM loan_product_version AS product
                     WHERE product.credit_model_version_id = OLD.id
                       AND product.catalog_scope = 'modelChild'
                       AND product.sealed_at IS NOT NULL) = 2
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
    'dev-unranked-m4b-credit-2026-v1',
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
        'provenance', 'GAME_BALANCE',
        'schemaVersion', 2
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
        prepayment_effect, collateral_rule, starting_eligible, quote_eligible,
        execution_eligible, prepayment_allowed, dsr_included, read_only,
        provenance_kind, display_order
    )
SELECT
    model.id, 'dev-student-fixed-equal-principal-2026-v1', '개발 학자금 고정금리 대출',
    'modelChild', 'studentLoan', 'bank', 'available', 'fixed', NULL, 170,
    NULL, 170, 170, 'none', 'actual365', 'equalPrincipal', 120, 'monthEnd', 0,
    1, 50000000, 0, 'reduceTerm', 'none', TRUE, FALSE, FALSE, TRUE, TRUE, FALSE,
    'GAME_BALANCE', 1
FROM credit_model_version AS model
WHERE model.version_key = 'dev-unranked-m4b-credit-2026-v1'
UNION ALL
SELECT
    model.id, 'dev-unsecured-variable-level-payment-2026-v1', '개발 변동금리 신용대출',
    'modelChild', 'unsecuredLoan', 'bank', 'available', 'variable', 'treasury3m', NULL,
    400, 300, 1500, 'monthlyDay1', 'actual365', 'levelPayment', 60, 'monthEnd', 0,
    1, 200000000, 10000, 'recalculatePayment', 'none', TRUE, TRUE, TRUE, TRUE, TRUE,
    FALSE, 'GAME_BALANCE', 2
FROM credit_model_version AS model
WHERE model.version_key = 'dev-unranked-m4b-credit-2026-v1';

INSERT INTO loan_product_version
    (
        credit_model_version_id, product_key, display_name, catalog_scope, product_kind,
        lender_sector, rate_status, rate_type, reference_rate_key, fixed_annual_rate_bp,
        spread_bp, minimum_annual_rate_bp, maximum_annual_rate_bp, rate_reset_rule,
        day_count_rule, repayment_method, term_months, payment_calendar, grace_months,
        minimum_principal_krw, maximum_principal_krw, prepayment_fee_ppm,
        prepayment_effect, collateral_rule, starting_eligible, quote_eligible,
        execution_eligible, prepayment_allowed, dsr_included, read_only,
        provenance_kind, display_order
    )
VALUES
    (
        NULL, 'compat-legacy-debt-zero-bullet-v1', '이전 버전 합산 부채', 'bridgeOnly',
        'legacyDebt', 'bridgeOnly', 'rateUnavailable', 'unavailable', NULL, NULL,
        NULL, NULL, NULL, 'none', 'unavailable', 'bullet', NULL, 'none', NULL,
        NULL, NULL, NULL, 'forbidden', 'notApplicable', FALSE, FALSE, FALSE, FALSE,
        FALSE, TRUE, 'COMPATIBILITY', 65535
    );

INSERT INTO loan_product_canonical_manifest (loan_product_version_id, canonical_json)
SELECT loan_product_version_id, canonical_json
FROM loan_product_canonical_projection;

UPDATE loan_product_version AS product
INNER JOIN loan_product_canonical_manifest AS manifest
    ON manifest.loan_product_version_id = product.id
SET product.canonical_sha256 = manifest.canonical_sha256,
    product.sealed_at = CURRENT_TIMESTAMP(3)
WHERE product.sealed_at IS NULL;

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
WHERE model.version_key = 'dev-unranked-m4b-credit-2026-v1'
UNION ALL
SELECT model.id, 'creditLoanKrw', 'unsecuredLoan', product.id, 2
FROM credit_model_version AS model
INNER JOIN loan_product_version AS product
    ON product.credit_model_version_id = model.id
   AND product.product_kind = 'unsecuredLoan'
WHERE model.version_key = 'dev-unranked-m4b-credit-2026-v1';

INSERT INTO credit_model_strict_manifest (credit_model_version_id, canonical_json)
SELECT credit_model_version_id, canonical_json
FROM credit_model_strict_projection
WHERE credit_model_version_id = (
    SELECT id FROM credit_model_version
    WHERE version_key = 'dev-unranked-m4b-credit-2026-v1'
);

UPDATE credit_model_version AS model
INNER JOIN credit_model_strict_manifest AS manifest
    ON manifest.credit_model_version_id = model.id
SET model.canonical_sha256 = manifest.canonical_sha256,
    model.sealed_at = CURRENT_TIMESTAMP(3)
WHERE model.version_key = 'dev-unranked-m4b-credit-2026-v1'
  AND model.sealed_at IS NULL;
