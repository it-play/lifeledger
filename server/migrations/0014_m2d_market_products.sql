-- M2-D v4 market factors, immutable market products, and the v2 annual-tax policy.

ALTER TABLE market_daily
    ADD COLUMN cpi_index BIGINT NULL AFTER equity_rate_shock_ppm,
    ADD COLUMN cpi_remainder BIGINT NULL AFTER cpi_index,
    ADD COLUMN llx_close_krw BIGINT NULL AFTER cpi_remainder,
    ADD COLUMN llx_return_ppm BIGINT NULL AFTER llx_close_krw,
    ADD COLUMN llx_fee_remainder BIGINT NULL AFTER llx_return_ppm,
    ADD COLUMN llx_fee_accumulator_ppm BIGINT NULL AFTER llx_fee_remainder,
    ADD COLUMN gold_close_krw_per_gram BIGINT NULL AFTER llx_fee_accumulator_ppm,
    ADD COLUMN gold_prior_open_cpi_index BIGINT NULL AFTER gold_close_krw_per_gram,
    ADD COLUMN gold_prior_open_treasury_10y_bp SMALLINT NULL
        AFTER gold_prior_open_cpi_index,
    ADD CONSTRAINT ck_market_daily_m2_complete CHECK (
        (
            cpi_index IS NULL
            AND cpi_remainder IS NULL
            AND llx_close_krw IS NULL
            AND llx_return_ppm IS NULL
            AND llx_fee_remainder IS NULL
            AND llx_fee_accumulator_ppm IS NULL
            AND gold_close_krw_per_gram IS NULL
            AND gold_prior_open_cpi_index IS NULL
            AND gold_prior_open_treasury_10y_bp IS NULL
        )
        OR
        (
            cpi_index IS NOT NULL
            AND cpi_remainder IS NOT NULL
            AND llx_close_krw IS NOT NULL
            AND llx_return_ppm IS NOT NULL
            AND llx_fee_remainder IS NOT NULL
            AND llx_fee_accumulator_ppm IS NOT NULL
            AND gold_close_krw_per_gram IS NOT NULL
            AND gold_prior_open_cpi_index IS NOT NULL
            AND gold_prior_open_treasury_10y_bp IS NOT NULL
        )
    ),
    ADD CONSTRAINT ck_market_daily_cpi_state CHECK (
        cpi_index IS NULL
        OR (
            cpi_index > 0
            AND cpi_remainder BETWEEN 0 AND 364999999
        )
    ),
    ADD CONSTRAINT ck_market_daily_llx_state CHECK (
        llx_close_krw IS NULL
        OR (
            llx_close_krw > 0
            AND llx_fee_remainder BETWEEN 0 AND 364
            AND llx_fee_accumulator_ppm >= 0
            AND (market_open = 1 OR llx_return_ppm = 0)
        )
    ),
    ADD CONSTRAINT ck_market_daily_gold_state CHECK (
        gold_close_krw_per_gram IS NULL
        OR (
            gold_close_krw_per_gram > 0
            AND gold_prior_open_cpi_index > 0
            AND gold_prior_open_cpi_index <= cpi_index
            AND gold_prior_open_treasury_10y_bp BETWEEN 0 AND 1500
        )
    );

INSERT INTO market_calibration (id, version, parameters)
SELECT
    4,
    'm2-2026-calibration-v4',
    JSON_SET(
        parameters,
        '$.m2',
        JSON_OBJECT(
            'cpi', JSON_OBJECT(
                'day0Index', 1000000,
                'annualRatePpm', 20000,
                'dayCountDenominator', 365
            ),
            'gold', JSON_OBJECT(
                'day0CloseKrwPerGram', 120000,
                'innovationScalePpm', 11000,
                'treasury10ySensitivityPpmPerBp', -250
            )
        )
    )
FROM market_calibration
WHERE version = 'm1-2026-calibration-v3';

INSERT INTO market_world
    (id, world_key, seed, start_date, day0_equity_close_krw, calibration_id)
VALUES
    (4, 'm2-2026-v4', 20260101, '2026-01-01', 100000, 4);

CREATE TABLE index_product_version (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    product_key                     VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                    VARCHAR(100)    NOT NULL,
    day0_close_krw                  BIGINT          NOT NULL,
    annual_management_fee_ppm       INT             NOT NULL,
    annual_distribution_rate_ppm    INT             NOT NULL,
    day_count_denominator           SMALLINT UNSIGNED NOT NULL,
    buy_fee_ppm                     INT             NOT NULL,
    sell_fee_ppm                    INT             NOT NULL,
    transaction_tax_ppm             INT             NOT NULL,
    published_at                    DATETIME(3)      NOT NULL,
    created_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_index_product_version_key (product_key),
    CONSTRAINT ck_index_product_version_terms CHECK (
        CHAR_LENGTH(product_key) > 0
        AND CHAR_LENGTH(display_name) > 0
        AND day0_close_krw > 0
        AND annual_management_fee_ppm BETWEEN 0 AND 1000000
        AND annual_distribution_rate_ppm BETWEEN 0 AND 1000000
        AND day_count_denominator > 0
        AND buy_fee_ppm BETWEEN 0 AND 1000000
        AND sell_fee_ppm BETWEEN 0 AND 1000000
        AND transaction_tax_ppm BETWEEN 0 AND 1000000
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE bond_product_version (
    id                      BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    product_key             VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name            VARCHAR(100)    NOT NULL,
    term_years              TINYINT UNSIGNED NOT NULL,
    face_value_krw          BIGINT          NOT NULL,
    max_order_units         INT UNSIGNED    NOT NULL,
    max_position_units      INT UNSIGNED    NOT NULL,
    buy_fee_ppm             INT             NOT NULL,
    sell_fee_ppm            INT             NOT NULL,
    published_at            DATETIME(3)     NOT NULL,
    created_at              DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_bond_product_version_key (product_key),
    CONSTRAINT ck_bond_product_version_terms CHECK (
        CHAR_LENGTH(product_key) > 0
        AND CHAR_LENGTH(display_name) > 0
        AND term_years > 0
        AND face_value_krw > 0
        AND max_order_units > 0
        AND max_position_units > 0
        AND buy_fee_ppm BETWEEN 0 AND 1000000
        AND sell_fee_ppm BETWEEN 0 AND 1000000
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE gold_product_version (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    product_key                 VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                VARCHAR(100)    NOT NULL,
    unit                        VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    buy_fee_ppm                 INT             NOT NULL,
    sell_fee_ppm                INT             NOT NULL,
    buy_tax_ppm                 INT             NOT NULL,
    sell_tax_ppm                INT             NOT NULL,
    withdrawal_100g_fee_krw     BIGINT          NOT NULL,
    withdrawal_1000g_fee_krw    BIGINT          NOT NULL,
    published_at                DATETIME(3)      NOT NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_gold_product_version_key (product_key),
    CONSTRAINT ck_gold_product_version_terms CHECK (
        CHAR_LENGTH(product_key) > 0
        AND CHAR_LENGTH(display_name) > 0
        AND unit = 'gram'
        AND buy_fee_ppm BETWEEN 0 AND 1000000
        AND sell_fee_ppm BETWEEN 0 AND 1000000
        AND buy_tax_ppm BETWEEN 0 AND 1000000
        AND sell_tax_ppm BETWEEN 0 AND 1000000
        AND withdrawal_100g_fee_krw >= 0
        AND withdrawal_1000g_fee_krw >= 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_index_product_version_no_update
BEFORE UPDATE ON index_product_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'index product versions are immutable';

CREATE TRIGGER tr_index_product_version_no_delete
BEFORE DELETE ON index_product_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'index product versions are immutable';

CREATE TRIGGER tr_bond_product_version_no_update
BEFORE UPDATE ON bond_product_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'bond product versions are immutable';

CREATE TRIGGER tr_bond_product_version_no_delete
BEFORE DELETE ON bond_product_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'bond product versions are immutable';

CREATE TRIGGER tr_gold_product_version_no_update
BEFORE UPDATE ON gold_product_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'gold product versions are immutable';

CREATE TRIGGER tr_gold_product_version_no_delete
BEFORE DELETE ON gold_product_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'gold product versions are immutable';

INSERT INTO index_product_version
    (
        id,
        product_key,
        display_name,
        day0_close_krw,
        annual_management_fee_ppm,
        annual_distribution_rate_ppm,
        day_count_denominator,
        buy_fee_ppm,
        sell_fee_ppm,
        transaction_tax_ppm,
        published_at
    )
VALUES
    (
        1,
        'llx-domestic-equity-2026-v1',
        'LLX 국내주식 지수',
        100000,
        1500,
        20000,
        365,
        0,
        0,
        0,
        CURRENT_TIMESTAMP(3)
    );

INSERT INTO bond_product_version
    (
        id,
        product_key,
        display_name,
        term_years,
        face_value_krw,
        max_order_units,
        max_position_units,
        buy_fee_ppm,
        sell_fee_ppm,
        published_at
    )
VALUES
    (1, 'kr-government-bond-3y-2026-v1', '대한민국 국고채 3년', 3, 10000, 100000, 100000, 0, 0, CURRENT_TIMESTAMP(3)),
    (2, 'kr-government-bond-10y-2026-v1', '대한민국 국고채 10년', 10, 10000, 100000, 100000, 0, 0, CURRENT_TIMESTAMP(3));

INSERT INTO gold_product_version
    (
        id,
        product_key,
        display_name,
        unit,
        buy_fee_ppm,
        sell_fee_ppm,
        buy_tax_ppm,
        sell_tax_ppm,
        withdrawal_100g_fee_krw,
        withdrawal_1000g_fee_krw,
        published_at
    )
VALUES
    (1, 'krx-gold-2026-v1', 'KRX 금시장 금 1g', 'gram', 0, 0, 0, 0, 20000, 100000, CURRENT_TIMESTAMP(3));

CREATE TABLE market_world_product_bundle (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    market_world_id                 BIGINT UNSIGNED NOT NULL,
    index_product_version_id        BIGINT UNSIGNED NOT NULL,
    bond_3y_product_version_id      BIGINT UNSIGNED NOT NULL,
    bond_10y_product_version_id     BIGINT UNSIGNED NOT NULL,
    gold_product_version_id         BIGINT UNSIGNED NOT NULL,
    published_at                    DATETIME(3)      NOT NULL,
    created_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_market_world_product_bundle_world (market_world_id),
    UNIQUE KEY uk_market_world_product_bundle_world_id (market_world_id, id),
    KEY ix_market_world_bundle_index_product (index_product_version_id),
    KEY ix_market_world_bundle_bond_3y_product (bond_3y_product_version_id),
    KEY ix_market_world_bundle_bond_10y_product (bond_10y_product_version_id),
    KEY ix_market_world_bundle_gold_product (gold_product_version_id),
    CONSTRAINT fk_market_world_bundle_world
        FOREIGN KEY (market_world_id) REFERENCES market_world (id),
    CONSTRAINT fk_market_world_bundle_index_product
        FOREIGN KEY (index_product_version_id) REFERENCES index_product_version (id),
    CONSTRAINT fk_market_world_bundle_bond_3y_product
        FOREIGN KEY (bond_3y_product_version_id) REFERENCES bond_product_version (id),
    CONSTRAINT fk_market_world_bundle_bond_10y_product
        FOREIGN KEY (bond_10y_product_version_id) REFERENCES bond_product_version (id),
    CONSTRAINT fk_market_world_bundle_gold_product
        FOREIGN KEY (gold_product_version_id) REFERENCES gold_product_version (id),
    CONSTRAINT ck_market_world_product_bundle_distinct_bonds CHECK (
        bond_3y_product_version_id <> bond_10y_product_version_id
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

-- A published bundle is a closed list and can only bind the registered v4 world to the exact
-- published product versions. Later product releases require a new world and a new bundle.
CREATE TRIGGER tr_market_world_product_bundle_valid_insert
BEFORE INSERT ON market_world_product_bundle
FOR EACH ROW
SET NEW.market_world_id = IF(
    EXISTS (
        SELECT 1
        FROM market_world AS world
        INNER JOIN market_calibration AS calibration ON calibration.id = world.calibration_id
        WHERE world.id = NEW.market_world_id
          AND BINARY world.world_key = BINARY 'm2-2026-v4'
          AND BINARY calibration.version = BINARY 'm2-2026-calibration-v4'
    )
    AND EXISTS (
        SELECT 1
        FROM index_product_version
        WHERE id = NEW.index_product_version_id
          AND BINARY product_key = BINARY 'llx-domestic-equity-2026-v1'
          AND published_at IS NOT NULL
    )
    AND EXISTS (
        SELECT 1
        FROM bond_product_version
        WHERE id = NEW.bond_3y_product_version_id
          AND BINARY product_key = BINARY 'kr-government-bond-3y-2026-v1'
          AND term_years = 3
          AND published_at IS NOT NULL
    )
    AND EXISTS (
        SELECT 1
        FROM bond_product_version
        WHERE id = NEW.bond_10y_product_version_id
          AND BINARY product_key = BINARY 'kr-government-bond-10y-2026-v1'
          AND term_years = 10
          AND published_at IS NOT NULL
    )
    AND EXISTS (
        SELECT 1
        FROM gold_product_version
        WHERE id = NEW.gold_product_version_id
          AND BINARY product_key = BINARY 'krx-gold-2026-v1'
          AND published_at IS NOT NULL
    ),
    NEW.market_world_id,
    NULL
);

CREATE TRIGGER tr_market_world_product_bundle_no_update
BEFORE UPDATE ON market_world_product_bundle
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'market world product bundles are immutable';

CREATE TRIGGER tr_market_world_product_bundle_no_delete
BEFORE DELETE ON market_world_product_bundle
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'market world product bundles are immutable';

INSERT INTO market_world_product_bundle
    (
        id,
        market_world_id,
        index_product_version_id,
        bond_3y_product_version_id,
        bond_10y_product_version_id,
        gold_product_version_id,
        published_at
    )
VALUES
    (1, 4, 1, 1, 2, 1, CURRENT_TIMESTAMP(3));

-- Existing saves predate product bundles and remain NULL. New v4 runs pin bundle id 1 explicitly.
ALTER TABLE save
    ADD COLUMN market_world_product_bundle_id BIGINT UNSIGNED NULL AFTER policy_set_id,
    ADD KEY ix_save_market_world_product_bundle_pin
        (market_world_id, market_world_product_bundle_id),
    ADD CONSTRAINT fk_save_market_world_product_bundle
        FOREIGN KEY (market_world_id, market_world_product_bundle_id)
        REFERENCES market_world_product_bundle (market_world_id, id);

INSERT INTO policy_set (id, policy_key, basis_date)
VALUES (2, 'kr-individual-2026-v2', '2026-01-01');

INSERT INTO policy_rule
    (policy_set_id, domain, rule_key, effective_from, effective_to, parameters)
SELECT
    2,
    domain,
    rule_key,
    effective_from,
    effective_to,
    parameters
FROM policy_rule
WHERE policy_set_id = 1;

INSERT INTO policy_rule
    (policy_set_id, domain, rule_key, effective_from, effective_to, parameters)
VALUES
    (
        2,
        'tax',
        'annualFinancialIncomeAssessment',
        '2026-01-01',
        NULL,
        JSON_OBJECT(
            'comprehensiveThresholdKrw', 20000000,
            'generalIncomeTaxRatePpm', 140000,
            'generalLocalIncomeTaxRatePpm', 14000,
            'nonFinancialComprehensiveIncomeKrw', 0,
            'incomeTaxCreditKrw', 0,
            'localIncomeTaxCreditKrw', 0,
            'comparisonFormula', 'independentMaxOfFormulaAAndB',
            'cashShortageTreatment', 'interestFreeAggregateDebt',
            'incomeTaxBrackets', JSON_ARRAY(
                JSON_OBJECT('lowerBoundKrw', 0, 'upperBoundKrw', 14000000, 'ratePpm', 60000),
                JSON_OBJECT('lowerBoundKrw', 14000000, 'upperBoundKrw', 50000000, 'ratePpm', 150000),
                JSON_OBJECT('lowerBoundKrw', 50000000, 'upperBoundKrw', 88000000, 'ratePpm', 240000),
                JSON_OBJECT('lowerBoundKrw', 88000000, 'upperBoundKrw', 150000000, 'ratePpm', 350000),
                JSON_OBJECT('lowerBoundKrw', 150000000, 'upperBoundKrw', 300000000, 'ratePpm', 380000),
                JSON_OBJECT('lowerBoundKrw', 300000000, 'upperBoundKrw', 500000000, 'ratePpm', 400000),
                JSON_OBJECT('lowerBoundKrw', 500000000, 'upperBoundKrw', 1000000000, 'ratePpm', 420000),
                JSON_OBJECT('lowerBoundKrw', 1000000000, 'upperBoundKrw', NULL, 'ratePpm', 450000)
            ),
            'localIncomeTaxBrackets', JSON_ARRAY(
                JSON_OBJECT('lowerBoundKrw', 0, 'upperBoundKrw', 14000000, 'ratePpm', 6000),
                JSON_OBJECT('lowerBoundKrw', 14000000, 'upperBoundKrw', 50000000, 'ratePpm', 15000),
                JSON_OBJECT('lowerBoundKrw', 50000000, 'upperBoundKrw', 88000000, 'ratePpm', 24000),
                JSON_OBJECT('lowerBoundKrw', 88000000, 'upperBoundKrw', 150000000, 'ratePpm', 35000),
                JSON_OBJECT('lowerBoundKrw', 150000000, 'upperBoundKrw', 300000000, 'ratePpm', 38000),
                JSON_OBJECT('lowerBoundKrw', 300000000, 'upperBoundKrw', 500000000, 'ratePpm', 40000),
                JSON_OBJECT('lowerBoundKrw', 500000000, 'upperBoundKrw', 1000000000, 'ratePpm', 42000),
                JSON_OBJECT('lowerBoundKrw', 1000000000, 'upperBoundKrw', NULL, 'ratePpm', 45000)
            ),
            'sourceRates', JSON_ARRAY(
                JSON_OBJECT('source', 'cmaInterest', 'incomeTaxRatePpm', 140000, 'localIncomeTaxRatePpm', 14000),
                JSON_OBJECT('source', 'depositInterest', 'incomeTaxRatePpm', 140000, 'localIncomeTaxRatePpm', 14000),
                JSON_OBJECT('source', 'bondCoupon', 'incomeTaxRatePpm', 140000, 'localIncomeTaxRatePpm', 14000),
                JSON_OBJECT('source', 'llxDistribution', 'incomeTaxRatePpm', 140000, 'localIncomeTaxRatePpm', 14000),
                JSON_OBJECT('source', 'isaEarlyClose', 'incomeTaxRatePpm', 140000, 'localIncomeTaxRatePpm', 14000)
            ),
            'filingDate', JSON_OBJECT('month', 5, 'day', 31)
        )
    );

-- The v2 policy cannot be sealed unless its strict annual rule is complete and every source rate
-- equals the pinned general-financial-income withholding rate.
CREATE TRIGGER tr_policy_set_v2_financial_rate_match
BEFORE UPDATE ON policy_set
FOR EACH ROW
SET NEW.policy_key = IF(
    BINARY NEW.policy_key <> BINARY 'kr-individual-2026-v2'
    OR NEW.sealed_at IS NULL
    OR EXISTS (
        SELECT 1
        FROM policy_rule AS general_rule
        INNER JOIN policy_rule AS annual_rule
            ON annual_rule.policy_set_id = general_rule.policy_set_id
           AND BINARY annual_rule.domain = BINARY 'tax'
           AND BINARY annual_rule.rule_key = BINARY 'annualFinancialIncomeAssessment'
        WHERE general_rule.policy_set_id = NEW.id
          AND BINARY general_rule.domain = BINARY 'tax'
          AND BINARY general_rule.rule_key = BINARY 'generalFinancialIncome'
          AND JSON_LENGTH(annual_rule.parameters) = 12
          AND JSON_LENGTH(JSON_EXTRACT(annual_rule.parameters, '$.filingDate')) = 2
          AND JSON_LENGTH(JSON_EXTRACT(annual_rule.parameters, '$.sourceRates')) = 5
          AND JSON_EXTRACT(annual_rule.parameters, '$.comprehensiveThresholdKrw')
              = JSON_EXTRACT(general_rule.parameters, '$.comprehensiveThresholdKrw')
          AND JSON_EXTRACT(annual_rule.parameters, '$.generalIncomeTaxRatePpm')
              = JSON_EXTRACT(general_rule.parameters, '$.incomeTaxPpm')
          AND JSON_EXTRACT(annual_rule.parameters, '$.generalLocalIncomeTaxRatePpm')
              = JSON_EXTRACT(general_rule.parameters, '$.localIncomeTaxPpm')
          AND JSON_EXTRACT(annual_rule.parameters, '$.nonFinancialComprehensiveIncomeKrw') = 0
          AND JSON_EXTRACT(annual_rule.parameters, '$.incomeTaxCreditKrw') = 0
          AND JSON_EXTRACT(annual_rule.parameters, '$.localIncomeTaxCreditKrw') = 0
          AND JSON_EXTRACT(annual_rule.parameters, '$.filingDate.month') = 5
          AND JSON_EXTRACT(annual_rule.parameters, '$.filingDate.day') = 31
          AND JSON_UNQUOTE(JSON_EXTRACT(annual_rule.parameters, '$.comparisonFormula'))
              = 'independentMaxOfFormulaAAndB'
          AND JSON_UNQUOTE(JSON_EXTRACT(annual_rule.parameters, '$.cashShortageTreatment'))
              = 'interestFreeAggregateDebt'
          AND JSON_UNQUOTE(JSON_EXTRACT(annual_rule.parameters, '$.sourceRates[0].source'))
              = 'cmaInterest'
          AND JSON_UNQUOTE(JSON_EXTRACT(annual_rule.parameters, '$.sourceRates[1].source'))
              = 'depositInterest'
          AND JSON_UNQUOTE(JSON_EXTRACT(annual_rule.parameters, '$.sourceRates[2].source'))
              = 'bondCoupon'
          AND JSON_UNQUOTE(JSON_EXTRACT(annual_rule.parameters, '$.sourceRates[3].source'))
              = 'llxDistribution'
          AND JSON_UNQUOTE(JSON_EXTRACT(annual_rule.parameters, '$.sourceRates[4].source'))
              = 'isaEarlyClose'
          AND JSON_EXTRACT(annual_rule.parameters, '$.sourceRates[0].incomeTaxRatePpm')
              = JSON_EXTRACT(general_rule.parameters, '$.incomeTaxPpm')
          AND JSON_EXTRACT(annual_rule.parameters, '$.sourceRates[1].incomeTaxRatePpm')
              = JSON_EXTRACT(general_rule.parameters, '$.incomeTaxPpm')
          AND JSON_EXTRACT(annual_rule.parameters, '$.sourceRates[2].incomeTaxRatePpm')
              = JSON_EXTRACT(general_rule.parameters, '$.incomeTaxPpm')
          AND JSON_EXTRACT(annual_rule.parameters, '$.sourceRates[3].incomeTaxRatePpm')
              = JSON_EXTRACT(general_rule.parameters, '$.incomeTaxPpm')
          AND JSON_EXTRACT(annual_rule.parameters, '$.sourceRates[4].incomeTaxRatePpm')
              = JSON_EXTRACT(general_rule.parameters, '$.incomeTaxPpm')
          AND JSON_EXTRACT(annual_rule.parameters, '$.sourceRates[0].localIncomeTaxRatePpm')
              = JSON_EXTRACT(general_rule.parameters, '$.localIncomeTaxPpm')
          AND JSON_EXTRACT(annual_rule.parameters, '$.sourceRates[1].localIncomeTaxRatePpm')
              = JSON_EXTRACT(general_rule.parameters, '$.localIncomeTaxPpm')
          AND JSON_EXTRACT(annual_rule.parameters, '$.sourceRates[2].localIncomeTaxRatePpm')
              = JSON_EXTRACT(general_rule.parameters, '$.localIncomeTaxPpm')
          AND JSON_EXTRACT(annual_rule.parameters, '$.sourceRates[3].localIncomeTaxRatePpm')
              = JSON_EXTRACT(general_rule.parameters, '$.localIncomeTaxPpm')
          AND JSON_EXTRACT(annual_rule.parameters, '$.sourceRates[4].localIncomeTaxRatePpm')
              = JSON_EXTRACT(general_rule.parameters, '$.localIncomeTaxPpm')
    ),
    NEW.policy_key,
    NULL
);

UPDATE policy_set
SET sealed_at = CURRENT_TIMESTAMP(3)
WHERE id = 2;

-- Existing runs keep their pinned v1-v3 world, NULL bundle, and v1 policy. Only new-run pointers move.
UPDATE market_world_assignment
SET world_id = 4
WHERE assignment_key = 'newRun';

UPDATE policy_set_assignment
SET policy_set_id = 2
WHERE assignment_key = 'newRun';
