-- M4-E2a corporation catalog, establishment policy, runtime, and separate ledger (§9.1–§9.2).

SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;
SET SESSION group_concat_max_len = 1048576;

INSERT INTO policy_source_document
    (source_key, source_url, checked_on, original_sha256)
VALUES
    (
        'law-local-tax-registration-article-28-2026-07-01',
        'https://law.go.kr/LSW/lsLinkCommonInfo.do?chrClsCd=010202&lsJoLnkSeq=1031543199',
        '2026-07-29',
        '85c2b878925ea3e78f0a13e579af10d94b28a0c0ce320fb224908a73fea11e3a'
    ),
    (
        'law-local-tax-education-article-151-2026-01-01',
        'https://www.law.go.kr/LSW/lsSideInfoP.do?docCls=jo&joNo=0151&lsiSeq=282559&urlMode=lsScJoRltInfoR',
        '2026-07-29',
        'deda164014c134a01750f6433eaf26d5f7a001aecb242d35474a6fb50533a151'
    ),
    (
        'nts-corporate-tax-rates-2026',
        'https://www.nts.go.kr/nts/cm/cntnts/cntntsView.do?cntntsId=7746&mi=2449',
        '2026-07-29',
        'e2974b42efabad2e2a9ba3ef20bd8409fdba0680aa8b1446a2524df01ac8a609'
    ),
    (
        'nts-dividend-withholding-rates-2026',
        'https://www.nts.go.kr/nts/cm/cntnts/cntntsView.do?cntntsId=7703&mi=2292',
        '2026-07-29',
        '00d43350c1fa12ed576af65b9f7f47a1ac16304f7510c79d99a2c8dbfe11e329'
    );

INSERT INTO policy_set (policy_key, basis_date, ranked_eligible)
VALUES ('dev-unranked-kr-corporation-2026-v5', '2026-07-29', FALSE);

INSERT INTO policy_rule
    (policy_set_id, domain, rule_key, effective_from, effective_to, parameters)
SELECT target.id, source_rule.domain, source_rule.rule_key,
       source_rule.effective_from, source_rule.effective_to, source_rule.parameters
FROM policy_set AS target
INNER JOIN run_rule_bundle_assignment AS assignment
    ON assignment.assignment_key = 'newRun'
INNER JOIN policy_rule AS source_rule
    ON source_rule.policy_set_id = assignment.policy_set_id
WHERE target.policy_key = 'dev-unranked-kr-corporation-2026-v5';

INSERT INTO policy_rule_clone_provenance
    (target_policy_rule_id, source_policy_rule_id, clone_kind)
SELECT target_rule.id,
       COALESCE(source_clone.source_policy_rule_id, source_rule.id),
       'sealedExactClone'
FROM policy_rule AS target_rule
INNER JOIN policy_set AS target_set
    ON target_set.id = target_rule.policy_set_id
INNER JOIN run_rule_bundle_assignment AS assignment
    ON assignment.assignment_key = 'newRun'
INNER JOIN policy_rule AS source_rule
    ON source_rule.policy_set_id = assignment.policy_set_id
   AND BINARY source_rule.domain = BINARY target_rule.domain
   AND BINARY source_rule.rule_key = BINARY target_rule.rule_key
   AND source_rule.effective_from = target_rule.effective_from
   AND source_rule.effective_to <=> target_rule.effective_to
LEFT JOIN policy_rule_clone_provenance AS source_clone
    ON source_clone.target_policy_rule_id = source_rule.id
WHERE target_set.policy_key = 'dev-unranked-kr-corporation-2026-v5';

INSERT INTO policy_rule_source
    (policy_rule_id, policy_source_document_id, citation_order)
SELECT target_rule.id, source_link.policy_source_document_id, source_link.citation_order
FROM policy_rule AS target_rule
INNER JOIN policy_set AS target_set ON target_set.id = target_rule.policy_set_id
INNER JOIN run_rule_bundle_assignment AS assignment ON assignment.assignment_key = 'newRun'
INNER JOIN policy_rule AS source_rule
    ON source_rule.policy_set_id = assignment.policy_set_id
   AND BINARY source_rule.domain = BINARY target_rule.domain
   AND BINARY source_rule.rule_key = BINARY target_rule.rule_key
   AND source_rule.effective_from = target_rule.effective_from
   AND source_rule.effective_to <=> target_rule.effective_to
INNER JOIN policy_rule_source AS source_link ON source_link.policy_rule_id = source_rule.id
WHERE target_set.policy_key = 'dev-unranked-kr-corporation-2026-v5';

INSERT INTO policy_rule
    (policy_set_id, domain, rule_key, effective_from, effective_to, parameters)
SELECT policy.id, 'corporation', 'standardRegistration', '2026-07-01', NULL,
       JSON_OBJECT(
           'localEducationTaxRatePpm', 200000,
           'minimumRegistrationLicenseTaxKrw', 112500,
           'registeredOfficeClass', 'standardRegisteredOffice',
           'registrationLicenseTaxRatePpm', 4000,
           'schemaVersion', 1,
           'unsupported', JSON_ARRAY(
               'overconcentrationSurcharge', 'taxReductionIndustry',
               'actualAddress', 'branchRegistration'
           )
       )
FROM policy_set AS policy
WHERE policy.policy_key = 'dev-unranked-kr-corporation-2026-v5';

INSERT INTO policy_rule
    (policy_set_id, domain, rule_key, effective_from, effective_to, parameters)
SELECT policy.id, 'corporation', 'corporateIncomeTax', '2026-01-01', NULL,
       JSON_OBJECT(
           'brackets', JSON_ARRAY(
               JSON_OBJECT('maximumTaxBaseKrw', 200000000, 'ratePpm', 100000, 'progressiveDeductionKrw', 0),
               JSON_OBJECT('maximumTaxBaseKrw', 20000000000, 'ratePpm', 200000, 'progressiveDeductionKrw', 20000000),
               JSON_OBJECT('maximumTaxBaseKrw', 300000000000, 'ratePpm', 220000, 'progressiveDeductionKrw', 420000000),
               JSON_OBJECT('maximumTaxBaseKrw', NULL, 'ratePpm', 250000, 'progressiveDeductionKrw', 9420000000)
           ),
           'fiscalYearEndMonth', 12,
           'schemaVersion', 1,
           'supportedEntityKind', 'domesticForProfitCorporation'
       )
FROM policy_set AS policy
WHERE policy.policy_key = 'dev-unranked-kr-corporation-2026-v5';

INSERT INTO policy_rule
    (policy_set_id, domain, rule_key, effective_from, effective_to, parameters)
SELECT policy.id, 'corporation', 'residentDividendWithholding', '2026-01-01', NULL,
       JSON_OBJECT(
           'incomeTaxRatePpm', 140000,
           'localIncomeTaxOnIncomeTaxPpm', 100000,
           'rounding', 'floorEachTax',
           'schemaVersion', 1,
           'supportedRecipient', 'residentIndividual'
       )
FROM policy_set AS policy
WHERE policy.policy_key = 'dev-unranked-kr-corporation-2026-v5';

INSERT INTO policy_rule_source
    (policy_rule_id, policy_source_document_id, citation_order)
SELECT rule.id, source.id, source_order.citation_order
FROM policy_rule AS rule
INNER JOIN policy_set AS policy ON policy.id = rule.policy_set_id
INNER JOIN (
    SELECT 'standardRegistration' AS rule_key,
           'law-local-tax-registration-article-28-2026-07-01' AS source_key,
           1 AS citation_order
    UNION ALL
    SELECT 'standardRegistration',
           'law-local-tax-education-article-151-2026-01-01', 2
    UNION ALL
    SELECT 'corporateIncomeTax', 'nts-corporate-tax-rates-2026', 1
    UNION ALL
    SELECT 'residentDividendWithholding', 'nts-dividend-withholding-rates-2026', 1
) AS source_order ON BINARY source_order.rule_key = BINARY rule.rule_key
INNER JOIN policy_source_document AS source ON source.source_key = source_order.source_key
WHERE policy.policy_key = 'dev-unranked-kr-corporation-2026-v5'
  AND rule.domain = 'corporation';

INSERT INTO policy_set_canonical_manifest (policy_set_id, canonical_json)
SELECT policy.id,
       CONCAT(
           '{"basisDate":', JSON_QUOTE(DATE_FORMAT(policy.basis_date, '%Y-%m-%d')),
           ',"policyKey":', JSON_QUOTE(policy.policy_key),
           ',"rankedEligible":', IF(policy.ranked_eligible, 'true', 'false'),
           ',"rules":[',
           COALESCE((
               SELECT GROUP_CONCAT(
                   CAST(JSON_OBJECT(
                       'domain', rule.domain,
                       'effectiveFrom', DATE_FORMAT(rule.effective_from, '%Y-%m-%d'),
                       'effectiveTo', IF(
                           rule.effective_to IS NULL,
                           NULL,
                           DATE_FORMAT(rule.effective_to, '%Y-%m-%d')
                       ),
                       'parameters', rule.parameters,
                       'ruleId', CAST(rule.id AS CHAR),
                       'ruleKey', rule.rule_key
                   ) AS CHAR CHARACTER SET utf8mb4)
                   ORDER BY rule.domain, rule.rule_key, rule.effective_from, rule.id
                   SEPARATOR ','
               )
               FROM policy_rule AS rule
               WHERE rule.policy_set_id = policy.id
           ), ''),
           '],"schemaVersion":1}'
       )
FROM policy_set AS policy
WHERE policy.policy_key = 'dev-unranked-kr-corporation-2026-v5';

UPDATE policy_set AS policy
INNER JOIN policy_set_canonical_manifest AS manifest ON manifest.policy_set_id = policy.id
SET policy.canonical_sha256 = manifest.canonical_sha256,
    policy.sealed_at = CURRENT_TIMESTAMP(3)
WHERE policy.policy_key = 'dev-unranked-kr-corporation-2026-v5'
  AND policy.sealed_at IS NULL;

INSERT INTO life_component_version
    (component_kind, version_key, availability, ranked_eligible)
VALUES ('corporation', 'dev-unranked-m4-corporation-2026-v1', 'active', FALSE);

CREATE TABLE corporation_component_profile (
    life_component_version_id       BIGINT UNSIGNED NOT NULL,
    registered_office_class         VARCHAR(40) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    minimum_capital_krw             BIGINT NOT NULL,
    maximum_capital_krw             BIGINT NOT NULL,
    game_administrative_fee_krw     BIGINT NOT NULL,
    maximum_corporations_per_run    TINYINT UNSIGNED NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (life_component_version_id),
    CONSTRAINT fk_corporation_component_profile_version
        FOREIGN KEY (life_component_version_id) REFERENCES life_component_version (id),
    CONSTRAINT ck_corporation_component_profile CHECK (
        registered_office_class = 'standardRegisteredOffice'
        AND minimum_capital_krw = 1000000
        AND maximum_capital_krw = 1000000000
        AND game_administrative_fee_krw = 30000
        AND maximum_corporations_per_run = 1
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE corporation_industry_template (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    life_component_version_id       BIGINT UNSIGNED NOT NULL,
    template_key                    VARCHAR(40) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                    VARCHAR(80) NOT NULL,
    template_order                  TINYINT UNSIGNED NOT NULL,
    base_monthly_revenue_krw        BIGINT NOT NULL,
    revenue_variation_ppm           INT UNSIGNED NOT NULL,
    variable_cost_ppm               INT UNSIGNED NOT NULL,
    fixed_monthly_cost_krw          BIGINT NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_corporation_template_key (life_component_version_id, template_key),
    UNIQUE KEY uk_corporation_template_order (life_component_version_id, template_order),
    UNIQUE KEY uk_corporation_template_version_id (life_component_version_id, id),
    CONSTRAINT fk_corporation_template_version
        FOREIGN KEY (life_component_version_id) REFERENCES life_component_version (id),
    CONSTRAINT ck_corporation_template_key CHECK (
        template_key IN ('softwareService', 'onlineRetail', 'contentStudio')
    ),
    CONSTRAINT ck_corporation_template_terms CHECK (
        template_order BETWEEN 1 AND 3
        AND CHAR_LENGTH(display_name) BETWEEN 1 AND 80
        AND base_monthly_revenue_krw BETWEEN 1 AND 9007199254740991
        AND revenue_variation_ppm BETWEEN 0 AND 900000
        AND variable_cost_ppm BETWEEN 0 AND 1000000
        AND fixed_monthly_cost_krw BETWEEN 0 AND 9007199254740991
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE corporation_operating_scale (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    life_component_version_id       BIGINT UNSIGNED NOT NULL,
    industry_template_id            BIGINT UNSIGNED NOT NULL,
    scale_key                       VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    scale_order                     TINYINT UNSIGNED NOT NULL,
    revenue_factor_ppm              INT UNSIGNED NOT NULL,
    fixed_cost_krw                  BIGINT NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_corporation_scale_key (industry_template_id, scale_key),
    UNIQUE KEY uk_corporation_scale_order (industry_template_id, scale_order),
    CONSTRAINT fk_corporation_scale_template
        FOREIGN KEY (life_component_version_id, industry_template_id)
        REFERENCES corporation_industry_template (life_component_version_id, id),
    CONSTRAINT ck_corporation_scale CHECK (
        scale_key IN ('lean', 'standard', 'growth')
        AND scale_order BETWEEN 1 AND 3
        AND revenue_factor_ppm BETWEEN 1 AND 3000000
        AND fixed_cost_krw BETWEEN 0 AND 9007199254740991
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_corporation_component_profile_draft_insert
BEFORE INSERT ON corporation_component_profile
FOR EACH ROW
SET NEW.life_component_version_id = IF(
    EXISTS (
        SELECT 1 FROM life_component_version AS component
        WHERE component.id = NEW.life_component_version_id
          AND component.component_kind = 'corporation'
          AND component.sealed_at IS NULL
    ),
    NEW.life_component_version_id,
    NULL
);

CREATE TRIGGER tr_corporation_template_draft_insert
BEFORE INSERT ON corporation_industry_template
FOR EACH ROW
SET NEW.life_component_version_id = IF(
    EXISTS (
        SELECT 1 FROM life_component_version AS component
        WHERE component.id = NEW.life_component_version_id
          AND component.component_kind = 'corporation'
          AND component.sealed_at IS NULL
    ),
    NEW.life_component_version_id,
    NULL
);

CREATE TRIGGER tr_corporation_scale_draft_insert
BEFORE INSERT ON corporation_operating_scale
FOR EACH ROW
SET NEW.life_component_version_id = IF(
    EXISTS (
        SELECT 1 FROM life_component_version AS component
        WHERE component.id = NEW.life_component_version_id
          AND component.component_kind = 'corporation'
          AND component.sealed_at IS NULL
    ),
    NEW.life_component_version_id,
    NULL
);

CREATE TRIGGER tr_corporation_component_profile_no_update
BEFORE UPDATE ON corporation_component_profile
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation component profiles are immutable';

CREATE TRIGGER tr_corporation_component_profile_no_delete
BEFORE DELETE ON corporation_component_profile
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation component profiles are immutable';

CREATE TRIGGER tr_corporation_template_no_update
BEFORE UPDATE ON corporation_industry_template
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation industry templates are immutable';

CREATE TRIGGER tr_corporation_template_no_delete
BEFORE DELETE ON corporation_industry_template
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation industry templates are immutable';

CREATE TRIGGER tr_corporation_scale_no_update
BEFORE UPDATE ON corporation_operating_scale
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation operating scales are immutable';

CREATE TRIGGER tr_corporation_scale_no_delete
BEFORE DELETE ON corporation_operating_scale
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation operating scales are immutable';

INSERT INTO corporation_component_profile
    (
        life_component_version_id, registered_office_class,
        minimum_capital_krw, maximum_capital_krw,
        game_administrative_fee_krw, maximum_corporations_per_run
    )
SELECT id, 'standardRegisteredOffice', 1000000, 1000000000, 30000, 1
FROM life_component_version
WHERE component_kind = 'corporation'
  AND version_key = 'dev-unranked-m4-corporation-2026-v1';

INSERT INTO corporation_industry_template
    (
        life_component_version_id, template_key, display_name, template_order,
        base_monthly_revenue_krw, revenue_variation_ppm,
        variable_cost_ppm, fixed_monthly_cost_krw
    )
SELECT component.id, seed.template_key, seed.display_name, seed.template_order,
       seed.base_monthly_revenue_krw, seed.revenue_variation_ppm,
       seed.variable_cost_ppm, seed.fixed_monthly_cost_krw
FROM life_component_version AS component
INNER JOIN (
    SELECT 'softwareService' AS template_key, '소프트웨어 서비스' AS display_name,
           1 AS template_order, 8000000 AS base_monthly_revenue_krw,
           350000 AS revenue_variation_ppm, 120000 AS variable_cost_ppm,
           2400000 AS fixed_monthly_cost_krw
    UNION ALL
    SELECT 'onlineRetail', '온라인 소매', 2, 18000000, 450000, 620000, 3000000
    UNION ALL
    SELECT 'contentStudio', '콘텐츠 스튜디오', 3, 10000000, 550000, 250000, 2000000
) AS seed
WHERE component.component_kind = 'corporation'
  AND component.version_key = 'dev-unranked-m4-corporation-2026-v1';

INSERT INTO corporation_operating_scale
    (
        life_component_version_id, industry_template_id,
        scale_key, scale_order, revenue_factor_ppm, fixed_cost_krw
    )
SELECT template.life_component_version_id, template.id,
       seed.scale_key, seed.scale_order, seed.revenue_factor_ppm, seed.fixed_cost_krw
FROM corporation_industry_template AS template
INNER JOIN (
    SELECT 'lean' AS scale_key, 1 AS scale_order, 750000 AS revenue_factor_ppm, 600000 AS fixed_cost_krw
    UNION ALL SELECT 'standard', 2, 1000000, 2000000
    UNION ALL SELECT 'growth', 3, 1350000, 5000000
) AS seed
WHERE template.life_component_version_id = (
    SELECT id FROM life_component_version
    WHERE component_kind = 'corporation'
      AND version_key = 'dev-unranked-m4-corporation-2026-v1'
);

INSERT INTO life_component_canonical_manifest
    (life_component_version_id, canonical_json)
SELECT component.id,
       CONCAT(
           '{"availability":', JSON_QUOTE(component.availability),
           ',"componentKind":"corporation"',
           ',"profile":', CAST(JSON_OBJECT(
               'gameAdministrativeFeeKrw', profile.game_administrative_fee_krw,
               'maximumCapitalKrw', profile.maximum_capital_krw,
               'maximumCorporationsPerRun', profile.maximum_corporations_per_run,
               'minimumCapitalKrw', profile.minimum_capital_krw,
               'registeredOfficeClass', profile.registered_office_class
           ) AS CHAR CHARACTER SET utf8mb4),
           ',"rankedEligible":', IF(component.ranked_eligible, 'true', 'false'),
           ',"schemaVersion":1',
           ',"templates":[',
           COALESCE((
               SELECT GROUP_CONCAT(
                   CAST(JSON_OBJECT(
                       'baseMonthlyRevenueKrw', template.base_monthly_revenue_krw,
                       'displayName', template.display_name,
                       'fixedMonthlyCostKrw', template.fixed_monthly_cost_krw,
                       'revenueVariationPpm', template.revenue_variation_ppm,
                       'templateKey', template.template_key,
                       'templateOrder', template.template_order,
                       'variableCostPpm', template.variable_cost_ppm
                   ) AS CHAR CHARACTER SET utf8mb4)
                   ORDER BY template.template_order SEPARATOR ','
               )
               FROM corporation_industry_template AS template
               WHERE template.life_component_version_id = component.id
           ), ''),
           '],"operatingScales":[',
           COALESCE((
               SELECT GROUP_CONCAT(
                   CAST(JSON_OBJECT(
                       'fixedCostKrw', scale.fixed_cost_krw,
                       'revenueFactorPpm', scale.revenue_factor_ppm,
                       'scaleKey', scale.scale_key,
                       'scaleOrder', scale.scale_order,
                       'templateKey', template.template_key
                   ) AS CHAR CHARACTER SET utf8mb4)
                   ORDER BY template.template_order, scale.scale_order SEPARATOR ','
               )
               FROM corporation_operating_scale AS scale
               INNER JOIN corporation_industry_template AS template
                   ON template.id = scale.industry_template_id
               WHERE scale.life_component_version_id = component.id
           ), ''),
           '],"versionKey":', JSON_QUOTE(component.version_key), '}'
       )
FROM life_component_version AS component
INNER JOIN corporation_component_profile AS profile
    ON profile.life_component_version_id = component.id
WHERE component.component_kind = 'corporation'
  AND component.version_key = 'dev-unranked-m4-corporation-2026-v1';

UPDATE life_component_version AS component
INNER JOIN life_component_canonical_manifest AS manifest
    ON manifest.life_component_version_id = component.id
SET component.canonical_sha256 = manifest.canonical_sha256,
    component.sealed_at = CURRENT_TIMESTAMP(3)
WHERE component.component_kind = 'corporation'
  AND component.version_key = 'dev-unranked-m4-corporation-2026-v1'
  AND component.sealed_at IS NULL;

INSERT INTO life_catalog_set
    (
        catalog_key, ranked_eligible, legacy_dependent_age_years,
        living_cost_component_version_id, welfare_component_version_id,
        life_event_component_version_id, insurance_component_version_id,
        insolvency_component_version_id, corporation_component_version_id
    )
SELECT 'dev-unranked-m4-life-corporation-2026-v6', FALSE,
       current_catalog.legacy_dependent_age_years,
       current_catalog.living_cost_component_version_id,
       current_catalog.welfare_component_version_id,
       current_catalog.life_event_component_version_id,
       current_catalog.insurance_component_version_id,
       current_catalog.insolvency_component_version_id,
       corporation.id
FROM run_rule_bundle_assignment AS assignment
INNER JOIN life_catalog_set AS current_catalog ON current_catalog.id = assignment.life_catalog_set_id
INNER JOIN life_component_version AS corporation
    ON corporation.component_kind = 'corporation'
   AND corporation.version_key = 'dev-unranked-m4-corporation-2026-v1'
   AND corporation.sealed_at IS NOT NULL
WHERE assignment.assignment_key = 'newRun';

UPDATE life_catalog_set
SET canonical_sha256 = SHA2(
        CAST(JSON_OBJECT(
            'catalogKey', catalog_key,
            'corporationComponentVersionId', CAST(corporation_component_version_id AS CHAR),
            'insolvencyComponentVersionId', CAST(insolvency_component_version_id AS CHAR),
            'insuranceComponentVersionId', CAST(insurance_component_version_id AS CHAR),
            'lifeEventComponentVersionId', CAST(life_event_component_version_id AS CHAR),
            'legacyDependentAgeYears', legacy_dependent_age_years,
            'livingCostComponentVersionId', CAST(living_cost_component_version_id AS CHAR),
            'schemaVersion', 2,
            'welfareComponentVersionId', CAST(welfare_component_version_id AS CHAR)
        ) AS CHAR CHARACTER SET utf8mb4),
        256
    ),
    sealed_at = CURRENT_TIMESTAMP(3)
WHERE catalog_key = 'dev-unranked-m4-life-corporation-2026-v6'
  AND sealed_at IS NULL;

ALTER TABLE life_catalog_set
    ADD UNIQUE KEY uk_life_catalog_corporation_component
        (id, corporation_component_version_id);

CREATE TABLE corporation (
    id                                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                             BIGINT UNSIGNED NOT NULL,
    run_revision                        INT UNSIGNED NOT NULL,
    life_catalog_set_id                 BIGINT UNSIGNED NOT NULL,
    policy_set_id                       BIGINT UNSIGNED NOT NULL,
    corporation_component_version_id    BIGINT UNSIGNED NOT NULL,
    industry_template_id                BIGINT UNSIGNED NOT NULL,
    registration_policy_rule_id         BIGINT UNSIGNED NOT NULL,
    name                                VARCHAR(40) NOT NULL,
    representative_name                 VARCHAR(20) NOT NULL,
    status                              VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    registered_office_class             VARCHAR(40) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    establishment_command_id            CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    established_game_day                INT UNSIGNED NOT NULL,
    capital_krw                         BIGINT NOT NULL,
    registration_license_tax_krw        BIGINT NOT NULL,
    local_education_tax_krw             BIGINT NOT NULL,
    game_administrative_fee_krw         BIGINT NOT NULL,
    total_establishment_fee_krw         BIGINT NOT NULL,
    cash_krw                            BIGINT NOT NULL,
    contributed_capital_krw             BIGINT NOT NULL,
    retained_earnings_krw               BIGINT NOT NULL DEFAULT 0,
    operating_payable_krw               BIGINT NOT NULL DEFAULT 0,
    distributable_profit_krw            BIGINT NOT NULL DEFAULT 0,
    personal_ledger_transaction_id      BIGINT UNSIGNED NULL,
    corporation_ledger_transaction_id   BIGINT UNSIGNED NULL,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_corporation_save_run (save_id, run_revision),
    UNIQUE KEY uk_corporation_save_run_id (save_id, run_revision, id),
    UNIQUE KEY uk_corporation_establishment_command (save_id, establishment_command_id),
    UNIQUE KEY uk_corporation_personal_ledger (personal_ledger_transaction_id),
    CONSTRAINT fk_corporation_bundle
        FOREIGN KEY (save_id, run_revision, life_catalog_set_id)
        REFERENCES run_rule_bundle (save_id, run_revision, life_catalog_set_id),
    CONSTRAINT fk_corporation_catalog_component
        FOREIGN KEY (life_catalog_set_id, corporation_component_version_id)
        REFERENCES life_catalog_set (id, corporation_component_version_id),
    CONSTRAINT fk_corporation_template
        FOREIGN KEY (corporation_component_version_id, industry_template_id)
        REFERENCES corporation_industry_template (life_component_version_id, id),
    CONSTRAINT fk_corporation_policy FOREIGN KEY (policy_set_id) REFERENCES policy_set (id),
    CONSTRAINT fk_corporation_registration_rule
        FOREIGN KEY (registration_policy_rule_id) REFERENCES policy_rule (id),
    CONSTRAINT fk_corporation_command
        FOREIGN KEY (save_id, establishment_command_id)
        REFERENCES command_identity (save_id, command_id),
    CONSTRAINT ck_corporation_name CHECK (
        CHAR_LENGTH(name) BETWEEN 2 AND 40 AND name = TRIM(name)
    ),
    CONSTRAINT ck_corporation_status CHECK (
        status IN ('draft', 'active', 'dormant', 'insolvent', 'dissolved')
    ),
    CONSTRAINT ck_corporation_establishment CHECK (
        registered_office_class = 'standardRegisteredOffice'
        AND capital_krw BETWEEN 1000000 AND 1000000000
        AND registration_license_tax_krw > 0
        AND local_education_tax_krw >= 0
        AND game_administrative_fee_krw >= 0
        AND total_establishment_fee_krw
            = registration_license_tax_krw + local_education_tax_krw + game_administrative_fee_krw
        AND cash_krw BETWEEN 0 AND 9007199254740991
        AND contributed_capital_krw = capital_krw
        AND retained_earnings_krw BETWEEN -9007199254740991 AND 9007199254740991
        AND operating_payable_krw BETWEEN 0 AND 9007199254740991
        AND distributable_profit_krw BETWEEN 0 AND 9007199254740991
        AND (
            (status = 'draft'
             AND personal_ledger_transaction_id IS NULL
             AND corporation_ledger_transaction_id IS NULL)
            OR
            (status <> 'draft'
             AND personal_ledger_transaction_id IS NOT NULL
             AND corporation_ledger_transaction_id IS NOT NULL)
        )
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE corporation_transition (
    id                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id             BIGINT UNSIGNED NOT NULL,
    run_revision        INT UNSIGNED NOT NULL,
    corporation_id      BIGINT UNSIGNED NOT NULL,
    transition_no       SMALLINT UNSIGNED NOT NULL,
    from_status         VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL,
    to_status           VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    command_id          CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    transition_game_day INT UNSIGNED NOT NULL,
    transition_reason   VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_corporation_transition_no
        (save_id, run_revision, corporation_id, transition_no),
    UNIQUE KEY uk_corporation_transition_status
        (save_id, run_revision, corporation_id, to_status),
    CONSTRAINT fk_corporation_transition_corporation
        FOREIGN KEY (save_id, run_revision, corporation_id)
        REFERENCES corporation (save_id, run_revision, id),
    CONSTRAINT fk_corporation_transition_command
        FOREIGN KEY (save_id, command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT ck_corporation_transition CHECK (
        transition_no BETWEEN 1 AND 64
        AND to_status IN ('draft', 'active', 'dormant', 'insolvent', 'dissolved')
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE corporation_ledger_transaction (
    id                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id             BIGINT UNSIGNED NOT NULL,
    run_revision        INT UNSIGNED NOT NULL,
    corporation_id      BIGINT UNSIGNED NOT NULL,
    game_day            INT UNSIGNED NOT NULL,
    transaction_kind    VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    correlation_id      CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    description         VARCHAR(255) NOT NULL,
    created_at          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_corporation_ledger_scope
        (save_id, run_revision, corporation_id, id),
    UNIQUE KEY uk_corporation_ledger_correlation
        (save_id, run_revision, corporation_id, transaction_kind, correlation_id),
    CONSTRAINT fk_corporation_ledger_corporation
        FOREIGN KEY (save_id, run_revision, corporation_id)
        REFERENCES corporation (save_id, run_revision, id),
    CONSTRAINT fk_corporation_ledger_command
        FOREIGN KEY (save_id, correlation_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT ck_corporation_ledger_transaction CHECK (
        transaction_kind IN (
            'establishment', 'monthlyRevenue', 'monthlyExpense',
            'officerPayroll', 'corporateTax', 'dividend'
        )
        AND CHAR_LENGTH(description) BETWEEN 1 AND 255
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

ALTER TABLE corporation
    ADD CONSTRAINT fk_corporation_personal_ledger
        FOREIGN KEY (save_id, run_revision, personal_ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    ADD CONSTRAINT fk_corporation_corporation_ledger
        FOREIGN KEY (save_id, run_revision, id, corporation_ledger_transaction_id)
        REFERENCES corporation_ledger_transaction (save_id, run_revision, corporation_id, id);

CREATE TABLE corporation_ledger_posting (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    save_id                         BIGINT UNSIGNED NOT NULL,
    run_revision                    INT UNSIGNED NOT NULL,
    corporation_id                  BIGINT UNSIGNED NOT NULL,
    corporation_ledger_transaction_id BIGINT UNSIGNED NOT NULL,
    posting_order                   SMALLINT UNSIGNED NOT NULL,
    account_code                    VARCHAR(40) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    amount_krw                      BIGINT NOT NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_corporation_ledger_posting_order
        (corporation_ledger_transaction_id, posting_order),
    CONSTRAINT fk_corporation_ledger_posting_transaction
        FOREIGN KEY (
            save_id, run_revision, corporation_id, corporation_ledger_transaction_id
        ) REFERENCES corporation_ledger_transaction (
            save_id, run_revision, corporation_id, id
        ),
    CONSTRAINT ck_corporation_ledger_posting CHECK (
        posting_order BETWEEN 1 AND 64
        AND account_code IN (
            'corporationCash', 'contributedCapital', 'operatingRevenue',
            'variableCostExpense', 'fixedCostExpense', 'officerPayrollExpense',
            'withholdingTaxLiability', 'operatingPayable', 'corporateTaxExpense',
            'corporateTaxPayable', 'retainedEarnings', 'dividendDistribution'
        )
        AND amount_krw <> 0
        AND amount_krw BETWEEN -9007199254740991 AND 9007199254740991
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE corporation_command_receipt (
    save_id                             BIGINT UNSIGNED NOT NULL,
    command_id                          CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    run_revision                        INT UNSIGNED NOT NULL,
    state_revision                      BIGINT UNSIGNED NOT NULL,
    game_day                            INT UNSIGNED NOT NULL,
    corporation_id                      BIGINT UNSIGNED NOT NULL,
    command_kind                        VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    payload_sha256                      CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    personal_ledger_transaction_id      BIGINT UNSIGNED NOT NULL,
    corporation_ledger_transaction_id   BIGINT UNSIGNED NOT NULL,
    result                              JSON NOT NULL,
    created_at                          DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (save_id, command_id),
    UNIQUE KEY uk_corporation_receipt_corporation_command
        (save_id, run_revision, corporation_id, command_kind),
    CONSTRAINT fk_corporation_receipt_command
        FOREIGN KEY (save_id, command_id) REFERENCES command_identity (save_id, command_id),
    CONSTRAINT fk_corporation_receipt_corporation
        FOREIGN KEY (save_id, run_revision, corporation_id)
        REFERENCES corporation (save_id, run_revision, id),
    CONSTRAINT fk_corporation_receipt_personal_ledger
        FOREIGN KEY (save_id, run_revision, personal_ledger_transaction_id)
        REFERENCES ledger_transaction (save_id, run_revision, id),
    CONSTRAINT fk_corporation_receipt_corporation_ledger
        FOREIGN KEY (
            save_id, run_revision, corporation_id, corporation_ledger_transaction_id
        ) REFERENCES corporation_ledger_transaction (
            save_id, run_revision, corporation_id, id
        ),
    CONSTRAINT ck_corporation_receipt CHECK (
        command_kind = 'createCorporation'
        AND payload_sha256 REGEXP '^[0-9a-f]{64}$'
        AND JSON_TYPE(result) = 'OBJECT'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_command_identity_corporation_valid_insert
BEFORE INSERT ON command_identity
FOR EACH ROW
FOLLOWS tr_command_identity_m4c4_valid_insert
SET NEW.command_kind = IF(
    NEW.command_kind <> 'createCorporation'
        OR EXISTS (
            SELECT 1 FROM save
            WHERE save.id = NEW.save_id
              AND save.run_revision = NEW.initial_run_revision
              AND save.state_revision = NEW.initial_state_revision
              AND save.game_day = NEW.initial_game_day
        ),
    NEW.command_kind,
    NULL
);

CREATE TRIGGER tr_corporation_draft_insert_valid
BEFORE INSERT ON corporation
FOR EACH ROW
SET NEW.status = IF(
    NEW.status = 'draft'
        AND NEW.personal_ledger_transaction_id IS NULL
        AND NEW.corporation_ledger_transaction_id IS NULL
        AND NEW.cash_krw = NEW.capital_krw
        AND NEW.contributed_capital_krw = NEW.capital_krw
        AND NEW.retained_earnings_krw = 0
        AND NEW.operating_payable_krw = 0
        AND NEW.distributable_profit_krw = 0
        AND NOT EXISTS (
            SELECT 1 FROM insolvency_case AS case_row
            WHERE case_row.save_id = NEW.save_id
              AND case_row.run_revision = NEW.run_revision
              AND case_row.status IN (
                  'prepared', 'filed', 'liquidation', 'discharged', 'rebuilding'
              )
        )
        AND EXISTS (
            SELECT 1
            FROM run_rule_bundle AS bundle
            INNER JOIN life_catalog_set AS catalog
                ON catalog.id = bundle.life_catalog_set_id
               AND catalog.id = NEW.life_catalog_set_id
               AND catalog.corporation_component_version_id
                    = NEW.corporation_component_version_id
            INNER JOIN market_world AS world
                ON world.id = bundle.market_world_id
            INNER JOIN save
                ON save.id = NEW.save_id
               AND save.run_revision = NEW.run_revision
               AND save.game_day = NEW.established_game_day
            INNER JOIN `character`
                ON `character`.save_id = save.id
               AND BINARY `character`.name = BINARY NEW.representative_name
            INNER JOIN command_identity AS identity
                ON identity.save_id = NEW.save_id
               AND BINARY identity.command_id = BINARY NEW.establishment_command_id
               AND identity.command_kind = 'createCorporation'
               AND identity.initial_run_revision = NEW.run_revision
               AND identity.initial_state_revision = save.state_revision
               AND identity.initial_game_day = NEW.established_game_day
            INNER JOIN life_component_version AS component
                ON component.id = NEW.corporation_component_version_id
               AND component.component_kind = 'corporation'
               AND component.version_key = 'dev-unranked-m4-corporation-2026-v1'
               AND component.availability = 'active'
               AND component.sealed_at IS NOT NULL
            INNER JOIN corporation_component_profile AS profile
                ON profile.life_component_version_id = component.id
               AND profile.registered_office_class = NEW.registered_office_class
               AND profile.game_administrative_fee_krw = NEW.game_administrative_fee_krw
               AND NEW.capital_krw BETWEEN profile.minimum_capital_krw
                                       AND profile.maximum_capital_krw
            INNER JOIN corporation_industry_template AS template
                ON template.id = NEW.industry_template_id
               AND template.life_component_version_id = component.id
            INNER JOIN policy_rule AS registration_rule
                ON registration_rule.id = NEW.registration_policy_rule_id
               AND registration_rule.policy_set_id = NEW.policy_set_id
               AND registration_rule.domain = 'corporation'
               AND registration_rule.rule_key = 'standardRegistration'
               AND registration_rule.effective_from
                    <= DATE_ADD(world.start_date, INTERVAL NEW.established_game_day DAY)
               AND (
                   registration_rule.effective_to IS NULL
                   OR registration_rule.effective_to
                        > DATE_ADD(world.start_date, INTERVAL NEW.established_game_day DAY)
               )
            WHERE bundle.save_id = NEW.save_id
              AND bundle.run_revision = NEW.run_revision
              AND bundle.policy_set_id = NEW.policy_set_id
              AND NEW.registration_license_tax_krw = GREATEST(
                  FLOOR(
                      NEW.capital_krw
                      * CAST(JSON_UNQUOTE(JSON_EXTRACT(
                          registration_rule.parameters,
                          '$.registrationLicenseTaxRatePpm'
                      )) AS UNSIGNED)
                      / 1000000
                  ),
                  CAST(JSON_UNQUOTE(JSON_EXTRACT(
                      registration_rule.parameters,
                      '$.minimumRegistrationLicenseTaxKrw'
                  )) AS UNSIGNED)
              )
              AND NEW.local_education_tax_krw = FLOOR(
                  NEW.registration_license_tax_krw
                  * CAST(JSON_UNQUOTE(JSON_EXTRACT(
                      registration_rule.parameters,
                      '$.localEducationTaxRatePpm'
                  )) AS UNSIGNED)
                  / 1000000
              )
              AND (
                  SELECT COUNT(*)
                  FROM policy_rule AS corporation_rule
                  WHERE corporation_rule.policy_set_id = NEW.policy_set_id
                    AND corporation_rule.domain = 'corporation'
                    AND corporation_rule.rule_key IN (
                        'standardRegistration',
                        'corporateIncomeTax',
                        'residentDividendWithholding'
                    )
                    AND corporation_rule.effective_from
                          <= DATE_ADD(world.start_date, INTERVAL NEW.established_game_day DAY)
                    AND (
                        corporation_rule.effective_to IS NULL
                        OR corporation_rule.effective_to
                              > DATE_ADD(world.start_date, INTERVAL NEW.established_game_day DAY)
                    )
              ) = 3
        ),
    NEW.status,
    NULL
);

CREATE TRIGGER tr_corporation_ledger_transaction_insert_valid
BEFORE INSERT ON corporation_ledger_transaction
FOR EACH ROW
SET NEW.transaction_kind = IF(
    NEW.transaction_kind = 'establishment'
        AND EXISTS (
            SELECT 1 FROM corporation AS corporation_row
            WHERE corporation_row.id = NEW.corporation_id
              AND corporation_row.save_id = NEW.save_id
              AND corporation_row.run_revision = NEW.run_revision
              AND corporation_row.status = 'draft'
              AND corporation_row.established_game_day = NEW.game_day
              AND BINARY corporation_row.establishment_command_id = BINARY NEW.correlation_id
        ),
    NEW.transaction_kind,
    NULL
);

CREATE TRIGGER tr_corporation_ledger_posting_insert_valid
BEFORE INSERT ON corporation_ledger_posting
FOR EACH ROW
SET NEW.account_code = IF(
    EXISTS (
        SELECT 1
        FROM corporation_ledger_transaction AS ledger
        INNER JOIN corporation AS corporation_row
            ON corporation_row.id = ledger.corporation_id
           AND corporation_row.save_id = ledger.save_id
           AND corporation_row.run_revision = ledger.run_revision
        WHERE ledger.id = NEW.corporation_ledger_transaction_id
          AND ledger.save_id = NEW.save_id
          AND ledger.run_revision = NEW.run_revision
          AND ledger.corporation_id = NEW.corporation_id
          AND ledger.transaction_kind = 'establishment'
          AND corporation_row.status = 'draft'
          AND (
              (NEW.posting_order = 1
               AND NEW.account_code = 'corporationCash'
               AND NEW.amount_krw = corporation_row.capital_krw)
              OR
              (NEW.posting_order = 2
               AND NEW.account_code = 'contributedCapital'
               AND NEW.amount_krw = -corporation_row.capital_krw)
          )
    ),
    NEW.account_code,
    NULL
);

ALTER TABLE ledger_transaction
    ADD CONSTRAINT ck_ledger_transaction_corporation_source CHECK (
        source_kind NOT LIKE 'corporation%'
        OR source_kind IN (
            'corporationEstablishment', 'corporationOfficerPayroll', 'corporationDividend'
        )
    );

ALTER TABLE ledger_posting
    ADD COLUMN corporation_id BIGINT UNSIGNED NULL AFTER insurance_contract_id,
    ADD KEY ix_ledger_posting_corporation (save_id, run_revision, corporation_id),
    ADD CONSTRAINT fk_ledger_posting_corporation
        FOREIGN KEY (save_id, run_revision, corporation_id)
        REFERENCES corporation (save_id, run_revision, id),
    DROP CHECK ck_ledger_posting_account_code,
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
            'militarySavingsGovernmentMatchIncome',
            'livingCostExpense', 'essentialArrearLiability',
            'loanPrincipalLiability', 'loanInterestExpense', 'loanInterestLiability',
            'loanFeeExpense', 'taxObligationLiability',
            'leaseDepositAsset', 'movingExpense',
            'leaseRentExpense', 'leaseArrearLiability',
            'propertyAsset', 'acquisitionIncidentalExpense',
            'propertyDispositionExpense', 'propertyTaxExpense',
            'welfareBenefitIncome', 'lifeEventExpense',
            'insurancePremiumExpense', 'insuranceClaimRecovery',
            'insolvencyDischargedDebt', 'insolvencyDischargeGain',
            'corporationInvestmentAsset', 'corporationRegistrationExpense'
        )
    ),
    ADD CONSTRAINT ck_ledger_posting_corporation_reference CHECK (
        corporation_id IS NULL
        OR account_code IN (
            'wallet', 'corporationInvestmentAsset', 'corporationRegistrationExpense',
            'salaryIncome', 'employmentIncomeTaxWithholding',
            'employmentLocalIncomeTaxWithholding', 'distributionIncome',
            'withholdingTaxLiability'
        )
    );

CREATE TRIGGER tr_ledger_transaction_corporation_source_insert
BEFORE INSERT ON ledger_transaction
FOR EACH ROW
FOLLOWS tr_ledger_transaction_insolvency_source_insert
SET NEW.source_kind = IF(
    NEW.source_kind NOT LIKE 'corporation%'
        OR (NEW.source_kind = 'corporationEstablishment' AND (
            NEW.source_id REGEXP '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            AND EXISTS (
                SELECT 1
                FROM corporation AS corporation_row
                INNER JOIN run_rule_bundle AS bundle
                    ON bundle.save_id = corporation_row.save_id
                   AND bundle.run_revision = corporation_row.run_revision
                WHERE BINARY corporation_row.establishment_command_id = BINARY NEW.source_id
                  AND corporation_row.save_id = NEW.save_id
                  AND corporation_row.run_revision = NEW.run_revision
                  AND corporation_row.status = 'draft'
                  AND bundle.policy_set_id = NEW.policy_set_id
            )
        )),
    NEW.source_kind,
    NULL
);

CREATE TRIGGER tr_ledger_posting_corporation_reference_insert
BEFORE INSERT ON ledger_posting
FOR EACH ROW
FOLLOWS tr_ledger_posting_insolvency_reference_insert
SET NEW.account_code = IF(
    NEW.corporation_id IS NULL
        AND NOT EXISTS (
            SELECT 1 FROM ledger_transaction AS ledger
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'corporationEstablishment'
        )
    OR EXISTS (
        SELECT 1
        FROM ledger_transaction AS ledger
        INNER JOIN corporation AS corporation_row
            ON corporation_row.save_id = ledger.save_id
           AND corporation_row.run_revision = ledger.run_revision
           AND BINARY corporation_row.establishment_command_id = BINARY ledger.source_id
        WHERE ledger.id = NEW.ledger_transaction_id
          AND ledger.save_id = NEW.save_id
          AND ledger.run_revision = NEW.run_revision
          AND ledger.source_kind = 'corporationEstablishment'
          AND corporation_row.id = NEW.corporation_id
          AND corporation_row.status = 'draft'
          AND (
              (NEW.posting_order = 1
               AND NEW.account_code = 'corporationInvestmentAsset'
               AND NEW.amount_krw = corporation_row.capital_krw)
              OR
              (NEW.posting_order = 2
               AND NEW.account_code = 'corporationRegistrationExpense'
               AND NEW.amount_krw = corporation_row.total_establishment_fee_krw)
              OR
              (NEW.posting_order = 3
               AND NEW.account_code = 'wallet'
               AND NEW.amount_krw = -(
                   corporation_row.capital_krw + corporation_row.total_establishment_fee_krw
               ))
          )
    ),
    NEW.account_code,
    NULL
);

CREATE TRIGGER tr_corporation_transition_insert_valid
BEFORE INSERT ON corporation_transition
FOR EACH ROW
SET NEW.transition_no = IF(
    (
        NEW.transition_no = 1
        AND NEW.from_status IS NULL
        AND NEW.to_status = 'draft'
        AND NEW.transition_reason = 'playerEstablished'
        AND NEW.command_id IS NOT NULL
        AND EXISTS (
            SELECT 1 FROM corporation AS corporation_row
            WHERE corporation_row.id = NEW.corporation_id
              AND corporation_row.save_id = NEW.save_id
              AND corporation_row.run_revision = NEW.run_revision
              AND corporation_row.status = 'draft'
              AND BINARY corporation_row.establishment_command_id = BINARY NEW.command_id
              AND corporation_row.established_game_day = NEW.transition_game_day
        )
    )
    OR (
        NEW.transition_no = 2
        AND NEW.from_status = 'draft'
        AND NEW.to_status = 'active'
        AND NEW.transition_reason = 'establishmentFunded'
        AND NEW.command_id IS NOT NULL
        AND EXISTS (
            SELECT 1 FROM corporation AS corporation_row
            WHERE corporation_row.id = NEW.corporation_id
              AND corporation_row.save_id = NEW.save_id
              AND corporation_row.run_revision = NEW.run_revision
              AND corporation_row.status = 'active'
              AND BINARY corporation_row.establishment_command_id = BINARY NEW.command_id
              AND corporation_row.established_game_day = NEW.transition_game_day
        )
    ),
    NEW.transition_no,
    NULL
);

CREATE TRIGGER tr_corporation_status_transition_only
BEFORE UPDATE ON corporation
FOR EACH ROW
SET NEW.id = IF(
    OLD.status = 'draft'
        AND NEW.status = 'active'
        AND NEW.id = OLD.id
        AND NEW.save_id = OLD.save_id
        AND NEW.run_revision = OLD.run_revision
        AND NEW.life_catalog_set_id = OLD.life_catalog_set_id
        AND NEW.policy_set_id = OLD.policy_set_id
        AND NEW.corporation_component_version_id = OLD.corporation_component_version_id
        AND NEW.industry_template_id = OLD.industry_template_id
        AND NEW.registration_policy_rule_id = OLD.registration_policy_rule_id
        AND BINARY NEW.name = BINARY OLD.name
        AND BINARY NEW.representative_name = BINARY OLD.representative_name
        AND BINARY NEW.registered_office_class = BINARY OLD.registered_office_class
        AND BINARY NEW.establishment_command_id = BINARY OLD.establishment_command_id
        AND NEW.established_game_day = OLD.established_game_day
        AND NEW.capital_krw = OLD.capital_krw
        AND NEW.registration_license_tax_krw = OLD.registration_license_tax_krw
        AND NEW.local_education_tax_krw = OLD.local_education_tax_krw
        AND NEW.game_administrative_fee_krw = OLD.game_administrative_fee_krw
        AND NEW.total_establishment_fee_krw = OLD.total_establishment_fee_krw
        AND NEW.cash_krw = OLD.capital_krw
        AND NEW.contributed_capital_krw = OLD.capital_krw
        AND NEW.retained_earnings_krw = 0
        AND NEW.operating_payable_krw = 0
        AND NEW.distributable_profit_krw = 0
        AND NEW.personal_ledger_transaction_id IS NOT NULL
        AND NEW.corporation_ledger_transaction_id IS NOT NULL
        AND NEW.created_at = OLD.created_at
        AND EXISTS (
            SELECT 1 FROM ledger_transaction AS ledger
            WHERE ledger.id = NEW.personal_ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'corporationEstablishment'
              AND BINARY ledger.source_id = BINARY NEW.establishment_command_id
              AND (
                  SELECT COALESCE(SUM(posting.amount_krw), 0)
                  FROM ledger_posting AS posting
                  WHERE posting.ledger_transaction_id = ledger.id
              ) = 0
              AND (
                  SELECT COUNT(*) FROM ledger_posting AS posting
                  WHERE posting.ledger_transaction_id = ledger.id
              ) = 3
        )
        AND EXISTS (
            SELECT 1 FROM corporation_ledger_transaction AS ledger
            WHERE ledger.id = NEW.corporation_ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.corporation_id = NEW.id
              AND ledger.transaction_kind = 'establishment'
              AND (
                  SELECT COALESCE(SUM(posting.amount_krw), 0)
                  FROM corporation_ledger_posting AS posting
                  WHERE posting.corporation_ledger_transaction_id = ledger.id
              ) = 0
              AND (
                  SELECT COUNT(*) FROM corporation_ledger_posting AS posting
                  WHERE posting.corporation_ledger_transaction_id = ledger.id
              ) = 2
        ),
    OLD.id,
    NULL
);

CREATE TRIGGER tr_corporation_no_delete
BEFORE DELETE ON corporation
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporations are append-only';

CREATE TRIGGER tr_corporation_transition_no_update
BEFORE UPDATE ON corporation_transition
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation transitions are immutable';

CREATE TRIGGER tr_corporation_transition_no_delete
BEFORE DELETE ON corporation_transition
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation transitions are immutable';

CREATE TRIGGER tr_corporation_ledger_transaction_no_update
BEFORE UPDATE ON corporation_ledger_transaction
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation ledger transactions are immutable';

CREATE TRIGGER tr_corporation_ledger_transaction_no_delete
BEFORE DELETE ON corporation_ledger_transaction
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation ledger transactions are immutable';

CREATE TRIGGER tr_corporation_ledger_posting_no_update
BEFORE UPDATE ON corporation_ledger_posting
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation ledger postings are immutable';

CREATE TRIGGER tr_corporation_ledger_posting_no_delete
BEFORE DELETE ON corporation_ledger_posting
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation ledger postings are immutable';

CREATE TRIGGER tr_corporation_receipt_insert_valid
BEFORE INSERT ON corporation_command_receipt
FOR EACH ROW
SET NEW.command_kind = IF(
    NEW.command_kind = 'createCorporation'
        AND EXISTS (
            SELECT 1
            FROM corporation AS corporation_row
            INNER JOIN command_identity AS identity
                ON identity.save_id = corporation_row.save_id
               AND BINARY identity.command_id = BINARY corporation_row.establishment_command_id
               AND identity.command_kind = 'createCorporation'
            INNER JOIN save
                ON save.id = corporation_row.save_id
               AND save.run_revision = corporation_row.run_revision
            WHERE corporation_row.id = NEW.corporation_id
              AND corporation_row.save_id = NEW.save_id
              AND corporation_row.run_revision = NEW.run_revision
              AND corporation_row.status = 'active'
              AND BINARY corporation_row.establishment_command_id = BINARY NEW.command_id
              AND corporation_row.established_game_day = NEW.game_day
              AND corporation_row.personal_ledger_transaction_id
                    = NEW.personal_ledger_transaction_id
              AND corporation_row.corporation_ledger_transaction_id
                    = NEW.corporation_ledger_transaction_id
              AND identity.initial_run_revision = NEW.run_revision
              AND identity.initial_state_revision + 1 = NEW.state_revision
              AND identity.initial_game_day = NEW.game_day
              AND save.state_revision = NEW.state_revision
              AND save.game_day = NEW.game_day
        ),
    NEW.command_kind,
    NULL
);

CREATE TRIGGER tr_corporation_receipt_no_update
BEFORE UPDATE ON corporation_command_receipt
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation command receipts are immutable';

CREATE TRIGGER tr_corporation_receipt_no_delete
BEFORE DELETE ON corporation_command_receipt
FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'corporation command receipts are immutable';

DROP TRIGGER tr_employment_finance_compatibility_valid_insert;

INSERT INTO employment_finance_compatibility
    (employment_policy_set_id, policy_set_id)
SELECT employment_assignment.employment_policy_set_id, policy.id
FROM employment_policy_assignment AS employment_assignment
INNER JOIN policy_set AS policy
    ON policy.policy_key = 'dev-unranked-kr-corporation-2026-v5'
   AND policy.sealed_at IS NOT NULL
WHERE employment_assignment.assignment_key = 'newRun';

CREATE TRIGGER tr_employment_finance_compatibility_valid_insert
BEFORE INSERT ON employment_finance_compatibility
FOR EACH ROW
SET NEW.employment_policy_set_id = IF(
    EXISTS (
        SELECT 1
        FROM employment_policy_set AS employment_policy
        INNER JOIN policy_set AS finance_policy
            ON finance_policy.id = NEW.policy_set_id
        WHERE employment_policy.id = NEW.employment_policy_set_id
          AND employment_policy.published_at IS NULL
          AND finance_policy.sealed_at IS NOT NULL
    ),
    NEW.employment_policy_set_id,
    NULL
);

UPDATE policy_set_assignment AS assignment
INNER JOIN policy_set AS policy
    ON policy.policy_key = 'dev-unranked-kr-corporation-2026-v5'
   AND policy.sealed_at IS NOT NULL
SET assignment.policy_set_id = policy.id
WHERE assignment.assignment_key = 'newRun';

UPDATE run_rule_bundle_assignment AS assignment
INNER JOIN policy_set_assignment AS finance_assignment
    ON finance_assignment.assignment_key = 'newRun'
INNER JOIN policy_set AS policy
    ON policy.id = finance_assignment.policy_set_id
   AND policy.policy_key = 'dev-unranked-kr-corporation-2026-v5'
   AND policy.sealed_at IS NOT NULL
INNER JOIN life_catalog_set AS catalog
    ON catalog.catalog_key = 'dev-unranked-m4-life-corporation-2026-v6'
   AND catalog.sealed_at IS NOT NULL
SET assignment.policy_set_id = policy.id,
    assignment.life_catalog_set_id = catalog.id,
    assignment.finance_assignment_revision = finance_assignment.assignment_revision
WHERE assignment.assignment_key = 'newRun';
