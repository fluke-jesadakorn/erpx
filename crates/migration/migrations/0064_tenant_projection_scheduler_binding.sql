-- Defense-in-depth for the durable tenant projection scheduler.
--
-- Scheduler rows are control-plane coordination state, but their tenant and
-- holding fields must remain inseparable even for recovery tooling or direct
-- SQL writers. The repository still performs active-state authorization and
-- locks the registry row; these composite references prevent a valid tenant
-- paired with a different holding, job, or claim batch from being persisted.

CREATE UNIQUE INDEX IF NOT EXISTS tenant_projection_jobs_id_scope_uq
    ON erpx_control.tenant_projection_jobs (id, tenant_id, holding_ref);

CREATE UNIQUE INDEX IF NOT EXISTS tenant_projection_claim_batches_binding_uq
    ON erpx_control.tenant_projection_claim_batches
       (claim_key, tenant_id, holding_ref, worker_id);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'erpx_control.tenant_projection_jobs'::regclass
          AND conname = 'tenant_projection_jobs_tenant_holding_fk'
    ) THEN
        ALTER TABLE erpx_control.tenant_projection_jobs
            ADD CONSTRAINT tenant_projection_jobs_tenant_holding_fk
            FOREIGN KEY (tenant_id, holding_ref)
            REFERENCES erpx_control.tenant_registry (tenant_id, holding_ref)
            ON DELETE RESTRICT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'erpx_control.tenant_projection_claim_batches'::regclass
          AND conname = 'tenant_projection_claim_batches_tenant_holding_fk'
    ) THEN
        ALTER TABLE erpx_control.tenant_projection_claim_batches
            ADD CONSTRAINT tenant_projection_claim_batches_tenant_holding_fk
            FOREIGN KEY (tenant_id, holding_ref)
            REFERENCES erpx_control.tenant_registry (tenant_id, holding_ref)
            ON DELETE RESTRICT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'erpx_control.tenant_projection_claims'::regclass
          AND conname = 'tenant_projection_claims_tenant_holding_fk'
    ) THEN
        ALTER TABLE erpx_control.tenant_projection_claims
            ADD CONSTRAINT tenant_projection_claims_tenant_holding_fk
            FOREIGN KEY (tenant_id, holding_ref)
            REFERENCES erpx_control.tenant_registry (tenant_id, holding_ref)
            ON DELETE RESTRICT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'erpx_control.tenant_projection_claims'::regclass
          AND conname = 'tenant_projection_claims_job_binding_fk'
    ) THEN
        ALTER TABLE erpx_control.tenant_projection_claims
            ADD CONSTRAINT tenant_projection_claims_job_binding_fk
            FOREIGN KEY (job_id, tenant_id, holding_ref)
            REFERENCES erpx_control.tenant_projection_jobs (id, tenant_id, holding_ref)
            ON DELETE RESTRICT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'erpx_control.tenant_projection_claims'::regclass
          AND conname = 'tenant_projection_claims_batch_binding_fk'
    ) THEN
        ALTER TABLE erpx_control.tenant_projection_claims
            ADD CONSTRAINT tenant_projection_claims_batch_binding_fk
            FOREIGN KEY (claim_key, tenant_id, holding_ref, worker_id)
            REFERENCES erpx_control.tenant_projection_claim_batches
                (claim_key, tenant_id, holding_ref, worker_id)
            ON DELETE RESTRICT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'erpx_control.tenant_projection_rebuilds'::regclass
          AND conname = 'tenant_projection_rebuilds_tenant_holding_fk'
    ) THEN
        ALTER TABLE erpx_control.tenant_projection_rebuilds
            ADD CONSTRAINT tenant_projection_rebuilds_tenant_holding_fk
            FOREIGN KEY (tenant_id, holding_ref)
            REFERENCES erpx_control.tenant_registry (tenant_id, holding_ref)
            ON DELETE RESTRICT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'erpx_control.tenant_projection_rebuilds'::regclass
          AND conname = 'tenant_projection_rebuilds_job_binding_fk'
    ) THEN
        ALTER TABLE erpx_control.tenant_projection_rebuilds
            ADD CONSTRAINT tenant_projection_rebuilds_job_binding_fk
            FOREIGN KEY (job_id, tenant_id, holding_ref)
            REFERENCES erpx_control.tenant_projection_jobs (id, tenant_id, holding_ref)
            ON DELETE RESTRICT;
    END IF;
END
$$;
