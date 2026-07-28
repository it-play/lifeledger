-- M4-E2 policy v5 exact-clones the E1 finance rules. Carry the typed C4 property-tax
-- profiles with those rules so the real-estate v6 planner remains executable.

DROP TRIGGER tr_property_acquisition_tax_policy_draft_insert;
DROP TRIGGER tr_property_annual_tax_policy_draft_insert;
DROP TRIGGER tr_property_annual_fair_market_draft_insert;
DROP TRIGGER tr_property_annual_tax_rate_draft_insert;
DROP TRIGGER tr_property_capital_gains_tax_policy_draft_insert;
DROP TRIGGER tr_property_capital_gains_tax_rate_draft_insert;

INSERT INTO property_acquisition_tax_policy_profile
    (
        policy_set_id, rule_id, supported_home_count,
        lower_price_maximum_krw, middle_price_maximum_krw,
        lower_rate_ppm, upper_rate_ppm, middle_rate_price_divisor_krw,
        middle_rate_offset_ppm, middle_rate_rounding,
        local_education_rate_ratio_ppm, payment_due_days
    )
SELECT target_policy.id, target_rule.id, source.supported_home_count,
       source.lower_price_maximum_krw, source.middle_price_maximum_krw,
       source.lower_rate_ppm, source.upper_rate_ppm,
       source.middle_rate_price_divisor_krw, source.middle_rate_offset_ppm,
       source.middle_rate_rounding, source.local_education_rate_ratio_ppm,
       source.payment_due_days
FROM policy_set AS target_policy
INNER JOIN policy_rule AS target_rule
    ON target_rule.policy_set_id = target_policy.id
   AND target_rule.domain = 'propertyTax'
   AND target_rule.rule_key = 'singleHomeAcquisitionTax'
INNER JOIN policy_set AS source_policy
    ON source_policy.policy_key = 'dev-unranked-kr-individual-insolvency-2026-v4'
   AND source_policy.sealed_at IS NOT NULL
INNER JOIN property_acquisition_tax_policy_profile AS source
    ON source.policy_set_id = source_policy.id
WHERE target_policy.policy_key = 'dev-unranked-kr-corporation-2026-v5'
  AND target_policy.sealed_at IS NOT NULL;

INSERT INTO property_annual_tax_policy_profile
    (
        policy_set_id, rule_id, supported_home_count,
        assessment_month, assessment_day, ownership_cutoff_rule,
        official_value_ratio_ppm, special_rate_official_value_maximum_krw,
        local_education_rate_ratio_ppm, first_payment_month, first_payment_day,
        second_payment_month, second_payment_day, payment_split_rule,
        unsupported_exclusion_codes
    )
SELECT target_policy.id, target_rule.id, source.supported_home_count,
       source.assessment_month, source.assessment_day, source.ownership_cutoff_rule,
       source.official_value_ratio_ppm, source.special_rate_official_value_maximum_krw,
       source.local_education_rate_ratio_ppm, source.first_payment_month,
       source.first_payment_day, source.second_payment_month, source.second_payment_day,
       source.payment_split_rule, source.unsupported_exclusion_codes
FROM policy_set AS target_policy
INNER JOIN policy_rule AS target_rule
    ON target_rule.policy_set_id = target_policy.id
   AND target_rule.domain = 'propertyTax'
   AND target_rule.rule_key = 'singleHomeAnnualPropertyTax'
INNER JOIN policy_set AS source_policy
    ON source_policy.policy_key = 'dev-unranked-kr-individual-insolvency-2026-v4'
   AND source_policy.sealed_at IS NOT NULL
INNER JOIN property_annual_tax_policy_profile AS source
    ON source.policy_set_id = source_policy.id
WHERE target_policy.policy_key = 'dev-unranked-kr-corporation-2026-v5'
  AND target_policy.sealed_at IS NOT NULL;

INSERT INTO property_annual_tax_fair_market_ratio_band
    (
        policy_set_id, band_order, official_value_upper_bound_krw,
        fair_market_value_ratio_ppm
    )
SELECT target_policy.id, source.band_order, source.official_value_upper_bound_krw,
       source.fair_market_value_ratio_ppm
FROM policy_set AS target_policy
INNER JOIN policy_set AS source_policy
    ON source_policy.policy_key = 'dev-unranked-kr-individual-insolvency-2026-v4'
   AND source_policy.sealed_at IS NOT NULL
INNER JOIN property_annual_tax_fair_market_ratio_band AS source
    ON source.policy_set_id = source_policy.id
WHERE target_policy.policy_key = 'dev-unranked-kr-corporation-2026-v5'
  AND target_policy.sealed_at IS NOT NULL;

INSERT INTO property_annual_tax_rate_bracket
    (
        policy_set_id, rate_schedule, bracket_order,
        tax_base_upper_bound_krw, rate_ppm, progressive_deduction_krw
    )
SELECT target_policy.id, source.rate_schedule, source.bracket_order,
       source.tax_base_upper_bound_krw, source.rate_ppm,
       source.progressive_deduction_krw
FROM policy_set AS target_policy
INNER JOIN policy_set AS source_policy
    ON source_policy.policy_key = 'dev-unranked-kr-individual-insolvency-2026-v4'
   AND source_policy.sealed_at IS NOT NULL
INNER JOIN property_annual_tax_rate_bracket AS source
    ON source.policy_set_id = source_policy.id
WHERE target_policy.policy_key = 'dev-unranked-kr-corporation-2026-v5'
  AND target_policy.sealed_at IS NOT NULL;

INSERT INTO property_capital_gains_tax_policy_profile
    (
        policy_set_id, rule_id, supported_home_count,
        high_value_threshold_krw, basic_deduction_krw,
        minimum_holding_years, minimum_residence_years,
        holding_deduction_start_years, holding_deduction_start_rate_ppm,
        holding_deduction_per_year_ppm, holding_deduction_maximum_ppm,
        residence_deduction_start_years, residence_deduction_start_rate_ppm,
        residence_deduction_per_year_ppm, residence_deduction_maximum_ppm,
        local_income_tax_ratio_ppm, payment_rule
    )
SELECT target_policy.id, target_rule.id, source.supported_home_count,
       source.high_value_threshold_krw, source.basic_deduction_krw,
       source.minimum_holding_years, source.minimum_residence_years,
       source.holding_deduction_start_years, source.holding_deduction_start_rate_ppm,
       source.holding_deduction_per_year_ppm, source.holding_deduction_maximum_ppm,
       source.residence_deduction_start_years, source.residence_deduction_start_rate_ppm,
       source.residence_deduction_per_year_ppm, source.residence_deduction_maximum_ppm,
       source.local_income_tax_ratio_ppm, source.payment_rule
FROM policy_set AS target_policy
INNER JOIN policy_rule AS target_rule
    ON target_rule.policy_set_id = target_policy.id
   AND target_rule.domain = 'propertyTax'
   AND target_rule.rule_key = 'singleHomeCapitalGainsTax'
INNER JOIN policy_set AS source_policy
    ON source_policy.policy_key = 'dev-unranked-kr-individual-insolvency-2026-v4'
   AND source_policy.sealed_at IS NOT NULL
INNER JOIN property_capital_gains_tax_policy_profile AS source
    ON source.policy_set_id = source_policy.id
WHERE target_policy.policy_key = 'dev-unranked-kr-corporation-2026-v5'
  AND target_policy.sealed_at IS NOT NULL;

INSERT INTO property_capital_gains_tax_rate_bracket
    (
        policy_set_id, tax_scope, bracket_order,
        taxable_amount_upper_bound_krw, rate_ppm, progressive_deduction_krw
    )
SELECT target_policy.id, source.tax_scope, source.bracket_order,
       source.taxable_amount_upper_bound_krw, source.rate_ppm,
       source.progressive_deduction_krw
FROM policy_set AS target_policy
INNER JOIN policy_set AS source_policy
    ON source_policy.policy_key = 'dev-unranked-kr-individual-insolvency-2026-v4'
   AND source_policy.sealed_at IS NOT NULL
INNER JOIN property_capital_gains_tax_rate_bracket AS source
    ON source.policy_set_id = source_policy.id
WHERE target_policy.policy_key = 'dev-unranked-kr-corporation-2026-v5'
  AND target_policy.sealed_at IS NOT NULL;

CREATE TRIGGER tr_property_acquisition_tax_policy_draft_insert
BEFORE INSERT ON property_acquisition_tax_policy_profile
FOR EACH ROW
SET NEW.policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM policy_set AS policy
        INNER JOIN policy_rule AS rule
            ON rule.policy_set_id = policy.id AND rule.id = NEW.rule_id
        WHERE policy.id = NEW.policy_set_id
          AND policy.sealed_at IS NULL
          AND rule.domain = 'propertyTax'
          AND rule.rule_key = 'singleHomeAcquisitionTax'
    ),
    NEW.policy_set_id,
    NULL
);

CREATE TRIGGER tr_property_annual_tax_policy_draft_insert
BEFORE INSERT ON property_annual_tax_policy_profile
FOR EACH ROW
SET NEW.policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM policy_set AS policy
        INNER JOIN policy_rule AS rule
            ON rule.policy_set_id = policy.id AND rule.id = NEW.rule_id
        WHERE policy.id = NEW.policy_set_id
          AND policy.sealed_at IS NULL
          AND rule.domain = 'propertyTax'
          AND rule.rule_key = 'singleHomeAnnualPropertyTax'
    ),
    NEW.policy_set_id,
    NULL
);

CREATE TRIGGER tr_property_annual_fair_market_draft_insert
BEFORE INSERT ON property_annual_tax_fair_market_ratio_band
FOR EACH ROW
SET NEW.policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM policy_set AS policy
        INNER JOIN property_annual_tax_policy_profile AS profile
            ON profile.policy_set_id = policy.id
        WHERE policy.id = NEW.policy_set_id AND policy.sealed_at IS NULL
    ),
    NEW.policy_set_id,
    NULL
);

CREATE TRIGGER tr_property_annual_tax_rate_draft_insert
BEFORE INSERT ON property_annual_tax_rate_bracket
FOR EACH ROW
SET NEW.policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM policy_set AS policy
        INNER JOIN property_annual_tax_policy_profile AS profile
            ON profile.policy_set_id = policy.id
        WHERE policy.id = NEW.policy_set_id AND policy.sealed_at IS NULL
    ),
    NEW.policy_set_id,
    NULL
);

CREATE TRIGGER tr_property_capital_gains_tax_policy_draft_insert
BEFORE INSERT ON property_capital_gains_tax_policy_profile
FOR EACH ROW
SET NEW.policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM policy_set AS policy
        INNER JOIN policy_rule AS rule
            ON rule.policy_set_id = policy.id AND rule.id = NEW.rule_id
        WHERE policy.id = NEW.policy_set_id
          AND policy.sealed_at IS NULL
          AND rule.domain = 'propertyTax'
          AND rule.rule_key = 'singleHomeCapitalGainsTax'
    ),
    NEW.policy_set_id,
    NULL
);

CREATE TRIGGER tr_property_capital_gains_tax_rate_draft_insert
BEFORE INSERT ON property_capital_gains_tax_rate_bracket
FOR EACH ROW
SET NEW.policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM policy_set AS policy
        INNER JOIN property_capital_gains_tax_policy_profile AS profile
            ON profile.policy_set_id = policy.id
        WHERE policy.id = NEW.policy_set_id AND policy.sealed_at IS NULL
    ),
    NEW.policy_set_id,
    NULL
);

CREATE TEMPORARY TABLE m4e2_property_policy_guard (
    guard_key VARCHAR(40) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    accepted  TINYINT UNSIGNED NOT NULL,
    CONSTRAINT ck_m4e2_property_policy_guard CHECK (accepted = 1)
);

INSERT INTO m4e2_property_policy_guard (guard_key, accepted)
SELECT 'v5-property-profile-exact-clone', IF(
    EXISTS (
        SELECT 1
        FROM policy_set AS target
        INNER JOIN property_acquisition_tax_policy_profile AS acquisition
            ON acquisition.policy_set_id = target.id
        INNER JOIN property_annual_tax_policy_profile AS annual
            ON annual.policy_set_id = target.id
        INNER JOIN property_capital_gains_tax_policy_profile AS capital
            ON capital.policy_set_id = target.id
        WHERE target.policy_key = 'dev-unranked-kr-corporation-2026-v5'
          AND target.sealed_at IS NOT NULL
    )
        AND (
            SELECT COUNT(*)
            FROM property_annual_tax_fair_market_ratio_band AS target_band
            INNER JOIN policy_set AS target ON target.id = target_band.policy_set_id
            WHERE target.policy_key = 'dev-unranked-kr-corporation-2026-v5'
        ) = (
            SELECT COUNT(*)
            FROM property_annual_tax_fair_market_ratio_band AS source_band
            INNER JOIN policy_set AS source ON source.id = source_band.policy_set_id
            WHERE source.policy_key = 'dev-unranked-kr-individual-insolvency-2026-v4'
        )
        AND (
            SELECT COUNT(*)
            FROM property_annual_tax_rate_bracket AS target_bracket
            INNER JOIN policy_set AS target ON target.id = target_bracket.policy_set_id
            WHERE target.policy_key = 'dev-unranked-kr-corporation-2026-v5'
        ) = (
            SELECT COUNT(*)
            FROM property_annual_tax_rate_bracket AS source_bracket
            INNER JOIN policy_set AS source ON source.id = source_bracket.policy_set_id
            WHERE source.policy_key = 'dev-unranked-kr-individual-insolvency-2026-v4'
        )
        AND (
            SELECT COUNT(*)
            FROM property_capital_gains_tax_rate_bracket AS target_bracket
            INNER JOIN policy_set AS target ON target.id = target_bracket.policy_set_id
            WHERE target.policy_key = 'dev-unranked-kr-corporation-2026-v5'
        ) = (
            SELECT COUNT(*)
            FROM property_capital_gains_tax_rate_bracket AS source_bracket
            INNER JOIN policy_set AS source ON source.id = source_bracket.policy_set_id
            WHERE source.policy_key = 'dev-unranked-kr-individual-insolvency-2026-v4'
        ),
    1,
    0
);

DROP TEMPORARY TABLE m4e2_property_policy_guard;
