CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.projection_checkpoints (
    projection_name TEXT PRIMARY KEY,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    last_event_id UUID,
    source_version BIGINT NOT NULL DEFAULT 0 CHECK (source_version >= 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_tenant.customer_summary (
    customer_id UUID PRIMARY KEY,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE erpx_tenant.projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.projection_checkpoints FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.customer_summary ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.customer_summary FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_projection_checkpoints ON erpx_tenant.projection_checkpoints;
CREATE POLICY tenant_scope_projection_checkpoints ON erpx_tenant.projection_checkpoints
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_customer_summary ON erpx_tenant.customer_summary;
CREATE POLICY tenant_scope_customer_summary ON erpx_tenant.customer_summary
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
