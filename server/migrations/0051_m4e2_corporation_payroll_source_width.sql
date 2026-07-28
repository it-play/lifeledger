-- M4-E2 corporation officer payroll source discriminator width (§9.3).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;

ALTER TABLE employment_income_event
    MODIFY source_kind VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL;
