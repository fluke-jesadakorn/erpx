-- Durable control-plane scheduling for bounded, tenant-routed projection work.
-- These rows coordinate workers only; aggregate and read-model state remains in
-- the tenant write/read databases.

CREATE TABLE IF NOT EXISTS erpx_control.tenant_projection_jobs (
    id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL CHECK (length(trim(tenant_id)) BETWEEN 1 AND 200),
    holding_ref TEXT NOT NULL CHECK (length(trim(holding_ref)) BETWEEN 1 AND 200),
    projection_name TEXT NOT NULL CHECK (
        length(trim(projection_name)) BETWEEN 1 AND 120
        AND projection_name ~ '^[A-Za-z0-9_.-]+$'
    ),
    job_key TEXT NOT NULL CHECK (length(trim(job_key)) BETWEEN 1 AND 200),
    kind TEXT NOT NULL CHECK (kind IN ('incremental', 'rebuild')),
    state TEXT NOT NULL CHECK (
        state IN (
            'pending', 'claimed', 'retryable', 'succeeded', 'dead_letter',
            'rebuild_pending', 'rebuild_succeeded'
        )
    ),
    available_at TIMESTAMPTZ NOT NULL,
    page_size INTEGER NOT NULL CHECK (page_size BETWEEN 1 AND 500),
    batch_size INTEGER NOT NULL CHECK (batch_size BETWEEN 1 AND 100),
    max_attempts INTEGER NOT NULL CHECK (max_attempts BETWEEN 1 AND 25),
    backoff_base_seconds BIGINT NOT NULL CHECK (backoff_base_seconds BETWEEN 1 AND 86400),
    backoff_max_seconds BIGINT NOT NULL CHECK (
        backoff_max_seconds BETWEEN 1 AND 86400
        AND backoff_max_seconds >= backoff_base_seconds
    ),
    payload JSONB NOT NULL DEFAULT '{}'::jsonb
        CHECK (octet_length(payload::text) <= 65536),
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 25),
    current_claim_id UUID,
    worker_id UUID,
    lease_until TIMESTAMPTZ,
    last_error TEXT CHECK (last_error IS NULL OR octet_length(last_error) <= 1000),
    dead_lettered_at TIMESTAMPTZ,
    rebuild_id UUID,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, holding_ref, projection_name, job_key),
    CONSTRAINT tenant_projection_claim_fields_consistent CHECK (
        (state = 'claimed') = (
            current_claim_id IS NOT NULL AND worker_id IS NOT NULL AND lease_until IS NOT NULL
        )
    ),
    CONSTRAINT tenant_projection_rebuild_kind_consistent CHECK (
        (kind = 'rebuild') OR state NOT IN ('rebuild_pending', 'rebuild_succeeded')
    )
);

CREATE INDEX IF NOT EXISTS tenant_projection_jobs_due_idx
    ON erpx_control.tenant_projection_jobs (tenant_id, holding_ref, available_at, id)
    WHERE state IN ('pending', 'retryable', 'rebuild_pending');

CREATE INDEX IF NOT EXISTS tenant_projection_jobs_stale_idx
    ON erpx_control.tenant_projection_jobs (tenant_id, holding_ref, lease_until, id)
    WHERE state = 'claimed';

CREATE TABLE IF NOT EXISTS erpx_control.tenant_projection_claim_batches (
    claim_key TEXT PRIMARY KEY CHECK (length(trim(claim_key)) BETWEEN 1 AND 200),
    tenant_id TEXT NOT NULL CHECK (length(trim(tenant_id)) BETWEEN 1 AND 200),
    holding_ref TEXT NOT NULL CHECK (length(trim(holding_ref)) BETWEEN 1 AND 200),
    worker_id UUID NOT NULL,
    sealed BOOLEAN NOT NULL DEFAULT false,
    result_count INTEGER NOT NULL DEFAULT 0 CHECK (result_count BETWEEN 0 AND 100),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS tenant_projection_claim_batches_scope_idx
    ON erpx_control.tenant_projection_claim_batches (tenant_id, holding_ref, created_at DESC);

CREATE TABLE IF NOT EXISTS erpx_control.tenant_projection_claims (
    claim_id UUID PRIMARY KEY,
    claim_key TEXT NOT NULL REFERENCES erpx_control.tenant_projection_claim_batches (claim_key),
    job_id UUID NOT NULL REFERENCES erpx_control.tenant_projection_jobs (id),
    tenant_id TEXT NOT NULL CHECK (length(trim(tenant_id)) BETWEEN 1 AND 200),
    holding_ref TEXT NOT NULL CHECK (length(trim(holding_ref)) BETWEEN 1 AND 200),
    worker_id UUID NOT NULL,
    claimed_at TIMESTAMPTZ NOT NULL,
    lease_until TIMESTAMPTZ NOT NULL CHECK (lease_until > claimed_at),
    state TEXT NOT NULL CHECK (state IN ('active', 'completed', 'stale', 'failed', 'dead_letter')),
    completed_at TIMESTAMPTZ,
    outcome TEXT,
    UNIQUE (claim_key, job_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS tenant_projection_active_job_claim_idx
    ON erpx_control.tenant_projection_claims (job_id)
    WHERE state = 'active';

CREATE INDEX IF NOT EXISTS tenant_projection_claims_worker_idx
    ON erpx_control.tenant_projection_claims (tenant_id, holding_ref, worker_id, state, lease_until);

CREATE TABLE IF NOT EXISTS erpx_control.tenant_projection_rebuilds (
    id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL CHECK (length(trim(tenant_id)) BETWEEN 1 AND 200),
    holding_ref TEXT NOT NULL CHECK (length(trim(holding_ref)) BETWEEN 1 AND 200),
    projection_name TEXT NOT NULL CHECK (
        length(trim(projection_name)) BETWEEN 1 AND 120
        AND projection_name ~ '^[A-Za-z0-9_.-]+$'
    ),
    rebuild_key TEXT NOT NULL CHECK (length(trim(rebuild_key)) BETWEEN 1 AND 200),
    job_id UUID NOT NULL REFERENCES erpx_control.tenant_projection_jobs (id),
    state TEXT NOT NULL CHECK (state IN ('pending', 'running', 'succeeded', 'dead_letter')),
    payload JSONB NOT NULL DEFAULT '{}'::jsonb
        CHECK (octet_length(payload::text) <= 65536),
    requested_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, holding_ref, projection_name, rebuild_key)
);

CREATE INDEX IF NOT EXISTS tenant_projection_rebuilds_scope_idx
    ON erpx_control.tenant_projection_rebuilds (tenant_id, holding_ref, state, updated_at DESC);
