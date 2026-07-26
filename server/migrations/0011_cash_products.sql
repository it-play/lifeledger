-- M2-B immutable cash-product catalog, run-scoped contracts, and annual income totals.

CREATE TABLE financial_institution (
    id                      BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    institution_key         VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name            VARCHAR(100)    NOT NULL,
    is_deposit_insured      BOOLEAN         NOT NULL,
    created_at              DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_financial_institution_key (institution_key),
    CONSTRAINT ck_financial_institution_key CHECK (CHAR_LENGTH(institution_key) > 0),
    CONSTRAINT ck_financial_institution_name CHECK (CHAR_LENGTH(display_name) > 0),
    CONSTRAINT ck_financial_institution_insured CHECK (
        is_deposit_insured IN (FALSE, TRUE)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_financial_institution_no_update
BEFORE UPDATE ON financial_institution
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'financial institutions are immutable';

CREATE TRIGGER tr_financial_institution_no_delete
BEFORE DELETE ON financial_institution
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'financial institutions are immutable';

CREATE TABLE cash_product_version (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    product_key                         VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                        VARCHAR(100)    NOT NULL,
    product_kind                        VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    institution_id                      BIGINT UNSIGNED NOT NULL,
    is_deposit_protection_eligible      BOOLEAN         NOT NULL,
    rate_reference                      VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    spread_bp                           SMALLINT         NOT NULL,
    minimum_interest_balance_krw        BIGINT              NULL,
    minimum_amount_krw                  BIGINT              NULL,
    maximum_amount_krw                  BIGINT              NULL,
    term_days                           SMALLINT UNSIGNED    NULL,
    term_months                         SMALLINT UNSIGNED    NULL,
    installment_count                   SMALLINT UNSIGNED    NULL,
    early_termination_rate_bp           SMALLINT UNSIGNED    NULL,
    day_count_denominator               SMALLINT UNSIGNED NOT NULL,
    created_at                          DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_cash_product_version_key (product_key),
    KEY ix_cash_product_version_institution_kind (institution_id, product_kind, id),
    CONSTRAINT fk_cash_product_version_institution
        FOREIGN KEY (institution_id) REFERENCES financial_institution (id),
    CONSTRAINT ck_cash_product_version_key CHECK (CHAR_LENGTH(product_key) > 0),
    CONSTRAINT ck_cash_product_version_name CHECK (CHAR_LENGTH(display_name) > 0),
    CONSTRAINT ck_cash_product_version_rate_reference CHECK (
        rate_reference = 'treasury3mBp'
    ),
    CONSTRAINT ck_cash_product_version_rate CHECK (
        spread_bp BETWEEN -10000 AND 10000
        AND (
            early_termination_rate_bp IS NULL
            OR early_termination_rate_bp <= 10000
        )
    ),
    CONSTRAINT ck_cash_product_version_protection CHECK (
        is_deposit_protection_eligible IN (FALSE, TRUE)
    ),
    CONSTRAINT ck_cash_product_version_day_count CHECK (day_count_denominator > 0),
    CONSTRAINT ck_cash_product_version_kind_shape CHECK (
        (
            product_kind IN ('cmaRp', 'cmaIssuedNote')
            AND is_deposit_protection_eligible = FALSE
            AND minimum_interest_balance_krw > 0
            AND minimum_amount_krw IS NULL
            AND maximum_amount_krw IS NULL
            AND term_days IS NULL
            AND term_months IS NULL
            AND installment_count IS NULL
            AND early_termination_rate_bp IS NULL
        )
        OR
        (
            product_kind = 'termDeposit'
            AND is_deposit_protection_eligible = TRUE
            AND minimum_interest_balance_krw IS NULL
            AND minimum_amount_krw > 0
            AND maximum_amount_krw >= minimum_amount_krw
            AND term_days > 0
            AND term_months IS NULL
            AND installment_count IS NULL
            AND early_termination_rate_bp IS NOT NULL
        )
        OR
        (
            product_kind = 'installmentSavings'
            AND is_deposit_protection_eligible = TRUE
            AND minimum_interest_balance_krw IS NULL
            AND minimum_amount_krw > 0
            AND maximum_amount_krw >= minimum_amount_krw
            AND term_days IS NULL
            AND term_months > 0
            AND installment_count > 0
            AND early_termination_rate_bp IS NOT NULL
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

-- A protected product can only belong to an insured institution.
CREATE TRIGGER tr_cash_product_version_valid_insert
BEFORE INSERT ON cash_product_version
FOR EACH ROW
SET NEW.institution_id = IF(
    EXISTS (
        SELECT 1
        FROM financial_institution AS institution
        WHERE institution.id = NEW.institution_id
          AND (
              NEW.is_deposit_protection_eligible = FALSE
              OR institution.is_deposit_insured = TRUE
          )
    ),
    NEW.institution_id,
    NULL
);

CREATE TRIGGER tr_cash_product_version_no_update
BEFORE UPDATE ON cash_product_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'cash product versions are immutable';

CREATE TRIGGER tr_cash_product_version_no_delete
BEFORE DELETE ON cash_product_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'cash product versions are immutable';

CREATE TABLE cma_account_contract (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    financial_account_id            BIGINT UNSIGNED NOT NULL,
    product_version_id              BIGINT UNSIGNED NOT NULL,
    rate_reference                  VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    spread_bp                       SMALLINT         NOT NULL,
    minimum_interest_balance_krw    BIGINT           NOT NULL,
    day_count_denominator           SMALLINT UNSIGNED NOT NULL,
    interest_remainder              BIGINT           NOT NULL DEFAULT 0,
    created_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_cma_account_contract_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_cma_account_contract_account
        (save_id, run_revision, financial_account_id),
    KEY ix_cma_account_contract_product_version (product_version_id),
    CONSTRAINT fk_cma_account_contract_account
        FOREIGN KEY (save_id, run_revision, financial_account_id)
        REFERENCES financial_account (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_cma_account_contract_product_version
        FOREIGN KEY (product_version_id) REFERENCES cash_product_version (id),
    CONSTRAINT ck_cma_account_contract_rate_reference CHECK (
        rate_reference = 'treasury3mBp'
    ),
    CONSTRAINT ck_cma_account_contract_minimum_balance CHECK (
        minimum_interest_balance_krw > 0
    ),
    CONSTRAINT ck_cma_account_contract_day_count CHECK (day_count_denominator > 0),
    CONSTRAINT ck_cma_account_contract_remainder CHECK (
        interest_remainder >= 0
        AND interest_remainder < day_count_denominator * 10000
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

-- The copied terms make an opened CMA independent from later catalog reads.
CREATE TRIGGER tr_cma_account_contract_valid_insert
BEFORE INSERT ON cma_account_contract
FOR EACH ROW
SET NEW.product_version_id = IF(
    NEW.interest_remainder = 0
        AND EXISTS (
            SELECT 1
            FROM financial_account AS account
            WHERE account.id = NEW.financial_account_id
              AND account.save_id = NEW.save_id
              AND account.run_revision = NEW.run_revision
              AND account.account_type = 'cma'
              AND account.status = 'open'
        )
        AND EXISTS (
            SELECT 1
            FROM cash_product_version AS product
            WHERE product.id = NEW.product_version_id
              AND product.product_kind IN ('cmaRp', 'cmaIssuedNote')
              AND BINARY product.rate_reference = BINARY NEW.rate_reference
              AND product.spread_bp = NEW.spread_bp
              AND product.minimum_interest_balance_krw = NEW.minimum_interest_balance_krw
              AND product.day_count_denominator = NEW.day_count_denominator
        ),
    NEW.product_version_id,
    NULL
);

-- Only the fixed-point remainder changes during daily accrual.
CREATE TRIGGER tr_cma_account_contract_remainder_only
BEFORE UPDATE ON cma_account_contract
FOR EACH ROW
SET NEW.id = IF(
    NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.financial_account_id = OLD.financial_account_id
        AND NEW.product_version_id = OLD.product_version_id
        AND BINARY NEW.rate_reference = BINARY OLD.rate_reference
        AND NEW.spread_bp = OLD.spread_bp
        AND NEW.minimum_interest_balance_krw = OLD.minimum_interest_balance_krw
        AND NEW.day_count_denominator = OLD.day_count_denominator
        AND NEW.created_at = OLD.created_at,
    OLD.id,
    NULL
);

CREATE TRIGGER tr_cma_account_contract_no_delete
BEFORE DELETE ON cma_account_contract
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'CMA contracts cannot be deleted';

CREATE TABLE cash_product_contract (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED    NOT NULL,
    financial_account_id                BIGINT UNSIGNED NOT NULL,
    product_version_id                  BIGINT UNSIGNED NOT NULL,
    contract_kind                       VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    status                              VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    principal_krw                       BIGINT               NULL,
    installment_amount_krw              BIGINT               NULL,
    term_days                           SMALLINT UNSIGNED     NULL,
    term_months                         SMALLINT UNSIGNED     NULL,
    installment_count                   SMALLINT UNSIGNED     NULL,
    annual_rate_bp                      INT              NOT NULL,
    early_termination_rate_bp           SMALLINT UNSIGNED NOT NULL,
    day_count_denominator               SMALLINT UNSIGNED NOT NULL,
    opened_game_day                     INT UNSIGNED     NOT NULL,
    maturity_game_day                   INT UNSIGNED     NOT NULL,
    closed_game_day                     INT UNSIGNED         NULL,
    closing_ledger_transaction_id       BIGINT UNSIGNED      NULL,
    cancellation_reason                 VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_cash_product_contract_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_cash_product_contract_closing_ledger
        (save_id, run_revision, closing_ledger_transaction_id),
    KEY ix_cash_product_contract_account_status
        (save_id, run_revision, financial_account_id, status, id),
    KEY ix_cash_product_contract_product_version (product_version_id),
    CONSTRAINT fk_cash_product_contract_account
        FOREIGN KEY (save_id, run_revision, financial_account_id)
        REFERENCES financial_account (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_cash_product_contract_product_version
        FOREIGN KEY (product_version_id) REFERENCES cash_product_version (id),
    CONSTRAINT fk_cash_product_contract_closing_ledger
        FOREIGN KEY (save_id, run_revision, closing_ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT ck_cash_product_contract_rate CHECK (
        annual_rate_bp BETWEEN 0 AND 10000
        AND early_termination_rate_bp <= annual_rate_bp
        AND day_count_denominator > 0
    ),
    CONSTRAINT ck_cash_product_contract_period CHECK (maturity_game_day > opened_game_day),
    CONSTRAINT ck_cash_product_contract_kind_shape CHECK (
        (
            contract_kind = 'termDeposit'
            AND principal_krw > 0
            AND installment_amount_krw IS NULL
            AND term_days > 0
            AND term_months IS NULL
            AND installment_count IS NULL
        )
        OR
        (
            contract_kind = 'installmentSavings'
            AND principal_krw IS NULL
            AND installment_amount_krw > 0
            AND term_days IS NULL
            AND term_months > 0
            AND installment_count > 0
        )
    ),
    CONSTRAINT ck_cash_product_contract_status CHECK (
        status IN ('active', 'matured', 'closedEarly', 'cancelled')
    ),
    CONSTRAINT ck_cash_product_contract_state_shape CHECK (
        (
            status = 'active'
            AND closed_game_day IS NULL
            AND closing_ledger_transaction_id IS NULL
            AND cancellation_reason IS NULL
        )
        OR
        (
            status = 'matured'
            AND closed_game_day = maturity_game_day
            AND closing_ledger_transaction_id IS NOT NULL
            AND cancellation_reason IS NULL
        )
        OR
        (
            status = 'closedEarly'
            AND closed_game_day >= opened_game_day
            AND closed_game_day < maturity_game_day
            AND closing_ledger_transaction_id IS NOT NULL
            AND cancellation_reason IS NULL
        )
        OR
        (
            status = 'cancelled'
            AND closed_game_day >= opened_game_day
            AND closing_ledger_transaction_id IS NULL
            AND CHAR_LENGTH(cancellation_reason) > 0
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

-- Contracts may only draw from an open taxable brokerage account, and all catalog terms are copied.
CREATE TRIGGER tr_cash_product_contract_valid_insert
BEFORE INSERT ON cash_product_contract
FOR EACH ROW
SET NEW.product_version_id = IF(
    NEW.status = 'active'
        AND NEW.closed_game_day IS NULL
        AND NEW.closing_ledger_transaction_id IS NULL
        AND NEW.cancellation_reason IS NULL
        AND EXISTS (
            SELECT 1
            FROM financial_account AS account
            WHERE account.id = NEW.financial_account_id
              AND account.save_id = NEW.save_id
              AND account.run_revision = NEW.run_revision
              AND account.account_type = 'taxableBrokerage'
              AND account.status = 'open'
        )
        AND EXISTS (
            SELECT 1
            FROM cash_product_version AS product
            WHERE product.id = NEW.product_version_id
              AND BINARY product.product_kind = BINARY NEW.contract_kind
              AND product.early_termination_rate_bp = NEW.early_termination_rate_bp
              AND product.day_count_denominator = NEW.day_count_denominator
              AND (
                  (
                      NEW.contract_kind = 'termDeposit'
                      AND NEW.principal_krw BETWEEN
                          product.minimum_amount_krw AND product.maximum_amount_krw
                      AND product.term_days = NEW.term_days
                      AND NEW.maturity_game_day = NEW.opened_game_day + NEW.term_days
                  )
                  OR
                  (
                      NEW.contract_kind = 'installmentSavings'
                      AND NEW.installment_amount_krw BETWEEN
                          product.minimum_amount_krw AND product.maximum_amount_krw
                      AND product.term_months = NEW.term_months
                      AND product.installment_count = NEW.installment_count
                  )
              )
        ),
    NEW.product_version_id,
    NULL
);

-- An active contract makes one terminal transition while its copied terms remain fixed.
CREATE TRIGGER tr_cash_product_contract_transition_only
BEFORE UPDATE ON cash_product_contract
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'active'
        AND NEW.status IN ('matured', 'closedEarly', 'cancelled')
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.financial_account_id = OLD.financial_account_id
        AND NEW.product_version_id = OLD.product_version_id
        AND BINARY NEW.contract_kind = BINARY OLD.contract_kind
        AND NEW.principal_krw <=> OLD.principal_krw
        AND NEW.installment_amount_krw <=> OLD.installment_amount_krw
        AND NEW.term_days <=> OLD.term_days
        AND NEW.term_months <=> OLD.term_months
        AND NEW.installment_count <=> OLD.installment_count
        AND NEW.annual_rate_bp = OLD.annual_rate_bp
        AND NEW.early_termination_rate_bp = OLD.early_termination_rate_bp
        AND NEW.day_count_denominator = OLD.day_count_denominator
        AND NEW.opened_game_day = OLD.opened_game_day
        AND NEW.maturity_game_day = OLD.maturity_game_day
        AND NEW.created_at = OLD.created_at,
    OLD.id,
    NULL
);

CREATE TRIGGER tr_cash_product_contract_no_delete
BEFORE DELETE ON cash_product_contract
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'cash product contracts cannot be deleted';

CREATE TABLE savings_installment (
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    contract_id                     BIGINT UNSIGNED NOT NULL,
    installment_no                  SMALLINT UNSIGNED NOT NULL,
    due_game_day                    INT UNSIGNED    NOT NULL,
    amount_krw                      BIGINT          NOT NULL,
    status                          VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    processed_game_day              INT UNSIGNED        NULL,
    ledger_transaction_id           BIGINT UNSIGNED     NULL,
    cancellation_reason             VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    created_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (contract_id, installment_no),
    UNIQUE KEY uk_savings_installment_ledger
        (save_id, run_revision, ledger_transaction_id),
    KEY ix_savings_installment_due
        (save_id, run_revision, status, due_game_day, contract_id, installment_no),
    CONSTRAINT fk_savings_installment_contract
        FOREIGN KEY (save_id, run_revision, contract_id)
        REFERENCES cash_product_contract (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_savings_installment_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT ck_savings_installment_number CHECK (installment_no > 0),
    CONSTRAINT ck_savings_installment_amount CHECK (amount_krw > 0),
    CONSTRAINT ck_savings_installment_status CHECK (
        status IN ('pending', 'paid', 'missed', 'cancelled')
    ),
    CONSTRAINT ck_savings_installment_state_shape CHECK (
        (
            status = 'pending'
            AND processed_game_day IS NULL
            AND ledger_transaction_id IS NULL
            AND cancellation_reason IS NULL
        )
        OR
        (
            status = 'paid'
            AND processed_game_day IS NOT NULL
            AND ledger_transaction_id IS NOT NULL
            AND cancellation_reason IS NULL
        )
        OR
        (
            status = 'missed'
            AND processed_game_day IS NOT NULL
            AND ledger_transaction_id IS NULL
            AND cancellation_reason IS NULL
        )
        OR
        (
            status = 'cancelled'
            AND processed_game_day IS NOT NULL
            AND ledger_transaction_id IS NULL
            AND CHAR_LENGTH(cancellation_reason) > 0
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

-- Enrollment inserts installment one as paid and every later installment as pending.
CREATE TRIGGER tr_savings_installment_valid_insert
BEFORE INSERT ON savings_installment
FOR EACH ROW
SET NEW.contract_id = IF(
    EXISTS (
        SELECT 1
        FROM cash_product_contract AS contract
        WHERE contract.id = NEW.contract_id
          AND contract.save_id = NEW.save_id
          AND contract.run_revision = NEW.run_revision
          AND contract.contract_kind = 'installmentSavings'
          AND contract.status = 'active'
          AND NEW.installment_no <= contract.installment_count
          AND NEW.amount_krw = contract.installment_amount_krw
          AND NEW.due_game_day < contract.maturity_game_day
          AND (
              (
                  NEW.installment_no = 1
                  AND NEW.due_game_day = contract.opened_game_day
                  AND NEW.status = 'paid'
                  AND NEW.processed_game_day = contract.opened_game_day
                  AND NEW.ledger_transaction_id IS NOT NULL
                  AND NEW.cancellation_reason IS NULL
              )
              OR
              (
                  NEW.installment_no > 1
                  AND NEW.due_game_day > contract.opened_game_day
                  AND NEW.status = 'pending'
                  AND NEW.processed_game_day IS NULL
                  AND NEW.ledger_transaction_id IS NULL
                  AND NEW.cancellation_reason IS NULL
              )
          )
    ),
    NEW.contract_id,
    NULL
);

CREATE TRIGGER tr_savings_installment_transition_only
BEFORE UPDATE ON savings_installment
FOR EACH ROW
SET NEW.contract_id = IF(
    OLD.status = 'pending'
        AND NEW.status IN ('paid', 'missed', 'cancelled')
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.contract_id = OLD.contract_id
        AND NEW.installment_no = OLD.installment_no
        AND NEW.due_game_day = OLD.due_game_day
        AND NEW.amount_krw = OLD.amount_krw
        AND NEW.created_at = OLD.created_at,
    OLD.contract_id,
    NULL
);

CREATE TRIGGER tr_savings_installment_no_delete
BEFORE DELETE ON savings_installment
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'savings installments cannot be deleted';

CREATE TABLE financial_income_year (
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    tax_year                        SMALLINT UNSIGNED NOT NULL,
    gross_financial_income_krw      BIGINT          NOT NULL DEFAULT 0,
    withheld_income_tax_krw         BIGINT          NOT NULL DEFAULT 0,
    withheld_local_income_tax_krw   BIGINT          NOT NULL DEFAULT 0,
    created_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, run_revision, tax_year),
    CONSTRAINT fk_financial_income_year_save
        FOREIGN KEY (save_id) REFERENCES save (id) ON DELETE CASCADE,
    CONSTRAINT ck_financial_income_year_tax_year CHECK (tax_year BETWEEN 1 AND 9999),
    CONSTRAINT ck_financial_income_year_amounts CHECK (
        gross_financial_income_krw >= 0
        AND withheld_income_tax_krw >= 0
        AND withheld_local_income_tax_krw >= 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

-- Tax totals may change, but their run and tax-year identity never does.
CREATE TRIGGER tr_financial_income_year_identity_only
BEFORE UPDATE ON financial_income_year
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.tax_year = OLD.tax_year
        AND NEW.gross_financial_income_krw >= OLD.gross_financial_income_krw
        AND NEW.withheld_income_tax_krw >= OLD.withheld_income_tax_krw
        AND NEW.withheld_local_income_tax_krw >= OLD.withheld_local_income_tax_krw
        AND NEW.created_at = OLD.created_at,
    OLD.save_id,
    NULL
);

CREATE TRIGGER tr_financial_income_year_no_delete
BEFORE DELETE ON financial_income_year
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'financial income years cannot be deleted';

INSERT INTO financial_institution
    (id, institution_key, display_name, is_deposit_insured)
VALUES
    (1, 'life-bank-a', '라이프은행 A', TRUE),
    (2, 'life-bank-b', '라이프은행 B', TRUE);

INSERT INTO cash_product_version
    (
        id,
        product_key,
        display_name,
        product_kind,
        institution_id,
        is_deposit_protection_eligible,
        rate_reference,
        spread_bp,
        minimum_interest_balance_krw,
        minimum_amount_krw,
        maximum_amount_krw,
        term_days,
        term_months,
        installment_count,
        early_termination_rate_bp,
        day_count_denominator
    )
VALUES
    (
        1,
        'cma-rp-2026-v1',
        '라이프 CMA RP형',
        'cmaRp',
        1,
        FALSE,
        'treasury3mBp',
        0,
        10000,
        NULL,
        NULL,
        NULL,
        NULL,
        NULL,
        NULL,
        365
    ),
    (
        2,
        'cma-issued-note-2026-v1',
        '라이프 CMA 발행어음형',
        'cmaIssuedNote',
        2,
        FALSE,
        'treasury3mBp',
        20,
        1000000,
        NULL,
        NULL,
        NULL,
        NULL,
        NULL,
        NULL,
        365
    ),
    (
        3,
        'life-bank-a-term-deposit-12m-2026-v1',
        '라이프은행 A 12개월 정기예금',
        'termDeposit',
        1,
        TRUE,
        'treasury3mBp',
        20,
        NULL,
        100000,
        1000000000,
        365,
        NULL,
        NULL,
        50,
        365
    ),
    (
        4,
        'life-bank-b-term-deposit-12m-2026-v1',
        '라이프은행 B 12개월 정기예금',
        'termDeposit',
        2,
        TRUE,
        'treasury3mBp',
        35,
        NULL,
        100000,
        1000000000,
        365,
        NULL,
        NULL,
        50,
        365
    ),
    (
        5,
        'life-bank-a-installment-savings-12m-2026-v1',
        '라이프은행 A 12개월 정기적금',
        'installmentSavings',
        1,
        TRUE,
        'treasury3mBp',
        50,
        NULL,
        10000,
        10000000,
        NULL,
        12,
        12,
        50,
        365
    ),
    (
        6,
        'life-bank-b-installment-savings-12m-2026-v1',
        '라이프은행 B 12개월 정기적금',
        'installmentSavings',
        2,
        TRUE,
        'treasury3mBp',
        65,
        NULL,
        10000,
        10000000,
        NULL,
        12,
        12,
        50,
        365
    );
