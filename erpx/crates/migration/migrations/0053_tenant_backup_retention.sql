-- Durable, tenant-bound backup-history retention actions.
-- Object references remain write-side data only; this table stores counts and
-- redacted state so a worker can replay the same action safely.
CREATE TABLE IF NOT EXISTS erpx_control.tenant_backup_retention_actions (
    id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES erpx_control.tenant_registry(tenant_id) ON DELETE RESTRICT,
    holding_ref TEXT NOT NULL CHECK (length(trim(holding_ref)) BETWEEN 1 AND 200),
    cutoff_at TIMESTAMPTZ NOT NULL,
    max_rows INTEGER NOT NULL CHECK (max_rows BETWEEN 1 AND 1000),
    dry_run BOOLEAN NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending', 'claimed', 'completed', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 3),
    max_attempts INTEGER NOT NULL CHECK (max_attempts BETWEEN 1 AND 3),
    worker_id UUID,
    claim_id UUID,
    lease_until TIMESTAMPTZ,
    jobs_selected INTEGER NOT NULL DEFAULT 0 CHECK (jobs_selected >= 0),
    jobs_deleted INTEGER NOT NULL DEFAULT 0 CHECK (jobs_deleted >= 0),
    runs_deleted INTEGER NOT NULL DEFAULT 0 CHECK (runs_deleted >= 0),
    object_refs_released INTEGER NOT NULL DEFAULT 0 CHECK (object_refs_released >= 0),
    remaining_eligible INTEGER NOT NULL DEFAULT 0 CHECK (remaining_eligible >= 0),
    error_present BOOLEAN NOT NULL DEFAULT FALSE,
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    idempotency_key TEXT NOT NULL CHECK (length(trim(idempotency_key)) BETWEEN 1 AND 200),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    CHECK ((state = 'claimed') = (claim_id IS NOT NULL AND worker_id IS NOT NULL AND lease_until IS NOT NULL)),
    CHECK (jobs_deleted <= jobs_selected),
    UNIQUE (tenant_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS tenant_backup_retention_actions_history_idx
    ON erpx_control.tenant_backup_retention_actions (tenant_id, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS tenant_backup_retention_actions_claim_idx
    ON erpx_control.tenant_backup_retention_actions (state, lease_until, updated_at);
