-- Durable control-plane coordination for tenant backup and restore-test work.
-- Aggregate/database artifacts remain outside this table; only bounded opaque
-- references and worker state are persisted here.
CREATE TABLE IF NOT EXISTS erpx_control.tenant_backup_jobs (
    id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES erpx_control.tenant_registry(tenant_id) ON DELETE RESTRICT,
    holding_ref TEXT NOT NULL CHECK (length(trim(holding_ref)) BETWEEN 1 AND 200),
    kind TEXT NOT NULL CHECK (kind IN ('full', 'restore_test')),
    backup_ref TEXT NOT NULL CHECK (length(trim(backup_ref)) BETWEEN 1 AND 200),
    restore_target_ref TEXT,
    candidate_read_database_key TEXT,
    source_version BIGINT CHECK (source_version IS NULL OR source_version > 0),
    source_projection TEXT NOT NULL DEFAULT 'operations' CHECK (
        source_projection IN ('operations', 'finance_banking', 'finance_subledger', 'finance_close')
    ),
    state TEXT NOT NULL CHECK (
        state IN ('pending', 'claimed', 'retryable', 'completed', 'failed', 'dead_letter')
    ),
    available_at TIMESTAMPTZ NOT NULL,
    max_attempts INTEGER NOT NULL CHECK (max_attempts BETWEEN 1 AND 5),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 5),
    worker_id UUID,
    claim_id UUID,
    lease_until TIMESTAMPTZ,
    last_error TEXT CHECK (last_error IS NULL OR octet_length(last_error) <= 1000),
    verified BOOLEAN NOT NULL DEFAULT FALSE,
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    idempotency_key TEXT NOT NULL CHECK (length(trim(idempotency_key)) BETWEEN 1 AND 200),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((kind = 'full' AND restore_target_ref IS NULL AND candidate_read_database_key IS NULL AND source_version IS NULL)
        OR (kind = 'restore_test' AND restore_target_ref IS NOT NULL AND candidate_read_database_key IS NOT NULL AND source_version > 0)),
    CHECK ((state = 'claimed') = (claim_id IS NOT NULL AND worker_id IS NOT NULL AND lease_until IS NOT NULL)),
    UNIQUE (tenant_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS tenant_backup_jobs_due_idx
    ON erpx_control.tenant_backup_jobs (tenant_id, holding_ref, available_at, id)
    WHERE state IN ('pending', 'retryable');

CREATE INDEX IF NOT EXISTS tenant_backup_jobs_history_idx
    ON erpx_control.tenant_backup_jobs (tenant_id, created_at DESC, id DESC);
