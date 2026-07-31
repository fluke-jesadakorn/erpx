-- Rebuildable read projection vocabulary for durable physical PITR jobs.
ALTER TABLE erpx_control.tenant_backup_jobs
    DROP CONSTRAINT IF EXISTS tenant_backup_jobs_kind_check;

ALTER TABLE erpx_control.tenant_backup_jobs
    ADD CONSTRAINT tenant_backup_jobs_kind_check
        CHECK (kind IN ('full', 'pitr', 'restore_test'));
