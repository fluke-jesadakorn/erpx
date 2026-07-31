CREATE SCHEMA IF NOT EXISTS erpx_control;

-- Rebuildable scheduler status metadata. Detailed source and execution
-- material remains on the control-write side.
CREATE TABLE IF NOT EXISTS erpx_control.tenant_projection_status (
    tenant_id TEXT NOT NULL CHECK (length(btrim(tenant_id)) BETWEEN 1 AND 200),
    holding_ref TEXT NOT NULL CHECK (length(btrim(holding_ref)) BETWEEN 1 AND 200),
    projection_name TEXT NOT NULL CHECK (
        length(trim(projection_name)) BETWEEN 1 AND 120
        AND projection_name ~ '^[A-Za-z0-9_.-]+$'
    ),
    job_key TEXT NOT NULL CHECK (length(btrim(job_key)) BETWEEN 1 AND 200),
    job_id UUID PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('incremental', 'rebuild')),
    state TEXT NOT NULL CHECK (
        state IN (
            'pending', 'claimed', 'retryable', 'succeeded', 'dead_letter',
            'rebuild_pending', 'rebuild_succeeded'
        )
    ),
    available_at TIMESTAMPTZ NOT NULL,
    attempts INTEGER NOT NULL CHECK (attempts BETWEEN 0 AND 25),
    max_attempts INTEGER NOT NULL CHECK (max_attempts BETWEEN 1 AND 25),
    lease_until TIMESTAMPTZ,
    last_error TEXT CHECK (last_error IS NULL OR octet_length(last_error) <= 1000),
    dead_lettered_at TIMESTAMPTZ,
    rebuild_id UUID,
    version BIGINT NOT NULL CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    published_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT tenant_projection_status_kind_state_consistent CHECK (
        (kind = 'rebuild') OR state NOT IN ('rebuild_pending', 'rebuild_succeeded')
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS tenant_projection_status_order_idx
    ON erpx_control.tenant_projection_status
       (tenant_id, holding_ref, projection_name, job_key, job_id);

CREATE INDEX IF NOT EXISTS tenant_projection_status_updated_idx
    ON erpx_control.tenant_projection_status (updated_at DESC, job_id DESC);
