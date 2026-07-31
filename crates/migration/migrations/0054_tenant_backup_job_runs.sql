-- Forward-only durable attribution for each scheduled backup attempt.
-- Kept separate from migration 0052 so upgrades never rewrite an applied
-- migration checksum.
CREATE TABLE IF NOT EXISTS erpx_control.tenant_backup_job_runs (
    job_id UUID NOT NULL REFERENCES erpx_control.tenant_backup_jobs(id) ON DELETE RESTRICT,
    attempt INTEGER NOT NULL CHECK (attempt BETWEEN 1 AND 5),
    run_id UUID NOT NULL REFERENCES erpx_control.tenant_backup_runs(id) ON DELETE RESTRICT,
    linked_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (job_id, attempt),
    UNIQUE (run_id)
);

CREATE INDEX IF NOT EXISTS tenant_backup_job_runs_run_idx
    ON erpx_control.tenant_backup_job_runs (run_id);
