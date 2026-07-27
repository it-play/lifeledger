-- M4-B loan, installment, delinquency, credit, and tax-obligation authority (§4, §10).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;

ALTER TABLE run_rule_bundle
    ADD UNIQUE KEY uk_run_rule_bundle_credit_pin
        (save_id, run_revision, credit_model_version_id);

CREATE TABLE credit_state (
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    household_id                BIGINT UNSIGNED NOT NULL,
    credit_model_version_id     BIGINT UNSIGNED NOT NULL,
    credit_units                SMALLINT UNSIGNED NOT NULL,
    credit_band                 VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    last_evaluated_game_day     INT UNSIGNED    NOT NULL,
    evaluation_revision         BIGINT UNSIGNED NOT NULL DEFAULT 0,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, run_revision),
    UNIQUE KEY uk_credit_state_household (household_id),
    UNIQUE KEY uk_credit_state_model
        (save_id, run_revision, credit_model_version_id),
    CONSTRAINT fk_credit_state_household
        FOREIGN KEY (save_id, run_revision, household_id)
        REFERENCES household (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_credit_state_bundle
        FOREIGN KEY (save_id, run_revision, credit_model_version_id)
        REFERENCES run_rule_bundle (save_id, run_revision, credit_model_version_id)
        ON DELETE CASCADE,
    CONSTRAINT ck_credit_state_units CHECK (credit_units <= 1000),
    CONSTRAINT ck_credit_state_band CHECK (
        (credit_band = 'prime' AND credit_units BETWEEN 850 AND 1000)
        OR (credit_band = 'standard' AND credit_units BETWEEN 650 AND 849)
        OR (credit_band = 'limited' AND credit_units BETWEEN 450 AND 649)
        OR (credit_band = 'distressed' AND credit_units BETWEEN 1 AND 449)
        OR (credit_band = 'insolvent' AND credit_units = 0)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE loan_quote (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    household_id                    BIGINT UNSIGNED NOT NULL,
    credit_model_version_id         BIGINT UNSIGNED NOT NULL,
    loan_product_version_id         BIGINT UNSIGNED NOT NULL,
    command_id                      CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    payload_sha256                  CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    expected_state_revision         BIGINT UNSIGNED NOT NULL,
    created_game_day                INT UNSIGNED    NOT NULL,
    expires_game_day                INT UNSIGNED    NOT NULL,
    requested_principal_krw         BIGINT          NOT NULL,
    verified_annual_income_krw      BIGINT              NULL,
    verified_income_source          VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NULL,
    existing_loan_balance_krw       BIGINT          NOT NULL,
    post_execution_balance_krw      BIGINT          NOT NULL,
    dsr_numerator_krw               BIGINT              NULL,
    dsr_denominator_krw             BIGINT              NULL,
    dsr_ratio_ppm                   BIGINT              NULL,
    dsr_limit_ppm                   INT UNSIGNED         NULL,
    stress_rate_bp                  SMALLINT UNSIGNED NOT NULL,
    decision_code                   VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    decision_reasons                JSON            NOT NULL,
    quoted_terms                    JSON            NOT NULL,
    created_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_loan_quote_save_command (save_id, command_id),
    UNIQUE KEY uk_loan_quote_save_run_id (save_id, run_revision, id),
    KEY ix_loan_quote_household_day (household_id, created_game_day, id),
    KEY ix_loan_quote_product (loan_product_version_id),
    CONSTRAINT fk_loan_quote_household
        FOREIGN KEY (save_id, run_revision, household_id)
        REFERENCES household (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_loan_quote_bundle
        FOREIGN KEY (save_id, run_revision, credit_model_version_id)
        REFERENCES run_rule_bundle (save_id, run_revision, credit_model_version_id)
        ON DELETE CASCADE,
    CONSTRAINT fk_loan_quote_product
        FOREIGN KEY (loan_product_version_id) REFERENCES loan_product_version (id),
    CONSTRAINT ck_loan_quote_command CHECK (
        command_id REGEXP '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
        AND payload_sha256 REGEXP '^[0-9a-f]{64}$'
    ),
    CONSTRAINT ck_loan_quote_expiry CHECK (expires_game_day = created_game_day),
    CONSTRAINT ck_loan_quote_amounts CHECK (
        requested_principal_krw > 0
        AND existing_loan_balance_krw >= 0
        AND post_execution_balance_krw
            = existing_loan_balance_krw + requested_principal_krw
        AND (verified_annual_income_krw IS NULL OR verified_annual_income_krw > 0)
    ),
    CONSTRAINT ck_loan_quote_income_shape CHECK (
        (verified_annual_income_krw IS NULL AND verified_income_source IS NULL)
        OR (
            verified_annual_income_krw IS NOT NULL
            AND CHAR_LENGTH(verified_income_source) > 0
        )
    ),
    CONSTRAINT ck_loan_quote_dsr_shape CHECK (
        (
            dsr_numerator_krw IS NULL
            AND dsr_denominator_krw IS NULL
            AND dsr_ratio_ppm IS NULL
            AND dsr_limit_ppm IS NULL
        )
        OR (
            dsr_numerator_krw >= 0
            AND dsr_denominator_krw > 0
            AND dsr_ratio_ppm >= 0
            AND dsr_limit_ppm > 0
            AND dsr_ratio_ppm = FLOOR(
                CAST(dsr_numerator_krw AS DECIMAL(65, 0)) * 1000000
                / dsr_denominator_krw
            )
        )
    ),
    CONSTRAINT ck_loan_quote_decision CHECK (
        decision_code IN (
            'eligible', 'debtServiceLimit', 'incomeUnavailable',
            'creditRestricted', 'valuationUnavailable'
        )
        AND JSON_TYPE(decision_reasons) = 'ARRAY'
        AND JSON_LENGTH(decision_reasons) BETWEEN 1 AND 8
        AND JSON_TYPE(quoted_terms) = 'OBJECT'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE loan_contract (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    household_id                    BIGINT UNSIGNED NOT NULL,
    credit_model_version_id         BIGINT UNSIGNED NOT NULL,
    loan_product_version_id         BIGINT UNSIGNED NOT NULL,
    loan_quote_id                   BIGINT UNSIGNED     NULL,
    origin_kind                     VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    origin_command_id               CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    product_kind                    VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    lender_sector                   VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    rate_status                     VARCHAR(20) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    rate_type                       VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    reference_rate_key              VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NULL,
    fixed_annual_rate_bp            SMALLINT UNSIGNED     NULL,
    applied_spread_bp               SMALLINT              NULL,
    minimum_annual_rate_bp          SMALLINT UNSIGNED     NULL,
    maximum_annual_rate_bp          SMALLINT UNSIGNED     NULL,
    current_annual_rate_bp          SMALLINT UNSIGNED     NULL,
    rate_reset_rule                 VARCHAR(20) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    day_count_denominator           SMALLINT UNSIGNED     NULL,
    repayment_method                VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    term_months                     SMALLINT UNSIGNED     NULL,
    total_installments              SMALLINT UNSIGNED     NULL,
    payment_calendar                VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    grace_months                    SMALLINT UNSIGNED     NULL,
    prepayment_fee_ppm              INT UNSIGNED          NULL,
    prepayment_effect               VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    dsr_included                    BOOLEAN         NOT NULL,
    read_only                       BOOLEAN         NOT NULL,
    status                          VARCHAR(20) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    original_principal_krw          BIGINT          NOT NULL,
    remaining_principal_krw         BIGINT          NOT NULL,
    accrued_interest_krw            BIGINT          NOT NULL DEFAULT 0,
    accrued_fee_krw                 BIGINT          NOT NULL DEFAULT 0,
    interest_remainder_numerator    DECIMAL(39, 0) NOT NULL DEFAULT 0,
    activated_game_day              INT UNSIGNED    NOT NULL,
    maturity_game_day               INT UNSIGNED        NULL,
    next_installment_no             SMALLINT UNSIGNED    NULL,
    oldest_unpaid_due_game_day      INT UNSIGNED        NULL,
    bridge_contract_slot            TINYINT UNSIGNED GENERATED ALWAYS AS (
        CASE WHEN origin_kind = 'legacyDebtBridge' THEN 1 ELSE NULL END
    ) STORED,
    created_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_loan_contract_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_loan_contract_origin_command
        (save_id, run_revision, origin_command_id, product_kind),
    UNIQUE KEY uk_loan_contract_quote (loan_quote_id),
    UNIQUE KEY uk_loan_contract_bridge (household_id, bridge_contract_slot),
    KEY ix_loan_contract_household_status (household_id, status, id),
    KEY ix_loan_contract_product (loan_product_version_id),
    CONSTRAINT fk_loan_contract_household
        FOREIGN KEY (save_id, run_revision, household_id)
        REFERENCES household (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_loan_contract_bundle
        FOREIGN KEY (save_id, run_revision, credit_model_version_id)
        REFERENCES run_rule_bundle (save_id, run_revision, credit_model_version_id)
        ON DELETE CASCADE,
    CONSTRAINT fk_loan_contract_product
        FOREIGN KEY (loan_product_version_id) REFERENCES loan_product_version (id),
    CONSTRAINT fk_loan_contract_quote
        FOREIGN KEY (save_id, run_revision, loan_quote_id)
        REFERENCES loan_quote (save_id, run_revision, id),
    CONSTRAINT ck_loan_contract_origin CHECK (
        origin_kind IN (
            'characterStartV2', 'legacyV1Mapping', 'quoteExecution', 'legacyDebtBridge'
        )
        AND (
            (origin_kind = 'quoteExecution' AND loan_quote_id IS NOT NULL)
            OR (origin_kind <> 'quoteExecution' AND loan_quote_id IS NULL)
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
    CONSTRAINT ck_loan_contract_kind CHECK (
        product_kind IN ('studentLoan', 'unsecuredLoan', 'legacyDebt')
    ),
    CONSTRAINT ck_loan_contract_lender CHECK (
        lender_sector IN ('bank', 'nonBank', 'bridgeOnly')
    ),
    CONSTRAINT ck_loan_contract_rate CHECK (
        rate_status IN ('available', 'rateUnavailable')
        AND rate_type IN ('fixed', 'variable', 'unavailable')
        AND rate_reset_rule IN ('none', 'monthlyDay1')
        AND (
            (
                rate_type = 'fixed'
                AND rate_status = 'available'
                AND reference_rate_key IS NULL
                AND fixed_annual_rate_bp IS NOT NULL
                AND applied_spread_bp IS NULL
                AND minimum_annual_rate_bp = fixed_annual_rate_bp
                AND maximum_annual_rate_bp = fixed_annual_rate_bp
                AND current_annual_rate_bp = fixed_annual_rate_bp
                AND rate_reset_rule = 'none'
            )
            OR (
                rate_type = 'variable'
                AND rate_status = 'available'
                AND CHAR_LENGTH(reference_rate_key) > 0
                AND fixed_annual_rate_bp IS NULL
                AND applied_spread_bp IS NOT NULL
                AND minimum_annual_rate_bp <= current_annual_rate_bp
                AND current_annual_rate_bp <= maximum_annual_rate_bp
                AND rate_reset_rule <> 'none'
            )
            OR (
                rate_type = 'unavailable'
                AND rate_status = 'rateUnavailable'
                AND reference_rate_key IS NULL
                AND fixed_annual_rate_bp IS NULL
                AND applied_spread_bp IS NULL
                AND minimum_annual_rate_bp IS NULL
                AND maximum_annual_rate_bp IS NULL
                AND current_annual_rate_bp IS NULL
                AND rate_reset_rule = 'none'
            )
        )
    ),
    CONSTRAINT ck_loan_contract_terms CHECK (
        repayment_method IN ('equalPrincipal', 'levelPayment', 'bullet')
        AND payment_calendar IN ('monthEnd', 'none')
        AND prepayment_effect IN ('reduceTerm', 'recalculatePayment', 'forbidden')
        AND dsr_included IN (FALSE, TRUE)
        AND read_only IN (FALSE, TRUE)
    ),
    CONSTRAINT ck_loan_contract_status CHECK (
        status IN (
            'pending', 'active', 'delinquent', 'defaulted', 'paidOff',
            'restructured', 'discharged', 'chargedOff', 'cancelled'
        )
    ),
    CONSTRAINT ck_loan_contract_amounts CHECK (
        original_principal_krw > 0
        AND remaining_principal_krw BETWEEN 0 AND original_principal_krw
        AND accrued_interest_krw >= 0
        AND accrued_fee_krw >= 0
    ),
    CONSTRAINT ck_loan_contract_schedule_shape CHECK (
        (
            read_only = FALSE
            AND day_count_denominator = 365
            AND term_months > 0
            AND total_installments = term_months
            AND payment_calendar = 'monthEnd'
            AND grace_months = 0
            AND prepayment_fee_ppm IS NOT NULL
            AND prepayment_effect <> 'forbidden'
            AND maturity_game_day > activated_game_day
            AND (
                (
                    status IN ('pending', 'active', 'delinquent')
                    AND next_installment_no BETWEEN 1 AND total_installments
                )
                OR (
                    status NOT IN ('pending', 'active', 'delinquent')
                    AND next_installment_no IS NULL
                )
            )
        )
        OR (
            read_only = TRUE
            AND origin_kind = 'legacyDebtBridge'
            AND product_kind = 'legacyDebt'
            AND lender_sector = 'bridgeOnly'
            AND rate_status = 'rateUnavailable'
            AND day_count_denominator IS NULL
            AND repayment_method = 'bullet'
            AND term_months IS NULL
            AND total_installments IS NULL
            AND payment_calendar = 'none'
            AND grace_months IS NULL
            AND prepayment_fee_ppm IS NULL
            AND prepayment_effect = 'forbidden'
            AND dsr_included = FALSE
            AND status = 'active'
            AND maturity_game_day IS NULL
            AND next_installment_no IS NULL
            AND oldest_unpaid_due_game_day IS NULL
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE loan_installment (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    loan_contract_id                BIGINT UNSIGNED NOT NULL,
    installment_no                  SMALLINT UNSIGNED NOT NULL,
    due_game_day                    INT UNSIGNED    NOT NULL,
    interest_period_start_game_day  INT UNSIGNED    NOT NULL,
    interest_period_end_game_day    INT UNSIGNED    NOT NULL,
    elapsed_days                    SMALLINT UNSIGNED NOT NULL,
    annual_rate_bp                  SMALLINT UNSIGNED NOT NULL,
    opening_principal_krw           BIGINT          NOT NULL,
    scheduled_fee_krw               BIGINT          NOT NULL,
    scheduled_interest_krw          BIGINT          NOT NULL,
    scheduled_principal_krw         BIGINT          NOT NULL,
    interest_remainder_before       DECIMAL(39, 0) NOT NULL,
    interest_remainder_after        DECIMAL(39, 0) NOT NULL,
    paid_fee_krw                    BIGINT          NOT NULL DEFAULT 0,
    paid_interest_krw               BIGINT          NOT NULL DEFAULT 0,
    paid_principal_krw              BIGINT          NOT NULL DEFAULT 0,
    status                          VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    schedule_revision               INT UNSIGNED    NOT NULL DEFAULT 1,
    created_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_loan_installment_no (loan_contract_id, installment_no),
    UNIQUE KEY uk_loan_installment_save_run_id (save_id, run_revision, id),
    KEY ix_loan_installment_due
        (save_id, run_revision, status, due_game_day, loan_contract_id, installment_no),
    CONSTRAINT fk_loan_installment_contract
        FOREIGN KEY (save_id, run_revision, loan_contract_id)
        REFERENCES loan_contract (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT ck_loan_installment_period CHECK (
        installment_no > 0
        AND interest_period_end_game_day = due_game_day
        AND interest_period_start_game_day <= interest_period_end_game_day
        AND elapsed_days
            = interest_period_end_game_day - interest_period_start_game_day + 1
        AND elapsed_days > 0
    ),
    CONSTRAINT ck_loan_installment_amounts CHECK (
        opening_principal_krw > 0
        AND scheduled_fee_krw >= 0
        AND scheduled_interest_krw >= 0
        AND scheduled_principal_krw >= 0
        AND paid_fee_krw BETWEEN 0 AND scheduled_fee_krw
        AND paid_interest_krw BETWEEN 0 AND scheduled_interest_krw
        AND paid_principal_krw BETWEEN 0 AND scheduled_principal_krw
    ),
    CONSTRAINT ck_loan_installment_status CHECK (
        status IN ('pending', 'due', 'partiallyPaid', 'paid', 'cancelled')
        AND (
            status NOT IN ('paid', 'cancelled')
            OR (
                status = 'paid'
                AND paid_fee_krw = scheduled_fee_krw
                AND paid_interest_krw = scheduled_interest_krw
                AND paid_principal_krw = scheduled_principal_krw
            )
            OR (
                status = 'cancelled'
                AND paid_fee_krw = 0
                AND paid_interest_krw = 0
                AND paid_principal_krw = 0
            )
        )
    ),
    CONSTRAINT ck_loan_installment_revision CHECK (schedule_revision > 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE loan_obligation_bucket (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    loan_contract_id            BIGINT UNSIGNED NOT NULL,
    loan_installment_id         BIGINT UNSIGNED NOT NULL,
    bucket_kind                 VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    due_game_day                INT UNSIGNED    NOT NULL,
    original_amount_krw         BIGINT          NOT NULL,
    paid_amount_krw             BIGINT          NOT NULL DEFAULT 0,
    status                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    delinquent_since_game_day   INT UNSIGNED        NULL,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_loan_bucket_installment_kind (loan_installment_id, bucket_kind),
    UNIQUE KEY uk_loan_bucket_save_run_id (save_id, run_revision, id),
    KEY ix_loan_bucket_priority
        (loan_contract_id, status, due_game_day, bucket_kind, id),
    CONSTRAINT fk_loan_bucket_contract
        FOREIGN KEY (save_id, run_revision, loan_contract_id)
        REFERENCES loan_contract (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_loan_bucket_installment
        FOREIGN KEY (save_id, run_revision, loan_installment_id)
        REFERENCES loan_installment (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT ck_loan_bucket_kind CHECK (
        bucket_kind IN ('fee', 'interest', 'principal')
    ),
    CONSTRAINT ck_loan_bucket_amount CHECK (
        original_amount_krw > 0
        AND paid_amount_krw BETWEEN 0 AND original_amount_krw
    ),
    CONSTRAINT ck_loan_bucket_status CHECK (
        status IN ('pending', 'delinquent', 'paid', 'discharged', 'chargedOff')
        AND (
            (status = 'pending' AND delinquent_since_game_day IS NULL)
            OR (
                status = 'delinquent'
                AND delinquent_since_game_day IS NOT NULL
                AND delinquent_since_game_day >= due_game_day
                AND paid_amount_krw < original_amount_krw
            )
            OR (status = 'paid' AND paid_amount_krw = original_amount_krw)
            OR status IN ('discharged', 'chargedOff')
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE loan_payment (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    loan_contract_id            BIGINT UNSIGNED NOT NULL,
    payment_no                  INT UNSIGNED    NOT NULL,
    payment_kind                VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    amount_krw                  BIGINT          NOT NULL,
    game_day                    INT UNSIGNED    NOT NULL,
    command_id                  CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    status                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ledger_transaction_id       BIGINT UNSIGNED     NULL,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_loan_payment_no (loan_contract_id, payment_no),
    UNIQUE KEY uk_loan_payment_command (save_id, run_revision, command_id),
    UNIQUE KEY uk_loan_payment_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_loan_payment_ledger (save_id, run_revision, ledger_transaction_id),
    CONSTRAINT fk_loan_payment_contract
        FOREIGN KEY (save_id, run_revision, loan_contract_id)
        REFERENCES loan_contract (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_loan_payment_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT ck_loan_payment_kind CHECK (
        payment_kind IN ('scheduledInstallment', 'manualPrepayment')
        AND (
            (payment_kind = 'scheduledInstallment' AND command_id IS NULL)
            OR (
                payment_kind = 'manualPrepayment'
                AND command_id REGEXP
                    '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            )
        )
    ),
    CONSTRAINT ck_loan_payment_amount CHECK (payment_no > 0 AND amount_krw > 0),
    CONSTRAINT ck_loan_payment_status CHECK (
        (status = 'prepared' AND ledger_transaction_id IS NULL)
        OR (status = 'applied' AND ledger_transaction_id IS NOT NULL)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE loan_payment_allocation (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    loan_contract_id            BIGINT UNSIGNED NOT NULL,
    loan_payment_id             BIGINT UNSIGNED NOT NULL,
    loan_obligation_bucket_id   BIGINT UNSIGNED     NULL,
    allocation_order            SMALLINT UNSIGNED NOT NULL,
    allocation_kind             VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    amount_krw                  BIGINT          NOT NULL,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_loan_allocation_order (loan_payment_id, allocation_order),
    UNIQUE KEY uk_loan_allocation_bucket
        (loan_payment_id, loan_obligation_bucket_id),
    KEY ix_loan_allocation_bucket (loan_obligation_bucket_id),
    CONSTRAINT fk_loan_allocation_contract
        FOREIGN KEY (save_id, run_revision, loan_contract_id)
        REFERENCES loan_contract (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_loan_allocation_payment
        FOREIGN KEY (save_id, run_revision, loan_payment_id)
        REFERENCES loan_payment (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_loan_allocation_bucket
        FOREIGN KEY (save_id, run_revision, loan_obligation_bucket_id)
        REFERENCES loan_obligation_bucket (save_id, run_revision, id),
    CONSTRAINT ck_loan_allocation_kind CHECK (
        allocation_kind IN (
            'overdueFee', 'overdueInterest', 'overduePrincipal',
            'currentFee', 'currentInterest', 'currentPrincipal',
            'prepaymentFee', 'prepaymentPrincipal'
        )
        AND (
            (
                allocation_kind IN (
                    'overdueFee', 'overdueInterest', 'overduePrincipal',
                    'currentFee', 'currentInterest', 'currentPrincipal'
                )
                AND loan_obligation_bucket_id IS NOT NULL
            )
            OR (
                allocation_kind IN ('prepaymentFee', 'prepaymentPrincipal')
                AND loan_obligation_bucket_id IS NULL
            )
        )
    ),
    CONSTRAINT ck_loan_allocation_amount CHECK (
        allocation_order > 0 AND amount_krw > 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE loan_rate_reset (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    loan_contract_id                BIGINT UNSIGNED NOT NULL,
    reset_no                        SMALLINT UNSIGNED NOT NULL,
    observation_game_day            INT UNSIGNED    NOT NULL,
    effective_from_game_day         INT UNSIGNED    NOT NULL,
    reference_rate_key              VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    observed_reference_rate_bp      SMALLINT         NOT NULL,
    applied_spread_bp               SMALLINT         NOT NULL,
    unclamped_annual_rate_bp        INT              NOT NULL,
    applied_annual_rate_bp          SMALLINT UNSIGNED NOT NULL,
    prior_level_payment_krw         BIGINT              NULL,
    recalculated_level_payment_krw  BIGINT              NULL,
    created_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_loan_rate_reset_no (loan_contract_id, reset_no),
    UNIQUE KEY uk_loan_rate_reset_effective (loan_contract_id, effective_from_game_day),
    KEY ix_loan_rate_reset_contract_day (loan_contract_id, effective_from_game_day, id),
    CONSTRAINT fk_loan_rate_reset_contract
        FOREIGN KEY (save_id, run_revision, loan_contract_id)
        REFERENCES loan_contract (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT ck_loan_rate_reset_days CHECK (
        reset_no > 0 AND effective_from_game_day = observation_game_day
    ),
    CONSTRAINT ck_loan_rate_reset_payment CHECK (
        (
            prior_level_payment_krw IS NULL
            AND recalculated_level_payment_krw IS NULL
        )
        OR (
            prior_level_payment_krw > 0
            AND recalculated_level_payment_krw > 0
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE credit_history (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    household_id                BIGINT UNSIGNED NOT NULL,
    credit_model_version_id     BIGINT UNSIGNED NOT NULL,
    loan_contract_id            BIGINT UNSIGNED     NULL,
    game_day                    INT UNSIGNED    NOT NULL,
    event_order                 SMALLINT UNSIGNED NOT NULL,
    event_kind                  VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    reason_code                 VARCHAR(48) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    delta_units                 SMALLINT         NOT NULL,
    unclamped_before_units      INT              NOT NULL,
    unclamped_after_units       INT              NOT NULL,
    before_units                SMALLINT UNSIGNED NOT NULL,
    after_units                 SMALLINT UNSIGNED NOT NULL,
    before_band                 VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    after_band                  VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_credit_history_order (household_id, game_day, event_order),
    KEY ix_credit_history_cursor (household_id, game_day, id),
    KEY ix_credit_history_contract (loan_contract_id, game_day, id),
    CONSTRAINT fk_credit_history_state
        FOREIGN KEY (save_id, run_revision, credit_model_version_id)
        REFERENCES credit_state (save_id, run_revision, credit_model_version_id)
        ON DELETE CASCADE,
    CONSTRAINT fk_credit_history_household
        FOREIGN KEY (save_id, run_revision, household_id)
        REFERENCES household (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_credit_history_contract
        FOREIGN KEY (save_id, run_revision, loan_contract_id)
        REFERENCES loan_contract (save_id, run_revision, id),
    CONSTRAINT ck_credit_history_kind CHECK (
        event_kind IN (
            'initial', 'activeToDelinquent', 'delinquentToDefaulted',
            'dailyPenalty', 'cleanRecovery', 'clamp', 'legalProcedure'
        )
    ),
    CONSTRAINT ck_credit_history_units CHECK (
        before_units <= 1000
        AND after_units <= 1000
        AND unclamped_after_units - unclamped_before_units = delta_units
    ),
    CONSTRAINT ck_credit_history_contract_shape CHECK (
        (
            event_kind IN ('activeToDelinquent', 'delinquentToDefaulted')
            AND loan_contract_id IS NOT NULL
        )
        OR (
            event_kind NOT IN ('activeToDelinquent', 'delinquentToDefaulted')
        )
    ),
    CONSTRAINT ck_credit_history_reason CHECK (CHAR_LENGTH(reason_code) > 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE tax_obligation (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    household_id                    BIGINT UNSIGNED NOT NULL,
    policy_set_id                   BIGINT UNSIGNED NOT NULL,
    source_kind                     VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    source_id                       VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    due_game_day                    INT UNSIGNED    NOT NULL,
    original_amount_krw             BIGINT          NOT NULL,
    paid_amount_krw                 BIGINT          NOT NULL DEFAULT 0,
    outstanding_amount_krw          BIGINT          NOT NULL,
    status                          VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    authority_ledger_transaction_id BIGINT UNSIGNED     NULL,
    created_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_tax_obligation_source
        (save_id, run_revision, source_kind, source_id),
    UNIQUE KEY uk_tax_obligation_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_tax_obligation_ledger
        (save_id, run_revision, authority_ledger_transaction_id),
    KEY ix_tax_obligation_active (household_id, status, due_game_day, id),
    CONSTRAINT fk_tax_obligation_household
        FOREIGN KEY (save_id, run_revision, household_id)
        REFERENCES household (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_tax_obligation_policy
        FOREIGN KEY (policy_set_id) REFERENCES policy_set (id),
    CONSTRAINT fk_tax_obligation_ledger
        FOREIGN KEY (save_id, run_revision, authority_ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT ck_tax_obligation_source CHECK (
        source_kind IN ('financialIncomeAssessment', 'yearEndTaxAssessment')
        AND source_id REGEXP '^[1-9][0-9]{0,19}$'
    ),
    CONSTRAINT ck_tax_obligation_amount CHECK (
        original_amount_krw > 0
        AND paid_amount_krw BETWEEN 0 AND original_amount_krw
        AND outstanding_amount_krw = original_amount_krw - paid_amount_krw
    ),
    CONSTRAINT ck_tax_obligation_status CHECK (
        status IN ('prepared', 'outstanding', 'paid', 'discharged', 'chargedOff')
        AND (
            (
                status = 'prepared'
                AND outstanding_amount_krw > 0
                AND authority_ledger_transaction_id IS NULL
            )
            OR (
                status = 'outstanding'
                AND outstanding_amount_krw > 0
                AND authority_ledger_transaction_id IS NOT NULL
            )
            OR (
                status = 'paid'
                AND outstanding_amount_krw = 0
                AND authority_ledger_transaction_id IS NOT NULL
            )
            OR (
                status IN ('discharged', 'chargedOff')
                AND authority_ledger_transaction_id IS NOT NULL
            )
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE loan_authority_bridge (
    loan_contract_id            BIGINT UNSIGNED NOT NULL,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    household_id                BIGINT UNSIGNED NOT NULL,
    bridged_principal_krw       BIGINT          NOT NULL,
    ledger_transaction_id       BIGINT UNSIGNED NOT NULL,
    bridge_key                  VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (loan_contract_id),
    UNIQUE KEY uk_loan_authority_bridge_household (household_id),
    UNIQUE KEY uk_loan_authority_bridge_ledger
        (save_id, run_revision, ledger_transaction_id),
    CONSTRAINT fk_loan_authority_bridge_contract
        FOREIGN KEY (save_id, run_revision, loan_contract_id)
        REFERENCES loan_contract (save_id, run_revision, id),
    CONSTRAINT fk_loan_authority_bridge_household
        FOREIGN KEY (save_id, run_revision, household_id)
        REFERENCES household (save_id, run_revision, id),
    CONSTRAINT fk_loan_authority_bridge_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT ck_loan_authority_bridge_amount CHECK (bridged_principal_krw > 0),
    CONSTRAINT ck_loan_authority_bridge_key CHECK (bridge_key = 'migration0029LegacyDebt')
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE tax_authority_bridge (
    tax_obligation_id           BIGINT UNSIGNED NOT NULL,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    household_id                BIGINT UNSIGNED NOT NULL,
    bridged_amount_krw          BIGINT          NOT NULL,
    ledger_transaction_id       BIGINT UNSIGNED NOT NULL,
    bridge_key                  VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (tax_obligation_id),
    UNIQUE KEY uk_tax_authority_bridge_ledger
        (save_id, run_revision, ledger_transaction_id),
    CONSTRAINT fk_tax_authority_bridge_obligation
        FOREIGN KEY (save_id, run_revision, tax_obligation_id)
        REFERENCES tax_obligation (save_id, run_revision, id),
    CONSTRAINT fk_tax_authority_bridge_household
        FOREIGN KEY (save_id, run_revision, household_id)
        REFERENCES household (save_id, run_revision, id),
    CONSTRAINT fk_tax_authority_bridge_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT ck_tax_authority_bridge_amount CHECK (bridged_amount_krw > 0),
    CONSTRAINT ck_tax_authority_bridge_key CHECK (bridge_key = 'migration0029TaxDebt')
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

-- Add typed posting ownership before runtime triggers inspect these references.
ALTER TABLE ledger_posting
    ADD COLUMN loan_contract_id BIGINT UNSIGNED NULL AFTER essential_arrear_id,
    ADD COLUMN tax_obligation_id BIGINT UNSIGNED NULL AFTER loan_contract_id;

CREATE TRIGGER tr_credit_state_valid_insert
BEFORE INSERT ON credit_state
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.credit_units = 700
        AND NEW.credit_band = 'standard'
        AND NEW.evaluation_revision = 0
        AND EXISTS (
            SELECT 1
            FROM run_rule_bundle AS bundle
            INNER JOIN credit_model_version AS model
                ON model.id = bundle.credit_model_version_id
               AND model.availability = 'active'
               AND model.sealed_at IS NOT NULL
            INNER JOIN household
                ON household.save_id = bundle.save_id
               AND household.run_revision = bundle.run_revision
               AND household.id = NEW.household_id
            INNER JOIN save
                ON save.id = bundle.save_id
               AND save.run_revision = bundle.run_revision
            WHERE bundle.save_id = NEW.save_id
              AND bundle.run_revision = NEW.run_revision
              AND bundle.credit_model_version_id = NEW.credit_model_version_id
              AND save.game_day = NEW.last_evaluated_game_day
              AND CAST(JSON_UNQUOTE(JSON_EXTRACT(model.parameters, '$.creditUnits.initial'))
                       AS UNSIGNED) = NEW.credit_units
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_credit_state_transition_only
BEFORE UPDATE ON credit_state
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.household_id = OLD.household_id
        AND NEW.credit_model_version_id = OLD.credit_model_version_id
        AND NEW.last_evaluated_game_day >= OLD.last_evaluated_game_day
        AND NEW.evaluation_revision = OLD.evaluation_revision + 1
        AND NEW.created_at = OLD.created_at,
    OLD.save_id,
    NULL
);

CREATE TRIGGER tr_credit_state_no_delete
BEFORE DELETE ON credit_state
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'credit state belongs to the run';

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
        INNER JOIN loan_product_version AS product
            ON product.id = NEW.loan_product_version_id
           AND product.credit_model_version_id = model.id
           AND product.catalog_scope = 'modelChild'
           AND product.product_kind = 'unsecuredLoan'
           AND product.quote_eligible = TRUE
           AND product.execution_eligible = TRUE
           AND product.sealed_at IS NOT NULL
        WHERE save.id = NEW.save_id
          AND save.run_revision = NEW.run_revision
          AND save.state_revision = NEW.expected_state_revision
          AND save.game_day = NEW.created_game_day
          AND NEW.requested_principal_krw
              BETWEEN product.minimum_principal_krw AND product.maximum_principal_krw
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_loan_quote_no_update
BEFORE UPDATE ON loan_quote
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'loan quotes are immutable';

CREATE TRIGGER tr_loan_quote_no_delete
BEFORE DELETE ON loan_quote
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'loan quotes are immutable';

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
                  AND (
                      (product.day_count_rule = 'actual365'
                       AND NEW.day_count_denominator = 365)
                  )
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
                      BETWEEN product.minimum_principal_krw AND product.maximum_principal_krw
                  AND (
                      NEW.origin_kind <> 'quoteExecution'
                      OR (
                          quote.decision_code = 'eligible'
                          AND quote.created_game_day = NEW.activated_game_day
                          AND quote.expires_game_day = NEW.activated_game_day
                          AND quote.loan_product_version_id = product.id
                          AND quote.requested_principal_krw = NEW.original_principal_krw
                      )
                  )
              )
              OR (
                  NEW.origin_kind = 'legacyDebtBridge'
                  AND product.catalog_scope = 'bridgeOnly'
                  AND product.credit_model_version_id IS NULL
                  AND product.product_key = 'compat-legacy-debt-zero-bullet-v1'
                  AND model.availability = 'disabled'
                  AND NEW.original_principal_krw = household.legacy_debt_krw_at_activation
                  AND NEW.remaining_principal_krw = NEW.original_principal_krw
              )
          )
    ),
    NEW.save_id,
    NULL
);

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
            OR (OLD.status = 'active' AND NEW.status IN ('active', 'delinquent', 'paidOff'))
            OR (OLD.status = 'delinquent' AND NEW.status IN ('active', 'delinquent', 'defaulted'))
            OR (OLD.status = 'defaulted'
                AND NEW.status IN ('defaulted', 'restructured', 'discharged', 'chargedOff'))
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_loan_contract_no_delete
BEFORE DELETE ON loan_contract
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'loan contracts are durable obligations';

CREATE TRIGGER tr_loan_installment_valid_insert
BEFORE INSERT ON loan_installment
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'pending'
        AND NEW.paid_fee_krw = 0
        AND NEW.paid_interest_krw = 0
        AND NEW.paid_principal_krw = 0
        AND EXISTS (
            SELECT 1 FROM loan_contract AS contract
            WHERE contract.id = NEW.loan_contract_id
              AND contract.save_id = NEW.save_id
              AND contract.run_revision = NEW.run_revision
              AND contract.read_only = FALSE
              AND contract.status IN ('pending', 'active')
              AND NEW.installment_no <= contract.total_installments
              AND NEW.annual_rate_bp BETWEEN
                    contract.minimum_annual_rate_bp AND contract.maximum_annual_rate_bp
              AND NEW.scheduled_principal_krw <= NEW.opening_principal_krw
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_loan_installment_transition_only
BEFORE UPDATE ON loan_installment
FOR EACH ROW
SET NEW.id = IF(
    NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.loan_contract_id = OLD.loan_contract_id
        AND NEW.installment_no = OLD.installment_no
        AND NEW.created_at = OLD.created_at
        AND (
            (
                OLD.status = 'pending'
                AND NEW.status = 'pending'
                AND NEW.schedule_revision = OLD.schedule_revision + 1
                AND NEW.paid_fee_krw = 0
                AND NEW.paid_interest_krw = 0
                AND NEW.paid_principal_krw = 0
            )
            OR (
                OLD.status IN ('pending', 'due', 'partiallyPaid')
                AND NEW.status IN ('due', 'partiallyPaid', 'paid')
                AND NEW.due_game_day = OLD.due_game_day
                AND NEW.interest_period_start_game_day
                    = OLD.interest_period_start_game_day
                AND NEW.interest_period_end_game_day = OLD.interest_period_end_game_day
                AND NEW.elapsed_days = OLD.elapsed_days
                AND NEW.annual_rate_bp = OLD.annual_rate_bp
                AND NEW.opening_principal_krw = OLD.opening_principal_krw
                AND NEW.scheduled_fee_krw = OLD.scheduled_fee_krw
                AND NEW.scheduled_interest_krw = OLD.scheduled_interest_krw
                AND NEW.scheduled_principal_krw = OLD.scheduled_principal_krw
                AND NEW.interest_remainder_before = OLD.interest_remainder_before
                AND NEW.interest_remainder_after = OLD.interest_remainder_after
                AND NEW.paid_fee_krw >= OLD.paid_fee_krw
                AND NEW.paid_interest_krw >= OLD.paid_interest_krw
                AND NEW.paid_principal_krw >= OLD.paid_principal_krw
                AND NEW.schedule_revision = OLD.schedule_revision
            )
            OR (
                OLD.status = 'pending'
                AND NEW.status = 'cancelled'
                AND NEW.due_game_day = OLD.due_game_day
                AND NEW.interest_period_start_game_day
                    = OLD.interest_period_start_game_day
                AND NEW.interest_period_end_game_day = OLD.interest_period_end_game_day
                AND NEW.elapsed_days = OLD.elapsed_days
                AND NEW.annual_rate_bp = OLD.annual_rate_bp
                AND NEW.opening_principal_krw = OLD.opening_principal_krw
                AND NEW.scheduled_fee_krw = OLD.scheduled_fee_krw
                AND NEW.scheduled_interest_krw = OLD.scheduled_interest_krw
                AND NEW.scheduled_principal_krw = OLD.scheduled_principal_krw
                AND NEW.interest_remainder_before = OLD.interest_remainder_before
                AND NEW.interest_remainder_after = OLD.interest_remainder_after
                AND NEW.schedule_revision = OLD.schedule_revision
            )
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_loan_installment_no_delete
BEFORE DELETE ON loan_installment
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'loan installments are durable history';

CREATE TRIGGER tr_loan_bucket_valid_insert
BEFORE INSERT ON loan_obligation_bucket
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'pending'
        AND NEW.paid_amount_krw = 0
        AND NEW.delinquent_since_game_day IS NULL
        AND EXISTS (
            SELECT 1 FROM loan_installment AS installment
            WHERE installment.id = NEW.loan_installment_id
              AND installment.save_id = NEW.save_id
              AND installment.run_revision = NEW.run_revision
              AND installment.loan_contract_id = NEW.loan_contract_id
              AND installment.due_game_day = NEW.due_game_day
              AND NEW.original_amount_krw = CASE NEW.bucket_kind
                    WHEN 'fee' THEN installment.scheduled_fee_krw
                    WHEN 'interest' THEN installment.scheduled_interest_krw
                    WHEN 'principal' THEN installment.scheduled_principal_krw
                  END
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_loan_bucket_transition_only
BEFORE UPDATE ON loan_obligation_bucket
FOR EACH ROW
SET NEW.id = IF(
    NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.loan_contract_id = OLD.loan_contract_id
        AND NEW.loan_installment_id = OLD.loan_installment_id
        AND BINARY NEW.bucket_kind = BINARY OLD.bucket_kind
        AND NEW.due_game_day = OLD.due_game_day
        AND NEW.original_amount_krw = OLD.original_amount_krw
        AND NEW.paid_amount_krw >= OLD.paid_amount_krw
        AND NEW.created_at = OLD.created_at
        AND (
            (OLD.status = 'pending' AND NEW.status IN ('pending', 'delinquent', 'paid'))
            OR (OLD.status = 'delinquent' AND NEW.status IN ('delinquent', 'paid'))
            OR (OLD.status IN ('pending', 'delinquent')
                AND NEW.status IN ('discharged', 'chargedOff'))
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_loan_bucket_no_delete
BEFORE DELETE ON loan_obligation_bucket
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'loan obligation buckets are durable history';

CREATE TRIGGER tr_loan_payment_valid_insert
BEFORE INSERT ON loan_payment
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'prepared'
        AND NEW.ledger_transaction_id IS NULL
        AND EXISTS (
            SELECT 1 FROM loan_contract AS contract
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
        AND NEW.created_at = OLD.created_at
        AND NEW.amount_krw = (
            SELECT COALESCE(SUM(allocation.amount_krw), 0)
            FROM loan_payment_allocation AS allocation
            WHERE allocation.loan_payment_id = OLD.id
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_loan_payment_no_delete
BEFORE DELETE ON loan_payment
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'loan payments are immutable history';

CREATE TRIGGER tr_loan_allocation_valid_insert
BEFORE INSERT ON loan_payment_allocation
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1 FROM loan_payment AS payment
        WHERE payment.id = NEW.loan_payment_id
          AND payment.save_id = NEW.save_id
          AND payment.run_revision = NEW.run_revision
          AND payment.loan_contract_id = NEW.loan_contract_id
          AND payment.status = 'prepared'
    )
        AND (
            NEW.loan_obligation_bucket_id IS NULL
            OR EXISTS (
                SELECT 1 FROM loan_obligation_bucket AS bucket
                WHERE bucket.id = NEW.loan_obligation_bucket_id
                  AND bucket.save_id = NEW.save_id
                  AND bucket.run_revision = NEW.run_revision
                  AND bucket.loan_contract_id = NEW.loan_contract_id
                  AND bucket.status IN ('pending', 'delinquent')
            )
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_loan_allocation_no_update
BEFORE UPDATE ON loan_payment_allocation
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'loan payment allocations are immutable';

CREATE TRIGGER tr_loan_allocation_no_delete
BEFORE DELETE ON loan_payment_allocation
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'loan payment allocations are immutable';

CREATE TRIGGER tr_loan_rate_reset_valid_insert
BEFORE INSERT ON loan_rate_reset
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1 FROM loan_contract AS contract
        WHERE contract.id = NEW.loan_contract_id
          AND contract.save_id = NEW.save_id
          AND contract.run_revision = NEW.run_revision
          AND contract.status IN ('active', 'delinquent')
          AND contract.rate_type = 'variable'
          AND BINARY contract.reference_rate_key = BINARY NEW.reference_rate_key
          AND contract.applied_spread_bp = NEW.applied_spread_bp
          AND NEW.unclamped_annual_rate_bp
              = NEW.observed_reference_rate_bp + NEW.applied_spread_bp
          AND NEW.applied_annual_rate_bp = LEAST(
              GREATEST(NEW.unclamped_annual_rate_bp, contract.minimum_annual_rate_bp),
              contract.maximum_annual_rate_bp
          )
          AND (
              (contract.repayment_method = 'levelPayment'
               AND NEW.prior_level_payment_krw IS NOT NULL)
              OR (contract.repayment_method <> 'levelPayment'
                  AND NEW.prior_level_payment_krw IS NULL)
          )
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_loan_rate_reset_no_update
BEFORE UPDATE ON loan_rate_reset
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'loan rate resets are immutable';

CREATE TRIGGER tr_loan_rate_reset_no_delete
BEFORE DELETE ON loan_rate_reset
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'loan rate resets are immutable';

CREATE TRIGGER tr_credit_history_valid_insert
BEFORE INSERT ON credit_history
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM credit_state AS state
        INNER JOIN credit_model_version AS model
            ON model.id = state.credit_model_version_id
           AND model.availability = 'active'
           AND model.sealed_at IS NOT NULL
        WHERE state.save_id = NEW.save_id
          AND state.run_revision = NEW.run_revision
          AND state.household_id = NEW.household_id
          AND state.credit_model_version_id = NEW.credit_model_version_id
          AND NEW.event_order > 0
          AND (
              (NEW.event_kind = 'initial' AND NEW.delta_units = 0)
              OR (
                  NEW.event_kind = 'activeToDelinquent'
                  AND NEW.delta_units = CAST(JSON_UNQUOTE(JSON_EXTRACT(
                      model.parameters,
                      '$.eventPenalty.activeToDelinquentUnits'
                  )) AS SIGNED)
              )
              OR (
                  NEW.event_kind = 'delinquentToDefaulted'
                  AND NEW.delta_units = CAST(JSON_UNQUOTE(JSON_EXTRACT(
                      model.parameters,
                      '$.eventPenalty.delinquentToDefaultedUnits'
                  )) AS SIGNED)
              )
              OR (
                  NEW.event_kind = 'dailyPenalty'
                  AND NEW.delta_units = CAST(JSON_UNQUOTE(JSON_EXTRACT(
                      model.parameters,
                      '$.dailyChange.delinquentOrDefaultedPenaltyUnits'
                  )) AS SIGNED)
              )
              OR (
                  NEW.event_kind = 'cleanRecovery'
                  AND NEW.delta_units = CAST(JSON_UNQUOTE(JSON_EXTRACT(
                      model.parameters,
                      '$.dailyChange.cleanRecoveryUnits'
                  )) AS SIGNED)
              )
              OR (
                  NEW.event_kind = 'legalProcedure'
                  AND NEW.delta_units = CAST(JSON_UNQUOTE(JSON_EXTRACT(
                      model.parameters,
                      '$.eventPenalty.legalProcedureUnits'
                  )) AS SIGNED)
              )
              OR NEW.event_kind = 'clamp'
          )
    )
        AND (
            NEW.loan_contract_id IS NULL
            OR EXISTS (
                SELECT 1 FROM loan_contract AS contract
                WHERE contract.id = NEW.loan_contract_id
                  AND contract.save_id = NEW.save_id
                  AND contract.run_revision = NEW.run_revision
                  AND contract.household_id = NEW.household_id
            )
        )
        AND (
            (NEW.before_band = 'prime' AND NEW.before_units BETWEEN 850 AND 1000)
            OR (NEW.before_band = 'standard' AND NEW.before_units BETWEEN 650 AND 849)
            OR (NEW.before_band = 'limited' AND NEW.before_units BETWEEN 450 AND 649)
            OR (NEW.before_band = 'distressed' AND NEW.before_units BETWEEN 1 AND 449)
            OR (NEW.before_band = 'insolvent' AND NEW.before_units = 0)
        )
        AND (
            (NEW.after_band = 'prime' AND NEW.after_units BETWEEN 850 AND 1000)
            OR (NEW.after_band = 'standard' AND NEW.after_units BETWEEN 650 AND 849)
            OR (NEW.after_band = 'limited' AND NEW.after_units BETWEEN 450 AND 649)
            OR (NEW.after_band = 'distressed' AND NEW.after_units BETWEEN 1 AND 449)
            OR (NEW.after_band = 'insolvent' AND NEW.after_units = 0)
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_credit_history_no_update
BEFORE UPDATE ON credit_history
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'credit history is append-only';

CREATE TRIGGER tr_credit_history_no_delete
BEFORE DELETE ON credit_history
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'credit history is append-only';

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

CREATE TRIGGER tr_tax_obligation_no_delete
BEFORE DELETE ON tax_obligation
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'tax obligations are durable authority';

CREATE TRIGGER tr_loan_authority_bridge_valid_insert
BEFORE INSERT ON loan_authority_bridge
FOR EACH ROW
SET NEW.loan_contract_id = IF(
    EXISTS (
        SELECT 1
        FROM loan_contract AS contract
        INNER JOIN household
            ON household.id = contract.household_id
           AND household.save_id = contract.save_id
           AND household.run_revision = contract.run_revision
        INNER JOIN ledger_transaction AS ledger
            ON ledger.id = NEW.ledger_transaction_id
           AND ledger.save_id = contract.save_id
           AND ledger.run_revision = contract.run_revision
           AND ledger.source_kind = 'debtAuthorityBridge'
           AND BINARY ledger.source_id = BINARY CAST(contract.id AS CHAR)
        WHERE contract.id = NEW.loan_contract_id
          AND contract.save_id = NEW.save_id
          AND contract.run_revision = NEW.run_revision
          AND contract.household_id = NEW.household_id
          AND contract.origin_kind = 'legacyDebtBridge'
          AND contract.read_only = TRUE
          AND contract.original_principal_krw = NEW.bridged_principal_krw
          AND household.legacy_debt_krw_at_activation = NEW.bridged_principal_krw
          AND NEW.bridged_principal_krw = -(
              SELECT COALESCE(SUM(posting.amount_krw), 0)
              FROM ledger_posting AS posting
              WHERE posting.ledger_transaction_id = ledger.id
                AND posting.account_code = 'loanPrincipalLiability'
                AND posting.loan_contract_id = contract.id
          )
          AND NEW.bridged_principal_krw = (
              SELECT COALESCE(SUM(posting.amount_krw), 0)
              FROM ledger_posting AS posting
              WHERE posting.ledger_transaction_id = ledger.id
                AND posting.account_code = 'debtPrincipal'
          )
    ),
    NEW.loan_contract_id,
    NULL
);

CREATE TRIGGER tr_loan_authority_bridge_no_update
BEFORE UPDATE ON loan_authority_bridge
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'loan authority bridges are immutable';

CREATE TRIGGER tr_loan_authority_bridge_no_delete
BEFORE DELETE ON loan_authority_bridge
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'loan authority bridges are immutable';

CREATE TRIGGER tr_tax_authority_bridge_valid_insert
BEFORE INSERT ON tax_authority_bridge
FOR EACH ROW
SET NEW.tax_obligation_id = IF(
    EXISTS (
        SELECT 1
        FROM tax_obligation AS obligation
        INNER JOIN ledger_transaction AS ledger
            ON ledger.id = NEW.ledger_transaction_id
           AND ledger.save_id = obligation.save_id
           AND ledger.run_revision = obligation.run_revision
           AND ledger.source_kind = 'debtAuthorityBridge'
           AND BINARY ledger.source_id
                = BINARY CONCAT('taxObligation:', CAST(obligation.id AS CHAR))
        WHERE obligation.id = NEW.tax_obligation_id
          AND obligation.save_id = NEW.save_id
          AND obligation.run_revision = NEW.run_revision
          AND obligation.household_id = NEW.household_id
          AND obligation.status = 'outstanding'
          AND obligation.original_amount_krw = NEW.bridged_amount_krw
          AND NEW.bridged_amount_krw = -(
              SELECT COALESCE(SUM(posting.amount_krw), 0)
              FROM ledger_posting AS posting
              WHERE posting.ledger_transaction_id = ledger.id
                AND posting.account_code = 'taxObligationLiability'
                AND posting.tax_obligation_id = obligation.id
          )
          AND NEW.bridged_amount_krw = (
              SELECT COALESCE(SUM(posting.amount_krw), 0)
              FROM ledger_posting AS posting
              WHERE posting.ledger_transaction_id = ledger.id
                AND posting.account_code = 'debtPrincipal'
          )
    ),
    NEW.tax_obligation_id,
    NULL
);

CREATE TRIGGER tr_tax_authority_bridge_no_update
BEFORE UPDATE ON tax_authority_bridge
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'tax authority bridges are immutable';

CREATE TRIGGER tr_tax_authority_bridge_no_delete
BEFORE DELETE ON tax_authority_bridge
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'tax authority bridges are immutable';

-- Loan installments join the closed settlement protocol with an exact version-1 payload.
ALTER TABLE scheduled_settlement
    DROP CHECK ck_scheduled_settlement_kind,
    DROP CHECK ck_scheduled_settlement_source_kind,
    ADD CONSTRAINT ck_scheduled_settlement_kind CHECK (
        kind IN (
            'cmaInterest', 'depositMaturity', 'savingsInstallment', 'savingsMaturity',
            'bondCoupon', 'bondMaturity', 'llxDistribution', 'financialIncomeFiling',
            'employmentPayroll', 'employmentReconciliation', 'militaryPay',
            'militarySavingsInstallment', 'militarySavingsMaturity',
            'militarySavingsGovernmentMatch', 'livingCostMonth', 'loanInstallment'
        )
    ),
    ADD CONSTRAINT ck_scheduled_settlement_source_kind CHECK (
        source_kind IN (
            'cmaAccount', 'depositContract', 'savingsContract', 'bondPosition',
            'indexPosition', 'taxYear', 'employmentContract', 'yearEndTaxAssessment',
            'militaryService', 'militarySavingsContract', 'militarySavingsInstallment',
            'livingCostMonth', 'loanContract'
        )
    ),
    ADD CONSTRAINT ck_scheduled_settlement_loan_payload CHECK (
        kind <> 'loanInstallment'
        OR (
            JSON_TYPE(payload) = 'OBJECT'
            AND JSON_LENGTH(payload) = 3
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.version')) = 'INTEGER'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.version')) = '1'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.loanContractId')) = 'STRING'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.loanContractId'))
                REGEXP '^[1-9][0-9]{0,19}$'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.installmentNo')) = 'INTEGER'
            AND CAST(JSON_UNQUOTE(JSON_EXTRACT(payload, '$.installmentNo')) AS UNSIGNED) > 0
            AND source_kind = 'loanContract'
            AND BINARY source_id
                = BINARY JSON_UNQUOTE(JSON_EXTRACT(payload, '$.loanContractId'))
            AND occurrence
                = CAST(JSON_UNQUOTE(JSON_EXTRACT(payload, '$.installmentNo')) AS UNSIGNED)
        )
    );

CREATE TRIGGER tr_scheduled_settlement_loan_insert
BEFORE INSERT ON scheduled_settlement
FOR EACH ROW
FOLLOWS tr_scheduled_settlement_living_cost_insert
SET NEW.status = IF(
    NEW.kind <> 'loanInstallment'
        OR EXISTS (
            SELECT 1
            FROM loan_contract AS contract
            INNER JOIN loan_installment AS installment
                ON installment.loan_contract_id = contract.id
               AND installment.installment_no = CAST(
                   JSON_UNQUOTE(JSON_EXTRACT(NEW.payload, '$.installmentNo')) AS UNSIGNED
               )
            WHERE contract.id = CAST(
                      JSON_UNQUOTE(JSON_EXTRACT(NEW.payload, '$.loanContractId')) AS UNSIGNED
                  )
              AND contract.save_id = NEW.save_id
              AND contract.run_revision = NEW.run_revision
              AND contract.read_only = FALSE
              AND contract.status IN ('active', 'delinquent')
              AND installment.save_id = NEW.save_id
              AND installment.run_revision = NEW.run_revision
              AND installment.status = 'pending'
              AND installment.due_game_day = NEW.due_game_day
        ),
    NEW.status,
    NULL
);

ALTER TABLE ledger_transaction
    ADD CONSTRAINT ck_ledger_transaction_loan_source CHECK (
        source_kind NOT LIKE 'loan%'
        AND source_kind <> 'debtAuthorityBridge'
        OR source_kind IN (
            'loanOrigination', 'loanInstallment', 'loanPrepayment', 'debtAuthorityBridge'
        )
    );

ALTER TABLE ledger_posting
    DROP CHECK ck_ledger_posting_account_code,
    DROP CHECK ck_ledger_posting_account_reference,
    ADD KEY ix_ledger_posting_loan_contract
        (save_id, run_revision, loan_contract_id),
    ADD KEY ix_ledger_posting_tax_obligation
        (save_id, run_revision, tax_obligation_id),
    ADD CONSTRAINT fk_ledger_posting_loan_contract
        FOREIGN KEY (save_id, run_revision, loan_contract_id)
        REFERENCES loan_contract (save_id, run_revision, id),
    ADD CONSTRAINT fk_ledger_posting_tax_obligation
        FOREIGN KEY (save_id, run_revision, tax_obligation_id)
        REFERENCES tax_obligation (save_id, run_revision, id),
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
            'loanFeeExpense', 'taxObligationLiability'
        )
    ),
    ADD CONSTRAINT ck_ledger_posting_account_reference CHECK (
        (
            account_code IN (
                'accountCash', 'productPrincipal',
                'pensionTaxExcludedContribution', 'pensionCreditedContribution'
            )
            AND financial_account_id IS NOT NULL
            AND military_savings_contract_id IS NULL
            AND living_cost_month_id IS NULL
            AND essential_arrear_id IS NULL
            AND loan_contract_id IS NULL
            AND tax_obligation_id IS NULL
        )
        OR (
            account_code IN (
                'militarySavingsPrincipal', 'militarySavingsBankInterest',
                'militarySavingsGovernmentMatchIncome'
            )
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NOT NULL
            AND living_cost_month_id IS NULL
            AND essential_arrear_id IS NULL
            AND loan_contract_id IS NULL
            AND tax_obligation_id IS NULL
        )
        OR (
            account_code = 'livingCostExpense'
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NULL
            AND living_cost_month_id IS NOT NULL
            AND essential_arrear_id IS NULL
            AND loan_contract_id IS NULL
            AND tax_obligation_id IS NULL
        )
        OR (
            account_code = 'essentialArrearLiability'
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NULL
            AND living_cost_month_id IS NULL
            AND essential_arrear_id IS NOT NULL
            AND loan_contract_id IS NULL
            AND tax_obligation_id IS NULL
        )
        OR (
            account_code IN (
                'loanPrincipalLiability', 'loanInterestExpense',
                'loanInterestLiability', 'loanFeeExpense'
            )
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NULL
            AND living_cost_month_id IS NULL
            AND essential_arrear_id IS NULL
            AND loan_contract_id IS NOT NULL
            AND tax_obligation_id IS NULL
        )
        OR (
            account_code = 'taxObligationLiability'
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NULL
            AND living_cost_month_id IS NULL
            AND essential_arrear_id IS NULL
            AND loan_contract_id IS NULL
            AND tax_obligation_id IS NOT NULL
        )
        OR (
            account_code NOT IN (
                'accountCash', 'productPrincipal',
                'pensionTaxExcludedContribution', 'pensionCreditedContribution',
                'militarySavingsPrincipal', 'militarySavingsBankInterest',
                'militarySavingsGovernmentMatchIncome',
                'livingCostExpense', 'essentialArrearLiability',
                'loanPrincipalLiability', 'loanInterestExpense',
                'loanInterestLiability', 'loanFeeExpense', 'taxObligationLiability'
            )
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NULL
            AND living_cost_month_id IS NULL
            AND essential_arrear_id IS NULL
            AND loan_contract_id IS NULL
            AND tax_obligation_id IS NULL
        )
    );

CREATE TRIGGER tr_ledger_transaction_loan_source_insert
BEFORE INSERT ON ledger_transaction
FOR EACH ROW
FOLLOWS tr_ledger_transaction_life_source_insert
SET NEW.source_kind = IF(
    (
        NEW.source_kind IN ('loanOrigination', 'debtAuthorityBridge')
        AND EXISTS (
            SELECT 1 FROM loan_contract AS contract
            WHERE contract.id = CAST(NEW.source_id AS UNSIGNED)
              AND BINARY NEW.source_id = BINARY CAST(contract.id AS CHAR)
              AND contract.save_id = NEW.save_id
              AND contract.run_revision = NEW.run_revision
              AND (
                  (NEW.source_kind = 'loanOrigination'
                   AND contract.origin_kind <> 'legacyDebtBridge')
                  OR (NEW.source_kind = 'debtAuthorityBridge'
                      AND contract.origin_kind = 'legacyDebtBridge')
              )
        )
    )
    OR (
        NEW.source_kind = 'debtAuthorityBridge'
        AND NEW.source_id REGEXP '^taxObligation:[1-9][0-9]{0,19}$'
        AND EXISTS (
            SELECT 1 FROM tax_obligation AS obligation
            WHERE obligation.id = CAST(SUBSTRING_INDEX(NEW.source_id, ':', -1) AS UNSIGNED)
              AND BINARY NEW.source_id
                    = BINARY CONCAT('taxObligation:', CAST(obligation.id AS CHAR))
              AND obligation.save_id = NEW.save_id
              AND obligation.run_revision = NEW.run_revision
              AND obligation.status = 'outstanding'
        )
    )
    OR (
        NEW.source_kind IN ('loanInstallment', 'loanPrepayment')
        AND EXISTS (
            SELECT 1 FROM loan_payment AS payment
            WHERE payment.id = CAST(NEW.source_id AS UNSIGNED)
              AND BINARY NEW.source_id = BINARY CAST(payment.id AS CHAR)
              AND payment.save_id = NEW.save_id
              AND payment.run_revision = NEW.run_revision
              AND payment.status = 'prepared'
              AND (
                  (NEW.source_kind = 'loanInstallment'
                   AND payment.payment_kind = 'scheduledInstallment')
                  OR (NEW.source_kind = 'loanPrepayment'
                      AND payment.payment_kind = 'manualPrepayment')
              )
        )
    )
    OR NEW.source_kind NOT IN (
        'loanOrigination', 'loanInstallment', 'loanPrepayment', 'debtAuthorityBridge'
    ),
    NEW.source_kind,
    NULL
);

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
