-- M5-E: typed corporation funding, working-capital debt, and dissolution authority.

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;

CREATE TABLE corporation_capital_contribution (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED NOT NULL,
    corporation_id                      BIGINT UNSIGNED NOT NULL,
    command_id                          CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    amount_krw                          BIGINT NOT NULL,
    wallet_before_krw                   BIGINT NOT NULL,
    wallet_after_krw                    BIGINT NOT NULL,
    cash_before_krw                     BIGINT NOT NULL,
    cash_after_krw                      BIGINT NOT NULL,
    contributed_capital_before_krw      BIGINT NOT NULL,
    contributed_capital_after_krw       BIGINT NOT NULL,
    corporation_ledger_transaction_id   BIGINT UNSIGNED NULL,
    personal_ledger_transaction_id      BIGINT UNSIGNED NULL,
    applied_game_day                    INT UNSIGNED NOT NULL,
    status                              VARCHAR(12) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    applied_at                          DATETIME(3) NULL,
    PRIMARY KEY (id),
    UNIQUE KEY uk_corporation_contribution_command (save_id, command_id),
    UNIQUE KEY uk_corporation_contribution_scope (save_id, run_revision, corporation_id, id),
    UNIQUE KEY uk_corporation_contribution_corp_ledger (corporation_ledger_transaction_id),
    UNIQUE KEY uk_corporation_contribution_personal_ledger (personal_ledger_transaction_id),
    CONSTRAINT fk_corporation_contribution_corporation
        FOREIGN KEY (save_id, run_revision, corporation_id)
        REFERENCES corporation (save_id, run_revision, id),
    CONSTRAINT ck_corporation_contribution CHECK (
        command_id REGEXP '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
        AND amount_krw BETWEEN 1 AND 9007199254740991
        AND wallet_before_krw BETWEEN amount_krw AND 9007199254740991
        AND wallet_after_krw = wallet_before_krw - amount_krw
        AND cash_before_krw BETWEEN 0 AND 9007199254740991
        AND cash_after_krw = cash_before_krw + amount_krw
        AND contributed_capital_before_krw BETWEEN 1 AND 9007199254740991
        AND contributed_capital_after_krw = contributed_capital_before_krw + amount_krw
        AND status IN ('preparing', 'applied')
        AND ((status = 'preparing' AND corporation_ledger_transaction_id IS NULL
              AND personal_ledger_transaction_id IS NULL AND applied_at IS NULL)
             OR (status = 'applied' AND corporation_ledger_transaction_id IS NOT NULL
                 AND personal_ledger_transaction_id IS NOT NULL AND applied_at IS NOT NULL))
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE corporation_working_capital_loan (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED NOT NULL,
    corporation_id                      BIGINT UNSIGNED NOT NULL,
    business_profile_id                 BIGINT UNSIGNED NOT NULL,
    business_catalog_version_id         BIGINT UNSIGNED NOT NULL,
    loan_product_id                     BIGINT UNSIGNED NOT NULL,
    command_id                          CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    original_principal_krw              BIGINT NOT NULL,
    outstanding_principal_krw           BIGINT NOT NULL,
    monthly_interest_rate_ppm           INT UNSIGNED NOT NULL,
    term_months                         SMALLINT UNSIGNED NOT NULL,
    originated_year                     SMALLINT UNSIGNED NOT NULL,
    originated_month                    TINYINT UNSIGNED NOT NULL,
    maturity_year                       SMALLINT UNSIGNED NOT NULL,
    maturity_month                      TINYINT UNSIGNED NOT NULL,
    personal_guarantee                  BOOLEAN NOT NULL,
    cash_before_krw                     BIGINT NOT NULL,
    cash_after_krw                      BIGINT NOT NULL,
    corporation_ledger_transaction_id   BIGINT UNSIGNED NULL,
    originated_game_day                 INT UNSIGNED NOT NULL,
    status                              VARCHAR(12) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    applied_at                          DATETIME(3) NULL,
    PRIMARY KEY (id),
    UNIQUE KEY uk_working_capital_loan_command (save_id, command_id),
    UNIQUE KEY uk_working_capital_loan_scope (save_id, run_revision, corporation_id, id),
    UNIQUE KEY uk_working_capital_loan_ledger (corporation_ledger_transaction_id),
    CONSTRAINT fk_working_capital_loan_corporation
        FOREIGN KEY (save_id, run_revision, corporation_id)
        REFERENCES corporation (save_id, run_revision, id),
    CONSTRAINT fk_working_capital_loan_profile
        FOREIGN KEY (save_id, run_revision, corporation_id, business_profile_id)
        REFERENCES corporation_business_profile (save_id, run_revision, corporation_id, id),
    CONSTRAINT fk_working_capital_loan_product
        FOREIGN KEY (business_catalog_version_id, loan_product_id)
        REFERENCES business_loan_product (business_catalog_version_id, id),
    CONSTRAINT ck_working_capital_loan CHECK (
        command_id REGEXP '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
        AND original_principal_krw BETWEEN 1 AND 9007199254740991
        AND outstanding_principal_krw BETWEEN 0 AND original_principal_krw
        AND monthly_interest_rate_ppm BETWEEN 1 AND 1000000
        AND term_months BETWEEN 1 AND 120
        AND originated_year BETWEEN 1 AND 9999 AND originated_month BETWEEN 1 AND 12
        AND maturity_year BETWEEN originated_year AND 9999 AND maturity_month BETWEEN 1 AND 12
        AND personal_guarantee = FALSE
        AND cash_before_krw BETWEEN 0 AND 9007199254740991
        AND cash_after_krw = cash_before_krw + original_principal_krw
        AND status IN ('preparing', 'active', 'matured', 'repaid')
        AND ((status = 'preparing' AND corporation_ledger_transaction_id IS NULL
              AND applied_at IS NULL)
             OR (status IN ('active', 'matured') AND outstanding_principal_krw > 0
                 AND corporation_ledger_transaction_id IS NOT NULL AND applied_at IS NOT NULL)
             OR (status = 'repaid' AND outstanding_principal_krw = 0
                 AND corporation_ledger_transaction_id IS NOT NULL AND applied_at IS NOT NULL))
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE corporation_working_capital_loan_repayment (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED NOT NULL,
    corporation_id                      BIGINT UNSIGNED NOT NULL,
    loan_id                             BIGINT UNSIGNED NOT NULL,
    command_id                          CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    principal_krw                       BIGINT NOT NULL,
    outstanding_before_krw              BIGINT NOT NULL,
    outstanding_after_krw               BIGINT NOT NULL,
    cash_before_krw                     BIGINT NOT NULL,
    cash_after_krw                      BIGINT NOT NULL,
    corporation_ledger_transaction_id   BIGINT UNSIGNED NULL,
    applied_game_day                    INT UNSIGNED NOT NULL,
    status                              VARCHAR(12) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    applied_at                          DATETIME(3) NULL,
    PRIMARY KEY (id),
    UNIQUE KEY uk_working_capital_repayment_command (save_id, command_id),
    UNIQUE KEY uk_working_capital_repayment_scope (save_id, run_revision, corporation_id, id),
    UNIQUE KEY uk_working_capital_repayment_ledger (corporation_ledger_transaction_id),
    CONSTRAINT fk_working_capital_repayment_loan
        FOREIGN KEY (save_id, run_revision, corporation_id, loan_id)
        REFERENCES corporation_working_capital_loan (save_id, run_revision, corporation_id, id),
    CONSTRAINT ck_working_capital_repayment CHECK (
        command_id REGEXP '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
        AND principal_krw BETWEEN 1 AND 9007199254740991
        AND outstanding_before_krw >= principal_krw
        AND outstanding_after_krw = outstanding_before_krw - principal_krw
        AND cash_before_krw >= principal_krw
        AND cash_after_krw = cash_before_krw - principal_krw
        AND status IN ('preparing', 'applied')
        AND ((status = 'preparing' AND corporation_ledger_transaction_id IS NULL
              AND applied_at IS NULL)
             OR (status = 'applied' AND corporation_ledger_transaction_id IS NOT NULL
                 AND applied_at IS NOT NULL))
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE corporation_dissolution (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED NOT NULL,
    corporation_id                      BIGINT UNSIGNED NOT NULL,
    command_id                          CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    distribution_krw                    BIGINT NOT NULL,
    capital_basis_krw                   BIGINT NOT NULL,
    realized_gain_loss_krw              BIGINT NOT NULL,
    wallet_before_krw                   BIGINT NOT NULL,
    wallet_after_krw                    BIGINT NOT NULL,
    cash_before_krw                     BIGINT NOT NULL,
    contributed_capital_before_krw      BIGINT NOT NULL,
    retained_earnings_before_krw        BIGINT NOT NULL,
    distributable_profit_before_krw     BIGINT NOT NULL,
    corporation_ledger_transaction_id   BIGINT UNSIGNED NULL,
    personal_ledger_transaction_id      BIGINT UNSIGNED NULL,
    applied_game_day                    INT UNSIGNED NOT NULL,
    status                              VARCHAR(12) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    applied_at                          DATETIME(3) NULL,
    PRIMARY KEY (id),
    UNIQUE KEY uk_corporation_dissolution_command (save_id, command_id),
    UNIQUE KEY uk_corporation_dissolution_scope (save_id, run_revision, corporation_id, id),
    UNIQUE KEY uk_corporation_dissolution_corp_ledger (corporation_ledger_transaction_id),
    UNIQUE KEY uk_corporation_dissolution_personal_ledger (personal_ledger_transaction_id),
    CONSTRAINT fk_corporation_dissolution_corporation
        FOREIGN KEY (save_id, run_revision, corporation_id)
        REFERENCES corporation (save_id, run_revision, id),
    CONSTRAINT ck_corporation_dissolution CHECK (
        command_id REGEXP '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
        AND distribution_krw BETWEEN 0 AND 9007199254740991
        AND capital_basis_krw BETWEEN 1 AND 9007199254740991
        AND realized_gain_loss_krw = capital_basis_krw - distribution_krw
        AND wallet_before_krw BETWEEN 0 AND 9007199254740991
        AND wallet_after_krw = wallet_before_krw + distribution_krw
        AND cash_before_krw = distribution_krw
        AND contributed_capital_before_krw = capital_basis_krw
        AND retained_earnings_before_krw
            = distribution_krw - contributed_capital_before_krw
        AND distributable_profit_before_krw BETWEEN 0 AND 9007199254740991
        AND status IN ('preparing', 'applied')
        AND ((status = 'preparing' AND corporation_ledger_transaction_id IS NULL
              AND personal_ledger_transaction_id IS NULL AND applied_at IS NULL)
             OR (status = 'applied' AND corporation_ledger_transaction_id IS NOT NULL
                 AND personal_ledger_transaction_id IS NOT NULL AND applied_at IS NOT NULL))
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

ALTER TABLE ledger_transaction
    DROP CHECK ck_ledger_transaction_corporation_source,
    ADD CONSTRAINT ck_ledger_transaction_corporation_source CHECK (
        source_kind NOT LIKE 'corporation%'
        OR source_kind IN (
            'corporationEstablishment', 'corporationCapitalContribution',
            'corporationOfficerPayroll', 'corporationDividend', 'corporationLiquidation'
        )
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
            'withholdingTaxLiability', 'realizedGainLoss'
        )
    );

ALTER TABLE corporation_ledger_transaction
    ADD COLUMN corporation_capital_contribution_id BIGINT UNSIGNED NULL AFTER corporation_dividend_id,
    ADD COLUMN working_capital_loan_id BIGINT UNSIGNED NULL AFTER corporation_capital_contribution_id,
    ADD COLUMN working_capital_loan_repayment_id BIGINT UNSIGNED NULL AFTER working_capital_loan_id,
    ADD COLUMN corporation_dissolution_id BIGINT UNSIGNED NULL AFTER working_capital_loan_repayment_id,
    ADD UNIQUE KEY uk_corporation_ledger_contribution
        (save_id, run_revision, corporation_id, corporation_capital_contribution_id),
    ADD UNIQUE KEY uk_corporation_ledger_working_loan
        (save_id, run_revision, corporation_id, working_capital_loan_id),
    ADD UNIQUE KEY uk_corporation_ledger_working_repayment
        (save_id, run_revision, corporation_id, working_capital_loan_repayment_id),
    ADD UNIQUE KEY uk_corporation_ledger_dissolution
        (save_id, run_revision, corporation_id, corporation_dissolution_id),
    ADD CONSTRAINT fk_corporation_ledger_contribution
        FOREIGN KEY (save_id, run_revision, corporation_id, corporation_capital_contribution_id)
        REFERENCES corporation_capital_contribution (save_id, run_revision, corporation_id, id),
    ADD CONSTRAINT fk_corporation_ledger_working_loan
        FOREIGN KEY (save_id, run_revision, corporation_id, working_capital_loan_id)
        REFERENCES corporation_working_capital_loan (save_id, run_revision, corporation_id, id),
    ADD CONSTRAINT fk_corporation_ledger_working_repayment
        FOREIGN KEY (save_id, run_revision, corporation_id, working_capital_loan_repayment_id)
        REFERENCES corporation_working_capital_loan_repayment (save_id, run_revision, corporation_id, id),
    ADD CONSTRAINT fk_corporation_ledger_dissolution
        FOREIGN KEY (save_id, run_revision, corporation_id, corporation_dissolution_id)
        REFERENCES corporation_dissolution (save_id, run_revision, corporation_id, id),
    DROP CHECK ck_corporation_ledger_transaction,
    ADD CONSTRAINT ck_corporation_ledger_transaction CHECK (
        transaction_kind IN (
            'establishment', 'monthlyRevenue', 'monthlyExpense', 'officerPayroll',
            'corporateTax', 'dividend', 'capitalContribution',
            'workingCapitalLoanDraw', 'workingCapitalLoanRepayment', 'liquidation'
        )
        AND CHAR_LENGTH(description) BETWEEN 1 AND 255
        AND (
            (transaction_kind IN ('monthlyRevenue', 'monthlyExpense', 'officerPayroll')
             AND correlation_id IS NULL AND operating_month_id IS NOT NULL
             AND corporation_tax_year_id IS NULL AND corporation_dividend_id IS NULL
             AND corporation_capital_contribution_id IS NULL AND working_capital_loan_id IS NULL
             AND working_capital_loan_repayment_id IS NULL AND corporation_dissolution_id IS NULL)
            OR (transaction_kind = 'corporateTax' AND correlation_id IS NULL
                AND operating_month_id IS NULL AND corporation_tax_year_id IS NOT NULL
                AND corporation_dividend_id IS NULL AND corporation_capital_contribution_id IS NULL
                AND working_capital_loan_id IS NULL AND working_capital_loan_repayment_id IS NULL
                AND corporation_dissolution_id IS NULL)
            OR (transaction_kind = 'dividend' AND correlation_id IS NOT NULL
                AND operating_month_id IS NULL AND corporation_tax_year_id IS NULL
                AND corporation_dividend_id IS NOT NULL
                AND corporation_capital_contribution_id IS NULL AND working_capital_loan_id IS NULL
                AND working_capital_loan_repayment_id IS NULL AND corporation_dissolution_id IS NULL)
            OR (transaction_kind = 'establishment' AND correlation_id IS NOT NULL
                AND operating_month_id IS NULL AND corporation_tax_year_id IS NULL
                AND corporation_dividend_id IS NULL AND corporation_capital_contribution_id IS NULL
                AND working_capital_loan_id IS NULL AND working_capital_loan_repayment_id IS NULL
                AND corporation_dissolution_id IS NULL)
            OR (transaction_kind = 'capitalContribution' AND correlation_id IS NOT NULL
                AND corporation_capital_contribution_id IS NOT NULL
                AND operating_month_id IS NULL AND corporation_tax_year_id IS NULL
                AND corporation_dividend_id IS NULL AND working_capital_loan_id IS NULL
                AND working_capital_loan_repayment_id IS NULL AND corporation_dissolution_id IS NULL)
            OR (transaction_kind = 'workingCapitalLoanDraw' AND correlation_id IS NOT NULL
                AND working_capital_loan_id IS NOT NULL
                AND operating_month_id IS NULL AND corporation_tax_year_id IS NULL
                AND corporation_dividend_id IS NULL AND corporation_capital_contribution_id IS NULL
                AND working_capital_loan_repayment_id IS NULL AND corporation_dissolution_id IS NULL)
            OR (transaction_kind = 'workingCapitalLoanRepayment' AND correlation_id IS NOT NULL
                AND working_capital_loan_repayment_id IS NOT NULL
                AND operating_month_id IS NULL AND corporation_tax_year_id IS NULL
                AND corporation_dividend_id IS NULL AND corporation_capital_contribution_id IS NULL
                AND working_capital_loan_id IS NULL AND corporation_dissolution_id IS NULL)
            OR (transaction_kind = 'liquidation' AND correlation_id IS NOT NULL
                AND corporation_dissolution_id IS NOT NULL
                AND operating_month_id IS NULL AND corporation_tax_year_id IS NULL
                AND corporation_dividend_id IS NULL AND corporation_capital_contribution_id IS NULL
                AND working_capital_loan_id IS NULL AND working_capital_loan_repayment_id IS NULL)
        )
    );

ALTER TABLE corporation_ledger_posting
    DROP CHECK ck_corporation_ledger_posting,
    ADD CONSTRAINT ck_corporation_ledger_posting CHECK (
        posting_order BETWEEN 1 AND 64
        AND account_code IN (
            'corporationCash', 'contributedCapital', 'operatingRevenue',
            'variableCostExpense', 'fixedCostExpense', 'officerPayrollExpense',
            'withholdingTaxLiability', 'operatingPayable', 'corporateTaxExpense',
            'corporateTaxPayable', 'retainedEarnings', 'dividendDistribution',
            'workingCapitalLoanLiability'
        )
        AND amount_krw <> 0
        AND amount_krw BETWEEN -9007199254740991 AND 9007199254740991
    );

ALTER TABLE corporation_business_month
    ADD COLUMN loan_interest_cost_krw BIGINT NOT NULL DEFAULT 0
        AFTER failed_contract_penalty_krw,
    ADD CONSTRAINT ck_corporation_business_month_loan_interest CHECK (
        loan_interest_cost_krw BETWEEN 0 AND 9007199254740991
    );

ALTER TABLE corporation
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
        AND retained_earnings_krw BETWEEN -9007199254740991 AND 9007199254740991
        AND operating_payable_krw BETWEEN 0 AND 9007199254740991
        AND corporate_tax_payable_krw BETWEEN 0 AND 9007199254740991
        AND distributable_profit_krw BETWEEN 0 AND 9007199254740991
        AND ((status = 'dissolved' AND contributed_capital_krw = 0
              AND cash_krw = 0 AND retained_earnings_krw = 0
              AND operating_payable_krw = 0 AND corporate_tax_payable_krw = 0
              AND distributable_profit_krw = 0)
             OR (status <> 'dissolved' AND contributed_capital_krw >= capital_krw
                 AND contributed_capital_krw <= 9007199254740991))
        AND ((status = 'draft' AND personal_ledger_transaction_id IS NULL
              AND corporation_ledger_transaction_id IS NULL)
             OR (status <> 'draft' AND personal_ledger_transaction_id IS NOT NULL
                 AND corporation_ledger_transaction_id IS NOT NULL))
    );

ALTER TABLE corporation_capital_contribution
    ADD CONSTRAINT fk_corporation_contribution_corp_ledger
        FOREIGN KEY (save_id, run_revision, corporation_id, corporation_ledger_transaction_id)
        REFERENCES corporation_ledger_transaction (save_id, run_revision, corporation_id, id),
    ADD CONSTRAINT fk_corporation_contribution_personal_ledger
        FOREIGN KEY (save_id, run_revision, personal_ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id);

ALTER TABLE corporation_working_capital_loan
    ADD CONSTRAINT fk_working_capital_loan_ledger
        FOREIGN KEY (save_id, run_revision, corporation_id, corporation_ledger_transaction_id)
        REFERENCES corporation_ledger_transaction (save_id, run_revision, corporation_id, id);

ALTER TABLE corporation_working_capital_loan_repayment
    ADD CONSTRAINT fk_working_capital_repayment_ledger
        FOREIGN KEY (save_id, run_revision, corporation_id, corporation_ledger_transaction_id)
        REFERENCES corporation_ledger_transaction (save_id, run_revision, corporation_id, id);

ALTER TABLE corporation_dissolution
    ADD CONSTRAINT fk_corporation_dissolution_corp_ledger
        FOREIGN KEY (save_id, run_revision, corporation_id, corporation_ledger_transaction_id)
        REFERENCES corporation_ledger_transaction (save_id, run_revision, corporation_id, id),
    ADD CONSTRAINT fk_corporation_dissolution_personal_ledger
        FOREIGN KEY (save_id, run_revision, personal_ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id);

CREATE TRIGGER tr_corporation_contribution_apply_only
BEFORE UPDATE ON corporation_capital_contribution
FOR EACH ROW
SET NEW.status = IF(
    OLD.status = 'preparing' AND NEW.status = 'applied'
    AND NEW.id = OLD.id AND NEW.save_id = OLD.save_id
    AND NEW.run_revision = OLD.run_revision AND NEW.corporation_id = OLD.corporation_id
    AND BINARY NEW.command_id = BINARY OLD.command_id
    AND NEW.amount_krw = OLD.amount_krw
    AND NEW.wallet_before_krw = OLD.wallet_before_krw
    AND NEW.wallet_after_krw = OLD.wallet_after_krw
    AND NEW.cash_before_krw = OLD.cash_before_krw AND NEW.cash_after_krw = OLD.cash_after_krw
    AND NEW.contributed_capital_before_krw = OLD.contributed_capital_before_krw
    AND NEW.contributed_capital_after_krw = OLD.contributed_capital_after_krw
    AND OLD.corporation_ledger_transaction_id IS NULL
    AND NEW.corporation_ledger_transaction_id IS NOT NULL
    AND OLD.personal_ledger_transaction_id IS NULL
    AND NEW.personal_ledger_transaction_id IS NOT NULL
    AND NEW.applied_game_day = OLD.applied_game_day AND NEW.created_at = OLD.created_at
    AND OLD.applied_at IS NULL AND NEW.applied_at IS NOT NULL,
    NEW.status,
    NULL
);

CREATE TRIGGER tr_corporation_contribution_no_delete
BEFORE DELETE ON corporation_capital_contribution
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation contributions are immutable';

CREATE TRIGGER tr_working_capital_loan_update_only
BEFORE UPDATE ON corporation_working_capital_loan
FOR EACH ROW
SET NEW.id = IF(
    NEW.id = OLD.id AND NEW.save_id = OLD.save_id
    AND NEW.run_revision = OLD.run_revision AND NEW.corporation_id = OLD.corporation_id
    AND NEW.business_profile_id = OLD.business_profile_id
    AND NEW.business_catalog_version_id = OLD.business_catalog_version_id
    AND NEW.loan_product_id = OLD.loan_product_id
    AND BINARY NEW.command_id = BINARY OLD.command_id
    AND NEW.original_principal_krw = OLD.original_principal_krw
    AND NEW.monthly_interest_rate_ppm = OLD.monthly_interest_rate_ppm
    AND NEW.term_months = OLD.term_months
    AND NEW.originated_year = OLD.originated_year AND NEW.originated_month = OLD.originated_month
    AND NEW.maturity_year = OLD.maturity_year AND NEW.maturity_month = OLD.maturity_month
    AND NEW.personal_guarantee = OLD.personal_guarantee
    AND NEW.cash_before_krw = OLD.cash_before_krw AND NEW.cash_after_krw = OLD.cash_after_krw
    AND NEW.originated_game_day = OLD.originated_game_day AND NEW.created_at = OLD.created_at
    AND (
        (OLD.status = 'preparing' AND NEW.status = 'active'
         AND NEW.outstanding_principal_krw = OLD.outstanding_principal_krw
         AND OLD.corporation_ledger_transaction_id IS NULL
         AND NEW.corporation_ledger_transaction_id IS NOT NULL
         AND OLD.applied_at IS NULL AND NEW.applied_at IS NOT NULL)
        OR (OLD.status = 'active' AND NEW.status = 'matured'
            AND NEW.outstanding_principal_krw = OLD.outstanding_principal_krw
            AND NEW.corporation_ledger_transaction_id = OLD.corporation_ledger_transaction_id
            AND NEW.applied_at = OLD.applied_at
            AND EXISTS (
                SELECT 1 FROM corporation_business_month AS business_month
                WHERE business_month.save_id = OLD.save_id
                  AND business_month.run_revision = OLD.run_revision
                  AND business_month.corporation_id = OLD.corporation_id
                  AND (business_month.operating_year > OLD.maturity_year
                       OR (business_month.operating_year = OLD.maturity_year
                           AND business_month.operating_month >= OLD.maturity_month))
            ))
        OR (OLD.status IN ('active', 'matured') AND NEW.status IN ('active', 'matured', 'repaid')
            AND NEW.corporation_ledger_transaction_id = OLD.corporation_ledger_transaction_id
            AND NEW.applied_at = OLD.applied_at
            AND NEW.outstanding_principal_krw < OLD.outstanding_principal_krw
            AND EXISTS (
                SELECT 1 FROM corporation_working_capital_loan_repayment AS repayment
                WHERE repayment.save_id = OLD.save_id AND repayment.run_revision = OLD.run_revision
                  AND repayment.corporation_id = OLD.corporation_id AND repayment.loan_id = OLD.id
                  AND repayment.status = 'preparing'
                  AND repayment.outstanding_before_krw = OLD.outstanding_principal_krw
                  AND repayment.outstanding_after_krw = NEW.outstanding_principal_krw
                  AND NEW.status = IF(NEW.outstanding_principal_krw = 0, 'repaid', OLD.status)
            ))
    ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_working_capital_loan_no_delete
BEFORE DELETE ON corporation_working_capital_loan
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'working capital loans are immutable';

CREATE TRIGGER tr_working_capital_repayment_apply_only
BEFORE UPDATE ON corporation_working_capital_loan_repayment
FOR EACH ROW
SET NEW.status = IF(
    OLD.status = 'preparing' AND NEW.status = 'applied'
    AND NEW.id = OLD.id AND NEW.save_id = OLD.save_id
    AND NEW.run_revision = OLD.run_revision AND NEW.corporation_id = OLD.corporation_id
    AND NEW.loan_id = OLD.loan_id AND BINARY NEW.command_id = BINARY OLD.command_id
    AND NEW.principal_krw = OLD.principal_krw
    AND NEW.outstanding_before_krw = OLD.outstanding_before_krw
    AND NEW.outstanding_after_krw = OLD.outstanding_after_krw
    AND NEW.cash_before_krw = OLD.cash_before_krw AND NEW.cash_after_krw = OLD.cash_after_krw
    AND OLD.corporation_ledger_transaction_id IS NULL
    AND NEW.corporation_ledger_transaction_id IS NOT NULL
    AND NEW.applied_game_day = OLD.applied_game_day AND NEW.created_at = OLD.created_at
    AND OLD.applied_at IS NULL AND NEW.applied_at IS NOT NULL,
    NEW.status,
    NULL
);

CREATE TRIGGER tr_working_capital_repayment_no_delete
BEFORE DELETE ON corporation_working_capital_loan_repayment
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'working capital repayments are immutable';

CREATE TRIGGER tr_corporation_dissolution_apply_only
BEFORE UPDATE ON corporation_dissolution
FOR EACH ROW
SET NEW.status = IF(
    OLD.status = 'preparing' AND NEW.status = 'applied'
    AND NEW.id = OLD.id AND NEW.save_id = OLD.save_id
    AND NEW.run_revision = OLD.run_revision AND NEW.corporation_id = OLD.corporation_id
    AND BINARY NEW.command_id = BINARY OLD.command_id
    AND NEW.distribution_krw = OLD.distribution_krw
    AND NEW.capital_basis_krw = OLD.capital_basis_krw
    AND NEW.realized_gain_loss_krw = OLD.realized_gain_loss_krw
    AND NEW.wallet_before_krw = OLD.wallet_before_krw
    AND NEW.wallet_after_krw = OLD.wallet_after_krw
    AND NEW.cash_before_krw = OLD.cash_before_krw
    AND NEW.contributed_capital_before_krw = OLD.contributed_capital_before_krw
    AND NEW.retained_earnings_before_krw = OLD.retained_earnings_before_krw
    AND NEW.distributable_profit_before_krw = OLD.distributable_profit_before_krw
    AND OLD.corporation_ledger_transaction_id IS NULL
    AND NEW.corporation_ledger_transaction_id IS NOT NULL
    AND OLD.personal_ledger_transaction_id IS NULL
    AND NEW.personal_ledger_transaction_id IS NOT NULL
    AND NEW.applied_game_day = OLD.applied_game_day AND NEW.created_at = OLD.created_at
    AND OLD.applied_at IS NULL AND NEW.applied_at IS NOT NULL,
    NEW.status,
    NULL
);

CREATE TRIGGER tr_corporation_dissolution_no_delete
BEFORE DELETE ON corporation_dissolution
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation dissolutions are immutable';

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
    ))
    OR (NEW.source_kind = 'corporationCapitalContribution'
        AND NEW.source_id REGEXP '^[1-9][0-9]*$' AND EXISTS (
        SELECT 1 FROM corporation_capital_contribution AS contribution
        INNER JOIN run_rule_bundle AS bundle
          ON bundle.save_id = contribution.save_id AND bundle.run_revision = contribution.run_revision
        WHERE contribution.id = CAST(NEW.source_id AS UNSIGNED)
          AND contribution.save_id = NEW.save_id AND contribution.run_revision = NEW.run_revision
          AND contribution.status = 'preparing' AND contribution.applied_game_day = NEW.game_day
          AND bundle.policy_set_id = NEW.policy_set_id
    ))
    OR (NEW.source_kind = 'corporationLiquidation'
        AND NEW.source_id REGEXP '^[1-9][0-9]*$' AND EXISTS (
        SELECT 1 FROM corporation_dissolution AS dissolution
        INNER JOIN run_rule_bundle AS bundle
          ON bundle.save_id = dissolution.save_id AND bundle.run_revision = dissolution.run_revision
        WHERE dissolution.id = CAST(NEW.source_id AS UNSIGNED)
          AND dissolution.save_id = NEW.save_id AND dissolution.run_revision = NEW.run_revision
          AND dissolution.status = 'preparing' AND dissolution.applied_game_day = NEW.game_day
          AND bundle.policy_set_id = NEW.policy_set_id
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
              'corporationEstablishment', 'corporationCapitalContribution',
              'corporationOfficerPayroll', 'corporationDividend', 'corporationLiquidation'
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
    )
    OR EXISTS (
        SELECT 1 FROM ledger_transaction AS ledger
        INNER JOIN corporation_capital_contribution AS contribution
          ON contribution.id = CAST(ledger.source_id AS UNSIGNED)
         AND contribution.save_id = ledger.save_id
         AND contribution.run_revision = ledger.run_revision
        WHERE ledger.id = NEW.ledger_transaction_id
          AND ledger.save_id = NEW.save_id AND ledger.run_revision = NEW.run_revision
          AND ledger.source_kind = 'corporationCapitalContribution'
          AND contribution.corporation_id = NEW.corporation_id
          AND contribution.status = 'preparing'
          AND ((NEW.posting_order = 1 AND NEW.account_code = 'corporationInvestmentAsset'
                AND NEW.amount_krw = contribution.amount_krw)
               OR (NEW.posting_order = 2 AND NEW.account_code = 'wallet'
                   AND NEW.amount_krw = -contribution.amount_krw))
    )
    OR EXISTS (
        SELECT 1 FROM ledger_transaction AS ledger
        INNER JOIN corporation_dissolution AS dissolution
          ON dissolution.id = CAST(ledger.source_id AS UNSIGNED)
         AND dissolution.save_id = ledger.save_id
         AND dissolution.run_revision = ledger.run_revision
        WHERE ledger.id = NEW.ledger_transaction_id
          AND ledger.save_id = NEW.save_id AND ledger.run_revision = NEW.run_revision
          AND ledger.source_kind = 'corporationLiquidation'
          AND dissolution.corporation_id = NEW.corporation_id
          AND dissolution.status = 'preparing'
          AND ((NEW.account_code = 'wallet' AND NEW.amount_krw = dissolution.distribution_krw
                AND dissolution.distribution_krw > 0)
               OR (NEW.account_code = 'corporationInvestmentAsset'
                   AND NEW.amount_krw = -dissolution.capital_basis_krw)
               OR (NEW.account_code = 'realizedGainLoss'
                   AND NEW.amount_krw = dissolution.realized_gain_loss_krw
                   AND dissolution.realized_gain_loss_krw <> 0))
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
    ))
    OR (NEW.transaction_kind = 'capitalContribution' AND EXISTS (
        SELECT 1 FROM corporation_capital_contribution AS contribution
        WHERE contribution.id = NEW.corporation_capital_contribution_id
          AND contribution.save_id = NEW.save_id AND contribution.run_revision = NEW.run_revision
          AND contribution.corporation_id = NEW.corporation_id
          AND BINARY contribution.command_id = BINARY NEW.correlation_id
          AND contribution.applied_game_day = NEW.game_day AND contribution.status = 'preparing'
    ))
    OR (NEW.transaction_kind = 'workingCapitalLoanDraw' AND EXISTS (
        SELECT 1 FROM corporation_working_capital_loan AS loan
        WHERE loan.id = NEW.working_capital_loan_id
          AND loan.save_id = NEW.save_id AND loan.run_revision = NEW.run_revision
          AND loan.corporation_id = NEW.corporation_id
          AND BINARY loan.command_id = BINARY NEW.correlation_id
          AND loan.originated_game_day = NEW.game_day AND loan.status = 'preparing'
    ))
    OR (NEW.transaction_kind = 'workingCapitalLoanRepayment' AND EXISTS (
        SELECT 1 FROM corporation_working_capital_loan_repayment AS repayment
        WHERE repayment.id = NEW.working_capital_loan_repayment_id
          AND repayment.save_id = NEW.save_id AND repayment.run_revision = NEW.run_revision
          AND repayment.corporation_id = NEW.corporation_id
          AND BINARY repayment.command_id = BINARY NEW.correlation_id
          AND repayment.applied_game_day = NEW.game_day AND repayment.status = 'preparing'
    ))
    OR (NEW.transaction_kind = 'liquidation' AND EXISTS (
        SELECT 1 FROM corporation_dissolution AS dissolution
        WHERE dissolution.id = NEW.corporation_dissolution_id
          AND dissolution.save_id = NEW.save_id AND dissolution.run_revision = NEW.run_revision
          AND dissolution.corporation_id = NEW.corporation_id
          AND BINARY dissolution.command_id = BINARY NEW.correlation_id
          AND dissolution.applied_game_day = NEW.game_day AND dissolution.status = 'preparing'
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
    )
    OR EXISTS (
        SELECT 1 FROM corporation_ledger_transaction AS ledger
        INNER JOIN corporation_capital_contribution AS contribution
          ON contribution.id = ledger.corporation_capital_contribution_id
         AND contribution.save_id = ledger.save_id
         AND contribution.run_revision = ledger.run_revision
         AND contribution.corporation_id = ledger.corporation_id
        WHERE ledger.id = NEW.corporation_ledger_transaction_id
          AND ledger.save_id = NEW.save_id AND ledger.run_revision = NEW.run_revision
          AND ledger.corporation_id = NEW.corporation_id
          AND ledger.transaction_kind = 'capitalContribution'
          AND contribution.status = 'preparing'
          AND ((NEW.posting_order = 1 AND NEW.account_code = 'corporationCash'
                AND NEW.amount_krw = contribution.amount_krw)
               OR (NEW.posting_order = 2 AND NEW.account_code = 'contributedCapital'
                   AND NEW.amount_krw = -contribution.amount_krw))
    )
    OR EXISTS (
        SELECT 1 FROM corporation_ledger_transaction AS ledger
        INNER JOIN corporation_working_capital_loan AS loan
          ON loan.id = ledger.working_capital_loan_id
         AND loan.save_id = ledger.save_id AND loan.run_revision = ledger.run_revision
         AND loan.corporation_id = ledger.corporation_id
        WHERE ledger.id = NEW.corporation_ledger_transaction_id
          AND ledger.save_id = NEW.save_id AND ledger.run_revision = NEW.run_revision
          AND ledger.corporation_id = NEW.corporation_id
          AND ledger.transaction_kind = 'workingCapitalLoanDraw' AND loan.status = 'preparing'
          AND ((NEW.posting_order = 1 AND NEW.account_code = 'corporationCash'
                AND NEW.amount_krw = loan.original_principal_krw)
               OR (NEW.posting_order = 2 AND NEW.account_code = 'workingCapitalLoanLiability'
                   AND NEW.amount_krw = -loan.original_principal_krw))
    )
    OR EXISTS (
        SELECT 1 FROM corporation_ledger_transaction AS ledger
        INNER JOIN corporation_working_capital_loan_repayment AS repayment
          ON repayment.id = ledger.working_capital_loan_repayment_id
         AND repayment.save_id = ledger.save_id AND repayment.run_revision = ledger.run_revision
         AND repayment.corporation_id = ledger.corporation_id
        WHERE ledger.id = NEW.corporation_ledger_transaction_id
          AND ledger.save_id = NEW.save_id AND ledger.run_revision = NEW.run_revision
          AND ledger.corporation_id = NEW.corporation_id
          AND ledger.transaction_kind = 'workingCapitalLoanRepayment'
          AND repayment.status = 'preparing'
          AND ((NEW.posting_order = 1 AND NEW.account_code = 'workingCapitalLoanLiability'
                AND NEW.amount_krw = repayment.principal_krw)
               OR (NEW.posting_order = 2 AND NEW.account_code = 'corporationCash'
                   AND NEW.amount_krw = -repayment.principal_krw))
    )
    OR EXISTS (
        SELECT 1 FROM corporation_ledger_transaction AS ledger
        INNER JOIN corporation_dissolution AS dissolution
          ON dissolution.id = ledger.corporation_dissolution_id
         AND dissolution.save_id = ledger.save_id
         AND dissolution.run_revision = ledger.run_revision
         AND dissolution.corporation_id = ledger.corporation_id
        WHERE ledger.id = NEW.corporation_ledger_transaction_id
          AND ledger.save_id = NEW.save_id AND ledger.run_revision = NEW.run_revision
          AND ledger.corporation_id = NEW.corporation_id
          AND ledger.transaction_kind = 'liquidation' AND dissolution.status = 'preparing'
          AND ((NEW.account_code = 'corporationCash'
                AND NEW.amount_krw = -dissolution.distribution_krw
                AND dissolution.distribution_krw > 0)
               OR (NEW.account_code = 'contributedCapital'
                   AND NEW.amount_krw = dissolution.capital_basis_krw)
               OR (NEW.account_code = 'retainedEarnings'
                   AND NEW.amount_krw = dissolution.retained_earnings_before_krw
                   AND dissolution.retained_earnings_before_krw <> 0))
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
    AND NEW.created_at = OLD.created_at
    AND (
        (OLD.status = 'draft' AND NEW.status = 'active'
         AND NEW.cash_krw = OLD.capital_krw AND NEW.contributed_capital_krw = OLD.capital_krw
         AND NEW.retained_earnings_krw = 0
         AND NEW.operating_payable_krw = 0 AND NEW.corporate_tax_payable_krw = 0
         AND NEW.distributable_profit_krw = 0
         AND NEW.personal_ledger_transaction_id IS NOT NULL
         AND NEW.corporation_ledger_transaction_id IS NOT NULL)
        OR (OLD.status = 'active' AND NEW.status IN ('active', 'insolvent')
            AND NEW.contributed_capital_krw = OLD.contributed_capital_krw
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
            AND NEW.contributed_capital_krw = OLD.contributed_capital_krw
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
            AND NEW.contributed_capital_krw = OLD.contributed_capital_krw
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
        OR (OLD.status = 'active' AND NEW.status = 'active'
            AND NEW.personal_ledger_transaction_id = OLD.personal_ledger_transaction_id
            AND NEW.corporation_ledger_transaction_id = OLD.corporation_ledger_transaction_id
            AND NEW.retained_earnings_krw = OLD.retained_earnings_krw
            AND NEW.operating_payable_krw = OLD.operating_payable_krw
            AND NEW.corporate_tax_payable_krw = OLD.corporate_tax_payable_krw
            AND NEW.distributable_profit_krw = OLD.distributable_profit_krw
            AND EXISTS (
                SELECT 1 FROM corporation_capital_contribution AS contribution
                WHERE contribution.save_id = OLD.save_id
                  AND contribution.run_revision = OLD.run_revision
                  AND contribution.corporation_id = OLD.id AND contribution.status = 'preparing'
                  AND contribution.cash_before_krw = OLD.cash_krw
                  AND contribution.cash_after_krw = NEW.cash_krw
                  AND contribution.contributed_capital_before_krw = OLD.contributed_capital_krw
                  AND contribution.contributed_capital_after_krw = NEW.contributed_capital_krw
            ))
        OR (OLD.status = 'active' AND NEW.status = 'active'
            AND NEW.contributed_capital_krw = OLD.contributed_capital_krw
            AND NEW.personal_ledger_transaction_id = OLD.personal_ledger_transaction_id
            AND NEW.corporation_ledger_transaction_id = OLD.corporation_ledger_transaction_id
            AND NEW.retained_earnings_krw = OLD.retained_earnings_krw
            AND NEW.operating_payable_krw = OLD.operating_payable_krw
            AND NEW.corporate_tax_payable_krw = OLD.corporate_tax_payable_krw
            AND NEW.distributable_profit_krw = OLD.distributable_profit_krw
            AND EXISTS (
                SELECT 1 FROM corporation_working_capital_loan AS loan
                WHERE loan.save_id = OLD.save_id AND loan.run_revision = OLD.run_revision
                  AND loan.corporation_id = OLD.id AND loan.status = 'preparing'
                  AND loan.cash_before_krw = OLD.cash_krw AND loan.cash_after_krw = NEW.cash_krw
            ))
        OR (OLD.status IN ('active', 'insolvent') AND NEW.status = OLD.status
            AND NEW.contributed_capital_krw = OLD.contributed_capital_krw
            AND NEW.personal_ledger_transaction_id = OLD.personal_ledger_transaction_id
            AND NEW.corporation_ledger_transaction_id = OLD.corporation_ledger_transaction_id
            AND NEW.retained_earnings_krw = OLD.retained_earnings_krw
            AND NEW.operating_payable_krw = OLD.operating_payable_krw
            AND NEW.corporate_tax_payable_krw = OLD.corporate_tax_payable_krw
            AND NEW.distributable_profit_krw = OLD.distributable_profit_krw
            AND EXISTS (
                SELECT 1 FROM corporation_working_capital_loan_repayment AS repayment
                WHERE repayment.save_id = OLD.save_id AND repayment.run_revision = OLD.run_revision
                  AND repayment.corporation_id = OLD.id AND repayment.status = 'preparing'
                  AND repayment.cash_before_krw = OLD.cash_krw
                  AND repayment.cash_after_krw = NEW.cash_krw
            ))
        OR (OLD.status IN ('active', 'dormant', 'insolvent') AND NEW.status = 'dissolved'
            AND NEW.cash_krw = 0 AND NEW.contributed_capital_krw = 0
            AND NEW.retained_earnings_krw = 0 AND NEW.operating_payable_krw = 0
            AND NEW.corporate_tax_payable_krw = 0 AND NEW.distributable_profit_krw = 0
            AND NEW.personal_ledger_transaction_id = OLD.personal_ledger_transaction_id
            AND NEW.corporation_ledger_transaction_id = OLD.corporation_ledger_transaction_id
            AND EXISTS (
                SELECT 1 FROM corporation_dissolution AS dissolution
                WHERE dissolution.save_id = OLD.save_id
                  AND dissolution.run_revision = OLD.run_revision
                  AND dissolution.corporation_id = OLD.id AND dissolution.status = 'preparing'
                  AND dissolution.cash_before_krw = OLD.cash_krw
                  AND dissolution.contributed_capital_before_krw = OLD.contributed_capital_krw
                  AND dissolution.retained_earnings_before_krw = OLD.retained_earnings_krw
                  AND dissolution.distributable_profit_before_krw = OLD.distributable_profit_krw
            ))
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
              AND (operating_month.operating_cost_payable_krw > 0
                   OR operating_month.payroll_payable_krw > 0)
        )
    )
    OR (
        NEW.transition_no BETWEEN 3 AND 64
        AND NEW.from_status IN ('active', 'dormant', 'insolvent')
        AND NEW.to_status = 'dissolved'
        AND NEW.transition_reason = 'playerDissolved'
        AND NEW.command_id IS NOT NULL
        AND EXISTS (
            SELECT 1 FROM corporation_dissolution AS dissolution
            INNER JOIN corporation AS corporation_row
              ON corporation_row.save_id = dissolution.save_id
             AND corporation_row.run_revision = dissolution.run_revision
             AND corporation_row.id = dissolution.corporation_id
            WHERE dissolution.save_id = NEW.save_id
              AND dissolution.run_revision = NEW.run_revision
              AND dissolution.corporation_id = NEW.corporation_id
              AND BINARY dissolution.command_id = BINARY NEW.command_id
              AND dissolution.status = 'applied'
              AND dissolution.applied_game_day = NEW.transition_game_day
              AND corporation_row.status = 'dissolved'
        )
    ),
    NEW.transition_no,
    NULL
);
