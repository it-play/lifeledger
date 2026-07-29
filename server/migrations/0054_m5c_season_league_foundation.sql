-- M5-C ranked ruleset qualification, season authority, and league definitions (§5.1).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;

CREATE TABLE ranked_ruleset_release (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    release_key                     VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    version_no                      INT UNSIGNED NOT NULL,
    schema_version                  SMALLINT UNSIGNED NOT NULL,
    status                          VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    market_world_id                 BIGINT UNSIGNED NOT NULL,
    policy_set_id                   BIGINT UNSIGNED NOT NULL,
    policy_set_sha256               CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    career_catalog_bundle_id        BIGINT UNSIGNED NOT NULL,
    employment_policy_set_id        BIGINT UNSIGNED NOT NULL,
    life_catalog_set_id             BIGINT UNSIGNED NOT NULL,
    credit_model_version_id         BIGINT UNSIGNED NOT NULL,
    real_estate_model_version_id    BIGINT UNSIGNED NOT NULL,
    content_bundle_id               BIGINT UNSIGNED NOT NULL,
    content_bundle_sha256           CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    engine_version                  VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    verification_evidence_key       VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    verification_evidence_sha256    CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    canonical_manifest_json         LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    release_sha256                  CHAR(64) CHARACTER SET ascii COLLATE ascii_bin
        GENERATED ALWAYS AS (SHA2(canonical_manifest_json, 256)) STORED,
    sealed_at                       DATETIME(3) NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_ranked_ruleset_release_key_version (release_key, version_no),
    UNIQUE KEY uk_ranked_ruleset_release_sha (release_sha256),
    UNIQUE KEY uk_ranked_ruleset_release_id_sha (id, release_sha256),
    KEY ix_ranked_ruleset_release_content (content_bundle_id),
    CONSTRAINT fk_ranked_ruleset_release_market
        FOREIGN KEY (market_world_id) REFERENCES market_world (id),
    CONSTRAINT fk_ranked_ruleset_release_policy
        FOREIGN KEY (policy_set_id) REFERENCES policy_set (id),
    CONSTRAINT fk_ranked_ruleset_release_career
        FOREIGN KEY (career_catalog_bundle_id) REFERENCES career_catalog_bundle (id),
    CONSTRAINT fk_ranked_ruleset_release_employment
        FOREIGN KEY (employment_policy_set_id) REFERENCES employment_policy_set (id),
    CONSTRAINT fk_ranked_ruleset_release_life
        FOREIGN KEY (life_catalog_set_id) REFERENCES life_catalog_set (id),
    CONSTRAINT fk_ranked_ruleset_release_credit
        FOREIGN KEY (credit_model_version_id) REFERENCES credit_model_version (id),
    CONSTRAINT fk_ranked_ruleset_release_real_estate
        FOREIGN KEY (real_estate_model_version_id) REFERENCES real_estate_model_version (id),
    CONSTRAINT fk_ranked_ruleset_release_content
        FOREIGN KEY (content_bundle_id, content_bundle_sha256)
        REFERENCES content_bundle (id, canonical_sha256),
    CONSTRAINT ck_ranked_ruleset_release_identity CHECK (
        release_key REGEXP '^[a-z0-9][a-zA-Z0-9._-]{0,95}$'
        AND version_no > 0
        AND schema_version > 0
    ),
    CONSTRAINT ck_ranked_ruleset_release_status CHECK (status = 'sealed'),
    CONSTRAINT ck_ranked_ruleset_release_sha CHECK (
        policy_set_sha256 REGEXP '^[0-9a-f]{64}$'
        AND content_bundle_sha256 REGEXP '^[0-9a-f]{64}$'
        AND verification_evidence_sha256 REGEXP '^[0-9a-f]{64}$'
    ),
    CONSTRAINT ck_ranked_ruleset_release_manifest CHECK (
        JSON_VALID(canonical_manifest_json)
        AND JSON_TYPE(canonical_manifest_json) = 'OBJECT'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_ranked_ruleset_release_valid_insert
BEFORE INSERT ON ranked_ruleset_release
FOR EACH ROW
SET NEW.release_key = IF(
    NEW.status = 'sealed'
        AND NEW.sealed_at IS NOT NULL
        AND NEW.engine_version = 'm5a-dev-v1'
        AND EXISTS (
            SELECT 1
            FROM market_world AS world
            WHERE world.id = NEW.market_world_id
        )
        AND EXISTS (
            SELECT 1
            FROM policy_set AS policy
            WHERE policy.id = NEW.policy_set_id
              AND BINARY policy.canonical_sha256 = BINARY NEW.policy_set_sha256
              AND policy.sealed_at IS NOT NULL
        )
        AND EXISTS (
            SELECT 1
            FROM career_catalog_bundle AS career
            WHERE career.id = NEW.career_catalog_bundle_id
              AND career.published_at IS NOT NULL
        )
        AND EXISTS (
            SELECT 1
            FROM employment_policy_set AS employment
            WHERE employment.id = NEW.employment_policy_set_id
              AND employment.published_at IS NOT NULL
        )
        AND EXISTS (
            SELECT 1
            FROM life_catalog_set AS life_catalog
            WHERE life_catalog.id = NEW.life_catalog_set_id
              AND life_catalog.sealed_at IS NOT NULL
        )
        AND EXISTS (
            SELECT 1
            FROM credit_model_version AS credit
            WHERE credit.id = NEW.credit_model_version_id
              AND credit.sealed_at IS NOT NULL
        )
        AND EXISTS (
            SELECT 1
            FROM real_estate_model_version AS real_estate
            WHERE real_estate.id = NEW.real_estate_model_version_id
              AND real_estate.sealed_at IS NOT NULL
        )
        AND EXISTS (
            SELECT 1
            FROM content_bundle AS content
            WHERE content.id = NEW.content_bundle_id
              AND BINARY content.canonical_sha256 = BINARY NEW.content_bundle_sha256
              AND content.status = 'sealed'
        )
        AND EXISTS (
            SELECT 1 FROM content_bundle_member AS member
            WHERE member.content_bundle_id = NEW.content_bundle_id
              AND member.authority_kind = 'careerCatalog'
              AND member.authority_id = NEW.career_catalog_bundle_id
        )
        AND EXISTS (
            SELECT 1 FROM content_bundle_member AS member
            WHERE member.content_bundle_id = NEW.content_bundle_id
              AND member.authority_kind = 'employmentPolicy'
              AND member.authority_id = NEW.employment_policy_set_id
        )
        AND EXISTS (
            SELECT 1 FROM content_bundle_member AS member
            WHERE member.content_bundle_id = NEW.content_bundle_id
              AND member.authority_kind = 'lifeCatalog'
              AND member.authority_id = NEW.life_catalog_set_id
        )
        AND EXISTS (
            SELECT 1 FROM content_bundle_member AS member
            WHERE member.content_bundle_id = NEW.content_bundle_id
              AND member.authority_kind = 'creditModel'
              AND member.authority_id = NEW.credit_model_version_id
        )
        AND EXISTS (
            SELECT 1 FROM content_bundle_member AS member
            WHERE member.content_bundle_id = NEW.content_bundle_id
              AND member.authority_kind = 'realEstateModel'
              AND member.authority_id = NEW.real_estate_model_version_id
        ),
    NEW.release_key,
    NULL
);

CREATE TRIGGER tr_ranked_ruleset_release_no_update
BEFORE UPDATE ON ranked_ruleset_release
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'ranked ruleset releases are immutable';

CREATE TRIGGER tr_ranked_ruleset_release_no_delete
BEFORE DELETE ON ranked_ruleset_release
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'ranked ruleset releases are immutable';

CREATE TABLE ranking_rule_version (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    rule_key                    VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    version_no                  INT UNSIGNED NOT NULL,
    schema_version              SMALLINT UNSIGNED NOT NULL,
    status                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    target_game_day             INT UNSIGNED NOT NULL,
    metric                      VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    liquidation_policy_key      VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    canonical_manifest_json     LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    ranking_rule_sha256         CHAR(64) CHARACTER SET ascii COLLATE ascii_bin
        GENERATED ALWAYS AS (SHA2(canonical_manifest_json, 256)) STORED,
    sealed_at                   DATETIME(3) NOT NULL,
    created_at                  DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_ranking_rule_key_version (rule_key, version_no),
    UNIQUE KEY uk_ranking_rule_sha (ranking_rule_sha256),
    UNIQUE KEY uk_ranking_rule_id_sha (id, ranking_rule_sha256),
    CONSTRAINT ck_ranking_rule_identity CHECK (
        rule_key REGEXP '^[a-z0-9][a-zA-Z0-9._-]{0,95}$'
        AND version_no > 0
        AND schema_version > 0
        AND target_game_day > 0
    ),
    CONSTRAINT ck_ranking_rule_status CHECK (status = 'sealed'),
    CONSTRAINT ck_ranking_rule_metric CHECK (metric = 'afterTaxNetWorthKrw'),
    CONSTRAINT ck_ranking_rule_manifest CHECK (
        JSON_VALID(canonical_manifest_json)
        AND JSON_TYPE(canonical_manifest_json) = 'OBJECT'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_ranking_rule_no_update
BEFORE UPDATE ON ranking_rule_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'ranking rule versions are immutable';

CREATE TRIGGER tr_ranking_rule_no_delete
BEFORE DELETE ON ranking_rule_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'ranking rule versions are immutable';

CREATE TABLE season (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    season_key                      VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    version_no                      INT UNSIGNED NOT NULL,
    display_name                    VARCHAR(120) NOT NULL,
    status                          VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    status_revision                 BIGINT UNSIGNED NOT NULL,
    ranked_ruleset_release_id       BIGINT UNSIGNED NOT NULL,
    ranked_ruleset_release_sha256   CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ranking_rule_version_id         BIGINT UNSIGNED NOT NULL,
    ranking_rule_sha256             CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    registration_open_at            DATETIME(6) NOT NULL,
    registration_close_at           DATETIME(6) NOT NULL,
    operation_close_at              DATETIME(6) NOT NULL,
    canonical_manifest_json         LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    season_manifest_sha256          CHAR(64) CHARACTER SET ascii COLLATE ascii_bin
        GENERATED ALWAYS AS (SHA2(canonical_manifest_json, 256)) STORED,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_season_key_version (season_key, version_no),
    UNIQUE KEY uk_season_manifest_sha (season_manifest_sha256),
    UNIQUE KEY uk_season_id_release_rule
        (id, ranked_ruleset_release_id, ranking_rule_version_id),
    CONSTRAINT fk_season_ranked_release
        FOREIGN KEY (ranked_ruleset_release_id, ranked_ruleset_release_sha256)
        REFERENCES ranked_ruleset_release (id, release_sha256),
    CONSTRAINT fk_season_ranking_rule
        FOREIGN KEY (ranking_rule_version_id, ranking_rule_sha256)
        REFERENCES ranking_rule_version (id, ranking_rule_sha256),
    CONSTRAINT ck_season_identity CHECK (
        season_key REGEXP '^[a-z0-9][a-zA-Z0-9._-]{0,95}$'
        AND version_no > 0
        AND CHAR_LENGTH(display_name) > 0
        AND status_revision > 0
    ),
    CONSTRAINT ck_season_status CHECK (
        status IN ('draft', 'registrationOpen', 'active', 'locked', 'finalized', 'archived')
    ),
    CONSTRAINT ck_season_window CHECK (
        registration_open_at < registration_close_at
        AND registration_close_at <= operation_close_at
    ),
    CONSTRAINT ck_season_manifest CHECK (
        JSON_VALID(canonical_manifest_json)
        AND JSON_TYPE(canonical_manifest_json) = 'OBJECT'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_season_valid_insert
BEFORE INSERT ON season
FOR EACH ROW
SET NEW.season_key = IF(
    NEW.status IN ('draft', 'registrationOpen')
        AND NEW.status_revision = 1
        AND NEW.registration_open_at < NEW.registration_close_at
        AND NEW.registration_close_at <= NEW.operation_close_at
        AND EXISTS (
            SELECT 1 FROM ranked_ruleset_release AS release_row
            WHERE release_row.id = NEW.ranked_ruleset_release_id
              AND BINARY release_row.release_sha256
                    = BINARY NEW.ranked_ruleset_release_sha256
              AND release_row.status = 'sealed'
        )
        AND EXISTS (
            SELECT 1 FROM ranking_rule_version AS ranking_rule
            WHERE ranking_rule.id = NEW.ranking_rule_version_id
              AND BINARY ranking_rule.ranking_rule_sha256
                    = BINARY NEW.ranking_rule_sha256
              AND ranking_rule.status = 'sealed'
        ),
    NEW.season_key,
    NULL
);

CREATE TRIGGER tr_season_transition_only
BEFORE UPDATE ON season
FOR EACH ROW
SET NEW.season_key = IF(
    NEW.id = OLD.id
        AND BINARY NEW.season_key = BINARY OLD.season_key
        AND NEW.version_no = OLD.version_no
        AND NEW.display_name = OLD.display_name
        AND NEW.ranked_ruleset_release_id = OLD.ranked_ruleset_release_id
        AND BINARY NEW.ranked_ruleset_release_sha256
            = BINARY OLD.ranked_ruleset_release_sha256
        AND NEW.ranking_rule_version_id = OLD.ranking_rule_version_id
        AND BINARY NEW.ranking_rule_sha256 = BINARY OLD.ranking_rule_sha256
        AND NEW.registration_open_at = OLD.registration_open_at
        AND NEW.registration_close_at = OLD.registration_close_at
        AND NEW.operation_close_at = OLD.operation_close_at
        AND BINARY NEW.canonical_manifest_json = BINARY OLD.canonical_manifest_json
        AND NEW.created_at = OLD.created_at
        AND NEW.status_revision = OLD.status_revision + 1
        AND (
            (OLD.status = 'draft' AND NEW.status = 'registrationOpen')
            OR (OLD.status = 'registrationOpen' AND NEW.status IN ('active', 'locked'))
            OR (OLD.status = 'active' AND NEW.status = 'locked')
            OR (OLD.status = 'locked' AND NEW.status = 'finalized')
            OR (OLD.status = 'finalized' AND NEW.status = 'archived')
        ),
    OLD.season_key,
    NULL
);

CREATE TRIGGER tr_season_no_delete
BEFORE DELETE ON season
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'seasons are immutable';

CREATE TABLE league_definition (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    season_id                       BIGINT UNSIGNED NOT NULL,
    league_key                      VARCHAR(128) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                    VARCHAR(120) NOT NULL,
    mode                            VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    character_preset_version_id     BIGINT UNSIGNED NULL,
    point_budget_version_id         BIGINT UNSIGNED NULL,
    minimum_participants            INT UNSIGNED NOT NULL,
    display_order                   INT UNSIGNED NOT NULL,
    canonical_manifest_json         LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    league_manifest_sha256          CHAR(64) CHARACTER SET ascii COLLATE ascii_bin
        GENERATED ALWAYS AS (SHA2(canonical_manifest_json, 256)) STORED,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_league_season_key (season_id, league_key),
    UNIQUE KEY uk_league_manifest_sha (league_manifest_sha256),
    UNIQUE KEY uk_league_season_id (season_id, id),
    KEY ix_league_preset (character_preset_version_id),
    KEY ix_league_budget (point_budget_version_id),
    CONSTRAINT fk_league_season FOREIGN KEY (season_id) REFERENCES season (id),
    CONSTRAINT fk_league_preset
        FOREIGN KEY (character_preset_version_id) REFERENCES character_preset_version (id),
    CONSTRAINT fk_league_budget
        FOREIGN KEY (point_budget_version_id) REFERENCES point_budget_version (id),
    CONSTRAINT ck_league_identity CHECK (
        league_key REGEXP '^[a-z0-9][a-zA-Z0-9._-]{0,127}$'
        AND CHAR_LENGTH(display_name) > 0
        AND minimum_participants > 0
    ),
    CONSTRAINT ck_league_mode CHECK (
        (
            mode = 'rankedPreset'
            AND character_preset_version_id IS NOT NULL
            AND point_budget_version_id IS NULL
        )
        OR (
            mode = 'rankedCustom'
            AND character_preset_version_id IS NULL
            AND point_budget_version_id IS NOT NULL
        )
    ),
    CONSTRAINT ck_league_manifest CHECK (
        JSON_VALID(canonical_manifest_json)
        AND JSON_TYPE(canonical_manifest_json) = 'OBJECT'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_league_definition_valid_insert
BEFORE INSERT ON league_definition
FOR EACH ROW
SET NEW.league_key = IF(
    EXISTS (
        SELECT 1
        FROM season AS season_row
        INNER JOIN ranked_ruleset_release AS release_row
            ON release_row.id = season_row.ranked_ruleset_release_id
        INNER JOIN content_bundle_member AS member
            ON member.content_bundle_id = release_row.content_bundle_id
        WHERE season_row.id = NEW.season_id
          AND (
              (
                  NEW.mode = 'rankedPreset'
                  AND NEW.character_preset_version_id IS NOT NULL
                  AND NEW.point_budget_version_id IS NULL
                  AND member.authority_kind = 'characterPreset'
                  AND member.authority_id = NEW.character_preset_version_id
              )
              OR (
                  NEW.mode = 'rankedCustom'
                  AND NEW.character_preset_version_id IS NULL
                  AND NEW.point_budget_version_id IS NOT NULL
                  AND member.authority_kind = 'pointBudget'
                  AND member.authority_id = NEW.point_budget_version_id
              )
          )
    ),
    NEW.league_key,
    NULL
);

CREATE TRIGGER tr_league_definition_no_update
BEFORE UPDATE ON league_definition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'league definitions are immutable';

CREATE TRIGGER tr_league_definition_no_delete
BEFORE DELETE ON league_definition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'league definitions are immutable';

CREATE TABLE season_assignment (
    assignment_key          VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    season_id               BIGINT UNSIGNED NOT NULL,
    assignment_revision     BIGINT UNSIGNED NOT NULL,
    updated_at              DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (assignment_key),
    KEY ix_season_assignment_season (season_id),
    CONSTRAINT fk_season_assignment_season FOREIGN KEY (season_id) REFERENCES season (id),
    CONSTRAINT ck_season_assignment CHECK (
        assignment_key = 'rankedRun' AND assignment_revision > 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_season_assignment_valid_insert
BEFORE INSERT ON season_assignment
FOR EACH ROW
SET
    NEW.assignment_key = IF(
        NEW.assignment_key = 'rankedRun'
            AND NEW.assignment_revision = 1
            AND EXISTS (
                SELECT 1 FROM season AS season_row
                WHERE season_row.id = NEW.season_id
                  AND season_row.status IN ('registrationOpen', 'active')
            ),
        NEW.assignment_key,
        NULL
    ),
    NEW.assignment_revision = 1;

CREATE TRIGGER tr_season_assignment_bump_revision
BEFORE UPDATE ON season_assignment
FOR EACH ROW
SET
    NEW.assignment_key = IF(
        BINARY NEW.assignment_key = BINARY OLD.assignment_key
            AND NEW.season_id <> OLD.season_id
            AND NEW.assignment_revision = OLD.assignment_revision
            AND EXISTS (
                SELECT 1 FROM season AS season_row
                WHERE season_row.id = NEW.season_id
                  AND season_row.status IN ('registrationOpen', 'active')
            ),
        OLD.assignment_key,
        NULL
    ),
    NEW.assignment_revision = OLD.assignment_revision + 1;

CREATE TRIGGER tr_season_assignment_no_delete
BEFORE DELETE ON season_assignment
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'season assignment must be updated in place';

INSERT INTO ranked_ruleset_release
    (
        release_key, version_no, schema_version, status,
        market_world_id, policy_set_id, policy_set_sha256,
        career_catalog_bundle_id, employment_policy_set_id,
        life_catalog_set_id, credit_model_version_id, real_estate_model_version_id,
        content_bundle_id, content_bundle_sha256, engine_version,
        verification_evidence_key, verification_evidence_sha256,
        canonical_manifest_json, sealed_at
    )
SELECT
    'm5c-ranked-ruleset-2026', 1, 1, 'sealed',
    assignment.market_world_id, assignment.policy_set_id, policy.canonical_sha256,
    assignment.career_catalog_bundle_id, assignment.employment_policy_set_id,
    assignment.life_catalog_set_id, assignment.credit_model_version_id,
    assignment.real_estate_model_version_id,
    content.id, content.canonical_sha256, 'm5a-dev-v1',
    'm4-paired-30y-2026-07-29',
    'a1021ed3a8b9e49416a25b1fdfe6b9138a42bfacabe4fffcebfcd993378692ee',
    CONCAT(
        '{"careerCatalogBundleId":',
            JSON_QUOTE(CAST(assignment.career_catalog_bundle_id AS CHAR)),
        ',"contentBundleId":', JSON_QUOTE(CAST(content.id AS CHAR)),
        ',"contentBundleSha256":', JSON_QUOTE(content.canonical_sha256),
        ',"creditModelVersionId":',
            JSON_QUOTE(CAST(assignment.credit_model_version_id AS CHAR)),
        ',"employmentPolicySetId":',
            JSON_QUOTE(CAST(assignment.employment_policy_set_id AS CHAR)),
        ',"engineVersion":"m5a-dev-v1"',
        ',"evidenceKey":"m4-paired-30y-2026-07-29"',
        ',"evidenceSha256":"a1021ed3a8b9e49416a25b1fdfe6b9138a42bfacabe4fffcebfcd993378692ee"',
        ',"lifeCatalogSetId":', JSON_QUOTE(CAST(assignment.life_catalog_set_id AS CHAR)),
        ',"marketWorldId":', JSON_QUOTE(CAST(assignment.market_world_id AS CHAR)),
        ',"policySetId":', JSON_QUOTE(CAST(assignment.policy_set_id AS CHAR)),
        ',"policySetSha256":', JSON_QUOTE(policy.canonical_sha256),
        ',"realEstateModelVersionId":',
            JSON_QUOTE(CAST(assignment.real_estate_model_version_id AS CHAR)),
        ',"releaseKey":"m5c-ranked-ruleset-2026"',
        ',"schemaVersion":1',
        ',"version":1}'
    ),
    CURRENT_TIMESTAMP(3)
FROM run_rule_bundle_assignment AS assignment
INNER JOIN policy_set AS policy ON policy.id = assignment.policy_set_id
INNER JOIN content_bundle_assignment AS content_assignment
    ON content_assignment.assignment_key = 'newRun'
INNER JOIN content_bundle AS content
    ON content.id = content_assignment.content_bundle_id
   AND content.status = 'sealed'
WHERE assignment.assignment_key = 'newRun'
  AND policy.sealed_at IS NOT NULL;

INSERT INTO ranking_rule_version
    (
        rule_key, version_no, schema_version, status, target_game_day, metric,
        liquidation_policy_key, canonical_manifest_json, sealed_at
    )
VALUES
    (
        'm5c-after-tax-net-worth-2026', 1, 1, 'sealed', 10950,
        'afterTaxNetWorthKrw', 'm5c-after-tax-liquidation-v1',
        '{"liquidationPolicyKey":"m5c-after-tax-liquidation-v1","metric":"afterTaxNetWorthKrw","ruleKey":"m5c-after-tax-net-worth-2026","schemaVersion":1,"targetGameDay":10950,"tieBreakers":["insolvencyDaysAsc","playerCommandCountAsc","runIdAsc"],"version":1}',
        CURRENT_TIMESTAMP(3)
    );

INSERT INTO season
    (
        season_key, version_no, display_name, status, status_revision,
        ranked_ruleset_release_id, ranked_ruleset_release_sha256,
        ranking_rule_version_id, ranking_rule_sha256,
        registration_open_at, registration_close_at, operation_close_at,
        canonical_manifest_json
    )
SELECT
    'm5c-2026-s1', 1, '2026 시즌 1', 'registrationOpen', 1,
    release_row.id, release_row.release_sha256,
    ranking_rule.id, ranking_rule.ranking_rule_sha256,
    '2026-07-29 00:00:00.000000',
    '2027-07-29 00:00:00.000000',
    '2030-07-29 00:00:00.000000',
    CONCAT(
        '{"displayName":"2026 시즌 1"',
        ',"operationCloseAt":"2030-07-29T00:00:00Z"',
        ',"rankedRulesetReleaseId":', JSON_QUOTE(CAST(release_row.id AS CHAR)),
        ',"rankedRulesetReleaseSha256":', JSON_QUOTE(release_row.release_sha256),
        ',"rankingRuleVersionId":', JSON_QUOTE(CAST(ranking_rule.id AS CHAR)),
        ',"rankingRuleSha256":', JSON_QUOTE(ranking_rule.ranking_rule_sha256),
        ',"registrationCloseAt":"2027-07-29T00:00:00Z"',
        ',"registrationOpenAt":"2026-07-29T00:00:00Z"',
        ',"schemaVersion":1',
        ',"seasonKey":"m5c-2026-s1"',
        ',"version":1}'
    )
FROM ranked_ruleset_release AS release_row
CROSS JOIN ranking_rule_version AS ranking_rule
WHERE release_row.release_key = 'm5c-ranked-ruleset-2026'
  AND release_row.version_no = 1
  AND ranking_rule.rule_key = 'm5c-after-tax-net-worth-2026'
  AND ranking_rule.version_no = 1;

INSERT INTO league_definition
    (
        season_id, league_key, display_name, mode,
        character_preset_version_id, point_budget_version_id,
        minimum_participants, display_order, canonical_manifest_json
    )
SELECT
    season_row.id,
    CONCAT('preset.', preset.preset_key, '.v', preset.version_no),
    CONCAT(preset.display_name, ' 리그'),
    'rankedPreset', preset.id, NULL, 2,
    FIELD(
        preset.preset_key,
        'rookie', 'early-start', 'late-start', 'supported', 'restart'
    ),
    CONCAT(
        '{"characterPresetVersionId":', JSON_QUOTE(CAST(preset.id AS CHAR)),
        ',"leagueKey":',
            JSON_QUOTE(CONCAT('preset.', preset.preset_key, '.v', preset.version_no)),
        ',"minimumParticipants":2',
        ',"mode":"rankedPreset"',
        ',"pointBudgetVersionId":null',
        ',"schemaVersion":1',
        ',"seasonId":', JSON_QUOTE(CAST(season_row.id AS CHAR)),
        '}'
    )
FROM season AS season_row
INNER JOIN ranked_ruleset_release AS release_row
    ON release_row.id = season_row.ranked_ruleset_release_id
INNER JOIN content_bundle_member AS member
    ON member.content_bundle_id = release_row.content_bundle_id
   AND member.authority_kind = 'characterPreset'
INNER JOIN character_preset_version AS preset ON preset.id = member.authority_id
WHERE season_row.season_key = 'm5c-2026-s1'
  AND season_row.version_no = 1
ORDER BY FIELD(
    preset.preset_key,
    'rookie', 'early-start', 'late-start', 'supported', 'restart'
), preset.id;

INSERT INTO league_definition
    (
        season_id, league_key, display_name, mode,
        character_preset_version_id, point_budget_version_id,
        minimum_participants, display_order, canonical_manifest_json
    )
SELECT
    season_row.id,
    CONCAT('custom.', budget.budget_key, '.v', budget.version_no),
    CONCAT(budget.display_name, ' 리그'),
    'rankedCustom', NULL, budget.id, 2, 1000 + budget.version_no,
    CONCAT(
        '{"characterPresetVersionId":null',
        ',"leagueKey":',
            JSON_QUOTE(CONCAT('custom.', budget.budget_key, '.v', budget.version_no)),
        ',"minimumParticipants":2',
        ',"mode":"rankedCustom"',
        ',"pointBudgetVersionId":', JSON_QUOTE(CAST(budget.id AS CHAR)),
        ',"schemaVersion":1',
        ',"seasonId":', JSON_QUOTE(CAST(season_row.id AS CHAR)),
        '}'
    )
FROM season AS season_row
INNER JOIN ranked_ruleset_release AS release_row
    ON release_row.id = season_row.ranked_ruleset_release_id
INNER JOIN content_bundle_member AS member
    ON member.content_bundle_id = release_row.content_bundle_id
   AND member.authority_kind = 'pointBudget'
INNER JOIN point_budget_version AS budget ON budget.id = member.authority_id
WHERE season_row.season_key = 'm5c-2026-s1'
  AND season_row.version_no = 1
ORDER BY budget.version_no, budget.id;

INSERT INTO season_assignment (assignment_key, season_id, assignment_revision)
SELECT 'rankedRun', id, 1
FROM season
WHERE season_key = 'm5c-2026-s1' AND version_no = 1;

ALTER TABLE run_manifest
    ADD COLUMN season_assignment_revision BIGINT UNSIGNED NULL
        AFTER league_definition_id,
    ADD COLUMN ranked_ruleset_release_id BIGINT UNSIGNED NULL
        AFTER season_assignment_revision,
    ADD COLUMN ranked_ruleset_release_sha256
        CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL
        AFTER ranked_ruleset_release_id,
    ADD COLUMN ranking_rule_version_id BIGINT UNSIGNED NULL
        AFTER ranked_ruleset_release_sha256,
    ADD COLUMN ranking_rule_sha256 CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL
        AFTER ranking_rule_version_id,
    ADD KEY ix_run_manifest_ranked_release (ranked_ruleset_release_id),
    ADD KEY ix_run_manifest_ranking_rule (ranking_rule_version_id),
    ADD CONSTRAINT fk_run_manifest_season FOREIGN KEY (season_id) REFERENCES season (id),
    ADD CONSTRAINT fk_run_manifest_league
        FOREIGN KEY (season_id, league_definition_id)
        REFERENCES league_definition (season_id, id),
    ADD CONSTRAINT fk_run_manifest_ranked_release
        FOREIGN KEY (ranked_ruleset_release_id, ranked_ruleset_release_sha256)
        REFERENCES ranked_ruleset_release (id, release_sha256),
    ADD CONSTRAINT fk_run_manifest_ranking_rule
        FOREIGN KEY (ranking_rule_version_id, ranking_rule_sha256)
        REFERENCES ranking_rule_version (id, ranking_rule_sha256),
    ADD CONSTRAINT ck_run_manifest_ranked_authority_shape CHECK (
        (
            mode = 'sandbox'
            AND season_assignment_revision IS NULL
            AND ranked_ruleset_release_id IS NULL
            AND ranked_ruleset_release_sha256 IS NULL
            AND ranking_rule_version_id IS NULL
            AND ranking_rule_sha256 IS NULL
        )
        OR (
            mode IN ('rankedPreset', 'rankedCustom')
            AND season_assignment_revision IS NOT NULL
            AND season_assignment_revision > 0
            AND ranked_ruleset_release_id IS NOT NULL
            AND ranked_ruleset_release_sha256 REGEXP '^[0-9a-f]{64}$'
            AND ranking_rule_version_id IS NOT NULL
            AND ranking_rule_sha256 REGEXP '^[0-9a-f]{64}$'
        )
    );

CREATE TRIGGER tr_run_manifest_m5c_ranked_valid_insert
BEFORE INSERT ON run_manifest
FOR EACH ROW
FOLLOWS tr_run_manifest_content_bundle_valid_insert
SET NEW.mode = IF(
    (
        NEW.mode = 'sandbox'
        AND NEW.season_id IS NULL
        AND NEW.league_definition_id IS NULL
        AND NEW.season_assignment_revision IS NULL
        AND NEW.ranked_ruleset_release_id IS NULL
        AND NEW.ranked_ruleset_release_sha256 IS NULL
        AND NEW.ranking_rule_version_id IS NULL
        AND NEW.ranking_rule_sha256 IS NULL
    )
    OR (
        NEW.mode IN ('rankedPreset', 'rankedCustom')
        AND NEW.ranking_eligible = TRUE
        AND NEW.ranking_ineligibility_reason IS NULL
        AND EXISTS (
            SELECT 1
            FROM season_assignment AS season_assignment_row
            INNER JOIN season AS season_row
                ON season_row.id = season_assignment_row.season_id
            INNER JOIN ranked_ruleset_release AS release_row
                ON release_row.id = season_row.ranked_ruleset_release_id
               AND BINARY release_row.release_sha256
                    = BINARY season_row.ranked_ruleset_release_sha256
            INNER JOIN ranking_rule_version AS ranking_rule
                ON ranking_rule.id = season_row.ranking_rule_version_id
               AND BINARY ranking_rule.ranking_rule_sha256
                    = BINARY season_row.ranking_rule_sha256
            INNER JOIN league_definition AS league
                ON league.season_id = season_row.id
               AND league.id = NEW.league_definition_id
            WHERE season_assignment_row.assignment_key = 'rankedRun'
              AND season_row.id = NEW.season_id
              AND season_assignment_row.assignment_revision
                    = NEW.season_assignment_revision
              AND season_row.status IN ('registrationOpen', 'active')
              AND CURRENT_TIMESTAMP(6) >= season_row.registration_open_at
              AND CURRENT_TIMESTAMP(6) < season_row.registration_close_at
              AND release_row.id = NEW.ranked_ruleset_release_id
              AND BINARY release_row.release_sha256
                    = BINARY NEW.ranked_ruleset_release_sha256
              AND ranking_rule.id = NEW.ranking_rule_version_id
              AND BINARY ranking_rule.ranking_rule_sha256
                    = BINARY NEW.ranking_rule_sha256
              AND release_row.market_world_id = NEW.market_world_id
              AND release_row.policy_set_id = NEW.policy_set_id
              AND release_row.career_catalog_bundle_id = NEW.career_catalog_bundle_id
              AND release_row.employment_policy_set_id = NEW.employment_policy_set_id
              AND release_row.life_catalog_set_id = NEW.life_catalog_set_id
              AND release_row.credit_model_version_id = NEW.credit_model_version_id
              AND release_row.real_estate_model_version_id = NEW.real_estate_model_version_id
              AND release_row.content_bundle_id = NEW.content_bundle_id
              AND BINARY release_row.content_bundle_sha256
                    = BINARY NEW.content_bundle_sha256
              AND BINARY release_row.engine_version = BINARY NEW.engine_version
              AND ranking_rule.target_game_day = NEW.target_game_day
              AND (
                  (
                      NEW.mode = 'rankedPreset'
                      AND league.mode = 'rankedPreset'
                      AND league.character_preset_version_id
                            = NEW.character_preset_version_id
                      AND NEW.point_budget_version_id IS NULL
                  )
                  OR (
                      NEW.mode = 'rankedCustom'
                      AND league.mode = 'rankedCustom'
                      AND league.point_budget_version_id = NEW.point_budget_version_id
                      AND NEW.character_preset_version_id IS NULL
                  )
              )
        )
    ),
    NEW.mode,
    NULL
);
