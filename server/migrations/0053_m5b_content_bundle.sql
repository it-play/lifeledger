-- M5-B immutable content publication bundle and new-run pin (§4.1, §10).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;
SET SESSION group_concat_max_len = 1048576;

-- Fail before permanent DDL if the reviewed M3/M4/M5-A development authorities moved.
CREATE TEMPORARY TABLE m5b_source_guard (
    guard_key  VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    accepted   TINYINT UNSIGNED NOT NULL,
    PRIMARY KEY (guard_key),
    CONSTRAINT ck_m5b_source_guard CHECK (accepted = 1)
);

INSERT INTO m5b_source_guard (guard_key, accepted)
SELECT 'active-authorities', IF(
    EXISTS (
        SELECT 1
        FROM career_catalog_assignment AS assignment
        INNER JOIN career_catalog_bundle AS bundle
            ON bundle.id = assignment.career_catalog_bundle_id
        WHERE assignment.assignment_key = 'newRun'
          AND bundle.bundle_key = 'dev-unranked-m3-v1'
          AND bundle.ranked_eligible = FALSE
          AND bundle.published_at IS NOT NULL
    )
    AND EXISTS (
        SELECT 1
        FROM career_catalog_assignment AS career_assignment
        INNER JOIN recruitment_ruleset_assignment AS assignment
            ON assignment.career_catalog_bundle_id = career_assignment.career_catalog_bundle_id
           AND assignment.assignment_key = 'newPosting'
        INNER JOIN recruitment_ruleset AS ruleset
            ON ruleset.id = assignment.recruitment_ruleset_id
        WHERE career_assignment.assignment_key = 'newRun'
          AND ruleset.ruleset_key = 'dev-unranked-m3-recruitment-v1'
          AND ruleset.ranked_eligible = FALSE
          AND ruleset.published_at IS NOT NULL
    )
    AND EXISTS (
        SELECT 1
        FROM employment_policy_assignment AS assignment
        INNER JOIN employment_policy_set AS policy
            ON policy.id = assignment.employment_policy_set_id
        WHERE assignment.assignment_key = 'newRun'
          AND policy.policy_key = 'dev-unranked-m3-employment-2026-v1'
          AND policy.ranked_eligible = FALSE
          AND policy.published_at IS NOT NULL
    )
    AND EXISTS (
        SELECT 1
        FROM run_rule_bundle_assignment AS assignment
        INNER JOIN life_catalog_set AS catalog
            ON catalog.id = assignment.life_catalog_set_id
        INNER JOIN credit_model_version AS credit
            ON credit.id = assignment.credit_model_version_id
        INNER JOIN real_estate_model_version AS real_estate
            ON real_estate.id = assignment.real_estate_model_version_id
        WHERE assignment.assignment_key = 'newRun'
          AND catalog.catalog_key = 'dev-unranked-m4-life-corporation-2026-v6'
          AND catalog.ranked_eligible = FALSE
          AND catalog.canonical_sha256
                = '7638c3900d5f2ad8bb6b726433a405ba2211189e0da7628155701205dabea2f0'
          AND catalog.sealed_at IS NOT NULL
          AND credit.version_key = 'dev-unranked-m4c3-credit-2026-v4'
          AND credit.ranked_eligible = FALSE
          AND credit.canonical_sha256
                = 'd878df2c179dc52557e18ba922a4c1e0e9f80c485ebc772f0c494f80928e6cf8'
          AND credit.sealed_at IS NOT NULL
          AND real_estate.version_key = 'dev-unranked-m4-real-estate-sale-tax-2026-v6'
          AND real_estate.ranked_eligible = FALSE
          AND real_estate.canonical_sha256
                = 'fe870274ed964116a1e07cb7d9d543a043188c2479945f236d3b7f525d1ea541'
          AND real_estate.sealed_at IS NOT NULL
    )
    AND (
        SELECT COUNT(*)
        FROM character_preset_version
        WHERE status = 'sealed'
    ) = 5
    AND (
        SELECT COUNT(*)
        FROM character_preset_version
        WHERE status = 'sealed'
          AND version_no = 1
          AND preset_key IN ('early-start', 'late-start', 'restart', 'rookie', 'supported')
    ) = 5
    AND EXISTS (
        SELECT 1
        FROM point_budget_assignment AS assignment
        INNER JOIN point_budget_version AS budget
            ON budget.id = assignment.point_budget_version_id
        WHERE assignment.assignment_key = 'newRun'
          AND budget.budget_key = 'dev-unranked-custom-2026'
          AND budget.version_no = 1
          AND budget.status = 'sealed'
          AND budget.ranked_eligible = FALSE
          AND budget.canonical_sha256
                = 'a340fcc4d1d7cd501b75fcb5f739b6d955d987352224af5da2d5be02636ca553'
    ),
    1,
    0
);

DROP TEMPORARY TABLE m5b_source_guard;

CREATE TABLE content_bundle (
    id                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    bundle_key          VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    version_no          INT UNSIGNED NOT NULL,
    schema_version      SMALLINT UNSIGNED NOT NULL,
    status              VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ranked_eligible     BOOLEAN NOT NULL DEFAULT FALSE,
    source_note         VARCHAR(512) NOT NULL,
    canonical_sha256    CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    sealed_at           DATETIME(3) NULL,
    retired_at          DATETIME(3) NULL,
    created_at          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_content_bundle_key_version (bundle_key, version_no),
    UNIQUE KEY uk_content_bundle_sha (canonical_sha256),
    UNIQUE KEY uk_content_bundle_id_sha (id, canonical_sha256),
    CONSTRAINT ck_content_bundle_key CHECK (
        bundle_key REGEXP '^[a-z0-9][a-zA-Z0-9._-]{0,95}$'
    ),
    CONSTRAINT ck_content_bundle_versions CHECK (version_no > 0 AND schema_version > 0),
    CONSTRAINT ck_content_bundle_status CHECK (status IN ('draft', 'sealed', 'retired')),
    CONSTRAINT ck_content_bundle_ranked CHECK (ranked_eligible IN (FALSE, TRUE)),
    CONSTRAINT ck_content_bundle_source CHECK (CHAR_LENGTH(source_note) > 0),
    CONSTRAINT ck_content_bundle_lifecycle CHECK (
        (
            status = 'draft'
            AND canonical_sha256 IS NULL
            AND sealed_at IS NULL
            AND retired_at IS NULL
        )
        OR (
            status = 'sealed'
            AND canonical_sha256 REGEXP '^[0-9a-f]{64}$'
            AND sealed_at IS NOT NULL
            AND retired_at IS NULL
        )
        OR (
            status = 'retired'
            AND canonical_sha256 REGEXP '^[0-9a-f]{64}$'
            AND sealed_at IS NOT NULL
            AND retired_at IS NOT NULL
            AND retired_at >= sealed_at
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE content_bundle_member (
    content_bundle_id       BIGINT UNSIGNED NOT NULL,
    authority_kind         VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    authority_id           BIGINT UNSIGNED NOT NULL,
    authority_key          VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    authority_version      INT UNSIGNED NOT NULL,
    authority_sha256       CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    source_note            VARCHAR(512) NOT NULL,
    created_at             DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (content_bundle_id, authority_kind, authority_id),
    UNIQUE KEY uk_content_bundle_member_key_version
        (content_bundle_id, authority_kind, authority_key, authority_version),
    UNIQUE KEY uk_content_bundle_member_sha (content_bundle_id, authority_sha256),
    CONSTRAINT fk_content_bundle_member_bundle
        FOREIGN KEY (content_bundle_id) REFERENCES content_bundle (id),
    CONSTRAINT ck_content_bundle_member_kind CHECK (
        authority_kind IN (
            'careerCatalog', 'recruitmentRuleset', 'employmentPolicy',
            'lifeCatalog', 'creditModel', 'realEstateModel',
            'characterPreset', 'pointBudget'
        )
    ),
    CONSTRAINT ck_content_bundle_member_identity CHECK (
        authority_id > 0
        AND authority_version > 0
        AND authority_key REGEXP '^[a-z0-9][a-zA-Z0-9._-]{0,95}$'
    ),
    CONSTRAINT ck_content_bundle_member_sha CHECK (
        authority_sha256 IS NULL OR authority_sha256 REGEXP '^[0-9a-f]{64}$'
    ),
    CONSTRAINT ck_content_bundle_member_source CHECK (CHAR_LENGTH(source_note) > 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE content_bundle_canonical_manifest (
    content_bundle_id   BIGINT UNSIGNED NOT NULL,
    canonical_json      LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    canonical_sha256    CHAR(64) CHARACTER SET ascii COLLATE ascii_bin
        GENERATED ALWAYS AS (SHA2(canonical_json, 256)) STORED,
    created_at          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (content_bundle_id),
    UNIQUE KEY uk_content_bundle_manifest_sha (canonical_sha256),
    CONSTRAINT fk_content_bundle_manifest_bundle
        FOREIGN KEY (content_bundle_id) REFERENCES content_bundle (id),
    CONSTRAINT ck_content_bundle_manifest_json CHECK (
        JSON_VALID(canonical_json) AND JSON_TYPE(canonical_json) = 'OBJECT'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE content_bundle_assignment (
    assignment_key          VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    content_bundle_id       BIGINT UNSIGNED NOT NULL,
    assignment_revision     BIGINT UNSIGNED NOT NULL,
    updated_at              DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (assignment_key),
    KEY ix_content_bundle_assignment_bundle (content_bundle_id),
    CONSTRAINT fk_content_bundle_assignment_bundle
        FOREIGN KEY (content_bundle_id) REFERENCES content_bundle (id),
    CONSTRAINT ck_content_bundle_assignment CHECK (
        assignment_key = 'newRun' AND assignment_revision > 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_content_bundle_draft_insert
BEFORE INSERT ON content_bundle
FOR EACH ROW
SET NEW.bundle_key = IF(
    NEW.status = 'draft'
        AND NEW.canonical_sha256 IS NULL
        AND NEW.sealed_at IS NULL
        AND NEW.retired_at IS NULL,
    NEW.bundle_key,
    NULL
);

CREATE TRIGGER tr_content_bundle_member_draft_insert
BEFORE INSERT ON content_bundle_member
FOR EACH ROW
SET NEW.authority_id = IF(
    EXISTS (
        SELECT 1 FROM content_bundle
        WHERE id = NEW.content_bundle_id AND status = 'draft'
    )
    AND (
        (
            NEW.authority_kind = 'careerCatalog'
            AND NEW.authority_version = 1
            AND NEW.authority_sha256 IS NULL
            AND EXISTS (
                SELECT 1 FROM career_catalog_bundle AS authority
                WHERE authority.id = NEW.authority_id
                  AND BINARY authority.bundle_key = BINARY NEW.authority_key
                  AND authority.published_at IS NOT NULL
                  AND authority.ranked_eligible = FALSE
            )
        )
        OR (
            NEW.authority_kind = 'recruitmentRuleset'
            AND NEW.authority_version = 1
            AND NEW.authority_sha256 IS NULL
            AND EXISTS (
                SELECT 1 FROM recruitment_ruleset AS authority
                WHERE authority.id = NEW.authority_id
                  AND BINARY authority.ruleset_key = BINARY NEW.authority_key
                  AND authority.published_at IS NOT NULL
                  AND authority.ranked_eligible = FALSE
            )
        )
        OR (
            NEW.authority_kind = 'employmentPolicy'
            AND NEW.authority_version = 1
            AND NEW.authority_sha256 IS NULL
            AND EXISTS (
                SELECT 1 FROM employment_policy_set AS authority
                WHERE authority.id = NEW.authority_id
                  AND BINARY authority.policy_key = BINARY NEW.authority_key
                  AND authority.published_at IS NOT NULL
                  AND authority.ranked_eligible = FALSE
            )
        )
        OR (
            NEW.authority_kind = 'lifeCatalog'
            AND NEW.authority_version
                = CAST(SUBSTRING_INDEX(NEW.authority_key, '-v', -1) AS UNSIGNED)
            AND EXISTS (
                SELECT 1 FROM life_catalog_set AS authority
                WHERE authority.id = NEW.authority_id
                  AND BINARY authority.catalog_key = BINARY NEW.authority_key
                  AND BINARY authority.canonical_sha256 = BINARY NEW.authority_sha256
                  AND authority.sealed_at IS NOT NULL
            )
        )
        OR (
            NEW.authority_kind = 'creditModel'
            AND NEW.authority_version
                = CAST(SUBSTRING_INDEX(NEW.authority_key, '-v', -1) AS UNSIGNED)
            AND EXISTS (
                SELECT 1 FROM credit_model_version AS authority
                WHERE authority.id = NEW.authority_id
                  AND BINARY authority.version_key = BINARY NEW.authority_key
                  AND BINARY authority.canonical_sha256 = BINARY NEW.authority_sha256
                  AND authority.sealed_at IS NOT NULL
            )
        )
        OR (
            NEW.authority_kind = 'realEstateModel'
            AND NEW.authority_version
                = CAST(SUBSTRING_INDEX(NEW.authority_key, '-v', -1) AS UNSIGNED)
            AND EXISTS (
                SELECT 1 FROM real_estate_model_version AS authority
                WHERE authority.id = NEW.authority_id
                  AND BINARY authority.version_key = BINARY NEW.authority_key
                  AND BINARY authority.canonical_sha256 = BINARY NEW.authority_sha256
                  AND authority.sealed_at IS NOT NULL
            )
        )
        OR (
            NEW.authority_kind = 'characterPreset'
            AND EXISTS (
                SELECT 1 FROM character_preset_version AS authority
                WHERE authority.id = NEW.authority_id
                  AND BINARY authority.preset_key = BINARY NEW.authority_key
                  AND authority.version_no = NEW.authority_version
                  AND BINARY authority.canonical_sha256 = BINARY NEW.authority_sha256
                  AND authority.status = 'sealed'
            )
        )
        OR (
            NEW.authority_kind = 'pointBudget'
            AND EXISTS (
                SELECT 1 FROM point_budget_version AS authority
                WHERE authority.id = NEW.authority_id
                  AND BINARY authority.budget_key = BINARY NEW.authority_key
                  AND authority.version_no = NEW.authority_version
                  AND BINARY authority.canonical_sha256 = BINARY NEW.authority_sha256
                  AND authority.status = 'sealed'
            )
        )
    ),
    NEW.authority_id,
    NULL
);

CREATE TRIGGER tr_content_bundle_member_no_update
BEFORE UPDATE ON content_bundle_member
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'content bundle members are immutable';

CREATE TRIGGER tr_content_bundle_member_no_delete
BEFORE DELETE ON content_bundle_member
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'content bundle members are immutable';

CREATE TRIGGER tr_content_bundle_manifest_draft_insert
BEFORE INSERT ON content_bundle_canonical_manifest
FOR EACH ROW
SET NEW.content_bundle_id = IF(
    JSON_VALID(NEW.canonical_json)
        AND EXISTS (
            SELECT 1 FROM content_bundle
            WHERE id = NEW.content_bundle_id AND status = 'draft'
        )
        AND BINARY NEW.canonical_json = BINARY (
            SELECT CONCAT(
                '{"bundleKey":', JSON_QUOTE(bundle.bundle_key),
                ',"members":[',
                GROUP_CONCAT(
                    CONCAT(
                        '{"authorityId":', JSON_QUOTE(CAST(member.authority_id AS CHAR)),
                        ',"authorityKey":', JSON_QUOTE(member.authority_key),
                        ',"authorityKind":', JSON_QUOTE(member.authority_kind),
                        ',"authoritySha256":',
                            IF(member.authority_sha256 IS NULL,
                               'null', JSON_QUOTE(member.authority_sha256)),
                        ',"authorityVersion":', member.authority_version,
                        ',"sourceNote":', JSON_QUOTE(member.source_note),
                        '}'
                    )
                    ORDER BY
                        FIELD(
                            member.authority_kind,
                            'careerCatalog', 'recruitmentRuleset', 'employmentPolicy',
                            'lifeCatalog', 'creditModel', 'realEstateModel',
                            'characterPreset', 'pointBudget'
                        ),
                        BINARY member.authority_key,
                        member.authority_version,
                        member.authority_id
                    SEPARATOR ','
                ),
                '],"rankedEligible":', IF(bundle.ranked_eligible, 'true', 'false'),
                ',"schemaVersion":', bundle.schema_version,
                ',"sourceNote":', JSON_QUOTE(bundle.source_note),
                ',"version":', bundle.version_no,
                '}'
            )
            FROM content_bundle AS bundle
            INNER JOIN content_bundle_member AS member
                ON member.content_bundle_id = bundle.id
            WHERE bundle.id = NEW.content_bundle_id
            GROUP BY bundle.id, bundle.bundle_key, bundle.ranked_eligible,
                     bundle.schema_version, bundle.source_note, bundle.version_no
        ),
    NEW.content_bundle_id,
    NULL
);

CREATE TRIGGER tr_content_bundle_manifest_no_update
BEFORE UPDATE ON content_bundle_canonical_manifest
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'content bundle manifests are immutable';

CREATE TRIGGER tr_content_bundle_manifest_no_delete
BEFORE DELETE ON content_bundle_canonical_manifest
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'content bundle manifests are immutable';

CREATE TRIGGER tr_content_bundle_transition_only
BEFORE UPDATE ON content_bundle
FOR EACH ROW
SET NEW.bundle_key = IF(
    (
        OLD.status = 'draft'
        AND NEW.status = 'sealed'
        AND NEW.id = OLD.id
        AND BINARY NEW.bundle_key = BINARY OLD.bundle_key
        AND NEW.version_no = OLD.version_no
        AND NEW.schema_version = OLD.schema_version
        AND NEW.ranked_eligible = OLD.ranked_eligible
        AND NEW.source_note = OLD.source_note
        AND NEW.created_at = OLD.created_at
        AND NEW.canonical_sha256 REGEXP '^[0-9a-f]{64}$'
        AND NEW.sealed_at IS NOT NULL
        AND NEW.retired_at IS NULL
        AND EXISTS (
            SELECT 1
            FROM content_bundle_canonical_manifest AS manifest
            WHERE manifest.content_bundle_id = OLD.id
              AND BINARY manifest.canonical_sha256 = BINARY NEW.canonical_sha256
        )
        AND (
            SELECT COUNT(*) FROM content_bundle_member AS member
            WHERE member.content_bundle_id = OLD.id
              AND member.authority_kind = 'careerCatalog'
        ) = 1
        AND (
            SELECT COUNT(*) FROM content_bundle_member AS member
            WHERE member.content_bundle_id = OLD.id
              AND member.authority_kind = 'recruitmentRuleset'
        ) = 1
        AND (
            SELECT COUNT(*) FROM content_bundle_member AS member
            WHERE member.content_bundle_id = OLD.id
              AND member.authority_kind = 'employmentPolicy'
        ) = 1
        AND (
            SELECT COUNT(*) FROM content_bundle_member AS member
            WHERE member.content_bundle_id = OLD.id
              AND member.authority_kind = 'lifeCatalog'
        ) = 1
        AND (
            SELECT COUNT(*) FROM content_bundle_member AS member
            WHERE member.content_bundle_id = OLD.id
              AND member.authority_kind = 'creditModel'
        ) = 1
        AND (
            SELECT COUNT(*) FROM content_bundle_member AS member
            WHERE member.content_bundle_id = OLD.id
              AND member.authority_kind = 'realEstateModel'
        ) = 1
        AND EXISTS (
            SELECT 1 FROM content_bundle_member AS member
            WHERE member.content_bundle_id = OLD.id
              AND member.authority_kind = 'characterPreset'
        )
        AND (
            SELECT COUNT(*) FROM content_bundle_member AS member
            WHERE member.content_bundle_id = OLD.id
              AND member.authority_kind = 'pointBudget'
        ) = 1
        AND NOT EXISTS (
            SELECT 1
            FROM content_bundle_member AS member
            WHERE member.content_bundle_id = OLD.id
              AND NOT (
                  (
                      member.authority_kind = 'careerCatalog'
                      AND member.authority_version = 1
                      AND member.authority_sha256 IS NULL
                      AND NEW.ranked_eligible = FALSE
                      AND EXISTS (
                          SELECT 1 FROM career_catalog_bundle AS authority
                          WHERE authority.id = member.authority_id
                            AND BINARY authority.bundle_key = BINARY member.authority_key
                            AND authority.published_at IS NOT NULL
                            AND authority.ranked_eligible = FALSE
                      )
                  )
                  OR (
                      member.authority_kind = 'recruitmentRuleset'
                      AND member.authority_version = 1
                      AND member.authority_sha256 IS NULL
                      AND NEW.ranked_eligible = FALSE
                      AND EXISTS (
                          SELECT 1 FROM recruitment_ruleset AS authority
                          WHERE authority.id = member.authority_id
                            AND BINARY authority.ruleset_key = BINARY member.authority_key
                            AND authority.published_at IS NOT NULL
                            AND authority.ranked_eligible = FALSE
                      )
                  )
                  OR (
                      member.authority_kind = 'employmentPolicy'
                      AND member.authority_version = 1
                      AND member.authority_sha256 IS NULL
                      AND NEW.ranked_eligible = FALSE
                      AND EXISTS (
                          SELECT 1 FROM employment_policy_set AS authority
                          WHERE authority.id = member.authority_id
                            AND BINARY authority.policy_key = BINARY member.authority_key
                            AND authority.published_at IS NOT NULL
                            AND authority.ranked_eligible = FALSE
                      )
                  )
                  OR (
                      member.authority_kind = 'lifeCatalog'
                      AND EXISTS (
                          SELECT 1 FROM life_catalog_set AS authority
                          WHERE authority.id = member.authority_id
                            AND BINARY authority.catalog_key = BINARY member.authority_key
                            AND BINARY authority.canonical_sha256
                                = BINARY member.authority_sha256
                            AND authority.sealed_at IS NOT NULL
                            AND (NEW.ranked_eligible = FALSE OR authority.ranked_eligible = TRUE)
                      )
                  )
                  OR (
                      member.authority_kind = 'creditModel'
                      AND EXISTS (
                          SELECT 1 FROM credit_model_version AS authority
                          WHERE authority.id = member.authority_id
                            AND BINARY authority.version_key = BINARY member.authority_key
                            AND BINARY authority.canonical_sha256
                                = BINARY member.authority_sha256
                            AND authority.sealed_at IS NOT NULL
                            AND (NEW.ranked_eligible = FALSE OR authority.ranked_eligible = TRUE)
                      )
                  )
                  OR (
                      member.authority_kind = 'realEstateModel'
                      AND EXISTS (
                          SELECT 1 FROM real_estate_model_version AS authority
                          WHERE authority.id = member.authority_id
                            AND BINARY authority.version_key = BINARY member.authority_key
                            AND BINARY authority.canonical_sha256
                                = BINARY member.authority_sha256
                            AND authority.sealed_at IS NOT NULL
                            AND (NEW.ranked_eligible = FALSE OR authority.ranked_eligible = TRUE)
                      )
                  )
                  OR (
                      member.authority_kind = 'characterPreset'
                      AND EXISTS (
                          SELECT 1 FROM character_preset_version AS authority
                          WHERE authority.id = member.authority_id
                            AND BINARY authority.preset_key = BINARY member.authority_key
                            AND authority.version_no = member.authority_version
                            AND BINARY authority.canonical_sha256
                                = BINARY member.authority_sha256
                            AND authority.status = 'sealed'
                            AND (NEW.ranked_eligible = FALSE OR authority.ranked_eligible = TRUE)
                      )
                  )
                  OR (
                      member.authority_kind = 'pointBudget'
                      AND EXISTS (
                          SELECT 1 FROM point_budget_version AS authority
                          WHERE authority.id = member.authority_id
                            AND BINARY authority.budget_key = BINARY member.authority_key
                            AND authority.version_no = member.authority_version
                            AND BINARY authority.canonical_sha256
                                = BINARY member.authority_sha256
                            AND authority.status = 'sealed'
                            AND (NEW.ranked_eligible = FALSE OR authority.ranked_eligible = TRUE)
                      )
                  )
              )
        )
    )
    OR (
        OLD.status = 'sealed'
        AND NEW.status = 'retired'
        AND NEW.id = OLD.id
        AND BINARY NEW.bundle_key = BINARY OLD.bundle_key
        AND NEW.version_no = OLD.version_no
        AND NEW.schema_version = OLD.schema_version
        AND NEW.ranked_eligible = OLD.ranked_eligible
        AND NEW.source_note = OLD.source_note
        AND BINARY NEW.canonical_sha256 = BINARY OLD.canonical_sha256
        AND NEW.sealed_at = OLD.sealed_at
        AND NEW.retired_at IS NOT NULL
        AND NEW.retired_at >= OLD.sealed_at
        AND NEW.created_at = OLD.created_at
        AND NOT EXISTS (
            SELECT 1 FROM content_bundle_assignment AS assignment
            WHERE assignment.content_bundle_id = OLD.id
        )
    ),
    OLD.bundle_key,
    NULL
);

CREATE TRIGGER tr_content_bundle_no_delete
BEFORE DELETE ON content_bundle
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'content bundles are immutable';

CREATE TRIGGER tr_content_bundle_assignment_valid_insert
BEFORE INSERT ON content_bundle_assignment
FOR EACH ROW
SET
    NEW.assignment_key = IF(
        NEW.assignment_key = 'newRun'
            AND NEW.assignment_revision = 1
            AND EXISTS (
                SELECT 1 FROM content_bundle AS bundle
                WHERE bundle.id = NEW.content_bundle_id AND bundle.status = 'sealed'
            ),
        NEW.assignment_key,
        NULL
    ),
    NEW.assignment_revision = 1;

CREATE TRIGGER tr_content_bundle_assignment_bump_revision
BEFORE UPDATE ON content_bundle_assignment
FOR EACH ROW
SET
    NEW.assignment_key = IF(
        BINARY NEW.assignment_key = BINARY OLD.assignment_key
            AND NEW.content_bundle_id <> OLD.content_bundle_id
            AND NEW.assignment_revision = OLD.assignment_revision
            AND EXISTS (
                SELECT 1 FROM content_bundle AS bundle
                WHERE bundle.id = NEW.content_bundle_id AND bundle.status = 'sealed'
            ),
        OLD.assignment_key,
        NULL
    ),
    NEW.assignment_revision = OLD.assignment_revision + 1;

CREATE TRIGGER tr_content_bundle_assignment_no_delete
BEFORE DELETE ON content_bundle_assignment
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'content bundle assignment must be updated in place';

INSERT INTO content_bundle
    (bundle_key, version_no, schema_version, status, ranked_eligible, source_note)
VALUES
    ('dev-unranked-m5-content-2026', 1, 1, 'draft', FALSE,
     'M3/M4 typed development authorities with M5-A start catalogs.');

INSERT INTO content_bundle_member
    (content_bundle_id, authority_kind, authority_id, authority_key,
     authority_version, authority_sha256, source_note)
SELECT target.id, 'careerCatalog', authority.id, authority.bundle_key,
       1, NULL,
       'M3-A/M3-D typed career catalog; legacy published authority has no standalone digest.'
FROM content_bundle AS target
INNER JOIN career_catalog_assignment AS assignment
    ON assignment.assignment_key = 'newRun'
INNER JOIN career_catalog_bundle AS authority
    ON authority.id = assignment.career_catalog_bundle_id
WHERE target.bundle_key = 'dev-unranked-m5-content-2026'
  AND target.version_no = 1
  AND authority.bundle_key = 'dev-unranked-m3-v1';

INSERT INTO content_bundle_member
    (content_bundle_id, authority_kind, authority_id, authority_key,
     authority_version, authority_sha256, source_note)
SELECT target.id, 'recruitmentRuleset', authority.id, authority.ruleset_key,
       1, NULL,
       'M3-B typed recruitment rules; legacy published authority has no standalone digest.'
FROM content_bundle AS target
INNER JOIN career_catalog_assignment AS career_assignment
    ON career_assignment.assignment_key = 'newRun'
INNER JOIN recruitment_ruleset_assignment AS assignment
    ON assignment.career_catalog_bundle_id = career_assignment.career_catalog_bundle_id
   AND assignment.assignment_key = 'newPosting'
INNER JOIN recruitment_ruleset AS authority
    ON authority.id = assignment.recruitment_ruleset_id
WHERE target.bundle_key = 'dev-unranked-m5-content-2026'
  AND target.version_no = 1
  AND authority.ruleset_key = 'dev-unranked-m3-recruitment-v1';

INSERT INTO content_bundle_member
    (content_bundle_id, authority_kind, authority_id, authority_key,
     authority_version, authority_sha256, source_note)
SELECT target.id, 'employmentPolicy', authority.id, authority.policy_key,
       1, NULL,
       'M3-C/M3-D typed employment policy; legacy published authority has no standalone digest.'
FROM content_bundle AS target
INNER JOIN employment_policy_assignment AS assignment
    ON assignment.assignment_key = 'newRun'
INNER JOIN employment_policy_set AS authority
    ON authority.id = assignment.employment_policy_set_id
WHERE target.bundle_key = 'dev-unranked-m5-content-2026'
  AND target.version_no = 1
  AND authority.policy_key = 'dev-unranked-m3-employment-2026-v1';

INSERT INTO content_bundle_member
    (content_bundle_id, authority_kind, authority_id, authority_key,
     authority_version, authority_sha256, source_note)
SELECT target.id, 'lifeCatalog', authority.id, authority.catalog_key,
       6, authority.canonical_sha256, 'M4 typed life catalog aggregate.'
FROM content_bundle AS target
INNER JOIN run_rule_bundle_assignment AS assignment
    ON assignment.assignment_key = 'newRun'
INNER JOIN life_catalog_set AS authority
    ON authority.id = assignment.life_catalog_set_id
WHERE target.bundle_key = 'dev-unranked-m5-content-2026'
  AND target.version_no = 1
  AND authority.catalog_key = 'dev-unranked-m4-life-corporation-2026-v6';

INSERT INTO content_bundle_member
    (content_bundle_id, authority_kind, authority_id, authority_key,
     authority_version, authority_sha256, source_note)
SELECT target.id, 'creditModel', authority.id, authority.version_key,
       4, authority.canonical_sha256, 'M4 typed credit and loan model.'
FROM content_bundle AS target
INNER JOIN run_rule_bundle_assignment AS assignment
    ON assignment.assignment_key = 'newRun'
INNER JOIN credit_model_version AS authority
    ON authority.id = assignment.credit_model_version_id
WHERE target.bundle_key = 'dev-unranked-m5-content-2026'
  AND target.version_no = 1
  AND authority.version_key = 'dev-unranked-m4c3-credit-2026-v4';

INSERT INTO content_bundle_member
    (content_bundle_id, authority_kind, authority_id, authority_key,
     authority_version, authority_sha256, source_note)
SELECT target.id, 'realEstateModel', authority.id, authority.version_key,
       6, authority.canonical_sha256, 'M4 typed housing and real-estate model.'
FROM content_bundle AS target
INNER JOIN run_rule_bundle_assignment AS assignment
    ON assignment.assignment_key = 'newRun'
INNER JOIN real_estate_model_version AS authority
    ON authority.id = assignment.real_estate_model_version_id
WHERE target.bundle_key = 'dev-unranked-m5-content-2026'
  AND target.version_no = 1
  AND authority.version_key = 'dev-unranked-m4-real-estate-sale-tax-2026-v6';

INSERT INTO content_bundle_member
    (content_bundle_id, authority_kind, authority_id, authority_key,
     authority_version, authority_sha256, source_note)
SELECT target.id, 'characterPreset', authority.id, authority.preset_key,
       authority.version_no, authority.canonical_sha256,
       'M5-A sealed character preset.'
FROM content_bundle AS target
CROSS JOIN character_preset_version AS authority
WHERE target.bundle_key = 'dev-unranked-m5-content-2026'
  AND target.version_no = 1
  AND authority.status = 'sealed'
  AND authority.version_no = 1
  AND authority.preset_key IN ('early-start', 'late-start', 'restart', 'rookie', 'supported');

INSERT INTO content_bundle_member
    (content_bundle_id, authority_kind, authority_id, authority_key,
     authority_version, authority_sha256, source_note)
SELECT target.id, 'pointBudget', authority.id, authority.budget_key,
       authority.version_no, authority.canonical_sha256,
       'M5-A sealed point budget.'
FROM content_bundle AS target
INNER JOIN point_budget_assignment AS assignment
    ON assignment.assignment_key = 'newRun'
INNER JOIN point_budget_version AS authority
    ON authority.id = assignment.point_budget_version_id
WHERE target.bundle_key = 'dev-unranked-m5-content-2026'
  AND target.version_no = 1
  AND authority.budget_key = 'dev-unranked-custom-2026'
  AND authority.version_no = 1;

INSERT INTO content_bundle_canonical_manifest (content_bundle_id, canonical_json)
SELECT bundle.id,
       CONCAT(
           '{"bundleKey":', JSON_QUOTE(bundle.bundle_key),
           ',"members":[',
           GROUP_CONCAT(
               CONCAT(
                   '{"authorityId":', JSON_QUOTE(CAST(member.authority_id AS CHAR)),
                   ',"authorityKey":', JSON_QUOTE(member.authority_key),
                   ',"authorityKind":', JSON_QUOTE(member.authority_kind),
                   ',"authoritySha256":',
                       IF(member.authority_sha256 IS NULL,
                          'null', JSON_QUOTE(member.authority_sha256)),
                   ',"authorityVersion":', member.authority_version,
                   ',"sourceNote":', JSON_QUOTE(member.source_note),
                   '}'
               )
               ORDER BY
                   FIELD(
                       member.authority_kind,
                       'careerCatalog', 'recruitmentRuleset', 'employmentPolicy',
                       'lifeCatalog', 'creditModel', 'realEstateModel',
                       'characterPreset', 'pointBudget'
                   ),
                   BINARY member.authority_key,
                   member.authority_version,
                   member.authority_id
               SEPARATOR ','
           ),
           '],"rankedEligible":', IF(bundle.ranked_eligible, 'true', 'false'),
           ',"schemaVersion":', bundle.schema_version,
           ',"sourceNote":', JSON_QUOTE(bundle.source_note),
           ',"version":', bundle.version_no,
           '}'
       )
FROM content_bundle AS bundle
INNER JOIN content_bundle_member AS member
    ON member.content_bundle_id = bundle.id
WHERE bundle.bundle_key = 'dev-unranked-m5-content-2026'
  AND bundle.version_no = 1
GROUP BY bundle.id, bundle.bundle_key, bundle.ranked_eligible,
         bundle.schema_version, bundle.source_note, bundle.version_no;

UPDATE content_bundle AS bundle
INNER JOIN content_bundle_canonical_manifest AS manifest
    ON manifest.content_bundle_id = bundle.id
SET bundle.status = 'sealed',
    bundle.canonical_sha256 = manifest.canonical_sha256,
    bundle.sealed_at = CURRENT_TIMESTAMP(3)
WHERE bundle.bundle_key = 'dev-unranked-m5-content-2026'
  AND bundle.version_no = 1
  AND bundle.status = 'draft';

INSERT INTO content_bundle_assignment
    (assignment_key, content_bundle_id, assignment_revision)
SELECT 'newRun', id, 1
FROM content_bundle
WHERE bundle_key = 'dev-unranked-m5-content-2026'
  AND version_no = 1
  AND status = 'sealed';

ALTER TABLE run_manifest
    ADD COLUMN content_bundle_id BIGINT UNSIGNED NULL
        AFTER real_estate_model_version_id,
    ADD COLUMN content_bundle_sha256 CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL
        AFTER content_bundle_id,
    ADD KEY ix_run_manifest_content_bundle (content_bundle_id),
    ADD CONSTRAINT fk_run_manifest_content_bundle
        FOREIGN KEY (content_bundle_id, content_bundle_sha256)
        REFERENCES content_bundle (id, canonical_sha256),
    ADD CONSTRAINT ck_run_manifest_content_bundle CHECK (
        (content_bundle_id IS NULL AND content_bundle_sha256 IS NULL)
        OR (
            content_bundle_id IS NOT NULL
            AND content_bundle_sha256 REGEXP '^[0-9a-f]{64}$'
        )
    );

CREATE TRIGGER tr_run_manifest_content_bundle_valid_insert
BEFORE INSERT ON run_manifest
FOR EACH ROW
SET NEW.mode = IF(
    NEW.content_bundle_id IS NOT NULL
        AND NEW.content_bundle_sha256 REGEXP '^[0-9a-f]{64}$'
        AND EXISTS (
            SELECT 1
            FROM content_bundle AS content
            WHERE content.id = NEW.content_bundle_id
              AND BINARY content.canonical_sha256 = BINARY NEW.content_bundle_sha256
              AND content.status = 'sealed'
        ),
    NEW.mode,
    NULL
);
