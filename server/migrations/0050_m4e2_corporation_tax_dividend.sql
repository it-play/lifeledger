-- M4-E2c corporation year close, dividend, and month history (§9.1, §9.4).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;

INSERT INTO policy_source_document
    (source_key, source_url, checked_on, original_sha256)
VALUES (
    'law-local-corporate-income-tax-article-103-20-2026-07-01',
    'https://www.law.go.kr/LSW/lsLinkCommonInfo.do?chrClsCd=010202&lsJoLnkSeq=1029490341',
    '2026-07-29',
    '36804bbee34f991d28b778655fba794d7de36c58cc633194417748fba0426e5a'
);

CREATE TABLE corporation_tax_policy_bracket (
    policy_rule_id                     BIGINT UNSIGNED NOT NULL,
    tax_kind                           VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    bracket_order                      TINYINT UNSIGNED NOT NULL,
    maximum_tax_base_krw               BIGINT NULL,
    rate_ppm                           INT UNSIGNED NOT NULL,
    progressive_deduction_krw          BIGINT NOT NULL,
    policy_source_document_id          BIGINT UNSIGNED NOT NULL,
    created_at                         DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (policy_rule_id, tax_kind, bracket_order),
    CONSTRAINT fk_corporation_tax_bracket_rule
        FOREIGN KEY (policy_rule_id) REFERENCES policy_rule (id),
    CONSTRAINT fk_corporation_tax_bracket_source
        FOREIGN KEY (policy_source_document_id) REFERENCES policy_source_document (id),
    CONSTRAINT ck_corporation_tax_bracket CHECK (
        tax_kind IN ('national', 'local')
        AND bracket_order BETWEEN 1 AND 4
        AND (maximum_tax_base_krw IS NULL OR maximum_tax_base_krw > 0)
        AND rate_ppm BETWEEN 1 AND 1000000
        AND progressive_deduction_krw BETWEEN 0 AND 9007199254740991
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

INSERT INTO corporation_tax_policy_bracket
    (policy_rule_id, tax_kind, bracket_order, maximum_tax_base_krw,
     rate_ppm, progressive_deduction_krw, policy_source_document_id)
SELECT rule.id, bracket.tax_kind, bracket.bracket_order, bracket.maximum_tax_base_krw,
       bracket.rate_ppm, bracket.progressive_deduction_krw, source.id
FROM policy_rule AS rule
INNER JOIN policy_set AS policy ON policy.id = rule.policy_set_id
INNER JOIN (
    SELECT 'national' AS tax_kind, 1 AS bracket_order, 200000000 AS maximum_tax_base_krw,
           100000 AS rate_ppm, 0 AS progressive_deduction_krw,
           'nts-corporate-tax-rates-2026' AS source_key
    UNION ALL SELECT 'national', 2, 20000000000, 200000, 20000000,
           'nts-corporate-tax-rates-2026'
    UNION ALL SELECT 'national', 3, 300000000000, 220000, 420000000,
           'nts-corporate-tax-rates-2026'
    UNION ALL SELECT 'national', 4, NULL, 250000, 9420000000,
           'nts-corporate-tax-rates-2026'
    UNION ALL SELECT 'local', 1, 200000000, 10000, 0,
           'law-local-corporate-income-tax-article-103-20-2026-07-01'
    UNION ALL SELECT 'local', 2, 20000000000, 20000, 2000000,
           'law-local-corporate-income-tax-article-103-20-2026-07-01'
    UNION ALL SELECT 'local', 3, 300000000000, 22000, 39800000,
           'law-local-corporate-income-tax-article-103-20-2026-07-01'
    UNION ALL SELECT 'local', 4, NULL, 25000, 655800000,
           'law-local-corporate-income-tax-article-103-20-2026-07-01'
) AS bracket
INNER JOIN policy_source_document AS source ON source.source_key = bracket.source_key
WHERE policy.policy_key = 'dev-unranked-kr-corporation-2026-v5'
  AND rule.domain = 'corporation' AND rule.rule_key = 'corporateIncomeTax';

CREATE TRIGGER tr_corporation_tax_policy_bracket_no_update
BEFORE UPDATE ON corporation_tax_policy_bracket
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation tax brackets are immutable';

CREATE TRIGGER tr_corporation_tax_policy_bracket_no_delete
BEFORE DELETE ON corporation_tax_policy_bracket
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation tax brackets are immutable';

ALTER TABLE financial_income_source_year
    DROP CHECK ck_financial_income_source_year_source,
    ADD CONSTRAINT ck_financial_income_source_year_source CHECK (
        source IN (
            'cmaInterest', 'depositInterest', 'bondCoupon',
            'llxDistribution', 'isaEarlyClose', 'corporationDividend'
        )
    );

ALTER TABLE corporation
    ADD COLUMN corporate_tax_payable_krw BIGINT NOT NULL DEFAULT 0
        AFTER operating_payable_krw,
    DROP CHECK ck_corporation_establishment,
    ADD CONSTRAINT ck_corporation_establishment CHECK (
        registered_office_class = 'standardRegisteredOffice'
        AND capital_krw BETWEEN 1000000 AND 1000000000
        AND registration_license_tax_krw > 0
        AND local_education_tax_krw >= 0
        AND game_administrative_fee_krw >= 0
        AND total_establishment_fee_krw
            = registration_license_tax_krw + local_education_tax_krw + game_administrative_fee_krw
        AND cash_krw BETWEEN 0 AND 9007199254740991
        AND contributed_capital_krw = capital_krw
        AND retained_earnings_krw BETWEEN -9007199254740991 AND 9007199254740991
        AND operating_payable_krw BETWEEN 0 AND 9007199254740991
        AND corporate_tax_payable_krw BETWEEN 0 AND 9007199254740991
        AND distributable_profit_krw BETWEEN 0 AND 9007199254740991
        AND (
            (status = 'draft'
             AND personal_ledger_transaction_id IS NULL
             AND corporation_ledger_transaction_id IS NULL)
            OR (status <> 'draft'
                AND personal_ledger_transaction_id IS NOT NULL
                AND corporation_ledger_transaction_id IS NOT NULL)
        )
    );

CREATE TABLE corporation_tax_year (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED NOT NULL,
    corporation_id                      BIGINT UNSIGNED NOT NULL,
    policy_rule_id                      BIGINT UNSIGNED NOT NULL,
    tax_year                            SMALLINT UNSIGNED NOT NULL,
    annual_pre_tax_profit_krw           BIGINT NOT NULL,
    tax_base_krw                        BIGINT NOT NULL,
    corporate_income_tax_krw            BIGINT NOT NULL,
    local_corporate_income_tax_krw      BIGINT NOT NULL,
    total_tax_krw                       BIGINT NOT NULL,
    retained_earnings_before_krw        BIGINT NOT NULL,
    retained_earnings_after_krw         BIGINT NOT NULL,
    corporate_tax_payable_before_krw    BIGINT NOT NULL,
    corporate_tax_payable_after_krw     BIGINT NOT NULL,
    distributable_profit_after_krw      BIGINT NOT NULL,
    ledger_transaction_id               BIGINT UNSIGNED NULL,
    applied_game_day                    INT UNSIGNED NOT NULL,
    status                              VARCHAR(12) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    applied_at                          DATETIME(3) NULL,
    PRIMARY KEY (id),
    UNIQUE KEY uk_corporation_tax_year
        (save_id, run_revision, corporation_id, tax_year),
    UNIQUE KEY uk_corporation_tax_year_scope
        (save_id, run_revision, corporation_id, id),
    UNIQUE KEY uk_corporation_tax_year_ledger (ledger_transaction_id),
    CONSTRAINT fk_corporation_tax_year_corporation
        FOREIGN KEY (save_id, run_revision, corporation_id)
        REFERENCES corporation (save_id, run_revision, id),
    CONSTRAINT fk_corporation_tax_year_policy_rule
        FOREIGN KEY (policy_rule_id) REFERENCES policy_rule (id),
    CONSTRAINT ck_corporation_tax_year CHECK (
        tax_year BETWEEN 1 AND 9999
        AND tax_base_krw BETWEEN 0 AND 9007199254740991
        AND corporate_income_tax_krw BETWEEN 0 AND 9007199254740991
        AND local_corporate_income_tax_krw BETWEEN 0 AND 9007199254740991
        AND total_tax_krw = corporate_income_tax_krw + local_corporate_income_tax_krw
        AND corporate_tax_payable_before_krw BETWEEN 0 AND 9007199254740991
        AND corporate_tax_payable_after_krw
            = corporate_tax_payable_before_krw + total_tax_krw
        AND distributable_profit_after_krw BETWEEN 0 AND 9007199254740991
        AND status IN ('preparing', 'applied')
        AND ((status = 'preparing' AND ledger_transaction_id IS NULL AND applied_at IS NULL)
             OR (status = 'applied'
                 AND (total_tax_krw = 0 OR ledger_transaction_id IS NOT NULL)
                 AND (total_tax_krw > 0 OR ledger_transaction_id IS NULL)
                 AND applied_at IS NOT NULL))
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE corporation_dividend (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED NOT NULL,
    corporation_id                      BIGINT UNSIGNED NOT NULL,
    tax_year                            SMALLINT UNSIGNED NOT NULL,
    policy_rule_id                      BIGINT UNSIGNED NOT NULL,
    command_id                          CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    gross_dividend_krw                  BIGINT NOT NULL,
    withheld_income_tax_krw             BIGINT NOT NULL,
    withheld_local_income_tax_krw       BIGINT NOT NULL,
    net_dividend_krw                    BIGINT NOT NULL,
    cash_before_krw                     BIGINT NOT NULL,
    cash_after_krw                      BIGINT NOT NULL,
    retained_earnings_before_krw        BIGINT NOT NULL,
    retained_earnings_after_krw         BIGINT NOT NULL,
    distributable_profit_before_krw     BIGINT NOT NULL,
    distributable_profit_after_krw      BIGINT NOT NULL,
    corporation_ledger_transaction_id   BIGINT UNSIGNED NULL,
    personal_ledger_transaction_id      BIGINT UNSIGNED NULL,
    paid_game_day                       INT UNSIGNED NOT NULL,
    status                              VARCHAR(12) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    applied_at                          DATETIME(3) NULL,
    PRIMARY KEY (id),
    UNIQUE KEY uk_corporation_dividend_command (save_id, command_id),
    UNIQUE KEY uk_corporation_dividend_scope
        (save_id, run_revision, corporation_id, id),
    UNIQUE KEY uk_corporation_dividend_corp_ledger (corporation_ledger_transaction_id),
    UNIQUE KEY uk_corporation_dividend_personal_ledger (personal_ledger_transaction_id),
    CONSTRAINT fk_corporation_dividend_corporation
        FOREIGN KEY (save_id, run_revision, corporation_id)
        REFERENCES corporation (save_id, run_revision, id),
    CONSTRAINT fk_corporation_dividend_policy_rule
        FOREIGN KEY (policy_rule_id) REFERENCES policy_rule (id),
    CONSTRAINT fk_corporation_dividend_command
        FOREIGN KEY (save_id, command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT ck_corporation_dividend CHECK (
        tax_year BETWEEN 1 AND 9999
        AND gross_dividend_krw BETWEEN 1 AND 9007199254740991
        AND withheld_income_tax_krw BETWEEN 0 AND gross_dividend_krw
        AND withheld_local_income_tax_krw BETWEEN 0 AND gross_dividend_krw
        AND net_dividend_krw
            = gross_dividend_krw - withheld_income_tax_krw - withheld_local_income_tax_krw
        AND cash_before_krw BETWEEN gross_dividend_krw AND 9007199254740991
        AND cash_after_krw = cash_before_krw - net_dividend_krw
        AND retained_earnings_after_krw
            = retained_earnings_before_krw - gross_dividend_krw
        AND distributable_profit_before_krw >= gross_dividend_krw
        AND distributable_profit_after_krw
            = distributable_profit_before_krw - gross_dividend_krw
        AND status IN ('preparing', 'applied')
        AND ((status = 'preparing'
              AND corporation_ledger_transaction_id IS NULL
              AND personal_ledger_transaction_id IS NULL AND applied_at IS NULL)
             OR (status = 'applied'
                 AND corporation_ledger_transaction_id IS NOT NULL
                 AND personal_ledger_transaction_id IS NOT NULL AND applied_at IS NOT NULL))
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE corporation_dividend_command_receipt (
    save_id                         BIGINT UNSIGNED NOT NULL,
    command_id                      CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    state_revision                  BIGINT UNSIGNED NOT NULL,
    game_day                        INT UNSIGNED NOT NULL,
    corporation_id                  BIGINT UNSIGNED NOT NULL,
    payload_sha256                  CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    result                          JSON NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, command_id),
    CONSTRAINT fk_corporation_dividend_receipt_command
        FOREIGN KEY (save_id, command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT fk_corporation_dividend_receipt_corporation
        FOREIGN KEY (save_id, run_revision, corporation_id)
        REFERENCES corporation (save_id, run_revision, id),
    CONSTRAINT ck_corporation_dividend_receipt CHECK (
        JSON_VALID(result)
        AND JSON_LENGTH(result) = 12
        AND JSON_UNQUOTE(JSON_EXTRACT(result, '$.commandId')) = command_id
        AND CAST(JSON_UNQUOTE(JSON_EXTRACT(result, '$.corporationId')) AS UNSIGNED)
            = corporation_id
        AND JSON_EXTRACT(result, '$.replayed') = FALSE
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

ALTER TABLE corporation_ledger_transaction
    ADD COLUMN corporation_tax_year_id BIGINT UNSIGNED NULL AFTER operating_month_id,
    ADD COLUMN corporation_dividend_id BIGINT UNSIGNED NULL AFTER corporation_tax_year_id,
    ADD UNIQUE KEY uk_corporation_ledger_tax_year
        (save_id, run_revision, corporation_id, transaction_kind, corporation_tax_year_id),
    ADD UNIQUE KEY uk_corporation_ledger_dividend
        (save_id, run_revision, corporation_id, transaction_kind, corporation_dividend_id),
    ADD CONSTRAINT fk_corporation_ledger_tax_year
        FOREIGN KEY (save_id, run_revision, corporation_id, corporation_tax_year_id)
        REFERENCES corporation_tax_year (save_id, run_revision, corporation_id, id),
    ADD CONSTRAINT fk_corporation_ledger_dividend
        FOREIGN KEY (save_id, run_revision, corporation_id, corporation_dividend_id)
        REFERENCES corporation_dividend (save_id, run_revision, corporation_id, id),
    DROP CHECK ck_corporation_ledger_transaction,
    ADD CONSTRAINT ck_corporation_ledger_transaction CHECK (
        transaction_kind IN (
            'establishment', 'monthlyRevenue', 'monthlyExpense',
            'officerPayroll', 'corporateTax', 'dividend'
        )
        AND CHAR_LENGTH(description) BETWEEN 1 AND 255
        AND (
            (transaction_kind IN ('monthlyRevenue', 'monthlyExpense', 'officerPayroll')
             AND correlation_id IS NULL AND operating_month_id IS NOT NULL
             AND corporation_tax_year_id IS NULL AND corporation_dividend_id IS NULL)
            OR (transaction_kind = 'corporateTax'
                AND correlation_id IS NULL AND operating_month_id IS NULL
                AND corporation_tax_year_id IS NOT NULL AND corporation_dividend_id IS NULL)
            OR (transaction_kind = 'dividend'
                AND correlation_id IS NOT NULL AND operating_month_id IS NULL
                AND corporation_tax_year_id IS NULL AND corporation_dividend_id IS NOT NULL)
            OR (transaction_kind = 'establishment'
                AND correlation_id IS NOT NULL AND operating_month_id IS NULL
                AND corporation_tax_year_id IS NULL AND corporation_dividend_id IS NULL)
        )
    );

ALTER TABLE corporation_tax_year
    ADD CONSTRAINT fk_corporation_tax_year_ledger
        FOREIGN KEY (save_id, run_revision, corporation_id, ledger_transaction_id)
        REFERENCES corporation_ledger_transaction (save_id, run_revision, corporation_id, id);

ALTER TABLE corporation_dividend
    ADD CONSTRAINT fk_corporation_dividend_corp_ledger
        FOREIGN KEY (save_id, run_revision, corporation_id, corporation_ledger_transaction_id)
        REFERENCES corporation_ledger_transaction (save_id, run_revision, corporation_id, id),
    ADD CONSTRAINT fk_corporation_dividend_personal_ledger
        FOREIGN KEY (save_id, run_revision, personal_ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id);

DROP TRIGGER tr_ledger_transaction_corporation_source_insert;
CREATE TRIGGER tr_ledger_transaction_corporation_source_insert
BEFORE INSERT ON ledger_transaction
FOR EACH ROW
FOLLOWS tr_ledger_transaction_insolvency_source_insert
SET NEW.source_kind = IF(
    NEW.source_kind NOT LIKE 'corporation%'
    OR (NEW.source_kind = 'corporationEstablishment' AND EXISTS (
        SELECT 1 FROM corporation AS corporation_row
        INNER JOIN run_rule_bundle AS bundle
          ON bundle.save_id = corporation_row.save_id
         AND bundle.run_revision = corporation_row.run_revision
        WHERE BINARY corporation_row.establishment_command_id = BINARY NEW.source_id
          AND corporation_row.save_id = NEW.save_id
          AND corporation_row.run_revision = NEW.run_revision
          AND corporation_row.status = 'draft'
          AND bundle.policy_set_id = NEW.policy_set_id
    ))
    OR (NEW.source_kind = 'corporationOfficerPayroll'
        AND NEW.source_id REGEXP '^[1-9][0-9]*$' AND EXISTS (
        SELECT 1 FROM corporation_operating_month AS operating_month
        INNER JOIN corporation AS corporation_row
          ON corporation_row.id = operating_month.corporation_id
         AND corporation_row.save_id = operating_month.save_id
         AND corporation_row.run_revision = operating_month.run_revision
        INNER JOIN run_rule_bundle AS bundle
          ON bundle.save_id = operating_month.save_id
         AND bundle.run_revision = operating_month.run_revision
        WHERE operating_month.id = CAST(NEW.source_id AS UNSIGNED)
          AND operating_month.save_id = NEW.save_id
          AND operating_month.run_revision = NEW.run_revision
          AND operating_month.payroll_status = 'paid'
          AND operating_month.status = 'preparing'
          AND corporation_row.status = 'active'
          AND bundle.policy_set_id = NEW.policy_set_id
          AND operating_month.applied_game_day = NEW.game_day
    ))
    OR (NEW.source_kind = 'corporationDividend'
        AND NEW.source_id REGEXP '^[1-9][0-9]*$' AND EXISTS (
        SELECT 1 FROM corporation_dividend AS dividend
        INNER JOIN corporation AS corporation_row
          ON corporation_row.id = dividend.corporation_id
         AND corporation_row.save_id = dividend.save_id
         AND corporation_row.run_revision = dividend.run_revision
        INNER JOIN run_rule_bundle AS bundle
          ON bundle.save_id = dividend.save_id AND bundle.run_revision = dividend.run_revision
        WHERE dividend.id = CAST(NEW.source_id AS UNSIGNED)
          AND dividend.save_id = NEW.save_id AND dividend.run_revision = NEW.run_revision
          AND dividend.status = 'preparing' AND corporation_row.status = 'active'
          AND bundle.policy_set_id = NEW.policy_set_id
          AND dividend.paid_game_day = NEW.game_day
    )),
    NEW.source_kind,
    NULL
);

DROP TRIGGER tr_ledger_posting_corporation_reference_insert;
CREATE TRIGGER tr_ledger_posting_corporation_reference_insert
BEFORE INSERT ON ledger_posting
FOR EACH ROW
FOLLOWS tr_ledger_posting_insolvency_reference_insert
SET NEW.account_code = IF(
    (NEW.corporation_id IS NULL AND NOT EXISTS (
        SELECT 1 FROM ledger_transaction AS ledger
        WHERE ledger.id = NEW.ledger_transaction_id
          AND ledger.save_id = NEW.save_id AND ledger.run_revision = NEW.run_revision
          AND ledger.source_kind IN (
              'corporationEstablishment', 'corporationOfficerPayroll', 'corporationDividend'
          )
    ))
    OR EXISTS (
        SELECT 1 FROM ledger_transaction AS ledger
        INNER JOIN corporation AS corporation_row
          ON corporation_row.save_id = ledger.save_id
         AND corporation_row.run_revision = ledger.run_revision
         AND BINARY corporation_row.establishment_command_id = BINARY ledger.source_id
        WHERE ledger.id = NEW.ledger_transaction_id
          AND ledger.save_id = NEW.save_id AND ledger.run_revision = NEW.run_revision
          AND ledger.source_kind = 'corporationEstablishment'
          AND corporation_row.id = NEW.corporation_id AND corporation_row.status = 'draft'
          AND ((NEW.posting_order = 1 AND NEW.account_code = 'corporationInvestmentAsset'
                AND NEW.amount_krw = corporation_row.capital_krw)
               OR (NEW.posting_order = 2 AND NEW.account_code = 'corporationRegistrationExpense'
                   AND NEW.amount_krw = corporation_row.total_establishment_fee_krw)
               OR (NEW.posting_order = 3 AND NEW.account_code = 'wallet'
                   AND NEW.amount_krw = -(corporation_row.capital_krw
                                         + corporation_row.total_establishment_fee_krw)))
    )
    OR EXISTS (
        SELECT 1 FROM ledger_transaction AS ledger
        INNER JOIN corporation_operating_month AS operating_month
          ON operating_month.id = CAST(ledger.source_id AS UNSIGNED)
         AND operating_month.save_id = ledger.save_id
         AND operating_month.run_revision = ledger.run_revision
        WHERE ledger.id = NEW.ledger_transaction_id
          AND ledger.save_id = NEW.save_id AND ledger.run_revision = NEW.run_revision
          AND ledger.source_kind = 'corporationOfficerPayroll'
          AND operating_month.corporation_id = NEW.corporation_id
          AND operating_month.payroll_status = 'paid' AND operating_month.status = 'preparing'
          AND NEW.account_code IN (
              'wallet', 'salaryIncome', 'employeeNationalPensionExpense',
              'employeeHealthInsuranceExpense', 'employeeLongTermCareExpense',
              'employeeEmploymentInsuranceExpense', 'employmentIncomeTaxWithholding',
              'employmentLocalIncomeTaxWithholding'
          )
    )
    OR EXISTS (
        SELECT 1 FROM ledger_transaction AS ledger
        INNER JOIN corporation_dividend AS dividend
          ON dividend.id = CAST(ledger.source_id AS UNSIGNED)
         AND dividend.save_id = ledger.save_id AND dividend.run_revision = ledger.run_revision
        WHERE ledger.id = NEW.ledger_transaction_id
          AND ledger.save_id = NEW.save_id AND ledger.run_revision = NEW.run_revision
          AND ledger.source_kind = 'corporationDividend'
          AND dividend.corporation_id = NEW.corporation_id AND dividend.status = 'preparing'
          AND ((NEW.posting_order = 1 AND NEW.account_code = 'wallet'
                AND NEW.amount_krw = dividend.net_dividend_krw)
               OR (NEW.posting_order = 2 AND NEW.account_code = 'distributionIncome'
                   AND NEW.amount_krw = -dividend.gross_dividend_krw)
               OR (NEW.posting_order = 3 AND NEW.account_code = 'withholdingTaxLiability'
                   AND NEW.amount_krw = dividend.withheld_income_tax_krw
                       + dividend.withheld_local_income_tax_krw))
    ),
    NEW.account_code,
    NULL
);

DROP TRIGGER tr_corporation_ledger_transaction_insert_valid;
CREATE TRIGGER tr_corporation_ledger_transaction_insert_valid
BEFORE INSERT ON corporation_ledger_transaction
FOR EACH ROW
SET NEW.transaction_kind = IF(
    (NEW.transaction_kind = 'establishment' AND EXISTS (
        SELECT 1 FROM corporation AS corporation_row
        WHERE corporation_row.id = NEW.corporation_id
          AND corporation_row.save_id = NEW.save_id
          AND corporation_row.run_revision = NEW.run_revision
          AND corporation_row.status = 'draft'
          AND corporation_row.established_game_day = NEW.game_day
          AND BINARY corporation_row.establishment_command_id = BINARY NEW.correlation_id
    ))
    OR (NEW.transaction_kind IN ('monthlyRevenue', 'monthlyExpense', 'officerPayroll')
        AND EXISTS (
        SELECT 1 FROM corporation_operating_month AS operating_month
        WHERE operating_month.id = NEW.operating_month_id
          AND operating_month.save_id = NEW.save_id
          AND operating_month.run_revision = NEW.run_revision
          AND operating_month.corporation_id = NEW.corporation_id
          AND operating_month.applied_game_day = NEW.game_day
          AND operating_month.status = 'preparing'
          AND (NEW.transaction_kind <> 'officerPayroll'
               OR operating_month.officer_gross_salary_krw > 0)
    ))
    OR (NEW.transaction_kind = 'corporateTax' AND EXISTS (
        SELECT 1 FROM corporation_tax_year AS tax_year
        WHERE tax_year.id = NEW.corporation_tax_year_id
          AND tax_year.save_id = NEW.save_id AND tax_year.run_revision = NEW.run_revision
          AND tax_year.corporation_id = NEW.corporation_id
          AND tax_year.applied_game_day = NEW.game_day AND tax_year.status = 'preparing'
    ))
    OR (NEW.transaction_kind = 'dividend' AND EXISTS (
        SELECT 1 FROM corporation_dividend AS dividend
        WHERE dividend.id = NEW.corporation_dividend_id
          AND dividend.save_id = NEW.save_id AND dividend.run_revision = NEW.run_revision
          AND dividend.corporation_id = NEW.corporation_id
          AND BINARY dividend.command_id = BINARY NEW.correlation_id
          AND dividend.paid_game_day = NEW.game_day AND dividend.status = 'preparing'
    )),
    NEW.transaction_kind,
    NULL
);

DROP TRIGGER tr_corporation_ledger_posting_insert_valid;
CREATE TRIGGER tr_corporation_ledger_posting_insert_valid
BEFORE INSERT ON corporation_ledger_posting
FOR EACH ROW
SET NEW.account_code = IF(
    EXISTS (
        SELECT 1 FROM corporation_ledger_transaction AS ledger
        INNER JOIN corporation AS corporation_row
          ON corporation_row.id = ledger.corporation_id
         AND corporation_row.save_id = ledger.save_id
         AND corporation_row.run_revision = ledger.run_revision
        WHERE ledger.id = NEW.corporation_ledger_transaction_id
          AND ledger.save_id = NEW.save_id AND ledger.run_revision = NEW.run_revision
          AND ledger.corporation_id = NEW.corporation_id
          AND ledger.transaction_kind = 'establishment' AND corporation_row.status = 'draft'
          AND ((NEW.posting_order = 1 AND NEW.account_code = 'corporationCash'
                AND NEW.amount_krw = corporation_row.capital_krw)
               OR (NEW.posting_order = 2 AND NEW.account_code = 'contributedCapital'
                   AND NEW.amount_krw = -corporation_row.capital_krw))
    )
    OR EXISTS (
        SELECT 1 FROM corporation_ledger_transaction AS ledger
        INNER JOIN corporation_operating_month AS operating_month
          ON operating_month.id = ledger.operating_month_id
         AND operating_month.save_id = ledger.save_id
         AND operating_month.run_revision = ledger.run_revision
         AND operating_month.corporation_id = ledger.corporation_id
        WHERE ledger.id = NEW.corporation_ledger_transaction_id
          AND ledger.save_id = NEW.save_id AND ledger.run_revision = NEW.run_revision
          AND ledger.corporation_id = NEW.corporation_id
          AND operating_month.status = 'preparing'
          AND ((ledger.transaction_kind = 'monthlyRevenue'
                AND NEW.account_code IN ('corporationCash', 'operatingRevenue'))
               OR (ledger.transaction_kind = 'monthlyExpense'
                   AND NEW.account_code IN (
                       'variableCostExpense', 'fixedCostExpense',
                       'corporationCash', 'operatingPayable'
                   ))
               OR (ledger.transaction_kind = 'officerPayroll'
                   AND NEW.account_code IN (
                       'officerPayrollExpense', 'corporationCash',
                       'withholdingTaxLiability', 'operatingPayable'
                   )))
    )
    OR EXISTS (
        SELECT 1 FROM corporation_ledger_transaction AS ledger
        INNER JOIN corporation_tax_year AS tax_year
          ON tax_year.id = ledger.corporation_tax_year_id
         AND tax_year.save_id = ledger.save_id AND tax_year.run_revision = ledger.run_revision
         AND tax_year.corporation_id = ledger.corporation_id
        WHERE ledger.id = NEW.corporation_ledger_transaction_id
          AND ledger.save_id = NEW.save_id AND ledger.run_revision = NEW.run_revision
          AND ledger.corporation_id = NEW.corporation_id
          AND ledger.transaction_kind = 'corporateTax' AND tax_year.status = 'preparing'
          AND ((NEW.posting_order = 1 AND NEW.account_code = 'corporateTaxExpense'
                AND NEW.amount_krw = tax_year.total_tax_krw)
               OR (NEW.posting_order = 2 AND NEW.account_code = 'corporateTaxPayable'
                   AND NEW.amount_krw = -tax_year.total_tax_krw))
    )
    OR EXISTS (
        SELECT 1 FROM corporation_ledger_transaction AS ledger
        INNER JOIN corporation_dividend AS dividend
          ON dividend.id = ledger.corporation_dividend_id
         AND dividend.save_id = ledger.save_id AND dividend.run_revision = ledger.run_revision
         AND dividend.corporation_id = ledger.corporation_id
        WHERE ledger.id = NEW.corporation_ledger_transaction_id
          AND ledger.save_id = NEW.save_id AND ledger.run_revision = NEW.run_revision
          AND ledger.corporation_id = NEW.corporation_id
          AND ledger.transaction_kind = 'dividend' AND dividend.status = 'preparing'
          AND ((NEW.posting_order = 1 AND NEW.account_code = 'dividendDistribution'
                AND NEW.amount_krw = dividend.gross_dividend_krw)
               OR (NEW.posting_order = 2 AND NEW.account_code = 'corporationCash'
                   AND NEW.amount_krw = -dividend.net_dividend_krw)
               OR (NEW.posting_order = 3 AND NEW.account_code = 'withholdingTaxLiability'
                   AND NEW.amount_krw = -(dividend.withheld_income_tax_krw
                                          + dividend.withheld_local_income_tax_krw)))
    ),
    NEW.account_code,
    NULL
);

DROP TRIGGER tr_corporation_status_transition_only;
CREATE TRIGGER tr_corporation_status_transition_only
BEFORE UPDATE ON corporation
FOR EACH ROW
SET NEW.id = IF(
    NEW.id = OLD.id AND NEW.save_id = OLD.save_id AND NEW.run_revision = OLD.run_revision
    AND NEW.life_catalog_set_id = OLD.life_catalog_set_id
    AND NEW.policy_set_id = OLD.policy_set_id
    AND NEW.corporation_component_version_id = OLD.corporation_component_version_id
    AND NEW.industry_template_id = OLD.industry_template_id
    AND NEW.registration_policy_rule_id = OLD.registration_policy_rule_id
    AND BINARY NEW.name = BINARY OLD.name
    AND BINARY NEW.representative_name = BINARY OLD.representative_name
    AND BINARY NEW.registered_office_class = BINARY OLD.registered_office_class
    AND BINARY NEW.establishment_command_id = BINARY OLD.establishment_command_id
    AND NEW.established_game_day = OLD.established_game_day
    AND NEW.capital_krw = OLD.capital_krw
    AND NEW.registration_license_tax_krw = OLD.registration_license_tax_krw
    AND NEW.local_education_tax_krw = OLD.local_education_tax_krw
    AND NEW.game_administrative_fee_krw = OLD.game_administrative_fee_krw
    AND NEW.total_establishment_fee_krw = OLD.total_establishment_fee_krw
    AND NEW.contributed_capital_krw = OLD.contributed_capital_krw
    AND NEW.created_at = OLD.created_at
    AND (
        (OLD.status = 'draft' AND NEW.status = 'active'
         AND NEW.cash_krw = OLD.capital_krw AND NEW.retained_earnings_krw = 0
         AND NEW.operating_payable_krw = 0 AND NEW.corporate_tax_payable_krw = 0
         AND NEW.distributable_profit_krw = 0
         AND NEW.personal_ledger_transaction_id IS NOT NULL
         AND NEW.corporation_ledger_transaction_id IS NOT NULL)
        OR (OLD.status = 'active' AND NEW.status IN ('active', 'insolvent')
            AND NEW.personal_ledger_transaction_id = OLD.personal_ledger_transaction_id
            AND NEW.corporation_ledger_transaction_id = OLD.corporation_ledger_transaction_id
            AND NEW.corporate_tax_payable_krw = OLD.corporate_tax_payable_krw
            AND NEW.distributable_profit_krw = OLD.distributable_profit_krw
            AND EXISTS (
                SELECT 1 FROM corporation_operating_month AS operating_month
                WHERE operating_month.save_id = OLD.save_id
                  AND operating_month.run_revision = OLD.run_revision
                  AND operating_month.corporation_id = OLD.id
                  AND operating_month.status = 'preparing'
                  AND operating_month.cash_before_krw = OLD.cash_krw
                  AND operating_month.cash_after_krw = NEW.cash_krw
                  AND operating_month.operating_payable_before_krw = OLD.operating_payable_krw
                  AND operating_month.operating_payable_after_krw = NEW.operating_payable_krw
                  AND operating_month.retained_earnings_before_krw = OLD.retained_earnings_krw
                  AND operating_month.retained_earnings_after_krw = NEW.retained_earnings_krw
                  AND NEW.status = IF(
                      operating_month.operating_cost_payable_krw > 0
                          OR operating_month.payroll_payable_krw > 0,
                      'insolvent', 'active'
                  )
            ))
        OR (OLD.status IN ('active', 'insolvent') AND NEW.status = OLD.status
            AND NEW.personal_ledger_transaction_id = OLD.personal_ledger_transaction_id
            AND NEW.corporation_ledger_transaction_id = OLD.corporation_ledger_transaction_id
            AND NEW.cash_krw = OLD.cash_krw
            AND NEW.operating_payable_krw = OLD.operating_payable_krw
            AND EXISTS (
                SELECT 1 FROM corporation_tax_year AS tax_year
                WHERE tax_year.save_id = OLD.save_id AND tax_year.run_revision = OLD.run_revision
                  AND tax_year.corporation_id = OLD.id AND tax_year.status = 'preparing'
                  AND tax_year.retained_earnings_before_krw = OLD.retained_earnings_krw
                  AND tax_year.retained_earnings_after_krw = NEW.retained_earnings_krw
                  AND tax_year.corporate_tax_payable_before_krw = OLD.corporate_tax_payable_krw
                  AND tax_year.corporate_tax_payable_after_krw = NEW.corporate_tax_payable_krw
                  AND tax_year.distributable_profit_after_krw = NEW.distributable_profit_krw
            ))
        OR (OLD.status = 'active' AND NEW.status = 'active'
            AND NEW.personal_ledger_transaction_id = OLD.personal_ledger_transaction_id
            AND NEW.corporation_ledger_transaction_id = OLD.corporation_ledger_transaction_id
            AND NEW.operating_payable_krw = OLD.operating_payable_krw
            AND NEW.corporate_tax_payable_krw = OLD.corporate_tax_payable_krw
            AND EXISTS (
                SELECT 1 FROM corporation_dividend AS dividend
                WHERE dividend.save_id = OLD.save_id AND dividend.run_revision = OLD.run_revision
                  AND dividend.corporation_id = OLD.id AND dividend.status = 'preparing'
                  AND dividend.cash_before_krw = OLD.cash_krw
                  AND dividend.cash_after_krw = NEW.cash_krw
                  AND dividend.retained_earnings_before_krw = OLD.retained_earnings_krw
                  AND dividend.retained_earnings_after_krw = NEW.retained_earnings_krw
                  AND dividend.distributable_profit_before_krw = OLD.distributable_profit_krw
                  AND dividend.distributable_profit_after_krw = NEW.distributable_profit_krw
            ))
    ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_corporation_tax_year_apply_only
BEFORE UPDATE ON corporation_tax_year
FOR EACH ROW
SET NEW.status = IF(
    OLD.status = 'preparing' AND NEW.status = 'applied'
    AND NEW.id = OLD.id AND NEW.save_id = OLD.save_id
    AND NEW.run_revision = OLD.run_revision AND NEW.corporation_id = OLD.corporation_id
    AND NEW.policy_rule_id = OLD.policy_rule_id AND NEW.tax_year = OLD.tax_year
    AND NEW.annual_pre_tax_profit_krw = OLD.annual_pre_tax_profit_krw
    AND NEW.tax_base_krw = OLD.tax_base_krw
    AND NEW.corporate_income_tax_krw = OLD.corporate_income_tax_krw
    AND NEW.local_corporate_income_tax_krw = OLD.local_corporate_income_tax_krw
    AND NEW.total_tax_krw = OLD.total_tax_krw
    AND NEW.retained_earnings_before_krw = OLD.retained_earnings_before_krw
    AND NEW.retained_earnings_after_krw = OLD.retained_earnings_after_krw
    AND NEW.corporate_tax_payable_before_krw = OLD.corporate_tax_payable_before_krw
    AND NEW.corporate_tax_payable_after_krw = OLD.corporate_tax_payable_after_krw
    AND NEW.distributable_profit_after_krw = OLD.distributable_profit_after_krw
    AND NEW.applied_game_day = OLD.applied_game_day AND NEW.created_at = OLD.created_at
    AND OLD.ledger_transaction_id IS NULL
    AND ((NEW.total_tax_krw = 0 AND NEW.ledger_transaction_id IS NULL)
         OR (NEW.total_tax_krw > 0 AND NEW.ledger_transaction_id IS NOT NULL))
    AND OLD.applied_at IS NULL AND NEW.applied_at IS NOT NULL,
    NEW.status,
    NULL
);

CREATE TRIGGER tr_corporation_tax_year_no_delete
BEFORE DELETE ON corporation_tax_year
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation tax years are immutable';

CREATE TRIGGER tr_corporation_dividend_apply_only
BEFORE UPDATE ON corporation_dividend
FOR EACH ROW
SET NEW.status = IF(
    OLD.status = 'preparing' AND NEW.status = 'applied'
    AND NEW.id = OLD.id AND NEW.save_id = OLD.save_id
    AND NEW.run_revision = OLD.run_revision AND NEW.corporation_id = OLD.corporation_id
    AND NEW.tax_year = OLD.tax_year AND NEW.policy_rule_id = OLD.policy_rule_id
    AND BINARY NEW.command_id = BINARY OLD.command_id
    AND NEW.gross_dividend_krw = OLD.gross_dividend_krw
    AND NEW.withheld_income_tax_krw = OLD.withheld_income_tax_krw
    AND NEW.withheld_local_income_tax_krw = OLD.withheld_local_income_tax_krw
    AND NEW.net_dividend_krw = OLD.net_dividend_krw
    AND NEW.cash_before_krw = OLD.cash_before_krw AND NEW.cash_after_krw = OLD.cash_after_krw
    AND NEW.retained_earnings_before_krw = OLD.retained_earnings_before_krw
    AND NEW.retained_earnings_after_krw = OLD.retained_earnings_after_krw
    AND NEW.distributable_profit_before_krw = OLD.distributable_profit_before_krw
    AND NEW.distributable_profit_after_krw = OLD.distributable_profit_after_krw
    AND NEW.paid_game_day = OLD.paid_game_day AND NEW.created_at = OLD.created_at
    AND OLD.corporation_ledger_transaction_id IS NULL
    AND OLD.personal_ledger_transaction_id IS NULL
    AND NEW.corporation_ledger_transaction_id IS NOT NULL
    AND NEW.personal_ledger_transaction_id IS NOT NULL
    AND OLD.applied_at IS NULL AND NEW.applied_at IS NOT NULL,
    NEW.status,
    NULL
);

CREATE TRIGGER tr_corporation_dividend_no_delete
BEFORE DELETE ON corporation_dividend
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation dividends are immutable';

CREATE TRIGGER tr_corporation_dividend_receipt_no_update
BEFORE UPDATE ON corporation_dividend_command_receipt
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation dividend receipts are immutable';

CREATE TRIGGER tr_corporation_dividend_receipt_no_delete
BEFORE DELETE ON corporation_dividend_command_receipt
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation dividend receipts are immutable';
