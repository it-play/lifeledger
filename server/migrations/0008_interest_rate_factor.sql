-- M1-C versioned interest-rate common factor. Legacy market rows remain explicitly factorless.

ALTER TABLE market_daily
    ADD COLUMN policy_rate_bp SMALLINT NULL AFTER equity_variance_ppm2,
    ADD COLUMN treasury_3m_bp SMALLINT NULL AFTER policy_rate_bp,
    ADD COLUMN treasury_1y_bp SMALLINT NULL AFTER treasury_3m_bp,
    ADD COLUMN treasury_3y_bp SMALLINT NULL AFTER treasury_1y_bp,
    ADD COLUMN treasury_10y_bp SMALLINT NULL AFTER treasury_3y_bp,
    ADD COLUMN policy_rate_change_bp SMALLINT NULL AFTER treasury_10y_bp,
    ADD COLUMN equity_rate_shock_ppm INT NULL AFTER policy_rate_change_bp,
    ADD CONSTRAINT ck_market_daily_rate_factor_complete
        CHECK (
            (
                policy_rate_bp IS NULL
                AND treasury_3m_bp IS NULL
                AND treasury_1y_bp IS NULL
                AND treasury_3y_bp IS NULL
                AND treasury_10y_bp IS NULL
                AND policy_rate_change_bp IS NULL
                AND equity_rate_shock_ppm IS NULL
            )
            OR
            (
                policy_rate_bp IS NOT NULL
                AND treasury_3m_bp IS NOT NULL
                AND treasury_1y_bp IS NOT NULL
                AND treasury_3y_bp IS NOT NULL
                AND treasury_10y_bp IS NOT NULL
                AND policy_rate_change_bp IS NOT NULL
                AND equity_rate_shock_ppm IS NOT NULL
            )
        ),
    ADD CONSTRAINT ck_market_daily_policy_rate
        CHECK (policy_rate_bp IS NULL OR (policy_rate_bp BETWEEN 0 AND 800 AND MOD(policy_rate_bp, 25) = 0)),
    ADD CONSTRAINT ck_market_daily_treasury_3m
        CHECK (treasury_3m_bp IS NULL OR treasury_3m_bp BETWEEN 0 AND 1500),
    ADD CONSTRAINT ck_market_daily_treasury_1y
        CHECK (treasury_1y_bp IS NULL OR treasury_1y_bp BETWEEN 0 AND 1500),
    ADD CONSTRAINT ck_market_daily_treasury_3y
        CHECK (treasury_3y_bp IS NULL OR treasury_3y_bp BETWEEN 0 AND 1500),
    ADD CONSTRAINT ck_market_daily_treasury_10y
        CHECK (treasury_10y_bp IS NULL OR treasury_10y_bp BETWEEN 0 AND 1500),
    ADD CONSTRAINT ck_market_daily_policy_rate_change
        CHECK (
            policy_rate_change_bp IS NULL
            OR (policy_rate_change_bp BETWEEN -800 AND 800 AND MOD(policy_rate_change_bp, 25) = 0)
        ),
    ADD CONSTRAINT ck_market_daily_equity_rate_shock
        CHECK (
            equity_rate_shock_ppm IS NULL
            OR equity_rate_shock_ppm = -policy_rate_change_bp * 300
        ),
    ADD CONSTRAINT ck_market_daily_closed_rate_shock
        CHECK (
            policy_rate_bp IS NULL
            OR market_open = 1
            OR (policy_rate_change_bp = 0 AND equity_rate_shock_ppm = 0)
        );

INSERT INTO market_calibration (id, version, parameters)
SELECT
    3,
    'm1-2026-calibration-v3',
    JSON_SET(
        parameters,
        '$.interestRates',
        JSON_OBJECT(
            'initialPolicyRateBp', 250,
            'neutralPolicyRateBp', 250,
            'updateIntervalSessions', 21,
            'meanReversionPpm', 250000,
            'innovationScaleBp', 25,
            'quantizationStepBp', 25,
            'minPolicyRateBp', 0,
            'maxPolicyRateBp', 800,
            'targets', JSON_OBJECT(
                'expansion', 350,
                'slowdown', 250,
                'recession', 100,
                'recovery', 200
            ),
            'yieldCurve', JSON_OBJECT(
                'treasury3m', JSON_OBJECT(
                    'policyWeightPpm', 900000,
                    'neutralWeightPpm', 100000,
                    'termPremiumBp', 5
                ),
                'treasury1y', JSON_OBJECT(
                    'policyWeightPpm', 750000,
                    'neutralWeightPpm', 250000,
                    'termPremiumBp', 15
                ),
                'treasury3y', JSON_OBJECT(
                    'policyWeightPpm', 500000,
                    'neutralWeightPpm', 500000,
                    'termPremiumBp', 30
                ),
                'treasury10y', JSON_OBJECT(
                    'policyWeightPpm', 250000,
                    'neutralWeightPpm', 750000,
                    'termPremiumBp', 60
                )
            ),
            'maxYieldBp', 1500,
            'equityShockPpmPerPolicyBp', 300
        )
    )
FROM market_calibration
WHERE version = 'm1-2026-calibration-v2';

INSERT INTO market_world
    (id, world_key, seed, start_date, day0_equity_close_krw, calibration_id)
VALUES
    (3, 'm1-2026-v3', 20260101, '2026-01-01', 100000, 3);

-- Existing saves retain their immutable world. Only future run creation follows this pointer.
UPDATE market_world_assignment
SET world_id = 3
WHERE assignment_key = 'newRun';
