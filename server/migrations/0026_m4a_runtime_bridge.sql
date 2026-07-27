-- M4-A runtime enums, strict posting ownership, and one-time pre-M4 run bridge (§3.3).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;

-- Refuse to guess provenance for an unknown pre-M4 graph. Revision zero explicitly means the
-- v1 market world predates the assignment table; every later mapping is the historical pointer
-- revision that first selected that immutable version.
CREATE TEMPORARY TABLE m4a_bridge_guard (
    guard_key      VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    accepted       TINYINT UNSIGNED NOT NULL,
    PRIMARY KEY (guard_key),
    CONSTRAINT ck_m4a_bridge_guard CHECK (accepted = 1)
) ENGINE = InnoDB;

INSERT INTO m4a_bridge_guard (guard_key, accepted)
SELECT 'knownRunGraphs',
       IF(
           NOT EXISTS (
               SELECT 1
               FROM save
               INNER JOIN market_world ON market_world.id = save.market_world_id
               INNER JOIN policy_set ON policy_set.id = save.policy_set_id
               LEFT JOIN career_run
                   ON career_run.save_id = save.id
                  AND career_run.run_revision = save.run_revision
               WHERE market_world.world_key NOT IN (
                         'm1-2026-v1', 'm1-2026-v2', 'm1-2026-v3', 'm2-2026-v4'
                     )
                  OR policy_set.policy_key NOT IN (
                         'kr-individual-2026-v1', 'kr-individual-2026-v2'
                     )
                  OR career_run.save_id IS NULL
                  OR save.debt_krw < 0
           )
           AND NOT EXISTS (
               SELECT 1 FROM `character` WHERE dependents > 100
           ),
           1,
           NULL
       );

-- Settlement protocol names are closed and their version-1 payload has one exact shape.
ALTER TABLE scheduled_settlement
    DROP CHECK ck_scheduled_settlement_kind,
    DROP CHECK ck_scheduled_settlement_source_kind,
    ADD CONSTRAINT ck_scheduled_settlement_kind CHECK (
        kind IN (
            'cmaInterest', 'depositMaturity', 'savingsInstallment', 'savingsMaturity',
            'bondCoupon', 'bondMaturity', 'llxDistribution', 'financialIncomeFiling',
            'employmentPayroll', 'employmentReconciliation', 'militaryPay',
            'militarySavingsInstallment', 'militarySavingsMaturity',
            'militarySavingsGovernmentMatch', 'livingCostMonth'
        )
    ),
    ADD CONSTRAINT ck_scheduled_settlement_source_kind CHECK (
        source_kind IN (
            'cmaAccount', 'depositContract', 'savingsContract', 'bondPosition',
            'indexPosition', 'taxYear', 'employmentContract', 'yearEndTaxAssessment',
            'militaryService', 'militarySavingsContract', 'militarySavingsInstallment',
            'livingCostMonth'
        )
    ),
    ADD CONSTRAINT ck_scheduled_settlement_living_cost_payload CHECK (
        kind <> 'livingCostMonth'
        OR (
            JSON_TYPE(payload) = 'OBJECT'
            AND JSON_LENGTH(payload) = 2
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.version')) = 'INTEGER'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.version')) = '1'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.livingCostMonthId')) = 'STRING'
            AND REGEXP_LIKE(
                JSON_UNQUOTE(JSON_EXTRACT(payload, '$.livingCostMonthId')),
                '^[1-9][0-9]{0,19}$'
            )
            AND source_kind = 'livingCostMonth'
            AND BINARY source_id = BINARY JSON_UNQUOTE(
                JSON_EXTRACT(payload, '$.livingCostMonthId')
            )
            AND occurrence = 1
        )
    );

CREATE TRIGGER tr_scheduled_settlement_living_cost_insert
BEFORE INSERT ON scheduled_settlement
FOR EACH ROW
FOLLOWS tr_scheduled_settlement_reconciliation_insert
SET NEW.status = IF(
    NEW.kind <> 'livingCostMonth'
        OR EXISTS (
            SELECT 1
            FROM living_cost_month AS living_month
            WHERE living_month.id = CAST(
                      JSON_UNQUOTE(JSON_EXTRACT(NEW.payload, '$.livingCostMonthId'))
                      AS UNSIGNED
                  )
              AND living_month.save_id = NEW.save_id
              AND living_month.run_revision = NEW.run_revision
              AND living_month.status = 'pending'
              AND living_month.due_game_day = NEW.due_game_day
              AND BINARY NEW.source_id = BINARY CAST(living_month.id AS CHAR)
              AND (
                  SELECT COUNT(*)
                  FROM living_cost_month_item AS item
                  WHERE item.living_cost_month_id = living_month.id
              ) = 9
        ),
    NEW.status,
    NULL
);

-- M4 source names are closed even though older finance source names remain extensible.
ALTER TABLE ledger_transaction
    ADD CONSTRAINT ck_ledger_transaction_life_source CHECK (
        source_kind NOT LIKE 'livingCost%'
        AND source_kind NOT LIKE 'essentialArrear%'
        OR source_kind IN ('livingCostMonth', 'essentialArrearPayment')
    );

ALTER TABLE ledger_posting
    DROP CHECK ck_ledger_posting_account_code,
    DROP CHECK ck_ledger_posting_account_reference,
    ADD COLUMN living_cost_month_id BIGINT UNSIGNED NULL
        AFTER military_savings_contract_id,
    ADD COLUMN essential_arrear_id BIGINT UNSIGNED NULL
        AFTER living_cost_month_id,
    ADD KEY ix_ledger_posting_living_cost_month
        (save_id, run_revision, living_cost_month_id),
    ADD KEY ix_ledger_posting_essential_arrear
        (save_id, run_revision, essential_arrear_id),
    ADD CONSTRAINT fk_ledger_posting_living_cost_month
        FOREIGN KEY (save_id, run_revision, living_cost_month_id)
        REFERENCES living_cost_month (save_id, run_revision, id),
    ADD CONSTRAINT fk_ledger_posting_essential_arrear
        FOREIGN KEY (save_id, run_revision, essential_arrear_id)
        REFERENCES essential_arrear (save_id, run_revision, id),
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
            'livingCostExpense', 'essentialArrearLiability'
        )
    ),
    ADD CONSTRAINT ck_ledger_posting_account_reference CHECK (
        (
            account_code IN (
                'accountCash', 'productPrincipal',
                'pensionTaxExcludedContribution', 'pensionCreditedContribution'
            )
            AND financial_account_id IS NOT NULL
            AND military_savings_contract_id IS NULL
            AND living_cost_month_id IS NULL
            AND essential_arrear_id IS NULL
        )
        OR (
            account_code IN (
                'militarySavingsPrincipal', 'militarySavingsBankInterest',
                'militarySavingsGovernmentMatchIncome'
            )
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NOT NULL
            AND living_cost_month_id IS NULL
            AND essential_arrear_id IS NULL
        )
        OR (
            account_code = 'livingCostExpense'
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NULL
            AND living_cost_month_id IS NOT NULL
            AND essential_arrear_id IS NULL
        )
        OR (
            account_code = 'essentialArrearLiability'
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NULL
            AND living_cost_month_id IS NULL
            AND essential_arrear_id IS NOT NULL
        )
        OR (
            account_code NOT IN (
                'accountCash', 'productPrincipal',
                'pensionTaxExcludedContribution', 'pensionCreditedContribution',
                'militarySavingsPrincipal', 'militarySavingsBankInterest',
                'militarySavingsGovernmentMatchIncome',
                'livingCostExpense', 'essentialArrearLiability'
            )
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NULL
            AND living_cost_month_id IS NULL
            AND essential_arrear_id IS NULL
        )
    );

CREATE TRIGGER tr_ledger_transaction_life_source_insert
BEFORE INSERT ON ledger_transaction
FOR EACH ROW
SET NEW.source_kind = IF(
    (
        NEW.source_kind = 'livingCostMonth'
        AND EXISTS (
            SELECT 1 FROM living_cost_month AS living_month
            WHERE living_month.id = CAST(NEW.source_id AS UNSIGNED)
              AND BINARY NEW.source_id = BINARY CAST(living_month.id AS CHAR)
              AND living_month.save_id = NEW.save_id
              AND living_month.run_revision = NEW.run_revision
              AND living_month.status = 'pending'
        )
    )
    OR (
        NEW.source_kind = 'essentialArrearPayment'
        AND EXISTS (
            SELECT 1 FROM essential_arrear_payment AS payment
            WHERE payment.id = CAST(NEW.source_id AS UNSIGNED)
              AND BINARY NEW.source_id = BINARY CAST(payment.id AS CHAR)
              AND payment.save_id = NEW.save_id
              AND payment.run_revision = NEW.run_revision
              AND payment.status = 'prepared'
        )
    )
    OR NEW.source_kind NOT IN ('livingCostMonth', 'essentialArrearPayment'),
    NEW.source_kind,
    NULL
);

CREATE TRIGGER tr_ledger_posting_life_reference_insert
BEFORE INSERT ON ledger_posting
FOR EACH ROW
SET NEW.account_code = IF(
    (
        NEW.account_code = 'livingCostExpense'
        AND EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            INNER JOIN living_cost_month AS living_month
                ON living_month.save_id = ledger.save_id
               AND living_month.run_revision = ledger.run_revision
               AND BINARY CAST(living_month.id AS CHAR) = BINARY ledger.source_id
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'livingCostMonth'
              AND living_month.id = NEW.living_cost_month_id
        )
    )
    OR (
        NEW.account_code = 'essentialArrearLiability'
        AND EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            INNER JOIN essential_arrear AS arrear
                ON arrear.save_id = ledger.save_id
               AND arrear.run_revision = ledger.run_revision
               AND arrear.id = NEW.essential_arrear_id
            LEFT JOIN living_cost_month_item AS item
                ON item.id = arrear.living_cost_month_item_id
            LEFT JOIN living_cost_month AS living_month
                ON living_month.id = item.living_cost_month_id
            LEFT JOIN essential_arrear_payment AS payment
                ON payment.essential_arrear_id = arrear.id
               AND BINARY CAST(payment.id AS CHAR) = BINARY ledger.source_id
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND (
                  (ledger.source_kind = 'livingCostMonth'
                   AND BINARY ledger.source_id = BINARY CAST(living_month.id AS CHAR))
                  OR (ledger.source_kind = 'essentialArrearPayment'
                      AND payment.status = 'prepared')
              )
        )
    )
    OR (
        NEW.account_code NOT IN ('livingCostExpense', 'essentialArrearLiability')
        AND NEW.living_cost_month_id IS NULL
        AND NEW.essential_arrear_id IS NULL
    ),
    NEW.account_code,
    NULL
);

-- Pin every existing run before creating any M4-owned state. Revision zero is explicit bridge
-- provenance for assignments that did not exist when that run was created.
INSERT INTO run_rule_bundle
    (
        save_id,
        run_revision,
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
        bundle_assignment_revision
    )
SELECT
    save.id,
    save.run_revision,
    save.market_world_id,
    save.policy_set_id,
    career_run.career_catalog_bundle_id,
    career_run.employment_policy_set_id,
    life.id,
    credit.id,
    real_estate.id,
    CASE market_world.world_key
        WHEN 'm1-2026-v1' THEN 0
        WHEN 'm1-2026-v2' THEN 1
        WHEN 'm1-2026-v3' THEN 2
        WHEN 'm2-2026-v4' THEN 3
    END,
    CASE policy_set.policy_key
        WHEN 'kr-individual-2026-v1' THEN 1
        WHEN 'kr-individual-2026-v2' THEN 2
    END,
    1,
    1,
    0
FROM save
INNER JOIN market_world ON market_world.id = save.market_world_id
INNER JOIN policy_set ON policy_set.id = save.policy_set_id
INNER JOIN career_run
    ON career_run.save_id = save.id
   AND career_run.run_revision = save.run_revision
INNER JOIN life_catalog_set AS life
    ON life.catalog_key = IF(
        market_world.world_key = 'm2-2026-v4',
        'dev-unranked-m4-life-2026-v1',
        'compatibility-m4a-pre-cpi-v1'
    )
   AND life.sealed_at IS NOT NULL
INNER JOIN credit_model_version AS credit
    ON credit.version_key = 'disabled-m4a-v1' AND credit.sealed_at IS NOT NULL
INNER JOIN real_estate_model_version AS real_estate
    ON real_estate.version_key = 'disabled-m4a-v1' AND real_estate.sealed_at IS NOT NULL;

-- From this point forward a run can only pin the locked composite newRun row. The permissive
-- insert trigger existed solely for the one-time historical bridge above.
DROP TRIGGER tr_run_rule_bundle_valid_insert;

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
        INNER JOIN run_rule_bundle_assignment AS assignment
            ON assignment.assignment_key = 'newRun'
        WHERE save.id = NEW.save_id
          AND save.run_revision = NEW.run_revision
          AND save.market_world_id = NEW.market_world_id
          AND save.policy_set_id = NEW.policy_set_id
          AND career_run.career_catalog_bundle_id = NEW.career_catalog_bundle_id
          AND career_run.employment_policy_set_id = NEW.employment_policy_set_id
          AND assignment.market_world_id = NEW.market_world_id
          AND assignment.policy_set_id = NEW.policy_set_id
          AND assignment.career_catalog_bundle_id = NEW.career_catalog_bundle_id
          AND assignment.employment_policy_set_id = NEW.employment_policy_set_id
          AND assignment.life_catalog_set_id = NEW.life_catalog_set_id
          AND assignment.credit_model_version_id = NEW.credit_model_version_id
          AND assignment.real_estate_model_version_id = NEW.real_estate_model_version_id
          AND assignment.market_assignment_revision = NEW.market_assignment_revision
          AND assignment.finance_assignment_revision = NEW.finance_assignment_revision
          AND assignment.career_assignment_revision = NEW.career_assignment_revision
          AND assignment.employment_assignment_revision = NEW.employment_assignment_revision
          AND assignment.assignment_revision = NEW.bundle_assignment_revision
    ),
    NEW.save_id,
    NULL
);

INSERT INTO household
    (
        save_id,
        run_revision,
        life_catalog_set_id,
        legacy_debt_krw_at_activation,
        created_game_day
    )
SELECT save.id,
       save.run_revision,
       bundle.life_catalog_set_id,
       save.debt_krw,
       0
FROM save
INNER JOIN run_rule_bundle AS bundle
    ON bundle.save_id = save.id AND bundle.run_revision = save.run_revision;

INSERT INTO household_member
    (
        save_id,
        run_revision,
        household_id,
        member_role,
        ordinal,
        birth_date,
        joined_game_day,
        tax_dependent_eligible
    )
SELECT household.save_id,
       household.run_revision,
       household.id,
       'player',
       0,
       career_run.birth_date,
       0,
       FALSE
FROM household
INNER JOIN career_run
    ON career_run.save_id = household.save_id
   AND career_run.run_revision = household.run_revision;

INSERT INTO household_member
    (
        save_id,
        run_revision,
        household_id,
        member_role,
        ordinal,
        birth_date,
        joined_game_day,
        tax_dependent_eligible
    )
WITH RECURSIVE dependent_ordinal (ordinal) AS (
    SELECT CAST(1 AS UNSIGNED)
    UNION ALL
    SELECT ordinal + 1
    FROM dependent_ordinal
    WHERE ordinal < 100
)
SELECT household.save_id,
       household.run_revision,
       household.id,
       'dependent',
       dependent_ordinal.ordinal,
       MAKEDATE(
           YEAR(market_world.start_date) - life_catalog.legacy_dependent_age_years,
           1
       ),
       0,
       TRUE
FROM household
INNER JOIN save
    ON save.id = household.save_id AND save.run_revision = household.run_revision
INNER JOIN `character` ON `character`.save_id = save.id
INNER JOIN market_world ON market_world.id = save.market_world_id
INNER JOIN life_catalog_set AS life_catalog
    ON life_catalog.id = household.life_catalog_set_id
INNER JOIN dependent_ordinal ON dependent_ordinal.ordinal <= `character`.dependents;

INSERT INTO residence
    (
        save_id,
        run_revision,
        household_id,
        region_key,
        tenure_type,
        effective_from_game_day
    )
SELECT household.save_id,
       household.run_revision,
       household.id,
       region.region_key,
       'rentFree',
       0
FROM household
INNER JOIN `character` ON `character`.save_id = household.save_id
INNER JOIN life_region AS region
    ON BINARY region.region_key = BINARY `character`.region;

-- CPI-capable v4 runs receive a complete default budget and zero remainders. Compatibility runs
-- intentionally receive neither, so every living-cost mutation fails closed as rateUnavailable.
INSERT INTO household_budget
    (
        save_id,
        run_revision,
        household_id,
        cost_of_living_profile_id,
        effective_from_game_day
    )
SELECT household.save_id,
       household.run_revision,
       household.id,
       profile.id,
       save.game_day
FROM household
INNER JOIN save
    ON save.id = household.save_id AND save.run_revision = household.run_revision
INNER JOIN market_world ON market_world.id = save.market_world_id
INNER JOIN run_rule_bundle AS bundle
    ON bundle.save_id = save.id AND bundle.run_revision = save.run_revision
INNER JOIN life_catalog_set AS life ON life.id = bundle.life_catalog_set_id
INNER JOIN cost_of_living_profile AS profile
    ON profile.life_component_version_id = life.living_cost_component_version_id
WHERE market_world.world_key = 'm2-2026-v4';

INSERT INTO household_budget_selection
    (
        cost_of_living_profile_id,
        household_budget_id,
        living_cost_category_id,
        living_cost_budget_band_id
    )
SELECT budget.cost_of_living_profile_id,
       budget.id,
       category.id,
       category.default_budget_band_id
FROM household_budget AS budget
INNER JOIN living_cost_category AS category
    ON category.cost_of_living_profile_id = budget.cost_of_living_profile_id
WHERE budget.sealed_at IS NULL;

UPDATE household_budget
SET sealed_at = CURRENT_TIMESTAMP(3)
WHERE sealed_at IS NULL;

INSERT INTO living_cost_remainder
    (
        save_id,
        run_revision,
        household_id,
        cost_of_living_profile_id,
        living_cost_category_id,
        remainder_numerator,
        last_year_month
    )
SELECT budget.save_id,
       budget.run_revision,
       budget.household_id,
       budget.cost_of_living_profile_id,
       category.id,
       0,
       NULL
FROM household_budget AS budget
INNER JOIN living_cost_category AS category
    ON category.cost_of_living_profile_id = budget.cost_of_living_profile_id
WHERE budget.sealed_at IS NOT NULL
  AND budget.effective_to_game_day IS NULL;

DROP TEMPORARY TABLE m4a_bridge_guard;

-- This is intentionally the final rollout mutation: only now may a new run select active M4-A.
UPDATE run_rule_bundle_assignment AS assignment
INNER JOIN life_catalog_set AS active_life
    ON active_life.catalog_key = 'dev-unranked-m4-life-2026-v1'
   AND active_life.sealed_at IS NOT NULL
SET assignment.life_catalog_set_id = active_life.id
WHERE assignment.assignment_key = 'newRun';
