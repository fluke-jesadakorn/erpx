-- Durable control-plane coordination for the Marketing delivery pass.
-- Delivery state remains authoritative in each tenant write database; these
-- rows only lease a bounded global worker pass and retain retry/DLQ history.

CREATE TABLE IF NOT EXISTS erpx_control.marketing_delivery_scheduler_jobs (
    id UUID PRIMARY KEY,
    job_key TEXT NOT NULL UNIQUE CHECK (
        length(trim(job_key)) BETWEEN 1 AND 200
        AND job_key ~ '^[A-Za-z0-9_.:-]+$'
    ),
    state TEXT NOT NULL CHECK (
        state IN ('pending', 'claimed', 'retryable', 'dead_letter')
    ),
    available_at TIMESTAMPTZ NOT NULL,
    cadence_seconds BIGINT NOT NULL CHECK (cadence_seconds BETWEEN 1 AND 86400),
    max_attempts INTEGER NOT NULL CHECK (max_attempts BETWEEN 1 AND 25),
    backoff_base_seconds BIGINT NOT NULL CHECK (backoff_base_seconds BETWEEN 1 AND 86400),
    backoff_max_seconds BIGINT NOT NULL CHECK (
        backoff_max_seconds BETWEEN 1 AND 86400
        AND backoff_max_seconds >= backoff_base_seconds
    ),
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 25),
    current_claim_id UUID,
    worker_id UUID,
    lease_until TIMESTAMPTZ,
    last_error TEXT CHECK (last_error IS NULL OR octet_length(last_error) <= 1000),
    dead_lettered_at TIMESTAMPTZ,
    last_succeeded_at TIMESTAMPTZ,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT marketing_delivery_scheduler_claim_fields_consistent CHECK (
        (state = 'claimed') = (
            current_claim_id IS NOT NULL AND worker_id IS NOT NULL AND lease_until IS NOT NULL
        )
    )
);

CREATE INDEX IF NOT EXISTS marketing_delivery_scheduler_due_idx
    ON erpx_control.marketing_delivery_scheduler_jobs (available_at, id)
    WHERE state IN ('pending', 'retryable');

CREATE INDEX IF NOT EXISTS marketing_delivery_scheduler_stale_idx
    ON erpx_control.marketing_delivery_scheduler_jobs (lease_until, id)
    WHERE state = 'claimed';

CREATE TABLE IF NOT EXISTS erpx_control.marketing_delivery_scheduler_claims (
    claim_id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES erpx_control.marketing_delivery_scheduler_jobs(id),
    worker_id UUID NOT NULL,
    claimed_at TIMESTAMPTZ NOT NULL,
    lease_until TIMESTAMPTZ NOT NULL CHECK (lease_until > claimed_at),
    state TEXT NOT NULL CHECK (state IN ('active', 'completed', 'stale', 'failed', 'dead_letter')),
    completed_at TIMESTAMPTZ,
    outcome TEXT,
    UNIQUE (job_id, claim_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS marketing_delivery_scheduler_active_claim_idx
    ON erpx_control.marketing_delivery_scheduler_claims (job_id)
    WHERE state = 'active';

CREATE INDEX IF NOT EXISTS marketing_delivery_scheduler_claim_worker_idx
    ON erpx_control.marketing_delivery_scheduler_claims (worker_id, state, lease_until);
