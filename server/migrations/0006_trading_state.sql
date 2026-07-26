-- M1-B player-state ordering, LLX positions, and the append-only execution ledger.

ALTER TABLE save
    ADD COLUMN state_revision BIGINT UNSIGNED NOT NULL DEFAULT 0 AFTER run_revision;

CREATE TABLE asset_position (
    save_id                 BIGINT UNSIGNED NOT NULL,
    symbol                  VARCHAR(8) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    quantity                INT UNSIGNED    NOT NULL,
    total_cost_basis_krw    BIGINT          NOT NULL,
    created_at              DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at              DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
                                                ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, symbol),
    CONSTRAINT fk_asset_position_save
        FOREIGN KEY (save_id) REFERENCES save (id) ON DELETE CASCADE,
    CONSTRAINT ck_asset_position_symbol CHECK (symbol = 'LLX'),
    CONSTRAINT ck_asset_position_quantity CHECK (quantity BETWEEN 1 AND 1000000),
    CONSTRAINT ck_asset_position_cost_basis CHECK (total_cost_basis_krw > 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE trade_execution (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    order_id                    CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    expected_run_revision       INT UNSIGNED    NOT NULL,
    expected_state_revision     BIGINT UNSIGNED NOT NULL,
    expected_game_day           INT UNSIGNED    NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    state_revision              BIGINT UNSIGNED NOT NULL,
    game_day                    INT UNSIGNED    NOT NULL,
    side                        VARCHAR(4) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    symbol                      VARCHAR(8) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    quantity                    INT UNSIGNED    NOT NULL,
    price_krw                   BIGINT          NOT NULL,
    gross_amount_krw            BIGINT          NOT NULL,
    removed_cost_basis_krw      BIGINT          NOT NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_trade_execution_save_order (save_id, order_id),
    KEY ix_trade_execution_save_run_state (save_id, run_revision, state_revision),
    CONSTRAINT fk_trade_execution_save
        FOREIGN KEY (save_id) REFERENCES save (id) ON DELETE CASCADE,
    CONSTRAINT ck_trade_execution_side CHECK (side IN ('buy', 'sell')),
    CONSTRAINT ck_trade_execution_symbol CHECK (symbol = 'LLX'),
    CONSTRAINT ck_trade_execution_quantity CHECK (quantity BETWEEN 1 AND 1000000),
    CONSTRAINT ck_trade_execution_price CHECK (price_krw > 0),
    CONSTRAINT ck_trade_execution_gross CHECK (gross_amount_krw > 0),
    CONSTRAINT ck_trade_execution_removed_basis CHECK (removed_cost_basis_krw >= 0),
    CONSTRAINT ck_trade_execution_run_revision
        CHECK (run_revision = expected_run_revision),
    CONSTRAINT ck_trade_execution_state_revision
        CHECK (state_revision = expected_state_revision + 1),
    CONSTRAINT ck_trade_execution_game_day CHECK (game_day = expected_game_day)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_trade_execution_no_update
BEFORE UPDATE ON trade_execution
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'trade execution rows are append-only';

CREATE TRIGGER tr_trade_execution_no_delete
BEFORE DELETE ON trade_execution
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'trade execution rows are append-only';
