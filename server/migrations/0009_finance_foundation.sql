-- M2-A immutable policy data, run-scoped accounts, and the finance audit foundation.

CREATE TABLE policy_set (
    id              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    policy_key      VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    basis_date      DATE            NOT NULL,
    sealed_at       DATETIME(3)         NULL,
    created_at      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_policy_set_policy_key (policy_key)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE policy_rule (
    id              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    policy_set_id   BIGINT UNSIGNED NOT NULL,
    domain          VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    rule_key        VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    effective_from  DATE            NOT NULL,
    effective_to    DATE                NULL,
    parameters      JSON            NOT NULL,
    created_at      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_policy_rule_set_domain_key_from
        (policy_set_id, domain, rule_key, effective_from),
    KEY ix_policy_rule_lookup
        (policy_set_id, domain, rule_key, effective_from, effective_to),
    CONSTRAINT fk_policy_rule_policy_set
        FOREIGN KEY (policy_set_id) REFERENCES policy_set (id),
    CONSTRAINT ck_policy_rule_domain CHECK (CHAR_LENGTH(domain) > 0),
    CONSTRAINT ck_policy_rule_key CHECK (CHAR_LENGTH(rule_key) > 0),
    CONSTRAINT ck_policy_rule_period
        CHECK (effective_to IS NULL OR effective_to >= effective_from)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_policy_set_draft_insert_only
BEFORE INSERT ON policy_set
FOR EACH ROW
SET NEW.policy_key = IF(NEW.sealed_at IS NULL, NEW.policy_key, NULL);

-- A policy set can only transition once from draft to sealed; its identity never changes.
CREATE TRIGGER tr_policy_set_seal_only
BEFORE UPDATE ON policy_set
FOR EACH ROW
SET NEW.policy_key = IF(
    OLD.sealed_at IS NULL
        AND NEW.sealed_at IS NOT NULL
        AND NEW.id = OLD.id
        AND BINARY NEW.policy_key = BINARY OLD.policy_key
        AND NEW.basis_date = OLD.basis_date
        AND NEW.created_at = OLD.created_at,
    NEW.policy_key,
    NULL
);

CREATE TRIGGER tr_policy_set_no_delete
BEFORE DELETE ON policy_set
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'policy set rows are immutable';

-- Rules can only be inserted while their set is still a draft.
CREATE TRIGGER tr_policy_rule_draft_insert_only
BEFORE INSERT ON policy_rule
FOR EACH ROW
SET NEW.domain = IF(
    EXISTS (
        SELECT 1
        FROM policy_set
        WHERE id = NEW.policy_set_id
          AND sealed_at IS NULL
    ) AND JSON_TYPE(NEW.parameters) = 'OBJECT',
    NEW.domain,
    NULL
);

CREATE TRIGGER tr_policy_rule_no_update
BEFORE UPDATE ON policy_rule
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'policy rule rows are immutable';

CREATE TRIGGER tr_policy_rule_no_delete
BEFORE DELETE ON policy_rule
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'policy rule rows are immutable';

INSERT INTO policy_set (id, policy_key, basis_date)
VALUES (1, 'kr-individual-2026-v1', '2026-01-01');

INSERT INTO policy_rule
    (policy_set_id, domain, rule_key, effective_from, effective_to, parameters)
VALUES
    (
        1,
        'tax',
        'generalFinancialIncome',
        '2026-01-01',
        NULL,
        JSON_OBJECT(
            'incomeTaxPpm', 140000,
            'localIncomeTaxPpm', 14000,
            'comprehensiveThresholdKrw', 20000000
        )
    ),
    (
        1,
        'tax',
        'basicIncomeBrackets',
        '2026-01-01',
        NULL,
        JSON_OBJECT(
            'brackets', JSON_ARRAY(
                JSON_OBJECT('upperBoundKrw', 14000000, 'ratePpm', 60000),
                JSON_OBJECT('upperBoundKrw', 50000000, 'ratePpm', 150000),
                JSON_OBJECT('upperBoundKrw', 88000000, 'ratePpm', 240000),
                JSON_OBJECT('upperBoundKrw', 150000000, 'ratePpm', 350000),
                JSON_OBJECT('upperBoundKrw', 300000000, 'ratePpm', 380000),
                JSON_OBJECT('upperBoundKrw', 500000000, 'ratePpm', 400000),
                JSON_OBJECT('upperBoundKrw', 1000000000, 'ratePpm', 420000),
                JSON_OBJECT('upperBoundKrw', 9223372036854775807, 'ratePpm', 450000)
            )
        )
    ),
    (
        1,
        'deposit',
        'protection',
        '2025-09-01',
        NULL,
        JSON_OBJECT('limitKrw', 100000000)
    ),
    (
        1,
        'isa',
        'eligibilityAndTax',
        '2026-01-01',
        NULL,
        JSON_OBJECT(
            'minimumAge', 19,
            'workingIncomeMinimumAge', 15,
            'comprehensiveTaxLookbackYears', 3,
            'annualContributionLimitKrw', 20000000,
            'totalContributionLimitKrw', 100000000,
            'maximumContributionYears', 5,
            'minimumTermYears', 3,
            'lowIncomeTotalSalaryLimitKrw', 50000000,
            'lowIncomeComprehensiveIncomeLimitKrw', 38000000,
            'generalTaxFreeLimitKrw', 2000000,
            'lowIncomeTaxFreeLimitKrw', 4000000,
            'separateIncomeTaxPpm', 90000,
            'separateLocalIncomeTaxPpm', 9000
        )
    ),
    (
        1,
        'pension',
        'contributionAndWithdrawal',
        '2026-01-01',
        NULL,
        JSON_OBJECT(
            'pensionSavingsCreditLimitKrw', 6000000,
            'combinedCreditLimitKrw', 9000000,
            'salaryHighCreditBoundaryKrw', 55000000,
            'comprehensiveIncomeHighCreditBoundaryKrw', 45000000,
            'highIncomeTaxCreditRatePpm', 150000,
            'highLocalIncomeTaxCreditRatePpm', 15000,
            'standardIncomeTaxCreditRatePpm', 120000,
            'standardLocalIncomeTaxCreditRatePpm', 12000,
            'minimumPensionAge', 55,
            'minimumEnrollmentYears', 5,
            'irpRiskAssetLimitPpm', 700000,
            'underAge70PensionTaxPpm', 55000,
            'underAge80PensionTaxPpm', 44000,
            'age80OrOlderPensionTaxPpm', 33000,
            'lifetimePensionTaxPpm', 33000,
            'nonPensionWithdrawalTaxPpm', 165000,
            'pensionReceiptLimitRatePpm', 1200000,
            'limitedReceiptYears', 10,
            'deferredRetirementFirst10YearsPpm', 700000,
            'deferredRetirementYears11To20Ppm', 600000,
            'deferredRetirementAfter20YearsPpm', 500000
        )
    ),
    (
        1,
        'gold',
        'krxWithdrawal',
        '2026-01-01',
        NULL,
        JSON_OBJECT(
            'vatRatePpm', 100000,
            'withdrawalUnitsGram', JSON_ARRAY(100, 1000)
        )
    );

UPDATE policy_set
SET sealed_at = CURRENT_TIMESTAMP(3)
WHERE id = 1;

CREATE TABLE policy_set_assignment (
    assignment_key        VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    policy_set_id         BIGINT UNSIGNED NOT NULL,
    assignment_revision   BIGINT UNSIGNED NOT NULL DEFAULT 1,
    updated_at            DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (assignment_key),
    KEY ix_policy_set_assignment_policy_set_id (policy_set_id),
    CONSTRAINT fk_policy_set_assignment_policy_set
        FOREIGN KEY (policy_set_id) REFERENCES policy_set (id),
    CONSTRAINT ck_policy_set_assignment_key CHECK (assignment_key = 'newRun')
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_policy_set_assignment_sealed_insert
BEFORE INSERT ON policy_set_assignment
FOR EACH ROW
SET
    NEW.assignment_key = IF(
        EXISTS (
            SELECT 1
            FROM policy_set
            WHERE id = NEW.policy_set_id
              AND sealed_at IS NOT NULL
        ),
        NEW.assignment_key,
        NULL
    ),
    NEW.assignment_revision = 1;

CREATE TRIGGER tr_policy_set_assignment_bump_revision
BEFORE UPDATE ON policy_set_assignment
FOR EACH ROW
SET
    NEW.assignment_key = IF(
        BINARY NEW.assignment_key = BINARY OLD.assignment_key
            AND EXISTS (
                SELECT 1
                FROM policy_set
                WHERE id = NEW.policy_set_id
                  AND sealed_at IS NOT NULL
            ),
        OLD.assignment_key,
        NULL
    ),
    NEW.assignment_revision = OLD.assignment_revision + 1;

CREATE TRIGGER tr_policy_set_assignment_no_delete
BEFORE DELETE ON policy_set_assignment
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'policy set assignment must be updated in place';

INSERT INTO policy_set_assignment (assignment_key, policy_set_id)
VALUES ('newRun', 1);

ALTER TABLE save
    ADD COLUMN policy_set_id BIGINT UNSIGNED NULL AFTER market_world_id;

UPDATE save
SET policy_set_id = 1
WHERE policy_set_id IS NULL;

ALTER TABLE save
    MODIFY COLUMN policy_set_id BIGINT UNSIGNED NOT NULL,
    ADD KEY ix_save_policy_set_id (policy_set_id),
    ADD CONSTRAINT fk_save_policy_set
        FOREIGN KEY (policy_set_id) REFERENCES policy_set (id);

CREATE TABLE financial_account (
    id                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id             BIGINT UNSIGNED NOT NULL,
    run_revision        INT UNSIGNED    NOT NULL,
    account_type        VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    status              VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    cash_krw            BIGINT          NOT NULL DEFAULT 0,
    is_default          BOOLEAN         NOT NULL DEFAULT FALSE,
    default_run_slot    TINYINT GENERATED ALWAYS AS (
        CASE WHEN is_default = TRUE THEN 1 ELSE NULL END
    ) STORED,
    opened_game_day     INT UNSIGNED    NOT NULL,
    created_at          DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at          DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_financial_account_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_financial_account_save_id (save_id, id),
    UNIQUE KEY uk_financial_account_default_run (save_id, run_revision, default_run_slot),
    KEY ix_financial_account_save_run_status (save_id, run_revision, status, id),
    CONSTRAINT fk_financial_account_save
        FOREIGN KEY (save_id) REFERENCES save (id) ON DELETE CASCADE,
    CONSTRAINT ck_financial_account_type CHECK (
        account_type IN (
            'taxableBrokerage',
            'cma',
            'isaGeneral',
            'isaLowIncome',
            'pensionSavings',
            'irp',
            'krxGold'
        )
    ),
    CONSTRAINT ck_financial_account_status CHECK (status IN ('open', 'matured', 'closed')),
    CONSTRAINT ck_financial_account_cash CHECK (cash_krw >= 0),
    CONSTRAINT ck_financial_account_default CHECK (
        is_default IN (FALSE, TRUE)
        AND (is_default = FALSE OR account_type = 'taxableBrokerage')
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_financial_account_state_update_only
BEFORE UPDATE ON financial_account
FOR EACH ROW
SET NEW.account_type = IF(
    NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND BINARY NEW.account_type = BINARY OLD.account_type
        AND NEW.is_default = OLD.is_default
        AND NEW.opened_game_day = OLD.opened_game_day
        AND NEW.created_at = OLD.created_at,
    OLD.account_type,
    NULL
);

CREATE TRIGGER tr_financial_account_no_delete
BEFORE DELETE ON financial_account
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'financial accounts must be closed, not deleted';

-- Preserve one default brokerage account for every historical execution run and current run.
INSERT INTO financial_account
    (save_id, run_revision, account_type, status, cash_krw, is_default, opened_game_day)
SELECT
    run.save_id,
    run.run_revision,
    'taxableBrokerage',
    IF(run.run_revision = save.run_revision, 'open', 'closed'),
    0,
    TRUE,
    0
FROM (
    SELECT id AS save_id, run_revision
    FROM save
    UNION
    SELECT save_id, run_revision
    FROM trade_execution
) AS run
INNER JOIN save ON save.id = run.save_id;

ALTER TABLE asset_position
    ADD COLUMN account_id BIGINT UNSIGNED NULL AFTER save_id;

UPDATE asset_position AS position
INNER JOIN save
    ON save.id = position.save_id
INNER JOIN financial_account AS account
    ON account.save_id = position.save_id
   AND account.run_revision = save.run_revision
   AND account.is_default = TRUE
SET position.account_id = account.id;

ALTER TABLE asset_position
    MODIFY COLUMN account_id BIGINT UNSIGNED NOT NULL,
    DROP PRIMARY KEY,
    ADD PRIMARY KEY (save_id, account_id, symbol),
    ADD CONSTRAINT fk_asset_position_account
        FOREIGN KEY (save_id, account_id)
        REFERENCES financial_account (save_id, id) ON DELETE CASCADE;

ALTER TABLE trade_execution
    ADD COLUMN account_id BIGINT UNSIGNED NULL AFTER save_id;

-- Replace the strict guard with a one-column guard while historical account ids are filled.
CREATE TRIGGER tr_trade_execution_account_backfill_only
BEFORE UPDATE ON trade_execution
FOR EACH ROW
SET NEW.order_id = IF(
    OLD.account_id IS NULL
        AND NEW.account_id IS NOT NULL
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND BINARY NEW.order_id = BINARY OLD.order_id
        AND NEW.expected_run_revision = OLD.expected_run_revision
        AND NEW.expected_state_revision = OLD.expected_state_revision
        AND NEW.expected_game_day = OLD.expected_game_day
        AND NEW.run_revision = OLD.run_revision
        AND NEW.state_revision = OLD.state_revision
        AND NEW.game_day = OLD.game_day
        AND BINARY NEW.side = BINARY OLD.side
        AND BINARY NEW.symbol = BINARY OLD.symbol
        AND NEW.quantity = OLD.quantity
        AND NEW.price_krw = OLD.price_krw
        AND NEW.gross_amount_krw = OLD.gross_amount_krw
        AND NEW.removed_cost_basis_krw = OLD.removed_cost_basis_krw
        AND NEW.created_at = OLD.created_at
        AND EXISTS (
            SELECT 1
            FROM financial_account AS account
            WHERE account.id = NEW.account_id
              AND account.save_id = OLD.save_id
              AND account.run_revision = OLD.run_revision
              AND account.is_default = TRUE
        ),
    OLD.order_id,
    NULL
);

DROP TRIGGER tr_trade_execution_no_update;

UPDATE trade_execution AS execution
INNER JOIN financial_account AS account
    ON account.save_id = execution.save_id
   AND account.run_revision = execution.run_revision
   AND account.is_default = TRUE
SET execution.account_id = account.id;

CREATE TRIGGER tr_trade_execution_no_update
BEFORE UPDATE ON trade_execution
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'trade execution rows are append-only';

DROP TRIGGER tr_trade_execution_account_backfill_only;

ALTER TABLE trade_execution
    MODIFY COLUMN account_id BIGINT UNSIGNED NOT NULL,
    ADD KEY ix_trade_execution_account_run (save_id, run_revision, account_id),
    ADD CONSTRAINT fk_trade_execution_account
        FOREIGN KEY (save_id, run_revision, account_id)
        REFERENCES financial_account (save_id, run_revision, id) ON DELETE CASCADE;

CREATE TABLE ledger_transaction (
    id                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id             BIGINT UNSIGNED NOT NULL,
    run_revision        INT UNSIGNED    NOT NULL,
    game_day            INT UNSIGNED    NOT NULL,
    policy_set_id       BIGINT UNSIGNED NOT NULL,
    source_kind         VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    source_id           VARCHAR(128) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    description         VARCHAR(255)    NOT NULL,
    created_at          DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_ledger_transaction_source
        (save_id, run_revision, source_kind, source_id),
    UNIQUE KEY uk_ledger_transaction_save_run_id (save_id, run_revision, id),
    KEY ix_ledger_transaction_policy_set_id (policy_set_id),
    CONSTRAINT fk_ledger_transaction_save
        FOREIGN KEY (save_id) REFERENCES save (id) ON DELETE CASCADE,
    CONSTRAINT fk_ledger_transaction_policy_set
        FOREIGN KEY (policy_set_id) REFERENCES policy_set (id),
    CONSTRAINT ck_ledger_transaction_source_kind CHECK (CHAR_LENGTH(source_kind) > 0),
    CONSTRAINT ck_ledger_transaction_source_id CHECK (CHAR_LENGTH(source_id) > 0),
    CONSTRAINT ck_ledger_transaction_description CHECK (CHAR_LENGTH(description) > 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE ledger_posting (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    ledger_transaction_id       BIGINT UNSIGNED NOT NULL,
    posting_order               SMALLINT UNSIGNED NOT NULL,
    account_code                VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    financial_account_id        BIGINT UNSIGNED     NULL,
    amount_krw                  BIGINT          NOT NULL,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_ledger_posting_transaction_order
        (ledger_transaction_id, posting_order),
    KEY ix_ledger_posting_transaction_run
        (save_id, run_revision, ledger_transaction_id),
    KEY ix_ledger_posting_account_run
        (save_id, run_revision, financial_account_id),
    CONSTRAINT fk_ledger_posting_transaction
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_ledger_posting_account
        FOREIGN KEY (save_id, run_revision, financial_account_id)
        REFERENCES financial_account (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT ck_ledger_posting_account_code CHECK (
        account_code IN (
            'wallet',
            'accountCash',
            'productPrincipal',
            'debtPrincipal',
            'openingEquity',
            'withholdingTaxLiability',
            'interestIncome',
            'feeExpense',
            'distributionIncome',
            'realizedGainLoss',
            'taxSettlement'
        )
    ),
    CONSTRAINT ck_ledger_posting_account_reference CHECK (
        (
            account_code IN ('accountCash', 'productPrincipal')
            AND financial_account_id IS NOT NULL
        )
        OR
        (
            account_code NOT IN ('accountCash', 'productPrincipal')
            AND financial_account_id IS NULL
        )
    ),
    CONSTRAINT ck_ledger_posting_amount CHECK (amount_krw <> 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_ledger_transaction_no_update
BEFORE UPDATE ON ledger_transaction
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'ledger transactions are append-only';

CREATE TRIGGER tr_ledger_transaction_no_delete
BEFORE DELETE ON ledger_transaction
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'ledger transactions are append-only';

CREATE TRIGGER tr_ledger_posting_no_update
BEFORE UPDATE ON ledger_posting
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'ledger postings are append-only';

CREATE TRIGGER tr_ledger_posting_no_delete
BEFORE DELETE ON ledger_posting
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'ledger postings are append-only';

-- Legacy M1 executions stay nullable; every new finance-aware execution links its ledger entry.
ALTER TABLE trade_execution
    ADD COLUMN ledger_transaction_id BIGINT UNSIGNED NULL AFTER account_id,
    ADD KEY ix_trade_execution_ledger
        (save_id, run_revision, ledger_transaction_id),
    ADD CONSTRAINT fk_trade_execution_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id) ON DELETE CASCADE;

CREATE TABLE scheduled_settlement (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED    NOT NULL,
    due_game_day                        INT UNSIGNED    NOT NULL,
    kind                                VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    payload                             JSON            NOT NULL,
    source_kind                         VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    source_id                           VARCHAR(128) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    occurrence                          INT UNSIGNED    NOT NULL,
    status                              VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    settled_ledger_transaction_id       BIGINT UNSIGNED     NULL,
    cancellation_ledger_transaction_id  BIGINT UNSIGNED     NULL,
    cancellation_reason                 VARCHAR(255)        NULL,
    created_at                          DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                          DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_scheduled_settlement_source
        (save_id, run_revision, source_kind, source_id, occurrence),
    KEY ix_scheduled_settlement_due
        (save_id, run_revision, status, due_game_day, id),
    KEY ix_scheduled_settlement_settled_ledger
        (save_id, run_revision, settled_ledger_transaction_id),
    KEY ix_scheduled_settlement_cancel_ledger
        (save_id, run_revision, cancellation_ledger_transaction_id),
    CONSTRAINT fk_scheduled_settlement_save
        FOREIGN KEY (save_id) REFERENCES save (id) ON DELETE CASCADE,
    CONSTRAINT fk_scheduled_settlement_settled_ledger
        FOREIGN KEY (save_id, run_revision, settled_ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_scheduled_settlement_cancel_ledger
        FOREIGN KEY (save_id, run_revision, cancellation_ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT ck_scheduled_settlement_kind CHECK (CHAR_LENGTH(kind) > 0),
    CONSTRAINT ck_scheduled_settlement_source_kind CHECK (CHAR_LENGTH(source_kind) > 0),
    CONSTRAINT ck_scheduled_settlement_source_id CHECK (CHAR_LENGTH(source_id) > 0),
    CONSTRAINT ck_scheduled_settlement_status CHECK (
        status IN ('pending', 'settled', 'cancelled')
    ),
    CONSTRAINT ck_scheduled_settlement_state_shape CHECK (
        (
            status = 'pending'
            AND settled_ledger_transaction_id IS NULL
            AND cancellation_ledger_transaction_id IS NULL
            AND cancellation_reason IS NULL
        )
        OR
        (
            status = 'settled'
            AND settled_ledger_transaction_id IS NOT NULL
            AND cancellation_ledger_transaction_id IS NULL
            AND cancellation_reason IS NULL
        )
        OR
        (
            status = 'cancelled'
            AND settled_ledger_transaction_id IS NULL
            AND CHAR_LENGTH(cancellation_reason) > 0
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_scheduled_settlement_pending_insert
BEFORE INSERT ON scheduled_settlement
FOR EACH ROW
SET NEW.status = IF(
    NEW.status = 'pending'
        AND NEW.settled_ledger_transaction_id IS NULL
        AND NEW.cancellation_ledger_transaction_id IS NULL
        AND NEW.cancellation_reason IS NULL,
    NEW.status,
    NULL
);

-- A settlement can make exactly one transition; all identity and payload fields stay fixed.
CREATE TRIGGER tr_scheduled_settlement_transition_only
BEFORE UPDATE ON scheduled_settlement
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'pending'
        AND NEW.status IN ('settled', 'cancelled')
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.due_game_day = OLD.due_game_day
        AND BINARY NEW.kind = BINARY OLD.kind
        AND NEW.payload <=> OLD.payload
        AND BINARY NEW.source_kind = BINARY OLD.source_kind
        AND BINARY NEW.source_id = BINARY OLD.source_id
        AND NEW.occurrence = OLD.occurrence
        AND NEW.created_at = OLD.created_at,
    OLD.id,
    NULL
);

CREATE TRIGGER tr_scheduled_settlement_no_delete
BEFORE DELETE ON scheduled_settlement
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'scheduled settlements cannot be deleted';

CREATE TABLE command_receipt (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    command_id                  CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    command_kind                VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    payload_sha256              CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    market_world_id             BIGINT UNSIGNED NOT NULL,
    state_revision              BIGINT UNSIGNED NOT NULL,
    game_day                    INT UNSIGNED    NOT NULL,
    result                      JSON            NOT NULL,
    ledger_transaction_id       BIGINT UNSIGNED     NULL,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_command_receipt_save_command (save_id, command_id),
    KEY ix_command_receipt_ledger
        (save_id, run_revision, ledger_transaction_id),
    CONSTRAINT fk_command_receipt_save
        FOREIGN KEY (save_id) REFERENCES save (id) ON DELETE CASCADE,
    CONSTRAINT fk_command_receipt_market_world
        FOREIGN KEY (market_world_id) REFERENCES market_world (id),
    CONSTRAINT fk_command_receipt_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT ck_command_receipt_command_id CHECK (
        command_id REGEXP '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
    ),
    CONSTRAINT ck_command_receipt_kind CHECK (CHAR_LENGTH(command_kind) > 0),
    CONSTRAINT ck_command_receipt_payload_hash CHECK (
        payload_sha256 REGEXP '^[0-9a-f]{64}$'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_command_receipt_no_update
BEFORE UPDATE ON command_receipt
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'command receipts are append-only';

CREATE TRIGGER tr_command_receipt_no_delete
BEFORE DELETE ON command_receipt
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'command receipts are append-only';

-- M1 balances are represented once at the current run boundary; earlier executions stay historical.
INSERT INTO ledger_transaction
    (save_id, run_revision, game_day, policy_set_id, source_kind, source_id, description)
SELECT
    save.id,
    save.run_revision,
    save.game_day,
    save.policy_set_id,
    'm2OpeningBalance',
    'migration0009',
    'M2 기초 잔액 이관'
FROM save
WHERE save.cash_krw <> 0
   OR save.debt_krw <> 0
   OR EXISTS (
       SELECT 1
       FROM asset_position AS position
       WHERE position.save_id = save.id
   );

INSERT INTO ledger_posting
    (
        save_id,
        run_revision,
        ledger_transaction_id,
        posting_order,
        account_code,
        financial_account_id,
        amount_krw
    )
SELECT
    save.id,
    save.run_revision,
    transaction.id,
    1,
    'wallet',
    NULL,
    save.cash_krw
FROM save
INNER JOIN ledger_transaction AS transaction
    ON transaction.save_id = save.id
   AND transaction.run_revision = save.run_revision
   AND transaction.source_kind = 'm2OpeningBalance'
   AND transaction.source_id = 'migration0009'
WHERE save.cash_krw <> 0
UNION ALL
SELECT
    save.id,
    save.run_revision,
    transaction.id,
    2,
    'productPrincipal',
    account.id,
    position.total_cost_basis_krw
FROM save
INNER JOIN financial_account AS account
    ON account.save_id = save.id
   AND account.run_revision = save.run_revision
   AND account.is_default = TRUE
INNER JOIN asset_position AS position
    ON position.save_id = save.id
   AND position.account_id = account.id
INNER JOIN ledger_transaction AS transaction
    ON transaction.save_id = save.id
   AND transaction.run_revision = save.run_revision
   AND transaction.source_kind = 'm2OpeningBalance'
   AND transaction.source_id = 'migration0009'
WHERE position.total_cost_basis_krw <> 0
UNION ALL
SELECT
    save.id,
    save.run_revision,
    transaction.id,
    3,
    'debtPrincipal',
    NULL,
    -save.debt_krw
FROM save
INNER JOIN ledger_transaction AS transaction
    ON transaction.save_id = save.id
   AND transaction.run_revision = save.run_revision
   AND transaction.source_kind = 'm2OpeningBalance'
   AND transaction.source_id = 'migration0009'
WHERE save.debt_krw <> 0
UNION ALL
SELECT
    save.id,
    save.run_revision,
    transaction.id,
    4,
    'openingEquity',
    NULL,
    -(save.cash_krw + COALESCE(position.total_cost_basis_krw, 0) - save.debt_krw)
FROM save
INNER JOIN financial_account AS account
    ON account.save_id = save.id
   AND account.run_revision = save.run_revision
   AND account.is_default = TRUE
LEFT JOIN asset_position AS position
    ON position.save_id = save.id
   AND position.account_id = account.id
INNER JOIN ledger_transaction AS transaction
    ON transaction.save_id = save.id
   AND transaction.run_revision = save.run_revision
   AND transaction.source_kind = 'm2OpeningBalance'
   AND transaction.source_id = 'migration0009'
WHERE save.cash_krw + COALESCE(position.total_cost_basis_krw, 0) - save.debt_krw <> 0;
