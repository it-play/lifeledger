-- M5-C: immutable ranked finalization boundary and liquidation evidence.

ALTER TABLE run_manifest
    ADD KEY ix_run_manifest_league_ranked
        (league_definition_id, ranking_eligible, save_id, run_revision);

CREATE TABLE run_finalization (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED NOT NULL,
    target_game_day             INT UNSIGNED NOT NULL,
    ranking_rule_version_id     BIGINT UNSIGNED NOT NULL,
    ranking_rule_sha256         CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    status                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    after_tax_net_worth_krw     BIGINT NULL,
    insolvency_days             INT UNSIGNED NULL,
    player_command_count        BIGINT UNSIGNED NULL,
    line_count                  INT UNSIGNED NULL,
    liquidation_canonical_json  LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NULL,
    liquidation_sha256          CHAR(64) CHARACTER SET ascii COLLATE ascii_bin
        GENERATED ALWAYS AS (SHA2(liquidation_canonical_json, 256)) STORED,
    failure_code                VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    completed_at                DATETIME(6) NULL,
    created_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY uk_run_finalization_source
        (save_id, run_revision, target_game_day, ranking_rule_version_id),
    KEY ix_run_finalization_ranking
        (status, after_tax_net_worth_krw, insolvency_days, player_command_count),
    CONSTRAINT fk_run_finalization_manifest
        FOREIGN KEY (save_id, run_revision)
        REFERENCES run_manifest (save_id, run_revision),
    CONSTRAINT fk_run_finalization_ranking_rule
        FOREIGN KEY (ranking_rule_version_id, ranking_rule_sha256)
        REFERENCES ranking_rule_version (id, ranking_rule_sha256),
    CONSTRAINT ck_run_finalization_identity CHECK (
        target_game_day > 0
        AND ranking_rule_sha256 REGEXP '^[0-9a-f]{64}$'
    ),
    CONSTRAINT ck_run_finalization_status CHECK (
        status IN ('planning', 'completed', 'failed')
    ),
    CONSTRAINT ck_run_finalization_shape CHECK (
        (
            status = 'planning'
            AND after_tax_net_worth_krw IS NULL
            AND insolvency_days IS NULL
            AND player_command_count IS NULL
            AND line_count IS NULL
            AND liquidation_canonical_json IS NULL
            AND failure_code IS NULL
            AND completed_at IS NULL
        )
        OR (
            status = 'completed'
            AND after_tax_net_worth_krw IS NOT NULL
            AND insolvency_days IS NOT NULL
            AND player_command_count IS NOT NULL
            AND line_count BETWEEN 1 AND 256
            AND JSON_VALID(liquidation_canonical_json)
            AND JSON_TYPE(liquidation_canonical_json) = 'OBJECT'
            AND failure_code IS NULL
            AND completed_at IS NOT NULL
        )
        OR (
            status = 'failed'
            AND after_tax_net_worth_krw IS NULL
            AND insolvency_days IS NULL
            AND player_command_count IS NULL
            AND line_count = 0
            AND liquidation_canonical_json IS NULL
            AND CHAR_LENGTH(failure_code) > 0
            AND completed_at IS NOT NULL
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE liquidation_line (
    run_finalization_id        BIGINT UNSIGNED NOT NULL,
    line_no                    INT UNSIGNED NOT NULL,
    component_key              VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    gross_krw                  BIGINT NOT NULL,
    cost_krw                   BIGINT NOT NULL,
    tax_krw                    BIGINT NOT NULL,
    net_krw                    BIGINT NOT NULL,
    policy_reference           VARCHAR(128) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    canonical_line_json        LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    line_sha256                CHAR(64) CHARACTER SET ascii COLLATE ascii_bin
        GENERATED ALWAYS AS (SHA2(canonical_line_json, 256)) STORED,
    created_at                 DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (run_finalization_id, line_no),
    UNIQUE KEY uk_liquidation_line_hash (run_finalization_id, line_sha256),
    CONSTRAINT fk_liquidation_line_finalization
        FOREIGN KEY (run_finalization_id) REFERENCES run_finalization (id),
    CONSTRAINT ck_liquidation_line_identity CHECK (
        line_no BETWEEN 1 AND 256
        AND component_key REGEXP '^[a-z][a-zA-Z0-9._-]{0,63}$'
        AND CHAR_LENGTH(policy_reference) > 0
    ),
    CONSTRAINT ck_liquidation_line_math CHECK (
        CAST(gross_krw AS DECIMAL(65, 0))
            - CAST(cost_krw AS DECIMAL(65, 0))
            - CAST(tax_krw AS DECIMAL(65, 0))
        = CAST(net_krw AS DECIMAL(65, 0))
    ),
    CONSTRAINT ck_liquidation_line_json CHECK (
        JSON_VALID(canonical_line_json)
        AND JSON_TYPE(canonical_line_json) = 'OBJECT'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_run_finalization_valid_insert
BEFORE INSERT ON run_finalization
FOR EACH ROW
SET NEW.status = IF(
    NEW.status = 'planning'
        AND EXISTS (
            SELECT 1
            FROM run_manifest AS manifest
            INNER JOIN save AS save_row ON save_row.id = manifest.save_id
            WHERE manifest.save_id = NEW.save_id
              AND manifest.run_revision = NEW.run_revision
              AND manifest.ranking_eligible = TRUE
              AND manifest.target_game_day = NEW.target_game_day
              AND manifest.ranking_rule_version_id = NEW.ranking_rule_version_id
              AND BINARY manifest.ranking_rule_sha256 = BINARY NEW.ranking_rule_sha256
              AND save_row.run_revision = manifest.run_revision
              AND save_row.game_day = manifest.target_game_day
        ),
    NEW.status,
    NULL
);

CREATE TRIGGER tr_liquidation_line_valid_insert
BEFORE INSERT ON liquidation_line
FOR EACH ROW
SET NEW.component_key = IF(
    EXISTS (
        SELECT 1
        FROM run_finalization AS finalization
        WHERE finalization.id = NEW.run_finalization_id
          AND finalization.status = 'planning'
    ),
    NEW.component_key,
    NULL
);

CREATE TRIGGER tr_liquidation_line_no_update
BEFORE UPDATE ON liquidation_line
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'liquidation lines are immutable';

CREATE TRIGGER tr_liquidation_line_no_delete
BEFORE DELETE ON liquidation_line
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'liquidation lines are immutable';

CREATE TRIGGER tr_run_finalization_terminal_transition
BEFORE UPDATE ON run_finalization
FOR EACH ROW
SET NEW.status = IF(
    NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.target_game_day = OLD.target_game_day
        AND NEW.ranking_rule_version_id = OLD.ranking_rule_version_id
        AND BINARY NEW.ranking_rule_sha256 = BINARY OLD.ranking_rule_sha256
        AND NEW.created_at = OLD.created_at
        AND OLD.status = 'planning'
        AND (
            (
                NEW.status = 'completed'
                AND NEW.line_count = (
                    SELECT COUNT(*)
                    FROM liquidation_line AS line
                    WHERE line.run_finalization_id = OLD.id
                )
                AND CAST(NEW.after_tax_net_worth_krw AS DECIMAL(65, 0)) = (
                    SELECT COALESCE(SUM(CAST(line.net_krw AS DECIMAL(65, 0))), 0)
                    FROM liquidation_line AS line
                    WHERE line.run_finalization_id = OLD.id
                )
            )
            OR (
                NEW.status = 'failed'
                AND NOT EXISTS (
                    SELECT 1
                    FROM liquidation_line AS line
                    WHERE line.run_finalization_id = OLD.id
                )
            )
        ),
    NEW.status,
    NULL
);

CREATE TRIGGER tr_run_finalization_no_delete
BEFORE DELETE ON run_finalization
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'run finalizations are append-only';
