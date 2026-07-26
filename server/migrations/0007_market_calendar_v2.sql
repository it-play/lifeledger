-- M1-B versioned KRX calendar world. Existing saves keep their assigned immutable world.

-- The numerical calibration stays fixed; v2 changes only the versioned trading calendar.
INSERT INTO market_calibration (id, version, parameters)
SELECT 2, 'm1-2026-calibration-v2', parameters
FROM market_calibration
WHERE version = 'm1-2026-calibration-v1';

INSERT INTO market_world
    (id, world_key, seed, start_date, day0_equity_close_krw, calibration_id)
VALUES
    (2, 'm1-2026-v2', 20260101, '2026-01-01', 100000, 2);

-- Save rows retain their world id. This pointer is consulted only when a new run starts.
CREATE TABLE market_world_assignment (
    assignment_key        VARCHAR(32)     NOT NULL,
    world_id              BIGINT UNSIGNED NOT NULL,
    assignment_revision   BIGINT UNSIGNED NOT NULL DEFAULT 1,
    updated_at            DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (assignment_key),
    KEY ix_market_world_assignment_world_id (world_id),
    CONSTRAINT fk_market_world_assignment_world
        FOREIGN KEY (world_id) REFERENCES market_world (id),
    CONSTRAINT ck_market_world_assignment_key
        CHECK (assignment_key = 'newRun')
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

INSERT INTO market_world_assignment (assignment_key, world_id)
VALUES ('newRun', 2);

-- The revision changes on every pointer write, so A -> B -> A cannot fool a prepared run.
CREATE TRIGGER tr_market_world_assignment_bump_revision
BEFORE UPDATE ON market_world_assignment
FOR EACH ROW
SET NEW.assignment_revision = OLD.assignment_revision + 1;

CREATE TRIGGER tr_market_world_assignment_no_delete
BEFORE DELETE ON market_world_assignment
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'market world assignment must be updated in place';
