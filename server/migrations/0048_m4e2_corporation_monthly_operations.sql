-- M4-E2b: deterministic monthly corporation revenue and operating expenses.

ALTER TABLE corporation_operating_scale
    ADD UNIQUE KEY uk_corporation_scale_version_template_id
        (life_component_version_id, industry_template_id, id);

CREATE TABLE corporation_operating_month (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED NOT NULL,
    corporation_id                      BIGINT UNSIGNED NOT NULL,
    corporation_component_version_id    BIGINT UNSIGNED NOT NULL,
    industry_template_id                BIGINT UNSIGNED NOT NULL,
    operating_scale_id                  BIGINT UNSIGNED NOT NULL,
    operating_year                      SMALLINT UNSIGNED NOT NULL,
    operating_month                     TINYINT UNSIGNED NOT NULL,
    entropy_stream                      INT UNSIGNED NOT NULL,
    entropy_word                        BIGINT UNSIGNED NOT NULL,
    shock_ppm                           INT UNSIGNED NOT NULL,
    employment_industry                 VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    base_monthly_revenue_krw            BIGINT NOT NULL,
    revenue_variation_ppm               INT UNSIGNED NOT NULL,
    variable_cost_ppm                   INT UNSIGNED NOT NULL,
    base_fixed_cost_krw                 BIGINT NOT NULL,
    scale_revenue_factor_ppm            INT UNSIGNED NOT NULL,
    scale_fixed_cost_krw                 BIGINT NOT NULL,
    officer_gross_salary_krw            BIGINT NOT NULL,
    revenue_krw                         BIGINT NOT NULL,
    variable_cost_krw                   BIGINT NOT NULL,
    operating_expense_krw               BIGINT NOT NULL,
    pre_payroll_profit_krw              BIGINT NOT NULL,
    cash_before_krw                     BIGINT NOT NULL,
    operating_cost_cash_paid_krw        BIGINT NOT NULL,
    operating_cost_payable_krw          BIGINT NOT NULL,
    cash_after_krw                      BIGINT NOT NULL,
    operating_payable_before_krw        BIGINT NOT NULL,
    operating_payable_after_krw         BIGINT NOT NULL,
    retained_earnings_before_krw        BIGINT NOT NULL,
    retained_earnings_after_krw         BIGINT NOT NULL,
    applied_game_day                    INT UNSIGNED NOT NULL,
    revenue_ledger_transaction_id       BIGINT UNSIGNED NULL,
    expense_ledger_transaction_id       BIGINT UNSIGNED NULL,
    status                              VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    applied_at                          DATETIME(3) NULL,
    PRIMARY KEY (id),
    UNIQUE KEY uk_corporation_operating_month
        (save_id, run_revision, corporation_id, operating_year, operating_month),
    UNIQUE KEY uk_corporation_operating_month_scope
        (save_id, run_revision, corporation_id, id),
    UNIQUE KEY uk_corporation_operating_month_revenue_ledger
        (revenue_ledger_transaction_id),
    UNIQUE KEY uk_corporation_operating_month_expense_ledger
        (expense_ledger_transaction_id),
    CONSTRAINT fk_corporation_operating_month_corporation
        FOREIGN KEY (save_id, run_revision, corporation_id)
        REFERENCES corporation (save_id, run_revision, id),
    CONSTRAINT fk_corporation_operating_month_scale
        FOREIGN KEY (
            corporation_component_version_id, industry_template_id, operating_scale_id
        ) REFERENCES corporation_operating_scale (
            life_component_version_id, industry_template_id, id
        ),
    CONSTRAINT ck_corporation_operating_month_key CHECK (
        operating_year BETWEEN 1 AND 9999
        AND operating_month BETWEEN 1 AND 12
        AND employment_industry IN ('itSoftware', 'retailService')
        AND shock_ppm BETWEEN 100000 AND 1900000
    ),
    CONSTRAINT ck_corporation_operating_month_terms CHECK (
        base_monthly_revenue_krw BETWEEN 1 AND 9007199254740991
        AND revenue_variation_ppm BETWEEN 0 AND 900000
        AND variable_cost_ppm BETWEEN 0 AND 1000000
        AND base_fixed_cost_krw BETWEEN 0 AND 9007199254740991
        AND scale_revenue_factor_ppm BETWEEN 1 AND 3000000
        AND scale_fixed_cost_krw BETWEEN 0 AND 9007199254740991
        AND officer_gross_salary_krw = 0
    ),
    CONSTRAINT ck_corporation_operating_month_result CHECK (
        revenue_krw BETWEEN 1 AND 9007199254740991
        AND variable_cost_krw BETWEEN 1 AND 9007199254740991
        AND operating_expense_krw
            = variable_cost_krw + base_fixed_cost_krw + scale_fixed_cost_krw
        AND pre_payroll_profit_krw = revenue_krw - operating_expense_krw
        AND pre_payroll_profit_krw BETWEEN -9007199254740991 AND 9007199254740991
        AND cash_before_krw BETWEEN 0 AND 9007199254740991
        AND operating_cost_cash_paid_krw BETWEEN 0 AND operating_expense_krw
        AND operating_cost_payable_krw
            = operating_expense_krw - operating_cost_cash_paid_krw
        AND cash_after_krw
            = cash_before_krw + revenue_krw - operating_cost_cash_paid_krw
        AND cash_after_krw BETWEEN 0 AND 9007199254740991
        AND operating_payable_before_krw BETWEEN 0 AND 9007199254740991
        AND operating_payable_after_krw
            = operating_payable_before_krw + operating_cost_payable_krw
        AND operating_payable_after_krw BETWEEN 0 AND 9007199254740991
        AND retained_earnings_before_krw BETWEEN -9007199254740991 AND 9007199254740991
        AND retained_earnings_after_krw
            = retained_earnings_before_krw + pre_payroll_profit_krw
        AND retained_earnings_after_krw BETWEEN -9007199254740991 AND 9007199254740991
    ),
    CONSTRAINT ck_corporation_operating_month_status CHECK (
        (status = 'preparing'
         AND revenue_ledger_transaction_id IS NULL
         AND expense_ledger_transaction_id IS NULL
         AND applied_at IS NULL)
        OR
        (status = 'applied'
         AND revenue_ledger_transaction_id IS NOT NULL
         AND expense_ledger_transaction_id IS NOT NULL
         AND applied_at IS NOT NULL)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

ALTER TABLE corporation_ledger_transaction
    MODIFY correlation_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    ADD COLUMN operating_month_id BIGINT UNSIGNED NULL AFTER correlation_id,
    ADD UNIQUE KEY uk_corporation_ledger_operating_month
        (save_id, run_revision, corporation_id, transaction_kind, operating_month_id),
    ADD CONSTRAINT fk_corporation_ledger_operating_month
        FOREIGN KEY (save_id, run_revision, corporation_id, operating_month_id)
        REFERENCES corporation_operating_month (save_id, run_revision, corporation_id, id),
    DROP CHECK ck_corporation_ledger_transaction,
    ADD CONSTRAINT ck_corporation_ledger_transaction CHECK (
        transaction_kind IN (
            'establishment', 'monthlyRevenue', 'monthlyExpense',
            'officerPayroll', 'corporateTax', 'dividend'
        )
        AND CHAR_LENGTH(description) BETWEEN 1 AND 255
        AND (
            (transaction_kind IN ('monthlyRevenue', 'monthlyExpense', 'officerPayroll')
             AND correlation_id IS NULL AND operating_month_id IS NOT NULL)
            OR
            (transaction_kind NOT IN ('monthlyRevenue', 'monthlyExpense', 'officerPayroll')
             AND correlation_id IS NOT NULL AND operating_month_id IS NULL)
        )
    );

ALTER TABLE corporation_operating_month
    ADD CONSTRAINT fk_corporation_operating_month_revenue_ledger
        FOREIGN KEY (
            save_id, run_revision, corporation_id, revenue_ledger_transaction_id
        ) REFERENCES corporation_ledger_transaction (
            save_id, run_revision, corporation_id, id
        ),
    ADD CONSTRAINT fk_corporation_operating_month_expense_ledger
        FOREIGN KEY (
            save_id, run_revision, corporation_id, expense_ledger_transaction_id
        ) REFERENCES corporation_ledger_transaction (
            save_id, run_revision, corporation_id, id
        );

DROP TRIGGER tr_corporation_ledger_transaction_insert_valid;
CREATE TRIGGER tr_corporation_ledger_transaction_insert_valid
BEFORE INSERT ON corporation_ledger_transaction
FOR EACH ROW
SET NEW.transaction_kind = IF(
    (
        NEW.transaction_kind = 'establishment'
        AND EXISTS (
            SELECT 1 FROM corporation AS corporation_row
            WHERE corporation_row.id = NEW.corporation_id
              AND corporation_row.save_id = NEW.save_id
              AND corporation_row.run_revision = NEW.run_revision
              AND corporation_row.status = 'draft'
              AND corporation_row.established_game_day = NEW.game_day
              AND BINARY corporation_row.establishment_command_id = BINARY NEW.correlation_id
        )
    )
    OR
    (
        NEW.transaction_kind IN ('monthlyRevenue', 'monthlyExpense')
        AND EXISTS (
            SELECT 1 FROM corporation_operating_month AS operating_month
            WHERE operating_month.id = NEW.operating_month_id
              AND operating_month.save_id = NEW.save_id
              AND operating_month.run_revision = NEW.run_revision
              AND operating_month.corporation_id = NEW.corporation_id
              AND operating_month.applied_game_day = NEW.game_day
              AND operating_month.status = 'preparing'
        )
    ),
    NEW.transaction_kind,
    NULL
);

DROP TRIGGER tr_corporation_ledger_posting_insert_valid;
CREATE TRIGGER tr_corporation_ledger_posting_insert_valid
BEFORE INSERT ON corporation_ledger_posting
FOR EACH ROW
SET NEW.account_code = IF(
    EXISTS (
        SELECT 1
        FROM corporation_ledger_transaction AS ledger
        INNER JOIN corporation AS corporation_row
            ON corporation_row.id = ledger.corporation_id
           AND corporation_row.save_id = ledger.save_id
           AND corporation_row.run_revision = ledger.run_revision
        WHERE ledger.id = NEW.corporation_ledger_transaction_id
          AND ledger.save_id = NEW.save_id
          AND ledger.run_revision = NEW.run_revision
          AND ledger.corporation_id = NEW.corporation_id
          AND ledger.transaction_kind = 'establishment'
          AND corporation_row.status = 'draft'
          AND (
              (NEW.posting_order = 1
               AND NEW.account_code = 'corporationCash'
               AND NEW.amount_krw = corporation_row.capital_krw)
              OR
              (NEW.posting_order = 2
               AND NEW.account_code = 'contributedCapital'
               AND NEW.amount_krw = -corporation_row.capital_krw)
          )
    )
    OR EXISTS (
        SELECT 1
        FROM corporation_ledger_transaction AS ledger
        INNER JOIN corporation_operating_month AS operating_month
            ON operating_month.id = ledger.operating_month_id
           AND operating_month.save_id = ledger.save_id
           AND operating_month.run_revision = ledger.run_revision
           AND operating_month.corporation_id = ledger.corporation_id
        WHERE ledger.id = NEW.corporation_ledger_transaction_id
          AND ledger.save_id = NEW.save_id
          AND ledger.run_revision = NEW.run_revision
          AND ledger.corporation_id = NEW.corporation_id
          AND operating_month.status = 'preparing'
          AND (
              (ledger.transaction_kind = 'monthlyRevenue' AND (
                  (NEW.posting_order = 1
                   AND NEW.account_code = 'corporationCash'
                   AND NEW.amount_krw = operating_month.revenue_krw)
                  OR
                  (NEW.posting_order = 2
                   AND NEW.account_code = 'operatingRevenue'
                   AND NEW.amount_krw = -operating_month.revenue_krw)
              ))
              OR
              (ledger.transaction_kind = 'monthlyExpense' AND (
                  (NEW.posting_order = 1
                   AND NEW.account_code = 'variableCostExpense'
                   AND NEW.amount_krw = operating_month.variable_cost_krw)
                  OR
                  (NEW.posting_order = 2
                   AND NEW.account_code = 'fixedCostExpense'
                   AND NEW.amount_krw = operating_month.base_fixed_cost_krw
                       + operating_month.scale_fixed_cost_krw)
                  OR
                  (NEW.posting_order = 3
                   AND NEW.account_code = 'corporationCash'
                   AND NEW.amount_krw = -operating_month.operating_cost_cash_paid_krw)
                  OR
                  (NEW.posting_order = 4
                   AND operating_month.operating_cost_payable_krw > 0
                   AND NEW.account_code = 'operatingPayable'
                   AND NEW.amount_krw = -operating_month.operating_cost_payable_krw)
              ))
          )
    ),
    NEW.account_code,
    NULL
);

DROP TRIGGER tr_corporation_status_transition_only;
CREATE TRIGGER tr_corporation_status_transition_only
BEFORE UPDATE ON corporation
FOR EACH ROW
SET NEW.id = IF(
    (
        OLD.status = 'draft'
        AND NEW.status = 'active'
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
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
        AND NEW.cash_krw = OLD.capital_krw
        AND NEW.contributed_capital_krw = OLD.capital_krw
        AND NEW.retained_earnings_krw = 0
        AND NEW.operating_payable_krw = 0
        AND NEW.distributable_profit_krw = 0
        AND NEW.personal_ledger_transaction_id IS NOT NULL
        AND NEW.corporation_ledger_transaction_id IS NOT NULL
        AND NEW.created_at = OLD.created_at
        AND EXISTS (
            SELECT 1 FROM ledger_transaction AS ledger
            WHERE ledger.id = NEW.personal_ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'corporationEstablishment'
              AND BINARY ledger.source_id = BINARY NEW.establishment_command_id
              AND (
                  SELECT COALESCE(SUM(posting.amount_krw), 0)
                  FROM ledger_posting AS posting
                  WHERE posting.ledger_transaction_id = ledger.id
              ) = 0
              AND (
                  SELECT COUNT(*) FROM ledger_posting AS posting
                  WHERE posting.ledger_transaction_id = ledger.id
              ) = 3
        )
        AND EXISTS (
            SELECT 1 FROM corporation_ledger_transaction AS ledger
            WHERE ledger.id = NEW.corporation_ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.corporation_id = NEW.id
              AND ledger.transaction_kind = 'establishment'
              AND (
                  SELECT COALESCE(SUM(posting.amount_krw), 0)
                  FROM corporation_ledger_posting AS posting
                  WHERE posting.corporation_ledger_transaction_id = ledger.id
              ) = 0
              AND (
                  SELECT COUNT(*) FROM corporation_ledger_posting AS posting
                  WHERE posting.corporation_ledger_transaction_id = ledger.id
              ) = 2
        )
    )
    OR
    (
        OLD.status = 'active'
        AND NEW.status IN ('active', 'insolvent')
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
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
        AND NEW.distributable_profit_krw = OLD.distributable_profit_krw
        AND NEW.personal_ledger_transaction_id = OLD.personal_ledger_transaction_id
        AND NEW.corporation_ledger_transaction_id = OLD.corporation_ledger_transaction_id
        AND NEW.created_at = OLD.created_at
        AND EXISTS (
            SELECT 1 FROM corporation_operating_month AS operating_month
            WHERE operating_month.save_id = OLD.save_id
              AND operating_month.run_revision = OLD.run_revision
              AND operating_month.corporation_id = OLD.id
              AND operating_month.status = 'preparing'
              AND operating_month.cash_before_krw = OLD.cash_krw
              AND operating_month.operating_payable_before_krw = OLD.operating_payable_krw
              AND operating_month.retained_earnings_before_krw = OLD.retained_earnings_krw
              AND operating_month.cash_after_krw = NEW.cash_krw
              AND operating_month.operating_payable_after_krw = NEW.operating_payable_krw
              AND operating_month.retained_earnings_after_krw = NEW.retained_earnings_krw
              AND NEW.status = IF(
                  operating_month.operating_cost_payable_krw > 0,
                  'insolvent',
                  'active'
              )
        )
    ),
    OLD.id,
    NULL
);

DROP TRIGGER tr_corporation_transition_insert_valid;
CREATE TRIGGER tr_corporation_transition_insert_valid
BEFORE INSERT ON corporation_transition
FOR EACH ROW
SET NEW.transition_no = IF(
    (
        NEW.transition_no = 1
        AND NEW.from_status IS NULL
        AND NEW.to_status = 'draft'
        AND NEW.transition_reason = 'playerEstablished'
        AND NEW.command_id IS NOT NULL
        AND EXISTS (
            SELECT 1 FROM corporation AS corporation_row
            WHERE corporation_row.id = NEW.corporation_id
              AND corporation_row.save_id = NEW.save_id
              AND corporation_row.run_revision = NEW.run_revision
              AND corporation_row.status = 'draft'
              AND BINARY corporation_row.establishment_command_id = BINARY NEW.command_id
              AND corporation_row.established_game_day = NEW.transition_game_day
        )
    )
    OR
    (
        NEW.transition_no = 2
        AND NEW.from_status = 'draft'
        AND NEW.to_status = 'active'
        AND NEW.transition_reason = 'establishmentFunded'
        AND NEW.command_id IS NOT NULL
        AND EXISTS (
            SELECT 1 FROM corporation AS corporation_row
            WHERE corporation_row.id = NEW.corporation_id
              AND corporation_row.save_id = NEW.save_id
              AND corporation_row.run_revision = NEW.run_revision
              AND corporation_row.status = 'active'
              AND BINARY corporation_row.establishment_command_id = BINARY NEW.command_id
              AND corporation_row.established_game_day = NEW.transition_game_day
        )
    )
    OR
    (
        NEW.transition_no = 3
        AND NEW.from_status = 'active'
        AND NEW.to_status = 'insolvent'
        AND NEW.transition_reason = 'operatingCashShortfall'
        AND NEW.command_id IS NULL
        AND EXISTS (
            SELECT 1
            FROM corporation AS corporation_row
            INNER JOIN corporation_operating_month AS operating_month
                ON operating_month.save_id = corporation_row.save_id
               AND operating_month.run_revision = corporation_row.run_revision
               AND operating_month.corporation_id = corporation_row.id
            WHERE corporation_row.id = NEW.corporation_id
              AND corporation_row.save_id = NEW.save_id
              AND corporation_row.run_revision = NEW.run_revision
              AND corporation_row.status = 'insolvent'
              AND operating_month.status = 'preparing'
              AND operating_month.applied_game_day = NEW.transition_game_day
              AND operating_month.operating_cost_payable_krw > 0
        )
    ),
    NEW.transition_no,
    NULL
);

CREATE TRIGGER tr_corporation_operating_month_apply_only
BEFORE UPDATE ON corporation_operating_month
FOR EACH ROW
SET NEW.status = IF(
    OLD.status = 'preparing'
        AND NEW.status = 'applied'
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.corporation_id = OLD.corporation_id
        AND NEW.corporation_component_version_id = OLD.corporation_component_version_id
        AND NEW.industry_template_id = OLD.industry_template_id
        AND NEW.operating_scale_id = OLD.operating_scale_id
        AND NEW.operating_year = OLD.operating_year
        AND NEW.operating_month = OLD.operating_month
        AND NEW.entropy_stream = OLD.entropy_stream
        AND NEW.entropy_word = OLD.entropy_word
        AND NEW.shock_ppm = OLD.shock_ppm
        AND BINARY NEW.employment_industry = BINARY OLD.employment_industry
        AND NEW.base_monthly_revenue_krw = OLD.base_monthly_revenue_krw
        AND NEW.revenue_variation_ppm = OLD.revenue_variation_ppm
        AND NEW.variable_cost_ppm = OLD.variable_cost_ppm
        AND NEW.base_fixed_cost_krw = OLD.base_fixed_cost_krw
        AND NEW.scale_revenue_factor_ppm = OLD.scale_revenue_factor_ppm
        AND NEW.scale_fixed_cost_krw = OLD.scale_fixed_cost_krw
        AND NEW.officer_gross_salary_krw = OLD.officer_gross_salary_krw
        AND NEW.revenue_krw = OLD.revenue_krw
        AND NEW.variable_cost_krw = OLD.variable_cost_krw
        AND NEW.operating_expense_krw = OLD.operating_expense_krw
        AND NEW.pre_payroll_profit_krw = OLD.pre_payroll_profit_krw
        AND NEW.cash_before_krw = OLD.cash_before_krw
        AND NEW.operating_cost_cash_paid_krw = OLD.operating_cost_cash_paid_krw
        AND NEW.operating_cost_payable_krw = OLD.operating_cost_payable_krw
        AND NEW.cash_after_krw = OLD.cash_after_krw
        AND NEW.operating_payable_before_krw = OLD.operating_payable_before_krw
        AND NEW.operating_payable_after_krw = OLD.operating_payable_after_krw
        AND NEW.retained_earnings_before_krw = OLD.retained_earnings_before_krw
        AND NEW.retained_earnings_after_krw = OLD.retained_earnings_after_krw
        AND NEW.applied_game_day = OLD.applied_game_day
        AND OLD.revenue_ledger_transaction_id IS NULL
        AND OLD.expense_ledger_transaction_id IS NULL
        AND NEW.revenue_ledger_transaction_id IS NOT NULL
        AND NEW.expense_ledger_transaction_id IS NOT NULL
        AND OLD.created_at = NEW.created_at
        AND OLD.applied_at IS NULL
        AND NEW.applied_at IS NOT NULL
        AND EXISTS (
            SELECT 1 FROM corporation AS corporation_row
            WHERE corporation_row.id = NEW.corporation_id
              AND corporation_row.save_id = NEW.save_id
              AND corporation_row.run_revision = NEW.run_revision
              AND corporation_row.cash_krw = NEW.cash_after_krw
              AND corporation_row.operating_payable_krw = NEW.operating_payable_after_krw
              AND corporation_row.retained_earnings_krw = NEW.retained_earnings_after_krw
              AND corporation_row.status = IF(
                  NEW.operating_cost_payable_krw > 0,
                  'insolvent',
                  'active'
              )
        )
        AND EXISTS (
            SELECT 1 FROM corporation_ledger_transaction AS ledger
            WHERE ledger.id = NEW.revenue_ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.corporation_id = NEW.corporation_id
              AND ledger.operating_month_id = NEW.id
              AND ledger.transaction_kind = 'monthlyRevenue'
              AND (
                  SELECT COALESCE(SUM(posting.amount_krw), 0)
                  FROM corporation_ledger_posting AS posting
                  WHERE posting.corporation_ledger_transaction_id = ledger.id
              ) = 0
              AND (
                  SELECT COUNT(*) FROM corporation_ledger_posting AS posting
                  WHERE posting.corporation_ledger_transaction_id = ledger.id
              ) = 2
        )
        AND EXISTS (
            SELECT 1 FROM corporation_ledger_transaction AS ledger
            WHERE ledger.id = NEW.expense_ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.corporation_id = NEW.corporation_id
              AND ledger.operating_month_id = NEW.id
              AND ledger.transaction_kind = 'monthlyExpense'
              AND (
                  SELECT COALESCE(SUM(posting.amount_krw), 0)
                  FROM corporation_ledger_posting AS posting
                  WHERE posting.corporation_ledger_transaction_id = ledger.id
              ) = 0
              AND (
                  SELECT COUNT(*) FROM corporation_ledger_posting AS posting
                  WHERE posting.corporation_ledger_transaction_id = ledger.id
              ) = IF(NEW.operating_cost_payable_krw > 0, 4, 3)
        ),
    NEW.status,
    NULL
);

CREATE TRIGGER tr_corporation_operating_month_no_delete
BEFORE DELETE ON corporation_operating_month
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation operating months are immutable';
