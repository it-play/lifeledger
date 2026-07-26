-- M1-A immutable market configuration and deterministic daily cache.

CREATE TABLE market_calibration (
    id              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    version         VARCHAR(64)     NOT NULL,
    parameters      JSON            NOT NULL,
    created_at      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_market_calibration_version (version)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE market_world (
    id                      BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    world_key               VARCHAR(64)     NOT NULL,
    seed                    BIGINT UNSIGNED NOT NULL,
    start_date              DATE            NOT NULL,
    day0_equity_close_krw   BIGINT          NOT NULL,
    calibration_id          BIGINT UNSIGNED NOT NULL,
    created_at              DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_market_world_world_key (world_key),
    KEY ix_market_world_calibration_id (calibration_id),
    CONSTRAINT fk_market_world_calibration
        FOREIGN KEY (calibration_id) REFERENCES market_calibration (id),
    CONSTRAINT ck_market_world_day0_equity_close
        CHECK (day0_equity_close_krw > 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

INSERT INTO market_calibration (id, version, parameters)
VALUES (
    1,
    'm1-2026-calibration-v1',
    '{
        "initialRegime": "expansion",
        "sessionsPerRegimeTransition": 21,
        "regimes": {
            "expansion": {
                "dailyDriftPpm": 420,
                "transitionPpm": {
                    "expansion": 870000,
                    "slowdown": 110000,
                    "recession": 10000,
                    "recovery": 10000
                }
            },
            "slowdown": {
                "dailyDriftPpm": 20,
                "transitionPpm": {
                    "expansion": 20000,
                    "slowdown": 870000,
                    "recession": 90000,
                    "recovery": 20000
                }
            },
            "recession": {
                "dailyDriftPpm": -630,
                "transitionPpm": {
                    "expansion": 5000,
                    "slowdown": 20000,
                    "recession": 870000,
                    "recovery": 105000
                }
            },
            "recovery": {
                "dailyDriftPpm": 620,
                "transitionPpm": {
                    "expansion": 105000,
                    "slowdown": 10000,
                    "recession": 15000,
                    "recovery": 870000
                }
            }
        },
        "equityGarch": {
            "initialVariancePpm2": 144000000,
            "omegaPpm2": 720000,
            "alphaPpm": 80000,
            "betaPpm": 915000,
            "minVariancePpm2": 16000000,
            "maxVariancePpm2": 2500000000
        }
    }'
);

INSERT INTO market_world
    (id, world_key, seed, start_date, day0_equity_close_krw, calibration_id)
VALUES
    (1, 'm1-2026-v1', 20260101, '2026-01-01', 100000, 1);

-- Explicit id 1 lets existing saves receive the common world without a nullable phase.
ALTER TABLE save
    ADD COLUMN market_world_id BIGINT UNSIGNED NOT NULL DEFAULT 1 AFTER run_revision,
    ADD KEY ix_save_market_world_id (market_world_id),
    ADD CONSTRAINT fk_save_market_world
        FOREIGN KEY (market_world_id) REFERENCES market_world (id);

-- MySQL does not allow foreign keys on user-partitioned InnoDB tables. The store verifies
-- world existence before generating and the composite primary key converges concurrent writers.
CREATE TABLE market_daily (
    world_id                   BIGINT UNSIGNED NOT NULL,
    game_day                  INT UNSIGNED    NOT NULL,
    market_date               DATE            NOT NULL,
    market_open               BOOLEAN         NOT NULL,
    session_index             INT UNSIGNED    NOT NULL,
    regime                    VARCHAR(16)     NOT NULL,
    equity_close_krw          BIGINT          NOT NULL,
    equity_return_ppm         BIGINT          NOT NULL,
    equity_residual_ppm       BIGINT          NOT NULL,
    equity_variance_ppm2      BIGINT          NOT NULL,
    created_at                DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (world_id, game_day),
    CONSTRAINT ck_market_daily_market_open CHECK (market_open IN (0, 1)),
    CONSTRAINT ck_market_daily_session_index CHECK (session_index <= game_day),
    CONSTRAINT ck_market_daily_regime
        CHECK (regime IN ('expansion', 'slowdown', 'recession', 'recovery')),
    CONSTRAINT ck_market_daily_equity_close CHECK (equity_close_krw > 0),
    CONSTRAINT ck_market_daily_equity_variance CHECK (equity_variance_ppm2 > 0),
    CONSTRAINT ck_market_daily_closed_return
        CHECK (market_open = 1 OR equity_return_ppm = 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci
PARTITION BY RANGE (game_day) (
    PARTITION p00000 VALUES LESS THAN (3660),
    PARTITION p03660 VALUES LESS THAN (7320),
    PARTITION p07320 VALUES LESS THAN (10980),
    PARTITION p10980 VALUES LESS THAN (14640),
    PARTITION p14640 VALUES LESS THAN (18300),
    PARTITION p18300 VALUES LESS THAN (21960),
    PARTITION p21960 VALUES LESS THAN (25620),
    PARTITION p25620 VALUES LESS THAN (29280),
    PARTITION p29280 VALUES LESS THAN (32940),
    PARTITION p32940 VALUES LESS THAN (36600),
    PARTITION pmax VALUES LESS THAN MAXVALUE
);

-- Configuration rows are versioned append-only; existing paths must never be rewritten.
CREATE TRIGGER tr_market_calibration_no_update
BEFORE UPDATE ON market_calibration
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'market calibration rows are immutable';

CREATE TRIGGER tr_market_calibration_no_delete
BEFORE DELETE ON market_calibration
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'market calibration rows are immutable';

CREATE TRIGGER tr_market_world_no_update
BEFORE UPDATE ON market_world
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'market world rows are immutable';

CREATE TRIGGER tr_market_world_no_delete
BEFORE DELETE ON market_world
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'market world rows are immutable';
