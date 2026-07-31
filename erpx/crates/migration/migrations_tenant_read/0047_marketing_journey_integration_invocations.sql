-- Rebuildable read model for profile-redacted Marketing journey integration
-- handoffs. The write-side invocation and outbox remain authoritative.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_journey_integration_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    invocation_id UUID NOT NULL,
    integration_ref TEXT NOT NULL CHECK (btrim(integration_ref) <> ''),
    journey_id UUID NOT NULL,
    run_id UUID NOT NULL,
    node_id UUID NOT NULL,
    profile_id UUID NOT NULL,
    scope JSONB NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    state TEXT NOT NULL CHECK (state IN ('queued', 'sending', 'succeeded', 'retry_scheduled', 'failed')),
    attempt_count INTEGER NOT NULL CHECK (attempt_count >= 0),
    version BIGINT NOT NULL CHECK (version > 0),
    provider_reference TEXT,
    last_error TEXT,
    next_attempt_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_ref, invocation_id)
);

CREATE INDEX IF NOT EXISTS marketing_journey_integration_read_scope_idx
    ON erpx_tenant.marketing_journey_integration_read_rows
       (holding_ref, updated_at DESC, invocation_id);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_journey_integration_projection_checkpoints (
    holding_ref TEXT PRIMARY KEY CHECK (btrim(holding_ref) <> ''),
    last_occurred_at TIMESTAMPTZ NOT NULL,
    last_event_id UUID NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE erpx_tenant.marketing_journey_integration_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_journey_integration_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_journey_integration_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_journey_integration_projection_checkpoints FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_marketing_journey_integration_read_rows
    ON erpx_tenant.marketing_journey_integration_read_rows;
CREATE POLICY tenant_scope_marketing_journey_integration_read_rows
    ON erpx_tenant.marketing_journey_integration_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_marketing_journey_integration_projection_checkpoints
    ON erpx_tenant.marketing_journey_integration_projection_checkpoints;
CREATE POLICY tenant_scope_marketing_journey_integration_projection_checkpoints
    ON erpx_tenant.marketing_journey_integration_projection_checkpoints
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
