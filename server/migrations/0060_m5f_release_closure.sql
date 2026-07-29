-- M5-F release closure: fixed feedback retention and automatic expiration (§9.5).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;

DROP TRIGGER tr_playtest_consent_policy_valid_insert;

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
            NEW.canonical_manifest_json, '$.messageMaximumCharacters')) AS UNSIGNED) = 500
        AND CAST(JSON_UNQUOTE(JSON_EXTRACT(
            NEW.canonical_manifest_json, '$.retentionMaximumDays')) AS UNSIGNED)
            BETWEEN 1 AND 365,
    NEW.policy_key,
    NULL
);

INSERT INTO playtest_consent_policy_version
    (scope, policy_key, version_no, schema_version, display_name, notice_text,
     canonical_manifest_json, sealed_at)
VALUES (
    'feedbackSubmission',
    'development-feedback-consent-2026',
    2,
    2,
    '개발 플레이테스트 피드백 동의',
    '버그와 사용성 피드백을 개발 목적으로 제출합니다. 실제 재산, 소득, 건강, 캐릭터 값, 원 단위 금액, OAuth 프로필, 세션 또는 명령 ID를 적지 마세요. 선택한 실행의 재현용 해시만 서버가 붙입니다. 활성 피드백 본문은 제출 후 최대 90일 보관하며 개별 삭제, 동의 철회 또는 계정 삭제 시 더 일찍 삭제됩니다. 사용 분석은 수집하지 않습니다.',
    '{"analyticsCollection":"disabled","displayName":"개발 플레이테스트 피드백 동의","maximumActiveFeedback":20,"messageMaximumCharacters":500,"noticeText":"버그와 사용성 피드백을 개발 목적으로 제출합니다. 실제 재산, 소득, 건강, 캐릭터 값, 원 단위 금액, OAuth 프로필, 세션 또는 명령 ID를 적지 마세요. 선택한 실행의 재현용 해시만 서버가 붙입니다. 활성 피드백 본문은 제출 후 최대 90일 보관하며 개별 삭제, 동의 철회 또는 계정 삭제 시 더 일찍 삭제됩니다. 사용 분석은 수집하지 않습니다.","policyKey":"development-feedback-consent-2026","retentionMaximumDays":90,"schemaVersion":2,"scope":"feedbackSubmission","version":2}',
    UTC_TIMESTAMP(6)
);

UPDATE playtest_consent_policy_assignment AS assignment
INNER JOIN playtest_consent_policy_version AS policy
    ON policy.scope = assignment.scope
   AND policy.policy_key = 'development-feedback-consent-2026'
   AND policy.version_no = 2
SET assignment.policy_version_id = policy.id,
    assignment.assignment_revision = assignment.assignment_revision + 1
WHERE assignment.scope = 'feedbackSubmission';

DROP TRIGGER tr_playtest_feedback_withdraw_only;

ALTER TABLE playtest_feedback
    DROP CHECK ck_playtest_feedback_active_shape,
    ADD CONSTRAINT ck_playtest_feedback_active_shape CHECK (
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
            status IN ('withdrawn', 'expired')
            AND category IS NULL
            AND severity IS NULL
            AND message IS NULL
            AND run_revision IS NULL
            AND run_manifest_sha256 IS NULL
            AND finalization_sha256 IS NULL
            AND withdrawn_at IS NOT NULL
        )
    );

CREATE TRIGGER tr_playtest_feedback_withdraw_only
BEFORE UPDATE ON playtest_feedback
FOR EACH ROW
SET NEW.status = IF(
    OLD.status = 'active'
        AND NEW.status IN ('withdrawn', 'expired')
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
