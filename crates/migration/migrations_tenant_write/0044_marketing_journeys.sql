-- Marketing journeys are authoritative tenant-write aggregates. The complete
-- domain value is retained for lossless reconstruction while bounded columns
-- support tenant-scoped locking, optimistic concurrency, and operations.
CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_journeys (
    holding_ref TEXT NOT NULL
        CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    journey_id UUID NOT NULL,
    scope JSONB NOT NULL,
    name TEXT NOT NULL
        CHECK (btrim(name) <> '' AND octet_length(name) <= 255),
    status TEXT NOT NULL
        CHECK (status IN ('draft', 'review', 'approved', 'scheduled',
                          'running', 'paused', 'completed', 'archived')),
    nodes JSONB NOT NULL,
    journey JSONB NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    last_command_id UUID NOT NULL,
    last_idempotency_key TEXT NOT NULL
        CHECK (btrim(last_idempotency_key) <> ''
               AND octet_length(last_idempotency_key) <= 255),
    actor JSONB NOT NULL,
    correlation_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_ref, journey_id)
);

CREATE INDEX IF NOT EXISTS marketing_journeys_status_idx
    ON erpx_tenant.marketing_journeys (holding_ref, status, updated_at, journey_id);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_journey_command_idempotency (
    holding_ref TEXT NOT NULL
        CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    idempotency_key TEXT NOT NULL
        CHECK (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 255),
    operation TEXT NOT NULL CHECK (operation IN ('create', 'transition')),
    request_hash TEXT NOT NULL
        CHECK (btrim(request_hash) <> '' AND octet_length(request_hash) <= 128),
    journey_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    event_id UUID NOT NULL,
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, idempotency_key)
);

CREATE INDEX IF NOT EXISTS marketing_journey_idempotency_journey_idx
    ON erpx_tenant.marketing_journey_command_idempotency
        (holding_ref, journey_id, created_at);

ALTER TABLE erpx_tenant.marketing_journeys ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_journeys FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_journey_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_journey_command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_marketing_journeys
    ON erpx_tenant.marketing_journeys;
CREATE POLICY tenant_scope_marketing_journeys
    ON erpx_tenant.marketing_journeys
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_marketing_journey_idempotency
    ON erpx_tenant.marketing_journey_command_idempotency;
CREATE POLICY tenant_scope_marketing_journey_idempotency
    ON erpx_tenant.marketing_journey_command_idempotency
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
