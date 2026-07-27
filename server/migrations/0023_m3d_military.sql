-- M3-D military service, military savings, and generalized employment-income events
-- (m3-career.md §9-§13).

SET SESSION group_concat_max_len = 1048576;
SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;

-- 0018 and 0022 published these development graphs before their M3-D typed children
-- existed. Refuse the one-time augmentation unless every relevant immutable input is the
-- reviewed graph. A production or ranked graph can never satisfy these exact-key guards.
CREATE TEMPORARY TABLE m3d_staged_guard (
    guard_key      VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    accepted       TINYINT UNSIGNED NOT NULL,
    PRIMARY KEY (guard_key),
    CONSTRAINT ck_m3d_staged_guard CHECK (accepted = 1)
);

INSERT INTO m3d_staged_guard (guard_key, accepted)
SELECT
    'career-bundle',
    IF(
        bundle.ranked_eligible = FALSE
            AND bundle.published_at IS NOT NULL
            AND SHA2(CONCAT_WS(CHAR(31),
                bundle.bundle_key,
                bundle.ranked_eligible,
                bundle.published_at IS NOT NULL,
                bundle.default_focused_job_family_key,
                (
                    SELECT GROUP_CONCAT(
                        CONCAT_WS(CHAR(30),
                            option_row.option_key,
                            option_row.service_type,
                            option_row.display_name,
                            option_row.effort_life_status,
                            option_row.compensation_kind,
                            option_row.pay_schedule,
                            COALESCE(option_row.minimum_education, '#'),
                            COALESCE(entry.entry_key, '#'),
                            option_row.grants_career_experience
                        )
                        ORDER BY option_row.option_key SEPARATOR '|'
                    )
                    FROM military_option_version AS option_row
                    LEFT JOIN spec_catalog_entry AS entry
                        ON entry.career_catalog_bundle_id
                            = option_row.career_catalog_bundle_id
                       AND entry.id = option_row.required_certification_entry_id
                    WHERE option_row.career_catalog_bundle_id = bundle.id
                ),
                (
                    SELECT GROUP_CONCAT(
                        CONCAT_WS(CHAR(30),
                            option_row.option_key,
                            family.job_family_key,
                            mapping.experience_credit_ppm
                        )
                        ORDER BY option_row.option_key, family.job_family_key SEPARATOR '|'
                    )
                    FROM military_option_job_family AS mapping
                    INNER JOIN military_option_version AS option_row
                        ON option_row.career_catalog_bundle_id
                            = mapping.career_catalog_bundle_id
                       AND option_row.id = mapping.military_option_version_id
                    INNER JOIN career_job_family AS family
                        ON family.career_catalog_bundle_id
                            = mapping.career_catalog_bundle_id
                       AND family.id = mapping.career_job_family_id
                    WHERE mapping.career_catalog_bundle_id = bundle.id
                ),
                (
                    SELECT GROUP_CONCAT(
                        institution.institution_key
                        ORDER BY institution.institution_key SEPARATOR '|'
                    )
                    FROM military_savings_institution_catalog AS catalog
                    INNER JOIN financial_institution AS institution
                        ON institution.id = catalog.financial_institution_id
                    WHERE catalog.career_catalog_bundle_id = bundle.id
                )
            ), 256) = '3c169063388067d2fa1cd6d5f5f33104df3ac53624edae873d8f2f4232294e89',
        1,
        0
    )
FROM career_catalog_bundle AS bundle
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO m3d_staged_guard (guard_key, accepted)
SELECT
    'employment-policy',
    IF(
        policy.ranked_eligible = FALSE
            AND policy.published_at IS NOT NULL
            AND SHA2(CONCAT_WS(CHAR(31),
                policy.policy_key,
                policy.ranked_eligible,
                DATE_FORMAT(policy.coverage_start, '%Y-%m-%d'),
                DATE_FORMAT(policy.coverage_end_exclusive, '%Y-%m-%d'),
                policy.published_at IS NOT NULL,
                (
                    SELECT GROUP_CONCAT(
                        CONCAT_WS(CHAR(30),
                            source.source_key,
                            source.title,
                            source.source_url,
                            DATE_FORMAT(source.verified_on, '%Y-%m-%d'),
                            source.content_sha256
                        )
                        ORDER BY source.source_key SEPARATOR '|'
                    )
                    FROM employment_policy_source AS source
                    WHERE source.employment_policy_set_id = policy.id
                ),
                (
                    SELECT GROUP_CONCAT(
                        CONCAT_WS(CHAR(30),
                            DATE_FORMAT(item.effective_from, '%Y-%m-%d'),
                            COALESCE(
                                DATE_FORMAT(item.effective_to_exclusive, '%Y-%m-%d'),
                                '#'
                            ),
                            item.monthly_income_rounding_unit_krw,
                            item.minimum_monthly_income_krw,
                            item.maximum_monthly_income_krw,
                            item.employee_rate_ppm,
                            item.employer_rate_ppm,
                            item.employee_rounding_unit_krw,
                            item.employer_rounding_unit_krw
                        )
                        ORDER BY item.effective_from SEPARATOR '|'
                    )
                    FROM national_pension_policy AS item
                    WHERE item.employment_policy_set_id = policy.id
                ),
                (
                    SELECT GROUP_CONCAT(
                        CONCAT_WS(CHAR(30),
                            item.tax_year,
                            item.basic_personal_deduction_krw,
                            item.taxable_income_rounding_unit_krw,
                            item.calculated_tax_rounding_unit_krw,
                            item.income_tax_credit_low_tax_boundary_krw,
                            item.income_tax_credit_low_rate_ppm,
                            item.income_tax_credit_high_base_krw,
                            item.income_tax_credit_high_rate_ppm,
                            item.credit_cap_salary_boundary_one_krw,
                            item.credit_cap_salary_boundary_two_krw,
                            item.credit_cap_one_krw,
                            item.credit_cap_two_base_krw,
                            item.credit_cap_two_reduction_rate_ppm,
                            item.credit_cap_two_floor_krw,
                            item.credit_cap_three_base_krw,
                            item.credit_cap_three_reduction_rate_ppm,
                            item.credit_cap_three_floor_krw
                        )
                        ORDER BY item.tax_year SEPARATOR '|'
                    )
                    FROM employment_annual_tax_policy AS item
                    WHERE item.employment_policy_set_id = policy.id
                )
            ), 256) = 'd420e70d0cf8c0a3485e6883bfd9325ffbc5518b7e8f72792bed40c704132d6a',
        1,
        0
    )
FROM employment_policy_set AS policy
WHERE BINARY policy.policy_key
    = BINARY 'dev-unranked-m3-employment-2026-v1';

DROP TEMPORARY TABLE m3d_staged_guard;

-- The source insertion trigger intentionally permits draft sets only. Remove it for the
-- checksum-guarded development augmentation, then restore the permanent rule immediately.
DROP TRIGGER tr_employment_policy_source_draft_insert;

INSERT INTO employment_policy_source
    (
        employment_policy_set_id,
        source_key,
        title,
        source_url,
        verified_on,
        content_sha256
    )
SELECT policy.id, source.source_key, source.title, source.source_url,
       '2026-07-27', source.content_sha256
FROM employment_policy_set AS policy
INNER JOIN (
    SELECT
        'mma-active-service-duration' AS source_key,
        '병무청 현역병 복무기간' AS title,
        'https://www.mma.go.kr/minwon/contents.do?mc=mma0000728' AS source_url,
        '2143e287983f030f4bcaac9050c92b0af8a53f8e96bbec31be854b0ab3e5494a'
            AS content_sha256
    UNION ALL SELECT
        'mma-social-service-duration', '병무청 사회복무요원 소집제도',
        'https://www.mma.go.kr/contents.do?mc=mma0000744',
        '134555a038eefc2fe2970a3d939f0408a67f2bdc4e30a5f985142bafe41d4333'
    UNION ALL SELECT
        'mma-special-service-duration', '병무청 산업기능·전문연구요원 복무기간',
        'https://www.mma.go.kr/minwon/contents.do?mc=mma0000760',
        'cfa8f66ea4ce6ebbd7a73d7dcc883510f54bf0643bb776a3042c2824008b974d'
    UNION ALL SELECT
        'mma-special-service-eligibility', '병무청 산업기능·전문연구요원 편입요건',
        'https://www.mma.go.kr/seoul/contents.do?mc=mma0000764',
        '9b16d22e2138b6b485f643ba3b5e56c23c10901b4e01439ce02e869204ff4702'
    UNION ALL SELECT
        'military-pay-2026', '2026 병 봉급 공무원보수규정 별표 13',
        'https://www.law.go.kr/flDownload.do?flSeq=160436483',
        'b04cf0ca475728dd05840bc7bb91f1918ca3714af673b71e5805041f8d920cda'
    UNION ALL SELECT
        'mpm-military-pay-2026', '인사혁신처 2026 군인 봉급표',
        'https://www.mpm.go.kr/mpm/info/resultPay/bizSalary/2026/',
        '82da403bcf39da6edf353a203359d1a09099400662a076f64c82ab71d9480dd0'
    UNION ALL SELECT
        'moel-minimum-wage-2026', '고용노동부 2026년 최저임금 고시',
        'https://www.moel.go.kr/news/enews/report/enewsView.do?news_seq=18144',
        'd23657d0b21eb57eef10bd92411dcd23966be6fb90da695e38e2b1a97e8f2ae3'
    UNION ALL SELECT
        'military-savings-enforcement-decree', '병역법 시행령 제158조의2',
        'https://www.law.go.kr/LSW/lsLinkCommonInfo.do?chrClsCd=010202&lspttninfSeq=171035',
        'd4e165da8558116f6b42443e6a080d2890620de0dce6af31c1d2e1cca0eeaf87'
    UNION ALL SELECT
        'kb-military-savings-terms-2026', '2026 장병내일준비적금 특약',
        'https://img2.kbstar.com/obj/ocommon/260213military_full.pdf',
        '8dec889f1b33559be873b3328ce9f5dba3649b23e7aa6098c18e7e0e785f9d51'
) AS source
WHERE BINARY policy.policy_key
    = BINARY 'dev-unranked-m3-employment-2026-v1';

CREATE TRIGGER tr_employment_policy_source_draft_insert
BEFORE INSERT ON employment_policy_source
FOR EACH ROW
SET NEW.employment_policy_set_id = IF(
    EXISTS (
        SELECT 1 FROM employment_policy_set
        WHERE id = NEW.employment_policy_set_id AND published_at IS NULL
    ),
    NEW.employment_policy_set_id,
    NULL
);

-- The independent February anchor belongs to the annual policy. The existing policy row is
-- changed only inside the exact guarded exception above; future rows must supply the value.
DROP TRIGGER tr_employment_annual_tax_policy_no_update;

ALTER TABLE employment_annual_tax_policy
    ADD COLUMN february_reconciliation_day_of_month TINYINT UNSIGNED NULL
        AFTER tax_year;

UPDATE employment_annual_tax_policy AS annual_policy
INNER JOIN employment_policy_set AS policy
    ON policy.id = annual_policy.employment_policy_set_id
SET annual_policy.february_reconciliation_day_of_month = 28
WHERE BINARY policy.policy_key
        = BINARY 'dev-unranked-m3-employment-2026-v1'
  AND policy.ranked_eligible = FALSE
  AND annual_policy.tax_year = 2026
  AND annual_policy.february_reconciliation_day_of_month IS NULL;

ALTER TABLE employment_annual_tax_policy
    MODIFY COLUMN february_reconciliation_day_of_month TINYINT UNSIGNED NOT NULL,
    ADD CONSTRAINT ck_employment_annual_tax_reconciliation_day CHECK (
        february_reconciliation_day_of_month BETWEEN 1 AND 31
    );

CREATE TRIGGER tr_employment_annual_tax_policy_no_update
BEFORE UPDATE ON employment_annual_tax_policy
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment annual tax policies are immutable';

-- The eligibility child is authoritative for M3-D. The similarly named columns on the 0018
-- option row are retained only as a deprecated compatibility projection.
CREATE TABLE military_option_eligibility_rule (
    career_catalog_bundle_id        BIGINT UNSIGNED NOT NULL,
    military_option_version_id      BIGINT UNSIGNED NOT NULL,
    minimum_education               VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL,
    required_certification_count    SMALLINT UNSIGNED NOT NULL,
    minimum_experience_days         INT UNSIGNED NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (career_catalog_bundle_id, military_option_version_id),
    CONSTRAINT fk_military_eligibility_option
        FOREIGN KEY (career_catalog_bundle_id, military_option_version_id)
        REFERENCES military_option_version (career_catalog_bundle_id, id),
    CONSTRAINT ck_military_eligibility_education CHECK (
        minimum_education IS NULL
        OR minimum_education IN ('highSchool', 'associate', 'bachelor', 'master', 'doctorate')
    ),
    CONSTRAINT ck_military_eligibility_bounds CHECK (
        required_certification_count <= 1000
        AND minimum_experience_days <= 365000
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE military_option_policy (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    employment_policy_set_id            BIGINT UNSIGNED NOT NULL,
    career_catalog_bundle_id            BIGINT UNSIGNED NOT NULL,
    military_option_version_id          BIGINT UNSIGNED NOT NULL,
    duration_policy_source_id           BIGINT UNSIGNED NULL,
    compensation_policy_source_id       BIGINT UNSIGNED NULL,
    effective_from                      DATE NOT NULL,
    effective_to_exclusive              DATE NULL,
    service_type                        VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    service_duration_months             SMALLINT UNSIGNED NOT NULL,
    pay_schedule_kind                   VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    payday_day_of_month                 TINYINT UNSIGNED NOT NULL,
    partial_month_pay_kind              VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    compensation_calculation_kind       VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    income_classification               VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    social_insurance_kind               VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    reimbursement_model_kind            VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    promotion_model_kind                VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    availability_status                 VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    data_status                         VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_military_option_policy_effective
        (employment_policy_set_id, career_catalog_bundle_id,
         military_option_version_id, effective_from),
    UNIQUE KEY uk_military_option_policy_set_bundle_id
        (employment_policy_set_id, career_catalog_bundle_id, id),
    UNIQUE KEY uk_military_option_policy_option_id
        (employment_policy_set_id, career_catalog_bundle_id, id,
         military_option_version_id),
    CONSTRAINT fk_military_option_policy_set
        FOREIGN KEY (employment_policy_set_id) REFERENCES employment_policy_set (id),
    CONSTRAINT fk_military_option_policy_option
        FOREIGN KEY (career_catalog_bundle_id, military_option_version_id)
        REFERENCES military_option_version (career_catalog_bundle_id, id),
    CONSTRAINT fk_military_option_policy_duration_source
        FOREIGN KEY (employment_policy_set_id, duration_policy_source_id)
        REFERENCES employment_policy_source (employment_policy_set_id, id),
    CONSTRAINT fk_military_option_policy_comp_source
        FOREIGN KEY (employment_policy_set_id, compensation_policy_source_id)
        REFERENCES employment_policy_source (employment_policy_set_id, id),
    CONSTRAINT ck_military_option_policy_period CHECK (
        effective_to_exclusive IS NULL OR effective_from < effective_to_exclusive
    ),
    CONSTRAINT ck_military_option_policy_service CHECK (
        service_type IN (
            'activeDuty', 'socialService', 'industrialTechnical',
            'professionalResearch', 'commissionedOfficer', 'nonCommissionedOfficer'
        )
        AND service_duration_months BETWEEN 1 AND 600
    ),
    CONSTRAINT ck_military_option_policy_pay_schedule CHECK (
        pay_schedule_kind = 'monthly'
        AND payday_day_of_month BETWEEN 1 AND 31
        AND partial_month_pay_kind IN ('fullMonthlyGross', 'verifiedPolicy')
    ),
    CONSTRAINT ck_military_option_policy_compensation CHECK (
        compensation_calculation_kind IN (
            'militaryStage', 'employmentPayrollMinimum', 'basePayOnly'
        )
        AND income_classification = 'employmentIncome'
        AND social_insurance_kind IN ('notAssessed', 'employmentPayroll')
    ),
    CONSTRAINT ck_military_option_policy_models CHECK (
        reimbursement_model_kind IN ('none', 'reimbursementNotModeled')
        AND promotion_model_kind IN ('notApplicable', 'ordinaryMinimumPromotion')
        AND availability_status IN ('available', 'policyUnavailable')
        AND data_status IN ('reviewedOfficial', 'devAssumption')
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE military_pay_stage (
    employment_policy_set_id        BIGINT UNSIGNED NOT NULL,
    career_catalog_bundle_id        BIGINT UNSIGNED NOT NULL,
    military_option_policy_id       BIGINT UNSIGNED NOT NULL,
    stage_order                     SMALLINT UNSIGNED NOT NULL,
    start_service_month             SMALLINT UNSIGNED NOT NULL,
    end_service_month_exclusive     SMALLINT UNSIGNED NOT NULL,
    monthly_gross_pay_krw           BIGINT NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (military_option_policy_id, stage_order),
    UNIQUE KEY uk_military_pay_stage_start
        (military_option_policy_id, start_service_month),
    CONSTRAINT fk_military_pay_stage_policy
        FOREIGN KEY (
            employment_policy_set_id,
            career_catalog_bundle_id,
            military_option_policy_id
        ) REFERENCES military_option_policy (
            employment_policy_set_id,
            career_catalog_bundle_id,
            id
        ),
    CONSTRAINT ck_military_pay_stage_bounds CHECK (
        stage_order > 0
        AND start_service_month < end_service_month_exclusive
        AND end_service_month_exclusive <= 600
        AND monthly_gross_pay_krw BETWEEN 0 AND 9007199254740991
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE military_savings_policy (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    employment_policy_set_id            BIGINT UNSIGNED NOT NULL,
    employment_policy_source_id         BIGINT UNSIGNED NOT NULL,
    effective_from                      DATE NOT NULL,
    effective_to_exclusive              DATE NULL,
    join_through                        DATE NOT NULL,
    minimum_remaining_service_months    SMALLINT UNSIGNED NOT NULL,
    max_contracts_per_service           TINYINT UNSIGNED NOT NULL,
    max_contracts_per_institution       TINYINT UNSIGNED NOT NULL,
    institution_monthly_limit_krw       BIGINT NOT NULL,
    person_monthly_limit_krw            BIGINT NOT NULL,
    limit_setting_unit_krw              BIGINT NOT NULL,
    minimum_installment_krw             BIGINT NOT NULL,
    installment_unit_krw                BIGINT NOT NULL,
    government_match_rate_ppm           INT UNSIGNED NOT NULL,
    government_match_next_month_day     TINYINT UNSIGNED NOT NULL,
    tax_exempt                          BOOLEAN NOT NULL,
    data_status                         VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_military_savings_policy_effective
        (employment_policy_set_id, effective_from),
    UNIQUE KEY uk_military_savings_policy_set_id
        (employment_policy_set_id, id),
    CONSTRAINT fk_military_savings_policy_set
        FOREIGN KEY (employment_policy_set_id) REFERENCES employment_policy_set (id),
    CONSTRAINT fk_military_savings_policy_source
        FOREIGN KEY (employment_policy_set_id, employment_policy_source_id)
        REFERENCES employment_policy_source (employment_policy_set_id, id),
    CONSTRAINT ck_military_savings_policy_period CHECK (
        effective_to_exclusive IS NULL OR effective_from < effective_to_exclusive
    ),
    CONSTRAINT ck_military_savings_policy_limits CHECK (
        minimum_remaining_service_months BETWEEN 1 AND 600
        AND max_contracts_per_service BETWEEN 1 AND 10
        AND max_contracts_per_institution BETWEEN 1 AND max_contracts_per_service
        AND institution_monthly_limit_krw > 0
        AND person_monthly_limit_krw >= institution_monthly_limit_krw
        AND person_monthly_limit_krw <= 9007199254740991
        AND limit_setting_unit_krw > 0
        AND minimum_installment_krw > 0
        AND installment_unit_krw > 0
        AND government_match_rate_ppm BETWEEN 1 AND 1000000
        AND government_match_next_month_day BETWEEN 1 AND 31
        AND tax_exempt IN (FALSE, TRUE)
        AND data_status IN ('reviewedOfficial', 'devAssumption')
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE military_savings_policy_eligible_service (
    employment_policy_set_id        BIGINT UNSIGNED NOT NULL,
    military_savings_policy_id      BIGINT UNSIGNED NOT NULL,
    service_type                    VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (military_savings_policy_id, service_type),
    CONSTRAINT fk_military_savings_eligible_policy
        FOREIGN KEY (employment_policy_set_id, military_savings_policy_id)
        REFERENCES military_savings_policy (employment_policy_set_id, id),
    CONSTRAINT ck_military_savings_eligible_service CHECK (
        service_type IN ('activeDuty', 'socialService')
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE military_savings_product_version (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    career_catalog_bundle_id            BIGINT UNSIGNED NOT NULL,
    military_savings_institution_id     BIGINT UNSIGNED NOT NULL,
    product_key                         VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                        VARCHAR(120) NOT NULL,
    ranked_eligible                     BOOLEAN NOT NULL DEFAULT FALSE,
    available_from                      DATE NOT NULL,
    available_to_exclusive              DATE NULL,
    minimum_term_months                 SMALLINT UNSIGNED NOT NULL,
    maximum_term_months                 SMALLINT UNSIGNED NOT NULL,
    day_count_denominator               SMALLINT UNSIGNED NOT NULL,
    interest_rounding_kind              VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    interest_rounding_unit_krw           BIGINT NOT NULL,
    early_termination_rate_bp           SMALLINT UNSIGNED NOT NULL,
    tax_treatment                       VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    provenance_kind                     VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    terms_source_url                    VARCHAR(1024) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    terms_verified_on                   DATE NOT NULL,
    terms_content_sha256                CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_military_savings_product_key
        (career_catalog_bundle_id, product_key),
    UNIQUE KEY uk_military_savings_product_institution
        (career_catalog_bundle_id, military_savings_institution_id),
    UNIQUE KEY uk_military_savings_product_bundle_id
        (career_catalog_bundle_id, id),
    CONSTRAINT fk_military_savings_product_institution
        FOREIGN KEY (career_catalog_bundle_id, military_savings_institution_id)
        REFERENCES military_savings_institution_catalog (career_catalog_bundle_id, id),
    CONSTRAINT ck_military_savings_product_key CHECK (
        CHAR_LENGTH(product_key) > 0
        AND product_key REGEXP '^[a-z0-9][a-z0-9._-]{0,95}$'
    ),
    CONSTRAINT ck_military_savings_product_period CHECK (
        available_to_exclusive IS NULL OR available_from < available_to_exclusive
    ),
    CONSTRAINT ck_military_savings_product_terms CHECK (
        ranked_eligible IN (FALSE, TRUE)
        AND minimum_term_months >= 1
        AND maximum_term_months >= minimum_term_months
        AND maximum_term_months <= 600
        AND day_count_denominator > 0
        AND interest_rounding_kind = 'floor'
        AND interest_rounding_unit_krw > 0
        AND early_termination_rate_bp <= 10000
        AND tax_treatment = 'taxExemptAtMaturity'
        AND provenance_kind IN ('reviewedOfficial', 'devAssumption')
        AND terms_source_url REGEXP '^https://[^[:space:]]+$'
        AND terms_content_sha256 REGEXP '^[0-9a-f]{64}$'
        AND (ranked_eligible = FALSE OR provenance_kind = 'reviewedOfficial')
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE military_savings_product_rate_band (
    career_catalog_bundle_id        BIGINT UNSIGNED NOT NULL,
    military_savings_product_id     BIGINT UNSIGNED NOT NULL,
    band_order                      SMALLINT UNSIGNED NOT NULL,
    minimum_term_months             SMALLINT UNSIGNED NOT NULL,
    maximum_term_months_exclusive   SMALLINT UNSIGNED NOT NULL,
    fixed_rate_bp                   SMALLINT UNSIGNED NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (military_savings_product_id, band_order),
    UNIQUE KEY uk_military_savings_rate_start
        (military_savings_product_id, minimum_term_months),
    CONSTRAINT fk_military_savings_rate_product
        FOREIGN KEY (career_catalog_bundle_id, military_savings_product_id)
        REFERENCES military_savings_product_version (career_catalog_bundle_id, id),
    CONSTRAINT ck_military_savings_rate_band CHECK (
        band_order > 0
        AND minimum_term_months < maximum_term_months_exclusive
        AND maximum_term_months_exclusive <= 601
        AND fixed_rate_bp <= 10000
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

INSERT INTO military_option_eligibility_rule
    (
        career_catalog_bundle_id,
        military_option_version_id,
        minimum_education,
        required_certification_count,
        minimum_experience_days
    )
SELECT bundle.id, option_row.id, seed.minimum_education,
       seed.required_certification_count, seed.minimum_experience_days
FROM career_catalog_bundle AS bundle
INNER JOIN military_option_version AS option_row
    ON option_row.career_catalog_bundle_id = bundle.id
INNER JOIN (
    SELECT 'activeDuty' AS service_type, NULL AS minimum_education,
           0 AS required_certification_count, 0 AS minimum_experience_days
    UNION ALL SELECT 'socialService', NULL, 0, 0
    UNION ALL SELECT 'industrialTechnical', NULL, 1, 0
    UNION ALL SELECT 'professionalResearch', 'master', 0, 0
    UNION ALL SELECT 'commissionedOfficer', 'bachelor', 0, 0
    UNION ALL SELECT 'nonCommissionedOfficer', 'highSchool', 0, 0
) AS seed
    ON BINARY seed.service_type = BINARY option_row.service_type
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1'
  AND bundle.ranked_eligible = FALSE;

INSERT INTO military_option_policy
    (
        employment_policy_set_id,
        career_catalog_bundle_id,
        military_option_version_id,
        duration_policy_source_id,
        compensation_policy_source_id,
        effective_from,
        effective_to_exclusive,
        service_type,
        service_duration_months,
        pay_schedule_kind,
        payday_day_of_month,
        partial_month_pay_kind,
        compensation_calculation_kind,
        income_classification,
        social_insurance_kind,
        reimbursement_model_kind,
        promotion_model_kind,
        availability_status,
        data_status
    )
SELECT policy.id, bundle.id, option_row.id,
       duration_source.id, compensation_source.id,
       '2026-01-01', NULL, seed.service_type, seed.service_duration_months,
       'monthly', 10, 'fullMonthlyGross',
       seed.compensation_calculation_kind, 'employmentIncome',
       seed.social_insurance_kind, seed.reimbursement_model_kind,
       seed.promotion_model_kind, 'available', 'devAssumption'
FROM employment_policy_set AS policy
INNER JOIN career_catalog_bundle AS bundle
    ON BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1'
   AND bundle.ranked_eligible = FALSE
INNER JOIN military_option_version AS option_row
    ON option_row.career_catalog_bundle_id = bundle.id
INNER JOIN (
    SELECT 'activeDuty' AS service_type, 18 AS service_duration_months,
           'militaryStage' AS compensation_calculation_kind,
           'notAssessed' AS social_insurance_kind,
           'none' AS reimbursement_model_kind,
           'ordinaryMinimumPromotion' AS promotion_model_kind,
           'mma-active-service-duration' AS duration_source_key,
           'military-pay-2026' AS compensation_source_key
    UNION ALL SELECT 'socialService', 21, 'militaryStage', 'notAssessed',
        'reimbursementNotModeled', 'notApplicable',
        'mma-social-service-duration', 'military-pay-2026'
    UNION ALL SELECT 'industrialTechnical', 34, 'employmentPayrollMinimum',
        'employmentPayroll', 'none', 'notApplicable',
        'mma-special-service-duration', 'moel-minimum-wage-2026'
    UNION ALL SELECT 'professionalResearch', 36, 'employmentPayrollMinimum',
        'employmentPayroll', 'none', 'notApplicable',
        'mma-special-service-duration', 'moel-minimum-wage-2026'
    UNION ALL SELECT 'commissionedOfficer', 36, 'basePayOnly',
        'employmentPayroll', 'none', 'notApplicable',
        NULL, 'mpm-military-pay-2026'
    UNION ALL SELECT 'nonCommissionedOfficer', 48, 'basePayOnly',
        'employmentPayroll', 'none', 'notApplicable',
        NULL, 'mpm-military-pay-2026'
) AS seed
    ON BINARY seed.service_type = BINARY option_row.service_type
LEFT JOIN employment_policy_source AS duration_source
    ON duration_source.employment_policy_set_id = policy.id
   AND BINARY duration_source.source_key = BINARY seed.duration_source_key
LEFT JOIN employment_policy_source AS compensation_source
    ON compensation_source.employment_policy_set_id = policy.id
   AND BINARY compensation_source.source_key = BINARY seed.compensation_source_key
WHERE BINARY policy.policy_key
    = BINARY 'dev-unranked-m3-employment-2026-v1'
  AND policy.ranked_eligible = FALSE;

INSERT INTO military_pay_stage
    (
        employment_policy_set_id,
        career_catalog_bundle_id,
        military_option_policy_id,
        stage_order,
        start_service_month,
        end_service_month_exclusive,
        monthly_gross_pay_krw
    )
SELECT option_policy.employment_policy_set_id,
       option_policy.career_catalog_bundle_id,
       option_policy.id, seed.stage_order, seed.start_month, seed.end_month,
       seed.monthly_gross_pay_krw
FROM military_option_policy AS option_policy
INNER JOIN employment_policy_set AS policy
    ON policy.id = option_policy.employment_policy_set_id
INNER JOIN (
    SELECT 'activeDuty' AS service_type, 1 AS stage_order,
           0 AS start_month, 2 AS end_month, 750000 AS monthly_gross_pay_krw
    UNION ALL SELECT 'activeDuty', 2, 2, 8, 900000
    UNION ALL SELECT 'activeDuty', 3, 8, 14, 1200000
    UNION ALL SELECT 'activeDuty', 4, 14, 18, 1500000
    UNION ALL SELECT 'socialService', 1, 0, 3, 750000
    UNION ALL SELECT 'socialService', 2, 3, 9, 900000
    UNION ALL SELECT 'socialService', 3, 9, 15, 1200000
    UNION ALL SELECT 'socialService', 4, 15, 21, 1500000
    UNION ALL SELECT 'industrialTechnical', 1, 0, 34, 2156880
    UNION ALL SELECT 'professionalResearch', 1, 0, 36, 2156880
    UNION ALL SELECT 'commissionedOfficer', 1, 0, 36, 2150400
    UNION ALL SELECT 'nonCommissionedOfficer', 1, 0, 48, 2133000
) AS seed
    ON BINARY seed.service_type = BINARY option_policy.service_type
WHERE BINARY policy.policy_key
    = BINARY 'dev-unranked-m3-employment-2026-v1';

INSERT INTO military_savings_policy
    (
        employment_policy_set_id,
        employment_policy_source_id,
        effective_from,
        effective_to_exclusive,
        join_through,
        minimum_remaining_service_months,
        max_contracts_per_service,
        max_contracts_per_institution,
        institution_monthly_limit_krw,
        person_monthly_limit_krw,
        limit_setting_unit_krw,
        minimum_installment_krw,
        installment_unit_krw,
        government_match_rate_ppm,
        government_match_next_month_day,
        tax_exempt,
        data_status
    )
SELECT policy.id, source.id, '2026-01-01', NULL, '2026-12-31',
       1, 2, 1, 300000, 550000, 50000, 1000, 1, 1000000, 25, TRUE,
       'devAssumption'
FROM employment_policy_set AS policy
INNER JOIN employment_policy_source AS source
    ON source.employment_policy_set_id = policy.id
   AND BINARY source.source_key = BINARY 'military-savings-enforcement-decree'
WHERE BINARY policy.policy_key
    = BINARY 'dev-unranked-m3-employment-2026-v1'
  AND policy.ranked_eligible = FALSE;

INSERT INTO military_savings_policy_eligible_service
    (employment_policy_set_id, military_savings_policy_id, service_type)
SELECT policy.employment_policy_set_id, policy.id, service.service_type
FROM military_savings_policy AS policy
CROSS JOIN (
    SELECT 'activeDuty' AS service_type
    UNION ALL SELECT 'socialService'
) AS service;

INSERT INTO military_savings_product_version
    (
        career_catalog_bundle_id,
        military_savings_institution_id,
        product_key,
        display_name,
        ranked_eligible,
        available_from,
        available_to_exclusive,
        minimum_term_months,
        maximum_term_months,
        day_count_denominator,
        interest_rounding_kind,
        interest_rounding_unit_krw,
        early_termination_rate_bp,
        tax_treatment,
        provenance_kind,
        terms_source_url,
        terms_verified_on,
        terms_content_sha256
    )
SELECT bundle.id, catalog.id,
       CONCAT('dev-military-savings-', institution.institution_key, '-2026-v1'),
       CONCAT('라이프 장병내일준비적금 ', institution.display_name),
       FALSE, '2026-01-01', '2027-01-01', 1, 24, 365, 'floor', 1, 0,
       'taxExemptAtMaturity', 'devAssumption',
       'https://img2.kbstar.com/obj/ocommon/260213military_full.pdf',
       '2026-07-27',
       '8dec889f1b33559be873b3328ce9f5dba3649b23e7aa6098c18e7e0e785f9d51'
FROM career_catalog_bundle AS bundle
INNER JOIN military_savings_institution_catalog AS catalog
    ON catalog.career_catalog_bundle_id = bundle.id
INNER JOIN financial_institution AS institution
    ON institution.id = catalog.financial_institution_id
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1'
  AND bundle.ranked_eligible = FALSE;

INSERT INTO military_savings_product_rate_band
    (
        career_catalog_bundle_id,
        military_savings_product_id,
        band_order,
        minimum_term_months,
        maximum_term_months_exclusive,
        fixed_rate_bp
    )
SELECT product.career_catalog_bundle_id, product.id, rate.band_order,
       rate.minimum_term_months, rate.maximum_term_months_exclusive,
       rate.fixed_rate_bp
FROM military_savings_product_version AS product
CROSS JOIN (
    SELECT 1 AS band_order, 1 AS minimum_term_months,
           12 AS maximum_term_months_exclusive, 400 AS fixed_rate_bp
    UNION ALL SELECT 2, 12, 15, 450
    UNION ALL SELECT 3, 15, 25, 500
) AS rate;

CREATE TRIGGER tr_military_eligibility_draft_insert
BEFORE INSERT ON military_option_eligibility_rule
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle
        WHERE id = NEW.career_catalog_bundle_id AND published_at IS NULL
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_military_eligibility_no_update
BEFORE UPDATE ON military_option_eligibility_rule
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military eligibility rules are immutable';

CREATE TRIGGER tr_military_eligibility_no_delete
BEFORE DELETE ON military_option_eligibility_rule
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military eligibility rules are immutable';

CREATE TRIGGER tr_military_option_policy_draft_insert
BEFORE INSERT ON military_option_policy
FOR EACH ROW
SET NEW.employment_policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM employment_policy_set AS policy
        INNER JOIN military_option_version AS option_row
            ON option_row.career_catalog_bundle_id = NEW.career_catalog_bundle_id
           AND option_row.id = NEW.military_option_version_id
           AND BINARY option_row.service_type = BINARY NEW.service_type
        WHERE policy.id = NEW.employment_policy_set_id
          AND policy.published_at IS NULL
          AND NEW.effective_from >= policy.coverage_start
          AND NEW.effective_from < policy.coverage_end_exclusive
          AND (
              NEW.effective_to_exclusive IS NULL
              OR NEW.effective_to_exclusive <= policy.coverage_end_exclusive
          )
          AND NOT EXISTS (
              SELECT 1 FROM military_option_policy AS existing
              WHERE existing.employment_policy_set_id = NEW.employment_policy_set_id
                AND existing.career_catalog_bundle_id = NEW.career_catalog_bundle_id
                AND existing.military_option_version_id
                    = NEW.military_option_version_id
                AND NOT (
                    COALESCE(existing.effective_to_exclusive, '9999-12-31')
                        <= NEW.effective_from
                    OR COALESCE(NEW.effective_to_exclusive, '9999-12-31')
                        <= existing.effective_from
                )
          )
    ),
    NEW.employment_policy_set_id,
    NULL
);

CREATE TRIGGER tr_military_option_policy_no_update
BEFORE UPDATE ON military_option_policy
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military option policies are immutable';

CREATE TRIGGER tr_military_option_policy_no_delete
BEFORE DELETE ON military_option_policy
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military option policies are immutable';

CREATE TRIGGER tr_military_pay_stage_draft_insert
BEFORE INSERT ON military_pay_stage
FOR EACH ROW
SET NEW.employment_policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM military_option_policy AS option_policy
        INNER JOIN employment_policy_set AS policy
            ON policy.id = option_policy.employment_policy_set_id
           AND policy.published_at IS NULL
        WHERE option_policy.id = NEW.military_option_policy_id
          AND option_policy.employment_policy_set_id = NEW.employment_policy_set_id
          AND option_policy.career_catalog_bundle_id = NEW.career_catalog_bundle_id
          AND NEW.end_service_month_exclusive
              <= option_policy.service_duration_months
          AND NOT EXISTS (
              SELECT 1 FROM military_pay_stage AS existing
              WHERE existing.military_option_policy_id
                    = NEW.military_option_policy_id
                AND NOT (
                    existing.end_service_month_exclusive <= NEW.start_service_month
                    OR NEW.end_service_month_exclusive <= existing.start_service_month
                )
          )
    ),
    NEW.employment_policy_set_id,
    NULL
);

CREATE TRIGGER tr_military_pay_stage_no_update
BEFORE UPDATE ON military_pay_stage
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military pay stages are immutable';

CREATE TRIGGER tr_military_pay_stage_no_delete
BEFORE DELETE ON military_pay_stage
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military pay stages are immutable';

CREATE TRIGGER tr_military_savings_policy_draft_insert
BEFORE INSERT ON military_savings_policy
FOR EACH ROW
SET NEW.employment_policy_set_id = IF(
    EXISTS (
        SELECT 1 FROM employment_policy_set AS policy
        WHERE policy.id = NEW.employment_policy_set_id
          AND policy.published_at IS NULL
          AND NEW.effective_from >= policy.coverage_start
          AND NEW.effective_from < policy.coverage_end_exclusive
          AND (
              NEW.effective_to_exclusive IS NULL
              OR NEW.effective_to_exclusive <= policy.coverage_end_exclusive
          )
          AND NOT EXISTS (
              SELECT 1 FROM military_savings_policy AS existing
              WHERE existing.employment_policy_set_id = NEW.employment_policy_set_id
                AND NOT (
                    COALESCE(existing.effective_to_exclusive, '9999-12-31')
                        <= NEW.effective_from
                    OR COALESCE(NEW.effective_to_exclusive, '9999-12-31')
                        <= existing.effective_from
                )
          )
    ),
    NEW.employment_policy_set_id,
    NULL
);

CREATE TRIGGER tr_military_savings_policy_no_update
BEFORE UPDATE ON military_savings_policy
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military savings policies are immutable';

CREATE TRIGGER tr_military_savings_policy_no_delete
BEFORE DELETE ON military_savings_policy
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military savings policies are immutable';

CREATE TRIGGER tr_military_savings_eligible_draft_insert
BEFORE INSERT ON military_savings_policy_eligible_service
FOR EACH ROW
SET NEW.employment_policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM military_savings_policy AS savings_policy
        INNER JOIN employment_policy_set AS policy
            ON policy.id = savings_policy.employment_policy_set_id
           AND policy.published_at IS NULL
        WHERE savings_policy.id = NEW.military_savings_policy_id
          AND savings_policy.employment_policy_set_id
              = NEW.employment_policy_set_id
    ),
    NEW.employment_policy_set_id,
    NULL
);

CREATE TRIGGER tr_military_savings_eligible_no_update
BEFORE UPDATE ON military_savings_policy_eligible_service
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military savings eligibility is immutable';

CREATE TRIGGER tr_military_savings_eligible_no_delete
BEFORE DELETE ON military_savings_policy_eligible_service
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military savings eligibility is immutable';

CREATE TRIGGER tr_military_savings_product_draft_insert
BEFORE INSERT ON military_savings_product_version
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle AS bundle
        WHERE bundle.id = NEW.career_catalog_bundle_id
          AND bundle.published_at IS NULL
          AND (bundle.ranked_eligible = FALSE OR NEW.ranked_eligible = TRUE)
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_military_savings_product_no_update
BEFORE UPDATE ON military_savings_product_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military savings products are immutable';

CREATE TRIGGER tr_military_savings_product_no_delete
BEFORE DELETE ON military_savings_product_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military savings products are immutable';

CREATE TRIGGER tr_military_savings_rate_draft_insert
BEFORE INSERT ON military_savings_product_rate_band
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1
        FROM military_savings_product_version AS product
        INNER JOIN career_catalog_bundle AS bundle
            ON bundle.id = product.career_catalog_bundle_id
           AND bundle.published_at IS NULL
        WHERE product.id = NEW.military_savings_product_id
          AND product.career_catalog_bundle_id = NEW.career_catalog_bundle_id
          AND NEW.minimum_term_months >= product.minimum_term_months
          AND NEW.maximum_term_months_exclusive
              <= product.maximum_term_months + 1
          AND NOT EXISTS (
              SELECT 1 FROM military_savings_product_rate_band AS existing
              WHERE existing.military_savings_product_id
                    = NEW.military_savings_product_id
                AND NOT (
                    existing.maximum_term_months_exclusive
                        <= NEW.minimum_term_months
                    OR NEW.maximum_term_months_exclusive
                        <= existing.minimum_term_months
                )
          )
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_military_savings_rate_no_update
BEFORE UPDATE ON military_savings_product_rate_band
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military savings rate bands are immutable';

CREATE TRIGGER tr_military_savings_rate_no_delete
BEFORE DELETE ON military_savings_product_rate_band
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military savings rate bands are immutable';

-- Qualification reads credited days rather than assuming every calendar day has full credit.
-- The military FK is added after military_service exists below.
ALTER TABLE spec_evidence
    ADD COLUMN credited_experience_days INT UNSIGNED NULL
        AFTER period_end_exclusive_date;

UPDATE spec_evidence
SET credited_experience_days = DATEDIFF(
    period_end_exclusive_date,
    period_start_date
)
WHERE kind = 'experience';

ALTER TABLE spec_evidence
    ADD CONSTRAINT ck_spec_evidence_credited_experience CHECK (
        (
            kind = 'experience'
            AND credited_experience_days IS NOT NULL
            AND period_start_date IS NOT NULL
            AND period_end_exclusive_date IS NOT NULL
            AND (
                (
                    source_kind = 'militaryService'
                    AND credited_experience_days
                        <= DATEDIFF(period_end_exclusive_date, period_start_date)
                )
                OR (
                    source_kind <> 'militaryService'
                    AND credited_experience_days
                        = DATEDIFF(period_end_exclusive_date, period_start_date)
                )
            )
        )
        OR (
            kind <> 'experience'
            AND credited_experience_days IS NULL
        )
    );

-- career_run becomes the durable military-state authority. Existing character values are read
-- exactly once; serving legacy characters receive a full future service below, with no
-- retroactive pay, experience, or savings installments.
DROP TRIGGER tr_career_run_valid_insert;
DROP TRIGGER tr_career_run_focus_only;

ALTER TABLE career_run
    ADD COLUMN military_status VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL
        AFTER birth_date;

UPDATE career_run
INNER JOIN `character`
    ON `character`.save_id = career_run.save_id
SET career_run.military_status = CASE BINARY `character`.military
    WHEN BINARY 'notServed' THEN 'unserved'
    WHEN BINARY 'serving' THEN 'serving'
    WHEN BINARY 'alternative' THEN 'serving'
    WHEN BINARY 'completed' THEN 'completed'
    WHEN BINARY 'exempted' THEN 'exempt'
    ELSE NULL
END
WHERE career_run.military_status IS NULL;

ALTER TABLE career_run
    MODIFY COLUMN military_status VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin
        NOT NULL DEFAULT 'unserved',
    ADD UNIQUE KEY uk_career_run_catalog_employment
        (save_id, run_revision, career_catalog_bundle_id, employment_policy_set_id),
    ADD CONSTRAINT ck_career_run_military_status CHECK (
        military_status IN ('unserved', 'serving', 'completed', 'exempt')
    );

CREATE TABLE military_service (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED NOT NULL,
    career_catalog_bundle_id            BIGINT UNSIGNED NOT NULL,
    employment_policy_set_id            BIGINT UNSIGNED NOT NULL,
    military_option_version_id          BIGINT UNSIGNED NOT NULL,
    military_option_policy_id           BIGINT UNSIGNED NOT NULL,
    service_type                        VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    status                              VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    source_kind                         VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    start_command_id                    CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    start_game_day                      INT UNSIGNED NOT NULL,
    end_game_day                        INT UNSIGNED NOT NULL,
    start_date                          DATE NOT NULL,
    end_exclusive_date                  DATE NOT NULL,
    credited_service_days               INT UNSIGNED NOT NULL DEFAULT 0,
    last_credited_game_day              INT UNSIGNED NULL,
    completed_game_day                  INT UNSIGNED NULL,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_military_service_save_run (save_id, run_revision),
    UNIQUE KEY uk_military_service_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_military_service_run_bundle_id
        (save_id, run_revision, career_catalog_bundle_id, id),
    KEY ix_military_service_status
        (save_id, run_revision, status, start_game_day, end_game_day, id),
    CONSTRAINT fk_military_service_career_run
        FOREIGN KEY (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            employment_policy_set_id
        ) REFERENCES career_run (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            employment_policy_set_id
        ) ON DELETE CASCADE,
    CONSTRAINT fk_military_service_option_policy
        FOREIGN KEY (
            employment_policy_set_id,
            career_catalog_bundle_id,
            military_option_policy_id,
            military_option_version_id
        ) REFERENCES military_option_policy (
            employment_policy_set_id,
            career_catalog_bundle_id,
            id,
            military_option_version_id
        ),
    CONSTRAINT fk_military_service_command
        FOREIGN KEY (save_id, start_command_id)
        REFERENCES command_identity (save_id, command_id),
    CONSTRAINT ck_military_service_type CHECK (
        service_type IN (
            'activeDuty', 'socialService', 'industrialTechnical',
            'professionalResearch', 'commissionedOfficer', 'nonCommissionedOfficer'
        )
    ),
    CONSTRAINT ck_military_service_status CHECK (
        status IN ('pendingStart', 'serving', 'completed')
    ),
    CONSTRAINT ck_military_service_source CHECK (
        (source_kind = 'userCommand' AND start_command_id IS NOT NULL)
        OR (source_kind = 'legacyBridge' AND start_command_id IS NULL)
    ),
    CONSTRAINT ck_military_service_period CHECK (
        start_game_day < end_game_day
        AND start_date < end_exclusive_date
    ),
    CONSTRAINT ck_military_service_credit CHECK (
        credited_service_days <= end_game_day - start_game_day
        AND (
            (
                credited_service_days = 0
                AND last_credited_game_day IS NULL
            )
            OR (
                credited_service_days > 0
                AND last_credited_game_day
                    = start_game_day + credited_service_days - 1
                AND last_credited_game_day < end_game_day
            )
        )
    ),
    CONSTRAINT ck_military_service_state_shape CHECK (
        (
            status = 'pendingStart'
            AND credited_service_days = 0
            AND last_credited_game_day IS NULL
            AND completed_game_day IS NULL
        )
        OR (
            status = 'serving'
            AND completed_game_day IS NULL
        )
        OR (
            status = 'completed'
            AND completed_game_day = end_game_day
            AND credited_service_days = end_game_day - start_game_day
            AND last_credited_game_day = end_game_day - 1
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

INSERT INTO military_service
    (
        save_id,
        run_revision,
        career_catalog_bundle_id,
        employment_policy_set_id,
        military_option_version_id,
        military_option_policy_id,
        service_type,
        status,
        source_kind,
        start_command_id,
        start_game_day,
        end_game_day,
        start_date,
        end_exclusive_date,
        credited_service_days,
        last_credited_game_day,
        completed_game_day
    )
SELECT career_run.save_id, career_run.run_revision,
       career_run.career_catalog_bundle_id, career_run.employment_policy_set_id,
       option_row.id, option_policy.id, option_row.service_type,
       'pendingStart', 'legacyBridge', NULL, save.game_day + 1,
       DATEDIFF(
           DATE_ADD(
               DATE_ADD(market_world.start_date, INTERVAL save.game_day + 1 DAY),
               INTERVAL option_policy.service_duration_months MONTH
           ),
           market_world.start_date
       ),
       DATE_ADD(market_world.start_date, INTERVAL save.game_day + 1 DAY),
       DATE_ADD(
           DATE_ADD(market_world.start_date, INTERVAL save.game_day + 1 DAY),
           INTERVAL option_policy.service_duration_months MONTH
       ),
       0, NULL, NULL
FROM career_run
INNER JOIN save
    ON save.id = career_run.save_id
   AND save.run_revision = career_run.run_revision
INNER JOIN `character`
    ON `character`.save_id = career_run.save_id
INNER JOIN market_world
    ON market_world.id = save.market_world_id
INNER JOIN military_option_version AS option_row
    ON option_row.career_catalog_bundle_id = career_run.career_catalog_bundle_id
   AND BINARY option_row.service_type = BINARY CASE BINARY `character`.military
       WHEN BINARY 'serving' THEN 'activeDuty'
       WHEN BINARY 'alternative' THEN 'socialService'
       ELSE ''
   END
INNER JOIN military_option_policy AS option_policy
    ON option_policy.employment_policy_set_id = career_run.employment_policy_set_id
   AND option_policy.career_catalog_bundle_id
        = career_run.career_catalog_bundle_id
   AND option_policy.military_option_version_id = option_row.id
   AND option_policy.availability_status = 'available'
   AND DATE_ADD(market_world.start_date, INTERVAL save.game_day + 1 DAY)
        >= option_policy.effective_from
   AND (
       option_policy.effective_to_exclusive IS NULL
       OR DATE_ADD(market_world.start_date, INTERVAL save.game_day + 1 DAY)
            < option_policy.effective_to_exclusive
   )
WHERE `character`.military IN ('serving', 'alternative')
  AND career_run.military_status = 'serving'
  AND NOT EXISTS (
      SELECT 1 FROM military_service AS existing
      WHERE existing.save_id = career_run.save_id
        AND existing.run_revision = career_run.run_revision
  );

CREATE TABLE military_service_progress (
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED NOT NULL,
    career_catalog_bundle_id            BIGINT UNSIGNED NOT NULL,
    military_service_id                 BIGINT UNSIGNED NOT NULL,
    career_job_family_id                BIGINT UNSIGNED NOT NULL,
    military_option_version_id          BIGINT UNSIGNED NOT NULL,
    experience_credit_ppm               INT UNSIGNED NOT NULL,
    credited_experience_day_ppm         BIGINT UNSIGNED NOT NULL DEFAULT 0,
    last_credited_game_day              INT UNSIGNED NULL,
    status                              VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (military_service_id, career_job_family_id),
    KEY ix_military_service_progress_run
        (save_id, run_revision, military_service_id, career_job_family_id),
    CONSTRAINT fk_military_service_progress_service
        FOREIGN KEY (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            military_service_id
        ) REFERENCES military_service (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            id
        ) ON DELETE CASCADE,
    CONSTRAINT fk_military_service_progress_mapping
        FOREIGN KEY (
            career_catalog_bundle_id,
            military_option_version_id,
            career_job_family_id
        ) REFERENCES military_option_job_family (
            career_catalog_bundle_id,
            military_option_version_id,
            career_job_family_id
        ),
    CONSTRAINT ck_military_service_progress_values CHECK (
        experience_credit_ppm BETWEEN 1 AND 1000000
        AND credited_experience_day_ppm <= 9007199254740991
        AND status IN ('active', 'finalized')
        AND (
            (credited_experience_day_ppm = 0 AND last_credited_game_day IS NULL)
            OR (credited_experience_day_ppm > 0 AND last_credited_game_day IS NOT NULL)
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

INSERT INTO military_service_progress
    (
        save_id,
        run_revision,
        career_catalog_bundle_id,
        military_service_id,
        career_job_family_id,
        military_option_version_id,
        experience_credit_ppm,
        credited_experience_day_ppm,
        last_credited_game_day,
        status
    )
SELECT service.save_id, service.run_revision, service.career_catalog_bundle_id,
       service.id, mapping.career_job_family_id,
       mapping.military_option_version_id, mapping.experience_credit_ppm,
       0, NULL, 'active'
FROM military_service AS service
INNER JOIN military_option_job_family AS mapping
    ON mapping.career_catalog_bundle_id = service.career_catalog_bundle_id
   AND mapping.military_option_version_id = service.military_option_version_id;

CREATE TRIGGER tr_military_service_valid_insert
BEFORE INSERT ON military_service
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'pendingStart'
        AND NEW.credited_service_days = 0
        AND NEW.last_credited_game_day IS NULL
        AND NEW.completed_game_day IS NULL
        AND (
            (
                NEW.source_kind = 'userCommand'
                AND EXISTS (
            SELECT 1
            FROM save
            INNER JOIN `character`
                ON `character`.save_id = save.id
            INNER JOIN market_world
                ON market_world.id = save.market_world_id
            INNER JOIN career_run
                ON career_run.save_id = save.id
               AND career_run.run_revision = save.run_revision
               AND career_run.career_catalog_bundle_id
                    = NEW.career_catalog_bundle_id
               AND career_run.employment_policy_set_id
                    = NEW.employment_policy_set_id
               AND career_run.military_status = 'unserved'
            INNER JOIN military_option_policy AS option_policy
                ON option_policy.id = NEW.military_option_policy_id
               AND option_policy.employment_policy_set_id
                    = NEW.employment_policy_set_id
               AND option_policy.career_catalog_bundle_id
                    = NEW.career_catalog_bundle_id
               AND option_policy.military_option_version_id
                    = NEW.military_option_version_id
               AND BINARY option_policy.service_type = BINARY NEW.service_type
               AND option_policy.availability_status = 'available'
            INNER JOIN military_option_eligibility_rule AS eligibility
                ON eligibility.career_catalog_bundle_id
                    = NEW.career_catalog_bundle_id
               AND eligibility.military_option_version_id
                    = NEW.military_option_version_id
            INNER JOIN command_identity AS identity
                ON identity.save_id = save.id
               AND BINARY identity.command_id = BINARY NEW.start_command_id
               AND identity.command_kind = 'startMilitaryService'
               AND identity.initial_run_revision = save.run_revision
               AND identity.initial_game_day = save.game_day
            WHERE save.id = NEW.save_id
              AND save.run_revision = NEW.run_revision
              AND `character`.military = 'notServed'
              AND NEW.start_game_day = save.game_day + 1
              AND NEW.start_date
                  = DATE_ADD(market_world.start_date, INTERVAL NEW.start_game_day DAY)
              AND NEW.end_exclusive_date = DATE_ADD(
                  NEW.start_date,
                  INTERVAL option_policy.service_duration_months MONTH
              )
              AND NEW.end_game_day
                  = DATEDIFF(NEW.end_exclusive_date, market_world.start_date)
              AND NEW.start_date >= option_policy.effective_from
              AND (
                  option_policy.effective_to_exclusive IS NULL
                  OR NEW.start_date < option_policy.effective_to_exclusive
              )
              AND (
                  eligibility.minimum_education IS NULL
                  OR FIELD(
                      `character`.education,
                      'highSchool', 'associate', 'bachelor', 'master', 'doctorate'
                  ) >= FIELD(
                      eligibility.minimum_education,
                      'highSchool', 'associate', 'bachelor', 'master', 'doctorate'
                  )
              )
              AND (
                  SELECT COUNT(*)
                  FROM spec_evidence AS evidence
                  WHERE evidence.save_id = save.id
                    AND evidence.run_revision = save.run_revision
                    AND evidence.kind = 'certification'
                    AND (
                        evidence.expires_on_game_day IS NULL
                        OR evidence.expires_on_game_day >= save.game_day
                    )
              ) >= eligibility.required_certification_count
              AND (
                  SELECT COALESCE(SUM(evidence.credited_experience_days), 0)
                  FROM spec_evidence AS evidence
                  WHERE evidence.save_id = save.id
                    AND evidence.run_revision = save.run_revision
                    AND evidence.kind = 'experience'
                    AND evidence.period_start_date IS NOT NULL
                    AND evidence.period_end_exclusive_date IS NOT NULL
              ) >= eligibility.minimum_experience_days
                )
            )
            OR (
                NEW.source_kind = 'legacyBridge'
                AND EXISTS (
                    SELECT 1
                    FROM save
                    INNER JOIN `character`
                        ON `character`.save_id = save.id
                    INNER JOIN market_world
                        ON market_world.id = save.market_world_id
                    INNER JOIN career_run
                        ON career_run.save_id = save.id
                       AND career_run.run_revision = save.run_revision
                       AND career_run.career_catalog_bundle_id
                            = NEW.career_catalog_bundle_id
                       AND career_run.employment_policy_set_id
                            = NEW.employment_policy_set_id
                       AND career_run.military_status = 'serving'
                    INNER JOIN military_option_policy AS option_policy
                        ON option_policy.id = NEW.military_option_policy_id
                       AND option_policy.employment_policy_set_id
                            = NEW.employment_policy_set_id
                       AND option_policy.career_catalog_bundle_id
                            = NEW.career_catalog_bundle_id
                       AND option_policy.military_option_version_id
                            = NEW.military_option_version_id
                       AND BINARY option_policy.service_type
                            = BINARY NEW.service_type
                       AND option_policy.availability_status = 'available'
                    WHERE save.id = NEW.save_id
                      AND save.run_revision = NEW.run_revision
                      AND BINARY NEW.service_type = BINARY CASE BINARY `character`.military
                          WHEN BINARY 'serving' THEN 'activeDuty'
                          WHEN BINARY 'alternative' THEN 'socialService'
                          ELSE ''
                      END
                      AND NEW.start_game_day = save.game_day + 1
                      AND NEW.start_date = DATE_ADD(
                          market_world.start_date,
                          INTERVAL NEW.start_game_day DAY
                      )
                      AND NEW.end_exclusive_date = DATE_ADD(
                          NEW.start_date,
                          INTERVAL option_policy.service_duration_months MONTH
                      )
                      AND NEW.end_game_day = DATEDIFF(
                          NEW.end_exclusive_date,
                          market_world.start_date
                      )
                      AND NEW.start_date >= option_policy.effective_from
                      AND (
                          option_policy.effective_to_exclusive IS NULL
                          OR NEW.start_date < option_policy.effective_to_exclusive
                      )
                )
            )
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_military_service_transition_only
BEFORE UPDATE ON military_service
FOR EACH ROW
SET NEW.id = IF(
    NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.career_catalog_bundle_id = OLD.career_catalog_bundle_id
        AND NEW.employment_policy_set_id = OLD.employment_policy_set_id
        AND NEW.military_option_version_id = OLD.military_option_version_id
        AND NEW.military_option_policy_id = OLD.military_option_policy_id
        AND BINARY NEW.service_type = BINARY OLD.service_type
        AND BINARY NEW.source_kind = BINARY OLD.source_kind
        AND NEW.start_command_id <=> OLD.start_command_id
        AND NEW.start_game_day = OLD.start_game_day
        AND NEW.end_game_day = OLD.end_game_day
        AND NEW.start_date = OLD.start_date
        AND NEW.end_exclusive_date = OLD.end_exclusive_date
        AND NEW.created_at = OLD.created_at
        AND (
            (
                OLD.status = 'pendingStart'
                AND NEW.status = 'serving'
                AND NEW.credited_service_days = 0
                AND NEW.last_credited_game_day IS NULL
                AND NEW.completed_game_day IS NULL
                AND OLD.start_game_day = (
                    SELECT game_day + 1 FROM save
                    WHERE id = OLD.save_id AND run_revision = OLD.run_revision
                )
            )
            OR (
                OLD.status = 'serving'
                AND NEW.status = 'serving'
                AND NEW.credited_service_days = OLD.credited_service_days + 1
                AND NEW.last_credited_game_day
                    = OLD.start_game_day + NEW.credited_service_days - 1
                AND NEW.completed_game_day IS NULL
                AND NEW.last_credited_game_day = (
                    SELECT game_day + 1 FROM save
                    WHERE id = OLD.save_id AND run_revision = OLD.run_revision
                )
                AND NEW.last_credited_game_day < OLD.end_game_day
            )
            OR (
                OLD.status = 'serving'
                AND NEW.status = 'completed'
                AND NEW.credited_service_days
                    = OLD.end_game_day - OLD.start_game_day
                AND NEW.last_credited_game_day = OLD.end_game_day - 1
                AND NEW.completed_game_day = OLD.end_game_day
                AND OLD.end_game_day = (
                    SELECT game_day + 1 FROM save
                    WHERE id = OLD.save_id AND run_revision = OLD.run_revision
                )
            )
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_military_service_no_delete
BEFORE DELETE ON military_service
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military services cannot be deleted';

CREATE TRIGGER tr_military_service_progress_valid_insert
BEFORE INSERT ON military_service_progress
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'active'
        AND NEW.credited_experience_day_ppm = 0
        AND NEW.last_credited_game_day IS NULL
        AND EXISTS (
            SELECT 1
            FROM military_service AS service
            INNER JOIN military_option_job_family AS mapping
                ON mapping.career_catalog_bundle_id
                    = service.career_catalog_bundle_id
               AND mapping.military_option_version_id
                    = service.military_option_version_id
               AND mapping.career_job_family_id = NEW.career_job_family_id
            WHERE service.id = NEW.military_service_id
              AND service.save_id = NEW.save_id
              AND service.run_revision = NEW.run_revision
              AND service.career_catalog_bundle_id
                    = NEW.career_catalog_bundle_id
              AND service.military_option_version_id
                    = NEW.military_option_version_id
              AND mapping.experience_credit_ppm = NEW.experience_credit_ppm
              AND service.status IN ('pendingStart', 'serving')
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_military_service_progress_transition
BEFORE UPDATE ON military_service_progress
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.career_catalog_bundle_id = OLD.career_catalog_bundle_id
        AND NEW.military_service_id = OLD.military_service_id
        AND NEW.career_job_family_id = OLD.career_job_family_id
        AND NEW.military_option_version_id = OLD.military_option_version_id
        AND NEW.experience_credit_ppm = OLD.experience_credit_ppm
        AND NEW.created_at = OLD.created_at
        AND OLD.status = 'active'
        AND (
            (
                NEW.status = 'active'
                AND NEW.credited_experience_day_ppm
                    = OLD.credited_experience_day_ppm + OLD.experience_credit_ppm
                AND NEW.last_credited_game_day = (
                    SELECT last_credited_game_day
                    FROM military_service
                    WHERE id = OLD.military_service_id
                      AND save_id = OLD.save_id
                      AND run_revision = OLD.run_revision
                )
            )
            OR (
                NEW.status = 'finalized'
                AND NEW.credited_experience_day_ppm
                    = OLD.credited_experience_day_ppm
                AND NEW.last_credited_game_day <=> OLD.last_credited_game_day
                AND EXISTS (
                    SELECT 1 FROM military_service
                    WHERE id = OLD.military_service_id
                      AND save_id = OLD.save_id
                      AND run_revision = OLD.run_revision
                      AND status = 'completed'
                )
            )
        ),
    OLD.save_id,
    NULL
);

CREATE TRIGGER tr_military_service_progress_no_delete
BEFORE DELETE ON military_service_progress
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military service progress cannot be deleted';

CREATE TRIGGER tr_career_run_valid_insert
BEFORE INSERT ON career_run
FOR EACH ROW
SET
    NEW.save_id = IF(
        EXISTS (
            SELECT 1
            FROM save
            INNER JOIN `character` ON `character`.save_id = save.id
            INNER JOIN market_world ON market_world.id = save.market_world_id
            INNER JOIN career_catalog_assignment AS career_assignment
                ON BINARY career_assignment.assignment_key = BINARY 'newRun'
               AND career_assignment.career_catalog_bundle_id
                    = NEW.career_catalog_bundle_id
            INNER JOIN career_catalog_bundle AS bundle
                ON bundle.id = career_assignment.career_catalog_bundle_id
               AND bundle.published_at IS NOT NULL
            INNER JOIN employment_policy_assignment AS employment_assignment
                ON BINARY employment_assignment.assignment_key = BINARY 'newRun'
               AND employment_assignment.employment_policy_set_id
                    = NEW.employment_policy_set_id
            INNER JOIN employment_policy_set AS employment_policy
                ON employment_policy.id
                    = employment_assignment.employment_policy_set_id
               AND employment_policy.published_at IS NOT NULL
            INNER JOIN employment_finance_compatibility AS compatibility
                ON compatibility.employment_policy_set_id = employment_policy.id
               AND compatibility.policy_set_id = save.policy_set_id
            WHERE save.id = NEW.save_id
              AND save.run_revision = NEW.run_revision
              AND BINARY NEW.focused_job_family_key
                  = BINARY bundle.default_focused_job_family_key
              AND NEW.birth_date
                  = MAKEDATE(YEAR(market_world.start_date) - `character`.age, 1)
        ),
        NEW.save_id,
        NULL
    ),
    NEW.military_status = (
        SELECT CASE BINARY `character`.military
            WHEN BINARY 'notServed' THEN 'unserved'
            WHEN BINARY 'serving' THEN 'serving'
            WHEN BINARY 'alternative' THEN 'serving'
            WHEN BINARY 'completed' THEN 'completed'
            WHEN BINARY 'exempted' THEN 'exempt'
            ELSE NULL
        END
        FROM `character`
        WHERE `character`.save_id = NEW.save_id
    );

CREATE TRIGGER tr_career_run_focus_only
BEFORE UPDATE ON career_run
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.career_catalog_bundle_id = OLD.career_catalog_bundle_id
        AND NEW.employment_policy_set_id = OLD.employment_policy_set_id
        AND NEW.birth_date = OLD.birth_date
        AND NEW.created_at = OLD.created_at
        AND EXISTS (
            SELECT 1 FROM save
            WHERE id = OLD.save_id AND run_revision = OLD.run_revision
        )
        AND EXISTS (
            SELECT 1 FROM career_job_family AS family
            WHERE family.career_catalog_bundle_id = OLD.career_catalog_bundle_id
              AND BINARY family.job_family_key = BINARY NEW.focused_job_family_key
        )
        AND (
            BINARY NEW.military_status = BINARY OLD.military_status
            OR (
                OLD.military_status = 'unserved'
                AND NEW.military_status = 'serving'
                AND EXISTS (
                    SELECT 1 FROM military_service AS service
                    WHERE service.save_id = OLD.save_id
                      AND service.run_revision = OLD.run_revision
                      AND (
                          service.status = 'serving'
                          OR (
                              service.status = 'pendingStart'
                              AND EXISTS (
                                  SELECT 1
                                  FROM career_scheduled_action AS start_action
                                  WHERE start_action.save_id = service.save_id
                                    AND start_action.run_revision = service.run_revision
                                    AND start_action.status = 'pending'
                                    AND start_action.action_kind
                                        = 'militaryServiceStart'
                                    AND start_action.source_kind = 'militaryService'
                                    AND start_action.source_id = service.id
                                    AND start_action.occurrence = 1
                                    AND start_action.due_game_day
                                        = service.start_game_day
                              )
                              AND EXISTS (
                                  SELECT 1
                                  FROM career_scheduled_action AS completion_action
                                  WHERE completion_action.save_id = service.save_id
                                    AND completion_action.run_revision
                                        = service.run_revision
                                    AND completion_action.status = 'pending'
                                    AND completion_action.action_kind
                                        = 'militaryServiceCompletion'
                                    AND completion_action.source_kind
                                        = 'militaryService'
                                    AND completion_action.source_id = service.id
                                    AND completion_action.occurrence = 2
                                    AND completion_action.due_game_day
                                        = service.end_game_day
                              )
                          )
                      )
                )
            )
            OR (
                OLD.military_status = 'serving'
                AND NEW.military_status = 'completed'
                AND EXISTS (
                    SELECT 1 FROM military_service AS service
                    WHERE service.save_id = OLD.save_id
                      AND service.run_revision = OLD.run_revision
                      AND service.status = 'completed'
                )
            )
        ),
    OLD.save_id,
    NULL
);

-- Every payroll and military-pay fact feeds one append-only annual event. The payroll backfill
-- preserves existing identities, policy pins, settlement links, and exact integer totals.
CREATE TABLE employment_income_event (
    id                                      BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                                 BIGINT UNSIGNED NOT NULL,
    run_revision                            INT UNSIGNED NOT NULL,
    employment_policy_set_id                BIGINT UNSIGNED NOT NULL,
    source_kind                             VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    source_id                               BIGINT UNSIGNED NOT NULL,
    occurrence                              BIGINT UNSIGNED NOT NULL,
    payroll_record_id                       BIGINT UNSIGNED NULL,
    military_service_id                     BIGINT UNSIGNED NULL,
    scheduled_settlement_id                 BIGINT UNSIGNED NOT NULL,
    ledger_transaction_id                   BIGINT UNSIGNED NULL,
    paid_game_day                           INT UNSIGNED NOT NULL,
    paid_date                               DATE NOT NULL,
    tax_year                                SMALLINT UNSIGNED NOT NULL,
    gross_employment_income_krw             BIGINT NOT NULL,
    employee_national_pension_krw           BIGINT NOT NULL,
    employee_health_insurance_krw           BIGINT NOT NULL,
    employee_long_term_care_krw             BIGINT NOT NULL,
    employee_employment_insurance_krw       BIGINT NOT NULL,
    employee_insurance_total_krw            BIGINT NOT NULL,
    withheld_income_tax_krw                 BIGINT NOT NULL,
    withheld_local_income_tax_krw           BIGINT NOT NULL,
    net_pay_krw                             BIGINT NOT NULL,
    created_at                              DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_employment_income_event_source
        (save_id, run_revision, source_kind, source_id, occurrence),
    UNIQUE KEY uk_employment_income_event_save_run_id
        (save_id, run_revision, id),
    UNIQUE KEY uk_employment_income_event_payroll
        (save_id, run_revision, payroll_record_id),
    UNIQUE KEY uk_employment_income_event_settlement
        (save_id, run_revision, scheduled_settlement_id),
    UNIQUE KEY uk_employment_income_event_ledger
        (save_id, run_revision, ledger_transaction_id),
    KEY ix_employment_income_event_year
        (save_id, run_revision, tax_year, id),
    KEY ix_employment_income_event_military
        (save_id, run_revision, military_service_id, occurrence),
    CONSTRAINT fk_employment_income_event_career_run
        FOREIGN KEY (save_id, run_revision, employment_policy_set_id)
        REFERENCES career_run (save_id, run_revision, employment_policy_set_id),
    CONSTRAINT fk_employment_income_event_payroll
        FOREIGN KEY (save_id, run_revision, payroll_record_id)
        REFERENCES payroll_record (save_id, run_revision, id),
    CONSTRAINT fk_employment_income_event_military
        FOREIGN KEY (save_id, run_revision, military_service_id)
        REFERENCES military_service (save_id, run_revision, id),
    CONSTRAINT fk_employment_income_event_settlement
        FOREIGN KEY (scheduled_settlement_id) REFERENCES scheduled_settlement (id),
    CONSTRAINT fk_employment_income_event_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT ck_employment_income_event_source CHECK (
        (
            source_kind = 'employmentPayroll'
            AND payroll_record_id IS NOT NULL
            AND military_service_id IS NULL
            AND source_id = payroll_record_id
        )
        OR (
            source_kind = 'militaryPay'
            AND payroll_record_id IS NULL
            AND military_service_id IS NOT NULL
            AND source_id = military_service_id
        )
    ),
    CONSTRAINT ck_employment_income_event_identity CHECK (
        occurrence BETWEEN 1 AND 9007199254740991
        AND tax_year BETWEEN 1 AND 9999
        AND tax_year = YEAR(paid_date)
    ),
    CONSTRAINT ck_employment_income_event_amounts CHECK (
        gross_employment_income_krw BETWEEN 0 AND 9007199254740991
        AND employee_national_pension_krw >= 0
        AND employee_health_insurance_krw >= 0
        AND employee_long_term_care_krw >= 0
        AND employee_employment_insurance_krw >= 0
        AND employee_insurance_total_krw >= 0
        AND withheld_income_tax_krw >= 0
        AND withheld_local_income_tax_krw >= 0
        AND net_pay_krw >= 0
        AND CAST(employee_insurance_total_krw AS DECIMAL(65, 0))
            = CAST(employee_national_pension_krw AS DECIMAL(65, 0))
            + CAST(employee_health_insurance_krw AS DECIMAL(65, 0))
            + CAST(employee_long_term_care_krw AS DECIMAL(65, 0))
            + CAST(employee_employment_insurance_krw AS DECIMAL(65, 0))
        AND CAST(net_pay_krw AS DECIMAL(65, 0))
            = CAST(gross_employment_income_krw AS DECIMAL(65, 0))
            - CAST(employee_insurance_total_krw AS DECIMAL(65, 0))
            - CAST(withheld_income_tax_krw AS DECIMAL(65, 0))
            - CAST(withheld_local_income_tax_krw AS DECIMAL(65, 0))
        AND (
            (gross_employment_income_krw = 0 AND ledger_transaction_id IS NULL)
            OR (gross_employment_income_krw > 0 AND ledger_transaction_id IS NOT NULL)
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

INSERT INTO employment_income_event
    (
        save_id,
        run_revision,
        employment_policy_set_id,
        source_kind,
        source_id,
        occurrence,
        payroll_record_id,
        military_service_id,
        scheduled_settlement_id,
        ledger_transaction_id,
        paid_game_day,
        paid_date,
        tax_year,
        gross_employment_income_krw,
        employee_national_pension_krw,
        employee_health_insurance_krw,
        employee_long_term_care_krw,
        employee_employment_insurance_krw,
        employee_insurance_total_krw,
        withheld_income_tax_krw,
        withheld_local_income_tax_krw,
        net_pay_krw,
        created_at
    )
SELECT payroll.save_id, payroll.run_revision, payroll.employment_policy_set_id,
       'employmentPayroll', payroll.id, payroll.period_no, payroll.id, NULL,
       payroll.scheduled_settlement_id, payroll.ledger_transaction_id,
       payroll.payday_game_day, payroll.payday, payroll.tax_year,
       payroll.gross_pay_krw, payroll.national_pension_employee_krw,
       payroll.health_insurance_employee_krw, payroll.long_term_care_employee_krw,
       payroll.employment_insurance_employee_krw,
       payroll.employee_insurance_total_krw, payroll.withheld_income_tax_krw,
       payroll.withheld_local_income_tax_krw, payroll.net_salary_pay_krw,
       payroll.created_at
FROM payroll_record AS payroll;

DROP TRIGGER tr_employment_income_year_valid_insert;
DROP TRIGGER tr_employment_income_year_transition_only;

ALTER TABLE employment_income_year
    ADD COLUMN income_event_count BIGINT UNSIGNED NOT NULL DEFAULT 0
        AFTER net_salary_pay_krw,
    ADD COLUMN last_income_event_id BIGINT UNSIGNED NULL
        AFTER income_event_count;

UPDATE employment_income_year AS income_year
SET income_year.income_event_count = (
        SELECT COUNT(*)
        FROM employment_income_event AS income_event
        WHERE income_event.save_id = income_year.save_id
          AND income_event.run_revision = income_year.run_revision
          AND income_event.tax_year = income_year.tax_year
    ),
    income_year.last_income_event_id = (
        SELECT MAX(income_event.id)
        FROM employment_income_event AS income_event
        WHERE income_event.save_id = income_year.save_id
          AND income_event.run_revision = income_year.run_revision
          AND income_event.tax_year = income_year.tax_year
    );

ALTER TABLE employment_income_year
    DROP CHECK ck_employment_income_year_state,
    ADD KEY ix_employment_income_year_last_event
        (save_id, run_revision, last_income_event_id),
    ADD CONSTRAINT fk_employment_income_year_last_event
        FOREIGN KEY (save_id, run_revision, last_income_event_id)
        REFERENCES employment_income_event (save_id, run_revision, id),
    ADD CONSTRAINT ck_employment_income_year_event_state CHECK (
        (
            income_event_count = 0
            AND last_income_event_id IS NULL
            AND gross_employment_income_krw = 0
            AND payroll_count = 0
            AND last_payroll_record_id IS NULL
        )
        OR (
            income_event_count > 0
            AND last_income_event_id IS NOT NULL
            AND (
                (payroll_count = 0 AND last_payroll_record_id IS NULL)
                OR (payroll_count > 0 AND last_payroll_record_id IS NOT NULL)
            )
        )
    );

CREATE TRIGGER tr_employment_income_event_valid_insert
BEFORE INSERT ON employment_income_event
FOR EACH ROW
SET NEW.save_id = IF(
    NOT EXISTS (
        SELECT 1 FROM employment_income_year AS income_year
        WHERE income_year.save_id = NEW.save_id
          AND income_year.run_revision = NEW.run_revision
          AND income_year.tax_year = NEW.tax_year
          AND income_year.status = 'finalized'
    )
        AND (
            (
                NEW.source_kind = 'employmentPayroll'
                AND EXISTS (
                    SELECT 1
                    FROM payroll_record AS payroll
                    WHERE payroll.id = NEW.payroll_record_id
                      AND payroll.save_id = NEW.save_id
                      AND payroll.run_revision = NEW.run_revision
                      AND payroll.employment_policy_set_id
                            = NEW.employment_policy_set_id
                      AND payroll.period_no = NEW.occurrence
                      AND payroll.scheduled_settlement_id
                            = NEW.scheduled_settlement_id
                      AND payroll.ledger_transaction_id
                            <=> NEW.ledger_transaction_id
                      AND payroll.payday_game_day = NEW.paid_game_day
                      AND payroll.payday = NEW.paid_date
                      AND payroll.tax_year = NEW.tax_year
                      AND payroll.gross_pay_krw
                            = NEW.gross_employment_income_krw
                      AND payroll.national_pension_employee_krw
                            = NEW.employee_national_pension_krw
                      AND payroll.health_insurance_employee_krw
                            = NEW.employee_health_insurance_krw
                      AND payroll.long_term_care_employee_krw
                            = NEW.employee_long_term_care_krw
                      AND payroll.employment_insurance_employee_krw
                            = NEW.employee_employment_insurance_krw
                      AND payroll.employee_insurance_total_krw
                            = NEW.employee_insurance_total_krw
                      AND payroll.withheld_income_tax_krw
                            = NEW.withheld_income_tax_krw
                      AND payroll.withheld_local_income_tax_krw
                            = NEW.withheld_local_income_tax_krw
                      AND payroll.net_salary_pay_krw = NEW.net_pay_krw
                )
            )
            OR (
                NEW.source_kind = 'militaryPay'
                AND EXISTS (
                    SELECT 1
                    FROM military_service AS service
                    INNER JOIN save
                        ON save.id = service.save_id
                       AND save.run_revision = service.run_revision
                    INNER JOIN market_world
                        ON market_world.id = save.market_world_id
                    INNER JOIN military_option_policy AS option_policy
                        ON option_policy.id = service.military_option_policy_id
                       AND option_policy.employment_policy_set_id
                            = service.employment_policy_set_id
                       AND option_policy.career_catalog_bundle_id
                            = service.career_catalog_bundle_id
                       AND option_policy.military_option_version_id
                            = service.military_option_version_id
                    INNER JOIN military_pay_stage AS pay_stage
                        ON pay_stage.military_option_policy_id = option_policy.id
                       AND pay_stage.start_service_month <= (
                           (YEAR(NEW.paid_date) - YEAR(service.start_date)) * 12
                               + MONTH(NEW.paid_date) - MONTH(service.start_date)
                               - IF(
                                   TIMESTAMPADD(
                                       MONTH,
                                       (YEAR(NEW.paid_date) - YEAR(service.start_date)) * 12
                                           + MONTH(NEW.paid_date)
                                           - MONTH(service.start_date),
                                       service.start_date
                                   ) > NEW.paid_date,
                                   1,
                                   0
                               )
                       )
                       AND pay_stage.end_service_month_exclusive > (
                           (YEAR(NEW.paid_date) - YEAR(service.start_date)) * 12
                               + MONTH(NEW.paid_date) - MONTH(service.start_date)
                               - IF(
                                   TIMESTAMPADD(
                                       MONTH,
                                       (YEAR(NEW.paid_date) - YEAR(service.start_date)) * 12
                                           + MONTH(NEW.paid_date)
                                           - MONTH(service.start_date),
                                       service.start_date
                                   ) > NEW.paid_date,
                                   1,
                                   0
                               )
                       )
                    INNER JOIN scheduled_settlement AS settlement
                        ON settlement.id = NEW.scheduled_settlement_id
                       AND settlement.save_id = service.save_id
                       AND settlement.run_revision = service.run_revision
                       AND settlement.kind = 'militaryPay'
                       AND settlement.source_kind = 'militaryService'
                       AND BINARY settlement.source_id
                            = BINARY CAST(service.id AS CHAR)
                       AND settlement.occurrence = NEW.occurrence
                       AND settlement.due_game_day = NEW.paid_game_day
                       AND settlement.status = 'pending'
                    LEFT JOIN ledger_transaction AS ledger
                        ON ledger.save_id = service.save_id
                       AND ledger.run_revision = service.run_revision
                       AND ledger.id = NEW.ledger_transaction_id
                    WHERE service.id = NEW.military_service_id
                      AND service.save_id = NEW.save_id
                      AND service.run_revision = NEW.run_revision
                      AND service.employment_policy_set_id
                            = NEW.employment_policy_set_id
                      AND service.status IN ('serving', 'completed')
                      AND NEW.paid_game_day >= service.start_game_day
                      AND NEW.paid_game_day < service.end_game_day
                      AND NEW.paid_date = DATE_ADD(
                          market_world.start_date,
                          INTERVAL NEW.paid_game_day DAY
                      )
                      AND NEW.gross_employment_income_krw
                            = pay_stage.monthly_gross_pay_krw
                      AND (
                          option_policy.social_insurance_kind = 'employmentPayroll'
                          OR (
                              option_policy.social_insurance_kind = 'notAssessed'
                              AND NEW.employee_national_pension_krw = 0
                              AND NEW.employee_health_insurance_krw = 0
                              AND NEW.employee_long_term_care_krw = 0
                              AND NEW.employee_employment_insurance_krw = 0
                              AND NEW.employee_insurance_total_krw = 0
                          )
                      )
                      AND (
                          (
                              NEW.gross_employment_income_krw = 0
                              AND ledger.id IS NULL
                          )
                          OR (
                              NEW.gross_employment_income_krw > 0
                              AND ledger.policy_set_id = save.policy_set_id
                              AND ledger.game_day = NEW.paid_game_day
                              AND BINARY ledger.source_kind = BINARY 'militaryPay'
                              AND BINARY ledger.source_id
                                    = BINARY CAST(settlement.id AS CHAR)
                          )
                      )
                )
            )
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_employment_income_event_no_update
BEFORE UPDATE ON employment_income_event
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment income events are immutable';

CREATE TRIGGER tr_employment_income_event_no_delete
BEFORE DELETE ON employment_income_event
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment income events are immutable';

CREATE TRIGGER tr_employment_income_year_valid_insert
BEFORE INSERT ON employment_income_year
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'open'
        AND NEW.finalized_on IS NULL
        AND EXISTS (
            SELECT 1 FROM career_run
            WHERE save_id = NEW.save_id
              AND run_revision = NEW.run_revision
              AND employment_policy_set_id = NEW.employment_policy_set_id
        )
        AND (
            (
                NEW.income_event_count = 0
                AND NEW.last_income_event_id IS NULL
                AND NEW.payroll_count = 0
                AND NEW.last_payroll_record_id IS NULL
                AND NEW.gross_employment_income_krw = 0
                AND NEW.employee_national_pension_krw = 0
                AND NEW.employee_health_insurance_krw = 0
                AND NEW.employee_long_term_care_krw = 0
                AND NEW.employee_employment_insurance_krw = 0
                AND NEW.employee_insurance_total_krw = 0
                AND NEW.withheld_income_tax_krw = 0
                AND NEW.withheld_local_income_tax_krw = 0
                AND NEW.net_salary_pay_krw = 0
            )
            OR (
                NEW.income_event_count = 1
                AND NEW.last_income_event_id IS NOT NULL
                AND NEW.income_event_count = (
                    SELECT COUNT(*) FROM employment_income_event AS income_event
                    WHERE income_event.save_id = NEW.save_id
                      AND income_event.run_revision = NEW.run_revision
                      AND income_event.tax_year = NEW.tax_year
                )
                AND NEW.last_income_event_id = (
                    SELECT MAX(income_event.id)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = NEW.save_id
                      AND income_event.run_revision = NEW.run_revision
                      AND income_event.tax_year = NEW.tax_year
                )
                AND NEW.payroll_count = (
                    SELECT COUNT(*) FROM employment_income_event AS income_event
                    WHERE income_event.save_id = NEW.save_id
                      AND income_event.run_revision = NEW.run_revision
                      AND income_event.tax_year = NEW.tax_year
                      AND income_event.source_kind = 'employmentPayroll'
                )
                AND NEW.last_payroll_record_id <=> (
                    SELECT MAX(income_event.payroll_record_id)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = NEW.save_id
                      AND income_event.run_revision = NEW.run_revision
                      AND income_event.tax_year = NEW.tax_year
                )
                AND NEW.gross_employment_income_krw = (
                    SELECT SUM(income_event.gross_employment_income_krw)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = NEW.save_id
                      AND income_event.run_revision = NEW.run_revision
                      AND income_event.tax_year = NEW.tax_year
                )
                AND NEW.employee_national_pension_krw = (
                    SELECT SUM(income_event.employee_national_pension_krw)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = NEW.save_id
                      AND income_event.run_revision = NEW.run_revision
                      AND income_event.tax_year = NEW.tax_year
                )
                AND NEW.employee_health_insurance_krw = (
                    SELECT SUM(income_event.employee_health_insurance_krw)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = NEW.save_id
                      AND income_event.run_revision = NEW.run_revision
                      AND income_event.tax_year = NEW.tax_year
                )
                AND NEW.employee_long_term_care_krw = (
                    SELECT SUM(income_event.employee_long_term_care_krw)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = NEW.save_id
                      AND income_event.run_revision = NEW.run_revision
                      AND income_event.tax_year = NEW.tax_year
                )
                AND NEW.employee_employment_insurance_krw = (
                    SELECT SUM(income_event.employee_employment_insurance_krw)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = NEW.save_id
                      AND income_event.run_revision = NEW.run_revision
                      AND income_event.tax_year = NEW.tax_year
                )
                AND NEW.employee_insurance_total_krw = (
                    SELECT SUM(income_event.employee_insurance_total_krw)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = NEW.save_id
                      AND income_event.run_revision = NEW.run_revision
                      AND income_event.tax_year = NEW.tax_year
                )
                AND NEW.withheld_income_tax_krw = (
                    SELECT SUM(income_event.withheld_income_tax_krw)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = NEW.save_id
                      AND income_event.run_revision = NEW.run_revision
                      AND income_event.tax_year = NEW.tax_year
                )
                AND NEW.withheld_local_income_tax_krw = (
                    SELECT SUM(income_event.withheld_local_income_tax_krw)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = NEW.save_id
                      AND income_event.run_revision = NEW.run_revision
                      AND income_event.tax_year = NEW.tax_year
                )
                AND NEW.net_salary_pay_krw = (
                    SELECT SUM(income_event.net_pay_krw)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = NEW.save_id
                      AND income_event.run_revision = NEW.run_revision
                      AND income_event.tax_year = NEW.tax_year
                )
            )
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_employment_income_year_transition_only
BEFORE UPDATE ON employment_income_year
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.tax_year = OLD.tax_year
        AND NEW.employment_policy_set_id = OLD.employment_policy_set_id
        AND NEW.created_at = OLD.created_at
        AND OLD.status = 'open'
        AND (
            (
                NEW.status = 'open'
                AND NEW.finalized_on IS NULL
                AND NEW.income_event_count = OLD.income_event_count + 1
                AND NEW.last_income_event_id > COALESCE(OLD.last_income_event_id, 0)
                AND NEW.income_event_count = (
                    SELECT COUNT(*) FROM employment_income_event AS income_event
                    WHERE income_event.save_id = OLD.save_id
                      AND income_event.run_revision = OLD.run_revision
                      AND income_event.tax_year = OLD.tax_year
                )
                AND NEW.last_income_event_id = (
                    SELECT MAX(income_event.id)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = OLD.save_id
                      AND income_event.run_revision = OLD.run_revision
                      AND income_event.tax_year = OLD.tax_year
                )
                AND NEW.payroll_count = (
                    SELECT COUNT(*) FROM employment_income_event AS income_event
                    WHERE income_event.save_id = OLD.save_id
                      AND income_event.run_revision = OLD.run_revision
                      AND income_event.tax_year = OLD.tax_year
                      AND income_event.source_kind = 'employmentPayroll'
                )
                AND NEW.last_payroll_record_id <=> (
                    SELECT MAX(income_event.payroll_record_id)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = OLD.save_id
                      AND income_event.run_revision = OLD.run_revision
                      AND income_event.tax_year = OLD.tax_year
                )
                AND NEW.gross_employment_income_krw = (
                    SELECT COALESCE(SUM(income_event.gross_employment_income_krw), 0)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = OLD.save_id
                      AND income_event.run_revision = OLD.run_revision
                      AND income_event.tax_year = OLD.tax_year
                )
                AND NEW.employee_national_pension_krw = (
                    SELECT COALESCE(SUM(income_event.employee_national_pension_krw), 0)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = OLD.save_id
                      AND income_event.run_revision = OLD.run_revision
                      AND income_event.tax_year = OLD.tax_year
                )
                AND NEW.employee_health_insurance_krw = (
                    SELECT COALESCE(SUM(income_event.employee_health_insurance_krw), 0)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = OLD.save_id
                      AND income_event.run_revision = OLD.run_revision
                      AND income_event.tax_year = OLD.tax_year
                )
                AND NEW.employee_long_term_care_krw = (
                    SELECT COALESCE(SUM(income_event.employee_long_term_care_krw), 0)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = OLD.save_id
                      AND income_event.run_revision = OLD.run_revision
                      AND income_event.tax_year = OLD.tax_year
                )
                AND NEW.employee_employment_insurance_krw = (
                    SELECT COALESCE(SUM(income_event.employee_employment_insurance_krw), 0)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = OLD.save_id
                      AND income_event.run_revision = OLD.run_revision
                      AND income_event.tax_year = OLD.tax_year
                )
                AND NEW.employee_insurance_total_krw = (
                    SELECT COALESCE(SUM(income_event.employee_insurance_total_krw), 0)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = OLD.save_id
                      AND income_event.run_revision = OLD.run_revision
                      AND income_event.tax_year = OLD.tax_year
                )
                AND NEW.withheld_income_tax_krw = (
                    SELECT COALESCE(SUM(income_event.withheld_income_tax_krw), 0)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = OLD.save_id
                      AND income_event.run_revision = OLD.run_revision
                      AND income_event.tax_year = OLD.tax_year
                )
                AND NEW.withheld_local_income_tax_krw = (
                    SELECT COALESCE(SUM(income_event.withheld_local_income_tax_krw), 0)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = OLD.save_id
                      AND income_event.run_revision = OLD.run_revision
                      AND income_event.tax_year = OLD.tax_year
                )
                AND NEW.net_salary_pay_krw = (
                    SELECT COALESCE(SUM(income_event.net_pay_krw), 0)
                    FROM employment_income_event AS income_event
                    WHERE income_event.save_id = OLD.save_id
                      AND income_event.run_revision = OLD.run_revision
                      AND income_event.tax_year = OLD.tax_year
                )
            )
            OR (
                NEW.status = 'finalized'
                AND NEW.gross_employment_income_krw = OLD.gross_employment_income_krw
                AND NEW.employee_national_pension_krw
                    = OLD.employee_national_pension_krw
                AND NEW.employee_health_insurance_krw
                    = OLD.employee_health_insurance_krw
                AND NEW.employee_long_term_care_krw
                    = OLD.employee_long_term_care_krw
                AND NEW.employee_employment_insurance_krw
                    = OLD.employee_employment_insurance_krw
                AND NEW.employee_insurance_total_krw
                    = OLD.employee_insurance_total_krw
                AND NEW.withheld_income_tax_krw = OLD.withheld_income_tax_krw
                AND NEW.withheld_local_income_tax_krw
                    = OLD.withheld_local_income_tax_krw
                AND NEW.net_salary_pay_krw = OLD.net_salary_pay_krw
                AND NEW.income_event_count = OLD.income_event_count
                AND NEW.last_income_event_id <=> OLD.last_income_event_id
                AND NEW.payroll_count = OLD.payroll_count
                AND NEW.last_payroll_record_id <=> OLD.last_payroll_record_id
                AND YEAR(NEW.finalized_on) = OLD.tax_year + 1
                AND MONTH(NEW.finalized_on) = 1
                AND DAY(NEW.finalized_on) = 1
            )
        ),
    OLD.save_id,
    NULL
);

CREATE TABLE military_savings_contract (
    id                                      BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                                 BIGINT UNSIGNED NOT NULL,
    run_revision                            INT UNSIGNED NOT NULL,
    military_service_id                     BIGINT UNSIGNED NOT NULL,
    career_catalog_bundle_id                BIGINT UNSIGNED NOT NULL,
    employment_policy_set_id                BIGINT UNSIGNED NOT NULL,
    military_savings_policy_id              BIGINT UNSIGNED NOT NULL,
    military_savings_product_id             BIGINT UNSIGNED NOT NULL,
    military_savings_institution_id         BIGINT UNSIGNED NOT NULL,
    status                                  VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    opened_command_id                       CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    close_command_id                        CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    opened_game_day                         INT UNSIGNED NOT NULL,
    monthly_contribution_krw                BIGINT NOT NULL,
    debit_day_of_month                      TINYINT UNSIGNED NOT NULL,
    term_months                             SMALLINT UNSIGNED NOT NULL,
    fixed_rate_bp                           SMALLINT UNSIGNED NOT NULL,
    first_installment_game_day              INT UNSIGNED NOT NULL,
    maturity_game_day                       INT UNSIGNED NOT NULL,
    principal_krw                           BIGINT NOT NULL DEFAULT 0,
    paid_installment_count                  SMALLINT UNSIGNED NOT NULL DEFAULT 0,
    missed_installment_count                SMALLINT UNSIGNED NOT NULL DEFAULT 0,
    bank_interest_krw                       BIGINT NOT NULL DEFAULT 0,
    government_match_entitlement_krw        BIGINT NOT NULL DEFAULT 0,
    government_match_received_krw           BIGINT NOT NULL DEFAULT 0,
    maturity_ledger_transaction_id          BIGINT UNSIGNED NULL,
    closed_game_day                         INT UNSIGNED NULL,
    closure_kind                            VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NULL,
    created_at                              DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                              DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_military_savings_contract_run_id
        (save_id, run_revision, id),
    UNIQUE KEY uk_military_savings_contract_service_institution
        (save_id, run_revision, military_service_id,
         military_savings_institution_id),
    UNIQUE KEY uk_military_savings_contract_open_command
        (save_id, opened_command_id),
    UNIQUE KEY uk_military_savings_contract_close_command
        (save_id, close_command_id),
    UNIQUE KEY uk_military_savings_contract_maturity_ledger
        (save_id, run_revision, maturity_ledger_transaction_id),
    KEY ix_military_savings_contract_status
        (save_id, run_revision, status, id),
    CONSTRAINT fk_military_savings_contract_service
        FOREIGN KEY (save_id, run_revision, military_service_id)
        REFERENCES military_service (save_id, run_revision, id),
    CONSTRAINT fk_military_savings_contract_policy
        FOREIGN KEY (employment_policy_set_id, military_savings_policy_id)
        REFERENCES military_savings_policy (employment_policy_set_id, id),
    CONSTRAINT fk_military_savings_contract_product
        FOREIGN KEY (career_catalog_bundle_id, military_savings_product_id)
        REFERENCES military_savings_product_version (career_catalog_bundle_id, id),
    CONSTRAINT fk_military_savings_contract_institution
        FOREIGN KEY (career_catalog_bundle_id, military_savings_institution_id)
        REFERENCES military_savings_institution_catalog (career_catalog_bundle_id, id),
    CONSTRAINT fk_military_savings_contract_open_command
        FOREIGN KEY (save_id, opened_command_id)
        REFERENCES command_identity (save_id, command_id),
    CONSTRAINT fk_military_savings_contract_close_command
        FOREIGN KEY (save_id, close_command_id)
        REFERENCES command_identity (save_id, command_id),
    CONSTRAINT fk_military_savings_contract_maturity_ledger
        FOREIGN KEY (save_id, run_revision, maturity_ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT ck_military_savings_contract_status CHECK (
        status IN ('active', 'matured', 'closed')
    ),
    CONSTRAINT ck_military_savings_contract_terms CHECK (
        monthly_contribution_krw BETWEEN 1 AND 9007199254740991
        AND debit_day_of_month BETWEEN 1 AND 31
        AND term_months BETWEEN 1 AND 600
        AND fixed_rate_bp <= 10000
        AND opened_game_day < first_installment_game_day
        AND first_installment_game_day < maturity_game_day
    ),
    CONSTRAINT ck_military_savings_contract_amounts CHECK (
        principal_krw BETWEEN 0 AND 9007199254740991
        AND paid_installment_count <= term_months
        AND missed_installment_count <= term_months
        AND paid_installment_count + missed_installment_count <= term_months
        AND bank_interest_krw BETWEEN 0 AND 9007199254740991
        AND government_match_entitlement_krw BETWEEN 0 AND 9007199254740991
        AND government_match_received_krw
            BETWEEN 0 AND government_match_entitlement_krw
    ),
    CONSTRAINT ck_military_savings_contract_state CHECK (
        (
            status = 'active'
            AND close_command_id IS NULL
            AND maturity_ledger_transaction_id IS NULL
            AND closed_game_day IS NULL
            AND closure_kind IS NULL
            AND bank_interest_krw = 0
        )
        OR (
            status = 'matured'
            AND close_command_id IS NULL
            AND (
                (
                    principal_krw + bank_interest_krw = 0
                    AND maturity_ledger_transaction_id IS NULL
                )
                OR (
                    principal_krw + bank_interest_krw > 0
                    AND maturity_ledger_transaction_id IS NOT NULL
                )
            )
            AND closed_game_day = maturity_game_day
            AND closure_kind = 'maturity'
        )
        OR (
            status = 'closed'
            AND close_command_id IS NOT NULL
            AND (
                (
                    principal_krw + bank_interest_krw = 0
                    AND maturity_ledger_transaction_id IS NULL
                )
                OR (
                    principal_krw + bank_interest_krw > 0
                    AND maturity_ledger_transaction_id IS NOT NULL
                )
            )
            AND closed_game_day IS NOT NULL
            AND closed_game_day < maturity_game_day
            AND closure_kind = 'earlyClose'
            AND government_match_entitlement_krw = 0
            AND government_match_received_krw = 0
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE military_savings_installment (
    id                                      BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                                 BIGINT UNSIGNED NOT NULL,
    run_revision                            INT UNSIGNED NOT NULL,
    military_savings_contract_id            BIGINT UNSIGNED NOT NULL,
    installment_no                          SMALLINT UNSIGNED NOT NULL,
    due_game_day                            INT UNSIGNED NOT NULL,
    scheduled_settlement_id                 BIGINT UNSIGNED NOT NULL,
    status                                  VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    planned_principal_krw                   BIGINT NOT NULL,
    paid_principal_krw                      BIGINT NOT NULL DEFAULT 0,
    paid_game_day                           INT UNSIGNED NULL,
    no_movement_reason                      VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NULL,
    matching_policy_id                      BIGINT UNSIGNED NULL,
    matching_rate_ppm                       INT UNSIGNED NULL,
    government_match_krw                    BIGINT NOT NULL DEFAULT 0,
    ledger_transaction_id                   BIGINT UNSIGNED NULL,
    government_match_settlement_id          BIGINT UNSIGNED NULL,
    government_match_ledger_transaction_id  BIGINT UNSIGNED NULL,
    government_match_paid_game_day          INT UNSIGNED NULL,
    created_at                              DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                              DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_military_savings_installment_contract_no
        (military_savings_contract_id, installment_no),
    UNIQUE KEY uk_military_savings_installment_run_id
        (save_id, run_revision, id),
    UNIQUE KEY uk_military_savings_installment_settlement
        (save_id, run_revision, scheduled_settlement_id),
    UNIQUE KEY uk_military_savings_installment_ledger
        (save_id, run_revision, ledger_transaction_id),
    UNIQUE KEY uk_military_savings_installment_match_settlement
        (save_id, run_revision, government_match_settlement_id),
    UNIQUE KEY uk_military_savings_installment_match_ledger
        (save_id, run_revision, government_match_ledger_transaction_id),
    KEY ix_military_savings_installment_due
        (save_id, run_revision, status, due_game_day, id),
    CONSTRAINT fk_military_savings_installment_contract
        FOREIGN KEY (save_id, run_revision, military_savings_contract_id)
        REFERENCES military_savings_contract (save_id, run_revision, id),
    CONSTRAINT fk_military_savings_installment_settlement
        FOREIGN KEY (scheduled_settlement_id) REFERENCES scheduled_settlement (id),
    CONSTRAINT fk_military_savings_installment_policy
        FOREIGN KEY (matching_policy_id) REFERENCES military_savings_policy (id),
    CONSTRAINT fk_military_savings_installment_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT fk_military_savings_installment_match_settlement
        FOREIGN KEY (government_match_settlement_id) REFERENCES scheduled_settlement (id),
    CONSTRAINT fk_military_savings_installment_match_ledger
        FOREIGN KEY (save_id, run_revision, government_match_ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT ck_military_savings_installment_identity CHECK (
        installment_no BETWEEN 1 AND 600
        AND planned_principal_krw BETWEEN 1 AND 9007199254740991
        AND paid_principal_krw BETWEEN 0 AND planned_principal_krw
        AND status IN ('scheduled', 'paid', 'missed')
    ),
    CONSTRAINT ck_military_savings_installment_state CHECK (
        (
            status = 'scheduled'
            AND paid_principal_krw = 0
            AND paid_game_day IS NULL
            AND no_movement_reason IS NULL
            AND matching_policy_id IS NULL
            AND matching_rate_ppm IS NULL
            AND government_match_krw = 0
            AND ledger_transaction_id IS NULL
            AND government_match_settlement_id IS NULL
            AND government_match_ledger_transaction_id IS NULL
            AND government_match_paid_game_day IS NULL
        )
        OR (
            status = 'paid'
            AND paid_principal_krw = planned_principal_krw
            AND paid_game_day = due_game_day
            AND no_movement_reason IS NULL
            AND matching_policy_id IS NOT NULL
            AND matching_rate_ppm BETWEEN 1 AND 1000000
            AND government_match_krw >= 0
            AND ledger_transaction_id IS NOT NULL
            AND (
                (
                    government_match_settlement_id IS NULL
                    AND government_match_ledger_transaction_id IS NULL
                    AND government_match_paid_game_day IS NULL
                )
                OR (
                    government_match_settlement_id IS NOT NULL
                    AND (
                        (
                            government_match_ledger_transaction_id IS NULL
                            AND government_match_paid_game_day IS NULL
                        )
                        OR (
                            government_match_ledger_transaction_id IS NOT NULL
                            AND government_match_paid_game_day IS NOT NULL
                        )
                    )
                )
            )
        )
        OR (
            status = 'missed'
            AND paid_principal_krw = 0
            AND paid_game_day = due_game_day
            AND no_movement_reason = 'insufficientWalletCash'
            AND matching_policy_id IS NULL
            AND matching_rate_ppm IS NULL
            AND government_match_krw = 0
            AND ledger_transaction_id IS NULL
            AND government_match_settlement_id IS NULL
            AND government_match_ledger_transaction_id IS NULL
            AND government_match_paid_game_day IS NULL
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_military_savings_contract_valid_insert
BEFORE INSERT ON military_savings_contract
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'active'
        AND NEW.close_command_id IS NULL
        AND NEW.principal_krw = 0
        AND NEW.paid_installment_count = 0
        AND NEW.missed_installment_count = 0
        AND NEW.bank_interest_krw = 0
        AND NEW.government_match_entitlement_krw = 0
        AND NEW.government_match_received_krw = 0
        AND NEW.maturity_ledger_transaction_id IS NULL
        AND NEW.closed_game_day IS NULL
        AND NEW.closure_kind IS NULL
        AND EXISTS (
            SELECT 1
            FROM military_service AS service
            INNER JOIN save
                ON save.id = service.save_id
               AND save.run_revision = service.run_revision
            INNER JOIN market_world
                ON market_world.id = save.market_world_id
            INNER JOIN military_savings_policy AS savings_policy
                ON savings_policy.id = NEW.military_savings_policy_id
               AND savings_policy.employment_policy_set_id
                    = NEW.employment_policy_set_id
            INNER JOIN military_savings_policy_eligible_service AS eligible
                ON eligible.employment_policy_set_id
                    = savings_policy.employment_policy_set_id
               AND eligible.military_savings_policy_id = savings_policy.id
               AND BINARY eligible.service_type = BINARY service.service_type
            INNER JOIN military_savings_product_version AS product
                ON product.id = NEW.military_savings_product_id
               AND product.career_catalog_bundle_id
                    = NEW.career_catalog_bundle_id
               AND product.military_savings_institution_id
                    = NEW.military_savings_institution_id
            INNER JOIN military_savings_product_rate_band AS rate_band
                ON rate_band.career_catalog_bundle_id
                    = product.career_catalog_bundle_id
               AND rate_band.military_savings_product_id = product.id
               AND NEW.term_months >= rate_band.minimum_term_months
               AND NEW.term_months < rate_band.maximum_term_months_exclusive
               AND rate_band.fixed_rate_bp = NEW.fixed_rate_bp
            INNER JOIN command_identity AS identity
                ON identity.save_id = save.id
               AND BINARY identity.command_id = BINARY NEW.opened_command_id
               AND identity.command_kind = 'openMilitarySavings'
               AND identity.initial_run_revision = save.run_revision
               AND identity.initial_game_day = save.game_day
            WHERE service.id = NEW.military_service_id
              AND service.save_id = NEW.save_id
              AND service.run_revision = NEW.run_revision
              AND service.career_catalog_bundle_id
                    = NEW.career_catalog_bundle_id
              AND service.employment_policy_set_id
                    = NEW.employment_policy_set_id
              AND service.status = 'serving'
              AND NEW.opened_game_day = save.game_day
              AND NEW.maturity_game_day = service.end_game_day
              AND NEW.first_installment_game_day > save.game_day
              AND NEW.first_installment_game_day < service.end_game_day
              AND NEW.term_months BETWEEN product.minimum_term_months
                  AND product.maximum_term_months
              AND DATE_ADD(
                  market_world.start_date,
                  INTERVAL NEW.opened_game_day DAY
              ) BETWEEN savings_policy.effective_from AND savings_policy.join_through
              AND (
                  savings_policy.effective_to_exclusive IS NULL
                  OR DATE_ADD(
                      market_world.start_date,
                      INTERVAL NEW.opened_game_day DAY
                  ) < savings_policy.effective_to_exclusive
              )
              AND DATE_ADD(
                  market_world.start_date,
                  INTERVAL NEW.opened_game_day DAY
              ) >= product.available_from
              AND (
                  product.available_to_exclusive IS NULL
                  OR DATE_ADD(
                      market_world.start_date,
                      INTERVAL NEW.opened_game_day DAY
                  ) < product.available_to_exclusive
              )
              AND NEW.monthly_contribution_krw
                    >= savings_policy.minimum_installment_krw
              AND MOD(
                  NEW.monthly_contribution_krw,
                  savings_policy.installment_unit_krw
              ) = 0
              AND MOD(
                  NEW.monthly_contribution_krw,
                  savings_policy.limit_setting_unit_krw
              ) = 0
              AND NEW.monthly_contribution_krw
                    <= savings_policy.institution_monthly_limit_krw
              AND (
                  SELECT COUNT(*)
                  FROM military_savings_contract AS existing
                  WHERE existing.save_id = NEW.save_id
                    AND existing.run_revision = NEW.run_revision
                    AND existing.military_service_id = NEW.military_service_id
                    AND existing.status = 'active'
              ) < savings_policy.max_contracts_per_service
              AND CAST(NEW.monthly_contribution_krw AS DECIMAL(65, 0))
                    + COALESCE((
                        SELECT SUM(existing.monthly_contribution_krw)
                        FROM military_savings_contract AS existing
                        WHERE existing.save_id = NEW.save_id
                          AND existing.run_revision = NEW.run_revision
                          AND existing.military_service_id
                                = NEW.military_service_id
                          AND existing.status = 'active'
                    ), 0) <= savings_policy.person_monthly_limit_krw
              AND DATE_ADD(
                  DATE_ADD(
                      market_world.start_date,
                      INTERVAL NEW.opened_game_day DAY
                  ),
                  INTERVAL savings_policy.minimum_remaining_service_months MONTH
              ) <= service.end_exclusive_date
              AND NEW.first_installment_game_day = DATEDIFF(
                  TIMESTAMPADD(
                      DAY,
                      LEAST(
                          NEW.debit_day_of_month,
                          DAY(LAST_DAY(TIMESTAMPADD(
                              MONTH,
                              IF(
                                  TIMESTAMPADD(
                                      DAY,
                                      LEAST(
                                          NEW.debit_day_of_month,
                                          DAY(LAST_DAY(DATE_ADD(
                                              market_world.start_date,
                                              INTERVAL NEW.opened_game_day DAY
                                          )))
                                      ) - 1,
                                      TIMESTAMPADD(
                                          DAY,
                                          1 - DAY(DATE_ADD(
                                              market_world.start_date,
                                              INTERVAL NEW.opened_game_day DAY
                                          )),
                                          DATE_ADD(
                                              market_world.start_date,
                                              INTERVAL NEW.opened_game_day DAY
                                          )
                                      )
                                  ) <= DATE_ADD(
                                      market_world.start_date,
                                      INTERVAL NEW.opened_game_day DAY
                                  ),
                                  1,
                                  0
                              ),
                              TIMESTAMPADD(
                                  DAY,
                                  1 - DAY(DATE_ADD(
                                      market_world.start_date,
                                      INTERVAL NEW.opened_game_day DAY
                                  )),
                                  DATE_ADD(
                                      market_world.start_date,
                                      INTERVAL NEW.opened_game_day DAY
                                  )
                              )
                          )))
                      ) - 1,
                      TIMESTAMPADD(
                          MONTH,
                          IF(
                              TIMESTAMPADD(
                                  DAY,
                                  LEAST(
                                      NEW.debit_day_of_month,
                                      DAY(LAST_DAY(DATE_ADD(
                                          market_world.start_date,
                                          INTERVAL NEW.opened_game_day DAY
                                      )))
                                  ) - 1,
                                  TIMESTAMPADD(
                                      DAY,
                                      1 - DAY(DATE_ADD(
                                          market_world.start_date,
                                          INTERVAL NEW.opened_game_day DAY
                                      )),
                                      DATE_ADD(
                                          market_world.start_date,
                                          INTERVAL NEW.opened_game_day DAY
                                      )
                                  )
                              ) <= DATE_ADD(
                                  market_world.start_date,
                                  INTERVAL NEW.opened_game_day DAY
                              ),
                              1,
                              0
                          ),
                          TIMESTAMPADD(
                              DAY,
                              1 - DAY(DATE_ADD(
                                  market_world.start_date,
                                  INTERVAL NEW.opened_game_day DAY
                              )),
                              DATE_ADD(
                                  market_world.start_date,
                                  INTERVAL NEW.opened_game_day DAY
                              )
                          )
                      )
                  ),
                  market_world.start_date
              )
              AND TIMESTAMPADD(
                  DAY,
                  LEAST(
                      NEW.debit_day_of_month,
                      DAY(LAST_DAY(TIMESTAMPADD(
                          MONTH,
                          NEW.term_months - 1,
                          TIMESTAMPADD(
                              DAY,
                              1 - DAY(DATE_ADD(
                                  market_world.start_date,
                                  INTERVAL NEW.first_installment_game_day DAY
                              )),
                              DATE_ADD(
                                  market_world.start_date,
                                  INTERVAL NEW.first_installment_game_day DAY
                              )
                          )
                      )))
                  ) - 1,
                  TIMESTAMPADD(
                      MONTH,
                      NEW.term_months - 1,
                      TIMESTAMPADD(
                          DAY,
                          1 - DAY(DATE_ADD(
                              market_world.start_date,
                              INTERVAL NEW.first_installment_game_day DAY
                          )),
                          DATE_ADD(
                              market_world.start_date,
                              INTERVAL NEW.first_installment_game_day DAY
                          )
                      )
                  )
              ) < service.end_exclusive_date
              AND TIMESTAMPADD(
                  DAY,
                  LEAST(
                      NEW.debit_day_of_month,
                      DAY(LAST_DAY(TIMESTAMPADD(
                          MONTH,
                          NEW.term_months,
                          TIMESTAMPADD(
                              DAY,
                              1 - DAY(DATE_ADD(
                                  market_world.start_date,
                                  INTERVAL NEW.first_installment_game_day DAY
                              )),
                              DATE_ADD(
                                  market_world.start_date,
                                  INTERVAL NEW.first_installment_game_day DAY
                              )
                          )
                      )))
                  ) - 1,
                  TIMESTAMPADD(
                      MONTH,
                      NEW.term_months,
                      TIMESTAMPADD(
                          DAY,
                          1 - DAY(DATE_ADD(
                              market_world.start_date,
                              INTERVAL NEW.first_installment_game_day DAY
                          )),
                          DATE_ADD(
                              market_world.start_date,
                              INTERVAL NEW.first_installment_game_day DAY
                          )
                      )
                  )
              ) >= service.end_exclusive_date
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_military_savings_contract_transition
BEFORE UPDATE ON military_savings_contract
FOR EACH ROW
SET NEW.id = IF(
    NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.military_service_id = OLD.military_service_id
        AND NEW.career_catalog_bundle_id = OLD.career_catalog_bundle_id
        AND NEW.employment_policy_set_id = OLD.employment_policy_set_id
        AND NEW.military_savings_policy_id = OLD.military_savings_policy_id
        AND NEW.military_savings_product_id = OLD.military_savings_product_id
        AND NEW.military_savings_institution_id
            = OLD.military_savings_institution_id
        AND NEW.opened_command_id = OLD.opened_command_id
        AND NEW.opened_game_day = OLD.opened_game_day
        AND NEW.monthly_contribution_krw = OLD.monthly_contribution_krw
        AND NEW.debit_day_of_month = OLD.debit_day_of_month
        AND NEW.term_months = OLD.term_months
        AND NEW.fixed_rate_bp = OLD.fixed_rate_bp
        AND NEW.first_installment_game_day = OLD.first_installment_game_day
        AND NEW.maturity_game_day = OLD.maturity_game_day
        AND NEW.created_at = OLD.created_at
        AND (
            (
                OLD.status = 'active'
                AND NEW.status = 'active'
                AND NEW.close_command_id IS NULL
                AND NEW.principal_krw >= OLD.principal_krw
                AND NEW.paid_installment_count >= OLD.paid_installment_count
                AND NEW.missed_installment_count >= OLD.missed_installment_count
                AND NEW.paid_installment_count + NEW.missed_installment_count
                    = OLD.paid_installment_count + OLD.missed_installment_count + 1
                AND NEW.bank_interest_krw = 0
                AND NEW.government_match_entitlement_krw
                    >= OLD.government_match_entitlement_krw
                AND NEW.government_match_received_krw
                    = OLD.government_match_received_krw
                AND NEW.principal_krw = (
                    SELECT COALESCE(SUM(installment.paid_principal_krw), 0)
                    FROM military_savings_installment AS installment
                    WHERE installment.save_id = OLD.save_id
                      AND installment.run_revision = OLD.run_revision
                      AND installment.military_savings_contract_id = OLD.id
                )
                AND NEW.paid_installment_count = (
                    SELECT COUNT(*)
                    FROM military_savings_installment AS installment
                    WHERE installment.save_id = OLD.save_id
                      AND installment.run_revision = OLD.run_revision
                      AND installment.military_savings_contract_id = OLD.id
                      AND installment.status = 'paid'
                )
                AND NEW.missed_installment_count = (
                    SELECT COUNT(*)
                    FROM military_savings_installment AS installment
                    WHERE installment.save_id = OLD.save_id
                      AND installment.run_revision = OLD.run_revision
                      AND installment.military_savings_contract_id = OLD.id
                      AND installment.status = 'missed'
                )
                AND NEW.government_match_entitlement_krw = (
                    SELECT COALESCE(SUM(installment.government_match_krw), 0)
                    FROM military_savings_installment AS installment
                    WHERE installment.save_id = OLD.save_id
                      AND installment.run_revision = OLD.run_revision
                      AND installment.military_savings_contract_id = OLD.id
                      AND installment.status = 'paid'
                )
                AND NEW.maturity_ledger_transaction_id IS NULL
                AND NEW.closed_game_day IS NULL
                AND NEW.closure_kind IS NULL
            )
            OR (
                OLD.status = 'active'
                AND NEW.status = 'matured'
                AND NEW.principal_krw = OLD.principal_krw
                AND NEW.paid_installment_count = OLD.paid_installment_count
                AND NEW.missed_installment_count = OLD.missed_installment_count
                AND NEW.government_match_received_krw
                    = OLD.government_match_received_krw
                AND NEW.government_match_entitlement_krw
                    = OLD.government_match_entitlement_krw
                AND NEW.principal_krw = (
                    SELECT COALESCE(SUM(installment.paid_principal_krw), 0)
                    FROM military_savings_installment AS installment
                    WHERE installment.save_id = OLD.save_id
                      AND installment.run_revision = OLD.run_revision
                      AND installment.military_savings_contract_id = OLD.id
                )
                AND NEW.closed_game_day = (
                    SELECT game_day + 1 FROM save
                    WHERE id = OLD.save_id AND run_revision = OLD.run_revision
                )
                AND NEW.close_command_id IS NULL
                AND NEW.closed_game_day = OLD.maturity_game_day
                AND NEW.closure_kind = 'maturity'
                AND (
                    (
                        NEW.principal_krw + NEW.bank_interest_krw = 0
                        AND NEW.maturity_ledger_transaction_id IS NULL
                    )
                    OR (
                        NEW.principal_krw + NEW.bank_interest_krw > 0
                        AND NEW.maturity_ledger_transaction_id IS NOT NULL
                        AND EXISTS (
                            SELECT 1
                            FROM ledger_transaction AS ledger
                            INNER JOIN scheduled_settlement AS settlement
                                ON settlement.save_id = ledger.save_id
                               AND settlement.run_revision = ledger.run_revision
                               AND settlement.id = CAST(ledger.source_id AS UNSIGNED)
                               AND settlement.kind = 'militarySavingsMaturity'
                               AND settlement.source_kind = 'militarySavingsContract'
                               AND BINARY settlement.source_id
                                    = BINARY CAST(OLD.id AS CHAR)
                               AND settlement.occurrence = OLD.term_months + 1
                               AND settlement.due_game_day = OLD.maturity_game_day
                            INNER JOIN save
                                ON save.id = ledger.save_id
                               AND save.run_revision = ledger.run_revision
                               AND save.policy_set_id = ledger.policy_set_id
                            WHERE ledger.id = NEW.maturity_ledger_transaction_id
                              AND ledger.save_id = OLD.save_id
                              AND ledger.run_revision = OLD.run_revision
                              AND ledger.game_day = OLD.maturity_game_day
                              AND ledger.source_kind = 'militarySavingsMaturity'
                        )
                    )
                )
            )
            OR (
                OLD.status = 'active'
                AND NEW.status = 'closed'
                AND NEW.principal_krw = OLD.principal_krw
                AND NEW.paid_installment_count = OLD.paid_installment_count
                AND NEW.missed_installment_count = OLD.missed_installment_count
                AND NEW.government_match_entitlement_krw = 0
                AND NEW.government_match_received_krw = 0
                AND (
                    (
                        NEW.principal_krw + NEW.bank_interest_krw = 0
                        AND NEW.maturity_ledger_transaction_id IS NULL
                    )
                    OR (
                        NEW.principal_krw + NEW.bank_interest_krw > 0
                        AND NEW.maturity_ledger_transaction_id IS NOT NULL
                    )
                )
                AND NEW.closed_game_day = (
                    SELECT game_day FROM save
                    WHERE id = OLD.save_id AND run_revision = OLD.run_revision
                )
                AND NEW.closed_game_day < OLD.maturity_game_day
                AND NEW.closure_kind = 'earlyClose'
                AND EXISTS (
                    SELECT 1
                    FROM command_identity AS identity
                    WHERE identity.save_id = OLD.save_id
                      AND BINARY identity.command_id = BINARY NEW.close_command_id
                      AND identity.command_kind = 'closeMilitarySavings'
                      AND identity.initial_run_revision = OLD.run_revision
                      AND identity.initial_game_day = NEW.closed_game_day
                )
                AND (
                    NEW.maturity_ledger_transaction_id IS NULL
                    OR EXISTS (
                        SELECT 1
                        FROM ledger_transaction AS ledger
                        INNER JOIN save
                            ON save.id = ledger.save_id
                           AND save.run_revision = ledger.run_revision
                           AND save.policy_set_id = ledger.policy_set_id
                        WHERE ledger.id = NEW.maturity_ledger_transaction_id
                          AND ledger.save_id = OLD.save_id
                          AND ledger.run_revision = OLD.run_revision
                          AND ledger.game_day = NEW.closed_game_day
                          AND ledger.source_kind = 'militarySavingsEarlyClose'
                          AND BINARY ledger.source_id = BINARY CAST(OLD.id AS CHAR)
                    )
                )
            )
            OR (
                OLD.status = 'matured'
                AND NEW.status = 'matured'
                AND NEW.close_command_id IS NULL
                AND NEW.principal_krw = OLD.principal_krw
                AND NEW.paid_installment_count = OLD.paid_installment_count
                AND NEW.missed_installment_count = OLD.missed_installment_count
                AND NEW.bank_interest_krw = OLD.bank_interest_krw
                AND NEW.government_match_entitlement_krw
                    = OLD.government_match_entitlement_krw
                AND NEW.government_match_received_krw
                    >= OLD.government_match_received_krw
                AND NEW.government_match_received_krw = (
                    SELECT COALESCE(SUM(installment.government_match_krw), 0)
                    FROM military_savings_installment AS installment
                    WHERE installment.save_id = OLD.save_id
                      AND installment.run_revision = OLD.run_revision
                      AND installment.military_savings_contract_id = OLD.id
                      AND installment.government_match_ledger_transaction_id
                          IS NOT NULL
                )
                AND NEW.maturity_ledger_transaction_id
                    = OLD.maturity_ledger_transaction_id
                AND NEW.closed_game_day = OLD.closed_game_day
                AND NEW.closure_kind = OLD.closure_kind
            )
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_military_savings_contract_no_delete
BEFORE DELETE ON military_savings_contract
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military savings contracts cannot be deleted';

CREATE TRIGGER tr_military_savings_installment_valid_insert
BEFORE INSERT ON military_savings_installment
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'scheduled'
        AND NEW.paid_principal_krw = 0
        AND NEW.paid_game_day IS NULL
        AND NEW.no_movement_reason IS NULL
        AND NEW.matching_policy_id IS NULL
        AND NEW.matching_rate_ppm IS NULL
        AND NEW.government_match_krw = 0
        AND NEW.ledger_transaction_id IS NULL
        AND NEW.government_match_settlement_id IS NULL
        AND NEW.government_match_ledger_transaction_id IS NULL
        AND NEW.government_match_paid_game_day IS NULL
        AND EXISTS (
            SELECT 1
            FROM military_savings_contract AS contract
            INNER JOIN save
                ON save.id = contract.save_id
               AND save.run_revision = contract.run_revision
            INNER JOIN market_world
                ON market_world.id = save.market_world_id
            INNER JOIN scheduled_settlement AS settlement
                ON settlement.id = NEW.scheduled_settlement_id
               AND settlement.save_id = contract.save_id
               AND settlement.run_revision = contract.run_revision
               AND settlement.kind = 'militarySavingsInstallment'
               AND settlement.source_kind = 'militarySavingsContract'
               AND BINARY settlement.source_id = BINARY CAST(contract.id AS CHAR)
               AND settlement.occurrence = NEW.installment_no
               AND settlement.due_game_day = NEW.due_game_day
               AND settlement.status = 'pending'
            WHERE contract.id = NEW.military_savings_contract_id
              AND contract.save_id = NEW.save_id
              AND contract.run_revision = NEW.run_revision
              AND contract.status = 'active'
              AND NEW.installment_no <= contract.term_months
              AND NEW.planned_principal_krw = contract.monthly_contribution_krw
              AND NEW.due_game_day < contract.maturity_game_day
              AND NEW.due_game_day = DATEDIFF(
                  DATE_ADD(
                      DATE_ADD(
                          DATE_ADD(
                              market_world.start_date,
                              INTERVAL contract.first_installment_game_day DAY
                          ),
                          INTERVAL 1 - DAY(DATE_ADD(
                              market_world.start_date,
                              INTERVAL contract.first_installment_game_day DAY
                          )) DAY
                      ),
                      INTERVAL NEW.installment_no - 1 MONTH
                  ) + INTERVAL LEAST(
                      contract.debit_day_of_month,
                      DAY(LAST_DAY(DATE_ADD(
                          DATE_ADD(
                              DATE_ADD(
                                  market_world.start_date,
                                  INTERVAL contract.first_installment_game_day DAY
                              ),
                              INTERVAL 1 - DAY(DATE_ADD(
                                  market_world.start_date,
                                  INTERVAL contract.first_installment_game_day DAY
                              )) DAY
                          ),
                          INTERVAL NEW.installment_no - 1 MONTH
                      )))
                  ) - 1 DAY,
                  market_world.start_date
              )
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_military_savings_installment_transition
BEFORE UPDATE ON military_savings_installment
FOR EACH ROW
SET NEW.id = IF(
    NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.military_savings_contract_id
            = OLD.military_savings_contract_id
        AND NEW.installment_no = OLD.installment_no
        AND NEW.due_game_day = OLD.due_game_day
        AND NEW.scheduled_settlement_id = OLD.scheduled_settlement_id
        AND NEW.planned_principal_krw = OLD.planned_principal_krw
        AND NEW.created_at = OLD.created_at
        AND (
            (
                OLD.status = 'scheduled'
                AND NEW.status IN ('paid', 'missed')
                AND NEW.paid_game_day = OLD.due_game_day
                AND OLD.due_game_day = (
                    SELECT game_day + 1 FROM save
                    WHERE id = OLD.save_id AND run_revision = OLD.run_revision
                )
                AND (
                    (
                        NEW.status = 'paid'
                        AND NEW.paid_principal_krw = OLD.planned_principal_krw
                        AND NEW.no_movement_reason IS NULL
                        AND NEW.matching_policy_id IS NOT NULL
                        AND EXISTS (
                            SELECT 1
                            FROM military_savings_contract AS contract
                            INNER JOIN military_savings_policy AS matching_policy
                                ON matching_policy.id = NEW.matching_policy_id
                               AND matching_policy.employment_policy_set_id
                                    = contract.employment_policy_set_id
                               AND matching_policy.government_match_rate_ppm
                                    = NEW.matching_rate_ppm
                            INNER JOIN save
                                ON save.id = contract.save_id
                               AND save.run_revision = contract.run_revision
                            INNER JOIN market_world
                                ON market_world.id = save.market_world_id
                            WHERE contract.id = OLD.military_savings_contract_id
                              AND contract.save_id = OLD.save_id
                              AND contract.run_revision = OLD.run_revision
                              AND DATE_ADD(
                                  market_world.start_date,
                                  INTERVAL OLD.due_game_day DAY
                              ) >= matching_policy.effective_from
                              AND (
                                  matching_policy.effective_to_exclusive IS NULL
                                  OR DATE_ADD(
                                      market_world.start_date,
                                      INTERVAL OLD.due_game_day DAY
                                  ) < matching_policy.effective_to_exclusive
                              )
                        )
                        AND NEW.government_match_krw = CAST(FLOOR(
                            CAST(NEW.paid_principal_krw AS DECIMAL(65, 0))
                                * NEW.matching_rate_ppm / 1000000
                        ) AS SIGNED)
                        AND NEW.ledger_transaction_id IS NOT NULL
                        AND EXISTS (
                            SELECT 1
                            FROM ledger_transaction AS ledger
                            INNER JOIN save
                                ON save.id = ledger.save_id
                               AND save.run_revision = ledger.run_revision
                               AND save.policy_set_id = ledger.policy_set_id
                            WHERE ledger.id = NEW.ledger_transaction_id
                              AND ledger.save_id = OLD.save_id
                              AND ledger.run_revision = OLD.run_revision
                              AND ledger.game_day = OLD.due_game_day
                              AND ledger.source_kind
                                  = 'militarySavingsInstallment'
                              AND BINARY ledger.source_id = BINARY CAST(
                                  OLD.scheduled_settlement_id AS CHAR
                              )
                        )
                    )
                    OR (
                        NEW.status = 'missed'
                        AND NEW.paid_principal_krw = 0
                        AND NEW.no_movement_reason = 'insufficientWalletCash'
                        AND NEW.matching_policy_id IS NULL
                        AND NEW.matching_rate_ppm IS NULL
                        AND NEW.government_match_krw = 0
                        AND NEW.ledger_transaction_id IS NULL
                    )
                )
            )
            OR (
                OLD.status = 'paid'
                AND NEW.status = 'paid'
                AND NEW.paid_principal_krw = OLD.paid_principal_krw
                AND NEW.paid_game_day = OLD.paid_game_day
                AND NEW.no_movement_reason IS NULL
                AND NEW.matching_policy_id = OLD.matching_policy_id
                AND NEW.matching_rate_ppm = OLD.matching_rate_ppm
                AND NEW.government_match_krw = OLD.government_match_krw
                AND NEW.ledger_transaction_id = OLD.ledger_transaction_id
                AND (
                    (
                        OLD.government_match_settlement_id IS NULL
                        AND NEW.government_match_settlement_id IS NOT NULL
                        AND NEW.government_match_ledger_transaction_id IS NULL
                        AND NEW.government_match_paid_game_day IS NULL
                        AND EXISTS (
                            SELECT 1
                            FROM scheduled_settlement AS settlement
                            INNER JOIN military_savings_contract AS contract
                                ON contract.id = OLD.military_savings_contract_id
                               AND contract.save_id = OLD.save_id
                               AND contract.run_revision = OLD.run_revision
                            INNER JOIN military_savings_policy AS contract_policy
                                ON contract_policy.id
                                    = contract.military_savings_policy_id
                               AND contract_policy.employment_policy_set_id
                                    = contract.employment_policy_set_id
                            INNER JOIN save
                                ON save.id = contract.save_id
                               AND save.run_revision = contract.run_revision
                            INNER JOIN market_world
                                ON market_world.id = save.market_world_id
                            WHERE settlement.id
                                    = NEW.government_match_settlement_id
                              AND settlement.save_id = OLD.save_id
                              AND settlement.run_revision = OLD.run_revision
                              AND settlement.kind
                                    = 'militarySavingsGovernmentMatch'
                              AND settlement.source_kind
                                    = 'militarySavingsInstallment'
                              AND BINARY settlement.source_id
                                    = BINARY CAST(OLD.id AS CHAR)
                              AND settlement.occurrence = 1
                              AND settlement.status = 'pending'
                              AND contract.status = 'matured'
                              AND settlement.due_game_day = DATEDIFF(
                                  TIMESTAMPADD(
                                      DAY,
                                      LEAST(
                                          contract_policy.government_match_next_month_day,
                                          DAY(LAST_DAY(TIMESTAMPADD(
                                              MONTH,
                                              1,
                                              TIMESTAMPADD(
                                                  DAY,
                                                  1 - DAY(DATE_ADD(
                                                      market_world.start_date,
                                                      INTERVAL contract.maturity_game_day DAY
                                                  )),
                                                  DATE_ADD(
                                                      market_world.start_date,
                                                      INTERVAL contract.maturity_game_day DAY
                                                  )
                                              )
                                          )))
                                      ) - 1,
                                      TIMESTAMPADD(
                                          MONTH,
                                          1,
                                          TIMESTAMPADD(
                                              DAY,
                                              1 - DAY(DATE_ADD(
                                                  market_world.start_date,
                                                  INTERVAL contract.maturity_game_day DAY
                                              )),
                                              DATE_ADD(
                                                  market_world.start_date,
                                                  INTERVAL contract.maturity_game_day DAY
                                              )
                                          )
                                      )
                                  ),
                                  market_world.start_date
                              )
                        )
                    )
                    OR (
                        OLD.government_match_settlement_id IS NOT NULL
                        AND NEW.government_match_settlement_id
                            = OLD.government_match_settlement_id
                        AND OLD.government_match_ledger_transaction_id IS NULL
                        AND NEW.government_match_ledger_transaction_id IS NOT NULL
                        AND NEW.government_match_paid_game_day IS NOT NULL
                        AND EXISTS (
                            SELECT 1
                            FROM scheduled_settlement AS settlement
                            INNER JOIN ledger_transaction AS ledger
                                ON ledger.save_id = settlement.save_id
                               AND ledger.run_revision = settlement.run_revision
                               AND ledger.id
                                    = NEW.government_match_ledger_transaction_id
                               AND ledger.game_day = settlement.due_game_day
                               AND ledger.source_kind
                                    = 'militarySavingsGovernmentMatch'
                               AND BINARY ledger.source_id
                                    = BINARY CAST(settlement.id AS CHAR)
                            INNER JOIN save
                                ON save.id = ledger.save_id
                               AND save.run_revision = ledger.run_revision
                               AND save.policy_set_id = ledger.policy_set_id
                            INNER JOIN military_savings_contract AS contract
                                ON contract.id = OLD.military_savings_contract_id
                               AND contract.save_id = OLD.save_id
                               AND contract.run_revision = OLD.run_revision
                            INNER JOIN military_savings_policy AS contract_policy
                                ON contract_policy.id
                                    = contract.military_savings_policy_id
                               AND contract_policy.employment_policy_set_id
                                    = contract.employment_policy_set_id
                            INNER JOIN market_world
                                ON market_world.id = save.market_world_id
                            WHERE settlement.id
                                    = OLD.government_match_settlement_id
                              AND settlement.save_id = OLD.save_id
                              AND settlement.run_revision = OLD.run_revision
                              AND settlement.kind
                                    = 'militarySavingsGovernmentMatch'
                              AND NEW.government_match_paid_game_day
                                    = settlement.due_game_day
                              AND contract.status = 'matured'
                              AND settlement.due_game_day = DATEDIFF(
                                  TIMESTAMPADD(
                                      DAY,
                                      LEAST(
                                          contract_policy.government_match_next_month_day,
                                          DAY(LAST_DAY(TIMESTAMPADD(
                                              MONTH,
                                              1,
                                              TIMESTAMPADD(
                                                  DAY,
                                                  1 - DAY(DATE_ADD(
                                                      market_world.start_date,
                                                      INTERVAL contract.maturity_game_day DAY
                                                  )),
                                                  DATE_ADD(
                                                      market_world.start_date,
                                                      INTERVAL contract.maturity_game_day DAY
                                                  )
                                              )
                                          )))
                                      ) - 1,
                                      TIMESTAMPADD(
                                          MONTH,
                                          1,
                                          TIMESTAMPADD(
                                              DAY,
                                              1 - DAY(DATE_ADD(
                                                  market_world.start_date,
                                                  INTERVAL contract.maturity_game_day DAY
                                              )),
                                              DATE_ADD(
                                                  market_world.start_date,
                                                  INTERVAL contract.maturity_game_day DAY
                                              )
                                          )
                                      )
                                  ),
                                  market_world.start_date
                              )
                        )
                    )
                )
            )
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_military_savings_installment_no_delete
BEFORE DELETE ON military_savings_installment
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military savings installments cannot be deleted';

-- Military lifecycle actions share the phase-10 queue without pretending to belong to a
-- recruitment ruleset. The exact nullable union prevents a military service id from leaking
-- into any pre-existing recruitment action.
DROP TRIGGER tr_career_scheduled_action_valid_insert;
DROP TRIGGER tr_career_scheduled_action_transition_only;

ALTER TABLE career_scheduled_action
    DROP FOREIGN KEY fk_career_scheduled_action_compatibility,
    DROP CHECK ck_career_scheduled_action_kind,
    DROP CHECK ck_career_scheduled_action_payload,
    MODIFY COLUMN recruitment_ruleset_id BIGINT UNSIGNED NULL,
    ADD COLUMN military_service_id BIGINT UNSIGNED NULL
        AFTER job_application_id,
    ADD KEY ix_career_scheduled_action_military_service
        (save_id, run_revision, career_catalog_bundle_id, military_service_id);

ALTER TABLE career_scheduled_action
    ADD CONSTRAINT fk_career_scheduled_action_compatibility
        FOREIGN KEY (career_catalog_bundle_id, recruitment_ruleset_id)
        REFERENCES career_recruitment_compatibility (
            career_catalog_bundle_id,
            recruitment_ruleset_id
        ),
    ADD CONSTRAINT fk_career_scheduled_action_military_service
        FOREIGN KEY (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            military_service_id
        ) REFERENCES military_service (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            id
        ),
    ADD CONSTRAINT ck_career_scheduled_action_kind CHECK (
        action_kind IN (
            'employmentStart', 'militaryServiceStart', 'militaryServiceCompletion',
            'documentReview', 'confirmationExpiry', 'interviewDecision',
            'offerExpiry', 'invitationGeneration'
        )
    ),
    ADD CONSTRAINT ck_career_scheduled_action_payload CHECK (
        payload_version = 1
        AND occurrence BETWEEN 1 AND 9007199254740991
        AND (
            (
                action_kind = 'employmentStart'
                AND phase_rank = 10
                AND source_kind = 'employmentStart'
                AND source_id = employment_contract_id
                AND occurrence = 1
                AND recruitment_ruleset_id IS NOT NULL
                AND employment_contract_id IS NOT NULL
                AND job_application_id IS NULL
                AND military_service_id IS NULL
                AND platform_catalog_id IS NULL
                AND platform_key IS NULL
                AND invitation_generation_game_day IS NULL
            )
            OR (
                action_kind IN ('militaryServiceStart', 'militaryServiceCompletion')
                AND phase_rank = 10
                AND source_kind = 'militaryService'
                AND source_id = military_service_id
                AND occurrence = CASE BINARY action_kind
                    WHEN BINARY 'militaryServiceStart' THEN 1
                    WHEN BINARY 'militaryServiceCompletion' THEN 2
                    ELSE 0
                END
                AND recruitment_ruleset_id IS NULL
                AND employment_contract_id IS NULL
                AND job_application_id IS NULL
                AND military_service_id IS NOT NULL
                AND platform_catalog_id IS NULL
                AND platform_key IS NULL
                AND invitation_generation_game_day IS NULL
            )
            OR (
                action_kind = 'documentReview'
                AND phase_rank = 20
                AND source_kind = 'documentReview'
                AND source_id = job_application_id
                AND occurrence = 1
                AND recruitment_ruleset_id IS NOT NULL
                AND employment_contract_id IS NULL
                AND job_application_id IS NOT NULL
                AND military_service_id IS NULL
                AND platform_catalog_id IS NULL
                AND platform_key IS NULL
                AND invitation_generation_game_day IS NULL
            )
            OR (
                action_kind = 'confirmationExpiry'
                AND phase_rank = 30
                AND source_kind = 'confirmationExpiry'
                AND source_id = job_application_id
                AND occurrence = 1
                AND recruitment_ruleset_id IS NOT NULL
                AND employment_contract_id IS NULL
                AND job_application_id IS NOT NULL
                AND military_service_id IS NULL
                AND platform_catalog_id IS NULL
                AND platform_key IS NULL
                AND invitation_generation_game_day IS NULL
            )
            OR (
                action_kind = 'interviewDecision'
                AND phase_rank = 40
                AND source_kind = 'interviewDecision'
                AND source_id = job_application_id
                AND occurrence = 1
                AND recruitment_ruleset_id IS NOT NULL
                AND employment_contract_id IS NULL
                AND job_application_id IS NOT NULL
                AND military_service_id IS NULL
                AND platform_catalog_id IS NULL
                AND platform_key IS NULL
                AND invitation_generation_game_day IS NULL
            )
            OR (
                action_kind = 'offerExpiry'
                AND phase_rank = 50
                AND source_kind = 'offerExpiry'
                AND source_id = job_application_id
                AND occurrence = 1
                AND recruitment_ruleset_id IS NOT NULL
                AND employment_contract_id IS NULL
                AND job_application_id IS NOT NULL
                AND military_service_id IS NULL
                AND platform_catalog_id IS NULL
                AND platform_key IS NULL
                AND invitation_generation_game_day IS NULL
            )
            OR (
                action_kind = 'invitationGeneration'
                AND phase_rank = 60
                AND source_kind = 'invitationGeneration'
                AND source_id = platform_catalog_id
                AND occurrence = invitation_generation_game_day
                AND recruitment_ruleset_id IS NOT NULL
                AND employment_contract_id IS NULL
                AND job_application_id IS NULL
                AND military_service_id IS NULL
                AND platform_catalog_id IS NOT NULL
                AND platform_key IS NOT NULL
                AND invitation_generation_game_day IS NOT NULL
                AND invitation_generation_game_day = due_game_day
            )
        )
    );

CREATE TRIGGER tr_career_scheduled_action_valid_insert
BEFORE INSERT ON career_scheduled_action
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'pending'
        AND NEW.completed_game_day IS NULL
        AND NEW.cancelled_game_day IS NULL
        AND EXISTS (
            SELECT 1
            FROM save
            INNER JOIN career_run
                ON career_run.save_id = save.id
               AND career_run.run_revision = save.run_revision
               AND career_run.career_catalog_bundle_id = NEW.career_catalog_bundle_id
            WHERE save.id = NEW.save_id
              AND save.run_revision = NEW.run_revision
              AND NEW.due_game_day >= save.game_day
        )
        AND (
            (
                NEW.action_kind = 'employmentStart'
                AND EXISTS (
                    SELECT 1 FROM employment_contract AS contract
                    WHERE contract.id = NEW.employment_contract_id
                      AND contract.save_id = NEW.save_id
                      AND contract.run_revision = NEW.run_revision
                      AND contract.career_catalog_bundle_id
                          = NEW.career_catalog_bundle_id
                      AND contract.recruitment_ruleset_id
                          = NEW.recruitment_ruleset_id
                      AND contract.status = 'pendingStart'
                      AND contract.start_game_day = NEW.due_game_day
                )
            )
            OR (
                NEW.action_kind IN ('militaryServiceStart', 'militaryServiceCompletion')
                AND EXISTS (
                    SELECT 1 FROM military_service AS service
                    WHERE service.id = NEW.military_service_id
                      AND service.save_id = NEW.save_id
                      AND service.run_revision = NEW.run_revision
                      AND service.career_catalog_bundle_id
                          = NEW.career_catalog_bundle_id
                      AND service.status IN ('pendingStart', 'serving')
                      AND NEW.due_game_day = CASE BINARY NEW.action_kind
                          WHEN BINARY 'militaryServiceStart' THEN service.start_game_day
                          WHEN BINARY 'militaryServiceCompletion' THEN service.end_game_day
                          ELSE NULL
                      END
                )
            )
            OR (
                NEW.action_kind = 'documentReview'
                AND EXISTS (
                    SELECT 1
                    FROM job_application AS application
                    INNER JOIN job_posting AS posting
                        ON posting.id = application.job_posting_id
                    WHERE application.id = NEW.job_application_id
                      AND application.save_id = NEW.save_id
                      AND application.run_revision = NEW.run_revision
                      AND application.career_catalog_bundle_id
                          = NEW.career_catalog_bundle_id
                      AND application.recruitment_ruleset_id
                          = NEW.recruitment_ruleset_id
                      AND application.status = 'submitted'
                      AND NEW.due_game_day = application.submitted_game_day
                          + posting.document_review_days
                )
            )
            OR (
                NEW.action_kind = 'confirmationExpiry'
                AND EXISTS (
                    SELECT 1 FROM job_application AS application
                    WHERE application.id = NEW.job_application_id
                      AND application.save_id = NEW.save_id
                      AND application.run_revision = NEW.run_revision
                      AND application.career_catalog_bundle_id
                          = NEW.career_catalog_bundle_id
                      AND application.recruitment_ruleset_id
                          = NEW.recruitment_ruleset_id
                      AND application.status = 'interviewAwaitingConfirmation'
                      AND application.confirmation_expires_exclusive_game_day
                          = NEW.due_game_day
                )
            )
            OR (
                NEW.action_kind = 'interviewDecision'
                AND EXISTS (
                    SELECT 1 FROM job_application AS application
                    WHERE application.id = NEW.job_application_id
                      AND application.save_id = NEW.save_id
                      AND application.run_revision = NEW.run_revision
                      AND application.career_catalog_bundle_id
                          = NEW.career_catalog_bundle_id
                      AND application.recruitment_ruleset_id
                          = NEW.recruitment_ruleset_id
                      AND application.status = 'interviewConfirmed'
                      AND application.interview_game_day = NEW.due_game_day
                )
            )
            OR (
                NEW.action_kind = 'offerExpiry'
                AND EXISTS (
                    SELECT 1
                    FROM job_application AS application
                    INNER JOIN job_offer AS offer
                        ON offer.save_id = application.save_id
                       AND offer.run_revision = application.run_revision
                       AND offer.job_application_id = application.id
                    WHERE application.id = NEW.job_application_id
                      AND application.save_id = NEW.save_id
                      AND application.run_revision = NEW.run_revision
                      AND application.career_catalog_bundle_id
                          = NEW.career_catalog_bundle_id
                      AND application.recruitment_ruleset_id
                          = NEW.recruitment_ruleset_id
                      AND application.status = 'offered'
                      AND offer.status = 'pending'
                      AND offer.expires_exclusive_game_day = NEW.due_game_day
                )
            )
            OR (
                NEW.action_kind = 'invitationGeneration'
                AND EXISTS (
                    SELECT 1
                    FROM platform_catalog AS platform
                    WHERE platform.id = NEW.platform_catalog_id
                      AND platform.career_catalog_bundle_id
                          = NEW.career_catalog_bundle_id
                      AND BINARY platform.platform_key = BINARY NEW.platform_key
                      AND platform.invitation_source IN ('resume', 'linkedinProfile')
                )
            )
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_career_scheduled_action_transition_only
BEFORE UPDATE ON career_scheduled_action
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'pending'
        AND NEW.status IN ('completed', 'cancelled')
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.career_catalog_bundle_id = OLD.career_catalog_bundle_id
        AND NEW.recruitment_ruleset_id <=> OLD.recruitment_ruleset_id
        AND BINARY NEW.action_kind = BINARY OLD.action_kind
        AND NEW.payload_version = OLD.payload_version
        AND NEW.phase_rank = OLD.phase_rank
        AND NEW.due_game_day = OLD.due_game_day
        AND BINARY NEW.source_kind = BINARY OLD.source_kind
        AND NEW.source_id = OLD.source_id
        AND NEW.occurrence = OLD.occurrence
        AND NEW.employment_contract_id <=> OLD.employment_contract_id
        AND NEW.job_application_id <=> OLD.job_application_id
        AND NEW.military_service_id <=> OLD.military_service_id
        AND NEW.platform_catalog_id <=> OLD.platform_catalog_id
        AND NEW.platform_key <=> OLD.platform_key
        AND NEW.invitation_generation_game_day
            <=> OLD.invitation_generation_game_day
        AND NEW.created_at = OLD.created_at
        AND (
            (
                NEW.status = 'completed'
                AND NEW.completed_game_day = OLD.due_game_day
                AND NEW.cancelled_game_day IS NULL
                AND OLD.due_game_day = (
                    SELECT game_day + 1
                    FROM save
                    WHERE id = OLD.save_id AND run_revision = OLD.run_revision
                )
                AND (
                    (
                        OLD.action_kind = 'employmentStart'
                        AND EXISTS (
                            SELECT 1
                            FROM employment_contract AS contract
                            WHERE contract.save_id = OLD.save_id
                              AND contract.run_revision = OLD.run_revision
                              AND contract.id = OLD.employment_contract_id
                              AND contract.status = 'active'
                              AND contract.last_credited_game_day = OLD.due_game_day
                        )
                    )
                    OR (
                        OLD.action_kind = 'militaryServiceStart'
                        AND EXISTS (
                            SELECT 1
                            FROM military_service AS service
                            WHERE service.save_id = OLD.save_id
                              AND service.run_revision = OLD.run_revision
                              AND service.id = OLD.military_service_id
                              AND service.status = 'serving'
                              AND service.start_game_day = OLD.due_game_day
                        )
                    )
                    OR (
                        OLD.action_kind = 'militaryServiceCompletion'
                        AND EXISTS (
                            SELECT 1
                            FROM military_service AS service
                            WHERE service.save_id = OLD.save_id
                              AND service.run_revision = OLD.run_revision
                              AND service.id = OLD.military_service_id
                              AND service.status = 'completed'
                              AND service.completed_game_day = OLD.due_game_day
                        )
                    )
                    OR (
                        OLD.action_kind = 'documentReview'
                        AND EXISTS (
                            SELECT 1
                            FROM job_application AS application
                            WHERE application.save_id = OLD.save_id
                              AND application.run_revision = OLD.run_revision
                              AND application.id = OLD.job_application_id
                              AND application.status IN (
                                  'documentRejected',
                                  'interviewAwaitingConfirmation'
                              )
                              AND application.document_decided_game_day
                                  = OLD.due_game_day
                        )
                    )
                    OR (
                        OLD.action_kind = 'confirmationExpiry'
                        AND EXISTS (
                            SELECT 1
                            FROM job_application AS application
                            WHERE application.save_id = OLD.save_id
                              AND application.run_revision = OLD.run_revision
                              AND application.id = OLD.job_application_id
                              AND application.status = 'withdrawn'
                              AND application.terminal_from_status
                                  = 'interviewAwaitingConfirmation'
                              AND application.terminal_game_day = OLD.due_game_day
                        )
                    )
                    OR (
                        OLD.action_kind = 'interviewDecision'
                        AND EXISTS (
                            SELECT 1
                            FROM job_application AS application
                            WHERE application.save_id = OLD.save_id
                              AND application.run_revision = OLD.run_revision
                              AND application.id = OLD.job_application_id
                              AND application.status IN ('interviewRejected', 'offered')
                              AND application.interview_decided_game_day
                                  = OLD.due_game_day
                        )
                    )
                    OR (
                        OLD.action_kind = 'offerExpiry'
                        AND EXISTS (
                            SELECT 1
                            FROM job_offer AS offer
                            INNER JOIN job_application AS application
                                ON application.save_id = offer.save_id
                               AND application.run_revision = offer.run_revision
                               AND application.id = offer.job_application_id
                            WHERE offer.save_id = OLD.save_id
                              AND offer.run_revision = OLD.run_revision
                              AND offer.job_application_id = OLD.job_application_id
                              AND offer.status = 'expired'
                              AND offer.decided_game_day = OLD.due_game_day
                              AND application.status = 'expired'
                        )
                    )
                    OR (
                        OLD.action_kind = 'invitationGeneration'
                        AND NOT EXISTS (
                            SELECT 1
                            FROM job_invitation AS invitation
                            WHERE invitation.save_id = OLD.save_id
                              AND invitation.run_revision = OLD.run_revision
                              AND invitation.status = 'open'
                              AND invitation.expires_exclusive_game_day
                                  <= OLD.due_game_day
                        )
                    )
                )
            )
            OR (
                NEW.status = 'cancelled'
                AND NEW.completed_game_day IS NULL
                AND NEW.cancelled_game_day IS NOT NULL
                AND NEW.cancelled_game_day = (
                    SELECT game_day
                    FROM save
                    WHERE id = OLD.save_id AND run_revision = OLD.run_revision
                )
                AND (
                    EXISTS (
                        SELECT 1
                        FROM command_identity AS identity
                        WHERE identity.save_id = OLD.save_id
                          AND identity.command_kind = 'startGame'
                          AND identity.initial_run_revision = OLD.run_revision
                          AND identity.initial_game_day = NEW.cancelled_game_day
                    )
                    OR (
                        OLD.action_kind = 'documentReview'
                        AND EXISTS (
                            SELECT 1
                            FROM job_application AS application
                            WHERE application.save_id = OLD.save_id
                              AND application.run_revision = OLD.run_revision
                              AND application.id = OLD.job_application_id
                              AND application.status IN ('withdrawn', 'closed')
                              AND application.terminal_game_day
                                  = NEW.cancelled_game_day
                        )
                    )
                    OR (
                        OLD.action_kind = 'confirmationExpiry'
                        AND EXISTS (
                            SELECT 1
                            FROM job_application AS application
                            WHERE application.save_id = OLD.save_id
                              AND application.run_revision = OLD.run_revision
                              AND application.id = OLD.job_application_id
                              AND (
                                  application.status = 'interviewConfirmed'
                                  OR (
                                      application.status IN ('withdrawn', 'closed')
                                      AND application.terminal_game_day
                                          = NEW.cancelled_game_day
                                  )
                              )
                        )
                    )
                    OR (
                        OLD.action_kind = 'interviewDecision'
                        AND EXISTS (
                            SELECT 1
                            FROM job_application AS application
                            WHERE application.save_id = OLD.save_id
                              AND application.run_revision = OLD.run_revision
                              AND application.id = OLD.job_application_id
                              AND application.status IN ('withdrawn', 'closed')
                              AND application.terminal_game_day
                                  = NEW.cancelled_game_day
                        )
                    )
                    OR (
                        OLD.action_kind = 'offerExpiry'
                        AND EXISTS (
                            SELECT 1
                            FROM job_offer AS offer
                            WHERE offer.save_id = OLD.save_id
                              AND offer.run_revision = OLD.run_revision
                              AND offer.job_application_id = OLD.job_application_id
                              AND offer.status IN ('accepted', 'declined', 'closed')
                              AND offer.decided_game_day = NEW.cancelled_game_day
                        )
                    )
                    OR (
                        OLD.action_kind = 'employmentStart'
                        AND EXISTS (
                            SELECT 1
                            FROM employment_contract AS contract
                            WHERE contract.save_id = OLD.save_id
                              AND contract.run_revision = OLD.run_revision
                              AND contract.id = OLD.employment_contract_id
                              AND contract.status = 'ended'
                        )
                    )
                )
            )
        ),
    OLD.id,
    NULL
);

INSERT INTO career_scheduled_action
    (
        save_id,
        run_revision,
        career_catalog_bundle_id,
        recruitment_ruleset_id,
        action_kind,
        payload_version,
        phase_rank,
        due_game_day,
        status,
        source_kind,
        source_id,
        occurrence,
        employment_contract_id,
        job_application_id,
        military_service_id,
        platform_catalog_id,
        platform_key,
        invitation_generation_game_day,
        completed_game_day,
        cancelled_game_day
    )
SELECT service.save_id, service.run_revision, service.career_catalog_bundle_id,
       NULL, 'militaryServiceStart', 1, 10, service.start_game_day,
       'pending', 'militaryService', service.id, 1,
       NULL, NULL, service.id, NULL, NULL, NULL, NULL, NULL
FROM military_service AS service
WHERE service.status = 'pendingStart'
  AND NOT EXISTS (
      SELECT 1 FROM career_scheduled_action AS existing
      WHERE existing.save_id = service.save_id
        AND existing.run_revision = service.run_revision
        AND existing.source_kind = 'militaryService'
        AND existing.source_id = service.id
        AND existing.occurrence = 1
  );

INSERT INTO career_scheduled_action
    (
        save_id,
        run_revision,
        career_catalog_bundle_id,
        recruitment_ruleset_id,
        action_kind,
        payload_version,
        phase_rank,
        due_game_day,
        status,
        source_kind,
        source_id,
        occurrence,
        employment_contract_id,
        job_application_id,
        military_service_id,
        platform_catalog_id,
        platform_key,
        invitation_generation_game_day,
        completed_game_day,
        cancelled_game_day
    )
SELECT service.save_id, service.run_revision, service.career_catalog_bundle_id,
       NULL, 'militaryServiceCompletion', 1, 10, service.end_game_day,
       'pending', 'militaryService', service.id, 2,
       NULL, NULL, service.id, NULL, NULL, NULL, NULL, NULL
FROM military_service AS service
WHERE service.status IN ('pendingStart', 'serving')
  AND NOT EXISTS (
      SELECT 1 FROM career_scheduled_action AS existing
      WHERE existing.save_id = service.save_id
        AND existing.run_revision = service.run_revision
        AND existing.source_kind = 'militaryService'
        AND existing.source_id = service.id
        AND existing.occurrence = 2
  );

-- Each career-crediting option/family pair owns a stackable catalog entry. Staged writes are
-- limited to the checksum-guarded development bundle; future bundles use the normal draft path.
CREATE TABLE military_option_experience_evidence_mapping (
    career_catalog_bundle_id        BIGINT UNSIGNED NOT NULL,
    military_option_version_id      BIGINT UNSIGNED NOT NULL,
    career_job_family_id            BIGINT UNSIGNED NOT NULL,
    spec_catalog_entry_id           BIGINT UNSIGNED NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (
        career_catalog_bundle_id,
        military_option_version_id,
        career_job_family_id
    ),
    UNIQUE KEY uk_military_experience_evidence_entry
        (career_catalog_bundle_id, spec_catalog_entry_id),
    CONSTRAINT fk_military_experience_evidence_option_family
        FOREIGN KEY (
            career_catalog_bundle_id,
            military_option_version_id,
            career_job_family_id
        ) REFERENCES military_option_job_family (
            career_catalog_bundle_id,
            military_option_version_id,
            career_job_family_id
        ),
    CONSTRAINT fk_military_experience_evidence_catalog
        FOREIGN KEY (career_catalog_bundle_id, spec_catalog_entry_id)
        REFERENCES spec_catalog_entry (career_catalog_bundle_id, id)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

DROP TRIGGER tr_spec_catalog_entry_draft_insert;
DROP TRIGGER tr_spec_catalog_contribution_draft_insert;

INSERT INTO spec_catalog_entry
    (
        career_catalog_bundle_id,
        entry_key,
        kind,
        display_name,
        stackable,
        validity_days
    )
SELECT bundle.id,
       CONCAT('military-experience-', option_row.option_key, '-', family.job_family_key),
       'experience',
       CONCAT(option_row.display_name, ' · ', family.display_name, ' 인정 경력'),
       TRUE,
       NULL
FROM career_catalog_bundle AS bundle
INNER JOIN military_option_job_family AS option_family
    ON option_family.career_catalog_bundle_id = bundle.id
INNER JOIN military_option_version AS option_row
    ON option_row.career_catalog_bundle_id = option_family.career_catalog_bundle_id
   AND option_row.id = option_family.military_option_version_id
INNER JOIN career_job_family AS family
    ON family.career_catalog_bundle_id = option_family.career_catalog_bundle_id
   AND family.id = option_family.career_job_family_id
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1'
  AND bundle.ranked_eligible = FALSE
  AND bundle.published_at IS NOT NULL;

INSERT INTO spec_catalog_contribution
    (
        career_catalog_bundle_id,
        spec_catalog_entry_id,
        career_job_family_id,
        contribution_bp
    )
SELECT option_family.career_catalog_bundle_id,
       entry.id,
       option_family.career_job_family_id,
       FLOOR(300 * option_family.experience_credit_ppm / 1000000)
FROM military_option_job_family AS option_family
INNER JOIN career_catalog_bundle AS bundle
    ON bundle.id = option_family.career_catalog_bundle_id
   AND BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1'
   AND bundle.ranked_eligible = FALSE
   AND bundle.published_at IS NOT NULL
INNER JOIN military_option_version AS option_row
    ON option_row.career_catalog_bundle_id = option_family.career_catalog_bundle_id
   AND option_row.id = option_family.military_option_version_id
INNER JOIN career_job_family AS family
    ON family.career_catalog_bundle_id = option_family.career_catalog_bundle_id
   AND family.id = option_family.career_job_family_id
INNER JOIN spec_catalog_entry AS entry
    ON entry.career_catalog_bundle_id = option_family.career_catalog_bundle_id
   AND BINARY entry.entry_key = BINARY CONCAT(
       'military-experience-', option_row.option_key, '-', family.job_family_key
   )
WHERE FLOOR(300 * option_family.experience_credit_ppm / 1000000) > 0;

INSERT INTO military_option_experience_evidence_mapping
    (
        career_catalog_bundle_id,
        military_option_version_id,
        career_job_family_id,
        spec_catalog_entry_id
    )
SELECT option_family.career_catalog_bundle_id,
       option_family.military_option_version_id,
       option_family.career_job_family_id,
       entry.id
FROM military_option_job_family AS option_family
INNER JOIN career_catalog_bundle AS bundle
    ON bundle.id = option_family.career_catalog_bundle_id
   AND BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1'
   AND bundle.ranked_eligible = FALSE
   AND bundle.published_at IS NOT NULL
INNER JOIN military_option_version AS option_row
    ON option_row.career_catalog_bundle_id = option_family.career_catalog_bundle_id
   AND option_row.id = option_family.military_option_version_id
INNER JOIN career_job_family AS family
    ON family.career_catalog_bundle_id = option_family.career_catalog_bundle_id
   AND family.id = option_family.career_job_family_id
INNER JOIN spec_catalog_entry AS entry
    ON entry.career_catalog_bundle_id = option_family.career_catalog_bundle_id
   AND BINARY entry.entry_key = BINARY CONCAT(
       'military-experience-', option_row.option_key, '-', family.job_family_key
   );

CREATE TRIGGER tr_spec_catalog_entry_draft_insert
BEFORE INSERT ON spec_catalog_entry
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle
        WHERE id = NEW.career_catalog_bundle_id AND published_at IS NULL
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_spec_catalog_contribution_draft_insert
BEFORE INSERT ON spec_catalog_contribution
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle
        WHERE id = NEW.career_catalog_bundle_id AND published_at IS NULL
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_military_experience_evidence_draft_insert
BEFORE INSERT ON military_option_experience_evidence_mapping
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1
        FROM career_catalog_bundle AS bundle
        INNER JOIN military_option_job_family AS option_family
            ON option_family.career_catalog_bundle_id = bundle.id
           AND option_family.military_option_version_id
                = NEW.military_option_version_id
           AND option_family.career_job_family_id = NEW.career_job_family_id
        INNER JOIN spec_catalog_entry AS entry
            ON entry.career_catalog_bundle_id = bundle.id
           AND entry.id = NEW.spec_catalog_entry_id
           AND entry.kind = 'experience'
           AND entry.stackable = TRUE
           AND entry.validity_days IS NULL
        INNER JOIN spec_catalog_contribution AS contribution
            ON contribution.career_catalog_bundle_id = bundle.id
           AND contribution.spec_catalog_entry_id = entry.id
           AND contribution.career_job_family_id = NEW.career_job_family_id
           AND contribution.contribution_bp = FLOOR(
               300 * option_family.experience_credit_ppm / 1000000
           )
           AND contribution.contribution_bp > 0
        WHERE bundle.id = NEW.career_catalog_bundle_id
          AND bundle.published_at IS NULL
          AND NOT EXISTS (
              SELECT 1
              FROM spec_catalog_contribution AS other_contribution
              WHERE other_contribution.career_catalog_bundle_id = bundle.id
                AND other_contribution.spec_catalog_entry_id = entry.id
                AND other_contribution.career_job_family_id
                    <> NEW.career_job_family_id
          )
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_military_experience_evidence_no_update
BEFORE UPDATE ON military_option_experience_evidence_mapping
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military experience evidence mappings are immutable';

CREATE TRIGGER tr_military_experience_evidence_no_delete
BEFORE DELETE ON military_option_experience_evidence_mapping
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military experience evidence mappings are immutable';

-- Experience qualification uses credited days, while the period remains the actual history shown
-- to the player. This separates partial military credit from calendar tenure.
DROP TRIGGER tr_spec_evidence_valid_insert;

ALTER TABLE spec_evidence
    ADD KEY ix_spec_evidence_source_military
        (save_id, run_revision, career_catalog_bundle_id, source_military_service_id),
    ADD CONSTRAINT fk_spec_evidence_source_military
        FOREIGN KEY (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            source_military_service_id
        ) REFERENCES military_service (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            id
        )
    ;

CREATE TRIGGER tr_spec_evidence_valid_insert
BEFORE INSERT ON spec_evidence
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM save
        INNER JOIN career_run
            ON career_run.save_id = save.id
           AND career_run.run_revision = save.run_revision
           AND career_run.career_catalog_bundle_id = NEW.career_catalog_bundle_id
        WHERE save.id = NEW.save_id
          AND save.run_revision = NEW.run_revision
    )
        AND EXISTS (
            SELECT 1
            FROM spec_catalog_entry AS catalog
            WHERE catalog.career_catalog_bundle_id = NEW.career_catalog_bundle_id
              AND catalog.id = NEW.spec_catalog_entry_id
              AND BINARY catalog.kind = BINARY NEW.kind
              AND (
                  (catalog.validity_days IS NULL AND NEW.expires_on_game_day IS NULL)
                  OR NEW.expires_on_game_day
                      = NEW.acquired_game_day + catalog.validity_days
              )
        )
        AND (
            (
                NEW.source_kind = 'bridgeEducation'
                AND NEW.acquired_game_day = 0
                AND NEW.period_start_date IS NULL
                AND NEW.period_end_exclusive_date IS NULL
                AND EXISTS (
                    SELECT 1
                    FROM career_bridge_education_mapping AS bridge
                    INNER JOIN `character`
                        ON `character`.save_id = NEW.save_id
                       AND BINARY `character`.education = BINARY bridge.education
                    WHERE bridge.career_catalog_bundle_id = NEW.career_catalog_bundle_id
                      AND bridge.spec_catalog_entry_id = NEW.spec_catalog_entry_id
                      AND BINARY bridge.evidence_key = BINARY NEW.evidence_key
                )
            )
            OR (
                NEW.source_kind = 'bridgeCertification'
                AND NEW.acquired_game_day = 0
                AND NEW.period_start_date IS NULL
                AND NEW.period_end_exclusive_date IS NULL
                AND EXISTS (
                    SELECT 1
                    FROM career_bridge_certification_order AS bridge
                    INNER JOIN `character`
                        ON `character`.save_id = NEW.save_id
                       AND bridge.certification_order <= `character`.certifications
                    WHERE bridge.career_catalog_bundle_id = NEW.career_catalog_bundle_id
                      AND bridge.spec_catalog_entry_id = NEW.spec_catalog_entry_id
                      AND BINARY bridge.evidence_key = BINARY NEW.evidence_key
                )
            )
            OR (
                NEW.source_kind = 'bridgeExperience'
                AND NEW.acquired_game_day = 0
                AND EXISTS (
                    SELECT 1
                    FROM career_bridge_experience_mapping AS bridge
                    INNER JOIN `character`
                        ON `character`.save_id = NEW.save_id
                       AND bridge.career_years = `character`.career_years
                    INNER JOIN save ON save.id = NEW.save_id
                    INNER JOIN market_world ON market_world.id = save.market_world_id
                    WHERE bridge.career_catalog_bundle_id = NEW.career_catalog_bundle_id
                      AND bridge.spec_catalog_entry_id = NEW.spec_catalog_entry_id
                      AND BINARY bridge.evidence_key = BINARY NEW.evidence_key
                      AND NEW.period_start_date = DATE_SUB(
                          market_world.start_date,
                          INTERVAL bridge.career_years YEAR
                      )
                      AND NEW.period_end_exclusive_date = market_world.start_date
                      AND NEW.credited_experience_days = DATEDIFF(
                          NEW.period_end_exclusive_date,
                          NEW.period_start_date
                      )
                )
            )
            OR (
                NEW.source_kind = 'activity'
                AND EXISTS (
                    SELECT 1
                    FROM spec_activity AS activity
                    INNER JOIN activity_catalog_entry AS catalog
                        ON catalog.career_catalog_bundle_id = activity.career_catalog_bundle_id
                       AND catalog.id = activity.activity_catalog_entry_id
                    WHERE activity.save_id = NEW.save_id
                      AND activity.run_revision = NEW.run_revision
                      AND activity.career_catalog_bundle_id = NEW.career_catalog_bundle_id
                      AND activity.id = NEW.source_activity_id
                      AND activity.status = 'completed'
                      AND activity.completed_game_day = NEW.acquired_game_day
                      AND catalog.evidence_catalog_entry_id = NEW.spec_catalog_entry_id
                )
            )
            OR (
                NEW.source_kind = 'employmentContract'
                AND EXISTS (
                    SELECT 1
                    FROM employment_contract AS contract
                    INNER JOIN save
                        ON save.id = contract.save_id
                       AND save.run_revision = contract.run_revision
                    INNER JOIN market_world
                        ON market_world.id = save.market_world_id
                    WHERE contract.id = NEW.source_employment_contract_id
                      AND contract.save_id = NEW.save_id
                      AND contract.run_revision = NEW.run_revision
                      AND contract.career_catalog_bundle_id
                          = NEW.career_catalog_bundle_id
                      AND contract.status = 'ended'
                      AND NEW.kind = 'experience'
                      AND NEW.acquired_game_day = contract.end_game_day
                      AND NEW.period_start_date = DATE_ADD(
                          market_world.start_date,
                          INTERVAL contract.start_game_day DAY
                      )
                      AND NEW.period_end_exclusive_date = DATE_ADD(
                          market_world.start_date,
                          INTERVAL contract.end_game_day DAY
                      )
                      AND NEW.credited_experience_days
                          = contract.credited_experience_days
                )
            )
            OR (
                NEW.source_kind = 'militaryService'
                AND EXISTS (
                    SELECT 1
                    FROM military_service AS service
                    INNER JOIN military_option_experience_evidence_mapping AS mapping
                        ON mapping.career_catalog_bundle_id
                            = service.career_catalog_bundle_id
                       AND mapping.military_option_version_id
                            = service.military_option_version_id
                       AND mapping.spec_catalog_entry_id
                            = NEW.spec_catalog_entry_id
                    INNER JOIN military_service_progress AS progress
                        ON progress.save_id = service.save_id
                       AND progress.run_revision = service.run_revision
                       AND progress.career_catalog_bundle_id
                            = service.career_catalog_bundle_id
                       AND progress.military_service_id = service.id
                       AND progress.military_option_version_id
                            = service.military_option_version_id
                       AND progress.career_job_family_id
                            = mapping.career_job_family_id
                       AND progress.status = 'finalized'
                    WHERE service.id = NEW.source_military_service_id
                      AND service.save_id = NEW.save_id
                      AND service.run_revision = NEW.run_revision
                      AND service.career_catalog_bundle_id
                          = NEW.career_catalog_bundle_id
                      AND service.status = 'completed'
                      AND NEW.kind = 'experience'
                      AND BINARY NEW.evidence_key = BINARY CONCAT(
                          'militaryService:',
                          service.id,
                          ':',
                          mapping.career_job_family_id
                      )
                      AND NEW.acquired_game_day = service.end_game_day
                      AND NEW.period_start_date = service.start_date
                      AND NEW.period_end_exclusive_date = service.end_exclusive_date
                      AND NEW.credited_experience_days = FLOOR(
                          progress.credited_experience_day_ppm / 1000000
                      )
                )
            )
        ),
    NEW.save_id,
    NULL
);

-- Recruitment eligibility follows the run-scoped military lifecycle. The immutable character
-- value remains useful for legacy display only and cannot represent a user-started service.
DROP TRIGGER tr_job_application_valid_insert;

CREATE TRIGGER tr_job_application_valid_insert
BEFORE INSERT ON job_application
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM save
        INNER JOIN `character` ON `character`.save_id = save.id
        INNER JOIN career_run
            ON career_run.save_id = save.id
           AND career_run.run_revision = save.run_revision
           AND career_run.career_catalog_bundle_id = NEW.career_catalog_bundle_id
        INNER JOIN job_posting AS posting
            ON posting.career_catalog_bundle_id = career_run.career_catalog_bundle_id
           AND posting.id = NEW.job_posting_id
           AND posting.recruitment_ruleset_id = NEW.recruitment_ruleset_id
           AND posting.market_world_id = save.market_world_id
        INNER JOIN platform_catalog AS platform
            ON platform.career_catalog_bundle_id = posting.career_catalog_bundle_id
           AND platform.id = posting.platform_catalog_id
        INNER JOIN recruitment_ruleset AS ruleset
            ON ruleset.id = posting.recruitment_ruleset_id
           AND ruleset.published_at IS NOT NULL
        WHERE save.id = NEW.save_id
          AND save.run_revision = NEW.run_revision
          AND save.game_day = NEW.submitted_game_day
          AND save.game_day >= posting.posted_game_day
          AND save.game_day < posting.closes_exclusive_game_day
          AND career_run.military_status <> 'serving'
          AND (
              posting.military_requirement = 'none'
              OR career_run.military_status IN ('completed', 'exempt')
          )
          AND (
              platform.same_region_only = FALSE
              OR BINARY `character`.region = BINARY posting.region
          )
          AND (
              posting.minimum_education IS NULL
              OR CASE `character`.education
                          WHEN 'highSchool' THEN 1
                          WHEN 'associate' THEN 2
                          WHEN 'bachelor' THEN 3
                          WHEN 'master' THEN 4
                          WHEN 'doctorate' THEN 5
                          ELSE 0
              END >= CASE posting.minimum_education
                          WHEN 'highSchool' THEN 1
                          WHEN 'associate' THEN 2
                          WHEN 'bachelor' THEN 3
                          WHEN 'master' THEN 4
                          WHEN 'doctorate' THEN 5
                          ELSE 6
              END
          )
          AND (
              posting.required_certification_entry_id IS NULL
              OR EXISTS (
                          SELECT 1
                          FROM spec_evidence AS certification
                          WHERE certification.save_id = NEW.save_id
                            AND certification.run_revision = NEW.run_revision
                            AND certification.career_catalog_bundle_id
                                = NEW.career_catalog_bundle_id
                            AND certification.kind = 'certification'
                            AND certification.spec_catalog_entry_id
                                = posting.required_certification_entry_id
                            AND certification.acquired_game_day
                                <= NEW.submitted_game_day
                            AND (
                                certification.expires_on_game_day IS NULL
                                OR certification.expires_on_game_day
                                    >= NEW.submitted_game_day
                            )
              )
          )
          AND posting.minimum_experience_days <= (
              SELECT COALESCE(SUM(
                          CASE
                              WHEN experience.kind = 'experience'
                                AND experience.period_start_date IS NOT NULL
                                AND experience.period_end_exclusive_date IS NOT NULL
                              THEN DATEDIFF(
                                  experience.period_end_exclusive_date,
                                  experience.period_start_date
                              )
                              ELSE 0
                          END
              ), 0)
              FROM spec_evidence AS experience
              WHERE experience.save_id = NEW.save_id
                AND experience.run_revision = NEW.run_revision
                AND experience.career_catalog_bundle_id
                    = NEW.career_catalog_bundle_id
                AND experience.acquired_game_day <= NEW.submitted_game_day
                AND (
                    experience.expires_on_game_day IS NULL
                    OR experience.expires_on_game_day >= NEW.submitted_game_day
                )
          )
          AND NEW.possessed_education_score_bp IS NULL
          AND NEW.interview_decided_game_day IS NULL
          AND NOT EXISTS (
              SELECT 1
              FROM employment_contract AS contract
              WHERE contract.save_id = NEW.save_id
                AND contract.run_revision = NEW.run_revision
                AND contract.status IN ('pendingStart', 'active')
          )
          AND (
              SELECT COUNT(*)
              FROM job_application AS active_application
              WHERE active_application.save_id = NEW.save_id
                AND active_application.run_revision = NEW.run_revision
                AND active_application.status IN (
                    'submitted', 'interviewAwaitingConfirmation',
                    'interviewConfirmed', 'offered'
                )
          ) < ruleset.active_application_limit
          AND posting.requires_resume = (NEW.resume_version_id IS NOT NULL)
          AND posting.requires_portfolio = (NEW.portfolio_version_id IS NOT NULL)
          AND posting.requires_linkedin_profile
              = (NEW.linkedin_profile_version_id IS NOT NULL)
          AND (
              NEW.resume_version_id IS NULL
              OR EXISTS (
                  SELECT 1 FROM profile_artifact_version AS artifact
                  WHERE artifact.save_id = NEW.save_id
                    AND artifact.run_revision = NEW.run_revision
                    AND artifact.career_catalog_bundle_id = NEW.career_catalog_bundle_id
                    AND artifact.id = NEW.resume_version_id
                    AND artifact.artifact_kind = 'resume'
                    AND artifact.sealed_at IS NOT NULL
                    AND artifact.created_game_day <= NEW.submitted_game_day
              )
          )
          AND (
              NEW.portfolio_version_id IS NULL
              OR EXISTS (
                  SELECT 1 FROM profile_artifact_version AS artifact
                  WHERE artifact.save_id = NEW.save_id
                    AND artifact.run_revision = NEW.run_revision
                    AND artifact.career_catalog_bundle_id = NEW.career_catalog_bundle_id
                    AND artifact.id = NEW.portfolio_version_id
                    AND artifact.artifact_kind = 'portfolio'
                    AND artifact.sealed_at IS NOT NULL
                    AND artifact.created_game_day <= NEW.submitted_game_day
              )
          )
          AND (
              NEW.linkedin_profile_version_id IS NULL
              OR EXISTS (
                  SELECT 1 FROM profile_artifact_version AS artifact
                  WHERE artifact.save_id = NEW.save_id
                    AND artifact.run_revision = NEW.run_revision
                    AND artifact.career_catalog_bundle_id = NEW.career_catalog_bundle_id
                    AND artifact.id = NEW.linkedin_profile_version_id
                    AND artifact.artifact_kind = 'linkedinProfile'
                    AND artifact.sealed_at IS NOT NULL
                    AND artifact.created_game_day <= NEW.submitted_game_day
              )
          )
          AND NEW.artifact_completeness_bp = (
              (
                  COALESCE((
                      SELECT completeness_bp FROM profile_artifact_version
                      WHERE id = NEW.resume_version_id
                        AND save_id = NEW.save_id AND run_revision = NEW.run_revision
                  ), 0)
                  + COALESCE((
                      SELECT completeness_bp FROM profile_artifact_version
                      WHERE id = NEW.portfolio_version_id
                        AND save_id = NEW.save_id AND run_revision = NEW.run_revision
                  ), 0)
                  + COALESCE((
                      SELECT completeness_bp FROM profile_artifact_version
                      WHERE id = NEW.linkedin_profile_version_id
                        AND save_id = NEW.save_id AND run_revision = NEW.run_revision
                  ), 0)
              ) DIV (
                  (NEW.resume_version_id IS NOT NULL)
                  + (NEW.portfolio_version_id IS NOT NULL)
                  + (NEW.linkedin_profile_version_id IS NOT NULL)
              )
          )
          AND (
              (
                  NEW.source_kind = 'direct'
                  AND NEW.status = 'submitted'
                  AND (
                      SELECT COUNT(*)
                      FROM job_application AS daily_application
                      WHERE daily_application.save_id = NEW.save_id
                        AND daily_application.run_revision = NEW.run_revision
                        AND daily_application.source_kind = 'direct'
                        AND daily_application.submitted_game_day = NEW.submitted_game_day
                  ) < ruleset.daily_application_limit
              )
              OR (
                  NEW.source_kind = 'invitation'
                  AND NEW.status = 'interviewAwaitingConfirmation'
                  AND NEW.document_decided_game_day = save.game_day
                  AND NEW.interview_game_day
                      = save.game_day + posting.interview_delay_days
                  AND NEW.confirmation_expires_exclusive_game_day = NEW.interview_game_day
                  AND EXISTS (
                      SELECT 1
                      FROM job_invitation AS invitation
                      WHERE invitation.id = NEW.source_invitation_id
                        AND invitation.save_id = NEW.save_id
                        AND invitation.run_revision = NEW.run_revision
                        AND invitation.career_catalog_bundle_id
                            = NEW.career_catalog_bundle_id
                        AND invitation.recruitment_ruleset_id
                            = NEW.recruitment_ruleset_id
                        AND invitation.job_posting_id = NEW.job_posting_id
                        AND invitation.status = 'open'
                        AND invitation.invitation_game_day <= save.game_day
                        AND invitation.expires_exclusive_game_day > save.game_day
                        AND invitation.artifact_completeness_bp
                            = NEW.artifact_completeness_bp
                        AND invitation.visible_education_score_bp
                            = NEW.visible_education_score_bp
                        AND invitation.visible_certification_score_bp
                            = NEW.visible_certification_score_bp
                        AND invitation.visible_language_score_bp
                            = NEW.visible_language_score_bp
                        AND invitation.visible_training_score_bp
                            = NEW.visible_training_score_bp
                        AND invitation.visible_experience_score_bp
                            = NEW.visible_experience_score_bp
                        AND invitation.visible_project_score_bp
                            = NEW.visible_project_score_bp
                        AND invitation.profile_artifact_version_id IN (
                            NEW.resume_version_id,
                            NEW.portfolio_version_id,
                            NEW.linkedin_profile_version_id
                        )
                  )
              )
          )
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_job_application_credited_experience_insert
BEFORE INSERT ON job_application
FOR EACH ROW
FOLLOWS tr_job_application_valid_insert
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM job_posting AS posting
        WHERE posting.id = NEW.job_posting_id
          AND posting.career_catalog_bundle_id = NEW.career_catalog_bundle_id
          AND posting.minimum_experience_days <= (
              SELECT COALESCE(SUM(experience.credited_experience_days), 0)
              FROM spec_evidence AS experience
              WHERE experience.save_id = NEW.save_id
                AND experience.run_revision = NEW.run_revision
                AND experience.career_catalog_bundle_id
                    = NEW.career_catalog_bundle_id
                AND experience.kind = 'experience'
                AND experience.acquired_game_day <= NEW.submitted_game_day
                AND (
                    experience.expires_on_game_day IS NULL
                    OR experience.expires_on_game_day >= NEW.submitted_game_day
                )
          )
    ),
    NEW.save_id,
    NULL
);

DROP TRIGGER tr_job_invitation_valid_insert;

CREATE TRIGGER tr_job_invitation_valid_insert
BEFORE INSERT ON job_invitation
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'open'
        AND NEW.accepted_application_id IS NULL
        AND NEW.decided_game_day IS NULL
        AND EXISTS (
            SELECT 1
            FROM save
            INNER JOIN `character` AS candidate
                ON candidate.save_id = save.id
            INNER JOIN career_run
                ON career_run.save_id = save.id
               AND career_run.run_revision = save.run_revision
               AND career_run.career_catalog_bundle_id = NEW.career_catalog_bundle_id
            INNER JOIN job_posting AS posting
                ON posting.id = NEW.job_posting_id
               AND posting.career_catalog_bundle_id
                    = career_run.career_catalog_bundle_id
               AND posting.recruitment_ruleset_id = NEW.recruitment_ruleset_id
               AND posting.platform_catalog_id = NEW.platform_catalog_id
               AND posting.market_world_id = save.market_world_id
            INNER JOIN platform_catalog AS platform
                ON platform.id = posting.platform_catalog_id
               AND platform.career_catalog_bundle_id
                    = posting.career_catalog_bundle_id
            INNER JOIN recruitment_ruleset AS ruleset
                ON ruleset.id = posting.recruitment_ruleset_id
            INNER JOIN profile_artifact_version AS artifact
                ON artifact.id = NEW.profile_artifact_version_id
               AND artifact.save_id = save.id
               AND artifact.run_revision = save.run_revision
               AND artifact.career_catalog_bundle_id
                    = career_run.career_catalog_bundle_id
               AND artifact.sealed_at IS NOT NULL
               AND artifact.created_game_day <= NEW.invitation_game_day
               AND artifact.completeness_bp = NEW.artifact_completeness_bp
            WHERE save.id = NEW.save_id
              AND save.run_revision = NEW.run_revision
              AND NEW.invitation_game_day = save.game_day + 1
              AND NOT EXISTS (
                  SELECT 1
                  FROM profile_artifact_version AS newer_artifact
                  WHERE newer_artifact.save_id = artifact.save_id
                    AND newer_artifact.run_revision = artifact.run_revision
                    AND BINARY newer_artifact.artifact_kind
                        = BINARY artifact.artifact_kind
                    AND newer_artifact.sealed_at IS NOT NULL
                    AND newer_artifact.created_game_day
                        <= NEW.invitation_game_day
                    AND (
                        newer_artifact.version_no > artifact.version_no
                        OR (
                            newer_artifact.version_no = artifact.version_no
                            AND newer_artifact.id > artifact.id
                        )
                    )
              )
              AND NOT EXISTS (
                  SELECT 1
                  FROM employment_contract AS contract
                  WHERE contract.save_id = save.id
                    AND contract.run_revision = save.run_revision
                    AND contract.status IN ('pendingStart', 'active')
              )
              AND career_run.military_status <> 'serving'
              AND (
                  posting.military_requirement = 'none'
                  OR career_run.military_status IN ('completed', 'exempt')
              )
              AND (
                  platform.same_region_only = FALSE
                  OR BINARY candidate.region = BINARY posting.region
              )
              AND (
                  posting.minimum_education IS NULL
                  OR CASE candidate.education
                      WHEN 'highSchool' THEN 1
                      WHEN 'associate' THEN 2
                      WHEN 'bachelor' THEN 3
                      WHEN 'master' THEN 4
                      WHEN 'doctorate' THEN 5
                      ELSE 0
                  END >= CASE posting.minimum_education
                      WHEN 'highSchool' THEN 1
                      WHEN 'associate' THEN 2
                      WHEN 'bachelor' THEN 3
                      WHEN 'master' THEN 4
                      WHEN 'doctorate' THEN 5
                      ELSE 6
                  END
              )
              AND (
                  posting.required_certification_entry_id IS NULL
                  OR EXISTS (
                      SELECT 1
                      FROM spec_evidence AS certification
                      WHERE certification.save_id = save.id
                        AND certification.run_revision = save.run_revision
                        AND certification.spec_catalog_entry_id
                            = posting.required_certification_entry_id
                        AND certification.acquired_game_day
                            <= NEW.invitation_game_day
                        AND (
                            certification.expires_on_game_day IS NULL
                            OR certification.expires_on_game_day
                                >= NEW.invitation_game_day
                        )
                  )
              )
              AND posting.minimum_experience_days <= (
                  SELECT COALESCE(SUM(
                      CASE
                          WHEN experience.kind = 'experience'
                            AND experience.period_start_date IS NOT NULL
                            AND experience.period_end_exclusive_date IS NOT NULL
                          THEN DATEDIFF(
                              experience.period_end_exclusive_date,
                              experience.period_start_date
                          )
                          ELSE 0
                      END
                  ), 0)
                  FROM spec_evidence AS experience
                  WHERE experience.save_id = save.id
                    AND experience.run_revision = save.run_revision
                    AND experience.acquired_game_day <= NEW.invitation_game_day
                    AND (
                        experience.expires_on_game_day IS NULL
                        OR experience.expires_on_game_day
                            >= NEW.invitation_game_day
                    )
              )
              AND EXISTS (
                  SELECT 1
                  FROM career_scheduled_action AS action
                  WHERE action.save_id = NEW.save_id
                    AND action.run_revision = NEW.run_revision
                    AND action.action_kind = 'invitationGeneration'
                    AND action.platform_catalog_id = NEW.platform_catalog_id
                    AND action.status = 'pending'
                    AND action.due_game_day = NEW.invitation_game_day
              )
              AND NEW.invitation_game_day >= posting.posted_game_day
              AND NEW.invitation_game_day < posting.closes_exclusive_game_day
              AND NEW.expires_exclusive_game_day
                  = posting.closes_exclusive_game_day
              AND NEW.invitation_roll < NEW.invitation_probability_ppm
              AND (
                  SELECT COUNT(*)
                  FROM job_invitation AS open_invitation
                  WHERE open_invitation.save_id = NEW.save_id
                    AND open_invitation.run_revision = NEW.run_revision
                    AND open_invitation.status = 'open'
              ) < ruleset.open_invitation_limit
              AND (
                  (
                      platform.platform_key = 'saramin'
                      AND platform.invitation_source = 'resume'
                      AND artifact.artifact_kind = 'resume'
                  )
                  OR (
                      platform.platform_key = 'linkedin'
                      AND platform.invitation_source = 'linkedinProfile'
                      AND artifact.artifact_kind = 'linkedinProfile'
                      AND artifact.open_to_work = TRUE
                      AND EXISTS (
                          SELECT 1
                          FROM profile_artifact_industry AS artifact_industry
                          WHERE artifact_industry.save_id = artifact.save_id
                            AND artifact_industry.run_revision = artifact.run_revision
                            AND artifact_industry.profile_artifact_version_id = artifact.id
                            AND artifact_industry.career_industry_id
                                = posting.career_industry_id
                      )
                  )
              )
              AND EXISTS (
                  SELECT 1
                  FROM recruitment_score_band AS score_band
                  INNER JOIN recruitment_pass_probability AS probability
                      ON probability.recruitment_ruleset_id
                            = score_band.recruitment_ruleset_id
                     AND probability.score_band_key = score_band.score_band_key
                     AND probability.stage = 'invitation'
                     AND probability.competition_band = posting.competition_band
                  WHERE score_band.recruitment_ruleset_id
                        = NEW.recruitment_ruleset_id
                    AND NEW.invitation_score_bp >= score_band.minimum_score_bp
                    AND NEW.invitation_score_bp
                        < score_band.maximum_exclusive_score_bp
                    AND NEW.invitation_probability_ppm
                        = probability.pass_probability_ppm
              )
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_job_invitation_credited_experience_insert
BEFORE INSERT ON job_invitation
FOR EACH ROW
FOLLOWS tr_job_invitation_valid_insert
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM job_posting AS posting
        WHERE posting.id = NEW.job_posting_id
          AND posting.career_catalog_bundle_id = NEW.career_catalog_bundle_id
          AND posting.minimum_experience_days <= (
              SELECT COALESCE(SUM(experience.credited_experience_days), 0)
              FROM spec_evidence AS experience
              WHERE experience.save_id = NEW.save_id
                AND experience.run_revision = NEW.run_revision
                AND experience.career_catalog_bundle_id
                    = NEW.career_catalog_bundle_id
                AND experience.kind = 'experience'
                AND experience.acquired_game_day <= NEW.invitation_game_day
                AND (
                    experience.expires_on_game_day IS NULL
                    OR experience.expires_on_game_day >= NEW.invitation_game_day
                )
          )
    ),
    NEW.save_id,
    NULL
);

-- Military financial work uses exact version-1 payloads. Ids stay JSON strings so they retain
-- their full unsigned BIGINT identity through JavaScript clients.
DROP TRIGGER tr_scheduled_settlement_reconciliation_insert;

ALTER TABLE scheduled_settlement
    DROP CHECK ck_scheduled_settlement_kind,
    DROP CHECK ck_scheduled_settlement_source_kind,
    ADD CONSTRAINT ck_scheduled_settlement_kind CHECK (
        kind IN (
            'cmaInterest', 'depositMaturity', 'savingsInstallment', 'savingsMaturity',
            'bondCoupon', 'bondMaturity', 'llxDistribution', 'financialIncomeFiling',
            'employmentPayroll', 'employmentReconciliation', 'militaryPay',
            'militarySavingsInstallment', 'militarySavingsMaturity',
            'militarySavingsGovernmentMatch'
        )
    ),
    ADD CONSTRAINT ck_scheduled_settlement_source_kind CHECK (
        source_kind IN (
            'cmaAccount', 'depositContract', 'savingsContract', 'bondPosition',
            'indexPosition', 'taxYear', 'employmentContract', 'yearEndTaxAssessment',
            'militaryService', 'militarySavingsContract', 'militarySavingsInstallment'
        )
    ),
    ADD CONSTRAINT ck_scheduled_settlement_military_payload CHECK (
        (
            kind = 'militaryPay'
            AND JSON_TYPE(payload) = 'OBJECT'
            AND JSON_LENGTH(payload) = 3
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.version')) = 'INTEGER'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.version')) = '1'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.militaryServiceId')) = 'STRING'
            AND REGEXP_LIKE(
                JSON_UNQUOTE(JSON_EXTRACT(payload, '$.militaryServiceId')),
                '^[1-9][0-9]{0,19}$'
            )
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.periodNo')) = 'INTEGER'
            AND CAST(
                JSON_UNQUOTE(JSON_EXTRACT(payload, '$.periodNo')) AS UNSIGNED
            ) > 0
            AND source_kind = 'militaryService'
            AND BINARY source_id = BINARY JSON_UNQUOTE(
                JSON_EXTRACT(payload, '$.militaryServiceId')
            )
            AND occurrence = CAST(
                JSON_UNQUOTE(JSON_EXTRACT(payload, '$.periodNo')) AS UNSIGNED
            )
        )
        OR (
            kind = 'militarySavingsInstallment'
            AND JSON_TYPE(payload) = 'OBJECT'
            AND JSON_LENGTH(payload) = 3
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.version')) = 'INTEGER'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.version')) = '1'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.contractId')) = 'STRING'
            AND REGEXP_LIKE(
                JSON_UNQUOTE(JSON_EXTRACT(payload, '$.contractId')),
                '^[1-9][0-9]{0,19}$'
            )
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.installmentNo')) = 'INTEGER'
            AND CAST(
                JSON_UNQUOTE(JSON_EXTRACT(payload, '$.installmentNo')) AS UNSIGNED
            ) > 0
            AND source_kind = 'militarySavingsContract'
            AND BINARY source_id = BINARY JSON_UNQUOTE(
                JSON_EXTRACT(payload, '$.contractId')
            )
            AND occurrence = CAST(
                JSON_UNQUOTE(JSON_EXTRACT(payload, '$.installmentNo')) AS UNSIGNED
            )
        )
        OR (
            kind = 'militarySavingsMaturity'
            AND JSON_TYPE(payload) = 'OBJECT'
            AND JSON_LENGTH(payload) = 2
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.version')) = 'INTEGER'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.version')) = '1'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.contractId')) = 'STRING'
            AND REGEXP_LIKE(
                JSON_UNQUOTE(JSON_EXTRACT(payload, '$.contractId')),
                '^[1-9][0-9]{0,19}$'
            )
            AND source_kind = 'militarySavingsContract'
            AND BINARY source_id = BINARY JSON_UNQUOTE(
                JSON_EXTRACT(payload, '$.contractId')
            )
        )
        OR (
            kind = 'militarySavingsGovernmentMatch'
            AND JSON_TYPE(payload) = 'OBJECT'
            AND JSON_LENGTH(payload) = 3
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.version')) = 'INTEGER'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.version')) = '1'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.contractId')) = 'STRING'
            AND REGEXP_LIKE(
                JSON_UNQUOTE(JSON_EXTRACT(payload, '$.contractId')),
                '^[1-9][0-9]{0,19}$'
            )
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.installmentNo')) = 'INTEGER'
            AND CAST(
                JSON_UNQUOTE(JSON_EXTRACT(payload, '$.installmentNo')) AS UNSIGNED
            ) > 0
            AND source_kind = 'militarySavingsInstallment'
            AND occurrence = 1
        )
        OR kind NOT IN (
            'militaryPay', 'militarySavingsInstallment',
            'militarySavingsMaturity', 'militarySavingsGovernmentMatch'
        )
    );

-- Legacy bridge services already belong to an active run when this migration is applied.
-- Only future regular paydays are scheduled; elapsed periods are never paid retroactively.
INSERT INTO scheduled_settlement
    (
        save_id,
        run_revision,
        due_game_day,
        kind,
        payload,
        source_kind,
        source_id,
        occurrence,
        status
    )
WITH RECURSIVE military_pay_period (period_no) AS (
    SELECT CAST(1 AS UNSIGNED)
    UNION ALL
    SELECT period_no + 1
    FROM military_pay_period
    WHERE period_no < 600
),
legacy_military_payday AS (
    SELECT service.save_id,
           service.run_revision,
           service.id AS military_service_id,
           save.game_day AS current_game_day,
           market_world.start_date AS world_start_date,
           service.start_date AS service_start_date,
           service.end_exclusive_date AS service_end_exclusive_date,
           military_pay_period.period_no,
           TIMESTAMPADD(
               DAY,
               LEAST(
                   option_policy.payday_day_of_month,
                   DAY(LAST_DAY(TIMESTAMPADD(
                       MONTH,
                       military_pay_period.period_no - 1 + IF(
                           LEAST(
                               option_policy.payday_day_of_month,
                               DAY(LAST_DAY(service.start_date))
                           ) < DAY(service.start_date),
                           1,
                           0
                       ),
                       TIMESTAMPADD(
                           DAY,
                           1 - DAY(service.start_date),
                           service.start_date
                       )
                   )))
               ) - 1,
               TIMESTAMPADD(
                   MONTH,
                   military_pay_period.period_no - 1 + IF(
                       LEAST(
                           option_policy.payday_day_of_month,
                           DAY(LAST_DAY(service.start_date))
                       ) < DAY(service.start_date),
                       1,
                       0
                   ),
                   TIMESTAMPADD(
                       DAY,
                       1 - DAY(service.start_date),
                       service.start_date
                   )
               )
           ) AS pay_date
    FROM military_service AS service
    INNER JOIN save
        ON save.id = service.save_id
       AND save.run_revision = service.run_revision
    INNER JOIN market_world
        ON market_world.id = save.market_world_id
    INNER JOIN military_option_policy AS option_policy
        ON option_policy.id = service.military_option_policy_id
       AND option_policy.employment_policy_set_id = service.employment_policy_set_id
       AND option_policy.career_catalog_bundle_id = service.career_catalog_bundle_id
       AND option_policy.military_option_version_id
            = service.military_option_version_id
       AND option_policy.pay_schedule_kind = 'monthly'
    CROSS JOIN military_pay_period
    WHERE service.source_kind = 'legacyBridge'
      AND service.status = 'pendingStart'
)
SELECT payday.save_id,
       payday.run_revision,
       DATEDIFF(payday.pay_date, payday.world_start_date),
       'militaryPay',
       JSON_OBJECT(
           'version', 1,
           'militaryServiceId', CAST(payday.military_service_id AS CHAR),
           'periodNo', CAST(payday.period_no AS SIGNED)
       ),
       'militaryService',
       CAST(payday.military_service_id AS CHAR),
       payday.period_no,
       'pending'
FROM legacy_military_payday AS payday
WHERE payday.pay_date >= payday.service_start_date
  AND payday.pay_date < payday.service_end_exclusive_date
  AND DATEDIFF(payday.pay_date, payday.world_start_date) > payday.current_game_day
  AND NOT EXISTS (
      SELECT 1
      FROM scheduled_settlement AS existing
      WHERE existing.save_id = payday.save_id
        AND existing.run_revision = payday.run_revision
        AND existing.source_kind = 'militaryService'
        AND BINARY existing.source_id
            = BINARY CAST(payday.military_service_id AS CHAR)
        AND existing.occurrence = payday.period_no
  )
ORDER BY payday.military_service_id, payday.period_no;

CREATE TRIGGER tr_scheduled_settlement_military_insert
BEFORE INSERT ON scheduled_settlement
FOR EACH ROW
FOLLOWS tr_scheduled_settlement_pending_insert
SET NEW.status = IF(
    NEW.kind NOT IN (
        'militaryPay', 'militarySavingsInstallment',
        'militarySavingsMaturity', 'militarySavingsGovernmentMatch'
    )
        OR (
            NEW.kind = 'militaryPay'
            AND EXISTS (
                SELECT 1
                FROM military_service AS service
                INNER JOIN save
                    ON save.id = service.save_id
                   AND save.run_revision = service.run_revision
                INNER JOIN market_world
                    ON market_world.id = save.market_world_id
                INNER JOIN military_option_policy AS option_policy
                    ON option_policy.id = service.military_option_policy_id
                   AND option_policy.employment_policy_set_id
                        = service.employment_policy_set_id
                   AND option_policy.career_catalog_bundle_id
                        = service.career_catalog_bundle_id
                   AND option_policy.military_option_version_id
                        = service.military_option_version_id
                   AND option_policy.pay_schedule_kind = 'monthly'
                INNER JOIN employment_policy_set AS employment_policy
                    ON employment_policy.id = service.employment_policy_set_id
                   AND employment_policy.published_at IS NOT NULL
                WHERE service.id = CAST(
                          JSON_UNQUOTE(JSON_EXTRACT(NEW.payload, '$.militaryServiceId'))
                          AS UNSIGNED
                      )
                  AND service.save_id = NEW.save_id
                  AND service.run_revision = NEW.run_revision
                  AND service.status IN ('pendingStart', 'serving')
                  AND BINARY NEW.source_id = BINARY CAST(service.id AS CHAR)
                  AND NEW.occurrence BETWEEN 1 AND 600
                  AND NEW.due_game_day = DATEDIFF(
                      TIMESTAMPADD(
                          DAY,
                          LEAST(
                              option_policy.payday_day_of_month,
                              DAY(LAST_DAY(TIMESTAMPADD(
                                  MONTH,
                                  NEW.occurrence - 1 + IF(
                                      LEAST(
                                          option_policy.payday_day_of_month,
                                          DAY(LAST_DAY(service.start_date))
                                      ) < DAY(service.start_date),
                                      1,
                                      0
                                  ),
                                  TIMESTAMPADD(
                                      DAY,
                                      1 - DAY(service.start_date),
                                      service.start_date
                                  )
                              )))
                          ) - 1,
                          TIMESTAMPADD(
                              MONTH,
                              NEW.occurrence - 1 + IF(
                                  LEAST(
                                      option_policy.payday_day_of_month,
                                      DAY(LAST_DAY(service.start_date))
                                  ) < DAY(service.start_date),
                                  1,
                                  0
                              ),
                              TIMESTAMPADD(
                                  DAY,
                                  1 - DAY(service.start_date),
                                  service.start_date
                              )
                          )
                      ),
                      market_world.start_date
                  )
                  AND DATE_ADD(
                      market_world.start_date,
                      INTERVAL NEW.due_game_day DAY
                  ) >= service.start_date
                  AND DATE_ADD(
                      market_world.start_date,
                      INTERVAL NEW.due_game_day DAY
                  ) < service.end_exclusive_date
                  AND (
                      NEW.occurrence = 1
                      OR EXISTS (
                          SELECT 1
                          FROM scheduled_settlement AS previous_schedule
                          WHERE previous_schedule.save_id = NEW.save_id
                            AND previous_schedule.run_revision = NEW.run_revision
                            AND previous_schedule.kind = 'militaryPay'
                            AND previous_schedule.source_kind = 'militaryService'
                            AND BINARY previous_schedule.source_id
                                = BINARY CAST(service.id AS CHAR)
                            AND previous_schedule.occurrence = NEW.occurrence - 1
                      )
                      OR (
                          service.source_kind = 'legacyBridge'
                          AND NEW.occurrence > 1
                          AND NEW.due_game_day > save.game_day
                          AND DATEDIFF(
                              TIMESTAMPADD(
                                  DAY,
                                  LEAST(
                                      option_policy.payday_day_of_month,
                                      DAY(LAST_DAY(TIMESTAMPADD(
                                          MONTH,
                                          CAST(NEW.occurrence AS SIGNED) - 2 + IF(
                                              LEAST(
                                                  option_policy.payday_day_of_month,
                                                  DAY(LAST_DAY(service.start_date))
                                              ) < DAY(service.start_date),
                                              1,
                                              0
                                          ),
                                          TIMESTAMPADD(
                                              DAY,
                                              1 - DAY(service.start_date),
                                              service.start_date
                                          )
                                      )))
                                  ) - 1,
                                  TIMESTAMPADD(
                                      MONTH,
                                      CAST(NEW.occurrence AS SIGNED) - 2 + IF(
                                          LEAST(
                                              option_policy.payday_day_of_month,
                                              DAY(LAST_DAY(service.start_date))
                                          ) < DAY(service.start_date),
                                          1,
                                          0
                                      ),
                                      TIMESTAMPADD(
                                          DAY,
                                          1 - DAY(service.start_date),
                                          service.start_date
                                      )
                                  )
                              ),
                              market_world.start_date
                          ) <= save.game_day
                      )
                  )
            )
        )
        OR (
            NEW.kind = 'militarySavingsInstallment'
            AND EXISTS (
                SELECT 1
                FROM military_savings_contract AS contract
                INNER JOIN save
                    ON save.id = contract.save_id
                   AND save.run_revision = contract.run_revision
                INNER JOIN market_world
                    ON market_world.id = save.market_world_id
                WHERE contract.id = CAST(
                          JSON_UNQUOTE(JSON_EXTRACT(NEW.payload, '$.contractId'))
                          AS UNSIGNED
                      )
                  AND contract.save_id = NEW.save_id
                  AND contract.run_revision = NEW.run_revision
                  AND contract.status = 'active'
                  AND BINARY NEW.source_id = BINARY CAST(contract.id AS CHAR)
                  AND NEW.occurrence BETWEEN 1 AND contract.term_months
                  AND NEW.due_game_day < contract.maturity_game_day
                  AND NEW.due_game_day = DATEDIFF(
                      TIMESTAMPADD(
                          DAY,
                          LEAST(
                              contract.debit_day_of_month,
                              DAY(LAST_DAY(TIMESTAMPADD(
                                  MONTH,
                                  NEW.occurrence - 1,
                                  TIMESTAMPADD(
                                      DAY,
                                      1 - DAY(DATE_ADD(
                                          market_world.start_date,
                                          INTERVAL contract.first_installment_game_day DAY
                                      )),
                                      DATE_ADD(
                                          market_world.start_date,
                                          INTERVAL contract.first_installment_game_day DAY
                                      )
                                  )
                              )))
                          ) - 1,
                          TIMESTAMPADD(
                              MONTH,
                              NEW.occurrence - 1,
                              TIMESTAMPADD(
                                  DAY,
                                  1 - DAY(DATE_ADD(
                                      market_world.start_date,
                                      INTERVAL contract.first_installment_game_day DAY
                                  )),
                                  DATE_ADD(
                                      market_world.start_date,
                                      INTERVAL contract.first_installment_game_day DAY
                                  )
                              )
                          )
                      ),
                      market_world.start_date
                  )
                  AND (
                      NEW.occurrence = 1
                      OR EXISTS (
                          SELECT 1
                          FROM scheduled_settlement AS previous_schedule
                          WHERE previous_schedule.save_id = NEW.save_id
                            AND previous_schedule.run_revision = NEW.run_revision
                            AND previous_schedule.kind = 'militarySavingsInstallment'
                            AND previous_schedule.source_kind
                                = 'militarySavingsContract'
                            AND BINARY previous_schedule.source_id
                                = BINARY CAST(contract.id AS CHAR)
                            AND previous_schedule.occurrence = NEW.occurrence - 1
                      )
                  )
            )
        )
        OR (
            NEW.kind = 'militarySavingsMaturity'
            AND EXISTS (
                SELECT 1
                FROM military_savings_contract AS contract
                WHERE contract.id = CAST(
                          JSON_UNQUOTE(JSON_EXTRACT(NEW.payload, '$.contractId'))
                          AS UNSIGNED
                      )
                  AND contract.save_id = NEW.save_id
                  AND contract.run_revision = NEW.run_revision
                  AND contract.status = 'active'
                  AND BINARY NEW.source_id = BINARY CAST(contract.id AS CHAR)
                  AND NEW.occurrence = contract.term_months + 1
                  AND NEW.due_game_day = contract.maturity_game_day
            )
        )
        OR (
            NEW.kind = 'militarySavingsGovernmentMatch'
            AND EXISTS (
                SELECT 1
                FROM military_savings_installment AS installment
                INNER JOIN military_savings_contract AS contract
                    ON contract.save_id = installment.save_id
                   AND contract.run_revision = installment.run_revision
                   AND contract.id = installment.military_savings_contract_id
                INNER JOIN military_savings_policy AS contract_policy
                    ON contract_policy.id = contract.military_savings_policy_id
                   AND contract_policy.employment_policy_set_id
                        = contract.employment_policy_set_id
                INNER JOIN save
                    ON save.id = installment.save_id
                   AND save.run_revision = installment.run_revision
                INNER JOIN market_world
                    ON market_world.id = save.market_world_id
                WHERE contract.id = CAST(
                          JSON_UNQUOTE(JSON_EXTRACT(NEW.payload, '$.contractId'))
                          AS UNSIGNED
                      )
                  AND installment.installment_no = CAST(
                          JSON_UNQUOTE(JSON_EXTRACT(NEW.payload, '$.installmentNo'))
                          AS UNSIGNED
                      )
                  AND installment.save_id = NEW.save_id
                  AND installment.run_revision = NEW.run_revision
                  AND installment.status = 'paid'
                  AND installment.government_match_krw > 0
                  AND installment.government_match_settlement_id IS NULL
                  AND contract.status = 'matured'
                  AND BINARY NEW.source_id = BINARY CAST(installment.id AS CHAR)
                  AND NEW.occurrence = 1
                  AND NEW.due_game_day = DATEDIFF(
                      TIMESTAMPADD(
                          DAY,
                          LEAST(
                              contract_policy.government_match_next_month_day,
                              DAY(LAST_DAY(TIMESTAMPADD(
                                  MONTH,
                                  1,
                                  TIMESTAMPADD(
                                      DAY,
                                      1 - DAY(DATE_ADD(
                                          market_world.start_date,
                                          INTERVAL contract.maturity_game_day DAY
                                      )),
                                      DATE_ADD(
                                          market_world.start_date,
                                          INTERVAL contract.maturity_game_day DAY
                                      )
                                  )
                              )))
                          ) - 1,
                          TIMESTAMPADD(
                              MONTH,
                              1,
                              TIMESTAMPADD(
                                  DAY,
                                  1 - DAY(DATE_ADD(
                                      market_world.start_date,
                                      INTERVAL contract.maturity_game_day DAY
                                  )),
                                  DATE_ADD(
                                      market_world.start_date,
                                      INTERVAL contract.maturity_game_day DAY
                                  )
                              )
                          )
                      ),
                      market_world.start_date
                  )
            )
        ),
    NEW.status,
    NULL
);

CREATE TRIGGER tr_scheduled_settlement_reconciliation_insert
BEFORE INSERT ON scheduled_settlement
FOR EACH ROW
FOLLOWS tr_scheduled_settlement_military_insert
SET NEW.status = IF(
    NEW.kind <> 'employmentReconciliation'
        OR EXISTS (
            SELECT 1
            FROM year_end_tax_assessment AS assessment
            INNER JOIN employment_income_year AS income_year
                ON income_year.save_id = assessment.save_id
               AND income_year.run_revision = assessment.run_revision
               AND income_year.tax_year = assessment.tax_year
               AND income_year.status = 'finalized'
            INNER JOIN employment_annual_tax_policy AS annual_policy
                ON annual_policy.employment_policy_set_id
                    = assessment.employment_policy_set_id
               AND annual_policy.id = assessment.employment_annual_tax_policy_id
            INNER JOIN save
                ON save.id = assessment.save_id
               AND save.run_revision = assessment.run_revision
            INNER JOIN market_world
                ON market_world.id = save.market_world_id
            WHERE assessment.id = CAST(
                      JSON_UNQUOTE(JSON_EXTRACT(NEW.payload, '$.assessmentId'))
                      AS UNSIGNED
                  )
              AND assessment.save_id = NEW.save_id
              AND assessment.run_revision = NEW.run_revision
              AND assessment.assessment_kind = 'employmentOnly'
              AND assessment.assessment_status IN ('provisional', 'definitive')
              AND assessment.tax_year = CAST(
                      JSON_UNQUOTE(JSON_EXTRACT(NEW.payload, '$.taxYear'))
                      AS UNSIGNED
                  )
              AND BINARY NEW.source_id = BINARY CAST(assessment.id AS CHAR)
              AND (
                  income_year.income_event_count > 0
                  OR income_year.gross_employment_income_krw > 0
                  OR income_year.employee_insurance_total_krw > 0
                  OR income_year.withheld_income_tax_krw > 0
                  OR income_year.withheld_local_income_tax_krw > 0
                  OR assessment.additional_tax_krw > 0
                  OR assessment.refund_krw > 0
              )
              AND NEW.due_game_day = COALESCE(
                  (
                      SELECT MIN(payroll_schedule.due_game_day)
                      FROM scheduled_settlement AS payroll_schedule
                      WHERE payroll_schedule.save_id = assessment.save_id
                        AND payroll_schedule.run_revision = assessment.run_revision
                        AND payroll_schedule.kind = 'employmentPayroll'
                        AND payroll_schedule.source_kind = 'employmentContract'
                        AND payroll_schedule.status = 'pending'
                        AND YEAR(DATE_ADD(
                            market_world.start_date,
                            INTERVAL payroll_schedule.due_game_day DAY
                        )) = assessment.tax_year + 1
                        AND MONTH(DATE_ADD(
                            market_world.start_date,
                            INTERVAL payroll_schedule.due_game_day DAY
                        )) = 2
                  ),
                  (
                      SELECT MIN(military_schedule.due_game_day)
                      FROM scheduled_settlement AS military_schedule
                      WHERE military_schedule.save_id = assessment.save_id
                        AND military_schedule.run_revision = assessment.run_revision
                        AND military_schedule.kind = 'militaryPay'
                        AND military_schedule.source_kind = 'militaryService'
                        AND military_schedule.status = 'pending'
                        AND YEAR(DATE_ADD(
                            market_world.start_date,
                            INTERVAL military_schedule.due_game_day DAY
                        )) = assessment.tax_year + 1
                        AND MONTH(DATE_ADD(
                            market_world.start_date,
                            INTERVAL military_schedule.due_game_day DAY
                        )) = 2
                  ),
                  DATEDIFF(
                      TIMESTAMPADD(
                          DAY,
                          LEAST(
                              annual_policy.february_reconciliation_day_of_month,
                              DAY(LAST_DAY(STR_TO_DATE(
                                  CONCAT(assessment.tax_year + 1, '-02-01'),
                                  '%Y-%m-%d'
                              )))
                          ) - 1,
                          STR_TO_DATE(
                              CONCAT(assessment.tax_year + 1, '-02-01'),
                              '%Y-%m-%d'
                          )
                      ),
                      market_world.start_date
                  )
              )
        ),
    NEW.status,
    NULL
);

-- Military source kinds are a closed namespace even though legacy finance source kinds remain
-- extensible. Savings postings carry their contract FK on the principal/interest/match side.
ALTER TABLE ledger_transaction
    ADD CONSTRAINT ck_ledger_transaction_military_source CHECK (
        source_kind NOT LIKE 'military%'
        OR source_kind IN (
            'militaryPay', 'militarySavingsInstallment',
            'militarySavingsMaturity', 'militarySavingsGovernmentMatch',
            'militarySavingsEarlyClose'
        )
    );

ALTER TABLE ledger_posting
    DROP CHECK ck_ledger_posting_account_code,
    DROP CHECK ck_ledger_posting_account_reference,
    ADD COLUMN military_savings_contract_id BIGINT UNSIGNED NULL
        AFTER financial_account_id,
    ADD KEY ix_ledger_posting_military_savings_contract
        (save_id, run_revision, military_savings_contract_id),
    ADD CONSTRAINT fk_ledger_posting_military_savings_contract
        FOREIGN KEY (save_id, run_revision, military_savings_contract_id)
        REFERENCES military_savings_contract (save_id, run_revision, id),
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
            'militarySavingsGovernmentMatchIncome'
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
        )
        OR (
            account_code IN (
                'militarySavingsPrincipal', 'militarySavingsBankInterest',
                'militarySavingsGovernmentMatchIncome'
            )
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NOT NULL
        )
        OR (
            account_code NOT IN (
                'accountCash', 'productPrincipal',
                'pensionTaxExcludedContribution', 'pensionCreditedContribution',
                'militarySavingsPrincipal', 'militarySavingsBankInterest',
                'militarySavingsGovernmentMatchIncome'
            )
            AND financial_account_id IS NULL
            AND military_savings_contract_id IS NULL
        )
    );

-- Publication closes every M3-D typed child graph. The original A/C publication triggers still
-- validate their own graphs first; these triggers only add D-specific completeness predicates.
CREATE TRIGGER tr_career_catalog_bundle_m3d_publish
BEFORE UPDATE ON career_catalog_bundle
FOR EACH ROW
FOLLOWS tr_career_catalog_bundle_publish_only
SET NEW.bundle_key = IF(
    OLD.published_at IS NULL
        AND NEW.published_at IS NOT NULL
        AND NEW.id = OLD.id
        AND BINARY NEW.bundle_key = BINARY OLD.bundle_key
        AND NEW.ranked_eligible = OLD.ranked_eligible
        AND (
            SELECT COUNT(*)
            FROM military_option_version AS option_row
            WHERE option_row.career_catalog_bundle_id = OLD.id
        ) = 6
        AND NOT EXISTS (
            SELECT 1
            FROM military_option_version AS option_row
            WHERE option_row.career_catalog_bundle_id = OLD.id
              AND NOT EXISTS (
                  SELECT 1
                  FROM military_option_eligibility_rule AS eligibility
                  WHERE eligibility.career_catalog_bundle_id = OLD.id
                    AND eligibility.military_option_version_id = option_row.id
              )
        )
        AND NOT EXISTS (
            SELECT 1
            FROM military_option_job_family AS option_family
            WHERE option_family.career_catalog_bundle_id = OLD.id
              AND NOT EXISTS (
                  SELECT 1
                  FROM military_option_experience_evidence_mapping AS mapping
                  INNER JOIN spec_catalog_entry AS entry
                      ON entry.career_catalog_bundle_id = mapping.career_catalog_bundle_id
                     AND entry.id = mapping.spec_catalog_entry_id
                     AND entry.kind = 'experience'
                     AND entry.stackable = TRUE
                     AND entry.validity_days IS NULL
                  INNER JOIN spec_catalog_contribution AS contribution
                      ON contribution.career_catalog_bundle_id
                          = mapping.career_catalog_bundle_id
                     AND contribution.spec_catalog_entry_id
                          = mapping.spec_catalog_entry_id
                     AND contribution.career_job_family_id
                          = mapping.career_job_family_id
                     AND contribution.contribution_bp = FLOOR(
                         300 * option_family.experience_credit_ppm / 1000000
                     )
                     AND contribution.contribution_bp > 0
                  WHERE mapping.career_catalog_bundle_id = OLD.id
                    AND mapping.military_option_version_id
                        = option_family.military_option_version_id
                    AND mapping.career_job_family_id
                        = option_family.career_job_family_id
                    AND NOT EXISTS (
                        SELECT 1
                        FROM spec_catalog_contribution AS other_contribution
                        WHERE other_contribution.career_catalog_bundle_id = OLD.id
                          AND other_contribution.spec_catalog_entry_id
                              = mapping.spec_catalog_entry_id
                          AND other_contribution.career_job_family_id
                              <> mapping.career_job_family_id
                    )
              )
        )
        AND (
            SELECT COUNT(*)
            FROM military_savings_product_version AS product
            WHERE product.career_catalog_bundle_id = OLD.id
        ) = (
            SELECT COUNT(*)
            FROM military_savings_institution_catalog AS institution
            WHERE institution.career_catalog_bundle_id = OLD.id
        )
        AND (
            SELECT COUNT(*)
            FROM military_savings_product_version AS product
            WHERE product.career_catalog_bundle_id = OLD.id
        ) > 0
        AND NOT EXISTS (
            SELECT 1
            FROM military_savings_product_version AS product
            WHERE product.career_catalog_bundle_id = OLD.id
              AND (
                  (
                      SELECT COALESCE(MIN(rate.minimum_term_months), 0)
                      FROM military_savings_product_rate_band AS rate
                      WHERE rate.military_savings_product_id = product.id
                  ) <> product.minimum_term_months
                  OR (
                      SELECT COALESCE(MAX(rate.maximum_term_months_exclusive), 0)
                      FROM military_savings_product_rate_band AS rate
                      WHERE rate.military_savings_product_id = product.id
                  ) <> product.maximum_term_months + 1
                  OR (
                      SELECT COALESCE(SUM(
                          rate.maximum_term_months_exclusive
                              - rate.minimum_term_months
                      ), 0)
                      FROM military_savings_product_rate_band AS rate
                      WHERE rate.military_savings_product_id = product.id
                  ) <> product.maximum_term_months - product.minimum_term_months + 1
                  OR (
                      SELECT COALESCE(MAX(rate.band_order), 0)
                      FROM military_savings_product_rate_band AS rate
                      WHERE rate.military_savings_product_id = product.id
                  ) <> (
                      SELECT COUNT(*)
                      FROM military_savings_product_rate_band AS rate
                      WHERE rate.military_savings_product_id = product.id
                  )
                  OR (
                      NEW.ranked_eligible = TRUE
                      AND (
                          product.ranked_eligible = FALSE
                          OR product.provenance_kind <> 'reviewedOfficial'
                          OR product.terms_verified_on > CURRENT_DATE()
                      )
                  )
              )
        ),
    OLD.bundle_key,
    NULL
);

CREATE TRIGGER tr_employment_policy_set_m3d_publish
BEFORE UPDATE ON employment_policy_set
FOR EACH ROW
FOLLOWS tr_employment_policy_set_publish_only
SET NEW.policy_key = IF(
    OLD.published_at IS NULL
        AND NEW.published_at IS NOT NULL
        AND NEW.id = OLD.id
        AND BINARY NEW.policy_key = BINARY OLD.policy_key
        AND NEW.ranked_eligible = OLD.ranked_eligible
        AND (
            SELECT COUNT(DISTINCT option_policy.service_type)
            FROM military_option_policy AS option_policy
            WHERE option_policy.employment_policy_set_id = OLD.id
        ) = 6
        AND (
            SELECT COUNT(DISTINCT option_policy.career_catalog_bundle_id)
            FROM military_option_policy AS option_policy
            WHERE option_policy.employment_policy_set_id = OLD.id
        ) = 1
        AND NOT EXISTS (
            SELECT 1
            FROM military_option_policy AS option_policy
            WHERE option_policy.employment_policy_set_id = OLD.id
              AND (
                  (
                      SELECT COALESCE(SUM(DATEDIFF(
                          COALESCE(
                              effective_policy.effective_to_exclusive,
                              OLD.coverage_end_exclusive
                          ),
                          effective_policy.effective_from
                      )), 0)
                      FROM military_option_policy AS effective_policy
                      WHERE effective_policy.employment_policy_set_id = OLD.id
                        AND effective_policy.career_catalog_bundle_id
                            = option_policy.career_catalog_bundle_id
                        AND effective_policy.military_option_version_id
                            = option_policy.military_option_version_id
                  ) <> DATEDIFF(OLD.coverage_end_exclusive, OLD.coverage_start)
                  OR (
                      SELECT COALESCE(MIN(stage.start_service_month), 601)
                      FROM military_pay_stage AS stage
                      WHERE stage.military_option_policy_id = option_policy.id
                  ) <> 0
                  OR (
                      SELECT COALESCE(MAX(stage.end_service_month_exclusive), 0)
                      FROM military_pay_stage AS stage
                      WHERE stage.military_option_policy_id = option_policy.id
                  ) <> option_policy.service_duration_months
                  OR (
                      SELECT COALESCE(SUM(
                          stage.end_service_month_exclusive
                              - stage.start_service_month
                      ), 0)
                      FROM military_pay_stage AS stage
                      WHERE stage.military_option_policy_id = option_policy.id
                  ) <> option_policy.service_duration_months
                  OR (
                      SELECT COALESCE(MAX(stage.stage_order), 0)
                      FROM military_pay_stage AS stage
                      WHERE stage.military_option_policy_id = option_policy.id
                  ) <> (
                      SELECT COUNT(*)
                      FROM military_pay_stage AS stage
                      WHERE stage.military_option_policy_id = option_policy.id
                  )
                  OR (
                      NEW.ranked_eligible = TRUE
                      AND (
                          option_policy.data_status <> 'reviewedOfficial'
                          OR option_policy.duration_policy_source_id IS NULL
                          OR option_policy.compensation_policy_source_id IS NULL
                          OR option_policy.partial_month_pay_kind <> 'verifiedPolicy'
                          OR option_policy.reimbursement_model_kind
                              = 'reimbursementNotModeled'
                          OR option_policy.availability_status <> 'available'
                      )
                  )
              )
        )
        AND (
            SELECT COALESCE(SUM(DATEDIFF(
                COALESCE(
                    savings_policy.effective_to_exclusive,
                    OLD.coverage_end_exclusive
                ),
                savings_policy.effective_from
            )), 0)
            FROM military_savings_policy AS savings_policy
            WHERE savings_policy.employment_policy_set_id = OLD.id
        ) = DATEDIFF(OLD.coverage_end_exclusive, OLD.coverage_start)
        AND NOT EXISTS (
            SELECT 1
            FROM military_savings_policy AS savings_policy
            WHERE savings_policy.employment_policy_set_id = OLD.id
              AND (
                  (
                      SELECT COUNT(*)
                      FROM military_savings_policy_eligible_service AS eligible
                      WHERE eligible.employment_policy_set_id = OLD.id
                        AND eligible.military_savings_policy_id = savings_policy.id
                  ) <> 2
                  OR NOT EXISTS (
                      SELECT 1
                      FROM military_savings_policy_eligible_service AS eligible
                      WHERE eligible.employment_policy_set_id = OLD.id
                        AND eligible.military_savings_policy_id = savings_policy.id
                        AND eligible.service_type = 'activeDuty'
                  )
                  OR NOT EXISTS (
                      SELECT 1
                      FROM military_savings_policy_eligible_service AS eligible
                      WHERE eligible.employment_policy_set_id = OLD.id
                        AND eligible.military_savings_policy_id = savings_policy.id
                        AND eligible.service_type = 'socialService'
                  )
                  OR (
                      NEW.ranked_eligible = TRUE
                      AND savings_policy.data_status <> 'reviewedOfficial'
                  )
              )
        )
        AND NOT EXISTS (
            SELECT 1
            FROM employment_annual_tax_policy AS annual_policy
            WHERE annual_policy.employment_policy_set_id = OLD.id
              AND annual_policy.february_reconciliation_day_of_month IS NULL
        ),
    OLD.policy_key,
    NULL
);
