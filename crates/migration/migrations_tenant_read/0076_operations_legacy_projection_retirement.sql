-- OPERATIONS_LEGACY_PROJECTION_RETIREMENT
-- Generation-v2 Operations projections are generation-fenced under
-- erpx_operations_read and erpx_cqrs_read. Remove the old shared JSON read
-- rows/checkpoints so a route can never fall back to a generation-one model.
DROP TABLE IF EXISTS erpx_tenant.operations_read_rows;
DROP TABLE IF EXISTS erpx_tenant.operations_projection_checkpoints;

-- Reassert the final generation-two manifest fence after the retirement.
UPDATE erpx_meta.schema_manifest
SET installed_at = now()
WHERE singleton AND schema_generation = 2
  AND database_role = 'tenant_read' AND migration_set = 'tenant-read-v2';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM erpx_meta.schema_manifest
        WHERE singleton=TRUE AND schema_generation=2 AND manifest_version=1
          AND database_role='tenant_read' AND migration_set='tenant-read-v2'
    ) THEN
        RAISE EXCEPTION 'tenant-read generation-v2 manifest is not current';
    END IF;
END;
$$;
