-- M4-E1 cash-only insolvency catalog, policy, runtime, and accounting (§8.1–§8.8).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;
SET SESSION group_concat_max_len = 1048576;

-- Legacy schema-v1 aggregates did not have an insolvency component. Their NULL preserves their
-- sealed bytes; every aggregate inserted after this migration is schema v2 and must pin one.
ALTER TABLE life_component_version
    DROP CHECK ck_life_component_version_kind,
    ADD CONSTRAINT ck_life_component_version_kind CHECK (
        component_kind IN (
            'livingCost', 'welfare', 'lifeEvent', 'insurance', 'insolvency', 'corporation'
        )
    );

ALTER TABLE life_catalog_set
    ADD COLUMN insolvency_component_version_id BIGINT UNSIGNED NULL
        AFTER insurance_component_version_id,
    ADD UNIQUE KEY uk_life_catalog_insolvency_component
        (id, insolvency_component_version_id),
    ADD KEY ix_life_catalog_set_insolvency (insolvency_component_version_id),
    ADD CONSTRAINT fk_life_catalog_set_insolvency
        FOREIGN KEY (insolvency_component_version_id) REFERENCES life_component_version (id);

DROP TRIGGER tr_life_catalog_set_draft_insert;

CREATE TRIGGER tr_life_catalog_set_draft_insert
BEFORE INSERT ON life_catalog_set
FOR EACH ROW
SET NEW.catalog_key = IF(
    NEW.sealed_at IS NULL
        AND NEW.canonical_sha256 IS NULL
        AND NEW.ranked_eligible IN (FALSE, TRUE)
        AND NEW.insolvency_component_version_id IS NOT NULL
        AND EXISTS (
            SELECT 1
            FROM life_component_version AS component
            WHERE component.id = NEW.insolvency_component_version_id
              AND component.component_kind = 'insolvency'
              AND component.sealed_at IS NOT NULL
        ),
    NEW.catalog_key,
    NULL
);

DROP TRIGGER tr_life_catalog_set_seal_only;

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
        AND NEW.insolvency_component_version_id = OLD.insolvency_component_version_id
        AND NEW.corporation_component_version_id = OLD.corporation_component_version_id
        AND NEW.created_at = OLD.created_at
        AND OLD.insolvency_component_version_id IS NOT NULL
        AND NEW.canonical_sha256 REGEXP '^[0-9a-f]{64}$'
        AND NEW.canonical_sha256 = SHA2(
            CAST(JSON_OBJECT(
                'catalogKey', OLD.catalog_key,
                'corporationComponentVersionId',
                    CAST(OLD.corporation_component_version_id AS CHAR),
                'insolvencyComponentVersionId',
                    CAST(OLD.insolvency_component_version_id AS CHAR),
                'insuranceComponentVersionId',
                    CAST(OLD.insurance_component_version_id AS CHAR),
                'lifeEventComponentVersionId',
                    CAST(OLD.life_event_component_version_id AS CHAR),
                'legacyDependentAgeYears', OLD.legacy_dependent_age_years,
                'livingCostComponentVersionId',
                    CAST(OLD.living_cost_component_version_id AS CHAR),
                'schemaVersion', 2,
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
            WHERE id = OLD.insolvency_component_version_id
              AND component_kind = 'insolvency' AND sealed_at IS NOT NULL
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

INSERT INTO life_component_version
    (component_kind, version_key, availability, ranked_eligible)
VALUES
    ('insolvency', 'dev-unranked-m4-insolvency-2026-v1', 'active', FALSE);

CREATE TABLE insolvency_component_profile (
    life_component_version_id           BIGINT UNSIGNED NOT NULL,
    procedure_kind                      VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    minimum_simulation_date             DATE NOT NULL,
    automatic_cash_protection_krw       BIGINT NOT NULL,
    standard_median_income_krw          BIGINT NOT NULL,
    living_expense_ratio_ppm            INT UNSIGNED NOT NULL,
    living_expense_months               TINYINT UNSIGNED NOT NULL,
    additional_protection_cap_krw       BIGINT NOT NULL,
    credit_restriction_game_days        SMALLINT UNSIGNED NOT NULL,
    maximum_claim_count                 TINYINT UNSIGNED NOT NULL,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (life_component_version_id),
    CONSTRAINT fk_insolvency_component_profile_version
        FOREIGN KEY (life_component_version_id) REFERENCES life_component_version (id),
    CONSTRAINT ck_insolvency_component_profile CHECK (
        procedure_kind = 'cashOnlyLiquidation'
        AND minimum_simulation_date = '2026-02-01'
        AND automatic_cash_protection_krw = 2500000
        AND standard_median_income_krw = 6494738
        AND living_expense_ratio_ppm = 400000
        AND living_expense_months = 6
        AND additional_protection_cap_krw = 15587371
        AND credit_restriction_game_days = 1825
        AND maximum_claim_count BETWEEN 1 AND 20
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_insolvency_component_profile_draft_insert
BEFORE INSERT ON insolvency_component_profile
FOR EACH ROW
SET NEW.life_component_version_id = IF(
    EXISTS (
        SELECT 1 FROM life_component_version AS component
        WHERE component.id = NEW.life_component_version_id
          AND component.component_kind = 'insolvency'
          AND component.sealed_at IS NULL
    ),
    NEW.life_component_version_id,
    NULL
);

CREATE TRIGGER tr_insolvency_component_profile_no_update
BEFORE UPDATE ON insolvency_component_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insolvency component profiles are immutable';

CREATE TRIGGER tr_insolvency_component_profile_no_delete
BEFORE DELETE ON insolvency_component_profile
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insolvency component profiles are immutable';

INSERT INTO insolvency_component_profile
    (
        life_component_version_id, procedure_kind, minimum_simulation_date,
        automatic_cash_protection_krw, standard_median_income_krw,
        living_expense_ratio_ppm, living_expense_months,
        additional_protection_cap_krw, credit_restriction_game_days,
        maximum_claim_count
    )
SELECT component.id, 'cashOnlyLiquidation', '2026-02-01',
       2500000, 6494738, 400000, 6, 15587371, 1825, 20
FROM life_component_version AS component
WHERE component.component_kind = 'insolvency'
  AND component.version_key = 'dev-unranked-m4-insolvency-2026-v1';

INSERT INTO life_component_canonical_manifest
    (life_component_version_id, canonical_json)
SELECT component.id,
       CAST(JSON_OBJECT(
           'availability', component.availability,
           'componentKind', component.component_kind,
           'profile', JSON_OBJECT(
               'additionalProtectionCapKrw', profile.additional_protection_cap_krw,
               'automaticCashProtectionKrw', profile.automatic_cash_protection_krw,
               'creditRestrictionGameDays', profile.credit_restriction_game_days,
               'livingExpenseMonths', profile.living_expense_months,
               'livingExpenseRatioPpm', profile.living_expense_ratio_ppm,
               'maximumClaimCount', profile.maximum_claim_count,
               'minimumSimulationDate', DATE_FORMAT(profile.minimum_simulation_date, '%Y-%m-%d'),
               'procedureKind', profile.procedure_kind,
               'standardMedianIncomeKrw', profile.standard_median_income_krw
           ),
           'rankedEligible', component.ranked_eligible,
           'schemaVersion', 1,
           'versionKey', component.version_key
       ) AS CHAR CHARACTER SET utf8mb4)
FROM life_component_version AS component
INNER JOIN insolvency_component_profile AS profile
    ON profile.life_component_version_id = component.id
WHERE component.component_kind = 'insolvency'
  AND component.version_key = 'dev-unranked-m4-insolvency-2026-v1';

UPDATE life_component_version AS component
INNER JOIN life_component_canonical_manifest AS manifest
    ON manifest.life_component_version_id = component.id
SET component.canonical_sha256 = manifest.canonical_sha256,
    component.sealed_at = CURRENT_TIMESTAMP(3)
WHERE component.component_kind = 'insolvency'
  AND component.version_key = 'dev-unranked-m4-insolvency-2026-v1'
  AND component.sealed_at IS NULL;

-- Clone the policy currently assigned to new runs, then append the E1 sourced rule. Existing
-- rules retain their exact parameters and source links through explicit clone provenance.
INSERT INTO policy_source_document
    (source_key, source_url, checked_on, original_sha256)
VALUES
    (
        'law-civil-execution-decree-cash-protection-2026-02-01',
        'https://www.law.go.kr/LSW/lsSideInfoP.do?docCls=jo&joBrNo=00&joNo=0002&lsiSeq=283025&urlMode=lsScJoRltInfoR',
        '2026-07-28',
        '04dcd6ded39931b4a15537ada379f3e506f5df66d8ed83530c4a4b574f04a624'
    ),
    (
        'law-debtor-rehabilitation-article-383-2026-07-28',
        'https://law.go.kr/LSW/lsLinkCommonInfo.do?chrClsCd=010202&lsJoLnkSeq=1024710677',
        '2026-07-28',
        '4653c20b407983befd3b3c60885fc752c9265b001ee6166a8d8d9ade1977941f'
    ),
    (
        'law-debtor-rehabilitation-decree-article-16-2026-07-28',
        'https://law.go.kr/LSW/lsInfoP.do?lsiSeq=263089&viewCls=lsRvsDocInfoR',
        '2026-07-28',
        'cc700c208c1c5bcbaa1064253b37f42e71c64f122ef3155bce5dc5d2e5fe1487'
    ),
    (
        'mohw-2026-standard-median-income-2026-07-28',
        'https://www.mohw.go.kr/board.es?act=view&bid=0026&list_no=1487112&mid=a10409020000',
        '2026-07-28',
        '5f6a7f04e28a6c232437743bdbced8fbc5b2d9d2da48e44af9cf62edfeec201d'
    );

INSERT INTO policy_set (policy_key, basis_date, ranked_eligible)
VALUES ('dev-unranked-kr-individual-insolvency-2026-v4', '2026-07-28', FALSE);

INSERT INTO policy_rule
    (policy_set_id, domain, rule_key, effective_from, effective_to, parameters)
SELECT target.id, source_rule.domain, source_rule.rule_key,
       source_rule.effective_from, source_rule.effective_to, source_rule.parameters
FROM policy_set AS target
INNER JOIN run_rule_bundle_assignment AS assignment
    ON assignment.assignment_key = 'newRun'
INNER JOIN policy_rule AS source_rule
    ON source_rule.policy_set_id = assignment.policy_set_id
WHERE target.policy_key = 'dev-unranked-kr-individual-insolvency-2026-v4';

DROP TRIGGER tr_policy_rule_clone_valid_insert;

CREATE TRIGGER tr_policy_rule_clone_valid_insert
BEFORE INSERT ON policy_rule_clone_provenance
FOR EACH ROW
SET NEW.target_policy_rule_id = IF(
    EXISTS (
        SELECT 1
        FROM policy_rule AS target_rule
        INNER JOIN policy_set AS target_set ON target_set.id = target_rule.policy_set_id
        INNER JOIN policy_rule AS source_rule ON source_rule.id = NEW.source_policy_rule_id
        INNER JOIN policy_set AS source_set ON source_set.id = source_rule.policy_set_id
        WHERE target_rule.id = NEW.target_policy_rule_id
          AND target_set.sealed_at IS NULL
          AND target_set.ranked_eligible = FALSE
          AND source_set.sealed_at IS NOT NULL
          AND BINARY target_rule.domain = BINARY source_rule.domain
          AND BINARY target_rule.rule_key = BINARY source_rule.rule_key
          AND target_rule.effective_from = source_rule.effective_from
          AND target_rule.effective_to <=> source_rule.effective_to
          AND target_rule.parameters = source_rule.parameters
          AND (
              EXISTS (
                  SELECT 1 FROM policy_rule_source AS source_link
                  WHERE source_link.policy_rule_id = source_rule.id
              )
              OR EXISTS (
                  SELECT 1 FROM policy_rule_legacy_provenance AS legacy
                  WHERE legacy.policy_rule_id = source_rule.id
              )
              OR EXISTS (
                  SELECT 1 FROM policy_rule_clone_provenance AS prior_clone
                  WHERE prior_clone.target_policy_rule_id = source_rule.id
              )
          )
    ),
    NEW.target_policy_rule_id,
    NULL
);

INSERT INTO policy_rule_clone_provenance
    (target_policy_rule_id, source_policy_rule_id, clone_kind)
SELECT target_rule.id,
       COALESCE(source_clone.source_policy_rule_id, source_rule.id),
       'sealedExactClone'
FROM policy_rule AS target_rule
INNER JOIN policy_set AS target_set
    ON target_set.id = target_rule.policy_set_id
INNER JOIN run_rule_bundle_assignment AS assignment
    ON assignment.assignment_key = 'newRun'
INNER JOIN policy_rule AS source_rule
    ON source_rule.policy_set_id = assignment.policy_set_id
   AND BINARY source_rule.domain = BINARY target_rule.domain
   AND BINARY source_rule.rule_key = BINARY target_rule.rule_key
   AND source_rule.effective_from = target_rule.effective_from
LEFT JOIN policy_rule_clone_provenance AS source_clone
    ON source_clone.target_policy_rule_id = source_rule.id
WHERE target_set.policy_key = 'dev-unranked-kr-individual-insolvency-2026-v4';

INSERT INTO policy_rule_source
    (policy_rule_id, policy_source_document_id, citation_order)
SELECT target_rule.id, source_link.policy_source_document_id, source_link.citation_order
FROM policy_rule AS target_rule
INNER JOIN policy_set AS target_set ON target_set.id = target_rule.policy_set_id
INNER JOIN run_rule_bundle_assignment AS assignment ON assignment.assignment_key = 'newRun'
INNER JOIN policy_rule AS source_rule
    ON source_rule.policy_set_id = assignment.policy_set_id
   AND BINARY source_rule.domain = BINARY target_rule.domain
   AND BINARY source_rule.rule_key = BINARY target_rule.rule_key
   AND source_rule.effective_from = target_rule.effective_from
INNER JOIN policy_rule_source AS source_link ON source_link.policy_rule_id = source_rule.id
WHERE target_set.policy_key = 'dev-unranked-kr-individual-insolvency-2026-v4';

INSERT INTO policy_rule
    (policy_set_id, domain, rule_key, effective_from, effective_to, parameters)
SELECT policy.id, 'insolvency', 'cashOnlyLiquidation', '2026-02-01', NULL,
       JSON_OBJECT(
           'additionalLivingExpenseExemption', JSON_OBJECT(
               'calculation', 'floorAtFinalProduct',
               'capKrw', 15587371,
               'householdRatioPpm', 400000,
               'months', 6,
               'provenance', 'LEGAL_STATUTE_WITH_GAME_BALANCE_ROUNDING',
               'standardMedianIncomeKrw', 6494738
           ),
           'automaticCashProtectionKrw', 2500000,
           'creditRestrictionGameDays', 1825,
           'creditRestrictionProvenance', 'GAME_BALANCE',
           'minimumSimulationDate', '2026-02-01',
           'procedureKind', 'cashOnlyLiquidation',
           'schemaVersion', 1,
           'supportedLoanKinds', JSON_ARRAY('studentLoan', 'unsecuredLoan')
       )
FROM policy_set AS policy
WHERE policy.policy_key = 'dev-unranked-kr-individual-insolvency-2026-v4';

INSERT INTO policy_rule_source
    (policy_rule_id, policy_source_document_id, citation_order)
SELECT rule.id, source.id, source_order.citation_order
FROM policy_rule AS rule
INNER JOIN policy_set AS policy ON policy.id = rule.policy_set_id
INNER JOIN (
    SELECT 'law-civil-execution-decree-cash-protection-2026-02-01' AS source_key,
           1 AS citation_order
    UNION ALL
    SELECT 'law-debtor-rehabilitation-article-383-2026-07-28', 2
    UNION ALL
    SELECT 'law-debtor-rehabilitation-decree-article-16-2026-07-28', 3
    UNION ALL
    SELECT 'mohw-2026-standard-median-income-2026-07-28', 4
) AS source_order
INNER JOIN policy_source_document AS source ON source.source_key = source_order.source_key
WHERE policy.policy_key = 'dev-unranked-kr-individual-insolvency-2026-v4'
  AND rule.domain = 'insolvency'
  AND rule.rule_key = 'cashOnlyLiquidation';

INSERT INTO policy_set_canonical_manifest (policy_set_id, canonical_json)
SELECT policy.id,
       CONCAT(
           '{"basisDate":', JSON_QUOTE(DATE_FORMAT(policy.basis_date, '%Y-%m-%d')),
           ',"policyKey":', JSON_QUOTE(policy.policy_key),
           ',"rankedEligible":', IF(policy.ranked_eligible, 'true', 'false'),
           ',"rules":[',
           COALESCE((
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
           ), ''),
           '],"schemaVersion":1}'
       )
FROM policy_set AS policy
WHERE policy.policy_key = 'dev-unranked-kr-individual-insolvency-2026-v4';

UPDATE policy_set AS policy
INNER JOIN policy_set_canonical_manifest AS manifest ON manifest.policy_set_id = policy.id
SET policy.canonical_sha256 = manifest.canonical_sha256,
    policy.sealed_at = CURRENT_TIMESTAMP(3)
WHERE policy.policy_key = 'dev-unranked-kr-individual-insolvency-2026-v4'
  AND policy.sealed_at IS NULL;

-- Clone the current sealed life graph and add only the insolvency component pointer.
INSERT INTO life_catalog_set
    (
        catalog_key, ranked_eligible, legacy_dependent_age_years,
        living_cost_component_version_id, welfare_component_version_id,
        life_event_component_version_id, insurance_component_version_id,
        insolvency_component_version_id, corporation_component_version_id
    )
SELECT 'dev-unranked-m4-life-insolvency-2026-v5', FALSE,
       current_catalog.legacy_dependent_age_years,
       current_catalog.living_cost_component_version_id,
       current_catalog.welfare_component_version_id,
       current_catalog.life_event_component_version_id,
       current_catalog.insurance_component_version_id,
       insolvency.id,
       current_catalog.corporation_component_version_id
FROM run_rule_bundle_assignment AS assignment
INNER JOIN life_catalog_set AS current_catalog ON current_catalog.id = assignment.life_catalog_set_id
INNER JOIN life_component_version AS insolvency
    ON insolvency.component_kind = 'insolvency'
   AND insolvency.version_key = 'dev-unranked-m4-insolvency-2026-v1'
   AND insolvency.sealed_at IS NOT NULL
WHERE assignment.assignment_key = 'newRun';

UPDATE life_catalog_set
SET canonical_sha256 = SHA2(
        CAST(JSON_OBJECT(
            'catalogKey', catalog_key,
            'corporationComponentVersionId', CAST(corporation_component_version_id AS CHAR),
            'insolvencyComponentVersionId', CAST(insolvency_component_version_id AS CHAR),
            'insuranceComponentVersionId', CAST(insurance_component_version_id AS CHAR),
            'lifeEventComponentVersionId', CAST(life_event_component_version_id AS CHAR),
            'legacyDependentAgeYears', legacy_dependent_age_years,
            'livingCostComponentVersionId', CAST(living_cost_component_version_id AS CHAR),
            'schemaVersion', 2,
            'welfareComponentVersionId', CAST(welfare_component_version_id AS CHAR)
        ) AS CHAR CHARACTER SET utf8mb4),
        256
    ),
    sealed_at = CURRENT_TIMESTAMP(3)
WHERE catalog_key = 'dev-unranked-m4-life-insolvency-2026-v5'
  AND sealed_at IS NULL;

CREATE TABLE insolvency_case (
    id                                      BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                                 BIGINT UNSIGNED NOT NULL,
    run_revision                            INT UNSIGNED NOT NULL,
    life_catalog_set_id                     BIGINT UNSIGNED NOT NULL,
    policy_set_id                           BIGINT UNSIGNED NOT NULL,
    insolvency_component_version_id         BIGINT UNSIGNED NOT NULL,
    procedure_kind                          VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    status                                  VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    prepared_command_id                     CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    composition_sha256                      CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    automatic_cash_protection_rule_id       BIGINT UNSIGNED NOT NULL,
    additional_exemption_rule_id            BIGINT UNSIGNED NOT NULL,
    prepared_game_day                       INT UNSIGNED NOT NULL,
    submitted_game_day                      INT UNSIGNED NULL,
    terminal_game_day                       INT UNSIGNED NULL,
    credit_restriction_end_exclusive        INT UNSIGNED NULL,
    wallet_cash_krw                         BIGINT NOT NULL,
    automatic_protected_krw                 BIGINT NOT NULL,
    additional_protected_krw                BIGINT NOT NULL,
    liquidatable_krw                        BIGINT NOT NULL,
    total_claim_krw                         BIGINT NOT NULL,
    claim_count                             TINYINT UNSIGNED NOT NULL,
    distributed_krw                         BIGINT NOT NULL DEFAULT 0,
    discharged_krw                          BIGINT NOT NULL DEFAULT 0,
    current_case_guard                      TINYINT
        GENERATED ALWAYS AS (
            IF(status IN ('prepared', 'filed', 'liquidation', 'discharged', 'rebuilding'), 1, NULL)
        ) STORED,
    created_at                              DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                              DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_insolvency_case_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_insolvency_case_current (save_id, run_revision, current_case_guard),
    UNIQUE KEY uk_insolvency_case_prepare_command (save_id, prepared_command_id),
    KEY ix_insolvency_case_history (save_id, run_revision, id),
    CONSTRAINT fk_insolvency_case_bundle
        FOREIGN KEY (save_id, run_revision, life_catalog_set_id)
        REFERENCES run_rule_bundle (save_id, run_revision, life_catalog_set_id),
    CONSTRAINT fk_insolvency_case_catalog_component
        FOREIGN KEY (life_catalog_set_id, insolvency_component_version_id)
        REFERENCES life_catalog_set (id, insolvency_component_version_id),
    CONSTRAINT fk_insolvency_case_policy
        FOREIGN KEY (policy_set_id) REFERENCES policy_set (id),
    CONSTRAINT fk_insolvency_case_prepare_command
        FOREIGN KEY (save_id, prepared_command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT fk_insolvency_case_automatic_rule
        FOREIGN KEY (automatic_cash_protection_rule_id) REFERENCES policy_rule (id),
    CONSTRAINT fk_insolvency_case_additional_rule
        FOREIGN KEY (additional_exemption_rule_id) REFERENCES policy_rule (id),
    CONSTRAINT ck_insolvency_case_identity CHECK (
        procedure_kind = 'cashOnlyLiquidation'
        AND composition_sha256 REGEXP '^[0-9a-f]{64}$'
        AND claim_count BETWEEN 1 AND 20
    ),
    CONSTRAINT ck_insolvency_case_money CHECK (
        wallet_cash_krw BETWEEN 0 AND 9007199254740991
        AND automatic_protected_krw BETWEEN 0 AND wallet_cash_krw
        AND additional_protected_krw BETWEEN 0 AND wallet_cash_krw - automatic_protected_krw
        AND liquidatable_krw
            = wallet_cash_krw - automatic_protected_krw - additional_protected_krw
        AND total_claim_krw BETWEEN 1 AND 9007199254740991
        AND total_claim_krw > wallet_cash_krw
        AND distributed_krw BETWEEN 0 AND liquidatable_krw
        AND discharged_krw BETWEEN 0 AND total_claim_krw
    ),
    CONSTRAINT ck_insolvency_case_state CHECK (
        (status = 'prepared' AND submitted_game_day IS NULL AND terminal_game_day IS NULL
         AND credit_restriction_end_exclusive IS NULL
         AND distributed_krw = 0 AND discharged_krw = 0)
        OR
        (status IN ('filed', 'liquidation', 'discharged')
         AND submitted_game_day IS NOT NULL AND terminal_game_day IS NULL
         AND credit_restriction_end_exclusive IS NOT NULL)
        OR
        (status = 'rebuilding' AND submitted_game_day IS NOT NULL AND terminal_game_day IS NULL
         AND credit_restriction_end_exclusive = submitted_game_day + 1825
         AND distributed_krw = liquidatable_krw
         AND total_claim_krw = distributed_krw + discharged_krw)
        OR
        (status = 'withdrawn' AND submitted_game_day IS NULL AND terminal_game_day IS NOT NULL
         AND credit_restriction_end_exclusive IS NULL
         AND distributed_krw = 0 AND discharged_krw = 0)
        OR
        (status = 'recovered' AND submitted_game_day IS NOT NULL
         AND terminal_game_day = credit_restriction_end_exclusive
         AND distributed_krw = liquidatable_krw
         AND total_claim_krw = distributed_krw + discharged_krw)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE insolvency_case_transition (
    id                      BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                 BIGINT UNSIGNED NOT NULL,
    run_revision            INT UNSIGNED NOT NULL,
    case_id                 BIGINT UNSIGNED NOT NULL,
    transition_no           TINYINT UNSIGNED NOT NULL,
    from_status             VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL,
    to_status               VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    command_id              CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    transition_game_day     INT UNSIGNED NOT NULL,
    transition_reason       VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at              DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_insolvency_transition_no (save_id, run_revision, case_id, transition_no),
    UNIQUE KEY uk_insolvency_transition_status (save_id, run_revision, case_id, to_status),
    CONSTRAINT fk_insolvency_transition_case
        FOREIGN KEY (save_id, run_revision, case_id)
        REFERENCES insolvency_case (save_id, run_revision, id),
    CONSTRAINT fk_insolvency_transition_command
        FOREIGN KEY (save_id, command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT ck_insolvency_transition_no CHECK (transition_no BETWEEN 1 AND 16),
    CONSTRAINT ck_insolvency_transition_status CHECK (
        to_status IN ('prepared', 'filed', 'liquidation', 'discharged', 'rebuilding', 'withdrawn', 'recovered')
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE insolvency_asset (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED NOT NULL,
    case_id                             BIGINT UNSIGNED NOT NULL,
    life_catalog_set_id                 BIGINT UNSIGNED NOT NULL,
    policy_set_id                       BIGINT UNSIGNED NOT NULL,
    insolvency_component_version_id     BIGINT UNSIGNED NOT NULL,
    asset_kind                          VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    authority_key                       VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    original_amount_krw                 BIGINT NOT NULL,
    automatic_protected_krw             BIGINT NOT NULL,
    additional_protected_krw            BIGINT NOT NULL,
    liquidatable_krw                    BIGINT NOT NULL,
    distributed_krw                     BIGINT NOT NULL DEFAULT 0,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_insolvency_asset_case_kind (save_id, run_revision, case_id, asset_kind),
    UNIQUE KEY uk_insolvency_asset_case_id (save_id, run_revision, case_id, id),
    CONSTRAINT fk_insolvency_asset_case
        FOREIGN KEY (save_id, run_revision, case_id)
        REFERENCES insolvency_case (save_id, run_revision, id),
    CONSTRAINT fk_insolvency_asset_catalog
        FOREIGN KEY (life_catalog_set_id, insolvency_component_version_id)
        REFERENCES life_catalog_set (id, insolvency_component_version_id),
    CONSTRAINT fk_insolvency_asset_policy
        FOREIGN KEY (policy_set_id) REFERENCES policy_set (id),
    CONSTRAINT ck_insolvency_asset CHECK (
        asset_kind = 'wallet'
        AND authority_key = 'save.walletCashKrw'
        AND original_amount_krw BETWEEN 0 AND 9007199254740991
        AND automatic_protected_krw BETWEEN 0 AND original_amount_krw
        AND additional_protected_krw
            BETWEEN 0 AND original_amount_krw - automatic_protected_krw
        AND liquidatable_krw
            = original_amount_krw - automatic_protected_krw - additional_protected_krw
        AND distributed_krw BETWEEN 0 AND liquidatable_krw
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE insolvency_claim (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED NOT NULL,
    case_id                             BIGINT UNSIGNED NOT NULL,
    life_catalog_set_id                 BIGINT UNSIGNED NOT NULL,
    policy_set_id                       BIGINT UNSIGNED NOT NULL,
    insolvency_component_version_id     BIGINT UNSIGNED NOT NULL,
    loan_contract_id                    BIGINT UNSIGNED NOT NULL,
    claim_class                         VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    principal_krw                       BIGINT NOT NULL,
    interest_krw                        BIGINT NOT NULL,
    fee_krw                             BIGINT NOT NULL,
    allowed_krw                         BIGINT NOT NULL,
    distributed_krw                     BIGINT NOT NULL DEFAULT 0,
    discharged_krw                      BIGINT NOT NULL DEFAULT 0,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_insolvency_claim_loan (save_id, run_revision, case_id, loan_contract_id),
    UNIQUE KEY uk_insolvency_claim_case_id (save_id, run_revision, case_id, id),
    KEY ix_insolvency_claim_page (save_id, run_revision, case_id, id),
    CONSTRAINT fk_insolvency_claim_case
        FOREIGN KEY (save_id, run_revision, case_id)
        REFERENCES insolvency_case (save_id, run_revision, id),
    CONSTRAINT fk_insolvency_claim_loan
        FOREIGN KEY (save_id, run_revision, loan_contract_id)
        REFERENCES loan_contract (save_id, run_revision, id),
    CONSTRAINT fk_insolvency_claim_catalog
        FOREIGN KEY (life_catalog_set_id, insolvency_component_version_id)
        REFERENCES life_catalog_set (id, insolvency_component_version_id),
    CONSTRAINT fk_insolvency_claim_policy
        FOREIGN KEY (policy_set_id) REFERENCES policy_set (id),
    CONSTRAINT ck_insolvency_claim CHECK (
        claim_class = 'generalUnsecured'
        AND principal_krw BETWEEN 0 AND 9007199254740991
        AND interest_krw BETWEEN 0 AND 9007199254740991
        AND fee_krw BETWEEN 0 AND 9007199254740991
        AND allowed_krw = principal_krw + interest_krw + fee_krw
        AND allowed_krw > 0
        AND distributed_krw BETWEEN 0 AND allowed_krw
        AND discharged_krw BETWEEN 0 AND allowed_krw
        AND (distributed_krw = 0 AND discharged_krw = 0
             OR allowed_krw = distributed_krw + discharged_krw)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE insolvency_distribution (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    case_id                         BIGINT UNSIGNED NOT NULL,
    claim_id                        BIGINT UNSIGNED NOT NULL,
    distribution_order              TINYINT UNSIGNED NOT NULL,
    amount_krw                      BIGINT NOT NULL,
    loan_payment_id                 BIGINT UNSIGNED NOT NULL,
    ledger_transaction_id           BIGINT UNSIGNED NOT NULL,
    applied_game_day                INT UNSIGNED NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_insolvency_distribution_claim (save_id, run_revision, case_id, claim_id),
    UNIQUE KEY uk_insolvency_distribution_order (save_id, run_revision, case_id, distribution_order),
    UNIQUE KEY uk_insolvency_distribution_payment (save_id, run_revision, loan_payment_id),
    UNIQUE KEY uk_insolvency_distribution_ledger (save_id, run_revision, ledger_transaction_id),
    KEY ix_insolvency_distribution_page (save_id, run_revision, case_id, id),
    CONSTRAINT fk_insolvency_distribution_claim
        FOREIGN KEY (save_id, run_revision, case_id, claim_id)
        REFERENCES insolvency_claim (save_id, run_revision, case_id, id),
    CONSTRAINT fk_insolvency_distribution_payment
        FOREIGN KEY (save_id, run_revision, loan_payment_id)
        REFERENCES loan_payment (save_id, run_revision, id),
    CONSTRAINT fk_insolvency_distribution_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT ck_insolvency_distribution CHECK (
        distribution_order BETWEEN 1 AND 20
        AND amount_krw BETWEEN 1 AND 9007199254740991
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE insolvency_command_receipt (
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED NOT NULL,
    insolvency_component_version_id     BIGINT UNSIGNED NOT NULL,
    command_id                          CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    command_kind                        VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    payload_sha256                      CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    case_id                             BIGINT UNSIGNED NOT NULL,
    result_json                         LONGTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    committed_state_revision            BIGINT UNSIGNED NOT NULL,
    committed_game_day                  INT UNSIGNED NOT NULL,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, command_id),
    CONSTRAINT fk_insolvency_receipt_command
        FOREIGN KEY (save_id, command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT fk_insolvency_receipt_case
        FOREIGN KEY (save_id, run_revision, case_id)
        REFERENCES insolvency_case (save_id, run_revision, id),
    CONSTRAINT ck_insolvency_receipt CHECK (
        command_kind IN ('prepareCase', 'submitCase', 'withdrawCase', 'recoverCase')
        AND payload_sha256 REGEXP '^[0-9a-f]{64}$'
        AND JSON_VALID(result_json)
        AND committed_state_revision > 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

-- Existing servicing identities remain immutable; E1 adds only terminal discharge and a payment
-- kind tied to an insolvency case. Zero distributions deliberately create no payment row.
ALTER TABLE loan_installment
    DROP CHECK ck_loan_installment_status,
    ADD CONSTRAINT ck_loan_installment_status CHECK (
        status IN ('pending', 'due', 'partiallyPaid', 'paid', 'cancelled', 'discharged')
        AND (
            status NOT IN ('paid', 'cancelled')
            OR (
                status = 'paid'
                AND paid_fee_krw = scheduled_fee_krw
                AND paid_interest_krw = scheduled_interest_krw
                AND paid_principal_krw = scheduled_principal_krw
            )
            OR (
                status = 'cancelled'
                AND paid_fee_krw = 0
                AND paid_interest_krw = 0
                AND paid_principal_krw = 0
            )
        )
    );

DROP TRIGGER tr_loan_installment_transition_only;

CREATE TRIGGER tr_loan_installment_transition_only
BEFORE UPDATE ON loan_installment
FOR EACH ROW
SET NEW.id = IF(
    NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.loan_contract_id = OLD.loan_contract_id
        AND NEW.installment_no = OLD.installment_no
        AND NEW.created_at = OLD.created_at
        AND (
            (
                OLD.status = 'pending'
                AND NEW.status = 'pending'
                AND NEW.schedule_revision = OLD.schedule_revision + 1
                AND NEW.paid_fee_krw = 0
                AND NEW.paid_interest_krw = 0
                AND NEW.paid_principal_krw = 0
            )
            OR (
                OLD.status IN ('pending', 'due', 'partiallyPaid')
                AND NEW.status IN ('due', 'partiallyPaid', 'paid')
                AND NEW.due_game_day = OLD.due_game_day
                AND NEW.interest_period_start_game_day
                    = OLD.interest_period_start_game_day
                AND NEW.interest_period_end_game_day = OLD.interest_period_end_game_day
                AND NEW.elapsed_days = OLD.elapsed_days
                AND NEW.annual_rate_bp = OLD.annual_rate_bp
                AND NEW.opening_principal_krw = OLD.opening_principal_krw
                AND NEW.scheduled_fee_krw = OLD.scheduled_fee_krw
                AND NEW.scheduled_interest_krw = OLD.scheduled_interest_krw
                AND NEW.scheduled_principal_krw = OLD.scheduled_principal_krw
                AND NEW.interest_remainder_before = OLD.interest_remainder_before
                AND NEW.interest_remainder_after = OLD.interest_remainder_after
                AND NEW.paid_fee_krw >= OLD.paid_fee_krw
                AND NEW.paid_interest_krw >= OLD.paid_interest_krw
                AND NEW.paid_principal_krw >= OLD.paid_principal_krw
                AND NEW.paid_fee_krw <= NEW.scheduled_fee_krw
                AND NEW.paid_interest_krw <= NEW.scheduled_interest_krw
                AND NEW.paid_principal_krw <= NEW.scheduled_principal_krw
                AND NEW.schedule_revision = OLD.schedule_revision
            )
            OR (
                OLD.status = 'pending'
                AND NEW.status = 'cancelled'
                AND NEW.due_game_day = OLD.due_game_day
                AND NEW.interest_period_start_game_day
                    = OLD.interest_period_start_game_day
                AND NEW.interest_period_end_game_day = OLD.interest_period_end_game_day
                AND NEW.elapsed_days = OLD.elapsed_days
                AND NEW.annual_rate_bp = OLD.annual_rate_bp
                AND NEW.opening_principal_krw = OLD.opening_principal_krw
                AND NEW.scheduled_fee_krw = OLD.scheduled_fee_krw
                AND NEW.scheduled_interest_krw = OLD.scheduled_interest_krw
                AND NEW.scheduled_principal_krw = OLD.scheduled_principal_krw
                AND NEW.interest_remainder_before = OLD.interest_remainder_before
                AND NEW.interest_remainder_after = OLD.interest_remainder_after
                AND NEW.schedule_revision = OLD.schedule_revision
            )
            OR (
                OLD.status IN ('pending', 'due', 'partiallyPaid')
                AND NEW.status = 'discharged'
                AND NEW.due_game_day = OLD.due_game_day
                AND NEW.interest_period_start_game_day
                    = OLD.interest_period_start_game_day
                AND NEW.interest_period_end_game_day = OLD.interest_period_end_game_day
                AND NEW.elapsed_days = OLD.elapsed_days
                AND NEW.annual_rate_bp = OLD.annual_rate_bp
                AND NEW.opening_principal_krw = OLD.opening_principal_krw
                AND NEW.scheduled_fee_krw = OLD.scheduled_fee_krw
                AND NEW.scheduled_interest_krw = OLD.scheduled_interest_krw
                AND NEW.scheduled_principal_krw = OLD.scheduled_principal_krw
                AND NEW.interest_remainder_before = OLD.interest_remainder_before
                AND NEW.interest_remainder_after = OLD.interest_remainder_after
                AND NEW.paid_fee_krw = OLD.paid_fee_krw
                AND NEW.paid_interest_krw = OLD.paid_interest_krw
                AND NEW.paid_principal_krw = OLD.paid_principal_krw
                AND NEW.schedule_revision = OLD.schedule_revision
            )
        ),
    OLD.id,
    NULL
);

ALTER TABLE loan_payment
    DROP CHECK ck_loan_payment_kind,
    ADD COLUMN insolvency_case_id BIGINT UNSIGNED NULL AFTER property_sale_execution_id,
    ADD KEY ix_loan_payment_insolvency_case (save_id, run_revision, insolvency_case_id),
    ADD CONSTRAINT fk_loan_payment_insolvency_case
        FOREIGN KEY (save_id, run_revision, insolvency_case_id)
        REFERENCES insolvency_case (save_id, run_revision, id),
    ADD CONSTRAINT ck_loan_payment_kind CHECK (
        payment_kind IN (
            'scheduledInstallment', 'manualPrepayment',
            'leaseMovePayoff', 'propertySalePayoff', 'insolvencyDistribution'
        )
        AND (
            (payment_kind = 'scheduledInstallment' AND command_id IS NULL
             AND property_sale_execution_id IS NULL AND insolvency_case_id IS NULL)
            OR
            (payment_kind IN ('manualPrepayment', 'leaseMovePayoff')
             AND command_id REGEXP
                 '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
             AND property_sale_execution_id IS NULL AND insolvency_case_id IS NULL)
            OR
            (payment_kind = 'propertySalePayoff' AND command_id IS NULL
             AND property_sale_execution_id IS NOT NULL AND insolvency_case_id IS NULL)
            OR
            (payment_kind = 'insolvencyDistribution' AND command_id IS NULL
             AND property_sale_execution_id IS NULL AND insolvency_case_id IS NOT NULL)
        )
    );

DROP TRIGGER tr_loan_payment_valid_insert;

CREATE TRIGGER tr_loan_payment_valid_insert
BEFORE INSERT ON loan_payment
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'prepared'
        AND NEW.ledger_transaction_id IS NULL
        AND EXISTS (
            SELECT 1 FROM loan_contract AS contract
            WHERE contract.id = NEW.loan_contract_id
              AND contract.save_id = NEW.save_id
              AND contract.run_revision = NEW.run_revision
              AND contract.read_only = FALSE
              AND (
                  (
                      contract.status IN ('active', 'delinquent')
                      AND NEW.insolvency_case_id IS NULL
                      AND (
                          NEW.payment_kind = 'scheduledInstallment'
                          OR (
                              NEW.payment_kind = 'manualPrepayment'
                              AND contract.status = 'active'
                              AND NOT EXISTS (
                                  SELECT 1 FROM loan_obligation_bucket AS bucket
                                  WHERE bucket.loan_contract_id = contract.id
                                    AND bucket.status = 'delinquent'
                              )
                          )
                      )
                  )
                  OR (
                      contract.status = 'defaulted'
                      AND NEW.payment_kind = 'insolvencyDistribution'
                      AND NEW.insolvency_case_id IS NOT NULL
                      AND EXISTS (
                          SELECT 1
                          FROM insolvency_case AS case_row
                          INNER JOIN insolvency_claim AS claim
                              ON claim.save_id = case_row.save_id
                             AND claim.run_revision = case_row.run_revision
                             AND claim.case_id = case_row.id
                             AND claim.loan_contract_id = contract.id
                          WHERE case_row.id = NEW.insolvency_case_id
                            AND case_row.save_id = NEW.save_id
                            AND case_row.run_revision = NEW.run_revision
                            AND case_row.status = 'prepared'
                            AND claim.distributed_krw = 0
                            AND claim.discharged_krw = 0
                            AND NEW.amount_krw BETWEEN 1 AND claim.allowed_krw
                      )
                  )
              )
        ),
    NEW.save_id,
    NULL
);

DROP TRIGGER tr_loan_payment_transition_only;

CREATE TRIGGER tr_loan_payment_transition_only
BEFORE UPDATE ON loan_payment
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'prepared'
        AND NEW.status = 'applied'
        AND NEW.ledger_transaction_id IS NOT NULL
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.loan_contract_id = OLD.loan_contract_id
        AND NEW.payment_no = OLD.payment_no
        AND BINARY NEW.payment_kind = BINARY OLD.payment_kind
        AND NEW.amount_krw = OLD.amount_krw
        AND NEW.game_day = OLD.game_day
        AND BINARY NEW.command_id <=> BINARY OLD.command_id
        AND NEW.insolvency_case_id <=> OLD.insolvency_case_id
        AND NEW.created_at = OLD.created_at
        AND NEW.amount_krw = (
            SELECT COALESCE(SUM(allocation.amount_krw), 0)
            FROM loan_payment_allocation AS allocation
            WHERE allocation.loan_payment_id = OLD.id
        ),
    OLD.id,
    NULL
);

ALTER TABLE ledger_transaction
    ADD CONSTRAINT ck_ledger_transaction_insolvency_source CHECK (
        source_kind NOT LIKE 'insolvency%'
        OR source_kind IN ('insolvencyDistribution', 'insolvencyDischarge')
    );

CREATE TRIGGER tr_ledger_transaction_insolvency_source_insert
BEFORE INSERT ON ledger_transaction
FOR EACH ROW
FOLLOWS tr_ledger_transaction_insurance_source_insert
SET NEW.source_kind = IF(
    NEW.source_kind NOT IN ('insolvencyDistribution', 'insolvencyDischarge')
        OR (
            NEW.source_id REGEXP '^[1-9][0-9]{0,19}$'
            AND (
                (
                    NEW.source_kind = 'insolvencyDistribution'
                    AND EXISTS (
                        SELECT 1
                        FROM loan_payment AS payment
                        INNER JOIN insolvency_case AS case_row
                            ON case_row.id = payment.insolvency_case_id
                           AND case_row.save_id = payment.save_id
                           AND case_row.run_revision = payment.run_revision
                        INNER JOIN run_rule_bundle AS bundle
                            ON bundle.save_id = case_row.save_id
                           AND bundle.run_revision = case_row.run_revision
                        WHERE BINARY CAST(payment.id AS CHAR) = BINARY NEW.source_id
                          AND payment.save_id = NEW.save_id
                          AND payment.run_revision = NEW.run_revision
                          AND payment.payment_kind = 'insolvencyDistribution'
                          AND payment.status = 'prepared'
                          AND case_row.status = 'prepared'
                          AND bundle.policy_set_id = NEW.policy_set_id
                    )
                )
                OR (
                    NEW.source_kind = 'insolvencyDischarge'
                    AND EXISTS (
                        SELECT 1
                        FROM insolvency_case AS case_row
                        INNER JOIN run_rule_bundle AS bundle
                            ON bundle.save_id = case_row.save_id
                           AND bundle.run_revision = case_row.run_revision
                        WHERE BINARY CAST(case_row.id AS CHAR) = BINARY NEW.source_id
                          AND case_row.save_id = NEW.save_id
                          AND case_row.run_revision = NEW.run_revision
                          AND case_row.status = 'prepared'
                          AND bundle.policy_set_id = NEW.policy_set_id
                    )
                )
            )
        ),
    NEW.source_kind,
    NULL
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
            'insurancePremiumExpense', 'insuranceClaimRecovery',
            'insolvencyDischargedDebt', 'insolvencyDischargeGain'
        )
    );

CREATE TRIGGER tr_ledger_posting_insolvency_reference_insert
BEFORE INSERT ON ledger_posting
FOR EACH ROW
FOLLOWS tr_ledger_posting_insurance_reference_insert
SET NEW.account_code = IF(
    NOT EXISTS (
        SELECT 1 FROM ledger_transaction AS ledger
        WHERE ledger.id = NEW.ledger_transaction_id
          AND ledger.save_id = NEW.save_id
          AND ledger.run_revision = NEW.run_revision
          AND ledger.source_kind IN ('insolvencyDistribution', 'insolvencyDischarge')
    )
        OR EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND (
                  (
                      ledger.source_kind = 'insolvencyDistribution'
                      AND (
                          (NEW.account_code = 'wallet' AND NEW.loan_contract_id IS NULL)
                          OR (
                              NEW.account_code IN (
                                  'loanPrincipalLiability', 'loanInterestExpense', 'loanFeeExpense'
                              )
                              AND EXISTS (
                                  SELECT 1 FROM loan_payment AS payment
                                  WHERE BINARY CAST(payment.id AS CHAR) = BINARY ledger.source_id
                                    AND payment.save_id = ledger.save_id
                                    AND payment.run_revision = ledger.run_revision
                                    AND payment.loan_contract_id = NEW.loan_contract_id
                                    AND payment.payment_kind = 'insolvencyDistribution'
                              )
                          )
                      )
                  )
                  OR (
                      ledger.source_kind = 'insolvencyDischarge'
                      AND NEW.account_code IN (
                          'insolvencyDischargedDebt', 'insolvencyDischargeGain'
                      )
                      AND NEW.loan_contract_id IS NULL
                  )
              )
        ),
    NEW.account_code,
    NULL
);

CREATE TRIGGER tr_insolvency_case_transition_no_update
BEFORE UPDATE ON insolvency_case_transition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insolvency case transitions are immutable';

CREATE TRIGGER tr_insolvency_case_transition_no_delete
BEFORE DELETE ON insolvency_case_transition
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insolvency case transitions are immutable';

CREATE TRIGGER tr_insolvency_asset_no_delete
BEFORE DELETE ON insolvency_asset
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insolvency assets are immutable identities';

CREATE TRIGGER tr_insolvency_claim_no_delete
BEFORE DELETE ON insolvency_claim
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insolvency claims are immutable identities';

CREATE TRIGGER tr_insolvency_distribution_no_update
BEFORE UPDATE ON insolvency_distribution
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insolvency distributions are immutable';

CREATE TRIGGER tr_insolvency_distribution_no_delete
BEFORE DELETE ON insolvency_distribution
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insolvency distributions are immutable';

CREATE TRIGGER tr_insolvency_receipt_no_update
BEFORE UPDATE ON insolvency_command_receipt
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insolvency command receipts are immutable';

CREATE TRIGGER tr_insolvency_receipt_no_delete
BEFORE DELETE ON insolvency_command_receipt
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'insolvency command receipts are immutable';

-- The only external publication mutation: future runs receive the new policy and life graph.
UPDATE run_rule_bundle_assignment AS assignment
INNER JOIN policy_set AS policy
    ON policy.policy_key = 'dev-unranked-kr-individual-insolvency-2026-v4'
   AND policy.sealed_at IS NOT NULL
INNER JOIN life_catalog_set AS catalog
    ON catalog.catalog_key = 'dev-unranked-m4-life-insolvency-2026-v5'
   AND catalog.sealed_at IS NOT NULL
SET assignment.policy_set_id = policy.id,
    assignment.life_catalog_set_id = catalog.id,
    assignment.assignment_revision = assignment.assignment_revision + 1
WHERE assignment.assignment_key = 'newRun';
