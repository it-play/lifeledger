-- Global command identity, resumable manual advances, and explicit settlement outcomes.

CREATE TABLE command_identity (
    save_id                     BIGINT UNSIGNED NOT NULL,
    command_id                  CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    command_kind                VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    payload_sha256              CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    initial_run_revision        INT UNSIGNED    NOT NULL,
    initial_state_revision      BIGINT UNSIGNED NOT NULL,
    initial_game_day            INT UNSIGNED    NOT NULL,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, command_id),
    CONSTRAINT fk_command_identity_save
        FOREIGN KEY (save_id) REFERENCES save (id) ON DELETE CASCADE,
    CONSTRAINT ck_command_identity_command_id CHECK (
        command_id REGEXP '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
    ),
    CONSTRAINT ck_command_identity_kind CHECK (CHAR_LENGTH(command_kind) > 0),
    CONSTRAINT ck_command_identity_payload_hash CHECK (
        payload_sha256 REGEXP '^[0-9a-f]{64}$'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

-- Receipts created by M2-A represent one-state-revision commands. Their original
-- cursor can therefore be reconstructed without changing the append-only receipt.
INSERT INTO command_identity
    (
        save_id,
        command_id,
        command_kind,
        payload_sha256,
        initial_run_revision,
        initial_state_revision,
        initial_game_day,
        created_at
    )
SELECT
    save_id,
    command_id,
    command_kind,
    payload_sha256,
    run_revision,
    CASE WHEN state_revision > 0 THEN state_revision - 1 ELSE 0 END,
    game_day,
    created_at
FROM command_receipt;

-- M1 executions predate command receipts. Preserve their exact request fingerprint so
-- their order UUIDs also reserve the save-wide command namespace.
INSERT INTO command_identity
    (
        save_id,
        command_id,
        command_kind,
        payload_sha256,
        initial_run_revision,
        initial_state_revision,
        initial_game_day,
        created_at
    )
SELECT
    execution.save_id,
    execution.order_id,
    'trade',
    SHA2(
        CONCAT(
            'lifeledger.portfolio.order.v1', CHAR(10),
            'expectedRunRevision=', execution.expected_run_revision, CHAR(10),
            'expectedStateRevision=', execution.expected_state_revision, CHAR(10),
            'expectedGameDay=', execution.expected_game_day, CHAR(10),
            'accountId=', execution.account_id, CHAR(10),
            'side=', execution.side, CHAR(10),
            'symbol=', execution.symbol, CHAR(10),
            'quantity=', execution.quantity
        ),
        256
    ),
    execution.expected_run_revision,
    execution.expected_state_revision,
    execution.expected_game_day,
    execution.created_at
FROM trade_execution AS execution
ON DUPLICATE KEY UPDATE
    -- Reconstructed duplicates are harmless only when every immutable identity field
    -- agrees. Assigning NULL to a NOT NULL column makes a legacy cross-kind/hash
    -- collision fail the migration instead of silently choosing one command.
    command_kind = IF(
        BINARY command_identity.command_kind = BINARY VALUES(command_kind)
            AND BINARY command_identity.payload_sha256 = BINARY VALUES(payload_sha256)
            AND command_identity.initial_run_revision = VALUES(initial_run_revision)
            AND command_identity.initial_state_revision = VALUES(initial_state_revision)
            AND command_identity.initial_game_day = VALUES(initial_game_day),
        command_identity.command_kind,
        NULL
    );

ALTER TABLE command_receipt
    ADD CONSTRAINT fk_command_receipt_identity
        FOREIGN KEY (save_id, command_id)
        REFERENCES command_identity (save_id, command_id) ON DELETE CASCADE;

CREATE TRIGGER tr_command_identity_no_update
BEFORE UPDATE ON command_identity
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'command identities are append-only';

CREATE TRIGGER tr_command_identity_no_delete
BEFORE DELETE ON command_identity
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'command identities are append-only';

CREATE TABLE advance_command_step (
    save_id                 BIGINT UNSIGNED NOT NULL,
    command_id              CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    step_no                 INT UNSIGNED    NOT NULL,
    before_run_revision     INT UNSIGNED    NOT NULL,
    before_state_revision   BIGINT UNSIGNED NOT NULL,
    before_game_day         INT UNSIGNED    NOT NULL,
    after_run_revision      INT UNSIGNED    NOT NULL,
    after_state_revision    BIGINT UNSIGNED NOT NULL,
    after_game_day          INT UNSIGNED    NOT NULL,
    created_at              DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, command_id, step_no),
    CONSTRAINT fk_advance_command_step_identity
        FOREIGN KEY (save_id, command_id)
        REFERENCES command_identity (save_id, command_id) ON DELETE CASCADE,
    CONSTRAINT ck_advance_command_step_number CHECK (step_no BETWEEN 1 AND 30),
    CONSTRAINT ck_advance_command_step_run CHECK (
        after_run_revision = before_run_revision
    ),
    CONSTRAINT ck_advance_command_step_state CHECK (
        after_state_revision = before_state_revision + 1
    ),
    CONSTRAINT ck_advance_command_step_day CHECK (
        after_game_day = before_game_day + 1
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_advance_command_step_no_update
BEFORE UPDATE ON advance_command_step
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'advance command steps are append-only';

CREATE TRIGGER tr_advance_command_step_no_delete
BEFORE DELETE ON advance_command_step
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'advance command steps are append-only';

-- A settled obligation distinguishes a real ledger movement from a valid zero-movement
-- result. This prevents fake one-won postings while keeping every due item auditable.
DROP TRIGGER tr_scheduled_settlement_transition_only;

ALTER TABLE scheduled_settlement
    DROP CHECK ck_scheduled_settlement_state_shape,
    ADD COLUMN outcome VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL AFTER status,
    ADD COLUMN outcome_reason VARCHAR(255) NULL AFTER outcome;

UPDATE scheduled_settlement
SET outcome = 'applied'
WHERE status = 'settled';

ALTER TABLE scheduled_settlement
    ADD CONSTRAINT ck_scheduled_settlement_outcome CHECK (
        outcome IS NULL OR outcome IN ('applied', 'noMovement')
    ),
    ADD CONSTRAINT ck_scheduled_settlement_state_shape CHECK (
        (
            status = 'pending'
            AND outcome IS NULL
            AND outcome_reason IS NULL
            AND settled_ledger_transaction_id IS NULL
            AND cancellation_ledger_transaction_id IS NULL
            AND cancellation_reason IS NULL
        )
        OR
        (
            status = 'settled'
            AND outcome = 'applied'
            AND outcome_reason IS NULL
            AND settled_ledger_transaction_id IS NOT NULL
            AND cancellation_ledger_transaction_id IS NULL
            AND cancellation_reason IS NULL
        )
        OR
        (
            status = 'settled'
            AND outcome = 'noMovement'
            AND CHAR_LENGTH(outcome_reason) > 0
            AND settled_ledger_transaction_id IS NULL
            AND cancellation_ledger_transaction_id IS NULL
            AND cancellation_reason IS NULL
        )
        OR
        (
            status = 'cancelled'
            AND outcome IS NULL
            AND outcome_reason IS NULL
            AND settled_ledger_transaction_id IS NULL
            AND CHAR_LENGTH(cancellation_reason) > 0
        )
    );

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
