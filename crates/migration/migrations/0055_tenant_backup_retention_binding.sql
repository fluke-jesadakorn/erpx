-- Enforce the tenant/holding pair at the database boundary as well as in the
-- repository. The composite reference prevents a valid tenant paired with a
-- different holding from being persisted by an out-of-band writer.
CREATE UNIQUE INDEX IF NOT EXISTS tenant_registry_tenant_holding_uq
    ON erpx_control.tenant_registry (tenant_id, holding_ref);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'erpx_control.tenant_backup_retention_actions'::regclass
          AND conname = 'tenant_backup_retention_tenant_holding_fk'
    ) THEN
        ALTER TABLE erpx_control.tenant_backup_retention_actions
            ADD CONSTRAINT tenant_backup_retention_tenant_holding_fk
            FOREIGN KEY (tenant_id, holding_ref)
            REFERENCES erpx_control.tenant_registry (tenant_id, holding_ref)
            ON DELETE RESTRICT;
    END IF;
END
$$;
