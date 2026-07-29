-- Versioned KRX-listed instrument catalog and market-data sync evidence (§9.3).

CREATE TABLE equity_catalog_version (
    id                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    version             VARCHAR(96) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    source              VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    source_as_of        DATE            NOT NULL,
    content_sha256      CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    instrument_count    INT UNSIGNED    NOT NULL,
    created_at          DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    published_at        DATETIME(3)         NULL,
    PRIMARY KEY (id),
    UNIQUE KEY uk_equity_catalog_version (version),
    UNIQUE KEY uk_equity_catalog_content (content_sha256),
    CONSTRAINT ck_equity_catalog_source CHECK (source = 'dataGoKr'),
    CONSTRAINT ck_equity_catalog_hash CHECK (content_sha256 REGEXP '^[0-9a-f]{64}$'),
    CONSTRAINT ck_equity_catalog_count CHECK (instrument_count > 0)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE equity_instrument_version (
    id                              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    equity_catalog_version_id       BIGINT UNSIGNED NOT NULL,
    isin                            CHAR(12) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    short_code                      VARCHAR(12) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    market                          VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    display_name                    VARCHAR(160) NOT NULL,
    corporation_name                VARCHAR(200) NOT NULL,
    corporation_registration_number VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL,
    dart_corp_code                  CHAR(8) CHARACTER SET ascii COLLATE ascii_bin NULL,
    industry_code                   VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NULL,
    created_at                      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (id),
    UNIQUE KEY uk_equity_instrument_isin (equity_catalog_version_id, isin),
    UNIQUE KEY uk_equity_instrument_code (equity_catalog_version_id, short_code),
    KEY ix_equity_instrument_name (equity_catalog_version_id, display_name, id),
    KEY ix_equity_instrument_corporation (equity_catalog_version_id, corporation_name, id),
    KEY ix_equity_instrument_market_code (equity_catalog_version_id, market, short_code),
    CONSTRAINT fk_equity_instrument_catalog
        FOREIGN KEY (equity_catalog_version_id) REFERENCES equity_catalog_version (id),
    CONSTRAINT ck_equity_instrument_isin CHECK (isin REGEXP '^KR[A-Z0-9]{10}$'),
    CONSTRAINT ck_equity_instrument_short_code CHECK (short_code REGEXP '^[0-9A-Z]{6,12}$'),
    CONSTRAINT ck_equity_instrument_market CHECK (market IN ('kospi', 'kosdaq', 'konex', 'other')),
    CONSTRAINT ck_equity_instrument_names CHECK (
        CHAR_LENGTH(display_name) > 0 AND CHAR_LENGTH(corporation_name) > 0
    ),
    CONSTRAINT ck_equity_instrument_dart CHECK (
        dart_corp_code IS NULL OR dart_corp_code REGEXP '^[0-9]{8}$'
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE equity_catalog_assignment (
    assignment_key              VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    equity_catalog_version_id   BIGINT UNSIGNED NOT NULL,
    assignment_revision         BIGINT UNSIGNED NOT NULL DEFAULT 1,
    updated_at                  DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
        ON UPDATE CURRENT_TIMESTAMP(3),
    PRIMARY KEY (assignment_key),
    KEY ix_equity_catalog_assignment_version (equity_catalog_version_id),
    CONSTRAINT fk_equity_catalog_assignment_version
        FOREIGN KEY (equity_catalog_version_id) REFERENCES equity_catalog_version (id),
    CONSTRAINT ck_equity_catalog_assignment_key CHECK (assignment_key = 'active')
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE market_data_sync_run (
    id                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    provider            VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    dataset             VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    status              VARCHAR(24) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    row_count           INT UNSIGNED NOT NULL DEFAULT 0,
    content_sha256      CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL,
    source_as_of        VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NULL,
    failure_code        VARCHAR(48) CHARACTER SET ascii COLLATE ascii_bin NULL,
    started_at          DATETIME(3) NOT NULL,
    completed_at        DATETIME(3) NOT NULL,
    PRIMARY KEY (id),
    KEY ix_market_data_sync_provider_completed (provider, completed_at),
    CONSTRAINT ck_market_data_sync_provider CHECK (
        provider IN ('dataGoKr', 'krx', 'openDart', 'fmp', 'ecos', 'kosis')
    ),
    CONSTRAINT ck_market_data_sync_status CHECK (
        status IN ('completed', 'notConfigured', 'failed')
    ),
    CONSTRAINT ck_market_data_sync_hash CHECK (
        content_sha256 IS NULL OR content_sha256 REGEXP '^[0-9a-f]{64}$'
    ),
    CONSTRAINT ck_market_data_sync_failure CHECK (
        (status = 'failed' AND failure_code IS NOT NULL)
        OR (status <> 'failed' AND failure_code IS NULL)
    )
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TRIGGER tr_equity_catalog_version_no_update
BEFORE UPDATE ON equity_catalog_version
FOR EACH ROW
SET NEW.version = IF(
    OLD.published_at IS NULL
        AND NEW.published_at IS NOT NULL
        AND BINARY NEW.version = BINARY OLD.version
        AND BINARY NEW.source = BINARY OLD.source
        AND NEW.source_as_of = OLD.source_as_of
        AND BINARY NEW.content_sha256 = BINARY OLD.content_sha256
        AND NEW.instrument_count = OLD.instrument_count
        AND NEW.created_at = OLD.created_at,
    NEW.version,
    NULL
);

CREATE TRIGGER tr_equity_catalog_version_no_delete
BEFORE DELETE ON equity_catalog_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'equity catalog versions are immutable';

CREATE TRIGGER tr_equity_instrument_no_update
BEFORE UPDATE ON equity_instrument_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'equity instrument versions are immutable';

CREATE TRIGGER tr_equity_instrument_valid_insert
BEFORE INSERT ON equity_instrument_version
FOR EACH ROW
SET NEW.equity_catalog_version_id = IF(
    EXISTS (
        SELECT 1
        FROM equity_catalog_version AS catalog
        WHERE catalog.id = NEW.equity_catalog_version_id
          AND catalog.published_at IS NULL
    ),
    NEW.equity_catalog_version_id,
    NULL
);

CREATE TRIGGER tr_equity_instrument_no_delete
BEFORE DELETE ON equity_instrument_version
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'equity instrument versions are immutable';

CREATE TRIGGER tr_equity_catalog_assignment_bump_revision
BEFORE UPDATE ON equity_catalog_assignment
FOR EACH ROW
SET
    NEW.equity_catalog_version_id = IF(
        BINARY NEW.assignment_key = BINARY OLD.assignment_key
            AND NEW.equity_catalog_version_id <> OLD.equity_catalog_version_id
            AND NEW.assignment_revision = OLD.assignment_revision
            AND EXISTS (
                SELECT 1
                FROM equity_catalog_version AS catalog
                WHERE catalog.id = NEW.equity_catalog_version_id
                  AND catalog.published_at IS NOT NULL
            ),
        NEW.equity_catalog_version_id,
        NULL
    ),
    NEW.assignment_revision = OLD.assignment_revision + 1;

CREATE TRIGGER tr_equity_catalog_assignment_valid_insert
BEFORE INSERT ON equity_catalog_assignment
FOR EACH ROW
SET NEW.equity_catalog_version_id = IF(
    NEW.assignment_revision = 1
        AND EXISTS (
            SELECT 1
            FROM equity_catalog_version AS catalog
            WHERE catalog.id = NEW.equity_catalog_version_id
              AND catalog.published_at IS NOT NULL
        ),
    NEW.equity_catalog_version_id,
    NULL
);

CREATE TRIGGER tr_equity_catalog_assignment_no_delete
BEFORE DELETE ON equity_catalog_assignment
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'equity catalog assignment must be updated in place';

CREATE TRIGGER tr_market_data_sync_run_no_update
BEFORE UPDATE ON market_data_sync_run
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'market data sync runs are immutable';

CREATE TRIGGER tr_market_data_sync_run_no_delete
BEFORE DELETE ON market_data_sync_run
FOR EACH ROW
SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'market data sync runs are immutable';
