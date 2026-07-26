-- M3-A run-scoped career specs, activities, and immutable artifact versions (§2.3–§4).

CREATE TABLE career_run (
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    career_catalog_bundle_id        BIGINT UNSIGNED NOT NULL,
    focused_job_family_key          VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    birth_date                      DATE            NOT NULL,
    created_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, run_revision),
    UNIQUE KEY uk_career_run_bundle
        (save_id, run_revision, career_catalog_bundle_id),
    CONSTRAINT fk_career_run_save
        FOREIGN KEY (save_id) REFERENCES save (id) ON DELETE CASCADE,
    CONSTRAINT fk_career_run_bundle
        FOREIGN KEY (career_catalog_bundle_id) REFERENCES career_catalog_bundle (id),
    CONSTRAINT fk_career_run_focus
        FOREIGN KEY (career_catalog_bundle_id, focused_job_family_key)
        REFERENCES career_job_family (career_catalog_bundle_id, job_family_key),
    CONSTRAINT ck_career_run_focus_key CHECK (CHAR_LENGTH(focused_job_family_key) > 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE spec_activity (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    career_catalog_bundle_id        BIGINT UNSIGNED NOT NULL,
    activity_catalog_entry_id       BIGINT UNSIGNED NOT NULL,
    status                          VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    priority                        TINYINT UNSIGNED     NULL,
    active_priority_slot            TINYINT UNSIGNED GENERATED ALWAYS AS (
        CASE WHEN status = 'active' THEN priority ELSE NULL END
    ) STORED,
    planned_game_day                INT UNSIGNED    NOT NULL,
    started_game_day                INT UNSIGNED        NULL,
    accumulated_effort_units        BIGINT UNSIGNED NOT NULL DEFAULT 0,
    completed_game_day              INT UNSIGNED        NULL,
    cancelled_game_day              INT UNSIGNED        NULL,
    cost_ledger_transaction_id      BIGINT UNSIGNED     NULL,
    created_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_spec_activity_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_spec_activity_save_run_bundle_id
        (save_id, run_revision, career_catalog_bundle_id, id),
    UNIQUE KEY uk_spec_activity_active_priority
        (save_id, run_revision, active_priority_slot),
    KEY ix_spec_activity_history
        (save_id, run_revision, planned_game_day, id),
    KEY ix_spec_activity_catalog
        (career_catalog_bundle_id, activity_catalog_entry_id),
    KEY ix_spec_activity_cost_ledger
        (save_id, run_revision, cost_ledger_transaction_id),
    CONSTRAINT fk_spec_activity_career_run
        FOREIGN KEY (save_id, run_revision, career_catalog_bundle_id)
        REFERENCES career_run (save_id, run_revision, career_catalog_bundle_id)
        ON DELETE CASCADE,
    CONSTRAINT fk_spec_activity_catalog
        FOREIGN KEY (career_catalog_bundle_id, activity_catalog_entry_id)
        REFERENCES activity_catalog_entry (career_catalog_bundle_id, id),
    CONSTRAINT fk_spec_activity_cost_ledger
        FOREIGN KEY (save_id, run_revision, cost_ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT ck_spec_activity_status CHECK (
        status IN ('planned', 'active', 'completed', 'cancelled')
    ),
    CONSTRAINT ck_spec_activity_state_shape CHECK (
        (
            status = 'planned'
            AND priority IS NULL
            AND started_game_day IS NULL
            AND accumulated_effort_units = 0
            AND completed_game_day IS NULL
            AND cancelled_game_day IS NULL
            AND cost_ledger_transaction_id IS NULL
        )
        OR (
            status = 'active'
            AND priority IS NOT NULL
            AND priority BETWEEN 1 AND 3
            AND started_game_day IS NOT NULL
            AND started_game_day >= planned_game_day
            AND completed_game_day IS NULL
            AND cancelled_game_day IS NULL
        )
        OR (
            status = 'completed'
            AND priority IS NOT NULL
            AND priority BETWEEN 1 AND 3
            AND started_game_day IS NOT NULL
            AND started_game_day >= planned_game_day
            AND completed_game_day IS NOT NULL
            AND completed_game_day >= started_game_day
            AND cancelled_game_day IS NULL
        )
        OR (
            status = 'cancelled'
            AND completed_game_day IS NULL
            AND cancelled_game_day IS NOT NULL
            AND cancelled_game_day >= planned_game_day
            AND (
                (
                    priority IS NULL
                    AND started_game_day IS NULL
                    AND accumulated_effort_units = 0
                    AND cost_ledger_transaction_id IS NULL
                )
                OR (
                    priority IS NOT NULL
                    AND priority BETWEEN 1 AND 3
                    AND started_game_day IS NOT NULL
                    AND started_game_day >= planned_game_day
                    AND cancelled_game_day >= started_game_day
                )
            )
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE spec_evidence (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    career_catalog_bundle_id        BIGINT UNSIGNED NOT NULL,
    evidence_key                    VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    spec_catalog_entry_id           BIGINT UNSIGNED NOT NULL,
    kind                            VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    acquired_game_day               INT UNSIGNED    NOT NULL,
    expires_on_game_day             INT UNSIGNED        NULL,
    period_start_date               DATE                NULL,
    period_end_exclusive_date       DATE                NULL,
    source_kind                     VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    source_activity_id              BIGINT UNSIGNED     NULL,
    source_employment_contract_id   BIGINT UNSIGNED     NULL,
    source_military_service_id      BIGINT UNSIGNED     NULL,
    created_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_spec_evidence_save_run_key (save_id, run_revision, evidence_key),
    UNIQUE KEY uk_spec_evidence_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_spec_evidence_save_run_bundle_id
        (save_id, run_revision, career_catalog_bundle_id, id),
    KEY ix_spec_evidence_history
        (save_id, run_revision, acquired_game_day, id),
    KEY ix_spec_evidence_catalog
        (career_catalog_bundle_id, spec_catalog_entry_id),
    KEY ix_spec_evidence_source_activity
        (save_id, run_revision, career_catalog_bundle_id, source_activity_id),
    CONSTRAINT fk_spec_evidence_career_run
        FOREIGN KEY (save_id, run_revision, career_catalog_bundle_id)
        REFERENCES career_run (save_id, run_revision, career_catalog_bundle_id)
        ON DELETE CASCADE,
    CONSTRAINT fk_spec_evidence_catalog
        FOREIGN KEY (career_catalog_bundle_id, spec_catalog_entry_id)
        REFERENCES spec_catalog_entry (career_catalog_bundle_id, id),
    CONSTRAINT fk_spec_evidence_source_activity
        FOREIGN KEY (save_id, run_revision, career_catalog_bundle_id, source_activity_id)
        REFERENCES spec_activity (save_id, run_revision, career_catalog_bundle_id, id),
    CONSTRAINT ck_spec_evidence_key CHECK (CHAR_LENGTH(evidence_key) > 0),
    CONSTRAINT ck_spec_evidence_kind CHECK (
        kind IN ('education', 'certification', 'language', 'training', 'experience', 'project')
    ),
    CONSTRAINT ck_spec_evidence_expiry CHECK (
        expires_on_game_day IS NULL OR expires_on_game_day >= acquired_game_day
    ),
    CONSTRAINT ck_spec_evidence_period CHECK (
        (
            period_start_date IS NULL
            AND period_end_exclusive_date IS NULL
        )
        OR (
            period_start_date IS NOT NULL
            AND period_end_exclusive_date IS NOT NULL
            AND (
                period_start_date < period_end_exclusive_date
                OR (
                    source_kind = 'bridgeExperience'
                    AND kind = 'experience'
                    AND period_start_date = period_end_exclusive_date
                )
            )
        )
    ),
    CONSTRAINT ck_spec_evidence_source_union CHECK (
        (
            source_kind IN ('bridgeEducation', 'bridgeCertification', 'bridgeExperience')
            AND source_activity_id IS NULL
            AND source_employment_contract_id IS NULL
            AND source_military_service_id IS NULL
            AND (
                (source_kind = 'bridgeEducation' AND kind = 'education')
                OR (source_kind = 'bridgeCertification' AND kind = 'certification')
                OR (
                    source_kind = 'bridgeExperience'
                    AND kind = 'experience'
                    AND period_start_date IS NOT NULL
                    AND period_end_exclusive_date IS NOT NULL
                )
            )
        )
        OR (
            source_kind = 'activity'
            AND source_activity_id IS NOT NULL
            AND source_employment_contract_id IS NULL
            AND source_military_service_id IS NULL
        )
        OR (
            source_kind = 'employmentContract'
            AND kind = 'experience'
            AND source_activity_id IS NULL
            AND source_employment_contract_id IS NOT NULL
            AND source_military_service_id IS NULL
            AND period_start_date IS NOT NULL
            AND period_end_exclusive_date IS NOT NULL
            AND period_start_date < period_end_exclusive_date
        )
        OR (
            source_kind = 'militaryService'
            AND kind = 'experience'
            AND source_activity_id IS NULL
            AND source_employment_contract_id IS NULL
            AND source_military_service_id IS NOT NULL
            AND period_start_date IS NOT NULL
            AND period_end_exclusive_date IS NOT NULL
            AND period_start_date < period_end_exclusive_date
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE profile_artifact_version (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    career_catalog_bundle_id        BIGINT UNSIGNED NOT NULL,
    artifact_kind                   VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    version_no                      INT UNSIGNED    NOT NULL,
    headline                        VARCHAR(120)    NOT NULL,
    summary                         VARCHAR(2000)   NOT NULL,
    open_to_work                    BOOLEAN             NULL,
    completeness_bp                 INT             NOT NULL,
    created_game_day                INT UNSIGNED    NOT NULL,
    sealed_at                       DATETIME(3)          NULL,
    created_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_profile_artifact_version_number
        (save_id, run_revision, artifact_kind, version_no),
    UNIQUE KEY uk_profile_artifact_version_save_run_id
        (save_id, run_revision, id),
    UNIQUE KEY uk_profile_artifact_version_save_run_bundle_id
        (save_id, run_revision, career_catalog_bundle_id, id),
    KEY ix_profile_artifact_version_history
        (save_id, run_revision, artifact_kind, version_no, id),
    CONSTRAINT fk_profile_artifact_version_career_run
        FOREIGN KEY (save_id, run_revision, career_catalog_bundle_id)
        REFERENCES career_run (save_id, run_revision, career_catalog_bundle_id)
        ON DELETE CASCADE,
    CONSTRAINT ck_profile_artifact_version_kind CHECK (
        artifact_kind IN ('portfolio', 'resume', 'linkedinProfile')
    ),
    CONSTRAINT ck_profile_artifact_version_number CHECK (version_no > 0),
    CONSTRAINT ck_profile_artifact_version_headline CHECK (
        CHAR_LENGTH(headline) BETWEEN 1 AND 120
    ),
    CONSTRAINT ck_profile_artifact_version_summary CHECK (CHAR_LENGTH(summary) <= 2000),
    CONSTRAINT ck_profile_artifact_version_linkedin CHECK (
        (
            artifact_kind = 'linkedinProfile'
            AND open_to_work IS NOT NULL
            AND open_to_work IN (FALSE, TRUE)
        )
        OR (
            artifact_kind <> 'linkedinProfile'
            AND open_to_work IS NULL
        )
    ),
    CONSTRAINT ck_profile_artifact_version_completeness CHECK (
        completeness_bp BETWEEN 0 AND 10000
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE profile_artifact_evidence (
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    career_catalog_bundle_id        BIGINT UNSIGNED NOT NULL,
    profile_artifact_version_id     BIGINT UNSIGNED NOT NULL,
    evidence_id                     BIGINT UNSIGNED NOT NULL,
    selection_order                 TINYINT UNSIGNED NOT NULL,
    created_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, run_revision, profile_artifact_version_id, evidence_id),
    UNIQUE KEY uk_profile_artifact_evidence_order
        (save_id, run_revision, profile_artifact_version_id, selection_order),
    KEY ix_profile_artifact_evidence_evidence
        (save_id, run_revision, evidence_id, profile_artifact_version_id),
    CONSTRAINT fk_profile_artifact_evidence_artifact
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
        ) ON DELETE CASCADE,
    CONSTRAINT fk_profile_artifact_evidence_evidence
        FOREIGN KEY (save_id, run_revision, career_catalog_bundle_id, evidence_id)
        REFERENCES spec_evidence (save_id, run_revision, career_catalog_bundle_id, id)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE profile_artifact_industry (
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED    NOT NULL,
    career_catalog_bundle_id        BIGINT UNSIGNED NOT NULL,
    profile_artifact_version_id     BIGINT UNSIGNED NOT NULL,
    career_industry_id              BIGINT UNSIGNED NOT NULL,
    selection_order                 TINYINT UNSIGNED NOT NULL,
    created_at                      DATETIME(3)      NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (
        save_id,
        run_revision,
        profile_artifact_version_id,
        career_industry_id
    ),
    UNIQUE KEY uk_profile_artifact_industry_order
        (save_id, run_revision, profile_artifact_version_id, selection_order),
    KEY ix_profile_artifact_industry_catalog
        (career_catalog_bundle_id, career_industry_id),
    CONSTRAINT fk_profile_artifact_industry_artifact
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
        ) ON DELETE CASCADE,
    CONSTRAINT fk_profile_artifact_industry_catalog
        FOREIGN KEY (career_catalog_bundle_id, career_industry_id)
        REFERENCES career_industry (career_catalog_bundle_id, id),
    CONSTRAINT ck_profile_artifact_industry_order CHECK (selection_order BETWEEN 1 AND 3)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_career_run_valid_insert
BEFORE INSERT ON career_run
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM save
        INNER JOIN `character` ON `character`.save_id = save.id
        INNER JOIN market_world ON market_world.id = save.market_world_id
        INNER JOIN career_catalog_assignment AS assignment
            ON BINARY assignment.assignment_key = BINARY 'newRun'
           AND assignment.career_catalog_bundle_id = NEW.career_catalog_bundle_id
        INNER JOIN career_catalog_bundle AS bundle
            ON bundle.id = assignment.career_catalog_bundle_id
           AND bundle.published_at IS NOT NULL
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

CREATE TRIGGER tr_career_run_no_delete
BEFORE DELETE ON career_run
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'career runs cannot be deleted';

CREATE TRIGGER tr_spec_activity_valid_insert
BEFORE INSERT ON spec_activity
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status IN ('planned', 'active')
        AND NEW.accumulated_effort_units = 0
        AND EXISTS (
        SELECT 1
        FROM save
        INNER JOIN career_run
            ON career_run.save_id = save.id
           AND career_run.run_revision = save.run_revision
           AND career_run.career_catalog_bundle_id = NEW.career_catalog_bundle_id
        WHERE save.id = NEW.save_id
          AND save.run_revision = NEW.run_revision
          AND save.game_day = NEW.planned_game_day
          AND (
              NEW.status = 'planned'
              OR NEW.started_game_day = save.game_day
          )
    )
        AND EXISTS (
            SELECT 1
            FROM activity_catalog_entry AS catalog
            WHERE catalog.career_catalog_bundle_id = NEW.career_catalog_bundle_id
              AND catalog.id = NEW.activity_catalog_entry_id
              AND NEW.accumulated_effort_units <= catalog.required_effort_units
              AND (
                  NEW.status = 'planned'
                  OR (
                      NEW.status = 'cancelled'
                      AND NEW.started_game_day IS NULL
                  )
                  OR (
                      (catalog.cost_krw = 0 AND NEW.cost_ledger_transaction_id IS NULL)
                      OR (catalog.cost_krw > 0 AND NEW.cost_ledger_transaction_id IS NOT NULL)
                  )
              )
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_spec_activity_transition_only
BEFORE UPDATE ON spec_activity
FOR EACH ROW
SET NEW.id = IF(
    NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.career_catalog_bundle_id = OLD.career_catalog_bundle_id
        AND NEW.activity_catalog_entry_id = OLD.activity_catalog_entry_id
        AND NEW.planned_game_day = OLD.planned_game_day
        AND NEW.created_at = OLD.created_at
        AND NEW.accumulated_effort_units >= OLD.accumulated_effort_units
        AND EXISTS (
            SELECT 1
            FROM save
            WHERE id = OLD.save_id
              AND run_revision = OLD.run_revision
        )
        AND EXISTS (
            SELECT 1
            FROM activity_catalog_entry AS catalog
            WHERE catalog.career_catalog_bundle_id = OLD.career_catalog_bundle_id
              AND catalog.id = OLD.activity_catalog_entry_id
              AND NEW.accumulated_effort_units <= catalog.required_effort_units
              AND (
                  NEW.status <> 'completed'
                  OR (
                      NEW.accumulated_effort_units = catalog.required_effort_units
                      AND NEW.started_game_day IS NOT NULL
                      AND NEW.completed_game_day IS NOT NULL
                      AND NEW.completed_game_day
                          >= NEW.started_game_day + catalog.minimum_calendar_days - 1
                  )
              )
              AND (
                  (
                      NEW.started_game_day IS NULL
                      AND NEW.cost_ledger_transaction_id IS NULL
                  )
                  OR (
                      (catalog.cost_krw = 0 AND NEW.cost_ledger_transaction_id IS NULL)
                      OR (catalog.cost_krw > 0 AND NEW.cost_ledger_transaction_id IS NOT NULL)
                  )
              )
        )
        AND (
            (
                OLD.status = 'planned'
                AND NEW.status = 'active'
                AND NEW.accumulated_effort_units = 0
                AND NEW.started_game_day = (
                    SELECT game_day
                    FROM save
                    WHERE id = OLD.save_id
                )
            )
            OR (
                OLD.status = 'planned'
                AND NEW.status = 'cancelled'
                AND NEW.accumulated_effort_units = 0
            )
            OR (
                OLD.status = 'active'
                AND NEW.status = 'active'
                AND NEW.priority = OLD.priority
                AND NEW.started_game_day = OLD.started_game_day
                AND NEW.completed_game_day IS NULL
                AND NEW.cancelled_game_day IS NULL
                AND NEW.cost_ledger_transaction_id <=> OLD.cost_ledger_transaction_id
            )
            OR (
                OLD.status = 'active'
                AND NEW.status = 'completed'
                AND NEW.priority = OLD.priority
                AND NEW.started_game_day = OLD.started_game_day
                AND NEW.cost_ledger_transaction_id <=> OLD.cost_ledger_transaction_id
            )
            OR (
                OLD.status = 'active'
                AND NEW.status = 'cancelled'
                AND NEW.priority = OLD.priority
                AND NEW.started_game_day = OLD.started_game_day
                AND NEW.accumulated_effort_units = OLD.accumulated_effort_units
                AND NEW.cost_ledger_transaction_id <=> OLD.cost_ledger_transaction_id
            )
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_spec_activity_no_delete
BEFORE DELETE ON spec_activity
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'career activities cannot be deleted';

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
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_spec_evidence_no_update
BEFORE UPDATE ON spec_evidence
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'spec evidence is immutable';

CREATE TRIGGER tr_spec_evidence_no_delete
BEFORE DELETE ON spec_evidence
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'spec evidence cannot be deleted';

CREATE TRIGGER tr_profile_artifact_version_valid_insert
BEFORE INSERT ON profile_artifact_version
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.sealed_at IS NULL
        AND BINARY NEW.headline = BINARY TRIM(NEW.headline)
        AND BINARY NEW.summary = BINARY TRIM(NEW.summary)
        AND NOT REGEXP_LIKE(
            NEW.headline,
            '[\\x{0000}-\\x{001F}\\x{0085}\\x{2028}\\x{2029}]'
        )
        AND NOT REGEXP_LIKE(
            NEW.summary,
            '[\\x{0000}-\\x{0008}\\x{000B}-\\x{001F}]'
        )
        AND EXISTS (
            SELECT 1
            FROM save
            INNER JOIN career_run
                ON career_run.save_id = save.id
               AND career_run.run_revision = save.run_revision
               AND career_run.career_catalog_bundle_id = NEW.career_catalog_bundle_id
            WHERE save.id = NEW.save_id
              AND save.run_revision = NEW.run_revision
              AND save.game_day = NEW.created_game_day
        )
        AND NEW.version_no = (
            SELECT COALESCE(MAX(existing.version_no), 0) + 1
            FROM profile_artifact_version AS existing
            WHERE existing.save_id = NEW.save_id
              AND existing.run_revision = NEW.run_revision
              AND BINARY existing.artifact_kind = BINARY NEW.artifact_kind
        ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_profile_artifact_evidence_valid_insert
BEFORE INSERT ON profile_artifact_evidence
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM profile_artifact_version AS artifact
        INNER JOIN spec_evidence AS evidence
            ON evidence.save_id = artifact.save_id
           AND evidence.run_revision = artifact.run_revision
           AND evidence.career_catalog_bundle_id = artifact.career_catalog_bundle_id
           AND evidence.id = NEW.evidence_id
        INNER JOIN career_run
            ON career_run.save_id = artifact.save_id
           AND career_run.run_revision = artifact.run_revision
           AND career_run.career_catalog_bundle_id = artifact.career_catalog_bundle_id
        INNER JOIN save ON save.id = artifact.save_id
        INNER JOIN market_world ON market_world.id = save.market_world_id
        WHERE artifact.id = NEW.profile_artifact_version_id
          AND artifact.save_id = NEW.save_id
          AND artifact.run_revision = NEW.run_revision
          AND artifact.career_catalog_bundle_id = NEW.career_catalog_bundle_id
          AND artifact.sealed_at IS NULL
          AND save.run_revision = artifact.run_revision
          AND evidence.acquired_game_day <= artifact.created_game_day
          AND NEW.selection_order = (
              SELECT COALESCE(MAX(existing.selection_order), 0) + 1
              FROM profile_artifact_evidence AS existing
              WHERE existing.save_id = NEW.save_id
                AND existing.run_revision = NEW.run_revision
                AND existing.profile_artifact_version_id
                    = NEW.profile_artifact_version_id
          )
          AND NEW.selection_order BETWEEN 1 AND CASE artifact.artifact_kind
              WHEN 'portfolio' THEN 12
              WHEN 'resume' THEN 40
              WHEN 'linkedinProfile' THEN 30
          END
          AND (
              artifact.artifact_kind <> 'portfolio'
              OR evidence.kind IN ('certification', 'training', 'project')
          )
          AND (
              artifact.artifact_kind <> 'resume'
              OR (
                  (
                      evidence.period_start_date IS NULL
                      OR (
                          evidence.period_start_date
                              >= DATE_ADD(career_run.birth_date, INTERVAL 15 YEAR)
                          AND evidence.period_end_exclusive_date
                              <= DATE_ADD(
                                  market_world.start_date,
                                  INTERVAL artifact.created_game_day DAY
                              )
                      )
                  )
                  AND (
                      evidence.kind NOT IN ('education', 'experience')
                      OR evidence.period_start_date IS NULL
                      OR NOT EXISTS (
                          SELECT 1
                          FROM profile_artifact_evidence AS selected
                          INNER JOIN spec_evidence AS selected_evidence
                              ON selected_evidence.save_id = selected.save_id
                             AND selected_evidence.run_revision = selected.run_revision
                             AND selected_evidence.id = selected.evidence_id
                          WHERE selected.save_id = NEW.save_id
                            AND selected.run_revision = NEW.run_revision
                            AND selected.profile_artifact_version_id
                                = NEW.profile_artifact_version_id
                            AND BINARY selected_evidence.kind = BINARY evidence.kind
                            AND evidence.period_start_date
                                < selected_evidence.period_end_exclusive_date
                            AND selected_evidence.period_start_date
                                < evidence.period_end_exclusive_date
                      )
                  )
              )
          )
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_profile_artifact_industry_valid_insert
BEFORE INSERT ON profile_artifact_industry
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM profile_artifact_version AS artifact
        INNER JOIN save ON save.id = artifact.save_id
        INNER JOIN career_industry AS industry
            ON industry.career_catalog_bundle_id = artifact.career_catalog_bundle_id
           AND industry.id = NEW.career_industry_id
        WHERE artifact.id = NEW.profile_artifact_version_id
          AND artifact.save_id = NEW.save_id
          AND artifact.run_revision = NEW.run_revision
          AND artifact.career_catalog_bundle_id = NEW.career_catalog_bundle_id
          AND artifact.artifact_kind = 'linkedinProfile'
          AND artifact.sealed_at IS NULL
          AND save.run_revision = artifact.run_revision
          AND NEW.selection_order = (
              SELECT COALESCE(MAX(existing.selection_order), 0) + 1
              FROM profile_artifact_industry AS existing
              WHERE existing.save_id = NEW.save_id
                AND existing.run_revision = NEW.run_revision
                AND existing.profile_artifact_version_id
                    = NEW.profile_artifact_version_id
          )
          AND NEW.selection_order BETWEEN 1 AND 3
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_profile_artifact_version_seal_only
BEFORE UPDATE ON profile_artifact_version
FOR EACH ROW
SET NEW.id = IF(
    OLD.sealed_at IS NULL
        AND NEW.sealed_at IS NOT NULL
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.career_catalog_bundle_id = OLD.career_catalog_bundle_id
        AND BINARY NEW.artifact_kind = BINARY OLD.artifact_kind
        AND NEW.version_no = OLD.version_no
        AND BINARY NEW.headline = BINARY OLD.headline
        AND BINARY NEW.summary = BINARY OLD.summary
        AND NEW.open_to_work <=> OLD.open_to_work
        AND NEW.completeness_bp = OLD.completeness_bp
        AND NEW.created_game_day = OLD.created_game_day
        AND NEW.created_at = OLD.created_at
        AND EXISTS (
            SELECT 1
            FROM save
            WHERE id = OLD.save_id
              AND run_revision = OLD.run_revision
        )
        AND NEW.completeness_bp = (
            SELECT COALESCE(SUM(
                CASE
                    WHEN checklist.rule_kind = 'headlinePresent'
                        AND CHAR_LENGTH(TRIM(OLD.headline)) > 0
                        THEN checklist.weight_bp
                    WHEN checklist.rule_kind = 'summaryPresent'
                        AND CHAR_LENGTH(TRIM(OLD.summary)) > 0
                        THEN checklist.weight_bp
                    WHEN checklist.rule_kind = 'minimumEvidenceCount'
                        AND (
                            SELECT COUNT(*)
                            FROM profile_artifact_evidence AS selected
                            WHERE selected.save_id = OLD.save_id
                              AND selected.run_revision = OLD.run_revision
                              AND selected.profile_artifact_version_id = OLD.id
                        ) >= checklist.minimum_count
                        THEN checklist.weight_bp
                    WHEN checklist.rule_kind = 'containsDimension'
                        AND EXISTS (
                            SELECT 1
                            FROM profile_artifact_evidence AS selected
                            INNER JOIN spec_evidence AS evidence
                                ON evidence.save_id = selected.save_id
                               AND evidence.run_revision = selected.run_revision
                               AND evidence.id = selected.evidence_id
                            WHERE selected.save_id = OLD.save_id
                              AND selected.run_revision = OLD.run_revision
                              AND selected.profile_artifact_version_id = OLD.id
                              AND BINARY evidence.kind = BINARY checklist.dimension
                        )
                        THEN checklist.weight_bp
                    WHEN checklist.rule_kind = 'containsEvidenceKind'
                        AND EXISTS (
                            SELECT 1
                            FROM profile_artifact_evidence AS selected
                            INNER JOIN spec_evidence AS evidence
                                ON evidence.save_id = selected.save_id
                               AND evidence.run_revision = selected.run_revision
                               AND evidence.id = selected.evidence_id
                            WHERE selected.save_id = OLD.save_id
                              AND selected.run_revision = OLD.run_revision
                              AND selected.profile_artifact_version_id = OLD.id
                              AND BINARY evidence.kind = BINARY checklist.evidence_kind
                        )
                        THEN checklist.weight_bp
                    WHEN checklist.rule_kind = 'projectPresent'
                        AND EXISTS (
                            SELECT 1
                            FROM profile_artifact_evidence AS selected
                            INNER JOIN spec_evidence AS evidence
                                ON evidence.save_id = selected.save_id
                               AND evidence.run_revision = selected.run_revision
                               AND evidence.id = selected.evidence_id
                            WHERE selected.save_id = OLD.save_id
                              AND selected.run_revision = OLD.run_revision
                              AND selected.profile_artifact_version_id = OLD.id
                              AND evidence.kind = 'project'
                        )
                        THEN checklist.weight_bp
                    WHEN checklist.rule_kind = 'openToWork'
                        AND OLD.open_to_work = TRUE
                        THEN checklist.weight_bp
                    WHEN checklist.rule_kind = 'industryCountAtLeast'
                        AND (
                            SELECT COUNT(*)
                            FROM profile_artifact_industry AS industry
                            WHERE industry.save_id = OLD.save_id
                              AND industry.run_revision = OLD.run_revision
                              AND industry.profile_artifact_version_id = OLD.id
                        ) >= checklist.minimum_count
                        THEN checklist.weight_bp
                    ELSE 0
                END
            ), 0)
            FROM artifact_checklist_rule AS checklist
            WHERE checklist.career_catalog_bundle_id = OLD.career_catalog_bundle_id
              AND BINARY checklist.artifact_kind = BINARY OLD.artifact_kind
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_profile_artifact_version_no_delete
BEFORE DELETE ON profile_artifact_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'artifact versions cannot be deleted';

CREATE TRIGGER tr_profile_artifact_evidence_no_update
BEFORE UPDATE ON profile_artifact_evidence
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'artifact evidence selections are immutable';

CREATE TRIGGER tr_profile_artifact_evidence_no_delete
BEFORE DELETE ON profile_artifact_evidence
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'artifact evidence selections cannot be deleted';

CREATE TRIGGER tr_profile_artifact_industry_no_update
BEFORE UPDATE ON profile_artifact_industry
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'artifact industry selections are immutable';

CREATE TRIGGER tr_profile_artifact_industry_no_delete
BEFORE DELETE ON profile_artifact_industry
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'artifact industry selections cannot be deleted';

-- Existing characters pin the currently assigned complete bundle without changing their M0–M2 run.
INSERT INTO career_run
    (
        save_id,
        run_revision,
        career_catalog_bundle_id,
        focused_job_family_key,
        birth_date
    )
SELECT
    save.id,
    save.run_revision,
    assignment.career_catalog_bundle_id,
    bundle.default_focused_job_family_key,
    MAKEDATE(YEAR(market_world.start_date) - `character`.age, 1)
FROM save
INNER JOIN `character` ON `character`.save_id = save.id
INNER JOIN market_world ON market_world.id = save.market_world_id
INNER JOIN career_catalog_assignment AS assignment
    ON BINARY assignment.assignment_key = BINARY 'newRun'
INNER JOIN career_catalog_bundle AS bundle
    ON bundle.id = assignment.career_catalog_bundle_id
   AND bundle.published_at IS NOT NULL;

INSERT INTO spec_evidence
    (
        save_id,
        run_revision,
        career_catalog_bundle_id,
        evidence_key,
        spec_catalog_entry_id,
        kind,
        acquired_game_day,
        expires_on_game_day,
        period_start_date,
        period_end_exclusive_date,
        source_kind
    )
SELECT
    career_run.save_id,
    career_run.run_revision,
    career_run.career_catalog_bundle_id,
    bridge.evidence_key,
    bridge.spec_catalog_entry_id,
    'education',
    0,
    catalog.validity_days,
    NULL,
    NULL,
    'bridgeEducation'
FROM career_run
INNER JOIN `character` ON `character`.save_id = career_run.save_id
INNER JOIN career_bridge_education_mapping AS bridge
    ON bridge.career_catalog_bundle_id = career_run.career_catalog_bundle_id
   AND BINARY bridge.education = BINARY `character`.education
INNER JOIN spec_catalog_entry AS catalog
    ON catalog.career_catalog_bundle_id = bridge.career_catalog_bundle_id
   AND catalog.id = bridge.spec_catalog_entry_id;

INSERT INTO spec_evidence
    (
        save_id,
        run_revision,
        career_catalog_bundle_id,
        evidence_key,
        spec_catalog_entry_id,
        kind,
        acquired_game_day,
        expires_on_game_day,
        period_start_date,
        period_end_exclusive_date,
        source_kind
    )
SELECT
    career_run.save_id,
    career_run.run_revision,
    career_run.career_catalog_bundle_id,
    bridge.evidence_key,
    bridge.spec_catalog_entry_id,
    'certification',
    0,
    catalog.validity_days,
    NULL,
    NULL,
    'bridgeCertification'
FROM career_run
INNER JOIN `character` ON `character`.save_id = career_run.save_id
INNER JOIN career_bridge_certification_order AS bridge
    ON bridge.career_catalog_bundle_id = career_run.career_catalog_bundle_id
   AND bridge.certification_order <= `character`.certifications
INNER JOIN spec_catalog_entry AS catalog
    ON catalog.career_catalog_bundle_id = bridge.career_catalog_bundle_id
   AND catalog.id = bridge.spec_catalog_entry_id;

INSERT INTO spec_evidence
    (
        save_id,
        run_revision,
        career_catalog_bundle_id,
        evidence_key,
        spec_catalog_entry_id,
        kind,
        acquired_game_day,
        expires_on_game_day,
        period_start_date,
        period_end_exclusive_date,
        source_kind
    )
SELECT
    career_run.save_id,
    career_run.run_revision,
    career_run.career_catalog_bundle_id,
    bridge.evidence_key,
    bridge.spec_catalog_entry_id,
    'experience',
    0,
    catalog.validity_days,
    DATE_SUB(market_world.start_date, INTERVAL bridge.career_years YEAR),
    market_world.start_date,
    'bridgeExperience'
FROM career_run
INNER JOIN `character` ON `character`.save_id = career_run.save_id
INNER JOIN save ON save.id = career_run.save_id
INNER JOIN market_world ON market_world.id = save.market_world_id
INNER JOIN career_bridge_experience_mapping AS bridge
    ON bridge.career_catalog_bundle_id = career_run.career_catalog_bundle_id
   AND bridge.career_years = `character`.career_years
INNER JOIN spec_catalog_entry AS catalog
    ON catalog.career_catalog_bundle_id = bridge.career_catalog_bundle_id
   AND catalog.id = bridge.spec_catalog_entry_id;

-- Replace the allowlist atomically so existing postings remain valid throughout the DDL change.
ALTER TABLE ledger_posting
    DROP CHECK ck_ledger_posting_account_code,
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
            'careerDevelopmentExpense'
        )
    );
