-- M4-B3 quote eligibility is published as a new immutable credit graph (§4.5).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;

INSERT INTO credit_model_version
    (version_key, availability, ranked_eligible, credit_policy_set_id, parameters)
SELECT
    'dev-unranked-m4b-credit-2026-v2',
    'active',
    FALSE,
    policy.id,
    JSON_OBJECT(
        'bands', JSON_ARRAY(
            JSON_OBJECT('band', 'prime', 'maximumUnits', 1000, 'minimumUnits', 850),
            JSON_OBJECT('band', 'standard', 'maximumUnits', 849, 'minimumUnits', 650),
            JSON_OBJECT('band', 'limited', 'maximumUnits', 649, 'minimumUnits', 450),
            JSON_OBJECT('band', 'distressed', 'maximumUnits', 449, 'minimumUnits', 1),
            JSON_OBJECT('band', 'insolvent', 'maximumUnits', 0, 'minimumUnits', 0)
        ),
        'creditUnits', JSON_OBJECT('initial', 700, 'maximum', 1000, 'minimum', 0),
        'dailyChange', JSON_OBJECT(
            'cleanRecoveryUnits', 1,
            'delinquentOrDefaultedPenaltyUnits', -5
        ),
        'defaultRule', JSON_OBJECT(
            'absoluteOldestBucketDays', 90,
            'amountAndAgeMinimumKrw', 1000000,
            'amountAndAgeOldestBucketDays', 30
        ),
        'eventPenalty', JSON_OBJECT(
            'activeToDelinquentUnits', -80,
            'delinquentToDefaultedUnits', -300,
            'legalProcedureUnits', 0
        ),
        'loanEligibility', JSON_OBJECT(
            'unsecuredLoan', JSON_OBJECT(
                'allowedCreditBands', JSON_ARRAY('prime', 'standard'),
                'disallowedContractStatuses',
                    JSON_ARRAY('delinquent', 'defaulted', 'restructured'),
                'maximumActiveContracts', 8
            )
        ),
        'provenance', 'GAME_BALANCE',
        'schemaVersion', 3
    )
FROM policy_set AS policy
WHERE policy.policy_key = 'dev-unranked-m4b-credit-policy-2026-v1'
  AND policy.sealed_at IS NOT NULL;

INSERT INTO loan_product_version
    (
        credit_model_version_id, product_key, display_name, catalog_scope, product_kind,
        lender_sector, rate_status, rate_type, reference_rate_key, fixed_annual_rate_bp,
        spread_bp, minimum_annual_rate_bp, maximum_annual_rate_bp, rate_reset_rule,
        day_count_rule, repayment_method, term_months, payment_calendar, grace_months,
        minimum_principal_krw, maximum_principal_krw, prepayment_fee_ppm,
        prepayment_effect, collateral_rule, starting_eligible, quote_eligible,
        execution_eligible, prepayment_allowed, dsr_included, read_only,
        provenance_kind, display_order
    )
SELECT
    model.id, 'dev-student-fixed-equal-principal-2026-v2', '개발 학자금 고정금리 대출',
    'modelChild', 'studentLoan', 'bank', 'available', 'fixed', NULL, 170,
    NULL, 170, 170, 'none', 'actual365', 'equalPrincipal', 120, 'monthEnd', 0,
    1, 50000000, 0, 'reduceTerm', 'none', TRUE, FALSE, FALSE, TRUE, TRUE, FALSE,
    'GAME_BALANCE', 1
FROM credit_model_version AS model
WHERE model.version_key = 'dev-unranked-m4b-credit-2026-v2'
UNION ALL
SELECT
    model.id, 'dev-unsecured-variable-level-payment-2026-v2', '개발 변동금리 신용대출',
    'modelChild', 'unsecuredLoan', 'bank', 'available', 'variable', 'treasury3m', NULL,
    400, 300, 1500, 'monthlyDay1', 'actual365', 'levelPayment', 60, 'monthEnd', 0,
    1, 200000000, 10000, 'recalculatePayment', 'none', TRUE, TRUE, TRUE, TRUE, TRUE,
    FALSE, 'GAME_BALANCE', 2
FROM credit_model_version AS model
WHERE model.version_key = 'dev-unranked-m4b-credit-2026-v2';

INSERT INTO loan_product_canonical_manifest (loan_product_version_id, canonical_json)
SELECT projection.loan_product_version_id, projection.canonical_json
FROM loan_product_canonical_projection AS projection
INNER JOIN loan_product_version AS product
    ON product.id = projection.loan_product_version_id
WHERE product.credit_model_version_id = (
    SELECT id FROM credit_model_version
    WHERE version_key = 'dev-unranked-m4b-credit-2026-v2'
);

UPDATE loan_product_version AS product
INNER JOIN loan_product_canonical_manifest AS manifest
    ON manifest.loan_product_version_id = product.id
SET product.canonical_sha256 = manifest.canonical_sha256,
    product.sealed_at = CURRENT_TIMESTAMP(3)
WHERE product.credit_model_version_id = (
    SELECT id FROM credit_model_version
    WHERE version_key = 'dev-unranked-m4b-credit-2026-v2'
)
  AND product.sealed_at IS NULL;

INSERT INTO loan_product_legacy_start_mapping
    (
        credit_model_version_id, legacy_field_key, product_kind,
        loan_product_version_id, mapping_order
    )
SELECT model.id, 'studentLoanKrw', 'studentLoan', product.id, 1
FROM credit_model_version AS model
INNER JOIN loan_product_version AS product
    ON product.credit_model_version_id = model.id
   AND product.product_kind = 'studentLoan'
WHERE model.version_key = 'dev-unranked-m4b-credit-2026-v2'
UNION ALL
SELECT model.id, 'creditLoanKrw', 'unsecuredLoan', product.id, 2
FROM credit_model_version AS model
INNER JOIN loan_product_version AS product
    ON product.credit_model_version_id = model.id
   AND product.product_kind = 'unsecuredLoan'
WHERE model.version_key = 'dev-unranked-m4b-credit-2026-v2';

INSERT INTO credit_model_strict_manifest (credit_model_version_id, canonical_json)
SELECT credit_model_version_id, canonical_json
FROM credit_model_strict_projection
WHERE credit_model_version_id = (
    SELECT id FROM credit_model_version
    WHERE version_key = 'dev-unranked-m4b-credit-2026-v2'
);

UPDATE credit_model_version AS model
INNER JOIN credit_model_strict_manifest AS manifest
    ON manifest.credit_model_version_id = model.id
SET model.canonical_sha256 = manifest.canonical_sha256,
    model.sealed_at = CURRENT_TIMESTAMP(3)
WHERE model.version_key = 'dev-unranked-m4b-credit-2026-v2'
  AND model.sealed_at IS NULL;

-- Existing runs keep v1. Only future runs pin the quote-capable v2 graph.
UPDATE run_rule_bundle_assignment AS assignment
INNER JOIN credit_model_version AS active_credit
    ON active_credit.version_key = 'dev-unranked-m4b-credit-2026-v2'
   AND active_credit.availability = 'active'
   AND active_credit.sealed_at IS NOT NULL
SET assignment.credit_model_version_id = active_credit.id
WHERE assignment.assignment_key = 'newRun';
