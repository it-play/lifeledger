-- M4-A immutable policy provenance and composite run-rule manifests (m4-life.md §2).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;
SET SESSION group_concat_max_len = 1048576;

-- Existing finance policy sets predate source-document links. Give their canonical content a
-- durable digest and mark every old rule as legacy provenance instead of inventing a source.
ALTER TABLE policy_set
    ADD COLUMN ranked_eligible BOOLEAN NOT NULL DEFAULT FALSE AFTER basis_date,
    ADD COLUMN canonical_sha256 CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL
        AFTER sealed_at,
    ADD CONSTRAINT ck_policy_set_ranked CHECK (ranked_eligible IN (FALSE, TRUE)),
    ADD CONSTRAINT ck_policy_set_ranked_key CHECK (
        ranked_eligible = FALSE OR policy_key NOT LIKE 'dev-unranked-%'
    );

DROP TRIGGER tr_policy_set_draft_insert_only;
DROP TRIGGER tr_policy_set_seal_only;

CREATE TABLE policy_set_canonical_manifest (
    policy_set_id       BIGINT UNSIGNED NOT NULL,
    canonical_json      LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    canonical_sha256    CHAR(64) CHARACTER SET ascii COLLATE ascii_bin
        GENERATED ALWAYS AS (SHA2(canonical_json, 256)) STORED,
    created_at          DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (policy_set_id),
    UNIQUE KEY uk_policy_set_canonical_manifest_sha (canonical_sha256),
    CONSTRAINT fk_policy_set_canonical_manifest_set
        FOREIGN KEY (policy_set_id) REFERENCES policy_set (id),
    CONSTRAINT ck_policy_set_canonical_manifest_json CHECK (JSON_VALID(canonical_json))
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

INSERT INTO policy_set_canonical_manifest (policy_set_id, canonical_json)
SELECT
    policy.id,
    CONCAT(
        '{"basisDate":', JSON_QUOTE(DATE_FORMAT(policy.basis_date, '%Y-%m-%d')),
        ',"policyKey":', JSON_QUOTE(policy.policy_key),
        ',"rankedEligible":', IF(policy.ranked_eligible, 'true', 'false'),
        ',"rules":[',
        COALESCE(
            (
                SELECT GROUP_CONCAT(
                    CAST(JSON_OBJECT(
                        'domain', rule.domain,
                        'effectiveFrom', DATE_FORMAT(rule.effective_from, '%Y-%m-%d'),
                        'effectiveTo', IF(
                            rule.effective_to IS NULL,
                            NULL,
                            DATE_FORMAT(rule.effective_to, '%Y-%m-%d')
                        ),
                        'parameters', rule.parameters,
                        'ruleId', CAST(rule.id AS CHAR),
                        'ruleKey', rule.rule_key
                    ) AS CHAR CHARACTER SET utf8mb4)
                    ORDER BY rule.domain, rule.rule_key, rule.effective_from, rule.id
                    SEPARATOR ','
                )
                FROM policy_rule AS rule
                WHERE rule.policy_set_id = policy.id
            ),
            ''
        ),
        '],"schemaVersion":1}'
    )
FROM policy_set AS policy
WHERE policy.sealed_at IS NOT NULL;

UPDATE policy_set AS policy
INNER JOIN policy_set_canonical_manifest AS manifest
    ON manifest.policy_set_id = policy.id
SET policy.canonical_sha256 = manifest.canonical_sha256
WHERE policy.sealed_at IS NOT NULL;

ALTER TABLE policy_set
    ADD CONSTRAINT ck_policy_set_publication_shape CHECK (
        (
            sealed_at IS NULL
            AND canonical_sha256 IS NULL
        )
        OR (
            sealed_at IS NOT NULL
            AND canonical_sha256 REGEXP '^[0-9a-f]{64}$'
        )
    );

CREATE TABLE policy_source_document (
    id                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    source_key          VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    source_url          VARCHAR(1024) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    checked_on          DATE            NOT NULL,
    original_sha256     CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at          DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_policy_source_document_key (source_key),
    CONSTRAINT ck_policy_source_document_key CHECK (
        source_key REGEXP '^[a-z0-9][a-z0-9._-]{0,95}$'
    ),
    CONSTRAINT ck_policy_source_document_url CHECK (
        source_url REGEXP '^https://[^[:space:]]+$'
    ),
    CONSTRAINT ck_policy_source_document_sha CHECK (
        original_sha256 REGEXP '^[0-9a-f]{64}$'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE policy_rule_source (
    policy_rule_id              BIGINT UNSIGNED NOT NULL,
    policy_source_document_id   BIGINT UNSIGNED NOT NULL,
    citation_order              SMALLINT UNSIGNED NOT NULL,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (policy_rule_id, policy_source_document_id),
    UNIQUE KEY uk_policy_rule_source_order (policy_rule_id, citation_order),
    KEY ix_policy_rule_source_document (policy_source_document_id),
    CONSTRAINT fk_policy_rule_source_rule
        FOREIGN KEY (policy_rule_id) REFERENCES policy_rule (id),
    CONSTRAINT fk_policy_rule_source_document
        FOREIGN KEY (policy_source_document_id) REFERENCES policy_source_document (id),
    CONSTRAINT ck_policy_rule_source_order CHECK (citation_order > 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE policy_rule_legacy_provenance (
    policy_rule_id      BIGINT UNSIGNED NOT NULL,
    provenance_key     VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    reason              VARCHAR(255)    NOT NULL,
    recorded_at         DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (policy_rule_id),
    CONSTRAINT fk_policy_rule_legacy_provenance_rule
        FOREIGN KEY (policy_rule_id) REFERENCES policy_rule (id),
    CONSTRAINT ck_policy_rule_legacy_provenance_key CHECK (
        provenance_key = 'preM4LegacyProvenance'
    ),
    CONSTRAINT ck_policy_rule_legacy_provenance_reason CHECK (CHAR_LENGTH(reason) > 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

INSERT INTO policy_rule_legacy_provenance (policy_rule_id, provenance_key, reason)
SELECT id,
       'preM4LegacyProvenance',
       'Rule was sealed before M4 source-document provenance was introduced.'
FROM policy_rule;

CREATE TRIGGER tr_policy_set_draft_insert_only
BEFORE INSERT ON policy_set
FOR EACH ROW
SET
    NEW.policy_key = IF(
        NEW.sealed_at IS NULL
            AND NEW.canonical_sha256 IS NULL
            AND NEW.ranked_eligible IN (FALSE, TRUE)
            AND (NEW.ranked_eligible = FALSE OR NEW.policy_key NOT LIKE 'dev-unranked-%'),
        NEW.policy_key,
        NULL
    );

-- Publication requires a caller-computed digest and explicit provenance for every rule. Legacy
-- provenance can preserve an old unranked graph, but can never publish a ranked policy graph.
CREATE TRIGGER tr_policy_set_seal_only
BEFORE UPDATE ON policy_set
FOR EACH ROW
SET NEW.policy_key = IF(
    OLD.sealed_at IS NULL
        AND NEW.sealed_at IS NOT NULL
        AND NEW.id = OLD.id
        AND BINARY NEW.policy_key = BINARY OLD.policy_key
        AND NEW.basis_date = OLD.basis_date
        AND NEW.ranked_eligible = OLD.ranked_eligible
        AND NEW.created_at = OLD.created_at
        AND NEW.canonical_sha256 REGEXP '^[0-9a-f]{64}$'
        AND EXISTS (
            SELECT 1
            FROM policy_set_canonical_manifest AS manifest
            WHERE manifest.policy_set_id = OLD.id
              AND BINARY manifest.canonical_sha256 = BINARY NEW.canonical_sha256
        )
        AND EXISTS (
            SELECT 1 FROM policy_rule AS rule WHERE rule.policy_set_id = OLD.id
        )
        AND NOT EXISTS (
            SELECT 1
            FROM policy_rule AS rule
            WHERE rule.policy_set_id = OLD.id
              AND NOT EXISTS (
                  SELECT 1
                  FROM policy_rule_source AS source_link
                  WHERE source_link.policy_rule_id = rule.id
              )
              AND NOT EXISTS (
                  SELECT 1
                  FROM policy_rule_legacy_provenance AS legacy
                  WHERE legacy.policy_rule_id = rule.id
              )
        )
        AND (
            NEW.ranked_eligible = FALSE
            OR NOT EXISTS (
                SELECT 1
                FROM policy_rule AS rule
                INNER JOIN policy_rule_legacy_provenance AS legacy
                    ON legacy.policy_rule_id = rule.id
                WHERE rule.policy_set_id = OLD.id
            )
        ),
    NEW.policy_key,
    NULL
);

CREATE TRIGGER tr_policy_source_document_no_update
BEFORE UPDATE ON policy_source_document
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'policy source documents are immutable';

CREATE TRIGGER tr_policy_set_canonical_manifest_draft_insert
BEFORE INSERT ON policy_set_canonical_manifest
FOR EACH ROW
SET NEW.policy_set_id = IF(
    JSON_VALID(NEW.canonical_json)
        AND EXISTS (
            SELECT 1 FROM policy_set
            WHERE id = NEW.policy_set_id AND sealed_at IS NULL
        ),
    NEW.policy_set_id,
    NULL
);

CREATE TRIGGER tr_policy_set_canonical_manifest_no_update
BEFORE UPDATE ON policy_set_canonical_manifest
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'policy canonical manifests are immutable';

CREATE TRIGGER tr_policy_set_canonical_manifest_no_delete
BEFORE DELETE ON policy_set_canonical_manifest
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'policy canonical manifests are immutable';

CREATE TRIGGER tr_policy_source_document_no_delete
BEFORE DELETE ON policy_source_document
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'policy source documents are immutable';

CREATE TRIGGER tr_policy_rule_source_draft_insert
BEFORE INSERT ON policy_rule_source
FOR EACH ROW
SET NEW.policy_rule_id = IF(
    EXISTS (
        SELECT 1
        FROM policy_rule AS rule
        INNER JOIN policy_set AS policy ON policy.id = rule.policy_set_id
        WHERE rule.id = NEW.policy_rule_id
          AND policy.sealed_at IS NULL
    ),
    NEW.policy_rule_id,
    NULL
);

CREATE TRIGGER tr_policy_rule_source_no_update
BEFORE UPDATE ON policy_rule_source
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'policy rule source links are immutable';

CREATE TRIGGER tr_policy_rule_source_no_delete
BEFORE DELETE ON policy_rule_source
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'policy rule source links are immutable';

CREATE TRIGGER tr_policy_rule_legacy_provenance_no_insert
BEFORE INSERT ON policy_rule_legacy_provenance
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'legacy policy provenance is closed after M4 bridge';

CREATE TRIGGER tr_policy_rule_legacy_provenance_no_update
BEFORE UPDATE ON policy_rule_legacy_provenance
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'legacy policy provenance is immutable';

CREATE TRIGGER tr_policy_rule_legacy_provenance_no_delete
BEFORE DELETE ON policy_rule_legacy_provenance
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'legacy policy provenance is immutable';

CREATE TABLE life_component_version (
    id                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    component_kind      VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    version_key         VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    availability        VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ranked_eligible     BOOLEAN         NOT NULL DEFAULT FALSE,
    canonical_sha256    CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    sealed_at           DATETIME(3)         NULL,
    created_at          DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_life_component_version_kind_key (component_kind, version_key),
    UNIQUE KEY uk_life_component_version_id_kind (id, component_kind),
    CONSTRAINT ck_life_component_version_kind CHECK (
        component_kind IN ('livingCost', 'welfare', 'lifeEvent', 'insurance', 'corporation')
    ),
    CONSTRAINT ck_life_component_version_availability CHECK (
        availability IN ('active', 'disabled')
    ),
    CONSTRAINT ck_life_component_version_ranked CHECK (ranked_eligible IN (FALSE, TRUE)),
    CONSTRAINT ck_life_component_version_key CHECK (
        version_key REGEXP '^[a-z0-9][a-zA-Z0-9._-]{0,95}$'
    ),
    CONSTRAINT ck_life_component_version_ranked_key CHECK (
        ranked_eligible = FALSE OR version_key NOT LIKE 'dev-unranked-%'
    ),
    CONSTRAINT ck_life_component_version_publication CHECK (
        (sealed_at IS NULL AND canonical_sha256 IS NULL)
        OR (sealed_at IS NOT NULL AND canonical_sha256 REGEXP '^[0-9a-f]{64}$')
    ),
    CONSTRAINT ck_life_component_version_disabled_ranked CHECK (
        availability <> 'disabled' OR ranked_eligible = FALSE
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE life_component_canonical_manifest (
    life_component_version_id   BIGINT UNSIGNED NOT NULL,
    canonical_json              LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    canonical_sha256            CHAR(64) CHARACTER SET ascii COLLATE ascii_bin
        GENERATED ALWAYS AS (SHA2(canonical_json, 256)) STORED,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (life_component_version_id),
    UNIQUE KEY uk_life_component_canonical_manifest_sha (canonical_sha256),
    CONSTRAINT fk_life_component_canonical_manifest_version
        FOREIGN KEY (life_component_version_id) REFERENCES life_component_version (id),
    CONSTRAINT ck_life_component_canonical_manifest_json CHECK (JSON_VALID(canonical_json))
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE credit_model_version (
    id                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    version_key         VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    availability        VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ranked_eligible     BOOLEAN         NOT NULL DEFAULT FALSE,
    parameters          JSON            NOT NULL,
    canonical_sha256    CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    sealed_at           DATETIME(3)         NULL,
    created_at          DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_credit_model_version_key (version_key),
    CONSTRAINT ck_credit_model_version_key CHECK (
        version_key REGEXP '^[a-z0-9][a-zA-Z0-9._-]{0,95}$'
    ),
    CONSTRAINT ck_credit_model_version_availability CHECK (
        availability IN ('active', 'disabled')
    ),
    CONSTRAINT ck_credit_model_version_ranked CHECK (ranked_eligible IN (FALSE, TRUE)),
    CONSTRAINT ck_credit_model_version_shape CHECK (
        JSON_TYPE(parameters) = 'OBJECT'
        AND (
            (sealed_at IS NULL AND canonical_sha256 IS NULL)
            OR (sealed_at IS NOT NULL AND canonical_sha256 REGEXP '^[0-9a-f]{64}$')
        )
        AND (availability <> 'disabled' OR ranked_eligible = FALSE)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE real_estate_model_version (
    id                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    version_key         VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    availability        VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ranked_eligible     BOOLEAN         NOT NULL DEFAULT FALSE,
    parameters          JSON            NOT NULL,
    canonical_sha256    CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    sealed_at           DATETIME(3)         NULL,
    created_at          DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_real_estate_model_version_key (version_key),
    CONSTRAINT ck_real_estate_model_version_key CHECK (
        version_key REGEXP '^[a-z0-9][a-zA-Z0-9._-]{0,95}$'
    ),
    CONSTRAINT ck_real_estate_model_version_availability CHECK (
        availability IN ('active', 'disabled')
    ),
    CONSTRAINT ck_real_estate_model_version_ranked CHECK (ranked_eligible IN (FALSE, TRUE)),
    CONSTRAINT ck_real_estate_model_version_shape CHECK (
        JSON_TYPE(parameters) = 'OBJECT'
        AND (
            (sealed_at IS NULL AND canonical_sha256 IS NULL)
            OR (sealed_at IS NOT NULL AND canonical_sha256 REGEXP '^[0-9a-f]{64}$')
        )
        AND (availability <> 'disabled' OR ranked_eligible = FALSE)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE life_catalog_set (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    catalog_key                         VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ranked_eligible                     BOOLEAN         NOT NULL DEFAULT FALSE,
    legacy_dependent_age_years          TINYINT UNSIGNED NOT NULL,
    living_cost_component_version_id    BIGINT UNSIGNED NOT NULL,
    welfare_component_version_id        BIGINT UNSIGNED NOT NULL,
    life_event_component_version_id     BIGINT UNSIGNED NOT NULL,
    insurance_component_version_id      BIGINT UNSIGNED NOT NULL,
    corporation_component_version_id    BIGINT UNSIGNED NOT NULL,
    canonical_sha256                    CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    sealed_at                           DATETIME(3)         NULL,
    created_at                          DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_life_catalog_set_key (catalog_key),
    UNIQUE KEY uk_life_catalog_set_id_components (
        id,
        living_cost_component_version_id,
        welfare_component_version_id,
        life_event_component_version_id,
        insurance_component_version_id,
        corporation_component_version_id
    ),
    KEY ix_life_catalog_set_living_cost (living_cost_component_version_id),
    KEY ix_life_catalog_set_welfare (welfare_component_version_id),
    KEY ix_life_catalog_set_life_event (life_event_component_version_id),
    KEY ix_life_catalog_set_insurance (insurance_component_version_id),
    KEY ix_life_catalog_set_corporation (corporation_component_version_id),
    CONSTRAINT fk_life_catalog_set_living_cost
        FOREIGN KEY (living_cost_component_version_id) REFERENCES life_component_version (id),
    CONSTRAINT fk_life_catalog_set_welfare
        FOREIGN KEY (welfare_component_version_id) REFERENCES life_component_version (id),
    CONSTRAINT fk_life_catalog_set_life_event
        FOREIGN KEY (life_event_component_version_id) REFERENCES life_component_version (id),
    CONSTRAINT fk_life_catalog_set_insurance
        FOREIGN KEY (insurance_component_version_id) REFERENCES life_component_version (id),
    CONSTRAINT fk_life_catalog_set_corporation
        FOREIGN KEY (corporation_component_version_id) REFERENCES life_component_version (id),
    CONSTRAINT ck_life_catalog_set_key CHECK (
        catalog_key REGEXP '^[a-z0-9][a-zA-Z0-9._-]{0,95}$'
    ),
    CONSTRAINT ck_life_catalog_set_ranked CHECK (ranked_eligible IN (FALSE, TRUE)),
    CONSTRAINT ck_life_catalog_set_legacy_age CHECK (
        legacy_dependent_age_years BETWEEN 0 AND 120
    ),
    CONSTRAINT ck_life_catalog_set_ranked_key CHECK (
        ranked_eligible = FALSE OR catalog_key NOT LIKE 'dev-unranked-%'
    ),
    CONSTRAINT ck_life_catalog_set_publication CHECK (
        (sealed_at IS NULL AND canonical_sha256 IS NULL)
        OR (sealed_at IS NOT NULL AND canonical_sha256 REGEXP '^[0-9a-f]{64}$')
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_life_component_version_draft_insert
BEFORE INSERT ON life_component_version
FOR EACH ROW
SET NEW.version_key = IF(
    NEW.sealed_at IS NULL
        AND NEW.canonical_sha256 IS NULL
        AND NEW.ranked_eligible IN (FALSE, TRUE)
        AND (NEW.availability <> 'disabled' OR NEW.ranked_eligible = FALSE),
    NEW.version_key,
    NULL
);

CREATE TRIGGER tr_life_component_version_seal_only
BEFORE UPDATE ON life_component_version
FOR EACH ROW
SET NEW.version_key = IF(
    OLD.sealed_at IS NULL
        AND NEW.sealed_at IS NOT NULL
        AND NEW.id = OLD.id
        AND BINARY NEW.component_kind = BINARY OLD.component_kind
        AND BINARY NEW.version_key = BINARY OLD.version_key
        AND BINARY NEW.availability = BINARY OLD.availability
        AND NEW.ranked_eligible = OLD.ranked_eligible
        AND NEW.created_at = OLD.created_at
        AND NEW.canonical_sha256 REGEXP '^[0-9a-f]{64}$'
        AND EXISTS (
            SELECT 1
            FROM life_component_canonical_manifest AS manifest
            WHERE manifest.life_component_version_id = OLD.id
              AND BINARY manifest.canonical_sha256 = BINARY NEW.canonical_sha256
        ),
    OLD.version_key,
    NULL
);

CREATE TRIGGER tr_life_component_version_no_delete
BEFORE DELETE ON life_component_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'life component versions are immutable';

CREATE TRIGGER tr_life_component_canonical_manifest_draft_insert
BEFORE INSERT ON life_component_canonical_manifest
FOR EACH ROW
SET NEW.life_component_version_id = IF(
    JSON_VALID(NEW.canonical_json)
        AND EXISTS (
            SELECT 1 FROM life_component_version
            WHERE id = NEW.life_component_version_id AND sealed_at IS NULL
        ),
    NEW.life_component_version_id,
    NULL
);

CREATE TRIGGER tr_life_component_canonical_manifest_no_update
BEFORE UPDATE ON life_component_canonical_manifest
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'life component manifests are immutable';

CREATE TRIGGER tr_life_component_canonical_manifest_no_delete
BEFORE DELETE ON life_component_canonical_manifest
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'life component manifests are immutable';

CREATE TRIGGER tr_credit_model_version_draft_insert
BEFORE INSERT ON credit_model_version
FOR EACH ROW
SET NEW.version_key = IF(
    NEW.sealed_at IS NULL
        AND NEW.canonical_sha256 IS NULL
        AND JSON_TYPE(NEW.parameters) = 'OBJECT'
        AND (NEW.availability <> 'disabled' OR NEW.ranked_eligible = FALSE),
    NEW.version_key,
    NULL
);

CREATE TRIGGER tr_credit_model_version_seal_only
BEFORE UPDATE ON credit_model_version
FOR EACH ROW
SET NEW.version_key = IF(
    OLD.sealed_at IS NULL
        AND NEW.sealed_at IS NOT NULL
        AND NEW.id = OLD.id
        AND BINARY NEW.version_key = BINARY OLD.version_key
        AND BINARY NEW.availability = BINARY OLD.availability
        AND NEW.ranked_eligible = OLD.ranked_eligible
        AND NEW.parameters = OLD.parameters
        AND NEW.created_at = OLD.created_at
        AND NEW.canonical_sha256 REGEXP '^[0-9a-f]{64}$'
        AND NEW.canonical_sha256 = SHA2(
            CAST(JSON_OBJECT(
                'availability', OLD.availability,
                'parameters', OLD.parameters,
                'schemaVersion', 1,
                'versionKey', OLD.version_key
            ) AS CHAR CHARACTER SET utf8mb4),
            256
        ),
    OLD.version_key,
    NULL
);

CREATE TRIGGER tr_credit_model_version_no_delete
BEFORE DELETE ON credit_model_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'credit model versions are immutable';

CREATE TRIGGER tr_real_estate_model_version_draft_insert
BEFORE INSERT ON real_estate_model_version
FOR EACH ROW
SET NEW.version_key = IF(
    NEW.sealed_at IS NULL
        AND NEW.canonical_sha256 IS NULL
        AND JSON_TYPE(NEW.parameters) = 'OBJECT'
        AND (NEW.availability <> 'disabled' OR NEW.ranked_eligible = FALSE),
    NEW.version_key,
    NULL
);

CREATE TRIGGER tr_real_estate_model_version_seal_only
BEFORE UPDATE ON real_estate_model_version
FOR EACH ROW
SET NEW.version_key = IF(
    OLD.sealed_at IS NULL
        AND NEW.sealed_at IS NOT NULL
        AND NEW.id = OLD.id
        AND BINARY NEW.version_key = BINARY OLD.version_key
        AND BINARY NEW.availability = BINARY OLD.availability
        AND NEW.ranked_eligible = OLD.ranked_eligible
        AND NEW.parameters = OLD.parameters
        AND NEW.created_at = OLD.created_at
        AND NEW.canonical_sha256 REGEXP '^[0-9a-f]{64}$'
        AND NEW.canonical_sha256 = SHA2(
            CAST(JSON_OBJECT(
                'availability', OLD.availability,
                'parameters', OLD.parameters,
                'schemaVersion', 1,
                'versionKey', OLD.version_key
            ) AS CHAR CHARACTER SET utf8mb4),
            256
        ),
    OLD.version_key,
    NULL
);

CREATE TRIGGER tr_real_estate_model_version_no_delete
BEFORE DELETE ON real_estate_model_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'real-estate model versions are immutable';

CREATE TRIGGER tr_life_catalog_set_draft_insert
BEFORE INSERT ON life_catalog_set
FOR EACH ROW
SET NEW.catalog_key = IF(
    NEW.sealed_at IS NULL
        AND NEW.canonical_sha256 IS NULL
        AND NEW.ranked_eligible IN (FALSE, TRUE),
    NEW.catalog_key,
    NULL
);

CREATE TRIGGER tr_life_catalog_set_seal_only
BEFORE UPDATE ON life_catalog_set
FOR EACH ROW
SET NEW.catalog_key = IF(
    OLD.sealed_at IS NULL
        AND NEW.sealed_at IS NOT NULL
        AND NEW.id = OLD.id
        AND BINARY NEW.catalog_key = BINARY OLD.catalog_key
        AND NEW.ranked_eligible = OLD.ranked_eligible
        AND NEW.legacy_dependent_age_years = OLD.legacy_dependent_age_years
        AND NEW.living_cost_component_version_id = OLD.living_cost_component_version_id
        AND NEW.welfare_component_version_id = OLD.welfare_component_version_id
        AND NEW.life_event_component_version_id = OLD.life_event_component_version_id
        AND NEW.insurance_component_version_id = OLD.insurance_component_version_id
        AND NEW.corporation_component_version_id = OLD.corporation_component_version_id
        AND NEW.created_at = OLD.created_at
        AND NEW.canonical_sha256 REGEXP '^[0-9a-f]{64}$'
        AND NEW.canonical_sha256 = SHA2(
            CAST(JSON_OBJECT(
                'catalogKey', OLD.catalog_key,
                'corporationComponentVersionId',
                    CAST(OLD.corporation_component_version_id AS CHAR),
                'insuranceComponentVersionId',
                    CAST(OLD.insurance_component_version_id AS CHAR),
                'lifeEventComponentVersionId',
                    CAST(OLD.life_event_component_version_id AS CHAR),
                'legacyDependentAgeYears', OLD.legacy_dependent_age_years,
                'livingCostComponentVersionId',
                    CAST(OLD.living_cost_component_version_id AS CHAR),
                'schemaVersion', 1,
                'welfareComponentVersionId',
                    CAST(OLD.welfare_component_version_id AS CHAR)
            ) AS CHAR CHARACTER SET utf8mb4),
            256
        )
        AND EXISTS (
            SELECT 1 FROM life_component_version
            WHERE id = OLD.living_cost_component_version_id
              AND component_kind = 'livingCost' AND sealed_at IS NOT NULL
              AND (NEW.ranked_eligible = FALSE OR ranked_eligible = TRUE)
        )
        AND EXISTS (
            SELECT 1 FROM life_component_version
            WHERE id = OLD.welfare_component_version_id
              AND component_kind = 'welfare' AND sealed_at IS NOT NULL
              AND (NEW.ranked_eligible = FALSE OR ranked_eligible = TRUE)
        )
        AND EXISTS (
            SELECT 1 FROM life_component_version
            WHERE id = OLD.life_event_component_version_id
              AND component_kind = 'lifeEvent' AND sealed_at IS NOT NULL
              AND (NEW.ranked_eligible = FALSE OR ranked_eligible = TRUE)
        )
        AND EXISTS (
            SELECT 1 FROM life_component_version
            WHERE id = OLD.insurance_component_version_id
              AND component_kind = 'insurance' AND sealed_at IS NOT NULL
              AND (NEW.ranked_eligible = FALSE OR ranked_eligible = TRUE)
        )
        AND EXISTS (
            SELECT 1 FROM life_component_version
            WHERE id = OLD.corporation_component_version_id
              AND component_kind = 'corporation' AND sealed_at IS NOT NULL
              AND (NEW.ranked_eligible = FALSE OR ranked_eligible = TRUE)
        ),
    OLD.catalog_key,
    NULL
);

CREATE TRIGGER tr_life_catalog_set_no_delete
BEFORE DELETE ON life_catalog_set
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'life catalog sets are immutable';

-- Disabled versions are real, sealed manifests with no executable child rows.
INSERT INTO life_component_version (component_kind, version_key, availability, ranked_eligible)
VALUES
    ('livingCost', 'disabled-m4a-compatibility-v1', 'disabled', FALSE),
    ('welfare', 'disabled-m4a-v1', 'disabled', FALSE),
    ('lifeEvent', 'disabled-m4a-v1', 'disabled', FALSE),
    ('insurance', 'disabled-m4a-v1', 'disabled', FALSE),
    ('corporation', 'disabled-m4a-v1', 'disabled', FALSE),
    ('livingCost', 'dev-unranked-m4-living-cost-2026-v1', 'active', FALSE);

INSERT INTO life_component_canonical_manifest
    (life_component_version_id, canonical_json)
SELECT
    id,
    CAST(JSON_OBJECT(
        'availability', availability,
        'componentKind', component_kind,
        'schemaVersion', 1,
        'versionKey', version_key
    ) AS CHAR CHARACTER SET utf8mb4)
FROM life_component_version
WHERE availability = 'disabled';

UPDATE life_component_version AS component
INNER JOIN life_component_canonical_manifest AS manifest
    ON manifest.life_component_version_id = component.id
SET component.canonical_sha256 = manifest.canonical_sha256,
    sealed_at = CURRENT_TIMESTAMP(3)
WHERE component.availability = 'disabled';

INSERT INTO credit_model_version
    (version_key, availability, ranked_eligible, parameters)
VALUES
    (
        'disabled-m4a-v1',
        'disabled',
        FALSE,
        JSON_OBJECT('reason', 'notImplemented', 'schemaVersion', 1)
    );

UPDATE credit_model_version
SET canonical_sha256 = SHA2(
        CAST(JSON_OBJECT(
            'availability', availability,
            'parameters', parameters,
            'schemaVersion', 1,
            'versionKey', version_key
        ) AS CHAR CHARACTER SET utf8mb4),
        256
    ),
    sealed_at = CURRENT_TIMESTAMP(3)
WHERE version_key = 'disabled-m4a-v1';

INSERT INTO real_estate_model_version
    (version_key, availability, ranked_eligible, parameters)
VALUES
    (
        'disabled-m4a-v1',
        'disabled',
        FALSE,
        JSON_OBJECT('reason', 'notImplemented', 'schemaVersion', 1)
    );

UPDATE real_estate_model_version
SET canonical_sha256 = SHA2(
        CAST(JSON_OBJECT(
            'availability', availability,
            'parameters', parameters,
            'schemaVersion', 1,
            'versionKey', version_key
        ) AS CHAR CHARACTER SET utf8mb4),
        256
    ),
    sealed_at = CURRENT_TIMESTAMP(3)
WHERE version_key = 'disabled-m4a-v1';

INSERT INTO life_catalog_set
    (
        catalog_key,
        ranked_eligible,
        legacy_dependent_age_years,
        living_cost_component_version_id,
        welfare_component_version_id,
        life_event_component_version_id,
        insurance_component_version_id,
        corporation_component_version_id
    )
SELECT
    'compatibility-m4a-pre-cpi-v1',
    FALSE,
    12,
    living.id,
    welfare.id,
    life_event.id,
    insurance.id,
    corporation.id
FROM life_component_version AS living
INNER JOIN life_component_version AS welfare
    ON welfare.component_kind = 'welfare' AND welfare.version_key = 'disabled-m4a-v1'
INNER JOIN life_component_version AS life_event
    ON life_event.component_kind = 'lifeEvent' AND life_event.version_key = 'disabled-m4a-v1'
INNER JOIN life_component_version AS insurance
    ON insurance.component_kind = 'insurance' AND insurance.version_key = 'disabled-m4a-v1'
INNER JOIN life_component_version AS corporation
    ON corporation.component_kind = 'corporation' AND corporation.version_key = 'disabled-m4a-v1'
WHERE living.component_kind = 'livingCost'
  AND living.version_key = 'disabled-m4a-compatibility-v1';

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
WHERE catalog_key = 'compatibility-m4a-pre-cpi-v1';

-- The active aggregate stays a draft until 0025 publishes its complete living-cost child graph.
INSERT INTO life_catalog_set
    (
        catalog_key,
        ranked_eligible,
        legacy_dependent_age_years,
        living_cost_component_version_id,
        welfare_component_version_id,
        life_event_component_version_id,
        insurance_component_version_id,
        corporation_component_version_id
    )
SELECT
    'dev-unranked-m4-life-2026-v1',
    FALSE,
    12,
    living.id,
    welfare.id,
    life_event.id,
    insurance.id,
    corporation.id
FROM life_component_version AS living
INNER JOIN life_component_version AS welfare
    ON welfare.component_kind = 'welfare' AND welfare.version_key = 'disabled-m4a-v1'
INNER JOIN life_component_version AS life_event
    ON life_event.component_kind = 'lifeEvent' AND life_event.version_key = 'disabled-m4a-v1'
INNER JOIN life_component_version AS insurance
    ON insurance.component_kind = 'insurance' AND insurance.version_key = 'disabled-m4a-v1'
INNER JOIN life_component_version AS corporation
    ON corporation.component_kind = 'corporation' AND corporation.version_key = 'disabled-m4a-v1'
WHERE living.component_kind = 'livingCost'
  AND living.version_key = 'dev-unranked-m4-living-cost-2026-v1';

CREATE TABLE run_rule_bundle_assignment (
    assignment_key                          VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    market_world_id                         BIGINT UNSIGNED NOT NULL,
    policy_set_id                           BIGINT UNSIGNED NOT NULL,
    career_catalog_bundle_id                BIGINT UNSIGNED NOT NULL,
    employment_policy_set_id                BIGINT UNSIGNED NOT NULL,
    life_catalog_set_id                     BIGINT UNSIGNED NOT NULL,
    credit_model_version_id                 BIGINT UNSIGNED NOT NULL,
    real_estate_model_version_id            BIGINT UNSIGNED NOT NULL,
    market_assignment_revision              BIGINT UNSIGNED NOT NULL,
    finance_assignment_revision             BIGINT UNSIGNED NOT NULL,
    career_assignment_revision              BIGINT UNSIGNED NOT NULL,
    employment_assignment_revision          BIGINT UNSIGNED NOT NULL,
    assignment_revision                     BIGINT UNSIGNED NOT NULL DEFAULT 1,
    updated_at                              DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (assignment_key),
    KEY ix_run_rule_bundle_assignment_market (market_world_id),
    KEY ix_run_rule_bundle_assignment_policy (policy_set_id),
    KEY ix_run_rule_bundle_assignment_career (career_catalog_bundle_id),
    KEY ix_run_rule_bundle_assignment_employment (employment_policy_set_id),
    KEY ix_run_rule_bundle_assignment_life (life_catalog_set_id),
    KEY ix_run_rule_bundle_assignment_credit (credit_model_version_id),
    KEY ix_run_rule_bundle_assignment_real_estate (real_estate_model_version_id),
    CONSTRAINT fk_run_rule_bundle_assignment_market
        FOREIGN KEY (market_world_id) REFERENCES market_world (id),
    CONSTRAINT fk_run_rule_bundle_assignment_policy
        FOREIGN KEY (policy_set_id) REFERENCES policy_set (id),
    CONSTRAINT fk_run_rule_bundle_assignment_career
        FOREIGN KEY (career_catalog_bundle_id) REFERENCES career_catalog_bundle (id),
    CONSTRAINT fk_run_rule_bundle_assignment_employment
        FOREIGN KEY (employment_policy_set_id) REFERENCES employment_policy_set (id),
    CONSTRAINT fk_run_rule_bundle_assignment_life
        FOREIGN KEY (life_catalog_set_id) REFERENCES life_catalog_set (id),
    CONSTRAINT fk_run_rule_bundle_assignment_credit
        FOREIGN KEY (credit_model_version_id) REFERENCES credit_model_version (id),
    CONSTRAINT fk_run_rule_bundle_assignment_real_estate
        FOREIGN KEY (real_estate_model_version_id) REFERENCES real_estate_model_version (id),
    CONSTRAINT ck_run_rule_bundle_assignment_key CHECK (assignment_key = 'newRun'),
    CONSTRAINT ck_run_rule_bundle_assignment_revisions CHECK (
        market_assignment_revision > 0
        AND finance_assignment_revision > 0
        AND career_assignment_revision > 0
        AND employment_assignment_revision > 0
        AND assignment_revision > 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE run_rule_bundle (
    save_id                                 BIGINT UNSIGNED NOT NULL,
    run_revision                            INT UNSIGNED    NOT NULL,
    market_world_id                         BIGINT UNSIGNED NOT NULL,
    policy_set_id                           BIGINT UNSIGNED NOT NULL,
    career_catalog_bundle_id                BIGINT UNSIGNED NOT NULL,
    employment_policy_set_id                BIGINT UNSIGNED NOT NULL,
    life_catalog_set_id                     BIGINT UNSIGNED NOT NULL,
    credit_model_version_id                 BIGINT UNSIGNED NOT NULL,
    real_estate_model_version_id            BIGINT UNSIGNED NOT NULL,
    market_assignment_revision              BIGINT UNSIGNED NOT NULL,
    finance_assignment_revision             BIGINT UNSIGNED NOT NULL,
    career_assignment_revision              BIGINT UNSIGNED NOT NULL,
    employment_assignment_revision          BIGINT UNSIGNED NOT NULL,
    bundle_assignment_revision              BIGINT UNSIGNED NOT NULL,
    created_at                              DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, run_revision),
    UNIQUE KEY uk_run_rule_bundle_life
        (save_id, run_revision, life_catalog_set_id),
    KEY ix_run_rule_bundle_market (market_world_id),
    KEY ix_run_rule_bundle_policy (policy_set_id),
    KEY ix_run_rule_bundle_career (career_catalog_bundle_id),
    KEY ix_run_rule_bundle_employment (employment_policy_set_id),
    KEY ix_run_rule_bundle_credit (credit_model_version_id),
    KEY ix_run_rule_bundle_real_estate (real_estate_model_version_id),
    CONSTRAINT fk_run_rule_bundle_save
        FOREIGN KEY (save_id) REFERENCES save (id) ON DELETE CASCADE,
    CONSTRAINT fk_run_rule_bundle_market
        FOREIGN KEY (market_world_id) REFERENCES market_world (id),
    CONSTRAINT fk_run_rule_bundle_policy
        FOREIGN KEY (policy_set_id) REFERENCES policy_set (id),
    CONSTRAINT fk_run_rule_bundle_career
        FOREIGN KEY (career_catalog_bundle_id) REFERENCES career_catalog_bundle (id),
    CONSTRAINT fk_run_rule_bundle_employment
        FOREIGN KEY (employment_policy_set_id) REFERENCES employment_policy_set (id),
    CONSTRAINT fk_run_rule_bundle_life
        FOREIGN KEY (life_catalog_set_id) REFERENCES life_catalog_set (id),
    CONSTRAINT fk_run_rule_bundle_credit
        FOREIGN KEY (credit_model_version_id) REFERENCES credit_model_version (id),
    CONSTRAINT fk_run_rule_bundle_real_estate
        FOREIGN KEY (real_estate_model_version_id) REFERENCES real_estate_model_version (id),
    CONSTRAINT ck_run_rule_bundle_revisions CHECK (
        finance_assignment_revision > 0
        AND career_assignment_revision > 0
        AND employment_assignment_revision > 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_run_rule_bundle_assignment_valid_insert
BEFORE INSERT ON run_rule_bundle_assignment
FOR EACH ROW
SET
    NEW.assignment_key = IF(
        NEW.assignment_key = 'newRun'
            AND NEW.assignment_revision = 1
            AND EXISTS (
                SELECT 1
                FROM market_world_assignment AS market_assignment
                INNER JOIN policy_set_assignment AS finance_assignment
                    ON finance_assignment.assignment_key = 'newRun'
                INNER JOIN career_catalog_assignment AS career_assignment
                    ON career_assignment.assignment_key = 'newRun'
                INNER JOIN employment_policy_assignment AS employment_assignment
                    ON employment_assignment.assignment_key = 'newRun'
                INNER JOIN policy_set AS policy
                    ON policy.id = NEW.policy_set_id AND policy.sealed_at IS NOT NULL
                INNER JOIN career_catalog_bundle AS career
                    ON career.id = NEW.career_catalog_bundle_id
                   AND career.published_at IS NOT NULL
                INNER JOIN employment_policy_set AS employment
                    ON employment.id = NEW.employment_policy_set_id
                   AND employment.published_at IS NOT NULL
                INNER JOIN employment_finance_compatibility AS compatibility
                    ON compatibility.employment_policy_set_id = employment.id
                   AND compatibility.policy_set_id = policy.id
                INNER JOIN life_catalog_set AS life
                    ON life.id = NEW.life_catalog_set_id AND life.sealed_at IS NOT NULL
                INNER JOIN credit_model_version AS credit
                    ON credit.id = NEW.credit_model_version_id AND credit.sealed_at IS NOT NULL
                INNER JOIN real_estate_model_version AS real_estate
                    ON real_estate.id = NEW.real_estate_model_version_id
                   AND real_estate.sealed_at IS NOT NULL
                WHERE market_assignment.assignment_key = 'newRun'
                  AND market_assignment.world_id = NEW.market_world_id
                  AND finance_assignment.policy_set_id = NEW.policy_set_id
                  AND career_assignment.career_catalog_bundle_id
                        = NEW.career_catalog_bundle_id
                  AND employment_assignment.employment_policy_set_id
                        = NEW.employment_policy_set_id
                  AND market_assignment.assignment_revision
                        = NEW.market_assignment_revision
                  AND finance_assignment.assignment_revision
                        = NEW.finance_assignment_revision
                  AND career_assignment.assignment_revision
                        = NEW.career_assignment_revision
                  AND employment_assignment.assignment_revision
                        = NEW.employment_assignment_revision
            ),
        NEW.assignment_key,
        NULL
    ),
    NEW.assignment_revision = 1;

CREATE TRIGGER tr_run_rule_bundle_assignment_bump_revision
BEFORE UPDATE ON run_rule_bundle_assignment
FOR EACH ROW
SET
    NEW.assignment_key = IF(
        BINARY NEW.assignment_key = BINARY OLD.assignment_key
            AND NEW.assignment_revision = OLD.assignment_revision
            AND EXISTS (
                SELECT 1
                FROM market_world_assignment AS market_assignment
                INNER JOIN policy_set_assignment AS finance_assignment
                    ON finance_assignment.assignment_key = 'newRun'
                INNER JOIN career_catalog_assignment AS career_assignment
                    ON career_assignment.assignment_key = 'newRun'
                INNER JOIN employment_policy_assignment AS employment_assignment
                    ON employment_assignment.assignment_key = 'newRun'
                INNER JOIN policy_set AS policy
                    ON policy.id = NEW.policy_set_id AND policy.sealed_at IS NOT NULL
                INNER JOIN career_catalog_bundle AS career
                    ON career.id = NEW.career_catalog_bundle_id
                   AND career.published_at IS NOT NULL
                INNER JOIN employment_policy_set AS employment
                    ON employment.id = NEW.employment_policy_set_id
                   AND employment.published_at IS NOT NULL
                INNER JOIN employment_finance_compatibility AS compatibility
                    ON compatibility.employment_policy_set_id = employment.id
                   AND compatibility.policy_set_id = policy.id
                INNER JOIN life_catalog_set AS life
                    ON life.id = NEW.life_catalog_set_id AND life.sealed_at IS NOT NULL
                INNER JOIN credit_model_version AS credit
                    ON credit.id = NEW.credit_model_version_id AND credit.sealed_at IS NOT NULL
                INNER JOIN real_estate_model_version AS real_estate
                    ON real_estate.id = NEW.real_estate_model_version_id
                   AND real_estate.sealed_at IS NOT NULL
                WHERE market_assignment.assignment_key = 'newRun'
                  AND market_assignment.world_id = NEW.market_world_id
                  AND finance_assignment.policy_set_id = NEW.policy_set_id
                  AND career_assignment.career_catalog_bundle_id
                        = NEW.career_catalog_bundle_id
                  AND employment_assignment.employment_policy_set_id
                        = NEW.employment_policy_set_id
                  AND market_assignment.assignment_revision
                        = NEW.market_assignment_revision
                  AND finance_assignment.assignment_revision
                        = NEW.finance_assignment_revision
                  AND career_assignment.assignment_revision
                        = NEW.career_assignment_revision
                  AND employment_assignment.assignment_revision
                        = NEW.employment_assignment_revision
            ),
        OLD.assignment_key,
        NULL
    ),
    NEW.assignment_revision = OLD.assignment_revision + 1;

CREATE TRIGGER tr_run_rule_bundle_assignment_no_delete
BEFORE DELETE ON run_rule_bundle_assignment
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'run rule bundle assignment must be updated in place';

CREATE TRIGGER tr_run_rule_bundle_valid_insert
BEFORE INSERT ON run_rule_bundle
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM save
        INNER JOIN career_run
            ON career_run.save_id = save.id
           AND career_run.run_revision = save.run_revision
        INNER JOIN policy_set AS policy
            ON policy.id = NEW.policy_set_id AND policy.sealed_at IS NOT NULL
        INNER JOIN career_catalog_bundle AS career
            ON career.id = NEW.career_catalog_bundle_id AND career.published_at IS NOT NULL
        INNER JOIN employment_policy_set AS employment
            ON employment.id = NEW.employment_policy_set_id
           AND employment.published_at IS NOT NULL
        INNER JOIN life_catalog_set AS life
            ON life.id = NEW.life_catalog_set_id AND life.sealed_at IS NOT NULL
        INNER JOIN credit_model_version AS credit
            ON credit.id = NEW.credit_model_version_id AND credit.sealed_at IS NOT NULL
        INNER JOIN real_estate_model_version AS real_estate
            ON real_estate.id = NEW.real_estate_model_version_id
           AND real_estate.sealed_at IS NOT NULL
        WHERE save.id = NEW.save_id
          AND save.run_revision = NEW.run_revision
          AND save.market_world_id = NEW.market_world_id
          AND save.policy_set_id = NEW.policy_set_id
          AND career_run.career_catalog_bundle_id = NEW.career_catalog_bundle_id
          AND career_run.employment_policy_set_id = NEW.employment_policy_set_id
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_run_rule_bundle_no_update
BEFORE UPDATE ON run_rule_bundle
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'run rule bundles are immutable';

CREATE TRIGGER tr_run_rule_bundle_no_delete
BEFORE DELETE ON run_rule_bundle
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'run rule bundles are immutable';

-- A compatibility pointer exists throughout the rollout. 0026 is the only migration that moves
-- newRun to the active M4-A aggregate after all schema, bridge, and enum changes are installed.
INSERT INTO run_rule_bundle_assignment
    (
        assignment_key,
        market_world_id,
        policy_set_id,
        career_catalog_bundle_id,
        employment_policy_set_id,
        life_catalog_set_id,
        credit_model_version_id,
        real_estate_model_version_id,
        market_assignment_revision,
        finance_assignment_revision,
        career_assignment_revision,
        employment_assignment_revision
    )
SELECT
    'newRun',
    market_assignment.world_id,
    finance_assignment.policy_set_id,
    career_assignment.career_catalog_bundle_id,
    employment_assignment.employment_policy_set_id,
    life.id,
    credit.id,
    real_estate.id,
    market_assignment.assignment_revision,
    finance_assignment.assignment_revision,
    career_assignment.assignment_revision,
    employment_assignment.assignment_revision
FROM market_world_assignment AS market_assignment
INNER JOIN policy_set_assignment AS finance_assignment
    ON finance_assignment.assignment_key = 'newRun'
INNER JOIN career_catalog_assignment AS career_assignment
    ON career_assignment.assignment_key = 'newRun'
INNER JOIN employment_policy_assignment AS employment_assignment
    ON employment_assignment.assignment_key = 'newRun'
INNER JOIN life_catalog_set AS life
    ON life.catalog_key = 'compatibility-m4a-pre-cpi-v1'
INNER JOIN credit_model_version AS credit
    ON credit.version_key = 'disabled-m4a-v1'
INNER JOIN real_estate_model_version AS real_estate
    ON real_estate.version_key = 'disabled-m4a-v1'
WHERE market_assignment.assignment_key = 'newRun';
