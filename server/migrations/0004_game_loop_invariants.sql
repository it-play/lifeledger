-- M0 game-loop invariants. Runtime speed remains in memory; the run generation is durable.

-- The old select-then-insert path could race, but every later read chose the lowest id.
-- Preserve that canonical save and remove higher ids that the application could never reach.
DELETE duplicate_save
FROM save AS duplicate_save
INNER JOIN save AS canonical_save
    ON canonical_save.user_id = duplicate_save.user_id
   AND canonical_save.id < duplicate_save.id;

ALTER TABLE save
    ADD COLUMN run_revision INT UNSIGNED NOT NULL DEFAULT 0 AFTER user_id,
    ADD UNIQUE KEY uk_save_user_id (user_id);

-- The unique index now covers both ownership lookup and the child side of the user FK.
ALTER TABLE save DROP INDEX ix_save_user_id;
