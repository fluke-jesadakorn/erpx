-- Role target scope is independent from the connector's transport scope.
-- Existing mappings are backfilled from their legacy scope key and continue
-- to be accepted as compatibility input.

ALTER TABLE erpx_identity.directory_role_mappings
    ADD COLUMN IF NOT EXISTS target_scope_key TEXT;

UPDATE erpx_identity.directory_role_mappings
SET target_scope_key = CASE
    WHEN scope_key = 'platform' THEN 'platform'
    WHEN scope_key LIKE 'holding:%' THEN 'holding'
    WHEN scope_key LIKE 'branch:%' THEN 'branch'
    ELSE 'holding'
END
WHERE target_scope_key IS NULL;

ALTER TABLE erpx_identity.directory_role_mappings
    ALTER COLUMN target_scope_key SET DEFAULT 'platform',
    ALTER COLUMN target_scope_key SET NOT NULL;

ALTER TABLE erpx_identity.directory_role_mappings
    DROP CONSTRAINT IF EXISTS directory_role_mappings_target_scope_check;
ALTER TABLE erpx_identity.directory_role_mappings
    ADD CONSTRAINT directory_role_mappings_target_scope_check
    CHECK (target_scope_key IN ('platform', 'holding', 'company', 'branch'));
