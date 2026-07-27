-- M3-C immutable employment policy, payroll, annual employment income, and pension allocation
-- foundations (m3-career.md §2, §7–§9, §12–§13).

CREATE TABLE employment_policy_set (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    policy_key                      VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ranked_eligible                 BOOLEAN         NOT NULL DEFAULT FALSE,
    coverage_start                  DATE            NOT NULL,
    coverage_end_exclusive          DATE            NOT NULL,
    published_at                    DATETIME(3)          NULL,
    created_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_employment_policy_set_key (policy_key),
    UNIQUE KEY uk_employment_policy_set_id_key (id, policy_key),
    CONSTRAINT ck_employment_policy_set_key CHECK (
        CHAR_LENGTH(policy_key) > 0
        AND policy_key REGEXP '^[a-z0-9][a-z0-9._-]{0,63}$'
    ),
    CONSTRAINT ck_employment_policy_set_ranked CHECK (ranked_eligible IN (FALSE, TRUE)),
    CONSTRAINT ck_employment_policy_set_ranked_key CHECK (
        ranked_eligible = FALSE OR policy_key NOT LIKE 'dev-unranked-%'
    ),
    CONSTRAINT ck_employment_policy_set_coverage CHECK (
        coverage_start < coverage_end_exclusive
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE employment_policy_source (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    employment_policy_set_id        BIGINT UNSIGNED NOT NULL,
    source_key                      VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    title                           VARCHAR(255)    NOT NULL,
    source_url                      VARCHAR(1024) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    verified_on                     DATE            NOT NULL,
    content_sha256                  CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_employment_policy_source_key
        (employment_policy_set_id, source_key),
    UNIQUE KEY uk_employment_policy_source_set_id
        (employment_policy_set_id, id),
    CONSTRAINT fk_employment_policy_source_set
        FOREIGN KEY (employment_policy_set_id) REFERENCES employment_policy_set (id),
    CONSTRAINT ck_employment_policy_source_key CHECK (
        CHAR_LENGTH(source_key) > 0
        AND source_key REGEXP '^[a-z0-9][a-z0-9._-]{0,63}$'
    ),
    CONSTRAINT ck_employment_policy_source_title CHECK (CHAR_LENGTH(title) > 0),
    CONSTRAINT ck_employment_policy_source_url CHECK (
        source_url REGEXP '^https://[^[:space:]]+$'
    ),
    CONSTRAINT ck_employment_policy_source_sha CHECK (
        content_sha256 REGEXP '^[0-9a-f]{64}$'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE national_pension_policy (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    employment_policy_set_id            BIGINT UNSIGNED NOT NULL,
    employment_policy_source_id         BIGINT UNSIGNED NOT NULL,
    effective_from                      DATE            NOT NULL,
    effective_to_exclusive              DATE                NULL,
    monthly_income_rounding_unit_krw    BIGINT          NOT NULL,
    minimum_monthly_income_krw          BIGINT          NOT NULL,
    maximum_monthly_income_krw          BIGINT          NOT NULL,
    employee_rate_ppm                   INT UNSIGNED    NOT NULL,
    employer_rate_ppm                   INT UNSIGNED    NOT NULL,
    employee_rounding_unit_krw          BIGINT          NOT NULL,
    employer_rounding_unit_krw          BIGINT          NOT NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_national_pension_policy_effective
        (employment_policy_set_id, effective_from),
    UNIQUE KEY uk_national_pension_policy_set_id
        (employment_policy_set_id, id),
    CONSTRAINT fk_national_pension_policy_set
        FOREIGN KEY (employment_policy_set_id) REFERENCES employment_policy_set (id),
    CONSTRAINT fk_national_pension_policy_source
        FOREIGN KEY (employment_policy_set_id, employment_policy_source_id)
        REFERENCES employment_policy_source (employment_policy_set_id, id),
    CONSTRAINT ck_national_pension_policy_period CHECK (
        effective_to_exclusive IS NULL OR effective_from < effective_to_exclusive
    ),
    CONSTRAINT ck_national_pension_policy_money CHECK (
        monthly_income_rounding_unit_krw > 0
        AND minimum_monthly_income_krw > 0
        AND maximum_monthly_income_krw >= minimum_monthly_income_krw
        AND maximum_monthly_income_krw <= 9007199254740991
        AND employee_rounding_unit_krw > 0
        AND employer_rounding_unit_krw > 0
    ),
    CONSTRAINT ck_national_pension_policy_rates CHECK (
        employee_rate_ppm BETWEEN 1 AND 1000000
        AND employer_rate_ppm BETWEEN 1 AND 1000000
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE health_insurance_policy (
    id                                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    employment_policy_set_id                    BIGINT UNSIGNED NOT NULL,
    employment_policy_source_id                 BIGINT UNSIGNED NOT NULL,
    effective_from                              DATE            NOT NULL,
    effective_to_exclusive                      DATE                NULL,
    monthly_remuneration_rounding_unit_krw      BIGINT          NOT NULL,
    employee_rate_ppm                           INT UNSIGNED    NOT NULL,
    employer_rate_ppm                           INT UNSIGNED    NOT NULL,
    employee_rounding_unit_krw                  BIGINT          NOT NULL,
    employer_rounding_unit_krw                  BIGINT          NOT NULL,
    created_at                                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_health_insurance_policy_effective
        (employment_policy_set_id, effective_from),
    UNIQUE KEY uk_health_insurance_policy_set_id
        (employment_policy_set_id, id),
    CONSTRAINT fk_health_insurance_policy_set
        FOREIGN KEY (employment_policy_set_id) REFERENCES employment_policy_set (id),
    CONSTRAINT fk_health_insurance_policy_source
        FOREIGN KEY (employment_policy_set_id, employment_policy_source_id)
        REFERENCES employment_policy_source (employment_policy_set_id, id),
    CONSTRAINT ck_health_insurance_policy_period CHECK (
        effective_to_exclusive IS NULL OR effective_from < effective_to_exclusive
    ),
    CONSTRAINT ck_health_insurance_policy_money CHECK (
        monthly_remuneration_rounding_unit_krw > 0
        AND employee_rounding_unit_krw > 0
        AND employer_rounding_unit_krw > 0
    ),
    CONSTRAINT ck_health_insurance_policy_rates CHECK (
        employee_rate_ppm BETWEEN 1 AND 1000000
        AND employer_rate_ppm BETWEEN 1 AND 1000000
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE long_term_care_policy (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    employment_policy_set_id            BIGINT UNSIGNED NOT NULL,
    employment_policy_source_id         BIGINT UNSIGNED NOT NULL,
    effective_from                      DATE            NOT NULL,
    effective_to_exclusive              DATE                NULL,
    health_premium_rate_numerator       BIGINT UNSIGNED NOT NULL,
    health_premium_rate_denominator     BIGINT UNSIGNED NOT NULL,
    employee_rounding_unit_krw          BIGINT          NOT NULL,
    employer_rounding_unit_krw          BIGINT          NOT NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_long_term_care_policy_effective
        (employment_policy_set_id, effective_from),
    UNIQUE KEY uk_long_term_care_policy_set_id
        (employment_policy_set_id, id),
    CONSTRAINT fk_long_term_care_policy_set
        FOREIGN KEY (employment_policy_set_id) REFERENCES employment_policy_set (id),
    CONSTRAINT fk_long_term_care_policy_source
        FOREIGN KEY (employment_policy_set_id, employment_policy_source_id)
        REFERENCES employment_policy_source (employment_policy_set_id, id),
    CONSTRAINT ck_long_term_care_policy_period CHECK (
        effective_to_exclusive IS NULL OR effective_from < effective_to_exclusive
    ),
    CONSTRAINT ck_long_term_care_policy_rate CHECK (
        health_premium_rate_numerator > 0
        AND health_premium_rate_denominator > 0
        AND health_premium_rate_numerator <= health_premium_rate_denominator
    ),
    CONSTRAINT ck_long_term_care_policy_rounding CHECK (
        employee_rounding_unit_krw > 0 AND employer_rounding_unit_krw > 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE employment_insurance_policy (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    employment_policy_set_id            BIGINT UNSIGNED NOT NULL,
    employment_policy_source_id         BIGINT UNSIGNED NOT NULL,
    effective_from                      DATE            NOT NULL,
    effective_to_exclusive              DATE                NULL,
    employee_rate_ppm                   INT UNSIGNED    NOT NULL,
    employee_rounding_unit_krw          BIGINT          NOT NULL,
    employer_rounding_unit_krw          BIGINT          NOT NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_employment_insurance_policy_effective
        (employment_policy_set_id, effective_from),
    UNIQUE KEY uk_employment_insurance_policy_set_id
        (employment_policy_set_id, id),
    CONSTRAINT fk_employment_insurance_policy_set
        FOREIGN KEY (employment_policy_set_id) REFERENCES employment_policy_set (id),
    CONSTRAINT fk_employment_insurance_policy_source
        FOREIGN KEY (employment_policy_set_id, employment_policy_source_id)
        REFERENCES employment_policy_source (employment_policy_set_id, id),
    CONSTRAINT ck_employment_insurance_policy_period CHECK (
        effective_to_exclusive IS NULL OR effective_from < effective_to_exclusive
    ),
    CONSTRAINT ck_employment_insurance_policy_rate CHECK (
        employee_rate_ppm BETWEEN 1 AND 1000000
    ),
    CONSTRAINT ck_employment_insurance_policy_rounding CHECK (
        employee_rounding_unit_krw > 0 AND employer_rounding_unit_krw > 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE employment_insurance_employer_rate (
    employment_policy_set_id            BIGINT UNSIGNED NOT NULL,
    employment_insurance_policy_id      BIGINT UNSIGNED NOT NULL,
    employer_size_band                  VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    employer_rate_ppm                   INT UNSIGNED NOT NULL,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (employment_insurance_policy_id, employer_size_band),
    CONSTRAINT fk_employment_insurance_employer_rate_policy
        FOREIGN KEY (employment_policy_set_id, employment_insurance_policy_id)
        REFERENCES employment_insurance_policy (employment_policy_set_id, id),
    CONSTRAINT ck_employment_insurance_employer_rate_band CHECK (
        employer_size_band IN ('under150', 'from150To999', 'atLeast1000', 'government')
    ),
    CONSTRAINT ck_employment_insurance_employer_rate_value CHECK (
        employer_rate_ppm BETWEEN 1 AND 1000000
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE industrial_accident_policy (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    employment_policy_set_id            BIGINT UNSIGNED NOT NULL,
    employment_policy_source_id         BIGINT UNSIGNED NOT NULL,
    effective_from                      DATE            NOT NULL,
    effective_to_exclusive              DATE                NULL,
    industry_key                        VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    employer_rate_ppm                   INT UNSIGNED    NOT NULL,
    employer_rounding_unit_krw          BIGINT          NOT NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_industrial_accident_policy_effective
        (employment_policy_set_id, industry_key, effective_from),
    UNIQUE KEY uk_industrial_accident_policy_set_id
        (employment_policy_set_id, id),
    CONSTRAINT fk_industrial_accident_policy_set
        FOREIGN KEY (employment_policy_set_id) REFERENCES employment_policy_set (id),
    CONSTRAINT fk_industrial_accident_policy_source
        FOREIGN KEY (employment_policy_set_id, employment_policy_source_id)
        REFERENCES employment_policy_source (employment_policy_set_id, id),
    CONSTRAINT ck_industrial_accident_policy_period CHECK (
        effective_to_exclusive IS NULL OR effective_from < effective_to_exclusive
    ),
    CONSTRAINT ck_industrial_accident_policy_industry CHECK (
        industry_key IN (
            'itSoftware', 'financeInsurance', 'manufacturing',
            'constructionEngineering', 'retailService', 'publicSocial'
        )
    ),
    CONSTRAINT ck_industrial_accident_policy_values CHECK (
        employer_rate_ppm BETWEEN 1 AND 1000000
        AND employer_rounding_unit_krw > 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE employment_withholding_table_version (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    employment_policy_set_id            BIGINT UNSIGNED NOT NULL,
    employment_policy_source_id         BIGINT UNSIGNED NOT NULL,
    table_key                           VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    effective_from                      DATE            NOT NULL,
    effective_to_exclusive              DATE                NULL,
    withholding_percentage_bp           SMALLINT UNSIGNED NOT NULL DEFAULT 10000,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_employment_withholding_version_key
        (employment_policy_set_id, table_key),
    UNIQUE KEY uk_employment_withholding_version_effective
        (employment_policy_set_id, effective_from),
    UNIQUE KEY uk_employment_withholding_version_set_id
        (employment_policy_set_id, id),
    CONSTRAINT fk_employment_withholding_version_set
        FOREIGN KEY (employment_policy_set_id) REFERENCES employment_policy_set (id),
    CONSTRAINT fk_employment_withholding_version_source
        FOREIGN KEY (employment_policy_set_id, employment_policy_source_id)
        REFERENCES employment_policy_source (employment_policy_set_id, id),
    CONSTRAINT ck_employment_withholding_version_key CHECK (
        CHAR_LENGTH(table_key) > 0
        AND table_key REGEXP '^[a-z0-9][a-z0-9._-]{0,63}$'
    ),
    CONSTRAINT ck_employment_withholding_version_period CHECK (
        effective_to_exclusive IS NULL OR effective_from < effective_to_exclusive
    ),
    CONSTRAINT ck_employment_withholding_version_percentage CHECK (
        withholding_percentage_bp = 10000
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE employment_withholding_table_row (
    id                                      BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    employment_policy_set_id                BIGINT UNSIGNED NOT NULL,
    employment_withholding_table_version_id BIGINT UNSIGNED NOT NULL,
    lower_bound_krw                         BIGINT          NOT NULL,
    upper_bound_exclusive_krw               BIGINT              NULL,
    family_count                            TINYINT UNSIGNED NOT NULL,
    child_count                             TINYINT UNSIGNED NOT NULL DEFAULT 0,
    income_tax_krw                          BIGINT          NOT NULL,
    created_at                              DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_employment_withholding_row_band (
        employment_withholding_table_version_id,
        family_count,
        child_count,
        lower_bound_krw
    ),
    UNIQUE KEY uk_employment_withholding_row_set_id
        (employment_policy_set_id, id),
    KEY ix_employment_withholding_row_lookup (
        employment_withholding_table_version_id,
        family_count,
        child_count,
        lower_bound_krw,
        upper_bound_exclusive_krw
    ),
    CONSTRAINT fk_employment_withholding_row_version
        FOREIGN KEY (employment_policy_set_id, employment_withholding_table_version_id)
        REFERENCES employment_withholding_table_version (employment_policy_set_id, id),
    CONSTRAINT ck_employment_withholding_row_family CHECK (
        family_count BETWEEN 1 AND 7 AND child_count = 0
    ),
    CONSTRAINT ck_employment_withholding_row_band CHECK (
        lower_bound_krw >= 0
        AND (
            upper_bound_exclusive_krw IS NULL
            OR upper_bound_exclusive_krw > lower_bound_krw
        )
    ),
    CONSTRAINT ck_employment_withholding_row_tax CHECK (
        income_tax_krw BETWEEN 0 AND 9007199254740991
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE local_income_withholding_policy (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    employment_policy_set_id            BIGINT UNSIGNED NOT NULL,
    employment_policy_source_id         BIGINT UNSIGNED NOT NULL,
    effective_from                      DATE            NOT NULL,
    effective_to_exclusive              DATE                NULL,
    income_tax_rate_ppm                 INT UNSIGNED    NOT NULL,
    rounding_unit_krw                   BIGINT          NOT NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_local_income_withholding_policy_effective
        (employment_policy_set_id, effective_from),
    UNIQUE KEY uk_local_income_withholding_policy_set_id
        (employment_policy_set_id, id),
    CONSTRAINT fk_local_income_withholding_policy_set
        FOREIGN KEY (employment_policy_set_id) REFERENCES employment_policy_set (id),
    CONSTRAINT fk_local_income_withholding_policy_source
        FOREIGN KEY (employment_policy_set_id, employment_policy_source_id)
        REFERENCES employment_policy_source (employment_policy_set_id, id),
    CONSTRAINT ck_local_income_withholding_policy_period CHECK (
        effective_to_exclusive IS NULL OR effective_from < effective_to_exclusive
    ),
    CONSTRAINT ck_local_income_withholding_policy_values CHECK (
        income_tax_rate_ppm BETWEEN 1 AND 1000000 AND rounding_unit_krw > 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE employment_annual_tax_policy (
    id                                      BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    employment_policy_set_id                BIGINT UNSIGNED NOT NULL,
    employment_policy_source_id             BIGINT UNSIGNED NOT NULL,
    tax_year                                SMALLINT UNSIGNED NOT NULL,
    basic_personal_deduction_krw            BIGINT          NOT NULL,
    taxable_income_rounding_unit_krw        BIGINT          NOT NULL,
    calculated_tax_rounding_unit_krw        BIGINT          NOT NULL,
    income_tax_credit_low_tax_boundary_krw  BIGINT          NOT NULL,
    income_tax_credit_low_rate_ppm          INT UNSIGNED    NOT NULL,
    income_tax_credit_high_base_krw         BIGINT          NOT NULL,
    income_tax_credit_high_rate_ppm         INT UNSIGNED    NOT NULL,
    credit_cap_salary_boundary_one_krw      BIGINT          NOT NULL,
    credit_cap_salary_boundary_two_krw      BIGINT          NOT NULL,
    credit_cap_one_krw                      BIGINT          NOT NULL,
    credit_cap_two_base_krw                 BIGINT          NOT NULL,
    credit_cap_two_reduction_rate_ppm       INT UNSIGNED    NOT NULL,
    credit_cap_two_floor_krw                BIGINT          NOT NULL,
    credit_cap_three_base_krw               BIGINT          NOT NULL,
    credit_cap_three_reduction_rate_ppm     INT UNSIGNED    NOT NULL,
    credit_cap_three_floor_krw              BIGINT          NOT NULL,
    created_at                              DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_employment_annual_tax_policy_year
        (employment_policy_set_id, tax_year),
    UNIQUE KEY uk_employment_annual_tax_policy_set_id
        (employment_policy_set_id, id),
    CONSTRAINT fk_employment_annual_tax_policy_set
        FOREIGN KEY (employment_policy_set_id) REFERENCES employment_policy_set (id),
    CONSTRAINT fk_employment_annual_tax_policy_source
        FOREIGN KEY (employment_policy_set_id, employment_policy_source_id)
        REFERENCES employment_policy_source (employment_policy_set_id, id),
    CONSTRAINT ck_employment_annual_tax_policy_year CHECK (tax_year BETWEEN 1 AND 9999),
    CONSTRAINT ck_employment_annual_tax_policy_values CHECK (
        basic_personal_deduction_krw >= 0
        AND taxable_income_rounding_unit_krw > 0
        AND calculated_tax_rounding_unit_krw > 0
        AND income_tax_credit_low_tax_boundary_krw > 0
        AND income_tax_credit_low_rate_ppm BETWEEN 1 AND 1000000
        AND income_tax_credit_high_base_krw >= 0
        AND income_tax_credit_high_rate_ppm BETWEEN 1 AND 1000000
        AND credit_cap_salary_boundary_one_krw > 0
        AND credit_cap_salary_boundary_two_krw > credit_cap_salary_boundary_one_krw
        AND credit_cap_one_krw >= 0
        AND credit_cap_two_base_krw >= credit_cap_two_floor_krw
        AND credit_cap_two_reduction_rate_ppm BETWEEN 1 AND 1000000
        AND credit_cap_two_floor_krw >= 0
        AND credit_cap_three_base_krw >= credit_cap_three_floor_krw
        AND credit_cap_three_reduction_rate_ppm BETWEEN 1 AND 1000000
        AND credit_cap_three_floor_krw >= 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE employment_income_deduction_bracket (
    employment_policy_set_id            BIGINT UNSIGNED NOT NULL,
    employment_annual_tax_policy_id      BIGINT UNSIGNED NOT NULL,
    bracket_order                       TINYINT UNSIGNED NOT NULL,
    lower_bound_krw                     BIGINT          NOT NULL,
    upper_bound_exclusive_krw           BIGINT              NULL,
    base_deduction_krw                  BIGINT          NOT NULL,
    marginal_rate_ppm                   INT UNSIGNED    NOT NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (employment_annual_tax_policy_id, bracket_order),
    UNIQUE KEY uk_employment_income_deduction_lower
        (employment_annual_tax_policy_id, lower_bound_krw),
    CONSTRAINT fk_employment_income_deduction_policy
        FOREIGN KEY (employment_policy_set_id, employment_annual_tax_policy_id)
        REFERENCES employment_annual_tax_policy (employment_policy_set_id, id),
    CONSTRAINT ck_employment_income_deduction_order CHECK (bracket_order > 0),
    CONSTRAINT ck_employment_income_deduction_band CHECK (
        lower_bound_krw >= 0
        AND (upper_bound_exclusive_krw IS NULL OR upper_bound_exclusive_krw > lower_bound_krw)
        AND base_deduction_krw >= 0
        AND marginal_rate_ppm BETWEEN 1 AND 1000000
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE employment_income_tax_bracket (
    employment_policy_set_id            BIGINT UNSIGNED NOT NULL,
    employment_annual_tax_policy_id      BIGINT UNSIGNED NOT NULL,
    bracket_order                       TINYINT UNSIGNED NOT NULL,
    lower_bound_krw                     BIGINT          NOT NULL,
    upper_bound_exclusive_krw           BIGINT              NULL,
    rate_ppm                            INT UNSIGNED    NOT NULL,
    quick_deduction_krw                 BIGINT          NOT NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (employment_annual_tax_policy_id, bracket_order),
    UNIQUE KEY uk_employment_income_tax_lower
        (employment_annual_tax_policy_id, lower_bound_krw),
    CONSTRAINT fk_employment_income_tax_bracket_policy
        FOREIGN KEY (employment_policy_set_id, employment_annual_tax_policy_id)
        REFERENCES employment_annual_tax_policy (employment_policy_set_id, id),
    CONSTRAINT ck_employment_income_tax_bracket_order CHECK (bracket_order > 0),
    CONSTRAINT ck_employment_income_tax_bracket_band CHECK (
        lower_bound_krw >= 0
        AND (upper_bound_exclusive_krw IS NULL OR upper_bound_exclusive_krw > lower_bound_krw)
        AND rate_ppm BETWEEN 1 AND 1000000
        AND quick_deduction_krw >= 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE other_income_reward_policy (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    employment_policy_set_id            BIGINT UNSIGNED NOT NULL,
    employment_policy_source_id         BIGINT UNSIGNED NOT NULL,
    effective_from                      DATE            NOT NULL,
    effective_to_exclusive              DATE                NULL,
    income_tax_rate_ppm                 INT UNSIGNED    NOT NULL,
    local_income_tax_rate_ppm           INT UNSIGNED    NOT NULL,
    income_tax_rounding_unit_krw        BIGINT          NOT NULL,
    local_income_tax_rounding_unit_krw  BIGINT          NOT NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_other_income_reward_policy_effective
        (employment_policy_set_id, effective_from),
    UNIQUE KEY uk_other_income_reward_policy_set_id
        (employment_policy_set_id, id),
    CONSTRAINT fk_other_income_reward_policy_set
        FOREIGN KEY (employment_policy_set_id) REFERENCES employment_policy_set (id),
    CONSTRAINT fk_other_income_reward_policy_source
        FOREIGN KEY (employment_policy_set_id, employment_policy_source_id)
        REFERENCES employment_policy_source (employment_policy_set_id, id),
    CONSTRAINT ck_other_income_reward_policy_period CHECK (
        effective_to_exclusive IS NULL OR effective_from < effective_to_exclusive
    ),
    CONSTRAINT ck_other_income_reward_policy_values CHECK (
        income_tax_rate_ppm BETWEEN 1 AND 1000000
        AND local_income_tax_rate_ppm BETWEEN 1 AND 1000000
        AND income_tax_rounding_unit_krw > 0
        AND local_income_tax_rounding_unit_krw > 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE employment_finance_compatibility (
    employment_policy_set_id            BIGINT UNSIGNED NOT NULL,
    policy_set_id                       BIGINT UNSIGNED NOT NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (employment_policy_set_id, policy_set_id),
    KEY ix_employment_finance_compatibility_finance (policy_set_id),
    CONSTRAINT fk_employment_finance_compatibility_employment
        FOREIGN KEY (employment_policy_set_id) REFERENCES employment_policy_set (id),
    CONSTRAINT fk_employment_finance_compatibility_finance
        FOREIGN KEY (policy_set_id) REFERENCES policy_set (id)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE employment_policy_assignment (
    assignment_key                  VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    employment_policy_set_id        BIGINT UNSIGNED NOT NULL,
    assignment_revision             BIGINT UNSIGNED NOT NULL DEFAULT 1,
    updated_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (assignment_key),
    KEY ix_employment_policy_assignment_set (employment_policy_set_id),
    CONSTRAINT fk_employment_policy_assignment_set
        FOREIGN KEY (employment_policy_set_id) REFERENCES employment_policy_set (id),
    CONSTRAINT ck_employment_policy_assignment_key CHECK (assignment_key = 'newRun')
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_employment_policy_set_draft_insert
BEFORE INSERT ON employment_policy_set
FOR EACH ROW
SET NEW.policy_key = IF(NEW.published_at IS NULL, NEW.policy_key, NULL);

-- Publication validates the complete typed graph. Date rows are insert-checked for overlap and
-- containment, so an exact summed coverage length also proves that there is no gap.
CREATE TRIGGER tr_employment_policy_set_publish_only
BEFORE UPDATE ON employment_policy_set
FOR EACH ROW
SET NEW.policy_key = IF(
    OLD.published_at IS NULL
        AND NEW.published_at IS NOT NULL
        AND NEW.id = OLD.id
        AND BINARY NEW.policy_key = BINARY OLD.policy_key
        AND NEW.ranked_eligible = OLD.ranked_eligible
        AND NEW.coverage_start = OLD.coverage_start
        AND NEW.coverage_end_exclusive = OLD.coverage_end_exclusive
        AND NEW.created_at = OLD.created_at
        AND (NEW.ranked_eligible = FALSE OR NEW.policy_key NOT LIKE 'dev-unranked-%')
        AND EXISTS (
            SELECT 1
            FROM employment_finance_compatibility AS compatibility
            INNER JOIN policy_set AS finance_policy
                ON finance_policy.id = compatibility.policy_set_id
               AND finance_policy.sealed_at IS NOT NULL
            WHERE compatibility.employment_policy_set_id = OLD.id
        )
        AND NOT EXISTS (
            SELECT 1
            FROM employment_policy_source AS source
            WHERE source.employment_policy_set_id = OLD.id
              AND (
                  CHAR_LENGTH(source.source_url) = 0
                  OR source.verified_on > CURRENT_DATE()
                  OR source.content_sha256 NOT REGEXP '^[0-9a-f]{64}$'
              )
        )
        AND (
            SELECT COALESCE(SUM(DATEDIFF(
                COALESCE(policy.effective_to_exclusive, OLD.coverage_end_exclusive),
                policy.effective_from
            )), 0)
            FROM national_pension_policy AS policy
            WHERE policy.employment_policy_set_id = OLD.id
        ) = DATEDIFF(OLD.coverage_end_exclusive, OLD.coverage_start)
        AND (
            SELECT COALESCE(SUM(DATEDIFF(
                COALESCE(policy.effective_to_exclusive, OLD.coverage_end_exclusive),
                policy.effective_from
            )), 0)
            FROM health_insurance_policy AS policy
            WHERE policy.employment_policy_set_id = OLD.id
        ) = DATEDIFF(OLD.coverage_end_exclusive, OLD.coverage_start)
        AND (
            SELECT COALESCE(SUM(DATEDIFF(
                COALESCE(policy.effective_to_exclusive, OLD.coverage_end_exclusive),
                policy.effective_from
            )), 0)
            FROM long_term_care_policy AS policy
            WHERE policy.employment_policy_set_id = OLD.id
        ) = DATEDIFF(OLD.coverage_end_exclusive, OLD.coverage_start)
        AND (
            SELECT COALESCE(SUM(DATEDIFF(
                COALESCE(policy.effective_to_exclusive, OLD.coverage_end_exclusive),
                policy.effective_from
            )), 0)
            FROM employment_insurance_policy AS policy
            WHERE policy.employment_policy_set_id = OLD.id
        ) = DATEDIFF(OLD.coverage_end_exclusive, OLD.coverage_start)
        AND NOT EXISTS (
            SELECT 1
            FROM employment_insurance_policy AS policy
            WHERE policy.employment_policy_set_id = OLD.id
              AND (
                  SELECT COUNT(*)
                  FROM employment_insurance_employer_rate AS rate
                  WHERE rate.employment_policy_set_id = OLD.id
                    AND rate.employment_insurance_policy_id = policy.id
              ) <> 4
        )
        AND (
            SELECT COUNT(DISTINCT policy.industry_key)
            FROM industrial_accident_policy AS policy
            WHERE policy.employment_policy_set_id = OLD.id
        ) = 6
        AND (
            SELECT COALESCE(SUM(DATEDIFF(
                COALESCE(policy.effective_to_exclusive, OLD.coverage_end_exclusive),
                policy.effective_from
            )), 0)
            FROM industrial_accident_policy AS policy
            WHERE policy.employment_policy_set_id = OLD.id
        ) = 6 * DATEDIFF(OLD.coverage_end_exclusive, OLD.coverage_start)
        AND (
            SELECT COALESCE(SUM(DATEDIFF(
                COALESCE(version.effective_to_exclusive, OLD.coverage_end_exclusive),
                version.effective_from
            )), 0)
            FROM employment_withholding_table_version AS version
            WHERE version.employment_policy_set_id = OLD.id
        ) = DATEDIFF(OLD.coverage_end_exclusive, OLD.coverage_start)
        AND NOT EXISTS (
            SELECT 1
            FROM employment_withholding_table_version AS version
            WHERE version.employment_policy_set_id = OLD.id
              AND (
                  (SELECT COUNT(DISTINCT withholding_row.family_count)
                   FROM employment_withholding_table_row AS withholding_row
                   WHERE withholding_row.employment_withholding_table_version_id = version.id
                     AND withholding_row.child_count = 0) <> 7
                  OR EXISTS (
                      SELECT 1
                      FROM employment_withholding_table_row AS withholding_row
                      WHERE withholding_row.employment_withholding_table_version_id = version.id
                        AND withholding_row.lower_bound_krw = 0
                      GROUP BY withholding_row.family_count, withholding_row.child_count
                      HAVING COUNT(*) <> 1
                  )
                  OR (SELECT COUNT(*)
                      FROM employment_withholding_table_row AS withholding_row
                      WHERE withholding_row.employment_withholding_table_version_id = version.id
                        AND withholding_row.lower_bound_krw = 0) <> 7
                  OR (SELECT COUNT(*)
                      FROM employment_withholding_table_row AS withholding_row
                      WHERE withholding_row.employment_withholding_table_version_id = version.id
                        AND withholding_row.upper_bound_exclusive_krw IS NULL) <> 7
                  OR EXISTS (
                      SELECT 1
                      FROM employment_withholding_table_row AS withholding_row
                      WHERE withholding_row.employment_withholding_table_version_id = version.id
                        AND withholding_row.upper_bound_exclusive_krw IS NOT NULL
                        AND NOT EXISTS (
                            SELECT 1
                            FROM employment_withholding_table_row AS next_row
                            WHERE next_row.employment_withholding_table_version_id = version.id
                              AND next_row.family_count = withholding_row.family_count
                              AND next_row.child_count = withholding_row.child_count
                              AND next_row.lower_bound_krw
                                  = withholding_row.upper_bound_exclusive_krw
                        )
                  )
                  OR EXISTS (
                      SELECT 1
                      FROM employment_withholding_table_row AS withholding_row
                      WHERE withholding_row.employment_withholding_table_version_id = version.id
                        AND withholding_row.lower_bound_krw > 0
                        AND NOT EXISTS (
                            SELECT 1
                            FROM employment_withholding_table_row AS previous_row
                            WHERE previous_row.employment_withholding_table_version_id = version.id
                              AND previous_row.family_count = withholding_row.family_count
                              AND previous_row.child_count = withholding_row.child_count
                              AND previous_row.upper_bound_exclusive_krw
                                  = withholding_row.lower_bound_krw
                        )
                  )
              )
        )
        AND (
            SELECT COALESCE(SUM(DATEDIFF(
                COALESCE(policy.effective_to_exclusive, OLD.coverage_end_exclusive),
                policy.effective_from
            )), 0)
            FROM local_income_withholding_policy AS policy
            WHERE policy.employment_policy_set_id = OLD.id
        ) = DATEDIFF(OLD.coverage_end_exclusive, OLD.coverage_start)
        AND (
            SELECT COALESCE(SUM(DATEDIFF(
                COALESCE(policy.effective_to_exclusive, OLD.coverage_end_exclusive),
                policy.effective_from
            )), 0)
            FROM other_income_reward_policy AS policy
            WHERE policy.employment_policy_set_id = OLD.id
        ) = DATEDIFF(OLD.coverage_end_exclusive, OLD.coverage_start)
        AND (
            (
                OLD.ranked_eligible = TRUE
                AND (
                    SELECT COUNT(*)
                    FROM employment_annual_tax_policy AS annual_policy
                    WHERE annual_policy.employment_policy_set_id = OLD.id
                      AND annual_policy.tax_year BETWEEN YEAR(OLD.coverage_start)
                          AND YEAR(DATE_SUB(OLD.coverage_end_exclusive, INTERVAL 1 DAY))
                ) = YEAR(DATE_SUB(OLD.coverage_end_exclusive, INTERVAL 1 DAY))
                    - YEAR(OLD.coverage_start) + 1
            )
            OR (
                OLD.ranked_eligible = FALSE
                AND OLD.policy_key LIKE 'dev-unranked-%'
                AND EXISTS (
                    SELECT 1
                    FROM employment_annual_tax_policy AS annual_policy
                    WHERE annual_policy.employment_policy_set_id = OLD.id
                      AND annual_policy.tax_year = YEAR(OLD.coverage_start)
                )
            )
        )
        AND NOT EXISTS (
            SELECT 1
            FROM employment_annual_tax_policy AS annual_policy
            WHERE annual_policy.employment_policy_set_id = OLD.id
              AND (
                  NOT EXISTS (
                      SELECT 1
                      FROM employment_income_deduction_bracket AS bracket
                      WHERE bracket.employment_annual_tax_policy_id = annual_policy.id
                        AND bracket.lower_bound_krw = 0
                  )
                  OR (SELECT COUNT(*)
                      FROM employment_income_deduction_bracket AS bracket
                      WHERE bracket.employment_annual_tax_policy_id = annual_policy.id
                        AND bracket.upper_bound_exclusive_krw IS NULL) <> 1
                  OR EXISTS (
                      SELECT 1
                      FROM employment_income_deduction_bracket AS bracket
                      WHERE bracket.employment_annual_tax_policy_id = annual_policy.id
                        AND bracket.upper_bound_exclusive_krw IS NOT NULL
                        AND NOT EXISTS (
                            SELECT 1
                            FROM employment_income_deduction_bracket AS next_bracket
                            WHERE next_bracket.employment_annual_tax_policy_id = annual_policy.id
                              AND next_bracket.lower_bound_krw
                                  = bracket.upper_bound_exclusive_krw
                        )
                  )
                  OR NOT EXISTS (
                      SELECT 1
                      FROM employment_income_tax_bracket AS bracket
                      WHERE bracket.employment_annual_tax_policy_id = annual_policy.id
                        AND bracket.lower_bound_krw = 0
                  )
                  OR (SELECT COUNT(*)
                      FROM employment_income_tax_bracket AS bracket
                      WHERE bracket.employment_annual_tax_policy_id = annual_policy.id
                        AND bracket.upper_bound_exclusive_krw IS NULL) <> 1
                  OR EXISTS (
                      SELECT 1
                      FROM employment_income_tax_bracket AS bracket
                      WHERE bracket.employment_annual_tax_policy_id = annual_policy.id
                        AND bracket.upper_bound_exclusive_krw IS NOT NULL
                        AND NOT EXISTS (
                            SELECT 1
                            FROM employment_income_tax_bracket AS next_bracket
                            WHERE next_bracket.employment_annual_tax_policy_id = annual_policy.id
                              AND next_bracket.lower_bound_krw
                                  = bracket.upper_bound_exclusive_krw
                        )
                  )
              )
        ),
    OLD.policy_key,
    NULL
);

CREATE TRIGGER tr_employment_policy_set_no_delete
BEFORE DELETE ON employment_policy_set
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment policy sets cannot be deleted';

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

CREATE TRIGGER tr_national_pension_policy_draft_insert
BEFORE INSERT ON national_pension_policy
FOR EACH ROW
SET NEW.employment_policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM employment_policy_set AS policy_set
        WHERE policy_set.id = NEW.employment_policy_set_id
          AND policy_set.published_at IS NULL
          AND NEW.effective_from >= policy_set.coverage_start
          AND COALESCE(NEW.effective_to_exclusive, policy_set.coverage_end_exclusive)
              <= policy_set.coverage_end_exclusive
    )
    AND NOT EXISTS (
        SELECT 1 FROM national_pension_policy AS existing
        WHERE existing.employment_policy_set_id = NEW.employment_policy_set_id
          AND NOT (
              COALESCE(existing.effective_to_exclusive, '9999-12-31') <= NEW.effective_from
              OR COALESCE(NEW.effective_to_exclusive, '9999-12-31')
                  <= existing.effective_from
          )
    ),
    NEW.employment_policy_set_id,
    NULL
);

CREATE TRIGGER tr_health_insurance_policy_draft_insert
BEFORE INSERT ON health_insurance_policy
FOR EACH ROW
SET NEW.employment_policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM employment_policy_set AS policy_set
        WHERE policy_set.id = NEW.employment_policy_set_id
          AND policy_set.published_at IS NULL
          AND NEW.effective_from >= policy_set.coverage_start
          AND COALESCE(NEW.effective_to_exclusive, policy_set.coverage_end_exclusive)
              <= policy_set.coverage_end_exclusive
    )
    AND NOT EXISTS (
        SELECT 1 FROM health_insurance_policy AS existing
        WHERE existing.employment_policy_set_id = NEW.employment_policy_set_id
          AND NOT (
              COALESCE(existing.effective_to_exclusive, '9999-12-31') <= NEW.effective_from
              OR COALESCE(NEW.effective_to_exclusive, '9999-12-31')
                  <= existing.effective_from
          )
    ),
    NEW.employment_policy_set_id,
    NULL
);

CREATE TRIGGER tr_long_term_care_policy_draft_insert
BEFORE INSERT ON long_term_care_policy
FOR EACH ROW
SET NEW.employment_policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM employment_policy_set AS policy_set
        WHERE policy_set.id = NEW.employment_policy_set_id
          AND policy_set.published_at IS NULL
          AND NEW.effective_from >= policy_set.coverage_start
          AND COALESCE(NEW.effective_to_exclusive, policy_set.coverage_end_exclusive)
              <= policy_set.coverage_end_exclusive
    )
    AND NOT EXISTS (
        SELECT 1 FROM long_term_care_policy AS existing
        WHERE existing.employment_policy_set_id = NEW.employment_policy_set_id
          AND NOT (
              COALESCE(existing.effective_to_exclusive, '9999-12-31') <= NEW.effective_from
              OR COALESCE(NEW.effective_to_exclusive, '9999-12-31')
                  <= existing.effective_from
          )
    ),
    NEW.employment_policy_set_id,
    NULL
);

CREATE TRIGGER tr_employment_insurance_policy_draft_insert
BEFORE INSERT ON employment_insurance_policy
FOR EACH ROW
SET NEW.employment_policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM employment_policy_set AS policy_set
        WHERE policy_set.id = NEW.employment_policy_set_id
          AND policy_set.published_at IS NULL
          AND NEW.effective_from >= policy_set.coverage_start
          AND COALESCE(NEW.effective_to_exclusive, policy_set.coverage_end_exclusive)
              <= policy_set.coverage_end_exclusive
    )
    AND NOT EXISTS (
        SELECT 1 FROM employment_insurance_policy AS existing
        WHERE existing.employment_policy_set_id = NEW.employment_policy_set_id
          AND NOT (
              COALESCE(existing.effective_to_exclusive, '9999-12-31') <= NEW.effective_from
              OR COALESCE(NEW.effective_to_exclusive, '9999-12-31')
                  <= existing.effective_from
          )
    ),
    NEW.employment_policy_set_id,
    NULL
);

CREATE TRIGGER tr_employment_insurance_rate_draft_insert
BEFORE INSERT ON employment_insurance_employer_rate
FOR EACH ROW
SET NEW.employment_policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM employment_insurance_policy AS policy
        INNER JOIN employment_policy_set AS policy_set
            ON policy_set.id = policy.employment_policy_set_id
           AND policy_set.published_at IS NULL
        WHERE policy.employment_policy_set_id = NEW.employment_policy_set_id
          AND policy.id = NEW.employment_insurance_policy_id
    ),
    NEW.employment_policy_set_id,
    NULL
);

CREATE TRIGGER tr_industrial_accident_policy_draft_insert
BEFORE INSERT ON industrial_accident_policy
FOR EACH ROW
SET NEW.employment_policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM employment_policy_set AS policy_set
        WHERE policy_set.id = NEW.employment_policy_set_id
          AND policy_set.published_at IS NULL
          AND NEW.effective_from >= policy_set.coverage_start
          AND COALESCE(NEW.effective_to_exclusive, policy_set.coverage_end_exclusive)
              <= policy_set.coverage_end_exclusive
    )
    AND NOT EXISTS (
        SELECT 1 FROM industrial_accident_policy AS existing
        WHERE existing.employment_policy_set_id = NEW.employment_policy_set_id
          AND BINARY existing.industry_key = BINARY NEW.industry_key
          AND NOT (
              COALESCE(existing.effective_to_exclusive, '9999-12-31') <= NEW.effective_from
              OR COALESCE(NEW.effective_to_exclusive, '9999-12-31')
                  <= existing.effective_from
          )
    ),
    NEW.employment_policy_set_id,
    NULL
);

CREATE TRIGGER tr_employment_withholding_version_draft_insert
BEFORE INSERT ON employment_withholding_table_version
FOR EACH ROW
SET NEW.employment_policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM employment_policy_set AS policy_set
        WHERE policy_set.id = NEW.employment_policy_set_id
          AND policy_set.published_at IS NULL
          AND NEW.effective_from >= policy_set.coverage_start
          AND COALESCE(NEW.effective_to_exclusive, policy_set.coverage_end_exclusive)
              <= policy_set.coverage_end_exclusive
    )
    AND NOT EXISTS (
        SELECT 1 FROM employment_withholding_table_version AS existing
        WHERE existing.employment_policy_set_id = NEW.employment_policy_set_id
          AND NOT (
              COALESCE(existing.effective_to_exclusive, '9999-12-31') <= NEW.effective_from
              OR COALESCE(NEW.effective_to_exclusive, '9999-12-31')
                  <= existing.effective_from
          )
    ),
    NEW.employment_policy_set_id,
    NULL
);

CREATE TRIGGER tr_employment_withholding_row_draft_insert
BEFORE INSERT ON employment_withholding_table_row
FOR EACH ROW
SET NEW.employment_policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM employment_withholding_table_version AS version
        INNER JOIN employment_policy_set AS policy_set
            ON policy_set.id = version.employment_policy_set_id
           AND policy_set.published_at IS NULL
        WHERE version.employment_policy_set_id = NEW.employment_policy_set_id
          AND version.id = NEW.employment_withholding_table_version_id
    )
    AND NOT EXISTS (
        SELECT 1
        FROM employment_withholding_table_row AS existing
        WHERE existing.employment_withholding_table_version_id
                = NEW.employment_withholding_table_version_id
          AND existing.family_count = NEW.family_count
          AND existing.child_count = NEW.child_count
          AND NOT (
              COALESCE(existing.upper_bound_exclusive_krw, 9007199254740991)
                  <= NEW.lower_bound_krw
              OR COALESCE(NEW.upper_bound_exclusive_krw, 9007199254740991)
                  <= existing.lower_bound_krw
          )
    ),
    NEW.employment_policy_set_id,
    NULL
);

CREATE TRIGGER tr_local_income_withholding_policy_draft_insert
BEFORE INSERT ON local_income_withholding_policy
FOR EACH ROW
SET NEW.employment_policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM employment_policy_set AS policy_set
        WHERE policy_set.id = NEW.employment_policy_set_id
          AND policy_set.published_at IS NULL
          AND NEW.effective_from >= policy_set.coverage_start
          AND COALESCE(NEW.effective_to_exclusive, policy_set.coverage_end_exclusive)
              <= policy_set.coverage_end_exclusive
    )
    AND NOT EXISTS (
        SELECT 1 FROM local_income_withholding_policy AS existing
        WHERE existing.employment_policy_set_id = NEW.employment_policy_set_id
          AND NOT (
              COALESCE(existing.effective_to_exclusive, '9999-12-31') <= NEW.effective_from
              OR COALESCE(NEW.effective_to_exclusive, '9999-12-31')
                  <= existing.effective_from
          )
    ),
    NEW.employment_policy_set_id,
    NULL
);

CREATE TRIGGER tr_employment_annual_tax_policy_draft_insert
BEFORE INSERT ON employment_annual_tax_policy
FOR EACH ROW
SET NEW.employment_policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM employment_policy_set AS policy_set
        WHERE policy_set.id = NEW.employment_policy_set_id
          AND policy_set.published_at IS NULL
          AND NEW.tax_year BETWEEN YEAR(policy_set.coverage_start)
              AND YEAR(DATE_SUB(policy_set.coverage_end_exclusive, INTERVAL 1 DAY))
    ),
    NEW.employment_policy_set_id,
    NULL
);

CREATE TRIGGER tr_employment_income_deduction_draft_insert
BEFORE INSERT ON employment_income_deduction_bracket
FOR EACH ROW
SET NEW.employment_policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM employment_annual_tax_policy AS annual_policy
        INNER JOIN employment_policy_set AS policy_set
            ON policy_set.id = annual_policy.employment_policy_set_id
           AND policy_set.published_at IS NULL
        WHERE annual_policy.employment_policy_set_id = NEW.employment_policy_set_id
          AND annual_policy.id = NEW.employment_annual_tax_policy_id
    )
    AND NOT EXISTS (
        SELECT 1 FROM employment_income_deduction_bracket AS existing
        WHERE existing.employment_annual_tax_policy_id = NEW.employment_annual_tax_policy_id
          AND NOT (
              COALESCE(existing.upper_bound_exclusive_krw, 9007199254740991)
                  <= NEW.lower_bound_krw
              OR COALESCE(NEW.upper_bound_exclusive_krw, 9007199254740991)
                  <= existing.lower_bound_krw
          )
    ),
    NEW.employment_policy_set_id,
    NULL
);

CREATE TRIGGER tr_employment_income_tax_bracket_draft_insert
BEFORE INSERT ON employment_income_tax_bracket
FOR EACH ROW
SET NEW.employment_policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM employment_annual_tax_policy AS annual_policy
        INNER JOIN employment_policy_set AS policy_set
            ON policy_set.id = annual_policy.employment_policy_set_id
           AND policy_set.published_at IS NULL
        WHERE annual_policy.employment_policy_set_id = NEW.employment_policy_set_id
          AND annual_policy.id = NEW.employment_annual_tax_policy_id
    )
    AND NOT EXISTS (
        SELECT 1 FROM employment_income_tax_bracket AS existing
        WHERE existing.employment_annual_tax_policy_id = NEW.employment_annual_tax_policy_id
          AND NOT (
              COALESCE(existing.upper_bound_exclusive_krw, 9007199254740991)
                  <= NEW.lower_bound_krw
              OR COALESCE(NEW.upper_bound_exclusive_krw, 9007199254740991)
                  <= existing.lower_bound_krw
          )
    ),
    NEW.employment_policy_set_id,
    NULL
);

CREATE TRIGGER tr_other_income_reward_policy_draft_insert
BEFORE INSERT ON other_income_reward_policy
FOR EACH ROW
SET NEW.employment_policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM employment_policy_set AS policy_set
        WHERE policy_set.id = NEW.employment_policy_set_id
          AND policy_set.published_at IS NULL
          AND NEW.effective_from >= policy_set.coverage_start
          AND COALESCE(NEW.effective_to_exclusive, policy_set.coverage_end_exclusive)
              <= policy_set.coverage_end_exclusive
    )
    AND NOT EXISTS (
        SELECT 1 FROM other_income_reward_policy AS existing
        WHERE existing.employment_policy_set_id = NEW.employment_policy_set_id
          AND NOT (
              COALESCE(existing.effective_to_exclusive, '9999-12-31') <= NEW.effective_from
              OR COALESCE(NEW.effective_to_exclusive, '9999-12-31')
                  <= existing.effective_from
          )
    ),
    NEW.employment_policy_set_id,
    NULL
);

CREATE TRIGGER tr_employment_finance_compatibility_valid_insert
BEFORE INSERT ON employment_finance_compatibility
FOR EACH ROW
SET NEW.employment_policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM employment_policy_set AS employment_policy
        INNER JOIN policy_set AS finance_policy ON finance_policy.id = NEW.policy_set_id
        WHERE employment_policy.id = NEW.employment_policy_set_id
          AND employment_policy.published_at IS NULL
          AND finance_policy.sealed_at IS NOT NULL
    ),
    NEW.employment_policy_set_id,
    NULL
);

CREATE TRIGGER tr_employment_policy_assignment_valid_insert
BEFORE INSERT ON employment_policy_assignment
FOR EACH ROW
SET
    NEW.employment_policy_set_id = IF(
        NEW.assignment_revision = 1
            AND EXISTS (
                SELECT 1 FROM employment_policy_set
                WHERE id = NEW.employment_policy_set_id AND published_at IS NOT NULL
            ),
        NEW.employment_policy_set_id,
        NULL
    ),
    NEW.assignment_revision = 1;

CREATE TRIGGER tr_employment_policy_assignment_bump_revision
BEFORE UPDATE ON employment_policy_assignment
FOR EACH ROW
SET
    NEW.assignment_key = IF(
        BINARY NEW.assignment_key = BINARY OLD.assignment_key
            AND NEW.employment_policy_set_id <> OLD.employment_policy_set_id
            AND NEW.assignment_revision = OLD.assignment_revision
            AND EXISTS (
                SELECT 1 FROM employment_policy_set
                WHERE id = NEW.employment_policy_set_id AND published_at IS NOT NULL
            ),
        OLD.assignment_key,
        NULL
    ),
    NEW.assignment_revision = OLD.assignment_revision + 1;

CREATE TRIGGER tr_employment_policy_assignment_no_delete
BEFORE DELETE ON employment_policy_assignment
FOR EACH ROW
SIGNAL SQLSTATE '45000'
    SET MESSAGE_TEXT = 'employment policy assignment must be updated in place';

CREATE TRIGGER tr_employment_policy_source_no_update
BEFORE UPDATE ON employment_policy_source
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment policy sources are immutable';

CREATE TRIGGER tr_employment_policy_source_no_delete
BEFORE DELETE ON employment_policy_source
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment policy sources are immutable';

CREATE TRIGGER tr_national_pension_policy_no_update
BEFORE UPDATE ON national_pension_policy
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'national pension policies are immutable';

CREATE TRIGGER tr_national_pension_policy_no_delete
BEFORE DELETE ON national_pension_policy
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'national pension policies are immutable';

CREATE TRIGGER tr_health_insurance_policy_no_update
BEFORE UPDATE ON health_insurance_policy
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'health insurance policies are immutable';

CREATE TRIGGER tr_health_insurance_policy_no_delete
BEFORE DELETE ON health_insurance_policy
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'health insurance policies are immutable';

CREATE TRIGGER tr_long_term_care_policy_no_update
BEFORE UPDATE ON long_term_care_policy
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'long-term-care policies are immutable';

CREATE TRIGGER tr_long_term_care_policy_no_delete
BEFORE DELETE ON long_term_care_policy
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'long-term-care policies are immutable';

CREATE TRIGGER tr_employment_insurance_policy_no_update
BEFORE UPDATE ON employment_insurance_policy
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment insurance policies are immutable';

CREATE TRIGGER tr_employment_insurance_policy_no_delete
BEFORE DELETE ON employment_insurance_policy
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment insurance policies are immutable';

CREATE TRIGGER tr_employment_insurance_rate_no_update
BEFORE UPDATE ON employment_insurance_employer_rate
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment insurance rates are immutable';

CREATE TRIGGER tr_employment_insurance_rate_no_delete
BEFORE DELETE ON employment_insurance_employer_rate
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment insurance rates are immutable';

CREATE TRIGGER tr_industrial_accident_policy_no_update
BEFORE UPDATE ON industrial_accident_policy
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'industrial accident policies are immutable';

CREATE TRIGGER tr_industrial_accident_policy_no_delete
BEFORE DELETE ON industrial_accident_policy
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'industrial accident policies are immutable';

CREATE TRIGGER tr_employment_withholding_version_no_update
BEFORE UPDATE ON employment_withholding_table_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment withholding versions are immutable';

CREATE TRIGGER tr_employment_withholding_version_no_delete
BEFORE DELETE ON employment_withholding_table_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment withholding versions are immutable';

CREATE TRIGGER tr_employment_withholding_row_no_update
BEFORE UPDATE ON employment_withholding_table_row
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment withholding rows are immutable';

CREATE TRIGGER tr_employment_withholding_row_no_delete
BEFORE DELETE ON employment_withholding_table_row
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment withholding rows are immutable';

CREATE TRIGGER tr_local_income_withholding_policy_no_update
BEFORE UPDATE ON local_income_withholding_policy
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'local income withholding policies are immutable';

CREATE TRIGGER tr_local_income_withholding_policy_no_delete
BEFORE DELETE ON local_income_withholding_policy
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'local income withholding policies are immutable';

CREATE TRIGGER tr_employment_annual_tax_policy_no_update
BEFORE UPDATE ON employment_annual_tax_policy
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment annual tax policies are immutable';

CREATE TRIGGER tr_employment_annual_tax_policy_no_delete
BEFORE DELETE ON employment_annual_tax_policy
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment annual tax policies are immutable';

CREATE TRIGGER tr_employment_income_deduction_no_update
BEFORE UPDATE ON employment_income_deduction_bracket
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment income deduction brackets are immutable';

CREATE TRIGGER tr_employment_income_deduction_no_delete
BEFORE DELETE ON employment_income_deduction_bracket
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment income deduction brackets are immutable';

CREATE TRIGGER tr_employment_income_tax_bracket_no_update
BEFORE UPDATE ON employment_income_tax_bracket
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment income tax brackets are immutable';

CREATE TRIGGER tr_employment_income_tax_bracket_no_delete
BEFORE DELETE ON employment_income_tax_bracket
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment income tax brackets are immutable';

CREATE TRIGGER tr_other_income_reward_policy_no_update
BEFORE UPDATE ON other_income_reward_policy
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'other-income reward policies are immutable';

CREATE TRIGGER tr_other_income_reward_policy_no_delete
BEFORE DELETE ON other_income_reward_policy
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'other-income reward policies are immutable';

CREATE TRIGGER tr_employment_finance_compatibility_no_update
BEFORE UPDATE ON employment_finance_compatibility
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment finance compatibility is immutable';

CREATE TRIGGER tr_employment_finance_compatibility_no_delete
BEFORE DELETE ON employment_finance_compatibility
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment finance compatibility is immutable';

-- Development-only 2026 fixture. Its key and ranked flag make the unreviewed calibration
-- impossible to select for ranked play; reviewed official data must be published as a new key.
INSERT INTO employment_policy_set
    (policy_key, ranked_eligible, coverage_start, coverage_end_exclusive)
VALUES ('dev-unranked-m3-employment-2026-v1', FALSE, '2026-01-01', '9999-12-31');

INSERT INTO employment_policy_source
    (
        employment_policy_set_id,
        source_key,
        title,
        source_url,
        verified_on,
        content_sha256
    )
SELECT policy_set.id, source.source_key, source.title, source.source_url,
       '2026-07-26', source.content_sha256
FROM employment_policy_set AS policy_set
INNER JOIN (
    SELECT
        'nts-withholding-guide' AS source_key,
        '국세청 근로소득 간이세액표 안내' AS title,
        'https://j.nts.go.kr/nts/cm/cntnts/cntntsView.do?cntntsId=7875&mi=6596' AS source_url,
        'c23672695db75b688e559599df8e149b057bc7de5afb6c83bcc00e41c87f0a55' AS content_sha256
    UNION ALL SELECT
        'withholding-table-through-2026-02', '소득세법 시행령 별표 2 (2026-02-28까지)',
        'https://www.law.go.kr/LSW/flDownload.do?flSeq=163197407',
        '9bc649fd2356861d8804e513df38ed58a14cacfc4b37a4d55e8be911ce5d03d6'
    UNION ALL SELECT
        'withholding-table-from-2026-03', '소득세법 시행령 별표 2 (2026-03-01부터)',
        'https://www.law.go.kr/LSW/flDownload.do?flSeq=163116877',
        '777fea70cca68f1a5ffc486cb7c9c3b2f2eec38c4fb03bebbe126a21abb992a2'
    UNION ALL SELECT
        'nps-2026-workplace-guide', '국민연금공단 2026년 사업장 실무안내',
        'https://m.nps.or.kr/fileDown.do?atchFileId=FL26000090&atchFileSn=1',
        '97c9f4c7dd06c3224df23d98c520621ee6d41f3117af169fdbe55c29b39a34d3'
    UNION ALL SELECT
        'nhis-2026-rates', '국민건강보험공단 2026년 건강·장기요양 보험료율',
        'https://edi.nhis.or.kr/portal/images/popup/20251204_pop01longdesc.html',
        'b548211a326528f2d608cfae72bdb627d1568d4731679e978d0261319eb067b4'
    UNION ALL SELECT
        'moel-employment-insurance', '고용노동부 고용보험료 부담비율',
        'https://www.moel.go.kr/info/astmgmt/employ/employList.do',
        '26661d02a6ea368a3f568f92bdc0d6afcf9149a9ff4ad68b9b29d085dc974aaf'
    UNION ALL SELECT
        'moel-industrial-accident-rates', '고용노동부 2026년 산재보험료율',
        'https://www.moel.go.kr/news/enews/report/enewsView.do?news_seq=18810',
        '6c6b07a64ff023da99548bdb3e967452eeba5f741ab289256e10be83937386d9'
    UNION ALL SELECT
        'moel-industrial-accident-burden', '고용노동부 산재보험 사용자 부담 원칙',
        'https://www.moel.go.kr/info/astmgmt/employ/sanjaeList.do',
        '31ff4e76210e8f97ba8886e2f370a71386f69ecc8be839a0f14d08ed7ed047fa'
) AS source
WHERE BINARY policy_set.policy_key = BINARY 'dev-unranked-m3-employment-2026-v1';

INSERT INTO national_pension_policy
    (
        employment_policy_set_id,
        employment_policy_source_id,
        effective_from,
        effective_to_exclusive,
        monthly_income_rounding_unit_krw,
        minimum_monthly_income_krw,
        maximum_monthly_income_krw,
        employee_rate_ppm,
        employer_rate_ppm,
        employee_rounding_unit_krw,
        employer_rounding_unit_krw
    )
SELECT policy_set.id, source.id, period.effective_from, period.effective_to_exclusive,
       1000, period.minimum_krw, period.maximum_krw, 47500, 47500, 10, 10
FROM employment_policy_set AS policy_set
INNER JOIN employment_policy_source AS source
    ON source.employment_policy_set_id = policy_set.id
   AND BINARY source.source_key = BINARY 'nps-2026-workplace-guide'
INNER JOIN (
    SELECT DATE('2026-01-01') AS effective_from, DATE('2026-07-01') AS effective_to_exclusive,
           400000 AS minimum_krw, 6370000 AS maximum_krw
    UNION ALL SELECT DATE('2026-07-01'), NULL, 410000, 6590000
) AS period
WHERE BINARY policy_set.policy_key = BINARY 'dev-unranked-m3-employment-2026-v1';

INSERT INTO health_insurance_policy
    (
        employment_policy_set_id,
        employment_policy_source_id,
        effective_from,
        effective_to_exclusive,
        monthly_remuneration_rounding_unit_krw,
        employee_rate_ppm,
        employer_rate_ppm,
        employee_rounding_unit_krw,
        employer_rounding_unit_krw
    )
SELECT policy_set.id, source.id, '2026-01-01', NULL, 1,
       35950, 35950, 10, 10
FROM employment_policy_set AS policy_set
INNER JOIN employment_policy_source AS source
    ON source.employment_policy_set_id = policy_set.id
   AND BINARY source.source_key = BINARY 'nhis-2026-rates'
WHERE BINARY policy_set.policy_key = BINARY 'dev-unranked-m3-employment-2026-v1';

INSERT INTO long_term_care_policy
    (
        employment_policy_set_id,
        employment_policy_source_id,
        effective_from,
        effective_to_exclusive,
        health_premium_rate_numerator,
        health_premium_rate_denominator,
        employee_rounding_unit_krw,
        employer_rounding_unit_krw
    )
SELECT policy_set.id, source.id, '2026-01-01', NULL,
       9448, 71900, 10, 10
FROM employment_policy_set AS policy_set
INNER JOIN employment_policy_source AS source
    ON source.employment_policy_set_id = policy_set.id
   AND BINARY source.source_key = BINARY 'nhis-2026-rates'
WHERE BINARY policy_set.policy_key = BINARY 'dev-unranked-m3-employment-2026-v1';

INSERT INTO employment_insurance_policy
    (
        employment_policy_set_id,
        employment_policy_source_id,
        effective_from,
        effective_to_exclusive,
        employee_rate_ppm,
        employee_rounding_unit_krw,
        employer_rounding_unit_krw
    )
SELECT policy_set.id, source.id, '2026-01-01', NULL, 9000, 10, 10
FROM employment_policy_set AS policy_set
INNER JOIN employment_policy_source AS source
    ON source.employment_policy_set_id = policy_set.id
   AND BINARY source.source_key = BINARY 'moel-employment-insurance'
WHERE BINARY policy_set.policy_key = BINARY 'dev-unranked-m3-employment-2026-v1';

INSERT INTO employment_insurance_employer_rate
    (
        employment_policy_set_id,
        employment_insurance_policy_id,
        employer_size_band,
        employer_rate_ppm
    )
SELECT policy.id, insurance.id, rate.employer_size_band, rate.employer_rate_ppm
FROM employment_policy_set AS policy
INNER JOIN employment_insurance_policy AS insurance
    ON insurance.employment_policy_set_id = policy.id
INNER JOIN (
    SELECT 'under150' AS employer_size_band, 11500 AS employer_rate_ppm
    UNION ALL SELECT 'from150To999', 11500
    UNION ALL SELECT 'atLeast1000', 11500
    UNION ALL SELECT 'government', 9000
) AS rate
WHERE BINARY policy.policy_key = BINARY 'dev-unranked-m3-employment-2026-v1';

INSERT INTO industrial_accident_policy
    (
        employment_policy_set_id,
        employment_policy_source_id,
        effective_from,
        effective_to_exclusive,
        industry_key,
        employer_rate_ppm,
        employer_rounding_unit_krw
    )
SELECT policy_set.id, source.id, '2026-01-01', NULL,
       industry.industry_key, 6000, 10
FROM employment_policy_set AS policy_set
INNER JOIN employment_policy_source AS source
    ON source.employment_policy_set_id = policy_set.id
   AND BINARY source.source_key = BINARY 'moel-industrial-accident-rates'
INNER JOIN (
    SELECT 'itSoftware' AS industry_key
    UNION ALL SELECT 'financeInsurance'
    UNION ALL SELECT 'manufacturing'
    UNION ALL SELECT 'constructionEngineering'
    UNION ALL SELECT 'retailService'
    UNION ALL SELECT 'publicSocial'
) AS industry
WHERE BINARY policy_set.policy_key = BINARY 'dev-unranked-m3-employment-2026-v1';

INSERT INTO employment_withholding_table_version
    (
        employment_policy_set_id,
        employment_policy_source_id,
        table_key,
        effective_from,
        effective_to_exclusive,
        withholding_percentage_bp
    )
SELECT policy_set.id, source.id, version.table_key,
       version.effective_from, version.effective_to_exclusive, 10000
FROM employment_policy_set AS policy_set
INNER JOIN (
    SELECT
        'dev-2024-table-through-2026-02' AS table_key,
        'withholding-table-through-2026-02' AS source_key,
        DATE('2026-01-01') AS effective_from,
        DATE('2026-03-01') AS effective_to_exclusive
    UNION ALL SELECT
        'dev-2026-table-from-2026-03',
        'withholding-table-from-2026-03',
        DATE('2026-03-01'),
        NULL
) AS version
INNER JOIN employment_policy_source AS source
    ON source.employment_policy_set_id = policy_set.id
   AND BINARY source.source_key = BINARY version.source_key
WHERE BINARY policy_set.policy_key = BINARY 'dev-unranked-m3-employment-2026-v1';

-- These coarse rows are deliberately unranked engine fixtures, not a transcription of the
-- official tables. Publication completeness still exercises every family and pay-band edge.
INSERT INTO employment_withholding_table_row
    (
        employment_policy_set_id,
        employment_withholding_table_version_id,
        lower_bound_krw,
        upper_bound_exclusive_krw,
        family_count,
        child_count,
        income_tax_krw
    )
SELECT version.employment_policy_set_id, version.id, band.lower_bound_krw,
       band.upper_bound_exclusive_krw, family.family_count, 0,
       GREATEST(
           band.base_tax_krw - (family.family_count - 1) * band.family_reduction_krw,
           0
       )
FROM employment_withholding_table_version AS version
INNER JOIN (
    SELECT 1 AS family_count UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4
    UNION ALL SELECT 5 UNION ALL SELECT 6 UNION ALL SELECT 7
) AS family
INNER JOIN (
    SELECT 0 AS lower_bound_krw, 1500000 AS upper_bound_exclusive_krw,
           0 AS base_tax_krw, 0 AS family_reduction_krw
    UNION ALL SELECT 1500000, 3000000, 19520, 5000
    UNION ALL SELECT 3000000, 5000000, 84850, 12000
    UNION ALL SELECT 5000000, NULL, 255000, 25000
) AS band
INNER JOIN employment_policy_set AS policy_set
    ON policy_set.id = version.employment_policy_set_id
WHERE BINARY policy_set.policy_key = BINARY 'dev-unranked-m3-employment-2026-v1';

INSERT INTO local_income_withholding_policy
    (
        employment_policy_set_id,
        employment_policy_source_id,
        effective_from,
        effective_to_exclusive,
        income_tax_rate_ppm,
        rounding_unit_krw
    )
SELECT policy_set.id, source.id, '2026-01-01', NULL, 100000, 10
FROM employment_policy_set AS policy_set
INNER JOIN employment_policy_source AS source
    ON source.employment_policy_set_id = policy_set.id
   AND BINARY source.source_key = BINARY 'nts-withholding-guide'
WHERE BINARY policy_set.policy_key = BINARY 'dev-unranked-m3-employment-2026-v1';

INSERT INTO employment_annual_tax_policy
    (
        employment_policy_set_id,
        employment_policy_source_id,
        tax_year,
        basic_personal_deduction_krw,
        taxable_income_rounding_unit_krw,
        calculated_tax_rounding_unit_krw,
        income_tax_credit_low_tax_boundary_krw,
        income_tax_credit_low_rate_ppm,
        income_tax_credit_high_base_krw,
        income_tax_credit_high_rate_ppm,
        credit_cap_salary_boundary_one_krw,
        credit_cap_salary_boundary_two_krw,
        credit_cap_one_krw,
        credit_cap_two_base_krw,
        credit_cap_two_reduction_rate_ppm,
        credit_cap_two_floor_krw,
        credit_cap_three_base_krw,
        credit_cap_three_reduction_rate_ppm,
        credit_cap_three_floor_krw
    )
SELECT policy_set.id, source.id, 2026, 1500000, 1, 1,
       1300000, 550000, 715000, 300000,
       33000000, 70000000, 740000,
       660000, 8000, 500000,
       500000, 5000, 200000
FROM employment_policy_set AS policy_set
INNER JOIN employment_policy_source AS source
    ON source.employment_policy_set_id = policy_set.id
   AND BINARY source.source_key = BINARY 'withholding-table-from-2026-03'
WHERE BINARY policy_set.policy_key = BINARY 'dev-unranked-m3-employment-2026-v1';

INSERT INTO employment_income_deduction_bracket
    (
        employment_policy_set_id,
        employment_annual_tax_policy_id,
        bracket_order,
        lower_bound_krw,
        upper_bound_exclusive_krw,
        base_deduction_krw,
        marginal_rate_ppm
    )
SELECT annual_policy.employment_policy_set_id, annual_policy.id,
       bracket.bracket_order, bracket.lower_bound_krw, bracket.upper_bound_exclusive_krw,
       bracket.base_deduction_krw, bracket.marginal_rate_ppm
FROM employment_annual_tax_policy AS annual_policy
INNER JOIN (
    SELECT 1 AS bracket_order, 0 AS lower_bound_krw, 5000000 AS upper_bound_exclusive_krw,
           0 AS base_deduction_krw, 700000 AS marginal_rate_ppm
    UNION ALL SELECT 2, 5000000, 15000000, 3500000, 400000
    UNION ALL SELECT 3, 15000000, 45000000, 7500000, 150000
    UNION ALL SELECT 4, 45000000, 100000000, 12000000, 50000
    UNION ALL SELECT 5, 100000000, NULL, 14750000, 20000
) AS bracket
INNER JOIN employment_policy_set AS policy_set
    ON policy_set.id = annual_policy.employment_policy_set_id
WHERE BINARY policy_set.policy_key = BINARY 'dev-unranked-m3-employment-2026-v1';

INSERT INTO employment_income_tax_bracket
    (
        employment_policy_set_id,
        employment_annual_tax_policy_id,
        bracket_order,
        lower_bound_krw,
        upper_bound_exclusive_krw,
        rate_ppm,
        quick_deduction_krw
    )
SELECT annual_policy.employment_policy_set_id, annual_policy.id,
       bracket.bracket_order, bracket.lower_bound_krw, bracket.upper_bound_exclusive_krw,
       bracket.rate_ppm, bracket.quick_deduction_krw
FROM employment_annual_tax_policy AS annual_policy
INNER JOIN (
    SELECT 1 AS bracket_order, 0 AS lower_bound_krw, 14000000 AS upper_bound_exclusive_krw,
           60000 AS rate_ppm, 0 AS quick_deduction_krw
    UNION ALL SELECT 2, 14000000, 50000000, 150000, 1260000
    UNION ALL SELECT 3, 50000000, 88000000, 240000, 5760000
    UNION ALL SELECT 4, 88000000, 150000000, 350000, 15440000
    UNION ALL SELECT 5, 150000000, 300000000, 380000, 19940000
    UNION ALL SELECT 6, 300000000, 500000000, 400000, 25940000
    UNION ALL SELECT 7, 500000000, 1000000000, 420000, 35940000
    UNION ALL SELECT 8, 1000000000, NULL, 450000, 65940000
) AS bracket
INNER JOIN employment_policy_set AS policy_set
    ON policy_set.id = annual_policy.employment_policy_set_id
WHERE BINARY policy_set.policy_key = BINARY 'dev-unranked-m3-employment-2026-v1';

INSERT INTO other_income_reward_policy
    (
        employment_policy_set_id,
        employment_policy_source_id,
        effective_from,
        effective_to_exclusive,
        income_tax_rate_ppm,
        local_income_tax_rate_ppm,
        income_tax_rounding_unit_krw,
        local_income_tax_rounding_unit_krw
    )
SELECT policy_set.id, source.id, '2026-01-01', NULL,
       200000, 20000, 1, 1
FROM employment_policy_set AS policy_set
INNER JOIN employment_policy_source AS source
    ON source.employment_policy_set_id = policy_set.id
   AND BINARY source.source_key = BINARY 'nts-withholding-guide'
WHERE BINARY policy_set.policy_key = BINARY 'dev-unranked-m3-employment-2026-v1';

INSERT INTO employment_finance_compatibility
    (employment_policy_set_id, policy_set_id)
SELECT employment_policy.id, finance_policy.id
FROM employment_policy_set AS employment_policy
INNER JOIN policy_set AS finance_policy ON finance_policy.sealed_at IS NOT NULL
WHERE BINARY employment_policy.policy_key
    = BINARY 'dev-unranked-m3-employment-2026-v1';

UPDATE employment_policy_set
SET published_at = CURRENT_TIMESTAMP(3)
WHERE BINARY policy_key = BINARY 'dev-unranked-m3-employment-2026-v1';

INSERT INTO employment_policy_assignment (assignment_key, employment_policy_set_id)
SELECT 'newRun', id
FROM employment_policy_set
WHERE BINARY policy_key = BINARY 'dev-unranked-m3-employment-2026-v1';

-- M3-A deliberately deferred the employment pin. Replace its focus-only trigger while the
-- nullable column is backfilled, then make the pin mandatory for every run revision.
DROP TRIGGER tr_career_run_valid_insert;
DROP TRIGGER tr_career_run_focus_only;

ALTER TABLE career_run
    ADD COLUMN employment_policy_set_id BIGINT UNSIGNED NULL
        AFTER career_catalog_bundle_id;

UPDATE career_run
INNER JOIN save
    ON save.id = career_run.save_id
   AND save.run_revision = career_run.run_revision
INNER JOIN employment_policy_assignment AS assignment
    ON BINARY assignment.assignment_key = BINARY 'newRun'
INNER JOIN employment_finance_compatibility AS compatibility
    ON compatibility.employment_policy_set_id = assignment.employment_policy_set_id
   AND compatibility.policy_set_id = save.policy_set_id
SET career_run.employment_policy_set_id = assignment.employment_policy_set_id
WHERE career_run.employment_policy_set_id IS NULL;

ALTER TABLE career_run
    MODIFY COLUMN employment_policy_set_id BIGINT UNSIGNED NOT NULL,
    ADD UNIQUE KEY uk_career_run_employment_policy
        (save_id, run_revision, employment_policy_set_id),
    ADD CONSTRAINT fk_career_run_employment_policy
        FOREIGN KEY (employment_policy_set_id) REFERENCES employment_policy_set (id);

CREATE TRIGGER tr_career_run_valid_insert
BEFORE INSERT ON career_run
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM save
        INNER JOIN `character` ON `character`.save_id = save.id
        INNER JOIN market_world ON market_world.id = save.market_world_id
        INNER JOIN career_catalog_assignment AS career_assignment
            ON BINARY career_assignment.assignment_key = BINARY 'newRun'
           AND career_assignment.career_catalog_bundle_id = NEW.career_catalog_bundle_id
        INNER JOIN career_catalog_bundle AS bundle
            ON bundle.id = career_assignment.career_catalog_bundle_id
           AND bundle.published_at IS NOT NULL
        INNER JOIN employment_policy_assignment AS employment_assignment
            ON BINARY employment_assignment.assignment_key = BINARY 'newRun'
           AND employment_assignment.employment_policy_set_id
                = NEW.employment_policy_set_id
        INNER JOIN employment_policy_set AS employment_policy
            ON employment_policy.id = employment_assignment.employment_policy_set_id
           AND employment_policy.published_at IS NOT NULL
        INNER JOIN employment_finance_compatibility AS compatibility
            ON compatibility.employment_policy_set_id = employment_policy.id
           AND compatibility.policy_set_id = save.policy_set_id
        WHERE save.id = NEW.save_id
          AND save.run_revision = NEW.run_revision
          AND BINARY NEW.focused_job_family_key
              = BINARY bundle.default_focused_job_family_key
          AND NEW.birth_date = MAKEDATE(YEAR(market_world.start_date) - `character`.age, 1)
    ),
    NEW.save_id,
    NULL
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
            SELECT 1
            FROM save
            WHERE id = OLD.save_id
              AND run_revision = OLD.run_revision
        )
        AND EXISTS (
            SELECT 1
            FROM career_job_family AS family
            WHERE family.career_catalog_bundle_id = OLD.career_catalog_bundle_id
              AND BINARY family.job_family_key = BINARY NEW.focused_job_family_key
        ),
    OLD.save_id,
    NULL
);

DROP TRIGGER tr_employment_contract_valid_insert;
DROP TRIGGER tr_employment_contract_transition_only;

ALTER TABLE employment_contract
    ADD COLUMN employment_policy_set_id BIGINT UNSIGNED NULL
        AFTER recruitment_ruleset_id,
    ADD COLUMN employer_size_band VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin
        NOT NULL DEFAULT 'under150' AFTER virtual_employer_id,
    ADD COLUMN payroll_baseline_period_no BIGINT UNSIGNED NOT NULL DEFAULT 1
        AFTER start_game_day;

UPDATE employment_contract AS contract
INNER JOIN career_run
    ON career_run.save_id = contract.save_id
   AND career_run.run_revision = contract.run_revision
   AND career_run.career_catalog_bundle_id = contract.career_catalog_bundle_id
SET contract.employment_policy_set_id = career_run.employment_policy_set_id
WHERE contract.employment_policy_set_id IS NULL;

UPDATE employment_contract AS contract
INNER JOIN save
    ON save.id = contract.save_id
   AND save.run_revision = contract.run_revision
INNER JOIN market_world ON market_world.id = save.market_world_id
SET contract.payroll_baseline_period_no = GREATEST(
    1,
    TIMESTAMPDIFF(
        MONTH,
        TIMESTAMPADD(
            DAY,
            1 - DAYOFMONTH(TIMESTAMPADD(
                DAY, contract.start_game_day, market_world.start_date
            )),
            TIMESTAMPADD(DAY, contract.start_game_day, market_world.start_date)
        ),
        TIMESTAMPADD(
            DAY,
            1 - DAYOFMONTH(TIMESTAMPADD(
                DAY, save.game_day, market_world.start_date
            )),
            TIMESTAMPADD(DAY, save.game_day, market_world.start_date)
        )
    ) + IF(
        TIMESTAMPADD(DAY, save.game_day, market_world.start_date) < TIMESTAMPADD(
            DAY,
            LEAST(
                contract.payday_day_of_month,
                DAY(LAST_DAY(TIMESTAMPADD(
                    DAY, save.game_day, market_world.start_date
                )))
            ) - DAYOFMONTH(TIMESTAMPADD(
                DAY, save.game_day, market_world.start_date
            )),
            TIMESTAMPADD(DAY, save.game_day, market_world.start_date)
        ),
        0,
        1
    )
);

ALTER TABLE employment_contract
    MODIFY COLUMN employment_policy_set_id BIGINT UNSIGNED NOT NULL,
    ADD UNIQUE KEY uk_employment_contract_save_run_policy_id
        (save_id, run_revision, employment_policy_set_id, id),
    ADD KEY ix_employment_contract_policy (employment_policy_set_id),
    ADD CONSTRAINT fk_employment_contract_employment_policy
        FOREIGN KEY (save_id, run_revision, employment_policy_set_id)
        REFERENCES career_run (save_id, run_revision, employment_policy_set_id),
    ADD CONSTRAINT ck_employment_contract_employer_size CHECK (
        employer_size_band IN ('under150', 'from150To999', 'atLeast1000', 'government')
    ),
    ADD CONSTRAINT ck_employment_contract_payroll_baseline CHECK (
        payroll_baseline_period_no > 0
    );

CREATE TRIGGER tr_employment_contract_valid_insert
BEFORE INSERT ON employment_contract
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'pendingStart'
        AND NEW.end_game_day IS NULL
        AND NEW.credited_experience_days = 0
        AND NEW.last_credited_game_day IS NULL
        AND NEW.employer_size_band = 'under150'
        AND NEW.payroll_baseline_period_no = 1
        AND EXISTS (
            SELECT 1
            FROM save
            INNER JOIN career_run
                ON career_run.save_id = save.id
               AND career_run.run_revision = save.run_revision
               AND career_run.career_catalog_bundle_id = NEW.career_catalog_bundle_id
               AND career_run.employment_policy_set_id = NEW.employment_policy_set_id
            INNER JOIN job_offer AS offer
                ON offer.save_id = save.id
               AND offer.run_revision = save.run_revision
               AND offer.career_catalog_bundle_id = NEW.career_catalog_bundle_id
               AND offer.id = NEW.job_offer_id
               AND offer.status = 'pending'
            INNER JOIN job_application AS application
                ON application.save_id = offer.save_id
               AND application.run_revision = offer.run_revision
               AND application.career_catalog_bundle_id
                    = offer.career_catalog_bundle_id
               AND application.id = offer.job_application_id
               AND application.id = NEW.job_application_id
               AND application.status = 'offered'
            WHERE save.id = NEW.save_id
              AND save.run_revision = NEW.run_revision
              AND save.game_day = NEW.created_game_day
              AND save.game_day < offer.expires_exclusive_game_day
              AND NEW.recruitment_ruleset_id = offer.recruitment_ruleset_id
              AND NEW.job_posting_id = offer.job_posting_id
              AND NEW.career_industry_id = offer.career_industry_id
              AND NEW.career_job_family_id = offer.career_job_family_id
              AND NEW.virtual_employer_id = offer.virtual_employer_id
              AND NEW.annual_salary_krw = offer.annual_salary_krw
              AND BINARY NEW.region = BINARY offer.region
              AND BINARY NEW.employment_type = BINARY offer.employment_type
              AND NEW.payday_day_of_month = offer.payday_day_of_month
              AND NEW.start_game_day = offer.start_game_day
              AND NEW.first_pay_reward_krw = offer.first_pay_reward_krw
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_employment_contract_transition_only
BEFORE UPDATE ON employment_contract
FOR EACH ROW
SET NEW.id = IF(
    NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.career_catalog_bundle_id = OLD.career_catalog_bundle_id
        AND NEW.recruitment_ruleset_id = OLD.recruitment_ruleset_id
        AND NEW.employment_policy_set_id = OLD.employment_policy_set_id
        AND NEW.job_offer_id = OLD.job_offer_id
        AND NEW.job_application_id = OLD.job_application_id
        AND NEW.job_posting_id = OLD.job_posting_id
        AND NEW.career_industry_id = OLD.career_industry_id
        AND NEW.career_job_family_id = OLD.career_job_family_id
        AND NEW.virtual_employer_id = OLD.virtual_employer_id
        AND BINARY NEW.employer_size_band = BINARY OLD.employer_size_band
        AND NEW.payroll_baseline_period_no = OLD.payroll_baseline_period_no
        AND NEW.annual_salary_krw = OLD.annual_salary_krw
        AND BINARY NEW.region = BINARY OLD.region
        AND BINARY NEW.employment_type = BINARY OLD.employment_type
        AND NEW.payday_day_of_month = OLD.payday_day_of_month
        AND NEW.start_game_day = OLD.start_game_day
        AND NEW.first_pay_reward_krw = OLD.first_pay_reward_krw
        AND NEW.created_game_day = OLD.created_game_day
        AND NEW.created_at = OLD.created_at
        AND EXISTS (
            SELECT 1 FROM save
            WHERE id = OLD.save_id AND run_revision = OLD.run_revision
        )
        AND (
            (
                OLD.status = 'pendingStart'
                AND NEW.status = 'active'
                AND NEW.end_game_day IS NULL
                AND NEW.credited_experience_days = 1
                AND NEW.last_credited_game_day = OLD.start_game_day
                AND NEW.last_credited_game_day = (
                    SELECT game_day + 1 FROM save WHERE id = OLD.save_id
                )
                AND EXISTS (
                    SELECT 1
                    FROM career_scheduled_action AS action
                    WHERE action.save_id = OLD.save_id
                      AND action.run_revision = OLD.run_revision
                      AND action.action_kind = 'employmentStart'
                      AND action.employment_contract_id = OLD.id
                      AND action.status = 'pending'
                      AND action.due_game_day = NEW.last_credited_game_day
                )
            )
            OR (
                OLD.status = 'active'
                AND NEW.status = 'active'
                AND NEW.end_game_day IS NULL
                AND NEW.credited_experience_days = OLD.credited_experience_days + 1
                AND NEW.last_credited_game_day = OLD.last_credited_game_day + 1
                AND NEW.last_credited_game_day = (
                    SELECT game_day + 1 FROM save WHERE id = OLD.save_id
                )
            )
            OR (
                OLD.status = 'pendingStart'
                AND NEW.status = 'ended'
                AND NEW.end_game_day = OLD.start_game_day
                AND NEW.credited_experience_days = 0
                AND NEW.last_credited_game_day IS NULL
                AND EXISTS (
                    SELECT 1
                    FROM command_identity AS identity
                    WHERE identity.save_id = OLD.save_id
                      AND identity.command_kind = 'startGame'
                      AND identity.initial_run_revision = OLD.run_revision
                      AND identity.initial_game_day = (
                          SELECT game_day FROM save WHERE id = OLD.save_id
                      )
                )
            )
            OR (
                OLD.status = 'active'
                AND NEW.status = 'ended'
                AND NEW.end_game_day = OLD.last_credited_game_day + 1
                AND NEW.credited_experience_days = OLD.credited_experience_days
                AND NEW.last_credited_game_day = OLD.last_credited_game_day
                AND EXISTS (
                    SELECT 1
                    FROM command_identity AS identity
                    WHERE identity.save_id = OLD.save_id
                      AND identity.command_kind = 'startGame'
                      AND identity.initial_run_revision = OLD.run_revision
                      AND identity.initial_game_day = (
                          SELECT game_day FROM save WHERE id = OLD.save_id
                      )
                )
            )
        ),
    OLD.id,
    NULL
);

DROP TRIGGER tr_scheduled_settlement_pending_insert;

ALTER TABLE scheduled_settlement
    MODIFY COLUMN occurrence BIGINT UNSIGNED NOT NULL,
    DROP CHECK ck_scheduled_settlement_kind,
    DROP CHECK ck_scheduled_settlement_source_kind,
    ADD CONSTRAINT ck_scheduled_settlement_kind CHECK (
        kind IN (
            'cmaInterest', 'depositMaturity', 'savingsInstallment', 'savingsMaturity',
            'bondCoupon', 'bondMaturity', 'llxDistribution', 'financialIncomeFiling',
            'employmentPayroll', 'employmentReconciliation'
        )
    ),
    ADD CONSTRAINT ck_scheduled_settlement_source_kind CHECK (
        source_kind IN (
            'cmaAccount', 'depositContract', 'savingsContract', 'bondPosition',
            'indexPosition', 'taxYear', 'employmentContract', 'yearEndTaxAssessment'
        )
    ),
    ADD CONSTRAINT ck_scheduled_settlement_employment_payload CHECK (
        (
            kind = 'employmentPayroll'
            AND JSON_TYPE(payload) = 'OBJECT'
            AND JSON_LENGTH(payload) = 3
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.version')) = 'INTEGER'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.version')) = '1'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.employmentContractId')) = 'STRING'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.employmentContractId'))
                REGEXP '^[1-9][0-9]{0,19}$'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.periodNo')) = 'INTEGER'
            AND CAST(JSON_UNQUOTE(JSON_EXTRACT(payload, '$.periodNo')) AS UNSIGNED) > 0
            AND source_kind = 'employmentContract'
            AND BINARY source_id
                = BINARY JSON_UNQUOTE(JSON_EXTRACT(payload, '$.employmentContractId'))
            AND occurrence
                = CAST(JSON_UNQUOTE(JSON_EXTRACT(payload, '$.periodNo')) AS UNSIGNED)
        )
        OR (
            kind = 'employmentReconciliation'
            AND JSON_TYPE(payload) = 'OBJECT'
            AND JSON_LENGTH(payload) = 3
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.version')) = 'INTEGER'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.version')) = '1'
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.taxYear')) = 'INTEGER'
            AND CAST(JSON_UNQUOTE(JSON_EXTRACT(payload, '$.taxYear')) AS UNSIGNED)
                BETWEEN 1 AND 9999
            AND JSON_TYPE(JSON_EXTRACT(payload, '$.assessmentId')) = 'STRING'
            AND JSON_UNQUOTE(JSON_EXTRACT(payload, '$.assessmentId'))
                REGEXP '^[1-9][0-9]{0,19}$'
            AND source_kind = 'yearEndTaxAssessment'
            AND BINARY source_id
                = BINARY JSON_UNQUOTE(JSON_EXTRACT(payload, '$.assessmentId'))
            AND occurrence = 1
        )
        OR kind NOT IN ('employmentPayroll', 'employmentReconciliation')
    );

CREATE TABLE payroll_record (
    id                                      BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                                 BIGINT UNSIGNED NOT NULL,
    run_revision                            INT UNSIGNED    NOT NULL,
    employment_contract_id                  BIGINT UNSIGNED NOT NULL,
    employment_policy_set_id                BIGINT UNSIGNED NOT NULL,
    scheduled_settlement_id                 BIGINT UNSIGNED NOT NULL,
    ledger_transaction_id                   BIGINT UNSIGNED     NULL,
    national_pension_policy_id              BIGINT UNSIGNED NOT NULL,
    health_insurance_policy_id              BIGINT UNSIGNED NOT NULL,
    long_term_care_policy_id                BIGINT UNSIGNED NOT NULL,
    employment_insurance_policy_id          BIGINT UNSIGNED NOT NULL,
    industrial_accident_policy_id           BIGINT UNSIGNED NOT NULL,
    employment_withholding_table_version_id BIGINT UNSIGNED NOT NULL,
    employment_withholding_table_row_id     BIGINT UNSIGNED NOT NULL,
    local_income_withholding_policy_id       BIGINT UNSIGNED NOT NULL,
    period_no                               BIGINT UNSIGNED NOT NULL,
    salary_month_ordinal                    TINYINT UNSIGNED NOT NULL,
    tax_year                                SMALLINT UNSIGNED NOT NULL,
    period_start_date                       DATE            NOT NULL,
    period_end_exclusive_date               DATE            NOT NULL,
    payday                                  DATE            NOT NULL,
    payday_game_day                         INT UNSIGNED    NOT NULL,
    calendar_days                           SMALLINT UNSIGNED NOT NULL,
    covered_days                            SMALLINT UNSIGNED NOT NULL,
    base_monthly_salary_krw                 BIGINT          NOT NULL,
    gross_pay_krw                           BIGINT          NOT NULL,
    national_pension_assessed               BOOLEAN         NOT NULL,
    national_pension_employee_basis_krw     BIGINT          NOT NULL,
    national_pension_employer_basis_krw     BIGINT          NOT NULL,
    national_pension_employee_rate_ppm      INT UNSIGNED    NOT NULL,
    national_pension_employer_rate_ppm      INT UNSIGNED    NOT NULL,
    national_pension_employee_rounding_unit_krw BIGINT      NOT NULL,
    national_pension_employer_rounding_unit_krw BIGINT      NOT NULL,
    national_pension_employee_krw           BIGINT          NOT NULL,
    national_pension_employer_krw           BIGINT          NOT NULL,
    health_insurance_assessed               BOOLEAN         NOT NULL,
    health_insurance_employee_basis_krw     BIGINT          NOT NULL,
    health_insurance_employer_basis_krw     BIGINT          NOT NULL,
    health_insurance_employee_rate_ppm      INT UNSIGNED    NOT NULL,
    health_insurance_employer_rate_ppm      INT UNSIGNED    NOT NULL,
    health_insurance_employee_rounding_unit_krw BIGINT      NOT NULL,
    health_insurance_employer_rounding_unit_krw BIGINT      NOT NULL,
    health_insurance_employee_krw           BIGINT          NOT NULL,
    health_insurance_employer_krw           BIGINT          NOT NULL,
    long_term_care_assessed                 BOOLEAN         NOT NULL,
    long_term_care_employee_health_basis_krw BIGINT         NOT NULL,
    long_term_care_employer_health_basis_krw BIGINT         NOT NULL,
    long_term_care_rate_numerator           BIGINT UNSIGNED NOT NULL,
    long_term_care_rate_denominator         BIGINT UNSIGNED NOT NULL,
    long_term_care_employee_rounding_unit_krw BIGINT        NOT NULL,
    long_term_care_employer_rounding_unit_krw BIGINT        NOT NULL,
    long_term_care_employee_krw             BIGINT          NOT NULL,
    long_term_care_employer_krw             BIGINT          NOT NULL,
    employment_insurance_assessed           BOOLEAN         NOT NULL,
    employment_insurance_employee_basis_krw BIGINT          NOT NULL,
    employment_insurance_employer_basis_krw BIGINT          NOT NULL,
    employment_insurance_employee_rate_ppm  INT UNSIGNED    NOT NULL,
    employment_insurance_employer_rate_ppm  INT UNSIGNED    NOT NULL,
    employment_insurance_employee_rounding_unit_krw BIGINT  NOT NULL,
    employment_insurance_employer_rounding_unit_krw BIGINT  NOT NULL,
    employment_insurance_employee_krw       BIGINT          NOT NULL,
    employment_insurance_employer_krw       BIGINT          NOT NULL,
    industrial_accident_assessed            BOOLEAN         NOT NULL,
    industrial_accident_basis_krw           BIGINT          NOT NULL,
    industrial_accident_employer_rate_ppm   INT UNSIGNED    NOT NULL,
    industrial_accident_employer_rounding_unit_krw BIGINT   NOT NULL,
    industrial_accident_employer_krw        BIGINT          NOT NULL,
    employee_insurance_total_krw            BIGINT          NOT NULL,
    employer_insurance_total_krw            BIGINT          NOT NULL,
    withholding_family_count                TINYINT UNSIGNED NOT NULL,
    withholding_child_count                 TINYINT UNSIGNED NOT NULL,
    withholding_lower_bound_krw             BIGINT          NOT NULL,
    withholding_upper_bound_exclusive_krw   BIGINT              NULL,
    withheld_income_tax_krw                 BIGINT          NOT NULL,
    local_income_tax_basis_krw              BIGINT          NOT NULL,
    local_income_tax_rate_ppm               INT UNSIGNED    NOT NULL,
    local_income_tax_rounding_unit_krw       BIGINT          NOT NULL,
    withheld_local_income_tax_krw           BIGINT          NOT NULL,
    net_salary_pay_krw                      BIGINT          NOT NULL,
    created_at                              DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_payroll_record_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_payroll_record_contract_period
        (save_id, run_revision, employment_contract_id, period_no),
    UNIQUE KEY uk_payroll_record_settlement
        (save_id, run_revision, scheduled_settlement_id),
    UNIQUE KEY uk_payroll_record_ledger
        (save_id, run_revision, ledger_transaction_id),
    KEY ix_payroll_record_history
        (save_id, run_revision, payday_game_day, id),
    KEY ix_payroll_record_tax_year
        (save_id, run_revision, tax_year, payday_game_day, id),
    CONSTRAINT fk_payroll_record_contract
        FOREIGN KEY (
            save_id, run_revision, employment_policy_set_id, employment_contract_id
        ) REFERENCES employment_contract (
            save_id, run_revision, employment_policy_set_id, id
        ),
    CONSTRAINT fk_payroll_record_settlement
        FOREIGN KEY (scheduled_settlement_id) REFERENCES scheduled_settlement (id),
    CONSTRAINT fk_payroll_record_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT fk_payroll_record_national_pension_policy
        FOREIGN KEY (employment_policy_set_id, national_pension_policy_id)
        REFERENCES national_pension_policy (employment_policy_set_id, id),
    CONSTRAINT fk_payroll_record_health_insurance_policy
        FOREIGN KEY (employment_policy_set_id, health_insurance_policy_id)
        REFERENCES health_insurance_policy (employment_policy_set_id, id),
    CONSTRAINT fk_payroll_record_long_term_care_policy
        FOREIGN KEY (employment_policy_set_id, long_term_care_policy_id)
        REFERENCES long_term_care_policy (employment_policy_set_id, id),
    CONSTRAINT fk_payroll_record_employment_insurance_policy
        FOREIGN KEY (employment_policy_set_id, employment_insurance_policy_id)
        REFERENCES employment_insurance_policy (employment_policy_set_id, id),
    CONSTRAINT fk_payroll_record_industrial_accident_policy
        FOREIGN KEY (employment_policy_set_id, industrial_accident_policy_id)
        REFERENCES industrial_accident_policy (employment_policy_set_id, id),
    CONSTRAINT fk_payroll_record_withholding_version
        FOREIGN KEY (employment_policy_set_id, employment_withholding_table_version_id)
        REFERENCES employment_withholding_table_version (employment_policy_set_id, id),
    CONSTRAINT fk_payroll_record_withholding_row
        FOREIGN KEY (employment_policy_set_id, employment_withholding_table_row_id)
        REFERENCES employment_withholding_table_row (employment_policy_set_id, id),
    CONSTRAINT fk_payroll_record_local_withholding_policy
        FOREIGN KEY (employment_policy_set_id, local_income_withholding_policy_id)
        REFERENCES local_income_withholding_policy (employment_policy_set_id, id),
    CONSTRAINT ck_payroll_record_period CHECK (
        period_no > 0
        AND salary_month_ordinal = MOD(period_no - 1, 12) + 1
        AND period_start_date < period_end_exclusive_date
        AND payday >= period_end_exclusive_date
        AND tax_year = YEAR(payday)
        AND calendar_days BETWEEN 28 AND 31
        AND covered_days BETWEEN 1 AND calendar_days
    ),
    CONSTRAINT ck_payroll_record_money CHECK (
        base_monthly_salary_krw >= 0
        AND gross_pay_krw >= 0
        AND national_pension_employee_basis_krw >= 0
        AND national_pension_employer_basis_krw >= 0
        AND national_pension_employee_krw >= 0
        AND national_pension_employer_krw >= 0
        AND health_insurance_employee_basis_krw >= 0
        AND health_insurance_employer_basis_krw >= 0
        AND health_insurance_employee_krw >= 0
        AND health_insurance_employer_krw >= 0
        AND long_term_care_employee_health_basis_krw >= 0
        AND long_term_care_employer_health_basis_krw >= 0
        AND long_term_care_employee_krw >= 0
        AND long_term_care_employer_krw >= 0
        AND employment_insurance_employee_basis_krw >= 0
        AND employment_insurance_employer_basis_krw >= 0
        AND employment_insurance_employee_krw >= 0
        AND employment_insurance_employer_krw >= 0
        AND industrial_accident_basis_krw >= 0
        AND industrial_accident_employer_krw >= 0
        AND employee_insurance_total_krw >= 0
        AND employer_insurance_total_krw >= 0
        AND withholding_lower_bound_krw >= 0
        AND (
            withholding_upper_bound_exclusive_krw IS NULL
            OR withholding_upper_bound_exclusive_krw > withholding_lower_bound_krw
        )
        AND withheld_income_tax_krw >= 0
        AND local_income_tax_basis_krw >= 0
        AND withheld_local_income_tax_krw >= 0
        AND net_salary_pay_krw >= 0
    ),
    CONSTRAINT ck_payroll_record_rates CHECK (
        national_pension_assessed IN (FALSE, TRUE)
        AND national_pension_employee_rate_ppm BETWEEN 1 AND 1000000
        AND national_pension_employer_rate_ppm BETWEEN 1 AND 1000000
        AND national_pension_employee_rounding_unit_krw > 0
        AND national_pension_employer_rounding_unit_krw > 0
        AND health_insurance_assessed IN (FALSE, TRUE)
        AND health_insurance_employee_rate_ppm BETWEEN 1 AND 1000000
        AND health_insurance_employer_rate_ppm BETWEEN 1 AND 1000000
        AND health_insurance_employee_rounding_unit_krw > 0
        AND health_insurance_employer_rounding_unit_krw > 0
        AND long_term_care_assessed IN (FALSE, TRUE)
        AND long_term_care_rate_numerator > 0
        AND long_term_care_rate_denominator > 0
        AND long_term_care_rate_numerator <= long_term_care_rate_denominator
        AND long_term_care_employee_rounding_unit_krw > 0
        AND long_term_care_employer_rounding_unit_krw > 0
        AND employment_insurance_assessed IN (FALSE, TRUE)
        AND employment_insurance_employee_rate_ppm BETWEEN 1 AND 1000000
        AND employment_insurance_employer_rate_ppm BETWEEN 1 AND 1000000
        AND employment_insurance_employee_rounding_unit_krw > 0
        AND employment_insurance_employer_rounding_unit_krw > 0
        AND industrial_accident_assessed IN (FALSE, TRUE)
        AND industrial_accident_employer_rate_ppm BETWEEN 1 AND 1000000
        AND industrial_accident_employer_rounding_unit_krw > 0
        AND withholding_family_count BETWEEN 1 AND 7
        AND withholding_child_count = 0
        AND local_income_tax_rate_ppm BETWEEN 1 AND 1000000
        AND local_income_tax_rounding_unit_krw > 0
    ),
    CONSTRAINT ck_payroll_record_totals CHECK (
        CAST(employee_insurance_total_krw AS DECIMAL(65, 0))
            = CAST(national_pension_employee_krw AS DECIMAL(65, 0))
            + CAST(health_insurance_employee_krw AS DECIMAL(65, 0))
            + CAST(long_term_care_employee_krw AS DECIMAL(65, 0))
            + CAST(employment_insurance_employee_krw AS DECIMAL(65, 0))
        AND CAST(employer_insurance_total_krw AS DECIMAL(65, 0))
            = CAST(national_pension_employer_krw AS DECIMAL(65, 0))
            + CAST(health_insurance_employer_krw AS DECIMAL(65, 0))
            + CAST(long_term_care_employer_krw AS DECIMAL(65, 0))
            + CAST(employment_insurance_employer_krw AS DECIMAL(65, 0))
            + CAST(industrial_accident_employer_krw AS DECIMAL(65, 0))
        AND CAST(net_salary_pay_krw AS DECIMAL(65, 0))
            = CAST(gross_pay_krw AS DECIMAL(65, 0))
            - CAST(employee_insurance_total_krw AS DECIMAL(65, 0))
            - CAST(withheld_income_tax_krw AS DECIMAL(65, 0))
            - CAST(withheld_local_income_tax_krw AS DECIMAL(65, 0))
        AND local_income_tax_basis_krw = withheld_income_tax_krw
    ),
    CONSTRAINT ck_payroll_record_assessment_shape CHECK (
        (
            gross_pay_krw = 0
            AND national_pension_assessed = FALSE
            AND health_insurance_assessed = FALSE
            AND long_term_care_assessed = FALSE
            AND employment_insurance_assessed = FALSE
            AND industrial_accident_assessed = FALSE
            AND employee_insurance_total_krw = 0
            AND employer_insurance_total_krw = 0
            AND withheld_income_tax_krw = 0
            AND withheld_local_income_tax_krw = 0
            AND net_salary_pay_krw = 0
            AND ledger_transaction_id IS NULL
        )
        OR (
            gross_pay_krw > 0
            AND employment_insurance_assessed = TRUE
            AND industrial_accident_assessed = TRUE
            AND ledger_transaction_id IS NOT NULL
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_scheduled_settlement_pending_insert
BEFORE INSERT ON scheduled_settlement
FOR EACH ROW
SET NEW.status = IF(
    NEW.status = 'pending'
        AND NEW.outcome IS NULL
        AND NEW.outcome_reason IS NULL
        AND NEW.settled_ledger_transaction_id IS NULL
        AND NEW.cancellation_ledger_transaction_id IS NULL
        AND NEW.cancellation_reason IS NULL
        AND (
            NEW.kind <> 'employmentPayroll'
            OR EXISTS (
                SELECT 1
                FROM employment_contract AS contract
                INNER JOIN save
                    ON save.id = contract.save_id
                   AND save.run_revision = contract.run_revision
                INNER JOIN market_world
                    ON market_world.id = save.market_world_id
                INNER JOIN employment_policy_set AS employment_policy
                    ON employment_policy.id = contract.employment_policy_set_id
                   AND employment_policy.published_at IS NOT NULL
                WHERE contract.id = CAST(
                          JSON_UNQUOTE(JSON_EXTRACT(NEW.payload, '$.employmentContractId'))
                          AS UNSIGNED
                      )
                  AND contract.save_id = NEW.save_id
                  AND contract.run_revision = NEW.run_revision
                  AND contract.status IN ('pendingStart', 'active')
                  AND BINARY NEW.source_id = BINARY CAST(contract.id AS CHAR)
                  AND NEW.occurrence = CAST(
                          JSON_UNQUOTE(JSON_EXTRACT(NEW.payload, '$.periodNo'))
                          AS UNSIGNED
                      )
                  AND NEW.occurrence >= contract.payroll_baseline_period_no
                  AND NEW.due_game_day = DATEDIFF(
                      TIMESTAMPADD(
                          DAY,
                          LEAST(
                              contract.payday_day_of_month,
                              DAY(LAST_DAY(
                                  TIMESTAMPADD(
                                      MONTH,
                                      NEW.occurrence,
                                      TIMESTAMPADD(
                                          DAY,
                                          1 - DAYOFMONTH(TIMESTAMPADD(
                                              DAY, contract.start_game_day,
                                              market_world.start_date
                                          )),
                                          TIMESTAMPADD(
                                              DAY, contract.start_game_day,
                                              market_world.start_date
                                          )
                                      )
                                  )
                              ))
                          ) - 1,
                          TIMESTAMPADD(
                              MONTH,
                              NEW.occurrence,
                              TIMESTAMPADD(
                                  DAY,
                                  1 - DAYOFMONTH(TIMESTAMPADD(
                                      DAY, contract.start_game_day,
                                      market_world.start_date
                                  )),
                                  TIMESTAMPADD(
                                      DAY, contract.start_game_day,
                                      market_world.start_date
                                  )
                              )
                          )
                      ),
                      market_world.start_date
                  )
                  AND DATE_ADD(
                      market_world.start_date,
                      INTERVAL NEW.due_game_day DAY
                  ) >= employment_policy.coverage_start
                  AND DATE_ADD(
                      market_world.start_date,
                      INTERVAL NEW.due_game_day DAY
                  ) < employment_policy.coverage_end_exclusive
                  AND (
                      (
                          NEW.occurrence = contract.payroll_baseline_period_no
                          AND NOT EXISTS (
                              SELECT 1 FROM payroll_record AS payroll
                              WHERE payroll.save_id = contract.save_id
                                AND payroll.run_revision = contract.run_revision
                                AND payroll.employment_contract_id = contract.id
                          )
                      )
                      OR EXISTS (
                          SELECT 1 FROM payroll_record AS previous_payroll
                          WHERE previous_payroll.save_id = contract.save_id
                            AND previous_payroll.run_revision = contract.run_revision
                            AND previous_payroll.employment_contract_id = contract.id
                            AND previous_payroll.period_no = NEW.occurrence - 1
                      )
                      OR EXISTS (
                          SELECT 1
                          FROM scheduled_settlement AS previous_schedule
                          WHERE previous_schedule.save_id = contract.save_id
                            AND previous_schedule.run_revision = contract.run_revision
                            AND previous_schedule.kind = 'employmentPayroll'
                            AND previous_schedule.source_kind = 'employmentContract'
                            AND BINARY previous_schedule.source_id
                                = BINARY CAST(contract.id AS CHAR)
                            AND previous_schedule.occurrence = NEW.occurrence - 1
                            AND previous_schedule.status = 'pending'
                      )
                  )
            )
        ),
    NEW.status,
    NULL
);

CREATE TRIGGER tr_payroll_record_valid_insert
BEFORE INSERT ON payroll_record
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM employment_contract AS contract
        INNER JOIN save
            ON save.id = contract.save_id
           AND save.run_revision = contract.run_revision
        INNER JOIN `character` ON `character`.save_id = save.id
        INNER JOIN market_world ON market_world.id = save.market_world_id
        INNER JOIN career_industry AS industry
            ON industry.career_catalog_bundle_id = contract.career_catalog_bundle_id
           AND industry.id = contract.career_industry_id
        INNER JOIN scheduled_settlement AS settlement
            ON settlement.id = NEW.scheduled_settlement_id
           AND settlement.save_id = contract.save_id
           AND settlement.run_revision = contract.run_revision
           AND settlement.kind = 'employmentPayroll'
           AND settlement.source_kind = 'employmentContract'
           AND BINARY settlement.source_id = BINARY CAST(contract.id AS CHAR)
           AND settlement.occurrence = NEW.period_no
           AND settlement.status = 'pending'
           AND settlement.due_game_day = NEW.payday_game_day
        INNER JOIN national_pension_policy AS pension_policy
            ON pension_policy.employment_policy_set_id = contract.employment_policy_set_id
           AND pension_policy.id = NEW.national_pension_policy_id
           AND NEW.payday >= pension_policy.effective_from
           AND (
               pension_policy.effective_to_exclusive IS NULL
               OR NEW.payday < pension_policy.effective_to_exclusive
           )
        INNER JOIN health_insurance_policy AS health_policy
            ON health_policy.employment_policy_set_id = contract.employment_policy_set_id
           AND health_policy.id = NEW.health_insurance_policy_id
           AND NEW.payday >= health_policy.effective_from
           AND (
               health_policy.effective_to_exclusive IS NULL
               OR NEW.payday < health_policy.effective_to_exclusive
           )
        INNER JOIN long_term_care_policy AS care_policy
            ON care_policy.employment_policy_set_id = contract.employment_policy_set_id
           AND care_policy.id = NEW.long_term_care_policy_id
           AND NEW.payday >= care_policy.effective_from
           AND (
               care_policy.effective_to_exclusive IS NULL
               OR NEW.payday < care_policy.effective_to_exclusive
           )
        INNER JOIN employment_insurance_policy AS employment_insurance_policy
            ON employment_insurance_policy.employment_policy_set_id
                = contract.employment_policy_set_id
           AND employment_insurance_policy.id = NEW.employment_insurance_policy_id
           AND NEW.payday >= employment_insurance_policy.effective_from
           AND (
               employment_insurance_policy.effective_to_exclusive IS NULL
               OR NEW.payday < employment_insurance_policy.effective_to_exclusive
           )
        INNER JOIN employment_insurance_employer_rate AS employment_employer_rate
            ON employment_employer_rate.employment_policy_set_id
                = employment_insurance_policy.employment_policy_set_id
           AND employment_employer_rate.employment_insurance_policy_id
                = employment_insurance_policy.id
           AND BINARY employment_employer_rate.employer_size_band
                = BINARY contract.employer_size_band
        INNER JOIN industrial_accident_policy AS accident_policy
            ON accident_policy.employment_policy_set_id = contract.employment_policy_set_id
           AND accident_policy.id = NEW.industrial_accident_policy_id
           AND BINARY accident_policy.industry_key = BINARY industry.industry_key
           AND NEW.payday >= accident_policy.effective_from
           AND (
               accident_policy.effective_to_exclusive IS NULL
               OR NEW.payday < accident_policy.effective_to_exclusive
           )
        INNER JOIN employment_withholding_table_version AS withholding_version
            ON withholding_version.employment_policy_set_id
                = contract.employment_policy_set_id
           AND withholding_version.id = NEW.employment_withholding_table_version_id
           AND NEW.payday >= withholding_version.effective_from
           AND (
               withholding_version.effective_to_exclusive IS NULL
               OR NEW.payday < withholding_version.effective_to_exclusive
           )
        INNER JOIN employment_withholding_table_row AS withholding_row
            ON withholding_row.employment_policy_set_id = contract.employment_policy_set_id
           AND withholding_row.employment_withholding_table_version_id
                = withholding_version.id
           AND withholding_row.id = NEW.employment_withholding_table_row_id
           AND withholding_row.family_count = 1 + `character`.dependents
           AND withholding_row.child_count = 0
           AND NEW.gross_pay_krw >= withholding_row.lower_bound_krw
           AND (
               withholding_row.upper_bound_exclusive_krw IS NULL
               OR NEW.gross_pay_krw < withholding_row.upper_bound_exclusive_krw
           )
        INNER JOIN local_income_withholding_policy AS local_policy
            ON local_policy.employment_policy_set_id = contract.employment_policy_set_id
           AND local_policy.id = NEW.local_income_withholding_policy_id
           AND NEW.payday >= local_policy.effective_from
           AND (
               local_policy.effective_to_exclusive IS NULL
               OR NEW.payday < local_policy.effective_to_exclusive
           )
        LEFT JOIN ledger_transaction AS ledger
            ON ledger.save_id = contract.save_id
           AND ledger.run_revision = contract.run_revision
           AND ledger.id = NEW.ledger_transaction_id
        WHERE contract.id = NEW.employment_contract_id
          AND contract.save_id = NEW.save_id
          AND contract.run_revision = NEW.run_revision
          AND contract.employment_policy_set_id = NEW.employment_policy_set_id
          AND contract.status = 'active'
          AND NEW.salary_month_ordinal = MOD(NEW.period_no - 1, 12) + 1
          AND NEW.period_start_date = IF(
              NEW.period_no = 1,
              TIMESTAMPADD(DAY, contract.start_game_day, market_world.start_date),
              TIMESTAMPADD(
                  MONTH,
                  NEW.period_no - 1,
                  TIMESTAMPADD(
                      DAY,
                      1 - DAYOFMONTH(TIMESTAMPADD(
                          DAY, contract.start_game_day, market_world.start_date
                      )),
                      TIMESTAMPADD(DAY, contract.start_game_day, market_world.start_date)
                  )
              )
          )
          AND NEW.period_end_exclusive_date = TIMESTAMPADD(
              MONTH,
              NEW.period_no,
              TIMESTAMPADD(
                  DAY,
                  1 - DAYOFMONTH(TIMESTAMPADD(
                      DAY, contract.start_game_day, market_world.start_date
                  )),
                  TIMESTAMPADD(DAY, contract.start_game_day, market_world.start_date)
              )
          )
          AND NEW.payday = TIMESTAMPADD(
              DAY,
              LEAST(
                  contract.payday_day_of_month,
                  DAY(LAST_DAY(NEW.period_end_exclusive_date))
              ) - 1,
              NEW.period_end_exclusive_date
          )
          AND NEW.payday_game_day = DATEDIFF(NEW.payday, market_world.start_date)
          AND NEW.calendar_days = DAY(LAST_DAY(NEW.period_start_date))
          AND NEW.covered_days = DATEDIFF(
              NEW.period_end_exclusive_date,
              NEW.period_start_date
          )
          AND NEW.base_monthly_salary_krw
              = contract.annual_salary_krw DIV 12
                + IF(
                    NEW.salary_month_ordinal <= MOD(contract.annual_salary_krw, 12),
                    1,
                    0
                )
          AND NEW.gross_pay_krw = IF(
              NEW.period_no = 1,
              CAST(FLOOR(
                  CAST(NEW.base_monthly_salary_krw AS DECIMAL(65, 0))
                      * NEW.covered_days / NEW.calendar_days
              ) AS SIGNED),
              NEW.base_monthly_salary_krw
          )
          AND NEW.national_pension_assessed
              = (NEW.gross_pay_krw > 0
                  AND (NEW.period_no > 1 OR DAY(NEW.period_start_date) = 1))
          AND NEW.national_pension_employee_basis_krw = LEAST(
              GREATEST(
                  (contract.annual_salary_krw DIV 12
                      DIV pension_policy.monthly_income_rounding_unit_krw)
                      * pension_policy.monthly_income_rounding_unit_krw,
                  pension_policy.minimum_monthly_income_krw
              ),
              pension_policy.maximum_monthly_income_krw
          )
          AND NEW.national_pension_employer_basis_krw
              = NEW.national_pension_employee_basis_krw
          AND NEW.national_pension_employee_rate_ppm = pension_policy.employee_rate_ppm
          AND NEW.national_pension_employer_rate_ppm = pension_policy.employer_rate_ppm
          AND NEW.national_pension_employee_rounding_unit_krw
              = pension_policy.employee_rounding_unit_krw
          AND NEW.national_pension_employer_rounding_unit_krw
              = pension_policy.employer_rounding_unit_krw
          AND NEW.national_pension_employee_krw = IF(
              NEW.national_pension_assessed,
              CAST(FLOOR(
                  CAST(NEW.national_pension_employee_basis_krw AS DECIMAL(65, 0))
                      * NEW.national_pension_employee_rate_ppm / 1000000
                      / NEW.national_pension_employee_rounding_unit_krw
              ) * NEW.national_pension_employee_rounding_unit_krw AS SIGNED),
              0
          )
          AND NEW.national_pension_employer_krw = IF(
              NEW.national_pension_assessed,
              CAST(FLOOR(
                  CAST(NEW.national_pension_employer_basis_krw AS DECIMAL(65, 0))
                      * NEW.national_pension_employer_rate_ppm / 1000000
                      / NEW.national_pension_employer_rounding_unit_krw
              ) * NEW.national_pension_employer_rounding_unit_krw AS SIGNED),
              0
          )
          AND NEW.health_insurance_assessed = NEW.national_pension_assessed
          AND NEW.health_insurance_employee_basis_krw
              = (contract.annual_salary_krw DIV 12
                  DIV health_policy.monthly_remuneration_rounding_unit_krw)
                  * health_policy.monthly_remuneration_rounding_unit_krw
          AND NEW.health_insurance_employer_basis_krw
              = NEW.health_insurance_employee_basis_krw
          AND NEW.health_insurance_employee_rate_ppm = health_policy.employee_rate_ppm
          AND NEW.health_insurance_employer_rate_ppm = health_policy.employer_rate_ppm
          AND NEW.health_insurance_employee_rounding_unit_krw
              = health_policy.employee_rounding_unit_krw
          AND NEW.health_insurance_employer_rounding_unit_krw
              = health_policy.employer_rounding_unit_krw
          AND NEW.health_insurance_employee_krw = IF(
              NEW.health_insurance_assessed,
              CAST(FLOOR(
                  CAST(NEW.health_insurance_employee_basis_krw AS DECIMAL(65, 0))
                      * NEW.health_insurance_employee_rate_ppm / 1000000
                      / NEW.health_insurance_employee_rounding_unit_krw
              ) * NEW.health_insurance_employee_rounding_unit_krw AS SIGNED),
              0
          )
          AND NEW.health_insurance_employer_krw = IF(
              NEW.health_insurance_assessed,
              CAST(FLOOR(
                  CAST(NEW.health_insurance_employer_basis_krw AS DECIMAL(65, 0))
                      * NEW.health_insurance_employer_rate_ppm / 1000000
                      / NEW.health_insurance_employer_rounding_unit_krw
              ) * NEW.health_insurance_employer_rounding_unit_krw AS SIGNED),
              0
          )
          AND NEW.long_term_care_assessed = NEW.health_insurance_assessed
          AND NEW.long_term_care_employee_health_basis_krw
              = NEW.health_insurance_employee_krw
          AND NEW.long_term_care_employer_health_basis_krw
              = NEW.health_insurance_employer_krw
          AND NEW.long_term_care_rate_numerator
              = care_policy.health_premium_rate_numerator
          AND NEW.long_term_care_rate_denominator
              = care_policy.health_premium_rate_denominator
          AND NEW.long_term_care_employee_rounding_unit_krw
              = care_policy.employee_rounding_unit_krw
          AND NEW.long_term_care_employer_rounding_unit_krw
              = care_policy.employer_rounding_unit_krw
          AND NEW.long_term_care_employee_krw = IF(
              NEW.long_term_care_assessed,
              CAST(FLOOR(
                  CAST(NEW.long_term_care_employee_health_basis_krw AS DECIMAL(65, 0))
                      * NEW.long_term_care_rate_numerator
                      / NEW.long_term_care_rate_denominator
                      / NEW.long_term_care_employee_rounding_unit_krw
              ) * NEW.long_term_care_employee_rounding_unit_krw AS SIGNED),
              0
          )
          AND NEW.long_term_care_employer_krw = IF(
              NEW.long_term_care_assessed,
              CAST(FLOOR(
                  CAST(NEW.long_term_care_employer_health_basis_krw AS DECIMAL(65, 0))
                      * NEW.long_term_care_rate_numerator
                      / NEW.long_term_care_rate_denominator
                      / NEW.long_term_care_employer_rounding_unit_krw
              ) * NEW.long_term_care_employer_rounding_unit_krw AS SIGNED),
              0
          )
          AND NEW.employment_insurance_assessed = TRUE
          AND NEW.employment_insurance_employee_basis_krw = NEW.gross_pay_krw
          AND NEW.employment_insurance_employer_basis_krw = NEW.gross_pay_krw
          AND NEW.employment_insurance_employee_rate_ppm
              = employment_insurance_policy.employee_rate_ppm
          AND NEW.employment_insurance_employer_rate_ppm
              = employment_employer_rate.employer_rate_ppm
          AND NEW.employment_insurance_employee_rounding_unit_krw
              = employment_insurance_policy.employee_rounding_unit_krw
          AND NEW.employment_insurance_employer_rounding_unit_krw
              = employment_insurance_policy.employer_rounding_unit_krw
          AND NEW.employment_insurance_employee_krw = CAST(FLOOR(
              CAST(NEW.employment_insurance_employee_basis_krw AS DECIMAL(65, 0))
                  * NEW.employment_insurance_employee_rate_ppm / 1000000
                  / NEW.employment_insurance_employee_rounding_unit_krw
          ) * NEW.employment_insurance_employee_rounding_unit_krw AS SIGNED)
          AND NEW.employment_insurance_employer_krw = CAST(FLOOR(
              CAST(NEW.employment_insurance_employer_basis_krw AS DECIMAL(65, 0))
                  * NEW.employment_insurance_employer_rate_ppm / 1000000
                  / NEW.employment_insurance_employer_rounding_unit_krw
          ) * NEW.employment_insurance_employer_rounding_unit_krw AS SIGNED)
          AND NEW.industrial_accident_assessed = TRUE
          AND NEW.industrial_accident_basis_krw = NEW.gross_pay_krw
          AND NEW.industrial_accident_employer_rate_ppm = accident_policy.employer_rate_ppm
          AND NEW.industrial_accident_employer_rounding_unit_krw
              = accident_policy.employer_rounding_unit_krw
          AND NEW.industrial_accident_employer_krw = CAST(FLOOR(
              CAST(NEW.industrial_accident_basis_krw AS DECIMAL(65, 0))
                  * NEW.industrial_accident_employer_rate_ppm / 1000000
                  / NEW.industrial_accident_employer_rounding_unit_krw
          ) * NEW.industrial_accident_employer_rounding_unit_krw AS SIGNED)
          AND NEW.withholding_family_count = withholding_row.family_count
          AND NEW.withholding_child_count = withholding_row.child_count
          AND NEW.withholding_lower_bound_krw = withholding_row.lower_bound_krw
          AND NEW.withholding_upper_bound_exclusive_krw
              <=> withholding_row.upper_bound_exclusive_krw
          AND NEW.withheld_income_tax_krw = withholding_row.income_tax_krw
          AND NEW.local_income_tax_basis_krw = withholding_row.income_tax_krw
          AND NEW.local_income_tax_rate_ppm = local_policy.income_tax_rate_ppm
          AND NEW.local_income_tax_rounding_unit_krw = local_policy.rounding_unit_krw
          AND NEW.withheld_local_income_tax_krw = CAST(FLOOR(
              CAST(NEW.local_income_tax_basis_krw AS DECIMAL(65, 0))
                  * NEW.local_income_tax_rate_ppm / 1000000
                  / NEW.local_income_tax_rounding_unit_krw
          ) * NEW.local_income_tax_rounding_unit_krw AS SIGNED)
          AND (
              (NEW.gross_pay_krw = 0 AND ledger.id IS NULL)
              OR (
                  NEW.gross_pay_krw > 0
                  AND ledger.id IS NOT NULL
                  AND ledger.policy_set_id = save.policy_set_id
                  AND ledger.game_day = NEW.payday_game_day
                  AND BINARY ledger.source_kind = BINARY 'employmentPayroll'
                  AND BINARY ledger.source_id = BINARY CAST(settlement.id AS CHAR)
              )
          )
          AND (
              (
                  NEW.period_no = contract.payroll_baseline_period_no
                  AND NOT EXISTS (
                      SELECT 1 FROM payroll_record AS existing_payroll
                      WHERE existing_payroll.save_id = contract.save_id
                        AND existing_payroll.run_revision = contract.run_revision
                        AND existing_payroll.employment_contract_id = contract.id
                  )
              )
              OR NEW.period_no = (
                  SELECT MAX(existing_payroll.period_no) + 1
                  FROM payroll_record AS existing_payroll
                  WHERE existing_payroll.save_id = contract.save_id
                    AND existing_payroll.run_revision = contract.run_revision
                    AND existing_payroll.employment_contract_id = contract.id
              )
          )
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_payroll_record_no_update
BEFORE UPDATE ON payroll_record
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'payroll records are immutable';

CREATE TRIGGER tr_payroll_record_no_delete
BEFORE DELETE ON payroll_record
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'payroll records are immutable';

INSERT IGNORE INTO scheduled_settlement
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
SELECT contract.save_id, contract.run_revision,
       DATEDIFF(
           TIMESTAMPADD(
               DAY,
               LEAST(
                   contract.payday_day_of_month,
                   DAY(LAST_DAY(period.period_end_exclusive_date))
               ) - 1,
               period.period_end_exclusive_date
           ),
           market_world.start_date
       ),
       'employmentPayroll',
       JSON_OBJECT(
           'version', 1,
           'employmentContractId', CAST(contract.id AS CHAR),
           'periodNo', CAST(contract.payroll_baseline_period_no AS SIGNED)
       ),
       'employmentContract',
       CAST(contract.id AS CHAR),
       contract.payroll_baseline_period_no,
       'pending'
FROM employment_contract AS contract
INNER JOIN save ON save.id = contract.save_id AND save.run_revision = contract.run_revision
INNER JOIN market_world ON market_world.id = save.market_world_id
INNER JOIN (
    SELECT active_contract.id,
           TIMESTAMPADD(
               MONTH,
               active_contract.payroll_baseline_period_no,
               TIMESTAMPADD(
                   DAY,
                   1 - DAYOFMONTH(TIMESTAMPADD(
                       DAY, active_contract.start_game_day, active_world.start_date
                   )),
                   TIMESTAMPADD(
                       DAY, active_contract.start_game_day, active_world.start_date
                   )
               )
           ) AS period_end_exclusive_date
    FROM employment_contract AS active_contract
    INNER JOIN save AS active_save
        ON active_save.id = active_contract.save_id
       AND active_save.run_revision = active_contract.run_revision
    INNER JOIN market_world AS active_world ON active_world.id = active_save.market_world_id
    WHERE active_contract.status IN ('pendingStart', 'active')
) AS period ON period.id = contract.id
WHERE contract.status IN ('pendingStart', 'active');

CREATE TABLE career_reward_payment (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED    NOT NULL,
    employment_contract_id              BIGINT UNSIGNED NOT NULL,
    employment_policy_set_id            BIGINT UNSIGNED NOT NULL,
    payroll_record_id                   BIGINT UNSIGNED NOT NULL,
    other_income_reward_policy_id       BIGINT UNSIGNED NOT NULL,
    ledger_transaction_id               BIGINT UNSIGNED NOT NULL,
    payment_date                        DATE            NOT NULL,
    payment_game_day                    INT UNSIGNED    NOT NULL,
    gross_reward_krw                    BIGINT          NOT NULL,
    income_tax_rate_ppm                 INT UNSIGNED    NOT NULL,
    local_income_tax_rate_ppm           INT UNSIGNED    NOT NULL,
    income_tax_rounding_unit_krw        BIGINT          NOT NULL,
    local_income_tax_rounding_unit_krw  BIGINT          NOT NULL,
    withheld_income_tax_krw             BIGINT          NOT NULL,
    withheld_local_income_tax_krw       BIGINT          NOT NULL,
    net_reward_krw                      BIGINT          NOT NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_career_reward_payment_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_career_reward_payment_contract
        (save_id, run_revision, employment_contract_id),
    UNIQUE KEY uk_career_reward_payment_payroll
        (save_id, run_revision, payroll_record_id),
    UNIQUE KEY uk_career_reward_payment_ledger
        (save_id, run_revision, ledger_transaction_id),
    KEY ix_career_reward_payment_history
        (save_id, run_revision, payment_game_day, id),
    CONSTRAINT fk_career_reward_payment_contract
        FOREIGN KEY (
            save_id, run_revision, employment_policy_set_id, employment_contract_id
        ) REFERENCES employment_contract (
            save_id, run_revision, employment_policy_set_id, id
        ),
    CONSTRAINT fk_career_reward_payment_payroll
        FOREIGN KEY (save_id, run_revision, payroll_record_id)
        REFERENCES payroll_record (save_id, run_revision, id),
    CONSTRAINT fk_career_reward_payment_policy
        FOREIGN KEY (employment_policy_set_id, other_income_reward_policy_id)
        REFERENCES other_income_reward_policy (employment_policy_set_id, id),
    CONSTRAINT fk_career_reward_payment_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT ck_career_reward_payment_values CHECK (
        gross_reward_krw > 0
        AND gross_reward_krw <= 9007199254740991
        AND income_tax_rate_ppm BETWEEN 1 AND 1000000
        AND local_income_tax_rate_ppm BETWEEN 1 AND 1000000
        AND income_tax_rounding_unit_krw > 0
        AND local_income_tax_rounding_unit_krw > 0
        AND withheld_income_tax_krw >= 0
        AND withheld_local_income_tax_krw >= 0
        AND net_reward_krw >= 0
        AND CAST(net_reward_krw AS DECIMAL(65, 0))
            = CAST(gross_reward_krw AS DECIMAL(65, 0))
            - CAST(withheld_income_tax_krw AS DECIMAL(65, 0))
            - CAST(withheld_local_income_tax_krw AS DECIMAL(65, 0))
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_career_reward_payment_valid_insert
BEFORE INSERT ON career_reward_payment
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM employment_contract AS contract
        INNER JOIN save
            ON save.id = contract.save_id
           AND save.run_revision = contract.run_revision
        INNER JOIN market_world ON market_world.id = save.market_world_id
        INNER JOIN payroll_record AS payroll
            ON payroll.save_id = contract.save_id
           AND payroll.run_revision = contract.run_revision
           AND payroll.employment_contract_id = contract.id
           AND payroll.id = NEW.payroll_record_id
           AND payroll.period_no = 1
        INNER JOIN other_income_reward_policy AS reward_policy
            ON reward_policy.employment_policy_set_id = contract.employment_policy_set_id
           AND reward_policy.id = NEW.other_income_reward_policy_id
           AND payroll.payday >= reward_policy.effective_from
           AND (
               reward_policy.effective_to_exclusive IS NULL
               OR payroll.payday < reward_policy.effective_to_exclusive
           )
        INNER JOIN ledger_transaction AS ledger
            ON ledger.save_id = contract.save_id
           AND ledger.run_revision = contract.run_revision
           AND ledger.id = NEW.ledger_transaction_id
           AND ledger.policy_set_id = save.policy_set_id
           AND ledger.game_day = payroll.payday_game_day
           AND BINARY ledger.source_kind = BINARY 'careerRewardPayment'
           AND BINARY ledger.source_id = BINARY CAST(contract.id AS CHAR)
        WHERE contract.id = NEW.employment_contract_id
          AND contract.save_id = NEW.save_id
          AND contract.run_revision = NEW.run_revision
          AND contract.employment_policy_set_id = NEW.employment_policy_set_id
          AND contract.first_pay_reward_krw = NEW.gross_reward_krw
          AND contract.first_pay_reward_krw > 0
          AND NEW.payment_date = payroll.payday
          AND NEW.payment_game_day = payroll.payday_game_day
          AND NEW.payment_date = DATE_ADD(
              market_world.start_date,
              INTERVAL NEW.payment_game_day DAY
          )
          AND NEW.income_tax_rate_ppm = reward_policy.income_tax_rate_ppm
          AND NEW.local_income_tax_rate_ppm = reward_policy.local_income_tax_rate_ppm
          AND NEW.income_tax_rounding_unit_krw
              = reward_policy.income_tax_rounding_unit_krw
          AND NEW.local_income_tax_rounding_unit_krw
              = reward_policy.local_income_tax_rounding_unit_krw
          AND NEW.withheld_income_tax_krw = CAST(
              FLOOR(
                  CAST(NEW.gross_reward_krw AS DECIMAL(65, 0))
                      * NEW.income_tax_rate_ppm / 1000000
                      / NEW.income_tax_rounding_unit_krw
              ) * NEW.income_tax_rounding_unit_krw AS SIGNED
          )
          AND NEW.withheld_local_income_tax_krw = CAST(
              FLOOR(
                  CAST(NEW.gross_reward_krw AS DECIMAL(65, 0))
                      * NEW.local_income_tax_rate_ppm / 1000000
                      / NEW.local_income_tax_rounding_unit_krw
              ) * NEW.local_income_tax_rounding_unit_krw AS SIGNED
          )
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_career_reward_payment_no_update
BEFORE UPDATE ON career_reward_payment
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'career reward payments are immutable';

CREATE TRIGGER tr_career_reward_payment_no_delete
BEFORE DELETE ON career_reward_payment
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'career reward payments are immutable';

CREATE TABLE employment_income_year (
    save_id                                 BIGINT UNSIGNED NOT NULL,
    run_revision                           INT UNSIGNED    NOT NULL,
    tax_year                               SMALLINT UNSIGNED NOT NULL,
    employment_policy_set_id               BIGINT UNSIGNED NOT NULL,
    status                                 VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    gross_employment_income_krw            BIGINT          NOT NULL DEFAULT 0,
    employee_national_pension_krw          BIGINT          NOT NULL DEFAULT 0,
    employee_health_insurance_krw          BIGINT          NOT NULL DEFAULT 0,
    employee_long_term_care_krw            BIGINT          NOT NULL DEFAULT 0,
    employee_employment_insurance_krw      BIGINT          NOT NULL DEFAULT 0,
    employee_insurance_total_krw           BIGINT          NOT NULL DEFAULT 0,
    withheld_income_tax_krw                BIGINT          NOT NULL DEFAULT 0,
    withheld_local_income_tax_krw          BIGINT          NOT NULL DEFAULT 0,
    net_salary_pay_krw                     BIGINT          NOT NULL DEFAULT 0,
    payroll_count                          BIGINT UNSIGNED NOT NULL DEFAULT 0,
    last_payroll_record_id                 BIGINT UNSIGNED     NULL,
    finalized_on                           DATE                NULL,
    created_at                             DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                             DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, run_revision, tax_year),
    KEY ix_employment_income_year_status
        (save_id, run_revision, status, tax_year),
    KEY ix_employment_income_year_last_payroll
        (save_id, run_revision, last_payroll_record_id),
    CONSTRAINT fk_employment_income_year_career_run
        FOREIGN KEY (save_id, run_revision, employment_policy_set_id)
        REFERENCES career_run (save_id, run_revision, employment_policy_set_id),
    CONSTRAINT fk_employment_income_year_last_payroll
        FOREIGN KEY (save_id, run_revision, last_payroll_record_id)
        REFERENCES payroll_record (save_id, run_revision, id),
    CONSTRAINT ck_employment_income_year_tax_year CHECK (tax_year BETWEEN 1 AND 9999),
    CONSTRAINT ck_employment_income_year_status CHECK (status IN ('open', 'finalized')),
    CONSTRAINT ck_employment_income_year_amounts CHECK (
        gross_employment_income_krw >= 0
        AND employee_national_pension_krw >= 0
        AND employee_health_insurance_krw >= 0
        AND employee_long_term_care_krw >= 0
        AND employee_employment_insurance_krw >= 0
        AND employee_insurance_total_krw >= 0
        AND withheld_income_tax_krw >= 0
        AND withheld_local_income_tax_krw >= 0
        AND net_salary_pay_krw >= 0
        AND CAST(employee_insurance_total_krw AS DECIMAL(65, 0))
            = CAST(employee_national_pension_krw AS DECIMAL(65, 0))
            + CAST(employee_health_insurance_krw AS DECIMAL(65, 0))
            + CAST(employee_long_term_care_krw AS DECIMAL(65, 0))
            + CAST(employee_employment_insurance_krw AS DECIMAL(65, 0))
        AND CAST(net_salary_pay_krw AS DECIMAL(65, 0))
            = CAST(gross_employment_income_krw AS DECIMAL(65, 0))
            - CAST(employee_insurance_total_krw AS DECIMAL(65, 0))
            - CAST(withheld_income_tax_krw AS DECIMAL(65, 0))
            - CAST(withheld_local_income_tax_krw AS DECIMAL(65, 0))
    ),
    CONSTRAINT ck_employment_income_year_state CHECK (
        (
            payroll_count = 0
            AND last_payroll_record_id IS NULL
            AND gross_employment_income_krw = 0
        )
        OR (
            payroll_count > 0
            AND last_payroll_record_id IS NOT NULL
        )
    ),
    CONSTRAINT ck_employment_income_year_finalized CHECK (
        (status = 'open' AND finalized_on IS NULL)
        OR (
            status = 'finalized'
            AND finalized_on IS NOT NULL
            AND YEAR(finalized_on) = tax_year + 1
            AND MONTH(finalized_on) = 1
            AND DAY(finalized_on) = 1
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_payroll_record_open_year_insert
BEFORE INSERT ON payroll_record
FOR EACH ROW
SET NEW.tax_year = IF(
    NOT EXISTS (
        SELECT 1
        FROM employment_income_year AS income_year
        WHERE income_year.save_id = NEW.save_id
          AND income_year.run_revision = NEW.run_revision
          AND income_year.tax_year = NEW.tax_year
          AND income_year.status = 'finalized'
    ),
    NEW.tax_year,
    NULL
);

CREATE TRIGGER tr_employment_income_year_valid_insert
BEFORE INSERT ON employment_income_year
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'open'
        AND NEW.finalized_on IS NULL
        AND EXISTS (
            SELECT 1
            FROM save
            INNER JOIN career_run
                ON career_run.save_id = save.id
               AND career_run.run_revision = save.run_revision
               AND career_run.employment_policy_set_id = NEW.employment_policy_set_id
            INNER JOIN payroll_record AS payroll
                ON payroll.save_id = save.id
               AND payroll.run_revision = save.run_revision
               AND payroll.id = NEW.last_payroll_record_id
               AND payroll.tax_year = NEW.tax_year
            WHERE save.id = NEW.save_id
              AND save.run_revision = NEW.run_revision
              AND NEW.payroll_count = 1
              AND NEW.gross_employment_income_krw = payroll.gross_pay_krw
              AND NEW.employee_national_pension_krw
                  = payroll.national_pension_employee_krw
              AND NEW.employee_health_insurance_krw
                  = payroll.health_insurance_employee_krw
              AND NEW.employee_long_term_care_krw
                  = payroll.long_term_care_employee_krw
              AND NEW.employee_employment_insurance_krw
                  = payroll.employment_insurance_employee_krw
              AND NEW.employee_insurance_total_krw
                  = payroll.employee_insurance_total_krw
              AND NEW.withheld_income_tax_krw = payroll.withheld_income_tax_krw
              AND NEW.withheld_local_income_tax_krw
                  = payroll.withheld_local_income_tax_krw
              AND NEW.net_salary_pay_krw = payroll.net_salary_pay_krw
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
                AND NEW.payroll_count = OLD.payroll_count + 1
                AND NEW.last_payroll_record_id > OLD.last_payroll_record_id
                AND NEW.gross_employment_income_krw >= OLD.gross_employment_income_krw
                AND NEW.employee_national_pension_krw
                    >= OLD.employee_national_pension_krw
                AND NEW.employee_health_insurance_krw
                    >= OLD.employee_health_insurance_krw
                AND NEW.employee_long_term_care_krw
                    >= OLD.employee_long_term_care_krw
                AND NEW.employee_employment_insurance_krw
                    >= OLD.employee_employment_insurance_krw
                AND NEW.employee_insurance_total_krw
                    >= OLD.employee_insurance_total_krw
                AND NEW.withheld_income_tax_krw >= OLD.withheld_income_tax_krw
                AND NEW.withheld_local_income_tax_krw
                    >= OLD.withheld_local_income_tax_krw
                AND NEW.net_salary_pay_krw >= OLD.net_salary_pay_krw
                AND NEW.payroll_count = (
                    SELECT COUNT(*)
                    FROM payroll_record AS payroll
                    WHERE payroll.save_id = OLD.save_id
                      AND payroll.run_revision = OLD.run_revision
                      AND payroll.tax_year = OLD.tax_year
                )
                AND NEW.last_payroll_record_id = (
                    SELECT MAX(payroll.id)
                    FROM payroll_record AS payroll
                    WHERE payroll.save_id = OLD.save_id
                      AND payroll.run_revision = OLD.run_revision
                      AND payroll.tax_year = OLD.tax_year
                )
                AND NEW.gross_employment_income_krw = (
                    SELECT COALESCE(SUM(payroll.gross_pay_krw), 0)
                    FROM payroll_record AS payroll
                    WHERE payroll.save_id = OLD.save_id
                      AND payroll.run_revision = OLD.run_revision
                      AND payroll.tax_year = OLD.tax_year
                )
                AND NEW.employee_national_pension_krw = (
                    SELECT COALESCE(SUM(payroll.national_pension_employee_krw), 0)
                    FROM payroll_record AS payroll
                    WHERE payroll.save_id = OLD.save_id
                      AND payroll.run_revision = OLD.run_revision
                      AND payroll.tax_year = OLD.tax_year
                )
                AND NEW.employee_health_insurance_krw = (
                    SELECT COALESCE(SUM(payroll.health_insurance_employee_krw), 0)
                    FROM payroll_record AS payroll
                    WHERE payroll.save_id = OLD.save_id
                      AND payroll.run_revision = OLD.run_revision
                      AND payroll.tax_year = OLD.tax_year
                )
                AND NEW.employee_long_term_care_krw = (
                    SELECT COALESCE(SUM(payroll.long_term_care_employee_krw), 0)
                    FROM payroll_record AS payroll
                    WHERE payroll.save_id = OLD.save_id
                      AND payroll.run_revision = OLD.run_revision
                      AND payroll.tax_year = OLD.tax_year
                )
                AND NEW.employee_employment_insurance_krw = (
                    SELECT COALESCE(SUM(payroll.employment_insurance_employee_krw), 0)
                    FROM payroll_record AS payroll
                    WHERE payroll.save_id = OLD.save_id
                      AND payroll.run_revision = OLD.run_revision
                      AND payroll.tax_year = OLD.tax_year
                )
                AND NEW.employee_insurance_total_krw = (
                    SELECT COALESCE(SUM(payroll.employee_insurance_total_krw), 0)
                    FROM payroll_record AS payroll
                    WHERE payroll.save_id = OLD.save_id
                      AND payroll.run_revision = OLD.run_revision
                      AND payroll.tax_year = OLD.tax_year
                )
                AND NEW.withheld_income_tax_krw = (
                    SELECT COALESCE(SUM(payroll.withheld_income_tax_krw), 0)
                    FROM payroll_record AS payroll
                    WHERE payroll.save_id = OLD.save_id
                      AND payroll.run_revision = OLD.run_revision
                      AND payroll.tax_year = OLD.tax_year
                )
                AND NEW.withheld_local_income_tax_krw = (
                    SELECT COALESCE(SUM(payroll.withheld_local_income_tax_krw), 0)
                    FROM payroll_record AS payroll
                    WHERE payroll.save_id = OLD.save_id
                      AND payroll.run_revision = OLD.run_revision
                      AND payroll.tax_year = OLD.tax_year
                )
                AND NEW.net_salary_pay_krw = (
                    SELECT COALESCE(SUM(payroll.net_salary_pay_krw), 0)
                    FROM payroll_record AS payroll
                    WHERE payroll.save_id = OLD.save_id
                      AND payroll.run_revision = OLD.run_revision
                      AND payroll.tax_year = OLD.tax_year
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
                AND NEW.payroll_count = OLD.payroll_count
                AND NEW.last_payroll_record_id = OLD.last_payroll_record_id
                AND YEAR(NEW.finalized_on) = OLD.tax_year + 1
                AND MONTH(NEW.finalized_on) = 1
                AND DAY(NEW.finalized_on) = 1
            )
        ),
    OLD.save_id,
    NULL
);

CREATE TRIGGER tr_employment_income_year_no_delete
BEFORE DELETE ON employment_income_year
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment income years cannot be deleted';

CREATE TABLE year_end_tax_assessment (
    id                                      BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                                 BIGINT UNSIGNED NOT NULL,
    run_revision                            INT UNSIGNED    NOT NULL,
    tax_year                                SMALLINT UNSIGNED NOT NULL,
    employment_policy_set_id                BIGINT UNSIGNED NOT NULL,
    employment_annual_tax_policy_id         BIGINT UNSIGNED NOT NULL,
    assessment_kind                         VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    assessment_status                       VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    definitive_slot                         TINYINT UNSIGNED GENERATED ALWAYS AS (
        CASE WHEN assessment_status = 'definitive' THEN 1 ELSE NULL END
    ) STORED,
    coordinator_key                         CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    uses_financial_income_assessment        BOOLEAN         NOT NULL,
    gross_employment_income_krw             BIGINT          NOT NULL,
    employment_income_deduction_krw         BIGINT          NOT NULL,
    adjusted_employment_income_krw          BIGINT          NOT NULL,
    basic_personal_deduction_krw            BIGINT          NOT NULL,
    insurance_income_deduction_krw          BIGINT          NOT NULL,
    taxable_employment_income_krw           BIGINT          NOT NULL,
    calculated_income_tax_krw               BIGINT          NOT NULL,
    employment_income_tax_credit_krw        BIGINT          NOT NULL,
    other_nonrefundable_tax_credit_krw      BIGINT          NOT NULL,
    pension_credit_eligible_krw             BIGINT          NOT NULL,
    actual_pension_income_tax_credit_krw    BIGINT          NOT NULL,
    actual_pension_local_tax_effect_krw     BIGINT          NOT NULL,
    actual_pension_credit_krw               BIGINT          NOT NULL,
    final_income_tax_krw                    BIGINT          NOT NULL,
    final_local_income_tax_krw              BIGINT          NOT NULL,
    prepaid_income_tax_krw                  BIGINT          NOT NULL,
    prepaid_local_income_tax_krw            BIGINT          NOT NULL,
    income_tax_adjustment_krw               BIGINT          NOT NULL,
    local_income_tax_adjustment_krw         BIGINT          NOT NULL,
    additional_tax_krw                      BIGINT          NOT NULL,
    refund_krw                              BIGINT          NOT NULL,
    assessed_on                             DATE            NOT NULL,
    created_at                              DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_year_end_tax_assessment_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_year_end_tax_assessment_kind
        (save_id, run_revision, tax_year, assessment_kind),
    UNIQUE KEY uk_year_end_tax_assessment_definitive
        (save_id, run_revision, tax_year, definitive_slot),
    UNIQUE KEY uk_year_end_tax_assessment_coordinator
        (save_id, run_revision, tax_year, coordinator_key, assessment_kind),
    KEY ix_year_end_tax_assessment_policy
        (employment_policy_set_id, employment_annual_tax_policy_id),
    CONSTRAINT fk_year_end_tax_assessment_income_year
        FOREIGN KEY (save_id, run_revision, tax_year)
        REFERENCES employment_income_year (save_id, run_revision, tax_year),
    CONSTRAINT fk_year_end_tax_assessment_annual_policy
        FOREIGN KEY (employment_policy_set_id, employment_annual_tax_policy_id)
        REFERENCES employment_annual_tax_policy (employment_policy_set_id, id),
    CONSTRAINT ck_year_end_tax_assessment_year CHECK (tax_year BETWEEN 1 AND 9999),
    CONSTRAINT ck_year_end_tax_assessment_kind CHECK (
        assessment_kind IN ('employmentOnly', 'combined')
    ),
    CONSTRAINT ck_year_end_tax_assessment_status CHECK (
        assessment_status IN ('provisional', 'definitive')
        AND (
            (assessment_kind = 'employmentOnly')
            OR (assessment_kind = 'combined' AND assessment_status = 'definitive')
        )
        AND uses_financial_income_assessment
            = (assessment_kind = 'combined')
    ),
    CONSTRAINT ck_year_end_tax_assessment_coordinator_key CHECK (
        coordinator_key REGEXP '^[0-9a-f]{64}$'
    ),
    CONSTRAINT ck_year_end_tax_assessment_amounts CHECK (
        gross_employment_income_krw >= 0
        AND employment_income_deduction_krw >= 0
        AND adjusted_employment_income_krw >= 0
        AND basic_personal_deduction_krw >= 0
        AND insurance_income_deduction_krw >= 0
        AND taxable_employment_income_krw >= 0
        AND calculated_income_tax_krw >= 0
        AND employment_income_tax_credit_krw >= 0
        AND other_nonrefundable_tax_credit_krw >= 0
        AND pension_credit_eligible_krw >= 0
        AND actual_pension_income_tax_credit_krw >= 0
        AND actual_pension_local_tax_effect_krw >= 0
        AND actual_pension_credit_krw
            = actual_pension_income_tax_credit_krw
                + actual_pension_local_tax_effect_krw
        AND final_income_tax_krw >= 0
        AND final_local_income_tax_krw >= 0
        AND prepaid_income_tax_krw >= 0
        AND prepaid_local_income_tax_krw >= 0
        AND additional_tax_krw >= 0
        AND refund_krw >= 0
        AND (additional_tax_krw = 0 OR refund_krw = 0)
        AND adjusted_employment_income_krw
            = GREATEST(gross_employment_income_krw - employment_income_deduction_krw, 0)
        AND taxable_employment_income_krw <= GREATEST(
            adjusted_employment_income_krw
                - basic_personal_deduction_krw
                - insurance_income_deduction_krw,
            0
        )
        AND income_tax_adjustment_krw
            = final_income_tax_krw - prepaid_income_tax_krw
        AND local_income_tax_adjustment_krw
            = final_local_income_tax_krw - prepaid_local_income_tax_krw
        AND CAST(additional_tax_krw AS DECIMAL(65, 0))
            = GREATEST(
                CAST(income_tax_adjustment_krw AS DECIMAL(65, 0))
                    + CAST(local_income_tax_adjustment_krw AS DECIMAL(65, 0)),
                0
            )
        AND CAST(refund_krw AS DECIMAL(65, 0))
            = GREATEST(
                -CAST(income_tax_adjustment_krw AS DECIMAL(65, 0))
                    - CAST(local_income_tax_adjustment_krw AS DECIMAL(65, 0)),
                0
            )
    ),
    CONSTRAINT ck_year_end_tax_assessment_date CHECK (
        YEAR(assessed_on) = tax_year + 1
        AND MONTH(assessed_on) = 1
        AND DAY(assessed_on) = 1
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_year_end_tax_assessment_valid_insert
BEFORE INSERT ON year_end_tax_assessment
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM employment_income_year AS income_year
        INNER JOIN employment_policy_set AS policy_set
            ON policy_set.id = income_year.employment_policy_set_id
           AND policy_set.published_at IS NOT NULL
        INNER JOIN employment_annual_tax_policy AS annual_policy
            ON annual_policy.employment_policy_set_id
                = income_year.employment_policy_set_id
           AND annual_policy.id = NEW.employment_annual_tax_policy_id
           AND (
               (
                   policy_set.ranked_eligible = TRUE
                   AND annual_policy.tax_year = income_year.tax_year
               )
               OR (
                   policy_set.ranked_eligible = FALSE
                   AND policy_set.policy_key LIKE 'dev-unranked-%'
                   AND annual_policy.tax_year = (
                       SELECT MAX(candidate.tax_year)
                       FROM employment_annual_tax_policy AS candidate
                       WHERE candidate.employment_policy_set_id = policy_set.id
                         AND candidate.tax_year <= income_year.tax_year
                   )
               )
           )
        WHERE income_year.save_id = NEW.save_id
          AND income_year.run_revision = NEW.run_revision
          AND income_year.tax_year = NEW.tax_year
          AND income_year.employment_policy_set_id = NEW.employment_policy_set_id
          AND income_year.status = 'finalized'
          AND NEW.gross_employment_income_krw
              = income_year.gross_employment_income_krw
          AND NEW.insurance_income_deduction_krw
              <= income_year.employee_insurance_total_krw
          AND NEW.taxable_employment_income_krw = CAST(
              FLOOR(
                  GREATEST(
                      CAST(NEW.adjusted_employment_income_krw AS DECIMAL(65, 0))
                          - CAST(NEW.basic_personal_deduction_krw AS DECIMAL(65, 0))
                          - CAST(NEW.insurance_income_deduction_krw AS DECIMAL(65, 0)),
                      0
                  ) / annual_policy.taxable_income_rounding_unit_krw
              ) * annual_policy.taxable_income_rounding_unit_krw AS SIGNED
          )
          AND (
              (
                  NEW.assessment_kind = 'employmentOnly'
                  AND NEW.prepaid_income_tax_krw
                      = income_year.withheld_income_tax_krw
                  AND NEW.prepaid_local_income_tax_krw
                      = income_year.withheld_local_income_tax_krw
              )
              OR (
                  NEW.assessment_kind = 'combined'
                  AND EXISTS (
                      SELECT 1
                      FROM year_end_tax_assessment AS employment_only
                      INNER JOIN financial_income_year AS financial_year
                          ON financial_year.save_id = employment_only.save_id
                         AND financial_year.run_revision = employment_only.run_revision
                         AND financial_year.tax_year = employment_only.tax_year
                      WHERE employment_only.save_id = NEW.save_id
                        AND employment_only.run_revision = NEW.run_revision
                        AND employment_only.tax_year = NEW.tax_year
                        AND employment_only.assessment_kind = 'employmentOnly'
                        AND employment_only.assessment_status = 'provisional'
                        AND NEW.gross_employment_income_krw
                            = employment_only.gross_employment_income_krw
                        AND NEW.employment_income_deduction_krw
                            = employment_only.employment_income_deduction_krw
                        AND NEW.adjusted_employment_income_krw
                            = employment_only.adjusted_employment_income_krw
                        AND NEW.basic_personal_deduction_krw
                            = employment_only.basic_personal_deduction_krw
                        AND NEW.insurance_income_deduction_krw
                            = employment_only.insurance_income_deduction_krw
                        AND NEW.taxable_employment_income_krw
                            = employment_only.taxable_employment_income_krw
                        AND NEW.employment_income_tax_credit_krw
                            = employment_only.employment_income_tax_credit_krw
                        AND NEW.prepaid_income_tax_krw
                            = employment_only.final_income_tax_krw
                                + financial_year.withheld_income_tax_krw
                        AND NEW.prepaid_local_income_tax_krw
                            = employment_only.final_local_income_tax_krw
                                + financial_year.withheld_local_income_tax_krw
                  )
              )
          )
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_year_end_tax_assessment_no_update
BEFORE UPDATE ON year_end_tax_assessment
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'year-end tax assessments are immutable';

CREATE TRIGGER tr_year_end_tax_assessment_no_delete
BEFORE DELETE ON year_end_tax_assessment
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'year-end tax assessments are immutable';

CREATE TRIGGER tr_scheduled_settlement_reconciliation_insert
BEFORE INSERT ON scheduled_settlement
FOR EACH ROW
FOLLOWS tr_scheduled_settlement_pending_insert
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
            INNER JOIN payroll_record AS last_payroll
                ON last_payroll.save_id = income_year.save_id
               AND last_payroll.run_revision = income_year.run_revision
               AND last_payroll.id = income_year.last_payroll_record_id
            INNER JOIN scheduled_settlement AS payroll_schedule
                ON payroll_schedule.save_id = assessment.save_id
               AND payroll_schedule.run_revision = assessment.run_revision
               AND payroll_schedule.kind = 'employmentPayroll'
               AND payroll_schedule.source_kind = 'employmentContract'
               AND BINARY payroll_schedule.source_id
                    = BINARY CAST(last_payroll.employment_contract_id AS CHAR)
               AND payroll_schedule.due_game_day = NEW.due_game_day
               AND payroll_schedule.status = 'pending'
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
              AND YEAR(DATE_ADD(
                      market_world.start_date,
                      INTERVAL NEW.due_game_day DAY
                  )) = assessment.tax_year + 1
              AND MONTH(DATE_ADD(
                      market_world.start_date,
                      INTERVAL NEW.due_game_day DAY
                  )) = 2
        ),
    NEW.status,
    NULL
);

CREATE TABLE pension_credit_allocation (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED    NOT NULL,
    tax_year                            SMALLINT UNSIGNED NOT NULL,
    year_end_tax_assessment_id          BIGINT UNSIGNED NOT NULL,
    contribution_source_id              BIGINT UNSIGNED NOT NULL,
    financial_account_id                BIGINT UNSIGNED NOT NULL,
    ledger_transaction_id               BIGINT UNSIGNED NOT NULL,
    allocated_contribution_krw          BIGINT          NOT NULL,
    income_tax_credit_krw               BIGINT          NOT NULL,
    local_income_tax_effect_krw         BIGINT          NOT NULL,
    total_credit_krw                    BIGINT          NOT NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_pension_credit_allocation_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_pension_credit_allocation_source
        (save_id, run_revision, tax_year, contribution_source_id),
    UNIQUE KEY uk_pension_credit_allocation_ledger
        (save_id, run_revision, ledger_transaction_id),
    KEY ix_pension_credit_allocation_assessment
        (save_id, run_revision, tax_year, year_end_tax_assessment_id),
    KEY ix_pension_credit_allocation_account
        (save_id, run_revision, financial_account_id, tax_year, id),
    CONSTRAINT fk_pension_credit_allocation_assessment
        FOREIGN KEY (save_id, run_revision, year_end_tax_assessment_id)
        REFERENCES year_end_tax_assessment (save_id, run_revision, id),
    CONSTRAINT fk_pension_credit_allocation_source
        FOREIGN KEY (save_id, run_revision, contribution_source_id)
        REFERENCES tax_account_event (save_id, run_revision, id),
    CONSTRAINT fk_pension_credit_allocation_account
        FOREIGN KEY (save_id, run_revision, financial_account_id)
        REFERENCES financial_account (save_id, run_revision, id),
    CONSTRAINT fk_pension_credit_allocation_ledger
        FOREIGN KEY (save_id, run_revision, ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT ck_pension_credit_allocation_year CHECK (tax_year BETWEEN 1 AND 9999),
    CONSTRAINT ck_pension_credit_allocation_amounts CHECK (
        allocated_contribution_krw > 0
        AND income_tax_credit_krw >= 0
        AND local_income_tax_effect_krw >= 0
        AND total_credit_krw = income_tax_credit_krw + local_income_tax_effect_krw
        AND total_credit_krw <= allocated_contribution_krw
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_pension_credit_allocation_valid_insert
BEFORE INSERT ON pension_credit_allocation
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM year_end_tax_assessment AS assessment
        INNER JOIN tax_account_event AS contribution
            ON contribution.save_id = assessment.save_id
           AND contribution.run_revision = assessment.run_revision
           AND contribution.id = NEW.contribution_source_id
           AND contribution.event_kind = 'pensionContribution'
           AND contribution.tax_year = NEW.tax_year
           AND contribution.financial_account_id = NEW.financial_account_id
        INNER JOIN ledger_transaction AS ledger
            ON ledger.save_id = assessment.save_id
           AND ledger.run_revision = assessment.run_revision
           AND ledger.id = NEW.ledger_transaction_id
           AND BINARY ledger.source_kind = BINARY 'pensionCreditAllocation'
           AND BINARY ledger.source_id = BINARY CAST(contribution.id AS CHAR)
        WHERE assessment.id = NEW.year_end_tax_assessment_id
          AND assessment.save_id = NEW.save_id
          AND assessment.run_revision = NEW.run_revision
          AND assessment.tax_year = NEW.tax_year
          AND assessment.assessment_status = 'definitive'
          AND NEW.allocated_contribution_krw <= contribution.movement_amount_krw
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_pension_credit_allocation_no_update
BEFORE UPDATE ON pension_credit_allocation
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'pension credit allocations are immutable';

CREATE TRIGGER tr_pension_credit_allocation_no_delete
BEFORE DELETE ON pension_credit_allocation
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'pension credit allocations are immutable';

DROP TRIGGER tr_financial_income_assessment_current_run_insert;
DROP TRIGGER tr_financial_income_assessment_transition_only;

ALTER TABLE financial_income_assessment
    ADD COLUMN year_end_tax_assessment_id BIGINT UNSIGNED NULL
        AFTER policy_set_id,
    ADD COLUMN employment_taxable_income_krw BIGINT NOT NULL DEFAULT 0
        AFTER other_comprehensive_income_krw,
    ADD COLUMN employment_deductions_krw BIGINT NOT NULL DEFAULT 0
        AFTER employment_taxable_income_krw,
    ADD COLUMN employment_final_prepaid_income_tax_krw BIGINT NOT NULL DEFAULT 0
        AFTER employment_deductions_krw,
    ADD COLUMN employment_final_prepaid_local_income_tax_krw BIGINT NOT NULL DEFAULT 0
        AFTER employment_final_prepaid_income_tax_krw,
    ADD KEY ix_financial_income_assessment_employment
        (save_id, run_revision, year_end_tax_assessment_id),
    ADD CONSTRAINT fk_financial_income_assessment_employment
        FOREIGN KEY (save_id, run_revision, year_end_tax_assessment_id)
        REFERENCES year_end_tax_assessment (save_id, run_revision, id),
    ADD CONSTRAINT ck_financial_income_assessment_employment CHECK (
        (
            year_end_tax_assessment_id IS NULL
            AND employment_taxable_income_krw = 0
            AND employment_deductions_krw = 0
            AND employment_final_prepaid_income_tax_krw = 0
            AND employment_final_prepaid_local_income_tax_krw = 0
        )
        OR (
            year_end_tax_assessment_id IS NOT NULL
            AND employment_taxable_income_krw >= 0
            AND employment_deductions_krw >= 0
            AND employment_final_prepaid_income_tax_krw >= 0
            AND employment_final_prepaid_local_income_tax_krw >= 0
        )
    );

CREATE TRIGGER tr_financial_income_assessment_current_run_insert
BEFORE INSERT ON financial_income_assessment
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'open'
        AND NEW.year_end_tax_assessment_id IS NULL
        AND NEW.employment_taxable_income_krw = 0
        AND NEW.employment_deductions_krw = 0
        AND NEW.employment_final_prepaid_income_tax_krw = 0
        AND NEW.employment_final_prepaid_local_income_tax_krw = 0
        AND EXISTS (
            SELECT 1
            FROM save
            WHERE id = NEW.save_id
              AND run_revision = NEW.run_revision
              AND policy_set_id = NEW.policy_set_id
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_financial_income_assessment_transition_only
BEFORE UPDATE ON financial_income_assessment
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.tax_year = OLD.tax_year
        AND NEW.policy_set_id = OLD.policy_set_id
        AND NEW.created_at = OLD.created_at
        AND (
            (
                OLD.status = 'open'
                AND NEW.status IN ('finalizedNoFiling', 'filingPending')
                AND NEW.gross_financial_income_krw = (
                    SELECT gross_financial_income_krw
                    FROM financial_income_year
                    WHERE save_id = OLD.save_id
                      AND run_revision = OLD.run_revision
                      AND tax_year = OLD.tax_year
                )
                AND NEW.withheld_income_tax_krw = (
                    SELECT withheld_income_tax_krw
                    FROM financial_income_year
                    WHERE save_id = OLD.save_id
                      AND run_revision = OLD.run_revision
                      AND tax_year = OLD.tax_year
                )
                AND NEW.withheld_local_income_tax_krw = (
                    SELECT withheld_local_income_tax_krw
                    FROM financial_income_year
                    WHERE save_id = OLD.save_id
                      AND run_revision = OLD.run_revision
                      AND tax_year = OLD.tax_year
                )
                AND (
                    (
                        NEW.year_end_tax_assessment_id IS NULL
                        AND NEW.employment_taxable_income_krw = 0
                        AND NEW.employment_deductions_krw = 0
                        AND NEW.employment_final_prepaid_income_tax_krw = 0
                        AND NEW.employment_final_prepaid_local_income_tax_krw = 0
                        AND (
                            NEW.status = 'finalizedNoFiling'
                            OR NOT EXISTS (
                                SELECT 1
                                FROM year_end_tax_assessment AS existing_employment
                                WHERE existing_employment.save_id = OLD.save_id
                                  AND existing_employment.run_revision = OLD.run_revision
                                  AND existing_employment.tax_year = OLD.tax_year
                            )
                        )
                    )
                    OR EXISTS (
                        SELECT 1
                        FROM year_end_tax_assessment AS employment_assessment
                        INNER JOIN year_end_tax_assessment AS employment_only
                            ON employment_only.save_id = employment_assessment.save_id
                           AND employment_only.run_revision
                                = employment_assessment.run_revision
                           AND employment_only.tax_year = employment_assessment.tax_year
                           AND employment_only.assessment_kind = 'employmentOnly'
                           AND employment_only.assessment_status = 'provisional'
                        WHERE employment_assessment.id
                                = NEW.year_end_tax_assessment_id
                          AND employment_assessment.save_id = OLD.save_id
                          AND employment_assessment.run_revision = OLD.run_revision
                          AND employment_assessment.tax_year = OLD.tax_year
                          AND employment_assessment.assessment_kind = 'combined'
                          AND employment_assessment.assessment_status = 'definitive'
                          AND NEW.employment_taxable_income_krw
                              = employment_only.taxable_employment_income_krw
                          AND NEW.employment_deductions_krw
                              = employment_only.employment_income_deduction_krw
                                + employment_only.basic_personal_deduction_krw
                                + employment_only.insurance_income_deduction_krw
                          AND NEW.employment_final_prepaid_income_tax_krw
                              = employment_only.final_income_tax_krw
                          AND NEW.employment_final_prepaid_local_income_tax_krw
                              = employment_only.final_local_income_tax_krw
                          AND NEW.income_tax_credit_krw
                              = employment_assessment.employment_income_tax_credit_krw
                                + employment_assessment.actual_pension_income_tax_credit_krw
                          AND NEW.local_income_tax_credit_krw
                              >= employment_assessment.actual_pension_local_tax_effect_krw
                          AND NEW.final_income_tax_krw
                              = employment_assessment.final_income_tax_krw
                          AND NEW.final_local_income_tax_krw
                              = employment_assessment.final_local_income_tax_krw
                          AND NEW.additional_tax_krw
                              = employment_assessment.additional_tax_krw
                          AND NEW.refund_krw = employment_assessment.refund_krw
                    )
                )
            )
            OR (
                OLD.status = 'filingPending'
                AND NEW.status = 'filed'
                AND NEW.year_end_tax_assessment_id
                    <=> OLD.year_end_tax_assessment_id
                AND NEW.gross_financial_income_krw = OLD.gross_financial_income_krw
                AND NEW.other_comprehensive_income_krw
                    = OLD.other_comprehensive_income_krw
                AND NEW.employment_taxable_income_krw
                    = OLD.employment_taxable_income_krw
                AND NEW.employment_deductions_krw = OLD.employment_deductions_krw
                AND NEW.employment_final_prepaid_income_tax_krw
                    = OLD.employment_final_prepaid_income_tax_krw
                AND NEW.employment_final_prepaid_local_income_tax_krw
                    = OLD.employment_final_prepaid_local_income_tax_krw
                AND NEW.withheld_income_tax_krw = OLD.withheld_income_tax_krw
                AND NEW.withheld_local_income_tax_krw
                    = OLD.withheld_local_income_tax_krw
                AND NEW.income_tax_formula_a_krw = OLD.income_tax_formula_a_krw
                AND NEW.income_tax_formula_b_krw = OLD.income_tax_formula_b_krw
                AND NEW.local_income_tax_formula_a_krw
                    = OLD.local_income_tax_formula_a_krw
                AND NEW.local_income_tax_formula_b_krw
                    = OLD.local_income_tax_formula_b_krw
                AND NEW.income_tax_credit_krw = OLD.income_tax_credit_krw
                AND NEW.local_income_tax_credit_krw = OLD.local_income_tax_credit_krw
                AND NEW.final_income_tax_krw = OLD.final_income_tax_krw
                AND NEW.final_local_income_tax_krw = OLD.final_local_income_tax_krw
                AND NEW.additional_tax_krw = OLD.additional_tax_krw
                AND NEW.refund_krw = OLD.refund_krw
                AND NEW.finalized_on = OLD.finalized_on
                AND NEW.filing_date = OLD.filing_date
                AND NEW.filed_on = OLD.filing_date
            )
        ),
    OLD.save_id,
    NULL
);

DROP TRIGGER tr_tax_value_event_valid_insert;

ALTER TABLE tax_account_value_event
    DROP CHECK ck_tax_value_event_cause,
    DROP CHECK ck_tax_value_event_delta,
    ADD CONSTRAINT ck_tax_value_event_cause CHECK (
        cause IN (
            'dailyMarketToMarket', 'tradeBasisAdjustment', 'pensionCreditFinalized'
        )
    ),
    ADD CONSTRAINT ck_tax_value_event_delta CHECK (
        (
            cause = 'dailyMarketToMarket'
            AND CAST(position_market_value_after_krw AS DECIMAL(65, 0))
                - CAST(position_market_value_before_krw AS DECIMAL(65, 0))
                = value_change_krw
            AND CAST(account_total_after_krw AS DECIMAL(65, 0))
                - CAST(account_total_before_krw AS DECIMAL(65, 0))
                = value_change_krw
        )
        OR (
            cause = 'tradeBasisAdjustment'
            AND value_change_krw = 0
            AND account_total_after_krw = account_total_before_krw
            AND before_tax_excluded_krw = after_tax_excluded_krw
            AND before_deferred_retirement_krw = after_deferred_retirement_krw
            AND before_credited_contribution_krw = after_credited_contribution_krw
            AND before_earnings_krw = after_earnings_krw
        )
        OR (
            cause = 'pensionCreditFinalized'
            AND source_kind = 'pensionCreditAllocation'
            AND value_change_krw = 0
            AND position_market_value_after_krw = position_market_value_before_krw
            AND account_total_after_krw = account_total_before_krw
            AND before_deferred_retirement_krw = after_deferred_retirement_krw
            AND before_earnings_krw = after_earnings_krw
            AND before_tax_excluded_krw > after_tax_excluded_krw
            AND after_credited_contribution_krw > before_credited_contribution_krw
            AND before_tax_excluded_krw - after_tax_excluded_krw
                = after_credited_contribution_krw - before_credited_contribution_krw
        )
    );

CREATE TRIGGER tr_tax_value_event_valid_insert
BEFORE INSERT ON tax_account_value_event
FOR EACH ROW
SET NEW.financial_account_id = IF(
    (
        NEW.cause IN ('dailyMarketToMarket', 'tradeBasisAdjustment')
        AND EXISTS (
            SELECT 1
            FROM pension_account_contract AS contract
            WHERE contract.save_id = NEW.save_id
              AND contract.run_revision = NEW.run_revision
              AND contract.financial_account_id = NEW.financial_account_id
              AND contract.status = 'active'
        )
    )
    OR (
        NEW.cause = 'pensionCreditFinalized'
        AND NEW.source_kind = 'pensionCreditAllocation'
        AND EXISTS (
            SELECT 1
            FROM pension_credit_allocation AS allocation
            INNER JOIN pension_account_contract AS contract
                ON contract.save_id = allocation.save_id
               AND contract.run_revision = allocation.run_revision
               AND contract.financial_account_id = allocation.financial_account_id
            WHERE allocation.save_id = NEW.save_id
              AND allocation.run_revision = NEW.run_revision
              AND allocation.financial_account_id = NEW.financial_account_id
              AND allocation.contribution_source_id = CAST(NEW.source_id AS UNSIGNED)
              AND allocation.tax_year = NEW.occurrence
              AND allocation.allocated_contribution_krw
                  = NEW.before_tax_excluded_krw - NEW.after_tax_excluded_krw
        )
    ),
    NEW.financial_account_id,
    NULL
);

ALTER TABLE ledger_posting
    MODIFY COLUMN account_code VARCHAR(48) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    DROP CHECK ck_ledger_posting_account_code,
    DROP CHECK ck_ledger_posting_account_reference,
    ADD CONSTRAINT ck_ledger_posting_account_code CHECK (
        account_code IN (
            'wallet',
            'accountCash',
            'productPrincipal',
            'debtPrincipal',
            'openingEquity',
            'withholdingTaxLiability',
            'interestIncome',
            'feeExpense',
            'distributionIncome',
            'realizedGainLoss',
            'taxSettlement',
            'careerDevelopmentExpense',
            'salaryIncome',
            'employeeNationalPensionExpense',
            'employeeHealthInsuranceExpense',
            'employeeLongTermCareExpense',
            'employeeEmploymentInsuranceExpense',
            'employmentIncomeTaxWithholding',
            'employmentLocalIncomeTaxWithholding',
            'otherIncomeReward',
            'otherIncomeTaxWithholding',
            'otherLocalIncomeTaxWithholding',
            'pensionTaxExcludedContribution',
            'pensionCreditedContribution'
        )
    ),
    ADD CONSTRAINT ck_ledger_posting_account_reference CHECK (
        (
            account_code IN (
                'accountCash', 'productPrincipal',
                'pensionTaxExcludedContribution', 'pensionCreditedContribution'
            )
            AND financial_account_id IS NOT NULL
        )
        OR (
            account_code NOT IN (
                'accountCash', 'productPrincipal',
                'pensionTaxExcludedContribution', 'pensionCreditedContribution'
            )
            AND financial_account_id IS NULL
        )
    );
