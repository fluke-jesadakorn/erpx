-- Operations read models are rebuildable and live only in the tenant read
-- database. They do not contain credentials or control-plane metadata.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.operations_projection_checkpoints (
    projection_name TEXT NOT NULL CHECK (btrim(projection_name) <> ''),
    aggregate_type TEXT NOT NULL CHECK (btrim(aggregate_type) <> ''),
    aggregate_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    last_event_id UUID,
    last_aggregate_version BIGINT NOT NULL CHECK (last_aggregate_version >= 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.operations_read_rows (
    projection_name TEXT NOT NULL CHECK (btrim(projection_name) <> ''),
    aggregate_type TEXT NOT NULL CHECK (btrim(aggregate_type) <> ''),
    aggregate_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    lifecycle_status TEXT NOT NULL CHECK (btrim(lifecycle_status) <> ''),
    state JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, aggregate_type, aggregate_id)
);

CREATE INDEX IF NOT EXISTS operations_projection_checkpoints_holding_idx
    ON erpx_tenant.operations_projection_checkpoints
    (holding_ref, projection_name, projected_at);
CREATE INDEX IF NOT EXISTS operations_read_rows_holding_idx
    ON erpx_tenant.operations_read_rows
    (holding_ref, projection_name, updated_at);

ALTER TABLE erpx_tenant.operations_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.operations_projection_checkpoints FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.operations_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.operations_read_rows FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_operations_projection_checkpoints
    ON erpx_tenant.operations_projection_checkpoints;
CREATE POLICY tenant_scope_operations_projection_checkpoints
    ON erpx_tenant.operations_projection_checkpoints
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_operations_read_rows ON erpx_tenant.operations_read_rows;
CREATE POLICY tenant_scope_operations_read_rows
    ON erpx_tenant.operations_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
