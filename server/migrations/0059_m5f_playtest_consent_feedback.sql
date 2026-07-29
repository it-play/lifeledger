-- M5-F: explicit playtest feedback consent and owner-scoped deletion authority.

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;

CREATE TABLE playtest_consent_policy_version (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    scope                       VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    policy_key                  VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    version_no                  INT UNSIGNED NOT NULL,
    schema_version              SMALLINT UNSIGNED NOT NULL,
    display_name                VARCHAR(120) NOT NULL,
    notice_text                 VARCHAR(2000) NOT NULL,
    canonical_manifest_json     LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    canonical_sha256            CHAR(64) CHARACTER SET ascii COLLATE ascii_bin
        GENERATED ALWAYS AS (SHA2(canonical_manifest_json, 256)) STORED,
    sealed_at                   DATETIME(6) NOT NULL,
    created_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY uk_playtest_consent_policy_key_version (policy_key, version_no),
    UNIQUE KEY uk_playtest_consent_policy_scope_version (scope, version_no),
    UNIQUE KEY uk_playtest_consent_policy_sha (canonical_sha256),
    CONSTRAINT ck_playtest_consent_policy_identity CHECK (
        scope = 'feedbackSubmission'
        AND policy_key REGEXP '^[a-z][a-zA-Z0-9._-]{0,95}$'
        AND version_no > 0
        AND schema_version > 0
        AND CHAR_LENGTH(display_name) BETWEEN 1 AND 120
        AND CHAR_LENGTH(notice_text) BETWEEN 1 AND 2000
    ),
    CONSTRAINT ck_playtest_consent_policy_manifest CHECK (
        JSON_VALID(canonical_manifest_json)
        AND JSON_TYPE(canonical_manifest_json) = 'OBJECT'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_playtest_consent_policy_valid_insert
BEFORE INSERT ON playtest_consent_policy_version
FOR EACH ROW
SET NEW.policy_key = IF(
    JSON_UNQUOTE(JSON_EXTRACT(NEW.canonical_manifest_json, '$.scope')) = NEW.scope
        AND JSON_UNQUOTE(JSON_EXTRACT(NEW.canonical_manifest_json, '$.policyKey'))
            = NEW.policy_key
        AND CAST(JSON_UNQUOTE(JSON_EXTRACT(
            NEW.canonical_manifest_json, '$.version')) AS UNSIGNED) = NEW.version_no
        AND CAST(JSON_UNQUOTE(JSON_EXTRACT(
            NEW.canonical_manifest_json, '$.schemaVersion')) AS UNSIGNED) = NEW.schema_version
        AND JSON_UNQUOTE(JSON_EXTRACT(NEW.canonical_manifest_json, '$.displayName'))
            = NEW.display_name
        AND JSON_UNQUOTE(JSON_EXTRACT(NEW.canonical_manifest_json, '$.noticeText'))
            = NEW.notice_text
        AND JSON_UNQUOTE(JSON_EXTRACT(
            NEW.canonical_manifest_json, '$.analyticsCollection')) = 'disabled'
        AND CAST(JSON_UNQUOTE(JSON_EXTRACT(
            NEW.canonical_manifest_json, '$.maximumActiveFeedback')) AS UNSIGNED) = 20
        AND CAST(JSON_UNQUOTE(JSON_EXTRACT(
            NEW.canonical_manifest_json, '$.messageMaximumCharacters')) AS UNSIGNED) = 500,
    NEW.policy_key,
    NULL
);

CREATE TRIGGER tr_playtest_consent_policy_no_update
BEFORE UPDATE ON playtest_consent_policy_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'playtest consent policy versions are immutable';

CREATE TRIGGER tr_playtest_consent_policy_no_delete
BEFORE DELETE ON playtest_consent_policy_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'playtest consent policy versions are immutable';

INSERT INTO playtest_consent_policy_version
    (scope, policy_key, version_no, schema_version, display_name, notice_text,
     canonical_manifest_json, sealed_at)
VALUES (
    'feedbackSubmission',
    'development-feedback-consent-2026',
    1,
    1,
    '개발 플레이테스트 피드백 동의',
    '버그와 사용성 피드백을 개발 목적으로 제출합니다. 실제 재산, 소득, 건강, 캐릭터 값, 원 단위 금액, OAuth 프로필, 세션 또는 명령 ID를 적지 마세요. 선택한 실행의 재현용 해시만 서버가 붙입니다. 동의를 철회하면 활성 피드백 본문과 실행 참조가 즉시 삭제됩니다. 사용 분석은 수집하지 않습니다.',
    '{"analyticsCollection":"disabled","displayName":"개발 플레이테스트 피드백 동의","maximumActiveFeedback":20,"messageMaximumCharacters":500,"noticeText":"버그와 사용성 피드백을 개발 목적으로 제출합니다. 실제 재산, 소득, 건강, 캐릭터 값, 원 단위 금액, OAuth 프로필, 세션 또는 명령 ID를 적지 마세요. 선택한 실행의 재현용 해시만 서버가 붙입니다. 동의를 철회하면 활성 피드백 본문과 실행 참조가 즉시 삭제됩니다. 사용 분석은 수집하지 않습니다.","policyKey":"development-feedback-consent-2026","retentionPolicy":"동의 유지 중에만 활성 본문을 보관하고 철회 시 즉시 지웁니다. 외부 모집 전 최대 보존 기간은 새 정책으로 고지합니다.","schemaVersion":1,"scope":"feedbackSubmission","version":1}',
    UTC_TIMESTAMP(6)
);

CREATE TABLE playtest_consent_policy_assignment (
    scope                       VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    policy_version_id           BIGINT UNSIGNED NOT NULL,
    assignment_revision         BIGINT UNSIGNED NOT NULL,
    updated_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
        ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (scope),
    CONSTRAINT fk_playtest_consent_assignment_policy
        FOREIGN KEY (policy_version_id) REFERENCES playtest_consent_policy_version (id),
    CONSTRAINT ck_playtest_consent_assignment CHECK (
        scope = 'feedbackSubmission'
        AND assignment_revision BETWEEN 1 AND 9007199254740991
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_playtest_consent_assignment_valid_insert
BEFORE INSERT ON playtest_consent_policy_assignment
FOR EACH ROW
SET NEW.scope = IF(
    EXISTS (
        SELECT 1
        FROM playtest_consent_policy_version AS policy
        WHERE policy.id = NEW.policy_version_id
          AND BINARY policy.scope = BINARY NEW.scope
    ),
    NEW.scope,
    NULL
);

CREATE TRIGGER tr_playtest_consent_assignment_valid_update
BEFORE UPDATE ON playtest_consent_policy_assignment
FOR EACH ROW
SET NEW.scope = IF(
    BINARY NEW.scope = BINARY OLD.scope
        AND NEW.assignment_revision > OLD.assignment_revision
        AND EXISTS (
            SELECT 1
            FROM playtest_consent_policy_version AS policy
            WHERE policy.id = NEW.policy_version_id
              AND BINARY policy.scope = BINARY NEW.scope
        ),
    NEW.scope,
    NULL
);

INSERT INTO playtest_consent_policy_assignment
    (scope, policy_version_id, assignment_revision)
SELECT scope, id, 1
FROM playtest_consent_policy_version
WHERE policy_key = 'development-feedback-consent-2026' AND version_no = 1;

CREATE TABLE playtest_consent (
    user_id                     BIGINT UNSIGNED NOT NULL,
    scope                       VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    policy_version_id           BIGINT UNSIGNED NOT NULL,
    status                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    revision                    BIGINT UNSIGNED NOT NULL,
    granted_at                  DATETIME(6) NOT NULL,
    withdrawn_at                DATETIME(6) NULL,
    created_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
        ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (user_id, scope),
    CONSTRAINT fk_playtest_consent_user
        FOREIGN KEY (user_id) REFERENCES user (id) ON DELETE CASCADE,
    CONSTRAINT fk_playtest_consent_policy
        FOREIGN KEY (policy_version_id) REFERENCES playtest_consent_policy_version (id),
    CONSTRAINT ck_playtest_consent_identity CHECK (
        scope = 'feedbackSubmission'
        AND revision BETWEEN 1 AND 9007199254740991
    ),
    CONSTRAINT ck_playtest_consent_status CHECK (
        (status = 'granted' AND withdrawn_at IS NULL)
        OR (status = 'withdrawn' AND withdrawn_at IS NOT NULL)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE playtest_consent_event (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    user_id                     BIGINT UNSIGNED NOT NULL,
    scope                       VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    policy_version_id           BIGINT UNSIGNED NOT NULL,
    consent_revision            BIGINT UNSIGNED NOT NULL,
    action                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    occurred_at                 DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY uk_playtest_consent_event_revision (user_id, scope, consent_revision),
    UNIQUE KEY uk_playtest_consent_event_owner (id, user_id, scope),
    CONSTRAINT fk_playtest_consent_event_current
        FOREIGN KEY (user_id, scope) REFERENCES playtest_consent (user_id, scope)
        ON DELETE CASCADE,
    CONSTRAINT fk_playtest_consent_event_policy
        FOREIGN KEY (policy_version_id) REFERENCES playtest_consent_policy_version (id),
    CONSTRAINT ck_playtest_consent_event CHECK (
        scope = 'feedbackSubmission'
        AND consent_revision BETWEEN 1 AND 9007199254740991
        AND action IN ('granted', 'withdrawn')
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_playtest_consent_event_valid_insert
BEFORE INSERT ON playtest_consent_event
FOR EACH ROW
SET NEW.action = IF(
    EXISTS (
        SELECT 1
        FROM playtest_consent AS consent
        WHERE consent.user_id = NEW.user_id
          AND BINARY consent.scope = BINARY NEW.scope
          AND consent.policy_version_id = NEW.policy_version_id
          AND consent.revision = NEW.consent_revision
          AND BINARY consent.status = BINARY NEW.action
    ),
    NEW.action,
    NULL
);

CREATE TRIGGER tr_playtest_consent_event_no_update
BEFORE UPDATE ON playtest_consent_event
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'playtest consent events are append-only';

CREATE TABLE playtest_feedback (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id                   CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    user_id                     BIGINT UNSIGNED NOT NULL,
    scope                       VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    consent_event_id            BIGINT UNSIGNED NOT NULL,
    status                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    category                    VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NULL,
    severity                    VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL,
    message                     VARCHAR(500) NULL,
    run_revision                INT UNSIGNED NULL,
    run_manifest_sha256         CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    finalization_sha256         CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    created_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    withdrawn_at                DATETIME(6) NULL,
    PRIMARY KEY (id),
    UNIQUE KEY uk_playtest_feedback_public_id (public_id),
    KEY ix_playtest_feedback_owner_status (user_id, scope, status, created_at, id),
    CONSTRAINT fk_playtest_feedback_consent_event
        FOREIGN KEY (consent_event_id, user_id, scope)
        REFERENCES playtest_consent_event (id, user_id, scope) ON DELETE CASCADE,
    CONSTRAINT ck_playtest_feedback_identity CHECK (
        public_id REGEXP '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
        AND scope = 'feedbackSubmission'
    ),
    CONSTRAINT ck_playtest_feedback_active_shape CHECK (
        (
            status = 'active'
            AND category IN ('bug', 'balance', 'usability', 'performance', 'rules', 'other')
            AND severity IN ('blocking', 'major', 'minor', 'suggestion')
            AND CHAR_LENGTH(message) BETWEEN 1 AND 500
            AND withdrawn_at IS NULL
            AND (
                (
                    run_revision IS NULL
                    AND run_manifest_sha256 IS NULL
                    AND finalization_sha256 IS NULL
                )
                OR (
                    run_revision IS NOT NULL
                    AND run_manifest_sha256 REGEXP '^[0-9a-f]{64}$'
                    AND (
                        finalization_sha256 IS NULL
                        OR finalization_sha256 REGEXP '^[0-9a-f]{64}$'
                    )
                )
            )
        )
        OR (
            status = 'withdrawn'
            AND category IS NULL
            AND severity IS NULL
            AND message IS NULL
            AND run_revision IS NULL
            AND run_manifest_sha256 IS NULL
            AND finalization_sha256 IS NULL
            AND withdrawn_at IS NOT NULL
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_playtest_feedback_valid_insert
BEFORE INSERT ON playtest_feedback
FOR EACH ROW
SET NEW.status = IF(
    NEW.status = 'active'
        AND EXISTS (
            SELECT 1
            FROM playtest_consent_event AS event
            INNER JOIN playtest_consent AS consent
                ON consent.user_id = event.user_id
               AND BINARY consent.scope = BINARY event.scope
            INNER JOIN playtest_consent_policy_assignment AS assignment
                ON BINARY assignment.scope = BINARY consent.scope
            WHERE event.id = NEW.consent_event_id
              AND event.user_id = NEW.user_id
              AND BINARY event.scope = BINARY NEW.scope
              AND event.action = 'granted'
              AND consent.status = 'granted'
              AND consent.revision = event.consent_revision
              AND consent.policy_version_id = assignment.policy_version_id
        ),
    NEW.status,
    NULL
);

CREATE TRIGGER tr_playtest_feedback_withdraw_only
BEFORE UPDATE ON playtest_feedback
FOR EACH ROW
SET NEW.status = IF(
    OLD.status = 'active'
        AND NEW.status = 'withdrawn'
        AND BINARY NEW.public_id = BINARY OLD.public_id
        AND NEW.user_id = OLD.user_id
        AND BINARY NEW.scope = BINARY OLD.scope
        AND NEW.consent_event_id = OLD.consent_event_id
        AND NEW.category IS NULL
        AND NEW.severity IS NULL
        AND NEW.message IS NULL
        AND NEW.run_revision IS NULL
        AND NEW.run_manifest_sha256 IS NULL
        AND NEW.finalization_sha256 IS NULL
        AND NEW.created_at = OLD.created_at
        AND NEW.withdrawn_at IS NOT NULL,
    NEW.status,
    NULL
);
