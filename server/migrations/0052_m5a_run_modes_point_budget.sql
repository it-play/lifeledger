-- M5-A immutable run modes, character presets, and versioned point budgets (§2–§3).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;

CREATE TABLE character_preset_version (
    id                      BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    preset_key              VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    version_no              INT UNSIGNED NOT NULL,
    display_name            VARCHAR(80) NOT NULL,
    summary                 VARCHAR(255) NOT NULL,
    status                  VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ranked_eligible         BOOLEAN NOT NULL,
    canonical_draft_json    LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    canonical_sha256        CHAR(64) CHARACTER SET ascii COLLATE ascii_bin
        GENERATED ALWAYS AS (SHA2(canonical_draft_json, 256)) STORED,
    sealed_at               DATETIME(3) NULL,
    created_at              DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_character_preset_key_version (preset_key, version_no),
    UNIQUE KEY uk_character_preset_sha (canonical_sha256),
    CONSTRAINT ck_character_preset_key CHECK (
        preset_key REGEXP '^[a-z0-9][a-zA-Z0-9.-]{0,63}$'
    ),
    CONSTRAINT ck_character_preset_version CHECK (version_no > 0),
    CONSTRAINT ck_character_preset_status CHECK (status IN ('sealed', 'retired')),
    CONSTRAINT ck_character_preset_ranked CHECK (ranked_eligible IN (FALSE, TRUE)),
    CONSTRAINT ck_character_preset_draft CHECK (
        JSON_VALID(canonical_draft_json)
        AND JSON_TYPE(canonical_draft_json) = 'OBJECT'
    ),
    CONSTRAINT ck_character_preset_sealed CHECK (sealed_at IS NOT NULL)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_character_preset_retire_only
BEFORE UPDATE ON character_preset_version
FOR EACH ROW
SET NEW.preset_key = IF(
    OLD.status = 'sealed'
        AND NEW.status = 'retired'
        AND NEW.id = OLD.id
        AND BINARY NEW.preset_key = BINARY OLD.preset_key
        AND NEW.version_no = OLD.version_no
        AND BINARY NEW.display_name = BINARY OLD.display_name
        AND BINARY NEW.summary = BINARY OLD.summary
        AND NEW.ranked_eligible = OLD.ranked_eligible
        AND BINARY NEW.canonical_draft_json = BINARY OLD.canonical_draft_json
        AND NEW.sealed_at = OLD.sealed_at
        AND NEW.created_at = OLD.created_at,
    OLD.preset_key,
    NULL
);

CREATE TRIGGER tr_character_preset_no_delete
BEFORE DELETE ON character_preset_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'character preset versions are immutable';

INSERT INTO character_preset_version
    (preset_key, version_no, display_name, summary, status, ranked_eligible,
     canonical_draft_json, sealed_at)
VALUES
    (
        'rookie', 1, '사회초년생', '기본값. 균형 잡힌 출발', 'sealed', FALSE,
        '{"age":25,"background":"independent","careerYears":1,"certifications":1,"creditLoanKrw":0,"dependents":0,"education":"bachelor","gender":"other","health":"normal","military":"completed","name":"사회초년생","region":"capitalArea","startingCashKrw":10000000,"studentLoanKrw":20000000}',
        CURRENT_TIMESTAMP(3)
    ),
    (
        'early-start', 1, '이른 출발', '시간은 많고 자본은 없음', 'sealed', FALSE,
        '{"age":19,"background":"dependent","careerYears":0,"certifications":0,"creditLoanKrw":0,"dependents":1,"education":"highSchool","gender":"other","health":"good","military":"notServed","name":"이른 출발","region":"smallCity","startingCashKrw":2000000,"studentLoanKrw":0}',
        CURRENT_TIMESTAMP(3)
    ),
    (
        'late-start', 1, '늦은 출발', '자본은 있고 시간이 짧음', 'sealed', FALSE,
        '{"age":38,"background":"independent","careerYears":10,"certifications":2,"creditLoanKrw":30000000,"dependents":2,"education":"bachelor","gender":"other","health":"normal","military":"completed","name":"늦은 출발","region":"metropolitan","startingCashKrw":50000000,"studentLoanKrw":0}',
        CURRENT_TIMESTAMP(3)
    ),
    (
        'supported', 1, '지원 받는 출발', '쉬운 난이도. 세제 한도 최적화가 주 과제', 'sealed', FALSE,
        '{"age":25,"background":"supportive","careerYears":0,"certifications":1,"creditLoanKrw":0,"dependents":0,"education":"master","gender":"other","health":"good","military":"exempted","name":"지원 받는 출발","region":"capitalArea","startingCashKrw":300000000,"studentLoanKrw":0}',
        CURRENT_TIMESTAMP(3)
    ),
    (
        'restart', 1, '재기', '신용 제약 하에서의 복구 플레이', 'sealed', FALSE,
        '{"age":45,"background":"independent","careerYears":20,"certifications":0,"creditLoanKrw":0,"dependents":0,"education":"highSchool","gender":"other","health":"poor","military":"completed","name":"재기","region":"rural","startingCashKrw":0,"studentLoanKrw":0}',
        CURRENT_TIMESTAMP(3)
    );

CREATE TABLE point_budget_version (
    id                      BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    budget_key              VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    version_no              INT UNSIGNED NOT NULL,
    schema_version          INT UNSIGNED NOT NULL,
    display_name            VARCHAR(80) NOT NULL,
    description             VARCHAR(255) NOT NULL,
    total_points            BIGINT NOT NULL,
    status                  VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ranked_eligible         BOOLEAN NOT NULL,
    canonical_manifest_json LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    canonical_sha256        CHAR(64) CHARACTER SET ascii COLLATE ascii_bin
        GENERATED ALWAYS AS (SHA2(canonical_manifest_json, 256)) STORED,
    sealed_at               DATETIME(3) NULL,
    created_at              DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_point_budget_key_version (budget_key, version_no),
    UNIQUE KEY uk_point_budget_sha (canonical_sha256),
    CONSTRAINT ck_point_budget_key CHECK (
        budget_key REGEXP '^[a-z0-9][a-zA-Z0-9.-]{0,63}$'
    ),
    CONSTRAINT ck_point_budget_version CHECK (version_no > 0 AND schema_version = 1),
    CONSTRAINT ck_point_budget_total CHECK (total_points BETWEEN 0 AND 9007199254740991),
    CONSTRAINT ck_point_budget_status CHECK (status IN ('draft', 'sealed', 'retired')),
    CONSTRAINT ck_point_budget_ranked CHECK (ranked_eligible IN (FALSE, TRUE)),
    CONSTRAINT ck_point_budget_manifest CHECK (
        JSON_VALID(canonical_manifest_json)
        AND JSON_TYPE(canonical_manifest_json) = 'OBJECT'
    ),
    CONSTRAINT ck_point_budget_sealed CHECK (
        (status = 'draft' AND sealed_at IS NULL)
        OR (status IN ('sealed', 'retired') AND sealed_at IS NOT NULL)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE point_budget_exclusive_group (
    point_budget_version_id BIGINT UNSIGNED NOT NULL,
    group_key               VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name            VARCHAR(80) NOT NULL,
    display_order           SMALLINT UNSIGNED NOT NULL,
    created_at              DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (point_budget_version_id, group_key),
    UNIQUE KEY uk_point_budget_group_order (point_budget_version_id, display_order),
    CONSTRAINT fk_point_budget_group_version
        FOREIGN KEY (point_budget_version_id) REFERENCES point_budget_version (id),
    CONSTRAINT ck_point_budget_group_key CHECK (
        group_key REGEXP '^[a-z][a-zA-Z0-9]{0,63}$'
    ),
    CONSTRAINT ck_point_budget_group_order CHECK (display_order > 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE point_budget_option (
    id                      BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    point_budget_version_id BIGINT UNSIGNED NOT NULL,
    option_key              VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name            VARCHAR(80) NOT NULL,
    description             VARCHAR(255) NOT NULL,
    display_order           SMALLINT UNSIGNED NOT NULL,
    cost_kind               VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    point_delta_per_unit    BIGINT NULL,
    minimum_quantity        INT UNSIGNED NOT NULL,
    maximum_quantity        INT UNSIGNED NOT NULL,
    exclusive_group_key     VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    effect_json             JSON NOT NULL,
    created_at              DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_point_budget_option_key (point_budget_version_id, option_key),
    UNIQUE KEY uk_point_budget_option_order (point_budget_version_id, display_order),
    UNIQUE KEY uk_point_budget_option_version_id (point_budget_version_id, id),
    CONSTRAINT fk_point_budget_option_version
        FOREIGN KEY (point_budget_version_id) REFERENCES point_budget_version (id),
    CONSTRAINT fk_point_budget_option_group
        FOREIGN KEY (point_budget_version_id, exclusive_group_key)
        REFERENCES point_budget_exclusive_group (point_budget_version_id, group_key),
    CONSTRAINT ck_point_budget_option_key CHECK (
        option_key REGEXP '^[a-z][a-zA-Z0-9]{0,63}$'
    ),
    CONSTRAINT ck_point_budget_option_order CHECK (display_order > 0),
    CONSTRAINT ck_point_budget_option_cost CHECK (
        (cost_kind IN ('fixed', 'perUnit') AND point_delta_per_unit IS NOT NULL)
        OR (cost_kind = 'tiered' AND point_delta_per_unit IS NULL)
    ),
    CONSTRAINT ck_point_budget_option_quantity CHECK (
        minimum_quantity > 0
        AND maximum_quantity >= minimum_quantity
        AND maximum_quantity <= 1000000
        AND (cost_kind <> 'fixed' OR (minimum_quantity = 1 AND maximum_quantity = 1))
    ),
    CONSTRAINT ck_point_budget_option_effect CHECK (JSON_TYPE(effect_json) = 'OBJECT')
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE point_budget_option_tier (
    point_budget_option_id  BIGINT UNSIGNED NOT NULL,
    tier_order              SMALLINT UNSIGNED NOT NULL,
    minimum_quantity        INT UNSIGNED NOT NULL,
    maximum_quantity        INT UNSIGNED NOT NULL,
    point_delta_per_unit    BIGINT NOT NULL,
    created_at              DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (point_budget_option_id, tier_order),
    UNIQUE KEY uk_point_budget_tier_minimum (point_budget_option_id, minimum_quantity),
    CONSTRAINT fk_point_budget_tier_option
        FOREIGN KEY (point_budget_option_id) REFERENCES point_budget_option (id),
    CONSTRAINT ck_point_budget_tier CHECK (
        tier_order > 0
        AND minimum_quantity > 0
        AND maximum_quantity >= minimum_quantity
        AND maximum_quantity <= 1000000
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE point_budget_option_condition (
    id                      BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    point_budget_version_id BIGINT UNSIGNED NOT NULL,
    point_budget_option_id  BIGINT UNSIGNED NOT NULL,
    condition_order         SMALLINT UNSIGNED NOT NULL,
    condition_kind          VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    related_option_id       BIGINT UNSIGNED NULL,
    fact_path               VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    comparison_kind         VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL,
    fact_value_kind         VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL,
    fact_integer_value      BIGINT NULL,
    fact_text_value         VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    created_at              DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_point_budget_condition_order (point_budget_option_id, condition_order),
    CONSTRAINT fk_point_budget_condition_option
        FOREIGN KEY (point_budget_version_id, point_budget_option_id)
        REFERENCES point_budget_option (point_budget_version_id, id),
    CONSTRAINT fk_point_budget_condition_related
        FOREIGN KEY (point_budget_version_id, related_option_id)
        REFERENCES point_budget_option (point_budget_version_id, id),
    CONSTRAINT ck_point_budget_condition_order CHECK (condition_order > 0),
    CONSTRAINT ck_point_budget_condition_shape CHECK (
        (
            condition_kind IN ('requiresOption', 'forbidsOption')
            AND related_option_id IS NOT NULL
            AND fact_path IS NULL
            AND comparison_kind IS NULL
            AND fact_value_kind IS NULL
            AND fact_integer_value IS NULL
            AND fact_text_value IS NULL
        )
        OR (
            condition_kind IN ('requiresFact', 'forbidsFact')
            AND related_option_id IS NULL
            AND fact_path REGEXP '^[a-z][a-zA-Z0-9]{0,63}$'
            AND comparison_kind IN ('equal', 'greaterOrEqual', 'lessOrEqual')
            AND (
                (fact_value_kind = 'integer' AND fact_integer_value IS NOT NULL
                    AND fact_text_value IS NULL)
                OR (fact_value_kind = 'text' AND fact_integer_value IS NULL
                    AND fact_text_value IS NOT NULL AND comparison_kind = 'equal')
            )
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_point_budget_version_seal_only
BEFORE UPDATE ON point_budget_version
FOR EACH ROW
SET NEW.budget_key = IF(
    (
        OLD.status = 'draft'
        AND NEW.status = 'sealed'
        AND NEW.id = OLD.id
        AND BINARY NEW.budget_key = BINARY OLD.budget_key
        AND NEW.version_no = OLD.version_no
        AND NEW.schema_version = OLD.schema_version
        AND BINARY NEW.display_name = BINARY OLD.display_name
        AND BINARY NEW.description = BINARY OLD.description
        AND NEW.total_points = OLD.total_points
        AND NEW.ranked_eligible = OLD.ranked_eligible
        AND BINARY NEW.canonical_manifest_json = BINARY OLD.canonical_manifest_json
        AND NEW.created_at = OLD.created_at
        AND NEW.sealed_at IS NOT NULL
        AND EXISTS (
            SELECT 1 FROM point_budget_option
            WHERE point_budget_version_id = OLD.id
        )
        AND NOT EXISTS (
            SELECT 1
            FROM point_budget_option AS option_row
            WHERE option_row.point_budget_version_id = OLD.id
              AND option_row.cost_kind = 'tiered'
              AND NOT EXISTS (
                  SELECT 1 FROM point_budget_option_tier AS tier
                  WHERE tier.point_budget_option_id = option_row.id
              )
        ),
    NEW.budget_key,
    IF(
        OLD.status = 'sealed'
            AND NEW.status = 'retired'
            AND NEW.id = OLD.id
            AND BINARY NEW.budget_key = BINARY OLD.budget_key
            AND NEW.version_no = OLD.version_no
            AND NEW.schema_version = OLD.schema_version
            AND BINARY NEW.display_name = BINARY OLD.display_name
            AND BINARY NEW.description = BINARY OLD.description
            AND NEW.total_points = OLD.total_points
            AND NEW.ranked_eligible = OLD.ranked_eligible
            AND BINARY NEW.canonical_manifest_json = BINARY OLD.canonical_manifest_json
            AND NEW.sealed_at = OLD.sealed_at
            AND NEW.created_at = OLD.created_at,
        NEW.budget_key,
        NULL
    )
);

CREATE TRIGGER tr_point_budget_version_no_delete
BEFORE DELETE ON point_budget_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'point budget versions are immutable';

CREATE TRIGGER tr_point_budget_group_draft_insert
BEFORE INSERT ON point_budget_exclusive_group
FOR EACH ROW
SET NEW.point_budget_version_id = IF(
    EXISTS (
        SELECT 1 FROM point_budget_version
        WHERE id = NEW.point_budget_version_id AND status = 'draft'
    ),
    NEW.point_budget_version_id,
    NULL
);

CREATE TRIGGER tr_point_budget_option_draft_insert
BEFORE INSERT ON point_budget_option
FOR EACH ROW
SET NEW.point_budget_version_id = IF(
    EXISTS (
        SELECT 1 FROM point_budget_version
        WHERE id = NEW.point_budget_version_id AND status = 'draft'
    ),
    NEW.point_budget_version_id,
    NULL
);

CREATE TRIGGER tr_point_budget_tier_draft_insert
BEFORE INSERT ON point_budget_option_tier
FOR EACH ROW
SET NEW.point_budget_option_id = IF(
    EXISTS (
        SELECT 1
        FROM point_budget_option AS option_row
        INNER JOIN point_budget_version AS version_row
            ON version_row.id = option_row.point_budget_version_id
        WHERE option_row.id = NEW.point_budget_option_id
          AND option_row.cost_kind = 'tiered'
          AND version_row.status = 'draft'
    ),
    NEW.point_budget_option_id,
    NULL
);

CREATE TRIGGER tr_point_budget_condition_draft_insert
BEFORE INSERT ON point_budget_option_condition
FOR EACH ROW
SET NEW.point_budget_version_id = IF(
    EXISTS (
        SELECT 1 FROM point_budget_version
        WHERE id = NEW.point_budget_version_id AND status = 'draft'
    ),
    NEW.point_budget_version_id,
    NULL
);

CREATE TRIGGER tr_point_budget_group_no_update
BEFORE UPDATE ON point_budget_exclusive_group
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'point budget groups are immutable';

CREATE TRIGGER tr_point_budget_group_no_delete
BEFORE DELETE ON point_budget_exclusive_group
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'point budget groups are immutable';

CREATE TRIGGER tr_point_budget_option_no_update
BEFORE UPDATE ON point_budget_option
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'point budget options are immutable';

CREATE TRIGGER tr_point_budget_option_no_delete
BEFORE DELETE ON point_budget_option
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'point budget options are immutable';

CREATE TRIGGER tr_point_budget_tier_no_update
BEFORE UPDATE ON point_budget_option_tier
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'point budget tiers are immutable';

CREATE TRIGGER tr_point_budget_tier_no_delete
BEFORE DELETE ON point_budget_option_tier
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'point budget tiers are immutable';

CREATE TRIGGER tr_point_budget_condition_no_update
BEFORE UPDATE ON point_budget_option_condition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'point budget conditions are immutable';

CREATE TRIGGER tr_point_budget_condition_no_delete
BEFORE DELETE ON point_budget_option_condition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'point budget conditions are immutable';

INSERT INTO point_budget_version
    (budget_key, version_no, schema_version, display_name, description, total_points,
     status, ranked_eligible, canonical_manifest_json)
VALUES (
    'dev-unranked-custom-2026', 1, 1, '개발 커스텀 예산',
    'M5-A 기능 검증용 조정 전 예산', 100, 'draft', FALSE,
    '{"budgetKey":"dev-unranked-custom-2026","costKinds":["fixed","perUnit","tiered"],"schemaVersion":1,"totalPoints":100,"version":1}'
);

INSERT INTO point_budget_exclusive_group
    (point_budget_version_id, group_key, display_name, display_order)
SELECT id, group_seed.group_key, group_seed.display_name, group_seed.display_order
FROM point_budget_version
INNER JOIN (
    SELECT 'education' AS group_key, '학력' AS display_name, 1 AS display_order
    UNION ALL SELECT 'startingCash', '시작 자금', 2
    UNION ALL SELECT 'health', '건강', 3
    UNION ALL SELECT 'background', '가정 배경', 4
    UNION ALL SELECT 'certifications', '자격증', 5
) AS group_seed
WHERE budget_key = 'dev-unranked-custom-2026' AND version_no = 1;

INSERT INTO point_budget_option
    (point_budget_version_id, option_key, display_name, description, display_order,
     cost_kind, point_delta_per_unit, minimum_quantity, maximum_quantity,
     exclusive_group_key, effect_json)
SELECT version_row.id, option_seed.option_key, option_seed.display_name,
       option_seed.description, option_seed.display_order, option_seed.cost_kind,
       option_seed.point_delta_per_unit, option_seed.minimum_quantity,
       option_seed.maximum_quantity, option_seed.exclusive_group_key,
       CAST(option_seed.effect_json AS JSON)
FROM point_budget_version AS version_row
INNER JOIN (
    SELECT 'educationHighSchool' AS option_key, '고졸' AS display_name,
           '고등학교 졸업으로 시작' AS description, 1 AS display_order,
           'fixed' AS cost_kind, 0 AS point_delta_per_unit,
           1 AS minimum_quantity, 1 AS maximum_quantity, 'education' AS exclusive_group_key,
           '{"kind":"setText","factPath":"education","value":"highSchool"}' AS effect_json
    UNION ALL SELECT 'educationBachelor', '학사', '학사 학위로 시작', 2,
           'fixed', 20, 1, 1, 'education',
           '{"kind":"setText","factPath":"education","value":"bachelor"}'
    UNION ALL SELECT 'educationMaster', '석사', '석사 학위로 시작', 3,
           'fixed', 35, 1, 1, 'education',
           '{"kind":"setText","factPath":"education","value":"master"}'
    UNION ALL SELECT 'startingCashPerMillion', '시작 자금 100만원',
           '수량마다 시작 자금 100만원', 4,
           'perUnit', 1, 1, 300, 'startingCash',
           '{"kind":"incrementInteger","factPath":"startingCashKrw","valuePerUnit":1000000}'
    UNION ALL SELECT 'healthGood', '건강 상', '건강 상태 상으로 시작', 5,
           'fixed', 10, 1, 1, 'health',
           '{"kind":"setText","factPath":"health","value":"good"}'
    UNION ALL SELECT 'healthNormal', '건강 중', '건강 상태 중으로 시작', 6,
           'fixed', 0, 1, 1, 'health',
           '{"kind":"setText","factPath":"health","value":"normal"}'
    UNION ALL SELECT 'healthPoor', '건강 하', '건강 상태 하로 시작', 7,
           'fixed', -10, 1, 1, 'health',
           '{"kind":"setText","factPath":"health","value":"poor"}'
    UNION ALL SELECT 'backgroundSupportive', '지원형', '가족 지원을 받는 배경', 8,
           'fixed', 20, 1, 1, 'background',
           '{"kind":"setText","factPath":"background","value":"supportive"}'
    UNION ALL SELECT 'backgroundIndependent', '독립형', '독립적인 가정 배경', 9,
           'fixed', 0, 1, 1, 'background',
           '{"kind":"setText","factPath":"background","value":"independent"}'
    UNION ALL SELECT 'backgroundDependent', '부양형', '가족 부양 의무가 있는 배경', 10,
           'fixed', -15, 1, 1, 'background',
           '{"kind":"setText","factPath":"background","value":"dependent"}'
    UNION ALL SELECT 'certificationsNone', '자격증 없음', '자격증 없이 시작', 11,
           'fixed', 0, 1, 1, 'certifications',
           '{"kind":"setInteger","factPath":"certifications","value":0}'
    UNION ALL SELECT 'certificationCount', '자격증 보유', '수량만큼 자격증을 보유', 12,
           'tiered', NULL, 1, 10, 'certifications',
           '{"kind":"incrementInteger","factPath":"certifications","valuePerUnit":1}'
) AS option_seed
WHERE version_row.budget_key = 'dev-unranked-custom-2026'
  AND version_row.version_no = 1;

INSERT INTO point_budget_option_tier
    (point_budget_option_id, tier_order, minimum_quantity, maximum_quantity,
     point_delta_per_unit)
SELECT option_row.id, tier_seed.tier_order, tier_seed.minimum_quantity,
       tier_seed.maximum_quantity, tier_seed.point_delta_per_unit
FROM point_budget_option AS option_row
INNER JOIN point_budget_version AS version_row
    ON version_row.id = option_row.point_budget_version_id
INNER JOIN (
    SELECT 1 AS tier_order, 1 AS minimum_quantity, 2 AS maximum_quantity,
           3 AS point_delta_per_unit
    UNION ALL SELECT 2, 3, 5, 5
    UNION ALL SELECT 3, 6, 10, 8
) AS tier_seed
WHERE version_row.budget_key = 'dev-unranked-custom-2026'
  AND version_row.version_no = 1
  AND option_row.option_key = 'certificationCount';

UPDATE point_budget_version
SET status = 'sealed', sealed_at = CURRENT_TIMESTAMP(3)
WHERE budget_key = 'dev-unranked-custom-2026' AND version_no = 1;

CREATE TABLE point_budget_assignment (
    assignment_key          VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    point_budget_version_id BIGINT UNSIGNED NOT NULL,
    assignment_revision     BIGINT UNSIGNED NOT NULL,
    updated_at              DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (assignment_key),
    CONSTRAINT fk_point_budget_assignment_version
        FOREIGN KEY (point_budget_version_id) REFERENCES point_budget_version (id),
    CONSTRAINT ck_point_budget_assignment CHECK (
        assignment_key = 'newRun' AND assignment_revision > 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

INSERT INTO point_budget_assignment
    (assignment_key, point_budget_version_id, assignment_revision)
SELECT 'newRun', id, 1
FROM point_budget_version
WHERE budget_key = 'dev-unranked-custom-2026' AND version_no = 1;

CREATE TABLE run_manifest (
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    mode                            VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    season_id                       BIGINT UNSIGNED NULL,
    league_definition_id            BIGINT UNSIGNED NULL,
    market_world_id                 BIGINT UNSIGNED NOT NULL,
    policy_set_id                   BIGINT UNSIGNED NOT NULL,
    career_catalog_bundle_id        BIGINT UNSIGNED NOT NULL,
    employment_policy_set_id        BIGINT UNSIGNED NOT NULL,
    life_catalog_set_id             BIGINT UNSIGNED NOT NULL,
    credit_model_version_id         BIGINT UNSIGNED NOT NULL,
    real_estate_model_version_id    BIGINT UNSIGNED NOT NULL,
    content_bundle_id               BIGINT UNSIGNED NULL,
    character_preset_version_id     BIGINT UNSIGNED NULL,
    point_budget_version_id         BIGINT UNSIGNED NULL,
    canonical_selections_json       JSON NOT NULL,
    engine_version                  VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    offline_policy_version_id       BIGINT UNSIGNED NULL,
    start_game_day                  INT UNSIGNED NOT NULL,
    target_game_day                 INT UNSIGNED NULL,
    ranking_eligible                BOOLEAN NOT NULL,
    ranking_ineligibility_reason    VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    manifest_canonical_json         LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    manifest_sha256                 CHAR(64) CHARACTER SET ascii COLLATE ascii_bin
        GENERATED ALWAYS AS (SHA2(manifest_canonical_json, 256)) STORED,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, run_revision),
    KEY ix_run_manifest_mode (mode, created_at),
    KEY ix_run_manifest_preset (character_preset_version_id),
    KEY ix_run_manifest_budget (point_budget_version_id),
    CONSTRAINT fk_run_manifest_rule_bundle
        FOREIGN KEY (save_id, run_revision)
        REFERENCES run_rule_bundle (save_id, run_revision) ON DELETE CASCADE,
    CONSTRAINT fk_run_manifest_preset
        FOREIGN KEY (character_preset_version_id) REFERENCES character_preset_version (id),
    CONSTRAINT fk_run_manifest_budget
        FOREIGN KEY (point_budget_version_id) REFERENCES point_budget_version (id),
    CONSTRAINT ck_run_manifest_mode CHECK (
        mode IN ('rankedPreset', 'rankedCustom', 'sandbox')
    ),
    CONSTRAINT ck_run_manifest_mode_shape CHECK (
        (
            mode = 'rankedPreset'
            AND season_id IS NOT NULL
            AND league_definition_id IS NOT NULL
            AND character_preset_version_id IS NOT NULL
            AND point_budget_version_id IS NULL
        )
        OR (
            mode = 'rankedCustom'
            AND season_id IS NOT NULL
            AND league_definition_id IS NOT NULL
            AND character_preset_version_id IS NULL
            AND point_budget_version_id IS NOT NULL
        )
        OR (
            mode = 'sandbox'
            AND season_id IS NULL
            AND league_definition_id IS NULL
        )
    ),
    CONSTRAINT ck_run_manifest_selection_json CHECK (
        JSON_TYPE(canonical_selections_json) = 'ARRAY'
    ),
    CONSTRAINT ck_run_manifest_days CHECK (
        target_game_day IS NULL OR target_game_day >= start_game_day
    ),
    CONSTRAINT ck_run_manifest_ranking CHECK (
        ranking_eligible IN (FALSE, TRUE)
        AND (ranking_eligible = FALSE OR mode IN ('rankedPreset', 'rankedCustom'))
        AND (
            (ranking_eligible = TRUE AND ranking_ineligibility_reason IS NULL)
            OR (ranking_eligible = FALSE AND ranking_ineligibility_reason IS NOT NULL)
        )
    ),
    CONSTRAINT ck_run_manifest_json CHECK (
        JSON_VALID(manifest_canonical_json)
        AND JSON_TYPE(manifest_canonical_json) = 'OBJECT'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

INSERT INTO run_manifest
    (save_id, run_revision, mode, market_world_id, policy_set_id,
     career_catalog_bundle_id, employment_policy_set_id, life_catalog_set_id,
     credit_model_version_id, real_estate_model_version_id,
     canonical_selections_json, engine_version, start_game_day,
     ranking_eligible, ranking_ineligibility_reason, manifest_canonical_json)
SELECT bundle.save_id, bundle.run_revision, 'sandbox', bundle.market_world_id,
       bundle.policy_set_id, bundle.career_catalog_bundle_id,
       bundle.employment_policy_set_id, bundle.life_catalog_set_id,
       bundle.credit_model_version_id, bundle.real_estate_model_version_id,
       JSON_ARRAY(), 'legacy-pre-m5', 0, FALSE, 'legacyRun',
       CAST(JSON_OBJECT(
           'careerCatalogBundleId', CAST(bundle.career_catalog_bundle_id AS CHAR),
           'creditModelVersionId', CAST(bundle.credit_model_version_id AS CHAR),
           'employmentPolicySetId', CAST(bundle.employment_policy_set_id AS CHAR),
           'engineVersion', 'legacy-pre-m5',
           'lifeCatalogSetId', CAST(bundle.life_catalog_set_id AS CHAR),
           'marketWorldId', CAST(bundle.market_world_id AS CHAR),
           'mode', 'sandbox',
           'policySetId', CAST(bundle.policy_set_id AS CHAR),
           'rankingEligible', FALSE,
           'rankingIneligibilityReason', 'legacyRun',
           'realEstateModelVersionId', CAST(bundle.real_estate_model_version_id AS CHAR),
           'runRevision', bundle.run_revision,
           'saveId', CAST(bundle.save_id AS CHAR),
           'schemaVersion', 1,
           'selections', JSON_ARRAY(),
           'startGameDay', 0
       ) AS CHAR CHARACTER SET utf8mb4)
FROM run_rule_bundle AS bundle;

CREATE TRIGGER tr_run_manifest_no_update
BEFORE UPDATE ON run_manifest
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'run manifests are immutable';

CREATE TRIGGER tr_run_manifest_valid_insert
BEFORE INSERT ON run_manifest
FOR EACH ROW
SET NEW.mode = IF(
    EXISTS (
        SELECT 1
        FROM run_rule_bundle AS bundle
        WHERE bundle.save_id = NEW.save_id
          AND bundle.run_revision = NEW.run_revision
          AND bundle.market_world_id = NEW.market_world_id
          AND bundle.policy_set_id = NEW.policy_set_id
          AND bundle.career_catalog_bundle_id = NEW.career_catalog_bundle_id
          AND bundle.employment_policy_set_id = NEW.employment_policy_set_id
          AND bundle.life_catalog_set_id = NEW.life_catalog_set_id
          AND bundle.credit_model_version_id = NEW.credit_model_version_id
          AND bundle.real_estate_model_version_id = NEW.real_estate_model_version_id
    ),
    NEW.mode,
    NULL
);

CREATE TRIGGER tr_run_manifest_no_delete
BEFORE DELETE ON run_manifest
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'run manifests are immutable';
