-- Final generation-two role marker after the E-commerce channel projection.
UPDATE erpx_meta.schema_manifest
SET installed_at = now()
WHERE singleton AND schema_generation = 2
  AND database_role = 'tenant_read' AND migration_set = 'tenant-read-v2';
