-- Published catalog values must be representable by the exact JSON client contract (§13.2).

ALTER TABLE activity_catalog_entry
    ADD CONSTRAINT ck_activity_catalog_entry_api_bounds CHECK (
        required_effort_units <= 9007199254740991
        AND daily_effort_cap_units <= 9007199254740991
        AND cost_krw <= 9007199254740991
    );

ALTER TABLE career_effort_capacity
    ADD CONSTRAINT ck_career_effort_capacity_api_bounds CHECK (
        effort_units <= 9007199254740991
    );

-- A separate publication guard keeps the original complete-graph trigger immutable while
-- ensuring a published bundle can always be returned by the bounded catalog endpoint.
CREATE TRIGGER tr_career_catalog_bundle_activity_api_bound
BEFORE UPDATE ON career_catalog_bundle
FOR EACH ROW
SET NEW.bundle_key = IF(
    OLD.published_at IS NULL
        AND NEW.published_at IS NOT NULL
        AND (
            SELECT COUNT(*)
            FROM activity_catalog_entry AS activity
            WHERE activity.career_catalog_bundle_id = NEW.id
        ) BETWEEN 1 AND 200,
    NEW.bundle_key,
    IF(
        OLD.published_at IS NULL AND NEW.published_at IS NOT NULL,
        NULL,
        NEW.bundle_key
    )
);
