-- Rebuildable, secret-free read projection for backup retention actions.
-- No backup, restore, storage, credential, or error text is projected.
CREATE TABLE IF NOT EXISTS erpx_control.tenant_backup_retention_actions (
    id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    holding_ref TEXT NOT NULL,
    cutoff_at TIMESTAMPTZ NOT NULL,
    max_rows INTEGER NOT NULL CHECK (max_rows BETWEEN 1 AND 1000),
    dry_run BOOLEAN NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending', 'claimed', 'completed', 'failed')),
    attempts INTEGER NOT NULL CHECK (attempts BETWEEN 0 AND 3),
    max_attempts INTEGER NOT NULL CHECK (max_attempts BETWEEN 1 AND 3),
    jobs_selected INTEGER NOT NULL CHECK (jobs_selected >= 0),
    jobs_deleted INTEGER NOT NULL CHECK (jobs_deleted >= 0),
    runs_deleted INTEGER NOT NULL CHECK (runs_deleted >= 0),
    object_refs_released INTEGER NOT NULL CHECK (object_refs_released >= 0),
    remaining_eligible INTEGER NOT NULL CHECK (remaining_eligible >= 0),
    error_present BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS tenant_backup_retention_actions_read_history_idx
    ON erpx_control.tenant_backup_retention_actions
        (tenant_id, holding_ref, created_at DESC, id DESC);
