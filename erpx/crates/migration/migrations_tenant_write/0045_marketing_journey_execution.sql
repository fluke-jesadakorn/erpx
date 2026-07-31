-- Journey execution cursors are tenant-write state. The initial cursor is a
-- durable seed at version zero; the first execution fact advances it to the
-- positive version required by the shared outbox contract.
CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_journey_execution_cursors (
    holding_ref TEXT NOT NULL
        CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    run_id UUID NOT NULL,
    journey_id UUID NOT NULL,
    profile_id UUID NOT NULL,
    scope JSONB NOT NULL,
    state TEXT NOT NULL
        CHECK (state IN ('ready', 'running', 'waiting', 'paused', 'completed')),
    next_node_id UUID,
    waiting_until TIMESTAMPTZ,
    version BIGINT NOT NULL CHECK (version >= 0),
    cursor JSONB NOT NULL,
    current_claim_id UUID,
    claimed_by UUID,
    claim_until TIMESTAMPTZ,
    next_attempt_at TIMESTAMPTZ NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    last_error TEXT,
    dead_lettered_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_ref, run_id)
);

CREATE INDEX IF NOT EXISTS marketing_journey_execution_due_idx
    ON erpx_tenant.marketing_journey_execution_cursors
        (holding_ref, state, next_attempt_at, waiting_until, claim_until, run_id);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_journey_execution_idempotency (
    holding_ref TEXT NOT NULL
        CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    idempotency_key TEXT NOT NULL
        CHECK (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 255),
    run_id UUID NOT NULL,
    request_hash TEXT NOT NULL
        CHECK (btrim(request_hash) <> '' AND octet_length(request_hash) <= 128),
    version BIGINT NOT NULL CHECK (version > 0),
    event_id UUID NOT NULL,
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, idempotency_key)
);

CREATE INDEX IF NOT EXISTS marketing_journey_execution_idempotency_run_idx
    ON erpx_tenant.marketing_journey_execution_idempotency
        (holding_ref, run_id, created_at);

ALTER TABLE erpx_tenant.marketing_journey_execution_cursors ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_journey_execution_cursors FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_journey_execution_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_journey_execution_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_marketing_journey_execution_cursors
    ON erpx_tenant.marketing_journey_execution_cursors;
CREATE POLICY tenant_scope_marketing_journey_execution_cursors
    ON erpx_tenant.marketing_journey_execution_cursors
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_marketing_journey_execution_idempotency
    ON erpx_tenant.marketing_journey_execution_idempotency;
CREATE POLICY tenant_scope_marketing_journey_execution_idempotency
    ON erpx_tenant.marketing_journey_execution_idempotency
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
