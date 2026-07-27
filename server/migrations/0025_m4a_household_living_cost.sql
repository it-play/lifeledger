-- M4-A typed living-cost catalog, household ownership, and monthly obligation state (§3).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;
SET SESSION group_concat_max_len = 1048576;

CREATE TABLE life_region (
    region_key          VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name        VARCHAR(64)     NOT NULL,
    region_order        TINYINT UNSIGNED NOT NULL,
    created_at          DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (region_key),
    UNIQUE KEY uk_life_region_order (region_order),
    CONSTRAINT ck_life_region_key CHECK (
        region_key IN ('capitalArea', 'metropolitan', 'smallCity', 'rural')
    ),
    CONSTRAINT ck_life_region_order CHECK (region_order BETWEEN 1 AND 4),
    CONSTRAINT ck_life_region_name CHECK (CHAR_LENGTH(display_name) > 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE cost_of_living_profile (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    life_component_version_id       BIGINT UNSIGNED NOT NULL,
    profile_key                     VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    base_year_month                 DATE            NOT NULL,
    base_cpi_index                  BIGINT UNSIGNED NOT NULL,
    proration_scale                 INT UNSIGNED    NOT NULL,
    legacy_dependent_age_years      TINYINT UNSIGNED NOT NULL,
    created_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_cost_of_living_profile_component (life_component_version_id),
    UNIQUE KEY uk_cost_of_living_profile_key (profile_key),
    UNIQUE KEY uk_cost_of_living_profile_component_id (life_component_version_id, id),
    CONSTRAINT fk_cost_of_living_profile_component
        FOREIGN KEY (life_component_version_id) REFERENCES life_component_version (id),
    CONSTRAINT ck_cost_of_living_profile_key CHECK (
        profile_key REGEXP '^[a-z0-9][a-zA-Z0-9._-]{0,95}$'
    ),
    CONSTRAINT ck_cost_of_living_profile_month CHECK (DAY(base_year_month) = 1),
    CONSTRAINT ck_cost_of_living_profile_cpi CHECK (base_cpi_index > 0),
    CONSTRAINT ck_cost_of_living_profile_proration CHECK (proration_scale = 377580),
    CONSTRAINT ck_cost_of_living_profile_legacy_age CHECK (
        legacy_dependent_age_years BETWEEN 0 AND 120
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE living_cost_budget_band (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    cost_of_living_profile_id       BIGINT UNSIGNED NOT NULL,
    band_key                        VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                    VARCHAR(64)     NOT NULL,
    band_order                      TINYINT UNSIGNED NOT NULL,
    factor_ppm                      INT UNSIGNED    NOT NULL,
    created_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_living_cost_budget_band_key (cost_of_living_profile_id, band_key),
    UNIQUE KEY uk_living_cost_budget_band_order (cost_of_living_profile_id, band_order),
    UNIQUE KEY uk_living_cost_budget_band_profile_id (cost_of_living_profile_id, id),
    CONSTRAINT fk_living_cost_budget_band_profile
        FOREIGN KEY (cost_of_living_profile_id) REFERENCES cost_of_living_profile (id),
    CONSTRAINT ck_living_cost_budget_band_key CHECK (
        band_key IN ('frugal', 'standard', 'generous')
    ),
    CONSTRAINT ck_living_cost_budget_band_order CHECK (band_order BETWEEN 1 AND 3),
    CONSTRAINT ck_living_cost_budget_band_factor CHECK (factor_ppm > 0),
    CONSTRAINT ck_living_cost_budget_band_name CHECK (CHAR_LENGTH(display_name) > 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE living_cost_category (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    cost_of_living_profile_id       BIGINT UNSIGNED NOT NULL,
    category_key                    VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                    VARCHAR(64)     NOT NULL,
    category_order                  TINYINT UNSIGNED NOT NULL,
    base_amount_krw                 BIGINT          NOT NULL,
    essential                       BOOLEAN         NOT NULL,
    shortage_treatment              VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    default_budget_band_id          BIGINT UNSIGNED NOT NULL,
    created_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_living_cost_category_key (cost_of_living_profile_id, category_key),
    UNIQUE KEY uk_living_cost_category_order (cost_of_living_profile_id, category_order),
    UNIQUE KEY uk_living_cost_category_profile_id (cost_of_living_profile_id, id),
    KEY ix_living_cost_category_default_band
        (cost_of_living_profile_id, default_budget_band_id),
    CONSTRAINT fk_living_cost_category_profile
        FOREIGN KEY (cost_of_living_profile_id) REFERENCES cost_of_living_profile (id),
    CONSTRAINT fk_living_cost_category_default_band
        FOREIGN KEY (cost_of_living_profile_id, default_budget_band_id)
        REFERENCES living_cost_budget_band (cost_of_living_profile_id, id),
    CONSTRAINT ck_living_cost_category_key CHECK (
        category_key IN (
            'housing', 'food', 'transport', 'communication', 'utilities',
            'healthcare', 'education', 'dependentCare', 'discretionary'
        )
    ),
    CONSTRAINT ck_living_cost_category_order CHECK (category_order BETWEEN 1 AND 9),
    CONSTRAINT ck_living_cost_category_amount CHECK (base_amount_krw >= 0),
    CONSTRAINT ck_living_cost_category_essential CHECK (essential IN (FALSE, TRUE)),
    CONSTRAINT ck_living_cost_category_shortage CHECK (
        (essential = TRUE AND shortage_treatment = 'essentialArrear')
        OR (essential = FALSE AND shortage_treatment = 'reduceToCash')
    ),
    CONSTRAINT ck_living_cost_category_name CHECK (CHAR_LENGTH(display_name) > 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE living_cost_region_factor (
    cost_of_living_profile_id       BIGINT UNSIGNED NOT NULL,
    living_cost_category_id         BIGINT UNSIGNED NOT NULL,
    region_key                      VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    factor_ppm                      INT UNSIGNED    NOT NULL,
    created_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (living_cost_category_id, region_key),
    KEY ix_living_cost_region_factor_profile_region
        (cost_of_living_profile_id, region_key, living_cost_category_id),
    CONSTRAINT fk_living_cost_region_factor_category
        FOREIGN KEY (cost_of_living_profile_id, living_cost_category_id)
        REFERENCES living_cost_category (cost_of_living_profile_id, id),
    CONSTRAINT fk_living_cost_region_factor_region
        FOREIGN KEY (region_key) REFERENCES life_region (region_key),
    CONSTRAINT ck_living_cost_region_factor_value CHECK (factor_ppm > 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE living_cost_region_bridge (
    cost_of_living_profile_id       BIGINT UNSIGNED NOT NULL,
    character_region                VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    region_key                      VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (cost_of_living_profile_id, character_region),
    KEY ix_living_cost_region_bridge_region (region_key),
    CONSTRAINT fk_living_cost_region_bridge_profile
        FOREIGN KEY (cost_of_living_profile_id) REFERENCES cost_of_living_profile (id),
    CONSTRAINT fk_living_cost_region_bridge_region
        FOREIGN KEY (region_key) REFERENCES life_region (region_key),
    CONSTRAINT ck_living_cost_region_bridge_character CHECK (
        character_region IN ('capitalArea', 'metropolitan', 'smallCity', 'rural')
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE living_cost_member_age_band (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    cost_of_living_profile_id       BIGINT UNSIGNED NOT NULL,
    member_role                     VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    age_band_key                    VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    minimum_age_years               TINYINT UNSIGNED NOT NULL,
    maximum_age_years_exclusive     TINYINT UNSIGNED NULL,
    created_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_living_cost_member_age_band_key
        (cost_of_living_profile_id, member_role, age_band_key),
    UNIQUE KEY uk_living_cost_member_age_band_profile_id
        (cost_of_living_profile_id, id),
    CONSTRAINT fk_living_cost_member_age_band_profile
        FOREIGN KEY (cost_of_living_profile_id) REFERENCES cost_of_living_profile (id),
    CONSTRAINT ck_living_cost_member_age_band_role CHECK (
        member_role IN ('dependent', 'partner', 'child', 'parent')
    ),
    CONSTRAINT ck_living_cost_member_age_band_period CHECK (
        maximum_age_years_exclusive IS NULL
        OR maximum_age_years_exclusive > minimum_age_years
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE living_cost_member_factor (
    cost_of_living_profile_id       BIGINT UNSIGNED NOT NULL,
    living_cost_member_age_band_id  BIGINT UNSIGNED NOT NULL,
    living_cost_category_id         BIGINT UNSIGNED NOT NULL,
    marginal_factor_ppm             INT UNSIGNED    NOT NULL,
    created_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (living_cost_member_age_band_id, living_cost_category_id),
    KEY ix_living_cost_member_factor_profile_category
        (cost_of_living_profile_id, living_cost_category_id),
    CONSTRAINT fk_living_cost_member_factor_age_band
        FOREIGN KEY (cost_of_living_profile_id, living_cost_member_age_band_id)
        REFERENCES living_cost_member_age_band (cost_of_living_profile_id, id),
    CONSTRAINT fk_living_cost_member_factor_category
        FOREIGN KEY (cost_of_living_profile_id, living_cost_category_id)
        REFERENCES living_cost_category (cost_of_living_profile_id, id),
    CONSTRAINT ck_living_cost_member_factor_value CHECK (marginal_factor_ppm >= 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE living_cost_tenure_factor (
    cost_of_living_profile_id       BIGINT UNSIGNED NOT NULL,
    tenure_type                     VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    housing_replacement_factor_ppm  INT UNSIGNED    NOT NULL,
    created_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (cost_of_living_profile_id, tenure_type),
    CONSTRAINT fk_living_cost_tenure_factor_profile
        FOREIGN KEY (cost_of_living_profile_id) REFERENCES cost_of_living_profile (id),
    CONSTRAINT ck_living_cost_tenure_factor_type CHECK (
        tenure_type IN ('owner', 'jeonse', 'monthlyRent', 'rentFree')
    ),
    CONSTRAINT ck_living_cost_tenure_factor_value CHECK (
        housing_replacement_factor_ppm BETWEEN 0 AND 1000000
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

-- Every typed child is append-only and can only be added to the one draft living-cost component.
CREATE TRIGGER tr_cost_of_living_profile_draft_insert
BEFORE INSERT ON cost_of_living_profile
FOR EACH ROW
SET NEW.life_component_version_id = IF(
    EXISTS (
        SELECT 1 FROM life_component_version
        WHERE id = NEW.life_component_version_id
          AND component_kind = 'livingCost'
          AND availability = 'active'
          AND sealed_at IS NULL
    ),
    NEW.life_component_version_id,
    NULL
);

CREATE TRIGGER tr_cost_of_living_profile_no_update
BEFORE UPDATE ON cost_of_living_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'cost-of-living profiles are immutable';

CREATE TRIGGER tr_cost_of_living_profile_no_delete
BEFORE DELETE ON cost_of_living_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'cost-of-living profiles are immutable';

CREATE TRIGGER tr_living_cost_budget_band_draft_insert
BEFORE INSERT ON living_cost_budget_band
FOR EACH ROW
SET NEW.cost_of_living_profile_id = IF(
    EXISTS (
        SELECT 1
        FROM cost_of_living_profile AS profile
        INNER JOIN life_component_version AS component
            ON component.id = profile.life_component_version_id
        WHERE profile.id = NEW.cost_of_living_profile_id AND component.sealed_at IS NULL
    ),
    NEW.cost_of_living_profile_id,
    NULL
);

CREATE TRIGGER tr_living_cost_category_draft_insert
BEFORE INSERT ON living_cost_category
FOR EACH ROW
SET NEW.cost_of_living_profile_id = IF(
    EXISTS (
        SELECT 1
        FROM cost_of_living_profile AS profile
        INNER JOIN life_component_version AS component
            ON component.id = profile.life_component_version_id
        WHERE profile.id = NEW.cost_of_living_profile_id AND component.sealed_at IS NULL
    ),
    NEW.cost_of_living_profile_id,
    NULL
);

CREATE TRIGGER tr_living_cost_region_factor_draft_insert
BEFORE INSERT ON living_cost_region_factor
FOR EACH ROW
SET NEW.cost_of_living_profile_id = IF(
    EXISTS (
        SELECT 1
        FROM cost_of_living_profile AS profile
        INNER JOIN life_component_version AS component
            ON component.id = profile.life_component_version_id
        WHERE profile.id = NEW.cost_of_living_profile_id AND component.sealed_at IS NULL
    ),
    NEW.cost_of_living_profile_id,
    NULL
);

CREATE TRIGGER tr_living_cost_region_bridge_draft_insert
BEFORE INSERT ON living_cost_region_bridge
FOR EACH ROW
SET NEW.cost_of_living_profile_id = IF(
    EXISTS (
        SELECT 1
        FROM cost_of_living_profile AS profile
        INNER JOIN life_component_version AS component
            ON component.id = profile.life_component_version_id
        WHERE profile.id = NEW.cost_of_living_profile_id AND component.sealed_at IS NULL
    ),
    NEW.cost_of_living_profile_id,
    NULL
);

CREATE TRIGGER tr_living_cost_member_age_band_draft_insert
BEFORE INSERT ON living_cost_member_age_band
FOR EACH ROW
SET NEW.cost_of_living_profile_id = IF(
    EXISTS (
        SELECT 1
        FROM cost_of_living_profile AS profile
        INNER JOIN life_component_version AS component
            ON component.id = profile.life_component_version_id
        WHERE profile.id = NEW.cost_of_living_profile_id AND component.sealed_at IS NULL
    ),
    NEW.cost_of_living_profile_id,
    NULL
);

CREATE TRIGGER tr_living_cost_member_factor_draft_insert
BEFORE INSERT ON living_cost_member_factor
FOR EACH ROW
SET NEW.cost_of_living_profile_id = IF(
    EXISTS (
        SELECT 1
        FROM cost_of_living_profile AS profile
        INNER JOIN life_component_version AS component
            ON component.id = profile.life_component_version_id
        WHERE profile.id = NEW.cost_of_living_profile_id AND component.sealed_at IS NULL
    ),
    NEW.cost_of_living_profile_id,
    NULL
);

CREATE TRIGGER tr_living_cost_tenure_factor_draft_insert
BEFORE INSERT ON living_cost_tenure_factor
FOR EACH ROW
SET NEW.cost_of_living_profile_id = IF(
    EXISTS (
        SELECT 1
        FROM cost_of_living_profile AS profile
        INNER JOIN life_component_version AS component
            ON component.id = profile.life_component_version_id
        WHERE profile.id = NEW.cost_of_living_profile_id AND component.sealed_at IS NULL
    ),
    NEW.cost_of_living_profile_id,
    NULL
);

CREATE TRIGGER tr_living_cost_budget_band_no_update
BEFORE UPDATE ON living_cost_budget_band FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'living-cost budget bands are immutable';
CREATE TRIGGER tr_living_cost_budget_band_no_delete
BEFORE DELETE ON living_cost_budget_band FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'living-cost budget bands are immutable';
CREATE TRIGGER tr_living_cost_category_no_update
BEFORE UPDATE ON living_cost_category FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'living-cost categories are immutable';
CREATE TRIGGER tr_living_cost_category_no_delete
BEFORE DELETE ON living_cost_category FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'living-cost categories are immutable';
CREATE TRIGGER tr_living_cost_region_factor_no_update
BEFORE UPDATE ON living_cost_region_factor FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'living-cost region factors are immutable';
CREATE TRIGGER tr_living_cost_region_factor_no_delete
BEFORE DELETE ON living_cost_region_factor FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'living-cost region factors are immutable';
CREATE TRIGGER tr_living_cost_region_bridge_no_update
BEFORE UPDATE ON living_cost_region_bridge FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'living-cost region bridges are immutable';
CREATE TRIGGER tr_living_cost_region_bridge_no_delete
BEFORE DELETE ON living_cost_region_bridge FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'living-cost region bridges are immutable';
CREATE TRIGGER tr_living_cost_member_age_band_no_update
BEFORE UPDATE ON living_cost_member_age_band FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'living-cost member age bands are immutable';
CREATE TRIGGER tr_living_cost_member_age_band_no_delete
BEFORE DELETE ON living_cost_member_age_band FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'living-cost member age bands are immutable';
CREATE TRIGGER tr_living_cost_member_factor_no_update
BEFORE UPDATE ON living_cost_member_factor FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'living-cost member factors are immutable';
CREATE TRIGGER tr_living_cost_member_factor_no_delete
BEFORE DELETE ON living_cost_member_factor FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'living-cost member factors are immutable';
CREATE TRIGGER tr_living_cost_tenure_factor_no_update
BEFORE UPDATE ON living_cost_tenure_factor FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'living-cost tenure factors are immutable';
CREATE TRIGGER tr_living_cost_tenure_factor_no_delete
BEFORE DELETE ON living_cost_tenure_factor FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'living-cost tenure factors are immutable';
CREATE TRIGGER tr_life_region_no_update
BEFORE UPDATE ON life_region FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'life regions are immutable';
CREATE TRIGGER tr_life_region_no_delete
BEFORE DELETE ON life_region FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'life regions are immutable';

INSERT INTO life_region (region_key, display_name, region_order)
VALUES
    ('capitalArea', '수도권', 1),
    ('metropolitan', '광역시', 2),
    ('smallCity', '중소도시', 3),
    ('rural', '농어촌', 4);

INSERT INTO cost_of_living_profile
    (
        life_component_version_id,
        profile_key,
        base_year_month,
        base_cpi_index,
        proration_scale,
        legacy_dependent_age_years
    )
SELECT id,
       'dev-unranked-m4-cost-profile-2026-v1',
       '2026-01-01',
       1000000,
       377580,
       12
FROM life_component_version
WHERE component_kind = 'livingCost'
  AND version_key = 'dev-unranked-m4-living-cost-2026-v1'
  AND sealed_at IS NULL;

INSERT INTO living_cost_budget_band
    (cost_of_living_profile_id, band_key, display_name, band_order, factor_ppm)
SELECT profile.id, seed.band_key, seed.display_name, seed.band_order, seed.factor_ppm
FROM cost_of_living_profile AS profile
INNER JOIN (
    SELECT 'frugal' AS band_key, '절약' AS display_name, 1 AS band_order, 850000 AS factor_ppm
    UNION ALL SELECT 'standard', '표준', 2, 1000000
    UNION ALL SELECT 'generous', '여유', 3, 1250000
) AS seed
WHERE profile.profile_key = 'dev-unranked-m4-cost-profile-2026-v1';

INSERT INTO living_cost_category
    (
        cost_of_living_profile_id,
        category_key,
        display_name,
        category_order,
        base_amount_krw,
        essential,
        shortage_treatment,
        default_budget_band_id
    )
SELECT profile.id,
       seed.category_key,
       seed.display_name,
       seed.category_order,
       seed.base_amount_krw,
       seed.essential,
       IF(seed.essential, 'essentialArrear', 'reduceToCash'),
       standard_band.id
FROM cost_of_living_profile AS profile
INNER JOIN living_cost_budget_band AS standard_band
    ON standard_band.cost_of_living_profile_id = profile.id
   AND standard_band.band_key = 'standard'
INNER JOIN (
    SELECT 'housing' AS category_key, '주거' AS display_name, 1 AS category_order,
           450000 AS base_amount_krw, TRUE AS essential
    UNION ALL SELECT 'food', '식비', 2, 350000, TRUE
    UNION ALL SELECT 'transport', '교통', 3, 120000, TRUE
    UNION ALL SELECT 'communication', '통신', 4, 60000, TRUE
    UNION ALL SELECT 'utilities', '공과금', 5, 100000, TRUE
    UNION ALL SELECT 'healthcare', '의료', 6, 70000, TRUE
    UNION ALL SELECT 'education', '교육', 7, 50000, FALSE
    UNION ALL SELECT 'dependentCare', '부양 돌봄', 8, 120000, TRUE
    UNION ALL SELECT 'discretionary', '선택 소비', 9, 180000, FALSE
) AS seed
WHERE profile.profile_key = 'dev-unranked-m4-cost-profile-2026-v1';

INSERT INTO living_cost_region_bridge
    (cost_of_living_profile_id, character_region, region_key)
SELECT profile.id, region.region_key, region.region_key
FROM cost_of_living_profile AS profile
CROSS JOIN life_region AS region
WHERE profile.profile_key = 'dev-unranked-m4-cost-profile-2026-v1';

INSERT INTO living_cost_region_factor
    (cost_of_living_profile_id, living_cost_category_id, region_key, factor_ppm)
SELECT profile.id, category.id, seed.region_key, seed.factor_ppm
FROM cost_of_living_profile AS profile
INNER JOIN living_cost_category AS category
    ON category.cost_of_living_profile_id = profile.id
INNER JOIN (
    SELECT 'capitalArea' AS region_key, 'housing' AS category_key, 1300000 AS factor_ppm
    UNION ALL SELECT 'capitalArea', 'food', 1080000
    UNION ALL SELECT 'capitalArea', 'transport', 1100000
    UNION ALL SELECT 'capitalArea', 'communication', 1000000
    UNION ALL SELECT 'capitalArea', 'utilities', 1050000
    UNION ALL SELECT 'capitalArea', 'healthcare', 1050000
    UNION ALL SELECT 'capitalArea', 'education', 1150000
    UNION ALL SELECT 'capitalArea', 'dependentCare', 1120000
    UNION ALL SELECT 'capitalArea', 'discretionary', 1120000
    UNION ALL SELECT 'metropolitan', 'housing', 1000000
    UNION ALL SELECT 'metropolitan', 'food', 1020000
    UNION ALL SELECT 'metropolitan', 'transport', 1050000
    UNION ALL SELECT 'metropolitan', 'communication', 1000000
    UNION ALL SELECT 'metropolitan', 'utilities', 1000000
    UNION ALL SELECT 'metropolitan', 'healthcare', 1000000
    UNION ALL SELECT 'metropolitan', 'education', 1000000
    UNION ALL SELECT 'metropolitan', 'dependentCare', 1000000
    UNION ALL SELECT 'metropolitan', 'discretionary', 1000000
    UNION ALL SELECT 'smallCity', 'housing', 820000
    UNION ALL SELECT 'smallCity', 'food', 960000
    UNION ALL SELECT 'smallCity', 'transport', 950000
    UNION ALL SELECT 'smallCity', 'communication', 1000000
    UNION ALL SELECT 'smallCity', 'utilities', 980000
    UNION ALL SELECT 'smallCity', 'healthcare', 950000
    UNION ALL SELECT 'smallCity', 'education', 900000
    UNION ALL SELECT 'smallCity', 'dependentCare', 900000
    UNION ALL SELECT 'smallCity', 'discretionary', 930000
    UNION ALL SELECT 'rural', 'housing', 680000
    UNION ALL SELECT 'rural', 'food', 940000
    UNION ALL SELECT 'rural', 'transport', 1100000
    UNION ALL SELECT 'rural', 'communication', 1030000
    UNION ALL SELECT 'rural', 'utilities', 1080000
    UNION ALL SELECT 'rural', 'healthcare', 920000
    UNION ALL SELECT 'rural', 'education', 850000
    UNION ALL SELECT 'rural', 'dependentCare', 850000
    UNION ALL SELECT 'rural', 'discretionary', 880000
) AS seed ON BINARY seed.category_key = BINARY category.category_key
WHERE profile.profile_key = 'dev-unranked-m4-cost-profile-2026-v1';

INSERT INTO living_cost_member_age_band
    (
        cost_of_living_profile_id,
        member_role,
        age_band_key,
        minimum_age_years,
        maximum_age_years_exclusive
    )
SELECT id, 'dependent', 'allAges', 0, NULL
FROM cost_of_living_profile
WHERE profile_key = 'dev-unranked-m4-cost-profile-2026-v1';

INSERT INTO living_cost_member_factor
    (
        cost_of_living_profile_id,
        living_cost_member_age_band_id,
        living_cost_category_id,
        marginal_factor_ppm
    )
SELECT profile.id, age_band.id, category.id, seed.factor_ppm
FROM cost_of_living_profile AS profile
INNER JOIN living_cost_member_age_band AS age_band
    ON age_band.cost_of_living_profile_id = profile.id
   AND age_band.member_role = 'dependent'
   AND age_band.age_band_key = 'allAges'
INNER JOIN living_cost_category AS category
    ON category.cost_of_living_profile_id = profile.id
INNER JOIN (
    SELECT 'housing' AS category_key, 200000 AS factor_ppm
    UNION ALL SELECT 'food', 350000
    UNION ALL SELECT 'transport', 150000
    UNION ALL SELECT 'communication', 100000
    UNION ALL SELECT 'utilities', 150000
    UNION ALL SELECT 'healthcare', 150000
    UNION ALL SELECT 'education', 350000
    UNION ALL SELECT 'dependentCare', 300000
    UNION ALL SELECT 'discretionary', 150000
) AS seed ON BINARY seed.category_key = BINARY category.category_key
WHERE profile.profile_key = 'dev-unranked-m4-cost-profile-2026-v1';

INSERT INTO living_cost_tenure_factor
    (cost_of_living_profile_id, tenure_type, housing_replacement_factor_ppm)
SELECT profile.id, seed.tenure_type, seed.factor_ppm
FROM cost_of_living_profile AS profile
INNER JOIN (
    SELECT 'owner' AS tenure_type, 350000 AS factor_ppm
    UNION ALL SELECT 'jeonse', 200000
    UNION ALL SELECT 'monthlyRent', 0
    UNION ALL SELECT 'rentFree', 0
) AS seed
WHERE profile.profile_key = 'dev-unranked-m4-cost-profile-2026-v1';

-- This transient view assembles the canonical bytes once while the graph is still draft.
-- Publication and later verification use the stored manifest, so hashes never depend on a
-- reader's group_concat_max_len session setting.
CREATE VIEW living_cost_component_canonical_manifest AS
SELECT
    component.id AS life_component_version_id,
    CONCAT(
        '{"availability":', JSON_QUOTE(component.availability),
        ',"budgetBands":[',
        (SELECT GROUP_CONCAT(
            CAST(JSON_OBJECT(
                'bandKey', band.band_key,
                'bandOrder', band.band_order,
                'displayName', band.display_name,
                'factorPpm', band.factor_ppm
            ) AS CHAR CHARACTER SET utf8mb4)
            ORDER BY band.band_order SEPARATOR ','
         ) FROM living_cost_budget_band AS band
         WHERE band.cost_of_living_profile_id = profile.id),
        '],"categories":[',
        (SELECT GROUP_CONCAT(
            CAST(JSON_OBJECT(
                'baseAmountKrw', category.base_amount_krw,
                'categoryKey', category.category_key,
                'categoryOrder', category.category_order,
                'defaultBandKey', band.band_key,
                'displayName', category.display_name,
                'essential', category.essential,
                'shortageTreatment', category.shortage_treatment
            ) AS CHAR CHARACTER SET utf8mb4)
            ORDER BY category.category_order SEPARATOR ','
         )
         FROM living_cost_category AS category
         INNER JOIN living_cost_budget_band AS band
             ON band.id = category.default_budget_band_id
         WHERE category.cost_of_living_profile_id = profile.id),
        '],"componentKind":"livingCost","legacyRegionBridge":[',
        (SELECT GROUP_CONCAT(
            CAST(JSON_OBJECT(
                'characterRegion', bridge.character_region,
                'regionKey', bridge.region_key
            ) AS CHAR CHARACTER SET utf8mb4)
            ORDER BY bridge.character_region SEPARATOR ','
         ) FROM living_cost_region_bridge AS bridge
         WHERE bridge.cost_of_living_profile_id = profile.id),
        '],"memberAgeBands":[',
        (SELECT GROUP_CONCAT(
            CAST(JSON_OBJECT(
                'ageBandKey', age_band.age_band_key,
                'maximumAgeYearsExclusive', age_band.maximum_age_years_exclusive,
                'memberRole', age_band.member_role,
                'minimumAgeYears', age_band.minimum_age_years
            ) AS CHAR CHARACTER SET utf8mb4)
            ORDER BY age_band.member_role, age_band.age_band_key SEPARATOR ','
         ) FROM living_cost_member_age_band AS age_band
         WHERE age_band.cost_of_living_profile_id = profile.id),
        '],"memberFactors":[',
        (SELECT GROUP_CONCAT(
            CAST(JSON_OBJECT(
                'ageBandKey', age_band.age_band_key,
                'categoryKey', category.category_key,
                'marginalFactorPpm', member_factor.marginal_factor_ppm,
                'memberRole', age_band.member_role
            ) AS CHAR CHARACTER SET utf8mb4)
            ORDER BY age_band.member_role, age_band.age_band_key,
                category.category_order SEPARATOR ','
         )
         FROM living_cost_member_factor AS member_factor
         INNER JOIN living_cost_member_age_band AS age_band
             ON age_band.id = member_factor.living_cost_member_age_band_id
         INNER JOIN living_cost_category AS category
             ON category.id = member_factor.living_cost_category_id
         WHERE member_factor.cost_of_living_profile_id = profile.id),
        '],"profile":',
        CAST(JSON_OBJECT(
            'baseCpiIndex', profile.base_cpi_index,
            'baseYearMonth', DATE_FORMAT(profile.base_year_month, '%Y-%m-%d'),
            'legacyDependentAgeYears', profile.legacy_dependent_age_years,
            'prorationScale', profile.proration_scale,
            'profileKey', profile.profile_key
        ) AS CHAR CHARACTER SET utf8mb4),
        ',"rankedEligible":', IF(component.ranked_eligible, 'true', 'false'),
        ',"regionFactors":[',
        (SELECT GROUP_CONCAT(
            CAST(JSON_OBJECT(
                'categoryKey', category.category_key,
                'factorPpm', factor.factor_ppm,
                'regionKey', factor.region_key
            ) AS CHAR CHARACTER SET utf8mb4)
            ORDER BY factor.region_key, category.category_order SEPARATOR ','
         )
         FROM living_cost_region_factor AS factor
         INNER JOIN living_cost_category AS category
             ON category.id = factor.living_cost_category_id
         WHERE factor.cost_of_living_profile_id = profile.id),
        '],"schemaVersion":1,"tenureFactors":[',
        (SELECT GROUP_CONCAT(
            CAST(JSON_OBJECT(
                'housingReplacementFactorPpm', tenure.housing_replacement_factor_ppm,
                'tenureType', tenure.tenure_type
            ) AS CHAR CHARACTER SET utf8mb4)
            ORDER BY tenure.tenure_type SEPARATOR ','
         ) FROM living_cost_tenure_factor AS tenure
         WHERE tenure.cost_of_living_profile_id = profile.id),
        '],"versionKey":', JSON_QUOTE(component.version_key), '}'
    ) AS canonical_json
FROM life_component_version AS component
INNER JOIN cost_of_living_profile AS profile
    ON profile.life_component_version_id = component.id;

INSERT INTO life_component_canonical_manifest
    (life_component_version_id, canonical_json)
SELECT life_component_version_id, canonical_json
FROM living_cost_component_canonical_manifest;

DROP VIEW living_cost_component_canonical_manifest;

-- Publication proves that every current typed axis is complete. Later slices publish a new
-- component rather than inserting rows into this sealed graph.
CREATE TRIGGER tr_life_component_version_living_publish
BEFORE UPDATE ON life_component_version
FOR EACH ROW
FOLLOWS tr_life_component_version_seal_only
SET NEW.version_key = IF(
    NEW.component_kind <> 'livingCost'
        OR NEW.availability <> 'active'
        OR (
            (SELECT COUNT(*) FROM cost_of_living_profile WHERE life_component_version_id = OLD.id) = 1
            AND NOT EXISTS (
                SELECT 1
                FROM cost_of_living_profile AS profile
                WHERE profile.life_component_version_id = OLD.id
                  AND (
                      profile.proration_scale <> 377580
                      OR (SELECT COUNT(*) FROM living_cost_budget_band AS band
                          WHERE band.cost_of_living_profile_id = profile.id) <> 3
                      OR (SELECT COUNT(*) FROM living_cost_category AS category
                          WHERE category.cost_of_living_profile_id = profile.id) <> 9
                      OR (SELECT COUNT(*) FROM living_cost_region_bridge AS bridge
                          WHERE bridge.cost_of_living_profile_id = profile.id) <> 4
                      OR (SELECT COUNT(*) FROM living_cost_region_factor AS factor
                          WHERE factor.cost_of_living_profile_id = profile.id) <> 36
                      OR (SELECT COUNT(*) FROM living_cost_member_age_band AS age_band
                          WHERE age_band.cost_of_living_profile_id = profile.id) <> 1
                      OR (SELECT COUNT(*) FROM living_cost_member_factor AS member_factor
                          WHERE member_factor.cost_of_living_profile_id = profile.id) <> 9
                      OR (SELECT COUNT(*) FROM living_cost_tenure_factor AS tenure
                          WHERE tenure.cost_of_living_profile_id = profile.id) <> 4
                      OR EXISTS (
                          SELECT 1
                          FROM living_cost_category AS category
                          WHERE category.cost_of_living_profile_id = profile.id
                            AND NOT EXISTS (
                                SELECT 1
                                FROM living_cost_budget_band AS band
                                WHERE band.id = category.default_budget_band_id
                                  AND band.cost_of_living_profile_id = profile.id
                                  AND band.band_key = 'standard'
                            )
                      )
                      OR EXISTS (
                          SELECT 1
                          FROM living_cost_category AS category
                          CROSS JOIN life_region AS region
                          WHERE category.cost_of_living_profile_id = profile.id
                            AND NOT EXISTS (
                                SELECT 1 FROM living_cost_region_factor AS factor
                                WHERE factor.cost_of_living_profile_id = profile.id
                                  AND factor.living_cost_category_id = category.id
                                  AND factor.region_key = region.region_key
                            )
                      )
                  )
            )
        ),
    NEW.version_key,
    NULL
);

UPDATE life_component_version AS component
INNER JOIN life_component_canonical_manifest AS manifest
    ON manifest.life_component_version_id = component.id
SET component.canonical_sha256 = manifest.canonical_sha256,
    component.sealed_at = CURRENT_TIMESTAMP(3)
WHERE component.component_kind = 'livingCost'
  AND component.version_key = 'dev-unranked-m4-living-cost-2026-v1'
  AND component.sealed_at IS NULL;

UPDATE life_catalog_set
SET canonical_sha256 = SHA2(
        CAST(JSON_OBJECT(
            'catalogKey', catalog_key,
            'corporationComponentVersionId', CAST(corporation_component_version_id AS CHAR),
            'insuranceComponentVersionId', CAST(insurance_component_version_id AS CHAR),
            'lifeEventComponentVersionId', CAST(life_event_component_version_id AS CHAR),
            'legacyDependentAgeYears', legacy_dependent_age_years,
            'livingCostComponentVersionId', CAST(living_cost_component_version_id AS CHAR),
            'schemaVersion', 1,
            'welfareComponentVersionId', CAST(welfare_component_version_id AS CHAR)
        ) AS CHAR CHARACTER SET utf8mb4),
        256
    ),
    sealed_at = CURRENT_TIMESTAMP(3)
WHERE catalog_key = 'dev-unranked-m4-life-2026-v1'
  AND sealed_at IS NULL;

CREATE TABLE household (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    life_catalog_set_id             BIGINT UNSIGNED NOT NULL,
    legacy_debt_krw_at_activation   BIGINT          NOT NULL,
    created_game_day                INT UNSIGNED    NOT NULL,
    created_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_household_save_run (save_id, run_revision),
    UNIQUE KEY uk_household_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_household_life_catalog (save_id, run_revision, life_catalog_set_id, id),
    CONSTRAINT fk_household_rule_bundle
        FOREIGN KEY (save_id, run_revision, life_catalog_set_id)
        REFERENCES run_rule_bundle (save_id, run_revision, life_catalog_set_id)
        ON DELETE CASCADE,
    CONSTRAINT ck_household_legacy_debt CHECK (legacy_debt_krw_at_activation >= 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE household_member (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    household_id                BIGINT UNSIGNED NOT NULL,
    member_role                 VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ordinal                     SMALLINT UNSIGNED NOT NULL,
    birth_date                  DATE            NOT NULL,
    joined_game_day             INT UNSIGNED    NOT NULL,
    left_game_day               INT UNSIGNED        NULL,
    tax_dependent_eligible      BOOLEAN         NOT NULL,
    active_member_slot          VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin
        GENERATED ALWAYS AS (
            CASE
                WHEN left_game_day IS NULL THEN CONCAT(member_role, ':', ordinal)
                ELSE NULL
            END
        ) STORED,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_household_member_period
        (household_id, member_role, ordinal, joined_game_day),
    UNIQUE KEY uk_household_member_active (household_id, active_member_slot),
    UNIQUE KEY uk_household_member_save_run_id (save_id, run_revision, id),
    KEY ix_household_member_active_tax
        (save_id, run_revision, left_game_day, tax_dependent_eligible, id),
    CONSTRAINT fk_household_member_household
        FOREIGN KEY (save_id, run_revision, household_id)
        REFERENCES household (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT ck_household_member_role CHECK (
        member_role IN ('player', 'dependent', 'partner', 'child', 'parent')
    ),
    CONSTRAINT ck_household_member_ordinal CHECK (
        (member_role = 'player' AND ordinal = 0 AND tax_dependent_eligible = FALSE)
        OR (member_role <> 'player' AND ordinal > 0)
    ),
    CONSTRAINT ck_household_member_period CHECK (
        left_game_day IS NULL OR left_game_day > joined_game_day
    ),
    CONSTRAINT ck_household_member_tax_eligible CHECK (
        tax_dependent_eligible IN (FALSE, TRUE)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE residence (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    household_id                BIGINT UNSIGNED NOT NULL,
    region_key                  VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    tenure_type                 VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    effective_from_game_day     INT UNSIGNED    NOT NULL,
    effective_to_game_day       INT UNSIGNED        NULL,
    active_residence_slot       TINYINT UNSIGNED GENERATED ALWAYS AS (
        CASE WHEN effective_to_game_day IS NULL THEN 1 ELSE NULL END
    ) STORED,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_residence_active (household_id, active_residence_slot),
    UNIQUE KEY uk_residence_save_run_id (save_id, run_revision, id),
    KEY ix_residence_history (household_id, effective_from_game_day, id),
    KEY ix_residence_region (region_key),
    CONSTRAINT fk_residence_household
        FOREIGN KEY (save_id, run_revision, household_id)
        REFERENCES household (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_residence_region
        FOREIGN KEY (region_key) REFERENCES life_region (region_key),
    CONSTRAINT ck_residence_tenure CHECK (
        tenure_type IN ('owner', 'jeonse', 'monthlyRent', 'rentFree')
    ),
    CONSTRAINT ck_residence_period CHECK (
        effective_to_game_day IS NULL OR effective_to_game_day > effective_from_game_day
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE household_budget (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    household_id                    BIGINT UNSIGNED NOT NULL,
    cost_of_living_profile_id       BIGINT UNSIGNED NOT NULL,
    effective_from_game_day         INT UNSIGNED    NOT NULL,
    effective_to_game_day           INT UNSIGNED        NULL,
    sealed_at                       DATETIME(3)          NULL,
    active_budget_slot              TINYINT UNSIGNED GENERATED ALWAYS AS (
        CASE
            WHEN sealed_at IS NOT NULL AND effective_to_game_day IS NULL THEN 1
            ELSE NULL
        END
    ) STORED,
    created_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_household_budget_active (household_id, active_budget_slot),
    UNIQUE KEY uk_household_budget_profile_id (cost_of_living_profile_id, id),
    UNIQUE KEY uk_household_budget_save_run_id (save_id, run_revision, id),
    CONSTRAINT fk_household_budget_household
        FOREIGN KEY (save_id, run_revision, household_id)
        REFERENCES household (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_household_budget_profile
        FOREIGN KEY (cost_of_living_profile_id) REFERENCES cost_of_living_profile (id),
    CONSTRAINT ck_household_budget_period CHECK (
        effective_to_game_day IS NULL OR effective_to_game_day > effective_from_game_day
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE household_budget_selection (
    cost_of_living_profile_id       BIGINT UNSIGNED NOT NULL,
    household_budget_id             BIGINT UNSIGNED NOT NULL,
    living_cost_category_id         BIGINT UNSIGNED NOT NULL,
    living_cost_budget_band_id      BIGINT UNSIGNED NOT NULL,
    created_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (household_budget_id, living_cost_category_id),
    KEY ix_household_budget_selection_band
        (cost_of_living_profile_id, living_cost_budget_band_id),
    CONSTRAINT fk_household_budget_selection_budget
        FOREIGN KEY (cost_of_living_profile_id, household_budget_id)
        REFERENCES household_budget (cost_of_living_profile_id, id) ON DELETE CASCADE,
    CONSTRAINT fk_household_budget_selection_category
        FOREIGN KEY (cost_of_living_profile_id, living_cost_category_id)
        REFERENCES living_cost_category (cost_of_living_profile_id, id),
    CONSTRAINT fk_household_budget_selection_band
        FOREIGN KEY (cost_of_living_profile_id, living_cost_budget_band_id)
        REFERENCES living_cost_budget_band (cost_of_living_profile_id, id)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE living_cost_remainder (
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    household_id                    BIGINT UNSIGNED NOT NULL,
    cost_of_living_profile_id       BIGINT UNSIGNED NOT NULL,
    living_cost_category_id         BIGINT UNSIGNED NOT NULL,
    remainder_numerator             DECIMAL(39, 0) NOT NULL,
    last_year_month                 DATE                NULL,
    updated_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (household_id, living_cost_category_id),
    KEY ix_living_cost_remainder_profile_category
        (cost_of_living_profile_id, living_cost_category_id),
    CONSTRAINT fk_living_cost_remainder_household
        FOREIGN KEY (save_id, run_revision, household_id)
        REFERENCES household (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_living_cost_remainder_category
        FOREIGN KEY (cost_of_living_profile_id, living_cost_category_id)
        REFERENCES living_cost_category (cost_of_living_profile_id, id),
    CONSTRAINT ck_living_cost_remainder_month CHECK (
        last_year_month IS NULL OR DAY(last_year_month) = 1
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE living_cost_month (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    household_id                    BIGINT UNSIGNED NOT NULL,
    life_catalog_set_id             BIGINT UNSIGNED NOT NULL,
    cost_of_living_profile_id       BIGINT UNSIGNED NOT NULL,
    household_budget_id             BIGINT UNSIGNED NOT NULL,
    residence_id                    BIGINT UNSIGNED NOT NULL,
    `year_month`                    DATE            NOT NULL,
    activation_date                 DATE            NOT NULL,
    due_game_day                    INT UNSIGNED    NOT NULL,
    cpi_game_day                    INT UNSIGNED    NOT NULL,
    cpi_index                       BIGINT UNSIGNED NOT NULL,
    region_key                      VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    tenure_type                     VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    household_fingerprint_sha256    CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    proration_scale                 INT UNSIGNED    NOT NULL,
    proration_units                 INT UNSIGNED    NOT NULL,
    days_in_month                   TINYINT UNSIGNED NOT NULL,
    status                          VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    gross_amount_krw                BIGINT              NULL,
    paid_amount_krw                 BIGINT              NULL,
    arrear_amount_krw               BIGINT              NULL,
    ledger_transaction_id           BIGINT UNSIGNED     NULL,
    created_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_living_cost_month_household_month (household_id, `year_month`),
    UNIQUE KEY uk_living_cost_month_save_run_id (save_id, run_revision, id),
    KEY ix_living_cost_month_profile (cost_of_living_profile_id),
    KEY ix_living_cost_month_budget (cost_of_living_profile_id, household_budget_id),
    KEY ix_living_cost_month_residence (save_id, run_revision, residence_id),
    KEY ix_living_cost_month_ledger (save_id, run_revision, ledger_transaction_id),
    CONSTRAINT fk_living_cost_month_household
        FOREIGN KEY (save_id, run_revision, household_id)
        REFERENCES household (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_living_cost_month_budget
        FOREIGN KEY (cost_of_living_profile_id, household_budget_id)
        REFERENCES household_budget (cost_of_living_profile_id, id),
    CONSTRAINT fk_living_cost_month_residence
        FOREIGN KEY (save_id, run_revision, residence_id)
        REFERENCES residence (save_id, run_revision, id),
    CONSTRAINT fk_living_cost_month_region
        FOREIGN KEY (region_key) REFERENCES life_region (region_key),
    CONSTRAINT fk_living_cost_month_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT ck_living_cost_month_date CHECK (
        DAY(`year_month`) = 1
        AND activation_date BETWEEN `year_month` AND LAST_DAY(`year_month`)
    ),
    CONSTRAINT ck_living_cost_month_cpi CHECK (cpi_index > 0),
    CONSTRAINT ck_living_cost_month_fingerprint CHECK (
        household_fingerprint_sha256 REGEXP '^[0-9a-f]{64}$'
    ),
    CONSTRAINT ck_living_cost_month_proration CHECK (
        proration_scale = 377580
        AND days_in_month BETWEEN 28 AND 31
        AND proration_units > 0
        AND proration_units <= proration_scale
        AND MOD(proration_scale, days_in_month) = 0
    ),
    CONSTRAINT ck_living_cost_month_tenure CHECK (
        tenure_type IN ('owner', 'jeonse', 'monthlyRent', 'rentFree')
    ),
    CONSTRAINT ck_living_cost_month_state CHECK (
        (
            status = 'pending'
            AND gross_amount_krw IS NULL
            AND paid_amount_krw IS NULL
            AND arrear_amount_krw IS NULL
            AND ledger_transaction_id IS NULL
        )
        OR (
            status = 'settled'
            AND gross_amount_krw >= 0
            AND paid_amount_krw >= 0
            AND arrear_amount_krw >= 0
            AND gross_amount_krw >= paid_amount_krw + arrear_amount_krw
            AND ledger_transaction_id IS NOT NULL
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE living_cost_month_item (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED    NOT NULL,
    living_cost_month_id                BIGINT UNSIGNED NOT NULL,
    cost_of_living_profile_id           BIGINT UNSIGNED NOT NULL,
    living_cost_category_id             BIGINT UNSIGNED NOT NULL,
    living_cost_budget_band_id          BIGINT UNSIGNED NOT NULL,
    category_key                        VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    category_order                      TINYINT UNSIGNED NOT NULL,
    essential                           BOOLEAN         NOT NULL,
    base_amount_krw                     BIGINT          NOT NULL,
    base_cpi_index                      BIGINT UNSIGNED NOT NULL,
    current_cpi_index                   BIGINT UNSIGNED NOT NULL,
    region_factor_ppm                   INT UNSIGNED    NOT NULL,
    household_factor_ppm                BIGINT UNSIGNED NOT NULL,
    budget_factor_ppm                   INT UNSIGNED    NOT NULL,
    tenure_replacement_factor_ppm       INT UNSIGNED    NOT NULL,
    prior_remainder_numerator           DECIMAL(39, 0) NOT NULL,
    gross_amount_krw                    BIGINT          NOT NULL,
    next_remainder_numerator            DECIMAL(39, 0) NOT NULL,
    paid_amount_krw                     BIGINT              NULL,
    arrear_amount_krw                   BIGINT              NULL,
    created_at                          DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                          DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_living_cost_month_item_category
        (living_cost_month_id, living_cost_category_id),
    UNIQUE KEY uk_living_cost_month_item_save_run_id (save_id, run_revision, id),
    KEY ix_living_cost_month_item_category
        (cost_of_living_profile_id, living_cost_category_id),
    KEY ix_living_cost_month_item_band
        (cost_of_living_profile_id, living_cost_budget_band_id),
    CONSTRAINT fk_living_cost_month_item_month
        FOREIGN KEY (save_id, run_revision, living_cost_month_id)
        REFERENCES living_cost_month (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_living_cost_month_item_category
        FOREIGN KEY (cost_of_living_profile_id, living_cost_category_id)
        REFERENCES living_cost_category (cost_of_living_profile_id, id),
    CONSTRAINT fk_living_cost_month_item_band
        FOREIGN KEY (cost_of_living_profile_id, living_cost_budget_band_id)
        REFERENCES living_cost_budget_band (cost_of_living_profile_id, id),
    CONSTRAINT ck_living_cost_month_item_category CHECK (
        category_key IN (
            'housing', 'food', 'transport', 'communication', 'utilities',
            'healthcare', 'education', 'dependentCare', 'discretionary'
        )
        AND category_order BETWEEN 1 AND 9
    ),
    CONSTRAINT ck_living_cost_month_item_factors CHECK (
        base_amount_krw >= 0
        AND base_cpi_index > 0
        AND current_cpi_index > 0
        AND region_factor_ppm > 0
        AND household_factor_ppm >= 1000000
        AND budget_factor_ppm > 0
        AND tenure_replacement_factor_ppm BETWEEN 0 AND 1000000
        AND gross_amount_krw >= 0
    ),
    CONSTRAINT ck_living_cost_month_item_outcome CHECK (
        (paid_amount_krw IS NULL AND arrear_amount_krw IS NULL)
        OR (
            paid_amount_krw >= 0
            AND arrear_amount_krw >= 0
            AND (
                (essential = TRUE AND paid_amount_krw + arrear_amount_krw = gross_amount_krw)
                OR (essential = FALSE AND arrear_amount_krw = 0
                    AND paid_amount_krw <= gross_amount_krw)
            )
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE essential_arrear (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    household_id                    BIGINT UNSIGNED NOT NULL,
    living_cost_month_item_id       BIGINT UNSIGNED NOT NULL,
    category_key                    VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    due_year_month                  DATE            NOT NULL,
    original_amount_krw             BIGINT          NOT NULL,
    paid_amount_krw                 BIGINT          NOT NULL DEFAULT 0,
    outstanding_amount_krw          BIGINT GENERATED ALWAYS AS (
        original_amount_krw - paid_amount_krw
    ) STORED,
    status                          VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_game_day                INT UNSIGNED    NOT NULL,
    closed_game_day                 INT UNSIGNED        NULL,
    created_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_essential_arrear_month_item (living_cost_month_item_id),
    UNIQUE KEY uk_essential_arrear_save_run_id (save_id, run_revision, id),
    KEY ix_essential_arrear_priority
        (save_id, run_revision, status, due_year_month, category_key, id),
    CONSTRAINT fk_essential_arrear_household
        FOREIGN KEY (save_id, run_revision, household_id)
        REFERENCES household (save_id, run_revision, id) ON DELETE CASCADE,
    CONSTRAINT fk_essential_arrear_month_item
        FOREIGN KEY (save_id, run_revision, living_cost_month_item_id)
        REFERENCES living_cost_month_item (save_id, run_revision, id),
    CONSTRAINT ck_essential_arrear_category CHECK (
        category_key IN (
            'housing', 'food', 'transport', 'communication', 'utilities',
            'healthcare', 'dependentCare'
        )
    ),
    CONSTRAINT ck_essential_arrear_month CHECK (DAY(due_year_month) = 1),
    CONSTRAINT ck_essential_arrear_amount CHECK (
        original_amount_krw > 0
        AND paid_amount_krw BETWEEN 0 AND original_amount_krw
    ),
    CONSTRAINT ck_essential_arrear_state CHECK (
        (status = 'active' AND paid_amount_krw < original_amount_krw AND closed_game_day IS NULL)
        OR (status = 'paid' AND paid_amount_krw = original_amount_krw
            AND closed_game_day IS NOT NULL AND closed_game_day >= created_game_day)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE essential_arrear_payment (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED    NOT NULL,
    essential_arrear_id         BIGINT UNSIGNED NOT NULL,
    payment_no                  INT UNSIGNED    NOT NULL,
    payment_kind                VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    amount_krw                  BIGINT          NOT NULL,
    game_day                    INT UNSIGNED    NOT NULL,
    command_id                  CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    status                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ledger_transaction_id       BIGINT UNSIGNED     NULL,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_essential_arrear_payment_no (essential_arrear_id, payment_no),
    UNIQUE KEY uk_essential_arrear_payment_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_essential_arrear_payment_ledger
        (save_id, run_revision, ledger_transaction_id),
    KEY ix_essential_arrear_payment_arrear
        (save_id, run_revision, essential_arrear_id),
    CONSTRAINT fk_essential_arrear_payment_arrear
        FOREIGN KEY (save_id, run_revision, essential_arrear_id)
        REFERENCES essential_arrear (save_id, run_revision, id),
    CONSTRAINT fk_essential_arrear_payment_command
        FOREIGN KEY (save_id, command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT fk_essential_arrear_payment_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT ck_essential_arrear_payment_number CHECK (payment_no > 0),
    CONSTRAINT ck_essential_arrear_payment_kind CHECK (
        (payment_kind = 'automatic' AND command_id IS NULL)
        OR (payment_kind = 'manual' AND command_id IS NOT NULL)
    ),
    CONSTRAINT ck_essential_arrear_payment_amount CHECK (amount_krw > 0),
    CONSTRAINT ck_essential_arrear_payment_state CHECK (
        (status = 'prepared' AND ledger_transaction_id IS NULL)
        OR (status = 'applied' AND ledger_transaction_id IS NOT NULL)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

-- Mutable runtime rows have one narrow state transition; identities and calculation inputs stay fixed.
CREATE TRIGGER tr_household_no_update
BEFORE UPDATE ON household FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'household identities are immutable';
CREATE TRIGGER tr_household_no_delete
BEFORE DELETE ON household FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'households are run-owned';

CREATE TRIGGER tr_household_member_transition_only
BEFORE UPDATE ON household_member
FOR EACH ROW
SET NEW.id = IF(
    OLD.left_game_day IS NULL
        AND NEW.left_game_day IS NOT NULL
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.household_id = OLD.household_id
        AND BINARY NEW.member_role = BINARY OLD.member_role
        AND NEW.ordinal = OLD.ordinal
        AND NEW.birth_date = OLD.birth_date
        AND NEW.joined_game_day = OLD.joined_game_day
        AND NEW.tax_dependent_eligible = OLD.tax_dependent_eligible
        AND NEW.created_at = OLD.created_at,
    OLD.id,
    NULL
);
CREATE TRIGGER tr_household_member_no_delete
BEFORE DELETE ON household_member FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'household member history is immutable';

CREATE TRIGGER tr_residence_transition_only
BEFORE UPDATE ON residence
FOR EACH ROW
SET NEW.id = IF(
    OLD.effective_to_game_day IS NULL
        AND NEW.effective_to_game_day IS NOT NULL
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.household_id = OLD.household_id
        AND BINARY NEW.region_key = BINARY OLD.region_key
        AND BINARY NEW.tenure_type = BINARY OLD.tenure_type
        AND NEW.effective_from_game_day = OLD.effective_from_game_day
        AND NEW.created_at = OLD.created_at,
    OLD.id,
    NULL
);
CREATE TRIGGER tr_residence_no_delete
BEFORE DELETE ON residence FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'residence history is immutable';

CREATE TRIGGER tr_household_budget_selection_draft_insert
BEFORE INSERT ON household_budget_selection
FOR EACH ROW
SET NEW.household_budget_id = IF(
    EXISTS (
        SELECT 1 FROM household_budget
        WHERE id = NEW.household_budget_id
          AND cost_of_living_profile_id = NEW.cost_of_living_profile_id
          AND sealed_at IS NULL
          AND effective_to_game_day IS NULL
    ),
    NEW.household_budget_id,
    NULL
);

CREATE TRIGGER tr_household_budget_transition
BEFORE UPDATE ON household_budget
FOR EACH ROW
SET NEW.id = IF(
    (
        OLD.sealed_at IS NULL
        AND NEW.sealed_at IS NOT NULL
        AND OLD.effective_to_game_day IS NULL
        AND NEW.effective_to_game_day IS NULL
        AND (
            SELECT COUNT(*) FROM household_budget_selection AS selection
            WHERE selection.household_budget_id = OLD.id
        ) = 9
        AND NOT EXISTS (
            SELECT 1
            FROM living_cost_category AS category
            WHERE category.cost_of_living_profile_id = OLD.cost_of_living_profile_id
              AND NOT EXISTS (
                  SELECT 1 FROM household_budget_selection AS selection
                  WHERE selection.household_budget_id = OLD.id
                    AND selection.living_cost_category_id = category.id
              )
        )
    )
    OR (
        OLD.sealed_at IS NOT NULL
        AND NEW.sealed_at = OLD.sealed_at
        AND OLD.effective_to_game_day IS NULL
        AND NEW.effective_to_game_day IS NOT NULL
    ),
    IF(
        NEW.save_id = OLD.save_id
            AND NEW.run_revision = OLD.run_revision
            AND NEW.household_id = OLD.household_id
            AND NEW.cost_of_living_profile_id = OLD.cost_of_living_profile_id
            AND NEW.effective_from_game_day = OLD.effective_from_game_day
            AND NEW.created_at = OLD.created_at,
        OLD.id,
        NULL
    ),
    NULL
);
CREATE TRIGGER tr_household_budget_no_delete
BEFORE DELETE ON household_budget FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'household budget history is immutable';
CREATE TRIGGER tr_household_budget_selection_no_update
BEFORE UPDATE ON household_budget_selection FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'budget selections are immutable';
CREATE TRIGGER tr_household_budget_selection_no_delete
BEFORE DELETE ON household_budget_selection FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'budget selections are immutable';

CREATE TRIGGER tr_living_cost_remainder_transition
BEFORE UPDATE ON living_cost_remainder
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.household_id = OLD.household_id
        AND NEW.cost_of_living_profile_id = OLD.cost_of_living_profile_id
        AND NEW.living_cost_category_id = OLD.living_cost_category_id
        AND (
            (OLD.last_year_month IS NULL AND NEW.last_year_month IS NOT NULL)
            OR NEW.last_year_month = DATE_ADD(OLD.last_year_month, INTERVAL 1 MONTH)
        ),
    OLD.save_id,
    NULL
);
CREATE TRIGGER tr_living_cost_remainder_no_delete
BEFORE DELETE ON living_cost_remainder FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'living-cost remainders are durable state';

CREATE TRIGGER tr_living_cost_month_transition_only
BEFORE UPDATE ON living_cost_month
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'pending'
        AND NEW.status = 'settled'
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.household_id = OLD.household_id
        AND NEW.life_catalog_set_id = OLD.life_catalog_set_id
        AND NEW.cost_of_living_profile_id = OLD.cost_of_living_profile_id
        AND NEW.household_budget_id = OLD.household_budget_id
        AND NEW.residence_id = OLD.residence_id
        AND NEW.`year_month` = OLD.`year_month`
        AND NEW.activation_date = OLD.activation_date
        AND NEW.due_game_day = OLD.due_game_day
        AND NEW.cpi_game_day = OLD.cpi_game_day
        AND NEW.cpi_index = OLD.cpi_index
        AND BINARY NEW.region_key = BINARY OLD.region_key
        AND BINARY NEW.tenure_type = BINARY OLD.tenure_type
        AND BINARY NEW.household_fingerprint_sha256
            = BINARY OLD.household_fingerprint_sha256
        AND NEW.proration_scale = OLD.proration_scale
        AND NEW.proration_units = OLD.proration_units
        AND NEW.days_in_month = OLD.days_in_month
        AND NEW.created_at = OLD.created_at
        AND (SELECT COUNT(*) FROM living_cost_month_item AS item
             WHERE item.living_cost_month_id = OLD.id) = 9
        AND NOT EXISTS (
            SELECT 1 FROM living_cost_month_item AS item
            WHERE item.living_cost_month_id = OLD.id
              AND (item.paid_amount_krw IS NULL OR item.arrear_amount_krw IS NULL)
        )
        AND NEW.gross_amount_krw = (
            SELECT COALESCE(SUM(item.gross_amount_krw), 0)
            FROM living_cost_month_item AS item WHERE item.living_cost_month_id = OLD.id
        )
        AND NEW.paid_amount_krw = (
            SELECT COALESCE(SUM(item.paid_amount_krw), 0)
            FROM living_cost_month_item AS item WHERE item.living_cost_month_id = OLD.id
        )
        AND NEW.arrear_amount_krw = (
            SELECT COALESCE(SUM(item.arrear_amount_krw), 0)
            FROM living_cost_month_item AS item WHERE item.living_cost_month_id = OLD.id
        ),
    OLD.id,
    NULL
);
CREATE TRIGGER tr_living_cost_month_no_delete
BEFORE DELETE ON living_cost_month FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'living-cost months are immutable history';

CREATE TRIGGER tr_living_cost_month_item_transition_only
BEFORE UPDATE ON living_cost_month_item
FOR EACH ROW
SET NEW.id = IF(
    OLD.paid_amount_krw IS NULL
        AND OLD.arrear_amount_krw IS NULL
        AND NEW.paid_amount_krw IS NOT NULL
        AND NEW.arrear_amount_krw IS NOT NULL
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.living_cost_month_id = OLD.living_cost_month_id
        AND NEW.cost_of_living_profile_id = OLD.cost_of_living_profile_id
        AND NEW.living_cost_category_id = OLD.living_cost_category_id
        AND NEW.living_cost_budget_band_id = OLD.living_cost_budget_band_id
        AND BINARY NEW.category_key = BINARY OLD.category_key
        AND NEW.category_order = OLD.category_order
        AND NEW.essential = OLD.essential
        AND NEW.base_amount_krw = OLD.base_amount_krw
        AND NEW.base_cpi_index = OLD.base_cpi_index
        AND NEW.current_cpi_index = OLD.current_cpi_index
        AND NEW.region_factor_ppm = OLD.region_factor_ppm
        AND NEW.household_factor_ppm = OLD.household_factor_ppm
        AND NEW.budget_factor_ppm = OLD.budget_factor_ppm
        AND NEW.tenure_replacement_factor_ppm = OLD.tenure_replacement_factor_ppm
        AND NEW.prior_remainder_numerator = OLD.prior_remainder_numerator
        AND NEW.gross_amount_krw = OLD.gross_amount_krw
        AND NEW.next_remainder_numerator = OLD.next_remainder_numerator
        AND NEW.created_at = OLD.created_at,
    OLD.id,
    NULL
);
CREATE TRIGGER tr_living_cost_month_item_no_delete
BEFORE DELETE ON living_cost_month_item FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'living-cost month items are immutable history';

CREATE TRIGGER tr_essential_arrear_transition_only
BEFORE UPDATE ON essential_arrear
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'active'
        AND NEW.status IN ('active', 'paid')
        AND NEW.paid_amount_krw > OLD.paid_amount_krw
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.household_id = OLD.household_id
        AND NEW.living_cost_month_item_id = OLD.living_cost_month_item_id
        AND BINARY NEW.category_key = BINARY OLD.category_key
        AND NEW.due_year_month = OLD.due_year_month
        AND NEW.original_amount_krw = OLD.original_amount_krw
        AND NEW.created_game_day = OLD.created_game_day
        AND NEW.created_at = OLD.created_at
        AND NEW.paid_amount_krw = (
            SELECT COALESCE(SUM(payment.amount_krw), 0)
            FROM essential_arrear_payment AS payment
            WHERE payment.essential_arrear_id = OLD.id AND payment.status = 'applied'
        ),
    OLD.id,
    NULL
);
CREATE TRIGGER tr_essential_arrear_no_delete
BEFORE DELETE ON essential_arrear FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'essential arrears are immutable obligations';

CREATE TRIGGER tr_essential_arrear_payment_transition_only
BEFORE UPDATE ON essential_arrear_payment
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'prepared'
        AND NEW.status = 'applied'
        AND NEW.ledger_transaction_id IS NOT NULL
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.essential_arrear_id = OLD.essential_arrear_id
        AND NEW.payment_no = OLD.payment_no
        AND BINARY NEW.payment_kind = BINARY OLD.payment_kind
        AND NEW.amount_krw = OLD.amount_krw
        AND NEW.game_day = OLD.game_day
        AND NEW.command_id <=> OLD.command_id
        AND NEW.created_at = OLD.created_at,
    OLD.id,
    NULL
);
CREATE TRIGGER tr_essential_arrear_payment_no_delete
BEFORE DELETE ON essential_arrear_payment FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'essential arrear payments are immutable';
