-- M4-D2 deterministic life-event catalog, monthly planning, and choice resolution (§7.3–§7.7).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;
SET SESSION group_concat_max_len = 1048576;

-- DDL auto-commits in MySQL. Preserve both durable run pins and the complete newRun assignment
-- so the final publication barrier can prove that only the life catalog pointer moved.
CREATE TEMPORARY TABLE m4d2_existing_run_life_pins (
    save_id                 BIGINT UNSIGNED NOT NULL,
    run_revision            INT UNSIGNED    NOT NULL,
    life_catalog_set_id     BIGINT UNSIGNED NOT NULL,
    PRIMARY KEY (save_id, run_revision)
) ENGINE = InnoDB;

INSERT INTO m4d2_existing_run_life_pins
    (save_id, run_revision, life_catalog_set_id)
SELECT save_id, run_revision, life_catalog_set_id
FROM run_rule_bundle;

CREATE TEMPORARY TABLE m4d2_previous_new_run_assignment (
    assignment_key                          VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin
                                                NOT NULL,
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
    assignment_revision                     BIGINT UNSIGNED NOT NULL,
    legacy_dependent_age_years               TINYINT UNSIGNED NOT NULL,
    living_cost_component_version_id         BIGINT UNSIGNED NOT NULL,
    welfare_component_version_id             BIGINT UNSIGNED NOT NULL,
    life_event_component_version_id          BIGINT UNSIGNED NOT NULL,
    insurance_component_version_id           BIGINT UNSIGNED NOT NULL,
    corporation_component_version_id         BIGINT UNSIGNED NOT NULL,
    PRIMARY KEY (assignment_key)
) ENGINE = InnoDB;

INSERT INTO m4d2_previous_new_run_assignment
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
        employment_assignment_revision,
        assignment_revision,
        legacy_dependent_age_years,
        living_cost_component_version_id,
        welfare_component_version_id,
        life_event_component_version_id,
        insurance_component_version_id,
        corporation_component_version_id
    )
SELECT assignment.assignment_key,
       assignment.market_world_id,
       assignment.policy_set_id,
       assignment.career_catalog_bundle_id,
       assignment.employment_policy_set_id,
       assignment.life_catalog_set_id,
       assignment.credit_model_version_id,
       assignment.real_estate_model_version_id,
       assignment.market_assignment_revision,
       assignment.finance_assignment_revision,
       assignment.career_assignment_revision,
       assignment.employment_assignment_revision,
       assignment.assignment_revision,
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
    ADD UNIQUE KEY uk_life_catalog_set_life_event_component
        (id, life_event_component_version_id);

CREATE TABLE life_event_fact_definition (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    life_component_version_id       BIGINT UNSIGNED NOT NULL,
    fact_order                      TINYINT UNSIGNED NOT NULL,
    fact_key                        VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    value_type                      VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    unit                            VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    enum_schema_key                 VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    window_kind                     VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    source_schema_version           SMALLINT UNSIGNED NOT NULL,
    source_kind                     VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_life_event_fact_order
        (life_component_version_id, fact_order),
    UNIQUE KEY uk_life_event_fact_key
        (life_component_version_id, fact_key),
    UNIQUE KEY uk_life_event_fact_component_id
        (life_component_version_id, id),
    CONSTRAINT fk_life_event_fact_component
        FOREIGN KEY (life_component_version_id) REFERENCES life_component_version (id),
    CONSTRAINT ck_life_event_fact_order CHECK (fact_order BETWEEN 1 AND 16),
    CONSTRAINT ck_life_event_fact_key CHECK (
        fact_key REGEXP '^[a-z][a-zA-Z0-9.]{0,63}$'
    ),
    CONSTRAINT ck_life_event_fact_type CHECK (
        (
            value_type = 'boolean'
            AND unit = 'boolean'
            AND enum_schema_key IS NULL
        )
        OR (
            value_type = 'count'
            AND unit = 'count'
            AND enum_schema_key IS NULL
        )
        OR (
            value_type = 'ageYears'
            AND unit = 'years'
            AND enum_schema_key IS NULL
        )
        OR (
            value_type = 'enum'
            AND unit = 'enum'
            AND enum_schema_key IS NOT NULL
            AND enum_schema_key REGEXP '^[a-z][a-zA-Z0-9]{0,63}$'
        )
    ),
    CONSTRAINT ck_life_event_fact_contract CHECK (
        window_kind = 'currentGameDay'
        AND source_schema_version = 1
        AND source_kind IN ('gameDay', 'household', 'residence', 'military')
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE life_event_definition (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    life_component_version_id       BIGINT UNSIGNED NOT NULL,
    schema_version                  SMALLINT UNSIGNED NOT NULL,
    entropy_stream_version          SMALLINT UNSIGNED NOT NULL,
    event_order                     TINYINT UNSIGNED NOT NULL,
    event_key                       VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                    VARCHAR(80) NOT NULL,
    purpose                         VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ranked_availability             VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    eligibility_ast                 JSON NOT NULL,
    ast_node_count                  SMALLINT UNSIGNED NOT NULL,
    ast_max_depth                   TINYINT UNSIGNED NOT NULL,
    hazard_ppm                      INT UNSIGNED NOT NULL,
    cooldown_game_days              SMALLINT UNSIGNED NOT NULL,
    maximum_occurrences             SMALLINT UNSIGNED NOT NULL,
    priority                        SMALLINT UNSIGNED NOT NULL,
    exclusive_group_key             VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    offer_duration_game_days        SMALLINT UNSIGNED NOT NULL,
    default_choice_key              VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_life_event_definition_order
        (life_component_version_id, event_order),
    UNIQUE KEY uk_life_event_definition_key
        (life_component_version_id, event_key),
    UNIQUE KEY uk_life_event_definition_component_id
        (life_component_version_id, id),
    CONSTRAINT fk_life_event_definition_component
        FOREIGN KEY (life_component_version_id) REFERENCES life_component_version (id),
    CONSTRAINT ck_life_event_definition_schema CHECK (
        schema_version = 1 AND entropy_stream_version = 1
    ),
    CONSTRAINT ck_life_event_definition_order CHECK (event_order BETWEEN 1 AND 32),
    CONSTRAINT ck_life_event_definition_key CHECK (
        event_key REGEXP '^[a-z][a-zA-Z0-9]{0,63}$'
        AND default_choice_key REGEXP '^[a-z][a-zA-Z0-9]{0,63}$'
        AND (
            exclusive_group_key IS NULL
            OR (
                exclusive_group_key IS NOT NULL
                AND exclusive_group_key REGEXP '^[a-z][a-zA-Z0-9]{0,63}$'
            )
        )
    ),
    CONSTRAINT ck_life_event_definition_name CHECK (
        CHAR_LENGTH(display_name) BETWEEN 1 AND 80
    ),
    CONSTRAINT ck_life_event_definition_provenance CHECK (
        purpose IN ('gameBalance', 'realPolicyReference')
        AND ranked_availability IN ('unrankedOnly', 'rankedAndUnranked')
    ),
    CONSTRAINT ck_life_event_definition_ast CHECK (
        COALESCE(
            JSON_TYPE(eligibility_ast) = 'OBJECT'
            AND JSON_LENGTH(eligibility_ast) = 3
            AND JSON_TYPE(JSON_EXTRACT(eligibility_ast, '$.version')) = 'INTEGER'
            AND JSON_UNQUOTE(JSON_EXTRACT(eligibility_ast, '$.version')) = '1'
            AND JSON_TYPE(JSON_EXTRACT(eligibility_ast, '$.kind')) = 'STRING'
            AND JSON_UNQUOTE(JSON_EXTRACT(eligibility_ast, '$.kind')) = 'all'
            AND JSON_TYPE(JSON_EXTRACT(eligibility_ast, '$.children')) = 'ARRAY'
            AND JSON_LENGTH(JSON_EXTRACT(eligibility_ast, '$.children'))
                BETWEEN 1 AND 32,
            FALSE
        ) = TRUE
        AND ast_node_count BETWEEN 1 AND 128
        AND ast_max_depth BETWEEN 1 AND 12
    ),
    CONSTRAINT ck_life_event_definition_probability CHECK (hazard_ppm <= 1000000),
    CONSTRAINT ck_life_event_definition_limits CHECK (
        cooldown_game_days <= 3660
        AND maximum_occurrences BETWEEN 1 AND 255
        AND offer_duration_game_days BETWEEN 1 AND 366
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE life_event_choice (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    life_component_version_id       BIGINT UNSIGNED NOT NULL,
    life_event_definition_id        BIGINT UNSIGNED NOT NULL,
    choice_order                    TINYINT UNSIGNED NOT NULL,
    choice_key                      VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                    VARCHAR(120) NOT NULL,
    decision_kind                   VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    effect_kind                     VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    effect_amount_krw               BIGINT NULL,
    effect_account_code             VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NULL,
    effect_ast                      JSON NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_life_event_choice_order
        (life_event_definition_id, choice_order),
    UNIQUE KEY uk_life_event_choice_key
        (life_event_definition_id, choice_key),
    UNIQUE KEY uk_life_event_choice_definition_id
        (life_event_definition_id, id),
    UNIQUE KEY uk_life_event_choice_component_definition_id
        (life_component_version_id, life_event_definition_id, id),
    CONSTRAINT fk_life_event_choice_definition
        FOREIGN KEY (life_component_version_id, life_event_definition_id)
        REFERENCES life_event_definition (life_component_version_id, id),
    CONSTRAINT ck_life_event_choice_order CHECK (choice_order BETWEEN 1 AND 8),
    CONSTRAINT ck_life_event_choice_key CHECK (
        choice_key REGEXP '^[a-z][a-zA-Z0-9]{0,63}$'
    ),
    CONSTRAINT ck_life_event_choice_name CHECK (
        CHAR_LENGTH(display_name) BETWEEN 1 AND 120
    ),
    CONSTRAINT ck_life_event_choice_decision CHECK (
        decision_kind IN ('accepted', 'declined')
    ),
    CONSTRAINT ck_life_event_choice_effect CHECK (
        COALESCE(
            JSON_TYPE(effect_ast) = 'OBJECT'
            AND JSON_TYPE(JSON_EXTRACT(effect_ast, '$.version')) = 'INTEGER'
            AND JSON_UNQUOTE(JSON_EXTRACT(effect_ast, '$.version')) = '1'
            AND JSON_TYPE(JSON_EXTRACT(effect_ast, '$.kind')) = 'STRING'
            AND BINARY JSON_UNQUOTE(JSON_EXTRACT(effect_ast, '$.kind'))
                = BINARY effect_kind
            AND (
                (
                    effect_kind = 'noEffect'
                    AND effect_amount_krw IS NULL
                    AND effect_account_code IS NULL
                    AND JSON_LENGTH(effect_ast) = 2
                )
                OR (
                    effect_kind = 'fixedWalletExpense'
                    AND effect_amount_krw IS NOT NULL
                    AND effect_amount_krw BETWEEN 1 AND 9007199254740991
                    AND effect_account_code IS NOT NULL
                    AND effect_account_code = 'lifeEventExpense'
                    AND JSON_LENGTH(effect_ast) = 4
                    AND JSON_TYPE(JSON_EXTRACT(effect_ast, '$.amountKrw')) = 'INTEGER'
                    AND JSON_UNQUOTE(JSON_EXTRACT(effect_ast, '$.amountKrw'))
                        = CAST(effect_amount_krw AS CHAR)
                    AND JSON_TYPE(JSON_EXTRACT(effect_ast, '$.accountCode')) = 'STRING'
                    AND BINARY JSON_UNQUOTE(JSON_EXTRACT(effect_ast, '$.accountCode'))
                        = BINARY effect_account_code
                )
            ),
            FALSE
        ) = TRUE
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE life_event_month_plan (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    life_catalog_set_id             BIGINT UNSIGNED NOT NULL,
    life_event_component_version_id BIGINT UNSIGNED NOT NULL,
    `year_month`                    CHAR(7) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    target_game_day                 INT UNSIGNED NOT NULL,
    authority_state_revision        BIGINT UNSIGNED NOT NULL,
    fact_registry_schema_version    SMALLINT UNSIGNED NOT NULL,
    entropy_stream_version          SMALLINT UNSIGNED NOT NULL,
    definition_count                TINYINT UNSIGNED NOT NULL,
    offered_count                   TINYINT UNSIGNED NOT NULL,
    status                          VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_life_event_month_plan_period
        (save_id, run_revision, life_event_component_version_id, `year_month`),
    UNIQUE KEY uk_life_event_month_plan_save_run_id
        (save_id, run_revision, id),
    UNIQUE KEY uk_life_event_month_plan_run_component_id
        (save_id, run_revision, life_event_component_version_id, id),
    CONSTRAINT fk_life_event_month_plan_rule_bundle
        FOREIGN KEY (save_id, run_revision, life_catalog_set_id)
        REFERENCES run_rule_bundle (save_id, run_revision, life_catalog_set_id),
    CONSTRAINT fk_life_event_month_plan_catalog_component
        FOREIGN KEY (life_catalog_set_id, life_event_component_version_id)
        REFERENCES life_catalog_set (id, life_event_component_version_id),
    CONSTRAINT ck_life_event_month_plan_period CHECK (
        `year_month` REGEXP '^[0-9]{4}-(0[1-9]|1[0-2])$'
    ),
    CONSTRAINT ck_life_event_month_plan_contract CHECK (
        fact_registry_schema_version = 1
        AND entropy_stream_version = 1
        AND definition_count BETWEEN 1 AND 32
        AND offered_count <= 8
        AND offered_count <= definition_count
        AND status IN ('planning', 'completed')
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE life_event_candidate (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    month_plan_id                   BIGINT UNSIGNED NOT NULL,
    life_event_component_version_id BIGINT UNSIGNED NOT NULL,
    life_event_definition_id        BIGINT UNSIGNED NOT NULL,
    candidate_order                 TINYINT UNSIGNED NOT NULL,
    occurrence_no                   SMALLINT UNSIGNED NOT NULL,
    eligibility_fact_fingerprint    CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    candidate_result                VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    unknown_reason                  VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NULL,
    roll_ppm                        INT UNSIGNED NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_life_event_candidate_definition
        (save_id, run_revision, month_plan_id, life_event_definition_id),
    UNIQUE KEY uk_life_event_candidate_order
        (save_id, run_revision, month_plan_id, candidate_order),
    UNIQUE KEY uk_life_event_candidate_save_run_id
        (save_id, run_revision, id),
    UNIQUE KEY uk_life_event_candidate_plan_definition_id
        (save_id, run_revision, month_plan_id, life_event_definition_id, id),
    CONSTRAINT fk_life_event_candidate_plan
        FOREIGN KEY (
            save_id, run_revision, life_event_component_version_id, month_plan_id
        ) REFERENCES life_event_month_plan (
            save_id, run_revision, life_event_component_version_id, id
        ),
    CONSTRAINT fk_life_event_candidate_definition
        FOREIGN KEY (life_event_component_version_id, life_event_definition_id)
        REFERENCES life_event_definition (life_component_version_id, id),
    CONSTRAINT ck_life_event_candidate_order CHECK (candidate_order BETWEEN 1 AND 32),
    CONSTRAINT ck_life_event_candidate_occurrence CHECK (occurrence_no BETWEEN 1 AND 256),
    CONSTRAINT ck_life_event_candidate_fingerprint CHECK (
        eligibility_fact_fingerprint REGEXP '^[0-9a-f]{64}$'
    ),
    CONSTRAINT ck_life_event_candidate_result CHECK (
        COALESCE(
            (
                candidate_result = 'ineligible'
                AND unknown_reason IS NULL
                AND roll_ppm IS NULL
            )
            OR (
                candidate_result = 'indeterminate'
                AND unknown_reason IS NOT NULL
                AND unknown_reason IN (
                    'authorityMissing', 'collectionLimitExceeded', 'arithmeticOverflow'
                )
                AND roll_ppm IS NULL
            )
            OR (
                candidate_result IN ('notSelected', 'suppressed', 'offered')
                AND unknown_reason IS NULL
                AND roll_ppm IS NOT NULL
                AND roll_ppm BETWEEN 0 AND 999999
            ),
            FALSE
        ) = TRUE
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE life_event_instance (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    life_catalog_set_id             BIGINT UNSIGNED NOT NULL,
    life_event_component_version_id BIGINT UNSIGNED NOT NULL,
    life_event_definition_id        BIGINT UNSIGNED NOT NULL,
    month_plan_id                   BIGINT UNSIGNED NOT NULL,
    candidate_id                    BIGINT UNSIGNED NOT NULL,
    occurrence_no                   SMALLINT UNSIGNED NOT NULL,
    offered_game_day                INT UNSIGNED NOT NULL,
    expires_game_day                INT UNSIGNED NOT NULL,
    status                          VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    resolution_kind                 VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL,
    resolved_choice_id              BIGINT UNSIGNED NULL,
    resolution_command_id           CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    resolution_sequence             TINYINT UNSIGNED NULL,
    resolved_game_day               INT UNSIGNED NULL,
    ledger_transaction_id           BIGINT UNSIGNED NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_life_event_instance_occurrence
        (save_id, run_revision, life_event_definition_id, occurrence_no),
    UNIQUE KEY uk_life_event_instance_candidate
        (save_id, run_revision, candidate_id),
    UNIQUE KEY uk_life_event_instance_plan_definition
        (save_id, run_revision, month_plan_id, life_event_definition_id),
    UNIQUE KEY uk_life_event_instance_save_run_id
        (save_id, run_revision, id),
    UNIQUE KEY uk_life_event_instance_resolution_command
        (save_id, resolution_command_id),
    UNIQUE KEY uk_life_event_instance_ledger
        (save_id, run_revision, ledger_transaction_id),
    KEY ix_life_event_instance_pending
        (save_id, run_revision, status, id),
    KEY ix_life_event_instance_history
        (save_id, run_revision, resolved_game_day, id),
    CONSTRAINT fk_life_event_instance_rule_bundle
        FOREIGN KEY (save_id, run_revision, life_catalog_set_id)
        REFERENCES run_rule_bundle (save_id, run_revision, life_catalog_set_id),
    CONSTRAINT fk_life_event_instance_catalog_component
        FOREIGN KEY (life_catalog_set_id, life_event_component_version_id)
        REFERENCES life_catalog_set (id, life_event_component_version_id),
    CONSTRAINT fk_life_event_instance_definition
        FOREIGN KEY (life_event_component_version_id, life_event_definition_id)
        REFERENCES life_event_definition (life_component_version_id, id),
    CONSTRAINT fk_life_event_instance_candidate
        FOREIGN KEY (
            save_id, run_revision, month_plan_id, life_event_definition_id, candidate_id
        ) REFERENCES life_event_candidate (
            save_id, run_revision, month_plan_id, life_event_definition_id, id
        ),
    CONSTRAINT fk_life_event_instance_choice
        FOREIGN KEY (
            life_event_component_version_id, life_event_definition_id, resolved_choice_id
        ) REFERENCES life_event_choice (
            life_component_version_id, life_event_definition_id, id
        ),
    CONSTRAINT fk_life_event_instance_command
        FOREIGN KEY (save_id, resolution_command_id)
        REFERENCES command_identity (save_id, command_id),
    CONSTRAINT fk_life_event_instance_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT ck_life_event_instance_period CHECK (
        occurrence_no BETWEEN 1 AND 255
        AND expires_game_day > offered_game_day
    ),
    CONSTRAINT ck_life_event_instance_state CHECK (
        COALESCE(
            (
                status = 'offered'
                AND resolution_kind IS NULL
                AND resolved_choice_id IS NULL
                AND resolution_command_id IS NULL
                AND resolution_sequence IS NULL
                AND resolved_game_day IS NULL
                AND ledger_transaction_id IS NULL
            )
            OR (
                status = 'resolved'
                AND resolution_kind IS NOT NULL
                AND resolution_kind IN ('accepted', 'declined', 'expired')
                AND resolved_choice_id IS NOT NULL
                AND resolution_sequence IS NOT NULL
                AND resolution_sequence = 1
                AND resolved_game_day IS NOT NULL
                AND (
                    (
                        resolution_kind IN ('accepted', 'declined')
                        AND resolution_command_id IS NOT NULL
                    )
                    OR (
                        resolution_kind = 'expired'
                        AND resolution_command_id IS NULL
                    )
                )
            ),
            FALSE
        ) = TRUE
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE life_event_transition (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    life_event_instance_id          BIGINT UNSIGNED NOT NULL,
    transition_no                   TINYINT UNSIGNED NOT NULL,
    from_status                     VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL,
    to_status                       VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    choice_id                       BIGINT UNSIGNED NULL,
    command_id                      CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    transition_game_day             INT UNSIGNED NOT NULL,
    transition_reason               VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_life_event_transition_no
        (save_id, run_revision, life_event_instance_id, transition_no),
    UNIQUE KEY uk_life_event_transition_status
        (save_id, run_revision, life_event_instance_id, to_status),
    CONSTRAINT fk_life_event_transition_instance
        FOREIGN KEY (save_id, run_revision, life_event_instance_id)
        REFERENCES life_event_instance (save_id, run_revision, id),
    CONSTRAINT fk_life_event_transition_choice
        FOREIGN KEY (choice_id) REFERENCES life_event_choice (id),
    CONSTRAINT fk_life_event_transition_command
        FOREIGN KEY (save_id, command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT ck_life_event_transition_shape CHECK (
        COALESCE(
            (
                transition_no = 1
                AND from_status IS NULL
                AND to_status = 'offered'
                AND choice_id IS NULL
                AND command_id IS NULL
                AND transition_reason = 'monthlyPlanner'
            )
            OR (
                transition_no = 2
                AND from_status IS NOT NULL
                AND from_status = 'offered'
                AND to_status IN ('accepted', 'declined', 'expired')
                AND choice_id IS NOT NULL
                AND (
                    (
                        to_status IN ('accepted', 'declined')
                        AND command_id IS NOT NULL
                        AND transition_reason = 'playerChoice'
                    )
                    OR (
                        to_status = 'expired'
                        AND command_id IS NULL
                        AND transition_reason = 'offerExpired'
                    )
                )
            )
            OR (
                transition_no = 3
                AND from_status IS NOT NULL
                AND from_status IN ('accepted', 'declined', 'expired')
                AND to_status = 'resolved'
                AND choice_id IS NOT NULL
                AND transition_reason IN ('effectApplied', 'noEffectResolved')
                AND (
                    (
                        from_status IN ('accepted', 'declined')
                        AND command_id IS NOT NULL
                    )
                    OR (from_status = 'expired' AND command_id IS NULL)
                )
            ),
            FALSE
        ) = TRUE
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

-- Catalog children are append-only and may be added only before their component has a manifest.
CREATE TRIGGER tr_life_event_fact_draft_insert
BEFORE INSERT ON life_event_fact_definition
FOR EACH ROW
SET NEW.life_component_version_id = IF(
    EXISTS (
        SELECT 1
        FROM life_component_version AS component
        WHERE component.id = NEW.life_component_version_id
          AND component.component_kind = 'lifeEvent'
          AND component.availability = 'active'
          AND component.sealed_at IS NULL
          AND component.canonical_sha256 IS NULL
          AND NOT EXISTS (
              SELECT 1
              FROM life_component_canonical_manifest AS manifest
              WHERE manifest.life_component_version_id = component.id
          )
    ),
    NEW.life_component_version_id,
    NULL
);

CREATE TRIGGER tr_life_event_fact_no_update
BEFORE UPDATE ON life_event_fact_definition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'life-event facts are immutable';

CREATE TRIGGER tr_life_event_fact_no_delete
BEFORE DELETE ON life_event_fact_definition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'life-event facts are immutable';

CREATE TRIGGER tr_life_event_definition_draft_insert
BEFORE INSERT ON life_event_definition
FOR EACH ROW
SET NEW.life_component_version_id = IF(
    EXISTS (
        SELECT 1
        FROM life_component_version AS component
        WHERE component.id = NEW.life_component_version_id
          AND component.component_kind = 'lifeEvent'
          AND component.availability = 'active'
          AND component.sealed_at IS NULL
          AND component.canonical_sha256 IS NULL
          AND NOT EXISTS (
              SELECT 1
              FROM life_component_canonical_manifest AS manifest
              WHERE manifest.life_component_version_id = component.id
          )
    ),
    NEW.life_component_version_id,
    NULL
);

CREATE TRIGGER tr_life_event_definition_no_update
BEFORE UPDATE ON life_event_definition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'life-event definitions are immutable';

CREATE TRIGGER tr_life_event_definition_no_delete
BEFORE DELETE ON life_event_definition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'life-event definitions are immutable';

CREATE TRIGGER tr_life_event_choice_draft_insert
BEFORE INSERT ON life_event_choice
FOR EACH ROW
SET NEW.life_component_version_id = IF(
    EXISTS (
        SELECT 1
        FROM life_event_definition AS definition
        INNER JOIN life_component_version AS component
            ON component.id = definition.life_component_version_id
        WHERE definition.id = NEW.life_event_definition_id
          AND definition.life_component_version_id = NEW.life_component_version_id
          AND component.component_kind = 'lifeEvent'
          AND component.availability = 'active'
          AND component.sealed_at IS NULL
          AND component.canonical_sha256 IS NULL
          AND NOT EXISTS (
              SELECT 1
              FROM life_component_canonical_manifest AS manifest
              WHERE manifest.life_component_version_id = component.id
          )
    ),
    NEW.life_component_version_id,
    NULL
);

CREATE TRIGGER tr_life_event_choice_no_update
BEFORE UPDATE ON life_event_choice
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'life-event choices are immutable';

CREATE TRIGGER tr_life_event_choice_no_delete
BEFORE DELETE ON life_event_choice
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'life-event choices are immutable';

CREATE TRIGGER tr_life_event_month_plan_valid_insert
BEFORE INSERT ON life_event_month_plan
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'planning'
        AND EXISTS (
            SELECT 1
            FROM save
            INNER JOIN market_world AS world
                ON world.id = save.market_world_id
            INNER JOIN run_rule_bundle AS bundle
                ON bundle.save_id = save.id
               AND bundle.run_revision = save.run_revision
            INNER JOIN life_catalog_set AS catalog
                ON catalog.id = bundle.life_catalog_set_id
               AND catalog.life_event_component_version_id
                    = NEW.life_event_component_version_id
            INNER JOIN life_component_version AS component
                ON component.id = catalog.life_event_component_version_id
               AND component.component_kind = 'lifeEvent'
               AND component.availability = 'active'
               AND component.sealed_at IS NOT NULL
            WHERE save.id = NEW.save_id
              AND save.run_revision = NEW.run_revision
              AND bundle.life_catalog_set_id = NEW.life_catalog_set_id
              AND save.state_revision = NEW.authority_state_revision
              AND (
                  save.game_day = NEW.target_game_day
                  OR save.game_day + 1 = NEW.target_game_day
              )
              AND DAYOFMONTH(
                    DATE_ADD(world.start_date, INTERVAL NEW.target_game_day DAY)
                  ) = 1
              AND BINARY NEW.`year_month` = BINARY DATE_FORMAT(
                    DATE_ADD(world.start_date, INTERVAL NEW.target_game_day DAY),
                    '%Y-%m'
                  )
              AND NEW.definition_count = (
                  SELECT COUNT(*)
                  FROM life_event_definition AS definition
                  WHERE definition.life_component_version_id = component.id
              )
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_life_event_candidate_valid_insert
BEFORE INSERT ON life_event_candidate
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM life_event_month_plan AS plan
        INNER JOIN life_event_definition AS definition
            ON definition.id = NEW.life_event_definition_id
           AND definition.life_component_version_id
                = plan.life_event_component_version_id
        WHERE plan.id = NEW.month_plan_id
          AND plan.save_id = NEW.save_id
          AND plan.run_revision = NEW.run_revision
          AND plan.life_event_component_version_id
                = NEW.life_event_component_version_id
          AND plan.status = 'planning'
          AND definition.event_order = NEW.candidate_order
          AND NEW.occurrence_no = (
              SELECT COUNT(*) + 1
              FROM life_event_instance AS prior
              WHERE prior.save_id = NEW.save_id
                AND prior.run_revision = NEW.run_revision
                AND prior.life_event_definition_id = definition.id
          )
          AND (
              NEW.candidate_result IN ('ineligible', 'indeterminate')
              OR (
                  NEW.occurrence_no <= definition.maximum_occurrences
                  AND NOT EXISTS (
                      SELECT 1
                      FROM life_event_instance AS prior
                      WHERE prior.save_id = NEW.save_id
                        AND prior.run_revision = NEW.run_revision
                        AND prior.life_event_definition_id = definition.id
                        AND prior.offered_game_day + definition.cooldown_game_days
                            > plan.target_game_day
                  )
              )
          )
          AND (
              NEW.candidate_result IN ('ineligible', 'indeterminate')
              OR (
                  NEW.candidate_result = 'notSelected'
                  AND NEW.roll_ppm >= definition.hazard_ppm
              )
              OR (
                  NEW.candidate_result IN ('suppressed', 'offered')
                  AND NEW.roll_ppm < definition.hazard_ppm
              )
          )
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_life_event_candidate_no_update
BEFORE UPDATE ON life_event_candidate
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'life-event candidates are immutable';

CREATE TRIGGER tr_life_event_candidate_no_delete
BEFORE DELETE ON life_event_candidate
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'life-event candidates are immutable';

CREATE TRIGGER tr_life_event_instance_valid_insert
BEFORE INSERT ON life_event_instance
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'offered'
        AND EXISTS (
            SELECT 1
            FROM life_event_candidate AS candidate
            INNER JOIN life_event_month_plan AS plan
                ON plan.id = candidate.month_plan_id
               AND plan.save_id = candidate.save_id
               AND plan.run_revision = candidate.run_revision
            INNER JOIN life_event_definition AS definition
                ON definition.id = candidate.life_event_definition_id
               AND definition.life_component_version_id
                    = candidate.life_event_component_version_id
            WHERE candidate.id = NEW.candidate_id
              AND candidate.save_id = NEW.save_id
              AND candidate.run_revision = NEW.run_revision
              AND candidate.month_plan_id = NEW.month_plan_id
              AND candidate.life_event_component_version_id
                    = NEW.life_event_component_version_id
              AND candidate.life_event_definition_id
                    = NEW.life_event_definition_id
              AND candidate.candidate_result = 'offered'
              AND candidate.occurrence_no = NEW.occurrence_no
              AND plan.status = 'planning'
              AND plan.life_catalog_set_id = NEW.life_catalog_set_id
              AND plan.target_game_day = NEW.offered_game_day
              AND NEW.expires_game_day
                    = NEW.offered_game_day + definition.offer_duration_game_days
              AND NEW.occurrence_no <= definition.maximum_occurrences
              AND NEW.occurrence_no = (
                  SELECT COUNT(*) + 1
                  FROM life_event_instance AS prior
                  WHERE prior.save_id = NEW.save_id
                    AND prior.run_revision = NEW.run_revision
                    AND prior.life_event_definition_id = definition.id
              )
              AND NOT EXISTS (
                  SELECT 1
                  FROM life_event_instance AS prior
                  WHERE prior.save_id = NEW.save_id
                    AND prior.run_revision = NEW.run_revision
                    AND prior.life_event_definition_id = definition.id
                    AND prior.offered_game_day + definition.cooldown_game_days
                        > NEW.offered_game_day
              )
              AND (
                  SELECT COUNT(*)
                  FROM life_event_instance AS pending
                  WHERE pending.save_id = NEW.save_id
                    AND pending.run_revision = NEW.run_revision
                    AND pending.status = 'offered'
              ) < 8
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_life_event_transition_valid_insert
BEFORE INSERT ON life_event_transition
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM life_event_instance AS instance
        INNER JOIN life_event_definition AS definition
            ON definition.id = instance.life_event_definition_id
           AND definition.life_component_version_id
                = instance.life_event_component_version_id
        LEFT JOIN life_event_choice AS choice_row
            ON choice_row.id = NEW.choice_id
           AND choice_row.life_event_definition_id = definition.id
           AND choice_row.life_component_version_id
                = instance.life_event_component_version_id
        WHERE instance.id = NEW.life_event_instance_id
          AND instance.save_id = NEW.save_id
          AND instance.run_revision = NEW.run_revision
          AND instance.status = 'offered'
          AND (
              (
                  NEW.transition_no = 1
                  AND NEW.transition_game_day = instance.offered_game_day
              )
              OR (
                  NEW.transition_no = 2
                  AND choice_row.id IS NOT NULL
                  AND EXISTS (
                      SELECT 1
                      FROM life_event_transition AS offered_transition
                      WHERE offered_transition.save_id = instance.save_id
                        AND offered_transition.run_revision = instance.run_revision
                        AND offered_transition.life_event_instance_id = instance.id
                        AND offered_transition.transition_no = 1
                        AND offered_transition.to_status = 'offered'
                  )
                  AND (
                      (
                          NEW.to_status IN ('accepted', 'declined')
                          AND choice_row.decision_kind = NEW.to_status
                          AND NEW.transition_game_day < instance.expires_game_day
                          AND EXISTS (
                              SELECT 1
                              FROM save
                              INNER JOIN command_identity AS identity
                                  ON identity.save_id = save.id
                                 AND BINARY identity.command_id
                                      = BINARY NEW.command_id
                                 AND identity.command_kind = 'resolveLifeEvent'
                              WHERE save.id = instance.save_id
                                AND save.run_revision = instance.run_revision
                                AND save.game_day = NEW.transition_game_day
                                AND identity.initial_run_revision
                                      = instance.run_revision
                                AND identity.initial_state_revision
                                      = save.state_revision
                                AND identity.initial_game_day
                                      = NEW.transition_game_day
                          )
                      )
                      OR (
                          NEW.to_status = 'expired'
                          AND BINARY choice_row.choice_key
                                = BINARY definition.default_choice_key
                          AND choice_row.effect_kind = 'noEffect'
                          AND NEW.transition_game_day = instance.expires_game_day
                          AND EXISTS (
                              SELECT 1
                              FROM save
                              WHERE save.id = instance.save_id
                                AND save.run_revision = instance.run_revision
                                AND (
                                    save.game_day = NEW.transition_game_day
                                    OR save.game_day + 1 = NEW.transition_game_day
                                )
                          )
                      )
                  )
              )
              OR (
                  NEW.transition_no = 3
                  AND choice_row.id IS NOT NULL
                  AND EXISTS (
                      SELECT 1
                      FROM life_event_transition AS decision_transition
                      WHERE decision_transition.save_id = instance.save_id
                        AND decision_transition.run_revision = instance.run_revision
                        AND decision_transition.life_event_instance_id = instance.id
                        AND decision_transition.transition_no = 2
                        AND decision_transition.to_status = NEW.from_status
                        AND decision_transition.choice_id = NEW.choice_id
                        AND decision_transition.command_id <=> NEW.command_id
                        AND decision_transition.transition_game_day
                              = NEW.transition_game_day
                  )
                  AND (
                      (
                          choice_row.effect_kind = 'fixedWalletExpense'
                          AND NEW.transition_reason = 'effectApplied'
                          AND EXISTS (
                              SELECT 1
                              FROM ledger_transaction AS ledger
                              WHERE ledger.save_id = instance.save_id
                                AND ledger.run_revision = instance.run_revision
                                AND ledger.game_day = NEW.transition_game_day
                                AND ledger.source_kind = 'lifeEventChoice'
                                AND BINARY ledger.source_id
                                    = BINARY CAST(instance.id AS CHAR)
                                AND (
                                    SELECT COUNT(*)
                                    FROM ledger_posting AS posting
                                    WHERE posting.ledger_transaction_id = ledger.id
                                      AND posting.save_id = ledger.save_id
                                      AND posting.run_revision = ledger.run_revision
                                ) = 2
                                AND EXISTS (
                                    SELECT 1
                                    FROM ledger_posting AS posting
                                    WHERE posting.ledger_transaction_id = ledger.id
                                      AND posting.save_id = ledger.save_id
                                      AND posting.run_revision = ledger.run_revision
                                      AND posting.posting_order = 1
                                      AND posting.account_code = 'lifeEventExpense'
                                      AND posting.amount_krw
                                            = choice_row.effect_amount_krw
                                )
                                AND EXISTS (
                                    SELECT 1
                                    FROM ledger_posting AS posting
                                    WHERE posting.ledger_transaction_id = ledger.id
                                      AND posting.save_id = ledger.save_id
                                      AND posting.run_revision = ledger.run_revision
                                      AND posting.posting_order = 2
                                      AND posting.account_code = 'wallet'
                                      AND posting.amount_krw
                                            = -choice_row.effect_amount_krw
                                )
                          )
                      )
                      OR (
                          choice_row.effect_kind = 'noEffect'
                          AND NEW.transition_reason = 'noEffectResolved'
                          AND NOT EXISTS (
                              SELECT 1
                              FROM ledger_transaction AS ledger
                              WHERE ledger.save_id = instance.save_id
                                AND ledger.run_revision = instance.run_revision
                                AND ledger.source_kind = 'lifeEventChoice'
                                AND BINARY ledger.source_id
                                    = BINARY CAST(instance.id AS CHAR)
                          )
                      )
                  )
              )
          )
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_life_event_transition_no_update
BEFORE UPDATE ON life_event_transition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'life-event transitions are immutable';

CREATE TRIGGER tr_life_event_transition_no_delete
BEFORE DELETE ON life_event_transition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'life-event transitions are immutable';

CREATE TRIGGER tr_life_event_instance_resolve_only
BEFORE UPDATE ON life_event_instance
FOR EACH ROW
SET NEW.status = IF(
    OLD.status = 'offered'
        AND NEW.status = 'resolved'
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.life_catalog_set_id = OLD.life_catalog_set_id
        AND NEW.life_event_component_version_id
            = OLD.life_event_component_version_id
        AND NEW.life_event_definition_id = OLD.life_event_definition_id
        AND NEW.month_plan_id = OLD.month_plan_id
        AND NEW.candidate_id = OLD.candidate_id
        AND NEW.occurrence_no = OLD.occurrence_no
        AND NEW.offered_game_day = OLD.offered_game_day
        AND NEW.expires_game_day = OLD.expires_game_day
        AND NEW.created_at = OLD.created_at
        AND NEW.resolution_sequence = 1
        AND EXISTS (
            SELECT 1
            FROM life_event_transition AS decision_transition
            INNER JOIN life_event_transition AS resolved_transition
                ON resolved_transition.save_id = decision_transition.save_id
               AND resolved_transition.run_revision = decision_transition.run_revision
               AND resolved_transition.life_event_instance_id
                    = decision_transition.life_event_instance_id
               AND resolved_transition.transition_no = 3
               AND resolved_transition.from_status = decision_transition.to_status
               AND resolved_transition.to_status = 'resolved'
               AND resolved_transition.choice_id = decision_transition.choice_id
               AND resolved_transition.command_id <=> decision_transition.command_id
               AND resolved_transition.transition_game_day
                    = decision_transition.transition_game_day
            INNER JOIN life_event_choice AS choice_row
                ON choice_row.id = decision_transition.choice_id
               AND choice_row.life_event_definition_id
                    = OLD.life_event_definition_id
               AND choice_row.life_component_version_id
                    = OLD.life_event_component_version_id
            WHERE decision_transition.save_id = OLD.save_id
              AND decision_transition.run_revision = OLD.run_revision
              AND decision_transition.life_event_instance_id = OLD.id
              AND decision_transition.transition_no = 2
              AND decision_transition.to_status = NEW.resolution_kind
              AND decision_transition.choice_id = NEW.resolved_choice_id
              AND decision_transition.command_id <=> NEW.resolution_command_id
              AND decision_transition.transition_game_day = NEW.resolved_game_day
              AND (
                  (
                      choice_row.effect_kind = 'noEffect'
                      AND NEW.ledger_transaction_id IS NULL
                  )
                  OR (
                      choice_row.effect_kind = 'fixedWalletExpense'
                      AND EXISTS (
                          SELECT 1
                          FROM ledger_transaction AS ledger
                          WHERE ledger.id = NEW.ledger_transaction_id
                            AND ledger.save_id = OLD.save_id
                            AND ledger.run_revision = OLD.run_revision
                            AND ledger.game_day = NEW.resolved_game_day
                            AND ledger.source_kind = 'lifeEventChoice'
                            AND BINARY ledger.source_id = BINARY CAST(OLD.id AS CHAR)
                            AND (
                                SELECT COUNT(*)
                                FROM ledger_posting AS posting
                                WHERE posting.ledger_transaction_id = ledger.id
                                  AND posting.save_id = ledger.save_id
                                  AND posting.run_revision = ledger.run_revision
                            ) = 2
                            AND EXISTS (
                                SELECT 1
                                FROM ledger_posting AS posting
                                WHERE posting.ledger_transaction_id = ledger.id
                                  AND posting.save_id = ledger.save_id
                                  AND posting.run_revision = ledger.run_revision
                                  AND posting.posting_order = 1
                                  AND posting.account_code = 'lifeEventExpense'
                                  AND posting.amount_krw = choice_row.effect_amount_krw
                            )
                            AND EXISTS (
                                SELECT 1
                                FROM ledger_posting AS posting
                                WHERE posting.ledger_transaction_id = ledger.id
                                  AND posting.save_id = ledger.save_id
                                  AND posting.run_revision = ledger.run_revision
                                  AND posting.posting_order = 2
                                  AND posting.account_code = 'wallet'
                                  AND posting.amount_krw = -choice_row.effect_amount_krw
                            )
                      )
                  )
              )
        ),
    NEW.status,
    NULL
);

CREATE TRIGGER tr_life_event_instance_no_delete
BEFORE DELETE ON life_event_instance
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'life-event instances are immutable history';

CREATE TRIGGER tr_life_event_month_plan_complete_only
BEFORE UPDATE ON life_event_month_plan
FOR EACH ROW
SET NEW.status = IF(
    OLD.status = 'planning'
        AND NEW.status = 'completed'
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.life_catalog_set_id = OLD.life_catalog_set_id
        AND NEW.life_event_component_version_id
            = OLD.life_event_component_version_id
        AND BINARY NEW.`year_month` = BINARY OLD.`year_month`
        AND NEW.target_game_day = OLD.target_game_day
        AND NEW.authority_state_revision = OLD.authority_state_revision
        AND NEW.fact_registry_schema_version = OLD.fact_registry_schema_version
        AND NEW.entropy_stream_version = OLD.entropy_stream_version
        AND NEW.definition_count = OLD.definition_count
        AND NEW.offered_count = OLD.offered_count
        AND NEW.created_at = OLD.created_at
        AND (
            SELECT COUNT(*)
            FROM life_event_candidate AS candidate
            WHERE candidate.save_id = OLD.save_id
              AND candidate.run_revision = OLD.run_revision
              AND candidate.month_plan_id = OLD.id
        ) = OLD.definition_count
        AND (
            SELECT COUNT(*)
            FROM life_event_candidate AS candidate
            WHERE candidate.save_id = OLD.save_id
              AND candidate.run_revision = OLD.run_revision
              AND candidate.month_plan_id = OLD.id
              AND candidate.candidate_result = 'offered'
        ) = OLD.offered_count
        AND (
            SELECT COUNT(*)
            FROM life_event_instance AS instance
            WHERE instance.save_id = OLD.save_id
              AND instance.run_revision = OLD.run_revision
              AND instance.month_plan_id = OLD.id
        ) = OLD.offered_count
        AND NOT EXISTS (
            SELECT 1
            FROM life_event_candidate AS candidate
            WHERE candidate.save_id = OLD.save_id
              AND candidate.run_revision = OLD.run_revision
              AND candidate.month_plan_id = OLD.id
              AND (
                  (
                      candidate.candidate_result = 'offered'
                      AND NOT EXISTS (
                          SELECT 1
                          FROM life_event_instance AS instance
                          WHERE instance.save_id = candidate.save_id
                            AND instance.run_revision = candidate.run_revision
                            AND instance.candidate_id = candidate.id
                            AND EXISTS (
                                SELECT 1
                                FROM life_event_transition AS transition_row
                                WHERE transition_row.save_id = instance.save_id
                                  AND transition_row.run_revision = instance.run_revision
                                  AND transition_row.life_event_instance_id = instance.id
                                  AND transition_row.transition_no = 1
                                  AND transition_row.to_status = 'offered'
                            )
                      )
                  )
                  OR (
                      candidate.candidate_result <> 'offered'
                      AND EXISTS (
                          SELECT 1
                          FROM life_event_instance AS instance
                          WHERE instance.save_id = candidate.save_id
                            AND instance.run_revision = candidate.run_revision
                            AND instance.candidate_id = candidate.id
                      )
                  )
              )
        )
        AND NOT EXISTS (
            SELECT 1
            FROM life_event_candidate AS candidate
            INNER JOIN life_event_definition AS definition
                ON definition.id = candidate.life_event_definition_id
            WHERE candidate.save_id = OLD.save_id
              AND candidate.run_revision = OLD.run_revision
              AND candidate.month_plan_id = OLD.id
              AND candidate.candidate_result = 'suppressed'
              AND (
                  definition.exclusive_group_key IS NULL
                  OR NOT EXISTS (
                      SELECT 1
                      FROM life_event_candidate AS winner
                      INNER JOIN life_event_definition AS winner_definition
                          ON winner_definition.id = winner.life_event_definition_id
                      WHERE winner.save_id = candidate.save_id
                        AND winner.run_revision = candidate.run_revision
                        AND winner.month_plan_id = candidate.month_plan_id
                        AND winner.candidate_result = 'offered'
                        AND BINARY winner_definition.exclusive_group_key
                            = BINARY definition.exclusive_group_key
                        AND (
                            winner_definition.priority < definition.priority
                            OR (
                                winner_definition.priority = definition.priority
                                AND BINARY winner_definition.event_key
                                    < BINARY definition.event_key
                            )
                        )
                  )
              )
        )
        AND NOT EXISTS (
            SELECT 1
            FROM life_event_candidate AS candidate
            INNER JOIN life_event_definition AS definition
                ON definition.id = candidate.life_event_definition_id
            WHERE candidate.save_id = OLD.save_id
              AND candidate.run_revision = OLD.run_revision
              AND candidate.month_plan_id = OLD.id
              AND candidate.candidate_result = 'offered'
              AND definition.exclusive_group_key IS NOT NULL
              AND EXISTS (
                  SELECT 1
                  FROM life_event_candidate AS earlier
                  INNER JOIN life_event_definition AS earlier_definition
                      ON earlier_definition.id = earlier.life_event_definition_id
                  WHERE earlier.save_id = candidate.save_id
                    AND earlier.run_revision = candidate.run_revision
                    AND earlier.month_plan_id = candidate.month_plan_id
                    AND earlier.candidate_result IN ('offered', 'suppressed')
                    AND earlier.id <> candidate.id
                    AND BINARY earlier_definition.exclusive_group_key
                        = BINARY definition.exclusive_group_key
                    AND (
                        earlier_definition.priority < definition.priority
                        OR (
                            earlier_definition.priority = definition.priority
                            AND BINARY earlier_definition.event_key
                                < BINARY definition.event_key
                        )
                    )
              )
        ),
    NEW.status,
    NULL
);

CREATE TRIGGER tr_life_event_month_plan_no_delete
BEFORE DELETE ON life_event_month_plan
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'life-event month plans are immutable history';

-- A life-event expense is a direct player choice, never a scheduled settlement or an
-- insurance-looking bridge. The instance projection later proves that both postings exist.
ALTER TABLE ledger_transaction
    ADD CONSTRAINT ck_ledger_transaction_life_event_source CHECK (
        source_kind NOT LIKE 'lifeEvent%'
        OR source_kind = 'lifeEventChoice'
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
            'welfareBenefitIncome', 'lifeEventExpense'
        )
    );

CREATE TRIGGER tr_ledger_transaction_life_event_source_insert
BEFORE INSERT ON ledger_transaction
FOR EACH ROW
FOLLOWS tr_ledger_transaction_welfare_source_insert
SET NEW.source_kind = IF(
    NEW.source_kind <> 'lifeEventChoice'
        OR (
            NEW.source_id REGEXP '^[1-9][0-9]{0,19}$'
            AND EXISTS (
                SELECT 1
                FROM life_event_instance AS instance
                INNER JOIN run_rule_bundle AS bundle
                    ON bundle.save_id = instance.save_id
                   AND bundle.run_revision = instance.run_revision
                INNER JOIN life_event_transition AS decision_transition
                    ON decision_transition.save_id = instance.save_id
                   AND decision_transition.run_revision = instance.run_revision
                   AND decision_transition.life_event_instance_id = instance.id
                   AND decision_transition.transition_no = 2
                INNER JOIN life_event_choice AS choice_row
                    ON choice_row.id = decision_transition.choice_id
                   AND choice_row.life_event_definition_id
                        = instance.life_event_definition_id
                   AND choice_row.life_component_version_id
                        = instance.life_event_component_version_id
                WHERE BINARY CAST(instance.id AS CHAR) = BINARY NEW.source_id
                  AND instance.save_id = NEW.save_id
                  AND instance.run_revision = NEW.run_revision
                  AND instance.status = 'offered'
                  AND decision_transition.to_status = 'accepted'
                  AND decision_transition.transition_game_day = NEW.game_day
                  AND choice_row.effect_kind = 'fixedWalletExpense'
                  AND choice_row.effect_amount_krw > 0
                  AND bundle.policy_set_id = NEW.policy_set_id
            )
        ),
    NEW.source_kind,
    NULL
);

CREATE TRIGGER tr_ledger_posting_life_event_reference_insert
BEFORE INSERT ON ledger_posting
FOR EACH ROW
FOLLOWS tr_ledger_posting_welfare_reference_insert
SET NEW.account_code = IF(
    (
        EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            INNER JOIN life_event_instance AS instance
                ON BINARY CAST(instance.id AS CHAR) = BINARY ledger.source_id
               AND instance.save_id = ledger.save_id
               AND instance.run_revision = ledger.run_revision
            INNER JOIN life_event_transition AS decision_transition
                ON decision_transition.save_id = instance.save_id
               AND decision_transition.run_revision = instance.run_revision
               AND decision_transition.life_event_instance_id = instance.id
               AND decision_transition.transition_no = 2
            INNER JOIN life_event_choice AS choice_row
                ON choice_row.id = decision_transition.choice_id
               AND choice_row.life_event_definition_id
                    = instance.life_event_definition_id
               AND choice_row.life_component_version_id
                    = instance.life_event_component_version_id
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'lifeEventChoice'
              AND instance.status = 'offered'
              AND decision_transition.to_status = 'accepted'
              AND choice_row.effect_kind = 'fixedWalletExpense'
              AND (
                  (
                      NEW.posting_order = 1
                      AND NEW.account_code = 'lifeEventExpense'
                      AND NEW.amount_krw = choice_row.effect_amount_krw
                  )
                  OR (
                      NEW.posting_order = 2
                      AND NEW.account_code = 'wallet'
                      AND NEW.amount_krw = -choice_row.effect_amount_krw
                  )
              )
        )
    )
    OR (
        NEW.account_code <> 'lifeEventExpense'
        AND NOT EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'lifeEventChoice'
        )
    ),
    NEW.account_code,
    NULL
);

INSERT INTO life_component_version
    (component_kind, version_key, availability, ranked_eligible)
VALUES
    ('lifeEvent', 'dev-unranked-m4-life-event-2026-v1', 'active', FALSE);

INSERT INTO life_event_fact_definition
    (
        life_component_version_id, fact_order, fact_key,
        value_type, unit, enum_schema_key, window_kind,
        source_schema_version, source_kind
    )
SELECT component.id,
       seed.fact_order,
       seed.fact_key,
       seed.value_type,
       seed.unit,
       seed.enum_schema_key,
       'currentGameDay',
       1,
       seed.source_kind
FROM life_component_version AS component
INNER JOIN (
    SELECT 1 AS fact_order, 'character.age' AS fact_key,
           'ageYears' AS value_type, 'years' AS unit,
           NULL AS enum_schema_key, 'gameDay' AS source_kind
    UNION ALL SELECT 2, 'household.dependentCount', 'count', 'count', NULL, 'household'
    UNION ALL SELECT 3, 'residence.exists', 'boolean', 'boolean', NULL, 'residence'
    UNION ALL SELECT 4, 'military.status', 'enum', 'enum', 'military', 'military'
) AS seed
    ON TRUE
WHERE component.component_kind = 'lifeEvent'
  AND component.version_key = 'dev-unranked-m4-life-event-2026-v1';

INSERT INTO life_event_definition
    (
        life_component_version_id,
        schema_version,
        entropy_stream_version,
        event_order,
        event_key,
        display_name,
        purpose,
        ranked_availability,
        eligibility_ast,
        ast_node_count,
        ast_max_depth,
        hazard_ppm,
        cooldown_game_days,
        maximum_occurrences,
        priority,
        exclusive_group_key,
        offer_duration_game_days,
        default_choice_key
    )
SELECT component.id,
       1,
       1,
       1,
       'fictionalDependentCareRequest',
       '가족 돌봄 요청',
       'gameBalance',
       'unrankedOnly',
       JSON_OBJECT(
           'version', 1,
           'kind', 'all',
           'children', JSON_ARRAY(
               JSON_OBJECT(
                   'kind', 'between',
                   'value', JSON_OBJECT(
                       'kind', 'fact',
                       'path', 'character.age',
                       'unit', 'years',
                       'window', JSON_OBJECT('kind', 'currentGameDay')
                   ),
                   'lower', JSON_OBJECT(
                       'kind', 'literal', 'valueType', 'ageYears',
                       'unit', 'years', 'value', 22
                   ),
                   'upper', JSON_OBJECT(
                       'kind', 'literal', 'valueType', 'ageYears',
                       'unit', 'years', 'value', 67
                   )
               ),
               JSON_OBJECT(
                   'kind', 'gte',
                   'left', JSON_OBJECT(
                       'kind', 'fact',
                       'path', 'household.dependentCount',
                       'unit', 'count',
                       'window', JSON_OBJECT('kind', 'currentGameDay')
                   ),
                   'right', JSON_OBJECT(
                       'kind', 'literal', 'valueType', 'count',
                       'unit', 'count', 'value', 1
                   )
               ),
               JSON_OBJECT(
                   'kind', 'fact',
                   'path', 'residence.exists',
                   'unit', 'boolean',
                   'window', JSON_OBJECT('kind', 'currentGameDay')
               ),
               JSON_OBJECT(
                   'kind', 'not',
                   'child', JSON_OBJECT(
                       'kind', 'eq',
                       'left', JSON_OBJECT(
                           'kind', 'fact',
                           'path', 'military.status',
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
           )
       ),
       13,
       4,
       1000000,
       365,
       1,
       100,
       'familyCare',
       7,
       'decline'
FROM life_component_version AS component
WHERE component.component_kind = 'lifeEvent'
  AND component.version_key = 'dev-unranked-m4-life-event-2026-v1';

INSERT INTO life_event_choice
    (
        life_component_version_id,
        life_event_definition_id,
        choice_order,
        choice_key,
        display_name,
        decision_kind,
        effect_kind,
        effect_amount_krw,
        effect_account_code,
        effect_ast
    )
SELECT definition.life_component_version_id,
       definition.id,
       1,
       'supportNow',
       '지금 돕는다',
       'accepted',
       'fixedWalletExpense',
       120000,
       'lifeEventExpense',
       JSON_OBJECT(
           'version', 1,
           'kind', 'fixedWalletExpense',
           'amountKrw', 120000,
           'accountCode', 'lifeEventExpense'
       )
FROM life_event_definition AS definition
WHERE definition.event_key = 'fictionalDependentCareRequest'
  AND definition.life_component_version_id = (
      SELECT id
      FROM life_component_version
      WHERE component_kind = 'lifeEvent'
        AND version_key = 'dev-unranked-m4-life-event-2026-v1'
  );

INSERT INTO life_event_choice
    (
        life_component_version_id,
        life_event_definition_id,
        choice_order,
        choice_key,
        display_name,
        decision_kind,
        effect_kind,
        effect_amount_krw,
        effect_account_code,
        effect_ast
    )
SELECT definition.life_component_version_id,
       definition.id,
       2,
       'decline',
       '이번에는 돕지 않는다',
       'declined',
       'noEffect',
       NULL,
       NULL,
       JSON_OBJECT('version', 1, 'kind', 'noEffect')
FROM life_event_definition AS definition
WHERE definition.event_key = 'fictionalDependentCareRequest'
  AND definition.life_component_version_id = (
      SELECT id
      FROM life_component_version
      WHERE component_kind = 'lifeEvent'
        AND version_key = 'dev-unranked-m4-life-event-2026-v1'
  );

-- Canonical child serializations are ordered independently of auto-increment IDs.
CREATE VIEW life_event_component_canonical_projection AS
SELECT component.id AS life_component_version_id,
       CAST(JSON_OBJECT(
           'availability', component.availability,
           'componentKind', component.component_kind,
           'definitionsCanonical', (
               SELECT GROUP_CONCAT(
                   CAST(JSON_OBJECT(
                       'astMaxDepth', definition.ast_max_depth,
                       'astNodeCount', definition.ast_node_count,
                       'choicesCanonical', (
                           SELECT GROUP_CONCAT(
                               CAST(JSON_OBJECT(
                                   'choiceKey', choice_row.choice_key,
                                   'choiceOrder', choice_row.choice_order,
                                   'decisionKind', choice_row.decision_kind,
                                   'displayName', choice_row.display_name,
                                   'effectAccountCode', choice_row.effect_account_code,
                                   'effectAmountKrw', choice_row.effect_amount_krw,
                                   'effectAst', choice_row.effect_ast,
                                   'effectKind', choice_row.effect_kind
                               ) AS CHAR CHARACTER SET utf8mb4)
                               ORDER BY choice_row.choice_order SEPARATOR '\n'
                           )
                           FROM life_event_choice AS choice_row
                           WHERE choice_row.life_event_definition_id = definition.id
                       ),
                       'cooldownGameDays', definition.cooldown_game_days,
                       'defaultChoiceKey', definition.default_choice_key,
                       'displayName', definition.display_name,
                       'eligibilityAst', definition.eligibility_ast,
                       'entropyStreamVersion', definition.entropy_stream_version,
                       'eventKey', definition.event_key,
                       'eventOrder', definition.event_order,
                       'exclusiveGroupKey', definition.exclusive_group_key,
                       'hazardPpm', definition.hazard_ppm,
                       'maximumOccurrences', definition.maximum_occurrences,
                       'offerDurationGameDays', definition.offer_duration_game_days,
                       'priority', definition.priority,
                       'purpose', definition.purpose,
                       'rankedAvailability', definition.ranked_availability,
                       'schemaVersion', definition.schema_version
                   ) AS CHAR CHARACTER SET utf8mb4)
                   ORDER BY definition.event_key SEPARATOR '\n'
               )
               FROM life_event_definition AS definition
               WHERE definition.life_component_version_id = component.id
           ),
           'factsCanonical', (
               SELECT GROUP_CONCAT(
                   CAST(JSON_OBJECT(
                       'enumSchemaKey', fact.enum_schema_key,
                       'factKey', fact.fact_key,
                       'factOrder', fact.fact_order,
                       'sourceKind', fact.source_kind,
                       'sourceSchemaVersion', fact.source_schema_version,
                       'unit', fact.unit,
                       'valueType', fact.value_type,
                       'windowKind', fact.window_kind
                   ) AS CHAR CHARACTER SET utf8mb4)
                   ORDER BY fact.fact_order SEPARATOR '\n'
               )
               FROM life_event_fact_definition AS fact
               WHERE fact.life_component_version_id = component.id
           ),
           'rankedEligible', component.ranked_eligible,
           'schemaVersion', 1,
           'versionKey', component.version_key
       ) AS CHAR CHARACTER SET utf8mb4) AS canonical_json
FROM life_component_version AS component
WHERE component.component_kind = 'lifeEvent'
  AND component.availability = 'active';

CREATE TRIGGER tr_life_component_version_life_event_publish
BEFORE UPDATE ON life_component_version
FOR EACH ROW
FOLLOWS tr_life_component_version_welfare_publish
SET NEW.version_key = IF(
    NEW.component_kind <> 'lifeEvent'
        OR NEW.availability <> 'active'
        OR (
            OLD.version_key = 'dev-unranked-m4-life-event-2026-v1'
            AND OLD.ranked_eligible = FALSE
            AND (
                SELECT COUNT(*)
                FROM life_event_fact_definition AS fact
                WHERE fact.life_component_version_id = OLD.id
            ) = 4
            AND (
                SELECT SUM(
                    CASE
                        WHEN fact.fact_order = 1
                             AND fact.fact_key = 'character.age'
                             AND fact.value_type = 'ageYears'
                             AND fact.unit = 'years'
                             AND fact.enum_schema_key IS NULL
                             AND fact.window_kind = 'currentGameDay'
                             AND fact.source_schema_version = 1
                             AND fact.source_kind = 'gameDay' THEN 1
                        WHEN fact.fact_order = 2
                             AND fact.fact_key = 'household.dependentCount'
                             AND fact.value_type = 'count'
                             AND fact.unit = 'count'
                             AND fact.enum_schema_key IS NULL
                             AND fact.window_kind = 'currentGameDay'
                             AND fact.source_schema_version = 1
                             AND fact.source_kind = 'household' THEN 1
                        WHEN fact.fact_order = 3
                             AND fact.fact_key = 'residence.exists'
                             AND fact.value_type = 'boolean'
                             AND fact.unit = 'boolean'
                             AND fact.enum_schema_key IS NULL
                             AND fact.window_kind = 'currentGameDay'
                             AND fact.source_schema_version = 1
                             AND fact.source_kind = 'residence' THEN 1
                        WHEN fact.fact_order = 4
                             AND fact.fact_key = 'military.status'
                             AND fact.value_type = 'enum'
                             AND fact.unit = 'enum'
                             AND fact.enum_schema_key = 'military'
                             AND fact.window_kind = 'currentGameDay'
                             AND fact.source_schema_version = 1
                             AND fact.source_kind = 'military' THEN 1
                        ELSE 0
                    END
                )
                FROM life_event_fact_definition AS fact
                WHERE fact.life_component_version_id = OLD.id
            ) = 4
            AND (
                SELECT COUNT(*)
                FROM life_event_definition AS definition
                WHERE definition.life_component_version_id = OLD.id
            ) BETWEEN 1 AND 32
            AND NOT EXISTS (
                SELECT 1
                FROM life_event_definition AS definition
                WHERE definition.life_component_version_id = OLD.id
                  AND (
                      definition.event_order > (
                          SELECT COUNT(*)
                          FROM life_event_definition AS sibling
                          WHERE sibling.life_component_version_id = OLD.id
                      )
                      OR (
                          SELECT COUNT(*)
                          FROM life_event_choice AS choice_row
                          WHERE choice_row.life_event_definition_id = definition.id
                      ) NOT BETWEEN 2 AND 8
                      OR EXISTS (
                          SELECT 1
                          FROM life_event_choice AS choice_row
                          WHERE choice_row.life_event_definition_id = definition.id
                            AND choice_row.choice_order > (
                                SELECT COUNT(*)
                                FROM life_event_choice AS sibling_choice
                                WHERE sibling_choice.life_event_definition_id = definition.id
                            )
                      )
                      OR NOT EXISTS (
                          SELECT 1
                          FROM life_event_choice AS default_choice
                          WHERE default_choice.life_event_definition_id = definition.id
                            AND BINARY default_choice.choice_key
                                = BINARY definition.default_choice_key
                            AND default_choice.effect_kind = 'noEffect'
                      )
                  )
            )
            AND EXISTS (
                SELECT 1
                FROM life_event_definition AS definition
                WHERE definition.life_component_version_id = OLD.id
                  AND definition.event_order = 1
                  AND definition.event_key = 'fictionalDependentCareRequest'
                  AND definition.display_name = '가족 돌봄 요청'
                  AND definition.purpose = 'gameBalance'
                  AND definition.ranked_availability = 'unrankedOnly'
                  AND definition.schema_version = 1
                  AND definition.entropy_stream_version = 1
                  AND definition.ast_node_count = 13
                  AND definition.ast_max_depth = 4
                  AND definition.hazard_ppm = 1000000
                  AND definition.cooldown_game_days = 365
                  AND definition.maximum_occurrences = 1
                  AND definition.priority = 100
                  AND definition.exclusive_group_key = 'familyCare'
                  AND definition.offer_duration_game_days = 7
                  AND definition.default_choice_key = 'decline'
                  AND JSON_LENGTH(definition.eligibility_ast) = 3
                  AND JSON_TYPE(
                        JSON_EXTRACT(definition.eligibility_ast, '$.children')
                      ) = 'ARRAY'
                  AND JSON_LENGTH(
                        JSON_EXTRACT(definition.eligibility_ast, '$.children')
                      ) = 4
                  AND JSON_UNQUOTE(JSON_EXTRACT(
                        definition.eligibility_ast, '$.children[0].kind'
                      )) = 'between'
                  AND JSON_UNQUOTE(JSON_EXTRACT(
                        definition.eligibility_ast, '$.children[0].value.path'
                      )) = 'character.age'
                  AND JSON_UNQUOTE(JSON_EXTRACT(
                        definition.eligibility_ast, '$.children[0].lower.value'
                      )) = '22'
                  AND JSON_UNQUOTE(JSON_EXTRACT(
                        definition.eligibility_ast, '$.children[0].upper.value'
                      )) = '67'
                  AND JSON_UNQUOTE(JSON_EXTRACT(
                        definition.eligibility_ast, '$.children[1].kind'
                      )) = 'gte'
                  AND JSON_UNQUOTE(JSON_EXTRACT(
                        definition.eligibility_ast, '$.children[1].left.path'
                      )) = 'household.dependentCount'
                  AND JSON_UNQUOTE(JSON_EXTRACT(
                        definition.eligibility_ast, '$.children[1].right.value'
                      )) = '1'
                  AND JSON_UNQUOTE(JSON_EXTRACT(
                        definition.eligibility_ast, '$.children[2].kind'
                      )) = 'fact'
                  AND JSON_UNQUOTE(JSON_EXTRACT(
                        definition.eligibility_ast, '$.children[2].path'
                      )) = 'residence.exists'
                  AND JSON_UNQUOTE(JSON_EXTRACT(
                        definition.eligibility_ast, '$.children[3].kind'
                      )) = 'not'
                  AND JSON_UNQUOTE(JSON_EXTRACT(
                        definition.eligibility_ast, '$.children[3].child.kind'
                      )) = 'eq'
                  AND JSON_UNQUOTE(JSON_EXTRACT(
                        definition.eligibility_ast, '$.children[3].child.left.path'
                      )) = 'military.status'
                  AND JSON_UNQUOTE(JSON_EXTRACT(
                        definition.eligibility_ast,
                        '$.children[3].child.right.schemaKey'
                      )) = 'military'
                  AND JSON_UNQUOTE(JSON_EXTRACT(
                        definition.eligibility_ast, '$.children[3].child.right.value'
                      )) = 'serving'
                  AND (
                      SELECT COUNT(*)
                      FROM life_event_choice AS choice_row
                      WHERE choice_row.life_event_definition_id = definition.id
                  ) = 2
                  AND EXISTS (
                      SELECT 1
                      FROM life_event_choice AS choice_row
                      WHERE choice_row.life_event_definition_id = definition.id
                        AND choice_row.choice_order = 1
                        AND choice_row.choice_key = 'supportNow'
                        AND choice_row.display_name = '지금 돕는다'
                        AND choice_row.decision_kind = 'accepted'
                        AND choice_row.effect_kind = 'fixedWalletExpense'
                        AND choice_row.effect_amount_krw = 120000
                        AND choice_row.effect_account_code = 'lifeEventExpense'
                        AND JSON_LENGTH(choice_row.effect_ast) = 4
                        AND JSON_UNQUOTE(JSON_EXTRACT(
                            choice_row.effect_ast, '$.amountKrw'
                        )) = '120000'
                  )
                  AND EXISTS (
                      SELECT 1
                      FROM life_event_choice AS choice_row
                      WHERE choice_row.life_event_definition_id = definition.id
                        AND choice_row.choice_order = 2
                        AND choice_row.choice_key = 'decline'
                        AND choice_row.display_name = '이번에는 돕지 않는다'
                        AND choice_row.decision_kind = 'declined'
                        AND choice_row.effect_kind = 'noEffect'
                        AND choice_row.effect_amount_krw IS NULL
                        AND choice_row.effect_account_code IS NULL
                        AND JSON_LENGTH(choice_row.effect_ast) = 2
                  )
            )
            AND (
                SELECT COUNT(*)
                FROM life_event_definition AS definition
                WHERE definition.life_component_version_id = OLD.id
            ) = 1
            AND EXISTS (
                SELECT 1
                FROM life_event_component_canonical_projection AS projection
                INNER JOIN life_component_canonical_manifest AS manifest
                    ON manifest.life_component_version_id
                        = projection.life_component_version_id
                   AND BINARY manifest.canonical_json = BINARY projection.canonical_json
                   AND BINARY manifest.canonical_sha256
                        = BINARY SHA2(projection.canonical_json, 256)
                WHERE projection.life_component_version_id = OLD.id
            )
        ),
    NEW.version_key,
    NULL
);

INSERT INTO life_component_canonical_manifest
    (life_component_version_id, canonical_json)
SELECT projection.life_component_version_id, projection.canonical_json
FROM life_event_component_canonical_projection AS projection
INNER JOIN life_component_version AS component
    ON component.id = projection.life_component_version_id
WHERE component.component_kind = 'lifeEvent'
  AND component.version_key = 'dev-unranked-m4-life-event-2026-v1'
  AND component.sealed_at IS NULL;

UPDATE life_component_version AS component
INNER JOIN life_component_canonical_manifest AS manifest
    ON manifest.life_component_version_id = component.id
SET component.canonical_sha256 = manifest.canonical_sha256,
    component.sealed_at = CURRENT_TIMESTAMP(3)
WHERE component.component_kind = 'lifeEvent'
  AND component.version_key = 'dev-unranked-m4-life-event-2026-v1'
  AND component.sealed_at IS NULL;

-- Clone the sealed D1 life aggregate and replace only its life-event component.
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
SELECT 'dev-unranked-m4-life-catalog-2026-v3',
       FALSE,
       previous.legacy_dependent_age_years,
       previous.living_cost_component_version_id,
       previous.welfare_component_version_id,
       life_event.id,
       previous.insurance_component_version_id,
       previous.corporation_component_version_id
FROM m4d2_previous_new_run_assignment AS previous
INNER JOIN life_catalog_set AS previous_catalog
    ON previous_catalog.id = previous.life_catalog_set_id
   AND previous_catalog.catalog_key = 'dev-unranked-m4-life-welfare-2026-v2'
   AND previous_catalog.sealed_at IS NOT NULL
INNER JOIN life_component_version AS welfare
    ON welfare.id = previous.welfare_component_version_id
   AND welfare.component_kind = 'welfare'
   AND welfare.version_key = 'dev-unranked-m4-welfare-2026-v1'
   AND welfare.availability = 'active'
   AND welfare.sealed_at IS NOT NULL
INNER JOIN life_component_version AS previous_life_event
    ON previous_life_event.id = previous.life_event_component_version_id
   AND previous_life_event.component_kind = 'lifeEvent'
   AND previous_life_event.version_key = 'disabled-m4a-v1'
   AND previous_life_event.availability = 'disabled'
   AND previous_life_event.sealed_at IS NOT NULL
INNER JOIN life_component_version AS insurance
    ON insurance.id = previous.insurance_component_version_id
   AND insurance.component_kind = 'insurance'
   AND insurance.version_key = 'disabled-m4a-v1'
   AND insurance.availability = 'disabled'
   AND insurance.sealed_at IS NOT NULL
INNER JOIN life_component_version AS corporation
    ON corporation.id = previous.corporation_component_version_id
   AND corporation.component_kind = 'corporation'
   AND corporation.version_key = 'disabled-m4a-v1'
   AND corporation.availability = 'disabled'
   AND corporation.sealed_at IS NOT NULL
INNER JOIN life_component_version AS life_event
    ON life_event.component_kind = 'lifeEvent'
   AND life_event.version_key = 'dev-unranked-m4-life-event-2026-v1'
   AND life_event.availability = 'active'
   AND life_event.ranked_eligible = FALSE
   AND life_event.sealed_at IS NOT NULL
WHERE previous.assignment_key = 'newRun';

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
WHERE catalog_key = 'dev-unranked-m4-life-catalog-2026-v3'
  AND sealed_at IS NULL;

CREATE TEMPORARY TABLE m4d2_publication_guard (
    guard_key VARCHAR(48) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    accepted  TINYINT UNSIGNED NOT NULL,
    CONSTRAINT ck_m4d2_publication_guard CHECK (accepted = 1)
) ENGINE = InnoDB;

INSERT INTO m4d2_publication_guard (guard_key, accepted)
SELECT 'sealed-life-event-v1', IF(
    EXISTS (
        SELECT 1
        FROM life_component_version AS component
        INNER JOIN life_component_canonical_manifest AS manifest
            ON manifest.life_component_version_id = component.id
        INNER JOIN life_event_component_canonical_projection AS projection
            ON projection.life_component_version_id = component.id
        INNER JOIN life_event_definition AS definition
            ON definition.life_component_version_id = component.id
        WHERE component.component_kind = 'lifeEvent'
          AND component.version_key = 'dev-unranked-m4-life-event-2026-v1'
          AND component.availability = 'active'
          AND component.ranked_eligible = FALSE
          AND component.sealed_at IS NOT NULL
          AND BINARY component.canonical_sha256 = BINARY manifest.canonical_sha256
          AND BINARY manifest.canonical_json = BINARY projection.canonical_json
          AND BINARY manifest.canonical_sha256
                = BINARY SHA2(projection.canonical_json, 256)
          AND definition.event_key = 'fictionalDependentCareRequest'
          AND definition.hazard_ppm = 1000000
          AND definition.maximum_occurrences = 1
          AND definition.offer_duration_game_days = 7
          AND (SELECT COUNT(*)
               FROM life_event_fact_definition AS fact
               WHERE fact.life_component_version_id = component.id) = 4
          AND (SELECT COUNT(*)
               FROM life_event_choice AS choice_row
               WHERE choice_row.life_event_definition_id = definition.id) = 2
    ),
    1,
    0
);

INSERT INTO m4d2_publication_guard (guard_key, accepted)
SELECT 'sealed-life-catalog-v3', IF(
    EXISTS (
        SELECT 1
        FROM life_catalog_set AS catalog
        INNER JOIN life_component_version AS life_event
            ON life_event.id = catalog.life_event_component_version_id
        INNER JOIN m4d2_previous_new_run_assignment AS previous
            ON previous.assignment_key = 'newRun'
           AND catalog.legacy_dependent_age_years
                = previous.legacy_dependent_age_years
           AND catalog.living_cost_component_version_id
                = previous.living_cost_component_version_id
           AND catalog.welfare_component_version_id
                = previous.welfare_component_version_id
           AND catalog.insurance_component_version_id
                = previous.insurance_component_version_id
           AND catalog.corporation_component_version_id
                = previous.corporation_component_version_id
        WHERE catalog.catalog_key = 'dev-unranked-m4-life-catalog-2026-v3'
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
          AND life_event.version_key = 'dev-unranked-m4-life-event-2026-v1'
          AND life_event.availability = 'active'
          AND life_event.sealed_at IS NOT NULL
    ),
    1,
    0
);

INSERT INTO m4d2_publication_guard (guard_key, accepted)
SELECT 'existing-run-pins-unchanged', IF(
    NOT EXISTS (
        SELECT 1
        FROM m4d2_existing_run_life_pins AS previous
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
            WHERE catalog.catalog_key = 'dev-unranked-m4-life-catalog-2026-v3'
        ),
    1,
    0
);

INSERT INTO m4d2_publication_guard (guard_key, accepted)
SELECT 'life-event-ledger-protocol', IF(
    EXISTS (
        SELECT 1
        FROM information_schema.CHECK_CONSTRAINTS AS constraint_row
        WHERE constraint_row.CONSTRAINT_SCHEMA = DATABASE()
          AND constraint_row.CONSTRAINT_NAME = 'ck_ledger_posting_account_code'
          AND constraint_row.CHECK_CLAUSE LIKE '%lifeEventExpense%'
    )
        AND EXISTS (
            SELECT 1
            FROM information_schema.CHECK_CONSTRAINTS AS constraint_row
            WHERE constraint_row.CONSTRAINT_SCHEMA = DATABASE()
              AND constraint_row.CONSTRAINT_NAME
                    = 'ck_ledger_transaction_life_event_source'
              AND constraint_row.CHECK_CLAUSE LIKE '%lifeEventChoice%'
        ),
    1,
    0
);

DROP TEMPORARY TABLE m4d2_publication_guard;

-- This is the only external publication mutation in 0039. The assignment trigger bumps its own
-- revision and revalidates all unchanged market, finance, career, employment, credit, and
-- real-estate pins.
UPDATE run_rule_bundle_assignment AS assignment
INNER JOIN life_catalog_set AS catalog
    ON catalog.catalog_key = 'dev-unranked-m4-life-catalog-2026-v3'
   AND catalog.sealed_at IS NOT NULL
SET assignment.life_catalog_set_id = catalog.id
WHERE assignment.assignment_key = 'newRun';

CREATE TEMPORARY TABLE m4d2_assignment_guard (
    guard_key VARCHAR(48) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    accepted  TINYINT UNSIGNED NOT NULL,
    CONSTRAINT ck_m4d2_assignment_guard CHECK (accepted = 1)
) ENGINE = InnoDB;

INSERT INTO m4d2_assignment_guard (guard_key, accepted)
SELECT 'new-run-life-event-v1-only', IF(
    EXISTS (
        SELECT 1
        FROM run_rule_bundle_assignment AS assignment
        INNER JOIN life_catalog_set AS catalog
            ON catalog.id = assignment.life_catalog_set_id
        INNER JOIN life_component_version AS life_event
            ON life_event.id = catalog.life_event_component_version_id
        INNER JOIN m4d2_previous_new_run_assignment AS previous
            ON previous.assignment_key = assignment.assignment_key
           AND assignment.market_world_id = previous.market_world_id
           AND assignment.policy_set_id = previous.policy_set_id
           AND assignment.career_catalog_bundle_id
                = previous.career_catalog_bundle_id
           AND assignment.employment_policy_set_id
                = previous.employment_policy_set_id
           AND assignment.credit_model_version_id
                = previous.credit_model_version_id
           AND assignment.real_estate_model_version_id
                = previous.real_estate_model_version_id
           AND assignment.market_assignment_revision
                = previous.market_assignment_revision
           AND assignment.finance_assignment_revision
                = previous.finance_assignment_revision
           AND assignment.career_assignment_revision
                = previous.career_assignment_revision
           AND assignment.employment_assignment_revision
                = previous.employment_assignment_revision
           AND assignment.assignment_revision = previous.assignment_revision + 1
           AND catalog.legacy_dependent_age_years
                = previous.legacy_dependent_age_years
           AND catalog.living_cost_component_version_id
                = previous.living_cost_component_version_id
           AND catalog.welfare_component_version_id
                = previous.welfare_component_version_id
           AND catalog.insurance_component_version_id
                = previous.insurance_component_version_id
           AND catalog.corporation_component_version_id
                = previous.corporation_component_version_id
        WHERE assignment.assignment_key = 'newRun'
          AND catalog.catalog_key = 'dev-unranked-m4-life-catalog-2026-v3'
          AND catalog.sealed_at IS NOT NULL
          AND life_event.version_key = 'dev-unranked-m4-life-event-2026-v1'
          AND life_event.availability = 'active'
          AND life_event.sealed_at IS NOT NULL
    )
        AND NOT EXISTS (
            SELECT 1
            FROM m4d2_existing_run_life_pins AS previous
            LEFT JOIN run_rule_bundle AS current_bundle
                ON current_bundle.save_id = previous.save_id
               AND current_bundle.run_revision = previous.run_revision
            WHERE current_bundle.save_id IS NULL
               OR current_bundle.life_catalog_set_id <> previous.life_catalog_set_id
        ),
    1,
    0
);

DROP TEMPORARY TABLE m4d2_assignment_guard;
DROP TEMPORARY TABLE m4d2_existing_run_life_pins;
DROP TEMPORARY TABLE m4d2_previous_new_run_assignment;
