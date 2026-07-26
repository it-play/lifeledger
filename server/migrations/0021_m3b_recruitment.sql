-- M3-B immutable recruitment rules and run-scoped recruitment state (§5–§7, §12–§15).
-- Recruitment rules are versioned separately so a published career catalog is never extended
-- after publication. A compatibility row connects two already-published immutable graphs.

CREATE TABLE recruitment_ruleset (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    ruleset_key                         VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ranked_eligible                     BOOLEAN         NOT NULL DEFAULT FALSE,
    active_application_limit            TINYINT UNSIGNED NOT NULL,
    daily_application_limit             TINYINT UNSIGNED NOT NULL,
    open_invitation_limit               TINYINT UNSIGNED NOT NULL,
    employment_start_delay_days         SMALLINT UNSIGNED NOT NULL,
    payday_day_of_month                 TINYINT UNSIGNED NOT NULL,
    published_at                        DATETIME(3)          NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_recruitment_ruleset_key (ruleset_key),
    UNIQUE KEY uk_recruitment_ruleset_id_key (id, ruleset_key),
    CONSTRAINT ck_recruitment_ruleset_key CHECK (
        CHAR_LENGTH(ruleset_key) > 0
        AND ruleset_key REGEXP '^[a-z0-9][a-z0-9._-]{0,63}$'
    ),
    CONSTRAINT ck_recruitment_ruleset_ranked CHECK (ranked_eligible IN (FALSE, TRUE)),
    CONSTRAINT ck_recruitment_ruleset_limits CHECK (
        active_application_limit BETWEEN 1 AND 10
        AND daily_application_limit BETWEEN 1 AND 3
        AND open_invitation_limit BETWEEN 1 AND 5
    ),
    CONSTRAINT ck_recruitment_ruleset_timing CHECK (
        employment_start_delay_days > 0
        AND payday_day_of_month BETWEEN 1 AND 31
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE recruitment_stage_component_weight (
    recruitment_ruleset_id              BIGINT UNSIGNED NOT NULL,
    stage                               VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    component                           VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    weight_bp                           INT             NOT NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (recruitment_ruleset_id, stage, component),
    CONSTRAINT fk_recruitment_component_weight_ruleset
        FOREIGN KEY (recruitment_ruleset_id) REFERENCES recruitment_ruleset (id),
    CONSTRAINT ck_recruitment_component_weight_stage CHECK (
        stage IN ('document', 'interview', 'invitation')
    ),
    CONSTRAINT ck_recruitment_component_weight_shape CHECK (
        (
            stage = 'document'
            AND component IN ('visibleFit', 'artifactCompleteness', 'platformAffinity')
        )
        OR (
            stage = 'interview'
            AND component IN ('possessedFit', 'experienceProjectFit', 'profileConsistency')
        )
        OR (
            stage = 'invitation'
            AND component IN ('artifactCompleteness', 'languageScore', 'experienceScore')
        )
    ),
    CONSTRAINT ck_recruitment_component_weight_value CHECK (weight_bp BETWEEN 0 AND 10000)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE recruitment_score_band (
    recruitment_ruleset_id              BIGINT UNSIGNED NOT NULL,
    score_band_key                      VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    minimum_score_bp                    INT             NOT NULL,
    maximum_exclusive_score_bp          INT             NOT NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (recruitment_ruleset_id, score_band_key),
    UNIQUE KEY uk_recruitment_score_band_min
        (recruitment_ruleset_id, minimum_score_bp),
    UNIQUE KEY uk_recruitment_score_band_max
        (recruitment_ruleset_id, maximum_exclusive_score_bp),
    CONSTRAINT fk_recruitment_score_band_ruleset
        FOREIGN KEY (recruitment_ruleset_id) REFERENCES recruitment_ruleset (id),
    CONSTRAINT ck_recruitment_score_band_key CHECK (
        CHAR_LENGTH(score_band_key) > 0
        AND score_band_key REGEXP '^[a-z][a-zA-Z0-9]{0,31}$'
    ),
    CONSTRAINT ck_recruitment_score_band_range CHECK (
        minimum_score_bp BETWEEN 0 AND 10000
        AND maximum_exclusive_score_bp BETWEEN 1 AND 10001
        AND minimum_score_bp < maximum_exclusive_score_bp
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE recruitment_pass_probability (
    recruitment_ruleset_id              BIGINT UNSIGNED NOT NULL,
    stage                               VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    competition_band                    VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    score_band_key                      VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    pass_probability_ppm                INT             NOT NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (
        recruitment_ruleset_id,
        stage,
        competition_band,
        score_band_key
    ),
    CONSTRAINT fk_recruitment_pass_probability_band
        FOREIGN KEY (recruitment_ruleset_id, score_band_key)
        REFERENCES recruitment_score_band (recruitment_ruleset_id, score_band_key),
    CONSTRAINT ck_recruitment_pass_probability_stage CHECK (
        stage IN ('document', 'interview', 'invitation')
    ),
    CONSTRAINT ck_recruitment_pass_probability_competition CHECK (
        competition_band IN ('low', 'medium', 'high')
    ),
    CONSTRAINT ck_recruitment_pass_probability_value CHECK (
        pass_probability_ppm BETWEEN 0 AND 1000000
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE career_recruitment_compatibility (
    career_catalog_bundle_id            BIGINT UNSIGNED NOT NULL,
    recruitment_ruleset_id              BIGINT UNSIGNED NOT NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (career_catalog_bundle_id, recruitment_ruleset_id),
    KEY ix_career_recruitment_compatibility_ruleset (recruitment_ruleset_id),
    CONSTRAINT fk_career_recruitment_compatibility_bundle
        FOREIGN KEY (career_catalog_bundle_id) REFERENCES career_catalog_bundle (id),
    CONSTRAINT fk_career_recruitment_compatibility_ruleset
        FOREIGN KEY (recruitment_ruleset_id) REFERENCES recruitment_ruleset (id)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE recruitment_ruleset_assignment (
    career_catalog_bundle_id            BIGINT UNSIGNED NOT NULL,
    assignment_key                      VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    recruitment_ruleset_id              BIGINT UNSIGNED NOT NULL,
    assignment_revision                 BIGINT UNSIGNED NOT NULL DEFAULT 1,
    updated_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (career_catalog_bundle_id, assignment_key),
    KEY ix_recruitment_ruleset_assignment_ruleset (recruitment_ruleset_id),
    CONSTRAINT fk_recruitment_ruleset_assignment_compatibility
        FOREIGN KEY (career_catalog_bundle_id, recruitment_ruleset_id)
        REFERENCES career_recruitment_compatibility (
            career_catalog_bundle_id,
            recruitment_ruleset_id
        ),
    CONSTRAINT ck_recruitment_ruleset_assignment_key CHECK (assignment_key = 'newPosting')
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_recruitment_ruleset_draft_insert
BEFORE INSERT ON recruitment_ruleset
FOR EACH ROW
SET NEW.ruleset_key = IF(NEW.published_at IS NULL, NEW.ruleset_key, NULL);

CREATE TRIGGER tr_recruitment_ruleset_publish_only
BEFORE UPDATE ON recruitment_ruleset
FOR EACH ROW
SET NEW.ruleset_key = IF(
    OLD.published_at IS NULL
        AND NEW.published_at IS NOT NULL
        AND NEW.id = OLD.id
        AND BINARY NEW.ruleset_key = BINARY OLD.ruleset_key
        AND NEW.ranked_eligible = OLD.ranked_eligible
        AND NEW.active_application_limit = OLD.active_application_limit
        AND NEW.daily_application_limit = OLD.daily_application_limit
        AND NEW.open_invitation_limit = OLD.open_invitation_limit
        AND NEW.employment_start_delay_days = OLD.employment_start_delay_days
        AND NEW.payday_day_of_month = OLD.payday_day_of_month
        AND NEW.created_at = OLD.created_at
        AND (
            SELECT COUNT(*)
            FROM recruitment_stage_component_weight AS component
            WHERE component.recruitment_ruleset_id = OLD.id
        ) = 9
        AND NOT EXISTS (
            SELECT 1
            FROM recruitment_stage_component_weight AS component
            WHERE component.recruitment_ruleset_id = OLD.id
            GROUP BY component.stage
            HAVING COUNT(*) <> 3 OR SUM(component.weight_bp) <> 10000
        )
        AND EXISTS (
            SELECT 1
            FROM recruitment_score_band AS band
            WHERE band.recruitment_ruleset_id = OLD.id
              AND band.minimum_score_bp = 0
        )
        AND EXISTS (
            SELECT 1
            FROM recruitment_score_band AS band
            WHERE band.recruitment_ruleset_id = OLD.id
              AND band.maximum_exclusive_score_bp = 10001
        )
        AND NOT EXISTS (
            SELECT 1
            FROM recruitment_score_band AS band
            WHERE band.recruitment_ruleset_id = OLD.id
              AND band.minimum_score_bp > 0
              AND NOT EXISTS (
                  SELECT 1
                  FROM recruitment_score_band AS predecessor
                  WHERE predecessor.recruitment_ruleset_id = OLD.id
                    AND predecessor.maximum_exclusive_score_bp = band.minimum_score_bp
              )
        )
        AND NOT EXISTS (
            SELECT 1
            FROM recruitment_score_band AS band
            WHERE band.recruitment_ruleset_id = OLD.id
              AND band.maximum_exclusive_score_bp < 10001
              AND NOT EXISTS (
                  SELECT 1
                  FROM recruitment_score_band AS successor
                  WHERE successor.recruitment_ruleset_id = OLD.id
                    AND successor.minimum_score_bp = band.maximum_exclusive_score_bp
              )
        )
        AND NOT EXISTS (
            SELECT 1
            FROM recruitment_score_band AS band
            CROSS JOIN (
                SELECT 'document' AS stage
                UNION ALL SELECT 'interview'
                UNION ALL SELECT 'invitation'
            ) AS stage
            CROSS JOIN (
                SELECT 'low' AS competition_band
                UNION ALL SELECT 'medium'
                UNION ALL SELECT 'high'
            ) AS competition
            WHERE band.recruitment_ruleset_id = OLD.id
              AND NOT EXISTS (
                  SELECT 1
                  FROM recruitment_pass_probability AS probability
                  WHERE probability.recruitment_ruleset_id = OLD.id
                    AND BINARY probability.stage = BINARY stage.stage
                    AND BINARY probability.competition_band
                        = BINARY competition.competition_band
                    AND BINARY probability.score_band_key = BINARY band.score_band_key
              )
        )
        AND (
            BINARY NEW.ruleset_key <> BINARY 'dev-unranked-m3-recruitment-v1'
            OR (
                NEW.ranked_eligible = FALSE
                AND NEW.active_application_limit = 10
                AND NEW.daily_application_limit = 3
                AND NEW.open_invitation_limit = 5
                AND NEW.employment_start_delay_days = 1
                AND NEW.payday_day_of_month = 25
                AND EXISTS (
                    SELECT 1 FROM recruitment_stage_component_weight
                    WHERE recruitment_ruleset_id = OLD.id
                      AND stage = 'document' AND component = 'visibleFit'
                      AND weight_bp = 6000
                )
                AND EXISTS (
                    SELECT 1 FROM recruitment_stage_component_weight
                    WHERE recruitment_ruleset_id = OLD.id
                      AND stage = 'document' AND component = 'artifactCompleteness'
                      AND weight_bp = 2500
                )
                AND EXISTS (
                    SELECT 1 FROM recruitment_stage_component_weight
                    WHERE recruitment_ruleset_id = OLD.id
                      AND stage = 'document' AND component = 'platformAffinity'
                      AND weight_bp = 1500
                )
                AND EXISTS (
                    SELECT 1 FROM recruitment_stage_component_weight
                    WHERE recruitment_ruleset_id = OLD.id
                      AND stage = 'interview' AND component = 'possessedFit'
                      AND weight_bp = 6000
                )
                AND EXISTS (
                    SELECT 1 FROM recruitment_stage_component_weight
                    WHERE recruitment_ruleset_id = OLD.id
                      AND stage = 'interview' AND component = 'experienceProjectFit'
                      AND weight_bp = 2500
                )
                AND EXISTS (
                    SELECT 1 FROM recruitment_stage_component_weight
                    WHERE recruitment_ruleset_id = OLD.id
                      AND stage = 'interview' AND component = 'profileConsistency'
                      AND weight_bp = 1500
                )
                AND EXISTS (
                    SELECT 1 FROM recruitment_stage_component_weight
                    WHERE recruitment_ruleset_id = OLD.id
                      AND stage = 'invitation' AND component = 'artifactCompleteness'
                      AND weight_bp = 5000
                )
                AND EXISTS (
                    SELECT 1 FROM recruitment_stage_component_weight
                    WHERE recruitment_ruleset_id = OLD.id
                      AND stage = 'invitation' AND component = 'languageScore'
                      AND weight_bp = 2500
                )
                AND EXISTS (
                    SELECT 1 FROM recruitment_stage_component_weight
                    WHERE recruitment_ruleset_id = OLD.id
                      AND stage = 'invitation' AND component = 'experienceScore'
                      AND weight_bp = 2500
                )
                AND (
                    SELECT COUNT(*) FROM recruitment_score_band
                    WHERE recruitment_ruleset_id = OLD.id
                ) = 3
                AND EXISTS (
                    SELECT 1 FROM recruitment_score_band
                    WHERE recruitment_ruleset_id = OLD.id
                      AND score_band_key = 'low'
                      AND minimum_score_bp = 0
                      AND maximum_exclusive_score_bp = 4000
                )
                AND EXISTS (
                    SELECT 1 FROM recruitment_score_band
                    WHERE recruitment_ruleset_id = OLD.id
                      AND score_band_key = 'medium'
                      AND minimum_score_bp = 4000
                      AND maximum_exclusive_score_bp = 7000
                )
                AND EXISTS (
                    SELECT 1 FROM recruitment_score_band
                    WHERE recruitment_ruleset_id = OLD.id
                      AND score_band_key = 'high'
                      AND minimum_score_bp = 7000
                      AND maximum_exclusive_score_bp = 10001
                )
                AND (
                    SELECT COUNT(*) FROM recruitment_pass_probability
                    WHERE recruitment_ruleset_id = OLD.id
                ) = 27
                AND NOT EXISTS (
                    SELECT 1
                    FROM recruitment_pass_probability AS probability
                    WHERE probability.recruitment_ruleset_id = OLD.id
                      AND probability.pass_probability_ppm <> CASE
                          WHEN probability.stage = 'document'
                              AND probability.competition_band = 'low'
                              AND probability.score_band_key = 'low' THEN 400000
                          WHEN probability.stage = 'document'
                              AND probability.competition_band = 'low'
                              AND probability.score_band_key = 'medium' THEN 700000
                          WHEN probability.stage = 'document'
                              AND probability.competition_band = 'low'
                              AND probability.score_band_key = 'high' THEN 900000
                          WHEN probability.stage = 'document'
                              AND probability.competition_band = 'medium'
                              AND probability.score_band_key = 'low' THEN 250000
                          WHEN probability.stage = 'document'
                              AND probability.competition_band = 'medium'
                              AND probability.score_band_key = 'medium' THEN 550000
                          WHEN probability.stage = 'document'
                              AND probability.competition_band = 'medium'
                              AND probability.score_band_key = 'high' THEN 800000
                          WHEN probability.stage = 'document'
                              AND probability.competition_band = 'high'
                              AND probability.score_band_key = 'low' THEN 120000
                          WHEN probability.stage = 'document'
                              AND probability.competition_band = 'high'
                              AND probability.score_band_key = 'medium' THEN 350000
                          WHEN probability.stage = 'document'
                              AND probability.competition_band = 'high'
                              AND probability.score_band_key = 'high' THEN 650000
                          WHEN probability.stage = 'interview'
                              AND probability.competition_band = 'low'
                              AND probability.score_band_key = 'low' THEN 350000
                          WHEN probability.stage = 'interview'
                              AND probability.competition_band = 'low'
                              AND probability.score_band_key = 'medium' THEN 650000
                          WHEN probability.stage = 'interview'
                              AND probability.competition_band = 'low'
                              AND probability.score_band_key = 'high' THEN 880000
                          WHEN probability.stage = 'interview'
                              AND probability.competition_band = 'medium'
                              AND probability.score_band_key = 'low' THEN 220000
                          WHEN probability.stage = 'interview'
                              AND probability.competition_band = 'medium'
                              AND probability.score_band_key = 'medium' THEN 500000
                          WHEN probability.stage = 'interview'
                              AND probability.competition_band = 'medium'
                              AND probability.score_band_key = 'high' THEN 760000
                          WHEN probability.stage = 'interview'
                              AND probability.competition_band = 'high'
                              AND probability.score_band_key = 'low' THEN 100000
                          WHEN probability.stage = 'interview'
                              AND probability.competition_band = 'high'
                              AND probability.score_band_key = 'medium' THEN 300000
                          WHEN probability.stage = 'interview'
                              AND probability.competition_band = 'high'
                              AND probability.score_band_key = 'high' THEN 600000
                          WHEN probability.stage = 'invitation'
                              AND probability.competition_band = 'low'
                              AND probability.score_band_key = 'low' THEN 50000
                          WHEN probability.stage = 'invitation'
                              AND probability.competition_band = 'low'
                              AND probability.score_band_key = 'medium' THEN 150000
                          WHEN probability.stage = 'invitation'
                              AND probability.competition_band = 'low'
                              AND probability.score_band_key = 'high' THEN 300000
                          WHEN probability.stage = 'invitation'
                              AND probability.competition_band = 'medium'
                              AND probability.score_band_key = 'low' THEN 35000
                          WHEN probability.stage = 'invitation'
                              AND probability.competition_band = 'medium'
                              AND probability.score_band_key = 'medium' THEN 120000
                          WHEN probability.stage = 'invitation'
                              AND probability.competition_band = 'medium'
                              AND probability.score_band_key = 'high' THEN 250000
                          WHEN probability.stage = 'invitation'
                              AND probability.competition_band = 'high'
                              AND probability.score_band_key = 'low' THEN 20000
                          WHEN probability.stage = 'invitation'
                              AND probability.competition_band = 'high'
                              AND probability.score_band_key = 'medium' THEN 80000
                          WHEN probability.stage = 'invitation'
                              AND probability.competition_band = 'high'
                              AND probability.score_band_key = 'high' THEN 200000
                          ELSE -1
                      END
                )
            )
        ),
    OLD.ruleset_key,
    NULL
);

CREATE TRIGGER tr_recruitment_ruleset_no_delete
BEFORE DELETE ON recruitment_ruleset
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'recruitment rulesets are immutable';

CREATE TRIGGER tr_recruitment_score_band_draft_insert
BEFORE INSERT ON recruitment_score_band
FOR EACH ROW
SET NEW.recruitment_ruleset_id = IF(
    EXISTS (
        SELECT 1 FROM recruitment_ruleset
        WHERE id = NEW.recruitment_ruleset_id AND published_at IS NULL
    ),
    NEW.recruitment_ruleset_id,
    NULL
);

CREATE TRIGGER tr_recruitment_score_band_no_update
BEFORE UPDATE ON recruitment_score_band
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'recruitment score bands are immutable';

CREATE TRIGGER tr_recruitment_score_band_no_delete
BEFORE DELETE ON recruitment_score_band
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'recruitment score bands are immutable';

CREATE TRIGGER tr_recruitment_component_weight_draft_insert
BEFORE INSERT ON recruitment_stage_component_weight
FOR EACH ROW
SET NEW.recruitment_ruleset_id = IF(
    EXISTS (
        SELECT 1 FROM recruitment_ruleset
        WHERE id = NEW.recruitment_ruleset_id AND published_at IS NULL
    ),
    NEW.recruitment_ruleset_id,
    NULL
);

CREATE TRIGGER tr_recruitment_component_weight_no_update
BEFORE UPDATE ON recruitment_stage_component_weight
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'recruitment component weights are immutable';

CREATE TRIGGER tr_recruitment_component_weight_no_delete
BEFORE DELETE ON recruitment_stage_component_weight
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'recruitment component weights are immutable';

CREATE TRIGGER tr_recruitment_probability_draft_insert
BEFORE INSERT ON recruitment_pass_probability
FOR EACH ROW
SET NEW.recruitment_ruleset_id = IF(
    EXISTS (
        SELECT 1 FROM recruitment_ruleset
        WHERE id = NEW.recruitment_ruleset_id AND published_at IS NULL
    ),
    NEW.recruitment_ruleset_id,
    NULL
);

CREATE TRIGGER tr_recruitment_probability_no_update
BEFORE UPDATE ON recruitment_pass_probability
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'recruitment probabilities are immutable';

CREATE TRIGGER tr_recruitment_probability_no_delete
BEFORE DELETE ON recruitment_pass_probability
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'recruitment probabilities are immutable';

CREATE TRIGGER tr_career_recruitment_compatibility_valid_insert
BEFORE INSERT ON career_recruitment_compatibility
FOR EACH ROW
SET NEW.career_catalog_bundle_id = IF(
    EXISTS (
        SELECT 1
        FROM career_catalog_bundle AS bundle
        INNER JOIN recruitment_ruleset AS ruleset
            ON ruleset.id = NEW.recruitment_ruleset_id
        WHERE bundle.id = NEW.career_catalog_bundle_id
          AND bundle.published_at IS NOT NULL
          AND ruleset.published_at IS NOT NULL
          AND (bundle.ranked_eligible = FALSE OR ruleset.ranked_eligible = TRUE)
          AND NOT EXISTS (
              SELECT 1
              FROM platform_catalog AS platform
              WHERE platform.career_catalog_bundle_id = bundle.id
                AND NOT EXISTS (
                    SELECT 1
                    FROM recruitment_pass_probability AS probability
                    WHERE probability.recruitment_ruleset_id = ruleset.id
                      AND BINARY probability.competition_band
                          = BINARY platform.competition_band
                )
          )
          AND NOT EXISTS (
              SELECT 1
              FROM job_template AS template
              WHERE template.career_catalog_bundle_id = bundle.id
                AND ((template.maximum_annual_salary_krw
                    - template.minimum_annual_salary_krw)
                    DIV template.salary_step_krw) + 1 < 3
          )
    ),
    NEW.career_catalog_bundle_id,
    NULL
);

CREATE TRIGGER tr_career_recruitment_compatibility_no_update
BEFORE UPDATE ON career_recruitment_compatibility
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'career recruitment compatibility is immutable';

CREATE TRIGGER tr_career_recruitment_compatibility_no_delete
BEFORE DELETE ON career_recruitment_compatibility
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'career recruitment compatibility is immutable';

CREATE TRIGGER tr_recruitment_assignment_valid_insert
BEFORE INSERT ON recruitment_ruleset_assignment
FOR EACH ROW
SET NEW.recruitment_ruleset_id = IF(
    NEW.assignment_revision = 1
        AND EXISTS (
            SELECT 1
            FROM career_recruitment_compatibility AS compatibility
            INNER JOIN recruitment_ruleset AS ruleset
                ON ruleset.id = compatibility.recruitment_ruleset_id
            WHERE compatibility.career_catalog_bundle_id
                    = NEW.career_catalog_bundle_id
              AND compatibility.recruitment_ruleset_id
                    = NEW.recruitment_ruleset_id
              AND ruleset.published_at IS NOT NULL
        ),
    NEW.recruitment_ruleset_id,
    NULL
);

CREATE TRIGGER tr_recruitment_assignment_bump_revision
BEFORE UPDATE ON recruitment_ruleset_assignment
FOR EACH ROW
SET
    NEW.assignment_key = IF(
        BINARY NEW.assignment_key = BINARY OLD.assignment_key
            AND NEW.career_catalog_bundle_id = OLD.career_catalog_bundle_id
            AND NEW.recruitment_ruleset_id <> OLD.recruitment_ruleset_id
            AND NEW.assignment_revision = OLD.assignment_revision
            AND EXISTS (
                SELECT 1
                FROM career_recruitment_compatibility AS compatibility
                INNER JOIN recruitment_ruleset AS ruleset
                    ON ruleset.id = compatibility.recruitment_ruleset_id
                WHERE compatibility.career_catalog_bundle_id
                        = OLD.career_catalog_bundle_id
                  AND ruleset.id = NEW.recruitment_ruleset_id
                  AND ruleset.published_at IS NOT NULL
            ),
        NEW.assignment_key,
        NULL
    ),
    NEW.assignment_revision = OLD.assignment_revision + 1;

CREATE TRIGGER tr_recruitment_assignment_no_delete
BEFORE DELETE ON recruitment_ruleset_assignment
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'recruitment ruleset assignments cannot be deleted';

CREATE TABLE job_posting (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    posting_key                         CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    market_world_id                     BIGINT UNSIGNED NOT NULL,
    career_catalog_bundle_id            BIGINT UNSIGNED NOT NULL,
    recruitment_ruleset_id              BIGINT UNSIGNED NOT NULL,
    platform_catalog_id                 BIGINT UNSIGNED NOT NULL,
    job_template_id                     BIGINT UNSIGNED NOT NULL,
    career_industry_id                  BIGINT UNSIGNED NOT NULL,
    career_job_family_id                BIGINT UNSIGNED NOT NULL,
    virtual_employer_id                 BIGINT UNSIGNED NOT NULL,
    slot_no                             SMALLINT UNSIGNED NOT NULL,
    posted_game_day                     INT UNSIGNED    NOT NULL,
    closes_exclusive_game_day           INT UNSIGNED    NOT NULL,
    region                              VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    employment_type                     VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    competition_band                    VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    minimum_education                   VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL,
    required_certification_entry_id     BIGINT UNSIGNED     NULL,
    minimum_experience_days             INT UNSIGNED    NOT NULL,
    military_requirement                VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    requires_resume                     BOOLEAN         NOT NULL,
    requires_portfolio                  BOOLEAN         NOT NULL,
    requires_linkedin_profile           BOOLEAN         NOT NULL,
    minimum_annual_salary_krw           BIGINT          NOT NULL,
    maximum_annual_salary_krw           BIGINT          NOT NULL,
    salary_step_krw                     BIGINT          NOT NULL,
    document_review_days                SMALLINT UNSIGNED NOT NULL,
    interview_delay_days                SMALLINT UNSIGNED NOT NULL,
    offer_expiry_days                   SMALLINT UNSIGNED NOT NULL,
    education_required_score_bp         INT             NOT NULL,
    education_weight_bp                 INT             NOT NULL,
    certification_required_score_bp     INT             NOT NULL,
    certification_weight_bp             INT             NOT NULL,
    language_required_score_bp          INT             NOT NULL,
    language_weight_bp                  INT             NOT NULL,
    training_required_score_bp          INT             NOT NULL,
    training_weight_bp                  INT             NOT NULL,
    experience_required_score_bp        INT             NOT NULL,
    experience_weight_bp                INT             NOT NULL,
    project_required_score_bp           INT             NOT NULL,
    project_weight_bp                   INT             NOT NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_job_posting_key (posting_key),
    UNIQUE KEY uk_job_posting_bundle_id (career_catalog_bundle_id, id),
    UNIQUE KEY uk_job_posting_occurrence (
        market_world_id,
        career_catalog_bundle_id,
        posted_game_day,
        platform_catalog_id,
        slot_no
    ),
    KEY ix_job_posting_feed (
        market_world_id,
        career_catalog_bundle_id,
        posted_game_day,
        platform_catalog_id,
        id
    ),
    KEY ix_job_posting_template
        (career_catalog_bundle_id, job_template_id),
    KEY ix_job_posting_job_family
        (career_catalog_bundle_id, career_job_family_id),
    CONSTRAINT fk_job_posting_world
        FOREIGN KEY (market_world_id) REFERENCES market_world (id),
    CONSTRAINT fk_job_posting_compatibility
        FOREIGN KEY (career_catalog_bundle_id, recruitment_ruleset_id)
        REFERENCES career_recruitment_compatibility (
            career_catalog_bundle_id,
            recruitment_ruleset_id
        ),
    CONSTRAINT fk_job_posting_platform
        FOREIGN KEY (career_catalog_bundle_id, platform_catalog_id)
        REFERENCES platform_catalog (career_catalog_bundle_id, id),
    CONSTRAINT fk_job_posting_template
        FOREIGN KEY (career_catalog_bundle_id, job_template_id)
        REFERENCES job_template (career_catalog_bundle_id, id),
    CONSTRAINT fk_job_posting_industry
        FOREIGN KEY (career_catalog_bundle_id, career_industry_id)
        REFERENCES career_industry (career_catalog_bundle_id, id),
    CONSTRAINT fk_job_posting_job_family
        FOREIGN KEY (career_catalog_bundle_id, career_job_family_id)
        REFERENCES career_job_family (career_catalog_bundle_id, id),
    CONSTRAINT fk_job_posting_employer
        FOREIGN KEY (career_catalog_bundle_id, virtual_employer_id)
        REFERENCES virtual_employer (career_catalog_bundle_id, id),
    CONSTRAINT fk_job_posting_certification
        FOREIGN KEY (career_catalog_bundle_id, required_certification_entry_id)
        REFERENCES spec_catalog_entry (career_catalog_bundle_id, id),
    CONSTRAINT ck_job_posting_key CHECK (
        posting_key REGEXP '^[0-9a-f]{64}$'
    ),
    CONSTRAINT ck_job_posting_timing CHECK (
        closes_exclusive_game_day > posted_game_day
        AND document_review_days > 0
        AND interview_delay_days > 0
        AND offer_expiry_days > 0
    ),
    CONSTRAINT ck_job_posting_region CHECK (
        region IN ('capitalArea', 'metropolitan', 'smallCity', 'rural')
    ),
    CONSTRAINT ck_job_posting_employment_type CHECK (employment_type = 'regular'),
    CONSTRAINT ck_job_posting_competition CHECK (
        competition_band IN ('low', 'medium', 'high')
    ),
    CONSTRAINT ck_job_posting_minimum_education CHECK (
        minimum_education IS NULL
        OR minimum_education IN ('highSchool', 'associate', 'bachelor', 'master', 'doctorate')
    ),
    CONSTRAINT ck_job_posting_military_requirement CHECK (
        military_requirement IN ('none', 'completedOrExempt')
    ),
    CONSTRAINT ck_job_posting_artifact_flags CHECK (
        requires_resume IN (FALSE, TRUE)
        AND requires_portfolio IN (FALSE, TRUE)
        AND requires_linkedin_profile IN (FALSE, TRUE)
        AND requires_resume + requires_portfolio + requires_linkedin_profile BETWEEN 1 AND 2
    ),
    CONSTRAINT ck_job_posting_salary_api_bound CHECK (
        minimum_annual_salary_krw > 0
        AND maximum_annual_salary_krw >= minimum_annual_salary_krw
        AND maximum_annual_salary_krw <= 9007199254740991
        AND salary_step_krw > 0
        AND salary_step_krw <= 9007199254740991
        AND MOD(minimum_annual_salary_krw, salary_step_krw) = 0
        AND MOD(maximum_annual_salary_krw, salary_step_krw) = 0
    ),
    CONSTRAINT ck_job_posting_dimension_scores CHECK (
        education_required_score_bp BETWEEN 0 AND 10000
        AND education_weight_bp BETWEEN 0 AND 10000
        AND certification_required_score_bp BETWEEN 0 AND 10000
        AND certification_weight_bp BETWEEN 0 AND 10000
        AND language_required_score_bp BETWEEN 0 AND 10000
        AND language_weight_bp BETWEEN 0 AND 10000
        AND training_required_score_bp BETWEEN 0 AND 10000
        AND training_weight_bp BETWEEN 0 AND 10000
        AND experience_required_score_bp BETWEEN 0 AND 10000
        AND experience_weight_bp BETWEEN 0 AND 10000
        AND project_required_score_bp BETWEEN 0 AND 10000
        AND project_weight_bp BETWEEN 0 AND 10000
        AND education_weight_bp + certification_weight_bp + language_weight_bp
            + training_weight_bp + experience_weight_bp + project_weight_bp = 10000
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_job_posting_valid_insert
BEFORE INSERT ON job_posting
FOR EACH ROW
SET NEW.posting_key = IF(
    EXISTS (
        SELECT 1
        FROM recruitment_ruleset_assignment AS assignment
        INNER JOIN recruitment_ruleset AS ruleset
            ON ruleset.id = assignment.recruitment_ruleset_id
           AND ruleset.published_at IS NOT NULL
        INNER JOIN career_recruitment_compatibility AS compatibility
            ON compatibility.recruitment_ruleset_id = ruleset.id
           AND compatibility.career_catalog_bundle_id = NEW.career_catalog_bundle_id
        INNER JOIN platform_catalog AS platform
            ON platform.career_catalog_bundle_id = compatibility.career_catalog_bundle_id
           AND platform.id = NEW.platform_catalog_id
        INNER JOIN job_template AS template
            ON template.career_catalog_bundle_id = compatibility.career_catalog_bundle_id
           AND template.id = NEW.job_template_id
           AND template.platform_catalog_id = platform.id
           AND template.career_industry_id = NEW.career_industry_id
           AND template.career_job_family_id = NEW.career_job_family_id
           AND template.virtual_employer_id = NEW.virtual_employer_id
        INNER JOIN virtual_employer AS employer
            ON employer.career_catalog_bundle_id = template.career_catalog_bundle_id
           AND employer.id = template.virtual_employer_id
        WHERE BINARY assignment.assignment_key = BINARY 'newPosting'
          AND assignment.career_catalog_bundle_id = NEW.career_catalog_bundle_id
          AND assignment.recruitment_ruleset_id = NEW.recruitment_ruleset_id
          AND NEW.slot_no < platform.daily_slot_count
          AND BINARY NEW.region = BINARY employer.region
          AND BINARY NEW.employment_type = BINARY template.employment_type
          AND BINARY NEW.competition_band = BINARY platform.competition_band
          AND NEW.minimum_education <=> template.minimum_education
          AND NEW.required_certification_entry_id
              <=> template.required_certification_entry_id
          AND NEW.minimum_experience_days = template.minimum_experience_days
          AND BINARY NEW.military_requirement = BINARY template.military_requirement
          AND NEW.minimum_annual_salary_krw = template.minimum_annual_salary_krw
          AND NEW.maximum_annual_salary_krw = template.maximum_annual_salary_krw
          AND NEW.salary_step_krw = template.salary_step_krw
          AND NEW.closes_exclusive_game_day
              = NEW.posted_game_day + template.posting_open_days
          AND NEW.document_review_days = platform.document_review_days
          AND NEW.interview_delay_days = template.interview_delay_days
          AND NEW.offer_expiry_days = template.offer_expiry_days
          AND NEW.requires_resume = EXISTS (
              SELECT 1 FROM platform_artifact_requirement AS requirement
              WHERE requirement.career_catalog_bundle_id = platform.career_catalog_bundle_id
                AND requirement.platform_catalog_id = platform.id
                AND BINARY requirement.artifact_kind = BINARY 'resume'
          )
          AND NEW.requires_portfolio = EXISTS (
              SELECT 1 FROM platform_artifact_requirement AS requirement
              WHERE requirement.career_catalog_bundle_id = platform.career_catalog_bundle_id
                AND requirement.platform_catalog_id = platform.id
                AND BINARY requirement.artifact_kind = BINARY 'portfolio'
          )
          AND NEW.requires_linkedin_profile = EXISTS (
              SELECT 1 FROM platform_artifact_requirement AS requirement
              WHERE requirement.career_catalog_bundle_id = platform.career_catalog_bundle_id
                AND requirement.platform_catalog_id = platform.id
                AND BINARY requirement.artifact_kind = BINARY 'linkedinProfile'
          )
          AND NEW.education_required_score_bp = (
              SELECT required_score_bp FROM job_template_dimension_requirement
              WHERE career_catalog_bundle_id = template.career_catalog_bundle_id
                AND job_template_id = template.id AND dimension = 'education'
          )
          AND NEW.education_weight_bp = (
              SELECT weight_bp FROM job_template_dimension_requirement
              WHERE career_catalog_bundle_id = template.career_catalog_bundle_id
                AND job_template_id = template.id AND dimension = 'education'
          )
          AND NEW.certification_required_score_bp = (
              SELECT required_score_bp FROM job_template_dimension_requirement
              WHERE career_catalog_bundle_id = template.career_catalog_bundle_id
                AND job_template_id = template.id AND dimension = 'certification'
          )
          AND NEW.certification_weight_bp = (
              SELECT weight_bp FROM job_template_dimension_requirement
              WHERE career_catalog_bundle_id = template.career_catalog_bundle_id
                AND job_template_id = template.id AND dimension = 'certification'
          )
          AND NEW.language_required_score_bp = (
              SELECT required_score_bp FROM job_template_dimension_requirement
              WHERE career_catalog_bundle_id = template.career_catalog_bundle_id
                AND job_template_id = template.id AND dimension = 'language'
          )
          AND NEW.language_weight_bp = (
              SELECT weight_bp FROM job_template_dimension_requirement
              WHERE career_catalog_bundle_id = template.career_catalog_bundle_id
                AND job_template_id = template.id AND dimension = 'language'
          )
          AND NEW.training_required_score_bp = (
              SELECT required_score_bp FROM job_template_dimension_requirement
              WHERE career_catalog_bundle_id = template.career_catalog_bundle_id
                AND job_template_id = template.id AND dimension = 'training'
          )
          AND NEW.training_weight_bp = (
              SELECT weight_bp FROM job_template_dimension_requirement
              WHERE career_catalog_bundle_id = template.career_catalog_bundle_id
                AND job_template_id = template.id AND dimension = 'training'
          )
          AND NEW.experience_required_score_bp = (
              SELECT required_score_bp FROM job_template_dimension_requirement
              WHERE career_catalog_bundle_id = template.career_catalog_bundle_id
                AND job_template_id = template.id AND dimension = 'experience'
          )
          AND NEW.experience_weight_bp = (
              SELECT weight_bp FROM job_template_dimension_requirement
              WHERE career_catalog_bundle_id = template.career_catalog_bundle_id
                AND job_template_id = template.id AND dimension = 'experience'
          )
          AND NEW.project_required_score_bp = (
              SELECT required_score_bp FROM job_template_dimension_requirement
              WHERE career_catalog_bundle_id = template.career_catalog_bundle_id
                AND job_template_id = template.id AND dimension = 'project'
          )
          AND NEW.project_weight_bp = (
              SELECT weight_bp FROM job_template_dimension_requirement
              WHERE career_catalog_bundle_id = template.career_catalog_bundle_id
                AND job_template_id = template.id AND dimension = 'project'
          )
    ),
    NEW.posting_key,
    NULL
);

CREATE TRIGGER tr_job_posting_no_update
BEFORE UPDATE ON job_posting
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'job postings are immutable';

CREATE TRIGGER tr_job_posting_no_delete
BEFORE DELETE ON job_posting
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'job postings are immutable';

CREATE TABLE job_application (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED    NOT NULL,
    career_catalog_bundle_id            BIGINT UNSIGNED NOT NULL,
    recruitment_ruleset_id              BIGINT UNSIGNED NOT NULL,
    job_posting_id                      BIGINT UNSIGNED NOT NULL,
    application_ordinal                 TINYINT UNSIGNED NOT NULL DEFAULT 1,
    source_kind                         VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    source_invitation_id                BIGINT UNSIGNED     NULL,
    status                              VARCHAR(40) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    terminal_from_status                VARCHAR(40) CHARACTER SET ascii COLLATE ascii_bin NULL,
    submitted_game_day                  INT UNSIGNED    NOT NULL,
    resume_version_id                   BIGINT UNSIGNED     NULL,
    portfolio_version_id                BIGINT UNSIGNED     NULL,
    linkedin_profile_version_id         BIGINT UNSIGNED     NULL,
    artifact_completeness_bp            INT             NOT NULL,
    visible_education_score_bp          INT             NOT NULL,
    visible_certification_score_bp      INT             NOT NULL,
    visible_language_score_bp           INT             NOT NULL,
    visible_training_score_bp           INT             NOT NULL,
    visible_experience_score_bp         INT             NOT NULL,
    visible_project_score_bp            INT             NOT NULL,
    possessed_education_score_bp        INT                 NULL,
    possessed_certification_score_bp    INT                 NULL,
    possessed_language_score_bp         INT                 NULL,
    possessed_training_score_bp         INT                 NULL,
    possessed_experience_score_bp       INT                 NULL,
    possessed_project_score_bp          INT                 NULL,
    document_visible_fit_bp             INT                 NULL,
    document_platform_affinity_bp       INT                 NULL,
    document_score_bp                   INT                 NULL,
    document_probability_ppm            INT                 NULL,
    document_roll                       INT                 NULL,
    document_decided_game_day           INT UNSIGNED        NULL,
    confirmation_expires_exclusive_game_day INT UNSIGNED    NULL,
    interview_game_day                  INT UNSIGNED        NULL,
    possessed_fit_bp                    INT                 NULL,
    experience_project_fit_bp           INT                 NULL,
    profile_consistency_bp              INT                 NULL,
    interview_score_bp                  INT                 NULL,
    interview_probability_ppm           INT                 NULL,
    interview_roll                      INT                 NULL,
    interview_decided_game_day          INT UNSIGNED        NULL,
    terminal_game_day                   INT UNSIGNED        NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_job_application_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_job_application_save_run_bundle_id
        (save_id, run_revision, career_catalog_bundle_id, id),
    UNIQUE KEY uk_job_application_posting
        (save_id, run_revision, job_posting_id),
    UNIQUE KEY uk_job_application_source_invitation
        (save_id, run_revision, source_invitation_id),
    KEY ix_job_application_history
        (save_id, run_revision, submitted_game_day, id),
    KEY ix_job_application_open
        (save_id, run_revision, status, id),
    KEY ix_job_application_posting_owner
        (career_catalog_bundle_id, job_posting_id),
    KEY ix_job_application_resume
        (save_id, run_revision, career_catalog_bundle_id, resume_version_id),
    KEY ix_job_application_portfolio
        (save_id, run_revision, career_catalog_bundle_id, portfolio_version_id),
    KEY ix_job_application_linkedin
        (save_id, run_revision, career_catalog_bundle_id, linkedin_profile_version_id),
    CONSTRAINT fk_job_application_career_run
        FOREIGN KEY (save_id, run_revision, career_catalog_bundle_id)
        REFERENCES career_run (save_id, run_revision, career_catalog_bundle_id)
        ON DELETE CASCADE,
    CONSTRAINT fk_job_application_posting
        FOREIGN KEY (career_catalog_bundle_id, job_posting_id)
        REFERENCES job_posting (career_catalog_bundle_id, id),
    CONSTRAINT fk_job_application_compatibility
        FOREIGN KEY (career_catalog_bundle_id, recruitment_ruleset_id)
        REFERENCES career_recruitment_compatibility (
            career_catalog_bundle_id,
            recruitment_ruleset_id
        ),
    CONSTRAINT fk_job_application_resume
        FOREIGN KEY (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            resume_version_id
        ) REFERENCES profile_artifact_version (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            id
        ),
    CONSTRAINT fk_job_application_portfolio
        FOREIGN KEY (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            portfolio_version_id
        ) REFERENCES profile_artifact_version (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            id
        ),
    CONSTRAINT fk_job_application_linkedin
        FOREIGN KEY (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            linkedin_profile_version_id
        ) REFERENCES profile_artifact_version (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            id
        ),
    CONSTRAINT ck_job_application_ordinal CHECK (application_ordinal = 1),
    CONSTRAINT ck_job_application_source CHECK (
        (source_kind = 'direct' AND source_invitation_id IS NULL)
        OR (source_kind = 'invitation' AND source_invitation_id IS NOT NULL)
    ),
    CONSTRAINT ck_job_application_status CHECK (
        status IN (
            'submitted', 'documentRejected', 'interviewAwaitingConfirmation',
            'interviewConfirmed', 'interviewRejected', 'offered', 'accepted',
            'declined', 'expired', 'withdrawn', 'closed'
        )
    ),
    CONSTRAINT ck_job_application_terminal_from CHECK (
        (status NOT IN ('withdrawn', 'closed') AND terminal_from_status IS NULL)
        OR (
            status = 'withdrawn'
            AND terminal_from_status IN (
                'submitted', 'interviewAwaitingConfirmation', 'interviewConfirmed'
            )
        )
        OR (
            status = 'closed'
            AND terminal_from_status IN (
                'submitted', 'interviewAwaitingConfirmation',
                'interviewConfirmed', 'offered'
            )
        )
    ),
    CONSTRAINT ck_job_application_artifact_score CHECK (
        artifact_completeness_bp BETWEEN 0 AND 10000
        AND visible_education_score_bp BETWEEN 0 AND 10000
        AND visible_certification_score_bp BETWEEN 0 AND 10000
        AND visible_language_score_bp BETWEEN 0 AND 10000
        AND visible_training_score_bp BETWEEN 0 AND 10000
        AND visible_experience_score_bp BETWEEN 0 AND 10000
        AND visible_project_score_bp BETWEEN 0 AND 10000
    ),
    CONSTRAINT ck_job_application_possessed_scores CHECK (
        (
            possessed_education_score_bp IS NULL
            AND possessed_certification_score_bp IS NULL
            AND possessed_language_score_bp IS NULL
            AND possessed_training_score_bp IS NULL
            AND possessed_experience_score_bp IS NULL
            AND possessed_project_score_bp IS NULL
        )
        OR (
            possessed_education_score_bp BETWEEN 0 AND 10000
            AND possessed_certification_score_bp BETWEEN 0 AND 10000
            AND possessed_language_score_bp BETWEEN 0 AND 10000
            AND possessed_training_score_bp BETWEEN 0 AND 10000
            AND possessed_experience_score_bp BETWEEN 0 AND 10000
            AND possessed_project_score_bp BETWEEN 0 AND 10000
        )
    ),
    CONSTRAINT ck_job_application_document_result CHECK (
        (
            document_visible_fit_bp IS NULL
            AND document_platform_affinity_bp IS NULL
            AND document_score_bp IS NULL
            AND document_probability_ppm IS NULL
            AND document_roll IS NULL
            AND document_decided_game_day IS NULL
        )
        OR (
            document_visible_fit_bp BETWEEN 0 AND 10000
            AND document_platform_affinity_bp BETWEEN 0 AND 10000
            AND document_score_bp BETWEEN 0 AND 10000
            AND document_probability_ppm BETWEEN 0 AND 1000000
            AND document_roll BETWEEN 0 AND 999999
            AND document_decided_game_day >= submitted_game_day
        )
    ),
    CONSTRAINT ck_job_application_interview_result CHECK (
        (
            possessed_fit_bp IS NULL
            AND experience_project_fit_bp IS NULL
            AND profile_consistency_bp IS NULL
            AND interview_score_bp IS NULL
            AND interview_probability_ppm IS NULL
            AND interview_roll IS NULL
            AND interview_decided_game_day IS NULL
        )
        OR (
            possessed_fit_bp BETWEEN 0 AND 10000
            AND experience_project_fit_bp BETWEEN 0 AND 10000
            AND profile_consistency_bp BETWEEN 0 AND 10000
            AND interview_score_bp BETWEEN 0 AND 10000
            AND interview_probability_ppm BETWEEN 0 AND 1000000
            AND interview_roll BETWEEN 0 AND 999999
            AND interview_decided_game_day = interview_game_day
        )
    ),
    CONSTRAINT ck_job_application_state_shape CHECK (
        (
            status = 'submitted'
            AND source_kind = 'direct'
            AND document_decided_game_day IS NULL
            AND confirmation_expires_exclusive_game_day IS NULL
            AND interview_game_day IS NULL
            AND interview_decided_game_day IS NULL
            AND terminal_game_day IS NULL
        )
        OR (
            status = 'documentRejected'
            AND document_decided_game_day IS NOT NULL
            AND confirmation_expires_exclusive_game_day IS NULL
            AND interview_game_day IS NULL
            AND interview_decided_game_day IS NULL
            AND terminal_game_day IS NULL
        )
        OR (
            status = 'interviewAwaitingConfirmation'
            AND document_decided_game_day IS NOT NULL
            AND interview_game_day > document_decided_game_day
            AND confirmation_expires_exclusive_game_day = interview_game_day
            AND interview_decided_game_day IS NULL
            AND terminal_game_day IS NULL
        )
        OR (
            status = 'interviewConfirmed'
            AND document_decided_game_day IS NOT NULL
            AND interview_game_day > document_decided_game_day
            AND confirmation_expires_exclusive_game_day = interview_game_day
            AND interview_decided_game_day IS NULL
            AND terminal_game_day IS NULL
        )
        OR (
            status IN ('interviewRejected', 'offered', 'accepted', 'declined', 'expired')
            AND document_decided_game_day IS NOT NULL
            AND interview_game_day > document_decided_game_day
            AND confirmation_expires_exclusive_game_day = interview_game_day
            AND interview_decided_game_day IS NOT NULL
            AND terminal_game_day IS NULL
        )
        OR (
            status = 'withdrawn'
            AND interview_decided_game_day IS NULL
            AND terminal_game_day IS NOT NULL
            AND (
                (
                    terminal_from_status = 'submitted'
                    AND document_decided_game_day IS NULL
                    AND confirmation_expires_exclusive_game_day IS NULL
                    AND interview_game_day IS NULL
                )
                OR (
                    terminal_from_status IN (
                        'interviewAwaitingConfirmation', 'interviewConfirmed'
                    )
                    AND document_decided_game_day IS NOT NULL
                    AND interview_game_day > document_decided_game_day
                    AND confirmation_expires_exclusive_game_day = interview_game_day
                )
            )
        )
        OR (
            status = 'closed'
            AND terminal_game_day IS NOT NULL
            AND (
                (terminal_from_status = 'submitted' AND document_decided_game_day IS NULL)
                OR (
                    terminal_from_status = 'interviewAwaitingConfirmation'
                    AND document_decided_game_day IS NOT NULL
                    AND confirmation_expires_exclusive_game_day = interview_game_day
                    AND interview_game_day IS NOT NULL
                    AND interview_decided_game_day IS NULL
                )
                OR (
                    terminal_from_status = 'interviewConfirmed'
                    AND document_decided_game_day IS NOT NULL
                    AND confirmation_expires_exclusive_game_day = interview_game_day
                    AND interview_game_day IS NOT NULL
                    AND interview_decided_game_day IS NULL
                )
                OR (
                    terminal_from_status = 'offered'
                    AND interview_decided_game_day IS NOT NULL
                )
            )
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE job_invitation (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED    NOT NULL,
    career_catalog_bundle_id            BIGINT UNSIGNED NOT NULL,
    recruitment_ruleset_id              BIGINT UNSIGNED NOT NULL,
    job_posting_id                      BIGINT UNSIGNED NOT NULL,
    platform_catalog_id                 BIGINT UNSIGNED NOT NULL,
    profile_artifact_version_id         BIGINT UNSIGNED NOT NULL,
    status                              VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    invitation_game_day                 INT UNSIGNED    NOT NULL,
    expires_exclusive_game_day          INT UNSIGNED    NOT NULL,
    artifact_completeness_bp            INT             NOT NULL,
    visible_education_score_bp          INT             NOT NULL,
    visible_certification_score_bp      INT             NOT NULL,
    visible_language_score_bp           INT             NOT NULL,
    visible_training_score_bp           INT             NOT NULL,
    visible_experience_score_bp         INT             NOT NULL,
    visible_project_score_bp            INT             NOT NULL,
    invitation_score_bp                 INT             NOT NULL,
    invitation_probability_ppm          INT             NOT NULL,
    invitation_roll                     INT             NOT NULL,
    accepted_application_id             BIGINT UNSIGNED     NULL,
    decided_game_day                    INT UNSIGNED        NULL,
    occurrence                          TINYINT UNSIGNED NOT NULL DEFAULT 1,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_job_invitation_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_job_invitation_save_run_bundle_id
        (save_id, run_revision, career_catalog_bundle_id, id),
    UNIQUE KEY uk_job_invitation_posting
        (save_id, run_revision, job_posting_id),
    UNIQUE KEY uk_job_invitation_accepted_application
        (save_id, run_revision, accepted_application_id),
    UNIQUE KEY uk_job_invitation_platform_day
        (save_id, run_revision, platform_catalog_id, invitation_game_day),
    KEY ix_job_invitation_open (save_id, run_revision, status, id),
    KEY ix_job_invitation_history
        (save_id, run_revision, invitation_game_day, id),
    CONSTRAINT fk_job_invitation_career_run
        FOREIGN KEY (save_id, run_revision, career_catalog_bundle_id)
        REFERENCES career_run (save_id, run_revision, career_catalog_bundle_id)
        ON DELETE CASCADE,
    CONSTRAINT fk_job_invitation_posting
        FOREIGN KEY (career_catalog_bundle_id, job_posting_id)
        REFERENCES job_posting (career_catalog_bundle_id, id),
    CONSTRAINT fk_job_invitation_platform
        FOREIGN KEY (career_catalog_bundle_id, platform_catalog_id)
        REFERENCES platform_catalog (career_catalog_bundle_id, id),
    CONSTRAINT fk_job_invitation_compatibility
        FOREIGN KEY (career_catalog_bundle_id, recruitment_ruleset_id)
        REFERENCES career_recruitment_compatibility (
            career_catalog_bundle_id,
            recruitment_ruleset_id
        ),
    CONSTRAINT fk_job_invitation_artifact
        FOREIGN KEY (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            profile_artifact_version_id
        ) REFERENCES profile_artifact_version (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            id
        ),
    CONSTRAINT fk_job_invitation_accepted_application
        FOREIGN KEY (save_id, run_revision, accepted_application_id)
        REFERENCES job_application (save_id, run_revision, id),
    CONSTRAINT ck_job_invitation_status CHECK (
        status IN ('open', 'accepted', 'declined', 'expired', 'closed')
    ),
    CONSTRAINT ck_job_invitation_timing CHECK (
        expires_exclusive_game_day > invitation_game_day
    ),
    CONSTRAINT ck_job_invitation_scores CHECK (
        artifact_completeness_bp BETWEEN 0 AND 10000
        AND visible_education_score_bp BETWEEN 0 AND 10000
        AND visible_certification_score_bp BETWEEN 0 AND 10000
        AND visible_language_score_bp BETWEEN 0 AND 10000
        AND visible_training_score_bp BETWEEN 0 AND 10000
        AND visible_experience_score_bp BETWEEN 0 AND 10000
        AND visible_project_score_bp BETWEEN 0 AND 10000
        AND invitation_score_bp BETWEEN 0 AND 10000
        AND invitation_probability_ppm BETWEEN 0 AND 1000000
        AND invitation_roll BETWEEN 0 AND 999999
    ),
    CONSTRAINT ck_job_invitation_occurrence CHECK (occurrence = 1),
    CONSTRAINT ck_job_invitation_state_shape CHECK (
        (
            status = 'open'
            AND accepted_application_id IS NULL
            AND decided_game_day IS NULL
        )
        OR (
            status = 'accepted'
            AND accepted_application_id IS NOT NULL
            AND decided_game_day IS NOT NULL
            AND decided_game_day < expires_exclusive_game_day
        )
        OR (
            status IN ('declined', 'closed')
            AND accepted_application_id IS NULL
            AND decided_game_day IS NOT NULL
        )
        OR (
            status = 'expired'
            AND accepted_application_id IS NULL
            AND decided_game_day >= expires_exclusive_game_day
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

ALTER TABLE job_application
    ADD CONSTRAINT fk_job_application_source_invitation
        FOREIGN KEY (save_id, run_revision, source_invitation_id)
        REFERENCES job_invitation (save_id, run_revision, id);

CREATE TABLE job_offer (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED    NOT NULL,
    career_catalog_bundle_id            BIGINT UNSIGNED NOT NULL,
    recruitment_ruleset_id              BIGINT UNSIGNED NOT NULL,
    job_application_id                  BIGINT UNSIGNED NOT NULL,
    job_posting_id                      BIGINT UNSIGNED NOT NULL,
    career_industry_id                  BIGINT UNSIGNED NOT NULL,
    career_job_family_id                BIGINT UNSIGNED NOT NULL,
    virtual_employer_id                 BIGINT UNSIGNED NOT NULL,
    status                              VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    annual_salary_krw                   BIGINT          NOT NULL,
    region                              VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    employment_type                     VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    payday_day_of_month                 TINYINT UNSIGNED NOT NULL,
    offered_game_day                    INT UNSIGNED    NOT NULL,
    start_game_day                      INT UNSIGNED    NOT NULL,
    expires_exclusive_game_day          INT UNSIGNED    NOT NULL,
    first_pay_reward_krw                BIGINT          NOT NULL DEFAULT 0,
    employment_contract_id              BIGINT UNSIGNED     NULL,
    decided_game_day                    INT UNSIGNED        NULL,
    occurrence                          TINYINT UNSIGNED NOT NULL DEFAULT 1,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_job_offer_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_job_offer_save_run_bundle_id
        (save_id, run_revision, career_catalog_bundle_id, id),
    UNIQUE KEY uk_job_offer_application
        (save_id, run_revision, job_application_id),
    UNIQUE KEY uk_job_offer_contract
        (save_id, run_revision, employment_contract_id),
    UNIQUE KEY uk_job_offer_occurrence
        (save_id, run_revision, job_application_id, occurrence),
    KEY ix_job_offer_open (save_id, run_revision, status, id),
    KEY ix_job_offer_posting (career_catalog_bundle_id, job_posting_id),
    CONSTRAINT fk_job_offer_career_run
        FOREIGN KEY (save_id, run_revision, career_catalog_bundle_id)
        REFERENCES career_run (save_id, run_revision, career_catalog_bundle_id)
        ON DELETE CASCADE,
    CONSTRAINT fk_job_offer_application
        FOREIGN KEY (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            job_application_id
        ) REFERENCES job_application (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            id
        ),
    CONSTRAINT fk_job_offer_posting
        FOREIGN KEY (career_catalog_bundle_id, job_posting_id)
        REFERENCES job_posting (career_catalog_bundle_id, id),
    CONSTRAINT fk_job_offer_compatibility
        FOREIGN KEY (career_catalog_bundle_id, recruitment_ruleset_id)
        REFERENCES career_recruitment_compatibility (
            career_catalog_bundle_id,
            recruitment_ruleset_id
        ),
    CONSTRAINT fk_job_offer_industry
        FOREIGN KEY (career_catalog_bundle_id, career_industry_id)
        REFERENCES career_industry (career_catalog_bundle_id, id),
    CONSTRAINT fk_job_offer_job_family
        FOREIGN KEY (career_catalog_bundle_id, career_job_family_id)
        REFERENCES career_job_family (career_catalog_bundle_id, id),
    CONSTRAINT fk_job_offer_employer
        FOREIGN KEY (career_catalog_bundle_id, virtual_employer_id)
        REFERENCES virtual_employer (career_catalog_bundle_id, id),
    CONSTRAINT ck_job_offer_status CHECK (
        status IN ('pending', 'accepted', 'declined', 'expired', 'closed')
    ),
    CONSTRAINT ck_job_offer_salary_api_bound CHECK (
        annual_salary_krw BETWEEN 1 AND 9007199254740991
        AND first_pay_reward_krw BETWEEN 0 AND 9007199254740991
    ),
    CONSTRAINT ck_job_offer_terms CHECK (
        region IN ('capitalArea', 'metropolitan', 'smallCity', 'rural')
        AND employment_type = 'regular'
        AND payday_day_of_month BETWEEN 1 AND 31
        AND start_game_day >= offered_game_day
        AND expires_exclusive_game_day > offered_game_day
        AND occurrence = 1
    ),
    CONSTRAINT ck_job_offer_state_shape CHECK (
        (
            status = 'pending'
            AND employment_contract_id IS NULL
            AND decided_game_day IS NULL
        )
        OR (
            status = 'accepted'
            AND employment_contract_id IS NOT NULL
            AND decided_game_day IS NOT NULL
            AND decided_game_day < expires_exclusive_game_day
        )
        OR (
            status IN ('declined', 'closed')
            AND employment_contract_id IS NULL
            AND decided_game_day IS NOT NULL
        )
        OR (
            status = 'expired'
            AND employment_contract_id IS NULL
            AND decided_game_day >= expires_exclusive_game_day
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE employment_contract (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED    NOT NULL,
    career_catalog_bundle_id            BIGINT UNSIGNED NOT NULL,
    recruitment_ruleset_id              BIGINT UNSIGNED NOT NULL,
    job_offer_id                        BIGINT UNSIGNED NOT NULL,
    job_application_id                  BIGINT UNSIGNED NOT NULL,
    job_posting_id                      BIGINT UNSIGNED NOT NULL,
    career_industry_id                  BIGINT UNSIGNED NOT NULL,
    career_job_family_id                BIGINT UNSIGNED NOT NULL,
    virtual_employer_id                 BIGINT UNSIGNED NOT NULL,
    status                              VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    active_contract_slot                TINYINT UNSIGNED GENERATED ALWAYS AS (
        CASE WHEN status IN ('pendingStart', 'active') THEN 1 ELSE NULL END
    ) STORED,
    annual_salary_krw                   BIGINT          NOT NULL,
    region                              VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    employment_type                     VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    payday_day_of_month                 TINYINT UNSIGNED NOT NULL,
    start_game_day                      INT UNSIGNED    NOT NULL,
    end_game_day                        INT UNSIGNED        NULL,
    credited_experience_days            INT UNSIGNED    NOT NULL DEFAULT 0,
    last_credited_game_day              INT UNSIGNED        NULL,
    first_pay_reward_krw                BIGINT          NOT NULL DEFAULT 0,
    created_game_day                    INT UNSIGNED    NOT NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_employment_contract_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_employment_contract_save_run_bundle_id
        (save_id, run_revision, career_catalog_bundle_id, id),
    UNIQUE KEY uk_employment_contract_offer (save_id, run_revision, job_offer_id),
    UNIQUE KEY uk_employment_contract_application
        (save_id, run_revision, job_application_id),
    UNIQUE KEY uk_employment_contract_active
        (save_id, run_revision, active_contract_slot),
    KEY ix_employment_contract_history
        (save_id, run_revision, start_game_day, id),
    KEY ix_employment_contract_posting
        (career_catalog_bundle_id, job_posting_id),
    CONSTRAINT fk_employment_contract_career_run
        FOREIGN KEY (save_id, run_revision, career_catalog_bundle_id)
        REFERENCES career_run (save_id, run_revision, career_catalog_bundle_id)
        ON DELETE CASCADE,
    CONSTRAINT fk_employment_contract_offer
        FOREIGN KEY (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            job_offer_id
        ) REFERENCES job_offer (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            id
        ),
    CONSTRAINT fk_employment_contract_application
        FOREIGN KEY (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            job_application_id
        ) REFERENCES job_application (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            id
        ),
    CONSTRAINT fk_employment_contract_posting
        FOREIGN KEY (career_catalog_bundle_id, job_posting_id)
        REFERENCES job_posting (career_catalog_bundle_id, id),
    CONSTRAINT fk_employment_contract_compatibility
        FOREIGN KEY (career_catalog_bundle_id, recruitment_ruleset_id)
        REFERENCES career_recruitment_compatibility (
            career_catalog_bundle_id,
            recruitment_ruleset_id
        ),
    CONSTRAINT fk_employment_contract_industry
        FOREIGN KEY (career_catalog_bundle_id, career_industry_id)
        REFERENCES career_industry (career_catalog_bundle_id, id),
    CONSTRAINT fk_employment_contract_job_family
        FOREIGN KEY (career_catalog_bundle_id, career_job_family_id)
        REFERENCES career_job_family (career_catalog_bundle_id, id),
    CONSTRAINT fk_employment_contract_employer
        FOREIGN KEY (career_catalog_bundle_id, virtual_employer_id)
        REFERENCES virtual_employer (career_catalog_bundle_id, id),
    CONSTRAINT ck_employment_contract_status CHECK (
        status IN ('pendingStart', 'active', 'ended')
    ),
    CONSTRAINT ck_employment_contract_salary_api_bound CHECK (
        annual_salary_krw BETWEEN 1 AND 9007199254740991
        AND first_pay_reward_krw BETWEEN 0 AND 9007199254740991
        AND credited_experience_days <= 9007199254740991
    ),
    CONSTRAINT ck_employment_contract_terms CHECK (
        region IN ('capitalArea', 'metropolitan', 'smallCity', 'rural')
        AND employment_type = 'regular'
        AND payday_day_of_month BETWEEN 1 AND 31
        AND created_game_day <= start_game_day
    ),
    CONSTRAINT ck_employment_contract_state_shape CHECK (
        (
            status = 'pendingStart'
            AND end_game_day IS NULL
            AND credited_experience_days = 0
            AND last_credited_game_day IS NULL
        )
        OR (
            status = 'active'
            AND end_game_day IS NULL
            AND credited_experience_days > 0
            AND last_credited_game_day >= start_game_day
            AND credited_experience_days
                = last_credited_game_day - start_game_day + 1
        )
        OR (
            status = 'ended'
            AND end_game_day IS NOT NULL
            AND (
                (
                    credited_experience_days = 0
                    AND last_credited_game_day IS NULL
                    AND end_game_day = start_game_day
                )
                OR (
                    credited_experience_days > 0
                    AND last_credited_game_day >= start_game_day
                    AND credited_experience_days
                        = last_credited_game_day - start_game_day + 1
                    AND end_game_day = last_credited_game_day + 1
                )
            )
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

ALTER TABLE job_offer
    ADD CONSTRAINT fk_job_offer_employment_contract
        FOREIGN KEY (save_id, run_revision, employment_contract_id)
        REFERENCES employment_contract (save_id, run_revision, id);

ALTER TABLE spec_evidence
    ADD KEY ix_spec_evidence_source_employment
        (save_id, run_revision, source_employment_contract_id),
    ADD CONSTRAINT fk_spec_evidence_source_employment
        FOREIGN KEY (save_id, run_revision, source_employment_contract_id)
        REFERENCES employment_contract (save_id, run_revision, id);

CREATE TABLE career_scheduled_action (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED    NOT NULL,
    career_catalog_bundle_id            BIGINT UNSIGNED NOT NULL,
    recruitment_ruleset_id              BIGINT UNSIGNED NOT NULL,
    action_kind                         VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    payload_version                     TINYINT UNSIGNED NOT NULL,
    phase_rank                          TINYINT UNSIGNED NOT NULL,
    due_game_day                        INT UNSIGNED    NOT NULL,
    status                              VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    source_kind                         VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    source_id                           BIGINT UNSIGNED NOT NULL,
    occurrence                          BIGINT UNSIGNED NOT NULL,
    employment_contract_id              BIGINT UNSIGNED     NULL,
    job_application_id                  BIGINT UNSIGNED     NULL,
    platform_catalog_id                 BIGINT UNSIGNED     NULL,
    invitation_generation_game_day      INT UNSIGNED        NULL,
    completed_game_day                  INT UNSIGNED        NULL,
    cancelled_game_day                  INT UNSIGNED        NULL,
    created_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                          DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_career_scheduled_action_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_career_scheduled_action_source_occurrence
        (save_id, run_revision, source_kind, source_id, occurrence),
    KEY ix_career_scheduled_action_due
        (save_id, run_revision, status, phase_rank, due_game_day, id),
    KEY ix_career_scheduled_action_application
        (save_id, run_revision, career_catalog_bundle_id, job_application_id),
    KEY ix_career_scheduled_action_contract
        (save_id, run_revision, career_catalog_bundle_id, employment_contract_id),
    CONSTRAINT fk_career_scheduled_action_career_run
        FOREIGN KEY (save_id, run_revision, career_catalog_bundle_id)
        REFERENCES career_run (save_id, run_revision, career_catalog_bundle_id)
        ON DELETE CASCADE,
    CONSTRAINT fk_career_scheduled_action_compatibility
        FOREIGN KEY (career_catalog_bundle_id, recruitment_ruleset_id)
        REFERENCES career_recruitment_compatibility (
            career_catalog_bundle_id,
            recruitment_ruleset_id
        ),
    CONSTRAINT fk_career_scheduled_action_contract
        FOREIGN KEY (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            employment_contract_id
        ) REFERENCES employment_contract (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            id
        ),
    CONSTRAINT fk_career_scheduled_action_application
        FOREIGN KEY (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            job_application_id
        ) REFERENCES job_application (
            save_id,
            run_revision,
            career_catalog_bundle_id,
            id
        ),
    CONSTRAINT fk_career_scheduled_action_platform
        FOREIGN KEY (career_catalog_bundle_id, platform_catalog_id)
        REFERENCES platform_catalog (career_catalog_bundle_id, id),
    CONSTRAINT ck_career_scheduled_action_kind CHECK (
        action_kind IN (
            'employmentStart', 'documentReview', 'confirmationExpiry',
            'interviewDecision', 'offerExpiry', 'invitationGeneration'
        )
    ),
    CONSTRAINT ck_career_scheduled_action_status CHECK (
        status IN ('pending', 'completed', 'cancelled')
    ),
    CONSTRAINT ck_career_scheduled_action_payload CHECK (
        payload_version = 1
        AND occurrence BETWEEN 1 AND 9007199254740991
        AND (
            (
                action_kind = 'employmentStart'
                AND phase_rank = 10
                AND source_kind = 'employmentStart'
                AND source_id = employment_contract_id
                AND occurrence = 1
                AND employment_contract_id IS NOT NULL
                AND job_application_id IS NULL
                AND platform_catalog_id IS NULL
                AND invitation_generation_game_day IS NULL
            )
            OR (
                action_kind = 'documentReview'
                AND phase_rank = 20
                AND source_kind = 'documentReview'
                AND source_id = job_application_id
                AND occurrence = 1
                AND employment_contract_id IS NULL
                AND job_application_id IS NOT NULL
                AND platform_catalog_id IS NULL
                AND invitation_generation_game_day IS NULL
            )
            OR (
                action_kind = 'confirmationExpiry'
                AND phase_rank = 30
                AND source_kind = 'confirmationExpiry'
                AND source_id = job_application_id
                AND occurrence = 1
                AND employment_contract_id IS NULL
                AND job_application_id IS NOT NULL
                AND platform_catalog_id IS NULL
                AND invitation_generation_game_day IS NULL
            )
            OR (
                action_kind = 'interviewDecision'
                AND phase_rank = 40
                AND source_kind = 'interviewDecision'
                AND source_id = job_application_id
                AND occurrence = 1
                AND employment_contract_id IS NULL
                AND job_application_id IS NOT NULL
                AND platform_catalog_id IS NULL
                AND invitation_generation_game_day IS NULL
            )
            OR (
                action_kind = 'offerExpiry'
                AND phase_rank = 50
                AND source_kind = 'offerExpiry'
                AND source_id = job_application_id
                AND occurrence = 1
                AND employment_contract_id IS NULL
                AND job_application_id IS NOT NULL
                AND platform_catalog_id IS NULL
                AND invitation_generation_game_day IS NULL
            )
            OR (
                action_kind = 'invitationGeneration'
                AND phase_rank = 60
                AND source_kind = 'invitationGeneration'
                AND source_id = platform_catalog_id
                AND occurrence = invitation_generation_game_day
                AND employment_contract_id IS NULL
                AND job_application_id IS NULL
                AND platform_catalog_id IS NOT NULL
                AND invitation_generation_game_day = due_game_day
            )
        )
    ),
    CONSTRAINT ck_career_scheduled_action_state_shape CHECK (
        (
            status = 'pending'
            AND completed_game_day IS NULL
            AND cancelled_game_day IS NULL
        )
        OR (
            status = 'completed'
            AND completed_game_day IS NOT NULL
            AND completed_game_day >= due_game_day
            AND cancelled_game_day IS NULL
        )
        OR (
            status = 'cancelled'
            AND completed_game_day IS NULL
            AND cancelled_game_day IS NOT NULL
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

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
        INNER JOIN recruitment_ruleset AS ruleset
            ON ruleset.id = posting.recruitment_ruleset_id
           AND ruleset.published_at IS NOT NULL
        WHERE save.id = NEW.save_id
          AND save.run_revision = NEW.run_revision
          AND save.game_day = NEW.submitted_game_day
          AND save.game_day >= posting.posted_game_day
          AND save.game_day < posting.closes_exclusive_game_day
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

CREATE TRIGGER tr_job_application_transition_only
BEFORE UPDATE ON job_application
FOR EACH ROW
SET NEW.id = IF(
    NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.career_catalog_bundle_id = OLD.career_catalog_bundle_id
        AND NEW.recruitment_ruleset_id = OLD.recruitment_ruleset_id
        AND NEW.job_posting_id = OLD.job_posting_id
        AND NEW.application_ordinal = OLD.application_ordinal
        AND BINARY NEW.source_kind = BINARY OLD.source_kind
        AND NEW.source_invitation_id <=> OLD.source_invitation_id
        AND NEW.submitted_game_day = OLD.submitted_game_day
        AND NEW.resume_version_id <=> OLD.resume_version_id
        AND NEW.portfolio_version_id <=> OLD.portfolio_version_id
        AND NEW.linkedin_profile_version_id <=> OLD.linkedin_profile_version_id
        AND NEW.artifact_completeness_bp = OLD.artifact_completeness_bp
        AND NEW.visible_education_score_bp = OLD.visible_education_score_bp
        AND NEW.visible_certification_score_bp = OLD.visible_certification_score_bp
        AND NEW.visible_language_score_bp = OLD.visible_language_score_bp
        AND NEW.visible_training_score_bp = OLD.visible_training_score_bp
        AND NEW.visible_experience_score_bp = OLD.visible_experience_score_bp
        AND NEW.visible_project_score_bp = OLD.visible_project_score_bp
        AND NEW.created_at = OLD.created_at
        AND (
            OLD.document_decided_game_day IS NULL
            OR (
                NEW.document_visible_fit_bp <=> OLD.document_visible_fit_bp
                AND NEW.document_platform_affinity_bp
                    <=> OLD.document_platform_affinity_bp
                AND NEW.document_score_bp <=> OLD.document_score_bp
                AND NEW.document_probability_ppm <=> OLD.document_probability_ppm
                AND NEW.document_roll <=> OLD.document_roll
                AND NEW.document_decided_game_day <=> OLD.document_decided_game_day
                AND NEW.confirmation_expires_exclusive_game_day
                    <=> OLD.confirmation_expires_exclusive_game_day
                AND NEW.interview_game_day <=> OLD.interview_game_day
            )
        )
        AND (
            OLD.interview_decided_game_day IS NULL
            OR (
                NEW.possessed_education_score_bp
                    <=> OLD.possessed_education_score_bp
                AND NEW.possessed_certification_score_bp
                    <=> OLD.possessed_certification_score_bp
                AND NEW.possessed_language_score_bp
                    <=> OLD.possessed_language_score_bp
                AND NEW.possessed_training_score_bp
                    <=> OLD.possessed_training_score_bp
                AND NEW.possessed_experience_score_bp
                    <=> OLD.possessed_experience_score_bp
                AND NEW.possessed_project_score_bp <=> OLD.possessed_project_score_bp
                AND NEW.possessed_fit_bp <=> OLD.possessed_fit_bp
                AND NEW.experience_project_fit_bp <=> OLD.experience_project_fit_bp
                AND NEW.profile_consistency_bp <=> OLD.profile_consistency_bp
                AND NEW.interview_score_bp <=> OLD.interview_score_bp
                AND NEW.interview_probability_ppm <=> OLD.interview_probability_ppm
                AND NEW.interview_roll <=> OLD.interview_roll
                AND NEW.interview_decided_game_day <=> OLD.interview_decided_game_day
            )
        )
        AND EXISTS (
            SELECT 1 FROM save
            WHERE id = OLD.save_id AND run_revision = OLD.run_revision
        )
        AND (
            (
                OLD.status = 'submitted'
                AND NEW.status IN ('documentRejected', 'interviewAwaitingConfirmation')
                AND EXISTS (
                    SELECT 1
                    FROM career_scheduled_action AS action
                    WHERE action.save_id = OLD.save_id
                      AND action.run_revision = OLD.run_revision
                      AND action.action_kind = 'documentReview'
                      AND action.job_application_id = OLD.id
                      AND action.status = 'pending'
                      AND action.due_game_day = NEW.document_decided_game_day
                )
                AND NEW.document_decided_game_day = OLD.submitted_game_day + (
                    SELECT posting.document_review_days
                    FROM job_posting AS posting
                    WHERE posting.id = OLD.job_posting_id
                )
                AND OLD.document_decided_game_day IS NULL
                AND OLD.possessed_education_score_bp IS NULL
                AND NEW.possessed_education_score_bp IS NULL
                AND (
                    NEW.status = 'documentRejected'
                    OR (
                        NEW.interview_game_day = NEW.document_decided_game_day + (
                            SELECT posting.interview_delay_days
                            FROM job_posting AS posting
                            WHERE posting.id = OLD.job_posting_id
                        )
                        AND NEW.confirmation_expires_exclusive_game_day
                            = NEW.interview_game_day
                    )
                )
            )
            OR (
                OLD.status = 'submitted'
                AND NEW.status = 'withdrawn'
                AND NEW.terminal_from_status = 'submitted'
                AND NEW.terminal_game_day = (
                    SELECT game_day FROM save WHERE id = OLD.save_id
                )
                AND NEW.document_decided_game_day IS NULL
                AND NEW.possessed_education_score_bp IS NULL
            )
            OR (
                OLD.status = 'interviewAwaitingConfirmation'
                AND NEW.status = 'interviewConfirmed'
                AND NEW.terminal_from_status IS NULL
                AND NEW.terminal_game_day IS NULL
                AND NEW.document_visible_fit_bp <=> OLD.document_visible_fit_bp
                AND NEW.document_platform_affinity_bp
                    <=> OLD.document_platform_affinity_bp
                AND NEW.document_score_bp <=> OLD.document_score_bp
                AND NEW.document_probability_ppm <=> OLD.document_probability_ppm
                AND NEW.document_roll <=> OLD.document_roll
                AND NEW.document_decided_game_day <=> OLD.document_decided_game_day
                AND NEW.confirmation_expires_exclusive_game_day
                    <=> OLD.confirmation_expires_exclusive_game_day
                AND NEW.interview_game_day <=> OLD.interview_game_day
                AND NEW.possessed_education_score_bp IS NULL
                AND (
                    SELECT game_day FROM save WHERE id = OLD.save_id
                ) < OLD.confirmation_expires_exclusive_game_day
            )
            OR (
                OLD.status IN (
                    'interviewAwaitingConfirmation', 'interviewConfirmed'
                )
                AND NEW.status = 'withdrawn'
                AND NEW.terminal_from_status = OLD.status
                AND (
                    NEW.terminal_game_day = (
                        SELECT game_day FROM save WHERE id = OLD.save_id
                    )
                    OR (
                        OLD.status = 'interviewAwaitingConfirmation'
                        AND EXISTS (
                            SELECT 1
                            FROM career_scheduled_action AS action
                            WHERE action.save_id = OLD.save_id
                              AND action.run_revision = OLD.run_revision
                              AND action.action_kind = 'confirmationExpiry'
                              AND action.job_application_id = OLD.id
                              AND action.status = 'pending'
                              AND action.due_game_day = NEW.terminal_game_day
                        )
                    )
                )
                AND NEW.document_visible_fit_bp <=> OLD.document_visible_fit_bp
                AND NEW.document_platform_affinity_bp
                    <=> OLD.document_platform_affinity_bp
                AND NEW.document_score_bp <=> OLD.document_score_bp
                AND NEW.document_probability_ppm <=> OLD.document_probability_ppm
                AND NEW.document_roll <=> OLD.document_roll
                AND NEW.document_decided_game_day <=> OLD.document_decided_game_day
                AND NEW.confirmation_expires_exclusive_game_day
                    <=> OLD.confirmation_expires_exclusive_game_day
                AND NEW.interview_game_day <=> OLD.interview_game_day
                AND NEW.possessed_education_score_bp IS NULL
            )
            OR (
                OLD.status = 'interviewConfirmed'
                AND NEW.status IN ('interviewRejected', 'offered')
                AND NEW.document_visible_fit_bp <=> OLD.document_visible_fit_bp
                AND NEW.document_platform_affinity_bp
                    <=> OLD.document_platform_affinity_bp
                AND NEW.document_score_bp <=> OLD.document_score_bp
                AND NEW.document_probability_ppm <=> OLD.document_probability_ppm
                AND NEW.document_roll <=> OLD.document_roll
                AND NEW.document_decided_game_day <=> OLD.document_decided_game_day
                AND NEW.confirmation_expires_exclusive_game_day
                    <=> OLD.confirmation_expires_exclusive_game_day
                AND NEW.interview_game_day <=> OLD.interview_game_day
                AND NEW.interview_decided_game_day = OLD.interview_game_day
                AND EXISTS (
                    SELECT 1
                    FROM career_scheduled_action AS action
                    WHERE action.save_id = OLD.save_id
                      AND action.run_revision = OLD.run_revision
                      AND action.action_kind = 'interviewDecision'
                      AND action.job_application_id = OLD.id
                      AND action.status = 'pending'
                      AND action.due_game_day = NEW.interview_decided_game_day
                )
                AND (
                    NEW.status = 'interviewRejected'
                    OR EXISTS (
                        SELECT 1 FROM job_offer AS offer
                        WHERE offer.save_id = OLD.save_id
                          AND offer.run_revision = OLD.run_revision
                          AND offer.job_application_id = OLD.id
                          AND offer.status = 'pending'
                    )
                )
            )
            OR (
                OLD.status = 'offered'
                AND NEW.status IN ('accepted', 'declined', 'expired')
                AND NEW.possessed_education_score_bp
                    <=> OLD.possessed_education_score_bp
                AND NEW.possessed_certification_score_bp
                    <=> OLD.possessed_certification_score_bp
                AND NEW.possessed_language_score_bp
                    <=> OLD.possessed_language_score_bp
                AND NEW.possessed_training_score_bp
                    <=> OLD.possessed_training_score_bp
                AND NEW.possessed_experience_score_bp
                    <=> OLD.possessed_experience_score_bp
                AND NEW.possessed_project_score_bp <=> OLD.possessed_project_score_bp
                AND NEW.possessed_fit_bp <=> OLD.possessed_fit_bp
                AND NEW.experience_project_fit_bp <=> OLD.experience_project_fit_bp
                AND NEW.profile_consistency_bp <=> OLD.profile_consistency_bp
                AND NEW.interview_score_bp <=> OLD.interview_score_bp
                AND NEW.interview_probability_ppm <=> OLD.interview_probability_ppm
                AND NEW.interview_roll <=> OLD.interview_roll
                AND NEW.interview_decided_game_day <=> OLD.interview_decided_game_day
                AND EXISTS (
                    SELECT 1 FROM job_offer AS offer
                    WHERE offer.save_id = OLD.save_id
                      AND offer.run_revision = OLD.run_revision
                      AND offer.job_application_id = OLD.id
                      AND BINARY offer.status = BINARY NEW.status
                )
            )
            OR (
                OLD.status IN (
                    'submitted', 'interviewAwaitingConfirmation',
                    'interviewConfirmed', 'offered'
                )
                AND NEW.status = 'closed'
                AND NEW.terminal_from_status = OLD.status
                AND NEW.terminal_game_day = (
                    SELECT game_day FROM save WHERE id = OLD.save_id
                )
                AND NEW.document_visible_fit_bp <=> OLD.document_visible_fit_bp
                AND NEW.document_platform_affinity_bp
                    <=> OLD.document_platform_affinity_bp
                AND NEW.document_score_bp <=> OLD.document_score_bp
                AND NEW.document_probability_ppm <=> OLD.document_probability_ppm
                AND NEW.document_roll <=> OLD.document_roll
                AND NEW.document_decided_game_day <=> OLD.document_decided_game_day
                AND NEW.confirmation_expires_exclusive_game_day
                    <=> OLD.confirmation_expires_exclusive_game_day
                AND NEW.interview_game_day <=> OLD.interview_game_day
                AND NEW.possessed_education_score_bp
                    <=> OLD.possessed_education_score_bp
                AND NEW.possessed_certification_score_bp
                    <=> OLD.possessed_certification_score_bp
                AND NEW.possessed_language_score_bp
                    <=> OLD.possessed_language_score_bp
                AND NEW.possessed_training_score_bp
                    <=> OLD.possessed_training_score_bp
                AND NEW.possessed_experience_score_bp
                    <=> OLD.possessed_experience_score_bp
                AND NEW.possessed_project_score_bp <=> OLD.possessed_project_score_bp
                AND NEW.possessed_fit_bp <=> OLD.possessed_fit_bp
                AND NEW.experience_project_fit_bp <=> OLD.experience_project_fit_bp
                AND NEW.profile_consistency_bp <=> OLD.profile_consistency_bp
                AND NEW.interview_score_bp <=> OLD.interview_score_bp
                AND NEW.interview_probability_ppm <=> OLD.interview_probability_ppm
                AND NEW.interview_roll <=> OLD.interview_roll
                AND NEW.interview_decided_game_day <=> OLD.interview_decided_game_day
            )
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_job_application_no_delete
BEFORE DELETE ON job_application
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'job applications cannot be deleted';

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
            WHERE save.id = NEW.save_id
              AND save.run_revision = NEW.run_revision
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

CREATE TRIGGER tr_job_invitation_transition_only
BEFORE UPDATE ON job_invitation
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'open'
        AND NEW.status IN ('accepted', 'declined', 'expired', 'closed')
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.career_catalog_bundle_id = OLD.career_catalog_bundle_id
        AND NEW.recruitment_ruleset_id = OLD.recruitment_ruleset_id
        AND NEW.job_posting_id = OLD.job_posting_id
        AND NEW.platform_catalog_id = OLD.platform_catalog_id
        AND NEW.profile_artifact_version_id = OLD.profile_artifact_version_id
        AND NEW.invitation_game_day = OLD.invitation_game_day
        AND NEW.expires_exclusive_game_day = OLD.expires_exclusive_game_day
        AND NEW.artifact_completeness_bp = OLD.artifact_completeness_bp
        AND NEW.visible_education_score_bp = OLD.visible_education_score_bp
        AND NEW.visible_certification_score_bp = OLD.visible_certification_score_bp
        AND NEW.visible_language_score_bp = OLD.visible_language_score_bp
        AND NEW.visible_training_score_bp = OLD.visible_training_score_bp
        AND NEW.visible_experience_score_bp = OLD.visible_experience_score_bp
        AND NEW.visible_project_score_bp = OLD.visible_project_score_bp
        AND NEW.invitation_score_bp = OLD.invitation_score_bp
        AND NEW.invitation_probability_ppm = OLD.invitation_probability_ppm
        AND NEW.invitation_roll = OLD.invitation_roll
        AND NEW.occurrence = OLD.occurrence
        AND NEW.created_at = OLD.created_at
        AND (
            NEW.decided_game_day = (
                SELECT game_day FROM save
                WHERE id = OLD.save_id AND run_revision = OLD.run_revision
            )
            OR (
                NEW.status = 'expired'
                AND EXISTS (
                    SELECT 1
                    FROM career_scheduled_action AS action
                    WHERE action.save_id = OLD.save_id
                      AND action.run_revision = OLD.run_revision
                      AND action.action_kind = 'invitationGeneration'
                      AND action.platform_catalog_id = OLD.platform_catalog_id
                      AND action.status = 'pending'
                      AND action.due_game_day = NEW.decided_game_day
                )
            )
        )
        AND (
            (
                NEW.status = 'accepted'
                AND NEW.decided_game_day < OLD.expires_exclusive_game_day
                AND EXISTS (
                    SELECT 1
                    FROM job_application AS application
                    WHERE application.id = NEW.accepted_application_id
                      AND application.save_id = OLD.save_id
                      AND application.run_revision = OLD.run_revision
                      AND application.source_kind = 'invitation'
                      AND application.source_invitation_id = OLD.id
                      AND application.status = 'interviewAwaitingConfirmation'
                )
            )
            OR (
                NEW.status = 'declined'
                AND NEW.accepted_application_id IS NULL
                AND NEW.decided_game_day < OLD.expires_exclusive_game_day
            )
            OR (
                NEW.status = 'expired'
                AND NEW.accepted_application_id IS NULL
                AND NEW.decided_game_day >= OLD.expires_exclusive_game_day
            )
            OR (
                NEW.status = 'closed'
                AND NEW.accepted_application_id IS NULL
            )
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_job_invitation_no_delete
BEFORE DELETE ON job_invitation
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'job invitations cannot be deleted';

CREATE TRIGGER tr_job_offer_valid_insert
BEFORE INSERT ON job_offer
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'pending'
        AND NEW.employment_contract_id IS NULL
        AND NEW.decided_game_day IS NULL
        AND EXISTS (
            SELECT 1
            FROM save
            INNER JOIN job_application AS application
                ON application.save_id = save.id
               AND application.run_revision = save.run_revision
               AND application.career_catalog_bundle_id
                    = NEW.career_catalog_bundle_id
               AND application.id = NEW.job_application_id
               AND application.status = 'interviewConfirmed'
            INNER JOIN job_posting AS posting
                ON posting.id = application.job_posting_id
               AND posting.id = NEW.job_posting_id
               AND posting.career_catalog_bundle_id
                    = application.career_catalog_bundle_id
               AND posting.recruitment_ruleset_id = NEW.recruitment_ruleset_id
            INNER JOIN job_template AS template
                ON template.id = posting.job_template_id
               AND template.career_catalog_bundle_id
                    = posting.career_catalog_bundle_id
            INNER JOIN platform_catalog AS platform
                ON platform.id = posting.platform_catalog_id
               AND platform.career_catalog_bundle_id
                    = posting.career_catalog_bundle_id
            INNER JOIN recruitment_ruleset AS ruleset
                ON ruleset.id = posting.recruitment_ruleset_id
            WHERE save.id = NEW.save_id
              AND save.run_revision = NEW.run_revision
              AND EXISTS (
                  SELECT 1
                  FROM career_scheduled_action AS action
                  WHERE action.save_id = NEW.save_id
                    AND action.run_revision = NEW.run_revision
                    AND action.action_kind = 'interviewDecision'
                    AND action.job_application_id = application.id
                    AND action.status = 'pending'
                    AND action.due_game_day = NEW.offered_game_day
              )
              AND NEW.career_industry_id = posting.career_industry_id
              AND NEW.career_job_family_id = posting.career_job_family_id
              AND NEW.virtual_employer_id = posting.virtual_employer_id
              AND BINARY NEW.region = BINARY posting.region
              AND BINARY NEW.employment_type = BINARY posting.employment_type
              AND NEW.payday_day_of_month = ruleset.payday_day_of_month
              AND NEW.expires_exclusive_game_day
                  = NEW.offered_game_day + posting.offer_expiry_days
              AND NEW.start_game_day = NEW.expires_exclusive_game_day
                  + ruleset.employment_start_delay_days
              AND NEW.first_pay_reward_krw = platform.first_pay_reward_krw
              AND NEW.annual_salary_krw BETWEEN
                  posting.minimum_annual_salary_krw
                  AND posting.maximum_annual_salary_krw
              AND MOD(
                  NEW.annual_salary_krw - posting.minimum_annual_salary_krw,
                  posting.salary_step_krw
              ) = 0
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_job_offer_transition_only
BEFORE UPDATE ON job_offer
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'pending'
        AND NEW.status IN ('accepted', 'declined', 'expired', 'closed')
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.career_catalog_bundle_id = OLD.career_catalog_bundle_id
        AND NEW.recruitment_ruleset_id = OLD.recruitment_ruleset_id
        AND NEW.job_application_id = OLD.job_application_id
        AND NEW.job_posting_id = OLD.job_posting_id
        AND NEW.career_industry_id = OLD.career_industry_id
        AND NEW.career_job_family_id = OLD.career_job_family_id
        AND NEW.virtual_employer_id = OLD.virtual_employer_id
        AND NEW.annual_salary_krw = OLD.annual_salary_krw
        AND BINARY NEW.region = BINARY OLD.region
        AND BINARY NEW.employment_type = BINARY OLD.employment_type
        AND NEW.payday_day_of_month = OLD.payday_day_of_month
        AND NEW.offered_game_day = OLD.offered_game_day
        AND NEW.start_game_day = OLD.start_game_day
        AND NEW.expires_exclusive_game_day = OLD.expires_exclusive_game_day
        AND NEW.first_pay_reward_krw = OLD.first_pay_reward_krw
        AND NEW.occurrence = OLD.occurrence
        AND NEW.created_at = OLD.created_at
        AND (
            NEW.decided_game_day = (
                SELECT game_day FROM save
                WHERE id = OLD.save_id AND run_revision = OLD.run_revision
            )
            OR (
                NEW.status = 'expired'
                AND EXISTS (
                    SELECT 1
                    FROM career_scheduled_action AS action
                    WHERE action.save_id = OLD.save_id
                      AND action.run_revision = OLD.run_revision
                      AND action.action_kind = 'offerExpiry'
                      AND action.job_application_id = OLD.job_application_id
                      AND action.status = 'pending'
                      AND action.due_game_day = NEW.decided_game_day
                )
            )
        )
        AND (
            (
                NEW.status = 'accepted'
                AND NEW.decided_game_day < OLD.expires_exclusive_game_day
                AND EXISTS (
                    SELECT 1
                    FROM employment_contract AS contract
                    WHERE contract.id = NEW.employment_contract_id
                      AND contract.save_id = OLD.save_id
                      AND contract.run_revision = OLD.run_revision
                      AND contract.job_offer_id = OLD.id
                      AND contract.status = 'pendingStart'
                )
            )
            OR (
                NEW.status = 'declined'
                AND NEW.employment_contract_id IS NULL
                AND NEW.decided_game_day < OLD.expires_exclusive_game_day
            )
            OR (
                NEW.status = 'expired'
                AND NEW.employment_contract_id IS NULL
                AND NEW.decided_game_day >= OLD.expires_exclusive_game_day
            )
            OR (
                NEW.status = 'closed'
                AND NEW.employment_contract_id IS NULL
            )
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_job_offer_no_delete
BEFORE DELETE ON job_offer
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'job offers cannot be deleted';

CREATE TRIGGER tr_employment_contract_valid_insert
BEFORE INSERT ON employment_contract
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'pendingStart'
        AND NEW.end_game_day IS NULL
        AND NEW.credited_experience_days = 0
        AND NEW.last_credited_game_day IS NULL
        AND EXISTS (
            SELECT 1
            FROM save
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
        AND NEW.job_offer_id = OLD.job_offer_id
        AND NEW.job_application_id = OLD.job_application_id
        AND NEW.job_posting_id = OLD.job_posting_id
        AND NEW.career_industry_id = OLD.career_industry_id
        AND NEW.career_job_family_id = OLD.career_job_family_id
        AND NEW.virtual_employer_id = OLD.virtual_employer_id
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
                AND NEW.credited_experience_days
                    = OLD.credited_experience_days + 1
                AND NEW.last_credited_game_day
                    = OLD.last_credited_game_day + 1
            )
            OR (
                OLD.status = 'pendingStart'
                AND NEW.status = 'ended'
                AND NEW.end_game_day = OLD.start_game_day
                AND NEW.credited_experience_days = 0
                AND NEW.last_credited_game_day IS NULL
            )
            OR (
                OLD.status = 'active'
                AND NEW.status = 'ended'
                AND NEW.end_game_day = OLD.last_credited_game_day + 1
                AND NEW.credited_experience_days = OLD.credited_experience_days
                AND NEW.last_credited_game_day = OLD.last_credited_game_day
            )
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_employment_contract_no_delete
BEFORE DELETE ON employment_contract
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'employment contracts cannot be deleted';

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
        AND NEW.recruitment_ruleset_id = OLD.recruitment_ruleset_id
        AND BINARY NEW.action_kind = BINARY OLD.action_kind
        AND NEW.payload_version = OLD.payload_version
        AND NEW.phase_rank = OLD.phase_rank
        AND NEW.due_game_day = OLD.due_game_day
        AND BINARY NEW.source_kind = BINARY OLD.source_kind
        AND NEW.source_id = OLD.source_id
        AND NEW.occurrence = OLD.occurrence
        AND NEW.employment_contract_id <=> OLD.employment_contract_id
        AND NEW.job_application_id <=> OLD.job_application_id
        AND NEW.platform_catalog_id <=> OLD.platform_catalog_id
        AND NEW.invitation_generation_game_day
            <=> OLD.invitation_generation_game_day
        AND NEW.created_at = OLD.created_at
        AND (
            (
                NEW.status = 'completed'
                AND NEW.completed_game_day = OLD.due_game_day
                AND NEW.cancelled_game_day IS NULL
            )
            OR (
                NEW.status = 'cancelled'
                AND NEW.completed_game_day IS NULL
                AND NEW.cancelled_game_day IS NOT NULL
            )
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_career_scheduled_action_no_delete
BEFORE DELETE ON career_scheduled_action
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'career scheduled actions cannot be deleted';

INSERT INTO recruitment_ruleset
    (
        ruleset_key,
        ranked_eligible,
        active_application_limit,
        daily_application_limit,
        open_invitation_limit,
        employment_start_delay_days,
        payday_day_of_month
    )
VALUES
    ('dev-unranked-m3-recruitment-v1', FALSE, 10, 3, 5, 1, 25);

INSERT INTO recruitment_stage_component_weight
    (recruitment_ruleset_id, stage, component, weight_bp)
SELECT ruleset.id, seed.stage, seed.component, seed.weight_bp
FROM recruitment_ruleset AS ruleset
CROSS JOIN (
    SELECT 'document' AS stage, 'visibleFit' AS component, 6000 AS weight_bp
    UNION ALL SELECT 'document', 'artifactCompleteness', 2500
    UNION ALL SELECT 'document', 'platformAffinity', 1500
    UNION ALL SELECT 'interview', 'possessedFit', 6000
    UNION ALL SELECT 'interview', 'experienceProjectFit', 2500
    UNION ALL SELECT 'interview', 'profileConsistency', 1500
    UNION ALL SELECT 'invitation', 'artifactCompleteness', 5000
    UNION ALL SELECT 'invitation', 'languageScore', 2500
    UNION ALL SELECT 'invitation', 'experienceScore', 2500
) AS seed
WHERE BINARY ruleset.ruleset_key = BINARY 'dev-unranked-m3-recruitment-v1';

INSERT INTO recruitment_score_band
    (
        recruitment_ruleset_id,
        score_band_key,
        minimum_score_bp,
        maximum_exclusive_score_bp
    )
SELECT
    ruleset.id,
    seed.score_band_key,
    seed.minimum_score_bp,
    seed.maximum_exclusive_score_bp
FROM recruitment_ruleset AS ruleset
CROSS JOIN (
    SELECT 'low' AS score_band_key, 0 AS minimum_score_bp,
           4000 AS maximum_exclusive_score_bp
    UNION ALL SELECT 'medium', 4000, 7000
    UNION ALL SELECT 'high', 7000, 10001
) AS seed
WHERE BINARY ruleset.ruleset_key = BINARY 'dev-unranked-m3-recruitment-v1';

INSERT INTO recruitment_pass_probability
    (
        recruitment_ruleset_id,
        stage,
        competition_band,
        score_band_key,
        pass_probability_ppm
    )
SELECT
    ruleset.id,
    seed.stage,
    seed.competition_band,
    seed.score_band_key,
    seed.pass_probability_ppm
FROM recruitment_ruleset AS ruleset
CROSS JOIN (
    SELECT 'document' AS stage, 'low' AS competition_band,
           'low' AS score_band_key, 400000 AS pass_probability_ppm
    UNION ALL SELECT 'document', 'low', 'medium', 700000
    UNION ALL SELECT 'document', 'low', 'high', 900000
    UNION ALL SELECT 'document', 'medium', 'low', 250000
    UNION ALL SELECT 'document', 'medium', 'medium', 550000
    UNION ALL SELECT 'document', 'medium', 'high', 800000
    UNION ALL SELECT 'document', 'high', 'low', 120000
    UNION ALL SELECT 'document', 'high', 'medium', 350000
    UNION ALL SELECT 'document', 'high', 'high', 650000
    UNION ALL SELECT 'interview', 'low', 'low', 350000
    UNION ALL SELECT 'interview', 'low', 'medium', 650000
    UNION ALL SELECT 'interview', 'low', 'high', 880000
    UNION ALL SELECT 'interview', 'medium', 'low', 220000
    UNION ALL SELECT 'interview', 'medium', 'medium', 500000
    UNION ALL SELECT 'interview', 'medium', 'high', 760000
    UNION ALL SELECT 'interview', 'high', 'low', 100000
    UNION ALL SELECT 'interview', 'high', 'medium', 300000
    UNION ALL SELECT 'interview', 'high', 'high', 600000
    UNION ALL SELECT 'invitation', 'low', 'low', 50000
    UNION ALL SELECT 'invitation', 'low', 'medium', 150000
    UNION ALL SELECT 'invitation', 'low', 'high', 300000
    UNION ALL SELECT 'invitation', 'medium', 'low', 35000
    UNION ALL SELECT 'invitation', 'medium', 'medium', 120000
    UNION ALL SELECT 'invitation', 'medium', 'high', 250000
    UNION ALL SELECT 'invitation', 'high', 'low', 20000
    UNION ALL SELECT 'invitation', 'high', 'medium', 80000
    UNION ALL SELECT 'invitation', 'high', 'high', 200000
) AS seed
WHERE BINARY ruleset.ruleset_key = BINARY 'dev-unranked-m3-recruitment-v1';

UPDATE recruitment_ruleset
SET published_at = CURRENT_TIMESTAMP(3)
WHERE BINARY ruleset_key = BINARY 'dev-unranked-m3-recruitment-v1';

INSERT INTO career_recruitment_compatibility
    (career_catalog_bundle_id, recruitment_ruleset_id)
SELECT bundle.id, ruleset.id
FROM career_catalog_bundle AS bundle
INNER JOIN recruitment_ruleset AS ruleset
    ON BINARY ruleset.ruleset_key = BINARY 'dev-unranked-m3-recruitment-v1'
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO recruitment_ruleset_assignment
    (career_catalog_bundle_id, assignment_key, recruitment_ruleset_id)
SELECT compatibility.career_catalog_bundle_id, 'newPosting', ruleset.id
FROM recruitment_ruleset AS ruleset
INNER JOIN career_recruitment_compatibility AS compatibility
    ON compatibility.recruitment_ruleset_id = ruleset.id
WHERE BINARY ruleset.ruleset_key = BINARY 'dev-unranked-m3-recruitment-v1';
