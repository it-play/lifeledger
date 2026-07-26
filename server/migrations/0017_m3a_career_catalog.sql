-- M3-A immutable career content bundle and the minimal A/B/D catalog (§2.1–§5, §10).

CREATE TABLE career_catalog_bundle (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    bundle_key                          VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ranked_eligible                     BOOLEAN         NOT NULL DEFAULT FALSE,
    default_focused_job_family_key      VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    published_at                        DATETIME(3)      NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_career_catalog_bundle_key (bundle_key),
    CONSTRAINT ck_career_catalog_bundle_key CHECK (CHAR_LENGTH(bundle_key) > 0),
    CONSTRAINT ck_career_catalog_bundle_ranked CHECK (ranked_eligible IN (FALSE, TRUE)),
    CONSTRAINT ck_career_catalog_bundle_publication_shape CHECK (
        published_at IS NULL OR default_focused_job_family_key IS NOT NULL
    ),
    CONSTRAINT ck_career_catalog_bundle_ranked_key CHECK (
        ranked_eligible = FALSE
        OR bundle_key NOT LIKE 'dev-unranked-%'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE career_industry (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    career_catalog_bundle_id    BIGINT UNSIGNED NOT NULL,
    industry_key                VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                VARCHAR(100)    NOT NULL,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_career_industry_bundle_key (career_catalog_bundle_id, industry_key),
    UNIQUE KEY uk_career_industry_bundle_id (career_catalog_bundle_id, id),
    CONSTRAINT fk_career_industry_bundle
        FOREIGN KEY (career_catalog_bundle_id) REFERENCES career_catalog_bundle (id),
    CONSTRAINT ck_career_industry_key CHECK (
        industry_key IN (
            'itSoftware', 'financeInsurance', 'manufacturing',
            'constructionEngineering', 'retailService', 'publicSocial'
        )
    ),
    CONSTRAINT ck_career_industry_name CHECK (CHAR_LENGTH(display_name) > 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE career_job_family (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    career_catalog_bundle_id    BIGINT UNSIGNED NOT NULL,
    industry_id                 BIGINT UNSIGNED NOT NULL,
    job_family_key              VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                VARCHAR(100)    NOT NULL,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_career_job_family_bundle_key (career_catalog_bundle_id, job_family_key),
    UNIQUE KEY uk_career_job_family_bundle_id (career_catalog_bundle_id, id),
    UNIQUE KEY uk_career_job_family_bundle_industry_id
        (career_catalog_bundle_id, industry_id, id),
    CONSTRAINT fk_career_job_family_bundle
        FOREIGN KEY (career_catalog_bundle_id) REFERENCES career_catalog_bundle (id),
    CONSTRAINT fk_career_job_family_industry
        FOREIGN KEY (career_catalog_bundle_id, industry_id)
        REFERENCES career_industry (career_catalog_bundle_id, id),
    CONSTRAINT ck_career_job_family_key CHECK (CHAR_LENGTH(job_family_key) > 0),
    CONSTRAINT ck_career_job_family_name CHECK (CHAR_LENGTH(display_name) > 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

ALTER TABLE career_catalog_bundle
    ADD CONSTRAINT fk_career_catalog_bundle_default_focus
        FOREIGN KEY (id, default_focused_job_family_key)
        REFERENCES career_job_family (career_catalog_bundle_id, job_family_key);

CREATE TABLE spec_catalog_entry (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    career_catalog_bundle_id    BIGINT UNSIGNED NOT NULL,
    entry_key                   VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    kind                        VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                VARCHAR(120)    NOT NULL,
    stackable                   BOOLEAN         NOT NULL,
    validity_days               INT UNSIGNED        NULL,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_spec_catalog_entry_bundle_key (career_catalog_bundle_id, entry_key),
    UNIQUE KEY uk_spec_catalog_entry_bundle_id (career_catalog_bundle_id, id),
    CONSTRAINT fk_spec_catalog_entry_bundle
        FOREIGN KEY (career_catalog_bundle_id) REFERENCES career_catalog_bundle (id),
    CONSTRAINT ck_spec_catalog_entry_key CHECK (CHAR_LENGTH(entry_key) > 0),
    CONSTRAINT ck_spec_catalog_entry_kind CHECK (
        kind IN ('education', 'certification', 'language', 'training', 'experience', 'project')
    ),
    CONSTRAINT ck_spec_catalog_entry_name CHECK (CHAR_LENGTH(display_name) > 0),
    CONSTRAINT ck_spec_catalog_entry_stackable CHECK (stackable IN (FALSE, TRUE)),
    CONSTRAINT ck_spec_catalog_entry_validity CHECK (
        validity_days IS NULL OR validity_days > 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE spec_catalog_contribution (
    career_catalog_bundle_id    BIGINT UNSIGNED NOT NULL,
    spec_catalog_entry_id       BIGINT UNSIGNED NOT NULL,
    career_job_family_id        BIGINT UNSIGNED NOT NULL,
    contribution_bp             INT             NOT NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (spec_catalog_entry_id, career_job_family_id),
    KEY ix_spec_contribution_bundle_job
        (career_catalog_bundle_id, career_job_family_id, spec_catalog_entry_id),
    CONSTRAINT fk_spec_contribution_entry
        FOREIGN KEY (career_catalog_bundle_id, spec_catalog_entry_id)
        REFERENCES spec_catalog_entry (career_catalog_bundle_id, id),
    CONSTRAINT fk_spec_contribution_job_family
        FOREIGN KEY (career_catalog_bundle_id, career_job_family_id)
        REFERENCES career_job_family (career_catalog_bundle_id, id),
    CONSTRAINT ck_spec_catalog_contribution_bp CHECK (
        contribution_bp BETWEEN 0 AND 10000
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE career_bridge_education_mapping (
    career_catalog_bundle_id    BIGINT UNSIGNED NOT NULL,
    education                   VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    evidence_key                VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    spec_catalog_entry_id       BIGINT UNSIGNED NOT NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (career_catalog_bundle_id, education),
    UNIQUE KEY uk_career_bridge_education_evidence (career_catalog_bundle_id, evidence_key),
    UNIQUE KEY uk_career_bridge_education_entry (career_catalog_bundle_id, spec_catalog_entry_id),
    CONSTRAINT fk_career_bridge_education_bundle
        FOREIGN KEY (career_catalog_bundle_id) REFERENCES career_catalog_bundle (id),
    CONSTRAINT fk_career_bridge_education_entry
        FOREIGN KEY (career_catalog_bundle_id, spec_catalog_entry_id)
        REFERENCES spec_catalog_entry (career_catalog_bundle_id, id),
    CONSTRAINT ck_career_bridge_education_value CHECK (
        education IN ('highSchool', 'associate', 'bachelor', 'master', 'doctorate')
    ),
    CONSTRAINT ck_career_bridge_education_evidence_key CHECK (CHAR_LENGTH(evidence_key) > 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE career_bridge_certification_order (
    career_catalog_bundle_id    BIGINT UNSIGNED NOT NULL,
    certification_order         TINYINT UNSIGNED NOT NULL,
    evidence_key                VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    spec_catalog_entry_id       BIGINT UNSIGNED NOT NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (career_catalog_bundle_id, certification_order),
    UNIQUE KEY uk_career_bridge_certification_evidence (career_catalog_bundle_id, evidence_key),
    UNIQUE KEY uk_career_bridge_certification_entry (career_catalog_bundle_id, spec_catalog_entry_id),
    CONSTRAINT fk_career_bridge_certification_bundle
        FOREIGN KEY (career_catalog_bundle_id) REFERENCES career_catalog_bundle (id),
    CONSTRAINT fk_career_bridge_certification_entry
        FOREIGN KEY (career_catalog_bundle_id, spec_catalog_entry_id)
        REFERENCES spec_catalog_entry (career_catalog_bundle_id, id),
    CONSTRAINT ck_career_bridge_certification_order CHECK (
        certification_order BETWEEN 1 AND 50
    ),
    CONSTRAINT ck_career_bridge_certification_evidence_key CHECK (CHAR_LENGTH(evidence_key) > 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE career_bridge_experience_mapping (
    career_catalog_bundle_id    BIGINT UNSIGNED NOT NULL,
    career_years                TINYINT UNSIGNED NOT NULL,
    evidence_key                VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    spec_catalog_entry_id       BIGINT UNSIGNED NOT NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (career_catalog_bundle_id, career_years),
    UNIQUE KEY uk_career_bridge_experience_evidence (career_catalog_bundle_id, evidence_key),
    UNIQUE KEY uk_career_bridge_experience_entry (career_catalog_bundle_id, spec_catalog_entry_id),
    CONSTRAINT fk_career_bridge_experience_bundle
        FOREIGN KEY (career_catalog_bundle_id) REFERENCES career_catalog_bundle (id),
    CONSTRAINT fk_career_bridge_experience_entry
        FOREIGN KEY (career_catalog_bundle_id, spec_catalog_entry_id)
        REFERENCES spec_catalog_entry (career_catalog_bundle_id, id),
    CONSTRAINT ck_career_bridge_experience_years CHECK (career_years <= 30),
    CONSTRAINT ck_career_bridge_experience_evidence_key CHECK (CHAR_LENGTH(evidence_key) > 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE activity_catalog_entry (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    career_catalog_bundle_id    BIGINT UNSIGNED NOT NULL,
    activity_key                VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                VARCHAR(120)    NOT NULL,
    minimum_calendar_days       INT UNSIGNED    NOT NULL,
    required_effort_units       BIGINT UNSIGNED NOT NULL,
    daily_effort_cap_units      BIGINT UNSIGNED NOT NULL,
    cost_krw                    BIGINT          NOT NULL,
    evidence_catalog_entry_id   BIGINT UNSIGNED NOT NULL,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_activity_catalog_entry_bundle_key (career_catalog_bundle_id, activity_key),
    UNIQUE KEY uk_activity_catalog_entry_bundle_id (career_catalog_bundle_id, id),
    KEY ix_activity_catalog_entry_evidence
        (career_catalog_bundle_id, evidence_catalog_entry_id),
    CONSTRAINT fk_activity_catalog_entry_bundle
        FOREIGN KEY (career_catalog_bundle_id) REFERENCES career_catalog_bundle (id),
    CONSTRAINT fk_activity_catalog_entry_evidence
        FOREIGN KEY (career_catalog_bundle_id, evidence_catalog_entry_id)
        REFERENCES spec_catalog_entry (career_catalog_bundle_id, id),
    CONSTRAINT ck_activity_catalog_entry_key CHECK (CHAR_LENGTH(activity_key) > 0),
    CONSTRAINT ck_activity_catalog_entry_name CHECK (CHAR_LENGTH(display_name) > 0),
    CONSTRAINT ck_activity_catalog_entry_terms CHECK (
        minimum_calendar_days > 0
        AND required_effort_units > 0
        AND daily_effort_cap_units > 0
        AND daily_effort_cap_units <= required_effort_units
        AND cost_krw >= 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE activity_catalog_allowed_status (
    career_catalog_bundle_id    BIGINT UNSIGNED NOT NULL,
    activity_catalog_entry_id   BIGINT UNSIGNED NOT NULL,
    life_status                 VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (career_catalog_bundle_id, activity_catalog_entry_id, life_status),
    CONSTRAINT fk_activity_catalog_status_entry
        FOREIGN KEY (career_catalog_bundle_id, activity_catalog_entry_id)
        REFERENCES activity_catalog_entry (career_catalog_bundle_id, id),
    CONSTRAINT ck_activity_catalog_status_value CHECK (
        life_status IN (
            'unemployed', 'employed', 'activeDuty', 'socialService',
            'specialService', 'officerOrNco'
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE career_effort_capacity (
    career_catalog_bundle_id    BIGINT UNSIGNED NOT NULL,
    life_status                 VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    effort_units                BIGINT UNSIGNED NOT NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (career_catalog_bundle_id, life_status),
    CONSTRAINT fk_career_effort_capacity_bundle
        FOREIGN KEY (career_catalog_bundle_id) REFERENCES career_catalog_bundle (id),
    CONSTRAINT ck_career_effort_capacity_status CHECK (
        life_status IN (
            'unemployed', 'employed', 'activeDuty', 'socialService',
            'specialService', 'officerOrNco'
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE artifact_checklist_rule (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    career_catalog_bundle_id    BIGINT UNSIGNED NOT NULL,
    artifact_kind               VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    rule_kind                   VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    minimum_count               TINYINT UNSIGNED     NULL,
    dimension                   VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NULL,
    evidence_kind               VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NULL,
    weight_bp                   INT             NOT NULL,
    rule_identity               VARCHAR(128) GENERATED ALWAYS AS (
        CONCAT(
            rule_kind, ':',
            COALESCE(CAST(minimum_count AS CHAR), ''), ':',
            COALESCE(dimension, ''), ':',
            COALESCE(evidence_kind, '')
        )
    ) STORED,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_artifact_checklist_rule_identity
        (career_catalog_bundle_id, artifact_kind, rule_identity),
    UNIQUE KEY uk_artifact_checklist_rule_bundle_id (career_catalog_bundle_id, id),
    CONSTRAINT fk_artifact_checklist_rule_bundle
        FOREIGN KEY (career_catalog_bundle_id) REFERENCES career_catalog_bundle (id),
    CONSTRAINT ck_artifact_checklist_rule_artifact_kind CHECK (
        artifact_kind IN ('portfolio', 'resume', 'linkedinProfile')
    ),
    CONSTRAINT ck_artifact_checklist_rule_weight CHECK (weight_bp BETWEEN 0 AND 10000),
    CONSTRAINT ck_artifact_checklist_rule_shape CHECK (
        (
            rule_kind IN ('headlinePresent', 'summaryPresent')
            AND minimum_count IS NULL
            AND dimension IS NULL
            AND evidence_kind IS NULL
        )
        OR (
            rule_kind = 'minimumEvidenceCount'
            AND minimum_count BETWEEN 1 AND CASE artifact_kind
                WHEN 'portfolio' THEN 12
                WHEN 'resume' THEN 40
                WHEN 'linkedinProfile' THEN 30
            END
            AND dimension IS NULL
            AND evidence_kind IS NULL
        )
        OR (
            rule_kind = 'containsDimension'
            AND minimum_count IS NULL
            AND dimension IN ('education', 'certification', 'language', 'training', 'experience', 'project')
            AND evidence_kind IS NULL
            AND (
                artifact_kind <> 'portfolio'
                OR dimension IN ('certification', 'training', 'project')
            )
        )
        OR (
            rule_kind = 'containsEvidenceKind'
            AND minimum_count IS NULL
            AND dimension IS NULL
            AND evidence_kind IN ('education', 'certification', 'language', 'training', 'experience', 'project')
            AND (
                artifact_kind <> 'portfolio'
                OR evidence_kind IN ('certification', 'training', 'project')
            )
        )
        OR (
            rule_kind = 'projectPresent'
            AND artifact_kind = 'portfolio'
            AND minimum_count IS NULL
            AND dimension IS NULL
            AND evidence_kind IS NULL
        )
        OR (
            rule_kind = 'openToWork'
            AND artifact_kind = 'linkedinProfile'
            AND minimum_count IS NULL
            AND dimension IS NULL
            AND evidence_kind IS NULL
        )
        OR (
            rule_kind = 'industryCountAtLeast'
            AND artifact_kind = 'linkedinProfile'
            AND minimum_count BETWEEN 1 AND 3
            AND dimension IS NULL
            AND evidence_kind IS NULL
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE platform_catalog (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    career_catalog_bundle_id    BIGINT UNSIGNED NOT NULL,
    platform_key                VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                VARCHAR(100)    NOT NULL,
    daily_slot_count            SMALLINT UNSIGNED NOT NULL,
    competition_band            VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    document_review_days        SMALLINT UNSIGNED NOT NULL,
    same_region_only            BOOLEAN         NOT NULL DEFAULT FALSE,
    invitation_source           VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    first_pay_reward_krw        BIGINT          NOT NULL DEFAULT 0,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_platform_catalog_bundle_key (career_catalog_bundle_id, platform_key),
    UNIQUE KEY uk_platform_catalog_bundle_id (career_catalog_bundle_id, id),
    CONSTRAINT fk_platform_catalog_bundle
        FOREIGN KEY (career_catalog_bundle_id) REFERENCES career_catalog_bundle (id),
    CONSTRAINT ck_platform_catalog_key CHECK (
        platform_key IN ('sarangbang', 'jobkorea', 'saramin', 'wanted', 'linkedin', 'work24')
    ),
    CONSTRAINT ck_platform_catalog_name CHECK (CHAR_LENGTH(display_name) > 0),
    CONSTRAINT ck_platform_catalog_terms CHECK (
        daily_slot_count > 0
        AND CHAR_LENGTH(competition_band) > 0
        AND document_review_days > 0
        AND same_region_only IN (FALSE, TRUE)
        AND invitation_source IN ('none', 'resume', 'linkedinProfile')
        AND first_pay_reward_krw >= 0
    ),
    CONSTRAINT ck_platform_catalog_behavior CHECK (
        (platform_key = 'sarangbang' AND same_region_only = TRUE)
        OR (platform_key <> 'sarangbang' AND same_region_only = FALSE)
    ),
    CONSTRAINT ck_platform_catalog_invitation CHECK (
        (platform_key = 'saramin' AND invitation_source = 'resume')
        OR (platform_key = 'linkedin' AND invitation_source = 'linkedinProfile')
        OR (platform_key NOT IN ('saramin', 'linkedin') AND invitation_source = 'none')
    ),
    CONSTRAINT ck_platform_catalog_reward CHECK (
        (platform_key = 'wanted' AND first_pay_reward_krw > 0)
        OR (platform_key <> 'wanted' AND first_pay_reward_krw = 0)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE platform_artifact_requirement (
    career_catalog_bundle_id    BIGINT UNSIGNED NOT NULL,
    platform_catalog_id         BIGINT UNSIGNED NOT NULL,
    artifact_kind               VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (career_catalog_bundle_id, platform_catalog_id, artifact_kind),
    CONSTRAINT fk_platform_artifact_requirement_platform
        FOREIGN KEY (career_catalog_bundle_id, platform_catalog_id)
        REFERENCES platform_catalog (career_catalog_bundle_id, id),
    CONSTRAINT ck_platform_artifact_requirement_kind CHECK (
        artifact_kind IN ('portfolio', 'resume', 'linkedinProfile')
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE platform_industry_weight (
    career_catalog_bundle_id    BIGINT UNSIGNED NOT NULL,
    platform_catalog_id         BIGINT UNSIGNED NOT NULL,
    career_industry_id          BIGINT UNSIGNED NOT NULL,
    weight_bp                   INT             NOT NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (career_catalog_bundle_id, platform_catalog_id, career_industry_id),
    KEY ix_platform_industry_weight_industry
        (career_catalog_bundle_id, career_industry_id, platform_catalog_id),
    CONSTRAINT fk_platform_industry_weight_platform
        FOREIGN KEY (career_catalog_bundle_id, platform_catalog_id)
        REFERENCES platform_catalog (career_catalog_bundle_id, id),
    CONSTRAINT fk_platform_industry_weight_industry
        FOREIGN KEY (career_catalog_bundle_id, career_industry_id)
        REFERENCES career_industry (career_catalog_bundle_id, id),
    CONSTRAINT ck_platform_industry_weight_bp CHECK (weight_bp BETWEEN 0 AND 10000)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE virtual_employer (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    career_catalog_bundle_id    BIGINT UNSIGNED NOT NULL,
    career_industry_id          BIGINT UNSIGNED NOT NULL,
    employer_key                VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                VARCHAR(120)    NOT NULL,
    region                      VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at                  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_virtual_employer_bundle_key (career_catalog_bundle_id, employer_key),
    UNIQUE KEY uk_virtual_employer_bundle_id (career_catalog_bundle_id, id),
    UNIQUE KEY uk_virtual_employer_bundle_industry_id
        (career_catalog_bundle_id, career_industry_id, id),
    CONSTRAINT fk_virtual_employer_bundle
        FOREIGN KEY (career_catalog_bundle_id) REFERENCES career_catalog_bundle (id),
    CONSTRAINT fk_virtual_employer_industry
        FOREIGN KEY (career_catalog_bundle_id, career_industry_id)
        REFERENCES career_industry (career_catalog_bundle_id, id),
    CONSTRAINT ck_virtual_employer_key CHECK (CHAR_LENGTH(employer_key) > 0),
    CONSTRAINT ck_virtual_employer_name CHECK (CHAR_LENGTH(display_name) > 0),
    CONSTRAINT ck_virtual_employer_region CHECK (
        region IN ('capitalArea', 'metropolitan', 'smallCity', 'rural')
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE job_template (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    career_catalog_bundle_id            BIGINT UNSIGNED NOT NULL,
    template_key                        VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    platform_catalog_id                 BIGINT UNSIGNED NOT NULL,
    career_industry_id                  BIGINT UNSIGNED NOT NULL,
    career_job_family_id                BIGINT UNSIGNED NOT NULL,
    virtual_employer_id                 BIGINT UNSIGNED NOT NULL,
    employment_type                     VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    minimum_education                   VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL,
    required_certification_entry_id     BIGINT UNSIGNED     NULL,
    minimum_experience_days             INT UNSIGNED    NOT NULL DEFAULT 0,
    military_requirement                VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    minimum_annual_salary_krw           BIGINT          NOT NULL,
    maximum_annual_salary_krw           BIGINT          NOT NULL,
    salary_step_krw                     BIGINT          NOT NULL,
    interview_delay_days                SMALLINT UNSIGNED NOT NULL,
    offer_expiry_days                   SMALLINT UNSIGNED NOT NULL,
    posting_open_days                   SMALLINT UNSIGNED NOT NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_job_template_bundle_key (career_catalog_bundle_id, template_key),
    UNIQUE KEY uk_job_template_bundle_id (career_catalog_bundle_id, id),
    KEY ix_job_template_platform
        (career_catalog_bundle_id, platform_catalog_id, id),
    KEY ix_job_template_job_family
        (career_catalog_bundle_id, career_job_family_id, id),
    KEY ix_job_template_employer
        (career_catalog_bundle_id, virtual_employer_id, id),
    KEY ix_job_template_required_certification
        (career_catalog_bundle_id, required_certification_entry_id),
    CONSTRAINT fk_job_template_bundle
        FOREIGN KEY (career_catalog_bundle_id) REFERENCES career_catalog_bundle (id),
    CONSTRAINT fk_job_template_platform
        FOREIGN KEY (career_catalog_bundle_id, platform_catalog_id)
        REFERENCES platform_catalog (career_catalog_bundle_id, id),
    CONSTRAINT fk_job_template_job_family
        FOREIGN KEY (career_catalog_bundle_id, career_industry_id, career_job_family_id)
        REFERENCES career_job_family (career_catalog_bundle_id, industry_id, id),
    CONSTRAINT fk_job_template_employer
        FOREIGN KEY (career_catalog_bundle_id, career_industry_id, virtual_employer_id)
        REFERENCES virtual_employer (career_catalog_bundle_id, career_industry_id, id),
    CONSTRAINT fk_job_template_required_certification
        FOREIGN KEY (career_catalog_bundle_id, required_certification_entry_id)
        REFERENCES spec_catalog_entry (career_catalog_bundle_id, id),
    CONSTRAINT ck_job_template_key CHECK (CHAR_LENGTH(template_key) > 0),
    CONSTRAINT ck_job_template_employment_type CHECK (employment_type = 'regular'),
    CONSTRAINT ck_job_template_minimum_education CHECK (
        minimum_education IS NULL
        OR minimum_education IN ('highSchool', 'associate', 'bachelor', 'master', 'doctorate')
    ),
    CONSTRAINT ck_job_template_military_requirement CHECK (
        military_requirement IN ('none', 'completedOrExempt')
    ),
    CONSTRAINT ck_job_template_salary CHECK (
        minimum_annual_salary_krw > 0
        AND maximum_annual_salary_krw >= minimum_annual_salary_krw
        AND salary_step_krw > 0
        AND MOD(minimum_annual_salary_krw, salary_step_krw) = 0
        AND MOD(maximum_annual_salary_krw, salary_step_krw) = 0
    ),
    CONSTRAINT ck_job_template_timing CHECK (
        interview_delay_days > 0
        AND offer_expiry_days > 0
        AND posting_open_days > 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE job_template_dimension_requirement (
    career_catalog_bundle_id    BIGINT UNSIGNED NOT NULL,
    job_template_id             BIGINT UNSIGNED NOT NULL,
    dimension                   VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    required_score_bp           INT             NOT NULL,
    weight_bp                   INT             NOT NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (career_catalog_bundle_id, job_template_id, dimension),
    CONSTRAINT fk_job_template_dimension_template
        FOREIGN KEY (career_catalog_bundle_id, job_template_id)
        REFERENCES job_template (career_catalog_bundle_id, id),
    CONSTRAINT ck_job_template_dimension_value CHECK (
        dimension IN ('education', 'certification', 'language', 'training', 'experience', 'project')
    ),
    CONSTRAINT ck_job_template_dimension_scores CHECK (
        required_score_bp BETWEEN 0 AND 10000
        AND weight_bp BETWEEN 0 AND 10000
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE military_option_version (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    career_catalog_bundle_id            BIGINT UNSIGNED NOT NULL,
    option_key                          VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    service_type                        VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                        VARCHAR(120)    NOT NULL,
    effort_life_status                  VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    compensation_kind                   VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    pay_schedule                        VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    minimum_education                   VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL,
    required_certification_entry_id     BIGINT UNSIGNED     NULL,
    grants_career_experience            BOOLEAN         NOT NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_military_option_version_bundle_key (career_catalog_bundle_id, option_key),
    UNIQUE KEY uk_military_option_version_bundle_service (career_catalog_bundle_id, service_type),
    UNIQUE KEY uk_military_option_version_bundle_id (career_catalog_bundle_id, id),
    KEY ix_military_option_required_certification
        (career_catalog_bundle_id, required_certification_entry_id),
    CONSTRAINT fk_military_option_version_bundle
        FOREIGN KEY (career_catalog_bundle_id) REFERENCES career_catalog_bundle (id),
    CONSTRAINT fk_military_option_required_certification
        FOREIGN KEY (career_catalog_bundle_id, required_certification_entry_id)
        REFERENCES spec_catalog_entry (career_catalog_bundle_id, id),
    CONSTRAINT ck_military_option_version_key CHECK (CHAR_LENGTH(option_key) > 0),
    CONSTRAINT ck_military_option_version_name CHECK (CHAR_LENGTH(display_name) > 0),
    CONSTRAINT ck_military_option_service_type CHECK (
        service_type IN (
            'activeDuty', 'socialService', 'industrialTechnical',
            'professionalResearch', 'commissionedOfficer', 'nonCommissionedOfficer'
        )
    ),
    CONSTRAINT ck_military_option_minimum_education CHECK (
        minimum_education IS NULL
        OR minimum_education IN ('highSchool', 'associate', 'bachelor', 'master', 'doctorate')
    ),
    CONSTRAINT ck_military_option_compensation CHECK (
        (service_type IN ('activeDuty', 'socialService') AND compensation_kind = 'militaryPay')
        OR (
            service_type IN (
                'industrialTechnical', 'professionalResearch',
                'commissionedOfficer', 'nonCommissionedOfficer'
            )
            AND compensation_kind = 'employmentPayroll'
        )
    ),
    CONSTRAINT ck_military_option_effort_status CHECK (
        (service_type = 'activeDuty' AND effort_life_status = 'activeDuty')
        OR (service_type = 'socialService' AND effort_life_status = 'socialService')
        OR (
            service_type IN ('industrialTechnical', 'professionalResearch')
            AND effort_life_status = 'specialService'
        )
        OR (
            service_type IN ('commissionedOfficer', 'nonCommissionedOfficer')
            AND effort_life_status = 'officerOrNco'
        )
    ),
    CONSTRAINT ck_military_option_schedule CHECK (pay_schedule = 'monthly'),
    CONSTRAINT ck_military_option_experience CHECK (
        grants_career_experience IN (FALSE, TRUE)
        AND (
            (service_type IN ('activeDuty', 'socialService') AND grants_career_experience = FALSE)
            OR (
                service_type IN (
                    'industrialTechnical', 'professionalResearch',
                    'commissionedOfficer', 'nonCommissionedOfficer'
                )
                AND grants_career_experience = TRUE
            )
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE military_option_job_family (
    career_catalog_bundle_id    BIGINT UNSIGNED NOT NULL,
    military_option_version_id  BIGINT UNSIGNED NOT NULL,
    career_job_family_id        BIGINT UNSIGNED NOT NULL,
    experience_credit_ppm       INT             NOT NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (
        career_catalog_bundle_id,
        military_option_version_id,
        career_job_family_id
    ),
    KEY ix_military_option_job_family_family
        (career_catalog_bundle_id, career_job_family_id, military_option_version_id),
    CONSTRAINT fk_military_option_job_family_option
        FOREIGN KEY (career_catalog_bundle_id, military_option_version_id)
        REFERENCES military_option_version (career_catalog_bundle_id, id),
    CONSTRAINT fk_military_option_job_family_family
        FOREIGN KEY (career_catalog_bundle_id, career_job_family_id)
        REFERENCES career_job_family (career_catalog_bundle_id, id),
    CONSTRAINT ck_military_option_job_family_credit CHECK (
        experience_credit_ppm BETWEEN 1 AND 1000000
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE military_savings_institution_catalog (
    id                          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    career_catalog_bundle_id    BIGINT UNSIGNED NOT NULL,
    financial_institution_id    BIGINT UNSIGNED NOT NULL,
    created_at                  DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_military_savings_institution_bundle_institution
        (career_catalog_bundle_id, financial_institution_id),
    UNIQUE KEY uk_military_savings_institution_bundle_id
        (career_catalog_bundle_id, id),
    CONSTRAINT fk_military_savings_institution_bundle
        FOREIGN KEY (career_catalog_bundle_id) REFERENCES career_catalog_bundle (id),
    CONSTRAINT fk_military_savings_institution_financial
        FOREIGN KEY (financial_institution_id) REFERENCES financial_institution (id)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE career_catalog_assignment (
    assignment_key                  VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    career_catalog_bundle_id        BIGINT UNSIGNED NOT NULL,
    assignment_revision             BIGINT UNSIGNED NOT NULL DEFAULT 1,
    updated_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (assignment_key),
    KEY ix_career_catalog_assignment_bundle (career_catalog_bundle_id),
    CONSTRAINT fk_career_catalog_assignment_bundle
        FOREIGN KEY (career_catalog_bundle_id) REFERENCES career_catalog_bundle (id),
    CONSTRAINT ck_career_catalog_assignment_key CHECK (assignment_key = 'newRun')
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_career_catalog_bundle_draft_insert
BEFORE INSERT ON career_catalog_bundle
FOR EACH ROW
SET NEW.bundle_key = IF(
    NEW.published_at IS NULL
        AND NEW.default_focused_job_family_key IS NULL,
    NEW.bundle_key,
    NULL
);

-- Publication closes the whole graph atomically. Every predicate is over the same bundle id,
-- so no child from another version can make an incomplete draft appear complete.
CREATE TRIGGER tr_career_catalog_bundle_publish_only
BEFORE UPDATE ON career_catalog_bundle
FOR EACH ROW
SET NEW.bundle_key = IF(
    OLD.published_at IS NULL
        AND NEW.published_at IS NOT NULL
        AND NEW.id = OLD.id
        AND BINARY NEW.bundle_key = BINARY OLD.bundle_key
        AND NEW.ranked_eligible = OLD.ranked_eligible
        AND NEW.created_at = OLD.created_at
        AND NEW.default_focused_job_family_key IS NOT NULL
        AND (
            NEW.ranked_eligible = FALSE
            OR NEW.bundle_key NOT LIKE 'dev-unranked-%'
        )
        AND EXISTS (
            SELECT 1
            FROM career_job_family AS focus
            WHERE focus.career_catalog_bundle_id = NEW.id
              AND BINARY focus.job_family_key = BINARY NEW.default_focused_job_family_key
        )
        AND (
            SELECT COUNT(*)
            FROM career_industry AS industry
            WHERE industry.career_catalog_bundle_id = NEW.id
        ) = 6
        AND NOT EXISTS (
            SELECT 1
            FROM career_industry AS industry
            WHERE industry.career_catalog_bundle_id = NEW.id
              AND (
                  SELECT COUNT(*)
                  FROM career_job_family AS family
                  WHERE family.career_catalog_bundle_id = NEW.id
                    AND family.industry_id = industry.id
              ) < 2
        )
        AND (
            SELECT COUNT(DISTINCT entry.kind)
            FROM spec_catalog_entry AS entry
            WHERE entry.career_catalog_bundle_id = NEW.id
        ) = 6
        AND NOT EXISTS (
            SELECT 1
            FROM spec_catalog_entry AS entry
            CROSS JOIN career_job_family AS family
            WHERE entry.career_catalog_bundle_id = NEW.id
              AND family.career_catalog_bundle_id = NEW.id
              AND NOT EXISTS (
                  SELECT 1
                  FROM spec_catalog_contribution AS contribution
                  WHERE contribution.career_catalog_bundle_id = NEW.id
                    AND contribution.spec_catalog_entry_id = entry.id
                    AND contribution.career_job_family_id = family.id
              )
        )
        AND (
            SELECT COUNT(*)
            FROM career_bridge_education_mapping AS bridge
            INNER JOIN spec_catalog_entry AS entry
                ON entry.career_catalog_bundle_id = bridge.career_catalog_bundle_id
               AND entry.id = bridge.spec_catalog_entry_id
            WHERE bridge.career_catalog_bundle_id = NEW.id
              AND BINARY entry.kind = BINARY 'education'
        ) = 5
        AND (
            SELECT COUNT(*)
            FROM career_bridge_certification_order AS bridge
            INNER JOIN spec_catalog_entry AS entry
                ON entry.career_catalog_bundle_id = bridge.career_catalog_bundle_id
               AND entry.id = bridge.spec_catalog_entry_id
            WHERE bridge.career_catalog_bundle_id = NEW.id
              AND BINARY entry.kind = BINARY 'certification'
        ) = 50
        AND (
            SELECT COUNT(*)
            FROM career_bridge_experience_mapping AS bridge
            INNER JOIN spec_catalog_entry AS entry
                ON entry.career_catalog_bundle_id = bridge.career_catalog_bundle_id
               AND entry.id = bridge.spec_catalog_entry_id
            WHERE bridge.career_catalog_bundle_id = NEW.id
              AND BINARY entry.kind = BINARY 'experience'
        ) = 31
        AND NOT EXISTS (
            SELECT 1
            FROM career_bridge_education_mapping AS education_bridge
            INNER JOIN career_bridge_certification_order AS certification_bridge
                ON certification_bridge.career_catalog_bundle_id = education_bridge.career_catalog_bundle_id
               AND BINARY certification_bridge.evidence_key = BINARY education_bridge.evidence_key
            WHERE education_bridge.career_catalog_bundle_id = NEW.id
        )
        AND NOT EXISTS (
            SELECT 1
            FROM career_bridge_education_mapping AS education_bridge
            INNER JOIN career_bridge_experience_mapping AS experience_bridge
                ON experience_bridge.career_catalog_bundle_id = education_bridge.career_catalog_bundle_id
               AND BINARY experience_bridge.evidence_key = BINARY education_bridge.evidence_key
            WHERE education_bridge.career_catalog_bundle_id = NEW.id
        )
        AND NOT EXISTS (
            SELECT 1
            FROM career_bridge_certification_order AS certification_bridge
            INNER JOIN career_bridge_experience_mapping AS experience_bridge
                ON experience_bridge.career_catalog_bundle_id = certification_bridge.career_catalog_bundle_id
               AND BINARY experience_bridge.evidence_key = BINARY certification_bridge.evidence_key
            WHERE certification_bridge.career_catalog_bundle_id = NEW.id
        )
        AND EXISTS (
            SELECT 1
            FROM activity_catalog_entry AS activity
            WHERE activity.career_catalog_bundle_id = NEW.id
        )
        AND NOT EXISTS (
            SELECT 1
            FROM activity_catalog_entry AS activity
            WHERE activity.career_catalog_bundle_id = NEW.id
              AND NOT EXISTS (
                  SELECT 1
                  FROM activity_catalog_allowed_status AS allowed_status
                  WHERE allowed_status.career_catalog_bundle_id = NEW.id
                    AND allowed_status.activity_catalog_entry_id = activity.id
              )
        )
        AND (
            SELECT COUNT(DISTINCT allowed_status.life_status)
            FROM activity_catalog_allowed_status AS allowed_status
            WHERE allowed_status.career_catalog_bundle_id = NEW.id
        ) = 6
        AND (
            SELECT COUNT(*)
            FROM career_effort_capacity AS capacity
            WHERE capacity.career_catalog_bundle_id = NEW.id
        ) = 6
        AND (
            SELECT COUNT(DISTINCT checklist.artifact_kind)
            FROM artifact_checklist_rule AS checklist
            WHERE checklist.career_catalog_bundle_id = NEW.id
        ) = 3
        AND NOT EXISTS (
            SELECT 1
            FROM artifact_checklist_rule AS checklist
            WHERE checklist.career_catalog_bundle_id = NEW.id
            GROUP BY checklist.artifact_kind
            HAVING SUM(checklist.weight_bp) <> 10000
        )
        AND (
            SELECT COUNT(*)
            FROM platform_catalog AS platform
            WHERE platform.career_catalog_bundle_id = NEW.id
        ) = 6
        AND (
            SELECT COUNT(*)
            FROM platform_artifact_requirement AS requirement
            WHERE requirement.career_catalog_bundle_id = NEW.id
        ) = 7
        AND NOT EXISTS (
            SELECT 1
            FROM platform_artifact_requirement AS requirement
            INNER JOIN platform_catalog AS platform
                ON platform.career_catalog_bundle_id = requirement.career_catalog_bundle_id
               AND platform.id = requirement.platform_catalog_id
            WHERE requirement.career_catalog_bundle_id = NEW.id
              AND NOT (
                  (platform.platform_key IN ('sarangbang', 'jobkorea', 'saramin', 'work24')
                      AND requirement.artifact_kind = 'resume')
                  OR (platform.platform_key = 'wanted'
                      AND requirement.artifact_kind IN ('resume', 'portfolio'))
                  OR (platform.platform_key = 'linkedin'
                      AND requirement.artifact_kind = 'linkedinProfile')
              )
        )
        AND (
            SELECT COUNT(*)
            FROM platform_industry_weight AS platform_weight
            WHERE platform_weight.career_catalog_bundle_id = NEW.id
        ) = 36
        AND NOT EXISTS (
            SELECT 1
            FROM platform_industry_weight AS platform_weight
            WHERE platform_weight.career_catalog_bundle_id = NEW.id
            GROUP BY platform_weight.platform_catalog_id
            HAVING COUNT(*) <> 6 OR SUM(platform_weight.weight_bp) <> 10000
        )
        AND NOT EXISTS (
            SELECT 1
            FROM career_industry AS industry
            WHERE industry.career_catalog_bundle_id = NEW.id
              AND NOT EXISTS (
                  SELECT 1
                  FROM virtual_employer AS employer
                  WHERE employer.career_catalog_bundle_id = NEW.id
                    AND employer.career_industry_id = industry.id
              )
        )
        AND NOT EXISTS (
            SELECT 1
            FROM platform_catalog AS platform
            WHERE platform.career_catalog_bundle_id = NEW.id
              AND NOT EXISTS (
                  SELECT 1
                  FROM job_template AS template
                  WHERE template.career_catalog_bundle_id = NEW.id
                    AND template.platform_catalog_id = platform.id
              )
        )
        AND NOT EXISTS (
            SELECT 1
            FROM career_job_family AS family
            WHERE family.career_catalog_bundle_id = NEW.id
              AND NOT EXISTS (
                  SELECT 1
                  FROM job_template AS template
                  WHERE template.career_catalog_bundle_id = NEW.id
                    AND template.career_job_family_id = family.id
              )
        )
        AND NOT EXISTS (
            SELECT 1
            FROM job_template AS template
            WHERE template.career_catalog_bundle_id = NEW.id
              AND (
                  (
                      SELECT COUNT(*)
                      FROM job_template_dimension_requirement AS requirement
                      WHERE requirement.career_catalog_bundle_id = NEW.id
                        AND requirement.job_template_id = template.id
                  ) <> 6
                  OR (
                      SELECT COALESCE(SUM(requirement.weight_bp), 0)
                      FROM job_template_dimension_requirement AS requirement
                      WHERE requirement.career_catalog_bundle_id = NEW.id
                        AND requirement.job_template_id = template.id
                  ) <> 10000
              )
        )
        AND NOT EXISTS (
            SELECT 1
            FROM job_template AS template
            INNER JOIN spec_catalog_entry AS certification
                ON certification.career_catalog_bundle_id = template.career_catalog_bundle_id
               AND certification.id = template.required_certification_entry_id
            WHERE template.career_catalog_bundle_id = NEW.id
              AND BINARY certification.kind <> BINARY 'certification'
        )
        AND (
            SELECT COUNT(*)
            FROM military_option_version AS military_option
            WHERE military_option.career_catalog_bundle_id = NEW.id
        ) = 6
        AND NOT EXISTS (
            SELECT 1
            FROM military_option_version AS military_option
            WHERE military_option.career_catalog_bundle_id = NEW.id
              AND military_option.grants_career_experience = TRUE
              AND NOT EXISTS (
                  SELECT 1
                  FROM military_option_job_family AS mapping
                  WHERE mapping.career_catalog_bundle_id = NEW.id
                    AND mapping.military_option_version_id = military_option.id
              )
        )
        AND NOT EXISTS (
            SELECT 1
            FROM military_option_job_family AS mapping
            INNER JOIN military_option_version AS military_option
                ON military_option.career_catalog_bundle_id = mapping.career_catalog_bundle_id
               AND military_option.id = mapping.military_option_version_id
            WHERE mapping.career_catalog_bundle_id = NEW.id
              AND military_option.grants_career_experience = FALSE
        )
        AND NOT EXISTS (
            SELECT 1
            FROM military_option_version AS military_option
            INNER JOIN spec_catalog_entry AS certification
                ON certification.career_catalog_bundle_id = military_option.career_catalog_bundle_id
               AND certification.id = military_option.required_certification_entry_id
            WHERE military_option.career_catalog_bundle_id = NEW.id
              AND BINARY certification.kind <> BINARY 'certification'
        )
        AND (
            SELECT COUNT(*)
            FROM military_savings_institution_catalog AS catalog_institution
            WHERE catalog_institution.career_catalog_bundle_id = NEW.id
        ) = 2
        AND (
            SELECT COUNT(*)
            FROM military_savings_institution_catalog AS catalog_institution
            INNER JOIN financial_institution AS institution
                ON institution.id = catalog_institution.financial_institution_id
            WHERE catalog_institution.career_catalog_bundle_id = NEW.id
              AND BINARY institution.institution_key IN (
                  BINARY 'life-bank-a', BINARY 'life-bank-b'
              )
        ) = 2,
    OLD.bundle_key,
    NULL
);

CREATE TRIGGER tr_career_catalog_bundle_no_delete
BEFORE DELETE ON career_catalog_bundle
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'career catalog bundles are immutable';

CREATE TRIGGER tr_career_catalog_assignment_published_insert
BEFORE INSERT ON career_catalog_assignment
FOR EACH ROW
SET
    NEW.assignment_key = IF(
        EXISTS (
            SELECT 1
            FROM career_catalog_bundle AS bundle
            WHERE bundle.id = NEW.career_catalog_bundle_id
              AND bundle.published_at IS NOT NULL
        ),
        NEW.assignment_key,
        NULL
    ),
    NEW.assignment_revision = 1;

CREATE TRIGGER tr_career_catalog_assignment_bump_revision
BEFORE UPDATE ON career_catalog_assignment
FOR EACH ROW
SET
    NEW.assignment_key = IF(
        BINARY NEW.assignment_key = BINARY OLD.assignment_key
            AND EXISTS (
                SELECT 1
                FROM career_catalog_bundle AS bundle
                WHERE bundle.id = NEW.career_catalog_bundle_id
                  AND bundle.published_at IS NOT NULL
            ),
        OLD.assignment_key,
        NULL
    ),
    NEW.assignment_revision = OLD.assignment_revision + 1;

CREATE TRIGGER tr_career_catalog_assignment_no_delete
BEFORE DELETE ON career_catalog_assignment
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'career catalog assignment must be updated in place';

-- Child rows are append-only even while drafting. This keeps publication validation free from
-- concurrent rewrites; a bad draft is abandoned and replaced by a new bundle key.
CREATE TRIGGER tr_career_industry_draft_insert
BEFORE INSERT ON career_industry
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle
        WHERE id = NEW.career_catalog_bundle_id AND published_at IS NULL
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_career_job_family_draft_insert
BEFORE INSERT ON career_job_family
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle
        WHERE id = NEW.career_catalog_bundle_id AND published_at IS NULL
    ),
    NEW.career_catalog_bundle_id,
    NULL
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

CREATE TRIGGER tr_bridge_education_mapping_draft_insert
BEFORE INSERT ON career_bridge_education_mapping
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle
        WHERE id = NEW.career_catalog_bundle_id AND published_at IS NULL
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_bridge_certification_order_draft_insert
BEFORE INSERT ON career_bridge_certification_order
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle
        WHERE id = NEW.career_catalog_bundle_id AND published_at IS NULL
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_bridge_experience_mapping_draft_insert
BEFORE INSERT ON career_bridge_experience_mapping
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle
        WHERE id = NEW.career_catalog_bundle_id AND published_at IS NULL
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_activity_catalog_entry_draft_insert
BEFORE INSERT ON activity_catalog_entry
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle
        WHERE id = NEW.career_catalog_bundle_id AND published_at IS NULL
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_activity_allowed_status_draft_insert
BEFORE INSERT ON activity_catalog_allowed_status
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle
        WHERE id = NEW.career_catalog_bundle_id AND published_at IS NULL
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_career_effort_capacity_draft_insert
BEFORE INSERT ON career_effort_capacity
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle
        WHERE id = NEW.career_catalog_bundle_id AND published_at IS NULL
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_artifact_checklist_rule_draft_insert
BEFORE INSERT ON artifact_checklist_rule
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle
        WHERE id = NEW.career_catalog_bundle_id AND published_at IS NULL
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_platform_catalog_draft_insert
BEFORE INSERT ON platform_catalog
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle
        WHERE id = NEW.career_catalog_bundle_id AND published_at IS NULL
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_platform_artifact_requirement_draft_insert
BEFORE INSERT ON platform_artifact_requirement
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle
        WHERE id = NEW.career_catalog_bundle_id AND published_at IS NULL
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_platform_industry_weight_draft_insert
BEFORE INSERT ON platform_industry_weight
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle
        WHERE id = NEW.career_catalog_bundle_id AND published_at IS NULL
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_virtual_employer_draft_insert
BEFORE INSERT ON virtual_employer
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle
        WHERE id = NEW.career_catalog_bundle_id AND published_at IS NULL
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_job_template_draft_insert
BEFORE INSERT ON job_template
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle
        WHERE id = NEW.career_catalog_bundle_id AND published_at IS NULL
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_job_template_dimension_draft_insert
BEFORE INSERT ON job_template_dimension_requirement
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle
        WHERE id = NEW.career_catalog_bundle_id AND published_at IS NULL
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_military_option_version_draft_insert
BEFORE INSERT ON military_option_version
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle
        WHERE id = NEW.career_catalog_bundle_id AND published_at IS NULL
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_military_option_job_family_draft_insert
BEFORE INSERT ON military_option_job_family
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle
        WHERE id = NEW.career_catalog_bundle_id AND published_at IS NULL
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_military_savings_institution_draft_insert
BEFORE INSERT ON military_savings_institution_catalog
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1 FROM career_catalog_bundle
        WHERE id = NEW.career_catalog_bundle_id AND published_at IS NULL
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_career_industry_no_update
BEFORE UPDATE ON career_industry
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'career industry rows are immutable';

CREATE TRIGGER tr_career_industry_no_delete
BEFORE DELETE ON career_industry
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'career industry rows are immutable';

CREATE TRIGGER tr_career_job_family_no_update
BEFORE UPDATE ON career_job_family
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'career job family rows are immutable';

CREATE TRIGGER tr_career_job_family_no_delete
BEFORE DELETE ON career_job_family
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'career job family rows are immutable';

CREATE TRIGGER tr_spec_catalog_entry_no_update
BEFORE UPDATE ON spec_catalog_entry
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'spec catalog entries are immutable';

CREATE TRIGGER tr_spec_catalog_entry_no_delete
BEFORE DELETE ON spec_catalog_entry
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'spec catalog entries are immutable';

CREATE TRIGGER tr_spec_catalog_contribution_no_update
BEFORE UPDATE ON spec_catalog_contribution
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'spec catalog contributions are immutable';

CREATE TRIGGER tr_spec_catalog_contribution_no_delete
BEFORE DELETE ON spec_catalog_contribution
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'spec catalog contributions are immutable';

CREATE TRIGGER tr_bridge_education_mapping_no_update
BEFORE UPDATE ON career_bridge_education_mapping
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'career education bridge rows are immutable';

CREATE TRIGGER tr_bridge_education_mapping_no_delete
BEFORE DELETE ON career_bridge_education_mapping
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'career education bridge rows are immutable';

CREATE TRIGGER tr_bridge_certification_order_no_update
BEFORE UPDATE ON career_bridge_certification_order
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'career certification bridge rows are immutable';

CREATE TRIGGER tr_bridge_certification_order_no_delete
BEFORE DELETE ON career_bridge_certification_order
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'career certification bridge rows are immutable';

CREATE TRIGGER tr_bridge_experience_mapping_no_update
BEFORE UPDATE ON career_bridge_experience_mapping
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'career experience bridge rows are immutable';

CREATE TRIGGER tr_bridge_experience_mapping_no_delete
BEFORE DELETE ON career_bridge_experience_mapping
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'career experience bridge rows are immutable';

CREATE TRIGGER tr_activity_catalog_entry_no_update
BEFORE UPDATE ON activity_catalog_entry
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'activity catalog entries are immutable';

CREATE TRIGGER tr_activity_catalog_entry_no_delete
BEFORE DELETE ON activity_catalog_entry
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'activity catalog entries are immutable';

CREATE TRIGGER tr_activity_allowed_status_no_update
BEFORE UPDATE ON activity_catalog_allowed_status
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'activity allowed statuses are immutable';

CREATE TRIGGER tr_activity_allowed_status_no_delete
BEFORE DELETE ON activity_catalog_allowed_status
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'activity allowed statuses are immutable';

CREATE TRIGGER tr_career_effort_capacity_no_update
BEFORE UPDATE ON career_effort_capacity
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'career effort capacities are immutable';

CREATE TRIGGER tr_career_effort_capacity_no_delete
BEFORE DELETE ON career_effort_capacity
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'career effort capacities are immutable';

CREATE TRIGGER tr_artifact_checklist_rule_no_update
BEFORE UPDATE ON artifact_checklist_rule
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'artifact checklist rules are immutable';

CREATE TRIGGER tr_artifact_checklist_rule_no_delete
BEFORE DELETE ON artifact_checklist_rule
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'artifact checklist rules are immutable';

CREATE TRIGGER tr_platform_catalog_no_update
BEFORE UPDATE ON platform_catalog
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'platform catalog rows are immutable';

CREATE TRIGGER tr_platform_catalog_no_delete
BEFORE DELETE ON platform_catalog
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'platform catalog rows are immutable';

CREATE TRIGGER tr_platform_artifact_requirement_no_update
BEFORE UPDATE ON platform_artifact_requirement
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'platform artifact requirements are immutable';

CREATE TRIGGER tr_platform_artifact_requirement_no_delete
BEFORE DELETE ON platform_artifact_requirement
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'platform artifact requirements are immutable';

CREATE TRIGGER tr_platform_industry_weight_no_update
BEFORE UPDATE ON platform_industry_weight
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'platform industry weights are immutable';

CREATE TRIGGER tr_platform_industry_weight_no_delete
BEFORE DELETE ON platform_industry_weight
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'platform industry weights are immutable';

CREATE TRIGGER tr_virtual_employer_no_update
BEFORE UPDATE ON virtual_employer
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'virtual employer rows are immutable';

CREATE TRIGGER tr_virtual_employer_no_delete
BEFORE DELETE ON virtual_employer
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'virtual employer rows are immutable';

CREATE TRIGGER tr_job_template_no_update
BEFORE UPDATE ON job_template
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'job template rows are immutable';

CREATE TRIGGER tr_job_template_no_delete
BEFORE DELETE ON job_template
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'job template rows are immutable';

CREATE TRIGGER tr_job_template_dimension_no_update
BEFORE UPDATE ON job_template_dimension_requirement
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'job template dimension requirements are immutable';

CREATE TRIGGER tr_job_template_dimension_no_delete
BEFORE DELETE ON job_template_dimension_requirement
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'job template dimension requirements are immutable';

CREATE TRIGGER tr_military_option_version_no_update
BEFORE UPDATE ON military_option_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military option versions are immutable';

CREATE TRIGGER tr_military_option_version_no_delete
BEFORE DELETE ON military_option_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military option versions are immutable';

CREATE TRIGGER tr_military_option_job_family_no_update
BEFORE UPDATE ON military_option_job_family
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military option job-family rows are immutable';

CREATE TRIGGER tr_military_option_job_family_no_delete
BEFORE DELETE ON military_option_job_family
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military option job-family rows are immutable';

CREATE TRIGGER tr_military_savings_institution_no_update
BEFORE UPDATE ON military_savings_institution_catalog
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military savings institutions are immutable';

CREATE TRIGGER tr_military_savings_institution_no_delete
BEFORE DELETE ON military_savings_institution_catalog
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'military savings institutions are immutable';
