-- Rebuildable, secret-free control read projection for tenant backup history.
-- The write-side job table remains authoritative; workers refresh this table.
CREATE TABLE IF NOT EXISTS erpx_control.tenant_backup_jobs (
    id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('full', 'restore_test')),
    state TEXT NOT NULL CHECK (
        state IN ('pending', 'claimed', 'retryable', 'completed', 'failed', 'dead_letter')
    ),
    available_at TIMESTAMPTZ NOT NULL,
    attempts INTEGER NOT NULL CHECK (attempts >= 0),
    max_attempts INTEGER NOT NULL CHECK (max_attempts BETWEEN 1 AND 5),
    verified BOOLEAN NOT NULL,
    error_present BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS tenant_backup_jobs_read_history_idx
    ON erpx_control.tenant_backup_jobs (tenant_id, created_at DESC, id DESC);
