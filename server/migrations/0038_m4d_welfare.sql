-- M4-D1 typed welfare evaluation, application, and phase-150 payment (§6.1–§6.7).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;
SET SESSION group_concat_max_len = 1048576;

-- DDL auto-commits in MySQL. Preserve the exact pre-migration pins so the final publication
-- barrier can prove that only the newRun pointer moved.
CREATE TEMPORARY TABLE m4d_existing_run_life_pins (
    save_id                 BIGINT UNSIGNED NOT NULL,
    run_revision            INT UNSIGNED    NOT NULL,
    life_catalog_set_id     BIGINT UNSIGNED NOT NULL,
    PRIMARY KEY (save_id, run_revision)
) ENGINE = InnoDB;

INSERT INTO m4d_existing_run_life_pins
    (save_id, run_revision, life_catalog_set_id)
SELECT save_id, run_revision, life_catalog_set_id
FROM run_rule_bundle;

CREATE TEMPORARY TABLE m4d_previous_new_run_life (
    assignment_revision                     BIGINT UNSIGNED NOT NULL,
    life_catalog_set_id                     BIGINT UNSIGNED NOT NULL,
    legacy_dependent_age_years               TINYINT UNSIGNED NOT NULL,
    living_cost_component_version_id         BIGINT UNSIGNED NOT NULL,
    welfare_component_version_id             BIGINT UNSIGNED NOT NULL,
    life_event_component_version_id          BIGINT UNSIGNED NOT NULL,
    insurance_component_version_id           BIGINT UNSIGNED NOT NULL,
    corporation_component_version_id         BIGINT UNSIGNED NOT NULL,
    PRIMARY KEY (life_catalog_set_id)
) ENGINE = InnoDB;

INSERT INTO m4d_previous_new_run_life
    (
        assignment_revision,
        life_catalog_set_id,
        legacy_dependent_age_years,
        living_cost_component_version_id,
        welfare_component_version_id,
        life_event_component_version_id,
        insurance_component_version_id,
        corporation_component_version_id
    )
SELECT assignment.assignment_revision,
       catalog.id,
       catalog.legacy_dependent_age_years,
       catalog.living_cost_component_version_id,
       catalog.welfare_component_version_id,
       catalog.life_event_component_version_id,
       catalog.insurance_component_version_id,
       catalog.corporation_component_version_id
FROM run_rule_bundle_assignment AS assignment
INNER JOIN life_catalog_set AS catalog
    ON catalog.id = assignment.life_catalog_set_id
WHERE assignment.assignment_key = 'newRun';

ALTER TABLE life_catalog_set
    ADD UNIQUE KEY uk_life_catalog_set_welfare_component
        (id, welfare_component_version_id);

CREATE TABLE welfare_fact_definition (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    life_component_version_id       BIGINT UNSIGNED NOT NULL,
    fact_order                      TINYINT UNSIGNED NOT NULL,
    fact_key                        VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    value_type                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    unit                            VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    enum_schema_key                 VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    window_kind                     VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    minimum_window_days             SMALLINT UNSIGNED NULL,
    maximum_window_days             SMALLINT UNSIGNED NULL,
    collection_bound                TINYINT UNSIGNED NULL,
    source_schema_version           SMALLINT UNSIGNED NOT NULL,
    source_kind                     VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_welfare_fact_definition_order
        (life_component_version_id, fact_order),
    UNIQUE KEY uk_welfare_fact_definition_key
        (life_component_version_id, fact_key),
    UNIQUE KEY uk_welfare_fact_definition_component_id
        (life_component_version_id, id),
    CONSTRAINT fk_welfare_fact_definition_component
        FOREIGN KEY (life_component_version_id) REFERENCES life_component_version (id),
    CONSTRAINT ck_welfare_fact_definition_order CHECK (fact_order BETWEEN 1 AND 32),
    CONSTRAINT ck_welfare_fact_definition_key CHECK (
        fact_key REGEXP '^[a-z][a-zA-Z0-9.]{0,63}$'
    ),
    CONSTRAINT ck_welfare_fact_definition_type CHECK (
        value_type IN (
            'boolean', 'integer', 'moneyKrw', 'count', 'ageYears',
            'date', 'string', 'enum'
        )
        AND (
            (value_type = 'boolean' AND unit = 'boolean')
            OR (value_type = 'integer' AND unit = 'integer')
            OR (value_type = 'moneyKrw' AND unit = 'krw')
            OR (value_type = 'count' AND unit = 'count')
            OR (value_type = 'ageYears' AND unit = 'years')
            OR (value_type = 'date' AND unit = 'date')
            OR (value_type = 'string' AND unit = 'string')
            OR (value_type = 'enum' AND unit = 'enum')
        )
        AND (
            (
                value_type = 'enum'
                AND enum_schema_key IS NOT NULL
                AND enum_schema_key REGEXP '^[a-z][a-zA-Z0-9]{0,63}$'
            )
            OR (value_type <> 'enum' AND enum_schema_key IS NULL)
        )
    ),
    CONSTRAINT ck_welfare_fact_definition_window CHECK (
        (
            window_kind IN ('currentGameDay', 'priorClose')
            AND minimum_window_days IS NULL
            AND maximum_window_days IS NULL
        )
        OR (
            window_kind = 'previousClosedDays'
            AND minimum_window_days IS NOT NULL
            AND maximum_window_days IS NOT NULL
            AND minimum_window_days = 1
            AND maximum_window_days = 366
        )
    ),
    CONSTRAINT ck_welfare_fact_definition_collection CHECK (
        collection_bound IS NULL OR collection_bound BETWEEN 1 AND 32
    ),
    CONSTRAINT ck_welfare_fact_definition_schema CHECK (source_schema_version = 1),
    CONSTRAINT ck_welfare_fact_definition_source CHECK (
        source_kind IN (
            'gameDay', 'household', 'residence', 'employment',
            'military', 'income', 'asset', 'debt'
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE welfare_program_version (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    life_component_version_id       BIGINT UNSIGNED NOT NULL,
    schema_version                  SMALLINT UNSIGNED NOT NULL,
    program_key                     VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                    VARCHAR(80) NOT NULL,
    purpose                         VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ranked_availability             VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    application_kind                VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    application_period_kind         VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    application_start_game_day      INT UNSIGNED NULL,
    application_end_game_day        INT UNSIGNED NULL,
    duplicate_group_key             VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    duplicate_scope                 VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    maximum_approved_per_group      TINYINT UNSIGNED NOT NULL,
    reassessment_basis              VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ast_node_count                  SMALLINT UNSIGNED NOT NULL,
    ast_max_depth                   TINYINT UNSIGNED NOT NULL,
    eligibility_ast                JSON NOT NULL,
    benefit_formula                 JSON NOT NULL,
    payment_schedule                JSON NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_welfare_program_version_key
        (life_component_version_id, program_key),
    UNIQUE KEY uk_welfare_program_version_component_id
        (life_component_version_id, id),
    CONSTRAINT fk_welfare_program_version_component
        FOREIGN KEY (life_component_version_id) REFERENCES life_component_version (id),
    CONSTRAINT ck_welfare_program_version_schema CHECK (schema_version = 1),
    CONSTRAINT ck_welfare_program_version_key CHECK (
        program_key REGEXP '^[a-z][a-zA-Z0-9]{0,63}$'
        AND duplicate_group_key REGEXP '^[a-z][a-zA-Z0-9]{0,63}$'
    ),
    CONSTRAINT ck_welfare_program_version_name CHECK (
        CHAR_LENGTH(display_name) BETWEEN 1 AND 80
    ),
    CONSTRAINT ck_welfare_program_version_purpose CHECK (
        purpose IN ('gameBalance', 'realPolicyReference')
        AND ranked_availability IN ('unrankedOnly', 'rankedAndUnranked')
    ),
    CONSTRAINT ck_welfare_program_version_application CHECK (
        application_kind = 'manual'
        AND application_period_kind = 'always'
        AND application_start_game_day IS NULL
        AND application_end_game_day IS NULL
    ),
    CONSTRAINT ck_welfare_program_version_duplicate CHECK (
        duplicate_scope = 'run' AND maximum_approved_per_group = 1
    ),
    CONSTRAINT ck_welfare_program_version_reassessment CHECK (
        reassessment_basis = 'eligibilityAtApplication'
    ),
    CONSTRAINT ck_welfare_program_version_ast_bound CHECK (
        ast_node_count BETWEEN 1 AND 128
        AND ast_max_depth BETWEEN 1 AND 12
        AND JSON_TYPE(eligibility_ast) = 'OBJECT'
    ),
    CONSTRAINT ck_welfare_program_version_benefit CHECK (
        JSON_TYPE(benefit_formula) = 'OBJECT'
        AND JSON_TYPE(payment_schedule) = 'OBJECT'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE welfare_program_constant (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    program_version_id              BIGINT UNSIGNED NOT NULL,
    constant_order                  TINYINT UNSIGNED NOT NULL,
    constant_key                    VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    value_type                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    unit                            VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    enum_schema_key                 VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NULL,
    boolean_value                   BOOLEAN NULL,
    integer_value                   BIGINT NULL,
    string_value                    VARCHAR(64) NULL,
    date_value                      DATE NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_welfare_program_constant_order
        (program_version_id, constant_order),
    UNIQUE KEY uk_welfare_program_constant_key
        (program_version_id, constant_key),
    UNIQUE KEY uk_welfare_program_constant_program_id
        (program_version_id, id),
    CONSTRAINT fk_welfare_program_constant_program
        FOREIGN KEY (program_version_id) REFERENCES welfare_program_version (id),
    CONSTRAINT ck_welfare_program_constant_order CHECK (constant_order BETWEEN 1 AND 64),
    CONSTRAINT ck_welfare_program_constant_key CHECK (
        constant_key REGEXP '^[a-z][a-zA-Z0-9]{0,63}$'
    ),
    CONSTRAINT ck_welfare_program_constant_shape CHECK (
        (
            value_type = 'boolean'
            AND unit = 'boolean'
            AND enum_schema_key IS NULL
            AND boolean_value IS NOT NULL
            AND boolean_value IN (FALSE, TRUE)
            AND integer_value IS NULL
            AND string_value IS NULL
            AND date_value IS NULL
        )
        OR (
            value_type IN ('integer', 'moneyKrw', 'count', 'ageYears')
            AND enum_schema_key IS NULL
            AND boolean_value IS NULL
            AND integer_value IS NOT NULL
            AND string_value IS NULL
            AND date_value IS NULL
            AND (
                (value_type = 'integer' AND unit = 'integer')
                OR (value_type = 'moneyKrw' AND unit = 'krw')
                OR (value_type = 'count' AND unit = 'count')
                OR (value_type = 'ageYears' AND unit = 'years')
            )
        )
        OR (
            value_type IN ('string', 'enum')
            AND boolean_value IS NULL
            AND integer_value IS NULL
            AND string_value IS NOT NULL
            AND CHAR_LENGTH(string_value) BETWEEN 1 AND 64
            AND date_value IS NULL
            AND unit = IF(value_type = 'enum', 'enum', 'string')
            AND (
                (value_type = 'string' AND enum_schema_key IS NULL)
                OR (
                    value_type = 'enum'
                    AND enum_schema_key IS NOT NULL
                    AND enum_schema_key IN ('region', 'welfareEmployment', 'military')
                )
            )
        )
        OR (
            value_type = 'date'
            AND enum_schema_key IS NULL
            AND boolean_value IS NULL
            AND integer_value IS NULL
            AND string_value IS NULL
            AND date_value IS NOT NULL
            AND unit = 'date'
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE welfare_program_condition (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    program_version_id              BIGINT UNSIGNED NOT NULL,
    condition_order                 TINYINT UNSIGNED NOT NULL,
    condition_code                  VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    public_label                    VARCHAR(80) NOT NULL,
    node_count                      SMALLINT UNSIGNED NOT NULL,
    max_depth                       TINYINT UNSIGNED NOT NULL,
    expression_ast                  JSON NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_welfare_program_condition_order
        (program_version_id, condition_order),
    UNIQUE KEY uk_welfare_program_condition_code
        (program_version_id, condition_code),
    UNIQUE KEY uk_welfare_program_condition_program_id
        (program_version_id, id),
    CONSTRAINT fk_welfare_program_condition_program
        FOREIGN KEY (program_version_id) REFERENCES welfare_program_version (id),
    CONSTRAINT ck_welfare_program_condition_order CHECK (condition_order BETWEEN 1 AND 32),
    CONSTRAINT ck_welfare_program_condition_code CHECK (
        condition_code REGEXP '^[a-z][a-zA-Z0-9]{0,63}$'
    ),
    CONSTRAINT ck_welfare_program_condition_label CHECK (
        CHAR_LENGTH(public_label) BETWEEN 1 AND 80
    ),
    CONSTRAINT ck_welfare_program_condition_ast CHECK (
        node_count BETWEEN 1 AND 128
        AND max_depth BETWEEN 1 AND 12
        AND JSON_TYPE(expression_ast) = 'OBJECT'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE welfare_reassessment_trigger (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    program_version_id              BIGINT UNSIGNED NOT NULL,
    trigger_order                   TINYINT UNSIGNED NOT NULL,
    source_kind                     VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_welfare_reassessment_trigger_order
        (program_version_id, trigger_order),
    UNIQUE KEY uk_welfare_reassessment_trigger_source
        (program_version_id, source_kind),
    CONSTRAINT fk_welfare_reassessment_trigger_program
        FOREIGN KEY (program_version_id) REFERENCES welfare_program_version (id),
    CONSTRAINT ck_welfare_reassessment_trigger_order CHECK (trigger_order BETWEEN 1 AND 32),
    CONSTRAINT ck_welfare_reassessment_trigger_source CHECK (
        source_kind IN (
            'gameDay', 'household', 'residence', 'employment',
            'military', 'income', 'asset', 'debt'
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

-- Runtime rows carry both the run's life catalog and welfare component pin. This prevents a
-- program from a later newRun assignment being evaluated inside an older run.
CREATE TABLE welfare_period_pin (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    life_catalog_set_id             BIGINT UNSIGNED NOT NULL,
    welfare_component_version_id    BIGINT UNSIGNED NOT NULL,
    program_version_id              BIGINT UNSIGNED NOT NULL,
    evaluation_game_day             INT UNSIGNED NOT NULL,
    authority_state_revision        BIGINT UNSIGNED NOT NULL,
    previous_closed_start_game_day  INT UNSIGNED NOT NULL,
    previous_closed_end_game_day    INT UNSIGNED NOT NULL,
    prior_close_state_revision      BIGINT UNSIGNED NOT NULL,
    fact_count                      TINYINT UNSIGNED NOT NULL,
    canonical_input_json            LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    fact_fingerprint                CHAR(64) CHARACTER SET ascii COLLATE ascii_bin
        GENERATED ALWAYS AS (SHA2(canonical_input_json, 256)) STORED,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_welfare_period_pin_authority
        (save_id, run_revision, program_version_id, evaluation_game_day,
         authority_state_revision),
    UNIQUE KEY uk_welfare_period_pin_fingerprint
        (save_id, run_revision, program_version_id, fact_fingerprint),
    UNIQUE KEY uk_welfare_period_pin_save_run_id
        (save_id, run_revision, id),
    UNIQUE KEY uk_welfare_period_pin_program_id
        (save_id, run_revision, program_version_id, id),
    UNIQUE KEY uk_welfare_period_pin_id_fingerprint
        (save_id, run_revision, id, fact_fingerprint),
    KEY ix_welfare_period_pin_catalog
        (life_catalog_set_id, welfare_component_version_id),
    CONSTRAINT fk_welfare_period_pin_rule_bundle
        FOREIGN KEY (save_id, run_revision, life_catalog_set_id)
        REFERENCES run_rule_bundle (save_id, run_revision, life_catalog_set_id),
    CONSTRAINT fk_welfare_period_pin_catalog_component
        FOREIGN KEY (life_catalog_set_id, welfare_component_version_id)
        REFERENCES life_catalog_set (id, welfare_component_version_id),
    CONSTRAINT fk_welfare_period_pin_program
        FOREIGN KEY (welfare_component_version_id, program_version_id)
        REFERENCES welfare_program_version (life_component_version_id, id),
    CONSTRAINT ck_welfare_period_pin_period CHECK (
        previous_closed_start_game_day <= previous_closed_end_game_day
        AND previous_closed_end_game_day = evaluation_game_day
        AND prior_close_state_revision <= authority_state_revision
    ),
    CONSTRAINT ck_welfare_period_pin_facts CHECK (
        fact_count <= 32
        AND JSON_VALID(canonical_input_json)
        AND JSON_EXTRACT(canonical_input_json, '$.facts') IS NOT NULL
        AND JSON_TYPE(JSON_EXTRACT(canonical_input_json, '$.facts')) = 'ARRAY'
        AND JSON_LENGTH(JSON_EXTRACT(canonical_input_json, '$.facts')) = fact_count
        AND JSON_EXTRACT(canonical_input_json, '$.schemaVersion') IS NOT NULL
        AND JSON_TYPE(JSON_EXTRACT(canonical_input_json, '$.schemaVersion')) = 'INTEGER'
        AND JSON_UNQUOTE(JSON_EXTRACT(canonical_input_json, '$.schemaVersion')) = '1'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE welfare_evaluation (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    program_version_id              BIGINT UNSIGNED NOT NULL,
    period_pin_id                   BIGINT UNSIGNED NOT NULL,
    fact_fingerprint                CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    evaluation_game_day             INT UNSIGNED NOT NULL,
    authority_state_revision        BIGINT UNSIGNED NOT NULL,
    status                          VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    condition_count                 TINYINT UNSIGNED NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_welfare_evaluation_fingerprint
        (save_id, run_revision, program_version_id, fact_fingerprint),
    UNIQUE KEY uk_welfare_evaluation_save_run_id
        (save_id, run_revision, id),
    UNIQUE KEY uk_welfare_evaluation_program_id
        (save_id, run_revision, program_version_id, id),
    KEY ix_welfare_evaluation_latest
        (save_id, run_revision, program_version_id, evaluation_game_day,
         authority_state_revision, id),
    CONSTRAINT fk_welfare_evaluation_period_pin
        FOREIGN KEY (save_id, run_revision, program_version_id, period_pin_id)
        REFERENCES welfare_period_pin (save_id, run_revision, program_version_id, id),
    CONSTRAINT fk_welfare_evaluation_pin_fingerprint
        FOREIGN KEY (save_id, run_revision, period_pin_id, fact_fingerprint)
        REFERENCES welfare_period_pin (save_id, run_revision, id, fact_fingerprint),
    CONSTRAINT ck_welfare_evaluation_fingerprint CHECK (
        fact_fingerprint REGEXP '^[0-9a-f]{64}$'
    ),
    CONSTRAINT ck_welfare_evaluation_status CHECK (
        status IN ('eligible', 'ineligible', 'indeterminate')
    ),
    CONSTRAINT ck_welfare_evaluation_condition_count CHECK (
        condition_count BETWEEN 1 AND 32
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE welfare_evaluation_condition (
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    evaluation_id                   BIGINT UNSIGNED NOT NULL,
    program_version_id              BIGINT UNSIGNED NOT NULL,
    program_condition_id            BIGINT UNSIGNED NOT NULL,
    condition_order                 TINYINT UNSIGNED NOT NULL,
    outcome                         VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    unknown_reason                  VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (evaluation_id, program_condition_id),
    UNIQUE KEY uk_welfare_evaluation_condition_order
        (evaluation_id, condition_order),
    KEY ix_welfare_evaluation_condition_run
        (save_id, run_revision, evaluation_id),
    CONSTRAINT fk_welfare_evaluation_condition_evaluation
        FOREIGN KEY (save_id, run_revision, program_version_id, evaluation_id)
        REFERENCES welfare_evaluation (save_id, run_revision, program_version_id, id),
    CONSTRAINT fk_welfare_evaluation_condition_catalog
        FOREIGN KEY (program_version_id, program_condition_id)
        REFERENCES welfare_program_condition (program_version_id, id),
    CONSTRAINT ck_welfare_evaluation_condition_order CHECK (
        condition_order BETWEEN 1 AND 32
    ),
    CONSTRAINT ck_welfare_evaluation_condition_outcome CHECK (
        (outcome IN ('passed', 'failed') AND unknown_reason IS NULL)
        OR (
            outcome = 'unknown'
            AND unknown_reason IS NOT NULL
            AND unknown_reason IN (
                'authorityMissing', 'valuationUnavailable',
                'collectionLimitExceeded', 'windowIncomplete', 'arithmeticOverflow'
            )
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE welfare_application (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    life_catalog_set_id             BIGINT UNSIGNED NOT NULL,
    welfare_component_version_id    BIGINT UNSIGNED NOT NULL,
    program_version_id              BIGINT UNSIGNED NOT NULL,
    eligibility_evaluation_id       BIGINT UNSIGNED NOT NULL,
    eligibility_fact_fingerprint    CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    eligibility_basis               VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    command_id                      CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    duplicate_group_key             VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    duplicate_group_claim_key       VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    benefit_amount_krw              BIGINT NOT NULL,
    payment_delay_game_days         SMALLINT UNSIGNED NOT NULL,
    status                          VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    application_game_day            INT UNSIGNED NOT NULL,
    approval_game_day               INT UNSIGNED NULL,
    paid_krw                        BIGINT NOT NULL DEFAULT 0,
    terminal_game_day               INT UNSIGNED NULL,
    terminal_reason                 VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_welfare_application_save_run_id
        (save_id, run_revision, id),
    UNIQUE KEY uk_welfare_application_program_id
        (save_id, run_revision, program_version_id, id),
    UNIQUE KEY uk_welfare_application_command
        (save_id, run_revision, command_id),
    UNIQUE KEY uk_welfare_application_duplicate_claim
        (save_id, run_revision, duplicate_group_claim_key),
    KEY ix_welfare_application_active
        (save_id, run_revision, status, id),
    CONSTRAINT fk_welfare_application_rule_bundle
        FOREIGN KEY (save_id, run_revision, life_catalog_set_id)
        REFERENCES run_rule_bundle (save_id, run_revision, life_catalog_set_id),
    CONSTRAINT fk_welfare_application_catalog_component
        FOREIGN KEY (life_catalog_set_id, welfare_component_version_id)
        REFERENCES life_catalog_set (id, welfare_component_version_id),
    CONSTRAINT fk_welfare_application_program
        FOREIGN KEY (welfare_component_version_id, program_version_id)
        REFERENCES welfare_program_version (life_component_version_id, id),
    CONSTRAINT fk_welfare_application_evaluation
        FOREIGN KEY (
            save_id, run_revision, program_version_id, eligibility_evaluation_id
        ) REFERENCES welfare_evaluation (save_id, run_revision, program_version_id, id),
    CONSTRAINT fk_welfare_application_command
        FOREIGN KEY (save_id, command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT ck_welfare_application_fingerprint CHECK (
        eligibility_fact_fingerprint REGEXP '^[0-9a-f]{64}$'
        AND eligibility_basis = 'eligibilityAtApplication'
    ),
    CONSTRAINT ck_welfare_application_duplicate CHECK (
        duplicate_group_key REGEXP '^[a-z][a-zA-Z0-9]{0,63}$'
        AND (
            duplicate_group_claim_key IS NULL
            OR BINARY duplicate_group_claim_key = BINARY duplicate_group_key
        )
    ),
    CONSTRAINT ck_welfare_application_amount CHECK (
        benefit_amount_krw BETWEEN 1 AND 9007199254740991
        AND payment_delay_game_days BETWEEN 1 AND 366
        AND paid_krw BETWEEN 0 AND benefit_amount_krw
    ),
    CONSTRAINT ck_welfare_application_state CHECK (
        (
            status = 'applied'
            AND approval_game_day IS NULL
            AND duplicate_group_claim_key IS NULL
            AND paid_krw = 0
            AND terminal_game_day IS NULL
            AND terminal_reason IS NULL
        )
        OR (
            status IN ('approved', 'active')
            AND approval_game_day IS NOT NULL
            AND approval_game_day = application_game_day
            AND duplicate_group_claim_key IS NOT NULL
            AND BINARY duplicate_group_claim_key = BINARY duplicate_group_key
            AND paid_krw = 0
            AND terminal_game_day IS NULL
            AND terminal_reason IS NULL
        )
        OR (
            status = 'rejected'
            AND approval_game_day IS NULL
            AND duplicate_group_claim_key IS NULL
            AND paid_krw = 0
            AND terminal_game_day IS NOT NULL
            AND terminal_reason IS NOT NULL
            AND terminal_reason IN ('ineligible', 'valuationUnavailable')
        )
        OR (
            status = 'exhausted'
            AND approval_game_day IS NOT NULL
            AND approval_game_day = application_game_day
            AND duplicate_group_claim_key IS NOT NULL
            AND BINARY duplicate_group_claim_key = BINARY duplicate_group_key
            AND paid_krw = benefit_amount_krw
            AND terminal_game_day IS NOT NULL
            AND terminal_reason IS NOT NULL
            AND terminal_reason = 'benefitPaid'
        )
        OR (
            status = 'terminated'
            AND approval_game_day IS NOT NULL
            AND approval_game_day = application_game_day
            AND duplicate_group_claim_key IS NOT NULL
            AND BINARY duplicate_group_claim_key = BINARY duplicate_group_key
            AND paid_krw = 0
            AND terminal_game_day IS NOT NULL
            AND terminal_reason IS NOT NULL
            AND terminal_reason = 'newRun'
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE welfare_application_transition (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    application_id                  BIGINT UNSIGNED NOT NULL,
    transition_no                   TINYINT UNSIGNED NOT NULL,
    from_status                     VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL,
    to_status                       VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    transition_game_day             INT UNSIGNED NOT NULL,
    transition_reason               VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_welfare_application_transition_no
        (save_id, run_revision, application_id, transition_no),
    UNIQUE KEY uk_welfare_application_transition_status
        (save_id, run_revision, application_id, to_status),
    CONSTRAINT fk_welfare_application_transition_application
        FOREIGN KEY (save_id, run_revision, application_id)
        REFERENCES welfare_application (save_id, run_revision, id),
    CONSTRAINT ck_welfare_application_transition_no CHECK (
        transition_no BETWEEN 1 AND 5
    ),
    CONSTRAINT ck_welfare_application_transition_status CHECK (
        (
            transition_no = 1
            AND from_status IS NULL
            AND to_status = 'applied'
            AND transition_reason = 'playerApplication'
        )
        OR (
            transition_no = 2
            AND from_status IS NOT NULL
            AND from_status = 'applied'
            AND (
                (to_status = 'approved' AND transition_reason = 'eligibilityApproved')
                OR (
                    to_status = 'rejected'
                    AND transition_reason IN ('ineligible', 'valuationUnavailable')
                )
            )
        )
        OR (
            transition_no = 3
            AND from_status IS NOT NULL
            AND from_status = 'approved'
            AND to_status = 'active'
            AND transition_reason = 'paymentScheduled'
        )
        OR (
            transition_no = 4
            AND from_status IS NOT NULL
            AND from_status = 'active'
            AND (
                (to_status = 'exhausted' AND transition_reason = 'benefitPaid')
                OR (to_status = 'terminated' AND transition_reason = 'newRun')
            )
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE welfare_payment (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    application_id                  BIGINT UNSIGNED NOT NULL,
    payment_no                      TINYINT UNSIGNED NOT NULL,
    due_game_day                    INT UNSIGNED NOT NULL,
    amount_krw                      BIGINT NOT NULL,
    status                          VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    scheduled_settlement_id         BIGINT UNSIGNED NULL,
    ledger_transaction_id           BIGINT UNSIGNED NULL,
    paid_game_day                   INT UNSIGNED NULL,
    cancelled_game_day              INT UNSIGNED NULL,
    cancellation_reason             VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_welfare_payment_save_run_id
        (save_id, run_revision, id),
    UNIQUE KEY uk_welfare_payment_application_no
        (save_id, run_revision, application_id, payment_no),
    UNIQUE KEY uk_welfare_payment_settlement
        (save_id, run_revision, scheduled_settlement_id),
    UNIQUE KEY uk_welfare_payment_ledger
        (save_id, run_revision, ledger_transaction_id),
    KEY ix_welfare_payment_due
        (save_id, run_revision, status, due_game_day, id),
    CONSTRAINT fk_welfare_payment_application
        FOREIGN KEY (save_id, run_revision, application_id)
        REFERENCES welfare_application (save_id, run_revision, id),
    CONSTRAINT fk_welfare_payment_settlement
        FOREIGN KEY (save_id, run_revision, scheduled_settlement_id)
        REFERENCES scheduled_settlement (save_id, run_revision, id),
    CONSTRAINT fk_welfare_payment_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT ck_welfare_payment_amount CHECK (
        payment_no = 1 AND amount_krw BETWEEN 1 AND 9007199254740991
    ),
    CONSTRAINT ck_welfare_payment_state CHECK (
        (
            status = 'pending'
            AND ledger_transaction_id IS NULL
            AND paid_game_day IS NULL
            AND cancelled_game_day IS NULL
            AND cancellation_reason IS NULL
        )
        OR (
            status = 'paid'
            AND scheduled_settlement_id IS NOT NULL
            AND ledger_transaction_id IS NOT NULL
            AND paid_game_day IS NOT NULL
            AND paid_game_day = due_game_day
            AND cancelled_game_day IS NULL
            AND cancellation_reason IS NULL
        )
        OR (
            status = 'cancelled'
            AND scheduled_settlement_id IS NOT NULL
            AND ledger_transaction_id IS NULL
            AND paid_game_day IS NULL
            AND cancelled_game_day IS NOT NULL
            AND cancellation_reason IS NOT NULL
            AND cancellation_reason = 'newRun'
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

-- Catalog children may be populated only while their welfare component is an unmanifested draft.
CREATE TRIGGER tr_welfare_fact_definition_draft_insert
BEFORE INSERT ON welfare_fact_definition
FOR EACH ROW
SET NEW.life_component_version_id = IF(
    EXISTS (
        SELECT 1
        FROM life_component_version AS component
        WHERE component.id = NEW.life_component_version_id
          AND component.component_kind = 'welfare'
          AND component.availability = 'active'
          AND component.sealed_at IS NULL
          AND component.canonical_sha256 IS NULL
          AND NOT EXISTS (
              SELECT 1 FROM life_component_canonical_manifest AS manifest
              WHERE manifest.life_component_version_id = component.id
          )
    ),
    NEW.life_component_version_id,
    NULL
);

CREATE TRIGGER tr_welfare_fact_definition_no_update
BEFORE UPDATE ON welfare_fact_definition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'welfare fact definitions are immutable';

CREATE TRIGGER tr_welfare_fact_definition_no_delete
BEFORE DELETE ON welfare_fact_definition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'welfare fact definitions are immutable';

CREATE TRIGGER tr_welfare_program_version_draft_insert
BEFORE INSERT ON welfare_program_version
FOR EACH ROW
SET NEW.life_component_version_id = IF(
    EXISTS (
        SELECT 1
        FROM life_component_version AS component
        WHERE component.id = NEW.life_component_version_id
          AND component.component_kind = 'welfare'
          AND component.availability = 'active'
          AND component.sealed_at IS NULL
          AND component.canonical_sha256 IS NULL
          AND (
              NEW.ranked_availability = 'unrankedOnly'
              OR component.ranked_eligible = TRUE
          )
          AND NOT EXISTS (
              SELECT 1 FROM life_component_canonical_manifest AS manifest
              WHERE manifest.life_component_version_id = component.id
          )
    )
        AND (
            SELECT COUNT(*) FROM welfare_program_version AS program
            WHERE program.life_component_version_id = NEW.life_component_version_id
        ) < 16,
    NEW.life_component_version_id,
    NULL
);

CREATE TRIGGER tr_welfare_program_version_no_update
BEFORE UPDATE ON welfare_program_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'welfare program versions are immutable';

CREATE TRIGGER tr_welfare_program_version_no_delete
BEFORE DELETE ON welfare_program_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'welfare program versions are immutable';

CREATE TRIGGER tr_welfare_program_constant_draft_insert
BEFORE INSERT ON welfare_program_constant
FOR EACH ROW
SET NEW.program_version_id = IF(
    EXISTS (
        SELECT 1
        FROM welfare_program_version AS program
        INNER JOIN life_component_version AS component
            ON component.id = program.life_component_version_id
        WHERE program.id = NEW.program_version_id
          AND component.component_kind = 'welfare'
          AND component.availability = 'active'
          AND component.sealed_at IS NULL
          AND component.canonical_sha256 IS NULL
          AND NOT EXISTS (
              SELECT 1 FROM life_component_canonical_manifest AS manifest
              WHERE manifest.life_component_version_id = component.id
          )
    )
        AND (
            SELECT COUNT(*) FROM welfare_program_constant AS constant
            WHERE constant.program_version_id = NEW.program_version_id
        ) < 64,
    NEW.program_version_id,
    NULL
);

CREATE TRIGGER tr_welfare_program_constant_no_update
BEFORE UPDATE ON welfare_program_constant
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'welfare program constants are immutable';

CREATE TRIGGER tr_welfare_program_constant_no_delete
BEFORE DELETE ON welfare_program_constant
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'welfare program constants are immutable';

CREATE TRIGGER tr_welfare_program_condition_draft_insert
BEFORE INSERT ON welfare_program_condition
FOR EACH ROW
SET NEW.program_version_id = IF(
    EXISTS (
        SELECT 1
        FROM welfare_program_version AS program
        INNER JOIN life_component_version AS component
            ON component.id = program.life_component_version_id
        WHERE program.id = NEW.program_version_id
          AND component.component_kind = 'welfare'
          AND component.availability = 'active'
          AND component.sealed_at IS NULL
          AND component.canonical_sha256 IS NULL
          AND NOT EXISTS (
              SELECT 1 FROM life_component_canonical_manifest AS manifest
              WHERE manifest.life_component_version_id = component.id
          )
    )
        AND (
            SELECT COUNT(*) FROM welfare_program_condition AS condition_row
            WHERE condition_row.program_version_id = NEW.program_version_id
        ) < 32,
    NEW.program_version_id,
    NULL
);

CREATE TRIGGER tr_welfare_program_condition_no_update
BEFORE UPDATE ON welfare_program_condition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'welfare program conditions are immutable';

CREATE TRIGGER tr_welfare_program_condition_no_delete
BEFORE DELETE ON welfare_program_condition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'welfare program conditions are immutable';

CREATE TRIGGER tr_welfare_reassessment_trigger_draft_insert
BEFORE INSERT ON welfare_reassessment_trigger
FOR EACH ROW
SET NEW.program_version_id = IF(
    EXISTS (
        SELECT 1
        FROM welfare_program_version AS program
        INNER JOIN life_component_version AS component
            ON component.id = program.life_component_version_id
        WHERE program.id = NEW.program_version_id
          AND component.component_kind = 'welfare'
          AND component.availability = 'active'
          AND component.sealed_at IS NULL
          AND component.canonical_sha256 IS NULL
          AND NOT EXISTS (
              SELECT 1 FROM life_component_canonical_manifest AS manifest
              WHERE manifest.life_component_version_id = component.id
          )
    )
        AND (
            SELECT COUNT(*) FROM welfare_reassessment_trigger AS trigger_row
            WHERE trigger_row.program_version_id = NEW.program_version_id
        ) < 32,
    NEW.program_version_id,
    NULL
);

CREATE TRIGGER tr_welfare_reassessment_trigger_no_update
BEFORE UPDATE ON welfare_reassessment_trigger
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'welfare reassessment triggers are immutable';

CREATE TRIGGER tr_welfare_reassessment_trigger_no_delete
BEFORE DELETE ON welfare_reassessment_trigger
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'welfare reassessment triggers are immutable';

-- Daily policy pins are written before settlements while save still points at prior close.
CREATE TRIGGER tr_welfare_period_pin_valid_insert
BEFORE INSERT ON welfare_period_pin
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM save
        INNER JOIN run_rule_bundle AS bundle
            ON bundle.save_id = save.id
           AND bundle.run_revision = save.run_revision
        INNER JOIN life_catalog_set AS catalog
            ON catalog.id = bundle.life_catalog_set_id
           AND catalog.welfare_component_version_id
                = NEW.welfare_component_version_id
        INNER JOIN life_component_version AS component
            ON component.id = catalog.welfare_component_version_id
           AND component.component_kind = 'welfare'
           AND component.availability = 'active'
           AND component.sealed_at IS NOT NULL
        INNER JOIN welfare_program_version AS program
            ON program.id = NEW.program_version_id
           AND program.life_component_version_id = component.id
        WHERE save.id = NEW.save_id
          AND save.run_revision = NEW.run_revision
          AND (
              save.game_day = NEW.evaluation_game_day
              OR save.game_day + 1 = NEW.evaluation_game_day
          )
          AND save.state_revision = NEW.authority_state_revision
          AND bundle.life_catalog_set_id = NEW.life_catalog_set_id
          AND JSON_UNQUOTE(
                  JSON_EXTRACT(NEW.canonical_input_json, '$.programVersionId')
              ) = CAST(NEW.program_version_id AS CHAR)
          AND CAST(JSON_UNQUOTE(
                  JSON_EXTRACT(NEW.canonical_input_json, '$.period.evaluationGameDay')
              ) AS UNSIGNED) = NEW.evaluation_game_day
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_welfare_period_pin_no_update
BEFORE UPDATE ON welfare_period_pin
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'welfare period pins are immutable';

CREATE TRIGGER tr_welfare_period_pin_no_delete
BEFORE DELETE ON welfare_period_pin
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'welfare period pins are immutable';

CREATE TRIGGER tr_welfare_evaluation_valid_insert
BEFORE INSERT ON welfare_evaluation
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM welfare_period_pin AS pin
        WHERE pin.id = NEW.period_pin_id
          AND pin.save_id = NEW.save_id
          AND pin.run_revision = NEW.run_revision
          AND pin.program_version_id = NEW.program_version_id
          AND BINARY pin.fact_fingerprint = BINARY NEW.fact_fingerprint
          AND pin.evaluation_game_day = NEW.evaluation_game_day
          AND pin.authority_state_revision = NEW.authority_state_revision
          AND NEW.condition_count = (
              SELECT COUNT(*)
              FROM welfare_program_condition AS condition_row
              WHERE condition_row.program_version_id = NEW.program_version_id
          )
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_welfare_evaluation_no_update
BEFORE UPDATE ON welfare_evaluation
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'welfare evaluations are immutable';

CREATE TRIGGER tr_welfare_evaluation_no_delete
BEFORE DELETE ON welfare_evaluation
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'welfare evaluations are immutable';

CREATE TRIGGER tr_welfare_evaluation_condition_valid_insert
BEFORE INSERT ON welfare_evaluation_condition
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM welfare_evaluation AS evaluation
        INNER JOIN welfare_program_condition AS condition_row
            ON condition_row.id = NEW.program_condition_id
           AND condition_row.program_version_id = evaluation.program_version_id
        WHERE evaluation.id = NEW.evaluation_id
          AND evaluation.save_id = NEW.save_id
          AND evaluation.run_revision = NEW.run_revision
          AND evaluation.program_version_id = NEW.program_version_id
          AND condition_row.condition_order = NEW.condition_order
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_welfare_evaluation_condition_no_update
BEFORE UPDATE ON welfare_evaluation_condition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'welfare evaluation evidence is immutable';

CREATE TRIGGER tr_welfare_evaluation_condition_no_delete
BEFORE DELETE ON welfare_evaluation_condition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'welfare evaluation evidence is immutable';

CREATE TRIGGER tr_welfare_application_valid_insert
BEFORE INSERT ON welfare_application
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'applied'
        AND EXISTS (
            SELECT 1
            FROM save
            INNER JOIN command_identity AS identity
                ON identity.save_id = save.id
               AND BINARY identity.command_id = BINARY NEW.command_id
               AND identity.command_kind = 'applyWelfareProgram'
            INNER JOIN welfare_evaluation AS evaluation
                ON evaluation.id = NEW.eligibility_evaluation_id
               AND evaluation.save_id = save.id
               AND evaluation.run_revision = NEW.run_revision
               AND evaluation.program_version_id = NEW.program_version_id
            INNER JOIN welfare_program_version AS program
                ON program.id = evaluation.program_version_id
               AND program.life_component_version_id
                    = NEW.welfare_component_version_id
            INNER JOIN welfare_program_constant AS benefit
                ON benefit.program_version_id = program.id
               AND BINARY benefit.constant_key = BINARY JSON_UNQUOTE(
                    JSON_EXTRACT(program.benefit_formula, '$.amount.key')
               )
            WHERE save.id = NEW.save_id
              AND save.run_revision = NEW.run_revision
              AND save.game_day = NEW.application_game_day
              AND identity.initial_run_revision = NEW.run_revision
              AND identity.initial_state_revision = save.state_revision
              AND identity.initial_game_day = NEW.application_game_day
              AND evaluation.status = 'eligible'
              AND evaluation.evaluation_game_day = save.game_day
              AND BINARY evaluation.fact_fingerprint
                    = BINARY NEW.eligibility_fact_fingerprint
              AND evaluation.condition_count = (
                  SELECT COUNT(*)
                  FROM welfare_evaluation_condition AS evidence
                  WHERE evidence.evaluation_id = evaluation.id
              )
              AND BINARY program.duplicate_group_key
                    = BINARY NEW.duplicate_group_key
              AND benefit.value_type = 'moneyKrw'
              AND benefit.unit = 'krw'
              AND benefit.integer_value = NEW.benefit_amount_krw
              AND CAST(JSON_UNQUOTE(
                    JSON_EXTRACT(program.payment_schedule, '$.delayGameDays')
                  ) AS UNSIGNED) = NEW.payment_delay_game_days
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_welfare_application_transition_only
BEFORE UPDATE ON welfare_application
FOR EACH ROW
SET NEW.status = IF(
    NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.life_catalog_set_id = OLD.life_catalog_set_id
        AND NEW.welfare_component_version_id = OLD.welfare_component_version_id
        AND NEW.program_version_id = OLD.program_version_id
        AND NEW.eligibility_evaluation_id = OLD.eligibility_evaluation_id
        AND BINARY NEW.eligibility_fact_fingerprint
              = BINARY OLD.eligibility_fact_fingerprint
        AND BINARY NEW.eligibility_basis = BINARY OLD.eligibility_basis
        AND BINARY NEW.command_id = BINARY OLD.command_id
        AND BINARY NEW.duplicate_group_key = BINARY OLD.duplicate_group_key
        AND NEW.benefit_amount_krw = OLD.benefit_amount_krw
        AND NEW.payment_delay_game_days = OLD.payment_delay_game_days
        AND NEW.application_game_day = OLD.application_game_day
        AND NEW.created_at = OLD.created_at
        AND (
            (
                OLD.status = 'applied'
                AND NEW.status = 'approved'
                AND NEW.approval_game_day = OLD.application_game_day
                AND BINARY NEW.duplicate_group_claim_key
                      = BINARY OLD.duplicate_group_key
                AND NEW.paid_krw = 0
                AND NEW.terminal_game_day IS NULL
                AND NEW.terminal_reason IS NULL
            )
            OR (
                OLD.status = 'approved'
                AND NEW.status = 'active'
                AND NEW.approval_game_day = OLD.approval_game_day
                AND BINARY NEW.duplicate_group_claim_key
                      = BINARY OLD.duplicate_group_claim_key
                AND NEW.paid_krw = 0
                AND NEW.terminal_game_day IS NULL
                AND NEW.terminal_reason IS NULL
                AND (
                    SELECT COUNT(*)
                    FROM welfare_application AS active_application
                    WHERE active_application.save_id = OLD.save_id
                      AND active_application.run_revision = OLD.run_revision
                      AND active_application.status = 'active'
                ) < 8
            )
            OR (
                OLD.status = 'applied'
                AND NEW.status = 'rejected'
                AND NEW.approval_game_day IS NULL
                AND NEW.duplicate_group_claim_key IS NULL
                AND NEW.paid_krw = 0
                AND NEW.terminal_game_day = OLD.application_game_day
                AND NEW.terminal_reason IN ('ineligible', 'valuationUnavailable')
            )
            OR (
                OLD.status = 'active'
                AND NEW.status = 'exhausted'
                AND NEW.approval_game_day = OLD.approval_game_day
                AND BINARY NEW.duplicate_group_claim_key
                      = BINARY OLD.duplicate_group_claim_key
                AND NEW.paid_krw = OLD.benefit_amount_krw
                AND NEW.terminal_reason = 'benefitPaid'
                AND EXISTS (
                    SELECT 1
                    FROM welfare_payment AS payment
                    WHERE payment.save_id = OLD.save_id
                      AND payment.run_revision = OLD.run_revision
                      AND payment.application_id = OLD.id
                      AND payment.payment_no = 1
                      AND payment.amount_krw = OLD.benefit_amount_krw
                      AND payment.status = 'paid'
                      AND payment.paid_game_day = NEW.terminal_game_day
                )
            )
            OR (
                OLD.status = 'active'
                AND NEW.status = 'terminated'
                AND NEW.approval_game_day = OLD.approval_game_day
                AND BINARY NEW.duplicate_group_claim_key
                      = BINARY OLD.duplicate_group_claim_key
                AND NEW.paid_krw = 0
                AND NEW.terminal_reason = 'newRun'
                AND EXISTS (
                    SELECT 1
                    FROM welfare_payment AS payment
                    WHERE payment.save_id = OLD.save_id
                      AND payment.run_revision = OLD.run_revision
                      AND payment.application_id = OLD.id
                      AND payment.status = 'cancelled'
                      AND payment.cancelled_game_day = NEW.terminal_game_day
                )
            )
        ),
    NEW.status,
    NULL
);

CREATE TRIGGER tr_welfare_application_no_delete
BEFORE DELETE ON welfare_application
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'welfare applications are immutable history';

CREATE TRIGGER tr_welfare_application_transition_valid_insert
BEFORE INSERT ON welfare_application_transition
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM welfare_application AS application
        WHERE application.id = NEW.application_id
          AND application.save_id = NEW.save_id
          AND application.run_revision = NEW.run_revision
          AND application.status = NEW.to_status
          AND (
              (
                  NEW.transition_no = 1
                  AND NOT EXISTS (
                      SELECT 1 FROM welfare_application_transition AS previous
                      WHERE previous.save_id = NEW.save_id
                        AND previous.run_revision = NEW.run_revision
                        AND previous.application_id = NEW.application_id
                  )
              )
              OR (
                  NEW.transition_no > 1
                  AND EXISTS (
                      SELECT 1 FROM welfare_application_transition AS previous
                      WHERE previous.save_id = NEW.save_id
                        AND previous.run_revision = NEW.run_revision
                        AND previous.application_id = NEW.application_id
                        AND previous.transition_no = NEW.transition_no - 1
                        AND previous.to_status = NEW.from_status
                  )
              )
          )
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_welfare_application_transition_no_update
BEFORE UPDATE ON welfare_application_transition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'welfare application transitions are immutable';

CREATE TRIGGER tr_welfare_application_transition_no_delete
BEFORE DELETE ON welfare_application_transition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'welfare application transitions are immutable';

CREATE TRIGGER tr_welfare_payment_valid_insert
BEFORE INSERT ON welfare_payment
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'pending'
        AND NEW.scheduled_settlement_id IS NULL
        AND EXISTS (
            SELECT 1
            FROM welfare_application AS application
            WHERE application.id = NEW.application_id
              AND application.save_id = NEW.save_id
              AND application.run_revision = NEW.run_revision
              AND application.status = 'active'
              AND application.benefit_amount_krw = NEW.amount_krw
              AND NEW.due_game_day
                    = application.application_game_day
                      + application.payment_delay_game_days
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_welfare_payment_transition_only
BEFORE UPDATE ON welfare_payment
FOR EACH ROW
SET NEW.status = IF(
    NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.application_id = OLD.application_id
        AND NEW.payment_no = OLD.payment_no
        AND NEW.due_game_day = OLD.due_game_day
        AND NEW.amount_krw = OLD.amount_krw
        AND NEW.created_at = OLD.created_at
        AND (
            (
                OLD.status = 'pending'
                AND NEW.status = 'pending'
                AND OLD.scheduled_settlement_id IS NULL
                AND NEW.scheduled_settlement_id IS NOT NULL
                AND NEW.ledger_transaction_id IS NULL
                AND NEW.paid_game_day IS NULL
                AND NEW.cancelled_game_day IS NULL
                AND NEW.cancellation_reason IS NULL
                AND EXISTS (
                    SELECT 1
                    FROM scheduled_settlement AS settlement
                    WHERE settlement.id = NEW.scheduled_settlement_id
                      AND settlement.save_id = OLD.save_id
                      AND settlement.run_revision = OLD.run_revision
                      AND settlement.kind = 'welfareBenefitPayment'
                      AND settlement.source_kind = 'welfarePayment'
                      AND BINARY settlement.source_id = BINARY CAST(OLD.id AS CHAR)
                      AND settlement.occurrence = OLD.payment_no
                      AND settlement.due_game_day = OLD.due_game_day
                      AND settlement.status = 'pending'
                )
            )
            OR (
                OLD.status = 'pending'
                AND NEW.status = 'paid'
                AND NEW.scheduled_settlement_id = OLD.scheduled_settlement_id
                AND NEW.ledger_transaction_id IS NOT NULL
                AND NEW.paid_game_day = OLD.due_game_day
                AND NEW.cancelled_game_day IS NULL
                AND NEW.cancellation_reason IS NULL
                AND EXISTS (
                    SELECT 1
                    FROM ledger_transaction AS ledger
                    WHERE ledger.id = NEW.ledger_transaction_id
                      AND ledger.save_id = OLD.save_id
                      AND ledger.run_revision = OLD.run_revision
                      AND ledger.game_day = OLD.due_game_day
                      AND ledger.source_kind = 'welfareBenefitPayment'
                      AND BINARY ledger.source_id = BINARY CAST(OLD.id AS CHAR)
                      AND (
                          SELECT COUNT(*) FROM ledger_posting AS posting
                          WHERE posting.ledger_transaction_id = ledger.id
                      ) = 2
                      AND (
                          SELECT COALESCE(SUM(posting.amount_krw), 0)
                          FROM ledger_posting AS posting
                          WHERE posting.ledger_transaction_id = ledger.id
                      ) = 0
                      AND EXISTS (
                          SELECT 1 FROM ledger_posting AS posting
                          WHERE posting.ledger_transaction_id = ledger.id
                            AND posting.account_code = 'welfareBenefitIncome'
                            AND posting.amount_krw = -OLD.amount_krw
                      )
                      AND EXISTS (
                          SELECT 1 FROM ledger_posting AS posting
                          WHERE posting.ledger_transaction_id = ledger.id
                            AND posting.account_code = 'wallet'
                            AND posting.amount_krw = OLD.amount_krw
                      )
                )
            )
            OR (
                OLD.status = 'pending'
                AND NEW.status = 'cancelled'
                AND NEW.scheduled_settlement_id = OLD.scheduled_settlement_id
                AND NEW.ledger_transaction_id IS NULL
                AND NEW.paid_game_day IS NULL
                AND NEW.cancelled_game_day IS NOT NULL
                AND NEW.cancellation_reason = 'newRun'
            )
        ),
    NEW.status,
    NULL
);

CREATE TRIGGER tr_welfare_payment_no_delete
BEFORE DELETE ON welfare_payment
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'welfare payments are immutable history';

-- Extend the finance protocol without changing any pre-M4-D source or account meaning.
ALTER TABLE ledger_transaction
    ADD CONSTRAINT ck_ledger_transaction_welfare_source CHECK (
        source_kind NOT LIKE 'welfare%'
        OR source_kind = 'welfareBenefitPayment'
    );

ALTER TABLE ledger_posting
    DROP CHECK ck_ledger_posting_account_code,
    ADD CONSTRAINT ck_ledger_posting_account_code CHECK (
        account_code IN (
            'wallet', 'accountCash', 'productPrincipal', 'debtPrincipal',
            'openingEquity', 'withholdingTaxLiability', 'interestIncome',
            'feeExpense', 'distributionIncome', 'realizedGainLoss', 'taxSettlement',
            'careerDevelopmentExpense', 'salaryIncome',
            'employeeNationalPensionExpense', 'employeeHealthInsuranceExpense',
            'employeeLongTermCareExpense', 'employeeEmploymentInsuranceExpense',
            'employmentIncomeTaxWithholding', 'employmentLocalIncomeTaxWithholding',
            'otherIncomeReward', 'otherIncomeTaxWithholding',
            'otherLocalIncomeTaxWithholding', 'pensionTaxExcludedContribution',
            'pensionCreditedContribution', 'militaryPayIncome',
            'militarySavingsPrincipal', 'militarySavingsBankInterest',
            'militarySavingsGovernmentMatchIncome',
            'livingCostExpense', 'essentialArrearLiability',
            'loanPrincipalLiability', 'loanInterestExpense', 'loanInterestLiability',
            'loanFeeExpense', 'taxObligationLiability',
            'leaseDepositAsset', 'movingExpense',
            'leaseRentExpense', 'leaseArrearLiability',
            'propertyAsset', 'acquisitionIncidentalExpense',
            'propertyDispositionExpense', 'propertyTaxExpense',
            'welfareBenefitIncome'
        )
    );

CREATE TRIGGER tr_ledger_transaction_welfare_source_insert
BEFORE INSERT ON ledger_transaction
FOR EACH ROW
FOLLOWS tr_ledger_transaction_property_source_insert
SET NEW.source_kind = IF(
    NEW.source_kind <> 'welfareBenefitPayment'
        OR (
            NEW.source_id REGEXP '^[1-9][0-9]{0,19}$'
            AND EXISTS (
                SELECT 1
                FROM welfare_payment AS payment
                INNER JOIN welfare_application AS application
                    ON application.id = payment.application_id
                   AND application.save_id = payment.save_id
                   AND application.run_revision = payment.run_revision
                WHERE BINARY CAST(payment.id AS CHAR) = BINARY NEW.source_id
                  AND payment.save_id = NEW.save_id
                  AND payment.run_revision = NEW.run_revision
                  AND payment.status = 'pending'
                  AND payment.due_game_day = NEW.game_day
                  AND payment.scheduled_settlement_id IS NOT NULL
                  AND application.status = 'active'
            )
        ),
    NEW.source_kind,
    NULL
);

CREATE TRIGGER tr_ledger_posting_welfare_reference_insert
BEFORE INSERT ON ledger_posting
FOR EACH ROW
FOLLOWS tr_ledger_posting_lease_rent_reference_insert
SET NEW.account_code = IF(
    (
        EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            INNER JOIN welfare_payment AS payment
                ON BINARY CAST(payment.id AS CHAR) = BINARY ledger.source_id
               AND payment.save_id = ledger.save_id
               AND payment.run_revision = ledger.run_revision
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'welfareBenefitPayment'
              AND payment.status = 'pending'
              AND (
                  (
                      NEW.account_code = 'welfareBenefitIncome'
                      AND NEW.amount_krw = -payment.amount_krw
                  )
                  OR (
                      NEW.account_code = 'wallet'
                      AND NEW.amount_krw = payment.amount_krw
                  )
              )
        )
    )
    OR (
        NEW.account_code <> 'welfareBenefitIncome'
        AND NOT EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'welfareBenefitPayment'
        )
    ),
    NEW.account_code,
    NULL
);

ALTER TABLE scheduled_settlement
    DROP CHECK ck_scheduled_settlement_kind,
    DROP CHECK ck_scheduled_settlement_source_kind,
    ADD CONSTRAINT ck_scheduled_settlement_kind CHECK (
        kind IN (
            'cmaInterest', 'depositMaturity', 'savingsInstallment',
            'savingsMaturity', 'bondCoupon', 'bondMaturity',
            'llxDistribution', 'financialIncomeFiling',
            'employmentPayroll', 'employmentReconciliation',
            'militaryPay', 'militarySavingsInstallment',
            'militarySavingsMaturity', 'militarySavingsGovernmentMatch',
            'livingCostMonth', 'loanInstallment', 'leaseRent',
            'propertyTaxPayment', 'welfareBenefitPayment'
        )
    ),
    ADD CONSTRAINT ck_scheduled_settlement_source_kind CHECK (
        source_kind IN (
            'cmaAccount', 'depositContract', 'savingsContract',
            'bondPosition', 'indexPosition', 'taxYear',
            'employmentContract', 'yearEndTaxAssessment',
            'militaryService', 'militarySavingsContract',
            'militarySavingsInstallment', 'livingCostMonth',
            'loanContract', 'leaseContract', 'propertyTaxEvent',
            'welfarePayment'
        )
    ),
    ADD CONSTRAINT ck_scheduled_settlement_welfare_payload CHECK (
        kind <> 'welfareBenefitPayment'
        OR (
            JSON_TYPE(payload) = 'OBJECT'
            AND JSON_LENGTH(payload) = 4
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.version')) = 'INTEGER'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.version')) = '1'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.welfarePaymentId')) = 'STRING'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.welfarePaymentId'))
                REGEXP '^[1-9][0-9]{0,19}$'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.applicationId')) = 'STRING'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.applicationId'))
                REGEXP '^[1-9][0-9]{0,19}$'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.paymentNo')) = 'INTEGER'
            AND CAST(
                JSON_UNQUOTE(JSON_EXTRACT(payload, '$.paymentNo')) AS UNSIGNED
            ) = 1
            AND source_kind = 'welfarePayment'
            AND BINARY source_id = BINARY JSON_UNQUOTE(
                JSON_EXTRACT(payload, '$.welfarePaymentId')
            )
            AND occurrence = CAST(
                JSON_UNQUOTE(JSON_EXTRACT(payload, '$.paymentNo')) AS UNSIGNED
            )
        )
    );

CREATE TRIGGER tr_scheduled_settlement_welfare_insert
BEFORE INSERT ON scheduled_settlement
FOR EACH ROW
FOLLOWS tr_scheduled_settlement_property_tax_insert
SET NEW.status = IF(
    NEW.kind <> 'welfareBenefitPayment'
        OR EXISTS (
            SELECT 1
            FROM welfare_payment AS payment
            INNER JOIN welfare_application AS application
                ON application.id = payment.application_id
               AND application.save_id = payment.save_id
               AND application.run_revision = payment.run_revision
            WHERE payment.id = CAST(
                      JSON_UNQUOTE(
                          JSON_EXTRACT(NEW.payload, '$.welfarePaymentId')
                      ) AS UNSIGNED
                  )
              AND payment.save_id = NEW.save_id
              AND payment.run_revision = NEW.run_revision
              AND payment.application_id = CAST(
                  JSON_UNQUOTE(JSON_EXTRACT(NEW.payload, '$.applicationId'))
                  AS UNSIGNED
              )
              AND payment.payment_no = NEW.occurrence
              AND payment.due_game_day = NEW.due_game_day
              AND payment.status = 'pending'
              AND payment.scheduled_settlement_id IS NULL
              AND application.status = 'active'
        ),
    NEW.status,
    NULL
);

CREATE TRIGGER tr_scheduled_settlement_welfare_transition
BEFORE UPDATE ON scheduled_settlement
FOR EACH ROW
FOLLOWS tr_scheduled_settlement_property_tax_transition
SET NEW.status = IF(
    OLD.kind <> 'welfareBenefitPayment'
        OR (
            NEW.status = 'settled'
            AND EXISTS (
                SELECT 1
                FROM welfare_payment AS payment
                WHERE payment.id = CAST(
                          JSON_UNQUOTE(
                              JSON_EXTRACT(OLD.payload, '$.welfarePaymentId')
                          ) AS UNSIGNED
                      )
                  AND payment.save_id = OLD.save_id
                  AND payment.run_revision = OLD.run_revision
                  AND payment.application_id = CAST(
                      JSON_UNQUOTE(JSON_EXTRACT(OLD.payload, '$.applicationId'))
                      AS UNSIGNED
                  )
                  AND payment.payment_no = OLD.occurrence
                  AND payment.status = 'paid'
                  AND payment.ledger_transaction_id
                        = NEW.settled_ledger_transaction_id
            )
        )
        OR (
            NEW.status = 'cancelled'
            AND NEW.cancellation_reason = 'newRun'
            AND NEW.cancellation_ledger_transaction_id IS NULL
            AND EXISTS (
                SELECT 1
                FROM welfare_payment AS payment
                WHERE payment.id = CAST(
                          JSON_UNQUOTE(
                              JSON_EXTRACT(OLD.payload, '$.welfarePaymentId')
                          ) AS UNSIGNED
                      )
                  AND payment.save_id = OLD.save_id
                  AND payment.run_revision = OLD.run_revision
                  AND payment.status = 'cancelled'
                  AND payment.scheduled_settlement_id = OLD.id
            )
        ),
    NEW.status,
    NULL
);

-- The first program is a fictional, unranked balance fixture. Its unusual numbers make an
-- accidental replacement with a real policy or a code default immediately visible.
INSERT INTO life_component_version
    (component_kind, version_key, availability, ranked_eligible)
VALUES
    ('welfare', 'dev-unranked-m4-welfare-2026-v1', 'active', FALSE);

INSERT INTO welfare_fact_definition
    (
        life_component_version_id, fact_order, fact_key, value_type, unit,
        enum_schema_key, window_kind, minimum_window_days, maximum_window_days,
        collection_bound, source_schema_version, source_kind
    )
SELECT component.id, seed.fact_order, seed.fact_key, seed.value_type, seed.unit,
       seed.enum_schema_key, seed.window_kind, seed.minimum_window_days,
       seed.maximum_window_days, seed.collection_bound, 1, seed.source_kind
FROM life_component_version AS component
INNER JOIN (
    SELECT 1 AS fact_order, 'character.age' AS fact_key,
           'ageYears' AS value_type, 'years' AS unit,
           CAST(NULL AS CHAR(64)) AS enum_schema_key,
           'currentGameDay' AS window_kind,
           CAST(NULL AS UNSIGNED) AS minimum_window_days,
           CAST(NULL AS UNSIGNED) AS maximum_window_days,
           CAST(NULL AS UNSIGNED) AS collection_bound,
           'gameDay' AS source_kind
    UNION ALL SELECT 2, 'household.memberCount', 'count', 'count', NULL,
           'currentGameDay', NULL, NULL, 32, 'household'
    UNION ALL SELECT 3, 'household.dependentCount', 'count', 'count', NULL,
           'currentGameDay', NULL, NULL, 32, 'household'
    UNION ALL SELECT 4, 'residence.exists', 'boolean', 'boolean', NULL,
           'currentGameDay', NULL, NULL, NULL, 'residence'
    UNION ALL SELECT 5, 'residence.region', 'enum', 'enum', 'region',
           'currentGameDay', NULL, NULL, NULL, 'residence'
    UNION ALL SELECT 6, 'career.employmentStatus', 'enum', 'enum',
           'welfareEmployment', 'currentGameDay', NULL, NULL, NULL,
           'employment'
    UNION ALL SELECT 7, 'military.status', 'enum', 'enum', 'military',
           'currentGameDay', NULL, NULL, NULL, 'military'
    UNION ALL SELECT 8, 'income.periodTotal', 'moneyKrw', 'krw', NULL,
           'previousClosedDays', 1, 366, 32, 'income'
    UNION ALL SELECT 9, 'asset.policyValuation', 'moneyKrw', 'krw', NULL,
           'priorClose', NULL, NULL, 32, 'asset'
    UNION ALL SELECT 10, 'debt.policyBalance', 'moneyKrw', 'krw', NULL,
           'priorClose', NULL, NULL, 32, 'debt'
) AS seed
    ON TRUE
WHERE component.component_kind = 'welfare'
  AND component.version_key = 'dev-unranked-m4-welfare-2026-v1';

INSERT INTO welfare_program_version
    (
        life_component_version_id, schema_version, program_key, display_name,
        purpose, ranked_availability,
        application_kind, application_period_kind,
        application_start_game_day, application_end_game_day,
        duplicate_group_key, duplicate_scope, maximum_approved_per_group,
        reassessment_basis, ast_node_count, ast_max_depth,
        eligibility_ast, benefit_formula, payment_schedule
    )
SELECT component.id,
       1,
       'fictionalRestartGrant',
       '라이프 새출발 지원금',
       'gameBalance',
       'unrankedOnly',
       'manual',
       'always',
       NULL,
       NULL,
       'fictionalRestartGrant',
       'run',
       1,
       'eligibilityAtApplication',
       24,
       4,
       JSON_OBJECT(
           'version', 1,
           'kind', 'all',
           'conditionCodes', JSON_ARRAY(
               'ageWindow', 'workTransition', 'recentIncome',
               'policyAsset', 'residenceKnown', 'notServing'
           )
       ),
       JSON_OBJECT(
           'version', 1,
           'kind', 'fixed',
           'amount', JSON_OBJECT(
               'kind', 'constant',
               'key', 'benefitKrw',
               'unit', 'krw'
           )
       ),
       JSON_OBJECT(
           'version', 1,
           'kind', 'once',
           'delayGameDays', 1
       )
FROM life_component_version AS component
WHERE component.component_kind = 'welfare'
  AND component.version_key = 'dev-unranked-m4-welfare-2026-v1';

INSERT INTO welfare_program_constant
    (
        program_version_id, constant_order, constant_key,
        value_type, unit, integer_value
    )
SELECT program.id, seed.constant_order, seed.constant_key,
       seed.value_type, seed.unit, seed.integer_value
FROM welfare_program_version AS program
INNER JOIN (
    SELECT 1 AS constant_order, 'minimumAgeYears' AS constant_key,
           'ageYears' AS value_type, 'years' AS unit, 22 AS integer_value
    UNION ALL SELECT 2, 'maximumAgeYears', 'ageYears', 'years', 67
    UNION ALL SELECT 3, 'incomeWindowDays', 'count', 'count', 30
    UNION ALL SELECT 4, 'incomeCapKrw', 'moneyKrw', 'krw', 1234567
    UNION ALL SELECT 5, 'assetCapKrw', 'moneyKrw', 'krw', 12345678
    UNION ALL SELECT 6, 'benefitKrw', 'moneyKrw', 'krw', 333000
) AS seed
    ON TRUE
WHERE program.program_key = 'fictionalRestartGrant'
  AND program.life_component_version_id = (
      SELECT id FROM life_component_version
      WHERE component_kind = 'welfare'
        AND version_key = 'dev-unranked-m4-welfare-2026-v1'
  );

INSERT INTO welfare_program_condition
    (
        program_version_id, condition_order, condition_code,
        public_label, node_count, max_depth, expression_ast
    )
SELECT program.id,
       1,
       'ageWindow',
       '나이 범위',
       4,
       2,
       JSON_OBJECT(
           'kind', 'between',
           'value', JSON_OBJECT(
               'kind', 'fact', 'path', 'character.age',
               'unit', 'years',
               'window', JSON_OBJECT('kind', 'currentGameDay')
           ),
           'lower', JSON_OBJECT('kind', 'constant', 'key', 'minimumAgeYears'),
           'upper', JSON_OBJECT('kind', 'constant', 'key', 'maximumAgeYears')
       )
FROM welfare_program_version AS program
WHERE program.program_key = 'fictionalRestartGrant'
  AND program.life_component_version_id = (
      SELECT id FROM life_component_version
      WHERE component_kind = 'welfare'
        AND version_key = 'dev-unranked-m4-welfare-2026-v1'
  );

INSERT INTO welfare_program_condition
    (
        program_version_id, condition_order, condition_code,
        public_label, node_count, max_depth, expression_ast
    )
SELECT program.id,
       2,
       'workTransition',
       '고용 전환 또는 부양가족',
       8,
       3,
       JSON_OBJECT(
           'kind', 'any',
           'children', JSON_ARRAY(
               JSON_OBJECT(
                   'kind', 'in',
                   'value', JSON_OBJECT(
                       'kind', 'fact', 'path', 'career.employmentStatus',
                       'unit', 'enum',
                       'window', JSON_OBJECT('kind', 'currentGameDay')
                   ),
                   'literals', JSON_ARRAY(
                       JSON_OBJECT(
                           'kind', 'literal', 'valueType', 'enum',
                           'unit', 'enum', 'schemaKey', 'welfareEmployment',
                           'value', 'none'
                       ),
                       JSON_OBJECT(
                           'kind', 'literal', 'valueType', 'enum',
                           'unit', 'enum', 'schemaKey', 'welfareEmployment',
                           'value', 'ended'
                       )
                   )
               ),
               JSON_OBJECT(
                   'kind', 'gte',
                   'left', JSON_OBJECT(
                       'kind', 'fact', 'path', 'household.dependentCount',
                       'unit', 'count',
                       'window', JSON_OBJECT('kind', 'currentGameDay')
                   ),
                   'right', JSON_OBJECT(
                       'kind', 'literal', 'valueType', 'count',
                       'unit', 'count', 'value', 1
                   )
               )
           )
       )
FROM welfare_program_version AS program
WHERE program.program_key = 'fictionalRestartGrant'
  AND program.life_component_version_id = (
      SELECT id FROM life_component_version
      WHERE component_kind = 'welfare'
        AND version_key = 'dev-unranked-m4-welfare-2026-v1'
  );

INSERT INTO welfare_program_condition
    (
        program_version_id, condition_order, condition_code,
        public_label, node_count, max_depth, expression_ast
    )
SELECT program.id,
       3,
       'recentIncome',
       '최근 30일 근로소득',
       3,
       2,
       JSON_OBJECT(
           'kind', 'lte',
           'left', JSON_OBJECT(
               'kind', 'fact', 'path', 'income.periodTotal',
               'unit', 'krw',
               'window', JSON_OBJECT(
                   'kind', 'previousClosedDays',
                   'days', JSON_OBJECT('kind', 'constant', 'key', 'incomeWindowDays')
               )
           ),
           'right', JSON_OBJECT('kind', 'constant', 'key', 'incomeCapKrw')
       )
FROM welfare_program_version AS program
WHERE program.program_key = 'fictionalRestartGrant'
  AND program.life_component_version_id = (
      SELECT id FROM life_component_version
      WHERE component_kind = 'welfare'
        AND version_key = 'dev-unranked-m4-welfare-2026-v1'
  );

INSERT INTO welfare_program_condition
    (
        program_version_id, condition_order, condition_code,
        public_label, node_count, max_depth, expression_ast
    )
SELECT program.id,
       4,
       'policyAsset',
       '직전 마감 정책평가자산',
       3,
       2,
       JSON_OBJECT(
           'kind', 'lte',
           'left', JSON_OBJECT(
               'kind', 'fact', 'path', 'asset.policyValuation',
               'unit', 'krw',
               'window', JSON_OBJECT('kind', 'priorClose')
           ),
           'right', JSON_OBJECT('kind', 'constant', 'key', 'assetCapKrw')
       )
FROM welfare_program_version AS program
WHERE program.program_key = 'fictionalRestartGrant'
  AND program.life_component_version_id = (
      SELECT id FROM life_component_version
      WHERE component_kind = 'welfare'
        AND version_key = 'dev-unranked-m4-welfare-2026-v1'
  );

INSERT INTO welfare_program_condition
    (
        program_version_id, condition_order, condition_code,
        public_label, node_count, max_depth, expression_ast
    )
SELECT program.id,
       5,
       'residenceKnown',
       '현재 거주지',
       1,
       1,
       JSON_OBJECT(
           'kind', 'fact', 'path', 'residence.exists',
           'unit', 'boolean',
           'window', JSON_OBJECT('kind', 'currentGameDay')
       )
FROM welfare_program_version AS program
WHERE program.program_key = 'fictionalRestartGrant'
  AND program.life_component_version_id = (
      SELECT id FROM life_component_version
      WHERE component_kind = 'welfare'
        AND version_key = 'dev-unranked-m4-welfare-2026-v1'
  );

INSERT INTO welfare_program_condition
    (
        program_version_id, condition_order, condition_code,
        public_label, node_count, max_depth, expression_ast
    )
SELECT program.id,
       6,
       'notServing',
       '복무 중 아님',
       4,
       3,
       JSON_OBJECT(
           'kind', 'not',
           'child', JSON_OBJECT(
               'kind', 'eq',
               'left', JSON_OBJECT(
                   'kind', 'fact', 'path', 'military.status',
                   'unit', 'enum',
                   'window', JSON_OBJECT('kind', 'currentGameDay')
               ),
               'right', JSON_OBJECT(
                   'kind', 'literal', 'valueType', 'enum',
                   'unit', 'enum', 'schemaKey', 'military',
                   'value', 'serving'
               )
           )
       )
FROM welfare_program_version AS program
WHERE program.program_key = 'fictionalRestartGrant'
  AND program.life_component_version_id = (
      SELECT id FROM life_component_version
      WHERE component_kind = 'welfare'
        AND version_key = 'dev-unranked-m4-welfare-2026-v1'
  );

INSERT INTO welfare_reassessment_trigger
    (program_version_id, trigger_order, source_kind)
SELECT program.id, seed.trigger_order, seed.source_kind
FROM welfare_program_version AS program
INNER JOIN (
    SELECT 1 AS trigger_order, 'gameDay' AS source_kind
    UNION ALL SELECT 2, 'household'
    UNION ALL SELECT 3, 'residence'
    UNION ALL SELECT 4, 'employment'
    UNION ALL SELECT 5, 'military'
    UNION ALL SELECT 6, 'income'
    UNION ALL SELECT 7, 'asset'
) AS seed
    ON TRUE
WHERE program.program_key = 'fictionalRestartGrant'
  AND program.life_component_version_id = (
      SELECT id FROM life_component_version
      WHERE component_kind = 'welfare'
        AND version_key = 'dev-unranked-m4-welfare-2026-v1'
  );

-- Canonical child serializations are stored as ordered strings inside the manifest object.
-- Every typed value still participates in the hash while avoiding reader-session aggregation.
CREATE VIEW welfare_component_canonical_projection AS
SELECT component.id AS life_component_version_id,
       CAST(JSON_OBJECT(
           'availability', component.availability,
           'componentKind', component.component_kind,
           'factsCanonical', (
               SELECT GROUP_CONCAT(
                   CAST(JSON_OBJECT(
                       'collectionBound', fact.collection_bound,
                       'enumSchemaKey', fact.enum_schema_key,
                       'factKey', fact.fact_key,
                       'factOrder', fact.fact_order,
                       'maximumWindowDays', fact.maximum_window_days,
                       'minimumWindowDays', fact.minimum_window_days,
                       'sourceKind', fact.source_kind,
                       'sourceSchemaVersion', fact.source_schema_version,
                       'unit', fact.unit,
                       'valueType', fact.value_type,
                       'windowKind', fact.window_kind
                   ) AS CHAR CHARACTER SET utf8mb4)
                   ORDER BY fact.fact_order SEPARATOR '\n'
               )
               FROM welfare_fact_definition AS fact
               WHERE fact.life_component_version_id = component.id
           ),
           'programsCanonical', (
               SELECT GROUP_CONCAT(
                   CAST(JSON_OBJECT(
                       'applicationEndGameDay', program.application_end_game_day,
                       'applicationKind', program.application_kind,
                       'applicationPeriodKind', program.application_period_kind,
                       'applicationStartGameDay', program.application_start_game_day,
                       'astMaxDepth', program.ast_max_depth,
                       'astNodeCount', program.ast_node_count,
                       'benefitFormula', program.benefit_formula,
                       'conditionsCanonical', (
                           SELECT GROUP_CONCAT(
                               CAST(JSON_OBJECT(
                                   'conditionCode', condition_row.condition_code,
                                   'conditionOrder', condition_row.condition_order,
                                   'expressionAst', condition_row.expression_ast,
                                   'maxDepth', condition_row.max_depth,
                                   'nodeCount', condition_row.node_count,
                                   'publicLabel', condition_row.public_label
                               ) AS CHAR CHARACTER SET utf8mb4)
                               ORDER BY condition_row.condition_order SEPARATOR '\n'
                           )
                           FROM welfare_program_condition AS condition_row
                           WHERE condition_row.program_version_id = program.id
                       ),
                       'constantsCanonical', (
                           SELECT GROUP_CONCAT(
                               CAST(JSON_OBJECT(
                                   'constantKey', constant_row.constant_key,
                                   'constantOrder', constant_row.constant_order,
                                   'booleanValue', constant_row.boolean_value,
                                   'dateValue', IF(
                                       constant_row.date_value IS NULL,
                                       NULL,
                                       DATE_FORMAT(constant_row.date_value, '%Y-%m-%d')
                                   ),
                                   'enumSchemaKey', constant_row.enum_schema_key,
                                   'integerValue', constant_row.integer_value,
                                   'stringValue', constant_row.string_value,
                                   'unit', constant_row.unit,
                                   'valueType', constant_row.value_type
                               ) AS CHAR CHARACTER SET utf8mb4)
                               ORDER BY constant_row.constant_order SEPARATOR '\n'
                           )
                           FROM welfare_program_constant AS constant_row
                           WHERE constant_row.program_version_id = program.id
                       ),
                       'displayName', program.display_name,
                       'duplicateGroupKey', program.duplicate_group_key,
                       'duplicateScope', program.duplicate_scope,
                       'eligibilityAst', program.eligibility_ast,
                       'maximumApprovedPerGroup', program.maximum_approved_per_group,
                       'paymentSchedule', program.payment_schedule,
                       'programKey', program.program_key,
                       'purpose', program.purpose,
                       'rankedAvailability', program.ranked_availability,
                       'reassessmentBasis', program.reassessment_basis,
                       'schemaVersion', program.schema_version,
                       'triggersCanonical', (
                           SELECT GROUP_CONCAT(
                               CAST(JSON_OBJECT(
                                   'sourceKind', trigger_row.source_kind,
                                   'triggerOrder', trigger_row.trigger_order
                               ) AS CHAR CHARACTER SET utf8mb4)
                               ORDER BY trigger_row.trigger_order SEPARATOR '\n'
                           )
                           FROM welfare_reassessment_trigger AS trigger_row
                           WHERE trigger_row.program_version_id = program.id
                       )
                   ) AS CHAR CHARACTER SET utf8mb4)
                   ORDER BY program.program_key SEPARATOR '\n'
               )
               FROM welfare_program_version AS program
               WHERE program.life_component_version_id = component.id
           ),
           'rankedEligible', component.ranked_eligible,
           'schemaVersion', 1,
           'versionKey', component.version_key
       ) AS CHAR CHARACTER SET utf8mb4) AS canonical_json,
       SHA2(
           CAST(JSON_OBJECT(
               'availability', component.availability,
               'componentKind', component.component_kind,
               'factsCanonical', (
                   SELECT GROUP_CONCAT(
                       CAST(JSON_OBJECT(
                           'collectionBound', fact.collection_bound,
                           'enumSchemaKey', fact.enum_schema_key,
                           'factKey', fact.fact_key,
                           'factOrder', fact.fact_order,
                           'maximumWindowDays', fact.maximum_window_days,
                           'minimumWindowDays', fact.minimum_window_days,
                           'sourceKind', fact.source_kind,
                           'sourceSchemaVersion', fact.source_schema_version,
                           'unit', fact.unit,
                           'valueType', fact.value_type,
                           'windowKind', fact.window_kind
                       ) AS CHAR CHARACTER SET utf8mb4)
                       ORDER BY fact.fact_order SEPARATOR '\n'
                   )
                   FROM welfare_fact_definition AS fact
                   WHERE fact.life_component_version_id = component.id
               ),
               'programsCanonical', (
                   SELECT GROUP_CONCAT(
                       CAST(JSON_OBJECT(
                           'applicationEndGameDay', program.application_end_game_day,
                           'applicationKind', program.application_kind,
                           'applicationPeriodKind', program.application_period_kind,
                           'applicationStartGameDay', program.application_start_game_day,
                           'astMaxDepth', program.ast_max_depth,
                           'astNodeCount', program.ast_node_count,
                           'benefitFormula', program.benefit_formula,
                           'conditionsCanonical', (
                               SELECT GROUP_CONCAT(
                                   CAST(JSON_OBJECT(
                                       'conditionCode', condition_row.condition_code,
                                       'conditionOrder', condition_row.condition_order,
                                       'expressionAst', condition_row.expression_ast,
                                       'maxDepth', condition_row.max_depth,
                                       'nodeCount', condition_row.node_count,
                                       'publicLabel', condition_row.public_label
                                   ) AS CHAR CHARACTER SET utf8mb4)
                                   ORDER BY condition_row.condition_order SEPARATOR '\n'
                               )
                               FROM welfare_program_condition AS condition_row
                               WHERE condition_row.program_version_id = program.id
                           ),
                           'constantsCanonical', (
                               SELECT GROUP_CONCAT(
                                   CAST(JSON_OBJECT(
                                       'constantKey', constant_row.constant_key,
                                       'constantOrder', constant_row.constant_order,
                                       'booleanValue', constant_row.boolean_value,
                                       'dateValue', IF(
                                           constant_row.date_value IS NULL,
                                           NULL,
                                           DATE_FORMAT(constant_row.date_value, '%Y-%m-%d')
                                       ),
                                       'enumSchemaKey', constant_row.enum_schema_key,
                                       'integerValue', constant_row.integer_value,
                                       'stringValue', constant_row.string_value,
                                       'unit', constant_row.unit,
                                       'valueType', constant_row.value_type
                                   ) AS CHAR CHARACTER SET utf8mb4)
                                   ORDER BY constant_row.constant_order SEPARATOR '\n'
                               )
                               FROM welfare_program_constant AS constant_row
                               WHERE constant_row.program_version_id = program.id
                           ),
                           'displayName', program.display_name,
                           'duplicateGroupKey', program.duplicate_group_key,
                           'duplicateScope', program.duplicate_scope,
                           'eligibilityAst', program.eligibility_ast,
                           'maximumApprovedPerGroup', program.maximum_approved_per_group,
                           'paymentSchedule', program.payment_schedule,
                           'programKey', program.program_key,
                           'purpose', program.purpose,
                           'rankedAvailability', program.ranked_availability,
                           'reassessmentBasis', program.reassessment_basis,
                           'schemaVersion', program.schema_version,
                           'triggersCanonical', (
                               SELECT GROUP_CONCAT(
                                   CAST(JSON_OBJECT(
                                       'sourceKind', trigger_row.source_kind,
                                       'triggerOrder', trigger_row.trigger_order
                                   ) AS CHAR CHARACTER SET utf8mb4)
                                   ORDER BY trigger_row.trigger_order SEPARATOR '\n'
                               )
                               FROM welfare_reassessment_trigger AS trigger_row
                               WHERE trigger_row.program_version_id = program.id
                           )
                       ) AS CHAR CHARACTER SET utf8mb4)
                       ORDER BY program.program_key SEPARATOR '\n'
                   )
                   FROM welfare_program_version AS program
                   WHERE program.life_component_version_id = component.id
               ),
               'rankedEligible', component.ranked_eligible,
               'schemaVersion', 1,
               'versionKey', component.version_key
           ) AS CHAR CHARACTER SET utf8mb4),
           256
       ) AS canonical_sha256
FROM life_component_version AS component
WHERE component.component_kind = 'welfare'
  AND component.availability = 'active';

CREATE TRIGGER tr_life_component_version_welfare_publish
BEFORE UPDATE ON life_component_version
FOR EACH ROW
FOLLOWS tr_life_component_version_living_publish
SET NEW.version_key = IF(
    NEW.component_kind <> 'welfare'
        OR NEW.availability <> 'active'
        OR (
            OLD.version_key = 'dev-unranked-m4-welfare-2026-v1'
            AND OLD.ranked_eligible = FALSE
            AND (
                SELECT COUNT(*)
                FROM welfare_fact_definition AS fact
                WHERE fact.life_component_version_id = OLD.id
            ) = 10
            AND NOT EXISTS (
                SELECT 1
                FROM welfare_fact_definition AS fact
                WHERE fact.life_component_version_id = OLD.id
                  AND (
                      fact.fact_order NOT BETWEEN 1 AND 10
                      OR fact.source_schema_version <> 1
                      OR (fact.collection_bound IS NOT NULL
                          AND fact.collection_bound > 32)
                  )
            )
            AND EXISTS (
                SELECT 1 FROM welfare_fact_definition AS fact
                WHERE fact.life_component_version_id = OLD.id
                  AND fact.fact_key = 'career.employmentStatus'
                  AND fact.value_type = 'enum'
                  AND fact.enum_schema_key = 'welfareEmployment'
                  AND fact.window_kind = 'currentGameDay'
            )
            AND EXISTS (
                SELECT 1 FROM welfare_fact_definition AS fact
                WHERE fact.life_component_version_id = OLD.id
                  AND fact.fact_key = 'income.periodTotal'
                  AND fact.value_type = 'moneyKrw'
                  AND fact.unit = 'krw'
                  AND fact.window_kind = 'previousClosedDays'
                  AND fact.minimum_window_days = 1
                  AND fact.maximum_window_days = 366
                  AND fact.collection_bound = 32
                  AND fact.source_kind = 'income'
            )
            AND EXISTS (
                SELECT 1 FROM welfare_fact_definition AS fact
                WHERE fact.life_component_version_id = OLD.id
                  AND fact.fact_key = 'asset.policyValuation'
                  AND fact.value_type = 'moneyKrw'
                  AND fact.window_kind = 'priorClose'
                  AND fact.collection_bound = 32
                  AND fact.source_kind = 'asset'
            )
            AND EXISTS (
                SELECT 1 FROM welfare_program_version AS program
                WHERE program.life_component_version_id = OLD.id
                  AND program.program_key = 'fictionalRestartGrant'
                  AND program.display_name = '라이프 새출발 지원금'
                  AND program.purpose = 'gameBalance'
                  AND program.ranked_availability = 'unrankedOnly'
                  AND program.application_kind = 'manual'
                  AND program.application_period_kind = 'always'
                  AND program.duplicate_group_key = 'fictionalRestartGrant'
                  AND program.duplicate_scope = 'run'
                  AND program.maximum_approved_per_group = 1
                  AND program.reassessment_basis = 'eligibilityAtApplication'
                  AND program.ast_node_count = 24
                  AND program.ast_max_depth = 4
                  AND JSON_UNQUOTE(
                      JSON_EXTRACT(program.eligibility_ast, '$.kind')
                  ) = 'all'
                  AND JSON_LENGTH(
                      JSON_EXTRACT(program.eligibility_ast, '$.conditionCodes')
                  ) = 6
                  AND JSON_UNQUOTE(
                      JSON_EXTRACT(program.benefit_formula, '$.kind')
                  ) = 'fixed'
                  AND JSON_UNQUOTE(
                      JSON_EXTRACT(program.benefit_formula, '$.amount.key')
                  ) = 'benefitKrw'
                  AND JSON_UNQUOTE(
                      JSON_EXTRACT(program.payment_schedule, '$.kind')
                  ) = 'once'
                  AND JSON_UNQUOTE(
                      JSON_EXTRACT(program.payment_schedule, '$.delayGameDays')
                  ) = '1'
            )
            AND (
                SELECT COUNT(*) FROM welfare_program_version AS program
                WHERE program.life_component_version_id = OLD.id
            ) = 1
            AND EXISTS (
                SELECT 1
                FROM welfare_program_version AS program
                WHERE program.life_component_version_id = OLD.id
                  AND (
                      SELECT COUNT(*)
                      FROM welfare_program_constant AS constant_row
                      WHERE constant_row.program_version_id = program.id
                  ) = 6
                  AND (
                      SELECT SUM(
                          CASE
                              WHEN constant_row.constant_key = 'minimumAgeYears'
                                   AND constant_row.value_type = 'ageYears'
                                   AND constant_row.unit = 'years'
                                   AND constant_row.integer_value = 22 THEN 1
                              WHEN constant_row.constant_key = 'maximumAgeYears'
                                   AND constant_row.value_type = 'ageYears'
                                   AND constant_row.unit = 'years'
                                   AND constant_row.integer_value = 67 THEN 1
                              WHEN constant_row.constant_key = 'incomeWindowDays'
                                   AND constant_row.value_type = 'count'
                                   AND constant_row.unit = 'count'
                                   AND constant_row.integer_value = 30 THEN 1
                              WHEN constant_row.constant_key = 'incomeCapKrw'
                                   AND constant_row.value_type = 'moneyKrw'
                                   AND constant_row.unit = 'krw'
                                   AND constant_row.integer_value = 1234567 THEN 1
                              WHEN constant_row.constant_key = 'assetCapKrw'
                                   AND constant_row.value_type = 'moneyKrw'
                                   AND constant_row.unit = 'krw'
                                   AND constant_row.integer_value = 12345678 THEN 1
                              WHEN constant_row.constant_key = 'benefitKrw'
                                   AND constant_row.value_type = 'moneyKrw'
                                   AND constant_row.unit = 'krw'
                                   AND constant_row.integer_value = 333000 THEN 1
                              ELSE 0
                          END
                      )
                      FROM welfare_program_constant AS constant_row
                      WHERE constant_row.program_version_id = program.id
                  ) = 6
                  AND (
                      SELECT COUNT(*)
                      FROM welfare_program_condition AS condition_row
                      WHERE condition_row.program_version_id = program.id
                  ) = 6
                  AND (
                      SELECT SUM(condition_row.node_count) + 1
                      FROM welfare_program_condition AS condition_row
                      WHERE condition_row.program_version_id = program.id
                  ) = program.ast_node_count
                  AND (
                      SELECT SUM(
                          CASE
                              WHEN condition_row.condition_order = 1
                                   AND condition_row.condition_code = 'ageWindow'
                                   AND condition_row.node_count = 4 THEN 1
                              WHEN condition_row.condition_order = 2
                                   AND condition_row.condition_code = 'workTransition'
                                   AND condition_row.node_count = 8 THEN 1
                              WHEN condition_row.condition_order = 3
                                   AND condition_row.condition_code = 'recentIncome'
                                   AND condition_row.node_count = 3 THEN 1
                              WHEN condition_row.condition_order = 4
                                   AND condition_row.condition_code = 'policyAsset'
                                   AND condition_row.node_count = 3 THEN 1
                              WHEN condition_row.condition_order = 5
                                   AND condition_row.condition_code = 'residenceKnown'
                                   AND condition_row.node_count = 1 THEN 1
                              WHEN condition_row.condition_order = 6
                                   AND condition_row.condition_code = 'notServing'
                                   AND condition_row.node_count = 4 THEN 1
                              ELSE 0
                          END
                      )
                      FROM welfare_program_condition AS condition_row
                      WHERE condition_row.program_version_id = program.id
                  ) = 6
                  AND (
                      SELECT COUNT(*)
                      FROM welfare_reassessment_trigger AS trigger_row
                      WHERE trigger_row.program_version_id = program.id
                  ) = 7
                  AND NOT EXISTS (
                      SELECT required.source_kind
                      FROM (
                          SELECT 'gameDay' AS source_kind
                          UNION ALL SELECT 'household'
                          UNION ALL SELECT 'residence'
                          UNION ALL SELECT 'employment'
                          UNION ALL SELECT 'military'
                          UNION ALL SELECT 'income'
                          UNION ALL SELECT 'asset'
                      ) AS required
                      WHERE NOT EXISTS (
                          SELECT 1
                          FROM welfare_reassessment_trigger AS trigger_row
                          WHERE trigger_row.program_version_id = program.id
                            AND trigger_row.source_kind = required.source_kind
                      )
                  )
            )
            AND EXISTS (
                SELECT 1
                FROM welfare_component_canonical_projection AS projection
                INNER JOIN life_component_canonical_manifest AS manifest
                    ON manifest.life_component_version_id
                        = projection.life_component_version_id
                   AND BINARY manifest.canonical_json = BINARY projection.canonical_json
                   AND BINARY manifest.canonical_sha256
                        = BINARY projection.canonical_sha256
                WHERE projection.life_component_version_id = OLD.id
            )
        ),
    NEW.version_key,
    NULL
);

INSERT INTO life_component_canonical_manifest
    (life_component_version_id, canonical_json)
SELECT projection.life_component_version_id, projection.canonical_json
FROM welfare_component_canonical_projection AS projection
INNER JOIN life_component_version AS component
    ON component.id = projection.life_component_version_id
WHERE component.version_key = 'dev-unranked-m4-welfare-2026-v1'
  AND component.sealed_at IS NULL;

UPDATE life_component_version AS component
INNER JOIN life_component_canonical_manifest AS manifest
    ON manifest.life_component_version_id = component.id
SET component.canonical_sha256 = manifest.canonical_sha256,
    component.sealed_at = CURRENT_TIMESTAMP(3)
WHERE component.component_kind = 'welfare'
  AND component.version_key = 'dev-unranked-m4-welfare-2026-v1'
  AND component.sealed_at IS NULL;

-- Clone the current newRun life graph and replace only its welfare component.
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
SELECT 'dev-unranked-m4-life-welfare-2026-v2',
       FALSE,
       previous.legacy_dependent_age_years,
       previous.living_cost_component_version_id,
       welfare.id,
       previous.life_event_component_version_id,
       previous.insurance_component_version_id,
       previous.corporation_component_version_id
FROM m4d_previous_new_run_life AS previous
INNER JOIN life_component_version AS welfare
    ON welfare.component_kind = 'welfare'
   AND welfare.version_key = 'dev-unranked-m4-welfare-2026-v1'
   AND welfare.availability = 'active'
   AND welfare.sealed_at IS NOT NULL;

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
WHERE catalog_key = 'dev-unranked-m4-life-welfare-2026-v2'
  AND sealed_at IS NULL;

CREATE TEMPORARY TABLE m4d_publication_guard (
    guard_key VARCHAR(48) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    accepted  TINYINT UNSIGNED NOT NULL,
    CONSTRAINT ck_m4d_publication_guard CHECK (accepted = 1)
);

INSERT INTO m4d_publication_guard (guard_key, accepted)
SELECT 'sealed-welfare-v1', IF(
    EXISTS (
        SELECT 1
        FROM life_component_version AS component
        INNER JOIN life_component_canonical_manifest AS manifest
            ON manifest.life_component_version_id = component.id
        INNER JOIN welfare_component_canonical_projection AS projection
            ON projection.life_component_version_id = component.id
        INNER JOIN welfare_program_version AS program
            ON program.life_component_version_id = component.id
        WHERE component.component_kind = 'welfare'
          AND component.version_key = 'dev-unranked-m4-welfare-2026-v1'
          AND component.availability = 'active'
          AND component.ranked_eligible = FALSE
          AND component.sealed_at IS NOT NULL
          AND BINARY component.canonical_sha256 = BINARY manifest.canonical_sha256
          AND BINARY manifest.canonical_json = BINARY projection.canonical_json
          AND BINARY manifest.canonical_sha256 = BINARY projection.canonical_sha256
          AND program.program_key = 'fictionalRestartGrant'
          AND program.purpose = 'gameBalance'
          AND program.ranked_availability = 'unrankedOnly'
          AND (SELECT COUNT(*) FROM welfare_fact_definition AS fact
               WHERE fact.life_component_version_id = component.id) = 10
          AND (SELECT COUNT(*) FROM welfare_program_constant AS constant_row
               WHERE constant_row.program_version_id = program.id) = 6
          AND (SELECT COUNT(*) FROM welfare_program_condition AS condition_row
               WHERE condition_row.program_version_id = program.id) = 6
          AND (SELECT COUNT(*) FROM welfare_reassessment_trigger AS trigger_row
               WHERE trigger_row.program_version_id = program.id) = 7
    ),
    1,
    0
);

INSERT INTO m4d_publication_guard (guard_key, accepted)
SELECT 'sealed-life-v2-clone', IF(
    EXISTS (
        SELECT 1
        FROM life_catalog_set AS catalog
        INNER JOIN life_component_version AS welfare
            ON welfare.id = catalog.welfare_component_version_id
        INNER JOIN m4d_previous_new_run_life AS previous
            ON previous.legacy_dependent_age_years
                = catalog.legacy_dependent_age_years
           AND previous.living_cost_component_version_id
                = catalog.living_cost_component_version_id
           AND previous.life_event_component_version_id
                = catalog.life_event_component_version_id
           AND previous.insurance_component_version_id
                = catalog.insurance_component_version_id
           AND previous.corporation_component_version_id
                = catalog.corporation_component_version_id
        WHERE catalog.catalog_key = 'dev-unranked-m4-life-welfare-2026-v2'
          AND catalog.ranked_eligible = FALSE
          AND catalog.sealed_at IS NOT NULL
          AND catalog.canonical_sha256 = SHA2(
              CAST(JSON_OBJECT(
                  'catalogKey', catalog.catalog_key,
                  'corporationComponentVersionId',
                      CAST(catalog.corporation_component_version_id AS CHAR),
                  'insuranceComponentVersionId',
                      CAST(catalog.insurance_component_version_id AS CHAR),
                  'lifeEventComponentVersionId',
                      CAST(catalog.life_event_component_version_id AS CHAR),
                  'legacyDependentAgeYears', catalog.legacy_dependent_age_years,
                  'livingCostComponentVersionId',
                      CAST(catalog.living_cost_component_version_id AS CHAR),
                  'schemaVersion', 1,
                  'welfareComponentVersionId',
                      CAST(catalog.welfare_component_version_id AS CHAR)
              ) AS CHAR CHARACTER SET utf8mb4),
              256
          )
          AND welfare.version_key = 'dev-unranked-m4-welfare-2026-v1'
          AND welfare.sealed_at IS NOT NULL
    ),
    1,
    0
);

INSERT INTO m4d_publication_guard (guard_key, accepted)
SELECT 'existing-run-pins-unchanged', IF(
    NOT EXISTS (
        SELECT 1
        FROM m4d_existing_run_life_pins AS previous
        LEFT JOIN run_rule_bundle AS current_bundle
            ON current_bundle.save_id = previous.save_id
           AND current_bundle.run_revision = previous.run_revision
        WHERE current_bundle.save_id IS NULL
           OR current_bundle.life_catalog_set_id <> previous.life_catalog_set_id
    )
        AND NOT EXISTS (
            SELECT 1
            FROM run_rule_bundle AS current_bundle
            INNER JOIN life_catalog_set AS catalog
                ON catalog.id = current_bundle.life_catalog_set_id
            WHERE catalog.catalog_key = 'dev-unranked-m4-life-welfare-2026-v2'
        ),
    1,
    0
);

INSERT INTO m4d_publication_guard (guard_key, accepted)
SELECT 'finance-protocol-expanded', IF(
    EXISTS (
        SELECT 1
        FROM information_schema.CHECK_CONSTRAINTS AS constraint_row
        WHERE constraint_row.CONSTRAINT_SCHEMA = DATABASE()
          AND constraint_row.CONSTRAINT_NAME = 'ck_ledger_posting_account_code'
          AND constraint_row.CHECK_CLAUSE LIKE '%welfareBenefitIncome%'
    )
        AND EXISTS (
            SELECT 1
            FROM information_schema.CHECK_CONSTRAINTS AS constraint_row
            WHERE constraint_row.CONSTRAINT_SCHEMA = DATABASE()
              AND constraint_row.CONSTRAINT_NAME = 'ck_scheduled_settlement_kind'
              AND constraint_row.CHECK_CLAUSE LIKE '%welfareBenefitPayment%'
        )
        AND EXISTS (
            SELECT 1
            FROM information_schema.CHECK_CONSTRAINTS AS constraint_row
            WHERE constraint_row.CONSTRAINT_SCHEMA = DATABASE()
              AND constraint_row.CONSTRAINT_NAME = 'ck_scheduled_settlement_source_kind'
              AND constraint_row.CHECK_CLAUSE LIKE '%welfarePayment%'
        ),
    1,
    0
);

DROP TEMPORARY TABLE m4d_publication_guard;

-- This is the only external publication mutation in 0038. The assignment trigger bumps its own
-- revision and revalidates every unchanged market/finance/career/employment pin.
UPDATE run_rule_bundle_assignment AS assignment
INNER JOIN life_catalog_set AS catalog
    ON catalog.catalog_key = 'dev-unranked-m4-life-welfare-2026-v2'
   AND catalog.sealed_at IS NOT NULL
SET assignment.life_catalog_set_id = catalog.id
WHERE assignment.assignment_key = 'newRun';

CREATE TEMPORARY TABLE m4d_assignment_guard (
    guard_key VARCHAR(48) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    accepted  TINYINT UNSIGNED NOT NULL,
    CONSTRAINT ck_m4d_assignment_guard CHECK (accepted = 1)
);

INSERT INTO m4d_assignment_guard (guard_key, accepted)
SELECT 'new-run-welfare-v1-only', IF(
    EXISTS (
        SELECT 1
        FROM run_rule_bundle_assignment AS assignment
        INNER JOIN life_catalog_set AS catalog
            ON catalog.id = assignment.life_catalog_set_id
        INNER JOIN life_component_version AS welfare
            ON welfare.id = catalog.welfare_component_version_id
        INNER JOIN m4d_previous_new_run_life AS previous
            ON assignment.assignment_revision = previous.assignment_revision + 1
           AND catalog.legacy_dependent_age_years
                = previous.legacy_dependent_age_years
           AND catalog.living_cost_component_version_id
                = previous.living_cost_component_version_id
           AND catalog.life_event_component_version_id
                = previous.life_event_component_version_id
           AND catalog.insurance_component_version_id
                = previous.insurance_component_version_id
           AND catalog.corporation_component_version_id
                = previous.corporation_component_version_id
        WHERE assignment.assignment_key = 'newRun'
          AND catalog.catalog_key = 'dev-unranked-m4-life-welfare-2026-v2'
          AND catalog.sealed_at IS NOT NULL
          AND welfare.version_key = 'dev-unranked-m4-welfare-2026-v1'
          AND welfare.availability = 'active'
          AND welfare.sealed_at IS NOT NULL
    )
        AND NOT EXISTS (
            SELECT 1
            FROM m4d_existing_run_life_pins AS previous
            LEFT JOIN run_rule_bundle AS current_bundle
                ON current_bundle.save_id = previous.save_id
               AND current_bundle.run_revision = previous.run_revision
            WHERE current_bundle.save_id IS NULL
               OR current_bundle.life_catalog_set_id <> previous.life_catalog_set_id
        ),
    1,
    0
);

DROP TEMPORARY TABLE m4d_assignment_guard;
DROP TEMPORARY TABLE m4d_existing_run_life_pins;
DROP TEMPORARY TABLE m4d_previous_new_run_life;
