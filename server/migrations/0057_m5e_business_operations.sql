-- M5-E: immutable business catalog and detailed corporation operating authority.

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;

CREATE TABLE business_catalog_version (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    catalog_key                 VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    version_no                  INT UNSIGNED NOT NULL,
    schema_version              SMALLINT UNSIGNED NOT NULL,
    status                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    engine_version              VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    canonical_manifest_json     LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    canonical_sha256            CHAR(64) CHARACTER SET ascii COLLATE ascii_bin
        GENERATED ALWAYS AS (SHA2(canonical_manifest_json, 256)) STORED,
    sealed_at                   DATETIME(6) NOT NULL,
    created_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY uk_business_catalog_key_version (catalog_key, version_no),
    UNIQUE KEY uk_business_catalog_sha (canonical_sha256),
    UNIQUE KEY uk_business_catalog_id_sha (id, canonical_sha256),
    CONSTRAINT ck_business_catalog_identity CHECK (
        catalog_key REGEXP '^[a-z0-9][a-zA-Z0-9._-]{0,95}$'
        AND version_no > 0
        AND schema_version > 0
        AND CHAR_LENGTH(engine_version) > 0
    ),
    CONSTRAINT ck_business_catalog_status CHECK (status = 'sealed'),
    CONSTRAINT ck_business_catalog_manifest CHECK (
        JSON_VALID(canonical_manifest_json)
        AND JSON_TYPE(canonical_manifest_json) = 'OBJECT'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_business_catalog_valid_insert
BEFORE INSERT ON business_catalog_version
FOR EACH ROW
SET NEW.catalog_key = IF(
    NEW.status = 'sealed'
        AND NEW.sealed_at IS NOT NULL
        AND JSON_UNQUOTE(JSON_EXTRACT(NEW.canonical_manifest_json, '$.catalogKey'))
            = NEW.catalog_key
        AND CAST(JSON_UNQUOTE(JSON_EXTRACT(
                NEW.canonical_manifest_json, '$.version')) AS UNSIGNED)
            = NEW.version_no
        AND CAST(JSON_UNQUOTE(JSON_EXTRACT(
                NEW.canonical_manifest_json, '$.schemaVersion')) AS UNSIGNED)
            = NEW.schema_version
        AND JSON_UNQUOTE(JSON_EXTRACT(NEW.canonical_manifest_json, '$.engineVersion'))
            = NEW.engine_version,
    NEW.catalog_key,
    NULL
);

CREATE TRIGGER tr_business_catalog_no_update
BEFORE UPDATE ON business_catalog_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'business catalog versions are immutable';

CREATE TRIGGER tr_business_catalog_no_delete
BEFORE DELETE ON business_catalog_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'business catalog versions are immutable';

CREATE TABLE business_contract_template (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    business_catalog_version_id BIGINT UNSIGNED NOT NULL,
    template_key                VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    industry_template_key       VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                VARCHAR(120) NOT NULL,
    template_order              SMALLINT UNSIGNED NOT NULL,
    required_capacity_units     SMALLINT UNSIGNED NOT NULL,
    duration_months             TINYINT UNSIGNED NOT NULL,
    revenue_krw                 BIGINT NOT NULL,
    variable_cost_ppm           INT UNSIGNED NOT NULL,
    failure_penalty_krw         BIGINT NOT NULL,
    created_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY uk_business_contract_template_key
        (business_catalog_version_id, template_key),
    UNIQUE KEY uk_business_contract_template_id
        (business_catalog_version_id, id),
    KEY ix_business_contract_template_industry
        (business_catalog_version_id, industry_template_key, template_order, id),
    CONSTRAINT fk_business_contract_template_catalog
        FOREIGN KEY (business_catalog_version_id) REFERENCES business_catalog_version (id),
    CONSTRAINT ck_business_contract_template CHECK (
        template_key REGEXP '^[a-z][a-zA-Z0-9._-]{0,95}$'
        AND industry_template_key IN ('softwareService', 'onlineRetail', 'contentStudio')
        AND CHAR_LENGTH(display_name) BETWEEN 1 AND 120
        AND template_order > 0
        AND required_capacity_units BETWEEN 1 AND 1000
        AND duration_months = 1
        AND revenue_krw BETWEEN 1 AND 9007199254740991
        AND variable_cost_ppm BETWEEN 0 AND 1000000
        AND failure_penalty_krw BETWEEN 0 AND 9007199254740991
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE business_role_template (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    business_catalog_version_id BIGINT UNSIGNED NOT NULL,
    role_key                    VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    industry_template_key       VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                VARCHAR(120) NOT NULL,
    role_order                  SMALLINT UNSIGNED NOT NULL,
    career_catalog_bundle_id    BIGINT UNSIGNED NOT NULL,
    career_job_template_id      BIGINT UNSIGNED NOT NULL,
    career_job_family_key       VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    monthly_gross_wage_krw      BIGINT NOT NULL,
    employer_cost_rate_ppm      INT UNSIGNED NOT NULL,
    capacity_units              SMALLINT UNSIGNED NOT NULL,
    maximum_positions           SMALLINT UNSIGNED NOT NULL,
    created_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY uk_business_role_template_key (business_catalog_version_id, role_key),
    UNIQUE KEY uk_business_role_template_id (business_catalog_version_id, id),
    KEY ix_business_role_template_industry
        (business_catalog_version_id, industry_template_key, role_order, id),
    CONSTRAINT fk_business_role_template_catalog
        FOREIGN KEY (business_catalog_version_id) REFERENCES business_catalog_version (id),
    CONSTRAINT fk_business_role_template_job
        FOREIGN KEY (career_catalog_bundle_id, career_job_template_id)
        REFERENCES job_template (career_catalog_bundle_id, id),
    CONSTRAINT ck_business_role_template CHECK (
        role_key REGEXP '^[a-z][a-zA-Z0-9._-]{0,95}$'
        AND industry_template_key IN ('softwareService', 'onlineRetail', 'contentStudio')
        AND CHAR_LENGTH(display_name) BETWEEN 1 AND 120
        AND role_order > 0
        AND CHAR_LENGTH(career_job_family_key) > 0
        AND monthly_gross_wage_krw BETWEEN 1 AND 9007199254740991
        AND employer_cost_rate_ppm BETWEEN 0 AND 1000000
        AND capacity_units BETWEEN 1 AND 1000
        AND maximum_positions BETWEEN 1 AND 100
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE business_marketing_band (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    business_catalog_version_id BIGINT UNSIGNED NOT NULL,
    band_key                    VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                VARCHAR(100) NOT NULL,
    band_order                  SMALLINT UNSIGNED NOT NULL,
    monthly_cost_krw            BIGINT NOT NULL,
    offer_slots                 SMALLINT UNSIGNED NOT NULL,
    created_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY uk_business_marketing_band_key (business_catalog_version_id, band_key),
    UNIQUE KEY uk_business_marketing_band_id (business_catalog_version_id, id),
    CONSTRAINT fk_business_marketing_band_catalog
        FOREIGN KEY (business_catalog_version_id) REFERENCES business_catalog_version (id),
    CONSTRAINT ck_business_marketing_band CHECK (
        band_key IN ('off', 'basic', 'growth')
        AND CHAR_LENGTH(display_name) BETWEEN 1 AND 100
        AND band_order BETWEEN 1 AND 3
        AND monthly_cost_krw BETWEEN 0 AND 9007199254740991
        AND offer_slots BETWEEN 1 AND 16
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE business_loan_product (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    business_catalog_version_id BIGINT UNSIGNED NOT NULL,
    product_key                 VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                VARCHAR(120) NOT NULL,
    minimum_principal_krw       BIGINT NOT NULL,
    maximum_principal_krw       BIGINT NOT NULL,
    principal_step_krw          BIGINT NOT NULL,
    monthly_interest_rate_ppm   INT UNSIGNED NOT NULL,
    term_months                 SMALLINT UNSIGNED NOT NULL,
    personal_guarantee          BOOLEAN NOT NULL,
    created_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY uk_business_loan_product_key (business_catalog_version_id, product_key),
    UNIQUE KEY uk_business_loan_product_id (business_catalog_version_id, id),
    CONSTRAINT fk_business_loan_product_catalog
        FOREIGN KEY (business_catalog_version_id) REFERENCES business_catalog_version (id),
    CONSTRAINT ck_business_loan_product CHECK (
        product_key REGEXP '^[a-z][a-zA-Z0-9._-]{0,95}$'
        AND CHAR_LENGTH(display_name) BETWEEN 1 AND 120
        AND minimum_principal_krw > 0
        AND maximum_principal_krw >= minimum_principal_krw
        AND maximum_principal_krw <= 9007199254740991
        AND principal_step_krw > 0
        AND MOD(minimum_principal_krw, principal_step_krw) = 0
        AND MOD(maximum_principal_krw, principal_step_krw) = 0
        AND monthly_interest_rate_ppm BETWEEN 1 AND 1000000
        AND term_months BETWEEN 1 AND 120
        AND personal_guarantee = FALSE
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_business_contract_template_no_update
BEFORE UPDATE ON business_contract_template
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'business contract templates are immutable';

CREATE TRIGGER tr_business_contract_template_no_delete
BEFORE DELETE ON business_contract_template
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'business contract templates are immutable';

CREATE TRIGGER tr_business_role_template_no_update
BEFORE UPDATE ON business_role_template
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'business role templates are immutable';

CREATE TRIGGER tr_business_role_template_no_delete
BEFORE DELETE ON business_role_template
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'business role templates are immutable';

CREATE TRIGGER tr_business_marketing_band_no_update
BEFORE UPDATE ON business_marketing_band
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'business marketing bands are immutable';

CREATE TRIGGER tr_business_marketing_band_no_delete
BEFORE DELETE ON business_marketing_band
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'business marketing bands are immutable';

CREATE TRIGGER tr_business_loan_product_no_update
BEFORE UPDATE ON business_loan_product
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'business loan products are immutable';

CREATE TRIGGER tr_business_loan_product_no_delete
BEFORE DELETE ON business_loan_product
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'business loan products are immutable';

CREATE TABLE business_catalog_assignment (
    assignment_key              VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    business_catalog_version_id BIGINT UNSIGNED NOT NULL,
    business_catalog_sha256     CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    assignment_revision         BIGINT UNSIGNED NOT NULL,
    updated_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
        ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (assignment_key),
    CONSTRAINT fk_business_catalog_assignment_version
        FOREIGN KEY (business_catalog_version_id, business_catalog_sha256)
        REFERENCES business_catalog_version (id, canonical_sha256),
    CONSTRAINT ck_business_catalog_assignment CHECK (
        assignment_key REGEXP '^[a-z][a-zA-Z0-9._-]{0,63}$'
        AND business_catalog_sha256 REGEXP '^[0-9a-f]{64}$'
        AND assignment_revision > 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

ALTER TABLE ranked_ruleset_release
    ADD COLUMN business_catalog_version_id BIGINT UNSIGNED NULL AFTER offline_policy_sha256,
    ADD COLUMN business_catalog_sha256 CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL
        AFTER business_catalog_version_id,
    ADD CONSTRAINT fk_ranked_ruleset_release_business_catalog
        FOREIGN KEY (business_catalog_version_id, business_catalog_sha256)
        REFERENCES business_catalog_version (id, canonical_sha256),
    ADD CONSTRAINT ck_ranked_ruleset_release_business_catalog CHECK (
        (business_catalog_version_id IS NULL AND business_catalog_sha256 IS NULL)
        OR (
            business_catalog_version_id IS NOT NULL
            AND business_catalog_sha256 REGEXP '^[0-9a-f]{64}$'
        )
    );

ALTER TABLE run_manifest
    ADD COLUMN business_catalog_version_id BIGINT UNSIGNED NULL AFTER offline_policy_sha256,
    ADD COLUMN business_catalog_sha256 CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL
        AFTER business_catalog_version_id,
    ADD UNIQUE KEY uk_run_manifest_business_catalog
        (save_id, run_revision, business_catalog_version_id, business_catalog_sha256),
    ADD CONSTRAINT fk_run_manifest_business_catalog
        FOREIGN KEY (business_catalog_version_id, business_catalog_sha256)
        REFERENCES business_catalog_version (id, canonical_sha256),
    ADD CONSTRAINT ck_run_manifest_business_catalog CHECK (
        (business_catalog_version_id IS NULL AND business_catalog_sha256 IS NULL)
        OR (
            business_catalog_version_id IS NOT NULL
            AND business_catalog_sha256 REGEXP '^[0-9a-f]{64}$'
        )
    );

CREATE TABLE corporation_business_profile (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED NOT NULL,
    corporation_id              BIGINT UNSIGNED NOT NULL,
    business_catalog_version_id BIGINT UNSIGNED NOT NULL,
    business_catalog_sha256     CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    effective_year              SMALLINT UNSIGNED NOT NULL,
    effective_month             TINYINT UNSIGNED NOT NULL,
    control_revision            BIGINT UNSIGNED NOT NULL,
    created_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
        ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY uk_corporation_business_profile (save_id, run_revision, corporation_id),
    UNIQUE KEY uk_corporation_business_profile_scope
        (save_id, run_revision, corporation_id, id),
    CONSTRAINT fk_corporation_business_profile_corporation
        FOREIGN KEY (save_id, run_revision, corporation_id)
        REFERENCES corporation (save_id, run_revision, id),
    CONSTRAINT fk_corporation_business_profile_manifest
        FOREIGN KEY (
            save_id, run_revision, business_catalog_version_id, business_catalog_sha256
        ) REFERENCES run_manifest (
            save_id, run_revision, business_catalog_version_id, business_catalog_sha256
        ),
    CONSTRAINT ck_corporation_business_profile CHECK (
        business_catalog_sha256 REGEXP '^[0-9a-f]{64}$'
        AND effective_year BETWEEN 1 AND 9999
        AND effective_month BETWEEN 1 AND 12
        AND control_revision > 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE corporation_staff_position (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED NOT NULL,
    corporation_id              BIGINT UNSIGNED NOT NULL,
    business_profile_id         BIGINT UNSIGNED NOT NULL,
    business_catalog_version_id BIGINT UNSIGNED NOT NULL,
    role_template_id            BIGINT UNSIGNED NOT NULL,
    position_no                 SMALLINT UNSIGNED NOT NULL,
    status                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    effective_year              SMALLINT UNSIGNED NULL,
    effective_month             TINYINT UNSIGNED NULL,
    hired_command_id            CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    ended_command_id            CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    created_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
        ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY uk_corporation_staff_position_no
        (save_id, run_revision, corporation_id, role_template_id, position_no),
    UNIQUE KEY uk_corporation_staff_position_scope
        (save_id, run_revision, corporation_id, id),
    CONSTRAINT fk_corporation_staff_position_profile
        FOREIGN KEY (save_id, run_revision, corporation_id, business_profile_id)
        REFERENCES corporation_business_profile (save_id, run_revision, corporation_id, id),
    CONSTRAINT fk_corporation_staff_position_role
        FOREIGN KEY (business_catalog_version_id, role_template_id)
        REFERENCES business_role_template (business_catalog_version_id, id),
    CONSTRAINT fk_corporation_staff_position_hired_command
        FOREIGN KEY (save_id, hired_command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT fk_corporation_staff_position_ended_command
        FOREIGN KEY (save_id, ended_command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT ck_corporation_staff_position CHECK (
        position_no > 0
        AND status IN ('vacant', 'hired', 'active', 'resigned', 'terminated')
        AND (
            (status = 'vacant' AND effective_year IS NULL AND effective_month IS NULL
             AND hired_command_id IS NULL AND ended_command_id IS NULL)
            OR
            (status IN ('hired', 'active') AND effective_year BETWEEN 1 AND 9999
             AND effective_month BETWEEN 1 AND 12 AND hired_command_id IS NOT NULL
             AND ended_command_id IS NULL)
            OR
            (status IN ('resigned', 'terminated') AND effective_year BETWEEN 1 AND 9999
             AND effective_month BETWEEN 1 AND 12 AND hired_command_id IS NOT NULL)
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE corporation_staff_transition (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED NOT NULL,
    corporation_id              BIGINT UNSIGNED NOT NULL,
    position_id                 BIGINT UNSIGNED NOT NULL,
    transition_no               SMALLINT UNSIGNED NOT NULL,
    from_status                 VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    to_status                   VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    command_id                  CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    effective_year              SMALLINT UNSIGNED NOT NULL,
    effective_month             TINYINT UNSIGNED NOT NULL,
    transition_game_day         INT UNSIGNED NOT NULL,
    created_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY uk_corporation_staff_transition_no (position_id, transition_no),
    CONSTRAINT fk_corporation_staff_transition_position
        FOREIGN KEY (save_id, run_revision, corporation_id, position_id)
        REFERENCES corporation_staff_position (save_id, run_revision, corporation_id, id),
    CONSTRAINT fk_corporation_staff_transition_command
        FOREIGN KEY (save_id, command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT ck_corporation_staff_transition CHECK (
        transition_no BETWEEN 1 AND 64
        AND from_status IN ('vacant', 'hired', 'active')
        AND to_status IN ('hired', 'active', 'resigned', 'terminated')
        AND from_status <> to_status
        AND effective_year BETWEEN 1 AND 9999
        AND effective_month BETWEEN 1 AND 12
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE corporation_customer_contract (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED NOT NULL,
    corporation_id              BIGINT UNSIGNED NOT NULL,
    business_profile_id         BIGINT UNSIGNED NOT NULL,
    business_catalog_version_id BIGINT UNSIGNED NOT NULL,
    contract_template_id        BIGINT UNSIGNED NOT NULL,
    occurrence_no               SMALLINT UNSIGNED NOT NULL,
    status                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    offered_year                SMALLINT UNSIGNED NOT NULL,
    offered_month               TINYINT UNSIGNED NOT NULL,
    service_year                SMALLINT UNSIGNED NOT NULL,
    service_month               TINYINT UNSIGNED NOT NULL,
    offer_entropy_word          BIGINT UNSIGNED NOT NULL,
    required_capacity_units     SMALLINT UNSIGNED NOT NULL,
    revenue_krw                 BIGINT NOT NULL,
    variable_cost_ppm           INT UNSIGNED NOT NULL,
    failure_penalty_krw         BIGINT NOT NULL,
    accepted_command_id         CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    terminal_command_id         CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    terminal_game_day           INT UNSIGNED NULL,
    created_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
        ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY uk_corporation_customer_contract_occurrence
        (save_id, run_revision, corporation_id, service_year, service_month,
         contract_template_id, occurrence_no),
    UNIQUE KEY uk_corporation_customer_contract_scope
        (save_id, run_revision, corporation_id, id),
    CONSTRAINT fk_corporation_customer_contract_profile
        FOREIGN KEY (save_id, run_revision, corporation_id, business_profile_id)
        REFERENCES corporation_business_profile (save_id, run_revision, corporation_id, id),
    CONSTRAINT fk_corporation_customer_contract_template
        FOREIGN KEY (business_catalog_version_id, contract_template_id)
        REFERENCES business_contract_template (business_catalog_version_id, id),
    CONSTRAINT fk_corporation_customer_contract_accepted_command
        FOREIGN KEY (save_id, accepted_command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT fk_corporation_customer_contract_terminal_command
        FOREIGN KEY (save_id, terminal_command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT ck_corporation_customer_contract CHECK (
        occurrence_no > 0
        AND status IN ('offered', 'accepted', 'active', 'completed', 'failed', 'cancelled')
        AND offered_year BETWEEN 1 AND 9999 AND offered_month BETWEEN 1 AND 12
        AND service_year BETWEEN 1 AND 9999 AND service_month BETWEEN 1 AND 12
        AND required_capacity_units BETWEEN 1 AND 1000
        AND revenue_krw BETWEEN 1 AND 9007199254740991
        AND variable_cost_ppm BETWEEN 0 AND 1000000
        AND failure_penalty_krw BETWEEN 0 AND 9007199254740991
        AND (
            (status = 'offered' AND accepted_command_id IS NULL
             AND terminal_command_id IS NULL AND terminal_game_day IS NULL)
            OR
            (status IN ('accepted', 'active') AND accepted_command_id IS NOT NULL
             AND terminal_command_id IS NULL AND terminal_game_day IS NULL)
            OR
            (status IN ('completed', 'failed') AND accepted_command_id IS NOT NULL
             AND terminal_command_id IS NULL AND terminal_game_day IS NOT NULL)
            OR
            (status = 'cancelled' AND terminal_command_id IS NOT NULL
             AND terminal_game_day IS NOT NULL)
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE corporation_contract_transition (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED NOT NULL,
    corporation_id              BIGINT UNSIGNED NOT NULL,
    contract_id                 BIGINT UNSIGNED NOT NULL,
    transition_no               SMALLINT UNSIGNED NOT NULL,
    from_status                 VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    to_status                   VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    command_id                  CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    transition_game_day         INT UNSIGNED NOT NULL,
    created_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY uk_corporation_contract_transition_no (contract_id, transition_no),
    CONSTRAINT fk_corporation_contract_transition_contract
        FOREIGN KEY (save_id, run_revision, corporation_id, contract_id)
        REFERENCES corporation_customer_contract (save_id, run_revision, corporation_id, id),
    CONSTRAINT fk_corporation_contract_transition_command
        FOREIGN KEY (save_id, command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT ck_corporation_contract_transition CHECK (
        transition_no BETWEEN 1 AND 64
        AND from_status IN ('offered', 'accepted', 'active')
        AND to_status IN ('accepted', 'active', 'completed', 'failed', 'cancelled')
        AND from_status <> to_status
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE corporation_monthly_plan (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED NOT NULL,
    corporation_id              BIGINT UNSIGNED NOT NULL,
    business_profile_id         BIGINT UNSIGNED NOT NULL,
    business_catalog_version_id BIGINT UNSIGNED NOT NULL,
    marketing_band_id           BIGINT UNSIGNED NOT NULL,
    effective_year              SMALLINT UNSIGNED NOT NULL,
    effective_month             TINYINT UNSIGNED NOT NULL,
    plan_revision               BIGINT UNSIGNED NOT NULL,
    cash_buffer_krw             BIGINT NOT NULL,
    contract_priority_json      JSON NOT NULL,
    command_id                  CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_game_day            INT UNSIGNED NOT NULL,
    created_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY uk_corporation_monthly_plan_revision
        (save_id, run_revision, corporation_id, effective_year, effective_month, plan_revision),
    UNIQUE KEY uk_corporation_monthly_plan_command (save_id, command_id),
    UNIQUE KEY uk_corporation_monthly_plan_scope
        (save_id, run_revision, corporation_id, id),
    KEY ix_corporation_monthly_plan_effective
        (save_id, run_revision, corporation_id, effective_year, effective_month,
         plan_revision DESC),
    CONSTRAINT fk_corporation_monthly_plan_profile
        FOREIGN KEY (save_id, run_revision, corporation_id, business_profile_id)
        REFERENCES corporation_business_profile (save_id, run_revision, corporation_id, id),
    CONSTRAINT fk_corporation_monthly_plan_marketing
        FOREIGN KEY (business_catalog_version_id, marketing_band_id)
        REFERENCES business_marketing_band (business_catalog_version_id, id),
    CONSTRAINT fk_corporation_monthly_plan_command
        FOREIGN KEY (save_id, command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT ck_corporation_monthly_plan CHECK (
        effective_year BETWEEN 1 AND 9999 AND effective_month BETWEEN 1 AND 12
        AND plan_revision > 0
        AND cash_buffer_krw BETWEEN 0 AND 9007199254740991
        AND JSON_TYPE(contract_priority_json) = 'ARRAY'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE corporation_business_month (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                     BIGINT UNSIGNED NOT NULL,
    run_revision                INT UNSIGNED NOT NULL,
    corporation_id              BIGINT UNSIGNED NOT NULL,
    business_profile_id         BIGINT UNSIGNED NOT NULL,
    business_catalog_version_id BIGINT UNSIGNED NOT NULL,
    corporation_operating_month_id BIGINT UNSIGNED NOT NULL,
    monthly_plan_id             BIGINT UNSIGNED NULL,
    operating_year              SMALLINT UNSIGNED NOT NULL,
    operating_month             TINYINT UNSIGNED NOT NULL,
    owner_capacity_units        SMALLINT UNSIGNED NOT NULL,
    employee_capacity_units     INT UNSIGNED NOT NULL,
    total_capacity_units        INT UNSIGNED NOT NULL,
    used_capacity_units         INT UNSIGNED NOT NULL,
    marketing_cost_krw          BIGINT NOT NULL,
    employee_gross_wage_krw     BIGINT NOT NULL,
    employee_employer_cost_krw  BIGINT NOT NULL,
    contract_revenue_krw        BIGINT NOT NULL,
    contract_variable_cost_krw  BIGINT NOT NULL,
    failed_contract_penalty_krw BIGINT NOT NULL,
    receivable_opening_krw      BIGINT NOT NULL,
    receivable_created_krw      BIGINT NOT NULL,
    receivable_collected_krw    BIGINT NOT NULL,
    receivable_closing_krw      BIGINT NOT NULL,
    completed_contract_count    SMALLINT UNSIGNED NOT NULL,
    failed_contract_count       SMALLINT UNSIGNED NOT NULL,
    active_employee_count       SMALLINT UNSIGNED NOT NULL,
    cash_buffer_krw             BIGINT NOT NULL,
    applied_game_day            INT UNSIGNED NOT NULL,
    created_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY uk_corporation_business_month
        (save_id, run_revision, corporation_id, operating_year, operating_month),
    UNIQUE KEY uk_corporation_business_month_operating
        (corporation_operating_month_id),
    CONSTRAINT fk_corporation_business_month_profile
        FOREIGN KEY (save_id, run_revision, corporation_id, business_profile_id)
        REFERENCES corporation_business_profile (save_id, run_revision, corporation_id, id),
    CONSTRAINT fk_corporation_business_month_operating
        FOREIGN KEY (save_id, run_revision, corporation_id, corporation_operating_month_id)
        REFERENCES corporation_operating_month (save_id, run_revision, corporation_id, id),
    CONSTRAINT fk_corporation_business_month_plan
        FOREIGN KEY (save_id, run_revision, corporation_id, monthly_plan_id)
        REFERENCES corporation_monthly_plan (save_id, run_revision, corporation_id, id),
    CONSTRAINT ck_corporation_business_month CHECK (
        operating_year BETWEEN 1 AND 9999 AND operating_month BETWEEN 1 AND 12
        AND owner_capacity_units BETWEEN 1 AND 1000
        AND total_capacity_units = owner_capacity_units + employee_capacity_units
        AND used_capacity_units <= total_capacity_units
        AND marketing_cost_krw BETWEEN 0 AND 9007199254740991
        AND employee_gross_wage_krw BETWEEN 0 AND 9007199254740991
        AND employee_employer_cost_krw BETWEEN 0 AND 9007199254740991
        AND contract_revenue_krw BETWEEN 0 AND 9007199254740991
        AND contract_variable_cost_krw BETWEEN 0 AND 9007199254740991
        AND failed_contract_penalty_krw BETWEEN 0 AND 9007199254740991
        AND receivable_opening_krw = 0
        AND receivable_created_krw = contract_revenue_krw
        AND receivable_collected_krw = contract_revenue_krw
        AND receivable_closing_krw = 0
        AND cash_buffer_krw BETWEEN 0 AND 9007199254740991
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE corporation_operation_command_receipt (
    save_id                     BIGINT UNSIGNED NOT NULL,
    command_id                  CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    run_revision                INT UNSIGNED NOT NULL,
    corporation_id              BIGINT UNSIGNED NOT NULL,
    command_kind                VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    payload_sha256              CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    result_json                 JSON NOT NULL,
    created_at                  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (save_id, command_id),
    KEY ix_corporation_operation_receipt_scope
        (save_id, run_revision, corporation_id, created_at),
    CONSTRAINT fk_corporation_operation_receipt_corporation
        FOREIGN KEY (save_id, run_revision, corporation_id)
        REFERENCES corporation (save_id, run_revision, id),
    CONSTRAINT fk_corporation_operation_receipt_command
        FOREIGN KEY (save_id, command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT ck_corporation_operation_receipt CHECK (
        command_kind IN (
            'acceptContract', 'cancelContract', 'hirePosition', 'terminatePosition',
            'setMonthlyPlan', 'capitalContribution', 'drawWorkingCapitalLoan',
            'repayWorkingCapitalLoan', 'dissolveCorporation'
        )
        AND payload_sha256 REGEXP '^[0-9a-f]{64}$'
        AND JSON_TYPE(result_json) = 'OBJECT'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_corporation_staff_transition_no_update
BEFORE UPDATE ON corporation_staff_transition
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation staff transitions are immutable';

CREATE TRIGGER tr_corporation_staff_transition_no_delete
BEFORE DELETE ON corporation_staff_transition
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation staff transitions are immutable';

CREATE TRIGGER tr_corporation_contract_transition_no_update
BEFORE UPDATE ON corporation_contract_transition
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation contract transitions are immutable';

CREATE TRIGGER tr_corporation_contract_transition_no_delete
BEFORE DELETE ON corporation_contract_transition
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation contract transitions are immutable';

CREATE TRIGGER tr_corporation_monthly_plan_no_update
BEFORE UPDATE ON corporation_monthly_plan
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation monthly plans are immutable';

CREATE TRIGGER tr_corporation_monthly_plan_no_delete
BEFORE DELETE ON corporation_monthly_plan
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation monthly plans are immutable';

CREATE TRIGGER tr_corporation_business_month_no_update
BEFORE UPDATE ON corporation_business_month
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation business months are immutable';

CREATE TRIGGER tr_corporation_business_month_no_delete
BEFORE DELETE ON corporation_business_month
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation business months are immutable';

CREATE TRIGGER tr_corporation_operation_receipt_no_update
BEFORE UPDATE ON corporation_operation_command_receipt
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation operation receipts are immutable';

CREATE TRIGGER tr_corporation_operation_receipt_no_delete
BEFORE DELETE ON corporation_operation_command_receipt
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation operation receipts are immutable';

INSERT INTO business_catalog_version
    (catalog_key, version_no, schema_version, status, engine_version,
     canonical_manifest_json, sealed_at)
VALUES (
    'm5e-business-operations-v1', 1, 1, 'sealed', 'm5a-dev-v1',
    CAST(JSON_OBJECT(
        'catalogKey', 'm5e-business-operations-v1',
        'contractTemplates', JSON_ARRAY(
            JSON_OBJECT('durationMonths', 1, 'failurePenaltyKrw', 500000,
                'industryTemplateKey', 'softwareService', 'requiredCapacityUnits', 4,
                'revenueKrw', 6000000, 'templateKey', 'maintenanceRetainer',
                'variableCostPpm', 100000),
            JSON_OBJECT('durationMonths', 1, 'failurePenaltyKrw', 750000,
                'industryTemplateKey', 'onlineRetail', 'requiredCapacityUnits', 5,
                'revenueKrw', 12000000, 'templateKey', 'seasonalFulfillment',
                'variableCostPpm', 400000),
            JSON_OBJECT('durationMonths', 1, 'failurePenaltyKrw', 600000,
                'industryTemplateKey', 'contentStudio', 'requiredCapacityUnits', 4,
                'revenueKrw', 8000000, 'templateKey', 'campaignProduction',
                'variableCostPpm', 200000)
        ),
        'engineVersion', 'm5a-dev-v1',
        'loanProducts', JSON_ARRAY(
            JSON_OBJECT('maximumPrincipalKrw', 50000000, 'minimumPrincipalKrw', 5000000,
                'monthlyInterestRatePpm', 5000, 'personalGuarantee', FALSE,
                'principalStepKrw', 1000000, 'productKey', 'workingCapital12m',
                'termMonths', 12)
        ),
        'marketingBands', JSON_ARRAY(
            JSON_OBJECT('bandKey', 'off', 'monthlyCostKrw', 0, 'offerSlots', 1),
            JSON_OBJECT('bandKey', 'basic', 'monthlyCostKrw', 500000, 'offerSlots', 2),
            JSON_OBJECT('bandKey', 'growth', 'monthlyCostKrw', 1500000, 'offerSlots', 3)
        ),
        'ownerCapacityByScale', JSON_OBJECT('growth', 6, 'lean', 2, 'standard', 4),
        'roleTemplates', JSON_ARRAY(
            JSON_OBJECT('capacityUnits', 3, 'careerJobFamilyKey', 'softwareEngineering',
                'employerCostRatePpm', 110000, 'industryTemplateKey', 'softwareService',
                'maximumPositions', 2, 'roleKey', 'softwareEngineer'),
            JSON_OBJECT('capacityUnits', 3, 'careerJobFamilyKey', 'retailOperations',
                'employerCostRatePpm', 110000, 'industryTemplateKey', 'onlineRetail',
                'maximumPositions', 2, 'roleKey', 'retailOperator'),
            JSON_OBJECT('capacityUnits', 3, 'careerJobFamilyKey', 'dataEngineering',
                'employerCostRatePpm', 110000, 'industryTemplateKey', 'contentStudio',
                'maximumPositions', 2, 'roleKey', 'contentProducer')
        ),
        'schemaVersion', 1,
        'version', 1
    ) AS CHAR CHARACTER SET utf8mb4),
    CURRENT_TIMESTAMP(6)
);

INSERT INTO business_contract_template
    (business_catalog_version_id, template_key, industry_template_key, display_name,
     template_order, required_capacity_units, duration_months, revenue_krw,
     variable_cost_ppm, failure_penalty_krw)
SELECT catalog.id, seed.template_key, seed.industry_template_key, seed.display_name,
       seed.template_order, seed.required_capacity_units, 1, seed.revenue_krw,
       seed.variable_cost_ppm, seed.failure_penalty_krw
FROM business_catalog_version AS catalog
INNER JOIN (
    SELECT 'maintenanceRetainer' AS template_key, 'softwareService' AS industry_template_key,
           '유지보수 리테이너' AS display_name, 1 AS template_order,
           4 AS required_capacity_units, 6000000 AS revenue_krw,
           100000 AS variable_cost_ppm, 500000 AS failure_penalty_krw
    UNION ALL
    SELECT 'seasonalFulfillment', 'onlineRetail', '시즌 풀필먼트', 1, 5,
           12000000, 400000, 750000
    UNION ALL
    SELECT 'campaignProduction', 'contentStudio', '캠페인 제작', 1, 4,
           8000000, 200000, 600000
) AS seed
WHERE catalog.catalog_key = 'm5e-business-operations-v1' AND catalog.version_no = 1;

INSERT INTO business_role_template
    (business_catalog_version_id, role_key, industry_template_key, display_name,
     role_order, career_catalog_bundle_id, career_job_template_id, career_job_family_key,
     monthly_gross_wage_krw, employer_cost_rate_ppm, capacity_units, maximum_positions)
SELECT catalog.id, seed.role_key, seed.industry_template_key, seed.display_name,
       seed.role_order, bundle.id, job.id, family.job_family_key,
       FLOOR(job.minimum_annual_salary_krw / 12), 110000, 3, 2
FROM business_catalog_version AS catalog
INNER JOIN (
    SELECT 'softwareEngineer' AS role_key, 'softwareService' AS industry_template_key,
           '소프트웨어 엔지니어' AS display_name, 1 AS role_order,
           'softwareEngineering' AS job_family_key
    UNION ALL
    SELECT 'retailOperator', 'onlineRetail', '리테일 운영 담당자', 1, 'retailOperations'
    UNION ALL
    SELECT 'contentProducer', 'contentStudio', '콘텐츠 제작자', 1, 'dataEngineering'
) AS seed
INNER JOIN career_catalog_bundle AS bundle
    ON bundle.bundle_key = 'dev-unranked-m3-v1'
INNER JOIN career_job_family AS family
    ON family.career_catalog_bundle_id = bundle.id
   AND family.job_family_key = seed.job_family_key
INNER JOIN job_template AS job
    ON job.career_catalog_bundle_id = bundle.id
   AND job.career_job_family_id = family.id
WHERE catalog.catalog_key = 'm5e-business-operations-v1' AND catalog.version_no = 1;

INSERT INTO business_marketing_band
    (business_catalog_version_id, band_key, display_name, band_order,
     monthly_cost_krw, offer_slots)
SELECT catalog.id, seed.band_key, seed.display_name, seed.band_order,
       seed.monthly_cost_krw, seed.offer_slots
FROM business_catalog_version AS catalog
INNER JOIN (
    SELECT 'off' AS band_key, '마케팅 없음' AS display_name, 1 AS band_order,
           0 AS monthly_cost_krw, 1 AS offer_slots
    UNION ALL SELECT 'basic', '기본 마케팅', 2, 500000, 2
    UNION ALL SELECT 'growth', '성장 마케팅', 3, 1500000, 3
) AS seed
WHERE catalog.catalog_key = 'm5e-business-operations-v1' AND catalog.version_no = 1;

INSERT INTO business_loan_product
    (business_catalog_version_id, product_key, display_name, minimum_principal_krw,
     maximum_principal_krw, principal_step_krw, monthly_interest_rate_ppm,
     term_months, personal_guarantee)
SELECT catalog.id, 'workingCapital12m', '12개월 운전자금 대출',
       5000000, 50000000, 1000000, 5000, 12, FALSE
FROM business_catalog_version AS catalog
WHERE catalog.catalog_key = 'm5e-business-operations-v1' AND catalog.version_no = 1;

INSERT INTO business_catalog_assignment
    (assignment_key, business_catalog_version_id, business_catalog_sha256,
     assignment_revision)
SELECT 'newSandboxRun', catalog.id, catalog.canonical_sha256, 1
FROM business_catalog_version AS catalog
WHERE catalog.catalog_key = 'm5e-business-operations-v1' AND catalog.version_no = 1;
