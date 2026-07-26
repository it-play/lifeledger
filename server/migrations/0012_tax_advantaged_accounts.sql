-- M2-C run-pinned tax profiles, tax-advantaged account contracts, and audit events.

ALTER TABLE financial_income_year
    ADD COLUMN tax_exempt_financial_income_krw BIGINT NOT NULL DEFAULT 0
        AFTER gross_financial_income_krw,
    ADD COLUMN separate_tax_financial_income_krw BIGINT NOT NULL DEFAULT 0
        AFTER tax_exempt_financial_income_krw,
    ADD COLUMN separate_withheld_income_tax_krw BIGINT NOT NULL DEFAULT 0
        AFTER withheld_local_income_tax_krw,
    ADD COLUMN separate_withheld_local_income_tax_krw BIGINT NOT NULL DEFAULT 0
        AFTER separate_withheld_income_tax_krw,
    ADD CONSTRAINT ck_financial_income_year_separate_amounts CHECK (
        tax_exempt_financial_income_krw >= 0
        AND separate_tax_financial_income_krw >= 0
        AND separate_withheld_income_tax_krw >= 0
        AND separate_withheld_local_income_tax_krw >= 0
    );

DROP TRIGGER tr_financial_income_year_identity_only;

-- General and preferential-tax buckets are independent, append-only annual totals.
CREATE TRIGGER tr_financial_income_year_identity_only
BEFORE UPDATE ON financial_income_year
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.tax_year = OLD.tax_year
        AND NEW.gross_financial_income_krw >= OLD.gross_financial_income_krw
        AND NEW.tax_exempt_financial_income_krw >= OLD.tax_exempt_financial_income_krw
        AND NEW.separate_tax_financial_income_krw >= OLD.separate_tax_financial_income_krw
        AND NEW.withheld_income_tax_krw >= OLD.withheld_income_tax_krw
        AND NEW.withheld_local_income_tax_krw >= OLD.withheld_local_income_tax_krw
        AND NEW.separate_withheld_income_tax_krw >= OLD.separate_withheld_income_tax_krw
        AND NEW.separate_withheld_local_income_tax_krw >= OLD.separate_withheld_local_income_tax_krw
        AND NEW.created_at = OLD.created_at,
    OLD.save_id,
    NULL
);

CREATE TABLE run_tax_profile (
    save_id                                             BIGINT UNSIGNED NOT NULL,
    run_revision                                        INT UNSIGNED    NOT NULL,
    source                                              VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    isa_records_complete                                BOOLEAN         NOT NULL,
    prior_year_employment_income_krw                    BIGINT          NOT NULL,
    prior_year_total_salary_krw                         BIGINT          NOT NULL,
    prior_year_comprehensive_income_krw                 BIGINT          NOT NULL,
    prior_year_employment_only                          BOOLEAN         NOT NULL,
    had_comprehensive_financial_income_last_three_years BOOLEAN         NOT NULL,
    created_at                                          DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, run_revision),
    CONSTRAINT fk_run_tax_profile_save
        FOREIGN KEY (save_id) REFERENCES save (id) ON DELETE CASCADE,
    CONSTRAINT ck_run_tax_profile_source CHECK (
        source IN ('m2Default', 'taxYearRecords')
    ),
    CONSTRAINT ck_run_tax_profile_booleans CHECK (
        isa_records_complete IN (FALSE, TRUE)
        AND prior_year_employment_only IN (FALSE, TRUE)
        AND had_comprehensive_financial_income_last_three_years IN (FALSE, TRUE)
    ),
    CONSTRAINT ck_run_tax_profile_income CHECK (
        prior_year_employment_income_krw >= 0
        AND prior_year_total_salary_krw >= 0
        AND prior_year_comprehensive_income_krw >= 0
    ),
    CONSTRAINT ck_run_tax_profile_m2_default CHECK (
        source <> 'm2Default'
        OR (
            isa_records_complete = TRUE
            AND prior_year_employment_income_krw = 0
            AND prior_year_total_salary_krw = 0
            AND prior_year_comprehensive_income_krw = 0
            AND prior_year_employment_only = TRUE
            AND had_comprehensive_financial_income_last_three_years = FALSE
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

-- Profiles are inserted only for the current run, then retained when the save advances.
CREATE TRIGGER tr_run_tax_profile_current_run_insert
BEFORE INSERT ON run_tax_profile
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM save
        WHERE id = NEW.save_id
          AND run_revision = NEW.run_revision
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_run_tax_profile_no_update
BEFORE UPDATE ON run_tax_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'run tax profiles are immutable';

CREATE TRIGGER tr_run_tax_profile_no_delete
BEFORE DELETE ON run_tax_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'run tax profiles are immutable';

-- Existing runs predate tax-year records and use the explicit M2 eligibility defaults.
INSERT INTO run_tax_profile
    (
        save_id,
        run_revision,
        source,
        isa_records_complete,
        prior_year_employment_income_krw,
        prior_year_total_salary_krw,
        prior_year_comprehensive_income_krw,
        prior_year_employment_only,
        had_comprehensive_financial_income_last_three_years
    )
SELECT
    id,
    run_revision,
    'm2Default',
    TRUE,
    0,
    0,
    0,
    TRUE,
    FALSE
FROM save;

CREATE TABLE isa_account_contract (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    financial_account_id            BIGINT UNSIGNED NOT NULL,
    account_type                    VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    status                          VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    active_run_slot                 TINYINT GENERATED ALWAYS AS (
        CASE WHEN status = 'active' THEN 1 ELSE NULL END
    ) STORED,
    opened_game_day                 INT UNSIGNED    NOT NULL,
    minimum_term_game_day           INT UNSIGNED    NOT NULL,
    total_contribution_krw          BIGINT          NOT NULL DEFAULT 0,
    principal_withdrawal_krw        BIGINT          NOT NULL DEFAULT 0,
    isa_tax_profit_krw              BIGINT          NOT NULL DEFAULT 0,
    isa_deductible_loss_krw         BIGINT          NOT NULL DEFAULT 0,
    closed_game_day                 INT UNSIGNED        NULL,
    closing_movement_amount_krw     BIGINT              NULL,
    closing_ledger_transaction_id   BIGINT UNSIGNED     NULL,
    cancellation_reason             VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    created_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_isa_account_contract_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_isa_account_contract_account
        (save_id, run_revision, financial_account_id),
    UNIQUE KEY uk_isa_account_contract_active_run
        (save_id, run_revision, active_run_slot),
    UNIQUE KEY uk_isa_account_contract_closing_ledger
        (save_id, run_revision, closing_ledger_transaction_id),
    KEY ix_isa_account_contract_status
        (save_id, run_revision, status, financial_account_id),
    CONSTRAINT fk_isa_account_contract_account
        FOREIGN KEY (save_id, run_revision, financial_account_id)
        REFERENCES financial_account (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_isa_account_contract_closing_ledger
        FOREIGN KEY (save_id, run_revision, closing_ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT ck_isa_account_contract_type CHECK (
        account_type IN ('isaGeneral', 'isaLowIncome')
    ),
    CONSTRAINT ck_isa_account_contract_status CHECK (
        status IN ('active', 'closed', 'cancelled')
    ),
    CONSTRAINT ck_isa_account_contract_period CHECK (
        minimum_term_game_day > opened_game_day
    ),
    CONSTRAINT ck_isa_account_contract_amounts CHECK (
        total_contribution_krw >= 0
        AND principal_withdrawal_krw >= 0
        AND principal_withdrawal_krw <= total_contribution_krw
        AND isa_tax_profit_krw >= 0
        AND isa_deductible_loss_krw >= 0
    ),
    CONSTRAINT ck_isa_account_contract_state_shape CHECK (
        (
            status = 'active'
            AND closed_game_day IS NULL
            AND closing_movement_amount_krw IS NULL
            AND closing_ledger_transaction_id IS NULL
            AND cancellation_reason IS NULL
        )
        OR
        (
            status = 'closed'
            AND closed_game_day >= opened_game_day
            AND closing_movement_amount_krw >= 0
            AND (
                (
                    closing_movement_amount_krw = 0
                    AND closing_ledger_transaction_id IS NULL
                )
                OR (
                    closing_movement_amount_krw > 0
                    AND closing_ledger_transaction_id IS NOT NULL
                )
            )
            AND cancellation_reason IS NULL
        )
        OR
        (
            status = 'cancelled'
            AND closed_game_day >= opened_game_day
            AND closing_movement_amount_krw IS NULL
            AND closing_ledger_transaction_id IS NULL
            AND CHAR_LENGTH(cancellation_reason) > 0
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_isa_account_contract_valid_insert
BEFORE INSERT ON isa_account_contract
FOR EACH ROW
SET NEW.financial_account_id = IF(
    NEW.status = 'active'
        AND NEW.total_contribution_krw = 0
        AND NEW.principal_withdrawal_krw = 0
        AND NEW.isa_tax_profit_krw = 0
        AND NEW.isa_deductible_loss_krw = 0
        AND NEW.closed_game_day IS NULL
        AND NEW.closing_movement_amount_krw IS NULL
        AND NEW.closing_ledger_transaction_id IS NULL
        AND NEW.cancellation_reason IS NULL
        AND EXISTS (
            SELECT 1
            FROM financial_account AS account
            WHERE account.id = NEW.financial_account_id
              AND account.save_id = NEW.save_id
              AND account.run_revision = NEW.run_revision
              AND BINARY account.account_type = BINARY NEW.account_type
              AND account.account_type IN ('isaGeneral', 'isaLowIncome')
              AND account.status = 'open'
              AND account.opened_game_day = NEW.opened_game_day
        )
        AND EXISTS (
            SELECT 1
            FROM run_tax_profile AS profile
            WHERE profile.save_id = NEW.save_id
              AND profile.run_revision = NEW.run_revision
        ),
    NEW.financial_account_id,
    NULL
);

-- ISA summaries grow monotonically until the active contract makes one terminal transition.
CREATE TRIGGER tr_isa_account_contract_transition_only
BEFORE UPDATE ON isa_account_contract
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'active'
        AND NEW.status IN ('active', 'closed', 'cancelled')
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.financial_account_id = OLD.financial_account_id
        AND BINARY NEW.account_type = BINARY OLD.account_type
        AND NEW.opened_game_day = OLD.opened_game_day
        AND NEW.minimum_term_game_day = OLD.minimum_term_game_day
        AND NEW.total_contribution_krw >= OLD.total_contribution_krw
        AND NEW.principal_withdrawal_krw >= OLD.principal_withdrawal_krw
        AND NEW.isa_tax_profit_krw >= OLD.isa_tax_profit_krw
        AND NEW.isa_deductible_loss_krw >= OLD.isa_deductible_loss_krw
        AND NEW.created_at = OLD.created_at,
    OLD.id,
    NULL
);

CREATE TRIGGER tr_isa_account_contract_no_delete
BEFORE DELETE ON isa_account_contract
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'ISA contracts cannot be deleted';

CREATE TABLE pension_account_contract (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED    NOT NULL,
    financial_account_id                BIGINT UNSIGNED NOT NULL,
    account_type                        VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    status                              VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    active_pension_savings_run_slot     TINYINT GENERATED ALWAYS AS (
        CASE
            WHEN status = 'active' AND account_type = 'pensionSavings' THEN 1
            ELSE NULL
        END
    ) STORED,
    active_irp_run_slot                 TINYINT GENERATED ALWAYS AS (
        CASE WHEN status = 'active' AND account_type = 'irp' THEN 1 ELSE NULL END
    ) STORED,
    opened_game_day                     INT UNSIGNED    NOT NULL,
    eligible_pension_start_game_day     INT UNSIGNED    NOT NULL,
    pension_started                     BOOLEAN         NOT NULL DEFAULT FALSE,
    pension_start_game_day              INT UNSIGNED        NULL,
    pension_start_tax_year              SMALLINT UNSIGNED   NULL,
    payment_years                       TINYINT UNSIGNED     NULL,
    lifetime                            BOOLEAN             NULL,
    cancelled_game_day                  INT UNSIGNED        NULL,
    cancellation_reason                 VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_pension_account_contract_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_pension_account_contract_account
        (save_id, run_revision, financial_account_id),
    UNIQUE KEY uk_pension_account_contract_active_savings
        (save_id, run_revision, active_pension_savings_run_slot),
    UNIQUE KEY uk_pension_account_contract_active_irp
        (save_id, run_revision, active_irp_run_slot),
    KEY ix_pension_account_contract_status
        (save_id, run_revision, status, account_type, financial_account_id),
    CONSTRAINT fk_pension_account_contract_account
        FOREIGN KEY (save_id, run_revision, financial_account_id)
        REFERENCES financial_account (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT ck_pension_account_contract_type CHECK (
        account_type IN ('pensionSavings', 'irp')
    ),
    CONSTRAINT ck_pension_account_contract_status CHECK (
        status IN ('active', 'cancelled')
    ),
    CONSTRAINT ck_pension_account_contract_period CHECK (
        eligible_pension_start_game_day > opened_game_day
    ),
    CONSTRAINT ck_pension_account_contract_boolean CHECK (
        pension_started IN (FALSE, TRUE)
        AND (lifetime IS NULL OR lifetime IN (FALSE, TRUE))
    ),
    CONSTRAINT ck_pension_account_contract_start_shape CHECK (
        (
            pension_started = FALSE
            AND pension_start_game_day IS NULL
            AND pension_start_tax_year IS NULL
            AND payment_years IS NULL
            AND lifetime IS NULL
        )
        OR
        (
            pension_started = TRUE
            AND pension_start_game_day >= eligible_pension_start_game_day
            AND pension_start_tax_year BETWEEN 1 AND 9999
            AND payment_years BETWEEN 5 AND 100
            AND lifetime IS NOT NULL
        )
    ),
    CONSTRAINT ck_pension_account_contract_state_shape CHECK (
        (
            status = 'active'
            AND cancelled_game_day IS NULL
            AND cancellation_reason IS NULL
        )
        OR
        (
            status = 'cancelled'
            AND cancelled_game_day >= opened_game_day
            AND CHAR_LENGTH(cancellation_reason) > 0
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_pension_account_contract_valid_insert
BEFORE INSERT ON pension_account_contract
FOR EACH ROW
SET NEW.financial_account_id = IF(
    NEW.status = 'active'
        AND NEW.pension_started = FALSE
        AND NEW.pension_start_game_day IS NULL
        AND NEW.pension_start_tax_year IS NULL
        AND NEW.payment_years IS NULL
        AND NEW.lifetime IS NULL
        AND NEW.cancelled_game_day IS NULL
        AND NEW.cancellation_reason IS NULL
        AND EXISTS (
            SELECT 1
            FROM financial_account AS account
            WHERE account.id = NEW.financial_account_id
              AND account.save_id = NEW.save_id
              AND account.run_revision = NEW.run_revision
              AND BINARY account.account_type = BINARY NEW.account_type
              AND account.account_type IN ('pensionSavings', 'irp')
              AND account.status = 'open'
              AND account.opened_game_day = NEW.opened_game_day
        )
        AND EXISTS (
            SELECT 1
            FROM run_tax_profile AS profile
            WHERE profile.save_id = NEW.save_id
              AND profile.run_revision = NEW.run_revision
        ),
    NEW.financial_account_id,
    NULL
);

-- Pension start is one-way; a new run may later cancel the otherwise immutable contract.
CREATE TRIGGER tr_pension_account_contract_transition_only
BEFORE UPDATE ON pension_account_contract
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'active'
        AND NEW.status IN ('active', 'cancelled')
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.financial_account_id = OLD.financial_account_id
        AND BINARY NEW.account_type = BINARY OLD.account_type
        AND NEW.opened_game_day = OLD.opened_game_day
        AND NEW.eligible_pension_start_game_day = OLD.eligible_pension_start_game_day
        AND NEW.pension_started >= OLD.pension_started
        AND (
            OLD.pension_started = FALSE
            OR (
                NEW.pension_started = TRUE
                AND NEW.pension_start_game_day = OLD.pension_start_game_day
                AND NEW.pension_start_tax_year = OLD.pension_start_tax_year
                AND NEW.payment_years = OLD.payment_years
                AND NEW.lifetime = OLD.lifetime
            )
        )
        AND NEW.created_at = OLD.created_at,
    OLD.id,
    NULL
);

CREATE TRIGGER tr_pension_account_contract_no_delete
BEFORE DELETE ON pension_account_contract
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'pension contracts cannot be deleted';

CREATE TABLE pension_tax_balance (
    save_id                              BIGINT UNSIGNED NOT NULL,
    run_revision                         INT UNSIGNED    NOT NULL,
    financial_account_id                 BIGINT UNSIGNED NOT NULL,
    tax_excluded_contribution_krw        BIGINT          NOT NULL DEFAULT 0,
    deferred_retirement_income_krw       BIGINT          NOT NULL DEFAULT 0,
    credited_contribution_krw            BIGINT          NOT NULL DEFAULT 0,
    earnings_krw                         BIGINT          NOT NULL DEFAULT 0,
    created_at                           DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                           DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, run_revision, financial_account_id),
    CONSTRAINT fk_pension_tax_balance_contract
        FOREIGN KEY (save_id, run_revision, financial_account_id)
        REFERENCES pension_account_contract
            (save_id, run_revision, financial_account_id) ON DELETE CASCADE,
    CONSTRAINT ck_pension_tax_balance_amounts CHECK (
        tax_excluded_contribution_krw >= 0
        AND deferred_retirement_income_krw >= 0
        AND credited_contribution_krw >= 0
        AND earnings_krw >= 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_pension_tax_balance_valid_insert
BEFORE INSERT ON pension_tax_balance
FOR EACH ROW
SET NEW.financial_account_id = IF(
    EXISTS (
        SELECT 1
        FROM pension_account_contract AS contract
        WHERE contract.save_id = NEW.save_id
          AND contract.run_revision = NEW.run_revision
          AND contract.financial_account_id = NEW.financial_account_id
          AND contract.status = 'active'
    ),
    NEW.financial_account_id,
    NULL
);

-- Layer amounts can move in either direction, but their account identity is fixed.
CREATE TRIGGER tr_pension_tax_balance_identity_only
BEFORE UPDATE ON pension_tax_balance
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.financial_account_id = OLD.financial_account_id
        AND NEW.created_at = OLD.created_at,
    OLD.save_id,
    NULL
);

CREATE TRIGGER tr_pension_tax_balance_no_delete
BEFORE DELETE ON pension_tax_balance
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'pension tax balances cannot be deleted';

CREATE TABLE pension_contribution_year (
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    financial_account_id        BIGINT UNSIGNED NOT NULL,
    tax_year                    SMALLINT UNSIGNED NOT NULL,
    total_contribution_krw      BIGINT          NOT NULL,
    credit_eligible_krw         BIGINT          NOT NULL,
    expected_credit_rate_ppm    INT UNSIGNED    NOT NULL,
    expected_credit_krw         BIGINT          NOT NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, run_revision, financial_account_id, tax_year),
    KEY ix_pension_contribution_year_lookup
        (save_id, run_revision, tax_year, financial_account_id),
    CONSTRAINT fk_pension_contribution_year_contract
        FOREIGN KEY (save_id, run_revision, financial_account_id)
        REFERENCES pension_account_contract
            (save_id, run_revision, financial_account_id) ON DELETE CASCADE,
    CONSTRAINT ck_pension_contribution_year_tax_year CHECK (
        tax_year BETWEEN 1 AND 9999
    ),
    CONSTRAINT ck_pension_contribution_year_amounts CHECK (
        total_contribution_krw > 0
        AND credit_eligible_krw >= 0
        AND credit_eligible_krw <= total_contribution_krw
        AND expected_credit_krw >= 0
        AND expected_credit_krw <= credit_eligible_krw
    ),
    CONSTRAINT ck_pension_contribution_year_rate CHECK (
        expected_credit_rate_ppm <= 1000000
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_pension_contribution_year_valid_insert
BEFORE INSERT ON pension_contribution_year
FOR EACH ROW
SET NEW.financial_account_id = IF(
    EXISTS (
        SELECT 1
        FROM pension_account_contract AS contract
        WHERE contract.save_id = NEW.save_id
          AND contract.run_revision = NEW.run_revision
          AND contract.financial_account_id = NEW.financial_account_id
          AND contract.status = 'active'
    ),
    NEW.financial_account_id,
    NULL
);

-- Contributions only grow; credit allocation may be rebalanced across both accounts.
CREATE TRIGGER tr_pension_contribution_year_identity_only
BEFORE UPDATE ON pension_contribution_year
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.financial_account_id = OLD.financial_account_id
        AND NEW.tax_year = OLD.tax_year
        AND NEW.total_contribution_krw >= OLD.total_contribution_krw
        AND NEW.expected_credit_rate_ppm = OLD.expected_credit_rate_ppm
        AND NEW.created_at = OLD.created_at,
    OLD.save_id,
    NULL
);

CREATE TRIGGER tr_pension_contribution_year_no_delete
BEFORE DELETE ON pension_contribution_year
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'pension contribution years cannot be deleted';

CREATE TABLE pension_withdrawal_year (
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    financial_account_id            BIGINT UNSIGNED NOT NULL,
    tax_year                        SMALLINT UNSIGNED NOT NULL,
    opening_account_value_krw       BIGINT          NOT NULL,
    pension_year_number             SMALLINT UNSIGNED   NULL,
    pension_limit_krw               BIGINT              NULL,
    -- Only ordinary pension payments consume the statutory annual limit.
    pension_withdrawn_krw           BIGINT          NOT NULL DEFAULT 0,
    unavoidable_withdrawn_krw       BIGINT          NOT NULL DEFAULT 0,
    non_pension_withdrawn_krw       BIGINT          NOT NULL DEFAULT 0,
    tax_free_withdrawn_krw          BIGINT          NOT NULL DEFAULT 0,
    withheld_tax_krw                BIGINT          NOT NULL DEFAULT 0,
    created_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, run_revision, financial_account_id, tax_year),
    KEY ix_pension_withdrawal_year_lookup
        (save_id, run_revision, tax_year, financial_account_id),
    CONSTRAINT fk_pension_withdrawal_year_contract
        FOREIGN KEY (save_id, run_revision, financial_account_id)
        REFERENCES pension_account_contract
            (save_id, run_revision, financial_account_id) ON DELETE CASCADE,
    CONSTRAINT ck_pension_withdrawal_year_tax_year CHECK (
        tax_year BETWEEN 1 AND 9999
    ),
    CONSTRAINT ck_pension_withdrawal_year_amounts CHECK (
        opening_account_value_krw >= 0
        AND (pension_limit_krw IS NULL OR pension_limit_krw >= 0)
        AND pension_withdrawn_krw >= 0
        AND unavoidable_withdrawn_krw >= 0
        AND non_pension_withdrawn_krw >= 0
        AND tax_free_withdrawn_krw >= 0
        AND tax_free_withdrawn_krw
            <= pension_withdrawn_krw
                + unavoidable_withdrawn_krw
                + non_pension_withdrawn_krw
        AND withheld_tax_krw >= 0
        AND withheld_tax_krw
            <= pension_withdrawn_krw
                + unavoidable_withdrawn_krw
                + non_pension_withdrawn_krw
    ),
    CONSTRAINT ck_pension_withdrawal_year_limit_shape CHECK (
        (
            pension_year_number IS NULL
            AND pension_limit_krw IS NULL
            AND pension_withdrawn_krw = 0
        )
        OR
        (
            pension_year_number BETWEEN 1 AND 10
            AND pension_limit_krw IS NOT NULL
            AND pension_withdrawn_krw <= pension_limit_krw
        )
        OR
        (
            pension_year_number >= 11
            AND pension_limit_krw IS NULL
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_pension_withdrawal_year_valid_insert
BEFORE INSERT ON pension_withdrawal_year
FOR EACH ROW
SET NEW.financial_account_id = IF(
    EXISTS (
        SELECT 1
        FROM pension_account_contract AS contract
        WHERE contract.save_id = NEW.save_id
          AND contract.run_revision = NEW.run_revision
          AND contract.financial_account_id = NEW.financial_account_id
          AND contract.status = 'active'
    ),
    NEW.financial_account_id,
    NULL
);

-- A pre-start non-pension row may pin its pension-year terms once; totals only grow.
CREATE TRIGGER tr_pension_withdrawal_year_identity_only
BEFORE UPDATE ON pension_withdrawal_year
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.financial_account_id = OLD.financial_account_id
        AND NEW.tax_year = OLD.tax_year
        AND NEW.opening_account_value_krw = OLD.opening_account_value_krw
        AND (
            (
                OLD.pension_year_number IS NULL
                AND (
                    NEW.pension_year_number IS NULL
                    OR NEW.pension_year_number > 0
                )
            )
            OR (
                NEW.pension_year_number = OLD.pension_year_number
                AND NEW.pension_limit_krw <=> OLD.pension_limit_krw
            )
        )
        AND NEW.pension_withdrawn_krw >= OLD.pension_withdrawn_krw
        AND NEW.unavoidable_withdrawn_krw >= OLD.unavoidable_withdrawn_krw
        AND NEW.non_pension_withdrawn_krw >= OLD.non_pension_withdrawn_krw
        AND NEW.tax_free_withdrawn_krw >= OLD.tax_free_withdrawn_krw
        AND NEW.withheld_tax_krw >= OLD.withheld_tax_krw
        AND NEW.created_at = OLD.created_at,
    OLD.save_id,
    NULL
);

CREATE TRIGGER tr_pension_withdrawal_year_no_delete
BEFORE DELETE ON pension_withdrawal_year
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'pension withdrawal years cannot be deleted';

CREATE TABLE tax_account_event (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    financial_account_id        BIGINT UNSIGNED NOT NULL,
    command_id                  CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    event_order                 SMALLINT UNSIGNED NOT NULL,
    event_kind                  VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    event_schema_version        SMALLINT UNSIGNED NOT NULL DEFAULT 1,
    game_day                    INT UNSIGNED    NOT NULL,
    tax_year                    SMALLINT UNSIGNED NOT NULL,
    movement_amount_krw         BIGINT          NOT NULL,
    payload                     JSON            NOT NULL,
    ledger_transaction_id       BIGINT UNSIGNED     NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_tax_account_event_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_tax_account_event_command_order (save_id, command_id, event_order),
    KEY ix_tax_account_event_account
        (save_id, run_revision, financial_account_id, id),
    KEY ix_tax_account_event_ledger
        (save_id, run_revision, ledger_transaction_id),
    CONSTRAINT fk_tax_account_event_account
        FOREIGN KEY (save_id, run_revision, financial_account_id)
        REFERENCES financial_account (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_tax_account_event_identity
        FOREIGN KEY (save_id, command_id)
        REFERENCES command_identity (save_id, command_id) ON DELETE CASCADE,
    CONSTRAINT fk_tax_account_event_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT ck_tax_account_event_order CHECK (event_order > 0),
    CONSTRAINT ck_tax_account_event_kind CHECK (
        event_kind IN (
            'isaOpened',
            'isaContribution',
            'isaPrincipalWithdrawal',
            'isaClosed',
            'pensionOpened',
            'pensionContribution',
            'pensionStarted',
            'pensionWithdrawal',
            'pensionTaxReclassification',
            'runCancelled'
        )
    ),
    CONSTRAINT ck_tax_account_event_schema CHECK (
        event_schema_version = 1 AND JSON_TYPE(payload) = 'OBJECT'
    ),
    CONSTRAINT ck_tax_account_event_tax_year CHECK (tax_year BETWEEN 1 AND 9999),
    CONSTRAINT ck_tax_account_event_movement CHECK (movement_amount_krw >= 0),
    CONSTRAINT ck_tax_account_event_ledger_shape CHECK (
        (
            event_kind IN ('isaOpened', 'pensionOpened', 'pensionStarted', 'runCancelled')
            AND movement_amount_krw = 0
            AND ledger_transaction_id IS NULL
        )
        OR
        (
            event_kind = 'isaClosed'
            AND (
                (
                    movement_amount_krw = 0
                    AND ledger_transaction_id IS NULL
                )
                OR (
                    movement_amount_krw > 0
                    AND ledger_transaction_id IS NOT NULL
                )
            )
        )
        OR
        (
            event_kind IN (
                'isaContribution',
                'isaPrincipalWithdrawal',
                'pensionContribution',
                'pensionWithdrawal',
                'pensionTaxReclassification'
            )
            AND movement_amount_krw > 0
            AND ledger_transaction_id IS NOT NULL
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

-- The event kind fixes both account family and whether a real ledger movement exists.
CREATE TRIGGER tr_tax_account_event_valid_insert
BEFORE INSERT ON tax_account_event
FOR EACH ROW
SET NEW.financial_account_id = IF(
    EXISTS (
        SELECT 1
        FROM financial_account AS account
        WHERE account.id = NEW.financial_account_id
          AND account.save_id = NEW.save_id
          AND account.run_revision = NEW.run_revision
          AND (
              (
                  NEW.event_kind IN (
                      'isaOpened',
                      'isaContribution',
                      'isaPrincipalWithdrawal',
                      'isaClosed'
                  )
                  AND account.account_type IN ('isaGeneral', 'isaLowIncome')
              )
              OR
              (
                  NEW.event_kind IN (
                      'pensionOpened',
                      'pensionContribution',
                      'pensionStarted',
                      'pensionWithdrawal',
                      'pensionTaxReclassification'
                  )
                  AND account.account_type IN ('pensionSavings', 'irp')
              )
              OR (
                  NEW.event_kind = 'runCancelled'
                  AND account.account_type IN (
                      'isaGeneral',
                      'isaLowIncome',
                      'pensionSavings',
                      'irp'
                  )
              )
          )
    )
        AND EXISTS (
            SELECT 1
            FROM command_identity AS identity
            WHERE identity.save_id = NEW.save_id
              AND BINARY identity.command_id = BINARY NEW.command_id
              AND identity.initial_run_revision = NEW.run_revision
        ),
    NEW.financial_account_id,
    NULL
);

CREATE TRIGGER tr_tax_account_event_no_update
BEFORE UPDATE ON tax_account_event
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'tax account events are append-only';

CREATE TRIGGER tr_tax_account_event_no_delete
BEFORE DELETE ON tax_account_event
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'tax account events are append-only';
