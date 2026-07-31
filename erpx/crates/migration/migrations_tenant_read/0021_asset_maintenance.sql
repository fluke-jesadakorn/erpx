-- Disposable Asset and Maintenance projections for one routed holding.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.asset_maintenance_read_rows (
    aggregate_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('asset', 'maintenance_work_order')),
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.asset_maintenance_projection_checkpoints (
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.asset_custody_transfer_read_rows (
    asset_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    transfer JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (asset_id, aggregate_version)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.asset_capacity_handoff_read_rows (
    source_event_id UUID PRIMARY KEY,
    source_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS asset_maintenance_read_scope_idx
    ON erpx_tenant.asset_maintenance_read_rows
        (holding_ref, company_id, branch_id, aggregate_type, reference);
CREATE INDEX IF NOT EXISTS asset_custody_read_scope_idx
    ON erpx_tenant.asset_custody_transfer_read_rows (holding_ref, asset_id, aggregate_version);
CREATE INDEX IF NOT EXISTS asset_capacity_handoff_read_scope_idx
    ON erpx_tenant.asset_capacity_handoff_read_rows
        (holding_ref, company_id, branch_id, projected_at DESC);

ALTER TABLE erpx_tenant.asset_maintenance_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.asset_maintenance_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.asset_maintenance_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.asset_maintenance_projection_checkpoints FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.asset_custody_transfer_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.asset_custody_transfer_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.asset_capacity_handoff_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.asset_capacity_handoff_read_rows FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_asset_maintenance_read_rows
    ON erpx_tenant.asset_maintenance_read_rows;
CREATE POLICY tenant_scope_asset_maintenance_read_rows
    ON erpx_tenant.asset_maintenance_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_asset_maintenance_checkpoints
    ON erpx_tenant.asset_maintenance_projection_checkpoints;
CREATE POLICY tenant_scope_asset_maintenance_checkpoints
    ON erpx_tenant.asset_maintenance_projection_checkpoints
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_asset_custody_read_rows
    ON erpx_tenant.asset_custody_transfer_read_rows;
CREATE POLICY tenant_scope_asset_custody_read_rows
    ON erpx_tenant.asset_custody_transfer_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_asset_capacity_handoff_read_rows
    ON erpx_tenant.asset_capacity_handoff_read_rows;
CREATE POLICY tenant_scope_asset_capacity_handoff_read_rows
    ON erpx_tenant.asset_capacity_handoff_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
