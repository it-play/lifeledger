-- Public save identity and immutable character attribution for live save rankings.

ALTER TABLE save
    ADD COLUMN public_uid CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NULL AFTER id;

UPDATE save
SET public_uid = LOWER(SHA2(CONCAT(
    CAST(id AS CHAR), ':', UUID(), ':', HEX(RANDOM_BYTES(32))
), 256))
WHERE public_uid IS NULL;

ALTER TABLE save
    MODIFY COLUMN public_uid CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    ADD UNIQUE KEY uk_save_public_uid (public_uid),
    ADD CONSTRAINT ck_save_public_uid CHECK (public_uid REGEXP '^[0-9a-f]{64}$');

-- Historical manifests predate public attribution and therefore remain anonymous.
-- Every manifest written after this migration freezes the starting character name.
ALTER TABLE run_manifest
    ADD COLUMN character_name VARCHAR(20) NULL AFTER run_revision;
