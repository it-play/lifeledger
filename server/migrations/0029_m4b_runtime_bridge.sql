-- M4-B fail-closed debt-authority bridge and final new-run credit activation (§4.1, §4.5).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;

-- Tax shortages created after M4-A activation are the only opaque debt increases that may sit
-- above the captured legacy principal and active essential arrears. Preserve their exact ledger
-- evidence before changing any authority.
CREATE TEMPORARY TABLE m4b_tax_debt_evidence (
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    household_id                    BIGINT UNSIGNED NOT NULL,
    policy_set_id                   BIGINT UNSIGNED NOT NULL,
    source_kind                     VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    source_id                       VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    due_game_day                    INT UNSIGNED    NOT NULL,
    original_amount_krw             BIGINT          NOT NULL,
    authority_ledger_transaction_id BIGINT UNSIGNED NOT NULL,
    PRIMARY KEY (save_id, run_revision, source_kind, source_id),
    UNIQUE KEY uk_m4b_tax_evidence_ledger
        (save_id, run_revision, authority_ledger_transaction_id),
    CONSTRAINT ck_m4b_tax_evidence_amount CHECK (original_amount_krw > 0)
) ENGINE = InnoDB;

INSERT INTO m4b_tax_debt_evidence
    (
        save_id, run_revision, household_id, policy_set_id, source_kind, source_id,
        due_game_day, original_amount_krw, authority_ledger_transaction_id
    )
SELECT
    settlement.save_id,
    settlement.run_revision,
    household.id,
    ledger.policy_set_id,
    CASE settlement.kind
        WHEN 'financialIncomeFiling' THEN 'financialIncomeAssessment'
        WHEN 'employmentReconciliation' THEN 'yearEndTaxAssessment'
    END,
    settlement.source_id,
    settlement.due_game_day,
    -SUM(posting.amount_krw),
    ledger.id
FROM scheduled_settlement AS settlement
INNER JOIN household
    ON household.save_id = settlement.save_id
   AND household.run_revision = settlement.run_revision
INNER JOIN ledger_transaction AS ledger
    ON ledger.id = settlement.settled_ledger_transaction_id
   AND ledger.save_id = settlement.save_id
   AND ledger.run_revision = settlement.run_revision
   AND ledger.created_at >= household.created_at
INNER JOIN ledger_posting AS posting
    ON posting.ledger_transaction_id = ledger.id
   AND posting.save_id = ledger.save_id
   AND posting.run_revision = ledger.run_revision
   AND posting.account_code = 'debtPrincipal'
WHERE settlement.status = 'settled'
  AND settlement.kind IN ('financialIncomeFiling', 'employmentReconciliation')
GROUP BY
    settlement.save_id,
    settlement.run_revision,
    household.id,
    ledger.policy_set_id,
    settlement.kind,
    settlement.source_id,
    settlement.due_game_day,
    ledger.id
HAVING SUM(posting.amount_krw) < 0;

CREATE TEMPORARY TABLE m4b_bridge_guard (
    guard_key      VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    accepted       TINYINT UNSIGNED NOT NULL,
    PRIMARY KEY (guard_key),
    CONSTRAINT ck_m4b_bridge_guard CHECK (accepted = 1)
) ENGINE = InnoDB;

-- Refuse any graph or aggregate that cannot be reconstructed without guessing provenance.
INSERT INTO m4b_bridge_guard (guard_key, accepted)
SELECT 'knownDebtGraphs',
       IF(
           NOT EXISTS (
               SELECT 1
               FROM save
               LEFT JOIN household
                   ON household.save_id = save.id
                  AND household.run_revision = save.run_revision
               LEFT JOIN run_rule_bundle AS bundle
                   ON bundle.save_id = save.id
                  AND bundle.run_revision = save.run_revision
               LEFT JOIN credit_model_version AS credit
                   ON credit.id = bundle.credit_model_version_id
               WHERE household.id IS NULL
                  OR bundle.save_id IS NULL
                  OR credit.version_key <> 'disabled-m4a-v1'
                  OR credit.availability <> 'disabled'
                  OR save.debt_krw < 0
                  OR household.legacy_debt_krw_at_activation < 0
                  OR CAST(save.debt_krw AS DECIMAL(65, 0))
                     <> CAST(household.legacy_debt_krw_at_activation AS DECIMAL(65, 0))
                        + COALESCE((
                            SELECT SUM(
                                CAST(arrear.original_amount_krw AS DECIMAL(65, 0))
                                - CAST(arrear.paid_amount_krw AS DECIMAL(65, 0))
                            )
                            FROM essential_arrear AS arrear
                            WHERE arrear.save_id = save.id
                              AND arrear.run_revision = save.run_revision
                              AND arrear.status = 'active'
                        ), 0)
                        + COALESCE((
                            SELECT SUM(CAST(evidence.original_amount_krw AS DECIMAL(65, 0)))
                            FROM m4b_tax_debt_evidence AS evidence
                            WHERE evidence.save_id = save.id
                              AND evidence.run_revision = save.run_revision
                        ), 0)
           )
           AND NOT EXISTS (SELECT 1 FROM loan_contract)
           AND NOT EXISTS (SELECT 1 FROM tax_obligation)
           AND NOT EXISTS (SELECT 1 FROM loan_authority_bridge)
           AND NOT EXISTS (SELECT 1 FROM tax_authority_bridge)
           AND EXISTS (
               SELECT 1 FROM credit_model_version
               WHERE version_key = 'dev-unranked-m4b-credit-2026-v1'
                 AND availability = 'active'
                 AND sealed_at IS NOT NULL
           )
           AND EXISTS (
               SELECT 1 FROM loan_product_version
               WHERE product_key = 'compat-legacy-debt-zero-bullet-v1'
                 AND catalog_scope = 'bridgeOnly'
                 AND credit_model_version_id IS NULL
                 AND sealed_at IS NOT NULL
           ),
           1,
           NULL
       );

INSERT INTO m4b_bridge_guard (guard_key, accepted)
SELECT 'knownTaxEvidence',
       IF(
           NOT EXISTS (
               SELECT 1
               FROM m4b_tax_debt_evidence AS evidence
               INNER JOIN save
                   ON save.id = evidence.save_id
                  AND save.run_revision = evidence.run_revision
               WHERE evidence.policy_set_id <> save.policy_set_id
                  OR (
                      evidence.source_kind = 'financialIncomeAssessment'
                      AND NOT EXISTS (
                          SELECT 1 FROM financial_income_assessment AS assessment
                          WHERE assessment.save_id = evidence.save_id
                            AND assessment.run_revision = evidence.run_revision
                            AND assessment.tax_year = CAST(evidence.source_id AS UNSIGNED)
                            AND BINARY evidence.source_id
                                = BINARY CAST(assessment.tax_year AS CHAR)
                            AND assessment.status = 'filed'
                            AND evidence.original_amount_krw
                                <= assessment.additional_tax_krw
                      )
                  )
                  OR (
                      evidence.source_kind = 'yearEndTaxAssessment'
                      AND NOT EXISTS (
                          SELECT 1 FROM year_end_tax_assessment AS assessment
                          WHERE assessment.id = CAST(evidence.source_id AS UNSIGNED)
                            AND BINARY evidence.source_id = BINARY CAST(assessment.id AS CHAR)
                            AND assessment.save_id = evidence.save_id
                            AND assessment.run_revision = evidence.run_revision
                            AND assessment.assessment_status = 'definitive'
                            AND evidence.original_amount_krw
                                <= assessment.additional_tax_krw
                      )
                  )
           ),
           1,
           NULL
       );

INSERT INTO tax_obligation
    (
        save_id, run_revision, household_id, policy_set_id, source_kind, source_id,
        due_game_day, original_amount_krw, paid_amount_krw, outstanding_amount_krw,
        status, authority_ledger_transaction_id
    )
SELECT
    evidence.save_id,
    evidence.run_revision,
    evidence.household_id,
    evidence.policy_set_id,
    evidence.source_kind,
    evidence.source_id,
    evidence.due_game_day,
    evidence.original_amount_krw,
    0,
    evidence.original_amount_krw,
    'outstanding',
    evidence.authority_ledger_transaction_id
FROM m4b_tax_debt_evidence AS evidence;

-- Move the old generic liability ledger balance to the typed tax-obligation account without
-- changing net worth or the save projection.
INSERT INTO ledger_transaction
    (save_id, run_revision, game_day, policy_set_id, source_kind, source_id, description)
SELECT
    obligation.save_id,
    obligation.run_revision,
    save.game_day,
    obligation.policy_set_id,
    'debtAuthorityBridge',
    CONCAT('taxObligation:', CAST(obligation.id AS CHAR)),
    '세금 부채 권위 이관'
FROM tax_obligation AS obligation
INNER JOIN save
    ON save.id = obligation.save_id AND save.run_revision = obligation.run_revision;

INSERT INTO ledger_posting
    (
        save_id, run_revision, ledger_transaction_id, posting_order, account_code,
        financial_account_id, military_savings_contract_id, living_cost_month_id,
        essential_arrear_id, loan_contract_id, tax_obligation_id, amount_krw
    )
SELECT
    obligation.save_id,
    obligation.run_revision,
    ledger.id,
    1,
    'debtPrincipal',
    NULL, NULL, NULL, NULL, NULL, NULL,
    obligation.original_amount_krw
FROM tax_obligation AS obligation
INNER JOIN ledger_transaction AS ledger
    ON ledger.save_id = obligation.save_id
   AND ledger.run_revision = obligation.run_revision
   AND ledger.source_kind = 'debtAuthorityBridge'
   AND BINARY ledger.source_id
        = BINARY CONCAT('taxObligation:', CAST(obligation.id AS CHAR))
UNION ALL
SELECT
    obligation.save_id,
    obligation.run_revision,
    ledger.id,
    2,
    'taxObligationLiability',
    NULL, NULL, NULL, NULL, NULL, obligation.id,
    -obligation.original_amount_krw
FROM tax_obligation AS obligation
INNER JOIN ledger_transaction AS ledger
    ON ledger.save_id = obligation.save_id
   AND ledger.run_revision = obligation.run_revision
   AND ledger.source_kind = 'debtAuthorityBridge'
   AND BINARY ledger.source_id
        = BINARY CONCAT('taxObligation:', CAST(obligation.id AS CHAR));

INSERT INTO tax_authority_bridge
    (
        tax_obligation_id, save_id, run_revision, household_id, bridged_amount_krw,
        ledger_transaction_id, bridge_key
    )
SELECT
    obligation.id,
    obligation.save_id,
    obligation.run_revision,
    obligation.household_id,
    obligation.original_amount_krw,
    ledger.id,
    'migration0029TaxDebt'
FROM tax_obligation AS obligation
INNER JOIN ledger_transaction AS ledger
    ON ledger.save_id = obligation.save_id
   AND ledger.run_revision = obligation.run_revision
   AND ledger.source_kind = 'debtAuthorityBridge'
   AND BINARY ledger.source_id
        = BINARY CONCAT('taxObligation:', CAST(obligation.id AS CHAR));

-- Each pre-M4 principal becomes one visible, immutable, unscheduled compatibility contract.
INSERT INTO loan_contract
    (
        save_id, run_revision, household_id, credit_model_version_id,
        loan_product_version_id, loan_quote_id, origin_kind, origin_command_id,
        product_kind, lender_sector, rate_status, rate_type, reference_rate_key,
        fixed_annual_rate_bp, applied_spread_bp, minimum_annual_rate_bp,
        maximum_annual_rate_bp, current_annual_rate_bp, rate_reset_rule,
        day_count_denominator, repayment_method, term_months, total_installments,
        payment_calendar, grace_months, prepayment_fee_ppm, prepayment_effect,
        dsr_included, read_only, status, original_principal_krw,
        remaining_principal_krw, accrued_interest_krw, accrued_fee_krw,
        interest_remainder_numerator, activated_game_day, maturity_game_day,
        next_installment_no, oldest_unpaid_due_game_day
    )
SELECT
    household.save_id,
    household.run_revision,
    household.id,
    bundle.credit_model_version_id,
    product.id,
    NULL,
    'legacyDebtBridge',
    NULL,
    'legacyDebt',
    'bridgeOnly',
    'rateUnavailable',
    'unavailable',
    NULL, NULL, NULL, NULL, NULL, NULL,
    'none',
    NULL,
    'bullet',
    NULL, NULL,
    'none',
    NULL, NULL,
    'forbidden',
    FALSE,
    TRUE,
    'active',
    household.legacy_debt_krw_at_activation,
    household.legacy_debt_krw_at_activation,
    0,
    0,
    0,
    household.created_game_day,
    NULL, NULL, NULL
FROM household
INNER JOIN run_rule_bundle AS bundle
    ON bundle.save_id = household.save_id
   AND bundle.run_revision = household.run_revision
INNER JOIN loan_product_version AS product
    ON product.product_key = 'compat-legacy-debt-zero-bullet-v1'
   AND product.catalog_scope = 'bridgeOnly'
   AND product.sealed_at IS NOT NULL
WHERE household.legacy_debt_krw_at_activation > 0;

INSERT INTO ledger_transaction
    (save_id, run_revision, game_day, policy_set_id, source_kind, source_id, description)
SELECT
    contract.save_id,
    contract.run_revision,
    save.game_day,
    save.policy_set_id,
    'debtAuthorityBridge',
    CAST(contract.id AS CHAR),
    '이전 버전 부채 권위 이관'
FROM loan_contract AS contract
INNER JOIN save
    ON save.id = contract.save_id AND save.run_revision = contract.run_revision
WHERE contract.origin_kind = 'legacyDebtBridge';

INSERT INTO ledger_posting
    (
        save_id, run_revision, ledger_transaction_id, posting_order, account_code,
        financial_account_id, military_savings_contract_id, living_cost_month_id,
        essential_arrear_id, loan_contract_id, tax_obligation_id, amount_krw
    )
SELECT
    contract.save_id,
    contract.run_revision,
    ledger.id,
    1,
    'debtPrincipal',
    NULL, NULL, NULL, NULL, NULL, NULL,
    contract.original_principal_krw
FROM loan_contract AS contract
INNER JOIN ledger_transaction AS ledger
    ON ledger.save_id = contract.save_id
   AND ledger.run_revision = contract.run_revision
   AND ledger.source_kind = 'debtAuthorityBridge'
   AND BINARY ledger.source_id = BINARY CAST(contract.id AS CHAR)
WHERE contract.origin_kind = 'legacyDebtBridge'
UNION ALL
SELECT
    contract.save_id,
    contract.run_revision,
    ledger.id,
    2,
    'loanPrincipalLiability',
    NULL, NULL, NULL, NULL, contract.id, NULL,
    -contract.original_principal_krw
FROM loan_contract AS contract
INNER JOIN ledger_transaction AS ledger
    ON ledger.save_id = contract.save_id
   AND ledger.run_revision = contract.run_revision
   AND ledger.source_kind = 'debtAuthorityBridge'
   AND BINARY ledger.source_id = BINARY CAST(contract.id AS CHAR)
WHERE contract.origin_kind = 'legacyDebtBridge';

INSERT INTO loan_authority_bridge
    (
        loan_contract_id, save_id, run_revision, household_id, bridged_principal_krw,
        ledger_transaction_id, bridge_key
    )
SELECT
    contract.id,
    contract.save_id,
    contract.run_revision,
    contract.household_id,
    contract.original_principal_krw,
    ledger.id,
    'migration0029LegacyDebt'
FROM loan_contract AS contract
INNER JOIN ledger_transaction AS ledger
    ON ledger.save_id = contract.save_id
   AND ledger.run_revision = contract.run_revision
   AND ledger.source_kind = 'debtAuthorityBridge'
   AND BINARY ledger.source_id = BINARY CAST(contract.id AS CHAR)
WHERE contract.origin_kind = 'legacyDebtBridge';

-- The post-bridge projection must have one authority for every won and no residual aggregate.
INSERT INTO m4b_bridge_guard (guard_key, accepted)
SELECT 'authorityProjection',
       IF(
           NOT EXISTS (
               SELECT 1
               FROM save
               INNER JOIN household
                   ON household.save_id = save.id
                  AND household.run_revision = save.run_revision
               WHERE CAST(save.debt_krw AS DECIMAL(65, 0))
                     <> COALESCE((
                            SELECT SUM(
                                CAST(contract.remaining_principal_krw AS DECIMAL(65, 0))
                                + CAST(contract.accrued_interest_krw AS DECIMAL(65, 0))
                                + CAST(contract.accrued_fee_krw AS DECIMAL(65, 0))
                            )
                            FROM loan_contract AS contract
                            WHERE contract.save_id = save.id
                              AND contract.run_revision = save.run_revision
                              AND contract.status IN ('active', 'delinquent', 'defaulted')
                        ), 0)
                        + COALESCE((
                            SELECT SUM(
                                CAST(arrear.original_amount_krw AS DECIMAL(65, 0))
                                - CAST(arrear.paid_amount_krw AS DECIMAL(65, 0))
                            )
                            FROM essential_arrear AS arrear
                            WHERE arrear.save_id = save.id
                              AND arrear.run_revision = save.run_revision
                              AND arrear.status = 'active'
                        ), 0)
                        + COALESCE((
                            SELECT SUM(CAST(obligation.outstanding_amount_krw AS DECIMAL(65, 0)))
                            FROM tax_obligation AS obligation
                            WHERE obligation.save_id = save.id
                              AND obligation.run_revision = save.run_revision
                              AND obligation.status = 'outstanding'
                        ), 0)
                  OR (
                      household.legacy_debt_krw_at_activation > 0
                      AND NOT EXISTS (
                          SELECT 1 FROM loan_authority_bridge AS bridge
                          INNER JOIN loan_contract AS contract
                              ON contract.id = bridge.loan_contract_id
                          WHERE bridge.household_id = household.id
                            AND bridge.bridged_principal_krw
                                = household.legacy_debt_krw_at_activation
                            AND contract.origin_kind = 'legacyDebtBridge'
                      )
                  )
                  OR EXISTS (
                      SELECT 1 FROM tax_obligation AS obligation
                      WHERE obligation.household_id = household.id
                        AND NOT EXISTS (
                            SELECT 1 FROM tax_authority_bridge AS bridge
                            WHERE bridge.tax_obligation_id = obligation.id
                              AND bridge.bridged_amount_krw
                                  = obligation.original_amount_krw
                        )
                  )
           ),
           1,
           NULL
       );

DROP TEMPORARY TABLE m4b_tax_debt_evidence;
DROP TEMPORARY TABLE m4b_bridge_guard;

-- Existing run bundles stay immutable. Only future runs pin the active credit graph, and the
-- composite assignment trigger advances its own revision atomically.
UPDATE run_rule_bundle_assignment AS assignment
INNER JOIN credit_model_version AS active_credit
    ON active_credit.version_key = 'dev-unranked-m4b-credit-2026-v1'
   AND active_credit.availability = 'active'
   AND active_credit.sealed_at IS NOT NULL
SET assignment.credit_model_version_id = active_credit.id
WHERE assignment.assignment_key = 'newRun';
