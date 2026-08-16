CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.outbox_events (
    event_id UUID PRIMARY KEY,
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    published_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS erpx_tenant.command_idempotency (
    idempotency_key TEXT PRIMARY KEY,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    request_hash TEXT NOT NULL,
    response JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE erpx_tenant.outbox_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.outbox_events FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_outbox_events ON erpx_tenant.outbox_events;
CREATE POLICY tenant_scope_outbox_events ON erpx_tenant.outbox_events
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_command_idempotency ON erpx_tenant.command_idempotency;
CREATE POLICY tenant_scope_command_idempotency ON erpx_tenant.command_idempotency
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
