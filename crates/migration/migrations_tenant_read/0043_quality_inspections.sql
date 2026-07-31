-- Disposable Quality read model and projection checkpoints.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.quality_inspection_read_rows (
    inspection_id UUID PRIMARY KEY,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_tenant.quality_projection_checkpoints (
    inspection_id UUID PRIMARY KEY,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS quality_inspection_read_scope_idx
    ON erpx_tenant.quality_inspection_read_rows
       (holding_ref, company_id, branch_id, reference, inspection_id);

ALTER TABLE erpx_tenant.quality_inspection_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.quality_inspection_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.quality_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.quality_projection_checkpoints FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_quality_read_rows
    ON erpx_tenant.quality_inspection_read_rows;
CREATE POLICY tenant_scope_quality_read_rows
    ON erpx_tenant.quality_inspection_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_quality_checkpoints
    ON erpx_tenant.quality_projection_checkpoints;
CREATE POLICY tenant_scope_quality_checkpoints
    ON erpx_tenant.quality_projection_checkpoints
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
