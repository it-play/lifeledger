-- 첫 스키마. 설계 근거는 plan-docs/development-plan.md §4.3, §4.4.
--
-- 규칙
--   · 금액은 BIGINT 원 단위 정수 (DECIMAL·부동소수점 금지)
--   · 열거형은 VARCHAR 로 두고 Rust enum 이 검증한다
--   · 시간 컬럼은 UTC DATETIME

CREATE TABLE user (
    id              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    email           VARCHAR(254)    NOT NULL,
    -- Argon2id PHC 문자열. 파라미터가 바뀌어도 길이가 넉넉하도록 잡는다
    password_hash   VARCHAR(255)    NOT NULL,
    created_at      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_user_email (email)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE session (
    id              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    user_id         BIGINT UNSIGNED NOT NULL,
    -- 쿠키에 담긴 토큰의 SHA-256. 원문은 저장하지 않는다
    token_hash      CHAR(64)        NOT NULL,
    expires_at      DATETIME(3)     NOT NULL,
    created_at      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_session_token_hash (token_hash),
    KEY ix_session_user_id (user_id),
    KEY ix_session_expires_at (expires_at),
    CONSTRAINT fk_session_user FOREIGN KEY (user_id) REFERENCES user (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE save (
    id              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    -- 인증이 붙기 전까지 NULL. 계정이 생기면 채워지고 그때 다중 세이브가 열린다 (§4.4)
    user_id         BIGINT UNSIGNED     NULL,
    -- 시작일로부터 며칠 지났는지. 표시용 날짜는 클라이언트가 계산한다 (§4.2)
    game_day        INT UNSIGNED    NOT NULL DEFAULT 0,
    cash_krw        BIGINT          NOT NULL,
    debt_krw        BIGINT          NOT NULL DEFAULT 0,
    created_at      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    KEY ix_save_user_id (user_id),
    CONSTRAINT fk_save_user FOREIGN KEY (user_id) REFERENCES user (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE `character` (
    save_id         BIGINT UNSIGNED NOT NULL,
    name            VARCHAR(20)     NOT NULL,
    age             INT UNSIGNED    NOT NULL,
    gender          VARCHAR(16)     NOT NULL,
    military        VARCHAR(16)     NOT NULL,
    region          VARCHAR(16)     NOT NULL,
    background      VARCHAR(16)     NOT NULL,
    education       VARCHAR(16)     NOT NULL,
    career_years    INT UNSIGNED    NOT NULL,
    certifications  INT UNSIGNED    NOT NULL,
    health          VARCHAR(16)     NOT NULL,
    dependents      INT UNSIGNED    NOT NULL,
    created_at      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    -- 세이브당 캐릭터 하나 (1:1)
    PRIMARY KEY (save_id),
    CONSTRAINT fk_character_save FOREIGN KEY (save_id) REFERENCES save (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;
