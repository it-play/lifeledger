-- M2-D source-level financial income and annual assessment lifecycle (§8.6, §11).
-- Historical financial_income_year rows remain aggregate-only: assigning an invented source
-- would corrupt their meaning. New M2-D writes maintain the aggregate and source rows together.

CREATE TABLE financial_income_source_year (
    save_id                         BIGINT UNSIGNED     NOT NULL,
    run_revision                    INT UNSIGNED        NOT NULL,
    tax_year                        SMALLINT UNSIGNED   NOT NULL,
    source                          VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    gross_income_krw                BIGINT              NOT NULL DEFAULT 0,
    withheld_income_tax_krw         BIGINT              NOT NULL DEFAULT 0,
    withheld_local_income_tax_krw   BIGINT              NOT NULL DEFAULT 0,
    created_at                      DATETIME(3)          NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                      DATETIME(3)          NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, run_revision, tax_year, source),
    CONSTRAINT fk_financial_income_source_year_parent
        FOREIGN KEY (save_id, run_revision, tax_year)
        REFERENCES financial_income_year (save_id, run_revision, tax_year) ON DELETE CASCADE,
    CONSTRAINT ck_financial_income_source_year_tax_year CHECK (tax_year BETWEEN 1 AND 9999),
    CONSTRAINT ck_financial_income_source_year_source CHECK (
        source IN (
            'cmaInterest',
            'depositInterest',
            'bondCoupon',
            'llxDistribution',
            'isaEarlyClose'
        )
    ),
    CONSTRAINT ck_financial_income_source_year_amounts CHECK (
        gross_income_krw >= 0
        AND withheld_income_tax_krw >= 0
        AND withheld_local_income_tax_krw >= 0
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE financial_income_assessment (
    save_id                              BIGINT UNSIGNED     NOT NULL,
    run_revision                         INT UNSIGNED        NOT NULL,
    tax_year                             SMALLINT UNSIGNED   NOT NULL,
    policy_set_id                        BIGINT UNSIGNED     NOT NULL,
    status                               VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    gross_financial_income_krw           BIGINT              NOT NULL DEFAULT 0,
    other_comprehensive_income_krw       BIGINT              NOT NULL DEFAULT 0,
    withheld_income_tax_krw              BIGINT              NOT NULL DEFAULT 0,
    withheld_local_income_tax_krw        BIGINT              NOT NULL DEFAULT 0,
    income_tax_formula_a_krw             BIGINT              NOT NULL DEFAULT 0,
    income_tax_formula_b_krw             BIGINT              NOT NULL DEFAULT 0,
    local_income_tax_formula_a_krw       BIGINT              NOT NULL DEFAULT 0,
    local_income_tax_formula_b_krw       BIGINT              NOT NULL DEFAULT 0,
    income_tax_credit_krw                BIGINT              NOT NULL DEFAULT 0,
    local_income_tax_credit_krw          BIGINT              NOT NULL DEFAULT 0,
    final_income_tax_krw                 BIGINT              NOT NULL DEFAULT 0,
    final_local_income_tax_krw           BIGINT              NOT NULL DEFAULT 0,
    additional_tax_krw                   BIGINT              NOT NULL DEFAULT 0,
    refund_krw                           BIGINT              NOT NULL DEFAULT 0,
    finalized_on                         DATE                    NULL,
    filing_date                          DATE                    NULL,
    filed_on                             DATE                    NULL,
    created_at                           DATETIME(3)          NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                           DATETIME(3)          NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, run_revision, tax_year),
    KEY ix_financial_income_assessment_status
        (save_id, run_revision, status, filing_date, tax_year),
    KEY ix_financial_income_assessment_policy_set (policy_set_id),
    CONSTRAINT fk_financial_income_assessment_parent
        FOREIGN KEY (save_id, run_revision, tax_year)
        REFERENCES financial_income_year (save_id, run_revision, tax_year) ON DELETE CASCADE,
    CONSTRAINT fk_financial_income_assessment_policy_set
        FOREIGN KEY (policy_set_id) REFERENCES policy_set (id),
    CONSTRAINT ck_financial_income_assessment_tax_year CHECK (tax_year BETWEEN 1 AND 9999),
    CONSTRAINT ck_financial_income_assessment_status CHECK (
        status IN ('open', 'finalizedNoFiling', 'filingPending', 'filed')
    ),
    CONSTRAINT ck_financial_income_assessment_amounts CHECK (
        gross_financial_income_krw >= 0
        AND other_comprehensive_income_krw >= 0
        AND withheld_income_tax_krw >= 0
        AND withheld_local_income_tax_krw >= 0
        AND income_tax_formula_a_krw >= 0
        AND income_tax_formula_b_krw >= 0
        AND local_income_tax_formula_a_krw >= 0
        AND local_income_tax_formula_b_krw >= 0
        AND income_tax_credit_krw >= 0
        AND local_income_tax_credit_krw >= 0
        AND final_income_tax_krw >= 0
        AND final_local_income_tax_krw >= 0
        AND additional_tax_krw >= 0
        AND refund_krw >= 0
        AND (additional_tax_krw = 0 OR refund_krw = 0)
        AND final_income_tax_krw
            <= GREATEST(income_tax_formula_a_krw, income_tax_formula_b_krw)
        AND final_local_income_tax_krw
            <= GREATEST(local_income_tax_formula_a_krw, local_income_tax_formula_b_krw)
    ),
    CONSTRAINT ck_financial_income_assessment_state_shape CHECK (
        (
            status = 'open'
            AND gross_financial_income_krw = 0
            AND other_comprehensive_income_krw = 0
            AND withheld_income_tax_krw = 0
            AND withheld_local_income_tax_krw = 0
            AND income_tax_formula_a_krw = 0
            AND income_tax_formula_b_krw = 0
            AND local_income_tax_formula_a_krw = 0
            AND local_income_tax_formula_b_krw = 0
            AND income_tax_credit_krw = 0
            AND local_income_tax_credit_krw = 0
            AND final_income_tax_krw = 0
            AND final_local_income_tax_krw = 0
            AND additional_tax_krw = 0
            AND refund_krw = 0
            AND finalized_on IS NULL
            AND filing_date IS NULL
            AND filed_on IS NULL
        )
        OR
        (
            status = 'finalizedNoFiling'
            AND income_tax_formula_a_krw = withheld_income_tax_krw
            AND income_tax_formula_b_krw = withheld_income_tax_krw
            AND final_income_tax_krw = withheld_income_tax_krw
            AND local_income_tax_formula_a_krw = withheld_local_income_tax_krw
            AND local_income_tax_formula_b_krw = withheld_local_income_tax_krw
            AND final_local_income_tax_krw = withheld_local_income_tax_krw
            AND additional_tax_krw = 0
            AND refund_krw = 0
            AND finalized_on IS NOT NULL
            AND YEAR(finalized_on) = tax_year + 1
            AND MONTH(finalized_on) = 1
            AND DAY(finalized_on) = 1
            AND filing_date IS NULL
            AND filed_on IS NULL
        )
        OR
        (
            status = 'filingPending'
            AND finalized_on IS NOT NULL
            AND YEAR(finalized_on) = tax_year + 1
            AND MONTH(finalized_on) = 1
            AND DAY(finalized_on) = 1
            AND filing_date IS NOT NULL
            AND YEAR(filing_date) = tax_year + 1
            AND filed_on IS NULL
        )
        OR
        (
            status = 'filed'
            AND finalized_on IS NOT NULL
            AND YEAR(finalized_on) = tax_year + 1
            AND MONTH(finalized_on) = 1
            AND DAY(finalized_on) = 1
            AND filing_date IS NOT NULL
            AND YEAR(filing_date) = tax_year + 1
            AND filed_on = filing_date
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

-- New annual rows belong to the current run at insertion time, then remain as run history.
CREATE TRIGGER tr_financial_income_year_current_run_insert
BEFORE INSERT ON financial_income_year
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM save
        WHERE id = NEW.save_id
          AND run_revision = NEW.run_revision
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_financial_income_source_year_current_run_insert
BEFORE INSERT ON financial_income_source_year
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM save
        WHERE id = NEW.save_id
          AND run_revision = NEW.run_revision
    )
    AND NOT EXISTS (
        SELECT 1
        FROM financial_income_assessment
        WHERE save_id = NEW.save_id
          AND run_revision = NEW.run_revision
          AND tax_year = NEW.tax_year
          AND status <> 'open'
    ),
    NEW.save_id,
    NULL
);

CREATE TRIGGER tr_financial_income_assessment_current_run_insert
BEFORE INSERT ON financial_income_assessment
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.status = 'open'
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

DROP TRIGGER tr_financial_income_year_identity_only;

-- Aggregate compatibility columns remain monotonic, but a finalized year is frozen.
CREATE TRIGGER tr_financial_income_year_identity_only
BEFORE UPDATE ON financial_income_year
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.tax_year = OLD.tax_year
        AND NEW.gross_financial_income_krw >= OLD.gross_financial_income_krw
        AND NEW.tax_exempt_financial_income_krw >= OLD.tax_exempt_financial_income_krw
        AND NEW.separate_tax_financial_income_krw >= OLD.separate_tax_financial_income_krw
        AND NEW.withheld_income_tax_krw >= OLD.withheld_income_tax_krw
        AND NEW.withheld_local_income_tax_krw >= OLD.withheld_local_income_tax_krw
        AND NEW.separate_withheld_income_tax_krw >= OLD.separate_withheld_income_tax_krw
        AND NEW.separate_withheld_local_income_tax_krw
            >= OLD.separate_withheld_local_income_tax_krw
        AND NEW.created_at = OLD.created_at
        AND NOT EXISTS (
            SELECT 1
            FROM financial_income_assessment
            WHERE save_id = OLD.save_id
              AND run_revision = OLD.run_revision
              AND tax_year = OLD.tax_year
              AND status <> 'open'
        ),
    OLD.save_id,
    NULL
);

CREATE TRIGGER tr_financial_income_source_year_monotonic_update
BEFORE UPDATE ON financial_income_source_year
FOR EACH ROW
SET NEW.save_id = IF(
    NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.tax_year = OLD.tax_year
        AND BINARY NEW.source = BINARY OLD.source
        AND NEW.gross_income_krw >= OLD.gross_income_krw
        AND NEW.withheld_income_tax_krw >= OLD.withheld_income_tax_krw
        AND NEW.withheld_local_income_tax_krw >= OLD.withheld_local_income_tax_krw
        AND NEW.created_at = OLD.created_at
        AND NOT EXISTS (
            SELECT 1
            FROM financial_income_assessment
            WHERE save_id = OLD.save_id
              AND run_revision = OLD.run_revision
              AND tax_year = OLD.tax_year
              AND status <> 'open'
        ),
    OLD.save_id,
    NULL
);

-- Finalization may write every calculated field once; filing may then change only its status/date.
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
            )
            OR
            (
                OLD.status = 'filingPending'
                AND NEW.status = 'filed'
                AND NEW.gross_financial_income_krw = OLD.gross_financial_income_krw
                AND NEW.other_comprehensive_income_krw = OLD.other_comprehensive_income_krw
                AND NEW.withheld_income_tax_krw = OLD.withheld_income_tax_krw
                AND NEW.withheld_local_income_tax_krw = OLD.withheld_local_income_tax_krw
                AND NEW.income_tax_formula_a_krw = OLD.income_tax_formula_a_krw
                AND NEW.income_tax_formula_b_krw = OLD.income_tax_formula_b_krw
                AND NEW.local_income_tax_formula_a_krw = OLD.local_income_tax_formula_a_krw
                AND NEW.local_income_tax_formula_b_krw = OLD.local_income_tax_formula_b_krw
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

CREATE TRIGGER tr_financial_income_source_year_no_delete
BEFORE DELETE ON financial_income_source_year
FOR EACH ROW
SIGNAL SQLSTATE '45000'
    SET MESSAGE_TEXT = 'financial income source years cannot be deleted';

CREATE TRIGGER tr_financial_income_assessment_no_delete
BEFORE DELETE ON financial_income_assessment
FOR EACH ROW
SIGNAL SQLSTATE '45000'
    SET MESSAGE_TEXT = 'financial income assessments cannot be deleted';
