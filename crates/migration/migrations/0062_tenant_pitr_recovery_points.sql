-- Durable, secret-free physical PITR admission metadata.
--
-- The recovery point contains only validated timestamps/LSNs and opaque
-- references. WAL bytes and physical clusters remain outside the control
-- database and are handled by the credentialed infrastructure executor.
ALTER TABLE erpx_control.tenant_backup_jobs
    ADD COLUMN IF NOT EXISTS pitr_recovery_point JSONB;

ALTER TABLE erpx_control.tenant_backup_jobs
    DROP CONSTRAINT IF EXISTS tenant_backup_jobs_kind_check,
    DROP CONSTRAINT IF EXISTS tenant_backup_jobs_check;

ALTER TABLE erpx_control.tenant_backup_jobs
    ADD CONSTRAINT tenant_backup_jobs_kind_shape_check CHECK (
        (kind = 'full'
            AND restore_target_ref IS NULL
            AND candidate_read_database_key IS NULL
            AND source_version IS NULL
            AND pitr_recovery_point IS NULL)
        OR (kind = 'restore_test'
            AND restore_target_ref IS NOT NULL
            AND candidate_read_database_key IS NOT NULL
            AND source_version > 0
            AND pitr_recovery_point IS NULL)
        OR (kind = 'pitr'
            AND restore_target_ref IS NULL
            AND candidate_read_database_key IS NULL
            AND source_version IS NULL
            AND jsonb_typeof(pitr_recovery_point) = 'object')
    );

ALTER TABLE erpx_control.tenant_backup_runs
    ADD COLUMN IF NOT EXISTS pitr_recovery_point JSONB,
    ADD CONSTRAINT tenant_backup_runs_pitr_recovery_point_shape_check CHECK (
        (kind = 'pitr' AND jsonb_typeof(pitr_recovery_point) = 'object')
        OR (kind <> 'pitr' AND pitr_recovery_point IS NULL)
    );
