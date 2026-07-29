-- M5-D: immutable offline policy, opt-in state, DB-time presence, and progress lease.

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;

CREATE TABLE offline_policy_version (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    policy_key                  VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    version_no                  INT UNSIGNED NOT NULL,
    schema_version              SMALLINT UNSIGNED NOT NULL,
    status                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    engine_version              VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    cadence_seconds             INT UNSIGNED NOT NULL,
    absence_window_cap_days     INT UNSIGNED NOT NULL,
    max_worker_batch_days       SMALLINT UNSIGNED NOT NULL,
    lease_seconds               SMALLINT UNSIGNED NOT NULL,
    presence_ttl_seconds        SMALLINT UNSIGNED NOT NULL,
    heartbeat_seconds           SMALLINT UNSIGNED NOT NULL,
    online_intent_ttl_seconds   SMALLINT UNSIGNED NOT NULL,
    canonical_manifest_json     LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    canonical_sha256            CHAR(64) CHARACTER SET ascii COLLATE ascii_bin
        GENERATED ALWAYS AS (SHA2(canonical_manifest_json, 256)) STORED,
    sealed_at                   DATETIME(6) NOT NULL,
    created_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY uk_offline_policy_key_version (policy_key, version_no),
    UNIQUE KEY uk_offline_policy_sha (canonical_sha256),
    UNIQUE KEY uk_offline_policy_id_sha (id, canonical_sha256),
    CONSTRAINT ck_offline_policy_identity CHECK (
        policy_key REGEXP '^[a-z0-9][a-zA-Z0-9._-]{0,95}$'
        AND version_no > 0
        AND schema_version > 0
        AND CHAR_LENGTH(engine_version) > 0
    ),
    CONSTRAINT ck_offline_policy_status CHECK (status = 'sealed'),
    CONSTRAINT ck_offline_policy_limits CHECK (
        cadence_seconds BETWEEN 1 AND 604800
        AND absence_window_cap_days BETWEEN 1 AND 3650
        AND max_worker_batch_days BETWEEN 1 AND absence_window_cap_days
        AND lease_seconds BETWEEN 5 AND 3600
        AND heartbeat_seconds BETWEEN 1 AND presence_ttl_seconds
        AND presence_ttl_seconds > heartbeat_seconds
        AND online_intent_ttl_seconds BETWEEN 1 AND 3600
    ),
    CONSTRAINT ck_offline_policy_manifest CHECK (
        JSON_VALID(canonical_manifest_json)
        AND JSON_TYPE(canonical_manifest_json) = 'OBJECT'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_offline_policy_valid_insert
BEFORE INSERT ON offline_policy_version
FOR EACH ROW
SET NEW.policy_key = IF(
    NEW.status = 'sealed'
        AND NEW.sealed_at IS NOT NULL
        AND JSON_UNQUOTE(JSON_EXTRACT(NEW.canonical_manifest_json, '$.policyKey'))
            = NEW.policy_key
        AND CAST(JSON_UNQUOTE(JSON_EXTRACT(
                NEW.canonical_manifest_json, '$.version')) AS UNSIGNED)
            = NEW.version_no
        AND CAST(JSON_UNQUOTE(JSON_EXTRACT(
                NEW.canonical_manifest_json, '$.schemaVersion')) AS UNSIGNED)
            = NEW.schema_version
        AND JSON_UNQUOTE(JSON_EXTRACT(NEW.canonical_manifest_json, '$.engineVersion'))
            = NEW.engine_version
        AND CAST(JSON_UNQUOTE(JSON_EXTRACT(
                NEW.canonical_manifest_json, '$.cadenceSeconds')) AS UNSIGNED)
            = NEW.cadence_seconds
        AND CAST(JSON_UNQUOTE(JSON_EXTRACT(
                NEW.canonical_manifest_json, '$.absenceWindowCapDays')) AS UNSIGNED)
            = NEW.absence_window_cap_days
        AND CAST(JSON_UNQUOTE(JSON_EXTRACT(
                NEW.canonical_manifest_json, '$.maxWorkerBatchDays')) AS UNSIGNED)
            = NEW.max_worker_batch_days
        AND CAST(JSON_UNQUOTE(JSON_EXTRACT(
                NEW.canonical_manifest_json, '$.leaseSeconds')) AS UNSIGNED)
            = NEW.lease_seconds
        AND CAST(JSON_UNQUOTE(JSON_EXTRACT(
                NEW.canonical_manifest_json, '$.presenceTtlSeconds')) AS UNSIGNED)
            = NEW.presence_ttl_seconds
        AND CAST(JSON_UNQUOTE(JSON_EXTRACT(
                NEW.canonical_manifest_json, '$.heartbeatSeconds')) AS UNSIGNED)
            = NEW.heartbeat_seconds
        AND CAST(JSON_UNQUOTE(JSON_EXTRACT(
                NEW.canonical_manifest_json, '$.onlineIntentTtlSeconds')) AS UNSIGNED)
            = NEW.online_intent_ttl_seconds,
    NEW.policy_key,
    NULL
);

CREATE TRIGGER tr_offline_policy_no_update
BEFORE UPDATE ON offline_policy_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'offline policy versions are immutable';

CREATE TRIGGER tr_offline_policy_no_delete
BEFORE DELETE ON offline_policy_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'offline policy versions are immutable';

CREATE TABLE offline_policy_assignment (
    assignment_key              VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    offline_policy_version_id   BIGINT UNSIGNED NOT NULL,
    offline_policy_sha256       CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    assignment_revision         BIGINT UNSIGNED NOT NULL,
    updated_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
        ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (assignment_key),
    CONSTRAINT fk_offline_policy_assignment_version
        FOREIGN KEY (offline_policy_version_id, offline_policy_sha256)
        REFERENCES offline_policy_version (id, canonical_sha256),
    CONSTRAINT ck_offline_policy_assignment_identity CHECK (
        assignment_key REGEXP '^[a-z][a-zA-Z0-9._-]{0,63}$'
        AND assignment_revision > 0
        AND offline_policy_sha256 REGEXP '^[0-9a-f]{64}$'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

ALTER TABLE ranked_ruleset_release
    ADD COLUMN offline_policy_version_id BIGINT UNSIGNED NULL AFTER engine_version,
    ADD COLUMN offline_policy_sha256 CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL
        AFTER offline_policy_version_id,
    ADD CONSTRAINT fk_ranked_ruleset_release_offline_policy
        FOREIGN KEY (offline_policy_version_id, offline_policy_sha256)
        REFERENCES offline_policy_version (id, canonical_sha256),
    ADD CONSTRAINT ck_ranked_ruleset_release_offline_policy CHECK (
        (offline_policy_version_id IS NULL AND offline_policy_sha256 IS NULL)
        OR (
            offline_policy_version_id IS NOT NULL
            AND offline_policy_sha256 REGEXP '^[0-9a-f]{64}$'
        )
    );

ALTER TABLE run_manifest
    ADD COLUMN offline_policy_sha256 CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL
        AFTER offline_policy_version_id,
    ADD CONSTRAINT fk_run_manifest_offline_policy
        FOREIGN KEY (offline_policy_version_id, offline_policy_sha256)
        REFERENCES offline_policy_version (id, canonical_sha256),
    ADD CONSTRAINT ck_run_manifest_offline_policy CHECK (
        (offline_policy_version_id IS NULL AND offline_policy_sha256 IS NULL)
        OR (
            offline_policy_version_id IS NOT NULL
            AND offline_policy_sha256 REGEXP '^[0-9a-f]{64}$'
        )
    );

CREATE TABLE offline_progress_setting (
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED NOT NULL,
    offline_policy_version_id   BIGINT UNSIGNED NOT NULL,
    offline_policy_sha256       CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    enabled                     BOOLEAN NOT NULL,
    status                      VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    absence_started_at          DATETIME(6) NULL,
    accrued_through             DATETIME(6) NULL,
    accrual_limit_at            DATETIME(6) NULL,
    window_accrued_days         INT UNSIGNED NOT NULL DEFAULT 0,
    pending_days                INT UNSIGNED NOT NULL DEFAULT 0,
    processed_days              BIGINT UNSIGNED NOT NULL DEFAULT 0,
    cancelled_pending_days      BIGINT UNSIGNED NOT NULL DEFAULT 0,
    online_intent_at            DATETIME(6) NULL,
    last_error_code             VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    revision                    BIGINT UNSIGNED NOT NULL,
    created_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
        ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (save_id, run_revision),
    KEY ix_offline_setting_worker
        (enabled, status, pending_days, absence_started_at, save_id),
    CONSTRAINT fk_offline_setting_manifest
        FOREIGN KEY (save_id, run_revision)
        REFERENCES run_manifest (save_id, run_revision),
    CONSTRAINT fk_offline_setting_policy
        FOREIGN KEY (offline_policy_version_id, offline_policy_sha256)
        REFERENCES offline_policy_version (id, canonical_sha256),
    CONSTRAINT ck_offline_setting_identity CHECK (
        offline_policy_sha256 REGEXP '^[0-9a-f]{64}$'
        AND revision > 0
        AND enabled IN (FALSE, TRUE)
    ),
    CONSTRAINT ck_offline_setting_status CHECK (
        status IN ('active', 'pausedBySystem')
        AND (status <> 'pausedBySystem' OR enabled = TRUE)
    ),
    CONSTRAINT ck_offline_setting_window CHECK (
        (
            absence_started_at IS NULL
            AND accrued_through IS NULL
            AND accrual_limit_at IS NULL
        )
        OR (
            enabled = TRUE
            AND status = 'active'
            AND absence_started_at IS NOT NULL
            AND accrued_through IS NOT NULL
            AND accrual_limit_at IS NOT NULL
            AND absence_started_at <= accrued_through
            AND accrued_through <= accrual_limit_at
        )
    ),
    CONSTRAINT ck_offline_setting_error CHECK (
        (status = 'active' AND last_error_code IS NULL)
        OR (status = 'pausedBySystem' AND CHAR_LENGTH(last_error_code) > 0)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE offline_online_presence (
    connection_token_sha256     CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED NOT NULL,
    expires_at                  DATETIME(6) NOT NULL,
    opened_at                   DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    heartbeat_at                DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (connection_token_sha256),
    KEY ix_offline_presence_save_expiry (save_id, run_revision, expires_at),
    CONSTRAINT fk_offline_presence_save
        FOREIGN KEY (save_id)
        REFERENCES save (id) ON DELETE CASCADE,
    CONSTRAINT ck_offline_presence_token CHECK (
        connection_token_sha256 REGEXP '^[0-9a-f]{64}$'
        AND opened_at <= heartbeat_at
        AND heartbeat_at < expires_at
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE progress_lease (
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED NOT NULL,
    holder_kind                 VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    holder_token_sha256         CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    generation                  BIGINT UNSIGNED NOT NULL,
    expires_at                  DATETIME(6) NOT NULL,
    acquired_at                 DATETIME(6) NOT NULL,
    renewed_at                  DATETIME(6) NOT NULL,
    PRIMARY KEY (save_id),
    KEY ix_progress_lease_expiry (expires_at, save_id),
    CONSTRAINT fk_progress_lease_save
        FOREIGN KEY (save_id)
        REFERENCES save (id) ON DELETE CASCADE,
    CONSTRAINT ck_progress_lease_identity CHECK (
        holder_kind IN ('online', 'worker')
        AND holder_token_sha256 REGEXP '^[0-9a-f]{64}$'
        AND generation > 0
        AND acquired_at <= renewed_at
        AND renewed_at < expires_at
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE offline_progress_attempt (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    attempt_key                 CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    event_kind                  VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED NOT NULL,
    game_day                    INT UNSIGNED NOT NULL,
    lease_generation            BIGINT UNSIGNED NOT NULL,
    retry_no                    SMALLINT UNSIGNED NOT NULL,
    engine_version              VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    error_code                  VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    created_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY uk_offline_attempt_event (attempt_key, event_kind),
    KEY ix_offline_attempt_run_day (save_id, run_revision, game_day, id),
    CONSTRAINT fk_offline_attempt_manifest
        FOREIGN KEY (save_id, run_revision)
        REFERENCES run_manifest (save_id, run_revision),
    CONSTRAINT ck_offline_attempt_identity CHECK (
        attempt_key REGEXP '^[0-9a-f-]{36}$'
        AND event_kind IN ('started', 'committed', 'failed')
        AND game_day > 0
        AND lease_generation > 0
        AND CHAR_LENGTH(engine_version) > 0
    ),
    CONSTRAINT ck_offline_attempt_error CHECK (
        (event_kind = 'failed' AND CHAR_LENGTH(error_code) > 0)
        OR (event_kind <> 'failed' AND error_code IS NULL)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_offline_attempt_no_update
BEFORE UPDATE ON offline_progress_attempt
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'offline progress attempts are append-only';

CREATE TRIGGER tr_offline_attempt_no_delete
BEFORE DELETE ON offline_progress_attempt
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'offline progress attempts are append-only';

INSERT INTO offline_policy_version
    (policy_key, version_no, schema_version, status, engine_version,
     cadence_seconds, absence_window_cap_days, max_worker_batch_days,
     lease_seconds, presence_ttl_seconds, heartbeat_seconds,
     online_intent_ttl_seconds, canonical_manifest_json, sealed_at)
VALUES
    ('m5d-offline-progress-v1', 1, 1, 'sealed', 'm5a-dev-v1',
     60, 90, 7, 30, 45, 15, 30,
     CAST(JSON_OBJECT(
         'absenceWindowCapDays', 90,
         'cadenceSeconds', 60,
         'engineVersion', 'm5a-dev-v1',
         'heartbeatSeconds', 15,
         'leaseSeconds', 30,
         'maxWorkerBatchDays', 7,
         'onlineIntentTtlSeconds', 30,
         'policyKey', 'm5d-offline-progress-v1',
         'presenceTtlSeconds', 45,
         'schemaVersion', 1,
         'version', 1
     ) AS CHAR CHARACTER SET utf8mb4),
     CURRENT_TIMESTAMP(6));

INSERT INTO offline_policy_assignment
    (assignment_key, offline_policy_version_id, offline_policy_sha256, assignment_revision)
SELECT 'newSandboxRun', policy.id, policy.canonical_sha256, 1
FROM offline_policy_version AS policy
WHERE policy.policy_key = 'm5d-offline-progress-v1'
  AND policy.version_no = 1;
