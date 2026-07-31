-- Rebuildable control-read status for Finance bank-provider import jobs.
-- It contains no request, authorization, credential, or provider payload.
CREATE SCHEMA IF NOT EXISTS erpx_control;

CREATE TABLE IF NOT EXISTS erpx_control.finance_bank_provider_import_job_status (
    job_id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    holding_ref TEXT NOT NULL,
    import_id UUID NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending', 'claimed', 'retryable', 'completed', 'dead_letter')),
    source_fence BIGINT NOT NULL CHECK (source_fence > 0),
    attempts SMALLINT NOT NULL CHECK (attempts BETWEEN 0 AND 8),
    max_attempts SMALLINT NOT NULL CHECK (max_attempts BETWEEN 1 AND 8),
    available_at TIMESTAMPTZ NOT NULL,
    lease_until TIMESTAMPTZ,
    last_failure_class TEXT,
    completed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS finance_bank_provider_import_status_scope_idx
    ON erpx_control.finance_bank_provider_import_job_status (tenant_id, holding_ref, updated_at DESC, job_id);
