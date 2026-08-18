-- Hard cutover fence repeated after the Sales/Finance vertical-slice schema.
-- A migration set must always end with the generation marker so a newly
-- appended feature migration cannot accidentally become its terminal fence.
DROP TABLE IF EXISTS erpx_cqrs_write.aggregate_snapshots;
DROP TABLE IF EXISTS erpx_tenant.aggregate_snapshots;

-- OPERATIONS_ORGANIZATION_LEGACY_AUTHORITY_RETIREMENT
DROP TABLE IF EXISTS erpx_tenant.operations_source_references;
DROP TABLE IF EXISTS erpx_tenant.operations_sla_notifications;
DROP TABLE IF EXISTS erpx_tenant.operations_events;
DROP TABLE IF EXISTS erpx_tenant.operations_aggregate_snapshots;

-- Final tenant_write / tenant-write-v2 generation fence. The authoritative
-- erpx_meta.schema_manifest must still identify schema_generation 2.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM erpx_meta.schema_manifest
        WHERE singleton=TRUE AND schema_generation=2 AND manifest_version=1
          AND database_role='tenant_write' AND migration_set='tenant-write-v2'
    ) THEN
        RAISE EXCEPTION 'tenant-write generation-v2 manifest is not current';
    END IF;
END;
$$;
