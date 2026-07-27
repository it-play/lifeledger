-- M4-D3 deterministic insurance catalog, contracts, premiums, and claims (§7.8–§7.13).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;
SET SESSION group_concat_max_len = 1048576;

-- MySQL DDL auto-commits. Keep durable pins and the complete assignment so publication can prove
-- that existing runs did not move and that newRun changed only its life aggregate pointer.
CREATE TEMPORARY TABLE m4d3_existing_run_life_pins (
    save_id                 BIGINT UNSIGNED NOT NULL,
    run_revision            INT UNSIGNED NOT NULL,
    life_catalog_set_id     BIGINT UNSIGNED NOT NULL,
    PRIMARY KEY (save_id, run_revision)
) ENGINE = InnoDB;

INSERT INTO m4d3_existing_run_life_pins
    (save_id, run_revision, life_catalog_set_id)
SELECT save_id, run_revision, life_catalog_set_id
FROM run_rule_bundle;

CREATE TEMPORARY TABLE m4d3_previous_new_run_assignment (
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

INSERT INTO m4d3_previous_new_run_assignment
    (
        assignment_key, market_world_id, policy_set_id, career_catalog_bundle_id,
        employment_policy_set_id, life_catalog_set_id, credit_model_version_id,
        real_estate_model_version_id, market_assignment_revision,
        finance_assignment_revision, career_assignment_revision,
        employment_assignment_revision, assignment_revision,
        legacy_dependent_age_years, living_cost_component_version_id,
        welfare_component_version_id, life_event_component_version_id,
        insurance_component_version_id, corporation_component_version_id
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
INNER JOIN life_catalog_set AS catalog ON catalog.id = assignment.life_catalog_set_id
WHERE assignment.assignment_key = 'newRun';

-- Create both draft headers before the insurance schema DDL. The permanent ALTER/CREATE
-- statements below commit them before the existing life-event child guards inspect the v2 clone.
INSERT INTO life_component_version
    (component_kind, version_key, availability, ranked_eligible)
VALUES
    ('lifeEvent', 'dev-unranked-m4-life-event-2026-v2', 'active', FALSE),
    ('insurance', 'dev-unranked-m4-insurance-2026-v1', 'active', FALSE);

ALTER TABLE life_catalog_set
    ADD UNIQUE KEY uk_life_catalog_event_insurance
        (id, life_event_component_version_id, insurance_component_version_id),
    ADD UNIQUE KEY uk_life_catalog_insurance_component
        (id, insurance_component_version_id);

ALTER TABLE life_event_instance
    ADD UNIQUE KEY uk_life_event_instance_component_id
        (save_id, run_revision, life_event_component_version_id, id);

CREATE TABLE insurance_fact_definition (
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
    UNIQUE KEY uk_insurance_fact_order (life_component_version_id, fact_order),
    UNIQUE KEY uk_insurance_fact_key (life_component_version_id, fact_key),
    UNIQUE KEY uk_insurance_fact_component_id (life_component_version_id, id),
    CONSTRAINT fk_insurance_fact_component
        FOREIGN KEY (life_component_version_id) REFERENCES life_component_version (id),
    CONSTRAINT ck_insurance_fact_order CHECK (fact_order BETWEEN 1 AND 16),
    CONSTRAINT ck_insurance_fact_key CHECK (
        fact_key REGEXP '^[a-z][a-zA-Z0-9.]{0,63}$'
    ),
    CONSTRAINT ck_insurance_fact_type CHECK (
        (value_type = 'boolean' AND unit = 'boolean' AND enum_schema_key IS NULL)
        OR (value_type = 'count' AND unit = 'count' AND enum_schema_key IS NULL)
        OR (value_type = 'ageYears' AND unit = 'years' AND enum_schema_key IS NULL)
        OR (
            value_type = 'enum' AND unit = 'enum'
            AND enum_schema_key IS NOT NULL
            AND enum_schema_key REGEXP '^[a-z][a-zA-Z0-9]{0,63}$'
        )
    ),
    CONSTRAINT ck_insurance_fact_authority CHECK (
        window_kind = 'currentGameDay'
        AND source_schema_version = 1
        AND source_kind IN ('gameDay', 'household', 'residence', 'military')
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE insurance_product_version (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    life_component_version_id       BIGINT UNSIGNED NOT NULL,
    schema_version                  SMALLINT UNSIGNED NOT NULL,
    product_order                   TINYINT UNSIGNED NOT NULL,
    product_key                     VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                    VARCHAR(80) NOT NULL,
    purpose                         VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ranked_availability             VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    eligibility_ast                 JSON NOT NULL,
    ast_node_count                  SMALLINT UNSIGNED NOT NULL,
    ast_max_depth                   TINYINT UNSIGNED NOT NULL,
    premium_krw                     BIGINT NOT NULL,
    premium_cadence_game_days       SMALLINT UNSIGNED NOT NULL,
    term_game_days                  SMALLINT UNSIGNED NOT NULL,
    waiting_game_days               SMALLINT UNSIGNED NOT NULL,
    claim_window_game_days          SMALLINT UNSIGNED NOT NULL,
    grace_game_days                 SMALLINT UNSIGNED NOT NULL,
    reinstatement_allowed           BOOLEAN NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_insurance_product_order (life_component_version_id, product_order),
    UNIQUE KEY uk_insurance_product_key (life_component_version_id, product_key),
    UNIQUE KEY uk_insurance_product_component_id (life_component_version_id, id),
    CONSTRAINT fk_insurance_product_component
        FOREIGN KEY (life_component_version_id) REFERENCES life_component_version (id),
    CONSTRAINT ck_insurance_product_schema CHECK (schema_version = 1),
    CONSTRAINT ck_insurance_product_order CHECK (product_order BETWEEN 1 AND 16),
    CONSTRAINT ck_insurance_product_key CHECK (
        product_key REGEXP '^[a-z][a-zA-Z0-9]{0,63}$'
    ),
    CONSTRAINT ck_insurance_product_name CHECK (CHAR_LENGTH(display_name) BETWEEN 1 AND 80),
    CONSTRAINT ck_insurance_product_provenance CHECK (
        purpose = 'gameBalance' AND ranked_availability = 'unrankedOnly'
    ),
    CONSTRAINT ck_insurance_product_ast CHECK (
        COALESCE(
            JSON_TYPE(eligibility_ast) = 'OBJECT'
            AND JSON_LENGTH(eligibility_ast) = 3
            AND JSON_TYPE(JSON_EXTRACT(eligibility_ast, '$.version')) = 'INTEGER'
            AND JSON_UNQUOTE(JSON_EXTRACT(eligibility_ast, '$.version')) = '1'
            AND JSON_TYPE(JSON_EXTRACT(eligibility_ast, '$.kind')) = 'STRING'
            AND JSON_UNQUOTE(JSON_EXTRACT(eligibility_ast, '$.kind')) = 'all'
            AND JSON_TYPE(JSON_EXTRACT(eligibility_ast, '$.children')) = 'ARRAY'
            AND JSON_LENGTH(JSON_EXTRACT(eligibility_ast, '$.children')) BETWEEN 1 AND 32,
            FALSE
        ) = TRUE
        AND ast_node_count BETWEEN 1 AND 128
        AND ast_max_depth BETWEEN 1 AND 12
    ),
    CONSTRAINT ck_insurance_product_money CHECK (
        premium_krw BETWEEN 1 AND 9007199254740991
    ),
    CONSTRAINT ck_insurance_product_period CHECK (
        premium_cadence_game_days BETWEEN 1 AND 366
        AND term_game_days BETWEEN premium_cadence_game_days AND 3660
        AND waiting_game_days < term_game_days
        AND claim_window_game_days BETWEEN 1 AND 366
    ),
    CONSTRAINT ck_insurance_product_v1 CHECK (
        grace_game_days = 0 AND reinstatement_allowed = FALSE
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE insurance_product_coverage (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    life_component_version_id       BIGINT UNSIGNED NOT NULL,
    product_version_id              BIGINT UNSIGNED NOT NULL,
    coverage_order                  TINYINT UNSIGNED NOT NULL,
    coverage_kind                   VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    event_key                       VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    effect_kind                     VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    deductible_krw                  BIGINT NOT NULL,
    occurrence_limit_krw            BIGINT NOT NULL,
    term_limit_krw                  BIGINT NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_insurance_coverage_order (product_version_id, coverage_order),
    UNIQUE KEY uk_insurance_coverage_event_effect
        (product_version_id, event_key, effect_kind),
    UNIQUE KEY uk_insurance_coverage_component_product_id
        (life_component_version_id, product_version_id, id),
    CONSTRAINT fk_insurance_coverage_product
        FOREIGN KEY (life_component_version_id, product_version_id)
        REFERENCES insurance_product_version (life_component_version_id, id),
    CONSTRAINT ck_insurance_coverage_order CHECK (coverage_order BETWEEN 1 AND 8),
    CONSTRAINT ck_insurance_coverage_contract CHECK (
        coverage_kind = 'fixedIndemnity'
        AND event_key REGEXP '^[a-z][a-zA-Z0-9]{0,63}$'
        AND effect_kind = 'fixedWalletExpense'
    ),
    CONSTRAINT ck_insurance_coverage_money CHECK (
        deductible_krw BETWEEN 0 AND 9007199254740991
        AND occurrence_limit_krw BETWEEN 1 AND 9007199254740991
        AND term_limit_krw BETWEEN occurrence_limit_krw AND 9007199254740991
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE insurance_contract (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    life_catalog_set_id             BIGINT UNSIGNED NOT NULL,
    insurance_component_version_id  BIGINT UNSIGNED NOT NULL,
    product_version_id              BIGINT UNSIGNED NOT NULL,
    enrollment_command_id           CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    status                          VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    start_game_day                  INT UNSIGNED NOT NULL,
    coverage_start_game_day         INT UNSIGNED NOT NULL,
    waiting_ends_game_day           INT UNSIGNED NOT NULL,
    term_end_exclusive              INT UNSIGNED NOT NULL,
    coverage_end_exclusive          INT UNSIGNED NOT NULL,
    paid_term_krw                   BIGINT NOT NULL DEFAULT 0,
    reserved_term_krw               BIGINT NOT NULL DEFAULT 0,
    terminal_game_day               INT UNSIGNED NULL,
    terminal_reason                 VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NULL,
    active_product_version_id        BIGINT UNSIGNED
        GENERATED ALWAYS AS (
            CASE WHEN status = 'active' THEN product_version_id ELSE NULL END
        ) STORED,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_insurance_contract_enrollment_command (save_id, enrollment_command_id),
    UNIQUE KEY uk_insurance_contract_active_product
        (save_id, run_revision, active_product_version_id),
    UNIQUE KEY uk_insurance_contract_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_insurance_contract_component_product_id
        (save_id, run_revision, insurance_component_version_id, product_version_id, id),
    KEY ix_insurance_contract_history (save_id, run_revision, start_game_day, id),
    KEY ix_insurance_contract_status (save_id, run_revision, status, id),
    CONSTRAINT fk_insurance_contract_rule_bundle
        FOREIGN KEY (save_id, run_revision, life_catalog_set_id)
        REFERENCES run_rule_bundle (save_id, run_revision, life_catalog_set_id),
    CONSTRAINT fk_insurance_contract_catalog_component
        FOREIGN KEY (life_catalog_set_id, insurance_component_version_id)
        REFERENCES life_catalog_set (id, insurance_component_version_id),
    CONSTRAINT fk_insurance_contract_product
        FOREIGN KEY (insurance_component_version_id, product_version_id)
        REFERENCES insurance_product_version (life_component_version_id, id),
    CONSTRAINT fk_insurance_contract_command
        FOREIGN KEY (save_id, enrollment_command_id)
        REFERENCES command_identity (save_id, command_id),
    CONSTRAINT ck_insurance_contract_period CHECK (
        coverage_start_game_day = start_game_day
        AND waiting_ends_game_day >= coverage_start_game_day
        AND term_end_exclusive > waiting_ends_game_day
        AND coverage_end_exclusive > coverage_start_game_day
        AND coverage_end_exclusive <= term_end_exclusive
    ),
    CONSTRAINT ck_insurance_contract_money CHECK (
        paid_term_krw BETWEEN 0 AND 9007199254740991
        AND reserved_term_krw BETWEEN 0 AND 9007199254740991
        AND paid_term_krw + reserved_term_krw <= 9007199254740991
    ),
    CONSTRAINT ck_insurance_contract_state CHECK (
        (status IN ('pending', 'active')
         AND coverage_end_exclusive = term_end_exclusive
         AND terminal_game_day IS NULL AND terminal_reason IS NULL)
        OR
        (status IN ('lapsed', 'expired', 'cancelled')
         AND terminal_game_day IS NOT NULL AND terminal_reason IS NOT NULL
         AND (
             (status = 'lapsed' AND terminal_reason = 'premiumMissed')
             OR (status = 'expired' AND terminal_reason IN ('termEnded', 'newRun'))
             OR (status = 'cancelled' AND terminal_reason = 'playerCancellation')
         ))
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE insurance_contract_eligibility_pin (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    contract_id                     BIGINT UNSIGNED NOT NULL,
    evaluation_game_day             INT UNSIGNED NOT NULL,
    fact_count                      TINYINT UNSIGNED NOT NULL,
    eligibility_result              VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    canonical_input_json            LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    fact_fingerprint                CHAR(64) CHARACTER SET ascii COLLATE ascii_bin
        GENERATED ALWAYS AS (SHA2(canonical_input_json, 256)) STORED,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_insurance_contract_eligibility_contract
        (save_id, run_revision, contract_id),
    UNIQUE KEY uk_insurance_contract_eligibility_fingerprint
        (save_id, run_revision, contract_id, fact_fingerprint),
    CONSTRAINT fk_insurance_contract_eligibility_contract
        FOREIGN KEY (save_id, run_revision, contract_id)
        REFERENCES insurance_contract (save_id, run_revision, id),
    CONSTRAINT ck_insurance_contract_eligibility CHECK (
        fact_count = 4
        AND eligibility_result = 'eligible'
        AND JSON_VALID(canonical_input_json)
        AND JSON_TYPE(CAST(canonical_input_json AS JSON)) = 'OBJECT'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE insurance_contract_transition (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    contract_id                     BIGINT UNSIGNED NOT NULL,
    transition_no                   TINYINT UNSIGNED NOT NULL,
    from_status                     VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL,
    to_status                       VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    command_id                      CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    transition_game_day             INT UNSIGNED NOT NULL,
    transition_reason               VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_insurance_contract_transition_no
        (save_id, run_revision, contract_id, transition_no),
    UNIQUE KEY uk_insurance_contract_transition_status
        (save_id, run_revision, contract_id, to_status),
    CONSTRAINT fk_insurance_contract_transition_contract
        FOREIGN KEY (save_id, run_revision, contract_id)
        REFERENCES insurance_contract (save_id, run_revision, id),
    CONSTRAINT fk_insurance_contract_transition_command
        FOREIGN KEY (save_id, command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT ck_insurance_contract_transition CHECK (
        (transition_no = 1 AND from_status IS NULL AND to_status = 'pending'
         AND command_id IS NOT NULL AND transition_reason = 'playerEnrollment')
        OR
        (transition_no = 2 AND from_status = 'pending' AND to_status = 'active'
         AND command_id IS NOT NULL AND transition_reason = 'firstPremiumPaid')
        OR
        (transition_no = 3 AND from_status = 'active'
         AND to_status IN ('lapsed', 'expired', 'cancelled')
         AND (
             (to_status = 'lapsed' AND command_id IS NULL
              AND transition_reason = 'premiumMissed')
             OR (to_status = 'expired' AND command_id IS NULL
                 AND transition_reason IN ('termEnded', 'newRun'))
             OR (to_status = 'cancelled' AND command_id IS NOT NULL
                 AND transition_reason = 'playerCancellation')
         ))
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE insurance_premium_charge (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    contract_id                     BIGINT UNSIGNED NOT NULL,
    charge_no                       TINYINT UNSIGNED NOT NULL,
    due_game_day                    INT UNSIGNED NOT NULL,
    amount_krw                      BIGINT NOT NULL,
    status                          VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    scheduled_settlement_id         BIGINT UNSIGNED NULL,
    ledger_transaction_id           BIGINT UNSIGNED NULL,
    paid_game_day                   INT UNSIGNED NULL,
    terminal_game_day               INT UNSIGNED NULL,
    terminal_reason                 VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_insurance_premium_contract_charge
        (save_id, run_revision, contract_id, charge_no),
    UNIQUE KEY uk_insurance_premium_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_insurance_premium_settlement
        (save_id, run_revision, scheduled_settlement_id),
    UNIQUE KEY uk_insurance_premium_ledger
        (save_id, run_revision, ledger_transaction_id),
    KEY ix_insurance_premium_due
        (save_id, run_revision, status, due_game_day, contract_id, charge_no, id),
    CONSTRAINT fk_insurance_premium_contract
        FOREIGN KEY (save_id, run_revision, contract_id)
        REFERENCES insurance_contract (save_id, run_revision, id),
    CONSTRAINT fk_insurance_premium_settlement
        FOREIGN KEY (save_id, run_revision, scheduled_settlement_id)
        REFERENCES scheduled_settlement (save_id, run_revision, id),
    CONSTRAINT fk_insurance_premium_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT ck_insurance_premium_charge CHECK (
        charge_no BETWEEN 1 AND 12
        AND amount_krw BETWEEN 1 AND 9007199254740991
    ),
    CONSTRAINT ck_insurance_premium_state CHECK (
        (status = 'scheduled' AND ledger_transaction_id IS NULL
         AND paid_game_day IS NULL AND terminal_game_day IS NULL
         AND terminal_reason IS NULL)
        OR
        (status = 'paid' AND ledger_transaction_id IS NOT NULL
         AND paid_game_day = due_game_day AND terminal_game_day IS NULL
         AND terminal_reason IS NULL
         AND (charge_no = 1 OR scheduled_settlement_id IS NOT NULL))
        OR
        (status = 'missed' AND charge_no > 1 AND scheduled_settlement_id IS NOT NULL
         AND ledger_transaction_id IS NULL AND paid_game_day IS NULL
         AND terminal_game_day = due_game_day AND terminal_reason = 'insufficientWalletCash')
        OR
        (status = 'cancelled' AND charge_no > 1 AND scheduled_settlement_id IS NOT NULL
         AND ledger_transaction_id IS NULL AND paid_game_day IS NULL
         AND terminal_game_day IS NOT NULL
         AND terminal_reason IN ('contractLapsed', 'playerCancellation', 'termEnded', 'newRun'))
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE insurance_claim (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    life_catalog_set_id             BIGINT UNSIGNED NOT NULL,
    life_event_component_version_id BIGINT UNSIGNED NOT NULL,
    insurance_component_version_id  BIGINT UNSIGNED NOT NULL,
    life_event_instance_id          BIGINT UNSIGNED NOT NULL,
    status                          VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    offered_game_day                INT UNSIGNED NOT NULL,
    contract_pin_count              TINYINT UNSIGNED NOT NULL,
    contract_pin_sha256             CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    gross_cost_krw                  BIGINT NULL,
    payout_krw                      BIGINT NULL,
    resolved_game_day               INT UNSIGNED NULL,
    filing_deadline_game_day        INT UNSIGNED NULL,
    paid_game_day                   INT UNSIGNED NULL,
    ledger_transaction_id           BIGINT UNSIGNED NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_insurance_claim_event (save_id, run_revision, life_event_instance_id),
    UNIQUE KEY uk_insurance_claim_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_insurance_claim_event_id
        (save_id, run_revision, life_event_component_version_id, life_event_instance_id, id),
    UNIQUE KEY uk_insurance_claim_ledger (save_id, run_revision, ledger_transaction_id),
    KEY ix_insurance_claim_pending (save_id, run_revision, status, id),
    KEY ix_insurance_claim_history (save_id, run_revision, resolved_game_day, id),
    CONSTRAINT fk_insurance_claim_rule_bundle
        FOREIGN KEY (save_id, run_revision, life_catalog_set_id)
        REFERENCES run_rule_bundle (save_id, run_revision, life_catalog_set_id),
    CONSTRAINT fk_insurance_claim_catalog_components
        FOREIGN KEY (
            life_catalog_set_id, life_event_component_version_id,
            insurance_component_version_id
        ) REFERENCES life_catalog_set (
            id, life_event_component_version_id, insurance_component_version_id
        ),
    CONSTRAINT fk_insurance_claim_event
        FOREIGN KEY (
            save_id, run_revision, life_event_component_version_id, life_event_instance_id
        ) REFERENCES life_event_instance (
            save_id, run_revision, life_event_component_version_id, id
        ),
    CONSTRAINT fk_insurance_claim_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT ck_insurance_claim_pin CHECK (
        contract_pin_count <= 8 AND contract_pin_sha256 REGEXP '^[0-9a-f]{64}$'
    ),
    CONSTRAINT ck_insurance_claim_money CHECK (
        (gross_cost_krw IS NULL OR gross_cost_krw BETWEEN 1 AND 9007199254740991)
        AND (payout_krw IS NULL OR payout_krw BETWEEN 0 AND 9007199254740991)
        AND (gross_cost_krw IS NULL OR payout_krw IS NULL OR payout_krw <= gross_cost_krw)
    ),
    CONSTRAINT ck_insurance_claim_state CHECK (
        (status = 'candidate' AND gross_cost_krw IS NULL AND payout_krw IS NULL
         AND resolved_game_day IS NULL AND filing_deadline_game_day IS NULL
         AND paid_game_day IS NULL AND ledger_transaction_id IS NULL)
        OR
        (status = 'notApplicable' AND gross_cost_krw IS NULL AND payout_krw IS NULL
         AND resolved_game_day IS NOT NULL AND filing_deadline_game_day IS NULL
         AND paid_game_day IS NULL AND ledger_transaction_id IS NULL)
        OR
        (status = 'notCovered' AND gross_cost_krw IS NOT NULL AND payout_krw = 0
         AND resolved_game_day IS NOT NULL AND filing_deadline_game_day IS NULL
         AND paid_game_day IS NULL AND ledger_transaction_id IS NULL)
        OR
        (status = 'ready' AND gross_cost_krw IS NOT NULL AND payout_krw > 0
         AND resolved_game_day IS NOT NULL
         AND filing_deadline_game_day > resolved_game_day
         AND paid_game_day IS NULL AND ledger_transaction_id IS NULL)
        OR
        (status = 'paid' AND gross_cost_krw IS NOT NULL AND payout_krw > 0
         AND resolved_game_day IS NOT NULL
         AND filing_deadline_game_day > resolved_game_day
         AND paid_game_day IS NOT NULL
         AND paid_game_day < filing_deadline_game_day
         AND ledger_transaction_id IS NOT NULL)
        OR
        (status = 'expired' AND gross_cost_krw IS NOT NULL AND payout_krw > 0
         AND resolved_game_day IS NOT NULL
         AND filing_deadline_game_day > resolved_game_day
         AND paid_game_day IS NULL AND ledger_transaction_id IS NULL)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE insurance_claim_contract_pin (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    claim_id                        BIGINT UNSIGNED NOT NULL,
    contract_id                     BIGINT UNSIGNED NOT NULL,
    pin_order                       TINYINT UNSIGNED NOT NULL,
    insurance_component_version_id  BIGINT UNSIGNED NOT NULL,
    product_version_id              BIGINT UNSIGNED NOT NULL,
    coverage_id                     BIGINT UNSIGNED NOT NULL,
    coverage_start_game_day         INT UNSIGNED NOT NULL,
    waiting_ends_game_day           INT UNSIGNED NOT NULL,
    coverage_end_exclusive          INT UNSIGNED NOT NULL,
    waiting_satisfied               BOOLEAN NOT NULL,
    deductible_krw                  BIGINT NOT NULL,
    occurrence_limit_krw            BIGINT NOT NULL,
    term_limit_krw                  BIGINT NOT NULL,
    paid_term_krw_at_offer          BIGINT NOT NULL,
    reserved_term_krw_at_offer      BIGINT NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_insurance_claim_pin_contract
        (save_id, run_revision, claim_id, contract_id),
    UNIQUE KEY uk_insurance_claim_pin_order
        (save_id, run_revision, claim_id, pin_order),
    UNIQUE KEY uk_insurance_claim_pin_claim_id
        (save_id, run_revision, claim_id, id),
    CONSTRAINT fk_insurance_claim_pin_claim
        FOREIGN KEY (save_id, run_revision, claim_id)
        REFERENCES insurance_claim (save_id, run_revision, id),
    CONSTRAINT fk_insurance_claim_pin_contract
        FOREIGN KEY (
            save_id, run_revision, insurance_component_version_id,
            product_version_id, contract_id
        ) REFERENCES insurance_contract (
            save_id, run_revision, insurance_component_version_id,
            product_version_id, id
        ),
    CONSTRAINT fk_insurance_claim_pin_coverage
        FOREIGN KEY (insurance_component_version_id, product_version_id, coverage_id)
        REFERENCES insurance_product_coverage (
            life_component_version_id, product_version_id, id
        ),
    CONSTRAINT ck_insurance_claim_pin_order CHECK (pin_order BETWEEN 1 AND 8),
    CONSTRAINT ck_insurance_claim_pin_period CHECK (
        waiting_ends_game_day >= coverage_start_game_day
        AND coverage_end_exclusive > coverage_start_game_day
    ),
    CONSTRAINT ck_insurance_claim_pin_waiting CHECK (waiting_satisfied IN (FALSE, TRUE)),
    CONSTRAINT ck_insurance_claim_pin_money CHECK (
        deductible_krw BETWEEN 0 AND 9007199254740991
        AND occurrence_limit_krw BETWEEN 1 AND 9007199254740991
        AND term_limit_krw BETWEEN occurrence_limit_krw AND 9007199254740991
        AND paid_term_krw_at_offer BETWEEN 0 AND term_limit_krw
        AND reserved_term_krw_at_offer BETWEEN 0 AND term_limit_krw
        AND paid_term_krw_at_offer + reserved_term_krw_at_offer <= term_limit_krw
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE insurance_claim_allocation (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    claim_id                        BIGINT UNSIGNED NOT NULL,
    claim_contract_pin_id           BIGINT UNSIGNED NOT NULL,
    contract_id                     BIGINT UNSIGNED NOT NULL,
    allocation_order                TINYINT UNSIGNED NOT NULL,
    raw_indemnity_krw               BIGINT NOT NULL,
    allocated_krw                   BIGINT NOT NULL,
    reserved_term_before_krw        BIGINT NOT NULL,
    reserved_term_after_krw         BIGINT NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_insurance_claim_allocation_contract
        (save_id, run_revision, claim_id, contract_id),
    UNIQUE KEY uk_insurance_claim_allocation_order
        (save_id, run_revision, claim_id, allocation_order),
    CONSTRAINT fk_insurance_claim_allocation_claim
        FOREIGN KEY (save_id, run_revision, claim_id)
        REFERENCES insurance_claim (save_id, run_revision, id),
    CONSTRAINT fk_insurance_claim_allocation_pin
        FOREIGN KEY (save_id, run_revision, claim_id, claim_contract_pin_id)
        REFERENCES insurance_claim_contract_pin (save_id, run_revision, claim_id, id),
    CONSTRAINT fk_insurance_claim_allocation_contract
        FOREIGN KEY (save_id, run_revision, contract_id)
        REFERENCES insurance_contract (save_id, run_revision, id),
    CONSTRAINT ck_insurance_claim_allocation_order CHECK (allocation_order BETWEEN 1 AND 8),
    CONSTRAINT ck_insurance_claim_allocation_money CHECK (
        raw_indemnity_krw BETWEEN 1 AND 9007199254740991
        AND allocated_krw BETWEEN 1 AND raw_indemnity_krw
        AND reserved_term_before_krw BETWEEN 0 AND 9007199254740991
        AND reserved_term_after_krw = reserved_term_before_krw + allocated_krw
        AND reserved_term_after_krw <= 9007199254740991
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE insurance_claim_transition (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    claim_id                        BIGINT UNSIGNED NOT NULL,
    transition_no                   TINYINT UNSIGNED NOT NULL,
    from_status                     VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL,
    to_status                       VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    command_id                      CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    transition_game_day             INT UNSIGNED NOT NULL,
    transition_reason               VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_insurance_claim_transition_no
        (save_id, run_revision, claim_id, transition_no),
    UNIQUE KEY uk_insurance_claim_transition_status
        (save_id, run_revision, claim_id, to_status),
    CONSTRAINT fk_insurance_claim_transition_claim
        FOREIGN KEY (save_id, run_revision, claim_id)
        REFERENCES insurance_claim (save_id, run_revision, id),
    CONSTRAINT fk_insurance_claim_transition_command
        FOREIGN KEY (save_id, command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT ck_insurance_claim_transition CHECK (
        (transition_no = 1 AND from_status IS NULL AND to_status = 'candidate'
         AND command_id IS NULL AND transition_reason = 'eventOffered')
        OR
        (transition_no = 2 AND from_status = 'candidate'
         AND to_status IN ('notApplicable', 'notCovered', 'ready')
         AND command_id IS NULL AND transition_reason = 'eventResolved')
        OR
        (transition_no = 3 AND from_status = 'ready'
         AND (
             (to_status = 'paid' AND command_id IS NOT NULL
              AND transition_reason = 'playerClaim')
             OR (to_status = 'expired' AND command_id IS NULL
                 AND transition_reason = 'filingDeadline')
         ))
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE insurance_command_receipt (
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    insurance_component_version_id  BIGINT UNSIGNED NOT NULL,
    command_id                      CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    command_kind                    VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    payload_sha256                  CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    contract_id                     BIGINT UNSIGNED NULL,
    claim_id                        BIGINT UNSIGNED NULL,
    ledger_transaction_id           BIGINT UNSIGNED NULL,
    result_json                     LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    committed_state_revision        BIGINT UNSIGNED NOT NULL,
    committed_game_day              INT UNSIGNED NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, command_id),
    CONSTRAINT fk_insurance_command_receipt_command
        FOREIGN KEY (save_id, command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT fk_insurance_command_receipt_contract
        FOREIGN KEY (save_id, run_revision, contract_id)
        REFERENCES insurance_contract (save_id, run_revision, id),
    CONSTRAINT fk_insurance_command_receipt_claim
        FOREIGN KEY (save_id, run_revision, claim_id)
        REFERENCES insurance_claim (save_id, run_revision, id),
    CONSTRAINT fk_insurance_command_receipt_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT ck_insurance_command_receipt CHECK (
        command_kind IN ('enrollInsurance', 'cancelInsurance', 'fileInsuranceClaim')
        AND payload_sha256 REGEXP '^[0-9a-f]{64}$'
        AND JSON_VALID(result_json)
        AND committed_state_revision > 0
        AND (
            (command_kind = 'enrollInsurance' AND contract_id IS NOT NULL
             AND claim_id IS NULL AND ledger_transaction_id IS NOT NULL)
            OR (command_kind = 'cancelInsurance' AND contract_id IS NOT NULL
                AND claim_id IS NULL AND ledger_transaction_id IS NULL)
            OR (command_kind = 'fileInsuranceClaim' AND contract_id IS NULL
                AND claim_id IS NOT NULL AND ledger_transaction_id IS NOT NULL)
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

-- Catalog children can only be appended to an unmanifested draft. Once published, every header
-- and child row is immutable, so old runs always retain the graph identified by their digest.
CREATE TRIGGER tr_insurance_fact_draft_insert
BEFORE INSERT ON insurance_fact_definition
FOR EACH ROW
SET NEW.life_component_version_id = IF(
    EXISTS (
        SELECT 1 FROM life_component_version AS component
        WHERE component.id = NEW.life_component_version_id
          AND component.component_kind = 'insurance'
          AND component.sealed_at IS NULL
          AND NOT EXISTS (
              SELECT 1 FROM life_component_canonical_manifest AS manifest
              WHERE manifest.life_component_version_id = component.id
          )
    ),
    NEW.life_component_version_id,
    NULL
);

CREATE TRIGGER tr_insurance_fact_no_update
BEFORE UPDATE ON insurance_fact_definition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insurance facts are immutable';

CREATE TRIGGER tr_insurance_fact_no_delete
BEFORE DELETE ON insurance_fact_definition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insurance facts are immutable';

CREATE TRIGGER tr_insurance_product_draft_insert
BEFORE INSERT ON insurance_product_version
FOR EACH ROW
SET NEW.life_component_version_id = IF(
    EXISTS (
        SELECT 1 FROM life_component_version AS component
        WHERE component.id = NEW.life_component_version_id
          AND component.component_kind = 'insurance'
          AND component.sealed_at IS NULL
          AND NOT EXISTS (
              SELECT 1 FROM life_component_canonical_manifest AS manifest
              WHERE manifest.life_component_version_id = component.id
          )
    ),
    NEW.life_component_version_id,
    NULL
);

CREATE TRIGGER tr_insurance_product_no_update
BEFORE UPDATE ON insurance_product_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insurance products are immutable';

CREATE TRIGGER tr_insurance_product_no_delete
BEFORE DELETE ON insurance_product_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insurance products are immutable';

CREATE TRIGGER tr_insurance_coverage_draft_insert
BEFORE INSERT ON insurance_product_coverage
FOR EACH ROW
SET NEW.life_component_version_id = IF(
    EXISTS (
        SELECT 1
        FROM insurance_product_version AS product
        INNER JOIN life_component_version AS component
            ON component.id = product.life_component_version_id
        WHERE product.id = NEW.product_version_id
          AND product.life_component_version_id = NEW.life_component_version_id
          AND component.component_kind = 'insurance'
          AND component.sealed_at IS NULL
          AND NOT EXISTS (
              SELECT 1 FROM life_component_canonical_manifest AS manifest
              WHERE manifest.life_component_version_id = component.id
          )
    ),
    NEW.life_component_version_id,
    NULL
);

CREATE TRIGGER tr_insurance_coverage_no_update
BEFORE UPDATE ON insurance_product_coverage
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insurance coverages are immutable';

CREATE TRIGGER tr_insurance_coverage_no_delete
BEFORE DELETE ON insurance_product_coverage
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insurance coverages are immutable';

CREATE TRIGGER tr_insurance_contract_valid_insert
BEFORE INSERT ON insurance_contract
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'pending'
        AND NEW.paid_term_krw = 0
        AND NEW.reserved_term_krw = 0
        AND EXISTS (
            SELECT 1
            FROM save
            INNER JOIN run_rule_bundle AS bundle
                ON bundle.save_id = save.id AND bundle.run_revision = save.run_revision
            INNER JOIN life_catalog_set AS catalog
                ON catalog.id = bundle.life_catalog_set_id
               AND catalog.insurance_component_version_id
                    = NEW.insurance_component_version_id
            INNER JOIN life_component_version AS component
                ON component.id = catalog.insurance_component_version_id
            INNER JOIN insurance_product_version AS product
                ON product.id = NEW.product_version_id
               AND product.life_component_version_id = component.id
            INNER JOIN command_identity AS identity
                ON identity.save_id = save.id
               AND BINARY identity.command_id = BINARY NEW.enrollment_command_id
               AND identity.command_kind = 'enrollInsurance'
            WHERE save.id = NEW.save_id
              AND save.run_revision = NEW.run_revision
              AND bundle.life_catalog_set_id = NEW.life_catalog_set_id
              AND catalog.sealed_at IS NOT NULL
              AND component.component_kind = 'insurance'
              AND component.version_key = 'dev-unranked-m4-insurance-2026-v1'
              AND component.availability = 'active'
              AND component.sealed_at IS NOT NULL
              AND product.schema_version = 1
              AND NEW.start_game_day = save.game_day
              AND NEW.coverage_start_game_day = NEW.start_game_day
              AND NEW.waiting_ends_game_day
                    = NEW.start_game_day + product.waiting_game_days
              AND NEW.term_end_exclusive = NEW.start_game_day + product.term_game_days
              AND NEW.coverage_end_exclusive = NEW.term_end_exclusive
              AND identity.initial_run_revision = save.run_revision
              AND identity.initial_state_revision = save.state_revision
              AND identity.initial_game_day = save.game_day
        )
        AND (
            SELECT COUNT(*) FROM insurance_contract AS active_contract
            WHERE active_contract.save_id = NEW.save_id
              AND active_contract.run_revision = NEW.run_revision
              AND active_contract.status = 'active'
        ) < 8,
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_insurance_contract_transition_only
BEFORE UPDATE ON insurance_contract
FOR EACH ROW
SET NEW.status = IF(
    NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.life_catalog_set_id = OLD.life_catalog_set_id
        AND NEW.insurance_component_version_id = OLD.insurance_component_version_id
        AND NEW.product_version_id = OLD.product_version_id
        AND BINARY NEW.enrollment_command_id = BINARY OLD.enrollment_command_id
        AND NEW.start_game_day = OLD.start_game_day
        AND NEW.coverage_start_game_day = OLD.coverage_start_game_day
        AND NEW.waiting_ends_game_day = OLD.waiting_ends_game_day
        AND NEW.term_end_exclusive = OLD.term_end_exclusive
        AND NEW.created_at = OLD.created_at
        AND (
            (
                OLD.status = 'pending' AND NEW.status = 'active'
                AND NEW.coverage_end_exclusive = OLD.coverage_end_exclusive
                AND NEW.paid_term_krw = OLD.paid_term_krw
                AND NEW.reserved_term_krw = OLD.reserved_term_krw
                AND NEW.terminal_game_day IS NULL AND NEW.terminal_reason IS NULL
                AND EXISTS (
                    SELECT 1 FROM insurance_contract_eligibility_pin AS pin
                    WHERE pin.save_id = OLD.save_id AND pin.run_revision = OLD.run_revision
                      AND pin.contract_id = OLD.id AND pin.eligibility_result = 'eligible'
                )
                AND (
                    SELECT COUNT(*) FROM insurance_premium_charge AS charge
                    WHERE charge.save_id = OLD.save_id AND charge.run_revision = OLD.run_revision
                      AND charge.contract_id = OLD.id
                ) = 12
                AND EXISTS (
                    SELECT 1 FROM insurance_premium_charge AS charge
                    WHERE charge.save_id = OLD.save_id AND charge.run_revision = OLD.run_revision
                      AND charge.contract_id = OLD.id AND charge.charge_no = 1
                      AND charge.status = 'paid' AND charge.due_game_day = OLD.start_game_day
                )
                AND NOT EXISTS (
                    SELECT 1 FROM insurance_premium_charge AS charge
                    WHERE charge.save_id = OLD.save_id AND charge.run_revision = OLD.run_revision
                      AND charge.contract_id = OLD.id AND charge.charge_no > 1
                      AND (charge.status <> 'scheduled'
                           OR charge.scheduled_settlement_id IS NULL)
                )
                AND EXISTS (
                    SELECT 1 FROM insurance_contract_transition AS transition_row
                    WHERE transition_row.save_id = OLD.save_id
                      AND transition_row.run_revision = OLD.run_revision
                      AND transition_row.contract_id = OLD.id
                      AND transition_row.transition_no = 2
                      AND transition_row.from_status = 'pending'
                      AND transition_row.to_status = 'active'
                )
            )
            OR (
                OLD.status = 'active'
                AND NEW.status IN ('lapsed', 'expired', 'cancelled')
                AND NEW.paid_term_krw = OLD.paid_term_krw
                AND NEW.reserved_term_krw = OLD.reserved_term_krw
                AND NEW.terminal_game_day IS NOT NULL
                AND EXISTS (
                    SELECT 1 FROM insurance_contract_transition AS transition_row
                    WHERE transition_row.save_id = OLD.save_id
                      AND transition_row.run_revision = OLD.run_revision
                      AND transition_row.contract_id = OLD.id
                      AND transition_row.transition_no = 3
                      AND transition_row.from_status = 'active'
                      AND transition_row.to_status = NEW.status
                      AND transition_row.transition_game_day = NEW.terminal_game_day
                      AND transition_row.transition_reason = NEW.terminal_reason
                )
                AND (
                    (NEW.status = 'lapsed' AND NEW.terminal_reason = 'premiumMissed'
                     AND NEW.coverage_end_exclusive = NEW.terminal_game_day + 1)
                    OR (NEW.status = 'cancelled'
                        AND NEW.terminal_reason = 'playerCancellation'
                        AND NEW.coverage_end_exclusive = NEW.terminal_game_day + 1)
                    OR (NEW.status = 'expired' AND NEW.terminal_reason = 'termEnded'
                        AND NEW.terminal_game_day = OLD.term_end_exclusive
                        AND NEW.coverage_end_exclusive = OLD.term_end_exclusive)
                    OR (NEW.status = 'expired' AND NEW.terminal_reason = 'newRun'
                        AND NEW.coverage_end_exclusive
                            = LEAST(OLD.term_end_exclusive, NEW.terminal_game_day + 1))
                )
            )
            OR (
                NEW.status = OLD.status
                AND NEW.coverage_end_exclusive = OLD.coverage_end_exclusive
                AND NEW.terminal_game_day <=> OLD.terminal_game_day
                AND BINARY NEW.terminal_reason <=> BINARY OLD.terminal_reason
                AND (
                    EXISTS (
                        SELECT 1 FROM insurance_claim_allocation AS allocation
                        INNER JOIN insurance_claim AS claim ON claim.id = allocation.claim_id
                        WHERE allocation.save_id = OLD.save_id
                          AND allocation.run_revision = OLD.run_revision
                          AND allocation.contract_id = OLD.id
                          AND claim.status = 'candidate'
                          AND OLD.paid_term_krw = NEW.paid_term_krw
                          AND allocation.reserved_term_before_krw = OLD.reserved_term_krw
                          AND allocation.reserved_term_after_krw = NEW.reserved_term_krw
                    )
                    OR EXISTS (
                        SELECT 1 FROM insurance_claim_allocation AS allocation
                        INNER JOIN insurance_claim_transition AS transition_row
                            ON transition_row.save_id = allocation.save_id
                           AND transition_row.run_revision = allocation.run_revision
                           AND transition_row.claim_id = allocation.claim_id
                           AND transition_row.transition_no = 3
                        WHERE allocation.save_id = OLD.save_id
                          AND allocation.run_revision = OLD.run_revision
                          AND allocation.contract_id = OLD.id
                          AND transition_row.to_status IN ('paid', 'expired')
                          AND OLD.reserved_term_krw
                                = NEW.reserved_term_krw + allocation.allocated_krw
                          AND (
                              (transition_row.to_status = 'paid'
                               AND NEW.paid_term_krw
                                    = OLD.paid_term_krw + allocation.allocated_krw)
                              OR (transition_row.to_status = 'expired'
                                  AND NEW.paid_term_krw = OLD.paid_term_krw)
                          )
                    )
                )
            )
        ),
    NEW.status,
    NULL
);

CREATE TRIGGER tr_insurance_contract_no_delete
BEFORE DELETE ON insurance_contract
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insurance contracts are immutable history';

CREATE TRIGGER tr_insurance_eligibility_valid_insert
BEFORE INSERT ON insurance_contract_eligibility_pin
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1 FROM insurance_contract AS contract
        WHERE contract.id = NEW.contract_id
          AND contract.save_id = NEW.save_id
          AND contract.run_revision = NEW.run_revision
          AND contract.status = 'pending'
          AND contract.start_game_day = NEW.evaluation_game_day
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_insurance_eligibility_no_update
BEFORE UPDATE ON insurance_contract_eligibility_pin
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insurance eligibility pins are immutable';

CREATE TRIGGER tr_insurance_eligibility_no_delete
BEFORE DELETE ON insurance_contract_eligibility_pin
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insurance eligibility pins are immutable';

CREATE TRIGGER tr_insurance_contract_transition_valid_insert
BEFORE INSERT ON insurance_contract_transition
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1 FROM insurance_contract AS contract
        WHERE contract.id = NEW.contract_id
          AND contract.save_id = NEW.save_id
          AND contract.run_revision = NEW.run_revision
          AND (
              (NEW.transition_no = 1 AND contract.status = 'pending'
               AND NEW.transition_game_day = contract.start_game_day
               AND BINARY NEW.command_id = BINARY contract.enrollment_command_id)
              OR (NEW.transition_no = 2 AND contract.status = 'pending'
                  AND NEW.transition_game_day = contract.start_game_day
                  AND BINARY NEW.command_id = BINARY contract.enrollment_command_id
                  AND EXISTS (
                      SELECT 1 FROM insurance_contract_transition AS pending_transition
                      WHERE pending_transition.save_id = contract.save_id
                        AND pending_transition.run_revision = contract.run_revision
                        AND pending_transition.contract_id = contract.id
                        AND pending_transition.transition_no = 1
                        AND pending_transition.to_status = 'pending'
                  ))
              OR (NEW.transition_no = 3 AND contract.status = 'active'
                  AND NEW.transition_game_day >= contract.start_game_day)
          )
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_insurance_contract_transition_no_update
BEFORE UPDATE ON insurance_contract_transition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insurance contract transitions are immutable';

CREATE TRIGGER tr_insurance_contract_transition_no_delete
BEFORE DELETE ON insurance_contract_transition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insurance contract transitions are immutable';

CREATE TRIGGER tr_insurance_premium_valid_insert
BEFORE INSERT ON insurance_premium_charge
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'scheduled'
        AND NEW.scheduled_settlement_id IS NULL
        AND EXISTS (
            SELECT 1
            FROM insurance_contract AS contract
            INNER JOIN insurance_product_version AS product
                ON product.id = contract.product_version_id
               AND product.life_component_version_id
                    = contract.insurance_component_version_id
            WHERE contract.id = NEW.contract_id
              AND contract.save_id = NEW.save_id
              AND contract.run_revision = NEW.run_revision
              AND contract.status = 'pending'
              AND NEW.amount_krw = product.premium_krw
              AND NEW.due_game_day
                    = contract.start_game_day
                      + (NEW.charge_no - 1) * product.premium_cadence_game_days
              AND NEW.due_game_day < contract.term_end_exclusive
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_insurance_premium_transition_only
BEFORE UPDATE ON insurance_premium_charge
FOR EACH ROW
SET NEW.status = IF(
    OLD.status = 'scheduled'
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.contract_id = OLD.contract_id
        AND NEW.charge_no = OLD.charge_no
        AND NEW.due_game_day = OLD.due_game_day
        AND NEW.amount_krw = OLD.amount_krw
        AND NEW.created_at = OLD.created_at
        AND (
            (NEW.status = 'scheduled' AND OLD.charge_no > 1
             AND OLD.scheduled_settlement_id IS NULL
             AND NEW.scheduled_settlement_id IS NOT NULL
             AND NEW.ledger_transaction_id IS NULL
             AND NEW.paid_game_day IS NULL
             AND NEW.terminal_game_day IS NULL AND NEW.terminal_reason IS NULL)
            OR (NEW.status = 'paid'
                AND NEW.scheduled_settlement_id <=> OLD.scheduled_settlement_id
                AND NEW.ledger_transaction_id IS NOT NULL
                AND NEW.paid_game_day = OLD.due_game_day
                AND NEW.terminal_game_day IS NULL AND NEW.terminal_reason IS NULL)
            OR (NEW.status = 'missed' AND OLD.charge_no > 1
                AND NEW.scheduled_settlement_id = OLD.scheduled_settlement_id
                AND NEW.ledger_transaction_id IS NULL AND NEW.paid_game_day IS NULL
                AND NEW.terminal_game_day = OLD.due_game_day
                AND NEW.terminal_reason = 'insufficientWalletCash')
            OR (NEW.status = 'cancelled' AND OLD.charge_no > 1
                AND NEW.scheduled_settlement_id = OLD.scheduled_settlement_id
                AND NEW.ledger_transaction_id IS NULL AND NEW.paid_game_day IS NULL
                AND NEW.terminal_game_day IS NOT NULL
                AND NEW.terminal_reason IN (
                    'contractLapsed', 'playerCancellation', 'termEnded', 'newRun'
                ))
        ),
    NEW.status,
    NULL
);

CREATE TRIGGER tr_insurance_premium_no_delete
BEFORE DELETE ON insurance_premium_charge
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insurance premium charges are immutable history';

CREATE TRIGGER tr_insurance_claim_valid_insert
BEFORE INSERT ON insurance_claim
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'candidate'
        AND EXISTS (
            SELECT 1
            FROM life_event_instance AS instance
            INNER JOIN life_catalog_set AS catalog
                ON catalog.id = instance.life_catalog_set_id
               AND catalog.life_event_component_version_id
                    = instance.life_event_component_version_id
               AND catalog.insurance_component_version_id
                    = NEW.insurance_component_version_id
            INNER JOIN life_component_version AS insurance_component
                ON insurance_component.id = catalog.insurance_component_version_id
            WHERE instance.id = NEW.life_event_instance_id
              AND instance.save_id = NEW.save_id
              AND instance.run_revision = NEW.run_revision
              AND instance.life_catalog_set_id = NEW.life_catalog_set_id
              AND instance.life_event_component_version_id
                    = NEW.life_event_component_version_id
              AND instance.status = 'offered'
              AND instance.offered_game_day = NEW.offered_game_day
              AND insurance_component.component_kind = 'insurance'
              AND insurance_component.version_key = 'dev-unranked-m4-insurance-2026-v1'
              AND insurance_component.availability = 'active'
              AND insurance_component.sealed_at IS NOT NULL
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_insurance_claim_projection_only
BEFORE UPDATE ON insurance_claim
FOR EACH ROW
SET NEW.status = IF(
    NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.life_catalog_set_id = OLD.life_catalog_set_id
        AND NEW.life_event_component_version_id = OLD.life_event_component_version_id
        AND NEW.insurance_component_version_id = OLD.insurance_component_version_id
        AND NEW.life_event_instance_id = OLD.life_event_instance_id
        AND NEW.offered_game_day = OLD.offered_game_day
        AND NEW.contract_pin_count = OLD.contract_pin_count
        AND BINARY NEW.contract_pin_sha256 = BINARY OLD.contract_pin_sha256
        AND NEW.created_at = OLD.created_at
        AND (
            (
                OLD.status = 'candidate'
                AND NEW.status IN ('notApplicable', 'notCovered', 'ready')
                AND EXISTS (
                    SELECT 1 FROM insurance_claim_transition AS transition_row
                    WHERE transition_row.save_id = OLD.save_id
                      AND transition_row.run_revision = OLD.run_revision
                      AND transition_row.claim_id = OLD.id
                      AND transition_row.transition_no = 2
                      AND transition_row.from_status = 'candidate'
                      AND transition_row.to_status = NEW.status
                      AND transition_row.transition_game_day = NEW.resolved_game_day
                )
                AND (
                    SELECT COUNT(*) FROM insurance_claim_contract_pin AS pin
                    WHERE pin.save_id = OLD.save_id AND pin.run_revision = OLD.run_revision
                      AND pin.claim_id = OLD.id
                ) = OLD.contract_pin_count
                AND (
                    (NEW.status = 'notApplicable'
                     AND NEW.gross_cost_krw IS NULL AND NEW.payout_krw IS NULL
                     AND NEW.filing_deadline_game_day IS NULL
                     AND NOT EXISTS (
                         SELECT 1 FROM insurance_claim_allocation AS allocation
                         WHERE allocation.save_id = OLD.save_id
                           AND allocation.run_revision = OLD.run_revision
                           AND allocation.claim_id = OLD.id
                     ))
                    OR (NEW.status = 'notCovered'
                        AND NEW.gross_cost_krw > 0 AND NEW.payout_krw = 0
                        AND NEW.filing_deadline_game_day IS NULL
                        AND NOT EXISTS (
                            SELECT 1 FROM insurance_claim_allocation AS allocation
                            WHERE allocation.save_id = OLD.save_id
                              AND allocation.run_revision = OLD.run_revision
                              AND allocation.claim_id = OLD.id
                        ))
                    OR (NEW.status = 'ready'
                        AND NEW.gross_cost_krw > 0 AND NEW.payout_krw > 0
                        AND NEW.filing_deadline_game_day > NEW.resolved_game_day
                        AND (
                            SELECT COUNT(*) FROM insurance_claim_allocation AS allocation
                            WHERE allocation.save_id = OLD.save_id
                              AND allocation.run_revision = OLD.run_revision
                              AND allocation.claim_id = OLD.id
                        ) BETWEEN 1 AND 8
                        AND NEW.payout_krw = (
                            SELECT SUM(allocation.allocated_krw)
                            FROM insurance_claim_allocation AS allocation
                            WHERE allocation.save_id = OLD.save_id
                              AND allocation.run_revision = OLD.run_revision
                              AND allocation.claim_id = OLD.id
                        ))
                )
                AND NEW.paid_game_day IS NULL AND NEW.ledger_transaction_id IS NULL
            )
            OR (
                OLD.status = 'ready' AND NEW.status IN ('paid', 'expired')
                AND NEW.gross_cost_krw = OLD.gross_cost_krw
                AND NEW.payout_krw = OLD.payout_krw
                AND NEW.resolved_game_day = OLD.resolved_game_day
                AND NEW.filing_deadline_game_day = OLD.filing_deadline_game_day
                AND EXISTS (
                    SELECT 1 FROM insurance_claim_transition AS transition_row
                    WHERE transition_row.save_id = OLD.save_id
                      AND transition_row.run_revision = OLD.run_revision
                      AND transition_row.claim_id = OLD.id
                      AND transition_row.transition_no = 3
                      AND transition_row.from_status = 'ready'
                      AND transition_row.to_status = NEW.status
                      AND transition_row.transition_game_day
                            = IF(NEW.status = 'paid', NEW.paid_game_day,
                                 NEW.filing_deadline_game_day)
                )
                AND (
                    (NEW.status = 'paid' AND NEW.paid_game_day IS NOT NULL
                     AND NEW.paid_game_day < OLD.filing_deadline_game_day
                     AND NEW.ledger_transaction_id IS NOT NULL)
                    OR (NEW.status = 'expired' AND NEW.paid_game_day IS NULL
                        AND NEW.ledger_transaction_id IS NULL)
                )
            )
        ),
    NEW.status,
    NULL
);

CREATE TRIGGER tr_insurance_claim_no_delete
BEFORE DELETE ON insurance_claim
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insurance claims are immutable history';

CREATE TRIGGER tr_insurance_claim_pin_valid_insert
BEFORE INSERT ON insurance_claim_contract_pin
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM insurance_claim AS claim
        INNER JOIN insurance_contract AS contract
            ON contract.id = NEW.contract_id
           AND contract.save_id = claim.save_id
           AND contract.run_revision = claim.run_revision
        INNER JOIN insurance_product_coverage AS coverage
            ON coverage.id = NEW.coverage_id
           AND coverage.life_component_version_id
                = contract.insurance_component_version_id
           AND coverage.product_version_id = contract.product_version_id
        INNER JOIN life_event_instance AS instance
            ON instance.id = claim.life_event_instance_id
           AND instance.save_id = claim.save_id
           AND instance.run_revision = claim.run_revision
        INNER JOIN life_event_definition AS definition
            ON definition.id = instance.life_event_definition_id
           AND definition.life_component_version_id
                = instance.life_event_component_version_id
        WHERE claim.id = NEW.claim_id
          AND claim.save_id = NEW.save_id
          AND claim.run_revision = NEW.run_revision
          AND claim.status = 'candidate'
          AND contract.status = 'active'
          AND NEW.insurance_component_version_id
                = contract.insurance_component_version_id
          AND NEW.product_version_id = contract.product_version_id
          AND coverage.event_key = definition.event_key
          AND EXISTS (
              SELECT 1 FROM life_event_choice AS choice_row
              WHERE choice_row.life_event_definition_id = definition.id
                AND choice_row.effect_kind = coverage.effect_kind
          )
          AND contract.start_game_day <= claim.offered_game_day
          AND claim.offered_game_day < contract.coverage_end_exclusive
          AND NEW.coverage_start_game_day = contract.coverage_start_game_day
          AND NEW.waiting_ends_game_day = contract.waiting_ends_game_day
          AND NEW.coverage_end_exclusive = contract.coverage_end_exclusive
          AND NEW.waiting_satisfied
                = (claim.offered_game_day >= contract.waiting_ends_game_day)
          AND NEW.deductible_krw = coverage.deductible_krw
          AND NEW.occurrence_limit_krw = coverage.occurrence_limit_krw
          AND NEW.term_limit_krw = coverage.term_limit_krw
          AND NEW.paid_term_krw_at_offer = contract.paid_term_krw
          AND NEW.reserved_term_krw_at_offer = contract.reserved_term_krw
          AND NEW.pin_order = (
              SELECT COUNT(*) + 1
              FROM insurance_claim_contract_pin AS prior_pin
              WHERE prior_pin.save_id = claim.save_id
                AND prior_pin.run_revision = claim.run_revision
                AND prior_pin.claim_id = claim.id
          )
          AND NOT EXISTS (
              SELECT 1 FROM insurance_claim_contract_pin AS prior_pin
              WHERE prior_pin.save_id = claim.save_id
                AND prior_pin.run_revision = claim.run_revision
                AND prior_pin.claim_id = claim.id
                AND prior_pin.contract_id >= NEW.contract_id
          )
          AND NEW.pin_order <= claim.contract_pin_count
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_insurance_claim_pin_no_update
BEFORE UPDATE ON insurance_claim_contract_pin
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insurance claim contract pins are immutable';

CREATE TRIGGER tr_insurance_claim_pin_no_delete
BEFORE DELETE ON insurance_claim_contract_pin
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insurance claim contract pins are immutable';

CREATE TRIGGER tr_insurance_claim_allocation_valid_insert
BEFORE INSERT ON insurance_claim_allocation
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM insurance_claim AS claim
        INNER JOIN insurance_claim_contract_pin AS pin
            ON pin.id = NEW.claim_contract_pin_id
           AND pin.save_id = claim.save_id
           AND pin.run_revision = claim.run_revision
           AND pin.claim_id = claim.id
        INNER JOIN insurance_contract AS contract
            ON contract.id = pin.contract_id
           AND contract.save_id = pin.save_id
           AND contract.run_revision = pin.run_revision
        WHERE claim.id = NEW.claim_id
          AND claim.save_id = NEW.save_id
          AND claim.run_revision = NEW.run_revision
          AND claim.status = 'candidate'
          AND NEW.contract_id = pin.contract_id
          AND pin.waiting_satisfied = TRUE
          AND contract.reserved_term_krw = NEW.reserved_term_before_krw
          AND NEW.allocation_order = (
              SELECT COUNT(*) + 1
              FROM insurance_claim_allocation AS prior_allocation
              WHERE prior_allocation.save_id = claim.save_id
                AND prior_allocation.run_revision = claim.run_revision
                AND prior_allocation.claim_id = claim.id
          )
          AND NOT EXISTS (
              SELECT 1 FROM insurance_claim_allocation AS prior_allocation
              WHERE prior_allocation.save_id = claim.save_id
                AND prior_allocation.run_revision = claim.run_revision
                AND prior_allocation.claim_id = claim.id
                AND prior_allocation.contract_id >= NEW.contract_id
          )
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_insurance_claim_allocation_no_update
BEFORE UPDATE ON insurance_claim_allocation
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insurance claim allocations are immutable';

CREATE TRIGGER tr_insurance_claim_allocation_no_delete
BEFORE DELETE ON insurance_claim_allocation
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insurance claim allocations are immutable';

CREATE TRIGGER tr_insurance_claim_transition_valid_insert
BEFORE INSERT ON insurance_claim_transition
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1 FROM insurance_claim AS claim
        WHERE claim.id = NEW.claim_id
          AND claim.save_id = NEW.save_id
          AND claim.run_revision = NEW.run_revision
          AND (
              (NEW.transition_no = 1 AND claim.status = 'candidate'
               AND NEW.transition_game_day = claim.offered_game_day)
              OR (NEW.transition_no = 2 AND claim.status = 'candidate'
                  AND NEW.transition_game_day IS NOT NULL
                  AND EXISTS (
                      SELECT 1 FROM insurance_claim_transition AS offered_transition
                      WHERE offered_transition.save_id = claim.save_id
                        AND offered_transition.run_revision = claim.run_revision
                        AND offered_transition.claim_id = claim.id
                        AND offered_transition.transition_no = 1
                        AND offered_transition.to_status = 'candidate'
                  ))
              OR (NEW.transition_no = 3 AND claim.status = 'ready'
                  AND (
                      (NEW.to_status = 'paid'
                       AND NEW.transition_game_day < claim.filing_deadline_game_day)
                      OR (NEW.to_status = 'expired'
                          AND NEW.transition_game_day = claim.filing_deadline_game_day)
                  ))
          )
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_insurance_claim_transition_no_update
BEFORE UPDATE ON insurance_claim_transition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insurance claim transitions are immutable';

CREATE TRIGGER tr_insurance_claim_transition_no_delete
BEFORE DELETE ON insurance_claim_transition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insurance claim transitions are immutable';

CREATE TRIGGER tr_insurance_receipt_valid_insert
BEFORE INSERT ON insurance_command_receipt
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM command_identity AS identity
        INNER JOIN run_rule_bundle AS bundle
            ON bundle.save_id = identity.save_id
           AND bundle.run_revision = NEW.run_revision
        INNER JOIN life_catalog_set AS catalog
            ON catalog.id = bundle.life_catalog_set_id
           AND catalog.insurance_component_version_id
                = NEW.insurance_component_version_id
        WHERE identity.save_id = NEW.save_id
          AND BINARY identity.command_id = BINARY NEW.command_id
          AND BINARY identity.command_kind = BINARY NEW.command_kind
          AND BINARY identity.payload_sha256 = BINARY NEW.payload_sha256
          AND identity.initial_run_revision = NEW.run_revision
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_insurance_receipt_no_update
BEFORE UPDATE ON insurance_command_receipt
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insurance command receipts are immutable';

CREATE TRIGGER tr_insurance_receipt_no_delete
BEFORE DELETE ON insurance_command_receipt
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insurance command receipts are immutable';

-- Insurance accounting uses only two exact sources and two new accounts. Source/reference
-- triggers bind every posting to the authoritative charge or ready claim before projection.
ALTER TABLE ledger_transaction
    ADD CONSTRAINT ck_ledger_transaction_insurance_source CHECK (
        source_kind NOT LIKE 'insurance%'
        OR source_kind IN ('insurancePremiumPayment', 'insuranceClaimPayment')
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
            'welfareBenefitIncome', 'lifeEventExpense',
            'insurancePremiumExpense', 'insuranceClaimRecovery'
        )
    );

CREATE TRIGGER tr_ledger_transaction_insurance_source_insert
BEFORE INSERT ON ledger_transaction
FOR EACH ROW
FOLLOWS tr_ledger_transaction_life_event_source_insert
SET NEW.source_kind = IF(
    NEW.source_kind NOT IN ('insurancePremiumPayment', 'insuranceClaimPayment')
        OR (
            NEW.source_id REGEXP '^[1-9][0-9]{0,19}$'
            AND (
                (NEW.source_kind = 'insurancePremiumPayment'
                 AND EXISTS (
                     SELECT 1
                     FROM insurance_premium_charge AS charge
                     INNER JOIN insurance_contract AS contract
                         ON contract.id = charge.contract_id
                        AND contract.save_id = charge.save_id
                        AND contract.run_revision = charge.run_revision
                     INNER JOIN run_rule_bundle AS bundle
                         ON bundle.save_id = contract.save_id
                        AND bundle.run_revision = contract.run_revision
                     WHERE BINARY CAST(charge.id AS CHAR) = BINARY NEW.source_id
                       AND charge.save_id = NEW.save_id
                       AND charge.run_revision = NEW.run_revision
                       AND charge.status = 'scheduled'
                       AND charge.due_game_day = NEW.game_day
                       AND contract.status IN ('pending', 'active')
                       AND bundle.policy_set_id = NEW.policy_set_id
                 ))
                OR (NEW.source_kind = 'insuranceClaimPayment'
                    AND EXISTS (
                        SELECT 1
                        FROM insurance_claim AS claim
                        INNER JOIN run_rule_bundle AS bundle
                            ON bundle.save_id = claim.save_id
                           AND bundle.run_revision = claim.run_revision
                        WHERE BINARY CAST(claim.id AS CHAR) = BINARY NEW.source_id
                          AND claim.save_id = NEW.save_id
                          AND claim.run_revision = NEW.run_revision
                          AND claim.status = 'ready'
                          AND NEW.game_day < claim.filing_deadline_game_day
                          AND bundle.policy_set_id = NEW.policy_set_id
                    ))
            )
        ),
    NEW.source_kind,
    NULL
);

CREATE TRIGGER tr_ledger_posting_insurance_reference_insert
BEFORE INSERT ON ledger_posting
FOR EACH ROW
FOLLOWS tr_ledger_posting_life_event_reference_insert
SET NEW.account_code = IF(
    EXISTS (
        SELECT 1
        FROM ledger_transaction AS ledger
        LEFT JOIN insurance_premium_charge AS charge
            ON ledger.source_kind = 'insurancePremiumPayment'
           AND BINARY CAST(charge.id AS CHAR) = BINARY ledger.source_id
           AND charge.save_id = ledger.save_id
           AND charge.run_revision = ledger.run_revision
        LEFT JOIN insurance_claim AS claim
            ON ledger.source_kind = 'insuranceClaimPayment'
           AND BINARY CAST(claim.id AS CHAR) = BINARY ledger.source_id
           AND claim.save_id = ledger.save_id
           AND claim.run_revision = ledger.run_revision
        WHERE ledger.id = NEW.ledger_transaction_id
          AND ledger.save_id = NEW.save_id
          AND ledger.run_revision = NEW.run_revision
          AND (
              (ledger.source_kind = 'insurancePremiumPayment'
               AND charge.status = 'scheduled'
               AND (
                   (NEW.posting_order = 1
                    AND NEW.account_code = 'insurancePremiumExpense'
                    AND NEW.amount_krw = charge.amount_krw)
                   OR (NEW.posting_order = 2 AND NEW.account_code = 'wallet'
                       AND NEW.amount_krw = -charge.amount_krw)
               ))
              OR (ledger.source_kind = 'insuranceClaimPayment'
                  AND claim.status = 'ready'
                  AND (
                      (NEW.posting_order = 1 AND NEW.account_code = 'wallet'
                       AND NEW.amount_krw = claim.payout_krw)
                      OR (NEW.posting_order = 2
                          AND NEW.account_code = 'insuranceClaimRecovery'
                          AND NEW.amount_krw = -claim.payout_krw)
                  ))
              OR (ledger.source_kind NOT IN (
                      'insurancePremiumPayment', 'insuranceClaimPayment'
                  )
                  AND NEW.account_code NOT IN (
                      'insurancePremiumExpense', 'insuranceClaimRecovery'
                  ))
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
            'propertyTaxPayment', 'welfareBenefitPayment', 'insurancePremium'
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
            'welfarePayment', 'insuranceContract'
        )
    ),
    ADD CONSTRAINT ck_scheduled_settlement_insurance_payload CHECK (
        kind <> 'insurancePremium'
        OR (
            JSON_TYPE(payload) = 'OBJECT'
            AND JSON_LENGTH(payload) = 4
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.version')) = 'INTEGER'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.version')) = '1'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.insuranceContractId')) = 'STRING'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.insuranceContractId'))
                REGEXP '^[1-9][0-9]{0,19}$'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.premiumChargeId')) = 'STRING'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.premiumChargeId'))
                REGEXP '^[1-9][0-9]{0,19}$'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.chargeNo')) = 'INTEGER'
            AND CAST(JSON_UNQUOTE(JSON_EXTRACT(payload, '$.chargeNo')) AS UNSIGNED)
                BETWEEN 2 AND 12
            AND source_kind = 'insuranceContract'
            AND BINARY source_id = BINARY JSON_UNQUOTE(
                JSON_EXTRACT(payload, '$.insuranceContractId')
            )
            AND occurrence = CAST(
                JSON_UNQUOTE(JSON_EXTRACT(payload, '$.chargeNo')) AS UNSIGNED
            )
        )
    );

CREATE TRIGGER tr_scheduled_settlement_insurance_insert
BEFORE INSERT ON scheduled_settlement
FOR EACH ROW
FOLLOWS tr_scheduled_settlement_welfare_insert
SET NEW.status = IF(
    NEW.kind <> 'insurancePremium'
        OR EXISTS (
            SELECT 1
            FROM insurance_premium_charge AS charge
            INNER JOIN insurance_contract AS contract
                ON contract.id = charge.contract_id
               AND contract.save_id = charge.save_id
               AND contract.run_revision = charge.run_revision
            WHERE charge.id = CAST(
                JSON_UNQUOTE(JSON_EXTRACT(NEW.payload, '$.premiumChargeId')) AS UNSIGNED
            )
              AND charge.save_id = NEW.save_id
              AND charge.run_revision = NEW.run_revision
              AND charge.contract_id = CAST(
                  JSON_UNQUOTE(JSON_EXTRACT(NEW.payload, '$.insuranceContractId'))
                  AS UNSIGNED
              )
              AND charge.charge_no = NEW.occurrence
              AND charge.due_game_day = NEW.due_game_day
              AND charge.status = 'scheduled'
              AND charge.scheduled_settlement_id IS NULL
              AND contract.status IN ('pending', 'active')
        ),
    NEW.status,
    NULL
);

CREATE TRIGGER tr_scheduled_settlement_insurance_transition
BEFORE UPDATE ON scheduled_settlement
FOR EACH ROW
FOLLOWS tr_scheduled_settlement_welfare_transition
SET NEW.status = IF(
    OLD.kind <> 'insurancePremium'
        OR (
            NEW.status = 'settled'
            AND EXISTS (
                SELECT 1 FROM insurance_premium_charge AS charge
                WHERE charge.id = CAST(
                    JSON_UNQUOTE(JSON_EXTRACT(OLD.payload, '$.premiumChargeId'))
                    AS UNSIGNED
                )
                  AND charge.save_id = OLD.save_id
                  AND charge.run_revision = OLD.run_revision
                  AND charge.status = 'paid'
                  AND charge.scheduled_settlement_id = OLD.id
                  AND charge.ledger_transaction_id = NEW.settled_ledger_transaction_id
            )
        )
        OR (
            NEW.status = 'cancelled'
            AND NEW.cancellation_ledger_transaction_id IS NULL
            AND EXISTS (
                SELECT 1 FROM insurance_premium_charge AS charge
                WHERE charge.id = CAST(
                    JSON_UNQUOTE(JSON_EXTRACT(OLD.payload, '$.premiumChargeId'))
                    AS UNSIGNED
                )
                  AND charge.save_id = OLD.save_id
                  AND charge.run_revision = OLD.run_revision
                  AND charge.scheduled_settlement_id = OLD.id
                  AND (
                      (charge.status = 'missed'
                       AND NEW.cancellation_reason = 'insurancePremiumMissed')
                      OR (charge.status = 'cancelled'
                          AND BINARY NEW.cancellation_reason
                                = BINARY charge.terminal_reason)
                  )
            )
        ),
    NEW.status,
    NULL
);

-- D3 needs a second occurrence after enrollment. Extend the publisher before cloning the sealed
-- D2 graph byte-for-byte and changing only the documented occurrence and cooldown fields.
DROP TRIGGER tr_life_component_version_life_event_publish;

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
        )
        OR (
            OLD.version_key = 'dev-unranked-m4-life-event-2026-v2'
            AND OLD.ranked_eligible = FALSE
            AND (
                SELECT COUNT(*) FROM life_event_fact_definition AS fact
                WHERE fact.life_component_version_id = OLD.id
            ) = 4
            AND NOT EXISTS (
                SELECT 1
                FROM life_event_fact_definition AS target_fact
                LEFT JOIN life_component_version AS source_component
                    ON source_component.component_kind = 'lifeEvent'
                   AND source_component.version_key
                        = 'dev-unranked-m4-life-event-2026-v1'
                LEFT JOIN life_event_fact_definition AS source_fact
                    ON source_fact.life_component_version_id = source_component.id
                   AND source_fact.fact_order = target_fact.fact_order
                WHERE target_fact.life_component_version_id = OLD.id
                  AND (
                      source_fact.id IS NULL
                      OR BINARY source_fact.fact_key <> BINARY target_fact.fact_key
                      OR BINARY source_fact.value_type <> BINARY target_fact.value_type
                      OR BINARY source_fact.unit <> BINARY target_fact.unit
                      OR NOT (BINARY source_fact.enum_schema_key
                              <=> BINARY target_fact.enum_schema_key)
                      OR BINARY source_fact.window_kind <> BINARY target_fact.window_kind
                      OR source_fact.source_schema_version
                            <> target_fact.source_schema_version
                      OR BINARY source_fact.source_kind <> BINARY target_fact.source_kind
                  )
            )
            AND (
                SELECT COUNT(*) FROM life_event_definition AS definition
                WHERE definition.life_component_version_id = OLD.id
            ) = 1
            AND EXISTS (
                SELECT 1
                FROM life_event_definition AS target_definition
                INNER JOIN life_component_version AS source_component
                    ON source_component.component_kind = 'lifeEvent'
                   AND source_component.version_key
                        = 'dev-unranked-m4-life-event-2026-v1'
                INNER JOIN life_event_definition AS source_definition
                    ON source_definition.life_component_version_id = source_component.id
                   AND BINARY source_definition.event_key
                        = BINARY target_definition.event_key
                WHERE target_definition.life_component_version_id = OLD.id
                  AND target_definition.event_order = source_definition.event_order
                  AND BINARY target_definition.display_name
                        = BINARY source_definition.display_name
                  AND BINARY target_definition.purpose = BINARY source_definition.purpose
                  AND BINARY target_definition.ranked_availability
                        = BINARY source_definition.ranked_availability
                  AND BINARY target_definition.eligibility_ast
                        = BINARY source_definition.eligibility_ast
                  AND target_definition.ast_node_count = source_definition.ast_node_count
                  AND target_definition.ast_max_depth = source_definition.ast_max_depth
                  AND target_definition.schema_version = source_definition.schema_version
                  AND target_definition.entropy_stream_version
                        = source_definition.entropy_stream_version
                  AND target_definition.hazard_ppm = source_definition.hazard_ppm
                  AND source_definition.cooldown_game_days = 365
                  AND source_definition.maximum_occurrences = 1
                  AND target_definition.cooldown_game_days = 30
                  AND target_definition.maximum_occurrences = 2
                  AND target_definition.priority = source_definition.priority
                  AND (BINARY target_definition.exclusive_group_key
                       <=> BINARY source_definition.exclusive_group_key)
                  AND target_definition.offer_duration_game_days
                        = source_definition.offer_duration_game_days
                  AND BINARY target_definition.default_choice_key
                        = BINARY source_definition.default_choice_key
                  AND (
                      SELECT COUNT(*) FROM life_event_choice AS target_choice
                      WHERE target_choice.life_event_definition_id = target_definition.id
                  ) = (
                      SELECT COUNT(*) FROM life_event_choice AS source_choice
                      WHERE source_choice.life_event_definition_id = source_definition.id
                  )
                  AND NOT EXISTS (
                      SELECT 1
                      FROM life_event_choice AS target_choice
                      LEFT JOIN life_event_choice AS source_choice
                        ON source_choice.life_event_definition_id = source_definition.id
                       AND source_choice.choice_order = target_choice.choice_order
                      WHERE target_choice.life_event_definition_id = target_definition.id
                        AND (
                            source_choice.id IS NULL
                            OR BINARY source_choice.choice_key
                                <> BINARY target_choice.choice_key
                            OR BINARY source_choice.display_name
                                <> BINARY target_choice.display_name
                            OR BINARY source_choice.decision_kind
                                <> BINARY target_choice.decision_kind
                            OR BINARY source_choice.effect_kind
                                <> BINARY target_choice.effect_kind
                            OR NOT (source_choice.effect_amount_krw
                                    <=> target_choice.effect_amount_krw)
                            OR NOT (BINARY source_choice.effect_account_code
                                    <=> BINARY target_choice.effect_account_code)
                            OR BINARY source_choice.effect_ast
                                <> BINARY target_choice.effect_ast
                        )
                  )
            )
            AND EXISTS (
                SELECT 1
                FROM life_event_component_canonical_projection AS projection
                INNER JOIN life_component_canonical_manifest AS manifest
                    ON manifest.life_component_version_id = projection.life_component_version_id
                   AND BINARY manifest.canonical_json = BINARY projection.canonical_json
                   AND BINARY manifest.canonical_sha256
                        = BINARY SHA2(projection.canonical_json, 256)
                WHERE projection.life_component_version_id = OLD.id
            )
        ),
    NEW.version_key,
    NULL
);

-- sqlx keeps the v2 header outside the snapshot used by these BEFORE INSERT guards even after
-- intervening permanent DDL. Startup has not opened the listener, so suspend only the three
-- draft-child guards for this bounded clone; immutable update/delete guards stay active.
DROP TRIGGER tr_life_event_fact_draft_insert;
DROP TRIGGER tr_life_event_definition_draft_insert;
DROP TRIGGER tr_life_event_choice_draft_insert;

INSERT INTO life_event_fact_definition
    (
        life_component_version_id, fact_order, fact_key, value_type, unit,
        enum_schema_key, window_kind, source_schema_version, source_kind
    )
SELECT target.id, source_fact.fact_order, source_fact.fact_key, source_fact.value_type,
       source_fact.unit, source_fact.enum_schema_key, source_fact.window_kind,
       source_fact.source_schema_version, source_fact.source_kind
FROM life_component_version AS target
INNER JOIN life_component_version AS source_component
    ON source_component.component_kind = 'lifeEvent'
   AND source_component.version_key = 'dev-unranked-m4-life-event-2026-v1'
   AND source_component.sealed_at IS NOT NULL
INNER JOIN life_event_fact_definition AS source_fact
    ON source_fact.life_component_version_id = source_component.id
WHERE target.component_kind = 'lifeEvent'
  AND target.version_key = 'dev-unranked-m4-life-event-2026-v2';

INSERT INTO life_event_definition
    (
        life_component_version_id, schema_version, entropy_stream_version,
        event_order, event_key, display_name, purpose, ranked_availability,
        eligibility_ast, ast_node_count, ast_max_depth, hazard_ppm,
        cooldown_game_days, maximum_occurrences, priority, exclusive_group_key,
        offer_duration_game_days, default_choice_key
    )
SELECT target.id, source_definition.schema_version,
       source_definition.entropy_stream_version, source_definition.event_order,
       source_definition.event_key, source_definition.display_name,
       source_definition.purpose, source_definition.ranked_availability,
       source_definition.eligibility_ast, source_definition.ast_node_count,
       source_definition.ast_max_depth, source_definition.hazard_ppm,
       30, 2, source_definition.priority, source_definition.exclusive_group_key,
       source_definition.offer_duration_game_days, source_definition.default_choice_key
FROM life_component_version AS target
INNER JOIN life_component_version AS source_component
    ON source_component.component_kind = 'lifeEvent'
   AND source_component.version_key = 'dev-unranked-m4-life-event-2026-v1'
   AND source_component.sealed_at IS NOT NULL
INNER JOIN life_event_definition AS source_definition
    ON source_definition.life_component_version_id = source_component.id
WHERE target.component_kind = 'lifeEvent'
  AND target.version_key = 'dev-unranked-m4-life-event-2026-v2';

INSERT INTO life_event_choice
    (
        life_component_version_id, life_event_definition_id, choice_order,
        choice_key, display_name, decision_kind, effect_kind,
        effect_amount_krw, effect_account_code, effect_ast
    )
SELECT target_definition.life_component_version_id,
       target_definition.id,
       source_choice.choice_order,
       source_choice.choice_key,
       source_choice.display_name,
       source_choice.decision_kind,
       source_choice.effect_kind,
       source_choice.effect_amount_krw,
       source_choice.effect_account_code,
       source_choice.effect_ast
FROM life_event_definition AS target_definition
INNER JOIN life_component_version AS target_component
    ON target_component.id = target_definition.life_component_version_id
   AND target_component.version_key = 'dev-unranked-m4-life-event-2026-v2'
INNER JOIN life_component_version AS source_component
    ON source_component.component_kind = 'lifeEvent'
   AND source_component.version_key = 'dev-unranked-m4-life-event-2026-v1'
   AND source_component.sealed_at IS NOT NULL
INNER JOIN life_event_definition AS source_definition
    ON source_definition.life_component_version_id = source_component.id
   AND BINARY source_definition.event_key = BINARY target_definition.event_key
INNER JOIN life_event_choice AS source_choice
    ON source_choice.life_event_definition_id = source_definition.id;

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

INSERT INTO life_component_canonical_manifest
    (life_component_version_id, canonical_json)
SELECT projection.life_component_version_id, projection.canonical_json
FROM life_event_component_canonical_projection AS projection
INNER JOIN life_component_version AS component
    ON component.id = projection.life_component_version_id
WHERE component.component_kind = 'lifeEvent'
  AND component.version_key = 'dev-unranked-m4-life-event-2026-v2'
  AND component.sealed_at IS NULL;

UPDATE life_component_version AS component
INNER JOIN life_component_canonical_manifest AS manifest
    ON manifest.life_component_version_id = component.id
SET component.canonical_sha256 = manifest.canonical_sha256,
    component.sealed_at = CURRENT_TIMESTAMP(3)
WHERE component.component_kind = 'lifeEvent'
  AND component.version_key = 'dev-unranked-m4-life-event-2026-v2'
  AND component.sealed_at IS NULL;

INSERT INTO insurance_fact_definition
    (
        life_component_version_id, fact_order, fact_key,
        value_type, unit, enum_schema_key, window_kind,
        source_schema_version, source_kind
    )
SELECT component.id, seed.fact_order, seed.fact_key, seed.value_type, seed.unit,
       seed.enum_schema_key, 'currentGameDay', 1, seed.source_kind
FROM life_component_version AS component
INNER JOIN (
    SELECT 1 AS fact_order, 'character.age' AS fact_key,
           'ageYears' AS value_type, 'years' AS unit,
           CAST(NULL AS CHAR(64)) AS enum_schema_key, 'gameDay' AS source_kind
    UNION ALL SELECT 2, 'household.dependentCount', 'count', 'count', NULL, 'household'
    UNION ALL SELECT 3, 'residence.exists', 'boolean', 'boolean', NULL, 'residence'
    UNION ALL SELECT 4, 'military.status', 'enum', 'enum', 'military', 'military'
) AS seed ON TRUE
WHERE component.component_kind = 'insurance'
  AND component.version_key = 'dev-unranked-m4-insurance-2026-v1';

INSERT INTO insurance_product_version
    (
        life_component_version_id, schema_version, product_order,
        product_key, display_name, purpose, ranked_availability,
        eligibility_ast, ast_node_count, ast_max_depth,
        premium_krw, premium_cadence_game_days, term_game_days,
        waiting_game_days, claim_window_game_days, grace_game_days,
        reinstatement_allowed
    )
SELECT component.id, 1, 1,
       'fictionalFamilyCareCover', '가족 돌봄 비용 보장',
       'gameBalance', 'unrankedOnly',
       JSON_OBJECT(
           'version', 1,
           'kind', 'all',
           'children', JSON_ARRAY(
               JSON_OBJECT(
                   'kind', 'between',
                   'value', JSON_OBJECT(
                       'kind', 'fact', 'path', 'character.age', 'unit', 'years',
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
                       'kind', 'fact', 'path', 'household.dependentCount',
                       'unit', 'count',
                       'window', JSON_OBJECT('kind', 'currentGameDay')
                   ),
                   'right', JSON_OBJECT(
                       'kind', 'literal', 'valueType', 'count',
                       'unit', 'count', 'value', 1
                   )
               ),
               JSON_OBJECT(
                   'kind', 'fact', 'path', 'residence.exists', 'unit', 'boolean',
                   'window', JSON_OBJECT('kind', 'currentGameDay')
               ),
               JSON_OBJECT(
                   'kind', 'not',
                   'child', JSON_OBJECT(
                       'kind', 'eq',
                       'left', JSON_OBJECT(
                           'kind', 'fact', 'path', 'military.status', 'unit', 'enum',
                           'window', JSON_OBJECT('kind', 'currentGameDay')
                       ),
                       'right', JSON_OBJECT(
                           'kind', 'literal', 'valueType', 'enum', 'unit', 'enum',
                           'schemaKey', 'military', 'value', 'serving'
                       )
                   )
               )
           )
       ),
       13, 4, 10000, 30, 360, 7, 7, 0, FALSE
FROM life_component_version AS component
WHERE component.component_kind = 'insurance'
  AND component.version_key = 'dev-unranked-m4-insurance-2026-v1';

INSERT INTO insurance_product_coverage
    (
        life_component_version_id, product_version_id, coverage_order,
        coverage_kind, event_key, effect_kind, deductible_krw,
        occurrence_limit_krw, term_limit_krw
    )
SELECT product.life_component_version_id, product.id, 1,
       'fixedIndemnity', 'fictionalDependentCareRequest', 'fixedWalletExpense',
       20000, 100000, 200000
FROM insurance_product_version AS product
INNER JOIN life_component_version AS component
    ON component.id = product.life_component_version_id
WHERE component.component_kind = 'insurance'
  AND component.version_key = 'dev-unranked-m4-insurance-2026-v1'
  AND product.product_key = 'fictionalFamilyCareCover';

CREATE VIEW insurance_component_canonical_projection AS
SELECT component.id AS life_component_version_id,
       CAST(JSON_OBJECT(
           'availability', component.availability,
           'componentKind', component.component_kind,
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
               FROM insurance_fact_definition AS fact
               WHERE fact.life_component_version_id = component.id
           ),
           'productsCanonical', (
               SELECT GROUP_CONCAT(
                   CAST(JSON_OBJECT(
                       'astMaxDepth', product.ast_max_depth,
                       'astNodeCount', product.ast_node_count,
                       'claimWindowGameDays', product.claim_window_game_days,
                       'coveragesCanonical', (
                           SELECT GROUP_CONCAT(
                               CAST(JSON_OBJECT(
                                   'coverageKind', coverage.coverage_kind,
                                   'coverageOrder', coverage.coverage_order,
                                   'deductibleKrw', coverage.deductible_krw,
                                   'effectKind', coverage.effect_kind,
                                   'eventKey', coverage.event_key,
                                   'occurrenceLimitKrw', coverage.occurrence_limit_krw,
                                   'termLimitKrw', coverage.term_limit_krw
                               ) AS CHAR CHARACTER SET utf8mb4)
                               ORDER BY coverage.coverage_order SEPARATOR '\n'
                           )
                           FROM insurance_product_coverage AS coverage
                           WHERE coverage.product_version_id = product.id
                       ),
                       'displayName', product.display_name,
                       'eligibilityAst', product.eligibility_ast,
                       'graceGameDays', product.grace_game_days,
                       'premiumCadenceGameDays', product.premium_cadence_game_days,
                       'premiumKrw', product.premium_krw,
                       'productKey', product.product_key,
                       'productOrder', product.product_order,
                       'purpose', product.purpose,
                       'rankedAvailability', product.ranked_availability,
                       'reinstatementAllowed', product.reinstatement_allowed,
                       'schemaVersion', product.schema_version,
                       'termGameDays', product.term_game_days,
                       'waitingGameDays', product.waiting_game_days
                   ) AS CHAR CHARACTER SET utf8mb4)
                   ORDER BY product.product_order SEPARATOR '\n'
               )
               FROM insurance_product_version AS product
               WHERE product.life_component_version_id = component.id
           ),
           'rankedEligible', component.ranked_eligible,
           'schemaVersion', 1,
           'versionKey', component.version_key
       ) AS CHAR CHARACTER SET utf8mb4) AS canonical_json
FROM life_component_version AS component
WHERE component.component_kind = 'insurance'
  AND component.availability = 'active';

CREATE TRIGGER tr_life_component_version_insurance_publish
BEFORE UPDATE ON life_component_version
FOR EACH ROW
FOLLOWS tr_life_component_version_life_event_publish
SET NEW.version_key = IF(
    NEW.component_kind <> 'insurance'
        OR NEW.availability <> 'active'
        OR (
            OLD.version_key = 'dev-unranked-m4-insurance-2026-v1'
            AND OLD.ranked_eligible = FALSE
            AND (
                SELECT COUNT(*) FROM insurance_fact_definition AS fact
                WHERE fact.life_component_version_id = OLD.id
            ) = 4
            AND (
                SELECT SUM(
                    CASE
                        WHEN fact.fact_order = 1 AND fact.fact_key = 'character.age'
                             AND fact.value_type = 'ageYears' AND fact.unit = 'years'
                             AND fact.enum_schema_key IS NULL
                             AND fact.source_kind = 'gameDay' THEN 1
                        WHEN fact.fact_order = 2
                             AND fact.fact_key = 'household.dependentCount'
                             AND fact.value_type = 'count' AND fact.unit = 'count'
                             AND fact.enum_schema_key IS NULL
                             AND fact.source_kind = 'household' THEN 1
                        WHEN fact.fact_order = 3 AND fact.fact_key = 'residence.exists'
                             AND fact.value_type = 'boolean' AND fact.unit = 'boolean'
                             AND fact.enum_schema_key IS NULL
                             AND fact.source_kind = 'residence' THEN 1
                        WHEN fact.fact_order = 4 AND fact.fact_key = 'military.status'
                             AND fact.value_type = 'enum' AND fact.unit = 'enum'
                             AND fact.enum_schema_key = 'military'
                             AND fact.source_kind = 'military' THEN 1
                        ELSE 0
                    END
                ) FROM insurance_fact_definition AS fact
                WHERE fact.life_component_version_id = OLD.id
                  AND fact.window_kind = 'currentGameDay'
                  AND fact.source_schema_version = 1
            ) = 4
            AND (
                SELECT COUNT(*) FROM insurance_product_version AS product
                WHERE product.life_component_version_id = OLD.id
            ) BETWEEN 1 AND 16
            AND EXISTS (
                SELECT 1
                FROM insurance_product_version AS product
                WHERE product.life_component_version_id = OLD.id
                  AND product.product_order = 1
                  AND product.product_key = 'fictionalFamilyCareCover'
                  AND product.display_name = '가족 돌봄 비용 보장'
                  AND product.schema_version = 1
                  AND product.purpose = 'gameBalance'
                  AND product.ranked_availability = 'unrankedOnly'
                  AND product.ast_node_count = 13 AND product.ast_max_depth = 4
                  AND product.premium_krw = 10000
                  AND product.premium_cadence_game_days = 30
                  AND product.term_game_days = 360
                  AND product.waiting_game_days = 7
                  AND product.claim_window_game_days = 7
                  AND product.grace_game_days = 0
                  AND product.reinstatement_allowed = FALSE
                  AND JSON_LENGTH(product.eligibility_ast) = 3
                  AND JSON_LENGTH(JSON_EXTRACT(product.eligibility_ast, '$.children')) = 4
                  AND JSON_UNQUOTE(JSON_EXTRACT(
                      product.eligibility_ast, '$.children[0].lower.value'
                  )) = '22'
                  AND JSON_UNQUOTE(JSON_EXTRACT(
                      product.eligibility_ast, '$.children[0].upper.value'
                  )) = '67'
                  AND JSON_UNQUOTE(JSON_EXTRACT(
                      product.eligibility_ast, '$.children[1].right.value'
                  )) = '1'
                  AND JSON_UNQUOTE(JSON_EXTRACT(
                      product.eligibility_ast, '$.children[2].path'
                  )) = 'residence.exists'
                  AND JSON_UNQUOTE(JSON_EXTRACT(
                      product.eligibility_ast, '$.children[3].child.right.value'
                  )) = 'serving'
                  AND (
                      SELECT COUNT(*) FROM insurance_product_coverage AS coverage
                      WHERE coverage.product_version_id = product.id
                  ) = 1
                  AND EXISTS (
                      SELECT 1 FROM insurance_product_coverage AS coverage
                      WHERE coverage.product_version_id = product.id
                        AND coverage.coverage_order = 1
                        AND coverage.coverage_kind = 'fixedIndemnity'
                        AND coverage.event_key = 'fictionalDependentCareRequest'
                        AND coverage.effect_kind = 'fixedWalletExpense'
                        AND coverage.deductible_krw = 20000
                        AND coverage.occurrence_limit_krw = 100000
                        AND coverage.term_limit_krw = 200000
                  )
            )
            AND EXISTS (
                SELECT 1
                FROM insurance_component_canonical_projection AS projection
                INNER JOIN life_component_canonical_manifest AS manifest
                    ON manifest.life_component_version_id = projection.life_component_version_id
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
FROM insurance_component_canonical_projection AS projection
INNER JOIN life_component_version AS component
    ON component.id = projection.life_component_version_id
WHERE component.component_kind = 'insurance'
  AND component.version_key = 'dev-unranked-m4-insurance-2026-v1'
  AND component.sealed_at IS NULL;

UPDATE life_component_version AS component
INNER JOIN life_component_canonical_manifest AS manifest
    ON manifest.life_component_version_id = component.id
SET component.canonical_sha256 = manifest.canonical_sha256,
    component.sealed_at = CURRENT_TIMESTAMP(3)
WHERE component.component_kind = 'insurance'
  AND component.version_key = 'dev-unranked-m4-insurance-2026-v1'
  AND component.sealed_at IS NULL;

-- An active insurance component is publishable only with the exact event graph that supplies
-- every covered event/effect pair. This prevents combining individually valid but incompatible
-- sealed components in a later aggregate.
CREATE TRIGGER tr_life_catalog_set_insurance_compatibility
BEFORE UPDATE ON life_catalog_set
FOR EACH ROW
FOLLOWS tr_life_catalog_set_seal_only
SET NEW.catalog_key = IF(
    NOT (OLD.sealed_at IS NULL AND NEW.sealed_at IS NOT NULL)
        OR EXISTS (
            SELECT 1 FROM life_component_version AS insurance_component
            WHERE insurance_component.id = OLD.insurance_component_version_id
              AND insurance_component.component_kind = 'insurance'
              AND insurance_component.availability = 'disabled'
        )
        OR (
            EXISTS (
                SELECT 1
                FROM life_component_version AS insurance_component
                INNER JOIN life_component_version AS event_component
                    ON event_component.id = OLD.life_event_component_version_id
                WHERE insurance_component.id = OLD.insurance_component_version_id
                  AND insurance_component.component_kind = 'insurance'
                  AND insurance_component.version_key
                        = 'dev-unranked-m4-insurance-2026-v1'
                  AND insurance_component.availability = 'active'
                  AND insurance_component.sealed_at IS NOT NULL
                  AND event_component.component_kind = 'lifeEvent'
                  AND event_component.version_key
                        = 'dev-unranked-m4-life-event-2026-v2'
                  AND event_component.availability = 'active'
                  AND event_component.sealed_at IS NOT NULL
            )
            AND NOT EXISTS (
                SELECT 1
                FROM insurance_product_coverage AS coverage
                WHERE coverage.life_component_version_id
                        = OLD.insurance_component_version_id
                  AND NOT EXISTS (
                      SELECT 1
                      FROM life_event_definition AS definition
                      INNER JOIN life_event_choice AS choice_row
                          ON choice_row.life_event_definition_id = definition.id
                         AND choice_row.life_component_version_id
                                = definition.life_component_version_id
                      WHERE definition.life_component_version_id
                                = OLD.life_event_component_version_id
                        AND BINARY definition.event_key = BINARY coverage.event_key
                        AND BINARY choice_row.effect_kind = BINARY coverage.effect_kind
                  )
            )
        ),
    NEW.catalog_key,
    NULL
);

-- Clone the sealed D2 aggregate, replacing only event v2 and insurance v1.
INSERT INTO life_catalog_set
    (
        catalog_key, ranked_eligible, legacy_dependent_age_years,
        living_cost_component_version_id, welfare_component_version_id,
        life_event_component_version_id, insurance_component_version_id,
        corporation_component_version_id
    )
SELECT 'dev-unranked-m4-life-catalog-2026-v4',
       FALSE,
       previous.legacy_dependent_age_years,
       previous.living_cost_component_version_id,
       previous.welfare_component_version_id,
       event_component.id,
       insurance_component.id,
       previous.corporation_component_version_id
FROM m4d3_previous_new_run_assignment AS previous
INNER JOIN life_catalog_set AS previous_catalog
    ON previous_catalog.id = previous.life_catalog_set_id
   AND previous_catalog.catalog_key = 'dev-unranked-m4-life-catalog-2026-v3'
   AND previous_catalog.sealed_at IS NOT NULL
INNER JOIN life_component_version AS previous_event
    ON previous_event.id = previous.life_event_component_version_id
   AND previous_event.version_key = 'dev-unranked-m4-life-event-2026-v1'
   AND previous_event.availability = 'active'
   AND previous_event.sealed_at IS NOT NULL
INNER JOIN life_component_version AS previous_insurance
    ON previous_insurance.id = previous.insurance_component_version_id
   AND previous_insurance.version_key = 'disabled-m4a-v1'
   AND previous_insurance.availability = 'disabled'
   AND previous_insurance.sealed_at IS NOT NULL
INNER JOIN life_component_version AS event_component
    ON event_component.component_kind = 'lifeEvent'
   AND event_component.version_key = 'dev-unranked-m4-life-event-2026-v2'
   AND event_component.availability = 'active'
   AND event_component.ranked_eligible = FALSE
   AND event_component.sealed_at IS NOT NULL
INNER JOIN life_component_version AS insurance_component
    ON insurance_component.component_kind = 'insurance'
   AND insurance_component.version_key = 'dev-unranked-m4-insurance-2026-v1'
   AND insurance_component.availability = 'active'
   AND insurance_component.ranked_eligible = FALSE
   AND insurance_component.sealed_at IS NOT NULL
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
WHERE catalog_key = 'dev-unranked-m4-life-catalog-2026-v4'
  AND sealed_at IS NULL;

CREATE TEMPORARY TABLE m4d3_publication_guard (
    guard_key VARCHAR(48) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    accepted  TINYINT UNSIGNED NOT NULL,
    CONSTRAINT ck_m4d3_publication_guard CHECK (accepted = 1)
) ENGINE = InnoDB;

INSERT INTO m4d3_publication_guard (guard_key, accepted)
SELECT 'sealed-event-v2', IF(
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
          AND component.version_key = 'dev-unranked-m4-life-event-2026-v2'
          AND component.availability = 'active'
          AND component.ranked_eligible = FALSE
          AND component.sealed_at IS NOT NULL
          AND BINARY component.canonical_sha256 = BINARY manifest.canonical_sha256
          AND BINARY manifest.canonical_json = BINARY projection.canonical_json
          AND BINARY manifest.canonical_sha256
                = BINARY SHA2(projection.canonical_json, 256)
          AND definition.event_key = 'fictionalDependentCareRequest'
          AND definition.hazard_ppm = 1000000
          AND definition.cooldown_game_days = 30
          AND definition.maximum_occurrences = 2
          AND definition.offer_duration_game_days = 7
          AND (SELECT COUNT(*) FROM life_event_fact_definition AS fact
               WHERE fact.life_component_version_id = component.id) = 4
          AND (SELECT COUNT(*) FROM life_event_choice AS choice_row
               WHERE choice_row.life_event_definition_id = definition.id) = 2
    ),
    1,
    0
);

INSERT INTO m4d3_publication_guard (guard_key, accepted)
SELECT 'sealed-insurance-v1', IF(
    EXISTS (
        SELECT 1
        FROM life_component_version AS component
        INNER JOIN life_component_canonical_manifest AS manifest
            ON manifest.life_component_version_id = component.id
        INNER JOIN insurance_component_canonical_projection AS projection
            ON projection.life_component_version_id = component.id
        INNER JOIN insurance_product_version AS product
            ON product.life_component_version_id = component.id
        INNER JOIN insurance_product_coverage AS coverage
            ON coverage.product_version_id = product.id
        WHERE component.component_kind = 'insurance'
          AND component.version_key = 'dev-unranked-m4-insurance-2026-v1'
          AND component.availability = 'active'
          AND component.ranked_eligible = FALSE
          AND component.sealed_at IS NOT NULL
          AND BINARY component.canonical_sha256 = BINARY manifest.canonical_sha256
          AND BINARY manifest.canonical_json = BINARY projection.canonical_json
          AND BINARY manifest.canonical_sha256
                = BINARY SHA2(projection.canonical_json, 256)
          AND product.product_key = 'fictionalFamilyCareCover'
          AND product.premium_krw = 10000
          AND product.premium_cadence_game_days = 30
          AND product.term_game_days = 360
          AND product.waiting_game_days = 7
          AND product.claim_window_game_days = 7
          AND coverage.event_key = 'fictionalDependentCareRequest'
          AND coverage.effect_kind = 'fixedWalletExpense'
          AND coverage.deductible_krw = 20000
          AND coverage.occurrence_limit_krw = 100000
          AND coverage.term_limit_krw = 200000
          AND (SELECT COUNT(*) FROM insurance_fact_definition AS fact
               WHERE fact.life_component_version_id = component.id) = 4
          AND (SELECT COUNT(*) FROM insurance_product_version AS sibling
               WHERE sibling.life_component_version_id = component.id) = 1
    ),
    1,
    0
);

INSERT INTO m4d3_publication_guard (guard_key, accepted)
SELECT 'sealed-life-catalog-v4', IF(
    EXISTS (
        SELECT 1
        FROM life_catalog_set AS catalog
        INNER JOIN m4d3_previous_new_run_assignment AS previous
            ON previous.assignment_key = 'newRun'
           AND catalog.legacy_dependent_age_years
                = previous.legacy_dependent_age_years
           AND catalog.living_cost_component_version_id
                = previous.living_cost_component_version_id
           AND catalog.welfare_component_version_id
                = previous.welfare_component_version_id
           AND catalog.corporation_component_version_id
                = previous.corporation_component_version_id
        INNER JOIN life_component_version AS event_component
            ON event_component.id = catalog.life_event_component_version_id
        INNER JOIN life_component_version AS insurance_component
            ON insurance_component.id = catalog.insurance_component_version_id
        WHERE catalog.catalog_key = 'dev-unranked-m4-life-catalog-2026-v4'
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
          AND event_component.version_key = 'dev-unranked-m4-life-event-2026-v2'
          AND event_component.sealed_at IS NOT NULL
          AND insurance_component.version_key
                = 'dev-unranked-m4-insurance-2026-v1'
          AND insurance_component.sealed_at IS NOT NULL
    ),
    1,
    0
);

INSERT INTO m4d3_publication_guard (guard_key, accepted)
SELECT 'existing-run-pins-unchanged', IF(
    NOT EXISTS (
        SELECT 1
        FROM m4d3_existing_run_life_pins AS previous
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
            WHERE catalog.catalog_key = 'dev-unranked-m4-life-catalog-2026-v4'
        ),
    1,
    0
);

INSERT INTO m4d3_publication_guard (guard_key, accepted)
SELECT 'insurance-ledger-settlement-protocol', IF(
    EXISTS (
        SELECT 1 FROM information_schema.CHECK_CONSTRAINTS AS constraint_row
        WHERE constraint_row.CONSTRAINT_SCHEMA = DATABASE()
          AND constraint_row.CONSTRAINT_NAME = 'ck_ledger_posting_account_code'
          AND constraint_row.CHECK_CLAUSE LIKE '%insurancePremiumExpense%'
          AND constraint_row.CHECK_CLAUSE LIKE '%insuranceClaimRecovery%'
    )
        AND EXISTS (
            SELECT 1 FROM information_schema.CHECK_CONSTRAINTS AS constraint_row
            WHERE constraint_row.CONSTRAINT_SCHEMA = DATABASE()
              AND constraint_row.CONSTRAINT_NAME = 'ck_scheduled_settlement_kind'
              AND constraint_row.CHECK_CLAUSE LIKE '%insurancePremium%'
        ),
    1,
    0
);

DROP TEMPORARY TABLE m4d3_publication_guard;

-- This is the only external publication mutation in 0040. The assignment trigger revalidates
-- every unchanged market, finance, career, employment, credit, and real-estate pin.
UPDATE run_rule_bundle_assignment AS assignment
INNER JOIN life_catalog_set AS catalog
    ON catalog.catalog_key = 'dev-unranked-m4-life-catalog-2026-v4'
   AND catalog.sealed_at IS NOT NULL
SET assignment.life_catalog_set_id = catalog.id
WHERE assignment.assignment_key = 'newRun';

CREATE TEMPORARY TABLE m4d3_assignment_guard (
    guard_key VARCHAR(48) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    accepted  TINYINT UNSIGNED NOT NULL,
    CONSTRAINT ck_m4d3_assignment_guard CHECK (accepted = 1)
) ENGINE = InnoDB;

INSERT INTO m4d3_assignment_guard (guard_key, accepted)
SELECT 'new-run-event-v2-insurance-v1-only', IF(
    EXISTS (
        SELECT 1
        FROM run_rule_bundle_assignment AS assignment
        INNER JOIN life_catalog_set AS catalog
            ON catalog.id = assignment.life_catalog_set_id
        INNER JOIN life_component_version AS event_component
            ON event_component.id = catalog.life_event_component_version_id
        INNER JOIN life_component_version AS insurance_component
            ON insurance_component.id = catalog.insurance_component_version_id
        INNER JOIN m4d3_previous_new_run_assignment AS previous
            ON previous.assignment_key = assignment.assignment_key
           AND assignment.market_world_id = previous.market_world_id
           AND assignment.policy_set_id = previous.policy_set_id
           AND assignment.career_catalog_bundle_id = previous.career_catalog_bundle_id
           AND assignment.employment_policy_set_id = previous.employment_policy_set_id
           AND assignment.credit_model_version_id = previous.credit_model_version_id
           AND assignment.real_estate_model_version_id = previous.real_estate_model_version_id
           AND assignment.market_assignment_revision = previous.market_assignment_revision
           AND assignment.finance_assignment_revision = previous.finance_assignment_revision
           AND assignment.career_assignment_revision = previous.career_assignment_revision
           AND assignment.employment_assignment_revision
                = previous.employment_assignment_revision
           AND assignment.assignment_revision = previous.assignment_revision + 1
           AND catalog.legacy_dependent_age_years
                = previous.legacy_dependent_age_years
           AND catalog.living_cost_component_version_id
                = previous.living_cost_component_version_id
           AND catalog.welfare_component_version_id
                = previous.welfare_component_version_id
           AND catalog.corporation_component_version_id
                = previous.corporation_component_version_id
        WHERE assignment.assignment_key = 'newRun'
          AND catalog.catalog_key = 'dev-unranked-m4-life-catalog-2026-v4'
          AND catalog.sealed_at IS NOT NULL
          AND event_component.version_key = 'dev-unranked-m4-life-event-2026-v2'
          AND event_component.sealed_at IS NOT NULL
          AND insurance_component.version_key
                = 'dev-unranked-m4-insurance-2026-v1'
          AND insurance_component.sealed_at IS NOT NULL
    )
        AND NOT EXISTS (
            SELECT 1
            FROM m4d3_existing_run_life_pins AS previous
            LEFT JOIN run_rule_bundle AS current_bundle
                ON current_bundle.save_id = previous.save_id
               AND current_bundle.run_revision = previous.run_revision
            WHERE current_bundle.save_id IS NULL
               OR current_bundle.life_catalog_set_id <> previous.life_catalog_set_id
        ),
    1,
    0
);

DROP TEMPORARY TABLE m4d3_assignment_guard;
DROP TEMPORARY TABLE m4d3_existing_run_life_pins;
DROP TEMPORARY TABLE m4d3_previous_new_run_assignment;
