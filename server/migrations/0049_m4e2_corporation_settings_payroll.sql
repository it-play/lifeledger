-- M4-E2b: append-only next-month corporation settings and officer payroll.

CREATE TABLE corporation_operating_setting (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED NOT NULL,
    corporation_id                      BIGINT UNSIGNED NOT NULL,
    corporation_component_version_id    BIGINT UNSIGNED NOT NULL,
    industry_template_id                BIGINT UNSIGNED NOT NULL,
    operating_scale_id                  BIGINT UNSIGNED NOT NULL,
    command_id                          CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    effective_year                      SMALLINT UNSIGNED NOT NULL,
    effective_month                     TINYINT UNSIGNED NOT NULL,
    officer_gross_salary_krw            BIGINT NOT NULL,
    created_game_day                    INT UNSIGNED NOT NULL,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_corporation_operating_setting_command (save_id, command_id),
    UNIQUE KEY uk_corporation_operating_setting_scope
        (save_id, run_revision, corporation_id, id),
    KEY ix_corporation_operating_setting_effective
        (save_id, run_revision, corporation_id, effective_year, effective_month, id),
    CONSTRAINT fk_corporation_operating_setting_corporation
        FOREIGN KEY (save_id, run_revision, corporation_id)
        REFERENCES corporation (save_id, run_revision, id),
    CONSTRAINT fk_corporation_operating_setting_scale
        FOREIGN KEY (
            corporation_component_version_id, industry_template_id, operating_scale_id
        ) REFERENCES corporation_operating_scale (
            life_component_version_id, industry_template_id, id
        ),
    CONSTRAINT fk_corporation_operating_setting_command
        FOREIGN KEY (save_id, command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT ck_corporation_operating_setting CHECK (
        effective_year BETWEEN 1 AND 9999
        AND effective_month BETWEEN 1 AND 12
        AND officer_gross_salary_krw BETWEEN 0 AND 100000000
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE corporation_setting_command_receipt (
    save_id                 BIGINT UNSIGNED NOT NULL,
    command_id              CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    run_revision            INT UNSIGNED NOT NULL,
    state_revision          BIGINT UNSIGNED NOT NULL,
    game_day                INT UNSIGNED NOT NULL,
    corporation_id          BIGINT UNSIGNED NOT NULL,
    operating_setting_id    BIGINT UNSIGNED NOT NULL,
    command_kind            VARCHAR(40) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    payload_sha256          CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    result                  JSON NOT NULL,
    created_at              DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, command_id),
    UNIQUE KEY uk_corporation_setting_receipt_setting
        (save_id, run_revision, corporation_id, operating_setting_id),
    CONSTRAINT fk_corporation_setting_receipt_command
        FOREIGN KEY (save_id, command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT fk_corporation_setting_receipt_setting
        FOREIGN KEY (save_id, run_revision, corporation_id, operating_setting_id)
        REFERENCES corporation_operating_setting (save_id, run_revision, corporation_id, id),
    CONSTRAINT ck_corporation_setting_receipt CHECK (
        command_kind = 'updateCorporationSettings'
        AND payload_sha256 REGEXP '^[0-9a-f]{64}$'
        AND JSON_TYPE(result) = 'OBJECT'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_command_identity_corporation_settings_valid_insert
BEFORE INSERT ON command_identity
FOR EACH ROW
FOLLOWS tr_command_identity_corporation_valid_insert
SET NEW.command_kind = IF(
    NEW.command_kind <> 'updateCorporationSettings'
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

CREATE TRIGGER tr_corporation_operating_setting_valid_insert
BEFORE INSERT ON corporation_operating_setting
FOR EACH ROW
SET NEW.id = IF(
    EXISTS (
        SELECT 1
        FROM corporation AS corporation_row
        INNER JOIN save
            ON save.id = corporation_row.save_id
           AND save.run_revision = corporation_row.run_revision
        INNER JOIN market_world AS world ON world.id = save.market_world_id
        INNER JOIN command_identity AS identity
            ON identity.save_id = corporation_row.save_id
           AND BINARY identity.command_id = BINARY NEW.command_id
        INNER JOIN corporation_operating_scale AS scale
            ON scale.id = NEW.operating_scale_id
           AND scale.life_component_version_id
                = corporation_row.corporation_component_version_id
           AND scale.industry_template_id = corporation_row.industry_template_id
        WHERE corporation_row.id = NEW.corporation_id
          AND corporation_row.save_id = NEW.save_id
          AND corporation_row.run_revision = NEW.run_revision
          AND corporation_row.status = 'active'
          AND NEW.corporation_component_version_id
                = corporation_row.corporation_component_version_id
          AND NEW.industry_template_id = corporation_row.industry_template_id
          AND identity.command_kind = 'updateCorporationSettings'
          AND identity.initial_run_revision = NEW.run_revision
          AND identity.initial_game_day = NEW.created_game_day
          AND NEW.effective_year = YEAR(
                DATE_ADD(
                    DATE_ADD(world.start_date, INTERVAL save.game_day DAY),
                    INTERVAL 1 MONTH
                )
              )
          AND NEW.effective_month = MONTH(
                DATE_ADD(
                    DATE_ADD(world.start_date, INTERVAL save.game_day DAY),
                    INTERVAL 1 MONTH
                )
              )
    ),
    NEW.id,
    NULL
);

CREATE TRIGGER tr_corporation_operating_setting_no_update
BEFORE UPDATE ON corporation_operating_setting
FOR EACH ROW SIGNAL SQLSTATE '45000'
SET MESSAGE_TEXT = 'corporation operating settings are immutable';

CREATE TRIGGER tr_corporation_operating_setting_no_delete
BEFORE DELETE ON corporation_operating_setting
FOR EACH ROW SIGNAL SQLSTATE '45000'
SET MESSAGE_TEXT = 'corporation operating settings are immutable';

CREATE TRIGGER tr_corporation_setting_receipt_valid_insert
BEFORE INSERT ON corporation_setting_command_receipt
FOR EACH ROW
SET NEW.command_kind = IF(
    EXISTS (
        SELECT 1
        FROM corporation_operating_setting AS setting_row
        INNER JOIN command_identity AS identity
            ON identity.save_id = setting_row.save_id
           AND BINARY identity.command_id = BINARY setting_row.command_id
        INNER JOIN save
            ON save.id = setting_row.save_id
           AND save.run_revision = setting_row.run_revision
        WHERE setting_row.id = NEW.operating_setting_id
          AND setting_row.save_id = NEW.save_id
          AND setting_row.run_revision = NEW.run_revision
          AND setting_row.corporation_id = NEW.corporation_id
          AND BINARY setting_row.command_id = BINARY NEW.command_id
          AND identity.command_kind = 'updateCorporationSettings'
          AND identity.initial_run_revision = NEW.run_revision
          AND identity.initial_state_revision + 1 = NEW.state_revision
          AND identity.initial_game_day = NEW.game_day
          AND save.state_revision = NEW.state_revision
          AND save.game_day = NEW.game_day
    ),
    NEW.command_kind,
    NULL
);

CREATE TRIGGER tr_corporation_setting_receipt_no_update
BEFORE UPDATE ON corporation_setting_command_receipt
FOR EACH ROW SIGNAL SQLSTATE '45000'
SET MESSAGE_TEXT = 'corporation setting receipts are immutable';

CREATE TRIGGER tr_corporation_setting_receipt_no_delete
BEFORE DELETE ON corporation_setting_command_receipt
FOR EACH ROW SIGNAL SQLSTATE '45000'
SET MESSAGE_TEXT = 'corporation setting receipts are immutable';

ALTER TABLE employment_income_event
    MODIFY scheduled_settlement_id BIGINT UNSIGNED NULL,
    ADD COLUMN corporation_operating_month_id BIGINT UNSIGNED NULL
        AFTER military_service_id,
    ADD UNIQUE KEY uk_employment_income_event_corporation_month
        (save_id, run_revision, corporation_operating_month_id),
    ADD CONSTRAINT fk_employment_income_event_corporation_month
        FOREIGN KEY (save_id, run_revision, corporation_operating_month_id)
        REFERENCES corporation_operating_month (save_id, run_revision, id),
    DROP CHECK ck_employment_income_event_source,
    DROP CHECK ck_employment_income_event_amounts,
    ADD CONSTRAINT ck_employment_income_event_source CHECK (
        (
            source_kind = 'employmentPayroll'
            AND payroll_record_id IS NOT NULL
            AND military_service_id IS NULL
            AND corporation_operating_month_id IS NULL
            AND scheduled_settlement_id IS NOT NULL
            AND source_id = payroll_record_id
        )
        OR (
            source_kind = 'militaryPay'
            AND payroll_record_id IS NULL
            AND military_service_id IS NOT NULL
            AND corporation_operating_month_id IS NULL
            AND scheduled_settlement_id IS NOT NULL
            AND source_id = military_service_id
        )
        OR (
            source_kind = 'corporationOfficerPayroll'
            AND payroll_record_id IS NULL
            AND military_service_id IS NULL
            AND corporation_operating_month_id IS NOT NULL
            AND scheduled_settlement_id IS NULL
            AND source_id = corporation_operating_month_id
            AND occurrence = 1
        )
    ),
    ADD CONSTRAINT ck_employment_income_event_amounts CHECK (
        gross_employment_income_krw BETWEEN 0 AND 9007199254740991
        AND employee_national_pension_krw >= 0
        AND employee_health_insurance_krw >= 0
        AND employee_long_term_care_krw >= 0
        AND employee_employment_insurance_krw >= 0
        AND employee_insurance_total_krw >= 0
        AND withheld_income_tax_krw >= 0
        AND withheld_local_income_tax_krw >= 0
        AND net_pay_krw >= 0
        AND employee_insurance_total_krw
            = employee_national_pension_krw
            + employee_health_insurance_krw
            + employee_long_term_care_krw
            + employee_employment_insurance_krw
        AND net_pay_krw
            = gross_employment_income_krw
            - employee_insurance_total_krw
            - withheld_income_tax_krw
            - withheld_local_income_tax_krw
        AND (
            (gross_employment_income_krw = 0 AND ledger_transaction_id IS NULL)
            OR (gross_employment_income_krw > 0 AND ledger_transaction_id IS NOT NULL)
        )
    );

ALTER TABLE corporation_operating_month
    ADD COLUMN operating_setting_id BIGINT UNSIGNED NULL AFTER operating_scale_id,
    ADD COLUMN employment_policy_set_id BIGINT UNSIGNED NULL AFTER operating_setting_id,
    ADD COLUMN payroll_status VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin
        NOT NULL DEFAULT 'notConfigured' AFTER officer_gross_salary_krw,
    ADD COLUMN national_pension_employee_krw BIGINT NOT NULL DEFAULT 0 AFTER payroll_status,
    ADD COLUMN national_pension_employer_krw BIGINT NOT NULL DEFAULT 0
        AFTER national_pension_employee_krw,
    ADD COLUMN health_insurance_employee_krw BIGINT NOT NULL DEFAULT 0
        AFTER national_pension_employer_krw,
    ADD COLUMN health_insurance_employer_krw BIGINT NOT NULL DEFAULT 0
        AFTER health_insurance_employee_krw,
    ADD COLUMN long_term_care_employee_krw BIGINT NOT NULL DEFAULT 0
        AFTER health_insurance_employer_krw,
    ADD COLUMN long_term_care_employer_krw BIGINT NOT NULL DEFAULT 0
        AFTER long_term_care_employee_krw,
    ADD COLUMN employment_insurance_employee_krw BIGINT NOT NULL DEFAULT 0
        AFTER long_term_care_employer_krw,
    ADD COLUMN employment_insurance_employer_krw BIGINT NOT NULL DEFAULT 0
        AFTER employment_insurance_employee_krw,
    ADD COLUMN industrial_accident_employer_krw BIGINT NOT NULL DEFAULT 0
        AFTER employment_insurance_employer_krw,
    ADD COLUMN employee_insurance_total_krw BIGINT NOT NULL DEFAULT 0
        AFTER industrial_accident_employer_krw,
    ADD COLUMN employer_insurance_total_krw BIGINT NOT NULL DEFAULT 0
        AFTER employee_insurance_total_krw,
    ADD COLUMN withheld_income_tax_krw BIGINT NOT NULL DEFAULT 0
        AFTER employer_insurance_total_krw,
    ADD COLUMN withheld_local_income_tax_krw BIGINT NOT NULL DEFAULT 0
        AFTER withheld_income_tax_krw,
    ADD COLUMN net_salary_pay_krw BIGINT NOT NULL DEFAULT 0
        AFTER withheld_local_income_tax_krw,
    ADD COLUMN total_payroll_cost_krw BIGINT NOT NULL DEFAULT 0 AFTER net_salary_pay_krw,
    ADD COLUMN withholding_liability_krw BIGINT NOT NULL DEFAULT 0
        AFTER total_payroll_cost_krw,
    ADD COLUMN payroll_cash_paid_krw BIGINT NOT NULL DEFAULT 0
        AFTER withholding_liability_krw,
    ADD COLUMN payroll_payable_krw BIGINT NOT NULL DEFAULT 0 AFTER payroll_cash_paid_krw,
    ADD COLUMN pre_tax_profit_krw BIGINT NOT NULL DEFAULT 0 AFTER pre_payroll_profit_krw,
    ADD COLUMN operating_cash_after_krw BIGINT NOT NULL DEFAULT 0
        AFTER operating_cost_payable_krw,
    ADD COLUMN payroll_ledger_transaction_id BIGINT UNSIGNED NULL
        AFTER expense_ledger_transaction_id,
    ADD COLUMN personal_payroll_ledger_transaction_id BIGINT UNSIGNED NULL
        AFTER payroll_ledger_transaction_id,
    ADD COLUMN employment_income_event_id BIGINT UNSIGNED NULL
        AFTER personal_payroll_ledger_transaction_id,
    ADD UNIQUE KEY uk_corporation_operating_month_payroll_ledger
        (payroll_ledger_transaction_id),
    ADD UNIQUE KEY uk_corporation_operating_month_personal_payroll_ledger
        (personal_payroll_ledger_transaction_id),
    ADD UNIQUE KEY uk_corporation_operating_month_income_event
        (employment_income_event_id),
    ADD CONSTRAINT fk_corporation_operating_month_setting
        FOREIGN KEY (save_id, run_revision, corporation_id, operating_setting_id)
        REFERENCES corporation_operating_setting (save_id, run_revision, corporation_id, id),
    ADD CONSTRAINT fk_corporation_operating_month_employment_policy
        FOREIGN KEY (save_id, run_revision, employment_policy_set_id)
        REFERENCES career_run (save_id, run_revision, employment_policy_set_id),
    ADD CONSTRAINT fk_corporation_operating_month_payroll_ledger
        FOREIGN KEY (save_id, run_revision, corporation_id, payroll_ledger_transaction_id)
        REFERENCES corporation_ledger_transaction (save_id, run_revision, corporation_id, id),
    ADD CONSTRAINT fk_corporation_operating_month_personal_payroll_ledger
        FOREIGN KEY (save_id, run_revision, personal_payroll_ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    ADD CONSTRAINT fk_corporation_operating_month_income_event
        FOREIGN KEY (save_id, run_revision, employment_income_event_id)
        REFERENCES employment_income_event (save_id, run_revision, id);

UPDATE corporation_operating_month
SET pre_tax_profit_krw = pre_payroll_profit_krw,
    operating_cash_after_krw = cash_after_krw;

ALTER TABLE corporation_operating_month
    ALTER payroll_status DROP DEFAULT,
    ALTER national_pension_employee_krw DROP DEFAULT,
    ALTER national_pension_employer_krw DROP DEFAULT,
    ALTER health_insurance_employee_krw DROP DEFAULT,
    ALTER health_insurance_employer_krw DROP DEFAULT,
    ALTER long_term_care_employee_krw DROP DEFAULT,
    ALTER long_term_care_employer_krw DROP DEFAULT,
    ALTER employment_insurance_employee_krw DROP DEFAULT,
    ALTER employment_insurance_employer_krw DROP DEFAULT,
    ALTER industrial_accident_employer_krw DROP DEFAULT,
    ALTER employee_insurance_total_krw DROP DEFAULT,
    ALTER employer_insurance_total_krw DROP DEFAULT,
    ALTER withheld_income_tax_krw DROP DEFAULT,
    ALTER withheld_local_income_tax_krw DROP DEFAULT,
    ALTER net_salary_pay_krw DROP DEFAULT,
    ALTER total_payroll_cost_krw DROP DEFAULT,
    ALTER withholding_liability_krw DROP DEFAULT,
    ALTER payroll_cash_paid_krw DROP DEFAULT,
    ALTER payroll_payable_krw DROP DEFAULT,
    ALTER pre_tax_profit_krw DROP DEFAULT,
    ALTER operating_cash_after_krw DROP DEFAULT,
    DROP CHECK ck_corporation_operating_month_terms,
    DROP CHECK ck_corporation_operating_month_result,
    DROP CHECK ck_corporation_operating_month_status,
    ADD CONSTRAINT ck_corporation_operating_month_terms CHECK (
        base_monthly_revenue_krw BETWEEN 1 AND 9007199254740991
        AND revenue_variation_ppm BETWEEN 0 AND 900000
        AND variable_cost_ppm BETWEEN 0 AND 1000000
        AND base_fixed_cost_krw BETWEEN 0 AND 9007199254740991
        AND scale_revenue_factor_ppm BETWEEN 1 AND 3000000
        AND scale_fixed_cost_krw BETWEEN 0 AND 9007199254740991
        AND officer_gross_salary_krw BETWEEN 0 AND 100000000
    ),
    ADD CONSTRAINT ck_corporation_operating_month_payroll CHECK (
        national_pension_employee_krw >= 0
        AND national_pension_employer_krw >= 0
        AND health_insurance_employee_krw >= 0
        AND health_insurance_employer_krw >= 0
        AND long_term_care_employee_krw >= 0
        AND long_term_care_employer_krw >= 0
        AND employment_insurance_employee_krw >= 0
        AND employment_insurance_employer_krw >= 0
        AND industrial_accident_employer_krw >= 0
        AND employee_insurance_total_krw
            = national_pension_employee_krw + health_insurance_employee_krw
            + long_term_care_employee_krw + employment_insurance_employee_krw
        AND employer_insurance_total_krw
            = national_pension_employer_krw + health_insurance_employer_krw
            + long_term_care_employer_krw + employment_insurance_employer_krw
            + industrial_accident_employer_krw
        AND net_salary_pay_krw
            = officer_gross_salary_krw - employee_insurance_total_krw
            - withheld_income_tax_krw - withheld_local_income_tax_krw
        AND total_payroll_cost_krw
            = officer_gross_salary_krw + employer_insurance_total_krw
        AND pre_tax_profit_krw = pre_payroll_profit_krw - total_payroll_cost_krw
        AND (
            payroll_status = 'notConfigured'
            AND officer_gross_salary_krw = 0
            AND employment_policy_set_id IS NULL
            AND total_payroll_cost_krw = 0
            AND withholding_liability_krw = 0
            AND payroll_cash_paid_krw = 0
            AND payroll_payable_krw = 0
            AND payroll_ledger_transaction_id IS NULL
            AND personal_payroll_ledger_transaction_id IS NULL
            AND employment_income_event_id IS NULL
            OR payroll_status = 'paid'
            AND officer_gross_salary_krw > 0
            AND employment_policy_set_id IS NOT NULL
            AND withholding_liability_krw
                = employee_insurance_total_krw + employer_insurance_total_krw
                + withheld_income_tax_krw + withheld_local_income_tax_krw
            AND payroll_cash_paid_krw = net_salary_pay_krw
            AND payroll_payable_krw = 0
            AND (
                status = 'preparing'
                AND payroll_ledger_transaction_id IS NULL
                AND personal_payroll_ledger_transaction_id IS NULL
                AND employment_income_event_id IS NULL
                OR status = 'applied'
                AND payroll_ledger_transaction_id IS NOT NULL
                AND personal_payroll_ledger_transaction_id IS NOT NULL
                AND employment_income_event_id IS NOT NULL
            )
            OR payroll_status = 'unpaid'
            AND officer_gross_salary_krw > 0
            AND employment_policy_set_id IS NOT NULL
            AND withholding_liability_krw = 0
            AND payroll_cash_paid_krw = 0
            AND payroll_payable_krw = total_payroll_cost_krw
            AND (
                status = 'preparing'
                AND payroll_ledger_transaction_id IS NULL
                OR status = 'applied'
                AND payroll_ledger_transaction_id IS NOT NULL
            )
            AND personal_payroll_ledger_transaction_id IS NULL
            AND employment_income_event_id IS NULL
        )
    ),
    ADD CONSTRAINT ck_corporation_operating_month_result CHECK (
        revenue_krw BETWEEN 1 AND 9007199254740991
        AND variable_cost_krw BETWEEN 1 AND 9007199254740991
        AND operating_expense_krw
            = variable_cost_krw + base_fixed_cost_krw + scale_fixed_cost_krw
        AND pre_payroll_profit_krw = revenue_krw - operating_expense_krw
        AND pre_payroll_profit_krw BETWEEN -9007199254740991 AND 9007199254740991
        AND pre_tax_profit_krw BETWEEN -9007199254740991 AND 9007199254740991
        AND cash_before_krw BETWEEN 0 AND 9007199254740991
        AND operating_cost_cash_paid_krw BETWEEN 0 AND operating_expense_krw
        AND operating_cost_payable_krw
            = operating_expense_krw - operating_cost_cash_paid_krw
        AND operating_cash_after_krw
            = cash_before_krw + revenue_krw - operating_cost_cash_paid_krw
        AND cash_after_krw = operating_cash_after_krw - payroll_cash_paid_krw
        AND cash_after_krw BETWEEN 0 AND 9007199254740991
        AND operating_payable_before_krw BETWEEN 0 AND 9007199254740991
        AND operating_payable_after_krw
            = operating_payable_before_krw + operating_cost_payable_krw
            + payroll_payable_krw
        AND operating_payable_after_krw BETWEEN 0 AND 9007199254740991
        AND retained_earnings_before_krw BETWEEN -9007199254740991 AND 9007199254740991
        AND retained_earnings_after_krw
            = retained_earnings_before_krw + pre_tax_profit_krw
        AND retained_earnings_after_krw BETWEEN -9007199254740991 AND 9007199254740991
    ),
    ADD CONSTRAINT ck_corporation_operating_month_status CHECK (
        (status = 'preparing'
         AND revenue_ledger_transaction_id IS NULL
         AND expense_ledger_transaction_id IS NULL
         AND applied_at IS NULL)
        OR
        (status = 'applied'
         AND revenue_ledger_transaction_id IS NOT NULL
         AND expense_ledger_transaction_id IS NOT NULL
         AND applied_at IS NOT NULL)
    );

ALTER TABLE ledger_posting
    DROP CHECK ck_ledger_posting_corporation_reference,
    ADD CONSTRAINT ck_ledger_posting_corporation_reference CHECK (
        corporation_id IS NULL
        OR account_code IN (
            'wallet', 'corporationInvestmentAsset', 'corporationRegistrationExpense',
            'salaryIncome', 'employeeNationalPensionExpense',
            'employeeHealthInsuranceExpense', 'employeeLongTermCareExpense',
            'employeeEmploymentInsuranceExpense', 'employmentIncomeTaxWithholding',
            'employmentLocalIncomeTaxWithholding', 'distributionIncome',
            'withholdingTaxLiability'
        )
    );

DROP TRIGGER tr_ledger_transaction_corporation_source_insert;
CREATE TRIGGER tr_ledger_transaction_corporation_source_insert
BEFORE INSERT ON ledger_transaction
FOR EACH ROW
FOLLOWS tr_ledger_transaction_insolvency_source_insert
SET NEW.source_kind = IF(
    NEW.source_kind NOT LIKE 'corporation%'
        OR (NEW.source_kind = 'corporationEstablishment' AND (
            NEW.source_id REGEXP '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            AND EXISTS (
                SELECT 1
                FROM corporation AS corporation_row
                INNER JOIN run_rule_bundle AS bundle
                    ON bundle.save_id = corporation_row.save_id
                   AND bundle.run_revision = corporation_row.run_revision
                WHERE BINARY corporation_row.establishment_command_id = BINARY NEW.source_id
                  AND corporation_row.save_id = NEW.save_id
                  AND corporation_row.run_revision = NEW.run_revision
                  AND corporation_row.status = 'draft'
                  AND bundle.policy_set_id = NEW.policy_set_id
            )
        ))
        OR (NEW.source_kind = 'corporationOfficerPayroll'
            AND NEW.source_id REGEXP '^[1-9][0-9]*$'
            AND EXISTS (
                SELECT 1
                FROM corporation_operating_month AS operating_month
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
            )
        ),
    NEW.source_kind,
    NULL
);

DROP TRIGGER tr_ledger_posting_corporation_reference_insert;
CREATE TRIGGER tr_ledger_posting_corporation_reference_insert
BEFORE INSERT ON ledger_posting
FOR EACH ROW
FOLLOWS tr_ledger_posting_insolvency_reference_insert
SET NEW.account_code = IF(
    NEW.corporation_id IS NULL
        AND NOT EXISTS (
            SELECT 1 FROM ledger_transaction AS ledger
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind IN (
                  'corporationEstablishment', 'corporationOfficerPayroll'
              )
        )
    OR EXISTS (
        SELECT 1
        FROM ledger_transaction AS ledger
        INNER JOIN corporation AS corporation_row
            ON corporation_row.save_id = ledger.save_id
           AND corporation_row.run_revision = ledger.run_revision
           AND BINARY corporation_row.establishment_command_id = BINARY ledger.source_id
        WHERE ledger.id = NEW.ledger_transaction_id
          AND ledger.save_id = NEW.save_id
          AND ledger.run_revision = NEW.run_revision
          AND ledger.source_kind = 'corporationEstablishment'
          AND corporation_row.id = NEW.corporation_id
          AND corporation_row.status = 'draft'
          AND (
              (NEW.posting_order = 1
               AND NEW.account_code = 'corporationInvestmentAsset'
               AND NEW.amount_krw = corporation_row.capital_krw)
              OR
              (NEW.posting_order = 2
               AND NEW.account_code = 'corporationRegistrationExpense'
               AND NEW.amount_krw = corporation_row.total_establishment_fee_krw)
              OR
              (NEW.posting_order = 3
               AND NEW.account_code = 'wallet'
               AND NEW.amount_krw = -(
                   corporation_row.capital_krw + corporation_row.total_establishment_fee_krw
               ))
          )
    )
    OR EXISTS (
        SELECT 1
        FROM ledger_transaction AS ledger
        INNER JOIN corporation_operating_month AS operating_month
            ON operating_month.id = CAST(ledger.source_id AS UNSIGNED)
           AND operating_month.save_id = ledger.save_id
           AND operating_month.run_revision = ledger.run_revision
        WHERE ledger.id = NEW.ledger_transaction_id
          AND ledger.save_id = NEW.save_id
          AND ledger.run_revision = NEW.run_revision
          AND ledger.source_kind = 'corporationOfficerPayroll'
          AND operating_month.corporation_id = NEW.corporation_id
          AND operating_month.payroll_status = 'paid'
          AND operating_month.status = 'preparing'
          AND NEW.account_code IN (
              'wallet', 'salaryIncome', 'employeeNationalPensionExpense',
              'employeeHealthInsuranceExpense', 'employeeLongTermCareExpense',
              'employeeEmploymentInsuranceExpense', 'employmentIncomeTaxWithholding',
              'employmentLocalIncomeTaxWithholding'
          )
    ),
    NEW.account_code,
    NULL
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
    OR (
        NEW.transaction_kind IN ('monthlyRevenue', 'monthlyExpense', 'officerPayroll')
        AND EXISTS (
            SELECT 1 FROM corporation_operating_month AS operating_month
            WHERE operating_month.id = NEW.operating_month_id
              AND operating_month.save_id = NEW.save_id
              AND operating_month.run_revision = NEW.run_revision
              AND operating_month.corporation_id = NEW.corporation_id
              AND operating_month.applied_game_day = NEW.game_day
              AND operating_month.status = 'preparing'
              AND (
                  NEW.transaction_kind <> 'officerPayroll'
                  OR operating_month.officer_gross_salary_krw > 0
              )
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
              OR
              (ledger.transaction_kind = 'officerPayroll' AND (
                  (NEW.posting_order = 1
                   AND NEW.account_code = 'officerPayrollExpense'
                   AND NEW.amount_krw = operating_month.total_payroll_cost_krw)
                  OR
                  (operating_month.payroll_status = 'paid'
                   AND NEW.posting_order = 2
                   AND NEW.account_code = 'corporationCash'
                   AND NEW.amount_krw = -operating_month.payroll_cash_paid_krw)
                  OR
                  (operating_month.payroll_status = 'paid'
                   AND operating_month.withholding_liability_krw > 0
                   AND NEW.posting_order = 3
                   AND NEW.account_code = 'withholdingTaxLiability'
                   AND NEW.amount_krw = -operating_month.withholding_liability_krw)
                  OR
                  (operating_month.payroll_status = 'unpaid'
                   AND NEW.posting_order = 2
                   AND NEW.account_code = 'operatingPayable'
                   AND NEW.amount_krw = -operating_month.payroll_payable_krw)
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
    )
    OR (
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
                  operating_month.operating_cost_payable_krw > 0
                      OR operating_month.payroll_payable_krw > 0,
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
    OR (
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
    OR (
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
              AND (
                  operating_month.operating_cost_payable_krw > 0
                  OR operating_month.payroll_payable_krw > 0
              )
        )
    ),
    NEW.transition_no,
    NULL
);

DROP TRIGGER tr_corporation_operating_month_apply_only;
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
        AND NEW.operating_setting_id <=> OLD.operating_setting_id
        AND NEW.employment_policy_set_id <=> OLD.employment_policy_set_id
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
        AND BINARY NEW.payroll_status = BINARY OLD.payroll_status
        AND NEW.national_pension_employee_krw = OLD.national_pension_employee_krw
        AND NEW.national_pension_employer_krw = OLD.national_pension_employer_krw
        AND NEW.health_insurance_employee_krw = OLD.health_insurance_employee_krw
        AND NEW.health_insurance_employer_krw = OLD.health_insurance_employer_krw
        AND NEW.long_term_care_employee_krw = OLD.long_term_care_employee_krw
        AND NEW.long_term_care_employer_krw = OLD.long_term_care_employer_krw
        AND NEW.employment_insurance_employee_krw
            = OLD.employment_insurance_employee_krw
        AND NEW.employment_insurance_employer_krw
            = OLD.employment_insurance_employer_krw
        AND NEW.industrial_accident_employer_krw = OLD.industrial_accident_employer_krw
        AND NEW.employee_insurance_total_krw = OLD.employee_insurance_total_krw
        AND NEW.employer_insurance_total_krw = OLD.employer_insurance_total_krw
        AND NEW.withheld_income_tax_krw = OLD.withheld_income_tax_krw
        AND NEW.withheld_local_income_tax_krw = OLD.withheld_local_income_tax_krw
        AND NEW.net_salary_pay_krw = OLD.net_salary_pay_krw
        AND NEW.total_payroll_cost_krw = OLD.total_payroll_cost_krw
        AND NEW.withholding_liability_krw = OLD.withholding_liability_krw
        AND NEW.payroll_cash_paid_krw = OLD.payroll_cash_paid_krw
        AND NEW.payroll_payable_krw = OLD.payroll_payable_krw
        AND NEW.revenue_krw = OLD.revenue_krw
        AND NEW.variable_cost_krw = OLD.variable_cost_krw
        AND NEW.operating_expense_krw = OLD.operating_expense_krw
        AND NEW.pre_payroll_profit_krw = OLD.pre_payroll_profit_krw
        AND NEW.pre_tax_profit_krw = OLD.pre_tax_profit_krw
        AND NEW.cash_before_krw = OLD.cash_before_krw
        AND NEW.operating_cost_cash_paid_krw = OLD.operating_cost_cash_paid_krw
        AND NEW.operating_cost_payable_krw = OLD.operating_cost_payable_krw
        AND NEW.operating_cash_after_krw = OLD.operating_cash_after_krw
        AND NEW.cash_after_krw = OLD.cash_after_krw
        AND NEW.operating_payable_before_krw = OLD.operating_payable_before_krw
        AND NEW.operating_payable_after_krw = OLD.operating_payable_after_krw
        AND NEW.retained_earnings_before_krw = OLD.retained_earnings_before_krw
        AND NEW.retained_earnings_after_krw = OLD.retained_earnings_after_krw
        AND NEW.applied_game_day = OLD.applied_game_day
        AND OLD.revenue_ledger_transaction_id IS NULL
        AND OLD.expense_ledger_transaction_id IS NULL
        AND OLD.payroll_ledger_transaction_id IS NULL
        AND OLD.personal_payroll_ledger_transaction_id IS NULL
        AND OLD.employment_income_event_id IS NULL
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
                  NEW.operating_cost_payable_krw > 0 OR NEW.payroll_payable_krw > 0,
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
              AND (SELECT COALESCE(SUM(posting.amount_krw), 0)
                   FROM corporation_ledger_posting AS posting
                   WHERE posting.corporation_ledger_transaction_id = ledger.id) = 0
              AND (SELECT COUNT(*) FROM corporation_ledger_posting AS posting
                   WHERE posting.corporation_ledger_transaction_id = ledger.id) = 2
        )
        AND EXISTS (
            SELECT 1 FROM corporation_ledger_transaction AS ledger
            WHERE ledger.id = NEW.expense_ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.corporation_id = NEW.corporation_id
              AND ledger.operating_month_id = NEW.id
              AND ledger.transaction_kind = 'monthlyExpense'
              AND (SELECT COALESCE(SUM(posting.amount_krw), 0)
                   FROM corporation_ledger_posting AS posting
                   WHERE posting.corporation_ledger_transaction_id = ledger.id) = 0
        )
        AND (
            NEW.payroll_status = 'notConfigured'
            AND NEW.payroll_ledger_transaction_id IS NULL
            AND NEW.personal_payroll_ledger_transaction_id IS NULL
            AND NEW.employment_income_event_id IS NULL
            OR NEW.payroll_status = 'unpaid'
            AND NEW.payroll_ledger_transaction_id IS NOT NULL
            AND NEW.personal_payroll_ledger_transaction_id IS NULL
            AND NEW.employment_income_event_id IS NULL
            AND EXISTS (
                SELECT 1 FROM corporation_ledger_transaction AS ledger
                WHERE ledger.id = NEW.payroll_ledger_transaction_id
                  AND ledger.save_id = NEW.save_id
                  AND ledger.run_revision = NEW.run_revision
                  AND ledger.corporation_id = NEW.corporation_id
                  AND ledger.operating_month_id = NEW.id
                  AND ledger.transaction_kind = 'officerPayroll'
                  AND (SELECT COALESCE(SUM(posting.amount_krw), 0)
                       FROM corporation_ledger_posting AS posting
                       WHERE posting.corporation_ledger_transaction_id = ledger.id) = 0
            )
            OR NEW.payroll_status = 'paid'
            AND NEW.payroll_ledger_transaction_id IS NOT NULL
            AND NEW.personal_payroll_ledger_transaction_id IS NOT NULL
            AND NEW.employment_income_event_id IS NOT NULL
            AND EXISTS (
                SELECT 1 FROM corporation_ledger_transaction AS ledger
                WHERE ledger.id = NEW.payroll_ledger_transaction_id
                  AND ledger.save_id = NEW.save_id
                  AND ledger.run_revision = NEW.run_revision
                  AND ledger.corporation_id = NEW.corporation_id
                  AND ledger.operating_month_id = NEW.id
                  AND ledger.transaction_kind = 'officerPayroll'
                  AND (SELECT COALESCE(SUM(posting.amount_krw), 0)
                       FROM corporation_ledger_posting AS posting
                       WHERE posting.corporation_ledger_transaction_id = ledger.id) = 0
            )
            AND EXISTS (
                SELECT 1 FROM employment_income_event AS income_event
                WHERE income_event.id = NEW.employment_income_event_id
                  AND income_event.save_id = NEW.save_id
                  AND income_event.run_revision = NEW.run_revision
                  AND income_event.corporation_operating_month_id = NEW.id
                  AND income_event.ledger_transaction_id
                        = NEW.personal_payroll_ledger_transaction_id
            )
        ),
    NEW.status,
    NULL
);

DROP TRIGGER tr_employment_income_event_valid_insert;
CREATE TRIGGER tr_employment_income_event_valid_insert
BEFORE INSERT ON employment_income_event
FOR EACH ROW
SET NEW.save_id = IF(
    NOT EXISTS (
        SELECT 1 FROM employment_income_year AS income_year
        WHERE income_year.save_id = NEW.save_id
          AND income_year.run_revision = NEW.run_revision
          AND income_year.tax_year = NEW.tax_year
          AND income_year.status = 'finalized'
    )
        AND (
            (
                NEW.source_kind = 'employmentPayroll'
                AND EXISTS (
                    SELECT 1
                    FROM payroll_record AS payroll
                    WHERE payroll.id = NEW.payroll_record_id
                      AND payroll.save_id = NEW.save_id
                      AND payroll.run_revision = NEW.run_revision
                      AND payroll.employment_policy_set_id = NEW.employment_policy_set_id
                      AND payroll.period_no = NEW.occurrence
                      AND payroll.scheduled_settlement_id = NEW.scheduled_settlement_id
                      AND payroll.ledger_transaction_id <=> NEW.ledger_transaction_id
                      AND payroll.payday_game_day = NEW.paid_game_day
                      AND payroll.payday = NEW.paid_date
                      AND payroll.tax_year = NEW.tax_year
                      AND payroll.gross_pay_krw = NEW.gross_employment_income_krw
                      AND payroll.national_pension_employee_krw
                            = NEW.employee_national_pension_krw
                      AND payroll.health_insurance_employee_krw
                            = NEW.employee_health_insurance_krw
                      AND payroll.long_term_care_employee_krw
                            = NEW.employee_long_term_care_krw
                      AND payroll.employment_insurance_employee_krw
                            = NEW.employee_employment_insurance_krw
                      AND payroll.employee_insurance_total_krw
                            = NEW.employee_insurance_total_krw
                      AND payroll.withheld_income_tax_krw = NEW.withheld_income_tax_krw
                      AND payroll.withheld_local_income_tax_krw
                            = NEW.withheld_local_income_tax_krw
                      AND payroll.net_salary_pay_krw = NEW.net_pay_krw
                )
            )
            OR (
                NEW.source_kind = 'militaryPay'
                AND EXISTS (
                    SELECT 1
                    FROM military_service AS service
                    INNER JOIN save
                        ON save.id = service.save_id
                       AND save.run_revision = service.run_revision
                    INNER JOIN market_world
                        ON market_world.id = save.market_world_id
                    INNER JOIN military_option_policy AS option_policy
                        ON option_policy.id = service.military_option_policy_id
                       AND option_policy.employment_policy_set_id
                            = service.employment_policy_set_id
                       AND option_policy.career_catalog_bundle_id
                            = service.career_catalog_bundle_id
                       AND option_policy.military_option_version_id
                            = service.military_option_version_id
                    INNER JOIN military_pay_stage AS pay_stage
                        ON pay_stage.military_option_policy_id = option_policy.id
                       AND pay_stage.start_service_month <= (
                           (YEAR(NEW.paid_date) - YEAR(service.start_date)) * 12
                               + MONTH(NEW.paid_date) - MONTH(service.start_date)
                               - IF(
                                   TIMESTAMPADD(
                                       MONTH,
                                       (YEAR(NEW.paid_date) - YEAR(service.start_date)) * 12
                                           + MONTH(NEW.paid_date)
                                           - MONTH(service.start_date),
                                       service.start_date
                                   ) > NEW.paid_date,
                                   1,
                                   0
                               )
                       )
                       AND pay_stage.end_service_month_exclusive > (
                           (YEAR(NEW.paid_date) - YEAR(service.start_date)) * 12
                               + MONTH(NEW.paid_date) - MONTH(service.start_date)
                               - IF(
                                   TIMESTAMPADD(
                                       MONTH,
                                       (YEAR(NEW.paid_date) - YEAR(service.start_date)) * 12
                                           + MONTH(NEW.paid_date)
                                           - MONTH(service.start_date),
                                       service.start_date
                                   ) > NEW.paid_date,
                                   1,
                                   0
                               )
                       )
                    INNER JOIN scheduled_settlement AS settlement
                        ON settlement.id = NEW.scheduled_settlement_id
                       AND settlement.save_id = service.save_id
                       AND settlement.run_revision = service.run_revision
                       AND settlement.kind = 'militaryPay'
                       AND settlement.source_kind = 'militaryService'
                       AND BINARY settlement.source_id = BINARY CAST(service.id AS CHAR)
                       AND settlement.occurrence = NEW.occurrence
                       AND settlement.due_game_day = NEW.paid_game_day
                       AND settlement.status = 'pending'
                    LEFT JOIN ledger_transaction AS ledger
                        ON ledger.save_id = service.save_id
                       AND ledger.run_revision = service.run_revision
                       AND ledger.id = NEW.ledger_transaction_id
                    WHERE service.id = NEW.military_service_id
                      AND service.save_id = NEW.save_id
                      AND service.run_revision = NEW.run_revision
                      AND service.employment_policy_set_id = NEW.employment_policy_set_id
                      AND service.status IN ('serving', 'completed')
                      AND NEW.paid_game_day >= service.start_game_day
                      AND NEW.paid_game_day < service.end_game_day
                      AND NEW.paid_date = DATE_ADD(
                          market_world.start_date,
                          INTERVAL NEW.paid_game_day DAY
                      )
                      AND NEW.gross_employment_income_krw = pay_stage.monthly_gross_pay_krw
                      AND (
                          option_policy.social_insurance_kind = 'employmentPayroll'
                          OR (
                              option_policy.social_insurance_kind = 'notAssessed'
                              AND NEW.employee_national_pension_krw = 0
                              AND NEW.employee_health_insurance_krw = 0
                              AND NEW.employee_long_term_care_krw = 0
                              AND NEW.employee_employment_insurance_krw = 0
                              AND NEW.employee_insurance_total_krw = 0
                          )
                      )
                      AND (
                          (NEW.gross_employment_income_krw = 0 AND ledger.id IS NULL)
                          OR (
                              NEW.gross_employment_income_krw > 0
                              AND ledger.policy_set_id = save.policy_set_id
                              AND ledger.game_day = NEW.paid_game_day
                              AND BINARY ledger.source_kind = BINARY 'militaryPay'
                              AND BINARY ledger.source_id
                                    = BINARY CAST(settlement.id AS CHAR)
                          )
                      )
                )
            )
            OR (
                NEW.source_kind = 'corporationOfficerPayroll'
                AND EXISTS (
                    SELECT 1
                    FROM corporation_operating_month AS operating_month
                    INNER JOIN save
                        ON save.id = operating_month.save_id
                       AND save.run_revision = operating_month.run_revision
                    INNER JOIN market_world ON market_world.id = save.market_world_id
                    INNER JOIN run_rule_bundle AS bundle
                        ON bundle.save_id = operating_month.save_id
                       AND bundle.run_revision = operating_month.run_revision
                    INNER JOIN ledger_transaction AS ledger
                        ON ledger.id = NEW.ledger_transaction_id
                       AND ledger.save_id = operating_month.save_id
                       AND ledger.run_revision = operating_month.run_revision
                    WHERE operating_month.id = NEW.corporation_operating_month_id
                      AND operating_month.save_id = NEW.save_id
                      AND operating_month.run_revision = NEW.run_revision
                      AND operating_month.employment_policy_set_id
                            = NEW.employment_policy_set_id
                      AND operating_month.payroll_status = 'paid'
                      AND operating_month.status = 'preparing'
                      AND operating_month.applied_game_day = NEW.paid_game_day
                      AND NEW.paid_date = DATE_ADD(
                          market_world.start_date,
                          INTERVAL NEW.paid_game_day DAY
                      )
                      AND NEW.tax_year = YEAR(NEW.paid_date)
                      AND NEW.gross_employment_income_krw
                            = operating_month.officer_gross_salary_krw
                      AND NEW.employee_national_pension_krw
                            = operating_month.national_pension_employee_krw
                      AND NEW.employee_health_insurance_krw
                            = operating_month.health_insurance_employee_krw
                      AND NEW.employee_long_term_care_krw
                            = operating_month.long_term_care_employee_krw
                      AND NEW.employee_employment_insurance_krw
                            = operating_month.employment_insurance_employee_krw
                      AND NEW.employee_insurance_total_krw
                            = operating_month.employee_insurance_total_krw
                      AND NEW.withheld_income_tax_krw
                            = operating_month.withheld_income_tax_krw
                      AND NEW.withheld_local_income_tax_krw
                            = operating_month.withheld_local_income_tax_krw
                      AND NEW.net_pay_krw = operating_month.net_salary_pay_krw
                      AND ledger.policy_set_id = bundle.policy_set_id
                      AND ledger.game_day = NEW.paid_game_day
                      AND BINARY ledger.source_kind
                            = BINARY 'corporationOfficerPayroll'
                      AND BINARY ledger.source_id
                            = BINARY CAST(operating_month.id AS CHAR)
                )
            )
        ),
    NEW.save_id,
    NULL
);
