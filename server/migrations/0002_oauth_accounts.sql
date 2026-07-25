-- 계정을 OAuth 기반으로 바꾸고 세이브를 계정 소유로 만든다 (§4.5).
--
-- 계정의 정체성은 (provider, provider_user_id) 쌍이다. 이메일이 아니다 —
-- 같은 사람이 두 제공자로 들어오면 별개 계정이어야 하고, 제공자가 이메일을
-- 바꿔 주더라도 계정이 갈리면 안 되기 때문이다.

-- 비밀번호를 직접 보관하지 않는다
ALTER TABLE user DROP COLUMN password_hash;

ALTER TABLE user
    ADD COLUMN provider         VARCHAR(16) NOT NULL AFTER id,
    -- 제공자마다 형식이 다르다: DataGSM 은 숫자, Google 은 21자리 문자열(sub)
    ADD COLUMN provider_user_id VARCHAR(64) NOT NULL AFTER provider,
    -- 표시용. 제공자가 주지 않을 수도 있다
    ADD COLUMN display_name     VARCHAR(64)     NULL AFTER email;

-- 이메일 단독 유니크는 뗀다 — 같은 이메일이 제공자별로 하나씩 있을 수 있다
ALTER TABLE user DROP INDEX uk_user_email;
ALTER TABLE user ADD UNIQUE KEY uk_user_provider_identity (provider, provider_user_id);
ALTER TABLE user ADD KEY ix_user_email (email);

-- 로그인 이전에 만들어진 익명 세이브를 정리하고 계정 소유로 못 박는다.
-- character 는 save 를 CASCADE 로 참조하므로 함께 지워진다.
DELETE FROM save WHERE user_id IS NULL;
ALTER TABLE save MODIFY COLUMN user_id BIGINT UNSIGNED NOT NULL;
