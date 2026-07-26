-- M2-D run-scoped asset state, immutable executions, and pension value audit (§8.2–§8.5).

-- Historical M1 rows did not persist charges or realized P&L. They keep NULL realized P&L;
-- every execution inserted after this migration must provide all three new values.
ALTER TABLE trade_execution
    ADD COLUMN fee_krw BIGINT NOT NULL DEFAULT 0 AFTER gross_amount_krw,
    ADD COLUMN tax_krw BIGINT NOT NULL DEFAULT 0 AFTER fee_krw,
    ADD COLUMN realized_gain_loss_krw BIGINT NULL AFTER removed_cost_basis_krw,
    ADD CONSTRAINT ck_trade_execution_charges CHECK (fee_krw >= 0 AND tax_krw >= 0),
    ADD CONSTRAINT ck_trade_execution_realized CHECK (
        realized_gain_loss_krw IS NULL
        OR (
            side = 'buy'
            AND removed_cost_basis_krw = 0
            AND realized_gain_loss_krw = 0
        )
        OR (
            side = 'sell'
            AND CAST(realized_gain_loss_krw AS DECIMAL(65, 0))
                = CAST(gross_amount_krw AS DECIMAL(65, 0))
                - CAST(removed_cost_basis_krw AS DECIMAL(65, 0))
                - CAST(fee_krw AS DECIMAL(65, 0))
                - CAST(tax_krw AS DECIMAL(65, 0))
        )
    );

CREATE TRIGGER tr_trade_execution_complete_insert
BEFORE INSERT ON trade_execution
FOR EACH ROW
SET NEW.symbol = IF(NEW.realized_gain_loss_krw IS NOT NULL, NEW.symbol, NULL);

CREATE TABLE bond_series (
    id                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    market_world_id     BIGINT UNSIGNED NOT NULL,
    product_version_id  BIGINT UNSIGNED NOT NULL,
    issued_date         DATE            NOT NULL,
    maturity_date       DATE            NOT NULL,
    coupon_rate_bp      INT             NOT NULL,
    issue_yield_bp      INT             NOT NULL,
    created_at          DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_bond_series_world_product_issue
        (market_world_id, product_version_id, issued_date),
    UNIQUE KEY uk_bond_series_world_id (market_world_id, id),
    KEY ix_bond_series_catalog
        (market_world_id, maturity_date, product_version_id, id),
    CONSTRAINT fk_bond_series_world
        FOREIGN KEY (market_world_id) REFERENCES market_world (id),
    CONSTRAINT fk_bond_series_product
        FOREIGN KEY (product_version_id) REFERENCES bond_product_version (id),
    CONSTRAINT ck_bond_series_dates CHECK (maturity_date > issued_date),
    CONSTRAINT ck_bond_series_rates CHECK (coupon_rate_bp >= 0 AND issue_yield_bp >= 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_bond_series_valid_insert
BEFORE INSERT ON bond_series
FOR EACH ROW
SET NEW.product_version_id = IF(
    EXISTS (
        SELECT 1
        FROM market_world_product_bundle AS bundle
        INNER JOIN bond_product_version AS product
            ON product.id = NEW.product_version_id
        WHERE bundle.market_world_id = NEW.market_world_id
          AND bundle.published_at IS NOT NULL
          AND product.published_at IS NOT NULL
          AND (
              (bundle.bond_3y_product_version_id = product.id AND product.term_years = 3)
              OR
              (bundle.bond_10y_product_version_id = product.id AND product.term_years = 10)
          )
    ),
    NEW.product_version_id,
    NULL
);

CREATE TRIGGER tr_bond_series_no_update
BEFORE UPDATE ON bond_series
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'bond series rows are immutable';

CREATE TRIGGER tr_bond_series_no_delete
BEFORE DELETE ON bond_series
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'bond series rows are immutable';

CREATE TABLE llx_distribution_entitlement (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    financial_account_id        BIGINT UNSIGNED NOT NULL,
    product_version_id          BIGINT UNSIGNED NOT NULL,
    record_game_day             INT UNSIGNED    NOT NULL,
    record_date                 DATE            NOT NULL,
    payment_game_day            INT UNSIGNED    NOT NULL,
    payment_date                DATE            NOT NULL,
    record_quantity             INT UNSIGNED    NOT NULL,
    record_close_krw            BIGINT          NOT NULL,
    per_share_distribution_krw  BIGINT          NOT NULL,
    gross_distribution_krw      BIGINT          NOT NULL,
    status                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    outcome                     VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL,
    paid_game_day               INT UNSIGNED        NULL,
    ledger_transaction_id       BIGINT UNSIGNED     NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_llx_entitlement_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_llx_entitlement_record
        (save_id, run_revision, financial_account_id, product_version_id, record_game_day),
    KEY ix_llx_entitlement_pending
        (save_id, run_revision, status, payment_game_day, id),
    KEY ix_llx_entitlement_ledger
        (save_id, run_revision, ledger_transaction_id),
    CONSTRAINT fk_llx_entitlement_account
        FOREIGN KEY (save_id, run_revision, financial_account_id)
        REFERENCES financial_account (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_llx_entitlement_product
        FOREIGN KEY (product_version_id) REFERENCES index_product_version (id),
    CONSTRAINT fk_llx_entitlement_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT ck_llx_entitlement_dates CHECK (
        payment_game_day > record_game_day AND payment_date > record_date
    ),
    CONSTRAINT ck_llx_entitlement_amounts CHECK (
        record_quantity > 0
        AND record_close_krw > 0
        AND per_share_distribution_krw >= 0
        AND gross_distribution_krw >= 0
        AND CAST(gross_distribution_krw AS DECIMAL(65, 0))
            = CAST(per_share_distribution_krw AS DECIMAL(65, 0)) * record_quantity
    ),
    CONSTRAINT ck_llx_entitlement_status CHECK (status IN ('pending', 'paid')),
    CONSTRAINT ck_llx_entitlement_outcome CHECK (
        outcome IS NULL OR outcome IN ('applied', 'noMovement')
    ),
    CONSTRAINT ck_llx_entitlement_state CHECK (
        (
            status = 'pending'
            AND outcome IS NULL
            AND paid_game_day IS NULL
            AND ledger_transaction_id IS NULL
        )
        OR
        (
            status = 'paid'
            AND outcome = 'applied'
            AND paid_game_day >= payment_game_day
            AND gross_distribution_krw > 0
            AND ledger_transaction_id IS NOT NULL
        )
        OR
        (
            status = 'paid'
            AND outcome = 'noMovement'
            AND paid_game_day >= payment_game_day
            AND gross_distribution_krw = 0
            AND ledger_transaction_id IS NULL
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_llx_entitlement_valid_insert
BEFORE INSERT ON llx_distribution_entitlement
FOR EACH ROW
SET NEW.financial_account_id = IF(
    NEW.status = 'pending'
    AND NEW.outcome IS NULL
    AND NEW.paid_game_day IS NULL
    AND NEW.ledger_transaction_id IS NULL
    AND EXISTS (
        SELECT 1
        FROM save
        INNER JOIN market_world_product_bundle AS bundle
            ON bundle.id = save.market_world_product_bundle_id
           AND bundle.market_world_id = save.market_world_id
        INNER JOIN financial_account AS account
            ON account.save_id = save.id
           AND account.run_revision = save.run_revision
           AND account.id = NEW.financial_account_id
        WHERE save.id = NEW.save_id
          AND save.run_revision = NEW.run_revision
          AND bundle.index_product_version_id = NEW.product_version_id
          AND account.status = 'open'
          AND account.account_type IN (
              'taxableBrokerage', 'isaGeneral', 'isaLowIncome', 'pensionSavings', 'irp'
          )
    ),
    NEW.financial_account_id,
    NULL
);

CREATE TRIGGER tr_llx_entitlement_transition_only
BEFORE UPDATE ON llx_distribution_entitlement
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'pending'
    AND NEW.status = 'paid'
    AND NEW.id = OLD.id
    AND NEW.save_id = OLD.save_id
    AND NEW.run_revision = OLD.run_revision
    AND NEW.financial_account_id = OLD.financial_account_id
    AND NEW.product_version_id = OLD.product_version_id
    AND NEW.record_game_day = OLD.record_game_day
    AND NEW.record_date = OLD.record_date
    AND NEW.payment_game_day = OLD.payment_game_day
    AND NEW.payment_date = OLD.payment_date
    AND NEW.record_quantity = OLD.record_quantity
    AND NEW.record_close_krw = OLD.record_close_krw
    AND NEW.per_share_distribution_krw = OLD.per_share_distribution_krw
    AND NEW.gross_distribution_krw = OLD.gross_distribution_krw
    AND NEW.created_at = OLD.created_at,
    OLD.id,
    NULL
);

CREATE TRIGGER tr_llx_entitlement_no_delete
BEFORE DELETE ON llx_distribution_entitlement
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'LLX entitlements cannot be deleted';

CREATE TABLE bond_position (
    id                      BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                 BIGINT UNSIGNED NOT NULL,
    run_revision            INT UNSIGNED    NOT NULL,
    financial_account_id    BIGINT UNSIGNED NOT NULL,
    market_world_id         BIGINT UNSIGNED NOT NULL,
    series_id               BIGINT UNSIGNED NOT NULL,
    product_version_id      BIGINT UNSIGNED NOT NULL,
    bond_units              INT UNSIGNED    NOT NULL,
    total_cost_basis_krw    BIGINT          NOT NULL,
    created_at              DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at              DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_bond_position_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_bond_position_account_series
        (save_id, run_revision, financial_account_id, series_id),
    KEY ix_bond_position_snapshot
        (save_id, run_revision, financial_account_id, bond_units, id),
    CONSTRAINT fk_bond_position_account
        FOREIGN KEY (save_id, run_revision, financial_account_id)
        REFERENCES financial_account (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_bond_position_series
        FOREIGN KEY (market_world_id, series_id)
        REFERENCES bond_series (market_world_id, id),
    CONSTRAINT fk_bond_position_product
        FOREIGN KEY (product_version_id) REFERENCES bond_product_version (id),
    CONSTRAINT ck_bond_position_shape CHECK (
        (bond_units = 0 AND total_cost_basis_krw = 0)
        OR (bond_units > 0 AND total_cost_basis_krw > 0)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_bond_position_valid_insert
BEFORE INSERT ON bond_position
FOR EACH ROW
SET NEW.series_id = IF(
    EXISTS (
        SELECT 1
        FROM save
        INNER JOIN market_world_product_bundle AS bundle
            ON bundle.id = save.market_world_product_bundle_id
           AND bundle.market_world_id = save.market_world_id
        INNER JOIN financial_account AS account
            ON account.save_id = save.id
           AND account.run_revision = save.run_revision
           AND account.id = NEW.financial_account_id
        INNER JOIN bond_series AS series
            ON series.id = NEW.series_id
           AND series.market_world_id = save.market_world_id
        INNER JOIN bond_product_version AS product
            ON product.id = series.product_version_id
        WHERE save.id = NEW.save_id
          AND save.run_revision = NEW.run_revision
          AND NEW.market_world_id = save.market_world_id
          AND NEW.product_version_id = product.id
          AND account.status = 'open'
          AND account.account_type IN (
              'taxableBrokerage', 'isaGeneral', 'isaLowIncome', 'pensionSavings', 'irp'
          )
          AND product.id IN (
              bundle.bond_3y_product_version_id, bundle.bond_10y_product_version_id
          )
          AND NEW.bond_units <= product.max_position_units
    ),
    NEW.series_id,
    NULL
);

CREATE TRIGGER tr_bond_position_state_update_only
BEFORE UPDATE ON bond_position
FOR EACH ROW
SET NEW.id = IF(
    NEW.id = OLD.id
    AND NEW.save_id = OLD.save_id
    AND NEW.run_revision = OLD.run_revision
    AND NEW.financial_account_id = OLD.financial_account_id
    AND NEW.market_world_id = OLD.market_world_id
    AND NEW.series_id = OLD.series_id
    AND NEW.product_version_id = OLD.product_version_id
    AND NEW.created_at = OLD.created_at
    AND EXISTS (
        SELECT 1
        FROM save
        INNER JOIN financial_account AS account
            ON account.save_id = save.id
           AND account.run_revision = save.run_revision
           AND account.id = NEW.financial_account_id
        INNER JOIN bond_series AS series
            ON series.id = NEW.series_id
           AND series.market_world_id = save.market_world_id
        INNER JOIN bond_product_version AS product
            ON product.id = series.product_version_id
        WHERE save.id = NEW.save_id
          AND save.run_revision = NEW.run_revision
          AND save.market_world_id = NEW.market_world_id
          AND account.status = 'open'
          AND product.id = NEW.product_version_id
          AND NEW.bond_units <= product.max_position_units
    ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_bond_position_no_delete
BEFORE DELETE ON bond_position
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'bond positions must be retained at zero';

CREATE TABLE bond_execution (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    state_revision              BIGINT UNSIGNED NOT NULL,
    game_day                    INT UNSIGNED    NOT NULL,
    command_id                  CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    financial_account_id        BIGINT UNSIGNED NOT NULL,
    market_world_id             BIGINT UNSIGNED NOT NULL,
    series_id                   BIGINT UNSIGNED NOT NULL,
    product_version_id          BIGINT UNSIGNED NOT NULL,
    side                        VARCHAR(4) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    bond_units                  INT UNSIGNED    NOT NULL,
    dirty_price_krw             BIGINT          NOT NULL,
    gross_amount_krw            BIGINT          NOT NULL,
    fee_krw                     BIGINT          NOT NULL,
    tax_krw                     BIGINT          NOT NULL,
    removed_cost_basis_krw      BIGINT          NOT NULL,
    realized_gain_loss_krw      BIGINT          NOT NULL,
    ledger_transaction_id       BIGINT UNSIGNED NOT NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_bond_execution_save_command (save_id, command_id),
    UNIQUE KEY uk_bond_execution_save_run_id (save_id, run_revision, id),
    KEY ix_bond_execution_account_series
        (save_id, run_revision, financial_account_id, series_id, id),
    KEY ix_bond_execution_ledger
        (save_id, run_revision, ledger_transaction_id),
    CONSTRAINT fk_bond_execution_identity
        FOREIGN KEY (save_id, command_id)
        REFERENCES command_identity (save_id, command_id) ON DELETE CASCADE,
    CONSTRAINT fk_bond_execution_account
        FOREIGN KEY (save_id, run_revision, financial_account_id)
        REFERENCES financial_account (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_bond_execution_series
        FOREIGN KEY (market_world_id, series_id)
        REFERENCES bond_series (market_world_id, id),
    CONSTRAINT fk_bond_execution_product
        FOREIGN KEY (product_version_id) REFERENCES bond_product_version (id),
    CONSTRAINT fk_bond_execution_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT ck_bond_execution_side CHECK (side IN ('buy', 'sell')),
    CONSTRAINT ck_bond_execution_amounts CHECK (
        bond_units > 0
        AND dirty_price_krw > 0
        AND gross_amount_krw > 0
        AND fee_krw >= 0
        AND tax_krw >= 0
        AND removed_cost_basis_krw >= 0
    ),
    CONSTRAINT ck_bond_execution_result CHECK (
        (
            side = 'buy'
            AND removed_cost_basis_krw = 0
            AND realized_gain_loss_krw = 0
        )
        OR (
            side = 'sell'
            AND CAST(realized_gain_loss_krw AS DECIMAL(65, 0))
                = CAST(gross_amount_krw AS DECIMAL(65, 0))
                - CAST(fee_krw AS DECIMAL(65, 0))
                - CAST(tax_krw AS DECIMAL(65, 0))
                - CAST(removed_cost_basis_krw AS DECIMAL(65, 0))
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_bond_execution_valid_insert
BEFORE INSERT ON bond_execution
FOR EACH ROW
SET NEW.series_id = IF(
    EXISTS (
        SELECT 1
        FROM command_identity AS identity
        INNER JOIN save ON save.id = identity.save_id
        INNER JOIN financial_account AS account
            ON account.save_id = save.id
           AND account.run_revision = save.run_revision
           AND account.id = NEW.financial_account_id
        INNER JOIN bond_series AS series
            ON series.id = NEW.series_id
           AND series.market_world_id = save.market_world_id
        INNER JOIN market_world_product_bundle AS bundle
            ON bundle.id = save.market_world_product_bundle_id
           AND bundle.market_world_id = save.market_world_id
        WHERE identity.save_id = NEW.save_id
          AND BINARY identity.command_id = BINARY NEW.command_id
          AND identity.initial_run_revision = NEW.run_revision
          AND identity.initial_state_revision + 1 = NEW.state_revision
          AND identity.initial_game_day = NEW.game_day
          AND save.run_revision = NEW.run_revision
          AND save.market_world_id = NEW.market_world_id
          AND account.status = 'open'
          AND account.account_type IN (
              'taxableBrokerage', 'isaGeneral', 'isaLowIncome', 'pensionSavings', 'irp'
          )
          AND series.product_version_id = NEW.product_version_id
          AND NEW.product_version_id IN (
              bundle.bond_3y_product_version_id, bundle.bond_10y_product_version_id
          )
    ),
    NEW.series_id,
    NULL
);

CREATE TRIGGER tr_bond_execution_no_update
BEFORE UPDATE ON bond_execution
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'bond executions are append-only';

CREATE TRIGGER tr_bond_execution_no_delete
BEFORE DELETE ON bond_execution
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'bond executions are append-only';

CREATE TABLE bond_lot (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    financial_account_id        BIGINT UNSIGNED NOT NULL,
    market_world_id             BIGINT UNSIGNED NOT NULL,
    series_id                   BIGINT UNSIGNED NOT NULL,
    acquired_execution_id       BIGINT UNSIGNED NOT NULL,
    acquired_game_day           INT UNSIGNED    NOT NULL,
    original_units              INT UNSIGNED    NOT NULL,
    remaining_units             INT UNSIGNED    NOT NULL,
    original_cost_basis_krw     BIGINT          NOT NULL,
    remaining_cost_basis_krw    BIGINT          NOT NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_bond_lot_buy_execution (save_id, run_revision, acquired_execution_id),
    KEY ix_bond_lot_fifo
        (save_id, run_revision, financial_account_id, series_id, remaining_units, id),
    CONSTRAINT fk_bond_lot_account
        FOREIGN KEY (save_id, run_revision, financial_account_id)
        REFERENCES financial_account (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_bond_lot_series
        FOREIGN KEY (market_world_id, series_id)
        REFERENCES bond_series (market_world_id, id),
    CONSTRAINT fk_bond_lot_execution
        FOREIGN KEY (save_id, run_revision, acquired_execution_id)
        REFERENCES bond_execution (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT ck_bond_lot_original CHECK (
        original_units > 0 AND original_cost_basis_krw > 0
    ),
    CONSTRAINT ck_bond_lot_remaining CHECK (
        remaining_units <= original_units
        AND remaining_cost_basis_krw <= original_cost_basis_krw
        AND (
            (remaining_units = 0 AND remaining_cost_basis_krw = 0)
            OR (remaining_units > 0 AND remaining_cost_basis_krw > 0)
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_bond_lot_valid_insert
BEFORE INSERT ON bond_lot
FOR EACH ROW
SET NEW.acquired_execution_id = IF(
    NEW.remaining_units = NEW.original_units
    AND NEW.remaining_cost_basis_krw = NEW.original_cost_basis_krw
    AND EXISTS (
        SELECT 1
        FROM bond_execution AS execution
        WHERE execution.id = NEW.acquired_execution_id
          AND execution.save_id = NEW.save_id
          AND execution.run_revision = NEW.run_revision
          AND execution.financial_account_id = NEW.financial_account_id
          AND execution.market_world_id = NEW.market_world_id
          AND execution.series_id = NEW.series_id
          AND execution.side = 'buy'
          AND execution.game_day = NEW.acquired_game_day
          AND execution.bond_units = NEW.original_units
          AND CAST(NEW.original_cost_basis_krw AS DECIMAL(65, 0))
              = CAST(execution.gross_amount_krw AS DECIMAL(65, 0))
              + CAST(execution.fee_krw AS DECIMAL(65, 0))
              + CAST(execution.tax_krw AS DECIMAL(65, 0))
    ),
    NEW.acquired_execution_id,
    NULL
);

CREATE TRIGGER tr_bond_lot_reduce_only
BEFORE UPDATE ON bond_lot
FOR EACH ROW
SET NEW.id = IF(
    NEW.id = OLD.id
    AND NEW.save_id = OLD.save_id
    AND NEW.run_revision = OLD.run_revision
    AND NEW.financial_account_id = OLD.financial_account_id
    AND NEW.market_world_id = OLD.market_world_id
    AND NEW.series_id = OLD.series_id
    AND NEW.acquired_execution_id = OLD.acquired_execution_id
    AND NEW.acquired_game_day = OLD.acquired_game_day
    AND NEW.original_units = OLD.original_units
    AND NEW.original_cost_basis_krw = OLD.original_cost_basis_krw
    AND NEW.remaining_units <= OLD.remaining_units
    AND NEW.remaining_cost_basis_krw <= OLD.remaining_cost_basis_krw
    AND NEW.created_at = OLD.created_at,
    OLD.id,
    NULL
);

CREATE TRIGGER tr_bond_lot_no_delete
BEFORE DELETE ON bond_lot
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'bond lots must be retained at zero';

CREATE TABLE gold_account_contract (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    financial_account_id        BIGINT UNSIGNED NOT NULL,
    product_version_id          BIGINT UNSIGNED NOT NULL,
    status                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    active_run_slot             TINYINT GENERATED ALWAYS AS (
        CASE WHEN status = 'active' THEN 1 ELSE NULL END
    ) STORED,
    opened_game_day             INT UNSIGNED    NOT NULL,
    closed_game_day             INT UNSIGNED        NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_gold_contract_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_gold_contract_account
        (save_id, run_revision, financial_account_id),
    UNIQUE KEY uk_gold_contract_active
        (save_id, run_revision, active_run_slot),
    CONSTRAINT fk_gold_contract_account
        FOREIGN KEY (save_id, run_revision, financial_account_id)
        REFERENCES financial_account (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_gold_contract_product
        FOREIGN KEY (product_version_id) REFERENCES gold_product_version (id),
    CONSTRAINT ck_gold_contract_status CHECK (status IN ('active', 'closed')),
    CONSTRAINT ck_gold_contract_state CHECK (
        (status = 'active' AND closed_game_day IS NULL)
        OR (status = 'closed' AND closed_game_day >= opened_game_day)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_gold_contract_valid_insert
BEFORE INSERT ON gold_account_contract
FOR EACH ROW
SET NEW.financial_account_id = IF(
    NEW.status = 'active'
    AND NEW.closed_game_day IS NULL
    AND EXISTS (
        SELECT 1
        FROM save
        INNER JOIN market_world_product_bundle AS bundle
            ON bundle.id = save.market_world_product_bundle_id
           AND bundle.market_world_id = save.market_world_id
        INNER JOIN financial_account AS account
            ON account.save_id = save.id
           AND account.run_revision = save.run_revision
           AND account.id = NEW.financial_account_id
        WHERE save.id = NEW.save_id
          AND save.run_revision = NEW.run_revision
          AND account.account_type = 'krxGold'
          AND account.status = 'open'
          AND bundle.gold_product_version_id = NEW.product_version_id
    ),
    NEW.financial_account_id,
    NULL
);

CREATE TRIGGER tr_gold_contract_close_only
BEFORE UPDATE ON gold_account_contract
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'active'
    AND NEW.status = 'closed'
    AND NEW.id = OLD.id
    AND NEW.save_id = OLD.save_id
    AND NEW.run_revision = OLD.run_revision
    AND NEW.financial_account_id = OLD.financial_account_id
    AND NEW.product_version_id = OLD.product_version_id
    AND NEW.opened_game_day = OLD.opened_game_day
    AND NEW.created_at = OLD.created_at,
    OLD.id,
    NULL
);

CREATE TRIGGER tr_gold_contract_no_delete
BEFORE DELETE ON gold_account_contract
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'gold account contracts cannot be deleted';

CREATE TABLE gold_position (
    save_id                 BIGINT UNSIGNED NOT NULL,
    run_revision            INT UNSIGNED    NOT NULL,
    financial_account_id    BIGINT UNSIGNED NOT NULL,
    product_version_id      BIGINT UNSIGNED NOT NULL,
    quantity_gram           INT UNSIGNED    NOT NULL,
    total_cost_basis_krw    BIGINT          NOT NULL,
    created_at              DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at              DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, run_revision, financial_account_id),
    CONSTRAINT fk_gold_position_contract
        FOREIGN KEY (save_id, run_revision, financial_account_id)
        REFERENCES gold_account_contract (save_id, run_revision, financial_account_id)
        ON DELETE CASCADE,
    CONSTRAINT fk_gold_position_product
        FOREIGN KEY (product_version_id) REFERENCES gold_product_version (id),
    CONSTRAINT ck_gold_position_shape CHECK (
        (quantity_gram = 0 AND total_cost_basis_krw = 0)
        OR (quantity_gram > 0 AND total_cost_basis_krw > 0)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_gold_position_valid_insert
BEFORE INSERT ON gold_position
FOR EACH ROW
SET NEW.product_version_id = IF(
    EXISTS (
        SELECT 1
        FROM gold_account_contract AS contract
        WHERE contract.save_id = NEW.save_id
          AND contract.run_revision = NEW.run_revision
          AND contract.financial_account_id = NEW.financial_account_id
          AND contract.product_version_id = NEW.product_version_id
          AND contract.status = 'active'
    ),
    NEW.product_version_id,
    NULL
);

CREATE TRIGGER tr_gold_position_state_update_only
BEFORE UPDATE ON gold_position
FOR EACH ROW
SET NEW.financial_account_id = IF(
    NEW.save_id = OLD.save_id
    AND NEW.run_revision = OLD.run_revision
    AND NEW.financial_account_id = OLD.financial_account_id
    AND NEW.product_version_id = OLD.product_version_id
    AND NEW.created_at = OLD.created_at
    AND EXISTS (
        SELECT 1
        FROM save
        INNER JOIN gold_account_contract AS contract
            ON contract.save_id = save.id
           AND contract.run_revision = save.run_revision
           AND contract.financial_account_id = NEW.financial_account_id
        INNER JOIN financial_account AS account
            ON account.save_id = contract.save_id
           AND account.run_revision = contract.run_revision
           AND account.id = contract.financial_account_id
        WHERE save.id = NEW.save_id
          AND save.run_revision = NEW.run_revision
          AND contract.status = 'active'
          AND contract.product_version_id = NEW.product_version_id
          AND account.status = 'open'
          AND account.account_type = 'krxGold'
    ),
    OLD.financial_account_id,
    NULL
);

CREATE TRIGGER tr_gold_position_no_delete
BEFORE DELETE ON gold_position
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'gold positions must be retained at zero';

CREATE TABLE gold_execution (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    state_revision              BIGINT UNSIGNED NOT NULL,
    game_day                    INT UNSIGNED    NOT NULL,
    command_id                  CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    financial_account_id        BIGINT UNSIGNED NOT NULL,
    product_version_id          BIGINT UNSIGNED NOT NULL,
    side                        VARCHAR(4) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    quantity_gram               INT UNSIGNED    NOT NULL,
    price_krw_per_gram          BIGINT          NOT NULL,
    gross_amount_krw            BIGINT          NOT NULL,
    fee_krw                     BIGINT          NOT NULL,
    tax_krw                     BIGINT          NOT NULL,
    removed_cost_basis_krw      BIGINT          NOT NULL,
    realized_gain_loss_krw      BIGINT          NOT NULL,
    ledger_transaction_id       BIGINT UNSIGNED NOT NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_gold_execution_save_command (save_id, command_id),
    UNIQUE KEY uk_gold_execution_save_run_id (save_id, run_revision, id),
    KEY ix_gold_execution_account (save_id, run_revision, financial_account_id, id),
    KEY ix_gold_execution_ledger (save_id, run_revision, ledger_transaction_id),
    CONSTRAINT fk_gold_execution_identity
        FOREIGN KEY (save_id, command_id)
        REFERENCES command_identity (save_id, command_id) ON DELETE CASCADE,
    CONSTRAINT fk_gold_execution_contract
        FOREIGN KEY (save_id, run_revision, financial_account_id)
        REFERENCES gold_account_contract (save_id, run_revision, financial_account_id)
        ON DELETE CASCADE,
    CONSTRAINT fk_gold_execution_product
        FOREIGN KEY (product_version_id) REFERENCES gold_product_version (id),
    CONSTRAINT fk_gold_execution_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT ck_gold_execution_side CHECK (side IN ('buy', 'sell')),
    CONSTRAINT ck_gold_execution_amounts CHECK (
        quantity_gram > 0
        AND price_krw_per_gram > 0
        AND gross_amount_krw > 0
        AND fee_krw >= 0
        AND tax_krw >= 0
        AND removed_cost_basis_krw >= 0
    ),
    CONSTRAINT ck_gold_execution_result CHECK (
        (
            side = 'buy'
            AND removed_cost_basis_krw = 0
            AND realized_gain_loss_krw = 0
        )
        OR (
            side = 'sell'
            AND CAST(realized_gain_loss_krw AS DECIMAL(65, 0))
                = CAST(gross_amount_krw AS DECIMAL(65, 0))
                - CAST(fee_krw AS DECIMAL(65, 0))
                - CAST(tax_krw AS DECIMAL(65, 0))
                - CAST(removed_cost_basis_krw AS DECIMAL(65, 0))
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_gold_execution_valid_insert
BEFORE INSERT ON gold_execution
FOR EACH ROW
SET NEW.financial_account_id = IF(
    EXISTS (
        SELECT 1
        FROM command_identity AS identity
        INNER JOIN save ON save.id = identity.save_id
        INNER JOIN gold_account_contract AS contract
            ON contract.save_id = save.id
           AND contract.run_revision = save.run_revision
           AND contract.financial_account_id = NEW.financial_account_id
        WHERE identity.save_id = NEW.save_id
          AND BINARY identity.command_id = BINARY NEW.command_id
          AND identity.initial_run_revision = NEW.run_revision
          AND identity.initial_state_revision + 1 = NEW.state_revision
          AND identity.initial_game_day = NEW.game_day
          AND save.run_revision = NEW.run_revision
          AND contract.status = 'active'
          AND contract.product_version_id = NEW.product_version_id
    ),
    NEW.financial_account_id,
    NULL
);

CREATE TRIGGER tr_gold_execution_no_update
BEFORE UPDATE ON gold_execution
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'gold executions are append-only';

CREATE TRIGGER tr_gold_execution_no_delete
BEFORE DELETE ON gold_execution
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'gold executions are append-only';

CREATE TABLE gold_withdrawal (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    state_revision              BIGINT UNSIGNED NOT NULL,
    game_day                    INT UNSIGNED    NOT NULL,
    command_id                  CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    financial_account_id        BIGINT UNSIGNED NOT NULL,
    product_version_id          BIGINT UNSIGNED NOT NULL,
    bar_size_gram               SMALLINT UNSIGNED NOT NULL,
    bar_count                   INT UNSIGNED    NOT NULL,
    quantity_gram               INT UNSIGNED    NOT NULL,
    removed_cost_basis_krw      BIGINT          NOT NULL,
    vat_krw                     BIGINT          NOT NULL,
    fee_krw                     BIGINT          NOT NULL,
    cash_charged_krw            BIGINT          NOT NULL,
    ledger_transaction_id       BIGINT UNSIGNED     NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_gold_withdrawal_save_command (save_id, command_id),
    UNIQUE KEY uk_gold_withdrawal_save_run_id (save_id, run_revision, id),
    KEY ix_gold_withdrawal_account (save_id, run_revision, financial_account_id, id),
    KEY ix_gold_withdrawal_ledger (save_id, run_revision, ledger_transaction_id),
    CONSTRAINT fk_gold_withdrawal_identity
        FOREIGN KEY (save_id, command_id)
        REFERENCES command_identity (save_id, command_id) ON DELETE CASCADE,
    CONSTRAINT fk_gold_withdrawal_contract
        FOREIGN KEY (save_id, run_revision, financial_account_id)
        REFERENCES gold_account_contract (save_id, run_revision, financial_account_id)
        ON DELETE CASCADE,
    CONSTRAINT fk_gold_withdrawal_product
        FOREIGN KEY (product_version_id) REFERENCES gold_product_version (id),
    CONSTRAINT fk_gold_withdrawal_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT ck_gold_withdrawal_bar CHECK (
        bar_size_gram IN (100, 1000)
        AND bar_count > 0
        AND CAST(quantity_gram AS DECIMAL(65, 0))
            = CAST(bar_size_gram AS DECIMAL(65, 0)) * bar_count
    ),
    CONSTRAINT ck_gold_withdrawal_amounts CHECK (
        removed_cost_basis_krw > 0
        AND vat_krw >= 0
        AND fee_krw >= 0
        AND CAST(cash_charged_krw AS DECIMAL(65, 0))
            = CAST(vat_krw AS DECIMAL(65, 0)) + CAST(fee_krw AS DECIMAL(65, 0))
        AND (
            (cash_charged_krw = 0 AND ledger_transaction_id IS NULL)
            OR (cash_charged_krw > 0 AND ledger_transaction_id IS NOT NULL)
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_gold_withdrawal_valid_insert
BEFORE INSERT ON gold_withdrawal
FOR EACH ROW
SET NEW.financial_account_id = IF(
    EXISTS (
        SELECT 1
        FROM command_identity AS identity
        INNER JOIN save ON save.id = identity.save_id
        INNER JOIN gold_account_contract AS contract
            ON contract.save_id = save.id
           AND contract.run_revision = save.run_revision
           AND contract.financial_account_id = NEW.financial_account_id
        WHERE identity.save_id = NEW.save_id
          AND BINARY identity.command_id = BINARY NEW.command_id
          AND identity.initial_run_revision = NEW.run_revision
          AND identity.initial_state_revision + 1 = NEW.state_revision
          AND identity.initial_game_day = NEW.game_day
          AND save.run_revision = NEW.run_revision
          AND contract.status = 'active'
          AND contract.product_version_id = NEW.product_version_id
    ),
    NEW.financial_account_id,
    NULL
);

CREATE TRIGGER tr_gold_withdrawal_no_update
BEFORE UPDATE ON gold_withdrawal
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'gold withdrawals are append-only';

CREATE TRIGGER tr_gold_withdrawal_no_delete
BEFORE DELETE ON gold_withdrawal
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'gold withdrawals are append-only';

CREATE TABLE physical_gold_holding (
    save_id                 BIGINT UNSIGNED NOT NULL,
    run_revision            INT UNSIGNED    NOT NULL,
    financial_account_id    BIGINT UNSIGNED NOT NULL,
    bar_size_gram           SMALLINT UNSIGNED NOT NULL,
    bar_count               INT UNSIGNED    NOT NULL,
    created_at              DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at              DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, run_revision, bar_size_gram),
    CONSTRAINT fk_physical_gold_contract
        FOREIGN KEY (save_id, run_revision, financial_account_id)
        REFERENCES gold_account_contract (save_id, run_revision, financial_account_id)
        ON DELETE CASCADE,
    CONSTRAINT ck_physical_gold_bar CHECK (
        bar_size_gram IN (100, 1000) AND bar_count > 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_physical_gold_valid_insert
BEFORE INSERT ON physical_gold_holding
FOR EACH ROW
SET NEW.financial_account_id = IF(
    EXISTS (
        SELECT 1
        FROM save
        INNER JOIN gold_account_contract AS contract
            ON contract.save_id = save.id
           AND contract.run_revision = save.run_revision
           AND contract.financial_account_id = NEW.financial_account_id
        INNER JOIN financial_account AS account
            ON account.save_id = contract.save_id
           AND account.run_revision = contract.run_revision
           AND account.id = contract.financial_account_id
        WHERE save.id = NEW.save_id
          AND save.run_revision = NEW.run_revision
          AND contract.status = 'active'
          AND account.status = 'open'
          AND account.account_type = 'krxGold'
    ),
    NEW.financial_account_id,
    NULL
);

CREATE TRIGGER tr_physical_gold_increase_only
BEFORE UPDATE ON physical_gold_holding
FOR EACH ROW
SET NEW.financial_account_id = IF(
    NEW.save_id = OLD.save_id
    AND NEW.run_revision = OLD.run_revision
    AND NEW.financial_account_id = OLD.financial_account_id
    AND NEW.bar_size_gram = OLD.bar_size_gram
    AND NEW.bar_count > OLD.bar_count
    AND NEW.created_at = OLD.created_at
    AND EXISTS (
        SELECT 1
        FROM save
        INNER JOIN gold_account_contract AS contract
            ON contract.save_id = save.id
           AND contract.run_revision = save.run_revision
           AND contract.financial_account_id = NEW.financial_account_id
        WHERE save.id = NEW.save_id
          AND save.run_revision = NEW.run_revision
          AND contract.status = 'active'
    ),
    OLD.financial_account_id,
    NULL
);

CREATE TRIGGER tr_physical_gold_no_delete
BEFORE DELETE ON physical_gold_holding
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'physical gold holdings cannot decrease';

CREATE TABLE pension_valuation_state (
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    financial_account_id        BIGINT UNSIGNED NOT NULL,
    last_valuation_game_day      INT UNSIGNED    NOT NULL,
    position_market_value_krw    BIGINT          NOT NULL,
    risk_asset_value_krw         BIGINT          NOT NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, run_revision, financial_account_id),
    CONSTRAINT fk_pension_valuation_contract
        FOREIGN KEY (save_id, run_revision, financial_account_id)
        REFERENCES pension_account_contract (save_id, run_revision, financial_account_id)
        ON DELETE CASCADE,
    CONSTRAINT ck_pension_valuation_values CHECK (
        position_market_value_krw >= 0
        AND risk_asset_value_krw >= 0
        AND risk_asset_value_krw <= position_market_value_krw
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_pension_valuation_valid_insert
BEFORE INSERT ON pension_valuation_state
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

CREATE TRIGGER tr_pension_valuation_forward_only
BEFORE UPDATE ON pension_valuation_state
FOR EACH ROW
SET NEW.financial_account_id = IF(
    NEW.save_id = OLD.save_id
    AND NEW.run_revision = OLD.run_revision
    AND NEW.financial_account_id = OLD.financial_account_id
    AND NEW.last_valuation_game_day >= OLD.last_valuation_game_day
    AND NEW.created_at = OLD.created_at,
    OLD.financial_account_id,
    NULL
);

CREATE TRIGGER tr_pension_valuation_no_delete
BEFORE DELETE ON pension_valuation_state
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'pension valuation state cannot be deleted';

CREATE TABLE tax_account_value_event (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED    NOT NULL,
    financial_account_id                BIGINT UNSIGNED NOT NULL,
    event_game_day                      INT UNSIGNED    NOT NULL,
    cause                               VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    source_kind                         VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    source_id                           VARCHAR(128) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    occurrence                          INT UNSIGNED    NOT NULL,
    position_market_value_before_krw    BIGINT          NOT NULL,
    position_market_value_after_krw     BIGINT          NOT NULL,
    account_total_before_krw            BIGINT          NOT NULL,
    account_total_after_krw             BIGINT          NOT NULL,
    value_change_krw                    BIGINT          NOT NULL,
    before_tax_excluded_krw             BIGINT          NOT NULL,
    before_deferred_retirement_krw      BIGINT          NOT NULL,
    before_credited_contribution_krw    BIGINT          NOT NULL,
    before_earnings_krw                 BIGINT          NOT NULL,
    after_tax_excluded_krw              BIGINT          NOT NULL,
    after_deferred_retirement_krw       BIGINT          NOT NULL,
    after_credited_contribution_krw     BIGINT          NOT NULL,
    after_earnings_krw                  BIGINT          NOT NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_tax_value_event_source
        (save_id, run_revision, financial_account_id, source_kind, source_id, occurrence),
    KEY ix_tax_value_event_account
        (save_id, run_revision, financial_account_id, event_game_day, id),
    CONSTRAINT fk_tax_value_event_pension
        FOREIGN KEY (save_id, run_revision, financial_account_id)
        REFERENCES pension_account_contract (save_id, run_revision, financial_account_id)
        ON DELETE CASCADE,
    CONSTRAINT ck_tax_value_event_cause CHECK (
        cause IN ('dailyMarketToMarket', 'tradeBasisAdjustment')
    ),
    CONSTRAINT ck_tax_value_event_source CHECK (
        CHAR_LENGTH(source_kind) > 0 AND CHAR_LENGTH(source_id) > 0
    ),
    CONSTRAINT ck_tax_value_event_values CHECK (
        position_market_value_before_krw >= 0
        AND position_market_value_after_krw >= 0
        AND account_total_before_krw >= 0
        AND account_total_after_krw >= 0
        AND before_tax_excluded_krw >= 0
        AND before_deferred_retirement_krw >= 0
        AND before_credited_contribution_krw >= 0
        AND before_earnings_krw >= 0
        AND after_tax_excluded_krw >= 0
        AND after_deferred_retirement_krw >= 0
        AND after_credited_contribution_krw >= 0
        AND after_earnings_krw >= 0
        AND CAST(account_total_before_krw AS DECIMAL(65, 0))
            = CAST(before_tax_excluded_krw AS DECIMAL(65, 0))
            + CAST(before_deferred_retirement_krw AS DECIMAL(65, 0))
            + CAST(before_credited_contribution_krw AS DECIMAL(65, 0))
            + CAST(before_earnings_krw AS DECIMAL(65, 0))
        AND CAST(account_total_after_krw AS DECIMAL(65, 0))
            = CAST(after_tax_excluded_krw AS DECIMAL(65, 0))
            + CAST(after_deferred_retirement_krw AS DECIMAL(65, 0))
            + CAST(after_credited_contribution_krw AS DECIMAL(65, 0))
            + CAST(after_earnings_krw AS DECIMAL(65, 0))
    ),
    CONSTRAINT ck_tax_value_event_delta CHECK (
        (
            cause = 'dailyMarketToMarket'
            AND CAST(position_market_value_after_krw AS DECIMAL(65, 0))
                - CAST(position_market_value_before_krw AS DECIMAL(65, 0))
                = value_change_krw
            AND CAST(account_total_after_krw AS DECIMAL(65, 0))
                - CAST(account_total_before_krw AS DECIMAL(65, 0))
                = value_change_krw
        )
        OR
        (
            cause = 'tradeBasisAdjustment'
            AND value_change_krw = 0
            AND account_total_after_krw = account_total_before_krw
            AND before_tax_excluded_krw = after_tax_excluded_krw
            AND before_deferred_retirement_krw = after_deferred_retirement_krw
            AND before_credited_contribution_krw = after_credited_contribution_krw
            AND before_earnings_krw = after_earnings_krw
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_tax_value_event_valid_insert
BEFORE INSERT ON tax_account_value_event
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

CREATE TRIGGER tr_tax_value_event_no_update
BEFORE UPDATE ON tax_account_value_event
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'tax account value events are append-only';

CREATE TRIGGER tr_tax_value_event_no_delete
BEFORE DELETE ON tax_account_value_event
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'tax account value events are append-only';
