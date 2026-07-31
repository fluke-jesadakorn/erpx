-- Durable checkpoints for rebuilding the disposable Marketing read model
-- from the tenant write outbox.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_projection_checkpoints (
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN (
        'marketing_profile', 'marketing_consent',
        'marketing_suppression', 'marketing_audience'
    )),
    aggregate_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE INDEX IF NOT EXISTS marketing_projection_checkpoints_scope_idx
    ON erpx_tenant.marketing_projection_checkpoints
       (holding_ref, aggregate_type, source_version, aggregate_id);

ALTER TABLE erpx_tenant.marketing_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_projection_checkpoints FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_marketing_projection_checkpoints
    ON erpx_tenant.marketing_projection_checkpoints;
CREATE POLICY tenant_scope_marketing_projection_checkpoints
    ON erpx_tenant.marketing_projection_checkpoints
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
