-- Durable control-plane scheduling for authorized Finance bank-provider imports.
-- Request/authorization JSON contains only validated opaque references and
-- authorization facts; credentials and provider payloads remain out of this
-- database. Tenant facts stay in the routed tenant write database.
CREATE TABLE IF NOT EXISTS erpx_control.finance_bank_provider_import_jobs (
    job_id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES erpx_control.tenant_registry(tenant_id) ON DELETE RESTRICT,
    holding_ref TEXT NOT NULL CHECK (length(trim(holding_ref)) BETWEEN 1 AND 200),
    import_id UUID NOT NULL,
    provider_account_ref TEXT NOT NULL CHECK (length(trim(provider_account_ref)) BETWEEN 1 AND 255),
    provider_key TEXT NOT NULL CHECK (length(trim(provider_key)) BETWEEN 1 AND 64),
    credential_key_ref TEXT NOT NULL CHECK (length(trim(credential_key_ref)) BETWEEN 1 AND 255),
    source_fence BIGINT NOT NULL CHECK (source_fence > 0),
    request JSONB NOT NULL,
    authorization_facts JSONB NOT NULL,
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    state TEXT NOT NULL CHECK (state IN ('pending', 'claimed', 'retryable', 'completed', 'dead_letter')),
    available_at TIMESTAMPTZ NOT NULL,
    max_attempts SMALLINT NOT NULL CHECK (max_attempts BETWEEN 1 AND 8),
    attempts SMALLINT NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 8),
    current_claim_id UUID,
    worker_id UUID,
    lease_until TIMESTAMPTZ,
    last_failure_class TEXT,
    dead_lettered_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((state = 'claimed') = (
        current_claim_id IS NOT NULL AND worker_id IS NOT NULL AND lease_until IS NOT NULL
    )),
    CHECK (last_failure_class IS NULL OR last_failure_class IN (
        'retryable', 'permanent', 'authorization', 'claim_lost', 'source_fence', 'reconciliation'
    )),
    UNIQUE (tenant_id, holding_ref, import_id)
);

CREATE INDEX IF NOT EXISTS finance_bank_provider_import_jobs_due_idx
    ON erpx_control.finance_bank_provider_import_jobs (available_at, job_id)
    WHERE state IN ('pending', 'retryable');

CREATE INDEX IF NOT EXISTS finance_bank_provider_import_jobs_claim_idx
    ON erpx_control.finance_bank_provider_import_jobs (lease_until, job_id)
    WHERE state = 'claimed';

CREATE TABLE IF NOT EXISTS erpx_control.finance_bank_provider_import_job_claims (
    claim_id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES erpx_control.finance_bank_provider_import_jobs(job_id) ON DELETE RESTRICT,
    worker_id UUID NOT NULL,
    source_fence BIGINT NOT NULL CHECK (source_fence > 0),
    claimed_at TIMESTAMPTZ NOT NULL,
    lease_until TIMESTAMPTZ NOT NULL CHECK (lease_until > claimed_at),
    state TEXT NOT NULL CHECK (state IN ('active', 'completed', 'stale', 'failed', 'dead_letter')),
    completed_at TIMESTAMPTZ,
    outcome JSONB,
    UNIQUE (job_id, claim_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS finance_bank_provider_import_active_claim_idx
    ON erpx_control.finance_bank_provider_import_job_claims (job_id)
    WHERE state = 'active';

CREATE INDEX IF NOT EXISTS finance_bank_provider_import_claim_worker_idx
    ON erpx_control.finance_bank_provider_import_job_claims (worker_id, state, lease_until);

CREATE TABLE IF NOT EXISTS erpx_control.finance_bank_provider_import_evidence (
    evidence_id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES erpx_control.finance_bank_provider_import_jobs(job_id) ON DELETE RESTRICT,
    claim_id UUID NOT NULL REFERENCES erpx_control.finance_bank_provider_import_job_claims(claim_id) ON DELETE RESTRICT,
    evidence_kind TEXT NOT NULL CHECK (evidence_kind IN ('reconciliation', 'rollback')),
    payload JSONB NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS finance_bank_provider_import_evidence_job_idx
    ON erpx_control.finance_bank_provider_import_evidence (job_id, recorded_at, evidence_id);
