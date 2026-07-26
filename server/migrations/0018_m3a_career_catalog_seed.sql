-- Complete non-ranked development content for the first M3 career bundle (§2.1).

INSERT INTO career_catalog_bundle (bundle_key, ranked_eligible)
VALUES ('dev-unranked-m3-v1', FALSE);

INSERT INTO career_industry
    (career_catalog_bundle_id, industry_key, display_name)
SELECT bundle.id, seed.industry_key, seed.display_name
FROM career_catalog_bundle AS bundle
CROSS JOIN (
    SELECT 'itSoftware' AS industry_key, 'IT·소프트웨어' AS display_name
    UNION ALL SELECT 'financeInsurance', '금융·보험'
    UNION ALL SELECT 'manufacturing', '제조·생산'
    UNION ALL SELECT 'constructionEngineering', '건설·기술'
    UNION ALL SELECT 'retailService', '유통·서비스'
    UNION ALL SELECT 'publicSocial', '공공·사회서비스'
) AS seed
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO career_job_family
    (career_catalog_bundle_id, industry_id, job_family_key, display_name)
SELECT bundle.id, industry.id, seed.job_family_key, seed.display_name
FROM career_catalog_bundle AS bundle
CROSS JOIN (
    SELECT 'itSoftware' AS industry_key,
           'softwareEngineering' AS job_family_key,
           '소프트웨어 개발' AS display_name
    UNION ALL SELECT 'itSoftware', 'dataEngineering', '데이터 엔지니어링'
    UNION ALL SELECT 'financeInsurance', 'financialPlanning', '재무 설계'
    UNION ALL SELECT 'financeInsurance', 'riskManagement', '리스크 관리'
    UNION ALL SELECT 'manufacturing', 'productionManagement', '생산 관리'
    UNION ALL SELECT 'manufacturing', 'qualityEngineering', '품질 기술'
    UNION ALL SELECT 'constructionEngineering', 'civilEngineering', '토목 기술'
    UNION ALL SELECT 'constructionEngineering', 'facilityEngineering', '설비 기술'
    UNION ALL SELECT 'retailService', 'retailOperations', '유통 운영'
    UNION ALL SELECT 'retailService', 'customerService', '고객 서비스'
    UNION ALL SELECT 'publicSocial', 'publicAdministration', '공공 행정'
    UNION ALL SELECT 'publicSocial', 'socialWelfare', '사회 복지'
) AS seed
INNER JOIN career_industry AS industry
    ON industry.career_catalog_bundle_id = bundle.id
   AND BINARY industry.industry_key = BINARY seed.industry_key
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO spec_catalog_entry
    (
        career_catalog_bundle_id,
        entry_key,
        kind,
        display_name,
        stackable,
        validity_days
    )
SELECT
    bundle.id,
    seed.entry_key,
    seed.kind,
    seed.display_name,
    seed.stackable,
    seed.validity_days
FROM career_catalog_bundle AS bundle
CROSS JOIN (
    SELECT 'bridge-education-highSchool' AS entry_key,
           'education' AS kind,
           '고등학교 졸업' AS display_name,
           FALSE AS stackable,
           NULL AS validity_days
    UNION ALL SELECT 'bridge-education-associate', 'education', '전문학사', FALSE, NULL
    UNION ALL SELECT 'bridge-education-bachelor', 'education', '학사', FALSE, NULL
    UNION ALL SELECT 'bridge-education-master', 'education', '석사', FALSE, NULL
    UNION ALL SELECT 'bridge-education-doctorate', 'education', '박사', FALSE, NULL
    UNION ALL SELECT 'activity-language-score', 'language', '공인 어학 성적', FALSE, 730
    UNION ALL SELECT 'activity-practical-training', 'training', '직무 실무 교육', TRUE, NULL
    UNION ALL SELECT 'activity-portfolio-project', 'project', '포트폴리오 프로젝트', TRUE, NULL
) AS seed
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO spec_catalog_entry
    (
        career_catalog_bundle_id,
        entry_key,
        kind,
        display_name,
        stackable,
        validity_days
    )
WITH RECURSIVE certification_sequence (certification_order) AS (
    SELECT 1
    UNION ALL
    SELECT certification_order + 1
    FROM certification_sequence
    WHERE certification_order < 50
)
SELECT
    bundle.id,
    CONCAT('bridge-certification-', LPAD(certification_order, 2, '0')),
    'certification',
    CONCAT('시작 자격증 ', LPAD(certification_order, 2, '0')),
    FALSE,
    NULL
FROM career_catalog_bundle AS bundle
CROSS JOIN certification_sequence
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO spec_catalog_entry
    (
        career_catalog_bundle_id,
        entry_key,
        kind,
        display_name,
        stackable,
        validity_days
    )
WITH RECURSIVE experience_sequence (career_years) AS (
    SELECT 0
    UNION ALL
    SELECT career_years + 1
    FROM experience_sequence
    WHERE career_years < 30
)
SELECT
    bundle.id,
    CONCAT('bridge-experience-', LPAD(career_years, 2, '0')),
    'experience',
    CONCAT('시작 경력 ', career_years, '년'),
    FALSE,
    NULL
FROM career_catalog_bundle AS bundle
CROSS JOIN experience_sequence
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO spec_catalog_contribution
    (
        career_catalog_bundle_id,
        spec_catalog_entry_id,
        career_job_family_id,
        contribution_bp
    )
SELECT
    bundle.id,
    entry.id,
    family.id,
    CASE entry.kind
        WHEN 'education' THEN 1800
        WHEN 'certification' THEN 150
        WHEN 'language' THEN 2200
        WHEN 'training' THEN 1800
        WHEN 'experience' THEN 300
        WHEN 'project' THEN 2400
    END
FROM career_catalog_bundle AS bundle
INNER JOIN spec_catalog_entry AS entry
    ON entry.career_catalog_bundle_id = bundle.id
INNER JOIN career_job_family AS family
    ON family.career_catalog_bundle_id = bundle.id
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO career_bridge_education_mapping
    (career_catalog_bundle_id, education, evidence_key, spec_catalog_entry_id)
SELECT bundle.id, seed.education, seed.evidence_key, entry.id
FROM career_catalog_bundle AS bundle
CROSS JOIN (
    SELECT 'highSchool' AS education,
           'bridge:education:highSchool' AS evidence_key,
           'bridge-education-highSchool' AS entry_key
    UNION ALL SELECT 'associate', 'bridge:education:associate', 'bridge-education-associate'
    UNION ALL SELECT 'bachelor', 'bridge:education:bachelor', 'bridge-education-bachelor'
    UNION ALL SELECT 'master', 'bridge:education:master', 'bridge-education-master'
    UNION ALL SELECT 'doctorate', 'bridge:education:doctorate', 'bridge-education-doctorate'
) AS seed
INNER JOIN spec_catalog_entry AS entry
    ON entry.career_catalog_bundle_id = bundle.id
   AND BINARY entry.entry_key = BINARY seed.entry_key
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO career_bridge_certification_order
    (
        career_catalog_bundle_id,
        certification_order,
        evidence_key,
        spec_catalog_entry_id
    )
WITH RECURSIVE certification_sequence (certification_order) AS (
    SELECT 1
    UNION ALL
    SELECT certification_order + 1
    FROM certification_sequence
    WHERE certification_order < 50
)
SELECT
    bundle.id,
    certification_order,
    CONCAT('bridge:certification:', LPAD(certification_order, 2, '0')),
    entry.id
FROM career_catalog_bundle AS bundle
CROSS JOIN certification_sequence
INNER JOIN spec_catalog_entry AS entry
    ON entry.career_catalog_bundle_id = bundle.id
   AND BINARY entry.entry_key = BINARY CONCAT(
       'bridge-certification-',
       LPAD(certification_order, 2, '0')
   )
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO career_bridge_experience_mapping
    (career_catalog_bundle_id, career_years, evidence_key, spec_catalog_entry_id)
WITH RECURSIVE experience_sequence (career_years) AS (
    SELECT 0
    UNION ALL
    SELECT career_years + 1
    FROM experience_sequence
    WHERE career_years < 30
)
SELECT
    bundle.id,
    career_years,
    CONCAT('bridge:experience:', LPAD(career_years, 2, '0')),
    entry.id
FROM career_catalog_bundle AS bundle
CROSS JOIN experience_sequence
INNER JOIN spec_catalog_entry AS entry
    ON entry.career_catalog_bundle_id = bundle.id
   AND BINARY entry.entry_key = BINARY CONCAT(
       'bridge-experience-',
       LPAD(career_years, 2, '0')
   )
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO activity_catalog_entry
    (
        career_catalog_bundle_id,
        activity_key,
        display_name,
        minimum_calendar_days,
        required_effort_units,
        daily_effort_cap_units,
        cost_krw,
        evidence_catalog_entry_id
    )
SELECT
    bundle.id,
    seed.activity_key,
    seed.display_name,
    seed.minimum_calendar_days,
    seed.required_effort_units,
    seed.daily_effort_cap_units,
    seed.cost_krw,
    evidence.id
FROM career_catalog_bundle AS bundle
CROSS JOIN (
    SELECT 'language-exam' AS activity_key,
           '공인 어학 시험 준비' AS display_name,
           21 AS minimum_calendar_days,
           84 AS required_effort_units,
           6 AS daily_effort_cap_units,
           60000 AS cost_krw,
           'activity-language-score' AS evidence_entry_key
    UNION ALL SELECT 'practical-training', '직무 실무 교육', 28, 112, 6, 180000,
                     'activity-practical-training'
    UNION ALL SELECT 'portfolio-project', '포트폴리오 프로젝트', 35, 140, 7, 90000,
                     'activity-portfolio-project'
) AS seed
INNER JOIN spec_catalog_entry AS evidence
    ON evidence.career_catalog_bundle_id = bundle.id
   AND BINARY evidence.entry_key = BINARY seed.evidence_entry_key
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO activity_catalog_allowed_status
    (career_catalog_bundle_id, activity_catalog_entry_id, life_status)
SELECT bundle.id, activity.id, status.life_status
FROM career_catalog_bundle AS bundle
INNER JOIN activity_catalog_entry AS activity
    ON activity.career_catalog_bundle_id = bundle.id
CROSS JOIN (
    SELECT 'unemployed' AS life_status
    UNION ALL SELECT 'employed'
    UNION ALL SELECT 'activeDuty'
    UNION ALL SELECT 'socialService'
    UNION ALL SELECT 'specialService'
    UNION ALL SELECT 'officerOrNco'
) AS status
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO career_effort_capacity
    (career_catalog_bundle_id, life_status, effort_units)
SELECT bundle.id, seed.life_status, seed.effort_units
FROM career_catalog_bundle AS bundle
CROSS JOIN (
    SELECT 'unemployed' AS life_status, 12 AS effort_units
    UNION ALL SELECT 'employed', 7
    UNION ALL SELECT 'activeDuty', 2
    UNION ALL SELECT 'socialService', 5
    UNION ALL SELECT 'specialService', 4
    UNION ALL SELECT 'officerOrNco', 3
) AS seed
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO artifact_checklist_rule
    (
        career_catalog_bundle_id,
        artifact_kind,
        rule_kind,
        minimum_count,
        dimension,
        evidence_kind,
        weight_bp
    )
SELECT
    bundle.id,
    seed.artifact_kind,
    seed.rule_kind,
    seed.minimum_count,
    seed.dimension,
    seed.evidence_kind,
    seed.weight_bp
FROM career_catalog_bundle AS bundle
CROSS JOIN (
    SELECT 'portfolio' AS artifact_kind, 'headlinePresent' AS rule_kind,
           NULL AS minimum_count, NULL AS dimension, NULL AS evidence_kind, 1500 AS weight_bp
    UNION ALL SELECT 'portfolio', 'summaryPresent', NULL, NULL, NULL, 1500
    UNION ALL SELECT 'portfolio', 'minimumEvidenceCount', 1, NULL, NULL, 2000
    UNION ALL SELECT 'portfolio', 'containsEvidenceKind', NULL, NULL, 'project', 2500
    UNION ALL SELECT 'portfolio', 'projectPresent', NULL, NULL, NULL, 2500
    UNION ALL SELECT 'resume', 'headlinePresent', NULL, NULL, NULL, 1000
    UNION ALL SELECT 'resume', 'summaryPresent', NULL, NULL, NULL, 1000
    UNION ALL SELECT 'resume', 'minimumEvidenceCount', 3, NULL, NULL, 1500
    UNION ALL SELECT 'resume', 'containsDimension', NULL, 'education', NULL, 1500
    UNION ALL SELECT 'resume', 'containsDimension', NULL, 'experience', NULL, 2500
    UNION ALL SELECT 'resume', 'containsEvidenceKind', NULL, NULL, 'certification', 1500
    UNION ALL SELECT 'resume', 'containsEvidenceKind', NULL, NULL, 'language', 1000
    UNION ALL SELECT 'linkedinProfile', 'headlinePresent', NULL, NULL, NULL, 1500
    UNION ALL SELECT 'linkedinProfile', 'summaryPresent', NULL, NULL, NULL, 1500
    UNION ALL SELECT 'linkedinProfile', 'minimumEvidenceCount', 2, NULL, NULL, 1500
    UNION ALL SELECT 'linkedinProfile', 'containsDimension', NULL, 'experience', NULL, 2000
    UNION ALL SELECT 'linkedinProfile', 'containsEvidenceKind', NULL, NULL, 'language', 1000
    UNION ALL SELECT 'linkedinProfile', 'openToWork', NULL, NULL, NULL, 1500
    UNION ALL SELECT 'linkedinProfile', 'industryCountAtLeast', 1, NULL, NULL, 1000
) AS seed
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO platform_catalog
    (
        career_catalog_bundle_id,
        platform_key,
        display_name,
        daily_slot_count,
        competition_band,
        document_review_days,
        same_region_only,
        invitation_source,
        first_pay_reward_krw
    )
SELECT
    bundle.id,
    seed.platform_key,
    seed.display_name,
    seed.daily_slot_count,
    seed.competition_band,
    seed.document_review_days,
    seed.same_region_only,
    seed.invitation_source,
    seed.first_pay_reward_krw
FROM career_catalog_bundle AS bundle
CROSS JOIN (
    SELECT 'sarangbang' AS platform_key, '사랑방' AS display_name,
           2 AS daily_slot_count, 'low' AS competition_band,
           1 AS document_review_days, TRUE AS same_region_only,
           'none' AS invitation_source, 0 AS first_pay_reward_krw
    UNION ALL SELECT 'jobkorea', '잡코리아', 5, 'high', 3, FALSE, 'none', 0
    UNION ALL SELECT 'saramin', '사람인', 4, 'medium', 2, FALSE, 'resume', 0
    UNION ALL SELECT 'wanted', '원티드', 3, 'high', 3, FALSE, 'none', 500000
    UNION ALL SELECT 'linkedin', 'LinkedIn', 3, 'high', 4, FALSE, 'linkedinProfile', 0
    UNION ALL SELECT 'work24', '고용24', 3, 'low', 2, FALSE, 'none', 0
) AS seed
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO platform_artifact_requirement
    (career_catalog_bundle_id, platform_catalog_id, artifact_kind)
SELECT bundle.id, platform.id, seed.artifact_kind
FROM career_catalog_bundle AS bundle
CROSS JOIN (
    SELECT 'sarangbang' AS platform_key, 'resume' AS artifact_kind
    UNION ALL SELECT 'jobkorea', 'resume'
    UNION ALL SELECT 'saramin', 'resume'
    UNION ALL SELECT 'wanted', 'resume'
    UNION ALL SELECT 'wanted', 'portfolio'
    UNION ALL SELECT 'linkedin', 'linkedinProfile'
    UNION ALL SELECT 'work24', 'resume'
) AS seed
INNER JOIN platform_catalog AS platform
    ON platform.career_catalog_bundle_id = bundle.id
   AND BINARY platform.platform_key = BINARY seed.platform_key
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO platform_industry_weight
    (
        career_catalog_bundle_id,
        platform_catalog_id,
        career_industry_id,
        weight_bp
    )
SELECT
    bundle.id,
    platform.id,
    industry.id,
    CASE
        WHEN BINARY industry.industry_key = BINARY CASE platform.platform_key
            WHEN 'sarangbang' THEN 'retailService'
            WHEN 'jobkorea' THEN 'manufacturing'
            WHEN 'saramin' THEN 'financeInsurance'
            WHEN 'wanted' THEN 'itSoftware'
            WHEN 'linkedin' THEN 'itSoftware'
            WHEN 'work24' THEN 'publicSocial'
        END THEN 2000
        ELSE 1600
    END
FROM career_catalog_bundle AS bundle
INNER JOIN platform_catalog AS platform
    ON platform.career_catalog_bundle_id = bundle.id
INNER JOIN career_industry AS industry
    ON industry.career_catalog_bundle_id = bundle.id
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO virtual_employer
    (
        career_catalog_bundle_id,
        career_industry_id,
        employer_key,
        display_name,
        region
    )
SELECT
    bundle.id,
    industry.id,
    seed.employer_key,
    seed.display_name,
    seed.region
FROM career_catalog_bundle AS bundle
CROSS JOIN (
    SELECT 'itSoftware' AS industry_key, 'hanul-digital' AS employer_key,
           '한울디지털' AS display_name, 'capitalArea' AS region
    UNION ALL SELECT 'financeInsurance', 'mirae-finance', '미래금융', 'capitalArea'
    UNION ALL SELECT 'manufacturing', 'saebit-manufacturing', '새빛제조', 'metropolitan'
    UNION ALL SELECT 'constructionEngineering', 'daon-engineering', '다온기술', 'smallCity'
    UNION ALL SELECT 'retailService', 'on-gil-retail', '온길유통', 'metropolitan'
    UNION ALL SELECT 'publicSocial', 'nuri-public-service', '누리공공서비스', 'rural'
) AS seed
INNER JOIN career_industry AS industry
    ON industry.career_catalog_bundle_id = bundle.id
   AND BINARY industry.industry_key = BINARY seed.industry_key
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO job_template
    (
        career_catalog_bundle_id,
        template_key,
        platform_catalog_id,
        career_industry_id,
        career_job_family_id,
        virtual_employer_id,
        employment_type,
        minimum_education,
        required_certification_entry_id,
        minimum_experience_days,
        military_requirement,
        minimum_annual_salary_krw,
        maximum_annual_salary_krw,
        salary_step_krw,
        interview_delay_days,
        offer_expiry_days,
        posting_open_days
    )
SELECT
    bundle.id,
    CONCAT(platform.platform_key, '-', family.job_family_key, '-regular'),
    platform.id,
    industry.id,
    family.id,
    employer.id,
    'regular',
    CASE industry.industry_key
        WHEN 'retailService' THEN 'highSchool'
        WHEN 'manufacturing' THEN 'associate'
        WHEN 'constructionEngineering' THEN 'associate'
        ELSE 'bachelor'
    END,
    NULL,
    CASE
        WHEN family.job_family_key IN ('customerService', 'retailOperations') THEN 0
        WHEN family.job_family_key IN ('softwareEngineering', 'financialPlanning') THEN 365
        ELSE 730
    END,
    CASE
        WHEN industry.industry_key IN ('constructionEngineering', 'publicSocial')
            THEN 'completedOrExempt'
        ELSE 'none'
    END,
    CASE industry.industry_key
        WHEN 'itSoftware' THEN 42000000
        WHEN 'financeInsurance' THEN 40000000
        WHEN 'manufacturing' THEN 36000000
        WHEN 'constructionEngineering' THEN 38000000
        WHEN 'retailService' THEN 30000000
        WHEN 'publicSocial' THEN 34000000
    END,
    CASE industry.industry_key
        WHEN 'itSoftware' THEN 70000000
        WHEN 'financeInsurance' THEN 65000000
        WHEN 'manufacturing' THEN 56000000
        WHEN 'constructionEngineering' THEN 60000000
        WHEN 'retailService' THEN 45000000
        WHEN 'publicSocial' THEN 52000000
    END,
    1000000,
    5,
    7,
    14
FROM career_catalog_bundle AS bundle
INNER JOIN career_job_family AS family
    ON family.career_catalog_bundle_id = bundle.id
INNER JOIN career_industry AS industry
    ON industry.career_catalog_bundle_id = bundle.id
   AND industry.id = family.industry_id
INNER JOIN virtual_employer AS employer
    ON employer.career_catalog_bundle_id = bundle.id
   AND employer.career_industry_id = industry.id
INNER JOIN platform_catalog AS platform
    ON platform.career_catalog_bundle_id = bundle.id
   AND BINARY platform.platform_key = BINARY CASE family.job_family_key
       WHEN 'softwareEngineering' THEN 'wanted'
       WHEN 'dataEngineering' THEN 'linkedin'
       WHEN 'financialPlanning' THEN 'jobkorea'
       WHEN 'riskManagement' THEN 'saramin'
       WHEN 'productionManagement' THEN 'jobkorea'
       WHEN 'qualityEngineering' THEN 'saramin'
       WHEN 'civilEngineering' THEN 'wanted'
       WHEN 'facilityEngineering' THEN 'jobkorea'
       WHEN 'retailOperations' THEN 'sarangbang'
       WHEN 'customerService' THEN 'sarangbang'
       WHEN 'publicAdministration' THEN 'work24'
       WHEN 'socialWelfare' THEN 'work24'
   END
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO job_template_dimension_requirement
    (
        career_catalog_bundle_id,
        job_template_id,
        dimension,
        required_score_bp,
        weight_bp
    )
SELECT
    bundle.id,
    template.id,
    seed.dimension,
    seed.required_score_bp,
    seed.weight_bp
FROM career_catalog_bundle AS bundle
INNER JOIN job_template AS template
    ON template.career_catalog_bundle_id = bundle.id
CROSS JOIN (
    SELECT 'education' AS dimension, 2500 AS required_score_bp, 1800 AS weight_bp
    UNION ALL SELECT 'certification', 2000, 1700
    UNION ALL SELECT 'language', 1000, 1000
    UNION ALL SELECT 'training', 1000, 1000
    UNION ALL SELECT 'experience', 2500, 2800
    UNION ALL SELECT 'project', 1500, 1700
) AS seed
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO military_option_version
    (
        career_catalog_bundle_id,
        option_key,
        service_type,
        display_name,
        effort_life_status,
        compensation_kind,
        pay_schedule,
        minimum_education,
        required_certification_entry_id,
        grants_career_experience
    )
SELECT
    bundle.id,
    seed.option_key,
    seed.service_type,
    seed.display_name,
    seed.effort_life_status,
    seed.compensation_kind,
    'monthly',
    seed.minimum_education,
    NULL,
    seed.grants_career_experience
FROM career_catalog_bundle AS bundle
CROSS JOIN (
    SELECT 'active-duty-v1' AS option_key, 'activeDuty' AS service_type,
           '현역' AS display_name, 'activeDuty' AS effort_life_status,
           'militaryPay' AS compensation_kind, NULL AS minimum_education,
           FALSE AS grants_career_experience
    UNION ALL SELECT 'social-service-v1', 'socialService', '사회복무요원',
                     'socialService', 'militaryPay', NULL, FALSE
    UNION ALL SELECT 'industrial-technical-v1', 'industrialTechnical', '산업기능요원',
                     'specialService', 'employmentPayroll', 'associate', TRUE
    UNION ALL SELECT 'professional-research-v1', 'professionalResearch', '전문연구요원',
                     'specialService', 'employmentPayroll', 'master', TRUE
    UNION ALL SELECT 'commissioned-officer-v1', 'commissionedOfficer', '장교',
                     'officerOrNco', 'employmentPayroll', 'bachelor', TRUE
    UNION ALL SELECT 'non-commissioned-officer-v1', 'nonCommissionedOfficer', '부사관',
                     'officerOrNco', 'employmentPayroll', 'highSchool', TRUE
) AS seed
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO military_option_job_family
    (
        career_catalog_bundle_id,
        military_option_version_id,
        career_job_family_id,
        experience_credit_ppm
    )
SELECT bundle.id, military_option.id, family.id, seed.experience_credit_ppm
FROM career_catalog_bundle AS bundle
CROSS JOIN (
    SELECT 'industrialTechnical' AS service_type,
           'productionManagement' AS job_family_key,
           1000000 AS experience_credit_ppm
    UNION ALL SELECT 'industrialTechnical', 'qualityEngineering', 1000000
    UNION ALL SELECT 'professionalResearch', 'softwareEngineering', 1000000
    UNION ALL SELECT 'professionalResearch', 'dataEngineering', 1000000
    UNION ALL SELECT 'commissionedOfficer', 'civilEngineering', 500000
    UNION ALL SELECT 'commissionedOfficer', 'publicAdministration', 500000
    UNION ALL SELECT 'nonCommissionedOfficer', 'qualityEngineering', 500000
    UNION ALL SELECT 'nonCommissionedOfficer', 'facilityEngineering', 500000
) AS seed
INNER JOIN military_option_version AS military_option
    ON military_option.career_catalog_bundle_id = bundle.id
   AND BINARY military_option.service_type = BINARY seed.service_type
INNER JOIN career_job_family AS family
    ON family.career_catalog_bundle_id = bundle.id
   AND BINARY family.job_family_key = BINARY seed.job_family_key
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

INSERT INTO military_savings_institution_catalog
    (career_catalog_bundle_id, financial_institution_id)
SELECT bundle.id, institution.id
FROM career_catalog_bundle AS bundle
INNER JOIN financial_institution AS institution
    ON BINARY institution.institution_key IN (
        BINARY 'life-bank-a',
        BINARY 'life-bank-b'
    )
WHERE BINARY bundle.bundle_key = BINARY 'dev-unranked-m3-v1';

-- Publication runs the complete graph validation from migration 0017.
UPDATE career_catalog_bundle
SET default_focused_job_family_key = 'softwareEngineering',
    published_at = CURRENT_TIMESTAMP(3)
WHERE BINARY bundle_key = BINARY 'dev-unranked-m3-v1'
  AND published_at IS NULL;

INSERT INTO career_catalog_assignment
    (assignment_key, career_catalog_bundle_id)
SELECT 'newRun', id
FROM career_catalog_bundle
WHERE BINARY bundle_key = BINARY 'dev-unranked-m3-v1'
  AND published_at IS NOT NULL;
